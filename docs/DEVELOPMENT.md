# Dark Tower - Development Guide

This guide helps you set up your local development environment for Dark Tower.

## Prerequisites

### Required Software

- **Rust**: 1.75+ (install via [rustup](https://rustup.rs/))
- **Node.js**: 20+ (for client development)
- **Docker**: 24+ and Docker Compose
- **Git**: Latest version

### Optional Tools

- **Cargo Watch**: Auto-rebuild on file changes
  ```bash
  cargo install cargo-watch
  ```
- **Protocol Buffer Compiler**: For manual proto compilation
  ```bash
  # macOS
  brew install protobuf

  # Ubuntu/Debian
  sudo apt install protobuf-compiler
  ```

## Quick Start

### 1. Clone the Repository

```bash
git clone https://github.com/nbuckles13/dark_tower.git
cd dark_tower
```

### 2. Start Infrastructure Services

Start PostgreSQL, Redis, and observability tools:

```bash
docker-compose up -d
```

This starts:
- **PostgreSQL** (port 5432): Main database
- **Redis** (port 6379): Ephemeral storage
- **pgAdmin** (port 8080): PostgreSQL web UI
- **Redis Commander** (port 8081): Redis web UI
- **Prometheus** (port 9090): Metrics collection
- **Grafana** (port 3000): Metrics visualization
- **Jaeger** (port 16686): Distributed tracing

### 3. Verify Services

```bash
# Check all services are running
docker-compose ps

# Check service health
docker-compose logs postgres | grep "ready to accept connections"
docker-compose logs redis | grep "Ready to accept connections"
```

### 4. Build the Rust Workspace

```bash
# Build all components
cargo build

# Run tests
cargo test

# Run with pedantic linting
cargo clippy -- -W clippy::pedantic
```

### 5. Run Individual Components

```bash
# Global Controller
cargo run --bin global-controller

# Meeting Controller
cargo run --bin meeting-controller

# Media Handler
cargo run --bin media-handler
```

### 6. Set Up the Client

```bash
cd client
npm install
npm run dev
```

The client will be available at `http://localhost:5173`.

## Development Workflow

### Auto-Rebuild on Changes

Use `cargo-watch` for automatic rebuilding:

```bash
# Watch and rebuild global-controller
cargo watch -x 'run --bin global-controller'

# Watch and run tests
cargo watch -x test

# Watch and run clippy
cargo watch -x 'clippy -- -W clippy::pedantic'
```

### Database Management

#### Accessing the Database

Using `psql`:
```bash
docker-compose exec postgres psql -U darktower -d dark_tower
```

Using pgAdmin:
1. Open `http://localhost:8080`
2. Login: `admin@darktower.dev` / `admin`
3. Add server:
   - Host: `postgres`
   - Port: `5432`
   - Database: `dark_tower`
   - Username: `darktower`
   - Password: `dev_password_change_in_production`

#### Running Migrations

```bash
# Apply migrations (to be implemented)
cargo run --bin migrate -- up

# Rollback migrations
cargo run --bin migrate -- down
```

#### Resetting the Database

```bash
# Stop and remove containers
docker-compose down

# Remove volumes (WARNING: deletes all data)
docker-compose down -v

# Start fresh
docker-compose up -d
```

### Redis Management

#### Using redis-cli

```bash
# Connect to Redis
docker-compose exec redis redis-cli -a dev_password_change_in_production

# Common commands
> KEYS *                    # List all keys
> GET key                   # Get value
> HGETALL meeting:*        # Get all hash fields
> FLUSHALL                 # Clear all data (use with caution!)
```

#### Using Redis Commander

Open `http://localhost:8081` in your browser.

### Protocol Buffer Development

After modifying `.proto` files:

```bash
# Rebuild proto-gen crate
cargo build -p proto-gen

# The generated code will be in:
# crates/proto-gen/src/generated/
```

### Testing

#### Unit Tests

```bash
# Run all tests
cargo test

# Run tests for a specific crate
cargo test -p global-controller

# Run with output
cargo test -- --nocapture

# Run specific test
cargo test test_name
```

#### Integration Tests

```bash
# Run integration tests
cargo test --test integration

# With logging
RUST_LOG=debug cargo test --test integration -- --nocapture
```

#### Load Testing

```bash
# To be implemented
cargo run --bin load-test -- --concurrent 100 --duration 60s
```

### Observability

#### Viewing Metrics

Open Prometheus: `http://localhost:9090`

Example queries:
```promql
# Request rate
rate(http_requests_total[5m])

# Active meetings
dark_tower_active_meetings

# P95 latency
histogram_quantile(0.95, rate(http_request_duration_seconds_bucket[5m]))
```

#### Viewing Dashboards

Open Grafana: `http://localhost:3000`
- Username: `admin`
- Password: `admin`

#### Viewing Traces

Open Jaeger: `http://localhost:16686`

Select service and explore traces.

### Code Quality

#### Linting

```bash
# Run clippy with pedantic warnings
cargo clippy --all-targets -- -W clippy::pedantic

# Auto-fix some issues
cargo clippy --fix --all-targets -- -W clippy::pedantic
```

#### Formatting

```bash
# Check formatting
cargo fmt --all -- --check

# Apply formatting
cargo fmt --all
```

#### Pre-commit Checks

Create `.git/hooks/pre-commit`:

```bash
#!/bin/bash
set -e

echo "Running pre-commit checks..."

# Format check
cargo fmt --all -- --check

# Clippy
cargo clippy --all-targets -- -W clippy::pedantic -D warnings

# Tests
cargo test

echo "All checks passed!"
```

Make it executable:
```bash
chmod +x .git/hooks/pre-commit
```

## Environment Variables

### Global Controller

```bash
export DATABASE_URL="postgresql://darktower:dev_password_change_in_production@localhost:5432/dark_tower"
export REDIS_URL="redis://:dev_password_change_in_production@localhost:6379"
export RUST_LOG="info,global_controller=debug"
export JWT_SECRET="dev-secret-change-in-production"
export OTLP_ENDPOINT="http://localhost:4317"
```

### Meeting Controller

```bash
export REDIS_URL="redis://:dev_password_change_in_production@localhost:6379"
export RUST_LOG="info,meeting_controller=debug"
export BIND_ADDRESS="0.0.0.0:4433"
export OTLP_ENDPOINT="http://localhost:4317"
```

### Media Handler

```bash
export RUST_LOG="info,media_handler=debug"
export BIND_ADDRESS="0.0.0.0:4434"
export MAX_STREAMS="10000"
export OTLP_ENDPOINT="http://localhost:4317"
```

### Using .env File

Create `.env` in project root:

```bash
# Database
DATABASE_URL=postgresql://darktower:dev_password_change_in_production@localhost:5432/dark_tower

# Redis
REDIS_URL=redis://:dev_password_change_in_production@localhost:6379

# Logging
RUST_LOG=info

# Security
JWT_SECRET=dev-secret-change-in-production

# Observability
OTLP_ENDPOINT=http://localhost:4317
```

Load with:
```bash
source .env
```

## Troubleshooting

### Port Already in Use

```bash
# Find process using port 5432
lsof -i :5432

# Kill process
kill -9 <PID>

# Or change port in docker-compose.yml
```

### Docker Permission Issues

```bash
# Add user to docker group (Linux)
sudo usermod -aG docker $USER
newgrp docker
```

### Rust Build Errors

```bash
# Update Rust
rustup update

# Clean build cache
cargo clean

# Rebuild
cargo build
```

### Database Connection Errors

```bash
# Verify PostgreSQL is running
docker-compose ps postgres

# Check logs
docker-compose logs postgres

# Verify connection string
psql "postgresql://darktower:dev_password_change_in_production@localhost:5432/dark_tower" -c "SELECT 1"
```

### Redis Connection Errors

```bash
# Verify Redis is running
docker-compose ps redis

# Test connection
redis-cli -h localhost -p 6379 -a dev_password_change_in_production ping
```

## Performance Tips

### Faster Builds

```bash
# Use sccache for caching
cargo install sccache
export RUSTC_WRAPPER=sccache

# Use mold linker (Linux)
sudo apt install mold
export RUSTFLAGS="-C link-arg=-fuse-ld=mold"
```

### Parallel Compilation

Add to `~/.cargo/config.toml`:

```toml
[build]
jobs = 8  # Number of CPU cores
```

### Release Builds for Performance Testing

```bash
# Build with optimizations
cargo build --release

# Run optimized binary
./target/release/global-controller
```

## IDE Setup

### VS Code

Recommended extensions:
- `rust-analyzer`: Rust language server
- `CodeLLDB`: Debugging support
- `Even Better TOML`: TOML syntax highlighting
- `Crates`: Dependency management
- `TODO Highlight`: Track TODOs

### IntelliJ IDEA / CLion

- Install Rust plugin
- Configure Rust toolchain
- Enable Clippy and rustfmt

## Contributing

See `CONTRIBUTING.md` for:
- Code style guidelines
- Pull request process
- Issue reporting
- Communication channels

## Resources

- [Rust Book](https://doc.rust-lang.org/book/)
- [Tokio Tutorial](https://tokio.rs/tokio/tutorial)
- [Quinn Documentation](https://docs.rs/quinn/)
- [Protocol Buffers Guide](https://protobuf.dev/)
- [WebTransport Explainer](https://w3c.github.io/webtransport/)

## Getting Help

- Check existing [issues](https://github.com/nbuckles13/dark_tower/issues)
- Read the [documentation](docs/)
- Ask in discussions

## Next Steps

Once your environment is set up:

1. Read [ARCHITECTURE.md](ARCHITECTURE.md) to understand the system
2. Review [API_CONTRACTS.md](API_CONTRACTS.md) for API details
3. Check the [Project Board](https://github.com/nbuckles13/dark_tower/projects) for tasks
4. Pick an issue labeled `good-first-issue` to start contributing

Happy coding!
