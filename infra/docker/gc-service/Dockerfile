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
# Build commands (normally infra/kind/scripts/deploy.sh:build_content_tagged_image,
# which passes both build args; from the repo root by hand):
#   docker build -f infra/docker/gc-service/Dockerfile -t gc-service:prod \
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

# The cargo parallelism cap lives in .cargo/config.toml (build.jobs; rationale there).
# Copy it in BEFORE the dependency cook — the heaviest compilation — so that step is
# capped too, not only the final build after `COPY . .`.
COPY .cargo/ .cargo/

# Copy recipe from planner stage
COPY --from=planner /build/recipe.json recipe.json

# Build dependencies only - this layer is cached unless Cargo.toml/Cargo.lock change
# ONE builder for every service image (CI image-caching): this chef/planner/builder
# block is byte-identical in infra/docker/{ac,gc,mc,mh}-service/Dockerfile, so
# BuildKit computes the same cache keys and the workspace compiles once per change
# instead of once per image. One package per cargo invocation, deliberately: a
# single `cargo build -p a -p b` unifies dependency features across packages, so
# each binary could differ from `cargo build -p <svc>`. Sequential builds share
# the target dir, so crates with identical features still compile once.
RUN for p in ac-service gc-service mc-service mh-service; do \
        cargo chef cook --release --recipe-path recipe.json --package "$p" || exit 1; \
    done

# Copy actual source code
COPY . .

# Build the services in parallel, each in its own copy of the cooked target dir
# (cargo serialises builds that share one). Each build stays a separate
# `cargo build -p <svc>`, so every binary is exactly what that command produces;
# the gain is the four fat-LTO links (single-threaded) overlapping. Only the
# binaries leave this step; the copies are removed in the same layer.
#
# The four builds share ONE job budget, the cap in .cargo/config.toml (build.jobs,
# or CARGO_BUILD_JOBS as cargo itself prefers): a jobserver FIFO that every cargo
# inherits through CARGO_MAKEFLAGS, the protocol GNU make speaks. Each client
# holds one implicit token, so the FIFO carries (cap - 4) and at most `cap` rustc
# processes run across all four. A cap below 4 cannot be honoured in parallel and
# fails the build rather than silently exceeding it.
RUN set -u; \
    cap="${CARGO_BUILD_JOBS:-$(sed -n 's/^jobs[[:space:]]*=[[:space:]]*\([0-9][0-9]*\)[[:space:]]*$/\1/p' .cargo/config.toml)}"; \
    case "$cap" in ''|*[!0-9]*) echo "cannot read build.jobs from .cargo/config.toml (got '$cap')" >&2; exit 1 ;; esac; \
    [ "$cap" -ge 4 ] || { echo "build.jobs=$cap is below the 4 parallel service builds" >&2; exit 1; }; \
    mkfifo /tmp/jobserver && exec 3<>/tmp/jobserver; \
    i=4; while [ "$i" -lt "$cap" ]; do printf '+' >&3; i=$((i + 1)); done; \
    export CARGO_MAKEFLAGS="-j --jobserver-auth=fifo:/tmp/jobserver"; \
    ( peak=0; while :; do \
        n=$(cat /proc/[0-9]*/comm 2>/dev/null | grep -cx rustc); \
        [ "$n" -gt "$peak" ] && { peak=$n; echo "$peak" > /tmp/rustc-peak; }; sleep 0.2; \
      done ) & sampler=$!; \
    pids=""; \
    for p in ac-service gc-service mc-service mh-service; do \
        cp -a target "target-$p"; \
        CARGO_TARGET_DIR="target-$p" cargo build --release --package "$p" > "build-$p.log" 2>&1 & \
        pids="$pids $!"; \
    done; \
    rc=0; for pid in $pids; do wait "$pid" || rc=1; done; \
    kill "$sampler"; \
    for p in ac-service gc-service mc-service mh-service; do echo "=== $p"; cat "build-$p.log"; done; \
    echo "job cap $cap; peak concurrent rustc $(cat /tmp/rustc-peak 2>/dev/null || echo 0)"; \
    [ "$rc" -eq 0 ] || { echo "a service build failed (logs above)" >&2; exit 1; }; \
    cp target-ac-service/release/auth-controller target-gc-service/release/gc-service \
        target-mc-service/release/mc-service target-mh-service/release/mh-service target/release/ && \
    rm -rf target-*-service build-*.log

# Strip debug symbols to reduce binary size
RUN strip target/release/auth-controller target/release/gc-service \
    target/release/mc-service target/release/mh-service

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
COPY --from=builder --chown=nonroot:nonroot /build/target/release/gc-service /usr/local/bin/gc-service

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
ENTRYPOINT ["/usr/local/bin/gc-service"]

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
COPY --from=builder --chown=nonroot:nonroot /build/target/release/gc-service /usr/local/bin/gc-service

# Copy migrations for runtime schema initialization
COPY --from=builder --chown=nonroot:nonroot /build/migrations /migrations

# Expose ports
EXPOSE 8080
EXPOSE 50051

# Health check using busybox (available in :debug variant)
# Checks if the binary exists and is readable (basic liveness check)
# In production, Kubernetes httpGet probes are preferred
HEALTHCHECK --interval=30s --timeout=3s --start-period=10s --retries=3 \
    CMD ["/busybox/sh", "-c", "test -f /usr/local/bin/gc-service"]

# Set environment variables
ENV RUST_LOG=info \
    BIND_ADDRESS=0.0.0.0:8080 \
    GC_GRPC_BIND_ADDRESS=0.0.0.0:50051

# Run the application
ENTRYPOINT ["/usr/local/bin/gc-service"]
