//! Provider health tracking with exponential backoff
//!
//! Tracks provider/model availability and implements exponential backoff
//! when failures occur. This enables graceful degradation during outages.

use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};

use tracing::{debug, info, warn};

/// Configuration for health tracking
#[derive(Debug, Clone)]
pub struct HealthConfig {
    /// Initial backoff duration after first failure (default: 30 seconds)
    pub initial_backoff: Duration,
    /// Maximum backoff duration (default: 5 minutes)
    pub max_backoff: Duration,
    /// Multiplier for exponential backoff (default: 2.0)
    pub backoff_multiplier: f64,
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            initial_backoff: Duration::from_secs(30),
            max_backoff: Duration::from_secs(300), // 5 minutes
            backoff_multiplier: 2.0,
        }
    }
}

/// Health state for a provider/model combination
#[derive(Debug, Clone)]
struct HealthState {
    /// Whether currently considered available (before backoff check)
    available: bool,
    /// Time of last failure (for backoff calculation)
    last_failure: Option<Instant>,
    /// Current backoff duration
    backoff_duration: Duration,
    /// Number of consecutive failures
    consecutive_failures: u32,
}

impl Default for HealthState {
    fn default() -> Self {
        Self {
            available: true,
            last_failure: None,
            backoff_duration: Duration::from_secs(30),
            consecutive_failures: 0,
        }
    }
}

/// Tracks health of provider/model combinations
///
/// Uses exponential backoff to avoid hammering unhealthy providers.
/// State is kept in-memory per instance (each Sentinel instance
/// independently tracks health based on its own observations).
pub struct ProviderHealthTracker {
    states: RwLock<HashMap<(String, String), HealthState>>,
    config: HealthConfig,
}

impl ProviderHealthTracker {
    /// Create a new health tracker with default configuration
    pub fn new() -> Self {
        Self::with_config(HealthConfig::default())
    }

    /// Create a new health tracker with custom configuration
    pub fn with_config(config: HealthConfig) -> Self {
        Self {
            states: RwLock::new(HashMap::new()),
            config,
        }
    }

    /// Check if a provider/model is currently available
    ///
    /// Returns true if:
    /// - Never seen before (unknown = available)
    /// - Currently healthy
    /// - In backoff but backoff period has elapsed (ready to retry)
    pub fn is_available(&self, provider: &str, model: &str) -> bool {
        let key = (provider.to_string(), model.to_string());
        let states = self.states.read().unwrap();

        match states.get(&key) {
            None => true, // Unknown = available
            Some(state) => {
                if state.available {
                    return true;
                }
                // Check if backoff period has elapsed
                if let Some(last_failure) = state.last_failure {
                    if last_failure.elapsed() >= state.backoff_duration {
                        debug!(
                            provider = %provider,
                            model = %model,
                            backoff_secs = state.backoff_duration.as_secs(),
                            "Backoff elapsed, provider ready for retry"
                        );
                        return true; // Ready to retry
                    }
                }
                false
            }
        }
    }

    /// Get remaining backoff time for a provider/model (for Retry-After header)
    pub fn backoff_remaining(&self, provider: &str, model: &str) -> Option<Duration> {
        let key = (provider.to_string(), model.to_string());
        let states = self.states.read().unwrap();

        states.get(&key).and_then(|state| {
            if state.available {
                return None;
            }
            state.last_failure.map(|last| {
                let elapsed = last.elapsed();
                if elapsed >= state.backoff_duration {
                    Duration::ZERO
                } else {
                    state.backoff_duration - elapsed
                }
            })
        })
    }

    /// Record a successful request (reset backoff)
    pub fn record_success(&self, provider: &str, model: &str) {
        let key = (provider.to_string(), model.to_string());
        let mut states = self.states.write().unwrap();

        if let Some(state) = states.get(&key) {
            if !state.available || state.consecutive_failures > 0 {
                info!(
                    provider = %provider,
                    model = %model,
                    previous_failures = state.consecutive_failures,
                    "Provider recovered, resetting health state"
                );
            }
        }

        states.insert(key, HealthState::default());
    }

    /// Record a failure (apply exponential backoff)
    pub fn record_failure(&self, provider: &str, model: &str) {
        let key = (provider.to_string(), model.to_string());
        let mut states = self.states.write().unwrap();

        let state = states.entry(key).or_default();
        state.available = false;
        state.last_failure = Some(Instant::now());
        state.consecutive_failures += 1;

        // Calculate exponential backoff: initial * multiplier^(failures-1)
        let backoff_secs = self.config.initial_backoff.as_secs_f64()
            * self
                .config
                .backoff_multiplier
                .powi(state.consecutive_failures as i32 - 1);
        let new_backoff = Duration::from_secs_f64(backoff_secs);
        state.backoff_duration = new_backoff.min(self.config.max_backoff);

        warn!(
            provider = %provider,
            model = %model,
            consecutive_failures = state.consecutive_failures,
            backoff_secs = state.backoff_duration.as_secs(),
            "Provider failure recorded, entering backoff"
        );
    }

    /// Get current state summary for debugging/metrics
    pub fn get_unavailable_providers(&self) -> Vec<(String, String, u32)> {
        let states = self.states.read().unwrap();
        states
            .iter()
            .filter(|(_, state)| !state.available)
            .map(|((provider, model), state)| {
                (provider.clone(), model.clone(), state.consecutive_failures)
            })
            .collect()
    }
}

impl Default for ProviderHealthTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn test_new_provider_is_available() {
        let tracker = ProviderHealthTracker::new();
        assert!(tracker.is_available("openai", "gpt-4o"));
    }

    #[test]
    fn test_failure_marks_unavailable() {
        let tracker = ProviderHealthTracker::new();
        tracker.record_failure("openai", "gpt-4o");
        assert!(!tracker.is_available("openai", "gpt-4o"));
    }

    #[test]
    fn test_success_resets_state() {
        let tracker = ProviderHealthTracker::new();
        tracker.record_failure("openai", "gpt-4o");
        assert!(!tracker.is_available("openai", "gpt-4o"));

        tracker.record_success("openai", "gpt-4o");
        assert!(tracker.is_available("openai", "gpt-4o"));
    }

    #[test]
    fn test_backoff_elapsed_makes_available() {
        let config = HealthConfig {
            initial_backoff: Duration::from_millis(50),
            max_backoff: Duration::from_secs(1),
            backoff_multiplier: 2.0,
        };
        let tracker = ProviderHealthTracker::with_config(config);

        tracker.record_failure("openai", "gpt-4o");
        assert!(!tracker.is_available("openai", "gpt-4o"));

        // Wait for backoff to elapse
        sleep(Duration::from_millis(60));
        assert!(tracker.is_available("openai", "gpt-4o"));
    }

    #[test]
    fn test_exponential_backoff_increases() {
        let config = HealthConfig {
            initial_backoff: Duration::from_secs(10),
            max_backoff: Duration::from_secs(300),
            backoff_multiplier: 2.0,
        };
        let tracker = ProviderHealthTracker::with_config(config);

        // First failure: 10s backoff
        tracker.record_failure("openai", "gpt-4o");
        let remaining1 = tracker.backoff_remaining("openai", "gpt-4o").unwrap();
        assert!(remaining1.as_secs() <= 10);

        // Second failure: 20s backoff
        tracker.record_failure("openai", "gpt-4o");
        let remaining2 = tracker.backoff_remaining("openai", "gpt-4o").unwrap();
        assert!(remaining2.as_secs() <= 20);
        assert!(remaining2.as_secs() > 10);
    }

    #[test]
    fn test_backoff_capped_at_max() {
        let config = HealthConfig {
            initial_backoff: Duration::from_secs(100),
            max_backoff: Duration::from_secs(150),
            backoff_multiplier: 2.0,
        };
        let tracker = ProviderHealthTracker::with_config(config);

        // Multiple failures should cap at max
        for _ in 0..10 {
            tracker.record_failure("openai", "gpt-4o");
        }
        let remaining = tracker.backoff_remaining("openai", "gpt-4o").unwrap();
        assert!(remaining.as_secs() <= 150);
    }

    #[test]
    fn test_different_providers_tracked_separately() {
        let tracker = ProviderHealthTracker::new();

        tracker.record_failure("openai", "gpt-4o");
        assert!(!tracker.is_available("openai", "gpt-4o"));
        assert!(tracker.is_available("openai", "gpt-4o-mini")); // Different model
        assert!(tracker.is_available("anthropic", "claude-3")); // Different provider
    }

    #[test]
    fn test_get_unavailable_providers() {
        let tracker = ProviderHealthTracker::new();
        tracker.record_failure("openai", "gpt-4o");
        tracker.record_failure("openai", "gpt-4o"); // 2 failures

        let unavailable = tracker.get_unavailable_providers();
        assert_eq!(unavailable.len(), 1);
        assert_eq!(
            unavailable[0],
            ("openai".to_string(), "gpt-4o".to_string(), 2)
        );
    }
}
