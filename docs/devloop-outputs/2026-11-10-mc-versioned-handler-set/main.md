# Devloop Output: MC-A — MC-owned, versioned, ref-addressed handler set; register-before-token

**Date**: 2026-11-10
**Task**: Story 4 task MC-A (run-story task #12): MC-owned, versioned, ref-addressed handler set; register-before-token; legacy divergence counter retired; MC cap hard-fail
**Specialist**: meeting-controller
**Mode**: Agent Teams (v2) — full, Gate-1 present (`--paired-with=test`, `--paired-with=protocol`)
**Branch**: `feature/mh-failure-transfer`
**Duration**: ~2h

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `97aac7c5e4d95e3991cc87afd810a4657eea1f73` |
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
| Security | `security` (RESOLVED-FIXED) |
| Test | `paired-test` (RESOLVED-FIXED) |
| Observability | `observability` (RESOLVED-FIXED) |
| Code Quality | `code-reviewer` (RESOLVED-FIXED) |
| DRY | `dry-reviewer` (RESOLVED-FIXED) |
| Operations | `operations` (RESOLVED-FIXED) |
| Semantic Guard | `semantic-guard` (CLEAR) |
| Protocol | `paired-protocol` (RESOLVED-FIXED) |

---

## Task Overview

### Objective
See task prompt (run-story task #12, story `docs/user-stories/2026-10-31-survive-handler-loss.md`, ADR-0039 §3/§4). Replace MC's frozen handler set with an MC-owned, versioned, ref-addressed set; register-before-token; MediaHandlerSet signalling; retire `mc_media_handler_set_divergence_total`; hard-fail MC caps.

### Scope
- **Service(s)**: mc-service (+ env-tests, docs)
- **Schema**: No
- **Cross-cutting**: No (consumer of P2 signalling contract)

### Debate Decision
NOT NEEDED - governed by ADR-0039.

---

## Cross-Boundary Classification

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `.env.example` (MC section: the two required caps) | Not mine, Minor-judgment | operations (standing panel) |
| `crates/env-tests/src/fixtures/mh_address.rs` (`live_mh_addresses`) | Not mine, Minor-judgment (paired with test) | test |
| `crates/env-tests/src/fixtures/participant.rs` (`Participant::handler_set`) | Not mine, Minor-judgment (paired with test) | test |
| `crates/env-tests/tests/02_mc_deployment_config.rs` (both REQUIRED caps in `WIRED_KEYS`) | Not mine, Minor-judgment (paired test) | test |
| `crates/env-tests/tests/27_mc_slot_placement.rs` | Not mine, Minor-judgment (paired with test) | test |
| `crates/mc-service/src/actors/controller.rs` | in-domain | — |
| `crates/mc-service/src/actors/join_park.rs` | in-domain | — |
| `crates/mc-service/src/actors/meeting.rs` | in-domain | — |
| `crates/mc-service/src/actors/meeting_media.rs` | in-domain | — |
| `crates/mc-service/src/actors/messages.rs` | in-domain | — |
| `crates/mc-service/src/actors/mod.rs` | in-domain | — |
| `crates/mc-service/src/actors/participant.rs` | in-domain | — |
| `crates/mc-service/src/config.rs` | in-domain | — |
| `crates/mc-service/src/errors.rs` | in-domain | — |
| `crates/mc-service/src/grpc/mc_service.rs` | in-domain | — |
| `crates/mc-service/src/grpc/media_coordination.rs` | in-domain | — |
| `crates/mc-service/src/grpc/mh_client.rs` | in-domain | — |
| `crates/mc-service/src/main.rs` | in-domain | — |
| `crates/mc-service/src/media_admission/epoch.rs` | in-domain | — |
| `crates/mc-service/src/media_routing/confirm.rs` | in-domain | — |
| `crates/mc-service/src/media_routing/connectivity.rs` | in-domain | — |
| `crates/mc-service/src/media_routing/generation.rs` | in-domain | — |
| `crates/mc-service/src/media_routing/mod.rs` | in-domain | — |
| `crates/mc-service/src/media_routing/placement.rs` | in-domain | — |
| `crates/mc-service/src/media_routing/pusher.rs` | in-domain | — |
| `crates/mc-service/src/media_routing/slots.rs` | in-domain | — |
| `crates/mc-service/src/media_routing/teardown.rs` | in-domain | — |
| `crates/mc-service/src/media_signaling/assignments.rs` | in-domain | — |
| `crates/mc-service/src/media_signaling/directive.rs` | in-domain | — |
| `crates/mc-service/src/media_signaling/outcome.rs` | in-domain | — |
| `crates/mc-service/src/observability/metrics.rs` | in-domain | — |
| `crates/mc-service/src/redis/client.rs` | in-domain | — |
| `crates/mc-service/src/redis/mod.rs` | in-domain | — |
| `crates/mc-service/src/webtransport/connection.rs` | in-domain | — |
| `crates/mc-service/src/webtransport/handler.rs` | in-domain | — |
| `crates/mc-service/src/webtransport/server.rs` | in-domain | — |
| `crates/mc-service/tests/common/accept_loop_rig.rs` | in-domain | — |
| `crates/mc-service/tests/common/media_session.rs` | in-domain | — |
| `crates/mc-service/tests/common/mod.rs` | in-domain | — |
| `crates/mc-service/tests/disconnect_latency_integration.rs` | in-domain | — |
| `crates/mc-service/tests/handler_set_integration.rs` | in-domain | — |
| `crates/mc-service/tests/join_tests.rs` | in-domain | — |
| `crates/mc-service/tests/kek_rotation_integration.rs` | in-domain | — |
| `crates/mc-service/tests/media_admission_integration.rs` | in-domain | — |
| `crates/mc-service/tests/media_client_signaling_integration.rs` | in-domain | — |
| `crates/mc-service/tests/media_connection_update_integration.rs` | in-domain | — |
| `crates/mc-service/tests/media_coordination_integration.rs` | in-domain | — |
| `crates/mc-service/tests/meeting_teardown_integration.rs` | in-domain | — |
| `crates/mc-service/tests/otel_webtransport_integration.rs` | in-domain | — |
| `crates/mc-service/tests/slot_placement_integration.rs` | in-domain | — |
| `crates/mc-service/tests/video_slots_integration.rs` | in-domain | — |
| `crates/mc-service/tests/webtransport_accept_loop_integration.rs` | in-domain | — |
| `crates/mc-test-utils/src/media.rs` | in-domain | — |
| `docs/DEVELOPMENT.md` (MC local-run block: the two required caps) | Not mine, Minor-judgment | operations (standing panel) |
| `docs/TODO.md` | in-domain | — |
| `docs/observability/metrics/mc-service.md` | in-domain | — |
| `docs/runbooks/gc-incident-response.md` ("frozen set" wording, one sentence) | Not mine, Minor-judgment | global-controller / operations (standing panel) |
| `docs/runbooks/mc-deployment.md` (§Configuration Reference rows for the six keys; cap-lever CrashLoop note) | Not mine, Minor-judgment | operations (standing panel) |
| `docs/runbooks/mc-incident-response.md` (Scenario 18 arm f replaced, frozen-set wording) | Not mine, Minor-judgment | operations (standing panel) |
| `docs/specialist-knowledge/dry-reviewer/INDEX.md` (edited by dry-reviewer: frozen-set pointer) | Not mine, Mechanical | dry-reviewer |
| `docs/specialist-knowledge/meeting-controller/INDEX.md` | in-domain | — |
| `docs/specialist-knowledge/security/INDEX.md` (stale pointers; line 52 re-pointed by security) | Not mine, Mechanical | security |
| `infra/grafana/dashboards/mc-media.json` | in-domain | — |
| `infra/services/mc-service/config.env` (add `MC_MAX_MEETINGS=1000`, `MC_MAX_PARTICIPANTS=10000`) | Not mine, Minor-judgment | operations / infrastructure (standing panel) |
| `infra/services/mc-service/mc-0-deployment.yaml` (two `configMapKeyRef` env entries) | Not mine, Minor-judgment | operations / infrastructure (standing panel) |
| `infra/services/mc-service/mc-1-deployment.yaml` (lockstep with mc-0) | Not mine, Minor-judgment | operations / infrastructure (standing panel) |
| `proto/dark_tower/signaling/v1/signaling.proto` (COMMENT-ONLY: `StreamAssignment.handler_ref` 0 = no edge; Gate-3 P-G3-1; TS bindings regenerated, not tracked in git) | Not mine, Domain-judgment (GSA; security co-reviews) | protocol |
| `scripts/generate-dev-certs.sh` (local-run `export MC_MAX_*` hints) | Not mine, Minor-judgment | infrastructure / operations (standing panel) |

The ONE GSA path is the comment-only `signaling.proto` edit above (Gate-3 P-G3-1, approved by the Lead with protocol + security reviewing). The SDK file `packages/sdk-core/src/signaling/SignalingClient.ts` is READ by a drift test (include_str!), not edited. Rows regenerated from the diff, one path per row.

---

## Planning

### Mechanism restatement

Instance: "replace the frozen set, park the first join". Mechanism: **every client-visible handler fact (url, ref, set membership) is a projection of ONE MC-owned versioned set, filtered to Active; the only input to that set is GC's hand-off at actor creation (later: MC-C adds/removes); and every reply/message that names a handler is emitted only after that handler's RegisterMeeting ack.** Same-owner siblings of the wider class found and pulled in: (1) legacy `JoinResponse.media_servers` + `SendTarget/StreamAssignment.media_handler_url` (same projection, Active-only); (2) observed connectivity `routable()` (edges only over Active members, so no directive can name a non-member); (3) config: the masking `.parse().ok().unwrap_or()` sits on SIX MC keys, not two (all six converted — see C).

### A. Handler set (`media_routing/placement.rs`)

- `MeetingHandlers { members: Vec<Member{endpoint, handler_ref: NonZeroU32, state: HandlerState}> (sorted by id), removed: BTreeSet<HandlerId>, version: u64, next_ref: NonZeroU32 }`. All fields private, no setters.
- `HandlerState { Registering, Active }` (private enum; read via `is_active`).
- **Emission rule (protocol P2, doc'd on the type for MC-B2/MC-C)**: a client-facing set is emitted iff the ACTIVE projection changed. `mark_active` and `remove` return a `SetChange { active_changed: bool }`, so removing a still-Registering handler bumps the internal version but emits nothing. All three mutators bump through ONE private `bump() -> Result<u64, HandlerSetError>` (`VersionExhausted`), as does `next_ref`; mutators return `Result`, no expect/saturate.
- **Version rule (doc'd on the type)**: `handler_set_version` is the epoch of MC's INTERNAL set; it advances by exactly 1 on every successful `add`, `mark_active` and `remove`, and on nothing else (never on a token refresh, a join, a push, a slot/mute change). Starts at 0; never reset; u64 (no wrap concern; `checked_add` -> loud error anyway). Clients see gaps; a client-facing set is EMITTED only on `mark_active` and `remove` (the two changes to the Active projection), so no emitted set repeats the previous Active list under a new version. MC-B1 reads the internal set (Registering included) for OWNED verdicts.
- **Refs (S-1)**: per-meeting `next_ref: NonZeroU32` starting at 1, advanced with `checked_add`; exhaustion -> `HandlerSetError::RefSpaceExhausted`, refusing the add, never wrapping. Never derived from id/url/hash; a new incarnation at a reused address gets a new ref. Same refuse-don't-wrap discipline as `sender_id.rs` / `generation.rs` but deliberately not a shared allocator; a row for `handler_set_version` + `handler_ref` added to generation.rs's sibling-counter table ("never sourced from policy_generation or the Redis fencing counter").
- `add(endpoint) -> Result<NonZeroU32, HandlerSetError>`: rejects duplicate id (`DuplicateHandlerId`), removed id (`PreviouslyRemoved`, S-2), duplicate client url under another id (`DuplicateUrl`, protocol #5); no bump on rejection. Enters as Registering.
- `mark_active(id, handler_ref) -> bool` keyed by id AND ref (S-3): unknown id, stale ref (removed incarnation), or already-Active -> no-op, no bump.
- `remove(id) -> bool`: one-way into `removed`; unknown -> no-op, no bump. No caller yet (MC-B2/MC-C) beyond tests.
- `from_handoff(endpoints)`: the ONE construction from GC's assignment: adds each (sorted, so Redis/GC enumeration order is irrelevant); keeps `HandlerSetError::Empty` ONLY at this boundary (a GC hand-off naming no handler stays a loud INVALID_REQUEST, as today). `MeetingHandlers::empty()` exists for MC-C; `is_empty()` doc fixed.
- Internal view (Registering + Active): `resolve` (S1a exact match, unchanged), `get`, `iter`, `ids` — used by push workers, teardown, binding gate, MH notifications.
- **Client view (Active only) — the ONE lookup** (protocol #1b, code-review #3, DRY #3): `client_handler(id) -> Option<ClientHandler<'_>{ url: &str, handler_ref: NonZeroU32 }>` returns None for a non-Active or unknown id; `url_of` is replaced by it (no separate ref map). `client_snapshot() -> ClientHandlerSet { version, handlers: Vec<{url, handler_ref}> }` iterates the same members through the same filter. The snapshot carries the version it was taken at. No grpc_endpoint, no handler_id, no Registering entry on the wire (S-4).
- Module doc rewritten (no "frozen, non-empty"; story-4 hazard text updated: membership changes are MC-B2/MC-C's, the set structure is here).

### B. Read-once hand-off (§3) — the set is installed at actor CREATION

- Who creates the actor: only `McAssignmentService::assign_meeting_with_mh` (GC's AssignMeetingWithMh) -> `controller.create_meeting`. That RPC handler already holds the assignment it writes to Redis. Plan: after `store_mh_assignments` succeeds, build `MeetingHandlers::from_handoff` from the SAME `Vec<MhEndpointInfo>` it just stored (one conversion fn, moved out of `connection.rs:read_meeting_handlers`) and pass it to `create_meeting(meeting_id, handlers)`; the controller hands it to `MeetingActor::spawn*` -> `MeetingMedia::new(meeting_id, handlers)`. The gRPC handler is not an actor, and the controller turn does no I/O, so no actor awaits Redis.
- The join path stops reading Redis entirely: `read_meeting_handlers` deleted, `JoinMedia.handlers` deleted, `install()` and its divergence branch deleted (deps still arrive with the first join as today). `MhAssignmentStore` (whose only production reader was the join) is deleted with the `WebTransportServer`/`handle_connection` `redis_client` parameter and `main.rs` wiring; `FencedRedisClient::get_mh_assignment` deleted unless a test needs it. The Redis WRITE stays (GC's hand-off record; not this task's to retire).
- OPS-2 (read failure): there is no separate read to fail. An invalid/empty hand-off is rejected by the RPC before the actor exists (INVALID_REQUEST, as today); a Redis store failure rejects the assignment (UNHEALTHY, as today). An actor therefore never exists without its hand-off set; nothing silently installs an empty set.
- S-5 / Q4: no join path can call get_mh_assignment (the trait is gone); concurrent first joins cannot race a read. A GC re-assign of a live meeting rewrites Redis but `create_meeting` returns Conflict, so the live actor's set is untouched (that is the "later Redis change" scenario).
- Test-churn consequence: `create_meeting(id)` gains a `handlers` argument (47 call sites, mostly via `tests/common` seed helpers; mechanical).

### C. Config (R-37) — `config.rs`

- All six lenient keys go through the existing `bounded<T>(name, raw, min, max)` (no third hand-rolled parse): `vars.get(K).map(|v| bounded(K, v, MIN, MAX)).transpose()?.unwrap_or(DEFAULT)`. Error names the key, echoes the value and carries the parse error (S-8, semantic-guard #3).
  - `MC_MAX_MEETINGS`, `MC_MAX_PARTICIPANTS`: `1..=u32::MAX` — **0 rejected** (a 0 cap is a silent total outage that reads as valid; same reasoning as the audio-slot floor).
  - `MC_CLOCK_SKEW_SECONDS`: `0..=common::jwt::MAX_CLOCK_SKEW.as_secs()` (security: existing policy; makes main.rs's `as i64` justifications true; AC precedent). Table rows for MAX and MAX+1.
  - Security notes BINDING_TOKEN_TTL / NONCE_GRACE / DISCONNECT_GRACE are INERT today (parsed + logged, never consumed: session.rs and meeting.rs hardcode their values; nonce grace has no consumer). Their runbook rows will say "parsed and validated at boot; not yet consumed (hardcoded in session.rs / meeting.rs)", and a docs/TODO.md entry names the constants. Wiring them is a behaviour change to ADR-0023 session binding with its own tests, outside this task's seam, so it is task-sized.
  - `MC_BINDING_TOKEN_TTL_SECONDS`, `MC_NONCE_GRACE_WINDOW_SECONDS`, `MC_DISCONNECT_GRACE_PERIOD_SECONDS`: parse-only (`0..=u64::MAX`), i.e. no NEW range policy — unparseable now hard-fails; whether 0 is valid for these ADR-0023 knobs is a security/ADR-0023 policy question I am not inventing here (raise it if you want a floor).
- **Lead ruling (story R-37 "missing or invalid")**: `MC_MAX_MEETINGS` and `MC_MAX_PARTICIPANTS` become REQUIRED: missing -> startup error naming the key; unparseable or 0 -> error. Both keys are added with today's defaults (1000 / 10000) to `infra/services/mc-service/config.env` and as `configMapKeyRef` entries in `mc-0-deployment.yaml` and `mc-1-deployment.yaml` (no envFrom; per-key entries, lockstep). Every test/rig that builds MC `Config` from env vars gets both keys. Hard refs (no `optional: true`); runbook rows "**Yes** (hard ref)" plus the rollout/rollback sentence from operations (same kustomize apply; `rollout undo` returns to the retained ConfigMap generation; pre-MC-A image ignores the keys; a bare `kubectl set image` without the manifest CrashLoops the new pod while RollingUpdate keeps the old one serving). Layer 3 dt-guard env-config parity stays green; mc-0/mc-1 lockstep. The four `_SECONDS` keys: missing keeps its default; unparseable hard-fails.
- Stale QUIC comment ("lenient pattern used by the other _SECONDS vars") rewritten. Table test over all six keys: "abc", "-1", "1.5", "", overflow -> error naming the key; valid value -> parsed value asserted; absent -> default; caps "0" -> error.
- mc-deployment.md rows: caps "Required; missing or invalid (unparseable or 0) refuses boot"; the four seconds keys "unparseable refuses boot (before MC-A it silently fell back to the default)", plus one sentence where the cap load-shed lever is described: a malformed value now CrashLoops the pod.

### D. Register before token + parked joins

- **Pusher -> actor report** (`pusher.rs`): `PushTarget` gains a `RegistrationReporter { tx: mpsc::UnboundedSender<HandlerRegistration>, handler_id, handler_ref }`. `HandlerRegistration { handler_id, handler_ref, outcome: Registered | InitialRegistrationFailed }` — ids only, no MeetingProgramming, no key material. The worker sends `Registered` on its FIRST `Settled::Confirmed`, and `InitialRegistrationFailed` once if its first job settles `Failed` (retries exhausted OR terminal) before any confirm. The reporter's `Drop` sends `InitialRegistrationFailed` if no verdict was ever sent (worker panic/abort/cancel), so a park always gets a terminal event. Non-blocking `UnboundedSender::send` (no await, usable in Drop); bounded at ≤2 messages per incarnation. A later confirm after an initial failure still sends `Registered` (the handler then becomes Active through the ordinary membership-change path).
- **Actor side**: `MeetingMedia` owns the receiver; the meeting actor's `select!` gets one arm for it (never an await on MH inside a turn). On `Registered`: `mark_active(id, ref)`; if it changed the set: (1) enqueue a `MediaHandlerSet` to every connected participant (except parked joiners, whose JoinResponse will carry it), (2) recompute routing (routable = connected ∩ Active) + reconcile(All) + flush — so the set always precedes any directive/assignment naming the new ref on the same ordered per-participant queue (protocol #1a). One INFO line per membership change with handler_id/ref/version as LOG fields only.
- **Parking** (`actors/join_park.rs`, keeps meeting.rs from growing): while the meeting's INITIAL verdict is pending (some hand-off handler has neither acked nor failed), a successful `handle_join`'s reply is not sent: `(respond_to, JoinResult)` is stored in a `Vec<ParkedJoin>` and the turn returns. Admission (roster, sender id, participant actor, broadcasts, slot admit) happens in that turn exactly as today; only the reply waits.
  - **Deliberate choice — every join arriving before the initial verdict parks, not only the very first.** Reason: a second joiner answered at once would get an EMPTY `media_servers`; today's SDK (pre-C1) never reads `MediaHandlerSet` and would have no media for the session — a real regression for parallel joins (multi-party e2e specs). Joins after the verdict reply at once (the task's "later joins reply at once" = after the initial verdict). This also matches code-review #4a / OPS-1a. The parked list is bounded by the participant cap.
  - **Release** = all initial hand-off handlers have a verdict (ack or failure) OR the park deadline fires, whichever first. Waits for ALL initial handlers (the prompt's "initial handlers' RegisterMeeting is acked"). Each parked reply is completed with `JoinResult.handler_set = client_snapshot()` taken at release (Active only; possibly empty).
  - **Park deadline (OPS-1)**: `FIRST_JOIN_PARK_DEADLINE = 8 s`, a `select!` arm on `sleep_until` (no `timeout()` wrapper). The pusher's worst case (3 × 10 s RPC + 3 s backoff) exceeds the SDK's `DEFAULT_JOIN_TIMEOUT_MS = 15_000`, so the deadline is what makes "the join never fails because the initial MH is dead" true. SSoT: a unit test `include_str!`s `packages/sdk-core/src/signaling/SignalingClient.ts`, parses `DEFAULT_JOIN_TIMEOUT_MS`, and fails if `deadline + 5 s margin >= it`. After the deadline the pusher keeps going; a later ack is an ordinary membership change (set ServerMessage).
  - Empty-set release: one WARN on `mc.actor.meeting` with meeting_id and the unacked handler_id(s) as LOG fields (obs c).
  - Teardown while parked: the actor's exit path answers each parked reply `Err(McError::Draining)` explicitly (counted as a join failure at the connection, which still owns the join outcome/latency metric — so a parked join's outcome and latency are recorded on completion, never at park time). A parked participant that disconnects is not observable until its reply is sent (the connection task is the one awaiting it); the reply send result is checked and a closed receiver is logged at debug with reason, never `let _ =` silently. No sender leaks: all live in `Vec<ParkedJoin>` drained on release/exit.
- `JoinResult.media_handlers: MeetingHandlers` -> `handler_set: ClientHandlerSet`.
- **S-9 (stale parked JoinResult)**: (a) at release the actor re-reads `meeting_kek` + `kek_generation` (and the handler snapshot) from its CURRENT state; only participant-specific fields (participant_id, sender_id, correlation_id, binding_token) come from the parked value. (b) Stated invariant: every message enqueued for a parked participant (KEK update, roster delta, directive, assignments, set) sits on its participant-actor outbound queue, which the connection forwards only AFTER writing the JoinResponse (connection.rs Step 8 before the Step 9 bridge loop), so a queued rotation is redundant-or-newer, never older. The roster stays the admission-time snapshot ON PURPOSE: the joins/leaves since admission are queued as ordered deltas behind the JoinResponse, so snapshot + deltas is exact; refreshing it would double-apply those deltas. Binding token is minted once at admission; a join answered Draining simply never delivers it (never logged). Test T8 (Tier 1b): park A, remove a member (leave-debounced rotation forced via the existing `kek_debounce_manual` seam + `ForceKekRotation`) while parked, release; A's JoinResponse carries the current generation, its sender_id equals the id MC holds for A after the rotation, and nothing reached A's wire before its JoinResponse.

### E. Client signalling (§4)

- `webtransport/handler.rs`: ONE projection `media_handler_set(&ClientHandlerSet) -> MediaHandlerSet` (`media_dial_token` empty — MC-D). `build_join_response` sets `media_handler_set: Some(..)` ALWAYS (empty included, protocol #7) and `media_servers = set.handlers.clone()` — the legacy field is the SAME Active projection (protocol #2, OPS-5a, Q2): old SDKs never see a Registering handler either.
- Membership-change set: new typed `ParticipantMessage::HandlerSet { set: ClientHandlerSet }`, encoded into a `ServerMessage` in `participant.rs` (not a raw bytes payload), full snapshot to every connected participant (protocol #8, semantic-guard #2).
- `directive.rs` / `assignments.rs`: target url AND ref from the one `client_handler(id)` lookup. Because `routable()` admits only Active members, an edge on a non-Active handler cannot be rendered; if one were, the existing fail-closed `HandlerUrlUnresolved` arm fires (never a ref-0 or empty-url target, S-4).
- No client-originated ref is parsed in this task (MhConnectionStatus ref handling is MC-D/C1), S-7 noted.

### F. Retirement of `mc_media_handler_set_divergence_total`

metrics.rs recorder + zero-init; meeting_media.rs install branch + module doc (:77) + `warn_unflushable` doc (:943, reworded without the counter); slot_placement_integration.rs:1036 (assertion replaced by the behavioural proof in T5); mc-media.json panel + gridPos closed; catalog entry + roster row (:394); mc-incident-response.md Sc 18 arm (f), "(a) to (f)" -> "(a) to (e)", "and for (f)" escalation text; replaced by an arm for "join succeeded, MediaHandlerSet empty, no media" (initial MH never acked; keys: the new WARN, "RegisterMeeting retries exhausted"/"failed terminally", push failure series). "frozen set" wording in MC runbooks updated. Acceptance grep per observability. metric_hygiene.rs untouched unless the grep finds it. Catalog: `mc_session_join_duration_seconds` gets a "first-join parking" population note (bounded by the 8 s deadline; excursions coincide with the pusher's failure lines), no re-bucketing.

### G. Tests (paired with @paired-test)

Tier 1a (placement.rs unit): refs non-zero/distinct; remove then add new id at same url -> new, larger ref; re-add removed id rejected, no bump; duplicate id / duplicate url rejected, no bump; remove unknown no-op; mark_active unknown / stale-ref / already-Active no-op; each successful change bumps by exactly 1; client view excludes Registering; ref exhaustion refuses (seeded next_ref via `#[cfg(test)]` constructor). Config table test (C). Park-deadline drift test (D). Pusher unit: Registered on first confirm only; InitialRegistrationFailed on exhaustion and on terminal; reporter Drop sends failure.
Tier 1b (new `tests/handler_set_integration.rs`, actor-level, `current_thread` + `start_paused`, per-handler-gated `MockMhRegistrationClient` — extended in tests/common, no second fake):
- T1 first_join_parks_until_ack_without_stalling_the_meeting: hold acks; A joins -> pending; B joins -> also pending (deliberate choice above); a get_state round-trip and an MH notification are processed while parked (mailbox live); release -> A and B replies carry H, non-zero ref, url == seeded url, version > 0; later C joins -> replies at once with the same version.
- T2 no_set_names_a_handler_before_its_ack: initial H1,H2; ack H1, hold H2 -> A stays parked (release waits for ALL initial verdicts); advance to the park deadline -> A released with exactly {H1}: H2 absent from the set, media_servers, every SendTarget and every StreamAssignment; then ack H2 -> exactly one MediaHandlerSet {H1,H2} with a bumped version, ordered before any directive/assignment naming H2's ref.
- T3 initial_handler_never_acks_join_succeeds_with_empty_set: errors until retries exhausted (paused time) -> join Ok, empty set, empty media_servers; deadline arm variant (hanging MH) -> released at 8 s; positive arm: a second meeting with an acking MH lists H.
- T4 integration arm: joins 2..n and slot/mute changes leave the version unchanged; mark_active advances it.
- T5 a_later_redis_change_does_not_alter_a_live_meeting (stack-level): join A; GC re-assign (store rewrite + Conflict) / mock store mutation; join B -> same set/version/refs; positive control: a NEW meeting assigned with the new record lists the new handlers; the join path performs zero store reads (trait gone; asserted structurally).
- T6 extend a_client_supplied_handler_url_never_reaches_a_send_target: needle scan over JoinResponse.media_handler_set and the set ServerMessage; every target/assignment handler_ref is a member of the receiver's latest set; a forged ref/url on MhConnectionStatus appears nowhere.
- T7 parked join answered on meeting teardown (Draining), no leak.
- media_dial_token asserted empty (protocol #6) in T1.
- Release-path proof under paused time (paired-test 1/2): every "pending" check is a poll (`now_or_never` after `yield_now`), never an await that can auto-advance; T1 and T3-error assert elapsed < `FIRST_JOIN_PARK_DEADLINE` (T3 pins the pusher backoff); T3-hang asserts release exactly at the deadline and pending one tick before; T2 advances to the deadline explicitly.
- Wire order (protocol P1, paired-test 4): the connection writes the JoinResponse before forwarding the participant queue (connection.rs Step 8 then the bridge loop), so in T1/T2 the first ServerMessage naming a handler_ref follows the JoinResponse and that ref is a member of the JoinResponse set.
- T5 asserts the re-assign RPC's actual rejection as the premise. Pusher reporter: Drop-on-cancel sends one failure; Registered-then-dropped sends nothing; confirm after initial failure sends Registered. Env-test 27 asserts set completeness (both Kind handlers) before the later-joiner stability check.
- Drift test: distinct "constant not found / unparseable" failure, a self-check of the literal value, margin from a named const; doc on `FIRST_JOIN_PARK_DEADLINE` saying why it is a const (coupled to the SDK constant, guarded by the drift test).
- Parked-release WARN carries `release_reason` (`verdicts`|`deadline`) and `parked_joins` as log fields; teardown-while-parked uses an existing `error_type` (`McError::Draining`'s label, verified at implementation; if it were new it would get catalog + zero-init + test).
- Test seed: all `create_meeting(id, handlers)` call sites go through one `tests/common` helper.
Existing tests that hold pushes before a join are adjusted (they now park); a mock MH that fails pushes now costs the retry schedule per first join — such tests move to paused time or ack first.
Env-test (paired): `fixtures/mc_session.rs` helper returning the JoinResponse's set; 27's canonical test asserts non-empty set (TRIAGE token on empty), non-zero distinct refs, version > 0, set urls == legacy media_servers urls, every url belongs to a live MH discovered by label, a later joiner sees the same version/refs. Residual recorded in the doc comment: "names only registered handlers" is proven at Tier 1b (T1/T2); the Layer-7 ordering proof lands with MH-1/T-1's bus check.

### H. Observability/hygiene

No new series; one retirement. handler_ref / handler_set_version / handler state / handler_id never a metric label or span attribute; log fields only, one INFO per membership change and one WARN per empty-set release. env-test 32 unchanged.

### I. Rollback

Single MC-only revert unit: no new config keys, P2 wire fields additive, legacy fields still populated (Active-only). Behaviour change visible on rollback: none beyond join latency of the parked first join.


---

### Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed (S-1..S-9 resolved) |
| Test (paired) | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed (re-confirmed after R-37 required-caps delta) |
| Semantic Guard | confirmed |
| Protocol (paired) | confirmed |

Lead rulings: R-37 caps required (missing or invalid refuses boot) per story line 78. Classification-sanity guard: `STATUS=OK`. Note: teammate prompts referenced INDEX.md / review-protocol.md by path (read as first action) rather than inlining them.

---

## Pre-Work

None

---

## Implementation Summary

Implemented per §Planning (all deltas from Gate 1 included). Highlights:

- **Handler set** (`media_routing/placement.rs`): `MeetingHandlers` is now MC-owned, versioned (`handler_set_version`, +1 on add / mark_active / remove through one checked `next_version`), ref-addressed (`NonZeroU32` per incarnation, `checked_add`, `RefSpaceExhausted`), with Registering/Active state, a one-way removed record, `DuplicateUrl` / `PreviouslyRemoved` refusals, `SetChange { version, active_changed }` (emission rule: emit iff the Active projection changed), and the Active-only client view `client_handler` / `client_snapshot` (`ClientHandlerSet`) as the ONE source of every client-facing url and ref. `HandlerSetError::Empty` only at `from_handoff`.
- **Read-once hand-off**: the set is built in `AssignMeetingWithMh` from the same `MhEndpointInfo` list it stores (`grpc/mc_service.rs::handler_set_from_handoff`, the one mapping) and passed to `create_meeting(id, handlers)` → actor creation. The join path no longer reads Redis: `read_meeting_handlers`, `JoinMedia.handlers`, `install()`'s divergence branch, the `MhAssignmentStore` trait, `FencedRedisClient::get_mh_assignment`, the WebTransport server's `redis_client` parameter and the now-unreachable `McError::MhAssignmentMissing` (never join-reachable/zero-initialised) are deleted.
- **Register before token**: `pusher::RegistrationReporter` reports `Registered` (first confirm) / `InitialRegistrationFailed` (exhausted, terminal, or worker end via `Drop`) on an unbounded channel the meeting actor drains in its own `select!` arm. `actors/join_park.rs` parks every join admitted before the initial verdict; release on all initial verdicts or `FIRST_JOIN_PARK_DEADLINE` (8 s, `select!` arm armed only while parked; SDK-deadline drift test with distinct extraction/margin failures). Release re-reads KEK + generation + snapshot (S-9); exit answers parked joins `Draining`. On `Registered`: set broadcast (non-parked) → release → reroute (routable = connected ∩ Active) → reconcile + flush.
- **Signalling**: `webtransport/handler.rs::media_handler_set` is the one projection for `JoinResponse.media_handler_set` (always `Some`) and the legacy `media_servers`; typed `ParticipantMessage::HandlerSet` encoded in `participant.rs` (new `payload_kind="media_handler_set"`, zero-initialised via `OUTBOUND_PAYLOAD_KINDS`, catalogued); `SendTarget` / `StreamAssignment` `handler_ref` from `client_handler`. `media_dial_token` empty (MC-D).
- **Retirement**: `mc_media_handler_set_divergence_total` recorder, zero-init, catalog entry + roster row, mc-media panel (neighbour widened to close the gap), runbook Sc 18 arm (f) (replaced by the "join ok, set empty" arm), old test assertion. Acceptance grep clean.
- **Config (R-37 + Lead ruling)**: caps REQUIRED (`MissingEnvVar` literal kept on one line for `dt-guard env-config`), `bounded(.., MIN_CAPACITY_CAP=1, u32::MAX)`; the four ADR-0023 seconds keys parse-only fail-loud via `optional_seconds`, `MC_CLOCK_SKEW_SECONDS` bounded by `common::jwt::MAX_CLOCK_SKEW`. Keys added to `config.env`, both deployments (hard refs), env-test 02, DEVELOPMENT.md, `.env.example`, `generate-dev-certs.sh`; runbook rows (inert keys documented truthfully) + `docs/TODO.md` entry for the three inert keys.
- **Tests**: placement/pusher/join_park/config/connection unit tests; new `tests/handler_set_integration.rs` (T1 park + live mailbox, T2 Active-only + wire order + deadline exactness, T3 fail/hang/positive arms, T4 version, T5 re-assign refused + positive control, T7 Draining, T8 S-9 rotation); T6 extension in `media_client_signaling_integration.rs`; env-test 27 `assert_handler_sets_are_ref_addressed_and_registered` (+ `live_mh_addresses`, `Participant::handler_set` fixtures). Per-handler hold/fail added to the existing `MockMhRegistrationClient`; one seed helper (`test_common::handler_set` via the production mapping).
- `./scripts/layer-fast.sh`: green (L1–L6).

---

## Files Modified

See §Cross-Boundary Classification: it is regenerated from the diff, one path per row (65 paths + this file). New files: `crates/mc-service/src/actors/join_park.rs`, `crates/mc-service/tests/handler_set_integration.rs`.

---

## Code Review Results

### Gate 3 — Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 1 | 1 | 0 | SEC-1: parked joiner removed during park got post-rotation KEK → roster check at release answers Err(Draining); T8 asserts. GSA ownership lens on proto comment hunk: OK |
| Test (paired) | RESOLVED-FIXED | 3 | 3 | 0 | F1 vacuous T1 ordering; F2 non-distinguishing T2 arm, exact version +1; F3 = SEC-1 |
| Observability | RESOLVED-FIXED | 1 | 1 | 0 | ADR-0032 emission test for new `payload_kind="media_handler_set"` drop value |
| Code Quality | RESOLVED-FIXED | 10 | 10 | 0 | misplaced docs, no-op version exhaustion, silent skip, lookup dedupe, dead API, naming, stale INDEX/TODO pointers |
| DRY | RESOLVED-FIXED | 5 | 5 | 0 | drain_frames hoisted; one test endpoint helper; env-test label() reuse; stale runbook/TODO pointers. Extraction opportunities: none |
| Operations | RESOLVED-FIXED | 2 | 2 | 0 | Sc 18 arm (f) recovery = reload until C1; frozen-set wording swept |
| Semantic Guard | CLEAR | 0 | 0 | 0 | Native SAFE on Credential Leak, Client Credential Lifetime, Actor Blocking, Error Context, Metrics Path. Forward note for MC-D: redact media_dial_token in ClientHandlerSet Debug |
| Protocol (paired) | RESOLVED-FIXED | 1 | 1 | 0 | P-G3-1: comment-only proto edit — handler_ref 0 = "no edge" on 3 edgeless StreamAssignment states; test helper pins 0 ⇔ edgeless |

GSA note: `proto/dark_tower/signaling/v1/signaling.proto` received a comment-only edit at Gate 3 (no wire change). Owner (protocol, paired) and security were both present in planning and review and confirmed the hunk.

---

## Accepted Deferrals

- (none surfaced in this devloop)

Out-of-scope pre-existing gap recorded (not a finding): `docs/TODO.md` — inert ADR-0023 MC seconds keys (parsed, hardcoded at use site).

---

## Rollback Procedure

1. Start commit: `97aac7c5e4d95e3991cc87afd810a4657eea1f73`
2. `git diff 97aac7c5..HEAD`
3. Safe-revert unit: **the whole commit**. Code + manifests (required cap keys) + docs move together; reverting only the manifests would CrashLoop the new image (caps required), reverting only code is safe (old image ignores the keys). The proto change is comment-only and independently revertible.

---

## Gate 2 — Full Validation

`DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh`: L1 OK, L2 OK, L3 OK, L4 OK, L5 OK, L6 OK, L7 OK (1758s, env-tests + browser E2E) — `TOTAL_RESULT=OK`. One attempt.

---

## Issues Encountered & Resolutions

None yet
