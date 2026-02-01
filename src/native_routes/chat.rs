//! Native API chat completions endpoint
//!
//! Handles chat completion requests in the unified Native API format.
//! Supports both streaming and non-streaming responses.

use std::sync::Arc;

use axum::{
    body::Body,
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use futures::StreamExt;
use serde_json::json;
use tracing::{debug, info, warn};

use crate::{
    middleware::auth::AuthenticatedUser,
    native::{
        error::NativeErrorResponse,
        request::ChatCompletionRequest,
        translate::{MessageTranslator, OpenAITranslator},
        types::Tier,
    },
    streaming::SseLineBuffer,
    AppState,
};

/// Usage statistics from stream chunks
#[derive(Debug, Clone, serde::Deserialize, Default)]
struct StreamUsage {
    #[serde(default)]
    prompt_tokens: u32,
    #[serde(default)]
    completion_tokens: u32,
    #[serde(default)]
    total_tokens: u32,
}

/// Streaming chunk for parsing content and usage
#[derive(Debug, Clone, serde::Deserialize)]
struct StreamChunk {
    #[serde(default)]
    choices: Vec<StreamChoice>,
    #[serde(default)]
    usage: Option<StreamUsage>,
}

/// Streaming choice with delta
#[derive(Debug, Clone, serde::Deserialize, Default)]
struct StreamChoice {
    #[serde(default)]
    delta: StreamDelta,
}

/// Streaming delta content
#[derive(Debug, Clone, serde::Deserialize, Default)]
struct StreamDelta {
    #[serde(default)]
    content: Option<String>,
}

/// Temporary: Map tier to default model until tier routing is implemented
/// This will be replaced by TierRouter in Phase 4 Plan 02/03
fn tier_to_default_model(tier: Tier) -> &'static str {
    match tier {
        Tier::Simple => "gpt-4o-mini",
        Tier::Moderate => "gpt-4o",
        Tier::Complex => "gpt-4o",
    }
}

/// Handle native chat completion requests
///
/// Accepts requests in Native API format, translates to OpenAI format,
/// and returns responses in Native API format.
///
/// # Request Format
///
/// ```json
/// {
///   "tier": "simple",           // Optional, defaults to simple. Phase 4 tier routing.
///   "messages": [...],          // Required
///   "stream": false,            // Optional, defaults to false
///   "temperature": 0.7,         // Optional
///   "max_tokens": 1000          // Optional
/// }
/// ```
pub async fn native_chat_completions(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    request: axum::extract::Request,
) -> Result<Response, NativeErrorResponse> {
    // Extract authenticated user from request extensions (set by auth middleware)
    let user = request
        .extensions()
        .get::<AuthenticatedUser>()
        .cloned()
        .ok_or_else(|| {
            NativeErrorResponse::internal("AuthenticatedUser not found in request extensions")
        })?;

    // Read request body
    let body = axum::body::to_bytes(request.into_body(), usize::MAX)
        .await
        .map_err(|e| NativeErrorResponse::internal(format!("Failed to read request body: {}", e)))?;

    // Parse as ChatCompletionRequest
    let native_request: ChatCompletionRequest = serde_json::from_slice(&body).map_err(|e| {
        NativeErrorResponse::validation(format!("Invalid request body: {}", e))
    })?;

    // Determine tier from request (default to Simple)
    let requested_tier = native_request.tier.unwrap_or_default();

    // Session handling: use stored provider/model or create new session
    // TODO: Phase 4 Plan 02/03 will add tier upgrade logic here
    let (provider, model) = if let Some(ref conv_id) = native_request.conversation_id {
        // Try to get existing session
        if let Some(session) = state.session_manager.get(conv_id).await.map_err(|e| {
            NativeErrorResponse::internal(format!("Session lookup failed: {}", e))
        })? {
            // Refresh TTL on activity (fire-and-forget, log errors)
            if let Err(e) = state.session_manager.touch(conv_id).await {
                warn!(conversation_id = %conv_id, error = %e, "Failed to refresh session TTL");
            }

            // Log tier info for debugging
            debug!(
                conversation_id = %conv_id,
                session_model = %session.model,
                requested_tier = ?requested_tier,
                "Using session model (tier upgrade logic in Phase 4 Plan 02/03)"
            );

            (session.provider, session.model)
        } else {
            // Session expired or never existed - create new session
            // Phase 4: Use tier to select model (currently hardcoded mapping)
            // TierRouter will replace this in Plan 02/03
            let model = tier_to_default_model(requested_tier).to_string();
            let provider = "openai".to_string();

            // Store new session
            state.session_manager.create(conv_id, &provider, &model, &user.external_id)
                .await
                .map_err(|e| NativeErrorResponse::internal(format!("Session creation failed: {}", e)))?;

            info!(
                conversation_id = %conv_id,
                model = %model,
                tier = ?requested_tier,
                "Created new session with tier-based model"
            );
            (provider, model)
        }
    } else {
        // No conversation_id - fresh selection each time (stateless mode)
        // Phase 4: Use tier to select model (currently hardcoded mapping)
        let model = tier_to_default_model(requested_tier).to_string();
        ("openai".to_string(), model)
    };

    let is_streaming = native_request.stream;

    info!(
        model = %model,
        provider = %provider,
        stream = %is_streaming,
        messages = %native_request.messages.len(),
        external_id = %user.external_id,
        conversation_id = ?native_request.conversation_id,
        "Processing native chat completion request"
    );

    // Translate request using OpenAI translator
    let translator = OpenAITranslator::new();
    let provider_request = translator
        .translate_request(&native_request)
        .map_err(|e| NativeErrorResponse::validation(e.to_string()))?;

    if is_streaming {
        handle_streaming(state, &headers, provider_request, model, user).await
    } else {
        handle_non_streaming(state, &headers, provider_request, model, user, translator).await
    }
}

/// Handle non-streaming chat completion
async fn handle_non_streaming(
    state: Arc<AppState>,
    headers: &HeaderMap,
    provider_request: serde_json::Value,
    model: String,
    user: AuthenticatedUser,
    translator: OpenAITranslator,
) -> Result<Response, NativeErrorResponse> {
    // Forward to OpenAI provider
    let provider_response = state
        .ai_provider
        .chat_completions(provider_request, headers)
        .await
        .map_err(|e| NativeErrorResponse::provider_error(e.to_string(), "openai"))?;

    // Translate response back to Native format
    let native_response = translator
        .translate_response(provider_response)
        .map_err(|e| NativeErrorResponse::internal(format!("Response translation failed: {}", e)))?;

    // Track usage
    let input_tokens = native_response.usage.prompt_tokens as u64;
    let output_tokens = native_response.usage.completion_tokens as u64;

    state.batching_tracker.track(
        user.email.clone(),
        input_tokens,
        output_tokens,
        Some(model.clone()),
    );

    info!(
        model = %model,
        input_tokens = input_tokens,
        output_tokens = output_tokens,
        external_id = %user.external_id,
        "Native chat completion completed"
    );

    Ok(Json(native_response).into_response())
}

/// Handle streaming chat completion
async fn handle_streaming(
    state: Arc<AppState>,
    headers: &HeaderMap,
    mut provider_request: serde_json::Value,
    model: String,
    user: AuthenticatedUser,
) -> Result<Response, NativeErrorResponse> {
    // Inject stream_options.include_usage: true to get token counts from OpenAI
    // This is critical for accurate usage tracking
    provider_request["stream_options"] = json!({
        "include_usage": true
    });

    // Forward streaming request to provider
    let stream = state
        .ai_provider
        .chat_completions_stream(provider_request, headers)
        .await
        .map_err(|e| NativeErrorResponse::provider_error(e.to_string(), "openai"))?;

    // Clone values for the stream closure
    let model_clone = model.clone();
    let tracker = state.batching_tracker.clone();
    let user_email = user.email.clone();
    let token_counter = state.token_counter.clone();

    // Track accumulated usage from stream
    let usage_accumulator = std::sync::Arc::new(std::sync::Mutex::new(StreamUsage::default()));
    let usage_for_stream = usage_accumulator.clone();

    // Track accumulated content for token counting fallback
    let content_accumulator = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let content_for_stream = content_accumulator.clone();

    // Buffer for accumulating incomplete SSE lines across chunk boundaries
    let line_buffer = std::sync::Arc::new(std::sync::Mutex::new(SseLineBuffer::new()));
    let line_buffer_for_stream = line_buffer.clone();

    // Clone model for metrics in stream closure
    let model_for_parse_error = model.clone();

    // Wrap the stream to extract content and usage from chunks
    // Since our Native API format is OpenAI-compatible, chunks pass through with minimal transformation
    let tracked_stream = stream.map(move |chunk| {
        match chunk {
            Ok(bytes) => {
                // Use line buffer to handle chunks split across network boundaries
                let complete_lines = line_buffer_for_stream.lock().unwrap().feed(&bytes);

                for line in complete_lines {
                    if let Some(json_str) = line.strip_prefix("data: ") {
                        let json_str = json_str.trim();
                        if json_str != "[DONE]" {
                            match serde_json::from_str::<StreamChunk>(json_str) {
                                Ok(chunk) => {
                                    // Accumulate content from delta
                                    if let Some(choice) = chunk.choices.first() {
                                        if let Some(ref content) = choice.delta.content {
                                            content_for_stream.lock().unwrap().push_str(content);
                                        }
                                    }
                                    // Capture usage if provided (usually in final chunk)
                                    if let Some(usage) = chunk.usage {
                                        let mut acc = usage_for_stream.lock().unwrap();
                                        acc.prompt_tokens = usage.prompt_tokens;
                                        acc.completion_tokens = usage.completion_tokens;
                                        acc.total_tokens = usage.total_tokens;
                                    }
                                }
                                Err(e) => {
                                    warn!(
                                        error = %e,
                                        sse_line = %if json_str.len() > 500 { &json_str[..500] } else { json_str },
                                        line_len = json_str.len(),
                                        model = %model_for_parse_error,
                                        "Failed to parse complete SSE line in native streaming"
                                    );
                                }
                            }
                        }
                    }
                }
                Ok(bytes)
            }
            Err(e) => {
                warn!(model = %model_clone, error = %e, "Stream error in native chat");
                Err(e)
            }
        }
    });

    // Create a stream that tracks usage after completion
    let user_email_final = user_email.clone();
    let model_for_metrics = model.clone();
    let model_for_counting = model.clone();
    let usage_final = usage_accumulator.clone();
    let content_final = content_accumulator.clone();
    let tracker_final = tracker.clone();

    let final_stream = async_stream::stream! {
        futures::pin_mut!(tracked_stream);
        while let Some(item) = tracked_stream.next().await {
            yield item;
        }

        // Stream completed - determine token counts
        let openai_usage = usage_final.lock().unwrap().clone();
        let accumulated_content = content_final.lock().unwrap().clone();

        // Prefer OpenAI usage if available, otherwise estimate
        let (input_tokens, output_tokens) = if openai_usage.prompt_tokens > 0 || openai_usage.completion_tokens > 0 {
            debug!(
                actual_input = openai_usage.prompt_tokens,
                actual_output = openai_usage.completion_tokens,
                model = %model_for_metrics,
                "Native streaming usage from OpenAI"
            );
            (openai_usage.prompt_tokens as u64, openai_usage.completion_tokens as u64)
        } else {
            // Fallback to estimation - OpenAI didn't return usage field
            let estimated_output = token_counter
                .count_tokens(&model_for_counting, &accumulated_content)
                .unwrap_or(0) as u64;
            warn!(
                model = %model_for_counting,
                estimated_output = estimated_output,
                content_len = accumulated_content.len(),
                "Using estimated token counts - OpenAI didn't return usage field"
            );
            (0, estimated_output)
        };

        // Track usage in Zion (fire-and-forget)
        tracker_final.track(user_email_final.clone(), input_tokens, output_tokens, Some(model_for_metrics.clone()));

        info!(
            model = %model_for_metrics,
            input_tokens = input_tokens,
            output_tokens = output_tokens,
            email = %user_email_final,
            "Native streaming usage tracked"
        );
    };

    // Build SSE response
    let body = Body::from_stream(final_stream);

    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .header(header::CONNECTION, "keep-alive")
        .header("X-Accel-Buffering", "no")
        .body(body)
        .map_err(|e| NativeErrorResponse::internal(format!("Failed to build response: {}", e)))?;

    info!(
        model = %model,
        external_id = %user.external_id,
        "Native streaming chat started"
    );

    Ok(response)
}
