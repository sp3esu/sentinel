# Vercel AI Gateway Integration

This document describes how Sentinel integrates with Vercel AI Gateway to route requests to LLM providers.

## Overview

Vercel AI Gateway is a unified API gateway that provides:
- Access to multiple LLM providers (OpenAI, Anthropic, etc.)
- Request routing and load balancing
- Usage analytics and monitoring

Sentinel uses Vercel AI Gateway as its upstream provider for AI completions.

## Architecture

```
┌────────┐     ┌─────────┐     ┌──────────────────┐     ┌────────┐
│ Client │────▶│ Sentinel│────▶│ Vercel AI Gateway│────▶│ OpenAI │
└────────┘     └─────────┘     └──────────────────┘     └────────┘
                    │                    │
                    │   Authorization    │
                    │   Bearer <key>     │
                    │───────────────────▶│
                    │                    │
                    │   OpenAI-format    │
                    │   Request/Response │
                    │◀──────────────────▶│
```

## Configuration

### Environment Variables

```bash
# Vercel AI Gateway URL
VERCEL_AI_GATEWAY_URL=https://gateway.ai.vercel.com/v1

# API key for authentication
VERCEL_AI_GATEWAY_API_KEY=your-vercel-key
```

## API Endpoints

Sentinel proxies these endpoints to Vercel AI Gateway:

### Chat Completions

```
POST /v1/chat/completions
```

**Request:**
```json
{
  "model": "gpt-4",
  "messages": [
    {"role": "system", "content": "You are a helpful assistant."},
    {"role": "user", "content": "Hello!"}
  ],
  "max_tokens": 100,
  "temperature": 0.7,
  "stream": false
}
```

**Response:**
```json
{
  "id": "chatcmpl-abc123",
  "object": "chat.completion",
  "created": 1705312800,
  "model": "gpt-4",
  "choices": [
    {
      "index": 0,
      "message": {
        "role": "assistant",
        "content": "Hello! How can I help you today?"
      },
      "finish_reason": "stop"
    }
  ],
  "usage": {
    "prompt_tokens": 20,
    "completion_tokens": 10,
    "total_tokens": 30
  }
}
```

### Completions (Legacy)

```
POST /v1/completions
```

**Request:**
```json
{
  "model": "gpt-3.5-turbo-instruct",
  "prompt": "Say hello",
  "max_tokens": 50
}
```

### Models

```
GET /v1/models
GET /v1/models/{model_id}
```

**Response:**
```json
{
  "object": "list",
  "data": [
    {
      "id": "gpt-4",
      "object": "model",
      "created": 1687882410,
      "owned_by": "openai"
    }
  ]
}
```

## Streaming

### Server-Sent Events (SSE)

For streaming requests (`"stream": true`), responses are sent as SSE:

```
data: {"id":"chatcmpl-abc","choices":[{"delta":{"content":"Hello"}}]}

data: {"id":"chatcmpl-abc","choices":[{"delta":{"content":"!"}}]}

data: {"id":"chatcmpl-abc","choices":[{"delta":{}}],"usage":{"prompt_tokens":10,"completion_tokens":2}}

data: [DONE]
```

### Usage in Streaming

To get token usage in streaming responses, Sentinel adds `stream_options`:

```json
{
  "model": "gpt-4",
  "messages": [...],
  "stream": true,
  "stream_options": {
    "include_usage": true
  }
}
```

The usage appears in the final chunk before `[DONE]`.

## Token Counting

### Pre-Request

Before forwarding, Sentinel estimates prompt tokens:

```rust
// Using tiktoken-rs
let prompt_tokens = counter.count_messages(&messages, model)?;
```

### Post-Response

After receiving the response:

1. **Non-streaming**: Use `usage` field directly
2. **Streaming**: Parse final chunk for `usage` field
3. **Fallback**: Count completion tokens locally

## Request Transformation

Sentinel may modify requests before forwarding:

### Headers

```
Authorization: Bearer <VERCEL_AI_GATEWAY_API_KEY>
Content-Type: application/json
X-Request-ID: <uuid>
```

### Body Modifications

- Adds `stream_options.include_usage` for streaming requests
- Preserves all other fields unchanged

## Error Handling

### Gateway Errors

| HTTP Status | Description | Sentinel Action |
|-------------|-------------|-----------------|
| 400 | Bad request | Return 400 to client |
| 401 | Invalid API key | Return 502, log error |
| 429 | Rate limited | Return 429 to client |
| 500 | Server error | Return 502, retry once |
| 503 | Service unavailable | Return 503, retry with backoff |

### Timeouts

- Default timeout: 300 seconds (for long completions)
- Configurable per request type
- Streaming requests use chunked timeout

## Model Mapping

Sentinel transparently passes model names to the gateway:

| Client Request | Gateway Request |
|----------------|-----------------|
| `gpt-4` | `gpt-4` |
| `gpt-3.5-turbo` | `gpt-3.5-turbo` |
| `claude-3-opus` | `claude-3-opus` |

The gateway handles routing to the appropriate provider.

## Connection Pooling

Sentinel maintains a connection pool to the gateway:

```rust
let http_client = reqwest::Client::builder()
    .pool_max_idle_per_host(100)
    .timeout(Duration::from_secs(300))
    .build()?;
```

## Monitoring

### Metrics

Sentinel records these metrics for gateway requests:

- `sentinel_requests_total{endpoint="chat_completions", status="success|error"}`
- `sentinel_request_duration_seconds{endpoint="chat_completions"}`
- `sentinel_tokens_processed_total{type="input|output"}`

### Logging

Enable debug logging for gateway interactions:

```bash
RUST_LOG=sentinel::proxy=debug cargo run
```

## Fallback Models

If the requested model is unavailable, Sentinel can:
1. Return the error from the gateway (default)
2. Fall back to an alternative model (configurable)
3. Return a cached response (for simple queries)

## Security

### API Key Protection

- API key is never logged
- Key is only sent to verified gateway URL
- Key is stored in environment, not code

### Request Validation

Before forwarding:
- Validate JSON structure
- Check model is in allowed list (if configured)
- Validate message format

## Troubleshooting

### Common Issues

**401 Unauthorized from Gateway**
- Verify `VERCEL_AI_GATEWAY_API_KEY` is correct
- Check key hasn't expired
- Ensure key has required permissions

**Timeouts**
- Increase timeout for long completions
- Check network connectivity
- Monitor gateway status page

**Malformed Responses**
- Check gateway version compatibility
- Verify model supports requested features
- Enable debug logging for response inspection

### Debug Commands

```bash
# Test gateway connectivity
curl -H "Authorization: Bearer $VERCEL_AI_GATEWAY_API_KEY" \
  https://gateway.ai.vercel.com/v1/models

# Check Sentinel logs
docker-compose logs -f sentinel | grep "vercel"
```
