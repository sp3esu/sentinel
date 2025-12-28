//! Mock infrastructure for testing external services
//!
//! This module provides mock servers and test helpers for external dependencies:
//! - Zion API (user limits and authentication)
//! - OpenAI API (chat completions, models)
//! - Redis (caching)
//!
//! All mocks are designed to be reusable across different test files and support
//! various response scenarios (success, errors, edge cases).

pub mod openai;
pub mod redis;
pub mod zion;

pub use openai::*;
pub use redis::*;
pub use zion::*;
