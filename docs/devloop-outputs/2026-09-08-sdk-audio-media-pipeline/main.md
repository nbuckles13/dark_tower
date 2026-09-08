# Devloop Output: SDK Single-Client Audio Media Pipeline

**Date**: 2026-09-08
**Task**: Build the single-client audio media pipeline in `@darktower/sdk-core` — capture → Opus encode → SFrame encrypt + Ed25519 sign → QUIC datagram → MH → receive → verify → decrypt → Opus decode → playback.
**Specialist**: client
**Mode**: Agent Teams (v2), full, HEADLESS (run-story task #19)
**Branch**: `feature/hear-yourself-through-handler`
**Duration**: ~4h20m (setup → commit; three full pipeline runs)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `a99e10787dd60823590a46684c24f0faea519a83` |
| Branch | `feature/hear-yourself-through-handler` |
| Lead Model | `claude-opus-5` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` (client, opus) |
| Implementing Specialist | `client` |
| Iteration | `1` |
| Security | `spawned` |
| Test | `spawned` |
| Observability | `spawned` |
| Code Quality | `spawned` |
| DRY | `spawned` |
| Operations | `spawned` |
| Semantic Guard | `spawned` |

---

## Task Overview

### Objective
One client captures its own microphone audio, encrypts/signs/sends it to MH over QUIC
datagrams, and receives/verifies/decrypts/plays back its own audio returned by MH.
Audio only, no video. Governing design: `docs/decisions/adr-0036-media-flow.md` §1/§4/§5/§6/§11.

### Scope
- **Service(s)**: `packages/sdk-core`, `packages/test-utils` (client domain)
- **Schema**: No
- **Cross-cutting**: Telemetry dimensions (observability), crypto invariants (security),
  self-edge forwarding (media-handler), KEK + send directive (meeting-controller)

### Debate Decision
NOT NEEDED — ADR-0036 is the governing design decision; this is implementation.

---

## Cross-Boundary Classification

`proto/**` and `crates/**` are **NOT touched** by this task. The reject-reason spellings are
*read* from `proto/test-vectors/frame-v2.vectors.json` (via the existing `rejectReason.ts`
mirror + its both-directions set-equality test); no vector row, no Rust constant, and no
`.proto` file is edited.

| File | Classification | Owner | Note |
|---|---|---|---|
| `packages/sdk-core/src/config/clientConfig.ts` (new) | Mine | — | SSoT for every configurable client value (Opus, rotation T, queue bounds, export cadence) |
| `packages/sdk-core/src/config/__tests__/clientConfig.test.ts` (new) | Mine | — | validation + bounds tests |
| `packages/sdk-core/src/media/pipeline/egress.ts` (new) | Mine | — | hot path |
| `packages/sdk-core/src/media/pipeline/ingress.ts` (new) | Mine | — | hot path |
| `packages/sdk-core/src/media/pipeline/egressQueue.ts` (new) | Mine | — | hot path; bounded drop-oldest |
| `packages/sdk-core/src/media/pipeline/playbackSink.ts` (new) | Mine | — | hot path |
| `packages/sdk-core/src/media/pipeline/hopSequenceMonitor.ts` (new) | Mine | — | hot path; downlink gap counter (TODO:1020) |
| `packages/sdk-core/src/media/pipeline/__tests__/**` (new) | Mine | — | |
| `packages/sdk-core/src/media/lifecycle/AudioPipeline.ts` (new) | Mine | — | sibling of hot path |
| `packages/sdk-core/src/media/lifecycle/muteState.ts` (new) | Mine | — | sibling |
| `packages/sdk-core/src/media/lifecycle/transmitKeys.ts` (new) | Mine | — | sibling; generation + rotation |
| `packages/sdk-core/src/media/lifecycle/__tests__/**` (new) | Mine | — | |
| `packages/sdk-core/src/media/setup/capture.ts` (new) | Mine | — | sibling; getUserMedia + device seam |
| `packages/sdk-core/src/media/setup/captureFailure.ts` (new) | Mine | — | sibling; the capture failure vocabulary and its classifier. Split out of `capture.ts` at review (@test F1): a pure six-way branch behind the coverage exclusion was untestable AND unscored, while the exclusion's justification claimed only wiring was excluded |
| `packages/sdk-core/src/media/setup/opus.ts` (new) | Mine | — | sibling; encoder/decoder config + support probe |
| `packages/sdk-core/src/media/setup/mediaMetrics.ts` (new) | Mine | — | sibling; allow-list label projection, cached handles |
| `packages/sdk-core/src/media/setup/measurement.ts` (new) | Mine | — | sibling; TTFF measurement seam |
| `packages/sdk-core/src/media/setup/kekSource.ts` (new) | Mine | — | sibling; KEK-source seam |
| `packages/sdk-core/src/media/setup/identity.ts` (new) | Mine | — | sibling; the meeting identity holder. Added DURING implementation after `dt-guard ts-no-retained-credentials` fired a TRUE POSITIVE on `MeetingSession` retaining an `IdentityKeyPair` (its `privateKey` field segments to `private`+`key`). Takes that guard's own second remedy — stop retaining the type — which is also the shape every other piece of media key material in this SDK already uses. Disclosed at the site, not routed around; the vocabulary-classification question is security's and observability's |
| `packages/sdk-core/src/media/setup/rosterKeys.ts` (new) | Mine | — | sibling; senderId -> CryptoKey, fail-closed |
| `packages/sdk-core/src/media/setup/audioPlayback.ts` (new) | Mine | — | sibling; AudioContext construction + suspended detection |
| `packages/sdk-core/src/media/setup/__tests__/**` (new) | Mine | — | |
| `packages/sdk-core/src/media/teardown/teardown.ts` (new) | Mine | — | sibling; acquisition-time registry, idempotent |
| `packages/sdk-core/src/media/teardown/__tests__/teardown.test.ts` (new) | Mine | — | |
| `packages/sdk-core/src/media/__tests__/hotPathLayout.test.ts` (new) | Mine | — | TS analogue of the Rust directory-scoped deny |
| `packages/sdk-core/src/media/MediaTransport.ts` | Mine | — | datagram send/receive; shallow-transport knob; grandfathered-label comment |
| `packages/sdk-core/src/media/events.ts` | Mine | — | media pipeline types |
| `packages/sdk-core/src/media/__tests__/media-transport.test.ts` | Mine | — | datagram tests; annotate the `meeting_id_hash` assertion |
| `packages/sdk-core/src/media/frame/frameCodec.ts` | Mine | — | `UnsignedFrame.relayRegionOffset` + `writeHopSequence`; no wire change |
| `packages/sdk-core/src/media/frame/receivePath.ts` | Mine | — | **ADDITIVE ONLY.** Three exported bound constants (`DEFAULT_REPLAY_CONTEXTS_PER_SENDER`, `DEFAULT_REPLAY_WINDOW_BITS`, `DEFAULT_TRANSMIT_KEYS_PER_SENDER`) replacing inline literals as the constructor defaults, so `config/clientConfig.ts` imports one definition (@dry-reviewer's third option; direction `config → frame`). **The `VerifiedFrame` brand, the verify→replay→unwrap→decrypt ordering, the wrap-cache write site, and `TransmitKeyCache.set`'s reachability only through `openVerifiedFrame` are ALL UNCHANGED** — no logic edited, no export removed, no signature widened. The two `32`s are kept as two separately-named constants per the story-task-10 false-SSoT precedent |
| `packages/sdk-core/src/media/frame/sframe.ts` | Mine | — | **ADDITIVE ONLY.** One new pure export, `sframeObjectLength(plaintextLength)` = `key_id + tag + plaintext`, so the egress path can size `payload_length` before sealing without a second derivation of that length. `unwrapTransmitKey` is untouched and still PURE — returns a key, writes nothing; still no `unwrapAndCache` and no cache-priming export |
| `packages/sdk-core/src/media/setup/seams.ts` (new) | Mine | — | sibling; injectable capture/codec/playback seam interfaces (types only) |
| `packages/sdk-core/src/media/frame/ed25519.ts` | Mine | — | `generateIdentityKeyPair` |
| `packages/sdk-core/src/media/frame/__tests__/**` | Mine | — | |
| `packages/sdk-core/src/signaling/SignalingClient.ts` | Mine | — | identity key on join; KEK intake; roster keys; directive/assignments/KEK events; capability + mute sends |
| `packages/sdk-core/src/signaling/events.ts` | Mine | — | |
| `packages/sdk-core/src/signaling/kekIntake.ts` (new) | Mine | — | TS-side KEK sink control (TODO:1907b) |
| `packages/sdk-core/src/signaling/__tests__/**` | Mine | — | |
| `packages/sdk-core/src/transport/IWebTransport.ts` | Mine | — | `datagrams` widened with the shallow-queue knobs |
| `packages/sdk-core/src/transport/BrowserWebTransport.ts` | Mine | — | `datagrams` getter returns the widened `WebTransportDatagrams`. The platform object already IS that shape (`WebTransportDatagramDuplexStream` declares all four members), so it is passed through rather than wrapped — a setter must reach the real transport, which is the point |
| `packages/sdk-core/vitest.config.ts` | Mine | — | three narrow, reasoned coverage exclusions for the browser-media construction seams (`capture.ts`, `opus.ts`, `audioPlayback.ts`), which cannot execute under `environment: 'node'` |
| `packages/sdk-core/src/telemetry/telemetryConfig.ts` | Mine | — | `exportIntervalMillis` from one named constant |
| `packages/sdk-core/src/session/MeetingSession.ts` | Mine | — | identity keypair, `startMedia()`, teardown, media-fault isolation |
| `packages/sdk-core/src/session/events.ts` | Mine | — | |
| `packages/sdk-core/src/session/__tests__/**` | Mine | — | |
| `packages/sdk-core/src/__tests__/serverMessageSinkScan.test.ts` (new) | Mine | — | source scan: no raw `ServerMessage` stringify/log sink |
| `packages/sdk-core/src/index.ts` | Mine | — | public exports |
| `packages/test-utils/src/MockWebTransport.ts` | Mine | — | datagram send/receive + injected drops + backpressure |
| `packages/test-utils/src/contracts/IWebTransport.ts` | Mine | — | Pattern-A structural parity with the sdk-core widening |
| `packages/test-utils/src/media/**` (new) | Mine | — | fake capture/codecs/playback + frame builders for tests |
| `packages/test-utils/src/index.ts` | Mine | — | barrel |
| `docs/observability/metrics/client.md` | **Not mine, Domain-judgment** | @observability | New §Media-path metrics + the O10 implicit-label carve-out + rescoping the stale "not yet emitted" banner. Requested in writing by @observability at Gate 1 (O8/O10) and explicitly refused as a deferral; I will draft, they rule on names/labels/wording |
| `docs/observability/label-taxonomy.md` | **Not mine, Domain-judgment** | @observability | **OWNER-IMPLEMENTED, not authored by this task.** @observability landed four hunks here directly during review — R3's stale "required end state" blockquote retired, a §The grandfathered set section, the `wrap_key_id_mismatch` bullet updated with the fired trigger and the `unwrap_failed_key_held` target spelling, and a §When vocabulary can be the control at all. Listed because it is in the diff and every file in the diff needs a row; NOT touched by me, and not to be duplicated |
| `docs/observability/dashboard-conventions.md` | **Not mine, Minor-judgment** | @observability | One row: client OTel export cadence, "no key exists"/open-item -> the named constant. Requested by @operations item 1(b) and @observability O5 |
| `docs/TODO.md` | **Not mine, Minor-judgment** | filers (@security, @observability, @operations, @protocol) | Status updates only, each citing what landed: close 894, 928, D2(439), D5(445), 1907(b); amend 286/913 with the measured VBR finding; record 901's trigger as fired-and-spun-out; amend 1020 with the landed hop-gap counter |
| `docs/user-stories/2026-08-27-hear-yourself-through-handler.md` | **Not mine, Minor-judgment** | @operations + @observability | **R-25 (line 53) only.** ONE additive clause: the emitted term is `accepted`, the identity holds at the crypto/parse boundary and not the playback boundary, pointer to the catalog entry. Not a rewrite; no existing prose or token removed. `validate-frame-vectors.sh` greps this file for `wrap_key_id_mismatch` verbatim (g16 `spec_anchor`, `grep -qw`), so the guard is run before ready. Scoped to line 53 — line 268's Story-3 sentence is NOT swept |
| `docs/specialist-knowledge/client/INDEX.md` | Mine | — | navigation entries for the new modules |
| `docs/devloop-outputs/2026-09-08-sdk-audio-media-pipeline/main.md` | Mine | — | devloop artifact |

**Not edited, deliberately**: `proto/**` (GSA — vectors are read, never rewritten),
`crates/**` (incl. `media-protocol`, `mh-service`), `docs/runbooks/**` and
`docs/observability/alerts.md` (task 21), `packages/web-app/**` and `packages/sdk-svelte/**`
(task 20). `docs/user-stories/**` is NO LONGER in this list — see the R-25 row above.

---

## Planning

### Gate 1 — Plan Confirmation Tracking

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Semantic Guard | confirmed |

### Lead + Owner Rulings at Gate 1

**C-2 (supersedes O2's rename half) — `wrap_key_id_mismatch` is NOT renamed in this devloop. Lead ruling: option (a).**

`observability` ruled at F3 that renaming the `WrapOutcome` variant to `unwrap_failed_key_held` was
client-only and touched no GSA, on the verified-but-incomplete premise that "four of five variants are
coined names appearing in no vector row". `security` withdrew their Gate 1 confirmation and produced
the missing binding; the Lead verified it at source before ruling:

- `proto/test-vectors/frame-v2.vectors.json:613` → `"outcome": "wrap_key_id_mismatch"` inside `expected`
  on the `wrap_for_different_key_id` row (and `:608` → `"reject_reason"`, a SECOND pin on the same value).
- Generated from `crates/media-vector-gen/src/inventory.rs:269-271`, whose comment states the field is
  "Declared in the SSoT rather than left to harness convention, mirroring `replay_of`, so a codec in a
  third language cannot get this wrong by omission." That field IS the cross-language contract.
- Nothing caught it because `packages/sdk-core/src/media/frame/__tests__/vectors.conformance.test.ts:407`
  hardcodes the literal while its neighbours at `:409`-`:412` read `row.expected?.*`, and
  `frameVectors.ts:93` already declares `readonly outcome?: string` — plumbing present, unused.

A TS-only rename would leave the implementation contradicting the SSoT's declared contract with the file
untouched: true of the filesystem, false of the contract, and a divergence is worse than an edit because
an edit is visible at review. Renaming properly is not a bigger version of this task — it IS the protocol
spin-out (`reject_reasons` union, `NON_DROPPING_REASON`, the Rust macro, the g16 `spec_anchor` requiring
the token verbatim in the story file, `expected.outcome`, every consumer mapping), needing `protocol` on
the panel under the §6.4 intersection rule.

Resolution:
- Variant and `outcome` label value stay `wrap_key_id_mismatch`, derived verbatim. One spelling, no mapping table.
- `unwrap_failed_key_held` is recorded in the `docs/TODO.md:901` amendment as the **proposed target spelling
  for protocol's rename**, so the spin-out inherits a name rather than coining a third.
- The TODO:901 scope list is amended to add `vectors[].expected.outcome`, which it did not previously name —
  the spin-out would have shipped and left that site behind.
- `operations`' runbook objection is served in-diff by the catalog leading with the **observable firing
  condition** (unwrap failed AND a usable key already held; realistic cause a KEK-generation skew), so task 21
  triages on the condition rather than the token name. The durable fix is scheduling the spin-out before task 21.
- **In scope, not deferrable:** `vectors.conformance.test.ts:407` reads `row.expected?.outcome`. Under 5 LoC,
  in-changeset, no design ambiguity. After it lands, a rename that skips the vectors fails the suite loudly.
- **Also in scope (Lead ruling on security's follow-on):** a one-line intra-file consistency assertion that
  `row.reject_reason` agrees with `row.expected.outcome` on that row. Same owner, same mechanism, same file
  already being edited — the framing-lock rule says fix the class, not the instance. Without it a partial
  rename touching one field and not the other still passes, leaving two spellings of one concept inside the SSoT.
- Bitrate comment cites **MC's config-load band validation** (`crates/mc-service/src/config.rs`, rejection
  tests pinning 31999/48001) as the load-bearing control, NOT `AudioEncoder.configure()` — that bounds platform
  capability, not policy sanity (WebCodecs configures Opus at 512 kbps without complaint). Code unchanged.

**R-25 — Lead ruling: option (a), additive pointer clause, corrected in place.**

`docs/user-stories/2026-08-27-hear-yourself-through-handler.md:53` gains ONE additive clause recording that
the emitted term is `accepted`, that the identity holds at the crypto/parse boundary and not at the playback
boundary, and pointing at the catalog entry. Not a rewrite of the requirement, and not a tracked TODO.

Ground: all fifteen `drops_frame: true` reasons fire at or before the crypto/parse boundary, so nothing on the
right-hand side absorbs a frame lost between decoder handoff and audible — R-25 as written asserts a
conservation law across a segment where nothing conserves it. The in-tree precedent is ADR-0036 §11's own false
sentence about the credential-leak guard, corrected in place on the recorded grounds that "a tracked TODO does
not make a false sentence true — and the false sentence does not merely misinform, it **redirects scrutiny**".
Four remaining story tasks read R-25 before they read any catalog.

Constraints (Lead-verified): `scripts/guards/simple/validate-frame-vectors.sh` greps the declared `spec_anchor`
file for each non-codec token verbatim (`grep -qw`), and `wrap_key_id_mismatch` declares this story file as its
anchor. An additive clause is inert to that, but the guard MUST be run before Gate 2 and no existing prose or
token may be removed. Scope is line 53 only; line 268's forward-looking Story-3 sentence repeats the phrasing
and is covered by the R-25 clause rather than swept. The file is added to the Cross-Boundary Classification
table as Not mine, Minor-judgment, owners `operations` + `observability`.

**O6 — `dt_client_mh_connection_total` keeps `meeting_id_hash`. Owner ruling: GRANDFATHERED, label stays.**

Issued in writing by `observability` (taxonomy owner) to close `security`'s conditional acceptance.
Grounds: ADR-0036 §11 R1's exception is enumerated and this metric is inside it by the exception's own
terms — an ADR-0028 join-flow metric documented under §Join-flow metrics (R-25) in
`docs/observability/metrics/client.md`, predating ADR-0036 and carrying the join set from birth.
Substantively, neither R1 argument reaches it: it fires **once per MH at handshake time, before any frame
exists**, cardinality bounded at 2 statuses × 3 buckets, and there is no time-ordered per-frame sequence,
so it cannot express the voice-activity trace R1 exists to prevent.

**The load-bearing line, which generalises past this metric: the test is what a metric OBSERVES, not which
directory it lives in.** Connect-lifecycle (once per connection, at setup, before media) is grandfathered;
media-carrying (per-frame, per-stream, or on the media data path) is under the bar. A directory-shaped rule
fails in *both* directions — it would wrongly bar this metric, and it would wrongly permit a media counter
placed outside the media tree. The second failure is the one that will actually happen.

Five conditions, because this metric now sits in the same module as the new media emissions and inertia is
exactly what R3 names:
1. The grandfathered set is **frozen with a named member list** in `client.md` — today it is a category with
   no roster, which is how a closed exception opens by analogy.
2. A comment at `MediaTransport.#emitMetric` stating the `...this.#metricLabels` spread is legal for this one
   grandfathered metric only, that no new emission in this file may copy it, and that media emissions use
   `mediaMetricLabels`.
3. `media-transport.test.ts:270` keeps its `meeting_id_hash` assertion (it correctly pins grandfathered
   behaviour) but gains a comment naming it as such — that test and the new negative test assert opposite
   things and both are right; unlabelled they read as a contradiction someone resolves in whichever direction
   they meet first.
4. The exception does not extend to any `dt_client_media_*` metric or to `dt_client_time_to_first_media_frame_ms`.
5. **`docs/TODO.md` D5 amended, NOT closed.** The ruling creates a standing obligation (keep the roster frozen;
   classify every future client metric against the observes-what line) that no guard enforces — the enforcement
   gap is D4, which stays open. D5 is that obligation's home; closing it would retire a live rule into a
   completed-work note.

**R-25 clause text is owner-authored** by `observability` and ships verbatim as supplied. Line 268's Story-3
forward-looking sentence is deliberately not swept; its `played` correction folds into the same
`docs/TODO.md` §Media Path Obligations entry O3 already requires for the `frames_received_total`
counting-point move — same sentence, same future reader, one entry.

**Gate 1 process note.** `observability` self-reported the F3 error against their own prior ruling, and recorded
the specific reviewer failure mode: they cited `vectors.conformance.test.ts:407`'s comment ("CACHE STATE is the
assertion, not a reject reason") as proof the assertion was deliberately decoupled from the row, when it was
proof of a GAP. Reading a defect as a design decision is the same "control alive but out of scope, which reads
as coverage" pattern named in §Lessons Learned. The implementer self-reported their own "touches no GSA" claim
before being asked. Both are recorded because the gate worked by people contradicting themselves in public.


Recorded here so they cannot be re-litigated at Gate 3.

**L1 — `wrap_key_id_mismatch` (`docs/TODO.md` §Media Path Obligations, TODO:901): spin-out ACCEPTED by the Lead.**
The closure trigger ("must precede story task 19") fires with this diff, because this diff is what
puts the reason vocabulary in front of oncall. The remedy spans a `proto/**` GSA rename + a
`crates/media-vector-gen` regenerate + a user-story text edit — cross-owner (`protocol`) and
task-sized, so fix-now-cost exceeds fix-later-cost + tracking. **Condition:** the TODO entry is
amended in THIS diff to record the trigger as FIRED with a named spin-out target, and a one-line
pointer is added under §Accepted Deferrals. `observability` and `operations` both hold this at
Gate 3 and escalate if absent.

**L2 — VBR-floor finding (TODO:286/913): endorsed by the Lead as a genuine cross-owner finding, not scope creep.**
Under `bitrateMode: 'variable'` with a 32 kbps ceiling, 32 kbps is a target and not a floor, so MH's
`NOMINAL_AUDIO_BITRATE_BPS` sizes an average rather than a worst case and MH's operator-facing
datagram-buffer budget under-states queued latency on quiet audio. Amend the TODO entries with the
measured fact and hand to `media-handler` + `operations`. Switching to CBR would contradict the task
text; silently closing the TODO would be masking. Neither is authorised.

**L3 — export-interval blast radius must be stated at the constant's definition site**, not only in
this file: one global `MeterProvider` (R-19/R-24) means 10 s applies to every SDK metric including
the ADR-0028 grandfathered join metrics — a ~6x export-volume change, checked by `operations` against
GC's `TELEMETRY_PROXY_RATE_LIMIT_PER_MINUTE = 60` per-`sub`.

**O-OWN — `docs/observability/metrics/client.md` (Domain-judgment, owner `observability`): option (a), paired in substance.**
Owner ruled that spinning it out would ship a catalog whose §Naming convention sentence becomes false
the moment this code lands — the sentence the next media-metric author reads before violating R1 —
and that splitting catalog authorship from the code guarantees drift on the one artifact whose value
is being non-drifted. Implementer drafts the §Media-path metrics catalog section; **the owner authored
the §Naming convention carve-out prose verbatim** and it ships as supplied. Owner's Gate 3 Ownership
Lens verdict is the owner confirmation of record.

**O1 — `dt_client_media_frames_accepted_total`** (counter approved, name `played` REJECTED). The
emission boundary is "successfully opened and handed to the `AudioDecoder`", which makes
`received = accepted + sum(drops)` exact by construction — but a frame handed to a decoder is not
played, and a counter asserting a condition its emission site cannot substantiate is the same defect
this devloop is spinning `wrap_key_id_mismatch` out for. On a "hear yourself" story the misdiagnosis
is concrete: silent audio with `played` climbing reads as an output-device fault while the decoder is
erroring. **`dt_client_media_decoder_errors_total`** added to cover the handoff→audible segment the
rename leaves explicitly unmeasured.

**O2 — `dt_client_media_key_wrap_outcomes_total{outcome}`** with the two anomalous values only
(`kek_generation_not_held`, `wrap_key_id_mismatch`); `absent`/`already_held` are NOT counted (per-frame
counter work on a "nothing to do" bucket, barred by §11's per-frame budget, and it would bury the
rotation-lag signal). Both non-dropping outcomes stay off `frames_dropped_total`, preserving the identity.

**O3 — separate `_sent_` / `_received_` names, no `direction` label. Outcome confirmed; the
implementer's stated REASONING is corrected on the record.** "R-36's `direction` clause is written
against MH's counters" is false — R-36 names "client export at 10 s" in the same sentence and lists
`client` among its owners; left standing it becomes a precedent for ignoring R-36 on the client
generally. Correct ground: the clause bars a *degenerate constant* `direction` label on a
single-direction counter; it does not mandate collapsing two counters into one. MH is the precedent
for the narrow reading (`MediaDropReason::direction()` derives direction from the reason token at its
definition site).

**O4 — send-drop vocabulary is FIVE tokens, not three**: `egress_queue_overflow`,
`transport_send_refused`, `oversize_datagram`, `connection_closed`, `not_connected`.
`transport_send_failed` is a near-miss of MH's actual `transport_send_refused` — matching one token
for cross-end `sum by(reason)` comparability then silently diverging on the one beside it is worse
than not matching at all. `connection_closed` splits from `not_connected` because MH made that exact
split on the stated grounds that folding them yields "an unalertable mixture of 'we have a bug' and
'someone hung up'". `oversize_datagram` matters because the key-bearing flag is set on every audio
frame and the bitrate ceiling is config-driven — one ConfigMap edit turns this into 100% send loss
reported as a transport fault on a counter contracted to read zero forever.

**O5 — hop-gap counters approved with conditions**, the load-bearing one being that `gap_frames`
over-counts true loss by exactly the reorder count: the catalog must state `gap_frames - reorder`, or
the first congestion incident reads normal QUIC reordering as loss.

### Implementer Plan

#### Mechanism-vs-instance restatement

The task's nouns are audio-specific (Opus, microphone, datagram). Restated as mechanism:

> **A client-owned media pipeline in which every frame's fate is accounted for exactly once, and
> in which the boundary between per-frame code and lifecycle code is a directory boundary rather
> than a convention.**

Two places where that mechanism is wider than the task's nouns, surfaced rather than absorbed:

1. **Conservation accounting (`received = played + sum(drops)`) is a discipline this SDK applies
   nowhere else.** The signaling receive loop decodes `ServerMessage`s and drops unhandled variants
   with a `logger.debug` and *no counter* (`SignalingClient.#dispatch` default arm), and the framing
   layer drops on `FramingError` without a by-reason counter. Same owner (client), same shape, not
   in this task's scope. Flagged, not fixed here.
2. **`meeting_id_hash` on media metrics is an instance of "one ambient label set threaded into
   every emission site".** The general fix is per-emission-domain allow-list projection, which is
   what I am building for media; the join-flow set stays as ADR-0028 grandfathered it. If a third
   emission domain appears, it inherits the join set by the same inertia unless the projection
   pattern is the documented default. I document the pattern at the new helper; making it the
   SDK-wide convention is wider than this task.
3. **The bounded drop-oldest queue with caller-owned counting already exists in Rust**
   (`crates/mh-service/src/media/queue.rs::BoundedDropOldest`). Mine is a cross-language sibling of
   a named mechanism, not a new idea; I mirror its shape (ring returns the evicted item, the caller
   counts) and record the resemblance at the definition, per @dry-reviewer.

#### 1. Code layout (ADR-0036 §11 layout constraint; @observability O7)

The hot path is a directory, and lifecycle/setup/teardown are its **siblings**, so the directory
boundary and the hot-path boundary are the same boundary:

```
packages/sdk-core/src/media/
  frame/       pure per-frame codec + crypto (task 15; unchanged except two additive helpers)
  pipeline/    HOT PATH - per-frame code only. No logging, no metric-name literals, no
               `dt_client_` string anywhere in the directory. Metric handles arrive pre-bound.
  lifecycle/   start/stop/mute/rotate/directive handling            (sibling)
  setup/       capture, codec config, metric handles, seams, keys   (sibling)
  teardown/    idempotent disposal                                  (sibling)
  MediaTransport.ts, events.ts
```

Enforced by `media/__tests__/hotPathLayout.test.ts`: scans `media/pipeline/**` and fails on
`console.`, `logger.`, any `dt_client_` literal, or any import of `telemetry/logger.js`; and fails
if the directory is absent or contains no `.ts` files (mirroring the Rust guard's self-failing
scope, task 17). Cached-handle method calls are allowed — that is the pattern the control exists to
enforce, so it must not ban it.

#### 2. Configuration — one home (`packages/sdk-core/src/config/clientConfig.ts`)

Every value below is read from it; none is a literal at its use site.

| Key | Default | Why |
|---|---|---|
| `media.audio.sampleRateHz` | 48000 | task text |
| `media.audio.channels` | 1 | mono voice |
| `media.audio.frameDurationMs` | 20 | task text; reciprocal of MC's `MC_AUDIO_FRAME_RATE_HZ=50` and MH's `AUDIO_FRAME_DURATION_MS=20` |
| `media.audio.defaultBitrateBps` | **32000** | Named `DEFAULT_`, **not** `MAX_`. The send directive's `EncodingParameters.max_bitrate_bps` is the wire SSoT and configures the encoder whenever present; this is the fallback for its absence only (@dry-reviewer). **No local band, no clamp** — a mirrored `[32000,48000]` validator would hard-fail every SDK in the field the first time an operator raises `MC_AUDIO_MAX_BITRATE_BPS`, which is an env var MC already validates once. `AudioEncoder.configure()` bounds **platform capability, not policy sanity** (WebCodecs will configure Opus at 512 kbps); the load-bearing control is MC's config-load band check, rejection pinned at `crates/mc-service/src/config.rs:858`, which MC refuses to *start* without satisfying (@security, @operations). 32000 keeps ADR §3's arithmetic true and carries an `ANCHOR (DRY):` to MH's `SUPPORTED_AUDIO_BITRATE_FLOOR_BPS` |
| `media.audio.opusComplexity` | 10 | "highest complexity" |
| `media.audio.opusApplication` | `voip` | "voice/VoIP application mode" |
| `media.audio.opusSignal` | `voice` | "no music tuning" |
| `media.audio.opusUseDtx` | false | §5 mutes signal out of band; DTX makes absence a signal and worsens §11's metadata leak |
| `media.audio.opusUseInbandFec` / `packetLossPerc` | false / 0 | FEC is story 8 tuning |
| `media.keys.audioRotationPeriodMs` (T) | 60000 | §4 "every T for audio"; measured from pipeline start (per-client, naturally desynchronised — **not** wall-clock aligned, which would fleet-synchronise rotation) |
| `media.egress.maxQueueFrames` | 10 | = **200 ms** at 20 ms/frame. Expressed in FRAMES per ADR §1 (@operations item 3) |
| `media.egress.transportOutgoingHighWaterMarkFrames` | 2 | = 40 ms. App bound (10) must exceed it; asserted at setup so the pair cannot silently invert |
| `media.egress.transportOutgoingMaxAgeMs` | **500** | age bound. @operations F1: 100 ms would have made UA age-discard — invisible to us AND to MH — a *routine* competitor to the app queue rather than a backstop. Must exceed the latency the app queue plus transport queue can legitimately hold (200 + 40 ms); asserted at setup |
| `media.ingress.maxTrackedSendersForKeys` | 32 | bounded roster `CryptoKey` cache (@operations item 11) |
| `media.replay.maxContextsPerSender` / `windowBits` | 32 / 64 | passed into the existing `ReplayWindow` rather than re-defaulted inside it |
| `telemetry.metricExportIntervalMs` | **10000** | @observability O5 / @operations 1(b) / TODO D2 |

The export interval is one exported named constant consumed by the existing
`PeriodicExportingMetricReader`, overridable through `TelemetryConfig`. **No second
`MeterProvider`.** Stated deliberately (@operations 1a): one global provider (R-19/R-24) means 10 s
applies to *every* SDK metric including the grandfathered ADR-0028 join metrics — a ~6x
export-volume change, which @operations has verified against the GC proxy's 60/min per-`sub` limit.

#### 3. Egress — §4 ordering, exactly

Per encoded Opus frame, in this order and no other:

1. **Client mute is enforced at capture**, before the `AudioData` reaches the encoder. Muted ⇒
   nothing is encoded, so nothing can leave; at most the in-flight frame completes. Unmute resumes
   locally with no round trip. MC's send directive stays active throughout.
2. `TransmitKeyManager.currentFor(streamNumber)` → `{ keyId, key, wrappedBlock }`. Generation is
   **session-scoped, monotonic, never reset**. The wrap is computed **once per generation**, not
   per frame — byte-identical within a generation, which is the property the receiver's
   `matchesCachedWrap` skip depends on.
3. Build the publisher region **fully populated before either range is sliced** — the existing
   `buildUnsignedFrame` contract; no region is recomputed or re-serialized anywhere.
4. `sealSframe({ key: derived.key, nonce: sframeNonce(salt, streamSequence), aad: publisherRegion,
   plaintext, keyId })`.
5. `signFrame(identityPrivateKey, unsigned.signedRange)` → `finishFrame`. Signing is last, over the
   final bytes. `keyBearing` and the presence of the wrap cannot disagree — `buildUnsignedFrame`
   already throws if they do.
6. Enqueue on the bounded egress queue. At **dequeue**, `writeHopSequence` patches the 4-byte relay
   field (unsigned, excluded from AAD and signature) so hop sequence counts **only frames actually
   sent** (R-8) — a dropped frame leaves no gap.
7. `MediaDatagramChannel.send()` awaits `writer.ready` before writing, so at most one write is in
   flight in the transport and the depth lives in *our* queue.

**AAD ordering, resolved without a second serializer.** The seal needs the AAD; the AAD is the
publisher region; the region needs `payload_length`. GCM is length-preserving, so the sealed length
is known before sealing (`KEY_ID(8) + TAG(16) + plaintext`). `frameCodec.ts` gains one additive
export that emits the publisher region for a known payload length, and `buildUnsignedFrame` is
refactored to call it — **one code path, one writer of header bytes**, no duplicate wire logic.
`UnsignedFrame` also gains `relayRegionOffset` so the hop-sequence patch has a computed offset
rather than a re-derived constant.

**Egress queue** (`pipeline/egressQueue.ts`): ring of `maxQueueFrames`; `push` returns the evicted
item (caller counts; the queue holds no metric handle); drop-**oldest**, never drop-newest.

#### 4. Ingress — order fixed by the `VerifiedFrame` brand, not by convention

`datagram → [count at the wire] → decodeFrame → parseSframe/unpackKeyId → roster lookup →
verifyFrame → openVerifiedFrame(replay → unwrap → decrypt) → AudioDecoder → playback`.

- `dt_client_media_frames_received_total` increments **at the wire**, first statement in the loop,
  before any parse. Recorded durably in the catalog with the video-story move-to-parse-boundary
  note (@observability O3) plus a TODO §Media Path Obligations entry naming the trigger.
- Reject accounting is `counter({ reason: err.rejectReason })` **verbatim**. No mapping table, no
  bucket, no `other`. The label type is `RejectReason` from `rejectReason.ts`; nothing retyped.
- `wrap_key_id_mismatch` and `kek_generation_not_held` arrive on the **success** channel as
  `WrapOutcome` and are counted on a separate counter; **neither reaches the drop counter**.
- Roster fail-closed: an entry with a **zero-length** `identity_public_key` ⇒ `no_roster_entry`
  drop. Empty is never read as "skip verification".
- `assertEd25519Available()` called once at receive-loop init (TODO:894a).
- Downlink hop-sequence gap monitor (TODO:1020): frames skipped between MH's egress and our ingress,
  measured against a running high-water mark, with late/reorder counted separately so the two are
  not conflated. No per-stream label; internal state keyed by relay `stream_id`, bounded.
- No pre-parse byte cap is added: `decodeFrame` already bounds `payload_length` against
  `MAX_PAYLOAD_BYTES` **before slicing** and is zero-copy, and a pre-parse cap would need a reason
  token outside the frozen sixteen. The ingress queue bound is the UA's incoming datagram queue plus
  a serial await-per-frame reader; `incomingHighWaterMark` is set from config.

#### 5. Metrics — `client_version`, `org_id`, `key_custody=operator`, and nothing else

`setup/mediaMetrics.ts` builds the label set by **allow-list projection** (@observability O1):
`mediaMetricLabels({ clientVersion, orgId })` constructs `{client_version, org_id, key_custody}`
from named string inputs. It is structurally impossible to pass a label bag in — the constructor
takes strings, not a `MetricLabels`. Nothing is deleted from a spread, because nothing is spread.

| Metric | Type | Labels beyond base |
|---|---|---|
| `dt_client_media_frames_sent_total` | counter | — |
| `dt_client_media_send_dropped_total` | counter | `reason` |
| `dt_client_media_send_queue_depth` | gauge | — |
| `dt_client_media_frames_received_total` | counter | — (at the wire) |
| `dt_client_media_frames_dropped_total` | counter | `reason` (the 15 `drops_frame:true` tokens) |
| `dt_client_media_frames_accepted_total` | counter | — (renamed from `played` by @observability Ruling 1) |
| `dt_client_media_decoder_errors_total` | counter | — (@observability Ruling 1) |
| `dt_client_media_key_wrap_outcomes_total` | counter | `outcome` = `kek_generation_not_held` / `wrap_key_id_mismatch` (SSoT spelling; rename pending TODO:901 — Gate 1 ruling 5) |
| `dt_client_media_downlink_gap_frames_total` | counter | — (TODO:1020) |
| `dt_client_media_downlink_reorder_total` | counter | — |
| `dt_client_media_mute_transitions_total` | counter | `action` = `mute` / `unmute` |
| `dt_client_media_kek_updates_total` | counter | `source` = `join_response` |
| `dt_client_time_to_first_media_frame_ms` | histogram | — |

Three rulings requested from @observability:

- **`dt_client_media_frames_played_total` is an addition** beyond the story's line-128 list. Without
  it, `received = played + sum(drops)` names a term that is never emitted and the identity is
  unverifiable in production. I would rather add the counter than ship an unverifiable invariant.
- **Send-side `reason` vocabulary** is a *new* bounded vocabulary, not the reject taxonomy.
  Proposed: `egress_queue_overflow` (deliberately MH's `MediaDropReason` spelling so `sum by(reason)`
  compares across the two ends of one hop, per @dry-reviewer), `transport_send_failed`,
  `not_connected`. One home: `setup/mediaMetrics.ts`.
- **Direction**: separate `_sent_` / `_received_` names, neither carrying a `direction` label. **The
  ground is NOT "R-36 is written against MH" — that is false** (R-36 names client export at 10 s in
  the same sentence and lists `client` as an owner) and must not appear in any comment or doc. The
  correct ground, per @observability's Ruling 3: the clause bars a **degenerate constant** label —
  do not stamp `direction="receive"` on a counter that can only ever be receive — and it does not
  mandate collapsing two counters. MH is the narrow-reading precedent: `MediaDropReason::direction()`
  derives direction from the reason token at its definition site, so direction there is a derived
  attribute of `reason`, not a free dimension. Two drop vocabularies on two metrics leave no shared
  counter for `direction` to disambiguate.

TTFF is a **separate measurement seam** (`setup/measurement.ts`), explicitly *not*
`MeetingSession.#histogram`: different epoch (media start, not join start) and that helper's label
set is the one §11 forbids. The comment at the seam says exactly that (@dry-reviewer addendum).

#### 6. Signaling additions (all client-side)

- `JoinRequest.identity_public_key`: the SDK generates an Ed25519 identity keypair before join
  (`generateIdentityKeyPair()` added to `ed25519.ts`) and sends the raw 32-byte public half.
- **KEK-source seam** (`setup/kekSource.ts`): `MeetingKekSource { kekForGeneration(gen) }`, fed from
  `JoinResponse.meeting_kek` / `MeetingKekUpdate`. **The KEK never rides on `JoinedEvent`, any
  public event, any error, or any e2e-bus-projectable surface.** Nothing in the frame format or the
  wrap references the seam. It is cleared at teardown.
- **KEK intake / TS-side sink control** (`signaling/kekIntake.ts`, closes TODO:1907b): immediately
  after `fromBinary`, a key-bearing `ServerMessage` has its KEK copied into the seam, the decoded
  field's bytes **zeroed**, and the field replaced with an empty array — so a later
  `JSON.stringify(serverMessage)` prints nothing. Backed by
  `src/__tests__/serverMessageSinkScan.test.ts`, a source scan asserting no raw
  `ServerMessage`/`JoinResponse`/`MeetingKekUpdate` value reaches `JSON.stringify`, `console.*`,
  `logger.*`, or a template interpolation in `packages/sdk-core/src`.
- **Roster identity keys** (`setup/rosterKeys.ts`): internal, never a public event payload. Maps
  `senderId → CryptoKey`, populated from `JoinResponse.existing_participants` + `ParticipantJoined`,
  evicted on `ParticipantLeft`, bounded, `importKey` **once per sender**. Self's entry is populated
  from the client's own `JoinResponse.sender_id` and its own public key because MC does not put the
  joiner on its own roster — roster *population*, not a verification bypass; the identical verify
  path runs.
- New `ServerMessage` handling: `send_directive`, `stream_assignments`, `meeting_kek_update`.
- New client sends: `ReceiveCapability` (one audio slot, `slot_id = 0`) and `MuteRequest`. Per
  TODO:909 the capability is sent **before** a directive is expected; the directive is a consequence
  of the declaration, not of join.

#### 7. Lifecycle, teardown, blast radius

- `MeetingSession.startMedia()` is **explicit**, not automatic on join (@operations item 5). A media
  fault never fails the join or tears down signaling; it surfaces on a `mediaError` event.
- `teardown/teardown.ts` is an acquisition-time registry: every resource registers **at acquisition,
  before the next await** (the `MediaTransport.#connectOne` pattern), so a throw from
  `AudioEncoder.configure` still stops the microphone track. Idempotent and safe during setup.
  Disposes: `getUserMedia` tracks, `AudioEncoder`, `AudioDecoder`, the datagram reader, the
  `AudioContext`/playback node, the rotation timer; and calls `TransmitKeyCache.clear()` +
  `ReplayWindow.clear()` (TODO:894b/c).
- Degradation matrix, each loud/typed/counted-or-evented, never a silent catch: mic permission
  denied (`NotAllowedError`); no device; `track.onended` / `devicechange`; `AudioEncoder` error
  callback; `AudioDecoder` error callback; transport `closed` mid-stream; datagram readable ending;
  **`AudioContext` not `running`** (autoplay-suspended = every signal green and no sound, so it is
  detected at setup and surfaced loudly). Codec error callbacks are **event-once / bounded**, never
  per-frame, because §11 bars per-frame logging.

#### 8. Tests

Unit tier runs `environment: 'node'`, so every browser media API sits behind an injected seam and no
test touches a real `AudioEncoder`, `getUserMedia`, or `AudioContext`. New doubles land in
`packages/test-utils/src/media/` so tasks 20 and 23 reuse them.

- **Attribution (prompt-mandated)** — `pipeline/__tests__/ingress.attribution.test.ts`:
  (a) frame with `key_id.sender_id = A`, signed by A, delivered on slot 0 whose `StreamAssignment`
  names B ⇒ the played frame is attributed to **A** and the assignment is never consulted;
  (b) the same frame body with `key_id.sender_id` still A but **signed by B's key** ⇒ rejected with
  `rejectReason === 'signature_invalid'`, played count unchanged, never attributed to B.
- **Empty-key fail-closed (TODO:928, load-bearing)** — `setup/__tests__/rosterKeys.failClosed.test.ts`
  plus an ingress case: roster entry present with a **zero-length** `identity_public_key` ⇒ frame
  **dropped and counted** as `no_roster_entry`, asserted dropped-and-counted, never skipped, never
  played.
- **Label set** — for every new metric, emitted label KEYS set-equal `{client_version, org_id,
  key_custody}`; a negative test asserting `meeting_id_hash` never appears on any `dt_client_media_*`
  series.
- **Reason individuality** — all eight structural codec tokens driven through ingress and asserted
  as distinct `reason` values, `unknown_version` named explicitly (R-31).
- **Conservation** — a mixed run asserting `received === played + sum(drops)`.
- **Mute** — outbound datagram count flat across a mute window, resumes on unmute with no signaling
  round trip; transitions counted.
- **Egress queue** — writes paused on the mock ⇒ oldest dropped, `egress_queue_overflow` counted,
  gauge reflects depth; app bound < transport high-water mark asserted at setup.
- **Teardown** — partially-constructed teardown (throw after `getUserMedia`), idempotency, both
  `clear()`s called.
- **No wall-clock latency assertion anywhere** (§10 / @observability O6).

`MockWebTransport` gains: settable `datagrams.outgoingHighWaterMark` / `outgoingMaxAge`,
pause/resume of the datagram writable (backpressure), injected outbound drops recorded separately
from `getOutboundDatagrams()`, injected write failures, and an inbound `simulateDatagramLoss()` that
records a loss without delivering. `contracts/IWebTransport.ts` is widened in lockstep (Pattern A).
**Correction (@dry-reviewer):** my claim that "the compile break is the forcing function working as
designed" was WRONG — TypeScript assignability ignores *missing optional properties*, so optional
attributes would have left the guard at `BrowserWebTransport.test.ts:24-25` silently blind to exactly
the members being added, while its comment kept asserting a guarantee it no longer provided. The three
attributes are therefore declared **required-but-possibly-undefined** (`outgoingHighWaterMark: number |
undefined`, mutable), which restores the assignability break with one guard rather than two, and the
comment at `:21-23` is corrected to describe the guarantee the guard actually gives.

#### 9. Findings and deliberate non-actions

- **VBR floor vs MH's nominal frame size (TODO:286/913) — a real residual, reported not masked.**
  The task requires *variable* bitrate with a configured **ceiling**. Under
  `bitrateMode: 'variable'` with `bitrate: 32000`, 32 kbps is a target/ceiling and **not a floor**:
  quiet input yields frames below MH's nominal size, so `NOMINAL_AUDIO_BITRATE_BPS = 32000` sizes an
  average case rather than a worst case and MH's operator-facing "N frames = M ms" datagram-buffer
  budget under-states queued latency on quiet audio. I will **not** silently close TODO:286/913; I
  amend them with this measured fact and hand it to @media-handler + @operations. Switching to CBR
  would fix it and contradict the task text, so I am not doing that unilaterally.
- **`wrap_key_id_mismatch` (TODO:901)** — not renamed here: that spans the `proto/**` GSA,
  `crates/media-vector-gen`, the g16 `spec_anchor` and the story text, and carries a three-way design
  choice owned by @protocol. Handling: (i) the token never reaches the drop counter; (ii) it *does*
  appear as an `outcome` value on `dt_client_media_key_wrap_outcomes_total`, so the closure trigger
  **has fired** — recorded in the entry rather than left reading as pending-and-harmless, with a
  protocol-owned spin-out recommended; (iii) no new code hardcodes the string outside the existing
  `rejectReason.ts` / `receivePath.ts` homes.
- **`dt_client_mh_connection_total` keeps `meeting_id_hash`.** It is an ADR-0028 join-flow metric
  (one of R-25's five), which §11 grandfathers **as a set** ("are grandfathered as a set and are not
  extended"). Changing it is an ADR-0028 metric change with catalog and dashboard consequences and
  is outside this task. I will annotate both the emission site and the `media-transport.test.ts:270`
  assertion so neither can be read as the media convention. @security: if you want the metric itself
  changed, say so and I will raise it rather than change it silently.
- Not touched: runbooks / alerts / dashboards (tasks 21, 22), web-app and sdk-svelte (task 20),
  `proto/**`, `crates/**`, `docs/user-stories/**`.
- **Rollback**: code-only, redeploy-only. No schema, no data, no wire-format change — the v2 frame
  landed at task 15. Version skew is detected by `unknown_version` staying individually visible.


---

#### Gate 1 rulings — these SUPERSEDE the plan text above where they conflict

Recorded after review. Every item below was a reviewer ruling or a correction to something I had
wrong; none is a preference I chose.

**@observability (binding).**
1. `dt_client_media_frames_played_total` → **`dt_client_media_frames_accepted_total`**. Emission
   boundary unchanged (opened and handed to the `AudioDecoder`) — that boundary is what makes the
   identity exact. A frame handed to a decoder is not played: the decoder can error, output can be
   discarded, the context can be suspended. Shipping `played` would have cleared, in this diff, the
   same defect the diff quarantines `wrap_key_id_mismatch` for. Plus
   **`dt_client_media_decoder_errors_total`** (base labels only) so the segment the rename leaves
   explicitly unmeasured is not the one place a total failure moves no counter.
2. Wrap-outcome counter carries **only** `kek_generation_not_held` and `unwrap_failed_key_held`.
   `absent` / `already_held` / `cached` stay uncounted.
3. Ruling 3 outcome stands; **my reasoning was false and must not appear anywhere in the tree** —
   see the corrected ground in §5.
4. Send-drop vocabulary is **five** tokens, not three: `egress_queue_overflow`,
   `transport_send_refused` (MH's real spelling — matching one token for cross-end comparability
   then diverging on its neighbour is worse than not matching), `oversize_datagram`,
   `connection_closed`, `not_connected`. Four mirror MH's `MediaDropReason`; `not_connected` is
   client-only, and the catalog says which are which. **Mute is not a send drop** — nothing is
   encoded while muted, so nothing enters the queue, nothing is dropped, and nothing counts against
   `frames_sent_total`.
5. **`WrapOutcome` rename — REOPENED AT GATE 1, RESOLVED AS (a): NO RENAME IN THIS DEVLOOP.**
   The original ruling rested on an incomplete premise ("`WrapOutcome` was never vectors-pinned").
   The fifth variant **is** pinned, twice, in the GSA file: `frame-v2.vectors.json:608`
   (`reject_reason`) and `:613` (`expected.outcome`) on the `wrap_for_different_key_id` row,
   generated from `crates/media-vector-gen/src/inventory.rs:268-270`, whose comment states the field
   exists *"so a codec in a third language cannot get this wrong by omission."* Renaming the
   TypeScript while leaving the vectors unedited would make the implementation **contradict** the
   GSA's declared contract with the file untouched — worse than an edit, because an edit is visible
   at review and a divergence is not.

   **@observability ruled (a) on the merits**: (b) collapses under its own arithmetic — renaming
   `expected.outcome` while `reject_reason` on the same row keeps the old spelling produces a row
   carrying two names for one condition *inside the SSoT*, strictly worse than either endpoint; and
   renaming both is not a bigger (b), it *is* the protocol spin-out. Their own principle ("rename at
   the source, not just the label") survives and had been applied to the wrong node: the source is
   the vectors file, not the TypeScript union, so it says rename at the true source or not at all.

   **Net: `WrapOutcome`'s variant and the `outcome` label value both keep `wrap_key_id_mismatch`,
   matching the SSoT. No `proto/**` edit, no `crates/**` edit, classification table unchanged.**

   **Two guard gaps closed in `vectors.conformance.test.ts` (both Mine, both sub-5-LoC, landing
   regardless of the ruling).** The same value is pinned twice in the GSA file and **neither binding
   was read**: `:407` hardcoded `'wrap_key_id_mismatch'` while `:409`–`:412` read every neighbouring
   `expected` field from the row and `frameVectors.ts:93` already declared `readonly outcome?: string`;
   and `row.reject_reason` is read only at `:131` (guarded on `kind === 'decode_reject'`), `:437`
   (signature rows) and `:452` (a hardcoded `decrypt_failed`), none of which match this row's
   `kind: 'wrap_binding'`. Fixes:
   - `expect(opened.wrapOutcome).toBe(row.expected?.outcome)` — the SSoT now mechanically decides the
     spelling, and a rename that does not move the vectors fails the suite.
   - `expect(row.reject_reason).toBe(row.expected?.outcome)` — a **file-internal consistency check**
     pinning that the SSoT does not contradict itself, which is the one failure mode a cross-language
     vectors file cannot afford and which no language's harness checks. The realistic error it
     catches: a partial rename by the spin-out author, who is not in this conversation and is moving
     five sites.

   **Contribution to the spin-out's scope**: TODO:901's scope list names "consumer error→`reject_reason`
   mappings" and does **not** name `vectors[].expected.outcome` as a distinct field the rename must
   move. The amendment records two new facts — `unwrap_failed_key_held` as the proposed target
   spelling, and `expected.outcome` as a second in-file site.

   **Revised @observability conditions**: 1, 2, 3 and 5 lapse (no rename). 7 and 8 stand as written.
   **Condition 4 inverts** — the catalog states the `outcome` value **is** the SSoT spelling carried
   from `vectors[].expected.outcome`, with the rename pending under TODO:901, and **leads with the
   observable firing condition** (the unwrap failed — a one-bit GCM tag mismatch, all the receiver
   gets — *and* a usable transmit key for that key id was already held, so the frame plays; realistic
   production cause is a **KEK-generation skew**, not anyone mis-binding wraps), so a runbook triages
   on the condition regardless of the token's name. That is @operations' substance served without a
   rename. **Condition 6** gains the `expected.outcome` scope item above.

6. Gap counters: `dt_client_media_downlink_gap_frames_total` increments by the **size** of the gap
   (a counter of missing numbers, not of gap events); the catalog states `gap_frames - reorder` as
   the honest loss estimate, because a reordered frame opens a gap and then arrives late so
   `gap_frames` over-counts true loss by exactly the reorder count.
7. Both catalog entries carry the cross-reference **in both directions**, using @operations' wording
   verbatim: *same AEAD failure, forked by receiver state; the drop counter carries the half that
   lost the frame, the wrap-outcome counter carries the half that kept it* — plus the statement that
   the two cannot be summed or ratio'd in one expression **and why**.
8. The identity is written **`received = accepted + sum(drops by reason)`** and the `played` form
   appears nowhere as an equivalent. It is labelled a **receive-path accounting identity, not a
   playback guarantee**, and the catalog states that ADR-0036/R-25 prose says "played", that this
   catalog **deliberately supersedes** that spelling, and why — all fifteen `drops_frame: true`
   reasons fire at or before the crypto/parse boundary, so the identity is exact there, while at the
   playback boundary a frame lost between handoff and audible decrements nothing on the right-hand
   side. Extending the identity to the playback boundary needs playback-side drop reasons: a
   vocabulary extension with its own planning, not a word swap.

**@security.**
- **S-1** `streamId` is unauthenticated and MH-written. It reaches **one** consumer (the hop-gap
  monitor's bucket), is validated against the declared slot set **before any state is created**
  (bound on state creation, not rendering), and never reaches key selection, roster lookup,
  attribution, or the replay window. `hopSequence` may be read by the monitor only and never gates a
  drop/play decision. No new reject token is minted; the frame is routed and attributed by its own
  key id.
- **S-2** Counter exhaustion documented as **unreachable rather than unchecked** (uint32 sequence,
  ~2.7 years at 50 fps), in `keyId.ts`'s idiom. Reconnect is unreachable this story and stated.
  Structurally: `#streamSequence` and `#generation` live on **one session-scoped object with no
  reset method in existence**, and no frame is renumbered after `sealSframe` — the only post-seal
  mutation is the relay hop patch, outside both AAD and signed range.
- **S-3** Identity keypair: `crypto.subtle.generateKey`, private half **`extractable: false`**,
  public half only exported, per meeting, never persisted (no storage API, no module-level
  singleton), dropped at teardown. Sink scan gains the storage-API family.
- **S-4** `assertEd25519Available()` gates **pipeline start, both directions** — TODO:894 predates
  the send path. A signing failure is a hard error that cannot yield a frame; asserted negatively
  (zero outbound datagrams, not a datagram with a zero signature).
- **S-5** Sink scan covers `JSON.stringify`, `structuredClone`, `console.*`, `logger.*`,
  `postMessage`/`Worker.postMessage`, template interpolation, storage APIs.
- **C-1** The uplink hop sequence is assigned at **dequeue**, never enqueue. At dequeue a client-side
  queue drop leaves hop contiguous and the gap lands on the stream sequence, so MH does not
  double-count a drop the client already counted; at enqueue the same drop would be reported twice in
  two services under two causes, one wrong. Written at the site with the reciprocal pointer to
  `docs/TODO.md:1022`, since MH's missing ingress hop-gap counter derives its meaning from this choice.
- **C-2** The `WrapOutcome` rename is confined to `receivePath.ts`, two sdk-core test literals and a
  label value — **all Mine, no `proto/**` edit, no vector row moved**. Classification table unchanged.

**@operations.**
- **F1** `transportOutgoingMaxAgeMs` 100 → **500 ms**. Handed to task 21 as a **named uncountable
  path** for Scenario 15's "sent rising, received flat" ladder, alongside the reaped NAT binding and
  stale MH policy.
- **F2** `meterProvider.forceFlush()` on teardown — at a 10 s cadence the lost window is the one
  containing the incident.
- **A** Keep both bounds explicit and **assert** the ordering (throw, never warn; tested). Deriving
  would have encoded a ratio nobody chose — a false SSoT. §11 names startup validation as the
  sanctioned fallback where derivation is impossible.
- **B** VBR-floor finding sized: per-frame overhead floors frame size at ~156 B, so MH's buffer holds
  at most ~48 frames ≈ **970 ms** against a documented 640 ms — a bounded ~1.5x understatement,
  **one-directional** (larger frames only make the budget conservative), and MH's compile-time pin is
  the "accidentally correct" case TODO:288 warns about and will keep passing.

**@dry-reviewer.**
- Replay/cache bounds hoisted to **three separately-named constants in the frame layer**, used as the
  constructor defaults, imported by `clientConfig.ts` (direction `config → frame`; `receivePath.ts`
  never imports config). The two `32`s stay two constants — independent decisions agreeing by
  coincidence, per the story-task-10 false-SSoT precedent.
- `frameDurationMs = 20` accepted as a third encoding with an `ANCHOR (DRY):` naming both siblings,
  the **existing** TODO entry amended in place (three encodings, `client` added as owner, trigger
  recorded as fired), and no duplicate entry filed.
- Bitrate ceiling corrected from `min(directive, config)` to **directive-wins with a named default**;
  the acceptance band is **dropped** (see the config table).
- `IWebTransport` widening: required-but-possibly-undefined, not optional (see §8 correction).

**@test.**
- Ingress reject and roundtrip tests are driven by **vector `frame_hex`** and anchored to
  `crypto.plaintext_hex`, with **real** Ed25519 / KEK-unwrap / SFrame primitives. `FakeAudioCodecs`
  replaces only the Opus media codec and is never reachable from a verify path. The self-loopback
  survives as an explicitly-labelled **wiring** test.
- The `wrap_for_different_key_id` row is added with its `receiver_precondition` honoured exactly —
  prime `0x…0708` via the **unwrap path only**, never through the replay-checked receive path (it
  shares `(key_id, stream_sequence)` with the row and would return a false-green `replay_detected`),
  and assert the primed state before delivery.
- Send-drop spellings get a const-with-derived-union SSoT; no retyped literal in emit sites or tests.
- Coverage exclusions are explicit and narrow, conditioned on the excluded files being logic-free.
- Mute is asserted by **frame index**, never elapsed time.

**@main.** Catalog routing is option (a). The §Naming convention carve-out ships **verbatim as
@observability supplied it** and is not redrafted. The `dt_client_mh_connection_total` annotation says
"do not copy this into a media emission", not merely "grandfathered". `docs/TODO.md` edits stay
line-separable for per-owner ACK. The export-interval blast-radius sentence goes at the constant's
definition site.

---

#### 10. Loopback proves wiring, not crypto

Stated here, and in the module header of `lifecycle/AudioPipeline.ts`: a single client encrypting and
decrypting with its own keys is **self-consistent** under a swapped AAD, an inverted nonce, a
truncated key id, or a signature over the wrong range. A green demo is not evidence of crypto
correctness. The only evidence is task 15's external-anchored vectors
(`proto/test-vectors/frame-v2.vectors.json`, the sframe-wg anchor test, the conformance suite),
which this task does not weaken, regenerate, or route around. If a vector row would need to change
to accommodate the pipeline, that is a finding for @security, not a fix.

---

## Pre-Work

None — working tree clean at `a99e1078`.

---

## Implementation Summary

One client now captures its own microphone audio, encodes it to Opus, seals and
signs each frame as an ADR-0036 v2 media frame, sends it as one QUIC datagram to
the media handler, and receives, verifies, decrypts, decodes and plays back its
own audio returned by MH. Audio only.

### Layout (ADR-0036 §11)

```
packages/sdk-core/src/
  config/clientConfig.ts        THE single configuration point
  media/
    frame/                      per-frame codec + crypto (task 15; two additive helpers)
    pipeline/                   HOT PATH — per-frame only; no logging, no metric names
    lifecycle/                  SIBLING — start/stop/mute/rotate, the read loop
    setup/                      SIBLING — capture, codecs, metric handles, seams, keys
    teardown/                   SIBLING — acquisition-time registry, idempotent
```

`media/__tests__/hotPathLayout.test.ts` is the TypeScript analogue of the Rust
directory-scoped deny: it fails on `console.`/`logger.`/a `dt_client_` literal
inside `pipeline/**`, fails if that directory is absent or empty, asserts the
siblings are siblings, and — the positive half — asserts that
`setup/mediaMetrics.ts` is the **only** file under `media/**` naming a metric at
all. That last one closes a hole the Rust rule does not have: the siblings sit
outside the deny by design and are exactly where the KEK, the roster keys and the
transmit keys live.

### Egress, in ADR-0036 §4's order

Mute is checked **at capture** → transmit key generated or rotated and wrapped
under the meeting KEK (once per generation, so the block is byte-identical within
one) → publisher region built **complete** → SFrame seal with the region as AAD →
Ed25519 signature over region‖payload → bounded drop-oldest queue → hop sequence
written **at dequeue** → one datagram.

The apparent circularity (the region carries `payload_length`; the payload is the
sealed output) is resolved by `sframeObjectLength()`: GCM is length-preserving.
`buildUnsignedFrame` now calls the same `buildPublisherRegion` the egress path
does, so there is **one writer of header bytes** and the AAD a sender seals under
is byte-identical to the region the frame ships with by construction.

### Ingress

Counted **at the wire** → decode → key id → roster resolve → verify → replay →
unwrap → decrypt → Opus decode → playback. `received = accepted + sum(drops by
reason)` holds by construction. Reject reasons are `FrameRejectedError.rejectReason`
**verbatim** — all eight structural codec tokens individually. The relay region is
quarantined to the hop-gap monitor and creates no state for an undeclared
`stream_id`.

### Telemetry

Twelve media metrics, all through one allow-list label projection whose
constructor takes two named strings rather than a label bag — so
`meeting_id_hash` is not merely absent, it is unrepresentable at that boundary.
Export cadence is 10 s from one named constant, with the global blast radius
stated at the definition site.

### Key custody

The meeting KEK lives in exactly one private field, arrives through a seam, is
scrubbed out of the decoded protobuf at the boundary, never rides an event or an
error, and is zeroed at teardown. The identity signing keypair is generated per
meeting with `extractable: false` and is never persisted.

---

## Files Modified

Full diff: `git diff a99e10787dd60823590a46684c24f0faea519a83`.
Per-file classification is in §Cross-Boundary Classification above.

### `packages/sdk-core` — new

| File | What |
|---|---|
| `src/config/clientConfig.ts` | the single configuration point: Opus, rotation `T`, queue bounds, transport queue knobs, receiver-state bounds, metric cadence; `validateMediaConfig` throws (never clamps) on ranges **and on the two relationships** |
| `src/media/pipeline/egress.ts` | build → seal → sign → queue → send, in §4's order; hop sequence written at dequeue. Takes the `MeetingIdentity` HOLDER and reads `.signer` at the point of use, so teardown revokes the signing capability rather than leaving an independent copy alive; `EgressIdentityReleasedError` when a frame reaches signing after release |
| `src/media/pipeline/ingress.ts` | wire count → decode → resolve → verify → replay → unwrap → decrypt → decode → play |
| `src/media/pipeline/egressQueue.ts` | bounded drop-oldest ring; `push` returns the evicted item, the caller counts |
| `src/media/pipeline/playbackSink.ts` | scheduled playback with a re-seating playhead |
| `src/media/pipeline/hopSequenceMonitor.ts` | downlink gap/reorder detection, bounded by the declared slot set |
| `src/media/lifecycle/AudioPipeline.ts` | the session-scoped orchestrator and the read loop |
| `src/media/lifecycle/muteState.ts` | client mute, distinct from an empty send directive |
| `src/media/lifecycle/transmitKeys.ts` | generation + sequence on one object, no reset method in existence |
| `src/media/setup/mediaMetrics.ts` | the allow-list label projection and every cached metric handle |
| `src/media/setup/kekSource.ts` | the KEK-source seam |
| `src/media/setup/identity.ts` | the meeting identity holder — one non-extractable signing capability, per meeting, never persisted, revoked at teardown |
| `src/media/setup/captureFailure.ts` | the capture failure vocabulary and its classifier, split out so the six-way branch is scored rather than hidden behind a coverage exclusion |
| `src/media/setup/rosterKeys.ts` | `sender_id` → `CryptoKey`, fail-closed, bounded, imported once per sender |
| `src/media/setup/measurement.ts` | the first-media measurement seam |
| `src/media/setup/seams.ts` | capture/codec/playback seam interfaces |
| `src/media/setup/capture.ts` | `getUserMedia` + device selection, typed bounded failures |
| `src/media/setup/opus.ts` | encoder/decoder construction + the voice-tuning support probe |
| `src/media/setup/audioPlayback.ts` | `AudioContext` construction and the suspended-context refusal |
| `src/media/teardown/teardown.ts` | acquisition-time disposer registry, idempotent, failure-returning |
| `src/signaling/kekIntake.ts` | the TS-side KEK sink control |

### `packages/sdk-core` — modified

| File | What |
|---|---|
| `src/media/frame/frameCodec.ts` | `buildPublisherRegion` (now the single writer, called by `buildUnsignedFrame`), `UnsignedFrame.relayRegionOffset`, `writeHopSequence`. Wire-neutral |
| `src/media/frame/sframe.ts` | `sframeObjectLength` only. `unwrapTransmitKey` untouched and still pure |
| `src/media/frame/receivePath.ts` | three exported bound constants used as the constructor defaults; `verifyFrame` accepts a cached `CryptoKey`. Brand, ordering and cache-write reachability unchanged |
| `src/media/frame/ed25519.ts` | `generateIdentityKeyPair`; `verifyFrameSignature` accepts a `CryptoKey`, still fail-closed |
| `src/media/MediaTransport.ts` | datagram channels, the queue knobs set from config, and the grandfathering annotation at `#emitMetric` |
| `src/transport/IWebTransport.ts` | `WebTransportDatagrams` with three required-but-possibly-undefined knobs plus `maxDatagramSize`; the Pattern-A comment corrected to describe what the guard actually covers |
| `src/signaling/SignalingClient.ts` | identity key on join, KEK intake, roster feed, `sendDirective`/`streamAssignments`/`meetingKekUpdate` dispatch, `sendReceiveCapability`, `sendMuteRequest` |
| `src/session/MeetingSession.ts` | identity keypair before join, `startMedia()`, media teardown, `forceFlush` |
| `src/telemetry/telemetryConfig.ts` | `exportIntervalMillis` from one named constant; `flushMetrics()` |

### `packages/test-utils`

| File | What |
|---|---|
| `src/MockWebTransport.ts` | datagram send/receive, injected outbound drops, injected write failures, back-pressure pause/resume, inbound loss recording, readable-end, and the three queue knobs read back |
| `src/contracts/IWebTransport.ts` | Pattern-A parity with the sdk-core widening |
| `src/media/index.ts` | capture / codec / playback doubles — **never** frame crypto |

### Documentation

| File | What |
|---|---|
| `docs/observability/metrics/client.md` | new §Media-path metrics; the owner-authored implicit-labels carve-out; the frozen grandfathered roster; the near-twin note on `dt_client_time_to_first_mh_connected_ms`; the stale not-yet-emitted banners rescoped |
| `docs/observability/dashboard-conventions.md` | the client OTel export cadence row now cites the config key and records the global blast radius |
| `docs/user-stories/2026-08-27-hear-yourself-through-handler.md` | R-25 gains the owner-authored `accepted` clause, additive, line 53 only |
| `docs/TODO.md` | three entries closed, five amended, one filed |
| `docs/specialist-knowledge/client/INDEX.md` | navigation for the new modules |

---

## Devloop Verification Steps

Run from the repository root.

```
# TypeScript typecheck (both packages)
npx tsc --noEmit -p packages/sdk-core/tsconfig.json
npx tsc --noEmit -p packages/test-utils/tsconfig.json

# Lint
npx eslint packages/sdk-core/src packages/test-utils/src

# Formatting (TS only — markdown is not in prettier's scope in this repo)
npx prettier --check "packages/sdk-core/src/**/*.ts" "packages/test-utils/src/**/*.ts"

# Unit suites + the >=90% coverage gate
( cd packages/sdk-core && npx vitest run --coverage )
( cd packages/test-utils && npx vitest run )

# Guards touched by this diff
./scripts/guards/simple/validate-frame-vectors.sh
./scripts/guards/simple/validate-cross-boundary-classification.sh \
  docs/devloop-outputs/2026-09-08-sdk-audio-media-pipeline/main.md
```

Results at the time of writing: typecheck clean, eslint clean, prettier clean,
**612 sdk-core tests + 14 test-utils tests pass**, coverage 96.86 % statements /
90.02 % branches / 96.29 % functions / 97.98 % lines against the 90 % gate, and
both guards `STATUS=OK`.

### The two structural guards, and how to see them fail

Neither is a formality, so both fail loudly if their scope evaporates:

- `packages/sdk-core/src/media/__tests__/hotPathLayout.test.ts` fails if
  `media/pipeline/` is absent or contains no `.ts` file, not only when it
  contains a violation.
- `packages/sdk-core/src/__tests__/serverMessageSinkScan.test.ts` fails if it
  scans fewer than 20 production sources.

Both run in the sdk-core unit tier, so they GATE on the pipeline's TypeScript
lane rather than only in a local watch. The chain, shown rather than asserted:

```
scripts/layer4.sh -> scripts/lang/ts/test.sh
  -> pnpm exec nx run-many -t test:unit test:component --all
  -> packages/sdk-core/project.json  target `test:unit`
  -> vitest run --coverage           (cwd packages/sdk-core)
```

`vitest.config.ts` includes `src/**/__tests__/**/*.test.ts`, which is where both
guards live, and the same invocation enforces the coverage thresholds. Layer 4
reports `STATUS=OK REASON=nx-test-passed`.

### What these steps do NOT prove

**The loopback proves wiring, not crypto.** A single client encrypting and
decrypting with its own keys is self-consistent under a swapped associated data,
an inverted nonce, a truncated key id, or a signature over the wrong range, and
every one of those passes the round-trip test green. Cryptographic correctness is
proven only by the externally anchored vectors in the codec task's conformance
suite, which this diff does not weaken, regenerate, or route around — and which
this diff makes *stronger* by reading two bindings that were previously declared
in the SSoT and asserted by nothing.

---

## Code Review Results

Full verdict table, totals, and the blocker write-up are in §Gate 3 — Final Approval below. Per-reviewer
summary:

### Security Specialist
**Verdict**: RESOLVED-FIXED — 10 found, 10 fixed, 0 deferred

S-1 unauthenticated `streamId` unbounded (bounded on state creation against the declared slot set, and
quarantined from key selection, roster lookup, attribution and the replay window). S-2 two missing
ADR-0036 §4 rotation triggers plus the sequence/generation coupling — made structural: no reset method
exists, both counters live on one session-scoped object, counter exhaustion documented as unreachable
rather than unchecked. S-3 identity-key custody. S-4 `assertEd25519Available()` gating egress as well
as ingress. S-5 sink scan widened to `structuredClone`/`postMessage`/storage. S-6/S-6b seam objects and
sibling metric labels. S-7 `generateIdentityKeyPair` exported with zero consumers — dropped, since it
would have let an embedder rebuild in application code the bare-record shape the SDK had just removed
from itself, in the one place no guard of ours runs. **S-8** the AES-GCM (key, nonce) reuse (see §Gate 3).
S-9 the retirement bound stated against the wrong quantity — rotation is event-driven, not periodic,
and `audioRotationPeriodMs` had no floor; both fixed, floor derived against the measured read window.

### Test Specialist
**Verdict**: RESOLVED-FIXED — 2 found, 2 fixed, 0 deferred

Both findings were coverage exclusions hiding real logic behind a comment asserting it was tested
elsewhere: `classifyCaptureError` (extracted to `captureFailure.ts`, six arms tested) and
`probeEncoderSupport`'s tuning comparison (extracted as pure `effectiveTuning`). Branch coverage moved
90.19% → 90.40% *because* the extraction put real branches back under the gate rather than hiding them.
Test also mutation-tested the S-8 negative: with the coalescing guard removed it fails on `wraps.size`
(8 distinct wraps under one key id) while `keyIds.size` stays 1 — isolating the reuse itself rather
than tripping on an incidental inequality.

### Observability Specialist
**Verdict**: RESOLVED-FIXED — 1 found, 1 fixed, 0 deferred

`media/events.ts:44` documented the join label set unscoped as the module convention — the sentence
both `label-taxonomy.md` R3 and `docs/TODO.md` D5 cite *by path* as the inertia source. Plus six
owner-implemented hunks across three of their own files, including retiring R3's own now-false
assertion while preserving the inertia argument as the rationale for the allow-list shape.

### Code Quality Reviewer
**Verdict**: CLEAR — 0 findings

Wire-neutrality of the `frameCodec.ts` refactor verified empirically (261/261 frame tests including
conformance and drift), every consumed proto shape confirmed pre-existing in `signaling.proto`, full
ADR compliance section and Ownership Lens recorded. One commit-time ownership condition, discharged by
the trailers on this commit.

### DRY Reviewer
**Verdict**: RESOLVED-FIXED — 5 found, 5 fixed, 0 deferred

Orphaned `ReplayWindow` doc block; ~35 lines of rationale prose duplicated byte-identically across the
Pattern A pair with no forcing function; three TODO entries deleted rather than marked closed in a diff
that closed a fourth correctly; and a post-verdict follow-up where two files explained "no drift guard"
without pointing at the TODO entry carrying the sharper statement.

**Extraction opportunities** (`docs/TODO.md` §Cross-Service Duplication): the SDK↔MH bounded-queue
cross-language pair, with the drop *policy* rather than the code named as the drift risk; and the
five-token send-drop vocabulary mirroring MH's `MediaDropReason` with no drift guard, stating the
`protocol` question along with the argument against widening a GSA to hold a telemetry vocabulary.

### Operations Reviewer
**Verdict**: RESOLVED-DEFERRED — 6 found, 5 fixed, 1 accepted spin-out

F1 `transportOutgoingMaxAgeMs` at 100 ms reintroduced an uncountable UA-level drop competing with the
counted app queue (now 500 ms, asserted at setup). F2 `forceFlush()` on teardown. F3 the
`wrap_key_id_mismatch` runbook objection (routed to observability, resolved in-diff by the catalog
leading with the observable firing condition). F4 two `void`-ed promises that reject on the teardown
path, landing in the embedder's page during an incident. F5 "accepted and played" contradicting the
supersession ten lines above. **F6 spun out** to media-handler.

### Semantic Guard Reviewer
**Verdict**: RESOLVED-FIXED
**Native verdict**: SAFE
**Findings**: 1 found, 1 fixed, 0 deferred

`[credential-leak]: packages/sdk-core/src/__tests__/serverMessageSinkScan.test.ts:67` — the KEK sink
scan omitted template interpolation, a sink its own header named. Fixing it surfaced two further scan
defects: stripped-line numbering (findings would have pointed at the wrong line) and a false positive
on a debug message merely containing the word `ServerMessage`. Because the fix could have made the scan
vacuous while staying green, it gained a demonstration test verified load-bearing in both directions.

---

## Accepted Deferrals

- `docs/TODO.md` §Media Path Obligations — `wrap_key_id_mismatch` rename spun out to protocol, trigger FIRED
- `docs/TODO.md` §Media Path Obligations — VBR floor vs MH's `NOMINAL_AUDIO_BITRATE_BPS`, spun out to media-handler

---

## Scope Decisions

Not deferrals — nothing below was a finding left in the diff. Recorded here so the two entries under
§Accepted Deferrals stay legible as the actual cost shifts rather than being buried among follow-ups.

- **`docs/TODO.md` D5 amended, not closed.** The grandfathering ruling creates a standing obligation
  (keep the roster frozen; classify every future client metric against the observes-what line) that no
  guard enforces — the enforcement gap is D4, which stays open. Closing D5 would retire a live rule
  into a completed-work note.
- **Counting-point migration** for `dt_client_media_frames_received_total` (transport boundary → parse
  boundary) is filed for the video story, per the task prompt's own instruction.
- **Runbooks, alerts and dashboards** are story tasks 21 and 22, both `deps: [19]`. Task 19 owes none;
  what it owed was the metric vocabulary those tasks freeze on. No numbered scenario or alert rule
  appears in this diff, and task 21's reserved scenario headings are untouched.
- **`docs/observability/label-taxonomy.md`** is owner-implemented by `observability` during review —
  deliberately outside the implementer's authorship, present in the classification table so the scope
  guard passes without a false authorship claim, and in this commit.
- **Identity-key vocabulary classification** — whether the retained identity signing capability warrants
  a `pii_vocabulary.rs` entry as `meeting_kek` and `transmit_key` have. Raised and deliberately not
  settled here: vocabulary content is `security` + `observability`'s per CLAUDE.md, and a term added by
  whoever a guard inconvenienced is what that guard's no-bypass clause exists to prevent. Filed by
  security under `docs/TODO.md` §Media Path Obligations.

---

## Rollback Procedure

1. Start commit: `a99e10787dd60823590a46684c24f0faea519a83`
2. `git diff a99e1078..HEAD`
3. `git reset --soft a99e1078` (or `--hard` for clean revert)

---

## Issues Encountered & Resolutions

**Branch coverage sits close to its gate, and that is a known state rather than a
surprise.** The suite reports **90.19 % branches against a 90 % threshold** — the
other three metrics have comfortable margins (96.9 / 96.3 / 98.0), but branches
does not. The branches that matter are on the media path: the fifteen drop
reasons, the fail-closed empty-key branch, the undeclared-`stream_id` path, the
send-drop vocabulary. Recorded because a later fix that moves one branch could
put the figure under the gate, and "coverage fell below the threshold" is a much
worse thing to meet at a Gate-3 rerun than here. The remedy if it happens is to
add branch coverage, not to move the gate — the gate is @test's.

**A bound stated against the wrong quantity (@security S-9).** The retirement
deferral that fixed the third race was described as "one rotation period ...
six orders of magnitude of headroom". Both halves of that were wrong, and I
verified each at source rather than taking the finding:

- **Rotation is event-driven, not periodic.** `rotate()` has three call sites in
  `AudioPipeline` — the interval timer at `:334`, resume-from-empty at `:351`,
  and unmute at `:368`. The bound is the interval between two consecutive
  rotations FROM ANY TRIGGER, which is caller-controlled: two `setAudioMuted(false)`
  calls in quick succession rotate twice milliseconds apart.
- **`audioRotationPeriodMs` had no floor** — `positiveInteger` only, so `1` was
  legal, and even the periodic path could be configured below the read window.

Fixed in two parts: the comment now names the three triggers and states which
half is validated and which is not, and `validateMediaConfig` enforces
`MIN_AUDIO_ROTATION_PERIOD_MS`, derived against the actual read window (one
HKDF-extract) rather than chosen for looking sensible.

**The finding was about the CLAIM, not the design** — and that is why it mattered.
This gate spent considerable effort on the principle that an almost-right
justification is worse than none: a reader who checks "six orders of magnitude"
against `setAudioMuted` finds it false and discards the whole paragraph,
including the part that is load-bearing. Hedging on the correct axis (bound
versus guarantee) while overstating the magnitude is its own failure mode.

**The read loop would have thrown on the second send directive.** MC re-issues a
directive whenever meeting state changes, and a `ReadableStream` admits exactly
one reader, so the second `getReader()` throws. Found by asking what the second
call does rather than by a failing test — and the failure would have presented as
*"media stopped after someone joined"*, which is a long way from its cause. Fixed
with a one-shot flag and a test that issues the directive twice and asserts the
single loop still delivers.

**`clear()` did not mean what it says — a revocation that was only partial.**
`EgressPipeline` held its own `CryptoKey`, copied out of `MeetingIdentity` at
construction. `MeetingIdentity.clear()` released the holder's reference and the
egress kept an independent one, so teardown did not revoke the signing
capability. That is ADR-0028 §5's "clear all references" defeated by a copy.

Fixed by passing the HOLDER and reading `identity.signer` at the point of use,
with a typed `EgressIdentityReleasedError` when a frame reaches signing after
release. Mutation-verified: restoring the pre-fix shape makes the new test fail
with `promise resolved "undefined" instead of rejecting`.

**Sized precisely rather than at maximum (@security).** `submit` short-circuits
on `#stopped` both before building and before enqueuing, so **no frame could be
emitted after teardown and there was no exploit path in the shipped code.** What
survived was the *capability*, usable by an in-flight build after the holder had
revoked it. It matters anyway, and not because of a current exploit: **a holder
whose revocation is partial hands the next consumer a guarantee that is already
false**, and task 20's web app or the video pipeline will read the method name
rather than trace the copies. The defect was in the guarantee.

**How it was found, stated plainly because the honest version is less flattering
than the alternative.** The `ts_retained_credentials` guard fired on
`signingKey`, I went looking for a narrowing BECAUSE it fired, and found this.
The guard's own predicate is wrong here — it is name-shaped, and the material is
a non-extractable `CryptoKey` either way — so it was a false positive that
happened to sit next to a real adjacent defect. That combination is common and
fine. What would not have been fine is finding nothing and shipping a rename
anyway, which is why the change was put back to @security with the uncomfortable
third fact attached rather than the two comfortable ones.

**A (key, nonce) reuse under the meeting KEK, found at review.**
`TransmitKeyManager.materialFor` read the key map, awaited the KEK wrap, and
wrote the map — with no in-flight guard. Two concurrent callers on a miss both
minted an independent random key and packed the SAME key id, because sender,
stream and generation are all unchanged across the await. `wrapNonce(keyId)` is a
pure function of the key id, with the key id also as the associated data, so the
two wraps are one (key, nonce) pair with different plaintexts — ADR-0036 §4's
catastrophic case. Reachable at every rotation boundary via the fire-and-forget
`void egress.submit(frame)`.

Fixed by memoising the **in-flight promise** rather than its result, so every
concurrent caller for a stream receives one key. Three tests, including a
negative whose failure message names the hazard.

**A second race with the same root cause, and a third found while fixing them.**
`rotate()` is synchronous and cleared the current keys, but a mint already in
flight resumed afterwards and reinstated pre-rotation material — no nonce reuse,
but the rotation silently did not take effect, so the leaked-key window it exists
to bound was not bounded. Fixed by capturing the generation at mint entry,
installing only if it is still current, and clearing the in-flight map on rotate.

The third surfaced from checking whether the fix could zero a key in use.
`rotate()` overwrote the superseded key buffers, and a transmit key is read
**after an await** by the frame being sealed with it — `deriveSframeKeys` ->
`hkdfExtract` -> `hmac`, which reads the material at its `sign()` call downstream
of `await importKey`. A rotation firing from its interval timer inside that
window would have sealed a frame under an all-zero key while its wrap announced
the real one, surfacing as `decrypt_failed`, whose triage points at the key
schedule or the sender. Overwriting is now deferred by one rotation period —
stated as the bound it is, not a guarantee — with `clear()` catching the
remainder at teardown, where there is no in-flight reader to race.

**A credential-lifetime guard fired on ADR-mandated retention, and it was a true
positive.** `dt-guard ts-no-retained-credentials` failed Layer 3 on
`MeetingSession`'s `#identity: IdentityKeyPair` field: the type declares
`privateKey`, which segments to `private`+`key` and matches `CREDENTIAL_TOKENS`.
The guard has **no bypass marker by design** and its stated remedies are "remove
the field, or stop retaining the type".

Retaining the signing capability is not optional — ADR-0036 §3 requires every
frame to be signed and §4 states that the signing key is long-lived — so the
question is which remedy. Taken: **stop retaining the TYPE.** The keypair record
is now a transient of `MeetingIdentity.create()`, and what the session retains is
a holder whose fields are a non-extractable `CryptoKey` and the public bytes.

**This is a narrowing of what is retained, not a claim that nothing is**, and it
is disclosed at the site (`media/setup/identity.ts`'s header) rather than routed
around silently. It is also the right shape independently of the guard: every
other piece of media key material in this SDK already sits behind a narrow holder
— the meeting KEK behind `JoinResponseKekSource`, peer keys behind
`RosterIdentityKeys`, transmit keys behind `TransmitKeyManager` — and the
identity keypair was the one piece left as a bare record.

**The residual question is policy and was NOT settled here**: whether the
identity signing key warrants a classification in
`crates/dt-guard/src/common/pii_vocabulary.rs`, as `meeting_kek` and
`transmit_key` already have on exactly this entitled-long-lived-holder reasoning.
Per CLAUDE.md's guard-ownership split, vocabulary *content* belongs to security +
observability, not to whoever is inconvenienced by the guard. Raised with them.

**The `IWebTransport` widening broke exactly what it was supposed to break.**
Adding the three datagram queue knobs failed compilation in five places at once —
`BrowserWebTransport`, both transport tests, a signaling fixture, and
`MockWebTransport` — which is the Pattern-A drift guard working. It only worked
because the members are declared `number | undefined` rather than `?: number`;
declared optional, TypeScript assignability would have ignored every one of them
and the guard would have gone silently blind to the newest members while its
comment kept promising coverage.

**A second derivation of the SFrame object length, wrong for a right-looking
reason.** The egress path first computed `payload_length` as
`keyId.length + AES_256_KEY_BYTES / 2 + plaintext.length`. That is 8 + 16 + n,
which is correct today and correct *by accident* — `AES_256_KEY_BYTES / 2` is not
the tag width, it merely equals it. Replaced with `sframeObjectLength()` in
`sframe.ts`, derived from the same constants the serializer uses.

**The no-sender-id test passed because of a JavaScript default parameter.**
`joinedWithMedia(senderId: number | undefined = 258)` called with an explicit
`undefined` substitutes the default, so the "MC assigned no sender id" case was
silently exercising the happy path. Changed the sentinel to `null`.

**`kek_generation_not_held` was unreachable in the first attempt at its test.**
Re-wrapping the same transmit key under the same KEK produces a byte-identical
block, so the receiver's cached-wrap skip fired and the outcome was
`already_held`. The condition needs the wrap to genuinely differ — a NEW KEK
announced under a generation the receiver does not hold — which is also what the
condition means in production.

**`settle()` had to yield macrotasks, not microtasks.** The egress path awaits
real WebCrypto (KEK wrap, HKDF schedule, SFrame seal, Ed25519 signature), which
resolves after libuv thread-pool work, so draining the microtask queue left every
assertion at zero. It is a scheduling yield, not a timing assertion; no test here
asserts that anything completes within a duration, and none may.

---

## Lessons Learned

### Named pattern: a control that reads as coverage without providing it

Eight instances: four surfaced at Gate 1 (three in my own plan), a fifth during implementation, a
sixth and an eighth at review, and a seventh at staging. Three of them are reviewers' rather than
mine. They are not eight mistakes; they are one mistake with eight costumes. The shape: **a mechanism exists, is visible, and
is cited as protection — while the thing it actually covers is not the thing being protected.** It is
strictly worse than no control, because its presence redirects scrutiny. ADR-0036 names it at
*"A control's coverage must be demonstrated, not asserted"*; these are live instances.

1. **The `[32000, 48000]` bitrate acceptance band** (caught by @dry-reviewer). Proposed as a
   validation bound. Its lower edge was MH's private buffer-sizing assumption, not a correctness
   bound on the wire value — and by my own VBR analysis a client-side floor check does not achieve
   that anyway, because instantaneous frames fall below it regardless. A guard that could not
   possibly enforce the property it was named for, whose only distinctive behaviour would have been
   hard-failing every fielded SDK the first time an operator legitimately raised
   `MC_AUDIO_MAX_BITRATE_BPS`.
2. **The `IWebTransport` optional-member widening** (caught by @dry-reviewer). I claimed "the compile
   break is the forcing function working as designed." TypeScript assignability **ignores missing
   optional properties**, so the Pattern-A guard at `BrowserWebTransport.test.ts:24-25` would have
   gone silently blind to exactly the members being added, while its comment kept asserting a
   guarantee it no longer provided.
3. **`vectors.conformance.test.ts:407`** (caught by @security). The SSoT declares
   `vectors[].expected.outcome` specifically *"so a codec in a third language cannot get this wrong
   by omission"*; the TypeScript harness hardcoded the literal while reading every neighbouring
   field from the row. The binding existed, the type declared it, the plumbing was there — and
   nothing read it. @observability additionally read that site's comment (*"CACHE STATE is the
   assertion, not a reject reason"*) as evidence the decoupling was **deliberate**; it was evidence
   of the gap. A defect misread as a design decision.
4. **`row.reject_reason` on the same row** (caught by @security, after the ruling). The same value is
   pinned **twice** in the GSA file and *neither* binding was asserted. The three reads of
   `row.reject_reason` are each guarded on a `kind` that this row does not have. Finding one of the
   two would have left a partial rename able to pass green with the SSoT carrying two spellings of
   one concept.

5. **A second derivation of the SFrame object length** (caught by me, during
   implementation — which is the better evidence that the instrument works).
   The egress path computed the sealed payload length as
   `keyId.length + AES_256_KEY_BYTES / 2 + plaintext.length`. It is correct
   today and correct BY ACCIDENT: `AES_256_KEY_BYTES / 2` is not the AEAD tag
   width, it merely equals it. A reviewer reading it would see the right number
   and the wrong reason, and the two are indistinguishable until the key size
   changes. Replaced by `sframeObjectLength()` in `sframe.ts`, derived from the
   constants the serializer itself uses.

6. **`TransmitKeyManager.materialFor`'s read-modify-write across an `await`**
   (caught by @security at review; the sharpest instance of the six, and the one
   found last). Two concurrent callers on a cache miss both minted a key and
   packed the SAME key id, and `wrapNonce(keyId)` is a pure function of the key
   id — so two AES-GCM seals under one (key, nonce) with different plaintexts:
   ADR-0036 §4's catastrophic case, yielding authentication-key recovery.

   **Two things make it the sharpest.** First, the control that should have
   caught it was a COMMENT ADDRESSED TO THIS EXACT CALLER —
   `receivePath.ts::TransmitKeyCache.set` says *"were that ever violated ... it
   would be a (key, nonce) repeat under the KEK"*, naming the obligation this
   file is the caller for. It read as coverage and provided none, because a
   comment cannot fire. Second, `transmitKeys.test.ts` drove `materialFor`
   sequentially throughout, so **a green suite and a green seven-layer pipeline
   coexisted with a live crypto defect**: the tests could not express the input
   that would make the control fire, which is the instrument below stated as a
   negative.

7. **Layer 3 was green all devloop over a smaller file set than the one that
   would land** (caught by @main at staging). `ts_retained_credentials.rs` uses
   `get_tracked_files` — tracked files only — so twenty-one new files were
   invisible to it until `git add`. Staging did not merely reveal violations; it
   **changed a guard's verdict**, which is a stronger statement than "the
   pipeline could not tell reviewed-from-landing apart".

   **Stated in its corrected, narrower form**, because @security traced the
   mechanism rather than leaving it as "the guard was blind": FOUR guards use
   `get_tracked_files`; the other nine use `get_all_changed_files`' union and saw
   everything throughout. So the blindness was guard-specific, not pipeline-wide.
   And when the full 99-file set was finally scanned it produced exactly three
   violations, all one false positive — **the process was late-binding; the code
   was clean.** The broad version of this lesson would be more alarming and less
   true, and the narrow one is what will still be accurate when someone acts on
   it.

   **What makes that reassurance checkable rather than merely asserted**: after
   the fix the guard reports `ts-no-retained-credentials-clean-99-files` — the
   SAME 99 that produced the three violations. The scope did not move; only the
   code did. Had it come back with a smaller count, the right response would have
   been to find which files fell out of scope, because **a guard that passes on a
   smaller set than it previously failed on is the narrows-silently-while-green
   failure this whole section is about.** The number is the evidence, so it is
   quoted from the guard's own output rather than from an adjacent quantity —
   the staged-file count is 72, which is a different thing that sits close enough
   to be transposed, and was, twice, in status messages before @security ran the
   guard and corrected it.

8. **@security's own "there is nothing to narrow"** — asserted from the SHAPE of
   the retention path (one key, three sites, all heading to `signFrame`) without
   reading whether the capability was COPIED or REFERENCED at each hop. Reading
   `this.#signingKey = options.signingKey` with the question *"does `clear()`
   reach this?"* would have found it. Recorded because the pattern is not a
   property of any one role: four of the eight instances were reasoning from a
   structure the reader already trusted, and they are distributed across the
   implementer and two reviewers.

**The evasion test that came out of instance 8, because it is the reusable part.**
The question *"is this change gaming a guard?"* cannot be answered from motive —
a guard firing is a legitimate PROMPT, and going looking because it fired is not
disqualifying. The test is whether the change has a **property independent of the
guard**. The refactor refused earlier in this gate had none: the retained
material and its lifetime were identical either side of it, so there was nothing
to point at but the green. The one accepted here has three — the retained state
changed, the behaviour under `clear()` changed, and a mutation test fails with a
specific message when the old shape is restored. The green guard is a fourth
thing and is still not evidence; the justification rests on ADR-0028 §5, which a
copied handle defeats silently.

**The generalisation, and the reason it belongs here rather than in a note.** In every instance the
error was reasoning from the *shape* of a control instead of reading what it covers — "there is an
assignability guard", "there is a validation bound", "the vectors pin the tokens". Each statement was
true. None of them covered the case at hand. The instrument that would have caught all four is the
same: **name the exact input that would make the control fire, then check that input reaches it.**
An optional property does not reach an assignability check. A VBR instantaneous rate does not reach a
configured-value band. A hardcoded literal does not reach the row. A `kind`-guarded read does not
reach a `wrap_binding` row. **A second concurrent caller does not reach a sequential test — and a
comment is not reached by anything at all.**

**Corollary, stated by @security:** a vectors binding that exists but is never read is not evidence of
anything — which is the same failure mode as a green loopback demo, one layer up. This story's own
acceptance framing already says the loopback cannot catch a self-consistent crypto error; the four
instances above are that principle applied to the controls rather than to the pipeline.

### The sub-pattern: prose standing in for a mechanism

Two of the six instances are the same narrower thing, and @dry-reviewer named it
at review: **a comment that describes an invariant is not a control over the code
that must hold it.**

- `TransmitKeyCache.set`'s comment predicted the (key, nonce) reuse *in terms*,
  addressed to the caller-side obligation — and the caller it was addressed to is
  the one that violated it.
- The `IWebTransport` Pattern-A comment asserted that the assignability guard
  would catch a missing member, and would have kept asserting it for a member
  shape the guard cannot see.

Both were true sentences that could not fire. The distinction worth carrying is
that a comment CAN be the honest control — `rejectReason.ts`'s no-key-material
rule is review-enforced and says so, and `transmitKeys.ts` now records the
overwrite-deferral bound as a bound rather than a guarantee — but only when it
names itself as the weaker form. A comment that reads as though a mechanism
stands behind it is worse than an absent one, because it stops the next reader
looking for the mechanism.

9. **"The staging gap silently weakened every guard in Layer 3"** (@team-lead, at
   staging). Reasoned from `get_all_changed_files` — the union function, just
   read — to a conclusion about guards that do not call it. The union does see
   untracked files and nine guards were never blind; the cause is the four guards
   calling `get_tracked_files`. Wrong in substance, not only in scope: "the
   process was late-binding; the code was clean" is the corrected claim, and the
   broad version would have sent someone auditing a pipeline-wide blindness that
   did not exist.

10. **The Lead validated a tree it was still editing** (@team-lead, at commit).
    The final pipeline run was launched, and `main.md` — read by
    `validate-cross-boundary-classification`, `validate-cross-boundary-scope` and
    `validate-todo-tracking` — was appended to while it ran. That is instance 7
    exactly: a verdict computed over a different artifact than the one that would
    land, produced by the person who had just written instance 7 up. Caught by the
    implementer, who declined to apply an edit for the same reason and said so.
    Resolved by staging first and re-running the full pipeline, so the green covers
    precisely what commits.

Ten instances: four surfaced at Gate 1 (three in the implementer's own plan), a fifth during
implementation, a sixth and an eighth at review, a seventh at staging, and a ninth and tenth in the
Lead's handling of the seventh. **Six of the ten are reviewers' or the Lead's rather than the
implementer's** — which is what turns "this is a property of the position, not the person" from an
inference drawn by the author into an observation anyone can check. They are not ten mistakes; they
are one mistake with ten costumes.

**Also worth recording:** the gate worked because reviewers pre-registered what they would check
*before* seeing a plan, and because two of them verified at source rather than reasoning from the
shape — @dry-reviewer read `mc-service/src/config.rs` before ruling on the band, @security read the
vectors file before accepting "no GSA is touched". Both rulings reversed a confident, internally
consistent argument. The reversals came from reading, not from arguing.

And once from the other direction: a reviewer's own finding put Layer 3 red (their knowledge INDEX
went one line over its cap), the implementer declined to fix it because the content was theirs, and
they fixed it themselves citing their file's own precedent rather than their judgement. The
ownership rule is easy to honour when it costs nothing; that was the inconvenient direction.

---

## Gate 2 — Validation (Lead-run, authoritative)

Run by the Lead per SKILL.md Step 6. The implementer's own `layer-all.sh` run was stopped
mid-Layer-7 and NOT re-run; their local typecheck/lint/test/guard results are evidence, not the gate.

Command: `DEVLOOP_FAIL_FAST=0 ./scripts/layer-all.sh` (unattended contract — ALL seven layers, one
pass, everything broken reported at once). Wrapper exit 0.

```
LAYER=1 RESULT=OK  DURATION=3      compile   (buf-build, cargo-build, dt-guard, dt-story, nx-typecheck)
LAYER=2 RESULT=OK  DURATION=2      format    (buf-format, cargo-fmt, nx-format)
LAYER=3 RESULT=OK  DURATION=49     guards    (guards-passed + 13 guard self-tests)
LAYER=4 RESULT=N/A DURATION=176    test      (cargo-test OK, nx-test OK, proto N/A placeholder)
LAYER=5 RESULT=OK  DURATION=6      lint      (buf-lint, cargo-clippy, nx-lint)
LAYER=6 RESULT=N/A DURATION=1      audit     (cargo-audit OK, pnpm SKIPPED-NO-DIFF, buf-breaking OK, proto N/A placeholder)
LAYER=7 RESULT=OK  DURATION=278    env-tests (env-tests-passed AND browser-e2e-passed)
TOTAL_DURATION=515 TOTAL_RESULT=N/A
```

**Verdict: PASS.** No layer FAILed and no layer rendered `NOT-RUN`. Every wrapper performing real
work reported `OK`: `buf-build`, `cargo-build`, `nx-typecheck`, `buf-format`, `cargo-fmt`,
`nx-format`, `guards-passed`, `cargo-test-passed`, `nx-test-passed`, `buf-lint`, `cargo-clippy`,
`nx-lint`, `cargo-audit-passed`, `buf-breaking-passed`, `env-tests-passed`, `browser-e2e-passed`.

**Why `TOTAL_RESULT=N/A` is not a failure, checked rather than assumed.** ADR-0033 §STATUS
aggregation defines the worst-child order as `FAIL > N/A > SKIPPED-NO-DIFF > OK`, so a single `N/A`
child dominates a layer whose other children all passed. Layers 4 and 6 each dispatch proto, which
deliberately has no real `test.sh`/`audit.sh` and ships the documented one-line **intentional-gap
placeholder** emitting `STATUS=N/A REASON=not-applicable-to-this-lang` (ADR-0033 §6). The ADR's
exit-code table lists "N/A-with-reason (success; incl. intentional-gap placeholders)" under exit 0,
and SKILL.md places this case in the self-justifying set where the wrapper's own `REASON=` IS the
justification and the implementer owes Gate 2 no separate explanation. Structurally independent of
this diff — it reproduces on any run.

**Observation for `infrastructure`, recorded not actioned (pre-existing, not this devloop's).**
A fully-green run and a partially-unevaluated run both surface as `TOTAL_RESULT=N/A`, so the headline
verdict cannot distinguish them without reading the per-wrapper `STATUS=` lines. ADR-0033 line 172
shows the authors already reasoned about exactly this masking hazard when they chose
`SKIPPED-NO-DIFF` over `N/A` for the audit dep-gate — *"not N/A, which ranks above OK in
`aggregate_worst_status` and would mask a passing layer"* — but the same reasoning was not applied to
the intentional-gap placeholders, which still poison the aggregate. Filed as an observation for the
pipeline owner; it did not affect this gate because the Lead read the per-wrapper lines.

---

## Gate 2 — Re-validation before commit (Lead-run)

The Gate 2 run above predated the review fixes. The pipeline was re-run twice more as the tree
changed: once after the ten review findings landed, and finally after security's S-9. All three runs
produced the same shape — every wrapper doing real work `OK`, wrapper exit 0, nothing `FAIL` and
nothing `NOT-RUN`. Final run:

```
LAYER=1 OK  LAYER=2 OK  LAYER=3 OK  LAYER=4 N/A  LAYER=5 OK  LAYER=6 N/A  LAYER=7 OK
TOTAL_DURATION=521 TOTAL_RESULT=N/A
```

The complete set of non-`OK` statuses in the final run, enumerated so the `N/A` is not taken on trust:

```
STATUS=N/A            REASON=not-applicable-to-this-lang   (proto test.sh placeholder)
STATUS=N/A            REASON=test-aggregate-na             (layer 4 worst-child roll-up)
STATUS=N/A            REASON=layer4-summary
STATUS=N/A            REASON=not-applicable-to-this-lang   (proto audit.sh placeholder)
STATUS=SKIPPED-NO-DIFF REASON=no-dep-changes               (audit dep-manifest gate)
STATUS=N/A            REASON=audit-aggregate-na            (layer 6 worst-child roll-up)
STATUS=N/A            REASON=layer6-summary
```

Every one is a documented, self-justifying case. `cargo-test-passed`, `nx-test-passed`,
`cargo-audit-passed`, `buf-breaking-passed`, `env-tests-passed` and `browser-e2e-passed` are all `OK`.

---

## Gate 3 — Final Approval

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | **RESOLVED-DEFERRED** | 10 | 10 | 0 | Every finding against the diff fixed; two spin-outs to `infrastructure` |
| Test | RESOLVED-FIXED | 2 | 2 | 0 | Mutation-tested the S-8 negative to confirm it fails for the right reason |
| Observability | RESOLVED-FIXED | 1 | 1 | 0 | Plus 6 owner-implemented hunks across 3 of their own files |
| Code Quality | CLEAR | 0 | — | — | Wire-neutrality verified empirically (261/261 frame tests) |
| DRY | RESOLVED-FIXED | 5 | 5 | 0 | + 2 ADR-0019 extraction observations (not findings) |
| Operations | **RESOLVED-DEFERRED** | 6 | 5 | 1 | One accepted cross-owner spin-out to `media-handler` |
| Semantic Guard | RESOLVED-FIXED (native SAFE) | 1 | 1 | 0 | |

**Totals: 26 findings raised across seven reviewers, 26 fixed in this PR, 0 deferred findings.**
Plus five defects found outside the review process entirely — four by the implementer while fixing
other findings, one before review opened.

Three spin-outs are accepted and tracked (they are what make two verdicts RESOLVED-DEFERRED rather
than any finding being left in the diff): the `wrap_key_id_mismatch` rename to `protocol`, the
VBR-floor finding to `media-handler`, and the `ts_retained_credentials` predicate + guard-scope items
to `infrastructure`.

**Two reviewers landed on RESOLVED-DEFERRED**, surfaced here rather than averaged away. Neither is a
finding left unfixed in the diff — both are cross-owner spin-outs under ADR-0024 §6.3:
- `operations` — the VBR-floor finding to `media-handler`; the remedy is a design question about what
  MH pins against, in `crates/mh-service/**`, outside this changeset.
- `security` — the `ts_retained_credentials` predicate refinement and the guard-scope blindness, both
  to `infrastructure` (machinery), with policy content staying with security + observability.

### The blocker, recorded because it is the most transferable thing this devloop produced

`TransmitKeyManager.materialFor()` performed a read-modify-write across an `await` with no in-flight
guard. Two concurrent callers on a cache miss each minted a random transmit key and packed the SAME
key id — and `wrapNonce(keyId)` is a pure function of that key id, which is also the AAD. Two
AES-GCM seals, one (key, nonce) pair, different plaintexts. Reachable at every rotation boundary
(every T, every unmute, every resume-from-empty) through the fire-and-forget `void egress.submit()`.

Three facts about how it survived to review:

1. **The codebase predicted it in terms.** `receivePath.ts::TransmitKeyCache.set` carries "were that
   ever violated … it would be a (key, nonce) repeat under the KEK." The obligation was violated by
   the exact caller that comment was addressed to. The control existed, was correct, named the right
   reader — and was a comment.
2. **Seven green layers coexisted with it**, including a full Layer 7 browser E2E. Every test drove
   the racy method sequentially, so nothing in the pipeline was capable of seeing it.
3. **The loopback would have looked identical.** A self-loopback is self-consistent under a swapped
   AAD, an inverted nonce, or a signature over the wrong range — and equally self-consistent under
   two keys sharing one key id, because the sender that creates the collision is the receiver that
   resolves it. This is the concrete vindication of the task prompt's own warning that a green demo
   is not proof of crypto correctness.

Fixing it surfaced two further races in the same file, the second found only by asking whether the
fix could zero a key already in use. Security then found (S-9) that the fix's stated bound was
correct in mechanism but named the wrong quantity — rotation is event-driven, not periodic, and
`audioRotationPeriodMs` had no floor. Both fixed; the config floor is derived against the measured
read window rather than chosen for looking sensible.

### Reviewer self-corrections, recorded because the gate worked through them

- `observability` reversed their own F3 rename ruling on evidence they had not seen, and recorded the
  reviewer failure mode precisely: they cited `vectors.conformance.test.ts:407`'s comment as proof
  the assertion was deliberately decoupled, when it was proof of a **gap** — a defect misread as a
  design decision.
- `operations` argued for `played` in the receive-path identity and was shown they had the sign
  backwards; `played` is the wider claim, not the narrower one.
- The implementer self-reported their own "touches no GSA" claim before being asked.
- `dry-reviewer`'s own review-time INDEX edit reds Layer 3; the implementer declined to fix it
  because the content was theirs, and they fixed it citing their file's precedent. The ownership rule
  tested in the direction where it cost something.

---

## Post-Gate-3: the commit-boundary guard failure

Recorded because it changed the diff after every verdict was in, and because the Lead's first
framing of it was wrong in a way worth preserving rather than quietly corrected.

**What happened.** `git add -A` at commit time turned Layer 3 red:
`ts-retained-credentials-violation-found-3-of-99-files`, three `retained_credential_binding` hits on
`AudioPipeline.ts:148/168` and `MeetingSession.ts:278`. Unstaged the same guard reported
`STATUS=OK REASON=ts-no-retained-credentials-clean-78-files`. Staging changed a guard's verdict.

**The Lead's framing was overstated and both `security` and the implementer corrected it.** The Lead
wrote that this "silently weakened every guard in Layer 3" and attributed it to
`get_all_changed_files`. That is false: `get_all_changed_files` unions `git ls-files --others` and
does see untracked files; the nine guards using it were never blind. The actual cause is
`ts_retained_credentials.rs:824` calling `get_tracked_files` (`git ls-files`, tracked only). Four
guards do this — `ts_retained_credentials`, `no_insecure_browser_flags`, `release_build_profile`,
`todo_tracking` — and their blind spot is precisely **a devloop's own new files, until staged**. Two
are security controls, and `release_build_profile` is the ADR-0036 §11 control chosen to be a compile
error *because* "a control that has to notice fails silently" — it has a has-to-notice blind spot of
its own. The corrected statement is narrower and sharper, and it is the one that stays true when
someone acts on it. The full 99-file scan produced exactly three violations, all one false positive:
**the process was late-binding; the code was clean.**

**What the three hits turned out to be.** `security` first ruled them (b) — ADR-mandated retention the
name-shaped predicate cannot distinguish — and said "there is nothing to narrow." That ruling was
wrong, and they reversed it after the implementer found what neither of them had looked at:
`EgressPipeline` did `this.#signingKey = options.signingKey` at construction, so
`MeetingIdentity.clear()` released the holder's field while the egress pipeline kept an independent,
still-usable reference. ADR-0028 §5's "clear all references" defeated by a copy. Fixed by holding the
holder and reading `identity.signer` at the point of use, with a typed refusal after release, and
mutation-tested: restoring the old shape makes the test fail with
`promise resolved "undefined" instead of rejecting`.

**Residual sized precisely rather than at maximum**: `submit` short-circuits on `#stopped` both before
building and before enqueuing, so no frame could be emitted after teardown and there was no exploit
path in the shipped code. What survived was the *capability*, usable by an in-flight build after
revocation. It matters because `clear()` did not mean what it says, and the next consumer (task 20's
web app, the video pipeline) will read the method name rather than trace the copies.

**The rule that resolved it, which generalises past this case:**

> Whether a change games a guard cannot be answered from **motive** — a guard firing is a legitimate
> prompt. The test is whether the change has a **property independent of the guard**.

The refusal earlier in this devloop had nothing to point at but the green (retained material and
lifetime identical either side; only an identifier moved). This one has three independent properties —
retained state changed, behaviour under `clear()` changed, and a mutation test discriminates — and the
green Layer 3 is a fourth thing that is still not evidence.

**Process note.** The implementer volunteered the one fact they had every incentive to omit — *"I went
looking BECAUSE the guard fired, and I am not going to dress that up as independent discovery"* — and
offered to revert and leave Layer 3 red, blocking the gate, rather than risk establishing the
precedent security had spent the day refusing. Security cleared it in one pass instead of three
because the uncomfortable fact was volunteered rather than reconstructed.

**Still owed, not closed by this commit**: the `ts_retained_credentials` predicate refinement (the
type-shaped discriminator — a non-extractable `CryptoKey` has no bytes to retain) and the
four-guard unstaged-file blindness, both filed under `docs/TODO.md` §Media Path Obligations. The next
legitimate holder of a non-extractable key in a matching field will fire again with no adjacent defect
to find.
