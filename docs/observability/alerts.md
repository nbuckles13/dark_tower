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
- **No data is healthy**, not a broken exporter or a bad scrape config. This series is expected to be permanently absent in normal operation.
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

Existing MC `severity: page` alerts: `MCDown`, `MCActorPanic`, `MCHighMailboxDepthCritical`, `MCMediaConnectionAllFailed`, `MCMediaGenerationDivergence`. That is the complete set — `mc-alerts.yaml` is the source of truth and this list is a convenience copy; verify against it rather than citing this line. (`MCHighLatency`, `MCHighMessageDropRate` and `MCGCHeartbeatFailure` appeared here and have never existed in any rules file.)

**That list is complete; the entries below it are not.** Only `MCMediaGenerationDivergence` has a full inventory entry — it landed with its alert, so byte-identical PromQL was applied at authoring time. The other four are **named here and uninventoried**, exactly as in §Media Handler Alerts. The `inventory_expr_drift` guard checks byte-identity for what is inventoried; it does not require that every rule have an entry, so nothing mechanical will report this section incomplete.

#### MCMediaGenerationDivergence

**Severity**: Page
**Condition**: MH echoes an applied media-policy generation that differs from the one MC sent, or echoes none at all — any occurrence in 15 minutes
**Impact**: MH is forwarding under stale or absent policy while every liveness signal reads green. ADR-0036 §8 calls this a partial blackhole reporting healthy. Blast radius is per (meeting, handler).
**Runbook**: [Scenario 15: Media Generation Divergence](../runbooks/mc-incident-response.md#scenario-15-media-generation-divergence)

**PromQL**:
```promql
sum(increase(mc_media_policy_pushes_total{outcome!~"match|handler_id_mismatch"}[15m])) > 0
```
`for: 0m`

**Response**:
1. **Do not wait for convergence — it will not converge.** Of ADR-0036 §8's four re-fire triggers only *structural change* is implemented in this build, so a diverged meeting stays media-dark until a rejoin.
2. Split on the `outcome` label first — the values have different first moves. `generation_mismatch` means MH's apply ran and did not take effect (open MH, read `mh_media_policy_applies_total{outcome}`); `no_applied_generation` means MH echoed nothing (confirm the MH image first — a rollout skew is the cheap explanation and it clears itself); `transport_mode_mismatch` is ADR-0036 §8's separate two-ends-must-agree echo failing, a different remedy reached from the same runbook.
3. Read `mc_media_generation_divergence` for the magnitude **second, never first**: it is last-write-wins at pod level, so a healthy push for an unrelated meeting erases a diverged reading, and a value of 0 is not evidence that nothing diverged.
4. Force a structural change on the affected meeting — with one participant that is a rejoin. This is the resolution, not a workaround.
5. **Do not restart MH.** It sheds every media session on the pod, recovers none of the already-dark ones, and destroys the evidence.

> **The selector is NEGATED, and that is the design, not a shorthand.** It reads "everything that is not a confirmed match and not the known-noisy diagnostic". `PolicyPushOutcome::ALL` is compile-checked in Rust, but **that compile error does not reach PromQL** — so under the negated form a sixth outcome added later pages and gets classified deliberately, while under a positive `outcome=~"a|b"` it would fall silently outside the alert. For a page alert whose subject is *a partial blackhole reporting healthy*, silence is the failure mode to defend against.
>
> `handler_id_mismatch` is the one exclusion beyond `match`: `MH_HANDLER_ID` is per-incarnation, so an ordinary MH restart produces it by construction and a bare `outcome != "match"` would page on every rollout. That exclusion is held at three sites — this rule, `docs/observability/metrics/mc-service.md`'s catalog entry, and `mc-deployment.md`'s post-deploy checklist — as **one decision with one revert trigger**, `2026-09-02-mh-stable-handler-id`; remove it in all three places together.

### Warning Alerts (Join Flow)

#### MCMediaMissingKeyMaterial

**Severity**: Warning
**Condition**: >5% of received media frames dropped for missing key material, sustained 15 minutes
**Impact**: Affected participants hear nothing while every server-side signal reads healthy — MH never opens a frame and structurally cannot observe either condition.
**Runbook**: [Scenario 16: Missing Key Material](../runbooks/mc-incident-response.md#scenario-16-missing-key-material)

> **THIS ALERT CANNOT FIRE TODAY, AND THAT IS NOT A THRESHOLD PROBLEM.** `dt_client_*` metrics reach no Prometheus in this deployment: the OTLP collector's metrics pipeline exports to `debug` (its own container log) and no Prometheus job scrapes the collector, so the series does not exist. **The absence of this alert firing is not evidence that key delivery is healthy.** The rule lands so that wiring the exporter is the single remaining step rather than a rule nobody wrote; the wiring is tracked in `docs/TODO.md` §Observability Debt. Stated here, adjacent to the PromQL, because every other entry in this file is written in the present tense and an unmarked entry would read as coverage.

**PromQL**:
```promql
(
  sum(rate(dt_client_media_frames_dropped_total{reason=~"no_kek_for_generation|no_roster_entry"}[5m]))
  /
  sum(rate(dt_client_media_frames_received_total[5m]))
) > 0.05
and
sum(rate(dt_client_media_frames_received_total[5m])) > 0
```
`for: 15m`

**Response**:
1. **Split on the `reason` label first** — the two arms have different remedies and are not equally instrumented.
2. `no_roster_entry`: no usable identity key for the sender, including the case where MC published an empty key. Signature verification cannot run, so attribution is failing and not merely decryption. Corroborate server-side with `mc_join_identity_key_presence_total{presence}` — a rising `absent` ratio answers "are clients publishing keys?" directly. A *malformed* key is a different condition and lands on `mc_session_join_failures_total{error_type="identity_key_invalid"}`.
3. `no_kek_for_generation`: no meeting KEK for the generation the frame's wrap announces. **This arm has no server-side counter and cannot have one** — `mc_meeting_kek_generated_total` increments unconditionally and its catalog entry states the inference is not computable even in principle. Absence of a signal here is not evidence the KEK is present.
4. Check the non-dump path first and completely: the per-join response-side condition (`meeting_kek` not exactly 32 bytes) and `dt_client_media_kek_updates_total{source="join_response"}`. Then read the runbook's dump gate before going further.
5. Both reasons are **expected transiently** at join and after a KEK rotation. The `for: 15m` window, not the threshold, is what separates the transient from the signal.

**Threshold provenance**: 5% is not SLO-derived and there is no observed baseline. At 20 ms/frame (50 frames/s) a 1–2 second join transient is well under 1% of a 5-minute window while a sustained delivery failure sits near 100%.

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
1. Check "Join Failures by Error Type" panel in MC Overview dashboard
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
2. Check "JWT Validations by Result" panel in MC Overview dashboard
3. If JWKS fetch failures -> check network connectivity to AC
4. If token expiry issues -> check clock skew between services
5. If sudden spike -> check for recent AC key rotation or config changes

---

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
1. Check "Session Join Latency P50/P95/P99" panel in MC Overview dashboard
2. Check JWT validation latency (may be slow JWKS fetch)
3. Check actor mailbox depth (may be backpressure)
4. Check WebTransport session setup time
5. Check MC pod resource utilization (CPU, memory)

---

## Media Handler Alerts

**Status**: ✅ Exists — **partially inventoried here**
**File**: `infra/docker/prometheus/rules/mh-alerts.yaml`

**This section inventories exactly one of that file's alerts — `MHMediaEgressQueueOverflowRate`,
below. Every other alert in `mh-alerts.yaml` ships uninventoried.** Read the rules file directly, and
**do not treat the absence of an entry below as the absence of an alert** — that is the inference a
reader naturally makes here, and it is wrong.

Inventoried alerts are **named, never counted.** A count would be a second encoding of
`mh-alerts.yaml` with nothing behind it, stale on the next rule added; this section already carries
two struck entries from that failure. A name degrades differently: an alert name that stops existing
is greppable, a count that stops being right is invisible.

The one entry is inventoried because it **landed with its alert** (ADR-0036 story 1), so the
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
2. `connection_closed` is routine — every participant leaves every meeting. `no_subscriber` is counted **once per frame**, not once per (frame × subscriber), so in a meeting with nobody subscribed it reads as 100% of egress off a single frame. **Do not widen the selector to include either**; that is the false fire the restriction exists to prevent, and it is why this alert is named for its selector rather than for the runbook scenario.
3. Sustained overflow means a subscriber the queue cannot drain into fast enough. There is no per-stream lever in this build — the bound is a startup-validated transport parameter. Escalate to `media-handler` with the reason breakdown and the affected pod.
4. **Nothing may rest on `mh_media_egress_queue_depth` alone** — one process-wide, last-writer-wins gauge fed by N per-subscriber queues, with a scrape interval orders of magnitude longer than the queue's fill-and-drain time. Trend input only.

> **This is an application queue bound, not a bandwidth budget.** It is ADR-0036 §1's transport-parameter bound, sized to trip before quinn's transport ceiling so the drop is countable in our code rather than discarded silently inside the library. There is no egress bandwidth budget, capacity gauge, stream ceiling or admission threshold in this build. The ordering is enforced at startup as `ConfigError::EgressQueueDoesNotBindFirst`, which is what stops this alert becoming dead by construction.
>
> **The client-side send drop is not covered by this rule and cannot be** — it happens in the sender, and MH structurally cannot observe it (ADR-0036 §11). A flat counter here is not evidence that frames are arriving. See the runbook's client-side ladder.

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

| Alert Group | Owner | Reviewer | Last Updated |
|-------------|-------|----------|--------------|
| GC Critical | Observability | GC Team + Operations | 2026-02-28 |
| GC Warning | Observability | GC Team | 2026-02-28 |
| AC Critical | Observability | AC Team + Operations | TBD |
| MC Critical | Observability | MC Team + Operations | 2026-03-27 |
| MC Warning (Join) | Observability | MC Team | 2026-03-27 |
| MC Info (Join) | Observability | MC Team | 2026-03-27 |
| MH Critical | Observability | MH Team + Operations | TBD |

**Update Frequency**: Review quarterly or after major SLO changes.

---

**Last Updated**: 2026-03-27
**Maintained By**: Observability Specialist + Operations Team
**Related Documents**:
- [ADR-0011: Observability Framework](../decisions/adr-0011-observability-framework.md)
- [Runbook Index](./runbooks.md)
- [Dashboard Catalog](./dashboards.md)
- [SLO Definitions](./slos.md) — authoritative for SLO targets (ADR-0011:40)
