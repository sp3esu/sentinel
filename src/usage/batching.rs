//! Batching Usage Tracker
//!
//! Fire-and-forget usage tracking that batches increments to protect Zion API.
//! Uses MPSC channels for async queueing, governor for rate limiting, and
//! failsafe for circuit breaking.
//!
//! Features:
//! - Non-blocking fire-and-forget tracking
//! - Aggregates increments by user before sending
//! - Uses batch-increment API for efficiency (up to 1000 items per request)
//! - Rate limits Zion API calls (default: 20 req/s)
//! - Circuit breaker for graceful degradation
//! - Redis persistence for failed increments with retry

use std::collections::HashMap;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;

use governor::{Quota, RateLimiter};
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::zion::{BatchIncrementItem, ZionClient};

/// Redis key prefix for failed usage increments
const REDIS_FAILED_INCREMENTS_KEY: &str = "sentinel:usage:failed";

/// Configuration for the batching usage tracker
#[derive(Debug, Clone)]
pub struct BatchingConfig {
    /// Maximum number of increments to batch before flushing
    pub max_batch_size: usize,
    /// Maximum time to wait before flushing a batch
    pub flush_interval: Duration,
    /// Channel buffer size for handling traffic spikes
    pub channel_buffer: usize,
    /// Maximum requests per second to Zion
    pub rate_limit_per_second: u32,
    /// Number of consecutive failures before circuit opens
    pub circuit_breaker_threshold: u32,
    /// Time to wait before attempting to close circuit
    pub circuit_breaker_reset: Duration,
    /// Time to wait before retrying failed increments from Redis
    pub retry_interval: Duration,
    /// Maximum number of failed increments to retry per cycle
    pub max_retry_batch: usize,
}

impl Default for BatchingConfig {
    fn default() -> Self {
        Self {
            max_batch_size: 100,
            flush_interval: Duration::from_millis(500),
            channel_buffer: 10_000,
            rate_limit_per_second: 20,
            circuit_breaker_threshold: 3,
            circuit_breaker_reset: Duration::from_secs(30),
            retry_interval: Duration::from_secs(60),
            max_retry_batch: 50,
        }
    }
}

/// A single usage increment to be batched (unified format)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct UsageIncrement {
    external_id: String,
    input_tokens: i64,
    output_tokens: i64,
    requests: i64,
}

/// Aggregated usage for a user
#[derive(Debug, Clone, Default)]
struct AggregatedUsage {
    input_tokens: i64,
    output_tokens: i64,
    requests: i64,
}

impl AggregatedUsage {
    fn add(&mut self, other: &UsageIncrement) {
        self.input_tokens += other.input_tokens;
        self.output_tokens += other.output_tokens;
        self.requests += other.requests;
    }

    fn is_empty(&self) -> bool {
        self.input_tokens == 0 && self.output_tokens == 0 && self.requests == 0
    }
}

/// Circuit breaker state
#[derive(Debug, Clone, Copy, PartialEq)]
enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

/// Batching usage tracker that protects Zion API from request floods.
///
/// Features:
/// - Fire-and-forget `track()` - non-blocking, never fails
/// - Aggregates increments by (user, limit) before sending
/// - Rate limits Zion API calls using governor
/// - Circuit breaker for graceful degradation
/// - Redis persistence for failed increments with retry
pub struct BatchingUsageTracker {
    sender: mpsc::Sender<UsageIncrement>,
}

impl BatchingUsageTracker {
    /// Create a new batching usage tracker
    ///
    /// Spawns a background task that processes increments.
    pub fn new(
        zion_client: Arc<ZionClient>,
        redis: redis::aio::ConnectionManager,
        config: BatchingConfig,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(config.channel_buffer);

        // Spawn background worker
        tokio::spawn(Self::background_worker(zion_client, redis, receiver, config));

        Self { sender }
    }

    /// Create with default configuration
    pub fn with_defaults(
        zion_client: Arc<ZionClient>,
        redis: redis::aio::ConnectionManager,
    ) -> Self {
        Self::new(zion_client, redis, BatchingConfig::default())
    }

    /// Track AI usage - fire-and-forget
    ///
    /// This method never blocks and never fails. If the channel is full,
    /// the increment is dropped and logged.
    pub fn track(&self, external_id: String, input_tokens: u64, output_tokens: u64) {
        // Send unified usage increment
        self.send_increment(UsageIncrement {
            external_id,
            input_tokens: input_tokens as i64,
            output_tokens: output_tokens as i64,
            requests: 1,
        });
    }

    /// Track a request without token usage - fire-and-forget
    ///
    /// Use this for endpoints that don't have token counts (audio, images, etc.)
    /// Only increments the request count.
    pub fn track_request_only(&self, external_id: String) {
        self.track(external_id, 0, 0);
    }

    /// Send a single increment to the channel (fire-and-forget)
    fn send_increment(&self, increment: UsageIncrement) {
        if let Err(e) = self.sender.try_send(increment) {
            match e {
                mpsc::error::TrySendError::Full(inc) => {
                    warn!(
                        external_id = %inc.external_id,
                        input_tokens = inc.input_tokens,
                        output_tokens = inc.output_tokens,
                        requests = inc.requests,
                        "Usage tracking channel full, dropping increment"
                    );
                }
                mpsc::error::TrySendError::Closed(inc) => {
                    error!(
                        external_id = %inc.external_id,
                        input_tokens = inc.input_tokens,
                        output_tokens = inc.output_tokens,
                        "Usage tracking channel closed, dropping increment"
                    );
                }
            }
        }
    }

    /// Background worker that processes increments
    async fn background_worker(
        zion_client: Arc<ZionClient>,
        redis: redis::aio::ConnectionManager,
        mut receiver: mpsc::Receiver<UsageIncrement>,
        config: BatchingConfig,
    ) {
        info!(
            batch_size = config.max_batch_size,
            flush_interval_ms = config.flush_interval.as_millis(),
            rate_limit = config.rate_limit_per_second,
            retry_interval_s = config.retry_interval.as_secs(),
            "Starting batching usage tracker worker"
        );

        // Create rate limiter
        let rate_limiter = RateLimiter::direct(Quota::per_second(
            NonZeroU32::new(config.rate_limit_per_second).unwrap(),
        ));

        // Circuit breaker state
        let mut circuit_state = CircuitState::Closed;
        let mut consecutive_failures: u32 = 0;
        let mut circuit_opened_at: Option<std::time::Instant> = None;

        // Aggregation buffer - keyed by external_id
        let mut buffer: HashMap<String, AggregatedUsage> = HashMap::new();
        let mut last_flush = std::time::Instant::now();
        let mut last_retry = std::time::Instant::now();

        loop {
            // Calculate time until next forced flush
            let time_until_flush = config
                .flush_interval
                .saturating_sub(last_flush.elapsed());

            // Calculate time until next retry attempt
            let time_until_retry = config
                .retry_interval
                .saturating_sub(last_retry.elapsed());

            tokio::select! {
                // Receive increment from channel
                maybe_increment = receiver.recv() => {
                    match maybe_increment {
                        Some(increment) => {
                            // Aggregate increment by external_id
                            buffer
                                .entry(increment.external_id.clone())
                                .or_default()
                                .add(&increment);

                            // Flush if batch is full
                            if buffer.len() >= config.max_batch_size {
                                Self::flush_buffer(
                                    &zion_client,
                                    &redis,
                                    &rate_limiter,
                                    &mut buffer,
                                    &mut circuit_state,
                                    &mut consecutive_failures,
                                    &mut circuit_opened_at,
                                    &config,
                                ).await;
                                last_flush = std::time::Instant::now();
                            }
                        }
                        None => {
                            // Channel closed, flush remaining and exit
                            if !buffer.is_empty() {
                                Self::flush_buffer(
                                    &zion_client,
                                    &redis,
                                    &rate_limiter,
                                    &mut buffer,
                                    &mut circuit_state,
                                    &mut consecutive_failures,
                                    &mut circuit_opened_at,
                                    &config,
                                ).await;
                            }
                            info!("Batching usage tracker shutting down");
                            break;
                        }
                    }
                }
                // Timer for periodic flush
                _ = tokio::time::sleep(time_until_flush) => {
                    if !buffer.is_empty() {
                        Self::flush_buffer(
                            &zion_client,
                            &redis,
                            &rate_limiter,
                            &mut buffer,
                            &mut circuit_state,
                            &mut consecutive_failures,
                            &mut circuit_opened_at,
                            &config,
                        ).await;
                        last_flush = std::time::Instant::now();
                    }
                }
                // Timer for retrying failed increments
                _ = tokio::time::sleep(time_until_retry) => {
                    // Only retry when circuit is closed
                    if circuit_state == CircuitState::Closed {
                        Self::retry_failed_increments(
                            &zion_client,
                            &redis,
                            &rate_limiter,
                            &mut circuit_state,
                            &mut consecutive_failures,
                            &mut circuit_opened_at,
                            &config,
                        ).await;
                    }
                    last_retry = std::time::Instant::now();
                }
            }
        }
    }

    /// Flush the aggregation buffer to Zion using batch-increment API
    async fn flush_buffer(
        zion_client: &Arc<ZionClient>,
        redis: &redis::aio::ConnectionManager,
        rate_limiter: &RateLimiter<
            governor::state::NotKeyed,
            governor::state::InMemoryState,
            governor::clock::DefaultClock,
        >,
        buffer: &mut HashMap<String, AggregatedUsage>,
        circuit_state: &mut CircuitState,
        consecutive_failures: &mut u32,
        circuit_opened_at: &mut Option<std::time::Instant>,
        config: &BatchingConfig,
    ) {
        // Check circuit breaker state
        match *circuit_state {
            CircuitState::Open => {
                // Check if we should try half-open
                if let Some(opened_at) = *circuit_opened_at {
                    if opened_at.elapsed() >= config.circuit_breaker_reset {
                        debug!("Circuit breaker transitioning to half-open");
                        *circuit_state = CircuitState::HalfOpen;
                    } else {
                        // Still open, drop increments
                        let count = buffer.len();
                        buffer.clear();
                        warn!(
                            dropped_count = count,
                            "Circuit breaker open, dropping usage increments"
                        );
                        return;
                    }
                }
            }
            CircuitState::Closed | CircuitState::HalfOpen => {
                // Continue processing
            }
        }

        // Drain buffer and filter out empty entries
        let increments: Vec<(String, AggregatedUsage)> = buffer
            .drain()
            .filter(|(_, usage)| !usage.is_empty())
            .collect();
        let total_increments = increments.len();

        if total_increments == 0 {
            return;
        }

        debug!(
            user_count = total_increments,
            "Flushing usage increments to Zion"
        );

        // Wait for rate limiter
        rate_limiter.until_ready().await;

        // Convert to batch increment items
        let batch_items: Vec<BatchIncrementItem> = increments
            .iter()
            .map(|(external_id, usage)| BatchIncrementItem {
                external_id: external_id.clone(),
                limit_name: "ai_usage".to_string(),
                ai_input_tokens: if usage.input_tokens > 0 {
                    Some(usage.input_tokens)
                } else {
                    None
                },
                ai_output_tokens: if usage.output_tokens > 0 {
                    Some(usage.output_tokens)
                } else {
                    None
                },
                ai_requests: if usage.requests > 0 {
                    Some(usage.requests)
                } else {
                    None
                },
            })
            .collect();

        // Send batch to Zion
        match zion_client.batch_increment(batch_items).await {
            Ok(result) => {
                // Reset failure count on success
                if *circuit_state == CircuitState::HalfOpen {
                    debug!("Circuit breaker closing after successful request");
                    *circuit_state = CircuitState::Closed;
                }
                *consecutive_failures = 0;
                *circuit_opened_at = None;

                if result.failed > 0 {
                    warn!(
                        processed = result.processed,
                        failed = result.failed,
                        "Batch increment completed with partial failures"
                    );
                    // Persist failed items to Redis for retry
                    for item_result in result.results.iter().filter(|r| !r.success) {
                        // Find the original usage data
                        if let Some((external_id, usage)) = increments
                            .iter()
                            .find(|(id, _)| *id == item_result.external_id)
                        {
                            let increment = UsageIncrement {
                                external_id: external_id.clone(),
                                input_tokens: usage.input_tokens,
                                output_tokens: usage.output_tokens,
                                requests: usage.requests,
                            };
                            if let Err(redis_err) =
                                Self::persist_failed_increment(redis, &increment).await
                            {
                                error!(
                                    error = %redis_err,
                                    external_id = %external_id,
                                    "Failed to persist failed increment to Redis"
                                );
                            }
                        }
                    }
                } else {
                    debug!(
                        processed = result.processed,
                        "Batch flush completed successfully"
                    );
                }
            }
            Err(e) => {
                *consecutive_failures += 1;

                warn!(
                    user_count = total_increments,
                    error = %e,
                    consecutive_failures = *consecutive_failures,
                    "Failed to batch increment usage"
                );

                // Persist all to Redis for retry
                for (external_id, usage) in &increments {
                    let increment = UsageIncrement {
                        external_id: external_id.clone(),
                        input_tokens: usage.input_tokens,
                        output_tokens: usage.output_tokens,
                        requests: usage.requests,
                    };
                    if let Err(redis_err) = Self::persist_failed_increment(redis, &increment).await
                    {
                        error!(
                            error = %redis_err,
                            external_id = %external_id,
                            "Failed to persist failed increment to Redis"
                        );
                    }
                }

                // Check if we should open circuit
                if *consecutive_failures >= config.circuit_breaker_threshold {
                    error!(
                        threshold = config.circuit_breaker_threshold,
                        reset_seconds = config.circuit_breaker_reset.as_secs(),
                        "Circuit breaker opening due to consecutive failures"
                    );
                    *circuit_state = CircuitState::Open;
                    *circuit_opened_at = Some(std::time::Instant::now());
                }
            }
        }
    }

    /// Persist a failed increment to Redis for later retry
    async fn persist_failed_increment(
        redis: &redis::aio::ConnectionManager,
        increment: &UsageIncrement,
    ) -> Result<(), redis::RedisError> {
        let mut conn = redis.clone();
        let json = serde_json::to_string(increment).map_err(|e| {
            redis::RedisError::from((
                redis::ErrorKind::IoError,
                "JSON serialization error",
                e.to_string(),
            ))
        })?;

        // Use RPUSH to add to a list (FIFO queue)
        conn.rpush::<_, _, ()>(REDIS_FAILED_INCREMENTS_KEY, json)
            .await?;

        debug!(
            external_id = %increment.external_id,
            input_tokens = increment.input_tokens,
            output_tokens = increment.output_tokens,
            requests = increment.requests,
            "Persisted failed increment to Redis"
        );

        Ok(())
    }

    /// Retry failed increments from Redis
    ///
    /// Uses single increment API for retries since these are typically
    /// smaller numbers of items that failed previously.
    async fn retry_failed_increments(
        zion_client: &Arc<ZionClient>,
        redis: &redis::aio::ConnectionManager,
        rate_limiter: &RateLimiter<
            governor::state::NotKeyed,
            governor::state::InMemoryState,
            governor::clock::DefaultClock,
        >,
        circuit_state: &mut CircuitState,
        consecutive_failures: &mut u32,
        circuit_opened_at: &mut Option<std::time::Instant>,
        config: &BatchingConfig,
    ) {
        let mut conn = redis.clone();

        // Get the number of failed increments
        let len: usize = match conn.llen(REDIS_FAILED_INCREMENTS_KEY).await {
            Ok(l) => l,
            Err(e) => {
                warn!(error = %e, "Failed to get failed increments count from Redis");
                return;
            }
        };

        if len == 0 {
            return;
        }

        let batch_size = len.min(config.max_retry_batch);
        info!(
            total_pending = len,
            batch_size = batch_size,
            "Retrying failed usage increments"
        );

        let mut success_count = 0;
        let mut failure_count = 0;

        for _ in 0..batch_size {
            // Pop from the front of the list (FIFO)
            let json: Option<String> = match conn.lpop(REDIS_FAILED_INCREMENTS_KEY, None).await {
                Ok(j) => j,
                Err(e) => {
                    warn!(error = %e, "Failed to pop from Redis queue");
                    break;
                }
            };

            let Some(json) = json else {
                break;
            };

            let increment: UsageIncrement = match serde_json::from_str(&json) {
                Ok(i) => i,
                Err(e) => {
                    error!(error = %e, json = %json, "Failed to deserialize increment");
                    continue;
                }
            };

            // Wait for rate limiter
            rate_limiter.until_ready().await;

            // Try to send to Zion using unified increment API
            match zion_client
                .increment_usage(
                    &increment.external_id,
                    increment.input_tokens,
                    increment.output_tokens,
                    increment.requests,
                )
                .await
            {
                Ok(_) => {
                    success_count += 1;
                    *consecutive_failures = 0;
                    debug!(
                        external_id = %increment.external_id,
                        input_tokens = increment.input_tokens,
                        output_tokens = increment.output_tokens,
                        requests = increment.requests,
                        "Retry successful"
                    );
                }
                Err(e) => {
                    failure_count += 1;
                    *consecutive_failures += 1;

                    warn!(
                        external_id = %increment.external_id,
                        input_tokens = increment.input_tokens,
                        output_tokens = increment.output_tokens,
                        error = %e,
                        "Retry failed, re-queuing"
                    );

                    // Re-queue the failed increment
                    if let Err(redis_err) =
                        Self::persist_failed_increment(redis, &increment).await
                    {
                        error!(
                            error = %redis_err,
                            "Failed to re-queue increment to Redis"
                        );
                    }

                    // Check if we should open circuit
                    if *consecutive_failures >= config.circuit_breaker_threshold {
                        error!(
                            threshold = config.circuit_breaker_threshold,
                            "Circuit breaker opening during retry"
                        );
                        *circuit_state = CircuitState::Open;
                        *circuit_opened_at = Some(std::time::Instant::now());
                        break;
                    }
                }
            }
        }

        if success_count > 0 || failure_count > 0 {
            info!(
                success = success_count,
                failed = failure_count,
                remaining = len.saturating_sub(success_count),
                "Retry batch completed"
            );
        }
    }
}

/// Metrics for the batching tracker
pub mod metrics {
    use metrics::{counter, gauge};

    /// Record a dropped increment due to channel full
    pub fn record_dropped_increment() {
        counter!("sentinel_usage_dropped_total").increment(1);
    }

    /// Record a successful increment
    pub fn record_successful_increment() {
        counter!("sentinel_usage_success_total").increment(1);
    }

    /// Record a failed increment
    pub fn record_failed_increment() {
        counter!("sentinel_usage_failed_total").increment(1);
    }

    /// Set the current circuit breaker state (0=closed, 1=half-open, 2=open)
    pub fn set_circuit_state(state: u8) {
        gauge!("sentinel_usage_circuit_state").set(state as f64);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_batching_config_default() {
        let config = BatchingConfig::default();
        assert_eq!(config.max_batch_size, 100);
        assert_eq!(config.flush_interval, Duration::from_millis(500));
        assert_eq!(config.channel_buffer, 10_000);
        assert_eq!(config.rate_limit_per_second, 20);
        assert_eq!(config.circuit_breaker_threshold, 3);
        assert_eq!(config.circuit_breaker_reset, Duration::from_secs(30));
        assert_eq!(config.retry_interval, Duration::from_secs(60));
        assert_eq!(config.max_retry_batch, 50);
    }

    #[test]
    fn test_aggregated_usage_default() {
        let usage = AggregatedUsage::default();
        assert_eq!(usage.input_tokens, 0);
        assert_eq!(usage.output_tokens, 0);
        assert_eq!(usage.requests, 0);
        assert!(usage.is_empty());
    }

    #[test]
    fn test_aggregated_usage_add() {
        let mut usage = AggregatedUsage::default();
        let increment = UsageIncrement {
            external_id: "user1".to_string(),
            input_tokens: 100,
            output_tokens: 50,
            requests: 1,
        };

        usage.add(&increment);
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.output_tokens, 50);
        assert_eq!(usage.requests, 1);
        assert!(!usage.is_empty());

        // Add more
        usage.add(&increment);
        assert_eq!(usage.input_tokens, 200);
        assert_eq!(usage.output_tokens, 100);
        assert_eq!(usage.requests, 2);
    }

    #[test]
    fn test_aggregated_usage_is_empty() {
        let empty = AggregatedUsage::default();
        assert!(empty.is_empty());

        let with_input = AggregatedUsage {
            input_tokens: 1,
            output_tokens: 0,
            requests: 0,
        };
        assert!(!with_input.is_empty());

        let with_output = AggregatedUsage {
            input_tokens: 0,
            output_tokens: 1,
            requests: 0,
        };
        assert!(!with_output.is_empty());

        let with_request = AggregatedUsage {
            input_tokens: 0,
            output_tokens: 0,
            requests: 1,
        };
        assert!(!with_request.is_empty());
    }

    #[test]
    fn test_usage_increment_serialization() {
        let increment = UsageIncrement {
            external_id: "user123".to_string(),
            input_tokens: 100,
            output_tokens: 50,
            requests: 1,
        };

        let json = serde_json::to_string(&increment).unwrap();
        let deserialized: UsageIncrement = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.external_id, "user123");
        assert_eq!(deserialized.input_tokens, 100);
        assert_eq!(deserialized.output_tokens, 50);
        assert_eq!(deserialized.requests, 1);
    }

    #[test]
    fn test_circuit_state_transitions() {
        let mut state = CircuitState::Closed;
        assert_eq!(state, CircuitState::Closed);

        state = CircuitState::Open;
        assert_eq!(state, CircuitState::Open);

        state = CircuitState::HalfOpen;
        assert_eq!(state, CircuitState::HalfOpen);

        state = CircuitState::Closed;
        assert_eq!(state, CircuitState::Closed);
    }

    #[test]
    fn test_aggregation_by_external_id() {
        use std::collections::HashMap;

        let mut buffer: HashMap<String, AggregatedUsage> = HashMap::new();

        // First increment for user1
        let inc1 = UsageIncrement {
            external_id: "user1".to_string(),
            input_tokens: 100,
            output_tokens: 50,
            requests: 1,
        };
        buffer.entry(inc1.external_id.clone()).or_default().add(&inc1);

        // Second increment for user1 (should aggregate)
        let inc2 = UsageIncrement {
            external_id: "user1".to_string(),
            input_tokens: 200,
            output_tokens: 100,
            requests: 1,
        };
        buffer.entry(inc2.external_id.clone()).or_default().add(&inc2);

        // Increment for user2
        let inc3 = UsageIncrement {
            external_id: "user2".to_string(),
            input_tokens: 50,
            output_tokens: 25,
            requests: 1,
        };
        buffer.entry(inc3.external_id.clone()).or_default().add(&inc3);

        // Verify aggregation
        assert_eq!(buffer.len(), 2);

        let user1_usage = buffer.get("user1").unwrap();
        assert_eq!(user1_usage.input_tokens, 300);
        assert_eq!(user1_usage.output_tokens, 150);
        assert_eq!(user1_usage.requests, 2);

        let user2_usage = buffer.get("user2").unwrap();
        assert_eq!(user2_usage.input_tokens, 50);
        assert_eq!(user2_usage.output_tokens, 25);
        assert_eq!(user2_usage.requests, 1);
    }
}
