# Devloop Output: ADR-0032 Step 5 — GC Metric-Test Backfill

**Date**: 2026-04-27
**Task**: Drain 25 uncovered GC metrics to 0 via per-failure-class component tests + per-service Cat B token-refresh extraction + `get_guest_token()` production instrumentation. Closes the long-lived `feature/mh-quic-mh-tests` branch — `run-guards.sh validate-metric-coverage` transitions from "gc-service: 25 only" red to fully green; branch then ready to merge to mainline.
**Specialist**: global-controller
**Mode**: Agent Teams (full)
**Branch**: `feature/mh-quic-mh-tests` (Option C — final phase; closes the branch)
**Duration**: 1 day (2026-04-27 single session, multi-iteration fix-or-defer cycle)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `16a783eb71bce1b3dc5c02058c79f45637b2ac83` |
| Branch | `feature/mh-quic-mh-tests` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `closed` |
| End Commit | `6062630` (reflection trims) on top of `48f1250` (Step 5 main) |
| Implementing Specialist | `global-controller` |
| Iteration | `1` |
| Security | `CLEAR` |
| Test | `RESOLVED` |
| Observability | `CLEAR` |
| Code Quality | `RESOLVED` |
| DRY | `CLEAR` |
| Operations | `RESOLVED` |

### Gate 2 Validation (2026-04-27)

| Layer | Result |
|-------|--------|
| L1 cargo check | ✅ |
| L2 cargo fmt --all | ✅ (auto-fixed: 2 files) |
| L3 guards (16) | ✅ all green — `validate-metric-coverage` GREEN for first time on branch |
| L4 workspace tests | ✅ (1 retry on AC `test_issue_user_token_timing_attack_prevention` flake — unrelated to Step 5; passed clean retry) |
| L5 clippy `-D warnings` | ✅ |
| L6 cargo audit | 7 pre-existing transitive vulns (quinn-proto, ring, rsa, rustls-webpki) — no new vulns from Step 5 (only Cargo.toml change is enabling existing `test-utils` feature) |
| L7 semantic-guard | ✅ (manual: no credential leaks, no sync-in-async, label semantics clean across 16 emission sites — user/guest never swapped, no error-context regressions) |
| L8 env-tests | ✅ 110 passed / 0 failed (1 retry on Loki `/ready` 503 → infrastructure timing, did not consume budget) |

---

## Task Overview

### Objective
Bring `gc-service` to 0 uncovered metrics under `scripts/guards/simple/validate-metric-coverage.sh`, with per-failure-class assertion fidelity. Match MH Step 2 / MC Step 3 / AC Step 4 quality bar. After Step 5 lands, `run-guards.sh` goes fully green for the first time on this branch.

### Scope
- **Service(s)**: gc-service (production code changes expected: closure simplification in `main.rs`, Cat B fn extraction in `observability/metrics.rs`, `get_guest_token` instrumentation in `handlers/meetings.rs`)
- **Schema**: No
- **Cross-cutting**: No (`crates/common/` untouched — Cat B is per-service parallel sibling, NOT cross-service consolidation)

### Debate Decision
NOT NEEDED — ADR-0032 establishes the design; this is phasing step 5 (final).

### Uncovered Metrics (25)

`gc_ac_request_duration_seconds`, `gc_ac_requests_total`, `gc_caller_type_rejected_total`, `gc_db_queries_total`, `gc_db_query_duration_seconds`, `gc_errors_total`, `gc_grpc_mc_call_duration_seconds`, `gc_grpc_mc_calls_total`, `gc_http_request_duration_seconds`, `gc_http_requests_total`, `gc_jwt_validations_total`, `gc_mc_assignment_duration_seconds`, `gc_mc_assignments_total`, `gc_meeting_creation_duration_seconds`, `gc_meeting_creation_failures_total`, `gc_meeting_creation_total`, `gc_meeting_join_duration_seconds`, `gc_meeting_join_failures_total`, `gc_meeting_join_total`, `gc_mh_selection_duration_seconds`, `gc_mh_selections_total`, `gc_registered_controllers`, `gc_token_refresh_duration_seconds`, `gc_token_refresh_failures_total`, `gc_token_refresh_total`

### Production code changes expected
- `crates/gc-service/src/main.rs` (closure simplification — replace inline closure body with call to per-service `record_token_refresh_metrics`)
- `crates/gc-service/src/observability/metrics.rs` (Cat B fn extraction + in-src cluster tests migration)
- `crates/gc-service/src/handlers/meetings.rs` (`get_guest_token` instrumentation — ~5-15 LoC, mirroring `join_meeting` pattern)

### Quality bar (matches Steps 2-4)
- Per-failure-class fidelity (every label combo emits via real production path, asserted via `MetricAssertion::snapshot()` with partial-label `assert_delta(0)` adjacency on siblings)
- `flavor = 'current_thread'` tokio runtime pinning + load-bearing file-header comment on every test file
- Histogram-first ordering in mixed-kind snapshots (drain-on-read)
- Real-recording-site drives over wrapper-Cat-C smoke; wrapper-Cat-C only as fallback for genuinely-unreachable paths, with explicit WRAPPER-CAT-C framing comment + TODO entry sized by-LoC for follow-up
- `assert_unobserved` on failure-path adjacency for any gauges (API now symmetric across counter/gauge/histogram per Step 4 iter-4)

### Plan-stage commitment fidelity (load-bearing)
Shipping reduced fidelity without explicit re-scoping discussion is a discipline failure (per AC iter-2 lesson). Each plan-stage commitment is a Gate-2 review gate.

### TODO entries to close at devloop close (verify each in docs/TODO.md)
- `**TokenManager::with_on_refresh closure + per-service record_token_refresh wrappers (3-service duplication)**` — closing line: "Resolved 2026-04-27 via ADR-0032 Step 5: per-service `record_token_refresh_metrics` is the canonical pattern; cross-service consolidation rejected as low-value abstraction over a 3-line closure."
- `get_guest_token` uninstrumented gap (referenced in `docs/specialist-knowledge/observability/INDEX.md:14` — update INDEX too)

### Out of scope
- Typed-label-source weak-form rule for ADR-0011 (TODO entry surfaced during Step 4 iter-4) — next architectural priority after Step 5.

---

## Planning

### Production-code changes (3 originally-scoped + 1 authorized expansion pending)

1. **`crates/gc-service/src/main.rs:124-128` (Cat B)**: extract closure body to per-service `record_token_refresh_metrics(&TokenRefreshEvent)` parallel sibling to MH/MC. Closure becomes `move |event| record_token_refresh_metrics(&event)`. Pure extraction, byte-identical, no canary required.

2. **`crates/gc-service/src/observability/metrics.rs`**: add `record_token_refresh_metrics`; replace 18 legacy no-recorder smoke tests with per-cluster `MetricAssertion`-backed tests mirroring AC's `metrics_module_emits_*_cluster()` shape.

3. **`crates/gc-service/src/handlers/meetings.rs:get_guest_token()`**: instrument with `record_meeting_join` mirroring `join_meeting`. ~10-15 LoC. Reuses existing metric family.

4. **PENDING @team-lead authorization (scope expansion A)** — `participant=user|guest` label on `gc_meeting_join_*` family per @observability ask:
   - Add `participant: &str` parameter to `record_meeting_join` wrapper.
   - Migrate 8 existing `join_meeting` call sites to pass `"user"`.
   - 7 new `get_guest_token` call sites pass `"guest"`.
   - New `guests_disabled` reason value on `gc_meeting_join_failures_total`.
   - Update `docs/observability/metrics/gc-service.md:203-245` catalog.
   - Dashboard `gc-overview.json` queries unchanged (Prometheus aggregates over new label by default); a `by(participant)` panel addition flagged-and-deferred.

### Test scope: 13 cluster files under `crates/gc-service/tests/`

| # | File | Metrics |
|---|---|---|
| 1 | `http_metrics_integration.rs` | `gc_http_requests_total`, `gc_http_request_duration_seconds` |
| 2 | `mc_assignment_metrics_integration.rs` | `gc_mc_assignment_duration_seconds`, `gc_mc_assignments_total` |
| 3 | `db_metrics_integration.rs` | `gc_db_queries_total`, `gc_db_query_duration_seconds` |
| 4 | `errors_metric_integration.rs` | `gc_errors_total` |
| 5 | `grpc_mc_call_metrics_integration.rs` | `gc_grpc_mc_calls_total`, `gc_grpc_mc_call_duration_seconds` |
| 6 | `jwt_validation_metrics_integration.rs` | `gc_jwt_validations_total` |
| 7 | `caller_type_rejected_metrics_integration.rs` | `gc_caller_type_rejected_total` |
| 8 | `mh_selection_metrics_integration.rs` | `gc_mh_selection_duration_seconds`, `gc_mh_selections_total` |
| 9 | `ac_request_metrics_integration.rs` | `gc_ac_requests_total`, `gc_ac_request_duration_seconds` |
| 10 | `meeting_creation_metrics_integration.rs` | `gc_meeting_creation_total`, `gc_meeting_creation_duration_seconds`, `gc_meeting_creation_failures_total` |
| 11 | `meeting_join_metrics_integration.rs` | `gc_meeting_join_total`, `gc_meeting_join_duration_seconds`, `gc_meeting_join_failures_total` |
| 12 | `registered_controllers_metrics_integration.rs` | `gc_registered_controllers` |
| 13 | `token_refresh_integration.rs` | `gc_token_refresh_total`, `gc_token_refresh_duration_seconds`, `gc_token_refresh_failures_total` |

### `db_queries` per-op drivability classification (per @test sign-off)

| Op | Disposition | Rationale |
|---|---|---|
| `count_active_participants` | wrapper-Cat-C | Only `?` propagation from `sqlx::query_as` |
| `add_participant` | driven error | FK violation drivable |
| `remove_participant` | wrapper-Cat-C | UPDATE no-match returns `Ok(rows_affected=0)` |
| `create_meeting` | driven error | unique-collision drivable |
| `log_audit_event` | driven error | FK violation drivable |
| `activate_meeting` | wrapper-Cat-C | UPDATE WHERE no-match returns `Ok(None)` |
| `register_mh` | wrapper-Cat-C | Idempotent UPSERT |
| `update_load_report` | wrapper-Cat-C | UPDATE no-match returns `Ok(rows_affected=0)` |
| `mark_stale_mh_unhealthy` | wrapper-Cat-C | UPDATE no-match returns `Ok(rows_affected=0)` |

6 success drives + 2 driven errors + 9 wrapper-Cat-C (post-F1 reclassification 2026-04-27 per @test): the entire `ParticipantsRepository` (`count_active_participants`, `add_participant`, `remove_participant`) is orphan — zero production callers (`crates/gc-service/src/repositories/mod.rs:21` "will be used in meeting join handler"). All 3 success drives + the 1 originally-driven FK error collapse to wrapper-Cat-C with orphan-recording-site canonical comment block (3 success-wiring proofs + 0 driven errors on the orphan side). Two driven errors remain on real production callers (`create_meeting` unique-collision, `log_audit_event` FK violation). 4 wrapper-Cat-C error stubs cover the no-business-error-branch ops. New TODO entries: orphan recording-site audit + cap-clamping-on-actual_type, both in `docs/TODO.md`.

### Test-fixture consolidation (AUTHORIZED by @team-lead — option 1)

Pre-existing 3-copy duplication of `TestKeypair` + `build_pkcs8_from_seed` + `TestUserClaims` across `meeting_tests.rs`, `auth_tests.rs`, `meeting_create_tests.rs` (lines documented in @dry-reviewer thread). Consume from a new `crates/gc-service/tests/common/mod.rs` (mirrors AC's `tests/common/`). Scope:
- New module: `tests/common/jwt_fixtures.rs` (TestKeypair, build_pkcs8_from_seed, TestUserClaims, TestServiceClaims, signing helpers).
- 3 in-place migrations (existing tests): import-line additions + remove the inline copies. NO functional changes; if an existing test needs more than fixture-import-line touches, flag and discuss.
- New cluster files import from common where JWT signing is required (~3-5 of 13).

Net diff: ~0 LoC for fixtures (consolidation - duplicate-removal); +13 cluster files.

### Cargo.toml addition

`common = { path = "../common", features = ["test-utils"] }` to `[dev-dependencies]` in `crates/gc-service/Cargo.toml` (mirrors AC line 61).

### Quality bar (all matches Steps 2–4)

- Per-failure-class fidelity, every label combo via real production path, asserted via `MetricAssertion::snapshot()` with partial-label `assert_delta(0)` adjacency.
- `flavor = "current_thread"` pinning + load-bearing file-header comment on every test file (including `#[sqlx::test]` files per @test ask).
- Histogram-first ordering in mixed-kind snapshots.
- Real-recording-site drives over wrapper-Cat-C smoke; wrapper-Cat-C only with explicit framing comment + sized TODO entry.
- `assert_value(0.0)` for explicit zero-fill paths; `assert_unobserved` for failure-path code paths that don't touch the metric (per @observability + @code-reviewer guidance on `gc_registered_controllers`).
- 4-cell adjacency-coverage matrix on `gc_registered_controllers` per @code-reviewer: full happy path, partial counts (zero-fill), empty counts, caller-error-before-update.
- `join_meeting` ↔ `get_guest_token` parity table sent to @code-reviewer (semantic differences: no `unauthorized` on guest, new `bad_request` and `guests_disabled` on guest).

### TODO closures at devloop end

- `docs/TODO.md:38` "TokenManager::with_on_refresh closure + per-service record_token_refresh wrappers (3-service duplication)" — close with: "Resolved 2026-04-27 via ADR-0032 Step 5: per-service `record_token_refresh_metrics` is the canonical pattern; cross-service consolidation rejected as low-value abstraction over a 3-line closure."
- `docs/specialist-knowledge/observability/INDEX.md:14` `get_guest_token` uninstrumented gap entry — close with note that the metric family is now shared across user/guest paths via the `participant` discriminator label (subject to scope-expansion-A authorization).
- New TODO: GC db_query wrapper-Cat-C closure (~6 ops, ~30 LoC test net) once fault-injection harness exists.

### Reviewer status

| Reviewer | Status | Notes |
|---|---|---|
| @code-reviewer | RESOLVED (post-F1 + F2) | Refinement-1 (parity-note), Refinement-2 (COVERAGE GAP), Cat B doc-comment parity all landed |
| @test | RESOLVED (post-F1 + F2 + F3 + F4 + F5) | All 3 ParticipantsRepository ops demoted to wrapper-Cat-C orphan; uniform adjacency; orphan-recording-site TODO entry |
| @observability | CLEAR (post-F1) | Catalog `gc_caller_type_rejected_total` cardinality-note honesty amendment + TODO entry for clamping fix |
| @dry-reviewer | CLEAR | Path Y completion landed: 9→6 cross-crate TestKeypair locations; mechanical migration discipline preserved |
| @security | CLEAR | No new common imports beyond MetricAssertion + TokenRefreshEvent; bounded label values; Cat B byte-identical |
| @operations | RESOLVED | Runbook Scenario 5 updated; Cat A canary acceptance criteria; new-time-series footnote per @code-reviewer FYI |
| @team-lead | Gate 3 advanced | Path A (add_participant orphan reclassification) + Path Y (TestKeypair 9→6) + free-fn deletion all authorized |

---

## Pre-Work

None.

---

## Implementation Summary

> **Step 5 includes 2 scope expansions authorized at plan stage** (per
> @team-lead's "Plan approved (final form)" message): (A) `participant=user|guest`
> label on `gc_meeting_join_*` family per @observability rationale
> (alert/triage signal preservation, cardinality stays under ADR-0011 cap-10);
> (B) per-crate test-fixture consolidation per @dry-reviewer rationale (avoid
> worsening the existing 3-copy `TestKeypair` triplication by 4-13× when
> adding 13 new cluster files). Both expansions were flagged at plan stage by
> the implementer; both were authorized by team-lead before implementation
> began. The `by(participant)` Grafana panel addition is flagged-and-deferred
> to a sized TODO entry per the explicit team-lead constraint. The
> `auth_tests.rs` + `meeting_tests.rs` in-place migrations were scope-capped
> to `meeting_create_tests.rs` only (1-of-3) per a follow-on flag during
> implementation; the other two files retain attack-vector-specific
> `TestKeypair` helpers (`create_hs256_token`, `create_token_with_wrong_key`,
> `create_tampered_token`) — see TODO entry "GC tests/common partial
> in-place migration."

### Production code (3 files)

1. **`crates/gc-service/src/main.rs`** — closure simplified to `move |event| record_token_refresh_metrics(&event)`. Cat B byte-identical extraction.

2. **`crates/gc-service/src/observability/metrics.rs`**:
   - Added `record_token_refresh_metrics(&TokenRefreshEvent)` per-service dispatcher (Cat B parallel sibling to MH/MC).
   - Modified `record_meeting_join` signature to add `participant: &str` parameter (per @observability + @team-lead authorization). Three emitted metrics now carry the `participant` label.
   - Replaced 18 legacy no-recorder smoke tests in `mod tests` with 25 per-cluster `MetricAssertion`-backed tests, including the 4-cell `gc_registered_controllers` adjacency matrix and a per-error-category token-refresh failure matrix. All pass.

3. **`crates/gc-service/src/handlers/meetings.rs`**:
   - 8 existing `join_meeting` call sites migrated to pass `"user"`.
   - `get_guest_token` newly instrumented: 7 emission sites + a parity comment block documenting semantic differences vs `join_meeting` (no `unauthorized` on guest path; new `bad_request` and `guests_disabled` error_types unique to guest path).

### Cat A canary acceptance criteria (per @operations Gate-2)

The `get_guest_token` instrumentation is Cat A (new metric emissions on a
previously-uninstrumented handler) but the canary risk envelope is narrow:
the metric family `gc_meeting_join_*` already exists and is already
plotted/alerted; behavior is purely additive; rollback is byte-clean.

**Canary `/metrics` evidence** (operations acceptance):
- `curl localhost:8080/metrics | grep '^gc_meeting_join_total{'` shows
  non-zero counts on at least two `error_type` values reachable via
  `get_guest_token` paths (e.g. `forbidden`/`guests_disabled` from
  `allow_guests=false`, plus `not_found`).
- `curl localhost:8080/metrics | grep '^gc_meeting_join_duration_seconds_count'`
  increments after a successful guest-token request.
- `curl localhost:8080/metrics | grep '^gc_meeting_join_' | wc -l` shows
  the new `participant=guest` label tuples in addition to the
  `participant=user` tuples; cardinality bounded by ADR-0011 cap-10
  (~32 series per histogram).

**Alert thresholds unchanged**: the existing `GCHighJoinFailureRate` (warning,
ratio alert) and `GCHighJoinLatency` (info, percentile alert) alerts absorb
the additive guest-token traffic without tuning. Ratio alerts preserve their
threshold at steady state because both numerator and denominator gain
guest-token contributions; percentile alerts surface real divergence at info
severity, which is the right venue. No `gc-alerts.yaml` modifications.

### Catalog & docs

4. **`docs/observability/metrics/gc-service.md`** — added `participant` label to all 3 `gc_meeting_join_*` rows, enumerated the new `guests_disabled` error_type value, and added a header note that the metric family is shared across user/guest paths (do NOT introduce a parallel `gc_guest_token_*` family).

5. **`docs/specialist-knowledge/observability/INDEX.md:14`** — updated to reflect `get_guest_token` instrumentation + `participant`-label shared family.

### Test infrastructure

6. **`crates/gc-service/Cargo.toml`** — added `common = { path = "../common", features = ["test-utils"] }` to dev-dependencies (mirrors AC's pattern).

7. **`crates/gc-service/tests/common/`** — NEW per-crate test scaffolding:
   - `mod.rs` — module declaration.
   - `jwt_fixtures.rs` — consolidated `TestKeypair`, `build_pkcs8_from_seed`, `TestUserClaims`, `TestServiceClaims` from the previously-triplicated inline copies.
   - In-place migration of `meeting_create_tests.rs` (1-of-3 per @team-lead's revised option-1 scope; the other two retain inline `TestKeypair` + attack-fixture methods unique to their threat-model coverage — see TODO entry).

### 13 cluster test files (Cat C)

| File | Tests | Scope |
|---|---|---|
| `http_metrics_integration.rs` | 7 | Real `http_metrics_middleware` via tower oneshot |
| `mc_assignment_metrics_integration.rs` | 5 | Wrapper-Cat-C label fidelity (success + 3 rejection_reasons + error) |
| `db_metrics_integration.rs` | 18 | 6 success drives + 2 driven errors (`create_meeting` unique-collision, `log_audit_event` FK) + 9 wrapper-Cat-C (3 orphan-recording-site success + 4 no-business-error + 2 driven adjacency siblings; counts collapse with shared MetricAssertion snapshots) — note: total test count is 18 since each orphan-Cat-C site folds success+adjacency into one `#[sqlx::test]` rather than splitting |
| `errors_metric_integration.rs` | 6 | Wrapper-Cat-C label fidelity for all (operation × error_type × status_code) triples |
| `grpc_mc_call_metrics_integration.rs` | 3 | Wrapper-Cat-C: success/rejected/error |
| `jwt_validation_metrics_integration.rs` | 6 | Full bounded label domain (success + 5 failure_reasons) |
| `caller_type_rejected_metrics_integration.rs` | 4 | Wrapper-Cat-C: 4 actual_type values × 2 grpc_services representative cells |
| `mh_selection_metrics_integration.rs` | 3 | success × {has_multiple true,false} + error |
| `ac_request_metrics_integration.rs` | 4 | meeting_token × {success,error}, guest_token × {success,error} |
| `meeting_creation_metrics_integration.rs` | 2 | success + per-error-class matrix (6 error_types) with `assert_unobserved` adjacency |
| `meeting_join_metrics_integration.rs` | 6 | Per-failure-class (participant × error_type) cartesian — including label-swap-bug catcher |
| `registered_controllers_metrics_integration.rs` | 5 | 4-cell gauge adjacency matrix + direct setter |
| `token_refresh_integration.rs` | 2 | Cat B per-service dispatcher representative drives |

### TODO closures + new entries

- `docs/TODO.md` — closed the 3-service `with_on_refresh` duplication entry per the ADR-0032 Step 5 canonical-pattern resolution. Updated the `tests/common/test_state.rs` per-service entry to reflect GC's `jwt_fixtures.rs` sibling. Added two new entries: (a) GC db_query wrapper-Cat-C closure (6 ops, ~30 LoC); (b) GC tests/common partial in-place migration (auth_tests + meeting_tests retain attack-vector helpers).

### Result

`scripts/guards/simple/validate-metric-coverage.sh` is fully green: all 25 GC metrics referenced by tests. AC + MC + MH + GC all pass. Zero compile warnings or clippy findings on `gc-service`. Full GC test suite passes (281 unit + ~155 integration tests).

---

## Files Modified

### Production code
- `crates/gc-service/src/main.rs` — closure simplification (Cat B)
- `crates/gc-service/src/observability/metrics.rs` — `record_token_refresh_metrics` Cat B fn + `participant` label on `record_meeting_join` + per-cluster `MetricAssertion` in-src tests
- `crates/gc-service/src/handlers/meetings.rs` — 8 `join_meeting` call-site migrations + 7 new `get_guest_token` instrumentation sites + parity comment

### Test infrastructure
- `crates/gc-service/Cargo.toml` — `common = { features = ["test-utils"] }` dev-dep
- `crates/gc-service/tests/common/mod.rs` — NEW
- `crates/gc-service/tests/common/jwt_fixtures.rs` — NEW
- `crates/gc-service/tests/meeting_create_tests.rs` — in-place migration to consume `tests/common/jwt_fixtures`

### Cluster test files (NEW)
- `crates/gc-service/tests/http_metrics_integration.rs`
- `crates/gc-service/tests/mc_assignment_metrics_integration.rs`
- `crates/gc-service/tests/db_metrics_integration.rs`
- `crates/gc-service/tests/errors_metric_integration.rs`
- `crates/gc-service/tests/grpc_mc_call_metrics_integration.rs`
- `crates/gc-service/tests/jwt_validation_metrics_integration.rs`
- `crates/gc-service/tests/caller_type_rejected_metrics_integration.rs`
- `crates/gc-service/tests/mh_selection_metrics_integration.rs`
- `crates/gc-service/tests/ac_request_metrics_integration.rs`
- `crates/gc-service/tests/meeting_creation_metrics_integration.rs`
- `crates/gc-service/tests/meeting_join_metrics_integration.rs`
- `crates/gc-service/tests/registered_controllers_metrics_integration.rs`
- `crates/gc-service/tests/token_refresh_integration.rs`

### Catalog & docs
- `docs/observability/metrics/gc-service.md` — `participant` label + `guests_disabled` value documented
- `docs/specialist-knowledge/observability/INDEX.md:14` — `get_guest_token` gap closed + `participant`-label note
- `docs/TODO.md` — 3-service duplication entry closed; partial-migration + db_query Cat-C TODOs added; `tests/common` per-service entry updated

---

## Devloop Verification Steps

- L1 cargo check: ✅
- L2 cargo fmt --all: ✅ (auto-fixed: 2 files)
- L3 guards (16): ✅ all green — `validate-metric-coverage` GREEN for the first time on this branch
- L4 workspace tests: ✅ (1 retry on AC `test_issue_user_token_timing_attack_prevention` flake — unrelated, passed clean retry)
- L5 clippy `-D warnings`: ✅
- L6 cargo audit: 7 pre-existing transitive vulns (quinn-proto, ring, rsa, rustls-webpki); no new vulns from Step 5
- L7 semantic-guard: ✅ (manual: no credential leaks, no sync-in-async, label semantics clean across 16 emission sites — user/guest never swapped)
- L8 env-tests: ✅ 110 passed / 0 failed (1 retry on Loki `/ready` 503 → infrastructure timing, did not consume budget)
- Post-fix-up re-verification: `cargo test -p gc-service` ALL PASS (~485 tests); guard GREEN; clippy + fmt clean; workspace check clean

---

## Code Review Results

| Reviewer | Verdict | Findings landed |
|---|---|---|
| @code-reviewer | RESOLVED | F1(a) parity-note reframed (`forbidden` user-only at source); F1(b) COVERAGE GAP at `generate_guest_id` + `create_ac_client`; F1(c) Option-B per-cell wiring-only annotation; F1(d) test-fixture migration; F2 Cat B doc-comment parity with MH/MC siblings; F3 informational only |
| @test | RESOLVED | F1 ParticipantsRepository orphan-classification expanded to all 3 ops (count_active_participants, add_participant, remove_participant); F2 orphan-recording-site TODO entry; F3 verbatim canonical comment block on all wrapper-Cat-C sites; F4 `_unused_utc` + `chrono::Utc` deletion; F5 uniform `(this_op, error)` `assert_delta(0)` adjacency on all 9 success drives |
| @observability | CLEAR | F1 catalog "Cardinality note" amendment for `gc_caller_type_rejected_total` (aspirational vs enforced); F1 TODO entry for `actual_type` allowlist-clamping; exclusivity invariant test for `guests_disabled` |
| @dry-reviewer | CLEAR | Path Y completion 9→6 cross-crate TestKeypair locations; per-crate fixture consolidation pattern matches AC; attack-vector helpers stay test-file-local |
| @security | CLEAR | Cat B byte-identical extraction; bounded label values; no new common imports beyond `MetricAssertion` + `TokenRefreshEvent`; `Instant::now()` baseline preserved at handler entry |
| @operations | RESOLVED | Runbook Scenario 5 updated with `participant`-axis breakdown query + new-time-series footnote; Cat A canary acceptance criteria; alert math audited (zero `without(...)` patterns) |

---

## Tech Debt

Closed (resolved by Step 5):
- 3-service `record_token_refresh` closure duplication (per-service `record_token_refresh_metrics` is the canonical pattern; cross-service consolidation rejected as low-value abstraction)
- Partial GC integration test fixture duplication (now resolved via Path Y completion)
- `get_guest_token` uninstrumented gap (now shares `gc_meeting_join_*` family via `participant` label)

Added (deferred for follow-up):
- GC db_query wrapper-Cat-C closure (4 of 9 ops with no business-error branch — fault-injection harness ~50 LoC per op)
- GC orphan recording-site audit (3 ParticipantsRepository ops; blocks on production handler integration)
- GC `gc_caller_type_rejected_total{actual_type}` cardinality clamping (~5 LoC `match` over known `service_type` values)
- GC dashboard panel for `gc_meeting_join_*` `participant` axis (deferred until guest-flow traffic non-zero)
- Cross-crate TestKeypair extraction (6 locations remain; gc-test-utils / common test-utils feature pending)

Cardinality status: `gc_meeting_join_failures_total::error_type` at 9 of 10 ADR-0011 cap (1-slot headroom). Tight but compliant; @observability accepted.

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `16a783eb71bce1b3dc5c02058c79f45637b2ac83`
2. Review all changes: `git diff 16a783eb71bce1b3dc5c02058c79f45637b2ac83..HEAD`
3. Soft reset: `git reset --soft 16a783eb71bce1b3dc5c02058c79f45637b2ac83`
4. Hard reset: `git reset --hard 16a783eb71bce1b3dc5c02058c79f45637b2ac83`

---

## Reflection

Step 5 closes the ADR-0032 phase: `validate-metric-coverage.sh` now GREEN across all 4 services (AC/GC/MC/MH), with each service's metrics covered by per-cluster `MetricAssertion`-backed component tests under per-failure-class fidelity. The long-lived `feature/mh-quic-mh-tests` branch is ready to merge to mainline.

Quality bar hit: per-failure-class assertion fidelity, `flavor = "current_thread"` pinning with load-bearing file-header comments, histogram-first ordering in mixed-kind snapshots, real-recording-site drives over wrapper-Cat-C smoke (with explicit framing on the 9 wrapper-Cat-C sites), `assert_unobserved` API symmetry across counter/gauge/histogram. Two scope expansions authorized at plan stage: participant label addition + per-crate test-fixture consolidation. Both landed clean.

Plan-stage commitment fidelity worked as the discipline backbone — 4 plan-stage delta flags surfaced during the cycle (Path A reclassification, Path Y completion expansion, free-fn deletion call, F1 broader-orphan expansion); all flagged to team-lead before silent application, all authorized explicitly, all landed cleanly with verifiable shipped state.

---

## Issues Encountered & Resolutions

**Plan-stage delta: add_participant orphan classification (Path A)**: discovered during @test ask 2 verification that the production fn had zero callers in `crates/gc-service/src/` (only the `// will be used in meeting join handler` comment at `mod.rs:21`). Initial plan had this as a "driven error" classification. Flagged to team-lead with three options; Path A authorized (demote 3 cells to wrapper-Cat-C orphan).

**F1 expansion to all 3 ParticipantsRepository ops**: post-Path-A @test review found that `count_active_participants` and `remove_participant` were also orphan (the entire `ParticipantsRepository` was unused). Expanded the orphan classification to all 3 ops; counts shifted 9 success → 6 success + 3 wrapper-Cat-C orphan.

**Path Y completion (TestKeypair migration)**: initial scope-cap (1-of-3 in-place migrations) was lifted by team-lead based on @dry-reviewer's "mechanical-only" framing. Migration of `auth_tests.rs` (clean) and `meeting_tests.rs` (with attack-helper free-fn extraction) brought count from 9→6 cross-crate locations. During the migration, discovered the 3 free-fn versions on `TestKeypair` had zero call sites — deleted as dead code with team-lead's confirm.

**Reviewer findings against pre-edit-pass state**: both @test (F3) and @code-reviewer (F1 a/b/c/d + F2) submitted findings citing line numbers/wording from pre-edit-pass state. Resolved by replying with grep evidence ("`grep -c \"...\"` returns 0 hits; current text reads:..."), avoiding re-shipping no-op edits.

**Reviewer-deferred edit landed during implementation phase**: @operations flagged that the runbook Scenario 5 update landed during the implementation phase before their explicit Gate-3 confirm, even though they earlier specified the spec text. No harm done, but the protocol-cleaner sequence is implementer-flags-pending → reviewer-confirms-during-finding-cycle → implementer-lands. Captured as durable feedback memory.

**`gc_caller_type_rejected_total{actual_type}` aspirational-vs-enforced gap**: pre-existing production-code defect (predates Step 5) where `claims.service_type` is read directly into the label without allowlist-clamping. @observability F1 deferred the production-code fix (out of Step 5 scope) to a sized TODO entry; required catalog honesty amendment landed.

---

## Lessons Learned

- **Orphan-recording-site classification check**: before classifying any metric-emission-site test as "real seam drive," `git grep` the parent fn's production callers. If zero, ALL tests against that fn are wrapper-Cat-C orphan, not just driven errors. Cheap check (one grep) catches a class of misleading-test patterns at classification time.
- **Plan-stage delta discipline pays off**: each of the 4 deltas (Path A, Path Y, free-fn deletion, F1 expansion) was flagged to team-lead before silent application. Total cost: ~5 small flag-and-confirm exchanges. Total benefit: zero silent scope reductions, zero "AC iter-2 trap" patterns, full reviewer confidence in the shipped state.
- **Pre-stage cite-data only for objective verify items**: line numbers, exact text, grep patterns, count audits, byte-identical extraction claims, single-source pinning, bounded-set membership. Reserve subjective/fidelity items (does main.md preserve spec verbatim, does this comment "earn its place," structural-shape parity) for the reviewer's own audit window — pre-staging those defeats the purpose.
- **Two distinct canonical comment-block variants**: standard wrapper-Cat-C (no-business-error-branch) vs orphan-recording-site (production fn has no caller). Readers should not have to infer which class of un-drivability they're looking at; header docstring index both.
- **Catalog: aspirational vs enforced for unbounded-source labels**: when a Prometheus label is sourced from JWT claims or external input without allowlist-clamping, the catalog should distinguish "expected/legitimate set" from "actually enforced bound." Forged-input cardinality risk lives in the gap.
- **Reviewer messages cross fix-up commits in the queue**: when a reviewer's finding cites text that's already been replaced, respond with grep evidence + framing ("messages crossed in the queue, here's current state"), don't re-ship. Include grep-fingerprints in "edits done" status messages so reviewers can confirm independently.
- **Mechanical migration discipline preserved scope**: @dry-reviewer's tight bounds (mechanical-only, no test-logic refactor, no helper-padding from sibling services, no cross-crate extraction) prevented scope-creep on Path Y. Conversion of `impl TestKeypair` attack-helpers to free fns taking `&TestKeypair` was within the spirit of mechanical (byte-identical bodies, only receiver-style changed) per team-lead's reading.
