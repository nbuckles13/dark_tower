# Devloop Output: MC-D — MC-minted media_dial credential, delivery and refresh; ref-keyed client status and FAILED re-push

**Date**: 2026-11-10
**Task**: Story 4 task MC-D (run-story task #16): MC-minted media_dial credential, delivery and refresh; ref-keyed client status and FAILED re-push
**Specialist**: meeting-controller
**Mode**: Agent Teams (v2) — full, Gate-1 present (`--paired-with=security`, `--paired-with=test`, `--paired-with=auth-controller` — GSA owner for `crates/common/src/jwt.rs`, added by Lead at Gate 1)
**Branch**: `feature/mh-failure-transfer`
**Duration**: ~3h

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `7b7e62cee44437497ca2dce1fa4b5d673efed899` |
| Branch | `feature/mh-failure-transfer` |
| Lead Model | `claude-opus-5-5` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` |
| Implementing Specialist | `meeting-controller` |
| Tier | `full` |
| Iteration | `1` |
| Security | `paired-security` (RESOLVED-FIXED) |
| Test | `paired-test` (RESOLVED-FIXED) |
| Observability | `observability` (RESOLVED-FIXED) |
| Code Quality | `code-reviewer` (RESOLVED-FIXED) |
| DRY | `dry-reviewer` (RESOLVED-FIXED) |
| Operations | `operations` (RESOLVED-FIXED) |
| Semantic Guard | `semantic-guard` (RESOLVED-FIXED) |
| Auth Controller (GSA owner, jwt.rs) | `paired-auth-controller` (RESOLVED-FIXED) |

---

## Task Overview

### Objective
See task prompt (run-story task #16, story `docs/user-stories/2026-10-31-survive-handler-loss.md`, ADR-0039 §4/§5). MC signs a short-lived per-handler media_dial credential locally and keeps every participant supplied with a valid one for every handler in its set.

### Scope
- **Service(s)**: mc-service (+ env-tests, docs, semantic checks)
- **Schema**: No
- **Cross-cutting**: No (consumer of P1/P2 contracts)

### Debate Decision
NOT NEEDED - governed by ADR-0039.

---

## Cross-Boundary Classification

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `crates/mc-service/Cargo.toml` | Mine | |
| `crates/mc-service/src/lib.rs` | Mine | |
| `crates/mc-service/src/media_dial/mod.rs` | Mine | |
| `crates/mc-service/src/media_dial/key.rs` | Mine (ADR-0027 primitive call site: Ed25519 generate/sign; security in Gate 1/3) | |
| `crates/mc-service/src/media_dial/refresh.rs` | Mine | |
| `crates/mc-service/src/media_dial/status.rs` | Mine | |
| `crates/mc-service/src/actors/meeting.rs` | Mine | |
| `crates/mc-service/src/actors/meeting_media.rs` | Mine | |
| `crates/mc-service/src/actors/messages.rs` | Mine | |
| `crates/mc-service/src/actors/participant.rs` | Mine | |
| `crates/mc-service/src/auth/mod.rs` | Mine (tests + one const swap) | |
| `crates/mc-service/src/config.rs` | Mine | |
| `crates/mc-service/src/grpc/gc_client.rs` | Mine | |
| `crates/mc-service/src/grpc/mh_client.rs` | Mine | |
| `crates/mc-service/src/main.rs` | Mine | |
| `crates/mc-service/src/media_routing/placement.rs` | Mine | |
| `crates/mc-service/src/media_routing/pusher.rs` | Mine | |
| `crates/mc-service/src/observability/metrics.rs` | Mine | |
| `crates/mc-service/src/webtransport/connection.rs` | Mine | |
| `crates/mc-service/src/webtransport/handler.rs` | Mine | |
| `crates/mc-service/src/webtransport/server.rs` | Mine | |
| `crates/mc-service/tests/common/accept_loop_rig.rs` | Mine | |
| `crates/mc-service/tests/common/media_session.rs` | Mine | |
| `crates/mc-service/tests/common/mod.rs` | Mine | |
| `crates/mc-service/tests/common/media_dial.rs` | Mine | |
| `crates/mc-service/tests/gc_integration.rs` | Mine | |
| `crates/mc-service/tests/handler_set_integration.rs` | Mine | |
| `crates/mc-service/tests/media_client_signaling_integration.rs` | Mine | |
| `crates/mc-service/tests/media_policy_push_integration.rs` | Mine | |
| `crates/mc-service/tests/otel_grpc_outbound_integration.rs` | Mine | |
| `crates/mc-service/tests/register_meeting_integration.rs` | Mine | |
| `crates/mc-service/tests/media_dial_integration.rs` | Mine | |
| `crates/mc-service/tests/media_dial_log_hygiene.rs` | Mine | |
| `docs/specialist-knowledge/meeting-controller/INDEX.md` | Mine | |
| `infra/services/mc-service/config.env` | Mine | |
| `crates/common/src/jwt.rs` | Not mine, Domain-judgment (GSA: `MediaDialClaims` + `MEDIA_DIAL_TOKEN_TYPE`, the claims SSoT for MC mint and MH-1 verify) | auth-controller |
| `crates/ac-service/src/handlers/internal_tokens.rs` | Not mine, Minor-judgment (literal "meeting"/"guest" to common consts, value-neutral; owner-approved P3) | auth-controller |
| `crates/mh-service/src/auth/mod.rs` | Not mine, Minor-judgment (literal "meeting" to `MEETING_TOKEN_TYPE`) | media-handler |
| `infra/services/mc-service/mc-0-deployment.yaml` | Not mine, Minor-judgment (one hard configMapKeyRef) | infrastructure |
| `infra/services/mc-service/mc-1-deployment.yaml` | Not mine, Minor-judgment (lockstep with mc-0) | infrastructure |
| `infra/grafana/dashboards/mc-media.json` | Not mine, Minor-judgment (two panels) | observability |
| `docs/observability/metrics/mc-service.md` | Not mine, Minor-judgment (catalog entries; drop-line wording) | observability |
| `docs/runbooks/mc-deployment.md` | Not mine, Minor-judgment (TTL config row) | operations |
| `scripts/guards/semantic/checks.md` | Not mine, Domain-judgment (credential-leak items 11-13 policy text; owed-poles paragraph) | security |
| `crates/dt-guard/tests/fixtures/credential_leak_key_custody/lens/pos_mc_log_dial_signing_key.rs` | Not mine, Domain-judgment (lens pole = policy content) | security |
| `crates/dt-guard/tests/fixtures/credential_leak_key_custody/lens/pos_contract_crossing_dial_signing_key.rs` | Not mine, Domain-judgment | security |
| `crates/dt-guard/tests/fixtures/credential_leak_key_custody/lens/pos_derive_debug_raw_dial_key_bytes.rs` | Not mine, Domain-judgment | security |
| `crates/dt-guard/tests/fixtures/credential_leak_key_custody/lens/pos_media_dial_token_raw_payload.rs` | Not mine, Domain-judgment | security |
| `crates/dt-guard/tests/fixtures/credential_leak_key_custody/lens/neg_media_dial_public_key_register.rs` | Not mine, Domain-judgment | security |
| `crates/dt-guard/tests/fixtures/credential_leak_key_custody/lens/neg_dial_signing_key_redacting_wrapper.rs` | Not mine, Domain-judgment | security |
| `crates/dt-guard/tests/fixtures/credential_leak_key_custody/lens/pos_contract_crossing_dial_seed_renamed.rs` | Not mine, Domain-judgment | security |
| `crates/dt-guard/tests/credential_leak_key_custody_fixtures.rs` | Not mine, Domain-judgment (pole-count bump with the poles) | security |
| `crates/env-tests/tests/27_mc_slot_placement.rs` | Not mine, Minor-judgment (empty-token assertion flips to token-per-member) | test |
| `crates/gc-service/tests/mh_replacement_tests.rs` | Not mine, Minor-judgment (test-only flake fix, Lead ruling) | global-controller |
| `crates/env-tests/Cargo.toml` | Not mine, Minor-judgment (comment only: the dependency-policy citation no longer misattributes it to ADR-0028) | test |
| `crates/env-tests/README.md` | Not mine, Minor-judgment (same citation fix) | test |
| `infra/grafana/dashboards/mc-overview.json` | Not mine, Minor-judgment (status-drop panel title and description gain `rate_limited`) | observability |
| `crates/env-tests/tests/02_mc_deployment_config.rs` | Not mine, Minor-judgment (new WIRED_KEYS entry) | test |

---

## Planning

### Mechanism restatement

Instance: "mint a dial token per handler, refresh it, re-push on FAILED". Mechanism: **every client-held credential that MH accepts is a pure function of (meeting dial key, roster identity, Active handler, MC clock). MC keeps each connected participant's holding fresh by construction: it pushes a full set at every point where the holding could go stale or wrong (join, membership change, TTL/3, client-reported FAILED). The private key never leaves the meeting actor, and only its 32-byte public half crosses any boundary.** The same-owner siblings this surfaces are all in scope here:
- (1) Every carrier of `MediaServerInfo` has to carry the token. That means `JoinResponse.media_handler_set`, the legacy `JoinResponse.media_servers` (same projection, so the same tokens), and the `ServerMessage` set.
- (2) The not-yet-wired `handle_reconnect` path is a "(re)join" too. It gets a `reason=join` push, so the invariant cannot be broken when that path is wired.
- (3) `MhConnectionStatus` intake is a client-driven hop into the SHARED meeting-actor mailbox. Every other such hop is bounded at the connection, so this one is bounded too (F below).
- (4) The `media_handler_set` drop doc and catalog line say "no re-sync until the next membership change". That stops being true here and is corrected in the same commit.

### A. Key (§5, L4-1): `media_dial/key.rs` (new module `crates/mc-service/src/media_dial/`)
- `DialSigningKey(ring::signature::Ed25519KeyPair)`, private field. It has no Clone, Serialize, Display or Deref, and no raw-bytes accessor. Its hand-written `Debug` prints the constant `DialSigningKey([REDACTED])`; ring's Debug is never relied on.
- `DialSigningKey::generate(&SystemRandom) -> Result<Self, DialKeyGenerationFailed>`: `generate_pkcs8`, then `from_pkcs8`. The PKCS#8 `Document` is dropped at the end of the function. It is never stored, cloned or handed to jsonwebtoken (S1).
- The public API is exactly `public_key() -> [u8; 32]` and `sign_media_dial(&MediaDialClaims) -> Result<MediaDialToken, MintError>`.
- The meeting actor field is `dial_signing_key: DialSigningKey`, generated in `spawn_inner` right after the KEK from the SAME `SystemRandom` the KEK uses, as a sibling of `admission`. No KEK path (`rotate`, epoch reset) can reach it, because those functions do not take the actor.
- RNG failure means `spawn_inner` returns `McError::Internal("Failed to initialise meeting key material")`, the same fail-closed arm as the KEK. There is no fallback. The seeded jitter RNG (C) is a different type, and no seam reaches key generation (S2).
- It is never persisted (Redis untouched), logged, traced or serialized, it dies with the actor, and it is never rotated.
- One INFO line at creation is extended. It says only "dial key provisioned" and carries no key bytes or fingerprint.
- `MediaDialToken(String)` is a newtype with a redacting Debug and `into_wire(self) -> String`. Its only consumer is the projection into `MediaServerInfo.media_dial_token`.
- `MintError` has two variants: `ExpOverflow`, and `Encode` (serde_json of a fixed struct). Neither carries a payload, and its Display is fixed text (S10).

### B. Token (§5, security ruling)
- **Hand-signed compact JWS (security S1 over DRY #2).** The header is the const bytes `{"alg":"EdDSA","typ":"JWT"}`, so a `kid` cannot exist by construction. The signing input is `b64url(header).b64url(claims_json)`, then `keypair.sign()`, and the token is `input.b64url(sig)` (base64 URL_SAFE_NO_PAD, already a dep). Not using jsonwebtoken's `EncodingKey` avoids the second, non-redacting DER copy of the private half. DRY agreed a second production signer is not duplication.
- **Claims SSoT, needing a Lead decision because it is a GSA.** I plan `MediaDialClaims { sub, token_type, meeting_id, aud, iat, exp }` plus `pub const MEDIA_DIAL_TOKEN_TYPE: &str = "media_dial"` in `crates/common/src/jwt.rs`, beside `MeetingTokenClaims`, with a redacting Debug on `sub`. MC serializes through it and MH-1 deserializes the same type, so field names, `aud` and the literal cannot drift (DRY #1). This is a GSA edit with owners auth-controller and security, and **auth-controller is not on this loop**. Lead: either add auth-controller, or rule MC-local with an MH-1 hoist obligation. My recommendation is common.
- **`sub` = the roster entry's `user_id`**, which is the validated AC meeting-token `sub` stored at join. I deliberately did not use `participant_id`. MH's binding RPC passes the token's `sub` as `NotifyParticipantConnectedRequest.participant_id`, and MC resolves that against `user_id` (`media_coordination.rs:resolve_and_record`, `meeting.rs:resolve_user`). A dial token carrying the MC participant_id would therefore make every MH-1 binding `ParticipantUnknown`. The value comes from the actor's roster, never from a request (S5). @paired-security: this refines your S5. "What the binding answer keys on" is `user_id`.
- `aud` = `HandlerId` (identity string, not the url). `meeting_id` = the actor's own id. `token_type` = `MEDIA_DIAL_TOKEN_TYPE`. `exp = iat.checked_add(ttl)`, and overflow is `MintError::ExpOverflow` (S7).
- **Clock (S7, paired-test #1).** `DialClock` is an enum. `Wall` reads `SystemTime` and is the production default. `Anchored { base_unix, base: tokio::time::Instant }` is selected only through `MeetingSeams` under `test-seams`, and gives `iat = base_unix + (Instant::now() - base)`, so paused time moves `iat` and the refresh arm together. Only the time source forks; the mint path is the same.
- **Mint scope (S8).** One function, `mint_set_for(participant) -> Result<MediaHandlerSet, MintError>`. It is called only for a roster member, over the meeting's Active client snapshot, which is register-before-token from MC-A. Every entry gets a token.

### C. Delivery and refresh (§5, L4-3): `media_dial/refresh.rs`, driven from `meeting.rs`
- **Projection.** `ClientHandlerEntry` gains `handler_id: HandlerId`. It is internal and never projected onto the wire, so S-4 from MC-A holds. `webtransport/handler.rs::media_handler_set(set, mint: impl FnMut(&HandlerId) -> Result<MediaDialToken, MintError>) -> Result<MediaHandlerSet, MintError>` stays THE one projection and now fills `media_dial_token` per entry.
- **Hop (S9, SEC-A ruling).** `JoinResult.handler_set` becomes the generated `MediaHandlerSet`, whose `skip_debug` Debug redacts. It is minted by the actor at reply time: at the join, or at park release (re-minted then). `ParticipantMessage::HandlerSet { set: MediaHandlerSet, reason: HandlerSetPushReason }` is typed and encoded in `participant.rs::handle_handler_set`, the `handle_kek_update` analogue. A token never travels as `SignalingPayload::Raw`, `envelope()` or `send_message()`. The legacy `media_servers` is `media_handler_set.handlers.clone()`, with the same tokens, delivered to the entitled holder.
- **Reasons and precedence (observability #2).** `HandlerSetPushReason { Join, Change, FailedReport, TokenRefresh }` has `ALL` and `label()`. Each message gets exactly one reason, with precedence join > change > failed_report > token_refresh. `Join` covers the JoinResponse (counted at the connection when the JoinResponse write succeeds, next to `record_session_join("success")`) and a reconnect push. `Change` covers the membership-change broadcast. **Counting is at successful enqueue:** a `try_send` Ok means a push, and an Err means `mc_participant_outbound_messages_dropped_total{payload_kind="media_handler_set"}`. That is the EXISTING value from MC-A, reused (observability #1; DRY #7 superseded because the value exists). So pushes + drops = attempts.
- **Change stays an immediate broadcast** in the turn that changes membership (as MC-A does today). It is now minted per participant. This keeps "the set precedes any directive naming the new ref" on each ordered queue. Each change push clears that participant's pending refresh or failed entry and reschedules it.
- **Refresh schedule.** `DialRefresh { next_due: BTreeMap<participant_id, Instant>, pending: BTreeMap<participant_id, reason>, rng: StdRng }`.
  - After any push to P: `next_due[P] = now + interval - jitter`, where `jitter ∈ [0, interval/10]` uniform from the seeded `StdRng`. The jitter is subtracted only, so the maximum gap between pushes is `interval = TTL/3 < TTL`.
  - Production seeds from `rand::make_rng()`, a non-crypto RNG that is fully separate from `SystemRandom`. `MeetingSeams.dial_refresh_seed: Option<u64>` fixes it in tests.
  - The `rand` workspace dep moves from dev-only to `[dependencies]`.
  - A `select!` arm on `sleep_until(min next_due)` moves due participants into `pending` (TokenRefresh). A ready-arm `if dial_refresh.has_pending()` flushes at most the same per-turn bound as the slot-view flush (`flush_batch`, i.e. `SLOT_VIEW_FLUSH_BATCH`, with the seam override), so it coalesces per turn and never starves the mailbox.
  - Only connected, non-parked roster members are pushed. On leave or disconnect, the participant's entries are removed (S8), and a reconnect re-enters with a join push.
  - No per-MC limiter (user ruling).
- **Version.** A token-only refresh re-snapshots `client_snapshot()`, so the version and refs come from the set itself, and only a membership change moves them.
- **Worst case.** A token held by an in-set client expires only after about 3 consecutive dropped pushes (TTL/3 cadence). This is documented in the catalog and the drop doc.

### D. RegisterMeeting carries the public half (§5)
- `MeetingMedia::new(meeting_id, handlers, media_dial_public_key: [u8; 32])`. `PushTarget.media_dial_public_key: [u8; 32]` becomes `MeetingProgramming.media_dial_public_key: [u8; 32]`, which `mh_client.rs` writes as `.to_vec()` on EVERY RegisterMeeting for the meeting.
- Only the 32 bytes reach the push worker. It never gets the key, the wrapper or an Arc of either (S4).
- `built_request_carries_no_key_material` is re-derived: the field equals a known verifying key's bytes, and it differs from the KEK bytes and from the seed and the PKCS#8 (distinguishing inputs).

### E. Config (§5, R-37 pattern)
- `MC_MEDIA_DIAL_TOKEN_TTL_SECONDS` is parsed through the existing `bounded()` with `[60, 900]`. Missing (REQUIRED, ops 1), unparseable, 59 or 901 refuses boot with an error naming the key (S15).
- `Config::media_dial() -> MediaDialTiming { ttl }`, and `MediaDialTiming::refresh_interval()` = TTL/3 is the ONE derivation. Both config gauges and the scheduler call it (observability #3, DRY #4).
- `MediaDialTiming` rides `MediaRoutingDeps`, next to `heartbeat`, so no spawn/controller signature churn. Mint and refresh both happen only after the first join has installed deps.
- Gauges are set once at boot in `main.rs`.
- The key is added to `config.env` (`300`) and as a HARD `configMapKeyRef` in mc-0 and mc-1. It gets a runbook row (default, range, TTL/3 refresh, jitter `[0, TTL/30]` earlier-only, the boot-refusal sentence, and the rollout and rollback sentence on the MC-A pattern) and a test-02 `WIRED_KEYS` entry.

### F. Client status by ref and FAILED re-push (§4, §5): `media_dial/status.rs` + `connection.rs`
- **Connection** (`handle_media_connection_update`). The legacy url-keyed participant-actor recording stays unchanged and non-routing.
  - Additionally, entries with `handler_ref != 0` are mapped to `ClientHandlerReport { handler_ref: NonZeroU32, handler_set_version, state }`, with no client strings forwarded. They go to the meeting actor as `MeetingMessage::ClientHandlerReports { participant_id, reports }`, where the participant_id is the connection's own and is never taken from the message.
  - The hop is gated by a new per-connection `ClientWorkLimiter` in `MediaSignalingContext` (`MH_STATUS_FORWARD_BURST = 32`, refill `250 ms`), as every other client-driven path into the shared actor mailbox is.
  - Over-rate batches are not forwarded and are counted on the EXISTING `mc_participant_mh_status_dropped_total` with a new `reason="rate_limited"` value (catalog, zero-init, test). **@observability:** that is a third value on an existing bounded counter, not a new series. If you would rather not have it, the fallback is a once-per-connection debug log.
- **Actor store** `ClientHandlerStatuses` holds, per participant, a `BTreeMap<NonZeroU32 ref, LatestReport { state, handler_set_version }>`, plus a per-participant `failed_repush_limiter: ClientWorkLimiter` (`FAILED_REPUSH_BURST = 4`, refill `2_000 ms`). The limiter is keyed by the session's roster entry, so it lives for the session (S13; DRY #8, no new limiter type).
  - It is a third map on purpose. It is neither `connectivity.rs` (server-observed, routing) nor `ParticipantActor::mh_statuses` (url-keyed, legacy). A one-line comment at the store says why.
  - Each report resolves its ref by EXACT match against the meeting's CURRENT Active snapshot, which is the set every participant was last sent. In set, it replaces the latest for that (participant, ref). Out of set (0 is unrepresentable, plus removed or never-issued refs), it is ignored and creates no entry.
  - The version is stored for correlation only, never as a discard criterion, so a stale version on an in-set ref is still recorded.
  - Entries are pruned when a ref leaves the set (at every membership change and on the test removal seam) and when the participant leaves. Size is bounded by the set size.
  - NEVER read by routing, edges or slots. Its consumer is MC-E.
  - A test-seams query, `MeetingMessage::ClientHandlerStatusesForTest`, reads it (paired-test #9).
- **FAILED re-push.**
  - If ANY in-set entry in the batch is FAILED, the actor calls `failed_repush_limiter.try_spend(now)` BEFORE any minting.
  - If it is granted, the participant is marked pending with `FailedReport`. It is flushed in the same turn's bounded flush, to that participant only, and has no routing effect.
  - If it is refused, nothing happens beyond one debug line per participant, once, with no token and no ref value.
  - An out-of-set FAILED spends no token.
  - The rule is already in the proto contract comment: P2 wrote "A FAILED report for an in-set ref MAY trigger a rate-limited credential re-push of the set to its sender only", and "NEVER routing". So **no proto edit**.

### G. KEK guard by construction (item 9), in `auth/mod.rs` tests, reusing its existing `TestKeypair` (DRY #3)
- **Arm a.** A token from the REAL `DialSigningKey::sign_media_dial` gives exactly `JwtError::MissingKid` from the inner `common::jwt::JwtValidator`, and the wiremock JWKS hit count is 0. `validate_meeting_token` and `validate_guest_token` both return `McError::JwtValidation`.
- **Arm b.** The same MC-minted claim set is re-signed by a foreign key, with a header kid that IS in the served JWKS. That gives the signature variant (the exact variant is asserted), and the JWKS is consulted. Its control is the same token shape signed with the live key, which gets past the signature, so the signature is the only difference.
- **Arm c.** An AC meeting token is accepted.
- @paired-test #10: GC's r15 pins exist from GC-3, so there is no GC work here.

### H. Telemetry (contract §MC). All three series carry `key_custody="operator"` and no id labels
- `mc_media_handler_set_pushes_total{reason}`: zero-initialised over `ALL`, with a panel `sum by (reason)(rate(...))` on mc-media.
- `mc_media_dial_token_ttl_seconds` and `mc_media_dial_token_refresh_interval_seconds`: one stat panel by pod.
- Catalog entries for all three. The `media_handler_set` drop line is reworded.
- ADR-0032 tests per reason:
  - a coalesced refresh + failed in one turn counts failed_report once and token_refresh zero;
  - a refresh with no change shows change unobserved;
  - a full channel increments the drop counter and not pushes;
  - the gauges are checked at 60/20 and 900/300.
- Any `#[instrument]` on mint, refresh or push uses `skip_all`. Logs carry meeting_id or handler_id only as log fields.

### I. checks.md + lens poles (task item 10, SEC-A owed poles)
- Items 11-13 are extended by name to the dial signing key and minted tokens: never in logs, metrics, traces, errors or Debug. The language-scope header is kept and the pii_vocabulary list is not restated.
- The "Story-4 poles are owed" paragraph is replaced with a pointer to the planted poles.
- Six lens fixtures, listed in the classification table:
  - must fire: the key in a `tracing` macro, the key crossing into an `internal.v1` construction, a `derive(Debug)` over raw dial key bytes, and a token in `SignalingPayload::Raw`;
  - must not fire: the public half on `media_dial_public_key`, and the redacting wrapper.
- The count in `credential_leak_key_custody_fixtures.rs` is bumped.
- `dial_signing_key` contains the existing `CREDENTIAL_TOKENS` term `signing_key` (verified), so **no pii_vocabulary edit**.

### J. Tests (paired-test #1-#12; `current_thread` + `start_paused`; non-default TTL 120, refresh 40; seeded jitter)
- **Unit tests in key.rs:**
  - header JSON is exactly two keys, with no kid;
  - signature verified with an independent `ring::signature::UnparsedPublicKey(ED25519, public_key())`;
  - `exp - iat == ttl`; ExpOverflow;
  - `{:?}` of the key, the token and the `ParticipantMessage::HandlerSet` contains none of the token, or the seed or PKCS#8 in b64url, b64 or hex (positive control: the token is non-empty).
- **Unit tests in refresh.rs:**
  - offsets are within `[interval - interval/10, interval]`, with max < TTL;
  - 3 or more participants produce more than one distinct offset;
  - the same seed gives the same schedule;
  - the precedence table.
- **Unit tests in status.rs:**
  - latest per ref;
  - out-of-set refs ignored;
  - stale version recorded;
  - pruning;
  - the limiter is spent only for in-set FAILED.
- **Config:** 59/901/"abc" are rejected with the key named; 60/900 are accepted; absent is 300; the gauges equal TTL and TTL/3.
- **Tier 1b `tests/media_dial_integration.rs`:**
  - The wire key is captured from the mock MH's `MeetingProgramming.media_dial_public_key`. It is byte-equal to the key every token verifies under, and differs from the JoinResponse KEK. Two meetings give different keys, and A's token fails against B's key.
  - Every entry carries a token across 2 handlers. Every token has `sub` = own `user_id` and != the other member's, `aud` = that handler's id, `meeting_id`, and `exp = iat + 120`.
  - Multi-TTL walk: steps of TTL/20 over 5 TTLs. At each step it decodes the newest token per in-set ref from each participant's queue and asserts `exp > clock_now`. Positive control: a non-drained snapshot of the first token is observed expired after TTL.
  - Token-only refresh: same version and refs, and different token bytes. `remove_handler_for_test` + broadcast advances the version.
  - FAILED with 3 participants: one frame to P1, zero to P2 and P3, the same version, and `failed_report` +1. `FAILED_REPUSH_BURST + 1` reports give exactly `FAILED_REPUSH_BURST` pushes, with the constant read from the crate. An out-of-set FAILED gives no push and leaves `failed_report` unobserved. The slot and routing snapshot is unchanged.
  - Latest status: DISCONNECTED then CONNECTED gives CONNECTED. A stale-version report is recorded. A report after the ref is removed is ignored.
  - TRACE log capture over mint, refresh, FAILED and RegisterMeeting contains no token and no key encoding (positive control: meeting_id is present).
  - AC JWKS absence: the auth fixture JWKS has no `x` equal to the wire key.
  - Existing `handler_set_integration.rs` and `media_client_signaling_integration.rs` assertions that `media_dial_token` is empty are flipped.
- **Env-test 27 (paired-test #12, option b; a deviation to note).** The meeting's public key exists only on MC→MH RegisterMeeting, and MH does not persist it before MH-1, so there is no honest Layer-7 source without a new debug surface (not adding one).
  - The test asserts that every member of a real join's set carries a non-empty token, with header exactly `{"alg":"EdDSA","typ":"JWT"}`, `token_type=media_dial`, `meeting_id`, `sub` == the participant's own meeting-token sub and != the peer's, and `aud` distinct per entry.
  - It also asserts `exp - iat ==` the deployed TTL from the MC ConfigMap, and that pushes{reason=join} rose.
  - Signature verification against the REGISTERED key is proven at Tier 1b, and MH acceptance is MH-1's.
  - Tokens are never printed in assertion messages.
- Test 32 stays green, and test 02 gains the key.

### K. Rollback
This is a single MC revert unit, plus the ConfigMap key. A pre-MC-D image ignores the key. MH ignores `media_dial_public_key` and the client ignores tokens until MH-1/C1, so rolling back loses nothing that is live.

### Gate-1 rulings and reviewer conditions
- Lead: `MediaDialClaims` + `MEDIA_DIAL_TOKEN_TYPE` in `crates/common/src/jwt.rs`; auth-controller on the loop as paired GSA owner (with security).
- Observability: `reason="rate_limited"` approved on `mc_participant_mh_status_dropped_total`, as a named const added to `MH_STATUS_DROP_REASONS` (zero-init). `cap` / `over_limit` become consts too. Catalog: mutually exclusive with the other reasons, and counted once per batch. Test: an over-rate batch moves `rate_limited` by 1 and `over_limit` by 0. Env-test 27 reads the TTL from the live gauge `mc_media_dial_token_ttl_seconds` (gauge_by_pods), with the ConfigMap as a cross-check.

- Security N1-N3 (carried into implementation):
  - N1: `DialClock::Anchored` is `#[cfg(feature = "test-seams")]` on the variant itself, so a release build cannot construct it.
  - N2: `MediaDialClaims.sub` has a redacting Debug. There are no From/Into, serde aliases or shared deserialize path with the meeting or guest claims.
  - N3: no token, segment or claims Debug in any assertion or panic message; field-equality asserts only.
- Paired-test addenda:
  1. Env-27 drops the shared-cluster `pushes{join}` check. Each `aud` must instead be in the live MH handler-id set (kube fixture), and `|iat - wall clock|` must be within a small skew.
  2. Connection limiter: `MH_STATUS_FORWARD_BURST+1` batches give exactly BURST forwarded (via the status query), `rate_limited` +1 with the others unobserved, and one more batch forwarded after the refill. The constants are read from the crate.
  3. FAILED limiter refill arm, plus per-participant isolation (P1 exhausted, P2 still granted).
  4. `reason=join` delta through a real connection, and `media_servers` tokens byte-identical to `media_handler_set`.
  5. After leave or disconnect, no further pushes to P. A parked joiner gets no refresh before release, then the release-minted set.
  6. A change push at t means the next refresh is no earlier than `t + interval - interval/10`.
  7. Multi-TTL walk: the token bytes differ per refresh, only current-set refs are checked, and one handler is removed mid-walk.
  - The sub test uses members whose `user_id` and `participant_id` both differ, and asserts `sub == user_id` AND `sub != participant_id`.
- DRY: the env-test claims decode uses `common::jwt::MediaDialClaims`, so the claims shape exists only once.

- Semantic-guard:
  - adds the must-fire pole `pos_contract_crossing_dial_seed_renamed.rs`: seed or PKCS#8 bytes, renamed `bytes`, assigned into `media_dial_public_key`. It has a deliberate-leak banner, uses synthetic bytes, is not compiled into mc-service, and comes with a count bump;
  - Debug-absence tests also cover `JoinResult` and the set-carrying `ServerMessage`.
- Code-reviewer:
  - **Interop:** a key.rs unit test decodes the minted token with `jsonwebtoken::decode::<MediaDialClaims>` (dev-dep), using `DecodingKey::from_ed_der(public_key())`, `Algorithm::EdDSA` and aud = handler_id.
  - **MintError is all-or-nothing by type:** the projection collects into `Result<Vec<_>, _>`.
    - At a join or a parked release, the join fails `McError::Internal` (generic text, no payload).
    - On change, refresh or FAILED, that participant is skipped, logged at error (bounded, no payload) and rescheduled. A partial set is never sent.
  - **Inbound state:** mapped with `ConnectionState::try_from`. UNSPECIFIED and unknown values are dropped at the connection, neither forwarded nor stored. `handler_set_version` stays `u64` end to end.
  - The roster has no participant-id newtype (plain `String`), so the maps keep that.
  - The workspace rand is 0.10: `rand::make_rng()`, per the root Cargo.toml note.
  - The `select!` arms in meeting.rs are one-line delegations.

- Paired-test: the key.rs test also parses the claims segment as `serde_json::Value` and asserts the key set is exactly {sub, token_type, meeting_id, aud, iat, exp}, with iat and exp as integers (wire names pinned independently of the shared struct).
- Operations:
  1. `MC_MEDIA_DIAL_TOKEN_TTL_SECONDS` is **REQUIRED**, the same as its R-37 siblings: absence is a `MissingEnvVar` boot refusal. The prompt's "default 300" is the shipped `config.env` value, the only copy. There is no in-code default, so nothing can drift, and every test/rig Config env gets the key.
  2. Runbook row:
     - forward: the key must exist before or with the image; a hand-applied image without it gives `CreateContainerConfigError`;
     - rollback: image first, then optionally drop the key;
     - a TTL change needs a pod restart, which ends that pod's meetings and their dial keys.
  3. Rollback sentence: "safe to revert alone until MH-1 is deployed; after that, reverting MC-D requires reverting MH-1 first". This also goes in the skew note, if there is one.
  4. Turn cost: `Change` is a HANDLER-membership change (mark_active, and later MC-B2/MC-C removal and add). Participant joins never broadcast a set to other participants, because only the joiner's own JoinResponse is minted. So a join storm costs O(N x H) signs in total, one join's worth per joiner (H signs), not O(N^2). A handler change costs one N x H burst, about 25 ms at N=300, H=4. It is rare (handler failure or transfer) and bounded by `MC_MAX_PARTICIPANTS`. Refresh and FAILED work goes through the per-turn bounded flush, at most `SLOT_VIEW_FLUSH_BATCH` participants x H signs per turn. No token caching, because that would add a second holding of minted tokens in the actor (custody), and no new series (observability: none beyond the contract). The existing actor message-latency and mailbox metrics are the turn-cost signal, and the catalog note cites them.

- Auth-controller (GSA owner, `common/jwt.rs`):
  - **P1, arm b restructured.** `verify_token` maps the aud rejection, like every other decode error, to `InvalidSignature`, so the planned control would not discriminate.
    - (b1) A forged token with a served kid and a foreign key, through MC's validator, gives `McError::JwtValidation` with JWKS hits >= 1.
    - (b2) A direct `jsonwebtoken::decode` against the served JWK with `validate_aud = false` gives `ErrorKind::InvalidSignature`. Positive control: it decodes OK under the forging key.
    - (d, new) A media_dial claim set signed by the LIVE AC key with a served kid is rejected by both `validate_meeting_token` and `validate_guest_token`.
    - A test comment records the aud wall in `verify_token` as a second wall.
  - **P2, `MediaDialClaims` shape.**
    - No `#[serde(default)]` anywhere. `aud: String`. `iat`/`exp: i64` with `impl HasIat`.
    - `validate()` checks `token_type == MEDIA_DIAL_TOKEN_TYPE` and a non-empty sub, tested with the wrong type, "meeting" and an empty sub.
    - A canonical AC meeting-claims JSON fails to deserialize as `MediaDialClaims`.
    - The doc comment states that MH-1 must not verify via `verify_token`/`JwtValidator`: the key is selected by meeting_id, EdDSA is pinned, aud is compared itself (for WRONG_HANDLER) or set explicitly, and the dial key is never in a JWKS.
    - Debug redacts sub, and the type never holds the token.
  - **P3, adopted (complete the invariant).** `MEETING_TOKEN_TYPE` and `GUEST_TOKEN_TYPE` constants beside `MEDIA_DIAL_TOKEN_TYPE`, used at the 5 production sites: `common/jwt.rs` (GuestTokenClaims::validate), `ac-service/src/handlers/internal_tokens.rs` (two issuance sites), `mc-service/src/auth/mod.rs`, and `mh-service/src/auth/mod.rs`.
  - **P4.** Arm a also asserts the token is under the validator's max size.

- Paired-test (REQUIRED TTL):
  - the absent-key refusal names the key;
  - the key has ONE home in the shared test-config builder(s), derived from `config.env` where a builder already reads it. Only dial-timing tests pass their own value;
  - the runbook row says "Required" (shipped value 300).

### Implementation deviations from the plan (recorded at implementation)
- **Config gauges** are published in `webtransport/server.rs` (`WebTransportServer::new`), from the SAME `MediaDialTiming` that is installed into `MediaRoutingDeps`, beside the settle-window gauge (the same discipline). Not in `main.rs`.
- **Log-hygiene test** has its own test binary, `tests/media_dial_log_hygiene.rs`. Tracing callsite interest is process-global, and a concurrent test in the same binary flaked the TRACE capture; the positive control caught it. The shared fixtures moved to `tests/common/media_dial.rs`.
- **Env-test 27, `aud`** (as fixed at Gate 3, paired-test F4). Each `aud` must name exactly one live MH pod: MH's id is `mh-{HOSTNAME}-{uuid8}`, and a format change fails closed. It must also be one value per handler, the same across participants, and distinct handlers must map to distinct pods. The TTL gauge is also cross-checked against the live ConfigMap.
- **Env-test 27, claims decoding (Gate-1 DRY note REVERSED at Gate 3, by dry-reviewer; accepted by auth-controller).** Claims are decoded as an untyped `serde_json::Value` with the EXACT key set asserted, not into `common::jwt::MediaDialClaims`. Two reasons. First, env-tests takes no `common` dependency under the dependency policy recorded in `crates/env-tests/Cargo.toml`: the suite links zero service crates, only wire-contract crates (`proto-gen`, `media-protocol`). Second, it is the stronger oracle: serde ignores unknown fields, so a typed decode would stay green if MC leaked an extra claim, while the exact key set over a `Value` catches it. The `"media_dial"` literal is restated on purpose (an independent black-box oracle), and `common::jwt`'s `token_type_discriminators_are_the_wire_values` pins the constant to the same wire value.
- **Env-test 27, live values.** The TTL comes from the live gauge (`gauge_by_pods`), and `iat` is checked against the wall clock. At Gate 3 (observability F3), `pushes{reason="join"}` was added back as a "rose" presence proof against a settled baseline, never an exact delta, because the counter is shared across the cluster.
- **REQUIRED TTL.** `MC_MEDIA_DIAL_TOKEN_TTL_SECONDS` has no Rust default (operations, Gate 1). The prompt's "default 300" is the shipped `config.env` value. This matches the sibling keys and the runbook row.
- **Mint ordering (Gate 3: security F-SEC-1, operations F2, code-review CQ-1).** The join mints BEFORE any admission state (`mint_handler_set_for(user_id, deps timing)`), so a mint failure refuses the join with nothing to unwind. This is tested by forcing `ExpOverflow` through the anchored clock (`a_join_whose_set_cannot_be_minted_admits_nothing`). At park release the joiner is already admitted, so on a re-mint failure the join is refused AND its admission is undone through the one leave path (`remove_and_broadcast_left(.., Removed)`) (Lead ruling), so the roster stays consistent with the reply. This is tested by forcing a release-time overflow (`a_parked_join_whose_set_cannot_be_minted_at_release_is_undone`).
- **Untouched rows dropped.** `env-tests/src/fixtures/participant.rs` and `actors/controller.rs` were not needed. `tests/media_client_signaling_integration.rs` carries the real-connection `join` count, the byte-identical `media_servers` check and the config-gauge assertions.

### Open items for the Lead (as raised)
1. The claims type in `common/jwt.rs` is a GSA, and auth-controller is not on this loop (B).
2. `sub` = `user_id`, not participant_id (B). This is correct per the binding path.
3. The env-test can verify only structure. Signature verification against the registered key is proven at Tier 1b (J).
4. The new `rate_limited` value on `mc_participant_mh_status_dropped_total` (F) is pending observability.

---

## Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security (paired) | confirmed |
| Test (paired) | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Semantic Guard | confirmed |
| Auth Controller (GSA owner, paired) | confirmed |

Lead rulings: claims SSoT in `crates/common/src/jwt.rs` (GSA; auth-controller + security pulled in); P3 token-type const sweep (ac-service, mh-service sites) accepted as invariant completion. Classification guard: `STATUS=OK` (Owner cell for jwt.rs normalised to the single manifest owner `auth-controller`; security is on the panel as paired reviewer).

---

## Pre-Work

None

---

## Gate 3 — Reviewer Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security (paired) | RESOLVED-FIXED | 1 | 1 | 0 | F-SEC-1 ghost participant on mint failure |
| Test (paired) | RESOLVED-FIXED | 6 | 6 | 0 | limiter/routing/arm-b/env-27 aud+TTL/park/ADR cite |
| Observability | RESOLVED-FIXED | 4 | 4 | 0 | drop panel/catalog, panel unit, env-27 join push, limiter wording |
| Code Quality | RESOLVED-FIXED | 4 | 4 | 0 | ghost participant, 2 comment placements, `#[expect]` |
| DRY | RESOLVED-FIXED | 1 | 1 | 0 | single wire ConnectionState decode; no new TODO extractions |
| Operations | RESOLVED-FIXED | 2 | 2 | 0 | runbook rollout failure modes; ghost participant |
| Semantic Guard | RESOLVED-FIXED (SAFE) | 2 | 2 | 0 | error-context on two refusal paths |
| Auth Controller (GSA owner) | RESOLVED-FIXED | 2 | 1 | 0 | F1 fixed; F2 withdrawn (env-test 27 Value decode reversal recorded) |

Lead additions: fixed time-dependent flake `gc-service::mh_replacement_tests::eligibility_predicate_matches_sql` (fix-don't-defer; global-controller test, Minor-judgment). Ownership lens for `crates/common/src/jwt.rs` GSA: auth-controller + security in Gate 1 and Gate 3 — OK.

---

## Gate 2 — Full Validation

`DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` (attempt 1): L1 OK, L2 OK, L3 OK, L4 OK, L5 OK, L6 OK, L7 OK (env-tests-passed, browser-e2e-passed; ~29.5 min) — `TOTAL_RESULT=OK`.

---

## Accepted Deferrals

- (none surfaced in this devloop)
