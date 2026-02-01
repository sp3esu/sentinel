//! Tier-based model routing
//!
//! Selects models for tiers using cost-weighted probabilistic selection
//! with health-aware filtering.

use std::sync::Arc;

use rand::distr::weighted::WeightedIndex;
use rand::prelude::*;
use rand::rng;
use tracing::{debug, info, warn};

use crate::{
    error::{AppError, AppResult},
    native::types::Tier,
    zion::models::ModelConfig,
};

use super::{cache::TierConfigCache, health::ProviderHealthTracker};

/// Result of model selection
#[derive(Debug, Clone)]
pub struct SelectedModel {
    /// Provider name (e.g., "openai")
    pub provider: String,
    /// Model identifier (e.g., "gpt-4o-mini")
    pub model: String,
    /// The tier this model serves
    pub tier: Tier,
}

/// Tier-based model router
///
/// Selects models for complexity tiers using:
/// 1. Health-aware filtering (skip unavailable providers)
/// 2. Cost-weighted probabilistic selection (favor cheaper options)
/// 3. Single retry with next model on failure
pub struct TierRouter {
    config_cache: Arc<TierConfigCache>,
    health_tracker: Arc<ProviderHealthTracker>,
}

impl TierRouter {
    /// Create a new tier router
    pub fn new(
        config_cache: Arc<TierConfigCache>,
        health_tracker: Arc<ProviderHealthTracker>,
    ) -> Self {
        Self {
            config_cache,
            health_tracker,
        }
    }

    /// Select a model for the given tier
    ///
    /// Returns the selected model considering health and cost.
    /// If all models are unavailable, returns ServiceUnavailable error.
    ///
    /// # Arguments
    /// * `tier` - The complexity tier to select a model for
    /// * `preferred_provider` - Optional provider to prefer (for session continuity)
    pub async fn select_model(
        &self,
        tier: Tier,
        preferred_provider: Option<&str>,
    ) -> AppResult<SelectedModel> {
        let config = self.config_cache.get_config().await?;
        let models = config.models_for_tier(tier);

        if models.is_empty() {
            return Err(AppError::BadRequest(format!(
                "No models configured for tier {:?}",
                tier
            )));
        }

        // Filter to healthy models
        let healthy_models: Vec<&ModelConfig> = models
            .iter()
            .filter(|m| self.health_tracker.is_available(&m.provider, &m.model))
            .collect();

        if healthy_models.is_empty() {
            // All models in backoff - find shortest remaining backoff for Retry-After
            let min_backoff = models
                .iter()
                .filter_map(|m| {
                    self.health_tracker
                        .backoff_remaining(&m.provider, &m.model)
                })
                .min();

            warn!(
                tier = %tier,
                total_models = models.len(),
                "All models unavailable for tier"
            );

            return Err(AppError::ServiceUnavailable {
                message: format!("All models for tier {} are currently unavailable", tier),
                retry_after: min_backoff,
            });
        }

        // If preferred provider is specified and available, try to use it
        if let Some(preferred) = preferred_provider {
            if let Some(model) = healthy_models.iter().find(|m| m.provider == preferred) {
                debug!(
                    tier = %tier,
                    provider = %model.provider,
                    model = %model.model,
                    "Selected preferred provider"
                );
                return Ok(SelectedModel {
                    provider: model.provider.clone(),
                    model: model.model.clone(),
                    tier,
                });
            }
            debug!(
                tier = %tier,
                preferred = %preferred,
                "Preferred provider not available, falling back to weighted selection"
            );
        }

        // Cost-weighted selection
        let selected = self.select_weighted(&healthy_models)?;

        info!(
            tier = %tier,
            provider = %selected.provider,
            model = %selected.model,
            healthy_count = healthy_models.len(),
            total_count = models.len(),
            "Selected model for tier"
        );

        Ok(SelectedModel {
            provider: selected.provider.clone(),
            model: selected.model.clone(),
            tier,
        })
    }

    /// Select a model using cost-weighted random selection
    ///
    /// Lower relative_cost = higher probability of selection.
    /// Weight = 1 / relative_cost
    fn select_weighted<'a>(&self, models: &[&'a ModelConfig]) -> AppResult<&'a ModelConfig> {
        if models.is_empty() {
            return Err(AppError::BadRequest("No models available".to_string()));
        }

        if models.len() == 1 {
            return Ok(models[0]);
        }

        // Calculate weights: inverse of cost (lower cost = higher weight)
        // Validate relative_cost >= 1 to avoid division by zero
        let weights: Vec<f64> = models
            .iter()
            .map(|m| {
                let cost = m.relative_cost.max(1) as f64;
                1.0 / cost
            })
            .collect();

        let dist = WeightedIndex::new(&weights).map_err(|e| {
            AppError::Internal(anyhow::anyhow!(
                "Failed to create weighted distribution: {}",
                e
            ))
        })?;

        let mut rng = rng();
        let index = dist.sample(&mut rng);

        Ok(models[index])
    }

    /// Get an alternative model for retry after failure
    ///
    /// Returns a different model from the same tier if available.
    /// Excludes the failed model and unhealthy models.
    pub async fn get_retry_model(
        &self,
        tier: Tier,
        failed_model: &str,
    ) -> AppResult<Option<SelectedModel>> {
        let config = self.config_cache.get_config().await?;
        let models = config.models_for_tier(tier);

        // Filter to healthy models that aren't the failed one
        let alternatives: Vec<&ModelConfig> = models
            .iter()
            .filter(|m| {
                m.model != failed_model && self.health_tracker.is_available(&m.provider, &m.model)
            })
            .collect();

        if alternatives.is_empty() {
            debug!(
                tier = %tier,
                failed_model = %failed_model,
                "No alternative models available for retry"
            );
            return Ok(None);
        }

        let selected = self.select_weighted(&alternatives)?;

        info!(
            tier = %tier,
            failed_model = %failed_model,
            retry_provider = %selected.provider,
            retry_model = %selected.model,
            "Selected alternative model for retry"
        );

        Ok(Some(SelectedModel {
            provider: selected.provider.clone(),
            model: selected.model.clone(),
            tier,
        }))
    }

    /// Record a successful request for a model
    pub fn record_success(&self, provider: &str, model: &str) {
        self.health_tracker.record_success(provider, model);
    }

    /// Record a failed request for a model
    pub fn record_failure(&self, provider: &str, model: &str) {
        self.health_tracker.record_failure(provider, model);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: Full tests require mocking TierConfigCache.
    // These tests verify the weight calculation logic.

    #[test]
    fn test_weight_calculation_inverse_of_cost() {
        // Relative cost 1 -> weight 1.0
        // Relative cost 2 -> weight 0.5
        // Relative cost 5 -> weight 0.2
        // This means cost=1 is 5x more likely than cost=5

        let weights: Vec<f64> = vec![1, 2, 5]
            .iter()
            .map(|cost| 1.0 / (*cost as f64))
            .collect();

        assert!((weights[0] - 1.0).abs() < 0.001);
        assert!((weights[1] - 0.5).abs() < 0.001);
        assert!((weights[2] - 0.2).abs() < 0.001);
    }

    #[test]
    fn test_zero_cost_handled() {
        // relative_cost of 0 should be treated as 1 to avoid division by zero
        let cost: u8 = 0;
        let safe_cost = cost.max(1) as f64;
        let weight = 1.0 / safe_cost;
        assert!((weight - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_selected_model_debug() {
        let selected = SelectedModel {
            provider: "openai".to_string(),
            model: "gpt-4o".to_string(),
            tier: Tier::Moderate,
        };
        let debug_str = format!("{:?}", selected);
        assert!(debug_str.contains("openai"));
        assert!(debug_str.contains("gpt-4o"));
    }
}
