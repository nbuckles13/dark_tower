# Devloop Output: MH Observability Metrics (User Story Task 10)

**Date**: 2026-04-17
**Task**: Add MH observability metrics per R-26/R-27 — register_meeting counters, timeout counter; verify wiring across auth/webtransport/session/mc_client
**Specialist**: media-handler
**Mode**: Agent Teams (v2) — full
**Branch**: `feature/mh-quic-mh-metrics`

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `0f09c1676f4813a78b363a00daf5a1faab3372ab` |
| Branch | `feature/mh-quic-mh-metrics` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@mh-metrics` |
| Implementing Specialist | `media-handler` |
| Iteration | `1` |
| Security | `confirmed` |
| Test | `confirmed` |
| Observability | `confirmed` |
| Code Quality | `confirmed` |
| DRY | `confirmed` |
| Operations | `confirmed` |

### Review Verdicts (Gate 3)

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | CLEAR | 0 | 0 | 0 | No security surface touched |
| Test | CLEAR | 0 | 0 | 0 | Unit test for new wrapper; rustdoc exceeds expectations |
| Observability | CLEAR | 0 | 0 | 0 | ADR-0011/0029 compliant; catalog drive-by cleanup correct |
| Code Quality | CLEAR | 0 | 0 | 0 | ADR-0002/0011/0029 compliant; drive-by catalog cleanup noted |
| DRY | CLEAR | 0 | 0 | 0 | No duplication; no extraction opportunities |
| Operations | CLEAR | 0 | 0 | 0 | Additive, rollback-safe; noted pre-existing audit findings as separate tech debt |

### Plan Confirmations (Gate 1)

| Reviewer | Plan Status | Notes |
|----------|-------------|-------|
| Security | confirmed | bounded labels, no PII, timeout records before error-return |
| Test | confirmed | unit tests mirror existing pattern; timeout site correctly at connection.rs not session/mod.rs |
| Observability | confirmed | ADR-0011 naming, keep richer JWT labels, catalog doc must update |
| Code Quality | confirmed | keep both `mh_grpc_requests_total` + `mh_register_meeting_total` (transport vs business); 7 error sites (not 5) |
| DRY | confirmed | per-service metric prefix is explicit false-positive boundary |
| Operations | confirmed | additive, rollback-safe, no config drift |

---

## Task Overview

### Objective
Complete MH observability metric coverage (R-26, R-27) — add the two missing counters (`mh_register_meeting_total`, `mh_register_meeting_timeouts_total`) and verify all instrumentation points in the auth, webtransport, session, and mc_client modules.

### Scope
- **Service(s)**: mh-service
- **Schema**: No
- **Cross-cutting**: No (Observability review is already mandatory)

### Debate Decision
NOT NEEDED — the design is specified by the user story and R-26/R-27.

### Known Starting State
- `crates/mh-service/src/observability/metrics.rs` already defines: `record_webtransport_connection`, `record_webtransport_handshake_duration`, `set_active_connections`, `record_jwt_validation`, `record_mc_notification`.
- Existing wiring found:
  - `grpc/auth_interceptor.rs:189/200/214` — service-token JWT validations
  - `webtransport/connection.rs:112/122` — meeting-token JWT validations
  - `webtransport/connection.rs:138` — handshake duration
  - `webtransport/server.rs:174/179/202/205/217` — connection outcomes + active connections gauge
  - `grpc/mc_client.rs:182/195/208` — MC notification outcomes
- Missing: `mh_register_meeting_total` and `mh_register_meeting_timeouts_total` — no functions, no wiring.
- Likely wiring locations for the new metrics: `grpc/mh_service.rs` (RegisterMeeting handler) and `session/mod.rs` (pending-connection timeout path).

---

## Implementation Summary

### Code
- New metric wrapper `record_register_meeting_timeout()` → `mh_register_meeting_timeouts_total` (counter, no labels, cardinality 1). Records before `remove_pending_connection` so the signal fires even if cleanup panics.
- Wired at a single site: `crates/mh-service/src/webtransport/connection.rs:219` (provisional-accept timeout arm). Explicitly NOT wired on the shutdown-cancellation arm.
- No changes to `grpc/mh_service.rs` — the RegisterMeeting receipt signal is served by the existing `mh_grpc_requests_total{method="register_meeting"}` counter (plan v2 resolution avoiding a duplicate business-level counter).

### Docs
- `docs/observability/metrics/mh-service.md` — new "RegisterMeeting Metrics" section; cross-reference under `mh_grpc_requests_total` pointing R-26 readers to `{method="register_meeting"}`; updated `mh_grpc_requests_total` method-count description (3 → 4) and cardinality (6 → 8) for accuracy.
- Existing 3-label `mh_jwt_validations_total` form (`result`/`token_type`/`failure_reason`) preserved — richer than R-27's `status`/`token_type` text; over-delivery noted and accepted by observability reviewer.

### Dashboard
- `infra/grafana/dashboards/mh-overview.json` — minimal stat panel (id 25, title "RegisterMeeting Timeouts (R-26)", `increase(...[$__rate_interval])` per ADR-0029 Category A, green→red threshold at 1). Task 12 will expand with alerting.

### Knowledge
- `docs/specialist-knowledge/media-handler/INDEX.md` — consolidated metric-recorder pointer + dedicated timeout fire-site pointer.
- `docs/specialist-knowledge/observability/INDEX.md` — timeout counter recording-site pointer.
- `docs/specialist-knowledge/code-reviewer/INDEX.md` — `record_register_meeting_timeout` pointer preserving non-obvious "fires only on timeout arm, not cancel arm" detail.

## Files Modified

```
crates/mh-service/src/observability/metrics.rs   | 26 +++++++++
crates/mh-service/src/webtransport/connection.rs |  1 +
docs/observability/metrics/mh-service.md         | 33 ++++++++++--
docs/specialist-knowledge/code-reviewer/INDEX.md |  2 +-
docs/specialist-knowledge/media-handler/INDEX.md |  4 +-
docs/specialist-knowledge/observability/INDEX.md |  2 +-
infra/grafana/dashboards/mh-overview.json        | 67 ++++++++++++++++++++++++
```

## Validation Pipeline

| Layer | Status | Notes |
|-------|--------|-------|
| 1. cargo check --workspace | PASS | |
| 2. cargo fmt --all | PASS | No changes needed |
| 3. guards (15) | PASS | Required one iteration — INDEX 76→75 lines (lead trim), then application-metrics guard needed the dashboard panel |
| 4. workspace tests | PASS | 106 mh-service unit + 7 GC integration + 6 MC integration; workspace otherwise green |
| 5. clippy -D warnings | PASS | |
| 6. cargo audit | INFO | 5 pre-existing transitive vulnerabilities (quinn/wtransport, sqlx/mysql) — not introduced by this PR, no CI gate; flagged as separate tech debt by operations |
| 7. semantic-guard | SAFE | No credential leak, actor blocking, error swallowing, cardinality explosion, or dashboard schema issues |
| 8. env-tests (Kind) | PASS | Passed on retry — initial failure was transient Loki warm-up post-rebuild; infra flakiness, not consumed attempt |

## Iteration History
- Gate 1 (planning): 2 iterations — v1 proposed both `mh_register_meeting_total` and `mh_register_meeting_timeouts_total`; code-reviewer flagged potential duplication with `mh_grpc_requests_total{method="register_meeting"}`. v2 dropped the business-level counter; all 6 reviewers confirmed v2.
- Implementation: had a brief drift back to v1-with-both-metrics before re-aligning with v2.
- Gate 2 (validation): 3 intermediate states before all guards green (INDEX line count, dashboard coverage).
- Gate 3 (review): single pass. All reviewers CLEAR on first examination.
- Reflection: 3 teammates updated INDEX pointers; 4 declined (no new navigation value).

## Tech Debt

### Deferred Findings
No findings deferred.

### Cross-Service Duplication (from DRY Reviewer)
No cross-service duplication detected.

### Audit Findings (from Operations — out-of-scope for this PR)
Operations flagged 5 pre-existing transitive cargo-audit vulnerabilities and suggested a separate operations task to (a) add cargo-audit to the guard pipeline and (b) triage for upgrade paths. Not this PR's responsibility; Lead to queue as follow-up.

## Rollback Procedure
Start commit: `0f09c1676f4813a78b363a00daf5a1faab3372ab`.
1. Review: `git diff 0f09c16..HEAD`
2. Soft reset: `git reset --soft 0f09c16`
3. Hard reset: `git reset --hard 0f09c16` (no migrations, no infra state)
