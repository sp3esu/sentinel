//! Tier configuration types
//!
//! Re-exports and utilities for tier configuration.

use crate::native::types::Tier;
use crate::zion::models::{ModelConfig, TierConfigData};

/// Type alias for tier configuration
pub type TierConfig = TierConfigData;

impl TierConfig {
    /// Get models for a specific tier
    pub fn models_for_tier(&self, tier: Tier) -> &[ModelConfig] {
        match tier {
            Tier::Simple => &self.tiers.simple,
            Tier::Moderate => &self.tiers.moderate,
            Tier::Complex => &self.tiers.complex,
        }
    }
}
