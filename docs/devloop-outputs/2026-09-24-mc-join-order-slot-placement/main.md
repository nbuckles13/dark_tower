# Devloop Output: MC multi-party static join-order slot placement (story 2 task 6)

**Date**: 2026-09-24
**Task**: Per-meeting join-order slot state in the MC meeting actor, per-handler placement, non-reflexive subscribes_to, structural re-push, receive-capability → actor (R-1..R-4, R-33)
**Specialist**: meeting-controller
**Mode**: Agent Teams (v2) — full, Gate-1 present (paired: test, client, media-handler)
**Branch**: `feature/hear-each-other`
**Duration**: multi-session (interrupted once; resumed headless 2026-09-24)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `58f323aa58af4d1b5b9ec647a39da7baed2bd65f` |
| Branch | `feature/hear-each-other` |
| Lead Model | `claude-opus-5-5` |

---

## Loop State (Internal)

<!-- This section is maintained by the Lead for state recovery after interruption. -->
<!-- Do not edit manually - the Lead updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` |
| Implementing Specialist | `meeting-controller` |
| Tier | `full` |
| Iteration | `1` |
| Security | `security` |
| Test | `paired-test` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Semantic Guard | `semantic-guard` |
| Paired Client | `paired-client` |
| Paired Media Handler | `paired-media-handler` |

<!-- LEAD REMINDER:
     - Update this table at EVERY phase transition
     - Capture teammate IDs AS SOON as you spawn them
     - When phase is review and all reviewers approve, advance to complete and proceed to Step 8 (Commit)
     - Only mark complete after Gate 3 approval
     - Use /devloop-status to check state
     - If interrupted, restart the devloop; main.md records start commit for rollback
     - Tier (ADR-0037 §D2): set the Tier row at setup from the manifest tier; under `light`, Step 5 SKIPS the Gate-1 plan round and writes a `### Gate 1 — SKIPPED (tier=light; reason: …)` marker instead of pending rows. A resumed devloop reads the tier from THIS Tier row (not the manifest, not the confirmations table); no Tier row ⇒ full (fail-safe).
-->

---

## Task Overview

### Objective
Task description (verbatim, run-story task #6):

> Multi-party static join-order slot placement in the Meeting Controller (docs/user-stories/2026-09-21-hear-each-other.md, R-1, R-2, R-3, R-4; ADR-0036 §6, §8). Pair with test (integration and env-test patterns), client and media-handler (consumers of the assignment shape).
> 
> The central new mechanism. Story 1's `crates/mc-service/src/media_routing/assignment.rs::compute_assignment` is a pure from-scratch recompute producing one egress stream per subscriber carrying ALL other sources as candidates (the MH-selects model, which is story 5's dynamic selection). R-4 slot stability cannot come from a pure recompute, because a from-scratch fill reshuffles slots when a middle sender leaves. So MC gains per-meeting slot state held in the meeting actor (`actors/meeting.rs`): per subscriber, an ordered vector of optional sender ids whose length is that subscriber's own declared audio slot count (per-subscriber, since N is client-declared; different subscribers may declare different counts), plus a per-meeting monotonic join-order sequence (NOT `SenderId`, which is deliberately non-Ord). Mutations: on JOIN, the joiner fills the first empty slot of each existing subscriber and nothing else moves; the joiner's own slot vector is filled from the earliest-joined other senders in join order, overflow left unassigned. On LEAVE, for each subscriber only the leaver's slot is freed and refilled with the earliest-joined currently-unassigned sender, if any; every other slot is untouched. A reconnecting participant keeps its original join-order rank (it never left the roster); a fresh rejoin gets a new rank. A pure render function turns the slot state into the existing `MeetingAssignment` output — now one pinned candidate per egress stream, N egress streams per subscriber; `egress_stream_id(subscriber, ordinal)` and `EgressOrdinalOverflow` already anticipate the per-slot ordinal loop. Slots that are declared but unfilled carry `SLOT_STATE_FEWER_SOURCES_THAN_SLOTS` on the wire; MC never emits `SLOT_STATE_ZERO_REQUESTED` or `SLOT_STATE_SOURCE_UNREACHABLE` this story and never fabricates a slot_id.
> 
> Multiple handlers (R-33; ADR-0036 §9): a meeting's participants MAY be placed on different handlers within the handler SET GC programs once via `assign_meeting` (up to two handlers, weighted random, frozen at first join; no GC change). Visibility is per handler: a subscriber's slot table is filled in join order from senders ON ITS OWN HANDLER only; participants on another handler consume no slot and are listed in the new `StreamAssignments.unreachable_sender_ids` (per-subscriber, MC-authoritative); the roster still shows everyone. No cascade and no client multi-handler send (story 6). MC scopes each client's handler URL list (`JoinResponse.media_servers` and `StreamAssignment.media_handler_url`) to that client's single assigned handler — the client transport is active/active and would connect to every URL it is given. A join or leave on a handler a subscriber cannot reach changes that subscriber's unreachable set but not its slots, and still counts as a structural change: re-emit its `StreamAssignments`. Placement lives in one seam: `webtransport/connection.rs::routing_input_for` (~2431) today gives every participant the FULL handler set, so `edge_handler` picks the sorted-first shared handler for every edge and everyone lands on handler[0]; the code documents that function as the single place that changes for per-participant placement. Implement: each participant gets ONE handler by round-robin over the sorted handler set keyed by join-order rank (a single-handler meeting is unchanged; a two-handler meeting alternates), plus a `test-seams`-gated dev-only override that pins participants to named handlers so S10 is deterministic (GC's set selection is weighted-random and returns both Kind handlers only when both are healthy). Co-location-preferred placement (fill handler[0] to its egress ceiling, then spill) is deferred: it needs the per-handler stream ceiling exposed to placement. This is §9 placement, not §7 selection. MC reads each handler's advertise address from the registration, never from a pod ordinal. A cross-handler join or leave re-emits the affected subscribers' StreamAssignments (their unreachable set changed) with NO MH re-push and NO generation bump, because no forwarding edge changed.
> 
> R-3, loopback removed: make `subscribes_to` non-reflexive — a participant never subscribes to itself, including when alone. Replace the N=1 self-edge test with a solo-participant-hears-nothing test (solo participant's slots all `FEWER_SOURCES_THAN_SLOTS`); remove the reflexive-predicate comment block. The MH-level Rust loopback test is untouched (MH executes edges blind).
> 
> Receive capability must reach the actor: today `handle_receive_capability` is connection-side and the actor never learns a participant's slot demand. Register the validated declaration (already bounded by `MC_MAX_RECEIVE_SLOTS`, 1..=64, with the per-connection declaration budget and identical-redeclaration short circuit) into the actor via a new message, making the actor the source of truth for membership, join order, slot demand and (later task) server-mute state. A declaration above the cap is rejected whole and counted by reason so it is observable (R-1); publish the configured slot cap as a gauge `mc_media_receive_slot_cap` from the same value the check enforces.
> 
> Structural re-push (R-4): `webtransport/connection.rs` ~641 pushes RegisterMeeting only when `is_first_participant`; the in-code comment lumping structural re-push with the handler-restart story is a story-1-era framing and is wrong. Every structural change — join, leave, capability change (and in the later task, server-mute change) — recomputes the assignment and re-pushes the FULL snapshot to each assigned handler, carrying the monotonic per-(meeting, handler) `policy_generation` that advances only when the assignment actually changed (`media_routing/generation.rs`; never the Redis fencing counter), and confirms MH's applied-generation echo via the existing confirm machinery, failing loudly on mismatch. Cadence, jitter and connectivity-loss re-assert stay in story 4. Add a third driver of slot-view emission: on a roster change the actor pushes updated full `StreamAssignments` lists to affected existing subscribers, with unchanged slots keeping their sender_id (so the client never resets a replay window), under a per-meeting work bound (the code already notes "a third caller of compose_and_emit needs its own bound"; the self-mute fan-out lesson applies), not the per-connection limiter.
> 
> Tests: unit tests for join-order fill, slot stability under join and leave, per-subscriber sizing, the reconnect-keeps-rank rule, the over-subscription case (N+2th sender unassigned), and the non-reflexive predicate; integration tests for structural re-push generation advance-only-on-change and applied-echo confirm, for slot-cap rejection counted, and for the re-emit bound. Rust env-tests against the live Kind cluster: two-sender routing with sender-to-slot binding observed at MH, over-subscription producing the fewer-sources state on the wire, and S10b: a meeting split across mh-0 and mh-1 where MC computes per-handler edge sets, each handler's snapshot carries only its own participants' edges, and each subscriber's `unreachable_sender_ids` names exactly the cross-handler participants while the roster carries all. Supersede story 1's loopback expectations in MC tests. No proto change: this task consumes the existing RegisterMeeting shape. Story-1's R-1 is superseded by R-3.

### Scope
- **Service(s)**: mc-service (primary); sdk-core + web-app e2e (client fixes, Lead ruling 1); env-tests; MH ConfigMap comments only (no MH code)
- **Schema**: No
- **Cross-cutting**: Yes — MC→MH push shape/cadence, MC→client signalling behaviour, client receive path; no proto change

### Debate Decision
{NEEDED/NOT NEEDED} - {Brief justification}

{If debate was needed, link to debate record: `docs/debates/YYYY-MM-DD-{topic}.md`}

---

## Cross-Boundary Classification

<!-- List EVERY planned file change. For each, classify per ADR-0024 §6.2:
     - Mine — in the implementing specialist's domain (trivial, the common case)
     - Not mine, Mechanical — cross-boundary, sed-test clean, guard-pipeline covered
     - Not mine, Minor-judgment — cross-boundary, bounded impact; owner must review & confirm at Gate 1 + Gate 3
     - Not mine, Domain-judgment — needs owner-implements or --paired-with=<owner>

     For Guarded Shared Area paths (ADR-0024 §6.4), Mechanical is disallowed; Owner must be filled.
     Fill Owner (if not mine) for cross-boundary rows.

     Path column convention: backtick-quoted paths. Globs (`*`, `?`, `[]`,
     trailing `/`, `/**`) and parenthetical annotations like `foo.rs` (regen)
     are tolerated by the `validate-cross-boundary-scope` parser at
     scripts/guards/common.sh, and are recommended where they clarify intent
     — use `dir/**` (or `dir/`, which the parser canonicalizes to
     `dir/**`) to scope a whole tree, `*.svelte` for a filename glob,
     and `(regen)` / `(cleanup)` /
     `(skeleton-only)` suffixes for per-row context. Prefer the simplest
     form that is accurate: if a literal path conveys the same information,
     use that; reach for a glob when enumerating every file would be noise,
     and reach for a parenthetical when the row's nature (regen, cleanup,
     new-vs-modify) materially changes how a reviewer reads it. Longer-form
     file-shape context (rationale, scope qualifiers, "why this shape") still
     belongs in § Implementation Summary or § Files Modified — the table
     answers one question per row: whose domain is this, and how stringent
     is the involvement. -->

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `crates/mc-service/src/media_routing/slots.rs` (new) | Mine | — |
| `crates/mc-service/src/media_routing/pusher.rs` (new) | Mine | — |
| `crates/mc-service/src/media_routing/assignment.rs` | Mine | — |
| `crates/mc-service/src/media_routing/generation.rs` | Mine | — |
| `crates/mc-service/src/media_routing/mod.rs` | Mine | — |
| `crates/mc-service/src/media_signaling/mod.rs` | Mine | — |
| `crates/mc-service/src/media_signaling/capability.rs` | Mine | — |
| `crates/mc-service/src/media_signaling/assignments.rs` | Mine | — |
| `crates/mc-service/src/media_signaling/directive.rs` | Mine | — |
| `crates/mc-service/src/media_signaling/outcome.rs` | Mine | — |
| `crates/mc-service/src/actors/meeting.rs` | Mine | — |
| `crates/mc-service/src/actors/messages.rs` | Mine | — |
| `crates/mc-service/src/actors/controller.rs` | Mine | — |
| `crates/mc-service/src/actors/mod.rs` | Mine | — |
| `crates/mc-service/src/actors/participant.rs` (test: `message_type` literal → `RAW_SERVER_MESSAGE_TYPE`, DRY F3) | Mine | — |
| `crates/mc-service/src/webtransport/connection.rs` | Mine | — |
| `crates/mc-service/src/webtransport/server.rs` | Mine | — |
| `crates/mc-service/src/grpc/mh_client.rs` | Mine | — |
| `crates/mc-service/src/observability/metrics.rs` | Mine | — |
| `crates/mc-service/src/lib.rs` (test-seams compile_error message) | Mine | — |
| `crates/mc-service/src/errors.rs` (`MediaPolicyDivergence` carries `applied_generation`) | Mine | — |
| `crates/mc-service/src/actors/meeting_media.rs` (new) | Mine | — |
| `crates/mc-service/src/media_routing/placement.rs` (new) | Mine | — |
| `crates/mc-service/tests/**` | Mine | — |
| `crates/mc-test-utils/src/media.rs` | Mine | — |
| `crates/mc-test-utils/src/token.rs` (comment) | Mine | — |
| `crates/mc-test-utils/src/mock_mh.rs` | Mine | — |
| `crates/env-tests/tests/27_mc_slot_placement.rs` (new) | Not mine, Minor-judgment | test |
| `crates/env-tests/tests/26_mh_quic.rs` (retire loopback test + `follow_mc_steering`; consume the shared MC/MH session fixture, DRY F6) | Not mine, Minor-judgment | test |
| `crates/env-tests/src/fixtures/` (new shared MC/MH WebTransport session fixture, DRY F6; orphaned metrics polls removed, DRY F10) | Not mine, Minor-judgment | test |
| `crates/env-tests/tests/24_join_flow.rs` (local `connect_mc`/`encode_framed`/`read_server_message`/`JoinRequest` literal replaced by the shared `fixtures::mc_session`, DRY F6 completion; no assertion changed) | Not mine, Minor-judgment | test |
| `crates/dt-guard/src/ts_retained_credentials.rs` (guard MACHINERY only — `mask_module_syntax` so inline `type` import/export specifiers are no longer read as declarations; no vocabulary/policy change; implemented by @infrastructure) | Not mine, Domain-judgment (owner-implemented) | infrastructure |
| `crates/env-tests/Cargo.toml` (wtransport incl. `dangerous-configuration`, proto-gen, prost, bytes moved dev-deps → deps so the lib fixture can use them; no new crate, no version change; env-tests is never shipped; flagged to security for Step-0 dependency audit) | Not mine, Minor-judgment | test (+ security audit) |
| `crates/mh-service/tests/media_session_binding_integration.rs` (module doc re-pointed from the retired loopback test; author paired-media-handler) | Not mine, Minor-judgment | media-handler |
| `packages/web-app/e2e/media-loopback.spec.ts` (author implementer, to paired-client's spec) | Not mine, Domain-judgment | client |
| `packages/web-app/e2e/fixtures.ts` (author implementer: shared `expectCountersFlatOverWindow`, stale loopback comments) | Not mine, Domain-judgment | client |
| `packages/web-app/e2e/README.md` (author implementer: media-loopback section rewritten for R-3) | Not mine, Minor-judgment | client |
| `docs/observability/metrics/mc-service.md` | Not mine, Minor-judgment | observability |
| `infra/grafana/dashboards/mc-overview.json` | Not mine, Minor-judgment | observability |
| `infra/docker/prometheus/rules/mc-alerts.yaml` (MCMediaGenerationDivergence `description` text and the `for: 0m` justification comment only; expr and `for:` untouched) | Not mine, Minor-judgment | operations |
| `docs/observability/alerts.md` (MCMediaGenerationDivergence Condition + Response steps 1-2) | Not mine, Minor-judgment | observability |
| `docs/specialist-knowledge/observability/INDEX.md` (pointer update for renamed/retired emitters) | Not mine, Mechanical | observability |
| `docs/specialist-knowledge/security/INDEX.md` (RegisterMeeting pointer; security F1: handler-url scoping, redirect prohibition, placement-pin seam) | Not mine, Minor-judgment | security |
| `docs/specialist-knowledge/semantic-guard/INDEX.md` (RegisterMeeting pointer) | Not mine, Mechanical | semantic-guard |
| `docs/runbooks/mc-incident-response.md` | Not mine, Minor-judgment | operations |
| `docs/runbooks/mc-deployment.md` (Carve-out #3 rollback hazard, adoption note beside the unconfirmed-pushes gate, stale "no re-assert" claims; OPS-R1) | Not mine, Minor-judgment | operations |
| `docs/runbooks/mh-deployment.md` (outcome-split rung: adoption arm out of this gate's scope) | Not mine, Minor-judgment | operations |
| `docs/runbooks/mh-incident-response.md` (Rung 2: retired "does not self-correct" premise; adoption counter named; OPS-R3) | Not mine, Minor-judgment | operations |
| `docs/runbooks/client-dev-local.md` (two outcome-split rungs: retired premise; adoption counter named; OPS-R3, observability G1) | Not mine, Minor-judgment | operations |
| `docs/runbooks/devloop-validation.md` (§8 triage row for `TRIAGE_HANDLER_SET` — "Triage MH handler-set health", the operator-lane literal `27_mc_slot_placement.rs` emits on a single-handler cluster; OPS-1) | Not mine, Minor-judgment | operations |
| `infra/services/mh-service/configmap.yaml` (comment blocks, OPS-7) | Not mine, Minor-judgment | media-handler |
| `packages/sdk-core/src/media/pipeline/hopSequenceMonitor.ts` (author paired-client: slot-and-sender keying, RFC-1982 resets) | Not mine, Domain-judgment | client |
| `packages/sdk-core/src/media/pipeline/ingress.ts` (author paired-client: hop observe call site only) | Not mine, Domain-judgment | client |
| `packages/sdk-core/src/media/lifecycle/AudioPipeline.ts` (author paired-client: read loop from `StreamAssignment.media_handler_url`) | Not mine, Domain-judgment | client |
| `packages/sdk-core/src/session/MeetingSession.ts` (author paired-client: wire streamAssignments to the pipeline) | Not mine, Domain-judgment | client |
| `packages/sdk-core/src/config/clientConfig.ts` (author paired-client: reorder-depth bound) | Not mine, Domain-judgment | client |
| `packages/sdk-core/src/media/pipeline/__tests__/` (author paired-client: unit tests for the hunks above) | Not mine, Domain-judgment | client |
| `packages/sdk-core/src/media/lifecycle/__tests__/` (author paired-client: unit tests for the hunks above) | Not mine, Domain-judgment | client |
| `packages/sdk-core/src/config/__tests__/` (author paired-client: unit tests for the hunks above) | Not mine, Domain-judgment | client |
| `packages/sdk-core/src/session/__tests__/` (author paired-client: unit tests for the hunks above) | Not mine, Domain-judgment | client |
| `packages/sdk-core/src/signaling/__tests__/` (author paired-client: no-streams `SendDirective` builder) | Not mine, Domain-judgment | client |
| `docs/user-stories/2026-09-21-hear-each-other.md` (Lead-authored: task 15/19 prompt amendments; reserved-scenario line correction) | Not mine, Minor-judgment | Lead |
| `docs/TODO.md` (my entries: close the two retired Media Path Obligations items, add the co-location capacity gap; the §Client Architecture signaling-loss/reconnect entry is Lead-authored, not edited by me) | Mine | — |
| `docs/specialist-knowledge/meeting-controller/INDEX.md` | Mine | — |

---

## Planning

### Problem restated as mechanism

Story 1 made the client-visible view (send directive + slot assignments) and the MH snapshot two outputs of one **pure from-scratch recompute** (`compute_assignment`), triggered from **connection-side** code: the push fires only for the first joiner, and a view is composed only for the connection that sent a message. Multi-party breaks that shape in three ways: (1) slot stability (R-4) is history-dependent, so the recompute needs **state**; (2) most structural changes (a leave, a peer's declaration, a peer's mute) happen with **no connection context** for the participants whose view changed; (3) placement (R-33) must be decided **before** the `JoinResponse` is built. The wider class: *every participant's client view and every handler's snapshot must equal the latest render of meeting state* — not only after join/leave, but after capability changes and client mute too (the current mute path recomposes the **muter's own** view, which under non-reflexive visibility is never the view that changed). This plan completes that invariant for all four drivers (join, leave, capability, mute), not only join/leave.

### Design

**D1. Pure slot state — `media_routing/slots.rs` (new).** `JoinRank(u64)` (per-meeting, monotonic, `Ord`, internal only: never on the wire, never a label/log/span field). `SlotTable { members: BTreeMap<JoinRank, Member>, rank_of: HashMap<SenderId, JoinRank>, next_rank }`, `Member { sender, handler: HandlerId, audio_slot_ids: Vec<SlotId>, slots: Vec<Option<SenderId>> }`. Rank and `sender_id` are visibly different things (rank is the ordering key; `sender_id` still comes only from `SenderIdAllocator::allocate()`).
- `subscribes_to(s, p)` = `s.sender != p.sender && s.handler == p.handler` — **non-reflexive** (R-3), one named predicate, the eligibility rule for every fill.
- **admit(sender, handler)** → new rank; each existing co-handler member with an empty slot takes the joiner in its FIRST empty slot; nothing else moves. Joiner starts with 0 slots (no declaration yet).
- **set_demand(sender, audio_slot_ids)** (validated declaration, post-cap — never sized from raw input): ordinal *i* ↔ *i*-th audio slot in declaration order. Same count → occupants keep ordinals (only slot ids change). Grow → new empties filled from earliest-rank unassigned eligible senders. Shrink → tail truncated; truncated senders become unassigned for this subscriber and are NOT placed elsewhere except that any remaining empty slot is refilled earliest-first (invariant below).
- **remove(sender)** → for each other member, only the leaver's slot is freed and refilled with the earliest-rank currently-unassigned eligible sender, if any.
- Reconnect = no table change (participant never left the roster, keeps rank and slots). Fresh rejoin = new participant → new `sender_id` → new rank.
- Invariants (asserted by an exhaustive small-scope model check, per @paired-test): no self; no duplicate per subscriber; only co-handler senders; no empty slot while an unassigned eligible sender exists; a filled slot changes only if its sender left or the subscriber redeclared; `slots.len()` == declared audio count; unreachable == exactly the cross-handler roster.
- **unreachable_for(s)** = roster members on another handler, excluding self and ALL co-handler senders (N+2th same-handler sender is "no slot", not unreachable), sorted numerically; meeting-local only; carries no handler identity.
- **render(handlers)** → `MeetingAssignment`: one `EgressStreamPlan` per FILLED slot, `egress_stream_id(subscriber, ordinal)` with ordinal = slot-vector index (stable across middle leaves and refills), `slot_id` = the client's declared id at that ordinal (never fabricated), exactly one candidate, placed on the subscriber's handler. Unfilled slots emit **no** egress stream (MH must not install sourceless edges; FEWER_SOURCES is client-wire only). Every handler in the set gets an entry, possibly empty (keeps the "every handler registered at first join" property MH needs for provisional promotion). `EgressOrdinalOverflow` stays a value (unreachable while the cap is ≤64; tested at the 256 boundary directly on `egress_stream_id`).
- `compute_assignment`, `MeetingRoutingInput`, `RoutingParticipant`, `edge_handler`, `MAIN_AUDIO_SLOT_ID` are **removed** (replaced, not left dead); the MH-selects all-candidates shape returns with story 5. Placement is one handler per participant; story 6 (multi-handler send) widens `Member.handler`.

**D2. Placement — one home.** `slots::place(rank, sorted_handlers) = sorted[rank % n]` via `.get()` (no indexing panic). Consumed by the actor at join and stored on the member; `JoinResponse.media_servers`, `StreamAssignment.media_handler_url` and `SendTarget.media_handler_url` are all read from the **same** `HandlerUrls` table (server-derived from the Redis registration, byte-identical strings) and the participant's placed handler. `media_servers` becomes a single entry. Handler identity/URL come from `MhAssignmentData` (the registration), never a pod ordinal. **Override**: `MeetingSeams { sender_id_cursor, placement_pins: BTreeMap<user_id, mh_id>, reemit_batch }` under `test-seams` only (subsumes the existing sender-id-cursor seam; `create_meeting_with_sender_id_cursor` → `create_meeting_with_seams`). A pin naming a handler outside the meeting's set fails the join loudly (never falls back). Seam is compiled out of every non-`debug_assertions` build; the `lib.rs` `compile_error!` message is extended to name it, keeping the needle phrase `must never be enabled in a release build` unbroken on one line. **The seam is NOT reachable in Kind** (release build) — it serves in-process MC integration tests only; the env-test gets determinism from sequential joins + round-robin over the sorted set and **hard-fails** on the precondition "exactly two distinct handler URLs across participants" (option (a) from @paired-test / OPS-1).

**D3. The actor is the single writer and the single composer.** `handle_connection` reads Redis **before** the actor join (a missing assignment now fails before admission) and passes `JoinMedia { deps: Arc<MediaRoutingDeps>, handlers: MeetingHandlers }` in the join message (`MediaRoutingDeps` = mh client, `PolicyGenerations`, mc_id, mc gRPC endpoint, `MediaStreamPolicy`; built once by `WebTransportServer`). The actor installs the handler set on first join (frozen; a later join carrying a different set logs ERROR and keeps the frozen set). `JoinResult` gains the placed handler endpoint. New message `RegisterReceiveCapability { participant_id, declaration, respond_to }`; `update_self_mute` unchanged in shape. After every structural change (join, leave via the single `remove_and_broadcast_left` choke-point — covers explicit leave, clean close and grace expiry —, capability, audio-mute change) the actor runs **reconcile**:
1. render `MeetingAssignment` from the slot table;
2. per handler: `next_generation(meeting, handler, &assignment)` (advances only on output change; never the Redis fencing counter) and publish `(generation, snapshot)` to that handler's push worker;
3. mark affected declared participants dirty (join/leave/capability → all declared; mute of S → subscribers holding S) and flush.

**D4. Push worker per (meeting, handler) — `media_routing/pusher.rs` (new).** One task per handler, spawned lazily by the actor, fed by a `tokio::sync::watch` (latest wins, coalesces bursts), lifetime = the **meeting's** cancel token (not a connection's — a leave-triggered push cannot be cancelled by the leave). Serialization removes both out-of-order hazards @paired-media-handler named (stale content under a higher number; a correct push misclassified as mismatch). The worker keeps `confirmed_generation`: a published job whose generation equals it is a no-op (**no RPC** — "cross-handler join → no re-push, no bump" holds); otherwise it pushes with the existing retry/terminal split and existing confirm (`MhRegistrationClient::register_meeting` + `confirm::evaluate`), abandoning a retry when a newer job has arrived. A failed push stays unconfirmed, so the NEXT structural change of any kind re-pushes it (OPS-2 recovery is "next roster event", and is loud meanwhile: existing error logs + `mc_media_policy_pushes_total{outcome}` + `mc_register_meeting_*`; the cadence backstop remains story 4). The actor never awaits gRPC; its only await is the shared `PolicyGenerations` lock (in-memory map insert, never held across I/O). `register_meeting_with_handlers`/`build_routing_input`/`routing_input_for` in `connection.rs` are removed (logic moves, not duplicated).

**D5. Client view emission (the third driver) with a per-meeting bound.** Per declared participant the actor stores `last_directive`/`last_assignments`. Flush: pop up to `reemit_batch` dirty participants per actor turn (Rust const, default 64 — a server work bound with no operator story, same register as `MUTE_WORK_BURST`; injectable via the seam), compose directive (`build_send_directive`) and assignments (`build_stream_assignments` + unreachable set + mute view from the actor's own roster), emit **only what changed**, directive before assignments, via the participant handle. Remaining dirty entries are carried (not dropped) and flushed on subsequent turns through a `select!` arm enabled while the set is non-empty — so the bound **defers and coalesces, never sheds**, and interleaves with mailbox traffic. This fixes @paired-client C1: a publisher whose target set changes (solo A gets a target when B declares; a leave empties a target set) receives a fresh `SendDirective`. Only declared participants are emitted to (C5b); every declared slot yields exactly one entry (C5a). Disconnected-in-grace participants are skipped; on actor-level reconnect `last_*` resets and the participant is re-dirtied, so the new connection gets the full current view. Directive: single target == the participant's placed handler (asserted in tests). `connection.rs::compose_and_emit` is deleted; the connection keeps parse/cap/budget/identical-redeclaration short-circuit/limiters and forwards accepted declarations to the actor (the short-circuit is armed only when the actor accepted the registration).

**D6. Retired story-1 premises (same premise, every site).** `SlotIdNotPlanned` rejection + `PlannedAudioSlot` + the per-connection `planned_audio_slot` and `handlers` caches + `DirectiveOutcome::NoPlannedEgressSlot` are removed (declared slots are now the assignment input); the `media_signaling/mod.rs` namespace-coupling section, `capability.rs` doc, `assignments.rs` module doc, the `media_servers` ordering comment block in `build_join_response`, the reflexive-predicate comment, the `media_routing/mod.rs` loopback/"one push" scope text and `docs/TODO.md` entry "The declared audio slot id must equal the MH egress stream id…" (closed) are rewritten or removed. `SlotState::SourceUnreachable` is no longer emitted: the handler-url-unresolvable arm becomes a counted composition failure (`DirectiveOutcome::HandlerUrlUnresolved`, logged), since both come from one table and a miss is an MC defect. `unmatched_plan_slots` is retired (see Gate-1 amendments, @observability ruling a). The TODO entry "Slot-state re-conveyal failures on the mute path are loud-but-unqueryable" is closed: all composition now runs in the actor and failures record `mc_media_send_directives_total` unconditionally.

**D7. Telemetry** (no identity anywhere: no rank, sender_id, slot_id, handler id/url, meeting id as label or span attribute; new actor fns `#[instrument(skip_all)]`).
- `mc_media_receive_slot_cap{key_custody}` gauge, set once at boot in `main.rs` from `ClientMediaConfig::max_receive_slots` (the field the parse check reads) via `set_receive_slot_cap`; test asserts gauge == that field (no literal). Cap rejection stays `mc_media_receive_capability_declarations_total{outcome="slot_count_over_cap"}` — **no new counter** (O-1; the partition holds).
- `mc_media_slot_view_emissions_total{outcome,key_custody}`, `outcome ∈ {emitted, deferred, delivery_failed, composition_failed}` (@security: an MC-defect composition failure must not hide inside ambient delivery failure; final shape @observability's call) (closed enum in `outcome.rs`, zero-initialised) — the bound's visible edge (O-3/OPS-4).
- `mc_media_unreachable_senders_total{key_custody}`, incremented by the unreachable count per emitted `StreamAssignments` (O-4).
- Placement INFO log at join on `mc.actor.meeting`: participant_id + mh_id (no rank, no sender id) — answers "which handler did X land on" (OPS-3).
- Catalog rows + `mc-overview.json` panels (stat for the cap gauge next to Receive Capability Declarations; rate panels for the two counters); edits to `mc_media_slot_states_total` (now also server-driven; `source_unreachable` no longer reachable; unfilled = `fewer_sources_than_slots`), `mc_media_generation_divergence` (written per structural change), `slot_id_not_planned` value removed, `no_planned_egress_slot` removed.
- `MCMediaGenerationDivergence` annotation + runbook Scenario 15 remedy updated (OPS-9): any structural change re-pushes; force one only if quiescent. Threshold not retuned: per-handler serialization removes the out-of-order class that would otherwise make non-match churn-driven; each remaining non-match is a real divergence (observability to confirm).
- Runbook: new scenario "participant hears only part of the roster" separating cross-handler visibility (expected, §9) from over-subscription (`fewer_sources`/unassigned) from a routing defect (@operations to confirm numbering — the story reserves MC 17 for rotation storm in task 18). MH ConfigMap comments for `MH_MAX_CANDIDATE_SOURCES_PER_EGRESS`/`MH_MAX_EGRESS_STREAMS_PER_MEETING` updated to the new binding constraint (OPS-7). Capacity gap (round-robin ignores per-handler ceiling; MH admission + whole-registration reject are the backstops, loud via confirm) recorded in `docs/TODO.md` §Media Path Obligations (OPS-5).

**D8. Tests.**
- Unit (`slots.rs`): exhaustive model check (≤6 ops over 4 participants × 2 handlers × slot counts {1,2,3}, join/leave/rejoin/reconnect/redeclare) asserting every invariant in D1; named: join-order fill, middle-sender leave refills only that slot, N+2th unassigned (and not in unreachable), per-subscriber sizing 1 vs 3, redeclare shrink/grow rule, reconnect-keeps-rank vs fresh-rejoin-new-rank producing DIFFERENT outcomes, non-reflexive predicate, solo participant: zero egress streams in the MH assignment and all slots `FEWER_SOURCES_THAN_SLOTS`. Render: one candidate per stream, stable `egress_stream_id` across a middle leave, unique ids and `(subscriber, slot_id)`, ZERO_REQUESTED/SOURCE_UNREACHABLE never emitted, handler-scoped edges only. Placement: alternation, sorted-set independence of Redis order.
- Actor unit: reconnect keeps rank/slots, re-dirties; pin outside set rejects.
- Integration (`tests/slot_placement_integration.rs` new, via the accept-loop rig): C1 (solo A, B joins same handler and declares → A receives a directive with one target equal to its `media_servers[0]`); join fill + leave refill re-emitted to existing subscribers with unchanged slots keeping `sender_id`; cross-handler join/leave → StreamAssignments re-emitted with changed unreachable set and **zero** new MH calls / unchanged generations; identical redeclaration → no push; generation advances only on change (asserted on the stub's received generations and call counts, not only metrics); graceful leave, clean close and grace-expiry each drive the re-push; slot-cap reject at cap+1 / accept at cap (derived from config), whole-reject leaves actor state unchanged, counter delta by reason; gauge == config; re-emit bound with seam batch=1 → `deferred` counted, all subscribers still converge, two meetings don't share the budget; two structural changes in flight → MH ends on the newest generation with no spurious mismatch. `media_policy_push_integration.rs`: applied-echo mismatch on a RE-push fails loudly; gen>0 zero-stream snapshot confirms `match`. Story-1 loopback assertions in `register_meeting_integration.rs`, `media_policy_push_integration.rs`, `media_client_signaling_integration.rs`, `otel_grpc_outbound_integration.rs`, `mc-test-utils/src/media.rs`, `grpc/mh_client.rs` are **rewritten** to two-party expectations, not dropped.
- Env-test (`crates/env-tests/tests/27_mc_slot_placement.rs`, new; Layer 7, run by the Lead): five distinct registered users join one meeting sequentially (ranks 0–4 → A,C,E on h0; B,D on h1). Hard precondition: exactly 2 distinct handler URLs, each participant's `media_servers` has length 1 and equals its own `media_handler_url`. **S10b**: every subscriber's filled slots are co-handler senders only; `unreachable_sender_ids` == exactly the cross-handler set (set equality) while the roster carries all five. **Two-sender binding at MH**: A declares 2 audio slots; C and E connect to h0 and send marked datagrams; A receives each with relay `stream_id` equal to the slot id A's own `StreamAssignments` named for that sender, distinct between the two (expected values read from the wire, not literals; positive control: both must arrive; sanity-run once against a swapped expectation). **Over-subscription**: C declares 1 slot → A fills it, E unassigned and absent from C's unreachable set; B declares 3 slots → one `ACTIVE` (D) + two `FEWER_SOURCES_THAN_SLOTS` with `sender_id` absent. No sleeps: bounded read-until-predicate helper with distinct "no StreamAssignments" vs "wrong content" failures. `26_mh_quic.rs::test_mh_forwards_an_audio_datagram_back_to_its_sender` (single-participant loopback) and its `follow_mc_steering` helper are retired, superseded by the above; the MH-level Rust loopback test is untouched.
- Browser: `packages/web-app/e2e/media-loopback.spec.ts` test 1 goes red the moment R-3 lands; @paired-client converts it to solo-hears-nothing (slot state `fewer_sources`, ingress flat) in this devloop — no `test.skip`.

### Gate-1 amendments (folded into the plan)
- (@paired-media-handler a) The actor publishes with `watch::Sender::send_replace` on EVERY reconcile (never an equality guard); the worker's rule is "published gen != confirmed_generation → push", evaluated on each wake. Test: push fails, a cross-handler join follows (this handler's assignment unchanged), the stub receives exactly one retry at the SAME generation.
- (@paired-media-handler b) configmap comments: the egress-stream bound is stated PER HANDLER (sum over subscribers placed on this handler of min(N_s, P_h − 1)); candidate-sources "no longer binds under static fill; rebinds when story 5 returns multi-candidate streams — do not lower it to 1"; the `MH_MAX_MUTED_SOURCES_PER_MEETING` precondition paragraph gains the two new zero-stream cases (joined-but-undeclared; alone on its handler) and is restated on the real invariant (muted senders on a handler ≤ P_h).
- (@paired-media-handler c) D4 code comments say recovery is "the next structural change, or after an MH restart the story-4 re-assert" — a reconnect does not re-push.
- (@paired-test T1) Model check: state-space search over canonical states (valid ops only, visited-set), render invariants checked every step (unique `egress_stream_id`, one candidate, edge on the subscriber's own handler, no stream for an unfilled slot), explored-state count reported and floored as a positive control, ~2s debug budget.
- (T2) Env-test binding gated by send-until-received per sender marker under a bounded deadline; no Prometheus gate; no timed negative assertion (cross-handler non-delivery is proven by the S10b wire set-equality plus MH's component-tier S10c, task 10).
- (T3) One `#[tokio::test]` owning the one 5-participant meeting, phase-labelled failures; 5 users registered once (registration cost noted against the AC 100/min Kind limit); precondition failure reads "ENVIRONMENT: GC returned N handlers".
- (T4) Grace expiry via paused tokio time; the in-flight test holds the first RPC with a stub-controlled barrier; bound convergence asserted on each subscriber's final view.
- (T5) Browser test 2 tightened to `fewer_sources` for a solo participant (part of the @paired-client ask).
- (T6) Implementation Summary maps every removed `assignment.rs` test and the retired loopback env-test's relay-region assertions (publisher region/payload/signature untouched, own hop sequence) to their new coverage; the latter are carried into 27's binding check. The `MAIN_AUDIO_STREAM_NUMBER` const assertion is kept.
- (T7) Generation no-change arm asserts both handlers, including the empty-snapshot one staying at its first-join generation with no re-push.
- (T8) Integration row: a participant reconnects in grace while peers join/leave; it receives the full current view with slots and rank unchanged.
- (@paired-client i) I (implementer) edit `packages/web-app/e2e/media-loopback.spec.ts`; @paired-client reviews. Test 1 becomes "solo participant hears nothing and is directed to send nothing (R-3)": the primary positive control is slot-0 `data-slot-state` polled to exactly `fewer_sources`; secondary, framesAccepted and framesSent stay flat over a window, through a shared flat-window helper factored from `expectEgressFlatWhileMuted` (with its sample-count vacuity check); `waitForFirstMediaFrame` is not called. The comment says egress is flat because the target set is empty (§5), not because of mute. Both tests assert `mediaServers.length === 1`; test 2 polls to exactly `fewer_sources`; the header is rewritten. The spec carries a comment naming the coverage loss (browser-tier structural client-mute proof) and its owner (task 15).
- (@paired-client ii) Client read-loop trigger + hop-monitor keying are routed by the Lead (task 7 prompt amendment or client hunks here), not `docs/TODO.md`.
- (@security S-A) `HandlerUrlUnresolved` fails closed: the affected participant's view is NOT emitted (neither directive nor assignments), counted on `mc_media_send_directives_total` + `mc_media_slot_view_emissions_total{outcome="composition_failed"}` and logged ERROR. MC never emits an `ACTIVE` slot with an empty `media_handler_url`, and never a `SendTarget` with an empty url; both are asserted.
- (S-B) `MeetingSeams`, its `Default`, `create_meeting_with_seams` and `spawn_with_seams` all carry `#[cfg(feature = "test-seams")]` on the TYPE and the fns — no always-present struct with gated fields. The `compile_error!` message is EXTENDED by appending a clause (never reflowed) naming what the placement seam does; `NEEDLE_PHRASE` itself is untouched (if that ever changes it returns to @security as Domain-judgment).
- (S-C) The handler set and its URL table are frozen TOGETHER as one `MeetingHandlers` value at first join, so ids and urls can never come from different Redis versions and no later join or changed Redis entry can add a handler to a live meeting or move a placed participant. Divergence is an ERROR log plus a counter (`mc_media_handler_set_divergence_total`, shape with @observability).
- (S-D) Amplification named in D5's doc: one accepted DISTINCT redeclaration now costs a render + per-handler generation + one publish + a dirty-flush over declared participants. Bounds: per-connection declaration budget, `watch` latest-wins coalescing, advance-only-on-change, and `reemit_batch`. Adversarial test added: rapid distinct redeclarations (alternating N=1/N=2) coalesce at the worker — assert the stub's call count is far below the declaration count and that other participants still converge.
- (S-E) The dirty structure is a `HashSet<participant_id>` with idempotent marking, so the carried backlog is bounded by declared participants, never by event count. Retained view state is O(participants × N) (`last_directive`/`last_assignments`), bounded by the roster, stated once in D5's doc.
- (S-F) Structural, not a filter: a participant enters `SlotTable` only through `admit(sender, handler)`, so a roster entry without a `sender_id` cannot be named by `unreachable_for`; `unreachable_for` reads `members` alone and has no path to any cross-meeting or global registry. Stated at the function.
- (@operations OPS-16, blocking) **Generation adoption after an MC restart.** `PolicyGenerations` is process memory. MH ignores a lower generation as `RejectedStale` without swapping, and ignores an EQUAL generation even when the content differs. MH echoes its truthfully installed number on every reply path. **This is a pre-existing latent defect, not a regression introduced here.** Today a restarted MC pushes generation 1 into a meeting MH holds at 1, lands on the equal branch, and the stale edges stay live permanently with only a WARN. Task 6 moves that case onto the `RejectedStale` branch, which would take K structural changes to climb out of. What this task changes is FREQUENCY: pushes now happen on every structural change, so restarts land on this routinely. Fix in `pusher.rs`: when a reply that `confirm::evaluate` actually classified shows `applied > sent`, call `PolicyGenerations::adopt_floor(meeting, handler, applied)` and re-push at **K+1 strictly**. Adoption is gated on a classified reply, never on the retry/terminal error split. A structural reject returns `invalid_argument` with no body and no echo, and reading a default 0 there would silently un-adopt. `RoutingTable::install` replaces the meeting's entry wholesale, so the K+1 push also swaps out the pre-restart table (and its re-issued-sender-id topology) in one round trip. The window is bounded by restart → first structural change → confirm → K+1. This fix is **not deferrable to story 4**: story 4's detector keys on `process_start_epoch_ms`, i.e. the MH-restarted direction, while this is the MC-restarted direction. No persistence, no Redis, no proto change. "Advances only when the assignment changes" is preserved, because adopting a floor corrects the counter's origin rather than advancing it. Test: push 1..K, drop and rebuild the registry (the restart), push again, and assert the stub sees a generation > K and confirms `match`, not an unbounded mismatch loop.
- (OPS-16 consequence, runbook — @operations ruling) MC Scenario 18 gains one arm; **no `docs/TODO.md` entry**, because `adopt_floor` is the mitigation for the misdelivery too and the post-fix window is one extra RPC round trip after the first post-restart structural change. Do NOT record "tasks 11 and 4 own it": task 11's `EndMeeting` covers only a graceful shutdown (a crash, OOM kill or eviction sends nothing), and story 4's detector covers the MH-restarted direction, not this one. The arm states: **symptom** — participants connected and the UI healthy but nobody hears anybody, with client-side `signature_invalid` rather than silence (the discriminator: ordinary divergence and a policy-bound refusal both present as silence, whereas signature failures mean frames ARE arriving and being rejected); **cause** — an MC restart re-issued per-meeting sender ids from 1 (`media_admission/sender_id.rs:148`, process memory) while MH still held the pre-restart table (`RoutingTable` is install-only; `EndMeeting` unimplemented); **expected resolution** — self-clears on the first post-restart structural change once adoption re-pushes above the installed number, and it **fails closed** (the wrong audio is never played, it is rejected at the signature check), so this is an availability event, not a confidentiality one; **what not to do** — never restart MH to clear it (it sheds every session on the pod, recovers no affected meeting and destroys the evidence); a failure to self-clear means adoption is not working and is capture-and-escalate.
- (OPS-16 residual, stated at the fix site not in `docs/TODO.md`) `adopt_floor`'s doc records what it does NOT fix: an MH connection that OUTLIVES an MC restart keeps its connect-time sender binding (`SenderBindings::bind`, incumbent-wins), so if the client's post-restart rejoin yields a different `sender_id`, no policy push can correct that connection. A client that fully reconnects its MH transport self-heals, because a same-participant re-bind is a takeover. Closing it needs the client or MC to drop the MH transport on MC-side session loss. **The residual is LIVE** (@paired-client, verified): the SDK does NOT tear the MH transport down on a spontaneous MC signaling close — `SignalingClient::#teardown` closes only the signaling transport and holds no reference to `MediaTransport`, and `MeetingSession` re-emits the signaling error without calling its own `disconnect()`, which is the only path to `#media.disconnect()`. So MC Scenario 18's arm GAINS a REQUIRED step: **the affected participant must fully rejoin, meaning a page reload** — a reload is what drops the MH transport (tab teardown); "click join again" is not enough, and there is no client reconnect logic to self-heal it (`MeetingSession` is single-use per join and the SDK sends an empty `correlation_id`/`binding_token`). The arm also warns that the participant may not self-report: per §6 they see slots still claiming `ACTIVE` with nothing arriving, so expect "I can't hear anyone", not "I was disconnected". The operator instruction (per @operations) is "**every participant who was connected to a meeting on that MC before the restart reloads the page**", scoped to meetings on that MC, and the arm says WHY it is not targeted: the affected set is unobservable from the client's presentation (the error is only stored in `MeetingStore.applyError`, nothing renders it, and silence is indistinguishable from the other silence-presenting causes), so do not wait for reports. Rejoin is the ONLY recovery, not merely a faster one. The fix-site residual sentence stays phrased plainly, so it remains true if the client's teardown behaviour changes.
- (@dry-reviewer D6 add) The `connection.rs` ~955 doc bullet claiming `ServerMuteRequest` is "consumed" is corrected (there is no dispatch arm; it falls to "ignored").

- (@paired-media-handler, OPS-16 constraints) `adopt_floor` re-pushes at **K+1 strictly** (an equal generation is an MH no-op even when the content differs), and the floor is read **only from a confirm reply** — a structural reject returns `Status::invalid_argument` with no body and no echo, so there is nothing to adopt on that path. Both stated at the function and covered by the restart test.
- (@paired-media-handler vs @operations, resolved in MH's favour) The `MH_MAX_MUTED_SOURCES_PER_MEETING` derivation does NOT survive: it rests on "every sender is also a subscriber holding at least one egress stream", which held only because story 1's predicate was reflexive. R-3 plus a joiner admitted with zero slots break it. The precondition paragraph gains the two zero-stream cases and is restated on the real invariant. Comment-only; practical impact nil.

- (@observability, `composition_failed` overlap) The `mc_media_slot_view_emissions_total` entry gains a "recorded so it is not alerted on twice" bullet in the `mc_media_sender_binding_responses_total` shape: a composition failure increments BOTH series; `mc_media_send_directives_total` carries the STAGE dimension and is the alerting/triage home; `composition_failed` exists only so the flush-turn disposition set stays complete. The entry also classifies its values as MC defect (`composition_failed` — any non-zero is a bug) versus environmental (`delivery_failed`), matching the sibling entry's split, and states explicitly that under the positive failure predicate a later variant defaults to NOT being a failure and must be classified deliberately.

### Surfaced to @team-lead (not blocking this plan, but story-level)
1. **Round-robin splits every ≥2-participant Kind meeting** when GC returns both handlers: ranks 0/1 land on different handlers, so a two-person meeting has zero edges, and R-1's N+1 demo in Kind hears only co-handler peers. This is the story's chosen rule (Assumption 10); tasks 15 (browser S1) and 19 (manual plan) must be written against it. Flagging because it reads like a regression to anyone not expecting it.
2. **Client receive path keyed on the send directive** (@paired-client C1 detail): under static fill a participant can have filled receive slots while being in nobody's slot (e.g. N=1 with three participants: C hears A, but nobody holds C). MC correctly sends C an empty target set (§5 "send nothing"), and today's SDK then never starts its read loop, so C hears nothing. MC will not put a receive concern into the send directive; the client must open its receive path from `StreamAssignment.media_handler_url` (task 7 scope). Raised with @paired-client.


---

### Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test (paired-test) | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Semantic Guard | confirmed |
| Paired Client | confirmed |
| Paired Media Handler | confirmed |

Classification-sanity guard: `STATUS=OK`. Plan approved by Lead 2026-09-24.

Lead rulings at Gate 1: (1) the two client fixes (read loop starts from `StreamAssignment.media_handler_url`; hop monitor keyed per slot with sender reset) fold into this loop, paired-client authoring; (2) round-robin placement kept per Assumption 10, with the task 15/19 manifest prompts amended by the Lead; (3) the OPS-16 floor adoption fixed in pusher.rs.

---

## Pre-Work

{Any pending changes committed before starting, dependencies resolved, etc.}

{Or "None" if no pre-work was required}

---

## Implementation Summary

### Slot state, placement, render (D1, D2)
| Item | Before | After |
|------|--------|-------|
| Assignment computation | `compute_assignment`: pure from-scratch recompute, one egress stream per subscriber with EVERY other source (and itself) as candidates | `media_routing/slots.rs::SlotTable`: per-meeting join-order state in the actor; `render()` emits one stream per FILLED slot, exactly one pinned candidate, `egress_stream_id(subscriber, slot-vector ordinal)` |
| Visibility | reflexive (loopback) | `subscribes_to` non-reflexive and co-handler only; the ONE eligibility rule (admit/set_demand/remove/unreachable) |
| Placement | every participant given the full handler set; edges on sorted-first handler | `media_routing/placement.rs::MeetingHandlers::place` — round-robin by `JoinRank` over a set sorted at construction; test-seams pins (in-process only) |
| `media_servers` / `StreamAssignment.media_handler_url` / `SendTarget.media_handler_url` | full, unsorted set / from assignment / from assignment | all three from the one frozen `MeetingHandlers`: exactly the placed handler |
| Slot demand | per-connection cache; `PlannedAudioSlot`, `slot_id_not_planned` | registered into the actor (`RegisterReceiveCapability`); declared audio slots are the assignment input; retired: `PlannedAudioSlot`, `slot_id_not_planned`, `no_planned_egress_slot`, `MAIN_AUDIO_SLOT_ID`, `RoutingParticipant`, `MeetingRoutingInput`, `edge_handler`, `HandlerUrls` |
| Unreachable set | always empty | `SlotTable::unreachable_for` → `StreamAssignments.unreachable_sender_ids` (structural sender-id rule, meeting-local) |

Removed-test accounting (@paired-test T6): story-1 `assignment.rs` tests → `n1_loopback…`/`n2_…four_edges` superseded by `slots::a_solo_participant_hears_nothing` + `a_joiner_fills…`; `disjoint_handler_membership…` → `slots::a_split_meeting…`; `overlapping_handler_membership_tie_breaks…` → retired with multi-membership (placement is one handler); `handler_with_no_participants…`/`empty_meeting…` → render gives every handler an entry (model check + `slot_placement_integration::a_cross_handler…` asserts both handlers registered); `egress_stream_id_and_subscriber_slot_are_unique…` → model check render invariants; `canonical_ordering…` → render sorts by `egress_stream_id` (model check), `visibility_is_reflexive` → `slots::visibility_is_non_reflexive_and_per_handler`; packing tests + `MAIN_AUDIO_STREAM_NUMBER` const assertion kept. The retired loopback env-test's relay-region assertions (stream id rewritten, own hop sequence, publisher region/payload/signature byte-identical) are carried into `27_mc_slot_placement.rs`'s binding phase with two senders.

### Actor as single writer and composer (D3, D5)
`actors/meeting_media.rs::MeetingMedia` holds the frozen handler set, slot table, push workers and per-participant last-emitted views. Every structural change — join, the single leave choke-point (explicit leave, clean close, grace expiry), capability declaration, self/server audio-mute — runs `reconcile` (render → per-handler `next_generation` → `publish` via `send_replace`) and a bounded `flush` (≤ `SLOT_VIEW_FLUSH_BATCH` per turn; `BTreeSet<participant_id>` dirty set, deferred never dropped; only changed messages sent, directive first; declared participants only). Reconnect re-dirties the reconnector without re-pushing. `compose_and_emit`, `routing_input_for`, `build_routing_input`, `register_meeting_with_handlers` deleted from `connection.rs`; the connection keeps parse/cap/budget/limiters and reads Redis (after resolving the meeting) BEFORE the actor join.

### Push worker (D4, OPS-16)
`media_routing/pusher.rs::HandlerPusher`: one task per (meeting, handler), meeting-scoped cancel, latest-wins `watch`, confirmed-generation no-op, retry/terminal split, supersede-on-newer, and `PolicyGenerations::adopt_floor` on a classified `generation_mismatch` with `applied > sent` (re-push at K+1). `McError::MediaPolicyDivergence` now carries `applied_generation`.

### Telemetry (D7)
New: `mc_media_receive_slot_cap` (set in `WebTransportServer::new` from the enforced field), `mc_media_slot_view_emissions_total{outcome=sent|deferred|delivery_failed|composition_failed}`, `mc_media_unreachable_senders_total`, `mc_media_handler_set_divergence_total`; retired `mc_media_unmatched_plan_slots_total` (+ tombstone). Catalog, dashboard (4 new panels, 4 edited, 1 retitled), alert description, `alerts.md`, runbook (client-signalling section, Scenario 15, new Scenario 18 incl. the MC-restart reload arm) updated. Placement INFO log at join.

### Client (Lead ruling 1; author paired-client unless noted)
sdk-core receive loop opens from `StreamAssignment.media_handler_url`; hop monitor keyed on (slot, sender) with RFC-1982 resets. Implementer: `media-loopback.spec.ts` → solo-hears-nothing (`fewer_sources` positive control + flat counters), shared `expectCountersFlatOverWindow`, e2e README.

### Additional Changes
- `mc-test-utils/src/mock_mh.rs`: `EchoUpTo(n)` behaviour; stub echoes `UNSPECIFIED` transport for an empty snapshot (real-MH fidelity).
- `infra/services/mh-service/configmap.yaml`: per-handler egress bound formula, candidate bound "rebinds in story 5", muted-sources derivation corrected (it rested on the reflexive self-edge).
- `docs/TODO.md`: two entries resolved (planned-slot rejection; mute-path unqueryable failures); co-location capacity gap filed.

---

## Files Modified

```
65 files changed, 7657 insertions(+), 4826 deletions(-)   (git diff --stat HEAD, new files included)
New: crates/mc-service/src/media_routing/{slots,placement,pusher}.rs, crates/mc-service/src/actors/meeting_media.rs,
     crates/mc-service/tests/slot_placement_integration.rs, crates/mc-service/tests/common/media_session.rs,
     crates/env-tests/tests/27_mc_slot_placement.rs
```

### Key Changes by File
| File | Changes |
|------|---------|
| `crates/mc-service/src/media_routing/slots.rs` | `SlotTable`, `JoinRank`, non-reflexive `subscribes_to`, render; named tests + exhaustive model check (~5k canonical states) |
| `crates/mc-service/src/media_routing/placement.rs` | `MeetingHandlers` (sorted, validated), `HandlerEndpoint`, round-robin `place` |
| `crates/mc-service/src/media_routing/pusher.rs` | per-(meeting, handler) push worker, floor adoption, unit tests |
| `crates/mc-service/src/media_routing/generation.rs` | `adopt_floor` + tests |
| `crates/mc-service/src/media_routing/assignment.rs` | reduced to output types + id packing |
| `crates/mc-service/src/actors/meeting_media.rs` | actor media state: install/admit/remove/declare/reconcile/flush, `MeetingSeams`, `JoinMedia`, `MediaRoutingDeps` |
| `crates/mc-service/src/actors/meeting.rs` | drivers wired at join, leave choke-point, capability, mutes, reconnect; flush arm in the run loop |
| `crates/mc-service/src/webtransport/connection.rs` | handle→Redis→join ordering, scoped `media_servers`, capability → actor, composition/push code removed |
| `crates/mc-service/src/media_signaling/*` | declared slots as input, unreachable parameter, fail-closed url miss, `SlotViewEmission` |
| `crates/mc-service/tests/*` | two-party rewrite of signalling tests; new placement integration suite |
| `crates/env-tests/tests/26_mh_quic.rs`, `27_mc_slot_placement.rs` | loopback test retired; five-participant split-handler env-test |
| docs / dashboard / alerts / runbook / MH configmap | as in Implementation Summary |

### Gate-3 fix round (summary; per-reviewer detail under Code Review Results)
| File | Changes |
|------|---------|
| `media_routing/generation.rs` | `adopt_floor` pairs a number only with the content recorded against it (MH F1); `current()` read accessor for deterministic "no new render" test negatives (test F1) |
| `media_routing/pusher.rs` | superseded-during-adoption arm (MH F1); first-confirm restart-floor discriminator; records exactly one of adoption / `generation_mismatch` (ops + observability); gated `ScriptedMh`; new tests |
| `grpc/mh_client.rs` | `MeetingProgramming::restart_floor_adoptable` + `is_restart_floor`; `confirm` does not record the restart floor; denominator doc |
| `observability/metrics.rs` | new `mc_media_policy_generation_adoptions_total{outcome=adopted\|superseded}` (bounded by `FloorAdoption::ALL`, zero-initialised); gauge rustdoc corrected (G3, G4) |
| `actors/meeting_media.rs` | unreachable-senders counted only after delivery (obs F1); WARN on the three invariant-violation skips (obs F3); `SignalingPayload::server_message` (DRY F3) |
| `actors/messages.rs`, `actors/participant.rs`, `webtransport/connection.rs` | `RAW_SERVER_MESSAGE_TYPE` + `SignalingPayload::server_message` (DRY F3); `ReconnectResult` placed-handler doc (client F2) |
| `media_routing/assignment.rs`, `media_routing/slots.rs` | `egress_ordinal` beside the packer (DRY F2); model check: Reconnect removed, per-handler keys, canonical order, join/leave "earliest" rules (test F5, F6) |
| `media_signaling/mod.rs` | bound-section pointer corrected (DRY F1) |
| `tests/slot_placement_integration.rs`, `tests/media_client_signaling_integration.rs`, `tests/common/{accept_loop_rig,media_session}.rs` | no fixed real-time sleeps: polls for positives, generation-unchanged for negatives (test F1, F2); rapid-redeclare and cap assertions (F3, F4); T8 gap stated (F9); one wait loop (DRY F8) |
| `tests/media_policy_push_integration.rs` | restart-floor classification tests |
| `mc-test-utils/src/mock_mh.rs` | echoes the last APPLIED snapshot's transport mode (MH F3) |
| env-tests (`24`, `26`, `27`, `src/fixtures/`) | shared MC/MH session fixture `fixtures/mc_session.rs` (DRY F6), consumed by 24 as well as 26/27 so no env-tests binary keeps a local framing/join copy (`join_message` added for 24's raw-reply rejection tests); orphans removed (DRY F10); non-ordinal slot ids (test F7, MH F5), with `slot_d > 255` now asserted rather than assumed so the mh-1 arm's u8-truncation claim cannot silently lapse; mh-1 binding arm (MH F2); stale `connect_mc()`/line-number citations in 26 re-pointed |
| `docs/TODO.md` (env-tests entries) | two entries discharged by this change marked `[x] RESOLVED`: "framed `ServerMessage` decode … shared home UNREACHABLE / do NOT promote the dev-dependencies" (its premise is reversed by DRY F6) and "three poll loops … un-unified" (two of the three deleted as orphans, DRY F10) |
| `crates/env-tests/src/fixtures/mc_session.rs` (Gate-2 L7 attempt 1 fix) | **Root cause of the L7 failure — a regression introduced by the fixture extraction.** `mh_connect` returned only the connection and let the MH connect (carrier) bi-stream fall out of scope. Dropping a `wtransport::SendStream` FINISHES it; MH's hold-open loop (`mh-service/src/webtransport/connection.rs`, after the sender binding) reads that FIN as "Client disconnected" and tears the media session down, so every 27 participant was unbound as soon as it was bound, and A received nothing on mh-0. 26 was unaffected because its tests hold the streams returned by `mh_open_connect`. Fix: `mh_connect` now returns `MhSession`, which owns the connection AND both carrier streams for as long as the caller holds it; 27 passes `.connection()`. Checked against the live Kind cluster: 27 passes with the fix, and reinstating the drop reproduces the exact `BINDING … markers seen: {} of 2` panic |
| `crates/dt-guard/src/ts_retained_credentials.rs` (@infrastructure) | latent matcher defect surfaced by paired-client's multi-line `import { AudioPipeline, type AudioPipelineOptions, type AudioSendDirective }` in `MeetingSession.ts`: `DECL_START_RE` read `type AudioPipelineOptions,` as a block declaration, walked into the next brace block and merged its fields into the real `AudioPipelineOptions` → 3 false `auth_state_password_with_token` findings. Fixed at the matcher (`mask_module_syntax`), not worked around in client code; 4 unit tests (3 fail pre-fix + a positive control); re-measured against the real tree: clean, 0 WARN |
| `docs/specialist-knowledge/security/INDEX.md` | sec F1's three added lines folded into one (plus one merge) to meet the 75-line cap; content unchanged |
| main.md §Accepted Deferrals | multi-line paragraph folded into the pointer bullet (`todo-tracking` pointer-only rule) |
| specialist INDEXes (MC, MH, test, DRY, operations, observability, semantic-guard) | 27 + `mc_session.rs` pointers added; "MH datagram round trip / media loopback → 26" re-pointed to 27 (test F8 remainder); deleted `poll_until_instance_above` replaced by `poll_until_any_instance_above` |
| docs / alerts / dashboard / runbooks | adoption counter across catalog, dashboard, rule comment + description, alerts.md, Scenarios 15/18, mc-deployment (Carve-out #3), mh-deployment, mh-incident-response, client-dev-local; TODO.md re-pointed (DRY F5, test F8); security INDEX (sec F1); e2e wording (client F1) |

---

## Devloop Verification Steps

### Gate 2 (Lead, resumed) — final: PASS on the committed tree

Attempt 1 (below) predates the Gate-3 fix round and is superseded. After resume:

- **Attempt 2 — Layer 7 FAIL** (`env-tests-failed`, L7 attempt 1 of 2): `27_mc_slot_placement` BINDING (mh-0) received zero datagrams. Root cause (implementer): the env-tests fixture extraction's `mc_session::mh_connect` dropped both halves of MH's connect bi-stream on return; the send-half FIN makes MH log "Client disconnected" and tear down the media session. Fixed by `MhSession`, which owns the connection plus both stream halves (`connection()` borrows `&self`). Verified: 27 passes alone on Kind; re-introducing the drop reproduces the exact panic. Layers 1-6 were green.
- **Attempt 3 — PASS** (L7 attempt 2 of 2):
```
LAYER=1 RESULT=OK DURATION=5
LAYER=2 RESULT=OK DURATION=4
LAYER=3 RESULT=OK DURATION=99
LAYER=4 RESULT=N/A DURATION=250   (cargo-test OK, nx-test OK; proto test N/A = intentional gap)
LAYER=5 RESULT=OK DURATION=1
LAYER=6 RESULT=N/A DURATION=3     (cargo/pnpm audit OK, buf-breaking OK; proto audit N/A = intentional gap)
LAYER=7 RESULT=OK DURATION=1201   (env-tests-passed incl. 27; browser-e2e-passed)
TOTAL_RESULT=N/A EXIT=0
```

### Gate 2 (Lead) — `DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh`, attempt 1: PASS

```
LAYER=1 RESULT=OK DURATION=4
LAYER=2 RESULT=OK DURATION=4
LAYER=3 RESULT=OK DURATION=98
LAYER=4 RESULT=N/A DURATION=249   (cargo-test OK, nx-test OK; proto test N/A = intentional gap)
LAYER=5 RESULT=OK DURATION=2
LAYER=6 RESULT=N/A DURATION=3     (cargo/pnpm audit OK, buf-breaking OK; proto audit N/A = intentional gap)
LAYER=7 RESULT=OK DURATION=1206   (env-tests-passed incl. 27_mc_slot_placement 1/1; browser-e2e-passed 10/10)
TOTAL_RESULT=N/A EXIT=0
```


### Layer 1: cargo check
**Status**: PASS/FAIL
**Duration**: ~Xs
**Output**: {Any relevant notes}

### Layer 2: cargo fmt
**Status**: PASS/FAIL
**Duration**: ~Xs
**Output**: {Any relevant notes}

### Layer 3: Simple Guards
**Status**: ALL PASS / X FAILED
**Duration**: ~Xs

| Guard | Status |
|-------|--------|
| api-version-check | PASS/FAIL |
| no-hardcoded-secrets | PASS/FAIL |
| no-pii-in-logs | PASS/FAIL |
| no-secrets-in-logs | PASS/FAIL |
| test-coverage | PASS/FAIL |

{Details on any failures}

### Layer 4: Unit Tests
**Status**: PASS/FAIL
**Duration**: ~Xs
**Output**: {Test counts, any failures}

### Layer 5: All Tests (Integration)
**Status**: PASS/FAIL
**Duration**: ~Xs
**Tests**: {X passed, Y failed}

{Details on any failures}

### Layer 6: Clippy
**Status**: PASS/FAIL
**Duration**: ~Xs
**Output**: {Any warnings}

### Layer 7: Env-tests
**Status**: PASS/FAIL
**Duration**: ~Xs
**Output**: {Wall-clock time for dev-cluster rebuild + env-test run; pass/fail summary; log path}

(Semantic-guard relocated to the Gate 2 reviewer panel per ADR-0033 Wave 3 #9. See § Code Review Results → Semantic Guard Reviewer below for its findings.)

---

## Code Review Results

Verdicts as reported by each reviewer. Three reviewers (DRY, paired-test, paired-media-handler)
have open items that all resolve into one in-flight change — the env-tests shared-fixture
extraction — and operations has scoped its verdict to exclude that same change pending a focused
re-review. Nothing is committed.

### Security Specialist
**Verdict**: RESOLVED-FIXED
**Findings**: 3 found, 3 fixed, 0 deferred

- F-SEC-3 (resume, dt-guard matcher): `mask_module_syntax` anchored on a declaration's brace when an import statement shared its line (`import Def from './x.js'; export interface Creds {`), silently dropping a credential-bearing type from the index (fail-open on a guard with no ignore hatch). Fixed by ending the mask at the statement's `;`; pinned by a mutation-checked test; limitation 5 rewritten. Masking cleared across `export type`, `import type`, `export { type X }`, multi-line lists, dynamic import; Phase 2 is unmasked; real tree clean, 9 credential-bearing types unchanged. INDEX merge content-preserving.

- F1: `docs/specialist-knowledge/security/INDEX.md` had only the moved-symbol repoint, leaving the new invariants discoverable only by reading the files they protect. Added three lines: handler-URL scoping through the one frozen `MeetingHandlers` (plus the client's sole dial site), the `MediaConnectionUpdate` redirect prohibition and the test that pins it, and the `#[cfg(test-seams)]`-on-the-type placement-pin seam with its fail-loud out-of-set behaviour.
- F2: the hop-sequence monitor bounded its newly-unverified input against state creation and frame acceptance but not against suppression of its own output (fixed by paired-client).
- Scope-drift check: two Cross-Boundary rows were missing (`actors/participant.rs`, `packages/sdk-core/src/signaling/__tests__/`), found by a mechanical diff of all changed paths against the table. Both added; neither had an ownership consequence.
- Flagged for dependency audit, not a defect: `crates/env-tests/Cargo.toml` moves `wtransport` (with `dangerous-configuration`), proto-gen, prost and bytes from dev-dependencies to dependencies so the lib fixture can use them. No new crate, no version change; env-tests is never shipped.

### Test Specialist
**Verdict**: RESOLVED-FIXED
**Findings**: 9 found, 9 fixed, 0 deferred

- Resume re-review: F7 closed (27 declares `[7,3]`, `[700,3,300]`, `[513]`, `[77]`, `[260]`, ECHO asserts declared ids, `slot_d > 255` positive control); F8 closed (only live pointer, `docs/TODO.md` R-15 entry, re-pointed to 27 BINDING). `24_join_flow.rs` move onto `McSession` verified assertion-preserving.

- F1, F2: every fixed real-time sleep removed. Positives poll until the last push to a handler carries the generation the actor last rendered (`pushes_caught_up`); negatives are deterministic — drain the actor's turn, then assert the rendered generation is UNCHANGED, using a new `PolicyGenerations::current()` exposed on `AcceptLoopRig`. Paused-time sites use a separately named `virtual_turns()`.
- F3: C now declares mid-churn and its view is asserted (no starvation); the positive control asserts MH's LAST snapshot carries A's final two-slot view, and B's mute is asserted to reach A as `SOURCE_MUTED`.
- F4: the cap test runs with a peer present, so an accepted declaration would create an edge; the always-true `>=` assertion is replaced by "no view for anyone, and the rendered generation unchanged".
- F5: `Op::Reconnect` removed from the model check and the tautological half of the unit test deleted, with a pointer to the actor-level reconnect proof.
- F6: model check gained per-handler key coverage, strictly-ascending canonical order, and both "earliest" transition rules (5070 states, 22596 transitions, ~161 ms).
- F9: the T8 doc states the gap — this tier proves one view was sent, not that it was the full current view, because the reconnect path takes no outbound channel here.
- F7 (non-ordinal declared slot ids) and F8 (dangling pointers) are in the in-flight env-tests change; `docs/TODO.md` and the mh-service test doc halves of F8 are already done.

### Observability Specialist
**Verdict**: RESOLVED-FIXED
**Findings**: 8 found, 8 fixed, 0 deferred

- F1: `record_unreachable_senders` moved below the delivery check, so the counter and the `outcome="sent"` denominator of its documented ratio move together.
- F2: the send-directives entry claimed "unconditionally / every composition"; the code records failures unconditionally and successes only on change. Corrected, with the failure-ratio caveat added.
- F3: three silent early returns in `flush_one` are invariant violations (no roster entry, no routing deps, no view) — now a WARN with a bounded `reason`. The two routine skips stay uncounted and are named in the catalog's increment boundary.
- F4, G1-G3: stale pointers and consumers corrected (`WebTransportServer::new`, the identity-rules scope, both `client-dev-local.md` outcome-split rungs, `mh-deployment.md`, and the gauge rustdoc's "written ONLY on a registration push" framing).
- **The alert-shape decision** (raised by operations as OPS-R1(a), ruled by observability): this task's floor adoption would have paged `MCMediaGenerationDivergence` on every MC rollout with live meetings. A PromQL subtraction was rejected as fail-open — a mismatch outside the 15m window with its adoption inside lets a genuine unrelated divergence net to zero. Landed instead as classification-at-source: a restart-floor reply on a (meeting, handler)'s FIRST confirm is recorded on the new `mc_media_policy_generation_adoptions_total`, never as `generation_mismatch`. The alert `expr`, `for: 0m`, `alerts.md` and the `mc-deployment.md` deploy gate are all unchanged. A failed adoption, and any higher echo after a confirm (the `u64::MAX` ratchet wedge), still page.
- G4: that counter had two populations under one name. Now `outcome={adopted,superseded}`, bounded by `FloorAdoption::ALL` — `adopted` is the recovery event (at most one per (meeting, handler) per MC process), `superseded` is routine coalescing with no such bound.

### Code Quality Reviewer
**Verdict**: CLEAR
**Findings**: 0 found

No production panics, typed errors, single `subscribes_to` predicate, SSoT for the slot-cap gauge / policy generation / handler URLs, no dangling references from the removed story-1 symbols, `test-seams` compile_error needle intact, no proto/GSA path touched.

### DRY Reviewer
**Verdict**: RESOLVED-DEFERRED (all 11 findings fixed; DEFERRED only because one ADR-0019 extraction opportunity was filed to `docs/TODO.md`)

- Resume re-review: F6, F10 closed (zero local copies of the MC/MH client plumbing in 24/26/27; orphaned polls deleted). F11 (new, fixed by reviewer): verbatim 20-line "any registered MH url" block in `26_mh_quic.rs` extracted to `any_registered_mh_url`. Filed: env-tests per-binary bootstrap trio (`docs/TODO.md` §Cross-Service Duplication, "From DRY Reviewer — story 2 task 6").

**True duplication findings** (entered fix-or-defer flow): 10 found, 8 fixed, 2 in flight, 0 deferred.
- F1: the bound-criterion section pointed at the wrong module and claimed a back-pointer that did not exist — module path corrected and the back-pointer added.
- F2: `egress_stream_id`'s pack layout was restated as a bare `0xFF` in the model check — `egress_ordinal` now sits beside the packer and is round-tripped in the packer's own test.
- F3: the participant-actor send envelope was implemented twice with `message_type: 0` at three sites — `SignalingPayload::server_message` + `RAW_SERVER_MESSAGE_TYPE`, with each caller keeping its own (legitimately different) failure accounting.
- F4: MH's test doc carried the retired loopback premise (fixed by paired-media-handler, in their own crate).
- F5: a live `docs/TODO.md` entry cited the deleted env-test as its evidence — re-pointed at 27's BINDING phase; the entry's status is unchanged.
- F7, F9: `2 ** 31` encoded three times, and a value computed twice in one try block (fixed by paired-client, who also found two further restatements).
- F8: the two `*_until` wait loops now delegate to one; the settle-bound sprawl was resolved by removing fixed sleeps entirely rather than routing them through one helper.
- F6 (lift the duplicated WebTransport/MC-join client into `crates/env-tests/src/fixtures/`, consumed by both 26 and 27) and F10 (orphaned metrics polls) are the in-flight change. F6 was accepted rather than argued down: "separate binaries can't share" is wrong here, and `27` already imported from that module.

**Extraction opportunities** (appended to `docs/TODO.md`): none added by this loop beyond the two entries already filed in Implementation Summary; the reviewer's noted opportunities (`start_stack`/`connect_client` copies, the 11 `cluster()` copies) are pre-existing and were filed by the reviewer, not against this diff.

### Operations Reviewer
**Verdict**: RESOLVED-FIXED (unscoped; its condition — a green Layer 7 on the final tree — met by Gate 2 attempt 3)
**Findings**: 10 found, 10 fixed, 0 deferred

- Resume re-review (5, fixed by reviewer): `McSession::try_read` bounds the body read too and panics on a partial frame; 27 `bind_until_received` no longer discards send errors or folds a closed receiver session into "nothing yet"; `crates/env-tests/Cargo.toml` states the real control keeping `dangerous-configuration` out of service images (per-package release builds), `docs/TODO.md` baseline corrected; `TRIAGE_HANDLER_SET` literal + `docs/runbooks/devloop-validation.md` §8 row + `client-dev-local.md` pointer; `docs/TODO.md` env-test enumeration 16 of 16. `MhSession` reviewed: no findings.

- OPS-1 (Gate 1): the `test-seams` placement override cannot exist in the `--release` Kind image the env-test targets — the env-test observes placement instead of pinning it, and hard-fails with an ENVIRONMENT message if the cluster has only one healthy MH.
- OPS-16 (Gate 1, blocking): generation recovery could not converge after an MC restart — a page whose documented remedy was a no-op. Fixed by `adopt_floor` + K+1 re-push, gated on a classified reply.
- OPS-R1: (a) the benign adoption paged on every MC rollout — see the observability entry; (b) no rollback carve-out for the hazard this change creates — Carve-out #3 added, with the bounded blast radius and the one case where restarting MH IS the remedy; (c) two stale post-deploy claims corrected.
- OPS-R2: `§Rollback Procedure` filled in with the real non-`git reset` hazard.
- OPS-R3: a partial invariant — the retired "divergence does not self-correct" premise survived at three of five sites. All corrected, and the two triage rungs that instruct an `outcome` split now name the adoption counter, since that arm is deliberately under no outcome label.

### Semantic Guard Reviewer
**Verdict**: CLEAR
**Native verdict**: SAFE
**Findings**: 0 found

Checked: Credential Leak; Client Credential Lifetime; Actor Blocking; Error Context Preservation; Metrics Path Completeness. No findings — the MC→MH `RegisterMeetingRequest` carries no key material (pinned by an in-tree test), the meeting actor's loop awaits no I/O (every push runs in a per-(meeting, handler) task), added `map_err`s carry their source, and every emission-bearing exit records.

### Paired Media Handler Specialist
**Verdict**: RESOLVED-FIXED
**Findings**: 4 found, 4 fixed, 0 deferred

F1 verified: `adopt_floor` pairs a generation only with its recorded content and the pusher abandons a superseded in-flight push; `is_restart_floor` is one fail-closed classifier shared by confirm and pusher. F3 verified: `mock_mh` echoes the last applied snapshot's transport mode. F2/F5 verified in env-test 27: the mh-1 binding arm proves the second handler is programmed at MH, and non-ordinal slot ids (with `slot_d > 255` and an ECHO check) make the wire stream_id / u8-truncation checks non-vacuous.

### Paired Infrastructure (guard-machinery owner, `crates/dt-guard/src/ts_retained_credentials.rs`)
**Verdict**: RESOLVED-FIXED
**Findings**: 3 found, 3 fixed, 0 deferred
**Ownership Lens**: Domain-judgment, owner infrastructure, owner in review; not a GSA; no policy/vocabulary content changed (`git diff` over `common/pii_vocabulary.rs` and `scripts/guards/**` empty).

- `IMPORT_STMT_RE` matched `import(` / `import.meta` expressions, masking a nested callback body and truncating an enclosing class's member walk (false negative). Fixed to `^\s*import\s*\{|^\s*import\s+[A-Za-z_$*]`, pinned by `dynamic_import_expressions_are_not_masked_as_module_syntax`.
- Accepted security's statement-side twin (F-SEC-3) as machinery owner.
- A regression assertion (`!is_credential_bearing("B")`) could not fail; replaced with masker-level assertions on the discriminating input. `cargo test -p dt-guard` 633 pass, clippy clean, live guard clean with no WARN.
- Process note: cargo linked a stale `dt_guard` rlib after a source edit — `touch` the source before trusting a green run on this crate.

### Gate 3 — Final Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred |
|----------|---------|----------|-------|----------|
| Security | RESOLVED-FIXED | 3 | 3 | 0 |
| Test (paired-test) | RESOLVED-FIXED | 9 | 9 | 0 |
| Observability | RESOLVED-FIXED | 8 | 8 | 0 |
| Code Quality | CLEAR | 0 | 0 | 0 |
| DRY | RESOLVED-DEFERRED | 11 | 11 | 0 (+1 extraction opportunity filed) |
| Operations | RESOLVED-FIXED | 10 | 10 | 0 |
| Semantic Guard | CLEAR | 0 | 0 | 0 |
| Paired Client | RESOLVED-FIXED | 8 | 8 | 0 |
| Paired Media Handler | RESOLVED-FIXED | 4 | 4 | 0 |
| Paired Infrastructure (dt-guard) | RESOLVED-FIXED | 3 | 3 | 0 |

Semantic Guard's CLEAR predates the resume-round edits (env-tests fixture, dt-guard matcher). Neither touches a production check surface: env-tests is a test crate and dt-guard is guard tooling. Security reviewed the dt-guard change with the credential lens.

### Paired Client Specialist
**Verdict**: RESOLVED-FIXED
**Findings**: 8 found, 8 fixed, 0 deferred

The MC wire shape is what the SDK expects on every path checked. The most serious item was in the client's own earlier code and was found while pairing on this task: MC sends a publisher nobody holds a `SendDirective` with NO streams (a specified success), and the SDK returned early instead of withdrawing the send instruction — which made one of only three transmit-key rotation sites unreachable. Security traced the consequence as forward secrecy, not a stale send: a publisher resumed under a key generation a DEPARTED holder still held. Fixed, with a test that fails without it.

---

### Implementer second review of paired-client's sdk-core hunks (as agreed at Gate 1)
Verdict: sound. Findings sent to @paired-client, owner decides fix/keep:
- F1 (minor): `AudioPipeline#fault` dedupes by stage, and all three new receive faults share `MediaFaultStage.Transport`, so the first one masks the others (e.g. "not connected" hides a later "moved mid-session", the MC-placement-defect signal).
- F2 (minor): after a slot refill, in-flight frames from the previous sender flip the hop mark back; each flip is a silent baseline reset, so loss inside that short window is uncounted — document or guard.
- No finding: ingress parse-then-observe ordering, the sole-dial-site comment, and the `streamAssignments` subscription placement.

Outcome (paired-client, owner):
- F1: FIXED by paired-client.
- F2: KEPT, by the owner's decision (paired-client's rationale is in their review record).
- Additional client fix found while pairing on the no-streams directive: a `SendDirective` with NO streams now withdraws any earlier send instruction, instead of returning early. The consequence is forward secrecy, not just a stale send. Without the fix, a publisher resumed under a transmit-key generation that a DEPARTED holder still held, so that holder could decrypt the resumed media. The comment leads with that. Tested with the `framedSendDirectiveWithoutStreams` builder in `packages/sdk-core/src/signaling/__tests__/helpers.ts`.

---

## Accepted Deferrals

- DRY extraction opportunity (not a deferred finding): env-tests per-binary bootstrap trio → `docs/TODO.md` §Cross-Service Duplication, "From DRY Reviewer — story 2 task 6 (2026-09-24)".
- No findings deferred — every Gate-1/Gate-3 finding across all nine reviewers was fixed in-loop; the two `docs/TODO.md` entries this loop ADDS (co-location capacity gap; client signalling-loss/reconnect, authored by the Lead) are newly-surfaced gaps, not deferred findings, and are recorded in the Implementation Summary.

---

## Rollback Procedure

1. Start commit: `58f323aa58af4d1b5b9ec647a39da7baed2bd65f` (the working tree at loop start; nothing in this loop is committed until the Lead commits it).
2. `git diff 58f323aa..HEAD`; `git reset --soft|--hard 58f323aa`. No schema migrations, no ConfigMap keys added (the MH configmap edits are comments only), no manifests to delete.
3. **A repo revert is not a deployment rollback, and the deployment rollback has one hazard `git reset` does not show (OPS-R1(b))**: MH's routing table is install-only and survives an MC rollback. A rolled-back MC (without `adopt_floor` and without structural re-push) restarts at generation 1 and pushes once per meeting's first join; MH, still holding the newer generation K, rejects 1 as stale forever. **Every meeting alive across the rollback stays media-dark.**
4. **Bounded blast radius**: only meetings alive on MH across the rollback. A new `meeting_id` has no MH entry (installed 0), so generation 1 applies — new meetings are unaffected.
5. **Remedy — the one exception to "DO NOT RESTART MH"**: restart the MH pods (clearing the install-only tables) or roll MC forward again; affected participants reload. Recorded as Carve-out #3 in `docs/runbooks/mc-deployment.md` §Post-Deploy Monitoring Checklist: MC↔MH Coordination, with a pointer from §How to Rollback.
6. Client coupling: the sdk-core hunks (paired-client) consume per-slot `StreamAssignments` and the scoped single-entry `media_servers`; roll MC and the SDK together per the existing "a deployment rollback must revert MC and the SDK together" note in `mc-deployment.md`.

---

## Issues Encountered & Resolutions

### Issue 1: The story-1 loopback env-test was the only end-to-end datagram proof, and R-3 deletes its premise
**Problem**: `26_mh_quic.rs::test_mh_forwards_an_audio_datagram_back_to_its_sender` proved R-15 end to end by having one participant hear itself. R-3 makes `subscribes_to` non-reflexive, so the self-edge it depended on no longer exists — and its `assert_eq!(view.stream_id(), 0)` had passed for any implementation anyway, because a single participant's own slot coincided with `MAIN_AUDIO_SLOT_ID`. Deleting it without a replacement would have left the media path with no in-cluster datagram evidence; keeping it would have required preserving a self-edge the story forbids.
**Resolution**: replaced, not dropped. `27_mc_slot_placement.rs`'s BINDING phase drives two distinct senders into one subscriber and asserts each frame arrives on the slot id that subscriber's OWN `StreamAssignments` named for that sender, read off the wire. Three artifacts citing the retired test were re-pointed (`docs/TODO.md`, MH's `media_session_binding_integration.rs` module doc, and the MH INDEX), each found by a citation sweep rather than by a guard — a citation whose target no longer exists is invisible to every check in the tree.

### Issue 2: A correct fix to a real page would have made the page fire on every deploy
**Problem**: OPS-16's floor adoption is the right recovery for an MC restart under a live meeting, but the reply it recovers from was classified `generation_mismatch`, which is `MCMediaGenerationDivergence`'s subject at `for: 0m` on `> 0`, severity page. So the fix would have paged once per surviving meeting on every MC rollout — worse than a noisy gate, because it trains oncall to wave through the page whose real subject is a handler running no policy at all.
**Resolution**: the first proposal (subtract an adoptions counter from the alert expression) was rejected on review as fail-open — a mismatch falling outside the 15m window with its paired adoption inside lets a genuine, unrelated divergence net to zero and not fire. Landed instead as classification at source: a restart-floor reply on a (meeting, handler)'s FIRST confirm is recorded on `mc_media_policy_generation_adoptions_total` and never as a mismatch. The alert expression, `for: 0m`, `alerts.md` and the deploy gate are all untouched. The discriminator is fail-closed (`first_confirm && !floor_adopted`, one predicate shared by `confirm` and the pusher), so a failed adoption and the `u64::MAX` ratchet wedge both still page.

### Issue 3: A generation could be paired with content it was never recorded against
**Problem**: found at Gate 3 by @paired-media-handler, in the adoption path added earlier in the same loop. `adopt_floor` returned the registry's existing number whenever it already exceeded MH's applied generation, without checking that the number was recorded against the content being pushed. Reachable in exactly the scenario adoption exists for: MC restarts, the worker is mid-flight with job (1, A), the actor renders B, C and D meanwhile, MH echoes 3, adoption returns 4 — and A is installed under 4. The worker then records `confirmed = 4`, so the real (4, D) is skipped as already-confirmed, and MH forwards a stale topology while every signal reads `match`.
**Resolution**: `adopt_floor` now reuses a recorded number only when its recorded assignment is the one being pushed; otherwise it allocates above both and records the given content. The pusher additionally abandons the push when a newer render is already published, so stale content is never installed even transiently. Both are pinned by tests, and the second was mutation-checked.

### Issue 4: A counter merged a rare recovery event with a routine one, under the recovery event's name
**Problem**: the adoptions counter had two call sites — a successful adoption, and a restart-floor reply discarded because a newer render superseded it. Counting the second is required (it is a reply deliberately not recorded as a mismatch, so omitting it would break the `pushes + adoptions` reconstruction), but merging them made two documented claims false, including a deploy-checklist bound of "one per (meeting, handler)" that the superseded path does not respect.
**Resolution**: a bounded `outcome={adopted,superseded}` label, `FloorAdoption::ALL`, zero-initialised for both values. The reconstruction is unchanged because it sums both. **This surfaced only because the mechanism was volunteered in a status message — it existed in the code and in no document**, which is why the per-disposition explanation now lives in the enum's variant docs rather than in a shared sentence.

---

## Lessons Learned

1. **A citation is not a reference, and nothing in the tree checks it.** This task deleted a test, a constant and four symbols, and each had prose elsewhere asserting what they proved. Compilation catches the code; nothing catches a runbook, a TODO entry or a module doc that names them. The sweep has to be a deliberate step keyed on the retired PREMISE, not on the symbol — "divergence does not self-correct" survived at five sites under three different wordings.
2. **When a classification changes, "what prose asserts the old behaviour" and "what queries depend on the old shape" are different searches.** Two reviewers swept independently, one for retired premises and one for stale consumers of the outcome breakdown. Neither needle found the other's catches, and the true scope was ten sites when each had confidently found six.
3. **A correction that is exact in the common case can still be fail-open.** The PromQL subtraction was arithmetically right and wrong as a design: it made the page's existence conditional on a second series, and netted genuine divergences to zero at window boundaries. Classifying at the source removed the arithmetic instead of guarding it — cheaper in artifacts touched, and with no residual to document.
4. **Test negatives should be deterministic, not timed.** Replacing "sleep, then assert nothing was pushed" with "drain the actor's turn, then assert the rendered generation is unchanged" removed a class of false-pass that a longer sleep only makes rarer. It needed a two-line read accessor on the registry, which is a low price for negatives that cannot lie.
5. **An assertion whose expected value coincides with an ordinal proves nothing.** Both the retired loopback test and the new env-test's first draft declared slot ids equal to their positions, so "MC wrote the declared slot id" and "MC wrote the ordinal" were indistinguishable. Two reviewers found this independently, in two different files, which suggests the fixture default is the hazard rather than either author.
6. **Reviewers' asks get implemented verbatim, so a wrong ask costs more than a wrong finding.** Three corrections in this loop were reviewers withdrawing their own claims. Each was caught by a different specialist, and the loop was better for the withdrawals being written down rather than quietly dropped.

---

## Appendix: Verification Commands

```bash
# Commands used for verification
./scripts/verify-completion.sh --layer full

# Individual steps
cargo check --workspace
cargo fmt --all --check
./scripts/guards/run-guards.sh
DATABASE_URL=... cargo test --workspace
DATABASE_URL=... cargo clippy --workspace --lib --bins -- -D warnings
./scripts/guards/semantic/credential-leak.sh path/to/file.rs
```
