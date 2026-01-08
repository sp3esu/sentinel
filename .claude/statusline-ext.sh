#!/bin/bash
# Sentinel Development Server Status Extension
# Polls the health endpoint and displays compact server status

# Colors are exported from parent script
# Available: $RED, $GREEN, $YELLOW, $CYAN, $DIM, $RESET, $WHITE, $LIGHT_BLUE, $MAGENTA

# Cache configuration
CACHE_FILE="/tmp/sentinel-statusline-health"
CACHE_MAX_AGE=5  # 5 seconds

# Server configuration
SENTINEL_PORT=${SENTINEL_PORT:-8080}
HEALTH_URL="http://localhost:${SENTINEL_PORT}/health"

# Function to fetch health data
fetch_health() {
    curl -s --max-time 2 "$HEALTH_URL" 2>/dev/null
}

# Function to format uptime (seconds to human readable)
format_uptime() {
    local seconds=$1
    local hours=$((seconds / 3600))
    local minutes=$(((seconds % 3600) / 60))

    if [ $hours -gt 0 ]; then
        printf "%dh %dm" "$hours" "$minutes"
    elif [ $minutes -gt 0 ]; then
        printf "%dm" "$minutes"
    else
        printf "%ds" "$seconds"
    fi
}

# Check cache validity
health_data=""
if [ -f "$CACHE_FILE" ]; then
    # macOS uses -f %m, Linux uses -c %Y
    if [[ "$OSTYPE" == "darwin"* ]]; then
        cache_mtime=$(stat -f %m "$CACHE_FILE" 2>/dev/null || echo 0)
    else
        cache_mtime=$(stat -c %Y "$CACHE_FILE" 2>/dev/null || echo 0)
    fi
    cache_age=$(($(date +%s) - cache_mtime))
    if [ $cache_age -lt $CACHE_MAX_AGE ]; then
        health_data=$(cat "$CACHE_FILE")
    fi
fi

# Fetch new data if cache is invalid
if [ -z "$health_data" ]; then
    health_data=$(fetch_health)
    if [ -n "$health_data" ]; then
        echo "$health_data" > "$CACHE_FILE"
    fi
fi

# Parse and display status
if [ -z "$health_data" ]; then
    # Server is offline or unreachable
    printf " ${DIM}|${RESET} ${DIM}[dev server: offline]${RESET}"
else
    # Parse JSON response using jq
    if ! command -v jq &> /dev/null; then
        printf " ${DIM}|${RESET} ${DIM}[jq required]${RESET}"
        exit 0
    fi

    status=$(echo "$health_data" | jq -r '.status // "unknown"')
    uptime=$(echo "$health_data" | jq -r '.uptime_seconds // 0')
    redis_latency=$(echo "$health_data" | jq -r '.checks.redis.latency_ms // 0')

    # Format uptime
    uptime_str=$(format_uptime "$uptime")

    # Choose color and symbol based on status
    case "$status" in
        "healthy")
            color=$GREEN
            symbol="✓"
            ;;
        "degraded")
            color=$YELLOW
            symbol="⚠"
            ;;
        "unhealthy")
            color=$RED
            symbol="✗"
            ;;
        *)
            color=$DIM
            symbol="?"
            ;;
    esac

    # Build status string
    printf " ${DIM}|${RESET} ${color}[%s %s${RESET} ${DIM}|${RESET} ${WHITE}%s${RESET} ${DIM}|${RESET} Redis: ${WHITE}%dms${RESET}${color}]${RESET}" \
        "$symbol" "$status" "$uptime_str" "$redis_latency"
fi
