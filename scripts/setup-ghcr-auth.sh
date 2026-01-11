#!/bin/bash
# =============================================================================
# Setup GitHub Container Registry Authentication
# =============================================================================
#
# This script configures Docker authentication for pulling private images
# from ghcr.io. Run this once after initial deployment.
#
# Requirements:
#   - GitHub Personal Access Token (PAT) with read:packages scope
#
# Usage:
#   sudo ./setup-ghcr-auth.sh
#
# =============================================================================

set -euo pipefail

echo "GitHub Container Registry Authentication Setup"
echo "=============================================="
echo ""
echo "This will configure Docker to pull images from ghcr.io"
echo ""
echo "You need a GitHub Personal Access Token (PAT) with 'read:packages' scope."
echo "Create one at: https://github.com/settings/tokens"
echo ""

read -p "GitHub Username: " GITHUB_USER
read -sp "GitHub PAT (hidden): " GITHUB_PAT
echo ""

echo "$GITHUB_PAT" | docker login ghcr.io -u "$GITHUB_USER" --password-stdin

if [[ $? -eq 0 ]]; then
    echo ""
    echo "SUCCESS: Docker authenticated with ghcr.io"
    echo ""
    echo "Credentials stored at: /root/.docker/config.json"
    echo "Watchtower will use these credentials to pull updated images."
else
    echo ""
    echo "ERROR: Authentication failed. Check your username and PAT."
    exit 1
fi
