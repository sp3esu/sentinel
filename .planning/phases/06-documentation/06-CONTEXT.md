# Phase 6: Documentation - Context

**Gathered:** 2026-02-01
**Status:** Ready for planning

<domain>
## Phase Boundary

Provide an OpenAPI specification for the Native API with protected access via dedicated API key. Includes Swagger UI for interactive exploration. Does not include documentation for existing /v1/* endpoints or external documentation sites.

</domain>

<decisions>
## Implementation Decisions

### Spec generation approach
- Use utoipa annotations (derive macros on types/handlers)
- Generate spec at runtime, always matches running code
- Also export static spec file to `docs/openapi.json` for CI/tooling (SDK generation, linting)

### Documentation content
- Comprehensive endpoint descriptions: full behavior notes, edge cases, usage guidance
- Include realistic request/response examples for each endpoint
- Document all error codes (400, 401, 403, 429, 500, 503) with examples per endpoint
- Comprehensive authentication documentation: JWT requirements, header format, examples, error handling

### Docs endpoint behavior
- GET /native/docs serves Swagger UI for interactive exploration
- GET /native/docs/openapi.json returns raw OpenAPI spec as JSON
- JSON only, no content negotiation
- Docs endpoints excluded from the spec itself (spec describes API functionality only)

### API key protection
- Header: `X-Docs-Key`
- Key configured via `DOCS_API_KEY` environment variable
- Optional in dev: skip key check when `DOCS_API_KEY` not set
- Return 404 (not 401/403) when key missing/invalid — hide endpoint existence from unauthorized access

### Claude's Discretion
- Swagger UI library choice (utoipa-swagger-ui or similar)
- Static file generation approach (build script vs cargo xtask)
- Internal code organization for documentation module

</decisions>

<specifics>
## Specific Ideas

- Security through obscurity: docs endpoint returns 404 when unauthorized to prevent discovery
- Dev-friendly: no key needed locally when env var unset

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 06-documentation*
*Context gathered: 2026-02-01*
