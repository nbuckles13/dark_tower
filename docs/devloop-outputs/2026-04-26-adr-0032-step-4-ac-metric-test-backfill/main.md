# Devloop Output: ADR-0032 Step 4 — AC Metric-Test Backfill

**Date**: 2026-04-26
**Task**: Drain 17 uncovered AC metrics to 0 via per-failure-class component tests, mirroring the MH Step 2 / MC Step 3 canonical pattern.
**Specialist**: auth-controller
**Mode**: Agent Teams (full)
**Branch**: `feature/mh-quic-mh-tests` (Option C — long-lived branch through Step 5)
**Duration**: ~4 hours (planning 22:18–22:35, implementation 22:35–23:14, validation 23:14–23:18, review 23:18–00:10, reflection 00:10–00:24)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `630213ce4f00ce652ff957ccab89b4e9cb2e4135` |
| Branch | `feature/mh-quic-mh-tests` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `reflection` |
| Implementer | `complete` |
| Implementing Specialist | `auth-controller` |
| Iteration | `3` (iter-2 fidelity-gap closure + iter-3 DRY Finding-1 (a) + observability dispositions) |
| Security | `RESOLVED` (S-1 closed via path (A) iter-2: 11/11 audit-log production sites) |
| Test | `CLEAR` (all 4 fidelity gaps closed iter-2) |
| Observability | `RESOLVED` (3 closed inline iter-2, 2 deferred with TODOs F2/F4) |
| Code Quality | `CLEAR` (1 non-blocking batched-cleanup nit) |
| DRY | `RESOLVED` iter-3 (Finding 1 fixed via re-introduced parameterized `jwt_fixtures.rs` consumed at 6 sites; Finding 2 TODO landed) |
| Operations | `CLEAR` (test-only, infra untouched, additivity verified) |

---

## Task Overview

### Objective
Bring `ac-service` to 0 uncovered metrics under `scripts/guards/simple/validate-metric-coverage.sh`, with per-failure-class assertion fidelity. Match MH Step 2 / MC Step 3 quality bar. Pure Cat C — no Cat B token-refresh extraction (AC is a token issuer, not a TokenManager consumer).

### Scope
- **Service(s)**: ac-service (tests only; no production code expected to be touched, except possibly `crates/common/src/observability/testing.rs` if `assert_unobserved` API needs to land)
- **Schema**: No
- **Cross-cutting**: Possibly — if the gauge-absence-assertion gap blocks coverage of AC's 4 gauge metrics, `crates/common/src/observability/testing.rs` may need a small API addition. Implementer + test reviewer decide.

### Debate Decision
NOT NEEDED — ADR-0032 establishes the design; this is phasing step 4.

### Uncovered Metrics (17)

ac_active_signing_keys, ac_audit_log_failures_total, ac_bcrypt_duration_seconds, ac_credential_operations_total, ac_db_queries_total, ac_db_query_duration_seconds, ac_errors_total, ac_http_request_duration_seconds, ac_http_requests_total, ac_jwks_requests_total, ac_key_rotation_last_success_timestamp, ac_key_rotation_total, ac_rate_limit_decisions_total, ac_signing_key_age_days, ac_token_issuance_duration_seconds, ac_token_issuance_total, ac_token_validations_total

---

## Planning

Plan converged across one Gate-1 round with 6 reviewers. Material decisions:

- **Pure Cat C** confirmed — `TokenRefreshEvent` / `spawn_token_manager` zero hits in `crates/ac-service/src/`. AC is a token issuer, not a TokenManager consumer. No Cat B extraction.
- **`MetricAssertion::assert_unobserved` API** lands in this devloop (option a) per @test T1 — closes the per-failure-class adjacency rail for gauges (the ADR-0032 §F4 gap MC Step 3 deferred). Symmetric across counter/gauge/histogram per @observability concur, with `ensure_no_kind_mismatch` hardening. Scope-creep guardrail: ONE common-crate API addition, further additions defer to separate PR.
- **Cluster 11 (audit-log)** drives 10 production event_types via `ALTER TABLE auth_events ADD CONSTRAINT block_inserts CHECK (...) NOT VALID` seam (preserves pre-INSERT SELECTs while breaking new INSERTs) + `DROP TABLE auth_events CASCADE` for fns without pre-queries. Real-recording-site, no wrapper-Cat-C smoke.
- **Cluster 13 (credential ops)** organized as 12-cell (operation × status) adjacency matrix per @test T3 + @dry-reviewer.
- **In-`src/` `metrics.rs::tests` migration** to per-cluster `MetricAssertion`-backed tests per @test T5 option (b) — replaces 14 legacy no-recorder smoke tests with 11 cluster tests; preserves 6 path-normalization unit tests.
- **`ErrorCategory` cluster (Cluster 13/errors)** → 4 production-driven tests (one per variant) per code-reviewer CR-4. `Internal` covered via `NotFound` + transitive `From<&AcError>` unit test.
- **Orphan-style label findings** filed as TODO entries (NOT inline-removed): `record_token_validation` (Phase-4-marker reservation, distinct from MC iter-2 stale orphans per code-reviewer CR-1 reconciliation) + `ac_jwks_requests_total{cache_status=hit|bypass}` (CDN/browser-cached at upstream layer; no production reach).
- **`clock_skew` cardinality drift** surfaced during plan review (production emits 5th `error_category` value not in catalog). Disposition pending team-lead; test asserts on production ground truth regardless.

Plan-stage commitment fidelity: each reviewer's confirmed bar gets enforced at Gate 2 review.

---

## Pre-Work

None.

---

## Implementation Summary

13 cluster test files (62 new integration tests after iter-2 scope-fidelity expansion: errors cluster grew from 2 → 4 (CR-4); audit_log cluster grew from 5 → 11 (per @team-lead Gap-1/2 ask + 6 new production-driven event_types); credential_ops cluster gained full 12-cell adjacency on all 11 tests via factored `assert_only_cell` helper (per @team-lead Gap-3 ask)) + in-src `metrics.rs::tests` migration to per-cluster `MetricAssertion`-backed tests (11 new tests, 14 legacy smoke tests removed) + `assert_unobserved` API expansion across all three query types in `common::observability::testing` (3 new methods + 16 module tests).

Per-cluster integration test breakdown:

| Cluster | File | Tests | Notes |
|---------|------|------:|-------|
| HTTP requests | `tests/http_metrics_integration.rs` | 5 | Drives `http_metrics_middleware` via tower oneshot for 200/404/405/500 |
| bcrypt | `tests/bcrypt_metrics_integration.rs` | 3 | Direct `crypto::hash_client_secret` / `verify_client_secret` with `DEFAULT_BCRYPT_COST` (load-bearing for histogram bucket fidelity) |
| token validation | `tests/token_validation_integration.rs` | 3 | Drives `verify_jwt`/`verify_user_jwt` with iat-future tokens; asserts `clock_skew` per production ground truth (catalog drift documented) |
| rate limit | `tests/rate_limit_metrics_integration.rs` | 6 | 3 gates × 2 outcomes per @security hard rule |
| key rotation | `tests/key_rotation_metrics_integration.rs` | 4 | `assert_unobserved` on failure-path gauges per @test T1 |
| JWKS | `tests/jwks_metrics_integration.rs` | 1 | `cache_status="miss"` only (production-reachable; orphan disposition recorded) |
| credential ops | `tests/credential_ops_metrics_integration.rs` | 11 | Full 12-cell (operation × status) adjacency matrix on every test (per @team-lead scope-fidelity iter 2). `assert_only_cell` helper applied uniformly to all 11 tests. |
| errors | `tests/errors_metric_integration.rs` | 4 | Per-`ErrorCategory` variant production-driven coverage (CR-4): Authentication / Authorization / Cryptographic / Internal |
| token issuance (service) | `tests/token_issuance_service_integration.rs` | 3 | `handle_service_token` with success + 2 error paths |
| token issuance (user) | `tests/token_issuance_user_integration.rs` | 2 | `handle_user_token` (password) + `handle_register` (registration) |
| internal tokens | `tests/internal_token_metrics_integration.rs` | 4 | meeting/guest token success + scope-rejection |
| audit failures | `tests/audit_log_failures_integration.rs` | 11 | Real-recording-site drives via `ALTER TABLE auth_events CHECK NOT VALID` and `DROP TABLE` seams. All 10 production sites driven (per @team-lead scope-fidelity iter 2): `key_generated`, `key_rotated`, `key_expired`, `service_registered`, `scopes_updated`, `service_deactivated`, `user_registered`, `service_token_failed`, `service_token_issued`, `user_login`, `user_login_failed` (last two via the parameterized `token_service.rs:362` site). All tests assert per-failure-class adjacency on the 9-10 sibling event_types. |
| db queries | `tests/db_metrics_integration.rs` | 9 | 7 success cells + 2 error cells (DROP TABLE seam, FK violation) |

In-src `metrics.rs::tests` migration:
- Removed 14 legacy `test_record_*` smoke tests (drove wrappers against the global no-op recorder; only proved no-panic).
- Added 11 per-cluster `metrics_module_emits_*_cluster` tests using `MetricAssertion::snapshot()` — mirrors MC Step 3 / MH Step 2 pattern.
- Preserved 6 path-normalization unit tests (`test_normalize_path_*`, `test_is_uuid_*`, `test_normalize_dynamic_path_edge_cases`) — those test path-normalization logic, not metric emission.

API expansion in `crates/common/src/observability/testing.rs`:
- `CounterQuery::assert_unobserved(self)` — hard-form absence assertion (vs soft-form `assert_delta(0)`).
- `GaugeQuery::assert_unobserved(self)` — closes ADR-0032 §F4 gap; `assert_value(0.0)` previously panicked when gauge was never emitted.
- `HistogramQuery::assert_unobserved(self)` — load-bearing drain-on-read caveat documented inline.
- All three include `ensure_no_kind_mismatch` hardening that surfaces kind mismatches with a redirect message.
- 16 new module tests prove the API end-to-end: 4 counter + 4 gauge + 5 histogram (success + panic-when-set + drain-trap proof) + 3 kind-mismatch panic tests (one per kind, parallel to the existing `mismatched_metric_kind_*_panics_clearly` pattern at lines 720-733). Per @test reviewer's final note: kind-mismatch hardening is the load-bearing distinction over `assert_delta(0)` / `assert_observation_count(0)`.
- Module doc-comment §"Unobserved semantics" section added documenting the unifying kind-mismatch invariant + per-kind asymmetry (counter hard-vs-soft form, gauge gap-fill, histogram observation-count equivalence + drain-on-read constraint).

---

## Files Modified

**New integration test files** (all under `crates/ac-service/tests/`):
- `audit_log_failures_integration.rs`
- `bcrypt_metrics_integration.rs`
- `credential_ops_metrics_integration.rs`
- `db_metrics_integration.rs`
- `errors_metric_integration.rs`
- `http_metrics_integration.rs`
- `internal_token_metrics_integration.rs`
- `jwks_metrics_integration.rs`
- `key_rotation_metrics_integration.rs`
- `rate_limit_metrics_integration.rs`
- `token_issuance_service_integration.rs`
- `token_issuance_user_integration.rs`
- `token_validation_integration.rs`

**New test fixtures**:
- `crates/ac-service/tests/common/test_state.rs` — `make_app_state(pool)`, `seed_signing_key(pool)`, `seed_service_credential(pool, client_id, scopes)`, `TEST_CLIENT_SECRET`. Per @dry-reviewer Finding 1 closure (iter 2 option (c)), the previously-shipped `seed_service_credential_with_cost` and `make_app_state_with_default_cost` were removed: every caller passed `MIN_BCRYPT_COST` (so `_with_cost` was a 1-call wrapper); bcrypt-bucket-fidelity tests in `tests/bcrypt_metrics_integration.rs` drive `crypto::hash_client_secret` directly with `DEFAULT_BCRYPT_COST` and never need an `AppState` factory.

- `crates/ac-service/tests/common/jwt_fixtures.rs` — `sign_service_token(pool, master_key, claims)` and `sign_user_token(pool, master_key, user_claims)`. Each fetches the active signing key, decrypts with the supplied master key, and signs the caller-supplied claims via `crypto::sign_jwt` / `sign_user_jwt`. Helpers are parameterized on claims (caller retains full control over `iat` / `scope` / `service_type` / `sub`), de-duplicating the 4-line signing-decrypt-sign block at 6 call sites across `tests/errors_metric_integration.rs` (2), `tests/token_validation_integration.rs` (3), and `tests/key_rotation_metrics_integration.rs` (1). Iter-3 closure of @dry-reviewer Finding 1 with disposition (a) — iter-2 deletion misread the finding (the iter-1 helpers were unused but the duplication itself was real); iter-3 corrects by adding helpers that the call sites now consume.

**Modified**:
- `crates/ac-service/Cargo.toml` — added `common = { path = "../common", features = ["test-utils"] }` to dev-dependencies (enables `MetricAssertion` in test harness).
- `crates/ac-service/src/observability/metrics.rs` — replaced 14 legacy smoke tests with 11 per-cluster `MetricAssertion`-backed tests (preserved 6 path-normalization unit tests).
- `crates/ac-service/tests/common/mod.rs` — added `pub mod test_state;` and (iter-3 re-add per @dry-reviewer Finding 1 (a)) `pub mod jwt_fixtures;`.
- `crates/common/src/observability/testing.rs` — added `assert_unobserved` to all three query types + 16 module tests (4 counter + 4 gauge + 5 histogram + 3 kind-mismatch panic).
- `docs/TODO.md` — closed §F4 entry (gauge absence assertion); added 3 new orphan-disposition entries (token validation wrapper narrow reach, clock_skew cardinality drift, jwks cache_status reservations); iter-2 closed audit-log 5/10 entry; added cross-service `tests/common/test_state.rs` per-service test scaffolding entry per @dry-reviewer Finding 2.

---

## Devloop Verification Steps

```bash
# 1. Format check (after rustfmt applied per-file)
cargo fmt --all -- --check
# → clean

# 2. Clippy across all targets with -D warnings
cargo clippy -p ac-service --all-targets -- -D warnings
cargo clippy -p common --all-targets -- -D warnings
# → clean

# 3. Full ac-service test suite
cargo test -p ac-service
# → 373 lib tests + 144 integration tests across 16 binaries = 517 tests, 0 failures

# 4. In-src per-cluster MetricAssertion tests
cargo test -p ac-service --lib observability::metrics::tests
# → 17 tests pass (11 cluster + 6 path-normalization)

# 5. Metric coverage guard
bash scripts/guards/simple/validate-metric-coverage.sh
# → ac (ac-service): Scanning 17 emitted metric name(s)... [no errors]
#    Remaining 25 errors are all gc-service (Step 5 scope, not ours)
```

---

## Code Review Results

Plan approved on 2026-04-26 by all 6 reviewers (security, test, observability, code-reviewer, dry-reviewer, operations). Conditional items honored:

- **CR-1a** (wrapper-Cat-C framing for `record_token_validation`): Added explicit `// WRAPPER-CAT-C: production callers planned for Phase 4 token-validation endpoint` comment block above `metrics_module_emits_token_validation_cluster` in `crates/ac-service/src/observability/metrics.rs`. Mirrors MC's `media_connection_failed` carve-out pattern. References production call sites at `crypto/mod.rs:284,439` and the production-path coverage in `tests/token_validation_integration.rs`.
- **CR-1b** (TODO entries land in `docs/TODO.md`, not just inline): 3 new orphan-disposition entries added to `docs/TODO.md` §Observability Debt + 1 closed (§F4 gauge absence assertion). Per ADR-0032 §Enforcement trivial-dodge mitigation.
- **CR-3** (consider folding credential_ops into http_metrics): Evaluated and kept separate. `record_credential_operation` is in admin_handler bodies; `record_http_request` fires from `http_metrics_middleware` (outside handlers). Folding would force credential_ops tests through the full router stack with JWT auth middleware, losing the direct-handler-call clarity. Decision documented inline.
- **CR-4** (Cluster 13 = 4 production-driven cases per `ErrorCategory` variant): `tests/errors_metric_integration.rs` now drives all 4 variants from real handler-error paths:
  - **Authentication**: `handle_service_token` with wrong `grant_type` → `AcError::InvalidCredentials` (`auth_handler.rs:232`)
  - **Authorization**: `handle_rotate_keys` with insufficient scope → `AcError::InsufficientScope` (`admin_handler.rs:231`)
  - **Cryptographic**: `handle_rotate_keys` with user JWT (no `service_type`) → `AcError::InvalidToken` (`admin_handler.rs:202`)
  - **Internal**: `handle_get_client` with non-existent UUID → `AcError::NotFound` (transitive coverage of `Database(_)` → `Internal` via `ErrorCategory::from(&AcError::Database(...))` unit test at `observability/mod.rs::tests::test_error_category_database_variant` per the CR-4 carve-out)
  - Each test asserts `assert_delta(0)` adjacency on the OTHER 3 sibling categories (label-swap-bug catcher per ADR-0032 §Pattern #3).

---

## Tech Debt

Three new TODO entries added to `docs/TODO.md` §Observability Debt (orphan-style findings surfaced during plan stage), one entry closed:

- **CLOSED** §F4 — `MetricAssertion` lacks gauge absence-of-emission assertion. Resolved by `assert_unobserved` symmetric API addition.
- **NEW** AC `record_token_validation` wrapper has narrow production reach — production hits only `("error", clock_skew)` despite 5 reserved `error_category` values. Disposition deferred until Phase 4 validation endpoints land.
- **NEW** AC `ac_token_validations_total{error_category}` cardinality drift vs catalog — production emits 5th value `clock_skew` not declared in `docs/observability/metrics/ac-service.md:39`. Reconciliation pending team-lead decision.
- **NEW** AC `ac_jwks_requests_total{cache_status}` reserved-but-unused label values — only `miss` is emitted in production (no cache layer in front of `handle_get_jwks` today). Disposition deferred until JWKS caching lands.
- **CLOSED iter 2** (per @team-lead scope-fidelity ask) AC `ac_audit_log_failures_total{event_type}` — all 10 production event_types now driven (was 5/10 in iter 1). Added 6 new tests for the previously-deferred sites (`key_rotated`, `key_expired`, `scopes_updated`, `service_deactivated`, `user_login`, `user_login_failed`). Existing tests upgraded to `assert_only_event_type` adjacency (`user_registered`, `service_token_failed`, `service_token_issued`). Total audit_log file: 11 tests covering all 10 production sites with 9-10-cell label-swap-bug adjacency.
- **CLOSED iter 2** (per @team-lead scope-fidelity ask) AC `ac_credential_operations_total{operation,status}` — full 12-cell adjacency on every test (was 4-of-11 in iter 1). Factored `assert_only_cell` helper applied uniformly across all 11 tests. The `rotate_secret/error` test (production not-found returns before wrapper fires) explicitly uses `assert_unobserved` on its target cell + `assert_delta(0)` on the 11 siblings.
- **CLOSED iter 2** (per @observability F3) AC `ac_token_issuance_total{grant_type=password|registration,status=error}` — added 2 production-driven error tests (`handle_user_token_bad_credentials_emits_grant_type_password_status_error`, `handle_register_duplicate_email_emits_grant_type_registration_status_error`) to `tests/token_issuance_user_integration.rs`. File grew from 2 → 4 tests covering all 4 (success × {password, registration}) and (error × {password, registration}) cells. Adjacency on success cells confirms error-path doesn't double-fire.
- **CLOSED iter 2** (per @observability F5) — 2 misleading test names renamed: `handle_rotate_keys_missing_auth_emits_status_error` → `handle_rotate_keys_missing_auth_does_not_emit_key_rotation` (the test asserts `assert_unobserved`, not "emits status_error"); `db_query_insert_auth_events_success` → `db_query_insert_auth_events_fk_violation_emits_error` (the test asserts the `error` cell via FK violation, not the success cell).
- **DEFERRED with TODO** (per @observability F2) AC `ac_db_queries_total{operation,table}` — 9 of 12 production (op, table) cells covered; ~9 cell combinations deferred (some reachable today via `users::register_user` / `users::update_last_login`, others Phase-4-gated). TODO entry at `docs/TODO.md` enumerates reachable-today vs Phase-4-gated split.
- **DEFERRED with TODO** (per @observability F4) AC `ac_http_requests_total{method,path,status_code}` — framework-error codes 400 (malformed JSON) / 415 (wrong Content-Type) not driven. Wrapper doc-comment at `metrics.rs:247-251` explicitly mentions these as captured paths. TODO entry at `docs/TODO.md` proposes `http_request_400_for_malformed_json_emits_status_400` and `http_request_415_for_wrong_content_type_emits_status_415` for follow-up devloop.
- **DEFERRED with TODO** (per @code-reviewer CR-7, non-blocking nit) — All 13 AC Step 4 metric-test files use `#![allow(clippy::unwrap_used, clippy::expect_used)]` instead of ADR-0002-preferred `#![expect(...)]`. Matches existing MC Step 3 + MH Step 2 metric-test pattern. Reviewer recommended a batched cross-step migration rather than per-step churn. TODO entry at `docs/TODO.md` §Observability Debt scopes the cleanup to AC + MC + MH metric-test files in one edit.
- **LEARNING — `jwt_fixtures.rs` lifecycle: iterate until the right abstraction emerges** (per @dry-reviewer Finding 1 (a)) — `crates/ac-service/tests/common/jwt_fixtures.rs` went through three states: iter-1 shipped speculative `sign_service_token_iat_offset` / `active_public_key_pem` helpers (5-axis variation, no callers); iter-2 deleted them as dead code; iter-3 re-introduced two simpler 1-axis helpers (`sign_service_token` / `sign_user_token`, parameterized only on caller-supplied claims) once the real call sites had stabilized and the shared 4-line signing-decrypt-sign block became visible. All three steps were correct for their information state — the lesson is that speculative-shape removal AND duplication-extraction are both right, just at different points in the iteration. Don't lock in an abstraction before the call sites exist; don't leave inline duplication once they do.

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `630213ce4f00ce652ff957ccab89b4e9cb2e4135`
2. Review all changes: `git diff 630213ce4f00ce652ff957ccab89b4e9cb2e4135..HEAD`
3. Soft reset: `git reset --soft 630213ce4f00ce652ff957ccab89b4e9cb2e4135`
4. Hard reset: `git reset --hard 630213ce4f00ce652ff957ccab89b4e9cb2e4135`

---

## Reflection

Step 4 closed all 17 uncovered AC metrics to 0 via per-failure-class component tests, then absorbed two reviewer iterations that converted scope-reduction footnotes into either covered tests or load-bearing TODO entries. The work clarified two reusable patterns and one discipline rule:

- **Test architecture**: 13 cluster integration test files under `crates/ac-service/tests/` (one per metric cluster) + an in-src `tests` module in `crates/ac-service/src/observability/metrics.rs` (11 `metrics_module_emits_*_cluster` + 6 preserved path-normalization unit tests, replacing 14 legacy no-op smoke tests). All shared fixtures live at `crates/ac-service/tests/common/test_state.rs` (`make_app_state`, `seed_signing_key`, `seed_service_credential`, `TEST_CLIENT_SECRET`).
- **Ground rule**: drive production sites via direct handler calls + `#[sqlx::test]`, NOT `TestAuthServer` harness — spawned tasks bypass the per-thread `MetricAssertion` recorder. `flavor = "current_thread"` pinning is load-bearing for the per-thread DebuggingRecorder.
- **API addition**: `assert_unobserved` symmetric across CounterQuery/GaugeQuery/HistogramQuery in `crates/common/src/observability/testing.rs` (kind-mismatch hardened, drain-on-read trap proof-tested) closes ADR-0032 §F4 (gauge absence-of-emission gap).
- **WRAPPER-CAT-C framing**: a dedicated comment pattern for orphan-but-Phase-4-reserved wrappers — example landed above `metrics_module_emits_token_validation_cluster` in `crates/ac-service/src/observability/metrics.rs`.
- **Audit-log fault-injection seams**: `break_auth_events_inserts` (`ALTER TABLE auth_events ADD CONSTRAINT block_inserts CHECK (...) NOT VALID` — surgical, preserves pre-INSERT SELECTs) vs `break_auth_events_table` (DROP CASCADE — heavier hammer for sites with no pre-query). The `ALL_EVENT_TYPES` doc-comment in `crates/ac-service/tests/audit_log_failures_integration.rs` is now the authoritative production-site mapping.
- **Discipline**: plan-stage commitment fidelity is load-bearing. "X of Y" footnotes are blockers for Ready-for-validation, not deferrals. Iter-2 closed every reviewer-flagged scope-reduction gap by either backfilling or filing a concrete TODO entry — no silent reductions.

---

## Issues Encountered & Resolutions

- **Histogram drain-on-read in in-src tests**: First and second `assert_observation_count_at_least` calls on the same metric failed because `take_entries()` drains all entries on first read. Resolved by consolidating to a single histogram check per snapshot and documenting the trap in the §"Unobserved semantics" doc-comment of `crates/common/src/observability/testing.rs`.
- **`valid_date_range` CHECK constraint violation in `expire_old_keys` fixture**: setting only `valid_until = NOW() - 1h` violated `valid_until > valid_from`. Fixed by UPDATE-shifting BOTH `valid_from` AND `valid_until` backwards in the test seed.
- **`register_user` chained dual-emission**: with `auth_events` broken, `register_user` emits BOTH `user_registered` (its own audit emission) AND `user_login` (chained via auto-login through `issue_user_token`). Resolved by asserting both event_types fire and applying `assert_delta(0)` adjacency on the remaining 9 siblings.
- **`user_login`/`user_login_failed` mis-classification at @security S-1**: I declared these forward-looking, then re-traced production and confirmed they ARE emitted via the parameterized `token_service.rs:362` site. Honest correction landed in iter 2; no behavior change, just an annotation fix.
- **INDEX.md size violations on first attempt**: collapsed the new §"Metric tests (ADR-0032 Step 4)" section + folded existing crypto/jwt entries into compact one-line groups to bring `auth-controller/INDEX.md` back under the 75-line cap.
- **Stale `jwt_fixtures.rs` pointer in `code-reviewer/INDEX.md`**: pointed to the iter-1 file shape (speculative `sign_service_token_iat_offset` / `active_public_key_pem`); removed in reflection phase. The iter-3 re-introduced file has a different shape (`sign_service_token` / `sign_user_token`) and is referenced from `auth-controller/INDEX.md:62`; not re-added to `code-reviewer/INDEX.md` because the new helpers are AC-test-scoped, not a code-reviewer specialist concern.
- **Scope-reduction discipline failure (team-lead feedback)**: shipping reduced fidelity without explicit re-scoping discussion was flagged as a discipline gap. Treated as load-bearing; iter-2 closed all 3 team-lead-flagged gaps + 4 @test findings + 5 @observability findings + 2 @dry-reviewer findings via either backfill or concrete TODO.
- **`jwt_fixtures.rs` lifecycle (iter-1 ship → iter-2 delete → iter-3 re-add with parameterization)**: shipped iter-1 with `sign_service_token_iat_offset` and `active_public_key_pem` — both unused at any call site. iter-2 deleted the helpers as speculative scaffolding. iter-3 (per @dry-reviewer Finding 1 (a) re-send) recognized the underlying 4-line signing-decrypt-sign block was still inlined at 6 call sites and re-added two parameterized helpers (`sign_service_token`, `sign_user_token`) taking caller-supplied claims. The 6 call sites now consume the helpers (errors_metric ×2, token_validation ×3, key_rotation ×1). Net file delta: ~36 lines deleted across the 3 test files, ~25 lines of helper code added. Lesson recorded in §Tech Debt and §Lessons Learned.
- **Bookkeeping miss in §Files Modified, then iter-3 reversal** (caught reflection-phase by @operations, then again pre-commit by @team-lead): after iter-2 deletion, jwt_fixtures.rs was still listed as a "new test fixture" — corrected mid-reflection to flag it as removed. iter-3 then re-added the file (Finding 1 (a)), so the §Files Modified entry now lists it as a shipped fixture again, with the iter-1→iter-2→iter-3 lifecycle captured in §Tech Debt as the DRY learning. Real lesson: iterate until the right abstraction emerges from real callers — iter-1 helpers were too speculative (5-axis variation), iter-2 deletion was correct for the dead code but didn't address the underlying inline duplication, iter-3 re-introduction with a 1-axis (claims) parameterization fits the actual shape of the 6 call sites.

---

## Lessons Learned

- **Per-failure-class fidelity beats aggregate counter checks**: ADR-0032 §Pattern #3 partial-label `assert_delta(0)` adjacency on every multi-label test catches label-swap bugs that pure presence assertions miss. The 12-cell adjacency-matrix factor pattern (`assert_only_cell` helper invoked uniformly across all (operation × status) cells) is a reusable shape for any small-cardinality counter family.
- **Histogram-first ordering in mixed-kind snapshots**: histograms drain on read; ordering checks histogram-first prevents subtle "second assertion sees zero entries" failures.
- **Surgical fault-injection > heavy hammer when production has pre-query side effects**: `CHECK ... NOT VALID` blocks INSERTs without invalidating SELECTs, which is required when the production site does a pre-INSERT lookup before the audit write.
- **Component tests must drive real handlers, not server harnesses, when the assertion recorder is per-thread**: the `flavor = "current_thread"` + `#[sqlx::test]` + direct-handler-call combination is the only configuration where the `MetricAssertion` recorder reliably captures emissions from production code paths.
- **Plan-stage commitment fidelity is the load-bearing discipline**: "X of Y completed" footnotes are NOT acceptable as deferrals — they are blockers for Ready-for-validation. Either land the full Y or explicitly re-scope before declaring readiness. This rule survived two reviewer iterations and will carry into future ADR-0032 phases.
- **INDEX.md has a 75-line cap and a strict pointer-validation regex**: `:Type::nested` doesn't strip cleanly (the regex's second-character class is `[A-Za-z_]`, not `:`). Use top-level `:Type` or `:function()` references; consolidate verbose code-location entries into one-line groups when the cap binds.
