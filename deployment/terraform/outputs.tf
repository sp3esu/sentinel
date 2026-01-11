# Terraform Outputs for Sentinel Cloudflare Configuration

# =============================================================================
# Zone Information
# =============================================================================

output "zone_id" {
  description = "Cloudflare Zone ID"
  value       = local.zone_id
}

output "zone_name" {
  description = "Zone domain name"
  value       = local.zone_name
}

# =============================================================================
# DNS Information
# =============================================================================

output "sentinel_fqdn" {
  description = "Fully qualified domain name for Sentinel"
  value       = local.fqdn
}

output "dns_record_id" {
  description = "ID of the Sentinel DNS A record"
  value       = cloudflare_dns_record.sentinel.id
}

# =============================================================================
# SSL/TLS Certificate Information
# =============================================================================

output "origin_certificate" {
  description = "Origin CA certificate (PEM format) - save to /etc/ssl/cloudflare/origin.pem"
  value       = cloudflare_origin_ca_certificate.sentinel.certificate
  sensitive   = true
}

output "origin_private_key" {
  description = "Origin CA private key (PEM format) - save to /etc/ssl/cloudflare/origin.key"
  value       = tls_private_key.origin.private_key_pem
  sensitive   = true
}

output "origin_certificate_expiry" {
  description = "Origin certificate expiration date"
  value       = cloudflare_origin_ca_certificate.sentinel.expires_on
}

output "origin_certificate_id" {
  description = "Origin certificate ID (for revocation if needed)"
  value       = cloudflare_origin_ca_certificate.sentinel.id
}

# =============================================================================
# Configuration Summary
# =============================================================================

output "configuration_summary" {
  description = "Summary of applied configuration"
  value = {
    domain              = local.fqdn
    ssl_mode            = var.ssl_mode
    min_tls_version     = var.min_tls_version
    websockets_enabled  = var.enable_websockets
    http2_enabled       = var.enable_http2
    http3_enabled       = var.enable_http3
    rate_limit          = "${var.rate_limit_requests_per_minute} req/min"
  }
}

# =============================================================================
# Instructions
# =============================================================================

output "next_steps" {
  description = "Instructions for completing the setup"
  value       = <<-EOT

    ============================================================
    Cloudflare configuration applied successfully!
    ============================================================

    Next steps:

    1. Save the origin certificate to your VPS:
       terraform output -raw origin_certificate > origin.pem
       terraform output -raw origin_private_key > origin.key

       Then copy to VPS:
       scp origin.pem origin.key deploy@your-vps:/etc/ssl/cloudflare/

    2. Set correct permissions on VPS:
       chmod 644 /etc/ssl/cloudflare/origin.pem
       chmod 600 /etc/ssl/cloudflare/origin.key

    3. Configure Nginx to use the certificate (see nginx-sentinel.conf)

    4. Restart Nginx:
       sudo systemctl reload nginx

    5. Test the endpoint:
       curl -I https://${local.fqdn}/health

    Certificate expires: ${cloudflare_origin_ca_certificate.sentinel.expires_on}

  EOT
}
