# Devloop Output: C-C1 — see-each-other grid (Svelte adapter, paired video grid, mute controls, R-18 notice, e2e bus, browser env-tests)

**Date**: 2026-10-24
**Task**: Story 3 task 17 (C-C1) — demonstrable see-each-other grid; see `/home/dev/.cache/devloop/story-runs/story-runner/2026-10-07-see-each-other/task-17.prompt`
**Specialist**: client
**Mode**: Agent Teams (v2) — full, Gate-1 present
**Branch**: `feature/user-see-each-other`
**Duration**: ~4h

---

## Loop Metadata

| Field        | Value                                      |
| ------------ | ------------------------------------------ |
| Start Commit | `37da0a503f474351779a9af2f62a69e2b70b8c55` |
| Branch       | `feature/user-see-each-other`              |
| Lead Model   | `claude-opus-5-5[1m]`                      |

---

## Loop State (Internal)

| Field                   | Value                                                                                             |
| ----------------------- | ------------------------------------------------------------------------------------------------- |
| Phase | `complete` |
| Implementer             | `implementer`                                                                                     |
| Implementing Specialist | `client`                                                                                          |
| Tier                    | `full`                                                                                            |
| Iteration               | `1`                                                                                               |
| Security                | `security`                                                                                        |
| Test                    | `paired-test` (paired; replaces test slot)                                                        |
| Observability           | `observability`                                                                                   |
| Code Quality            | `code-reviewer`                                                                                   |
| DRY                     | `dry-reviewer`                                                                                    |
| Operations              | `operations`                                                                                      |
| Semantic Guard          | `semantic-guard` (included: e2e bus is a window-exposed sink; R-18/notice; reason-text rendering) |

---

## Task Overview

### Objective

See task prompt (story 3 task 17, C-C1).

### Scope

- **Service(s)**: client (`packages/sdk-svelte`, `packages/web-app`, possibly `packages/sdk-core`)
- **Schema**: No
- **Cross-cutting**: No (client-only; env-tests run against Kind)

### Debate Decision

NOT NEEDED — designed in story file Design §Wire freeze.

---

## Cross-Boundary Classification

One path or glob per row (the scope guard matches the first cell).

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `packages/sdk-core/src/**` | Mine | — |
| `packages/sdk-svelte/**` | Mine | — |
| `packages/web-app/src/**` | Mine | — |
| `packages/web-app/README.md` | Mine | — |
| `packages/web-app/playwright.config.ts` | Mine | — |
| `packages/web-app/tests/bundle-content.test.ts` | Mine | — |
| `packages/web-app/tests/configEnv.test.ts` | Mine (paired with test) | — |
| `packages/web-app/tests/videoPlan.test.ts` | Mine (paired with test) | — |
| `packages/web-app/tests/videoBusShape.test.ts` | Minor-judgment (contract tests) | test |
| `packages/web-app/tests/slotOracle.test.ts` | Minor-judgment (oracle narrowness pin) | test |
| `packages/web-app/tests/videoEvidence.test.ts` | Minor-judgment (camera-branch tests) | test |
| `packages/web-app/e2e/videoBusShape.ts` | Minor-judgment (fold state lists; `parseSendCounts` / `parseReceiveDrops`) | test |
| `packages/web-app/e2e/slotOracle.ts` | Minor-judgment (fold the two state lists, kept narrow) | test |
| `packages/web-app/e2e/videoEvidence.ts` | Minor-judgment (named camera-occupant branch) | test |
| `packages/web-app/e2e/*.ts` | Mine (paired with test): 9 new specs, the video partial-connectivity case, story-2 specs touched for renamed testids, harness helpers | — |
| `packages/web-app/e2e/README.md` | Mine (paired with test) | — |
| `scripts/layer7.sh` | Minor-judgment (BROWSER_E2E_TIMEOUT default 600 -> 1500, MEASURED ~556s suite; layer7.test.sh derives the value, no edit needed) | infrastructure |
| `scripts/dev-web.sh` | Minor-judgment (VIDEO DEMO comment + one banner echo line: tick Camera, Start media) | infrastructure |
| `docs/runbooks/devloop-validation.md` | Minor-judgment (§6.7 restatement of the browser timeout default) | operations |
| `docs/specialist-knowledge/client/INDEX.md` | Mine | — |
| `docs/observability/metrics/client.md` | Mechanical (one-line attribution: the video read-back is multi-party-see.spec.ts's) | observability |
| `docs/user-stories/2026-09-21-hear-each-other-manual-test-plan.md` | Mechanical (`start-media` rename, media-status / slot copy) | test |
| `proto/dark_tower/signaling/v1/signaling.proto` | Not mine, Domain-judgment (GSA; `ServerMuteRequest.reason` comment only) | protocol |
| `crates/mc-service/src/webtransport/connection.rs` | Not mine, Minor-judgment (doc comment restating the stale proto claim) | meeting-controller |

---

## Planning

**Mechanism restatement.** The task is one rule applied at four surfaces: _every media fact a user or a test reads is a projection of exactly one SDK-owned signal, delivered at the granularity of the thing it describes_ (per slot, per participant, per kind). Instances: (a) the grid/rows (per-slot and per-participant reactive cells); (b) the R-18 notice (a latched projection of `captureResumedAfterServerMute`, per kind); (c) the e2e bus (sampled projections of SDK counters plus render-surface lumas); (d) R-19 self view (a projection of the camera gate's failure latch). Same-owner siblings found by the wider class, completed here: per-participant server-mute cells replace the whole-map `serverMutes` (every row re-rendered on anyone's mute: same defect class); the slot list today re-renders on every `StreamAssignments`; `errorText.ts`'s `PUBLISH_KIND_NOT_SUPPORTED_FIX` copy is stale since C-A2 (video publishes now). No out-of-owner siblings.

### 1. SDK additions (minimal, counts and closed tokens only)

- `VideoSlotEvidence.framesDecoded` (cumulative per slot; G1). `framesRendered` is counted at the render surface in `VideoTile` (paired-test G1: more independent than an SDK count).
- `KeyframeGate.producedBySource(): Readonly<Record<MediaKeyframeSource, number>>` (cumulative counts per `MEDIA_KEYFRAME_SOURCES`, the same call site as the metric) and `MeetingSession.videoSendCounts: { framesSent, keyframes } | undefined` (framesSent = `VideoEgress.framesSent`). No timings, no GoP.
- `cameraHealthChanged: { failure: MediaCaptureFailureOutcome | undefined }` session event, emitted by `VideoPublisher` when the camera gate's latched failure changes (set on failure, cleared on user mute/unmute or a successful restore). R-19 self view renders from it. Closed vocabulary (`MEDIA_CAPTURE_FAILURE_OUTCOMES`).
- Export `createCameraCapture` from the barrel (for the test-build-only `forceCameraCapture` lever, G5).
- Nothing new reaches `MetricsSink`: no new series, labels or names (@observability 3).

### 2. Svelte adapter (`sdk-svelte`)

- `MediaStore` gains **per-slot cells** `slot(slotId)` → `{ assignment ($state), render ($state VideoRenderState|undefined) }`, updated ONLY when that slot's assignment/render actually changes (field compare), and **per-participant cells** `participant(participantId)` → `{ audioServerMuted, videoServerMuted, serverMutedBy, unmuteRequested }` ($state each). Cells live in a plain (non-reactive) Map, created on first write/read, so a lookup tracks nothing but the cell. The whole-map `serverMutes`/`unmuteRequests` are REMOVED (callers migrate to cells; one home). The `slots` array and `unreachableSenderIds` stay (layout/roster-level, rarely changes).
- **Pairing view** cells `pairOf(senderId)` → `{ audioSlotId, videoSlotId }` recomputed via sdk-core `pairSlotsBySender(assignments, liveVideo)` (reuse; `liveVideo` from the `videoSlotRenderChanged` active senders the store already holds) and written only on change.
- `cameraFailure` cell; `clientMute` (audio, video) from `muteChanged` (videoMuted added).
- **R-18 notice latch** `resumeNotices`: one cell per kind (`audio`, `video`), set true by `captureResumedAfterServerMute`, cleared ONLY by `acknowledgeResume(kind)`. Store-held so a re-mount/grid rebuild cannot drop it (@security 1b). Two kinds → two independent notices; a repeat of the same kind while pending stays one pending notice (never cleared by it).
- Frames NEVER touch the store: `VideoTile.svelte` (new, exported) takes `slotId`, the `VideoReceiveView`, and an optional `onSurface(canvas, countRendered)` callback; on mount it calls `setVideoSink(slotId, sink)` where `drawFrame` sizes the canvas backing store to `frame.displayWidth/Height` (no letterbox; CSS scales) and `drawImage`s synchronously (borrow contract), detaching on destroy. Shows the canvas only when `render.kind === 'active'`; otherwise renders its children (avatar slot). A live slot with no frame yet keeps the previous rendering because the SDK keeps the previous render state until first decode (R-5); no spinner.
- `bindMeetingSession` wires `videoSlotRenderChanged`, `captureResumedAfterServerMute`, `cameraHealthChanged` into the same aggregate unsubscribe.
- **Granularity proof** (`MediaStore.test.ts`, extending the story-2 counters + positive controls): slot A's state change recomputes only A's tile derivation (B's count unchanged, A's advanced); participant X's server/client mute recomputes only X's tile+row derivations; a decoded frame (sink `drawFrame`) recomputes nothing.

### 3. Web app

- **Join view restructure** (the publish set is frozen at `MeetingSession` construction, and `bindMeetingSession` must run at component init): `JoinMeeting.svelte` keeps meeting code + **publish-set chooser** (checkboxes `publish-microphone`, `publish-camera`; initial state = the `publish` lever when set, else the SDK default `['audio']` — no restated default: read from a new exported `DEFAULT_PUBLISH` in sdk-core `config/clientConfig.ts`). Submit mounts a new `MeetingRoom.svelte` (keyed per attempt) that builds the session (+ e2e instrumentation and bus) at its init and joins. A construction refusal (`NothingToPublishOrReceive`, `PublishKindNotSupported`) is shown via `errorText()` in `last-error`; never a silent empty meeting. DOM contract kept: `meeting-state`, `participant-list`, `last-error`, `in-meeting`, `start-media` (renamed from `start-audio` everywhere: specs, fixtures, README, story-2 manual plan), `mute-toggle`/`mute-state` (audio).
- **InMeeting**: own controls — audio mute (existing), **video mute** (`video-mute-toggle`/`video-mute-state`, from SDK `muteChanged`), own server-mute indicators per kind (`server-mute-state-audio|video`), request-unmute, **self camera status** `self-camera-state` with `data-camera-state` = `live|off|muted|host_off|failed` and R-19 reason text from `cameraFailure` (`lib/participantState.ts` copy per `MEDIA_CAPTURE_FAILURE_OUTCOMES`, exhaustive Record). R-28: the VP8 media-start fault and `VideoCodecUnsupportedError` already arrive as `mediaFault`/thrown error → rendered via existing `media-fault`/`media-error` (errorText); assertion added.
- **Video grid** `video-grid`: one `video-slot-{id}` tile per declared video slot (ids from `session.receiveSlots`: N..N+M-1), `data-slot-state` = raw wire token (or `awaiting-assignment`), `data-rendered` = `video|avatar|empty`, `data-participant-id` (from avatar's named sender or the verified live sender via the roster), `data-paired-audio-slot`. Tile content: participant name, their paired audio status inline (heard / muted / not in an audio slot) so audio+video read as ONE participant (R-3, paired by sender id via the pairing cell, never slot index), and the four indicators. Audio rows (`slot-{id}`, existing testids) gain `data-paired-video-slot`.
- **Four indicators** per participant (tile and roster row): `mute-{kind}-{client|server}-{participantId}` with `data-muted` true/false/unknown; visibly distinct text + glyph per indicator. Client mute of others from their slot's `avatar_client_muted` (tri-state as today, now per kind); server mute per kind from the participant cell.
- **Host affordance** per kind: `server-mute-audio-{pid}` / `server-mute-video-{pid}`, shown only under the existing host predicate (`session.isHost` or the test lever); label from wire state; MC is the authority (no new "assume host" path).
- **R-18 notice** `resume-notice-{kind}`: `role="alertdialog"` + `aria-live="assertive"`, fixed above the grid (`position: fixed`, high `z-index`), text "The host turned your camera/microphone back on"; buttons `resume-notice-mute-{kind}` (calls `session.setVideoMuted(true)`/`setAudioMuted(true)` at once, then acknowledges) and `resume-notice-ack-{kind}`. No timeout, no Escape/click-outside close, no prop/setting/env/query to disable.
- **`lib/slotState.ts`**: final copy, still `Record<SlotStateToken,string>` (no default, no index signature). Avatar copy is a function of the resolved participant name: `avatarText(token, name)` via an exhaustive `Record<AvatarSlotState, (name)=>string>` ("{name} turned their camera off", "{name}'s camera was turned off by the host", "{name} is not sending video"); unavailable "this meeting allows fewer video slots"; fewer sources; never-emitted states fail-closed "unexpected state from the meeting controller: …". Names via `nameForSenderId` (reuse), server-muted wording reuses/extends `serverMutedByText`. Names truncated to a bound (`MAX_DISPLAY_NAME_CHARS`) before render, text interpolation only, never `{@html}`.
- **`lib/session.ts`**: `buildMeetingSession(config, e2e, levers, publish)`; `forceCameraCapture` lever (test-levers build only) injects `createCameraCapture`.

### 4. E2E bus (all inside the existing `if (__E2E_HOOKS__)` body of `installE2EHooks`; explicit allowlist projections, no spreads)

Field by field (new/changed):

- `videoSlots` — `{ type:'videoSlots', slots: [{ slotId:number, mcState:SlotStateToken, rendered:'video'|'avatar'|'empty', renderedSenderId?:string (stringified VERIFIED key-id sender of the last rendered frame; sender component only, never a key id), framesDecoded:number (SDK), framesRendered:number (tile paints), lumas?: number[24] (render canvas, only while rendered==='video' && framesRendered>0) }] }`. A slot is OMITTED until MC has named it (G2). No timestamp. Sampled on the 250 ms tick.
- `sendCounts` — `{ type:'sendCounts', video: { framesSent:number, keyframes: { directive_start:number, request:number, abandonment:number } } }` (G3; keyed by kind, never stream number; closed cause set = `MEDIA_KEYFRAME_SOURCES`). Present only once the video publisher exists. Audio send count stays on `mediaFrameCounts.framesSent`.
- `muteState` gains `videoMuted:boolean`.
- `receiveLayers` unchanged; its existing `drops` rows `{slot, senderId|null, reason (RejectReason closed vocabulary), count}` get a strict `parseReceiveDrops` and are used in failure diagnostics (G4).
- `sendTracks`, `videoCaptureSource`, `slotAssignments` (with `kind`) unchanged.
- Never on the bus: key ids, generations-in-keyid, salts, bytes, sizes, GoP, keyframe timings, per-frame timings, roster/identity keys, names beyond the existing roster projection.
- **Contract direction (DRY 3):** `e2e/videoBusShape.ts` stays the one contract. `src/__tests__/e2eBus.test.ts` round-trips every new projection through its strict parsers (`parseVideoSlots`, `parseSendCounts`, `parseSendTracks`, `parseVideoCaptureSource`, `parseReceiveLayers/Drops`), so the app cannot drift from it. `BUS_SLOT_STATES`/oracle lists folded into one list (`satisfies readonly SlotStateToken[]`).
- **Lumas (security 4):** the render-surface registry and `getImageData` sampler live in `e2eAnalysis.ts` inside `if (__E2E_HOOKS__)`; `VideoTile`'s `onSurface` is wired by the web app to a function that is a no-op outside the define. No `willReadFrequently` anywhere. README central-half Y formula over the whole backing store (no letterbox by construction).
- **R-30 vs R-32 reconciliation (security 3):** R-32 is the specific rule: the unpacked, meeting-scoped `sender_id` per slot (stringified) is allowed on the TEST-BUILD bus (story-2 `receiveLayers` precedent); the packed key id, generation, stream number never.
- **Bundle test:** FORBIDDEN gains `videoSlots`, `sendCounts`, `getImageData`, `forceCameraCapture`, `testPattern` (buildKnobs key); each with the existing positive-control build.

### 5. Browser env-tests (Layer 7), paired with test

Global: `SUITE_VIDEO_RECEIVE_SLOTS` stays 0 so the six story-2 specs ARE the M=0 audio-only reruns, unchanged; each already asserts declared M=0 at runtime (`expectDeclaredReceiveSlots`). Video specs set N/M/publish per context via `leversForPlan` and assert their effective N/M. `DT_TEST_PATTERN=1` added once to the webServer env; `expectBuildKnobs` gains a `testPattern` knob (bus `buildKnobs.testPattern`). Shared helpers (reuse, extend): `cohortContexts.joinPlanMember(member, code, plan)` (generalises `joinMember`: tone read only for audio publishers, pattern announcement for video publishers); `fixtures.setMuteViaUi(page, muted, kind='audio')` (generalised, not copied); `fixtures.setServerMuteViaUi(host, subject, kind, muted)`; `fixtures.waitForVideoSlotState(page, slotId, pred)` (one liveness helper; failure text = liveness + last bus state + diagnostics); new `e2e/failureDiagnostics.ts` `withVideoDiagnostics(baseline, fn)` (try/catch rethrow, dump never throws, labelled MH budget rejection / pending edges / client fault) reusing `diagnoseMissingSender` + new mcMetrics reads (`mh_media_edges_pending`, `mh_media_keyframe_requests_sent_total` by trigger, `mh_media_keyframe_request_write_failures_total`, `mc_media_slot_states_total{media_kind="video"}`, `mc_media_unreachable_senders_total`; names from the catalogs). Joins serialised; expectations only from `SlotOracle` driven in spec order; expected sender from join binding; windows via `sampleWindow` with paired movers.

- `multi-party-see.spec.ts` (R-1, R-5, R-32, R-33, R-29): 4 members, all `both`, N=3, M=3; every receiver × every sender: video three layers (`expectVideoSlots`) + rendered state + audio tone (`expectHearsAtAllLayers`); `expectPatternAnnounced`; R-29: baselines before first join, `received_video` (+ `sent_video`) rises, `received_kindless` not above.
- `publish-mixes.spec.ts` (R-2, R-3, R-4): A audio-only, B video-only, C both, D receive-only (N=3,M=3), D chooses its set through the DOM chooser; receive-only and audio-only appear as `avatar_no_video` in video slots; video-only has no audio slot; publish-nothing at N=0/M=0 refused in the DOM with the errorText refusal (no `joined` event); an N=0 receiver and an M=0 receiver each work.
- `video-over-subscription.spec.ts` (R-1): the cap is read INDEPENDENTLY of MC's report: `MC_MAX_VIDEO_RECEIVE_SLOTS` from `infra/services/mc-service/config.env` (the Kind overlay does not patch it; same `configEnv` reader path as AC's limit, new non-negative-int reader beside `readVideoProfile`); assert bus `serverCap` EQUALS it; declare cap+1; the tail slot is `unavailable` / rendered `empty`, the join succeeds, the others render.
- `client-mute-video.spec.ts` (R-17, R-15): video mute → muted client's `cameraTrackLive` false AND `sendCounts.video.framesSent` flat (mover: its audio framesSent), every receiver `avatar_client_muted`; audio mute → mic open, audio send flat, video flowing; unmute resumes (layers 1-3). One context in CAMERA mode (`forceCameraCapture` lever) with a fixture-owned `getUserMedia` init-script recorder asserts real `readyState` live→ended→new live (G5 preferred).
- `server-mute-video.spec.ts` (R-16, R-18, R-15): host clicks the per-kind DOM affordance; muted client's camera ends, every receiver `avatar_server_muted`, audio flows; server unmute resumes capture and the R-18 notice is visible and survives a re-render; inline mute button client-mutes; with the participant client-muted first, server unmute does NOT resume (camera false + video framesSent flat with audio mover) and no notice.
- `late-joiner-video.spec.ts` (R-10, R-33): video flowing among 3; a 4th joins; it passes layers 1-3; the publisher's `sendCounts.video.keyframes` total advanced across the join; no interval key in the live profile/directive (asserted from the dumped profile env).
- `kek-rotation-video.spec.ts` (R-14): a member leaves; remaining receivers' `kekGeneration` advances; a `sampleWindow` spanning the rotation shows layer 2 advancing and no layer-1 fall-back. Leaver lockout: cross-referenced to `sdk-core/src/media/lifecycle/__tests__/videoReceiver.test.ts` "a video LEAVER cannot open post-rotation video" (client integration test, R-14's stated home).
- `video-codec-unavailable.spec.ts` (R-28 UI): a test-owned `addInitScript` makes `VideoEncoder.isConfigSupported` report unsupported for that context (no browser flag, no app lever); joining with camera published surfaces the SDK's VP8 media-start fault in `media-fault`/`media-error`; never an audio-only degradation. The SDK's join-time probe calls `VideoEncoder.isConfigSupported` (`media/setup/vp8.ts:probeVp8Support`), and `startMedia` then refuses media as a WHOLE (throws `VideoCodecUnsupportedError` + fatal `mediaFault` before any pipeline is built): asserted as joined + `media-fault`/`media-error` visible + no `mute-toggle` + no `sendTracks`/`sendCounts` bus events. Positive control in the same spec: a second context without the override publishes video and its `sendCounts.video.framesSent` advances.
- `partial-connectivity.spec.ts` gains a second test (R-33): same partition with M=2. Unreachability is read from `unreachable_sender_ids` ONLY (`judgeUnreachable(oracle.expected(B).unreachable, bus)`; `source_unreachable` is a never-emitted slot token and is never waited for); B's video slots match the oracle (C consumes no slot); across one `sampleWindow` no sample has `renderedSenderId === C` and no `(slot, C)` layer row advances, with B's slot from A advancing as the mover. The original M=0 test unchanged.
- **Review refinements (Gate 1, paired-test P4-P7):** R-18 non-suppressibility is asserted in the DOM: after Escape, a click outside the dialog and a real re-render (a member joins, the grid rebuilds) the notice is still visible; it disappears only after `resume-notice-ack-*`/`resume-notice-mute-*`; after the inline mute, camera false + video framesSent flat in a window with the audio mover. Late joiner: the profile-dump parse is proven non-vacuous (`readVideoProfile` finds the five `MEDIA_VIDEO_*` keys) before the interval-key absence is asserted; keyframe evidence = publisher `keyframes` total delta >= 1 between a pre-join and a post-first-render sample, no steady-state flatness. The camera-mode member is excluded from `judgePatternAnnouncement`/layer 3 by a named branch; the `getUserMedia` recorder (fixture-owned, e2e only) records VIDEO tracks only, asserts >= 1 track before the mute, then old `ended` + a NEW `live` track after unmute. `waitForVideoSlotState` uses ONE named liveness constant.
- **DRY refinements:** names are truncated with sdk-core's existing `MAX_DISPLAY_NAME_LENGTH` (exported from the barrel), not a new bound; `DEFAULT_PUBLISH` lives beside `DEFAULT_RECEIVE_*_SLOTS` in `config/clientConfig.ts` and `slotPlan.ts` consumes it (no `['audio']` literal left); `participantState.ts` migrates to the participant cells in the same change. Observability conditions: every PromQL string stays in `mcMetrics.ts`; `withVideoDiagnostics` rethrows the ORIGINAL error with the dump appended, a dump failure reported inside the dump text.
- **Code-quality refinements (Gate 1):** ONE exhaustive copy table: `SLOT_STATE_TEXT: Record<SlotStateToken, (ctx: { name: string }) => string>` (avatar entries use the name; others ignore it), `AvatarSlotState` derived from the SDK (`Extract<…, \`avatar_${string}\`>`), `unspecified` gets the same fail-closed "unexpected state" copy and no media. The folded bus/oracle state list is derived from an exhaustive `Record<SlotStateToken, true>`-style source (Object.keys), so a new wire token breaks it at compile time. The ORACLE's `ExpectedSlotState` stays narrow (never `switch_pending`/`withheld_by_congestion`/`source_unreachable`/`unspecified`), pinned in `slotOracle.test.ts`; the MC cap reader's comment states it fails closed if a Kind patch ever changes the cap. Store cells: lifetime = the MediaStore = one MeetingRoom attempt, bounded by roster participants and N+M slots; the factory only constructs (never assigns an existing cell) and a test reads a never-written cell inside a `$derived`then writes it.`start-audio`renamed`start-media`everywhere (incl. story-2 fixtures).`buildMeetingSession(config, { e2e, levers, publish })`. `setMuteViaUi(page, kind, muted)`with`kind`REQUIRED.`createCameraCapture`barrel export marked`@internal`naming its one test-lever consumer. Camera state: one exported union`SelfCameraState` + exhaustive copy Record; e2e reads the type.
- **Lead scope addition (fix, don't defer):** the `ServerMuteRequest.reason` proto comment claims the reason is "echoed to another participant"; `ParticipantMuteUpdate` has no reason field and MC neither forwards nor logs it. Comment reworded to the truth (accepted, not relayed; the truncate-before-logging and not-trusted-markup obligations kept for any future consumer), wording confirmed with @protocol; the two sibling restatements (MC `connection.rs`, SDK `SignalingClient.ts`) corrected in the same change. `docs/API_CONTRACTS.md` carries no such claim (checked). Consequence for the UI criterion: no reason text reaches the client, so "server-muted reason truncated and rendered as text" applies to the text this task renders about a server mute: the resolved display names (`serverMutedBy` via the roster, and every participant name), truncated to `MAX_DISPLAY_NAME_LENGTH` and text-interpolated only. The SDK keeps sending `reason` empty; no free-text reason input is added.
- No `test.skip` anywhere. README catalog rows per spec with R-numbers.
- **Wall clock (ops 1):** measure the full suite from `layer-7-browser-e2e.log`, then set `BROWSER_E2E_TIMEOUT` in `scripts/layer7.sh`, `scripts/layer7.test.sh` and runbook §6.7 together, and update README §Budgets.

### Risks

- Kind capacity for 4 contexts × VP8 320×180@15 encode+decode ×3: measured on first run; escalate to @team-lead if CPU-bound rather than shrink the cohort silently.
- MC/MH defects found by the oracle are findings for their owners, never oracle edits.

---

## Pre-Work

None

---

## Gate 1 — Plan Confirmations

| Reviewer                           | Plan Status |
| ---------------------------------- | ----------- |
| Security                           | confirmed   |
| Test (paired)                      | confirmed   |
| Observability                      | confirmed   |
| Code Quality                       | confirmed   |
| DRY                                | confirmed   |
| Operations                         | confirmed   |
| Semantic Guard                     | confirmed   |
| Protocol (GSA, proto comment hunk) | confirmed   |

---

## Implementation Summary

- **SDK (sdk-core)**: per-slot `framesDecoded` on `VideoSlotEvidence`; `KeyframeGate.producedBySource()` (same call site as the metric); `VideoPublisher.sendTotals()` / `cameraFailure` and the `cameraHealthChanged` session event (R-19 reason); `DEFAULT_PUBLISH` beside `DEFAULT_RECEIVE_*_SLOTS` (consumed by `slotPlan.ts`); barrel exports `MAX_DISPLAY_NAME_LENGTH` and `createCameraCapture` (@internal, one test-lever consumer). No new metric names, labels or series. (Two first-draft `MeetingSession` getters were moved onto `VideoPublisher`, reachable as `session.video`, to keep `MeetingSession` under the dt-guard declaration-size cap.)
- **Adapter (sdk-svelte)**: `MediaStore` per-slot / per-participant / pairing cells in a `CellRegistry` whose readers never construct (Svelte does not track a source created inside the reading reaction — found and fixed under the granularity test); roster events pre-create participant cells; R-18 per-kind latch; `VideoTile.svelte` (canvas `VideoSink`, backing store sized to the frame, `onSurface`). Whole-map `serverMutes` / `unmuteRequests` and `snapshots.ts` removed. Granularity proven by counted derivations: slot change -> only that tile, mute -> only that participant, frames -> nothing.
- **Web app**: `JoinMeeting` (code + publish-set chooser seeded from `DEFAULT_PUBLISH` / the lever) mounts one `MeetingRoom` per attempt; `InMeeting` (audio rows + M-tile video grid paired by sender, video mute, own per-kind server-mute indicators, R-19 self camera with reason, R-28 fault); `VideoSlotTile`, `MuteIndicators` (four distinct indicators), `ResumeNotice` (alertdialog, store latch, inline mute, no other dismissal); `ParticipantRow` per-kind host buttons; one exhaustive name-aware copy table in `slotState.ts`; bounded names; `forceCameraCapture` test lever; e2e bus `videoSlots` / `sendCounts` / `muteState.videoMuted` / `buildKnobs.testPattern`, render-surface lumas in `e2eAnalysis.ts`; bundle FORBIDDEN list extended with positive controls.
- **Browser env-tests**: 9 new specs + a video partial-connectivity test, on a shared `videoScenario.ts` (oracle driven in join order, MC caps read independently), `failureDiagnostics.ts`, `joinPlanMember` (chooser via DOM, effective N/M/publish asserted), strict `parseSendCounts` / `parseReceiveDrops`, camera-mode `getUserMedia` recorder. Story-2 specs remain the M=0 reruns. `BROWSER_E2E_TIMEOUT` 600 -> 1500 s from the measured 556 s suite.
- **Comment fixes (lead scope addition)**: `ServerMuteRequest.reason` proto comment + MC / SDK restatements.

## Files Modified

See `git status` (67 tracked files modified, 1 deleted, 22 new); the classification table lists every path.

## Devloop Verification Steps

- `./scripts/layer-fast.sh`: green (all six layers OK).
- sdk-core 1714, sdk-svelte 33 (browser), web-app 157 (browser) + 461 (node) unit/component tests green.
- **Layer 7 browser suite on the live Kind cluster** (targeted runner mirroring `scripts/layer7.sh`'s env: ports.json URLs, a provisioned per-run org, the deployed `media-codec-profile` dump): **29/29 passed in 9.1 min (557 s)**, twice, and again after the Gate-3 fixes (**29/29, 547 s**). Every new spec passed; story-2 specs unchanged and green. Log kept at `/tmp/c1/layer7-browser-evidence.log` (container-local).

---

## Code Review Results

| Reviewer       | Verdict | Findings | Fixed | Deferred | Notes |
| -------------- | ------- | -------- | ----- | -------- | ----- |
| Security | RESOLVED-FIXED | 1 | 1 | 0 | S1 host unmute label copy |
| Test (paired) | RESOLVED-FIXED | 5 | 5 | 0 | T1-T5 (concurrent rotation sampling, codec-unavailable window, named liveness bound, residual documented, avatar copy asserted) |
| Observability | RESOLVED-FIXED | 1 | 1 | 0 | client.md catalog read-back attribution |
| Code Quality | RESOLVED-FIXED | 7 | 7 | 0 | SSoT/exhaustiveness (slotIds, renderedKind, MEDIA_KINDS, keyframe Record, lazy lumas, rename, single roster scan) |
| DRY | RESOLVED-FIXED | 7 | 6 | 0 | F5 withdrawn (deliberate independent luma grid, reason at E2E_LUMA_GRID_*) |
| Operations | RESOLVED-FIXED | 2 | 2 | 0 | bounded diagnostics page reads; dev-web banner camera step |
| Semantic Guard | CLEAR | 0 | 0 | 0 | native SAFE |
| Protocol | CLEAR | 0 | 0 | 0 | GSA reason-comment hunks |

---

## Accepted Deferrals

- (none surfaced in this devloop)

---

## Rollback Procedure

1. Start commit: `37da0a503f474351779a9af2f62a69e2b70b8c55`
2. `git diff 37da0a50..HEAD`
3. `git reset --soft 37da0a50` / `--hard`
4. **Safe-revert unit**: TBD

---

## Issues Encountered & Resolutions

None yet

## Lead Notes

- INDEX/review-protocol injection: each teammate was instructed to Read its `docs/specialist-knowledge/{name}/INDEX.md` and `review-protocol.md` in full as its first action (content-identical to inline injection), and the implementer reads the task-17 prompt file verbatim.

## Gate 2 — Full Validation (Lead)

`DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` on the final tree: attempt 1, `TOTAL_RESULT=OK` (no FMT_APPLIED).

| Layer | Result | Duration (s) |
|-------|--------|--------------|
| 1 | OK | 5 |
| 2 | OK | 4 |
| 3 | OK | 156 |
| 4 | OK | 157 |
| 5 | OK | 1 |
| 6 | OK | 5 |
| 7 | OK (`browser-e2e-passed`, Rust env-tests + full browser suite incl. all new video specs and story-2 M=0 reruns) | 1416 |

The fast check (`layer-fast.sh`) was green before review. The implementer's targeted Kind runs (29/29 browser, twice before review and once after the fixes) were supporting evidence only; the authoritative run is the `layer7.sh` run above.

## Gate 3 Notes (Lead)

- Scope addition: the Lead folded in security's Gate-1 side observation (a stale `ServerMuteRequest.reason` comment, GSA). The `protocol` owner was pulled into planning and review (CLEAR), and security covered the GSA intersection.
- Semantic-guard and protocol gave their CLEAR verdicts before the review-fix round. The fix round touched no credential or GSA surface (copy, SSoT refactors, e2e helpers, one SDK diagnostics field).
- DRY F5 was withdrawn by the reviewer, not deferred: the e2e luma sampler keeps its own grid geometry on purpose, so it measures independently of the generator.
- Implementer incident: a repo-wide `nx format:write` reformatted about 680 unrelated files. All of them were reverted, and only this task's files remain in the diff. This main.md was reflowed by prettier; its content is intact.
