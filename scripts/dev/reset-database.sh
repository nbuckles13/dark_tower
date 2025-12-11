#!/bin/bash
#
# reset-database.sh
#
# Reset the test database by dropping all tables and rerunning migrations.
# This is useful for quick iteration during development.
#
# Usage:
#   ./scripts/dev/reset-database.sh [--confirm]
#
# Options:
#   --confirm    Skip confirmation prompt (for scripted usage)

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Project root (two levels up from this script)
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

# Configuration
export DATABASE_URL="postgresql://postgres:postgres@localhost:5433/dark_tower_test"

# Parse arguments
SKIP_CONFIRM=false
if [[ "${1:-}" == "--confirm" ]]; then
    SKIP_CONFIRM=true
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

# Check if database is running
check_database() {
    log_info "Checking if PostgreSQL is running..."

    if ! docker ps | grep -q dark-tower-postgres-test; then
        log_error "PostgreSQL container is not running"
        log_info "Start it with: ./scripts/dev/start-local-stack.sh"
        exit 1
    fi

    if ! docker exec dark-tower-postgres-test pg_isready -U postgres &> /dev/null; then
        log_error "PostgreSQL is not ready"
        exit 1
    fi

    log_success "PostgreSQL is running"
}

# Confirm action
confirm_reset() {
    if [ "$SKIP_CONFIRM" = true ]; then
        return 0
    fi

    echo ""
    log_warn "This will DROP ALL TABLES in the dark_tower_test database!"
    echo -e "${YELLOW}Database URL: ${DATABASE_URL}${NC}"
    echo ""
    read -p "Are you sure you want to continue? (yes/no): " -r
    echo ""

    if [[ ! $REPLY =~ ^[Yy][Ee][Ss]$ ]]; then
        log_info "Reset cancelled"
        exit 0
    fi
}

# Drop all tables
drop_all_tables() {
    log_info "Dropping all tables..."

    # SQL to drop all tables in the public schema
    local drop_sql="
        DO \$\$ DECLARE
            r RECORD;
        BEGIN
            FOR r IN (SELECT tablename FROM pg_tables WHERE schemaname = 'public') LOOP
                EXECUTE 'DROP TABLE IF EXISTS ' || quote_ident(r.tablename) || ' CASCADE';
            END LOOP;
        END \$\$;
    "

    if docker exec -i dark-tower-postgres-test psql -U postgres -d dark_tower_test -c "$drop_sql" &> /dev/null; then
        log_success "All tables dropped"
    else
        log_error "Failed to drop tables"
        return 1
    fi
}

# Drop the _sqlx_migrations table to allow migrations to run fresh
drop_migrations_table() {
    log_info "Dropping _sqlx_migrations table..."

    docker exec -i dark-tower-postgres-test psql -U postgres -d dark_tower_test -c \
        "DROP TABLE IF EXISTS _sqlx_migrations CASCADE;" &> /dev/null

    log_success "Migrations table dropped"
}

# Run migrations
run_migrations() {
    log_info "Running migrations..."

    cd "${PROJECT_ROOT}"

    if sqlx migrate run --database-url "${DATABASE_URL}"; then
        log_success "Migrations completed successfully"
    else
        log_error "Migrations failed"
        return 1
    fi
}

# Display migration info
display_migration_info() {
    log_info "Checking applied migrations..."

    local migration_query="SELECT version, description, success, installed_on FROM _sqlx_migrations ORDER BY version;"

    echo ""
    docker exec -i dark-tower-postgres-test psql -U postgres -d dark_tower_test -c "$migration_query"
    echo ""
}

# Display table info
display_table_info() {
    log_info "Current database tables:"

    local table_query="
        SELECT
            schemaname,
            tablename,
            pg_size_pretty(pg_total_relation_size(schemaname||'.'||tablename)) AS size
        FROM pg_tables
        WHERE schemaname = 'public'
        ORDER BY tablename;
    "

    echo ""
    docker exec -i dark-tower-postgres-test psql -U postgres -d dark_tower_test -c "$table_query"
    echo ""
}

# Main execution
main() {
    log_info "Database Reset Tool for Dark Tower"
    echo ""

    check_database
    confirm_reset

    log_info "Starting database reset..."
    drop_all_tables
    drop_migrations_table
    run_migrations

    echo ""
    log_success "Database reset complete!"
    echo ""

    display_migration_info
    display_table_info

    log_info "Next steps:"
    echo "  - Run ./scripts/dev/seed-test-data.sh to add sample data"
    echo "  - Start AC service with: ./scripts/dev/start-local-stack.sh --start-service"
    echo ""
}

# Run main function
main
