# Devloop Output: MH JWKS JWT Validation + WebTransport Server + Connection Handler + Auth Interceptor Upgrade

**Date**: 2026-04-13
**Task**: Implement JWKS-based JWT validation, WebTransport server, connection handler with provisional accept, and upgrade MhAuthInterceptor to full JWKS-based validation
**Specialist**: media-handler
**Mode**: Agent Teams (full)
**Branch**: `feature/mh-quic-webtransport`
**Duration**: ~35m

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `799ddac4f2d6452f2cdb1b62bacc374f6e8737fa` |
| Branch | `feature/mh-quic-webtransport` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@mh-quic-wt` |
| Implementing Specialist | `media-handler` |
| Iteration | `3` |
| Security | `security@mh-quic-wt` |
| Test | `test@mh-quic-wt` |
| Observability | `observability@mh-quic-wt` |
| Code Quality | `code-reviewer@mh-quic-wt` |
| DRY | `dry-reviewer@mh-quic-wt` |
| Operations | `operations@mh-quic-wt` |

---

## Task Overview

### Objective
Implement MH-side components for client-to-MH QUIC/WebTransport connections: JWKS-based JWT validation for meeting tokens, WebTransport server with TLS 1.3, connection handler with provisional accept and RegisterMeeting timeout, and upgrade MhAuthInterceptor from structural-only to full JWKS-based ServiceClaims validation.

### Scope
- **Service(s)**: mh-service (primary), common crate (reuse only)
- **Schema**: No
- **Cross-cutting**: No (MH-only, reuses existing common crate patterns)

### Debate Decision
NOT NEEDED - Follows established MC WebTransport pattern and common crate JWT infrastructure. No new architectural decisions required.

### User Story Reference
`docs/user-stories/2026-04-12-mh-quic-connection.md` — Task #3

### Requirements Covered
- R-7: MH validates meeting JWTs via AC JWKS endpoint using shared JwtValidator
- R-8: MH implements WebTransport server on port 4434 with TLS 1.3
- R-9: Client connects to all assigned MHs in parallel (active/active)
- R-14: RegisterMeeting timeout enforcement (provisional accept + 15s timeout)
- R-21: Upgrade MhAuthInterceptor to full JWKS-based ServiceClaims validation
- R-26: WebTransport metrics (connections, handshake duration, active connections gauge)
- R-27: JWT validation metrics (success/failure by token type)

---

## Planning

Implementer drafted approach covering 12 file changes. All 6 reviewers confirmed after Q&A:
- Security: 5 questions on auth binding, guest rejection, error sanitization — all resolved
- Code Quality: 3 questions on SessionManager pattern, scope checking, error naming — all resolved
- Test: 6 questions on test coverage, JWKS unreachable, framing tests — all resolved
- Observability: confirmed metrics naming and histogram buckets
- DRY: confirmed thin wrapper pattern, noted framing extraction opportunity
- Operations: confirmed startup order, config handling, rollback safety

---

## Pre-Work

None

---

## Implementation Summary

### New Modules
- `crates/mh-service/src/auth/mod.rs` — `MhJwtValidator` wrapping common `JwtValidator<MeetingTokenClaims>` with `token_type == "meeting"` enforcement; 8 unit tests
- `crates/mh-service/src/session/mod.rs` — `SessionManager` with `tokio::sync::RwLock`, `Notify`-based pending connection promotion, meeting cleanup; 9 unit tests
- `crates/mh-service/src/webtransport/mod.rs` — module declarations
- `crates/mh-service/src/webtransport/server.rs` — `WebTransportServer` with TLS 1.3, capacity-bounded accept loop, CancellationToken shutdown
- `crates/mh-service/src/webtransport/connection.rs` — connection handler: session accept, bidi stream, length-prefixed JWT read (64KB max), validation, provisional accept with configurable timeout

### Modified Files
- `errors.rs` — Added `JwtValidation`, `WebTransportError`, `MeetingNotRegistered` variants with `From<JwtError>`, bounded labels, generic client messages
- `config.rs` — Added `ac_jwks_url` (required, scheme-validated), `register_meeting_timeout_seconds` (default 15, clamped to 300s max), `max_connections` (default 10000)
- `grpc/auth_interceptor.rs` — Added `MhAuthLayer`/`MhAuthService` (async tower Layer with JWKS-based `ServiceClaims` validation + `service.write.mh` scope check); legacy `MhAuthInterceptor` kept; 7 async integration tests
- `main.rs` — Wired `JwksClient`, `MhJwtValidator`, `SessionManager`, `WebTransportServer`, replaced interceptor with `MhAuthLayer`; WebTransport starts before GC registration
- `observability/metrics.rs` — Added 4 metrics: `mh_webtransport_connections_total`, `mh_webtransport_handshake_duration_seconds`, `mh_active_connections`, `mh_jwt_validations_total`
- `Cargo.toml` — Added `tower`, `chrono`, dev-deps for tests
- `infra/grafana/dashboards/mh-overview.json` — Added "Client Connections" row with 6 panels
- `docs/observability/metrics/mh-service.md` — Added catalog entries for 4 new metrics

---

## Files Modified

```
 crates/mh-service/Cargo.toml                     |  15 +-
 crates/mh-service/src/auth/mod.rs                | NEW
 crates/mh-service/src/session/mod.rs             | NEW
 crates/mh-service/src/webtransport/mod.rs        | NEW
 crates/mh-service/src/webtransport/server.rs     | NEW
 crates/mh-service/src/webtransport/connection.rs | NEW
 crates/mh-service/src/config.rs                  | 135 +++
 crates/mh-service/src/errors.rs                  | 116 ++-
 crates/mh-service/src/grpc/auth_interceptor.rs   | 515 ++++++++++++-
 crates/mh-service/src/grpc/mod.rs                |   2 +-
 crates/mh-service/src/lib.rs                     |   3 +
 crates/mh-service/src/main.rs                    |  67 +-
 crates/mh-service/src/observability/metrics.rs   |  78 +-
 crates/mh-service/tests/gc_integration.rs        |   3 +
 docs/TODO.md                                     |   3 +-
 docs/observability/metrics/mh-service.md         |  54 ++
 infra/grafana/dashboards/mh-overview.json        | 511 ++++++++++++
```

---

## Devloop Verification Steps

### Layer 1: cargo check
**Status**: PASS
**Duration**: ~1s

### Layer 2: cargo fmt
**Status**: PASS
**Duration**: ~1s

### Layer 3: Simple Guards
**Status**: ALL PASS (15/15)
**Duration**: ~6s

### Layer 4: Tests
**Status**: PASS
**Duration**: ~30s
**Output**: 1170 tests passed, 0 failed

### Layer 5: Clippy
**Status**: PASS
**Duration**: ~5s
**Output**: 0 warnings

### Layer 6: Cargo Audit
**Status**: PASS (3 pre-existing vulnerabilities, not introduced by this change)

### Layer 7: Semantic Guard
**Status**: SAFE
**Output**: No credential leaks, no async blocking, no error context issues

### Layer 8: Env-tests
**Status**: PASS (pre-existing MC WebTransport connectivity failures, verified on clean codebase)

---

## Code Review Results

### Security Specialist
**Verdict**: RESOLVED
**Findings**: 1 found, 1 fixed, 0 deferred

- `register_meeting_timeout_seconds` accepted any u64 value — added MAX_REGISTER_MEETING_TIMEOUT_SECONDS (300s) clamping

### Test Specialist
**Verdict**: RESOLVED
**Findings**: 3 found, 2 fixed, 1 deferred

- Missing MhAuthService async tests — added 7 integration tests
- Missing JWKS unreachable test — added test_validate_meeting_token_jwks_unreachable
- `read_framed_message` unit tests — deferred (concrete wtransport type, covered by Task #14 E2E)

### Observability Specialist
**Verdict**: RESOLVED
**Findings**: 1 found, 1 fixed, 0 deferred

- Label name mismatch (`status` vs `result`) for mh_jwt_validations_total — renamed to `result`

### Code Quality Reviewer
**Verdict**: RESOLVED
**Findings**: 4 found, 3 fixed, 1 deferred

- `#[allow]` → `#[expect]` with reason in 6 production locations — fixed
- Missing MhAuthService scope tests — added 2 integration tests
- participant_id/meeting_id at INFO level — deferred (matches MC pattern, cross-service fix needed)

### DRY Reviewer
**Verdict**: CLEAR
**Findings**: 0

Extraction opportunities noted (tech debt, not findings):
- `read_framed_message` (~40 lines identical in MC + MH)
- TestKeypair duplication (7th copy)
- WebTransport server bind pattern

### Operations Reviewer
**Verdict**: RESOLVED
**Findings**: 2 found, 2 fixed, 0 deferred

- AC_JWKS_URL missing scheme validation — added http/https check
- New config fields not logged at startup — added to startup log event

---

## Tech Debt

### Deferred Findings

| Finding | Reviewer | Location | Deferral Justification | Follow-up Task |
|---------|----------|----------|------------------------|----------------|
| `read_framed_message` unit tests | Test | `webtransport/connection.rs` | Concrete `wtransport::RecvStream` not constructable without QUIC connection; abstracting requires production code changes | Task #14 (E2E tests) |
| participant_id/meeting_id at INFO level | Code Quality | `webtransport/connection.rs` | Matches existing MC pattern; fixing MH alone creates inconsistency | Cross-service PII utilities |

### Cross-Service Duplication (from DRY Reviewer)

| Pattern | New Location | Existing Location | Follow-up Task |
|---------|--------------|-------------------|----------------|
| read_framed_message | `crates/mh-service/src/webtransport/connection.rs` | `crates/mc-service/src/webtransport/connection.rs` | Extract to common when 3rd service needs WebTransport |
| TestKeypair/build_pkcs8_from_seed | `crates/mh-service/src/auth/mod.rs` (tests) | `crates/mc-test-utils/src/jwt_test.rs` | Extract to common-test-utils |

### Temporary Code (from Code Reviewer)

No temporary code detected.

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `799ddac4f2d6452f2cdb1b62bacc374f6e8737fa`
2. Review all changes: `git diff 799ddac..HEAD`
3. Soft reset (preserves changes): `git reset --soft 799ddac`
4. Hard reset (clean revert): `git reset --hard 799ddac`

---

## Reflection

All 7 teammates updated their INDEX.md navigation files:
- media-handler: Added auth, session, webtransport code pointers and integration seams
- security: Added MH JWT validation, auth layer, WebTransport TLS pointers
- test: Added MhJwtValidator, SessionManager, MhAuthService test pointers
- observability: Added MH WebTransport metrics and tracing target pointers
- code-reviewer: Added MhJwtValidator, MhAuthLayer, SessionManager, WebTransport pointers
- dry-reviewer: Added WebTransport cross-service section, updated auth interceptor entries, updated TODO.md
- operations: Added WebTransport server, JWT validation, session management pointers

INDEX validation passed after reflection.

---

## Issues Encountered & Resolutions

### Issue 1: Metrics guard failure (Layer 3)
**Problem**: New metrics lacked dashboard panels and catalog documentation
**Resolution**: Added 6 Grafana panels and 4 catalog entries

### Issue 2: Multi-line counter! macro
**Problem**: Guard grep pattern couldn't parse multi-line `counter!("mh_jwt_validations_total",` call
**Resolution**: Moved metric name to same line as `counter!(`

### Issue 3: Pre-commit clippy errors
**Problem**: `uninlined_format_args` and `cast_possible_truncation` in test code
**Resolution**: Inlined format args and added `#[expect]` annotation

---

## Lessons Learned

1. Metrics guard requires metric name on same line as macro invocation — multi-line counter!/histogram!/gauge! calls are not detected
2. Dashboard and catalog updates must accompany metric code changes — guards enforce this bidirectionally
3. Auth interceptor upgrade from sync Interceptor to async tower::Layer is the correct pattern for JWKS validation in tonic

---

## Appendix: Verification Commands

```bash
cargo check --workspace
cargo fmt --all --check
./scripts/guards/run-guards.sh
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo audit
```
