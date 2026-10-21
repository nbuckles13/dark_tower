# Devloop Output: MH-A1 per-stream ingress attribution

**Date**: 2026-10-21
**Task**: Story 3 task 8 (MH-A1): per-stream ingress attribution against source_streams, per-stream server mute + PAUSED, mixed-transport registrations, derived egress costs
**Specialist**: media-handler
**Mode**: Agent Teams (v2) — full, Gate-1 present <!-- panel mode + Gate-1 tier (ADR-0037 §D2); see the Tier row in Loop State -->
**Branch**: `feature/user-see-each-other`
**Duration**: ~3h

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `1898d9471cfcaf31b1e90a506dde1f973f402892` |
| Branch | `feature/user-see-each-other` |
| Lead Model | `claude-opus-5-5[1m]` |

---

## Loop State (Internal)

<!-- This section is maintained by the Lead for state recovery after interruption. -->
<!-- Do not edit manually - the Lead updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer (media-handler)` |
| Implementing Specialist | `media-handler` |
| Tier | `full` |
| Iteration | `2` |
| Security | `security (RESOLVED-FIXED)` |
| Test | `test (RESOLVED-FIXED)` |
| Observability | `observability (RESOLVED-FIXED)` |
| Code Quality | `code-reviewer (RESOLVED-FIXED)` |
| DRY | `dry-reviewer (RESOLVED-FIXED)` |
| Operations | `operations (RESOLVED-FIXED)` |
| Semantic Guard | `semantic-guard (CLEAR)` |
| Paired Protocol | `paired-protocol (RESOLVED-FIXED)` |

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
Story 3 (`docs/user-stories/2026-10-07-see-each-other.md`) task #8, MH-A1. Full task text: `/home/dev/.cache/devloop/story-runs/story-runner/2026-10-07-see-each-other/task-8.prompt` (headless run-story). Per-stream ingress attribution against `source_streams`, DROP_AT_INGRESS / PAUSED dispositions, mixed-transport registrations, apply-time rejects, derived egress costs from `media-codec-profile`, telemetry, env-tests.

### Scope
- **Service(s)**: mh-service (+ env-tests, mh-test-utils, dt-guard env_config, infra config.env, observability docs/dashboard)
- **Schema**: No
- **Cross-cutting**: No (MH only; MC emission is task 12)

### Debate Decision
NOT NEEDED - governed by ADR-0036 as amended by task 1 and the story's Design §Wire freeze

{If debate was needed, link to debate record: `docs/debates/2026-10-21-{topic}.md`}

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
| `crates/mh-service/src/routing/mod.rs` | Mine | — |
| `crates/mh-service/src/routing/sources.rs` (new) | Mine | — |
| `crates/mh-service/src/routing/tests.rs` (new) | Mine | — |
| `crates/mh-service/src/media/forwarder.rs` | Mine | — |
| `crates/mh-service/src/media/forward.rs` | Mine | — |
| `crates/mh-service/src/grpc/mh_service.rs` | Mine | — |
| `crates/mh-service/src/session/mod.rs` | Mine | — |
| `crates/mh-service/src/config.rs` | Mine | — |
| `crates/mh-service/src/main.rs` | Mine | — |
| `crates/mh-service/src/observability/metrics.rs` | Mine | — |
| `crates/mh-service/tests/**` | Mine | — |
| `crates/mh-test-utils/src/**` | Mine | — |
| `docs/specialist-knowledge/media-handler/INDEX.md` | Mine | — |
| `infra/services/mh-service/config.env` | Mine | — |
| `proto/dark_tower/internal/v1/internal.proto` (comments only) | Not mine, Domain-judgment | protocol |
| `crates/env-tests/tests/37_mh_stream_attribution.rs` (new) | Not mine, Minor-judgment | test |
| `crates/env-tests/tests/01_mh_deployment_config.rs` | Not mine, Minor-judgment | test |
| `crates/env-tests/tests/26_mh_quic.rs` | Not mine, Minor-judgment | test |
| `crates/env-tests/tests/29_mh_meeting_teardown.rs` | Not mine, Minor-judgment | test |
| `crates/env-tests/src/fixtures/injection.rs` (new; hoisted from 26) | Not mine, Minor-judgment | test |
| `crates/env-tests/src/fixtures/mod.rs` | Not mine, Mechanical | — |
| `crates/env-tests/src/fixtures/egress_admission.rs` | Not mine, Minor-judgment | test |
| `infra/services/mh-service/mh-0-deployment.yaml` (comments only) | Not mine, Minor-judgment | infrastructure |
| `infra/services/mh-service/mh-1-deployment.yaml` (comments only) | Not mine, Minor-judgment | infrastructure |
| `infra/services/media-codec-profile/profile.env` (comments only) | Not mine, Minor-judgment | infrastructure |
| `infra/kubernetes/overlays/kind/services/mh-service/configmap-egress-budget-patch.yaml` | Not mine, Minor-judgment | infrastructure |
| `docs/observability/metrics/mh-service.md` | Not mine, Minor-judgment | observability |
| `docs/observability/label-taxonomy.md` | Not mine, Minor-judgment | observability |
| `infra/grafana/dashboards/mh-media.json` | Not mine, Minor-judgment | observability |
| `docs/TODO.md` | Not mine, Mechanical | — |
| `docs/runbooks/mh-deployment.md` | Not mine, Minor-judgment | operations |
| `crates/mc-service/src/media_routing/assignment.rs` (comment only) | Not mine, Minor-judgment | meeting-controller |
| `infra/grafana/dashboards/mh-overview.json` | Not mine, Minor-judgment | observability |
| `crates/env-tests/src/fixtures/kube.rs` | Not mine, Mechanical | — |
| `crates/env-tests/tests/02_mc_deployment_config.rs` | Not mine, Mechanical | — |
| `.env.example` | Not mine, Mechanical | — |
| `docs/DEVELOPMENT.md` | Not mine, Mechanical | — |
| `docs/specialist-knowledge/code-reviewer/INDEX.md` | Not mine, Mechanical | — |


---

## Planning

### Mechanism restatement
Instance: "attribute datagrams by relay id against `source_streams`". Mechanism: **every ingress frame resolves to exactly one declared (bound sender, uplink id, transport) or is dropped; routing state is keyed by that declared source, never by sender alone.** The wider class is the sender-keyed `by_sender` edge index: if attribution only gated disposition while fan-out still walked the sender's edges, the relabel case would reach the other kind's slots. So the index itself moves to (sender, stream), and every per-source fact (disposition, transport, all-muted, the future GoP state) lives in `MeetingRoutes`, bounded and freed with it. Task 9's stream path calls the same lookup.

### 1. Data model + lookup (`routing/mod.rs`)
- `MeetingPolicy { meeting, generation, edges, declarations: Declarations }`. `transport_mode` and `server_muted` leave `MeetingPolicy`.
- `Declarations` is an enum: `Declared { streams: HashMap<(SenderId, StreamNumber), SourceDecl{transport, disposition}>, union_muted: HashSet<SenderId>, echo order Vec }`, or `Transitional(TransitionalStory2 { muted: HashSet<SenderId>, echo: Vec<SenderId> })`.
- `MeetingRoutes` replaces `by_sender` with `by_source: HashMap<(SenderId, StreamNumber), Vec<usize>>`, which is still meeting-local and keeps the `.get` discipline. It precomputes `all_muted: HashSet<SenderId>` at build: senders with at least 1 declared stream where every stream is DROP_AT_INGRESS, unioned with the legacy `server_muted_sources`. This set is non-vacuous; a sender with zero declared streams is NOT muted.
- **The single lookup, used by both task 8 and task 9**: `RoutingSnapshot::attribute(&MeetingKey, SenderId, uplink_id: u16, arrival: TransportMode) -> Attribution<'_>`:
  `NoPolicy | ServerMuted | Unmatched | TransportMismatch | Route { paused: bool, edges: SourceEdges<'_> }`.
  The order is: absent meeting -> `NoPolicy`; sender in `all_muted` -> `ServerMuted` (shortcut, first); `StreamNumber::try_from(u16)` failure (>255) -> `Unmatched` (never `as u8`); undeclared -> `Unmatched`; transport != declared -> `TransportMismatch`; DROP_AT_INGRESS -> `ServerMuted`; otherwise `Route`. Only `Route` carries an edge iterator (`SourceEdges`: borrowing, non-allocating, over the source's meeting-local indices). The drop variants carry no edges, so the stream path cannot route a dropped stream by forgetting a check (security S1).
- `RoutingSnapshot::is_sender_server_muted(&MeetingKey, SenderId)`: the shortcut alone, so `forward_one` can keep running it before decode. `attribute` calls the same private predicate. `for_each_source` and `is_server_muted` are deleted (no sender-only traversal remains).
- Echo: `MeetingRoutes::applied_source_streams() -> Vec<SourceStreamEcho>` is built from the INSTALLED routes. Declared path: every declared stream with its transport, in request order, not filtered by disposition. Legacy path: one `{sender, 0, DATAGRAM}` per distinct sender NAMED (candidates, then muted senders), first-appearance order. `legacy_transport_echo()` echoes field 5. Legacy: DATAGRAM if any edge was applied, else UNSPECIFIED. Declared: UNSPECIFIED always (protocol P1). The divergence detector compares declarations as sets (replaces `server_muted_matches`).
- An edge's transport: derived from its candidates' declaration. A zero-candidate egress stays legal and carries no transport.
- priority_group docstring: lower number = higher priority (audio 1, video 2). Recorded only; task 9 maps it to stream priority.

### 2. Apply-time rejects (`MeetingPolicy::from_request`)
The order is load-bearing:
1. Counts, before any per-element work: egress <= bound; candidates per egress <= bound; `source_streams.len()` <= `MH_MAX_SOURCE_STREAMS_PER_MEETING`; `server_muted_sources.len()` <= the same bound (its own key's read is gone; protocol P6).
2. Egress shape/widths/duplicates (unchanged).
3. Muted set: width + duplicate rejects (unchanged; these are the superseded entries).
4. Declarations:
   - Non-empty `source_streams`: per element `SenderId::from_wire`, `StreamNumber::from_wire`, `TransportMode::try_from` and `SourceDisposition::try_from`. UNSPECIFIED **or an unknown int** -> `rejected_invalid`, never a default. A duplicate (sender, stream) -> `rejected_invalid`.
   - Empty `source_streams`: `transitional_story2_reading()`.
5. Egress-vs-declarations: a candidate absent from the declarations -> `TransportInconsistent` (`rejected_transport_inconsistent`). Candidates of one egress with differing declared transports -> the same token.
   - On the declared path `EgressStream.transport_mode` (field 6) is IGNORED, because MC-A stops writing it.
   - On the legacy path field 6 keeps story-2's UNSPECIFIED/unknown -> `rejected_invalid`. A non-DATAGRAM value -> `rejected_transport_inconsistent`, because it contradicts the synthesized DATAGRAM declaration (protocol P2).
   - The undeclared-candidate rule is written generically. After MC-B deletes the legacy branch, an empty list with egresses rejects as `rejected_transport_inconsistent` with no new token.
- `HeterogeneousTransportModes` is removed. New `PolicyRejection::{UndeclaredCandidate, InconsistentCandidateTransports, DuplicateSourceStream, UnspecifiedDisposition, UnspecifiedSourceTransport, TooManySourceStreams}`.
  - `PolicyRejection::outcome()` maps the two inconsistency rejections to the new `PolicyApplyOutcome::RejectedTransportInconsistent` (added to `ALL`, so it is zero-initialised) and everything else to `RejectedInvalid`.
  - The reasons stay `&'static str` and carry no ids.
- **The legacy function**: `fn transitional_story2_reading(...)`, commented exactly "transitional: deleted by MC-B with the superseded proto fields". It returns `Declarations::Transitional`. The rule is applied at LOOKUP time: any bound sender, label 0, DATAGRAM, so an unwatched sender drops `no_subscriber`, never `unmatched_stream`. The muted senders are DROP_AT_INGRESS, which the shortcut covers. Nothing is materialized per bound sender, so there is nothing to bound.
  - Its only other appearances are the `Transitional` match arms, which the compiler forces MC-B to delete with the type. No other code constructs it or reads stream 0.
  - The comment also carries the deploy-order invariant (operations 6): this MH must be fully rolled out before MC-A ships source_streams, and rolling MH back below this commit after MC-A is unsafe. The runbook wording belongs to tasks 4/19.
- Union rule (proto field 7, NON-EMPTY bullet): on the declared path, a sender in `server_muted_sources` joins `all_muted`. Its frames are `server_muted`, unattributable ones included. Its streams are still echoed.

### 3. Forward path (`media/forward.rs`)
The order in `forward_one` is:
1. `is_sender_server_muted` on the spawn-bound sender, BEFORE decode. This has no client-reachable input, and it keeps runbook 19's signal.
2. Sampler, then `decode_datagram`.
3. `attribute(meeting, sender, view.stream_id(), Datagram)` reads the relay id through the codec view, never an offset. The payload and the key id are never read.
4. Drops: `Unmatched` -> `unmatched_stream`, `TransportMismatch` -> `transport_mismatch`, `ServerMuted` -> `server_muted`, `NoPolicy` -> `no_policy`. All are Ingress-direction, so no hop number is consumed.
5. `Route{paused}`: if paused, `paused_source_frames().increment(1)`. Then the existing fan-out over that source's edges only, with an unchanged `no_subscriber` / `forwarded{ingress}` accounting identity (`forwarded{ingress}` is once per frame, never per edge).
- Hop state stays keyed by `egress_stream_id` in `ConnectionForwarder`. Nothing client-labelled keys it, so re-pushes never reset it (the test pins this).
- Comment rewrite (R-36): the FORECLOSED / audio-only text becomes the slot-terms property, pointing at `internal.proto MutedSource` as its single home. The label is publisher-set and unauthenticated; signing it would protect nothing because the publisher sets both the label and the key id. No comment claims that MH identifies a frame's true stream.

### 4. Derived costs (`config.rs`)
- `AUDIO_FRAME_OVERHEAD_BYTES = SFRAME_OBJECT_OVERHEAD_BYTES + NOMINAL_HEADER_BYTES + SIGNATURE_SIZE` (= 160). `NOMINAL_AUDIO_FRAME_BYTES` is rewritten as `NOMINAL_OPUS_PAYLOAD_BYTES + AUDIO_FRAME_OVERHEAD_BYTES`, so there is one enumeration (DRY 1).
- `VIDEO_FRAME_OVERHEAD_BYTES = AUDIO_FRAME_OVERHEAD_BYTES - WRAPPED_TRANSMIT_KEY_SIZE` (= 110; the wrapped key amortises to zero for video).
- `stream_cost_bps(max_bitrate, frame_rate, overhead_bytes) = max_bitrate + overhead_bytes*8*frame_rate`, with checked arithmetic.
  - Audio at the profile (48000, 50) = **112 000 bps**.
  - Video at Kind/test (250k, 15) = 263 200.
  - Video at prod (800k, 30) = 826 400.
  - NOTE: the prompt and story say audio is "about 90 kbps". That figure is ADR-0036 §4's nominal estimate (~174 B + 50 B at the nominal bitrate). The prompt's own formula at MAX_BITRATE gives 112 kbps, and the TODO entry this closes is precisely that 90k under-costs audio. I pin the formula's value (112 000), and the test also spells it out independently. Raised to @team-lead.
- `EgressAdmission::derive` now takes the derived costs. The `stream_cost_*` fields are now derived (doc updated). The type-blind max is kept, and the gauges keep publishing the enforced values. There is no headroom knob.
- Required reads `MEDIA_AUDIO_MAX_BITRATE_BPS`, `MEDIA_AUDIO_FRAME_RATE_HZ`, `MEDIA_VIDEO_MAX_BITRATE_BPS`, `MEDIA_VIDEO_FRAME_RATE_HZ` are each `MissingEnvVar("LITERAL")` at the call site (DRY 4; env-config rule 1 sees them).
  - Boot refusals: audio bitrate outside `SUPPORTED_AUDIO_BITRATE_FLOOR_BPS..=CEILING_BPS`; frame rate != `1000 / AUDIO_FRAME_DURATION_MS`. Video values must be non-zero and must not overflow.
  - New `ConfigError` variants name the key, the received value and the envelope, and say that the profile is the shared `media-codec-profile`: "revert that ConfigMap; it rolls MC and MH together" (operations 3).
- The `MH_STREAM_COST_*` and `MH_MAX_MUTED_SOURCES_PER_MEETING` reads are removed. If one is still set, there is an INFO startup line "retired key set and ignored" (operations 7).
- `MH_MAX_SOURCE_STREAMS_PER_MEETING` is a REQUIRED read with no code default; the deployed value 1024 lives in config.env, and "default 1024" means the deployed value (operations 2).
  - Bounds `1..=MAX_SOURCE_STREAMS_PER_MEETING_CEILING` (4096).
  - The boot refusal if it is below `MH_MAX_EGRESS_STREAMS_PER_MEETING` (renamed variant) is kept.
  - **Ceiling conflict, raised to lead**: the existing tripwire `const _: assert!(muted ceiling >= egress ceiling)` exists so that no egress value MH accepts is unpairable. With source ceiling 4096 < egress ceiling 8192 it fails. Proposal: lower `MAX_EGRESS_STREAMS_PER_MEETING_CEILING` to 4096, then keep the tripwire as `source ceiling >= egress ceiling`. The deployed egress value is 512, so nothing deployed moves.
- `PolicyLimits.max_muted_sources_per_meeting` -> `max_source_streams_per_meeting`. Gauge `mh_media_muted_sources_per_meeting_limit` is replaced by `mh_media_source_streams_per_meeting_limit`.
- `EGRESS_STREAM_CEILING_RECOMMENDED_MIN = demo_streams(DEMO_PARTICIPANTS=6, DEMO_AUDIO_SLOTS=5, DEMO_VIDEO_SLOTS=4) = 54`, with `demo_streams(p,n,m) = p*(n+m)`. `MIN_EGRESS_STREAM_CEILING = demo_streams(2, 1, 0) = 2` uses the same fn. `all_hear_all_streams` is deleted, which leaves one formula (DRY 3). Its anchor in env-test 01 (`required_edges`) moves to the same P x (N+M) shape and names the constants it mirrors.
- **Capacity change on deploy (operations 4)**: the video cost goes from 2.5 Mbps (configured) to 0.8264 Mbps (derived, prod profile), so the ceiling rises about 3x for the same budget.
  - Base placeholder 10 Mbps: ceiling 4 -> 12, still < 54, so the nudge fires; pinned by `base_placeholder_budget_is_below_recommended_min`.
  - Kind 250 Mbps: 100 -> 302. **That breaks S9 and env-test 01's upper window (max feasible 159 at slot cap 8), so story-2 env-tests would go red at this commit.** I therefore re-size the Kind budget patch in this task to keep the Kind ceiling at 100: 82 640 000 bps / 826 400 = 100 exactly, the same ceiling as today. INF-C re-sizes again for the test profile.
  - The change is recorded in config.env, the Kind patch comment, and (via @team-lead) the commit message.

### 5. Config surface
- config.env, the mh-{0,1} deployment comments and profile.env comments: "current image still reads" becomes "unread since this change; step-2 deletion waits until the rollback floor is at/past it". The TODO "Delete the story-3 retired config keys" entry gains that trigger.
- Keys and `optional: true` refs are kept (operations 1).
- No dt-guard change: env_config rule 1 keys on the `MissingEnvVar` literals and resolves the shared generator already (task 3), and I will verify that `dt-guard env-config` passes.
- Operations 3 (a Layer-3 envelope check on profile.env): declined. `env_config` has no value-validation construct, so adding one is new guard machinery (infrastructure). The boot refusal is loud, and env-test 01 already parses the live profile. Recorded as reasoning.

### 6. Fixtures
- `tests/common/media_frame.rs` stamps the uplink id `0` (the audio send id) instead of 0xFFFF. The rewrite assertion stays non-vacuous because the forward-integration subscriber slots become non-zero where they assert the rewrite (checked per test). The hop sequence is already distinct.
- `mh-test-utils::media_policy` gains `source_stream(sender, stream, mode, disposition)` and `register_request_declared(..)`, so each fixture has one home (DRY 7). `egress()` stays (field 6 DATAGRAM for the legacy path).
- `env-tests/fixtures/media.rs` is not re-edited (task 7 did it).

### 7. Telemetry
- `MediaDropReason::{UnmatchedStream, TransportMismatch}` are added to `ALL` (Ingress direction; `unmatched_stream`, `transport_mismatch`).
- New counter `mh_media_paused_source_frames_total` (no labels), resolved in `MediaMetricHandles` and zero-initialised.
- New outcome `rejected_transport_inconsistent`.
- New config gauge `mh_media_source_streams_per_meeting_limit`; the muted gauge is removed. The cost gauges are unchanged in name and now derived.
- Each gets a catalog row (mh-service.md), with the muted-gauge row replaced and the cost rows' source corrected.
- mh-media.json: basic rate panels for the two drop tokens and the paused counter (explicit `editorMode`, `rate()`); the policy-outcome panel picks up the new token. Descriptions at ~L1478 and ~L1582 are fixed.
- label-taxonomy.md: correct any MH kind-label implication.
- No log, label or span carries a stream id, sender, size or timing.

### 8. Tests (Tier 1a/1b)
- **Unit (routing)**: the outcome table through `attribute`. Rows: label 0 dgram -> Route(audio edges only); label 1 dgram -> TransportMismatch; 7 and 256 -> Unmatched; label 0 with video DROP -> audio edges only; all-DROP sender with label 7 -> ServerMuted (ordering row); PAUSED -> Route{paused}; zero-declared sender -> Unmatched, not ServerMuted; union-muted FORWARD stream -> ServerMuted.
  - Every reject with its token and an `outcome()` mapping; unknown enum ints for both enums; the count bound beats a duplicate.
  - Legacy reading (stream 0 any bound sender, unwatched -> no edges, muted -> shortcut, field-6 rules, echo shape).
  - Echo from the applied snapshot.
  - Two-meeting negative extended to (sender, stream).
- **Unit (forward_one, in `media/forward.rs` tests or `tests/media_forward_integration.rs`)**: the same table on hand-built frames, with routing outcome + MetricAssertion deltas + the identity (`forwarded{ingress}` delta + ingress-drop deltas == 1).
  - A positive control in each negative row.
  - Key-id vs relay-label disagreement: the payload's first 8 bytes carry a key id naming stream 1 while the label is 0; the test asserts the bytes differ from the agreeing frame, and that routing follows the label.
  - Re-push (identical + an unrelated disposition toggle at a new generation) -> hop continues prev+1.
- **gRPC (`mh_service` tests / `policy_apply_integration.rs`)**: each reject -> outcome delta for that token, other outcomes 0, prior policy still live (echo + routes).
  - A mixed-mode registration applies and echoes every stream.
  - The legacy path echoes field 5 as story 2 did.
  - `rejected_transport_inconsistent` is zero-initialised.
- **Config**: exact audio/video costs, with the expected value spelled from literals in the test (`48_000 + 160*8*50`, etc.) alongside the constant-derived one. Kind and prod video. `recommended_min_is_54`. `base_placeholder_budget_is_below_recommended_min`.
  - Boot refusals: below floor, above ceiling, frame-rate mismatch, with the boundaries accepted at floor and ceiling.
  - Source bound at 0/1/4096/4097 and below the egress bound.
- `media_server_mute_integration.rs`: legacy story-2 behaviour (unchanged assertions), plus an explicit-source_streams variant (audio DROP, video FORWARD).

### 9. Env-tests
New `crates/env-tests/tests/37_mh_stream_attribution.rs` (37 is the next free number).
- It is driven like 26's injection tests: real GC/MC joins give the MH connections MC-bound sender ids, then `RegisterMeeting` with explicit `source_streams` is injected on both pods at a generation above MC's.
- 26's `Injection` helper moves into `fixtures/mh_grpc.rs` (one home; 26 and 37 both use it). Datagrams come from `fixtures/media.rs` and T-A's `VideoPublisher` with `VideoTransport::Datagram` + `UplinkLabel`.
- The fns are SERIALISED into one fn per distinct reason token, so no two concurrent fns produce the same token:
  (a) mixed registration applies and echoes every stream;
  (b) audio DROP_AT_INGRESS drops that sender's audio at every receiver while declared, with another sender's audio as the positive-control stop rule;
  (c) the patched client: video DROP; video-labelled datagram -> transport_mismatch; audio-relabelled -> audio slots only, nothing on any video slot id, audio unaffected; stop rule = K control frames;
  (d) unmatched_stream counted;
  (e) PAUSED still forwards + the paused counter.
- The primary proof is per-entity receipt; the pinned-pod counter rise is secondary.
- Header: P=3, N=2, M=1 per fn -> 9 each; at most 3 concurrent -> sum <= 27 (<= 30).
- No wall-clock pass conditions.
- Story-2 specs with explicit `source_streams`:
  - 26's injection tests (server mute, misattribution, edge churn) are parameterised legacy + explicit.
  - 29's probe gets an explicit-`source_streams` variant.
  - 27/28/32 are driven by the real story-2 MC, which cannot send `source_streams` until MC-A. They stay on the legacy path here, and MC-A turns them explicit. **Raised to @team-lead/@test as the one gap against "both through the legacy path and with explicit source_streams".**
- env-test 01:
  - The CONFIG_GAUGES rows for the retired keys are replaced. The cost gauges are compared against costs computed in-test from `media-protocol` constants + the live profile (independent of MH code).
  - The muted gauge is replaced by the source-streams gauge.
  - The Kind window uses the derived costs.
  - RETIRING_KEYS stays (rollback image still requires them), with its comment updated.
  - `egress_admission.rs`'s pinned figures stay valid (ceiling 100).


### Gate-1 amendments (panel feedback folded in)
- **Test.**
  - 37 is ONE fn with sequential phases (a)-(e), and its header line is `//! Edge-sum (concurrent Σ P×(N+M)): 9`.
  - Absence uses a stop rule: K control frames whose send sequence is greater than the last test frame's, seen at EACH subscriber under the claim. A timeout is a failure only (CONTROL-NOT-OBSERVED), never a pass.
  - For unmatched/transport_mismatch: the header states this file is the sole producer of those tokens, and the rise must be >= the publisher's sent count.
  - Added cases:
    - hop continuity across FORWARD->PAUSED->FORWARD on the carrying stream;
    - legacy label 1/7 -> unmatched_stream;
    - legacy field 6 non-DATAGRAM -> rejected_transport_inconsistent;
    - after EndMeeting, attribute -> NoPolicy.
- **Security.**
  - The unit table pins that the transport check runs before per-stream DROP: a video-muted sender's video-labelled datagram is transport_mismatch.
  - Transitional with a label != 0 is Unmatched.
  - Candidate stream width (>255) is rejected_invalid before the undeclared-candidate check.
- **Protocol.**
  - Legacy field-6 rules are stated once, in the field-7 TRANSITIONAL READING block.
  - The video-cost doc notes that the extension region is excluded.
  - The echo test covers a rejected re-push: the echo stays the PRIOR set.
  - The label-256 row uses low byte = a declared FORWARD stream.
- **DRY.**
  - Env-test 01's oracle pins its header sum against MAX_HEADER_BYTES - MAX_EXT_BYTES, with an ANCHOR on both sides.
  - The Kind patch comment states the formula symbolically plus the enforcing test, and no figures (lead ruling).
  - All five demo_streams sites move together, including mh-service.md:414.
  - 26 imports Injection from fixtures/mh_grpc.rs.
- **Observability.**
  - The paused counter and the source-streams gauge carry key_custody=operator.
  - The paused increment is colocated with forwarded{ingress}, so paused ⊆ forwarded{ingress}; it is documented as an overlay.
  - The new tokens go in the saturation-or-input group.
  - no_subscriber is reworded per source stream.
  - The muted gauge is renamed at every reference.
- **Operations.** The Kind patch comment adds that the budget and this image roll back together.
- **Semantic-guard.** The Attribution match is exhaustive with no `_` arm. Every reject goes through `PolicyRejection::outcome()`. The retired-key INFO names the key only.

### Where reviewer input lands
- Security S1-S9: §1 (enum lookup, edges by source), §3 (order), §1 (non-vacuous all_muted), §2 (unknown ints), §2/§4 (bounds), §1 (legacy as a rule), §1 (state inside MeetingRoutes), §3 (hop keyed by egress id), §7.
- Protocol P1-P9: P1/P3/P4 in §1 echo; P2/P6 in §2; P5 in §1; P7 is `attribute`; P8: I edit the three proto comments (fields 5, 6, 7 + the SubscriberSlot superseded list) and protocol reviews; P9: the mh_service declared-vs-applied WARN becomes a per-stream compare of the declared request vs the applied echo (counts only, no ids). On the legacy path it keeps the field-5 compare.
- DRY 1-8: §4, §4 (90 vs 112 raised), §4, §4, §7, §1, §6, §2.
- Operations 1-7: §5, §4, §4, §4 (incl. the Kind re-size), §7 + env-test 01, §2 legacy comment, §4.
- Test 1-11: §8/§9.

### Gate 1 — Plan Confirmations

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

Classification-sanity guard: OK (after Lead corrected internal.proto Owner cell to the single manifest owner `protocol`; security is still in planning + review per GSA rule).
Lead note (27/28/32 with explicit source_streams): MC-A's task text (story manifest task 12) already states its registrations always carry `source_streams` and its gate is Layers 1-7 green, so every story-2 env-test runs on explicit `source_streams` from MC-A on. No story edit needed.

---

## Pre-Work

None

---

## Implementation Summary

### Attribution (R-20) and dispositions (R-16, R-17)
| Item | Before | After |
|------|--------|-------|
| Edge index | `by_sender: SenderId -> edges` | `by_source: (SenderId, StreamNumber) -> edges` inside `MeetingRoutes` |
| Lookup | `for_each_source(meeting, sender)` + `is_server_muted` | `RoutingSnapshot::attribute(meeting, sender, uplink_id, arrival)` -> `NoPolicy / ServerMuted / Unmatched / TransportMismatch / Route{paused, edges}` (only `Route` carries edges; the single lookup task 9 calls); `is_server_muted` = the non-vacuous sender-level shortcut |
| `forward_one` | sender mute -> decode -> fan-out over sender's edges | sender shortcut (pre-decode) -> decode -> attribute (relay id via codec view, `TryFrom<u16>`, never `as u8`) -> exhaustive drop or fan-out over that source stream's edges; paused overlay counted where `forwarded{ingress}` is |
| Declarations | `server_muted: Vec<SenderId>`, `transport_mode` | `Declarations::{Declared, Transitional}` in `routing/sources.rs`; one owner Vec; index + all-muted set derived at install |
| Legacy | n/a | `transitional_story2_reading` ("transitional: deleted by MC-B with the superseded proto fields"): lookup-time rule, label 0 DATAGRAM for any bound sender; muted senders via shortcut; field 6 read only here |

### Apply-time rejects
- New `PolicyRejection` variants for source streams; `outcome()` (exhaustive) maps undeclared candidate / inconsistent candidate transports / transitional non-DATAGRAM egress to the new `rejected_transport_inconsistent`, everything else to `rejected_invalid`. `HeterogeneousTransportModes` removed. Count bounds first (source streams; muted set now bounded by the same key).
- Echo: `applied_source_streams` + legacy field 5 from ONE snapshot load of the APPLIED routes; field 5 = story-2 value on the transitional reading, UNSPECIFIED when declared. The MH-side two-ends WARN compares per-stream sets (counts only).

### Derived costs and config (R-23, R-24)
- `AUDIO_FRAME_OVERHEAD_BYTES` (160, from media-protocol sizes; `NOMINAL_AUDIO_FRAME_BYTES` rebuilt on it), `VIDEO_FRAME_OVERHEAD_BYTES` (110), `derived_stream_cost_bps`. Audio 112 000 bps (lead ruling), video 263 200 (Kind test profile) / 826 400 (prod).
- Required reads `MEDIA_{AUDIO,VIDEO}_{MAX_BITRATE_BPS,FRAME_RATE_HZ}`, `MH_MAX_SOURCE_STREAMS_PER_MEETING` (1..=4096, >= egress bound); boot refusals on the audio envelope / frame rate naming the shared profile; reads of the three retired keys removed (INFO when still set). `MAX_EGRESS_STREAMS_PER_MEETING_CEILING` 8192 -> 4096 (lead ruling). `EGRESS_STREAM_CEILING_RECOMMENDED_MIN = demo_streams(6, 5, 4) = 54`; `all_hear_all_streams` deleted.
- Kind budget 250 Mbps -> 82.64 Mbps so the Kind ceiling stays 100 (lead ruling). Capacity change on deploy (for the commit message): base placeholder ceiling 4 -> 12 (still < 54); Kind unchanged at 100 by the re-size.

### Telemetry
- `MediaDropReason::{UnmatchedStream, TransportMismatch}` (ingress), `mh_media_paused_source_frames_total`, `rejected_transport_inconsistent`, `mh_media_source_streams_per_meeting_limit` (replaces the muted gauge); catalog rows, two basic panels, taxonomy correction.

### Additional Changes
- `internal.proto` comment edits (fields 5, 6, 7 bound, SubscriberSlot superseded list, MutedSource "audio-only" sentence) — comment-only.
- env-tests: new `37_mh_stream_attribution.rs`; `Injection` hoisted to `fixtures/injection.rs`; 26 S4 runs on both readings; 29 adds a declared-reading probe + echo check; 01 compares the cost gauges against an independent oracle from media-protocol + the live profile.
- Fixture: `tests/common/media_frame.rs` stamps uplink id 0 (was 0xFFFF); forward-integration slot moved to 3 so the rewrite stays observable.
- Gate-3 placement notes: the declared-reading server-mute coverage planned for `media_server_mute_integration.rs` lives in `tests/media_attribution_integration.rs` (`an_audio_drop_at_ingress_stream_drops_server_muted`, the relabel row). In env-test 26, S4 and edge churn run on both readings; S5 (two concurrent senders) is driven by MC's real policy and so stays transitional until MC-A (stated in its doc comment).

---

## Files Modified

See `git diff --stat` at review time (38 files; 5 new: `routing/sources.rs`, `routing/tests.rs`, `tests/media_attribution_integration.rs`, `env-tests/src/fixtures/injection.rs`, `env-tests/tests/37_mh_stream_attribution.rs`).

---

## Devloop Verification Steps

Gate 2 = `DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` (headless: all layers run).

| Attempt | L1 | L2 | L3 | L4 | L5 | L6 | L7 | TOTAL |
|---------|----|----|----|----|----|----|----|-------|
| 1 | OK | OK | OK | OK | OK | OK | FAIL (env-test 37 phase (c): transport_mismatch 6/20 per pod; browser suite skipped) | FAIL |
| 2 | OK (6s) | OK (4s) | OK (161s) | OK (151s) | OK (1s) | OK (5s) | OK (937s; env-tests-passed, browser-e2e-passed) | **OK** (1265s) |

Attempt 1 root cause: env-test 37 sent an unpaced 20-frame burst that overflowed MH's 2 KiB per-connection datagram receive buffer (correct MH behaviour; `transport_receive_dropped`=14). The fix paces test sends at 20 ms (env-test only; see Issue 4). L7 attempts used: 2 of 2.

(Semantic-guard relocated to the Gate 2 reviewer panel per ADR-0033 Wave 3 #9. See § Code Review Results → Semantic Guard Reviewer below for its findings.)

---

## Code Review Results

### Security Specialist
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED
**Findings**: {count} found, {count} fixed, {count} deferred

{Key findings and resolutions, or "No findings"}

### Test Specialist
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED
**Findings**: {count} found, {count} fixed, {count} deferred

{Key findings and resolutions, or "No findings"}

### Observability Specialist
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED
**Findings**: {count} found, {count} fixed, {count} deferred

{Key findings and resolutions, or "No findings"}

### Code Quality Reviewer
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED
**Findings**: {count} found, {count} fixed, {count} deferred

{Key findings and resolutions, or "No findings"}

### DRY Reviewer
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED

**True duplication findings** (entered fix-or-defer flow):
{List findings sent to implementer, or "None"}

**Extraction opportunities** (appended to `docs/TODO.md`):
{One bullet per `docs/TODO.md` entry added, citing the section heading the entry was added under, or "None"}

### Operations Reviewer
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED
**Findings**: {count} found, {count} fixed, {count} deferred

{Key findings and resolutions, or "No findings"}

### Semantic Guard Reviewer
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED
**Native verdict**: SAFE / UNSAFE (mapped by Lead per `.claude/agents/semantic-guard.md` §Verdict Mapping)
**Findings**: {count} found, {count} fixed, {count} deferred

{Per-finding `[check-name]: file/path.rs:line - description` block, or "No findings"}

{Note any findings folded with Code Reviewer at Gate 3 §Deduplication, with "(also flagged by code-reviewer)" attribution.}

### Gate 3 — Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 1 | 1 | 0 | apply-time linear scan → one index |
| Test | RESOLVED-FIXED | 4 | 4 | 0 | env-test 37 ordering/evidence, 26 declared variant, multi-meeting test |
| Observability | RESOLVED-FIXED | 3 | 3 | 0 | panel description, taxonomy wording, doc count |
| Code Quality | RESOLVED-FIXED | 8 | 8 | 0 | incl. index lookup, #[expect], latency semantics restored |
| DRY | RESOLVED-FIXED | 3 | 3 | 0 | env-test builders + PROFILE_CONFIGMAP single home |
| Operations | RESOLVED-FIXED | 2 | 2 | 0 | runbook retired-key rows, stale citations |
| Semantic Guard | CLEAR | 0 | 0 | 0 | native SAFE |
| Paired Protocol | RESOLVED-FIXED | 3 | 3 | 0 | doc placement, MC comment key name, wrap |

Ownership lens: internal.proto (GSA) comment edits — protocol + security in planning and review; both confirmed.

---

## Accepted Deferrals

**Each entry here is an issue the devloop chose NOT to fix.** Every bullet is a cost shift: the implementer didn't pay the fix-now cost, so a future reader will pay fix-later cost + tracking overhead. List only what was actually deferred — not "follow-ups" or "future improvements" or "potential extractions." If something was fixed, it doesn't belong here.

**Tech debt entries themselves live in `docs/TODO.md`. This section holds only pointers to those entries.** Do not create a `TODO.md` at the repo root or anywhere else — there is exactly one `docs/TODO.md` for the whole project. Do not inline the debt body here — multi-line entries belong in `docs/TODO.md`, not in this section.

Each pointer is exactly one bullet of the form `- \`docs/TODO.md\` §SECTION-NAME — one-line hook (≤80 chars)`. If you wrote more than one line per entry, you're writing it in the wrong file — move the body to `docs/TODO.md` and leave only the pointer here.

Examples:

```
- `docs/TODO.md` §Observability Debt — orphan recording-site audit follow-up
- `docs/TODO.md` §Cross-Service Duplication (DRY) — extract record_token_refresh_metrics
```

or:

```
- (none surfaced in this devloop)
```

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `{start_commit}`
2. Review all changes: `git diff {start_commit}..HEAD`
3. Soft reset (preserves changes): `git reset --soft {start_commit}`
4. Hard reset (clean revert): `git reset --hard {start_commit}`
5. For schema changes: rollback requires a forward migration — `git reset` alone is insufficient if migrations were applied
6. For infrastructure changes: may require `skaffold delete` or `kubectl delete -f` if manifests were applied
7. **Safe-revert unit** (answer explicitly, even if "the whole commit"): can any part of this diff be reverted or cherry-picked on its own, or does a partial revert reconstruct a state worse than either endpoint (e.g. a security fix split from the change that made it necessary, or a client/server/alert-rule set that must move together)? If partial reverts are unsafe, name the unit and the safe direction. **Answer**: `{the whole commit | <unit + safe direction>}` (This converts silence into a visible unanswered slot; it cannot distinguish a checked answer from a reflexive one.)

---

## Issues Encountered & Resolutions

### Issue 1: Story's "audio ≈ 90 kbps" is inconsistent with its own cost formula
**Problem**: The prompt and story (L676, L1181) expect the derived audio cost to be about 90 kbps. The prompt's own formula at the profile values gives MAX_BITRATE + non-payload bits x frame rate = 48 000 + 160 B x 8 x 50 = **112 000 bps**. The 160 B is the 14-byte v3 prefix, 50-byte wrapped key, 2-byte ext length, 6-byte relay region, 24-byte SFrame overhead and 64-byte signature. The 90 kbps figure is ADR-0036 §4's estimate at the NOMINAL bitrate. It is below even the no-wrapped-key floor (48k + 110 x 8 x 50 = 92k), and the prompt requires the wrapped key on every audio frame.
**Resolution**: Lead ruling: pin the formula value, 112 000 bps. The video figures match the story (0.83 / 0.265 Mbps).

### Issue 2: Source-stream ceiling below the egress ceiling
**Problem**: `MH_MAX_SOURCE_STREAMS_PER_MEETING` is bounded 1..=4096, and the boot rule sources >= egress is kept. The egress ceiling was 8192, so egress values 4097..8192 could never be paired, and the const tripwire fails.
**Resolution**: Lead ruling: lower `MAX_EGRESS_STREAMS_PER_MEETING_CEILING` to 4096. The tripwire is kept as source ceiling >= egress ceiling.

### Issue 3: Derived costs move the Kind ceiling out of S9's window
**Problem**: At the prod profile, the derived video cost (826 400 bps, replacing 2.5 Mbps) raises the Kind ceiling from 100 to 302. That is above S9's feasible maximum (159), so env-tests 01 and 28 would fail at this commit.
**Resolution**: Lead ruling: re-size the Kind budget patch so the ceiling stays at 100. The patch comment cites the formula and the enforcing env-test and does not restate figures.

### Issue 4: Gate 2 attempt 1 — env-test 37 counted 6 of 20 `transport_mismatch` frames per pod
**Problem**: Phase (c)'s video publisher sent its 20 datagrams in an unpaced burst. MH's per-connection datagram receive buffer is deliberately small (`DATAGRAM_RECEIVE_BUFFER_BYTES` = 2 KiB; ADR-0036 §11's bounded drop-oldest ingress) and holds about 6 frames of that size before the ingress loop reads, so quinn evicted the rest. Live evidence: on both pods `transport_mismatch` = 6 and `transport_receive_dropped` = 14 (= 20 − 6), counted at connection teardown. MH behaved correctly; the test's send pattern was the defect.
**Resolution**: Every test frame in 37 is now paced at a real publisher's cadence (`FRAME_INTERVAL` = 20 ms per connection; video via `VideoPublisher::run`, audio via a `tokio::time::interval`). This is the send rate, not a pass condition. The `>= TEST_FRAMES` gates are unchanged. Re-run of 37 alone against the Gate-2 cluster passed: `transport_mismatch` +20 and `unmatched_stream` +20 on each pod, and `transport_receive_dropped` stayed at 14 (zero loss).

## Lessons Learned

1. {Key takeaway 1}
2. {Key takeaway 2}
3. {Key takeaway 3}

{Add more as applicable}

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
