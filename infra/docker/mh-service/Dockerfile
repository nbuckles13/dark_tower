# ========================================
# Multi-Stage Dockerfile for Media Handler
# ========================================
#
# Uses cargo-chef for efficient dependency caching in Docker builds.
# https://github.com/LukeMathWalker/cargo-chef
#
# Build targets:
#   - Default: Minimal distroless (production-ready, no HEALTHCHECK)
#   - runtime-with-healthcheck: Debug distroless (includes HEALTHCHECK, for testing)
#
# Build commands:
#   docker build -t mh-service:prod .
#   docker build -t mh-service:debug --target runtime-with-healthcheck .

# ========================================
# Stage 1: Chef base (install cargo-chef)
# ========================================
ARG RUST_VERSION=1.91
FROM docker.io/library/rust:${RUST_VERSION}-slim AS chef

RUN cargo install cargo-chef

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# ========================================
# Stage 2: Planner (generate recipe.json)
# ========================================
FROM chef AS planner

# Copy entire project to generate dependency recipe
COPY . .

# Generate recipe.json - captures all dependency information
RUN cargo chef prepare --recipe-path recipe.json

# ========================================
# Stage 3: Builder (build with cached deps)
# ========================================
FROM chef AS builder

# Copy recipe from planner stage
COPY --from=planner /build/recipe.json recipe.json

# Build dependencies only - this layer is cached unless Cargo.toml/Cargo.lock change
RUN cargo chef cook --release --recipe-path recipe.json --package mh-service

# Copy actual source code
COPY . .

# Build the application (dependencies are already cached)
RUN cargo build --release --package mh-service

# Strip debug symbols to reduce binary size
RUN strip target/release/mh-service

# ========================================
# Stage 4: Minimal Production Runtime (Default)
# ========================================
FROM gcr.io/distroless/cc-debian12 AS runtime

# Metadata labels
LABEL maintainer="Dark Tower Team" \
      version="0.1.0" \
      description="Dark Tower Media Handler - Media forwarding and quality adaptation" \
      org.opencontainers.image.source="https://github.com/your-org/dark_tower" \
      org.opencontainers.image.title="Media Handler" \
      org.opencontainers.image.description="Media Handler for Dark Tower platform"

# Copy the binary from builder
COPY --from=builder --chown=nonroot:nonroot /build/target/release/mh-service /usr/local/bin/mh-service

# Expose ports (health/metrics, gRPC, WebTransport)
EXPOSE 8083
EXPOSE 50053
EXPOSE 4434

# Set environment variables (can be overridden at runtime)
ENV RUST_LOG=info \
    MH_HEALTH_BIND_ADDRESS=0.0.0.0:8083 \
    MH_GRPC_BIND_ADDRESS=0.0.0.0:50053 \
    MH_WEBTRANSPORT_BIND_ADDRESS=0.0.0.0:4434

# Note: Distroless images run as non-root by default (uid 65532)
# No need to explicitly set USER as the distroless base already uses nonroot

# Health check note:
# This minimal distroless image doesn't include shell utilities required for HEALTHCHECK
# For Kubernetes deployments (primary target), use native httpGet probes:
#
#   livenessProbe:
#     httpGet:
#       path: /health
#       port: 8083
#     initialDelaySeconds: 10
#     periodSeconds: 10
#     timeoutSeconds: 5
#     failureThreshold: 3
#
#   readinessProbe:
#     httpGet:
#       path: /ready
#       port: 8083
#     initialDelaySeconds: 5
#     periodSeconds: 5
#     timeoutSeconds: 3
#     failureThreshold: 3
#
# For standalone Docker with HEALTHCHECK needs, build with --target runtime-with-healthcheck

# Run the application
ENTRYPOINT ["/usr/local/bin/mh-service"]

# ========================================
# Stage 5: Debug Runtime with HEALTHCHECK (Optional)
# ========================================
FROM gcr.io/distroless/cc-debian12:debug AS runtime-with-healthcheck

# Metadata labels
LABEL maintainer="Dark Tower Team" \
      version="0.1.0-debug" \
      description="Dark Tower Media Handler - Debug build with healthcheck support" \
      org.opencontainers.image.source="https://github.com/your-org/dark_tower" \
      org.opencontainers.image.title="Media Handler (Debug)" \
      org.opencontainers.image.description="Media Handler with busybox shell for healthcheck"

# Copy the binary from builder
COPY --from=builder --chown=nonroot:nonroot /build/target/release/mh-service /usr/local/bin/mh-service

# Expose ports
EXPOSE 8083
EXPOSE 50053
EXPOSE 4434

# Health check using busybox (available in :debug variant)
# Checks if the binary exists and is readable (basic liveness check)
# In production, Kubernetes httpGet probes are preferred
HEALTHCHECK --interval=30s --timeout=3s --start-period=10s --retries=3 \
    CMD ["/busybox/sh", "-c", "test -f /usr/local/bin/mh-service"]

# Set environment variables
ENV RUST_LOG=info \
    MH_HEALTH_BIND_ADDRESS=0.0.0.0:8083 \
    MH_GRPC_BIND_ADDRESS=0.0.0.0:50053 \
    MH_WEBTRANSPORT_BIND_ADDRESS=0.0.0.0:4434

# Run the application
ENTRYPOINT ["/usr/local/bin/mh-service"]
