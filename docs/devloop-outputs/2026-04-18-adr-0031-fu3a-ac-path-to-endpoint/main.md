# Devloop Output: ADR-0031 FU#3a — AC `path` → `endpoint` rename

**Date**: 2026-04-18
**Task**: Rename the `path` label to `endpoint` on `ac_http_requests_total` and `ac_http_request_duration_seconds` to align with the canonical label name in `docs/observability/label-taxonomy.md` and with GC's existing `endpoint` label.
**Specialist**: auth-controller (owns AC)
**Mode**: Agent Teams (light)
**Branch**: `feature/dashboard-owner-debate`

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `f580a265d1f4305a5631dea7c6902a992f6aa1ea` |
| Branch | `feature/dashboard-owner-debate` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@devloop-fu3a-ac-path-to-endpoint` |
| Implementing Specialist | `auth-controller` |
| Iteration | `1` |
| Security | `APPROVE (security@devloop-fu3a-ac-path-to-endpoint) — 0 findings` |
| Observability | `APPROVE (observability@devloop-fu3a-ac-path-to-endpoint) — 1 optional finding, partially fixed inline + TODO'd the rest` |

---

## Task Overview

### Objective

Close ADR-0031 FU#3a (AC half) — first of three label-canonicalization follow-ups. Aligns AC's `path` label with the canonical `endpoint` name documented in `docs/observability/label-taxonomy.md`. GC already emits `endpoint`; AC catches up.

**Scope split note**: the TODO entry bundled this AC rename with GC's `status` → `status_code`/`status_category` schema change. Lead split them: the AC rename is mechanical (single label key swap); the GC status split is a metric-schema change with different ripple. GC status split becomes its own follow-up (new TODO entry) to be scheduled separately.

### Scope

1. `crates/ac-service/src/observability/metrics.rs` — rename `"path"` → `"endpoint"` on the 2 label invocations (lines 259, 266).
2. `infra/grafana/dashboards/ac-overview.json` — rewrite 7 PromQL references from `path=...` to `endpoint=...`.
3. `docs/observability/metrics/ac-service.md` — update 3 catalog doc references (label name + example values).
4. Verify guards pass end-to-end. No ac-alerts.yaml exists (AC has no alerts currently).

### Posture

Mechanical cross-artifact rename. All changes land atomically in this PR. Guards (`validate-dashboard-panels.sh` + `validate-application-metrics.sh`) are the forcing function against missed updates.

### Debate Decision

NOT NEEDED — documented canonical name per label-taxonomy.md; scope is a mechanical rename.

---

## Reference

- TODO.md "ADR-0031 label-canonicalization follow-ups → AC `path` / GC `endpoint` → canonical `endpoint`"
- Canonical-name source: `docs/observability/label-taxonomy.md` §Shared Label Names
- Guards: `scripts/guards/simple/validate-dashboard-panels.sh`, `scripts/guards/simple/validate-application-metrics.sh`

---

## Implementation Summary

Closes ADR-0031 label-canonicalization FU#3a (AC rename half). Renamed AC's HTTP-metric label `path` → `endpoint` to match the canonical name documented in `label-taxonomy.md` and already in use by GC. Mechanical cross-artifact rename; single atomic PR.

### Changes
1. `crates/ac-service/src/observability/metrics.rs`: 2 label invocations on HTTP histogram + counter + docstring comment. Internal `normalized_path: String` variable left unchanged (not a label — just the local name).
2. `infra/grafana/dashboards/ac-overview.json`: 7 primary references (3 `by (... path)` aggregators, 3 `{{path}}` legend formats, 1 panel description) + 3 panel titles "by Path" → "by Endpoint". Zero `path` tokens remain in the dashboard post-rename.
3. `docs/observability/metrics/ac-service.md`: 2 label entries (lines 193, 204) + 1 usage/cardinality comment (line 196).
4. `docs/runbooks/ac-service-deployment.md`: 1 PromQL sample output (line 1024) updated path → endpoint. **Plus** 1 observability-optional-finding fix on the same line: `status="200"` → `status_code="200"` (pre-existing drift, not introduced by this devloop; fixed inline since line was already hot).
5. `TODO.md`: (a) removed the old combined AC+GC entry under label-canonicalization follow-ups; (b) added a new, focused "### GC `status` → `status_code` + `status_category` split" entry preserving coordinated-migration discipline; (c) added a new "### AC runbook sample/PromQL accuracy pass" entry capturing 4 remaining `status=` drift references at lines 303, 1057, 1058, 1117 (owner: auth-controller, no deadline).

### Survey discipline
Implementer's own survey (grep-audit) found 1 additional stray reference beyond the 7 I enumerated (ac-service-deployment.md:1024). Fixed inline. No other stray references.

### Scope split from original TODO entry
Original FU#3a TODO entry bundled AC's `path`/`endpoint` rename with GC's `status` schema change. Lead split them: the AC rename is pure label substitution (fits a mechanical-is-mechanical `--light` devloop); the GC schema split is schema-level surgery (two new labels, one deprecated; different ripple across dashboards/alerts/alertmanager). GC work now has its own TODO entry for separate scheduling.

---

## Files Modified

| File | Changes |
|------|---------|
| `crates/ac-service/src/observability/metrics.rs` | 2 label key renames + docstring comment |
| `infra/grafana/dashboards/ac-overview.json` | 7 PromQL refs + 3 panel titles + 1 description |
| `docs/observability/metrics/ac-service.md` | 2 label entries + 1 usage comment |
| `docs/runbooks/ac-service-deployment.md` | 1 sample output (rename) + 1 `status`/`status_code` drift fix |
| `TODO.md` | Restructure — split old bundled entry, add runbook accuracy sub-entry |

Net: 5 files, +38 / −30.

---

## Devloop Verification Steps

- L1 (cargo check): PASS
- L2 (cargo fmt): PASS
- L3 (guards): 18/18 PASS — `validate-dashboard-panels`, `validate-application-metrics` both green
- L4 (cargo test -p ac-service): 20/20 metrics tests PASS
- L5 (clippy): trivial — no Rust logic changes
- L6 (cargo audit): pre-existing vulnerabilities, not this devloop's concern
- L7 (semantic): Lead-judgment SAFE — rename-only, no semantic surface added
- L8 (env-tests): skipped — no service-behavior changes; label rename is schema-level metric emission only

---

## Code Review Results

### Security Specialist
**Verdict**: APPROVE (0 findings)

Rename-only confirmed: no new hostnames/credentials/PII, no annotation drift beyond label substitution. PromQL semantics intact (same metric name, same `job="ac-service"` + `status_code=~"4..|5.."` filters; only the path/endpoint label key changed). No stray `path=` selectors remain in ac-overview.json (grep-verified).

### Observability Specialist
**Verdict**: APPROVE (1 optional finding: partially fixed inline, remaining TODO'd)

End-to-end rename consistency verified. Canonical-name alignment confirmed against label-taxonomy.md §Shared Label Names (bounded HTTP path, medium cardinality ~50). PromQL semantic equivalence confirmed (pure renames, no panel drift). TODO.md split coherent. Guards 18/18 pass.

**Optional finding (partial fix + TODO)**: observability flagged pre-existing `status=` PromQL drift in runbook samples (lines 1024, 303, 1057, 1058, 1117 — all predate ADR-0031). Implementer fixed line 1024 inline (directly adjacent to the primary rename) and TODO'd the remaining four as an "AC runbook sample/PromQL accuracy pass" owned by auth-controller. Lead accepted the split — ownership judgment + the TODO has proper framing + the remaining 4 lines represent a different class of work (documentation accuracy, not canonicalization).

---

## Rollback Procedure

1. Start commit: `f580a265d1f4305a5631dea7c6902a992f6aa1ea`
2. Soft reset: `git reset --soft f580a26`
3. Changes contained to AC metrics + ac-overview.json + ac-service metric catalog.
