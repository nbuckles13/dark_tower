# Devloop Output: C-A2 — SDK video publish path

**Date**: 2026-10-22
**Task**: Story 3 "see each other" task 13 (C-A2): SDK video publish path — camera + test-pattern capture, VP8, stream-per-group egress with per-target abandonment, video keys, keyframe-request acceptance, keyframe-floor sync guard; deletes C-A1's two transitional shims. Full task text: `~/.cache/devloop/story-runs/story-runner/2026-10-07-see-each-other/task-13.prompt`.
**Specialist**: client
**Mode**: Agent Teams (v2) — full, Gate-1 present
**Branch**: `feature/user-see-each-other`
**Duration**: ~3h45m

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `c0eff5f4831f14c12622e67c5841611d28883134` |
| Branch | `feature/user-see-each-other` |
| Lead Model | `claude-opus-5-5[1m]` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` |
| Implementing Specialist | `client` |
| Tier | `full` |
| Iteration | `1` |
| Security | `security` |
| Test | `test` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Semantic Guard | `semantic-guard` (spawned: diff carries client key material / credential lifetime) |
| Meeting Controller | `meeting-controller` (conditional reviewer: keyframe-floor sync guard, per task text) |

---

## Task Overview

### Objective
See task text (referenced above). Story: `docs/user-stories/2026-10-07-see-each-other.md` (Design §Wire freeze wins on conflict).

### Scope
- **Service(s)**: client (sdk-core, web-app), scripts/guards
- **Schema**: No
- **Cross-cutting**: Yes (guard reads MC constant)

### Debate Decision
NOT NEEDED — design fixed by story + ADR.

---

## Cross-Boundary Classification

Every planned file change is listed (scope-drift guard input). Mine = client. Only Domain-judgment / GSA rows carry an Owner. No GSA path is edited: `crates/mc-service/src/media_signaling/directive.rs` is READ by the floor guard, never edited; `packages/proto-gen/**` is not edited (see Planning §Shim deletion, the verify-codegen presence asserts are MC-B's).

| Path | Classification | Owner | Notes |
|------|----------------|-------|-------|
| `crates/mc-service/src/media_signaling/directive.rs` | Not mine, Minor-judgment | | doc-only DRIFT SIGNAL comment on `MIN_KEYFRAME_INTERVAL_MS_CEILING` (reviewed and approved by meeting-controller) |
| `docs/observability/alerts.md` | Not mine, Minor-judgment | | "Deliberate absences (client video send path)" inventory (observability on panel) |
| `docs/observability/dashboards.md` | Not mine, Minor-judgment | | Group B (panel only) list gains the video send-path series (observability on panel) |
| `docs/observability/label-taxonomy.md` | Not mine, Minor-judgment | | `source` and `outcome` vocabulary rows (observability on panel) |
| `docs/observability/metrics/client.md` | Not mine, Minor-judgment | | catalog rows for the 7 new series, video send-drop-ratio note, stale C-A1 text (observability on panel) |
| `docs/specialist-knowledge/client/INDEX.md` | Mine | |  |
| `infra/grafana/dashboards/client-media.json` | Not mine, Minor-judgment | | 7 basic panels + stale "until C-A2" / MH-pointer panel text (observability on panel) |
| `infra/services/otel-collector/collector.yaml` | Not mine, Minor-judgment | | 7 exported names added to the include list + C-A2 cardinality recompute (existing construct; infrastructure) |
| `packages/sdk-core/src/config/__tests__/clientConfig.test.ts` | Mine | |  |
| `packages/sdk-core/src/config/clientConfig.ts` | Mine | |  |
| `packages/sdk-core/src/globals.d.ts` | Mine | |  |
| `packages/sdk-core/src/index.ts` | Mine | |  |
| `packages/sdk-core/src/media/MediaTransport.ts` | Mine | |  |
| `packages/sdk-core/src/media/__tests__/helpers.ts` | Mine | |  |
| `packages/sdk-core/src/media/__tests__/hotPathLayout.test.ts` | Mine | |  |
| `packages/sdk-core/src/media/__tests__/mhControlChannel.test.ts` | Mine | |  |
| `packages/sdk-core/src/media/lifecycle/AudioPipeline.ts` | Mine | |  |
| `packages/sdk-core/src/media/lifecycle/VideoPublisher.ts` | Mine | |  |
| `packages/sdk-core/src/media/lifecycle/__tests__/VideoPublisher.test.ts` | Mine | |  |
| `packages/sdk-core/src/media/lifecycle/__tests__/audioPipeline.degradation.test.ts` | Mine | |  |
| `packages/sdk-core/src/media/lifecycle/__tests__/audioPipeline.deviceAndCounts.test.ts` | Mine | |  |
| `packages/sdk-core/src/media/lifecycle/__tests__/audioPipeline.intervalSignals.test.ts` | Mine | |  |
| `packages/sdk-core/src/media/lifecycle/__tests__/audioPipeline.multiSender.test.ts` | Mine | |  |
| `packages/sdk-core/src/media/lifecycle/__tests__/audioPipeline.test.ts` | Mine | |  |
| `packages/sdk-core/src/media/lifecycle/__tests__/cameraGate.test.ts` | Mine | |  |
| `packages/sdk-core/src/media/lifecycle/__tests__/keyframePolicy.test.ts` | Mine | |  |
| `packages/sdk-core/src/media/lifecycle/__tests__/transmitKeys.test.ts` | Mine | |  |
| `packages/sdk-core/src/media/lifecycle/cameraGate.ts` | Mine | |  |
| `packages/sdk-core/src/media/lifecycle/faultOnce.ts` | Mine | | new: the one event-once fault dedupe (DRY F1), used by audio + video |
| `packages/sdk-core/src/media/lifecycle/keyframePolicy.ts` | Mine | |  |
| `packages/sdk-core/src/media/lifecycle/transmitKeys.ts` | Mine | |  |
| `packages/sdk-core/src/media/pipeline/__tests__/frameSeal.test.ts` | Mine | |  |
| `packages/sdk-core/src/media/pipeline/__tests__/nonceUniqueness.property.test.ts` | Mine | |  |
| `packages/sdk-core/src/media/pipeline/__tests__/videoEgress.test.ts` | Mine | |  |
| `packages/sdk-core/src/media/pipeline/egress.ts` | Mine | |  |
| `packages/sdk-core/src/media/pipeline/frameSeal.ts` | Mine | |  |
| `packages/sdk-core/src/media/pipeline/videoEgress.ts` | Mine | |  |
| `packages/sdk-core/src/media/setup/__tests__/mediaMetrics.test.ts` | Mine | |  |
| `packages/sdk-core/src/media/setup/__tests__/testPattern.test.ts` | Mine | |  |
| `packages/sdk-core/src/media/setup/__tests__/videoCapture.test.ts` | Mine | |  |
| `packages/sdk-core/src/media/setup/__tests__/vp8.test.ts` | Mine | |  |
| `packages/sdk-core/src/media/setup/capture.ts` | Mine | |  |
| `packages/sdk-core/src/media/setup/captureFailure.ts` | Mine | |  |
| `packages/sdk-core/src/media/setup/mediaMetrics.ts` | Mine | |  |
| `packages/sdk-core/src/media/setup/meetingSenderId.ts` | Mine | |  |
| `packages/sdk-core/src/media/setup/seams.ts` | Mine | |  |
| `packages/sdk-core/src/media/setup/testPattern.ts` | Mine | |  |
| `packages/sdk-core/src/media/setup/testTone.ts` | Mine | |  |
| `packages/sdk-core/src/media/setup/videoCapture.ts` | Mine | |  |
| `packages/sdk-core/src/media/setup/videoCaptureBrowser.ts` | Mine | | new: browser-only camera/pattern platform (coverage-excluded like capture.ts) |
| `packages/sdk-core/src/media/setup/vp8.ts` | Mine | |  |
| `packages/sdk-core/src/media/setup/vp8Encoder.ts` | Mine | | new: browser-only VP8 encoder factory (coverage-excluded like opus.ts) |
| `packages/sdk-core/src/session/MeetingSession.ts` | Mine | |  |
| `packages/sdk-core/src/session/__tests__/mediaSelection.video.test.ts` | Mine | |  |
| `packages/sdk-core/src/session/__tests__/meeting-session.test.ts` | Mine | |  |
| `packages/sdk-core/src/session/__tests__/mhControlSink.test.ts` | Mine | |  |
| `packages/sdk-core/src/session/__tests__/sendDirective.perKind.test.ts` | Mine | |  |
| `packages/sdk-core/src/session/events.ts` | Mine | |  |
| `packages/sdk-core/src/session/mediaSelection.ts` | Mine | |  |
| `packages/sdk-core/src/session/mediaStart.ts` | Mine | | new (lead-accepted split from MeetingSession, forced by the dt-guard 800-line class cap): session media build, one key manager + KEK binding, release on every exit, VP8 join probe |
| `packages/sdk-core/src/session/mediaWiring.ts` | Mine | |  |
| `packages/sdk-core/src/session/slotPlan.ts` | Mine | |  |
| `packages/sdk-core/src/signaling/SignalingClient.ts` | Mine | |  |
| `packages/sdk-core/src/signaling/__tests__/helpers.ts` | Mine | |  |
| `packages/sdk-core/src/signaling/__tests__/legacySourceMutedShim.test.ts` | Mine | | DELETED (shim b tests) |
| `packages/sdk-core/src/signaling/__tests__/maps.test.ts` | Mine | |  |
| `packages/sdk-core/src/signaling/__tests__/mediaSignaling.test.ts` | Mine | |  |
| `packages/sdk-core/src/signaling/__tests__/signaling-client.test.ts` | Mine | |  |
| `packages/sdk-core/src/signaling/__tests__/slotStateMap.test.ts` | Mine | |  |
| `packages/sdk-core/src/signaling/codecMap.ts` | Mine | |  |
| `packages/sdk-core/src/signaling/events.ts` | Mine | |  |
| `packages/sdk-core/src/signaling/legacySourceMutedShim.ts` | Mine | | DELETED (shim b) |
| `packages/sdk-core/src/signaling/slotStateMap.ts` | Mine | |  |
| `packages/sdk-core/tests/bundle-content.test.ts` | Mine | |  |
| `packages/sdk-core/tests/toneMarkers.ts` | Mine | |  |
| `packages/sdk-core/vite.config.ts` | Mine | |  |
| `packages/sdk-core/vitest.config.ts` | Mine | | define + two coverage exclusions with stated reason |
| `packages/sdk-svelte/src/globals.d.ts` | Mine | |  |
| `packages/sdk-svelte/vitest.config.ts` | Mine | |  |
| `packages/web-app/src/__tests__/e2eBus.test.ts` | Mine | |  |
| `packages/web-app/src/__tests__/helpers/MockMeetingSession.ts` | Mine | |  |
| `packages/web-app/src/__tests__/slotState.test.ts` | Mine | |  |
| `packages/web-app/src/__tests__/testLevers.test.ts` | Mine | |  |
| `packages/web-app/src/globals.d.ts` | Mine | |  |
| `packages/web-app/src/lib/e2eBus.ts` | Mine | |  |
| `packages/web-app/src/lib/session.ts` | Mine | |  |
| `packages/web-app/src/lib/testLevers.ts` | Mine | |  |
| `packages/web-app/tests/bundle-content.test.ts` | Mine | |  |
| `packages/web-app/tests/testDefines.test.ts` | Mine | |  |
| `packages/web-app/tests/testPattern.crossCheck.test.ts` | Mine | |  |
| `packages/web-app/vite.config.ts` | Mine | |  |
| `packages/web-app/vite/testDefines.ts` | Mine | |  |
| `packages/web-app/vitest.config.ts` | Mine | |  |
| `pnpm-lock.yaml` | Mine | | lockfile from `pnpm install` after the two overrides (client owns the pnpm workspace; lead ruling) |
| `pnpm-workspace.yaml` | Mine | | security-floor overrides `follow-redirects >=1.16.1 <2` (GHSA-q3jh-wrc6-jgq8, GHSA-8r9p-f939-6h3c) and `vite >=8.2.4 <9` (GHSA-587h-7wmj-h5pm) |
| `scripts/guards/simple/validate-keyframe-floor-sync.sh` | Mine | |  |
| `scripts/guards/validate-keyframe-floor-sync.test.sh` | Mine | |  |
| `scripts/layer3.sh` | Not mine, Minor-judgment | | one `run_and_emit` self-test line, existing construct (infrastructure-owned script) |

Not edited (reviewed in planning): `packages/proto-gen/**` (MC-B's presence asserts).

## Planning

### Mechanism restatement
"Add a video send path" is, in mechanism terms: **one session-scoped key/counter owner feeding two kinds' egress, with every keyframe (whatever its cause) passing ONE per-stream gate (effective interval + single deferral), and every capture device lit only while an explicit predicate holds.** The wider same-owner siblings this surfaces, all folded in: (i) the `TransmitKeyManager` moves from `AudioPipeline`-private to session-owned (a second manager for video would be a nonce repeat); (ii) seal-once/sign-once is extracted from `egress.ts` so video does not grow a second copy of the crypto order; (iii) `dt_client_media_encoder_errors_total` is wired for AUDIO too (partial-invariant: per-kind series must not be "video only"); (iv) the sender-id range check becomes one validator shared by tone and pattern; (v) the pattern-absence proof extends the sdk-core library bundle test as well as the web-app one.

### (1) Shim deletion — ALL THREE C-A1 shims
- **a** — `MeetingSession.startMedia` post-join `ReceiveCapability` send deleted; redeclaration path untouched.
- **b** — `legacySourceMutedShim.ts` + its tests deleted; `SignalingClient` call sites (`participantLeft`, `participantMuteUpdate`, `#emitAssignments`) deleted. `slotStateMap.ts` becomes `Partial<Record<SlotState, SlotStateToken>>` with no `SOURCE_MUTED` key: any unlisted wire value (the legacy 7 included) maps to `unspecified`, never `active` (fail-closed, unchanged security property). A test pins "every SlotState member but one is mapped, and the unmapped one is `unspecified`" without naming it. MC-B (task 15) restores the exhaustive `Record` after deleting the member.
- **c** — `legacyEmptyStreamsWithdrawal` deleted (security criterion "none survives"). MC-A lists every published kind on every directive (`build_send_directive` iterates `publish_set.kinds()`), so an empty `streams` list from MC is now a protocol fault: each published kind counts `send_directive_kind_missing` and fails closed.
- **Grep (verification step):** `git grep -nE 'SOURCE_MUTED|source_muted|max_receive_slots|maxReceiveSlots' -- packages ':!packages/sdk-core/src/proto' ':!packages/proto-gen'` → zero. Exclusions: generated protobuf-es output (gitignored) and `packages/proto-gen/scripts/verify-codegen.sh`, whose lines are PRESENCE asserts that the proto still carries the superseded members "until MC-B deletes it" — flipping them is MC-B's proto deletion, and editing them now would fail codegen verification while the proto still has them. **Raised to @team-lead** as the one judgment call on "zero references in packages/".

### (2) Capture
- `media/setup/videoCapture.ts`: `createCameraCapture({width,height,frameRateHz,deviceId})` — `getUserMedia({video:{width:{ideal},height:{ideal},frameRate:{ideal},deviceId:{exact}?}})` at the DIRECTED values, frames through the shared (now generic, exported) `capture.ts:trackCapture` over `MediaStreamTrackProcessor<VideoFrame>`. Failure vocabulary = `captureFailure.ts` (messages become kind-aware: `CAPTURE_MESSAGE[kind][reason]`), no fork.
- **Test pattern (R-27)**: `media/setup/testPattern.ts` pure generator; header restates `packages/web-app/e2e/README.md` §"Video test pattern (THE spec)" and names it the single home (6×4 grid, ⌊c·W/6⌋/⌊r·H/4⌋ edges, row-major table G0 I15..I12 G1 / I11..I6 / I5..I0 / G2 C3..C0 G3, guards W/B/B/W, white=1, pure 0/255, Gray `n^(n>>1)` of capturedFrameIndex mod 16 MSB-first, advanced per CAPTURED frame). Input = meeting `sender_id` only, through the new shared `meetingSenderId.ts:assertMeetingSenderId` (throws on 0, 65536, -1, NaN, 1.5; `testTone.ts` uses it too). Capture: `createTestPatternCapture` (in `videoCapture.ts`) paints into an `OffscreenCanvas` at the directed W×H/fps and yields `VideoFrame`s through the SAME capture seam, so it starts/stops on exactly the camera's edges; it THROWS if `__DT_TEST_PATTERN__` is false (reached in a non-pattern build). Never imports `e2e/`.
- **Gate**: `__DT_TEST_PATTERN__`, env `DT_TEST_PATTERN=1`, resolved by `vite/testDefines.ts:resolveOptInTestDefine` (the one predicate; new `TEST_PATTERN_ENV`); `false` in sdk-core library build, vitest configs and every production build; production opt-in throws. Bundle FORBIDDEN tokens: `__DT_TEST_PATTERN__`, `test_pattern`, `OffscreenCanvas` (synthesis), each with a positive control in the gated dev build; same tokens added to sdk-core's library bundle test.
- **Selection**: `session/mediaSelection.ts:selectVideoCaptureSource(injected, senderId)` beside `selectCaptureSource` — the only `__DT_TEST_PATTERN__` site; returns `{factory, info:{mode:'camera'|'test_pattern', patternId?}}`. Pattern id published on the e2e bus as `videoCaptureSource {mode, patternId}` where `captureSource` is published today; plus `sendTracks {cameraTrackLive, micTrackOpen}` (README contract), sampled on the existing tick.
- **Camera-light predicate** (VideoPublisher, one function): `directed && targets non-empty && !clientMuted && !captureFailed`. Evaluated on every input edge (directive, mute, failure); capture STOPS (track.stop) whenever it turns false — withdrawal (unwatched or server-mute, both arrive as empty targets) and client video mute; re-acquired when it turns true, reusing the granted `deviceId` (`track.getSettings().deviceId`) so there is no device re-pick. Re-acquire happens only on an EDGE, never in a loop. Mic untouched (audio mute keeps it open, as today).
- **R-19**: a track `ended`, or a failed acquire/re-acquire → `capture_failures_total{media_kind="video", outcome}` (`permission_denied|no_device|device_unavailable|ended|…` from the existing vocabulary), one non-fatal `capture` fault, and the participant is set **video client-muted** through the shared `MuteState` (so `muteChanged` fires and `MuteRequest{video_muted:true, audio_muted:<current>}` goes to MC). A `failureLatched` flag records that the mute is failure-caused. NO timer retry. Retry only on: (a) the next directive restore edge (empty→non-empty) while latched — auto-clears the failure-mute and re-acquires once; (b) a user `setVideoMuted(false)`. A user `setVideoMuted(true)` clears the latch (user intent wins; a later restore does not unmute them). Failure again → same path, bounded by MC's directive edges.

### (3) Codec
- `media/setup/vp8.ts` mirroring `opus.ts`: `probeVp8Support(profile, api = globalThis)` runs `VideoEncoder.isConfigSupported` AND `VideoDecoder.isConfigSupported`; throws `VideoCodecUnsupportedError{stage:'encoder'|'decoder'|'platform'}` (bounded messages). `buildVp8EncoderConfig(encoding)` (pure: `codec:'vp8'`, W, H, framerate, bitrate, `latencyMode:'realtime'`, no keyframe interval member — WebCodecs has none). `createVideoEncoder` factory: thin; re-probes the DIRECTED config before `configure`; seam `encode(frame, keyFrame: boolean)`, `reconfigure(encoding)`, `close()`; output `{data, timestampUs, key}` from `chunk.type`. Nothing shared with `e2e/vp8Probe.ts` (independent by design).
- **When**: at `join()`, if `publish` includes video OR M>0, the probe runs before the signalling join at a compiled CAPABILITY-PROBE profile (`clientConfig` `video.capabilityProbe`, the production `media-codec-profile` values, used ONLY to decide the declaration — never an encoding default). Pass ⇒ `supported_codecs=[CODEC_VP8]` (codecMap gains public `vp8`). Fail ⇒ VP8 not declared, the result is held, and `startMedia()` THROWS `VideoCodecUnsupportedError` and emits a fatal `mediaFault{stage:'encoder'|'decoder'}` — never audio-only. Each directed encoding is re-probed by the encoder factory (Kind's 320×180 profile differs from production); failure there = fatal video fault + `encoder_errors_total{video}`.
- **Keying**: `encode(frame,{keyFrame:true})` only when the keyframe gate (6) fires (directive start/resume, target added, publisher-decided abandonment, accepted request). Group boundaries in egress follow `chunk.type==='key'`.

### (4) Video egress — `media/pipeline/videoEgress.ts` (hot path: no logging, pre-bound `MediaKindMetrics` handles; covered by the directory-scoped `media/__tests__/hotPathLayout.test.ts`, which already scans every `.ts` under `pipeline/`)
- Submits are SERIALISED on one promise chain (stream order matters; audio datagrams tolerate reorder, a group stream does not). Per frame: on a key chunk → `transmitKeys.rotate()` (group start rotates), `nextGopCounter` (null ⇒ stream stops forever, counted `gop_counter_exhausted` per lane); delta chunks carry the current group's counter. ONE `materialFor`, ONE `nextStreamSequence`, ONE seal, ONE sign via the shared `frameSeal.ts`, identical bytes to every target (lane copies only for the in-place hop write, as audio).
- Flags: `independentlyDecodable = key`; `keyBearing = framesSinceKeyChange < N` where key change is detected by material identity (`material !== last`), so ANY trigger (T, group start, KEK) re-arms the wrap; `discardable = false` for every video frame (key-bearing never discardable; VP8 deltas are references).
- Relay `stream_id` = `pipeline/sendId.ts:uplinkRelayStreamId(videoDirective)` — the one stamper.
- Per target (lane, keyed by URL, RETAINED across directives so the hop counter lives outside the QUIC stream object): `hopSequence`, `group: {writer, queue, frames, bytes} | undefined`, `state: 'awaiting_group' | 'in_group' | 'mh_stopped'`. Key chunk → for every active lane: FINISH the previous group stream (`close()`), open a new uni stream via `MediaTransport.openUniStream(url)` (selector over connected transports), first frame = keyframe, back to back. Delta → only `in_group` lanes.
- Writes: per-lane single-flight drain; `writeHopSequence` stamped when the frame is WRITTEN (dequeue), so backlog drops consume no number and hop is gapless across groups.
- Backlog cap (frames AND bytes, `clientConfig`): an enqueue that would exceed it → RESET (`abort()`) that lane's group stream, drop its queue, `awaiting_group` (publisher-decided), count `dt_client_media_send_groups_abandoned_total{video}`, and notify the lifecycle `onSelfAbandoned()` → self-key cause `abandonment`. Encoder, other lanes and audio never wait (enqueue is synchronous; nothing awaits a slow lane).
- MH STOP_SENDING / write rejection with the platform stream-error shape → lane `mh_stopped`: no re-key, not counted as "no group in progress"; it rejoins at the next key chunk started for any other reason. Not counted client-side (MH counts its own refusals) — @observability to confirm.
- `targetsAwaitingGroup()` = active lanes in `awaiting_group` (never `mh_stopped`): what the gate re-checks at fire time. Withdrawal/mute: `endGroups()` FINs current streams (all written frames are whole), discards unsent backlog, lanes → `awaiting_group`.

### (5) GoP and keys
- P2's `TransmitKeyManager.nextGopCounter` is the only counter; video calls it on every key chunk (incl. resume). Never reset; u32::MAX stop. Internal `currentGopCounter(stream)` added for request comparison (not exported from the package; never logged/labelled).
- `TransmitKeyManager` becomes SESSION-OWNED: `MeetingSession.startMedia` constructs exactly one and injects it into `AudioPipeline` and `VideoPublisher` (one per sender — the nonce boundary).
- Rotation triggers (session-wide generation): audio T (existing), **video T** (`videoRotationPeriodMs` timer in VideoPublisher, own floor), every group start (egress, on key chunk), every KEK receipt (existing single `onCurrentGenerationChanged` subscription in AudioPipeline rotates the shared manager — one subscription, so no double rotate), audio unmute / audio resume-from-empty (existing). Key-bearing N (default 3) per stream from material change; audio stays key-bearing every frame. Neither T nor KEK forces a keyframe. Stream sequence continues across withdraw/resume. `transmitKeys.ts` trigger-table comment rewritten.

### (6) Keyframe gate — `media/lifecycle/keyframePolicy.ts` (pure, injected clock + timer)
- Inputs: directive `(targets, directedMinIntervalMs)`, mute (client video mute; server mute = empty targets), `request(url, gopCounter)`, `selfKey(cause)`, and providers `currentGop()`, `targetsAwaitingGroup()`.
- `request`: `not_sending` unless directed with non-empty targets, `url ∈ currentTargets`, not muted (a request never starts capture); `stale_counter` if `gop < current`; inside effective interval → `min_interval` (deferred); else keyframe now, outcome `honoured` or `honoured_counter_ahead` (gop > current). Outcome counted ONCE at receipt on `keyframe_requests_received_total{video,outcome}`.
- Effective interval = `max(directed, KEYFRAME_MIN_INTERVAL_FLOOR_MS=100)`. Directed below floor → clamped, `dt_client_media_keyframe_min_interval_clamped_total{video}` per directive RECEIPT that clamps, WARN once per session from lifecycle (`directed_ms`, `floor_ms` only).
- Self-key: directive start/resume (empty→non-empty), target added → `directive_start`; publisher-decided abandonment → `abandonment`.
- ONE deferral per stream: `{creator, merged causes, timer}`; any later cause MERGES (never a second timer). Discarded on withdrawal or mute; a target-set change re-checks it (held request whose handler left is dropped from the set). At fire EVERY condition re-checked: a request needs its handler still targeted and counter ≥ current; a self-key needs `targetsAwaitingGroup() > 0`; fires if any member survives, counted ONCE on `dt_client_media_keyframes_produced_total{video,source}` under the creator's cause (or, if the creator lapsed, the earliest surviving merged cause). Only the named stream keys (stream_number ≠ directed video stream ⇒ `not_sending`/unattributed as today).
- Missing encoding field on a video stream (codec≠VP8, width/height/frame_rate/max_bitrate absent or 0, `min_keyframe_interval_ms` absent) → typed non-fatal `config` fault, `dt_client_media_send_directive_encoding_refused_total{video}` (no field label; the WARN names the field), video fails closed (empty targets). No client default for any video encoding value.

### (7) Floor guard
`scripts/guards/simple/validate-keyframe-floor-sync.sh` (shape of `validate-slug-class-sync.sh`): env-overridable `KEYFRAME_FLOOR_SYNC_MC_SRC` / `KEYFRAME_FLOOR_SYNC_SDK_SRC`; extracts exactly one `^pub const MIN_KEYFRAME_INTERVAL_MS_FLOOR: u32 = ([0-9_]+);$` and exactly one `^export const KEYFRAME_MIN_INTERVAL_FLOOR_MS = ([0-9_]+);$`; fails on not-found / >1 match / inequality, naming both files, "fix the extraction rather than deleting the guard". Auto-discovered by `run-guards.sh`; self-test `scripts/guards/validate-keyframe-floor-sync.test.sh` (in-sync pass, MC drift, SDK drift, each literal missing, duplicate, positive control on the REAL tree) wired via one `run_and_emit` in `scripts/layer3.sh`. The SDK constant's comment carries `ANCHOR (DRY):` naming `MIN_KEYFRAME_INTERVAL_MS_FLOOR`, `MC_VIDEO_MIN_KEYFRAME_INTERVAL_MS`, the guard and the clamp counter.

### (8) Client knobs (code-config, `clientConfig.ts`, not ConfigMap/env-config)
`keys.videoRotationPeriodMs` (default 60 000, floor `MIN_VIDEO_ROTATION_PERIOD_MS`), `keys.videoKeyBearingFrames` (N, default 3, bounds 1..30), `egress.videoMaxBacklogFrames` / `egress.videoMaxBacklogBytes` (bounded, positive), compiled `KEYFRAME_MIN_INTERVAL_FLOOR_MS = 100`. Out-of-range ⇒ typed `ClientConfigError` (never silently clamped; the keyframe interval floor is the only floor-applied value, by spec). Logged once at video start (`[dt-media-video-config]`): each knob with `configured|default`, the floor as `compiled`. Comment states the tuning/rollback story: these are bundle constants, so a change is a web-app rebuild + redeploy.

### Telemetry (all per-kind on `MediaKindMetrics`; no new label KEY — `source`, `outcome` already allow-listed)
| Series | Labels | Zero-init |
|---|---|---|
| `dt_client_media_keyframes_produced_total` | `media_kind`, `source`=`directive_start\|request\|abandonment` (const vocab) | yes, video, when video is first directed |
| `dt_client_media_send_groups_abandoned_total` | `media_kind` | yes, same time |
| `dt_client_media_keyframe_min_interval_clamped_total` | `media_kind` | lazy (panel `or vector(0)`, no alert) |
| `dt_client_media_encoder_errors_total` | `media_kind` (audio wired too) | lazy |
| `dt_client_media_send_directive_encoding_refused_total` | `media_kind` | lazy |
| existing `keyframe_requests_received_total{video,outcome}` honour paths, `capture_failures_total{video,outcome}` | | as today (lazy) |
Catalog rows, label-taxonomy `source` row, alerts.md "none (story 3 OBS-B)", basic dashboard panels, collector include list + C-A2 cardinality paragraph; stale "until C-A2" panel/catalog text updated. No GoP, size, timing, sender/stream id anywhere (pattern id only on the test bus, README carve-out).

### Web app
`publish` test lever (`audio|video|both|none`, README cohort contract) routed to `MeetingSession.publish`; the app's DEFAULT publish set stays audio (C-C1 owns the grid/mute UX; flagged to @team-lead). Bus events as in (2).

### Tests and gates mapping (node tier, `MockWebTransport`, injected clock, no sleeps)
- `lifecycle/__tests__/keyframePolicy.test.ts`: stale/duplicate burst → one keyframe; counter-ahead honoured+counted; wrong handler / undirected / muted → `not_sending`; deferral one-pending, discarded on withdrawal / mute / target change, re-checked at fire; request + too-soon self-key share one deferral → one keyframe counted once; each `source`; abandonment never `directive_start`; effective interval = max(directed, floor) + clamp counted, WARN once; only named stream keys; missing encoding field refused.
- `pipeline/__tests__/videoEgress.test.ts` (mock uni streams): seal once, identical bytes per target; first frame of every group stream is the key, back to back, FIN on next group; backlog cap → RESET one target only, others + encoder unstalled, `groups_abandoned` counted, then a keyed group on every target (held as the single deferral inside the interval, never dropped); target added → new group on all targets bounded by interval; STOP_SENDING → no keyframe at once or at a later fire, target gets the next MH-requested group; directed start keys with no request; all-targets-in-group never self-keys; hop sequence gapless across abandonment + groups; stamp from directive.
- `lifecycle/__tests__/transmitKeys.test.ts` + `pipeline/__tests__/videoKeys.test.ts`: key-bearing on first N frames after T / group start / KEK, never discardable; nonce-uniqueness property test across audio+video (key id, sequence) over randomized interleavings of rotations; GoP never reset, increments on resume.
- `lifecycle/__tests__/VideoPublisher.test.ts`: camera stops on unwatched / server-mute withdrawal / client mute and re-acquires on restore with the same deviceId; mic stays open; R-19 counted + video-muted report + no retry loop (failure → no further acquire until restore edge / user unmute).
- `setup/__tests__/vp8.test.ts`: `probeVp8Support` throws typed on encoder-no / decoder-no / missing API; `session/__tests__/meeting-session*.test.ts`: VP8 declared only when both pass; `startMedia` fails loudly + fatal fault on probe failure; shim a gone (no post-join ReceiveCapability).
- `setup/__tests__/testPattern.test.ts`: ids 1, 0x8001, 0xA5A5, 65535 painted, each block against the test's OWN transcription of the README table; Gray counter across 17+ captured frames (wraps, adjacent differ by one bit); throws on 0, 65536, -1, NaN, 1.5. `web-app/tests/testPattern.crossCheck.test.ts`: generator → README luma projection → `e2e/patternDetector.ts:decodePattern`.
- `session/__tests__/sendDirective.perKind.test.ts`: empty streams now a counted fault (shim c gone); video encoding fields required.
- Bundle tests (web-app + sdk-core); floor guard self-test; grep in verification steps.
- Gates: sdk-core, sdk-svelte, web-app, test-utils lint/typecheck/unit green; story-2 browser specs green. No env-tests (C-C1).

### Reuse checklist (DRY reviewer's list)
1 `sendId.ts` only. 2 `nextGopCounter` only. 3 `trackCapture` generic + `captureFailure.ts` vocabulary. 4 one sender-id validator; one predicate in `testDefines.ts`; selection beside `selectCaptureSource`; `testLevers.ts` gains `publish`. 5 `vp8.ts` mirrors `opus.ts`; `isConfigSupported` plumbing is two lines per kind with different config types, not worth a shared abstraction; `e2e/vp8Probe.ts` untouched. 6 `egressQueue.ts` NOT reused: its policy is drop-oldest, and dropping a frame mid-group silently corrupts decode — the video policy is reset-the-group, with a byte cap the queue does not have; `writeHopSequence` (codec) reused. 7 all knobs via `clientConfig.ts`. 8 guard per item (7). 9 handles in `mediaMetrics.ts`. 10 grep in (1).

### Rollback / deploy note
This client requires MC-A (task 12) server-side: with shims a/c gone, a pre-MC-A controller (which only directs after a post-join `ReceiveCapability` and spells withdrawal as empty streams) would leave this client undirected / faulting. MC-A is already merged on this branch. Reverting THIS commit alone is safe (MC-A still accepts the C-A1 redeclaration as `accepted_unchanged`). Reverting MC-A without this commit is NOT safe.


### Gate-1 amendments (from reviewer input, all accepted)
- **semantic-guard**: key-change detection compares the packed KEY ID (non-secret), never a material reference; no lane/deferral/egress field holds material. The session-owned `TransmitKeyManager.clear()` is registered for teardown at construction and runs on every exit, including a `startMedia` failure (tested).
- **code-reviewer**: the KEK `onCurrentGenerationChanged` subscription moves from `AudioPipeline` to the session, beside the manager it rotates (one subscription; video-only KEK-rotation test). The camera predicate and R-19 latch become a pure `lifecycle/cameraGate.ts`. The video submit chain catches every link loudly, awaits only seal/sign (never a lane write; tested with a stalled lane), and checks a closed-epoch after each await so nothing is sealed or written after `endGroups()`/withdraw/stop. There is one pure `validateVideoEncoding` and distinct typed errors (`VideoCodecUnsupportedError`, `VideoEncodingRefusedError`). The `slotStateMap` type comment names MC-B restoring the exhaustive Record.
- **test / code-reviewer**: bundle FORBIDDEN tokens are `__DT_TEST_PATTERN__`, `test_pattern`, a generator-unique `dt-test-pattern` tag and the generator refusal string. `OffscreenCanvas` is dropped as fragile. Test additions 1-10 plus the extras are accepted (see test's list).
- **observability**: MH STOP_SENDING is not counted client-side. The `send_groups_abandoned_total` catalog row says it counts publisher-decided abandonments only and points to MH's stream-admission series. The STOP_SENDING test asserts that neither `groups_abandoned` nor `keyframes_produced{abandonment}` moves. `hotPathLayout.test.ts` names `videoEgress.ts` explicitly.
- **dry-reviewer**: the guard self-test uses `scripts/lang/_test_helpers.sh` if the sibling self-tests do.
- **security S1-S7**:
  - S1: a directive-restore retry only for `ended|device_unavailable|no_device|unsupported|unknown`; after `permission_denied`/`platform_unsupported` only a user unmute retries. Acquire first, then lift the failure-mute.
  - S2: every emitted key chunk, solicited or not, stamps the gate's last-keyframe clock. A reconfigure is a gated keyframe decision.
  - S3: only a backlog RESET goes to `awaiting_group`; every other lane failure, including a hung or rejected open, goes to `mh_stopped`. Opens are never awaited on the submit chain.
  - S4: key-id bytes, per stream.
  - S5: an encode-time mute/directive epoch is checked at egress, and stale output is dropped before sealing.
  - S6: H is the read-loop connection URL, byte-exact against CURRENT targets. Malformed and unknown-stream requests are refused before any gate state is touched.
  - S7: `unspecified` is never live.
  - Pattern id on the bus only under `__DT_TEST_PATTERN__`, no counter, with an R-27 carve-out comment.
- **meeting-controller**:
  - a receive-only empty publish set plus empty streams gives no fault;
  - self-keying is driven by the video target-URL diff only (audio-only changes and target removals never key);
  - field-required checks are video-only;
  - the clamp is tested with a synthetic 50 ms value;
  - the guard drift fixtures are seded copies of the real files;
  - the clamp reads the anchored constant.
- **security S3 ruling**: when a lane's open is still pending and its backlog cap trips, the lane aborts or resets whatever exists, drops its queue and goes to `mh_stopped`, never `awaiting_group`. It sends no `onSelfAbandoned` and counts no `keyframes_produced{abandonment}`. A MAX_STREAMS stall is MH's decision. Observability decides whether `groups_abandoned` ticks; my default is NO, so that series stays publisher-decided only. Test: hung open, then backlog overflow, then the interval elapses. Expect 0 keyframes and the lane in `mh_stopped`. A later accepted request from that handler then starts a group that reaches it.

New files added to the plan: `packages/sdk-core/src/media/lifecycle/cameraGate.ts` (Mine).

### Lead rulings (Gate 1)
1. **Grep carve-out ACCEPTED.** The criterion is "no SDK or web-app code references SOURCE_MUTED / max_receive_slots". The exact grep, which must return zero lines:
   `git grep -nE 'SOURCE_MUTED|source_muted|max_receive_slots|maxReceiveSlots' -- packages ':!packages/sdk-core/src/proto' ':!packages/proto-gen'`
   It excludes two things: generated protobuf-es output (`packages/sdk-core/src/proto`, gitignored) and `packages/proto-gen/scripts/verify-codegen.sh`. The verify-codegen lines are proto-PRESENCE asserts ("superseded-but-present until MC-B deletes it"). They are proto-presence tooling owned by MC-B (task 15), which deletes the members and flips those asserts together. Editing them here would fail codegen verification while the proto still carries the members.
2. **Web-app default publish set stays AUDIO.** "Video publishing is enabled by C-A2" means the SDK accepts `video` in the publish set: the typed "not yet supported" refusal is removed. The web-app publish-set chooser belongs to C-C1. This task adds the `publish` test lever per the e2e cohort contract. Story-2 browser specs stay green.

### Gate 1

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Semantic Guard | confirmed |
| Meeting Controller | confirmed |

Classification-sanity guard: `STATUS=OK`. Lead rulings: (1) `packages/proto-gen/scripts/verify-codegen.sh` presence asserts + generated protobuf-es excluded from the zero-ref grep (proto-presence tooling, MC-B task 15); (2) web-app default publish set stays audio — SDK accepts VIDEO (story l.1577), chooser is C-C1 (l.2521); `publish` test lever added.

---

## Pre-Work

None

---

## Implementation Summary

- **Shims deleted (a, b, c).**
  - a: `MeetingSession.startMedia` no longer sends a post-join `ReceiveCapability`.
  - b: `legacySourceMutedShim.ts` and its call sites are gone. `slotStateMap.ts` is now a `Partial` table: the superseded member maps to `unspecified`, never `active`. MC-B restores the exhaustive `Record`.
  - c: an empty `streams` list is a counted `kind_missing` fault for each published kind; a receive-only participant is never counted.
  - Grep (lead ruling 1) returns zero lines.
- **Video publish path.**
  - `media/lifecycle/VideoPublisher.ts` wires three pure modules: `cameraGate.ts` (when the camera is lit; R-19 latch: restore-edge retry except after `permission_denied`/`platform_unsupported`, acquire before unmute, a user mute clears the latch), `keyframePolicy.ts` (request acceptance, effective interval max(directed, 100), one merged deferral re-checked at fire, clamp counted + WARN once, unsolicited key chunks restart the interval), and `pipeline/videoEgress.ts`.
  - `pipeline/videoEgress.ts`:
    - serialised seal/sign through the shared `frameSeal.ts`; key-bearing detected by key-id generation;
    - one uni stream per group per target, FIN at the next group;
    - backlog reset is publisher-decided (`awaiting_group`, counted, self-key);
    - STOP_SENDING, any write or open failure, and a hung open are `mh_stopped` (no re-key, not counted);
    - hop sequence is assigned at write and persists per lane;
    - an encode-time epoch drops output after mute or withdrawal.
- **Keys.**
  - The session owns ONE `TransmitKeyManager` with the ONE KEK subscription beside it (`session/mediaStart.ts`). Both are cleared on every exit, start failure included.
  - Video rotates on `videoRotationPeriodMs`, at every group start and on every KEK receipt.
  - `currentGopCounter` is a read-only internal getter.
- **Codec.**
  - `media/setup/vp8.ts:probeVp8Support` checks encoder AND decoder.
  - At join, when video is published or received, the probe runs at a compiled capability-probe profile. `CODEC_VP8` is declared only on a pass; a failure makes `startMedia` throw `VideoCodecUnsupportedError` and emit a fatal fault.
  - The encoder re-probes the DIRECTED encoding.
  - `validateVideoEncoding` refuses any missing field; the refusal is counted, raises a typed fault and fails closed.
- **Capture.**
  - `videoCapture.ts` holds the camera (`getUserMedia({video})` at the directed geometry, granted `deviceId` reused) and the gated test pattern (`testPattern.ts` restates the e2e README spec; `meetingSenderId.ts` is shared with the tone).
  - `selectVideoCaptureSource` is the only `__DT_TEST_PATTERN__` site.
  - The define is wired through `vite/testDefines.ts` under the one predicate; it is false in the library build and in every vitest config.
- **Floor guard.** `scripts/guards/simple/validate-keyframe-floor-sync.sh`, with a self-test against edited copies of the real files, wired in `layer3.sh`.
- **Telemetry.**
  - Five per-kind series: `keyframes_produced{source}`, `send_groups_abandoned`, `keyframe_min_interval_clamped`, `encoder_errors` (audio wired too), `send_directive_encoding_refused`.
  - Catalog rows, a taxonomy row, collector allowlist entries plus the cardinality recompute (145 honest, about 3.4 runs, no cap change), and five panels.
- **Web app.** `publish` test lever; bus events `videoCaptureSource` (patternId only in a pattern build) and `sendTracks`; bundle FORBIDDEN tokens (`__DT_TEST_PATTERN__`, `test_pattern`, `dt-test-pattern`, `sendTracks`) with positive controls; a production pattern opt-in throws.
- **Microphone only when audio is published** (lead ruling 2): `startMedia` passes `publishesAudio`. A video-only or receive-only participant never acquires the microphone.
- **Audit floors** (lead ruling 1): `pnpm-workspace.yaml` overrides for `follow-redirects` and `vite`; lockfile regenerated.
- **Refactor forced by a guard.** `dt-guard ts_retained_credentials` caps class size at 800 lines, and `MeetingSession` was about 797. The media construction moved to `session/mediaStart.ts`; the class is now under 800.

## Files Modified

See the Cross-Boundary Classification table (every changed file is listed there).

## Devloop Verification Steps

- `git grep -nE 'SOURCE_MUTED|source_muted|max_receive_slots|maxReceiveSlots' -- packages ':!packages/sdk-core/src/proto' ':!packages/proto-gen'` → no lines. A plain `grep -rn` over the working tree, which includes untracked files, gives the same result.
- sdk-core: `tsc --noEmit`, `eslint src/` clean; `vitest run --coverage` 1488 passed, branches 90.77% (gate 90).
- web-app: eslint `src/ e2e/` clean, svelte-check 0 errors; component tier 127 passed; node tier (`testDefines`, `testPattern.crossCheck`) passed. sdk-svelte: svelte-check clean, 27 passed.
- `scripts/guards/validate-keyframe-floor-sync.test.sh`: 10/10 ok.
- `pnpm audit`: three newly published highs fixed per the lead's ruling (a) with `pnpm-workspace.yaml` overrides `follow-redirects >=1.16.1 <2` and `vite >=8.2.4 <9`, then `pnpm install`. `DEVLOOP_AUDIT_FORCE_RUN=1 scripts/lang/ts/audit.sh` → `STATUS=OK`.
- Mic gap (lead ruling 2): `AudioPipeline` takes `publishesAudio`. When audio isn't published there is no capture, no encoder and no audio rotation timer. Tests: video-only and receive-only sessions make 0 microphone capture calls, with a positive control.
- **Lead fast check** (`DEVLOOP_FMT_APPLY=1 ./scripts/layer-fast.sh`, after the first "Ready for review"): L1-L6 OK.
- **Gate 2** (`DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` on the final post-review tree): L1 OK (8s), L2 OK (6s), L3 OK (162s), L4 OK (157s), L5 OK (8s), L6 OK (5s), L7 OK (1107s; Rust env-tests + browser E2E incl. the story-2 specs). `TOTAL_RESULT=OK`, attempt 1 of 3 (L1-6) and 1 of 2 (L7).

---

## Code Review Results

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 6 | 6 | 0 | F1: a frame could be sealed under an all-zero key (two rotations inside the seal's HKDF await); fixed by a synchronous key copy plus an all-zero refusal. F2: deferral dedupe. F3: compiled interval ceiling, guard-synced to MC. F4: hop number claimed synchronously. F5: `publishesAudio` required. F6: doc pointer. |
| Test | RESOLVED-FIXED | 9 | 9 | 0 | T1-T9: selection-gate test, pre-key burst race, withdraw/resume in the nonce property test, a deferral that is really held in the STOP_SENDING case, self-test values derived from the real files, stream-3 stamp, marker comment, duplicate row, re-arm-then-fire. |
| Observability | RESOLVED-FIXED | 9 | 9 | 0 | MH pointer, WARN once per episode, alert inventory and dashboards, provenance log line, video drop-ratio contract, audio encoder build failure counted, stale comment removed. Ruled two new series with operations. |
| Code Quality | RESOLVED-FIXED | 8 | 8 | 0 | CQ1: an orphaned live camera on a stale acquire. CQ2: RESET (not FIN) for a truncated group. CQ3: release awaits the video drain. CQ4: the test-only KEK subscription removed. CQ5-CQ8. |
| DRY | RESOLVED-FIXED | 2 | 2 | 0 | `lifecycle/faultOnce.ts` extracted; one `ConfigValueSource` type. |
| Operations | RESOLVED-FIXED | 3 | 3 | 0 | An oversized keyframe always admitted into an empty backlog; `send_groups_stopped_total`; encoder backpressure (`videoMaxEncodeQueue`, `encode_queue_dropped_total`). |
| Semantic Guard | CLEAR | 0 | 0 | 0 | Native SAFE (Credential Leak client items 5-10 and Client Credential Lifetime applied). Re-checked after F1 and the KEK move. |
| Meeting Controller | RESOLVED-FIXED | 4 | 4 | 0 | F1: a directive that arrives before `start()` was never keyed (normal join path). F2/F3: self-test hardening. F4: a doc-only DRIFT SIGNAL comment on MC's ceiling (Minor-judgment, owner-approved). |

Note for C-C1 (task 17), from security, not a finding: after an R-19 failure-mute, a restore edge can relight the camera without user action, so the UI must present a failure-mute distinctly from a user mute.

---

## Accepted Deferrals

- (none surfaced in this devloop)

---

## Rollback Procedure

1. Start commit: `c0eff5f4831f14c12622e67c5841611d28883134`
2. `git diff c0eff5f4..HEAD`
3. Safe-revert unit: this devloop's commit alone (MC-A still accepts the C-A1 redeclaration). Do NOT revert MC-A (task 12) without also reverting this: the shims this deletes were what let the client run against a pre-MC-A controller. After MC-B (task 15) deletes `SOURCE_MUTED` / `max_receive_slots` from the proto, reverting this commit alone no longer builds (the restored shims reference deleted members): revert MC-B first, then this.

---

## Issues Encountered & Resolutions

### Issue 1: pnpm audit red on newly published advisories
**Problem**: L6 failed on GHSA-q3jh-wrc6-jgq8 and GHSA-8r9p-f939-6h3c (follow-redirects, via nx>axios) and GHSA-587h-7wmj-h5pm (vite 8.2.0-8.2.3), none caused by this diff.
**Resolution**: Lead ruled fix-in-loop: `pnpm-workspace.yaml` overrides plus a lockfile update. No suppression.

### Issue 2: pre-existing mic acquisition when audio is not published
**Problem**: C-A1 left `startMedia` opening the microphone for video-only and receive-only publish sets.
**Resolution**: Lead ruled it in (privacy surface, small). `publishesAudio` is required; tests show zero capture calls.

---

## Lessons Learned

1. Sharing one TransmitKeyManager across kinds multiplies rotation triggers, which widens every pre-existing await window over key material. Re-audit seal-time key reads whenever triggers are added.
2. "Directive before start" is the normal join path (E3 replay). Lifecycle classes must treat a pending directive as a transition from nothing.
