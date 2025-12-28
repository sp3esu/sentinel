//! Proxy module
//!
//! Handles request forwarding to upstream AI providers.
//!
//! This module provides a generic abstraction layer for AI providers,
//! allowing easy switching between different backends (OpenAI, Anthropic, etc.)

pub mod headers;
pub mod logging;
pub mod openai;
pub mod provider;

pub use headers::{build_default_headers, build_proxy_headers, is_hop_by_hop_header, SAFE_HEADERS_TO_FORWARD};
pub use logging::RequestContext;
pub use openai::{OpenAIClient, OpenAIProvider};
pub use provider::{AiProvider, ByteStream};
