//! Native API chat completions endpoint
//!
//! Handles chat completion requests in the unified Native API format.
//! Supports both streaming and non-streaming responses.
//! Uses tier routing for model selection based on complexity.

use std::sync::Arc;

use axum::{
    body::Body,
    extract::State,
    http::{header, HeaderMap, HeaderValue, StatusCode},
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

/// Model selection result with tier for session storage
struct ModelSelection {
    provider: String,
    model: String,
    tier: Tier,
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
///   "tier": "simple",           // Optional, defaults to simple
///   "messages": [...],          // Required
///   "stream": false,            // Optional, defaults to false
///   "temperature": 0.7,         // Optional
///   "max_tokens": 1000          // Optional
/// }
/// ```
///
/// # Tier Routing
///
/// - `simple`: Fast, cheap models (e.g., gpt-4o-mini)
/// - `moderate`: Balanced models (e.g., gpt-4o)
/// - `complex`: Most capable models (e.g., gpt-4o with higher cost)
///
/// Within a session (conversation_id provided):
/// - Tier can only be upgraded (simple -> moderate -> complex)
/// - Downgrades are silently ignored (uses session tier)
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

    // Resolve model selection based on session and tier
    let selection = resolve_model_selection(&state, &native_request, requested_tier, &user)
        .await?;

    let is_streaming = native_request.stream;

    info!(
        model = %selection.model,
        provider = %selection.provider,
        tier = %selection.tier,
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
        handle_streaming(state, &headers, provider_request, selection, user).await
    } else {
        handle_non_streaming(state, &headers, provider_request, selection, user, translator).await
    }
}

/// Resolve model selection based on session and tier
///
/// Handles:
/// - Existing session lookup with tier upgrade logic
/// - New session creation with tier routing
/// - Stateless mode (no session)
async fn resolve_model_selection(
    state: &Arc<AppState>,
    request: &ChatCompletionRequest,
    requested_tier: Tier,
    user: &AuthenticatedUser,
) -> Result<ModelSelection, NativeErrorResponse> {
    if let Some(ref conv_id) = request.conversation_id {
        // Try to get existing session
        if let Some(session) = state.session_manager.get(conv_id).await.map_err(|e| {
            NativeErrorResponse::internal(format!("Session lookup failed: {}", e))
        })? {
            // Refresh TTL on activity (fire-and-forget, log errors)
            if let Err(e) = state.session_manager.touch(conv_id).await {
                warn!(conversation_id = %conv_id, error = %e, "Failed to refresh session TTL");
            }

            // Check if tier upgrade is needed
            if session.tier.can_upgrade_to(&requested_tier) && requested_tier > session.tier {
                // Tier upgrade: select new model for higher tier
                let selected = state
                    .tier_router
                    .select_model(requested_tier, Some(&session.provider))
                    .await
                    .map_err(NativeErrorResponse::from_app_error)?;

                // Update session with new tier/model
                state
                    .session_manager
                    .upgrade_tier(conv_id, &selected.provider, &selected.model, requested_tier)
                    .await
                    .map_err(|e| {
                        NativeErrorResponse::internal(format!("Session upgrade failed: {}", e))
                    })?;

                info!(
                    conversation_id = %conv_id,
                    old_tier = %session.tier,
                    new_tier = %requested_tier,
                    model = %selected.model,
                    "Session tier upgraded"
                );

                return Ok(ModelSelection {
                    provider: selected.provider,
                    model: selected.model,
                    tier: requested_tier,
                });
            }

            // No upgrade needed - use existing session model
            debug!(
                conversation_id = %conv_id,
                session_tier = %session.tier,
                requested_tier = %requested_tier,
                model = %session.model,
                "Using session model (no tier upgrade)"
            );

            return Ok(ModelSelection {
                provider: session.provider,
                model: session.model,
                tier: session.tier,
            });
        }

        // Session expired or never existed - create new session
        let selected = state
            .tier_router
            .select_model(requested_tier, None)
            .await
            .map_err(NativeErrorResponse::from_app_error)?;

        // Store new session
        state
            .session_manager
            .create(
                conv_id,
                &selected.provider,
                &selected.model,
                requested_tier,
                &user.external_id,
            )
            .await
            .map_err(|e| NativeErrorResponse::internal(format!("Session creation failed: {}", e)))?;

        info!(
            conversation_id = %conv_id,
            model = %selected.model,
            tier = %requested_tier,
            "Created new session with tier routing"
        );

        return Ok(ModelSelection {
            provider: selected.provider,
            model: selected.model,
            tier: requested_tier,
        });
    }

    // No conversation_id - stateless mode, fresh selection each time
    let selected = state
        .tier_router
        .select_model(requested_tier, None)
        .await
        .map_err(NativeErrorResponse::from_app_error)?;

    debug!(
        model = %selected.model,
        tier = %requested_tier,
        "Stateless model selection"
    );

    Ok(ModelSelection {
        provider: selected.provider,
        model: selected.model,
        tier: requested_tier,
    })
}

/// Handle non-streaming chat completion
async fn handle_non_streaming(
    state: Arc<AppState>,
    headers: &HeaderMap,
    provider_request: serde_json::Value,
    selection: ModelSelection,
    user: AuthenticatedUser,
    translator: OpenAITranslator,
) -> Result<Response, NativeErrorResponse> {
    // Try primary request with retry on failure
    let (native_response, final_model, _final_provider) = match execute_with_retry(
        &state,
        headers,
        provider_request,
        &selection,
        &translator,
    )
    .await
    {
        Ok(result) => result,
        Err(e) => return Err(e),
    };

    // Track usage
    let input_tokens = native_response.usage.prompt_tokens as u64;
    let output_tokens = native_response.usage.completion_tokens as u64;

    state.batching_tracker.track(
        user.email.clone(),
        input_tokens,
        output_tokens,
        Some(final_model.clone()),
    );

    info!(
        model = %final_model,
        input_tokens = input_tokens,
        output_tokens = output_tokens,
        external_id = %user.external_id,
        "Native chat completion completed"
    );

    // Build response with custom headers
    let mut response = Json(native_response).into_response();
    add_sentinel_headers(response.headers_mut(), &final_model, selection.tier);

    Ok(response)
}

/// Execute request with single retry on provider failure
async fn execute_with_retry(
    state: &Arc<AppState>,
    headers: &HeaderMap,
    provider_request: serde_json::Value,
    selection: &ModelSelection,
    translator: &OpenAITranslator,
) -> Result<(crate::native::response::ChatCompletionResponse, String, String), NativeErrorResponse>
{
    // Try primary model
    match state
        .ai_provider
        .chat_completions(provider_request.clone(), headers)
        .await
    {
        Ok(provider_response) => {
            // Record success
            state
                .tier_router
                .record_success(&selection.provider, &selection.model);

            let native_response = translator
                .translate_response(provider_response)
                .map_err(|e| {
                    NativeErrorResponse::internal(format!("Response translation failed: {}", e))
                })?;

            return Ok((
                native_response,
                selection.model.clone(),
                selection.provider.clone(),
            ));
        }
        Err(e) => {
            // Record failure
            state
                .tier_router
                .record_failure(&selection.provider, &selection.model);

            warn!(
                model = %selection.model,
                provider = %selection.provider,
                error = %e,
                "Primary model failed, attempting retry"
            );

            // Try to get alternative model for retry
            let retry_model = state
                .tier_router
                .get_retry_model(selection.tier, &selection.model)
                .await
                .map_err(NativeErrorResponse::from_app_error)?;

            match retry_model {
                Some(alternative) => {
                    info!(
                        original_model = %selection.model,
                        retry_model = %alternative.model,
                        "Retrying with alternative model"
                    );

                    // Retry with alternative model
                    match state
                        .ai_provider
                        .chat_completions(provider_request, headers)
                        .await
                    {
                        Ok(provider_response) => {
                            state
                                .tier_router
                                .record_success(&alternative.provider, &alternative.model);

                            let native_response = translator
                                .translate_response(provider_response)
                                .map_err(|e| {
                                    NativeErrorResponse::internal(format!(
                                        "Response translation failed: {}",
                                        e
                                    ))
                                })?;

                            return Ok((
                                native_response,
                                alternative.model.clone(),
                                alternative.provider.clone(),
                            ));
                        }
                        Err(retry_err) => {
                            state
                                .tier_router
                                .record_failure(&alternative.provider, &alternative.model);

                            warn!(
                                retry_model = %alternative.model,
                                error = %retry_err,
                                "Retry also failed"
                            );

                            return Err(NativeErrorResponse::provider_error(
                                format!("All models failed: {}", retry_err),
                                &alternative.provider,
                            ));
                        }
                    }
                }
                None => {
                    // No alternative available
                    return Err(NativeErrorResponse::provider_error(
                        e.to_string(),
                        &selection.provider,
                    ));
                }
            }
        }
    }
}

/// Add X-Sentinel-Model and X-Sentinel-Tier headers to response
fn add_sentinel_headers(headers: &mut HeaderMap, model: &str, tier: Tier) {
    if let Ok(value) = HeaderValue::from_str(model) {
        headers.insert("X-Sentinel-Model", value);
    }
    if let Ok(value) = HeaderValue::from_str(&tier.to_string()) {
        headers.insert("X-Sentinel-Tier", value);
    }
}

/// Handle streaming chat completion
///
/// Note: For streaming, retry is only possible BEFORE any chunks are sent.
/// Once streaming starts, we fail fast without retry.
async fn handle_streaming(
    state: Arc<AppState>,
    headers: &HeaderMap,
    mut provider_request: serde_json::Value,
    selection: ModelSelection,
    user: AuthenticatedUser,
) -> Result<Response, NativeErrorResponse> {
    // Inject stream_options.include_usage: true to get token counts from OpenAI
    // This is critical for accurate usage tracking
    provider_request["stream_options"] = json!({
        "include_usage": true
    });

    // Forward streaming request to provider
    // Note: No retry after streaming starts - would cause duplicate partial responses
    let stream = match state
        .ai_provider
        .chat_completions_stream(provider_request.clone(), headers)
        .await
    {
        Ok(stream) => {
            state
                .tier_router
                .record_success(&selection.provider, &selection.model);
            stream
        }
        Err(e) => {
            state
                .tier_router
                .record_failure(&selection.provider, &selection.model);

            warn!(
                model = %selection.model,
                provider = %selection.provider,
                error = %e,
                "Streaming request failed (no retry for streaming)"
            );

            return Err(NativeErrorResponse::provider_error(
                e.to_string(),
                &selection.provider,
            ));
        }
    };

    // Clone values for the stream closure
    let model_clone = selection.model.clone();
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
    let model_for_parse_error = selection.model.clone();

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
    let model_for_metrics = selection.model.clone();
    let model_for_counting = selection.model.clone();
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

    // Build SSE response with custom headers
    let body = Body::from_stream(final_stream);

    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .header(header::CONNECTION, "keep-alive")
        .header("X-Accel-Buffering", "no")
        .header("X-Sentinel-Model", &selection.model)
        .header("X-Sentinel-Tier", selection.tier.to_string())
        .body(body)
        .map_err(|e| NativeErrorResponse::internal(format!("Failed to build response: {}", e)))?;

    info!(
        model = %selection.model,
        tier = %selection.tier,
        external_id = %user.external_id,
        "Native streaming chat started"
    );

    Ok(response)
}
