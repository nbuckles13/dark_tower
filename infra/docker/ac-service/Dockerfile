# ========================================
# Multi-Stage Dockerfile for AC Service
# ========================================
#
# Uses cargo-chef for efficient dependency caching in Docker builds.
# https://github.com/LukeMathWalker/cargo-chef
#
# Build targets:
#   - Default: Minimal distroless (production-ready, no HEALTHCHECK)
#   - runtime-with-healthcheck: Debug distroless (includes HEALTHCHECK, for testing)
#
# Build commands (normally infra/kind/scripts/deploy.sh:build_content_tagged_image,
# which passes both build args; from the repo root by hand):
#   docker build -f infra/docker/ac-service/Dockerfile -t ac-service:prod \
#     --build-arg RUST_VERSION="$(. infra/lib/rust-toolchain.sh && rust_toolchain_version rust-toolchain.toml)" \
#     --build-arg CARGO_CHEF_VERSION=<deploy.sh's CARGO_CHEF_VERSION> .
#   add `--target runtime-with-healthcheck` (tag :debug) for the debug variant.
#
# Build args (both REQUIRED, no defaults — a missing one fails the build):
#   RUST_VERSION        The repo's ONE Rust version, derived from rust-toolchain.toml by
#                       infra/lib/rust-toolchain.sh. Unset yields `rust:-slim-bookworm`,
#                       an invalid reference.
#   CARGO_CHEF_VERSION  Exact cargo-chef pin, ONE value in deploy.sh. Unpinned,
#                       `cargo install` resolves the newest release, which can require
#                       a newer rustc than the pinned base and break every image at once.

# ========================================
# Stage 1: Chef base (install cargo-chef)
# ========================================
ARG RUST_VERSION
# Builder pinned to the runtime's Debian release (bookworm = distroless
# cc-debian12): the binary links the builder's glibc, and the floating `-slim`
# tag has moved on to trixie. setup.test.sh fails on builder/runtime drift.
FROM docker.io/library/rust:${RUST_VERSION}-slim-bookworm AS chef

# The cargo parallelism cap (build.jobs, .cargo/config.toml) applies to every
# compile, including this one: cargo reads config from the working directory up.
WORKDIR /build
COPY .cargo/ .cargo/
ARG CARGO_CHEF_VERSION
RUN test -n "${CARGO_CHEF_VERSION}" || { echo "CARGO_CHEF_VERSION build-arg is required (set by infra/kind/scripts/deploy.sh)" >&2; exit 1; } && \
    cargo install cargo-chef --locked --version "=${CARGO_CHEF_VERSION}"

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
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

# The cargo parallelism cap lives in .cargo/config.toml (build.jobs; rationale there).
# Copy it in BEFORE the dependency cook — the heaviest compilation — so that step is
# capped too, not only the final build after `COPY . .`.
COPY .cargo/ .cargo/

# Copy recipe from planner stage
COPY --from=planner /build/recipe.json recipe.json

# Build dependencies only - this layer is cached unless Cargo.toml/Cargo.lock change
RUN cargo chef cook --release --recipe-path recipe.json --package ac-service

# Copy actual source code
COPY . .

# Build the application (dependencies are already cached)
RUN cargo build --release --package ac-service

# Strip debug symbols to reduce binary size
RUN strip target/release/auth-controller

# ========================================
# Stage 4: Minimal Production Runtime (Default)
# ========================================
FROM gcr.io/distroless/cc-debian12 AS runtime

# Metadata labels
LABEL maintainer="Dark Tower Team" \
      version="0.1.0" \
      description="Dark Tower Authentication Controller - OAuth 2.0 service-to-service authentication" \
      org.opencontainers.image.source="https://github.com/your-org/dark_tower" \
      org.opencontainers.image.title="AC Service" \
      org.opencontainers.image.description="Authentication Controller for Dark Tower platform"

# Copy the binary from builder
COPY --from=builder --chown=nonroot:nonroot /build/target/release/auth-controller /usr/local/bin/auth-controller

# Copy migrations for runtime schema initialization
COPY --from=builder --chown=nonroot:nonroot /build/migrations /migrations

# Expose default port (HTTP/2 over TLS in production via Linkerd)
EXPOSE 8082

# Set environment variables (can be overridden at runtime)
ENV RUST_LOG=info \
    BIND_ADDRESS=0.0.0.0:8082

# Note: Distroless images run as non-root by default (uid 65532)
# No need to explicitly set USER as the distroless base already uses nonroot

# Health check note:
# This minimal distroless image doesn't include shell utilities required for HEALTHCHECK
# For Kubernetes deployments (primary target), use native httpGet probes:
#
#   livenessProbe:
#     httpGet:
#       path: /health
#       port: 8082
#     initialDelaySeconds: 10
#     periodSeconds: 30
#     timeoutSeconds: 3
#     failureThreshold: 3
#
#   readinessProbe:
#     httpGet:
#       path: /health
#       port: 8082
#     initialDelaySeconds: 5
#     periodSeconds: 10
#     timeoutSeconds: 3
#     failureThreshold: 3
#
# For standalone Docker with HEALTHCHECK needs, build with --target runtime-with-healthcheck

# Run the application
ENTRYPOINT ["/usr/local/bin/auth-controller"]

# ========================================
# Stage 5: Debug Runtime with HEALTHCHECK (Optional)
# ========================================
FROM gcr.io/distroless/cc-debian12:debug AS runtime-with-healthcheck

# Metadata labels
LABEL maintainer="Dark Tower Team" \
      version="0.1.0-debug" \
      description="Dark Tower Authentication Controller - Debug build with healthcheck support" \
      org.opencontainers.image.source="https://github.com/your-org/dark_tower" \
      org.opencontainers.image.title="AC Service (Debug)" \
      org.opencontainers.image.description="Authentication Controller with busybox shell for healthcheck"

# Copy the binary from builder
COPY --from=builder --chown=nonroot:nonroot /build/target/release/auth-controller /usr/local/bin/auth-controller

# Copy migrations for runtime schema initialization
COPY --from=builder --chown=nonroot:nonroot /build/migrations /migrations

# Expose default port
EXPOSE 8082

# Health check using busybox (available in :debug variant)
# Checks if the binary exists and is readable (basic liveness check)
# In production, Kubernetes httpGet probes are preferred
HEALTHCHECK --interval=30s --timeout=3s --start-period=10s --retries=3 \
    CMD ["/busybox/sh", "-c", "test -f /usr/local/bin/auth-controller"]

# Set environment variables
ENV RUST_LOG=info \
    BIND_ADDRESS=0.0.0.0:8082

# Run the application
ENTRYPOINT ["/usr/local/bin/auth-controller"]
