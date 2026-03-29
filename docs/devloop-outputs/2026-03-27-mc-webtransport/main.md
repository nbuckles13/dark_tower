# Devloop Output: MC WebTransport server + join flow handler

**Date**: 2026-03-27
**Task**: Implement MC WebTransport server + join flow connection handler (wtransport TLS, accept loop, JoinRequest/Response, ParticipantJoined/Left bridge, CancellationToken wiring)
**Specialist**: meeting-controller
**Mode**: full
**Branch**: `feature/meeting-join-user-story-devloop-task7`
**Duration**: ~4 hours (2 iterations including architecture refactor)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `3a064a423ad4036ed24edf2c932690b886933310` |
| Branch | `feature/meeting-join-user-story-devloop-task7` |
| End Commit | `dcee11c` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementing Specialist | `meeting-controller` |
| Iteration | `2` |

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

Implementer proposed a 9-file plan (3 new, 6 modified):
1. Config: add `tls_cert_path`, `tls_key_path` (required, fail-fast)
2. Controller: add `GetMeetingHandle` message for handler→actor bridge
3. WebTransport server (`server.rs`): wtransport endpoint, TLS, accept loop with CancellationToken
4. Connection handler (`handler.rs`): per-connection join flow — read JoinRequest, validate JWT, lookup meeting, join, send JoinResponse, bridge loop
5. ConnectionActor wiring: `mpsc::Sender<Bytes>` channel for testable stream output
6. main.rs: wire WebTransport server startup replacing Phase 6g TODO
7. Length-prefixed protobuf framing (4-byte BE + bytes)

All 6 reviewers confirmed with detailed inputs on security (JWT before actor, bounded reads), observability (tracing targets, lifecycle events), operations (bind-before-spawn), and test (trait-based stream abstraction).

---

## Implementation Summary

### Iteration 1
Initial implementation: WebTransport server, connection handler, config changes, controller handle lookup, ConnectionActor stream wiring, main.rs integration. 12 files changed, 194 unit tests + 17 integration tests pass.

### Iteration 2 — Architecture Refactor
Human review during devloop identified architectural issues:
1. **ConnectionActor renamed to ParticipantActor** — the actor represents a participant in a meeting, not the connection
2. **Handler promoted to proper ConnectionActor** — the bridge loop is the real connection actor (owns WebTransport socket, has typed handle via `mpsc::Sender<Bytes>`, runs select loop)
3. **Fire-and-forget join through controller** — `ControllerMessage::JoinConnection` replaces `GetMeetingHandle`; handler never holds `MeetingActorHandle`
4. **ParticipantActor owns disconnect** — notifies meeting on exit via `MeetingActorHandle` passed at spawn
5. **outbound_tx threaded through entire join flow** — `stream_tx` flows from handler → controller → MeetingActor → `ParticipantActor::spawn_inner()`, so R-8 notifications reach the wire end-to-end
6. **Dead code cleanup** — removed `GetMeetingHandle`, `SetStreamTx`; stripped `handler.rs` to encoding utility

### Files Changed (final)
- New: `actors/participant.rs`, `webtransport/connection.rs`, `webtransport/handler.rs`, `webtransport/mod.rs`, `webtransport/server.rs`
- Deleted: `actors/connection.rs`
- Modified: `actors/controller.rs`, `actors/meeting.rs`, `actors/messages.rs`, `actors/metrics.rs`, `actors/mod.rs`, `config.rs`, `grpc/gc_client.rs`, `lib.rs`, `main.rs`, `tests/gc_integration.rs`

26 files changed, +2219/-652 lines.

---

## Review Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | CLEAR | 3 (iter 1) | 2 | 1 obs | Capacity bound, name length, outbound wiring (resolved in iter 2) |
| Test | RESOLVED | 4 | 4 | 0 | encode_participant_update, build_join_response, spawn_with_stream tests |
| Observability | CLEAR | 3 | 3 | 0 | Tracing targets, participant_type field, deferred metrics |
| Code Quality | CLEAR | 7 | 7 | 0 | Bridge loop wiring, u32 cast, outbound_tx threading |
| DRY | CLEAR | 0 | 0 | 0 | No duplication introduced |
| Operations | CLEAR | 1 | 1 | 0 | Bind-before-spawn for fail-fast |

---

## Task 19 Added

New user story task: Move JWT auth from JoinRequest protobuf to HTTP/3 CONNECT headers (enables off-box auth termination). Amend ADR-0023.
