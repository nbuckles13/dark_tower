#!/bin/bash
# devloop.sh — Isolated devloop execution with persistent containers.
#
# Creates a local git clone, spins up a dev container + postgres
# (sharing a network namespace), and drops you into Claude Code with
# --dangerously-skip-permissions.
#
# Containers persist between sessions — Ctrl-D exits Claude but
# they stay running. Re-run this script to re-attach.
#
# Usage:
#   ./devloop.sh [--rebuild] [--recreate] <task-slug> [base-branch]
#   ./devloop.sh --rebuild                              # rebuild image only
#   ./devloop.sh --refresh-creds <task-slug>
#
# Options:
#   --rebuild         Force rebuild the dev container image (exits if no task-slug)
#   --recreate        Destroy and recreate containers, preserving the local clone
#   --refresh-creds   Copy fresh OAuth credentials into a running container
#
# Examples:
#   ./devloop.sh td-42-rate-limiting                     # branches from current branch
#   ./devloop.sh td-42-rate-limiting main                # branches from main
#   ./devloop.sh --rebuild                              # just rebuild the image
#   ./devloop.sh --rebuild td-42-rate-limiting
#   ./devloop.sh --recreate td-42-rate-limiting         # new containers, same clone
#   ./devloop.sh --rebuild --recreate td-42-rate-limiting  # rebuild image + recreate
#   ./devloop.sh --refresh-creds td-42-rate-limiting   # from another terminal
#
# Prerequisites:
#   - podman installed
#   - Authenticated with Claude Code (run 'claude' once to log in)
#
# See: docs/decisions/adr-0025-containerized-devloop.md

set -euo pipefail

# ─── Configuration ──────────────────────────────────────────────

REBUILD_IMAGE=false
REFRESH_CREDS=false
RECREATE=false
while [[ "${1:-}" == --* ]]; do
    case "$1" in
        --rebuild) REBUILD_IMAGE=true; shift ;;
        --refresh-creds) REFRESH_CREDS=true; shift ;;
        --recreate) RECREATE=true; shift ;;
        *) echo "Unknown option: $1" >&2; exit 1 ;;
    esac
done

# --rebuild with no task-slug: just build the image and exit
if $REBUILD_IMAGE && [ -z "${1:-}" ]; then
    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    IMAGE="darktower-dev:latest"
    echo "Building dev container image..."
    OLD_IMAGE_ID=$(podman images -q "$IMAGE" 2>/dev/null || true)
    podman build -t "$IMAGE" "$SCRIPT_DIR"
    if [ -n "$OLD_IMAGE_ID" ] && [ "$OLD_IMAGE_ID" != "$(podman images -q "$IMAGE")" ]; then
        podman rmi "$OLD_IMAGE_ID" 2>/dev/null || true
    fi
    echo "Image rebuilt: ${IMAGE}"
    exit 0
fi

TASK_SLUG="${1:?Usage: devloop.sh [--rebuild] <task-slug> [base-branch]}"
# Kind cluster names must be DNS labels (a-z, 0-9, hyphens only).
if [[ ! "$TASK_SLUG" =~ ^[a-z0-9]([a-z0-9-]*[a-z0-9])?$ ]]; then
    echo "ERROR: Invalid task slug: '${TASK_SLUG}'" >&2
    echo "  Must be lowercase alphanumeric with hyphens (e.g., 'my-task-42')" >&2
    exit 1
fi
BASE_BRANCH="${2:-$(git rev-parse --abbrev-ref HEAD)}"

# Resolve paths relative to the repo root
REPO_ROOT="$(git rev-parse --show-toplevel)"
CLONE_DIR="${REPO_ROOT}/../worktrees/${TASK_SLUG}"
DEV_CONTAINER="devloop-${TASK_SLUG}-dev"
DB_CONTAINER="devloop-${TASK_SLUG}-db"
IMAGE="darktower-dev:latest"
BRANCH_NAME="feature/${TASK_SLUG}"
GITHUB_REMOTE="$(git -C "$REPO_ROOT" remote get-url origin)"

# Git identity — defaults to your git config
GIT_AUTHOR_NAME="${GIT_AUTHOR_NAME:-$(git config user.name)}"
GIT_AUTHOR_EMAIL="${GIT_AUTHOR_EMAIL:-$(git config user.email)}"

# Helper binary configuration (ADR-0030: host-side cluster helper)
# Separate --target-dir avoids cargo lock conflicts with container builds.
# Built from $REPO_ROOT (host source), NOT $CLONE_DIR — security invariant.
HELPER_TARGET_DIR="${REPO_ROOT}/target/devloop-helper"
HELPER_BINARY="${HELPER_TARGET_DIR}/release/devloop-helper"
HELPER_RUNTIME_DIR="/tmp/devloop-${TASK_SLUG}"

# ─── Quick actions (early exit) ───────────────────────────────────

refresh_credentials() {
    if [ ! -f "${HOME}/.claude/.credentials.json" ]; then
        echo "No credentials file found at ${HOME}/.claude/.credentials.json" >&2
        exit 1
    fi
    if ! podman container exists "$DEV_CONTAINER" 2>/dev/null; then
        echo "Container not running: ${DEV_CONTAINER}" >&2
        exit 1
    fi
    podman cp "${HOME}/.claude/.credentials.json" "${DEV_CONTAINER}:/home/dev/.claude/.credentials.json"
    echo "Credentials refreshed in ${DEV_CONTAINER}"
}

if $REFRESH_CREDS; then
    refresh_credentials
    exit 0
fi

# ─── Helper Functions ────────────────────────────────────────────

# Check if a PID belongs to a running devloop-helper process.
# Guards against PID recycling by checking /proc/$pid/cmdline.
is_helper_process_alive() {
    local pid="$1"
    kill -0 "$pid" 2>/dev/null && grep -q "devloop-helper" "/proc/$pid/cmdline" 2>/dev/null
}

cleanup() {
    echo "Destroying containers..."
    podman rm -f "$DEV_CONTAINER" 2>/dev/null || true
    podman rm -f "$DB_CONTAINER" 2>/dev/null || true

    # Remove named network (ADR-0030)
    podman network rm "devloop-${TASK_SLUG}-net" 2>/dev/null || true

    # Kill helper process (graceful SIGTERM, then force SIGKILL)
    if [ -f "$HELPER_RUNTIME_DIR/helper.pid" ]; then
        local pid
        pid=$(cat "$HELPER_RUNTIME_DIR/helper.pid")
        if is_helper_process_alive "$pid"; then
            echo "Stopping helper (PID $pid)..."
            kill "$pid" 2>/dev/null || true
            for i in $(seq 1 10); do
                kill -0 "$pid" 2>/dev/null || break
                sleep 0.5
            done
            kill -9 "$pid" 2>/dev/null || true
        fi
    fi

    # Delete Kind cluster if kind is available
    if command -v kind &>/dev/null; then
        echo "Deleting Kind cluster: devloop-${TASK_SLUG}..."
        kind delete cluster --name "devloop-${TASK_SLUG}" 2>/dev/null || true
    fi

    # Remove helper runtime directory
    rm -rf "$HELPER_RUNTIME_DIR"

    # Release port reservation
    rm -rf "$HOME/.cache/devloop/devloop-${TASK_SLUG}"
    local registry="$HOME/.cache/devloop/port-registry.json"
    if [ -f "$registry" ] && command -v jq &>/dev/null; then
        jq --arg slug "$TASK_SLUG" '.entries |= map(select(.slug != $slug))' "$registry" > "${registry}.tmp" \
            && mv "${registry}.tmp" "$registry"
    fi

    echo "Removing clone: ${CLONE_DIR}"
    rm -rf "$CLONE_DIR"
    echo "Cleaned up."
}

# Build the devloop-helper binary (ADR-0030).
# Always rebuilds — cargo no-ops if source unchanged (~0.1s), and stale
# binaries cause hard-to-debug port-map issues.
build_helper() {
    echo "Building devloop-helper..."
    cargo build --release -p devloop-helper \
        --target-dir "$HELPER_TARGET_DIR" \
        --manifest-path "$REPO_ROOT/Cargo.toml"
}

# Detect the podman host-gateway IP address (ADR-0030).
# Detection order per ADR-0030 Section 2:
#   1. podman info (authoritative, no container needed)
#   2. Parse /etc/hosts for host.containers.internal
#   3. Fail with actionable error
detect_host_gateway_ip() {
    local gw_ip

    # Method 1: podman info (most authoritative — queries podman directly)
    gw_ip=$(podman info --format '{{.Host.NetworkBackendInfo.DNS.HostGatewayIP}}' 2>/dev/null)
    if [ -n "$gw_ip" ] && [ "$gw_ip" != "<nil>" ] && [ "$gw_ip" != "<no value>" ]; then
        echo "$gw_ip"
        return 0
    fi

    # Method 2: check loopback interface for podman's host-gateway address
    gw_ip=$(ip addr show lo 2>/dev/null | grep -oP '10\.255\.\d+\.\d+' | head -1)
    if [ -n "$gw_ip" ]; then
        echo "$gw_ip"
        return 0
    fi

    # Method 3: parse /etc/hosts for host.containers.internal
    gw_ip=$(grep 'host.containers.internal' /etc/hosts 2>/dev/null | awk '{print $1}' | head -1)
    if [ -n "$gw_ip" ]; then
        echo "$gw_ip"
        return 0
    fi

    echo "ERROR: Cannot detect host-gateway IP." >&2
    echo "  Method 1: 'podman info --format {{.Host.NetworkBackendInfo.DNS.HostGatewayIP}}' returned empty" >&2
    echo "  Method 2: 'ip addr show lo' has no 10.255.x.x address" >&2
    echo "  Method 3: /etc/hosts has no host.containers.internal entry" >&2
    echo "  This is required for container-to-Kind networking (ADR-0030)." >&2
    return 1
}

# Launch the helper as a background process, reusing if already alive.
launch_helper() {
    local pid_file="$HELPER_RUNTIME_DIR/helper.pid"

    # Check if already running
    if [ -f "$pid_file" ]; then
        local pid
        pid=$(cat "$pid_file")
        if is_helper_process_alive "$pid"; then
            echo "Helper already running (PID $pid)"
            return 0
        fi
        echo "Stale helper PID file found, will relaunch..."
    fi

    # Launch helper binary as a background process.
    #
    # --project-root points at CLONE_DIR (NOT REPO_ROOT) so service rebuilds,
    # setup.sh, and kind-config generation reflect the devloop's branch state
    # — `/work` inside the dev container is mounted from CLONE_DIR, so this
    # is what edits land in. The helper *binary* is still compiled from
    # REPO_ROOT (see build_helper above) — that pin is what protects the
    # binary from container tampering. See ADR-0030 §"Build-context
    # trichotomy" for the full security model.
    local helper_args=("$TASK_SLUG" --project-root "$CLONE_DIR")
    if [ -n "${HOST_GATEWAY_IP:-}" ]; then
        helper_args+=(--host-gateway-ip "$HOST_GATEWAY_IP")
    fi
    "$HELPER_BINARY" "${helper_args[@]}" \
        2>>"$HELPER_RUNTIME_DIR/helper-stderr.log" &
    local helper_pid=$!

    # Wait for socket to appear (up to 10s)
    local socket_path="$HELPER_RUNTIME_DIR/helper.sock"
    for i in $(seq 1 20); do
        if [ -S "$socket_path" ]; then
            echo "Helper ready (PID $helper_pid)"
            return 0
        fi
        # Check if helper exited early (crash on startup)
        if ! kill -0 "$helper_pid" 2>/dev/null; then
            echo "ERROR: Helper process exited during startup" >&2
            if [ -f "$HELPER_RUNTIME_DIR/helper-stderr.log" ]; then
                echo "Helper stderr:" >&2
                tail -20 "$HELPER_RUNTIME_DIR/helper-stderr.log" >&2
            fi
            return 1
        fi
        sleep 0.5
    done

    echo "ERROR: Helper socket not ready after 10s" >&2
    kill "$helper_pid" 2>/dev/null || true
    return 1
}

# Detect orphaned Kind clusters with no corresponding running devloop container.
detect_orphan_clusters() {
    if ! command -v kind &>/dev/null; then
        return 0
    fi

    local clusters
    clusters=$(kind get clusters 2>/dev/null | grep "^devloop-" || true)
    if [ -z "$clusters" ]; then
        return 0
    fi

    while IFS= read -r cluster; do
        local slug="${cluster#devloop-}"
        local dev_container="devloop-${slug}-dev"
        local helper_pid_file="/tmp/${cluster}/helper.pid"

        # A cluster is only orphaned if BOTH the dev container is not running
        # AND the helper process is dead/missing. This avoids a race where
        # devloop.sh is mid-startup (helper launched, container not yet created).
        local container_alive=false
        local helper_alive=false
        if podman ps --format "{{.Names}}" 2>/dev/null | grep -q "^${dev_container}$"; then
            container_alive=true
        fi
        if [ -f "$helper_pid_file" ]; then
            local hpid
            hpid=$(cat "$helper_pid_file")
            if is_helper_process_alive "$hpid"; then
                helper_alive=true
            fi
        fi

        if ! $container_alive && ! $helper_alive; then
            if [[ -t 0 ]]; then
                echo "Orphaned Kind cluster found: $cluster (no running container or helper)"
                read -p "  Delete? [y/N] " -n 1 -r
                echo
                if [[ $REPLY =~ ^[Yy]$ ]]; then
                    echo "  Deleting $cluster..."
                    kind delete cluster --name "$cluster" 2>/dev/null || true
                    rm -rf "/tmp/${cluster}" 2>/dev/null || true
                    podman network rm "devloop-${slug}-net" 2>/dev/null || true
                    # Clean stale port registry entry
                    local registry="$HOME/.cache/devloop/port-registry.json"
                    if [ -f "$registry" ] && command -v jq &>/dev/null; then
                        jq --arg slug "$slug" '.entries |= map(select(.slug != $slug))' "$registry" > "${registry}.tmp" \
                            && mv "${registry}.tmp" "$registry"
                    fi
                fi
            else
                echo "WARNING: Orphaned Kind cluster: $cluster (no running container or helper)"
            fi
        fi
    done <<< "$clusters"
}

menu_reenter_or_cleanup() {
    echo "  [r] Re-enter container"
    echo "  [d] Destroy containers and clone (un-pushed changes will be lost)"
    echo "  [q] Quit (containers stay running)"
    read -p "Choice: " -n 1 -r
    echo
    case $REPLY in
        r|R) exec "$0" "$TASK_SLUG" "$BASE_BRANCH" ;;
        d|D) cleanup ;;
        *) echo "Containers still running. Re-enter with: $0 ${TASK_SLUG}" ;;
    esac
}

is_container_running() {
    podman ps --format "{{.Names}}" 2>/dev/null | grep -q "^${1}$"
}

# Ensure the per-devloop git clone exists at $CLONE_DIR.
#
# Must run BEFORE launch_helper because the helper's --project-root points
# at $CLONE_DIR (so service rebuilds and setup.sh see the devloop's branch
# state, not the user's main checkout). Idempotent: no-ops if the clone
# already exists.
ensure_clone() {
    if [ -d "$CLONE_DIR" ]; then
        echo "Clone already exists: ${CLONE_DIR}"
        return 0
    fi

    # Ensure the branch exists locally before cloning.
    # If it only exists on the remote (e.g., PR iteration), fetch it first.
    if ! git show-ref --verify --quiet "refs/heads/${BRANCH_NAME}"; then
        if git ls-remote --exit-code --heads "$GITHUB_REMOTE" "$BRANCH_NAME" &>/dev/null; then
            echo "Fetching remote branch: ${BRANCH_NAME}"
            git fetch "$GITHUB_REMOTE" "${BRANCH_NAME}:${BRANCH_NAME}"
        fi
    fi

    echo "Creating local clone: ${CLONE_DIR}"
    if git show-ref --verify --quiet "refs/heads/${BRANCH_NAME}"; then
        # Branch exists — clone with it
        git clone --local --branch "$BRANCH_NAME" "$REPO_ROOT" "$CLONE_DIR"
    else
        # Fresh start — clone base branch, create the feature branch
        git clone --local --branch "$BASE_BRANCH" "$REPO_ROOT" "$CLONE_DIR"
        git -C "$CLONE_DIR" checkout -b "$BRANCH_NAME"
    fi
    # Point origin at GitHub (not the local repo)
    git -C "$CLONE_DIR" remote set-url origin "$GITHUB_REMOTE"
}

# ─── Validate Prerequisites ─────────────────────────────────────

if ! command -v podman &>/dev/null; then
    echo "ERROR: podman is required. Install with: sudo apt install podman" >&2
    exit 1
fi

if [ -z "${ANTHROPIC_API_KEY:-}" ] && [ ! -f "${HOME}/.claude/.credentials.json" ]; then
    echo "ERROR: No authentication found. Either set ANTHROPIC_API_KEY or log in with 'claude' first." >&2
    exit 1
fi

# Check inotify limits for Kind multi-cluster support
if [ -f /proc/sys/fs/inotify/max_user_instances ]; then
    INOTIFY_INSTANCES=$(cat /proc/sys/fs/inotify/max_user_instances)
    if [ "$INOTIFY_INSTANCES" -lt 512 ]; then
        echo "WARNING: fs.inotify.max_user_instances=${INOTIFY_INSTANCES} (recommend 1024+)" >&2
        echo "  Kind clusters may fail with 'too many open files'." >&2
        echo "  Fix: sudo sysctl fs.inotify.max_user_instances=1024" >&2
        echo ""
    fi
fi

# ─── Build image if needed ───────────────────────────────────────

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

if $REBUILD_IMAGE || ! podman image exists "$IMAGE"; then
    echo "Building dev container image..."
    OLD_IMAGE_ID=$(podman images -q "$IMAGE" 2>/dev/null || true)
    podman build -t "$IMAGE" "$SCRIPT_DIR"
    if [ -n "$OLD_IMAGE_ID" ] && [ "$OLD_IMAGE_ID" != "$(podman images -q "$IMAGE")" ]; then
        podman rmi "$OLD_IMAGE_ID" 2>/dev/null || true
    fi
fi

# ─── Helper setup (ADR-0030/0031) ─────────────────────────────

# Ensure the devloop's git clone exists BEFORE launching the helper.
# The helper's --project-root points at CLONE_DIR, so the clone has to
# exist when the helper starts (see ADR-0030 §"Build-context trichotomy").
ensure_clone

# Build and launch the host-side cluster helper if kind is available.
# Without kind, devloop.sh works exactly as before (compile/test only, no cluster).
# The host-gateway IP (ADR-0030) is required for Kind NodePort binding —
# without it, cluster creation would produce invalid listenAddress values.
EXTRA_PODMAN_ARGS=()
HOST_GATEWAY_IP=""

if command -v kind &>/dev/null; then
    # Detect host-gateway IP first — this gates all cluster features (ADR-0030).
    # An empty HOST_GATEWAY_IP would produce listenAddress: "" in the Kind config,
    # which could bind to 0.0.0.0 (explicitly prohibited by ADR-0030 Section 7).
    if HOST_GATEWAY_IP=$(detect_host_gateway_ip); then
        echo "Host-gateway IP: ${HOST_GATEWAY_IP}"
    else
        echo "ERROR: Host-gateway IP detection failed. Cluster features disabled." >&2
        echo "devloop will continue without Kind cluster support." >&2
        HOST_GATEWAY_IP=""
    fi

    if [ -n "$HOST_GATEWAY_IP" ]; then
        detect_orphan_clusters
        build_helper
        # Ensure runtime dir exists before launching (helper creates it too,
        # but we need it for the stderr log redirect)
        mkdir -p -m 0700 "$HELPER_RUNTIME_DIR"
        if launch_helper; then
            # Mount the helper runtime directory into the container. This is a read-write
            # mount because: (1) the unix socket requires rw for client connections, and
            # (2) files like kubeconfig/ports.json are created after container start (by
            # dev-cluster setup) so individual ro file mounts aren't possible (files must
            # exist at mount time). Accepted risk: container can modify host-generated
            # files, but blast radius is limited to the dev session's own state.
            EXTRA_PODMAN_ARGS+=(-v "$HELPER_RUNTIME_DIR:/tmp/devloop:Z")
            EXTRA_PODMAN_ARGS+=(-e "KUBECONFIG=/tmp/devloop/kubeconfig")
        else
            echo "ERROR: Failed to start devloop helper. Cluster features disabled." >&2
            echo "devloop will continue without Kind cluster support." >&2
        fi
    fi
fi

# ─── Phase 1: Setup (idempotent) ────────────────────────────────

# --recreate: tear down containers but keep the clone
if $RECREATE; then
    echo "Recreating containers (clone preserved)..."
    podman rm -f "$DEV_CONTAINER" 2>/dev/null || true
    podman rm -f "$DB_CONTAINER" 2>/dev/null || true
    podman network rm "devloop-${TASK_SLUG}-net" 2>/dev/null || true
fi

if ! is_container_running "$DEV_CONTAINER"; then
    echo "=== Setting up containers for: ${TASK_SLUG} ==="

    # Clone is ensured earlier (before the helper launch) — see ensure_clone().

    # Clean up any stopped containers from a previous run
    podman rm -f "$DEV_CONTAINER" 2>/dev/null || true
    podman rm -f "$DB_CONTAINER" 2>/dev/null || true

    # Create named network for container DNS (ADR-0030).
    # Named networks allow DB access via container name instead of localhost,
    # and enable host.containers.internal routing to host-gateway-bound ports.
    NETWORK_NAME="devloop-${TASK_SLUG}-net"
    podman network create "$NETWORK_NAME" 2>/dev/null || true

    # Start postgres container
    echo "Starting PostgreSQL..."
    podman run -d --name "$DB_CONTAINER" \
        --network "$NETWORK_NAME" \
        -e POSTGRES_PASSWORD=postgres \
        -e POSTGRES_DB=dark_tower_test \
        docker.io/library/postgres:16-bookworm

    # Mount user-level Claude config files if they exist (read-only, to fixed paths
    # that entrypoint.sh will copy to the correct $HOME inside the container)
    if [ -f "${HOME}/.claude/settings.json" ]; then
        EXTRA_PODMAN_ARGS+=(-v "${HOME}/.claude/settings.json:/tmp/claude-user-settings.json:ro")
    fi
    if [ -f "${HOME}/.claude.json" ]; then
        EXTRA_PODMAN_ARGS+=(-v "${HOME}/.claude.json:/tmp/claude-user-config.json:ro")
    fi
    if [ -f "${HOME}/.claude/.credentials.json" ]; then
        EXTRA_PODMAN_ARGS+=(-v "${HOME}/.claude/.credentials.json:/tmp/claude-credentials.json:ro")
    fi
    if [ -n "${ANTHROPIC_API_KEY:-}" ]; then
        EXTRA_PODMAN_ARGS+=(-e "ANTHROPIC_API_KEY=${ANTHROPIC_API_KEY}")
    fi

    # Start dev container on the named network (ADR-0030).
    # Uses container DNS to reach postgres via container name.
    # host.containers.internal routes to host-gateway-bound Kind NodePorts.
    echo "Starting dev container..."
    podman run -d --name "$DEV_CONTAINER" \
        --userns=keep-id \
        --network "$NETWORK_NAME" \
        -v "$(realpath "$CLONE_DIR"):/work:Z" \
        -v cargo-registry:/tmp/cargo-home/registry \
        -v cargo-git:/tmp/cargo-home/git \
        -e CARGO_HOME=/tmp/cargo-home \
        ${EXTRA_PODMAN_ARGS[@]+"${EXTRA_PODMAN_ARGS[@]}"} \
        -e DATABASE_URL="postgresql://postgres:postgres@${DB_CONTAINER}:5432/dark_tower_test" \
        -e AC_MASTER_KEY="AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=" \
        -e GIT_AUTHOR_NAME="${GIT_AUTHOR_NAME}" \
        -e GIT_AUTHOR_EMAIL="${GIT_AUTHOR_EMAIL}" \
        -e GIT_COMMITTER_NAME="${GIT_AUTHOR_NAME}" \
        -e GIT_COMMITTER_EMAIL="${GIT_AUTHOR_EMAIL}" \
        -e RUST_BACKTRACE=1 \
        -e CARGO_TERM_COLOR=always \
        "$IMAGE"

    # Wait for entrypoint to complete setup
    echo "Waiting for container initialization..."
    sleep 5

    echo "=== Containers ready ==="
else
    echo "=== Containers already running: ${DEV_CONTAINER} ==="
fi

# ─── Infrastructure health check (ADR-0030 Step 6) ──────────────

# Verify helper and cluster health on every entry (including re-attach).
# This ensures the infrastructure is ready for env-tests in the devloop skill.
if [ -n "$HOST_GATEWAY_IP" ] && command -v kind &>/dev/null; then
    INFRA_STATUS="ok"

    # 1. Check if helper PID is alive — restart if dead
    if [ -f "$HELPER_RUNTIME_DIR/helper.pid" ]; then
        HELPER_PID=$(cat "$HELPER_RUNTIME_DIR/helper.pid")
        if ! is_helper_process_alive "$HELPER_PID"; then
            echo "Helper process dead (PID $HELPER_PID), restarting..."
            build_helper
            if launch_helper; then
                INFRA_STATUS="helper=restarted"
            else
                echo "WARNING: Failed to restart helper. Cluster features may not work." >&2
                INFRA_STATUS="helper=failed"
            fi
        fi
    fi

    # 2. Check cluster readiness and trigger eager setup if needed
    CLUSTER_NAME="devloop-${TASK_SLUG}"
    PORTS_FILE_PATH="$HELPER_RUNTIME_DIR/ports.json"
    NEEDS_SETUP=false
    if ! kind get clusters 2>/dev/null | grep -q "^${CLUSTER_NAME}$"; then
        echo "No Kind cluster found for ${TASK_SLUG}, starting setup in background..."
        NEEDS_SETUP=true
    elif [ ! -f "$PORTS_FILE_PATH" ]; then
        echo "Ports file missing, running setup in background (idempotent)..."
        NEEDS_SETUP=true
    fi

    if $NEEDS_SETUP; then
        # Subshell ensures setup.pid is cleaned up when setup finishes,
        # preventing stale PID issues on re-attach (PID recycling).
        (podman exec "$DEV_CONTAINER" dev-cluster setup \
            >> "$HELPER_RUNTIME_DIR/eager-setup.log" 2>&1; \
            rm -f "$HELPER_RUNTIME_DIR/setup.pid") &
        EAGER_SETUP_PID=$!
        echo "$EAGER_SETUP_PID" > "$HELPER_RUNTIME_DIR/setup.pid"
        INFRA_STATUS="cluster=setup-pending"
    elif [ "$INFRA_STATUS" = "ok" ]; then
        INFRA_STATUS="helper=alive cluster=ready"
    fi

    echo "Infrastructure: ${INFRA_STATUS}"
fi

# ─── Phase 2: Attach ────────────────────────────────────────────

echo ""
# Set terminal title to devloop slug
printf '\033]0;devloop: %s\007' "$TASK_SLUG"

echo "Dropping into Claude Code..."
echo "(Ctrl-D or /exit to detach — containers stay running)"
echo ""

# Refresh OAuth credentials before each attach (host sessions may have rotated
# the refresh token since the container started, invalidating the container's copy).
if [ -f "${HOME}/.claude/.credentials.json" ]; then
    podman exec "$DEV_CONTAINER" mkdir -p /home/dev/.claude 2>/dev/null || true
    podman cp "${HOME}/.claude/.credentials.json" "${DEV_CONTAINER}:/home/dev/.claude/.credentials.json"
fi

# Update Claude Code to latest before each attach
echo "Updating Claude Code..."
podman exec --user=0 "$DEV_CONTAINER" npm install -g @anthropic-ai/claude-code --loglevel=warn

podman exec -it "$DEV_CONTAINER" claude --dangerously-skip-permissions --remote-control "$TASK_SLUG" || true

# ─── Phase 3: Post-session ──────────────────────────────────────

echo ""

COMMITS=$(git -C "$CLONE_DIR" log --oneline "${BASE_BRANCH}..HEAD" 2>/dev/null || true)

# Check if a PR already exists for this branch
PR_URL=$(cd "$CLONE_DIR" && gh pr list --head "$BRANCH_NAME" --state open --json url -q '.[0].url' 2>/dev/null || true)

if [ -n "$PR_URL" ]; then
    echo "=== PR exists: ${PR_URL} ==="
    echo ""

    # Check for unpushed commits on the branch
    UNPUSHED=$(git -C "$CLONE_DIR" log --oneline "origin/${BRANCH_NAME}..HEAD" 2>/dev/null || true)
    if [ -n "$UNPUSHED" ]; then
        echo "Unpushed commits:"
        echo "$UNPUSHED"
        echo ""
        echo "  [p] Push to update PR"
        echo "  [r] Re-enter container"
        echo "  [d] Destroy containers and clone (un-pushed changes will be lost)"
        echo "  [q] Quit (containers stay running)"
        read -p "Choice: " -n 1 -r
        echo
        case $REPLY in
            p|P)
                git -C "$CLONE_DIR" push origin "$BRANCH_NAME"
                echo "Pushed. PR updated."
                echo ""
                menu_reenter_or_cleanup
                ;;
            r|R) exec "$0" "$TASK_SLUG" "$BASE_BRANCH" ;;
            d|D) cleanup ;;
            *) echo "Containers still running. Re-enter with: $0 ${TASK_SLUG}" ;;
        esac
    else
        menu_reenter_or_cleanup
    fi

elif [ -n "$COMMITS" ]; then
    echo "=== Commits on ${BRANCH_NAME} ==="
    echo "$COMMITS"
    echo ""
    echo "  [r] Re-enter container"
    echo "  [d] Destroy containers and clone (un-pushed changes will be lost)"
    echo "  [q] Quit (containers stay running)"
    read -p "Choice: " -n 1 -r
    echo

    case $REPLY in
        r|R) exec "$0" "$TASK_SLUG" "$BASE_BRANCH" ;;
        d|D) cleanup ;;
        *) echo "Containers still running. Re-enter with: $0 ${TASK_SLUG}" ;;
    esac

else
    echo "No new commits on ${BRANCH_NAME}."
    echo ""
    menu_reenter_or_cleanup
fi
