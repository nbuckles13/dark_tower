# Devloop Output: MH-1 — MH media_dial-only admission and pending-path deletion

**Date**: 2026-11-10
**Task**: Story 4 task MH-1 (run-story #19): MH admits clients only on an MC-minted media_dial credential (ADR-0039 §3/§5/§7/§8); delete the pending-before-registration path; env-test fixture switch to media_dial_token.
**Specialist**: media-handler
**Mode**: Agent Teams (v2) — full, Gate-1 present
**Branch**: `feature/mh-failure-transfer`
**Duration**: ~6h (2026-11-10 → 2026-11-11)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `7001bec9865b2ffb27f3372c652349c2f56c7050` |
| Branch | `feature/mh-failure-transfer` |
| Lead Model | `claude-opus-5-5` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer (spawned)` |
| Implementing Specialist | `media-handler` |
| Tier | `full` |
| Iteration | `2` |
| Security | `spawned` |
| Test | `spawned` |
| Observability | `spawned` |
| Code Quality | `spawned` |
| DRY | `spawned` |
| Operations | `spawned` |
| Semantic Guard | `spawned` |
| Paired Protocol | `spawned` |
| Client (conditional reviewer) | `spawned` |

---

## Task Overview

### Objective
Full task text: run-story task-19 prompt (story `docs/user-stories/2026-10-31-survive-handler-loss.md`, task MH-1).

### Scope
- **Service(s)**: mh-service (+ env-tests fixtures, proto comments, infra config, dashboards/alerts/runbooks)
- **Schema**: No
- **Cross-cutting**: Yes (proto comment GSA edit with protocol; env-tests)

### Debate Decision
NOT NEEDED — governed by ADR-0039 and debate `docs/debates/2026-10-28-mh-replacement-and-migration/`.

---

## Cross-Boundary Classification

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `Cargo.lock` | Mine | — |
| `crates/env-tests/src/fixtures/mc_session.rs` | Domain-judgment | test |
| `crates/env-tests/src/fixtures/participant.rs` | Domain-judgment | test |
| `crates/env-tests/src/fixtures/injection.rs` | Domain-judgment | test |
| `crates/env-tests/src/fixtures/mh_grpc.rs` | Domain-judgment | test |
| `crates/env-tests/Cargo.toml` | Minor-judgment | test |
| `crates/env-tests/tests/01_mh_deployment_config.rs` | Minor-judgment | test |
| `crates/env-tests/tests/26_mh_quic.rs` | Minor-judgment | test |
| `crates/env-tests/tests/27_mc_slot_placement.rs` | Minor-judgment | test |
| `crates/env-tests/tests/28_mh_egress_admission.rs` | Minor-judgment | test |
| `crates/env-tests/tests/34_mc_kek_rotation.rs` | Minor-judgment | test |
| `crates/env-tests/tests/35_mc_server_mute_teardown.rs` | Minor-judgment | test |
| `crates/env-tests/tests/46_mh_media_dial_admission.rs` | Domain-judgment | test |
| `crates/mh-service/Cargo.toml` | Mine | — |
| `crates/mh-service/src/auth/mod.rs` | Mine | — |
| `crates/mh-service/src/config.rs` | Mine | — |
| `crates/mh-service/src/grpc/auth_interceptor.rs` | Mine | — |
| `crates/mh-service/src/grpc/mh_service.rs` | Mine | — |
| `crates/mh-service/src/main.rs` | Mine | — |
| `crates/mh-service/src/observability/metrics.rs` | Mine | — |
| `crates/mh-service/src/observability/mod.rs` | Mine | — |
| `crates/mh-service/src/session/mod.rs` | Mine | — |
| `crates/mh-service/src/session/release.rs` | Mine | — |
| `crates/mh-service/src/webtransport/close.rs` | Mine | — |
| `crates/mh-service/src/webtransport/connection.rs` | Mine | — |
| `crates/mh-service/src/webtransport/server.rs` | Mine | — |
| `crates/mh-service/tests/auth_layer_integration.rs` | Mine | — |
| `crates/mh-service/tests/common/accept_loop_rig.rs` | Mine | — |
| `crates/mh-service/tests/common/tokens.rs` | Mine | — |
| `crates/mh-service/tests/common/wt_client.rs` | Mine | — |
| `crates/mh-service/tests/gc_integration.rs` | Mine | — |
| `crates/mh-service/tests/mc_heartbeat_integration.rs` | Mine | — |
| `crates/mh-service/tests/media_dial_admission_integration.rs` | Mine | — |
| `crates/mh-service/tests/media_session_binding_integration.rs` | Mine | — |
| `crates/mh-service/tests/otel_grpc_integration.rs` | Mine | — |
| `crates/mh-service/tests/otel_webtransport_integration.rs` | Mine | — |
| `crates/mh-service/tests/register_meeting_integration.rs` | Mine | — |
| `crates/mh-service/tests/webtransport_accept_loop_integration.rs` | Mine | — |
| `crates/mh-service/tests/webtransport_close_code_integration.rs` | Mine | — |
| `crates/mh-service/tests/webtransport_integration.rs` | Mine | — |
| `crates/mh-test-utils/Cargo.toml` | Mine | — |
| `crates/mh-test-utils/src/lib.rs` | Mine | — |
| `crates/mh-test-utils/src/media_dial.rs` | Mine | — |
| `crates/mh-test-utils/src/media_policy.rs` | Mine | — |
| `crates/mh-test-utils/src/session.rs` | Mine | — |
| `docs/API_CONTRACTS.md` | Minor-judgment | protocol |
| `docs/TODO.md` | Mine | — |
| `docs/WEBTRANSPORT_FLOW.md` | Minor-judgment | protocol |
| `docs/observability/metrics/mh-service.md` | Minor-judgment | observability |
| `docs/decisions/adr-0003-service-authentication.md` | Minor-judgment | security |
| `docs/runbooks/client-dev-local.md` | Mechanical | client |
| `docs/runbooks/mc-incident-response.md` | Minor-judgment | operations |
| `docs/runbooks/mh-deployment.md` | Minor-judgment | operations |
| `docs/runbooks/mh-incident-response.md` | Minor-judgment | operations |
| `docs/specialist-knowledge/media-handler/INDEX.md` | Mine | — |
| `docs/specialist-knowledge/security/INDEX.md` | Mechanical | security |
| `infra/docker/prometheus/rules/mh-alerts.yaml` | Minor-judgment | observability |
| `infra/grafana/dashboards/mh-overview.json` | Mine | — |
| `infra/services/mh-service/config.env` | Mine | — |
| `infra/services/mh-service/deployment.yaml` | Mechanical | infrastructure |
| `packages/sdk-core/src/media/MediaTransport.ts` | Mechanical | client |
| `proto/dark_tower/internal/v1/internal.proto` | Domain-judgment | protocol |
| `proto/dark_tower/signaling/v1/signaling.proto` | Domain-judgment | protocol |

Notes: `Cargo.lock` = the `mh-test-utils` → `common`/`ring`/`base64`/`serde_json` edges and `jsonwebtoken` moving to `mh-service` `[dependencies]` (no new crates). `docs/runbooks/mh-incident-response.md`: Scenario 24 "Media-Dial Credential Rejections" (reserved anchor) written; Scenario 13 retired to a pointer, its surviving arms moved to Scenario 27. Proto rows: security co-signs (GSA).
---

## Planning

> Lead note (Gate 1): GSA co-signers for both `proto/**` rows are protocol (manifest owner, Owner column) **plus security** (ADR-0024 §6.4 GSA rule); the Owner column holds only the manifest owner per the classification guard.

### Restated as mechanism

"MH's client admission is keyed on a per-meeting verification key that only a registration can install; nothing is held for a meeting without one." The same-owner sibling class is every place that encodes the OLD mechanism (AC-JWKS meeting token at MH accept; "hold until registered"): the validator, the pending structures, the timeout config/metric/panel/runbook arms, the env-test connect fixture, the proto/SDK/runbook prose that describes either. All are in the table above; none is left for a later task (P3 alone deletes the `join_token` FIELD).

### 1. Key intake (`grpc/mh_service.rs`, `session/mod.rs`)
- `MediaDialKey([u8; 32])` in `auth/mod.rs`: `Copy`, hand-written `Debug` printing `MediaDialKey(<32 bytes>)`, no `Display`, no serde, never string-encoded. `MediaDialKey::from_wire(&[u8]) -> Option<Self>` is the ONE length check.
- `register_meeting`: a RECORD reject in the pre-boundary block (beside `heartbeat_interval_ms == 0`): not exactly 32 bytes -> `INVALID_ARGUMENT` `"media_dial_public_key must be exactly 32 bytes"`, nothing upserted (prior key stays in force), WARN with `reason="media_dial_key_length"` (no bytes, no length-of-attacker-data beyond the enum reason).
- `MeetingRegistration.media_dial_key: MediaDialKey` — installed by the UPSERT (`handle_register_meeting`), so a refused apply neither blocks nor rolls it back. Release removes it with the record.
- SameMc re-registration that CHANGES the key: WARN (`mh.session`, fields `meeting_id`, `registered_mc_id`, `caller_mc_id`, `media_dial_key_changed = true`; no bytes/fingerprint; detection only, never blocks the upsert), per `docs/TODO.md` "MH does not verify meeting ownership" (its "unsignalled today" sentence updated to point at the WARN in this commit). Tests: fires on a key change; no WARN on a same-key SameMc re-registration (@security ruling: IN).
- Lookup: `SessionManagerHandle::media_dial_key(meeting_id) -> Option<MediaDialKey>` — read-only actor query; an unknown meeting creates nothing.

### 2. Verification (`auth/mod.rs`, called from `close::admit`)
`MhJwtValidator`, `validate_meeting_token`, the `CommonJwtValidator` re-export and their tests are DELETED. The AC `JwksClient` stays only for `MhAuthLayer`.

Pure, synchronous, split at the one async step (the actor lookup):
- `fn unverified_meeting(token) -> Result<MeetingKey, MediaDialRejection>` — (a) `token.len() > common::jwt::MAX_JWT_SIZE_BYTES` -> `Malformed` (before any decode); (b) `jsonwebtoken::decode_header` (the library's one header parser; no hand-rolled split/base64), require `header.alg == Algorithm::EdDSA` (none/unknown alg fails the parse, HS*/RS*/ES* fail the compare -> `Malformed`); `kid`/`jwk`/`jku`/`x5u`/`x5c` are never read (present-and-well-typed = ignored; a member that is not a valid JOSE type is a malformed header -> `Malformed`, documented at the verifier and pinned by one test with three arms per @security/@code-reviewer: well-formed attacker jwk + token signed by the attacker key -> `signature_invalid`; type-invalid `"kid": 5` -> `malformed`; meeting-key-signed token carrying kid/jwk -> admitted); (c) `jsonwebtoken::dangerous::insecure_decode_claims::<UnverifiedMeetingId>` (private struct holding only `meeting_id`) -> `Malformed` on failure. The unverified id is NEVER logged/traced/labelled.
- async: `media_dial_key(meeting)`; `None` -> `NoMeetingKey`.
- `fn verify_with_key(token, &MeetingKey, MediaDialKey, skew) -> Result<MediaDialClaims, MediaDialRejection>` — (d) `jsonwebtoken::decode::<MediaDialClaims>` with `DecodingKey::from_ed_der(&key.0)` (per call; mirrors `common::jwt::verify_token`'s key construction), `Validation::new(EdDSA)`, `leeway = skew`, `validate_aud = false`, `required_spec_claims = {exp}`; error kinds mapped in ONE `fn rejection_for(&ErrorKind)`: `InvalidSignature` -> `SignatureInvalid`, `ExpiredSignature` -> `Expired`, `_` -> `Malformed` (commented as deliberately fail-closed; incl. an AC claim set signed by the MC key); (e) `common::jwt::validate_iat(iat, skew)` -> `Expired` (a future iat is a clock-window failure; documented); VERIFIED `claims.meeting_id != looked-up meeting` -> `Malformed` (parser disagreement, load-bearing per @security S4); `claims.token_type != common::jwt::MEDIA_DIAL_TOKEN_TYPE` -> `WrongType`, THEN `claims.validate()` (the authority; any error left can only be the empty sub) -> `Malformed` (security fallback ruling; no GSA edit).
- (f) in `close::admit` only after (a)–(e): `claims.aud != handler_id` -> `AudMismatch`.
- `MediaDialRejection {Malformed, NoMeetingKey, SignatureInvalid, Expired, WrongType, AudMismatch}`: `ALL`, `as_label()` (exact ADR-0039 §8 tokens), and ONE `close_code()`-style mapping: `AudMismatch` -> WRONG_HANDLER, all else DECLINED (wildcard-free). `Display` = the label only (no token/claim/jsonwebtoken text).
- `crates/common/src/jwt.rs` is NOT edited: the typed-error variant needs auth-controller (GSA owner) in planning+review and it is not on this team, so per @security's ruling MH uses the fallback above.
- AC meeting token at MH: fails at (c) (`NoMeetingKey`, unregistered) or (d) (`SignatureInvalid`, registered) — never `wrong_type`. The catalog says so; `wrong_type` fires only for an MC-key-signed token of another type.
- No jti cache: nothing recorded per token.

### 3. `close.rs` / `connection.rs` admission
- `close::validate` becomes `close::admit(token, verifier ctx, session_manager, connection_id) -> Result<ValidatedSession, SessionClose>`-shaped: (a)–(e) failure -> `SessionClose::declined()`; after (e) a `ValidatedSession` is built and (f) failure returns `session.closing(ValidatedClose::WrongHandler)` (the only WRONG_HANDLER constructor, so it is reachable only post-validation). Records `mh_jwt_validations_total{token_type="media_dial"}` exactly once on every exit (success/none or failure/<label>); WARN carries `failure_reason` and connection_id only (ops #5).
- `serve_session` reads `MhConnectRequest.media_dial_token` ONLY; `join_token` is never read (non-empty ignored, empty dial token -> `Malformed` -> DECLINED, no fallback). `#[instrument(skip_all)]` on every fn taking the token.
- ONE fn owns the `MediaDialRejection` -> exit-shape mapping (commented: WRONG_HANDLER is a validated session's intentional close, not a credential error). Admission refusals keep today's `Err(MhError::JwtValidation(<generic>))` -> DECLINED + `status="error"` route; WRONG_HANDLER returns `Ok(SessionEnd::Close(..))`. Every admission refusal is a SESSION close via `close_session` (capsule, empty message) — never `end_without_code`.
- Verification happens BEFORE `add`/notify: nothing is held for a refused dial.

### 4. Pending cap (`session/mod.rs`)
- `ConnectionEntry` gains `is_bound: bool`. `add_connection` (fire-and-forget) is replaced by `admit_connection(meeting_id, entry, cap) -> AdmitOutcome {Admitted, MeetingNotHeld, PendingCapReached}` — ONE actor turn does "meeting still registered?" + "count of `!bound` entries for (meeting, sub) < cap?" + insert (S8, S9 atomic). After a successful bind, `mark_bound(meeting, connection_id)`.
- The pending count is DERIVED from the entries themselves, so there is no separate counter to leak: every exit already removes the entry (decline -> `close_declined_connection` -> `remove_connection`; disconnect -> existing removal; release -> `close_active_connections`). I'll rebut a separate RAII counter on that basis, and add a test that a slot frees after each exit (bind ok, sender 0, MC error, release).
- `PendingCapReached` -> DECLINED, new outcome `MediaSessionStartOutcome::DeclinedPendingCap` = `declined_pending_cap` (+ `StartDecline::PendingCap`, close code DECLINED, `mc_may_hold_a_registration` = false, jitter 0; MC was never asked). `MeetingNotHeld` (released between verify and admit) -> DECLINED, not counted as a start (nothing attempted), INFO log.
- Left participant: MC answers 0 -> existing `NoSenderBinding` -> DECLINED. No frame reaches an unbound connection: no subscriber/egress exists until `start_media_session`. Test with a positive control.
- Release while awaiting a bind — CARRIED from ADR-0039 §6 unchanged (no amendment): on a REMOVAL cause (`ReleaseKind::closes_pending`, kept and re-documented as "awaiting bind") those connections are closed `HANDLER_RELEASED`; on `MEETING_ENDED` they are left to the bind outcome (bound entries close `HANDLER_RELEASED` as today). A bind answer for a meeting no longer held binds NOTHING: `sender_id` 0 -> DECLINED (existing), nonzero -> DECLINED (meeting not held; checked atomically with the bind in the actor). The connection's close token is raced against the in-flight `NotifyParticipantConnected`: a removal-cause release closes it `HANDLER_RELEASED` AT the release, and the abandoned answer is a no-op (no second close, never re-coded DECLINED; @client). Tests: both arms (release in flight + nonzero -> nothing bound, DECLINED / HANDLER_RELEASED per cause; same answer with no release -> binds).

### 5. Deletions (pending path, single commit)
`PendingConnection`, `pending_connections`, `meeting_notifiers`, `AddPendingConnection`/`RemovePendingConnection` + handles, `handle_add/remove_pending_connection`, `release_pending_connections`, `register_meeting` returning promoted connections (now `Result<(), RegisterRefusal>`; `promoted_pending_count` log field goes), `RegistrationOutcome`, `await_meeting_registration` + its test module (connection.rs tests ~1557–1735), the session tests for pending (2597–2800, 3125–3160), mh_service.rs tests using `add_pending_connection` (1270, 2142), `register_meeting_timeout` threading (server.rs, main.rs), `MH_REGISTER_MEETING_TIMEOUT_SECONDS` (config field, `DEFAULT_/MAX_` consts, parse, Debug, tests) -> `RETIRED_KEYS` (doc comment generalised: "ignored if set; detects a stale override", not "kept for rollback"), `record_register_meeting_timeout` + zero-init + `present_at_zero` + tests, `webtransport_integration.rs::provisional_connection_kicked_after_register_meeting_timeout`, env-test 26's ignored stub. Completion check: repo grep for `PendingConnection|meeting_notifiers|await_meeting_registration|register_meeting_timeout|REGISTER_MEETING_TIMEOUT|validate_meeting_token|MhJwtValidator` returns zero hits outside `docs/devloop-outputs/`, `docs/debates/`, ADRs and dated user stories.

### 6. Config (`config.rs`, `config.env`, `deployment.yaml`)
- `MH_MEDIA_DIAL_CLOCK_SKEW_SECONDS`: default 30 when unset, `parse_in_range` [0, `MAX_MEDIA_DIAL_CLOCK_SKEW_SECONDS` = 60] inclusive (one const for parse + message; 60/61 boundary tested) (0 allowed: exact clocks; >60 hard fail naming the key and `config.env`, "fix NTP, do not raise skew").
- `MH_MAX_PENDING_CONNECTIONS_PER_PARTICIPANT`: default 2 when unset, `parse_in_range` [1, `MAX_PENDING_CONNECTIONS_PER_PARTICIPANT_CEILING` = 16] (0 is a hard fail: it would decline every dial; 16 is OUR choice — ADR-0039 §5 says only "small" — documented at the const).
- Both REQUIRED `configMapKeyRef`s (no `optional`), in `config.env`, the one MH template, `mh-deployment.md` config reference rows; startup log fields.
- Config gauge `mh_media_dial_clock_skew_seconds` (telemetry-contract MH row "media_dial exp skew | config gauge"). No pending-cap gauge: not in the contract (invent nothing) — @observability, say if you want it anyway.

### 7. Telemetry
- `mh_jwt_validations_total`: zero-init split per token type (service: signature_invalid|expired|scope_mismatch|malformed; media_dial: the six), driven by `MediaDialRejection::ALL` — one vocabulary feeding emit and zero-init; `meeting`/`validation_failed` removed; pin test zero-init == emit vocabulary. Pre-validation envelope failures (no token extracted) record nothing on this series (as today).
- `mh_media_session_starts_total{outcome="declined_pending_cap"}` (enum ALL, zero-init, pin list, catalog "expected near-zero; sustained = redial storm").
- `mh_register_meeting_timeouts_total` deleted with catalog section, mh-overview panel, runbook/mc-runbook refs (re-pointed at `mh_jwt_validations_total{token_type="media_dial",failure_reason="no_meeting_key"}`).
- Session-close catalog: `wrong_handler` now emitted (step f).
- Alert annotations only (rules are OBS-ALERTS'): `MHHighJwtValidationFailures` text re-grounded on media_dial reasons; `MHMediaSessionDeclineRate` description's outcome count. The decline-ratio expr already counts the new outcome in its numerator — intended (a redial storm is a decline).

### 8. Proto comments (with @paired-protocol; wording sent to @security before edit)
Adopt protocol's ITEM 1, 7a, s1–s3. ITEM 7b: protocol's original text (cause-dependent fate per ADR-0039 §6) plus "a nonzero answer for a released meeting binds nothing and closes `DECLINED`". `EndMeetingRequest.cause` comment stays true as written. Point "Today:" at `close::admit`.

### 9. Tests (Tier 1b, `tests/media_dial_admission_integration.rs` + unit tests in `auth/mod.rs`)
ONE forge in `crates/mh-test-utils/src/media_dial.rs` (ring compact JWS, seeded key, arbitrary header JSON for alg none/HS256/kid/jwk/attacker-jwk). Unit table over the pure fns for every reason (paired positive arms). Integration (accept-loop rig + mock MC) — every negative paired with its positive varying one field, each asserting close code + `mh_jwt_validations_total` delta (siblings unobserved) + no `NotifyParticipantConnected` on the mock MC + no held entry:
accepted incl. same token twice sequentially; an EXPIRED AC token in `join_token` beside a valid dial token admitted (the migration case); SDK join-time shape (valid dial token + non-empty AC `join_token`) admitted; AC token with empty dial token DECLINED; cross-meeting key; exp beyond/within a NON-default skew (7 s); wrong token_type; AC meeting token; malformed via BOTH oversize (cap+1 vs exactly cap) and a non-EdDSA alg (none, HS256); unregistered and released meeting (nothing parked); wrong aud -> WRONG_HANDLER; bad signature + wrong aud -> DECLINED; expired + wrong aud -> DECLINED; kid/jwk(+attacker key that signed it) -> outcome unchanged on accept and reject arms, JWKS mock zero hits; pending cap with non-default cap 3 (mock MC holds the Connected RPC; cap-th admitted, cap+1 DECLINED +1 `declined_pending_cap`, other outcomes unobserved; other sub / other meeting admitted; slot frees after each exit: bind ok, sender 0, MC error, release, and the client dropping while its Connected RPC is in flight); left participant (sender 0 -> DECLINED; a frame from a publisher reaches a bound peer and not the pending connection); key 31/33 refused, 32 accepted; refused apply does not roll back the key; last registration's key wins; config tables for both keys (missing, bounds, 0, non-numeric). Existing suites switch their connect helper (`tests/common/tokens.rs`, `wt_client.rs`) to minted dial tokens and register meetings with the forge's public key; `jwks_rig.rs` stays only for `MhAuthLayer` suites.

### 10. Env-tests (paired with @test)
- Fixture switch: `mc_session::mh_connect_frame(media_dial_token)` sets `media_dial_token`, `join_token` empty; `participant::reach(phase, &Participant, url)` resolves the token via `p.sets.ref_for_url(url)` + `dial_token(ref)` (missing token = MC defect, loud); `open_sessions` and every caller in 26/27/28/34/35 switch (28 reads `server.media_dial_token`). Proof grep: `rg -n 'join_token' crates/env-tests/src/fixtures/mc_session.rs crates/env-tests/src/fixtures/participant.rs crates/env-tests/tests` shows only MC `JoinRequest` sites and the empty MH field.
- NEW `tests/46_mh_media_dial_admission.rs` (non-destructive: no pool mutation, no `#[serial]`): valid MC token connects; wrong aud = handler A's token dialled at handler B (baseline 2 handlers per meeting); AC meeting token DECLINED; unheld meeting (all participants leave, EndMeeting lands, redial the saved token, bounded poll) DECLINED; left participant (P leaves, redials -> DECLINED and receives nothing while a still-joined peer DOES receive in the same window). "Dial after the join JWT's lifetime would have expired": dial with a token from a later `token_refresh` set (asserted a DIFFERENT string from the join-time one; bounded wait ~TTL/3) and (i) `join_token` empty, (ii) a NON-valid AC token (signature byte tampered) in `join_token` -> both admitted. Wrong-aud leg fails PRECONDITION (never skips) if the set has < 2 distinct handlers. Unheld-meeting leg waits on a positive release signal (MH registered-meetings / MC end-meeting evidence), then ONE dial -> DECLINED; positive control = the same token admitted earlier in the test.
- 26_mh_quic: AC-JWT accept/forged/oversized legs become media_dial legs (`assert_mh_rejects` keeps its paired valid-dial control).
- 01_mh_deployment_config: both keys as hard refs; retired key absent with a positive control from the same lookup.

### 11. Runbooks / ops
- `mh-deployment.md`: config rows; rollout order MC-D -> SDK C1 -> MH-1; MC rollback below MC-D while MH-1 is live blacks out new meetings' media; MH rollback below MH-1 safe only until P3 ("P3 raises the MH rollback floor to MH-1"); checklist/24h watch rows replace the timeout gate with `mh_jwt_validations_total{token_type="media_dial",result="failure"}` by reason + `declined_pending_cap`; rollback trigger replaced.
- `mh-incident-response.md`: Scenario 2 rewritten for the service-token path + a pointer to Scenario 24 for the client path (signature_invalid = MC key/registration drift; no_meeting_key = ordering bug or dial to a released handler; expired = refresh push or clock; aud_mismatch = stale set; wrong_type = confusion); NEW `### Scenario 24: Media-Dial Credential Rejections` (exact heading, reserved anchor in runbook-alert-changes.md §J) carrying the per-reason triage; Scenario 13 heading kept as a one-line RETIRED pointer (TODO.md links it); its surviving arms (meeting cap, ownership reject / generation wedge, teardown discriminator table) move verbatim to NEW Scenario 27 (24-26 are reserved) and links re-point; severity line; close-code table (`declined` causes incl. pending cap + left participant; `wrong_handler` live).
- `mc-incident-response.md`: timeout refs re-pointed; the old-MH rollback nuance corrected.
- Client docs: `MediaTransport.ts` `arm()` INTERIM sentences and `client-dev-local.md` F30 updated (doc-only).

### Safe-revert unit
The single MH-1 commit (admission switch + pending deletion + fixture switch + config/metric retirement): reverting it can never produce a mixed-credential image.

### Cross-task obligations
(a) two new keys: config.env + template + runbook rows + 01 coverage; (b) retired key/metric/panel/runbook arms removed everywhere, key in RETIRED_KEYS; (c) series changes: catalog + panel + zero-init + ADR-0032 tests; (d) test-gates "MH" token + pending bullets implemented as above; (e) no id labels; tokens/keys never in logs, metrics, traces or errors.

---

## Pre-Work

None

---

## Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Semantic Guard | confirmed |
| Paired Protocol | confirmed |
| Client | confirmed |

---

## Implementation Summary

- **Verifier** (`crates/mh-service/src/auth/mod.rs`, rewritten): `MediaDialKey` (32 raw bytes, redacting `Debug`), `MediaDialRejection` (the six ADR-0039 §8 labels, `is_wrong_handler` = the one code mapping), `unverified_meeting` (a–c: `MAX_JWT_SIZE_BYTES`, `jsonwebtoken::decode_header` + `alg == EdDSA`, `insecure_decode_claims` into a meeting_id-only struct), `verify_with_key` (d–e: EdDSA with the registered key, leeway = skew, `validate_aud=false`, `exp` required, `validate_iat`, verified meeting_id == looked-up, `token_type` then `MediaDialClaims::validate`), `check_audience` (f). `MhJwtValidator`/`validate_meeting_token`/re-export deleted; `MhAuthLayer` imports `common::jwt::JwtValidator` directly.
- **Admission** (`webtransport/close.rs:admit`): the only `ValidatedSession` constructor; records `mh_jwt_validations_total{token_type="media_dial"}` once per dial; WRONG_HANDLER built from the validated session. `connection.rs`: reads only `media_dial_token`; `refusal_exit` is the one exit-shape mapping; `admit_connection` (pending cap, held re-check); the bind RPC races the close token (removal-cause release closes `HANDLER_RELEASED` at the release); `bind_connection` binds nothing for a released meeting (DECLINED). `ClientAdmission` replaces the JWT validator and the registration timeout on `WebTransportServer`/`handle_connection`.
- **Session actor**: key on `MeetingRegistration` (upsert-installed), `media_dial_key` lookup, `AdmitOutcome`/`BindOutcome`, `ConnectionEntry.is_bound` (pending count derived from entries), SameMc key-change WARN, release closes bound connections always and awaiting-bind ones only on removal (`ReleaseKind::closes_awaiting_bind`). Pending path deleted (`PendingConnection`, notifiers, pending messages/handlers, promotion).
- **Key intake** (`grpc/mh_service.rs`): record reject `INVALID_ARGUMENT` for a key that is not 32 bytes, before the upsert.
- **Config**: `MH_MEDIA_DIAL_CLOCK_SKEW_SECONDS` [0,60] and `MH_MAX_PENDING_CONNECTIONS_PER_PARTICIPANT` [1,16], REQUIRED; `MH_REGISTER_MEETING_TIMEOUT_SECONDS` retired to `RETIRED_KEYS`; main.rs skew 300 hardcode gone.
- **Metrics**: `record_media_dial_validation`, per-token-type zero-init (pinned), `declined_pending_cap`, `mh_media_dial_clock_skew_seconds`; `mh_register_meeting_timeouts_total` deleted.
- **Tests**: unit tables in `auth/mod.rs`, session actor (cap, bind-after-release both arms, key lookup/last-wins, key-change WARN), `mh_service.rs` (31/33/0 refused, refused apply installs key), config tables; Tier 1b `tests/media_dial_admission_integration.rs` (15 tests); existing suites migrated to dial tokens. ONE forge: `crates/mh-test-utils/src/media_dial.rs`.
- **Env-tests**: fixture switch (`mc_session.rs`, `participant.rs`; callers 26/27/28/34/35), new `tests/46_mh_media_dial_admission.rs`, `01_mh_deployment_config.rs` keys.
- **Proto comments, docs, infra, dashboards, runbooks**: per the plan (Scenario 24 written, Scenario 13 retired → 27).

---

## Files Modified

See the Cross-Boundary Classification table (one row per changed file). Tests deleted with the pending path: `webtransport_integration.rs` `provisional_connection_kicked_after_register_meeting_timeout`, `provisional_connection_survives_when_register_meeting_arrives_within_window`; `webtransport_close_code_integration.rs` `a_registration_timeout_closes_with_declined` (replaced by the unheld-meeting test); `media_session_binding_integration.rs` `a_connection_promoted_after_provisional_accept_also_binds`; connection.rs `await_meeting_registration` unit tests; session pending/notifier tests (replaced by admit/bind/key tests); env-test 26's ignored register-timeout stub.

---

## Gate 3 — Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 1 | 1 | 0 | F-SEC-1 verified-key re-check at admit |
| Test | RESOLVED-FIXED | 7 | 7 | 0 | |
| Observability | RESOLVED-FIXED | 5 | 5 | 0 | attacker-drivable reasons investigate-only |
| Code Quality | RESOLVED-FIXED | 7 | 7 | 0 | |
| DRY | RESOLVED-FIXED | 3 | 2 | 0 | F1 withdrawn (Lead Gate-1 ruling) → TODO extraction opportunity |
| Operations | RESOLVED-FIXED | 3 | 3 | 0 | |
| Semantic Guard | RESOLVED-FIXED (SAFE) | 1 | 1 | 0 | metrics-path completeness (catalog definition) |
| Paired Protocol | RESOLVED-FIXED | 3 | 3 | 0 | |
| Client | CLEAR | 0 | 0 | 0 | |

---

## Accepted Deferrals

- (none surfaced in this devloop)

DRY extraction opportunity: `docs/TODO.md` §Cross-Service Duplication — typed MediaDialClaims::validate() error (GSA)

---

## Rollback Procedure

1. Start commit: `7001bec9865b2ffb27f3372c652349c2f56c7050`
2. `git diff 7001bec9..HEAD`; `git reset --soft|--hard 7001bec9`
3. **Safe-revert unit**: the single MH-1 commit (admission switch + pending-path deletion + env-test fixture switch + config/metric retirement). Never revert part of it: a partial revert yields an MH that accepts neither or both credential types, or env-tests that dial with the wrong one.

---

## Issues Encountered & Resolutions

### Issue 1: Gate 2 Layer 7 attempt 1 FAIL
**Problem**: 7 env-tests injecting RegisterMeeting directly lacked a 32-byte media_dial_public_key (refused InvalidArgument); 46 left-participant leg converge failure.
**Resolution**: routed to implementer (iteration 2): shared `mh_grpc::fixture_dial_public_key()` for direct registrations; 46 left leg uses a clean close + await ParticipantLeft. Test re-reviewed (RESOLVED-FIXED). Gate 2 attempt 2: `layer-all.sh` TOTAL_RESULT=OK (L1-7 OK, env-tests + browser E2E passed).

---

## Lessons Learned

1. Credential switches must sweep every direct-gRPC fixture that installs the credential's verifying material (env-test RegisterMeeting injectors), not only the client-dial fixtures — Layer 7 caught three missed sites.
2. Env-test "participant left" legs need a clean close (voluntary leave); an abrupt drop keeps the participant on MC's roster for its disconnect grace window.
3. A verify-then-admit split across actor turns needs the verifying key re-checked at admit (F-SEC-1).
