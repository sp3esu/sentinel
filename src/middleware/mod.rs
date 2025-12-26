//! Middleware module
//!
//! Contains Tower middleware for authentication and rate limiting.

pub mod auth;
pub mod rate_limiter;

pub use auth::{auth_middleware, AuthenticatedUser};
pub use rate_limiter::{
    check_rate_limit, increment_rate_limit, rate_limit_exceeded_response, rate_limit_middleware,
    RateLimitConfig, RateLimitResult,
};
