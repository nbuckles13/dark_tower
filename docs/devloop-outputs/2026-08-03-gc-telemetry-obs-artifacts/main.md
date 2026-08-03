# Devloop Output: GC Telemetry Proxy & CORS Observability Artifacts

**Date**: 2026-08-03
**Task**: Ship GC server-side observability artifacts (alerts, dashboard row, metric catalog, runbook scenarios) for the telemetry proxy and CORS layer (R-51/R-52, Obs Task B)
**Specialist**: observability
**Mode**: Agent Teams (v2), full, HEADLESS (run-story task #16)
**Branch**: `feature/user-story-run-test`
**Duration**: TBD

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `0d3112fd604e58ce4222032f8df50feba637b367` |
| Branch | `feature/user-story-run-test` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` |
| Implementing Specialist | `observability` |
| Iteration | `1` |
| Security | `CLEAR` |
| Test | `RESOLVED-DEFERRED` |
| Observability | `RESOLVED-DEFERRED` |
| Code Quality | `RESOLVED-DEFERRED` |
| DRY | `RESOLVED-DEFERRED` |
| Operations | `RESOLVED-FIXED` |
| Semantic Guard | `CLEAR` |
| Paired GC Owner | `CLEAR (owner co-sign)` |

---

## Task Overview

### Objective

Ship the alerting/dashboard/docs surface for GC telemetry proxy + CORS metrics already emitted by story tasks #5 and #10:

1. `infra/docker/prometheus/rules/gc-alerts.yaml`: add `GCTelemetryProxyHighRejectionRate` (warn, rejection > 10% of ingest for 10m) and `GCTelemetryProxySilent` (page, `absent_over_time(gc_telemetry_ingest_total[15m])` gated by a 1h non-zero baseline guard). Each carries a `runbook_url` annotation pointing at the new incident sections.
2. `infra/grafana/dashboards/gc-overview.json`: new Telemetry Ingest row with 5 panels (ingest rate by status; p50/p99 ingest duration; payload size p99 by kind; rate-limit rejections by reason; CORS preflight rate by origin_class). Extend the existing dashboard — no new file.
3. `docs/observability/metrics/gc-service.md`: Telemetry Proxy Metrics + CORS Preflight Metrics catalog sections.
4. `docs/runbooks/gc-incident-response.md`: two scenario sections referenced by the alert `runbook_url` annotations.

Metrics in scope (already emitted): `gc_cors_preflight_total{origin_class, status}`, `gc_telemetry_ingest_total{status, payload_kind}`, `gc_telemetry_ingest_duration_seconds{status}` (p99 SLO < 200ms), `gc_telemetry_payload_bytes{payload_kind}`, `gc_telemetry_rate_limited_total{reason}`, `gc_telemetry_pii_attributes_dropped_total`.

Constraint: dt-guard `alert-rules-policy` and `dashboard-panels` checks run at Gate 2 — alert and panel shapes must stay compliant.

### Scope
- **Service(s)**: GC (observability surface only — no Rust code changes expected)
- **Schema**: No
- **Cross-cutting**: No (all target files are observability-owned surfaces)

### Debate Decision
NOT NEEDED — alert thresholds and panel shapes were fixed by the user story (R-51/R-52); no cross-service design decision required.

---

## Cross-Boundary Classification

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `infra/docker/prometheus/rules/gc-alerts.yaml` | Not mine — Domain-judgment (service-owned per ADR-0031; story Obs Task B hands authorship to observability). §6.3 routing decision recorded by @team-lead: @paired-global-controller added to the team for owner co-sign at Gate 1 + Gate 3 on these hunks | global-controller (co-sign: @paired-global-controller) |
| `infra/grafana/dashboards/gc-overview.json` | Not mine — Domain-judgment (same ADR-0031 service ownership; story-assigned). Same §6.3 routing: @paired-global-controller owner co-sign at Gate 1 + Gate 3 | global-controller (co-sign: @paired-global-controller) |
| `docs/observability/metrics/gc-service.md` | Mine (observability-owned metric catalog; edits are stale-forward-ref cleanup + alert cross-refs) | — |
| `docs/observability/alerts.md` | Mine (observability-owned alert inventory; see Planning §Scope widening — file not named in task but every existing GC alert has an entry here; skipping it creates SSoT drift) | — |
| `docs/runbooks/gc-incident-response.md` | Not mine — Domain-judgment (runbooks are operations-owned; task assigns the two scenario sections to this loop; @operations reviews content) | operations |
| `docs/devloop-outputs/2026-08-03-gc-telemetry-obs-artifacts/main.md` | Mine (devloop bookkeeping) | — |
| `docs/TODO.md` | Mine (tech-debt filing per Lead decision C — §Observability Debt entry for the `normalize_endpoint` follow-up; row added by @team-lead at Gate 2 after the scope guard flagged the file missing from this table) | — |

No Rust files are touched (task constraint: metrics already emitted by tasks #5/#10).

**Commit trailer requirement (@operations Gate 1 #3)**: the implementation commit must record the cross-boundary approvals, not just this prose — `Approved-Cross-Boundary: global-controller` (gc-alerts.yaml + gc-overview.json hunks, per @paired-global-controller Gate 1/Gate 3 co-sign) and `Approved-Cross-Boundary: operations` (runbook hunks, authorized by @operations Gate 3 sign-off).

---

## Planning

### Current-state findings (deltas vs task text)

1. **Dashboard row already exists in minimal form.** `gc-overview.json` already has row id 53 "Telemetry Ingest (minimal — enriched in task #16)" with panels 54–58 (ingest by status, duration p99, payload size p99, rate-limit by reason, PII drops) and row 59 "CORS Preflight (R-1 / R-52)" with panel 60. This task is the named "task #16" — the work is **enrichment**, not net-new: rename row 53, add p50 to panel 55, make panel 56 per-`payload_kind`. Panels 54, 57, 60 already match their specs. Proposal: keep panel 60 in its existing CORS row rather than relocating it into the Telemetry row — the required 5-panel set is satisfied across the two existing rows and relocation churns task #5's landed structure for no operator value.
2. **Catalog sections already exist.** "Telemetry Proxy Metrics (R-2 / R-51)" and "CORS Metrics (R-1 / R-52)" landed with tasks #5/#10 (the dashboard-panels guard requires catalog presence). Remaining work: update the two stale "dashboards + alerts are added by task #16" forward-notes to reference the now-real alerts/panels, and add alert cross-references.
3. **Dead selector in catalog honesty note.** The catalog tells alert authors the size-rejection class is a union with `gc_http_requests_total{endpoint=~".*/telemetry/.*",status_code="413"}`. That selector can match nothing: `normalize_endpoint()` (`crates/gc-service/src/observability/metrics.rs`) has no arm for `/api/v1/telemetry/v1/{metrics,traces}`, so all telemetry requests emit `endpoint="/other"`. Fix the catalog text now (docs are in scope); the Rust `normalize_endpoint` arm is out of scope per the no-instrumentation constraint — flagged as a follow-up task (task-sized only because this loop is barred from Rust edits; the edit itself is two match arms + tests).
4. **Mechanism restatement (silence detection).** Task instance-language: `absent_over_time(gc_telemetry_ingest_total[15m])` gated by a 1h baseline. Mechanism: *detect ingest silence against a non-zero baseline*. Silence has TWO shapes: (a) series **absent** — pod restarted and no telemetry arrived since (absent_over_time catches this); (b) series **present but flat** — pod alive, traffic stopped; counter series persist for the life of the process, so `absent_over_time` can NEVER fire in this shape. Shape (b) needs the `rate == 0 and offset-baseline > 0` form (the `GCMeetingCreationStopped` shape). Proposal: `or`-combine both branches so the page covers the whole mechanism class, not just the named instance.

### Planned changes

**1. `gc-alerts.yaml` — two new alerts**

`GCTelemetryProxyHighRejectionRate` (group `gc-service-warning`, `component: telemetry-proxy`):

```promql
(
  sum(rate(gc_telemetry_ingest_total{status=~"rejected_.*|error"}[10m]))
  /
  sum(rate(gc_telemetry_ingest_total[10m]))
) > 0.10
and
sum(rate(gc_telemetry_ingest_total[10m])) > 0
```
`for: 10m`, `severity: warning`. Rejection selector is catalog-canonical `status=~"rejected_.*|error"` — **RESOLVED (a) by @observability**: dropping `error` would orphan the collector-down failure mode entirely (collector 502s increment `status="error"`, so `GCTelemetryProxySilent` won't fire for it and telemetry 502s are too low-volume to move `GCHighErrorRate`). Conditions honored (as corrected by @paired-global-controller point 2, consistent with finding 3): primary triage branches on the telemetry counter's OWN `status` values (`rejected_size` vs `rejected_rate` vs `error`); discriminating WITHIN `error` (client-fault 415/400 vs collector-fault 502/503) via HTTP metrics is NOT endpoint-scopable today — interim guidance is `gc_http_requests_total{endpoint="/other",status_code}` with an explicit conflation caveat (plus GC logs), becoming clean only after the normalize_endpoint follow-up lands. Alert description, corrected catalog text, and Scenario 10 tell this same story. Scenario 10's FIRST diagnosis step branches on fault direction, routing server-fault to the Scenario 5 error-rate path (@operations requirement B). The `and … > 0` no-traffic gate matches the established `GCMeetingCreationFailureRate` / `GCHighJoinFailureRate` shape; single-event-in-idle-window firing (ratio 1.0) is ACCEPTED deliberately for consistency with that precedent (@observability minor note 2 — no rate floor). `runbook_url: docs/runbooks/gc-incident-response.md#scenario-10-telemetry-proxy-high-rejection-rate`.

`GCTelemetryProxySilent` (group `gc-service-page`, `component: telemetry-proxy`):

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
`for: 5m`, `severity: page` (fixed by R-51). Both branches share ONE baseline definition — `sum(increase(gc_telemetry_ingest_total[1h] offset 15m)) > 0`, i.e. "events occurred in [t-75m, t-15m]" — per @observability's Gate 1 requested change (the earlier branch-A `[15m] offset 1h` gate was a 15m window ending 1h ago and would miss bursty traffic that last flowed 30–50m ago). Day-1 zero traffic never pages (empty gate vector on both branches). Deviation from task's "min_over_time" suggestion (approved by @team-lead decision B + @observability): bare per-series `min_over_time` on the RHS of `and on()` risks duplicate-match-group errors (status × payload_kind series) and tests series *presence*, not *events*; the `increase[1h]` window also satisfies the guard's expr-window rule. Description will state the honest detection delay (~20m for the flat shape: 15m rate window + 5m `for:`), per the GCMeetingCreationStopped precedent, plus the known benign trigger for the absent branch: a rolling restart in a low-traffic environment (counter registers lazily on first request post-restart) — check deploy timeline first (@paired-global-controller point 3). The `impact` annotation justifies the page in 3am terms: total loss of client-side observability + possible leading indicator of client-reachability/CORS/ingress breakage the server cannot otherwise see; directs oncall to immediately check user-facing traffic (@operations condition 1, @paired-global-controller point 3). No `or vector(0)` anywhere (@operations).

Guard compliance: severities in allowed set; `for:` present and ≥30s (or qualifying expr windows); repo-relative `runbook_url`s resolving to an existing file; no denylisted content in annotations.

**2. `gc-overview.json` — enrich existing panels (no id/gridPos churn)**

- Row 53 title → `Telemetry Ingest (R-2 / R-51)`.
- Panel 55 → title "Telemetry Ingest Duration (p50/p99)"; add second target `histogram_quantile(0.50, sum by(le) (rate(gc_telemetry_ingest_duration_seconds_bucket[$__rate_interval])))` legend `p50` (refId B).
- Panel 56 → title "Telemetry Payload Size (p99 by kind)"; expr → `histogram_quantile(0.99, sum by(payload_kind, le) (rate(gc_telemetry_payload_bytes_bucket[$__rate_interval])))`, legend `p99 {{payload_kind}}`.
- Panels 54, 57, 58, 60 unchanged (already compliant: `$__rate_interval`, templated `$datasource`, units declared, counters inside `increase()`).

**3. `docs/observability/metrics/gc-service.md`**

- Replace the two stale "added by task #16" notes with references to the two live alerts + enriched dashboard row.
- Correct the dead union-selector guidance (see finding 3): document that pre-handler 401/413 rejects currently surface as `endpoint="/other"` and note the follow-up.
- Add an **Alerting** line to the telemetry section naming both alerts.

**4. `docs/runbooks/gc-incident-response.md`**

- Headings exactly `### Scenario 10: Telemetry Proxy High Rejection Rate` and `### Scenario 11: Telemetry Ingest Silent` (anchors must match the alert `runbook_url` fragments — colons drop out of GitHub slugs).
- Full established shape: Symptoms / Diagnosis (bash + PromQL) / Common Root Causes / Remediation with expected recovery times per option + verification-of-recovery query / Escalation.
- **Update the runbook Table of Contents** with both new entries (@operations).
- Scenario 10: FIRST diagnosis step splits fault direction — client-fault (`rejected_*`, 400/413/415, rate-limit) vs server-fault (`error` + `gc_http` 502/503) — and routes server-fault to Scenario 5 (@operations B).
- Scenario 11 requirements (@operations):
  - Step 1 is an "is this user-impacting?" check (`gc_http_requests_total` rate + error rate); whole-service silent → route to Scenario 4; CORS preflight rejections spiking → CORS misconfig path.
  - Prose describes BOTH fired shapes (flat counters vs absent series) and how to tell which branch fired (run branches separately / check series existence).
  - Common Root Causes includes two benign causes: natural traffic trough (fast confirmation: `gc_http` traffic also near-zero AND error rate normal → not an incident; persistent trough-flapping is an Alertmanager time-of-day routing concern, not a rule change — GCMeetingCreationStopped precedent) and client rollout disabled/sampled-down telemetry (check recent web-app/SDK deploys).
  - **Auto-resolve ≠ recovery note**: after ~1h15m of continuous silence the baseline window drains and the alert resolves while ingest is still dead — verify `sum(rate(gc_telemetry_ingest_total[15m])) > 0` before closing the incident (@operations A).
  - Detection-delay math stated (~20m flat shape: 15m window + 5m `for:`).
- Additive edit to the Specialist Contacts table: a client/web-app row for telemetry-broken-by-client-rollout escalation (pre-ACKed by @operations).

**Alert Thresholds (cross-cutting review required)** — ADR-0031 structured block:

| Metric | Condition | For | Severity | Runbook |
|--------|-----------|-----|----------|---------|
| `gc_telemetry_ingest_total` | `sum(rate({status=~"rejected_.*\|error"}[10m])) / sum(rate([10m])) > 0.10`, gated `and sum(rate([10m])) > 0` | 10m | warning | `docs/runbooks/gc-incident-response.md#scenario-10-telemetry-proxy-high-rejection-rate` |
| `gc_telemetry_ingest_total` | `(sum(rate([15m])) == 0 and sum(increase([1h] offset 15m)) > 0) or (absent_over_time([15m]) and on() (sum(increase([1h] offset 15m)) > 0))` | 5m | page | `docs/runbooks/gc-incident-response.md#scenario-11-telemetry-ingest-silent` |

Reviewers: observability, operations. (Owner co-sign: @paired-global-controller per §6.3 routing.)

**5. `docs/observability/alerts.md` (scope widening — flagging per workflow)**

- Mechanism-language restatement: the repo convention is "every alert rule has an inventory entry in `docs/observability/alerts.md`" (verified: all 18 existing GC alerts have entries — count corrected per @dry-reviewer). The task names 4 files; the mechanism produces a 5th. Adding the two entries (GCHighJoinFailureRate-format) avoids SSoT drift. **APPROVED by @team-lead (decision A).** Per @dry-reviewer: PromQL blocks in alerts.md will be verbatim-identical to the gc-alerts.yaml exprs; catalog Alerting line names the alerts only (no threshold restatement — thresholds live in rules + inventory + runbook, the existing triple).

### Open questions — ALL RESOLVED at Gate 1

1. Silent expr shape → **or-combined, unified baseline gate** (approved: @team-lead B, @observability, @operations, @test).
2. Rejection selector → **keep `rejected_.*|error`** with fault-direction split in description + Scenario 10 step 1 (resolved by @observability; ops condition B honored).
3. alerts.md widening → **approved** (@team-lead A); normalize_endpoint Rust gap → **deferred by constraint**, tracked in `docs/TODO.md` §Observability Debt (@team-lead C).
4. Scenario numbering/escalation → **confirmed** (@operations, with ToC + contacts-row + auto-resolve + benign-causes requirements folded in above).

### Verification (per @test Gate 1 asks)

- Full `scripts/guards/run-guards.sh` (not just alert-rules-policy + dashboard-panels — the application-metrics guard exercises the panel-56 expr change and catalog edits). Result recorded in Devloop Verification Steps.
- `python3 -m json.tool` sanity-parse of gc-overview.json.
- `promtool check rules` on gc-alerts.yaml: **promtool is NOT available in this container** (no binary, no docker) — recorded explicitly as a verification gap, no silent skip. Mitigations: PromQL reviewed line-by-line by @observability at Gate 1 (done for the Silent expr — the highest-syntax-risk artifact), YAML parse validated locally, and @test will independently attempt promtool at review.
- Catalog↔alert selector consistency check (manual): the catalog's canonical rejection-rate example and the alert expr selector stay character-identical (@test #2 — guard-invisible, review-enforced).
- alerts.md PromQL blocks verbatim-identical to gc-alerts.yaml exprs (@dry-reviewer #2).

---

## Pre-Work

None

---

## Implementation Summary

Implemented exactly per the converged Gate 1 plan; no deviations.

1. **Alerts** (`gc-alerts.yaml`): `GCTelemetryProxySilent` (page group, `component: telemetry-proxy`) — or-combined flat-counter + absent-series silence detection, both branches gated on the unified baseline `sum(increase(gc_telemetry_ingest_total[1h] offset 15m)) > 0`, `for: 5m`; description carries detection-delay (~20m), rolling-restart benign trigger, and auto-resolve-is-not-recovery notes; impact justifies the page in 3am terms. `GCTelemetryProxyHighRejectionRate` (warning group) — catalog-canonical `status=~"rejected_.*|error"` selector over 10m windows, `> 0.10`, no-traffic gate, `for: 10m`; description leads with telemetry-status triage and carries the `endpoint="/other"` conflation caveat.
2. **Dashboard** (`gc-overview.json`): row 53 retitled "Telemetry Ingest (R-2 / R-51)"; panel 55 → p50 (refId A) + p99 (refId B) duration targets, retitled; panel 56 → `sum by(payload_kind, le)` with `p99 {{payload_kind}}` legend, retitled. No id/gridPos churn; panels 54/57/58/60 untouched.
3. **Catalog** (`gc-service.md`): stale "task #16 will add" forward-notes replaced with live alert/dashboard references; dead union selector `endpoint=~".*/telemetry/.*"` corrected to `endpoint="/other"` with a new Endpoint Caveat block pointing at the TODO entry; `error`-status note records the deliberate keep-`error` selector decision.
4. **Runbook** (`gc-incident-response.md`): Scenarios 10 & 11 in full established shape; Scenario 10 step 1 splits fault direction on telemetry status values (server-fault → Scenario 5); Scenario 11 step 1 is the user-impact check (→ Scenario 4), covers both silence shapes, two benign causes, auto-resolve caveat, detection-delay math. ToC + Specialist Contacts (Client/Web-App row) + Last Updated bumped.
5. **Inventory** (`docs/observability/alerts.md`): both entries added in severity-correct subsections; PromQL blocks verified character-identical to the YAML exprs (automated dedent-compare).
6. **Debt tracking** (`docs/TODO.md` §Observability Debt): normalize_endpoint telemetry-arm entry with file:line refs, consequence, action (including the list of caveats to remove when it lands), <20 LoC estimate, owner global-controller (with observability), team-lead decision C provenance.

---

## Files Modified

| File | Change |
|------|--------|
| `infra/docker/prometheus/rules/gc-alerts.yaml` | +2 alerts (GCTelemetryProxySilent page, GCTelemetryProxyHighRejectionRate warning) |
| `infra/grafana/dashboards/gc-overview.json` | Row 53 title; panel 55 p50/p99; panel 56 per-kind p99 |
| `docs/observability/metrics/gc-service.md` | Forward-refs → live refs; endpoint caveat; selector decision note |
| `docs/runbooks/gc-incident-response.md` | +Scenarios 10/11, ToC, Specialist Contacts row, Last Updated |
| `docs/observability/alerts.md` | +2 inventory entries (verbatim PromQL) |
| `docs/TODO.md` | +normalize_endpoint entry (§Observability Debt) |
| `docs/devloop-outputs/2026-08-03-gc-telemetry-obs-artifacts/main.md` | This document |

Commit trailers required (per Cross-Boundary Classification): `Approved-Cross-Boundary: global-controller` (alerts+dashboard hunks), `Approved-Cross-Boundary: operations` (runbook hunks) — @team-lead adds at commit time after Gate 3.

---

## Devloop Verification Steps

| Check | Result |
|-------|--------|
| `python3 -m json.tool` on gc-overview.json | PASS |
| `validate-alert-rules` (dt-guard alert-rules-policy) | PASS (`alert-rules-clean-4-files`) |
| `dt-guard dashboard-panels --root /work` | PASS (`dashboard-panels-clean-12-files`) |
| `validate-application-metrics` | PASS (`application-metrics-clean`) |
| Full `scripts/guards/run-guards.sh` | **FINAL (Gate 2 attempt 2): 35/35 PASS — full green** (incl. layers 1-7). Attempt-1 history: 34/35 with 1 FAIL = pre-existing `validate-todo-tracking` violation in the 2026-07-29 loop's main.md (inline_debt_body; verified pre-existing via `git stash` re-run and escalated to @team-lead). RESOLVED by @team-lead's separate hygiene commit `65b8912` (preamble reflow + 75-line-cap debt filed per that bullet's own instruction); attempt 2 green, independently re-confirmed by @test (`todo-tracking-clean`). |
| `promtool check rules` | **GAP (declared at Gate 1)**: promtool not in container, docker absent. Mitigations: PromQL line-by-line review by @observability at Gate 1; automated dedent-compare proves alerts.md blocks == YAML exprs (IDENTICAL × 2); @test to attempt independently at review. |
| Anchor slugs vs runbook headings | PASS (automated: both fragments match GitHub-slugified headings; 1 yaml ref + 1 ToC ref each) |
| Catalog↔alert selector character-identity | PASS (`status=~"rejected_.*|error"` present in both) |
| alerts.md PromQL == yaml exprs | PASS (automated compare: IDENTICAL × 2) |

---

## Code Review Results

All findings fixed in-loop; zero new deferrals beyond the pre-approved decision-C item.

| Reviewer | Findings | Resolution | Verdict |
|----------|----------|------------|---------|
| security | 0 (Gate 1 checklist pre-baked; hygiene/anchors/no-guard:ignore honored) | — | CLEAR |
| test | 1: verification record showed attempt-1 34/35, not final green | FIXED — run-guards row now leads with final 35/35 (attempt-2), attempt-1 history kept as resolved note → `65b8912` | RESOLVED-DEFERRED (accepts decision-C deferral; pending re-verify ping) |
| observability | 2: (i) `gc_jwt_validations_total` cited as telemetry-401 signal at 3 sites — phantom-signal risk (counter is gRPC service-token only); (ii) re-review sibling: pre-existing line 473 of the same catalog honesty block still credited the counter, contradicting the fixed union text below | BOTH FIXED — (i) runbook step 4: GC logs authoritative + counter as explicitly-labeled corroboration-only; alerts.md step 4: counter → GC logs with caveat; catalog union line rewritten with does-NOT-provide note; (ii) line 473: counter credit dropped ("plus GC logs, NOT here — and NOT on `gc_jwt_validations_total` either"), pointing down to the union bullet so the block is internally consistent | RESOLVED-DEFERRED (all fixes verified at all 4 sites; "deferred" = solely the tracked normalize_endpoint spin-out) |
| code-reviewer | 0 | — | RESOLVED-DEFERRED (bookkeeping on decision-C deferral; zero findings) |
| dry-reviewer | 0 (all 3 Gate 1 commitments verified: byte-identical PromQL, no threshold restatement in catalog, shared baseline subexpr) | Filed rules↔inventory drift-guard TODO under §Cross-Service Duplication (their action, not this diff) | RESOLVED-DEFERRED (extraction-TODO technicality; explicitly zero-findings) |
| operations | 2: F1 Scenario 11 CORS remediation didn't name `CORS_ALLOWED_ORIGINS`/location/fail-closed; F2 alerts.md severity style drift `Critical (page)` | BOTH FIXED — F1: key + check command + locations + fail-closed clause in RC2 and Option 1 (facts verified against config.rs:445,1033 + configmap.yaml:64); F2: normalized to `Critical` | RESOLVED-FIXED (authorizes `Approved-Cross-Boundary: operations` trailer) |
| semantic-guard | 0 | — | CLEAR |
| paired-global-controller (owner co-sign) | 1 (cross-domain courtesy flag, same as ops F2): severity style drift | FIXED (same edit) | CLEAR — GC-owned surfaces clean; both Gate 1 amendments verified landed |

---

## Accepted Deferrals

- `docs/TODO.md` §Observability Debt — GC normalize_endpoint() telemetry arm (decision C; barred by no-Rust constraint)
- `docs/TODO.md` §Cross-Service Duplication (DRY) — rules↔alerts.md inventory drift guard missing (GC 20/20 today)

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `0d3112fd604e58ce4222032f8df50feba637b367`
2. Review all changes: `git diff 0d3112fd604e58ce4222032f8df50feba637b367..HEAD`
3. Soft reset (preserves changes): `git reset --soft 0d3112fd604e58ce4222032f8df50feba637b367`
4. Hard reset (clean revert): `git reset --hard 0d3112fd604e58ce4222032f8df50feba637b367`

---

## Issues Encountered & Resolutions

1. **Pre-existing guard failure on the branch** (not this diff): `validate-todo-tracking` flags `docs/devloop-outputs/2026-07-29-client-credential-lifetime-guard-task58/main.md:2703` (`inline_debt_body` — the knowledge-index 75-line-cap "Not yet filed" bullet). Confirmed pre-existing by stashing this loop's changes and re-running (still fails). Not masked, not unilaterally fixed (the bullet deliberately records an owner-to-file state belonging to @paired-infrastructure-2 / story close); escalated to @team-lead for routing.
2. **PyYAML absent in container** — YAML validity instead proven via the alert-rules guard (which parses the file) passing.

---

## Lessons Learned

1. **Read the current tree before trusting task text.** The task said "gains a new Telemetry Ingest row"; the row already existed minimally (landed by tasks #5/#10, explicitly marked "enriched in task #16"). Studying the artifacts first turned a duplicate-row hazard into a clean enrich-in-place diff with zero id/gridPos churn.
2. **Mechanism restatement caught a real coverage hole.** Restating "add an absent_over_time alert" as "detect ingest silence against a non-zero baseline" surfaced that counter series persist for process lifetime — absent_over_time alone can NEVER fire in the pod-alive-but-silent shape, which is the realistic one. The or-combined expr covers the whole mechanism class.
3. **Verify documented selectors against emitting code.** The catalog's union selector (`endpoint=~".*/telemetry/.*"`) was dead on arrival — `normalize_endpoint()` has no telemetry arm. A doc that prescribes a selector should be checked against the label values the code can actually emit (same phantom-signal class as the `gc_jwt_validations_total` finding: a signal source credited with visibility it structurally lacks).
4. **Phantom signals hide in siblings.** Fixing the JWT-counter miscredit at 3 sites still left a 4th, pre-existing sibling 20 lines above the corrected text in the same block. When fixing a claim-class, grep for the claim, not just the sites you wrote.
5. **Baseline-gated absence alerts need an auto-resolve caveat.** Any "silent vs baseline" alert self-resolves when the baseline window drains (~1h15m here); without the "resolution is not recovery" runbook note, oncall closes incidents that are still live.
6. **Pre-existing branch reds: prove, escalate, don't absorb.** `git stash` + guard re-run cleanly attributed the todo-tracking failure to the branch, not this diff; the Lead's separate hygiene commit kept the feature diff clean and the repair auditable.
