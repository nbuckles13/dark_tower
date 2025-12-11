# AC Service Incident Response Runbook

**Service**: Authentication Controller (ac-service)
**Owner**: Security Team
**On-Call Rotation**: PagerDuty - Dark Tower Auth Team
**Last Updated**: 2025-12-10

---

## Table of Contents

1. [Severity Classification](#severity-classification)
2. [Escalation Paths](#escalation-paths)
3. [Common Failure Scenarios](#common-failure-scenarios)
4. [Diagnostic Commands](#diagnostic-commands)
5. [Recovery Procedures](#recovery-procedures)
6. [Postmortem Template](#postmortem-template)

---

## Severity Classification

Use this table to classify incidents and determine response times:

| Severity | Description | Response Time | Examples | Escalation |
|----------|-------------|---------------|----------|------------|
| **P1 (Critical)** | Service down, complete auth failure | **15 minutes** | All token issuance failing (>95% error rate), Database unreachable, All pods crash-looping, JWKS endpoint returning 500s | Immediate page, escalate to Engineering Lead after 30 min |
| **P2 (High)** | Degraded performance, partial failures | **1 hour** | High latency (p99 > 1s), 10-50% error rate, Single pod failing, Rate limiting blocking legitimate traffic | Page if persists > 15 min, escalate to Service Owner after 2 hours |
| **P3 (Medium)** | Non-critical issue, workaround available | **4 hours** | Single client affected, JWKS cache issues, Metrics unavailable, Non-critical alerts firing | Slack notification, escalate if not resolved in 8 hours |
| **P4 (Low)** | Minor issue, no immediate impact | **24 hours** | Log noise, Cosmetic dashboard issues, Deprecated endpoint warnings | Normal ticket, review in next on-call handoff |

### Severity Upgrade Triggers

Automatically upgrade severity if:
- P2 persists for > 2 hours → Upgrade to P1
- P3 affects multiple clients → Upgrade to P2
- Any security breach suspected → Upgrade to P1 + notify Security Team immediately

---

## Escalation Paths

### Initial Response

**On-Call Engineer** (First Responder):
1. Acknowledge alert within 5 minutes
2. Assess severity using table above
3. Post incident notice in `#incidents` Slack channel
4. Begin investigation using diagnostic commands
5. Engage additional specialists as needed

### Escalation Chain

```
On-Call Engineer (0-15 min)
    ↓ (if not resolved in 30 min for P1, 2h for P2)
Service Owner / Tech Lead
    ↓ (if architectural decision needed)
Engineering Manager
    ↓ (if multi-service impact)
Infrastructure Team / SRE Lead
```

### Specialist Contacts

| Team | When to Engage | Contact |
|------|----------------|---------|
| **Database Team** | Database connectivity issues, migration failures, query performance | #database-oncall, PagerDuty: DB-Team |
| **Infrastructure/SRE** | Kubernetes issues, network problems, resource constraints | #infra-oncall, PagerDuty: SRE |
| **Security Team** | Suspected breach, key compromise, audit log failures | #security-incidents (CRITICAL ONLY) |
| **Global Controller Team** | Cross-service auth issues, federation problems | #gc-oncall |
| **Product/Business** | Customer impact assessment, external communications | Engineering Manager escalates |

### External Dependencies

- **PostgreSQL**: Managed by Database Team (see Database Team escalation)
- **Redis** (future): Managed by Infrastructure Team
- **Kubernetes**: Managed by Infrastructure Team
- **Prometheus/Grafana**: Managed by Observability Team (#observability)

---

## Common Failure Scenarios

### Scenario 1: Database Connection Failures

**Symptoms**:
- 503 Service Unavailable on `/ready` endpoint
- Error logs: `connection refused`, `too many connections`, `authentication failed`
- Metrics: `ac_db_queries_total{status="error"}` spiking
- All token issuance requests failing

**Diagnosis**:

```bash
# 1. Check readiness endpoint
curl http://ac-service:8080/ready
# Expected: {"status":"not_ready","database":"unhealthy","error":"..."}

# 2. Check pod status
kubectl get pods -n dark-tower -l app=ac-service

# 3. Check recent logs for DB errors
kubectl logs -n dark-tower -l app=ac-service --tail=100 | grep -i "database\|connection\|sqlx"

# 4. Check database connectivity from pod
kubectl exec -it -n dark-tower deployment/ac-service -- sh -c 'psql $DATABASE_URL -c "SELECT 1"'

# 5. Check connection pool metrics
curl http://ac-service:9090/metrics | grep -E "sqlx_|ac_db_"

# 6. Check database service status
kubectl get svc -n dark-tower postgresql
kubectl get endpoints -n dark-tower postgresql
```

**Common Root Causes**:

1. **Database Pod Down**: PostgreSQL pod crashed or evicted
   - Check: `kubectl get pods -n dark-tower -l app=postgresql`
   - Fix: Investigate database pod logs, check resource limits

2. **Connection Pool Exhausted**: Too many concurrent connections
   - Check: `ac_db_queries_total` - look for slow queries holding connections
   - Fix: Identify slow queries, increase pool size (if safe), or scale pods

3. **Network Partition**: Network policy blocking traffic
   - Check: `kubectl get networkpolicies -n dark-tower`
   - Fix: Verify network policies allow ac-service → postgresql traffic

4. **Database Credentials Rotated**: Secret updated but pods not restarted
   - Check: Compare secret with running pod env vars
   - Fix: Restart deployment to pick up new secrets

5. **Database Disk Full**: PostgreSQL out of disk space
   - Check: Database team escalation required
   - Fix: Database team handles disk expansion

**Remediation**:

```bash
# Option 1: Restart pods to clear stuck connections (quick fix)
kubectl rollout restart deployment/ac-service -n dark-tower

# Option 2: Scale down and up to force new connections
kubectl scale deployment/ac-service -n dark-tower --replicas=0
sleep 5
kubectl scale deployment/ac-service -n dark-tower --replicas=3

# Option 3: Emergency: Bypass readiness probe to restore partial service
# (ONLY if DB is actually healthy and it's a false positive)
kubectl patch deployment/ac-service -n dark-tower -p '{"spec":{"template":{"spec":{"containers":[{"name":"ac-service","readinessProbe":null}]}}}}'
# WARNING: This will route traffic to unhealthy pods. Revert ASAP.

# After remediation, verify recovery
kubectl get pods -n dark-tower -l app=ac-service
curl http://ac-service:8080/ready
curl http://ac-service:8080/metrics | grep ac_token_issuance_total
```

**Escalation**: If database is unresponsive for >5 minutes, page Database Team immediately.

---

### Scenario 2: Key Rotation Stuck

**Symptoms**:
- Alert: `AC-003: Key rotation failed`
- Old keys not expiring (check `signing_keys` table)
- JWKS endpoint returning outdated keys
- Metrics: `ac_key_rotation_total{status="error"}` incrementing
- Logs: Key rotation errors, encryption failures

**Diagnosis**:

```bash
# 1. Check current signing keys in database
kubectl exec -it -n dark-tower deployment/ac-service -- sh -c 'psql $DATABASE_URL -c "SELECT key_id, is_active, created_at, activated_at, expires_at FROM signing_keys ORDER BY created_at DESC LIMIT 5;"'

# 2. Check JWKS endpoint
curl http://ac-service:8080/.well-known/jwks.json | jq '.keys[] | {kid, use, alg}'

# 3. Check key rotation metrics
curl http://ac-service:9090/metrics | grep ac_key_rotation

# 4. Check key rotation logs
kubectl logs -n dark-tower -l app=ac-service --tail=500 | grep -i "key_rotation\|signing_key"

# 5. Check last successful rotation timestamp
curl http://ac-service:9090/metrics | grep ac_key_rotation_last_success_timestamp
```

**Common Root Causes**:

1. **Master Key Unavailable**: Encryption key secret missing or corrupted
   - Check: `kubectl get secret -n dark-tower ac-master-key`
   - Fix: Restore secret from backup, restart pods

2. **Database Constraint Violation**: Race condition during rotation
   - Check: Database logs for constraint errors
   - Fix: Clean up partial rotation state, retry

3. **Clock Skew**: System clocks out of sync causing expiration logic errors
   - Check: `date` on pod vs actual time
   - Fix: Verify NTP configuration, restart affected pods

4. **Insufficient Permissions**: Database user lacks INSERT/UPDATE permissions
   - Check: Database grants for ac-service user
   - Fix: Database team grants permissions

**Remediation**:

```bash
# Option 1: Trigger manual key rotation (requires admin token)
# First, get an admin token
export ADMIN_TOKEN=$(curl -X POST http://ac-service:8080/api/v1/auth/service/token \
  -H "Content-Type: application/json" \
  -d '{"client_id":"admin-client","client_secret":"<admin-secret>","grant_type":"client_credentials","scope":"admin:keys"}' \
  | jq -r '.access_token')

# Trigger rotation
curl -X POST http://ac-service:8080/internal/rotate-keys \
  -H "Authorization: Bearer $ADMIN_TOKEN"

# Option 2: Force activate a specific key (emergency only)
kubectl exec -it -n dark-tower deployment/ac-service -- sh -c 'psql $DATABASE_URL -c "UPDATE signing_keys SET is_active = true, activated_at = NOW() WHERE key_id = '\''<key_id>'\'';"'

# Option 3: Clean up stuck rotation and retry
# WARNING: Only do this if you understand the state
kubectl exec -it -n dark-tower deployment/ac-service -- sh -c 'psql $DATABASE_URL -c "DELETE FROM signing_keys WHERE is_active = false AND created_at > NOW() - interval '\''1 hour'\'';"'
# Then trigger rotation again

# Verify recovery
curl http://ac-service:8080/.well-known/jwks.json | jq '.keys | length'
curl http://ac-service:9090/metrics | grep ac_active_signing_keys
```

**Escalation**: If master key is lost, escalate to Security Team immediately. Key recovery requires secure backup access.

---

### Scenario 3: High Latency / Slow Responses

**Symptoms**:
- Alert: `AC-001: Token issuance latency SLO breach`
- p99 latency > 350ms (target), possibly > 1s
- Timeouts (30s timeout per ADR-0012)
- Metrics: `ac_token_issuance_duration_seconds` histogram skewed right

**Diagnosis**:

```bash
# 1. Check current latency metrics
curl http://ac-service:9090/metrics | grep ac_token_issuance_duration_seconds

# 2. Check database query performance
curl http://ac-service:9090/metrics | grep ac_db_query_duration_seconds

# 3. Check bcrypt performance (should be ~250ms)
curl http://ac-service:9090/metrics | grep ac_bcrypt_duration_seconds

# 4. Check pod resource utilization
kubectl top pods -n dark-tower -l app=ac-service

# 5. Check for slow queries in logs
kubectl logs -n dark-tower -l app=ac-service --tail=1000 | grep -E "duration_ms|slow"

# 6. Check database performance (escalate to DB team if needed)
# Database team has tools for pg_stat_statements analysis
```

**Common Root Causes**:

1. **Database Query Slow**: Unoptimized queries, missing indexes
   - Check: `ac_db_query_duration_seconds{operation="select"}` p99
   - Fix: Database team investigates slow queries, adds indexes

2. **Bcrypt Cost Too High**: Cost factor too high for pod CPU
   - Check: `ac_bcrypt_duration_seconds` - should be 150-250ms
   - Fix: Currently cost=12 (hardcoded). If consistently >300ms, consider reducing to cost=11 (security team approval required)

3. **Resource Contention**: Insufficient CPU/memory
   - Check: `kubectl top pods` - CPU/memory at limits
   - Fix: Increase resource requests/limits, scale horizontally

4. **High Request Volume**: Unexpected traffic spike
   - Check: `ac_token_issuance_total` rate
   - Fix: Scale horizontally, verify not a DDoS attack

5. **Network Latency**: Pod-to-database network slow
   - Check: Ping database from pod, check network metrics
   - Fix: Infrastructure team investigates CNI issues

**Remediation**:

```bash
# Option 1: Scale horizontally to distribute load
kubectl scale deployment/ac-service -n dark-tower --replicas=6

# Option 2: Increase resource limits (requires manifest update)
kubectl patch deployment/ac-service -n dark-tower -p '{"spec":{"template":{"spec":{"containers":[{"name":"ac-service","resources":{"limits":{"cpu":"2000m","memory":"1Gi"},"requests":{"cpu":"1000m","memory":"512Mi"}}}]}}}}'

# Option 3: Restart pods to clear any memory leaks (unlikely with Rust, but possible)
kubectl rollout restart deployment/ac-service -n dark-tower

# Option 4: Emergency load shedding (future feature)
# When implemented, increase rate limits to drop low-priority traffic

# Verify recovery
curl http://ac-service:9090/metrics | grep -A 20 ac_token_issuance_duration_seconds_bucket
# Check p99 is back under 350ms
```

**Escalation**:
- If database queries are slow (>100ms p99), escalate to Database Team
- If CPU/memory issues persist after scaling, escalate to Infrastructure Team
- If appears to be attack, escalate to Security Team

---

### Scenario 4: Rate Limiting Issues

**Symptoms**:
- Legitimate clients receiving 429 Too Many Requests
- Customer complaints about authentication failures
- Metrics: `ac_rate_limit_decisions_total{action="rejected"}` high
- Logs: Rate limit rejections for known good clients

**Diagnosis**:

```bash
# 1. Check rate limit metrics
curl http://ac-service:9090/metrics | grep ac_rate_limit_decisions_total

# 2. Check which clients are being rate limited
kubectl logs -n dark-tower -l app=ac-service --tail=500 | grep "rate_limit.*rejected"

# 3. Check error metrics by status code
curl http://ac-service:9090/metrics | grep 'ac_errors_total.*status_code="429"'

# 4. Check recent traffic patterns
curl http://ac-service:9090/metrics | grep ac_token_issuance_total
```

**Common Root Causes**:

1. **Legitimate Traffic Spike**: Expected high load (e.g., product launch)
   - Check: Coordinate with product team on expected traffic
   - Fix: Temporarily increase rate limits, scale service

2. **Client Retry Storm**: Client library retrying aggressively
   - Check: Logs showing same client_id repeatedly
   - Fix: Contact client team to fix retry backoff logic

3. **Rate Limit Too Restrictive**: Limits set too low for normal use
   - Check: Historical traffic patterns vs current limits
   - Fix: Adjust rate limits in configuration

4. **DDoS Attack**: Malicious traffic overwhelming service
   - Check: Unusual traffic patterns, unknown client IDs
   - Fix: Escalate to Security Team, implement IP-based blocking

**Remediation**:

```bash
# Option 1: Temporarily increase rate limits (requires config change + restart)
# Edit ConfigMap:
kubectl edit configmap ac-service-config -n dark-tower
# Update rate limit values, then:
kubectl rollout restart deployment/ac-service -n dark-tower

# Option 2: Whitelist specific client (emergency - requires code change or future feature)
# Currently not supported - escalate to engineering team

# Option 3: Scale service to handle more concurrent requests
kubectl scale deployment/ac-service -n dark-tower --replicas=8

# Option 4: If under attack, implement emergency IP blocking
# Requires infrastructure team to update ingress rules
# Escalate to Security + Infrastructure teams

# Verify recovery
kubectl logs -n dark-tower -l app=ac-service --tail=100 | grep "429"
curl http://ac-service:9090/metrics | grep 'ac_errors_total.*status_code="429"'
```

**Escalation**:
- If attack suspected, page Security Team immediately
- If widespread client impact, notify Engineering Manager for customer communication

---

### Scenario 5: Token Validation Failures

**Symptoms**:
- Valid tokens being rejected by downstream services
- Logs: Signature verification failures, expired tokens being issued
- Metrics: `ac_token_validations_total{status="error"}` high (in downstream services)
- JWKS endpoint issues

**Diagnosis**:

```bash
# 1. Check JWKS endpoint is responding
curl http://ac-service:8080/.well-known/jwks.json
# Should return JSON with "keys" array

# 2. Verify keys in JWKS match database
kubectl exec -it -n dark-tower deployment/ac-service -- sh -c 'psql $DATABASE_URL -c "SELECT key_id, is_active FROM signing_keys WHERE is_active = true;"'

# 3. Check for clock skew issues
kubectl exec -it -n dark-tower deployment/ac-service -- date
# Compare with actual time - should be within 1 second

# 4. Test token issuance and immediate validation
# Issue a token
TOKEN=$(curl -X POST http://ac-service:8080/api/v1/auth/service/token \
  -H "Content-Type: application/json" \
  -d '{"client_id":"test-client","client_secret":"test-secret","grant_type":"client_credentials"}' \
  | jq -r '.access_token')

# Decode token (use jwt.io or jwt-cli)
echo $TOKEN | jwt decode -

# 5. Check JWKS cache metrics (if implemented)
curl http://ac-service:9090/metrics | grep ac_jwks_requests_total
```

**Common Root Causes**:

1. **Clock Skew**: System clocks out of sync
   - Check: `date` on ac-service pods vs actual time
   - Fix: Verify NTP, restart pods

2. **Key Rotation Mid-Flight**: Token signed with old key, JWKS updated
   - Check: Token `kid` header vs JWKS key IDs
   - Fix: This is normal - clients should retry. If persistent, key rotation interval may be too short

3. **JWKS Caching Issues**: Clients caching old JWKS too long
   - Check: JWKS cache headers (future feature)
   - Fix: Document recommended client cache TTL (5 minutes)

4. **Corrupted Key**: Signing key corrupted in database
   - Check: Try signing operation, check for crypto errors
   - Fix: Force key rotation to generate new key

5. **Wrong Algorithm**: Token signed with different algorithm than advertised
   - Check: Token header `alg` field vs JWKS `alg` field
   - Fix: This indicates a code bug - escalate to engineering

**Remediation**:

```bash
# Option 1: Force key rotation to regenerate keys
# (See Scenario 2 for key rotation commands)

# Option 2: Verify and fix clock skew
kubectl exec -it -n dark-tower deployment/ac-service -- sh -c 'ntpdate -q pool.ntp.org'
# If skew detected, restart pods to sync time

# Option 3: Clear any stale JWKS caches (client-side)
# Contact affected service teams to restart their services

# Option 4: Emergency - manually verify token is valid
# Use https://jwt.io or jwt-cli to decode and verify signature
# This helps determine if issue is with AC or downstream service

# Verify recovery
# Issue new token and have downstream service validate it
curl -X POST http://downstream-service:8080/validate-token \
  -H "Authorization: Bearer $TOKEN"
```

**Escalation**:
- If algorithm mismatch detected, escalate to Engineering Lead immediately (potential security issue)
- If widespread validation failures, escalate to Service Owner

---

### Scenario 6: Complete Service Outage (All Pods Down)

**Symptoms**:
- All ac-service pods in CrashLoopBackOff or Pending state
- 503 Service Unavailable on all endpoints
- No healthy pods in `kubectl get pods -l app=ac-service`

**Diagnosis**:

```bash
# 1. Check pod status
kubectl get pods -n dark-tower -l app=ac-service

# 2. Check pod events
kubectl describe pods -n dark-tower -l app=ac-service

# 3. Check recent logs before crash
kubectl logs -n dark-tower -l app=ac-service --previous --tail=100

# 4. Check deployment status
kubectl describe deployment ac-service -n dark-tower

# 5. Check resource quotas
kubectl describe resourcequota -n dark-tower

# 6. Check node status
kubectl get nodes
kubectl describe node <node-name>
```

**Common Root Causes**:

1. **Bad Deployment**: Recent deployment introduced panic/crash
   - Check: Deployment history, recent changes
   - Fix: Rollback to previous version

2. **Out of Memory**: Pods OOMKilled due to memory limits
   - Check: Pod events show "OOMKilled"
   - Fix: Increase memory limits, investigate memory leak

3. **Missing Secret**: Required secret deleted or corrupted
   - Check: `kubectl get secret -n dark-tower ac-master-key`
   - Fix: Restore secret from backup

4. **ImagePullBackOff**: Cannot pull container image
   - Check: Pod events show image pull errors
   - Fix: Verify image registry credentials, image exists

5. **Node Failure**: All nodes where pods were scheduled failed
   - Check: `kubectl get nodes` - nodes NotReady
   - Fix: Infrastructure team handles node recovery

**Remediation**:

```bash
# Option 1: Rollback deployment to last known good version
kubectl rollout undo deployment/ac-service -n dark-tower
kubectl rollout status deployment/ac-service -n dark-tower

# Option 2: Force reschedule pods
kubectl delete pods -n dark-tower -l app=ac-service
# Deployment will recreate them

# Option 3: Manually scale up from zero (if scaled down accidentally)
kubectl scale deployment/ac-service -n dark-tower --replicas=3

# Option 4: Check and restore missing secrets
kubectl get secret -n dark-tower ac-master-key
# If missing, restore from secure backup (Security team escalation)

# Option 5: Bypass resource limits (emergency only)
kubectl patch deployment/ac-service -n dark-tower -p '{"spec":{"template":{"spec":{"containers":[{"name":"ac-service","resources":{"limits":{"memory":"2Gi"}}}]}}}}'

# Verify recovery
kubectl get pods -n dark-tower -l app=ac-service
kubectl logs -n dark-tower -l app=ac-service --tail=50
curl http://ac-service:8080/ready
```

**Escalation**:
- If rollback fails, escalate to Engineering Lead immediately
- If node issues, escalate to Infrastructure Team
- If secret compromise suspected, escalate to Security Team

---

### Scenario 7: Audit Log Failures

**Symptoms**:
- Alert: `ac_audit_log_failures_total > 0` (compliance-critical)
- Logs: Audit log write failures
- Security team notification

**Diagnosis**:

```bash
# 1. Check audit log failure metrics
curl http://ac-service:9090/metrics | grep ac_audit_log_failures_total

# 2. Check logs for audit failures
kubectl logs -n dark-tower -l app=ac-service --tail=500 | grep -i "audit.*fail"

# 3. Check audit_logs table connectivity
kubectl exec -it -n dark-tower deployment/ac-service -- sh -c 'psql $DATABASE_URL -c "SELECT COUNT(*) FROM audit_logs WHERE created_at > NOW() - interval '\''1 hour'\'';"'

# 4. Check database disk space (via DB team)
```

**Common Root Causes**:

1. **Database Write Failure**: Database unavailable for writes
   - Check: Database status, replication lag
   - Fix: Database team investigation

2. **Table Partition Missing**: Audit log partition not created for current month
   - Check: `\d+ audit_logs` in psql
   - Fix: Create partition for current period

3. **Disk Full**: Database disk full, cannot write
   - Check: Database team monitors
   - Fix: Database team expands disk or cleans up old partitions

**Remediation**:

```bash
# Option 1: Create missing partition (if applicable)
kubectl exec -it -n dark-tower deployment/ac-service -- sh -c 'psql $DATABASE_URL -c "CREATE TABLE IF NOT EXISTS audit_logs_y2025m12 PARTITION OF audit_logs FOR VALUES FROM ('\''2025-12-01'\'') TO ('\''2026-01-01'\'');"'

# Option 2: Escalate to Database Team immediately
# Audit log failures are compliance-critical - cannot be ignored

# Verify recovery
# Check that new audit entries are being written
kubectl exec -it -n dark-tower deployment/ac-service -- sh -c 'psql $DATABASE_URL -c "SELECT COUNT(*) FROM audit_logs WHERE created_at > NOW() - interval '\''5 minutes'\'';"'

# Verify metric reset
curl http://ac-service:9090/metrics | grep ac_audit_log_failures_total
```

**Escalation**:
- **IMMEDIATE**: Page Database Team AND Security Team
- Audit log failures are compliance violations - highest priority after service outage

---

## Diagnostic Commands

### Quick Health Check

```bash
# Check service health
curl http://ac-service:8080/health      # Liveness (should always return "OK")
curl http://ac-service:8080/ready       # Readiness (checks DB + signing keys)

# Check pod status
kubectl get pods -n dark-tower -l app=ac-service

# Check recent errors in logs
kubectl logs -n dark-tower -l app=ac-service --tail=100 | grep -i error
```

### Metrics Analysis

```bash
# Get all metrics
curl http://ac-service:9090/metrics

# Token issuance metrics
curl http://ac-service:9090/metrics | grep ac_token_issuance

# Database metrics
curl http://ac-service:9090/metrics | grep ac_db_

# Error metrics
curl http://ac-service:9090/metrics | grep ac_errors_total

# Key rotation metrics
curl http://ac-service:9090/metrics | grep ac_key_rotation

# Rate limiting metrics
curl http://ac-service:9090/metrics | grep ac_rate_limit
```

### Database Queries

```bash
# Connect to database from pod
kubectl exec -it -n dark-tower deployment/ac-service -- sh -c 'psql $DATABASE_URL'

# Check active signing keys
kubectl exec -it -n dark-tower deployment/ac-service -- sh -c 'psql $DATABASE_URL -c "SELECT key_id, is_active, created_at, activated_at, expires_at FROM signing_keys ORDER BY created_at DESC LIMIT 5;"'

# Check recent service credentials (for authentication debugging)
kubectl exec -it -n dark-tower deployment/ac-service -- sh -c 'psql $DATABASE_URL -c "SELECT client_id, created_at, is_active FROM service_credentials ORDER BY created_at DESC LIMIT 10;"'

# Check audit log recent entries
kubectl exec -it -n dark-tower deployment/ac-service -- sh -c 'psql $DATABASE_URL -c "SELECT event_type, result, created_at FROM audit_logs ORDER BY created_at DESC LIMIT 20;"'

# Check for slow queries (requires pg_stat_statements extension)
# Escalate to Database Team for this analysis
```

### Log Analysis

```bash
# Stream logs in real-time
kubectl logs -n dark-tower -l app=ac-service -f

# Get logs from all pods
kubectl logs -n dark-tower -l app=ac-service --all-containers --tail=200

# Get logs from previous pod instance (after crash)
kubectl logs -n dark-tower <pod-name> --previous

# Search for specific errors
kubectl logs -n dark-tower -l app=ac-service --tail=1000 | grep -E "error|panic|fatal"

# Search for authentication failures
kubectl logs -n dark-tower -l app=ac-service --tail=1000 | grep -i "auth.*fail"

# Search for database errors
kubectl logs -n dark-tower -l app=ac-service --tail=1000 | grep -i "database\|sqlx"
```

### Resource Utilization

```bash
# Check CPU and memory usage
kubectl top pods -n dark-tower -l app=ac-service

# Check node resources
kubectl top nodes

# Check resource limits
kubectl describe deployment ac-service -n dark-tower | grep -A 5 "Limits:"

# Check events for resource issues
kubectl get events -n dark-tower --field-selector involvedObject.name=ac-service --sort-by='.lastTimestamp'
```

### Network Debugging

```bash
# Test service connectivity
kubectl run -it --rm debug --image=nicolaka/netshoot --restart=Never -- /bin/bash
# From debug pod:
curl http://ac-service.dark-tower.svc.cluster.local:8080/health
nslookup ac-service.dark-tower.svc.cluster.local
ping ac-service.dark-tower.svc.cluster.local

# Check service endpoints
kubectl get endpoints -n dark-tower ac-service

# Check network policies
kubectl get networkpolicies -n dark-tower
```

---

## Recovery Procedures

### Service Restart Procedure

**When to use**: Minor issues, stuck connections, memory leaks (rare in Rust)

```bash
# 1. Verify current state
kubectl get pods -n dark-tower -l app=ac-service
curl http://ac-service:8080/metrics | grep ac_token_issuance_total

# 2. Perform rolling restart (zero downtime)
kubectl rollout restart deployment/ac-service -n dark-tower

# 3. Monitor rollout
kubectl rollout status deployment/ac-service -n dark-tower

# 4. Verify recovery
kubectl get pods -n dark-tower -l app=ac-service
curl http://ac-service:8080/ready
curl http://ac-service:8080/metrics | grep ac_errors_total

# 5. Check logs for startup errors
kubectl logs -n dark-tower -l app=ac-service --tail=50
```

**Rollback on failure**:
```bash
kubectl rollout undo deployment/ac-service -n dark-tower
```

---

### Database Failover Procedure

**When to use**: Primary database failure, planned maintenance

**WARNING**: This procedure requires coordination with Database Team. Do NOT execute without Database Team approval.

```bash
# 1. Verify database status
# Escalate to Database Team - they handle failover

# 2. After Database Team completes failover, verify new connection
kubectl exec -it -n dark-tower deployment/ac-service -- sh -c 'psql $DATABASE_URL -c "SELECT 1"'

# 3. If pods have stale connections, restart them
kubectl rollout restart deployment/ac-service -n dark-tower

# 4. Verify recovery
curl http://ac-service:8080/ready
# Should show database: "healthy"

# 5. Monitor metrics for 15 minutes
watch -n 10 'curl -s http://ac-service:9090/metrics | grep -E "ac_token_issuance|ac_db_query|ac_errors"'
```

---

### Emergency Key Rotation

**When to use**: Key compromise suspected, security incident, corrupted key

**WARNING**: This is a security-critical operation. Coordinate with Security Team.

```bash
# 1. Assess situation
# - Is current key compromised? (Security Team determines)
# - Are tokens currently being issued? (Check metrics)
# - Can we tolerate brief downtime? (Check with on-call lead)

# 2. Backup current state
kubectl exec -it -n dark-tower deployment/ac-service -- sh -c 'psql $DATABASE_URL -c "COPY signing_keys TO STDOUT CSV HEADER;"' > /tmp/signing_keys_backup.csv

# 3. Generate admin token for rotation
export ADMIN_TOKEN=$(curl -X POST http://ac-service:8080/api/v1/auth/service/token \
  -H "Content-Type: application/json" \
  -d '{"client_id":"admin-client","client_secret":"'"$ADMIN_SECRET"'","grant_type":"client_credentials","scope":"admin:keys"}' \
  | jq -r '.access_token')

# 4. Trigger emergency rotation
curl -X POST http://ac-service:8080/internal/rotate-keys \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"force":true}'

# 5. Verify new key is active
kubectl exec -it -n dark-tower deployment/ac-service -- sh -c 'psql $DATABASE_URL -c "SELECT key_id, is_active, created_at FROM signing_keys ORDER BY created_at DESC LIMIT 3;"'

# 6. Verify JWKS updated
curl http://ac-service:8080/.well-known/jwks.json | jq '.keys[] | {kid, use, alg}'

# 7. If compromise confirmed, invalidate old tokens
# (Requires token blacklist feature - future implementation)
# For now, wait for old tokens to expire (default 1 hour)

# 8. Notify downstream services
# Services should refresh JWKS within 5 minutes (recommended cache TTL)

# 9. Monitor for validation failures
# Check downstream service logs for signature verification errors
# This is expected for ~5 minutes during JWKS cache refresh

# 10. Document incident
# Create postmortem using template below
```

**Rollback**: If new key is corrupt, manually reactivate previous key (requires database access).

---

### Load Shedding / Traffic Control

**When to use**: Overwhelming traffic, DDoS attack, protecting service from cascading failure

```bash
# Option 1: Scale horizontally (handles legitimate traffic increases)
kubectl scale deployment/ac-service -n dark-tower --replicas=10

# Option 2: Increase resource limits (if CPU/memory constrained)
kubectl patch deployment/ac-service -n dark-tower -p '{"spec":{"template":{"spec":{"containers":[{"name":"ac-service","resources":{"limits":{"cpu":"2000m","memory":"1Gi"}}}]}}}}'

# Option 3: Rate limiting (future feature - config-based)
# Edit ConfigMap to reduce rate limits
kubectl edit configmap ac-service-config -n dark-tower
# Update: token_issuance_rate_limit: 10 (from 100)
kubectl rollout restart deployment/ac-service -n dark-tower

# Option 4: Emergency IP blocking (Infrastructure Team)
# If under attack, provide attacker IPs to Infrastructure Team
# They update ingress rules or network policies

# Option 5: Circuit breaker (future feature)
# Automatically reject requests when error rate > threshold
# Not yet implemented - manual intervention required

# Verify traffic levels
curl http://ac-service:9090/metrics | grep ac_token_issuance_total
```

---

## Postmortem Template

Use this template for all P1 and P2 incidents:

```markdown
# Postmortem: [Incident Title]

**Date**: YYYY-MM-DD
**Severity**: P1/P2/P3
**Duration**: [Start time] - [End time] (Total: X hours Y minutes)
**Status**: Resolved / Mitigated / Investigating
**Author**: [On-call engineer name]
**Reviewers**: [Tech Lead, Engineering Manager]

---

## Executive Summary

[1-2 sentences describing what happened and impact]

---

## Impact

**User Impact**:
- Number of affected requests: [metric]
- Error rate: X% (normal: Y%)
- Affected customers: [list if known, or "all customers"]
- Duration of impact: [X minutes/hours]

**Business Impact**:
- Revenue impact: $[estimate] or N/A
- Reputation impact: [description]
- SLA breach: Yes/No - [details]

**Metrics**:
- Peak error rate: [from ac_errors_total]
- Peak latency: [from ac_token_issuance_duration_seconds]
- Total failed requests: [from ac_token_issuance_total{status="error"}]

---

## Timeline

All times in UTC. Link to relevant Slack threads, PagerDuty incidents, and dashboards.

| Time (UTC) | Event |
|------------|-------|
| HH:MM | [First alert fired / User report received] |
| HH:MM | [On-call engineer acknowledged] |
| HH:MM | [Investigation began - diagnostic commands run] |
| HH:MM | [Root cause identified: ...] |
| HH:MM | [Remediation started: ...] |
| HH:MM | [Service recovered to baseline] |
| HH:MM | [Incident declared resolved] |
| HH:MM | [Postmortem review completed] |

---

## Root Cause

[Detailed explanation of what caused the incident]

**Technical Details**:
- Component: [e.g., ac-service, database, Kubernetes]
- Failure mode: [e.g., connection pool exhaustion, OOMKill, key rotation stuck]
- Why it happened: [e.g., traffic spike exceeded capacity, bug in X, config change Y]

**Contributing Factors**:
- [Factor 1: e.g., Insufficient monitoring of X]
- [Factor 2: e.g., Lack of rate limiting]
- [Factor 3: e.g., Missing alert for Y]

---

## Detection

**How was the incident detected?**
- [ ] Automated alert (name: [alert name])
- [ ] Customer report
- [ ] Manual monitoring
- [ ] Other: [describe]

**Time to detect**: [X minutes from start of issue to detection]

**What went well**:
- [e.g., Alert fired within SLA (15 min for P1)]
- [e.g., Clear error messages in logs]

**What could be improved**:
- [e.g., Alert threshold too high, missed early warning signs]
- [e.g., No alert for specific condition]

---

## Response

**What went well**:
- [e.g., Diagnostic commands in runbook were accurate]
- [e.g., Rollback completed quickly]
- [e.g., Clear communication in #incidents channel]

**What could be improved**:
- [e.g., Escalation to Database Team was delayed]
- [e.g., Runbook missing steps for X scenario]
- [e.g., Unclear who had authority to execute Y]

**Lessons Learned**:
- [Lesson 1]
- [Lesson 2]
- [Lesson 3]

---

## Action Items

All action items must have an owner and due date. Track in issue tracker.

| Action | Owner | Due Date | Priority | Status |
|--------|-------|----------|----------|--------|
| [Fix root cause: ...] | [Name] | YYYY-MM-DD | P0 | Open |
| [Update runbook: ...] | [Name] | YYYY-MM-DD | P1 | Open |
| [Add alert: ...] | [Name] | YYYY-MM-DD | P1 | Open |
| [Improve monitoring: ...] | [Name] | YYYY-MM-DD | P2 | Open |
| [Update documentation: ...] | [Name] | YYYY-MM-DD | P2 | Open |

**Prevention**:
- [How will we prevent this class of failure in the future?]

**Detection**:
- [How will we detect this faster next time?]

**Mitigation**:
- [How will we reduce impact if it happens again?]

---

## Supporting Information

**Dashboards**:
- [Link to Grafana dashboard during incident timeframe]

**Logs**:
- [Link to log aggregator with relevant query]

**Metrics**:
- [Prometheus query showing incident impact]

**Related Incidents**:
- [Links to similar past incidents]

**Communication**:
- [Slack #incidents thread]
- [PagerDuty incident link]
- [Customer communication (if applicable)]

---

## Appendix

[Any additional context, graphs, logs, or technical details]
```

---

## Maintenance and Updates

**Runbook Ownership**:
- **Primary**: Operations Specialist
- **Reviewers**: Service Owner (Auth Controller), On-call rotation members

**Review Schedule**:
- After every P1/P2 incident (update within 24 hours)
- Monthly review during on-call handoff
- Quarterly comprehensive review

**Change Process**:
1. Create pull request with runbook updates
2. Review by on-call rotation members
3. Test new diagnostic commands in staging
4. Merge and notify team in #dark-tower-ops channel

**Version History**:
- 2025-12-10: Initial version (Phase 4B Operational Readiness)
- [Future updates tracked here]

---

## Additional Resources

- **ADR-0011**: Observability Framework
- **ADR-0012**: Operational Readiness Requirements
- **Metrics Catalog**: `/docs/observability/metrics/ac-service.md`
- **SLO Definitions**: `/docs/observability/slos.md`
- **AC Service Architecture**: `/docs/ARCHITECTURE.md` (AC section)
- **Database Schema**: `/docs/DATABASE_SCHEMA.md`
- **On-call Rotation**: PagerDuty schedule "Dark Tower Auth Team"
- **Slack Channels**:
  - `#incidents` - Active incident coordination
  - `#dark-tower-ops` - Operational discussions
  - `#ac-service` - Service-specific channel
  - `#database-oncall` - Database team escalation
  - `#infra-oncall` - Infrastructure team escalation
  - `#security-incidents` - Security team (CRITICAL ONLY)

---

**Remember**: When in doubt, escalate. It's better to involve specialists early than to struggle alone during an incident.
