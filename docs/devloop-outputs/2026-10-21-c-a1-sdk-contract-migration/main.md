# Devloop Output: Story 3 C-A1 — SDK contract migration

**Date**: 2026-10-21
**Task**: Story 3 task 11 (C-A1): SDK join publish set + per-kind receive declaration, flat slot states, per-kind mute & send directives, MC send-id stamping, uni-stream transport seam + MH control channel, slot env-var rename
**Specialist**: client
**Mode**: Agent Teams (v2) — full, Gate-1 present <!-- panel mode + Gate-1 tier (ADR-0037 §D2); see the Tier row in Loop State -->
**Branch**: `feature/user-see-each-other`
**Duration**: ~3h (setup → commit)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `90b656a9673500c41e75391231ec95bc7b58f97b` |
| Branch | `feature/user-see-each-other` |
| Lead Model | `claude-opus-5-5[1m]` |

---

## Loop State (Internal)

<!-- This section is maintained by the Lead for state recovery after interruption. -->
<!-- Do not edit manually - the Lead updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `spawned (implementer)` |
| Implementing Specialist | `client` |
| Tier | `full` |
| Iteration | `1` |
| Security | `spawned (security)` |
| Test | `spawned (test)` |
| Observability | `spawned (observability)` |
| Code Quality | `spawned (code-quality)` |
| DRY | `spawned (dry)` |
| Operations | `spawned (operations)` |
| Semantic Guard | `spawned (semantic-guard)` |

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
Story 3 task 11 (C-A1): migrate the SDK to the story-3 wire contract (join publish set + per-kind receive declaration, flat slot states, per-kind mute & directives, MC send-id stamping, uni-stream seam + MH control channel, slot env-var rename) while staying green against the pre-MC-A controller via marked transitional shims (a), (b), (c).

### Scope
- **Service(s)**: client (sdk-core, sdk-svelte, web-app, test-utils); dev-web.sh; client docs/dashboard/collector allowlist
- **Schema**: No
- **Cross-cutting**: Yes (observability catalog/collector/dashboard, ops runbooks, infra dev script — all Minor-judgment, review-only)

### Debate Decision
NOT NEEDED - design fixed by the story file's Design §Wire freeze and ADR-0036.

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
| `packages/sdk-core/src/**` | Mine | — |
| `packages/sdk-svelte/src/**` | Mine | — |
| `packages/web-app/src/**` | Mine | — |
| `packages/web-app/tests/**` | Mine | — |
| `packages/web-app/e2e/**` | Mine | — |
| `packages/web-app/vite-env.d.ts` | Mine | — |
| `packages/web-app/playwright.config.ts` | Mine | — |
| `packages/test-utils/src/**` | Mine | — |
| `docs/specialist-knowledge/client/INDEX.md` | Mine | — |
| `scripts/dev-web.sh` | Not mine, Minor-judgment | infrastructure |
| `scripts/dev-web.test.sh` | Not mine, Minor-judgment | infrastructure |
| `docs/observability/metrics/client.md` | Not mine, Minor-judgment | observability |
| `docs/observability/metrics/mc-service.md` | Not mine, Minor-judgment | observability |
| `infra/grafana/dashboards/client-media.json` | Not mine, Minor-judgment | observability |
| `infra/services/otel-collector/collector.yaml` | Not mine, Minor-judgment | observability |
| `docs/runbooks/client-dev-local.md` | Not mine, Minor-judgment | operations |
| `docs/runbooks/mc-incident-response.md` | Not mine, Minor-judgment | operations |
| `docs/user-stories/2026-10-07-see-each-other.md` | Not mine, Minor-judgment | observability |


---

## Planning

### Mechanism restatement

The SDK's client↔MC and client↔MH contracts move from "one kind, one total slot count, one mute bit" to "per kind everywhere": the join carries what I publish (a fixed set) and what I receive (N audio + M video slot ids); every slot reports one flat state; every mute and directive is per kind; uplink relay ids come from MC's send id; the MH bidi stream becomes two-way. The wider class is "every place the SDK assumes audio-only or a total slot count": mute (MuteState, MuteRequest, ServerMuteRequest, ParticipantMuteUpdate projection), slot declaration (ids, diagnostics, caps, env vars, dev-web, docs), slot state (union, web-app table, e2e oracle, specs), directive (listener, faults). All of them are in this plan. Siblings outside my files (MC's `slot_state_label`, MC's directive builder, MC caps) are MC-A/MC-B's and are not touched.

### Escalations for @team-lead (each needs a ruling before I implement; my recommendation in bold)

- **E1 — the live pre-MC-A MC spells "nobody watches you" as `SendDirective{streams: []}`** (`crates/mc-service/src/media_signaling/directive.rs::build_send_directive` omits unwatched streams; `solo-participant.spec.ts` asserts exactly this). The prompt's rule "a published kind MISSING from `streams` is a protocol fault, never a withdrawal" would make every story-2 solo/last-holder case raise a `mediaFault` (rendered in the UI) and a fault counter against the controller live at this gate. **Recommend a third marked shim (c), deleted by C-A2 with the other two: a directive whose `streams` list is EMPTY is read as the pre-MC-A withdrawal spelling for every published kind (empty targets, rotation-on-resume preserved, no fault); a NON-empty `streams` missing a published kind is the counted, typed fault.** In both cases the missing kind's send state fails CLOSED (targets = []) so the stale target set is never kept (the forward-secrecy argument in `mediaWiring.ts`); "never read as a withdrawal" is honoured as: counted + faulted, not silently accepted. Post-MC-A, MC always lists every published kind, so (c) is dead code until C-A2 deletes it.
- **E2 — the SDK has no reconnect path** (`MeetingSession` is single-use per join; `SignalingClient` stores correlation/binding "for future reconnection (storage only)"). "The reconnect path re-sends the ORIGINAL set" has nothing to attach to. **Recommend: the publish set and slot counts are copied + frozen into dedicated immutable session fields at construction (not the options object), the one JoinRequest builder reads only those fields, and `SignalingClient` stores the frozen publish set beside correlation/binding for the future reconnect. Tests: join carries it; mutating the caller's array after construction changes nothing; the stored set is frozen.** No reconnect is invented.
- **E3 — MC-A ordering hazard I am fixing in-scope.** After MC-A, MC answers the JOIN-carried declaration with `StreamAssignments` + `SendDirective` immediately after `JoinResponse`, before `startMedia()` subscribes; shim (a)'s identical redeclaration is then `accepted_unchanged` and MC sends NOTHING, so a pipeline started later would never see a directive or assignment (story-2 specs would fail at MC-A's gate). Fix: the session caches the latest `sendDirective` and `streamAssignments` from signaling creation and applies them when the pipeline is built. No new behaviour, just ordering-robustness.
- **E4 — observability rulings I am adopting (taxonomy owner):** capture failures ship as `dt_client_media_capture_failures_total{media_kind, outcome}` (not `reason`; label-taxonomy reserves `reason` for frame-drop families), and the story file's `{reason}` mentions for this series are updated in this change. Keyframe-request zero-init: see Telemetry below — per-kind outcome vocabulary declared now, NOT zero-initialised (no `media_kind="video"` series before video flows; audio has no keyframes); the kind-free session counter is zero-initialised. This deviates from the prompt's "zero-initialised" wording; recorded here.
- **E5 — capture-failure outcome set.** The prompt lists four values "from the existing CaptureFailure vocabulary". The vocabulary has six (`permission_denied`, `no_device`, `device_unavailable`, `unsupported`, `platform_unsupported`, `unknown`). **Recommend deriving the label set from the vocabulary (SSoT, per dry-reviewer) plus `ended` = 7 values**; dropping three would make audio failures with those causes uncountable.
- **E6 — SDK default M.** **Recommend `DEFAULT_RECEIVE_VIDEO_SLOTS = 0`** in `clientConfig.ts` until a receive path exists (C-B); dev-web exports M=3 explicitly, playwright pins `VITE_DT_VIDEO_RECEIVE_SLOTS=0` (story-2 specs stay audio-only, R-33 "story-2 audio specs green with M=0"; pre-MC-A MC counts video slots against its retired TOTAL cap and would also emit non-active assignments for them).

### Lead rulings on E1–E6 (verbatim, team-lead = main)
E1 APPROVED: shim (c) as you propose. Mark it like (a)/(b), naming C-A2 (task 13) as the deletion owner. Empty `streams` = pre-MC-A withdrawal, with no fault. A non-empty list missing a published kind = typed fault + counter. The missing kind ALWAYS fails closed (targets=[]). Add a test for each branch. Write the dependency into the catalog row, as observability asked.
E2 APPROVED: frozen fields + one JoinRequest builder + immutability tests. Do not invent a reconnect path. State in main.md that the "reconnect re-sends" requirement has no attach point yet, and that the frozen set is its future source.
E3 APPROVED (in scope): cache the latest directive/assignments and apply them when the pipeline is built. Test it.
E4 APPROVED per the taxonomy owner's ruling: `outcome` not `reason`. Keyframe per-kind outcomes are declared but not zero-initialised; the kind-free unattributed counter IS zero-initialised. Update the story file mentions to match. This changes the wording of the prompt's security criterion "capture_failures{reason}" (the label is renamed; nothing is weakened), so note it for @security.
E5 APPROVED: the full CaptureFailure vocabulary + `ended`.
E6 APPROVED: SDK default M=0, dev-web M=3, playwright pins video=0.

**E2 note:** the prompt's "the reconnect path re-sends the ORIGINAL set" has NO attach point in the SDK today (no reconnect path exists; `MeetingSession` is single-use per join). The frozen publish set held on the session and stored on `SignalingClient` beside correlation/binding is that future path's source.

### Gate-1 notes folded (security S1-S3, Q2; code-reviewer 1-8)
- S1 capture-gate invariant, written in `serverMuteResume.ts`'s header with C-A2 named as the task that must keep it: only the effective target set plus client mute withhold/resume capture or send for a kind; a server-mute `ParticipantMuteUpdate` never stops or starts capture by itself. Test: self update(false) with non-empty targets changes no capture/egress state and emits nothing.
- S2: the machine is fed the EFFECTIVE send state after validation (kind-missing fault, header-version refusal and shim (c) all count as empty), and is subscribed to self mute updates and directives from signaling creation, not `startMedia()`. Before media starts the effective state is empty. Updates for other participants never touch the self latch (tested). Test: latched → kind-missing fail-closed → valid non-empty ⇒ exactly one event.
- S3: unwatched→watched with no latch ⇒ no event (tested beside the residual test).
- Q2 (host server-mute, unchanged kind): "known false" after join. MC replays, right after `JoinResponse`, a `ParticipantMuteUpdate` with BOTH kind flags for every other participant server-muted in EITHER kind (`crates/mc-service/src/actors/meeting.rs::server_mute_replay`, written at `webtransport/connection.rs` step 8b), and broadcasts every later change. So a participant with no update is server-unmuted in both kinds.
- Code-reviewer: each shim is one deletable unit marked `TRANSITIONAL (C-A1 shim x) — deleted by C-A2 (task 13)`. Shim (b) lives in `signaling/legacySourceMutedShim.ts` (a pure mapping function plus a small holder; SignalingClient makes one or two calls into it). Shim (c) is one named function in `mediaWiring.ts`.
- The E3 holder subscribes first, then replays the cached value once at pipeline build; a test covers a live update arriving after the replay.
- An `undefined` stamp means a typed fault and nothing sent. The 8-bit bound is derived from an existing key-id constant (cite the `SendStream.stream_number` comment).
- Naming: `explicitMuteFields(state)`, `isServerMuteRecorded`.
- The test-utils IWebTransport copy points to the canonical prose.
- Each outcome vocabulary is one `as const` tuple with a derived union.
- `serverMuteResume.ts` has an explicit switch transition table, with the BY-DESIGN residual comment at that code path.

### (1) Send-id stamping (R-20)
- New `packages/sdk-core/src/media/pipeline/sendId.ts`: `uplinkRelayStreamId(stream: { readonly streamNumber: number }): number | undefined` — the ONE stamping function; returns the directive's `SendStream.stream_number` when it is an integer in the 8-bit key-id domain, else `undefined` (caller faults, sends nothing). Exported for C-A2's video egress.
- `AudioPipeline.#applyDirective` (today `streamId: declaredSlotIds[0] ?? 0`) calls it with the directive. Test: slots 0..2 declared, directive `stream_number` 7 → every uplink frame's relay `stream_id` is 7; a second test asserts no declared slot id ever appears.

### (2) Join and declaration (R-1, R-2)
- `MeetingSessionOptions`: `publish?: readonly PublishKind[]` (default `['audio']`), `audioReceiveSlots?: ParsedReceiveSlots`, `videoReceiveSlots?: ParsedReceiveSlots` (replaces `receiveSlots`). `MediaConfig.receive` gains `videoSlots` (default 0, E6); `audioSlots`/`videoSlots` validated `>= 0`.
- Constructor validation, typed `SignalingError` codes (new): `PublishKindNotSupported` ("video publishing is not yet supported by this SDK build" — a `video` entry, or duplicates/unknown values) and `NothingToPublishOrReceive` (publish set empty AND N+M = 0; UX only, MC is the authority). Copy in `web-app/src/lib/errorText.ts`, bounded fields only.
- `SignalingJoinParams` gains `publishKinds` and `receiveCapability`; `SignalingClient` maps them to `JoinRequest.publish_kinds` (closed `MEDIA_KIND_AUDIO` / `MEDIA_KIND_VIDEO_CAMERA`) and `receive_capability` (audio ids `0..N-1` then video `N..N+M-1`, id order). One builder `session/mediaSelection.ts:receiveDeclaration(n, m)` produces the list used by BOTH the join and shim (a).
- Shim (a): `startMedia()` still sends the post-join `ReceiveCapability`, identical (same builder output) — marked `TRANSITIONAL (C-A1 shim a) — deleted by C-A2`.
- `JoinedEvent` gains `maxAudioReceiveSlots` / `maxVideoReceiveSlots` (tags 13/14, `number | undefined`, never coerced; absent = unknown, 0 legal = "video disabled"). Tag 12 no longer read (`maxReceiveSlots` removed from `JoinedEvent`).
- `MeetingSession.receiveSlots` → `{ publish, audio: {declared, source, serverCap}, video: {declared, source, serverCap} }`, `serverCap: {state:'known', value} | {state:'unknown'}` (no `invalid` state: 0 is legal per the proto).
- RETIRE: `receiveSlotIdsToDeclare`'s throw, `classifySlotCap`'s invalid branch, `SignalingErrorCode.ReceiveSlotsOverCap` (+ `errorCodeMap`, errorText), `MediaKindMetrics.receiveSlotsRejected`, `dt_client_media_receive_slots_rejected_total` everywhere (`grep -rn receive_slots_rejected`): catalog rows (client.md :396 table, :1216 deficit blind-spot, :1220 entry), collector allowlist :93 + budget comment :412 (recomputed), dashboard panel (client-media.json :1222/:1301, gridPos re-checked), runbooks (client-dev-local F18 ~1556-1614 incl. :1597; mc-incident-response :1992, :3191-3200), `mc-service.md:676` triage sentence. Replacement test: the counter is never emitted on an over-cap declaration.

### (3) Env-var rename
- `VITE_DT_AUDIO_RECEIVE_SLOTS` / `VITE_DT_VIDEO_RECEIVE_SLOTS`, parsed by `parseReceiveSlots` (now accepts 0, still strict integer, no fallback). `VITE_DT_RECEIVE_SLOTS` set ⇒ typed throw from web-app `config.ts`; the message is ONE exported constant in `clientConfig.ts` (`RETIRED_RECEIVE_SLOTS_MESSAGE`) naming both new variables; web-app config and its test assert that constant.
- Readers/writers: `clientConfig.ts`, `session/events.ts`, web-app `config.ts`, `session.ts`, `testLevers.ts` (lever `receiveSlots` → `audioReceiveSlots` + `videoReceiveSlots`; `publish` lever stays C-A2's), `errorText.ts`, `vite-env.d.ts` (old name kept as optional only so the loud check can read it), `playwright.config.ts` (`VITE_DT_AUDIO_RECEIVE_SLOTS=SUITE_AUDIO_RECEIVE_SLOTS`, `VITE_DT_VIDEO_RECEIVE_SLOTS=0`), `e2e/{fixtures,cohort,cohortContexts,over-subscription.spec}.ts` (lever rename; `SUITE_RECEIVE_SLOTS` → `SUITE_AUDIO_RECEIVE_SLOTS`), `e2e/README.md`, `src/__tests__/{receiveSlotsConfig,errorText,testLevers}.test.ts`, `docs/runbooks/{client-dev-local,mc-incident-response}.md`, `docs/observability/metrics/client.md`, `client-media.json` descriptions.
- `scripts/dev-web.sh`: old name set ⇒ HARD FAIL first (before cluster probes) naming both new vars; exports both with demo defaults `DEMO_AUDIO_RECEIVE_SLOTS=3` / `DEMO_VIDEO_RECEIVE_SLOTS=3` (one variable each, echo interpolates them); non-integer/negative ⇒ HARD FAIL; reads `MC_MAX_AUDIO_RECEIVE_SLOTS` and `MC_MAX_VIDEO_RECEIVE_SLOTS` from `MC_SHARED_CONFIGMAP`, stops reading `MC_MAX_RECEIVE_SLOTS`; CANNOT VERIFY hard fails kept per key (absent file / absent key / malformed); over-cap ⇒ NOTICE "excess slots will show unavailable", made honest for the transition window per @operations ("until MC-A lands, N+M over the retired total `MC_MAX_RECEIVE_SLOTS` is rejected whole: `mc_media_receive_capability_declarations_total{outcome="slot_count_over_cap"}`; MC-A removes this"); video cap 0 ⇒ "video disabled"; echoes effective N and M with source; live-value kubectl one-liner switched to the per-kind keys; header comments (~23, ~130, ~633-670) updated. `dev-web.test.sh` extended: old name fails loudly, over-cap notice (not fail), video cap 0 ⇒ "video disabled", CANNOT VERIFY on each absent key, default N/M extraction.

### (4) Flat slot states (R-4) + shim (b) + pairing view
- `StreamAssignmentEvent['slotState']` = `unspecified | active | avatar_no_video | avatar_client_muted | avatar_server_muted | fewer_sources_than_slots | unavailable | switch_pending | withheld_by_congestion | source_unreachable` — spellings identical to MC `slot_state_label()` (renames `fewer_sources`→`fewer_sources_than_slots`, `withheld_congestion`→`withheld_by_congestion`; removes `source_muted`, `zero_requested`). `unspecified` is the fail-closed token for 0 and for any unknown number. ONE mapping site, `signaling/slotStateMap.ts` (`Record<SlotState, token>` keyed on the wire enum like `codecMap.ts`, so a new wire value is a compile error; out-of-range numbers → `unspecified`). Nothing but `ACTIVE` maps to `active` (table test over every generated enum value + an unknown number).
- `StreamAssignmentEvent` gains `mediaKind: 'audio' | 'video' | 'unknown'` from the wire (unknown kind ⇒ consumers treat as no media). Video `active` has no `senderId` (passed through as absent).
- Shim (b) in `SignalingClient` (marked, deleted by C-A2): wire `SOURCE_MUTED` → `avatar_server_muted` if the latest self/other `ParticipantMuteUpdate` for that slot's sender (participant↔sender map from the roster) has the slot kind server-muted, else `avatar_client_muted` (fail-closed default, including unknown sender / no update). Never `active`. When a `ParticipantMuteUpdate` arrives after the assignments, the latest assignments are re-derived and re-emitted so the server-wins mapping is not stuck on the default.
- Pairing view: `signaling/pairing.ts:pairSlotsBySender(assignments)` → `ReadonlyMap<senderId, {audioSlotId?, videoSlotId?}>` from assignments that name a sender; exposed as `MeetingSession.slotPairing`. Live-video pairing from verified frames is C-B's.
- Consumers: `AudioPipeline` assignment `active` flag uses `slotState === 'active'` (unchanged rule). web-app `slotState.ts` table (minimal copy; C-C1 writes final), `participantState.ts` (client mute from `avatar_client_muted`, server from `avatar_server_muted`), `InMeeting.svelte` comment, `server-mute.spec.ts` → `avatar_server_muted`, `solo-participant.spec.ts` / `over-subscription.spec.ts` / `slotOracle.ts` (`PENDING_SLOT_STATES` gets `satisfies readonly SlotStateToken[]`) / `videoEvidence.ts` token renames, plus their tests.

### (5) Per-kind mute and directives (R-15..R-18)
- `MuteState` gains `videoMuted` + `setVideoMuted`; snapshot `{audioMuted, videoMuted}`. Session-owned (one instance injected into `AudioPipeline`), so `MeetingSession.setAudioMuted` / `setVideoMuted` (and the pipeline's existing `setAudioMuted`) touch only their kind. Video mute has no capture to stop yet (C-A2).
- ONE builder `signaling/muteRequest.ts:bothKinds(current)` produces `{audioMuted, videoMuted}` with BOTH fields explicitly set, used by `sendMuteRequest(state)` and `sendServerMuteRequest(target, state)`; marked "until MC-B: both kinds on every request". Client: the unchanged kind is my current client-mute state. Host: `MeetingSession.setServerMute(participantId, kind, muted)`; the unchanged kind is that target's latest server-mute state from `ParticipantMuteUpdate` (default false). Callers updated (`ParticipantRow.svelte` → `'audio'`).
- `ParticipantMuteEvent` gains `videoServerMuted` (server mute only, as today); `MediaStore` unchanged in behaviour (audio), carries the field.
- `mediaWiring.sendDirectiveListener` becomes per kind over the frozen publish set: header_version ≠ 3 ⇒ session counter `send_directive_header_version_refused` (+ the pipeline's existing fail-closed fault/send-nothing); published kind missing from a non-empty `streams` ⇒ per-kind counter `send_directive_kind_missing` + typed `mediaFault` (stage `config`) + fail closed (empty targets); empty `streams` ⇒ shim (c) (E1). Audio withdrawal keeps the mic open, nothing sent, as today. Client mute never changes the directive.
- Server-mute tracking per kind from the SELF `ParticipantMuteUpdate` (self participantId from `JoinedEvent`), in a small pure module `session/serverMuteResume.ts` (testable state machine, per kind):
  - self update kind=true ⇒ latch `serverMuteRecorded[kind] = true`;
  - self update kind=false while that kind's targets are NON-empty (capture never stopped — e.g. the pre-MC-A MC, which does not withdraw) ⇒ clear the latch;
  - directive targets for kind go empty→non-empty ⇒ if latched: client-muted ⇒ no event (audio stays suppressed at capture), else emit `captureResumedAfterServerMute {kind}`; clear the latch either way.
  - So: update-then-resume fires; resume-then-update fires (latch was set by the earlier mute) and the later clearing update raises nothing; lost unmute update still fires; never-recorded ⇒ no event (named residual test, BY DESIGN comment). Event payload is `{kind}` only. Bridged on `MeetingSessionEventMap`.

### (6) Transport seam + MH control channel
- `IWebTransport` (sdk-core canonical AND test-utils contract) gains REQUIRED `createUnidirectionalStream(): Promise<WritableStream<Uint8Array>>` and `incomingUnidirectionalStreams: ReadableStream<ReadableStream<Uint8Array>>`; `BrowserWebTransport` implements them (pass-through to the platform object).
- `MockWebTransport`: outbound uni streams (captured per stream by ordinal index, no `Date.now()`), inbound uni streams via `simulateIncomingUniStream()` returning a handle (`enqueue`, `close`, `reset(code)`), per-stream reset/abort injection (`resetOutboundUniStream(i, code)` ⇒ next write rejects with a typed error carrying the code; inbound reset ⇒ next read rejects typed), inspector distinguishes `closed` / `reset` / `aborted`; slow-reader knob = per-stream promise gate (`pauseUniStreamWrites(i)` / `resumeUniStreamWrites(i)`), gated writes absent from the inspector until released, no timers; MH control helpers at raw framed bytes (no protobuf in test-utils): `simulateMhServerMessage(streamIndex, framedBytes)` and `getOutboundBidiFrames(streamIndex)` (length-prefix split). `clearInspector()` clears uni captures. One test per knob with a positive control.
- `MediaTransport`: after `MhConnectRequest`, `#startControlChannel(url, stream)` (takes only url + stream; jwt/envelope never captured) runs a read loop reusing `FrameDecoder` (64 KiB bound checked on the length prefix before the body is buffered — same bound as `MhClientMessage`) and decodes `MhServerMessage`: a framing error (oversize/empty) ⇒ `malformed`, loop stops (stream desynchronised; media unaffected); undecodable protobuf or unset variant ⇒ `malformed`, loop continues; `keyframe_request` ⇒ handed to an injected `MhControlSink`. Never throws untyped; no frame bytes on any error/cause. `sendRecoveryRequest(url, {slotId, senderId, gopCounter})` — per-handler control send through `sendFramedMessage`, serialised per URL. File header updated ("No frame I/O beyond the connect envelope" removed).
- Session's control sink: with no video stream, EVERY keyframe request is refused `not_sending`; attribution: stream_number → kind via the latest directive's streams for a published kind ⇒ that kind's handle (`keyframeRequestReceived('not_sending')`), otherwise the kind-free session counter (`not_sending`); malformed ⇒ session counter `malformed`. A request never touches capture/encoder.

### Telemetry (on OBS-A's handles; catalog rows + collector allowlist + dashboard panels in this change)
- `dt_client_media_capture_failures_total{media_kind, outcome}` — per-kind; outcomes = `CaptureFailure` vocabulary + `ended` (one const, derived, `satisfies`); emitted by audio on start/device-swap failure (classified reason) and on track end (`ended`); lazily created (no zero-init).
- `dt_client_media_keyframe_requests_received_total{media_kind, outcome}` — per-kind; outcome const `MEDIA_KEYFRAME_REQUEST_OUTCOMES` = honoured, honoured_counter_ahead, stale_counter, min_interval, not_sending, malformed (all declared); not zero-initialised now (E4).
- `dt_client_media_keyframe_requests_unattributed_total{outcome}` — session (kind-free), outcome ∈ {not_sending, malformed}; zero-initialised at join; reads zero on a conforming MH.
- `dt_client_media_send_directive_kind_missing_total{media_kind}` — per-kind; zero-initialised for each published kind at join; reads zero forever against a conforming MC; alertable at > 0.
- `dt_client_media_send_directive_header_version_refused_total` — session; zero-initialised at join; reads zero forever; alertable at > 0.
- No GoP, stream number, slot id, sender id, size or timing in any label/log/event/bus field. R-18 event carries `{kind}` only.

### Tests (node tier, MockWebTransport, injected clock, deterministic flushes, no sleeps)
Join carries publish set + declaration in id order (N or M = 0 allowed; decode the sent bytes); publish set frozen/copied (E2); 0+0+nothing refused locally; video publish refused typed; per-kind caps readable incl. absent ⇒ unknown, 0 ⇒ known 0; over-cap throw + counter gone; old env name loud (web-app + dev-web.test.sh); slot-state table over every generated enum value + unknown; shim (b) server-wins, client default, unknown sender, re-derive on late update; pairing view; per-kind mute independence; both kinds explicit on every mute/server-mute request (decoded); R-18: update-before-resume, resume-before-update, lost update, no second event on clearing update, client-muted ⇒ no event, residual never-recorded (named); directive: kind missing (non-empty streams) counted + faulted + fail-closed, empty streams shim (c), header version refused counted; send-id stamp = directive stream_number 7 with slots 0..2; MhServerMessage loop: split prefix, split body, oversize refused from header alone, zero-length, malformed protobuf, unset variant, well-formed request ⇒ `not_sending`, loop survives / terminates as intended; recovery send framing (decoded); cached directive/assignments applied at startMedia (E3); MockWebTransport knob tests.

### Gate-1 notes folded (observability, non-blocking)
- Capture-failure series is lazily created; catalog row says absence = no failures yet and panels use `or vector(0)`.
- `send_directive_kind_missing_total` "reads zero forever" holds at this gate only because shim (c) reads empty `streams` as the legacy withdrawal; stated in its row with the C-A2 deletion pointer.
- `keyframe_requests_unattributed_total` row: never summed by outcome with `keyframe_requests_received_total` (per-metric `outcome` rule, label-taxonomy.md:105).
- Collector budget comment shows recomputed arithmetic counting only series that can exist at this gate (no video), and lists what later tasks add.

### Gate-1 notes folded (test M1-M6, dry a-f)
- Mock directions: outbound peer action `simulateOutboundUniStopSending(i, code)`; outbound states `open | closed | aborted | stopped`; inbound `handle.reset(code)`, SDK `readable.cancel()` recorded as `cancelled`. Rejections use the platform shape `{name:'WebTransportError', source:'stream', streamErrorCode}` declared once in the contract. No `simulateMhServerMessage` (use existing `simulateServerMessage`). `getOutboundBidiFrames` throws on a truncated trailing frame/partial prefix (self-tested). Inbound uni buffered before the SDK reads; `createUnidirectionalStream` on a closed transport rejects; `simulateClose`/`simulateError` error open uni streams (tested). Separate ordinal spaces for inbound uni, outbound uni and bidi.
- R-18 extra named test: mute update, unmute update while targets non-empty, later empty→non-empty ⇒ 0 events.
- Header-version refusal uses `isSupportedHeaderVersion` and is counted once (mediaWiring), pipeline fault unchanged. `slotStateMap.ts` replaces the SignalingClient switch. Shim (b)/pairing reuse the existing roster sender index. Both IWebTransport copies get REQUIRED members. `receiveDeclaration(n, m)` is the only slot-id producer (join, shim a, AudioPipeline). Old-name tests assert the two new variable names.

### Rollback
Client + dev script + docs only; no MC/MH config or manifest change. Safe-revert unit: the whole commit (the env-var rename spans web-app, playwright, dev-web.sh and docs together).


### Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed (after S1–S3, Q2) |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Semantic Guard | confirmed |

Classification-sanity guard: `STATUS=OK`. Lead rulings E1–E6 approved (see §Planning). Lead note: INDEX.md + review-protocol were injected by reference (teammates read them as first action) rather than inline.

---

## Pre-Work

None

---

## Implementation Summary

### Contract migration (sdk-core)
| Item | Before | After |
|------|--------|-------|
| Uplink relay `stream_id` | `declaredSlotIds[0] ?? 0` | `pipeline/sendId.ts:uplinkRelayStreamId(directive)` — MC send id, 8-bit domain from `keyId.ts:STREAM_EXCLUSIVE_BOUND`; out of range → typed fault, nothing sent |
| Join | no publish set / declaration | `JoinRequest.publish_kinds` + `receive_capability` (audio `0..N-1`, video `N..N+M-1`) from the frozen `SlotPlan` (`session/slotPlan.ts`) |
| Publish set | — | default `['audio']`; video → `PublishKindNotSupported`; empty + 0+0 → `NothingToPublishOrReceive` (typed, at construction; copy in `web-app/src/lib/errorText.ts`) |
| Caps | tag 12 `max_receive_slots`, `invalid` state | tags 13/14 per kind, `known` (0 legal) / `unknown`; `MeetingSession.receiveSlots` = `{publish, audio, video}` |
| Over-cap | SDK throw + `dt_client_media_receive_slots_rejected_total` | retired (catalog rows, collector, dashboard panel, runbooks, mc-service.md in the same change) |
| Slot states | 8 tokens incl. `source_muted`, `zero_requested`, `fewer_sources`, `withheld_congestion` | flat, MC `slot_state_label` spellings; one map `signaling/slotStateMap.ts` (nothing but ACTIVE → `active`); `StreamAssignmentEvent.mediaKind`; pairing view `signaling/pairing.ts` / `MeetingSession.slotPairing` |
| Mute | audio only, `videoMuted` hardcoded false | `MuteState` per kind (session-owned), `setAudioMuted`/`setVideoMuted`; both kinds explicit on every Mute/ServerMuteRequest (`signaling/muteRequest.ts:explicitMuteFields`); `setServerMute(id, kind, muted)` carries the target's current other kind |
| Directive | audio-only, `streams: []` = withdrawal | per published kind; missing kind in non-empty streams → counted + typed fault + fail closed; header version refused → counted once; shim (c) for empty streams |
| R-18 | — | `session/serverMuteResume.ts` (invariant S1 in header) fed by `session/signalingMediaState.ts`; `captureResumedAfterServerMute {kind}` event |
| E3 | directive/assignments subscribed at `startMedia()` | cached from signalling creation, replayed once on pipeline attach |
| Transport seam | bidi + datagrams | `IWebTransport` REQUIRED `createUnidirectionalStream()` / `incomingUnidirectionalStreams` (+ platform stream-error shape); `BrowserWebTransport` pass-through |
| MH control channel | write-once connect envelope | `MhServerMessage` read loop (`FrameDecoder`, 64 KiB bound on the prefix), `sendRecoveryRequest` (serialised per handler); every keyframe request refused `not_sending` |
| Signalling writes | concurrent sends raced the writer lock (a lost mute report) | encode + trace-inject at call time, writes serialised (`framing/sendFramed.ts:encodeFramedMessage`/`writeFrame`) |

### Transitional shims (each marked, deletion owner C-A2 / task 13)
- (a) post-join `ReceiveCapability` identical to the join-carried one (`MeetingSession.startMedia`).
- (b) legacy `SOURCE_MUTED` → avatar token, server wins, client-muted fail-closed default, re-derived on a late mute update (`signaling/legacySourceMutedShim.ts`).
- (c) empty `streams` = pre-MC-A withdrawal (`session/mediaWiring.ts:legacyEmptyStreamsWithdrawal`).
- e2e harness: `expectDeclaredReceiveSlots` records a `transitional` annotation + gate-log warning instead of failing when the pre-MC-A controller sends no per-kind caps (deletion owner MC-A, task 12).

### Telemetry
New: `dt_client_media_capture_failures_total{media_kind,outcome}`, `dt_client_media_keyframe_requests_received_total{media_kind,outcome}` (5 per-kind outcomes), `dt_client_media_keyframe_requests_unattributed_total{outcome}`, `dt_client_media_send_directive_kind_missing_total{media_kind}`, `dt_client_media_send_directive_header_version_refused_total`, `dt_client_media_mh_control_channel_ended_total{outcome}` (Gate 3, observability F1). Retired: `dt_client_media_receive_slots_rejected_total`. Catalog rows, collector allowlist + recomputed budget (28 names; 75 streams per org at this gate), dashboard panels (retired panel replaced in place; four appended).

### Env-var rename
`VITE_DT_AUDIO_RECEIVE_SLOTS` / `VITE_DT_VIDEO_RECEIVE_SLOTS` (≥ 0, no alias); the retired name fails loudly in the web app (`RETIRED_RECEIVE_SLOTS_MESSAGE`) and first in `dev-web.sh`. `dev-web.sh`: demo N=3/M=3, per-kind caps from `MC_SHARED_CONFIGMAP`, CANNOT VERIFY per key, over-cap NOTICE, "video disabled" at 0, transition authority note; `dev-web.test.sh` extended (217 assertions). Levers `audioReceiveSlots`/`videoReceiveSlots`; playwright pins M=0.

---

## Files Modified

```
 101 files changed, 6879 insertions(+), 1248 deletions(-)
```

See §Implementation Summary for per-area changes.

---

## Devloop Verification Steps

Gate 2: `DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` → `TOTAL_RESULT=OK` (TOTAL_DURATION=1361s; log `/tmp/lead-all.log`).

| Layer | Result | Duration |
|-------|--------|----------|
| 1 Compile | OK | 5s |
| 2 Format | OK | 4s |
| 3 Guards | OK | 164s |
| 4 Test | OK | 145s |
| 5 Lint | OK | 1s |
| 6 Audit | OK | 4s |
| 7 Env-tests (Rust env-tests + browser E2E) | OK | 1038s |

Story-2 browser specs green against the pre-MC-A controller; the e2e transitional cap-tolerance warning printed for A/B/C as intended.

---

## Code Review Results

See §Gate 3 — Verdicts below. Per-reviewer detail: Security S-1 (pre-startMedia R-18 tests); Test F1–F3 (fixture tolerance now owned by MC-A + console.warn, mock abort-while-gated test, unstampable stream_number test); Observability F1–F3 (`dt_client_media_mh_control_channel_ended_total`, per-kind keyframe type excludes `malformed`, exhaustive `kindMetrics`); Code Quality F1–F9 (single `media/mediaKind.ts` leaf + single mapper, shim-b simplification, always-wired controlSink, dev-web helpers, doc/import fixes); DRY F1–F2 (dev-web demo constants single home, mock length-prefix comment; no TODO entries); Operations F1–F2 (dev-web participant counts, control-channel entry dropped on stream end). Semantic Guard: SAFE, no findings.

---

### Gate 3 — Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 1 | 1 | 0 | S-1: pre-startMedia R-18 tests |
| Test | RESOLVED-FIXED | 3 | 3 | 0 | F1 fixture tolerance → MC-A deletion owner + warn |
| Observability | RESOLVED-FIXED | 3 | 3 | 0 | control-channel-ended counter; malformed type; exhaustive kindMetrics |
| Code Quality | RESOLVED-FIXED | 9 | 9 | 0 | |
| DRY | RESOLVED-FIXED | 2 | 2 | 0 | no TODO entries |
| Operations | RESOLVED-FIXED | 2 | 2 | 0 | |
| Semantic Guard | CLEAR (SAFE) | 0 | 0 | 0 | |

## Accepted Deferrals

- (none surfaced in this devloop)

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `90b656a9673500c41e75391231ec95bc7b58f97b`
2. Review all changes: `git diff 90b656a9673500c41e75391231ec95bc7b58f97b..HEAD`
3. Soft reset (preserves changes): `git reset --soft 90b656a9673500c41e75391231ec95bc7b58f97b`
4. Hard reset (clean revert): `git reset --hard 90b656a9673500c41e75391231ec95bc7b58f97b`
5. For schema changes: rollback requires a forward migration — `git reset` alone is insufficient if migrations were applied
6. For infrastructure changes: may require `skaffold delete` or `kubectl delete -f` if manifests were applied
7. **Safe-revert unit** (answer explicitly, even if "the whole commit"): can any part of this diff be reverted or cherry-picked on its own, or does a partial revert reconstruct a state worse than either endpoint (e.g. a security fix split from the change that made it necessary, or a client/server/alert-rule set that must move together)? If partial reverts are unsafe, name the unit and the safe direction. **Answer**: `the whole commit` — client-only (no MC/MH config or manifests); the SDK, dev-web.sh, catalog/collector/dashboard and runbooks move together (G3 client-metrics-export couples metric code to catalog rows) (This converts silence into a visible unanswered slot; it cannot distinguish a checked answer from a reflexive one.)

---

### Lead ruling (Gate 3) on the e2e cap tolerance
Adopted @test F1 as written: MC-A (task 12) is the deletion owner of the `expectDeclaredReceiveSlots` transitional tolerance (not C-A2); task 12's text in the story file carries an explicit line telling MC-A to restore the strict known-cap and cap >= N assertion; the tolerance also `console.warn`s so it shows in this gate's log.

## Issues Encountered & Resolutions

### Issue 1: `MeetingSession` grew past the declaration-size guard
**Problem**: `dt-guard ts_retained_credentials` requires the largest TS declaration ≤ 800 lines (`MAX_DECL_BLOCK_LINES` 2400 / 3); `MeetingSession` reached 946.
**Resolution**: split, not raised: `session/slotPlan.ts` (publish set, N/M, declaration, caps, log line), `session/signalingMediaState.ts` (E3 cache, server/client mute, R-18 feed), `mediaWiring.ts:pipelineListeners`/`zeroInitDirectiveSeries`. Now 797 — thin margin for C-A2.

### Issue 2: concurrent signalling sends lost a mute report
**Problem**: two sends in flight raced `stream.writable.getWriter()`; the loser threw and the best-effort mute report swallowed it (found by the per-kind mute test).
**Resolution**: encode/trace-inject at call time, serialise only the write (SignalingClient and the MH control send).

### Issue 3: pre-MC-A controller sends no per-kind caps
**Problem**: the story-2 browser fixture asserted a KNOWN cap from tag 12, which the SDK no longer reads.
**Resolution**: transitional annotation + gate-log warning until MC-A; per the test Gate-3 ruling MC-A (task 12) deletes the branch and restores `unknown` as a failure (line added to task 12 in the story file).

---

## Lessons Learned

1. A prompt rule written against the post-MC-A controller ("missing kind = fault") can fault the pre-MC-A controller live at this gate; check each wire rule against the controller live at THIS gate (→ shim c).
2. Transitional test tolerances must be owned by the FIRST task whose gate should see the strict behaviour (MC-A), not the shim-cleanup task.
3. `MeetingSession` is at 793/800 lines of the declaration cap; C-A2 must extract before adding.

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
