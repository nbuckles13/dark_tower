#!/usr/bin/env bash
# Test wrapper script
#
# Ensures the test database is running and migrations are applied,
# then runs cargo test with all provided arguments.
#
# Usage:
#   ./scripts/test.sh --workspace
#   ./scripts/test.sh -p ac-service --lib
#   ./scripts/test.sh --workspace -- --test-threads=1

set -e

# Configuration
CONTAINER_NAME="dark-tower-postgres-test"
COMPOSE_FILE="docker-compose.test.yml"
DATABASE_URL="postgresql://postgres:postgres@localhost:5433/dark_tower_test"
MAX_WAIT_SECONDS=30

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log_info() {
    echo -e "${GREEN}[test]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[test]${NC} $1"
}

log_error() {
    echo -e "${RED}[test]${NC} $1"
}

# Detect container runtime (podman or docker)
detect_runtime() {
    if command -v podman &> /dev/null; then
        echo "podman"
    elif command -v docker &> /dev/null; then
        echo "docker"
    else
        log_error "Neither podman nor docker found. Please install one."
        exit 1
    fi
}

# Detect compose command
detect_compose() {
    local runtime="$1"
    if [ "$runtime" = "podman" ]; then
        if command -v podman-compose &> /dev/null; then
            echo "podman-compose"
        else
            log_error "podman-compose not found. Please install it."
            exit 1
        fi
    else
        if command -v docker-compose &> /dev/null; then
            echo "docker-compose"
        elif docker compose version &> /dev/null; then
            echo "docker compose"
        else
            log_error "docker-compose not found. Please install it."
            exit 1
        fi
    fi
}

# Check if container is running
is_container_running() {
    local runtime="$1"
    $runtime ps --format "{{.Names}}" 2>/dev/null | grep -q "^${CONTAINER_NAME}$"
}

# Check if database is ready to accept connections
is_db_ready() {
    local runtime="$1"
    $runtime exec "$CONTAINER_NAME" pg_isready -U postgres &> /dev/null
}

# Start the test database
start_db() {
    local compose="$1"
    log_info "Starting test database..."
    $compose -f "$COMPOSE_FILE" up -d postgres-test
}

# Wait for database to be ready
wait_for_db() {
    local runtime="$1"
    local waited=0

    log_info "Waiting for database to be ready..."
    while ! is_db_ready "$runtime"; do
        if [ $waited -ge $MAX_WAIT_SECONDS ]; then
            log_error "Database did not become ready within ${MAX_WAIT_SECONDS}s"
            exit 1
        fi
        sleep 1
        ((waited++))
    done
    log_info "Database is ready"
}

# Check if migrations are pending
has_pending_migrations() {
    # sqlx migrate info returns pending migrations - check if any exist
    local info
    info=$(DATABASE_URL="$DATABASE_URL" sqlx migrate info 2>/dev/null)
    echo "$info" | grep -q "pending"
}

# Run migrations only if needed
run_migrations_if_needed() {
    if has_pending_migrations; then
        log_info "Applying pending database migrations..."
        if ! DATABASE_URL="$DATABASE_URL" sqlx migrate run 2>&1; then
            log_error "Failed to run migrations"
            exit 1
        fi
    fi
}

# Main
main() {
    local runtime
    local compose

    runtime=$(detect_runtime)
    compose=$(detect_compose "$runtime")

    # Check if DB is running
    if ! is_container_running "$runtime"; then
        start_db "$compose"
        wait_for_db "$runtime"
    elif ! is_db_ready "$runtime"; then
        wait_for_db "$runtime"
    fi

    # Check and apply migrations if needed
    run_migrations_if_needed

    # Run cargo test with all provided arguments
    log_info "Running: cargo test $*"
    export DATABASE_URL
    exec cargo test "$@"
}

main "$@"
