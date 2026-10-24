# Devloop Output: C-C2 local-only debug panel (R-7 self-check, R-35 stats overlay)

**Date**: 2026-10-24
**Task**: Story 3 task 18 (C-C2): R-7 camera/mic loopback self-check + R-35 statistics overlay in DebugPanel.svelte, local-only, off by default
**Specialist**: client
**Mode**: Agent Teams (v2) — full, Gate-1 present; --paired-with=security
**Branch**: `feature/user-see-each-other`
**Duration**: ~2h

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `85d80070d44a813cb85a2d6845619f8a613420eb` |
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
| Security | `paired-security` |
| Test | `test` |
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
Story 3 task 18 (C-C2): R-7 local camera/mic loopback self-check and R-35 local statistics overlay, both off by default, local only, never exported, mounted in one line each from InMeeting/JoinMeeting via `DebugPanel.svelte`.

### Scope
- **Service(s)**: client (sdk-core, web-app, test-utils)
- **Schema**: No
- **Cross-cutting**: No

### Debate Decision
NOT NEEDED - client-local feature within the story design (R-7, R-35)


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
| `packages/sdk-core/src/media/pipeline/frameAccounting.ts` | Mine | — |
| `packages/sdk-core/src/media/pipeline/ingress.ts` | Mine | — |
| `packages/sdk-core/src/media/pipeline/videoIngress.ts` | Mine | — |
| `packages/sdk-core/src/media/pipeline/videoSlotPipeline.ts` | Mine | — |
| `packages/sdk-core/src/media/pipeline/videoEgress.ts` | Mine | — |
| `packages/sdk-core/src/media/pipeline/egress.ts` | Mine | — |
| `packages/sdk-core/src/media/lifecycle/localStats.ts` (new) | Mine | — |
| `packages/sdk-core/src/media/lifecycle/AudioPipeline.ts` | Mine | — |
| `packages/sdk-core/src/media/lifecycle/VideoReceiver.ts` | Mine | — |
| `packages/sdk-core/src/media/lifecycle/VideoPublisher.ts` | Mine | — |
| `packages/sdk-core/src/media/setup/seams.ts` | Mine | — |
| `packages/sdk-core/src/media/setup/capture.ts` | Mine | — |
| `packages/sdk-core/src/media/setup/videoCapture.ts` (test-pattern `track: undefined`) | Mine | — |
| `packages/sdk-core/src/media/setup/videoCaptureBrowser.ts` | Mine | — |
| `packages/sdk-core/src/media/setup/localPreview.ts` (new) | Mine | — |
| `packages/sdk-core/src/session/localDebugSurface.ts` (new) | Mine | — |
| `packages/sdk-core/src/index.ts` | Mine | — |
| `packages/sdk-core/src/media/lifecycle/__tests__/` (extended unit tests, seam fakes) | Mine | — |
| `packages/sdk-core/src/media/pipeline/__tests__/` (new/extended counter tests) | Mine | — |
| `packages/sdk-core/src/media/setup/__tests__/` (new `localPreview.test.ts`) | Mine | — |
| `packages/sdk-core/src/session/__tests__/` (seam fakes, narrow-view keys, local surface test) | Mine | — |
| `packages/test-utils/src/media/index.ts` (`FakeCaptureSource.track`) | Mine | — |
| `packages/sdk-svelte/src/__tests__/MediaStore.test.ts` (view fake gains `localStats`) | Mine | — |
| `packages/web-app/src/views/DebugPanel.svelte` (new) | Mine | — |
| `packages/web-app/src/lib/debugStats.ts` (new) | Mine | — |
| `packages/web-app/src/lib/selfCheck.ts` (new) | Mine | — |
| `packages/web-app/src/lib/debugToggle.ts` (new) | Mine | — |
| `packages/web-app/src/views/InMeeting.svelte` (one-line mount) | Mine | — |
| `packages/web-app/src/views/JoinMeeting.svelte` (one-line mount) | Mine | — |
| `packages/web-app/src/__tests__/` (new debugPanel/selfCheck; extended inMeeting/joinMeeting/e2eBus; MockMeetingSession) | Mine | — |
| `packages/web-app/tests/debugNeverExported.test.ts` (new) | Mine | — |
| `packages/web-app/tests/debugStats.test.ts` (new) | Mine | — |
| `packages/web-app/tests/bundle-content.test.ts` (marker swap; security reviews) | Mine | — |
| `packages/web-app/src/lib/e2eAnalysis.ts` (header comment only) | Mine | — |
| `docs/specialist-knowledge/client/INDEX.md` | Mine | — |


---

## Planning

### Mechanism restatement

"A local read-only diagnostic surface over the media path that must be provably
disjoint from every export sink (MetricsSink, OTel, e2e bus, console) and from
every send seam." Same-owner siblings: the e2e bus (already an explicit
allow-list projection; it must NOT gain the new fields) and `unfulfilledSnapshot()`
(already the R-35 overlay hook from task 14; reused, not re-derived). No wider
class needing completion.

### Getter gap (tasks 13/14 did not land everything this task assumed)

Exists: `VideoPublisher.sendTotals()` (framesSent, keyframes by cause),
`AudioPipeline.frameCounts` (aggregate), `VideoReceiver.evidence()` (per-slot
`framesDecoded`), `unfulfilledSnapshot()`, `ReceiveLanes.activityFor/pendingFor`.
MISSING, added as pull-only snapshot state (plain integer `++` beside the existing
metric increments, no callback, no logging, no per-frame allocation, no string-keyed
lookups; per-slot tallies are PRE-ALLOCATED at construction for the declared slot ids
and looked up by numeric slot id):

| Need | Where (one increment, adjacent to the existing metric call) |
|---|---|
| per-slot packets + bytes arrived (both kinds) | `FrameAccounting.arrived(slot, bytes)` — audio ingress after hop observe; video ingress after slot binding |
| per-slot drops by reject reason (both kinds) | `FrameAccounting.dropped()` already receives the slot; bumps the slot's preallocated `Record<RejectReason, number>` |
| video per-slot local (post-accept) drops | `VideoSlotPipeline` — the 10 `metrics.decodeQueueDropped()` sites funnel through one `#localDrop()` that does metric + `++` |
| video per-slot jitter depth, keyframes seen | `VideoSlotPipeline.jitterDepth` (`#queue.length`), `#keyframesSeen++` in `push` |
| audio per-slot depth, decode count | existing lane `pendingFor` / `activityFor(sender).decoded` via the pipeline's retained assignments (audio has NO jitter buffer by design — `playbackSink.ts`; the overlay labels it "pre-decoder queue") |
| send bytes (both), groups abandoned (video) | `Egress.#bytesSent`, `VideoEgress.#bytesSent` / `#groupsAbandoned` beside `frameSent()` / `sendGroupAbandoned()` |

One assembly point: `MeetingSession.localStats(): LocalMediaStats | undefined`
(types in new `media/lifecycle/localStats.ts`; cumulative counters only, slot ids +
kind, NO sender ids, no timestamps). It is a NEW type, deliberately not a widening of
`VideoSlotEvidence`/`VideoSendCounts`/`MediaFrameCounts` (those feed the R-32 bus,
which security restricted to "no sizes"); it composes the existing getters
(`sendTotals().keyframes`, `framesDecoded`, `unfulfilledSnapshot`) rather than
re-counting them. Not wired to `mediaMetrics.ts`, MetricsSink or `e2eBus.ts`.
(SUPERSEDED at implementation — see "Implementation deviation": the assembly point is
`session/localDebugSurface.ts:localStatsOf`, and `VideoReceiveView` gains a read-only
`localStats()`, OK'd by paired-security under F4.)

### R-7 self-check (`lib/selfCheck.ts` controller + panel UI)

- In-meeting: REUSE ONLY. New read-only `MeetingSession.localCaptureTracks:
  { audio?: MediaStreamTrack; video?: MediaStreamTrack }` (via an optional `track`
  getter on `trackCapture`'s handle → `CaptureSource`/`VideoCaptureSource` seams →
  `AudioPipeline.captureTrack` / `VideoPublisher.captureTrack`). Reference only: never
  `stop()`, never `.enabled`, never `applyConstraints`, never `clone()`. Re-read on
  each panel tick; srcObject rebinds when the track identity changes, shows "camera
  off" when absent/ended (video mute stops the track → no reference held). Never opens
  its own capture in-meeting, so the session's camera-light rule is untouched.
  Test-pattern build: no track → "no local camera track (test pattern)".
- Pre-join (JoinMeeting, before any attempt): OWN capture, one `getUserMedia`
  ({video, audio}) on an explicit "Start self-check" click (never auto-started, never
  persisted). Stopped on "Stop", toggle-off and destroy (submitting Join unmounts it
  because the mount is in the idle `{:else}` branch). Pending-gUM race: a result
  arriving after close is stopped immediately. Failure: `err.name` from a fixed
  allowlist (NotAllowedError, NotReadableError, NotFoundError, OverconstrainedError,
  else "UnknownError") — never the message or cause.
- Mic: level METER only (own `AudioContext` → `MediaStreamAudioSourceNode` →
  `AnalyserNode`, never connected to `destination`, never audible); closed on stop.
- Injected seams for tests: `acquire`, `audioContextFactory`, tracks source, timer.

### R-35 overlay (`lib/debugStats.ts` + panel)

- One `rates(prev, next, dtMs)` pure function: packets/s and bits/s per row from
  successive snapshots; first sample / dt<=0 / negative delta (reset, slot gone) →
  `undefined` rendered as "—" (never NaN/Infinity). Only the previous snapshot is
  held. Explicit field allow-list projection to rows; labels are `kind` + slot id only.
- Per slot: pkt/s, kbit/s, depth, decoded, drops by reason (non-zero tokens only),
  keyframes seen (video), unfulfilled time-in-state (video). Per sent stream: pkt/s,
  kbit/s (summed over handlers), keyframes by cause, groups abandoned.
- Sampled on ONE timer (`DEBUG_STATS_SAMPLE_INTERVAL_MS = 1000`, own constant) only
  while the panel is open; cleared on hide and destroy. Injected `clock`/`setInterval`
  props for tests. No console.*, nothing on window/globalThis.

### Toggle + mounting

- One toggle button `data-testid="debug-panel-toggle"` ("Show debug options" /
  "Hide debug options"), at the end of the in-meeting section and under the idle Join
  form. Hidden by default. Persisted ONLY as `localStorage['darktower.debugPanel.v1']
  = '1'|'0'`; read fails closed (anything but exactly '1', or a throw → off); write in
  try/catch. Only panel visibility is persisted; the self-check never auto-starts.
- Mounts (one line each, narrowest props, no session/auth object crosses):
  - InMeeting: `<DebugPanel stats={() => session.localStats()} tracks={() => session.localCaptureTracks} />`
  - JoinMeeting (idle branch): `<DebugPanel />` (pre-join own-capture self-check only)
- Errors inside the panel are caught and rendered in the panel; never thrown into the view.

### Tests

- node `web-app/tests/debugNeverExported.test.ts`: walks DebugPanel.svelte's relative
  import closure (asserts it equals {DebugPanel.svelte, debugStats.ts, selfCheck.ts,
  debugToggle.ts}, each non-empty); denies import specifiers resolving to
  `lib/e2eBus`, `e2eAnalysis`, any sdk-core `telemetry/`, `MetricsSink`,
  `getMetricsSink`, `flushMetrics`, `OtelMetricsSink`, `__darktower_test__`,
  `IWebTransport`/`MediaTransport`/`SignalingClient`; raw `dt_client_` anywhere
  (comments included); `console.`; `globalThis`/`window.` writes. Positive controls:
  the matcher hits `src/lib/e2eBus.ts` (resolved real path) and synthetic bad
  sources. Reverse direction: `e2eBus.ts` and sdk-core `mediaMetrics.ts` never
  reference `localStats`/`localCaptureTracks`.
- node `web-app/tests/debugStats.test.ts`: exact rates over ≥3 snapshots, first
  sample, dt=0, negative delta, slot disappearing.
- browser `src/__tests__/selfCheck.test.ts`: recording Proxies typed as
  `IWebTransport` and `MediaTransport` reachable from the same source object the
  in-meeting mount closes over; open/close; zero accesses; AND proof it ran (video
  srcObject set, analyser created). Reused track: `stop` spy never called, no clone;
  track swap rebinds. Own capture: every track stopped on close/destroy; pending-gUM
  race stops late stream; error-name allowlist.
- browser `src/__tests__/debugPanel.test.ts`: default hidden (empty storage), toggle
  shows/hides + persists, throwing getItem/setItem, rates rendered from synthetic
  snapshot sequence on injected clock with exact numbers, no getter calls after hide
  or unmount.
- sdk-core unit tests with exact counts for every new counter (per-slot arrived/drops,
  keyframes seen, jitter depth, local drops, bytes sent, groups abandoned) and for
  `MeetingSession.localStats()` assembly / `localCaptureTracks`.


### Gate-1 amendments

- DRY A: pre-join capture goes through a new sdk-core `media/setup/localPreview.ts:openLocalPreview()` (beside `createCameraCapture`), reusing `classifyCaptureError` and the "no track → stop all, NoDevice" handling; it throws `MediaCaptureError`. The panel renders the `CaptureFailure` token (via `err.reason`; anything else → `unknown`) — no second name allowlist. Race handling and stop-on-close stay in `lib/selfCheck.ts` (injected `openPreview` seam).
- DRY B: option (i). The meter keeps `createAnalyser` with `getByteTimeDomainData` (time-domain RMS); `createAnalyser` is removed from `bundle-content.test.ts` FORBIDDEN, and the e2eAnalysis tree-shake proof keeps its unique markers `getFloatFrequencyData` + `receiveAnalysis` (both still present in the positive-control build). Comment in the test records why. Security ruling (paired-security): meter computes a scalar RMS from `getByteTimeDomainData` only and retains nothing; `getByteFrequencyData` is ADDED to the forbidden set (handled so the positive-control rule stays honest — it is not in any build, so it goes in a never-anywhere bucket rather than the must-appear-in-control list); `e2eAnalysis.ts` header comment updated to name the tokens that now carry the proof; selfCheck test asserts the analyser's only source is the local capture track. The debugNeverExported closure scan (which has positive controls) also denies `getByteFrequencyData`/`getFloatFrequencyData` in the debug modules.
- Code-review 1: the seam property is REQUIRED `readonly track: MediaStreamTrack | undefined` on `CaptureSource` and `VideoCaptureSource` (the `grantedDeviceId` precedent); every implementation and fake updated.
- Code-review 2: neutral "no local camera track" in every case.
- Test (1)-(6), observability (a)-(c), security notes (1)-(3) folded in as stated in their messages.

### Implementation deviation (surfaced to reviewers)

`MeetingSession` gains NO member. Adding `localStats()` / `localCaptureTracks` pushed the
`MeetingSession` class to 816 lines, tripping dt-guard's
`ts_retained_credentials::cap_clears_the_largest_real_declaration_with_margin`
(cap 2400 must be >= 3x the largest declaration; baseline was 798). Rather than raise a
security-owned guard threshold, the assembly moved to sdk-core
`session/localDebugSurface.ts:localStatsOf(session)` / `localCaptureTracksOf(session)`, which
read the session's EXISTING public getters (`media`, `video`, `videoReceive`) structurally.
`VideoReceiveView` (security F4's narrow view) IS widened by one READ-ONLY member, `localStats()` — superseding the Planning statement that no bus-fed/narrow type is widened; paired-security explicitly OK'd this under F4 at Gate 3
(the narrow-view key test updated). `AudioPipeline.localStats()` returns `undefined` once
stopped, so the snapshot is `undefined` after leave/disconnect. InMeeting's mount is still one
line: `<DebugPanel stats={() => localStatsOf(session)} tracks={() => localCaptureTracksOf(session)} />`.

### Ops

Client-bundle only: no proto, config, infra, alert or metric change. Additive
read-only SDK getters; reverting the commit is the whole rollback. No env-tests (local
only by design). How to open: in-meeting, click "Show debug options" at the bottom of
the meeting view (or under the Join form before joining); the self-check needs
"Start self-check".


### Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security (paired) | confirmed |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Semantic Guard | confirmed |

Classification-sanity guard: `STATUS=OK REASON=cross-boundary-classification-clean-1-files`. Plan approved by Lead.

---

## Pre-Work

None

---

## Implementation Summary

### SDK (sdk-core): pull-only local snapshot state
| Item | Before | After |
|------|--------|-------|
| `FrameAccounting` | kind totals only | + per declared slot tally (packets, bytes, drops by reason), pre-allocated, numeric lookup, unknown slot ignored; `arrived()` / `slotTally()` |
| `VideoSlotPipeline` | `framesDecoded` | + `keyframesSeen`, `jitterDepth`, `localDrops` (11 `decodeQueueDropped` sites funnel through `#localDrop()`) |
| `EgressPipeline` / `VideoEgress` | `framesSent` | + `bytesSent`; video + `groupsAbandoned` |
| `AudioPipeline` / `VideoReceiver` / `VideoPublisher` | — | `localStats()` / `localSendStats()`; `captureTrack` (video: only while lit) |
| capture seams | — | REQUIRED `readonly track: MediaStreamTrack \| undefined` on `CaptureSource` / `VideoCaptureSource` |
| `VideoReceiveView` | 3 members | + read-only `localStats()` |
| new | — | `localStats.ts` (types), `localPreview.ts:openLocalPreview` (pre-join capture, `classifyCaptureError`), `session/localDebugSurface.ts:localStatsOf` / `localCaptureTracksOf` |

### Web app
`views/DebugPanel.svelte` + `lib/{debugStats,selfCheck,debugToggle}.ts`; one-line mounts in
`InMeeting.svelte` (closures over `localStatsOf` / `localCaptureTracksOf`) and in
`JoinMeeting.svelte`'s idle branch (no props; own preview capture). `bundle-content.test.ts`
marker swap (`createAnalyser` out, `getByteFrequencyData` never-anywhere) per security ruling.

How to open (for OPS-B): "Show debug options" (`data-testid=debug-panel-toggle`) at the bottom of
the in-meeting view, or under the Join form before joining; the self-check then needs "Start
self-check".

---

## Files Modified

```
 docs/specialist-knowledge/client/INDEX.md          |   4 +-
 packages/sdk-core/src/index.ts                     |  18 +
 .../sdk-core/src/media/lifecycle/AudioPipeline.ts  |  53 +++
 .../sdk-core/src/media/lifecycle/VideoPublisher.ts |  28 ++
 .../sdk-core/src/media/lifecycle/VideoReceiver.ts  |  35 +-
 .../lifecycle/__tests__/VideoPublisher.test.ts     |  47 +++
 .../audioPipeline.deviceAndCounts.test.ts          |  79 +++-
 .../lifecycle/__tests__/videoReceiver.test.ts      |  47 +++
 .../sdk-core/src/media/lifecycle/localStats.ts     |  88 ++++
 .../pipeline/__tests__/egress.multiTarget.test.ts  |   5 +
 .../__tests__/frameAccounting.slotTally.test.ts    |  54 +++
 .../media/pipeline/__tests__/videoEgress.test.ts   |  24 ++
 .../pipeline/__tests__/videoSlotPipeline.test.ts   |  29 ++
 packages/sdk-core/src/media/pipeline/egress.ts     |   8 +
 .../sdk-core/src/media/pipeline/frameAccounting.ts |  69 +++-
 packages/sdk-core/src/media/pipeline/ingress.ts    |  16 +-
 .../sdk-core/src/media/pipeline/videoEgress.ts     |  16 +
 .../sdk-core/src/media/pipeline/videoIngress.ts    |   2 +
 .../src/media/pipeline/videoSlotPipeline.ts        |  51 ++-
 .../src/media/setup/__tests__/localPreview.test.ts |  87 ++++
 packages/sdk-core/src/media/setup/capture.ts       |  37 +-
 packages/sdk-core/src/media/setup/localPreview.ts  |  80 ++++
 packages/sdk-core/src/media/setup/seams.ts         |  16 +
 packages/sdk-core/src/media/setup/videoCapture.ts  |   2 +-
 .../src/media/setup/videoCaptureBrowser.ts         |   3 +
 .../session/__tests__/mediaSelection.video.test.ts |   1 +
 .../src/session/__tests__/meeting-session.test.ts  |  32 ++
 packages/sdk-core/src/session/localDebugSurface.ts |  57 +++
 .../sdk-svelte/src/__tests__/MediaStore.test.ts    |   1 +
 packages/test-utils/src/media/index.ts             |   2 +
 packages/web-app/src/__tests__/debugPanel.test.ts  | 271 ++++++++++++
 packages/web-app/src/__tests__/e2eBus.test.ts      |   1 +
 .../src/__tests__/helpers/MockMeetingSession.ts    |  44 +-
 packages/web-app/src/__tests__/inMeeting.test.ts   |  25 ++
 packages/web-app/src/__tests__/joinMeeting.test.ts |  80 ++++
 packages/web-app/src/__tests__/selfCheck.test.ts   | 452 +++++++++++++++++++++
 packages/web-app/src/lib/debugStats.ts             | 158 +++++++
 packages/web-app/src/lib/debugToggle.ts            |  49 +++
 packages/web-app/src/lib/e2eAnalysis.ts            |   9 +-
 packages/web-app/src/lib/selfCheck.ts              | 290 +++++++++++++
 packages/web-app/src/views/DebugPanel.svelte       | 307 ++++++++++++++
 packages/web-app/src/views/InMeeting.svelte        |   4 +-
 packages/web-app/src/views/JoinMeeting.svelte      |   6 +
 packages/web-app/tests/bundle-content.test.ts      |  31 +-
 packages/web-app/tests/debugNeverExported.test.ts  | 338 +++++++++++++++
 packages/web-app/tests/debugStats.test.ts          |  97 +++++
 46 files changed, 3110 insertions(+), 43 deletions(-)
```

### Key Changes by File
| File | Changes |
|------|---------|
| `packages/web-app/src/views/DebugPanel.svelte` | New self-contained panel: toggle, R-7 self-check UI, R-35 overlay |
| `packages/web-app/src/lib/{selfCheck,debugStats,debugToggle}.ts` | Self-check controller, rate computation from snapshots, fail-closed localStorage toggle |
| `packages/web-app/src/views/{InMeeting,JoinMeeting}.svelte` | One-line mounts |
| `packages/web-app/tests/debugNeverExported.test.ts` | Mechanical never-exported import-closure + reverse scan |
| `packages/web-app/tests/bundle-content.test.ts` | createAnalyser off the deny list (security ruling); getByteFrequencyData banned |
| `packages/sdk-core/src/media/lifecycle/localStats.ts`, `session/localDebugSurface.ts` | LocalMediaStats type + `localStatsOf`/`localCaptureTracksOf` |
| `packages/sdk-core/src/media/setup/localPreview.ts` | `openLocalPreview()` pre-join capture reusing `classifyCaptureError` |
| `packages/sdk-core/src/media/pipeline/*` | Pull-only per-slot/per-stream counters beside existing metric increments |

---

## Devloop Verification Steps

### Gate 2 — `DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh`
**TOTAL_RESULT**: OK (TOTAL_DURATION=1732s), first attempt; no FMT_APPLIED rewrites.

| Layer | Result | Duration (s) |
|-------|--------|--------------|
| 1 Compile | OK | 5 |
| 2 Format | OK | 5 |
| 3 Guards | OK | 153 |
| 4 Test | OK | 155 |
| 5 Lint | OK | 1 |
| 6 Audit | OK | 5 |
| 7 Env-tests (Rust + browser E2E) | OK | 1408 |

The fast check (`layer-fast.sh`, L1-6) was green before review (334s; WARN BUDGET_TOTAL_BREACH on L3+L6 vs the 90s p95 budget, which is a warning, not a gate). No new env-tests: both features are local-only by design; Layer 7 ran the existing suites as a regression net.

---

## Code Review Results

### Gate 3 Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security (paired) | RESOLVED-FIXED | 1 | 1 | 0 | S1 scan scope; both deviations accepted |
| Test | RESOLVED-FIXED | 5 | 5 | 0 | |
| Observability | RESOLVED-FIXED | 2 | 2 | 0 | |
| Code Quality | RESOLVED-FIXED | 10 | 10 | 0 | |
| DRY | RESOLVED-FIXED | 3 | 3 | 0 | |
| Operations | RESOLVED-FIXED | 2 | 2 | 0 | |
| Semantic Guard | CLEAR | 0 | 0 | 0 | native SAFE |

### Security Specialist (paired)
**Verdict**: RESOLVED-FIXED
**Findings**: 1 found, 1 fixed, 0 deferred

- S1: the never-exported scan did not cover `session/localDebugSurface.ts` or `media/setup/localPreview.ts`. Both are now scanned; localPreview has a single asserted `globalThis.navigator?.mediaDevices` allowance; reverse tokens added.
- Deviation (1), assembly moved to `localDebugSurface.ts` with MeetingSession unchanged (dt-guard ts_retained_credentials size cap): ACCEPTED.
- Deviation (2), read-only `VideoReceiveView.localStats()`: ACCEPTED under F4 (read-only fresh copy; the bus picks only `evidence()`; the reverse scan pins that).
- Gate-1 ruling: `createAnalyser` comes off the bundle-content deny list (own-mic meter); the stripping proof re-anchored on `getFloatFrequencyData`/`receiveAnalysis`; `getByteFrequencyData` banned from all builds.

### Test Specialist
**Verdict**: RESOLVED-FIXED
**Findings**: 5 found, 5 fixed, 0 deferred

F1 vacuous no-transport test (now mounts through the real `localStatsOf`/`localCaptureTracksOf` closures over a recording-proxied session); F2 non-discriminating dt=0/reset assertions; F3 re-export scan gap; F4 SDK surface scan scope; F5 masked meter failure (`meterFailed` state).

### Observability Specialist
**Verdict**: RESOLVED-FIXED
**Findings**: 2 found, 2 fixed, 0 deferred

F1 `export … from` blind spot in the never-exported scan (now parsed and followed, with 3 positive controls); F2 plan/code drift on the VideoReceiveView widening (record corrected; security OK).

### Code Quality Reviewer
**Verdict**: RESOLVED-FIXED
**Findings**: 10 found, 10 fixed, 0 deferred

Meter AudioContext retry storm; 3 masked/mislabelled failure paths (meter, stats getter, swallowed `start()` rejection); N/A convention for audio `localDrops`; SSoT kind type; duplicated `streamOf` default; `CaptureFailure`-typed failure field; 2 text nits; stale duplicate table in main.md.

### DRY Reviewer
**Verdict**: RESOLVED-FIXED

**True duplication findings** (entered fix-or-defer flow):
- Keyframe causes hand-restated in `debugStats.ts` (now projected from SDK counts) — fixed
- `'unknown'` literal vs `CaptureFailure.Unknown` — fixed
- Duplicated `new MediaStream([t])` default (now one `streamOfTrack`) — fixed
- (Gate 1) pre-join error allowlist would duplicate `classifyCaptureError` → `openLocalPreview()` in sdk-core

**Extraction opportunities** (appended to `docs/TODO.md`):
None

### Operations Reviewer
**Verdict**: RESOLVED-FIXED
**Findings**: 2 found, 2 fixed, 0 deferred

F1 a self-check tick throw left the panel stuck on "running" (now stops, releases capture, shows failed/unknown); F2 no evidence the pre-join preview is released before `join()` (new joinMeeting test).

### Semantic Guard Reviewer
**Verdict**: CLEAR
**Native verdict**: SAFE
**Findings**: 0 found, 0 fixed, 0 deferred

No findings (Credential Leak items 5-10, Client Credential Lifetime). JoinMeeting mounts `<DebugPanel />` with no props; InMeeting passes two narrow closures; localStorage holds only '1'/'0'.

---

## Accepted Deferrals

- (none surfaced in this devloop)

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `85d80070d44a813cb85a2d6845619f8a613420eb`
2. Review all changes: `git diff 85d80070..HEAD`
3. Soft reset (preserves changes): `git reset --soft 85d80070`
4. Hard reset (clean revert): `git reset --hard 85d80070`
5. No schema changes.
6. No infrastructure changes; client bundle only (redeploy web-app after revert).
7. **Safe-revert unit**: **Answer**: `the whole commit`. The SDK snapshot getters are additive and read-only, so the SDK half could stand alone. But reverting the SDK without the web-app breaks compilation of DebugPanel and its tests, and reverting `bundle-content.test.ts` alone re-bans `createAnalyser`, which the meter uses. Revert as one unit.

---

## Issues Encountered & Resolutions

### Issue 1: Overlay getters assumed from tasks 13/14 mostly absent
**Problem**: per-slot bytes/packets/drops-by-reason/jitter depth/keyframes seen and per-stream bytes/groups abandoned did not exist.
**Resolution**: added as pull-only snapshot counters beside the existing metric increments (no per-frame callbacks/logging), read via `localStatsOf`.

### Issue 2: MeetingSession size cap
**Problem**: adding members pushed MeetingSession past the dt-guard ts_retained_credentials size-cap test margin.
**Resolution**: free functions in `session/localDebugSurface.ts` over narrow Pick-typed sources; the security-owned threshold was not raised. Security accepted.

---

## Lessons Learned

1. Task prompts naming "getters provided by task N" should be verified at planning; here most were missing.
2. Mechanical "never exported" scans need re-export handling and positive controls, or they can pass vacuously.

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
