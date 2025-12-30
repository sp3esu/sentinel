# Sentinel VPS Deployment Guide

This guide covers deploying Sentinel AI Proxy to a dedicated VPS behind Cloudflare.

## Current Project Status

The Sentinel project already has solid deployment foundations:
- Multi-stage Dockerfile with non-root user and health checks
- docker-compose.yml with Redis, health checks, and restart policies
- Kubernetes-ready health endpoints (`/health`, `/health/ready`, `/health/live`)
- Prometheus metrics at `/metrics`
- Environment-based configuration

---

## 1. VPS Initial Setup (Ubuntu 22.04/24.04)

### 1.1 Create Non-Root User

```bash
# On VPS as root
adduser deploy
usermod -aG sudo deploy
```

### 1.2 SSH Key Authentication

```bash
# On local machine
ssh-keygen -t ed25519 -C "deploy@sentinel"
ssh-copy-id -i ~/.ssh/id_ed25519.pub deploy@your-vps-ip

# On VPS - disable password auth
sudo nano /etc/ssh/sshd_config
```

Set these values:
```
PermitRootLogin no
PasswordAuthentication no
PubkeyAuthentication yes
MaxAuthTries 3
```

```bash
sudo sshd -t && sudo systemctl restart sshd
```

### 1.3 Firewall (UFW)

```bash
sudo apt install ufw
sudo ufw default deny incoming
sudo ufw default allow outgoing
sudo ufw allow ssh
sudo ufw allow 80/tcp
sudo ufw allow 443/tcp
sudo ufw enable
```

### 1.4 Fail2Ban

```bash
sudo apt install fail2ban
sudo cp /etc/fail2ban/jail.conf /etc/fail2ban/jail.local
sudo nano /etc/fail2ban/jail.local
```

Add:
```ini
[sshd]
enabled = true
maxretry = 3
bantime = 24h
```

```bash
sudo systemctl enable fail2ban && sudo systemctl start fail2ban
```

### 1.5 Automatic Security Updates

```bash
sudo apt install unattended-upgrades
sudo dpkg-reconfigure --priority=low unattended-upgrades
```

---

## 2. Docker Installation

```bash
# Install Docker
curl -fsSL https://get.docker.com | sh
sudo usermod -aG docker deploy

# Install Docker Compose plugin
sudo apt install docker-compose-plugin

# Enable Docker on boot
sudo systemctl enable docker
```

---

## 3. Application Deployment

### 3.1 Clone and Configure

```bash
sudo mkdir -p /opt/sentinel
sudo chown deploy:deploy /opt/sentinel
cd /opt/sentinel

# Clone repository (or copy files)
git clone <your-repo-url> .

# Create production environment file
cp .env.example .env
nano .env
```

### 3.2 Production Environment Variables

```bash
# /opt/sentinel/.env
SENTINEL_HOST=0.0.0.0
SENTINEL_PORT=8080
RUST_LOG=sentinel=info,tower_http=info

REDIS_URL=redis://sentinel-redis:6379

ZION_API_URL=https://your-zion-api.com
ZION_API_KEY=your-api-key

OPENAI_API_URL=https://api.openai.com/v1
OPENAI_API_KEY=sk-your-key

CACHE_TTL_SECONDS=300
JWT_CACHE_TTL_SECONDS=300

# Docker Compose overrides
SENTINEL_DOCKER_PORT=8080
SENTINEL_REDIS_PORT=6379
```

### 3.3 Production docker-compose.override.yml

Create `/opt/sentinel/docker-compose.override.yml`:

```yaml
version: "3.8"

services:
  sentinel:
    # Remove source mounts for production (use built image)
    volumes: []
    # Only expose to localhost (Caddy/nginx will proxy)
    ports:
      - "127.0.0.1:8080:8080"
    logging:
      driver: json-file
      options:
        max-size: "10m"
        max-file: "3"
    deploy:
      resources:
        limits:
          memory: 512M

  sentinel-redis:
    ports:
      - "127.0.0.1:6379:6379"
    command: >
      redis-server
      --appendonly yes
      --appendfsync everysec
      --maxmemory 256mb
      --maxmemory-policy allkeys-lru
```

### 3.4 Build and Start

```bash
cd /opt/sentinel
docker compose build --no-cache
docker compose up -d
docker compose ps
docker compose logs -f sentinel
```

---

## 4. Reverse Proxy with Caddy

Caddy provides automatic HTTPS and easy configuration.

### 4.1 Install Caddy

```bash
sudo apt install -y debian-keyring debian-archive-keyring apt-transport-https curl
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' | sudo gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' | sudo tee /etc/apt/sources.list.d/caddy-stable.list
sudo apt update
sudo apt install caddy
```

### 4.2 Cloudflare Origin Certificate

In Cloudflare Dashboard > SSL/TLS > Origin Server:
1. Create certificate for your domain (e.g., `api.yourdomain.com`)
2. Choose 15-year validity
3. Download certificate and private key

```bash
sudo mkdir -p /etc/cloudflare
sudo nano /etc/cloudflare/origin-cert.pem   # Paste certificate
sudo nano /etc/cloudflare/origin-key.pem    # Paste private key
sudo chmod 600 /etc/cloudflare/origin-key.pem
sudo chown caddy:caddy /etc/cloudflare/*
```

### 4.3 Caddyfile Configuration

```bash
sudo nano /etc/caddy/Caddyfile
```

```
api.yourdomain.com {
    reverse_proxy localhost:8080

    # Use Cloudflare origin certificate
    tls /etc/cloudflare/origin-cert.pem /etc/cloudflare/origin-key.pem

    # Timeouts for long AI requests
    reverse_proxy localhost:8080 {
        transport http {
            read_timeout 300s
            write_timeout 300s
        }
    }

    # Request logging
    log {
        output file /var/log/caddy/sentinel-access.log
        format json
    }

    # Headers
    header {
        -Server
        X-Content-Type-Options nosniff
        X-Frame-Options DENY
    }
}
```

```bash
sudo systemctl restart caddy
sudo systemctl enable caddy
```

---

## 5. Cloudflare Configuration

### 5.1 DNS Setup

1. Add A record: `api.yourdomain.com` -> Your VPS IP
2. Enable orange cloud (proxy) for DDoS protection

### 5.2 SSL/TLS Settings

- **Mode:** Full (Strict) - requires origin certificate
- **Always Use HTTPS:** On
- **Minimum TLS Version:** 1.2

### 5.3 Network Settings

- **WebSockets:** On (required for SSE streaming)
- **HTTP/2:** On
- **HTTP/3 (QUIC):** On

### 5.4 Critical: SSE Streaming Timeout

**Problem:** Cloudflare Free/Pro plans have 100-second idle timeout that can kill long AI requests.

**Solution 1: Implement Keep-Alive in Sentinel** (Recommended)

Modify streaming handlers to send SSE comments every 30 seconds. This requires code changes to `src/routes/chat.rs` and `src/routes/completions.rs`:

```rust
// Send keep-alive comment every 30 seconds during streaming
// SSE format: `: keep-alive\n\n`
// This resets Cloudflare's idle timeout without affecting clients
```

**Solution 2: Cloudflare Enterprise**

Enterprise plans allow configuring longer timeouts (up to 6000 seconds).

**Solution 3: DNS-Only Mode (Gray Cloud)**

For the API subdomain, use DNS-only mode (gray cloud) to bypass Cloudflare proxy. This loses DDoS protection but removes timeout restrictions.

### 5.5 Cache Rules

Create a rule to bypass caching for API:

1. Go to Rules > Cache Rules
2. Create rule:
   - **When:** URI Path contains `/v1/`
   - **Then:** Bypass cache

### 5.6 Basic Security Settings

- **Security Level:** Medium
- **Challenge Passage:** 30 minutes
- **Browser Integrity Check:** On
- **Bot Fight Mode:** On (but may need to allowlist API clients)

---

## 5.5 Advanced Security: WAF & Firewall Hardening

This section covers comprehensive security for protecting Sentinel from unauthorized access, DDoS attacks, and hiding the server IP.

### Security Architecture

```
Desktop App (with custom User-Agent)
        │
        ▼ (HTTPS to api.yourdomain.com)
┌───────────────────────────────────────────┐
│   Cloudflare Edge                         │
│   ├─ DDoS Protection (Layer 3/4/7)        │
│   ├─ WAF (OWASP + Custom Rules)           │
│   ├─ Bot Fight Mode                       │
│   └─ Rate Limiting (per IP)               │
└───────────────────────────────────────────┘
        │
        ▼ (Only Cloudflare IPs allowed)
┌───────────────────────────────────────────┐
│   VPS Firewall (iptables/UFW)             │
│   └─ deployment/cloudflare-firewall.sh    │
└───────────────────────────────────────────┘
        │
        ▼
┌───────────────────────────────────────────┐
│   Nginx/Caddy (TLS termination)           │
│   └─ deployment/nginx-sentinel.conf       │
└───────────────────────────────────────────┘
        │
        ▼
┌───────────────────────────────────────────┐
│   Sentinel (Application)                  │
│   ├─ JWT validation via Zion              │
│   └─ Per-user rate limiting (Redis)       │
└───────────────────────────────────────────┘
```

### 5.5.1 WAF Managed Rules

Navigate to: **Security → WAF → Managed Rules**

Enable these rulesets:
- **Cloudflare Managed Ruleset** - General protection
- **Cloudflare OWASP Core Ruleset** - OWASP Top 10 protection

### 5.5.2 WAF Custom Rules

Navigate to: **Security → WAF → Custom Rules**

**Rule 1: Require Your Desktop App User-Agent**

Only allow requests from your desktop application:

```
Expression:
(http.request.uri.path contains "/v1/") and
(not http.user_agent contains "YourDesktopApp/")

Action: Block
```

Replace `YourDesktopApp/` with your actual User-Agent string.

**Rule 2: Require Authorization Header on API Routes**

Block requests without JWT token:

```
Expression:
(http.request.uri.path contains "/v1/") and
(not any(http.request.headers["authorization"][*] contains "Bearer"))

Action: Block
```

**Rule 3: Block Common Attack Patterns**

```
Expression:
(http.request.uri.path contains "..") or
(http.request.uri.path contains ".env") or
(http.request.uri.path contains ".git") or
(http.request.uri.query contains "eval(") or
(http.request.uri.query contains "base64")

Action: Block
```

**Rule 4: Geographic Restrictions (Optional)**

If your users are in specific countries:

```
Expression:
(not ip.geoip.country in {"US" "CA" "GB" "DE" "FR"})

Action: Challenge
```

### 5.5.3 Rate Limiting Rules

Navigate to: **Security → WAF → Rate Limiting Rules**

**Rule 1: General API Rate Limit**

```
Expression: (http.request.uri.path contains "/v1/")
Characteristics: IP
Period: 1 minute
Requests: 100
Action: Block for 1 minute
```

**Rule 2: Aggressive Rate Limit**

```
Expression: (http.request.uri.path contains "/v1/")
Characteristics: IP
Period: 10 seconds
Requests: 20
Action: Challenge
```

**Rule 3: Auth Endpoint Protection**

```
Expression: (http.request.uri.path eq "/v1/chat/completions")
Characteristics: IP + Headers (Authorization)
Period: 1 minute
Requests: 60
Action: Block for 5 minutes
```

### 5.5.4 Bot Fight Mode

Navigate to: **Security → Bots**

- Enable **Bot Fight Mode** (Free)
- On Pro plan, enable **Super Bot Fight Mode**
- Configure to challenge or block definitely automated traffic

**Important:** Your desktop app may be flagged. Solutions:
1. Set a consistent, unique User-Agent
2. If issues persist, add WAF exception for your User-Agent
3. Consider using Cloudflare Access for machine-to-machine auth

### 5.5.5 VPS Firewall: Only Allow Cloudflare IPs

This is critical - it ensures your server IP cannot be accessed directly, even if discovered.

**Using the provided script:**

```bash
# Copy script to server
scp deployment/cloudflare-firewall.sh deploy@your-vps:/opt/sentinel/deployment/

# On VPS
sudo chmod +x /opt/sentinel/deployment/cloudflare-firewall.sh
sudo /opt/sentinel/deployment/cloudflare-firewall.sh

# Add to cron for weekly updates (Cloudflare IPs change occasionally)
echo "0 4 * * 0 root /opt/sentinel/deployment/cloudflare-firewall.sh >> /var/log/cf-fw.log 2>&1" | sudo tee -a /etc/crontab
```

**Alternative: Manual UFW setup**

```bash
# Deny all HTTP/HTTPS by default
sudo ufw deny 80/tcp
sudo ufw deny 443/tcp

# Allow only Cloudflare IP ranges (abbreviated, see script for full list)
sudo ufw allow from 173.245.48.0/20 to any port 80,443 proto tcp
sudo ufw allow from 103.21.244.0/22 to any port 80,443 proto tcp
# ... add all Cloudflare ranges
```

### 5.5.6 Hide Origin IP

Ensure your real server IP is never exposed:

1. **Never use the IP directly** - Always use the Cloudflare-proxied domain
2. **Check for leaks:**
   ```bash
   # These should NOT return your VPS IP
   dig +short api.yourdomain.com  # Should return Cloudflare IPs
   nslookup api.yourdomain.com
   ```
3. **Historical DNS:** Check sites like SecurityTrails for historical DNS records
4. **Email headers:** Don't send email from the VPS
5. **SSL certificates:** Don't use Let's Encrypt directly (use Cloudflare origin certs)

### 5.5.7 Desktop App Configuration

Update your desktop app to work with this security setup:

```typescript
// Example: Setting custom User-Agent
const headers = {
  'User-Agent': 'YourDesktopApp/1.0.0',
  'Authorization': `Bearer ${userJwtToken}`,
  'Content-Type': 'application/json'
};

// API calls
const response = await fetch('https://api.yourdomain.com/v1/chat/completions', {
  method: 'POST',
  headers,
  body: JSON.stringify(requestBody)
});
```

**Optional: Certificate Pinning**

For additional security, pin Cloudflare's edge certificate in your app to prevent MITM attacks.

### 5.5.8 Monitoring Blocked Requests

Navigate to: **Security → Events**

Review blocked requests to:
- Identify false positives (legitimate users blocked)
- Tune WAF rules if needed
- Monitor attack patterns

---

## 6. Monitoring Setup

### 6.1 Basic Monitoring Stack

Create `/opt/monitoring/docker-compose.yml`:

```yaml
version: "3.8"

services:
  prometheus:
    image: prom/prometheus:latest
    container_name: prometheus
    restart: unless-stopped
    volumes:
      - ./prometheus.yml:/etc/prometheus/prometheus.yml
      - prometheus-data:/prometheus
    command:
      - '--config.file=/etc/prometheus/prometheus.yml'
      - '--storage.tsdb.retention.time=15d'
    ports:
      - "127.0.0.1:9090:9090"
    extra_hosts:
      - "host.docker.internal:host-gateway"

  grafana:
    image: grafana/grafana:latest
    container_name: grafana
    restart: unless-stopped
    volumes:
      - grafana-data:/var/lib/grafana
    environment:
      - GF_SECURITY_ADMIN_PASSWORD=${GRAFANA_PASSWORD}
      - GF_USERS_ALLOW_SIGN_UP=false
    ports:
      - "127.0.0.1:3000:3000"

  node-exporter:
    image: prom/node-exporter:latest
    container_name: node-exporter
    restart: unless-stopped
    volumes:
      - /proc:/host/proc:ro
      - /sys:/host/sys:ro
      - /:/rootfs:ro
    command:
      - '--path.procfs=/host/proc'
      - '--path.sysfs=/host/sys'

volumes:
  prometheus-data:
  grafana-data:
```

### 6.2 Prometheus Configuration

Create `/opt/monitoring/prometheus.yml`:

```yaml
global:
  scrape_interval: 15s

scrape_configs:
  - job_name: 'prometheus'
    static_configs:
      - targets: ['localhost:9090']

  - job_name: 'sentinel'
    static_configs:
      - targets: ['host.docker.internal:8080']
    metrics_path: '/metrics'

  - job_name: 'node-exporter'
    static_configs:
      - targets: ['node-exporter:9100']
```

### 6.3 Start Monitoring

```bash
cd /opt/monitoring
echo "GRAFANA_PASSWORD=your-secure-password" > .env
docker compose up -d
```

### 6.4 Expose Grafana (Optional)

Add to `/etc/caddy/Caddyfile`:

```
monitoring.yourdomain.com {
    reverse_proxy localhost:3000
    tls /etc/cloudflare/origin-cert.pem /etc/cloudflare/origin-key.pem

    # Basic auth for security
    basicauth * {
        admin $2a$14$... # Use: caddy hash-password
    }
}
```

---

## 7. Systemd Service (Alternative to Docker restart)

For more control, create `/etc/systemd/system/sentinel.service`:

```ini
[Unit]
Description=Sentinel AI Proxy
Requires=docker.service
After=docker.service

[Service]
Type=oneshot
RemainAfterExit=yes
WorkingDirectory=/opt/sentinel
ExecStart=/usr/bin/docker compose up -d
ExecStop=/usr/bin/docker compose down
TimeoutStartSec=0

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl daemon-reload
sudo systemctl enable sentinel
```

**Note:** The docker-compose.yml already has `restart: unless-stopped`, so systemd is optional. Choose one approach, not both.

---

## 8. Deployment Checklist

### Pre-deployment
- [ ] VPS provisioned with Ubuntu 22.04/24.04
- [ ] SSH key authentication configured
- [ ] UFW firewall enabled (22, 80, 443)
- [ ] Fail2Ban installed and configured
- [ ] Automatic security updates enabled

### Application Setup
- [ ] Docker and Docker Compose installed
- [ ] Application cloned to /opt/sentinel
- [ ] Production .env file configured
- [ ] docker-compose.override.yml created
- [ ] Images built successfully
- [ ] Services healthy: `docker compose ps`

### Reverse Proxy
- [ ] Caddy or Nginx installed
- [ ] Cloudflare origin certificate installed
- [ ] Reverse proxy configured (use `deployment/nginx-sentinel.conf` as reference)
- [ ] Reverse proxy service running

### Cloudflare
- [ ] DNS A record pointing to VPS (orange cloud enabled)
- [ ] SSL mode: Full (Strict)
- [ ] WebSockets enabled
- [ ] Cache bypass rule for /v1/
- [ ] (Optional) Keep-alive implemented for SSE

### Cloudflare Security (see Section 5.5)
- [ ] WAF Managed Rules enabled (Cloudflare + OWASP)
- [ ] WAF Custom Rule: Require desktop app User-Agent
- [ ] WAF Custom Rule: Require Authorization header
- [ ] WAF Custom Rule: Block attack patterns
- [ ] Rate limiting rules configured
- [ ] Bot Fight Mode enabled

### VPS Security Hardening
- [ ] Cloudflare-only firewall configured (`deployment/cloudflare-firewall.sh`)
- [ ] Cron job for Cloudflare IP updates
- [ ] Origin IP verified hidden (dig/nslookup returns Cloudflare IPs)
- [ ] SSH restricted to admin IPs only

### Monitoring
- [ ] Prometheus scraping /metrics
- [ ] Grafana dashboards configured
- [ ] Node exporter for system metrics
- [ ] Cloudflare Security Events monitored

### Verification
- [ ] Health check passing: `curl https://api.yourdomain.com/health`
- [ ] API working: test chat completion
- [ ] Streaming working: test SSE response
- [ ] Metrics visible in Grafana
- [ ] Direct IP access blocked (test: `curl http://YOUR_VPS_IP` - should timeout)

---

## 9. Maintenance Commands

```bash
# View logs
docker compose -f /opt/sentinel/docker-compose.yml logs -f

# Restart services
docker compose -f /opt/sentinel/docker-compose.yml restart

# Update application
cd /opt/sentinel
git pull
docker compose build --no-cache
docker compose up -d

# Check health
curl -s http://localhost:8080/health | jq

# Redis CLI
docker exec -it sentinel-redis redis-cli

# View metrics
curl -s http://localhost:8080/metrics
```

---

## 10. Potential Code Changes Needed

### 10.1 SSE Keep-Alive for Cloudflare (Recommended)

To avoid Cloudflare's 100-second timeout, implement keep-alive comments in streaming responses:

**Files to modify:**
- `src/routes/chat.rs`
- `src/routes/completions.rs`

**Implementation approach:**
```rust
// During streaming, send SSE comments every 30 seconds
// Format: ": keep-alive\n\n"
// This doesn't affect clients but resets Cloudflare's idle timer
```

### 10.2 Dockerfile Optimization (Optional)

Consider using `cargo-chef` for faster builds if you're doing frequent deployments. The current Dockerfile is already well-optimized with dependency caching.

---

## 11. Deployment Directory Files

The `deployment/` directory contains ready-to-use configuration files:

### deployment/cloudflare-firewall.sh

Firewall script that configures iptables to only allow HTTP/HTTPS traffic from Cloudflare IPs. This ensures your server cannot be accessed directly even if the IP is discovered.

```bash
# Install and run
sudo cp deployment/cloudflare-firewall.sh /usr/local/bin/
sudo chmod +x /usr/local/bin/cloudflare-firewall.sh
sudo /usr/local/bin/cloudflare-firewall.sh

# Options
sudo /usr/local/bin/cloudflare-firewall.sh --ufw  # Use UFW instead of iptables
```

### deployment/nginx-sentinel.conf

Complete Nginx configuration for reverse proxying to Sentinel with:
- Cloudflare origin certificate SSL
- Rate limiting (backup layer)
- Security headers
- Optimized timeouts for AI streaming
- Cloudflare real IP restoration

```bash
# Install
sudo cp deployment/nginx-sentinel.conf /etc/nginx/sites-available/sentinel
sudo ln -s /etc/nginx/sites-available/sentinel /etc/nginx/sites-enabled/
# Edit to set your domain name
sudo nano /etc/nginx/sites-available/sentinel
sudo nginx -t && sudo systemctl reload nginx
```

### deployment/sentinel.service

Systemd service file for managing Sentinel via Docker Compose:

```bash
# Install
sudo cp deployment/sentinel.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable sentinel
sudo systemctl start sentinel

# Manage
sudo systemctl status sentinel
sudo systemctl restart sentinel
sudo journalctl -u sentinel -f
```

---

## Sources

- [cargo-chef for Docker builds](https://github.com/LukeMathWalker/cargo-chef)
- [Cloudflare Full Strict SSL](https://developers.cloudflare.com/ssl/origin-configuration/ssl-modes/full-strict/)
- [Cloudflare SSE timeout issue](https://community.cloudflare.com/t/are-server-sent-events-sse-supported-or-will-they-trigger-http-524-timeouts/499621)
- [SSE timeout mitigation](https://smartscope.blog/en/Infrastructure/sse-timeout-mitigation-cloudflare-alb/)
- [Docker restart policies](https://docs.docker.com/engine/containers/start-containers-automatically/)
- [dockprom monitoring stack](https://github.com/stefanprodan/dockprom)
- [Cloudflare IP Ranges](https://www.cloudflare.com/ips/)
- [Cloudflare WAF Custom Rules](https://developers.cloudflare.com/waf/custom-rules/)
