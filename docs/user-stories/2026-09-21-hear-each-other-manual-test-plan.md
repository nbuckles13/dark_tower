# Manual Test Plan — Hear Each Other, the N+1 Demo (pre-close validation)

**Story**: [2026-09-21-hear-each-other](2026-09-21-hear-each-other.md) (R-32; story task 19)
**Date written**: 2026-09-30
**Status**: Written. Automated equivalents run 2026-09-30 (see [Run record](#run-record)). The
human pass on Windows Chrome is **NOT RUN — pending before `/close-story`**.
**Purpose**: Before `/close-story`, a human covers what automation structurally cannot see. That means
three things. Hearing is by ear, on Windows Chrome over mirrored networking. There is a
real-microphone pass, with headphones. And the rendered UI is looked at by a person, whereas the
browser suite reads wire tokens and the dev-only event bus. The plan also walks the dashboards,
which only exist as a whole when someone reads them. Every step names its automated equivalent,
so a human-only failure can be told apart from a regression that the suite should already have
caught.

References:

- `docs/runbooks/client-dev-local.md`: bring-up §3, media triage ladder §4.5, and failure modes
  F16 (silence after a leave) and F17 (static fill). F15, F18 and F19 are adjacent.
- `scripts/dev-web.sh --help`: the N+1 bring-up, the test tone, and why not a second machine.
- `packages/web-app/e2e/README.md` §What the multi-party specs assert: the automated scenarios this
  plan mirrors.
- `docs/runbooks/mc-incident-response.md`: Scenario 16 (rotation arm), 18 (hears only part of the
  roster), 19 (KEK rotation stalled), 20 (meeting teardown failing / MH budget ratchet) and 21
  (server mute not enforced); and `docs/runbooks/mh-incident-response.md` Scenario 18 Arm B
  (unreclaimed edges) and Scenario 19 (server mute not taking effect at ingress).

**Result convention.** Mark each checkbox. Note anomalies inline under the step. A failed step is
recorded as FAIL with what was seen, never softened into a pass-with-notes. Any defect routes
through `/devloop` before story close.

**Cite, don't restate.** This plan writes no literal N, no W, no caps and no tone frequencies. N is
what `dev-web.sh` exports (`VITE_DT_RECEIVE_SLOTS`) and echoes in its preflight. The server cap is
`MC_MAX_RECEIVE_SLOTS`. W is `MC_KEK_ROTATION_DEBOUNCE_SECONDS` and the disconnect grace is
`MC_DISCONNECT_GRACE_PERIOD_SECONDS`, all in `infra/services/mc-service/config.env`. Record the
values *with the run*, not in the steps.

---

## What the demo proves — and what it does not

**Proves (routing, attribution, server-mute enforcement, rotation on leave):**

- **Routing.** MC fills each receiver's slots in join order from the senders it shares a connected
  handler with. The handlers forward exactly those edges.
- **Attribution.** A tone heard in the cell naming B came from the sender whose identity key MC's
  roster lists for B. The frame's key-id sender component resolves to that roster entry, and the
  frame verifies under it. This is routing and attribution correctness **given an honest MC**. It
  is not proof of who B is (see below).
- **Server-mute enforcement.** A host's server mute silences B at every receiver while B's client
  keeps sending. MH drops B at ingress (`server_muted`). B cannot lift it. A non-host is refused.
- **Rotation on leave.** A leave rotates the meeting KEK. The remaining participants keep hearing
  each other across the switch, and a joiner after the rotation opens current frames.

**Does not prove:**

- **The crypto.** Every participant in this demo runs the same TypeScript stack, so an error that
  is consistent on both sides (key schedule, framing, AAD) still sounds right. The cross-language
  vectors (`crates/media-vector-gen`, `proto/test-vectors/frame-v2.vectors.json`, and the pinned
  SFrame-WG anchor under `proto/test-vectors/external/sframe-wg/`) remain the **only** independent
  check of it. That includes the verify/decrypt behind attribution: it runs on the same TypeScript
  stack, so a signing error that is consistent on both sides would not show up here either.
- **Anything about who the other participants are.** Roster keys are trust-on-first-use, as
  distributed by MC. Nothing in this demo authenticates participants to each other, and no step
  here may be read as doing so. Attestation is a separately-sequenced story.
- **Leaver lockout.** No step observes the leaver failing to open post-rotation frames; the
  leaver's context is gone. That is asserted only by the client integration test
  `packages/sdk-core/src/media/lifecycle/__tests__/audioPipeline.multiSender.test.ts` (S7: a leaver
  holding only the old KEK cannot open post-rotation frames), cited in Phase 4.
- **Forward secrecy or end-to-end properties.** A leave rotation bounds a leaver's exposure to the
  window the story states. It is not forward secrecy.
- **Scale, cross-machine or cross-network behaviour.** Every context runs on one machine against
  the static Kind cluster.

---

## Phase 0 — Cold start (WSL2, the STATIC cluster)

The human pass runs against the **static** Kind topology, with host ports from
`infra/kind/kind-config.yaml`. A devloop cluster is a different topology with different ports, and
`dev-web.sh` does not target it.

```bash
unset DT_HOST_GATEWAY_IP                                  # F8: a stale export rewrites advertise addrs
./infra/kind/scripts/setup.sh                             # several minutes on first run
DT_TEST_TONE=1 DT_TEST_LEVERS=1 scripts/dev-web.sh --check   # preflight only
DT_TEST_TONE=1 DT_TEST_LEVERS=1 scripts/dev-web.sh           # leave running
```

- `DT_TEST_TONE=1` makes the build send a constant per-participant tone in place of the
  microphone. The tone is a build define, `__DT_TEST_TONE__`, so one microphone never feeds back
  across N tabs.
- `DT_TEST_LEVERS=1` enables the per-context test levers used in Phases 3 and 8
  (`src/lib/testLevers.ts`).
- Both are **build-time** defines, and both accept exactly `1` (`vite/testDefines.ts`). A reload
  does not change them. To change either, stop the server and re-run it.

Checks:

- [ ] Preflight green. Every ✗ is a hard fail, including missing cert fingerprints and a WebTransport
      listener check that cannot run. Stop and fix; never work around one.
- [ ] **Copy the preflight's `receive slots: effective N=… (…) <= server cap …` line into the run
      record.** That is the N for this run. The count of participants below is N+1, **not** a fixed
      count; the launcher default is `DEMO_RECEIVE_SLOTS` in `scripts/dev-web.sh`.
- [ ] Preflight shows `test tone: ON requested` and `test levers: ON requested`.
- [ ] After the first context loads (any view), its console
      `ev.filter(e => e.type === 'buildKnobs').at(-1)` shows `testLevers: true` and
      `telemetrySinkActive: true`. If either is false, **stop and relaunch the dev server** with the
      right environment. A reload does not fix it, and without it Phase 3's non-host step shows no
      button and Phase 8's block is ignored. That is a setup error, not a product failure.
- [ ] If Chrome fails but WSL2 `curl -I http://127.0.0.1:5173` works, it is a mirrored-networking
      regression (F9). Fix `.wslconfig` before going on.

Access points on the static cluster are the Prometheus and Grafana hostPorts in
`infra/kind/kind-config.yaml`. Grafana is `admin/admin`, so use it only on a trusted network
(runbook §1). Loki is reached through Grafana Explore.

### Browsing contexts and accounts

- **N+1 browsing contexts on this machine**, one per participant. Separate **Chrome profiles** are
  the known-good shape; create demo profiles for this and remove them afterwards.
  - The auth session is per-context, in-memory state (`packages/web-app/src/App.svelte`). It is
    **not** shared between tabs, and a reload loses it, so you sign in again.
  - Whether N+1 simultaneously-capturing tabs in **one** profile work is **unverified**.
  - A second machine is not an option. `http://demo.localhost:5173` is a secure context, and a
    LAN address is not.
- **Each context is a DISTINCT registered account, with a distinct display name** matching its
  label: A, B, C, D… for the first N+1; E, the extra joiner; F, the late joiner.
  - Slot cells name the sender through the roster, so two accounts sharing a name make the
    attribution check vacuous.
  - Budget N+3 profiles and accounts at most. F may reuse C's profile once C has left.
  - To create each account: in its profile, open `http://demo.localhost:5173`, use the **Sign up**
    nav with a distinct email and the label as the display name, and keep the prefilled `demo` org.
    `setup.sh` seeds that org (`client-dev-local.md` §3 Step 1).
- Keep every window visible and unminimised.

Evidence available in every context (dev builds carry the E2E bus). Open DevTools → Console:

```js
const ev = window.__darktower_test__.events;
ev.filter(e => e.type === 'captureSource').at(-1)      // own mode + toneHz (tone ON?)
ev.find(e => e.type === 'joined')                        // own senderId, offered mediaServers
ev.filter(e => e.type === 'mediaConnected').map(e => e.mhUrl)   // handlers ACTUALLY connected
ev.filter(e => e.type === 'receiveSlots').at(-1)         // declared N, serverCap
ev.filter(e => e.type === 'slotAssignments').at(-1)      // per-slot sender + wire state
ev.filter(e => e.type === 'buildKnobs').at(-1)           // testLevers: true?
```

---

## Phase 1 — N+1 participants hear each other

A signs in and **creates** the meeting, which makes A the host. The host is the creator
(`gc-service` `handlers/meetings.rs`). The others sign in and join A's code, in label order.

**Before D clicks the Join nav**, set the non-host lever in D's console. It is read once, when the
join view mounts (`JoinMeeting.svelte`, `readTestLeversForSession`):

```js
window.__dt_test_levers__ = { forceHostControls: true }
```

This renders the host affordance in a NON-host context, so Phase 3 exercises MC's refusal rather
than a hidden button. A malformed lever is refused loudly, as `last-error` with no join; that is a
typo, not a product failure.

Every participant clicks **Start audio**. It needs a user gesture; until then `media-status` reads
"audio not started", and the participant sends nothing.

Checks:

- [ ] Each participant's `captureSource` shows the test tone. The `toneHz` values are pairwise
      distinct across the cohort; a collision makes the by-ear check ambiguous.
- [ ] **Record, per participant, which handlers it connected to.** Use the `mediaConnected` URLs,
      and note the `joined.mediaServers` offered set once. With both MH pods healthy in Kind, every
      participant connects to every offered handler (R-33).
  - Cross-check server-side by participant id. Take it from `joined.participantId` on that
    context's bus, then run
    `kubectl logs -n dark-tower deploy/mh-0 | grep "Connection established for registered meeting" | grep <participantId>`,
    and the same for `deploy/mh-1`.
  - The line's target is `mh.webtransport.connection` and it carries `participant_id`. The Loki
    form is `{app="mh-service"} |= "Connection established for registered meeting" |= "<participantId>"`.
- [ ] Each participant's slot grid (`slot-list`) has **exactly N cells**. Every cell is `active`
      ("receiving audio") and **names a different other participant**. No cell names self; loopback
      is removed.
- [ ] **By ear:** each participant hears the other N tones. N tones heard at once are a chord, so
      check one sender at a time. Each sender in turn clicks its own **Mute**. At every other
      participant, the chord changes when it mutes and is restored when it unmutes. The muted
      sender's cell reads `source_muted` ("the sender is muted") at each receiver while muted.
- [ ] Roster rows are all `data-reachability=reachable`, with no server-mute marks.

Automated equivalent: `multi-party-hear.spec.ts` (S1). It checks all ordered pairs at three
layers: key id at the assigned slot, verify/decrypt under the roster key, and the decoded tone.

## Phase 2 — One more than N+1: the extra joiner E

E signs in and joins, then clicks Start audio.

- [ ] At every earlier participant (A…), all N cells stay `active` and keep naming the same earlier
      joiners. E appears on their roster **in no slot and NOT marked unreachable**. This is **F17's
      static-fill signature**: the receiver's slots are full, E is the source it has no slot for,
      and no drop counter moves. It is **not** a `fewer_sources` cell at those receivers;
      `fewer_sources` appears only where a receiver has more slots than eligible senders.
- [ ] Nobody among the earlier N+1 hears E's tone.
- [ ] E's grid has N `active` cells naming the **earliest** N joiners, not the latest. E hears
      exactly those N tones: use the per-sender mute check from Phase 1.
- [ ] client-media *Receive Path - Receive-Slot Refusals (N over MC cap)* stays flat.

Automated equivalent: `over-subscription.spec.ts` (S2), which runs at a per-context N of `R_SLOTS`
(from that spec).

## Phase 3 — Server mute (A is host)

A clicks **Mute for everyone** on B's roster row (`server-mute-<B>`).

- [ ] At every other participant, B's tone disappears. B's slot reads `source_muted`, and B's roster
      row shows `participant-server-muted-<B>` ("muted for everyone by A").
- [ ] B's own view shows `server-mute-state` = "muted for everyone by A". This is **distinct** from
      `mute-state`, which stays "unmuted".
- [ ] B's own **Mute / Unmute** toggle still works independently. B's client mute changes
      `mute-state` and nothing else, and B stays server-muted throughout.
- [ ] mh-media *Ingress Drops by Reason*: `server_muted` climbs at B's frame rate while the mute
      holds. The series is pre-registered at zero, so it reads flat zero before and flat again
      after the unmute. "No data" is a broken pipeline, not zero.
- [ ] mc-media *Server Mute Requests*: `action="mute", outcome="applied"` stepped, and
      *Server-Muted Sources* shows 1 while the mute holds.
- [ ] **B tries to self-clear and cannot.** B's only affordance is `request-unmute` ("Ask the host
      to unmute you"). There is no self-clear button. B clicks it:
  - [ ] A's row for B shows `unmute-requested-<B>` ("asks to be unmuted").
  - [ ] B's `server-mute-state` stays set, and B is still silent everywhere.
  - [ ] mc-media *Unmute Requests* stepped.
- [ ] **A non-host tries to server-mute and is refused.** D (forced host controls) clicks
      **Mute for everyone** on C's row:
  - [ ] D shows `host-control-error-<C>`. D **stays joined** (`meeting-state` = `joined`) and keeps
        hearing.
  - [ ] C is **not** muted at any participant.
  - [ ] mc-media *Server Mute Requests* shows `outcome="not_permitted"` stepped.
- [ ] **A unmutes B** (`Unmute for everyone`). B's tone returns at every participant, the slot is
      `active` again, `server-mute-state` clears, and mh-media `server_muted` goes flat.
- [ ] Nothing on screen calls the mute a ban.

Automated equivalent: `server-mute.spec.ts`. It covers S3, client structural mute and its
composition with server mute, and the non-host refusal.

## Phase 4 — C leaves: no gap, refill, rotation

C clicks the **Create** nav, which unmounts the meeting view and closes the session cleanly. Closing
C's window abruptly may read to MC as a *lost* connection instead. C is then removed, and the
refill and rotation start, only when MC's disconnect grace expires
(`crates/mc-service/tests/slot_placement_integration.rs`, grace expiry drives the same re-push).

- [ ] **The remaining participants keep hearing each other with no audible gap** across the leave
      and the rotation that follows. Listen through W after the leave. An audible gap is a FAIL
      here: consult **F16**. F16 notes that a brief key-material drop burst on the *counters* right
      after a leave is expected; *sustained* drops, or a gap you can hear, are the signal.
- [ ] At A, B and D, the cell that named C now names **E** (`active`); nothing else moved (R-4).
      E's tone is now heard there.
- [ ] At E, C's cell is refilled by the **earliest-joined sender E was not hearing**.
- [ ] The rotation is visible:
  - [ ] mc-overview *Meeting KEK Issuance by Trigger*: `participant_left` steps once. It lands
        within MC's disconnect grace plus W after the leave; W is a debounce, so read both from
        `config.env`.
  - [ ] mc-media *KEK Push Outcomes*: one increment per remaining recipient, all `delivered`.
        These are the per-recipient push outcomes; `participant_gone` is the only benign
        non-delivered value.
  - [ ] mc-media *KEK Rotation Pending Age vs Overdue Threshold* rises after the leave and returns
        to zero at the rotation. It is a maximum across live meetings, so read it on an otherwise
        idle cluster.
  - [ ] client-media *KEK Rotations vs Generations Retained*: both series step together.
        Rotations with no retention is F16 rung 4.
        - **Caveat, observed in this plan's run:** the FIRST increment of a freshly-born client
          series is invisible on this panel, as its own description's series-absent semantics
          state. A new org or a collector restart can therefore read flat on correct behaviour.
        - The raw counters are cumulative since series birth, including the org's earlier activity,
          so read them only as a before/after **delta**. **Before C clicks Create**, record
          `sum(dt_client_media_kek_updates_total{source="kek_update"})` and
          `sum(dt_client_media_kek_generations_retained_total)` in Prometheus (absent = 0). Read
          both again after W. Each delta should equal the number of remaining participants. Put
          both readings in the run record.
  - [ ] The refill's evidence is each receiver's grid and bus (`slotAssignments`). MC's panels are
        aggregate rates only: mc-media *Slot States* and *Receive Slot Fill vs Cap*. *Edge Moves by
        Reason* is **not** refill evidence. It counts edges that changed *handler*, and in a
        healthy leave it stays flat on `unexpected`.

Automated equivalents:

- `kek-rotation.spec.ts` (S6): the remaining clients' KEK generation advances, and a later joiner
  opens current frames.
- Rotation on leave, against the cluster: `crates/env-tests/tests/34_mc_kek_rotation.rs`.
- Debounce coalescing: `crates/mc-service/tests/kek_rotation_integration.rs`.
- Leaver lockout: `packages/sdk-core/src/media/lifecycle/__tests__/audioPipeline.multiSender.test.ts`
  (S7). A departed browser tears down, so this is a client integration test (R-17), not a cluster
  test.

The refill itself is MC's slot logic, tested in `crates/mc-service/tests/slot_placement_integration.rs`
(R-4: a middle leave refills only the freed slot).

## Phase 5 — A late joiner after the rotation hears everyone

First make room, so the late joiner's N slots cover everyone. **E leaves** (Create nav), which
leaves N participants. This is one more leave-triggered rotation. Wait for it on *Meeting KEK
Issuance by Trigger*.

**F** joins after that rotation and clicks Start audio. F may be C's profile rejoining; a rejoin is
a fresh join with a fresh participant id and sender id.

- [ ] F's N cells are all `active` and name every remaining participant. F **hears all N tones**,
      checked per sender with the mute check.
- [ ] Every remaining participant has F in a cell and hears F's tone.
- [ ] No sustained key-material drops at F: client-media *Receive Path - Frame Drops by Reason*.
      A joiner after a rotation holds only the current generation, so opening everyone's frames
      proves the senders re-wrapped under it.

Automated equivalent: `kek-rotation.spec.ts`, the joiner after rotation.

Without the extra leave, F would be the N+2-th participant. F17 static fill would then mean F hears
only the earliest N, which is designed behaviour, not a failure of this step.

## Phase 6 — The meeting ends: edge occupancy falls

All remaining participants leave.

- [ ] mh-media *Egress Edges vs Edge-Limit Backstop*: `mh_media_egress_edges` falls by this
      meeting's edges on every instance once MC has ended the meeting. That is to zero if no other
      meeting is live, and it happens after the disconnect grace for any lost connection.
      *Installed Egress Streams vs Stream Ceiling* is the companion panel.
- [ ] mh-media *Meeting Teardowns by Outcome*: `released` stepped, **and** mh-media *Registered
      Meetings vs Registration Cap* falls by this meeting on its instance. The two together are the
      teardown proof (`docs/observability/metrics/mh-service.md`); `released` alone only says the
      release was decided.
- [ ] If edges **stay** up with no live meeting, that is a failure here; record it. The procedure is
      `docs/runbooks/mh-incident-response.md` Scenario 18 Arm B, and the teardown cause is
      `docs/runbooks/mc-incident-response.md` Scenario 20.

## Phase 7 — Solo participant hears silence (loopback removed)

In a **fresh** meeting, one participant joins alone and starts audio.

- [ ] Every slot cell reads `fewer_sources` ("no one to fill this slot yet"), as an under-filled
      grid with no spinner. No cell names self.
- [ ] Silence, and `media-status` stays "no audio from other participants yet".
- [ ] mc-media *Send Directives by Outcome*: `emitted_empty_targets` moves. The solo sender is
      directed to send nothing, so MH's drop panels stay flat. Flat is the expected reading here,
      not evidence of anything.

Automated equivalent: `solo-participant.spec.ts`.

## Phase 8 — OPTIONAL: partial connectivity (the client handler lever)

This needs `DT_TEST_LEVERS=1`, which Phase 0 set. Use a fresh meeting with three participants.

1. A joins with no lever. In A's console, read the offered URLs:
   `__darktower_test__.events.find(e => e.type === 'joined').mediaServers`. Call them `u0` and `u1`.
   MC says order carries no meaning, so pick by URL, never by position. Which pod serves each URL
   is incidental.
2. **Before** B clicks the Join nav: `window.__dt_test_levers__ = { blockHandlers: ['<u1>'] }`. B is
   on u0's handler only.
3. **Before** C clicks the Join nav: `window.__dt_test_levers__ = { blockHandlers: ['<u0>'] }`. C
   is on u1's handler only.

A URL MC did not offer, or blocking every offered handler, is refused loudly: `last-error` shows,
and the session disconnects.

- [ ] `mediaConnected`: A = both, B = {u0}, C = {u1}. Record them.
- [ ] **A hears both B and C.**
- [ ] **B and C each hear only A.** Each sees the other marked unreachable on the **roster**
      (`participant-unreachable-<id>`, "the sender cannot be reached"), in no slot. B's and C's
      spare cells read `fewer_sources`.
- [ ] mc-media *Unreachable Senders Named* moves.

Automated equivalents:

- `partial-connectivity.spec.ts` (S10a).
- Below the browser: `crates/env-tests/tests/27_mc_slot_placement.rs`, the canonical partial
  connectivity across two handlers, with a Rust mock client.

## Phase 9 — Real microphone, headphones (human audibility)

Stop the dev server and re-run it **without** `DT_TEST_TONE`: `scripts/dev-web.sh`. The preflight
must show `test tone: OFF`. The tone is a build define, so a reload is not enough.

Use two profiles, A and B, on one machine with **headphones**. Every context captures the same
physical microphone, so **Mute** every context except A.

- [ ] Speak. B hears A's voice clearly in the headphones, with no echo. B's cell names A.
- [ ] A hears nothing of itself; loopback is removed.
- [ ] client-media *Send Path - Capture Source (SAFETY: test_tone outside dev)* reads the
      microphone mode for this session.

---

## Dashboards to read (titles verified in `infra/grafana/dashboards/`)

Dashboards are named here by uid:

- `client-media` = *Client SDK - Media Path*
- `mc-overview` = *Meeting Controller - Overview*
- `mc-media` = *Meeting Controller - Media Path*
- `mh-media` = *Media Handler - Media Path*

| What | Dashboard → panel |
|------|-------------------|
| Rotation by trigger | mc-overview → *Meeting KEK Issuance by Trigger* |
| Per-recipient push outcomes | mc-media → *KEK Push Outcomes*; policy pushes: *Media Policy Pushes by Outcome* |
| Rotation lag, coalescing, failures | mc-media → *KEK Rotation Pending Age vs Overdue Threshold*, *KEK Departures Coalesced per Rotation*, *KEK Rotation Failures* |
| Client side of rotation | client-media → *KEK Rotations vs Generations Retained*, *Key and Mute Lifecycle* (see the first-increment caveat in Phase 4) |
| MH `server_muted` ingress drops | mh-media → *Ingress Drops by Reason* |
| Server-mute decisions | mc-media → *Server Mute Requests*, *Server-Muted Sources*, *Unmute Requests* |
| Edge occupancy after a meeting ends | mh-media → *Egress Edges vs Edge-Limit Backstop*, *Installed Egress Streams vs Stream Ceiling*, *Meeting Teardowns by Outcome*, *Registered Meetings vs Registration Cap* |
| Slot fill / refill (aggregate only; per-receiver truth is the grid) | mc-media → *Slot States*, *Receive Slot Fill vs Cap*, *Receive Slot Cap* |
| Handler moves (should be flat on `unexpected`) | mc-media → *Edge Moves by Reason* |
| Receive health | client-media → *Receive Path - Frames Received / Accepted*, *Receive Path - Frame Drops by Reason*, *Receive Path - Silent Active Sources (deficit)* |
| Tone vs microphone | client-media → *Send Path - Capture Source (SAFETY: test_tone outside dev)* |

`dt_client_*` series reach Prometheus because the launcher defaults `VITE_TELEMETRY_ENDPOINT`. If
the client-media board is empty, work F16 rung 1 (a dead pipe) first. Client series are fleet
aggregates with no participant label, so per-participant claims come from that participant's
screen and bus, never from a client panel.

## Triage on failure

- **F16** (`client-dev-local.md`): I could hear everyone, then went silent after someone left.
  Leave-correlated silence is the rotation signature. The remedy is `mc-incident-response.md`
  Scenario 16, rotation arm. The immediate unblock is a **rejoin**: click the Create nav, then Join,
  and enter the same code. A page reload also works, but it drops the in-memory sign-in, so you sign
  in again.
- **Rotation never happened.** `participant_left` never steps, or *KEK Rotation Pending Age vs
  Overdue Threshold* keeps climbing. This is `mc-incident-response.md` Scenario 19 (KEK rotation
  stalled), not F16: F16 covers a rotation that happened but did not reach the client.
- **F17**: I can hear some people but not all. This is static fill, and it is the designed
  behaviour at more than N+1 participants. It is not static fill if a cell says fewer sources while
  senders are unassigned, or if a participant is marked unreachable. For the second case, see MC
  Scenario 18.
- Adjacent failure modes:
  - F15: sustained key-material drops at a join.
  - F18: an over-cap N hears nobody.
  - F19: a source MC calls active, of which nothing decodes.
  - Server mute not enforced (Phase 3). There are two causes, told apart by
    `mc_media_server_muted_sources` (mc-media *Server-Muted Sources*):
    - MC decision/push side: `mc-incident-response.md` Scenario 21.
    - MH ingress side: `mh-incident-response.md` Scenario 19.
  - Edges not released after the meeting ends (Phase 6): MH Scenario 18 Arm B and MC Scenario 20.

---

## Promote-to-E2E verdicts

| Plan step | Automated today | Human-only residue |
|-----------|-----------------|--------------------|
| N+1 hear each other | `multi-party-hear.spec.ts`, all pairs at 3 layers | By-ear on Windows Chrome; rendered names |
| Extra joiner (static fill) | `over-subscription.spec.ts` at `R_SLOTS` | Rendered grid at the launcher's N |
| Server mute / self-clear / non-host refusal | `server-mute.spec.ts` | Rendered indicators; audible disappearance |
| Leave → refill → rotation, no gap | `kek-rotation.spec.ts` (rotation + joiner); refill in `slot_placement_integration.rs` | **The refill at a browser receiver and "no audible gap" are not asserted by any browser spec** |
| Late joiner hears everyone | `kek-rotation.spec.ts` | By ear |
| Edge occupancy falls after end | MH/MC teardown integration tests | The dashboard reading |
| Solo silence | `solo-participant.spec.ts` | Rendered under-filled grid |
| Partial connectivity | `partial-connectivity.spec.ts`, `27_mc_slot_placement.rs` | Rendered unreachable mark |
| Real microphone | none, by nature | All of it |

Candidate for a later story, not this one: a browser leave-and-refill spec that asserts that at
N+2, a leave's freed slot at every receiver is refilled by the earliest unassigned sender, with
the other slots' accepted counts advancing in the same window.

---

## Run record

### Run 1 — 2026-09-30, headless story runner (task 19), automated equivalents only

**Who and where.** This was a headless devloop in a Linux container. It had no Windows Chrome and no
audio device, and there was no human.

- It ran against the **devloop** Kind cluster `devloop-hear-each-other`, not the static topology
  this plan targets. The devloop cluster uses the helper's dynamic ports
  (`dev-cluster status` / `/tmp/devloop/ports.json`).
- Both MH pods and both MC pods were Running.
- Evidence logs from the run are outside git; the relevant output is quoted here.

**N the launcher echoed.**
`DT_TEST_TONE=1 scripts/dev-web.sh --check` printed:

```
✓ receive slots: effective N=3 (launcher demo default) <= server cap MC_MAX_RECEIVE_SLOTS=8 (infra/services/mc-service/config.env)
test tone: ON requested (DT_TEST_TONE=1; vite.config.ts accepts only 1 and fails the launch otherwise)
```

So N=3 and the cohort is N+1=4. The live cap agrees: Prometheus `mc_media_receive_slot_cap` reads
the same on both MC instances.

| Step | Evidence run here | Result |
|------|-------------------|--------|
| Phase 0 — preflight | `scripts/dev-web.sh --check` in this container | **FAIL** (exit 1). ✗ AC not answering on 127.0.0.1:8443; ✗ GC on :8444; ✗ mc-0/mc-1/mh-0/mh-1 `CANNOT VERIFY — … 'ss' is not installed`. Cause: the environment, not the product. This container is not the static topology (no host ports 8443/8444; `iproute2` absent). Hard fails by the script's own policy, and correctly so. The human must run Phase 0 on WSL2. |
| Phase 0 — page renders in Windows Chrome | — | **NOT RUN** (human-only, pending before `/close-story`) |
| Phase 1 — N+1 hear each other (routing, attribution, tone) | `multi-party-hear.spec.ts` (S1: N=3, 4 participants, 12 ordered pairs at 3 layers) | **PASS** (automated) |
| Phase 1 — per-participant handlers connected | Every multi-party browser join waits for a `mediaConnected` on EVERY offered handler (`cohortContexts.ts:joinMember` → `fixtures.ts:waitForAllMediaConnected`), and S10a asserts at least two handlers are offered. Prometheus `increase(mh_webtransport_connections_total{status="accepted"}[20m])` read 146 on EACH of the two MH instances, with 0 rejected and 0 error. | **PASS** (automated): every automated participant connected to both handlers; the S10a lever contexts were deliberately narrowed. Per-participant record for the human cohort is **NOT RUN**. |
| Phase 1 — by ear; rendered names in the grid | — | **NOT RUN** (human-only) |
| Phase 2 — extra joiner / static fill | `over-subscription.spec.ts` (S2, per-context N=2; run ×4 incl. repeats) | **PASS** (automated), at N=2, not the launcher's N. By-ear and rendered grid at the launcher's N: **NOT RUN** (human-only) |
| Phase 3 — server mute, B's own view, self-clear refused, non-host refused, unmute restores | `server-mute.spec.ts`, all 3 tests (run ×4 incl. repeats). Prometheus over the first pass: `mc_media_server_mute_requests_total` `mute/applied` +5, `mute/not_permitted` +3, `unmute/applied` +3; `mc_media_unmute_requests_total{outcome="relayed"}` +2; `mh_media_frames_dropped_total{reason="server_muted",direction="ingress"}` +997 | **PASS** (automated). Audible disappearance and rendered indicators: **NOT RUN** (human-only) |
| Phase 4 — C leaves: rotation | `kek-rotation.spec.ts` (S6). Prometheus: `mc_meeting_kek_generated_total{trigger="participant_left"}` +2; `mc_meeting_kek_pushes_total{outcome="delivered"}` +4, every other outcome 0. Client raw counters: `dt_client_media_kek_updates_total{source="kek_update"}` = 2 and `dt_client_media_kek_generations_retained_total` = 2. `sum(increase(…[15m]))` of both read **0**; this is the first-increment caveat recorded in Phase 4. Pending age back at 0. | **PASS** (automated) |
| Phase 4 — refill of C's slot by E at a browser receiver; no audible gap | No browser spec asserts either (see Promote-to-E2E). The refill is covered below the browser by `slot_placement_integration.rs`, which was not run in this devloop. | **NOT RUN** (human-only). **Gap:** there is no automated browser equivalent. |
| Phase 5 — late joiner after rotation | `kek-rotation.spec.ts`: the joiner after the rotation opens every remaining sender at 3 layers | **PASS** (automated). By ear: **NOT RUN** |
| Phase 6 — edges fall after meeting end | Prometheus after the suite: `mh_media_egress_edges` = 0 on both instances; `mh_media_meeting_teardowns_total{outcome="released"}` +66, `rejected_ownership`/`unknown_meeting` 0 | **PASS** (automated read). Dashboard reading: **NOT RUN** |
| Phase 7 — solo silence | `solo-participant.spec.ts`, both tests; `mc_media_send_directives_total{outcome="emitted_empty_targets"}` moved | **PASS** (automated). Rendered grid: **NOT RUN** |
| Phase 8 — partial connectivity (optional) | `partial-connectivity.spec.ts` (S10a); `mc_media_unreachable_senders_total` moved | **PASS** (automated). Human lever run: **NOT RUN** |
| Phase 9 — real microphone with headphones | — | **NOT RUN** (human-only, pending before `/close-story`) |
| F16 reads | Rung 1: `dt_client_media_frames_sent_total` present (3 series). Rung 2: no `dt_client_media_frames_dropped_total` series exists, so no client drop was recorded (a dead pipe is ruled out by rung 1). Rung 3/4 as in the Phase 4 row. | Read; nothing indicates F16 |
| F17 reads | `mc_media_receive_slot_cap` on both MC; `mc_media_receive_capability_declarations_total{outcome="slot_count_over_cap"}` = 0; `dt_client_media_receive_slots_rejected_total` increase 0 | Read; consistent with static fill |
| Alerts | `ALERTS{alertstate="firing"}` empty after the suite | Clean |
| Dashboards | Grafana lists all four (client-media, mc-overview, mc-media, mh-media); every panel title cited above exists in `infra/grafana/dashboards/` | Present |

**Full browser suite.** `pnpm exec playwright test` ran through the Layer-7 wiring, with a per-run
org and `E2E_*`/`VITE_*` taken from ports.json.

- **First pass** (before review): **17 passed (3.9 min)**. A repeat pass of two of the specs gave
  **12 passed**.
- **Final code** (after the review fixes: the raced hard bound, the same-page mover rule, and
  `expectCountersFlatOverWindow` on the same stop rule):
  - The full suite again: **17 passed (3.7 min)**.
  - `--repeat-each=3` over **every** spec using `observeWindow` (`server-mute`,
    `over-subscription`, `partial-connectivity`) plus `solo-participant`, which exercises
    `expectCountersFlatOverWindow`: **21 passed (2.8 min)**.
  - No `HARNESS:` failure in any run.

The spec-level PASS rows above are from the final-code runs.

**Deviations.**

1. The run was against the devloop cluster, not the static cluster the human uses.
2. Playwright reused an already-running dev server on :5173. That is safe, because every
   multi-party spec asserts the tone, lever and telemetry build knobs at runtime and fails a reused
   server that lacks them.
3. `auth-rejection.spec.ts` logs an expected telemetry export 401. That test is unauthenticated by
   design.

**Outcome.** No failing product step was found. **This run does NOT satisfy R-32 on its own.** The
human pass is outstanding: Phase 0 on WSL2; every by-ear, rendered-UI and real-microphone check;
and the Phase 4 refill/no-gap check, which has no automated browser equivalent. It must be run and
recorded here as Run 2 before `/close-story`. The human records the preflight's N line, the
handlers each participant connected to, and each checkbox's result.
