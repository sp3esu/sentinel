# Cloudflare Terraform Configuration for Sentinel

This directory contains Terraform configuration for managing Cloudflare settings for the Sentinel AI Proxy.

## What This Configures

| Resource | Description |
|----------|-------------|
| DNS | A record for `sentinel.mindsmith.app` (proxied) |
| SSL/TLS | Full (Strict) mode, TLS 1.2 minimum, origin certificate |
| WAF | Cloudflare Managed + OWASP Core rulesets |
| Rate Limiting | 100 requests/minute per IP |
| Cache | Bypass for `/v1/*`, `/health`, `/metrics` |
| Network | WebSockets, HTTP/2, HTTP/3 enabled |

## Prerequisites

1. **Cloudflare Account** with `mindsmith.app` zone
2. **Terraform** >= 1.0 installed
3. **API Token** with these permissions:
   - Zone:Read, Zone:Edit
   - DNS:Read, DNS:Edit
   - SSL and Certificates:Read, SSL and Certificates:Edit
   - Firewall Services:Read, Firewall Services:Edit
   - Zone WAF:Read, Zone WAF:Edit

### Create API Token

1. Go to [Cloudflare Dashboard > Profile > API Tokens](https://dash.cloudflare.com/profile/api-tokens)
2. Click "Create Token"
3. Use "Edit zone DNS" template as a starting point
4. Add additional permissions listed above
5. Restrict to `mindsmith.app` zone

## Quick Start

```bash
# 1. Copy example variables
cp terraform.tfvars.example terraform.tfvars

# 2. Edit with your values
# - cloudflare_api_token
# - cloudflare_zone_id
# - cloudflare_account_id
# - origin_server_ip

# 3. Initialize Terraform
terraform init

# 4. Preview changes
terraform plan

# 5. Apply configuration
terraform apply
```

## Outputs

After applying, retrieve important outputs:

```bash
# View all outputs
terraform output

# Export origin certificate
terraform output -raw origin_certificate > origin.pem
terraform output -raw origin_private_key > origin.key

# View configuration summary
terraform output configuration_summary
```

## Deploy Certificate to VPS

```bash
# Copy certificates to VPS
scp origin.pem origin.key deploy@your-vps:/tmp/

# On VPS: Move to correct location
ssh deploy@your-vps
sudo mkdir -p /etc/ssl/cloudflare
sudo mv /tmp/origin.pem /etc/ssl/cloudflare/
sudo mv /tmp/origin.key /etc/ssl/cloudflare/
sudo chmod 644 /etc/ssl/cloudflare/origin.pem
sudo chmod 600 /etc/ssl/cloudflare/origin.key
sudo chown root:root /etc/ssl/cloudflare/*

# Reload Nginx
sudo nginx -t && sudo systemctl reload nginx
```

## File Structure

```
terraform/
├── main.tf           # Provider config, zone data source
├── dns.tf            # DNS A record
├── ssl.tf            # SSL settings, origin certificate
├── security.tf       # WAF rules, rate limiting
├── cache.tf          # Cache bypass rules
├── variables.tf      # Input variable definitions
├── outputs.tf        # Output values
├── terraform.tfvars.example  # Example variable values
├── .gitignore        # Ignore state and secrets
└── README.md         # This file
```

## Managing Configuration

### View Current State
```bash
terraform show
```

### Update Configuration
```bash
# Edit .tf files, then:
terraform plan   # Review changes
terraform apply  # Apply changes
```

### Import Existing Resources
```bash
# If resources already exist in Cloudflare:
terraform import cloudflare_dns_record.sentinel <zone_id>/<record_id>
```

### Detect Drift
```bash
# Show differences between state and actual configuration
terraform plan -refresh-only
```

### Destroy (Caution!)
```bash
# Remove all managed resources
terraform destroy
```

## Troubleshooting

### "Zone not found"
- Verify `cloudflare_zone_id` is correct
- Check API token has Zone:Read permission

### "Invalid API token"
- Regenerate token with correct permissions
- Ensure token is for correct account

### "Resource already exists"
- Use `terraform import` to import existing resources
- Or remove them from Cloudflare dashboard first

### Rate limiting not working
- Rate limits apply per IP, per period
- Test with: `for i in {1..150}; do curl -s https://sentinel.mindsmith.app/v1/models > /dev/null; done`

## Security Notes

- **Never commit `terraform.tfvars`** - contains API token
- **Origin certificate private key** is sensitive - handle carefully
- **State file** may contain secrets - use remote backend for production

## Remote Backend (Recommended for Teams)

```hcl
# Add to main.tf for remote state storage
terraform {
  backend "s3" {
    bucket = "your-terraform-state-bucket"
    key    = "sentinel/cloudflare/terraform.tfstate"
    region = "us-east-1"
  }
}
```
