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

### 5.6 Security Settings

- **Security Level:** Medium
- **Challenge Passage:** 30 minutes
- **Browser Integrity Check:** On
- **Bot Fight Mode:** On (but may need to allowlist API clients)

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
- [ ] Caddy installed
- [ ] Cloudflare origin certificate installed
- [ ] Caddyfile configured
- [ ] Caddy service running

### Cloudflare
- [ ] DNS A record pointing to VPS
- [ ] SSL mode: Full (Strict)
- [ ] WebSockets enabled
- [ ] Cache bypass rule for /v1/
- [ ] (Optional) Keep-alive implemented for SSE

### Monitoring
- [ ] Prometheus scraping /metrics
- [ ] Grafana dashboards configured
- [ ] Node exporter for system metrics

### Verification
- [ ] Health check passing: `curl https://api.yourdomain.com/health`
- [ ] API working: test chat completion
- [ ] Streaming working: test SSE response
- [ ] Metrics visible in Grafana

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

## Sources

- [cargo-chef for Docker builds](https://github.com/LukeMathWalker/cargo-chef)
- [Cloudflare Full Strict SSL](https://developers.cloudflare.com/ssl/origin-configuration/ssl-modes/full-strict/)
- [Cloudflare SSE timeout issue](https://community.cloudflare.com/t/are-server-sent-events-sse-supported-or-will-they-trigger-http-524-timeouts/499621)
- [SSE timeout mitigation](https://smartscope.blog/en/Infrastructure/sse-timeout-mitigation-cloudflare-alb/)
- [Docker restart policies](https://docs.docker.com/engine/containers/start-containers-automatically/)
- [dockprom monitoring stack](https://github.com/stefanprodan/dockprom)
