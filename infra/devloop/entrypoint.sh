#!/bin/bash
# Dev-loop container entrypoint.
#
# Runs migrations, sets up Claude config, then sleeps to keep the container
# alive. Users interact via: podman exec -it <container> claude --dangerously-skip-permissions
set -euo pipefail

echo "=== Dark Tower dev container starting ==="

# Ensure CARGO_HOME is writable and pre-installed tools remain on PATH.
# Add clone's infra/devloop to PATH so dev-cluster always matches the source.
mkdir -p "${CARGO_HOME:-/tmp/cargo-home}"
export PATH="/work/infra/devloop:/usr/local/cargo/bin:${PATH}"

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

# Update Claude Code to latest in the background (non-blocking).
# The Dockerfile provides a base version; this keeps it current without rebuilds.
npm update -g @anthropic-ai/claude-code >/dev/null 2>&1 &

echo "=== Container ready. Attach with: podman exec -it <name> claude --dangerously-skip-permissions ==="

# If a command was passed (e.g., `podman run ... bash -c '...'`), run it.
# Otherwise keep the container alive for attach/detach via podman exec.
if [ $# -gt 0 ]; then
    exec "$@"
else
    exec sleep infinity
fi
