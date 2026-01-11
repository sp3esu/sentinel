# DNS Configuration for Sentinel
# Creates A record for sentinel.mindsmith.app

# =============================================================================
# DNS Records
# =============================================================================

# Primary A record for Sentinel API
resource "cloudflare_dns_record" "sentinel" {
  zone_id = local.zone_id
  name    = var.subdomain
  content = var.origin_server_ip
  type    = "A"
  proxied = true  # Enable Cloudflare proxy (orange cloud)
  ttl     = 1     # Auto TTL when proxied
  comment = "Sentinel AI Proxy - managed by Terraform"
}
