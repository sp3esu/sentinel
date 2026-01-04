//! Configuration management for Sentinel
//!
//! Configuration is loaded from environment variables.

use anyhow::{Context, Result};
use std::env;

/// Application configuration
#[derive(Debug, Clone)]
pub struct Config {
    /// Host to bind to
    pub host: String,
    /// Port to listen on
    pub port: u16,

    /// Redis connection URL
    pub redis_url: String,

    /// Zion API base URL
    pub zion_api_url: String,
    /// Zion API key for external service authentication
    pub zion_api_key: String,

    /// OpenAI API URL
    pub openai_api_url: String,
    /// OpenAI API key (required for AI provider)
    pub openai_api_key: Option<String>,

    /// Cache TTL for user limits (in seconds)
    pub cache_ttl_seconds: u64,
    /// Cache TTL for JWT validation (in seconds)
    pub jwt_cache_ttl_seconds: u64,

    /// Enable debug endpoints (development only)
    pub debug_enabled: bool,
}

impl Config {
    /// Load configuration from environment variables
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            host: env::var("SENTINEL_HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
            port: env::var("SENTINEL_PORT")
                .unwrap_or_else(|_| "8080".to_string())
                .parse()
                .context("Invalid SENTINEL_PORT")?,

            redis_url: env::var("REDIS_URL")
                .unwrap_or_else(|_| "redis://localhost:6379".to_string()),

            zion_api_url: env::var("ZION_API_URL")
                .context("ZION_API_URL must be set")?,
            zion_api_key: env::var("ZION_API_KEY")
                .context("ZION_API_KEY must be set")?,

            openai_api_url: env::var("OPENAI_API_URL")
                .unwrap_or_else(|_| "https://api.openai.com/v1".to_string()),
            openai_api_key: env::var("OPENAI_API_KEY").ok(),

            cache_ttl_seconds: env::var("CACHE_TTL_SECONDS")
                .unwrap_or_else(|_| "300".to_string())
                .parse()
                .context("Invalid CACHE_TTL_SECONDS")?,
            jwt_cache_ttl_seconds: env::var("JWT_CACHE_TTL_SECONDS")
                .unwrap_or_else(|_| "300".to_string())
                .parse()
                .context("Invalid JWT_CACHE_TTL_SECONDS")?,

            debug_enabled: env::var("SENTINEL_DEBUG")
                .map(|v| v == "true" || v == "1")
                .unwrap_or(false),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_values() {
        // Set required env vars
        env::set_var("ZION_API_URL", "http://localhost:3000");
        env::set_var("ZION_API_KEY", "test-key");

        let config = Config::from_env().unwrap();

        assert_eq!(config.host, "0.0.0.0");
        assert_eq!(config.port, 8080);
        assert_eq!(config.redis_url, "redis://localhost:6379");
        assert_eq!(config.openai_api_url, "https://api.openai.com/v1");
        assert_eq!(config.cache_ttl_seconds, 300);

        // Clean up
        env::remove_var("ZION_API_URL");
        env::remove_var("ZION_API_KEY");
    }
}
