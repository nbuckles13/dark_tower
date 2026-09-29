# GC Service Deployment Runbook

**Service**: Global Controller (gc-service)
**Version**: Phase 4 (ADR-0010 Implementation)
**Last Updated**: 2026-03-27
**Owner**: Operations Team

---

## Overview

This runbook covers deployment, rollback, and troubleshooting procedures for the Global Controller service. The GC service is the HTTP/3 API gateway responsible for meeting join orchestration, MC assignment, and geographic routing.

**Critical Service**: GC downtime impacts all meeting join operations. Follow pre-deployment checklist carefully.

---

## Manifest Structure

GC service Kubernetes manifests are managed via Kustomize with a base/overlay pattern:

```
infra/
├── services/gc-service/                    # Base manifests
│   ├── kustomization.yaml                  # Explicit resource list
│   ├── config.env                          # configMapGenerator source (name hash-suffixed)
│   ├── deployment.yaml
│   ├── service.yaml
│   ├── secret.yaml
│   ├── pdb.yaml
│   ├── network-policy.yaml
│   └── service-monitor.yaml               # Present in dir but not in kustomization.yaml (needs Prometheus Operator CRD)
└── kubernetes/overlays/kind/
    └── services/gc-service/
        └── kustomization.yaml             # Kind overlay — refs base, adds Kind-specific labels
```

- **Base** (`infra/services/gc-service/`): Contains all production manifests. The `kustomization.yaml` explicitly lists each resource. Files like `service-monitor.yaml` are present in the directory but omitted from `kustomization.yaml` when they require CRDs not available in all environments.
- **Kind overlay** (`infra/kubernetes/overlays/kind/services/gc-service/`): References the base and adds Kind-specific labels.
- Deploy with: `kubectl apply -k infra/kubernetes/overlays/kind/services/gc-service/`

---

## Table of Contents

1. [Pre-Deployment Checklist](#pre-deployment-checklist)
2. [Deployment Steps](#deployment-steps)
3. [Rollback Procedure](#rollback-procedure)
4. [Configuration Reference](#configuration-reference)
5. [Common Deployment Issues](#common-deployment-issues)
6. [Smoke Tests](#smoke-tests)
7. [Monitoring and Verification](#monitoring-and-verification)
8. [OTel Collector Upgrade Discipline](#otel-collector-upgrade-discipline)

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

- [ ] **Story 2 task 12: this GC release is a PREREQUISITE of MC's, not an
      independent change.** GC now revives an ENDED meeting assignment on the
      next join (`atomic_assign`), so `assign_meeting` is called once per
      meeting INCARNATION. The MC release that ships alongside it ends a meeting
      when its last participant leaves and calls `NotifyMeetingEnded`; against a
      GC without this fix, every later join to that meeting id fails until the
      retention cleanup removes the row (`GC_RETENTION_DAYS`, 7 days by default).
      **Order: forward GC → MC → MH; rollback MH → MC → GC.** GC is first
      forward and last back. The same total order, and why MC/MH's own
      sender_id rule wins where the two appear to conflict, is recorded in
      `mc-deployment.md` §Coordination; a one-sided note is how the pair gets
      separated. The change is query-only
      (no migration), so rolling GC's image back is clean once MC is back.

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

> **Mechanism (ADR-0038 §2):** the dev Kind cluster runs migrations as the pre-rollout
> `db-migrate` Job (`infra/services/db-migrate/`, image `infra/docker/db-migrate/`), which
> `setup.sh` runs to Complete before applying the environment root — that is also the intended
> production shape (a pre-upgrade hook / sync-wave). The operator-shell `sqlx` procedure below
> is the manual fallback for a non-Kind environment; it is **not** how the dev cluster is
> migrated. Dev-cluster failures: `docs/runbooks/devloop-validation.md` §6.7 (`REASON=migration-failed`).

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

**Option B: Using kubectl apply -k (declarative, Kustomize)**

```bash
# Update infra/services/gc-service/deployment.yaml
# Change image tag: gc-service:latest → gc-service:v1.2.3

# Apply via Kustomize overlay (Kind environment)
kubectl apply -k infra/kubernetes/overlays/kind/services/gc-service/

# Verify change
kubectl describe deployment gc-service -n dark-tower | grep Image:
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
- [ ] CORS preflight from an allowed origin returns 200 + `Access-Control-Allow-Origin` (stanza a)
- [ ] CORS preflight from a disallowed origin returns 403 with no `Access-Control-Allow-Origin` (stanza b)
- [ ] Telemetry proxy accepts an authenticated OTLP POST — 202 (stanza c)
- [ ] Telemetry proxy rejects an unauthenticated POST — 401 (stanza d)
- [ ] Telemetry proxy rejects an oversize body — 413 (stanza e)
- [ ] Telemetry proxy rate-limits a per-user burst — 202s then 429s (stanza f)

#### CORS + Telemetry Proxy Smoke Tests (R-1 / R-2)

**Purpose:** Verify the browser-facing CORS allowlist (`crates/gc-service/src/middleware/cors_observer.rs`, `crates/gc-service/src/routes/mod.rs::build_cors_layer`) and the sanitizing OTLP telemetry proxy (`crates/gc-service/src/handlers/telemetry.rs::ingest`) behave correctly after deploy.

**Prerequisites (shared by every stanza below):**

```bash
# Port-forward to a GC pod
kubectl port-forward -n dark-tower deployment/gc-service 8080:8080 &
```

- An allowed origin must be configured in `CORS_ALLOWED_ORIGINS` (this runbook assumes `http://localhost:5173`; confirm the live value with `kubectl get deployment/gc-service -n dark-tower -o jsonpath='{.spec.template.spec.containers[?(@.name=="gc-service")].env[?(@.name=="CORS_ALLOWED_ORIGINS")].valueFrom.configMapKeyRef.name}'`, which gives the content-hash-suffixed ConfigMap name, then `kubectl get configmap <that-name> -n dark-tower -o yaml | grep CORS_ALLOWED_ORIGINS`). The allowlist is fail-closed (empty ⇒ no origin allowed) and never `*`.
- A user access token is needed for the authenticated telemetry stanzas. Obtain one with the AC-login pattern from Test 5 (camelCase `accessToken` per the API contract):

```bash
USER_TOKEN=$(curl -s -X POST http://ac-service.dark-tower.svc.cluster.local:8082/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d "{\"email\":\"$SMOKE_EMAIL\",\"password\":\"$SMOKE_PASSWORD\"}" \
  | jq -r '.accessToken')
```

- The telemetry stanzas POST the canned fixture `infra/smoke/empty-otlp-metrics.bin` (a valid `ExportMetricsServiceRequest` with zero data points) with `Content-Type: application/x-protobuf`. Stanzas expecting **202** additionally require the in-cluster OTel collector to be reachable (an unreachable collector yields 502; an empty `OTEL_COLLECTOR_ENDPOINT` yields 503).

**(a) CORS preflight from an ALLOWED origin → 200 + ACAO**

```bash
curl -i -X OPTIONS http://localhost:8080/api/v1/meetings \
  -H "Origin: http://localhost:5173" \
  -H "Access-Control-Request-Method: POST" \
  -H "Access-Control-Request-Headers: authorization,content-type"
```

Expected:

```
HTTP/1.1 200 OK
Access-Control-Allow-Origin: http://localhost:5173
```

> **NOTE (behavior vs. expectation):** an allowed preflight returns **200 OK, NOT 204**. `tower_http`'s `CorsLayer` short-circuits a genuine preflight with 200, and `cors_preflight_observer` passes that 200 through untouched — see the unit test `allowed_preflight_passes_through_200_with_acao`, which asserts `StatusCode::OK`. The echoed `Access-Control-Allow-Origin` is the exact configured origin, never `*` (the layer is built explicit-origins-only with `allow_credentials(false)`).

**(b) CORS preflight from a DISALLOWED origin → 403, no ACAO**

```bash
curl -i -X OPTIONS http://localhost:8080/api/v1/meetings \
  -H "Origin: https://evil.example.com" \
  -H "Access-Control-Request-Method: POST"
```

Expected: `HTTP/1.1 403 Forbidden`, with **no** `Access-Control-Allow-Origin` header, no `Access-Control-Allow-Credentials`, and an empty body. `cors_preflight_observer` rewrites the layer's denied 200-no-ACAO into a fresh 403 and records `gc_cors_preflight_total{origin_class="denied",status="403"}` (the denied-preflight warn log carries the structured `requested_origin` field).

**(c) Telemetry proxy — AUTHENTICATED OTLP POST → 202**

```bash
curl -i -X POST http://localhost:8080/api/v1/telemetry/v1/metrics \
  -H "Authorization: Bearer $USER_TOKEN" \
  -H "Content-Type: application/x-protobuf" \
  --data-binary @infra/smoke/empty-otlp-metrics.bin
```

Expected: `HTTP/1.1 202 Accepted`. The proxy decodes the OTLP request, applies the deny-by-default PII filter (a no-op for the empty fixture), re-encodes, and forwards to the collector. A 502 means the collector is unreachable (`GcError::BadGateway`); a 503 means the proxy is disabled (empty `OTEL_COLLECTOR_ENDPOINT`).

**(d) Telemetry proxy — UNAUTHENTICATED → 401**

```bash
curl -i -X POST http://localhost:8080/api/v1/telemetry/v1/metrics \
  -H "Content-Type: application/x-protobuf" \
  --data-binary @infra/smoke/empty-otlp-metrics.bin
```

Expected:

```
HTTP/1.1 401 Unauthorized
WWW-Authenticate: Bearer realm="dark-tower-api", error="invalid_token"
```

The route sits behind `require_user_auth`; a missing Bearer token is rejected before the handler runs.

**(e) Telemetry proxy — OVERSIZE body (> 256 KiB) → 413**

```bash
# 300000 bytes (~293 KiB): above the 256 KiB user-facing limit, below the 512 KiB
# (2x) layer ceiling, so it hits the handler's exact-limit gate (status="rejected_size").
head -c 300000 /dev/zero > /tmp/oversize-otlp.bin

curl -i -X POST http://localhost:8080/api/v1/telemetry/v1/metrics \
  -H "Authorization: Bearer $USER_TOKEN" \
  -H "Content-Type: application/x-protobuf" \
  --data-binary @/tmp/oversize-otlp.bin

rm -f /tmp/oversize-otlp.bin
```

Expected: `HTTP/1.1 413 Payload Too Large` with body code `PAYLOAD_TOO_LARGE`. The size check (`body.len() > telemetry_proxy_max_bytes`; default `DEFAULT_TELEMETRY_PROXY_MAX_BYTES` = 262144) runs FIRST — before content-type and before OTLP decode — so the oversize body is rejected regardless of content. A body above 512 KiB (2×) is instead rejected by the route `DefaultBodyLimit` layer (still 413, but no `rejected_size` metric).

**(f) Telemetry proxy — per-user RATE-LIMIT burst → 202s then 429s**

```bash
# Use a token whose `sub` has an unspent budget (a dedicated smoke account is
# cleanest: the GCRA cell is keyed on JWT `sub`, so re-logging the same user
# does NOT reset it). Acquire it from a SEPARATE account than $USER_TOKEN:
BURST_TOKEN=$(curl -s -X POST http://ac-service.dark-tower.svc.cluster.local:8082/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d "{\"email\":\"$SMOKE_BURST_EMAIL\",\"password\":\"$SMOKE_BURST_PASSWORD\"}" \
  | jq -r '.accessToken')

# Fire 70 rapid sequential POSTs.
for i in $(seq 1 70); do
  curl -s -o /dev/null -w "%{http_code}\n" -X POST \
    http://localhost:8080/api/v1/telemetry/v1/metrics \
    -H "Authorization: Bearer $BURST_TOKEN" \
    -H "Content-Type: application/x-protobuf" \
    --data-binary @infra/smoke/empty-otlp-metrics.bin
done | sort | uniq -c
```

Expected: a mix of `202` then `429` — roughly the first ~60 return **202** and the remainder return **429** (`RATE_LIMIT_EXCEEDED`). The per-user limiter is a `governor` GCRA quota of `DEFAULT_TELEMETRY_PROXY_RATE_LIMIT_PER_MINUTE` = 60 requests/minute keyed on JWT `sub`, so burst capacity is 60 and cells replenish at ~1/second — the exact 202/429 split depends on how long the loop takes. The 429 rejection (`status="rejected_rate"`, and `gc_telemetry_rate_limited_total{reason="per_user"}`) fires before OTLP decode/forward.

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

**ConfigMap: `gc-service-config`** (namespace: `dark-tower`). It is **generated** from
`infra/services/gc-service/config.env` by `configMapGenerator`, and the live object's name carries a content
hash (`gc-service-config-<hash>`, ADR-0038 §2). The block below is the rendered shape,
with the hash omitted.

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
# (the ConfigMap name is content-hash-suffixed; resolve it through the Deployment)
kubectl get configmap -n dark-tower -o yaml \
  "$(kubectl get deployment/gc-service -n dark-tower \
     -o jsonpath='{.spec.template.spec.containers[?(@.name=="gc-service")].env[?(@.name=="AC_JWKS_URL")].valueFrom.configMapKeyRef.name}')" \
  | grep AC_JWKS_URL
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
1. If an MC instance (`mc-0` / `mc-1`) has no running pod, restore that singleton Deployment to `replicas: 1` — **MC does not scale by replicas** (extra replicas make joins fail; see `docs/runbooks/gc-incident-response.md` Scenario 3, Remediation Scenario A, for the capacity levers)
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
# {"status":"ready","database":"healthy","acJwks":"available"}

# Kill port-forward
kill %1
```

**Success criteria:**
- HTTP 200 status
- JSON body with `status: "ready"`
- `database: "healthy"`
- `acJwks: "available"`
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
  -d "grant_type=client_credentials" | jq -r '.accessToken')

# Test authenticated endpoint
curl -i http://localhost:8080/api/v1/me \
  -H "Authorization: Bearer $TOKEN"

# Expected response:
# HTTP/1.1 200 OK
# Content-Type: application/json
#
# {"sub":"test-client-id","scopes":[...],"serviceType":"..."}

# Kill port-forward
kill %1
```

**Success criteria:**
- HTTP 200 status with valid token
- HTTP 401 with invalid/missing token
- Response time: <200ms

### Test 5: Meeting Join Endpoint

**Purpose:** Verify meeting join flow end-to-end — token validation, meeting lookup, MC assignment, and join token issuance.

**Prerequisites:** A valid meeting must exist. Use smoke test 6 (Meeting Creation) first, or use a known test meeting code.

```bash
# Port-forward to pod
kubectl port-forward -n dark-tower deployment/gc-service 8080:8080 &

# Step 1: Obtain a user token (reuse from test 6 or get fresh)
USER_TOKEN=$(curl -s -X POST http://ac-service.dark-tower.svc.cluster.local:8082/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"email":"smoke-test@example.com","password":"SmokeTest123!"}' \
  | jq -r '.accessToken')

# Step 2: Join the meeting
MEETING_CODE="REPLACE_WITH_VALID_CODE"  # From test 6 output
JOIN_RESPONSE=$(curl -s -w "\n%{http_code}" http://localhost:8080/api/v1/meetings/${MEETING_CODE} \
  -H "Authorization: Bearer $USER_TOKEN")

HTTP_CODE=$(echo "$JOIN_RESPONSE" | tail -1)
BODY=$(echo "$JOIN_RESPONSE" | sed '$d')

echo "HTTP Status: $HTTP_CODE"
echo "Response: $BODY"

# Step 3: Verify response structure
MEETING_ID=$(echo "$BODY" | jq -r '.meetingId')
MC_ID=$(echo "$BODY" | jq -r '.mcAssignment.mcId')
MC_URL=$(echo "$BODY" | jq -r '.mcAssignment.mcUrl')
JOIN_TOKEN=$(echo "$BODY" | jq -r '.token')

echo "Meeting ID: $MEETING_ID"
echo "MC ID: $MC_ID"
echo "MC URL: $MC_URL"
echo "Join token present: $([ -n "$JOIN_TOKEN" ] && [ "$JOIN_TOKEN" != "null" ] && echo 'yes' || echo 'no')"

# Step 4: Verify MC assignment is a healthy MC
kubectl exec -it deployment/gc-service -n dark-tower -- \
  psql $DATABASE_URL -c "SELECT id, status, last_heartbeat FROM meeting_controllers WHERE id = '${MC_ID}';"

# Step 5: Verify invalid meeting code returns 404
INVALID_RESPONSE=$(curl -s -o /dev/null -w "%{http_code}" \
  http://localhost:8080/api/v1/meetings/INVALID-CODE-999 \
  -H "Authorization: Bearer $USER_TOKEN")
echo "Invalid code HTTP status: $INVALID_RESPONSE (expect 404)"

# Step 6: Verify unauthenticated request returns 401
UNAUTH_RESPONSE=$(curl -s -o /dev/null -w "%{http_code}" \
  http://localhost:8080/api/v1/meetings/${MEETING_CODE})
echo "Unauthenticated HTTP status: $UNAUTH_RESPONSE (expect 401)"

# Step 7: Verify join metrics incremented
curl -s http://localhost:8080/metrics | grep -E "gc_meeting_join_total|gc_meeting_join_duration"

# Kill port-forward
kill %1
```

**Success criteria:**
- HTTP 200 status for valid meeting code with valid token
- Response contains `meetingId` (non-empty UUID)
- Response contains `mcAssignment` with `mcId` and `mcUrl` (assigned MC is healthy/active)
- Response contains `token` (join token for MC WebTransport session)
- HTTP 404 for invalid meeting code
- HTTP 401 for missing/invalid token
- `gc_meeting_join_total` counter incremented
- Response time: <200ms (SLO)

### Test 6: Meeting Creation

**Purpose:** Verify meeting creation endpoint creates meetings with valid codes and secure defaults.

```bash
# Port-forward to GC pod
kubectl port-forward -n dark-tower deployment/gc-service 8080:8080 &

# Step 1: Obtain a user JWT via AC register endpoint
USER_TOKEN=$(curl -s -X POST http://ac-service.dark-tower.svc.cluster.local:8082/api/v1/auth/register \
  -H "Content-Type: application/json" \
  -d '{"email":"smoke-test@example.com","password":"SmokeTest123!","displayName":"Smoke Test"}' \
  | jq -r '.accessToken')

# If user already exists, login instead
if [ -z "$USER_TOKEN" ] || [ "$USER_TOKEN" = "null" ]; then
  USER_TOKEN=$(curl -s -X POST http://ac-service.dark-tower.svc.cluster.local:8082/api/v1/auth/login \
    -H "Content-Type: application/json" \
    -d '{"email":"smoke-test@example.com","password":"SmokeTest123!"}' \
    | jq -r '.accessToken')
fi

# Step 2: Create a meeting
RESPONSE=$(curl -s -w "\n%{http_code}" -X POST http://localhost:8080/api/v1/meetings \
  -H "Authorization: Bearer $USER_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"displayName":"Smoke Test Meeting"}')

HTTP_CODE=$(echo "$RESPONSE" | tail -1)
BODY=$(echo "$RESPONSE" | sed '$d')

echo "HTTP Status: $HTTP_CODE"
echo "Response: $BODY"

# Step 3: Verify response
MEETING_ID=$(echo "$BODY" | jq -r '.meetingId')
MEETING_CODE=$(echo "$BODY" | jq -r '.meetingCode')

echo "Meeting ID: $MEETING_ID"
echo "Meeting Code: $MEETING_CODE"

# Step 4: Verify meeting code format (12 alphanumeric characters)
if echo "$MEETING_CODE" | grep -qE '^[A-Za-z0-9]{12}$'; then
  echo "PASS: Meeting code is 12 alphanumeric characters"
else
  echo "FAIL: Meeting code format invalid: $MEETING_CODE"
fi

# Step 5: Verify response does NOT leak sensitive fields (neither snake_case nor camelCase form)
if echo "$BODY" | jq -e '.join_token_secret, .joinTokenSecret | select(. != null)' > /dev/null 2>&1; then
  echo "FAIL: Response contains joinTokenSecret/join_token_secret — sensitive data leak"
else
  echo "PASS: Response does not contain joinTokenSecret/join_token_secret"
fi

# Kill port-forward
kill %1
```

**Success criteria:**
- HTTP 201 status
- Response body contains `meetingId` (non-empty UUID)
- Response body contains `meetingCode` (12 alphanumeric characters)
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
sum(increase(gc_meeting_creation_total[5m])) > 0

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
- [ ] No unexpected capacity exhaustion in `gc_meeting_creation_failures_total{error_type="org_limit"}`
- [ ] **Zero** `gc_meeting_creation_failures_total{error_type=~"org_inactive|org_not_provisioned"}` (see note below)
- [ ] No unexpected role denials in `gc_meeting_creation_failures_total{error_type="forbidden"}`

> **`error_type="forbidden"` changed meaning.** Before the meeting-refusal-cause split it covered
> both org capacity exhaustion and role denial; it now means **role denial only**. Capacity
> exhaustion is `org_limit`. A checklist or dashboard query still selecting `forbidden` for
> capacity will report clean *during a real cap exhaustion*. If you are carrying the old
> association, re-read the four selectors above.
>
> **`org_inactive` and `org_not_provisioned` are expected to read a permanent zero** — a `0` is
> healthy (they are present-at-zero from process start); *absence* of the series is a scrape/export
> fault, not calm. They are organization-state faults with no legitimate cause in this system, so any
> non-zero value is the signal. It is most likely right after a deploy, which makes this checklist the
> highest-signal place to catch them.
> If either is non-zero, do **not** treat it as a code regression and do **not** roll back on that
> basis: check first whether GC and AC are pointed at the same database (`gc-service-secrets` and
> `ac-service-secrets` each carry an independent `DATABASE_URL`, and nothing enforces that they
> agree), and whether the database was reset or restored. Alert `GCMeetingCreationOrgStateInvalid`
> covers the same condition; triage is
> `gc-incident-response.md#scenario-8-meeting-creation-limit-exhaustion`.

**2-hour check:**

- [ ] No `GCMeetingCreationStopped` alert firing
- [ ] Meeting creation failure rate trend is stable (not increasing)
- [ ] No pod restarts since deployment completed
- [ ] Logs show no repeated error patterns related to meeting creation

**4-hour check:**

- [ ] No limit exhaustion patterns (no spike in `error_type="org_limit"`)
- [ ] `error_type=~"org_inactive|org_not_provisioned"` still zero
- [ ] Code collision count = 0 (`gc_meeting_creation_failures_total{error_type="code_collision"}`)
- [ ] Meeting creation latency trend is stable
- [ ] Database query latency for meeting operations is within baseline

**24-hour check:**

- [ ] All meeting creation alerts clear (no `GCMeetingCreationStopped`, `GCMeetingCreationFailureRate`, `GCMeetingCreationLatencyHigh`, `GCMeetingCreationOrgStateInvalid`)
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

### Post-Deploy Monitoring Checklist: Join Flow

Use this checklist after any deployment that touches meeting join code (join handler, MC assignment, join token issuance, or join-related configuration). For routine deployments that do not affect the join path, the general monitoring section above is sufficient.

**15-minute check:**

```promql
# Join rate (should be > 0 if traffic is flowing)
sum(increase(gc_meeting_join_total[5m]))

# Join failure rate (should be < 1%)
sum(rate(gc_meeting_join_failures_total[5m]))
/ sum(rate(gc_meeting_join_total[5m]))

# Join latency p95 (SLO: < 200ms)
histogram_quantile(0.95,
  sum by(le) (rate(gc_meeting_join_duration_seconds_bucket[5m]))
)
```

- [ ] `gc_meeting_join_total` rate stable (confirms join traffic is flowing)
- [ ] `gc_meeting_join_duration_seconds` p95 within SLO (<200ms)
- [ ] `gc_meeting_join_failures_total` not spiking (failure rate <1%)
- [ ] No new `GCHighJoinFailureRate` or `GCHighJoinLatency` alerts firing

**1-hour check:**

- [ ] Join success rate trend is stable (not degrading)
- [ ] No pod restarts since deployment completed
- [ ] MC assignment latency within baseline (`gc_mc_assignment_duration_seconds` p95 <20ms)
- [ ] Logs show no repeated error patterns related to join flow

**4-hour check:**

- [ ] All join flow alerts clear
- [ ] Join latency trend is stable (no drift toward SLO boundary)
- [ ] No anomalous patterns in join failure reasons
- [ ] Database query latency for join operations within baseline

**Rollback criteria** (trigger immediate rollback if any):

- `gc_meeting_join_failures_total` rate >5% for 10 minutes
- `gc_meeting_join_duration_seconds` p95 >500ms for 5 minutes (2.5x SLO)
- `GCHighJoinFailureRate` or `GCHighJoinLatency` alert fires and does not resolve within 5 minutes

```bash
# Rollback command
kubectl rollout undo deployment/gc-service -n dark-tower

# Note: In-flight join requests will fail; clients will retry.
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

## OTel Collector Upgrade Discipline

The dev OTel collector (`infra/services/otel-collector/`) is the in-cluster sink
for AC/GC/MC/MH OTLP-gRPC traces and the GC `/api/v1/telemetry` OTLP-HTTP proxy
path (R-2). This section is the upgrade discipline for it and the triage home for
the `OTelExportFailureRate` alert.

> **STATUS 2026-09-28 — THIS SECTION PREDATES ADR-0038 AND ITS CONFIG-ONLY
> MECHANICS ARE NOW WRONG.** The collector config is a content-addressed
> `configMapGenerator` (`infra/services/otel-collector/collector.yaml`), so a
> config-only change RENAMES the ConfigMap and `apply -k` ROLLS the collector
> pod, exactly as an image change does. Every statement below that a config-only
> change "alters nothing in the pod template" or needs a manual restart is
> superseded. The consequence to weigh before relying on the config-only
> exemptions here: a config-only change now RESTARTS the singleton collector,
> which under R-54 fail-hard-at-init is the same hazard as an image change.
> Rewrite tracked in `docs/TODO.md` (owner operations).
>
> **"A collector change" means one of TWO things, and they have different
> procedures.** This section was first written for IMAGE changes (a tag bump,
> alone or with its ConfigMap), which alter the pod template and so roll the pod
> on their own. A CONFIG-ONLY change (just `collector.yaml` — e.g. a metric-name
> allowlist addition) alters nothing in the pod template, so an `apply -k` leaves
> the running collector on the config it already loaded. Three statements here
> were once correct for the image case and silent or wrong for the config-only one
> (the change-window rule, the rollback procedure, and a triage-ladder count).
> When you read any statement below about changing or reverting the collector,
> ask WHICH KIND it is scoped to — and when you add one, say.

### Blast radius (read this first): THREE of four services, live now

**The hazard is ACTIVE on this branch. Do not discount it.** An earlier revision of
this section said no service called `init_otel` yet and that a collector
outage had "no service impact" — that was true before R-55 and is false now. It is
recorded here rather than silently deleted because a runbook that *understates* a
hazard is the failure mode this section exists to prevent.

**Which services actually probe the collector, and which does not:**

| Service | Probes the collector at init? | Why |
|---|---|---|
| AC | **Yes** | `OTEL_ENABLED: "true"` in the Kind overlay + egress rule on 4317 |
| GC | **Yes** | same |
| MC | **Yes** | same |
| MH | **No** | no `OTEL_ENABLED`, no `OTLP_ENDPOINT`, **and no 4317 egress rule** |

MH's `main.rs` *does* call `init_otel`, but `Config::from_env` defaults
`otel_enabled` to false and nothing in `infra/services/mh-service/` or the Kind
overlay turns it on, so it never opens a connection to the collector.

**Say "three of four", never "all four".** Two practical consequences:

- A verification run that brings the stack up against a new collector exercises
  **AC, GC and MC**. MH's leg passes because it never runs — reporting it as
  four-of-four coverage is a vacuous green.
- **MH carries a trap for whoever enables it.** Setting `OTEL_ENABLED=true` for MH
  without first adding the collector egress rule to
  `infra/services/mh-service/network-policy.yaml` will CrashLoop MH on the
  **NetworkPolicy**, not on the collector — and the symptom (fail-hard at init,
  collector unreachable) looks identical to a collector problem. Add the egress
  rule first.

### Causal chain

R-54 OTel init is **fail-hard at init**: if `init_otel` cannot reach the collector
at startup, the service exits non-zero. So a botched collector image/config
upgrade makes AC, GC and MC fail init *simultaneously* → `CrashLoopBackoff` across
three of the four services. A bad collector change is a multi-service outage
trigger, not a localized observability issue.

**What `init_otel` actually probes, because it changes which failures matter.**
The probe is a `tonic::transport::Endpoint::connect` with a short connect timeout
(`crates/common/src/observability/otel.rs`) — a **connection** probe, not a data
export. So:

- A collector that is **gone** (OOMKilled, CrashLooping, evicted, not yet Ready)
  fails the probe → services fail init.
- A collector that is **up but refusing or dropping data** (the `memory_limiter`
  shedding under pressure, a filter discarding datapoints) still accepts the
  connection → services start normally and lose telemetry instead.

That asymmetry is deliberate and is why `memory_limiter` is first on both
pipelines: it converts what would be an OOMKill — and therefore a three-service
init outage — into bounded telemetry loss. **Do not remove it** to "let the
collector use all its memory."

### Mitigation: upgrade in a separate change window

- Upgrade the collector **image** (an image bump, or an image bump with its
  ConfigMap) in a **separate change window** from any service deploy. Never bundle
  it with an AC/GC/MC/MH rollout. A **config-only** change is scoped out of this
  rule on purpose — see §Config-only changes for why and for its own procedure.
- Verify the collector is `Ready` **independently** (its readiness gate / the
  cluster-health env-test) *before* and *without* rolling any service.

### Pre-upgrade checklist

Run these **before** committing a new tag. Steps 1-3 are executable; do not
substitute release notes for any of them — the previously pinned 0.103.1 is the
standing proof that a component's presence is not derivable from its version
number.

1. **Acceptance suite against the candidate image, reading the committed config.**

   ```bash
   scripts/otel-collector/acceptance.sh <candidate-image>
   ```

   It extracts `config.yaml` from the committed
   `infra/services/otel-collector/collector.yaml` (so it cannot drift from what
   ships) and runs the two-writer, single-writer-idle, clock-skew, staleness and
   filtering cases. On a readiness failure it prints the `components` hint.

2. **Component presence, from the image itself** — if step 1 fails at startup:

   ```bash
   kubectl run comp --rm -i --restart=Never \
     --image=<candidate-image> -- components
   ```

   `delta_to_cumulative`, `filter`, `transform`, `memory_limiter`, `health_check`,
   the `otlp` receiver and the `prometheus` exporter must all be listed.

3. **Readiness surface still answers.** The probes and the cluster-setup gate both
   hit the `health_check` extension at `/` on `:13133`. If a future image drops or
   renames that extension, the liveness probe, the readiness probe,
   `deploy_otel_collector()`'s `rollout status` in `infra/kind/scripts/setup.sh`, and
   `crates/env-tests/tests/00_cluster_health.rs` all break **together** and the
   symptom is a collector that never goes Ready.

4. **Confirm no service deploy is scheduled in the same window.**

5. **After rollout, verify the running image is the one that was tested.** The
   deployment pins by *tag*, not `tag@sha256` (see §Rollback for why), so the tag
   alone does not prove which bytes are running:

   ```bash
   kubectl get pod -n dark-tower -l app=otel-collector \
     -o jsonpath='{.items[*].status.containerStatuses[*].imageID}'
   ```

   Compare against the verified digest recorded in the comment on the `image:`
   line in `infra/services/otel-collector/deployment.yaml`.

### Config-only changes (ConfigMap edit, no image change)

Steps 1-5 above assume an image change. A change to
`infra/services/otel-collector/collector.yaml` alone (a new allowlisted metric
name, a label rule, a processor setting) follows this path instead.

**The hazard is a SILENTLY STALE collector, not a crash.** The ConfigMap is not
content-hashed and the pod template carries no checksum annotation, so applying
a changed ConfigMap changes nothing in the pod spec, and the collector reads
`--config` once at startup with no file watcher. An un-restarted pod keeps the
previous config in memory while the committed file is correct and
`dt-guard client-metrics-export` is green. **Skip-symptom:** a metric name
present in the committed allowlist and absent from Prometheus. That reads
identically to "no browser is running".

1. **Run the acceptance suite against the committed config.** It extracts the
   committed ConfigMap, so it validates what actually ships:

   ```bash
   scripts/otel-collector/acceptance.sh
   ```

2. **Restart the collector.** This is **automatic** only through
   `deploy_otel_collector()` in `infra/kind/scripts/setup.sh`, and only when the
   apply reports the ConfigMap `configured`. It is **MANUAL** on any cluster not
   brought up through `setup.sh`, and after a bare `kubectl apply -k` or a
   hand-edited ConfigMap:

   ```bash
   kubectl rollout restart deployment/otel-collector -n dark-tower
   kubectl rollout status deployment/otel-collector -n dark-tower --timeout=180s
   ```

   `rollout status` alone is **not** evidence of a restart. With no rollout in
   progress it returns "successfully rolled out" immediately.

3. **Verify the running process started after the content was written.** Run
   all three checks in order. Each one on its own is fail-open.

   ```bash
   # (a) BEFORE the apply: capture the pod uid
   kubectl get pod -l app=otel-collector -n dark-tower -o jsonpath='{.items[*].metadata.uid}'
   # ... apply + restart ...
   # (a) AFTER: capture it again; it MUST differ
   kubectl get pod -l app=otel-collector -n dark-tower -o jsonpath='{.items[*].metadata.uid}'
   # (b) the new pod's startTime must be STRICTLY LATER than the ConfigMap's last change
   kubectl get pod -l app=otel-collector -n dark-tower -o jsonpath='{.items[*].status.startTime}'
   kubectl get configmap otel-collector-config -n dark-tower --show-managed-fields \
     -o jsonpath='{range .metadata.managedFields[*]}{.time}{"\n"}{end}' | sort | tail -1
   # (c) THEN confirm the content
   kubectl get configmap otel-collector-config -n dark-tower \
     -o jsonpath='{.data.config\.yaml}' | grep <new-name>
   ```

   Capture the uid **before the apply** so that the check also covers a pod the
   apply itself replaced. `--show-managed-fields` is required because kubectl
   strips `managedFields` otherwise. `crates/env-tests/tests/01_mh_deployment_config.rs`
   uses the same source.
   **The content check alone is fail-open.** The ConfigMap is mounted without
   `subPath`, so the kubelet resyncs the file in place on a pod that never
   restarted. The file can be correct on disk while the process still holds the
   old config. (The collector image is distroless, so `kubectl exec ... grep`
   against the mounted file does not run. That is why (c) reads the ConfigMap.
   Because the mount is a projection of the ConfigMap, (a) plus (b) are what tie
   that content to the running process.)

   **What each leg tells you when it fails, so the diagnosis is not crossed.**
   (a) or (b) failing means *the running process predates this config*: the
   O-A staleness failure, and the remedy is a restart. (c) failing while (a)
   and (b) pass means *the pod is current but the ConfigMap does not hold what
   was intended*: the apply or the overlay dropped it, and the remedy is in the
   edit. Read alike, a staleness failure gets triaged as a content bug.

   **What (c) is and is not.** It is an API-server read of the same desired
   state the committed file declares and `dt-guard client-metrics-export`
   already pins. So it confirms that the apply actually WROTE what the file says
   (a partially failed apply, or an overlay that lost the key, both show here).
   It is a control-plane round trip. It observes nothing about the collector
   that is running.

   **A residual no config-inspection check closes.** A projection that is
   stale but still valid YAML passes all three legs: the collector starts
   cleanly, goes Ready, `rollout status` succeeds, the uid changed, `startTime`
   postdates the ConfigMap change, and the ConfigMap holds the new name, while
   the process serves the previous filter list. `status.startTime` is when the
   kubelet accepted the pod, which precedes volume population, so ordering (b)
   is not strictly airtight against a configmap watch event that has not yet
   landed. The earlier form of this check, a grep of the mounted file, had the
   same blind spot and concealed it. The only control that closes it is
   BEHAVIOURAL: observe a new name actually traverse the deployed pipeline to
   Prometheus. Do not reach for another config-inspection variant to close it —
   every member of that family shares this gap.

   **Do NOT run that behavioural check casually against the deployed
   collector.** An injected datapoint lands in the cluster's real Prometheus,
   survives `metric_expiration` plus retention, and is indistinguishable from
   browser traffic. Worse, a name inside any loaded alert rule's selector can
   distort that alert — and the dangerous direction is the SILENT one. For a
   ratio alert such as `MCMediaMissingKeyMaterial`, injecting into the
   DENOMINATOR dilutes the ratio and can hold a genuinely firing alert below its
   threshold. (Injecting only into the numerator does not page anyone: with no
   denominator series the `sum(rate(...))` is an empty vector, the division is
   empty, and the rule's non-zero-denominator guard already contains that case.)
   A "synthetic" `org_id` does not contain either effect: the rule's `sum()`s
   are unqualified, so there is no `org_id` selector for the marking to fall
   outside — it helps whoever reads the series later and protects nothing. The
   procedure, and the constraint that its probe name must appear in NO loaded
   rule's selector ON EITHER SIDE of any ratio (re-checked against
   `infra/docker/prometheus/rules/` each time, never assumed), is tracked with
   the collector config-staleness entry in `docs/TODO.md`.
   `scripts/otel-collector/acceptance.sh` runs the same traversal against a
   scratch collector, which is the safe default.

**A green layer 7 says NOTHING about this mechanism.** A fresh cluster's apply
reports `created`, not `configured`, so the restart branch never fires there.
If you edit the ConfigMap and reason "layer 7 was green", you have learned
nothing about whether the change is live.

**Change windows and rollback:**

- A config-only change is **NOT** subject to the separate-change-window rule in
  §Mitigation. That rule exists for R-54 fail-hard-at-init. A rejected config
  stalls the rollout with the old pod still Ready and serving, so it does not
  take the collector away from the services that probe it.
- §Rollback's "image tag and ConfigMap are one atomic unit" **still binds**. A
  config-only REVERT is safe. An image revert is not, unless the ConfigMap is
  reverted in the same change.

### Triage: which failure mode am I looking at?

The key triage step is distinguishing the two modes:

- **Runtime degradation** — the `OTelExportFailureRate` warning alert fires.
  Spans/metrics are dropping, but the **data plane is UP** and users are
  unaffected. This is fail-soft: the export pipeline is degraded, the services
  are healthy.
- **Init-time collector failure** — pod `Unready` / `CrashLoopBackoff` across
  AC/GC/MC (not MH — see §Blast radius). The **services are DOWN**. This is the
  fail-hard path: the collector is unreachable at service init.

  > **`OTelExportFailureRate` is still dormant, and the reason is NOT the one this
  > runbook used to give.** It previously said "dormant until R-55". R-55 has
  > landed; the alert is dormant because `dt_otel_export_failures_total` **has no
  > emitter anywhere in `crates/`** — nothing increments it. So it cannot fire, and
  > its silence says nothing about export health. Do not chase a non-firing alert,
  > and do not read its silence as a healthy export path. Same note is carried at
  > the rule itself in `infra/docker/prometheus/rules/otel-alerts.yaml`.

- **Client metrics stopped arriving, but the services are fine.** The collector is
  up and the data plane is healthy, but `dt_client_*` series are absent. This mode
  did not exist before R-27 and is the most likely one you will actually meet — see
  §When the client series go quiet below, because **absence is the designed
  behaviour of five different controls** and is indistinguishable from "no browser
  is running" unless you check.

### Rollback

> **THE IMAGE TAG AND THE ConfigMap ARE ONE ATOMIC UNIT. REVERTING THE TAG ALONE
> IS A CRASHLOOP.**
>
> `infra/services/otel-collector/collector.yaml` references the
> `delta_to_cumulative` processor, which **does not exist in 0.103.1**. Reverting
> the image without reverting the ConfigMap gives the collector a config naming a
> component it does not have: it fails at startup, every time, immediately. And a
> collector that is genuinely down takes AC, GC and MC with it on their next
> restart — so the "safe" half of a rollback is the half that causes the outage.

**Procedure.** Revert `deployment.yaml` (tag) and `collector.yaml` **in the same
change**, then `kubectl apply -k` the otel-collector overlay. Because the collector
is a single Deployment and the tag change alters its pod template, the rollout
happens on its own and that is the whole rollback. Verify with the `imageID` check
in the pre-upgrade checklist that the pod is running what you think it is.

**A CONFIG-ONLY revert is NOT the whole rollback at `apply -k`.** Reverting just
`collector.yaml` (for example, backing out an allowlist addition — the safe
direction) changes nothing in the pod template, so the apply leaves the running
collector on the config it already loaded. Follow §Config-only changes for the
restart and the three-leg check; stopping here reproduces the stale-config state
that section exists to prevent, while following this procedure verbatim.

**Rollout mechanics are on your side — do not "fix" them.** The Deployment has
`replicas: 1` and **no `strategy:` block**, so the defaults apply (`maxUnavailable`
25% → 0, `maxSurge` 25% → 1): a new pod must pass readiness on `:13133` before the
old one is terminated. A bad image therefore **stalls** the rollout with the old
collector still serving, and `kubectl rollout status` hangs rather than the fleet
falling over. Do not add `strategy: Recreate` or `maxUnavailable: 1`. The windows
that *are* genuinely exposed are a fresh cluster bring-up (`setup.sh`) and a
node/pod reschedule, because in both cases there is no old pod to keep serving.

#### The rollback has a one-way floor, and a browser is what sets it

**Once a delta-emitting SDK bundle has been served to any browser, the collector
can never be reverted below the tag that introduced `delta_to_cumulative`.**

The SDK exports **delta** temporality so that summing across browsers happens in
the collector and no per-browser identity label is needed. A delta-emitting browser
against a collector without `delta_to_cumulative` is the failure this whole design
exists to fix: same-identity deltas are not summed, the counter oscillates between
writers' raw per-interval values, `rate()` reads ≈ 0, and delta histogram points are
dropped outright. It is silent — the series still exists and still has plausible
values.

**Nothing in the cluster will tell you a browser is what makes the revert unsafe.**
A browser runs whatever bundle its tab loaded; you cannot roll the client fleet
forward atomically and you cannot roll it back at all. There is no per-browser `up`
signal and there structurally cannot be one (a per-session `service.instance.id` is
barred as a participant dimension, ADR-0036 §11), so you cannot even enumerate which
bundles are live.

**The floor rests on tag immutability, and that is an assumption, not a guarantee.**
`deployment.yaml` pins by tag rather than `tag@sha256`, because `setup.sh`'s preload
path (`kustomize | grep image:` → `podman save` → `kind load image-archive`) is
unproven with a digest reference, and a broken preload breaks whole-cluster
bring-up — a worse failure than the one immutability would prevent. The mitigation
is the `imageID`-against-digest check in the pre-upgrade checklist. If an upstream
tag is ever re-pushed, that check is what catches it.

#### During a temporality transition the fleet is mixed, and the metric is wrong

Recorded, not fixed — there is no clean remedy that does not reach for the barred
per-instance identity label.

While a temporality change is rolling out, some tabs still emit the old bundle's
**cumulative** datapoints while new tabs emit **delta**. Because every browser at a
given `{client_version, org_id, key_custody}` deliberately shares one stream
identity, both would write to the *same* stream: `delta_to_cumulative` accumulates
the delta writers and passes the cumulative writers through untouched, and the
exporter then interleaves two unrelated cumulative series under one identity —
spurious resets and wrong totals that **look live**.

The collector's `filter/client_metric_temporality` closes this by **dropping**
non-delta sums and histograms, so the stale-bundle case reads as *missing* data
rather than as plausible *wrong* data. That is the right trade, and it is the
reason the next section exists: a dropped fleet and a quiet fleet look the same
from the dashboard.

In dev the window is bounded by the env-test reloading its browsers. It is written
down because it is a standing property of *no per-instance label + an uncontrolled
client + a temporality change*, and it does not go away when this design meets a
real fleet.

### When the client series go quiet

**Absence is the designed behaviour of five separate controls on the metrics path,
and on a deployed cluster all five are indistinguishable from "no browser is
running".** Because the presence of a client series is the **only** liveness signal
the browser fleet has (no per-browser `up`, §11), you cannot infer anything from
quiet without checking. The drops are:

| Control | Drops |
|---|---|
| `filter/client_metric_names` | any metric outside the media-metric allowlist |
| `filter/client_metric_temporality` | non-delta sums/histograms (a stale browser bundle) |
| `transform/client_metric_labels` (`keep_keys`) | any label key outside the curated set |
| `delta_to_cumulative` (`max_streams`) | streams beyond the cap |
| `memory_limiter` | everything, while shedding under memory pressure |

**Triage ladder, cheapest first:**

1. `up{job="otel-collector"}` — is the client-series endpoint being scraped at all?
   If `0`, nothing below matters; the collector or the scrape is the problem.
2. `up{job="otel-collector-telemetry"}` — is the collector's *own* telemetry being
   scraped? This is a **separate job on purpose**, so that `up{job="otel-collector"}`
   keeps meaning "the client-series endpoint".
3. The `otelcol_*` drop counters on that second job. This is the **only** channel
   through which the five drops above are observable; without it a dropped fleet and
   a quiet Sunday are the same picture. Read the family names off the running pod on
   the committed tag — they have moved between collector versions, and some of them
   are healthy-non-zero (the name allowlist discards non-media metrics on **every**
   batch, by design), so a naive "points dropped > 0" reading is always true.
4. If the quiet series is a **recently allowlisted** name, check that the running
   collector pod started after the ConfigMap changed (§Config-only changes, step 3).
   A stale pod passes 1-3 cleanly.
5. Only if 1-4 are clean does quiet mean "no browser is emitting".

#### Reading client series after a collector outage: timestamps are ARRIVAL time

`transform/client_metric_clock` restamps every delta datapoint's `time` and
`start_time` to the collector's own clock on arrival. This is load-bearing — without
it, browsers with skewed clocks sharing one stream identity make
`delta_to_cumulative` decide "out of order" across machines, and a single browser
stamped far in the future silently stops an honest fleet's accumulation. But it
means **stored timestamps are collector-arrival time, not browser-event time**, and
that changes how you read an incident:

1. **Onset is misattributed.** Browsers buffer while the collector is unreachable and
   flush on recovery; every buffered point is stamped at *arrival*. So "when did the
   drops start", read from `dt_client_media_frames_dropped_total`, returns the
   **recovery moment**, not the true onset — and correlating client drops against
   MC-side events will be off by the full outage duration. This is the trap in
   `mc-incident-response.md` Scenario 16, whose entire triage is that correlation.
2. **Ratio-shaped alerts survive this; absolute-rate ones do not.**
   `MCMediaMissingKeyMaterial` is a ratio, and the browser buffered numerator and
   denominator in the same batch, so both compress equally and the ratio is
   preserved. That is a property worth keeping deliberately: an absolute-rate
   threshold on a `dt_client_*` series would fire on the recovery artefact alone. The
   convention is recorded in `docs/observability/alert-conventions.md`.
3. **The artefact is distinguishable, via `up`.** If you see a client-series spike,
   check whether `up{job="otel-collector"} == 0` anywhere in the preceding window. If
   it was, the spike is a flush artefact and the true onset is somewhere inside the
   gap. (This is a second reason the exporter is *pull*, not remote-write: collector
   loss becomes `up == 0` on a job, which turns out to be load-bearing for reading
   the data correctly and not merely for liveness.)
4. **A long outage loses the oldest data outright.** The browser's OTLP exporter queue
   is bounded, so a sufficiently long outage yields a *partial* re-dated burst: the
   visible spike understates what was lost, and the gap is not recoverable.

### Break-glass / escape hatch

If a collector problem is blocking a service deploy that cannot wait for a
separate window, disable per-service OTel init so the service can boot without the
collector:

- **AC (R-55 landed, task #25):** OTel init is gated by the explicit `OTEL_ENABLED` flag.
  Break-glass = set `OTEL_ENABLED=false` in AC's ConfigMap (or drop the Kind overlay's
  `OTEL_ENABLED=true` patch) and restart AC; it boots with the OTel layer off and no collector
  dependency. Blanking `OTLP_ENDPOINT` is no longer the lever — presence-gating was replaced by
  the boolean.
- **GC (R-55 landed, task #26):** OTel init is gated by the explicit `OTEL_ENABLED` flag.
  Break-glass = set `OTEL_ENABLED=false` in GC's ConfigMap (or drop the Kind overlay's
  `OTEL_ENABLED=true` patch) and restart GC; it boots with the OTel layer off and no collector
  dependency. GC is a 2-replica `Deployment` (`maxUnavailable: 0`, PDB `minAvailable: 1`) —
  unlike AC's StatefulSet, a bad `OTEL_ENABLED=true` rollout does **not** cause an outage: new
  pods CrashLoop before ever binding their HTTP/gRPC ports, so they never pass readiness and the
  rollout stalls with the old ReplicaSet still serving all traffic. It also does **not**
  self-heal — `kubectl rollout status` hangs indefinitely until an operator intervenes
  (`kubectl rollout undo`, or flip the break-glass lever above).
- **MC (landed):** OTel init is gated by the explicit `OTEL_ENABLED` flag, same as AC and GC.
  Break-glass = set `OTEL_ENABLED=false` in MC's ConfigMap (or drop the Kind overlay's
  `infra/kubernetes/overlays/kind/services/mc-service/configmap-otel-patch.yaml`, which is what
  flips it to `"true"` for dev) and restart MC; it boots with the OTel layer off and no collector
  dependency.
- **MH: no lever is needed, because MH has no collector dependency to break.** It has no
  `OTEL_ENABLED` and no `OTLP_ENDPOINT` in `infra/services/mh-service/` or the Kind overlay, so
  `otel_enabled` defaults false and `init_otel` is never called. **Do not "enable OTel on MH" as a
  break-glass step or as a convenience** — it has no 4317 egress rule either, so turning it on
  CrashLoops MH on the NetworkPolicy while presenting the same symptom as a collector outage. Add
  the egress rule to `infra/services/mh-service/network-policy.yaml` first.

### Escalation / ownership

The collector is owned by **infrastructure**. A fleet-wide `CrashLoop` caused by a
collector change is a **deployment-discipline incident** (a bundled/botched
upgrade), not a service bug — route it accordingly and restore the collector
before investigating individual services.

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
- **Kubernetes Manifests (base):** `infra/services/gc-service/`
- **Kubernetes Manifests (Kind overlay):** `infra/kubernetes/overlays/kind/services/gc-service/`
- **Database Migrations:** `migrations/`

---

**Document Version:** 1.3
**Last Reviewed:** 2026-03-31
**Next Review:** 2026-04-30
