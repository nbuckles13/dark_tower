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
#   ./devloop.sh [--rebuild] <task-slug> [base-branch]
#   ./devloop.sh --refresh-creds <task-slug>
#
# Options:
#   --rebuild         Force rebuild the dev container image
#   --refresh-creds   Copy fresh OAuth credentials into a running container
#
# Examples:
#   ./devloop.sh td-42-rate-limiting
#   ./devloop.sh td-42-rate-limiting main
#   ./devloop.sh --rebuild td-42-rate-limiting
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
while [[ "${1:-}" == --* ]]; do
    case "$1" in
        --rebuild) REBUILD_IMAGE=true; shift ;;
        --refresh-creds) REFRESH_CREDS=true; shift ;;
        *) echo "Unknown option: $1" >&2; exit 1 ;;
    esac
done

TASK_SLUG="${1:?Usage: devloop.sh [--rebuild] <task-slug> [base-branch]}"
BASE_BRANCH="${2:-main}"

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

cleanup() {
    echo "Destroying containers..."
    podman rm -f "$DEV_CONTAINER" 2>/dev/null || true
    podman rm -f "$DB_CONTAINER" 2>/dev/null || true
    echo "Removing clone: ${CLONE_DIR}"
    rm -rf "$CLONE_DIR"
    echo "Cleaned up."
}

push_and_create_pr() {
    local title="$1"
    local body="$2"
    git -C "$CLONE_DIR" push -u origin "$BRANCH_NAME"
    cd "$CLONE_DIR" && gh pr create --title "$title" --body "$body"
    rm -f "$PR_META"
    echo ""
    echo "  [d] Destroy containers and clone (un-pushed changes will be lost)"
    echo "  [q] Quit (containers stay running)"
    read -p "Choice: " -n 1 -r
    echo
    case $REPLY in
        d|D) cleanup ;;
        *) echo "Containers still running. Re-enter with: $0 ${TASK_SLUG}" ;;
    esac
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

# ─── Validate Prerequisites ─────────────────────────────────────

if ! command -v podman &>/dev/null; then
    echo "ERROR: podman is required. Install with: sudo apt install podman" >&2
    exit 1
fi

if [ -z "${ANTHROPIC_API_KEY:-}" ] && [ ! -f "${HOME}/.claude/.credentials.json" ]; then
    echo "ERROR: No authentication found. Either set ANTHROPIC_API_KEY or log in with 'claude' first." >&2
    exit 1
fi

# ─── Build image if needed ───────────────────────────────────────

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

if $REBUILD_IMAGE || ! podman image exists "$IMAGE"; then
    echo "Building dev container image..."
    podman build -t "$IMAGE" "$SCRIPT_DIR"
fi

# ─── Phase 1: Setup (idempotent) ────────────────────────────────

if ! is_container_running "$DEV_CONTAINER"; then
    echo "=== Setting up clone and containers for: ${TASK_SLUG} ==="

    # Create local clone if it doesn't exist.
    # Uses --local for hardlinked objects (fast, space-efficient).
    # The clone is fully self-contained — no external .git references.
    if [ ! -d "$CLONE_DIR" ]; then
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
    else
        echo "Clone already exists: ${CLONE_DIR}"
    fi

    # Clean up any stopped containers from a previous run
    podman rm -f "$DEV_CONTAINER" 2>/dev/null || true
    podman rm -f "$DB_CONTAINER" 2>/dev/null || true

    # Start postgres container
    echo "Starting PostgreSQL..."
    podman run -d --name "$DB_CONTAINER" \
        -e POSTGRES_PASSWORD=postgres \
        -e POSTGRES_DB=dark_tower_test \
        docker.io/library/postgres:16-bookworm

    # Mount user-level Claude config files if they exist (read-only, to fixed paths
    # that entrypoint.sh will copy to the correct $HOME inside the container)
    CLAUDE_MOUNTS=""
    if [ -f "${HOME}/.claude/settings.json" ]; then
        CLAUDE_MOUNTS="${CLAUDE_MOUNTS} -v ${HOME}/.claude/settings.json:/tmp/claude-user-settings.json:ro"
    fi
    if [ -f "${HOME}/.claude.json" ]; then
        CLAUDE_MOUNTS="${CLAUDE_MOUNTS} -v ${HOME}/.claude.json:/tmp/claude-user-config.json:ro"
    fi
    if [ -f "${HOME}/.claude/.credentials.json" ]; then
        CLAUDE_MOUNTS="${CLAUDE_MOUNTS} -v ${HOME}/.claude/.credentials.json:/tmp/claude-credentials.json:ro"
    fi

    # Start dev container sharing the DB container's network namespace.
    # This gives localhost connectivity to postgres while allowing --userns=keep-id
    # for proper file ownership in the bind-mounted clone.
    echo "Starting dev container..."
    # shellcheck disable=SC2086
    podman run -d --name "$DEV_CONTAINER" \
        --userns=keep-id \
        --network "container:$DB_CONTAINER" \
        -v "$(realpath "$CLONE_DIR"):/work:Z" \
        -v cargo-registry:/tmp/cargo-home/registry \
        -v cargo-git:/tmp/cargo-home/git \
        -e CARGO_HOME=/tmp/cargo-home \
        $CLAUDE_MOUNTS \
        -e DATABASE_URL="postgresql://postgres:postgres@localhost:5432/dark_tower_test" \
        ${ANTHROPIC_API_KEY:+-e ANTHROPIC_API_KEY="${ANTHROPIC_API_KEY}"} \
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

# ─── Phase 2: Attach ────────────────────────────────────────────

echo ""
echo "Dropping into Claude Code..."
echo "(Ctrl-D or /exit to detach — containers stay running)"
echo ""

# Refresh OAuth credentials before each attach (host sessions may have rotated
# the refresh token since the container started, invalidating the container's copy).
if [ -f "${HOME}/.claude/.credentials.json" ]; then
    podman cp "${HOME}/.claude/.credentials.json" "${DEV_CONTAINER}:/home/dev/.claude/.credentials.json"
fi

podman exec -it "$DEV_CONTAINER" claude --dangerously-skip-permissions || true

# ─── Phase 3: Post-session ──────────────────────────────────────

echo ""

PR_META="${CLONE_DIR}/.devloop-pr.json"
COMMITS=$(git -C "$CLONE_DIR" log --oneline "${BASE_BRANCH}..HEAD" 2>/dev/null || true)

# Check if a PR already exists for this branch
PR_URL=$(cd "$CLONE_DIR" && gh pr view --json url -q .url 2>/dev/null || true)

if [ -n "$PR_URL" ]; then
    echo "=== PR exists: ${PR_URL} ==="
    echo ""
    menu_reenter_or_cleanup

elif [ -n "$COMMITS" ]; then
    echo "=== Commits on ${BRANCH_NAME} ==="
    echo "$COMMITS"
    echo ""

    if [ -f "$PR_META" ]; then
        PR_TITLE=$(jq -r .title "$PR_META")
        echo "PR ready: ${PR_TITLE}"
        echo ""
        echo "  [p] Push and create PR"
        echo "  [e] Edit PR description, then push"
        echo "  [r] Re-enter container"
        echo "  [q] Quit (containers stay running)"
        read -p "Choice: " -n 1 -r
        echo

        case $REPLY in
            p|P)
                PR_BODY=$(jq -r .body "$PR_META")
                push_and_create_pr "$PR_TITLE" "$PR_BODY"
                ;;
            e|E)
                DRAFT="${CLONE_DIR}/.pr-body-draft.md"
                jq -r .body "$PR_META" > "$DRAFT"
                ${EDITOR:-vi} "$DRAFT"
                PR_BODY=$(cat "$DRAFT")
                rm -f "$DRAFT"
                push_and_create_pr "$PR_TITLE" "$PR_BODY"
                ;;
            r|R)
                exec "$0" "$TASK_SLUG" "$BASE_BRANCH"
                ;;
            *)
                echo "Containers still running. Re-enter with: $0 ${TASK_SLUG}"
                ;;
        esac
    else
        echo "No PR metadata found (.devloop-pr.json)."
        echo "Re-enter the container to complete the devloop and generate PR metadata."
        echo ""
        menu_reenter_or_cleanup
    fi

else
    echo "No new commits on ${BRANCH_NAME}."
    echo ""
    menu_reenter_or_cleanup
fi
