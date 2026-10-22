# Devloop Output: C-B SDK video receive path (story 3 task 14)

**Date**: 2026-10-22
**Task**: Story 3 task 14 (C-B): SDK video receive path — uni-stream reader, receive policy, M slot pipelines, render decision, recovery requests
**Specialist**: client
**Mode**: Agent Teams (v2) — full, Gate-1 present
**Branch**: `feature/user-see-each-other`
**Duration**: ~3h

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `fb8c03a02f8b8275d21aad7f7d9ba8857afa4b6d` |
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
| Implementing Specialist | `client` |
| Tier | `full` |
| Iteration | `1` |
| Security | `security` (paired) |
| Test | `test` (paired) |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Semantic Guard | `semantic-guard` |

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
Build the SDK video receive path for story 3 (task 14, C-B): the incremental uni-stream reader, one receive policy by kind, exactly M reusable VP8 slot pipelines, the live/avatar render decision behind a `VideoSink` seam, event-driven receiver recovery requests, video replay/key scoping, SDK config knobs, telemetry (incl. the R-30 unfulfilled-slot series) and the R-32 evidence hooks.

### Scope
- **Service(s)**: Browser SDK (`packages/sdk-core`, `packages/test-utils`); observability docs/dashboard; OTel collector allowlist; client runbook
- **Schema**: No
- **Cross-cutting**: No (client only; no wire/GSA change)

### Debate Decision
NOT NEEDED - implements a decomposed story task; the open questions (E1–E3, label/shape rulings) were adjudicated at Gate 1 by the Lead and the owning reviewers

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
| `packages/sdk-core/src/media/frame/frameCodec.ts` | Mine | — |
| `packages/sdk-core/src/media/frame/rejectReason.ts` | Mine | — |
| `packages/sdk-core/src/media/pipeline/streamReader.ts` (new) | Mine | — |
| `packages/sdk-core/src/media/pipeline/frameAccounting.ts` (new) | Mine | — |
| `packages/sdk-core/src/media/pipeline/receivePolicy.ts` (new) | Mine | — |
| `packages/sdk-core/src/media/pipeline/ingress.ts` | Mine | — |
| `packages/sdk-core/src/media/pipeline/videoIngress.ts` (new) | Mine | — |
| `packages/sdk-core/src/media/pipeline/videoSlotPipeline.ts` (new) | Mine | — |
| `packages/sdk-core/src/media/pipeline/renderDecision.ts` (new) | Mine | — |
| `packages/sdk-core/src/media/pipeline/decoderRestartGate.ts` (new) | Mine | — |
| `packages/sdk-core/src/media/pipeline/receiveLanes.ts` | Mine | — |
| `packages/sdk-core/src/media/teardown/teardown.ts` | Mine | — |
| `packages/sdk-core/src/media/frame/sframe.ts` | Mine | — |
| `packages/sdk-core/src/media/pipeline/__tests__/**` | Mine | — |
| `packages/sdk-core/src/media/lifecycle/VideoReceiver.ts` (new) | Mine | — |
| `packages/sdk-core/src/media/lifecycle/recoveryRequestQueue.ts` (new) | Mine | — |
| `packages/sdk-core/src/media/lifecycle/unfulfilledSlots.ts` (new) | Mine | — |
| `packages/sdk-core/src/media/lifecycle/AudioPipeline.ts` | Mine | — |
| `packages/sdk-core/src/media/lifecycle/receiveTransports.ts` | Mine | — |
| `packages/sdk-core/src/media/lifecycle/receiverState.ts` (new: `bindReceiverState`) | Mine | — |
| `packages/sdk-core/src/media/setup/vp8.ts` | Mine | — |
| `packages/sdk-core/src/signaling/rosterKeyFeed.ts` | Mine | — |
| `packages/sdk-core/src/media/lifecycle/__tests__/**` | Mine | — |
| `packages/sdk-core/src/media/setup/seams.ts` | Mine | — |
| `packages/sdk-core/src/media/setup/vp8Decoder.ts` (new) | Mine | — |
| `packages/sdk-core/src/media/setup/mediaMetrics.ts` | Mine | — |
| `packages/sdk-core/src/media/setup/__tests__/**` | Mine | — |
| `packages/sdk-core/src/media/MediaTransport.ts` | Mine | — |
| `packages/sdk-core/src/media/__tests__/**` | Mine | — |
| `packages/sdk-core/src/config/clientConfig.ts` | Mine | — |
| `packages/sdk-core/src/session/mediaStart.ts` | Mine | — |
| `packages/sdk-core/src/session/mediaWiring.ts` | Mine | — |
| `packages/sdk-core/src/session/signalingMediaState.ts` | Mine | — |
| `packages/sdk-core/src/session/MeetingSession.ts` | Mine | — |
| `packages/sdk-core/src/session/events.ts` | Mine | — |
| `packages/sdk-core/src/session/__tests__/**` | Mine | — |
| `packages/sdk-core/src/signaling/pairing.ts` | Mine | — |
| `packages/sdk-core/src/signaling/__tests__/pairing.test.ts` | Mine | — |
| `packages/sdk-core/src/index.ts` | Mine | — |
| `packages/sdk-core/src/telemetry/MetricsSink.ts` | Not mine, Minor-judgment | observability |
| `packages/sdk-core/src/telemetry/OtelMetricsSink.ts` | Not mine, Minor-judgment | observability |
| `packages/sdk-core/src/telemetry/__tests__/sinks.test.ts` | Not mine, Minor-judgment | observability |
| `packages/test-utils/src/contracts/MetricsSink.ts` (parity copy) | Not mine, Minor-judgment | observability |
| `packages/test-utils/src/InMemoryMetricsSink.ts` | Not mine, Minor-judgment | observability |
| `packages/test-utils/src/media/video.ts` (new) | Mine | — |
| `packages/test-utils/src/index.ts` | Mine | — |
| `packages/test-utils/src/__tests__/FakeVideoDecoders.test.ts` (new) | Mine | — |
| `packages/sdk-core/vitest.config.ts` (coverage exclude for browser-only `vp8Decoder.ts`) | Mine | — |
| `packages/sdk-core/src/config/__tests__/videoReceiveConfig.test.ts` (new) | Mine | — |
| `packages/sdk-core/src/signaling/SignalingClient.ts` (roster index delegates) | Mine | — |
| `packages/sdk-core/src/signaling/__tests__/rosterKeyFeed.test.ts` | Mine | — |
| `docs/observability/metrics/client.md` | Not mine, Minor-judgment | observability |
| `docs/observability/label-taxonomy.md` (recovery `reason` vocabulary row) | Not mine, Minor-judgment | observability |
| `docs/observability/dashboards.md` | Not mine, Minor-judgment | observability |
| `docs/observability/alerts.md` (deliberate-absence inventory for receive-path series, if required) | Not mine, Minor-judgment | observability |
| `infra/grafana/dashboards/client-media.json` | Not mine, Minor-judgment | observability |
| `infra/services/otel-collector/collector.yaml` (name allowlist + cardinality recompute) | Not mine, Minor-judgment | infrastructure |
| `docs/runbooks/client-dev-local.md` (receive-config refuse-to-start, `stream_slot_mismatch`) | Not mine, Minor-judgment | operations |
| `docs/TODO.md` (ADR §2 "timestamp inside the ciphertext" has no producer — lead ruling E1) | Not mine, Minor-judgment | protocol |
| `docs/specialist-knowledge/client/INDEX.md` | Mine | — |

No Guarded Shared Area path is edited: `proto/**` (vectors file, `signaling.proto`) and `crates/media-protocol/**` are READ only. The ADR-0027 "crypto primitives wherever referenced" criterion: no new primitive call site — the video policy reuses `verifyFrame` / `openVerifiedFrame` unchanged; security is paired on it.

------|----------------|---------------------|

---

## Planning

### Mechanism restatement

Instance view: "add a video receive path". Mechanism view: **a second transport-shaped ingress (ordered byte streams, many frames per stream) feeds the SAME receive policy as the datagram ingress. Policy output then goes to a fixed set of M per-slot decode pipelines, not to per-sender lanes. What is RENDERED is a pure function of (MC slot state, verified sender of the held decoded frame, sender guards).** The wider class this exposes has same-owner siblings, all mine and all in this plan:
- the audio ingress must call the same policy kernel, so "one policy by kind" is literally one function;
- the accounting identity's counting point (`ingress.ts` NOTE) moves to the parse boundary for both kinds;
- the rebind purge, generation discard and teardown clear must cover the video receiver's key cache and replay window ("complete the invariant": `rosterInvalidationListener` purges both);
- the pairing view gains live video.

Nothing outside client + observability/ops docs belongs to the class.

### Escalations to @team-lead (sent; recommendation in bold)
- **E1 — no sender PTS exists.** ADR §2 says the payload carries a PTS "inside the ciphertext", but C-A2 seals bare VP8 (`videoEgress.ts:331`), and audio seals bare Opus. **Recommend (A): the receiver timestamps each whole frame at arrival on the injected clock, and re-basing maps a new source's arrival times onto the slot's monotonic playout timeline.** No wire or publish change. This also removes the hostile-PTS surface (security Q4). The missing producer is recorded as a TODO. (B) would add a PTS prefix on both sides, which changes C-A2's landed payload and isn't in the wire freeze.
- **E2 — no token for "frame on an undeclared/non-video slot id".** **Recommend `stream_slot_mismatch` plus a stream abort, also bumping the existing `undeclared_stream_id_total` diagnostic.** An audio slot id on a uni stream is treated the same (security 6). The alternative is a new vectors token (proto/** GSA, protocol-owned).
- **E3 — `DEFAULT_RECEIVE_VIDEO_SLOTS` stays 0** with a new rationale: until C-C1 renders a grid, a default M>0 directs publishers' cameras at a client with no UI to show them (privacy + egress cost). dev-web exports M=3 (validated at that M, see config), and playwright still pins 0 (R-33). C-C1 revisits.

### Lead rulings on E1–E3 (recorded decisions, team-lead 2026-10-22)
- **E1 — option (A) APPROVED.** The story file wins over the prompt (wire-freeze clause): story R-6 / line 43 say only "the new source's timestamps re-based onto the current playout timeline"; "inside the ciphertext" appears only in the prompt and the ADR §2 diagram. Receiver arrival timestamps (injected clock), one monotonic playout timeline per slot, re-based on a switch. Option (B) (payload-format change) REFUSED. ADR §2's in-ciphertext PTS has no producer — an escaped ADR detail recorded as a `docs/TODO.md` entry (owners protocol + client; task-sized: a payload-format change across publish, receive, and the ADR, with its own vectors question). ADR not edited. The switch test still uses synthetic timestamps.
- **E2 — APPROVED.** A frame on an undeclared/non-video slot id aborts the stream, counts in the identity as `stream_slot_mismatch`, and bumps `undeclared_stream_id_total{media_kind="video"}`. Mapping documented in the catalog row and the ingress note.
- **E3 — APPROVED.** `DEFAULT_RECEIVE_VIDEO_SLOTS` stays 0; rationale comment updated.

### Security Gate-1 changes folded (S1, S2)
- **S1:** the loss-event rule is "send iff `(sender, gop)` ≠ the slot's LAST reported pair" — NO monotonic clause (an unverified huge counter must not suppress later genuine requests; reissued sender ids restart at 0). Test: a bad-signature frame claiming (V, huge) then a genuine V failure at a lower counter still sends.
- **S2:** the mute guard's sender→participant lookup is the INVERSE of `RosterKeyFeed`'s own index (`participantIdOf(senderId)`), maintained by the same `joined()` entry that feeds the identity key — so a verified peer sender is mapped by construction. Fail closed anyway: a verified sender with no mapping is NOT drawn (closed, counted on the post-accept `decode_queue_dropped_total{media_kind="video"}`, never on `sender_server_muted`, so A5 is not fed by a lookup gap). Self is caught first by `self_sender`.
- Pre-decode VP8 check: keyframes only, bounds-checked against the plaintext length; a short/malformed keyframe header is a typed post-accept discard (`decode_queue_dropped`, slot stays awaiting-keyframe), never an exception or out-of-bounds read.
- `VideoSink` interface doc: borrow only; the embedder must clone to retain.

### Reused vs new (DRY)
| Need | Reused | New |
|---|---|---|
| v3 prefix parse + MAX check before alloc | `frameCodec.ts` `parsePrefix`/`parsePayloadLength`, exported via a new `peekFrameExtent(bytes)` (steps 1–11 on a prefix; returns `need-more` or `{totalLength}`; throws `codecReject`) and `MAX_FRAME_BYTES` derived from `WIRE_CONSTANTS` | `pipeline/streamReader.ts`: chunk accumulation only. No offsets or u32 reads restated. Whole frame → `decodeFrame` |
| policy | `verifyFrame`, `openVerifiedFrame`, `ReplayWindow`, `TransmitKeyCache`, `ReceiverKeys`, `IdentityKeyResolver`, `assignmentReject`/`keyReject` | `pipeline/receivePolicy.ts:receiveByKind()`, ONE function that audio `IngressPipeline.accept` AND video call; the resolve/verify/gate/open body moves out of `ingress.ts` into it |
| accounting | `#framesReceived/#Accepted/#Dropped/#lastDropReason` + `MediaKindMetrics.frame*` | extracted into `pipeline/frameAccounting.ts` (one-increment-two-readers kept), used by both ingresses |
| evidence L1/L2/drops | `ReceiveVerificationRecorder` (slot = relay slot id, same key space) | — |
| decoder seam / fault isolation | `AudioDecoderSeam` shape, `Lane`'s instance counter + `decoderRestartBackoffMs` + `FaultOnce` | `VideoDecoderSeam`/`VideoDecoderFactory` in `seams.ts`; `setup/vp8Decoder.ts` (browser-only, coverage-excluded like `vp8Encoder.ts`) |
| tokens | `RejectReason`/`ALL_REJECT_REASONS` only | `VIDEO_POST_DECODE_REJECT_REASONS` const (subset, `satisfies`) in `rejectReason.ts` |
| recovery reason vocab | — | `MEDIA_RECOVERY_REASONS` closed const in `mediaMetrics.ts` beside the handle |
| config | `positiveInteger`/`ClientConfigError`, `receive.videoSlots` (M is not re-derived) | `VideoReceiveConfig` + `videoReceiveConfigLogLine()`, a sibling of `videoConfigLogLine` |
| post-accept loss | `decodeQueueDropped()` (video kind) | — |
| interval signals | `startIntervalSignals` timer pattern | distinct-active-sources sample (video) |
| hop monitor | **deliberately NOT used for video.** A downlink group stream's loss shows up as a stream RESET (R-12, counted `group_abandoned`), and reliable ordered streams have no reorder. MH-reset groups would read as hop gaps and double-count MH abandonment as network loss | — |
| GoP compare | not `keyframePolicy.ts`, which is the publisher's request-acceptance logic. Receive dedupe is a single last-pair compare | — |

### (1) Incremental uni-stream reader (R-21) — `pipeline/streamReader.ts`, `lifecycle/videoStreamTransports.ts`
- `MediaTransport.incomingUniStreamsFor(url)` is a selector over connected transports, never a dialer, the same rule as `getDatagramChannel`. One acceptor loop per handler transport, started at most once (the `receiveTransports.ts` pattern).
- **Concurrency cap**: a GLOBAL count of open incoming streams across handlers (memory bound). A stream arriving at the cap is `cancel()`ed at once and counted `dt_client_media_incoming_streams_refused_total{media_kind="video"}`. It is not queued and not parsed (security 6). It is outside the frame identity, because no frame was parsed.
- Per stream, `UniStreamFrameReader` holds at most the header-peek bytes (`≤` max header, a fixed small buffer) until `peekFrameExtent` returns `totalLength`. Only then does it allocate ONE `Uint8Array(totalLength)` and copy chunks into it. There is no concat growth. `payload_length > MAX` throws from the peek before any allocation. **Allocator seam**: the reader takes `alloc: (n) => Uint8Array` (default `new Uint8Array`). The test spies it and asserts it is never called for MAX+1 / u32::MAX (test 2).
- **Counting point = the parse boundary**: a frame is RECEIVED when its first byte arrives at the reader (`frameReceived()` once). Every received frame ends exactly once as accepted or one drop reason:
  - a prefix codec error (`unknown_version`, `reserved_flag_bit_set`, `key_bearing_discardable`, `payload_length_exceeds_max`, `extensions_too_large`) drops that frame and aborts the stream (framing is lost);
  - a whole-frame `decodeFrame` error drops the frame and the stream continues (the extent was valid);
  - a reset or FIN mid-frame gives `stream_partial_reset` for the partial frame, once (a FIN mid-frame is also a partial frame; same token);
  - a relay `stream_id` that differs from the stream's first frame drops that frame as `stream_slot_mismatch` and aborts the stream (remaining bytes unread, nothing more counted);
  - a first-frame `stream_id` that is not a declared VIDEO slot follows E2.
- Slot binding = the FIRST frame's relay `stream_id`. **Security note: for video the relay id SELECTS the slot pipeline.** That is by design (MH owns the per-subscriber slot mapping; media kind comes from transport+slot). It never feeds attribution: the render tag and pairing use only the verified key-id sender. `ingress.ts`'s quarantine comment gets a video paragraph saying exactly that.
- Per stream it keeps `lastHeader: {senderId, gopCounter} | undefined`, set once a frame's prefix AND its 8 key-id bytes (the payload start) are buffered (pure parse via `unpackKeyId(parseSframe…)` on the buffered slice; any parse failure leaves it unset). This is used only for the reset recovery request (R3: no sender → nothing).
- **Reader corpus** (`__tests__/streamReader.corpus.test.ts`), generated from every valid row of `frame-v3.vectors.json`:
  - (a) truncation at every byte offset, then EOF and then reset;
  - (b) `payload_length` ∈ {0, MAX, MAX+1, 0xFFFFFFFF} written into valid rows;
  - (c) every single-bit flip over the fixed prefix + wrapped-key + ext_length bytes;
  - (d) a valid multi-frame stream, split at every 1-byte boundary plus 64 splits from a fixed-seed PRNG (the seed is in the test name).
  - Every case asserts outcome ∈ {accepted, a `RejectReason` member, `incomplete-at-eof`, `incomplete-at-reset`}, with no untyped throw. Positive control: case count > 0 and ≥1 accepted.

### (2) ONE receive policy (R-5) — `pipeline/receivePolicy.ts`
`receiveByKind(decoded, keyIdSender, gate, ctx)`, where `gate` is `{kind:'audio', laneFor}` | `{kind:'video'}`. The kind comes from the CALLER's transport (datagram ingress passes audio, the uni reader passes video). It is never read from the frame (security 10).
- Shared: resolve the identity key from `key_id.sender_id` only. If there is none, `no_roster_entry` (now thrown via `keyReject` so both ingresses use one catch). Then `verifyFrame`.
- Audio: `laneFor(sender)` or `sender_not_assigned` BEFORE open (unchanged), then `openVerifiedFrame`.
- Video: `openVerifiedFrame` (replay inside, per (KEK gen, sender, key-id stream)), in ANY slot state. This is the decode-before-live exception.
- Render-side guards, evaluated in `renderDecision.ts` on the DECODED frame's verified sender each time a frame would be drawn (initial render, avatar→ACTIVE hand-off, every new output):
  - the sender is this client's `senderId` (always defined here, since `startMedia` refuses without one, so no 0/undefined compare: security 8): `self_sender`;
  - the sender is video-server-muted per the latest `ParticipantMuteUpdate`: `sender_server_muted`.
  - In both cases the frame is closed, never drawn, and counted on `frames_dropped_total{media_kind="video"}`.
  - A5 counts only these, never avatar-slot decode-before-live (a muted-sender frame in ANY slot state is the MH-failure signal; avatar_client_muted / no_video frames are never counted). The muted-sender lookup is a participant→sender map maintained in `SignalingMediaState` from the roster (joined/participantJoined/Left), keyed by verified sender.
- **Identity at the parse boundary (both kinds):** `received = accepted + Σ drops{reason ∉ VIDEO_POST_DECODE_REJECT_REASONS}`. "Accepted" = verified + decrypted + handed to the slot pipeline. `decode_over_bound`, `self_sender` and `sender_server_muted` are POST-DECODE drops on the same counter. That is the only place they can occur, because they judge decoded output or the render moment. The catalog states the identity with that subtraction (observability to confirm). Other post-accept losses (jitter overflow, awaiting-keyframe discards, superseded-source trailing frames) go on `dt_client_media_decode_queue_dropped_total{media_kind="video"}` (existing post-accept counter, catalog text widened).
- Branch table test: a table of rows covering audio unassigned, both-kinds unrostered, bad signature, replay, video in each non-ACTIVE state (decoded, not rendered), video ACTIVE (rendered), self, server-muted, and kind-from-transport (the same frame bytes on the datagram vs the uni path route to the lane vs the pipeline). A completeness assertion checks that the set of reasons/outcomes the rows produce equals the declared policy outcome set, so a new branch without a row fails.

### (3) Exactly M slot pipelines (R-6) — `pipeline/videoSlotPipeline.ts`
- One pipeline per declared VIDEO slot id, built EAGERLY at receiver start, with one decoder created at construction. M is fixed for the session, so the count is bounded by M and never by sender count. Test: 1/4/10 senders → exactly M constructions.
- The decoder is configured `{codec:'vp8', codedWidth: maxWidth, codedHeight: maxHeight, optimizeForLatency: true}` from `videoReceive` bounds. There is never a `description` or bitstream/extension value. Native WebCodecs only; no new dependency.
- **Jitter buffer** = a bounded FIFO (`jitterMaxFrames`, `jitterMaxBytes`) of opened frames tagged `{sender, gop, isKeyframe, bytes, timelineUs}` (`isKeyframe` = the signed `independentlyDecodable` flag; no key material is ever queued — semantic-guard Q1), fed to the decoder in order while `decodeQueueSize < decoderQueueTarget`. Overflow evicts the incoming frame's group, which gives `local_drop` + a loss event. **Max hold**: a frame older than `jitterMaxHoldMs` (injected clock, checked on every push and output — no timer) is discarded with its group → `local_drop` (security 4: nothing sender-chosen can stall a slot). There is no playout pacing (target delay 0). Catch-up is future work per the story.
- **Group gate (security 3)**, keyed only on the SIGNED header `independentlyDecodable` flag + `gop_counter` and the VERIFIED key-id sender:
  - the slot's current decode group is `(sender, gop)` of the last keyframe enqueued;
  - a non-key frame is admitted only if it belongs to that group;
  - a keyframe of a newer group of the same sender, or any keyframe of another sender, starts a new group;
  - for a different sender, that is a **source switch**: no new decoder, no flush. Frames already queued ahead play out first. Trailing old-source frames that arrive AFTER the switch keyframe are discarded (post-accept), never fed on B's reference.
  - Non-key frames from a sender that is not the current group, or while awaiting a keyframe after a loss, are discarded (post-accept) with no request.
- **Re-basing (E1-A)**: `timelineUs = slotTimelineOffset + arrivalUs`. On a switch, `offset = lastTimelineUs + nominalFrameIntervalUs − newSourceFirstArrivalUs`, so chunk timestamps stay monotonic and the new source keeps its relative spacing. This is asserted with synthetic clock values.
- **Render attribution through the async decoder (security 2)**: a FIFO of `{sender, gop, timelineUs}` is pushed at `decode()` and popped at output, matched by timestamp. It is never "the slot's current source".
- Output bounds: `codedWidth/Height > max` → `decode_over_bound`. Frame rate: a ring of the last `maxFrameRateHz` output times; an output whose ring-oldest is <1000 ms old → `decode_over_bound`. The pre-decode VP8 keyframe-header dimension check (security 5, optional) is **adopted**. It is a 10-byte read of the VP8 key-frame header (RFC 6386 §9.1) in decrypted plaintext, before the native decoder sees it: over-bound → `decode_over_bound`. It is pre-decode, but on the same post-accept list.
- **Fault isolation**: a decoder error closes only this slot's decoder, marks the slot awaiting-keyframe, and produces a loss event (`decoder_error`, fields from the chunk at the head of the in-flight FIFO). Recreation happens lazily on the next admitted keyframe, at most once per `decoderRestartBackoffMs` (the existing knob, same meaning). That bounds churn (ops 4). Other slots, and audio (separate pipeline), are untouched. Test: A errors; B and audio delivered counts keep rising.
- Test seam: the injected `VideoDecoderFactory`, which counts constructions. It is not on the e2e bus.

### (4) Render decision + VideoSink (R-4, R-5) — `pipeline/renderDecision.ts` (pure) + slot pipeline
- MC state per declared video slot comes from `StreamAssignments` (`mediaKind==='video'`). `RenderState` is one of:
  - `{kind:'active', senderId}`;
  - `{kind:'avatar', participantSenderId, reason: 'no_video'|'client_muted'|'server_muted'}`;
  - `{kind:'fewer_sources'}`;
  - `{kind:'unavailable'}`;
  - `{kind:'none'}`, which is fail-closed for `unspecified`/`switch_pending`/`withheld_by_congestion`/`source_unreachable`/no assignment yet. No media is drawn.
- Each slot holds its LATEST decoded frame that passed the bounds, tagged with its verified sender.
  - ACTIVE + an eligible held frame → draw.
  - ACTIVE with no eligible frame → keep the previous render state (no spinner).
  - ACTIVE→avatar → switch at once. The held frame is closed, and the trailing group `(sender, gop)` is recorded. Later decodes from that sender at gop ≤ recorded are not eligible, so trailing frames are ignored; a resume keyframe (newer gop) is eligible.
  - avatar→ACTIVE → the avatar is kept until an eligible frame exists. A frame decoded while in avatar state (newer group) renders AT the transition.
  - The self/muted guards run on every draw candidate, including the held one at the transition (security 9).
- `VideoSink { drawFrame(frame: VideoFrame): void }` is attached per slot by the embedder: `MeetingSession.setVideoSink(slotId, sink | undefined)`. Its contract is BORROW: the sink must not retain the frame past the call (clone if needed). The SDK owns close: a frame is closed exactly once, on replacement, on a guard drop, on an over-bound drop, on ACTIVE→avatar, or at teardown. Frame delivery never goes through events or stores. Only `RenderState` changes are emitted (`videoSlotRenderChanged {slotId, state}` on `MeetingSessionEventMap`), on transitions only.
- Pairing: `pairSlotsBySender(assignments, liveVideo: ReadonlyMap<slotId, verifiedSender>)`. An ACTIVE video slot pairs by the verified sender of its last RENDERED frame. `MeetingSession.slotPairing` reads both.

### (5) Recovery requests (R-13) — loss tracking in the slot pipeline, send queue in `lifecycle/recoveryRequestQueue.ts`
- A loss event happens when a frame on this slot fails at `(sender, gop)` taken from the FAILING frame's header. A request is sent iff that pair differs from the slot's LAST reported pair (no monotonic clause — security S1). State is exactly one pair per slot (security R2, O(1)). Code comment: "bounded by groups for honest input, by frames received under hostile input (amplification ≤ 1); ≤ 1 queued per slot".
- Mapping table (security 1):

| Outcome | Request | reason |
|---|---|---|
| `signature_invalid`, whole-frame codec rejects after the extent (`extensions_malformed`, `trailing_bytes`, `payload_length_exceeds_available`, `truncated`) with a parseable key-id sender | yes | `verify_failed` |
| `no_roster_entry` | **yes** | `verify_failed` |
| `decrypt_failed`, `unwrap_failed`, `no_transmit_key`, `no_kek_for_generation` | **yes** | `decrypt_failed` |
| decoder error | yes | `decoder_error` |
| jitter overflow / max-hold discard | yes | `local_drop` |
| stream RESET with a parsed header (`lastHeader`) | yes | `group_abandoned` |
| `replay_detected`, `kek_generation_stale`, `self_sender`, `sender_server_muted`, `decode_over_bound`, `stream_slot_mismatch`/undeclared, prefix codec errors (no key id), reset without a header, FIN, awaiting-keyframe/superseded discards | no | — |

  `no_roster_entry`/`no_transmit_key`/`no_kek_for_generation` are YES because roster/KEK/key-bearing lag on a new source's keyframe would otherwise leave the slot undecodable with no keyframe interval to save it. They are bounded by R2 like everything else. `kek_generation_stale` is a stale tail, so it sends nothing.
- The target is the handler whose transport delivered the failing frame/stream. One request per loss event, no timer, no retry. The queue holds ≤1 unsent request per (handler, slot) with replace-on-newer, and calls `MediaTransport.sendRecoveryRequest` only when no send is in flight for that handler, so nothing obsolete waits in C-A1's `tail` chain.
- `dt_client_media_recovery_requests_sent_total{media_kind="video", reason}` is counted on a successful write. A failed write raises an event-once transport fault and is not re-sent. A replaced request is not counted. The 5 reasons are zero-initialised when the video receiver starts.
- Fields carry `slotId`, `senderId` and `gopCounter` only (R1/R4). The header values never reach the pairing view, the render tag, hooks, labels or logs.

### (6) Keys and replay (R-14)
- The video key id carries stream = MC's video send id (1) vs audio's 0 (C-A2 `TransmitKeyManager`), so the (scope, sender, stream) contexts and high-water do not collide (security 7).
- The video receiver has its OWN `TransmitKeyCache` + `ReplayWindow` (from `receiverState` config). Wiring points:
  - its own `kekSource.onGenerationDropped` listener (`discardGeneration` both), registered in its teardown;
  - `clear()` both at teardown;
  - `rosterInvalidationListener` purges audio AND video keys (keys only; replay retained). `TransmitKeyPurger` becomes a list, wired in `MeetingSession.#createSignaling` → `mediaStart`.
- Tests:
  - a replayed WHOLE earlier group (keyframe + deltas, transmit-key gen k) after group k+1 → every frame `replay_detected` via the high-water, never rendered;
  - a leaver holding only KEK g cannot open frames under g+1 (`no_kek_for_generation`), with a positive control: the same sender's frame under g is accepted;
  - a non-key-bearing frame with an unknown key id → `no_transmit_key` (decode reject, plus recovery per the table);
  - a rebind purge drops video keys and keeps replay.

### (7) Configuration (R-23) — `clientConfig.ts` `media.videoReceive`
| knob | default | floor / rule |
|---|---|---|
| `maxIncomingStreams` | 16 | ≥ `2 × receive.videoSlots + VIDEO_STREAM_CAP_HEADROOM` (named const = 2) |
| `readerMemoryCeilingBytes` | 32 MiB | `maxIncomingStreams × MAX_FRAME_BYTES ≤ ceiling`, else **throw** naming both values and the product |
| `jitterMaxFrames` / `jitterMaxBytes` / `jitterMaxHoldMs` | 60 / 2 MiB / 2000 | positive integers; `jitterMaxBytes ≥ MAX_FRAME_BYTES` (one maximal frame must fit, else a legal frame can never be buffered) |
| `decodeMaxWidth` / `decodeMaxHeight` / `decodeMaxFrameRateHz` | 1280 / 720 / 60 | positive; ≥ `video.capabilityProbe` geometry (a production profile must fit) |
| `decoderQueueTarget` | 3 | positive |

Validated in `validateMediaConfig`, so a bad config refuses to start the pipeline. Defaults pass at M = 0…7 (test at M=3, dev-web's value, and M=7, the max that the default cap allows). `videoReceiveConfigLogLine(media, source)` is logged once at receiver start, a sibling of `videoConfigLogLine`, with numbers and provenance only. Code-config, outside the ConfigMap and env-config guard.

### Telemetry (per-kind handle `metrics.video`; catalog row + collector allowlist + basic dashboard panel each)
- Reused on `media_kind="video"`: `frames_received/accepted/dropped_total`, `decoder_errors_total`, `decode_queue_dropped_total`, `undeclared_stream_id_total`, `key_wrap_outcomes_total`.
- NEW (names for observability to confirm):
  - `dt_client_media_recovery_requests_sent_total{source}` (label renamed from `reason` by the taxonomy ruling below);
  - `dt_client_media_incoming_streams_refused_total`;
  - `dt_client_media_slot_unfulfilled_duration_ms` (histogram, buckets past 100 s);
  - `dt_client_media_slots_unfulfilled` (gauge, panel only);
  - `dt_client_media_active_source_intervals_total` (COUNTER per O1 below);
  - `dt_client_media_time_to_first_render_ms` (media start → first rendered frame, once per session);
  - `dt_client_media_slot_active_to_first_render_ms` (each ACTIVE entry → its first render; an entry that exits unrendered is not observed, because it is covered by the unfulfilled histogram).
  - The latency histograms are open objectives with buckets extended to 30 s.
- **Unfulfilled** (`lifecycle/unfulfilledSlots.ts`, pure, injected clock):
  - entry: ACTIVE with no eligible rendered frame from the current source (at ACTIVE entry before the first draw, or on a loss event while ACTIVE);
  - exits, each one observing the histogram once: next rendered frame, MC leaves ACTIVE, the slot is omitted from a snapshot, receiver stop (leave/disconnect/dispose all route to it);
  - the gauge is set to the ABSOLUTE count on every transition, re-set on the export tick (DELTA last-value rule, as with `capture_source`; observability to confirm vs "transitions only"), and set to 0 at teardown;
  - no labels beyond the base + `media_kind`;
  - the local `unfulfilledSnapshot()` gives per-slot `{slotId, unfulfilled, sinceMs}`; it is for C-C2's overlay and is never exported.
- **Bucket boundaries**: named consts in `mediaMetrics.ts` (`SLOT_UNFULFILLED_BUCKETS_MS` up to 300 000; `FIRST_RENDER_BUCKETS_MS` up to 30 000). Mechanism is pending observability's choice: (i) an optional `boundaries` param on `MetricsSink.histogram` passed as OTel `advice.explicitBucketBoundaries` (touches the sink contract + test-utils parity copy), or (ii) the sink constructor takes a name→boundaries registry. I lean (i). Tests read the const and the InMemory sink's recorded boundaries.
- Per-frame code in `pipeline/` has no names and no logging, and handles are pre-bound. `hotPathLayout.test.ts` positive control adds the new pipeline files.

### Evidence hooks (R-32), exposed from the SDK, wired to the bus by C-C1
- `MeetingSession.videoReceiveSnapshot()` → per declared video slot: `{slotId, mcState: SlotStateToken, render: RenderState['kind'], lastRenderedSender: number | undefined, unfulfilled: boolean}` (NO duration — `unfulfilledForMs`/time-in-state lives only in the local `unfulfilledSnapshot()` for C-C2, per the security ruling below).
  - **Allowed ids**: `slotId` (the key) and `lastRenderedSender` (the VERIFIED sender, by design). Nothing else is an id.
  - A test scans the shape (keys + value types) so no other field can carry an id or a string beyond the closed tokens.
- Layer-2 per (slot, sender) counters: the existing injected `ReceiveVerificationRecorder`, now also fed by video ingress (L1 keyed, L2 verified, drops by reason, post-decode drops included).

### Tests (node tier, `MockWebTransport` uni streams, injected clock, no wall-time waits; `FakeVideoDecoder` + `FakeVideoFrame` with a close counter in test-utils)
- An `expectIdentity(counts)` helper is asserted after EVERY drop-path test.
- Coverage of test asks 1–10:
  - source switch: zero new decoders, no flush, old plays out, re-based timestamps;
  - R-5 both orders + ACTIVE→avatar trailing;
  - fault isolation (slot B + audio counts rise);
  - exactly M;
  - recovery: exact fields from the failing header incl. just-after-switch, once per pair, a decoder-error loop sends one, queued-replace, no timer (advance the clock and scheduler → zero sends), nothing for a silent new source / headerless reset;
  - unfulfilled: every entry/exit incl. teardown, gauge → 0, a bucket > 100 000, label set exactly the base + `media_kind`;
  - reader corpus + allocator spy;
  - self/muted never drawn + counted;
  - leaver + replayed whole group with positive controls;
  - every `VideoFrame` closed exactly once across render, drop, over-bound, avatar switch, teardown;
  - config floors / ceiling refusal / log line;
  - hot-path layout.


### Code-quality + DRY Gate-1 items folded
- **CQ1 / ONE policy:** `RECEIVE_POLICY_OUTCOMES` is ONE closed const covering `receiveByKind` outcomes AND the render-guard outcomes (`self_sender`, `sender_server_muted`, drawn, held); the comment at the split names it; the branch-table completeness check is against it.
- **CQ2 / dedupe (post-S1):** send iff `(sender, gop)` ≠ the slot's last reported pair. A different sender always sends (new source). A→B→A with A's counter lower than an earlier A report: SENDS (one pair of state; the earlier A counter is forgotten by design — MH dedupes per counter; amplification ≤ 1). Table row + test.
- **CQ3:** every deadline (frame-rate ring, `jitterMaxHoldMs`, backoff, unfulfilled, latency) reads the injected `clock` only; no `performance.now()`/`Date.now()` in `pipeline/` (the only `Date.now` is the existing production default at the option boundary).
- **CQ4:** the `stream_partial_reset` catalog row says it covers FIN-truncated frames too.
- **CQ5:** `receiveByKind(keyed, gate, ctx)` takes ONE parsed struct `KeyedFrame {decoded, keyIdParts}` built once by the caller (it is needed earlier for the recorder's layer-1 key and the hop observation), so the two cannot disagree.
- **CQ6:** the TODO entry quotes ADR §2's "presentation timestamp inside the ciphertext" line; the re-base site carries a code comment citing it and the lead's E1 ruling.
- **P1:** no second index. The guard composes `SignalingMediaState`'s per-participant server-mute state with `RosterKeyFeed`'s ONE participant↔sender map: a `participantIdOf(senderId)` reverse query on the same map (no second structure), so it follows rebind/leave. Unmapped verified sender → not drawn (S2). `senderIdOf`'s "presentation only" comment updated (a drop guard on the VERIFIED sender, never attribution) — wording to security.
- **P2:** decoder config via `setup/vp8.ts:buildVp8DecoderConfig(vp8 profile from videoReceive bounds)`, extended with `optimizeForLatency`; the RFC 6386 keyframe-header dimension read lives in `setup/vp8.ts` (pure function) — VP8 bitstream knowledge has one home.
- **P3:** render mapping is an exhaustive `Record<SlotStateToken, RenderRule>`; avatar reason is `Extract<SlotStateToken, \`avatar_${string}\`>` (the token itself), so a new wire state is a compile error.
- **P4:** `ReceiveTransports` is generalised: the per-url started-once registry, selector, not-connected / locked faults, disposer registration, stopped-aware loop and defect catch become ONE `PerHandlerLoops<T>` (in `receiveTransports.ts`), parametrised by `readableFor` and a loop body; the datagram loop and the uni-stream acceptor are the two bodies. No second `open()`.
- **P5:** separate per-kind cache/replay instances kept (DRY ruling: not duplication); the generation-dropped listener + teardown clear become ONE helper `bindReceiverState({cache, replay}, kekSource, register)` in `lifecycle/` used by AudioPipeline AND VideoReceiver (the "registered exactly once, synchronous, not re-entrant" invariant lives there).
- **P6:** `nominalFrameIntervalUs = 1e6 / videoReceive.decodeMaxFrameRateHz` (existing knob; it only needs to be > 0 and ≤ a real frame spacing for monotonicity).

### Observability Gate-1 rulings folded (a, b, O1, O2, d)
- **(a) Buckets:** optional `boundaries` param on `MetricsSink.histogram` → OTel `advice.explicitBucketBoundaries`; instruments are cached by name so boundaries bind at FIRST creation — a later call with different boundaries THROWS in dev/test mode, warns and keeps the first in prod. Consts `SLOT_UNFULFILLED_BUCKETS_MS` (→300 000) and `FIRST_RENDER_BUCKETS_MS` (→30 000) in `mediaMetrics.ts` are the SSoT; catalog rows cite the const name and note a boundary change splits `le` series across client versions.
- **(b) Gauge cadence:** absolute count on every transition, re-set on the interval tick, 0 at teardown, timer stopped at teardown. `dt_client_media_slots_unfulfilled` row carries the SINGLE-WRITER-MEANINGFUL block (as `send_queue_depth`): not a fleet count, panel only; the fleet signal is the histogram (A9 is OBS-B's call); the histogram observes on EXIT only, so a slot stuck all session first appears at teardown.
- **O1 (observability's metric-shape call, story-2 precedent `dt_client_media_receive_source_deficit_total` — a gauge across N identical-identity browsers is last-writer-wins):** distinct active sources ships as COUNTER `dt_client_media_active_source_intervals_total` (unit source-intervals, video): each interval tick adds the number of distinct verified senders with ≥1 decoded output in that interval; `add(0)` at receiver start. Fed from decode activity, never slot state.
- **O2:** `self_sender` and `sender_server_muted` are decided at ARRIVAL inside `receiveByKind` — after verify, before open: pre-accept drops (never decoded, never rendered, no recovery request). `sender_server_muted` then means "a frame from a sender ALREADY video-server-muted when it arrived" — the precise MH-failure signal A5 needs. The render-time guard stays as a NEVER-RENDER check on held frames; a held frame it rejects is closed and counted post-accept on `decode_queue_dropped_total{media_kind="video"}`. The identity's exclusion set shrinks to `VIDEO_POST_DECODE_REJECT_REASONS = ['decode_over_bound']` (incl. the pre-decode VP8 header check, which runs after accept): `received = accepted + Σ drops{reason ≠ decode_over_bound}`; the catalog's identity section states that one exception and why the token cannot move. **Supersedes** the earlier "render-side post-decode" placement of these two guards in §(2). **Liveness repair (observability ruling; owner extension caused by O2):** each slot stores the header `(sender, gop)` of its LAST `sender_server_muted` arrival drop (one pair, from a VERIFIED frame). On that sender's video server-mute→unmute EDGE (`ParticipantMuteUpdate`) the slot raises ONE loss event through the normal path, reason **`server_mute_lifted`** (a SIXTH closed value in `MEDIA_RECOVERY_REASONS`, zero-initialised with the others — **OBS-B (task 20): the vocabulary has 6 values, not the 5 in the task text**), same last-pair dedupe, no timer, no retry. The stored pair is cleared when used, when the slot's source changes, and at teardown. Recovery table row for `sender_server_muted`: "no request at drop; one at the unmute edge". Tests: resume keyframe dropped → unmute edge → exactly one request with that frame's fields + `server_mute_lifted`; repeated edge / second drop at the same pair → nothing; unmute with no recorded drop → nothing. Security concurred, adding: fire only if the slot's current decode group is NOT already a newer verified group from that sender (a resume keyframe that arrived after the unmute and decoded → nothing); tests: a source change between the drop and the unmute → nothing; a newer group from that sender already decoded → nothing.
- **A5 calibration (catalog):** the `sender_server_muted` reason text states the residual race — a `ParticipantMuteUpdate` can reach the client before MH applies DROP_AT_INGRESS, so a short burst per ordinary server mute is expected; a SUSTAINED rate is the MH-enforcement signal; pointer that OBS-B's A5 threshold must tolerate the burst.
- **Label key ruling (taxonomy owner, during implementation): the recovery counter's cause label is `source`, NOT the task text's `reason`.** `dt_client_media_recovery_requests_sent_total{media_kind="video", source=verify_failed|decrypt_failed|decoder_error|local_drop|group_abandoned|server_mute_lifted}`; const `MEDIA_RECOVERY_SOURCES`. Why: `reason` is the frame-drop label space ("must not acquire a third home", `label-taxonomy.md`), and `decrypt_failed` here is a SUPERSET of the codec token of that name, so `{reason="decrypt_failed"}` would silently mix frame drops with request causes; `source` is the client's "what caused this event" key (as `keyframes_produced_total{source}`) and is already forwarded/kept. **Handoff to OBS-B (task 20) / T-B (task 21): select on `source`, and the vocabulary has 6 values.**
- **(c)/(d) rows:** `decode_queue_dropped_total` text widened (jitter overflow/max-hold, awaiting-keyframe, superseded trailing, render-guard rejects, unmatched decoder output); `stream_slot_mismatch` counts the one offending frame (unread bytes not counted; also covers undeclared/non-video slot ids per E2); `stream_partial_reset` counts the partial frame once (received at first byte; includes FIN-truncated); `incoming_streams_refused_total` outside the frame identity (unit: streams); `recovery_requests_sent_total` counted on successful write — replaced requests and failed writes not counted (a failed write is an event-once transport `mediaFault`); `time_to_first_render_ms` cross-refs `dt_client_time_to_first_media_frame_ms` (wire vs render boundary — the "arriving but not showing" read); `slot_active_to_first_render_ms` states survivorship bias; both "open objective, observed never gated". All `Exported: yes` + collector allowlist + basic panel; the "23 OF 28" banner rewritten without a hand-maintained count.

### Security ruling — evidence hooks vs test-bus identity (verbatim)
"Security ruling — R-32 evidence vs the R-30/§11 test-bus identity deny. The story says two things that conflict. R-30 says no sender/stream identity on the test bus. R-32 says per-slot attribution is asserted on the test bus per receiver per sender. R-32 cannot be met without naming the verified sender per slot, and the harm §11 guards against is the time-ordered per-stream record (the voice-activity trace): GoP values, frame sizes, keyframe timings and per-frame events. A slot→sender mapping is not that record. MC's slotAssignments already puts the same mapping on the bus (story 2 precedent, `packages/web-app/src/lib/e2eBus.ts`). The R-32 evidence hooks may therefore expose exactly these identity values: `slotId`, the VERIFIED key-id `lastRenderedSender`, and per-(slot, sender) layer-2 counters (cumulative counts by closed reason token). Everything else is closed tokens and booleans. Nothing else may appear: no GoP counter (ADR-0036 §2 says explicitly 'never a test-bus value'), no stream number, no frame sizes, no timestamps, durations or time-in-state (so `unfulfilledForMs` stays in the local overlay snapshot `unfulfilledSnapshot()` and is NOT on the evidence hook; the bus carries the `unfulfilled` boolean only), no hop sequences, and no per-frame events. The hooks are SDK getters. They are never metric labels, log fields or span attributes. They reach the bus only through C-C1's opt-in test define, with the production bundle-absence proof (`packages/web-app/tests/bundle-content.test.ts`), exactly like story 2's ReceiveVerificationRecorder. The R-30 sentence should be read as excluding the R-32 attribution evidence; the lead records the wording reconciliation for /close-story."

Security also concurred with O2 (self/muted pre-accept, verified sender, latest `ParticipantMuteUpdate` at check time; branch-table rows move with them).

### Test Gate-1 items folded (T1–T12, all adopted)
T1 completeness set is an exported `RECEIVE_POLICY_OUTCOMES` const the policy's throw/return type is checked against with `satisfies`; T2 the recovery mapping is a code const (`RECOVERY_REASON_FOR`) tested by iterating EVERY `ALL_REJECT_REASONS` member + decoder_error/local_drop/reset±header; T3 allocator positive control (called once, n == totalLength); T4 lastHeader boundary either side of key-id completion (incl. mid-frame FIN: same token, no request); T5 `FakeVideoDecoder` is test-driven (`emitOutput`/`emitError`), plus an unmatched/out-of-order output test that fails closed (not drawn, closed, counted on `decode_queue_dropped`); T6 a FakeVideoFrame registry asserted in afterEach (closeCount === 1), incl. a throwing sink; T7 exact clock edges (hold, hold+1; 1000 ms ring edge; backoff inside/at); T8 unfulfilled idempotence (two losses → one entry), ACTIVE across a source switch with no loss → NO entry (the new source's keyframe renders as it arrives; a client cannot see a switch before that), multi-slot teardown → gauge 0 + N observations, exact durations; T9 leaver and rebind purge driven through the real receiver wiring (generation-drop listener, purge list; replay high-water retained after purge); T10 exact config boundaries + log once; T11 any edit to an existing test file is listed in main.md with its reason, no assertion removed or loosened; T12 `expectIdentity` reads the reason-labelled counter excluding exactly `VIDEO_POST_DECODE_REJECT_REASONS`, one test hits `decode_over_bound` (excluded) AND `self_sender` + `sender_server_muted` (now INSIDE the identity as pre-accept drops, post-O2) and asserts the identity holds; one audio datagram call. Switch test asserts exact `timelineUs` values.

### Operations notes folded
Cap-floor refusal names `maxIncomingStreams`, M and the floor; `readerMemoryCeilingBytes` doc comment states the per-slot jitter dimension (M × `jitterMaxBytes`) is OUTSIDE it.

### Ops answers
- Blast radius: per-slot only (above).
- Runbook: `client-dev-local.md` gains rows for the receive-config refusal (error names both values) and for a non-zero `stream_slot_mismatch` / `incoming_streams_refused` (MH-side defect pointer).
- Rollback: the commit is revertible on its own. It adds only a receive consumer, and C-A2's publish path is independent. Reverting leaves video receive-less, as before.


### Gate 1 — Plan Confirmations (Lead)

| Reviewer | Plan Status |
|----------|-------------|
| Security (paired) | confirmed — incl. explicit ruling: recovery_request fields from the failing (possibly unverified) header ACCEPTED, conditions R1–R4 |
| Test (paired) | confirmed (T1–T12 folded) |
| Observability | confirmed |
| Code Quality | confirmed (CQ1–CQ6 folded) |
| DRY | confirmed (P1–P6 folded) |
| Operations | confirmed |
| Semantic Guard | confirmed |

Classification-sanity guard: `STATUS=OK` (no GSA paths).

### Lead rulings (Gate 1)

- **E1** — option (A) arrival-time timestamps from the injected clock, re-based per slot playout timeline. Story R-6 / line 43 (which wins per the wire-freeze clause) does not require an in-ciphertext PTS; ADR §2's PTS has no producer → `docs/TODO.md` entry, no ADR edit.
- **E2** — undeclared/non-video slot id aborts the stream, counted `stream_slot_mismatch` + `undeclared_stream_id{media_kind="video"}` diagnostic.
- **E3** — `DEFAULT_RECEIVE_VIDEO_SLOTS` stays 0 (no grid UI until C-C1).
- **Deviation from task text, accepted:** `self_sender` / `sender_server_muted` are decided pre-accept (after verify, before decrypt), with a draw-time re-check retained. The task text placed them render-side. This is stricter (security concurs) and avoids A5 mute-race pollution (observability). The consequence is a sixth recovery reason, `server_mute_lifted`: one request per unmute edge from a verified stored pair, with no timer and no retry. Task 20 (OBS-B) must take the vocabulary as 6 values; the story file enumerates the reasons only in task 14's own text.
- **Deviation from task text, accepted:** the distinct-active-video-sources series ships as COUNTER `dt_client_media_active_source_intervals_total`, not a gauge. Observability is the shape owner, and story-2 `receive_source_deficit` is the precedent (a gauge is last-writer-wins across browsers).
- **For /close-story:** R-30's "no sender/stream identity on the test bus" conflicts with R-32's per-slot/per-sender attribution. Security's ruling (slotId, verified lastRenderedSender and per-(slot, sender) cumulative counters allowed on the test-define-gated bus; no GoP/sizes/timestamps/durations/per-frame events) stands for this task, and R-30's wording needs amending at story close.

---

## Pre-Work

{Any pending changes committed before starting, dependencies resolved, etc.}

{Or "None" if no pre-work was required}

---

## Implementation Summary

### Receive path (hot path, `packages/sdk-core/src/media/pipeline/`)
| Item | Before | After |
|------|--------|-------|
| Receive policy | resolve/verify/gate/open inline in `ingress.ts` (audio only) | ONE `receiveByKind(keyed, gate, ctx)` (`receivePolicy.ts`) for both kinds; audio ingress calls it; `RECEIVE_POLICY_OUTCOMES` closed outcome set (policy + render) |
| Accounting | private counters in `IngressPipeline` | `frameAccounting.ts` shared; counting point = parse boundary per kind; video identity excludes `VIDEO_POST_DECODE_REJECT_REASONS` = [`decode_over_bound`] |
| Stream reader | none (datagram-only codec) | `streamReader.ts`: extent via new `frameCodec.ts:peekFrameExtent` (MAX checked before alloc), one exact allocation per frame, whole frames only, `lastHeader` for reset requests |
| Video ingress | none | `videoIngress.ts`: slot binding from first frame (E2 `stream_slot_mismatch`), the policy, `RECOVERY_REASON_FOR` exhaustive table, abort accounting |
| Slot pipelines | none | `videoSlotPipeline.ts`: exactly M (eager decoder), jitter buffer (frames/bytes/max-hold, event-driven), signed-flag group gate, arrival-time re-based timeline (E1), sender tag via in-flight FIFO, geometry/rate/VP8-header bounds, per-slot fault isolation + backoff, loss dedupe (last pair, S1), `server_mute_lifted` repair, close-exactly-once |
| Render decision | none | `renderDecision.ts`: exhaustive `Record<SlotStateToken, RenderRule>`, never-render re-check (self / muted / unmapped) |

### Lifecycle, setup, session
- `lifecycle/VideoReceiver.ts` orchestrator (M slots, per-handler uni acceptor with global cap, own cache/replay via `receiverState.ts:bindReceiverState` shared with audio, recovery queue, unfulfilled series, decode-activity counter, evidence, live senders, `settled()` test seam).
- `lifecycle/receiveTransports.ts` generalised to `PerHandlerReadLoops<T, C>` (datagram + uni acceptor bodies); `recoveryRequestQueue.ts` (one queued per (handler, slot), one in flight per handler); `unfulfilledSlots.ts`.
- `setup/seams.ts` (`VideoDecoderSeam`, `VideoDecoderFactory`, `VideoSink` borrow contract), `setup/vp8.ts` (`buildVp8DecoderConfig` + `optimizeForLatency`, `vp8KeyframeDimensions`), `setup/vp8Decoder.ts` (browser-only, coverage-excluded), `setup/mediaMetrics.ts` (7 series, `MEDIA_RECOVERY_SOURCES`, bucket consts), `MediaTransport.incomingUniStreamsFor`.
- `config/clientConfig.ts`: `media.videoReceive` + `VIDEO_STREAM_CAP_HEADROOM` + `videoReceiveConfigLogLine`; `DEFAULT_RECEIVE_VIDEO_SLOTS` stays 0 (E3 rationale).
- Session: `mediaStart.ts` builds the receiver when M>0, purges both kinds; `mediaWiring.ts` routes video assignments, `bestEffortMuteReport` (moved out of `MeetingSession` to keep the class at the dt-guard 800-line cap); `signalingMediaState.ts` `videoMuteState(sender)` through the roster's one index + unmute edge; `MeetingSession.videoReceive` (a frozen NARROW view: `setVideoSink`, `evidence()`, `unfulfilledSnapshot()` — security F4; the live `VideoReceiver` stays internal), `videoSlotRenderChanged` event, live-video `slotPairing`; `SignalingClient` roster-index delegates; `rosterKeyFeed.participantIdOf`; `pairing.ts` live video.
- Telemetry: `MetricsSink.histogram(…, boundaries?)` → OTel advice (throw on drift in `throw` mode); test-utils parity copy + InMemory records boundaries.
- Test doubles: `test-utils/src/media/video.ts` (`FakeVideoDecoders`, `FakeVideoFrameRegistry`).

### Gate-3 fixes (all findings fixed; none deferred)
- **operations**: OPS-1 unfulfilled + recovery-queue stop SYNCHRONOUSLY in `VideoReceiver.stop()` before teardown awaits (session `disconnect()` test asserts exit + gauge 0 with no await); OPS-2 collector comment `source`/`MEDIA_RECOVERY_SOURCES`; OPS-3 F22 gains the failed-write fault line and the `VITE_DT_VIDEO_RECEIVE_SLOTS=0` / `MC_MAX_VIDEO_RECEIVE_SLOTS=0` levers.
- **security**: F1 `setSink` re-checks `mayRender` (discard counted); F2 delta-flagged VP8 key-frame bitstream discarded (`vp8IsKeyFrame`); F3 synchronous `decode()` throw → this slot's decoder fault; F4 narrow frozen `VideoReceiveView` on the session; F5 O(1) reverse map in `RosterKeyFeed` written only with the forward map (`#bind`/`#unbind`).
- **dry-reviewer**: F1 `parseExtLength`/`extLengthOffset` shared by `decodeFrame` and `peekFrameExtent`; F2 `readRelayStreamId` + extent `relayRegionOffset`/`payloadLength`, reader knows no field positions; F3 `sframe.ts:keyIdSenderOf` used by audio ingress, video ingress and the reader; F4 ONE accept/drop tail in `FrameAccounting` (wrap outcome by exclusion + verification evidence) — video now reports `key_wrap_outcomes_total`; F5 ONE `classifySender` (`self|muted|unmapped|ok`, arrival vs render mapping documented, security ruling); F6 `pipeline/decoderRestartGate.ts` used by audio lanes and video slots; F7 `teardown.ts:teardownFailureMessage`.
- **test**: F1 silent-new-source test; F2 audio ingress keeps accepting after a video decoder fault (+ audio identity with `[]`); F3 receiver-level `self_sender` counted/not drawn/identity; F4 both `decode_over_bound` paths + identity; F5 `disconnect()` is the SDK's single teardown entry point (no separate leave/dispose API) — session test asserts synchronously; F6 corpus asserts exact `incomplete` + exact allocations, MAX case exact extent; F7 max-hold asserts no late feed (found and fixed a real bug: the expiry now runs BEFORE the group gate in `push`).
- **observability**: F1 as OPS-1; F2 "rendered" = delivered to an attached sink without throwing (no sink → not rendered until attach; throwing sink → fixed-text `playback` fault, stays unfulfilled, no latency) — catalog states it; F3 as OPS-2; F4 histogram row reworded (no fleet "now" signal); F5 routine MH resets noted on `stream_partial_reset` and `group_abandoned`; nit: panel names `VIDEO_STREAM_CAP_HEADROOM`.
- **code-reviewer**: CQ-1 `receiveByKind` overloads (audio result carries `lane: DecodeLane`, no `?.`); CQ-2 = security F5; CQ-3 = DRY F2; CQ-4 = security F3; CQ-5 docs now say `MeetingSession.videoReceive.setVideoSink`; CQ-6 boundaries compared by reference then element-wise (no allocation), drift warned once per name; CQ-7 `#outputExceedsBounds` / `#countOverBound`; CQ-8 `isAvatarState` narrowing; CQ-9 `ReceiverStatePrefix = '' | 'video-'`; CQ-10 one refusal per decode-bound key; CQ-11 rewrap + `#reads` moved.

### Docs / ops
Catalog rows (7 new + identity section + widened rows + count-free banner), collector allowlist + C-B budget, 7 dashboard panels, label-taxonomy `source` row, dashboards.md / alerts.md deliberate absences, `client-dev-local.md` F21/F22, `docs/TODO.md` ADR §2 PTS entry, INDEX.md.

### Edits to EXISTING test files (T11: none removes or loosens an assertion)
- `media/setup/__tests__/mediaMetrics.test.ts` — 7 rows added to the per-kind emitter table (required by its completeness check) + a C-B describe block.
- `media/setup/__tests__/vp8.test.ts`, `telemetry/__tests__/sinks.test.ts`, `signaling/__tests__/pairing.test.ts`, `signaling/__tests__/rosterKeyFeed.test.ts`, `session/__tests__/meeting-session.test.ts` — appended describe blocks only.
- `media/__tests__/hotPathLayout.test.ts` — the positive-control name list gains the six new pipeline files.
- `media/pipeline/__tests__/frameFixtures.ts` (helper) — two OPTIONAL spec fields (`independentlyDecodable`, `sealTransmitKey`), defaults unchanged.
- Existing audio suites (`ingress.*`, `receiveLanes`, `audioPipeline*`) pass UNCHANGED after the policy/accounting refactor.

---

## Files Modified

```
docs/TODO.md                                       |   2 +
 docs/observability/alerts.md                       |  14 +
 docs/observability/dashboards.md                   |   1 +
 docs/observability/label-taxonomy.md               |   2 +
 docs/observability/metrics/client.md               | 244 +++++++-
 docs/runbooks/client-dev-local.md                  |  49 ++
 docs/specialist-knowledge/client/INDEX.md          |  16 +-
 infra/grafana/dashboards/client-media.json         | 651 +++++++++++++++++++++
 infra/services/otel-collector/collector.yaml       |  33 +-
 packages/sdk-core/src/config/clientConfig.ts       | 158 ++++-
 packages/sdk-core/src/index.ts                     |  22 +
 packages/sdk-core/src/media/MediaTransport.ts      |  13 +
 .../src/media/__tests__/hotPathLayout.test.ts      |  13 +-
 packages/sdk-core/src/media/frame/frameCodec.ts    |  74 +++
 packages/sdk-core/src/media/frame/rejectReason.ts  |  19 +
 .../sdk-core/src/media/lifecycle/AudioPipeline.ts  |  40 +-
 .../src/media/lifecycle/receiveTransports.ts       | 185 ++++--
 .../src/media/pipeline/__tests__/frameFixtures.ts  |  15 +-
 packages/sdk-core/src/media/pipeline/ingress.ts    | 216 +++----
 .../src/media/setup/__tests__/mediaMetrics.test.ts |  58 ++
 .../sdk-core/src/media/setup/__tests__/vp8.test.ts |  51 ++
 packages/sdk-core/src/media/setup/mediaMetrics.ts  | 179 +++++-
 packages/sdk-core/src/media/setup/seams.ts         |  64 ++
 packages/sdk-core/src/media/setup/vp8.ts           |  59 +-
 packages/sdk-core/src/session/MeetingSession.ts    |  25 +-
 .../src/session/__tests__/meeting-session.test.ts  | 119 ++++
 packages/sdk-core/src/session/events.ts            |  22 +
 packages/sdk-core/src/session/mediaStart.ts        |  75 ++-
 packages/sdk-core/src/session/mediaWiring.ts       |  55 +-
 .../sdk-core/src/session/signalingMediaState.ts    |  40 +-
 packages/sdk-core/src/signaling/SignalingClient.ts |  10 +
 .../src/signaling/__tests__/pairing.test.ts        |  16 +
 .../src/signaling/__tests__/rosterKeyFeed.test.ts  |  13 +
 packages/sdk-core/src/signaling/pairing.ts         |  20 +-
 packages/sdk-core/src/signaling/rosterKeyFeed.ts   |  21 +-
 packages/sdk-core/src/telemetry/MetricsSink.ts     |  15 +-
 packages/sdk-core/src/telemetry/OtelMetricsSink.ts |  21 +-
 .../sdk-core/src/telemetry/__tests__/sinks.test.ts |  40 ++
 packages/sdk-core/vitest.config.ts                 |   2 +
 packages/test-utils/src/InMemoryMetricsSink.ts     |  10 +-
 packages/test-utils/src/contracts/MetricsSink.ts   |  15 +-
 packages/test-utils/src/index.ts                   |   9 +
 42 files changed, 2377 insertions(+), 329 deletions(-)
```
New (untracked) files: `pipeline/{streamReader,receivePolicy,frameAccounting,videoIngress,videoSlotPipeline,renderDecision}.ts`, `lifecycle/{VideoReceiver,receiverState,recoveryRequestQueue,unfulfilledSlots}.ts`, `setup/vp8Decoder.ts`, `test-utils/src/media/video.ts`, and their tests (see the Cross-Boundary table).

`./scripts/layer-fast.sh`: Layers 1-6 OK (sdk-core 1691 unit tests, coverage 96.2 / 91.0 / 95.1 / 97.6; test-utils 44).

---

## Devloop Verification Steps

The fast check (`DEVLOOP_FMT_APPLY=1 ./scripts/layer-fast.sh`) passed Layers 1-6 before review. After the Gate-3 fixes, the implementer re-ran it: Layers 1-6 OK, sdk-core 1704 tests, coverage 96.1% statements / 91.0% branches, test-utils 45 tests.

Gate 2 was the full `DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` on the final tree. Result: `TOTAL_RESULT=OK`, `TOTAL_DURATION=1368`.

| Layer | Result | Duration (s) |
|-------|--------|--------------|
| 1 Compile | OK | 8 |
| 2 Format | OK | 5 |
| 3 Guards | OK | 155 (budget WARN only) |
| 4 Test | OK | 153 (budget WARN only) |
| 5 Lint | OK | 6 |
| 6 Audit | OK | 6 |
| 7 Env-tests (Rust env-tests + browser E2E) | OK | 1035 |

There are no env-tests for the new video receive behaviour. Its browser scenarios belong to task 17 (C-C1). Layer 7 shows that the existing env-tests and browser specs stay green.

---

## Code Review Results

Per-reviewer findings and fixes are in §"Gate-3 fixes" (implementer) and summarised in the Lead table below. No findings were deferred. The semantic guard ran Credential Leak items 5-10 and Client Credential Lifetime; the native verdict was SAFE and it had no findings. DRY added no `docs/TODO.md` extraction entries: nothing duplicates another service.

### Gate 3 — Verdicts (Lead)

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security (paired) | RESOLVED-FIXED | 5 | 5 | 0 | setSink never-render check; delta-flagged VP8 keyframe bypass; sync decode() throw containment; narrow frozen `videoReceive` view; O(1) reverse index |
| Test (paired) | RESOLVED-FIXED | 7 | 7 | 0 | F7 exposed a real max-hold expiry bug, now fixed |
| Observability | RESOLVED-FIXED | 5 (+1 nit) | 5 | 0 | sync teardown before final flush; "rendered" = delivered to a sink without throwing |
| Code Quality | RESOLVED-FIXED | 11 | 11 | 0 | |
| DRY | RESOLVED-FIXED | 7 | 7 | 0 | no TODO extraction entries |
| Operations | RESOLVED-FIXED | 3 | 3 | 0 | runbook F22 off lever |
| Semantic Guard | CLEAR (native SAFE) | 0 | 0 | 0 | |

---

## Accepted Deferrals

**Each entry here is an issue the devloop chose NOT to fix.** Every bullet is a cost shift: the implementer didn't pay the fix-now cost, so a future reader will pay fix-later cost + tracking overhead. List only what was actually deferred — not "follow-ups" or "future improvements" or "potential extractions." If something was fixed, it doesn't belong here.

**Tech debt entries themselves live in `docs/TODO.md`. This section holds only pointers to those entries.** Do not create a `TODO.md` at the repo root or anywhere else — there is exactly one `docs/TODO.md` for the whole project. Do not inline the debt body here — multi-line entries belong in `docs/TODO.md`, not in this section.

Each pointer is exactly one bullet of the form `- \`docs/TODO.md\` §SECTION-NAME — one-line hook (≤80 chars)`. If you wrote more than one line per entry, you're writing it in the wrong file — move the body to `docs/TODO.md` and leave only the pointer here.

- (none surfaced in this devloop)

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `fb8c03a02f8b8275d21aad7f7d9ba8857afa4b6d`
2. Review all changes: `git diff fb8c03a02f8b8275d21aad7f7d9ba8857afa4b6d..HEAD`
3. Soft reset (preserves changes): `git reset --soft fb8c03a02f8b8275d21aad7f7d9ba8857afa4b6d`
4. Hard reset (clean revert): `git reset --hard fb8c03a02f8b8275d21aad7f7d9ba8857afa4b6d`
5. For schema changes: rollback requires a forward migration — `git reset` alone is insufficient if migrations were applied
6. For infrastructure changes: may require `skaffold delete` or `kubectl delete -f` if manifests were applied
7. **Safe-revert unit** (answer explicitly, even if "the whole commit"): can any part of this diff be reverted or cherry-picked on its own, or does a partial revert reconstruct a state worse than either endpoint (e.g. a security fix split from the change that made it necessary, or a client/server/alert-rule set that must move together)? If partial reverts are unsafe, name the unit and the safe direction. **Answer**: the whole commit (SDK receive path + collector allowlist + client-media dashboard + metric catalog + runbook F21/F22 move together). Once tasks 20 (OBS-B) / 21 (T-B) land, reverting this alone leaves the A5/A9 selectors silently empty (fails open), so revert those together or not at all. Prefer the off lever: `VITE_DT_VIDEO_RECEIVE_SLOTS=0` (client) or `MC_MAX_VIDEO_RECEIVE_SLOTS=0` (server). (This converts silence into a visible unanswered slot; it cannot distinguish a checked answer from a reflexive one.)

---

## Issues Encountered & Resolutions

### Issue 1: ADR §2 in-ciphertext PTS has no producer
**Problem**: The task asked to re-base "presentation timestamps (inside the ciphertext)", but C-A2 seals bare VP8 and no PTS exists.
**Resolution**: Lead ruling E1(A): use arrival timestamps from the injected clock, re-based per slot. Filed in `docs/TODO.md` §Media Path Obligations; the ADR is unedited.

### Issue 2: Max-hold expiry let a later delta reach the decoder
**Problem**: Test F7's stronger assertion showed that, after a frame expired for max-hold, the next delta of the same group was still fed to the decoder.
**Resolution**: Expiry now runs ahead of the group gate, and a regression test covers it.

### Issue 3: Deviations from the task text
**Problem**: Reviewer rulings changed several things the task text names. The recovery label is `source`, not `reason`, with 6 values (`server_mute_lifted` added). The active-sources series is a counter, not a gauge. Self and muted frames drop at arrival instead of render.
**Resolution**: The Lead accepted these with recorded rationale (§Lead rulings, §Planning). Tasks 20/21 must select on `source`. R-30's wording conflicts with R-32 and needs reconciling at /close-story.

---

## Lessons Learned

1. A task text can quote an ADR detail that an upstream task never built (the PTS). Check the producer exists before designing the consumer.
2. Moving a guard earlier, here self and muted frames to before decrypt, can create a liveness gap; the unmute freeze needed the `server_mute_lifted` repair. Ask what the earlier drop takes away.
3. Strengthening a weak test assertion found a real bug (F7). Test reviewers' "assert the exact outcome" findings pay for themselves.

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
