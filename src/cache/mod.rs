//! Cache module
//!
//! Provides caching for user limits and JWT validation.
//! Supports Redis-based caching for production and in-memory caching for testing.

pub mod redis;
pub mod subscription;

#[cfg(any(test, feature = "test-utils"))]
mod in_memory;

pub use self::redis::RedisCache;
pub use self::subscription::SubscriptionCache;

#[cfg(any(test, feature = "test-utils"))]
pub use self::in_memory::InMemoryCache;
