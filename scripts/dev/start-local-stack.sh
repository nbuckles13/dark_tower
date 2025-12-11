#!/bin/bash
#
# start-local-stack.sh
#
# Start the local development stack for Dark Tower AC service testing.
# This script:
#   1. Starts PostgreSQL and Redis via docker-compose
#   2. Waits for databases to be healthy
#   3. Runs database migrations
#   4. Optionally starts the AC service
#
# Usage:
#   ./scripts/dev/start-local-stack.sh [--start-service]
#
# Options:
#   --start-service    Also start the AC service after infrastructure is ready

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Project root (two levels up from this script)
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
COMPOSE_FILE="${PROJECT_ROOT}/docker-compose.test.yml"

# Configuration
export DATABASE_URL="postgresql://postgres:postgres@localhost:5433/dark_tower_test"
export AC_MASTER_KEY="AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8="

# Parse arguments
START_SERVICE=false
if [[ "${1:-}" == "--start-service" ]]; then
    START_SERVICE=true
fi

# Helper functions
log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check prerequisites
check_prerequisites() {
    log_info "Checking prerequisites..."

    if ! command -v docker &> /dev/null; then
        log_error "docker is not installed or not in PATH"
        exit 1
    fi

    if ! command -v docker-compose &> /dev/null && ! docker compose version &> /dev/null; then
        log_error "docker-compose is not installed or not in PATH"
        exit 1
    fi

    if ! command -v psql &> /dev/null; then
        log_warn "psql is not installed - will skip connection test"
    fi

    if ! command -v sqlx &> /dev/null; then
        log_error "sqlx-cli is not installed. Install with: cargo install sqlx-cli --no-default-features --features postgres"
        exit 1
    fi

    log_success "Prerequisites check passed"
}

# Start docker-compose services
start_docker_services() {
    log_info "Starting Docker services..."

    cd "${PROJECT_ROOT}"

    # Use docker-compose or docker compose depending on what's available
    if command -v docker-compose &> /dev/null; then
        docker-compose -f "${COMPOSE_FILE}" up -d
    else
        docker compose -f "${COMPOSE_FILE}" up -d
    fi

    log_success "Docker services started"
}

# Wait for PostgreSQL to be ready
wait_for_postgres() {
    log_info "Waiting for PostgreSQL to be ready..."

    local max_attempts=30
    local attempt=1

    while [ $attempt -le $max_attempts ]; do
        if docker exec dark-tower-postgres-test pg_isready -U postgres &> /dev/null; then
            log_success "PostgreSQL is ready"
            return 0
        fi

        echo -n "."
        sleep 1
        attempt=$((attempt + 1))
    done

    log_error "PostgreSQL did not become ready in time"
    return 1
}

# Wait for Redis to be ready
wait_for_redis() {
    log_info "Waiting for Redis to be ready..."

    local max_attempts=30
    local attempt=1

    while [ $attempt -le $max_attempts ]; do
        if docker exec dark-tower-redis-test redis-cli ping &> /dev/null; then
            log_success "Redis is ready"
            return 0
        fi

        echo -n "."
        sleep 1
        attempt=$((attempt + 1))
    done

    log_error "Redis did not become ready in time"
    return 1
}

# Run database migrations
run_migrations() {
    log_info "Running database migrations..."

    cd "${PROJECT_ROOT}"

    if sqlx migrate run --database-url "${DATABASE_URL}"; then
        log_success "Migrations completed successfully"
    else
        log_error "Migrations failed"
        return 1
    fi
}

# Display connection information
display_info() {
    echo ""
    echo -e "${GREEN}╔════════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${GREEN}║${NC}  ${BLUE}Dark Tower Local Development Stack${NC}                        ${GREEN}║${NC}"
    echo -e "${GREEN}╚════════════════════════════════════════════════════════════════╝${NC}"
    echo ""
    echo -e "${BLUE}PostgreSQL:${NC}"
    echo "  Host:     localhost"
    echo "  Port:     5433"
    echo "  Database: dark_tower_test"
    echo "  User:     postgres"
    echo "  Password: postgres"
    echo "  URL:      ${DATABASE_URL}"
    echo ""
    echo -e "${BLUE}Redis:${NC}"
    echo "  Host:     localhost"
    echo "  Port:     6380"
    echo ""
    echo -e "${BLUE}Environment Variables:${NC}"
    echo "  export DATABASE_URL=\"${DATABASE_URL}\""
    echo "  export AC_MASTER_KEY=\"${AC_MASTER_KEY}\""
    echo ""
    echo -e "${YELLOW}Quick Commands:${NC}"
    echo "  Connect to DB:    psql ${DATABASE_URL}"
    echo "  Reset DB:         ./scripts/dev/reset-database.sh"
    echo "  Seed test data:   ./scripts/dev/seed-test-data.sh"
    echo "  Stop stack:       docker-compose -f docker-compose.test.yml down"
    echo ""
}

# Start AC service
start_ac_service() {
    log_info "Starting AC service..."

    cd "${PROJECT_ROOT}"

    # Set environment variables and start the service
    export DATABASE_URL="${DATABASE_URL}"
    export AC_MASTER_KEY="${AC_MASTER_KEY}"
    export AC_BIND_ADDR="0.0.0.0:8080"
    export RUST_LOG="ac_service=debug,tower_http=debug"

    log_info "AC service will start with:"
    log_info "  DATABASE_URL: ${DATABASE_URL}"
    log_info "  AC_BIND_ADDR: ${AC_BIND_ADDR}"
    log_info "  RUST_LOG: ${RUST_LOG}"
    echo ""

    cargo run --bin ac-service
}

# Main execution
main() {
    log_info "Starting Dark Tower local development stack..."
    echo ""

    check_prerequisites
    start_docker_services
    wait_for_postgres
    wait_for_redis
    run_migrations
    display_info

    if [ "$START_SERVICE" = true ]; then
        start_ac_service
    else
        log_info "Stack is ready! Use --start-service flag to also start the AC service."
    fi
}

# Run main function
main
