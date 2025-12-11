#!/bin/bash
#
# seed-test-data.sh
#
# Seed the test database with sample data for manual testing.
# This script creates:
#   - Sample service credentials (service_credentials table)
#   - Active signing key (signing_keys table)
#   - Displays credentials for manual API testing
#
# Usage:
#   ./scripts/dev/seed-test-data.sh

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m' # No Color

# Project root (two levels up from this script)
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

# Configuration
export DATABASE_URL="postgresql://postgres:postgres@localhost:5433/dark_tower_test"
export AC_MASTER_KEY="AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8="

# Sample credentials (these will be displayed to the user)
# Secrets are bcrypt hashes of the plaintext shown in output
declare -A SERVICE_CREDENTIALS=(
    ["global-controller"]="global-controller-secret-dev-001"
    ["meeting-controller"]="meeting-controller-secret-dev-002"
    ["media-handler"]="media-handler-secret-dev-003"
    ["test-client"]="test-client-secret-dev-999"
)

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

# Check if tables exist
check_tables() {
    log_info "Checking if required tables exist..."

    local check_query="
        SELECT EXISTS (
            SELECT FROM information_schema.tables
            WHERE table_schema = 'public'
            AND table_name = 'service_credentials'
        );
    "

    local result=$(docker exec -i dark-tower-postgres-test psql -U postgres -d dark_tower_test -t -c "$check_query" | tr -d ' ')

    if [[ "$result" != "t" ]]; then
        log_error "Required tables not found. Run migrations first:"
        log_info "  ./scripts/dev/reset-database.sh"
        exit 1
    fi

    log_success "Required tables exist"
}

# Generate bcrypt hash for a password
# Note: We use a simple approach here - in production, the AC service does this
generate_bcrypt_hash() {
    local password="$1"

    # Use Python to generate bcrypt hash (more portable than htpasswd)
    # Cost factor: 12 (matching AC service production settings)
    python3 -c "import bcrypt; print(bcrypt.hashpw(b'${password}', bcrypt.gensalt(rounds=12)).decode())"
}

# Create or update service credentials
create_service_credentials() {
    log_info "Creating service credentials..."

    for client_id in "${!SERVICE_CREDENTIALS[@]}"; do
        local client_secret="${SERVICE_CREDENTIALS[$client_id]}"

        log_info "Processing: ${client_id}"

        # Generate bcrypt hash
        local hashed_secret=$(generate_bcrypt_hash "$client_secret")

        # Check if credential already exists
        local check_query="SELECT client_id FROM service_credentials WHERE client_id = '${client_id}';"
        local exists=$(docker exec -i dark-tower-postgres-test psql -U postgres -d dark_tower_test -t -c "$check_query" | tr -d ' ')

        if [[ -n "$exists" ]]; then
            log_warn "  Credential already exists, updating..."

            # Update existing credential
            local update_query="
                UPDATE service_credentials
                SET
                    hashed_secret = '${hashed_secret}',
                    updated_at = NOW()
                WHERE client_id = '${client_id}';
            "

            docker exec -i dark-tower-postgres-test psql -U postgres -d dark_tower_test -c "$update_query" &> /dev/null
        else
            log_info "  Creating new credential..."

            # Insert new credential
            local insert_query="
                INSERT INTO service_credentials (client_id, hashed_secret, is_active, created_at, updated_at)
                VALUES ('${client_id}', '${hashed_secret}', true, NOW(), NOW());
            "

            docker exec -i dark-tower-postgres-test psql -U postgres -d dark_tower_test -c "$insert_query" &> /dev/null
        fi

        log_success "  ✓ ${client_id}"
    done

    log_success "Service credentials created/updated"
}

# Ensure an active signing key exists
create_signing_key() {
    log_info "Checking for active signing key..."

    # Check if an active key exists
    local check_query="SELECT key_id FROM signing_keys WHERE is_active = true LIMIT 1;"
    local active_key=$(docker exec -i dark-tower-postgres-test psql -U postgres -d dark_tower_test -t -c "$check_query" | tr -d ' ')

    if [[ -n "$active_key" ]]; then
        log_success "Active signing key already exists (key_id: ${active_key})"
        return 0
    fi

    log_info "No active signing key found. Creating one..."
    log_warn "This requires running the AC service to generate keys properly."
    log_info "The AC service will auto-create a key on first startup if none exists."

    # We don't create signing keys directly in the database because:
    # 1. They require proper Ed25519 key generation
    # 2. Private keys must be encrypted with AC_MASTER_KEY
    # 3. The AC service handles this securely

    log_info "To create a signing key, start the AC service:"
    echo "  ./scripts/dev/start-local-stack.sh --start-service"
    echo ""
}

# Display credentials for manual testing
display_credentials() {
    echo ""
    echo -e "${GREEN}╔════════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${GREEN}║${NC}  ${BOLD}${BLUE}Test Service Credentials${NC}                                  ${GREEN}║${NC}"
    echo -e "${GREEN}╚════════════════════════════════════════════════════════════════╝${NC}"
    echo ""

    for client_id in "${!SERVICE_CREDENTIALS[@]}"; do
        local client_secret="${SERVICE_CREDENTIALS[$client_id]}"

        echo -e "${CYAN}${BOLD}${client_id}${NC}"
        echo -e "  ${BLUE}client_id:${NC}     ${client_id}"
        echo -e "  ${BLUE}client_secret:${NC} ${client_secret}"
        echo ""
    done

    echo -e "${YELLOW}${BOLD}Example: Get Access Token${NC}"
    echo -e "${YELLOW}━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo ""
    echo "curl -X POST http://localhost:8080/v1/token \\"
    echo "  -H 'Content-Type: application/x-www-form-urlencoded' \\"
    echo "  -d 'grant_type=client_credentials' \\"
    echo "  -d 'client_id=test-client' \\"
    echo "  -d 'client_secret=test-client-secret-dev-999'"
    echo ""
    echo -e "${YELLOW}${BOLD}Example: Get JWKS (Public Keys)${NC}"
    echo -e "${YELLOW}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo ""
    echo "curl http://localhost:8080/.well-known/jwks.json"
    echo ""
    echo -e "${BLUE}${BOLD}Database Info${NC}"
    echo -e "${BLUE}━━━━━━━━━━━━━${NC}"
    echo "  Database URL: ${DATABASE_URL}"
    echo "  Connect:      psql ${DATABASE_URL}"
    echo ""
}

# Display current database state
display_database_state() {
    log_info "Current database state:"

    # Count service credentials
    local creds_count=$(docker exec -i dark-tower-postgres-test psql -U postgres -d dark_tower_test -t -c \
        "SELECT COUNT(*) FROM service_credentials;" | tr -d ' ')

    # Count active signing keys
    local active_keys=$(docker exec -i dark-tower-postgres-test psql -U postgres -d dark_tower_test -t -c \
        "SELECT COUNT(*) FROM signing_keys WHERE is_active = true;" | tr -d ' ')

    # Count total signing keys
    local total_keys=$(docker exec -i dark-tower-postgres-test psql -U postgres -d dark_tower_test -t -c \
        "SELECT COUNT(*) FROM signing_keys;" | tr -d ' ')

    echo ""
    echo -e "  ${BLUE}Service Credentials:${NC} ${creds_count}"
    echo -e "  ${BLUE}Active Signing Keys:${NC} ${active_keys} (total: ${total_keys})"
    echo ""

    if [[ "$active_keys" -eq "0" ]]; then
        log_warn "No active signing keys! JWT token issuance will fail."
        log_info "Start the AC service to auto-create a signing key."
    fi
}

# Main execution
main() {
    log_info "Test Data Seeding Tool for Dark Tower"
    echo ""

    # Check for Python3 (needed for bcrypt)
    if ! command -v python3 &> /dev/null; then
        log_error "python3 is required but not installed"
        log_info "Install python3 and the bcrypt module:"
        log_info "  pip3 install bcrypt"
        exit 1
    fi

    # Check for bcrypt module
    if ! python3 -c "import bcrypt" &> /dev/null; then
        log_error "Python bcrypt module is not installed"
        log_info "Install with: pip3 install bcrypt"
        exit 1
    fi

    check_database
    check_tables
    create_service_credentials
    create_signing_key
    display_database_state
    display_credentials

    log_success "Test data seeding complete!"
}

# Run main function
main
