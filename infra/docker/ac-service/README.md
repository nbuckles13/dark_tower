# AC Service Docker Image

Multi-stage Dockerfile for the Dark Tower Authentication Controller service following ADR-0012 infrastructure architecture.

## Image Variants

### Production (Default)
```bash
docker build -t dark-tower/ac-service:latest .
```

**Characteristics:**
- Base: `gcr.io/distroless/cc-debian12` (minimal)
- Security: Non-root user (uid 65532)
- Size: ~50-80 MB (stripped binary)
- Attack surface: Minimal (no shell, no package manager)
- Health check: None (use Kubernetes probes)

**Recommended for:** Production deployments with Kubernetes

### Debug (With HEALTHCHECK)
```bash
docker build -t dark-tower/ac-service:debug --target runtime-with-healthcheck .
```

**Characteristics:**
- Base: `gcr.io/distroless/cc-debian12:debug`
- Includes: Busybox shell for HEALTHCHECK
- Security: Non-root user (uid 65532)
- Size: ~80-100 MB
- Health check: File existence check every 30s

**Recommended for:** Standalone Docker testing, local development

## Build Requirements

### Context
Build from repository root:
```bash
cd /path/to/dark_tower
docker build -f infra/docker/ac-service/Dockerfile -t ac-service:latest .
```

### Required Files
- `Cargo.toml`, `Cargo.lock` - Workspace configuration
- `crates/ac-service/` - AC service source code
- `crates/common/` - Shared utilities
- `crates/ac-test-utils/` - Test utilities (dependencies only)
- `migrations/` - Database migrations

## Runtime Configuration

### Required Environment Variables
```bash
DATABASE_URL=postgresql://user:pass@host:5432/dbname
AC_MASTER_KEY=<base64-encoded-32-byte-key>  # For key encryption at rest
```

### Optional Environment Variables
```bash
BIND_ADDRESS=0.0.0.0:8082              # Default bind address
RUST_LOG=info                           # Logging level (debug, info, warn, error)
CLUSTER_NAME=us                         # Cluster identifier (default: us)
OTLP_ENDPOINT=http://otel:4317         # OpenTelemetry endpoint
```

### Generating AC_MASTER_KEY
```bash
# Generate a 32-byte random key and base64 encode it
openssl rand -base64 32
```

## Running the Container

### Standalone Docker
```bash
docker run -d \
  --name ac-service \
  -p 8082:8082 \
  -e DATABASE_URL=postgresql://postgres:postgres@db:5432/dark_tower \
  -e AC_MASTER_KEY=$(openssl rand -base64 32) \
  -e CLUSTER_NAME=us-west \
  dark-tower/ac-service:latest
```

### Docker Compose
```yaml
services:
  ac-service:
    image: dark-tower/ac-service:latest
    ports:
      - "8082:8082"
    environment:
      DATABASE_URL: postgresql://postgres:postgres@postgres:5432/dark_tower
      AC_MASTER_KEY: ${AC_MASTER_KEY}
      CLUSTER_NAME: us-west
      RUST_LOG: info
    depends_on:
      - postgres
```

## Kubernetes Deployment

### Health Probes (Recommended)
```yaml
apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: ac-service
spec:
  template:
    spec:
      containers:
      - name: ac-service
        image: dark-tower/ac-service:latest
        ports:
        - containerPort: 8082
        env:
        - name: DATABASE_URL
          valueFrom:
            secretKeyRef:
              name: ac-service-secrets
              key: database-url
        - name: AC_MASTER_KEY
          valueFrom:
            secretKeyRef:
              name: ac-service-secrets
              key: master-key
        livenessProbe:
          httpGet:
            path: /health
            port: 8082
          initialDelaySeconds: 10
          periodSeconds: 30
          timeoutSeconds: 3
          failureThreshold: 3
        readinessProbe:
          httpGet:
            path: /health
            port: 8082
          initialDelaySeconds: 5
          periodSeconds: 10
          timeoutSeconds: 3
          failureThreshold: 3
        resources:
          requests:
            cpu: 500m
            memory: 1Gi
          limits:
            cpu: 2000m
            memory: 2Gi
```

## Security Features

### Distroless Base
- No shell or package manager
- Minimal attack surface
- Only includes runtime dependencies (glibc, libgcc)
- Regularly updated by Google

### Non-Root User
- Runs as UID 65532 (nonroot)
- No privilege escalation possible
- Compliant with Pod Security Standards (Restricted)

### Build Optimizations
- Multi-stage build (dependencies cached separately)
- Stripped binary (debug symbols removed)
- Release profile (optimizations enabled)
- Minimal layer count

### Supply Chain Security
- Use with Trivy scanning (see `.github/workflows/`)
- SBOM generation supported
- Reproducible builds (Cargo.lock pinned)

## Exposed Ports

| Port | Protocol | Purpose |
|------|----------|---------|
| 8082 | TCP | HTTP/2 API (HTTPS in production via Linkerd mTLS) |

## Health Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/health` | GET | Basic health check (returns "OK") |
| `/jwks` | GET | JSON Web Key Set (public keys for JWT verification) |

## Image Size Comparison

| Variant | Compressed | Uncompressed |
|---------|------------|--------------|
| Production (distroless/cc) | ~25 MB | ~70 MB |
| Debug (distroless/cc:debug) | ~30 MB | ~90 MB |
| Alpine-based (reference) | ~35 MB | ~100 MB |
| Debian-slim (reference) | ~80 MB | ~200 MB |

## Build Performance

### Cold Build (no cache)
- **Duration:** ~5-10 minutes (depends on network and CPU)
- **Network:** ~500 MB (Rust toolchain + dependencies)

### Warm Build (cached dependencies)
- **Duration:** ~1-2 minutes (only rebuilds changed code)
- **Network:** Minimal (only pulls updated base images)

### Caching Strategy
1. **Layer 1:** Rust toolchain (rust:1.83-slim)
2. **Layer 2:** Cargo dependencies (cached via dummy src)
3. **Layer 3:** Application code (rebuilds on code changes)
4. **Layer 4:** Runtime (distroless base)

## Troubleshooting

### Container fails to start
```bash
# Check logs
docker logs ac-service

# Common issues:
# 1. Missing DATABASE_URL or AC_MASTER_KEY
# 2. Database not reachable
# 3. Invalid AC_MASTER_KEY (must be 32 bytes, base64-encoded)
```

### Build fails
```bash
# Check Docker context
docker build -f infra/docker/ac-service/Dockerfile .

# Common issues:
# 1. Building from wrong directory (must be repo root)
# 2. Missing Cargo.lock or Cargo.toml
# 3. Network issues downloading dependencies
```

### Health check always fails (debug variant)
```bash
# Verify container is running
docker ps -a

# Check if binary exists
docker exec ac-service /busybox/ls -la /usr/local/bin/

# Check health status
docker inspect --format='{{.State.Health.Status}}' ac-service
```

## References

- [ADR-0012: Infrastructure Architecture](../../../docs/decisions/adr-0012-infrastructure-architecture.md)
- [AC Service README](../../../crates/ac-service/README.md)
- [Distroless Documentation](https://github.com/GoogleContainerTools/distroless)
- [Kubernetes Best Practices](https://kubernetes.io/docs/concepts/configuration/overview/)
