# Zion Integration

This document describes how Sentinel integrates with the Zion governance system for user authentication and usage tracking.

## Overview

Zion is an external governance system that manages:
- User accounts and authentication
- Usage limits and quotas
- Subscription tiers

Sentinel acts as a consumer of Zion's APIs to:
1. Validate user JWTs
2. Check user limits before processing requests
3. Track usage after requests complete

## Authentication

### JWT Passthrough

Sentinel uses Zion JWT passthrough authentication:

```
┌────────┐     ┌─────────┐     ┌──────────┐     ┌──────────────┐
│ Client │────▶│ Sentinel│────▶│ Zion API │────▶│ User Profile │
└────────┘     └─────────┘     └──────────┘     └──────────────┘
     │              │                │                   │
     │  JWT Token   │                │                   │
     │─────────────▶│                │                   │
     │              │  Validate JWT  │                   │
     │              │───────────────▶│                   │
     │              │                │  User Data        │
     │              │◀───────────────│◀──────────────────│
     │              │                │                   │
```

### Flow

1. Client authenticates with Zion and receives a JWT
2. Client includes JWT in `Authorization: Bearer <token>` header
3. Sentinel hashes the JWT and checks Redis cache
4. On cache miss, validates via `GET /api/v1/users/me`
5. Caches validated user profile for `JWT_CACHE_TTL_SECONDS` (default 5 minutes)
6. Extracts `external_id` for subsequent Zion API calls

### Caching

JWT validation results are cached in Redis:

```
Key:   sentinel:profile:{sha256(token)}
Value: {"id": "...", "email": "...", "externalId": "...", ...}
TTL:   JWT_CACHE_TTL_SECONDS (default 300)
```

## Usage Limits

### Limit Types

Sentinel expects these limits to be configured in Zion:

| Limit Name | Description | Unit |
|------------|-------------|------|
| `ai_input_tokens` | Tokens in prompts/inputs | tokens |
| `ai_output_tokens` | Tokens in completions/outputs | tokens |
| `ai_requests` | Total API request count | requests |

### Limit Structure

```json
{
  "limitId": "clx123abc",
  "name": "ai_input_tokens",
  "displayName": "AI Input Tokens",
  "unit": "tokens",
  "limit": 100000,
  "used": 45000,
  "remaining": 55000,
  "resetPeriod": "MONTHLY",
  "periodStart": "2024-01-01T00:00:00Z",
  "periodEnd": "2024-01-31T23:59:59Z"
}
```

### Reset Periods

- `DAILY` - Resets at midnight UTC
- `WEEKLY` - Resets on Monday at midnight UTC
- `MONTHLY` - Resets on the 1st of each month
- `NEVER` - Lifetime limit, never resets

## API Endpoints

### Get User Limits

```
GET /api/v1/limits/external/{externalId}
x-api-key: <ZION_API_KEY>
```

Response:
```json
{
  "success": true,
  "data": {
    "userId": "user123",
    "externalId": "ext456",
    "limits": [
      {
        "limitId": "...",
        "name": "ai_input_tokens",
        "limit": 100000,
        "used": 45000,
        "remaining": 55000,
        ...
      }
    ]
  }
}
```

### Increment Usage

```
POST /api/v1/usage/external/increment
x-api-key: <ZION_API_KEY>
Content-Type: application/json

{
  "externalId": "ext456",
  "limitName": "ai_input_tokens",
  "amount": 1500
}
```

Response:
```json
{
  "success": true,
  "data": {
    "limitId": "...",
    "name": "ai_input_tokens",
    "limit": 100000,
    "used": 46500,
    "remaining": 53500,
    ...
  }
}
```

### Validate JWT

```
GET /api/v1/users/me
Authorization: Bearer <zion-jwt>
```

Response:
```json
{
  "success": true,
  "data": {
    "id": "user123",
    "email": "user@example.com",
    "name": "John Doe",
    "externalId": "ext456",
    "emailVerified": true,
    "createdAt": "2024-01-01T00:00:00Z",
    "lastLoginAt": "2024-01-15T10:30:00Z"
  }
}
```

## Caching Strategy

### User Limits

Limits are cached in Redis to reduce load on Zion:

```
Key:   sentinel:limits:{externalId}
Value: [{"name": "ai_input_tokens", ...}, ...]
TTL:   CACHE_TTL_SECONDS (default 300)
```

### Cache Invalidation

- **TTL-based**: Limits expire after `CACHE_TTL_SECONDS`
- **On update**: After incrementing usage, the cached limit is updated locally
- **Webhook (future)**: Zion can push updates via webhook

## Error Handling

### Zion API Errors

| HTTP Status | Sentinel Behavior |
|-------------|-------------------|
| 401 | Return 401 Unauthorized to client |
| 404 | Return 404 Not Found (user not found) |
| 429 | Retry with backoff, or return 503 |
| 5xx | Return 502 Bad Gateway |

### Fallback Behavior

If Zion is unavailable:
1. Use cached limits if available (even if expired)
2. Log warning and allow request with monitoring
3. Return 503 Service Unavailable if no cache exists

## Configuration

### Environment Variables

```bash
# Zion API base URL
ZION_API_URL=https://api.zion.example.com

# API key for external endpoints (x-api-key header)
ZION_API_KEY=your-api-key-here

# Cache TTL for user limits (seconds)
CACHE_TTL_SECONDS=300

# Cache TTL for JWT validation (seconds)
JWT_CACHE_TTL_SECONDS=300
```

## Zion Setup

### Creating Limits

In your Zion admin panel, create the following limits:

1. **AI Input Tokens**
   - Name: `ai_input_tokens`
   - Unit: `tokens`
   - Default limit: Based on subscription tier

2. **AI Output Tokens**
   - Name: `ai_output_tokens`
   - Unit: `tokens`
   - Default limit: Based on subscription tier

3. **AI Requests**
   - Name: `ai_requests`
   - Unit: `requests`
   - Default limit: Based on subscription tier

### Subscription Tiers

Example tier configuration:

| Tier | Input Tokens | Output Tokens | Requests |
|------|--------------|---------------|----------|
| Free | 10,000/month | 5,000/month | 100/month |
| Pro | 100,000/month | 50,000/month | 1,000/month |
| Enterprise | Unlimited | Unlimited | Unlimited |

## Troubleshooting

### Common Issues

**JWT validation failing**
- Check that `ZION_API_URL` is correct
- Verify JWT is not expired
- Check Zion API logs for errors

**Limits not updating**
- Check Redis connectivity
- Verify `ZION_API_KEY` has correct permissions
- Check cache TTL configuration

**High latency**
- Increase cache TTL if acceptable
- Check network latency to Zion API
- Monitor Redis performance

### Debug Logging

Enable debug logging to see Zion API interactions:

```bash
RUST_LOG=sentinel::zion=debug,sentinel::cache=debug cargo run
```
