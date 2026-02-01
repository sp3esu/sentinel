# Phase 4: Tier Routing - Research

**Researched:** 2026-02-01
**Domain:** Tier-based model routing with cost-weighted selection and provider health tracking
**Confidence:** HIGH

## Summary

Phase 4 implements tier-based routing that maps complexity tiers (simple, moderate, complex) to specific AI models based on configuration from Zion. The core mechanism involves: (1) fetching tier configuration from a new Zion endpoint, (2) caching it with 30-minute TTL, (3) selecting models using cost-weighted probabilistic selection, and (4) tracking provider/model health with exponential backoff.

The research reveals that Sentinel already has all foundational infrastructure needed:
- `SessionManager` and session stickiness (Phase 3) for model-level locking
- `SubscriptionCache` pattern for Zion config caching
- `failsafe` crate for circuit breaker patterns (already in Cargo.toml)
- `metrics` crate for Prometheus metrics (already integrated)
- Established patterns for async health checks (see `health.rs`)

The main new work involves:
1. Adding `Tier` enum and `tier` field to `ChatCompletionRequest` (replacing `model`)
2. Creating `TierConfig` types and Zion client method for `GET /api/v1/tiers/config`
3. Building `TierRouter` service for model selection with weighted random choice
4. Adding provider health tracking with exponential backoff
5. Implementing session tier upgrade logic (simple -> moderate -> complex only)

**Primary recommendation:** Follow the existing `SubscriptionCache` pattern for tier config caching. Use `rand::distributions::WeightedIndex` for cost-based selection (no new dependencies needed since `rand` is a transitive dependency). Use the existing `failsafe` crate for circuit breaker/backoff patterns.

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| serde/serde_json | 1.x | Tier config serialization | Already in use throughout codebase |
| rand | 0.8.x | Weighted random selection | Transitive dependency, WeightedIndex is O(log N) |
| failsafe | 1.2 | Circuit breaker for provider health | Already in Cargo.toml, unused until now |
| metrics | 0.22 | Prometheus metrics for tier usage | Already integrated in `routes/metrics.rs` |
| chrono | 0.4 | Timestamp for backoff tracking | Already in Cargo.toml |
| tokio | 1.x | Async health checks, timers | Already the async runtime |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| tracing | 0.1 | Debug logging for routing decisions | Already in use, add tier/model spans |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| rand WeightedIndex | weighted_rand crate | WeightedIndex is O(log N) vs O(1), but simpler and no new dep |
| failsafe | Custom exponential backoff | failsafe already in deps, battle-tested |
| In-memory health state | Redis health state | In-memory is simpler, instance-local health is appropriate |

**No new dependencies required** - all needed libraries are already in Cargo.toml or transitive dependencies.

## Architecture Patterns

### Recommended Project Structure

```
src/
  native/
    types.rs          # UPDATE: Add Tier enum
    request.rs        # UPDATE: Replace model with tier field
    session.rs        # UPDATE: Add tier to Session, tier upgrade logic
  tiers/
    mod.rs            # NEW: Module exports
    config.rs         # NEW: TierConfig types
    router.rs         # NEW: TierRouter for model selection
    health.rs         # NEW: ProviderHealthTracker with exponential backoff
    cache.rs          # NEW: TierConfigCache (follows SubscriptionCache pattern)
  zion/
    client.rs         # UPDATE: Add get_tier_config method
    models.rs         # UPDATE: Add TierConfigResponse types
  routes/
    metrics.rs        # UPDATE: Add tier/model selection metrics
  lib.rs              # UPDATE: Add TierRouter to AppState
```

### Pattern 1: Tier Enum with Ordering

**What:** Define tiers as an ordered enum for upgrade-only session logic.

**When to use:** Throughout the tier routing implementation.

**Example:**

```rust
// src/native/types.rs
use serde::{Deserialize, Serialize};

/// Complexity tier for model routing
/// Ordered from lowest to highest complexity for upgrade comparison
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    Simple,
    Moderate,
    Complex,
}

impl Default for Tier {
    fn default() -> Self {
        Tier::Simple
    }
}

impl Tier {
    /// Check if upgrading from current tier to new tier is allowed
    /// Upgrades: simple -> moderate -> complex allowed
    /// Downgrades: not allowed within session
    pub fn can_upgrade_to(&self, new_tier: &Tier) -> bool {
        new_tier >= self
    }
}
```

### Pattern 2: Tier Configuration from Zion

**What:** Cache tier->model mapping from dedicated Zion endpoint.

**When to use:** For all tier routing decisions.

**Example:**

```rust
// src/zion/models.rs - add to existing file
use serde::{Deserialize, Serialize};

/// Model configuration for a single provider/model combination
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelConfig {
    /// Provider name (e.g., "openai", "anthropic")
    pub provider: String,
    /// Model identifier (e.g., "gpt-4o-mini", "gpt-4o")
    pub model: String,
    /// Relative cost score (1-10, lower is cheaper)
    pub relative_cost: u8,
    /// Input token price per million (for reporting)
    pub input_price_per_million: f64,
    /// Output token price per million (for reporting)
    pub output_price_per_million: f64,
}

/// Tier configuration mapping
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TierMapping {
    pub simple: Vec<ModelConfig>,
    pub moderate: Vec<ModelConfig>,
    pub complex: Vec<ModelConfig>,
}

/// Response from tier config endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TierConfigResponse {
    pub success: bool,
    pub data: TierConfigData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TierConfigData {
    pub version: String,
    pub updated_at: String,
    pub tiers: TierMapping,
}

// src/zion/client.rs - add method
impl ZionClient {
    /// Get tier configuration (global, not per-user)
    #[instrument(skip(self))]
    pub async fn get_tier_config(&self) -> AppResult<TierConfigData> {
        let url = format!("{}/api/v1/tiers/config", self.base_url);

        let response = self
            .client
            .get(&url)
            .headers(self.api_key_headers())
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(AppError::UpstreamError(format!(
                "Zion tier config error {}: {}", status, text
            )));
        }

        let body = response.text().await?;
        let result: TierConfigResponse = serde_json::from_str(&body)?;
        Ok(result.data)
    }
}
```

### Pattern 3: Cost-Weighted Model Selection

**What:** Select model probabilistically favoring lower-cost options.

**When to use:** When selecting a model for a tier with multiple options.

**Example:**

```rust
// src/tiers/router.rs
use rand::distributions::WeightedIndex;
use rand::prelude::*;

/// Weight calculation: inverse of relative_cost for probabilistic selection
/// Higher relative_cost -> lower weight -> less likely to be chosen
fn calculate_weights(models: &[ModelConfig]) -> Vec<f64> {
    models.iter()
        .map(|m| 1.0 / (m.relative_cost as f64))
        .collect()
}

/// Select a model from available options using cost-weighted random selection
pub fn select_model_weighted(
    models: &[ModelConfig],
    health_tracker: &ProviderHealthTracker,
) -> Option<&ModelConfig> {
    // Filter to healthy models only
    let healthy_models: Vec<&ModelConfig> = models.iter()
        .filter(|m| health_tracker.is_available(&m.provider, &m.model))
        .collect();

    if healthy_models.is_empty() {
        return None;
    }

    // If only one option, return it
    if healthy_models.len() == 1 {
        return Some(healthy_models[0]);
    }

    // Calculate weights (inverse of cost)
    let weights: Vec<f64> = healthy_models.iter()
        .map(|m| 1.0 / (m.relative_cost as f64))
        .collect();

    // Create weighted distribution
    let dist = WeightedIndex::new(&weights).ok()?;
    let mut rng = thread_rng();
    let index = dist.sample(&mut rng);

    Some(healthy_models[index])
}
```

### Pattern 4: Provider Health Tracking with Exponential Backoff

**What:** Track provider/model availability with exponential backoff on failures.

**When to use:** To skip unavailable providers and implement retry logic.

**Example:**

```rust
// src/tiers/health.rs
use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};

/// Health state for a provider/model combination
#[derive(Debug, Clone)]
struct HealthState {
    /// Whether currently considered available
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
            backoff_duration: Duration::from_secs(30), // Start at 30s
            consecutive_failures: 0,
        }
    }
}

/// Configuration for health tracking
pub struct HealthConfig {
    pub initial_backoff: Duration,    // 30 seconds
    pub max_backoff: Duration,        // 5 minutes
    pub backoff_multiplier: f64,      // 2.0 (double on each failure)
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

/// Tracks health of provider/model combinations
pub struct ProviderHealthTracker {
    states: RwLock<HashMap<(String, String), HealthState>>,
    config: HealthConfig,
}

impl ProviderHealthTracker {
    pub fn new(config: HealthConfig) -> Self {
        Self {
            states: RwLock::new(HashMap::new()),
            config,
        }
    }

    /// Check if a provider/model is currently available
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
                        return true; // Ready to retry
                    }
                }
                false
            }
        }
    }

    /// Record a successful request (reset backoff)
    pub fn record_success(&self, provider: &str, model: &str) {
        let key = (provider.to_string(), model.to_string());
        let mut states = self.states.write().unwrap();

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

        // Exponential backoff: double each time, cap at max
        let new_backoff = Duration::from_secs_f64(
            self.config.initial_backoff.as_secs_f64()
            * self.config.backoff_multiplier.powi(state.consecutive_failures as i32 - 1)
        );
        state.backoff_duration = new_backoff.min(self.config.max_backoff);
    }
}
```

### Pattern 5: Session Tier Upgrade Logic

**What:** Sessions lock to model but can upgrade tier (simple -> moderate -> complex).

**When to use:** When handling requests with existing sessions.

**Example:**

```rust
// src/native/session.rs - update Session struct
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Session {
    pub id: String,
    pub provider: String,
    pub model: String,
    pub tier: Tier,  // NEW: track the tier
    pub external_id: String,
    pub created_at: i64,
}

// src/native_routes/chat.rs - session handling logic
async fn resolve_model_for_request(
    state: &AppState,
    request: &ChatCompletionRequest,
    session: Option<&Session>,
    external_id: &str,
) -> Result<(String, String, Tier), NativeErrorResponse> {
    let requested_tier = request.tier.unwrap_or_default();

    if let Some(session) = session {
        // Existing session: check if tier upgrade is allowed
        if !session.tier.can_upgrade_to(&requested_tier) {
            // Downgrade not allowed, use session's tier/model
            debug!(
                session_tier = ?session.tier,
                requested_tier = ?requested_tier,
                "Tier downgrade not allowed, using session tier"
            );
            return Ok((session.provider.clone(), session.model.clone(), session.tier));
        }

        if requested_tier > session.tier {
            // Tier upgrade: select new model for higher tier
            // But keep same provider for consistency when possible
            let (provider, model) = state.tier_router
                .select_model(requested_tier, Some(&session.provider))
                .await?;

            // Update session with new tier/model
            state.session_manager.update_tier(
                &session.id, &provider, &model, requested_tier
            ).await?;

            return Ok((provider, model, requested_tier));
        }

        // Same tier: use existing session model
        return Ok((session.provider.clone(), session.model.clone(), session.tier));
    }

    // No session: fresh selection
    let (provider, model) = state.tier_router
        .select_model(requested_tier, None)
        .await?;

    Ok((provider, model, requested_tier))
}
```

### Pattern 6: Tier Config Cache

**What:** Cache tier configuration with 30-minute TTL following SubscriptionCache pattern.

**When to use:** For all tier config lookups.

**Example:**

```rust
// src/tiers/cache.rs
use std::sync::Arc;
use crate::cache::redis::{keys, RedisCache};
use crate::zion::ZionClient;

pub struct TierConfigCache {
    cache: Arc<RedisCache>,
    zion_client: Arc<ZionClient>,
    ttl: u64, // 1800 seconds = 30 minutes
}

impl TierConfigCache {
    pub fn new(
        cache: Arc<RedisCache>,
        zion_client: Arc<ZionClient>,
        ttl: u64,
    ) -> Self {
        Self { cache, zion_client, ttl }
    }

    pub async fn get_config(&self) -> AppResult<TierConfigData> {
        let cache_key = "sentinel:tiers:config";

        // Try cache first
        if let Some(config) = self.cache.get::<TierConfigData>(&cache_key).await? {
            debug!("Tier config cache hit");
            return Ok(config);
        }

        debug!("Tier config cache miss, fetching from Zion");

        // Fetch from Zion
        let config = self.zion_client.get_tier_config().await?;

        // Cache the result
        self.cache.set_with_ttl(&cache_key, &config, self.ttl).await?;

        Ok(config)
    }
}
```

### Anti-Patterns to Avoid

- **Random selection without weights:** Always use cost-weighted selection, even with only 2 options. This ensures cheaper models get proportionally more traffic.

- **Global health state in Redis:** Keep health state in-memory per instance. Each Sentinel instance should independently track provider health based on its own observations.

- **Blocking health checks:** Use non-blocking backoff timer checks. Don't make synchronous health probe requests in the request path.

- **Tier downgrade allowed:** Never allow tier downgrade within a session. This would break conversation consistency (lower-tier models may not understand context from higher-tier responses).

- **Model field in request:** Remove the `model` field entirely from native API. Tier is the only selection mechanism.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Weighted random selection | Custom probability math | `rand::distributions::WeightedIndex` | Correct probability distribution, O(log N) |
| Exponential backoff | Manual duration calculation | Duration arithmetic with min/max | Simple, clear, no external dep |
| Circuit breaker | Custom state machine | `failsafe` crate (already in deps) | Battle-tested, handles edge cases |
| Config caching | New cache abstraction | Follow `SubscriptionCache` pattern | Consistent with existing code |

**Key insight:** The codebase already has patterns for everything needed. Follow existing patterns rather than inventing new abstractions.

## Common Pitfalls

### Pitfall 1: Session Model Mismatch After Tier Upgrade

**What goes wrong:** Session upgraded from simple to moderate tier, but session still references old model. Subsequent requests use wrong model.

**Why it happens:** Forgot to update session when tier upgrade occurs.

**How to avoid:** When tier upgrade happens, call `session_manager.update_tier()` to persist new provider/model/tier. Add a `Session.update()` method that modifies in place.

**Warning signs:** Logs show tier upgrade but model stays the same.

### Pitfall 2: All Models Unavailable Returns Wrong Error

**What goes wrong:** All models for a tier are in backoff, but error returned is generic "upstream error" instead of 503 with retry info.

**Why it happens:** Selection function returns None, caller doesn't handle gracefully.

**How to avoid:** When `select_model_weighted` returns None, return `AppError::ServiceUnavailable` with message indicating tier and retry-after header based on shortest backoff remaining.

**Warning signs:** 502 errors during provider outages instead of 503.

### Pitfall 3: Race Condition on Tier Config Cache Miss

**What goes wrong:** Multiple concurrent requests all get cache miss, all fetch from Zion, causing unnecessary load.

**Why it happens:** No coordination between concurrent fetches.

**How to avoid:** For Phase 4 v1, accept this tradeoff (Zion can handle it). For v2, consider using a mutex or single-flight pattern. Document as known limitation.

**Warning signs:** Zion logs show burst of tier config requests.

### Pitfall 4: Health State Lost on Restart

**What goes wrong:** Sentinel restarts, forgets all provider health state, immediately sends traffic to recently-failed provider.

**Why it happens:** Health state is in-memory only.

**How to avoid:** Accept this tradeoff for simplicity. After restart, all providers start as "available". First failure will re-trigger backoff. Alternatively, persist recent failures to Redis with TTL.

**Warning signs:** Immediate 502s after restart during provider outage.

### Pitfall 5: Cost Weight of Zero

**What goes wrong:** Model with relative_cost of 0 causes division by zero or panic.

**Why it happens:** Weight calculation uses `1.0 / cost`.

**How to avoid:** Validate that relative_cost is always >= 1 when parsing Zion config. Add assertion/validation in `ModelConfig` deserialization.

**Warning signs:** Panics in model selection.

## Code Examples

### Cache Key for Tier Config

```rust
// src/cache/redis.rs - add to keys module
pub mod keys {
    // ... existing keys ...

    /// Tier configuration cache key (global, not per-user)
    pub fn tier_config() -> &'static str {
        "sentinel:tiers:config"
    }
}
```

### Config Extension

```rust
// src/config.rs - add new field
pub struct Config {
    // ... existing fields ...

    /// Tier config cache TTL (default: 30 minutes = 1800 seconds)
    pub tier_config_ttl_seconds: u64,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            // ... existing ...
            tier_config_ttl_seconds: env::var("TIER_CONFIG_TTL_SECONDS")
                .unwrap_or_else(|_| "1800".to_string())
                .parse()
                .context("Invalid TIER_CONFIG_TTL_SECONDS")?,
        })
    }
}
```

### Metrics for Tier Routing

```rust
// src/routes/metrics.rs - add new metrics
pub fn register_metrics() {
    // ... existing ...

    metrics::describe_counter!(
        "sentinel_tier_requests_total",
        "Total requests by tier"
    );
    metrics::describe_counter!(
        "sentinel_model_selections_total",
        "Model selections by tier and model"
    );
    metrics::describe_counter!(
        "sentinel_provider_failures_total",
        "Provider failures triggering backoff"
    );
    metrics::describe_gauge!(
        "sentinel_provider_available",
        "Provider availability (1=available, 0=in backoff)"
    );
}

/// Record a tier request
pub fn record_tier_request(tier: &str) {
    metrics::counter!("sentinel_tier_requests_total", "tier" => tier.to_string())
        .increment(1);
}

/// Record model selection
pub fn record_model_selection(tier: &str, provider: &str, model: &str) {
    metrics::counter!(
        "sentinel_model_selections_total",
        "tier" => tier.to_string(),
        "provider" => provider.to_string(),
        "model" => model.to_string()
    )
    .increment(1);
}

/// Record provider failure
pub fn record_provider_failure(provider: &str, model: &str) {
    metrics::counter!(
        "sentinel_provider_failures_total",
        "provider" => provider.to_string(),
        "model" => model.to_string()
    )
    .increment(1);
}

/// Update provider availability gauge
pub fn set_provider_available(provider: &str, model: &str, available: bool) {
    metrics::gauge!(
        "sentinel_provider_available",
        "provider" => provider.to_string(),
        "model" => model.to_string()
    )
    .set(if available { 1.0 } else { 0.0 });
}
```

### Response Header for Selected Model

```rust
// In handler, after model selection:
let response = Response::builder()
    .status(StatusCode::OK)
    .header("X-Sentinel-Model", &model)
    .header("X-Sentinel-Tier", tier.to_string())
    .header(header::CONTENT_TYPE, "application/json")
    .body(body)?;
```

### Default Fallback Models (v1 only)

```rust
// Hardcoded defaults when Zion is unavailable AND cache is empty
// This should never happen in production (fail explicit per decisions)
// But useful for development/testing

const DEFAULT_TIER_MODELS: &[(&str, &str, &str)] = &[
    // (tier, provider, model)
    ("simple", "openai", "gpt-4o-mini"),
    ("moderate", "openai", "gpt-4o"),
    ("complex", "openai", "gpt-4o"),
];
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Client specifies model | Client specifies tier | This phase | Simpler API, better cost control |
| Static model mapping | Dynamic config from Zion | This phase | Hot-reconfigurable without redeploy |
| Equal probability selection | Cost-weighted selection | This phase | Lower costs, natural load distribution |
| No provider health tracking | Exponential backoff | This phase | Graceful degradation during outages |

**Deprecated/outdated:**
- `model` field in ChatCompletionRequest: Removed in favor of `tier`

## Open Questions

1. **Active health probes vs passive failure detection?**
   - What we know: Decisions specify "active health checks"
   - What's unclear: Frequency, timeout, which endpoints to probe
   - Recommendation: Start with passive (detect failures from real requests). Add active probes in v2 if needed. Passive is simpler and avoids unnecessary API calls.

2. **Retry on same model or next model?**
   - What we know: "Retry once with next model in tier, then fail"
   - What's unclear: Should retry be transparent to client or return error with option to retry?
   - Recommendation: Transparent retry. Client sends one request, Sentinel tries up to 2 models before failing with 503.

3. **Session update atomicity?**
   - What we know: Need to update session provider/model/tier on upgrade
   - What's unclear: What if update fails after successful request?
   - Recommendation: Fire-and-forget update with logging. Worst case is session isn't upgraded, next request will try again.

## Sources

### Primary (HIGH confidence)

- Sentinel codebase `/src/cache/subscription.rs` - Cache pattern reference
- Sentinel codebase `/src/native/session.rs` - Session management pattern
- Sentinel codebase `/src/routes/metrics.rs` - Prometheus metrics pattern
- Sentinel codebase `/src/routes/health.rs` - Health check pattern
- Sentinel codebase `/src/zion/client.rs` - Zion API client pattern
- docs.rs/rand/0.8 - WeightedIndex documentation
- docs.rs/failsafe/1.3 - Circuit breaker API
- docs.rs/metrics/0.22 - Metrics crate usage

### Secondary (MEDIUM confidence)

- CONTEXT.md decisions - User-specified implementation choices
- Phase 3 SESSION patterns - Foundation for tier stickiness

### Tertiary (LOW confidence)

- None - all findings verified against codebase or official docs

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - All libraries already in Cargo.toml or transitive deps
- Architecture: HIGH - Follows established patterns in codebase
- Pitfalls: HIGH - Based on analysis of existing code and common distributed systems patterns

**Research date:** 2026-02-01
**Valid until:** 60 days (stable domain, established patterns)
