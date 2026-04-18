# Devloop Output: ADR-0031 FU#3b — MC bare `type` → `heartbeat_type`

**Date**: 2026-04-18
**Task**: Rename the bare `type` label on `mc_gc_heartbeats_total` and `mc_gc_heartbeat_latency_seconds` to `heartbeat_type`. Bare `type` shadows a generic identifier and causes ambiguity in cross-service dashboards (grouping by `type` across metrics would mix heartbeat-type with unrelated dimensions).
**Specialist**: meeting-controller (owns MC)
**Mode**: Agent Teams (light)
**Branch**: `feature/dashboard-owner-debate`

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `f18aa4d6faa7d1de9c550a181074d93d3d467a36` |
| Branch | `feature/dashboard-owner-debate` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@devloop-fu3b-mc-heartbeat-type` |
| Implementing Specialist | `meeting-controller` |
| Iteration | `1` |
| Security | `APPROVE (security@devloop-fu3b-mc-heartbeat-type) — 0 findings, 1 non-blocking observation (dual-catalog, TODO'd)` |
| Observability | `APPROVE (observability@devloop-fu3b-mc-heartbeat-type) — 2 findings surfaced iteratively on mc.md, both fixed; dual-catalog deferred` |

---

## Task Overview

### Objective

Close ADR-0031 label-canonicalization FU#3b. Rename bare `type` → `heartbeat_type` on MC's heartbeat metrics. Aligns with the principle (§label-taxonomy naming) that label keys should be self-describing, especially for labels that describe the *thing being measured* rather than a technical dimension.

### Scope

1. `crates/mc-service/src/observability/metrics.rs` — 2 label invocations:
   - Line 235: `mc_gc_heartbeats_total` counter.
   - Line 246: `mc_gc_heartbeat_latency_seconds` histogram.
2. `infra/grafana/dashboards/mc-overview.json`, `mc-slos.json`, `mc-logs.json` — PromQL references. The `type` string appears ~244 times across these files but the majority are Grafana panel-type attributes (e.g., `"type": "timeseries"`), NOT the heartbeat label. Implementer should filter: only PromQL expressions referencing `mc_gc_heartbeats_total{type=...}` / `mc_gc_heartbeat_latency_seconds{type=...}` / `by (type)` on those metrics need rewriting.
3. `infra/docker/prometheus/rules/mc-alerts.yaml` — current grep shows 0 references. Verify nothing got missed.
4. `docs/observability/metrics/mc-service.md` — catalog entries for the two heartbeat metrics.
5. `TODO.md` — remove the closed follow-up entry.

### Posture

Mechanical-ish. Scope is narrower than FU#3a (fewer ripple files) but requires filter-discipline to avoid renaming Grafana panel-type attributes.

### Debate Decision

NOT NEEDED — canonical-name target is documented; scope is a label-key rename.

---

## Reference

- TODO.md "ADR-0031 label-canonicalization follow-ups → MC bare `type` → `heartbeat_type`"
- Canonical-name target: `docs/observability/label-taxonomy.md` §Shared Label Names
- Guards: `scripts/guards/simple/validate-dashboard-panels.sh`, `validate-alert-rules.sh`, `validate-application-metrics.sh`

---

## Implementation Summary

Closes ADR-0031 label-canonicalization FU#3b. Renamed bare `type` label → `heartbeat_type` on MC's two heartbeat metrics (`mc_gc_heartbeats_total` counter, `mc_gc_heartbeat_latency_seconds` histogram). Bare `type` shadows Grafana's generic panel-type attribute and makes cross-metric aggregations ambiguous.

### Filter-discipline

The word `type` appears ~244 times across MC dashboards — the vast majority are Grafana JSON panel-kind attributes (`"type": "timeseries"` etc.) that must NOT be renamed. Only PromQL-expression references on the two heartbeat metrics qualify. Implementer correctly renamed just 1 query in mc-overview.json (the only one that grouped heartbeat-latency `by (le, type)`). Other "by Type" panels on MC dashboards (Actor Mailbox, Actor Panics, Token Refresh, Session Join) use distinct labels (`actor_type`, `error_type`, `failure_reason`) and were correctly untouched.

### Changes (+/- line stat updated post-polish)

1. `crates/mc-service/src/observability/metrics.rs` — 2 label invocations + 2 docstring comments.
2. `docs/observability/metrics/mc-service.md` — both catalog entries + cardinality-budget table.
3. `docs/observability/metrics/mc.md` (legacy parallel catalog) — 4 references updated: line 130, 142, 149, 323. Caught iteratively (implementer's initial fix missed line 323 — observability re-surfaced it).
4. `docs/observability/label-taxonomy.md` §Current Drift — MC heartbeat entry removed; list re-numbered.
5. `infra/grafana/dashboards/mc-overview.json` — 1 PromQL rewrite (heartbeat latency percentile panel: `sum by(le, type)` → `sum by(le, heartbeat_type)`, legend `{{type}}` → `{{heartbeat_type}}`, panel title "P99 by Type" → "P99 by Heartbeat Type", description prose).
6. `TODO.md` — (a) removed the closed "MC bare `type` → `heartbeat_type`" follow-up; (b) added new "Dual catalog: `docs/observability/metrics/mc.md` vs `mc-service.md`" entry under Convention Follow-ups capturing the drift risk surfaced during this devloop.

### Unaffected surfaces confirmed

- `infra/grafana/dashboards/mc-slos.json`, `mc-logs.json` — no heartbeat-metric `type` references.
- `infra/docker/prometheus/rules/mc-alerts.yaml` — MCGCHeartbeatErrorRate groups by `status`, not `type`. Untouched.
- `docs/runbooks/mc-incident-response.md`, `mc-deployment.md` — no heartbeat-metric `type` PromQL samples.

### Tech-debt surfaced

Dual MC metric catalog (`mc.md` ~339 lines + `mc-service.md` ~408 lines, both with "Meeting Controller Metrics Catalog" title and near-identical headers). `mc-service.md` matches the sibling `ac-service.md` / `gc-service.md` / `mh-service.md` pattern and should be authoritative; `mc.md` is legacy and creates drift risk. Caught by observability during this devloop's review when the initial heartbeat_type rename was applied only to mc-service.md. TODO.md entry now captures the cleanup (delete mc.md, repoint `metrics.rs:256` doc-comment, verify no dashboard/alert links). Owner: observability.

---

## Files Modified

| File | Changes |
|------|---------|
| `crates/mc-service/src/observability/metrics.rs` | 2 label key renames + 2 docstring updates |
| `docs/observability/metrics/mc-service.md` | Both catalog entries + cardinality-budget table row |
| `docs/observability/metrics/mc.md` | 4 refs (lines 130, 142, 149, 323) |
| `docs/observability/label-taxonomy.md` | Current Drift list: MC heartbeat entry removed |
| `infra/grafana/dashboards/mc-overview.json` | 1 heartbeat-latency panel (PromQL + legend + title + description) |
| `TODO.md` | Closed FU#3b entry; added dual-catalog cleanup entry |

Net: 6 files, small diff.

---

## Devloop Verification Steps

- L1 (cargo check): PASS
- L2 (cargo fmt): PASS
- L3 (guards): 18/18 PASS — validate-alert-rules, validate-dashboard-panels, validate-application-metrics all clean
- L4 (cargo test -p mc-service --lib observability::metrics): 23/23 PASS
- L5 (clippy): trivial — no Rust logic change
- L6 (cargo audit): pre-existing vulnerabilities, not this devloop's concern
- L7 (semantic): Lead-judgment SAFE — label rename only, filter-discipline verified by grep
- L8 (env-tests): skipped — no service-behavior changes

---

## Code Review Results

### Security Specialist
**Verdict**: APPROVE (0 findings; 1 non-blocking observation)

Pure mechanical rename. No credentials/PII/hostnames. PromQL semantic equivalence verified (same metric, same aggregation, just label key). Filter discipline verified via grep — no Grafana panel-type attributes accidentally renamed. Non-blocking observation about stale `type` in legacy `mc.md` catalog — independently caught by observability and fixed in-PR.

### Observability Specialist
**Verdict**: APPROVE (2 findings surfaced iteratively on `mc.md`, both fixed in-PR)

End-to-end rename consistency verified across primary surfaces. Filter discipline clean. First-pass review found 3 stale refs in `mc.md` (lines 130/142/149); second-pass re-review found a 4th (line 323 cardinality-budget table row). All fixed inline. Dual-catalog cleanup (bigger issue of mc.md's continued existence) appropriately deferred to TODO.md entry — the devloop's scope is the rename, not a dual-catalog audit.

**Lesson from this review cycle**: iterative review on duplicate documentation surfaces — when the same content exists in multiple files, one review pass may not catch all drift. Two-pass review caught it here. Adds weight to the dual-catalog TODO entry (drift risk is real, not hypothetical).

---

## Rollback Procedure

1. Start commit: `f18aa4d6faa7d1de9c550a181074d93d3d467a36`
2. Soft reset: `git reset --soft f18aa4d`
3. Contained to MC metrics.rs + MC dashboards + MC catalog + TODO.md.
