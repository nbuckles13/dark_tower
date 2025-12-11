# Scripts Directory

This directory contains utility scripts for Dark Tower development and operations.

## Directory Structure

```
scripts/
├── dev/              # Development and testing scripts
│   ├── start-local-stack.sh    # Start local development environment
│   ├── reset-database.sh       # Reset test database
│   ├── seed-test-data.sh       # Populate test data
│   └── README.md               # Development scripts documentation
└── README.md         # This file
```

## Available Script Categories

### Development Scripts (`dev/`)

Scripts for local development, testing, and database management.

**Quick Start:**
```bash
# First time setup
cd /home/nathan/code/dark_tower
./scripts/dev/start-local-stack.sh
./scripts/dev/seed-test-data.sh

# Start AC service
./scripts/dev/start-local-stack.sh --start-service
```

See [`dev/README.md`](dev/README.md) for detailed documentation.

## Script Conventions

All scripts in this directory follow these conventions:

### 1. POSIX Compatibility
- Use `#!/bin/bash` shebang
- Avoid bash-specific features where possible
- Test on both Linux and macOS

### 2. Error Handling
- Always use `set -euo pipefail` at the top
- Exit on errors, undefined variables, and pipe failures
- Provide meaningful error messages

### 3. Output Format
- Use color-coded output:
  - **Blue** for informational messages
  - **Green** for success messages
  - **Yellow** for warnings
  - **Red** for errors
- Include clear section headers for output

### 4. Prerequisites
- Check for required tools before execution
- Provide installation instructions if tools are missing
- Fail fast with helpful error messages

### 5. Idempotency
- Scripts should be safe to run multiple times
- Check current state before making changes
- Skip operations that are already complete

### 6. Documentation
- Include usage comments at the top of each script
- Document required environment variables
- Provide examples in README files

## Environment Variables

Scripts use these standard environment variables:

### Database
```bash
DATABASE_URL="postgresql://postgres:postgres@localhost:5433/dark_tower_test"
```

### Security
```bash
AC_MASTER_KEY="AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8="  # Test only
```

### Service Configuration
```bash
AC_BIND_ADDR="0.0.0.0:8080"
RUST_LOG="ac_service=debug,tower_http=debug"
```

## Adding New Scripts

When adding a new script category:

1. **Create a subdirectory** (e.g., `scripts/ops/` for operational scripts)
2. **Add a README.md** in the subdirectory with:
   - Purpose of scripts in this category
   - Prerequisites
   - Usage examples
   - Troubleshooting
3. **Update this README** with a link to the new category
4. **Follow conventions** listed above
5. **Make scripts executable**: `chmod +x scripts/category/script-name.sh`

## Future Script Categories

Planned script categories (to be added as needed):

### Operations (`ops/`)
- Production deployment scripts
- Backup and restore utilities
- Health check scripts
- Log aggregation helpers

### CI/CD (`ci/`)
- Build automation
- Test coverage reporting
- Artifact publishing
- Release automation

### Monitoring (`monitoring/`)
- Metrics collection
- Alert testing
- Performance profiling
- Resource usage tracking

### Security (`security/`)
- Credential rotation
- Security scanning
- Penetration test helpers
- Compliance checking

## Related Documentation

- **Development Workflow**: `../.claude/DEVELOPMENT_WORKFLOW.md`
- **Project Status**: `../docs/PROJECT_STATUS.md`
- **Architecture**: `../docs/ARCHITECTURE.md`
- **Docker Setup**: `../docker-compose.test.yml`

## Support

For issues or questions about scripts:

1. Check the README in the relevant subdirectory
2. Review the script's usage comments
3. Consult related documentation
4. Check GitHub issues for known problems

## Contributing

When contributing new scripts:

1. Follow the conventions above
2. Test thoroughly on Linux and macOS
3. Document in the appropriate README
4. Add error handling and validation
5. Include examples in documentation
6. Make scripts executable before committing
