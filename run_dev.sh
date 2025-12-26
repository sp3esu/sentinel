#!/bin/bash

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Project directory
PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$PROJECT_DIR"

echo -e "${GREEN}Starting Sentinel AI Proxy Development Environment${NC}"
echo "=================================================="

# Cleanup function
cleanup() {
    echo -e "\n${YELLOW}Shutting down...${NC}"

    # Stop Redis container
    if docker ps -q --filter "name=sentinel-redis" | grep -q .; then
        echo "Stopping Redis container..."
        docker stop sentinel-redis >/dev/null 2>&1 || true
    fi

    echo -e "${GREEN}Cleanup complete.${NC}"
    exit 0
}

# Set trap for cleanup on exit
trap cleanup SIGINT SIGTERM EXIT

# Check for Docker
check_docker() {
    echo -n "Checking for Docker... "
    if ! command -v docker &> /dev/null; then
        echo -e "${RED}NOT FOUND${NC}"
        echo "Docker is required but not installed. Please install Docker first."
        exit 1
    fi

    if ! docker info &> /dev/null; then
        echo -e "${RED}NOT RUNNING${NC}"
        echo "Docker daemon is not running. Please start Docker first."
        exit 1
    fi
    echo -e "${GREEN}OK${NC}"
}

# Check for Docker Compose
check_docker_compose() {
    echo -n "Checking for Docker Compose... "
    if docker compose version &> /dev/null; then
        DOCKER_COMPOSE="docker compose"
        echo -e "${GREEN}OK (docker compose)${NC}"
    elif command -v docker-compose &> /dev/null; then
        DOCKER_COMPOSE="docker-compose"
        echo -e "${GREEN}OK (docker-compose)${NC}"
    else
        echo -e "${YELLOW}NOT FOUND${NC}"
        echo "Docker Compose not found. Will use docker run for Redis."
        DOCKER_COMPOSE=""
    fi
}

# Setup environment
setup_env() {
    echo -n "Checking environment... "
    if [ ! -f ".env" ]; then
        if [ -f ".env.example" ]; then
            cp .env.example .env
            echo -e "${YELLOW}Created .env from .env.example${NC}"
            echo -e "${YELLOW}Please update .env with your actual values if needed.${NC}"
        else
            echo -e "${RED}No .env or .env.example found${NC}"
            exit 1
        fi
    else
        echo -e "${GREEN}OK${NC}"
    fi

    # Source the environment
    set -a
    source .env
    set +a
}

# Start Redis
start_redis() {
    echo -n "Starting Redis... "

    # Check if Redis is already running
    if docker ps -q --filter "name=sentinel-redis" | grep -q .; then
        echo -e "${GREEN}Already running${NC}"
        return
    fi

    # Remove stopped container if exists
    docker rm sentinel-redis >/dev/null 2>&1 || true

    # Start Redis container
    docker run -d \
        --name sentinel-redis \
        -p 6379:6379 \
        redis:7-alpine \
        >/dev/null 2>&1

    # Wait for Redis to be ready
    for i in {1..30}; do
        if docker exec sentinel-redis redis-cli ping >/dev/null 2>&1; then
            echo -e "${GREEN}OK${NC}"
            return
        fi
        sleep 0.5
    done

    echo -e "${RED}Failed to start Redis${NC}"
    exit 1
}

# Check Rust toolchain
check_rust() {
    echo -n "Checking Rust toolchain... "
    if ! command -v cargo &> /dev/null; then
        echo -e "${RED}NOT FOUND${NC}"
        echo "Cargo is required but not installed. Please install Rust first."
        exit 1
    fi
    echo -e "${GREEN}OK${NC}"
}

# Run the application
run_app() {
    echo ""
    echo -e "${GREEN}Starting Sentinel...${NC}"
    echo "=================================================="

    # Check if cargo-watch is available
    if command -v cargo-watch &> /dev/null || cargo watch --version &> /dev/null 2>&1; then
        echo "Using cargo-watch for hot reload..."
        echo ""
        cargo watch -x run
    else
        echo -e "${YELLOW}cargo-watch not found. Install with: cargo install cargo-watch${NC}"
        echo "Running with cargo run (no hot reload)..."
        echo ""
        cargo run
    fi
}

# Main execution
main() {
    check_docker
    check_docker_compose
    check_rust
    setup_env
    start_redis
    run_app
}

main
