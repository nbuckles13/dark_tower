# Devloop Output: MH Logs + SLO Grafana Dashboards (task #62)

**Date**: 2026-08-06
**Task**: Create `mh-logs.json` and `mh-slos.json` — the two per-service Grafana dashboards MH is missing — following the established {ac,gc,mc} patterns; wire them into Grafana provisioning; add a tracked TODO for a dashboard-set completeness guard.
**Specialist**: observability
**Mode**: Agent Teams (full)
**Branch**: `feature/user-story-run-test`
**Duration**: TBD

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `174aadb9ea6d721609cc0526f66393e4cb3c54b8` |
| Branch | `feature/user-story-run-test` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` |
| Implementing Specialist | `observability` |
| Iteration | `1` |
| Security | `security` (CLEAR) |
| Test | `test` (CLEAR) |
| Observability | `observability` (RESOLVED-DEFERRED) |
| Code Quality | `code-reviewer` (RESOLVED-FIXED) |
| DRY | `dry-reviewer` (RESOLVED-DEFERRED) |
| Operations | `operations` (RESOLVED-DEFERRED) |
| Semantic Guard | `semantic-guard` (CLEAR) |

---

## Task Overview

### Objective
Manual testing (2026-08-05) found MH is the only service missing its logs and SLO
dashboards: `infra/grafana/dashboards/` has `{ac,gc,mc}-logs.json` and
`{ac,gc,mc}-slos.json` but only `mh-overview.json` for MH. Close the gap by creating
`mh-logs.json` and `mh-slos.json` (service-owned dashboards per ADR-0031), wiring them
into the Grafana `configMapGenerator`, and tracking a completeness guard as follow-up.

### Scope
- **Service(s)**: MH (observability artifacts only — no MH Rust code changes)
- **Schema**: No
- **Cross-cutting**: No (dashboards + one kustomize manifest edit)

### Debate Decision
NOT NEEDED — additive, pattern-following observability artifacts within existing boundaries (ADR-0031).

---

## Cross-Boundary Classification

<!-- Filled by implementer at planning; Lead validates at Gate 1. -->

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `infra/grafana/dashboards/mh-logs.json` | Mine | — |
| `infra/grafana/dashboards/mh-slos.json` | Mine | — |
| `infra/grafana/kustomization.yaml` | Mine | — |
| `docs/TODO.md` | Mine | — |

Note: `infra/grafana/kustomization.yaml` is a K8s manifest but the edit is limited to the
`grafana-dashboards-mh` `configMapGenerator` block (append two dashboard files, mirroring the
existing {ac,gc,mc} blocks). The bidirectional `validate-kustomize.sh` guard enforces
dashboard↔kustomize coverage, so this edit is mandatory, not optional. Operations is a
reviewer on this loop and sees it at the standard gate.

---

## Planning

**Gate 1 — Plan Approved (2026-08-06).** Classification-sanity guard: `STATUS=OK` (all
rows Mine, no GSA paths). All 7 reviewers confirmed the plan.

Approved plan (4 deliverables):
1. `mh-logs.json` — structural clone of `mc-logs.json` (Log Volume Over Time bars, Recent
   Logs w/ `$pod`+`$level`, Error Logs `level="ERROR"`, Warning Logs `level="WARN"`);
   `app="mh-service"`, `uid: mh-logs`, tags `["mh-service","logs"]`. Kept byte-consistent
   with the mc reference (incl. `level` template-var stream).
2. `mh-slos.json` — 6 gauges modeled on `mc-slos.json`, all `job="mh-service"`, metrics
   verified in `metrics.rs` + catalog: GC heartbeat p95<100ms + success-rate; WebTransport
   handshake p99; token-refresh success-rate + p99; JWT validation success-rate. Counters in
   `rate()`, histogram `_bucket` in `rate()`, units on every non-logs panel, templated
   datasource; SLO dashboard exempt from `$__rate_interval` (uses `[5m]` like mc-slos).
3. `infra/grafana/kustomization.yaml` — add both files under `grafana-dashboards-mh`
   (mirrors {ac,gc,mc}); satisfies bidirectional `validate-kustomize.sh`.
4. `docs/TODO.md` §Observability Debt — tracked (not-implemented) dashboard-set completeness
   guard entry.

Gate-1 decisions:
- **`level` template-var scoping** (raised by @security): keep MH byte-consistent with the
  `mc-logs.json` reference now — scoping only MH's `level` var to `{app="mh-service"}` while
  {ac,gc,mc} stay unscoped would introduce one-off cross-service divergence, contrary to the
  "match the established pattern" requirement. The consistent-across-all-four improvement is
  a separate tracked follow-up, not this task.

---

## Pre-Work

None.

---

## Implementation Summary

Created the two per-service Grafana dashboards MH was missing and wired provisioning:

- **`mh-logs.json`** — 4-panel logs dashboard, structural clone of `mc-logs.json`: Log Volume
  Over Time (level-grouped bars), Recent Logs (`$pod`+`$level` template vars), Error Logs
  (`level="ERROR"`), Warning Logs (`level="WARN"`). `app="mh-service"`, `uid: mh-logs`, tags
  `["mh-service","logs"]`. Kept byte-consistent with the mc reference.
- **`mh-slos.json`** — 6 SLO gauges modeled on `mc-slos.json`: GC heartbeat p95 latency
  (<100ms), GC heartbeat success rate, WebTransport handshake p99, JWT validation success rate,
  token-refresh success rate, token-refresh p99. Counters in `rate()`; histogram `_bucket` via
  `histogram_quantile(..., sum by(le)(rate(...[5m])))`; units on all panels; `$datasource`
  templated. Both p99 gauges use bucket-ceiling (5s) red for one consistent philosophy;
  thresholds cross-checked against `mh-alerts.yaml`.
- **`kustomization.yaml`** — both files added to the `grafana-dashboards-mh` `configMapGenerator`
  (alphabetical, mirrors {ac,gc,mc}); satisfies bidirectional `validate-kustomize.sh`.
- **`docs/TODO.md`** — three §Observability Debt entries + one §Cross-Service Duplication (DRY)
  entry (all tracked, not implemented here) — see §Accepted Deferrals.

---

## Files Modified

```
 M docs/TODO.md
 M infra/grafana/kustomization.yaml
?? infra/grafana/dashboards/mh-logs.json   (new)
?? infra/grafana/dashboards/mh-slos.json   (new)
```
(Dashboards are new/untracked, so `git diff --stat HEAD` does not show them — verified by
direct read + JSON parse after the post-review fix.)

---

## Devloop Verification Steps

**Gate 2 — `./scripts/layer-all.sh` → TOTAL PASS (exit 0).** Per-layer:

| Layer | Result | Notes |
|-------|--------|-------|
| 1 Compile | OK | |
| 2 Format | OK | Prettier + rustfmt clean |
| 3 Guards | OK | incl. `dashboard-panels` → `clean-14-files` (was 12; both new dashboards scanned), `validate-kustomize` → OK (kubeconform env-skip, bidirectional coverage ran) |
| 4 Test | OK | proto SKIPPED-NO-DIFF |
| 5 Lint | OK | cargo clippy + nx lint |
| 6 Audit | N/A(agg) | cargo-audit OK, pnpm-audit OK, buf-breaking OK |
| 7 Env-tests | OK | Rust env-tests OK + browser E2E OK against live cluster (DURATION=847s incl. bring-up) |

Both target guards confirmed green inside Layer 3; no artifact-specific triggers beyond the
kustomize/dashboard guards (dashboards are JSON, kustomization is a generator edit).

---

## Code Review Results

**Gate 3 — all verdicts CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED; none ESCALATED.**

| Reviewer | Verdict | Findings | Fixed | Deferred |
|----------|---------|----------|-------|----------|
| Security | CLEAR | 0 | 0 | 0 |
| Test | CLEAR | 0 | 0 | 0 |
| Observability | RESOLVED-DEFERRED | 2 | 1 | 1 |
| Code Quality | RESOLVED-FIXED | 1 | 1 | 0 |
| DRY | RESOLVED-DEFERRED | 0 | 0 | 0 (1 extraction-opportunity TODO) |
| Operations | RESOLVED-DEFERRED | 2 | 0 | 2 |
| Semantic Guard | CLEAR (native SAFE) | 0 | 0 | 0 |

- **Security**: label-only Loki queries, `$datasource` templated, no secrets/PII/injection; ConfigMap not Secret. No findings.
- **Test**: both dashboards genuinely scanned by `dashboard-panels` (12→14 files); metrics dual-sourced; bidirectional kustomize coverage confirmed independent of the kubeconform env-skip. Surfaced (and tracked) a guard-capability gap: panel label keys/values are not validated against the catalog — folded into the completeness-guard TODO. CLEAR.
- **Observability**: caught a half-applied post-review edit — handshake p99 panel description rewritten but `thresholds.steps` left at yellow=0.5/red=1 — bounced it; fixed to yellow=1/red=5 (verified). Second finding (handshake + token-refresh p99 thresholds are bucket-ceiling markers, not ratified SLOs) accepted as a tracked deferral → RESOLVED-DEFERRED.
- **Code Quality**: same half-applied p99 edit; confirmed fixed. ADR-0031/0029/0011 compliant; kustomization block parity confirmed. RESOLVED-FIXED.
- **DRY**: no accidental duplication (clones fully re-pointed to `mh_*`); per-service copies accepted per ADR-0019 exception; recorded a 4-way `*-logs.json` clone-set extraction pointer. RESOLVED-DEFERRED (driven solely by that TODO pointer).
- **Operations**: `kubectl kustomize infra/grafana/` builds clean, all three MH dashboards provisioned, `disableNameSuffixHash` preserved, no setup.sh regression. Two SLO-alert-gap follow-ups accepted as deferrals → RESOLVED-DEFERRED.
- **Semantic Guard**: diff outside the Rust/TS check remit; applied dashboard-config lens (no secret leak, no PII queries, no concept-substitution) — SAFE → CLEAR.

---

## Accepted Deferrals

At least one reviewer landed on RESOLVED-DEFERRED — accepted deferrals exist (bodies in `docs/TODO.md`):

- `docs/TODO.md` §Observability Debt — MH SLO gauges (handshake/token-refresh p99) use unratified/bucket-ceiling thresholds; reconcile + add p99 burn alerts once ADR-0011 ratifies MH SLOs (observability + operations)
- `docs/TODO.md` §Observability Debt — dashboard-set completeness guard (every service has overview+logs+slos) + `dashboard-panels` panel-label validation gap
- `docs/TODO.md` §Observability Debt — scope `level` template-var to `{app="<svc>-service"}` across all four `*-logs.json` (security)
- `docs/TODO.md` §Cross-Service Duplication (DRY) — `*-logs.json` 4-way clone-set → `_template-service-logs.json` extraction candidate (dry-reviewer)

(The `level`-var Gate-1 scope decision is recorded under §Planning, not here.)

---

## Rollback Procedure

Start commit `174aadb9ea6d721609cc0526f66393e4cb3c54b8`; `git reset --hard` restores. No
schema/migration; dashboards are re-provisioned on next `setup.sh` apply.
