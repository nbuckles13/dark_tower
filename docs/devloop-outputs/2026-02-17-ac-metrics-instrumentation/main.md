# Devloop Output: Wire AC Dead-Code Metrics Into Production

**Date**: 2026-02-17
**Task**: Wire 9 dead-code AC metrics recording functions into production call sites
**Specialist**: auth-controller
**Mode**: Agent Teams (v2) — Full (escalated from --light due to instrumentation code)
**Branch**: `feature/gc-registered-mc-metrics`
**Duration**: ~25m (1 iteration + review fixes)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `3ef8e8418016a097878d85668508f92cb6d14c8b` |
| Branch | `feature/gc-registered-mc-metrics` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@ac-metrics-instrumentation` |
| Implementing Specialist | `auth-controller` |
| Iteration | `1` |
| Security | `CLEAR` |
| Test | `PASS` |
| Observability | `APPROVED` |
| Code Quality | `PASS` |
| DRY | `CLEAR` |
| Operations | `APPROVED` |

---

## Validation Results (Iteration 1)

| Layer | Command | Result |
|-------|---------|--------|
| 1. Compile | `cargo check --workspace` | PASS |
| 2. Format | `cargo fmt --all -- --check` | PASS (fixed on retry) |
| 3. Guards | `./scripts/guards/run-guards.sh` | PASS (12/12) |
| 4. Tests | `./scripts/test.sh --workspace` | PASS |
| 5. Clippy | `cargo clippy --workspace --lib --bins -- -D warnings` | PASS |
| 6. Audit | `cargo audit` | PASS (2 pre-existing: ring, rustls-pemfile) |
| 7. Semantic | `semantic-guard` agent | PASS (2 non-blocking notes on pre-existing code) |

---

## Task Overview

### Objective
Wire 9 dead-code AC metrics recording functions into their production call sites. Remove #[allow(dead_code)] annotations once wired.

### Metrics to Wire
1. Key Management: ac_active_signing_keys, ac_signing_key_age_days, ac_key_rotation_last_success_timestamp
2. Database: ac_db_queries_total, ac_db_query_duration_seconds
3. Security & Crypto: ac_bcrypt_duration_seconds, ac_rate_limit_decisions_total, ac_audit_log_failures_total, ac_admin_operations_total

### Scope
- **Service(s)**: ac-service
- **Schema**: No
- **Cross-cutting**: No

### Debate Decision
NOT NEEDED - Single-service instrumentation wiring

---

## Implementation Summary

### Iteration 1: Wire 9 Dead-Code AC Metrics

| Category | Metrics | Call Sites |
|----------|---------|------------|
| Key Management | `ac_active_signing_keys`, `ac_signing_key_age_days`, `ac_key_rotation_last_success_timestamp` | `key_management_service.rs` (init, rotate), `admin_handler.rs` (rotate) |
| Database | `ac_db_queries_total`, `ac_db_query_duration_seconds` | `auth_events.rs` (2), `service_credentials.rs` (1), `signing_keys.rs` (3) |
| Security & Crypto | `ac_bcrypt_duration_seconds` | `crypto/mod.rs` (hash, verify) |
| Security & Crypto | `ac_rate_limit_decisions_total` | `token_service.rs` (2), `user_service.rs` (1) |
| Security & Crypto | `ac_audit_log_failures_total` | `key_management_service.rs` (3), `registration_service.rs` (3), `token_service.rs` (3), `user_service.rs` (1) |
| Security & Crypto | `ac_admin_operations_total` | `admin_handler.rs` (7 CRUD handlers, success + error paths) |

### Additional Changes
- `init_key_metrics()` function: Seeds gauge values from DB at startup (handles restart-to-zero gap)
- Removed `#[allow(dead_code)]` from 8 functions (1 retained: `record_token_validation` — Phase 4)
- Updated AC dashboard panel descriptions: removed "Pending instrumentation" from 9 wired metrics
- Updated metrics catalog (`ac-service.md`): documented call sites, removed stale status notes

---

## Files Modified

```
 crates/ac-service/src/crypto/mod.rs                    |  17 ++-
 crates/ac-service/src/handlers/admin_handler.rs        |  32 +++++-
 crates/ac-service/src/main.rs                          |   6 +
 crates/ac-service/src/observability/metrics.rs         |  14 +--
 crates/ac-service/src/repositories/auth_events.rs      |  22 +++-
 crates/ac-service/src/repositories/service_credentials.rs |  14 ++-
 crates/ac-service/src/repositories/signing_keys.rs     |  22 +++-
 crates/ac-service/src/services/key_management_service.rs |  34 ++++++
 crates/ac-service/src/services/registration_service.rs |   4 +
 crates/ac-service/src/services/token_service.rs        |   8 ++
 crates/ac-service/src/services/user_service.rs         |   4 +
 docs/observability/metrics/ac-service.md               |  18 ++-
 infra/grafana/dashboards/ac-overview.json              |  (panel descriptions)
```

---

## Code Review Results

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | CLEAR | 0 | 0 | 0 | No side-channels, no PII, no key material exposure |
| Test | PASS | 0 | 0 | 2 | TD: `users.rs`/`organizations.rs` db instrumentation, `init_key_metrics` test |
| Observability | APPROVED | 2 | 1 | 1 | Fixed: dashboard descriptions. Deferred: rotate_keys admin_operation (has own metric) |
| Code Quality | PASS | 2 | 3 | 0 | Fixed: missing admin_op on early return, get_all_active_keys db instrumentation, duplicate gauge removal |
| DRY | CLEAR | 0 | 0 | 2 | TD-21: record_db_query signature inconsistency. TD-19: init_metrics_recorder boilerplate |
| Operations | APPROVED | 1 | 1 | 0 | Fixed: stale "Pending instrumentation" panel descriptions |

---

## Tech Debt

### Deferred Findings
| ID | Description | Source |
|----|-------------|--------|
| TD-new | `record_db_query` instrumentation for `users.rs` (15 queries) and `organizations.rs` (5 queries) | Test reviewer |
| TD-new | Dedicated `#[sqlx::test]` for `init_key_metrics` | Test reviewer |
| TD-new | `handle_rotate_keys` doesn't record `record_admin_operation()` (has dedicated `ac_key_rotation_total`) | Observability reviewer |
| TD-21 | AC `record_db_query()` has 4 params (with `table`) vs GC's 3 params (without) — inconsistency for future common extraction | DRY reviewer |
| TD-19 | `init_metrics_recorder()` boilerplate across all 3 services | DRY reviewer |

---

## Human Review (Iteration 2)

**Feedback**: "Fix two deferred items: (1) Add record_db_query instrumentation to all query call sites in users.rs and organizations.rs. (2) Rename ac_admin_operations_total to ac_credential_operations_total — this metric tracks CRUD operations on service credentials, not all admin operations. Update metric name in metrics.rs, all call sites in admin_handler.rs, the AC dashboard (ac-overview.json), and the metrics catalog (ac-service.md). Do NOT add the metric to handle_rotate_keys — key rotation has its own dedicated metric."

---

## Iteration 2: DB Instrumentation Gaps + Metric Rename

**Mode**: Light (3 teammates: implementer + security + observability)

### Changes
1. **DB query instrumentation**: Added `record_db_query` to 10 query call sites:
   - `users.rs`: 8 functions (get_by_email, get_by_id, create_user, update_last_login, get_user_roles, add_user_role, remove_user_role, email_exists_in_org)
   - `organizations.rs`: 2 functions (get_by_subdomain, get_by_id)
2. **Metric rename**: `ac_admin_operations_total` → `ac_credential_operations_total`, `record_admin_operation` → `record_credential_operation` across metrics.rs, admin_handler.rs (19 call sites), ac-overview.json, ac-service.md, ADR-0003, knowledge files

### Validation Results (Iteration 2)

| Layer | Result |
|-------|--------|
| 1. Compile | PASS |
| 2. Format | PASS |
| 3. Guards | PASS (12/12) |
| 4. Tests | PASS |
| 5. Clippy | PASS |
| 6. Audit | PASS (2 pre-existing) |
| 7. Semantic | PASS (1 pre-existing MC bug flagged, AC changes clean) |

### Code Review Results (Iteration 2)

| Reviewer | Verdict | Findings | Fixed | Deferred |
|----------|---------|----------|-------|----------|
| Security | CLEAR | 0 | 0 | 0 |
| Observability | RESOLVED | 3 | 3 | 0 |

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit: `3ef8e8418016a097878d85668508f92cb6d14c8b`
2. Review changes: `git diff 3ef8e84..HEAD`
3. Soft reset: `git reset --soft 3ef8e84`
4. Hard reset: `git reset --hard 3ef8e84`
