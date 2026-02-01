//! Tier routing module
//!
//! Handles mapping complexity tiers to AI models based on configuration from Zion.
//! Uses cost-weighted selection with health-aware filtering.

pub mod cache;
pub mod config;
pub mod health;

pub use cache::TierConfigCache;
pub use config::TierConfig;
pub use health::{HealthConfig, ProviderHealthTracker};
