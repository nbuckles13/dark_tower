# ========================================
# Multi-Stage Dockerfile for Global Controller
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
#   docker build -t global-controller:prod .
#   docker build -t global-controller:debug --target runtime-with-healthcheck .

# ========================================
# Stage 1: Chef base (install cargo-chef)
# ========================================
ARG RUST_VERSION=1.83
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
RUN cargo chef cook --release --recipe-path recipe.json --package global-controller

# Copy actual source code
COPY . .

# Build the application (dependencies are already cached)
RUN cargo build --release --package global-controller

# Strip debug symbols to reduce binary size
RUN strip target/release/global-controller

# ========================================
# Stage 4: Minimal Production Runtime (Default)
# ========================================
FROM gcr.io/distroless/cc-debian12 AS runtime

# Metadata labels
LABEL maintainer="Dark Tower Team" \
      version="0.1.0" \
      description="Dark Tower Global Controller - HTTP/3 API gateway and meeting management" \
      org.opencontainers.image.source="https://github.com/your-org/dark_tower" \
      org.opencontainers.image.title="Global Controller" \
      org.opencontainers.image.description="Global Controller for Dark Tower platform"

# Copy the binary from builder
COPY --from=builder --chown=nonroot:nonroot /build/target/release/global-controller /usr/local/bin/global-controller

# Copy migrations for runtime schema initialization
COPY --from=builder --chown=nonroot:nonroot /build/migrations /migrations

# Expose ports (HTTP API and gRPC)
EXPOSE 8080
EXPOSE 50051

# Set environment variables (can be overridden at runtime)
ENV RUST_LOG=info \
    BIND_ADDRESS=0.0.0.0:8080 \
    GC_GRPC_BIND_ADDRESS=0.0.0.0:50051

# Note: Distroless images run as non-root by default (uid 65532)
# No need to explicitly set USER as the distroless base already uses nonroot

# Health check note:
# This minimal distroless image doesn't include shell utilities required for HEALTHCHECK
# For Kubernetes deployments (primary target), use native httpGet probes:
#
#   livenessProbe:
#     httpGet:
#       path: /health
#       port: 8080
#     initialDelaySeconds: 10
#     periodSeconds: 10
#     timeoutSeconds: 5
#     failureThreshold: 3
#
#   readinessProbe:
#     httpGet:
#       path: /ready
#       port: 8080
#     initialDelaySeconds: 5
#     periodSeconds: 5
#     timeoutSeconds: 3
#     failureThreshold: 3
#
# For standalone Docker with HEALTHCHECK needs, build with --target runtime-with-healthcheck

# Run the application
ENTRYPOINT ["/usr/local/bin/global-controller"]

# ========================================
# Stage 5: Debug Runtime with HEALTHCHECK (Optional)
# ========================================
FROM gcr.io/distroless/cc-debian12:debug AS runtime-with-healthcheck

# Metadata labels
LABEL maintainer="Dark Tower Team" \
      version="0.1.0-debug" \
      description="Dark Tower Global Controller - Debug build with healthcheck support" \
      org.opencontainers.image.source="https://github.com/your-org/dark_tower" \
      org.opencontainers.image.title="Global Controller (Debug)" \
      org.opencontainers.image.description="Global Controller with busybox shell for healthcheck"

# Copy the binary from builder
COPY --from=builder --chown=nonroot:nonroot /build/target/release/global-controller /usr/local/bin/global-controller

# Copy migrations for runtime schema initialization
COPY --from=builder --chown=nonroot:nonroot /build/migrations /migrations

# Expose ports
EXPOSE 8080
EXPOSE 50051

# Health check using busybox (available in :debug variant)
# Checks if the binary exists and is readable (basic liveness check)
# In production, Kubernetes httpGet probes are preferred
HEALTHCHECK --interval=30s --timeout=3s --start-period=10s --retries=3 \
    CMD ["/busybox/sh", "-c", "test -f /usr/local/bin/global-controller"]

# Set environment variables
ENV RUST_LOG=info \
    BIND_ADDRESS=0.0.0.0:8080 \
    GC_GRPC_BIND_ADDRESS=0.0.0.0:50051

# Run the application
ENTRYPOINT ["/usr/local/bin/global-controller"]
