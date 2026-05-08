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

has_pending_migrations() {
  local info
  info=$(DATABASE_URL="$DATABASE_URL" sqlx migrate info 2>/dev/null) || true
  echo "$info" | grep -q "pending"
}

run_migrations_if_needed() {
  if has_pending_migrations; then
    log_info "Applying pending database migrations..."
    if ! DATABASE_URL="$DATABASE_URL" sqlx migrate run 2>&1; then
      log_error "Failed to run migrations"
      exit 1
    fi
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

  run_migrations_if_needed

  log_info "Running: cargo test $*"
  export DATABASE_URL

  # Wrap cargo with the STATUS contract via run_and_emit.
  run_and_emit "cargo-test" cargo test "$@"
}

main "$@"
