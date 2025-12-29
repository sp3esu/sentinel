//! AI Provider abstraction layer
//!
//! Defines the trait interface for AI providers (OpenAI, Anthropic, etc.)
//! to enable pluggable backend support.

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{HeaderMap, Method, Response};
use bytes::Bytes;
use futures::Stream;
use std::pin::Pin;

use crate::error::AppResult;

/// Stream type for streaming responses from AI providers
pub type ByteStream = Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>;

/// Trait defining the interface for AI providers
///
/// Implementations of this trait handle communication with specific AI backends
/// (OpenAI, Anthropic, Azure, etc.) while maintaining a consistent interface
/// for the rest of the application.
///
/// # Design Note
///
/// This trait uses `serde_json::Value` instead of generics to be dyn-compatible,
/// allowing `Arc<dyn AiProvider>` for runtime polymorphism. Callers should
/// serialize their typed requests to `Value` before calling these methods,
/// and deserialize the `Value` responses to their typed structs.
///
/// # Security
///
/// Implementations MUST:
/// - Never forward client Authorization headers to upstream providers
/// - Use provider-specific API keys from configuration
/// - Filter headers using the whitelist in `headers.rs`
#[async_trait]
pub trait AiProvider: Send + Sync {
    /// Get the provider name for logging and metrics
    fn name(&self) -> &'static str;

    /// Chat completions (non-streaming)
    ///
    /// Sends a chat completion request and returns the full response.
    async fn chat_completions(
        &self,
        request: serde_json::Value,
        incoming_headers: &HeaderMap,
    ) -> AppResult<serde_json::Value>;

    /// Chat completions (streaming)
    ///
    /// Sends a chat completion request and returns a stream of response chunks.
    async fn chat_completions_stream(
        &self,
        request: serde_json::Value,
        incoming_headers: &HeaderMap,
    ) -> AppResult<ByteStream>;

    /// Text completions (non-streaming) - legacy endpoint
    ///
    /// Sends a text completion request and returns the full response.
    async fn completions(
        &self,
        request: serde_json::Value,
        incoming_headers: &HeaderMap,
    ) -> AppResult<serde_json::Value>;

    /// Text completions (streaming) - legacy endpoint
    ///
    /// Sends a text completion request and returns a stream of response chunks.
    async fn completions_stream(
        &self,
        request: serde_json::Value,
        incoming_headers: &HeaderMap,
    ) -> AppResult<ByteStream>;

    /// Embeddings
    ///
    /// Generates embeddings for the given input.
    async fn embeddings(
        &self,
        request: serde_json::Value,
        incoming_headers: &HeaderMap,
    ) -> AppResult<serde_json::Value>;

    /// List available models
    ///
    /// Returns a list of models available from this provider.
    async fn list_models(&self) -> AppResult<serde_json::Value>;

    /// Get a specific model by ID
    ///
    /// Returns details about a specific model.
    async fn get_model(&self, model_id: &str) -> AppResult<serde_json::Value>;

    /// Responses API (non-streaming)
    ///
    /// Sends a responses request and returns the full response.
    /// Used for OpenAI's newer Responses API.
    async fn responses(
        &self,
        request: serde_json::Value,
        incoming_headers: &HeaderMap,
    ) -> AppResult<serde_json::Value>;

    /// Responses API (streaming)
    ///
    /// Sends a responses request and returns a stream of response chunks.
    async fn responses_stream(
        &self,
        request: serde_json::Value,
        incoming_headers: &HeaderMap,
    ) -> AppResult<ByteStream>;

    /// Forward a raw request (pass-through)
    ///
    /// Forwards an arbitrary request to the provider's API.
    /// Used for endpoints not explicitly supported by typed methods.
    async fn forward_raw(
        &self,
        method: Method,
        path: &str,
        incoming_headers: HeaderMap,
        body: Body,
    ) -> AppResult<Response<Body>>;
}
