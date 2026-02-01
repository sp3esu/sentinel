---
phase: 04-tier-routing
plan: 02
subsystem: tier-routing
tags: [health-tracking, model-selection, routing, metrics]
dependency-graph:
  requires: ["04-01", "04-01b"]
  provides: ["TierRouter", "ProviderHealthTracker", "SelectedModel", "tier-metrics"]
  affects: ["04-03"]
tech-stack:
  added: ["rand 0.9"]
  patterns: ["exponential-backoff", "cost-weighted-selection", "health-tracking"]
key-files:
  created:
    - src/tiers/health.rs
    - src/tiers/router.rs
  modified:
    - src/tiers/mod.rs
    - src/error.rs
    - src/routes/metrics.rs
    - src/lib.rs
    - Cargo.toml
decisions:
  - id: exponential-backoff-config
    choice: "30s initial, 2x multiplier, 5min max"
    rationale: "Per context decisions - balance retry speed with provider protection"
  - id: cost-weighted-formula
    choice: "weight = 1/relative_cost"
    rationale: "Simple inverse weighting favors cheaper models proportionally"
  - id: rand-0.9-update
    choice: "Use rand::rng() instead of deprecated thread_rng()"
    rationale: "Follow rand 0.9 API changes"
metrics:
  duration: 6min
  completed: 2026-02-01
---

# Phase 4 Plan 2: Model Selection Logic Summary

TierRouter with cost-weighted selection and ProviderHealthTracker with exponential backoff.

## What Was Built

### ProviderHealthTracker (`src/tiers/health.rs`)

Tracks provider/model availability with exponential backoff:

```rust
pub struct ProviderHealthTracker {
    states: RwLock<HashMap<(String, String), HealthState>>,
    config: HealthConfig,
}
```

Key features:
- Unknown providers default to available
- Failure triggers backoff: 30s initial, doubles each failure, caps at 5min
- Success resets state completely
- Backoff elapsed = ready for retry
- Each provider/model tracked independently

### TierRouter (`src/tiers/router.rs`)

Selects models for complexity tiers:

```rust
pub struct TierRouter {
    config_cache: Arc<TierConfigCache>,
    health_tracker: Arc<ProviderHealthTracker>,
}
```

Selection algorithm:
1. Get models for tier from cached config
2. Filter to healthy models only
3. If preferred provider available, use it
4. Otherwise, cost-weighted random selection (weight = 1/relative_cost)
5. If all models unavailable, return 503 with Retry-After

### ServiceUnavailable Error Enhancement

Updated `AppError::ServiceUnavailable` to include retry information:

```rust
ServiceUnavailable {
    message: String,
    retry_after: Option<Duration>,
}
```

Returns 503 status with `Retry-After` header when duration provided.

### Tier Routing Metrics

New Prometheus metrics for observability:
- `sentinel_tier_requests_total{tier}` - requests by tier
- `sentinel_model_selections_total{tier,provider,model}` - selections
- `sentinel_provider_failures_total{provider,model}` - failures triggering backoff
- `sentinel_model_retries_total{tier,failed_model,retry_model}` - retries
- `sentinel_provider_health{provider,model}` - health gauge (1/0)

## Commits

| Hash | Description |
|------|-------------|
| dd528d0 | Add ProviderHealthTracker with exponential backoff |
| 2a2f7d2 | Add TierRouter with cost-weighted model selection |
| 836d8ea | Add ServiceUnavailable error with Retry-After header |
| 5c9827f | Add tier routing Prometheus metrics |
| 1925c4e | Export tier routing types from lib.rs |

## Files Changed

| File | Change |
|------|--------|
| `src/tiers/health.rs` | Created - ProviderHealthTracker, HealthConfig |
| `src/tiers/router.rs` | Created - TierRouter, SelectedModel |
| `src/tiers/mod.rs` | Updated - export health, router modules |
| `src/error.rs` | Updated - ServiceUnavailable with retry_after |
| `src/routes/metrics.rs` | Updated - tier routing metrics |
| `src/lib.rs` | Updated - export new tier types |
| `Cargo.toml` | Updated - add rand 0.9 dependency |

## Tests Added

Health tracking (8 tests):
- test_new_provider_is_available
- test_failure_marks_unavailable
- test_success_resets_state
- test_backoff_elapsed_makes_available
- test_exponential_backoff_increases
- test_backoff_capped_at_max
- test_different_providers_tracked_separately
- test_get_unavailable_providers

Router (3 tests):
- test_weight_calculation_inverse_of_cost
- test_zero_cost_handled
- test_selected_model_debug

## Decisions Made

| Decision | Rationale |
|----------|-----------|
| 30s initial backoff | Balance between quick recovery and provider protection |
| 2x backoff multiplier | Standard exponential backoff |
| 5min max backoff | Prevent indefinite provider exclusion |
| weight = 1/relative_cost | Simple, effective cost weighting |
| Preferred provider first | Session continuity takes precedence |
| rand 0.9 API | Use current API, not deprecated thread_rng |

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] rand crate not in dependencies**
- Found during Task 2
- Issue: WeightedIndex requires rand crate
- Fix: Added rand 0.9 to Cargo.toml
- Commit: 2a2f7d2

**2. [Rule 3 - Blocking] rand 0.9 API changes**
- Found during Task 2
- Issue: rand::distributions::WeightedIndex moved to rand::distr::weighted
- Fix: Updated import paths, used rand::rng() instead of thread_rng()
- Commit: 2a2f7d2

**3. [Rule 1 - Bug] Lifetime issue in select_weighted**
- Found during Task 2
- Issue: Return type needed explicit lifetime annotation
- Fix: Added `'a` lifetime parameter to function signature
- Commit: 2a2f7d2

## Next Phase Readiness

Ready for Plan 03 (Handler Integration):
- TierRouter can select models via `select_model(tier, preferred_provider)`
- ProviderHealthTracker records success/failure
- Retry support via `get_retry_model(tier, failed_model)`
- Metrics ready for recording in handler
- All types exported from lib.rs
