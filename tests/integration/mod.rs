//! Integration tests for the Sentinel AI Proxy
//!
//! This module contains integration tests that verify the complete request/response
//! flow through the proxy, including authentication, rate limiting, and AI provider
//! interactions.

pub mod chat_completions;
pub mod health;
pub mod models;
pub mod rate_limiting;
pub mod token_estimation_accuracy;
pub mod token_tracking;
