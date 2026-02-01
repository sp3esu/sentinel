# =============================================================================
# Sentinel AI Proxy - Multi-stage Dockerfile
# =============================================================================

# -----------------------------------------------------------------------------
# Stage 1: Builder
# -----------------------------------------------------------------------------
FROM rust:1.83-slim AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Create a new empty project for caching dependencies
WORKDIR /app

# Copy manifests first for better layer caching
COPY Cargo.toml Cargo.lock* ./

# Create dummy source files to build dependencies
# (includes export_openapi binary defined in Cargo.toml)
RUN mkdir -p src/bin && \
    echo 'fn main() { println!("Dummy"); }' > src/main.rs && \
    echo 'fn main() {}' > src/bin/export_openapi.rs

# Build dependencies only (this layer will be cached)
RUN cargo build --release && \
    rm -rf src target/release/deps/sentinel*

# Copy the actual source code
COPY src ./src

# Build the actual application
RUN cargo build --release

# -----------------------------------------------------------------------------
# Stage 2: Production Runtime
# -----------------------------------------------------------------------------
FROM debian:bookworm-slim AS runtime

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create a non-root user for security
RUN useradd --create-home --shell /bin/bash sentinel

# Set working directory
WORKDIR /app

# Copy the compiled binary from builder stage
COPY --from=builder /app/target/release/sentinel /app/sentinel

# Change ownership to non-root user
RUN chown -R sentinel:sentinel /app

# Switch to non-root user
USER sentinel

# Expose the application port
EXPOSE 8080

# Health check using curl to the liveness endpoint
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD curl --fail http://localhost:8080/health/live || exit 1

# Set the entrypoint
ENTRYPOINT ["/app/sentinel"]
