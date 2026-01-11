# SSL/TLS Configuration for Sentinel
# Configures Full (Strict) mode and generates origin certificate

# =============================================================================
# Zone Settings for SSL/TLS
# =============================================================================

resource "cloudflare_zone_setting" "ssl" {
  zone_id    = local.zone_id
  setting_id = "ssl"
  value      = var.ssl_mode
}

resource "cloudflare_zone_setting" "min_tls_version" {
  zone_id    = local.zone_id
  setting_id = "min_tls_version"
  value      = var.min_tls_version
}

resource "cloudflare_zone_setting" "tls_1_3" {
  zone_id    = local.zone_id
  setting_id = "tls_1_3"
  value      = "on"
}

resource "cloudflare_zone_setting" "automatic_https_rewrites" {
  zone_id    = local.zone_id
  setting_id = "automatic_https_rewrites"
  value      = "on"
}

resource "cloudflare_zone_setting" "always_use_https" {
  zone_id    = local.zone_id
  setting_id = "always_use_https"
  value      = "on"
}

# =============================================================================
# Origin Certificate
# =============================================================================

# Generate private key for origin certificate
resource "tls_private_key" "origin" {
  algorithm = "RSA"
  rsa_bits  = 2048
}

# Generate CSR for origin certificate
resource "tls_cert_request" "origin" {
  private_key_pem = tls_private_key.origin.private_key_pem

  subject {
    common_name  = local.fqdn
    organization = "Sentinel"
  }

  dns_names = [
    local.fqdn,
    "*.${local.fqdn}"
  ]
}

# Request Cloudflare Origin CA certificate
resource "cloudflare_origin_ca_certificate" "sentinel" {
  csr                = tls_cert_request.origin.cert_request_pem
  hostnames          = [local.fqdn]
  request_type       = "origin-rsa"
  requested_validity = var.origin_cert_validity_days
}

# =============================================================================
# Network Settings
# =============================================================================

resource "cloudflare_zone_setting" "websockets" {
  zone_id    = local.zone_id
  setting_id = "websockets"
  value      = var.enable_websockets ? "on" : "off"
}

resource "cloudflare_zone_setting" "http2" {
  zone_id    = local.zone_id
  setting_id = "http2"
  value      = var.enable_http2 ? "on" : "off"
}

resource "cloudflare_zone_setting" "http3" {
  zone_id    = local.zone_id
  setting_id = "http3"
  value      = var.enable_http3 ? "on" : "off"
}
