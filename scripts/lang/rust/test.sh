#!/usr/bin/env bash
# Rust test wrapper.
#
# Body migrated verbatim from the original scripts/test.sh:
#   - Detects podman/docker runtime and compose command.
#   - Brings up the test postgres container if not running.
#   - Applies pending sqlx migrations.
#   - Runs `cargo test "$@"` (args flow through).
#
# Wraps the cargo invocation with the ADR-0033 §6 STATUS contract.

set -euo pipefail
IFS=$'\n\t'
source "$(dirname "${BASH_SOURCE[0]}")/../_common.sh"
__rust_test_repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
# task #50: the helpers below (detect_runtime/wait_for_db/run_migrations)
# can `exit 1` BEFORE main reaches run_and_emit — the canonical silent-skip-at-pipeline-
# edge case. The EXIT trap emits STATUS=FAIL REASON=wrapper-aborted-early-exit-<rc> so
# the dispatcher sees a loud FAIL instead of an empty pipe (which aggregated to UNKNOWN).
install_wrapper_exit_trap

# Configuration (preserved from original scripts/test.sh)
CONTAINER_NAME="dark-tower-postgres-test"
COMPOSE_FILE="docker-compose.test.yml"
DEFAULT_DATABASE_URL="postgresql://postgres:postgres@localhost:5433/dark_tower_test"
MAX_WAIT_SECONDS=30

log_info()  { printf '%b[test]%b %s\n' "${DEVLOOP_GREEN}"  "${DEVLOOP_NC}" "$1"; }
log_warn()  { printf '%b[test]%b %s\n' "${DEVLOOP_YELLOW}" "${DEVLOOP_NC}" "$1"; }
log_error() { printf '%b[test]%b %s\n' "${DEVLOOP_RED}"    "${DEVLOOP_NC}" "$1"; }

detect_runtime() {
  if command -v podman &>/dev/null; then
    echo "podman"
  elif command -v docker &>/dev/null; then
    echo "docker"
  else
    log_error "Neither podman nor docker found. Please install one."
    exit 1
  fi
}

detect_compose() {
  local runtime="$1"
  if [[ "$runtime" == "podman" ]]; then
    if command -v podman-compose &>/dev/null; then
      echo "podman-compose"
    else
      log_error "podman-compose not found. Please install it."
      exit 1
    fi
  else
    if command -v docker-compose &>/dev/null; then
      echo "docker-compose"
    elif docker compose version &>/dev/null; then
      echo "docker compose"
    else
      log_error "docker-compose not found. Please install it."
      exit 1
    fi
  fi
}

is_container_running() {
  local runtime="$1"
  $runtime ps --format "{{.Names}}" 2>/dev/null | grep -q "^${CONTAINER_NAME}$"
}

is_db_ready() {
  local runtime="$1"
  $runtime exec "$CONTAINER_NAME" pg_isready -U postgres &>/dev/null
}

start_db() {
  local compose="$1"
  log_info "Starting test database..."
  $compose -f "$COMPOSE_FILE" up -d postgres-test
}

wait_for_db() {
  local runtime="$1"
  local waited=0
  log_info "Waiting for database to be ready..."
  while ! is_db_ready "$runtime"; do
    if [[ $waited -ge $MAX_WAIT_SECONDS ]]; then
      log_error "Database did not become ready within ${MAX_WAIT_SECONDS}s"
      exit 1
    fi
    sleep 1
    ((waited++))
  done
  log_info "Database is ready"
}

# THE one migration of the unit-test DB (infra/devloop/entrypoint.sh no longer
# runs one). Always `migrate run` — it is idempotent (pending-only via
# `_sqlx_migrations`), so there is no pre-check to get wrong: the former
# `sqlx migrate info 2>/dev/null || true | grep pending` gate SKIPPED the run
# silently when sqlx was missing or `info` failed on checksum drift, and the
# tests then ran against an unmigrated/drifted DB with misattributed failures.
run_migrations() {
  # The CLI must match the workspace's sqlx library (the `_sqlx_migrations` shape
  # `#[sqlx::test]` reads): pinned from Cargo.lock by the ONE reader, which the
  # devloop image also builds from — but an image built before a sqlx bump keeps
  # the old CLI, so drift is CHECKED here, not assumed away.
  local want have
  # shellcheck source=../../../infra/lib/cargo-lock-version.sh
  source "${__rust_test_repo_root}/infra/lib/cargo-lock-version.sh"
  want="$(cargo_lock_version "${__rust_test_repo_root}/Cargo.lock" sqlx)" || exit 1
  if ! command -v sqlx >/dev/null 2>&1; then
    log_error "sqlx-cli not found on PATH — cannot migrate the test database, so the tests would run against an unmigrated schema. The devloop image installs it (infra/devloop/Dockerfile); on a host: cargo install sqlx-cli --locked --no-default-features --features postgres --version =${want}"
    exit 1
  fi
  have="$(sqlx --version 2>/dev/null | awk '{print $2}')"
  if [[ "$have" != "$want" ]]; then
    log_error "SQLX_CLI_VERSION_MISMATCH: sqlx-cli ${have:-<unknown>} != Cargo.lock sqlx ${want} — the CLI would write a _sqlx_migrations shape the library may not read. Rebuild the devloop image: infra/devloop/devloop.sh --rebuild (host). Outside the devloop: cargo install sqlx-cli --locked --no-default-features --features postgres --version =${want}"
    exit 1
  fi
  log_info "Applying database migrations (pending-only)..."
  if ! DATABASE_URL="$DATABASE_URL" sqlx migrate run 2>&1; then
    log_error "Failed to run migrations (an edited applied migration fails here: restore it, or reset the test DB)"
    exit 1
  fi
}

check_external_db() {
  if [[ -z "${DATABASE_URL:-}" ]]; then
    return 1
  fi
  local host_port host port
  host_port=$(echo "$DATABASE_URL" | sed -n 's|.*@\([^/]*\)/.*|\1|p')
  host="${host_port%%:*}"
  port="${host_port##*:}"
  pg_isready -h "$host" -p "$port" -U postgres -q 2>/dev/null
}

main() {
  if check_external_db; then
    log_info "External database reachable at ${DATABASE_URL} — skipping container management"
  else
    DATABASE_URL="$DEFAULT_DATABASE_URL"
    local runtime compose
    runtime=$(detect_runtime)
    compose=$(detect_compose "$runtime")

    if ! is_container_running "$runtime"; then
      start_db "$compose"
      wait_for_db "$runtime"
    elif ! is_db_ready "$runtime"; then
      wait_for_db "$runtime"
    fi
  fi

  run_migrations

  log_info "Running: cargo test $*"
  export DATABASE_URL

  # Wrap cargo with the STATUS contract via run_and_emit.
  # --no-fail-fast: every test binary runs even when one fails, so one red run reports every
  # failure (all test runners, every layer — scripts/layer7.test.sh pins it).
  run_and_emit "cargo-test" cargo test --no-fail-fast "${CARGO_LOCKED[@]}" "$@"
}

main "$@"
