# Phase 4: Tier Routing - Context

**Gathered:** 2026-02-01
**Status:** Ready for planning

<domain>
## Phase Boundary

Map complexity tiers (simple | moderate | complex) to specific models based on configuration from Zion. The API accepts a tier, and Sentinel selects the appropriate model considering cost, availability, and session stickiness. Only OpenAI is wired for v1.

</domain>

<decisions>
## Implementation Decisions

### Tier naming & semantics
- Fixed tier names in code: `simple`, `moderate`, `complex` as enum
- Native API accepts tier only — model field removed entirely
- Default tier is `simple` if client doesn't specify
- Observability: X-Sentinel-Model response header + Prometheus metrics for tier usage and model selection counts

### Provider selection logic
- Weighted selection by cost when multiple providers can serve a tier (probabilistic, favoring cheaper)
- v1 resolves to specific OpenAI models per tier (e.g., simple→gpt-4o-mini, moderate→gpt-4o)
- Model-level session stickiness: session locks to specific model, not just provider
- Tier upgrade only within session: can go simple→moderate→complex, but not downgrade

### Zion config structure
- New dedicated endpoint: `GET /api/v1/tiers/config`
- Cache with 30-minute TTL (longer than subscription cache since config changes infrequently)
- Global config — one tier mapping for all users
- Cost data includes both relative (1-10 scale for selection) and actual token pricing (for reporting)

### Fallback behavior
- If Zion unavailable and cache empty: return 503 error (fail explicit)
- Active health checks to detect provider/model availability
- Retry once with next model in tier, then fail if that also fails
- Exponential backoff for unavailable providers: start 30s, double on failure, max 5min, reset on success

### Claude's Discretion
- Exact health check implementation (frequency, timeout, endpoints to probe)
- Prometheus metric names and labels
- Specific default model mappings for v1 fallback (gpt-4o-mini, gpt-4o, etc.)
- Weight calculation formula for probabilistic selection

</decisions>

<specifics>
## Specific Ideas

- Session model upgrade but not downgrade keeps conversations consistent while allowing escalation
- Weighted selection by cost means cheaper models get more traffic but system still distributes load

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 04-tier-routing*
*Context gathered: 2026-02-01*
