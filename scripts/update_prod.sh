#!/bin/bash
# =============================================================================
# Sentinel AI Proxy - Production Deployment Script
# =============================================================================
#
# Builds the Docker image locally, pushes to GHCR, and updates the VPS.
#
# Usage:
#   ./scripts/update_prod.sh
#
# Prerequisites:
#   - Docker logged into GHCR: docker login ghcr.io -u sp3esu
#   - SSH access to VPS configured
#
# =============================================================================

set -euo pipefail

IMAGE="ghcr.io/sp3esu/sentinel:latest"
VPS_HOST="178.216.200.189"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m'

log() { echo -e "${BLUE}[$(date '+%H:%M:%S')]${NC} $1"; }
success() { echo -e "${GREEN}[$(date '+%H:%M:%S')] ✓${NC} $1"; }
error() { echo -e "${RED}[$(date '+%H:%M:%S')] ✗${NC} $1"; exit 1; }

# Ensure we're in the project root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_ROOT"

echo ""
echo "═══════════════════════════════════════════════════════════"
echo "          SENTINEL PRODUCTION DEPLOYMENT"
echo "═══════════════════════════════════════════════════════════"
echo ""

# Step 1: Build image locally
log "Building Docker image..."
if ! docker build -t "$IMAGE" .; then
    error "Docker build failed"
fi
success "Image built"

# Step 2: Push to GHCR
log "Pushing to GHCR..."
if ! docker push "$IMAGE"; then
    error "Docker push failed (are you logged into ghcr.io?)"
fi
success "Image pushed to GHCR"

# Step 3: SSH to VPS, pull latest code, and update
log "Connecting to VPS..."
if ! ssh "${VPS_HOST}" "cd /opt/sentinel && sudo git pull && sudo ./scripts/update.sh"; then
    error "VPS update failed"
fi

echo ""
echo "═══════════════════════════════════════════════════════════"
success "Production deployment complete!"
echo "═══════════════════════════════════════════════════════════"
echo ""
