# AC Service Deployment Runbook

**Service**: Authentication Controller (ac-service)
**Version**: Phase 4B (Production Ready)
**Last Updated**: 2025-12-10
**Owner**: Operations Team

---

## Overview

This runbook covers deployment, rollback, and troubleshooting procedures for the Authentication Controller service. The AC service is a critical component responsible for OAuth 2.0 service-to-service authentication, JWT token issuance, and JWKS endpoint for federated authentication.

**Critical Service**: AC downtime impacts all service-to-service authentication. Follow pre-deployment checklist carefully.

---

## Table of Contents

1. [Pre-Deployment Checklist](#pre-deployment-checklist)
2. [Deployment Steps](#deployment-steps)
3. [Rollback Procedure](#rollback-procedure)
4. [Configuration Reference](#configuration-reference)
5. [Common Deployment Issues](#common-deployment-issues)
6. [Smoke Tests](#smoke-tests)
7. [Monitoring and Verification](#monitoring-and-verification)

---

## Pre-Deployment Checklist

Complete ALL items before deploying to production:

### Code Quality

- [ ] **Code review approved** by at least one reviewer
- [ ] **CI tests passing** in GitHub Actions
  ```bash
  # Verify CI status
  gh pr checks <PR-NUMBER>
  ```
- [ ] **Test coverage ≥86%** (targeting 95% for Phase 4B completion)
  ```bash
  # Check coverage report
  cargo llvm-cov --workspace --lcov --output-path lcov.info
  ```
- [ ] **Linting passes** with zero warnings
  ```bash
  cargo clippy --workspace --lib --bins -- -D warnings
  ```
- [ ] **Security scan passes** (Trivy, no CRITICAL vulnerabilities)
  ```bash
  trivy image ac-service:latest --severity CRITICAL
  ```

### Infrastructure

- [ ] **Database migrations reviewed** by Database specialist (if applicable)
  - Check `migrations/` directory for new migration files
  - Review migration rollback plan (DOWN migration)
  - Verify migration idempotency

- [ ] **Config changes documented** in this runbook or ADR
  - New environment variables added to ConfigMap/Secret
  - Default values appropriate for production
  - Breaking changes communicated to dependent services

- [ ] **Rollback plan confirmed**
  - Previous container image available in registry
  - Database migrations reversible (or snapshot taken)
  - Rollback criteria defined (see [Rollback Procedure](#rollback-procedure))

- [ ] **Capacity planning verified**
  - Current resource utilization <70% (CPU/memory)
  - Database connection pool capacity sufficient
  - Rate limiting thresholds appropriate

### Coordination

- [ ] **Maintenance window scheduled** (if downtime expected)
  - Dependent services notified (GC, MC, MH)
  - Users notified if user-facing impact
  - On-call engineer available

- [ ] **Runbook reviewed** by deployment engineer
  - All steps understood
  - Required access verified (kubectl, database, monitoring)

---

## Deployment Steps

### 1. Pre-Deployment Verification

**Verify current state:**

```bash
# Check current deployment status
kubectl get statefulset ac-service -n dark-tower

# Check current pod status
kubectl get pods -n dark-tower -l app=ac-service

# Check current resource utilization
kubectl top pods -n dark-tower -l app=ac-service

# Verify readiness of current pods
kubectl get pods -n dark-tower -l app=ac-service -o json | jq '.items[].status.conditions[] | select(.type=="Ready")'
```

**Expected output:**
- StatefulSet shows desired replicas (2)
- All pods in Running state
- All pods Ready=True
- CPU <70%, Memory <70%

### 2. Database Migration (if required)

**CRITICAL:** Database migrations must complete BEFORE container deployment.

```bash
# Connect to database
export DATABASE_URL="postgresql://ac_user:<password>@postgres.dark-tower.svc.cluster.local:5432/dark_tower?sslmode=verify-full"

# Check current migration status
sqlx migrate info

# Run pending migrations
sqlx migrate run

# Verify migration success
sqlx migrate info
```

**Rollback plan:** If migration fails, manually run DOWN migration:

```bash
# Find the failed migration number
sqlx migrate info

# Manually apply DOWN migration (from migration file comments)
psql $DATABASE_URL -f migrations/<MIGRATION_FILE>.sql
# Then manually execute the DOWN commands from the file
```

**Wait for migration completion** before proceeding. Do NOT deploy containers if migration fails.

### 3. Update Container Image

**Option A: Using kubectl (direct deployment)**

```bash
# Set new image version
export NEW_VERSION="v1.2.3"  # Replace with actual version tag

# Update StatefulSet with new image
kubectl set image statefulset/ac-service \
  ac-service=ac-service:${NEW_VERSION} \
  -n dark-tower

# Verify image updated
kubectl describe statefulset ac-service -n dark-tower | grep Image:
```

**Option B: Using kubectl apply (declarative)**

```bash
# Update infra/services/ac-service/statefulset.yaml
# Change image tag: ac-service:latest → ac-service:v1.2.3

# Apply updated manifest
kubectl apply -f infra/services/ac-service/statefulset.yaml

# Verify change
kubectl describe statefulset ac-service -n dark-tower | grep Image:
```

**Option C: Using Helm (if Helm chart available)**

```bash
# Update values.yaml or override
helm upgrade ac-service ./charts/ac-service \
  --namespace dark-tower \
  --set image.tag=v1.2.3 \
  --wait \
  --timeout 10m
```

### 4. Rolling Update Monitoring

StatefulSets update pods **one at a time** in reverse ordinal order (ac-service-1, then ac-service-0).

**Monitor rollout:**

```bash
# Watch pod status (Ctrl+C to exit)
kubectl get pods -n dark-tower -l app=ac-service -w

# Check rollout status
kubectl rollout status statefulset/ac-service -n dark-tower

# Monitor logs from new pod
kubectl logs -f ac-service-1 -n dark-tower
```

**Expected sequence:**
1. ac-service-1 terminates (SIGTERM sent, 30s drain period)
2. ac-service-1 new pod starts
3. ac-service-1 new pod becomes Ready (health check + readiness check pass)
4. ac-service-0 terminates (repeat process)
5. ac-service-0 new pod starts and becomes Ready

**Typical timeline:**
- Pod termination: 30-35 seconds (graceful shutdown)
- Pod startup: 5-15 seconds (database connection + signing key init)
- Total per pod: ~45-50 seconds
- **Total rollout: ~2 minutes for 2 replicas**

### 5. Verify Deployment Success

**Pod health:**

```bash
# All pods Running and Ready
kubectl get pods -n dark-tower -l app=ac-service

# Check pod events for errors
kubectl get events -n dark-tower --field-selector involvedObject.name=ac-service-0
kubectl get events -n dark-tower --field-selector involvedObject.name=ac-service-1
```

**Logs review:**

```bash
# Check for startup errors
kubectl logs ac-service-0 -n dark-tower --tail=50
kubectl logs ac-service-1 -n dark-tower --tail=50

# Look for error patterns
kubectl logs -n dark-tower -l app=ac-service --tail=100 | grep -i "error\|panic\|fatal"
```

**Expected log messages:**
```
Starting Auth Controller
Configuration loaded successfully
Connecting to database...
Database connection established
Initializing signing keys...
Signing keys initialized
Prometheus metrics recorder initialized
Auth Controller listening on 0.0.0.0:8082
```

### 6. Run Smoke Tests

**See [Smoke Tests](#smoke-tests) section below for detailed test procedures.**

Minimum required smoke tests:
- [ ] Health check returns 200 OK
- [ ] Readiness check returns 200 OK
- [ ] JWKS endpoint returns valid JSON
- [ ] Service token issuance succeeds (if test credentials available)

### 7. Monitor Metrics

**Verify metrics collection:**

```bash
# Port-forward to access metrics endpoint
kubectl port-forward -n dark-tower ac-service-0 8082:8082 &

# Fetch metrics
curl http://localhost:8082/metrics

# Kill port-forward
kill %1
```

**Check key metrics:**
- `ac_db_query_duration_seconds` - Database query latency
- `ac_token_issuance_duration_seconds` - Token issuance latency
- `ac_bcrypt_duration_seconds` - Password hashing time

**Prometheus queries (if Prometheus available):**

```promql
# Error rate (should be near 0%)
rate(ac_http_requests_total{status=~"5.."}[5m])

# p99 latency (should be <350ms per ADR-0011)
histogram_quantile(0.99, rate(ac_token_issuance_duration_seconds_bucket[5m]))

# Active database connections
ac_db_connections_active
```

### 8. Traffic Verification

**Confirm service is receiving traffic:**

```bash
# Check Service endpoints
kubectl get endpoints ac-service -n dark-tower

# Verify traffic reaching pods (requires metrics)
kubectl port-forward -n dark-tower ac-service-0 8082:8082 &
curl http://localhost:8082/metrics | grep ac_http_requests_total
kill %1
```

Expected: `ac_http_requests_total` counter increasing over time.

### 9. Post-Deployment Checklist

- [ ] All pods Running and Ready
- [ ] Smoke tests pass (health, ready, JWKS, token issuance)
- [ ] No errors in logs (last 5 minutes)
- [ ] Metrics available in Prometheus
- [ ] Error rate <1% (if baseline traffic exists)
- [ ] p99 latency <350ms (ADR-0011 SLO)
- [ ] Database connection pool healthy (<80% utilization)
- [ ] Dependent services authenticated successfully (GC, MC, MH)

---

## Rollback Procedure

### When to Rollback

**Immediate rollback criteria** (do not wait):

1. **Pod startup failures**
   - Pods stuck in CrashLoopBackOff >5 minutes
   - Pods failing readiness checks consistently
   - Database connection failures

2. **Critical functionality broken**
   - Token issuance endpoint returning 500 errors
   - JWKS endpoint unavailable
   - Signing key initialization failures

3. **Severe performance degradation**
   - p99 latency >1000ms (3x SLO)
   - Error rate >10%
   - Database connection pool exhausted

4. **Security issues discovered**
   - Vulnerability in new code
   - Credential leakage
   - Authorization bypass

**Monitoring period before declaring success:**
- Minimum: 15 minutes post-deployment
- Recommended: 1 hour for major changes
- Critical changes: 24 hours with on-call monitoring

### How to Rollback

**Step 1: Identify previous version**

```bash
# Find previous image version
kubectl rollout history statefulset/ac-service -n dark-tower

# Get image from previous revision
kubectl rollout history statefulset/ac-service -n dark-tower --revision=<PREVIOUS_REVISION>
```

**Step 2: Rollback StatefulSet**

```bash
# Rollback to previous revision
kubectl rollout undo statefulset/ac-service -n dark-tower

# Or rollback to specific revision
kubectl rollout undo statefulset/ac-service -n dark-tower --to-revision=<REVISION>

# Monitor rollback
kubectl rollout status statefulset/ac-service -n dark-tower
```

**Step 3: Verify rollback success**

```bash
# Check pods running previous version
kubectl get pods -n dark-tower -l app=ac-service -o jsonpath='{.items[*].spec.containers[0].image}'

# Run smoke tests (see Smoke Tests section)
# Verify health, ready, JWKS endpoints
```

**Step 4: Rollback database migration (if applicable)**

**CRITICAL:** Only rollback database if new migration is incompatible with previous code.

```bash
# Connect to database
export DATABASE_URL="postgresql://ac_user:<password>@postgres.dark-tower.svc.cluster.local:5432/dark_tower?sslmode=verify-full"

# Identify migration to rollback
sqlx migrate info

# Option A: Manual rollback (safest)
# Execute DOWN migration commands from migration file comments
psql $DATABASE_URL

# Option B: Restore from backup (if migration is destructive)
# Contact DBA team or use CloudNativePG restore procedure
```

**Step 5: Post-rollback verification**

- [ ] All pods running previous image version
- [ ] Smoke tests pass
- [ ] Error rate returned to baseline
- [ ] Latency returned to baseline
- [ ] No errors in logs

**Step 6: Incident retrospective**

- Document rollback reason
- Create incident report
- Identify root cause
- Update pre-deployment checklist if needed

### Database Rollback Considerations

**Backward-compatible migrations** (safe to rollback code):
- Adding new columns with defaults
- Creating new indexes
- Adding new tables (not used by old code)

**Backward-incompatible migrations** (require coordination):
- Dropping columns or tables
- Changing column types
- Removing indexes (may cause performance issues)
- Modifying constraints

**If migration is backward-incompatible:**
1. **DO NOT** rollback code until migration is rolled back
2. Restore database from snapshot (if available)
3. Or manually execute DOWN migration
4. THEN rollback code

---

## Configuration Reference

### Environment Variables

| Variable | Required | Description | Default | Example |
|----------|----------|-------------|---------|---------|
| `DATABASE_URL` | **Yes** | PostgreSQL connection string with TLS | None | `postgresql://ac_user:password@postgres.dark-tower.svc.cluster.local:5432/dark_tower?sslmode=verify-full` |
| `AC_MASTER_KEY` | **Yes** | Base64-encoded 32-byte AES-256-GCM master key for signing key encryption | None | `base64(random_32_bytes)` |
| `BIND_ADDRESS` | No | TCP bind address for HTTP server | `0.0.0.0:8082` | `0.0.0.0:8082` |
| `OTLP_ENDPOINT` | No | OpenTelemetry collector endpoint (future observability) | None | `http://otel-collector:4317` |
| `CLUSTER_NAME` | No | Cluster identifier for key ID generation | `us` | `us`, `eu`, `ap` |
| `RUST_LOG` | No | Logging level | `info` | `info,ac_service=debug` |

### Kubernetes Secrets

**Secret: `ac-service-secrets`** (namespace: `dark-tower`)

```yaml
apiVersion: v1
kind: Secret
metadata:
  name: ac-service-secrets
  namespace: dark-tower
type: Opaque
data:
  DATABASE_URL: <base64-encoded-connection-string>
  AC_MASTER_KEY: <base64-encoded-32-byte-key>
```

**Creating secrets:**

```bash
# Generate master key (32 random bytes)
AC_MASTER_KEY=$(openssl rand -base64 32)

# Create DATABASE_URL (adjust credentials and host)
DATABASE_URL="postgresql://ac_user:REPLACE_PASSWORD@postgres.dark-tower.svc.cluster.local:5432/dark_tower?sslmode=verify-full"

# Create secret
kubectl create secret generic ac-service-secrets \
  --from-literal=DATABASE_URL="${DATABASE_URL}" \
  --from-literal=AC_MASTER_KEY="${AC_MASTER_KEY}" \
  --namespace dark-tower \
  --dry-run=client -o yaml | kubectl apply -f -
```

**Rotating secrets:**

```bash
# Update secret
kubectl create secret generic ac-service-secrets \
  --from-literal=DATABASE_URL="${NEW_DATABASE_URL}" \
  --from-literal=AC_MASTER_KEY="${NEW_AC_MASTER_KEY}" \
  --namespace dark-tower \
  --dry-run=client -o yaml | kubectl apply -f -

# Restart pods to pick up new secret
kubectl rollout restart statefulset/ac-service -n dark-tower
```

**CRITICAL:** Master key rotation requires re-encrypting all signing keys in database. See ADR-0008 for key rotation procedure.

### Kubernetes ConfigMap

**ConfigMap: `ac-service-config`** (namespace: `dark-tower`)

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: ac-service-config
  namespace: dark-tower
data:
  AC_CLUSTER_NAME: "dark-tower-prod"  # Adjust per cluster
```

**Updating ConfigMap:**

```bash
# Edit ConfigMap
kubectl edit configmap ac-service-config -n dark-tower

# Or apply updated manifest
kubectl apply -f infra/services/ac-service/configmap.yaml

# Restart pods to pick up changes
kubectl rollout restart statefulset/ac-service -n dark-tower
```

### Resource Limits

**Current configuration** (from `statefulset.yaml`):

```yaml
resources:
  requests:
    cpu: 500m      # 0.5 CPU cores
    memory: 1Gi    # 1 GiB RAM
  limits:
    cpu: 2000m     # 2 CPU cores
    memory: 2Gi    # 2 GiB RAM
```

**Tuning guidance:**
- **requests**: Guaranteed resources, used for scheduling
- **limits**: Maximum resources, pod killed if exceeded
- Increase if pods OOMKilled or CPU throttled
- Decrease if utilization consistently <30%

### Database Configuration

**PostgreSQL connection pool settings** (hardcoded in `main.rs`, ADR-0012):

- **max_connections**: 20 (increased from 5 for production capacity)
- **min_connections**: 2 (warm connections to reduce latency)
- **acquire_timeout**: 5 seconds (fail fast on connection issues)
- **idle_timeout**: 600 seconds (10 minutes)
- **max_lifetime**: 1800 seconds (30 minutes)
- **statement_timeout**: 5 seconds (fail fast on hung queries)

**PostgreSQL TLS requirements** (ADR-0012):
- Production **MUST** use `sslmode=verify-full`
- Local development may use `sslmode=disable` (warning logged)

---

## Common Deployment Issues

### Issue 1: Database Connection Failures

**Symptoms:**
- Pods stuck in CrashLoopBackOff
- Logs show: `Failed to connect to database: <error>`
- Readiness probe fails

**Causes:**
- `DATABASE_URL` incorrect or missing in Secret
- Database not accessible (network policy, DNS, credentials)
- Database not accepting connections (max_connections reached)
- TLS configuration mismatch (`sslmode` incorrect)

**Resolution:**

```bash
# Verify Secret exists and is mounted
kubectl get secret ac-service-secrets -n dark-tower
kubectl describe pod ac-service-0 -n dark-tower | grep -A 5 "Mounts:"

# Check Secret contents (base64 decode)
kubectl get secret ac-service-secrets -n dark-tower -o jsonpath='{.data.DATABASE_URL}' | base64 -d

# Test database connectivity from pod
kubectl exec -it ac-service-0 -n dark-tower -- /bin/sh
# (if shell not available, use debug container)
kubectl debug -it ac-service-0 -n dark-tower --image=postgres:15 -- psql $DATABASE_URL

# Check PostgreSQL logs for connection rejections
kubectl logs -n dark-tower postgres-0 --tail=100 | grep -i "connection\|authentication"

# Verify network policy allows traffic
kubectl get networkpolicy -n dark-tower
kubectl describe networkpolicy ac-service -n dark-tower
```

**Fix:**
1. Correct `DATABASE_URL` in Secret
2. Verify PostgreSQL credentials
3. Add TLS certificate to Secret if using `sslmode=verify-full`
4. Adjust network policy to allow AC → PostgreSQL traffic (TCP:5432)

### Issue 2: Migration Failures

**Symptoms:**
- `sqlx migrate run` fails with error
- Logs show: `Failed to initialize signing key: <error>`
- Pods crash during startup after database init

**Causes:**
- Migration SQL syntax error
- Missing database permissions
- Concurrent migrations (multiple instances running migration)
- Schema conflicts (existing tables/columns)

**Resolution:**

```bash
# Check migration status
export DATABASE_URL="postgresql://..."
sqlx migrate info

# Check PostgreSQL logs for migration errors
kubectl logs -n dark-tower postgres-0 --tail=100 | grep -i "error\|failed"

# Verify database permissions
psql $DATABASE_URL -c "\du"  # List roles and permissions
psql $DATABASE_URL -c "\l"   # List databases

# Manually inspect failed migration
psql $DATABASE_URL
# \dt  -- List tables
# \d table_name  -- Describe table
```

**Fix:**
1. Rollback failed migration (execute DOWN migration manually)
2. Fix migration SQL syntax
3. Grant required permissions to `ac_user` role
4. Re-run migration
5. If concurrent execution issue, ensure only one instance runs migrations (use init container or migration job)

### Issue 3: Pod Startup Failures

**Symptoms:**
- Pods remain in Pending or ContainerCreating state
- Pods crash immediately after startup
- Liveness probe fails repeatedly

**Causes:**
- Missing Secret or ConfigMap
- Image pull failures (registry authentication, network)
- Insufficient node resources (CPU/memory)
- `AC_MASTER_KEY` invalid (not 32 bytes, not base64-encoded)

**Resolution:**

```bash
# Check pod events
kubectl describe pod ac-service-0 -n dark-tower

# Common events to look for:
# - "FailedMount" → Secret/ConfigMap missing
# - "ImagePullBackOff" → Image not available
# - "Insufficient cpu/memory" → Node resources exhausted

# Verify image exists
kubectl describe pod ac-service-0 -n dark-tower | grep "Image:"
docker pull <image-url>  # Test pull manually

# Check node resources
kubectl top nodes
kubectl describe node <node-name>

# Verify master key format
kubectl get secret ac-service-secrets -n dark-tower -o jsonpath='{.data.AC_MASTER_KEY}' | base64 -d | wc -c
# Should output: 32
```

**Fix:**
1. Create missing Secret/ConfigMap
2. Fix image pull credentials (ImagePullSecret)
3. Scale down other pods or add nodes
4. Regenerate `AC_MASTER_KEY` with correct format:
   ```bash
   openssl rand -base64 32
   ```

### Issue 4: Certificate Issues (TLS)

**Symptoms:**
- Database connection fails with TLS error
- Logs show: `certificate verify failed`
- Pods connect in development but not production

**Causes:**
- `sslmode=verify-full` requires CA certificate
- PostgreSQL certificate not trusted
- Certificate expired or hostname mismatch

**Resolution:**

```bash
# Check DATABASE_URL sslmode
kubectl get secret ac-service-secrets -n dark-tower -o jsonpath='{.data.DATABASE_URL}' | base64 -d

# Verify PostgreSQL certificate
kubectl exec -n dark-tower postgres-0 -- cat /var/lib/postgresql/server.crt

# Check certificate expiration
openssl s_client -connect postgres.dark-tower.svc.cluster.local:5432 -starttls postgres | openssl x509 -noout -dates
```

**Fix:**

**Option A:** Add CA certificate to DATABASE_URL

```bash
# Copy CA cert to Secret
kubectl create secret generic postgres-ca \
  --from-file=ca.crt=/path/to/ca.crt \
  --namespace dark-tower

# Update DATABASE_URL to reference mounted cert
DATABASE_URL="postgresql://user:pass@host:5432/db?sslmode=verify-full&sslrootcert=/etc/postgres-ca/ca.crt"

# Mount Secret in StatefulSet
# Add to statefulset.yaml:
#   volumeMounts:
#   - name: postgres-ca
#     mountPath: /etc/postgres-ca
#     readOnly: true
#   volumes:
#   - name: postgres-ca
#     secret:
#       secretName: postgres-ca
```

**Option B:** Use `sslmode=require` (less secure, not recommended for production)

```bash
DATABASE_URL="postgresql://user:pass@host:5432/db?sslmode=require"
```

**Option C:** Use CloudNativePG certificate (if using CloudNativePG operator)

```bash
# CloudNativePG creates TLS secrets automatically
# Reference in DATABASE_URL:
DATABASE_URL="postgresql://user:pass@postgres-rw.dark-tower.svc.cluster.local:5432/db?sslmode=verify-full&sslrootcert=/controller/certificates/server-ca.crt"
```

### Issue 5: Signing Key Initialization Failures

**Symptoms:**
- Readiness probe fails with `signing_key: unavailable`
- Logs show: `Failed to initialize signing key`
- Token issuance returns 500 error

**Causes:**
- `AC_MASTER_KEY` invalid (wrong length, not base64)
- Database migration not run (signing_keys table missing)
- Key rotation conflict (active key expired, no new key)

**Resolution:**

```bash
# Check logs for specific error
kubectl logs ac-service-0 -n dark-tower | grep "signing key"

# Verify signing_keys table exists
psql $DATABASE_URL -c "\d signing_keys"

# Check for existing keys
psql $DATABASE_URL -c "SELECT key_id, is_active, valid_from, valid_until FROM signing_keys ORDER BY created_at DESC LIMIT 5;"

# Verify AC_MASTER_KEY format
kubectl get secret ac-service-secrets -n dark-tower -o jsonpath='{.data.AC_MASTER_KEY}' | base64 -d | wc -c
# Should be exactly 32 bytes
```

**Fix:**

1. **Missing table:** Run database migrations
   ```bash
   sqlx migrate run
   ```

2. **Invalid master key:** Regenerate and update Secret
   ```bash
   NEW_MASTER_KEY=$(openssl rand -base64 32)
   kubectl create secret generic ac-service-secrets \
     --from-literal=AC_MASTER_KEY="${NEW_MASTER_KEY}" \
     --dry-run=client -o yaml | kubectl apply -f -
   kubectl rollout restart statefulset/ac-service -n dark-tower
   ```
   **WARNING:** Changing master key invalidates all existing signing keys. Only do this in disaster recovery.

3. **Expired keys:** Manually trigger key rotation
   ```bash
   # Call key rotation endpoint (requires service token with rotation scope)
   curl -X POST https://ac-service.dark-tower.svc.cluster.local:8082/internal/rotate-keys \
     -H "Authorization: Bearer <service-token-with-rotation-scope>"
   ```

---

## Smoke Tests

Run these tests immediately after deployment to verify core functionality.

### Test 1: Health Check (Liveness)

**Purpose:** Verify process is running and responsive.

```bash
# Port-forward to pod
kubectl port-forward -n dark-tower ac-service-0 8082:8082 &

# Test health endpoint
curl -i http://localhost:8082/health

# Expected response:
# HTTP/1.1 200 OK
# Content-Length: 2
#
# OK

# Kill port-forward
kill %1
```

**Success criteria:**
- HTTP 200 status
- Response body: `OK`
- Response time: <100ms

### Test 2: Readiness Check

**Purpose:** Verify database connectivity and signing key availability.

```bash
# Port-forward to pod
kubectl port-forward -n dark-tower ac-service-0 8082:8082 &

# Test readiness endpoint
curl -i http://localhost:8082/ready

# Expected response:
# HTTP/1.1 200 OK
# Content-Type: application/json
#
# {"status":"ready","database":"healthy","signing_key":"available"}

# Kill port-forward
kill %1
```

**Success criteria:**
- HTTP 200 status
- JSON body with `status: "ready"`
- `database: "healthy"`
- `signing_key: "available"`
- Response time: <500ms

**If readiness fails:**

```bash
# Check readiness response for specific failure
curl http://localhost:8082/ready | jq .

# Possible failures:
# - database: "unhealthy" → Database connection issue (see Issue 1)
# - signing_key: "unavailable" → No active key (see Issue 5)
# - signing_key: "error" → Key query failed (check logs)
```

### Test 3: JWKS Endpoint

**Purpose:** Verify JWKS endpoint is accessible and returns valid JSON Web Key Set.

```bash
# Port-forward to pod
kubectl port-forward -n dark-tower ac-service-0 8082:8082 &

# Fetch JWKS
curl -i http://localhost:8082/.well-known/jwks.json

# Expected response:
# HTTP/1.1 200 OK
# Content-Type: application/json
#
# {"keys":[{"kty":"OKP","crv":"Ed25519","x":"...","use":"sig","alg":"EdDSA","kid":"auth-us-2025-01"}]}

# Kill port-forward
kill %1
```

**Success criteria:**
- HTTP 200 status
- Content-Type: `application/json`
- JSON array with at least one key
- Each key has: `kty`, `crv`, `x`, `use`, `alg`, `kid`
- `alg: "EdDSA"`, `crv: "Ed25519"`
- Response time: <100ms

**Validate JWKS structure:**

```bash
curl http://localhost:8082/.well-known/jwks.json | jq .

# Verify fields:
jq '.keys[0].kty' # Should be "OKP"
jq '.keys[0].alg' # Should be "EdDSA"
jq '.keys[0].kid' # Should match pattern: auth-{cluster}-{year}-{seq}
```

### Test 4: Service Token Issuance (Optional)

**Purpose:** Verify end-to-end token issuance flow.

**Prerequisites:**
- Test service credentials registered in database
- Or use admin credentials to register new service

**Test with existing credentials:**

```bash
# Port-forward to pod
kubectl port-forward -n dark-tower ac-service-0 8082:8082 &

# Request service token (replace credentials)
curl -i -X POST http://localhost:8082/api/v1/auth/service/token \
  -H "Content-Type: application/x-www-form-urlencoded" \
  -u "test-client-id:test-client-secret" \
  -d "grant_type=client_credentials"

# Expected response:
# HTTP/1.1 200 OK
# Content-Type: application/json
#
# {
#   "access_token": "eyJhbGc...",
#   "token_type": "Bearer",
#   "expires_in": 7200,
#   "scope": "service.write.mh service.read.gc"
# }

# Kill port-forward
kill %1
```

**Success criteria:**
- HTTP 200 status
- JSON response with `access_token`, `token_type`, `expires_in`, `scope`
- `token_type: "Bearer"`
- `access_token` is valid JWT (3 base64 parts separated by dots)
- Response time: <350ms (p99 SLO per ADR-0011)

**Validate token structure:**

```bash
# Extract access_token from response
ACCESS_TOKEN=$(curl -s -X POST http://localhost:8082/api/v1/auth/service/token \
  -u "test-client-id:test-client-secret" \
  -d "grant_type=client_credentials" | jq -r '.access_token')

# Decode JWT header (without verification)
echo $ACCESS_TOKEN | cut -d. -f1 | base64 -d 2>/dev/null | jq .
# Expected: {"alg":"EdDSA","typ":"JWT","kid":"auth-us-2025-01"}

# Decode JWT payload
echo $ACCESS_TOKEN | cut -d. -f2 | base64 -d 2>/dev/null | jq .
# Expected: {"sub":"service_id","service_type":"...","scopes":[...],"iss":"...","iat":...,"exp":...}
```

### Test 5: Metrics Endpoint

**Purpose:** Verify Prometheus metrics are exposed.

```bash
# Port-forward to pod
kubectl port-forward -n dark-tower ac-service-0 8082:8082 &

# Fetch metrics
curl -s http://localhost:8082/metrics | head -50

# Expected output (Prometheus text format):
# # HELP ac_http_requests_total Total number of HTTP requests
# # TYPE ac_http_requests_total counter
# ac_http_requests_total{method="GET",path="/health",status="200"} 42
# ...

# Kill port-forward
kill %1
```

**Success criteria:**
- HTTP 200 status
- Prometheus text format (lines starting with `#` for metadata)
- Metrics present: `ac_http_requests_total`, `ac_db_query_duration_seconds`, `ac_token_issuance_duration_seconds`
- Response time: <1s

---

## Monitoring and Verification

### Key Metrics to Monitor Post-Deployment

**Service health:**

```promql
# Pod restart count (should be 0 after initial deployment)
kube_pod_container_status_restarts_total{namespace="dark-tower",pod=~"ac-service-.*"}

# Pod readiness (should be 1 for all pods)
kube_pod_status_ready{namespace="dark-tower",pod=~"ac-service-.*"}
```

**Error rate:**

```promql
# HTTP 5xx error rate (should be <1%)
sum(rate(ac_http_requests_total{status=~"5.."}[5m])) by (status)
/ sum(rate(ac_http_requests_total[5m])) * 100

# Database query errors (should be 0)
rate(ac_db_query_errors_total[5m])
```

**Latency:**

```promql
# p99 token issuance latency (SLO: <350ms)
histogram_quantile(0.99, rate(ac_token_issuance_duration_seconds_bucket[5m]))

# p99 database query latency (SLO: <50ms)
histogram_quantile(0.99, rate(ac_db_query_duration_seconds_bucket[5m]))

# p99 HTTP request duration
histogram_quantile(0.99, rate(http_request_duration_seconds_bucket{namespace="dark-tower",service="ac-service"}[5m]))
```

**Resource utilization:**

```promql
# CPU usage (should be <70% sustained)
rate(container_cpu_usage_seconds_total{namespace="dark-tower",pod=~"ac-service-.*"}[5m]) * 100

# Memory usage (should be <70% of limit)
container_memory_working_set_bytes{namespace="dark-tower",pod=~"ac-service-.*"}
/ container_spec_memory_limit_bytes * 100

# Database connection pool utilization (should be <80%)
ac_db_connections_active / ac_db_connections_max * 100
```

### Grafana Dashboards

**Recommended dashboards:**
- **AC Service Overview** - Health, error rate, latency, throughput
- **AC Database Performance** - Query latency, connection pool, slow queries
- **Kubernetes Pod Metrics** - CPU, memory, network, restarts

**Key panels:**
1. Request rate (QPS)
2. Error rate (%)
3. p50/p95/p99 latency
4. Database connection pool utilization
5. Active signing keys count
6. Token issuance success rate

### Alerting Rules

**Critical alerts (page on-call):**

```yaml
- alert: ACServiceDown
  expr: up{job="ac-service"} == 0
  for: 1m
  severity: critical

- alert: ACHighErrorRate
  expr: rate(ac_http_requests_total{status=~"5.."}[5m]) / rate(ac_http_requests_total[5m]) > 0.05
  for: 5m
  severity: critical

- alert: ACDatabaseConnectionFailed
  expr: rate(ac_db_connection_errors_total[5m]) > 0
  for: 2m
  severity: critical

- alert: ACNoActiveSigningKey
  expr: ac_active_signing_keys_count == 0
  for: 1m
  severity: critical
```

**Warning alerts (email/Slack):**

```yaml
- alert: ACHighLatency
  expr: histogram_quantile(0.99, rate(ac_token_issuance_duration_seconds_bucket[5m])) > 0.5
  for: 10m
  severity: warning

- alert: ACDatabaseConnectionPoolHigh
  expr: ac_db_connections_active / ac_db_connections_max > 0.8
  for: 5m
  severity: warning

- alert: ACHighMemoryUsage
  expr: container_memory_working_set_bytes{pod=~"ac-service-.*"} / container_spec_memory_limit_bytes > 0.85
  for: 10m
  severity: warning
```

### Logs Analysis

**Useful log queries (if using centralized logging):**

```bash
# Errors in last hour
kubectl logs -n dark-tower -l app=ac-service --since=1h | grep -i "error"

# Failed token issuance attempts
kubectl logs -n dark-tower -l app=ac-service --since=1h | grep "token issuance failed"

# Database connection issues
kubectl logs -n dark-tower -l app=ac-service --since=1h | grep -i "database.*error"

# Slow queries (if logged)
kubectl logs -n dark-tower -l app=ac-service --since=1h | grep "slow query"
```

**Log aggregation queries (Loki/CloudWatch/Splunk):**

```logql
# Loki query: Error rate by pod
sum by (pod) (rate({namespace="dark-tower",app="ac-service"} |~ "(?i)error" [5m]))

# Loki query: Top error messages
topk(10, sum by (error_message) (rate({namespace="dark-tower",app="ac-service"} | json | level="error" [1h])))
```

---

## Emergency Contacts

**On-Call Rotation:** See PagerDuty schedule

**Escalation:**
1. **L1:** On-call SRE (PagerDuty)
2. **L2:** Backend team lead
3. **L3:** Infrastructure architect

**Related Teams:**
- **Database Team:** For PostgreSQL issues (CloudNativePG, migrations)
- **Security Team:** For cryptographic issues (signing keys, master key rotation)
- **Service Teams:** For integration issues (GC, MC, MH authentication failures)

---

## References

- **ADR-0003:** Service Authentication & Federation
- **ADR-0008:** Key Rotation Strategy
- **ADR-0011:** Observability Framework
- **ADR-0012:** Infrastructure Architecture
- **Source Code:** `crates/ac-service/`
- **Kubernetes Manifests:** `infra/services/ac-service/`
- **Database Migrations:** `migrations/`

---

**Document Version:** 1.0
**Last Reviewed:** 2025-12-10
**Next Review:** 2026-01-10
