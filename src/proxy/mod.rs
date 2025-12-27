//! Proxy module
//!
//! Handles request forwarding to upstream AI providers.

pub mod openai;
pub mod vercel_gateway;

pub use openai::OpenAIClient;
pub use vercel_gateway::VercelGateway;
