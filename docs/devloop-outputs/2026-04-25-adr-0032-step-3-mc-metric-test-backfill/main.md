# Devloop Output: ADR-0032 Step 3 — MC Metric-Test Backfill

**Date**: 2026-04-25
**Task**: Drain 25 uncovered MC metrics to 0 via per-failure-class component tests, mirroring the MH Step 2 canonical pattern.
**Specialist**: meeting-controller
**Mode**: Agent Teams (full)
**Branch**: `feature/mh-quic-mh-tests` (Option C — long-lived branch until Steps 3-5 complete)
**Duration**: TBD

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `0f66ea1e18071fd9a62b2652eae567a9105b9348` |
| Branch | `feature/mh-quic-mh-tests` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `complete` |
| Implementing Specialist | `meeting-controller` |
| Iteration | `1` |
| Security | `CLEAR` |
| Test | `RESOLVED` (5/5 fixed) |
| Observability | `RESOLVED` (2/2 fixed) |
| Code Quality | `RESOLVED` (3/3 fixed) |
| DRY | `RESOLVED` (1/3 fixed, 2 tech-debt) |
| Operations | `CLEAR` |

---

## Task Overview

### Objective
Bring `mc-service` to 0 uncovered metrics under `scripts/guards/simple/validate-metric-coverage.sh`, with per-failure-class assertion fidelity (every label combo a recorder can emit must have an assertion path that would fail if that combo broke). Match MH Step 2 quality bar.

### Scope
- **Service(s)**: mc-service (tests only; production code touched only for Cat B extraction at `main.rs:147-155` + recorder split + ride-along DebuggingRecorder migration)
- **Schema**: No
- **Cross-cutting**: No (MC-internal; references the existing `crates/common/src/observability/testing.rs` MetricAssertion helper)

### Debate Decision
NOT NEEDED — ADR-0032 already establishes the design; this is phasing step 3.

### Uncovered Metrics (25)

mc_actor_mailbox_depth, mc_actor_panics_total, mc_caller_type_rejected_total, mc_connections_active, mc_errors_total, mc_fenced_out_total, mc_gc_heartbeat_latency_seconds, mc_gc_heartbeats_total, mc_jwt_validations_total, mc_media_connection_failures_total, mc_meetings_active, mc_message_latency_seconds, mc_messages_dropped_total, mc_mh_notifications_received_total, mc_recovery_duration_seconds, mc_redis_latency_seconds, mc_register_meeting_duration_seconds, mc_register_meeting_total, mc_session_join_duration_seconds, mc_session_join_failures_total, mc_session_joins_total, mc_token_refresh_duration_seconds, mc_token_refresh_failures_total, mc_token_refresh_total, mc_webtransport_connections_total

---

## Planning

### Plan Confirmation Status

| Reviewer | Plan Status | Notes |
|----------|-------------|-------|
| Security | confirmed | Q5: self-signed PEMs SAN-scoped to loopback (mirrors MH); will verify Cat B fn signature is pure + no real OAuth/AC creds in mock chain at code-review time |
| Test | confirmed | Q2: Plan C (direct `handle_client_message` call); rig-driven session-join discipline now hard requirement (label-swap-bug catch via `assert_delta(0)` partial-label, histogram-first, current_thread pinning, real injected faults for the 4 error_type variants) |
| Observability | confirmed | G-class split: 3 orphan wrappers (Bucket 1) + 2 Redis-class (Bucket 2), both wrapper-Cat-C with tech-debt entries; sub-finding: 5 actual Redis ops, not 6 (`set` is phantom doc-example) |
| Code Quality | confirmed | 4 plan-level findings: F1 corrected `expired` vs `signature_invalid` JWT mapping (past-`exp` lands in `signature_invalid`, only `IatTooFarInFuture` reaches `expired`); F2 keep `handle_client_message` private + wrapper-call discipline in tests/; F3 MailboxMonitor per-actor-type instance discipline; F4 jwt-validation cluster B/C token_type-scoped snapshot. All resolved before approval. |
| DRY | confirmed | Q4: `tests/common/mod.rs` (NOT mc-test-utils — trait-impl mocks belong with own-crate's traits, mirrors MH's `mock_mc.rs`); 2 tech-debt entries (Cat B clone, accept-loop rig clone — both ADR-0032 §Step 6 anticipated) |
| Operations | confirmed | Q6: rcgen+tempfile dev-deps OK; byte-identical emission; CI-guard MC-drained-AC/GC-red is expected interim state per ADR-0032 phasing |


---

## Pre-Work

None.

---

## Implementation Summary

Drained 25 uncovered MC metric names to 0 via per-failure-class component tests, mirroring the MH Step 2 canonical pattern. Five deliverables:

1. **Cat B extraction**: Added `record_token_refresh_metrics(&TokenRefreshEvent)` next to existing `record_token_refresh()` in `crates/mc-service/src/observability/metrics.rs`. `with_on_refresh` closure at `main.rs:147-155` now calls the pure fn. Production emission byte-identical.
2. **Cat C component tests** (8 new test files + extension to `gc_integration.rs`):
   - `tests/webtransport_accept_loop_integration.rs` — 7 tests covering accept-loop status labels (`accepted`/`rejected`/`error`) + per-failure-class drilldown for `mc_session_join_failures_total{error_type}` (4 variants: `internal`, `meeting_not_found`, `mh_assignment_missing`, `jwt_validation`).
   - `tests/auth_layer_integration.rs` — 8 tests covering every `failure_reason` (`none`, `signature_invalid`, `expired`, `malformed`, `scope_mismatch`) + 3 caller-type-rejected combos.
   - `tests/media_coordination_integration.rs` — 4 tests (`mh_notifications` + `media_connection_failed` wrapper-Cat C per @code-reviewer F2).
   - `tests/register_meeting_integration.rs` — 2 tests via stub MH gRPC server (success + `accepted: false` rejection).
   - `tests/actor_metrics_integration.rs` — 5 tests for actor-system metrics with adjacency.
   - `tests/redis_metrics_integration.rs` — wrapper-invocation Cat C (5 ops + `stale_generation`).
   - `tests/orphan_metrics_integration.rs` — wrapper-only (3 orphan metrics).
   - `tests/token_refresh_integration.rs` — Cat B integration cover.
   - `tests/gc_integration.rs` extended with 4 MetricAssertion-using tests for `mc_gc_heartbeats_total{status,type}` × `mc_gc_heartbeat_latency_seconds{type}`.
3. **Accept-loop component-test rig**: `tests/common/accept_loop_rig.rs` mirrors MH's canonical rig, divergences documented in file header (mock injection at redis/mh_client seams, `MeetingControllerActorHandle` instead of `SessionManagerHandle`, `mc_id`/`mc_grpc_endpoint` constructor args).
4. **`TestServer::accept_loop` fork deletion**: Removed the bypass at `crates/mc-service/tests/join_tests.rs:213-246`; `TestServer` now wraps `AcceptLoopRig`. `MockMhAssignmentStore` + `MockMhRegistrationClient` moved to `tests/common/mod.rs`.
5. **Hand-rolled `DebuggingRecorder` migration**: Replaced `test_prometheus_metrics_endpoint_integration` at `metrics.rs:766-827` with 6 focused per-cluster MetricAssertion tests in `metrics.rs::tests` (`metrics_module_emits_join_flow_cluster`, etc.) + 2 Cat B matrix tests covering all 6 `error_category` variants.

### Discipline applied

- All `MetricAssertion`-using `#[tokio::test]` pinned to `flavor = "current_thread"` explicitly with load-bearing file-header comments.
- Histogram-first ordering enforced in mixed-kind snapshots (drain-on-read).
- Negative `assert_delta(0)` adjacency on every multi-label rig-driven session-join test (label-swap-bug catcher per @test review).
- Real injected faults for the 4 reachable error_type variants (no wrapper shortcuts on the rig path).
- Past-`exp` mapping (`signature_invalid`, NOT `expired`) documented inline per @code-reviewer F1; only `IatTooFarInFuture` reaches `failure_reason="expired"`.
- `handle_client_message` kept private per @code-reviewer F2 (option (b)); decode→match→record path covered by existing `connection.rs::tests:847-892`, wrapper-Cat C in `tests/` for guard satisfaction.
- 5 actually-emitted Redis ops asserted (`get`, `hset`, `eval`, `incr`, `del`); `set` phantom from doc-comment correctly not asserted (caught by @observability).

---

## Files Modified

**Production code (Cat B extraction only — emission byte-identical):**
- `crates/mc-service/src/observability/metrics.rs` — added `record_token_refresh_metrics`; replaced legacy `DebuggingRecorder` test with 8 MetricAssertion tests (6 per-cluster + 2 Cat B matrix).
- `crates/mc-service/src/main.rs` — closure body at `:147-155` reduced to `record_token_refresh_metrics(&event)` call.

**Test infrastructure:**
- `crates/mc-service/tests/common/mod.rs` (new) — moved `MockMhAssignmentStore` + `MockMhRegistrationClient` here from `join_tests.rs`.
- `crates/mc-service/tests/common/accept_loop_rig.rs` (new) — MC-tailored byte-identical rig.
- `crates/mc-service/tests/join_tests.rs` — `TestServer::accept_loop` fork deleted; `TestServer` wraps `AcceptLoopRig`.

**New integration tests:**
- `crates/mc-service/tests/webtransport_accept_loop_integration.rs` (new, 7 tests)
- `crates/mc-service/tests/auth_layer_integration.rs` (new, 8 tests)
- `crates/mc-service/tests/media_coordination_integration.rs` (new, 4 tests)
- `crates/mc-service/tests/register_meeting_integration.rs` (new, 2 tests)
- `crates/mc-service/tests/actor_metrics_integration.rs` (new, 5 tests)
- `crates/mc-service/tests/redis_metrics_integration.rs` (new, 2 tests)
- `crates/mc-service/tests/orphan_metrics_integration.rs` (new, 3 tests)
- `crates/mc-service/tests/token_refresh_integration.rs` (new, 2 tests)
- `crates/mc-service/tests/gc_integration.rs` — extended with 4 MetricAssertion tests at end.

**Cargo.toml:**
- `crates/mc-service/Cargo.toml` — added `rcgen = "0.13"`, `tempfile = "3"` to `[dev-dependencies]` (matches MH).

**Tech debt:**
- `docs/TODO.md` — marked DebuggingRecorder entry done; added 2 new entries (orphans, Redis-class) per @observability sign-off; updated `write_self_signed_pems` entry to reflect MC adoption.

---

## Devloop Verification Steps

| Layer | Command | Verdict | Notes |
|-------|---------|---------|-------|
| 1 | `cargo check --workspace --all-targets` | PASS | |
| 2 | `cargo fmt --all -- --check` | PASS | (after `cargo fmt -p mc-service` applied) |
| 3 | `bash scripts/guards/run-guards.sh` | EXPECTED-FAIL | 15/16 pass; `validate-metric-coverage` reports `mc-service: 0` (drained), AC: 17, GC: 25 unchanged from Step 2 baseline. Lead-accepted interim state per ADR-0032 phasing. |
| 4 | `cargo test -p mc-service` | PASS | 320 pass, 0 failed. |
| 5 | `cargo clippy --workspace --all-targets -- -D warnings` | PASS | |
| 6 | `cargo audit` | PASS | 7 vulnerabilities pre-existing (verified by stash comparison vs baseline; none introduced by Step 3). |
| 7 | semantic-guard agent | SAFE | Cat B byte-identity verified, no secrets, no unsafe, no actor blocking, test isolation correct via per-thread `LocalRecorderGuard`. |
| 8 | env-tests | SKIPPED | Test-only + Cat B scope; matches Step 2 precedent. |

---

## Code Review Results

### Security Specialist
**Verdict**: CLEAR — 0 findings. Cat B fn signature pure (`&TokenRefreshEvent`, no captured state). Accept-loop rig wires only mocks (no real OAuth/AC creds). Self-signed PEMs SAN-scoped to loopback, dev-only.

### Test Specialist
**Verdict**: RESOLVED — 5 findings, 5 fixed.
- F1: load-bearing fidelity gap on `mc_media_connection_failures_total` — production-path emission now asserted via in-file `connection.rs::tests:842-889` with sibling-adjacency.
- F2: misleading "must NOT emit accepted" comment fixed.
- F3: dead `wait_for_join_succeeded` sleep wrapper deleted.
- F4: `actor_metrics_integration.rs` discloses wrapper-only carve-out + points at `crates/mc-service/src/actors/*.rs::tests` for production-path coverage; gauge-absence-assertion API limitation documented inline. Follow-up TODO filed against `MetricAssertion` (`assert_unobserved`/`assert_value_or_unobserved` API addition).
- F5: dead `_force_*` stubs and unused imports removed.

### Observability Specialist
**Verdict**: RESOLVED — 2 findings, 2 fixed.
- F1: Bucket-2 phantom `set` op corrected in `docs/TODO.md`; explicit 4/4/4/2/2 distribution across `get`/`hset`/`eval`/`incr`/`del` documented.
- F2: contradictory comment in `webtransport_accept_loop_integration.rs:314-319` rewritten with accept→error ordering-invariant phrasing.

### Code Quality Reviewer
**Verdict**: RESOLVED — 3 findings, 3 fixed.
- FX1: dead `_force_*` scaffolds — already-fixed in prior round (resolved with @test F5).
- FX2: misleading "must NOT emit accepted" comment fixed.
- FX3: `TestServer` lost `controller_handle.cancel()` on Drop — fixed; `impl Drop for TestServer` calls `self.rig.controller_handle.cancel()` with attribution comment.

**ADR Compliance**: ADR-0032 ✓ (per-failure-class table for all 25 metrics + Cat B byte-identical extraction); ADR-0001 ✓ (no actor wiring touched); ADR-0002 ✓ (no `#[expect]` unfulfilled-lint footgun); ADR-0011 ✓ (label cardinality bounded); ADR-0019 ✓ (mocks correctly scoped to `tests/common/`, near-clones flagged as Step-6 extraction candidates).

### DRY Reviewer
**Verdict**: RESOLVED — 1 fix-class finding fixed, 2 tech-debt observations.

True duplication findings (entered fix-or-defer flow):
- F-DRY-1: TestStack bring-up duplication between `join_tests.rs::TestServer::start` and `webtransport_accept_loop_integration.rs::start_rig`. FIXED via `tests/common/mod.rs::build_test_stack` + `seed_meeting_with_mh` helpers; net ~55 LoC of duplication removed.

Extraction opportunities (tech debt observations):
- F-DRY-2: `mock_token_receiver` now a 4-copy pattern in MC (added `register_meeting_integration.rs`); existing TODO.md entry updated.
- F-DRY-3: `NoopService` test pattern crosses 4 locations; existing TODO.md entry updated with attribution.

### Operations Reviewer
**Verdict**: CLEAR — 0 findings. Production change verified test-and-extraction-only. No infra/runbooks/dashboards/alerts touched. Dev-only deps (`rcgen`/`tempfile`) under `[dev-dependencies]`. CI guard transition is the agreed Step 2→3 phasing-window interim state. Layer 8 skip appropriate. Rollback risk trivial.

---

## Tech Debt

Three new entries landed in `docs/TODO.md`:

1. **§Observability Debt — MC observability orphans — wire production callers or remove**: Three `mc_*` wrappers (`record_message_latency`, `record_recovery_duration`, `record_error`) have ZERO production callers in `crates/mc-service/src/`. Either disposition closes the issue (wire OR remove); deliberately not biased toward one resolution. Test files are wrapper-only at HEAD; replace with real-recording-site drives once production callers exist.

2. **§Observability Debt — MC Redis-class metrics not driven by tests through the production path**: 16 `record_redis_latency` + 2 `record_fenced_out("stale_generation")` production sites in `redis/client.rs` are not exercised by any test that drives the production path. Two paths to close: (a) `redis::aio::ConnectionManager` trait abstraction for fake injection in `client.rs::tests`, or (b) real Redis fixture in `tests/`. Both genuine scope creep beyond Step 3.

3. **§Cross-Service Duplication (DRY) — `write_self_signed_pems` accept-loop TLS helper**: Updated existing entry — MC's accept-loop rig now also uses the helper. Two-call duplication acceptable; consolidation triggers when AC or GC backfills (Steps 4-5) introduce a third caller. Likely target: `common::observability::testing::pem` or a new shared test-utils crate.

Plus marked complete:
- **§Cross-Service Duplication (DRY) — Hand-rolled `DebuggingRecorder` in MC metric-emission tests**: Migrated to `MetricAssertion` per-cluster tests during this step.

Pre-existing entry already covers the Cat B closure duplication post-Step-4 extraction (line 38, "TokenManager::with_on_refresh closure + per-service record_token_refresh wrappers (3-service duplication)") — no new entry needed; that one is updated in spirit by MC's Cat B sibling now existing alongside MH's, completing the 2-of-3 set.

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `0f66ea1e18071fd9a62b2652eae567a9105b9348`
2. Review all changes: `git diff 0f66ea1e18071fd9a62b2652eae567a9105b9348..HEAD`
3. Soft reset (preserves changes): `git reset --soft 0f66ea1e18071fd9a62b2652eae567a9105b9348`
4. Hard reset (clean revert): `git reset --hard 0f66ea1e18071fd9a62b2652eae567a9105b9348`

---

## Reflection

All 7 specialist INDEX.md files updated with new pointers (accept-loop rig, `tests/common/` shared scaffolding, Cat B `record_token_refresh_metrics`, 8 new integration tests, deletion of `TestServer::accept_loop` fork). All files at or below 75-line cap; INDEX guard clean. Operations INDEX updated to reflect Step 2 + Step 3 both complete (MH ✓, MC ✓; AC + GC remaining for Steps 4-5).

A small follow-up landed during reflection: brace-expansion path notation (`{a,b}` syntax) is not handled by `validate-knowledge-index.sh` and was flagged as stale-pointer warnings on first pass. Six reviewers re-ran reflections with explicit per-service pointers; INDEX guard now clean.

---

## Human Review (Iteration 2) — 2026-04-26

**Feedback**: "We should not have metrics that are not used; if nothing else we can't test them, so the every-metric-must-be-tested guard will either complain or we have a pointless test." Disposition for the 3 MC observability orphans logged in §Tech Debt: REMOVE all three (`record_message_latency`, `record_recovery_duration`, `record_error`).

**Mode**: `--continue --light`

**Iter 2 Loop Metadata**:
- Iter 2 start commit: `8e813a2a6716127592d1f3bb678aefb3812b6465`
- Iter 2 team: `adr-0032-step-3-iter2-orphans`
- Iter 2 reviewers: Security + Observability (light mode 3-team: implementer + 2 reviewers)

**Iter 2 Verdicts**:
- Security: CLEAR
- Observability: RESOLVED (1 finding, 1 fixed — F1 stale `mc_errors_total` doc-comment on `McError::status_code()` rewritten to reference the 2026-04-26 removal and the open cross-service error-metric strategy TODO entry; method kept with tightened `#[allow(dead_code)]` justification)

**Iter 2 Implementation Summary**:
- Source: removed wrappers `record_message_latency`, `record_recovery_duration`, `record_error` + their `histogram!`/`counter!` macro emissions + bucket configs (`Matcher::Prefix("mc_message")`, `Matcher::Prefix("mc_recovery")`) + 3 in-module `test_record_*` tests; renumbered remaining smoke-test calls 1→5 from 1→8.
- `crates/mc-service/src/observability/mod.rs`: removed 3 `pub use` re-exports + 3 rows from the metric-summary doc-comment table.
- `crates/mc-service/tests/orphan_metrics_integration.rs`: deleted entirely.
- `crates/mc-service/src/errors.rs`: F1 fix — `McError::status_code()` doc-comment rewritten (no longer claims `mc_errors_total` callsite); `#[allow(dead_code)]` justification updated.
- Dashboards: 6 panels removed from `mc-overview.json`; 7 of 10 panels removed from `mc-slos.json` (the broken-since-emission-was-removed half); MC target trimmed in `errors-overview.json`; "SLO Violations" panel removed from `errors-overview.json`.
- Alerts: 3 alerts removed from `mc-alerts.yaml` (`MCHighLatency`, `MCHighMessageDropRate`, `MCMeetingStale`).
- Catalog: 3 metric entries + 2 PromQL example blocks + 2 SLO definitions + `message_type` cardinality row removed from `docs/observability/metrics/mc-service.md`.
- INDEX: removed orphan-metric tests pointer from `docs/specialist-knowledge/meeting-controller/INDEX.md`.
- TODO: closed "MC observability orphans — wire production callers or remove" with `[x]` + resolution note pointing at iter-2 main.md; opened "MC orphan-metric removal: clean up runbook + ADR-0023/0029 historical refs" as a follow-up scope (4 doc files, mechanical edits, owner: observability); pre-existing "Cross-service error-metric strategy is incoherent — rationalize" remains OPEN as designed.

**Iter 2 Validation Results**:
| Layer | Verdict | Notes |
|-------|---------|-------|
| 1 cargo check | PASS | |
| 2 cargo fmt | PASS | |
| 3 simple guards | EXPECTED-FAIL | 15/16 PASS; `validate-metric-coverage` red because AC: 17, GC: 25 remain uncovered (Steps 4-5 scope); `mc-service: 0` ✓ |
| 4 cargo test -p mc-service | PASS | 312 pass / 0 fail / 2 ignored (was 320 pre-iter-2; net -8 from removed orphan tests) |
| 5 clippy -D warnings | PASS | |
| 6 cargo audit | PASS | 7 vulnerabilities pre-existing (verified vs baseline) |
| 7 semantic-guard | SAFE | Zero leftover call sites, zero dashboard/alert dangling refs, logically pure deletion |
| 8 env-tests | SKIPPED | Test-only + delete-only scope; matches Step 2/3 precedent |

---

## Human Review (Iteration 3) — 2026-04-26

**Feedback**: "We were tracking status in the ADRs and have now pivoted to tracking status via user stories instead." Plus: ADR-0023's 7-row metric table and ADR-0029's exhaustive metric-name lists are maintenance traps. Disposition: amend in place — replace ADR-0023 §11 with a category-level statement + catalog pointer; replace ADR-0029's exhaustive lists with catalog pointers + 1-2 illustrative examples per category.

**Mode**: `--continue --light`

**Iter 3 Loop Metadata**:
- Iter 3 start commit: `ea89907440b15b186530880d7b3d3a1ae7a6b73c`
- Iter 3 team: `adr-0032-step-3-iter3-docs`
- Iter 3 reviewers: Security + Observability (light mode 3-team: implementer + 2 reviewers)

**Iter 3 Verdicts**:
- Security: CLEAR (0 findings) — verified runbook substitutions preserve security signals; no PII/credentials in PromQL; ADR amendments retain security invariants (JWT validations, fence events, caller-type rejections, session-join failures, etc.)
- Observability: RESOLVED (1 advisory, 1 fixed) — A1 minor wording suggestion on ADR-0023 §11 counter bullet ("for failure events" undersells the outcome/lifecycle counters listed); catalog pointer is authoritative so non-blocking. Implementer applied: bullet reworded to "for outcome and failure events (session/meeting lifecycle outcomes, JWT validations, fence events, actor panics, message drops, MC↔MH coordination, token refreshes, caller-type rejections)" at adr-0023:829.

**Iter 3 Implementation Summary**:
- `docs/runbooks/mc-deployment.md`: replaced dead `mc_message_latency_seconds` references with live equivalents (`mc_session_join_duration_seconds`, `mc_redis_latency_seconds`, scoped failure counters) across §8 metrics list, §9 post-deploy checklist, §Rollback degradation criteria, §Monitoring drop-rate / latency PromQL blocks, §Grafana dashboard hints.
- `docs/runbooks/mc-incident-response.md`: §Scenario 1 dropped 2 dead diagnosis steps and renumbered; §Scenario 4 step 2 swapped curl-grep target; §Scenario 5 (High Latency) re-anchored on `MCHighJoinLatency` (info, p95 >2s) + Redis SLO with current PromQL; §Diagnostic Toolkit metrics-grep updated.
- `docs/decisions/adr-0023-meeting-controller-architecture.md` §11: replaced 7-row metric table with category-level statement (gauges / histograms / counters with examples) + pointer to `docs/observability/metrics/mc-service.md`. §Phase 6h NOT touched per scope.
- `docs/decisions/adr-0029-dashboard-metric-presentation.md`: Categories A/B/C and "New Stat Panels" replaced exhaustive lists with rule + pointer + 1 illustrative example each. Dropped dead `mc_message_latency_seconds_count` MC Traffic Summary bullet, replaced with `mc_session_joins_total`.
- `docs/observability/metrics/mc.md`: DELETED. Token-refresh trio (`mc_token_refresh_total`, `mc_token_refresh_duration_seconds`, `mc_token_refresh_failures_total`) merged into `mc-service.md` (new section + PromQL example + SLO entry + cardinality row). Code-side doc-comment at `crates/mc-service/src/observability/metrics.rs:211` updated from `mc.md` → `mc-service.md`.
- TODO closure: §Observability Debt "MC orphan-metric removal: clean up runbook + ADR historical refs" marked `[x]` with one-line resolution.

**Iter 3 Validation Results**:
| Layer | Verdict | Notes |
|-------|---------|-------|
| 1 cargo check | PASS | |
| 2 cargo fmt | PASS | |
| 3 simple guards | EXPECTED-FAIL | 15/16 PASS; `validate-metric-coverage` red (AC: 17, GC: 25 remain — Steps 4-5 scope) |
| 4 cargo test -p mc-service | PASS | 312 pass / 0 fail / 2 ignored (unchanged from iter 2 — doc-only diff) |
| 5 clippy -D warnings | PASS | |
| 6 cargo audit | PASS | 7 pre-existing vulnerabilities |
| 7 semantic-guard | SAFE | `mc.md` migration verified, doc-comment fix path-only, ADR amendments structurally complete |
| 8 env-tests | SKIPPED | Doc-only changes |

**Iter 3 Implementation Summary**:

Doc-only cleanup of the 5 files surfaced in iter 2. No source/dashboard/alert touch beyond a one-line doc-comment path fix in `metrics.rs`.

1. **`docs/runbooks/mc-deployment.md`**:
   - §8 "Check key metrics" — replaced `mc_message_latency_seconds` bullet with `mc_session_join_duration_seconds` + `mc_redis_latency_seconds`.
   - §9 Post-Deployment Checklist — replaced "Message processing latency within SLO (<500ms p95)" with "Session join duration within SLO (p95 < 2s)".
   - §Rollback "Severe performance degradation" — replaced "p95 message latency >1s (2x SLO)" with "p95 session join duration >4s (2x SLO)" + "Redis p99 latency >50ms (5x SLO)".
   - §Monitoring "Message drop rate" — replaced rate(mc_messages_dropped_total)/(...+ mc_message_latency_seconds_count) ratio with simpler `sum by(actor_type) (rate(mc_messages_dropped_total[5m]))` (dropped denominator since the message-rate proxy no longer exists).
   - §Latency block — replaced p95 message-processing query with session-join p95 (status="success") and Redis p99 queries.
   - §Grafana Dashboards descriptions — updated "MC Overview" and "MC SLOs" hints to current panel surface.

2. **`docs/runbooks/mc-incident-response.md`**:
   - §Scenario 1 Diagnosis — removed steps 3 ("Check message processing rate vs incoming rate") and 4 ("Check for slow message processing") which depended on `mc_message_latency_seconds_count`/`_bucket`. Renumbered remaining steps. Updated "Slow Message Processing" root-cause check to use mailbox depth + Redis p99 + session join p95 instead of "p99 processing latency by actor type".
   - §Scenario 4 (Meetings Stuck) Diagnosis step 2 — replaced `grep mc_message_latency_seconds` with `grep -E "mc_session_joins_total|mc_session_join_duration_seconds"`.
   - §Scenario 5 (High Latency) — re-anchored on the live alert (`MCHighJoinLatency` info, p95 >2s) and Redis SLO. Replaced 3 PromQL queries (curl-grep, p95 by actor_type, recovery verification) with current `mc_session_join_duration_seconds` and `mc_redis_latency_seconds` equivalents. Symptoms list rewritten around session-join latency.
   - §Diagnostic Toolkit metrics-grep block — replaced "Message latency metrics" bullet with "Session join metrics" + "Redis op latency" greps.

3. **`docs/decisions/adr-0023-meeting-controller-architecture.md`** §11:
   - Replaced 7-row metric table with the category-level statement (gauges / histograms / counters with examples) and a pointer to `docs/observability/metrics/mc-service.md`.
   - §Phase 6h NOT touched (per task instructions; covered by §Documentation Hygiene "Remove status/tracking from all ADRs" TODO).

4. **`docs/decisions/adr-0029-dashboard-metric-presentation.md`**:
   - Category A: replaced 9-bullet exhaustive metric-name list with a categorical description + catalog pointer + one illustrative example (`ac_token_issuance_total`).
   - Category B: kept derived/normalized rule, swapped the dead `mc_message_latency_seconds`-implicit example for `mc_session_join_duration_seconds_bucket` and `mc_redis_latency_seconds_bucket` as live histogram examples.
   - Category C: appended catalog pointer.
   - "New Stat Panels" — Traffic Summary row: replaced AC/GC/MC bullets with categorical descriptions + one illustrative example each. Dropped the `mc_message_latency_seconds_count` (messages-processed proxy) bullet; replaced with `mc_session_joins_total`. Security Events row reframed categorically.

5. **`docs/observability/metrics/mc.md`** — disposition: **DELETED**. Survey: mc.md was a stale parallel of mc-service.md (mc-service.md has all gauges/heartbeats/redis/fencing PLUS the join-flow / MH coordination / caller-type metrics added in Step 3; mc.md was missing those). The only unique-and-still-live content was the 3 token-manager metrics (`mc_token_refresh_total`, `mc_token_refresh_duration_seconds`, `mc_token_refresh_failures_total`). Merged that section + a token-refresh PromQL example + the Token Refresh Latency SLO + a `error_type` (token refresh) cardinality row into mc-service.md before deleting mc.md. Updated the lone code-side reference (`crates/mc-service/src/observability/metrics.rs:211` doc-comment) from `metrics/mc.md` → `metrics/mc-service.md`.

**TODO updates**:
- §Observability Debt "MC orphan-metric removal" entry: marked `[x]`, body replaced with one-line resolution per task spec.

**Files modified (iter 3)**:
- `docs/runbooks/mc-deployment.md`
- `docs/runbooks/mc-incident-response.md`
- `docs/decisions/adr-0023-meeting-controller-architecture.md` (§11 only; §Phase 6h untouched)
- `docs/decisions/adr-0029-dashboard-metric-presentation.md`
- `docs/observability/metrics/mc-service.md` (token-refresh trio + SLO + cardinality row added)
- `docs/observability/metrics/mc.md` (deleted)
- `crates/mc-service/src/observability/metrics.rs` (doc-comment path fix only — no behavior change)
- `docs/TODO.md` (close orphan-metric entry)

**Verification**: `grep -r "message_latency\|recovery_duration\|mc_errors_total" docs/runbooks/ docs/decisions/adr-0023-meeting-controller-architecture.md docs/decisions/adr-0029-dashboard-metric-presentation.md docs/observability/metrics/` returns no matches except ADR-0023 §Phase 6h (out of scope, tracked separately). Devloop output history files retained as historical record.

---

## Issues Encountered & Resolutions

### Issue 1: Brace-expansion path notation rejected by INDEX guard
**Problem**: First-pass reflection used brace-expansion notation (e.g. `crates/{mh,mc}-service/tests/common/accept_loop_rig.rs`) to compress paired MC/MH pointers. The INDEX guard treats this as a literal path and flagged 9 stale-pointer warnings across 5 INDEX files.
**Resolution**: Reviewers re-ran reflection with explicit per-service pointers (or directory pointers like `crates/mc-service/src/actors/`) on the second pass. INDEX guard now clean.

### Issue 2: F-DRY-1 TestStack bring-up duplication discovered post-rig-landing
**Problem**: After the new accept-loop rig landed, `TestServer::start` and `start_rig` had ~50 LoC of overlapping bring-up logic (JWKS rig spin-up, jwt-validator construction, mock-store seeding).
**Resolution**: Extracted `TestStackHandles` + `build_test_stack(keypair_label)` + `seed_meeting_with_mh(handles, meeting_id)` to `tests/common/mod.rs`. Both call sites collapsed to ~13 lines each.

---

## Lessons Learned

1. The MH Step 2 canonical pattern transferred cleanly to MC with documented divergences (mock injection at redis/mh_client seams, controller-vs-session-manager handle, mc_id/mc_grpc_endpoint constructor args). Documenting MH→MC divergences in the rig file header up front avoided re-litigating them at code review.
2. Per-failure-class fidelity must be the bar, not minimum-effort guard-passing. @test caught a load-bearing fidelity gap on `mc_media_connection_failures_total` at code review (originally wrapper-only, escalated to in-file production-path emission test). The plan's "Plan C" framing wasn't sufficient guarantee — needed the assertion to actually exercise the production fn.
3. Plan-stage scout work pays off. @observability's "5 not 6 redis ops" catch (the `set` phantom in a doc-comment) at planning time would have produced an undetectable test-only artifact if missed.
4. INDEX brace-expansion compression doesn't work; either explicit per-service pointers or directory-level pointers. Document this in the SKILL.md or add brace-expansion handling to `validate-knowledge-index.sh` (latter is observability-team's call).
