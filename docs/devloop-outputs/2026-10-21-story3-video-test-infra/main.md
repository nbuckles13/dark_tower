# Devloop Output: Story 3 T-A — video test infrastructure

**Date**: 2026-10-21
**Task**: T-A: video test infrastructure (Rust video fixtures with group-start source, pattern detector, slot oracle, cohort/evidence extensions, VP8 substrate probe) — story `docs/user-stories/2026-10-07-see-each-other.md` task 7
**Specialist**: test
**Mode**: Agent Teams (v2) — full, Gate-1 present
**Branch**: `feature/user-see-each-other`
**Duration**: ~3h (1 review→impl iteration; Gate 2 first-pass green: L1-7 OK, VP8 probe OK on devloop Chromium)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `814f0c30465c2e7b6a3322e5da8ad4bcf7134c1b` |
| Branch | `feature/user-see-each-other` |
| Lead Model | `claude-opus-5-5[1m]` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` |
| Implementing Specialist | `test` |
| Tier | `full` |
| Iteration | `1` |
| Security | `security` (paired-security: active collaborator on gating / no-identity rules) |
| Test | `test` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Semantic Guard | not spawned (test-infrastructure diff; no check surface in `scripts/guards/semantic/checks.md` — fixtures are keyless) |
| Paired | `paired-client`, `paired-media-handler`, `paired-infrastructure` |

---

## Task Overview

### Objective
Full task text: the run-story prompt `task-7.prompt` (story file task 7, "T-A"). Deliverables: (1) `crates/env-tests/src/fixtures/video.rs` + `media.rs` real send-id/gop stamping; (2) `packages/web-app/e2e/patternDetector.ts`; (3) `slotOracle.ts`; (4) cohort/evidence extensions; (5) `vp8Probe.ts` with Layer-7 (global-setup + layer7.sh `REASON=browser-vp8-unsupported`) and CI component-tier callers.

### Scope
- **Service(s)**: env-tests crate, web-app e2e harness, scripts/layer7.sh
- **Schema**: No
- **Cross-cutting**: Yes (test infra consumed by MH/MC/client tasks)

### Debate Decision
NOT NEEDED — design fixed by ADR-0036 and the story file's Design §Wire freeze.

---

## Cross-Boundary Classification

Every planned file. Only Domain-judgment rows carry an owner; Mechanical/Minor rows are review-only (SKILL §Cross-Boundary Edits). No GSA path is touched (`crates/media-protocol/**` and `proto/**` are consumed, never edited).

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `crates/env-tests/src/fixtures/video/**` (new directory module: `mod.rs` docs + outcome tables, `schedule.rs`, `encode.rs`, `reader.rs`, `transport.rs`, `publisher.rs`, `subscriber.rs`, `control.rs`) | Mine | — |
| `crates/env-tests/src/fixtures/mod.rs` | Mine | — |
| `crates/env-tests/Cargo.toml` (tokio `test-util` dev-dependency for paused time) | Mine | — |
| `crates/env-tests/src/fixtures/media.rs` | Mine | — |
| `crates/env-tests/src/fixtures/mc_session.rs` | Mine | — |
| `crates/env-tests/README.md` | Mine | — |
| `crates/env-tests/src/fixtures/kube.rs` (Gate 3: comment-only ANCHOR naming the layer7.sh twin) | Mine | — |
| `crates/env-tests/tests/26_mh_quic.rs` (Gate 3: one `const _` assert that the phase floor stays ≥ AUDIO_SEQUENCE_CEILING) | Mine | — |
| `crates/env-tests/tests/35_mc_server_mute_teardown.rs` (Gate 3: same `const _` assert) | Mine | — |
| `packages/web-app/e2e/patternDetector.ts` (new) | Mine | — |
| `packages/web-app/e2e/slotOracle.ts` (new) | Mine | — |
| `packages/web-app/e2e/videoBusShape.ts` (new) | Mine | — |
| `packages/web-app/e2e/videoEvidence.ts` (new) | Mine | — |
| `packages/web-app/e2e/receiveEvidence.ts` (Playwright video-evidence wrappers; `LayerRow` re-export) | Mine | — |
| `packages/web-app/e2e/vp8Probe.ts` (new) | Mine | — |
| `packages/web-app/e2e/cohort.ts` | Mine | — |
| `packages/web-app/e2e/cohortContexts.ts` | Mine | — |
| `packages/web-app/e2e/configEnv.ts` | Mine | — |
| `packages/web-app/e2e/global-setup.ts` | Mine | — |
| `packages/web-app/e2e/README.md` | Mine | — |
| `packages/web-app/e2e/fixtures.ts` (Gate 3: typed optional `senderId` on `JoinedBusEvent`) | Mine | — |
| `packages/web-app/tests/_patternSpec.ts` (new, Gate 3: test-only README-derived pattern layout) | Mine | — |
| `packages/web-app/tests/patternDetector.test.ts` (new) | Mine | — |
| `packages/web-app/tests/slotOracle.test.ts` (new) | Mine | — |
| `packages/web-app/tests/videoEvidence.test.ts` (new) | Mine | — |
| `packages/web-app/tests/vp8Probe.test.ts` (new) | Mine | — |
| `packages/web-app/tests/cohort.test.ts` | Mine | — |
| `packages/web-app/tests/configEnv.test.ts` | Mine | — |
| `packages/web-app/src/__tests__/vp8Probe.test.ts` (new; component tier, test-only) | Not mine, Minor-judgment | client |
| `packages/web-app/vitest.config.ts` (one test-runner-only define) | Not mine, Minor-judgment | client |
| `scripts/layer7.sh` (Phase-1g profile export + verdict-file routing, two REASON tokens) | Not mine, Domain-judgment | infrastructure (paired) |
| `scripts/layer7.test.sh` | Not mine, Minor-judgment | infrastructure |
| `docs/runbooks/devloop-validation.md` | Not mine, Minor-judgment | operations |
| `.github/workflows/ci-client.yml` (paths filter + header comment) | Not mine, Minor-judgment | infrastructure |
| `docs/user-stories/2026-10-07-see-each-other.md` (MC-A KIND ORDER + NO-VIDEO PRECEDENCE amendment lines, lead rulings) | Mine | — |
| `docs/TODO.md` (Gate 3: DRY reviewer's §Cross-Service Duplication key-id-packer entry) | Mine | — |
| `docs/devloop-outputs/2026-10-21-story3-video-test-infra/main.md` | Mine | — |

---

## Planning

### Findings before planning
- **media.rs is already fixed by P2 (7d9a159b), verified.** `AUDIO_SEND_ID = 0` and `gop_counter: stream_sequence` are in tree, so the prompt's "currently stamps 0xFFFF" is stale. MH-A1's prerequisite holds. What remains for this task: (a) `VIDEO_SEND_ID = 1` beside `AUDIO_SEND_ID`, the one home both fixtures use; (b) a residual: `bind_until_received_each` restarts `seq` at 1 on every call, so a sender driven through several phases on one connection sends a REGRESSING gop counter, against ADR §2 "never resets while the sender id lives". Fix: one process-wide monotonic sequence source (`next_audio_sequence()`, an `AtomicU32`) feeding every call. It is monotonic per sender because it is monotonic globally, and `assert_relayed_on_slot` still re-encodes from the received `stream_sequence`. MH ignores audio counters, but the fixture should not model a sender ADR §2 forbids.
- **Not a wire-freeze disagreement.** Nothing in the prompt contradicts §Wire freeze.
- **R-3 ambiguity, reported to @team-lead.** When a subscriber has empty slots of BOTH kinds at one event (its own join, or a redeclaration), the fill ORDER between kinds changes the outcome. Example: A is receive-only (earliest), B publishes both, the subscriber has N=1, M=1. Audio-first gives audio=B, video=B (rule 1 pairs). Video-first gives video=A, audio=B. The story text does not order the kinds. The oracle makes the order ONE named constant (`KIND_FILL_ORDER`, proposed audio then video, iterated to a fixed point one slot at a time) with a test pinning it. MC-A must pick the same order, or the env-tests will (correctly) disagree.

### Deliverable 1 — `crates/env-tests/src/fixtures/video.rs`
Keyless. No crypto import, nothing derived. No `tracing`/`println!`/`eprintln!` anywhere in the module, and no `Debug` of reports inside fixture panic strings. Panics name the phase, slot/sender and outcome only. GoP values and lengths live only in returned values (R-30).

**Hermetic by construction (the seam).** Every rule is a pure core. The wtransport loops only shovel bytes:
- `GroupStartSource { None, OnKeyframeRequest { min_interval: Duration }, Periodic(Duration) }`, default `None`. Final names, agreed with paired-media-handler and recorded in the module docs. `Periodic`'s field and module docs say it is fixture-only and never a production keyframe interval (ADR-0036 as amended has none).
- `GroupScheduler` (pure, takes `tokio::time::Instant`):
  - Counter convention mirrors the SDK `nextGopCounter`. The first group is gop 0. Each later group start takes `checked_add(1)`. At `u32::MAX` the next group start returns `Exhausted`, and the publisher stops the stream with no wrap.
  - `on_request(stream_number: u32, gop: u32, now)`. It is answered only if `stream_number == directed send id` (the directed id, NOT the label knob; values above u16 are just "other stream") AND `gop >= current`. Otherwise it is `IgnoredOtherStream` / `IgnoredLower`. The already-answered repeat falls out of the rule because its counter is now lower; there is no separate dedupe path.
  - `min_interval` faithfully mirrors the client: one deferred slot per stream, re-checked against `>= current` when it fires on tokio time.
  - `Periodic` fires on `next_deadline()`.
  - One ordered event log, `PublisherEvent::{KeyframeRequest { stream_number, gop_counter, outcome: Answered|Deferred|IgnoredLower|IgnoredOtherStream }, GroupStart { gop_counter, cause: StreamStart|Request|Periodic|DeferredRequest }}`. This proves "a request was observed before the group start". It is an in-memory value only.
- Knobs on `VideoPublisherConfig`:
  - `label: UplinkLabel { Directed, Override(u16) }`. Default = the directed send id (`VIDEO_SEND_ID` from media.rs). `Override(AUDIO_SEND_ID)` is the relabel case; any other value is undeclared.
  - `transport: { StreamPerGroup, Datagram }`. Datagram sizes each frame against `max_datagram_size()` at send time and fails loudly if it does not fit, never truncating or splitting.
  - `mode: { Compliant, KeepSendingWhileMuted }`. `set_directed(bool)` is what a compliant publisher obeys; the patched client ignores it (R-16).
  - `fault: { None, StallMidFrame { after_frames }, ResetMidGroup { after_frames } }`. Stall writes a header plus part of the payload, then PARKS the stream handle until `release_stall()` (dropping would FINISH it). Reset calls `reset(RESET_MID_GROUP_CODE)` explicitly; the code is documented as uninterpreted by MH. A group rollover calls `finish()` explicitly.
  - `frame_interval` (default 20 ms) and `payload_len` (default 200 B).
- Frame build: `encode_video_frame(..)` goes through `media_protocol::codec::encode_frame` only.
  - Group start: independently_decodable=true, key_bearing=false, discardable=false. Other frames: all false. Every frame carries its group's counter.
  - Payload = an 8-byte key id in the frame.rs layout (`sender_id(16)|stream(8)=VIDEO_SEND_ID|generation(40)=0`, from sender_id plus fixed constants only), then marker fill. Hop sentinel `0xDEAD_BEEF`.
- Transport seam: `UplinkTransport`/`UplinkStream` traits (open_uni / write_all / finish / reset / send_datagram / max_datagram_size), implemented for `wtransport::Connection`. A `cfg(test)` in-memory recorder implements them for the self-tests. The control source trait `KeyframeRequestSource` is implemented for the MH carrier and, in tests, for a scripted channel. Paused time drives everything (`#[tokio::test(start_paused = true)]` + `advance`), with no QUIC loopback under paused time.
- `VideoSubscriber`: a pure `StreamFrameReader` uses `media_protocol::reader::HeaderReader`, then counts exactly `tail_len()` tail bytes. It keeps only the first `KEY_ID_BYTES` of the payload (for the key-id sender) and never sizes a buffer from `payload_length`. Over-max surfaces as `payload_length_exceeds_max` (reason token only).
  - Per frame: key-id sender, relay slot id, hop_sequence, gop_counter, independently_decodable.
  - Per stream: whether the first frame was a group start, whether the slot id changed within the stream, and the end as `Finished | Reset | Partial` (ended mid-header or mid-tail is never reported as a frame).
- Recovery request: `recovery_request_frame(slot_id, sender_id, gop_counter)` is a `MhClientMessage::RecoveryRequest` framed by `mc_session::frame()`. Every field is explicit, and any sender_id is allowed (it drives source_mismatch).
- Carrier: `MhSession` gains `control(&mut self) -> (&mut SendStream, &mut RecvStream)`. It is BORROW-only, so the hold-open invariant cannot be broken by a move or drop. The bounded length-prefixed read in `McSession::try_read` is factored into one `read_framed(recv, bound, max, what)` that both use. Inbound `MhServerMessage` is bounded at 64 KiB before allocation, and a malformed message or one without its oneof panics. Docs: until MH-B lands, MH's hold-open loop discards carrier bytes, so recovery writes have no effect yet.
- Module docs:
  - the outcome table verbatim from the prompt, plus media-handler's three additions (single-stream DROP_AT_INGRESS → `server_muted` after attribution with audio unaffected; the all-DROP sender shortcut runs FIRST; the stream path: label 1 uni → video slots only, PAUSED forwarded + counted, label 0 uni → `transport_mismatch`);
  - metric names in full (`mh_media_frames_dropped_total{reason}`, `mh_media_paused_source_frames_total`);
  - the recovery table (equal → forwarded, lower → stale, higher → counter_ahead with no clamping, wrong sender → source_mismatch and never upstream, plus `unmapped_slot` for a datagram slot);
  - asserting owners: MH-A1 (task 8), MH-A2 (task 9), MH-B (task 10). The fixture's own tests assert no MH behaviour;
  - pending-gauge sources: `None` = a late subscriber waits while MH retries; bound-but-never-started = zero requests answerable;
  - the per-file edge-sum header rule (Σ over concurrently running fns of P×(N+M); `cargo test` runs one file's fns in parallel and files sequentially; INF-C sizes the Kind ceiling ≈120 against the largest per-file sum, 72 in `27_mc_slot_placement`).

Rust self-tests (inline `#[cfg(test)]`, run by Layers 1–6, no cluster):
1. Each knob's bytes decode through the production `decode_stream_frame`/`decode_datagram` to exactly the claimed fields: label Directed/Override(audio)/undeclared; transport stream vs datagram; group-start flags; payload key-id sender; hop sentinel; counter carried by every frame of a group.
2. Framing split at EVERY offset 0..=len of a two-frame stream (so splits land in both headers and both tails), with field equality asserted, not `is_ok()`. Plus a declared `payload_length` far over max on a 14-byte buffer → `payload_length_exceeds_max` with no tail bytes supplied. Plus mid-header/mid-tail ends → `Partial`.
3. `OnKeyframeRequest` in ONE scripted sequence with exact group-start counts and outcomes: equal → 1, higher → 1, lower → 0, other stream → 0, answered repeat → 0. Also the log order (request index < its group start index), `min_interval` deferral (one deferred, fires at the floor on paused time, re-checked), and `u32::MAX` → `Exhausted` (no wrap, stream stops).
4. `Periodic` on paused time: exactly k group starts at k×interval and none at interval−1 ms. `None`: none after the first, over a long advance.
5. Recovery request round-trips through prost with every field non-default.
6. Stall / reset / finish are told apart on the recorder: stall = complete header + partial tail with neither finish nor reset until release; reset = `reset(code)` with no finish; rollover = `finish()`.
7. Datagram over `max_datagram_size` panics loudly. KeepSendingWhileMuted sends while undirected; Compliant sends nothing.
8. media.rs: `audio_datagram` decodes with stream_id 0 and gop == seq; `next_audio_sequence()` is strictly increasing across calls.

### Deliverable 2 — `e2e/patternDetector.ts` (spec agreed with paired-client)
- The spec is written ONCE in README prose. It is a 6×4 grid with row-major i = r·6 + c and blocks spanning floor(c·W/6)… as client stated:
  - row0 `G0 I15 I14 I13 I12 G1`
  - row1 `I11..I6`
  - row2 `I5..I0`
  - row3 `G2 C3 C2 C1 C0 G3`
  - Corner guards G0=white, G1=black, G2=black, G3=white. Gray(n mod 16) MSB first, advancing on every CAPTURED frame.
  - Input is sender_id 1..65535 only. The README notes known non-catches: a 180° rotation keeps the guards and is caught by the expected id; a shift catches deterministically only where a corner lands on opposite content, and the counter corner makes it fail within any live window.
- Luma projection is per block: the mean of Y = .299R + .587G + .114B over the central half, an integer 0..255, sampled at the render canvas, and absent (never zero-filled) on avatar/empty slots.
- API (client's Q1, accepted):
  - `decodePattern(lumas) → { decided, guardPass, id|null, counter|null }`.
  - `judgeSlotPattern(samples, expectedId) → { idMatch, guardPass, liveness: 'live'|'frozen'|'insufficient', decodedIds }`.
  - Named constants `BLACK_MAX = 80`, `WHITE_MIN = 175`, with margins documented. Any undecided block = not a pattern frame, never coerced.
  - guardPass over a window = every sample passed. Liveness counts only decided+guard-passing samples: ≥2 of them, with a counter change across any pair. Never require +1.
  - One sample → `insufficient` (a verdict the evidence helper turns into a HARNESS throw).
  - Counter changes but the guard fails mid-window → `guardPass:false` (the guard verdict fails the slot) and liveness is computed over the valid samples.
  - Malformed input (length 23/25, NaN, null, non-integer, <0, >255) THROWS `PatternInputError`.
- `tests/patternDetector.test.ts` builds lumas from its OWN hand-written layout table, transcribed from the README, never importing detector constants. It adds literal grids (e.g. 0x8001 → indices 1 and 17 white). It covers:
  - every bit of 16;
  - guard fail on blank, inverted, mirrored, and one-column/one-row shifts (fixtures chosen per spec, plus a live-window shift);
  - liveness gaps/frozen/single;
  - noise within margin accepted, beyond rejected;
  - the malformed throws.

### Deliverable 3 — `e2e/slotOracle.ts`
An event-sourced model with no MC import, written from R-3's text:
- `join(id, { publish, audioSlots, videoSlots, handlers })`, `leave`, `setMute(id, kind, { client, server })`, `redeclare`, `setHandlers`, `expected(subscriber)`.
- Fill: per subscriber per kind, (1) the earliest joiner holding the other kind but not this one, else (2) the earliest eligible joiner holding none of this kind.
- Eligibility: shares a connected handler with the subscriber, is not the subscriber, and (audio) publishes audio.
- An occupied slot moves only on its occupant's leave or the pair becoming unreachable. Over-cap truncates to `unavailable`.
- Output per slot: `{ state, occupant? }`. Video: `active` (publishes video, no mute), `avatar_no_video`, `avatar_client_muted`, `avatar_server_muted` (server wins), plus `fewer_sources` and `unavailable`. Audio mirrors story 2.
- The six wire tokens are typed locally. The SDK union lacks them until C-A1 lands, so C-C1 adds `satisfies` against the SDK union then (noted in README).
- `KIND_FILL_ORDER` as above.
- `tests/slotOracle.test.ts`: hand-written expected occupants per scenario (never a second oracle call), covering pair completion, stability across a leave, receive-only in a video slot, truncation, the no-camera-holds/later-camera-unseen consequence, two-handler visibility, and the kind-order pin.

### Deliverable 4 — cohort + evidence
- `cohort.ts`:
  - `PublishSet = 'audio'|'video'|'both'|'none'`;
  - `CohortMemberPlan { publish, audioReceiveSlots, videoReceiveSlots }`;
  - `assertPlanFitsCohort(plan, registeredSize)`, so registration stays once per run within AC's window (the limit is still read from AC config);
  - pure `leversForPlan(member)` mapping to the lever names client implements (to be confirmed: keep the existing `receiveSlots` for audio N and add `publish` and `videoReceiveSlots`, so the one concept keeps one name).
- `cohortContexts.ts` `ContextLevers` gains those optional fields (type only). No existing spec passes them, so the app's strict lever parser is unaffected until client lands them.
- `videoBusShape.ts`: the agreed bus types and strict parsers (`videoSlots` without `atMs`, `slotAssignments.kind`, `sendTracks`, `videoCaptureSource`). Malformed → throw.
- `videoEvidence.ts` (beside `receiveEvidence.ts`; reuses its `LayerRow`/`latestLayers` and `windowSampling.sampleWindow`, counting samples by order):
  - pure judges for layer 1 (`renderedSenderId` == the oracle's expected occupant), layer 2 (`receiveLayers` verified for (slot, sender) advances over the window), layer 3 (`judgeSlotPattern` with the expected occupant's OWN announced `videoCaptureSource.patternId`);
  - the MC state token and rendered video/avatar/empty;
  - unreachable from `unreachableSenderIds`;
  - the send side: `cameraTrackLive === (directed && !clientMuted && cameraHealthy)`, mic stays open on audio mutes;
  - Playwright wrappers. Failure text names slot, labels, expected/actual states and booleans; decoded ids appear only in thrown assertion text (meeting-scoped sender_id, story-2 precedent), never attached or logged.
- `tests/videoEvidence.test.ts`: fixture bus snapshots, one independent failure per layer (wrong sender / count not advancing / wrong detector id), a camera positive control (an unwatched publisher reading live=true fails), and state/rendered mismatches.
- `tests/cohort.test.ts` is extended.

### Deliverable 5 — `e2e/vp8Probe.ts` (no SDK import; `probeVp8Support` is not touched)
- `runVp8ProbeInPage(profile)` is fully self-contained (page.evaluate-serialisable). It reads `globalThis.VideoEncoder/VideoDecoder/VideoFrame/OffscreenCanvas` (the node-test seam):
  - checks secure context;
  - checks `isConfigSupported` for both;
  - encodes one synthetic OffscreenCanvas frame with `keyFrame:true`;
  - decodes with the encoder's emitted decoderConfig;
  - compares displayWidth/Height.
- Every step is raced against `PROBE_STEP_BOUND_MS` (a liveness bound, never a latency gate; fake timers in the node test). Frames and codecs are closed in finally.
- The observation carries only booleans, numbers and enum outcomes, with no browser free text. The pure `classifyVp8Probe(obs, profile)` maps it to `pass | webcodecs_unavailable | encoder_config_unsupported | decoder_config_unsupported | encode_failed | decode_failed | dimension_mismatch`.
- Profile validation (codec === 'vp8', positive ints) throws as a harness error. The profile reaches page.evaluate as a structured argument.
- `runVp8ProbeInBrowser(browser, profile)` (Layer 7) does `page.route('https://vp8-probe.invalid/**')` → goto, which gives a secure origin with no new launch flag.
- **Layer 7** (paired-infrastructure's design, adopted):
  - Phase 1g removes and exports `E2E_VP8_VERDICT_FILE=${DEVLOOP_TMP}/layer-7-vp8-verdict`.
  - It resolves the DEPLOYED profile the way `kube.rs:resolve_generation` does (mc-service pods → exactly one `media-codec-profile-<hash>` generation → `kubectl get cm` → `KEY=value` dump to `${DEVLOOP_TMP}/layer-7-media-codec-profile.env`, exported as `E2E_MEDIA_CODEC_PROFILE_ENV`). Bash dumps ALL keys and lists none. Failure → `precondition_fail media-codec-profile-unresolved`.
  - `global-setup.ts` runs the probe FIRST, before certs and cohort. Its profile comes from `readVideoProfile` over that file, and it THROWS if the env var is unset, with no repo fallback.
  - On an unsupported verdict it writes exactly `browser-vp8-unsupported` (line 1) plus a sub-check detail line, then throws with remediation. A profile-file parse failure writes `media-codec-profile-unresolved`. A harness error writes nothing.
  - layer7.sh's `browser_rc != 0` branch: line 1 ∈ that fixed two-token list → `precondition_fail <token>` with a cause naming the sub-check and probed codec/WxH/fps/bitrate (config, not identity). Otherwise today's FAIL. This fails closed.
  - Residual (ops O1): the probe runs after the Rust suite. A Phase-1g pre-suite run was rejected because there is no TS runner outside Playwright, and a Playwright run triggers global-setup's cohort registration. Recorded in the runbook row.
- **CI**: `configEnv.ts` gains `mediaCodecProfilePath()` and `readVideoProfile(text, source)` (the five key constants declared once; codec non-empty, four via `requirePositiveInt`). Both callers use it. `vitest.config.ts` injects `__E2E_VP8_PROBE_PROFILE__` (test-runner only; not in vite.config.ts, so it never reaches an app build). `src/__tests__/vp8Probe.test.ts` runs the real probe in Chromium browser mode under `pnpm test:component`. A failure fails the job, with no skip.
- Tests:
  - `tests/vp8Probe.test.ts`: every verdict, encode throws, decode yields no frame, an output never firing → bounded failure on fake timers, and invalid-profile throws;
  - `tests/configEnv.test.ts`: `readVideoProfile` over the real profile.env matches the file's own text, which proves vitest.config's define is derived, not hardcoded (the testDefines precedent);
  - `layer7.test.sh` (a) token → PRECONDITION_FAILURE/exit 2; (b) no file → FAIL browser-e2e-failed/exit 1; (c) stale pre-created file → FAIL (proves the `rm -f`); (d) FAKE_KUBECTL answers pods/cm → the stub sees the dumped profile; two generations → `media-codec-profile-unresolved`; (e) `media-codec-profile-unresolved` written by the suite routes too.
- Runbook: both tokens in all three places (L7 token list, §6.7 table, symptom catalogue). vp8 remediation: first check whether this diff touched `infra/services/media-codec-profile/` or the Kind profile patch, then the Playwright Chromium pin/bake. Do not edit the probe to pass.
- `ci-client.yml`: add `infra/services/media-codec-profile/**` to the paths filter, and fix the stale "node-based" header line.

### Gate-1 amendments (supersede the text above where they differ)
- **Kind fill order, lead ruling:** audio then video, one slot at a time, to a fixed point. It is pinned by `KIND_FILL_ORDER` plus a test. A dated amendment line in the story's MC-A SLOT TABLE "Fill order" bullets makes it one source that MC-A reads.
- **Verdict file carries ONE token (ops O7, which wins over the earlier two-token addition).**
  - Global-setup writes `browser-vp8-unsupported` only, as line 1. Line 2 is a sub-check from the fixed list `webcodecs_unavailable|encoder_config_unsupported|decoder_config_unsupported|encode_failed|encode_timeout|decode_failed|decode_timeout|dimension_mismatch`. The timeout and error variants are distinct, so a hang is never triaged as a codec error (test C).
  - layer7.sh validates line 2 against `VP8_PROBE_SUBCHECKS` (security P2). A bad line 2 is FAIL with a fixed cause. Nothing file-borne reaches the status output unvalidated.
  - A node test parses that bash array and asserts it equals the TS list, so the two copies cannot drift.
  - `media-codec-profile-unresolved` is emitted ONLY by Phase-1g bash: kubectl failure, generation ≠ 1, or cm-read failure, all decided on exit status. A global-setup profile parse failure, or an unset `E2E_MEDIA_CODEC_PROFILE_ENV`, is a harness error: it throws, writes nothing, and lands in FAIL browser-e2e-failed. The real-profile.env `tests/configEnv.test.ts` stays mandatory as the L1–6 positive control.
  - Routing cases:
    - (a) token + valid sub-check, rc≠0 → PRECONDITION exit 2;
    - (b) no file → FAIL;
    - (c) a stale pre-created file → FAIL (proves the `rm -f`);
    - (d) a garbage or `media-codec-profile-unresolved` line 1 → FAIL;
    - (e) a bad line 2 → FAIL;
    - (f) a valid file with rc 0 → loud FAIL (never silent green);
    - (g) the Phase-1g profile export, and two generations → `media-codec-profile-unresolved`.
  - The cause names the sub-check and the dumped profile path. The probed values are printed by global-setup's own error in the browser log; they are not read from the verdict file.
- **Layer 3 anchor (security P1):** the expected id is the expected occupant's sender_id from the harness's own join binding, the same source as layer 1. `videoCaptureSource.patternId === senderId` is asserted separately on the send side, and a test case shows a mismatch fails.
- **Bus identity carve-out (observability):** R-30's "no sender identity on the test bus" vs R-32 layer 1. `renderedSenderId`, `receiveLayers.senderId` and `videoCaptureSource.patternId` carry the meeting-scoped sender_id on the TEST-BUILD bus only, a deliberate R-32 carve-out following the story-2 `receiveLayers` precedent (security S4). There is never a stream number, relay id, GoP, size, key id, timing or durable identity. The `videoBusShape.ts` parsers are EXACT-KEY: any extra field throws.
- **Slot tokens (DRY):** `active` and `fewer_sources` are typed now with `satisfies readonly SlotStateToken[]`. Only the five pending tokens are local, with the C-C1 note at the type site.
- **Test reviewer A–D:**
  - (A) One exported `vp8ProbeProfileDefine()` helper is what `vitest.config.ts` calls, with a node test that it equals `readVideoProfile(profile.env)`. The component test throws on a missing or malformed define and asserts `verdict === 'pass'`.
  - (B) The cases above.
  - (C) The timeout and error sub-checks.
  - (D) `Exhausted` is tested via a test-only scheduler constructor at `u32::MAX`.

- **Code-reviewer CQ1–CQ5:**
  - CQ1: `browser-vp8-unsupported` and the sub-check list are declared once in `vp8Probe.ts`. A node test reads `scripts/layer7.sh` and asserts the bash token and `VP8_PROBE_SUBCHECKS` equal the TS ones. Its positive control: the extraction must find the array, or it fails with a separate reason.
  - CQ2: `next_audio_sequence()` stays process-wide. Threading a per-sender counter would change the signatures at 12 call sites across `26`/`27`/`35`, which own their own phase floors. Its doc states the invariant: the global count stays far below the lowest phase floor (1M), and bind phases precede floor phases.
  - CQ3: the key id is packed and decoded from frame.rs's `KEY_ID_*` constants with checked conversions. A `stream_number` u32 above u16 is "other stream" via `u16::try_from`.
  - CQ4: `video` is a directory module.
  - CQ5: `read_framed` text is phase + `what` + reason only.
- **Paired media-handler conditions:**
  - the KID `stream` byte is the DIRECTED send id, never the label, so `Override(AUDIO_SEND_ID)` gives key-id stream 1 with relay label 0 (MH-A1 routes by the relay label; documented);
  - `generation = 0` is documented as a placeholder;
  - `payload_len >= KEY_ID_BYTES` is checked at config time, and a short payload on the subscriber is a harness error;
  - the KID is never in panic strings, Debug or logs;
  - the docs note that media.rs audio stays marker-only.

### Not in scope (owned elsewhere)
Generator, bus projection, lever parsing and the bundle FORBIDDEN entry (client C-A*/C-B/C-C1; the README contract states the forbidden-token expectation). Scenario env-tests (tasks 8/9/10/12/15/17). No new metric series, labels, dashboards or alerts.

---

## Pre-Work

None

---

## Implementation Summary

Implemented as planned, including every Gate-1 amendment. `./scripts/layer-fast.sh` is green on L1–L6:
- nextest: 4302 tests passed;
- web-app node tier: 15 files, 396 tests;
- web-app component tier: 14 files, 114 tests, including the real VP8 round trip in Chromium;
- `scripts/layer7.test.sh`: 568 passed.

**Deliverable 1 — Rust fixtures**
- `fixtures/video/` is a directory module, per CQ4:
  - `mod.rs` holds the contract docs: knob names, GoP convention, the MH outcome table and recovery table with full metric names and asserting owners, pending-gauge sources, and the per-file edge-sum rule.
  - `schedule.rs`: `GroupScheduler` and the ordered `PublisherEvent` log.
  - `encode.rs`: frames go through `encode_frame`; the KID is packed from the `KEY_ID_*` constants with the stream byte equal to the directed id and generation 0 as a placeholder.
  - `reader.rs`: `StreamFrameReader` over `HeaderReader`. It counts tail bytes and keeps only the KID.
  - `transport.rs`: the `UplinkTransport`/`UplinkStream` seam, implemented for wtransport, with an in-memory recorder for tests.
  - `control.rs`: a cancel-safe carrier keyframe-request reader (same bound as the other carrier reads; the reason it does not use `read_framed` is documented) and the recovery-request writer.
  - `publisher.rs` and `subscriber.rs`.
- 34 hermetic self-tests run on tokio paused time with no QUIC. To enable paused time, env-tests gains a tokio `test-util` dev-dependency.
- `media.rs`:
  - adds `VIDEO_SEND_ID`;
  - `next_audio_sequence()` is checked against `AUDIO_SEQUENCE_CEILING`, which fixes the per-phase GoP regression;
  - documents the caller-driven, marker-only audio frame;
  - adds three tests.
- `mc_session.rs`:
  - one shared `read_framed()` (now used by `McSession::try_read`);
  - `MAX_FRAME_BYTES` is now `pub`;
  - borrow-only `MhSession::control()`.
- `crates/env-tests/README.md` documents the fixtures and the edge-sum rule.

**Deliverables 2–4 — TypeScript**
- `patternDetector.ts`: corner-guard spec, 80/175 thresholds, and `PatternInputError` for malformed input.
- `slotOracle.ts`: `KIND_FILL_ORDER`. `active`/`fewer_sources` use `satisfies SlotStateToken[]`; the other four tokens are local, with a note at the type site.
- `videoBusShape.ts`: exact-key parsers.
- `videoEvidence.ts`: pure judges for the three layers, slot state, rendered kind, the send side, and `patternId === senderId`.
- `receiveEvidence.ts`: the Playwright wrappers. A node test cannot import `fixtures.ts`/`env.ts`, so the wrappers sit beside the story-2 helpers; `LayerRow` has one home and is re-exported.
- `cohort.ts`/`cohortContexts.ts`: `PublishSet`, plans, `assertPlanFitsCohort`, and `leversForPlan`, which emits `receiveSlots`, `publish` and `videoReceiveSlots`.
- `e2e/README.md`: spec, projection, bus contract and evidence sections.
- Node tests: four files, 152 tests.

**Deliverable 5 — VP8 probe**
- `vp8Probe.ts`:
  - self-contained `runVp8ProbeInPage`;
  - pure `classifyVp8Probe`;
  - 8 sub-checks, with timeout distinct from error;
  - `runVp8ProbeInBrowser` on a `page.route`'d secure origin;
  - no SDK or src import.
- `configEnv.ts`: `readVideoProfile`, `mediaCodecProfilePath`, and `vp8ProbeProfileDefine`, which `vitest.config.ts` calls in one line.
- `global-setup.ts`: runs the probe first, with no repo fallback, and writes the verdict file only on an unsupported verdict.
- Component test: `src/__tests__/vp8Probe.test.ts`. Node test: `tests/vp8Probe.test.ts`, covering every verdict and step hangs on fake timers, plus a drift test against `layer7.sh` that has a distinct extraction-failure error.
- `scripts/layer7.sh`:
  - Phase 1g removes the stale verdict file and dump, and dumps the deployed profile via `__dump_media_codec_profile`, which resolves the generation and routes to `media-codec-profile-unresolved`;
  - `__route_vp8_verdict` validates line 1 and line 2 before `precondition_fail`, and fails closed otherwise;
  - rc 0 with a verdict file present is a loud FAIL.
- `scripts/layer7.test.sh`: cases V1–V8.
- Runbook: both tokens in all three places.
- `ci-client.yml`: paths filter and header updated.

**Gate-3 fixes (all fixed, none deferred).** `layer-fast.sh` green again; `layer7.test.sh` 585 passed, web-app node tier 406 passed.
- Observability F1: exact-key `parseReceiveLayers`, used at both read sites.
- Security:
  - F1: `Debug` redacted on `PublisherEvent`, `Decision`, `GroupScheduler`, `UplinkLabel` and `VideoPublisherConfig`, with tests.
  - F2: README gating and lumas bullets.
- Test:
  - F1: one README-derived `tests/_patternSpec.ts`.
  - F2: import scanners now catch side-effect imports and `require`.
  - F3: falling back to an avatar mid-window fails.
  - F4: the duplicate send-side row is dropped, with a doc note.
  - F5: `read_stream` sits behind a `DownlinkStream` seam, with four event-sequence tests.
- Code-reviewer:
  - F1: `#[expect]`.
  - F2: one slot-token home.
  - F3: `const _` floor asserts in tests 26 and 35.
  - F4: `checked_frame_len`.
  - F5: `fixture_signature`.
  - F6: no launch args.
  - F7: readers held in a `JoinSet`.
  - F8: `on_request` simplified.
  - F9: `KINDS`.
  - F10: codec list.
  - F11: thresholds documented as a design expectation (the client's measurement is cited).
  - F12: typing, `PUBLISH_SETS`, `judgeLayer1/2/3`.
- DRY:
  - F1: one slot-token home.
  - F2: `HOP_SENTINEL` lives in media.rs.
  - F3: the jq matches kube.rs (projected volumes, 10-char hash), with an ANCHOR at both sites and mirrored test cases.
  - F4: one `unquote`.
  - F5: `PUBLISH_SETS`.
  - F6: the KID packer carries an ANCHOR and refuses sender 0. The extraction itself is filed by the DRY reviewer in `docs/TODO.md`.
- Operations OPS-1: `PROBE_TOTAL_BOUND_MS` sets the component-test timeout.
- Paired-infrastructure:
  - F1: the error file is now `.stderr.log`.
  - F2: a pod-list parse failure has its own cause.
  - F3: no launch args.
  - F4: `pods-unlistable` and `pods-garbage` cases.
- Paired-client:
  - F1: the join-bound sender comes from `joined.senderId`.
  - F2: no launch args.
  - F3: vitest header corrected.
- Paired-media-handler:
  - F1: STOP_SENDING is an observable `StoppedByPeer` outcome; frames are withheld until the next group start.
  - F2: `Next::Quiet` is returned only while the connection is live, plus `ConnectionClosed`.
  - F3: compliant withdrawal finishes the group stream, requests are `NotSending`, and re-direction is a `Resume` group.
- Lead ruling: NO-VIDEO PRECEDENCE amendment in the story, with a pinning oracle test.

**Story file** — dated KIND ORDER amendment line under MC-A's SLOT TABLE (lead ruling).

**Not run here**: Layer 7 itself (the probe on the live cluster) is the Lead's `layer-all.sh` run. The probe was exercised on this Chromium through both callers' code paths: the component tier, and a throwaway Playwright spec at the base and Kind test profiles.

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
| Paired client | confirmed |
| Paired media-handler | confirmed |
| Paired infrastructure | confirmed |

---

## Gate 3 — Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security (paired) | RESOLVED-FIXED | 2 | 2 | 0 | Debug redaction class-wide; README gating/lumas contract |
| Test | RESOLVED-FIXED | 6 | 6 | 0 | README-parsed pattern spec; paused time everywhere |
| Observability | RESOLVED-FIXED | 1 | 1 | 0 | exact-key parseReceiveLayers |
| Code Quality | RESOLVED-FIXED | 12 | 12 | 0 | import graph clean |
| DRY | RESOLVED-DEFERRED | 6 | 6 | 0 | +1 ADR-0019 extraction opportunity filed (key-id packer → media-protocol) |
| Operations | RESOLVED-FIXED | 1 | 1 | 0 | component-test timeout derived from probe bound |
| Paired client | RESOLVED-FIXED | 3 | 3 | 0 | |
| Paired media-handler | RESOLVED-FIXED | 3 | 3 | 0 | STOP_SENDING typed; ConnectionClosed; withdraw→new group |
| Paired infrastructure | RESOLVED-FIXED | 4 | 4 | 0 | |
| Semantic Guard | not spawned | — | — | — | |

Lead rulings: R-3 KIND_FILL_ORDER (audio then video, fixed point) at Gate 1; NO-VIDEO PRECEDENCE at Gate 3 — both recorded as dated amendments in the story's MC-A slot table.

---

## Accepted Deferrals

- DRY extraction opportunity (ADR-0019, trigger = a third key-id packer): shared key-id pack/unpack in `crates/media-protocol` (GSA) replacing `media-vector-gen/src/kid.rs` + `env-tests/src/fixtures/video/encode.rs` packers — `docs/TODO.md` §"From DRY Reviewer — story 3 task 7 (2026-10-21)". Both sites carry `ANCHOR (DRY)` notes.
