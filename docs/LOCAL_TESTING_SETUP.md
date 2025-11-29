# Local Testing Setup

This guide helps you set up a local development environment for running tests.

## Prerequisites

- Ubuntu 24.04 (or compatible Linux distribution)
- Rust toolchain (already installed)
- Podman (for container management)

## Installing Podman

We use Podman instead of Docker for local development because:
- **Daemonless**: No background daemon required
- **Rootless**: Better security, runs without root privileges
- **Docker-compatible**: Drop-in replacement for Docker CLI
- **Production-ready**: Same container runtime used in production

### Installation Steps

```bash
# Update package list
sudo apt-get update

# Install Podman and podman-compose
sudo apt-get install -y podman podman-compose

# Verify installation
podman --version
podman-compose --version

# Optional: Create docker alias for compatibility
alias docker='podman'
alias docker-compose='podman-compose'

# Add to ~/.bashrc for persistence
echo "alias docker='podman'" >> ~/.bashrc
echo "alias docker-compose='podman-compose'" >> ~/.bashrc
```

## Starting Test Database

```bash
# Start PostgreSQL test database
podman-compose -f docker-compose.test.yml up -d postgres-test

# Verify it's running
podman ps

# Check logs
podman logs dark-tower-postgres-test

# Wait for database to be healthy
podman healthcheck run dark-tower-postgres-test
```

## Running Tests

```bash
# Set environment variables
export DATABASE_URL=postgresql://postgres:postgres@localhost:5433/dark_tower_test
export AC_MASTER_KEY=AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=
export AC_BIND_ADDRESS=127.0.0.1:8082

# Run all tests
cargo test --workspace

# Run specific package tests
cargo test -p ac-service

# Run specific test
cargo test -p ac-service --lib repositories::service_credentials::tests::test_create_and_get_service_credential
```

## Stopping Test Database

```bash
# Stop database (keeps data)
podman-compose -f docker-compose.test.yml stop

# Stop and remove containers (removes data)
podman-compose -f docker-compose.test.yml down

# Stop and remove containers + volumes (complete cleanup)
podman-compose -f docker-compose.test.yml down -v
```

## Troubleshooting

### Port Already in Use

If port 5433 is already in use:

```bash
# Check what's using the port
sudo lsof -i :5433

# Kill the process or change port in docker-compose.test.yml
```

### Permission Denied

If you get permission errors with Podman:

```bash
# Enable rootless Podman
podman system migrate
```

### Database Connection Issues

```bash
# Check if database is running
podman ps | grep postgres

# Check database logs
podman logs dark-tower-postgres-test

# Restart database
podman-compose -f docker-compose.test.yml restart postgres-test
```

## Environment Variables

Create a `.env.test` file for convenience:

```bash
# .env.test
DATABASE_URL=postgresql://postgres:postgres@localhost:5433/dark_tower_test
AC_MASTER_KEY=AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=
AC_BIND_ADDRESS=127.0.0.1:8082
```

Then source it before running tests:

```bash
source .env.test
cargo test --workspace
```

## CI/CD

The CI/CD pipeline (GitHub Actions) uses Docker with PostgreSQL on port 5432.
Local tests use Podman with PostgreSQL on port 5433 to avoid conflicts.

Both environments run the same migrations and tests, ensuring consistency.
