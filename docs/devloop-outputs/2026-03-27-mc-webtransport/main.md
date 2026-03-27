# Devloop Output: MC WebTransport server + join flow handler

**Date**: 2026-03-27
**Task**: Implement MC WebTransport server + join flow connection handler (wtransport TLS, accept loop, JoinRequest/Response, ParticipantJoined/Left bridge, CancellationToken wiring)
**Specialist**: meeting-controller
**Mode**: full
**Branch**: `feature/meeting-join-user-story-devloop-task7`
**Duration**: ~TBD

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `3a064a423ad4036ed24edf2c932690b886933310` |
| Branch | `feature/meeting-join-user-story-devloop-task7` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `reflection` |
| Implementer | `pending` |
| Implementing Specialist | `meeting-controller` |
| Iteration | `2` |
| Security | `pending` |
| Test | `pending` |
| Observability | `pending` |
| Code Quality | `pending` |
| DRY | `pending` |
| Operations | `pending` |

---

## Task Overview

### Objective
Implement the MC WebTransport server and join flow connection handler. This is the core client-facing entry point: clients connect via WebTransport (HTTP/3 over QUIC), send JoinRequest with meeting JWT, get back JoinResponse with participant roster, and receive ParticipantJoined/Left notifications.

### Scope
- **Service(s)**: mc-service (new WebTransport module, connection handler wiring)
- **Schema**: No
- **Cross-cutting**: No

### Requirements Covered
- R-5: MC accepts WebTransport connections from clients (HTTP/3 over QUIC with TLS 1.3) on port 4433
- R-7: MC processes JoinRequest, creates participant session with session binding tokens, returns JoinResponse with participant roster and media server info
- R-8: MC bridges ConnectionActor to MeetingActor for bidirectional signaling (ParticipantJoined/ParticipantLeft notifications only)

### Security Decisions

| Decision | Choice | Rationale | ADR Reference |
|----------|--------|-----------|---------------|
| TLS termination | wtransport crate | TLS 1.3 required for QUIC/WebTransport | ADR-0023 |
| JWT validation | McJwtValidator (common) | EdDSA, JWKS-based | ADR-0020 |
| Session binding | HMAC-SHA256 + HKDF | Per ADR-0023 Section 1 | ADR-0023 |

### Design Details

**Layer architecture** (bottom-up):
1. Actor system (existing) — `MeetingActor::handle_join()`, session binding tokens, participant broadcast
2. JWT validation (task 9, done) — Uses common `JwksClient` + `validate_token`, adds MC-specific validation (meeting_id match, token_type check)
3. Connection handler (new) — Per-connection task: read JoinRequest from bidirectional stream, validate JWT, look up meeting via `MeetingControllerActorHandle`, call `MeetingActorHandle::join()`, serialize JoinResponse, spawn ConnectionActor wired to WebTransport stream
4. WebTransport server (new) — `wtransport` crate, TLS 1.3 termination, accept loop spawning connection tasks, CancellationToken wiring

**File changes**:
- New: `src/webtransport/mod.rs`, `src/webtransport/server.rs`, `src/webtransport/handler.rs`
- Modified: `src/config.rs` — add `tls_cert_path`, `tls_key_path`
- Modified: `src/main.rs` — replace `TODO (Phase 6g)` with WebTransport server startup
- Modified: `src/actors/connection.rs` — wire WebTransport stream to `handle_send()`/`handle_update()`/`graceful_close()` (replacing Phase 6g TODOs)
- Modified: `Cargo.toml` — add `wtransport` dep

**Signaling scope**: Only `ParticipantJoined` and `ParticipantLeft` are sent over the wire. Other `ParticipantStateUpdate` variants (MuteChanged, Disconnected, Reconnected) are logged but not serialized.

**JWT validation chain**: size check → extract kid → JWKS lookup → EdDSA signature verify → exp check → iat check (with clock skew) → meeting_id match → token_type check ("meeting" or "guest")

---

## Plan Confirmation

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |

---

## Planning

TBD

---

## Implementation Summary

TBD

---

## Review Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | | | | | |
| Test | | | | | |
| Observability | | | | | |
| Code Quality | | | | | |
| DRY | | | | | |
| Operations | | | | | |
