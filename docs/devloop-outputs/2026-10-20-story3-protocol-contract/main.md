# Devloop Output: Story 3 protocol contract and ADR-0036 amendment bundle (P1)

**Date**: 2026-10-20
**Task**: Story 3 (see-each-other) task P1 — additive signaling/internal proto changes, codegen, roundtrip tests, ADR-0036 2026-10-13 amendment bundle, API/WebTransport docs
**Specialist**: protocol
**Mode**: Agent Teams (v2) — full, Gate-1 present <!-- panel mode + Gate-1 tier (ADR-0037 §D2); see the Tier row in Loop State -->
**Branch**: `feature/user-see-each-other`
**Duration**: resumed session ~2.5h (plus the interrupted planning session)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `8a17b2e56711b446d9c0e66385b3df5dcabeb74a` |
| Branch | `feature/user-see-each-other` |
| Lead Model | `claude-opus-5-5[1m]` |

---

## Loop State (Internal)

<!-- This section is maintained by the Lead for state recovery after interruption. -->
<!-- Do not edit manually - the Lead updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` |
| Implementing Specialist | `protocol` |
| Tier | `full` |
| Iteration | `2` (resume) |
| Security | `security` |
| Test | `test` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Semantic Guard | `not spawned — no check surface (no new key material, no Debug/Display over payload/key, no credential-lifetime code)` |
| Paired Specialist | `paired-media-handler` (GSA `crates/media-protocol/**` co-owner; mh-service comment edits) |
| Conditional Reviewers | `meeting-controller`, `client` (consumers named by the task) |

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
Land every story-3 wire change ADDITIVELY in `signaling.proto` and `internal.proto` (superseded items marked "SUPERSEDED -- deleted by MC-B", `SLOT_STATE_ZERO_REQUESTED` reserved), regenerate Rust + TS codegen, extend the contract gates, fix the hop-local relay-id comments, and land the ADR-0036 2026-10-13 amendment bundle (normative v3 table in §2) plus API/WebTransport doc updates and three TODO closures.

### Scope
- **Service(s)**: proto contracts (GSA); compile-preserving shims in mc-service, mh-service, sdk-core, web-app; comment-only edits in media-protocol, mh-service
- **Schema**: No
- **Cross-cutting**: Yes (MC, MH, client consume the shapes)

### Debate Decision
NOT NEEDED - the wire was frozen by the story's Design §Wire freeze (2026-10-08, three amendments); this task transcribes it.

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
| `proto/dark_tower/signaling/v1/signaling.proto` | Mine (GSA `proto/**`; security reviews; consumers meeting-controller, media-handler, client) | protocol |
| `proto/dark_tower/internal/v1/internal.proto` | Mine (GSA `proto/**`; security reviews; consumers meeting-controller, media-handler) | protocol |
| `crates/proto-gen/tests/signaling_roundtrip.rs` | Mine (GSA `proto-gen/**`; new-value tests + struct-literal/`Option<bool>` compile fixes) | protocol |
| `crates/proto-gen/tests/internal_roundtrip.rs` | Mine (GSA `proto-gen/**`) | protocol |
| `crates/proto-gen/tests/common/mod.rs` (new; hoisted `top_level_field_numbers`) | Mine (GSA `proto-gen/**`) | protocol |
| `crates/proto-gen/src/lib.rs` (redacting Debug for `JoinRequest`/`JoinResponse` lists the new fields) | Mine (GSA `proto-gen/**`; no `skip_debug` change) | protocol |
| `packages/proto-gen/scripts/verify-codegen.sh` | Mine (GSA `proto-gen/**`; presence + member-scoped absence oracle) | protocol |
| `crates/media-protocol/src/frame.rs` (comment-only) | Not mine, Domain-judgment (GSA `crates/media-protocol/**`, co-owned; paired) | media-handler |
| `crates/media-protocol/src/codec.rs` (comment-only, relay-id doc) | Not mine, Domain-judgment (GSA, paired) | media-handler |
| `crates/mh-service/src/routing/mod.rs` (comment-only, identifier table) | Not mine, Domain-judgment (paired) | media-handler |
| `crates/mh-service/src/media/forward.rs` (comment-only, FORECLOSED rewrite) | Not mine, Domain-judgment (paired; security reviews) | media-handler |
| `crates/mh-service/src/webtransport/connection.rs` (compile shim: new `MhClientMessage` variant before connect = existing invalid-connect reject) | Not mine, Minor-judgment | media-handler |
| `crates/mh-service/src/grpc/mh_service.rs` (compile fill: `applied_source_streams: vec![]`) | Not mine, Minor-judgment | media-handler |
| `crates/mh-service/tests/**` (compile fills: `source_streams: Vec::new()`) | Not mine, Minor-judgment | media-handler |
| `crates/mh-test-utils/**` (compile fill: `source_streams: Vec::new()`) | Not mine, Minor-judgment | media-handler |
| `crates/mc-service/src/webtransport/connection.rs` (mute `Option<bool>` shim `unwrap_or(false)`, JoinResponse fills) | Not mine, Minor-judgment | meeting-controller |
| `crates/mc-service/src/media_signaling/assignments.rs` (`slot_state_label` arms 8..11, drop `zero_requested`; tests) | Not mine, Minor-judgment | meeting-controller (observability co-reviews tokens) |
| `crates/mc-service/src/observability/metrics.rs` (`SLOT_STATES` domain + witness) | Not mine, Minor-judgment | meeting-controller / observability |
| `crates/mc-service/src/media_signaling/directive.rs` (`Codec::Vp8` into the not-audio arm; `min_keyframe_interval_ms: None` fill) | Not mine, Minor-judgment | meeting-controller |
| `crates/mc-service/src/grpc/mh_client.rs` (compile fill: `source_streams: vec![]`) | Not mine, Minor-judgment | meeting-controller |
| `crates/mc-service/src/media_routing/confirm.rs` (test compile fill) | Not mine, Minor-judgment | meeting-controller |
| `crates/mc-test-utils/**` (compile fill) | Not mine, Minor-judgment | meeting-controller |
| `crates/mc-service/tests/**` (compile fills, `Some(..)` mute literals) | Not mine, Minor-judgment | meeting-controller |
| `crates/env-tests/**` (compile fills only: `src/fixtures/mc_session.rs`, `src/fixtures/mh_grpc.rs`, `tests/26_mh_quic.rs`, `tests/35_mc_server_mute_teardown.rs`; feature-gated, found at Gate 2 L7) | Not mine, Minor-judgment | test |
| `packages/sdk-core/src/signaling/SignalingClient.ts` (delete the `ZERO_REQUESTED` arm only) | Not mine, Minor-judgment | client |
| `packages/sdk-core/src/signaling/codecMap.ts` (`[Codec.VP8]: null`) | Not mine, Minor-judgment | client |
| `packages/sdk-core/src/media/frame/frameCodec.ts` (comment-only, hop-local relay id) | Not mine, Minor-judgment | client |
| `packages/sdk-core/src/media/frame/keyId.ts` (comment-only, hop-local relay id) | Not mine, Minor-judgment | client |
| `packages/sdk-core/src/media/pipeline/egress.ts` (comment-only, hop-local relay id; NOT the stamping call site, C-A1) | Not mine, Minor-judgment | client |
| `crates/mc-service/src/actors/meeting.rs` (comment-only, superseded per-kind mute sentence; review fix) | Not mine, Minor-judgment | meeting-controller |
| `crates/media-vector-gen/src/kid.rs` (comment-only, hop-local relay id; review fix) | Not mine, Domain-judgment (paired) | media-handler |
| `crates/media-vector-gen/src/json.rs` (comment-only, hop-local relay id; review fix) | Not mine, Domain-judgment (paired) | media-handler |
| `docs/observability/metrics/mh-service.md` (superseded FORECLOSED paragraph, dated correction; review fix) | Not mine, Minor-judgment | observability |
| `packages/sdk-core/src/media/lifecycle/AudioPipeline.ts` (comment-only, corrected directive rule; client-approved review fix) | Not mine, Minor-judgment | client |
| `packages/sdk-core/src/media/lifecycle/muteState.ts` (comment-only, corrected directive rule; review fix) | Not mine, Minor-judgment | client |
| `packages/sdk-core/src/signaling/__tests__/maps.test.ts` (VP8 -> null pin; review fix) | Not mine, Minor-judgment | client |
| `packages/sdk-core/src/signaling/__tests__/mediaSignaling.test.ts` (reserved 5 and 8..11 -> unspecified; review fix) | Not mine, Minor-judgment | client |
| `docs/user-stories/2026-10-07-see-each-other.md` (lead-directed amendment: C-A1/C-A2 send both mute kinds until MC-B; MC-B switches to changed-kind-only; OBS-A label-taxonomy note) | Mine (lead ruling at review) | protocol |
| `docs/decisions/adr-0036-media-flow.md` | Mine (ADR amendment bundle; security reviews all of it; MC/MH/client/test/observability their sections) | protocol |
| `docs/API_CONTRACTS.md` | Mine | protocol |
| `docs/WEBTRANSPORT_FLOW.md` | Mine | protocol |
| `docs/specialist-knowledge/protocol/INDEX.md` | Mine | protocol |
| `docs/TODO.md` (close three entries) | Mine (entries owned by protocol / closing pointer to MC-A for the allowlist entry) | protocol |
| `docs/observability/metrics/mc-service.md` (slot_state value list, closed-TODO sentence) | Not mine, Minor-judgment | observability |
| `infra/grafana/dashboards/mc-media.json` (description + override regex drop `zero_requested`) | Not mine, Minor-judgment | observability |
| `docs/runbooks/client-dev-local.md` (zero_requested mention) | Not mine, Minor-judgment | operations |
| `docs/runbooks/mc-incident-response.md` (slot_state domain statement; review fix) | Not mine, Minor-judgment | operations |
| `docs/observability/label-taxonomy.md` (stale "eight-value" count only; observability-approved review fix) | Not mine, Minor-judgment | observability |
| `docs/devloop-outputs/2026-10-20-story3-protocol-contract/main.md` | Mine | protocol |

---

## Planning

Authority: story `docs/user-stories/2026-10-07-see-each-other.md` Design §Wire freeze + §Protocol Changes, then the P1 task prompt. No story/prompt disagreement found on wire shapes. One prompt-internal infeasibility (D1 below) is escalated to the Lead.

### Mechanism restatement
"Land the story-3 wire additively" is, mechanically: (1) add frozen tags/values; (2) **re-state three rules that change meaning everywhere they are restated** — the relay `stream_id` is hop-local (uplink = MC send id, downlink = subscriber slot id), slot state is a flat enum (ZERO_REQUESTED reserved, SOURCE_MUTED superseded), and per-kind server mute holds in slot terms (FORECLOSED reasoning rewritten). Rule (2) has more sites than the prompt names (DRY reviewer's sweep): frame.rs ×4, codec.rs, frameCodec.ts, keyId.ts, routing/mod.rs, CandidateSource/SendStream/ReceiveSlot disclaimers, ADR §2, API_CONTRACTS, WEBTRANSPORT_FLOW, plus the slot-state label mirrors (MC, SDK, web-app, catalog, dashboard, runbook). All are in scope here (complete the invariant).

### D1 — Declared deviation: "no consumer edits beyond comments" cannot hold
prost/protobuf-es make the frozen wire changes compile-visible:
- `optional bool` -> `Option<bool>` (MC `connection.rs` mute/server-mute handlers, ~8 test constructors);
- reserving `ZERO_REQUESTED` deletes `SlotState::ZeroRequested` / TS `SlotState.ZERO_REQUESTED`; values 8..11 break exhaustive Rust matches (`slot_state_label`, `metrics.rs` witness);
- `CODEC_VP8` breaks `directive.rs`'s exhaustive codec match;
- new fields break exhaustive struct literals (`EncodingParameters`, `JoinRequest/Response`, `RegisterMeetingRequest/Response`) — no `..Default` at those sites;
- new `MhClientMessage` oneof variant breaks MH's connect-phase match.
Plan: minimal **behaviour-preserving** compile fills, each listed in the Cross-Boundary table with its owner:
- new fields filled with their ABSENT value (`None`, `vec![]`); audio directive `min_keyframe_interval_ms: None` (audio ignores it, rule (8));
- mute: `request.audio_muted.unwrap_or(false)` at MC read sites with a one-line "interim; MC-B replaces with absent = unchanged, both-absent rejected and counted" comment. Byte-for-byte today's semantics (proto3 implicit `false` is absent on the wire). Test literals become `Some(x)`;
- `Codec::Vp8` joins the existing `CodecNotAudio` arm;
- MC `slot_state_label` gains `avatar_no_video`, `unavailable`, `avatar_client_muted`, `avatar_server_muted` (wire names lower-snake, never "live"), drops `zero_requested`; `SLOT_STATES` + witness likewise; the `assert_ne!(ZeroRequested)` test line is deleted (unrepresentable now). No emission change: MC still emits only today's states;
- SDK (client's ruling): delete ONLY the `case SlotState.ZERO_REQUESTED` arm in `SignalingClient.ts`; the `'zero_requested'` union token and web-app text stay for C-A1's union rewrite. New wire values 8..11 fall to the existing `default -> 'unspecified'` (fail closed) until C-A1 maps them. `codecMap.ts` gains `[Codec.VP8]: null` (no public `Vp8` member until C-A2's probe gate) — `fromWireCodec(5)` already returns null, so behaviour is unchanged;
- SDK wire effect, no edit: `SignalingClient.ts` sets `videoMuted: false` explicitly, which now goes on the wire as present-false (was omitted). Harmless pre-MC-B; C-A1 switches to sending only the changed kind;
- MH: `Some(RecoveryRequest(_))` as the FIRST message is the existing "Invalid connect message" reject (not a connect). Post-connect reading is MH-B's.
Escalated to @team-lead as a prompt-internal contradiction; proceeding on this basis unless told otherwise.

### Signaling changes (tags per the frozen wire)
- `Codec.CODEC_VP8 = 5` (only video codec MC directs; client declares only after `VideoEncoder`/`VideoDecoder.isConfigSupported` both pass).
- `JoinRequest.publish_kinds = 8` (repeated MediaKind), `receive_capability = 9`. Comment: validation first (UNSPECIFIED / VIDEO_SCREEN / unknown / duplicate rejected and counted, never ignored), then the neither-publish-nor-receive refusal judged on the declaration AS SENT before truncation; absent `receive_capability` = zero slots of each kind (not an error by itself); truncation never fails a join; reconnect presenting a different publish SET (order-insensitive) is refused and counted; post-join `ClientMessage.receive_capability` stays the redeclaration path, and a redeclaration leaving the participant neither publishing nor receiving is refused.
- `JoinResponse.max_audio_receive_slots = 13`, `max_video_receive_slots = 14` (optional; informational, R-23, MC truncates). `max_receive_slots = 12` "SUPERSEDED -- deleted by MC-B, then reserved by tag and name".
- `SlotState`: `AVATAR_NO_VIDEO = 8`, `UNAVAILABLE = 9` (no sender, no url, echoes declared slot_id; permanent for the declaration, changes only on redeclaration; video cap 0 -> every video slot UNAVAILABLE, R-26), `AVATAR_CLIENT_MUTED = 10`, `AVATAR_SERVER_MUTED = 11` (server wins; both kinds). `SOURCE_MUTED` "SUPERSEDED -- deleted by MC-B, then reserved by number and name"; its "no enum value will be added" paragraph deleted (R-4 overrules it). `ZERO_REQUESTED` reserved by number 5 and name, with the unemittability reasoning moved beside the `reserved` line. SWITCH_PENDING / WITHHELD_BY_CONGESTION / SOURCE_UNREACHABLE: retained, never emitted, consumers fail closed AND count; reason given at each. Telemetry labels follow wire names (`active`, never "live") — no restated count.
- `StreamAssignment.sender_id`: absent on video ACTIVE (client attributes from the verified key-id sender); present on audio ACTIVE and every AVATAR_*; absent on FEWER_SOURCES and UNAVAILABLE; never coerced to 0.
- `MuteRequest` / `ServerMuteRequest` per-kind fields -> `optional bool`. Absent = that kind unchanged; both absent rejected and counted. Comment says **tag-compatible but NOT semantics-compatible with a pre-optional encoder** (its `false` is indistinguishable from absent); senders MUST set every kind they mean. Names the defect: `crates/mc-service/src/media_signaling/outcome.rs::ServerMuteAction::from_request` folds both fields into one Mute/Unmute (`{audio:true, video:false}` mutes both; `{false,false}` the only unmute); MC-B fixes. `reason` caveats kept.
- `ParticipantMuteUpdate` (R-18): self copy is the ONLY source of own server-mute state; MC MUST send it before the directive change that restores capture, on the same MC->client signalling stream in order; client MUST NOT resume capture on a server-unmute path without the non-suppressible notice even if order is violated (fail-safe).
- `EncodingParameters.min_keyframe_interval_ms = 6` (optional). Video only and required on video; client refuses a video stream without it and enforces max(directed, compiled 100 ms floor); audio ignores it. THE single normative home for `MEDIA_VIDEO_MIN_KEYFRAME_INTERVAL_MS` (shared `media-codec-profile`, 100..=10000, default 500, landed by INF-A — not live in tree yet); MC directs it and publishes `mc_media_video_keyframe_min_interval_seconds` = the same parsed value converted to seconds; MH also reads it. Other sites name the key and point here.
- `SendDirective`/`SendStream` (R-8/R-16/R-17): direct a kind only while watched and not server-muted; withdrawn = listed with EMPTY targets; published-but-never-watched = listed with empty targets; never-published kind = never listed. Server mute withdraws (capture stops, camera light off); client mute MAY stay directed. Video = `stream_number` 1, VIDEO_CAMERA, VP8, STREAM_PER_GROUP; audio stays 0. `stream_number` is also the uplink relay id. Header sentence "MC keeps a send directive active while a client reports itself client-muted" corrected with a dated note. `header_version`: v3 only, derived from `PROTOCOL_VERSION`; floor language deleted.
- `ParticipantCapabilities.supported_header_versions`: a list lacking 3 is refused at join, before admission (no slot, sender id or roster entry allocated), with the existing generic join-refusal shape, no version echoed, counted at MC. Owner MC-A. "ENFORCEMENT NOT YET ALLOCATED" paragraph deleted. No allowlist, no floor.
- New `MhServerMessage { oneof message { KeyframeRequest keyframe_request = 1; } string trace_parent = 20; string trace_state = 21; }` (MH sets none). `KeyframeRequest { uint32 stream_number = 1; uint32 gop_counter = 2; }` — `stream_number` is the publisher's uplink relay id (MC send id), NOT a slot id. Publisher rules (R-10/R-11) and MH rules (after one frame, retry until group start at `MH_KEYFRAME_REQUEST_RETRY_MS`, MH-B, default 500, 100..=10000, MUST be >= `MEDIA_VIDEO_MIN_KEYFRAME_INTERVAL_MS`, enforced by MH startup validation, fail closed). Untrusted at the publisher: a compromised MH can at most drive keyframes at max(directed, 100 ms floor). Same bidi stream / 4-byte prefix / size bound as `MhClientMessage`; scalars only.
- `MhClientMessage.recovery_request = 2`; `RecoveryRequest { uint32 slot_id = 1; uint32 sender_id = 2; uint32 gop_counter = 3; }` — `slot_id` is the receiver's DOWNLINK relay id; `sender_id` staleness guard only (`source_mismatch`), never authority, never forwarded; equal -> forward, lower -> `stale`, higher -> `counter_ahead`, no clamping; MH re-sends with its own fields, aggregated per source stream at `MH_KEYFRAME_REQUEST_AGGREGATION_MS` (MH-B, default 100); per-connection bound `MH_RECOVERY_REQUESTS_PER_CONNECTION_PER_SEC` (MH-B); counter 0 legal. Not a token-refresh variant (read-once pointer untouched). The two 100 ms values (client compiled floor vs MH aggregation default) are called out as distinct.
- Every new GoP counter / stream number / slot id / sender id field: "OBSERVABILITY (ADR-0036 §11): as `SubscriberSlot`" one-liner.
- Hop-local relay id: `ReceiveSlot.slot_id`, `SendStream.stream_number`, `CandidateSource.stream_number` disclaimers reworded to "8-bit key-id field vs 16-bit relay field; on the uplink the relay field carries `stream_number`'s value, publisher-set, unauthenticated, never cross-checked against the key id (signing considered and dropped)". Disclaimers kept.

### Internal changes
- `SourceStream { sender_id = 1; stream_number = 2; dark_tower.signaling.v1.TransportMode transport_mode = 3; SourceDisposition disposition = 4; }` on `RegisterMeetingRequest.source_streams = 8`; `enum SourceDisposition { SOURCE_DISPOSITION_UNSPECIFIED = 0; _FORWARD = 1; _PAUSED = 2; _DROP_AT_INGRESS = 3; }`. Rules: absence now means UNDECLARED (drops `unmatched_stream`), the opposite of the old muted set — MC MUST list every stream published by every participant attached to this handler, watched or not, which covers every candidate in this handler's egresses; behaviour names only; PAUSED = client mute (switch to it completes at once as forward-nothing, cancels keyframe wait, no ingress drop); DROP_AT_INGRESS = server mute, implies paused, drops before forwarding, counted `server_muted`; UNSPECIFIED disposition or transport rejects; disposition change advances `policy_generation`; immediate hard rejects: duplicate (sender_id, stream_number), undeclared candidate and mixed candidate modes (`rejected_transport_inconsistent`), count over `MH_MAX_SOURCE_STREAMS_PER_MEETING` (INF-A; replaces `MH_MAX_MUTED_SOURCES_PER_MEETING`); uplink id > 255 can never match -> `unmatched_stream`, never truncated. Legacy (transitional, deleted by MC-B with the superseded proto fields): EMPTY list = stream 0 DATAGRAM per sender with `server_muted_sources` senders DROP_AT_INGRESS; after MC-B an empty list with egress streams rejects as `rejected_transport_inconsistent`. **Overlap precedence (security Gate 1; the story is silent on it, and MC-A keeps writing `server_muted_sources` while also emitting `source_streams`):** until MC-B deletes it, a sender in `server_muted_sources` is DROP_AT_INGRESS for EVERY one of its streams, whatever `source_streams` says. The two fields combine as a union, the more restrictive wins, and neither un-mutes the other. This is fail-closed and reproduces story-2 behaviour in the overlap. **Traceability:** a gap-fill requested by security at Gate 1 and agreed by media-handler. Story line 1137 defines MH's reading only for an EMPTY `source_streams`, and line 1697 has MC-A keep writing `server_muted_sources`, so the non-empty-plus-non-empty case is a genuine gap, not new mechanism. **Scope:** this rule and the empty-list path are ONE transitional mechanism. It is stated once, at `server_muted_sources` ("read only by the transitional path, deleted by MC-B with the superseded proto fields"), and cross-referenced from `source_streams`, so MC-B has a single reader to delete. **Counting:** frames from a union-muted sender count as `server_muted` (the sender-level shortcut, story line 1158), including frames that cannot be attributed. They never count as `unmatched_stream`. `sender_id` -> `SubscriberSlot`; `stream_number` SSoT `SendStream.stream_number` + KEY_ID_BYTES anchor. Attribution rule (R-20) and the dropped signed-label alternative recorded here.
- IMMEDIATE HARD REJECTS list at `SubscriberSlot` gains the new rejects; `server_muted_sources` entries marked superseded.
- FORECLOSED at `MutedSource` rewritten with a dated correction (2026-10-13): old claim, why superseded, slot-terms property, its dependency on the receiver rule "kind from slot, never frame" (R-5), the self-selectable-label insight retained as the reason the property is in slot terms, signed-label alternative dropped.
- `RegisterMeetingResponse.applied_source_streams = 6` of `SourceStreamEcho { sender_id = 1; stream_number = 2; transport_mode = 3; }` — built from the APPLIED snapshot (legacy path echoes the synthesized entries); MC compares per stream, keyed by (sender_id, stream_number), order irrelevant, unconditionally; missing or UNSPECIFIED is a mismatch; per-stream not per-mode counts (a swap reads green on counts). Fields reference `SourceStream`. `transport_mode = 5` "SUPERSEDED -- deleted by MC-B".
- `EgressStream.transport_mode = 6` "SUPERSEDED -- deleted by MC-B; an edge's mode derives from its candidate's `SourceStream`". `server_muted_sources = 7` and `MutedSource` "SUPERSEDED by `source_streams` -- deleted by MC-B".
- `priority_group`: lower number = higher priority; MC assigns audio 1, video 2; MC never assigns 0. `supersede_on_independent_frame` true for video edges.

### Threat-model note (security point 1)
The uplink relay `stream_id` is chosen by the (possibly muted, possibly patched) publisher and MH does not authenticate it. The property is therefore stated in SLOT terms, not "MH identifies the frame's stream": (a) a server-muted stream is DROP_AT_INGRESS, so frames attributed to it never forward; (b) a frame relabelled onto the participant's other declared stream must also match that stream's declared transport, so it can only reach slots of the OTHER kind; (c) receivers decode by slot kind, never by frame (R-5), so it cannot render as the muted kind. (c) is a load-bearing receiver dependency and is stated as such. Signing the label adds nothing: the publisher also sets the key id.

### ADR-0036 amendment bundle (dated 2026-10-13, reversed text kept visible, blockquote convention, task codes only)
§1 (cadence row of the transport table + "Why" two-parameters text) and §7 ("Keyframe cadence is now two parameters", "bounded wait ... open item"): on-demand keyframes, per-destination abandonment, cut-through. §2: a NORMATIVE v3 table (absolute offsets for the 14-byte prefix + cursor rule for conditional blocks, sizes, flag bit values and legal mask, wrapped-key sub-layout (AEAD input key||tag, nonce/AAD -> §4), TLV grammar + registry stated or pinned to its exact normative location, frame extent formula for stream readers (`trailing_bytes` applies to a datagram), the full decode check ORDER with the closed reject vocabulary including where `key_bearing_discardable` sits, gop_counter accepts every u32, MAX_PAYLOAD_BYTES before allocation, big-endian, 14-byte prefix arithmetic, flag bits, wrapped key sub-layout, ext_length semantics, relay fields, signed range = publisher region || payload, AEAD AD = publisher region, reject tokens `key_bearing_discardable`/`unknown_version`, gop_counter rules, hop-local relay ids, vectors file wins — P2's v3 vectors path named as P2's). Existing §2 prose saying "which of the subscriber's slots", the mid-meeting-joiner downgrade paragraph, and Appendix "Version 2" get dated corrections. §4 cadence rows + key-bearing rule + video T/group-start/KEK receipt + nonce/replay per (KEK gen, sender, stream) + leaver W holds for video. §5 four controls, withdrawal on server mute with MH backstop, host-can-resume with notice, R-18 obligation + fail-safe; "MC keeps the send directive active ..." corrected. §6 flat table, truncation to UNAVAILABLE supersedes reject (and story 2 R-1), MC never names live video source, pending MH-internal with no give-up timeout (closes the Open Item), decode-before-active exception. §7 per-kind server mute in slot terms + attribution rule; each independent stream (simulcast later) its own id. §10 client pipeline-reuse gate. Open Items: switch-abandonment item closed by pointer. Addendum: story 3 line -> story file; story 4 resume/keyframe jitter; story 5 switch-completion line; new adaptive-capacity story; interim cost derivation. Appendix: delta summary, pointing at §2 rather than re-tabulating. Nothing says end-to-end/zero-trust; `key_custody=operator` untouched.

### Docs
- `docs/API_CONTRACTS.md`, `docs/WEBTRANSPORT_FLOW.md`: MH bidi stream after `MhConnectRequest` carries `RecoveryRequest` (client->MH) and `KeyframeRequest` (MH->client), same framing; stale v1 42-byte layouts replaced by a pointer to ADR §2; shadow restatements (max_receive_slots, slot states, header_version floor, "MC keeps the send directive", server_muted_sources) updated or turned into proto pointers.
- INDEX.md pointers updated (stay under its size cap).
- `docs/TODO.md` closures (`[x]` + CLOSED 2026-10-20 + pointer, original kept): (1) `SLOT_STATE_ZERO_REQUESTED may be unemittable` -> reserved, pointer to the `reserved` comment; (2) header-version allowlist/floor "neither task owns" -> owner MC-A, no allowlist/floor (v3-only refusal), both former sites fixed; (3) Media Path Obligations (e) single `transport_mode` echo -> `applied_source_streams`, MH-A1 retires the homogeneous-mode reject.
- Observability mirrors: `docs/observability/metrics/mc-service.md` value list + closed-TODO sentence, `infra/grafana/dashboards/mc-media.json` description + override regex, `docs/runbooks/client-dev-local.md` mention.
- `docs/observability/label-taxonomy.md:643` stale "eight-value" count: fixed in this devloop at observability's request (review F1); OBS-A's media_kind edits are separate.
- MC consumer comments at `assignments.rs:22/:407` about ZERO_REQUESTED: updated only where the code edit touches them; otherwise MC-A.

### Tests and gates
- Shared helper: `top_level_field_numbers` (with its nested `varint` and doc comment) is hoisted into `crates/proto-gen/tests/common/mod.rs` and used by both roundtrip files (DRY ruling).
- `signaling_roundtrip.rs`: optional mute (old-shape `false` case named `old_shape_false_is_indistinguishable_from_absent`, with a comment on the pre-P1-unmute-reads-as-unchanged hazard; the ZERO_REQUESTED raw number is one local constant citing the proto `reserved` line) 4 cells × 2 messages + byte-level tag-presence pin + old-shape `bool` bytes decode as `Some(true)` and old `false` decodes as None; reserved ZERO_REQUESTED (raw 5 preserved, `try_from` Err, `from_str_name` None); open-enum unknown 99 for SlotState, MediaKind (packed `publish_kinds`), Codec; slot-state distinctness rewritten over all named values with 8..11 pinned; publish_kinds order/duplicate preserved; receive_capability absent vs present-empty; per-kind caps absent/0/n; min_keyframe_interval_ms absent/0/500; VP8 == 5; StreamAssignment sender presence per state; both envelopes incl. 20/21, gop_counter 0 and u32::MAX; ConnectRequest unchanged.
- `internal_roundtrip.rs`: SourceStream all dispositions incl. UNSPECIFIED, open-enum 99 for SourceDisposition, order/dup preserved, echo swap case, empty echo emits no field 6; existing MutedSource tests stay.
- `verify-codegen.sh`: presence for every new name read from regenerated output; member-scoped absence helper for enum members (scoped to the `SlotState` enum block) with a positive control; absence list = ZERO_REQUESTED only; superseded-but-present names stay presence asserts.
- No `skip_debug` change.

### Expected Layer-State (declared per CONVENTIONS.md "Intentional wire breaks") — SUPERSEDED 2026-10-20, see Lead Decisions item 3: Layer 6 now expected green, 0 findings
Layer 6 `buf breaking` (FILE) — exactly **5 findings, 2 rule classes, all in `signaling.proto`, 0 in `internal.proto`** (measured by @operations on a scratch reproduction with buf 1.72; I will re-measure on the real diff before reporting):
- `ENUM_VALUE_NO_DELETE` ×1 — `SlotState` value 5 (`SLOT_STATE_ZERO_REQUESTED`) deleted/reserved.
- `FIELD_SAME_CARDINALITY` ×4 — `MuteRequest.audio_muted`, `MuteRequest.video_muted`, `ServerMuteRequest.audio_muted`, `ServerMuteRequest.video_muted` (implicit -> explicit presence).
Any sixth finding is a regression. No `breaking.ignore` entry will be added. Per CONVENTIONS.md a headless Lead may not accept this red: it must be escalated to the human. Every other layer is expected green.

### Rollback / safe-revert unit
The whole commit, reverted as one unit (proto + codegen + consumer compile fills move together; partial revert breaks the build). Safe backward: P1 changes no on-wire behaviour (new tags unread; optional-presence change is tag-compatible). P1 precedes the v3 cutover (P2), so it rolls back alone; the R-25 all-components-together rule binds from P2 onward. No text will claim P1 is part of the coordinated cutover.

### Questions for the Lead
1. D1 (consumer compile fills) — confirm the deviation from "no consumer edits beyond comments".
2. Layer 6 will be red by declaration (5 findings) — needs human acceptance per CONVENTIONS.md.

---

### Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed (after mute-list precedence rule added) |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Paired media-handler | confirmed (3 conditions on the transitional mute mechanism, all in plan) |
| Meeting-controller | confirmed |
| Client | confirmed |
| Semantic Guard | not spawned (no check surface) |

Classification-sanity guard: first run FAILED (`owner_not_in_manifest` ×2 — Owner "media-handler (co-owner with protocol)" on two `crates/media-protocol/**` rows); Lead normalised Owner to `media-handler`; re-run `STATUS=OK REASON=cross-boundary-classification-clean-1-files`.

### Lead Decisions (Gate 1)

1. **D1 — APPROVED (Lead).** "No consumer edits beyond comments" is internally infeasible with the frozen wire (prost/protobuf-es surface `optional bool`, the reserved enum value, new enum values and new fields at compile time). This is a prompt-internal contradiction, not a prompt-vs-story disagreement; the behaviour-preserving compile fills listed in the Cross-Boundary table are the most complete option that keeps "workspace builds and every existing test stays green". All five consumer-side reviewers concur.
2. **Layer 6 declared red — ESCALATED to the human.** The frozen wire (story Design §Wire freeze; task prompt items (4) reserve `SLOT_STATE_ZERO_REQUESTED`, (6) `optional bool` mute fields) mandates 5 `buf breaking` FILE findings. Per `docs/protocol/CONVENTIONS.md` "Intentional wire breaks", a headless Lead may not accept that red, and the pre-commit hook will not accept a non-PASS Gate-2 verdict. Implementing first and escalating at Gate 2 would produce a no-commit escalation whose work the runner cannot `--restart` from, so the devloop escalates here, before implementation, with an approved plan ready to execute.

3. **Layer-6 escalation RESOLVED by the human (2026-10-20).** Commit `205ee7ef` ("buf breaking: enforce wire compatibility, not source compatibility") changed `proto/buf.yaml` to `FILE` + `WIRE_JSON` minus `ENUM_VALUE_NO_DELETE`, `FIELD_SAME_CARDINALITY`, `FIELD_NO_DELETE`, and recorded the policy in `docs/protocol/CONVENTIONS.md` §Enforcement correction note. Measured there: P1's five FILE findings drop to **0**. The Expected Layer-State section above is therefore superseded: **Layer 6 `buf breaking` is expected GREEN with 0 findings; any finding is a regression.** No declaration needed (the change is not a wire break). The plan is otherwise unchanged; Gate 1 stands approved and the devloop resumes at implementation with a respawned roster.

## Human Review (Iteration 2)

**Feedback**: "This devloop was interrupted before completion. Resume from main.md state: finish incomplete phases, then gates and commit as normal."

## Pre-Work

None.

---

## Implementation Summary

### Signalling contract (`signaling.proto`)
| Item | Before | After |
|------|--------|-------|
| `Codec` | OPUS/VP9/AV1/H264 | + `CODEC_VP8 = 5` (declared only after both WebCodecs probes pass) |
| `JoinRequest` | no publish/receive declaration | `publish_kinds = 8`, `receive_capability = 9`; validation-then-neither-refusal on the declaration as sent; reconnect with a different set refused |
| `JoinResponse` | `max_receive_slots = 12` (total, reject) | 12 SUPERSEDED (MC-B); `max_audio_receive_slots = 13`, `max_video_receive_slots = 14` (informational, truncation) |
| `SlotState` | 7 states + UNSPECIFIED, "no value will be added" | flat enum: `AVATAR_NO_VIDEO = 8`, `UNAVAILABLE = 9`, `AVATAR_CLIENT_MUTED = 10`, `AVATAR_SERVER_MUTED = 11`; `SOURCE_MUTED` SUPERSEDED (MC-B); `ZERO_REQUESTED` reserved by number and name; retained states fail closed and count |
| `StreamAssignment.sender_id` | absent when no source | presence per state (absent on video ACTIVE, FEWER_SOURCES, UNAVAILABLE) |
| `MuteRequest` / `ServerMuteRequest` | `bool` per kind | `optional bool`; four controls; tag- but not semantics-compatible; `ServerMuteAction::from_request` defect named |
| `ParticipantMuteUpdate` | — | R-18 self-copy-before-directive obligation + client fail-safe |
| `EncodingParameters` | — | `optional uint32 min_keyframe_interval_ms = 6` (single normative home of `MEDIA_VIDEO_MIN_KEYFRAME_INTERVAL_MS`) |
| `SendStream`/`SendDirective` | "MC keeps directive active while client-muted"; floor language | watched-and-not-server-muted rule, withdrawal = empty targets, video = stream 1; dated correction; v3-only, no floor |
| `supported_header_versions` | allowlist + floor, "ENFORCEMENT NOT YET ALLOCATED" | v3-only refusal at join, owner MC-A; paragraph deleted |
| MH bidi stream | connect only | `MhClientMessage.recovery_request = 2` (`RecoveryRequest`), new `MhServerMessage { KeyframeRequest = 1; trace 20/21 }` with publisher and MH rules |
| Relay `stream_id` | "subscriber-scoped" | hop-local everywhere it is restated (uplink = MC send id, downlink = slot id) |

### Internal contract (`internal.proto`)
| Item | Before | After |
|------|--------|-------|
| `SourceStream` / `SourceDisposition` | — | new; `RegisterMeetingRequest.source_streams = 8`; attribution rule; absence = undeclared |
| `server_muted_sources` / `MutedSource` | sender-only mute | SUPERSEDED (MC-B); the ONE statement of the transitional path (empty list legacy reading + union/more-restrictive-wins overlap rule, counted `server_muted`) |
| `MutedSource` FORECLOSED block | "per-kind ingress mute is foreclosed" | dated 2026-10-13 correction; property stated in slot terms (a)/(b)/(c), signing dropped |
| `RegisterMeetingResponse` | single `transport_mode = 5` echo | 5 SUPERSEDED; `applied_source_streams = 6` of `SourceStreamEcho`, per stream, unconditional |
| `EgressStream` | `transport_mode = 6`; priority unspecified | 6 SUPERSEDED (derived from candidate's `SourceStream`); lower `priority_group` = higher, audio 1 / video 2; supersede true for video |
| IMMEDIATE HARD REJECTS list | story-2 set | + story-3 rejects; superseded/retired entries marked |

### Compile fills (D1, behaviour-preserving)
- MC: mute handlers read `Option<bool>` via `unwrap_or(false)` (interim comment naming MC-B); `JoinResponse` per-kind caps `None`; `RegisterMeetingRequest.source_streams: vec![]`; `Codec::Vp8` joins the `CodecNotAudio` arm; audio `min_keyframe_interval_ms: None`; `slot_state_label` + `SLOT_STATES` domain/witness gain the four new labels and drop `zero_requested`.
- MH: `applied_source_streams: vec![]`; a `RecoveryRequest` as the FIRST message is the existing generic invalid-connect reject.
- Tests/fixtures (mc, mh, env-tests, test-utils): absent values for new fields, `Some(..)` mute literals.
- SDK: `[Codec.VP8]: null` in `codecMap.ts`; the `SlotState.ZERO_REQUESTED` arm deleted from `SignalingClient.ts` (unknown/new values fall to `unspecified`, fail closed, until C-A1).
- `proto-gen` redacting `Debug` for `JoinRequest`/`JoinResponse` lists the new (non-secret) fields; no `skip_debug` change.

### Comment-only (hop-local relay id, slot-terms mute)
`crates/media-protocol/src/frame.rs` (module doc, obligations, key-id disclaimer, `STREAM_ID_FIELD_BYTES`, `stream_id()`), `codec.rs` (`rewrite_relay_region`, `MediaFrameParts.stream_id`), `crates/mh-service/src/routing/mod.rs` (identifier table, `StreamNumber`), `crates/mh-service/src/media/forward.rs` (FORECLOSED rewrite, dated), SDK `frameCodec.ts`, `keyId.ts`, `egress.ts`.

### Tests and gates
- `crates/proto-gen/tests/common/mod.rs`: hoisted `top_level_field_numbers`.
- `signaling_roundtrip.rs`: mute presence cells x2 messages, byte-level tag presence, old-shape true/false (`old_shape_false_is_indistinguishable_from_absent`), reserved ZERO_REQUESTED (number, name, raw 5), open-enum 99 (SlotState, packed MediaKind, Codec), flat-enum distinctness with 8..11 pinned, sender presence per state, publish_kinds order/dups, receive_capability absent/empty/populated, per-kind caps absent/0/n, Debug of new join fields, VP8 = 5, min keyframe interval absent/0/500, both MH envelopes (tags 1/2 + 20/21, gop 0 and u32::MAX), connect keeps tag 1.
- `internal_roundtrip.rs`: every disposition incl. UNSPECIFIED, order + duplicates, pinned numbers, open-enum 99, empty list emits no field 8, swap case (counts equal, per-stream distinct), empty echo emits no field 6.
- `verify-codegen.sh`: presence of every new name; member-scoped enum helpers (`assert_enum_member`, `assert_enum_member_absent` with a positive control); absence = `SlotState.ZERO_REQUESTED` only; `SOURCE_MUTED` stays a presence assert.
- Layer 6 `buf breaking`: GREEN, 0 findings (policy of `205ee7ef`); no `breaking.ignore`.

### Docs
- ADR-0036 2026-10-13 amendment bundle (§1, §2 normative v3 table + decode order, §4, §5, §6, §7, §8, §10, Open Items, Addendum incl. adaptive-capacity story, Appendix delta). `key_bearing_discardable` placed at decode step 5 (right after the reserved-flag check; needs only the flags byte), `gop_counter` read unchecked at step 8, `unknown_version` stays step 2.
- `docs/API_CONTRACTS.md`, `docs/WEBTRANSPORT_FLOW.md`: MH bidi stream messages, stale v1 layouts replaced by pointers to ADR §2, shadow restatements updated or turned into proto pointers (incl. stale `StreamAssignment` tags corrected; WEBTRANSPORT_FLOW "E2E Encryption" heading corrected; `MeetingKekUpdate` field 3 added).
- `docs/TODO.md`: three entries closed. Observability mirrors: `mc-service.md`, `mc-media.json` (drop `zero_requested`), `client-dev-local.md`. INDEX.md pointers.
- `label-taxonomy.md:643` count and `mc-incident-response.md` step-3 domain statement fixed at review (observability F1/F2, operations).

### Additional Changes
- Interpretation for security to confirm: group-start rotation and "first N frames" key carriage are scoped to VIDEO in ADR §4; audio rows (rotate every T, key on every frame) are unchanged.

---

## Files Modified

```
 crates/env-tests/src/fixtures/mc_session.rs        |   4 +
 crates/env-tests/src/fixtures/mh_grpc.rs           |   1 +
 crates/mc-service/src/grpc/mh_client.rs            |   5 +
 crates/mc-service/src/media_routing/confirm.rs     |   1 +
 .../mc-service/src/media_signaling/assignments.rs  |  25 +-
 crates/mc-service/src/media_signaling/directive.rs |   7 +-
 crates/mc-service/src/observability/metrics.rs     |  22 +-
 crates/mc-service/src/webtransport/connection.rs   |  40 +-
 crates/mc-service/tests/common/media_session.rs    |  10 +-
 crates/mc-service/tests/join_tests.rs              |  12 +-
 .../tests/media_admission_integration.rs           |   2 +
 .../tests/media_connection_update_integration.rs   |   2 +
 .../tests/otel_webtransport_integration.rs         |   2 +
 .../tests/webtransport_accept_loop_integration.rs  |   6 +-
 crates/mc-test-utils/src/mock_mh.rs                |   2 +
 crates/media-protocol/src/codec.rs                 |   8 +-
 crates/media-protocol/src/frame.rs                 |  39 +-
 crates/mh-service/src/grpc/mh_service.rs           |   4 +
 crates/mh-service/src/media/forward.rs             |  20 +-
 crates/mh-service/src/routing/mod.rs               |  14 +-
 crates/mh-service/src/webtransport/connection.rs   |  13 +
 crates/mh-service/tests/auth_layer_integration.rs  |   1 +
 crates/mh-service/tests/otel_grpc_integration.rs   |   2 +
 .../tests/register_meeting_integration.rs          |   1 +
 crates/mh-test-utils/src/media_policy.rs           |   3 +
 crates/proto-gen/src/lib.rs                        |   7 +
 crates/proto-gen/tests/internal_roundtrip.rs       | 256 ++++++++--
 crates/proto-gen/tests/signaling_roundtrip.rs      | 448 ++++++++++++++++-
 docs/API_CONTRACTS.md                              | 146 ++++--
 docs/TODO.md                                       |   6 +-
 docs/WEBTRANSPORT_FLOW.md                          |  70 ++-
 docs/decisions/adr-0036-media-flow.md              | 382 ++++++++++++++-
 docs/observability/metrics/mc-service.md           |   6 +-
 docs/runbooks/client-dev-local.md                  |   2 +-
 docs/specialist-knowledge/protocol/INDEX.md        |  10 +-
 infra/grafana/dashboards/mc-media.json             |   4 +-
 packages/proto-gen/scripts/verify-codegen.sh       |  89 ++++
 packages/sdk-core/src/media/frame/frameCodec.ts    |   4 +-
 packages/sdk-core/src/media/frame/keyId.ts         |  10 +-
 packages/sdk-core/src/media/pipeline/egress.ts     |   7 +-
 packages/sdk-core/src/signaling/SignalingClient.ts |   2 -
 packages/sdk-core/src/signaling/codecMap.ts        |   4 +
 proto/dark_tower/internal/v1/internal.proto        | 255 +++++++++-
 proto/dark_tower/signaling/v1/signaling.proto      | 529 +++++++++++++++++----
 44 files changed, 2186 insertions(+), 297 deletions(-)
crates/proto-gen/tests/common/mod.rs (new)
```

---

## Devloop Verification Steps

Gate 2: `DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` (unattended run-all).

| Attempt | L1 | L2 | L3 | L4 | L5 | L6 | L7 | TOTAL |
|---------|----|----|----|----|----|----|----|-------|
| 1 | OK | OK | OK | OK | OK | OK (`buf-breaking-passed`, 0 findings) | FAIL `env-tests-failed` | FAIL |
| 2 | OK (5s) | OK (3s) | OK (139s) | OK (150s) | OK (2s) | OK (1s) | OK (841s) | **OK** (1141s) |

- **L7 attempt 1**: env-tests did not compile — `crates/env-tests/tests/26_mh_quic.rs:1317` `RegisterMeetingRequest` missing `source_streams` (feature-gated, so L1 never built it). Fixed with the absent value, plus the `ServerMuteRequest` literal in `35_mc_server_mute_teardown.rs`; implementer then ran `cargo check --workspace --all-features --tests` clean.
- **L6**: `buf breaking` 0 findings under the 205ee7ef wire-compat policy, no `breaking.ignore`.
- L3 budget note: `WARN BUDGET_TOTAL_BREACH GUARD_AUDIT_DURATION=139 BUDGET=90` (pre-existing guard-suite cost, not diff-caused; same on fast checks).

---

## Code Review Results

### Gate 3 — Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 2 | 2 | 0 | slot-terms property generalised to every stream of a server-muted kind; ADR §6 decode-before-active scoped to video-muted senders. Confirmed R-14 reading (only video cadence changes) |
| Test | RESOLVED-FIXED | 4 | 4 | 0 | byte-level frozen-tag pins; MH RecoveryRequest-first test; SDK VP8/8..11 tests; `slot_state_label` pinned to `as_str_name()` |
| Observability | RESOLVED-DEFERRED | 4 | 3 | 1 | label-taxonomy count, runbook count, metrics.rs doc fixed; F4 dead `'zero_requested'` SDK/web-app token deferred to C-A1 |
| Code Quality | RESOLVED-FIXED | 7 | 7 | 0 | comment reflows, stale "all three" comment, test doc, dashboard regex |
| DRY | RESOLVED-FIXED | 5 | 5 | 0 | missed restatement sites (media-vector-gen, mh-service.md, runbook), `roundtrip`/`UNKNOWN_ENUM_VALUE` hoisted, forward.rs cut to pointer |
| Operations | RESOLVED-FIXED | 1 | 1 | 0 | mc-incident-response.md "all eight variants" |
| Paired media-handler | RESOLVED-FIXED | 4 | 4 | 0 | missed sibling sites; forward.rs rewording; MH reject test; "every connection-bound sender" wording |
| Meeting-controller | RESOLVED-FIXED | 2 | 2 | 0 | proto INTERIM-UNTIL-MC-B note; metrics.rs wrap; story sequencing gap fixed (below) |
| Client | RESOLVED-FIXED | 3 | 3 | 0 | ADR §2 step 13 arithmetic; encode-side rule; 3 stale SDK "MC keeps directive active" comments |
| Semantic Guard | not spawned | — | — | — | no check surface |

### Lead Decisions (Gate 3)

1. **C-A1 / MC-B mute sequencing gap (raised by meeting-controller).** C-A1 ran before MC-B and was told to send only the changed mute kind; P1's interim MC shim reads absent as `false`, so an audio-muted participant toggling video would be recorded audio-unmuted. Lead chose option (a), fixed in-tree in the story file: C-A1 (and C-A2's R-19 MuteRequest) send BOTH kinds on every request; MC-B owns the switch to changed-kind-only once its absent=unchanged parsing lands. Residual (already MC-B's): host video-only server mute folds to mute-both until MC-B (`ServerMuteAction::from_request`).
2. **Observability F4 deferral accepted.** Removing `'zero_requested'` alone would be a partial edit of the SDK slot-state union that C-A1's prompt explicitly rewrites (also removing `source_muted`); nothing can produce the token (mapper arm removed, a test pins wire 5 to `'unspecified'`). Client (Gate 1), operations and DRY concur.

## Accepted Deferrals

- `docs/TODO.md` §Client Architecture — dead `'zero_requested'` SDK token, removed by C-A1

---

## Rollback Procedure

1. Start commit: `8a17b2e56711b446d9c0e66385b3df5dcabeb74a` (resume base `205ee7ef`).
2. `git revert <this commit>`.
3. No schema or infra apply involved.
4. **Safe-revert unit**: the whole commit. Proto + consumer compile fills + tests move together (a partial revert breaks the build). P1 changes no on-wire behaviour and precedes the v3 cutover (P2), so it rolls back alone.

---

## Issues Encountered & Resolutions

### Issue 1: Gate-1 escalation on declared Layer-6 red
**Problem**: Under `breaking.use: [FILE]` the frozen wire produced 5 source-only findings that a headless Lead may not accept.
**Resolution**: The human switched `proto/buf.yaml` to wire-compat rules (205ee7ef); the devloop resumed and Layer 6 passed with 0 findings.

### Issue 2: "team-lead" address unreachable in the resumed session
**Problem**: Teammates could not message `team-lead`.
**Resolution**: Lead is addressed as `main`; all verdicts were delivered there.

---

## Lessons Learned

1. A "sweep every restatement" plan still missed ~10 sites (media-vector-gen, SDK lifecycle comments, runbooks); reviewer sweeps of the tree were what completed the invariant.
2. Making a field `optional` is wire-compatible but creates a sequencing hazard between producer and consumer tasks; check the story's task order against interim semantics.

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
