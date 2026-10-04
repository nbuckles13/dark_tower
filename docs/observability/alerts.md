# Prometheus Alerts Catalog

This document catalogs all Prometheus alerting rules for Dark Tower services.

## Alert Organization

Alerts are organized by:
- **Severity**: Critical (page immediately), Warning (notify), Info (log only)
- **Service**: Per-service alert groups (AC, GC, MC, MH)
- **Component**: Infrastructure, application logic, database, etc.

All alert rules are stored in `infra/docker/prometheus/rules/` and loaded by both Prometheus
deployments — the docker-compose one (`infra/docker/prometheus/prometheus.yml`) and the in-cluster
one (`infra/kubernetes/observability/prometheus.yml`, mounted via `configMapGenerator`;
`prometheus-config.yaml` alongside it holds only the RBAC, Deployment and Service) — via a
`rule_files` glob of `rules/[a-z]*-alerts.yaml`, byte-identical in both. The character class excludes `_template-service-alerts.yaml`, whose
placeholder PromQL would fail rule-file parsing at startup; the operative property is **"does not
begin with a lowercase letter"**, not "begins with `_`".

> **This sentence asserted loading for months before any Prometheus loaded anything** — `rule_files`
> was commented out in the docker config and absent entirely from the cluster config, so every alert
> in this catalog was an artifact nothing evaluated. It is true as of the rule-loading fix. Recorded
> because the false version is a plausible reason nobody checked: a doc that states a property
> confidently is one of the ways the property stops being verified.

---

## Alert Severity Levels

### Critical

**Action**: Page on-call engineer immediately
**Response Time**: <15 minutes
**Channels**: PagerDuty + Slack #incidents
**Escalation**: 15min → on-call lead

**Criteria**:
- Service outage or severe degradation
- SLO violation (availability <99.9%, latency above threshold)
- Data loss risk
- Security incident

### Warning

**Action**: Notify team, investigate during business hours
**Response Time**: <1 hour
**Channels**: Slack #alerts
**Escalation**: 1h → service owner

**Criteria**:
- Performance degradation (not yet SLO violation)
- Resource saturation approaching limits
- Non-critical failures (elevated error rate, slow queries)

### Info

**Action**: Log only, no notification
**Response Time**: Best effort
**Channels**: Prometheus logs only
**Escalation**: None

**Criteria**:
- Informational events (deployments, config changes)
- Trend analysis (slow growth patterns)

---

## Global Controller Alerts

**File**: `infra/docker/prometheus/rules/gc-alerts.yaml`

### Critical Alerts

#### GCDown

**Severity**: Critical
**Condition**: No GC pods running for >1 minute
**Impact**: Complete service outage, users cannot join meetings
**Runbook**: [docs/runbooks/gc-down.md](../runbooks/gc-down.md) (to be created)

**PromQL**:
```promql
up{job="gc-service"} == 0
```
`for: 1m`

**Response**:
1. Check GC pod status (`kubectl get pods`)
2. Check deployment status (`kubectl describe deployment gc-service`)
3. Review logs from crashed pods
4. Escalate to platform team if Kubernetes issue

---

#### GCHighErrorRate

**Severity**: Critical
**Condition**: Error rate >1% for >5 minutes
**Impact**: Availability SLO violation (99.9% target), error budget consumption
**Runbook**: [docs/runbooks/gc-high-error-rate.md](../runbooks/gc-high-error-rate.md) (to be created)

**PromQL**:
```promql
(
  sum(rate(gc_http_requests_total{status_code=~"[45].."}[5m]))
  /
  sum(rate(gc_http_requests_total[5m]))
) > 0.01
```
`for: 5m`

**Response**:
1. Identify failing endpoints (dashboard or Prometheus)
2. Check for recent deployments (rollback if needed)
3. Check dependency health (database, AC, MC)
4. Scale horizontally if capacity issue

---

#### GCHighLatency

**Severity**: Critical
**Condition**: p95 HTTP latency >200ms for >5 minutes
**Impact**: Latency SLO violation, poor user experience
**Runbook**: [docs/runbooks/gc-high-latency.md](../runbooks/gc-high-latency.md)

**PromQL**:
```promql
histogram_quantile(0.95,
  sum by(le) (rate(gc_http_request_duration_seconds_bucket[5m]))
) > 0.200
```
`for: 5m`

**Response**:
1. Check latency source (database, MC assignment, token refresh)
2. Check resource utilization (CPU, memory)
3. Investigate slow queries
4. Scale horizontally if CPU bound

---

#### GCMCAssignmentSlow

**Severity**: Critical
**Condition**: MC assignment p95 latency >20ms for >5 minutes
**Impact**: Slow meeting join, critical path degradation
**Runbook**: [Scenario 3: MC Assignment Failures](../runbooks/gc-incident-response.md#scenario-3-mc-assignment-failures)

**PromQL**:
```promql
histogram_quantile(0.95,
  sum by(le) (rate(gc_mc_assignment_duration_seconds_bucket[5m]))
) > 0.020
```
`for: 5m`

**Population caveat (R-6)**: the histogram is bimodal (reuse vs new assignment, no separating label) and this expression is unfiltered, so p95 is dominated by the reuse path. See `docs/observability/metrics/gc-service.md` "Population note (R-6)"; fix tracked in `docs/TODO.md` §Observability Debt.

**Response**:
1. Check MC pod availability
2. Check database query latency (MC selection)
3. Check gRPC connectivity to MC
4. Scale MC if capacity issue

---

#### GCDatabaseDown

**Severity**: Critical
**Condition**: Database query error rate >50% for >1 minute
**Impact**: Complete GC outage, all operations fail
**Runbook**: [docs/runbooks/gc-database-issues.md](../runbooks/gc-database-issues.md)

**PromQL**:
```promql
(
  sum(rate(gc_db_queries_total{status="error"}[1m]))
  /
  sum(rate(gc_db_queries_total[1m]))
) > 0.5
```
`for: 1m`

**Response**:
1. Check PostgreSQL pod status
2. Test database connectivity from GC
3. Check NetworkPolicy allows GC→DB traffic
4. Escalate to DBA if database corruption or replication issues

---

#### GCErrorBudgetBurnRateCritical

**Severity**: Critical
**Condition**: Error budget burning at >10x sustainable rate for >1 hour
**Impact**: 30-day error budget will be exhausted in <3 days
**Runbook**: [docs/runbooks/gc-high-error-rate.md](../runbooks/gc-high-error-rate.md) (to be created)

**PromQL**:
```promql
(
  sum(rate(gc_http_requests_total{status_code=~"[45].."}[1h]))
  /
  sum(rate(gc_http_requests_total[1h]))
) / 0.001 > 10
```
`for: 1h`

**Response**:
1. Identify root cause of elevated error rate
2. Check recent deployments (rollback if regression)
3. Check dependency failures
4. Implement immediate mitigation to stop burn rate

---

#### GCMeetingCreationStopped

**Severity**: Critical
**Condition**: Zero meeting creation traffic for >15 minutes, with traffic in the prior hour
**Detection Delay**: ~30 minutes (15m rate window + 15m `for` clause)
**Impact**: Users cannot create meetings, possible service outage
**Runbook**: [docs/runbooks/gc-incident-response.md#scenario-4](../runbooks/gc-incident-response.md#scenario-4-complete-service-outage)

**PromQL**:
```promql
sum(rate(gc_meeting_creation_total[15m])) == 0
and
sum(rate(gc_meeting_creation_total[15m] offset 1h)) > 0
```
`for: 15m`

**Response**:
1. Check GC pod health and logs
2. Verify routing to `/api/v1/meetings` endpoint
3. Check upstream dependencies (database, auth)
4. Check for recent deployments or config changes

---

#### GCTelemetryProxySilent

**Severity**: Critical
**Condition**: Zero telemetry ingest events for 15+ minutes, with events in the baseline hour before that (covers both silence shapes: flat counters while the pod is alive, and absent series after a restart)
**Detection Delay**: ~20 minutes for the flat shape (15m rate window + 5m `for` clause)
**Auto-Resolve Caveat**: resolves on its own after ~1h15m of continuous silence as the baseline window drains — resolution is not recovery; verify ingest rate is non-zero before closing
**Impact**: Total loss of client-side observability; possible leading indicator of client-facing breakage (CORS, ingress, auth) the server cannot otherwise see
**Runbook**: [Scenario 11: Telemetry Ingest Silent](../runbooks/gc-incident-response.md#scenario-11-telemetry-ingest-silent)

**PromQL**:
```promql
(
  sum(rate(gc_telemetry_ingest_total[15m])) == 0
  and
  sum(increase(gc_telemetry_ingest_total[1h] offset 15m)) > 0
)
or
(
  absent_over_time(gc_telemetry_ingest_total[15m])
  and on()
  (sum(increase(gc_telemetry_ingest_total[1h] offset 15m)) > 0)
)
```
`for: 5m`

**Response**:
1. Check user-facing traffic first (`gc_http_requests_total` rate + error rate) — whole-service silence → Scenario 4
2. Check CORS preflight outcomes (`gc_cors_preflight_total`) for a denied/403 spike
3. Determine which silence shape fired (run the two branches separately); absent shape → check deploy/restart timeline (rolling restart in a low-traffic environment is a known benign trigger)
4. Check 401s on `gc_http_requests_total{endpoint="/other"}` + GC logs (auth regression rejects telemetry pre-handler; `gc_jwt_validations_total` is service-token/gRPC-only — corroboration for shared JWKS root causes, never clearance for the HTTP user-token path)
5. Rule out benign causes: client rollout disabled/sampled-down telemetry; natural traffic trough

---

### Warning Alerts

#### GCHighMemory

**Severity**: Warning
**Condition**: Memory usage >85% for >10 minutes
**Impact**: Risk of OOM kill, pod restart
**Runbook**: [docs/runbooks/gc-high-memory.md](../runbooks/gc-high-memory.md) (to be created)

**PromQL**:
```promql
(
  container_memory_usage_bytes{container="gc-service"}
  /
  container_spec_memory_limit_bytes{container="gc-service"}
) > 0.85
```
`for: 10m`

**Response**:
1. Check for memory leak (heap profiling)
2. Increase memory limits if needed
3. Restart pod as temporary mitigation
4. Investigate leak in code

---

#### GCHighCPU

**Severity**: Warning
**Condition**: CPU usage >80% for >5 minutes
**Impact**: High CPU may increase latency
**Runbook**: [docs/runbooks/gc-high-cpu.md](../runbooks/gc-high-cpu.md) (to be created)

**PromQL**:
```promql
rate(container_cpu_usage_seconds_total{container="gc-service"}[5m]) > 0.80
```
`for: 5m`

**Response**:
1. Check request rate (traffic spike?)
2. Profile CPU usage (identify hot paths)
3. Scale horizontally
4. Optimize hot paths in code

---

#### GCMCAssignmentFailures

**Severity**: Warning
**Condition**: MC assignment failure rate >5% for >5 minutes
**Impact**: Some users unable to join meetings
**Runbook**: [Scenario 3: MC Assignment Failures](../runbooks/gc-incident-response.md#scenario-3-mc-assignment-failures)

**PromQL**:
```promql
(
  sum(rate(gc_mc_assignments_total{status!="success"}[5m]))
  /
  sum(rate(gc_mc_assignments_total[5m]))
) > 0.05
```
`for: 5m`

<!-- ANCHOR (DRY): the rejection_reason values in step 2 mirror the terminal match in
     crates/gc-service/src/services/mc_assignment.rs (source of truth). Sibling mirrors:
     docs/observability/metrics/gc-service.md (:74 + cardinality table) and the
     gc-incident-response.md legend — edit all in lockstep. -->
**Response**:
1. **Break down by `rejection_reason` FIRST** (`sum by(rejection_reason) (increase(gc_mc_assignments_total{status!="success"}[5m]))`). Emitted values: `at_capacity`, `draining`, `unhealthy`, `unspecified`, `no_mcs_available`, `invalid_request`.
2. Check MC pod health and heartbeat in database.
3. **Scale MC ONLY for a genuine capacity reason** (`at_capacity`, or `no_mcs_available` with an empty/unhealthy pool). Do **NOT** scale for `invalid_request` — that is a GC-side contract violation, not a fleet problem; follow Scenario F (do not scale, escalate to the GC owner).

---

#### GCDatabaseSlow

**Severity**: Warning
**Condition**: Database query p99 latency >50ms for >5 minutes
**Impact**: Slow queries may cause HTTP latency SLO violations
**Runbook**: [docs/runbooks/gc-database-issues.md](../runbooks/gc-database-issues.md)

**PromQL**:
```promql
histogram_quantile(0.99,
  sum by(le) (rate(gc_db_query_duration_seconds_bucket[5m]))
) > 0.050
```
`for: 5m`

**Response**:
1. Identify slow queries (pg_stat_activity)
2. Check for missing indexes
3. Check database resource usage
4. Optimize queries or add indexes

---

#### GCTokenRefreshFailures

**Severity**: Warning
**Condition**: Token refresh failure rate >10% for >5 minutes
**Impact**: Risk of authentication failures for GC→MC/MH calls
**Runbook**: [docs/runbooks/gc-token-refresh-failures.md](../runbooks/gc-token-refresh-failures.md) (to be created)

**PromQL**:
```promql
(
  sum(rate(gc_token_refresh_total{status="error"}[5m]))
  /
  sum(rate(gc_token_refresh_total[5m]))
) > 0.10
```
`for: 5m`

**Response**:
1. Check AC service health
2. Check network connectivity to AC
3. Check token expiration configuration
4. Review AC logs for rejection reasons

---

#### GCErrorBudgetBurnRateWarning

**Severity**: Warning
**Condition**: Error budget burning at >5x sustainable rate for >6 hours
**Impact**: 30-day error budget will be exhausted in <6 days
**Runbook**: [docs/runbooks/gc-high-error-rate.md](../runbooks/gc-high-error-rate.md) (to be created)

**PromQL**:
```promql
(
  sum(rate(gc_http_requests_total{status_code=~"[45].."}[6h]))
  /
  sum(rate(gc_http_requests_total[6h]))
) / 0.001 > 5
```
`for: 6h`

**Response**:
1. Investigate error rate trend
2. Identify error sources
3. Plan mitigation before reaching critical burn rate

---

#### GCPodRestartingFrequently

**Severity**: Warning
**Condition**: Pod restart rate >1 per hour for >5 minutes
**Impact**: Service instability, potential crash loop
**Runbook**: [docs/runbooks/gc-pod-crashes.md](../runbooks/gc-pod-crashes.md) (to be created)

**PromQL**:
```promql
rate(kube_pod_container_status_restarts_total{container="gc-service"}[1h]) > 0.016
```
`for: 5m`

**Response**:
1. Check logs from crashed pods
2. Check for OOM kills (memory limits)
3. Check liveness probe configuration
4. Investigate panic/crash causes in code

---

#### GCMeetingCreationFailureRate

**Severity**: Warning
**Condition**: Meeting creation failure rate >5% for >5 minutes
**Impact**: Some users unable to create meetings
**Runbook**: [Scenario 8: Limit Exhaustion](../runbooks/gc-incident-response.md#scenario-8-meeting-creation-limit-exhaustion), [Scenario 9: Code Collision](../runbooks/gc-incident-response.md#scenario-9-meeting-code-collision)

**PromQL**:
```promql
(
  sum(rate(gc_meeting_creation_total{status="error"}[5m]))
  /
  sum(rate(gc_meeting_creation_total[5m]))
) > 0.05
and
sum(rate(gc_meeting_creation_total[5m])) > 0
```
`for: 5m`

**Response**:
1. Check "Meeting Creation Failures by Type" dashboard panel for error breakdown
2. If `org_limit` errors dominate → genuine capacity exhaustion, [Scenario 8: Limit Exhaustion](../runbooks/gc-incident-response.md#scenario-8-meeting-creation-limit-exhaustion)
3. If `org_inactive` or `org_not_provisioned` errors present → organization-state fault, **not** a full cap. See `GCMeetingCreationOrgStateInvalid` below and [Scenario 8](../runbooks/gc-incident-response.md#scenario-8-meeting-creation-limit-exhaustion)
4. If `forbidden` errors dominate → role denial (caller lacks a meeting-create role). **Not** a capacity problem — no runbook scenario applies
5. If `code_collision` errors present → [Scenario 9: Code Collision](../runbooks/gc-incident-response.md#scenario-9-meeting-code-collision) (investigate seriously)
6. If `db_error` errors dominate → [Scenario 1: Database Connection Failures](../runbooks/gc-incident-response.md#scenario-1-database-connection-failures)
7. Check database health and query latency

> **`forbidden` changed meaning on 2026-08-14 (story R-6).** It previously covered
> both role denial and org cap exhaustion; cap exhaustion is now `org_limit`. A
> `forbidden` spike is no longer evidence of a capacity problem.

---

#### GCMeetingCreationOrgStateInvalid

**Severity**: Warning
**Condition**: Any meeting creation refused with `error_type` of `org_inactive` or `org_not_provisioned` within the last hour, sustained 15m
**Impact**: Affected callers cannot create meetings. `org_not_provisioned` returns 500 (and so also counts toward the aggregate HTTP error-rate SLO alerts); `org_inactive` returns 403
**Runbook**: [Scenario 8: Limit Exhaustion](../runbooks/gc-incident-response.md#scenario-8-meeting-creation-limit-exhaustion)

**PromQL**:
```promql
sum(increase(gc_meeting_creation_failures_total{error_type=~"org_inactive|org_not_provisioned"}[1h])) > 0
```
`for: 15m`

**Absence semantics — read this before treating a firing as a misconfiguration**:
- **Steady state is provably zero.** No code path deactivates or deletes an organization, and AC will not mint a token for an inactive org. A valid token naming a missing or inactive org means GC and AC disagree about database state.
- **Zero is healthy; no data is a fault.** The series is present-at-zero from process start, so normal operation reads `0`, not absent. Absence means the series stopped being exported — pod down, scrape broken, metric or label renamed — and should be investigated rather than read as calm. (Before the present-at-zero fix this rule could not have fired at all: the series was born at 1 on its first event with no `0→1` edge for `increase()` to measure.)
- **One occurrence is the signal.** `for: 15m` debounces scrape flapping; it does **not** require repeated events. Expected detection delay ~15m.
- **This rule has no automated exerciser** — the repo has no `promtool test rules` harness — so the fact that it has never fired before is *not* evidence that it works, nor that this firing is spurious.

**Response**:
1. Confirm which value fired: `sum by(error_type) (increase(gc_meeting_creation_failures_total[1h]))`
2. Work the cause table in [Scenario 8](../runbooks/gc-incident-response.md#scenario-8-meeting-creation-limit-exhaustion) — triage order is database volume reset, then GC/AC pointed at different databases, then a restore, then out-of-band SQL
3. Distinguish the two regimes by duration: a one-off org-state change decays to zero within ~1h15m as outstanding tokens expire; anything sustained past that is an ongoing divergence, not a tail
4. `org_limit` is deliberately **excluded** from this rule — a full cap is capacity behaviour, not a fault

---

#### GCMeetingCreationLatencyHigh

**Severity**: Warning
**Condition**: Meeting creation p95 latency >500ms for >5 minutes
**Threshold Rationale**: 500ms is higher than the 200ms aggregate HTTP SLO because meeting creation involves DB writes, CSPRNG code generation, and atomic limit-check CTE. The aggregate `GCHighLatency` alert covers SLO violations at 200ms.
**Impact**: Slow meeting creation experience
**Runbook**: [docs/runbooks/gc-incident-response.md#scenario-2](../runbooks/gc-incident-response.md#scenario-2-high-latency--slow-responses)

**PromQL**:
```promql
histogram_quantile(0.95,
  sum by(le) (rate(gc_meeting_creation_duration_seconds_bucket[5m]))
) > 0.500
```
`for: 5m`

**Response**:
1. Check "Meeting Creation Latency" dashboard panel for latency trend
2. Check database query latency (create_meeting operation)
3. Investigate code generation performance (collision retries)
4. Check resource utilization (CPU, memory)

---

#### GCHighJoinFailureRate

**Severity**: Warning
**Condition**: Meeting join failure rate >5% for >5 minutes
**Impact**: Some users unable to join meetings
**Runbook**: [Scenario 3: MC Assignment Failures](../runbooks/gc-incident-response.md#scenario-3-mc-assignment-failures)

**PromQL**:
```promql
(
  sum(rate(gc_meeting_join_total{status="error"}[5m]))
  /
  sum(rate(gc_meeting_join_total[5m]))
) > 0.05
and
sum(rate(gc_meeting_join_total[5m])) > 0
```
`for: 5m`

**Response**:
1. Check "Meeting Join Failures by Type" dashboard panel for error breakdown
2. If `mc_assignment` errors dominate → [Scenario 3: MC Assignment Failures](../runbooks/gc-incident-response.md#scenario-3-mc-assignment-failures)
3. If `ac_request` errors present → check AC service health and token refresh
4. If `not_found` errors dominate → check meeting lookup and database health
5. Check "Meeting Join Success Rate (%)" gauge for current success rate

---

#### GCTelemetryProxyHighRejectionRate

**Severity**: Warning
**Condition**: Telemetry ingest rejection rate (`status=~"rejected_.*|error"`) >10% for >10 minutes, gated on non-zero traffic
**Impact**: Client telemetry being dropped — degraded client-side observability; if `error` dominates with 502/503, the collector forwarding path is failing
**Runbook**: [Scenario 10: Telemetry Proxy High Rejection Rate](../runbooks/gc-incident-response.md#scenario-10-telemetry-proxy-high-rejection-rate)

**PromQL**:
```promql
(
  sum(rate(gc_telemetry_ingest_total{status=~"rejected_.*|error"}[10m]))
  /
  sum(rate(gc_telemetry_ingest_total[10m]))
) > 0.10
and
sum(rate(gc_telemetry_ingest_total[10m])) > 0
```
`for: 10m`

**Response**:
1. Split fault direction on the telemetry counter's own `status` values (`rejected_size`/`rejected_rate` → client fault; `error` → step 2)
2. Split `error` via paired HTTP status codes — 400/415 client fault vs 502/503 collector fault (caveat: telemetry routes report as `endpoint="/other"` on `gc_http_*`, conflated with other unrecognized paths; confirm via GC logs)
3. If rate-limited: check `gc_telemetry_rate_limited_total` by reason (client retry loop without backoff?)
4. If client fault: correlate with web-app/SDK deploy timeline → Client/Web-App Team rollback
5. If collector fault: check otel-collector health → Infrastructure/SRE

---

#### GCHighJoinLatency

**Severity**: Info
**Condition**: Meeting join p95 latency >2s for >5 minutes
**Impact**: Slow meeting join experience
**Runbook**: [Scenario 2: High Latency](../runbooks/gc-incident-response.md#scenario-2-high-latency--slow-responses)

**PromQL**:
```promql
histogram_quantile(0.95,
  sum by(le) (rate(gc_meeting_join_duration_seconds_bucket[5m]))
) > 2.0
```
`for: 5m`

**Population caveat (R-6)**: the histogram is bimodal (reuse vs new assignment, no separating label) and this expression is unfiltered, so p95 is dominated by the reuse path. See `docs/observability/metrics/gc-service.md` "Population note (R-6)"; fix tracked in `docs/TODO.md` §Observability Debt.

**Response**:
1. Check "Meeting Join Latency (P50/P95/P99)" dashboard panel for latency trend
2. Check MC assignment latency (may be slow MC selection)
3. Check AC token request latency (may be slow token issuance)
4. Check database query latency for meeting lookups
5. Check resource utilization (CPU, memory)

---

## Authentication Controller Alerts

**Status**: 🚧 To be created
**File**: `infra/docker/prometheus/rules/ac-alerts.yaml` (planned)

**Planned Critical Alerts**:
- `ACDown` - No AC pods running
- `ACHighTokenIssuanceLatency` - Token issuance p99 >350ms
- `ACHighTokenValidationErrorRate` - Validation errors >1%
- `ACKeyRotationFailed` - Key rotation failed

**Planned Warning Alerts**:
- `ACHighCPU` - CPU >80%
- `ACHighMemory` - Memory >85%
- `ACJWKSCacheMissRate` - JWKS cache miss rate >10%

---

## Meeting Controller Alerts

**File**: `infra/docker/prometheus/rules/mc-alerts.yaml`

### Critical Alerts

Existing MC `severity: page` alerts: `MCDown`, `MCActorPanic`, `MCHighMailboxDepthCritical`, `MCMediaConnectionAllFailed`, `MCMediaGenerationDivergence`, `MCKekRotationOverdue`. That is the complete set — `mc-alerts.yaml` is the source of truth and this list is a convenience copy; verify against it rather than citing this line. (`MCHighLatency`, `MCHighMessageDropRate` and `MCGCHeartbeatFailure` appeared here and have never existed in any rules file.)

**Every MC rule is inventoried as of story 2 task 16**, except the pre-story-2 ones this section never covered: `MCDown`, `MCActorPanic`, `MCHighMailboxDepthCritical`, `MCMediaConnectionAllFailed` and the join-flow warnings not listed below. Read `mc-alerts.yaml` rather than inferring absence from this file. The ten entries task 16 owed are the two KEK warnings, the page, the sender-id info rule and the six task-12 teardown warnings, all below by name. The `inventory_expr_drift` guard holds byte-identity for every inventoried entry. It does **not** require that every rule have one, so nothing mechanical reports this file incomplete.

#### MCMediaGenerationDivergence

**Severity**: Page
**Condition**: MH echoes an applied media-policy generation that differs from the one MC sent, or echoes none at all — any occurrence in 15 minutes. Since story 2 pushes happen on every structural change rather than once per meeting; the one-event threshold holds because each push to a handler is serialized (no out-of-order non-match), and a push that fails at transport never reaches this counter (it lands on `mc_register_meeting_total{status="error"}`), so higher push volume does not widen the false-positive surface.
**Impact**: MH is forwarding under stale or absent policy while every liveness signal reads green. ADR-0036 §8 calls this a partial blackhole reporting healthy. Blast radius is per (meeting, handler).
**Runbook**: [Scenario 15: Media Generation Divergence](../runbooks/mc-incident-response.md#scenario-15-media-generation-divergence)

**PromQL**:
```promql
sum(increase(mc_media_policy_pushes_total{outcome!~"match|handler_id_mismatch"}[15m])) > 0
```
`for: 0m`

**Response**:
1. **Any structural change re-pushes; force one only if the meeting is quiescent.** Of ADR-0036 §8's four re-fire triggers only *structural change* is implemented, but since story 2 it fires on every join, leave, capability declaration and mute, and a push that failed stays unconfirmed so the next roster event of any kind re-sends it. A meeting with churn therefore often clears by itself — check whether the counter has stopped moving before acting. There is still no periodic re-assert (story 4), so a quiescent meeting stays dark until something changes. An MC restart against a live handler is **not counted here at all**: MC adopts MH's generation as a floor, re-pushes above it, and records that on `mc_media_policy_generation_adoptions_total` instead, so this page does not fire on MC rollouts. A `generation_mismatch` with `applied > sent` that DOES reach this alert is a FAILED adoption or a higher echo after a confirm (e.g. the `u64::MAX` ratchet wedge) — not rollout noise.
2. Split on the `outcome` label first — the values have different first moves. **The MC-restart arm is under NO `outcome` label by design**: read `mc_media_policy_generation_adoptions_total` and the adoption WARN at `mc.register_meeting.trigger` (carrying `sent_generation`, `applied_generation`, `adopted_generation`) alongside the split, so a clean breakdown after an MC restart is not mistaken for "nothing happened". `generation_mismatch` means MH's apply ran and did not take effect (open MH, read `mh_media_policy_applies_total{outcome}`); `no_applied_generation` means MH echoed nothing (confirm the MH image first — a rollout skew is the cheap explanation and it clears itself); `transport_mode_mismatch` is ADR-0036 §8's separate two-ends-must-agree echo failing, a different remedy reached from the same runbook.
3. Read `mc_media_generation_divergence` for the magnitude **second, never first**: it is last-write-wins at pod level, so a healthy push for an unrelated meeting erases a diverged reading, and a value of 0 is not evidence that nothing diverged.
4. **If the meeting is quiescent, force a structural change — prefer a capability re-declaration or a mute toggle.** Either makes MC re-publish, and the unconfirmed push is re-sent. **Do not reach for leave-and-rejoin in a meeting with one participant**: as of story 2 task 12 the last participant leaving *ends* the meeting — MC releases the handler and notifies GC — so the rejoin lands on a re-created meeting rather than re-publishing to the diverged one. The divergence clears, but you have replaced the meeting instead of repairing it, and the evidence goes with it. A mute toggle is the cheapest safe trigger and costs the participant nothing.
5. **Do not restart MH.** It sheds every media session on the pod, recovers none of the already-dark ones, and destroys the evidence.

> **The selector is NEGATED, and that is the design, not a shorthand.** It reads "everything that is not a confirmed match and not the known-noisy diagnostic". `PolicyPushOutcome::ALL` is compile-checked in Rust, but **that compile error does not reach PromQL** — so under the negated form a sixth outcome added later pages and gets classified deliberately, while under a positive `outcome=~"a|b"` it would fall silently outside the alert. For a page alert whose subject is *a partial blackhole reporting healthy*, silence is the failure mode to defend against.
>
> `handler_id_mismatch` is the one exclusion beyond `match`: `MH_HANDLER_ID` is per-incarnation, so an ordinary MH restart produces it by construction and a bare `outcome != "match"` would page on every rollout. That exclusion is held at three sites — this rule, `docs/observability/metrics/mc-service.md`'s catalog entry, and `mc-deployment.md`'s post-deploy checklist — as **one decision with one revert trigger**, `2026-09-02-mh-stable-handler-id`; remove it in all three places together.

#### MCKekRotationOverdue

**Severity**: Page
**Condition**: The oldest un-rotated departure in some meeting is older than the published overdue threshold, with no arithmetic: `mc_meeting_kek_rotation_pending_age_seconds` is compared bare against `mc_meeting_kek_rotation_overdue_threshold_seconds` (W times a compile-time multiplier, published by MC from the value its timer enforces). The condition must hold for 2 minutes.
**Impact**: **Confidentiality, not media quality.** A departed participant still holds a KEK that opens every current participant's media in that meeting. When this fires, W has already been exceeded by roughly the multiplier; it is not a leading indicator.
**Runbook**: [Scenario 19: KEK Rotation Stalled](../runbooks/mc-incident-response.md#scenario-19-kek-rotation-stalled)

**PromQL**:
```promql
mc_meeting_kek_rotation_pending_age_seconds > mc_meeting_kek_rotation_overdue_threshold_seconds
```
`for: 2m`

**Label-set identity is load-bearing**: `a > b` matches equal label sets, so a label added to only one of the two gauges silently empties the comparison. **Demonstration** (ADR-0036 §11): stall the rotation timer and watch pending age cross the threshold gauge. Fork first on `mc_meeting_kek_rotation_failures_total{reason}`; if neither reason moves, suspect a wedged meeting actor (`mc_actor_mailbox_depth`).

### Warning Alerts (Join Flow)

#### MCMediaMissingKeyMaterial

**Severity**: Warning
**Condition**: >5% of received media frames dropped for missing key material (four reasons: `no_kek_for_generation`, `kek_generation_stale`, `no_roster_entry`, `unwrap_failed`), sustained 15 minutes
**Impact**: Affected participants hear nothing while every server-side signal reads healthy — MH never opens a frame and structurally cannot observe any of the four conditions.
**Runbook**: [Scenario 16: Missing Key Material](../runbooks/mc-incident-response.md#scenario-16-missing-key-material)

> **THIS ALERT CAN NOW FIRE, AND HAS BEEN PROVEN TO MATCH.** It previously carried a notice that it was structurally incapable of matching, because `dt_client_*` metrics reached no Prometheus. That is fixed: the collector's `prometheus` exporter, the `otel-collector` scrape job and the GC telemetry-filter widening landed together, and the expression below returned `0.286` against live data driven by sdk-core's own built bundle through the GC proxy. Recorded as an event rather than deleted, because "this alert has never fired" means something different before and after that date, and a responder reading history needs to know which side of it they are on.
>
> **Two scope limits that are NOT threshold problems and will not show up as a failure.**
>
> 1. **Guest participants are invisible to this alert.** GC's telemetry routes require user claims (`route_layer(require_user_auth)`), so guest tokens receive a 401 and emit nothing. Both the numerator and the denominator therefore exclude every guest in the meeting. A meeting whose affected participants are all guests reads completely healthy here. Tracked as a latent gap in `docs/TODO.md`; the practical consequence today is that every browser in an N+1 demo must sign in as a registered user or the detector is blind to it.
> 2. **Absence of data is still not evidence of health.** The series is now lazily created on first emission, so it is absent — not zero — until a browser reports. The non-zero-denominator guard in the expression correctly suppresses the alert in that state, which is right, but it means silence spans both "key delivery is healthy" and "no client has been connected recently". Discriminate before concluding: `docs/observability/dashboards.md` §Client SDK Media Path carries the three-step procedure.

**PromQL**:
```promql
(
  sum(rate(dt_client_media_frames_dropped_total{reason=~"no_kek_for_generation|kek_generation_stale|no_roster_entry|unwrap_failed"}[5m]))
  /
  sum(rate(dt_client_media_frames_received_total[5m]))
) > 0.05
and
sum(rate(dt_client_media_frames_received_total[5m])) > 0
```
`for: 15m`

**The selector is an EXPLICIT TOKEN ENUMERATION, and its partition is MECHANICAL** (story 2 task 16). `dt-guard client-metrics-export` (G7) requires every `drops_frame: true` token in `proto/test-vectors/frame-v2.vectors.json` to be in **exactly one** of this alternation or the guard's `NOT_KEY_DELIVERY` list, which carries each excluded token's reason; `sender_not_assigned` (misrouting) is one of them. So a new reject token fails the build until someone classifies it. **Deriving membership from `layer` was considered and rejected**: `layer` names the processing stage, not the remedy owner, and `unwrap_failed` (layer `crypto`) belongs here. **`unwrap_failed` joined by decision**: it is an authenticated frame whose KEK unwrap failed. Its main cause is a KEK-bytes split between sender and receiver, which is key distribution; the others are sender wrap bugs, whose user symptom is identical and which nothing else selects. It has no healthy transient.

**Threshold, re-derived against W over the four-token set (task 16)**: 5% and `for: 15m` stand. The healthy ratio is a duty cycle of about (per-rotation KEK push skew) / W. Rotations happen at most once per W per meeting, W ≥ `MIN_KEK_ROTATION_DEBOUNCE_SECONDS`, retention far exceeds the skew so `kek_generation_stale` is ~0, `no_roster_entry` is join-only, and `unwrap_failed` has no transient. Holding 5% for 15 minutes at the W floor needs a skew of 5% of W on every rotation, with a leave every W. The story's ~25x margin holds iff skew is under 0.2% of W. **The skew is unmeasured**: this is a stated assumption, not a ratified number. The full derivation is in the rule comment.

**Blind spot and its named compensating control**: this is a fleet-wide ratio, so one client in a hundred with a failing push reads 1% forever. `MCKekPushFailureRate` (MC's per-recipient push counter) and `MCClientKekConflictingKey` are the small-denominator detectors.

**Response**:
1. **Split on the `reason` label first** — the four arms have different remedies and are not equally instrumented.
2. `no_roster_entry`: no usable identity key for the sender, including the case where MC published an empty key. Signature verification cannot run, so attribution is failing and not merely decryption. Corroborate server-side with `mc_join_identity_key_presence_total{presence}` — a rising `absent` ratio answers "are clients publishing keys?" directly. A *malformed* key is a different condition and lands on `mc_session_join_failures_total{error_type="identity_key_invalid"}`.
3. `no_kek_for_generation`: the frame's wrap announces a KEK generation NEWER than any this receiver holds — **MC's KEK push has not arrived** (MC delivery path). **This arm has no server-side counter and cannot have one** — `mc_meeting_kek_generated_total` increments unconditionally and its catalog entry states the inference is not computable even in principle. Absence of a signal here is not evidence the KEK is present.
4. `kek_generation_stale`: the frame's wrap announces a generation OLDER than this receiver still retains (it keeps the current generation plus at most one previous, for `min(W/2, ceiling)` where W is MC's `kek_rotation_debounce_seconds`). At the shipped default W, client retention is **already at its ceiling** (`KEK_RETENTION_CEILING_MS`, 30 s), so raising `MC_KEK_ROTATION_DEBOUNCE_SECONDS` does **not** lengthen it — it only flips the fleet to `ceiling_clamped`. The only lever that lengthens retention past 30 s is the client ceiling itself, an SDK constant requiring a release **and** a deliberate key-lifetime bound (ADR-0036 §4) — a security decision, not a remedy an operator applies. Lowering W *shortens* retention and makes this worse. A sustained `kek_generation_stale` at default configuration therefore points at **sender-side rotation skew**, not at MC's debounce. Read `dt_client_media_kek_retention_anomalies_total{outcome}` for why retention was what it was (`floor_substituted` = an MC older than the field, expected during a deliberate MC rollback; `below_rewrap_latency` = W/2 under the client's re-wrap latency) and `dt_client_media_kek_install_refusals_total{outcome}` for KEK messages the client refused. Both KEK arms fire only when no usable transmit key for the frame's key id is already cached.
5. Check the non-dump path first and completely: the per-join response-side condition (`meeting_kek` not exactly 32 bytes) and `dt_client_media_kek_updates_total{source=~"join_response|kek_update"}` — the join response is no longer the only KEK source. Then read the runbook's dump gate before going further.
6. `unwrap_failed`: an authenticated frame whose KEK unwrap failed. It has **no** healthy transient, so a sustained rate is always signal. The main cause is a KEK-bytes split between sender and receiver; check `MCClientKekConflictingKey` / `dt_client_media_kek_install_refusals_total{outcome="conflicting_key"}`. The others are a sender wrap or key-schedule bug (`client`) and a misbehaving authenticated member (`security`). Runbook Scenario 16 Arm 4.
7. The first three reasons are **expected transiently** at join and after every KEK rotation. The `for: 15m` window, not the threshold, is what separates the transient from the signal.

**Threshold provenance**: 5% is not SLO-derived and there is no observed baseline. The re-derivation against W is above and in the rule comment.

#### MCHighJoinFailureRate

**Severity**: Warning
**Condition**: Session join failure rate >5% for >5 minutes
**Impact**: Some users unable to join meetings via WebTransport
**Runbook**: [Scenario 8: Join Failures](../runbooks/mc-incident-response.md#scenario-8-join-failures)

**PromQL**:
```promql
(
  sum(rate(mc_session_joins_total{status="failure"}[5m]))
  /
  sum(rate(mc_session_joins_total[5m]))
) > 0.05
and
sum(rate(mc_session_joins_total[5m])) > 0
```
`for: 5m`

**Response**:
1. Check "Session Join Failures by Type" panel in MC Overview dashboard
2. If `jwt_validation` errors dominate -> check AC service health and JWKS endpoint
3. If `meeting_not_found` errors -> check GC-MC meeting state synchronization
4. If `mc_capacity_exceeded` errors -> scale MC horizontally
5. Check WebTransport connection health and rejection rates

---

#### MCHighWebTransportRejections

**Severity**: Warning
**Condition**: WebTransport connection rejection rate >10% for >5 minutes
**Impact**: Users unable to establish WebTransport sessions
**Runbook**: [Scenario 9: WebTransport Rejections](../runbooks/mc-incident-response.md#scenario-9-webtransport-rejections)

**PromQL**:
```promql
(
  sum(rate(mc_webtransport_connections_total{status="rejected"}[5m]))
  /
  sum(rate(mc_webtransport_connections_total[5m]))
) > 0.10
and
sum(rate(mc_webtransport_connections_total[5m])) > 0
```
`for: 5m`

**Response**:
1. Check MC capacity (active meetings count vs limit)
2. Check "WebTransport Connections by Status" panel in MC Overview dashboard
3. If capacity-related -> scale MC horizontally
4. If client errors -> check client SDK version and WebTransport compatibility
5. Check MC pod resource utilization (CPU, memory)

---

#### MCHighJwtValidationFailures

**Severity**: Warning
**Condition**: JWT validation failure rate >10% for >5 minutes
**Impact**: Users unable to authenticate for meeting join
**Runbook**: [Scenario 10: JWT Validation Failures](../runbooks/mc-incident-response.md#scenario-10-jwt-validation-failures)

**PromQL**:
```promql
(
  sum(rate(mc_jwt_validations_total{result="failure"}[5m]))
  /
  sum(rate(mc_jwt_validations_total[5m]))
) > 0.10
and
sum(rate(mc_jwt_validations_total[5m])) > 0
```
`for: 5m`

**Response**:
1. Check AC service health and JWKS endpoint availability
2. Check "JWT Validations by Result & Type" panel in MC Overview dashboard
3. If JWKS fetch failures -> check network connectivity to AC
4. If token expiry issues -> check clock skew between services
5. If sudden spike -> check for recent AC key rotation or config changes

---

### Warning Alerts (Media Keys)

#### MCKekPushFailureRate

**Severity**: Warning
**Condition**: More than 1% of per-recipient `MeetingKekUpdate` pushes neither delivered nor to a benign grace-period member (`participant_gone`), for 10 minutes, with a non-zero-denominator guard.
**Impact**: Members that miss a push cannot open frames under the new generation and do not rotate their own transmit keys. There is no re-push, so in a quiet meeting this does not clear on its own.
**Runbook**: [Scenario 16: Missing Key Material](../runbooks/mc-incident-response.md#scenario-16-missing-key-material)

**PromQL**:
```promql
(
  sum(rate(mc_meeting_kek_pushes_total{outcome!~"delivered|participant_gone"}[10m]))
  /
  sum(rate(mc_meeting_kek_pushes_total[10m]))
) > 0.01
and
sum(rate(mc_meeting_kek_pushes_total[10m])) > 0
```
`for: 10m`

**This is the small-denominator detector** that `MCMediaMissingKeyMaterial`'s fleet-wide client ratio cannot be: one client in a hundred whose pushes always fail reads 1% there forever. Split on `outcome` first: `dropped_outbound`, `actor_unavailable`, `timed_out`.

#### MCKekRotationStorm

**Severity**: Warning
**Condition**: Leave-triggered rotations (`mc_meeting_kek_generated_total{trigger="participant_left"}`) per active meeting exceed twice 1/W, with W read from `mc_meeting_kek_rotation_window_seconds`, averaged over 10 minutes.
**Impact**: Every rotation pushes a new KEK to every member and makes every sender rotate its transmit keys. A storm multiplies control-plane and client key work across the fleet.
**Runbook**: [Scenario 17: KEK Rotation Storm / Flapping Participant](../runbooks/mc-incident-response.md#scenario-17-kek-rotation-storm--flapping-participant)

**PromQL**:
```promql
(
  sum(rate(mc_meeting_kek_generated_total{trigger="participant_left"}[10m]))
  /
  sum(avg_over_time(mc_meetings_active[10m]))
) > 2 / scalar(max(mc_meeting_kek_rotation_window_seconds))
and
sum(avg_over_time(mc_meetings_active[10m])) > 0
and on()
sum(mc_meetings_active) > 0
```
`for: 10m`

The debounce limits each meeting to one leave rotation per W, so this is **not reachable while the debounce works**: suspect the debounce, not the churn. Fork on the `trigger` label first.

#### MCClientKekConflictingKey

**Severity**: Warning
**Condition**: Any client has counted `dt_client_media_kek_install_refusals_total{outcome="conflicting_key"}`: it was handed different KEK bytes under a generation it already holds, and refused them.
**Impact**: Part of a meeting may be unable to open another member's media (peers see `unwrap_failed`) with no server-side signal. It is security-relevant, because a same-generation key swap is what the refusal exists to catch.
**Runbook**: [Tripwire: conflicting KEK](../runbooks/mc-incident-response.md#tripwire-conflicting-kek)

**PromQL**:
```promql
sum(dt_client_media_kek_install_refusals_total{outcome="conflicting_key"}) > 0
```
`for: 1m`

**Presence-shaped, not `increase()`**: the counter is absent until the first increment and appears already at ≥ 1, so `increase()` would never see the one event that matters. `dt-guard client-metrics-export` (`tripwire_rate_wrapped`) fails the build on an `increase()`/`rate()` wrapping; that is a shape guard, and the appear-at-1 firing claim is unverified by execution. **It clears when the collector's exporter drops the idle series** (`metric_expiration`, `infra/services/otel-collector/collector.yaml`), not when the condition stops. A second occurrence while firing shows only in the raw counter. Idle tabs do not hold it up. **Every known non-zero cause is listed at the rule**. A new legitimate cause is added there; the rule is never retired or silenced for it.

**Sole observable witness** of its arm, and the small-denominator detector for `MCMediaMissingKeyMaterial`'s `unwrap_failed` arm.

#### MCClientRosterKeyRebind

**Severity**: Warning
**Condition**: Any client has counted `dt_client_media_roster_key_rebinds_total{outcome="rebind"}`: a sender id arrived bound to a different identity key. The `downgrade` arm is expected non-zero and is panel-only.
**Impact**: Frames from the rebound sender may be attributed to the wrong identity or refused. Sender attribution is what ADR-0036 §3 signatures exist to provide.
**Runbook**: [Tripwire: roster key rebind](../runbooks/mc-incident-response.md#tripwire-roster-key-rebind)

**PromQL**:
```promql
sum(dt_client_media_roster_key_rebinds_total{outcome="rebind"}) > 0
```
`for: 1m`

**Presence-shaped, not `increase()`**: the counter is absent until the first increment and appears already at ≥ 1, so `increase()` would never see the one event that matters. `dt-guard client-metrics-export` (`tripwire_rate_wrapped`) fails the build on an `increase()`/`rate()` wrapping; that is a shape guard, and the appear-at-1 firing claim is unverified by execution. **It clears when the collector's exporter drops the idle series** (`metric_expiration`, `infra/services/otel-collector/collector.yaml`), not when the condition stops. A second occurrence while firing shows only in the raw counter. Idle tabs do not hold it up. **Every known non-zero cause is listed at the rule**. A new legitimate cause is added there; the rule is never retired or silenced for it.

**Not zero-forever** (paired-client correction at task 16): a lost `ParticipantLeft` followed by a sender-id reissue reaches it. That cause has a **necessary condition**: a sender-id-exhaustion epoch reset in the meeting (`mc_meeting_kek_generated_total{trigger="sender_space_exhausted"}` / `MCKekEpochResetOnSenderIdExhaustion`; ids are reissued only under a later generation, `media_admission/epoch.rs`). Check it first, as the raw counter with no range window (it covers MC's process lifetime). It is fleet-wide with no meeting label, so only a fleet-wide zero rules the cause out. `mc_participant_outbound_messages_dropped_total{payload_kind="participant_update_left"}`, read over MC's lifetime, only supports it. A short-window zero rules nothing out, because the lost Left can precede the reissue by hours.

### Warning Alerts (Meeting Teardown)

#### MCEndMeetingOwnershipRejected

**Severity**: Warning
**Condition**: Any `mc_media_end_meeting_total{outcome="rejected_ownership"}` in 15 minutes: a handler answered `EndMeeting` with FAILED_PRECONDITION because the `mc_id` MC sent is not the one recorded for the meeting.
**Impact**: The handler keeps that meeting's registration, routes and edge budget until it restarts. The release is refused and never retried.
**Runbook**: [Scenario 20: Meeting Teardown Failing / MH Budget Ratchet](../runbooks/mc-incident-response.md#scenario-20-meeting-teardown-failing--mh-budget-ratchet)

**PromQL**:
```promql
sum(increase(mc_media_end_meeting_total{outcome="rejected_ownership"}[15m])) > 0
```
`for: 0m`

Fleet-contract-reads-zero-forever and present at zero from process start, so `> 0` is never inert. The graceful-shutdown population (`superseded_by_successor`) is deliberately not selected, because it is routine on rolling deploys.

#### MCEndMeetingFailureRate

**Severity**: Warning
**Condition**: More than 5% of meeting releases to handlers fail, over 15 minutes. Both selectors are positive enumerations, and there is a non-zero-denominator guard.
**Impact**: Handlers keep ended meetings' registrations and edge budget, which consumes `MH_MAX_REGISTERED_MEETINGS` and denies new meetings on that handler.
**Runbook**: [Scenario 20: Meeting Teardown Failing / MH Budget Ratchet](../runbooks/mc-incident-response.md#scenario-20-meeting-teardown-failing--mh-budget-ratchet)

**PromQL**:
```promql
(
  sum(rate(mc_media_end_meeting_total{outcome=~"rejected_ownership|unimplemented|unavailable_exhausted|invalid_argument|error"}[15m]))
  /
  sum(rate(mc_media_end_meeting_total{outcome=~"released|rejected_ownership|unimplemented|unavailable_exhausted|invalid_argument|error"}[15m]))
) > 0.05
and
sum(rate(mc_media_end_meeting_total{outcome=~"released|rejected_ownership|unimplemented|unavailable_exhausted|invalid_argument|error"}[15m])) > 0
```
`for: 15m`

#### MCPushQuiesceTimeouts

**Severity**: Warning
**Condition**: Any `mc_media_push_quiesce_total{outcome="timed_out"}` in 30 minutes: a teardown released without its policy pushes draining.
**Impact**: Possibly one meeting's registration and edge budget re-created on a handler after its release, held until that handler restarts.
**Runbook**: [Scenario 20: Meeting Teardown Failing / MH Budget Ratchet](../runbooks/mc-incident-response.md#scenario-20-meeting-teardown-failing--mh-budget-ratchet)

**PromQL**:
```promql
sum(increase(mc_media_push_quiesce_total{outcome="timed_out"}[30m])) > 0
```
`for: 0m`

#### MCTeardownFenceBackstop

**Severity**: Warning
**Condition**: Any `mc_media_teardown_fence_backstop_total` in 30 minutes: a teardown task hung or died, and the controller's per-meeting fence was lifted by deadline.
**Impact**: Rejoins of that meeting were held until the deadline, and one handler may hold a leaked registration.
**Runbook**: [Scenario 20: Meeting Teardown Failing / MH Budget Ratchet](../runbooks/mc-incident-response.md#scenario-20-meeting-teardown-failing--mh-budget-ratchet)

**PromQL**:
```promql
sum(increase(mc_media_teardown_fence_backstop_total[30m])) > 0
```
`for: 0m`

#### MCNotifyMeetingEndedFailing

**Severity**: Warning
**Condition**: Any `mc_gc_notify_meeting_ended_total{status="error"}` in 15 minutes. The notify is at-most-once, so an error is final.
**Impact**: Joins to the affected meeting id fail with `meeting_not_found` until MC restarts or is marked unhealthy.
**Runbook**: [Scenario 20: Meeting Teardown Failing / MH Budget Ratchet](../runbooks/mc-incident-response.md#scenario-20-meeting-teardown-failing--mh-budget-ratchet)

**PromQL**:
```promql
sum(increase(mc_gc_notify_meeting_ended_total{status="error"}[15m])) > 0
```
`for: 0m`

#### MCMeetingEndedNotificationsDropped

**Severity**: Warning
**Condition**: Any `mc_gc_meeting_ended_notifications_dropped_total` in 15 minutes: MC's notify queue was full.
**Impact**: GC keeps each affected meeting's assignment live, and joins to those meeting ids fail until MC restarts.
**Runbook**: [Scenario 20: Meeting Teardown Failing / MH Budget Ratchet](../runbooks/mc-incident-response.md#scenario-20-meeting-teardown-failing--mh-budget-ratchet)

**PromQL**:
```promql
sum(increase(mc_gc_meeting_ended_notifications_dropped_total[15m])) > 0
```
`for: 0m`

### Info Alerts (Join Flow)

#### MCHighJoinLatency

**Severity**: Info
**Condition**: Session join p95 latency >2s for >5 minutes (successful joins only)
**Threshold Rationale**: MC SLO is p99 <500ms for message processing. The join flow is end-to-end (WebTransport accept to JoinResponse) and includes JWT validation and actor processing, so a 2s p95 threshold serves as a leading indicator. There is **no** aggregate latency page alert: the processing SLO has no alert rule today, so do not read this threshold as backed up by something else. (`MCHighLatency` was cited here and in `MCHighJoinLatency`'s own annotation and has never existed.)
**Impact**: Slow meeting join experience
**Runbook**: [Scenario 5: High Latency](../runbooks/mc-incident-response.md#scenario-5-high-latency)

**PromQL**:
```promql
histogram_quantile(0.95,
  sum by(le) (rate(mc_session_join_duration_seconds_bucket{status="success"}[5m]))
) > 2.0
```
`for: 5m`

**Response**:
1. Check "Session Join Latency (P50/P95/P99)" panel in MC Overview dashboard
2. Check JWT validation latency (may be slow JWKS fetch)
3. Check actor mailbox depth (may be backpressure)
4. Check WebTransport session setup time
5. Check MC pod resource utilization (CPU, memory)

### Info Alerts (Media Keys)

#### MCKekEpochResetOnSenderIdExhaustion

**Severity**: Info
**Condition**: A meeting exhausted its sender-id namespace and MC performed an immediate KEK-epoch reset (`trigger="sender_space_exhausted"`), any occurrence in 15 minutes.
**Impact**: None to availability: admission succeeded and the meeting is recovering on its own. It is a security-relevant signal, because one authenticated participant can drive it cheaply.
**Runbook**: [Scenario 17: KEK Rotation Storm / Flapping Participant](../runbooks/mc-incident-response.md#scenario-17-kek-rotation-storm--flapping-participant)

**PromQL**:
```promql
increase(mc_meeting_kek_generated_total{trigger="sender_space_exhausted"}[15m]) > 0
```
`for: 0m`

**This REPLACES the retired `MCSenderIdSpaceExhausted`.** A responder who remembers the old warning ("NOT SELF-CLEARING") will want to end the meeting. **Do not.** Exhaustion now triggers a new KEK and a fresh namespace, and the join succeeds. The old rule's `sender_id_space_exhausted` join-failure label is never emitted again, so that rule would be permanently silent. **Deploy coupling**: rule files self-roll with the Prometheus ConfigMap and the MC image does not. The retirement must land with or after the task-9 MC image and be restored with or before any image rollback (`docs/runbooks/mc-deployment.md` §Coordination).

### Deliberate absences (MC)

- **Server mute has no alert, by decision** (story 2 task 12). A non-zero mute rate is healthy moderation, and the refusal-side values are client-inflatable. The inverse condition, "a mute was applied but the muted sender is still heard", would need a per-meeting correlation that ADR-0036 §11 bars. Coverage is the browser env-test S3 (`packages/web-app/e2e/server-mute.spec.ts`), MC runbook Scenario 21 (Server Mute Not Enforced), and MH runbook Scenario 19 (Server Mute Not Taking Effect At Ingress), and the ADR-0031 demonstration is to apply a mute and watch **MH's** `mh_media_frames_dropped_total{reason="server_muted"}` move, not only MC's counter. The decision is recorded at `mc-alerts.yaml` above `MCEndMeetingOwnershipRejected`; it is cited here, not restated.

---

## Media Handler Alerts

**Status**: ✅ Exists — **partially inventoried here**
**File**: `infra/docker/prometheus/rules/mh-alerts.yaml`

**This section inventories `MHMediaEgressQueueOverflowRate`, `MHMediaEgressEdgeHeadroomLow` and
`MHMediaEgressBudgetExhausted`, below. Every other alert in `mh-alerts.yaml` ships uninventoried.** Read the rules file directly, and
**do not treat the absence of an entry below as the absence of an alert** — that is the inference a
reader naturally makes here, and it is wrong.

Inventoried alerts are **named, never counted.** A count would be a second encoding of
`mh-alerts.yaml` with nothing behind it, stale on the next rule added; this section already carries
two struck entries from that failure. A name degrades differently: an alert name that stops existing
is greppable, a count that stops being right is invisible.

The entries are inventoried because they **landed with their alerts** (ADR-0036 story 1; story 2 tasks 16 and 18), so the
byte-identical-PromQL discipline was applied at authoring time rather than reconstructed. The rest
are deliberately **not retro-filled** — backfilling is tracked in `docs/TODO.md`. Note what the
`inventory_expr_drift` guard does and does not cover: it holds byte-identity for whatever *is*
inventoried, so a backfilled entry is checked from the moment it lands, but it does **not** require
that every rule have an entry. Nothing mechanical will tell you this section is incomplete. This
paragraph is that signal.

The rules file also records two **deliberate omissions** in its header comments — no burn-rate
page/warning pair, and no RegisterMeeting-apply alert — both gated on the MH SLO **target**, which is
unratified and lands in story 8. See `docs/observability/slos.md`, which is authoritative for that
rule; the rules-file comment is the copy.

#### MHMediaEgressQueueOverflowRate

**Severity**: Warning
**Condition**: >1% of egress datagram attempts dropped on MH's bounded application egress queue, for 5 minutes
**Impact**: Audible loss for affected subscribers. Audio is one frame per QUIC datagram with no retransmission, so a dropped datagram is a dropped frame. MH readiness, handshake latency and connection-accept rate all read healthy throughout.
**Runbook**: [Scenario 17: Media Datagram Drop](../runbooks/mh-incident-response.md#scenario-17-media-datagram-drop)

**PromQL**:
```promql
(
  sum(rate(mh_media_frames_dropped_total{direction="egress",reason="egress_queue_overflow"}[5m]))
  /
  (
    sum(rate(mh_media_frames_forwarded_total{direction="egress"}[5m]))
  + sum(rate(mh_media_frames_dropped_total{direction="egress"}[5m]))
  )
) > 0.01
and
(
  sum(rate(mh_media_frames_forwarded_total{direction="egress"}[5m]))
+ sum(rate(mh_media_frames_dropped_total{direction="egress"}[5m]))
) > 0
```
`for: 5m`

**Response**:
1. Break the drop counter down by `reason` **before** concluding back-pressure. Only `egress_queue_overflow` is what this rule measures.
2. `connection_closed` is routine — every participant leaves every meeting. `no_subscriber` is counted **once per frame**, not once per (frame × subscriber); when it is routine and when sustained is a fault is read from `docs/observability/metrics/mh-service.md` §`mh_media_frames_dropped_total` (the `no_subscriber` row), not restated here. **Do not widen the selector to include either**; that is the false fire the restriction exists to prevent, and it is why this alert is named for its selector rather than for the runbook scenario.
3. Sustained overflow means a subscriber the queue cannot drain into fast enough. There is no per-stream lever in this build — the bound is a startup-validated transport parameter. Escalate to `media-handler` with the reason breakdown and the affected pod.
4. **Nothing may rest on `mh_media_egress_queue_depth` alone** — one process-wide, last-writer-wins gauge fed by N per-subscriber queues, with a scrape interval orders of magnitude longer than the queue's fill-and-drain time. Trend input only.

> **This is an application queue bound, not a bandwidth budget.** It is ADR-0036 §1's transport-parameter bound, sized to trip before quinn's transport ceiling so the drop is countable in our code rather than discarded silently inside the library. Since story 2 task 8 the egress-budget chain exists, but it is an **admission-time** control: it refuses a forwarding policy at `RegisterMeeting` (`mh_media_stream_admission_total`, `mh_media_stream_admission_rejection_ratio` against its threshold gauge) and never sheds a datagram, so it cannot move this counter and must not be read into it. The ordering is enforced at startup as `ConfigError::EgressQueueDoesNotBindFirst`, which is what stops this alert becoming dead by construction.
>
> **The client-side send drop is not covered by this rule and cannot be** — it happens in the sender, and MH structurally cannot observe it (ADR-0036 §11). A flat counter here is not evidence that frames are arriving. See the runbook's client-side ladder.

#### MHMediaEgressEdgeHeadroomLow

**Severity**: Warning
**Condition**: `mh_media_egress_edges` above an alert-owned headroom fraction of `mh_media_egress_stream_ceiling`, for 10 minutes. The ceiling is the admission bound that binds first. `mh_media_egress_edges_limit` (`MH_MAX_TOTAL_EGRESS_EDGES`) is a backstop that MH refuses to start below the ceiling, so this denominator covers both. No bound is ever a literal.
**Impact**: New participants on the handler will soon be refused at admission and get silence, while everyone already talking keeps talking and every liveness signal reads healthy.
**Runbook**: [Scenario 20: Meeting Teardown Failing / MH Budget Ratchet](../runbooks/mc-incident-response.md#scenario-20-meeting-teardown-failing--mh-budget-ratchet)

**PromQL**:
```promql
mh_media_egress_edges / mh_media_egress_stream_ceiling > 0.8
```
`for: 10m`

**Fork LEAK vs LOAD first.** LEAK means edges climb with uptime while `mh_media_meeting_teardowns_total{outcome="released"}` stays flat. That is a reclamation regression, a true positive on a long-lived pod: never tune the fraction up to silence it. LOAD means edges track `mh_media_registered_meetings` and demand, and the remedy is capacity or placement. This is the **early warning**; `MHMediaEgressBudgetExhausted`, below, is the rejection-side failure. The fraction is **alert-owned and unratified**: it has no config home, so it duplicates no configured value (the derivation is in the rule's ADR-0031 block). **Demonstration**: lower the egress budget so the derived ceiling falls below a demo meeting's edges, and watch the ratio cross (lowering the edge limit instead makes MH refuse to start); confirm both gauges are present per instance with identical label sets.

#### MHMediaEgressBudgetExhausted

**Severity**: Warning
**Condition**: The windowed admission rejection ratio is above its configured threshold, gauge against gauge, for 10 minutes. The threshold is `MH_EGRESS_REJECTION_RATIO_THRESHOLD`, which reaches PromQL only through `mh_media_stream_admission_rejection_ratio_threshold`. There is no arithmetic and no literal.
**Impact**: New participants on the handler hear nobody and are heard by nobody, while existing participants and every liveness signal read healthy.
**Runbook**: [Scenario 18: Media Egress Budget Exhaustion](../runbooks/mh-incident-response.md#scenario-18-media-egress-budget-exhaustion)

**PromQL**:
```promql
mh_media_stream_admission_rejection_ratio > mh_media_stream_admission_rejection_ratio_threshold
```
`for: 10m`

**Sustained rejection while placement keeps targeting the pod is a triage step, not a second alert.** GC removes a handler at its `max_streams` from NEW-meeting placement only. An already-assigned meeting keeps targeting it (R-6), so refusals continue. The runbook reads GC's view at that step.

**Its leading indicator is scoped, not general.** The statement's one home is MH Scenario 18, "Is there a leading indicator?".

**Shape notes** (the derivations are in the rule's ADR-0031 block):
- Label-set identity: both gauges carry exactly `key_custody` plus the scrape labels. A label added to one of them silently empties the comparison.
- There is no zero-denominator guard, by construction: an empty window publishes 0.0, and all-rejected publishes 1.0.
- There is no evidence-floor conjunct. A lone rejection ages out of the 5-minute window long before `for: 10m` elapses. The session actor's WARN applies a minimum decision count and may disagree at low volume. The rule is authoritative.
- It is not a burn-rate alert and makes no error-budget claim. The MH SLO target is unratified (`docs/observability/slos.md`, story 8).

**Demonstration** (inject blast radius in the rule's ADR-0031 block: both MH pods roll on the lower and again on the revert, and the FIRE half needs sustained refused registrations for the full 10 minutes): on Kind only, lower the deployed `MH_EGRESS_BUDGET_BPS` so the derived ceiling sits below what a demo meeting installs, then keep joining participants. The ratio gauge should cross the threshold gauge. Then confirm the selector matches a real container: both gauges present per `mh` instance with identical label sets. On a live pod, lowering the budget is a new-meeting outage (`docs/runbooks/mh-deployment.md` §Operator levers).

### Deliberate absences (MH)

These are recorded so that the absence reads as a decision, not an omission:
1. No burn-rate pair (gated on the unratified MH SLO target; `docs/observability/slos.md`).
2. No RegisterMeeting-apply alert (same gate).
3. **No teardown-never-arrives rule — recorded here, but an OPEN obligation, not a design decision.** An edge-holding leaked meeting is caught by `MHMediaEgressEdgeHeadroomLow`'s LEAK fork. A ZERO-edge leaked registration has no alert: its first symptom is `rejected_meeting_cap` at the cap. The instrument exists, and only authorship remains. Tracked in `docs/TODO.md` §Observability Debt, "No leading indicator for registered-meeting exhaustion", with owners and trigger. *(Corrected 2026-09-30, story 2 task 18: this entry previously assigned the rule to task 18, which never carried it.)*
4. **`mh_media_egress_stream_ceiling_recommended_min` has no alert, by design** (story 2 task 16). It is ADVISORY: the ceiling a demo of the configured size needs, published so an operator can compare it with `mh_media_egress_stream_ceiling`. Nothing enforces it, which is exactly why it is spelled `_recommended_min` and not `_threshold` (`docs/observability/label-taxonomy.md` §R4 suffix rule: a threshold spelling tells a responder that something automatic is watching).

**Remaining unbuilt candidates** (aspirational — thresholds are unratified):
- `MHHighPacketLoss` - Packet loss >1%
- `MHForwardingQueueBacklog` - Queue depth high

> `MHDown` and `MHHighCPU` were also removed from this list on 2026-09-02: **both already exist** in
> `mh-alerts.yaml`. Listing a shipped alert as unbuilt is the same dangling-entry failure as the
> struck `MHHighJitter` above — a candidate list reads as a work queue — and it contradicted the
> then-current "documents none of it" note directly above (since reframed as a partial inventory,
> when `MHMediaEgressQueueOverflowRate` landed; the contradiction it names was real against the
> wording of the day).

> **Two entries removed here on 2026-09-02**, because this commit made them dangling:
> - **`MHHighJitter` — "Jitter p99 >20ms"**: the MH audio-jitter objective is **struck as
>   unmeasurable** (ADR-0011 amendment 2026-09-02; ADR-0036 amendments table). MH forwards and does
>   not buffer; perceived jitter is a client-side jitter-buffer property. No `*jitter*` metric is
>   emitted by any service, so this alert could never fire. **Deleted rather than repointed** — an
>   entry in a planned-alert list reads as a work queue, and a pointer would instruct someone to
>   build an alert against a metric that will never exist.
> - **`MHHighAudioLatency` — "Audio forwarding p99 >30ms"**: the `>30ms` figure predates the
>   forwarding objective's measurement point, which was redefined by the same amendment to
>   ingress-read-complete → egress-enqueued. A threshold and a measurement point are a pair, so that
>   number is **not ratified against the current SLI**. The alert is legitimate future work, but it
>   cannot carry a number until story 8 ratifies one; see `docs/observability/slos.md`. Left out of
>   the candidate list above rather than restated with a stale threshold.

---

## Client SDK Alerts

**File**: `infra/docker/prometheus/rules/client-alerts.yaml` (new in story 2 task 16; ADR-0031 owner `client`)

The placement rule: a client-origin series whose **remedy is the SDK** lives here. A client-origin series whose remedy is a server lives in that server's file, with the `MCClient` prefix (`MCMediaMissingKeyMaterial`, `MCClientKekConflictingKey`, `MCClientRosterKeyRebind`). See `alert-conventions.md` §Alert naming.

#### ClientKekRetentionViolation

**Severity**: Warning
**Condition**: Any client has counted `dt_client_media_kek_retention_violations_total`: an SDK install path tried to hold more than the current KEK generation plus one previous (a COUNT bound, not the W-derived time bound). `RetentionGuard` failed safe, zeroizing and dropping the excess in the same call.
**Impact**: An SDK regression in key-lifetime code. The fail-safe held, so no superseded key was retained past the install, but the path that should have prevented the over-retention is broken, and a further regression could remove the fail-safe with it.
**Runbook**: [Tripwire: KEK retention violation](../runbooks/mc-incident-response.md#tripwire-kek-retention-violation)

**PromQL**:
```promql
sum(dt_client_media_kek_retention_violations_total) > 0
```
`for: 1m`

**Presence-shaped, not `increase()`**: the counter is absent until the first increment and appears already at ≥ 1, so `increase()` would never see the one event that matters. `dt-guard client-metrics-export` (`tripwire_rate_wrapped`) fails the build on an `increase()`/`rate()` wrapping; that is a shape guard, and the appear-at-1 firing claim is unverified by execution. **It clears when the collector's exporter drops the idle series** (`metric_expiration`, `infra/services/otel-collector/collector.yaml`), not when the condition stops. A second occurrence while firing shows only in the raw counter. Idle tabs do not hold it up. **Every known non-zero cause is listed at the rule**. A new legitimate cause is added there; the rule is never retired or silenced for it.

**A zero-forever tripwire against a future SDK refactor, not a live-condition detector.** The invariant is unit-tested, and there is no known legitimate cause. The remedy is an SDK rollback, not an operator action.

### Deliberate absences (client receive path)

`MCMediaMissingKeyMaterial` covers four key-delivery reject reasons. `unwrap_failed` joined it in story 2 task 16, partly because nothing else watched it. That same argument applies to the other receive-path drop reasons, so their absence is recorded here as a decision rather than inherited. Each is visible on **client-media panel 3** (Frame Drops by Reason), and none is alerted:

- **`signature_invalid`, `decrypt_failed`, `replay_detected`** (receive-path crypto). Any authenticated roster member can drive each of them at every receiver by authoring the frames, and transit corruption also lands on `signature_invalid`. A fleet-ratio threshold on them would be attacker-controllable. No baseline exists to set one, and a per-sender split, the dimension that would make them actionable, is barred by ADR-0036 §11. Their triage owners differ, as recorded in the guard's `NOT_KEY_DELIVERY` reasons: `security` for forgery, `client` for sender key-schedule defects.
- **The eight codec structural rejects and `no_transmit_key`.** These are byte-determined, so a malformed or non-conforming sender produces them, and so can anyone authoring frames. A codec regression shows as a step on panel 3 at a release boundary. `no_transmit_key` is a protocol violation, and the `protocol`/`client` code owns it.
- **`sender_not_assigned`** is misrouting, whose remedy is MC placement and MH forwarding. It is covered on the server side by MC's slot and send-target metrics, not by a client alert.

**Whether any of these should alert is an open decision, not a closed one.** It is tracked in `docs/TODO.md` §Observability Debt ("Receive-path crypto drop reasons have no alert"), with the trigger being the first observed baseline.

---

## Telemetry Pipeline Alerts (OTel collector) — SPECIFIED, NOT LOADED

> ## NONE OF THE THREE BELOW EXISTS AS A RULE. NOTHING FIRES ON ANY OF THESE CONDITIONS TODAY.
>
> These are **specifications for rules to be written**, not inventory entries.
> They are in this file because the definitions and their derivations are
> `observability`'s and this is where that reasoning lives; the rule files are
> `operations`-owned (ADR-0011 §Documentation Ownership) and no rule file under
> `infra/docker/prometheus/rules/` contains any of these names. **Do not read
> this section as coverage** — that is the precise failure this file's own
> inventory guard exists to prevent, whose message reads: *"An inventory entry
> reads as coverage; a responder searching for what covers a failure mode would
> find this and stop looking. Delete it, or land the rule."*
>
> **The heading level below is `###` DELIBERATELY, and must not be promoted to
> `####` until the rules land.** `dt-guard alert-rules` parses `#### <AlertName>`
> as an inventory entry and requires a matching rule plus a byte-identical
> `promql` block; promoting these headings before the rules exist turns the guard
> red, which is correct behaviour and the reason the distinction is mechanical
> rather than a matter of tone. **When the rules land, promote the headings in the
> same commit** — that is what moves them from specification to inventory and puts
> them under the guard.
>
> Tracked in `docs/TODO.md` §Observability Debt.
>
> This block was added after review found that the section, as first written, was
> indistinguishable from the loaded entries above it *and* sat below the guard's
> detection threshold — so the guard reported clean having examined nothing. Both
> halves are recorded because the second is the more dangerous one.

**Why these three are a SET and must not be pruned as redundant.** The collector's
metrics path is composed entirely of allowlists — a metric-name filter, a temporality
filter, `keep_keys`, `max_streams`, `memory_limiter` — and **every allowlist fails by
making something absent**. On the client-media board absence is already ambiguous with
"no browser connected", so a pipeline built only of fail-quiet controls has no way to
report its own breakage. These three give it one: **the band watches the output, the
drop counters watch the machine, and the anchor watches the drop counters.** Remove any
one and the remaining two lose the property that motivated them.

Rule files are `operations`-owned (ADR-0011 §Documentation Ownership); the definitions
and reasoning below are `observability`'s.

### ClientSeriesCountOutOfBand

**Severity**: Warning
**Signal**: `count({job="otel-collector"})` against a derived band.

**A BAND, NOT A CEILING — deliberately.** A ceiling is structurally blind to the failure
this is most likely to see: a label key missing from `keep_keys` makes several series
**collapse into one**, so the count goes *down*. Inflation and collapse are both real and
they move in opposite directions.

Derivation constraints, all three of which are load-bearing and none re-derivable from the
rule file:

- **The floor derives from the UNCONDITIONALLY-EMITTED series only**, never from the label
  cross-product. OTel JS creates a series lazily per label set, so a healthy cluster with
  zero drops legitimately has no drop-family series at all; a cross-product floor fires
  hardest when everything works.
- **The floor is conditioned on the pipe being live** via the *send* counter, not the
  receive counter. A solo browser with nobody listening legitimately receives nothing, so
  a receive-keyed condition would disable the floor in exactly the single-browser case
  someone is most likely to be looking at.
- **The ceiling tolerates at least one full `metric_expiration` of tenant overlap.**
  `scripts/layer7.sh` provisions a fresh per-run organization and GC stamps its UUID as
  `org_id`, so at every CI run boundary the previous run's series are still exported and
  the count legitimately sits at a multiple of the single-tenant expectation. **This is a
  dev-cluster artefact of per-run provisioning, not a production shape** — in a real
  deployment `org_id` is stable and no overlap exists. Do not carry the headroom into
  production, and do not size for production and then get paged by CI.

**What it does NOT catch**: a *single* metric losing *one* label key. Two series becoming
one against a fleet-sized band is noise. That residual is tracked in `docs/TODO.md`
(client metric label keys are unguarded) and is not closed by this alert.

### CollectorDroppingTelemetry

**Severity**: Warning
**Signal**: the healthy-**zero** `otelcol_*` families at `> 0` — receiver refused, processor
refused (the `memory_limiter` backpressure signal), exporter send-failed, and the
`delta_to_cumulative` stream-limit counter.

**NEVER alert on the filter processor's own drop count.** Discarding every metric outside
the name allowlist is the filter doing its job on every batch, so that family is
healthy-**nonzero** at all times and an alert on it fires continuously on a working
collector. It belongs on a panel, not in a rule. (It also carries no `processor` label, so
it cannot distinguish the two filters even if you wanted it to.)

**The partition is a MEASURED FACT about a specific image, not a derivation** — read off
the running pod, recorded with its tag and digest in the devloop record. Nothing recomputes
it, and the event that invalidates it is an image bump. Two live traps confirming that: the
exposed family keeps the **old** `deltatocumulative` spelling even though the processor is
configured as `delta_to_cumulative`; and every metric-flavoured family is **absent until the
first datapoint flows**, so absence here means "nothing has failed *or* nothing has run" —
which is why the anchor below exists. Re-read the names at every bump; the pre-upgrade
checklist in the collector runbook is the human control and inherits every weakness of that
class.

### CollectorSelfTelemetryAbsent

**Severity**: Warning
**Signal**: `absent()` on a process gauge from `job="otel-collector-telemetry"` — a family
that is present-at-idle from process start and does not depend on any pipeline activity.

**This is the positive control for the rule above, and without it that rule inherits the
defect it was added to fix.** If a family is renamed at the next image bump,
`CollectorDroppingTelemetry` silently stops matching and goes quiet — two rules added to
detect silent drops, failing silently. This anchor turns a wholesale rename, a format change
or a broken scrape **red**.

**Scope, stated so the anchor does not read as full coverage**: it catches wholesale loss,
**not** an individually renamed family. That remains the pre-upgrade checklist's job.

**`for:` differs from the capacity gauge over the same series, and that is why there are two
rules rather than one.** The anchor needs a `for:` long enough to ride out a collector
restart or a missed scrape without flapping; a capacity reading wants none. Do not collapse
them as duplicates.

**Capacity note**: the collector runs Guaranteed QoS (`requests.memory == limits.memory`).
A utilisation ratio alerts against **`limits.memory`** — the limit is what OOMKills, the
request only affects scheduling — and the comment must name which, because the two were
different values earlier in this design's history and a bare figure is ambiguous between
them. Record what any measured RSS figure was taken against; the same number reads as
comfortable or alarming depending on a limit that has already changed twice.

---

## Alert Configuration Standards

> **Moved.** Alert authoring standards are now maintained in
> [`docs/observability/alert-conventions.md`](./alert-conventions.md), which
> is machine-enforced by `scripts/guards/simple/validate-alert-rules.sh` per
> ADR-0031. The subsections below are thin pointers into the normative doc.
> `alerts.md` remains the per-service alert catalog.

### 1. Alert Naming Convention

**Format**: `{SERVICE}{COMPONENT}{CONDITION}` (e.g., `GCDown`, `GCHighLatency`,
`ACKeyRotationFailed`). See [alert-conventions.md](./alert-conventions.md) for
the full naming convention.

### 2. Required Fields

See [alert-conventions.md §annotation-hygiene](./alert-conventions.md#annotation-hygiene)
for the required-annotation table (`summary`, `description`, `runbook_url`
required; `impact` recommended) and the `labels.severity` / `service` /
`component` requirements.

Note: `labels.severity` must be in `{page, warning, info}` per ADR-0031.

### 3. Threshold Selection

See [alert-conventions.md §severity-taxonomy](./alert-conventions.md#severity-taxonomy)
for the page / warning / info calibration anchors with user-impact framing.

### 4. Duration (`for` clause)

See [alert-conventions.md §for-conventions](./alert-conventions.md#for-conventions).
Floor is 30s (guard-enforced). Typical windows: 30s–1m (critical path
outages), 5m–10m (steady-state thresholds), 1h+ (long-window burn-rate).
Match `for:` to the `rate()` window where applicable.

### 5. Runbook Requirement

See [alert-conventions.md §annotation-hygiene](./alert-conventions.md#annotation-hygiene)
for the `runbook_url` format rule. Must be repo-relative under
`docs/runbooks/` (guard-enforced). Every runbook should cover:
symptom, impact, diagnosis, mitigation, escalation.

---

## Alert Routing

**Configuration**: Prometheus Alertmanager (`infra/docker/prometheus/alertmanager.yml`)

### Critical Alerts

```yaml
route:
  routes:
    - match:
        severity: critical
      receiver: pagerduty-critical
      group_wait: 10s
      group_interval: 5m
      repeat_interval: 1h
      continue: true
    - match:
        severity: critical
      receiver: slack-incidents
```

**Channels**:
- PagerDuty (immediate page)
- Slack #incidents

**Escalation**: 15min → on-call lead

### Warning Alerts

```yaml
route:
  routes:
    - match:
        severity: warning
      receiver: slack-alerts
      group_wait: 30s
      group_interval: 10m
      repeat_interval: 4h
```

**Channels**:
- Slack #alerts

**Escalation**: 1h → service owner

---

## Alert Fatigue Prevention

Per ADR-0011, the following controls prevent alert fatigue:

### 1. Deduplication

- Group similar alerts (same service, same issue)
- Deduplication window: 5 minutes

### 2. Severity Bumping

- Warning → Critical if firing >30 minutes
- Implemented in Alertmanager routing

### 3. Volume Limiting

- Max 20 alerts/hour per service
- If exceeded: Suppress and page SRE lead
- Indicates widespread issue requiring urgent attention

### 4. Alert Quality Metrics

Track alert quality:
```promql
# Alert precision (true positive rate)
alerts_fired_total{resolution="true_positive"} / alerts_fired_total

# Time to resolution
histogram_quantile(0.95, alert_resolution_duration_seconds)
```

---

## Alert Testing

Before deploying alerts, test:

1. **PromQL Validation**: Test query returns expected results
   ```bash
   # Test query against Prometheus
   curl -G 'http://localhost:9090/api/v1/query' \
     --data-urlencode 'query=up{job="gc-service"} == 0'
   ```

2. **Alert Simulation**: Use `amtool` to simulate alerts
   ```bash
   amtool alert add alertname=GCDown \
     severity=critical \
     service=gc-service
   ```

3. **Runbook Validation**: Verify runbook exists and is accessible
   ```bash
   curl -I https://github.com/yourorg/dark_tower/blob/main/docs/runbooks/gc-high-latency.md
   ```

4. **Cardinality Check**: Ensure labels don't explode cardinality
   ```promql
   # Count unique label combinations
   count by(__name__, job, severity) (ALERTS)
   ```

---

## Alert Ownership

| Alert Group | Owner | Reviewer |
|-------------|-------|----------|
| GC Critical | Observability | GC Team + Operations |
| GC Warning | Observability | GC Team |
| AC Critical | Observability | AC Team + Operations |
| MC Critical | Observability | MC Team + Operations |
| MC Warning (Join) | Observability | MC Team |
| MC Info (Join) | Observability | MC Team |
| MH Critical | Observability | MH Team + Operations |

**Update Frequency**: Review quarterly or after major SLO changes.

---

**Maintained By**: Observability Specialist + Operations Team
**Related Documents**:
- [ADR-0011: Observability Framework](../decisions/adr-0011-observability-framework.md)
- [Runbook Index](./runbooks.md)
- [Dashboard Catalog](./dashboards.md)
- [SLO Definitions](./slos.md) — authoritative for SLO targets (ADR-0011:40)
