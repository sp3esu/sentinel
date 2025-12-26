//! Mock infrastructure for testing external services
//!
//! This module provides mock servers and test helpers for external dependencies:
//! - Zion API (user limits and authentication)
//! - Vercel AI Gateway (chat completions, models)
//! - Redis (caching)
//!
//! All mocks are designed to be reusable across different test files and support
//! various response scenarios (success, errors, edge cases).

pub mod redis;
pub mod vercel_gateway;
pub mod zion;

pub use redis::*;
pub use vercel_gateway::*;
pub use zion::*;
