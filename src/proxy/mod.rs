//! Proxy module
//!
//! Handles request forwarding to upstream AI providers.

pub mod vercel_gateway;

pub use vercel_gateway::VercelGateway;
