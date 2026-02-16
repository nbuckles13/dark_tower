# Dark Tower Dev-Loop Container
#
# Pre-built image with full Rust toolchain and development tools.
# Used by devloop.sh for isolated Claude Code execution.
#
# Build:  podman build -t darktower-dev:latest infra/devloop/
# Rebuild weekly or when toolchain/dependencies change.

FROM docker.io/library/rust:1.91-slim-bookworm

# System packages required for building (ring, bcrypt, protobuf), guards (jq, bc,
# python3-yaml for validate-infrastructure-metrics), and database tools
# (postgresql-client for pg_isready used by entrypoint + test.sh)
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    pkg-config \
    protobuf-compiler \
    git \
    curl \
    ca-certificates \
    jq \
    bc \
    postgresql-client \
    python3-yaml \
    && rm -rf /var/lib/apt/lists/*

# Nightly toolchain (required by guards/strip-test-code.sh)
RUN rustup toolchain install nightly --profile minimal

# Additional components for stable toolchain
RUN rustup component add llvm-tools-preview rustfmt clippy

# Cargo extensions for validation pipeline
RUN cargo install cargo-llvm-cov && \
    cargo install sqlx-cli --no-default-features --features postgres && \
    cargo install cargo-audit

# GitHub CLI (for reading PRs, issues — read-only use)
RUN curl -fsSL https://cli.github.com/packages/githubcli-archive-keyring.gpg \
    | dd of=/usr/share/keyrings/githubcli-archive-keyring.gpg && \
    echo "deb [arch=$(dpkg --print-architecture) signed-by=/usr/share/keyrings/githubcli-archive-keyring.gpg] https://cli.github.com/packages stable main" \
    | tee /etc/apt/sources.list.d/github-cli.list > /dev/null && \
    apt-get update && apt-get install -y --no-install-recommends gh && \
    rm -rf /var/lib/apt/lists/*

# Node.js 22 + Claude Code CLI
RUN curl -fsSL https://deb.nodesource.com/setup_22.x | bash - && \
    apt-get install -y --no-install-recommends nodejs && \
    npm install -g @anthropic-ai/claude-code && \
    rm -rf /var/lib/apt/lists/*

# Home directory for the mapped user (--userns=keep-id).
# Without this, HOME defaults to WORKDIR (/work) and Claude writes
# state files (.claude/, .claude.json) into the git checkout.
RUN mkdir -p /home/dev && chmod 777 /home/dev
ENV HOME=/home/dev

# Pre-create CARGO_HOME with open permissions so --userns=keep-id users can write.
# Named volumes mount over registry/ and git/ subdirs, but cargo audit and other
# tools write directly to CARGO_HOME (e.g., advisory-db/).
RUN mkdir -p /tmp/cargo-home && chmod 777 /tmp/cargo-home

# Entrypoint script
COPY entrypoint.sh /usr/local/bin/entrypoint.sh
RUN chmod +x /usr/local/bin/entrypoint.sh

WORKDIR /work

ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
