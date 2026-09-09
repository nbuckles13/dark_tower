# Devloop Output: Web-app in-meeting view, Svelte media stores, browser loopback env-test, secure-context runbook

**Date**: 2026-09-09
**Task**: Story task #20 — in-meeting experience (slot state, mute control, mic selection), sdk-svelte media/mute stores, Layer-7 browser loopback env-test, `docs/runbooks/client-dev-local.md` secure-context subsection
**Specialist**: client
**Mode**: Agent Teams (v2)
**Branch**: `feature/hear-yourself-through-handler`
**Duration**: ~Xm (approximate total time)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `94e1be9a12800d7cd9d31dc8f47a9c7f6331daef` |
| Branch | `feature/hear-yourself-through-handler` |
| Lead Model | `claude-opus-5` |

---

## Loop State (Internal)

<!-- This section is maintained by the Lead for state recovery after interruption. -->
<!-- Do not edit manually - the Lead updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` (client, opus) |
| Implementing Specialist | `client` |
| Iteration | `3` (Gate 2 PASSED on attempt 3) |
| Security | `spawned` |
| Test | `spawned` |
| Observability | `spawned` |
| Code Quality | `spawned` |
| DRY | `spawned` |
| Operations | `spawned` |
| Semantic Guard | `spawned` |
| Infrastructure (conditional) | `spawned` |

<!-- LEAD REMINDER:
     - Update this table at EVERY phase transition
     - Capture teammate IDs AS SOON as you spawn them
     - When phase is review and all reviewers approve, advance to complete and proceed to Step 8 (Commit)
     - Only mark complete after Gate 3 approval
     - Use /devloop-status to check state
     - If interrupted, restart the devloop; main.md records start commit for rollback
-->

---

## Task Overview

### Objective
Build the in-meeting experience for ADR-0036 story 1: a joined participant sees its slot state,
hears its own audio returned through MH, and can mute, unmute and choose a microphone — with the
mute indication driven by the SDK's client-mute state and never inferred from frame absence. Plus
the Svelte adapter bindings, a Layer-7 browser loopback env-test, and the frozen
`## Secure Context and Media Setup` runbook section that `scripts/dev-web.sh` and operations'
media triage ladder both point at.

### Scope
- **Service(s)**: `packages/{sdk-core,sdk-svelte,web-app}`; `docs/runbooks/client-dev-local.md`
  (operations-owned), `scripts/dev-web.{sh,test.sh}` (infrastructure-owned)
- **Schema**: No
- **Cross-cutting**: Yes — a co-owned runbook, an infrastructure-owned script pair, and the
  `docs/TODO.md:759` close-out

### Debate Decision
NOT NEEDED — ADR-0036 is the governing design decision; this is implementation.

---

## Cross-Boundary Classification

Every file this plan touches has a row. `docs/runbooks/client-dev-local.md` is operations-owned and
co-owned this story; `scripts/dev-web.sh` / `scripts/dev-web.test.sh` are infrastructure-owned.
Neither path appears in `scripts/guards/simple/cross-boundary-ownership.yaml` (it carries no docs or
scripts keys), so the Layer-B guard will not catch these — the table and the Gate-1/Gate-3
confirmations are the whole control.

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `packages/sdk-core/src/media/pipeline/egress.ts` (additive getter) | Mine | — |
| `packages/sdk-core/src/media/pipeline/ingress.ts` (additive counter + getter) | Mine | — |
| `packages/sdk-core/src/media/lifecycle/AudioPipeline.ts` | Mine | — |
| `packages/sdk-core/src/media/lifecycle/muteState.ts` (two comment lines: client mute does not release the capture device) | Mine | — |
| `packages/sdk-core/src/media/lifecycle/__tests__/audioPipeline.deviceAndCounts.test.ts` (new) | Mine | — |
| `packages/sdk-core/src/session/events.ts` | Mine | — |
| `packages/sdk-core/src/index.ts` (export `MediaFrameCounts`) | Mine | — |
| `packages/sdk-core/src/session/MeetingSession.ts` | Mine | — |
| `packages/sdk-core/src/session/__tests__/meeting-session.test.ts` | Mine | — |
| `packages/sdk-svelte/src/stores/MediaStore.svelte.ts` (new) | Mine | — |
| `packages/sdk-svelte/src/stores/MeetingStore.svelte.ts` | Mine | — |
| `packages/sdk-svelte/src/stores/bindMeetingSession.ts` | Mine | — |
| `packages/sdk-svelte/src/index.ts` | Mine | — |
| `packages/sdk-svelte/src/__tests__/**` (incl. `helpers/BoundHarness.svelte`, new `helpers/GranularityHarness.svelte`) | Mine | — |
| `packages/web-app/src/views/InMeeting.svelte` (new) | Mine | — |
| `packages/web-app/src/views/JoinMeeting.svelte` | Mine | — |
| `packages/web-app/src/lib/e2eBus.ts` | Mine | — |
| `packages/web-app/src/lib/slotState.ts` (new) | Mine | — |
| `packages/web-app/src/__tests__/**` (incl. `helpers/MockMeetingSession.ts`) | Mine | — |
| `packages/web-app/tests/bundle-content.test.ts` (comment only — see Planning §5) | Mine | — |
| `packages/web-app/e2e/mcMetrics.ts` (prettier reformat; pre-existing drift, see below) | Mine | — |
| `packages/web-app/package.json` (`lint` widened to cover `e2e/`) | Mine | — |
| `packages/web-app/e2e/fixtures.ts` | Mine | — |
| `packages/web-app/e2e/media-loopback.spec.ts` (new) | Mine | — |
| `packages/web-app/e2e/README.md` | Mine | — |
| `package.json` (`pnpm.overrides`: `fast-uri` floor `>=3.1.4` -> `>=3.1.6`) | **Not mine, Minor-judgment** | @security |
| `pnpm-lock.yaml` (regen from the override bump) | **Not mine, Mechanical** | @security |
| `docs/observability/metrics/client.md` (first-media entry: the wire-boundary consequence) | **Not mine, Minor-judgment** | @observability |
| `docs/specialist-knowledge/client/INDEX.md` | Mine | — |
| `docs/devloop-outputs/2026-09-09-web-app-in-meeting-loopback/main.md` | Mine | — |
| `docs/runbooks/client-dev-local.md` (new `## Secure Context and Media Setup`; F7 severity fix; §5 heading + TOC de-count; Changelog + Last Updated) | **Not mine, Minor-judgment** | @operations |
| `scripts/dev-web.sh` (header: trim the SECURE CONTEXT block from four statements to one claim + the anchor — the four-API enumeration is REMOVED from the script and moves to the runbook — and delete the reconcile-later paragraph) | **Not mine, Minor-judgment** | @infrastructure |
| `scripts/dev-web.test.sh` (retarget `pointer-names-the-apis` to the runbook side; make the conditional block fail-closed; add prose-parity + flag-parity assertions, each with its own positive control) | **Not mine, Minor-judgment** | @infrastructure |
| `docs/TODO.md` (close `:759` with the decision that closed it; file ONE new open entry under `## Documentation Hygiene`, owner infrastructure, cross-referenced from the 759 clause) | **Not mine, Minor-judgment** | filers (@dry-reviewer, @infrastructure, @security) |

**Entered scope DURING implementation, disclosed here rather than routed around:**

- `scripts/dev-web.sh`'s **fingerprints hard-fail counter-message** (one clause at `:294-299`) plus
  one assertion in `scripts/dev-web.test.sh`'s existing block. Both files already had rows. The
  clause was **ruled out of scope by the Lead at plan approval and then ruled back in**: @infrastructure,
  who owns the file, checked the branch rather than reasoning from the category and showed the
  premise was false for this edit — `:294-299` is an existing `fail` plus three `echo`s (a string
  edit, no new control flow, no new print site, no touch to the `HARD_FAIL` accumulator) and
  `dev-web.test.sh:148-162` is already fully driven by `make_root without-fingerprints` with an
  existing positive control. The Lead reversed on that evidence. **Recorded because the audit trail
  matters more than the outcome**: a clause entered a diff by owner ACK after a Lead ruling was
  reversed on the owner's evidence, which is the cross-boundary process working rather than being
  bypassed. Scope stayed exactly as ACKed — one clause, one assertion, nothing on the WebTransport
  listener messages, no blanket secure-context print.
- `package.json` + `pnpm-lock.yaml`, **at Gate 2**. Four high `fast-uri` advisories reached
  `packages/sdk-core` through `vite-plugin-dts -> @microsoft/api-extractor -> @microsoft/tsdoc-config
  -> ajv`. Pre-existing and unrelated to this work; the gate fired because the one-line lint-scope
  change to `packages/web-app/package.json` tripped the dep-manifest gate. Not deferrable — the
  layer was red and the tree already has the mechanism. **The `fast-uri` override already existed**
  at `>=3.1.4 <4`; the advisories patch at `>=3.1.6`, so the floor was raised rather than a second
  entry added, and the `<4` ceiling (ajv needs v3) is left alone. No advisory suppressed and no
  suppression list widened. `pnpm audit` goes from 4 high to 0 high (2 moderate remain, below the
  gate's threshold and untouched by this change).
- `packages/web-app/package.json` and `packages/web-app/e2e/mcMetrics.ts`. The `lint` script covered
  `src/**` only, so the whole `e2e/` tree — including this task's new spec and fixtures — was
  **ungated for eslint and prettier**, and `mcMetrics.ts` was already unformatted with nothing
  catching it. Widening the script closes the gap and puts the new e2e code under the same gate as
  everything else; the reformat is what that widening required. Both Mine, both trivial, and
  leaving a directory silently unchecked is the masked-failure shape the conventions bar.

**Deliberately NOT touched**: `proto/**`, `crates/**` (including `crates/dt-guard/**` — the fake-media
flags pass the insecure-flag guard on vocabulary non-membership, so no allowlist entry is added and
the 2-entry allowlist stays at 2), `docs/observability/metrics/client.md` and
`docs/observability/label-taxonomy.md` (no new metric name is introduced anywhere in this diff — see
Planning §7), `packages/web-app/playwright.config.ts` (the fake-media flags are already there;
nothing to change), `packages/web-app/vitest.config.ts` / `packages/sdk-svelte/vitest.config.ts`
(their fake-media flag copies are pre-existing and cross-package — @dry-reviewer files the
extraction opportunity; not fixed here).

---

## Planning

### Gate 1 — Plan Confirmations (Lead-maintained)

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed (6 findings, all resolved in plan text) |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed (2 findings, both resolved) |
| DRY | confirmed |
| Operations | confirmed (R8 + R9 resolved) |
| Semantic Guard | confirmed |
| Infrastructure (conditional owner of `scripts/dev-web.*`) | confirmed (2 conditions met + 1 self-correction) |

**Gate 1 guard**: `./scripts/guards/simple/validate-cross-boundary-classification.sh` →
`STATUS=OK REASON=cross-boundary-classification-clean-1-files` (exit 0). "Plan approved" issued
2026-09-09.

**Gate 1 caught four substantive plan defects before a line of code was written**, three of them in
§9's `docs/TODO.md:759` reconciliation rather than in the media code:

1. **@security** — plan §10 enumerated the prohibited browser-trust settings verbatim while §8 R4
   imposed "describe the class, never print the literal" on the runbook. `dt-guard
   no-insecure-browser-flags` scans **tracked** files and `.md` is in scope, so the plan file was
   invisible to the guard only because it was untracked; committing it would have red-ed Layer 3 at
   Gate 2. Fixed as prose, explicitly not a third allowlist entry — an entry earned by merely
   *discussing* the prohibition would erode the guard's docs coverage to nothing.
2. **@code-reviewer** — adding the three bus event-type literals to `bundle-content.test.ts`'s
   `FORBIDDEN` list was a category error that would have red-ed a *correct* production build:
   `FORBIDDEN` holds marker tokens, and `firstMediaFrame` / `muteState` name production concepts
   shipped via sdk-core. The natural fix would have been to weaken the test — the ADR-0028 vacuity
   trap, introduced by the forcing function itself.
3. **@security F2 + @operations R8, reached independently** — retiring `pointer-names-the-apis`
   while moving the four-API enumeration to the runbook would have left that claim guarded nowhere,
   making §9's "retargeted, never net-reduced" false on its own terms. A comment is a note to a
   human, not a control. @infrastructure recorded that this misapplied their *own* stated standard.
4. **@operations R9** — `scripts/dev-web.test.sh:327-331` already carries an in-tree obligation
   naming this task ("Story task 20 asserts the same fact from the runbook side; the two must not
   contradict"). Only the header was pinned; the runbook side had a promise, not a control.

Net effect: the runbook-side extraction carries four assertions under four distinct reason tokens,
plus a separate non-empty-body vacuity guard, plus @infrastructure's positive control on the
flag-parity check (exactly 2 flags extracted, under a token distinct from the mismatch token). Live
assertion count in `dev-web.test.sh` goes **up**, and the conditional `else PASS++` branch — review
protocol vacuity mechanism 5 — becomes a hard fail now that the heading exists by construction.

**Lead ruling on scope**: no blanket hard-fail printing of the secure-context text (a block that
always prints is a block nobody reads, and it would degrade the script's remedy-per-failure
discipline). The one genuinely reachable gap — a counter-message at the fingerprints hard fail,
where a developer reaches for a certificate-validation-disabling flag — is an infrastructure-owned
follow-up, filed in `docs/TODO.md` at verdict time alongside the `--help`-only gap so neither
obligation vanishes with the TODO:759 entry that carried them.

**Ruling 2 REVERSED 2026-09-09, on the owner's evidence.** I ruled the targeted counter-message out
of scope as "a behavioural change to failure output in an infrastructure-owned script."
@infrastructure checked the file rather than reasoning from the category and showed the description
is false for this edit: `scripts/dev-web.sh:294-299` is an existing `fail` plus three `echo` lines,
so appending a clause is a string edit — no new control flow, no new print site, no touch to the
`HARD_FAIL` accumulator — and `scripts/dev-web.test.sh:148-162` already has its driver (`make_root
without-fingerprints`), three text assertions and an existing positive control, so covering the
clause is one more `assert_status` in a block that already runs. That is the opposite profile from
the `--help` item they declined, which adds a new print at a new point in the control flow on a path
with no existing driver and touches the accumulator.

The deciding argument is theirs: the header trim removes counter-message statements from a block
reachable **only** via `--help`, while the fingerprints hard fail is a message that actually prints
at the failure moment. Landing the trim while deferring the clause would have made this diff
net-worse at exactly what the TODO:759 argument was about — the same decision seen from the output
side, not a coincidence of scope. The review protocol's suspicious-deferral check failed all three
counts (under 5 LoC, file already in the changeset, no design ambiguity), which is the protocol
naming my own ruling as the sunk-cost pattern it warns about explicitly.

Scope as owner-ACKed and no wider: one clause on the fingerprints message, plus one `assert_status`
in the existing block; the runbook F7 half rides along in the F7 prose edit already planned
(operations-owned). **Not** the MC/MH WebTransport listener messages, and **no** blanket
secure-context print — ruling 1 stands and security's remedy-per-failure position is untouched. This
entered scope by owner ACK after a Lead ruling was reversed, not as a new plan condition, so Gate 1
confirmations stand. The `--help`-only gap remains deferred and is filed at verdict time as an open
Documentation Hygiene entry cross-referenced from the TODO:759 close-out.

The implementer stopped and escalated rather than picking between a Lead ruling and the file owner's
ACK. That was the correct call and is recorded as such.

`infrastructure` was added to the panel as a **conditional domain reviewer** because
`scripts/dev-web.sh` / `scripts/dev-web.test.sh` (infrastructure-owned) entered scope via the
`docs/TODO.md:759` obligation, whose stated closing condition is this task. Per ADR-0024 §6.3 a
Minor-judgment cross-boundary edit requires the owner-specialist present as a reviewer; without this
addition the two script rows would have had no owner to confirm them at Gate 1 or Gate 3.

### 0. Mechanism restatement (and the widening it produces)

The task is phrased as an instance: *build a mute button that doesn't lie*. Restated as a mechanism:

> **In the media path, every "nothing is arriving" condition is ambiguous, and the UI must render it
> from an explicit signal that the SDK owns — never infer it from frame absence. Each such piece of
> state has exactly ONE authoritative holder, ONE path to the DOM, and ONE path to the test bus.**

That mechanism produces a wider class than the task names, with the same owner (client). The
task names one member (client mute); ADR-0036 §5/§6 name three more that are structurally identical
and that this diff therefore also covers rather than leaving as the next person's bug:

| Absence-shaped condition | Authoritative holder | Rendered from |
|---|---|---|
| I muted myself | `MuteState` in `AudioPipeline` (§5) | `muteChanged` event |
| the far end is muted / congested / unreachable / under-filled | MC, on the wire (§6) | `StreamAssignments.slot_state` |
| media is arriving at all | `FirstMediaObserver` (§10) | `firstMediaFrame` event |
| the pipeline broke (decoder/playback/transport/capture) | `AudioPipeline#fault` | `fault` event |

All four are already produced by task #19's SDK; none of them reaches `MeetingSession`'s event map,
so none can reach a Svelte store or the bus today. So the shape of this change is: **bridge the four
absence-signals onto the session facade once**, and let the store, the view and the bus all read that
one bridge. Surfaced to reviewers because it is one extra event (`mediaFault`) and one extra store
cell beyond the literal task text, and because "fault" is the member most likely to be re-derived
badly later (a spinner that means four different things).

### 1. What exists (read before planning)

Task #19 (`docs/devloop-outputs/2026-09-08-sdk-audio-media-pipeline/main.md`) landed the whole audio
pipeline. Everything this task needs on the SDK side already exists and is emitted:

- `AudioPipeline` emits `muteChanged` / `firstMediaFrame` / `frameAccepted` / `fault`, and
  `setAudioMuted()` enforces mute **at capture** (`#onCapturedFrame` releases the `AudioData` before
  the encoder sees it) — so egress is already gated upstream of encode/encrypt, which is exactly what
  @security asked for in their point 5. There is no `track.enabled = false` anywhere and none is added.
- `EgressPipeline.#hopSequence` is already a monotone count of frames **actually sent** (advanced at
  dequeue, not enqueue), and the pipeline instance is created once per session
  (`#applyDirective` guards with `if (!this.#egress)`), so it does not reset on a directive change.
- `SignalingClient` already emits `streamAssignments` with the mapped §6 slot-state vocabulary.
- `listMicrophones()` is already exported from `@darktower/sdk-core` and already documents the
  empty-label-before-permission case.
- Metrics already exist and are catalogued: `dt_client_media_frames_sent_total`,
  `dt_client_media_frames_received_total`, `dt_client_media_frames_accepted_total`,
  `dt_client_media_mute_transitions_total{action}`, `dt_client_time_to_first_media_frame_ms`.

Server side, the loopback is in place: MC's `media_routing/assignment.rs` produces the N=1 self-edge
and MH forwards it (`media/forward.rs`), landed by tasks #11/#12/#16/#24/#25/#26.

### 2. sdk-core — bridge the four signals; no new metric, no new hot-path work

`session/events.ts` — `MeetingSessionEventMap` gains exactly four members:
`muteChanged: MuteSnapshot`, `firstMediaFrame: number`, `streamAssignments: StreamAssignmentsEvent`,
`mediaFault: MediaFault`. All four payload types are already exported from `sdk-core`'s barrel; no
new type is coined.

`session/MeetingSession.ts` — bridges them: `streamAssignments` in `#wireSignaling` alongside the
existing `participantJoined`/`participantLeft`/`error` bridges; the other three in `startMedia()`
immediately after the pipeline is constructed. Pure forwarding, no state.

**No new method on `MeetingSession`.** The view drives mute through the `AudioPipeline` that
`startMedia()` returns (also reachable as `session.media`), which is already public. A delegating
`session.setAudioMuted()` would be a second name for one behaviour and a second place to get the
no-pipeline case wrong.

**Counter reads for the bus (@observability #2/#3).** No per-frame emission and no test-only callback
on the forward path:

- `EgressPipeline` gains `get framesSent(): number` returning the existing `#hopSequence`. Zero new
  per-frame work — it is a read of bookkeeping the send path already maintains.
- `IngressPipeline` gains `#framesAccepted` and `get framesAccepted(): number`. The increment goes
  **immediately beside the existing `this.#metrics.frameAccepted()` call** (`pipeline/ingress.ts:216`),
  not on a separate branch — @dry-reviewer's and @observability's shared condition: **one increment,
  two readers** (the metrics sink and the getter). A `++` at a different point in the function would
  be a second encoding of "how many frames arrived" that agrees today and drifts the first time a
  drop path lands between the two sites, with the bus field and the metric disagreeing and nothing
  failing. Same rule for `framesSent`, whose source `#hopSequence += 1` already sits adjacent to
  `this.#metrics.frameSent()` after a successful send (@observability verified the ordering: the
  oversize branch and the transport-refused `catch` both `continue` before it, so the getter counts
  exactly what the metric counts — attempted-vs-successful is not a divergence here).
- **Each mirror pair gets a comment at its site** saying the integer and the counter must move
  together and why (the bus field is the structural mute assertion's evidence, and the Layer 7
  failure message invites a reader to map the bus field onto `dt_client_media_frames_sent_total`).
  @observability explicitly asked for a comment rather than a guard: the risk is a future reorder,
  and what is needed is that the next person reordering that block sees the coupling.
- **One comment recording why a frame COUNT is safe when a frame SIZE is not** (@security Finding 3),
  at a single site of my choosing among the two getters or the bus projection: the counts are
  non-revealing **only because the encoder is configured with DTX off** — `muteState.ts` already
  states that "with DTX, absence of frames becomes a signal". Enable DTX later and the count becomes
  exactly the voice-activity trace ADR-0036 §11 names, both on the bus at 4 samples/second and,
  more importantly, in the pre-existing production `dt_client_media_frames_sent_total`. A comment,
  not a mechanism; @security is not asking for a guard.
- **Two comment lines in `media/lifecycle/muteState.ts`** (@security Finding 4): client mute
  deliberately does **not** release the capture device. The invariant is that no media leaves the
  device, not that the device is closed, so the browser/OS microphone indicator stays lit while
  muted and that is expected. Without this, a reader who sees a lit indicator beside a "muted" UI
  reads it as a mute failure and "fixes" it by stopping the track — which breaks instantaneous
  unmute and reintroduces a resumption path with a server round trip, i.e. the exact thing §5's
  local-first design exists to prevent.
- `AudioPipeline` gains `get frameCounts(): { framesSent: number; framesAccepted: number }` —
  allocates one object **per read**, and reads are sampled (4/s in test builds), never per frame.

`pipeline/egress.ts` and `pipeline/ingress.ts` stay clean under `media/__tests__/hotPathLayout.test.ts`:
no logger import, no logging sink, no `dt_client_` literal added.

**Device switching (`AudioPipeline.setCaptureDevice(deviceId)`).** A picker that silently does nothing
after `start()` is a masked failure, so the switch is real: `start()` stores the capture in a
`#capture` field and registers **one** teardown closure that reads the field
(`register('capture', () => this.#capture?.stop())`) instead of closing over the instance;
`setCaptureDevice` stops the old capture, builds a new one through the same `captureFactory`, and
starts it with the same callbacks. Before `start()` it just records the id. `TeardownRegistry` has no
unregister, which is exactly why the single field-reading registration is the right shape.
Encoder/decoder/playback are untouched (same sample rate and channel count), the mute check is
per-frame so it survives the swap, and the first-media epoch is not restarted.

Tests: device-switch (swap while running, swap before start, swap to the same id is a no-op, teardown
after a swap stops the *new* capture), counter monotonicity, and the four event bridges.

### 3. sdk-svelte — a separate reactive cell set, so mute does not invalidate the roster

New `stores/MediaStore.svelte.ts` with **one `$state` cell per field** (not one cell holding an
object — replacing an object invalidates every reader):

`audioMuted: boolean` · `firstMediaFrameMs: number | undefined` · `slots: readonly StreamAssignmentEvent[]` ·
`lastMediaFault: MediaFault | undefined`, each behind a getter, each with a typed mutator.

`MeetingStore` gains `readonly media = new MediaStore()` — a plain (non-reactive) field holding the
media store. Reading `store.media` tracks nothing, so a template that reads `store.participants` is
not invalidated by a mute toggle and vice versa. `bindMeetingSession(session)` keeps its exact
signature and return type; `subscribeSession` gains four subscriptions that drive `store.media`. So
the answer to @dry-reviewer (b) is: **shares the skeleton, does not reimplement it** — one
`subscribeSession`, one aggregate unsubscribe, one `onDestroy`.

`MediaStore` is exported from the barrel for consumers that want the type.

Re-render granularity is asserted, not asserted-about: a browser test mounts a harness with a
roster-projection `$derived` that increments a counter when it recomputes, toggles mute several
times, and asserts the counter does not move (and, as the positive control, that it *does* move on a
`participantJoined`).

### 4. web-app — the in-meeting view

New `views/InMeeting.svelte`, rendered by `JoinMeeting.svelte` **in addition to** its existing
markup once `store.meetingState === 'joined'`. JoinMeeting's current DOM (`meeting-state`,
`participant-list`, `participant-${id}`, `last-error`, the form) is left byte-identical so no existing
spec or `fixtures.ts` helper moves.

DOM contract (new testids):

| testid | Role |
|---|---|
| `in-meeting` | the view root; present only after a settled join |
| `mic-select` | `<select>` of `listMicrophones()`; labels rendered, `deviceId` used as the option value |
| `start-audio` | starts the media pipeline (explicit, per task #19's kill-switch note) |
| `mute-toggle` | the control; `aria-pressed` bound to the SDK state |
| `mute-state` | the visible indication — renders `muted` / `unmuted` |
| `slot-list`, `slot-${slotId}` | one row per assignment, with `data-slot-state="<wire token>"` |
| `media-fault` | the bounded fault message, when one exists |

- **One owner of mute (@dry-reviewer (a)).** The button's handler calls
  `pipeline.setAudioMuted(!store.media.audioMuted)`; the indicator renders `store.media.audioMuted`,
  which is set only by the SDK's `muteChanged` event. The view holds **no** local mute boolean, and
  nothing anywhere derives mute from frame counts, timers or frame absence.
- **Far-end state comes from the wire.** `lib/slotState.ts` is a tiny SSoT mapping the eight §6 wire
  tokens (plus one clearly-distinct client-side `awaiting-assignment` for "MC has not sent
  StreamAssignments yet", which is a different fact from any wire state) to user-facing text. Unit
  tested exhaustively so a new wire token is a compile error, not a blank cell.
- **Device labels stay in the DOM (@security 4, @observability 4).** `label` and `deviceId` are
  rendered into the `<option>` and used as the `getUserMedia` constraint, and go nowhere else: not to
  a bus payload, not to a `data-*` attribute, not to a metric label, not to `console.*`, not to the
  telemetry proxy, and nothing is persisted. Capture failures surface through the SDK's existing
  bounded `MediaCaptureError` vocabulary via `errorText`; no raw `DOMException.message` reaches the
  DOM or the bus.
- **No `console.*` and no `logger.ts` `JoinEvent` extension** anywhere in this diff.

### 5. e2eBus — three new events, inside the existing `__E2E_HOOKS__` block

All additions go inside the single existing `if (__E2E_HOOKS__) { ... }` in
`packages/web-app/src/lib/e2eBus.ts`. Whitelist projections of scalars only:

| Bus event | Payload | Source |
|---|---|---|
| `firstMediaFrame` | `{ elapsedMs: number }` | session `firstMediaFrame` (same value as `dt_client_time_to_first_media_frame_ms`) |
| `muteState` | `{ audioMuted: boolean }` | session `muteChanged` |
| `mediaFrameCounts` | `{ framesSent: number, framesAccepted: number, atMs: number }` | **sampled** read of `session.media?.frameCounts` |

No key material, no key ids, no salts, no frame bytes, no plaintext, no decoded samples, no device
label, no `deviceId`, no participant name, no meeting code, no token. `framesSent` /
`framesAccepted` are plain integers, never a frame list and never per-frame sizes (which §11 names as
the voice-activity trace).

`installE2EHooks(session)` widens its parameter to a structural type that also has the
`media` getter, and **returns a disposer** (a no-op function in prod builds). The sampler is a
`setInterval` at a named constant (250 ms) that emits only while `session.media` exists;
`JoinMeeting.svelte` disposes it in `onDestroy`. Nothing in the SDK is added for the bus's benefit —
the bus reads a counter the send path already keeps.

**Prod absence — corrected at Gate 1 (@code-reviewer finding A, verified at source).** An earlier
draft proposed adding the three bus event-type strings to `bundle-content.test.ts`'s `FORBIDDEN`
list. That is a category error and would have **red-ed a correct production build**: `FORBIDDEN`
holds MARKER tokens only, and two of the three strings name production concepts —
`firstMediaFrame` is emitted by production sdk-core at `AudioPipeline.ts:211` and `muteState` is a
production getter at `AudioPipeline.ts:216`, both of which ship in the web-app bundle because it
includes sdk-core. The existing bus event-type strings (`joined`, `stateChange`, `mediaConnected`)
are deliberately absent from `FORBIDDEN` for exactly this reason. The "fix" for the resulting red
would have been to weaken the test — the ADR-0028 vacuity trap.

So: **`FORBIDDEN` gains nothing.** The DCE proof already exists and already covers the new code —
every new emit and the sampler itself live inside the single `if (__E2E_HOOKS__)` block whose
window-attach marker `__darktower_test__` and whose gate `__E2E_HOOKS__` are both already
`FORBIDDEN`; if those two are absent from a real production build, everything inside the gate is
provably eliminated. What I *do* add is a comment in `bundle-content.test.ts` recording why event-type
literals must never be added to that list, citing the two production sites — a durable control
against the next author repeating the mistake I just made, which a silent retraction would not
provide.

### 6. Layer 7 browser env-test

New `packages/web-app/e2e/media-loopback.spec.ts`, one spec, following `join-happy-path.spec.ts`.
Reuses `fixtures.ts` (`authAsSharedUser` → 0 registrations, `bootstrapMeeting`, `joinAsUser`,
`waitForJoined`, `waitForAllMediaConnected`, `busEvents`, `busFailureContext`) — **@dry-reviewer (c):
`fixtures.ts` for everything; `mcMetrics.ts`/`instanceCounters.ts` are not used, because every
assertion here is client-observable and a Prometheus baseline-delta would add a second, slower source
of truth for a fact the bus already carries.** New helpers live in `fixtures.ts` beside the existing
ones (`startAudio`, `waitForFirstMediaFrame`, `frameCountSamples`, `setMuteViaUi`,
`expectMuteIndicator`, `expectEgressAdvances`, `expectEgressFlatWhileMuted`).

Sequence and what each step proves:

1. Sign in as the shared user, `bootstrapMeeting`, `joinAsUser`, `waitForJoined`,
   `waitForAllMediaConnected` — the join and MH handshake preconditions, reusing the existing helpers.
2. Click `start-audio`. Chromium's `--use-fake-device-for-media-stream` supplies the synthesized
   capture and `--use-fake-ui-for-media-stream` auto-grants the microphone permission. **Both flags
   are already in `playwright.config.ts`; no flag is added, and no cert-bypass / web-security /
   insecure-origin flag exists anywhere in this diff.**
3. `waitForFirstMediaFrame` — **the functional pass/fail**: a `firstMediaFrame` bus event within a
   bounded wait means audio the client encrypted, signed and sent came back through MH, was verified,
   decrypted and decoded. Its `elapsedMs` is **OBSERVED and REPORTED, never gated**: pushed to
   `testInfo.annotations` and printed. The only assertions on the number are that it is a finite
   non-negative number — there is no threshold, and adding one later is the §10 defect, not a
   tightening. The *wait* bound is a liveness bound on the event's existence, not a latency budget,
   and its failure message says so.
4. Positive control before mute: `expectEgressAdvances` — `framesSent` strictly increases across
   samples. Without this, every later assertion passes vacuously on a dead pipeline.
5. Mute via `mute-toggle`; assert the `mute-state` DOM text and the `muteState` bus event agree
   (indicator ⇔ SDK state).
6. **Structural silence**: take the `framesSent` baseline from the first sample after a bounded
   post-mute settle (frames already queued at the moment of mute may still drain — §5 stops capture
   within one frame, it does not un-queue), then assert every subsequent sample inside the muted
   window equals that baseline, requiring at least 4 samples spanning ≥1.5 s so flatness cannot be
   satisfied by having no samples. No audio-energy sampling, no bare `sleep`-and-hope.
7. Unmute; `expectEgressAdvances` again **and** assert `framesAccepted` advances — send resumed *and*
   audio is arriving again at the ear end.

Config: `retries: 0`, `workers: 1`, `fullyParallel: false` unchanged, nothing overridden per-spec.
**Budget (@operations):** expected ~25 s wall clock (join+MH ≈ 5 s, start ≈ 2 s, first media ≈ 2 s,
pre-mute observation 1.5 s, muted window 2.5 s, post-unmute 1.5 s, plus auth/bootstrap); the per-test
ceiling remains the config's `timeout: 120_000` — not overridden — with each wait carrying its own
tighter, assertion-meaningful bound. Registration cost: **0** (shared user by sign-in).
**Failure output (@operations):** every failure names the observation, not a boolean —
e.g. `mute regression: framesSent moved 412 → 431 across 6 samples spanning 2.4s while muted (mute at
t=…); egress must be flat because client mute is enforced at capture (ADR-0036 §5)`, and
`no audio returned: no 'firstMediaFrame' bus event within 15000ms after start-audio; last frame
counts sent=0 accepted=0` + the existing `busFailureContext`. `e2e/README.md` gains the budget row
and the assertion-catalog entries.

Layer 7 picks the spec up automatically: `scripts/layer7.sh` Phase 2 runs
`pnpm --filter @darktower/web-app test:e2e` over the whole `e2e/` directory. No new lane, no skip
path, no conditional exit-0.

### 7. Observability posture (explicit answers to @observability)

**No new metric name appears anywhere in this diff** — not in web-app, not in sdk-svelte, not in
sdk-core. `docs/observability/metrics/client.md` and `docs/observability/label-taxonomy.md` are
therefore untouched. The three bus events are projections of state that already has production homes
(`dt_client_time_to_first_media_frame_ms`, `dt_client_media_mute_transitions_total{action}`,
`dt_client_media_frames_sent_total` / `_received_total` / `_accepted_total`); the bus is a test-only
channel and is not the sole home of any signal. Mute state and slot state become DOM and bus content,
never metric dimensions. Device labels reach the DOM only.

### 8. Runbook (`docs/runbooks/client-dev-local.md`) — acking @operations R1–R7

- **R1** heading is exactly `## Secure Context and Media Setup` — two hashes, that title case, no
  punctuation. No subheading under it (or anywhere earlier in the file) will contain the word pair
  "secure"+"context", so `dev-web.test.sh`'s `head -1` grep lands on mine.
- **R2** placed between `## 2. Prerequisites` and `## 3. Bring-up`, unnumbered, with the TOC entry in
  the same position. Not inside §4 or §5.
- **R3** fact-only, ~30–40 lines: the four APIs as one all-or-nothing gate (and that they are
  *absent* — `navigator.mediaDevices` and `crypto.subtle` are `undefined`, the WebCodecs and
  WebTransport constructors are not there — which is *why* it presents as joined-with-no-audio rather
  than a permission prompt); `http://<org-subdomain>.localhost:5173` is potentially trustworthy in
  **Chrome** per W3C Secure Contexts §3.1 (`.localhost` per RFC 6761), so plain HTTP works; the
  non-localhost HTTP symptom in **one sentence** with a forward pointer to operations' §4.5 ladder and
  no diagnosis; the no-microphone demo launch naming
  `--use-fake-device-for-media-stream` and `--use-fake-ui-for-media-stream` verbatim.
- **R4** the remedy set is exactly the script's: use a `.localhost` name or a loopback literal, or
  terminate real TLS. It does **not** say "an HTTP origin is insecure". No origin-trust-override,
  cert-validation-disabling or web-security-disabling flag is named — not even as a
  "don't do this" example. The class is described in prose; the literal is never printed.
- **R5** F7's Symptom is corrected: `dev-web.sh` **hard fails** on a missing fingerprints file and
  does not start the dev server, plus the clause that the check is presence-only
  (`[[ -s ... ]]`), so a **stale-but-present** file passes preflight green and the browser still
  refuses the handshake — which is the whole reason F7 exists.
- **R6** `## 5. Failure modes (F1–F11)` → `## 5. Failure modes` and TOC `:50` → `#5-failure-modes`.
  Exactly those two lines. No `grep -rl | xargs sed`; the devloop records and the story prompt keep
  their text (`docs/TODO.md:1299` artifact-kind rule). I re-ran the sweep and confirm @operations'
  result: no other live in-repo reference to `#5-failure-modes-f1f11`. The §5 preamble's "F1/F8" and
  "F1/F9" pairs are member names, not a count, and stay. The commit will cite the general rule this
  is an instance of (`docs/observability/metrics/mc-service.md:351` — state a set by naming members,
  never by restating a count).
- **R7** a Changelog row in the 2026-08-06 shape and `**Last Updated**` → 2026-09-09.
- Commit trailer: `Approved-Cross-Boundary: operations` with a reason clause.

### 9. Closing `docs/TODO.md:759` — implementing @infrastructure's Gate-1 ruling

@infrastructure has ruled on the shape of their file (they own it; I implement it):
**one-line summary + pointer**, not pointer alone. Their reasoning stands on its own — `--help`
prints the header verbatim, and the sentence that must survive is the *anti-flag counter-message*,
because the moment a developer sees "no audio over `http://<lan-ip>`" is exactly when a prohibited
origin-trust flag gets typed into a terminal, where no tracked-file guard can ever see it. So the
substance shrinks by ~80% while the only sentence whose absence causes harm stays.

1. **`scripts/dev-web.sh`** — the SECURE CONTEXT block (`:50-66`) is trimmed to the one-claim form
   @infrastructure specified: non-loopback `http://` silently disables the pipeline and presents as
   "joined, no audio"; `.localhost` names and loopback literals **are** potentially trustworthy,
   which is why the demo works over plain `http://`; the approved fix (`.localhost` name, loopback
   literal, or real TLS — never a browser flag); and the anchor. The API enumeration and the
   "why it is gated" explanation move to the runbook, which becomes canonical for them. The
   "deliberate offline copy / reconcile when task 20 lands" paragraph is **deleted** — its window has
   closed.
2. **`scripts/dev-web.test.sh`** — assertions are **retargeted, never net-reduced**:
   - `pointer-names-the-apis` is **RETARGETED, not retired** — @security Finding 2 and @operations
     R8 independently caught that my first draft was a net coverage reduction wearing a
     reconciliation's clothes. Retiring it while the new parity assertion carried only two tokens
     would have left "getUserMedia, WebCodecs, WebCrypto and WebTransport are named at all" with
     **zero** mechanical coverage anywhere — not the header (assertion gone) and not the runbook
     (never covered) — and a comment is a note to a human, not a control. So the four API names
     join the runbook-side prose-parity token set as `runbook-names-the-apis`, under its own reason
     token; the comment at the old site records a **move**, which is what §9 claims happened. The
     fact is operationally load-bearing rather than decorative: the all-or-nothing property is *why*
     the symptom is silence rather than a permission prompt, and it is the sentence that stops a
     developer concluding their microphone is broken.
   - `pointer-cites-frozen-anchor`, `pointer-says-non-loopback`, `pointer-gives-approved-fix` and
     `pointer-does-not-contradict-task-20` all **survive**; the trimmed wording satisfies each, and
     the last one is what keeps the header from flatly claiming an HTTP origin is insecure.
   - The conditional anchor block **stops being conditional**: the heading now exists by
     construction, so `else PASS++` becomes a **FAIL** under a distinct reason token
     (`secure-context-heading-missing`, separate from `secure-context-anchor-resolves`) so
     "the target vanished" is not triaged as "the pointer is misspelled". This is the
     review-protocol vacuity mechanism-5 case (subject observes nothing, reports clean).
   - **New prose-parity assertion, both directions.** Extract the runbook's
     `## Secure Context and Media Setup` body by a **bounded** range (heading line to the next
     `^## ` or EOF — structurally cannot run to EOF unbounded, the same reasoning already recorded at
     `dev-web.sh:95-101` for `awk` vs `sed -n '2,/^$/p'`), then assert it carries **four** token
     sets, each under its own reason token — because the SSoT move must not leave the runbook side
     carrying strictly fewer guarantees than the header side it replaces (@operations R8 + R9,
     which are one request, not two):

     | Reason token | Asserts | Why it exists |
     |---|---|---|
     | `runbook-says-non-loopback` | the non-loopback / "joined, no audio" claim | the symptom |
     | `runbook-gives-approved-fix` | `.localhost` name / loopback literal / real TLS | the instruction |
     | `runbook-names-the-apis` | all four gated API names | R8 — retargeted from `pointer-names-the-apis`, not retired |
     | `runbook-does-not-contradict-header` | the potentially-trustworthy claim | R9 — see below |

     **R9 is an obligation already written into the tree naming this task.**
     `scripts/dev-web.test.sh:327-331`'s own comment says *"Story task 20 asserts the same fact from
     the runbook side; the two must not contradict"* — a commitment the previous devloop recorded in
     repo, not a preference. Today only the HEADER is pinned against flatly claiming an HTTP origin
     is insecure. §8 R4 promises the runbook will not say it either, but **a promise is not a
     control**: the runbook could later be reworded to "HTTP origins are insecure" and nothing would
     red, while the assertion whose own comment anticipated the mirror sits one file away.
     Guarded against its own vacuity: a **separate** assertion with its own reason token that the
     extracted body is non-empty, so an extraction that produced nothing reads as
     "extraction produced nothing", never as "the runbook lost the claim".

     **Constraints go at the SITE, not only here** (@operations). The next reader of these
     `runbook-*` tokens is @operations extending the same runbook in task #21, and the plan will not
     be open in front of them — the test will. So any token that constrains where they may edit says
     so in a comment at the assertion, in the shape the previous devloop used on me: that block's
     own comment ("Story task 20 asserts the same fact from the runbook side") is the only reason R9
     was findable at all, because it left the obligation in the place the next author would be
     standing. The same applies to the bounded extraction range — if the section grows a `###`
     subheading, the extraction still stops at the next `^## `, and that needs to be stated where
     someone would otherwise assume subsections are covered.

     **Recorded residual** (@operations will note it in their verdict either way, so it belongs
     here too): the retained header line and the runbook prose overlap on three claims, and the
     parity assertions make them a **checked pair** rather than drift-prone duplication — but they
     are guarded for **presence**, not for non-contradiction in general. We are pinning the specific
     contradictions that were actually reasoned about; we are not proving the two texts agree.
   - **New flag-parity assertion, with its own positive control.** Both fake-media flag names
     appearing in the runbook section must also appear in `packages/web-app/playwright.config.ts`.
     As first specified this was an "every X has a Y" over an extracted set and therefore **vacuous
     when the extraction yields zero** (@infrastructure, vacuity mechanism 1) — the concrete path
     being a reflow that pushes the flags outside the bounded section extraction, after which the
     assertion goes green while the runbook no longer documents the flags at all. That is the same
     shape as the `else PASS++` branch being removed three assertions away. So it carries a positive
     control: **exactly 2** flag names must be extracted, under a reason token
     (`flag-parity-extracted-both`) distinct from the mismatch token
     (`flag-parity-matches-playwright-config`), so "the runbook stopped mentioning the flags" is
     loudly distinguishable from "the flags disagree with playwright.config.ts". Pinning the count
     at 2 is safe because the section I am authoring is the in-repo source of that expectation.
   - **Every parity failure message names the runbook half as operations-owned** (@operations). A
     failure must say what broke *and who owns it*, or the first person to hit it in the
     infrastructure lane "fixes" it by editing prose in a file they do not own — the exact
     coordination failure this co-ownership arrangement exists to prevent.
3. **`docs/TODO.md:759` is closed in this diff**, in the file's own convention (`:118`, `:184`):
   flipped to `- [x]` with a **RESOLVED 2026-09-09** clause naming this devloop directory, stating
   which option was taken and why, and carrying @infrastructure's point that §3 Step 0 already runs
   the SSoT split in the **opposite** direction for the check-list/severities (script canonical,
   runbook refuses to re-enumerate) — a clean per-topic split, not a contradiction, recorded so the
   next reader does not "harmonize" the two directions and break whichever side loses.

**The fourth fake-media-flag encoding (Lead item 2).** @infrastructure has ruled it justified and not
to be deduped: it is prose a human pastes into a Chrome command line and can never consume a TS
constant, so it is structurally outside any SSoT the three configs could share (and those three are
not cheaply unifiable either — `@darktower/test-utils` exports from `dist/`, so importing it at
vitest/playwright **config-load** time creates a cold-`dist` build-order failure in three packages).
It is still not left free-floating: the runbook prose cites `packages/web-app/playwright.config.ts`
by path, and the flag-parity assertion above mechanically ties the two together. The pre-existing
three-way copy is @dry-reviewer's to file as an extraction opportunity; I do not touch it.

**Anchors NOT touched (@infrastructure's sweep, which I reproduced).** The per-entry
`### F7 — …` slugs stay byte-identical: `docs/runbooks/mh-incident-response.md` links into them at
`:370`, `:393` and `:963` via `client-dev-local.md#f7--browser-refuses-the-mcmh-webtransport-handshake`,
and the section-heading de-count does not affect them. The F7 edit under @operations R5 changes
**prose inside** that entry, never its heading.

### 10. Browser-trust prohibition — stated by CLASS, never by literal

**This section deliberately prints no flag, property or environment-variable name.** An earlier
draft enumerated them, and @security verified mechanically that committing that draft would turn
Layer 3 red: `dt-guard no-insecure-browser-flags` collects candidates **tracked-only**, `.md` is in
its candidate suffixes, and docs are deliberately in scope — so this file becomes a scanned file the
moment it is committed, and the enumeration would have produced at least five hits across all three
rule classes. That is the same discipline §8 R4 imposes on the runbook ("describe the class, never
print the literal"), applied to the plan that imposes it. The vocabulary's only home is
`crates/dt-guard/src/no_insecure_browser_flags.rs`; nothing else restates it.

The claim, which is fully expressible without printing anything: **no browser flag, harness launch
option, config property or environment variable that disables certificate validation, that disables
web security, that forces an insecure origin to be treated as trustworthy, or that suppresses the
security-warning gating, appears in any script, config, test, runbook line or plan line in this
diff** — including inside a "don't do this" sentence, because the guard matches the literal
regardless of surrounding prose and a reader under time pressure copies it out of the "don't"
sentence. MC/MH trust stays exclusively on `serverCertificateHashes` pinning. The evidence is the
guard's own green, not a restatement here.

Three consequences carried through the rest of the diff:

- **No allowlist entry is added, for any reason.** The two sanctioned fake-media flags pass on
  **vocabulary non-membership**, not by exemption; the allowlist stays at its current 2 entries. If
  a devloop plan could earn an entry merely by *discussing* the prohibition, the guard's coverage of
  docs — its highest-value coverage per its own module header — would erode to nothing.
- **The `docs/TODO.md:759` RESOLVED clause and the `## Secure Context and Media Setup` section are
  under the identical rule.** §8 R4 covers the runbook; this extends it explicitly to the TODO
  closure text.
- **§ Code Review Results paraphrases reviewer messages; it does not transcribe them.** Several
  Gate-1 messages named the literals in order to constrain me. Quoting them verbatim into this file
  would reintroduce the same failure in the same document. @security has said their verdict text
  will name classes rather than literals for the same reason.

For the future, recorded here because this is where the next author will look: an assertion of the
form "this section names no prohibited setting" must **import the vocabulary from `dt-guard`** rather
than spell literals. The e2e suite already does exactly that, which is why `dt-guard`'s module header
can claim no test file needs an exemption.


### 12. Commit trailers, and the Gate-3 commitments made at Gate 1

**Trailers must match the Cross-Boundary table** (@security Finding 5, @code-reviewer item B). Every
non-Mine row is Minor-judgment, and ADR-0024 §6.6 requires an owner hunk-ACK trailer per owner. The
commit carries **both**, each with a reason clause:

- `Approved-Cross-Boundary: operations` — for `docs/runbooks/client-dev-local.md` (new
  `## Secure Context and Media Setup`, the F7 severity correction, the §5 de-count, Changelog +
  Last Updated), reason clause naming @operations' R1–R8 ruling.
- `Approved-Cross-Boundary: infrastructure` — for `scripts/dev-web.sh` and `scripts/dev-web.test.sh`,
  reason clause naming the "one-line summary + pointer" ruling and `docs/TODO.md:759` / F-DRY-E as
  the authority.
- **`docs/TODO.md` is discharged under the infrastructure trailer.** The row's filers are
  @dry-reviewer, @infrastructure and @security; the edit is the close-out of an entry whose closing
  decision @infrastructure made and whose closure @dry-reviewer has co-signed as one of its filers,
  and it changes no other entry. Stated explicitly here rather than left inferable, because "which
  trailer covers this row" is exactly the question a Gate-3 auditor asks and the table alone does not
  answer it.

**Gate-3 commitments (@operations):**

1. **ANSWERED at Gate 2 attempt 2 — no timeout change needed.** Playwright reports **10 passed
   (1.4m)**, so the browser suite is **~84 s** wall clock. The 548 s the layer took is dominated by
   Phase-1 cluster bring-up and image rebuild, which is why the layer total is the wrong number to
   read: it would have suggested the suite was at 91% of budget when it is at roughly **14%** of the
   600 s `BROWSER_E2E_TIMEOUT` — far below @operations' ~420 s threshold for raising
   `DEVLOOP_BROWSER_E2E_TIMEOUT`. **No bump in this diff.** This task's spec added ~25 s to a suite
   that was already passing.

   Why the number was worth capturing rather than assumed: on `BROWSER_E2E_TIMEOUT` exhaustion
   `scripts/layer7.sh` prints "exited 124" and emits `FAIL browser-e2e-failed` — the **same terminal
   status as a genuine assertion failure, in the IMPLEMENTER lane** — so a budget exhaustion presents
   as a diff bug and sends triage hunting a defect that does not exist. Two readings of this run
   would each have been wrong in that direction: the layer total (548 s) overstates it, and attempt
   1's browser figure was inflated by the failing spec burning its full 20 s first-media wait. The
   84 s figure is from the green run and is the one to compare against next time.
2. **Rollback atomicity is stated in § Rollback Procedure**, which is filled rather than left as
   template text. The new parity assertions create a coupling that did not previously exist:
   `scripts/dev-web.test.sh` (infrastructure) now asserts prose inside
   `docs/runbooks/client-dev-local.md` (operations), both pinned against `scripts/dev-web.sh`.
   Reverting any one alone reds Layer 3 with a message naming neither of the other two, so revert is
   **all-or-nothing** across those three files plus the `docs/TODO.md:759` flip.

**One follow-up filed as a NEW OPEN ENTRY, deliberately out of scope** (@infrastructure raised it
after their ruling, and has since ruled on both the scope and the tracking shape):
`dev-web.sh`'s secure-context text is surfaced **only** by `--help`; it is not printed on a hard-fail
exit. So the "substance at the point of failure" argument in `docs/TODO.md:759` is weaker than that
entry implies — a developer who hits a hard fail does not see the line unless they separately run
`--help`. This argues *for* keeping the retained line, not against it, so it does not change the
decision. @infrastructure has **declined it in-scope** and I agree with their grounds: printing the
retained line on hard-fail exit changes `dev-web.sh`'s **output behaviour**, not its prose. It needs
its own assertions in `dev-web.test.sh` covering the hard-fail output path (which today asserts only
on the header block and `--help`) and it interacts with the `HARD_FAIL` accumulator and the fail-path
exit. Bolting it on here would make the one genuinely behavioural change in this diff the
least-reviewed thing in it.

**Tracking shape, per @infrastructure's ruling — cross-referenced FROM the closed entry, not buried
IN it.** My first proposal was to fold it into the TODO:759 close-out clause; that was wrong, because
a `- [x]` entry is a record, not a work item, and a live follow-up written into a closed entry's
prose is discoverable only by someone re-reading a checked-off box. That is the definition of a lost
obligation, and it would be a fresh instance of the failure mode `docs/TODO.md` names at its own top.
So:

- **A NEW open `- [ ]` entry** under `## Documentation Hygiene`, owner `infrastructure`, stating the
  gap: `dev-web.sh` surfaces the secure-context text only through the `--help` awk derivation at
  `:106` and never prints it on hard-fail exit, so the anti-flag deterrent does not fire at the
  moment someone reaches for an insecure-origin setting — precisely the moment @security's TODO:759
  argument was about.
- **The 759 close-out clause points at it in one sentence** as the residual that closing 759 exposed.
  It links rather than restates, which also keeps it clear of the duplicate-entry problem.
- The new entry records that this **weakens the premise in 759** — the terminal-half argument assumed
  the substance reaches a reader at the point of failure, and today it does not — while arguing *for*
  keeping the retained line, so it does not disturb the decision this devloop is landing. Written
  explicitly so a future reader cannot cite the weakened premise as grounds to reopen the trim.

### 11. Test plan (targeted, run while iterating; Lead runs Gate 2)

- `sdk-core` (vitest, node): device switch, counter monotonicity, the four session event bridges.
- `sdk-svelte` (vitest browser/chromium): `MediaStore` cells, `subscribeSession` wiring + aggregate
  unsubscribe on unmount, and the re-render granularity test with its positive control.
- `web-app` (vitest browser/chromium): `InMeeting` renders the mute indicator from injected SDK
  state and never from frame counts; all eight §6 slot tokens render; mute click drives the SDK and
  the indicator follows the SDK's echo, not the click; device list renders the empty-label case;
  `e2eBus` projects exactly the three new events with exactly the whitelisted fields.
- `web-app` (vitest node): the extended `bundle-content.test.ts` prod-absence proof.
- `scripts/dev-web.test.sh` (Layer 3) for the runbook anchor + the two new parity assertions.
- Layer 7 `media-loopback.spec.ts` against the live Kind cluster.

---

## Pre-Work

None. The tree was clean at `94e1be9a` and task #19's SDK audio pipeline (the dependency this task
builds on) was already landed.

---

## Implementation Summary

### The shape of the change

The task reads as "build a mute button that doesn't lie". Restated as a mechanism: **in the media
path every "nothing is arriving" condition is ambiguous and must be rendered from an explicit signal
the SDK owns — one authoritative holder per state, one path to the DOM, one path to the test bus.**

That produces a wider class than the task names, and task #19's SDK already produced all four
members; none of them reached `MeetingSessionEventMap`, so none could reach a store, a view or the
bus. So the core of this diff is **one bridge of four events**, read identically by the store, the
view and the bus, rather than four ad-hoc paths added one bug at a time.

| Absence-shaped condition | Authoritative holder | Reaches the UI as |
|---|---|---|
| I muted myself (§5) | `MuteState`, consulted at capture | `muteChanged` |
| far end muted / congested / unreachable / under-filled (§6) | MC, on the wire | `streamAssignments` |
| media is arriving at all (§10) | `FirstMediaObserver` | `firstMediaFrame` |
| the pipeline broke | `AudioPipeline#fault` | `mediaFault` |

### sdk-core

| Item | Before | After |
|---|---|---|
| `MeetingSessionEventMap` | roster + state + error only | + `muteChanged` / `firstMediaFrame` / `streamAssignments` / `mediaFault`, all forwarded verbatim |
| `EgressPipeline` | `#hopSequence` internal | + `get framesSent()` over the SAME field, incremented on the same line as `metrics.frameSent()` |
| `IngressPipeline` | — | + `#framesAccepted`, incremented beside `metrics.frameAccepted()`; `get framesAccepted()` |
| `AudioPipeline` | device fixed at `start()` | + `frameCounts` (allocates per READ, never per frame) and `setCaptureDevice()` — a real swap, with the single teardown registration now reading a `#capture` field |
| `muteState.ts` | — | comments recording that mute does NOT release the device (so the OS indicator stays lit, and "fixing" that breaks instantaneous unmute), and that a frame COUNT is safe only under DTX-off |

No new metric name anywhere. The counters are reads of bookkeeping the pipelines already kept beside
their existing `dt_client_media_frames_*_total` counters — one increment, two readers — so a bus
projection structurally cannot disagree with the metric.

### sdk-svelte

New `MediaStore` with **one `$state` cell per field**, hung off `MeetingStore.media` as a plain
non-reactive field. `bindMeetingSession` keeps its exact signature; the four media subscriptions join
the existing aggregate unsubscribe. Re-render granularity is measured, not claimed: a harness counts
`$derived` recomputations and asserts a mute toggle moves the roster count by zero — with the
positive control that a `participantJoined` does move it, and the inverse direction too.

### web-app

`views/InMeeting.svelte`, rendered by `JoinMeeting` **in addition to** its existing markup once
joined, so no existing testid or fixture moved. The mute indicator reads `store.media.audioMuted`
(written only by the SDK's echo); `aria-pressed` and the visible text come from that same value.
`lib/slotState.ts` maps the eight §6 wire tokens exhaustively by type, plus a deliberately distinct
`awaiting-assignment` for "MC has not answered yet". Device labels and `deviceId` reach the
`<option>` and the `getUserMedia` constraint and nowhere else.

`e2eBus` gains three scalar-only events inside the single existing `__E2E_HOOKS__` block. The frame
count is **sampled** (250 ms) from a getter — never a per-frame emission, which would have put a
test-only channel on the production hot path.

### Layer 7

`media-loopback.spec.ts`: first-media as the functional pass/fail, latency annotated and printed but
never compared, mute proven structurally by a flat send counter, and positive controls on both sides
of the muted window plus a separate assertion on the sample count so a stalled sampler reads as a
harness failure rather than a working mute.

### Docs and scripts

The frozen `## Secure Context and Media Setup` section landed between §2 and §3; F7's stale "may
warn" corrected to the current hard fail plus the presence-only / stale-but-present asymmetry that is
F7's actual reason for existing; the §5 heading de-counted. `dev-web.sh`'s SECURE CONTEXT block
trimmed to one claim plus the anchor per @infrastructure's ruling, with the fingerprints hard fail
gaining a generic counter-message. `dev-web.test.sh` went **66 → 77 live assertions**.

### Five things that were fixed rather than deferred

Recorded here rather than under § Accepted Deferrals, which holds pointers to `docs/TODO.md` entries
and not devloop-local scope reasoning. Each of these had a plausible "file it and move on" path:

| Item | Why it was fixed in-diff |
|---|---|
| F7's stale "may warn" severity | @operations pre-refused a deferral: <5 LoC, inside a file already being edited, no design ambiguity |
| `pointer-names-the-apis` retargeted, not retired | Retiring it would have left the four-API claim guarded nowhere — the reconciliation would have traded guarded prose for unguarded prose |
| The `e2e/` lint gap | `packages/web-app`'s `lint` covered `src/**` only, so this task's own new spec would have shipped ungated. A silently unchecked directory is the masked-failure shape |
| `MeetingSession` media-bridge tests | `validate-cross-boundary-scope` surfaced the row as planned-but-untouched; it was a real hole, since both consumer suites drive a mock session |
| The `no_roster_entry` loopback defect (Gate 2) | It is the story's headline objective. Deferring it would have shipped an in-meeting view that joins and returns no audio |

### Additional Changes

- `packages/web-app`'s `lint` script covered `src/**` only, leaving the entire `e2e/` tree ungated
  for eslint and prettier — including this task's new spec. Widened to `{src,e2e}`, which required
  reformatting a pre-existing drift in `e2e/mcMetrics.ts`.
- `docs/TODO.md:759` closed with the decision recorded, and the `--help`-only residual filed as its
  own open Documentation Hygiene entry rather than buried in the closed one.

---

## Files Modified

```
docs/TODO.md                                       |   52 +-
 .../2026-09-09-web-app-in-meeting-loopback/main.md | 1594 ++++++++++++++++++++
 docs/observability/metrics/client.md               |   15 +
 docs/runbooks/client-dev-local.md                  |  138 +-
 docs/specialist-knowledge/client/INDEX.md          |   37 +-
 docs/specialist-knowledge/security/INDEX.md        |    4 +-
 package.json                                       |    2 +-
 packages/sdk-core/src/index.ts                     |    1 +
 .../sdk-core/src/media/lifecycle/AudioPipeline.ts  |  169 ++-
 .../audioPipeline.deviceAndCounts.test.ts          |  368 +++++
 packages/sdk-core/src/media/lifecycle/muteState.ts |   23 +
 packages/sdk-core/src/media/pipeline/egress.ts     |   29 +
 packages/sdk-core/src/media/pipeline/ingress.ts    |   92 ++
 packages/sdk-core/src/session/MeetingSession.ts    |   73 +
 .../src/session/__tests__/meeting-session.test.ts  |  203 ++-
 packages/sdk-core/src/session/events.ts            |   55 +
 .../sdk-svelte/src/__tests__/MediaStore.test.ts    |  204 +++
 .../src/__tests__/helpers/BoundHarness.svelte      |    6 +
 .../__tests__/helpers/GranularityHarness.svelte    |   38 +
 packages/sdk-svelte/src/index.ts                   |    1 +
 .../sdk-svelte/src/stores/MediaStore.svelte.ts     |  144 ++
 .../sdk-svelte/src/stores/MeetingStore.svelte.ts   |   16 +
 .../sdk-svelte/src/stores/bindMeetingSession.ts    |   13 +-
 packages/web-app/e2e/README.md                     |   44 +
 packages/web-app/e2e/fixtures.ts                   |  315 ++++
 packages/web-app/e2e/mcMetrics.ts                  |    7 +-
 packages/web-app/e2e/media-loopback.spec.ts        |  172 +++
 packages/web-app/package.json                      |    2 +-
 packages/web-app/src/__tests__/e2eBus.test.ts      |  145 +-
 .../src/__tests__/helpers/MockMeetingSession.ts    |   85 +-
 packages/web-app/src/__tests__/inMeeting.test.ts   |  329 ++++
 packages/web-app/src/__tests__/slotState.test.ts   |   79 +
 packages/web-app/src/lib/e2eBus.ts                 |   99 +-
 packages/web-app/src/lib/slotState.ts              |  104 ++
 packages/web-app/src/views/InMeeting.svelte        |  236 +++
 packages/web-app/src/views/JoinMeeting.svelte      |   22 +-
 packages/web-app/tests/bundle-content.test.ts      |   31 +
 pnpm-lock.yaml                                     |   10 +-
 scripts/dev-web.sh                                 |   37 +-
 scripts/dev-web.test.sh                            |  204 ++-
 40 files changed, 5095 insertions(+), 103 deletions(-)
```

### Key Changes by File
| File | Changes |
|------|---------|
| `packages/sdk-core/src/session/events.ts` | Four absence-signals added to `MeetingSessionEventMap`, with the §5/§6/§10 reasoning at the site |
| `packages/sdk-core/src/session/MeetingSession.ts` | Bridges them verbatim — `streamAssignments` in `#wireSignaling` (signalling state, arrives before media starts), the other three right after the pipeline is constructed so nothing it emits is missed |
| `packages/sdk-core/src/media/pipeline/egress.ts` | `get framesSent()` over the existing `#hopSequence`; one-increment-two-readers comment |
| `packages/sdk-core/src/media/pipeline/ingress.ts` | `#framesAccepted` incremented beside `metrics.frameAccepted()`; `get framesAccepted()` |
| `packages/sdk-core/src/media/lifecycle/AudioPipeline.ts` | `frameCounts`, `setCaptureDevice()`, `#capture` field + single field-reading teardown registration, shared `#onCaptureEnded` |
| `packages/sdk-core/src/media/lifecycle/muteState.ts` | Comments: mute does not release the device; DTX-off is what makes a frame count safe to expose |
| `packages/sdk-svelte/src/stores/MediaStore.svelte.ts` | New — one `$state` cell per field, projections only |
| `packages/sdk-svelte/src/stores/{MeetingStore.svelte,bindMeetingSession}.ts` | `media` as a non-reactive field; four subscriptions into the existing aggregate unsubscribe |
| `packages/web-app/src/views/InMeeting.svelte` | New — mute control + indicator, mic picker with a real swap, slot rows, fault surface |
| `packages/web-app/src/lib/slotState.ts` | New — exhaustive-by-type §6 mapping; `awaiting-assignment` deliberately not a wire token |
| `packages/web-app/src/lib/e2eBus.ts` | Three scalar-only events + a sampled counter reader; returns a disposer |
| `packages/web-app/e2e/{fixtures.ts,media-loopback.spec.ts}` | Media helpers and the Layer-7 spec |
| `packages/web-app/tests/bundle-content.test.ts` | Comment recording why event-type literals must never enter `FORBIDDEN` |
| `docs/runbooks/client-dev-local.md` | Frozen secure-context section; F7 severity + presence-only correction; §5 de-count; changelog |
| `scripts/dev-web.sh` | Header trimmed to one claim + anchor; generic counter-message on the fingerprints hard fail |
| `scripts/dev-web.test.sh` | Retargeted + fail-closed + prose/flag parity; 66 → 77 assertions |
| `docs/TODO.md` | `:759` closed with the decision; residual filed as its own open entry |
| `packages/sdk-core/src/session/MeetingSession.ts` (Gate 2) | Publishes its OWN identity key into the roster resolver at join — without it the loopback drops every returned frame `no_roster_entry` |
| `package.json` (Gate 2) | `fast-uri` override floor `>=3.1.4` → `>=3.1.6`; existing entry raised, not duplicated |

## Devloop Verification Steps

Layers 1/2/6 (`cargo check` / `cargo fmt` / `clippy`) are **not applicable** — this diff touches no
Rust. The Lead runs Gate 2; the below are the targeted checks run while iterating.

### Client package suites
**Status**: PASS

| Suite | Result |
|---|---|
| `sdk-core` (vitest, node) | 51 files, **657** tests passed (was 642 — +15) |
| `sdk-svelte` (vitest browser/chromium) | 4 files, **22** tests passed (was 13 — +9) |
| `web-app` component (vitest browser/chromium) | 7 files, **39** tests passed (was 18 — +21) |
| `web-app` node tier (incl. the real production `vite build`) | 3 files, **35** tests passed |
| `lint` (svelte-check + eslint + prettier) in all three packages | PASS |

### Layer 3 guards run directly
**Status**: PASS

| Guard | Result |
|---|---|
| `validate-no-insecure-browser-flags` | `STATUS=OK` — 1619 candidate files, 13 enumerated settings, **0 hits**, allowlist unchanged at 2 |
| `scripts/dev-web.test.sh` | **77 passed, 0 failed** (baseline on `94e1be9a` was 66 — measured by stash, so the "retargeted, never net-reduced" claim is verified against the artifact, not the plan) |
| `validate-cross-boundary-scope` | `STATUS=OK REASON=cross-boundary-scope-no-drift` (see Issue 2 — it caught real drift first) |
| `validate-subdomain-regex-sync` | `STATUS=OK` — 7 sites in sync |
| `validate-doc-citations-no-line-numbers` | `STATUS=OK` — clean across 24 docs (red at Gate-2 attempt 1; see Issue 7) |
| `scripts/lang/ts/audit.sh` (`pnpm audit`) | `STATUS=OK REASON=pnpm-audit-passed` — 4 high → 0 high (red at Gate-2 attempt 1; see Issue 8) |

### Layer 7: Env-tests
**Status**: PASS (Gate 2 attempt 2) — **10 browser tests passed, 1.4m**, including both
media-loopback specs. Attempt 1 failed on `framesAccepted stayed at 0 across 55 samples`, which was
the env-test doing its job: see Issue 6. Layer 7 consumed neither of its two attempts on the green
pass.

`media-loopback.spec.ts` is picked up automatically: `scripts/layer7.sh` Phase 2 runs
`pnpm --filter @darktower/web-app test:e2e` over the whole `e2e/` directory, so there is no new lane,
no diff trigger and no skip path. The per-test ceiling is the config's unchanged `timeout: 120_000`.
Measured suite duration **~84 s** against the 600 s `BROWSER_E2E_TIMEOUT` (~14% of budget); see §12
for @operations' Gate-3 ask, now answered.

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

---

## Accepted Deferrals

**Each entry here is an issue the devloop chose NOT to fix.** Every bullet is a cost shift.

- `docs/TODO.md` §Documentation Hygiene — `dev-web.sh` counter-message fires only via `--help`

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `94e1be9a12800d7cd9d31dc8f47a9c7f6331daef`
2. Review all changes: `git diff 94e1be9a..HEAD`
3. Soft reset (preserves changes): `git reset --soft 94e1be9a`
4. Hard reset (clean revert): `git reset --hard 94e1be9a`
5. No schema changes and no migrations — `git reset` is sufficient on that axis.
6. No infrastructure manifests are applied, so no `skaffold delete` / `kubectl delete -f` is needed.

**REVERT IS ALL-OR-NOTHING ACROSS FOUR FILES.** This diff creates a documentation coupling that did
not previously exist: `scripts/dev-web.test.sh` (infrastructure-owned) asserts prose tokens inside
`docs/runbooks/client-dev-local.md` (operations-owned), and both are pinned against the trimmed
header in `scripts/dev-web.sh`. Reverting any ONE of them alone turns Layer 3 red with a message that
names neither of the other two:

- revert `docs/runbooks/client-dev-local.md` alone → the anchor check fails
  (`secure-context-heading-missing`) and every parity assertion fails;
- revert `scripts/dev-web.sh` alone → the restored four-statement header is inconsistent with the
  runbook that now claims to be canonical for the enumeration, and `docs/TODO.md:759` is closed
  against a state that no longer exists;
- revert `scripts/dev-web.test.sh` alone → the fail-closed anchor check and both parity assertions
  disappear silently, which is the vacuity state this diff exists to remove.

So a partial revert must not be attempted. Revert together:
`scripts/dev-web.sh`, `scripts/dev-web.test.sh`, `docs/runbooks/client-dev-local.md`, and the
`docs/TODO.md:759` close-out flip. The code halves (`packages/**`) are independently revertable and
carry no cross-file documentation coupling.

---

## Issues Encountered & Resolutions

### Issue 1: My own plan document would have failed Layer 3
**Problem**: Planning §10 enumerated the prohibited browser-trust flags as a compliance statement.
`dt-guard no-insecure-browser-flags` collects candidates **tracked-only**, `.md` is a candidate
suffix, and docs are deliberately in scope — so the file would have produced five hits across all
three rule classes the moment it was committed. The section imposing "describe the class, never
print the literal" on the runbook violated that rule three paragraphs later.
**Resolution**: @security ran the guard rather than reading the section charitably, and caught it at
Gate 1. §10 rewritten to name three classes and print nothing; verified by extracting the guard's
full vocabulary and sweeping the file, then by running the guard with everything staged (1619 files,
0 hits). No allowlist entry — the allowlist stays at 2.

### Issue 2: The bundle-content mechanism I proposed would have red-ed a correct build
**Problem**: The plan proposed adding the three bus event-type strings to `bundle-content.test.ts`'s
`FORBIDDEN` list as the prod-absence proof. Two of them name **production** concepts:
`firstMediaFrame` is emitted by sdk-core's `AudioPipeline` and `muteState` is a production getter on
the same class, and both ship in the web-app bundle. The test would have gone red on a correct
production build, and the natural fix would have been to weaken it — the ADR-0028 vacuity trap,
introduced by the forcing function meant to prevent it.
**Resolution**: @code-reviewer checked the two call sites at source. `FORBIDDEN` gains nothing; the
existing `__darktower_test__` / `__E2E_HOOKS__` markers already prove elimination of everything
inside the single gate, including the new sampler. A comment now records why event-type literals
must never be added, citing both production sites — a durable control rather than a silent
retraction.

### Issue 3: The reconciliation was a coverage reduction wearing a cleanup's clothes
**Problem**: Plan §9 retired `pointer-names-the-apis` on the grounds that the four-API enumeration
*moved* to the runbook, while the replacement parity assertion carried only two tokens. Net effect:
the enumeration would have been guarded **nowhere** — not the header (assertion gone), not the
runbook (never covered). The claim "retargeted, never net-reduced" was false on its own terms, since
a comment is a note to a human, not a control.
**Resolution**: @security (F2) and @operations (R8) reached this independently. Retargeted rather
than retired. @operations then found R9 — `dev-web.test.sh`'s own comment says *"Story task 20
asserts the same fact from the runbook side; the two must not contradict"*, an in-repo obligation
naming this task that had sat unfulfilled with only a promise on the runbook side. The runbook
extraction now carries eight assertions under per-claim reason tokens plus a separate non-empty
vacuity guard. Suite went 66 → 77 live assertions, measured by stash rather than asserted.

### Issue 4: A Lead ruling reversed on the file owner's evidence
**Problem**: The Lead ruled the fingerprints hard-fail counter-message out of scope as "a
behavioural change to failure output". @infrastructure, who owns the file, checked the branch instead
of reasoning from the category: `dev-web.sh:294-299` is an existing `fail` plus three `echo`s, and
`dev-web.test.sh:148-162` is already fully driven with a positive control — so it is a string edit
with a one-line test, and the deferral failed all three counts of the review protocol's
suspicious-deferral check.
**Resolution**: I stopped and surfaced the conflict rather than picking a side; the Lead reversed on
that evidence. Recorded in the classification table because the audit trail is the point: the clause
entered by owner ACK after a reversal, which is the cross-boundary process working rather than being
bypassed. @infrastructure separately warned that the natural phrasing would red the flags guard on
their own file — the clause names the class and prints no literal.

### Issue 5: A planned test row the guard caught as never written
**Problem**: `validate-cross-boundary-scope` reported `scope_drift_planned_untouched` for
`session/__tests__/*.test.ts`. The four event bridges in the real `MeetingSession` had no test: both
`sdk-svelte` and `web-app` drive a **mock** session, so every test above the bridge would stay green
while a joined participant watched an indicator that never moved.
**Resolution**: Written, not deleted. First attempt duplicated the 30-line GC/AC fetch stub into a
new file; that was thrown away and the tests moved into `meeting-session.test.ts` to reuse the
existing rig — a second copy of the join response in a sibling file is how two versions of it start
to disagree. `driveJoin` gained an optional `joinInit` because `startMedia()` fails closed without a
sender id, which had made the first version of the ordering assertion pass on an empty list for the
wrong reason.

### Issue 6: Gate 2 — the env-test found a real integration gap, and the reported diagnosis was wrong

**Problem**: `media-loopback.spec.ts` failed with `framesAccepted stayed at 0 across 55 samples`,
while `firstMediaFrame` reported a 31 ms round trip and the slot-state test passed. The Gate-2
report read that as a counter-wiring bug — "a counter that is 0 when the thing it counts demonstrably
happened" — on the inference that first-media proves decoded audio came back.

**It does not.** `IngressPipeline.accept()` calls `this.#firstMedia.onFrameReceived()` as its FIRST
statement, before `decodeFrame` — deliberately, because that metric is defined at the wire. So the
31 ms proved a datagram *arrived*, not that any frame was accepted. `framesAccepted = 0` was
truthful, and the counter was right: **every returned frame was being dropped.**

**Root cause**: MC's roster carries `existing_participants` — everyone except us — so
`SignalingClient.#feedRosterKeys` never learns this client's own identity key. In the
one-participant loopback every frame MH returns carries `key_id.sender_id == our own id`,
`identityKeyFor` returns `undefined`, and the receive path fails closed on `no_roster_entry`. Not
some frames: all of them, silently, with audio visibly arriving at the wire. This is exactly the
failure mode the whole story is about, and it is the one thing no unit-tier loopback could have
found — every one of them seeds its own roster by hand.

**Resolution**: `MeetingSession` now publishes its own `(senderId, identityPublicKey)` into the
roster resolver once the join settles. **This is not a self-trust branch** — the story spec forbids
one and `ingress.ts` says so explicitly. Verification still runs: a frame must carry a signature that
verifies against that key, and only the holder of the matching private key can produce one, so a
forged frame claiming our sender id is rejected exactly as before. The ingress path keeps its single
uniform "resolve the key for `key_id.sender_id`, then verify" rule with no `if (senderId === mine)`
anywhere; we are supplying the key for a sender the roster structurally cannot tell us about.

Pinned by a regression test asserted at `RosterIdentityKeys.upsert` — the same seam
`#feedRosterKeys` uses for peers — rather than on a private field, and **verified to fail without
the fix** (commenting out the call reds it with the intended message). It also asserts the key is 32
bytes, because `upsert` records an *absence* for a wrong-length key rather than throwing, so a
call-count-only assertion would pass on a zero-length "key" that left the resolver empty.

**What the test design got right, and is deliberately unchanged**: the failure message named exactly
what to compare (`framesAccepted` against `framesSent`), and the separate sample-count assertion kept
this from being reported as a working mute.

### Issue 7: Gate 2 — a bare line-number citation, in the same rot class I was removing

**Problem**: `validate-doc-citations-no-line-numbers` red on
`docs/observability/metrics/mc-service.md:351`, cited in my runbook changelog entry. The *rule* it
points at is right and was @dry-reviewer's Gate-1 suggestion; the bare line-number *form* is what the
guard forbids — and it is the same rot class as the `(F1–F11)` count being removed two sections away
in the same commit. A second encoding of a location that drifts on the next edit.

**Resolution**: cited by section heading instead —
`docs/observability/metrics/mc-service.md` §`mc_media_sender_binding_responses_total`.

### Issue 8: Gate 2 — a pre-existing advisory surfaced by a one-line lint-scope change

**Problem**: `pnpm audit` red on four high `fast-uri` advisories reaching `packages/sdk-core` through
`vite-plugin-dts -> @microsoft/api-extractor -> @microsoft/tsdoc-config -> ajv`. Pre-existing and
unrelated to this work; the gate fired because the `packages/web-app/package.json` lint-scope change
tripped the dep-manifest gate.

**Resolution**: not deferred — the layer was red and the tree already has the mechanism. Worth noting
what the fix was *not*: a `fast-uri` override **already existed** at `>=3.1.4 <4`, so this is a floor
raise to `>=3.1.6` rather than a new entry, and the `<4` ceiling (ajv needs v3) is untouched. No
advisory suppressed, no suppression list widened. 4 high → 0 high; the 2 remaining moderates sit
below the gate threshold and are unaffected by this change.

### Issue 9: Gate 2 attempt 2 — two self-inflicted Layer 3 violations from the attempt-1 edits

**Problem (a)**: `validate-knowledge-index` — `docs/specialist-knowledge/client/INDEX.md` hit 84
lines against a 75 cap. My six new navigation rows pushed it over.

**Resolution**: consolidated rather than trimmed. The new rows are the ones a future client
specialist needs for this story's surfaces, so instead four `## Browser E2E Harness` rows, four
`## Client Test Utilities` rows, two telemetry rows and three `## SDK Core` rows were merged into
single lines — the INDEX is a navigation map, and a map with one entry per file is a directory
listing. 84 → **74**, one line of headroom. The Gate-2 roster self-seed surface was folded into the
existing key-material row at zero line cost, because that is where someone looking for it would
already be.

**Problem (b)**: `validate-todo-tracking` — `[inline_debt_body]` on § Accepted Deferrals. My
"nothing else was deferred, and here is what was fixed instead" paragraph is real content, but that
section holds one-line pointers into `docs/TODO.md` and nothing else; devloop-local scope reasoning
belongs in the scope or summary sections.

**Resolution**: the narrative moved to § Implementation Summary as *Five things that were fixed
rather than deferred*, and gained the Gate-2 roster defect as a fifth row. § Accepted Deferrals is
now the single pointer bullet it is supposed to be.

### Issue 10: Gate 3 — the runbook stated a symptom that cannot happen, and three reviewers derived it three different wrong ways

**Problem** (@security A, @operations F-OPS-4, @infrastructure F3, all independently): my Secure
Context section said a non-loopback HTTP origin "joins, shows the meeting, lists participants — and
returns no audio". It cannot. **WebTransport is one of the four gated APIs**, MC signalling runs over
it with no fallback, so the join dies at `connecting-mc` and never reaches `joined`. The section
contradicted itself: it correctly listed WebTransport as gated four paragraphs above. Worse,
`dev-web.test.sh` pinned the false claim on **both** sides — so had only one side been corrected the
parity assertion would have red-ed on true prose, and had both been "corrected" to the same wrong
text the guard would have become positive evidence that the defect was right.

**What happened next is the more interesting half.** Three reviewers each supplied a corrected
symptom, and **all three were wrong, in three different directions**:

| Source | Claimed the user sees | Actually |
|---|---|---|
| @security (first) | an error naming WebTransport as unavailable | the message is replaced before rendering |
| @operations | "not available in this **environment**" → misread as *browser unsupported* | that text never reaches the UI |
| @infrastructure | same misattribution reading | same |

Each was plausible, internally consistent, and derived from reading rather than running. @operations
retracted theirs in the same breath as having told me to derive strings from code and not from review
threads — and said so explicitly, which is the right way for that to go.

**Resolution**: I derived it by **execution**, per @infrastructure's instruction, not by reading.
The first harness said `Unexpected error` — which was itself an artifact: importing `MeetingSession`
by absolute path and `errorText` through the package alias put two copies of `SdkError` in the module
graph, so `instanceof` failed. Re-run through the real alias, the answer is:

```
SIGNALING: Signaling transport closed
```

**byte-identical to the string F1 already documents** (`client-dev-local.md`, F1's Symptom) — I
checked, and they agree, which is the whole point. The `WebTransport is not available in this
environment` text survives only on `Error.cause`, which `errorText` deliberately never renders
(R-23). So there is **no UI-visible discriminator at all**: the origin fault is indistinguishable
from an MC-reachability failure at rung 1, and the address bar is the discriminator precisely because
the error is useless rather than because it is informative. The prose now says that, quotes the
string, and offers the devtools cause-expansion as an explicit not-in-the-UI confirmation.

`"no audio"` was retired as the pinned token (it now belongs solely to F7's genuine case — a secure
origin whose media path fails) and replaced with `connecting-mc` on both sides.

### Issue 11: Gate 3 — assertions that pinned everything except the sentence they existed to protect

**Problem** (@security E): this diff put the anti-flag counter-message in three places and pinned
**one**. The `dev-web.sh` header comment claimed the retained duplicate was "guarded on both sides",
while four assertions pinned *which API names appear* and zero pinned the sentence §9's whole
retention argument rests on. And `dt-guard no-insecure-browser-flags` does not cover it: that guard
denies a prohibited literal being **added**; nothing detected the counter-message being **removed**.

**Resolution**: `pointer-warns-against-browser-flag` and `runbook-warns-against-browser-flag`, both
pinning phrases that name no prohibited literal. Plus `pointer-states-the-symptom` — @operations and
@infrastructure both noticed `pointer-says-non-loopback` pinned the *word* `NON-LOOPBACK`, which
survives any rewrite, so the header could have lost the symptom entirely while staying green.

### Issue 12: Gate 3 — the remaining findings

- **@observability**: the bus projected two terms of a three-term identity, so "accepted flat" carried
  no reason and the Gate-2 state was indistinguishable on the bus from MH never forwarding. My own
  failure text *stated* that ambiguity instead of resolving it. `framesReceived`, `framesDropped` and
  the bounded `lastDropReason` now ride the same one-increment-two-readers rule, and one
  `diagnoseCounters()` renders the reading — "arriving and being REJECTED" vs "nothing came back".
- **@test**: `slotState.test.ts`'s `WIRE_TOKENS` was hand-maintained, and TypeScript does not force an
  array to be exhaustive over a union — so a ninth §6 token would have dropped silently out of the
  distinctness test while `toHaveLength(8)` still passed. Now derived from `SLOT_STATE_TEXT`'s keys,
  which the compiler *does* force exhaustive.
- **@dry-reviewer**: `expectEgressAdvances` / `expectIngressAdvances` were one algorithm twice, with a
  duplicated "find the newest sample" predicate. Collapsed to a parameterised helper; both distinct
  diagnoses preserved via a message-builder argument.
- **@infrastructure**: the flag-parity check hardcoded a *fifth* copy of the flag pair it existed to
  keep in sync, and could not see a flag the runbook documents and the config does not. Now derived
  from `playwright.config.ts` with two positive controls. Also `assert_runbook_claim`, so the eight
  prose failures route themselves to the operations-owned file instead of emitting a bare needle.
- **@operations**: changelog restored to chronological order; the three `§4.5` pointers (to a section
  that lands with task #21) re-pointed at `§4`, which exists; F7 gained the cert-validation
  counter-message, since the stale-but-present case is the one branch where `dev-web.sh`'s own warning
  never prints.
- **@observability (2nd round)**: `lastDropReason` was typed `string` while the comment beside it
  claimed boundedness *by construction* — and that comment was the basis on which the token had been
  cleared of the cardinality and PII hazards. The type and the claim disagreed, and the type was the
  one a future assignment would obey: nothing stopped a `DOMException.message` landing there and
  compiling. Now `RejectReason | undefined` at all three sites (field, getter, `MediaFrameCounts`),
  matching `MediaMetrics.frameDropped(reason: RejectReason)` beside the call sites. Verified by
  NEGATIVE CONTROL rather than by inspection — assigning an arbitrary string produces
  `TS2322: Type 'string' is not assignable to type 'RejectReason | undefined'`, so the guarantee is
  structural. `e2e/fixtures.ts` deliberately keeps `string | undefined`: it deserializes from an
  untyped bus, and asserting the union there would claim a guarantee runtime data cannot carry.
- **@security C/D**: `#seedOwnRosterKey` records that the entry is replaceable by a later roster
  message exactly like a peer's — bounded by MC already being the key distributor, not by anything
  special here. `bundle-content.test.ts`'s rule restated as the *token* test ("exists nowhere in
  production") rather than the over-broad category ban, with `mediaFrameCounts` added as the valid
  marker that ban would have excluded.

---

## Lessons Learned

1. **Three of the four Gate-1 defects were in the documentation reconciliation, not the media code.**
   The media work had a governing ADR, a landed dependency and a clear shape. The §9 script/runbook
   reconciliation had none of that, and it is where every real mistake landed — including writing a
   compliance section that would have failed the compliance guard. Prose edits that move a guarded
   claim between files deserve the scrutiny a refactor gets.
2. **"Retired with a comment, not silently dropped" is not a control.** Two reviewers independently
   read past that sentence to ask where the claim ends up asserted, and the answer was nowhere. When
   an SSoT move relocates prose, count the assertions on both sides before and after — and measure
   it, since the claim is about the artifact.
3. **Verify mechanically even when the claim is about your own diff.** My insecure-flag sweep used a
   term list I had extracted by hand, with nothing proving the list was complete; @security re-derived
   the vocabulary from the guard and used the guard's own count as the positive control. A sweep with
   an unverified needle list reports clean for the same reason a vacuous assertion does.
4. **A forcing function can introduce the failure it was meant to prevent.** The `FORBIDDEN` proposal
   was a genuine attempt at a mechanical prod-absence proof and would have produced a red build on
   correct code, whose obvious remedy is weakening the test. Before adding a token to a deny list,
   check that it names something which exists *only* on the wrong side of the gate.
5. **An env-test earns its cost the first time it runs, and its failure message is most of the
   value.** The loopback found a defect that four unit tiers structurally could not: every
   lower-tier loopback seeds its own roster, which is precisely the step that was missing. The
   diagnosis then hinged on knowing that `firstMediaFrame` is measured at the wire rather than after
   decode — two adjacent counters that mean different things, where the plausible reading of the
   pair pointed at the wrong component. Metrics whose boundary is not obvious from the name should
   say where they are taken, at the point a reader will compare them.
6. **Three reviewers derived the same string three wrong ways, and only execution settled it.** The
   corrected symptom arrived with three independent, mutually inconsistent readings attached, each
   from someone who had the source open. Running the path took one scratch test — and even that lied
   the first time, because a dual-module-graph `instanceof` failure made a real `SignalingError`
   render as `Unexpected error`. A derivation that is not executed is a hypothesis, and a harness
   that has not been sanity-checked is one too.
7. **Stopping on a conflict was cheaper than resolving it.** The Lead and the file owner disagreed
   about scope; surfacing it took one message and produced a better outcome than either picking the
   Lead's ruling (which would have left the diff net-worse at the thing it was about) or picking the
   owner's (which would have overridden an explicit instruction).

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

---

## Gate 2 — Validation

### Attempt 1 (2026-09-09) — TOTAL_RESULT=FAIL

Run unattended (`DEVLOOP_FAIL_FAST=0`), so all seven layers were evaluated — no layer is `NOT-RUN`
and no red is unmeasured.

| Layer | Result | Reason |
|-------|--------|--------|
| 1 Compile | OK | |
| 2 Format | OK | |
| 3 Guards | **FAIL** | `bare-line-cites-1-violation-s-across-24-doc-s` |
| 4 Test | **N/A** | `test-aggregate-na` — see "Not the implementer's" below |
| 5 Lint | OK | |
| 6 Audit | **FAIL** | `pnpm-audit-failed` |
| 7 Env-tests | **FAIL** | `browser-e2e-failed` |

`TOTAL_DURATION=858`, Layer 7 `DURATION=560`.

**Defect 1 — Layer 3, `validate-doc-citations-no-line-numbers`.**

```
VIOLATION: docs/runbooks/client-dev-local.md — doc-citations-line-numbers-found —
docs/runbooks/client-dev-local.md:1238: docs/observability/metrics/mc-service.md:351
```

The `mc-service.md:351` citation @dry-reviewer supplied at Gate 1. The *rule* it cites is right; the
bare line-number *form* is what the guard forbids — and it is the same rot class as the `(F1–F11)`
count being removed two sections away in the same file. Fix: cite by section heading.

**Defect 2 — Layer 7, `media-loopback.spec.ts` test 1. A real bug, NOT the integration signal the
implementer flagged as possible.**

```
Error: decoded audio did not resume (after unmute): framesAccepted stayed at 0
across 55 samples. Frames may be leaving without coming back — compare framesSent.
```

The run's own evidence rules out the MH/MC explanation: `observed round trip to first decoded frame:
31ms` printed, and **test 2 (slot state rendered from the wire) PASSED** — so MC filled the slot and
decoded frames genuinely came back. `framesAccepted` was **0**, not merely flat, while the thing it
counts demonstrably happened. That is the ingress half of the "one increment, two readers" claim
failing: `firstMediaFrame` fires from one path and the counter increments on another, or the getter
reads a different pipeline instance than the one decoding.

Worth recording because it is the test design working: the failure message named exactly what to
compare, and the separate sample-count assertion stopped a zero counter from reading as a working
mute. Both were @test's and @infrastructure's Gate-1 vacuity requirements, and they are what made
this diagnosable in one read instead of a bisect.

**Defect 3 — Layer 6, `pnpm audit`.** Four `fast-uri` high advisories (GHSA-5jgf-p345-68v8,
GHSA-f65p-4m7j-42xc, GHSA-fph4-wmhf-6fwf, +1), all via `packages/sdk-core > vite-plugin-dts >
@microsoft/api-extractor > @microsoft/tsdoc-config > ajv > fast-uri`; vulnerable `<3.1.6`. The
advisories are pre-existing and unrelated to this task — the gate fired because the one-line
`packages/web-app/package.json` lint-scope change tripped the dep-manifest gate. Red is red: routed
to the existing `package.json:pnpm.overrides` transitive-security mechanism, **not** to a
suppression-list entry.

### Not the implementer's — Layer 4 issues no verdict while every test passes

```
STATUS=N/A REASON=not-applicable-to-this-lang    (proto — documented intentional gap)
STATUS=OK  REASON=cargo-test-passed              (rust — 392 tests, 0 failed)
STATUS=OK  REASON=nx-test-passed                 (ts — all suites)
STATUS=N/A REASON=test-aggregate-na              (aggregate)
```

Two OK children and one documented `N/A` aggregate to `N/A`, so Layer 4 cannot report OK at all
while proto's placeholder exists, and `TOTAL_RESULT` absorbs it silently. The devloop skill draws a
hard line between a self-justifying `SKIPPED-NO-DIFF` and an unexpected `N/A` that signals a wrapper
bug, and instructs the Lead not to rationalize the latter — but a Layer 4 that reports `N/A` on
*every* run trains exactly that rationalization, and after the tenth time nobody looks. The
masked-failure shape is that a genuine aggregation regression would be indistinguishable from this
steady state; confirming the tests really passed required reading the children's own STATUS lines,
which is the manual step the ADR-0033 §4 contract exists to remove.

Nothing in this diff touches `scripts/layer4.sh`, `scripts/test.sh`, or the per-language wrappers.
Routed to @operations + @infrastructure to file as ONE `docs/TODO.md` entry at verdict time; the
underlying question (should a documented `N/A` child suppress an otherwise-OK aggregate, or should
worst-child treat `N/A` as neutral?) is an ADR-0033 §4 amendment, not only a script fix. The
implementer was told explicitly not to fix it here.

### Attempt 2 — TOTAL_RESULT=FAIL (Layer 7 GREEN, two new Layer-3 violations)

Layers 6 and 7 cleared. Layer 3 red on two violations **introduced by the attempt-1 fixes**, not
carried over: `validate-knowledge-index` (`docs/specialist-knowledge/client/INDEX.md` at 84 lines
against a 75 cap) and `validate-guards` (`main.md:1053 [inline_debt_body]` — a tech-debt narrative
inlined in § Accepted Deferrals where the skill requires pointer bullets). Both self-inflicted and
both locally reproducible; the implementer had re-run only the two previously-failing guards rather
than the suite.

**The Lead's defect-2 diagnosis in attempt 1 was WRONG, and the implementer corrected it with
source evidence.** The report read `firstMediaFrame: 31ms` as proof decoded audio returned and
concluded the counter was miswired. It is not proof: `IngressPipeline.accept()` calls
`onFrameReceived()` as its **first statement, before `decodeFrame`**, because that measurement is
defined at the wire. `framesAccepted = 0` across 55 samples was truthful — every returned frame was
being dropped.

**Root cause — a real integration defect no unit tier could reach.** MC's roster carries
`existing_participants`, i.e. everyone *except* us. In a one-participant loopback every frame MH
returns carries our own `sender_id`, `identityKeyFor` returns `undefined`, and the receive path
fails closed on `no_roster_entry` — all frames, silently, with audio visibly arriving at the wire.
That is precisely the failure mode this story exists to eliminate. Every lower-tier loopback test
hand-seeds its roster, which is exactly the step that was missing, so only a live-cluster env-test
could surface it. Fix: `MeetingSession` publishes its own `(senderId, identityPublicKey)` into the
roster resolver at join — **not** a self-trust branch (the story spec forbids one, and `ingress.ts`
says so). Verification still runs against that key and only we hold the private half, so a forged
frame claiming our sender id is rejected exactly as before; ingress keeps one uniform
resolve-then-verify rule with no `if (senderId === mine)` anywhere. Pinned by a regression test at
the `RosterIdentityKeys.upsert` seam, **verified to fail without the fix**, asserting the 32-byte key
length because `upsert` records an *absence* for a wrong-length key rather than throwing — a
call-count-only assertion would have passed on an empty resolver.

This is the single most valuable thing the devloop produced, and the mechanism that produced it was
@test's and @infrastructure's Gate-1 vacuity requirements: the failure message named exactly what to
compare, and the separate sample-count assertion stopped a zero counter from reading as a working
mute.

### Attempt 3 — GATE 2 PASSED

```
LAYER=1 OK  LAYER=2 OK  LAYER=3 OK  LAYER=4 N/A
LAYER=5 OK  LAYER=6 N/A LAYER=7 OK  TOTAL_RESULT=N/A
```

`layer-all.sh` wrapper exit 0; **zero `STATUS=FAIL` anywhere in the run**. `TOTAL_DURATION=557`.

`TOTAL_RESULT=N/A` is the pre-existing aggregation defect documented above, not a verdict on this
diff. Rather than accept the aggregate — which the skill says is the authoritative line, and which
here cannot express success — every child STATUS was read directly:

```
Layer 4:  proto N/A (intentional gap) · rust OK cargo-test-passed · ts OK nx-test-passed
Layer 6:  proto N/A (intentional gap) · cargo-audit OK · pnpm-audit OK · buf-breaking OK
Layer 7:  env-tests OK · browser-e2e OK
```

The only `N/A` values in the entire run are proto's two documented intentional-gap placeholders,
whose own `REASON=not-applicable-to-this-lang` is their justification per ADR-0033 §6.

**@operations' Gate-3 timing ask, answered.** Playwright: **10 passed (1.4m)** — the browser suite is
~84s against the 600s `BROWSER_E2E_TIMEOUT`, about **14% of budget**, well under the ~420s threshold
@operations set. **No `DEVLOOP_BROWSER_E2E_TIMEOUT` bump in this diff.** The new spec adds ~25s to an
already-passing suite. Note the two available misreadings, both of which would have argued for a
phantom bump: the Layer-7 total (548s on attempt 2, 273s here) is dominated by Phase-1 cluster
bring-up and image rebuild, and attempt 1's browser figure was inflated by the failing spec burning
its full 20s first-media wait. The 84s green-run figure is the one to compare against.

---

## Gate 3 — Reviewer Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | **RESOLVED-FIXED** | 5 | 5 | 0 | Finding A: runbook stated a symptom the code cannot produce |
| Test | **RESOLVED-FIXED** | 1 | 1 | 0 | `WIRE_TOKENS` was not compile-forced exhaustive (vacuity mech. 4) |
| Observability | **RESOLVED-FIXED** | 3 | 3 | 0 | bus projected 2 of a 3-term identity; first-media boundary; `lastDropReason` typed `string` |
| Code Quality | **CLEAR** | 0 | 0 | 0 | mandatory ADR Compliance + Ownership Lens sections supplied |
| DRY | **RESOLVED-DEFERRED** | 1 | 1 | 0 | verdict driven by 2 filed extraction opportunities, not by an unfixed finding |
| Operations | **RESOLVED-FIXED** | 4 | 4 | 0 | hunk-ACK given for every runbook hunk |
| Semantic Guard | **CLEAR** | 0 | 0 | 0 | verified the roster self-seed is not a self-trust branch |
| Infrastructure | **RESOLVED-DEFERRED** | 3 | 3 | 1 | the accepted deferral is the `--help` gap infrastructure declined themselves |

No reviewer ESCALATED. Two RESOLVED-DEFERRED verdicts, neither carrying an unfixed finding about
this diff — see the two Lead rulings below.

### Lead ruling 1 — @dry-reviewer's RESOLVED-DEFERRED stands

@dry-reviewer flagged the tension for me rather than picking: their one actual finding was fixed and
nothing was deferred or spun out, but the verdict template enumerates "DRY extraction opportunities"
under §Accepted Deferrals and makes a non-empty section decide the verdict, while the ADR-0019
exception says extraction opportunities do not enter the fix-or-defer flow. **Upholding the literal
reading.** Both filed entries are genuine cost shifted to future work, and the protocol's own stated
rationale is that a single bullet there should not be averaged away by the fixes. The substantive
state — one finding found and fixed — is recorded above so the verdict does not overstate it.

### Lead ruling 2 — @operations' RESOLVED-FIXED stands

Their `docs/TODO.md` §Process entry is a finding about *this review*, not about the diff; nothing of
theirs remains in it. §Accepted Deferrals is scoped to "issues that were findings and remain in the
diff", so it does not apply. Folding a co-owned `review-protocol.md` edit into an ADR-0036
client-media diff would have smuggled it past its co-owners.


### Convergent findings — three reviewers, independently

**The dangling `§4.5` references** (`client-dev-local.md:272`, `:294`) were found by @security, @operations
and @dry-reviewer separately, from three different directions: as a broken diagnosis hand-off, as a
repeat of the dead-pointer failure a previous devloop filed a whole TODO entry about, and as a
forward reference that reads present-tense. §4.5 lands with operations' task #21. Resolution:
point at **§4**, whose §4.0 already is a triage ladder, so the reference is correct today and stays
correct after §4.5 exists — and @operations took the re-pointing as their own task-#21 edit rather
than handing back debt.

**@security Finding A is the most consequential of the review.** The new runbook section correctly
lists WebTransport among the four secure-context-gated APIs, then states that a non-loopback HTTP
origin leaves a developer "joined… and returns no audio". Joining *requires* WebTransport:
`SignalingClient` has one transport with no HTTP fallback, and `BrowserWebTransport.ts:90-98` throws
a typed error, so the join dies at `connecting-mc`. The section states a symptom that cannot occur —
and `dev-web.test.sh:411` now pins `"no audio"` as the symptom token, locking the false claim in on
both sides so that correcting it would look like a regression. The practical cost is exactly what
the section exists to prevent: a developer told to expect silence matches a loud failure to F1, works
the MC ladder, finds nothing, and reaches for a flag.

That is worth recording as a pattern: the parity assertions this devloop added are a mechanism for
keeping two texts agreeing, not for keeping either one true. A pinned claim is harder to fix than an
unpinned one.

### A Lead error corrected by a reviewer

In the Start Review brief I suggested the `no_roster_entry` drop path would present to an operator as
silence with healthy-looking upstream counters. @observability verified otherwise: the path is
instrumented at `ingress.ts:196` and surfaces as
`dt_client_media_frames_dropped_total{reason="no_roster_entry"}`, wired through
`configureTelemetryIfEnabled`. The diagnostic gap was in the **test channel**, not in production —
which is why their finding targets the e2e bus rather than the metrics catalog.

---

## The Relay Failure — the loop's most transferable output

@operations filed a candidate sixth assertion-vacuity mechanism in `docs/TODO.md` §Process /
Review-Protocol, co-signed by @infrastructure. It is worth restating here because it fired **three
times inside one devloop, from three different roles**, and because the existing five mechanisms in
`review-protocol.md` cover assertions in code — not reviewer-to-reviewer claims.

**The mechanism**: a composite claim whose *cheap* limb is verified launders its *expensive* limb, so
the composite travels with the credibility of its cheapest part. Its tell is that it looks maximally
rigorous exactly when it is most wrong.

The three instances:

1. **@operations** — in the same message instructing @implementer to derive strings from code rather
   than from review threads, supplied a claim about that string derived from a review thread. They
   caught and retracted it themselves, explicitly, to both recipients.
2. **@infrastructure** — verified the cheaply-falsifiable limb of the same finding (state machine,
   missing fallback: one grep each) and relayed the expensive limb (what the rendered error *reads
   as*, five hops from the throw site) on trust. It reached @implementer as an instruction within
   one hop.
3. **The Lead (me)** — at Gate 2, verified that `framesAccepted` was 0 and inferred that the counter
   was miswired from a plausible reading of `firstMediaFrame: 31ms`, without tracing where that
   measurement is taken. @implementer corrected it from source. That inference sent the implementer
   after the wrong defect and would have cost an attempt had they accepted it.

**The invariant that only becomes visible with all three side by side**: the correction always came
from whoever *executed*, never from whoever read more carefully. Re-reading does not fix this class.

**And the caveat @implementer's near-miss forced onto the remedy**: "run it" is necessary but not
sufficient. Their *first* execution also lied — it returned `Unexpected error`, an artifact of two
`SdkError` copies in the vitest module graph making `instanceof` false; the true string appeared only
through the real module alias. A run through a harness whose module graph differs from production is
itself another cheap limb laundering an expensive conclusion, one level down. The filed entry
therefore requires stating the environment a run happened in alongside its result.

Three reviewers independently produced a corrected symptom for one sentence of runbook prose and all
three were wrong, in three different directions. One wrong correction reads as an individual slip;
three reads as a property of reviewing prose about runtime behaviour without running it.

---

## Gate 2 — Final Re-run (post-review)

The diff changed materially during review, so the full pipeline was re-run before commit:

```
LAYER=1 OK  LAYER=2 OK  LAYER=3 OK  LAYER=4 N/A
LAYER=5 OK  LAYER=6 N/A LAYER=7 OK  TOTAL_DURATION=853
```

Wrapper exit 0, **zero `STATUS=FAIL` anywhere**. The only `N/A` values in the run are proto's two
documented intentional-gap placeholders (`not-applicable-to-this-lang`) and the two aggregates they
dominate — the pre-existing defect filed against `docs/TODO.md:487`.

Final state: sdk-core 658 · sdk-svelte 22 · web-app component 39 · web-app node 36 · lint clean in
all three packages · **41/41 Layer 3 guards** · `dev-web.test.sh` **79 passed, 0 failed** (66 at
baseline → 77 → 79; up every round, never down — @infrastructure's "retargeted, never net-reduced"
condition, verified against the artifact by its owner) · `pnpm audit` 0 high · browser suite ~84s of
a 600s budget.

