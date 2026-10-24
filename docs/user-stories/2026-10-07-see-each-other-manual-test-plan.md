# Manual Test Plan — See Each Other, the Video Demo (pre-close validation)

**Story**: [2026-10-07-see-each-other](2026-10-07-see-each-other.md) (R-34 and R-28; story task 21)
**Date written**: 2026-10-24
**Status**: Written. The automated equivalents and the R-28 capacity evidence were run on
2026-10-24 (see [Capacity evidence](#capacity-evidence) and [Run record](#run-record)). The human
pass on Windows Chrome is **NOT RUN — pending before `/close-story`**.
**Purpose**: Before `/close-story`, a human covers what automation structurally cannot see:
- **Seeing**, on Windows Chrome over mirrored networking. The browser suite reads wire tokens, the
  dev-only event bus and the luma blocks of a test pattern; it never looks at a tile the way a
  person does.
- A **real-camera pass** at the production codec profile, with headphones.
- **The camera light**, which no automation can observe.
- The **rendered UI**: tiles, avatars, the four mute indicators and the server-unmute notice.

The plan also walks the dashboards, which only exist as a whole when someone reads them. It gives a
human eye to R-5, R-7, R-15 through R-19, R-26, R-27 and R-35. Every step names its automated
equivalent (file and test title, as written in the tree), so a human-only failure can be told
apart from a regression the suite should already have caught.

References:

- `scripts/dev-web.sh --help`, block "VIDEO DEMO — SEE AND HEAR": the one-machine video bring-up,
  the test-pattern camera, camera contention, and why not a second machine.
- `docs/runbooks/client-dev-local.md`: bring-up §3, the media triage ladder §4.5, and the video
  entries F21–F27. F12–F19 are the audio ladder.
- `packages/web-app/e2e/README.md` §"What the video specs assert": the automated scenarios this plan
  mirrors.
- `docs/runbooks/mh-incident-response.md` Scenarios 18–23 and `docs/runbooks/mc-incident-response.md`
  Scenarios 8, 16, 18, 21, 22 and 23 (titles in [Triage on failure](#triage-on-failure)).
- `docs/runbooks/mh-deployment.md` §"Story 3 video: the 24-hour watch list, in the order things rise
  first", §"Rollout With Media Flowing" and §"Operator levers for media: video off, and nothing
  else"; `docs/runbooks/mc-deployment.md` §"Video-off lever".
- The story-2 plan, [2026-09-21-hear-each-other-manual-test-plan.md](2026-09-21-hear-each-other-manual-test-plan.md):
  Phase 11 re-runs its Phases 1–4.

**Result convention.** Mark each checkbox and note anomalies inline under the step. A failed step
is recorded as FAIL with what was seen, never softened into a pass-with-notes. Record the triage
pointer you used. A failure is fixed in its owning task's area through `/devloop`, or raised to the
lead. It is never papered over in this text.

**Cite, don't restate.** This plan writes no literal N, M, cap, codec value, keyframe interval or
ceiling in its steps. There is one exception: Phase 1's worked grid, which holds only at the
launcher defaults it cites. The Capacity evidence and Run record sections record values *with the
run*.
- N and M are what `dev-web.sh` exports (`VITE_DT_AUDIO_RECEIVE_SLOTS`, `VITE_DT_VIDEO_RECEIVE_SLOTS`)
  and echoes in its preflight.
- MC's caps are `MC_MAX_AUDIO_RECEIVE_SLOTS` and `MC_MAX_VIDEO_RECEIVE_SLOTS` in
  `infra/services/mc-service/config.env`.
- The codec profile is the deployed `media-codec-profile` ConfigMap. Read it back with
  `kubectl -n dark-tower get configmap -l component=media-codec-profile -o yaml`, and pick the
  generation the mc and mh pods reference.
- The minimum keyframe interval is `mc_media_video_keyframe_min_interval_seconds`.

Record the values *with the run*, not in the steps.

**No wall-clock pass/fail.** No step asks you to judge a latency. Join-to-first-video and
assignment-to-first-render are *observed* on the open-objective panels and recorded, never gated
(`docs/observability/slos.md` §"Story 3 open objectives"). Recovery is observed as state returning
to zero, never timed.

**No insecure browser flag, anywhere.** The test pattern replaces the camera in-app, with no
`--use-fake-*` flag and no camera prompt (except the Phase 9 pre-join self-check, which opens the
real devices on purpose). MC/MH trust is the dev-cert fingerprint pinning, never a bypass.

**Evidence artifacts stay out of the repo.** Traces and HARs from these runs contain live meeting
and user tokens (`packages/web-app/e2e/README.md`: "they stay gitignored, local-only and are never
promoted to CI artifacts or attached to issues"). Do not commit, attach or paste them. Overlay
values may be quoted only as kind plus slot id, never with anything that names a sender.

---

## What the demo proves — and what it does not

**Proves (routing and pairing, publish mixes, the four mutes, watched-only capture, rotation with
video, the operator lever):**

- **Routing and pairing.**
  - MC fills each receiver's audio and video slots per R-3, in join order, from the participants it
    shares a connected handler with. The handlers forward exactly those edges.
  - The client pairs a participant's tile with that participant's audio row by publisher, never by
    slot index.
- **Attribution by content.**
  - A test pattern in the tile naming B came from the sender whose identity key MC's roster lists
    for B. The rendered frame's verified key-id sender names it, and live video slots carry no MC
    publisher name (R-5).
  - This holds **given an honest MC**. It is not proof of who B is.
- **The four mute controls** behave as R-15 to R-18 state, per kind and composed, including the
  host-resumes-capture notice.
- **Watched-only capture.** A camera that nobody holds in a video slot is off (R-8).
- **Rotation on leave with video flowing**, and a late joiner's clean first picture (R-10, R-14).
- **The video-off lever** (R-26) turns video off and leaves audio alone.

**Does not prove:**

- **The crypto.** Every participant runs the identical TypeScript SFrame stack, and MH is keyless,
  so an error that is consistent on both sides is invisible here: key schedule, framing, AAD, or the
  verify/decrypt behind attribution. The cross-language frame-v3 vectors (`crates/media-vector-gen`,
  `proto/test-vectors/frame-v3.vectors.json`, story task 6) and the pinned SFrame-WG anchor under
  `proto/test-vectors/external/sframe-wg/` remain the **only** independent check of it.
- **Who the other participants are.** Roster keys are trust-on-first-use, as distributed by MC.
  Nothing here authenticates participants to each other.
- **Privacy, stated plainly: a host action can resume your camera or microphone unless you have
  muted it yourself** (R-18 / ADR-0036 §5). The resume notice makes it visible; it does not prevent
  it.
- **The R-18 by-design residual.** If no server mute was recorded on the client, a capture resume
  shows no notice. That is by design. Its automated equivalent is
  `packages/sdk-core/src/session/__tests__/serverMuteResume.test.ts`.
- **The R-18 ordering fail-safe.** The notice fires in either arrival order and when the self update
  is lost. That is automation-only; do not try to reproduce the ordering by hand. Automated
  equivalent: `serverMuteResume.test.ts`, tests "fires once when the unmute update arrives BEFORE
  the resume", "fires once when the resume arrives BEFORE the unmute update" and "fires once when
  the unmute update is LOST".
- **A patched or non-cooperating client under server video mute.** A browser here always obeys a
  withdrawal. MH's ingress enforcement against a client that keeps sending is proved only by the
  Rust env-tests:
  - `crates/env-tests/tests/35_mc_server_mute_teardown.rs::test_video_server_mute_withdraws_drops_a_patched_publisher_and_unmute_restores_it`;
  - MH-A1's `crates/env-tests/tests/37_mh_stream_attribution.rs::test_per_stream_attribution_on_the_datagram_path`,
    phase (c), the patched client at `DROP_AT_INGRESS`.

  See Phase 4, step 4h.
- **Leaver lockout.** The leaver's context is gone, so no step can watch it fail to open
  post-rotation video. That is asserted by
  `packages/sdk-core/src/media/lifecycle/__tests__/videoReceiver.test.ts`, "a video LEAVER cannot
  open post-rotation video (positive control: pre-rotation accepted)" (Phase 6).
- **Forward secrecy or end-to-end properties**, **scale, cross-machine or cross-network behaviour.**
  Every context runs on one machine against one Kind cluster. Capacity is the recorded evidence
  below, not a claim this demo makes.

---

## Phase 0 — Cold start (WSL2, the STATIC cluster)

The human pass runs against the **static** Kind topology (host ports from
`infra/kind/kind-config.yaml`), as in story 2. A devloop cluster is a different topology and
`dev-web.sh` does not target it.

```bash
unset DT_HOST_GATEWAY_IP                                                 # F8
./infra/kind/scripts/setup.sh                                            # first run: several minutes
DT_TEST_TONE=1 DT_TEST_PATTERN=1 DT_TEST_LEVERS=1 scripts/dev-web.sh --check   # preflight only
DT_TEST_TONE=1 DT_TEST_PATTERN=1 DT_TEST_LEVERS=1 scripts/dev-web.sh           # leave running
```

- `DT_TEST_PATTERN=1` replaces the camera **in-app** with a synthetic per-participant test pattern.
  It is derived from the sender id and changes every frame, so a frozen tile is visible. One
  physical webcam generally cannot be opened by several profiles (camera contention, `dev-web.sh`).
- `DT_TEST_TONE=1` replaces the microphone with a per-participant tone.
- `DT_TEST_LEVERS=1` enables the per-context test levers used in Phases 2 and 4
  (`packages/web-app/src/lib/testLevers.ts`).
- All three are **build-time** defines that accept exactly `1` (`packages/web-app/vite/testDefines.ts`,
  `resolveOptInTestDefine`). A reload does not change them; stop the server and re-run it.
- **The test pattern and the test tone exist only in dev builds.** A production build that asks for
  either fails at the gate, and no production bundle carries them. Automated equivalents:
  - `packages/web-app/tests/bundle-content.test.ts`: "a PRODUCTION build asked for the test pattern
    FAILS at the gate, never coerces it off", "a PRODUCTION build asked for the test tone FAILS at
    the gate, never coerces it off", and "production bundle excludes forbidden token %s";
  - `packages/web-app/tests/testDefines.test.ts`;
  - `packages/sdk-core/tests/bundle-content.test.ts`: "carries no test-PATTERN generator, gate or
    mode token outside type declarations (story 3 R-27)".

Checks:

- [ ] Preflight green. Every ✗ is a hard fail; stop and fix it, never work around it.
- [ ] **Copy the preflight's receive-slots line (effective N and M against MC's per-kind caps) into
      the run record.** The cohort below is P = 4 participants, A to D, unless a phase says
      otherwise.
- [ ] Preflight shows `test pattern: ON requested`, `test tone: ON requested` and `test levers: ON
      requested`.
- [ ] After the first context loads, its console `ev.filter(e => e.type === 'buildKnobs').at(-1)`
      shows the test pattern, test tone and test levers on, and `telemetrySinkActive: true`. If not,
      **relaunch the dev server**; a reload does not fix it. This is a setup error, not a product
      failure.
- [ ] Record the deployed codec profile: the ConfigMap read-back above, **not** the patch file.
- [ ] If Chrome fails but WSL2 `curl -I http://127.0.0.1:5173` works, this is F9; fix `.wslconfig`.

### Browsing contexts and accounts

- **One browsing context per participant**, each a separate **Chrome profile**. This is the
  known-good shape: the sign-in is per-context, in-memory state.
- **Each context is a DISTINCT registered account, with a distinct display name** matching its label
  (A, B, C, D; E for the late joiner, which may reuse a leaver's profile).
  - Tiles name the participant through the roster, so a shared name makes the by-content check
    vacuous.
  - Sign up per profile at `http://demo.localhost:5173` with the label as the display name, and keep
    the prefilled `demo` org (`client-dev-local.md` §3).
- **Debug panel state persists per profile.** The panel's open/closed bit is stored in
  `localStorage` (`darktower.debugPanel.v1`). It is hidden by default only on a fresh profile or
  with that key cleared. A returning profile may show it open, which is not a Phase 9 failure.
- Keep every window visible, unminimised and **uncovered**. Chrome throttles a window that other
  windows fully cover exactly as it throttles a hidden tab: timers drop to about 1/s, so a
  test-pattern publisher there sends ~1 fps (the pattern is painted on a timer; a real camera
  track is not) and every receiver shows that tile at ~1 fps. That is not a product fault. If five
  windows will not fit uncovered, launch the demo Chrome with
  `--disable-backgrounding-occluded-windows --disable-renderer-backgrounding
  --disable-background-timer-throttling` (throttling only; no security flag). Check: the
  `sendCounts` snippet below reads about the directed frame rate in every context.

Evidence available in every context (dev builds carry the E2E bus). Open DevTools → Console:

```js
const ev = window.__darktower_test__.events;
ev.filter(e => e.type === 'buildKnobs').at(-1)          // pattern / tone / levers ON?
ev.find(e => e.type === 'joined')                         // own senderId, offered mediaServers
ev.filter(e => e.type === 'receiveSlots').at(-1)          // declared N and M, serverCap per kind, publish set
ev.filter(e => e.type === 'videoSlots').at(-1)            // per video slot: mcState, rendered, frames decoded/rendered
ev.filter(e => e.type === 'sendTracks').at(-1)            // cameraTrackLive, micTrackOpen (the light, for the test pattern)
ev.filter(e => e.type === 'sendCounts').at(-1)            // video framesSent, keyframes by cause
```

`sendTracks.cameraTrackLive` stands in for the camera light while the test pattern is on. The
**physical** light is checked in Phase 10, the real-camera session.

---

## Phase 1 — See and hear by content

A signs in and **creates** the meeting, so A is the host. B, C and D join A's code **in label
order**. In the join view's publish chooser (`publish-chooser`), each ticks **Camera**
(`publish-camera`) and keeps **Microphone** (`publish-microphone`), clicks Join (`join-button`),
then **Start media** (`start-media`).

**Expected grid (R-3).** Every participant publishes both kinds and joins in turn. The grid below
holds at the launcher defaults, `DEMO_AUDIO_RECEIVE_SLOTS` / `DEMO_VIDEO_RECEIVE_SLOTS` in
`scripts/dev-web.sh`, which the preflight echoes. Each receiver's video slots therefore hold the other three participants
**in join order**, and each video slot is paired with the audio slot of the same participant.

| Receiver | Video slots, in declaration order (paired audio row = the same participant) |
|----------|----------------------------------------------------------------------------|
| A | B, C, D |
| B | A, C, D |
| C | A, B, D |
| D | A, B, C |

If the preflight's N or M differ from those defaults, derive the grid from R-3 in the
[story](2026-10-07-see-each-other.md) §Requirements (`packages/web-app/e2e/slotOracle.ts` encodes
it). Write the derived grid into the run record before joining.

- [ ] Each tile (`video-slot-<id>`) shows **showing <name>** for the expected participant. Its
      `data-slot-state` is `active`, and it is paired (`data-paired-audio-slot`) with that
      participant's audio row.
- [ ] **The pattern changes every frame.** Watch each tile; a frozen tile is a FAIL even when its
      state reads `active`. The participant's own *announced* pattern is irrelevant; the tile has to
      be live.
- [ ] Each participant hears the other three tones. Check one sender at a time with that sender's
      **Mute** (`mute-toggle`), as in story 2 Phase 1.
- [ ] No tile shows "unexpected state from the meeting controller (…)". That copy is the
      fail-closed rendering of a state the client must never get. Seeing it in any phase is a FAIL.
- [ ] No tile is a spinner, in any phase.
- [ ] Observe, do not judge: client-media *Video Time To First Render (p95)* and *Video Slot ACTIVE
      To First Render (p95)*. Record the readings. Read the second beside *Video Slot Unfulfilled
      Duration (p95)*, because it only sees slots that did render.

Automated equivalent:
- `packages/web-app/e2e/multi-party-see.spec.ts`, "every receiver sees every other sender at three
  layers and hears them (N=3, M=3)". Every video slot is checked against the test's own slot oracle
  (`e2e/slotOracle.ts`) at three layers: the rendered frame's verified key-id sender, the
  receive-layer counters, and the render-surface pattern decode with its liveness verdict. Every
  receiver hears every other sender by tone.
- Below the browser: `crates/env-tests/tests/27_mc_slot_placement.rs::test_pairing_stability_and_watched_only_video_directive`
  and `test_all_connected_everyone_hears_everyone_with_one_target_each`.

## Phase 2 — Publish mixes

Leave the meeting from Phase 1 (each context: the Create nav, which leaves gracefully). A creates a
new meeting. In join order, the chooser is set to:

| Who | Microphone | Camera | Expected at the others |
|-----|-----------|--------|------------------------|
| A | ✓ | ✓ | live tile and tone |
| B | ✓ | — (audio-only) | avatar "B is not sending video" in a video slot; tone heard |
| C | — | ✓ (video-only) | live tile; no tone, and C takes **no audio slot** |
| D | — | — (receive-only) | avatar "D is not sending video" in a video slot; takes **no audio slot** |

- [ ] The audio-only and receive-only participants show as the **"no video" avatar**
      (`data-slot-state` = `avatar_no_video`) in video slots, never as a blank tile or a spinner.
- [ ] **Audio-only holds no video publisher precedence: it never becomes a video publisher.**
  - B sits in a video slot as the "no video" avatar wherever R-3 places it. R-3's pair-completion
    rule may well place it there, because B already holds an audio slot at that receiver. That is
    correct MC behaviour, not a fault.
  - B's own `cameraTrackLive` stays false the whole time, even while B is held in a video slot.
  - No video frames are sent from B (bus `sendCounts.video.framesSent` stays 0).
  - Derive the expected occupants from R-3 (or `slotOracle.ts`) and write them in the run record.
- [ ] **Receive-only takes no audio slot.** No audio row names D, at any receiver. D still sees and
      hears the others.
- [ ] **The neither-publish-nor-receive join is refused, generically.** E opens a new profile. Before
      its join view mounts, it sets this in the console:

      ```js
      window.__dt_test_levers__ = { audioReceiveSlots: '0', videoReceiveSlots: '0' }
      ```

      It then unticks both chooser boxes and clicks Join.
  - `last-error` shows `data-refused="true"`, with copy that says to choose the microphone or the
    camera.
  - E never reaches `in-meeting`.
  - No one else's roster ever shows E.
  - MC's wire answer to any media-declaration refusal is the generic "Invalid join request"
    (`crates/mc-service/src/errors.rs`). The browser refuses locally first, so a human sees only the
    local message.
- [ ] mc-media *Join Refusals - Story 3 Media Join Values*. A patched join at MC would step it;
      E's local refusal does not.

Automated equivalent:
- `packages/web-app/e2e/publish-mixes.spec.ts`:
  - "audio-only, video-only, both and receive-only: avatars for the non-video, by content for the
    rest";
  - "a join that would neither publish nor receive is REFUSED, visibly, and never joins";
  - "an N=0 receiver (video only) and an M=0 receiver (audio only) each work".
- MC's generic refusal:
  - `crates/env-tests/tests/27_mc_slot_placement.rs::test_publish_mixes_render_per_kind_and_refusals_are_twinned`;
  - `crates/mc-service/tests/video_slots_integration.rs::join_refusals_are_counted_generically_and_each_has_an_admitted_twin`.

## Phase 3 — Over-subscription, truncation and under-fill

**Truncation.** Restart the dev server with `VITE_DT_VIDEO_RECEIVE_SLOTS` above MC's video cap
(it is one setting per dev server, R-1), and keep the other knobs. Alternatively, set the
per-context lever in one context before its join view mounts:
`window.__dt_test_levers__ = { videoReceiveSlots: '<cap + 1>' }`, with the cap taken from
`MC_MAX_VIDEO_RECEIVE_SLOTS`.

- [ ] The over-declaring participant **joins** (`meeting-state` = `joined`).
- [ ] Slots past the cap render **"this meeting allows fewer video slots"**
      (`data-slot-state` = `unavailable`, `data-rendered` = `empty`). They are permanent and never a
      spinner.
- [ ] `receiveSlots.video.serverCap` on the bus equals `MC_MAX_VIDEO_RECEIVE_SLOTS`.
- [ ] mc-media *Slot States by Kind* shows `slot_state="unavailable"` for video. There is no separate
      truncation counter.

**Under-fill.** With fewer other video participants than M (for example, two participants at the
launcher's M):

- [ ] The extra slots read **"no one to show in this slot yet"** (`fewer_sources_than_slots`). The
      grid is visibly under-filled, not padded with blanks or spinners.

Automated equivalent:
- `packages/web-app/e2e/video-over-subscription.spec.ts`, "the excess slots read unavailable, the
  join succeeds and the others render", and
  `27_mc_slot_placement.rs::test_video_over_subscription_truncates_to_unavailable`.
- Under-fill is judged by the oracle inside every video spec (`VideoScenario.expectVideo`).
- Component tier: `packages/web-app/src/__tests__/inMeeting.test.ts`, "each wire slot state renders
  distinctly, including the far-end-muted case", and `packages/web-app/src/__tests__/slotState.test.ts`,
  "no two states render the same text (per kind)".
- **No test asserts the literal under-fill copy**; it is human-only (see Promote-to-E2E).

## Phase 4 — The four mute controls, alone and composed (A is host)

Use the Phase 1 cohort: all four publish both kinds. Before **D's** join view mounts, set
`window.__dt_test_levers__ = { forceHostControls: true }`. This renders the host buttons in a
non-host context, so step 4g exercises MC's refusal rather than a hidden button. Without the lever,
a non-host sees **no** server-mute buttons at all; confirm that once in C's view.

The camera-light checks below read `self-camera-state` and the bus `sendTracks.cameraTrackLive`
(the test pattern has no physical light). Phase 10 repeats them with a physical light.

- [ ] **4a Client audio mute (B).** B clicks **Mute** (`mute-toggle`); `mute-state` reads "muted".
  - [ ] **The mic stays open** (`sendTracks.micTrackOpen` true).
  - [ ] **Nothing is sent:** B's tone disappears everywhere.
  - [ ] B's video keeps flowing.
  - [ ] At peers, B's `mute-audio-client-<B>` has `data-muted="true"` and reads "mic: muted by
        them". The element is always rendered; its value is the check.
  - [ ] Unmute restores the tone.
- [ ] **4b Client video mute (B).** B clicks **Turn camera off** (`video-mute-toggle`);
      `video-mute-state` reads "camera off".
  - [ ] **The camera ends**: `sendTracks.cameraTrackLive` false, and `self-camera-state` reads "you
        turned your camera off".
  - [ ] Peers see B's tile as **"B turned their camera off"** (`avatar_client_muted`), never a frozen
        last frame.
  - [ ] At peers, B's `mute-video-client-<B>` reads "camera: off by them".
  - [ ] B's tone is unaffected.
  - [ ] Turn camera on restores the live pattern.
- [ ] **4c Server audio mute (A mutes B).** A clicks **Mute microphone for everyone**
      (`server-mute-audio-<B>`).
  - [ ] B's tone disappears everywhere.
  - [ ] B's `server-mute-state` has `data-server-muted="true"` and reads "microphone muted for
        everyone by <A's name>".
  - [ ] At peers, `mute-audio-server-<B>` reads "mic: muted by host".
  - [ ] **B's video is untouched.**
- [ ] **4d Server video mute (A mutes C).** A clicks **Turn camera off for everyone**
      (`server-mute-video-<C>`).
  - [ ] **C's camera ends** (`cameraTrackLive` false), and `self-camera-state` reads "the host
        turned your camera off".
  - [ ] Peers see **"C's camera was turned off by the host"** (`avatar_server_muted`).
  - [ ] C's `server-mute-state-video` reads "camera turned off for everyone by <A's name>".
  - [ ] At peers, `mute-video-server-<C>` reads "camera: off by host".
  - [ ] A's button for C flips to "Turn camera back on for everyone".
  - [ ] **C's audio is untouched.**
  - [ ] mc-media *Send Directive Withdrawals by Kind and Reason* (`reason="server_muted"`) stepped.
  - [ ] *Server-Muted Sources* reflects it.
- [ ] **4e Server unmute while client-muted (C).** C first turns its own camera off (4b). A then
      clicks **Turn camera back on for everyone**.
  - [ ] **Capture stays off**, and C's light (`cameraTrackLive`) stays off.
  - [ ] **NO notice** appears at C.
  - [ ] Peers show `avatar_client_muted` for C.
- [ ] **4f Server unmute while NOT client-muted (the host resumes capture, R-18).** A server-mutes
      C's video again, with C not client-muted, and then A unmutes.
  - [ ] **The camera light comes back on AND the notice is visible in the same moment**, rendered
        above the video grid (`resume-notice-video`, `role="alertdialog"`). The text, from
        `packages/web-app/src/views/ResumeNotice.svelte` (quoted; check that file if the wording
        changed), is: "The host turned your camera back on." with the action "Turn my camera off
        now" and "OK".
  - [ ] **Non-suppressible:**
    - it is still showing after you look away and back;
    - **Escape** does not close it;
    - **clicking outside** does not close it.

    Do not test reload as a dismissal path; a reload is not a pass.
  - [ ] **"Turn my camera off now"** (`resume-notice-mute-video`) stops capture **at once, with no
        confirm step**. The light goes off, peers see "C turned their camera off", and the notice
        clears.
  - [ ] Repeat the unmute, and press **OK** (`resume-notice-ack-video`) instead. It only
        acknowledges: capture stays on.
  - [ ] **Kinds are independent.** Server-mute and unmute C's audio as well (C not client-muted;
        A's audio unmute button reads "Turn microphone back on for everyone").
        Two separate notices appear (`resume-notice-audio`, with the text "The host turned your
        microphone back on." and the action "Mute my microphone now"). Neither replaces the other,
        and acting on one leaves the other.
- [ ] **4g A non-host is refused.** D (forced host controls) clicks **Turn camera off for everyone**
      on B's row.
  - [ ] D shows `host-control-error-<B>`.
  - [ ] D stays joined.
  - [ ] **B is not muted anywhere.**
  - [ ] mc-media *Server Mute Requests by Kind* shows a refused (`not_permitted`) outcome.
- [ ] **4h The patched-client backstop is not exercised by hand.** A browser always obeys a
      withdrawal. Note it in the run record as automation-only (MH-A1's env-test, below). Also read
      client-media *Video From Server-Muted Senders (ALERTED)* and mh-media *Ingress Drops by
      Reason* (`server_muted`). The normal shape is flat drops near zero, because the directive is
      withdrawn.
- [ ] **A request about one kind never changes the other.** Across 4a–4f, the other kind's
      indicator and media never moved. Note any exception.
- [ ] mc-media *Client Mute Requests by Kind and Outcome*, *Server Mute Requests by Kind*, *Unmute
      Requests* and *Server-Muted Sources* moved as above.

Automated equivalents:

- Client mute: `packages/web-app/e2e/client-mute-video.spec.ts`, "video mute ends the camera and
  shows avatar_client_muted; audio mute keeps the mic open and video flowing".
- Server mute and the notice: `packages/web-app/e2e/server-mute-video.spec.ts`, "server video mute
  ends the camera everywhere; unmute resumes with a non-suppressible notice; client mute composes".
  It covers 4d–4f, including Escape, click-outside and a re-render.
- Notice UI: `packages/web-app/src/__tests__/inMeeting.test.ts`, "R-18: nothing hides the notice but
  an acknowledgement — not Escape, a click outside, or a re-mount" and "R-18: the inline action
  client-mutes THAT kind at once, then clears only that notice".
- The R-18 event and its ordering fail-safe:
  `packages/sdk-core/src/session/__tests__/serverMuteResume.test.ts` (the three ordering tests named
  above, and "client-muted: no capture resumes, so NO event (and the latch is consumed)").
- Per-kind independence:
  `packages/sdk-core/src/media/lifecycle/__tests__/muteState.test.ts`, "muting audio never changes
  video, and vice versa".
- Non-host refusal:
  - `packages/web-app/e2e/server-mute.spec.ts`, "a NON-host server-mute request is REFUSED by MC,
    and the refused client stays joined". This is audio; **no video-specific non-host browser spec
    exists**.
  - `crates/env-tests/tests/35_mc_server_mute_teardown.rs::test_non_host_server_mute_is_refused_and_counted`.
- The patched client under server video mute:
  - `35_mc_server_mute_teardown.rs::test_video_server_mute_withdraws_drops_a_patched_publisher_and_unmute_restores_it`;
  - `crates/env-tests/tests/37_mh_stream_attribution.rs::test_per_stream_attribution_on_the_datagram_path`
    (phase (c)).
- Client video mute at MH:
  `35_mc_server_mute_teardown.rs::test_client_video_mute_pauses_without_touching_the_directive_and_a_storm_converges`.

## Phase 5 — Watched-only directives

A creates a meeting. B joins with a lever that receives no video, set before its join view mounts:
`window.__dt_test_levers__ = { videoReceiveSlots: '0' }`. B ticks Camera. A ticks Camera.

- [ ] With only A and B in the meeting, B watches nobody and **A's camera is off**: A's
      `cameraTrackLive` is false. A's `self-camera-state` reads "your camera is on whenever
      someone is watching", which is the R-8 explanation; it is not an error.
- [ ] **A watcher lights it.** C joins with video slots (no lever). A's camera comes on
      (`cameraTrackLive` true), and C's tile for A goes live.
- [ ] **The last watcher leaving ends it.** C leaves. A's camera goes off again.
- [ ] mc-media *Send Directive Withdrawals by Kind and Reason* records the unwatched withdrawal.
      `mc_media_send_directives_total{outcome="emitted_empty_targets"}` moving is routine success,
      not a failure.

Automated equivalent:
- `packages/web-app/e2e/watched-only-video.spec.ts`, "an unwatched camera is off; a watcher lights
  it; the last watcher leaving ends it";
- `27_mc_slot_placement.rs::test_pairing_stability_and_watched_only_video_directive`;
- `packages/sdk-core/src/media/lifecycle/__tests__/cameraGate.test.ts`, "acquires on direction,
  stops on withdrawal (unwatched or server-muted), re-acquires on restore".

## Phase 6 — Leave and rotation with video flowing; the late joiner

Use the Phase 1 cohort (A–D, all both). **C leaves** (the Create nav).

- [ ] At A, B and D, the remaining tiles **keep moving with no visible stall**. Watch the patterns
      across the leave.
- [ ] C's slot at each remaining receiver is refilled or under-filled per R-3. **No tile shows C
      after C has left.**
- [ ] mc-overview *Meeting KEK Issuance by Trigger* steps `participant_left`, once the rotation
      debounce elapses.
- [ ] **A leaver loses video after rotation.** No step can watch it, because the leaver's context is
      gone. See the automated equivalent below.
- [ ] **E joins late**, with video already flowing. E's tiles fill per R-3, and **its first frame in
      every tile is a clean picture: never a smeared or blocky mid-group frame**. That is the
      on-demand keyframe; there is no periodic keyframe to wait for.
- [ ] client-media *Video Keyframes Produced by Cause* shows a `request` step at the publishers when
      E joins.

Automated equivalent:
- `packages/web-app/e2e/kek-rotation-video.spec.ts`, "video keeps rendering and decrypting at the
  remaining receivers across the rotation";
- `packages/web-app/e2e/late-joiner-video.spec.ts`, "a participant joining with video already
  flowing sees it, with no keyframe interval anywhere";
- leaver lockout: `videoReceiver.test.ts`, "a video LEAVER cannot open post-rotation video
  (positive control: pre-rotation accepted)";
- `crates/env-tests/tests/34_mc_kek_rotation.rs::a_leave_with_video_flowing_rotates_the_kek_without_touching_video_directives`;
- `crates/env-tests/tests/39_mh_video_keyframes.rs::test_on_demand_keyframes_and_recovery`, phase
  (b), LATE JOINER.

## Phase 7 — Camera revoked mid-call (run in the Phase 10 real-camera session)

This needs a **real** camera: the test pattern cannot be revoked. Run it in Phase 10's session
(the dev server without `DT_TEST_PATTERN`). A two-participant meeting, B with a real camera:

- [ ] B revokes the camera permission in Chrome's site settings for `demo.localhost:5173`, **or**
      unplugs the camera.
  - [ ] At A, B's tile becomes **"B turned their camera off"** (video-muted). It is **never a
        perpetually pending tile** and never a frozen last frame.
  - [ ] B's own `self-camera-state` reads "your camera stopped working; others see your camera as
        off", with the reason "the camera stopped (unplugged or revoked)".
  - [ ] **B's camera light goes off.**
- [ ] B restores the permission or replugs the camera, then turns the camera on again. Video
      resumes.
- [ ] client-media *Capture Failures by Kind and Outcome* shows `ended` for video.

Automated equivalent: **no browser spec or env-test.** The component and unit equivalents are:
- `packages/sdk-core/src/media/lifecycle/__tests__/cameraGate.test.ts`, "a track ending presents the
  participant as video-muted and does not retry by itself";
- `packages/sdk-core/src/media/lifecycle/__tests__/VideoPublisher.test.ts`, "a track that ENDS
  mid-call: counted ended, muted, and retried exactly once on the next restore edge";
- `packages/web-app/src/__tests__/inMeeting.test.ts`, "R-19: a failed camera is shown to its owner
  WITH the reason".

If a camera-permission prompt is denied up front instead, that is `client-dev-local.md` F23.

## Phase 8 — Video cap 0, the operator lever (R-26)

**This phase rolls MC.** Run it near the end, and restore before anything else. **Run every deploy
in this plan (here and Phase 10, switch and restore) in a shell with `DT_HOST_GATEWAY_IP` unset**
(`unset DT_HOST_GATEWAY_IP; ./infra/kind/scripts/deploy.sh`). `deploy.sh` renders the MC/MH
advertise addresses from it at deploy time, so a leftover export makes the next join die exactly
like F8. That would be misread as the lever or the profile breaking joins. The procedure has
one home: `docs/runbooks/mc-deployment.md` §"Video-off lever". The why, cost and blast radius are
in `docs/runbooks/mh-deployment.md` §"Operator levers for media: video off, and nothing else". Do
not restate them, and **never** `kubectl edit` the ConfigMap.

1. On the deployed commit, with nothing else changed, set `MC_MAX_VIDEO_RECEIVE_SLOTS=0` in
   `infra/services/mc-service/config.env`, then run `./infra/kind/scripts/deploy.sh` (in a devloop
   container, `dev-cluster deploy`).
2. The deploy rolls mc-0 and mc-1 only, never MH. **The roll reconnects everyone**, so participants
   rejoin under the new cap. That reconnect is the cost of the lever, not a failure.

- [ ] **MH did not roll.** Read `kubectl -n dark-tower get pods -l app=mh-service` (age and
      restarts) before and after the deploy. mh-0 and mh-1 must NOT have restarted. If they did,
      something besides the lever changed: FAIL, and triage via `mh-deployment.md` §"Rollout With
      Media Flowing". No automated test covers this half of the blast-radius claim, because the lever
      is deliberately not an env-test.
- [ ] `mc_media_receive_slot_cap{media_kind="video"}` reads 0 on **both** MC pods (mc-media *Receive
      Slot Cap*).
- [ ] Every video slot renders **"this meeting allows fewer video slots"** (`unavailable`).
- [ ] **No camera light**: `cameraTrackLive` is false in every context.
- [ ] **Audio is unaffected**: every tone is heard as in Phase 1.
- [ ] mc-media *Slot States by Kind* shows every video slot `unavailable`.
- [ ] **Restored and verified:**
  - [ ] `git checkout -- infra/services/mc-service/config.env`, then the same deploy (another MC
        roll).
  - [ ] `mc_media_receive_slot_cap{media_kind="video"}` reads the committed value again on both MC
        pods.
  - [ ] Only then move on.

Automated equivalent:
`crates/mc-service/tests/video_slots_integration.rs::the_video_cap_zero_lever_turns_video_off_and_leaves_audio_identical`.
It is deliberately not an env-test, because flipping the lever would roll the shared cluster. If
slots read `unavailable` unexpectedly, see `client-dev-local.md` F18. If joins die after either
deploy, see F8 first (`DT_HOST_GATEWAY_IP`).

## Phase 9 — Debug options: the self-check (R-7) and the statistics overlay (R-35)

- [ ] On a **fresh** profile (or with `darktower.debugPanel.v1` cleared), the debug panel is
      **hidden by default**. Only the toggle **Show debug options** (`debug-panel-toggle`) is
      visible, on the join view and in the meeting.
- [ ] Open it. The panel (`debug-panel`) says "Local only: nothing in this panel is sent, recorded
      or reported."
- [ ] **Self-check before joining (R-7).** Before joining, click **Start self-check**
      (`self-check-start`). Pre-join, it opens the **real** camera and microphone through the
      browser, even in a test-pattern build. Expect a permission prompt, and do it in **one profile
      only** (camera contention).
  - [ ] your own camera renders locally (`self-check-video`);
  - [ ] the **Microphone level** meter (`self-check-level`) moves. The microphone is metered, never
        played back (F27);
  - [ ] **no handler is involved**: you are not in a meeting;
  - [ ] **Stop** (`self-check-stop`) releases the preview capture before the join.
- [ ] **Self-check in the meeting.** It **reuses** the session's own tracks and opens no second
      capture.
  - In this test-pattern build, the pattern has no camera track. The panel therefore correctly shows
    `self-check-no-camera` ("no local camera track"), with no video. The meter follows the test
    tone's track. Neither is a FAIL.
  - Check the in-meeting **camera** reuse (`self-check-video` shows your own camera) in the Phase 10
    real-camera session. Alternatively, use a context opened with
    `window.__dt_test_levers__ = { forceCameraCapture: true }`.
- [ ] **Statistics overlay (R-35)**, in the meeting:
  - [ ] the receive table (`debug-stats-receive`) has one row per slot, with **packet rate, bit
        rate, queue (buffer depth), decoded, keyframes seen, drops** and **unfulfilled**;
  - [ ] the send table (`debug-stats-send`) has **sent, packet rate, bit rate, keyframes by cause,
        groups abandoned**;
  - [ ] the values move while media flows.
- [ ] **No overlay value is exported.** With the overlay open, run in Prometheus:
      `count by(__name__)({__name__=~"dt_client_.*"})`.
  - [ ] **Positive control first:** `dt_client_media_frames_received_total` is present. An empty
        result is not a pass.
  - [ ] Every name returned is in the collector's explicit allowlist
        (`infra/services/otel-collector/collector.yaml`) and catalogued in
        `docs/observability/metrics/client.md`.
  - [ ] **Label names.** `count by(__name__)` drops labels, so read them separately:
        `curl -s http://<prometheus>/api/v1/labels --data-urlencode 'match[]={__name__=~"dt_client_.*"}'`.
        Every label name must be in the collector's `keep_keys(attributes, [...])` line
        (`infra/services/otel-collector/collector.yaml`) or be a scrape or histogram label (`job`,
        `instance`, `le`, …). Positive control: `media_kind` is present. A byte, size or
        per-slot-id label is a FAIL.
  - [ ] **Names.** `count({__name__=~"dt_client_.*(byte|size).*"})` is empty. An overlay-only series,
        such as a per-stream bytes rate, must not exist. The positive control above keeps an
        empty result meaningful.

Automated equivalent:
- `packages/web-app/tests/debugNeverExported.test.ts`, "DENY: no debug module imports an export
  path, a sink, a transport, or an unlisted value" and "REVERSE: the export paths never reference
  the local surface (with their own controls)";
- `packages/web-app/src/__tests__/debugPanel.test.ts`, "hidden by default with empty storage; the
  toggle shows, hides and persists only the boolean";
- `packages/web-app/src/__tests__/inMeeting.test.ts`, "the debug toggle is present and the panel is
  HIDDEN by default; the overlay reads nothing while hidden";
- `packages/web-app/src/__tests__/selfCheck.test.ts`, "in-meeting (REUSE), mounted exactly as
  InMeeting mounts it: only the allow-listed getters are read, zero transport access, and it RAN".

## Phase 10 — Production profile, real cameras, headphones

There is no `dev-web.sh` override for the codec profile. The one authoritative procedure is the
header of `infra/kubernetes/overlays/kind/services/media-codec-profile/configmap-test-profile-patch.yaml`
("PRODUCTION-QUALITY DEMO"); follow it, not a copy. In outline:

1. **Switch.** Delete the `patches:` entry in the sibling `kustomization.yaml`. Kind then runs the
   production values in `infra/services/media-codec-profile/profile.env`. Then run
   `unset DT_HOST_GATEWAY_IP; ./infra/kind/scripts/deploy.sh` (F8; in a devloop container,
   `dev-cluster deploy`). **This rolls
   mc-0, mc-1, mh-0 and mh-1, and every live meeting drops.**
2. **Check the ceiling first.** Per MH pod, read `mh_media_egress_stream_ceiling` (mh-media
   *Installed Egress Streams vs Stream Ceiling*) and compare it with this meeting's P × (N + M).
   Compare each pod with its own ceiling. A two-participant pass fits. A larger demo may also need
   the budget patch the header names, and **profile and budget roll back together**.
3. **Relaunch the dev server without the test pattern or tone**, so the camera and microphone are
   real: `DT_TEST_LEVERS=1 scripts/dev-web.sh`.
4. **Real cameras.** One physical camera serves one profile (camera contention). Use **two cameras
   on this machine**, one per profile, selected in each profile's Chrome camera setting. A second
   machine is **not** available: `dev-web.sh` "WHY NOT A SECOND MACHINE" applies, since
   `demo.localhost` is a secure context and a LAN address is not.
5. **Wear headphones** on every participant, so the microphones do not feed back.

- [ ] Read back the deployed `media-codec-profile` ConfigMap: it shows the base values, and mc-0,
      mc-1, mh-0 and mh-1 all reference one generation.
- [ ] Re-run **Phase 1** with two participants and real cameras. Each sees the other's real camera
      and hears the other's voice.
- [ ] Re-run the **Phase 9 in-meeting self-check**: `self-check-video` now shows your own camera
      (reused, no second capture).
- [ ] Re-run **Phase 4**, watching the **physical camera light**:
  - [ ] it goes **off** on client video mute (4b);
  - [ ] it goes **off** on server video mute (4d);
  - [ ] it stays **off** on server unmute while client-muted (4e);
  - [ ] it comes **on, with the notice visible at the same time**, on server unmute while not
        client-muted (4f);
  - [ ] it goes **off at once** on "Turn my camera off now".
- [ ] **The light goes off on withdrawal.** Give B the lever `videoReceiveSlots: '0'` (Phase 5). A's
      light is off while only B watches nobody, comes on when a second watcher joins, and goes off
      when that watcher leaves.
- [ ] Run **Phase 7** (camera revoked) here.
- [ ] **Restored and verified:**
  - [ ] Run the restore step from the patch header (`configmap-test-profile-patch.yaml`,
        "PRODUCTION-QUALITY DEMO", step 3: it restores the profile AND the budget patch together),
        then the deploy, again with `DT_HOST_GATEWAY_IP` unset (F8).
  - [ ] The ConfigMap read-back shows the test profile again.
  - [ ] `mh_media_egress_stream_ceiling` is back at its pre-phase reading.
  - [ ] **No Layer 7 run may follow a half-restore.**
  - [ ] `crates/env-tests/tests/01_mh_deployment_config.rs::test_kind_media_codec_profile_is_the_committed_test_profile`
        is the restore check, and fails loudly naming the patch otherwise.

Automated equivalent: **none at the production profile**, by nature. The real camera, the physical
light and the voice are human-only. Profile integrity:
- `01_mh_deployment_config.rs::test_kind_media_codec_profile_is_the_committed_test_profile`;
- `crates/env-tests/tests/02_mc_deployment_config.rs::test_mc_and_mh_resolve_one_media_codec_profile_generation`.

## Phase 11 — Story-2 audio regression (M = 0)

Restart the dev server with `VITE_DT_VIDEO_RECEIVE_SLOTS=0`, keeping `DT_TEST_TONE=1` and
`DT_TEST_LEVERS=1`. No participant ticks Camera.

- [ ] Run the story-2 plan's Phases 1–4
      ([hear-each-other plan](2026-09-21-hear-each-other-manual-test-plan.md)). Use **today's** UI
      and panel names: "Mute microphone for everyone", and mc-media *Server Mute Requests by Kind*
      and *Slot States by Kind*. The older names in that plan are stale.
- [ ] They behave exactly as story 2 recorded them. No video grid cell appears, and no camera is
      requested.

Automated equivalent: the six story-2 specs, run at M = 0 in every Layer 7 run
(`packages/web-app/e2e/cohort.ts` `SUITE_VIDEO_RECEIVE_SLOTS = 0`, asserted at runtime per context):
- `multi-party-hear.spec.ts`;
- `partial-connectivity.spec.ts` (story 2 S10a);
- `server-mute.spec.ts`;
- `kek-rotation.spec.ts`;
- `over-subscription.spec.ts`;
- `solo-participant.spec.ts`.

The list is from `packages/web-app/e2e/README.md`, "Story-2 reruns with M=0 (R-33)".

## Phase 12 — Recovery after an induced publisher→MH abandonment (R-12, R-30, amendment 4)

**Induction: no human-reachable method exists in the tree today. This is a gap, raised to the
lead; it is not dropped.**

- The publisher's per-target backlog cap is an SDK configuration default
  (`packages/sdk-core/src/config/clientConfig.ts`, `media.egress.videoMaxBacklogFrames` /
  `videoMaxBacklogBytes`). The web-app plumbs no env, define or lever for it
  (`packages/web-app/src/lib/session.ts` passes no media config). So "a dev build with a lowered
  per-target backlog cap" is not reachable without a code change.
- Host-level traffic shaping of the publisher's uplink below the profile's fixed bitrate needs no
  browser flag. But the tree documents no procedure for it. Linux `tc` in WSL does not shape
  Windows Chrome's egress under mirrored networking. So it is not a verified method for this pass.

**If a verified induction is added later**, the step is:
1. Hold one publisher's uplink below the deployed profile's video bitrate (read back, never
   restated) until it abandons its group toward its handler.
2. Lift the constraint.

Then check:
- [ ] The publisher's **send groups abandoned** rises: its R-35 overlay "Groups abandoned" column, and
      client-media *Video Send Groups Abandoned (publisher-decided)*.
- [ ] Each peer's tile for that publisher **resumes with a clean picture, never a smeared mid-group
      frame, with no reload or rejoin**.
- [ ] The publisher's keyframes by cause show the **self-keyed group start** (`abandonment`):
      client-media *Video Keyframes Produced by Cause*, and the overlay's "Keyframes by cause" column.
- [ ] mh-media *Pending Edges* returns to **0**, and *Stream-Path Group Resets by Reason (per
      destination)* shows the reset.
- [ ] **Each receiver's** unfulfilled reading returns to zero. Read each receiver's **R-35 overlay**
      "Unfulfilled" column. client-media *Video Slots Unfulfilled Now (single-writer gauge)* is
      single-writer across browsers, so it only corroborates. It is authoritative only on a
      one-browser run.
  - MH's pending gauge excludes a slot whose publisher is absent from that handler, so "pending is
    0" is necessary but not sufficient. The overlay is the complement.
- [ ] Recovery is observed as **state returning to zero, never timed**. Repeated abandonment while
      the constraint holds is accepted (R-12). The pass condition applies after the constraint is
      lifted.

Automated equivalent:
- publisher abandonment, `packages/sdk-core/src/media/pipeline/__tests__/videoEgress.test.ts`:
  - "a backlog cap on one target RESETS that target only (never finishes it); others and the chain
    never stall";
  - "after a publisher-decided abandonment, the NEXT key chunk starts a group on EVERY target";
- the re-key, `packages/sdk-core/src/media/lifecycle/__tests__/VideoPublisher.test.ts`: "a
  publisher-decided abandonment re-keys (held as the deferral inside the interval), counted as
  abandonment";
- MH-B's keyframe env-test, `crates/env-tests/tests/39_mh_video_keyframes.rs::test_on_demand_keyframes_and_recovery`:
  phase (e), PUBLISHER ABANDONMENT; (d), BACKLOG; (f), STUCK PENDING.

Triage: `mh-incident-response.md` **Scenario 22: Video Slot Stuck Pending**, and `client-dev-local.md`
F26.

---

## Security acceptance (checked at Gate 3, walked by the human)

| Criterion | Where in this plan |
|-----------|--------------------|
| A patched or non-cooperating client under server video mute | Phase 4, step 4h. Automation-only: MH-A1's `37_mh_stream_attribution.rs::test_per_stream_attribution_on_the_datagram_path` phase (c), and `35_mc_server_mute_teardown.rs::test_video_server_mute_withdraws_drops_a_patched_publisher_and_unmute_restores_it` |
| Host unmute resumes the camera with the notice visible | Phase 4f (`cameraTrackLive`); Phase 10 (physical light) |
| The camera light goes off on withdrawal and on mute | Phases 4b, 4d and 5 (`cameraTrackLive`); Phase 10 (physical light) |
| A leaver loses video after rotation | Phase 6, `videoReceiver.test.ts` (automation-only, by nature) |
| No insecure browser flag | Header; no step uses one |
| Test pattern and tone only in dev builds | Phase 0 (the gate and bundle tests) |
| Traces and HARs not committed | Header; `packages/web-app/e2e/README.md` |

---

## Dashboards to read (titles verified in `infra/grafana/dashboards/`)

- `client-media` = *Client SDK - Media Path*. **Read its "READ FIRST - how to read an empty panel on
  this board" first**: check `up{job="otel-collector"}==1`, then `count(dt_client_media_frames_received_total)`.
  Client series are lazy, so an empty panel is ambiguous.
- `mc-media` = *Meeting Controller - Media Path*
- `mh-media` = *Media Handler - Media Path*
- `mc-overview` (KEK issuance); `mh-slos` = *Media Handler - SLOs*

| What | Dashboard → panel |
|------|-------------------|
| Slot fill, truncation, the cap | mc-media → *Slot States by Kind*, *Receive Slot Fill vs Cap*, *Receive Slot Cap* |
| Join refusals | mc-media → *Join Refusals - Story 3 Media Join Values* |
| Mutes | mc-media → *Client Mute Requests by Kind and Outcome*, *Server Mute Requests by Kind*, *Unmute Requests*, *Server-Muted Sources*; client-media → *Key and Mute Lifecycle* |
| Directive withdrawals (mute, unwatched) | mc-media → *Send Directive Withdrawals by Kind and Reason*, *Send Directives by Outcome* |
| Keyframe interval in force | mc-media → *Directed Video Minimum Keyframe Interval* |
| Non-cooperating sender | client-media → *Video From Server-Muted Senders (ALERTED)*; mh-media → *Ingress Drops by Reason* |
| Recovery (Phase 12) | client-media → *Video Send Groups Abandoned (publisher-decided)*, *Video Keyframes Produced by Cause*, *Video Slots Unfulfilled Now (single-writer gauge)*, *Video Recovery Requests Sent (by cause)*; mh-media → *Stream-Path Group Resets by Reason (per destination)*, *Pending Edges*, *Oldest Pending Edge*, *Pending Exits by Outcome*, *Keyframe Requests Sent*, *Recovery Requests by Outcome* |
| Capacity | mh-media → *Installed Egress Streams vs Stream Ceiling* (per instance; never the fleet, never *Egress Edges vs Edge-Limit Backstop*) |
| Decode health | client-media → *Video Decoded Source-Intervals*, *Video Decoder Failures per Accepted Frame (ALERTED)*, *Capture Failures by Kind and Outcome* |
| Open objectives (observe, never gate) | client-media → *Video Time To First Render (p95)*, *Video Slot ACTIVE To First Render (p95)* (beside *Video Slot Unfulfilled Duration (p95)*), *Time to First Media Frame (p95 / p50)* |
| Pattern vs camera, tone vs microphone | client-media → *Send Path - Capture Source (SAFETY: test_tone outside dev)* |

The alert thresholds for `ClientVideoSlotUnfulfilled` and `MHVideoEdgeStuckPending` are **fault
bounds, not objectives**. Do not read them as targets. The first 24 hours after a deploy follow
`mh-deployment.md` §"Story 3 video: the 24-hour watch list, in the order things rise first".

## Triage on failure

| Symptom | Pointer |
|---------|---------|
| Tile frozen or blank; `stream_slot_mismatch` / `stream_partial_reset` | `client-dev-local.md` F22; `mh-incident-response.md` Scenario 21 (Slow Video Subscriber (Stream-Credit Stall)) |
| A live slot shows neither video nor an avatar (unfulfilled); recovery stuck (Phase 12) | F26; `mh-incident-response.md` Scenario 22 (Video Slot Stuck Pending); `mc-incident-response.md` Scenario 23 (Client Video Slot Unfulfilled) |
| Decode failures | `mc-incident-response.md` Scenario 22 (Client Video Decode Failures) |
| Keyframes produced climbing without bound | `mh-incident-response.md` Scenario 20 (Video Keyframe Storm) |
| Server video mute not taking effect | `mc-incident-response.md` Scenario 21 (Server Mute Not Enforced); `mh-incident-response.md` Scenario 19 (Server Mute Not Taking Effect At Ingress) |
| A client-muted participant still sends | `mh-incident-response.md` Scenario 23 (Client Sending While Client-Muted) |
| Some receivers miss some senders; admission refusals | `mh-incident-response.md` Scenario 18 (Media Egress Budget Exhaustion); `mc-incident-response.md` Scenario 18 (A Participant Hears Only Part of the Roster) |
| Silence or blank tiles after a leave | `mc-incident-response.md` Scenario 16 (Missing Key Material); F16 |
| Join fails | `mc-incident-response.md` Scenario 8 (Join Failures); F1 / F7 / F8 |
| Slots read `unavailable` unexpectedly | F18 |
| Camera permission denied; no video in a tab | F23; F24 |
| Media start fails with a `media.videoReceive` config error; a v2 directive refused | F21; F25 |
| Need the local tools | F27 (the R-7 self-check and the R-35 overlay) |
| Audio regression (Phase 11) | F12–F17, as in the story-2 plan |

---

## Promote-to-E2E verdicts

| Plan step | Automated today | Human-only residue |
|-----------|-----------------|--------------------|
| 1 See and hear by content | `multi-party-see.spec.ts` (3 layers + tone, oracle-predicted) | Seeing the tiles; rendered names and pairing |
| 2 Publish mixes, refusal | `publish-mixes.spec.ts`, `27_mc_slot_placement.rs` | Rendered avatars and refusal copy |
| 3 Truncation / under-fill | `video-over-subscription.spec.ts`; under-fill via the oracle; `inMeeting.test.ts` / `slotState.test.ts` (distinct state copy) | **The under-fill copy is asserted by no test** (candidate) |
| 4 Four mutes, notice, non-host | `client-mute-video.spec.ts`, `server-mute-video.spec.ts`, `server-mute.spec.ts` (audio non-host), `35_…`, `37_…` | Rendered indicators; **a video-kind non-host browser refusal** (candidate) |
| 5 Watched-only | `watched-only-video.spec.ts` | The physical light (Phase 10) |
| 6 Rotation, late joiner | `kek-rotation-video.spec.ts`, `late-joiner-video.spec.ts`, `videoReceiver.test.ts` | "No visible stall"; a clean first picture by eye |
| 7 Camera revoked | Unit and component only | **No browser equivalent** (candidate: a spec that stops the camera track mid-call via `forceCameraCapture` and asserts `avatar_client_muted` at peers) |
| 8 Video cap 0 | `video_slots_integration.rs` (not an env-test, by design) | The live lever and its roll; **that MH does not roll** (human-only by design) |
| 9 Debug options | `debugNeverExported.test.ts`, `debugPanel.test.ts`, `selfCheck.test.ts` | Seeing the self-check and the overlay |
| 10 Production profile, real cameras | None, by nature | All of it |
| 11 Story-2 at M = 0 | The six story-2 specs at M = 0 | As in story 2 |
| 12 Recovery after abandonment | `videoEgress.test.ts`, `VideoPublisher.test.ts`, `39_mh_video_keyframes.rs` | **No human-reachable induction** (gap raised to the lead) |

---

## Capacity evidence

R-28: cohort size, CPU load and Kind capacity for the video cohort are verified as evidence, not
assumed. This is a **recorded run, not a gate**. It is the story's last-task measurement on this
commit, and nothing in the profile, the ceiling or the cohort was changed to make it fit.

**Run.** 2026-10-24, commit `bc82880f` (`bc82880fea30c9a68dbfa6a08a4d6c4630b11169`) plus this
task's uncommitted test-harness files, which change no product code. It ran right after this
task's green Layer 7 run on the same commit and cluster.

**Which run is the record.** Five evidence runs were made.
- Runs 1 and 2 failed on harness bugs, fixed before the next run (see [Run record](#run-record)).
- Runs 3, 4 and 5 passed.
- **Run 5 is the record.** It is the only run of the final harness, after the review fixes: the
  slot-row completeness checks, the harness/liveness error split, and the `/proc` read errors no
  longer swallowed.
- Across runs 3–5:
  - see-all renderer CPU was 16.6–18.2 % of one core per context, with gpu-process 58.2–70.6 % and
    browser 33.6–39.9 %;
  - over-subscription renderer CPU was 11.5–14.2 %, with gpu-process 43.6–51.0 % and browser
    23.8–27.8 %;
  - every liveness, unfulfilled, recovery and edge result was identical.

**Host.**
- Hardware: the devloop host, Intel Core i9-14900KF, 32 logical CPUs, 16 GB RAM.
- OS: Linux under WSL2 (kernel `6.6.87.2-microsoft-standard-WSL2`).
- The browsers ran in the devloop container, which has a hard memory cap of about 11.6 GB.
- The Kind node `devloop-user-see-each-other-control-plane` (capacity 32 CPU) shares the same host.
- Browser: Playwright 1.62.1, `chrome-headless-shell`, with the harness's fake-media flags only.

So browser and cluster load share one CPU pool. The figures are this host's, not a per-machine
claim.

**Profile, read back from the deployed ConfigMap** (`media-codec-profile-f6mgg6k6gk`, the one
generation mc-0, mc-1, mh-0 and mh-1 reference; dumped by Layer 7 Phase 1g):
`MEDIA_VIDEO_CODEC=vp8`, `MEDIA_VIDEO_WIDTH=320`, `MEDIA_VIDEO_HEIGHT=180`,
`MEDIA_VIDEO_FRAME_RATE_HZ=15`, `MEDIA_VIDEO_MAX_BITRATE_BPS=250000`, `MEDIA_AUDIO_CODEC=opus`,
`MEDIA_AUDIO_MAX_BITRATE_BPS=48000`, `MEDIA_AUDIO_FRAME_RATE_HZ=50`. The keyframe interval in force
was `mc_media_video_keyframe_min_interval_seconds` = 0.5 on both MC pods. MC's caps were
`mc_media_receive_slot_cap` audio 8 and video 4.

**Method.**
- The scenarios are `packages/web-app/e2e/capacity.evidence.ts` (shapes in its `SHAPES`), run
  through `playwright.capacity.config.ts` (`pnpm --filter @darktower/web-app test:capacity-evidence`).
  This is an opt-in recorded run, never part of `test:e2e`. The env was the Layer-7 one (`E2E_*`,
  the run's org, `E2E_MEDIA_CODEC_PROFILE_ENV`).
- Each shape is 4 contexts, all publishing both kinds, joined serially, with the slot oracle
  predicting every slot.
- **Window**: from all four converged to a span of at least 30 s. Over the window, every receiver's
  video slots were judged by the video detector (`VideoScenario.expectVideo`: three layers incl.
  the pattern liveness verdict) in repeated rounds. A harness or bus-contract error aborts the run;
  it is never counted as liveness lost.
- **CPU** is process-level, in `/proc` utime+stime per renderer process (`e2e/processCpu.ts`). Each
  context's renderer is mapped by an in-page busy-loop fingerprint taken while the page was idle.
  Shared browser-wide processes are reported separately. All values are a **% of one core** over
  the window.
- **Frames** come from the bus `videoSlots` deltas. Keyframes by cause come from the bus
  `sendCounts`.
- **Groups abandoned and unfulfilled** come from each context's R-35 overlay, opened only at the
  window's two ends. The bus and overlay rows must be exactly the declared video slots, or the run
  is a HARNESS error.
- **MH** values come from Prometheus at the window bounds (queries below).
- Series names are from `docs/observability/metrics/mh-service.md` and `client.md`.
- Raw records are kept outside git (`[capacity-evidence]` JSON lines).

### See-all: P=4, N=3, M=3, 24 edges

Window `2026-10-24T16:32:25.242Z` → `16:32:57.407Z` (30.1 s, 3 liveness rounds per receiver).

| Context | Renderer CPU (% of one core) | Video frames decoded / rendered per slot, in the window (slots 3, 4, 5) | Liveness lost | Video frames sent | Keyframes by cause in the window (since start) | Groups abandoned (start → end) | Unfulfilled at window end |
|---|---|---|---|---|---|---|---|
| A | 17.3 | 462/462, 462/462, 462/462 | no | 462 | 0 (directive_start 1, request 2, abandonment 0) | 0 → 0 | 0 ms on every slot |
| B | 17.1 | 462/462, 462/462, 462/462 | no | 462 | 0 (directive_start 1, request 2, abandonment 0) | 0 → 0 | 0 ms on every slot |
| C | 17.3 | 462/462, 462/462, 462/462 | no | 462 | 0 (directive_start 1, request 1, abandonment 0) | 0 → 0 | 0 ms on every slot |
| D | 17.0 | 462/462, 462/462, 462/462 | no | 462 | 0 (directive_start 1, request 0, abandonment 0) | 0 → 0 | 0 ms on every slot |

Shared processes (% of one core): browser 33.8, gpu-process 62.5, utility 8.7, zygote 0.

Every slot was `active` and rendered video. 462 frames in 30.1 s is the profile's frame rate, and
decoded equals rendered on every slot.

### Over-subscription: P=4, N=2, M=2, 16 edges (3 candidate senders per receiver for 2 slots of each kind)

Window `2026-10-24T16:33:21.765Z` → `16:34:03.589Z` (39.5 s, 4 liveness rounds per receiver).

| Context | Renderer CPU (% of one core) | Video frames decoded / rendered per slot, in the window (slots 2, 3) | Liveness lost | Video frames sent | Keyframes by cause in the window (since start) | Groups abandoned (start → end) | Unfulfilled at window end |
|---|---|---|---|---|---|---|---|
| A | 13.7 | 603/603, 603/603 | no | 602 | 0 (directive_start 1, request 2, abandonment 0) | 0 → 0 | 0 ms on every slot |
| B | 14.2 | 603/603, 602/602 | no | 602 | 0 (directive_start 1, request 2, abandonment 0) | 0 → 0 | 0 ms on every slot |
| C | 13.9 | 603/603, 602/602 | no | 602 | 0 (directive_start 1, request 0, abandonment 0) | 0 → 0 | 0 ms on every slot |
| D | 11.9 | 602/602, 602/602 | no | **0** | 0 (all 0) | 0 → 0 | 0 ms on every slot |

Shared processes (% of one core): browser 27.8, gpu-process 51.0, utility 7.0, zygote 0.

D is the last joiner, and every other receiver's two video slots hold earlier joiners (R-3). So
nobody watches D, and **D's camera is correctly off** (R-8 watched-only; 0 frames sent). This is
the expected shape, not a fault.

### Recovery activity and MH (Prometheus, over each window)

| Series | See-all | Over-subscription |
|---|---|---|
| `mh_media_egress_edges`, peak per instance (mh-0 / mh-1) | **24** / 0 | 0 / **16** |
| `mh_media_egress_stream_ceiling` per instance | 135 / 135 | 135 / 135 |
| Peak edges / that instance's ceiling (headroom) | 24 / 135 = 17.8 % (111 streams) | 16 / 135 = 11.9 % (119 streams) |
| `mh_media_group_resets_total`, increase (summed over `reason`) | 0, both instances | 0, both instances |
| `mh_media_edges_pending` at window end (max in the window) | 0 (0), both instances | 0 (0), both instances |
| `mh_media_stream_admission_total{outcome!="admitted"}` increase | 0 | 0 |
| `dt_client_media_send_groups_abandoned_total` increase (fleet) | 0 | 0 |
| `dt_client_media_keyframes_produced_total` increase by `source` (fleet) | 0 for every source | 0 for every source |
| `dt_client_media_slots_unfulfilled` at window end | 0 | 0 |

Queries: every one was evaluated at the window's end. The range `[R]` is the window length rounded
to whole seconds: `[30s]` for see-all, `[40s]` for over-subscription. Ranges are per instance unless
marked fleet.

| Row | PromQL |
|---|---|
| Peak edges | `max_over_time(mh_media_egress_edges[R])` |
| Ceiling | `mh_media_egress_stream_ceiling` |
| Group resets | `sum by(instance)(increase(mh_media_group_resets_total[R]))` (by `reason` too: every reason 0) |
| Pending at end / max | `mh_media_edges_pending` / `max_over_time(mh_media_edges_pending[R])` |
| Admission refusals | `sum by(instance)(increase(mh_media_stream_admission_total{outcome!="admitted"}[R]))` |
| Client abandoned (fleet) | `sum(increase(dt_client_media_send_groups_abandoned_total[R]))` |
| Client keyframes (fleet) | `sum by(source)(increase(dt_client_media_keyframes_produced_total[R]))` |
| Unfulfilled gauge | `dt_client_media_slots_unfulfilled` |

"Max in the window" for `mh_media_edges_pending` is scrape-sampled. A pending episode shorter than
the scrape interval (`infra/kubernetes/observability/prometheus.yml`) is invisible to it. The
per-context overlay reading is the complement.

A range wider than the window reaches back into the join and is not evidence about the window. An
earlier draft of this record used `[40s]` on a 30.9 s window (run 4), which showed a spurious fleet
`request` of 1.25 from join-time requests; at `[31s]` it read 0.

Notes:
- Each meeting's edges sat on a single handler (see-all on mh-0, over-subscription on mh-1). Pod-IP
  mapping at the run: mh-0 = 192.168.61.27, mh-1 = 192.168.61.36.
- Per-instance comparison is the binding one. `mh_media_egress_edges_limit` (65536) is the
  backstop, not the bound.
- The fleet client series are corroboration only. `dt_client_media_slots_unfulfilled` is
  single-writer across browsers, and client series carry no per-browser identity. The
  per-context columns above (bus and overlay) are the authority.
- Under amendment 4 there is no MH retry or aggregation traffic. Recovery load here is the
  self-keyed `directive_start` and the requested keyframes at join (the since-start column), and
  nothing during steady state.

### Rust side (env-test per-file edge sums, declared and measured)

Declared, from each file's `//! Edge-sum` header:

| File | Edge-sum |
|---|---|
| `27_mc_slot_placement.rs` | **35** (the largest; the header's breakdown sums to 35) |
| `39_mh_video_keyframes.rs` | 30 |
| `35_mc_server_mute_teardown.rs` | 14 |
| `34_mc_kek_rotation.rs` | 12 |
| `37_mh_stream_attribution.rs`, `38_mh_video_forwarding.rs` | 9 each |
| `26_mh_quic.rs` / `36_mc_media_kind_labels.rs` | 4 / 2 |
| `28_mh_egress_admission.rs` | exempt (sized from the live ceiling by `fixtures::egress_admission::size_s9_meeting`) |

**Measured, over this task's whole Layer 7 run** (both suites, 15:32:15 → 15:56:51 UTC), from
`max_over_time(mh_media_egress_edges[1476s])` at the run's end, scrape-sampled:
- mh-0 peaked at **24**;
- mh-1 peaked at **45**. It held 45 from about 15:39:15 to 15:39:25, then 40.

The peak falls in the Rust env-test phase (the browser suite began about 15:47:30). It is
**unattributed to a file**: the env-test log carries no per-file timestamps. It exceeds the largest
declared header (35), so it is either the exempt file 28 or edges retained from an earlier file. It
is 45 / 135 = **33.3 %** of that instance's ceiling.

Against the deployed ceiling of 135 per instance:
- the largest declared file is 25.9 % and the measured peak is 33.3 %;
- the per-file cap `MAX_PER_FILE_EDGE_SUM` = 72 plus `RETAINED_EDGE_FLOOR` = 39
  (`crates/env-tests/src/fixtures/egress_admission.rs`) gives the budget's design point of 111, which
  is 82.2 %.

All three are below the story-blocking condition (≥ 90 % of the ceiling).

**Recorded disagreement:** the task prompt expected file 27 at "≤72 by formula, about 60 emitted",
and "about 120" for the ceiling. The tree's header says 35, the measured Layer-7 peak on any one
instance is 45 (unattributed), and the live gauge says 135. These are recorded as the brief
requires.

### CI Chromium job's component-tier VP8 probe

`packages/web-app/src/__tests__/vp8Probe.test.ts` is the test `ci-client.yml`'s `component` job
runs. It ran here under the same vitest browser config (`pnpm --filter @darktower/web-app exec
vitest run --config vitest.config.ts src/__tests__/vp8Probe.test.ts`); result: 1 passed.
- **Method** (sampler outside git): a wrapper ran that command and read `/proc` every 50 ms. It
  kept every process whose argv0 matched `chrom`, recorded its last-seen cumulative utime+stime per
  `--type=`, and noted the span between its first and last sighting.
- **Result:** the cumulative CPU of the probe's Chromium processes over their 0.30 s life,
  **startup included**, was renderer 0.15 s, browser 0.10 s, utility 0.07 s and gpu-process 0. The
  test body itself reported about 8 ms, so most of this is process startup, not the encode and
  decode.

**Gap, not estimated:** the CI runner's own figure could not be read from the devloop. There is no
`gh` authentication here, and the CI job does not record process CPU. To close it, either add a
`/proc` sampler around that step in `ci-client.yml` (infrastructure-owned), or read a CI run's
resource usage from the GitHub UI.

### Verdict

**The cohort fits, on this host, at the deployed test profile.**
- No slot lost liveness in any round.
- No video slot was unfulfilled at window end.
- The story-blocking condition is peak edges at or above 90 % of the instance's own ceiling. Peak
  was 17.8 % (see-all) and 11.9 % (over-subscription), and 33.3 % at the measured Layer-7 peak.
- Renderer CPU per context was 11.9–17.3 % of one core (11.5–18.2 % across runs 3–5). The shared
  GPU process was the largest single consumer, at 51.0–62.5 %.
- Recovery activity in steady state was zero: no groups abandoned, no resets, nothing pending.

Nothing in the profile, the ceiling or the cohort was changed to obtain this. **No story-blocking
finding.**

---

## Run record

### Run 1 — 2026-10-24, headless story runner (task 21), automated equivalents only

**Who and where.** This was a headless devloop in a Linux container on the devloop host (see
[Capacity evidence](#capacity-evidence)). It had no Windows Chrome, no camera, no audio device and
no human.
- It ran against the **devloop** Kind cluster `devloop-user-see-each-other`, not the static
  topology this plan targets.
- Commit `bc82880f`. Org `e2e-f3484678dba11666`, provisioned per run by Layer 7 Phase 1h.
- Logs are kept outside git; the relevant figures are quoted here.

**Layer 7** (`./scripts/layer7.sh`, the full lane):
`LAYER=7 … DURATION=1476 RESULT=OK REASON=env-tests-passed`; `STATUS=OK REASON=browser-e2e-passed`.
- Rust env-tests: **373 passed, 0 failed** (4 ignored).
- Browser suite: **29 passed (8.8 min)**.

| Step | Evidence run here | Result |
|------|-------------------|--------|
| Phase 0: preflight; the dev-only gate | Not run against the static cluster (devloop topology, as story 2 Run 1). The gate and bundle tests (`bundle-content.test.ts`, `testDefines.test.ts`) are web-app's node tier, which Layer 4 runs, not Layer 7. Layer 7's VP8 probe passed (`[e2e] VP8 substrate probe OK`, at the deployed profile). | Probe **PASS** (automated). Preflight on WSL2: **NOT RUN** (human-only) |
| Phase 1: see and hear by content | `multi-party-see.spec.ts` "every receiver sees every other sender at three layers and hears them (N=3, M=3)" | **PASS** (automated) |
| Phase 2: publish mixes and the refusal | `publish-mixes.spec.ts`, all 3 tests | **PASS** (automated) |
| Phase 3: truncation | `video-over-subscription.spec.ts` | **PASS** (automated). The under-fill copy: **NOT RUN** (human-only; no automated assertion) |
| Phase 4: client mute | `client-mute-video.spec.ts` | **PASS** (automated) |
| Phase 4: server mute, unmute resume, notice | `server-mute-video.spec.ts` | **PASS** (automated) |
| Phase 4: non-host refused | `server-mute.spec.ts` "a NON-host server-mute request is REFUSED by MC, and the refused client stays joined" (audio); `35_mc_server_mute_teardown.rs::test_non_host_server_mute_is_refused_and_counted` | **PASS** (automated). Video-kind browser refusal: **gap** (no spec) |
| Phase 4: patched client under server video mute | `35_mc_server_mute_teardown.rs::test_video_server_mute_withdraws_drops_a_patched_publisher_and_unmute_restores_it`; `37_mh_stream_attribution.rs::test_per_stream_attribution_on_the_datagram_path` | **PASS** (automated; automation-only by nature) |
| Phase 5: watched-only | `watched-only-video.spec.ts`; `27_mc_slot_placement.rs::test_pairing_stability_and_watched_only_video_directive` | **PASS** (automated) |
| Phase 6: rotation with video; late joiner | `kek-rotation-video.spec.ts`; `late-joiner-video.spec.ts`; `34_mc_kek_rotation.rs`; `39_mh_video_keyframes.rs::test_on_demand_keyframes_and_recovery` | **PASS** (automated). Leaver lockout `videoReceiver.test.ts` is SDK unit tier, not run by Layer 7 |
| Phase 7: camera revoked | No browser or env-test equivalent | **NOT RUN** (human-only). **Gap:** automation is unit/component only |
| Phase 8: video cap 0 | `video_slots_integration.rs` (MC integration tier, Layer 4; not run by Layer 7) | **NOT RUN** here (human-only lever) |
| Phase 9: debug options; nothing exported | `debugNeverExported.test.ts` and the component tests (Layer 4 tiers) | **NOT RUN** here (human-only) |
| Phase 10: production profile, real cameras | None by nature. Profile integrity: `01_mh_deployment_config.rs::test_kind_media_codec_profile_is_the_committed_test_profile` and `02_mc_deployment_config.rs::test_mc_and_mh_resolve_one_media_codec_profile_generation` | Integrity **PASS** (automated). Real-camera pass: **NOT RUN** (human-only) |
| Phase 11: story-2 at M = 0 | `multi-party-hear.spec.ts`, `partial-connectivity.spec.ts` (S10a), `server-mute.spec.ts` (3), `kek-rotation.spec.ts`, `over-subscription.spec.ts`, `solo-participant.spec.ts` (2), all at `SUITE_VIDEO_RECEIVE_SLOTS = 0` | **PASS** (automated) |
| Phase 12: recovery after abandonment | `39_mh_video_keyframes.rs::test_on_demand_keyframes_and_recovery` (phases (d)–(f)) | **PASS** (automated). Human induction: **gap** (no human-reachable method in the tree) |
| R-28: unusable VP8 fails loudly | `video-codec-unavailable.spec.ts` | **PASS** (automated) |
| R-28: capacity evidence | `capacity.evidence.ts`, both shapes | **PASS**, recorded above; cohort fits |

**Steps that failed:** none in this run. The harness's own first two evidence attempts failed
loudly as designed, and were fixed in the harness before runs 3–5 (run 5 is the record):
- the renderer type was unreadable from rewritten child titles: `HARNESS: … fingerprint is
  ambiguous or absent`;
- a 0 ms unfulfilled reading was misread as unfulfilled.

No product step failed.

**Gaps raised to the lead** (state, and what would close each):
1. **Phase 12 induction.** No human-reachable induction exists. Either expose the SDK's per-target
   backlog cap through a dev-only build define in web-app (client), or document a verified host
   shaping procedure for a publisher whose egress `tc` can shape (operations).
2. **Phase 7.** There is no browser equivalent for camera revocation. Close it with a Playwright
   spec that ends the camera track mid-call (`forceCameraCapture`) and asserts `avatar_client_muted`
   at peers.
3. **Video-kind non-host refusal.** There is no browser spec; the audio spec and env-test 35 cover
   the wire refusal.
4. **CI-runner VP8 probe CPU.** It is not readable from the devloop (see Capacity evidence).
5. **Phase 10 "two machines".** This is not available per `dev-web.sh` (WHY NOT A SECOND MACHINE).
   The plan uses two cameras on one machine.

**Outcome. This run does NOT satisfy R-34 on its own.** The human pass, **Run 2 — NOT RUN —
pending before `/close-story`**, must be run on Windows Chrome over mirrored networking against the
static cluster, and recorded here. It covers:
- Phase 0 on WSL2;
- every seeing, rendered-UI and notice check;
- Phases 7–10 (the camera revocation, the video-off lever, the debug panel, the production-profile
  real-camera pass with headphones, and every physical camera-light check);
- the Phase 11 re-run of story 2's Phases 1–4.

The human records the preflight's slot line, the derived grids, each checkbox's result, and the
open-objective panel readings from Phase 1 (observed, never gated). Those readings are the first
real-browser measurements toward the `docs/observability/slos.md` open item.
