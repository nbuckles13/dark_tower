#!/bin/bash
# Dev-loop container entrypoint.
#
# Runs migrations, sets up Claude config, then sleeps to keep the container
# alive. Users interact via: podman exec -it <container> claude --dangerously-skip-permissions
set -euo pipefail

echo "=== Dark Tower dev container starting ==="

# Ensure CARGO_HOME is writable. PATH (incl. /work/infra/devloop) is set at
# the image level via Dockerfile ENV — required so `podman exec` sessions also
# pick it up, since exec doesn't run this entrypoint.
mkdir -p "${CARGO_HOME:-/tmp/cargo-home}"

# Wait for postgres to be ready.
# With named networks (ADR-0030), DB is at $DB_HOST:5432 via container DNS.
# With --network container: (legacy), DB is at localhost:5432.
DB_HOST="${DATABASE_URL##*@}"    # strip everything before @
DB_HOST="${DB_HOST%%:*}"         # strip port and everything after
DB_HOST="${DB_HOST:-localhost}"   # fallback
echo "Waiting for PostgreSQL at ${DB_HOST}..."
for i in $(seq 1 30); do
    if pg_isready -h "$DB_HOST" -p 5432 -U postgres -q 2>/dev/null; then
        echo "PostgreSQL ready."
        break
    fi
    if [ "$i" -eq 30 ]; then
        echo "WARNING: PostgreSQL not ready after 30s. Tests requiring DB may fail."
    fi
    sleep 1
done

# Run database migrations if sqlx-cli is available and DATABASE_URL is set
if command -v sqlx &>/dev/null && [ -n "${DATABASE_URL:-}" ] && [ -d "/work/migrations" ]; then
    echo "Running database migrations..."
    sqlx migrate run --source /work/migrations || echo "WARNING: Migration failed (may already be applied)"
fi

# Set up user-level Claude config files if mounted by devloop.sh
mkdir -p "${HOME}/.claude"
if [ -f /tmp/claude-user-settings.json ]; then
    # Runner-specific config (Stop hook, bg-wait ceiling) is NOT set here:
    # entrypoint.sh is baked into the image, so edits sit unbaked in-tree —
    # run-story preflight owns that patching (in-container, every run).
    cp /tmp/claude-user-settings.json "${HOME}/.claude/settings.json"
fi
if [ -f /tmp/claude-user-config.json ]; then
    # Patch installMethod to match how Claude is installed in the container (npm),
    # and disable auto-updates (entrypoint handles updates via npm update)
    jq '.installMethod = "npm" | .autoUpdates = false | .autoUpdatesProtectedForNative = false' /tmp/claude-user-config.json > "${HOME}/.claude.json"
fi
if [ -f /tmp/claude-credentials.json ]; then
    cp /tmp/claude-credentials.json "${HOME}/.claude/.credentials.json"
fi

# Activate pre-commit hooks in the clone (git clone --local doesn't copy local config)
if [ -d /work/.githooks ]; then
    git -C /work config core.hooksPath /work/.githooks
fi

# Update Claude Code to latest. The Dockerfile provides a base version; this
# keeps it current without rebuilds.
#
# FOREGROUND AND UNMASKED, deliberately (same reasoning as the pnpm block below).
# This was previously backgrounded with output discarded, which broke two things:
#   1. It raced the readiness signal, so run-story's preflight could sample
#      `claude --version` BEFORE the update landed. The story then ran on an
#      UNPROBED version, and the probe marker preflight wrote
#      (.substrate-probe-ok-<version>) named the OLD version — so every later
#      story read a stale marker and skipped the probe too. Silently.
#   2. `>/dev/null 2>&1` masked a failed or partial update entirely.
# A version the substrate probe never validated is exactly the risk the probe
# exists to eliminate, so the version must be settled before readiness.
# Non-fatal: an offline host should still get a usable container, but loudly.
echo "Updating Claude Code (blocks readiness so the version is settled before preflight samples it)..."
npm update -g @anthropic-ai/claude-code \
    || echo "WARNING: claude-code update failed — container will run the image's baked version. The run-story substrate probe binds whatever 'claude --version' reports, so this is safe but may be stale." >&2

# Materialize TS workspace deps if pnpm-lock is present and nx isn't yet installed.
# Foreground (blocks readiness) is intentional: silent no-op of TS wrappers when
# nx is missing is exactly the failure mode this closes — fail loudly here so it
# is visible before the devloop touches TS scripts. Placed last so the
# "Container ready" signal honestly reflects pnpm install completion.
if command -v pnpm &>/dev/null && [ -f /work/pnpm-lock.yaml ] && [ ! -x /work/node_modules/.bin/nx ]; then
    echo "Running pnpm install (first run on this host populates the pnpm-store cache, ~30-60s; subsequent devloops are 2-5s)..."
    pnpm install --frozen-lockfile --dir /work || {
        echo "ERROR: pnpm install failed — TS pipeline wrappers will not work in this devloop."
        echo "       To debug: podman run --rm -it --entrypoint bash darktower-dev:latest"
        exit 1
    }
fi

echo "=== Container ready. Attach with: podman exec -it <name> claude --dangerously-skip-permissions ==="

# If a command was passed (e.g., `podman run ... bash -c '...'`), run it.
# Otherwise keep the container alive for attach/detach via podman exec.
if [ $# -gt 0 ]; then
    exec "$@"
else
    exec sleep infinity
fi
