# Devloop Output: L7 counter-delta per-instance robustness + fail-loudly

**Date**: 2026-08-29
**Task**: Make Layer-7 counter-delta assertions robust to pod rollovers (per-instance comparison instead of cluster-wide `sum`), and stop masking Prometheus query failures as `0.0` readings. Closes docs/TODO.md §Env-Test Resilience lines 188-189.
**Specialist**: test (paired with observability)
**Mode**: Agent Teams (full) + `--paired-with=observability`
**Branch**: `feature/hear-yourself-through-handler`
**Duration**: TBD

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `1c7794806dbdbcb18a48297b008e97a308e9bd04` |
| Branch | `feature/hear-yourself-through-handler` |
| Lead Model | `claude-opus-4-8[1m]` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` (test) |
| Implementing Specialist | `test` |
| Iteration | `1` |
| Security | `security` — RESOLVED-FIXED |
| Test | `test-reviewer` — RESOLVED-DEFERRED |
| Observability | `paired-observability` — CLEAR |
| Code Quality | `code-reviewer` — RESOLVED-FIXED |
| DRY | `dry-reviewer` — RESOLVED-FIXED |
| Operations | `operations` — RESOLVED-DEFERRED |
| Semantic Guard | `semantic-guard` — CLEAR |

---

## Task Overview

### Objective
Two coupled defects in the Layer-7 counter-delta helpers (docs/TODO.md lines 188-189):

1. **Stale-series inflation across pod rollover** — baseline captured as cluster-wide `sum(...)` instant vector; old-pod series still inside Prometheus staleness window at baseline time expire mid-assertion; fresh pods' counters can never exceed the inflated baseline → false-negative 60s timeout. Loud, never a false pass. Recurred 2026-08-28 (2nd occurrence), forcing the fix.
2. **Fail-loudly violation** — `mc_participant_mh_status_counter` maps a Prometheus query error to `0.0` (`Err(_) => return 0.0`), so a Prometheus outage reads as "counter is 0" and misdirects triage at the service under test.

### Scope
- **Service(s)**: none (test-infra only)
- **Schema**: No
- **Cross-cutting**: Yes — Rust helpers (`crates/env-tests/tests/26_mh_quic.rs`) + TS mirror (`packages/web-app/e2e/mcMetrics.ts`) must keep identical semantics.
- **Explicit non-scope**: no service runtime code, no alert rules, no dashboards.

### Observability anchor (Lead + paired-observability, plan stage)
Prometheus scrape config (`infra/kubernetes/observability/prometheus-config.yaml`) uses `kubernetes_sd_configs` `role: pod` with **no** relabel to a `pod` target_label. The per-series identity label Prometheus auto-attaches is therefore **`instance`** (`__address__` = pod IP:port), distinct per pod and new on every rollover. Per-instance PromQL: `sum by (instance) (mc_participant_mh_status_total{state="..."})`. To be confirmed by paired-observability at Gate 1.

### Affected helpers
- Rust: `mc_participant_mh_status_counter` + `assert_participant_mh_status_increases_past` (~728-770); `mh_notification_counter` + `assert_notification_counter_increases_past` + `wait_for_notification_counter_stable` (~262-340).
- Error-mask sites (`Err(_) => return 0.0`): `26_mh_quic.rs:331`, `26_mh_quic.rs:735`. Sibling-audit candidates: `31_gc_telemetry.rs:324` (`unwrap_or(0.0)`), `30_observability.rs` boolean helpers.
- TS mirror: `promInstantSum`, `pollUntilSumAbove`, consumers, module header.

---

## Final Summary

**Phase**: complete · **Verdicts**: Security RESOLVED-FIXED · Test RESOLVED-DEFERRED · Observability CLEAR · Code Quality RESOLVED-FIXED · DRY RESOLVED-FIXED · Operations RESOLVED-DEFERRED · Semantic Guard CLEAR (no ESCALATED).

**Outcome**: Both counter-delta helper families (Rust `26_mh_quic.rs` + TS `mcMetrics.ts`) now capture the baseline as a per-instance map via `sum by (instance)(...)` and pass when any present instance beats its own baseline (fresh post-rollover pod passes at value>0) — killing the stale-series-inflation flake (2 occurrences, 2026-08-05 & 08-28) without adding a false-pass class (one narrow residual window documented, not papered over). Prometheus query errors now fail loud (panic/throw naming PromQL + error), empty-vector-is-zero preserved and unit-tested. The pure decision (`any_instance_exceeds_baseline` / `anyInstanceExceedsBaseline`) is the SSoT per language, each with deterministic no-cluster unit tests; a Rust↔TS non-finite-parse divergence was caught at Gate 3 and pinned. Closes docs/TODO.md 188/189 (2 of 3 defect-class sites; the GC 3rd site is a tracked same-owner follow-up). Gate-2 re-validation green incl. Layer-7 `26_mh_quic` + browser E2E vs live cluster.

**Non-scope honored**: no service runtime code, no alert rules, no dashboards. Sole cross-boundary edit: `docs/runbooks/devloop-validation.md` (operations F1, hunk-ACKed + trailer).

**Not committed by this devloop** (unrelated, pre-existing at session start): `docs/user-stories/2026-08-27-hear-yourself-through-handler.md` (story-runner's task-1 escalation state) — left for the runner/story-close.

---

## Cross-Boundary Classification

All changed code files are test-owned (implementer is the test specialist; `packages/web-app/e2e/**` and the env-tests crate are test-domain). No Guarded Shared Area paths; no service runtime code, alert rules, or dashboards. (`docs/TODO.md` and the user-story manifest are guard-exempt append/tracking targets.)

| Path | Classification | Owner |
|------|----------------|-------|
| `crates/env-tests/src/fixtures/metrics.rs` | Mine | test |
| `crates/env-tests/tests/26_mh_quic.rs` | Mine | test |
| `packages/web-app/e2e/instanceCounters.ts` | Mine | test |
| `packages/web-app/e2e/mcMetrics.ts` | Mine | test |
| `packages/web-app/tests/instanceCounters.test.ts` | Mine | test |
| `packages/web-app/e2e/join-happy-path.spec.ts` | Mine | test |
| `packages/web-app/e2e/mc-token-rejection.spec.ts` | Mine | test |
| `docs/runbooks/devloop-validation.md` | Minor-judgment | operations |

Cross-boundary note: `docs/runbooks/devloop-validation.md` (@operations F1 — §8 lane-attribution rows + §6.7 pointers + §11 changelog for the new fail-loud panic). Operations pre-hunk-ACKed the verbatim wording and supplied the trailer `Approved-Cross-Boundary: operations lane-attribution rows for the new fail-loud panic; anti-reverse-masking rule preserved`. Owner on panel + confirmed → Minor-judgment satisfied.

---

## Gate 1 — Plan Approval

| Reviewer | Plan Status |
|----------|-------------|
| Security | deferred-to-Gate-3 (test-infra only, no auth/crypto/secret paths) |
| Test | implementer is test specialist; independent test-reviewer pass at Gate 3 |
| Observability (paired) | **confirmed** — `instance` label + `sum by (instance)` PromQL, soundness + 2 doc-comment caveats delivered |
| Code Quality | deferred-to-Gate-3 |
| DRY | deferred-to-Gate-3 |
| Operations | deferred-to-Gate-3 |
| Semantic Guard | deferred-to-Gate-3 |

**Gate-1 note (Lead):** Contained test-infra-only change (no GSA paths, no service runtime/alert/dashboard code). The one domain reviewer whose input the plan materially depends on — paired-observability — has confirmed the label/PromQL. Full reviewer panel is spawned at Gate 3 to review the actual diff (where a test-infra change gets the most review value). Classification-sanity: all files Mine; no GSA path marked Mechanical → trivially satisfied. Plan approved.

### Approved plan (files)
1. `crates/env-tests/src/fixtures/metrics.rs` — pure `results_to_instance_map`, `any_instance_exceeds_baseline`, fail-loud `instance_counter_map`, `#[cfg(test)]` unit tests (a/b/c + empty).
2. `crates/env-tests/tests/26_mh_quic.rs` — both helper families + `wait_for_notification_counter_stable` → per-instance map; doc comments updated, staleness caveat dropped.
3. `packages/web-app/e2e/instanceCounters.ts` — NEW pure TS mirror (no env import → hermetic).
4. `packages/web-app/e2e/mcMetrics.ts` — per-instance readers, header rewrite, churn-hint reconciled.
5. `packages/web-app/tests/instanceCounters.test.ts` — NEW vitest unit test (a/b/c).
6-7. `join-happy-path.spec.ts`, `mc-token-rejection.spec.ts` — mechanical rename/Map baselines.
8. `docs/TODO.md` — close lines 188 + 189.

Audit (no change, justified): `31_gc_telemetry.rs:324` already fail-loud (`.expect`); `30_observability.rs` `Err(_)=>false` inside `assert_eventually` retry closures (legit readiness contract).

### Scope decision (Lead, devloop-local — NOT a deferral)
The fix converts ALL call sites of the shared counter-delta helper, including two series beyond TODO 188-189's named pair (`mc_participant_mh_status_total`, `mc_mh_notifications_received_total`): also `mc_participant_leaves_total` (unfiltered, per-`reason` multi-series → genuine per-instance aggregation) and `mc_session_join_failures_total{error_type}`. **This is required, not creep:** `promInstantSum`/`pollUntilSumAbove` (TS) and the Rust helper family ARE the shared code being fixed — converting the helper necessarily converts every consumer. Leaving the two extra series on the old cluster-wide-`sum` path would either retain the exact stale-series bug there or force two parallel helper implementations (drift), violating the single-source-of-truth principle the task exists to enforce. Flagged by @paired-observability at plan stage; approved by Lead. Still within the "no service runtime code / alert rules / dashboards" boundary — all four are read-only test-side PromQL selectors.

---

## Gate 3 — Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | **RESOLVED-FIXED** | 2 | 2 | 0 | Both fixed, re-verified by running tests (5 Rust + 5 vitest, incl. both `should_panic`). No GSA path; no secret leak in panic msgs; query-error-as-0.0 mask removed. |
| Test | **RESOLVED-DEFERRED** | 3 (2 subst) | 2 | 1 | F2 NaN/±Inf parity fixed both langs; F3 token tightened; F1 `31_gc_telemetry` = accepted named same-owner follow-up (task-sized: magnitude predicate + GC routing question). Core verified proof-of-trap against source. |
| Observability (paired) | **CLEAR** | 0 | 0 | 0 | Both locked-in points verified; no cross-pod summing; fail-loud seam + `#[should_panic]`; 3 caveats in both families; no scope creep |
| Code Quality | **RESOLVED-FIXED** | 1 (+2 opt) | 1 | 0 | NaN/±Inf fixed both sides (Rust `is_finite()` filter, TS `Number()`+empty-guard+`isFinite`), anomaly cases pinned in both suites; "cannot drift" softened; naming opt withdrawn (not a deferral). ADR-0002/0028/0030/0032/0033 COMPLIANT; 7 files `Mine`, no GSA. |
| DRY | **RESOLVED-FIXED** | 2 (+1 TODO) | 2 | 0 | F1 `poll_until_any_instance_above` extracted (+ per-metric PromQL builders closing a 3rd drift seam); F2 `instance_maps_equal` moved to fixtures + 3 cases. Parity re-checked against source. §Cross-Service-Dup TODO = ADR-0019 extraction (not a deferral). |
| Operations | **RESOLVED-DEFERRED** | 5 | 3 | 1 (+1 routed) | F1 runbook §8/§6.7 both suites + shared `failLoudTail` TS lane clause (fix, hunk-ACK confirmed on final state + trailer), F2 stale TODO (fix), F5 sorted self-describing diag (fix); F3 labelmap guard (accepted deferral, 3-way trigger), F4 stale INDEX (routed→story-close). Deploy surface clean; "loud, never false pass" survives. |
| Semantic Guard | **CLEAR** | 0 | 0 | 0 | Assertion not a tautology (fresh pod passes only on genuine >0 increment); fail-loud real; no comment drift; SSoT parity holds |

---

## Gate 2 — Validation

| Attempt | Result | Layer | Reason | Action |
|---------|--------|-------|--------|--------|
| 1 | FAIL | 2 (format) | `cargo-fmt-failed` — rustfmt line-wrapping in `metrics.rs` + `26_mh_quic.rs` (Layer 1 compile passed) | Routed to @implementer: run `cargo fmt` |
| 2 | FAIL | 3 (guards) | `cross-boundary-scope-drift-inbound-7` — main.md had no machine-readable `## Cross-Boundary Classification` table (Lead plan-doc omission, not implementer code) | Lead added the classification table (7 test-owned files); guard → `no-drift` |
| 3 | **PASS** | — | L1/2/3/5/7 OK; L4/6 `N/A` (proto intentional-gap fold, exit 0). All suites `0 failed`. New Rust unit tests (incl. `should_panic` fail-loud) + new vitest node test ran green. **L7: `env-tests-passed` + `browser-e2e-passed`; `test_mc_media_connection_update_increments_participant_mh_status_metric ... ok`** vs live cluster (472s). | Advance to Gate 3 |

`TOTAL_RESULT=N/A` verdict note: N/A here is the documented self-justifying proto intentional-gap (`REASON=not-applicable-to-this-lang`) outranking OK in the worst-child fold; exit 0. No FAIL (exit 1) or PRECONDITION_FAILURE (exit 2) anywhere. Every verb that ran = OK.

---

## Accepted Deferrals

- `31_gc_telemetry.rs` defect-#1 (stale-series flake) exposure — third site of the counter-delta defect class; task-sized (magnitude predicate + @global-controller one-pod-vs-summed routing question), Lead-approved named same-owner follow-up. Body → `docs/TODO.md` §Env-Test Resilience (authoritative) + §Cross-Service Duplication pointer. Forces @test-reviewer → RESOLVED-DEFERRED.
- Labelmap-collision guard (@operations F3) — real control is a guard, not a fourth unenforced comment (ADR-0036); config infra/observability co-owned, infrastructure not on panel. Body → `docs/TODO.md` §Env-Test Resilience (names both `prometheus-config.yaml` + `docker/prometheus.yml` surfaces). Owner: operations (guard) + infrastructure/observability co-sign.

## Scope & Process Decisions (devloop-local)

### Lead scope correction (2026-08-29, Gate 3 re-validation)
The implementer's revision initially edited 4 not-mine files. Lead reverted 2 as beyond the approved dispositions, keeping the devloop's cross-boundary footprint to the single owner-on-panel file:
- REVERTED `infra/kubernetes/observability/prometheus-config.yaml` (F3 interim comment) — F3 was deferred in full to the guard TODO; an unenforced comment is not the control (ADR-0036), infrastructure not on panel, and @operations' own position was "neither surface is changing in this diff."
- REVERTED `docs/specialist-knowledge/{client,dry-reviewer}/INDEX.md` (F4) — INDEX curation is the story-close home; client/dry-reviewer-owned; symmetric-excluded from the scope guard. Recorded as story-close follow-up below.
- KEPT `docs/runbooks/devloop-validation.md` (F1) — operations-owned, pre-hunk-ACKed with classification row + `Approved-Cross-Boundary:` trailer. Only cross-boundary edit in the final diff.

## Story-close follow-ups (routed to /close-story Phase 2, not fixed here)

- **Stale INDEX pointers (@operations F4)** — `docs/specialist-knowledge/client/INDEX.md:43` and `dry-reviewer/INDEX.md:44` point at the deleted `pollUntilSumAbove`; the new SSoT module `packages/web-app/e2e/instanceCounters.ts` is registered in no INDEX. INDEX reflection is the story-close home (SKILL §Story-scope reflection; INDEX files symmetric-excluded from the scope guard); owned by client/dry-reviewer/test, not editable mid-devloop by the test implementer. At story close: repoint the two dead references to `pollUntilAnyInstanceAbove`/`instanceCounters.ts` and register the new module.
