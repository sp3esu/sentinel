//! Test entry point for mock infrastructure
//!
//! This file serves as the integration test entry point for the mock infrastructure.
//! It allows running tests within the mocks module using `cargo test --test mocks_test`.

mod mocks;

// The tests are defined within each mock module (zion.rs, vercel_gateway.rs, redis.rs)
// and are automatically run when this test file is compiled.
