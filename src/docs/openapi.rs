//! OpenAPI specification for the Native API
//!
//! Aggregates all Native API endpoints and schemas into a single OpenAPI document.

use utoipa::{
    openapi::security::{Http, HttpAuthScheme, SecurityScheme},
    Modify, OpenApi,
};

use crate::native::{
    error::{NativeError, NativeErrorResponse},
    request::{ChatCompletionRequest, StopSequence},
    response::{
        ChatCompletionResponse, Choice, ChoiceMessage, Delta, StreamChoice, StreamChunk,
        ToolCallDelta, ToolCallFunctionDelta, Usage,
    },
    types::{
        Content, ContentPart, FunctionDefinition, ImageUrl, Message, Role, Tier, ToolCall,
        ToolCallFunction, ToolChoice, ToolDefinition, ToolResult, ToolResultContent,
    },
};

/// OpenAPI specification for the Sentinel Native API
#[derive(OpenApi)]
#[openapi(
    info(
        title = "Sentinel Native API",
        version = "1.0.0",
        description = "Native API for Sentinel AI Proxy - unified format with tier routing and session management"
    ),
    paths(
        crate::native_routes::chat::native_chat_completions
    ),
    components(
        schemas(
            // Types
            Role,
            ImageUrl,
            ContentPart,
            Content,
            Message,
            Tier,
            FunctionDefinition,
            ToolDefinition,
            ToolCallFunction,
            ToolCall,
            ToolResultContent,
            ToolResult,
            ToolChoice,
            // Request
            StopSequence,
            ChatCompletionRequest,
            // Response
            Usage,
            ChoiceMessage,
            Choice,
            ChatCompletionResponse,
            ToolCallFunctionDelta,
            ToolCallDelta,
            Delta,
            StreamChoice,
            StreamChunk,
            // Error
            NativeError,
            NativeErrorResponse,
        )
    ),
    modifiers(&SecurityAddon),
    tags(
        (name = "Chat", description = "Chat completion endpoints")
    )
)]
pub struct NativeApiDoc;

/// Security scheme addon for Bearer JWT authentication
struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "bearer_auth",
                SecurityScheme::Http(Http::new(HttpAuthScheme::Bearer)),
            );
        }
    }
}
