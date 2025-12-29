//! Integration tests entry point for Sentinel API endpoints
//!
//! This file serves as the integration test entry point.
//! Run these tests using `cargo test --test integration_tests`.

mod common;
mod integration;
mod mocks;

// Tests are defined within the integration module:
// - integration/health.rs - Health endpoint tests
// - integration/models.rs - Models endpoint tests
// - integration/chat_completions.rs - Chat completions endpoint tests
// - integration/token_tracking.rs - Token tracking blackbox tests
