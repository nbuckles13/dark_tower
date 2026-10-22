# Devloop Output: Story 3 MC-A — video slots, flat slot states, video directives, join publish set, per-stream source list

**Date**: 2026-10-22
**Task**: run-story task #12 (MC-A) of `docs/user-stories/2026-10-07-see-each-other.md`; full task text: `~/.cache/devloop/story-runs/story-runner/2026-10-07-see-each-other/task-12.prompt`
**Specialist**: meeting-controller
**Mode**: Agent Teams (v2) — full, Gate-1 present
**Branch**: `feature/user-see-each-other`
**Duration**: ~5h (2 Gate-2 attempts)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `628344b584f28ff801a2b9e967a23c1aea34ddae` |
| Branch | `feature/user-see-each-other` |
| Lead Model | `claude-opus-5-5[1m]` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` |
| Implementing Specialist | `meeting-controller` |
| Tier | `full` |
| Iteration | `2` (Gate 2 attempt 1 failed: L3 scope-drift, L7 env-24 + env-34) |
| Security | `security` |
| Test | `test` (paired: --paired-with=test, expanded role) |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Semantic Guard | `semantic-guard` |
| Paired | `paired-infrastructure`, `paired-client` |
| Conditional | `protocol` (GSA: `proto/signaling.proto` comment) |

---

## Task Overview

### Objective
MC-A per task #12 prompt: per-kind video/audio slot vectors with truncation to per-kind caps, flat slot-state vocabulary for both kinds, video send directives, join publish set validation, per-stream `source_streams` in MH registration, echo check per stream, new MC series, env-tests 27/34, config key move `MC_VIDEO_MIN_KEYFRAME_INTERVAL_MS`.

### Scope
- **Service(s)**: mc-service (primary); infra config (paired infrastructure); web-app e2e fixture (paired client); proto comment (protocol, GSA)
- **Schema**: No
- **Cross-cutting**: Yes

### Debate Decision
NOT NEEDED — governed by ADR-0036 as amended by P1 and the story's Design §Wire freeze.

---

## Cross-Boundary Classification

Every file in the diff has a row. Mine = meeting-controller. GSA rows are never Mechanical.

| Path | Classification | Owner | Notes |
|------|----------------|-------|-------|
| `.env.example` | Not mine, Domain-judgment | infrastructure | key move, retired-key prose; authored by @paired-infrastructure |
| `crates/env-tests/README.md` | Not mine, Domain-judgment | test | story 3 MC-A env-test scenarios / kind-aware fixtures; authored by @test |
| `crates/env-tests/src/fixtures/egress_admission.rs` | Not mine, Domain-judgment | test | story 3 MC-A env-test scenarios / kind-aware fixtures; authored by @test |
| `crates/env-tests/src/fixtures/mc_session.rs` | Not mine, Domain-judgment | test | story 3 MC-A env-test scenarios / kind-aware fixtures; authored by @test |
| `crates/env-tests/src/fixtures/participant.rs` | Not mine, Domain-judgment | test | story 3 MC-A env-test scenarios / kind-aware fixtures; authored by @test |
| `crates/env-tests/src/fixtures/video/mod.rs` | Not mine, Domain-judgment | test | story 3 MC-A env-test scenarios / kind-aware fixtures; authored by @test |
| `crates/env-tests/src/fixtures/video/probe.rs` | Not mine, Domain-judgment | test | story 3 MC-A env-test scenarios / kind-aware fixtures; authored by @test |
| `crates/env-tests/tests/01_mh_deployment_config.rs` | Not mine, Domain-judgment | test | key move / retired-key reads; with infrastructure; authored by @test |
| `crates/env-tests/tests/02_mc_deployment_config.rs` | Not mine, Domain-judgment | test | key move / retired-key reads; with infrastructure; authored by @test |
| `crates/env-tests/tests/26_mh_quic.rs` | Not mine, Domain-judgment | test | story 3 MC-A env-test scenarios / kind-aware fixtures; authored by @test |
| `crates/env-tests/tests/27_mc_slot_placement.rs` | Not mine, Domain-judgment | test | story 3 MC-A env-test scenarios / kind-aware fixtures; authored by @test |
| `crates/env-tests/tests/28_mh_egress_admission.rs` | Not mine, Domain-judgment | test | story 3 MC-A env-test scenarios / kind-aware fixtures; authored by @test |
| `crates/env-tests/tests/24_join_flow.rs` | Not mine, Domain-judgment | test | story 3 env-test adaptation (edge-sum header / join-turn view) |
| `crates/env-tests/tests/29_mh_meeting_teardown.rs` | Not mine, Domain-judgment | test | story 3 env-test adaptation (edge-sum header / join-turn view) |
| `crates/env-tests/tests/34_mc_kek_rotation.rs` | Not mine, Domain-judgment | test | story 3 MC-A env-test scenarios / kind-aware fixtures; authored by @test |
| `crates/env-tests/tests/35_mc_server_mute_teardown.rs` | Not mine, Domain-judgment | test | story 3 MC-A env-test scenarios / kind-aware fixtures; authored by @test |
| `crates/env-tests/tests/36_mc_media_kind_labels.rs` | Not mine, Domain-judgment | test | story 3 MC-A env-test scenarios / kind-aware fixtures; authored by @test |
| `crates/env-tests/tests/37_mh_stream_attribution.rs` | Not mine, Domain-judgment | test | story 3 MC-A env-test scenarios / kind-aware fixtures; authored by @test |
| `crates/env-tests/tests/38_mh_video_forwarding.rs` | Not mine, Domain-judgment | test | story 3 MC-A env-test scenarios / kind-aware fixtures; authored by @test |
| `crates/env-tests/tests/39_mh_video_keyframes.rs` | Not mine, Domain-judgment | test | story 3 MC-A env-test scenarios / kind-aware fixtures; authored by @test |
| `crates/mc-service/src/actors/controller.rs` | Mine | — |  |
| `crates/mc-service/src/actors/meeting.rs` | Mine | — |  |
| `crates/mc-service/src/actors/meeting_media.rs` | Mine | — |  |
| `crates/mc-service/src/actors/messages.rs` | Mine | — |  |
| `crates/mc-service/src/config.rs` | Mine | — |  |
| `crates/mc-service/src/errors.rs` | Mine | — |  |
| `crates/mc-service/src/grpc/gc_client.rs` | Mine | — |  |
| `crates/mc-service/src/grpc/mh_client.rs` | Mine | — |  |
| `crates/mc-service/src/lib.rs` | Mine | — |  |
| `crates/mc-service/src/main.rs` | Mine | — |  |
| `crates/mc-service/src/media_kind.rs` | Mine | — |  |
| `crates/mc-service/src/media_routing/assignment.rs` | Mine | — |  |
| `crates/mc-service/src/media_routing/confirm.rs` | Mine | — |  |
| `crates/mc-service/src/media_routing/generation.rs` | Mine | — |  |
| `crates/mc-service/src/media_routing/mod.rs` | Mine | — |  |
| `crates/mc-service/src/media_routing/pusher.rs` | Mine | — |  |
| `crates/mc-service/src/media_routing/slots.rs` | Mine | — |  |
| `crates/mc-service/src/media_signaling/assignments.rs` | Mine | — |  |
| `crates/mc-service/src/media_signaling/capability.rs` | Mine | — |  |
| `crates/mc-service/src/media_signaling/directive.rs` | Mine | — |  |
| `crates/mc-service/src/media_signaling/mod.rs` | Mine | — |  |
| `crates/mc-service/src/media_signaling/outcome.rs` | Mine | — |  |
| `crates/mc-service/src/observability/metrics.rs` | Mine | — |  |
| `crates/mc-service/src/webtransport/connection.rs` | Mine | — |  |
| `crates/mc-service/src/webtransport/server.rs` | Mine | — |  |
| `crates/mc-service/tests/common/media_session.rs` | Mine | — |  |
| `crates/mc-service/tests/common/mod.rs` | Mine | — |  |
| `crates/mc-service/tests/gc_integration.rs` | Mine | — |  |
| `crates/mc-service/tests/join_tests.rs` | Mine | — |  |
| `crates/mc-service/tests/kek_rotation_integration.rs` | Mine | — |  |
| `crates/mc-service/tests/media_admission_integration.rs` | Mine | — |  |
| `crates/mc-service/tests/media_client_signaling_integration.rs` | Mine | — |  |
| `crates/mc-service/tests/media_connection_update_integration.rs` | Mine | — |  |
| `crates/mc-service/tests/media_policy_push_integration.rs` | Mine | — |  |
| `crates/mc-service/tests/otel_grpc_outbound_integration.rs` | Mine | — |  |
| `crates/mc-service/tests/otel_webtransport_integration.rs` | Mine | — |  |
| `crates/mc-service/tests/server_mute_integration.rs` | Mine | — |  |
| `crates/mc-service/tests/slot_placement_integration.rs` | Mine | — |  |
| `crates/mc-service/tests/video_slots_integration.rs` | Mine | — |  |
| `crates/mc-service/tests/webtransport_accept_loop_integration.rs` | Mine | — |  |
| `crates/mc-test-utils/src/media.rs` | Mine | — |  |
| `crates/mc-test-utils/src/mock_mh.rs` | Mine | — |  |
| `crates/mh-service/src/config.rs` | Not mine, Minor-judgment | media-handler | doc line names `MC_VIDEO_MIN_KEYFRAME_INTERVAL_MS`; authored by @paired-infrastructure |
| `docs/API_CONTRACTS.md` | Not mine, Minor-judgment | protocol | JoinRequest/JoinResponse rows |
| `docs/DEVELOPMENT.md` | Not mine, Domain-judgment | infrastructure | key move, retired-key prose; authored by @paired-infrastructure |
| `docs/TODO.md` | Not mine, Minor-judgment | lead | status notes on entries this task changes |
| `docs/observability/metrics/client.md` | Not mine, Minor-judgment | observability | new/reshaped series rows and basic panels |
| `docs/observability/metrics/mc-service.md` | Not mine, Minor-judgment | observability | new/reshaped series rows and basic panels |
| `docs/runbooks/client-dev-local.md` | Not mine, Domain-judgment | operations | premise sweep, key rows, join-token triage |
| `docs/runbooks/mc-deployment.md` | Not mine, Domain-judgment | operations | premise sweep, key rows, join-token triage |
| `docs/runbooks/mc-incident-response.md` | Not mine, Domain-judgment | operations | premise sweep, key rows, join-token triage |
| `docs/runbooks/mh-deployment.md` | Not mine, Domain-judgment | operations | premise sweep, key rows, join-token triage |
| `docs/specialist-knowledge/meeting-controller/INDEX.md` | Mine | — |  |
| `infra/grafana/dashboards/mc-media.json` | Not mine, Minor-judgment | observability | new/reshaped series rows and basic panels |
| `infra/services/mc-service/config.env` | Not mine, Domain-judgment | infrastructure | key move, retired-key prose; authored by @paired-infrastructure |
| `infra/services/mc-service/mc-0-deployment.yaml` | Not mine, Domain-judgment | infrastructure | key move, retired-key prose; authored by @paired-infrastructure |
| `infra/services/mc-service/mc-1-deployment.yaml` | Not mine, Domain-judgment | infrastructure | key move, retired-key prose; authored by @paired-infrastructure |
| `infra/services/media-codec-profile/profile.env` | Not mine, Domain-judgment | infrastructure | key move, retired-key prose; authored by @paired-infrastructure |
| `infra/services/mh-service/mh-0-deployment.yaml` | Not mine, Domain-judgment | infrastructure | key move, retired-key prose; authored by @paired-infrastructure |
| `infra/services/mh-service/mh-1-deployment.yaml` | Not mine, Domain-judgment | infrastructure | key move, retired-key prose; authored by @paired-infrastructure |
| `packages/sdk-core/src/config/clientConfig.ts` | Not mine, Minor-judgment | client | comment-only premise deletions; authored by @paired-client |
| `packages/web-app/e2e/README.md` | Not mine, Domain-judgment | client | strict per-kind cap check / premise prose; authored by @paired-client |
| `packages/web-app/e2e/cohort.ts` | Not mine, Domain-judgment | client | strict per-kind cap check / premise prose; authored by @paired-client |
| `packages/web-app/e2e/fixtures.ts` | Not mine, Domain-judgment | client | strict per-kind cap check / premise prose; authored by @paired-client |
| `packages/web-app/e2e/multi-party-hear.spec.ts` | Not mine, Domain-judgment | client | strict per-kind cap check / premise prose; authored by @paired-client |
| `packages/web-app/e2e/solo-participant.spec.ts` | Not mine, Domain-judgment | client | strict per-kind cap check / premise prose; authored by @paired-client |
| `packages/web-app/src/lib/config.ts` | Not mine, Minor-judgment | client | comment-only premise deletions; authored by @paired-client |
| `packages/web-app/src/lib/participantState.ts` | Not mine, Minor-judgment | client | comment-only premise deletions; authored by @paired-client |
| `proto/dark_tower/signaling/v1/signaling.proto` | Not mine, Domain-judgment (GSA wire format) | protocol | COMMENT ONLY (keyframe-interval home, tag-12 preamble, l.1020); security in review; authored by @protocol |
| `scripts/dev-web.sh` | Not mine, Domain-judgment | infrastructure | key move, retired-key prose; authored by @paired-infrastructure |
| `scripts/dev-web.test.sh` | Not mine, Domain-judgment | infrastructure | key move, retired-key prose; authored by @paired-infrastructure |

---

## Planning

### Mechanism restatement

Instance framing: "add video slots". Mechanism: **every per-participant media rule in MC becomes a function of (participant, kind)**: the slot vector, cap, truncation, eligibility, edge stream, egress id, directive stream, source-stream entry + disposition, slot state, mute projection and every kind-labelled series. The invariant to complete is "no MC code path assumes audio-only", plus the paired config/doc/test premise sweep ("until MC-A" text) that becomes false when this lands. Wider class (all "until MC-A" premises, retired-key readers in other crates) is taken in-loop, owners paired (DRY B/C/D, ops 3, infra 1-8, client 1-3).

### Order of work (one devloop; subagents for doc/infra sweeps after approval)

1. **Vocabulary + parse** (`media_signaling/capability.rs`, `outcome.rs`)
   - Kind type: reuse `MediaKindLabel` (index/pick/per_kind/ALL) everywhere in the slot table and composition — no third kind enum (DRY E). A new `served_kind(proto) -> Result<MediaKindLabel, CapabilityOutcome>` closed match is the ONE wire->served-kind mapping: AUDIO->Audio, VIDEO_CAMERA->Video, VIDEO_SCREEN->`media_kind_unsupported`, UNSPECIFIED/unknown i32->`media_kind_unspecified` (existing token). `from_proto` stays for labelling only; its "ONE mapping" doc is narrowed to "label projection" and points at `served_kind` for admission.
   - `PublishSet` (typed `[bool; 2]` by `MediaKindLabel::index`, private fields, `parse(&[i32])`): closed set {AUDIO, VIDEO_CAMERA}; UNSPECIFIED, SCREEN, unknown, duplicate -> `Err` (join token `publish_kind_invalid`); empty = receive-only. Order-insensitive equality (reconnect compare).
   - `ReceiveCapabilityDeclaration::parse(msg, budget)`: (S1, per Lead ruling 1) no count-based refusal; work is bounded by the 64 KiB signalling message limit cited at the parse site. (S2) every slot validated (u16 range, ONE duplicate namespace across kinds, served kind, pins). `SlotCountOverCap` removed (enum, ALL, zero-init, catalog, dashboard).
   - Truncation is a separate pure step `declaration.truncate(caps) -> TruncatedDeclaration` keeping the first `cap` slots of each kind in declaration order; excess ids recorded as unavailable. Neither check uses the declaration AS SENT (`declares_any()`), before truncation.
   - New `CapabilityOutcome` tokens: `media_kind_unsupported`, `would_publish_and_receive_nothing`; `slot_count_over_cap` removed.

2. **Join path** (`webtransport/connection.rs`), all AFTER JWT + meeting_id binding + identity key (S3, client f), BEFORE `join_connection` (nothing allocated), in this order (ops 1):
   1. header version: `capabilities.supported_header_versions` contains `PROTOCOL_VERSION` (absent capabilities = empty = refused) -> `header_version_unsupported`;
   2. `publish_kinds` -> `publish_kind_invalid`;
   3. join-carried `receive_capability` parse (absent = zero slots) -> join refused; the specific token is counted on `mc_media_receive_capability_declarations_total`, and the join failure on `mc_session_join_failures_total{error_type="receive_capability_invalid"}` (Lead ruling 2: adopted);
   4. neither check (empty publish set AND zero declared slots as sent) -> `publishes_and_receives_nothing`.
   Each is a typed `McError` variant whose `error_type_label()` feeds `JOIN_FAILURE_ERROR_TYPES` + the enum-tie test (obs 4); client sees `ERROR_CODE_INVALID_REQUEST` + one generic string per class; logs carry only the token and counts, never `{:?}` of JoinRequest/JoinResponse (sg 1).
   - The parsed `PublishSet` + truncated declaration ride `ConnectionJoin` into `handle_join`: publish set stored on the roster `Participant` (fixed for the membership) and on the slot-table member; the declaration registered (`media.declare`) in the SAME actor turn before the first reconcile, so the joiner gets its directive + assignments queued behind JoinResponse and the mute replay (client b). The connection's `MediaSignalingContext` records the join declaration, so the SDK's identical post-join redeclaration is `accepted_unchanged` (client a; integration test), and holds the publish set for the redeclaration neither check (connection-local, no actor hop; counted `would_publish_and_receive_nothing`, previous declaration kept byte-identical — S6).
   - JoinResponse: tags 13/14 from the SAME `ClientMediaConfig` caps the gauge reads; **tag 12 no longer written** (protocol ruling); test asserts `max_receive_slots == None`, both per-kind `Some`.
   - **Reconnect (S5)**: enforced in actor `handle_reconnect`; `ConnectionReconnect` gains the presented `PublishSet` and optional declaration. Set compare against the roster entry BEFORE any binding rotation / Reconnected broadcast; mismatch -> `McError::ReconnectPublishSetMismatch`, counted `reconnect_publish_set_mismatch` (recorded at the actor arm, the only caller), entry untouched. A reconnect's declaration is a redeclaration (neither -> kept previous). Documented: not production-reachable until reconnect is wired to the WT accept path (actor-seam tests only). No "same sub = reconnect" rule.

3. **Config** (`config.rs`, `media_signaling/mod.rs`, `main.rs`): required `MC_MAX_AUDIO_RECEIVE_SLOTS` 1..=64, `MC_MAX_VIDEO_RECEIVE_SLOTS` 0..=64 (range constants named so S1 derives from them), `MEDIA_AUDIO_{CODEC,MAX_BITRATE_BPS,FRAME_RATE_HZ}`, `MEDIA_VIDEO_{CODEC,WIDTH,HEIGHT,FRAME_RATE_HZ,MAX_BITRATE_BPS}`, `MC_VIDEO_MIN_KEYFRAME_INTERVAL_MS`. Errors name key, value and bound (sg 5). Reads of `MC_MAX_RECEIVE_SLOTS` / `MC_AUDIO_*` removed. Startup log names `max_audio_receive_slots`, `max_video_receive_slots`, `video_codec`, `video_width`, `video_height`, `video_frame_rate_hz`, `video_max_bitrate_bps`, `video_min_keyframe_interval_ms` (+ audio fields) — runbook post-deploy checklist uses these exact names (ops 3). The future Kind test profile (320x180/15/250000) is inside the bounds. That patch is INF-C's (task 16) and does not exist yet: Kind runs the production 640x360/30/800000 in this task, and env-test 27's edge-sum header holds at production (infra a). Module doc: caps read once at startup; live vectors never re-truncated; the R-26 lever applies through the MC restart (everyone reconnects).

4. **Directive** (`directive.rs`): `VideoEncoding` (private fields, validating `new`; VP8 only; 16..=1920 w/h; 1..=60 fps; 100000..=4000000 bps; `MIN_KEYFRAME_INTERVAL_MS_FLOOR: u32 = 100` / `_CEILING: u32 = 10_000` single-literal constants, floor comment naming `validate-keyframe-floor-sync.sh`, the SDK floor in `clientConfig.ts` and `keyframe_min_interval_clamped`). Shared codec parse knows `vp8`; each encoding checks its kind. `MediaStreamPolicy{audio, video}` keyed by `MAIN_AUDIO_STREAM_NUMBER`/`MAIN_VIDEO_STREAM_NUMBER`. `build_send_directive(publisher, publish_set, assignment, handlers, policy)`: EVERY published kind ALWAYS listed (audio stream 0 first), unpublished never; targets = handlers owning that stream's rendered out-edges, transport per kind (DATAGRAM / STREAM_PER_GROUP); unwatched = listed with empty targets. Video carries all six fields; audio leaves `min_keyframe_interval_ms` absent. Still no mute parameter (module boundary unchanged).

5. **Slot table** (`slots.rs`, `assignment.rs`): `Member{ sender, publishes: PublishSet, connected, slot_ids: [Vec<u16>;2], slots: [Vec<Option<Edge>>;2] }` (per kind via `MediaKindLabel::index`). Eligibility per kind: never self (explicit for both kinds — S7a); shared connected handler; audio requires `publishes(Audio)`; video any participant. Fill for an empty slot of kind K: (1) earliest joiner holding a slot of the other kind with S but none of K; else (2) earliest eligible holding none of K. Refill: `KIND_FILL_ORDER = [Audio, Video]`, one slot per kind per pass, repeat to a fixed point. Stability: join, leave, mute, other-kind redeclaration, unrelated connectivity never move an occupied slot; leave frees only the leaver's slots; same-count redeclaration keeps occupants by ordinal. Edge repair on connectivity per kind as today.
   - `egress_stream_id(subscriber, kind, ordinal)` layout: bits 0-7 ordinal (cap <= 64), bits 8-23 subscriber `sender_id` (u16), **bit 24 kind (0 audio, 1 video)** — audio ids are byte-identical to story 2 (so M=0 audio is identical), injective across (subscriber, kind, ordinal); `unpack` (kind + ordinal) beside the pack; collision test over both kinds.
   - `MAIN_VIDEO_STREAM_NUMBER: u8 = 1`, `VIDEO_PRIORITY_GROUP: u32 = 2` beside their audio siblings (contract constants, not config); pinned audio 0 / video 1 in directive AND `source_streams` tests.
   - Render: an edge plan only for an audio slot, or a video slot whose occupant publishes video (no edge for AVATAR_NO_VIDEO / UNAVAILABLE / FEWER / unreachable — S7d). Video: candidate (occupant, 1), `supersede_on_independent_frame = true`, priority 2. `EgressStream.transport_mode` no longer written (UNSPECIFIED). `source_streams` per handler: one entry per (sender, stream) in EVERY member's publish set (superset of directed streams; bound 2 x roster vs `MH_MAX_SOURCE_STREAMS_PER_MEETING`=1024, checked against MC's participant cap in the module doc), DATAGRAM for 0 / STREAM_PER_GROUP for 1, disposition DROP_AT_INGRESS if that kind server-muted, else PAUSED if client-muted, else FORWARD. Render takes a required per-sender `SourceFlags` projection (server/client per kind) explicitly projected from the roster (no roster struct by value — sg 2). Old sender-only `server_muted_sources` written exactly as story 2. Render invariants tested: one entry per (sender, stream); every candidate listed; one transport per (sender, stream); edge stream kind == slot kind (S7b). Generation advances only on output change (disposition change advances; identical re-render does not).
   - **Model check**: extend the existing exhaustive BFS (no proptest/new dep): <=4 participants, depth ~6 (capped with named deeper cases if slow; never random), ops = join with publish set {A, V, AV, none}, leave, per-kind client/server mute flips, redeclare per kind counts {0,1,2, over cap}, connectivity. Asserted after every step: no self; no sender twice per kind per subscriber; audio slots hold audio publishers only; each fill picks rule-(1) else earliest rule-(2) (via the oracle); occupied slot changes only by occupant leave / pair unreachable / that kind's redeclaration; over-cap slots UNAVAILABLE; every edge handler in both connected sets; edge stream kind == slot kind; render invariants. **Independent expected-occupant oracle** in the test module, written from the rule text, sharing no code with the fill (stated at its definition — DRY G; not ported from `slotOracle.ts`).

6. **Slot states** (`assignments.rs`): composed from the slot table's per-kind view (occupant, handler, publish set) + truncated declaration + per-kind `SourceMuteView` (client and server flags kept apart; replaces audio-only projection). Video: ACTIVE (no sender — no field derived from the sender either; S8), AVATAR_NO_VIDEO (named, no url; precedence over mutes), AVATAR_CLIENT_MUTED / AVATAR_SERVER_MUTED (named, server wins; url = the edge's handler, since the edge still exists), FEWER_SOURCES (no sender, empty url), UNAVAILABLE (declared id echoed, no sender, empty url). Audio: ACTIVE names the sender; mute avatars as video; never AVATAR_NO_VIDEO. Nothing emits SOURCE_MUTED (unit assertion over the model check's states). `slot_state_label` keeps mirroring every wire variant (incl. `source_muted` until MC-B).
   - Mute of EITHER kind (client or server) re-renders (dispositions) and dirties the holders of that source across both kinds; each holder re-emitted only if its view changed, under the existing `SLOT_VIEW_FLUSH_BATCH`. A story-2 client's video_muted toggle with no video published changes nothing (render identical -> no generation advance; views identical -> no emission).

7. **Echo check** (`confirm.rs`, `mh_client.rs`): `PushExpectation` carries the sent `source_streams`; compare `applied_source_streams` per (sender, stream) unconditionally; arms `absent_from_echo`, `echoed_unspecified`, `mode_differs` all -> existing `transport_mode_mismatch` at the same precedence; the legacy single `transport_mode` compare is removed (MH echoes UNSPECIFIED once `source_streams` is declared). Confirm-failure `error!` gains `arm` and `stream_number` fields (no labels, no sender_id). `mock_mh` echoes `applied_source_streams` from the applied request.

8. **Metrics** (`metrics.rs`, catalog, `mc-media.json`): video kind at the slot-state and send-target call sites; `set_receive_slot_cap` publishes BOTH kinds from the same values as tags 13/14 (video 0 renders as a sample; doc + catalog row rewritten); `mc_media_send_directive_withdrawals_total{media_kind, reason}` with closed `WithdrawalReason{Unwatched, ServerMuted}` (ALL/label, full cross product zero-initialised; `server_muted` emitted by MC-B), counted in `flush_one` only on a DELIVERED directive change where that kind's stream went non-empty -> empty targets vs `last_directive` (never on a re-render with empty targets; reconnect's reset view does not count; counted on DELIVERY, unlike `send_directives_total`, which counts at composition. That asymmetry is documented in the doc and the catalog: a withdrawal whose delivery failed shows up only on `slot_view_emissions{delivery_failed}`) — test pins no double count; `mc_media_video_keyframe_min_interval_seconds` gauge set once at boot from the validated `VideoEncoding` (ms/1000; doc says it is config, not GoP timing). All carry `key_custody=operator`, no identity labels, present-at-zero test, basic panel, catalog row. No truncation counter, no GoP/frame timing/mute history in MC logs or labels.

9. **Tests**
   - Unit: model check + oracle; truncation order; parse rejects + ceiling; join validation order (v2-only + no publish_kinds counts `header_version_unsupported` — ops 1); `VideoEncoding` bounds; stream-number pins; slot-state mapping both kinds incl. server-wins + no-video precedence + video ACTIVE senderless; `egress_stream_id` no collision; render invariants; echo arms; withdrawal no-double-count.
   - Integration (`crates/mc-service/tests/`): harness join defaults gain `publish_kinds=[AUDIO]`, `supported_header_versions=[3]` (existing tests stay audio); R-2 refusals at join and on redeclaration with tokens; truncation to UNAVAILABLE with ids echoed; R-8 watched-only video directive, withdrawal on last watcher's leave (`unwatched` counted once) and restore; R-26 with `MC_MAX_VIDEO_RECEIVE_SLOTS=0` (video-only receive-only client ADMITTED, every video slot UNAVAILABLE, no video targets, audio identical to a no-video run); disposition change advances generation, identical re-render does not; join-carried + identical redeclaration = `accepted_unchanged`; reconnect mismatch at the actor seam; flipped `slot_placement_integration.rs:1152` (video cap now published).
   - Env-tests (with @test; per-entity wire evidence only, no sleeps/wall-clock gates): 27 gains publish mixes (A, V, AV, receive-only -> AVATAR_NO_VIDEO; neither refused), video over-subscription past the Kind cap 4 -> UNAVAILABLE with filled slots matching an expectation written from the rule text, pairing + stability across join/leave, watched-only video directive + withdrawal, two-handler video visibility, v2-only client refused; header records concurrent edge sum <= 72. 34 gains: leave while video is directed and flowing (T-A `VideoPublisher`/`VideoSubscriber`) rotates the KEK to post-removal members and does NOT re-issue/withdraw/restart any video directive. Env-test fixtures' join defaults updated. 26's transitional test re-pointed/renamed with @test (Assertion Vacuity #5); 28/01 re-pointed to `MC_MAX_AUDIO_RECEIVE_SLOTS` (S9 is audio-only; pigeonhole stays in `egress_admission.rs`).

10. **Config move + premise sweep** (paired owners; handed to them after approval where they offered, otherwise subagents under my ownership): infra items 1-8 (profile.env -> 8 keys; config.env new key + rationale citing constant names; HARD refs on mc-0/mc-1 to `mc-service-config`; retired refs stay optional; env-test 02 positive + profile-absence check; env-test 01 REMOVED_KEYS doc + profile check; `.env.example`; DEVELOPMENT.md; dev-web.sh/test; mh yaml prose; mh config.rs doc key name). Runbooks per operations 2-4 (row 875 rewritten incl. "the one-time move rolled MH; edits now roll MC only"; rows 869/873/1157-1161; checklist field names; "until MC-A" sweep in mc-deployment, mc-incident-response, mh-deployment, client-dev-local). Client 1-3 (fixtures.ts strict for BOTH kinds; README; four comment deletions). Proto comment per protocol's text + tag-12 preamble sentence. Catalog/dashboard per observability 1-8. `docs/API_CONTRACTS.md` retired-total prose. INDEX.md.



### Lead rulings (2026-10-22)
1. `declaration_too_large`: **NOT adopted.** R-1 says an over-cap declaration is truncated and is never a failure. The parse work bound is the existing signalling message-size limit, which `read_framed_message` caps at 64 KiB. Its constant is cited at the parse site. Parse plus the duplicate check is O(bytes) of an already size-bounded message. There is no count-based refusal: each kind is truncated to its cap, keeping the first `cap` slots in declaration order. Step 1 is amended accordingly. Security may escalate at Gate 1 if it holds this is a hard safety line.
2. `receive_capability_invalid` on `mc_session_join_failures_total`: **ADOPTED** as an additional zero-initialised token. The specific cause stays on the capability series. The catalog says these are two views of one event and that only the join series is alerted on. The mc-incident-response triage entries cover all five join tokens (ops).
3. Mute-avatar url = the edge's handler: **ADOPTED** (R-5 decode-before-live). NO_VIDEO, UNAVAILABLE and FEWER carry an empty url and no edge.
4. Env-tests 01/28 and `egress_admission.rs` re-point (this PULLS FORWARD a step story l.773 assigns to INF-C, so INF-C must not redo it) to `MC_MAX_AUDIO_RECEIVE_SLOTS` in-loop. Ceiling-gauge re-sizing is left to INF-C (task 16).


### Post-ruling review notes carried into implementation
- Security: the join-carried declaration uses the SAME parser as the redeclaration path (no second entry point). The `publish_kinds` parse fails on the first invalid or duplicate value, with no collect-then-check. `supported_header_versions` uses a linear `contains`. UNAVAILABLE ids are stored once per declaration as plain ids, with no fill, edge or render work. The module doc states the amplification: about 64 KiB of input gives about 10k UNAVAILABLE entries, re-emitted only to that holder. A max-size declaration test (close to `MAX_MESSAGE_SIZE` of minimal slots) shows the client is admitted and truncated, and that the StreamAssignments encodes and is emitted. If the encoded view would exceed the client's framed-read bound, the composition fails loudly and is counted (`composition_failed` + a `send_directives_total` stage) rather than wedging the stream.
- DRY: `served_kind` and `from_proto` each name the other. A unit test walks every proto `MediaKind` and checks that `served_kind` Ok(k) implies `from_proto` is Some(k). The oracle restates `[Audio, Video]` as its own literal, mirroring the rule text and `slotOracle.ts:KIND_FILL_ORDER`, and imports nothing from the fill.
- Test: C1 parity stays in env-test 02. The reconnect mismatch test asserts the roster entry keeps its original publish set.
- Ops: mc-incident-response gets one-line triage entries for all five join tokens.
- Observability: the dashboard items (a)-(d) and a full grep of the catalog and dashboards for `until MC-A`, `MC_MAX_RECEIVE_SLOTS` and `source_muted`.

- Code quality: CQ1 is accepted. `MediaKindLabel`, `served_kind` and `PublishSet` move to a new leaf module `crates/mc-service/src/media_kind.rs`, re-exported from `media_signaling`, so the OBS-A call sites don't change. That keeps the dependency one-way (`media_signaling` -> `media_routing` -> `media_kind`). The `Video` variant doc is reworded: `from_proto` is the label projection, and `served_kind` is admission, where SCREEN is refused. CQ2 is accepted: infallible `of_ref`/`of_mut` accessors sit beside `of`/`index`, with no `[kind.index()]` sites. CQ3 is accepted: a pure `validate_join_media(&JoinRequest, caps) -> Result<ValidatedJoinMedia, McError>` runs after JWT/identity and before `join_connection`, and its ordering is unit-tested. The `McError` variant carries the `CapabilityOutcome`, so one value feeds both counters.
- Client and protocol: a unit test pins that mute avatars carry the edge's url and that NO_VIDEO, UNAVAILABLE and FEWER have an empty one. Protocol authors the signaling.proto edits and handles codegen; I don't touch those files.

### Test-specialist items (accepted as written; @test F1-F2, M1-M5, U1-U3, E1-E7, K1, C1)
- Fixtures: `mc_session::join_request` defaults to publish=[AUDIO], `supported_header_versions=[PROTOCOL_VERSION]` (derived), receive_capability absent; explicit join-declaration builder; `try_mc_join` for refusals. `participant.rs` helpers take a `media_kind` filter; every existing caller passes AUDIO.
- Model check: oracle state carried in the BFS `World`, updated from the op alone; EXACT per-slot occupant AND state equality after every step; canonical key includes publish sets, per-kind mute flags, declared counts and per-kind vectors; oracle self-tests with literal prompt examples; positive-control counters (rule-(1)!=rule-(2) fills, order-mattered events, UNAVAILABLE seen, non-video occupant in video slot, state-changing mute flips, per-op transitions); caps audio=1/video=1 with counts {0,1,2}, named R-26 cap-0 case, adversarial chooser kept.
- K1 FIFO: confirmed by code. `push_kek_update` (`conn.send_kek_update`) and the view flush (`connection.send`) both enqueue on the SAME participant-actor mailbox, in meeting-actor turn order; the participant actor writes its one outbound stream in mailbox order. A redeclaration after the KEK update therefore yields a StreamAssignments that is ordered after any directive the leave turn emitted, so "no SendDirective before the fence" is sound.
- Env-test 02: interval key moves to MC_CONFIGMAP rows; retired keys tolerated as optional per INF-A's step; per-instance parity of `mc_media_receive_slot_cap{media_kind}` and `mc_media_video_keyframe_min_interval_seconds` against the ConfigMap lands in 27 if 02 must stay Prometheus-free.

### Authorship split (after "Plan approved")
- meeting-controller (me + my subagents): all `crates/mc-service/**`, `crates/mc-test-utils/**`, catalog + dashboard, runbook sweep (operations reviews), API_CONTRACTS/TODO, INDEX.
- @test: `crates/env-tests/src/fixtures/{mc_session,participant}.rs`, env-tests 27 and 34 (co-designed), 26 re-point, 01/02/28 + `egress_admission.rs` edits (01/02 with @paired-infrastructure).
- @paired-infrastructure: profile.env, mc config.env, mc-0/mc-1 + mh-0/mh-1 yaml, `.env.example`, DEVELOPMENT.md, dev-web.sh/.test.sh, mh-service config.rs doc line.
- @paired-client: fixtures.ts (both kinds strict), e2e/README.md, cohort.ts, clientConfig.ts, web-app config.ts, participantState.ts.
- @protocol: the signaling.proto comment edits (+ codegen check).

### Explicitly out of scope (owner + reason)
- Per-kind mute request parsing (absent = unchanged), directive withdrawal on server mute (`server_muted` reason emission), deletion of superseded proto fields, MH legacy path / `TRANSITIONAL_STREAM`, SDK shims and their comments: MC-B (task 15) / C-A2 (task 13) by the story plan.
- Wiring ADR-0023 reconnect to the WT accept path: not in any task; the publish-set check is enforced at the actor seam and documented as such.
- OBS-B layout/alerts beyond the basic panels.

### Open points
Resolved by the Lead rulings above.

### Gate 1

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test (paired) | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Semantic Guard | confirmed |
| Infrastructure (paired) | confirmed |
| Client (paired) | confirmed |
| Protocol (GSA) | confirmed |

---

## Pre-Work

None

---

## Implementation Summary

**MC code** (`crates/mc-service`)
- `media_kind.rs` (new leaf module): `MediaKindLabel` (moved here and re-exported; adds `of_ref`/`of_mut`/`other`/`to_proto`), `served_kind` (closed admission: SCREEN refused), and `PublishSet`.
- `media_signaling/capability.rs`: the parse validates every slot (one id namespace), refuses SCREEN with `media_kind_unsupported`, and has no count refusal. The bound is the 64 KiB frame. `SlotDemand` truncates each kind to its cap; the excess becomes UNAVAILABLE.
- `outcome.rs`: `SlotCountOverCap` removed; `MediaKindUnsupported`, `WouldPublishAndReceiveNothing` and `WithdrawalReason` added. `applied_no_recompose` is documented as unreachable.
- `directive.rs`: `VideoEncoding` (validated, private fields, VP8 only) and the floor/ceiling constants. The publish set is always listed, with empty targets when unwatched. Video targets are STREAM_PER_GROUP and carry all six encoding fields. The directive still has no mute path.
- `media_routing/slots.rs`:
  - per-kind vectors with explicit self-exclusion; audio slots hold audio publishers only; any participant can fill a video slot;
  - rule (1)/(2), `KIND_FILL_ORDER` audio-first to a fixed point;
  - the render emits a video edge only for a video publisher (stream 1, supersede, priority 2), stops writing `EgressStream.transport_mode`, builds `source_streams` for every published stream with dispositions, and keeps the legacy muted set computed from audio streams;
  - the exhaustive two-kind model check (3 people to depth 4, 4 people to depth 3, two choosers) runs against an INDEPENDENT oracle: exact occupant and wire-state equality, positive-control counters, and named deeper cases.
- `assignment.rs`:
  - `MAIN_VIDEO_STREAM_NUMBER`, `VIDEO_PRIORITY_GROUP`, and the `stream_number_of`/`transport_of`/`priority_group_of` helpers;
  - `SourceStreamPlan` with its dispositions;
  - `egress_stream_id` puts the kind in bit 24, so audio ids are unchanged; unpack sits beside the pack, with a collision test.
- `assignments.rs`: the flat state table for both kinds. Server mute wins, NO_VIDEO takes precedence, live video names no sender, and a mute avatar keeps its url. `SourceMuteView` is per kind, client and server.
- `confirm.rs` and `mh_client.rs`: `source_streams` is encoded and the echo is compared per stream (`first_echo_mismatch` with three arms, all mapping to `transport_mode_mismatch`, plus the `echo_arm`/`echo_stream_number` log fields). The legacy single echo is no longer compared.
- `webtransport/connection.rs`:
  - pure `validate_join_media`: header version, then publish set, then declaration, then the neither check;
  - five join tokens counted before the reply;
  - the join-carried declaration seeds the redeclaration short-circuit; the redeclaration neither-check is connection-local;
  - JoinResponse sets tags 13 and 14 and no longer writes tag 12.
- Actors: the publish set is fixed on the roster. The join turn registers the declaration. A self- or server-mute of either kind reconciles. The reconnect publish-set check happens before any state change. Withdrawals are counted on a delivered non-empty→empty transition. A view larger than the frame is refused at composition and counted.
- `config.rs`, `main.rs` and `server.rs`: per-kind caps, `MEDIA_*` profile keys and `MC_VIDEO_MIN_KEYFRAME_INTERVAL_MS`. The retired keys are no longer read. The startup log carries the new fields. Both cap gauges and the keyframe gauge are published at boot.
- `metrics.rs`: the withdrawals counter (full cross product zero-initialised), the keyframe gauge, the join tokens and the present-at-zero checks.

**Tests**
- Unit tests as planned.
- `mock_mh` echoes `applied_source_streams` from the applied snapshot.
- New `tests/video_slots_integration.rs` covers:
  - join refusals, each with a twin;
  - the redeclaration neither-refusal;
  - the shim-a no-op;
  - video states;
  - the watched-only directive, its withdrawal (counted once) and restore;
  - the R-26 lever;
  - the reconnect mismatch at the actor seam;
  - a frame-sized declaration;
  - the gauges.
- The existing suites were adapted to the join-turn view and the flat states.

**Owner-authored**
- env-test fixtures and 01/02/26/27/28/34/35–39: @test. Not yet run on Layer 7.
- Infra config, yaml, `.env`, DEVELOPMENT and dev-web: @paired-infrastructure.
- e2e fixtures and comments: @paired-client.
- `signaling.proto` comments: @protocol.
- Runbooks, catalog, dashboard, API_CONTRACTS, TODO and INDEX: a meeting-controller subagent, reviewed by operations and observability.

`./scripts/layer-fast.sh`: green, all six layers.

---

## Code Review Results

### Gate 3

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 2 | 2 | 0 | F1 reconnect neither-redeclaration uncounted; F2 oversize-view guard measured payload not framed envelope |
| Test (paired) | RESOLVED-FIXED | 8 | 8 | 0 | T1-T3 sleep/vacuous negatives; T4 video-mute re-emit test; T5 source_streams wire pins; T6 model-check expectations from prod helpers; T7 kind-order self-test; T8 fmt. Lead ruling: edge-sum headers added to 26/28/29/35/36 |
| Observability | RESOLVED-FIXED | 1 | 1 | 0 | reconnect_publish_set_mismatch failure-only counting documented (remove when reconnect wired to join path) |
| Code Quality | RESOLVED-FIXED | 9 | 9 | 0 | stale mute docs; single neither predicate; silent reconnect path; single fail-closed mute default; shared parse_codec; constants in error text; O(n) echo index; private TransportModeOrd; shared count_join |
| DRY | RESOLVED-FIXED | 1 | 1 | 0 | env-test 37 stale "until MC-A" header. No TODO extraction entries |
| Operations | RESOLVED-FIXED | 1 | 1 | 0 | config.env rationale for outright key deletion + runbook rollback rule |
| Semantic Guard | RESOLVED-FIXED (native SAFE after fix) | 1 | 1 | 0 | [metrics-path-completeness] meeting.rs handle_reconnect uncounted neither + `.is_ok()` masking |
| Infrastructure (paired) | CLEAR | 0 | 0 | 0 | env-config positive control verified; Kind render checked |
| Client (paired) | CLEAR | 0 | 0 | 0 | no C-A1 gap found |
| Protocol (GSA) | CLEAR | 0 | 0 | 0 | protocol authored signaling.proto comment edits incl. review-time tag-12 preamble broadening (covered by its CLEAR) |

Ownership Lens: Domain-judgment rows (test, infrastructure, client, operations) had owners in plan + review; GSA `signaling.proto` (comment-only) authored by protocol with security in plan + review.

---

## Accepted Deferrals

- (none surfaced in this devloop)

---

## Rollback Procedure

1. Start commit: `628344b584f28ff801a2b9e967a23c1aea34ddae`
2. `git diff 628344b5..HEAD`
3. Safe-revert unit: **Answer**: the whole commit — image AND manifests together. A one-version MC rollback to the pre-MC-A image within the v3 era works only because (a) the retired keys `MC_MAX_RECEIVE_SLOTS`/`MC_AUDIO_*` are still in `mc-service-config` with optional refs, (b) the old MC does not read `MC_VIDEO_MIN_KEYFRAME_INTERVAL_MS` (an extra key is harmless), and (c) MH treats a registration with no `source_streams` as the legacy path. The profile-key removal rolls MH once (hash change).

---

## Issues Encountered & Resolutions

- Lead note: teammate prompts point each teammate at its `docs/specialist-knowledge/{name}/INDEX.md` and `review-protocol.md` to Read first (INDEX files total ~138KB) rather than inlining contents.
- Gate 2 attempt 1 (Lead): L3 scope-drift (env-test 29 row missing) and L7 env-tests 24 (join-turn directive/view before ParticipantJoined — test adapted to the documented ordering contract) and 34 (test dropped C's MC+MH sessions together → partial-connectivity widen/narrow of A's targets; test re-sequenced, assertion unchanged). No MC code change.
- Observation (not a finding against this diff, filed for task 17 / meeting-controller): `docs/TODO.md` §Media Path Obligations — leave with MH-first close widens then narrows remaining publishers' targets.

## Gate 2 Result

Attempt 2: `DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` — L1-L7 OK, `TOTAL_RESULT=OK` (env-tests all pass; browser E2E 17 passed, `browser-e2e-passed`). Attempt 1 failed (see Issues).
