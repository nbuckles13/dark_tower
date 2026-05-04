# Dark Tower Dev-Loop Container
#
# Pre-built image with full Rust toolchain and development tools.
# Used by devloop.sh for isolated Claude Code execution.
#
# Build:  podman build -t darktower-dev:latest infra/devloop/
# Rebuild weekly or when toolchain/dependencies change.

FROM docker.io/library/rust:1.95-slim-bookworm

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
    socat \
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

# kubectl for Kind cluster access (ADR-0030)
# Version pinned to match Kind's default K8s version; sha256 verified at build time.
ARG KUBECTL_VERSION=v1.32.3
RUN curl -fsSL "https://dl.k8s.io/release/${KUBECTL_VERSION}/bin/linux/amd64/kubectl" \
        -o /usr/local/bin/kubectl \
    && curl -fsSL "https://dl.k8s.io/release/${KUBECTL_VERSION}/bin/linux/amd64/kubectl.sha256" \
        -o /tmp/kubectl.sha256 \
    && echo "$(cat /tmp/kubectl.sha256)  /usr/local/bin/kubectl" | sha256sum -c - \
    && chmod +x /usr/local/bin/kubectl \
    && rm -f /tmp/kubectl.sha256

# Node.js 22 + Claude Code CLI
RUN curl -fsSL https://deb.nodesource.com/setup_22.x | bash - && \
    apt-get install -y --no-install-recommends nodejs && \
    npm install -g @anthropic-ai/claude-code && \
    rm -rf /var/lib/apt/lists/*

# === Browser-client toolchain (R-35) ===
# Measured size delta: ~970 MB additional vs. prior image (2026-05-03 build).
# Story §139 estimated ~330 MB; the actual delta is ~3x the planning-time
# estimate because `npx playwright install chromium` pulls THREE blobs, not
# one: Chromium (~280 MB), Chromium Headless Shell (~280 MB, used in some
# Playwright headless modes), and ffmpeg (~50 MB), plus apt deps and fonts
# (~50 MB) and npm cache layers from the smoke step.
# A future cleanup pass could drop chromium_headless_shell-* and ffmpeg-*
# from /opt/ms-playwright if size becomes a constraint — see TODO.md.
# Measure: podman image inspect darktower-dev:latest --format '{{.Size}}'.

# pnpm via Corepack (Node 22 ships corepack; no apt install needed).
# COREPACK_ENABLE_DOWNLOAD_PROMPT=0 is persistent — without it, every first-use
# of pnpm in a running container prompts interactively to download the tarball.
# COREPACK_HOME pins the activation state to a system path. Without this,
# `corepack prepare --activate` writes to $HOME/.cache/node/corepack at build
# time (HOME=/root) but the runtime HOME is /home/dev (set below) — the
# activation is then unreachable and pnpm falls back to Corepack's built-in
# default version. /opt/corepack mirrors the shared-path pattern used for
# PLAYWRIGHT_BROWSERS_PATH below.
# `corepack prepare ... --activate` only registers the version; the actual
# download is lazy. The `pnpm --version` line forces the download into the
# image so containers ship with pnpm fully resolved.
# Version contract: PNPM_VERSION here MUST match `packageManager` in the
# repo-root package.json. Corepack reads packageManager when running pnpm
# inside /work, and a mismatch causes a runtime download (defeating the bake).
ENV COREPACK_ENABLE_DOWNLOAD_PROMPT=0 \
    COREPACK_HOME=/opt/corepack
ARG PNPM_VERSION=10.33.2
RUN mkdir -p /opt/corepack \
 && corepack enable \
 && corepack prepare pnpm@${PNPM_VERSION} --activate \
 && pnpm --version \
 && chmod -R a+rX /opt/corepack

# Playwright Chromium system deps.
# Story (R-35) §139 / §464 verbatim list, in the order given there:
#   libnss3 libatk-bridge2.0-0 libdrm2 libxkbcommon0 libxcomposite1
#   libxdamage1 libxfixes3 libxrandr2 libgbm1 libpango-1.0-0 libcairo2
#   libasound2 fonts-liberation fonts-noto-color-emoji
# Plus two additions (libatk1.0-0, libcups2) that Playwright's `--with-deps`
# pulls in for headless Chromium on Debian bookworm but the story list omits
# (canonical source: playwright/lib/server/registry/dependencies.ts). Without
# these the build-time Chromium smoke fails with missing-symbol errors.
# We install the explicit list rather than `playwright install --with-deps` so
# the apt set is auditable here and installed under our own cache discipline.
RUN apt-get update && apt-get install -y --no-install-recommends \
    libnss3 libatk-bridge2.0-0 libatk1.0-0 libcups2 libdrm2 \
    libxkbcommon0 libxcomposite1 libxdamage1 libxfixes3 libxrandr2 \
    libgbm1 libpango-1.0-0 libcairo2 libasound2 \
    fonts-liberation fonts-noto-color-emoji \
 && rm -rf /var/lib/apt/lists/*

# Build-time: Layer below writes Chromium under PLAYWRIGHT_BROWSERS_PATH.
# Runtime:    PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1 stops downstream `pnpm install`
#             inside the running container from re-downloading Chromium —
#             it must use the image-baked browser at /opt/ms-playwright.
ENV PLAYWRIGHT_BROWSERS_PATH=/opt/ms-playwright \
    PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1

# Pre-cache Chromium. Load-bearing: PLAYWRIGHT_VERSION is the contract version
# between this image (which bakes Chromium at /opt/ms-playwright) and the
# packages/web-app `playwright` devDep (story task #18). Bumping here REQUIRES
# bumping the consumer pin in lockstep — Playwright refuses to launch a Chromium
# revision that does not match its npm-package's recorded revision.
# Trust chain: the Chromium browser blob is fetched from Playwright's HTTPS CDN.
# Integrity is established transitively through the version-pinned `playwright`
# npm tarball (integrity-checked by npm), which carries the Chromium revision
# and download manifest. Playwright does not publish per-version sha256 for the
# browser blob, so equivalent verification to the kubectl block above is not
# applicable; this is the same trust posture as official mcr.microsoft.com/playwright
# images, acceptable inside the devloop sandbox per ADR-0025.
# `chmod -R a+rX` makes the cache world-readable + dir-traversable (least-privilege)
# so --userns=keep-id mapped users can read it. The cache is intentionally
# read-only at runtime; mutating the baked browser requires an image rebuild.
ARG PLAYWRIGHT_VERSION=1.49.0
RUN npx -y playwright@${PLAYWRIGHT_VERSION} install chromium \
 && chmod -R a+rX /opt/ms-playwright

# Build-time smoke: catches missing shared libs (e.g., libcups2, libatk1.0-0)
# that Playwright's install step does NOT detect (extraction != dlopen). Path
# resolved via `playwright install --dry-run` so future on-disk layout changes
# don't break this step silently — Playwright treats --dry-run output as a
# tooling contract.
RUN BROWSER_DIR="$(npx -y playwright@${PLAYWRIGHT_VERSION} install --dry-run chromium 2>&1 | awk '/^browser: chromium / {found=1; next} found && /Install location:/ {print $3; exit}')" \
 && test -x "${BROWSER_DIR}/chrome-linux/chrome" \
 && "${BROWSER_DIR}/chrome-linux/chrome" --version

# Rollback: rebuild from prior commit (no state migration; named volumes are
# toolchain-agnostic).

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
