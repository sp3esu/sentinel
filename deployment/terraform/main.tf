# Cloudflare Terraform Configuration for Sentinel
# https://sentinel.mindsmith.app

terraform {
  required_version = ">= 1.0"

  required_providers {
    cloudflare = {
      source  = "cloudflare/cloudflare"
      version = "~> 5.0"
    }
    tls = {
      source  = "hashicorp/tls"
      version = "~> 4.0"
    }
  }
}

# =============================================================================
# Provider Configuration
# =============================================================================

provider "cloudflare" {
  api_token = var.cloudflare_api_token
}

# =============================================================================
# Data Sources
# =============================================================================

# Fetch zone details
data "cloudflare_zone" "main" {
  zone_id = var.cloudflare_zone_id
}

# =============================================================================
# Local Values
# =============================================================================

locals {
  zone_id     = var.cloudflare_zone_id
  zone_name   = data.cloudflare_zone.main.name
  fqdn        = "${var.subdomain}.${local.zone_name}"
  account_id  = var.cloudflare_account_id
}
