# API Documentation Access

Sentinel provides interactive API documentation via Swagger UI for the Native API endpoints.

## Endpoints

| Endpoint | Description |
|----------|-------------|
| `/native/docs` | Swagger UI - interactive API explorer |
| `/native/docs/openapi.json` | Raw OpenAPI 3.x specification |

## Development Mode

When `DOCS_API_KEY` is **not set**, documentation is publicly accessible:

```bash
# View Swagger UI in browser
open http://localhost:8080/native/docs

# Fetch OpenAPI spec
curl http://localhost:8080/native/docs/openapi.json
```

## Production Mode

When `DOCS_API_KEY` is set, all documentation endpoints require the `X-Docs-Key` header.
Requests without a valid key receive a `404 Not Found` response (to hide endpoint existence).

### Configuration

Set the API key in your environment:

```bash
export DOCS_API_KEY=your-secret-docs-key
```

Or add to `.env`:

```
DOCS_API_KEY=your-secret-docs-key
```

### Accessing Protected Docs

Include the `X-Docs-Key` header in requests:

```bash
# Fetch OpenAPI spec
curl -H "X-Docs-Key: your-secret-docs-key" \
  http://localhost:8080/native/docs/openapi.json

# Download spec to file
curl -H "X-Docs-Key: your-secret-docs-key" \
  http://localhost:8080/native/docs/openapi.json > openapi.json
```

### Browser Access

To view Swagger UI in production, you can use a browser extension to add the `X-Docs-Key` header, or use tools like:

- [ModHeader](https://modheader.com/) (Chrome/Firefox)
- [Requestly](https://requestly.io/) (Chrome/Firefox)

Configure the extension to add:
- Header name: `X-Docs-Key`
- Header value: Your `DOCS_API_KEY` value

## Security Notes

- The docs endpoint returns `404` (not `401` or `403`) when unauthorized to avoid revealing endpoint existence
- Use a strong, unique key for `DOCS_API_KEY` in production
- Consider restricting documentation access to internal networks in sensitive environments
