#!/bin/bash
# =============================================================================
# Sentinel AI Proxy - Manual Update Script
# =============================================================================
#
# Usage:
#   sudo ./update.sh              # Update to latest
#   sudo ./update.sh --rollback   # Rollback to previous image
#   sudo ./update.sh --check      # Check for updates without applying
#
# =============================================================================

set -euo pipefail

INSTALL_DIR="/opt/sentinel"
COMPOSE_FILE="docker-compose.prod.yml"
CONTAINER_NAME="sentinel-proxy"
HEALTH_ENDPOINT="http://127.0.0.1:8080/health/live"
HEALTH_TIMEOUT=60

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log() { echo -e "${BLUE}[$(date '+%H:%M:%S')]${NC} $1"; }
success() { echo -e "${GREEN}[$(date '+%H:%M:%S')] SUCCESS:${NC} $1"; }
error() { echo -e "${RED}[$(date '+%H:%M:%S')] ERROR:${NC} $1"; }
warn() { echo -e "${YELLOW}[$(date '+%H:%M:%S')] WARNING:${NC} $1"; }

ACTION="update"
while [[ $# -gt 0 ]]; do
    case "$1" in
        --rollback) ACTION="rollback"; shift ;;
        --check) ACTION="check"; shift ;;
        --help)
            echo "Usage: $0 [--rollback|--check|--help]"
            exit 0
            ;;
        *) error "Unknown option: $1"; exit 1 ;;
    esac
done

cd "$INSTALL_DIR"

get_current_digest() {
    docker inspect --format='{{.Image}}' "$CONTAINER_NAME" 2>/dev/null || echo "none"
}

check_health() {
    local timeout=$1
    local elapsed=0

    while [[ $elapsed -lt $timeout ]]; do
        if curl -sf "$HEALTH_ENDPOINT" &>/dev/null; then
            return 0
        fi
        sleep 2
        elapsed=$((elapsed + 2))
    done
    return 1
}

save_rollback_image() {
    local current_digest
    current_digest=$(get_current_digest)
    if [[ "$current_digest" != "none" ]]; then
        echo "$current_digest" > "$INSTALL_DIR/.rollback-image"
        log "Saved rollback image: $current_digest"
    fi
}

do_update() {
    log "Starting Sentinel update..."

    log "Checking current service health..."
    if ! check_health 10; then
        warn "Service is not healthy before update. Proceeding anyway..."
    else
        success "Service is healthy"
    fi

    save_rollback_image

    log "Pulling latest image..."
    if ! docker compose -f "$COMPOSE_FILE" pull sentinel; then
        error "Failed to pull latest image"
        exit 1
    fi

    local old_digest new_digest
    old_digest=$(get_current_digest)
    new_digest=$(docker inspect --format='{{.Id}}' ghcr.io/sp3esu/sentinel:latest 2>/dev/null || echo "unknown")

    if [[ "$old_digest" == "$new_digest" ]]; then
        success "Already running the latest version"
        exit 0
    fi

    log "New image detected. Restarting service..."

    if ! docker compose -f "$COMPOSE_FILE" up -d --no-deps --force-recreate sentinel; then
        error "Failed to restart service"
        error "Attempting automatic rollback..."
        do_rollback
        exit 1
    fi

    log "Waiting for service to become healthy..."
    if ! check_health "$HEALTH_TIMEOUT"; then
        error "Service failed health check after update"
        error "Attempting automatic rollback..."
        do_rollback
        exit 1
    fi

    success "Update completed successfully!"
    docker compose -f "$COMPOSE_FILE" ps
}

do_rollback() {
    log "Starting rollback..."

    if [[ ! -f "$INSTALL_DIR/.rollback-image" ]]; then
        error "No rollback image saved. Cannot rollback."
        exit 1
    fi

    local rollback_image
    rollback_image=$(cat "$INSTALL_DIR/.rollback-image")

    log "Rolling back to: $rollback_image"

    docker compose -f "$COMPOSE_FILE" stop sentinel
    docker compose -f "$COMPOSE_FILE" rm -f sentinel
    docker tag "$rollback_image" ghcr.io/sp3esu/sentinel:latest
    docker compose -f "$COMPOSE_FILE" up -d sentinel

    log "Waiting for rollback service to become healthy..."
    if ! check_health "$HEALTH_TIMEOUT"; then
        error "Rollback failed health check!"
        exit 1
    fi

    success "Rollback completed successfully!"
}

do_check() {
    log "Checking for updates..."

    docker compose -f "$COMPOSE_FILE" pull sentinel

    local current_digest new_digest
    current_digest=$(get_current_digest)
    new_digest=$(docker inspect --format='{{.Id}}' ghcr.io/sp3esu/sentinel:latest 2>/dev/null || echo "unknown")

    if [[ "$current_digest" == "$new_digest" ]]; then
        success "Already running the latest version"
    else
        warn "Update available!"
        echo "  Current: $current_digest"
        echo "  Latest:  $new_digest"
        echo ""
        echo "Run 'sudo $0' to apply the update"
    fi
}

case "$ACTION" in
    update) do_update ;;
    rollback) do_rollback ;;
    check) do_check ;;
esac
