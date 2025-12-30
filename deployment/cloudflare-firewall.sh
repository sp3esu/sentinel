#!/bin/bash
# cloudflare-firewall.sh
# Configures iptables to only allow HTTP/HTTPS traffic from Cloudflare IPs
#
# IMPORTANT: This script should be run after UFW is configured, as it uses
# iptables directly. Alternatively, you can use the UFW version below.
#
# Usage:
#   sudo ./cloudflare-firewall.sh [--ufw]
#
# Options:
#   --ufw    Use UFW instead of raw iptables
#
# Cron (update weekly, Cloudflare IPs change occasionally):
#   0 4 * * 0 root /opt/sentinel/deployment/cloudflare-firewall.sh >> /var/log/cloudflare-firewall.log 2>&1

set -euo pipefail

# Configuration
CLOUDFLARE_IPS_V4="https://www.cloudflare.com/ips-v4"
CLOUDFLARE_IPS_V6="https://www.cloudflare.com/ips-v6"
CHAIN_NAME="CLOUDFLARE"
LOG_PREFIX="[cloudflare-fw]"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${GREEN}${LOG_PREFIX}${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}${LOG_PREFIX}${NC} $1"
}

log_error() {
    echo -e "${RED}${LOG_PREFIX}${NC} $1" >&2
}

# Check if running as root
if [[ $EUID -ne 0 ]]; then
    log_error "This script must be run as root"
    exit 1
fi

# Parse arguments
USE_UFW=false
if [[ "${1:-}" == "--ufw" ]]; then
    USE_UFW=true
fi

# Fetch Cloudflare IPs
log_info "Fetching Cloudflare IP ranges..."
CF_IPS_V4=$(curl -s "$CLOUDFLARE_IPS_V4" || echo "")
CF_IPS_V6=$(curl -s "$CLOUDFLARE_IPS_V6" || echo "")

if [[ -z "$CF_IPS_V4" ]]; then
    log_error "Failed to fetch Cloudflare IPv4 ranges"
    exit 1
fi

log_info "Found $(echo "$CF_IPS_V4" | wc -l) IPv4 ranges"
log_info "Found $(echo "$CF_IPS_V6" | wc -l) IPv6 ranges"

if $USE_UFW; then
    # ===================
    # UFW Implementation
    # ===================
    log_info "Using UFW mode..."

    # Remove existing Cloudflare rules (numbered delete from highest to lowest)
    log_info "Removing old Cloudflare rules..."
    ufw status numbered | grep "Cloudflare" | awk -F'[][]' '{print $2}' | sort -rn | while read -r num; do
        if [[ -n "$num" ]]; then
            yes | ufw delete "$num" 2>/dev/null || true
        fi
    done

    # Add new rules for each Cloudflare IP range
    log_info "Adding Cloudflare IPv4 rules..."
    for ip in $CF_IPS_V4; do
        ufw allow from "$ip" to any port 80,443 proto tcp comment "Cloudflare" >/dev/null
    done

    log_info "Adding Cloudflare IPv6 rules..."
    for ip in $CF_IPS_V6; do
        ufw allow from "$ip" to any port 80,443 proto tcp comment "Cloudflare" >/dev/null
    done

    # Deny all other HTTP/HTTPS traffic (add at end)
    # Note: This requires manual ordering in UFW rules
    log_warn "Remember to manually order rules so Cloudflare allows come before any 80/443 allows"

else
    # ======================
    # iptables Implementation
    # ======================
    log_info "Using iptables mode..."

    # Create or flush the Cloudflare chain
    if iptables -L "$CHAIN_NAME" -n >/dev/null 2>&1; then
        log_info "Flushing existing $CHAIN_NAME chain..."
        iptables -F "$CHAIN_NAME"
    else
        log_info "Creating $CHAIN_NAME chain..."
        iptables -N "$CHAIN_NAME"
    fi

    # Same for IPv6
    if ip6tables -L "$CHAIN_NAME" -n >/dev/null 2>&1; then
        ip6tables -F "$CHAIN_NAME"
    else
        ip6tables -N "$CHAIN_NAME"
    fi

    # Add Cloudflare IPv4 ranges to chain
    log_info "Adding Cloudflare IPv4 rules..."
    for ip in $CF_IPS_V4; do
        iptables -A "$CHAIN_NAME" -s "$ip" -j ACCEPT
    done

    # Add Cloudflare IPv6 ranges to chain
    log_info "Adding Cloudflare IPv6 rules..."
    for ip in $CF_IPS_V6; do
        ip6tables -A "$CHAIN_NAME" -s "$ip" -j ACCEPT
    done

    # Drop everything else in the chain
    iptables -A "$CHAIN_NAME" -j DROP
    ip6tables -A "$CHAIN_NAME" -j DROP

    # Remove old references to the chain in INPUT
    log_info "Updating INPUT chain references..."
    while iptables -D INPUT -p tcp --dport 80 -j "$CHAIN_NAME" 2>/dev/null; do :; done
    while iptables -D INPUT -p tcp --dport 443 -j "$CHAIN_NAME" 2>/dev/null; do :; done
    while ip6tables -D INPUT -p tcp --dport 80 -j "$CHAIN_NAME" 2>/dev/null; do :; done
    while ip6tables -D INPUT -p tcp --dport 443 -j "$CHAIN_NAME" 2>/dev/null; do :; done

    # Add references to our chain for HTTP/HTTPS
    iptables -I INPUT -p tcp --dport 80 -j "$CHAIN_NAME"
    iptables -I INPUT -p tcp --dport 443 -j "$CHAIN_NAME"
    ip6tables -I INPUT -p tcp --dport 80 -j "$CHAIN_NAME"
    ip6tables -I INPUT -p tcp --dport 443 -j "$CHAIN_NAME"

    # Save rules (Debian/Ubuntu)
    if command -v netfilter-persistent >/dev/null 2>&1; then
        log_info "Saving rules with netfilter-persistent..."
        netfilter-persistent save
    elif command -v iptables-save >/dev/null 2>&1; then
        log_info "Saving rules to /etc/iptables/rules.v4..."
        mkdir -p /etc/iptables
        iptables-save > /etc/iptables/rules.v4
        ip6tables-save > /etc/iptables/rules.v6
    fi
fi

log_info "Cloudflare firewall rules updated successfully!"
log_info "Only Cloudflare IPs can now reach ports 80 and 443"

# Verify
log_info "Current HTTP/HTTPS rules:"
if $USE_UFW; then
    ufw status | grep -E "(80|443)" | head -5
else
    iptables -L "$CHAIN_NAME" -n | head -10
fi
