# Security Review: MC Observability Configuration

**Reviewer**: Security Specialist
**Date**: 2026-02-09
**Scope**: Infrastructure configuration (Grafana dashboard, Prometheus alerts, runbooks)

---

## Files Reviewed

| File | Lines | Type |
|------|-------|------|
| `infra/grafana/dashboards/mc-overview.json` | 1382 | Grafana Dashboard |
| `infra/docker/prometheus/rules/mc-alerts.yaml` | 239 | Prometheus Alert Rules |
| `docs/runbooks/mc-deployment.md` | 806 | Deployment Runbook |
| `docs/runbooks/mc-incident-response.md` | 1042 | Incident Response Runbook |

---

## Security Analysis

### 1. Dashboard Queries - Information Disclosure Review

**Finding**: No PII or sensitive data in metric labels or queries.

All PromQL queries use aggregate metrics without exposing sensitive identifiers:
- `mc_meetings_active` - Count only, no meeting_id exposure
- `mc_connections_active` - Count only, no connection/user ID exposure
- `mc_actor_mailbox_depth` - Aggregated by `actor_type` (system label, not user data)
- `mc_message_processing_duration_seconds` - Histogram, no message content
- `mc_gc_heartbeat_total` - Aggregated by `status` (success/error)
- Container metrics use `pod=~"meeting-controller-.*"` regex, no IP addresses

**Labels checked**:
- `actor_type`: System-level label (e.g., "MeetingActor", "ConnectionActor") - SAFE
- `status`: Operation status ("success", "error") - SAFE
- `job`: Service name ("meeting-controller") - SAFE
- `pod`: Pod name pattern (no user data) - SAFE

**Verdict**: PASS - No PII or sensitive data exposed in dashboard queries.

---

### 2. Alert Rules - Information Disclosure Review

**Finding**: Alert annotations are properly sanitized.

Checked all 14 alert rules for sensitive data in annotations:

| Alert | Summary/Description Content | Status |
|-------|---------------------------|--------|
| MCDown | Service status only | SAFE |
| MCActorPanic | `actor_type` label (system) | SAFE |
| MCHighMailboxDepthCritical | `actor_type` and numeric value | SAFE |
| MCHighLatency | Numeric latency value | SAFE |
| MCHighMessageDropRate | Percentage value | SAFE |
| MCGCHeartbeatFailure | Error rate percentage | SAFE |
| MCHighMailboxDepthWarning | `actor_type` and numeric value | SAFE |
| MCHighMemory | `pod` name and percentage | SAFE |
| MCHighCPU | `pod` name and percentage | SAFE |
| MCLowConnectionCount | Count values | SAFE |
| MCMeetingStale | Count values | SAFE |
| MCGCHeartbeatWarning | Error rate percentage | SAFE |
| MCPodRestartingFrequently | `pod` name and restart count | SAFE |
| MCCapacityWarning | Active meeting count | SAFE |

**Runbook URLs**: All point to public GitHub repository path, no internal infrastructure details exposed.

**Verdict**: PASS - No PII in alert annotations.

---

### 3. Runbook Security - Command Safety Review

**Finding**: Commands are safe and follow security best practices.

**Deployment Runbook** (`mc-deployment.md`):
- `kubectl` commands use namespace restrictions (`-n dark-tower`)
- Port-forward commands bind to localhost only (8080:8080)
- No hardcoded secrets in examples
- TLS secrets referenced by name, not content
- Database URL referenced via environment variable (`$DATABASE_URL`), not hardcoded
- SQL commands use proper escaping and are administrative queries only

**Incident Response Runbook** (`mc-incident-response.md`):
- All diagnostic commands are read-only (metrics, logs, status)
- Database queries are SELECT-only in diagnostics
- UPDATE statements for `meeting_controllers` table are administrative (status changes)
- No `DELETE FROM` or destructive database commands on user data
- No `kubectl delete namespace` or cluster-wide destructive commands

**Checked for dangerous patterns**:
- `kubectl delete --all`: Not present
- `rm -rf`: Not present
- Hardcoded credentials: Not present
- Hardcoded API keys: Not present
- Hardcoded private keys: Not present
- `--force` flags without safeguards: Not present

**Verdict**: PASS - Commands are safe and appropriate for runbooks.

---

### 4. Credential Handling

**Finding**: Credentials are properly externalized.

- TLS certificates referenced via Kubernetes Secret (`mc-service-secrets`)
- Database connection uses environment variable (`$DATABASE_URL`)
- No plaintext secrets in any file
- ConfigMap/Secret separation follows best practices

**Verdict**: PASS - No credential exposure.

---

## Summary

| Category | Status | Notes |
|----------|--------|-------|
| Dashboard PII | PASS | No sensitive identifiers in queries |
| Alert PII | PASS | No sensitive data in annotations |
| Runbook Commands | PASS | All commands safe, read-only diagnostics |
| Credential Handling | PASS | Properly externalized to Secrets |

---

## Verdict

```
verdict: APPROVED
finding_count:
  blocker: 0
  critical: 0
  major: 0
  minor: 0
  tech_debt: 0
checkpoint_exists: true
summary: Infrastructure configuration follows privacy-by-default. No PII in metrics/alerts, no credential exposure in runbooks, all commands are safe.
```

---

## Recommendations (Non-blocking)

These are best-practice suggestions, not required changes:

1. **Consider audit logging**: When admin API is implemented for meeting cleanup, ensure audit logging is in place for database modifications.

2. **NetworkPolicy documentation**: The runbook references NetworkPolicy but doesn't include the actual policy YAML. Consider adding reference to the policy file location.

3. **Future expansion**: As meeting-specific debugging is needed, ensure any future metrics with `meeting_id` labels use hashed/pseudonymized identifiers rather than raw UUIDs.

---

**Review completed**: 2026-02-09
**Reviewer**: Security Specialist
