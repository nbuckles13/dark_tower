# Development Scripts

Convenience scripts for local development and testing of the Dark Tower AC service.

## Prerequisites

Before using these scripts, ensure you have:

1. **Docker & Docker Compose** - For running PostgreSQL and Redis
2. **sqlx-cli** - For database migrations
   ```bash
   cargo install sqlx-cli --no-default-features --features postgres
   ```
3. **Python 3 with bcrypt** - For password hashing in seed script
   ```bash
   pip3 install bcrypt
   ```
4. **psql** (optional) - For direct database access and verification

## Available Scripts

### 1. start-local-stack.sh

Start the complete local development environment.

**What it does:**
- Starts PostgreSQL (port 5433) and Redis (port 6380) via docker-compose
- Waits for services to be healthy
- Runs database migrations
- Displays connection information
- Optionally starts the AC service

**Usage:**
```bash
# Start infrastructure only
./scripts/dev/start-local-stack.sh

# Start infrastructure AND AC service
./scripts/dev/start-local-stack.sh --start-service
```

**Environment variables set:**
- `DATABASE_URL=postgresql://postgres:postgres@localhost:5433/dark_tower_test`
- `AC_MASTER_KEY=AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=`

### 2. reset-database.sh

Reset the test database by dropping all tables and rerunning migrations.

**What it does:**
- Checks if PostgreSQL is running
- Prompts for confirmation (can be skipped with `--confirm`)
- Drops all tables in the public schema
- Drops the `_sqlx_migrations` table
- Reruns all migrations
- Displays migration status and table info

**Usage:**
```bash
# Interactive mode (asks for confirmation)
./scripts/dev/reset-database.sh

# Auto-confirm (for scripts)
./scripts/dev/reset-database.sh --confirm
```

**When to use:**
- After changing migration files
- When you need a clean database state
- During development iterations

### 3. seed-test-data.sh

Populate the database with sample service credentials for manual testing.

**What it does:**
- Creates/updates service credentials for:
  - `global-controller`
  - `meeting-controller`
  - `media-handler`
  - `test-client`
- Checks for active signing keys (recommends starting AC service if none exist)
- Displays credentials and example curl commands

**Usage:**
```bash
./scripts/dev/seed-test-data.sh
```

**Created credentials:**

| client_id | client_secret |
|-----------|---------------|
| `global-controller` | `global-controller-secret-dev-001` |
| `meeting-controller` | `meeting-controller-secret-dev-002` |
| `media-handler` | `media-handler-secret-dev-003` |
| `test-client` | `test-client-secret-dev-999` |

> **Note:** These are development-only credentials with bcrypt cost factor 12

## Common Workflows

### First-Time Setup

```bash
# 1. Start the infrastructure
./scripts/dev/start-local-stack.sh

# 2. Seed test data
./scripts/dev/seed-test-data.sh

# 3. Start the AC service (in a new terminal)
export DATABASE_URL="postgresql://postgres:postgres@localhost:5433/dark_tower_test"
export AC_MASTER_KEY="AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8="
cargo run --bin ac-service
```

### Daily Development

```bash
# Start infrastructure (if not already running)
./scripts/dev/start-local-stack.sh

# In another terminal, start AC service
./scripts/dev/start-local-stack.sh --start-service
```

### Testing Migration Changes

```bash
# 1. Reset database with new migrations
./scripts/dev/reset-database.sh

# 2. Reseed test data
./scripts/dev/seed-test-data.sh

# 3. Run tests
cargo test --workspace
```

### Manual API Testing

```bash
# 1. Ensure stack is running
./scripts/dev/start-local-stack.sh

# 2. Start AC service
./scripts/dev/start-local-stack.sh --start-service

# 3. Get an access token
curl -X POST http://localhost:8080/v1/token \
  -H 'Content-Type: application/x-www-form-urlencoded' \
  -d 'grant_type=client_credentials' \
  -d 'client_id=test-client' \
  -d 'client_secret=test-client-secret-dev-999'

# 4. Get public keys (JWKS)
curl http://localhost:8080/.well-known/jwks.json

# 5. Check metrics
curl http://localhost:8080/metrics
```

## Database Access

### Direct PostgreSQL Access

```bash
# Connect to the database
psql postgresql://postgres:postgres@localhost:5433/dark_tower_test

# Useful queries
SELECT * FROM service_credentials;
SELECT key_id, algorithm, is_active, created_at FROM signing_keys;
SELECT * FROM _sqlx_migrations;
```

### Using Docker Exec

```bash
# Execute SQL directly
docker exec -i dark-tower-postgres-test psql -U postgres -d dark_tower_test -c "SELECT * FROM service_credentials;"

# Interactive shell
docker exec -it dark-tower-postgres-test psql -U postgres -d dark_tower_test
```

## Troubleshooting

### PostgreSQL Container Not Starting

```bash
# Check if port 5433 is already in use
lsof -i :5433

# View container logs
docker logs dark-tower-postgres-test

# Remove old container and volumes
docker-compose -f docker-compose.test.yml down -v
```

### Migrations Failing

```bash
# Check current migration status
docker exec -i dark-tower-postgres-test psql -U postgres -d dark_tower_test -c "SELECT * FROM _sqlx_migrations;"

# Manually revert (if needed)
sqlx migrate revert --database-url postgresql://postgres:postgres@localhost:5433/dark_tower_test

# Reset and try again
./scripts/dev/reset-database.sh
```

### AC Service Not Starting

```bash
# Check environment variables
echo $DATABASE_URL
echo $AC_MASTER_KEY

# Verify database connectivity
psql $DATABASE_URL -c "SELECT version();"

# Check if signing key exists
psql $DATABASE_URL -c "SELECT COUNT(*) FROM signing_keys WHERE is_active = true;"
```

### Python bcrypt Module Missing

```bash
# Install bcrypt
pip3 install bcrypt

# Or use system package manager
# Ubuntu/Debian:
sudo apt-get install python3-bcrypt

# macOS:
brew install python3
pip3 install bcrypt
```

## Script Implementation Details

### Script Safety Features

All scripts include:
- **Set -euo pipefail** - Exit on error, undefined variables, and pipe failures
- **Color-coded output** - Visual distinction for info/success/warning/error messages
- **Prerequisites checking** - Verify required tools before execution
- **Health checks** - Wait for services to be ready before proceeding
- **Idempotency** - Safe to run multiple times (where applicable)

### Environment Variables

Scripts use consistent environment variables:

```bash
DATABASE_URL="postgresql://postgres:postgres@localhost:5433/dark_tower_test"
AC_MASTER_KEY="AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8="
```

These match the values in:
- `docker-compose.test.yml` (database configuration)
- GitHub Actions CI (`.github/workflows/ci.yml`)
- Test utilities (`crates/ac-test-utils`)

### Port Mapping

To avoid conflicts with production databases:
- PostgreSQL: `5433` (host) → `5432` (container)
- Redis: `6380` (host) → `6379` (container)

## Integration with CI/CD

These scripts are designed for local development but share configuration with CI:

- **Database URL**: Same format as GitHub Actions
- **Master Key**: Same test key used in CI
- **Migration Strategy**: Same sqlx commands

This ensures local development closely mirrors the CI environment.

## Security Notes

**⚠️ DEVELOPMENT ONLY**

These scripts and credentials are for **local development and testing only**:

- Master key is a fixed test value (never use in production)
- Service credentials are hardcoded sample values
- PostgreSQL has no password restrictions
- No TLS/SSL encryption

**Production deployments must:**
- Use unique, randomly-generated master keys
- Rotate credentials regularly
- Use strong password policies
- Enable TLS/SSL encryption
- Follow ADR-0006 (Zero-Trust Architecture)

## Related Documentation

- **Main README**: `../../README.md` - Project overview
- **Architecture**: `../../docs/ARCHITECTURE.md` - System design
- **Database Schema**: `../../docs/DATABASE_SCHEMA.md` - Table definitions
- **AC Service**: `../../crates/ac-service/README.md` - Service documentation
- **Migrations**: `../../migrations/` - SQL migration files
- **Docker Compose**: `../../docker-compose.test.yml` - Container configuration

## Contributing

When adding new development scripts:

1. Follow POSIX shell compatibility (`#!/bin/bash`)
2. Include color-coded output helpers
3. Add prerequisite checks
4. Document usage in this README
5. Make scripts executable (`chmod +x`)
6. Test on both Linux and macOS
7. Add error handling and helpful messages
