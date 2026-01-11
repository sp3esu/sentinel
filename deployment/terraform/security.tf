# Security Configuration for Sentinel
# WAF rules, rate limiting, and security settings

# =============================================================================
# Security Settings
# =============================================================================

resource "cloudflare_zone_setting" "security_level" {
  zone_id    = local.zone_id
  setting_id = "security_level"
  value      = "medium"
}

resource "cloudflare_zone_setting" "browser_check" {
  zone_id    = local.zone_id
  setting_id = "browser_check"
  value      = "on"
}

resource "cloudflare_zone_setting" "challenge_ttl" {
  zone_id    = local.zone_id
  setting_id = "challenge_ttl"
  value      = "1800"  # 30 minutes
}

# =============================================================================
# WAF Managed Rulesets
# =============================================================================

# Deploy Cloudflare Managed Ruleset
resource "cloudflare_ruleset" "waf_managed" {
  zone_id     = local.zone_id
  name        = "Sentinel WAF Managed Rules"
  description = "Deploy Cloudflare managed WAF rulesets"
  kind        = "zone"
  phase       = "http_request_firewall_managed"

  # Cloudflare Managed Ruleset
  rules {
    action = "execute"
    action_parameters {
      id = "efb7b8c949ac4650a09736fc376e9aee"  # Cloudflare Managed Ruleset
    }
    expression  = "true"
    description = "Execute Cloudflare Managed Ruleset"
    enabled     = true
  }

  # OWASP Core Ruleset
  rules {
    action = "execute"
    action_parameters {
      id = "4814384a9e5d4991b9815dcfc25d2f1f"  # Cloudflare OWASP Core Ruleset
    }
    expression  = "true"
    description = "Execute OWASP Core Ruleset"
    enabled     = true
  }
}

# =============================================================================
# Custom WAF Rules
# =============================================================================

resource "cloudflare_ruleset" "waf_custom" {
  zone_id     = local.zone_id
  name        = "Sentinel Custom WAF Rules"
  description = "Custom security rules for Sentinel API"
  kind        = "zone"
  phase       = "http_request_firewall_custom"

  # Block requests to API endpoints without Authorization header
  rules {
    action      = "block"
    expression  = "(starts_with(http.request.uri.path, \"/v1/\")) and (not any(http.request.headers[\"authorization\"][*] ne \"\"))"
    description = "Block API requests without Authorization header"
    enabled     = true
  }

  # Block common attack patterns
  rules {
    action      = "block"
    expression  = "(http.request.uri.query contains \"<script\") or (http.request.uri.query contains \"javascript:\") or (http.request.uri.query contains \"onerror=\")"
    description = "Block XSS patterns in query strings"
    enabled     = true
  }

  # Block SQL injection patterns
  rules {
    action      = "block"
    expression  = "(http.request.uri.query contains \"UNION SELECT\") or (http.request.uri.query contains \"' OR '\") or (http.request.uri.query contains \"1=1\")"
    description = "Block SQL injection patterns"
    enabled     = true
  }
}

# =============================================================================
# Rate Limiting
# =============================================================================

resource "cloudflare_ruleset" "rate_limiting" {
  zone_id     = local.zone_id
  name        = "Sentinel Rate Limiting"
  description = "Rate limiting rules for Sentinel API"
  kind        = "zone"
  phase       = "http_ratelimit"

  # General API rate limit
  rules {
    action = "block"
    action_parameters {
      response {
        status_code  = 429
        content      = "{\"error\":\"Rate limit exceeded. Please slow down.\"}"
        content_type = "application/json"
      }
    }
    ratelimit {
      characteristics     = ["cf.colo.id", "ip.src"]
      period              = 60
      requests_per_period = var.rate_limit_requests_per_minute
      mitigation_timeout  = 60
    }
    expression  = "(starts_with(http.request.uri.path, \"/v1/\"))"
    description = "Rate limit API endpoints (${var.rate_limit_requests_per_minute} req/min)"
    enabled     = true
  }
}

# =============================================================================
# Bot Management (Basic)
# =============================================================================

resource "cloudflare_zone_setting" "bot_management" {
  zone_id    = local.zone_id
  setting_id = "bot_management"
  value = jsonencode({
    enable_js                          = true
    fight_mode                         = true
    optimize_wordpress                 = false
    sbfm_definitely_automated          = "block"
    sbfm_likely_automated              = "managed_challenge"
    sbfm_verified_bots                 = "allow"
    sbfm_static_resource_protection    = false
    suppress_session_score             = false
  })
}
