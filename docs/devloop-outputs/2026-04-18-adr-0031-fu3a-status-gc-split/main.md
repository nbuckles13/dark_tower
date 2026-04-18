# Devloop Output: ADR-0031 FU#3a-status — GC `status` schema split (HTTP metrics)

**Date**: 2026-04-18
**Task**: On GC HTTP metrics, split the overloaded `status` label (currently a categorized bucket: `success`/`error`/etc.) into `status_code` (raw HTTP code, aligning with AC's canonical) plus `status_category` (the existing categorization). Leave non-HTTP GC metrics' `status` alone — they're semantic outcomes, not HTTP codes.
**Specialist**: global-controller (owns GC)
**Mode**: Agent Teams (light)
**Branch**: `feature/dashboard-owner-debate`

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `69c2b0cee11117a1e2cd484c5287bc951c1c4312` |
| Branch | `feature/dashboard-owner-debate` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@devloop-fu3a-status-gc-split` |
| Implementing Specialist | `global-controller` |
| Iteration | `1` |
| Security | `APPROVE (security@devloop-fu3a-status-gc-split) — 0 findings` |
| Observability | `APPROVE (observability@devloop-fu3a-status-gc-split) — 1 non-blocking nit addressed inline` |

---

## Task Overview

### Objective

Close ADR-0031 FU#3a-status (carved out of FU#3a during the AC path rename devloop). Align GC's HTTP metric label naming with AC's canonical: `status_code` for raw HTTP code, `status_category` for the categorized bucket. Currently GC overloads `status` to mean the category and has no raw-code-level label.

### Scope

**Primary targets (HTTP metrics, schema change)**:
- `gc_http_requests_total` and `gc_http_request_duration_seconds`: add `status_code` (raw code via `status_code.to_string()`), rename current `status` → `status_category`.
- Update gc-overview.json + gc-slos.json + gc-alerts.yaml references to migrate queries to whichever of `status_code` / `status_category` is semantically right per query.
- Update `docs/observability/metrics/gc-service.md` catalog entries.

**Non-HTTP `status` uses (AUDIT required, likely leave alone)**:
GC also emits `status` on non-HTTP metrics: `gc_mc_assignment_*`, `gc_mc_assignments_total`, `gc_db_queries_total`, `gc_token_refresh_total`, `gc_ac_requests_total`, `gc_grpc_mc_calls_total`, `gc_mh_selection_*`, `gc_mh_selections_total`. Per the metric source, these `status` values are **semantic outcomes** (`success`/`error`/`rejected`/etc.) — NOT HTTP status codes. They don't fit a `status_code` rename. Implementer should audit each and make a per-metric call:
- Leave as `status` (semantic outcome; the ADR-0031 taxonomy doesn't explicitly forbid this)
- Rename to `result` if that feels semantically cleaner (consistent-with-future-authoring but wider ripple)

Recommend leaving them alone unless the implementer's audit reveals consistency issues. Keep this devloop scoped to HTTP metrics. If non-HTTP `status` renames are worth doing, they go in a follow-up TODO entry.

### Design notes

- **`categorize_status_code(status_code)`** helper in `gc-service/src/observability/metrics.rs` already exists. The split means `record_http_request` emits both values (raw `status_code.to_string()` and `categorize_status_code(status_code)` as `status_category`) rather than just the categorized one.
- **Cardinality check**: raw HTTP codes bump cardinality meaningfully. AC's catalog says "medium, ~50 combinations, bounded by known paths and status codes." GC's similar. Add `status_code` times `status_category` should stay below ADR-0011's 1000 combos/metric budget easily — endpoint count × 10-20 status codes is fine. Verify before shipping.
- **No alertmanager.yml**: routing currently documented only in `docs/observability/alerts.md` — implementer checks if severity/team routing references `status={...}` and updates if so.

### Debate Decision

NOT NEEDED — schema shape agreed (raw + categorized, mirror AC's canonical). Only domain judgment is the non-HTTP audit.

---

## Reference

- Canonical names: `docs/observability/label-taxonomy.md` §Shared Label Names
- GC metric catalog: `docs/observability/metrics/gc-service.md`
- GC metric source: `crates/gc-service/src/observability/metrics.rs`
- Prior AC precedent (different shape — AC already emits raw `status_code`): `docs/observability/metrics/ac-service.md`

---

## Implementation Summary

Closes ADR-0031 FU#3a-status. GC HTTP metrics (`gc_http_requests_total`, `gc_http_request_duration_seconds`) now align with AC's canonical shape: raw `status_code` only. The implementer's survey uncovered that this was a narrower change than first scoped — the **counter** was already emitting `status_code`; only the **histogram** was the outlier emitting a categorized `status`. Dashboards and alerts were already keyed on `status_code`. The work reduced to "fix the histogram + update the catalog + clean up dead helper."

### Schema choice: `status_code` only (not dual-emit)

Three decisive reasons over emit-both (`status_code` + `status_category`):
1. **Cardinality**: emit-both worst-case ~25,000/metric — above ADR-0011's 1,000/metric budget. Raw-only worst-case ~1,050 (typically ≪300 in practice).
2. **AC canonical parity**: AC emits only `status_code`. Verbatim alignment.
3. **No new information content**: category is 100% derivable from raw code via PromQL regex (`status_code=~"2.."` / `"[45].."` / etc.). Dashboards and alerts already use this pattern.

### Changes

1. `crates/gc-service/src/observability/metrics.rs` — histogram now emits `status_code` to match counter. Removed dead `categorize_status_code()` helper and its test. Updated module-level cardinality doc.
2. `docs/observability/metrics/gc-service.md` — catalog entries for both HTTP metrics updated. Cardinality math tightened per observability's non-blocking nit: nominal worst-case ~1,050 nudges ADR-0011's 1,000 ceiling; realistic observed stays ≪300 because no single `(method, endpoint)` pair surfaces >~3 codes; revisit trigger ("if a new endpoint surfaces >5 distinct codes"); note that histogram `_bucket` expansion (~12×) is tracked separately by `validate-histogram-buckets.sh`. Cardinality management table now shows the HTTP-scoped `status_code` row alongside a non-HTTP `status` row listing the 9 metrics that keep `status` as a semantic-outcome label.
3. `TODO.md` — removed the closed follow-up entry under "ADR-0031 label-canonicalization follow-ups".

### Non-HTTP `status` audit

GC emits `status` on 9 non-HTTP metrics: `gc_mc_assignments_total`, `gc_db_queries_total`, `gc_token_refresh_total`, `gc_ac_requests_total`, `gc_grpc_mc_calls_total`, `gc_mh_selections_total`, `gc_meeting_creation_total`, `gc_meeting_join_total`, `gc_registered_controllers`. Values are semantic outcomes (`success`/`error`/`rejected`) — NOT HTTP codes. Label taxonomy §70 explicitly allows `status` as a coarse outcome classifier. Audit result: leave all 9 alone. No rename; no per-metric judgment call flagged the need for a `result` / `outcome` alternative.

### Surface that didn't need touching (discovered, flagged)

- `infra/grafana/dashboards/gc-overview.json` + `gc-slos.json` — every HTTP-metric query already uses `status_code=~"..."`. Zero edits.
- `infra/docker/prometheus/rules/gc-alerts.yaml` — burn-rate alerts (GCHighErrorRate line 36, GCErrorBudgetBurnRateCritical line 104, GCErrorBudgetBurnRateWarning line 229) already use `status_code=~"[45].."`. Zero edits.
- `docs/observability/alerts.md` — HTTP snippets already use `status_code`. Zero edits.
- No `alertmanager.yml` in repo (routing doc is the source of truth, already correct).

---

## Files Modified

| File | Changes |
|------|---------|
| `crates/gc-service/src/observability/metrics.rs` | Histogram status_code emission, removed dead categorize_status_code helper + its test |
| `docs/observability/metrics/gc-service.md` | HTTP entries, cardinality math tightening, HTTP-vs-non-HTTP table split |
| `TODO.md` | Closed follow-up entry removed |

Net: 3 files, +39 / −68 lines.

---

## Devloop Verification Steps

- L1 (cargo check): PASS
- L2 (cargo fmt): PASS
- L3 (guards): 18/18 PASS
- L4 (tests `-p gc-service --lib observability::metrics`): 19/19 PASS; removed `test_categorize_status_code` (dead helper deleted)
- L5 (clippy): trivial — only metric-emission surgery and dead-code removal
- L6 (cargo audit): pre-existing vulnerabilities, not this devloop's concern
- L7 (semantic): Lead-judgment SAFE — narrow metrics.rs surgery + catalog updates
- L8 (env-tests): skipped — no service-behavior changes

---

## Code Review Results

### Security Specialist
**Verdict**: APPROVE (0 findings)

`status_code` is u16→string from framework response codes — no user-controlled ingress. Cardinality bounded to enumerable HTTP code set. No PromQL injection surface (zero query rewrites needed). Guards pass.

### Observability Specialist
**Verdict**: APPROVE (1 non-blocking nit addressed inline)

Option Y (raw-only) cardinality-correct and canonically-aligned. Non-HTTP audit left 9 metrics with semantic-outcome `status` label per taxonomy §70. Dashboards and alerts already use `status_code`. Catalog updated.

**Nit addressed**: worst-case ~1,050 nudges ADR-0011's 1,000 ceiling. Implementer clarified catalog text with (a) realistic observed <300, (b) anchor example showing why, (c) revisit trigger on new endpoints >5 codes, (d) note on separate bucket-expansion tracking.

---

## Rollback Procedure

1. Start commit: `69c2b0cee11117a1e2cd484c5287bc951c1c4312`
2. Soft reset: `git reset --soft 69c2b0c`
3. Contained to gc-service metrics.rs + gc-overview.json + gc-slos.json + gc-alerts.yaml + gc-service metric catalog + TODO.md.
