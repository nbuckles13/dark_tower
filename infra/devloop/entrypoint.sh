#!/bin/bash
# Dev-loop container entrypoint.
#
# Runs migrations, sets up Claude config, then sleeps to keep the container
# alive. Users interact via: podman exec -it <container> claude --dangerously-skip-permissions
set -euo pipefail

echo "=== Dark Tower dev container starting ==="

# Ensure CARGO_HOME is writable and pre-installed tools remain on PATH
mkdir -p "${CARGO_HOME:-/tmp/cargo-home}"
export PATH="/usr/local/cargo/bin:${PATH}"

# Wait for postgres to be ready (sidecar in the same pod)
echo "Waiting for PostgreSQL..."
for i in $(seq 1 30); do
    if pg_isready -h localhost -p 5432 -U postgres -q 2>/dev/null; then
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
    # and disable auto-updates (pinned version in Dockerfile)
    jq '.installMethod = "npm" | .autoUpdates = false | .autoUpdatesProtectedForNative = false' /tmp/claude-user-config.json > "${HOME}/.claude.json"
fi
if [ -f /tmp/claude-credentials.json ]; then
    cp /tmp/claude-credentials.json "${HOME}/.claude/.credentials.json"
fi

echo "=== Container ready. Attach with: podman exec -it <name> claude --dangerously-skip-permissions ==="

# If a command was passed (e.g., `podman run ... bash -c '...'`), run it.
# Otherwise keep the container alive for attach/detach via podman exec.
if [ $# -gt 0 ]; then
    exec "$@"
else
    exec sleep infinity
fi
