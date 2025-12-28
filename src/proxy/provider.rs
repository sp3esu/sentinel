//! AI Provider abstraction layer
//!
//! Defines the trait interface for AI providers (OpenAI, Anthropic, etc.)
//! to enable pluggable backend support.

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{HeaderMap, Method, Response};
use bytes::Bytes;
use futures::Stream;
use serde::{de::DeserializeOwned, Serialize};
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
    async fn chat_completions<T, R>(&self, request: &T, incoming_headers: &HeaderMap) -> AppResult<R>
    where
        T: Serialize + Send + Sync,
        R: DeserializeOwned;

    /// Chat completions (streaming)
    ///
    /// Sends a chat completion request and returns a stream of response chunks.
    async fn chat_completions_stream<T>(
        &self,
        request: &T,
        incoming_headers: &HeaderMap,
    ) -> AppResult<ByteStream>
    where
        T: Serialize + Send + Sync;

    /// Text completions (non-streaming) - legacy endpoint
    ///
    /// Sends a text completion request and returns the full response.
    async fn completions<T, R>(&self, request: &T, incoming_headers: &HeaderMap) -> AppResult<R>
    where
        T: Serialize + Send + Sync,
        R: DeserializeOwned;

    /// Text completions (streaming) - legacy endpoint
    ///
    /// Sends a text completion request and returns a stream of response chunks.
    async fn completions_stream<T>(
        &self,
        request: &T,
        incoming_headers: &HeaderMap,
    ) -> AppResult<ByteStream>
    where
        T: Serialize + Send + Sync;

    /// Embeddings
    ///
    /// Generates embeddings for the given input.
    async fn embeddings<T, R>(&self, request: &T, incoming_headers: &HeaderMap) -> AppResult<R>
    where
        T: Serialize + Send + Sync,
        R: DeserializeOwned;

    /// List available models
    ///
    /// Returns a list of models available from this provider.
    async fn list_models<R>(&self) -> AppResult<R>
    where
        R: DeserializeOwned;

    /// Get a specific model by ID
    ///
    /// Returns details about a specific model.
    async fn get_model<R>(&self, model_id: &str) -> AppResult<R>
    where
        R: DeserializeOwned;

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
