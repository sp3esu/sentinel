//! Cache module
//!
//! Provides Redis-based caching for user limits and JWT validation.

pub mod redis;

pub use self::redis::RedisCache;
