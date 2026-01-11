#!/bin/bash
# =============================================================================
# Sentinel AI Proxy - Initial VPS Deployment Script
# =============================================================================
#
# This script sets up a complete Sentinel environment on a fresh Ubuntu VPS.
#
# Usage:
#   sudo ./deploy-initial.sh --env-file /tmp/.env.prod --repo git@github.com:org/sentinel.git --domain api.example.com --ssh-key /tmp/github_deploy_key
#
# Requirements:
#   - Fresh Ubuntu 22.04 or 24.04 server
#   - Root or sudo access
#   - GitHub deploy key (private key file)
#   - .env.prod file with secrets copied to server
#   - (Optional) Cloudflare origin SSL certificate and key
#
# =============================================================================

set -euo pipefail

# =============================================================================
# Configuration
# =============================================================================

SCRIPT_VERSION="1.0.0"
LOG_FILE="/var/log/sentinel-deploy.log"
INSTALL_DIR="/opt/sentinel"
TOTAL_STEPS=13

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m' # No Color

# Status tracking
declare -A STEP_STATUS
CURRENT_STEP=0
FAILED_STEP=""
FAILED_MESSAGE=""

# Arguments
ENV_FILE=""
REPO_URL=""
DOMAIN=""
SSH_KEY_PATH=""
BRANCH="main"
SKIP_FIREWALL=false
SKIP_SECURITY=false
SKIP_NGINX=false
CERT_PATH=""
KEY_PATH=""

# =============================================================================
# Logging Functions
# =============================================================================

log() {
    local level="$1"
    local message="$2"
    local timestamp
    timestamp=$(date '+%Y-%m-%d %H:%M:%S')

    # Write to log file
    echo "[$timestamp] [$level] $message" >> "$LOG_FILE"

    # Display to console with colors
    case "$level" in
        INFO)
            echo -e "${BLUE}[$timestamp]${NC} ${CYAN}[INFO]${NC}  $message"
            ;;
        OK)
            echo -e "${BLUE}[$timestamp]${NC} ${GREEN}[OK]${NC}    $message"
            ;;
        WARN)
            echo -e "${BLUE}[$timestamp]${NC} ${YELLOW}[WARN]${NC}  $message"
            ;;
        ERROR)
            echo -e "${BLUE}[$timestamp]${NC} ${RED}[ERROR]${NC} $message"
            ;;
        SKIP)
            echo -e "${BLUE}[$timestamp]${NC} ${YELLOW}[SKIP]${NC}  $message"
            ;;
        STEP)
            echo -e "${BLUE}[$timestamp]${NC} ${BOLD}[Step $CURRENT_STEP/$TOTAL_STEPS]${NC} $message"
            ;;
    esac
}

log_cmd() {
    # Log command output to file only
    "$@" >> "$LOG_FILE" 2>&1
}

# =============================================================================
# Helper Functions
# =============================================================================

print_banner() {
    echo -e "${CYAN}"
    echo "╔════════════════════════════════════════════════════════════════╗"
    echo "║                                                                ║"
    echo "║              SENTINEL AI PROXY DEPLOYMENT                      ║"
    echo "║                     Version $SCRIPT_VERSION                            ║"
    echo "║                                                                ║"
    echo "╚════════════════════════════════════════════════════════════════╝"
    echo -e "${NC}"
    echo ""
}

print_usage() {
    cat << EOF
Usage: $0 [OPTIONS]

Required:
  --env-file PATH       Path to .env.prod file with secrets
  --repo URL            Git repository URL to clone (SSH format)
  --domain DOMAIN       Domain for Nginx (e.g., api.example.com)
  --ssh-key PATH        Path to GitHub deploy key (private key file)

Optional:
  --branch BRANCH       Git branch to checkout (default: main)
  --skip-firewall       Skip UFW firewall configuration
  --skip-security       Skip Fail2Ban and security hardening
  --skip-nginx          Skip Nginx reverse proxy installation
  --cert-path PATH      Path to Cloudflare origin certificate (.pem)
  --key-path PATH       Path to Cloudflare origin private key (.key)
  --help                Show this help message

Examples:
  # Full deployment with SSL certificates
  sudo $0 --env-file /tmp/.env.prod \\
          --repo git@github.com:org/sentinel.git \\
          --domain api.example.com \\
          --ssh-key /tmp/github_deploy_key \\
          --cert-path /tmp/origin.pem \\
          --key-path /tmp/origin.key

  # Initial test deployment (skip firewall to avoid lockout)
  sudo $0 --env-file /tmp/.env.prod \\
          --repo git@github.com:org/sentinel.git \\
          --domain api.example.com \\
          --ssh-key /tmp/github_deploy_key \\
          --skip-firewall --skip-security
EOF
}

parse_args() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --env-file)
                ENV_FILE="$2"
                shift 2
                ;;
            --repo)
                REPO_URL="$2"
                shift 2
                ;;
            --domain)
                DOMAIN="$2"
                shift 2
                ;;
            --ssh-key)
                SSH_KEY_PATH="$2"
                shift 2
                ;;
            --branch)
                BRANCH="$2"
                shift 2
                ;;
            --skip-firewall)
                SKIP_FIREWALL=true
                shift
                ;;
            --skip-security)
                SKIP_SECURITY=true
                shift
                ;;
            --skip-nginx)
                SKIP_NGINX=true
                shift
                ;;
            --cert-path)
                CERT_PATH="$2"
                shift 2
                ;;
            --key-path)
                KEY_PATH="$2"
                shift 2
                ;;
            --help)
                print_usage
                exit 0
                ;;
            *)
                echo -e "${RED}Error: Unknown option: $1${NC}"
                print_usage
                exit 1
                ;;
        esac
    done

    # Validate required arguments
    local missing=()
    [[ -z "$ENV_FILE" ]] && missing+=("--env-file")
    [[ -z "$REPO_URL" ]] && missing+=("--repo")
    [[ -z "$DOMAIN" ]] && missing+=("--domain")
    [[ -z "$SSH_KEY_PATH" ]] && missing+=("--ssh-key")

    if [[ ${#missing[@]} -gt 0 ]]; then
        echo -e "${RED}Error: Missing required arguments: ${missing[*]}${NC}"
        echo ""
        print_usage
        exit 1
    fi

    # Validate env file exists
    if [[ ! -f "$ENV_FILE" ]]; then
        echo -e "${RED}Error: Environment file not found: $ENV_FILE${NC}"
        exit 1
    fi

    # Validate SSH key file exists
    if [[ ! -f "$SSH_KEY_PATH" ]]; then
        echo -e "${RED}Error: SSH key file not found: $SSH_KEY_PATH${NC}"
        exit 1
    fi
}

mark_step() {
    local step="$1"
    local status="$2"
    STEP_STATUS["$step"]="$status"
}

# =============================================================================
# Cleanup and Summary
# =============================================================================

print_summary() {
    echo ""
    echo -e "${CYAN}════════════════════════════════════════════════════════════════${NC}"

    if [[ -n "$FAILED_STEP" ]]; then
        echo -e "${RED}              SENTINEL DEPLOYMENT FAILED${NC}"
    else
        echo -e "${GREEN}              SENTINEL DEPLOYMENT COMPLETE${NC}"
    fi

    echo -e "${CYAN}════════════════════════════════════════════════════════════════${NC}"
    echo ""
    echo -e "${BOLD}STATUS SUMMARY${NC}"
    echo "──────────────"

    # Print status for each step
    local steps=(
        "1:System packages"
        "2:SSH key setup"
        "3:Docker"
        "4:Firewall (UFW)"
        "5:Security hardening"
        "6:Repository clone"
        "7:Environment config"
        "8:Production override"
        "9:Docker build"
        "10:Service verification"
        "11:Nginx reverse proxy"
        "12:Systemd service"
    )

    for step_info in "${steps[@]}"; do
        local num="${step_info%%:*}"
        local name="${step_info#*:}"
        local status="${STEP_STATUS[$num]:-pending}"

        case "$status" in
            ok)
                echo -e "  ${GREEN}[✓]${NC} $name"
                ;;
            skip)
                echo -e "  ${YELLOW}[−]${NC} $name ${YELLOW}(skipped)${NC}"
                ;;
            fail)
                echo -e "  ${RED}[✗]${NC} $name ${RED}(failed)${NC}"
                ;;
            *)
                echo -e "  ${YELLOW}[ ]${NC} $name ${YELLOW}(not started)${NC}"
                ;;
        esac
    done

    echo ""

    if [[ -z "$FAILED_STEP" ]]; then
        echo -e "${BOLD}SERVICES${NC}"
        echo "────────"
        echo "  Sentinel:  http://127.0.0.1:8080 (internal)"
        echo "  Redis:     127.0.0.1:6379 (internal)"
        if [[ "$SKIP_NGINX" != true ]]; then
            echo "  Nginx:     https://$DOMAIN (external)"
        fi
        echo ""

        echo -e "${BOLD}HEALTH ENDPOINTS${NC}"
        echo "────────────────"
        echo "  curl http://127.0.0.1:8080/health"
        echo "  curl http://127.0.0.1:8080/health/ready"
        echo "  curl http://127.0.0.1:8080/metrics"
        echo ""

        echo -e "${BOLD}USEFUL COMMANDS${NC}"
        echo "───────────────"
        echo "  sudo systemctl status sentinel"
        echo "  sudo docker compose -f $INSTALL_DIR/docker-compose.yml logs -f"
        if [[ "$SKIP_NGINX" != true ]]; then
            echo "  sudo journalctl -u nginx -f"
            echo "  sudo tail -f /var/log/nginx/sentinel-access.log"
        fi
        echo ""

        echo -e "${BOLD}NEXT STEPS${NC}"
        echo "──────────"
        echo "  1. Configure Cloudflare DNS A record → $(curl -s ifconfig.me 2>/dev/null || echo 'YOUR_VPS_IP')"
        echo "  2. Set Cloudflare SSL mode to \"Full (Strict)\""
        echo "  3. Enable WebSockets in Cloudflare"
        echo "  4. Test: curl https://$DOMAIN/health"

        if [[ "$SKIP_FIREWALL" == true ]] || [[ "$SKIP_SECURITY" == true ]]; then
            echo ""
            echo -e "${YELLOW}  ⚠ WARNING: Security features were skipped. Run again without${NC}"
            echo -e "${YELLOW}    --skip-firewall and --skip-security for production.${NC}"
        fi
    else
        echo -e "${RED}FAILURE DETAILS${NC}"
        echo "───────────────"
        echo -e "  Step: $FAILED_STEP"
        echo -e "  Error: $FAILED_MESSAGE"
        echo ""
        echo "  Check the log file for details:"
        echo "  sudo tail -100 $LOG_FILE"
    fi

    echo ""
    echo -e "${CYAN}LOG FILE: $LOG_FILE${NC}"
    echo -e "${CYAN}════════════════════════════════════════════════════════════════${NC}"
}

cleanup() {
    local exit_code=$?

    if [[ $exit_code -ne 0 ]] && [[ -z "$FAILED_STEP" ]]; then
        FAILED_STEP="Unknown"
        FAILED_MESSAGE="Script exited with code $exit_code"
    fi

    print_summary
    exit $exit_code
}

trap cleanup EXIT

# =============================================================================
# Step 1: Prerequisites Check
# =============================================================================

step_prerequisites() {
    CURRENT_STEP=1
    log STEP "Checking prerequisites..."

    # Check if running as root
    if [[ $EUID -ne 0 ]]; then
        FAILED_STEP="Prerequisites"
        FAILED_MESSAGE="This script must be run as root (use sudo)"
        exit 1
    fi

    # Check Ubuntu version
    if [[ -f /etc/os-release ]]; then
        . /etc/os-release
        if [[ "$ID" != "ubuntu" ]]; then
            FAILED_STEP="Prerequisites"
            FAILED_MESSAGE="This script requires Ubuntu (detected: $ID)"
            exit 1
        fi

        case "$VERSION_ID" in
            22.04|24.04)
                log OK "Ubuntu $VERSION_ID detected"
                ;;
            *)
                log WARN "Ubuntu $VERSION_ID detected (recommended: 22.04 or 24.04)"
                ;;
        esac
    else
        log WARN "Could not detect OS version"
    fi

    # Check internet connectivity
    if ! ping -c 1 -W 5 8.8.8.8 &>/dev/null; then
        FAILED_STEP="Prerequisites"
        FAILED_MESSAGE="No internet connectivity"
        exit 1
    fi
    log OK "Internet connectivity verified"

    # Initialize log file
    mkdir -p "$(dirname "$LOG_FILE")"
    echo "=== Sentinel Deployment Log ===" > "$LOG_FILE"
    echo "Started: $(date)" >> "$LOG_FILE"
    echo "Arguments: $*" >> "$LOG_FILE"
    echo "" >> "$LOG_FILE"

    mark_step 1 ok
}

# =============================================================================
# Step 2: System Update & Packages
# =============================================================================

step_system_packages() {
    CURRENT_STEP=2
    log STEP "Installing system packages..."

    # Update package lists
    log INFO "Updating package lists..."
    if ! apt-get update >> "$LOG_FILE" 2>&1; then
        FAILED_STEP="System packages"
        FAILED_MESSAGE="apt-get update failed"
        exit 1
    fi

    # Upgrade existing packages
    log INFO "Upgrading existing packages..."
    DEBIAN_FRONTEND=noninteractive apt-get upgrade -y >> "$LOG_FILE" 2>&1 || true

    # Install required packages
    log INFO "Installing required packages..."
    local packages=(
        curl
        git
        jq
        ufw
        fail2ban
        unattended-upgrades
        apt-transport-https
        ca-certificates
        gnupg
        lsb-release
    )

    if ! DEBIAN_FRONTEND=noninteractive apt-get install -y "${packages[@]}" >> "$LOG_FILE" 2>&1; then
        FAILED_STEP="System packages"
        FAILED_MESSAGE="Failed to install required packages"
        exit 1
    fi

    log OK "System packages installed"
    mark_step 1 ok
}

# =============================================================================
# Step 3: SSH Key Setup
# =============================================================================

step_ssh_setup() {
    CURRENT_STEP=3
    log STEP "Configuring SSH key for GitHub..."

    # Create .ssh directory for root (script runs as root)
    mkdir -p /root/.ssh
    chmod 700 /root/.ssh

    # Copy SSH key
    cp "$SSH_KEY_PATH" /root/.ssh/github_deploy_key
    chmod 600 /root/.ssh/github_deploy_key

    # Configure SSH to use this key for github.com
    cat >> /root/.ssh/config << 'EOF'
Host github.com
    HostName github.com
    User git
    IdentityFile /root/.ssh/github_deploy_key
    IdentitiesOnly yes
    StrictHostKeyChecking accept-new
EOF
    chmod 600 /root/.ssh/config

    # Test SSH connection
    log INFO "Testing GitHub SSH connection..."
    local ssh_output
    ssh_output=$(ssh -T git@github.com 2>&1 || true)

    if echo "$ssh_output" | grep -q "successfully authenticated"; then
        log OK "GitHub SSH authentication configured"
    elif echo "$ssh_output" | grep -q "You've successfully authenticated"; then
        log OK "GitHub SSH authentication configured"
    else
        log WARN "SSH test output: $ssh_output"
        FAILED_STEP="SSH setup"
        FAILED_MESSAGE="Failed to authenticate with GitHub. Check SSH key."
        exit 1
    fi

    mark_step 2 ok
}

# =============================================================================
# Step 4: Docker Installation
# =============================================================================

step_docker() {
    CURRENT_STEP=4
    log STEP "Installing Docker..."

    # Check if Docker is already installed
    if command -v docker &>/dev/null; then
        local docker_version
        docker_version=$(docker --version | awk '{print $3}' | tr -d ',')
        log OK "Docker already installed (version $docker_version)"
    else
        # Install Docker using official script
        log INFO "Downloading Docker installation script..."
        if ! curl -fsSL https://get.docker.com -o /tmp/get-docker.sh >> "$LOG_FILE" 2>&1; then
            FAILED_STEP="Docker"
            FAILED_MESSAGE="Failed to download Docker installation script"
            exit 1
        fi

        log INFO "Running Docker installation..."
        if ! sh /tmp/get-docker.sh >> "$LOG_FILE" 2>&1; then
            FAILED_STEP="Docker"
            FAILED_MESSAGE="Docker installation failed"
            exit 1
        fi
        rm -f /tmp/get-docker.sh

        local docker_version
        docker_version=$(docker --version | awk '{print $3}' | tr -d ',')
        log OK "Docker $docker_version installed"
    fi

    # Install Docker Compose plugin if not present
    if ! docker compose version &>/dev/null; then
        log INFO "Installing Docker Compose plugin..."
        if ! apt-get install -y docker-compose-plugin >> "$LOG_FILE" 2>&1; then
            FAILED_STEP="Docker"
            FAILED_MESSAGE="Failed to install Docker Compose plugin"
            exit 1
        fi
    fi

    local compose_version
    compose_version=$(docker compose version --short 2>/dev/null || echo "unknown")
    log OK "Docker Compose $compose_version available"

    # Enable Docker on boot
    systemctl enable docker >> "$LOG_FILE" 2>&1
    systemctl start docker >> "$LOG_FILE" 2>&1

    # Add deploy user to docker group (if exists)
    if id "deploy" &>/dev/null; then
        usermod -aG docker deploy >> "$LOG_FILE" 2>&1
        log INFO "Added 'deploy' user to docker group"
    fi

    mark_step 3 ok
}

# =============================================================================
# Step 5: Firewall Configuration
# =============================================================================

step_firewall() {
    CURRENT_STEP=5

    if [[ "$SKIP_FIREWALL" == true ]]; then
        log SKIP "Firewall configuration (--skip-firewall)"
        mark_step 4 skip
        return
    fi

    log STEP "Configuring firewall (UFW)..."

    # Reset UFW to defaults
    ufw --force reset >> "$LOG_FILE" 2>&1

    # Set default policies
    ufw default deny incoming >> "$LOG_FILE" 2>&1
    ufw default allow outgoing >> "$LOG_FILE" 2>&1

    # Allow SSH (critical - do this first!)
    ufw allow ssh >> "$LOG_FILE" 2>&1
    log INFO "Allowed SSH (port 22)"

    # Allow HTTP and HTTPS
    ufw allow 80/tcp >> "$LOG_FILE" 2>&1
    ufw allow 443/tcp >> "$LOG_FILE" 2>&1
    log INFO "Allowed HTTP (80) and HTTPS (443)"

    # Enable UFW
    ufw --force enable >> "$LOG_FILE" 2>&1

    log OK "Firewall configured and enabled"
    mark_step 4 ok
}

# =============================================================================
# Step 6: Security Hardening
# =============================================================================

step_security() {
    CURRENT_STEP=6

    if [[ "$SKIP_SECURITY" == true ]]; then
        log SKIP "Security hardening (--skip-security)"
        mark_step 5 skip
        return
    fi

    log STEP "Applying security hardening..."

    # Configure Fail2Ban
    log INFO "Configuring Fail2Ban..."
    cat > /etc/fail2ban/jail.local << 'EOF'
[DEFAULT]
bantime = 24h
findtime = 10m
maxretry = 3

[sshd]
enabled = true
port = ssh
filter = sshd
logpath = /var/log/auth.log
maxretry = 3
bantime = 24h
EOF

    systemctl enable fail2ban >> "$LOG_FILE" 2>&1
    systemctl restart fail2ban >> "$LOG_FILE" 2>&1
    log OK "Fail2Ban configured"

    # Enable unattended upgrades
    log INFO "Enabling automatic security updates..."
    cat > /etc/apt/apt.conf.d/50unattended-upgrades << 'EOF'
Unattended-Upgrade::Allowed-Origins {
    "${distro_id}:${distro_codename}-security";
    "${distro_id}ESMApps:${distro_codename}-apps-security";
    "${distro_id}ESM:${distro_codename}-infra-security";
};
Unattended-Upgrade::AutoFixInterruptedDpkg "true";
Unattended-Upgrade::MinimalSteps "true";
Unattended-Upgrade::Remove-Unused-Dependencies "true";
Unattended-Upgrade::Automatic-Reboot "false";
EOF

    cat > /etc/apt/apt.conf.d/20auto-upgrades << 'EOF'
APT::Periodic::Update-Package-Lists "1";
APT::Periodic::Unattended-Upgrade "1";
APT::Periodic::AutocleanInterval "7";
EOF

    systemctl enable unattended-upgrades >> "$LOG_FILE" 2>&1
    log OK "Automatic security updates enabled"

    mark_step 5 ok
}

# =============================================================================
# Step 7: Clone Repository
# =============================================================================

step_clone_repo() {
    CURRENT_STEP=7
    log STEP "Cloning repository..."

    # Create installation directory
    if [[ -d "$INSTALL_DIR" ]]; then
        log WARN "Directory $INSTALL_DIR already exists, backing up..."
        mv "$INSTALL_DIR" "${INSTALL_DIR}.backup.$(date +%Y%m%d%H%M%S)"
    fi

    mkdir -p "$INSTALL_DIR"

    # Clone repository
    log INFO "Cloning from $REPO_URL (branch: $BRANCH)..."

    # SSH config from step_ssh_setup() handles GitHub authentication
    if ! git clone --branch "$BRANCH" "$REPO_URL" "$INSTALL_DIR" >> "$LOG_FILE" 2>&1; then
        FAILED_STEP="Clone repository"
        FAILED_MESSAGE="Failed to clone repository. Check SSH key configuration."
        exit 1
    fi

    # Set ownership
    if id "deploy" &>/dev/null; then
        chown -R deploy:deploy "$INSTALL_DIR"
        log INFO "Set ownership to 'deploy' user"
    fi

    log OK "Repository cloned to $INSTALL_DIR"
    mark_step 6 ok
}

# =============================================================================
# Step 8: Environment Setup
# =============================================================================

step_environment() {
    CURRENT_STEP=8
    log STEP "Configuring environment..."

    # Copy environment file
    cp "$ENV_FILE" "$INSTALL_DIR/.env"
    chmod 600 "$INSTALL_DIR/.env"

    # Validate required variables
    local required_vars=(
        "ZION_API_URL"
        "ZION_API_KEY"
        "OPENAI_API_KEY"
    )

    local missing_vars=()
    for var in "${required_vars[@]}"; do
        if ! grep -q "^${var}=" "$INSTALL_DIR/.env" || grep -q "^${var}=your-" "$INSTALL_DIR/.env"; then
            missing_vars+=("$var")
        fi
    done

    if [[ ${#missing_vars[@]} -gt 0 ]]; then
        FAILED_STEP="Environment setup"
        FAILED_MESSAGE="Missing or placeholder values for: ${missing_vars[*]}"
        exit 1
    fi

    log OK "Environment configured"
    mark_step 7 ok
}

# =============================================================================
# Step 9: Production Override
# =============================================================================

step_production_override() {
    CURRENT_STEP=9
    log STEP "Creating production configuration..."

    # Create docker-compose.override.yml for production
    cat > "$INSTALL_DIR/docker-compose.override.yml" << 'EOF'
# Production overrides for Sentinel
# Generated by deploy-initial.sh

services:
  sentinel:
    # Remove source mounts for production (use built image)
    volumes: []
    # Only expose to localhost (Nginx will proxy)
    ports:
      - "127.0.0.1:8080:8080"
    # Production logging
    logging:
      driver: json-file
      options:
        max-size: "10m"
        max-file: "3"
    # Resource limits
    deploy:
      resources:
        limits:
          memory: 512M

  sentinel-redis:
    # Only expose to localhost
    ports:
      - "127.0.0.1:6379:6379"
    # Redis memory limit
    command: >
      redis-server
      --appendonly yes
      --appendfsync everysec
      --maxmemory 256mb
      --maxmemory-policy allkeys-lru
    logging:
      driver: json-file
      options:
        max-size: "10m"
        max-file: "3"
EOF

    log OK "Production override created"
    mark_step 8 ok
}

# =============================================================================
# Step 10: Build & Start Docker
# =============================================================================

step_docker_build() {
    CURRENT_STEP=10
    log STEP "Building and starting Docker containers..."

    cd "$INSTALL_DIR"

    # Build images
    log INFO "Building Docker images (this may take a few minutes)..."
    if ! docker compose build --no-cache >> "$LOG_FILE" 2>&1; then
        FAILED_STEP="Docker build"
        FAILED_MESSAGE="Failed to build Docker images"
        exit 1
    fi
    log OK "Docker images built"

    # Start services
    log INFO "Starting services..."
    if ! docker compose up -d >> "$LOG_FILE" 2>&1; then
        FAILED_STEP="Docker build"
        FAILED_MESSAGE="Failed to start Docker containers"
        exit 1
    fi

    # Wait for health checks
    log INFO "Waiting for services to be healthy..."
    local timeout=120
    local elapsed=0

    while [[ $elapsed -lt $timeout ]]; do
        if curl -sf http://127.0.0.1:8080/health/live &>/dev/null; then
            log OK "Services started and healthy"
            mark_step 9 ok
            return
        fi
        sleep 2
        elapsed=$((elapsed + 2))

        # Show progress every 10 seconds
        if [[ $((elapsed % 10)) -eq 0 ]]; then
            log INFO "Still waiting... ($elapsed/${timeout}s)"
        fi
    done

    FAILED_STEP="Docker build"
    FAILED_MESSAGE="Services did not become healthy within ${timeout}s"
    docker compose logs >> "$LOG_FILE" 2>&1
    exit 1
}

# =============================================================================
# Step 11: Verify Services
# =============================================================================

step_verify_services() {
    CURRENT_STEP=11
    log STEP "Verifying services..."

    cd "$INSTALL_DIR"

    # Check Redis
    log INFO "Checking Redis..."
    if ! docker compose exec -T sentinel-redis redis-cli ping >> "$LOG_FILE" 2>&1; then
        FAILED_STEP="Service verification"
        FAILED_MESSAGE="Redis is not responding"
        exit 1
    fi
    log OK "Redis is healthy"

    # Check Sentinel health/live
    log INFO "Checking Sentinel liveness..."
    if ! curl -sf http://127.0.0.1:8080/health/live >> "$LOG_FILE" 2>&1; then
        FAILED_STEP="Service verification"
        FAILED_MESSAGE="Sentinel /health/live endpoint not responding"
        exit 1
    fi
    log OK "Sentinel liveness check passed"

    # Check Sentinel health/ready
    log INFO "Checking Sentinel readiness..."
    if ! curl -sf http://127.0.0.1:8080/health/ready >> "$LOG_FILE" 2>&1; then
        log WARN "Sentinel /health/ready returned non-200 (may be expected if Zion is not reachable)"
    else
        log OK "Sentinel readiness check passed"
    fi

    mark_step 10 ok
}

# =============================================================================
# Step 12: Nginx Setup
# =============================================================================

step_nginx() {
    CURRENT_STEP=12

    if [[ "$SKIP_NGINX" == true ]]; then
        log SKIP "Nginx installation (--skip-nginx)"
        mark_step 11 skip
        return
    fi

    log STEP "Setting up Nginx reverse proxy..."

    # Install Nginx
    log INFO "Installing Nginx..."
    if ! apt-get install -y nginx >> "$LOG_FILE" 2>&1; then
        FAILED_STEP="Nginx setup"
        FAILED_MESSAGE="Failed to install Nginx"
        exit 1
    fi

    # Create SSL directory
    mkdir -p /etc/ssl/cloudflare

    # Copy SSL certificates if provided
    if [[ -n "$CERT_PATH" ]] && [[ -n "$KEY_PATH" ]]; then
        if [[ -f "$CERT_PATH" ]] && [[ -f "$KEY_PATH" ]]; then
            cp "$CERT_PATH" /etc/ssl/cloudflare/origin.pem
            cp "$KEY_PATH" /etc/ssl/cloudflare/origin.key
            chmod 600 /etc/ssl/cloudflare/origin.key
            log OK "SSL certificates installed"
        else
            log WARN "SSL certificate files not found, skipping SSL setup"
        fi
    else
        log WARN "No SSL certificates provided. Nginx will be configured but HTTPS may not work."
        log INFO "To add certificates later, place them at:"
        log INFO "  - /etc/ssl/cloudflare/origin.pem"
        log INFO "  - /etc/ssl/cloudflare/origin.key"
    fi

    # Copy Nginx configuration
    if [[ -f "$INSTALL_DIR/deployment/nginx-sentinel.conf" ]]; then
        cp "$INSTALL_DIR/deployment/nginx-sentinel.conf" /etc/nginx/sites-available/sentinel

        # Update domain name in config
        sed -i "s/api\.yourdomain\.com/$DOMAIN/g" /etc/nginx/sites-available/sentinel

        # Update SSL certificate paths
        sed -i "s|/etc/ssl/cloudflare/|/etc/ssl/cloudflare/|g" /etc/nginx/sites-available/sentinel

        # Enable site
        ln -sf /etc/nginx/sites-available/sentinel /etc/nginx/sites-enabled/

        # Remove default site
        rm -f /etc/nginx/sites-enabled/default

        log OK "Nginx configuration installed"
    else
        log WARN "Nginx config not found at $INSTALL_DIR/deployment/nginx-sentinel.conf"
        log INFO "Creating minimal Nginx configuration..."

        cat > /etc/nginx/sites-available/sentinel << EOF
server {
    listen 80;
    server_name $DOMAIN;

    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_http_version 1.1;
        proxy_set_header Host \$host;
        proxy_set_header X-Real-IP \$remote_addr;
        proxy_set_header X-Forwarded-For \$proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto \$scheme;
        proxy_buffering off;
        proxy_cache off;
    }
}
EOF
        ln -sf /etc/nginx/sites-available/sentinel /etc/nginx/sites-enabled/
        rm -f /etc/nginx/sites-enabled/default
    fi

    # Add Cloudflare IP ranges to nginx.conf for real IP restoration
    if ! grep -q "set_real_ip_from" /etc/nginx/nginx.conf; then
        log INFO "Adding Cloudflare IP ranges to nginx.conf..."

        # Create a snippet file for Cloudflare IPs
        cat > /etc/nginx/conf.d/cloudflare-ips.conf << 'EOF'
# Cloudflare IP ranges for real IP restoration
# Update periodically from https://www.cloudflare.com/ips/

set_real_ip_from 173.245.48.0/20;
set_real_ip_from 103.21.244.0/22;
set_real_ip_from 103.22.200.0/22;
set_real_ip_from 103.31.4.0/22;
set_real_ip_from 141.101.64.0/18;
set_real_ip_from 108.162.192.0/18;
set_real_ip_from 190.93.240.0/20;
set_real_ip_from 188.114.96.0/20;
set_real_ip_from 197.234.240.0/22;
set_real_ip_from 198.41.128.0/17;
set_real_ip_from 162.158.0.0/15;
set_real_ip_from 104.16.0.0/13;
set_real_ip_from 104.24.0.0/14;
set_real_ip_from 172.64.0.0/13;
set_real_ip_from 131.0.72.0/22;

real_ip_header CF-Connecting-IP;
EOF
    fi

    # Test Nginx configuration
    if ! nginx -t >> "$LOG_FILE" 2>&1; then
        FAILED_STEP="Nginx setup"
        FAILED_MESSAGE="Nginx configuration test failed"
        exit 1
    fi

    # Enable and restart Nginx
    systemctl enable nginx >> "$LOG_FILE" 2>&1
    systemctl restart nginx >> "$LOG_FILE" 2>&1

    log OK "Nginx configured and running"
    mark_step 11 ok
}

# =============================================================================
# Step 13: Systemd Service
# =============================================================================

step_systemd() {
    CURRENT_STEP=13
    log STEP "Setting up systemd service..."

    # Copy systemd service file
    if [[ -f "$INSTALL_DIR/deployment/sentinel.service" ]]; then
        cp "$INSTALL_DIR/deployment/sentinel.service" /etc/systemd/system/sentinel.service
        log OK "Systemd service file installed"
    else
        log INFO "Creating systemd service file..."
        cat > /etc/systemd/system/sentinel.service << EOF
[Unit]
Description=Sentinel AI Proxy
Requires=docker.service
After=docker.service network-online.target
Wants=network-online.target

[Service]
Type=oneshot
RemainAfterExit=yes
WorkingDirectory=$INSTALL_DIR
ExecStart=/usr/bin/docker compose up -d --remove-orphans
ExecStop=/usr/bin/docker compose down
TimeoutStartSec=120
TimeoutStopSec=30
Restart=on-failure
RestartSec=10

[Install]
WantedBy=multi-user.target
EOF
    fi

    # Reload systemd and enable service
    systemctl daemon-reload >> "$LOG_FILE" 2>&1
    systemctl enable sentinel >> "$LOG_FILE" 2>&1

    log OK "Systemd service enabled (will start on boot)"
    mark_step 12 ok
}

# =============================================================================
# Main Execution
# =============================================================================

main() {
    print_banner
    parse_args "$@"

    log INFO "Starting Sentinel deployment..."
    log INFO "Repository: $REPO_URL"
    log INFO "Branch: $BRANCH"
    log INFO "Domain: $DOMAIN"
    log INFO "Install directory: $INSTALL_DIR"
    [[ "$SKIP_FIREWALL" == true ]] && log WARN "Firewall configuration will be skipped"
    [[ "$SKIP_SECURITY" == true ]] && log WARN "Security hardening will be skipped"
    [[ "$SKIP_NGINX" == true ]] && log WARN "Nginx installation will be skipped"
    echo ""

    step_prerequisites
    step_system_packages
    step_ssh_setup
    step_docker
    step_firewall
    step_security
    step_clone_repo
    step_environment
    step_production_override
    step_docker_build
    step_verify_services
    step_nginx
    step_systemd

    log OK "Deployment completed successfully!"
}

main "$@"
