# Zion Batch Increment Endpoint Specification

## Overview

This document specifies a new batch increment endpoint for Zion that allows Sentinel to report multiple usage increments in a single API call, dramatically reducing the number of HTTP requests between services.

## Current State

Sentinel currently makes 3 individual API calls per user request:
```
POST /api/v1/usage/external/increment {externalId: "A", limitName: "ai_input_tokens", amount: 100}
POST /api/v1/usage/external/increment {externalId: "A", limitName: "ai_output_tokens", amount: 50}
POST /api/v1/usage/external/increment {externalId: "A", limitName: "ai_requests", amount: 1}
```

With batching enabled in Sentinel, increments are aggregated but still sent individually due to lack of a batch endpoint.

## Proposed Endpoint

### `POST /api/v1/usage/external/batch-increment`

Increment multiple usage limits in a single atomic operation.

### Request

**Headers:**
```
Content-Type: application/json
x-api-key: <external-api-key>
```

**Body:**
```json
{
  "increments": [
    {
      "externalId": "user-external-id-1",
      "limitName": "ai_input_tokens",
      "amount": 100
    },
    {
      "externalId": "user-external-id-1",
      "limitName": "ai_output_tokens",
      "amount": 50
    },
    {
      "externalId": "user-external-id-1",
      "limitName": "ai_requests",
      "amount": 1
    },
    {
      "externalId": "user-external-id-2",
      "limitName": "ai_input_tokens",
      "amount": 200
    }
  ]
}
```

**Fields:**

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `increments` | Array | Yes | List of increment operations |
| `increments[].externalId` | String | Yes | External ID of the user |
| `increments[].limitName` | String | Yes | Name of the limit to increment |
| `increments[].amount` | Integer | No | Amount to increment (default: 1) |

### Response

**Success (200 OK):**
```json
{
  "success": true,
  "data": {
    "processed": 4,
    "failed": 0,
    "results": [
      {
        "externalId": "user-external-id-1",
        "limitName": "ai_input_tokens",
        "success": true,
        "newValue": 1100,
        "limit": 10000
      },
      {
        "externalId": "user-external-id-1",
        "limitName": "ai_output_tokens",
        "success": true,
        "newValue": 550,
        "limit": 50000
      },
      {
        "externalId": "user-external-id-1",
        "limitName": "ai_requests",
        "success": true,
        "newValue": 101,
        "limit": 1000
      },
      {
        "externalId": "user-external-id-2",
        "limitName": "ai_input_tokens",
        "success": true,
        "newValue": 200,
        "limit": 10000
      }
    ]
  }
}
```

**Partial Success (200 OK with failures):**
```json
{
  "success": true,
  "data": {
    "processed": 4,
    "failed": 1,
    "results": [
      {
        "externalId": "user-external-id-1",
        "limitName": "ai_input_tokens",
        "success": true,
        "newValue": 1100,
        "limit": 10000
      },
      {
        "externalId": "invalid-user",
        "limitName": "ai_input_tokens",
        "success": false,
        "error": "User not found"
      }
    ]
  }
}
```

**Error Responses:**

| Status | Description |
|--------|-------------|
| 400 | Invalid request body |
| 401 | Missing or invalid API key |
| 413 | Too many increments (max 1000 per request) |
| 500 | Internal server error |

### Implementation Notes

1. **Atomicity**: Each increment should be atomic, but the batch as a whole doesn't need to be transactional. If one increment fails, others should still succeed.

2. **Ordering**: Increments are processed in array order. If the same (externalId, limitName) pair appears multiple times, amounts are cumulative.

3. **Rate Limiting**: This endpoint should have a higher rate limit than the individual increment endpoint since it replaces multiple calls.

4. **Validation**:
   - Reject if `increments` array is empty
   - Reject if `increments` has more than 1000 items
   - Validate each increment individually

5. **Limit Checking**: The endpoint should check limits but NOT reject if limits are exceeded. Usage tracking is about recording what happened, not preventing it. Sentinel handles limit enforcement separately.

6. **Performance**: Consider using database batching (e.g., `INSERT ... ON CONFLICT DO UPDATE` for PostgreSQL) to minimize database round trips.

## Expected Traffic Reduction

| Sentinel Traffic | Without Batch | With Batch | Reduction |
|-----------------|---------------|------------|-----------|
| 100 req/s | 300 calls/s | ~2 calls/s | 99.3% |
| 1,000 req/s | 3,000 calls/s | ~20 calls/s | 99.3% |
| 10,000 req/s | 30,000 calls/s | ~20 calls/s | 99.9% |

Note: Reduction improves at higher traffic because Sentinel's batching aggregates more increments per flush.

## Sentinel Integration

Once the batch endpoint is available, update Sentinel's `BatchingUsageTracker` to use it:

```rust
// In src/zion/client.rs
impl ZionClient {
    pub async fn batch_increment_usage(
        &self,
        increments: &[BatchIncrementItem],
    ) -> AppResult<BatchIncrementResponse> {
        let url = format!("{}/api/v1/usage/external/batch-increment", self.base_url);

        let request = BatchIncrementRequest { increments };

        let response = self.client
            .post(&url)
            .headers(self.api_key_headers())
            .json(&request)
            .send()
            .await?;

        // Handle response...
    }
}
```

## Timeline

1. **Phase 1** (Current): Sentinel batches locally, sends individual calls rate-limited
2. **Phase 2**: Zion implements batch endpoint
3. **Phase 3**: Sentinel uses batch endpoint for maximum efficiency
