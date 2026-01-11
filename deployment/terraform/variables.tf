# Cloudflare Terraform Variables for Sentinel
# See terraform.tfvars.example for usage

# =============================================================================
# Required Variables
# =============================================================================

variable "cloudflare_api_token" {
  description = "Cloudflare API token with Zone, DNS, SSL, and WAF permissions"
  type        = string
  sensitive   = true
}

variable "cloudflare_zone_id" {
  description = "Cloudflare Zone ID for mindsmith.app"
  type        = string
}

variable "cloudflare_account_id" {
  description = "Cloudflare Account ID"
  type        = string
}

variable "origin_server_ip" {
  description = "IP address of the origin VPS server"
  type        = string
}

# =============================================================================
# Optional Variables with Defaults
# =============================================================================

variable "subdomain" {
  description = "Subdomain for Sentinel (e.g., 'sentinel' for sentinel.mindsmith.app)"
  type        = string
  default     = "sentinel"
}

variable "ssl_mode" {
  description = "SSL/TLS encryption mode (off, flexible, full, strict)"
  type        = string
  default     = "strict"

  validation {
    condition     = contains(["off", "flexible", "full", "strict"], var.ssl_mode)
    error_message = "SSL mode must be one of: off, flexible, full, strict"
  }
}

variable "min_tls_version" {
  description = "Minimum TLS version"
  type        = string
  default     = "1.2"

  validation {
    condition     = contains(["1.0", "1.1", "1.2", "1.3"], var.min_tls_version)
    error_message = "TLS version must be one of: 1.0, 1.1, 1.2, 1.3"
  }
}

variable "origin_cert_validity_days" {
  description = "Validity period for origin certificate in days (max 5475 = 15 years)"
  type        = number
  default     = 5475
}

variable "rate_limit_requests_per_minute" {
  description = "Maximum requests per minute for rate limiting"
  type        = number
  default     = 100
}

variable "enable_websockets" {
  description = "Enable WebSocket support"
  type        = bool
  default     = true
}

variable "enable_http2" {
  description = "Enable HTTP/2 support"
  type        = bool
  default     = true
}

variable "enable_http3" {
  description = "Enable HTTP/3 (QUIC) support"
  type        = bool
  default     = true
}

variable "browser_ttl" {
  description = "Browser cache TTL in seconds (0 = respect origin headers)"
  type        = number
  default     = 0
}
