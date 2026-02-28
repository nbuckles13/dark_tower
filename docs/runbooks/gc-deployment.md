# GC Service Deployment Runbook

**Service**: Global Controller (gc-service)
**Version**: Phase 4 (ADR-0010 Implementation)
**Last Updated**: 2026-02-28
**Owner**: Operations Team

---

## Overview

This runbook covers deployment, rollback, and troubleshooting procedures for the Global Controller service. The GC service is the HTTP/3 API gateway responsible for meeting join orchestration, MC assignment, and geographic routing.

**Critical Service**: GC downtime impacts all meeting join operations. Follow pre-deployment checklist carefully.

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
- [ ] **Test coverage meets minimum** (targeting 90%+ for critical paths)
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
  trivy image gc-service:latest --severity CRITICAL
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
  - MC pod capacity sufficient for expected load

### Coordination

- [ ] **Maintenance window scheduled** (if downtime expected)
  - Dependent services notified (AC, MC, MH, Client)
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
kubectl get deployment gc-service -n dark-tower

# Check current pod status
kubectl get pods -n dark-tower -l app=gc-service

# Check current resource utilization
kubectl top pods -n dark-tower -l app=gc-service

# Verify readiness of current pods
kubectl get pods -n dark-tower -l app=gc-service -o json | jq '.items[].status.conditions[] | select(.type=="Ready")'
```

**Expected output:**
- Deployment shows desired replicas (2+)
- All pods in Running state
- All pods Ready=True
- CPU <70%, Memory <70%

### 2. Database Migration (if required)

**CRITICAL:** Database migrations must complete BEFORE container deployment.

```bash
# Connect to database
export DATABASE_URL="postgresql://gc_user:<password>@postgres.dark-tower.svc.cluster.local:5432/dark_tower?sslmode=verify-full"

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

# Update Deployment with new image
kubectl set image deployment/gc-service \
  gc-service=gc-service:${NEW_VERSION} \
  -n dark-tower

# Verify image updated
kubectl describe deployment gc-service -n dark-tower | grep Image:
```

**Option B: Using kubectl apply (declarative)**

```bash
# Update infra/services/gc-service/deployment.yaml
# Change image tag: gc-service:latest → gc-service:v1.2.3

# Apply updated manifest
kubectl apply -f infra/services/gc-service/deployment.yaml

# Verify change
kubectl describe deployment gc-service -n dark-tower | grep Image:
```

**Option C: Using Skaffold (development)**

```bash
# Build and deploy with Skaffold
skaffold run -p gc-service
```

### 4. Rolling Update Monitoring

Deployments update pods via rolling strategy (maxSurge=1, maxUnavailable=0 for zero-downtime).

**Monitor rollout:**

```bash
# Watch pod status (Ctrl+C to exit)
kubectl get pods -n dark-tower -l app=gc-service -w

# Check rollout status
kubectl rollout status deployment/gc-service -n dark-tower

# Monitor logs from new pod
kubectl logs -f deployment/gc-service -n dark-tower
```

**Expected sequence:**
1. New pod created (beyond current replica count)
2. New pod starts and becomes Ready (health check + readiness check pass)
3. Old pod terminates (SIGTERM sent, 30s drain period)
4. Repeat for all pods

**Typical timeline:**
- Pod startup: 5-15 seconds (database connection + JWKS fetch + TokenManager init)
- Pod termination: 30-35 seconds (graceful shutdown)
- Total per pod: ~45 seconds
- **Total rollout: ~3-4 minutes for 3 replicas**

### 5. Verify Deployment Success

**Pod health:**

```bash
# All pods Running and Ready
kubectl get pods -n dark-tower -l app=gc-service

# Check pod events for errors
kubectl get events -n dark-tower --field-selector involvedObject.kind=Pod,involvedObject.name=gc-service-<pod-suffix>
```

**Logs review:**

```bash
# Check for startup errors
kubectl logs deployment/gc-service -n dark-tower --tail=50

# Look for error patterns
kubectl logs -n dark-tower -l app=gc-service --tail=100 | grep -i "error\|panic\|fatal"
```

**Expected log messages:**
```
Starting Global Controller
Configuration loaded successfully
Connecting to database...
Database connection established
Fetching JWKS from AC service...
JWKS cache initialized
TokenManager started (background refresh)
Prometheus metrics recorder initialized
Global Controller listening on 0.0.0.0:8080
```

### 6. Run Smoke Tests

**See [Smoke Tests](#smoke-tests) section below for detailed test procedures.**

Minimum required smoke tests:
- [ ] Health check returns 200 OK
- [ ] Readiness check returns 200 OK
- [ ] Metrics endpoint returns Prometheus format
- [ ] Authenticated endpoint works (with valid token)

### 7. Monitor Metrics

**Verify metrics collection:**

```bash
# Port-forward to access metrics endpoint
kubectl port-forward -n dark-tower deployment/gc-service 8080:8080 &

# Fetch metrics
curl http://localhost:8080/metrics

# Kill port-forward
kill %1
```

**Check key metrics:**
- `gc_http_request_duration_seconds` - HTTP request latency (p95 <200ms SLO)
- `gc_mc_assignment_duration_seconds` - MC assignment latency (p95 <20ms SLO)
- `gc_db_query_duration_seconds` - Database query latency (p99 <50ms)

**Prometheus queries (if Prometheus available):**

```promql
# Error rate (should be near 0%)
sum(rate(gc_http_requests_total{status_code=~"5.."}[5m])) / sum(rate(gc_http_requests_total[5m]))

# p95 latency (should be <200ms per ADR-0010)
histogram_quantile(0.95, sum by(le) (rate(gc_http_request_duration_seconds_bucket[5m])))

# MC assignment success rate
sum(rate(gc_mc_assignments_total{status="success"}[5m])) / sum(rate(gc_mc_assignments_total[5m]))
```

### 8. Traffic Verification

**Confirm service is receiving traffic:**

```bash
# Check Service endpoints
kubectl get endpoints gc-service -n dark-tower

# Verify traffic reaching pods (requires metrics)
kubectl port-forward -n dark-tower deployment/gc-service 8080:8080 &
curl http://localhost:8080/metrics | grep gc_http_requests_total
kill %1
```

Expected: `gc_http_requests_total` counter increasing over time.

### 9. Post-Deployment Checklist

- [ ] All pods Running and Ready
- [ ] Smoke tests pass (health, ready, metrics, authenticated endpoint)
- [ ] No errors in logs (last 5 minutes)
- [ ] Metrics available in Prometheus
- [ ] Error rate <1% (if baseline traffic exists)
- [ ] p95 latency <200ms (ADR-0010 SLO)
- [ ] MC assignment success rate >99%
- [ ] Database connection pool healthy (<80% utilization)
- [ ] TokenManager refreshing tokens successfully

---

## Rollback Procedure

### When to Rollback

**Immediate rollback criteria** (do not wait):

1. **Pod startup failures**
   - Pods stuck in CrashLoopBackOff >5 minutes
   - Pods failing readiness checks consistently
   - Database connection failures
   - AC JWKS fetch failures

2. **Critical functionality broken**
   - Meeting join endpoint returning 500 errors
   - MC assignment failures >10%
   - Token validation failures (401 on valid tokens)

3. **Severe performance degradation**
   - p95 latency >500ms (2.5x SLO)
   - Error rate >5%
   - Database connection pool exhausted

4. **Security issues discovered**
   - Vulnerability in new code
   - Token validation bypass
   - Authorization failures

**Monitoring period before declaring success:**
- Minimum: 15 minutes post-deployment
- Recommended: 1 hour for major changes
- Critical changes: 24 hours with on-call monitoring

### How to Rollback

**Step 1: Identify previous version**

```bash
# Find previous image version
kubectl rollout history deployment/gc-service -n dark-tower

# Get image from previous revision
kubectl rollout history deployment/gc-service -n dark-tower --revision=<PREVIOUS_REVISION>
```

**Step 2: Rollback Deployment**

```bash
# Rollback to previous revision
kubectl rollout undo deployment/gc-service -n dark-tower

# Or rollback to specific revision
kubectl rollout undo deployment/gc-service -n dark-tower --to-revision=<REVISION>

# Monitor rollback
kubectl rollout status deployment/gc-service -n dark-tower
```

**Step 3: Verify rollback success**

```bash
# Check pods running previous version
kubectl get pods -n dark-tower -l app=gc-service -o jsonpath='{.items[*].spec.containers[0].image}'

# Run smoke tests (see Smoke Tests section)
# Verify health, ready, metrics endpoints
```

**Step 4: Rollback database migration (if applicable)**

**CRITICAL:** Only rollback database if new migration is incompatible with previous code.

```bash
# Connect to database
export DATABASE_URL="postgresql://gc_user:<password>@postgres.dark-tower.svc.cluster.local:5432/dark_tower?sslmode=verify-full"

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
| `DATABASE_URL` | **Yes** | PostgreSQL connection string with TLS | None | `postgresql://gc_user:password@postgres.dark-tower.svc.cluster.local:5432/dark_tower?sslmode=verify-full` |
| `AC_JWKS_URL` | **Yes** | Authentication Controller JWKS endpoint | None | `http://ac-service.dark-tower.svc.cluster.local:8082/.well-known/jwks.json` |
| `AC_TOKEN_URL` | **Yes** | Authentication Controller token endpoint | None | `http://ac-service.dark-tower.svc.cluster.local:8082/api/v1/auth/service/token` |
| `GC_CLIENT_ID` | **Yes** | OAuth client ID for GC service | None | `gc-service` |
| `GC_CLIENT_SECRET` | **Yes** | OAuth client secret for GC service | None | Secret value |
| `BIND_ADDRESS` | No | TCP bind address for HTTP server | `0.0.0.0:8080` | `0.0.0.0:8080` |
| `JWT_CLOCK_SKEW_SECONDS` | No | Allowed clock skew for JWT validation | `60` | `60` |
| `RUST_LOG` | No | Logging level | `info` | `info,gc_service=debug` |

### Kubernetes Secrets

**Secret: `gc-service-secrets`** (namespace: `dark-tower`)

```yaml
apiVersion: v1
kind: Secret
metadata:
  name: gc-service-secrets
  namespace: dark-tower
type: Opaque
data:
  DATABASE_URL: <base64-encoded-connection-string>
  GC_CLIENT_SECRET: <base64-encoded-secret>
```

**Creating secrets:**

```bash
# Create DATABASE_URL (adjust credentials and host)
DATABASE_URL="postgresql://gc_user:REPLACE_PASSWORD@postgres.dark-tower.svc.cluster.local:5432/dark_tower?sslmode=verify-full"

# Create secret
kubectl create secret generic gc-service-secrets \
  --from-literal=DATABASE_URL="${DATABASE_URL}" \
  --from-literal=GC_CLIENT_SECRET="${GC_CLIENT_SECRET}" \
  --namespace dark-tower \
  --dry-run=client -o yaml | kubectl apply -f -
```

**Rotating secrets:**

```bash
# Update secret
kubectl create secret generic gc-service-secrets \
  --from-literal=DATABASE_URL="${NEW_DATABASE_URL}" \
  --from-literal=GC_CLIENT_SECRET="${NEW_GC_CLIENT_SECRET}" \
  --namespace dark-tower \
  --dry-run=client -o yaml | kubectl apply -f -

# Restart pods to pick up new secret
kubectl rollout restart deployment/gc-service -n dark-tower
```

### Kubernetes ConfigMap

**ConfigMap: `gc-service-config`** (namespace: `dark-tower`)

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: gc-service-config
  namespace: dark-tower
data:
  AC_JWKS_URL: "http://ac-service.dark-tower.svc.cluster.local:8082/.well-known/jwks.json"
  AC_TOKEN_URL: "http://ac-service.dark-tower.svc.cluster.local:8082/api/v1/auth/service/token"
  GC_CLIENT_ID: "gc-service"
  JWT_CLOCK_SKEW_SECONDS: "60"
```

### Resource Limits

**Current configuration** (from `deployment.yaml`):

```yaml
resources:
  requests:
    cpu: 250m      # 0.25 CPU cores
    memory: 256Mi  # 256 MiB RAM
  limits:
    cpu: 1000m     # 1 CPU core
    memory: 512Mi  # 512 MiB RAM
```

**Tuning guidance:**
- **requests**: Guaranteed resources, used for scheduling
- **limits**: Maximum resources, pod killed if exceeded
- Increase if pods OOMKilled or CPU throttled
- Decrease if utilization consistently <30%

### Database Configuration

**PostgreSQL connection pool settings** (configured in code per ADR-0012):

- **max_connections**: 10 (default, adjust for production)
- **min_connections**: 2 (warm connections to reduce latency)
- **acquire_timeout**: 5 seconds (fail fast on connection issues)
- **idle_timeout**: 600 seconds (10 minutes)

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
kubectl get secret gc-service-secrets -n dark-tower
kubectl describe pod <gc-pod> -n dark-tower | grep -A 5 "Mounts:"

# Check Secret contents (base64 decode)
kubectl get secret gc-service-secrets -n dark-tower -o jsonpath='{.data.DATABASE_URL}' | base64 -d

# Test database connectivity from pod
kubectl exec -it <gc-pod> -n dark-tower -- psql $DATABASE_URL -c "SELECT 1"

# Check PostgreSQL logs for connection rejections
kubectl logs -n dark-tower postgres-0 --tail=100 | grep -i "connection\|authentication"

# Verify network policy allows traffic
kubectl get networkpolicy -n dark-tower
kubectl describe networkpolicy gc-service -n dark-tower
```

**Fix:**
1. Correct `DATABASE_URL` in Secret
2. Verify PostgreSQL credentials
3. Adjust network policy to allow GC → PostgreSQL traffic (TCP:5432)

### Issue 2: AC JWKS Fetch Failures

**Symptoms:**
- Pods failing readiness checks
- Logs show: `Failed to fetch JWKS` or `JWKS cache unavailable`
- All authenticated endpoints returning 401

**Causes:**
- AC service not running or not ready
- `AC_JWKS_URL` incorrect in ConfigMap
- NetworkPolicy blocking GC → AC traffic
- AC service returning invalid JWKS

**Resolution:**

```bash
# Check AC service is running
kubectl get pods -n dark-tower -l app=ac-service

# Test JWKS endpoint directly
kubectl exec -it <gc-pod> -n dark-tower -- curl -i $AC_JWKS_URL

# Check GC logs for JWKS errors
kubectl logs deployment/gc-service -n dark-tower --tail=100 | grep -i "jwks"

# Verify AC_JWKS_URL in ConfigMap
kubectl get configmap gc-service-config -n dark-tower -o yaml | grep AC_JWKS_URL
```

**Fix:**
1. Ensure AC service is running and ready
2. Correct `AC_JWKS_URL` in ConfigMap
3. Adjust NetworkPolicy to allow GC → AC traffic (TCP:8082)

### Issue 3: TokenManager Refresh Failures

**Symptoms:**
- Logs show: `TokenManager: Failed to refresh token`
- Metrics: `gc_token_refresh_total{status="error"}` incrementing
- GC → MC calls failing with authentication errors

**Causes:**
- AC service rejecting token requests
- `GC_CLIENT_ID` or `GC_CLIENT_SECRET` incorrect
- AC rate limiting GC requests
- Network connectivity issues to AC token endpoint

**Resolution:**

```bash
# Check TokenManager logs
kubectl logs deployment/gc-service -n dark-tower --tail=100 | grep -i "token"

# Test token endpoint directly
kubectl exec -it <gc-pod> -n dark-tower -- curl -X POST $AC_TOKEN_URL \
  -u "${GC_CLIENT_ID}:${GC_CLIENT_SECRET}" \
  -d "grant_type=client_credentials"

# Check AC service logs for rejection reasons
kubectl logs deployment/ac-service -n dark-tower --tail=100 | grep -i "rejected\|invalid"
```

**Fix:**
1. Verify `GC_CLIENT_ID` and `GC_CLIENT_SECRET` are correct
2. Check AC service credentials table for GC service registration
3. Review AC rate limiting configuration

### Issue 4: Pod Startup Failures

**Symptoms:**
- Pods remain in Pending or ContainerCreating state
- Pods crash immediately after startup
- Liveness probe fails repeatedly

**Causes:**
- Missing Secret or ConfigMap
- Image pull failures (registry authentication, network)
- Insufficient node resources (CPU/memory)

**Resolution:**

```bash
# Check pod events
kubectl describe pod <gc-pod> -n dark-tower

# Common events to look for:
# - "FailedMount" → Secret/ConfigMap missing
# - "ImagePullBackOff" → Image not available
# - "Insufficient cpu/memory" → Node resources exhausted

# Verify image exists
kubectl describe pod <gc-pod> -n dark-tower | grep "Image:"
docker pull <image-url>  # Test pull manually

# Check node resources
kubectl top nodes
kubectl describe node <node-name>
```

**Fix:**
1. Create missing Secret/ConfigMap
2. Fix image pull credentials (ImagePullSecret)
3. Scale down other pods or add nodes

### Issue 5: MC Assignment Slow or Failing

**Symptoms:**
- Meeting join latency high (>500ms)
- Logs show: `MC assignment timeout` or `No healthy MCs available`
- Metrics: `gc_mc_assignment_duration_seconds` p95 >20ms

**Causes:**
- No healthy MC pods registered
- MC heartbeats stale in database
- GC → MC gRPC connectivity issues
- Database query for MC selection slow

**Resolution:**

```bash
# Check MC pods are running
kubectl get pods -n dark-tower -l app=mc-service

# Check MC registrations in database
kubectl exec -it <gc-pod> -n dark-tower -- psql $DATABASE_URL -c \
  "SELECT id, region, capacity, current_sessions, last_heartbeat FROM meeting_controllers WHERE last_heartbeat > NOW() - INTERVAL '30 seconds';"

# Test gRPC connectivity to MC
kubectl exec -it <gc-pod> -n dark-tower -- grpcurl -plaintext mc-service.dark-tower.svc.cluster.local:9090 list

# Check database query latency
kubectl logs deployment/gc-service -n dark-tower --tail=100 | grep "select_mc\|mc_assignment"
```

**Fix:**
1. Scale MC pods if none running
2. Verify MC heartbeat mechanism is working
3. Check NetworkPolicy allows GC → MC gRPC traffic (TCP:9090)
4. Add database index on `last_heartbeat` column if query slow

---

## Smoke Tests

Run these tests immediately after deployment to verify core functionality.

### Test 1: Health Check (Liveness)

**Purpose:** Verify process is running and responsive.

```bash
# Port-forward to pod
kubectl port-forward -n dark-tower deployment/gc-service 8080:8080 &

# Test health endpoint
curl -i http://localhost:8080/health

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

**Purpose:** Verify database connectivity and JWKS availability.

```bash
# Port-forward to pod
kubectl port-forward -n dark-tower deployment/gc-service 8080:8080 &

# Test readiness endpoint
curl -i http://localhost:8080/ready

# Expected response:
# HTTP/1.1 200 OK
# Content-Type: application/json
#
# {"status":"ready","database":"healthy","jwks":"available"}

# Kill port-forward
kill %1
```

**Success criteria:**
- HTTP 200 status
- JSON body with `status: "ready"`
- `database: "healthy"`
- `jwks: "available"`
- Response time: <500ms

### Test 3: Metrics Endpoint

**Purpose:** Verify Prometheus metrics are exposed.

```bash
# Port-forward to pod
kubectl port-forward -n dark-tower deployment/gc-service 8080:8080 &

# Fetch metrics
curl -s http://localhost:8080/metrics | head -50

# Expected output (Prometheus text format):
# # HELP gc_http_requests_total Total number of HTTP requests
# # TYPE gc_http_requests_total counter
# gc_http_requests_total{method="GET",endpoint="/health",status_code="200"} 42
# ...

# Kill port-forward
kill %1
```

**Success criteria:**
- HTTP 200 status
- Prometheus text format (lines starting with `#` for metadata)
- Metrics present: `gc_http_requests_total`, `gc_http_request_duration_seconds`
- Response time: <1s

### Test 4: Authenticated Endpoint

**Purpose:** Verify JWT validation and authenticated endpoints work.

```bash
# Port-forward to pod
kubectl port-forward -n dark-tower deployment/gc-service 8080:8080 &

# Get a service token from AC (requires test credentials)
TOKEN=$(curl -s -X POST http://ac-service.dark-tower.svc.cluster.local:8082/api/v1/auth/service/token \
  -u "test-client-id:test-client-secret" \
  -d "grant_type=client_credentials" | jq -r '.access_token')

# Test authenticated endpoint
curl -i http://localhost:8080/api/v1/me \
  -H "Authorization: Bearer $TOKEN"

# Expected response:
# HTTP/1.1 200 OK
# Content-Type: application/json
#
# {"service_id":"test-client-id","service_type":"...","scopes":[...]}

# Kill port-forward
kill %1
```

**Success criteria:**
- HTTP 200 status with valid token
- HTTP 401 with invalid/missing token
- Response time: <200ms

### Test 5: Meeting Join Endpoint (if test meeting exists)

**Purpose:** Verify meeting join flow end-to-end.

```bash
# Port-forward to pod
kubectl port-forward -n dark-tower deployment/gc-service 8080:8080 &

# Test meeting join (requires valid meeting code and token)
curl -i http://localhost:8080/api/v1/meetings/TEST-CODE \
  -H "Authorization: Bearer $TOKEN"

# Expected response:
# HTTP/1.1 200 OK
# Content-Type: application/json
#
# {"meeting_id":"...","mc_assignment":{"mc_id":"...","mc_url":"..."},"token":"..."}

# Kill port-forward
kill %1
```

**Success criteria:**
- HTTP 200 status for valid meeting code
- HTTP 404 for invalid meeting code
- Response includes MC assignment details
- Response time: <200ms (SLO)

### Test 6: Meeting Creation

**Purpose:** Verify meeting creation endpoint creates meetings with valid codes and secure defaults.

```bash
# Port-forward to GC pod
kubectl port-forward -n dark-tower deployment/gc-service 8080:8080 &

# Step 1: Obtain a user JWT via AC register endpoint
USER_TOKEN=$(curl -s -X POST http://ac-service.dark-tower.svc.cluster.local:8082/api/v1/auth/register \
  -H "Content-Type: application/json" \
  -d '{"email":"smoke-test@example.com","password":"SmokeTest123!","display_name":"Smoke Test"}' \
  | jq -r '.access_token')

# If user already exists, login instead
if [ -z "$USER_TOKEN" ] || [ "$USER_TOKEN" = "null" ]; then
  USER_TOKEN=$(curl -s -X POST http://ac-service.dark-tower.svc.cluster.local:8082/api/v1/auth/login \
    -H "Content-Type: application/json" \
    -d '{"email":"smoke-test@example.com","password":"SmokeTest123!"}' \
    | jq -r '.access_token')
fi

# Step 2: Create a meeting
RESPONSE=$(curl -s -w "\n%{http_code}" -X POST http://localhost:8080/api/v1/meetings \
  -H "Authorization: Bearer $USER_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"display_name":"Smoke Test Meeting"}')

HTTP_CODE=$(echo "$RESPONSE" | tail -1)
BODY=$(echo "$RESPONSE" | sed '$d')

echo "HTTP Status: $HTTP_CODE"
echo "Response: $BODY"

# Step 3: Verify response
MEETING_ID=$(echo "$BODY" | jq -r '.meeting_id')
MEETING_CODE=$(echo "$BODY" | jq -r '.meeting_code')

echo "Meeting ID: $MEETING_ID"
echo "Meeting Code: $MEETING_CODE"

# Step 4: Verify meeting code format (12 alphanumeric characters)
if echo "$MEETING_CODE" | grep -qE '^[A-Za-z0-9]{12}$'; then
  echo "PASS: Meeting code is 12 alphanumeric characters"
else
  echo "FAIL: Meeting code format invalid: $MEETING_CODE"
fi

# Step 5: Verify response does NOT leak sensitive fields
if echo "$BODY" | jq -e '.join_token_secret' > /dev/null 2>&1; then
  echo "FAIL: Response contains join_token_secret — sensitive data leak"
else
  echo "PASS: Response does not contain join_token_secret"
fi

# Kill port-forward
kill %1
```

**Success criteria:**
- HTTP 201 status
- Response body contains `meeting_id` (non-empty UUID)
- Response body contains `meeting_code` (12 alphanumeric characters)
- Response includes secure defaults (e.g., meeting status)
- Response does NOT contain `join_token_secret` or other sensitive fields
- Response time: <500ms

---

## Monitoring and Verification

### Key Metrics to Monitor Post-Deployment

**Service health:**

```promql
# Pod restart count (should be 0 after initial deployment)
kube_pod_container_status_restarts_total{namespace="dark-tower",pod=~"gc-service-.*"}

# Pod readiness (should be 1 for all pods)
kube_pod_status_ready{namespace="dark-tower",pod=~"gc-service-.*"}
```

**Error rate:**

```promql
# HTTP 5xx error rate (should be <1%)
sum(rate(gc_http_requests_total{status_code=~"5.."}[5m]))
/ sum(rate(gc_http_requests_total[5m])) * 100

# MC assignment failure rate (should be <1%)
sum(rate(gc_mc_assignments_total{status!="success"}[5m]))
/ sum(rate(gc_mc_assignments_total[5m])) * 100
```

**Latency:**

```promql
# p95 HTTP latency (SLO: <200ms)
histogram_quantile(0.95, sum by(le) (rate(gc_http_request_duration_seconds_bucket[5m])))

# p95 MC assignment latency (SLO: <20ms)
histogram_quantile(0.95, sum by(le) (rate(gc_mc_assignment_duration_seconds_bucket[5m])))

# p99 database query latency (SLO: <50ms)
histogram_quantile(0.99, sum by(le) (rate(gc_db_query_duration_seconds_bucket[5m])))
```

**Resource utilization:**

```promql
# CPU usage (should be <70% sustained)
rate(container_cpu_usage_seconds_total{namespace="dark-tower",pod=~"gc-service-.*"}[5m])

# Memory usage (should be <70% of limit)
container_memory_working_set_bytes{namespace="dark-tower",pod=~"gc-service-.*"}
/ container_spec_memory_limit_bytes * 100
```

### Grafana Dashboards

**Recommended dashboards:**
- **GC Overview** - Request rate, error rate, latency, MC assignments
- **GC SLOs** - Error budget, availability, latency compliance

See `infra/grafana/dashboards/gc-overview.json` and `gc-slos.json`.

### Alerting Rules

**Critical alerts (page on-call):**
- `GCDown` - No GC pods running for >1 minute
- `GCHighErrorRate` - Error rate >1% for >5 minutes
- `GCHighLatency` - p95 latency >200ms for >5 minutes
- `GCMCAssignmentSlow` - MC assignment p95 >20ms for >5 minutes
- `GCDatabaseDown` - Database error rate >50% for >1 minute

See `infra/docker/prometheus/rules/gc-alerts.yaml` for full list.

### Post-Deploy Monitoring Checklist: Meeting Creation

Use this checklist after any deployment that touches meeting creation code (handler, repository, migrations, or meeting-related configuration). For routine deployments that do not affect meeting creation, the general monitoring section above is sufficient.

**1-hour observation window** (mandatory for meeting creation changes):

Monitor continuously for the first hour after deployment. Do not declare success until 1 hour has passed without anomalies.

**30-minute check:**

```promql
# Meeting creation rate > 0 (traffic is flowing)
sum(rate(gc_meeting_creation_total[5m])) > 0

# Meeting creation error rate < 1%
sum(rate(gc_meeting_creation_total{status="error"}[5m]))
/ sum(rate(gc_meeting_creation_total[5m])) < 0.01

# Meeting creation p95 latency < 500ms
histogram_quantile(0.95,
  sum by(le) (rate(gc_meeting_creation_duration_seconds_bucket[5m]))
) < 0.500
```

- [ ] Meeting creation rate > 0 (confirms endpoint is receiving traffic)
- [ ] Meeting creation error rate < 1%
- [ ] Meeting creation p95 latency < 500ms
- [ ] No `code_collision` errors in `gc_meeting_creation_failures_total{error_type="code_collision"}`
- [ ] No unexpected `forbidden` errors in `gc_meeting_creation_failures_total{error_type="forbidden"}`

**2-hour check:**

- [ ] No `GCMeetingCreationStopped` alert firing
- [ ] Meeting creation failure rate trend is stable (not increasing)
- [ ] No pod restarts since deployment completed
- [ ] Logs show no repeated error patterns related to meeting creation

**4-hour check:**

- [ ] No limit exhaustion patterns (no spike in `error_type="forbidden"`)
- [ ] Code collision count = 0 (`gc_meeting_creation_failures_total{error_type="code_collision"}`)
- [ ] Meeting creation latency trend is stable
- [ ] Database query latency for meeting operations is within baseline

**24-hour check:**

- [ ] All meeting creation alerts clear (no `GCMeetingCreationStopped`, `GCMeetingCreationFailureRate`, `GCMeetingCreationLatencyHigh`)
- [ ] Daily meeting creation volume matches pre-deployment baseline expectations
- [ ] No anomalous patterns in error type distribution
- [ ] Remove any temporary monitoring overrides or escalation holds

**Rollback criteria** (trigger immediate rollback if any):
- Meeting creation error rate > 5% for 10 minutes
- Meeting creation p95 latency > 500ms for 5 minutes (aligned with `GCMeetingCreationLatencyHigh` alert threshold)
- Pod restart rate > 1/hour
- Any `code_collision` errors (investigate before rollback — may indicate deeper issue)

```bash
# Rollback command
kubectl rollout undo deployment/gc-service -n dark-tower

# Note: Created meetings remain as inert rows in the database.
# No data cleanup is required after rollback.
```

### Logs Analysis

**Useful log queries:**

```bash
# Errors in last hour
kubectl logs -n dark-tower -l app=gc-service --since=1h | grep -i "error"

# Failed meeting joins
kubectl logs -n dark-tower -l app=gc-service --since=1h | grep "join.*failed"

# Database connection issues
kubectl logs -n dark-tower -l app=gc-service --since=1h | grep -i "database.*error"

# MC assignment failures
kubectl logs -n dark-tower -l app=gc-service --since=1h | grep -i "mc_assignment.*error"
```

---

## Emergency Contacts

**On-Call Rotation:** See PagerDuty schedule

**Escalation:**
1. **L1:** On-call SRE (PagerDuty)
2. **L2:** GC service owner / Backend team lead
3. **L3:** Infrastructure architect

**Related Teams:**
- **Database Team:** For PostgreSQL issues (CloudNativePG, migrations)
- **AC Team:** For authentication/token issues
- **MC Team:** For MC assignment and connectivity issues
- **Infrastructure Team:** For Kubernetes, network, resource issues

---

## References

- **ADR-0010:** Global Controller Architecture
- **ADR-0011:** Observability Framework
- **ADR-0012:** Infrastructure Architecture
- **Source Code:** `crates/gc-service/`
- **Kubernetes Manifests:** `infra/services/gc-service/`
- **Database Migrations:** `migrations/`

---

**Document Version:** 1.1
**Last Reviewed:** 2026-02-28
**Next Review:** 2026-03-28
