//! Native API types for Sentinel
//!
//! This module defines the canonical message format that all providers translate to/from.
//! Types are designed to be OpenAI-compatible for seamless integration with existing clients.

pub mod error;
pub mod request;
pub mod response;
pub mod session;
pub mod streaming;
pub mod translate;
pub mod types;

// Re-export key types for convenience
pub use request::{ChatCompletionRequest, StopSequence};
pub use response::{
    ChatCompletionResponse, Choice, ChoiceMessage, Delta, StreamChoice, StreamChunk, Usage,
};
pub use session::{Session, SessionManager};
pub use types::{Content, ContentPart, ImageUrl, Message, Role};
