# Cache Configuration for Sentinel
# Bypasses cache for API endpoints to ensure real-time responses

# =============================================================================
# Cache Settings
# =============================================================================

resource "cloudflare_zone_setting" "cache_level" {
  zone_id    = local.zone_id
  setting_id = "cache_level"
  value      = "aggressive"
}

resource "cloudflare_zone_setting" "browser_cache_ttl" {
  zone_id    = local.zone_id
  setting_id = "browser_cache_ttl"
  value      = var.browser_ttl
}

# =============================================================================
# Cache Rules
# =============================================================================

resource "cloudflare_ruleset" "cache_rules" {
  zone_id     = local.zone_id
  name        = "Sentinel Cache Rules"
  description = "Cache bypass rules for Sentinel API"
  kind        = "zone"
  phase       = "http_request_cache_settings"

  # Bypass cache for all API endpoints
  rules {
    action = "set_cache_settings"
    action_parameters {
      cache = false
    }
    expression  = "(starts_with(http.request.uri.path, \"/v1/\"))"
    description = "Bypass cache for API endpoints"
    enabled     = true
  }

  # Bypass cache for health endpoints
  rules {
    action = "set_cache_settings"
    action_parameters {
      cache = false
    }
    expression  = "(starts_with(http.request.uri.path, \"/health\"))"
    description = "Bypass cache for health endpoints"
    enabled     = true
  }

  # Bypass cache for metrics endpoint
  rules {
    action = "set_cache_settings"
    action_parameters {
      cache = false
    }
    expression  = "(http.request.uri.path eq \"/metrics\")"
    description = "Bypass cache for metrics endpoint"
    enabled     = true
  }
}
