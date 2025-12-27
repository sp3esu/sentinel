#!/bin/bash

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Project directory
PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$PROJECT_DIR"

echo -e "${GREEN}Starting Sentinel AI Proxy Development Environment${NC}"
echo "=================================================="

# Detect Docker Compose command
detect_docker_compose() {
    if docker compose version &> /dev/null; then
        echo "docker compose"
    elif command -v docker-compose &> /dev/null; then
        echo "docker-compose"
    else
        echo ""
    fi
}

DOCKER_COMPOSE=$(detect_docker_compose)

# Cleanup function
cleanup() {
    echo -e "\n${YELLOW}Shutting down...${NC}"

    if [ -n "$DOCKER_COMPOSE" ]; then
        $DOCKER_COMPOSE down
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
    if [ -n "$DOCKER_COMPOSE" ]; then
        echo -e "${GREEN}OK ($DOCKER_COMPOSE)${NC}"
    else
        echo -e "${RED}NOT FOUND${NC}"
        echo "Docker Compose is required but not installed."
        echo "Please install Docker Compose: https://docs.docker.com/compose/install/"
        exit 1
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

    # Enable debug logging by default in dev mode
    if [ -z "$RUST_LOG" ]; then
        export RUST_LOG="sentinel=debug,tower_http=debug"
        echo -e "  ${BLUE}RUST_LOG:${NC} $RUST_LOG (default for dev)"
    else
        echo -e "  ${BLUE}RUST_LOG:${NC} $RUST_LOG (from environment)"
    fi
}

# Build Docker images
build_images() {
    echo -e "\n${BLUE}Building Docker images (no cache)...${NC}"
    $DOCKER_COMPOSE build --no-cache
}

# Start services
start_services() {
    echo -e "\n${BLUE}Starting services...${NC}"
    $DOCKER_COMPOSE up -d

    echo -e "\n${YELLOW}Waiting for services to be healthy...${NC}"

    # Wait for Redis to be healthy
    echo -n "  Redis: "
    for i in {1..30}; do
        if $DOCKER_COMPOSE exec -T sentinel-redis redis-cli ping >/dev/null 2>&1; then
            echo -e "${GREEN}healthy${NC}"
            break
        fi
        if [ $i -eq 30 ]; then
            echo -e "${RED}failed${NC}"
            echo "Redis failed to start. Check logs with: $DOCKER_COMPOSE logs sentinel-redis"
            exit 1
        fi
        sleep 1
    done

    # Wait for Sentinel to be healthy
    echo -n "  Sentinel: "
    for i in {1..60}; do
        if curl -sf http://localhost:${SENTINEL_DOCKER_PORT:-8080}/health/live >/dev/null 2>&1; then
            echo -e "${GREEN}healthy${NC}"
            break
        fi
        if [ $i -eq 60 ]; then
            echo -e "${RED}failed${NC}"
            echo "Sentinel failed to start. Check logs with: $DOCKER_COMPOSE logs sentinel"
            exit 1
        fi
        sleep 1
    done
}

# Show service info
show_info() {
    SENTINEL_PORT=${SENTINEL_DOCKER_PORT:-8080}
    REDIS_PORT=${SENTINEL_REDIS_PORT:-6380}

    echo ""
    echo -e "${GREEN}=================================================="
    echo -e "Development environment is ready!"
    echo -e "==================================================${NC}"
    echo ""
    echo -e "  ${BLUE}Sentinel API:${NC}  http://localhost:${SENTINEL_PORT}"
    echo -e "  ${BLUE}Health Check:${NC}  http://localhost:${SENTINEL_PORT}/health"
    echo -e "  ${BLUE}Metrics:${NC}       http://localhost:${SENTINEL_PORT}/metrics"
    echo -e "  ${BLUE}Redis:${NC}         localhost:${REDIS_PORT}"
    echo ""
    echo -e "  ${YELLOW}Useful commands:${NC}"
    echo "    $DOCKER_COMPOSE logs -f          # Follow all logs"
    echo "    $DOCKER_COMPOSE logs -f sentinel # Follow Sentinel logs"
    echo "    $DOCKER_COMPOSE ps               # Show running services"
    echo "    $DOCKER_COMPOSE restart sentinel # Restart Sentinel"
    echo ""
    echo -e "${YELLOW}Press Ctrl+C to stop all services${NC}"
    echo ""
}

# Follow logs
follow_logs() {
    $DOCKER_COMPOSE logs -f
}

# Main execution
main() {
    check_docker
    check_docker_compose
    setup_env
    build_images
    start_services
    show_info
    follow_logs
}

main
