# Devloop Output: MH Datagram Receive-Path Gap (story task 26, R-15)

**Date**: 2026-09-08
**Task**: Diagnose and fix the MH datagram receive-path gap — client-sent audio datagrams produce no forwarded frames AND no drop counts; close R-15 by un-`#[ignore]`-ing `26_mh_quic.rs::test_mh_forwards_an_audio_datagram_back_to_its_sender`
**Specialist**: media-handler
**Mode**: Agent Teams (v2) — full, HEADLESS RUN (run-story task #26)
**Branch**: `feature/hear-yourself-through-handler`
**Duration**: TBD

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `d090c753480fdd335580f9d2a231a085d81ab125` |
| Branch | `feature/hear-yourself-through-handler` |
| Lead Model | `claude-opus-5` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` (resumed and committed 2026-09-09) |
| Implementer | `implementer` (media-handler, spawned) |
| Implementing Specialist | `media-handler` |
| Iteration | `1` |
| Security | `security` (spawned) |
| Test | `test` (spawned) |
| Observability | `observability` (spawned) |
| Code Quality | `code-reviewer` (spawned) |
| DRY | `dry-reviewer` (spawned) |
| Operations | `operations` (spawned) |
| Semantic Guard | `semantic-guard` (spawned) |

---

## Task Overview

### Objective

Verbatim task description (from `/tmp/devloop/story-runner/2026-08-27-hear-yourself-through-handler/task-26.prompt`):

> Diagnose and fix the MH datagram receive-path gap found at the task-24 escalation (third session, docs/devloop-outputs/2026-09-05-sender-id-binding-contract/main.md): with the participant bound, the media session started (mh_media_session_starts_total{outcome=started} incremented), and a forwarding policy installed, a client-sent audio datagram produces NO effect — mh_media_frames_forwarded_total is 0 AND every mh_media_frames_dropped_total{reason} series is 0 on both handler instances, so datagrams are not reaching the routing lookup at all; a frame arriving at a handler with no matching edge should at minimum count a no-route drop. The gap is upstream of edge matching, somewhere between the QUIC connection's datagram receive and the media ingress loop wired in story task 16 (crates/mh-service/src/media/ingress.rs, the transport trait's receive path from tasks 5 and 12, and the session-start handoff in webtransport/connection.rs / session/). Candidate causes to check rather than assume: the ingress loop not being spawned for a bound connection, the datagram receive side of the WtMediaTransport impl never being polled, max_datagram_frame_size negotiation, or the receive loop reading from a different connection handle than the one the client sends on. Diagnose first on the live Kind cluster with the counters and logs; fix in MH; add the missing observable if one exists (a frame that arrives on a connection with no started media session, or before the ingress loop is live, must be counted, not silently dropped — the story's drop-by-reason discipline). DEFINITION OF DONE: delete the #[ignore] on crates/env-tests/tests/26_mh_quic.rs::test_mh_forwards_an_audio_datagram_back_to_its_sender and the test passes against the live cluster — this closes story requirement R-15 and the docs/TODO.md §Media Path Obligations R-15 entry per its four-part closure condition. Depends on task 25 (client steering to the edge-bearing handler) so the test exercises the full corrected path. Zero-copy/no-macro/§11 constraints of the media directory unchanged; pair with test for the env-test fixture.

### Scope
- **Service(s)**: mh-service (primary); env-tests fixture
- **Schema**: No
- **Cross-cutting**: Observability (new drop-reason observable), Test (env-test fixture)

### Debate Decision
NOT NEEDED — diagnosis + fix within existing ADR-0036 §2/§7/§11 boundaries.

---

## Diagnosis

**Verdict: there is no MH datagram receive-path defect. The task's premise is falsified by
direct experiment on the live Kind cluster.** What the task calls a "gap" was an inference
drawn from a metric reading taken over a pod lifetime in which no client ever sent a datagram.
A different, real, previously-unnamed observability hole *was* found while establishing that —
see §The missing observable DOES exist — and it is not the one the escalation predicted.

All readings are from cluster `devloop-hear-yourself-through-handler` at commit `d090c753`,
pods `mh-0-56bcfcd4bc-qk6wj` (`192.168.4.79`) and `mh-1-7887d89f9f-bbrfx` (`192.168.4.119`),
scraped from each pod's own `:8083/metrics` — immediate, not through Prometheus's ≤15 s lag.

### E1 — MH source is byte-identical to the tree the escalation was written against

```
$ git diff --stat cc56d7fc..d090c753 -- crates/mh-service/
(empty)
```

`cc56d7fc` is task 24's commit, `d090c753` task 25's. Task 25 changed MC and the env-test
fixture only. **Whatever changed between "R-15 fails" and "R-15 passes", it was not MH.**

### E2 — The `#[ignore]`d R-15 test passes against the live cluster, unmodified, 3 of 3 runs

```
$ ENV_TEST_ORG_SUBDOMAIN=devtest cargo test -p env-tests --features all --test 26_mh_quic \
    -- --ignored --exact test_mh_forwards_an_audio_datagram_back_to_its_sender
test test_mh_forwards_an_audio_datagram_back_to_its_sender ... ok   (18.39s)
test test_mh_forwards_an_audio_datagram_back_to_its_sender ... ok   (28.38s)
test test_mh_forwards_an_audio_datagram_back_to_its_sender ... ok   (30.38s)
```

Non-vacuous by construction: the test loops until a datagram returns or panics at a 15 s
deadline, then asserts on the returned bytes — `stream_id == 0` (rewritten off the `0xFFFF`
publisher sentinel to the subscriber's own slot), `hop_sequence != 0xDEAD_BEEF`, and
`publisher_region` / `payload` / `signature` byte-identical. Confirmed on the counters: mh-0
moved `mh_media_frames_forwarded_total{direction="ingress"}` 0 → 1 and `{direction="egress"}`
0 → 1. One frame in, one frame out.

### E3 — Each of the four named candidate causes, disproven by measurement

A throwaway probe (`crates/env-tests/tests/zz_diag_task26.rs`, written for diagnosis and
deleted — it is not in the diff) drove a real GC→MC join, declared a `ReceiveCapability`, read
MC's `SendDirective`, and sent datagrams to a handler of its choosing.

| Candidate cause | Verdict | Evidence |
|---|---|---|
| Ingress loop not spawned for a bound connection | **DISPROVEN** | 40 datagrams to the **non-steered** handler mh-1 → `mh_media_frames_forwarded_total{direction="ingress"}` **+40** on mh-1. The loop is spawned and reading. |
| `WtMediaTransport`'s receive side never polled | **DISPROVEN** | Same reading: 40 datagrams travelled `recv_datagram()` → size cap → ingress ring → `forward_one`. |
| `max_datagram_frame_size` negotiation | **DISPROVEN** | Client reports `conn.max_datagram_size() == Some(1413)`; 60 of 60 `send_datagram()` calls returned `Ok`, zero `TooLarge` / `UnsupportedByPeer`. |
| Receive loop on a different connection handle than the client sends on | **DISPROVEN** | E2's round trip returns on the same `wtransport::Connection` the client sent on. |

### E4 — A frame with no matching edge IS counted, exactly as the task says it "should at minimum"

The 40 datagrams to the non-placed handler mh-1 produced, on mh-1 and nowhere else:

```
mh_media_frames_forwarded_total{direction="ingress",key_custody="operator"}                     40
mh_media_frames_dropped_total{reason="no_subscriber",direction="egress",key_custody="operator"} 40
```

That is `forward_one`'s `visited == 0` branch (`crates/mh-service/src/media/forward.rs`)
behaving as written: it increments `forwarded{ingress}` so the ingress-attempts identity stays
whole, then attributes the drop to `no_subscriber` because a policy *is* installed and no edge
names this sender. **The no-route drop the task says is missing has been there all along.**

### E5 — Why the escalation read zeros: no datagram was ever sent

The third session's own record states it (`docs/devloop-outputs/2026-09-05-sender-id-binding-contract/main.md`
§Resume): *"No pipeline run was made. Layer 7 remains EXHAUSTED at 2/2 and no gate was
re-attempted. Everything below is read-only diagnosis: unit tests, source reading, `kubectl
logs`, and **one Prometheus query**."*

The R-15 test was `#[ignore]`d for the whole lifetime of the pods being queried, and it is the
**only** thing in the tree that sends a media datagram. Every other `26_mh_quic.rs` scenario
opens a WebTransport connection, gets a media session started, and never sends a frame.

So `mh_media_session_starts_total{outcome="started"}` climbing while every frame series sits at
0 is **exactly the expected shape** of a suite that starts sessions and sends no media. Live
confirmation on this cluster: `started` = 13 on mh-0 and 10 on mh-1 against 50 and 40 ingress
frames — the great majority of started sessions contribute zero frames, permanently.

The escalation's inference — *"so datagrams are not reaching the routing lookup at all"* —
required that a datagram had been sent. None had. Same failure mode that document's §Lessons
Learned records four times over: a conclusion generalised from a reading whose precondition was
never checked.

### E6 — The missing observable DOES exist, and it is a different hole

The task's conditional clause ("add the missing observable **if one exists**") resolves to
**YES**, on evidence, in the window the task's own wording names: *"a frame that arrives on a
connection with no started media session, or before the ingress loop is live"*.

**Measurement.** One connection to the steered handler mh-0. 20 datagrams sent **before the
connect envelope was even written** — so before `validate_meeting_token`, before
`resolve_sender_binding`, before any loop existed — then the JWT frame, then 40 more after the
session started. All 60 sends returned `Ok`:

```
mh-0 before:  forwarded{ingress} 3   forwarded{egress} 3
mh-0 after:   forwarded{ingress} 50  forwarded{egress} 50      →  +47
probe:        presession_sent=20 presession_err=None  send_ok=40 received_back=40
```

**60 sent, 47 counted. 13 datagrams vanished with no counter and no log line anywhere in MH.**
The ~7 that survived match the buffer arithmetic: `DATAGRAM_RECEIVE_BUFFER_BYTES` is 2 KiB and
the fixture frame is ~236 B, so ≈8 fit before quinn starts evicting.

**Mechanism, read from source rather than assumed** (`quinn-proto-0.11.17`, the version in
`Cargo.lock`): `connection/datagrams.rs:133-136` evicts the oldest queued datagram to make
room — `while self.incoming.memory_used() + size_with_overhead > window { debug!("dropping
stale datagram"); self.recv(); }` — a `debug!` and nothing else. No counter MH can read, no
error surfaced to the application.

**But it is NOT structurally invisible, which is the part that matters.**
`connection/mod.rs:2733` calls `self.stats.frame_rx.record(&frame)` on **every** decoded frame,
ten lines **before** the datagram reaches the accept/evict decision at `:2743`. So
`quinn::Connection::stats().frame_rx.datagram` counts every DATAGRAM frame the connection ever
received, evicted ones included. `wtransport::Connection::quic_connection()` is already
compiled in for MH — the `wtransport/quinn` feature is on for `with_custom_transport`, and
`crates/mh-service/Cargo.toml:23-27` already records that accessor as a known side effect.

Had that observable existed, the task-24 escalation would have had a signal distinguishing
"datagrams never arrived" (client-side) from "datagrams arrived and MH threw them away"
(MH-side) — the exact discrimination three sessions could not make.

---

## Cross-Boundary Classification

No Guarded Shared Area is touched: the diff reaches neither `crates/media-protocol/**`,
`proto/**`, nor `crates/common/src/webtransport/**`. Every row is either mine or an
owner-implemented edit in another specialist's artifact.

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `crates/mh-service/src/observability/metrics.rs` | Mine | — |
| `crates/mh-service/src/media/ingress.rs` | Mine | — |
| `crates/mh-service/src/webtransport/connection.rs` | Mine | — |
| `crates/mh-service/tests/media_session_binding_integration.rs` | Mine (paired with @test) | — |
| `crates/env-tests/tests/26_mh_quic.rs` | Minor-judgment / Not mine | test |
| `infra/services/mh-service/configmap.yaml` | Owner-implements / Not mine | operations |
| `crates/common/src/observability/testing.rs` | Owner-implements / Not mine (documentation only) | observability |
| `docs/observability/metrics/mh-service.md` | Owner-implements / Not mine | observability |
| `infra/grafana/dashboards/mh-overview.json` | Owner-implements / Not mine | observability |
| `infra/grafana/dashboards/mh-slos.json` | Owner-implements / Not mine | observability |
| `docs/observability/slos.md` | Owner-implements / Not mine | observability |
| `infra/docker/prometheus/rules/mh-alerts.yaml` | Owner-implements / Not mine | operations |
| `docs/runbooks/mh-incident-response.md` | Owner-implements / Not mine | operations |
| `docs/TODO.md` | **Shared filing surface, per-entry authorship** — mine, observability, dry-reviewer, operations | see §Files Modified for which entry is whose |
| `docs/devloop-outputs/2026-09-08-mh-datagram-receive-path-gap/main.md` | Mine | — |

**Two rows in the planned table were NOT touched, and one row was added — recorded rather than
silently reconciled, because Gate 2's scope-drift check flags both directions.**

- `crates/mh-service/tests/media_metrics_integration.rs` — **planned, not touched.** The
  `MediaDropReason::ALL` iteration and the sixteen-token cross-language collision test are both
  written to derive their token list from the enum, so extending `ALL` from 11 to 13 extended
  their coverage with no edit. Verified by running them, not by assuming: the full
  `cargo test -p mh-service` suite is green with both new tokens. That the file needed no change
  is the design working — a hand-written token list there would have needed one.
- `crates/mh-service/tests/webtransport_integration.rs` — **planned, not touched.** The two
  firing-path tests moved to `media_session_binding_integration.rs`, which already owns the
  `AcceptLoopRig`, `MockMcServer` with its decline arms and `MetricAssertion`. Putting them in
  `webtransport_integration.rs` would have meant re-standing the decline mock beside an existing
  copy. Agreed with @test before implementation.
- `crates/common/src/observability/testing.rs` — **touched, not planned, and the classification
  needed care.** @observability's rustdoc on `CounterQuery::with_labels`, documenting the
  first-match-not-aggregate behaviour that produced my trivially-green non-vacuity check.
  **Verified documentation only**: `git diff -U0` filtered to non-comment lines is EMPTY — 21
  added lines, all rustdoc, no behaviour, no signature, no call-site semantics. `crates/common/**`
  outside the Guarded subset has no single owning specialist and requires **@dry-reviewer +
  @code-reviewer** approval, with affected-specialist involvement review-only while call-site
  semantics are unchanged — which they are. Not in the Guarded subset
  (`jwt.rs`, `meeting_token.rs`, `token_manager.rs`, `secret.rs`, `webtransport/**`), so
  Mechanical is not barred on surface grounds — but **deliberately not classified Mechanical**: a
  rustdoc that encodes a *policy about how to use a helper* is not sed-clean. Flagged to both
  approvers rather than letting either meet it first in the diff.
- `infra/services/mh-service/configmap.yaml` — **touched, not planned.** @operations' own
  `MH_KEEPALIVE_INTERVAL_MS` comment fix, which they made under their ownership and told me
  about; comment-only, value unchanged. It is theirs, not mine, and it gets a row because the
  scope-drift check reads the diff and not the ownership.


---

## Planning

### Gate 1 — Plan Confirmation Tracking

| Reviewer | Plan Status |
|----------|-------------|
| Security | **confirmed** (rev 2) — SEC-1..SEC-5 all resolved |
| Test | **confirmed** — both preconditions met |
| Observability | **confirmed** — wtransport 0.7.2 verified positive |
| Code Quality | **confirmed** (re-confirmed for rev 2) |
| DRY | **confirmed** — F1 closed (integer deleted, not updated) |
| Operations | **confirmed** — OPS-F1 adopted, OPS-F2 fixed not filed |
| Semantic Guard | **confirmed** |

**Gate 1 CLOSED** — all seven confirmed; `validate-cross-boundary-classification.sh` returned
`STATUS=OK REASON=cross-boundary-classification-clean-1-files`; "Plan approved" issued.

### Lead Rulings (planning round 1)

1. **The task's premise is falsified and the devloop proceeds anyway.** Live-cluster diagnosis
   disproved all four named candidate causes by measurement, and the `#[ignore]`d R-15 env-test
   passes unmodified 3/3. That is a successful task-26, not a blocked one — the task said
   "candidate causes to check rather than assume."
2. **The task description and story manifest are NOT amended.** ADR-0035 §3/§4 reserve task status
   and the manifest to the runner; the task prompt is an immutable input to this run. The
   falsification is recorded in §Diagnosis and carried honestly into the commit message.
3. **The DoD is unchanged and binding** — the `#[ignore]` comes off and the test passes against the
   live cluster. That it already passes makes the DoD cheap, not void.
4. **The conditional clause "add the missing observable if one exists" resolves to YES**, for a
   different hole than predicted: datagrams evicted from quinn's receive buffer with only a
   `debug!`. Its existence is settled; its *shape* is open pending @security SEC-1.
5. **F1 fixed, not deferred** — the restated cardinality integer is deleted rather than updated.
6. **D3 (the three homes of the misdirecting "client-side, not MH" triage claim) is not optional.**
7. **The `crates/mh-service/src/media/ingress.rs` deviation is APPROVED as a deliberate, reviewed
   departure from the task's wording.** The task says "§11 constraints of the media directory
   unchanged." @security, @code-reviewer and @dry-reviewer independently read that as a constraint
   on the *invariants*, not a ban on the directory appearing in the diff, and the Lead agrees:
   rev-1's sample-at-session-start was arithmetically wrong (SEC-1), and a loop-local `u64` is the
   shape correctness forces. Every §11 invariant holds — zero allocation, no macro form, no
   registry lookup, no clock read, and no per-participant dimension escaping the directory, the
   last now enforced by a no-`Debug`/no-`Display` newtype (SEC-4) rather than by reviewer
   vigilance. This is a stronger position than the literal reading would have produced.
8. **@operations' forensic-not-detective ceiling is ACCEPTED and recorded, not buried.**
   `transport_receive_dropped` moves at connection close, so MH still has no signal that detects
   silent ingress media loss *while a meeting is running*. An exact forensic counter beats
   nothing and makes a real defect visible for the first time; the live variant is a scoped
   follow-on named in @operations' verdict. A reader of this commit must not mistake it for
   "MH can now see this as it happens".

### Retractions during planning (the panel working)

Three load-bearing claims were asserted, checked by a peer against the mechanism, and withdrawn in
those words: @implementer's rev-1 "strengthens the ingress-attempts identity" (inverted — it
double-counted); @dry-reviewer's "the sample is independent of every other series" (wrong — quinn
evicts drop-oldest, so survivors are re-counted); and @observability's own identity claim in the
file that is SSoT for what these metrics mean. Each was caught by reading the source rather than
accepting the report — the same discipline whose absence funded this task.

### Plan

#### Restating the problem in mechanism-language

The task is written in instance-language: *"the MH datagram receive-path gap"*. Restated as a
mechanism, what the diagnosis found is:

> **MH counts datagrams only from the moment its per-connection ingress loop exists. Every
> datagram the QUIC connection receives outside that loop's lifetime is discarded by quinn's
> receive-buffer eviction with no MH-side signal — so "MH received nothing" and "MH received it
> and threw it away" are the same reading.**

The wider class that restatement produces is **every window in a connection's life in which
media can arrive but no MH reader exists**: before the JWT gate, during provisional meeting
registration, during the MC `NotifyParticipantConnected` round trip, on a declined session
before the close lands, and after media teardown. Same mechanism, same owner (MH), wider than
the single instance the task names. Surfaced here per the planning instruction; the scope
decision (cover the pre-session window now, file the post-teardown tail) is argued at the end
rather than assumed.

#### What is NOT being changed

**No MH behavioural change — there is no defect to fix.** `crates/mh-service/src/media/**` is
untouched: no macro is added under it, the hot path acquires no work, no queue bound or size
cap moves, `for_each_source` keeps its `MeetingKey`, `forward_one` gains no fallback, and no
"echo back to sender" path is introduced (@security S2/S4). The transport seam's contract holds
— `WtMediaTransport` gains no metric handle and no constructor argument (@observability O3).

#### D1 — Close R-15 (the stated DoD)

Delete the `#[ignore]` on
`crates/env-tests/tests/26_mh_quic.rs::test_mh_forwards_an_audio_datagram_back_to_its_sender`
and rewrite its doc comment, which currently asserts as fact a defect that does not exist.
**No assertion is weakened and no helper is added** — the three-step gate, the STEP 2 positive
control and all five content assertions stand exactly as written (@test #4, @dry-reviewer #3).
Evidence of a real pass is §Diagnosis E2.

Then delete the `docs/TODO.md` R-15 entry per its own four-part closure condition: contract
field landed (task 24), `bind()` has a production caller (task 24), `#[ignore]` removed and the
test passes (here).

#### D2 — Add the missing observable

A twelfth `MediaDropReason`, **`no_media_session`** (`direction = ingress`): *datagrams this
QUIC connection received before its media session existed, which no MH reader ever saw.*

**Where it is counted — @security S1 answered directly.** In
`webtransport/connection.rs::start_media_session`, which runs **strictly after**
`jwt_validator.validate_meeting_token(&token)` (connection.rs:258) **and strictly after**
`resolve_sender_binding` returns `Bound`. It reads
`connection.quic_connection().stats().frame_rx.datagram` — a number quinn already maintains for
its own accounting. **No I/O, no datagram read, no loop spawned, no allocation, no pre-auth
work path.** There is no counting-only receive loop; the value is handed over, sampled once per
connection. A second sample sits in the `Declined` branch before `close_declined_connection`,
so a client pushing media into a connection MH is about to close is counted rather than
silently discarded — same helper, same post-gate position (@operations #2).

**Why a drop reason and not a session-start outcome (@observability O2).** It counts
*datagrams*, not connections; the decline itself is already counted on
`mh_media_session_starts_total`. It **strengthens** the ingress-attempts identity rather than
corrupting it: `forwarded{ingress} + dropped{ingress reasons}` moves from "every datagram the
ingress loop read" to "every datagram the connection received that MH can account for". The
catalog entry will say so explicitly, since a reader reconciling the two would otherwise think
it broke.

**Vocabulary (@dry-reviewer #1).** Distinct from all 11 MH tokens and all 8 codec tokens, and
not a respelling: `no_policy` = the control plane never programmed this handler;
`no_subscriber` = programmed, but no edge names this sender; `no_local_subscriber` = an edge
names a subscriber not connected here. **All three presuppose a live loop that read the frame.**
`no_media_session` is the case where there was no loop. Not a crypto/key-layer token.
Mechanical consequences: `ALL: [Self; 11]` → `[Self; 12]`; `dropped: [Counter; 11]` →
`[Counter; 12]` plus its destructuring arm; wildcard-free `as_str`; exactly one `direction()`
arm. `docs/observability/metrics/mh-service.md:496` "Cardinality: Low (19 = 11 MH-local + 8
codec)" becomes `20 = 12 + 8` — a hand-restated integer nothing guards (@observability O9).

**No new constant, no config change (@dry-reviewer #2, @operations #3).**
`DATAGRAM_RECEIVE_BUFFER_BYTES` does not move, no env var is added, no manifest changes, no
transport parameter is renegotiated. **Rollback shape is @operations' case (a): the image rolls
back alone, cleanly, no ordering constraint, no client-visible protocol change.**

**Firing path is real, not dead machinery (@test #2, the `StreamRateLimited` lesson).** It
already fired 13 times on the live cluster in §Diagnosis E6, before a counter existed to record
it.

**Steady-state claim, stated explicitly for @operations #1 and @observability O2.** Zero
forever *for a correct client*: a client must not send media before MC's `SendDirective`, and
MH's session starts within milliseconds of the connect envelope, so an occurrence is the signal
— the `MHMediaSenderBindingOutOfRange` shape. **Catalog group: should-read-zero
invariant-violation.** Honest caveat that belongs in the threshold rather than in the group: a
client reconnecting to a handler it already holds a directive for can legitimately race the new
connection's setup, so I recommend a small `for:` window rather than a single-sample trigger.
@operations owns the rule and the severity; @observability owns the group classification.

**Runbook split (@operations follow-up #2).** @operations writes the incident-response scenario
in `docs/runbooks/mh-incident-response.md` **in this same change**, and I supply the three
inputs: counter `mh_media_frames_dropped_total{reason="no_media_session",direction="ingress"}`;
zero-forever claim as stated above; first diagnostic move — *check whether
`mh_media_frames_forwarded_total{direction="ingress"}` on the same instance is also flat: if
`no_media_session` is rising and ingress-forwarded is flat, a client is publishing before MH is
ready to forward (client/steering timing); if both are rising, media is flowing and only the
connection-setup race is being counted.* No number is referenced from any other file; anything
pointing at it does so by title.

#### D3 — Correct the three homes of the falsified triage claim (@observability O1)

`metrics.rs:354-358` (`MediaSessionStartOutcome::Started`'s docstring, mine),
`docs/observability/metrics/mh-service.md:368` + `:407-412`, and
`infra/grafana/dashboards/mh-overview.json:2167` all assert that `started` climbing with flat
ingress frames is *"a client-side condition rather than an MH one"*. §Diagnosis E5 shows why
that misdirected three sessions: on this tree that shape is produced by any connection which
starts a session and sends nothing — which is most of them — and it says nothing about which
side is at fault. Corrected claim: `started` proves the loops were *spawned*, which is upstream
of the loop *receiving* anything; `mh_media_frames_dropped_total{reason="no_media_session"}` is
the new discriminator, non-zero only if datagrams actually arrived.

@observability offered to author O1 and O9 (the six stale "pending the sender_id binding / not
wired" caveats) as owner-implemented edits. **Accepted for the five files in their artifacts**;
I take `metrics.rs:354-358`, which is mine.

#### D4 — Tests

- **Regression pin for D2 (@test #1)**, in `crates/mh-service/tests/webtransport_integration.rs`,
  which already drives the real accept loop over a real `wtransport` connection: a client sends
  N datagrams **before** writing the connect envelope, then completes the handshake; assert
  `mh_media_frames_dropped_total{reason="no_media_session"}` rose by at least 1. It exercises
  the real quinn `frame_rx` path rather than a double, and it reds on today's tree (the token
  does not exist).
- `crates/mh-service/tests/media_metrics_integration.rs`: the 16-token cross-language collision
  test and the `MediaDropReason::ALL` iteration stay green with the twelfth token; the in-crate
  macro-deny walker stays green (nothing under `media/` changes).
- **Honest limitation, stated rather than papered over (@test #1):** there is no unit-tier pin
  for "the receive path works end to end", because there is no defect to pin. The pin is the
  now-running env-test plus the existing component-tier suites; the mechanism that actually
  unblocked R-15 lives in MC and carries task 25's own component-tier inversion test.

#### Answers to the remaining pre-load items

- **@security S3** — bounded `&'static str`; no label derived from the datagram, the connect
  envelope, the peer address, or any meeting/participant id. **S5** — the env-test's token comes
  from the existing `AuthClient` → GC join path; loopback is asserted only via an installed
  policy edge; no token or payload is printed. A wholesale-empty Prometheus reading is
  structurally red because STEP 2 (`poll_until_exactly_one_instance_rose`) cannot be satisfied
  by an absent series — it is the test's positive control.
- **@semantic-guard** — every exit path in the receive → ingress → routing chain records a
  counter (E4 verified this on the cluster, not by reading); no new `.map_err` discards context;
  no actor `select!` loop is touched; MH parses no wrapped-key field, so credential-leak items
  11-13 do not fire.
- **@operations #5** — no topology is parsed in Rust; MH reads a quinn accounting value.
- **@dry-reviewer #3/#4/#5** — no new env-test helper; `validate_16bit_id()` is not created;
  nothing is hoisted to `crates/common/`.
- **@code-reviewer** — table above has a row for every file; §11 zero-copy/no-macro discipline
  unchanged; no GSA path touched.

#### Open question for reviewers (the wider class from the restatement)

The **post-teardown / mid-session tail** is the same mechanism and is *also* measurable, by
differencing `frame_rx.datagram` at teardown against a count of frames the ingress loop read.
I propose to **file it rather than build it**, on three grounds: (a) it has a legitimately
non-zero steady state — a datagram in flight when a client hangs up — so it is not alertable
the way D2's counter is; (b) it needs a per-frame counter in the ingress loop, which is hot-path
cost the task explicitly says to leave unchanged; (c) it is not what the task asks for. It would
be filed in `docs/TODO.md` naming the accessor and the trade-off, so the next author inherits
the finding rather than re-deriving it. **Push back if you disagree.**


---

## Planning — REVISION 2 (after Gate 1 findings; supersedes D2 above)

Five reviewers converged on the same defect in D2 as originally written, from four directions.
**They are right and D2 rev-1 is withdrawn.** @security SEC-1 and @observability F1 are the same
finding, and my own §Diagnosis E6 data proves it: sampling `frame_rx.datagram` at
`start_media_session` counts the union of (a) frames quinn evicted and (b) frames still buffered
that `run_ingress` reads moments later and counts again as `forwarded{ingress}`. On the E6 run
that is 20 recorded against 13 truly lost, with 7 frames in both addends — so rev-1's claim that
it "strengthens the ingress-attempts identity" was **inverted**. It would have been the first
token in the family able to count a frame that was also forwarded, which is exactly the
denominator corruption `mh-service.md:414` forbids.

### D2′ — Two tokens, counted exactly at teardown

**Adopting @observability's Resolution A, split per @operations' OPS-F1.**

**Token 1 — `transport_receive_dropped`, `direction = ingress`.** For connections that *had* a
media session. `run_ingress` keeps a loop-local `u64 frames_read`, incremented once per datagram
it takes off the transport, and returns it alongside `LoopExit` — the shape `media/ingress.rs`'s
own module doc already establishes ("These loops RETURN; the caller emits"). At teardown, and
**after** the ingress task has been awaited, `webtransport/connection.rs` reads
`frame_rx.datagram` once and increments the token by `total_rx.saturating_sub(frames_read)`.

Why this is the right shape and not merely a fix:
- **Exact, with no overlap by construction**, and — per **@security SEC-5** — stated with its
  residue rather than as an equality, because I have just been bitten once by a catalog entry
  asserting an identity that did not hold. `run_forward`'s `select!` returns on cancellation
  **without draining**, so frames still in the ingress ring at teardown were counted in
  `frames_read` (hence excluded from the new drop counter) but were never forwarded and never
  dropped. The claim is therefore: `forwarded{ingress} + dropped{ingress reasons}` = every
  DATAGRAM frame the connection received, **up to a bounded per-connection residue of at most
  `INGRESS_QUEUE_FRAMES` frames left in the ring at cancellation** — cited by NAME, never
  restated as an integer, in the catalog, the runbook heading or the alert annotation
  (@security's binding condition: a hand-restated bound drifts silently the moment the queue
  is tuned, and it drifts toward UNDERSTATING the residue, so the next reader hits a
  discrepancy the doc told them was impossible — same defect class as the `:496` cardinality
  line, and CLAUDE.md's SSoT convention). Still strictly stronger
  than rev-1's "every one the loop read", and the residue is small, bounded and documented so
  the first person to reconcile the series finds it rather than opens an incident.
- **It absorbs all three windows in one number** — pre-session arrivals, quinn's mid-session
  eviction, and the post-teardown tail — so the deferral I proposed in rev-1 dissolves, and
  @test's Q3 gap (the sliver between session-start returning and the loop actually draining)
  disappears with it: a frame in that sliver is either read by the loop (counted there) or shed
  (counted here).
- **Provenance, stated plainly for @test Q2: quinn exposes NO evicted-datagram count.** The
  only native signal is the `debug!("dropping stale datagram")` at
  `quinn-proto-0.11.17/src/connection/datagrams.rs:133-136`, which ADR-0036 §11 rules out. What
  I read is `frame_rx.datagram` — every DATAGRAM frame decoded off the wire — and I difference it
  against a count MH itself keeps. **Nothing is inferred from buffer-size arithmetic.**
- **Naming: `transport_receive_dropped`.** @observability proposed `datagram_unread` and said
  they were indifferent to the word ("I care about the split, not the word"); @operations had
  already confirmed against `transport_receive_dropped` and authored their alert expression and
  runbook heading on it, asking to be pinged before any rename. Resolved to
  `transport_receive_dropped` on merits plus cost: it is the ingress-side mirror of the existing
  egress-side `transport_send_refused` — same `transport_*` prefix for "the layer below us",
  opposite direction — which makes the pair legible in the catalog, and renaming would invalidate
  committed work in two files for no gain. **@observability owns the label space and can overrule
  this; flagged to them explicitly rather than quietly kept.** What survives the naming question
  either way is their honesty requirement, which is the substance: under Resolution A this is a
  **union of three windows** —
  pre-session arrivals, mid-session eviction while the loop was behind, and post-teardown
  arrivals — and the third of those was never "dropped by the transport", MH simply never read
  it. Not a respelling of `ingress_queue_overflow`, which is MH's *own* ring. **The catalog entry
  must say the union is a union and that the pre-session window is expected to dominate**
  (@observability's honesty requirement): a reader who assumes it is purely pre-session will
  mis-triage the day it is not, and MH cannot split the three without more sampling.

- **@security SEC-4 — `frames_read` must never reach a log line, and it is made structurally
  impossible rather than remembered.** At ~50 frames/s a per-connection count of published media
  frames IS talk duration for that participant — the same *kind* of value as the per-frame sizes
  `media/ingress.rs`'s module doc bars from crossing out of the directory, and the macro-deny
  walker structurally cannot see a sibling logging a value that came OUT of `media/`. The hazard
  is concrete: widening only `run_ingress`'s return type breaks the homogeneous
  `[("ingress", …), ("forward", …), ("egress", …)]` array at `connection.rs:494-521`, forcing a
  hand-written ingress arm — and `connection.rs:555-562` emits `connection_id` alongside
  `participant_id`, so a `frames_read` field on the ingress arm would be joinable to a named
  participant. Remedy, taking @security's optional stronger form: the count is returned as a
  **newtype implementing neither `Display` nor `Debug`**, with a private inner field and — after
  @semantic-guard's Q2 — **no accessor that returns the count at all**. Its only method is
  `unread_since(self, received: u64) -> u64`, which consumes the value and returns the
  *difference*. So `debug!(?frames_read)` and `debug!(%frames_read)` are compile errors, and the
  determined path @semantic-guard flagged as the residual under a `get()` — `debug!(x.get())` —
  does not exist either: a sibling cannot obtain the count, only the residual, which is
  near-zero in health and is not a talk-duration proxy. The count never leaves the module. Same
  remedy `IngressFrame`'s hand-rolled `Debug` already uses. The existing ingress log statement
  stays byte-identical (LoopExit only), the delta is computed in a separate macro-free
  expression, and a comment at the ingress arm cites the module doc's second-route argument for
  the next author who restructures that array.
- **Failure mode.** If the ingress task join-errors, `frames_read` is unavailable and **no count
  is emitted** — fail closed to silence rather than publish a wrong number.
- **Documented limitation:** the signal only appears when a connection closes, so a long-lived
  connection bleeding frames stays invisible until it ends. This goes in the catalog entry.

**@operations' forensic-vs-detective ceiling — ACCEPTED AND RECORDED, not argued away.** Because
the delta is only computable once `frames_read` is final, this counter is **forensic**: during a
live incident nothing increments until sessions close, and meetings run for an hour. It therefore
does NOT close @operations' original gap ("no alert detects silent media loss while it is
happening"), and neither the catalog, the dashboard nor the runbook may imply that it does. I
looked for a cheap live variant and there is not one: making the count readable mid-session needs
either a shared atomic plus a periodic sampler task per connection, or a tick in the receive
loop's `select!` — both are real per-connection structure, not a marginal increment, and both are
task-sized. That is filed with the D6 entry rather than asserted as impossible. The nearest
*existing* detective signal remains `forwarded{ingress}` failing to track the expected publisher
frame rate on the steered instance, which is what D3's corrected triage row now points a
responder at.

**Token 2 — `no_media_session`, `direction = ingress`.** For connections whose media session was
**declined**. Sampled in the `Declined` branch before `close_declined_connection`, as plain
`frame_rx.datagram` — which is **exact** here, because no ingress loop is ever spawned, so every
one of those datagrams is discarded.

@operations' OPS-F1 is why this is a separate token rather than a second sample under token 1,
and the argument is decisive: `config.rs:429` records a **~48 s** MC-client retry budget, so
during an MC outage each declined client contributes on the order of 48 s × 50 frames/s before
its close lands. Across a reconnect herd — which `MC_UNAVAILABLE_CLOSE_JITTER_MAX_MS` exists
because we expect — that is thousands of counts per incident. Sharing a token would make token 1
unreadable and would fire a second page that merely restates `MHMediaSessionDeclineRate` at the
moment on-call is already saturated. Token 2 is **diagnostic-only, no alert**, fully explained
one row up by `mh_media_session_starts_total{outcome=declined_*}`, and the catalog says so.

**Group classification: `saturation-or-input`, both tokens.** @observability's F2 ruling is
binding and **reverses my rev-1 recommendation of should-read-zero.** Their criterion is not "is
it zero in a conformant fleet" but "does non-zero mean *we* have a bug", and every producer here
is input- or timing-driven. @operations: the alert must therefore be a rate over a sustained
window, not a `> 0` trigger with a short `for:`. There is a second, independent reason (below).

**@security SEC-3 — the value is client-influenced, and this is stated in the catalog.**
Mechanism re-verified against the **pinned** version, not a neighbouring one, after @observability
flagged that they had read 0.7.1 while `Cargo.lock` pins 0.7.2: `cargo tree -p mh-service -i
wtransport` resolves `wtransport v0.7.2`, and `wtransport-0.7.2/src/driver/mod.rs:172-188` shows
`receive_datagram(session_id)` looping and discarding on session-id mismatch with a `debug!`.
The claim holds on the version the build actually uses.
**Consequence recorded in the CATALOG, not only in the operations artifact (@security + @operations):
this counter is NOT usable as an SLI.** A value any authenticated client can inflate would let a
client manufacture SLO burn — an availability attack converted into an error-budget attack, and a
client-controllable deploy block if it ever gates a release. Story 8 reads the catalog first, so
the constraint has to live where the next author looks.
`frame_rx.datagram` counts raw QUIC DATAGRAM frames one layer below WebTransport session
demultiplexing, so datagrams carrying an unmatched HTTP/3 session-id varint still increment it
while wtransport discards them — they land in the delta. Any client past the JWT gate can
therefore inflate this counter. It is an **upper-bound input signal, not an MH-internal
invariant**, and the catalog entry will say so in those words. A should-read-zero classification
would have handed an authenticated client a lever to force an on-call page; the
saturation-or-input group plus a rate-over-window rule removes it.

**@security SEC-2 / @test Q5 — the claim I made is withdrawn.** Both sample points sit after
`validate_meeting_token`, so **every pre-auth and failed-auth exit path remains uncounted**:
`accept_bi` failure, `read_framed_message` error, `MhClientMessage` decode failure, empty oneof,
JWT validation failure, `RegistrationOutcome::Timeout`, `RegistrationOutcome::Cancelled`. The
peer that opens a session, floods datagrams and never authenticates stays exactly as invisible as
before. **"Makes pre-auth datagram consumption visible" will not appear in the artifact, the
catalog, or the runbook.** The residual is filed in `docs/TODO.md` with those seven exit paths
named, alongside the honest design question of whether a pre-auth flood belongs on a
media/session-scoped counter at all.

**Mechanical consequences.** `MediaDropReason::ALL: [Self; 11]` → `[Self; 13]`;
`dropped: [Counter; 11]` → `[Counter; 13]` plus two new destructuring arms; wildcard-free
`as_str`; two `direction()` arms, both joining the existing Ingress group.

**@observability F3 — the footgun is pinned at the read site.** `stats()` exposes both
`frame_rx.datagram` (QUIC DATAGRAM frames, singular — what we want) and `udp_rx.datagrams`
(`stats.rs:12`, UDP packets received, plural, one letter apart on the same struct). A comment at
the read site names the distinction, and the integration assertion is an **exact** expected count
rather than `> 0`.

**@dry-reviewer #4 — boundary note at the call site.** `crates/mh-service/Cargo.toml:23-27` and
`crates/mh-service/src/transport/mod.rs:244-266` record that `quic_connection()` compiling here
does **not** license using it for datagram I/O. One sentence at the call site says this is a
stats read only and points at that clause, so a later author cannot cite it as precedent for a
send.

### D2′ consequences for the rest of the plan

- **`crates/mh-service/src/media/ingress.rs` IS now touched**, and gains a Cross-Boundary row.
  The change is a loop-local `u64 += 1` and a return-type widening. **No macro, no metric handle,
  no registry lookup, no allocation, no clock read, no change to zero-copy or to any cap or
  bound** — §11's constraints are intact and the in-crate macro-deny walker stays green.
  @code-reviewer confirmed rev-1 on the basis that `media/**` was untouched; this is the one
  thing in rev-2 that changes that premise, and it is flagged to them explicitly.
- **`MediaSession`'s `ingress` handle** changes from `JoinHandle<LoopExit>` to carry the count,
  so teardown awaits ingress separately from the `for (name, task)` loop over forward and egress.

### D5 — @dry-reviewer F1: DELETE the restated integer, do not update it

`docs/observability/metrics/mh-service.md:496` reads "Cardinality: Low (19 = 11 MH-local + 8
codec)". Rev-1 proposed updating it to `20 = 12 + 8`; **that is the wrong remedy and @dry is
right.** Updating it reproduces the failure the number is an instance of and re-arms it for the
fourteenth token — and it is the only unconverted member of a class whose two siblings in the
same file (`:261` `PolicyApplyOutcome`, `:394` `MediaSessionStartOutcome`) already say
"deliberately no restated integer". `mc-service.md:351` records that such a number drifted four
times in one devloop. The line is deleted in favour of naming the vocabularies and pointing at
the compile-checked `ALL`. Routed to @observability, who owns the file and is already editing it.
Scope is that one line: `:450` and `:615` restate integers over CLOSED axes and are deliberately
out of scope.

### D6 — @operations OPS-F2: the TAIL is now FIXED; the BUFFER INVERSION is filed

**Two different things were travelling under one heading, and rev-2 separates them.**
@operations withdrew their acceptance of the tail deferral on the grounds that ground (b)
evaporates once the ingress-loop count exists for correctness — and they are right, so **the
mid-session and post-teardown tails are no longer deferred at all.** D2′'s token 1 absorbs them
by construction. @dry-reviewer's retraction reached the same place from the other side: ground
(b) was priced against the wrong artifact, since a task-local `u64 += 1` returned through the
loop's exit value is not a `metrics` counter increment — no atomic, no label lookup, no
allocation, no macro. `docs/TODO.md`'s "the sizing was done without opening the loop" entry is the
in-tree precedent for exactly that mistake, and I made it. Nothing is deferred here.

**What IS filed is @operations' independent finding**, which no counter fixes:


**Arithmetic verified independently just now:**

- `DATAGRAM_RECEIVE_BUFFER_BYTES` = 2048 B; `NOMINAL_AUDIO_FRAME_BYTES` = 236 B → **≈8.7 frames,
  ≈174 ms** of quinn-side absorption.
- `INGRESS_QUEUE_FRAMES` = **16** (`config.rs:403`), whose own doc claims "~320 ms of ingest
  backlog, far beyond any healthy scheduling gap".
- **quinn's buffer is roughly half MH's own ring**, so under scheduling pressure quinn sheds
  first and MH's ring never reaches its bound.

Three consequences the entry will name: (1) `mh_media_frames_dropped_total{reason=
"ingress_queue_overflow"}` looks **structurally unreachable in production** — the same "a counter
that exists and cannot fire" objection `media/ingress.rs`'s own module doc makes for loop C;
(2) the documented "~320 ms" absorption is fiction, real absorption is ~174 ms set by a constant
chosen to clear the max-Opus-frame floor; (3) `config.rs:1473` enforces exactly this ordering on
the **egress** side (`ConfigError::EgressQueueDoesNotBindFirst`) and the ingress side has no
counterpart, so ADR-0036 §1's principle is enforced in one direction and violated in the other.
Both derivations go in the entry — @operations' 8.7 nominal and my measured ~7 survivors, which
agree once encapsulation overhead is included. `EgressQueueDoesNotBindFirst` is named as the
model for the missing check. The @security SEC-2 pre-auth residual is filed adjacent so the two
halves of the mechanism are found together.

Note that D2′'s token 1 **does not fix** this inversion — but it does make it visible for the
first time, which is why the entry is a config/validation question rather than an observability
one.

### D4′ — Tests, revised for @test Q1/Q2/Q4

- **Determinism (Q1), and the pin's home moves.** Both firing-path tests live in
  `crates/mh-service/tests/media_session_binding_integration.rs`, not `webtransport_integration.rs`:
  that file already owns the real `AcceptLoopRig`, `MockMcServer` with its per-participant
  `SenderReplies` table, every decline arm and `MetricAssertion` — and both new tokens are
  session-lifecycle observables, which is that file's subject.
  1. `datagrams_arriving_before_the_ingress_loop_are_counted_not_silently_evicted` — fires
     `transport_receive_dropped`.
  2. `datagrams_arriving_on_a_declined_connection_are_counted_not_silently_discarded` — fires
     `no_media_session`, with a pairing control asserting
     `mh_media_session_starts_total{outcome="declined_no_sender_binding"}` rose in the same run,
     so a green cannot mean "the connection was never declined". Two tokens means two firing
     paths, and the `StreamRateLimited` lesson applies per token (@test).
  Eviction is arithmetic, not timing: (a) `DATAGRAM_RECEIVE_BUFFER_BYTES` = 2048 B is a
  compile-time constant and the rig goes through the same `build_transport_config`; (b) the
  client sends N datagrams whose TOTAL BYTES exceed that window by a wide margin before writing
  the connect envelope, and quinn's check at `datagrams.rs:128-136` is on `incoming.memory_used()`,
  so once cumulative bytes pass the window it MUST evict; (c) nothing drains that window — the
  ingress loop does not exist yet and `recv_datagram` has exactly one caller, which is the
  property the whole diagnosis rests on.

  **Assertions — a loss-tolerant sandwich, NOT the equality I first promised.** The test cannot
  read `frame_rx.datagram` (un-exported MH-side quinn state — it is exactly the missing term the
  deferred live-detector supplies, and @test's ruling is: do not export it to satisfy a test), so
  an equality could only be against the sent count `N + M` — and QUIC datagrams are unreliable
  and the client's own send buffer can evict under a burst, so any loss would red a CORRECT
  implementation. Three assertions instead, each deterministic under loss:
  - **Anti-double-count invariant** (the one that reds if rev-1's bug regresses):
    `forwarded{ingress} + Σ dropped{direction="ingress"} <= N + M`. **Per @test's catch the
    eviction term appears EXACTLY ONCE** — `transport_receive_dropped` carries
    `direction = ingress`, so the wildcard sum already contains it, and adding a separate
    `+ transport_receive_dropped` term would make the left side `Q + E`, exceed `N + M` on a
    CORRECT implementation and false-red. Resolution (a): the wildcard sum includes it, no
    separate term. The left side is then `frames_read + evicted` = `Q`, and `Q <= N + M` holds
    always, because QUIC never retransmits a datagram and loss only lowers `Q`. rev-1 would have
    EXCEEDED `N + M` by the surviving-buffered subset, which is precisely SEC-1.
  - **Counter fired**: `transport_receive_dropped >= N − ceil(DATAGRAM_RECEIVE_BUFFER_BYTES /
    frame_bytes)`, deterministic from the pinned window and the total-bytes construction. A
    comment at the site records that this bound assumes negligible transit loss on the burst —
    true on the loopback rig, NOT true if ported to a lossy harness (@test).
  - **Non-vacuity** (@observability's caution, @test's endorsement): `forwarded{ingress} >= 1` in
    the same run, so the sandwich cannot pass over an empty set — the exact "nothing was sent"
    state that funded this whole task.
- **Gate execution (Q4).** `scripts/layer7.sh:825` runs `cargo test -p env-tests --features all`
  with **no `--ignored`** (verified by grep: there is no `--ignored` anywhere in that file), and
  `all = ["smoke", "flows", "observability", "resilience"]` includes the `flows` feature that
  gates `26_mh_quic.rs`. So deleting the attribute puts the test in the default set the gate
  runs. I will paste a run in exactly that mode — no `--ignored`, against the tree with the
  attribute deleted — before claiming the DoD.
- The `media_metrics_integration.rs` 16-token collision test, the `MediaDropReason::ALL`
  iteration and the macro-deny walker all stay green with thirteen tokens.


### Recorded Lead rulings and accepted deviations (Gate 1)

**DELIBERATE, REVIEWED DEVIATION FROM THE TASK WORDING — `crates/mh-service/src/media/ingress.rs`
is in the diff.** The task says "Zero-copy/no-macro/§11 constraints of the media directory
unchanged." Rev-1 read that as "the directory does not appear in the diff", and **that literal
reading is what produced the double-count**: with no count of what the loop read, the only
available number was `frame_rx.datagram` at session start, which overlaps `forwarded{ingress}` by
the surviving-buffered subset. Three reviewers independently read the sentence as a constraint on
the *invariants* rather than a ban on the *directory*, and @main ruled it approved. The
invariants all hold, checked against §11's text rather than its summary (@code-reviewer): the
per-frame work added is a loop-local `u64 += 1` — not a macro form, not an allocation, not a
registry lookup, not a clock read — and the directory-scoped deny bans macro forms while
explicitly allowing increment/record calls. The return-type widening is addressed by no §11 rule.
The one §11 hazard genuinely engaged is the voice-activity-trace bar, and SEC-4's
no-`Display`/no-`Debug` newtype converts it from reviewer vigilance into a compile error, which is
a stronger position than the literal reading would have produced.

**EXPLICITLY ACCEPTED CEILING — `transport_receive_dropped` is FORENSIC, NOT DETECTIVE, and MH
still cannot see silent ingress media loss while a meeting is running.** It moves only at
connection close; meetings run for an hour; so it describes an incident that has already ended.
A reader of this commit could otherwise mistake it for "MH can now see this", which is why it is
stated here and not only in the runbook. @operations was offered the block and declined it —
"visibility we can document beats coverage we don't have" — and put the inverted habit into their
Scenario 16 before any diagnostic step: **a flat series during an active no-audio incident is NOT
evidence that ingress is healthy.** The follow-on that closes it is named in `docs/TODO.md` and is
much smaller than either of us assumed: it needs only `mh_media_datagrams_received_total`
(a periodic sum of `frame_rx.datagram` over live connections), because the accounted half already
exists live as `forwarded{ingress} + dropped{direction="ingress"}` — so the live silent-loss rate
is a PromQL subtraction of two series, one of which is already published. Owner: observability
with media-handler.

**VERIFIED AGAINST THE PINNED VERSION, not a neighbouring one (@main's precondition 1).**
@observability flagged that they had read wtransport **0.7.1** while `Cargo.lock` pins **0.7.2**,
and that limitations 2 and 3 of their catalog entry plus @security's SEC-3 all rest on it.
Checked and POSITIVE: `Cargo.lock:5263-5265` pins 0.7.2; `cargo tree -p mh-service -i wtransport`
resolves `wtransport v0.7.2` (the lockfile and the resolved graph can disagree, so both were
checked); both versions ARE extracted in this container, which is why 0.7.1 was found first; and
`wtransport-0.7.2/src/driver/mod.rs:172-188` shows `receive_datagram(session_id)` looping and
discarding on session-id mismatch with a bare `debug!`, unchanged from 0.7.1. Nothing needs
rewording. Recorded rather than remembered, because this is the same unchecked-premise shape as
the escalation that funded the task.

### Reviewer notes carried into implementation

- **@dry-reviewer note 1** — the catalog must record WHY the two tokens are two, or a future
  reviewer collapses them: they share a mechanism and differ only by connection outcome, which is
  exactly the shape someone merges as "one condition, redundant split". Neither series is a
  function of the other, so neither may be deleted as redundant. (Routed to @observability.)
- **@dry-reviewer note 2** — the `transport_send_refused` / `transport_receive_dropped` pairing is
  **LEXICAL, not semantic**. The former's rustdoc says "should read zero forever"; the latter has
  a legitimately non-zero tail. The names may pair; the catalog GROUPS must not, and the entry
  must state the asymmetry rather than let a reader infer symmetry from the surface — otherwise
  they import zero-forever alerting discipline onto a counter that will not be zero, and then
  either alert on noise or dismiss a real rise as "the usual tail". (Routed to @observability.)
- **@dry-reviewer note 3** — a sentence at the `frames_read` declaration recording that it is
  task-local and deliberately un-emitted: as a series it would be
  `forwarded{ingress} + ingress_queue_overflow + oversize_datagram` and duplicate all three, and
  the first reconciler to find them disagreeing at a scrape boundary would chase a false
  discrepancy. What justifies it existing at all is being **per-connection at teardown**, which is
  exactly what those three cannot give across a scrape boundary.
- **@semantic-guard Q1 — the join-error is NOT silent.** Only the frame *tally* is suppressed.
  `connection.rs:513-520` already emits `warn!(target: "mh.webtransport.connection", …, "Media
  loop task failed")` with the error, so a dead ingress task is an error-level event today and
  this change does not relocate the silent-drop hole one layer up.
- **@semantic-guard Q2 — the newtype's accessor is narrower than a `get()`.** The inner field is
  private (a non-`pub` tuple field), and there is **no accessor returning the count**. The only
  method is `unread_since(self, received: u64) -> u64`, which consumes the value and returns the
  *difference*. A sibling therefore cannot obtain `frames_read` at all — only the residual, which
  is near-zero in health and is not a talk-duration proxy. The type-system control is structural,
  not cosmetic.
- **@semantic-guard's checks.md gap, to be filed**: no check in the authoritative list has as its
  subject "a value derived from per-frame media crossing a privacy boundary into a log". It is a
  linkability concern, which Credential-leak §11 explicitly scopes OUT. The newtype is the only
  control on that path today, and the macro-deny walker structurally cannot see it. Filed in
  `docs/TODO.md` §Media Path Obligations as a named-check candidate so the type-system control
  does not silently become the institutional memory.
- **@operations' alert window moved 30m → 2h** after @observability found the ratio inflates by
  roughly `session_length/window` — the numerator lands in one instant at connection close while
  the denominator accrues continuously, so at 30m an hour-long session losing a true 5% reads
  9.5% and a `> 0.05` rule trips at ~2.6% true loss. Nothing in MH changes; recorded here so no
  artifact of mine cites a stale window.

---

## Pre-Work

None — tree clean at `d090c753`.

---

## Implementation Summary

**The headline is a negative result, and it is the deliverable.** There was no MH datagram
receive-path defect. `crates/mh-service/` is byte-identical between task 24's commit and task
25's, and `26_mh_quic.rs::test_mh_forwards_an_audio_datagram_back_to_its_sender` passes against
the live cluster **unmodified**. §Diagnosis E1-E6 carries the evidence; the commit message says
so plainly, because a commit claiming to have fixed a receive-path gap would be false.

**What was actually wrong, and it is one layer above where anyone was looking.** Datagrams that
arrive on a connection with no reader are evicted inside quinn's datagram receive buffer with
only a `debug!` and no counter anywhere in MH. Measured: 60 datagrams on one connection produced
47 counted; 13 vanished. That is why three sessions could not tell "the client sent nothing" from
"MH threw it away" — and it is the "add the missing observable if one exists" clause resolving to
YES, for a different hole than the task predicted.

### What landed

1. **Two `MediaDropReason` tokens** (`ALL: [Self; 11]` → `[Self; 13]`, `dropped: [Counter; 13]`,
   wildcard-free `as_str`, one `direction()` arm each, both `ingress`):
   - `transport_receive_dropped` — connections that HAD a media session. Computed at teardown as
     `quinn::Connection::stats().frame_rx.datagram` minus what `run_ingress` actually read.
     **Exact, with no overlap by construction.**
   - `no_media_session` — DECLINED connections, where no ingress loop is ever spawned so every
     datagram is discarded. Split onto its own token because an MC outage would otherwise let
     declines swamp the first signal exactly when both matter.
2. **`media::ingress::FramesRead`** — a loop-local `u64` returned through the loop's exit value,
   wrapped in a newtype with no `Display`, no `Debug`, no derive of either, a private field and
   **no accessor returning the count**; its only method consumes the value and returns the
   difference. At ~50 frames/s that count is talk duration for a named participant, and
   `connection.rs` logs `connection_id` beside `participant_id` — so this makes
   `debug!(?frames_read)` a compile error rather than a review miss.
3. **Two component-tier firing paths** in `media_session_binding_integration.rs`, driving the real
   accept loop over a real wtransport/quinn connection (a test double has no receive buffer to
   evict from and would be green against any implementation).
4. **The `#[ignore]` deleted** and the doc comment rewritten — it asserted as fact a defect that
   does not exist.
5. **Three homes of the falsified triage claim corrected** — `metrics.rs`'s `Started` docstring is
   mine; @observability took the catalog row and the panel description.
6. **Four `docs/TODO.md` entries**: the R-15 entry deleted per its own four-part closure
   condition, and three filed — the quinn-buffer/ingress-ring inversion, the missing live
   detector, and the seven uncounted pre-auth exit paths — plus @semantic-guard's missing check
   class.

### Verification

- **DoD, in the gate's own mode** — no `--ignored`, no `--exact`, attribute deleted:
  ```
  $ cargo test -p env-tests --features all --test 26_mh_quic
  test test_mh_forwards_an_audio_datagram_back_to_its_sender ... ok
  test result: ok. 9 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 153.21s
  ```
  The two remaining `ignored` are pre-existing component-tier stubs, not this test.
- **Both new series live on both pods** after redeploy, present-and-zero from process start.
- **Mutation-tested rather than assumed green.** Three mutations, each red:
  | Mutation | Assertion that fired |
  |---|---|
  | Delete the teardown emission | `…at least 31 could not have survived, but only 0 were counted` |
  | **Emit the raw received total (rev-1's double-counting shape)** | `ingress accounting exceeded what was sent (0 forwarded + 48 dropped > 40 sent)` |
  | Delete the declined-branch emission | `40 datagrams sent, 0 counted` |
  The middle one is the important one: it is @security's SEC-1 reproduced, and the assertion
  catches it.
- `cargo test -p mh-service` green (241 unit + every integration binary).

### Two things found while implementing that were not in the plan

- **The `MetricAssertion` label filter selects the FIRST matching series; it does not aggregate.**
  `with_labels(&[("direction", "ingress")])` over a multi-series family returns one arbitrary
  member and looks exactly like a total. It bit this work: the ingress "sum" read back the
  eviction term alone, which made a non-vacuity check trivially true. The test now sums explicitly
  over a list DERIVED from `MediaDropReason::ALL` and `ALL_REJECT_REASONS`, so it cannot go stale
  as either vocabulary grows.
- **A `metrics` handle binds to the recorder installed when it is RESOLVED, not when it is
  incremented.** `AcceptLoopRig` calls `resolve_media_handles()` at start, so a snapshot taken
  afterwards installs a recorder those handles never write to — handle-based counters then read
  zero while macro-emitted ones read correctly, which is a mixture that produces a confident
  wrong green. The new test snapshots BEFORE the rig starts and says why at the site.
- **The `forwarded{ingress} >= 1` non-vacuity floor I promised @test does not hold in this rig**,
  and replacing it was not cosmetic: the rig installs no forwarding policy, so a frame reaching
  `forward_one` lands on `no_policy`, which deliberately does not increment `forwarded{ingress}`.
  The floor would have failed against a CORRECT implementation. It is now "ingress drops exceed
  the eviction term" — the same property, naming no reason, so it survives a rig that later does
  install a policy.


### The fourth implementation-time finding, and what all of them turn out to be

**O-1, found by @observability and independently by @test.** The non-vacuity floor that replaced
`forwarded{ingress} >= 1` was `ingress_drops > unread` — and it is wrong in the opposite
direction from the one it replaced. `ingress_drops` already contains the eviction term, so that
form asserts *"some read frame produced an ingress DROP"*: true in a rig with no forwarding
policy, **false the moment a policy is installed and frames forward cleanly**, at which point it
reds a correct implementation while its own message claims nothing reached the forward path. Its
comment asserted the opposite. Now `forwarded + ingress_drops > unread` — the whole accounted
set, because either addend alone is a partial view of "was anything read", a read frame being
free to terminate in either.

Together with the three above that makes four, and they are not four incidents. **They are two
classes with different remedies, and the distinction is load-bearing rather than taxonomic.**
The split is @dry-reviewer's, ruled by @team-lead; the refinements below are attributed
individually because each defeated somebody's earlier version.

#### Class A — an unchecked second encoding of a value whose authoritative home is another construct

Instances: the restated cardinality integer at `mh-service.md:496`; the "twelfth value" ordinals
at `metrics.rs:784`/`:937`; the `crates/common/` rustdoc citing a `docs/TODO.md` entry that had
never been written; `configmap.yaml` reserving "Scenario 15" in a runbook sequence it does not own.

**The invariant is: is there a mechanism that makes the copy fail when the original changes?**
Two weaker predicates were proposed and both were defeated. *"Correct when written, invalidated
later"* (@dry-reviewer) fails on the dangling citation, which was never correct — nothing rotted,
there was no target. *"A coordinate in a namespace the citing file does not own"* (mine, and
@observability's, in `docs/TODO.md` § *Citation Rot*) fails on the ordinals, which are the same file, the
same `impl`, **three lines above `ALL: [Self; 13]`**, with no second author and no boundary
crossed — so it would not flag the one instance this devloop created. Namespace-ownership
survives as the dominant sub-case, and is what yields the actionable remedy (cite by a stable
token, never by position), but it is not the definition.

**`ALL: [Self; N]` is the definition by contrast, not merely an example of the remedy**
(@observability's counter-example, sharpened by @dry-reviewer): the same value, in the same file,
restated once in a form the compiler checks and once in a form it does not, three lines apart.
Class A is mechanically preventable, and that array is the proof the remedy already works in-tree.

**The class covers CLAIMS, not only values** (@dry-reviewer's widening): a predicate, a definition
or a rule of thumb restated in a second place with nothing reconciling the two drifts on exactly
the mechanism of a cardinality integer.

#### Class B — a claim asserted from the wrong artifact, false the day it was written

**Led by the instance that funded this task**: the escalation's *"datagrams are not reaching the
routing lookup at all"*, derived from a Prometheus reading whose precondition — that a datagram
had ever been sent — was never checked. It is the only member whose cost is measurable: **three
sessions.** Then: the triage row asserting that `started` climbing with flat ingress frames is a
client-side condition; @observability's ingress identity asserted from the emission sites rather
than from `run_forward`'s cancellation path; @dry-reviewer's session-start-sample argument
reasoned from the counter's semantics rather than from `datagrams.rs`'s eviction discipline; my
own floor validated against the one rig it was written for; and my rev-1 design, which was correct
about the frames it counted and wrong about which of them were already counted elsewhere.

**Nothing rotted.** Each was false on the day it was authored, because its author reasoned from a
proxy instead of opening the thing itself.

**The remedy is a describable review move, not an exhortation to be careful.** In every instance,
the person who caught it **opened the artifact the claim was ABOUT, not the artifact the claim was
IN** — one hop upstream of where the sentence lived. @security opened `run_forward`.
@dry-reviewer opened `datagrams.rs`. @observability opened the sibling catalog entries. I opened
the cluster instead of re-reading the metric. That is a move a reviewer can be asked to perform.

**Class B is not mechanically preventable at all**, and that asymmetry is why the two classes must
not be merged (@team-lead's ruling): **Class A's remedy is enforced by a mechanism and survives
inattention; Class B's is a practice and degrades silently exactly when review attention is
scarce.** A reader of a merged entry could reasonably conclude that a citation guard would have
caught the receive-path gap. It would not have.

#### Two things that keep the record honest

**Membership is contingent on the artifact landscape, not intrinsic to the mistake**
(@dry-reviewer). The remedy test decides by what controls *currently exist* — so if a predicate
stated across two conversations were ever captured in an artifact, the identical failure would
become mechanically checkable and would move to Class A. Recorded so nobody later "fixes" an
instance by re-taxonomising it instead of by creating the artifact.

**Two Class A instances were CREATED in this diff while two others were being removed, and both
created ones were @observability's** — the dangling citation, and the catalog sentence @security's
SEC-5 falsified. They recorded both themselves, unprompted, and are also the reviewer who filed
the class in the first place before either of their own instances was found. A record that omits
this is a worse record. **A third belongs to @dry-reviewer**: they stated Class A's predicate one
way to @observability and a weaker way to me and @team-lead, and the two diverged. They asked for
it recorded under their name, classified it Class A on shape, and conceded to Class B on the
remedy test — remedy being what the split was built on, so remedy decides membership. Their own
sentence *"nothing in this loop would have caught it"* was the deciding evidence, since it asserts
that no mechanism was available, which is Class B's defining property.

**And a fourth is mine, found in this artifact while writing this section.** The Cross-Boundary
row read `` docs/TODO.md | Mine `` and the §Files Modified summary said "three entries filed".
The row was true when written and was falsified by @observability and @dry-reviewer writing in
behind it; the summary was **already stale by my own hand**, since `:725` predated it and I
summarised the file's contents without re-reading them. Both are corrected above to per-entry
authorship across four authors. No guard would have caught either — `docs/TODO.md` is not a
Guarded Shared Area and "Mine" passes the classification check.


#### The class spans owners, and the protocol has no construct for that — SEC-6, found at Gate 3

**Two more Class A instances were in this diff the whole time**, found by @security while
re-checking their own Gate 1 condition against the shipped artifacts:
`docs/observability/metrics/mh-service.md:451` restating `INGRESS_QUEUE_FRAMES` as `(16)`, and
`docs/runbooks/mh-incident-response.md:1345-1359` restating `2048`, `16` and the derived
`8.7 frames` / `~174 ms` / *"roughly half"*. The second is the more dangerous by a wide margin:
**the number is not decoration there, it is the whole conclusion.** Step 3's finding — quinn sheds
before MH's ring, therefore `ingress_queue_overflow` may never fire, therefore this is a sizing
defect — **is an inequality between two constants.** Retune either and the inequality inverts
while the prose still asserts it, and a responder mid-incident is actively steered toward a defect
that no longer exists. A stale integer in a catalog is a confusing reconciliation; a stale
inequality in a runbook step is a wrong diagnosis under time pressure. The fix keeps the
relationship by constant name and makes the check a step the responder **performs** rather than a
fact the runbook asserts, so it fails toward "go look" rather than toward a confident wrong answer.

**Why the sweep missed them, which is the actual finding.** F1 and F3 were fixed by sweeping the
files I own. These two survived because they live in files I do not — @observability's and
@operations'. **A fix applied per-owner does not sweep a class that spans owners**, and nothing in
this protocol says "this class, everywhere, regardless of author": the Cross-Boundary table's unit
is a path with one owner, reviewers are assigned by domain over the diff, and **a class-wide sweep
has no owner at all.** This is the path-vs-hunk gap arriving as a consequence rather than as an
argument — it now has an instance that cost something instead of a hypothetical. @dry-reviewer is
filing it in `docs/TODO.md`; see §Accepted Deferrals.

**And instance 1 is mine, with a mechanism worth stating precisely rather than apologising for.**
@security made that wording a **binding condition** at Gate 1. I applied it — to `main.md`, where
§Planning D2′ reads *"cited by NAME, never restated as an integer, in the catalog, the runbook
heading or the alert annotation."* I never carried it to the two specialists who then wrote those
three artifacts. **The condition was discharged against the wrong document: satisfied in the one
nobody ships, missed in the two that do.** "I forgot to tell them" would be the worse record; the
true one is that a binding condition was recorded as met against an artifact that was not its
target, which is itself the Class A shape one level up — my plan text as an unchecked second
encoding of an obligation whose authoritative home was the artifacts.

**This is the first place in this record where the two classes interact, and it is worth the
sentence.** These Class A instances were found only because @security re-opened the shipped
artifacts instead of trusting the plan that promised the condition had been met — **Class B's
remedy, applied to a reviewer's own prior confirmation, catching Class A instances nobody's sweep
covered.** The nine instances above become eleven, and the count is less interesting than the
mechanism: the classes are distinct in remedy but not disjoint in occasion.

**The per-owner sweep DOES work — it just has no owner until someone names the class.** Told about
one instance each, @observability swept their own file and found a second (`mh-service.md:458`,
"bounded by 16") that a fix aimed only at the reported line would have missed, and @operations
found a third in their alert's `impact` annotation. Both were self-swept, unprompted, immediately.
That is the other half of the path-vs-hunk finding: the remedy is effective and cheap once a
person assigns it, and nothing in the protocol assigns it.

**A fourth instance is mine and it is the sharpest of them.** §Accepted Deferrals opened with
*"recorded here as pointers rather than restated — restating them would be the Class A defect this
devloop spent its review budget on"* — and the very next bullet restated one. `dt-guard`'s
`validate-todo-tracking` caught it as `[inline_debt_body]`; @operations flagged rather than fixed.
**The rule and its violation were adjacent, written in one pass, by the author of the ruling.**
Together with @observability's `(16)` beside their own class definition, that is the third
independent occurrence of *maximum context did not prevent it* — which is the entry's thesis
rather than a fourth incident: this is one gap in the guard set, not N authoring mistakes.

**The sharpest sub-pattern, and it happened TWICE: a remedy applied without re-reading its
surroundings generates the class it is closing.** @test found the `FramesRead` formula missing a
term; I added it; @code-reviewer showed the corrected formula was still wrong and had it deleted;
**the section HEADING then survived my own deletion, still asserting the claim the body had just
retracted.** Four passes over one rustdoc paragraph, three of them producing a fresh instance —
by people with the class definition open. Separately, converting @test's wall-clock settles to
polling **created a dead `assert!(counted >= 1)`** that the poll made unreachable: a control that
reads as present and cannot fire, which is the `StreamRateLimited` shape, manufactured by the fix
for a different finding. Both are removed. Neither was carelessness, and that is the point — this
is one gap in the guard set, not N authoring mistakes.

**The uncomfortable observation, which is @dry-reviewer's**: no mechanical check in this tree saw
any of the eleven. Every one was caught by a person reading one artifact against another.

---

## Files Modified

| File | Owner | What |
|------|-------|------|
| `crates/mh-service/src/observability/metrics.rs` | mine | Two `MediaDropReason` tokens + arrays/arms; `record_media_frames_dropped` sibling emitter; `Started` docstring corrected |
| `crates/mh-service/src/media/ingress.rs` | mine | `FramesRead` newtype; loop-local count returned through the exit value |
| `crates/mh-service/src/webtransport/connection.rs` | mine | `quic_datagram_frames_received` stats read; teardown delta; declined-branch sample; ingress arm split out with the log statement byte-identical |
| `crates/mh-service/tests/media_session_binding_integration.rs` | mine (paired @test) | Two firing-path tests; `connect_flooding_first`; `ingress_drop_total` |
| `crates/env-tests/tests/26_mh_quic.rs` | test | `#[ignore]` deleted; doc comment and module scenario list rewritten |
| `docs/TODO.md` | **four authors** | MINE: R-15 entry deleted per its four-part closure condition; buffer-inversion, live-detector and pre-auth-exit-paths entries filed; the SLO entry's R-15 blocker clause corrected. OBSERVABILITY: `:725` (MetricAssertion first-match/sum), `:2055-2064` (citation-class generalisation). DRY-REVIEWER: `:2066` (review-coverage gap). OPERATIONS: §Devloop Container Resource Hygiene item (E) |
| `docs/observability/metrics/mh-service.md` | observability | Catalog entries, three-way triage row, limitations, `:496` integer deleted |
| `infra/grafana/dashboards/mh-overview.json` | observability | Panel description corrected; stale caveats replaced |
| `infra/grafana/dashboards/mh-slos.json` | observability | Stale caveats replaced |
| `docs/observability/slos.md` | observability | Stale blockquote replaced |
| `infra/docker/prometheus/rules/mh-alerts.yaml` | operations | `MHIngressDatagramsNeverRead`, warning, 2h window |
| `docs/runbooks/mh-incident-response.md` | operations | Scenario 16 |
| `infra/services/mh-service/configmap.yaml` | operations | `MH_KEEPALIVE_INTERVAL_MS` anchor fix (comment only) |

---

## Devloop Verification Steps

### Gate 2 — `./scripts/layer-all.sh` (unattended, `DEVLOOP_FAIL_FAST=0`, all seven layers)

**Run 1 — VOID, does not consume an attempt.** Started before the implementer's O-1 and
sampling-order fixes landed, so its verdict was about a tree that no longer existed. Not triaged.
Two signals from it were still informative: Layer 3 failed on `validate-cross-boundary-scope`
(the missing `crates/common/src/observability/testing.rs` row — since added, now
`STATUS=OK REASON=cross-boundary-scope-no-drift`), and Layer 7 PASSED.

**Run 2 — settled tree.**

| Layer | Result | Duration |
|-------|--------|----------|
| 1 Compile | OK | 11s |
| 2 Format | OK | 1s |
| 3 Guards | OK | 48s (WARN BUDGET_BREACH, budget 20s) |
| 4 Test | see below | 11s |
| 5 Lint | OK | 4s |
| 6 Audit | N/A (`no-dep-changes`) | 2s |
| 7 Env-tests | OK (`env-tests-passed`, `browser-e2e-passed`) | 567s |

### Layer 4 — an environment fault, diagnosed to root cause, NOT the diff

Runs 1 and 2 both failed Layer 4 with `rust-lld` aborting —
`collect2: fatal error: ld terminated with signal 6 [Aborted], core dumped`, LLVM backtrace through
`llvm::parallel::TaskGroup::spawn` -> `std::__throw_system_error`, i.e. **thread-creation failure,
not a compile error**. It struck `gc-service (token_refresh_integration)` on run 1 and
`mc-service (otel_grpc_outbound_integration)` on run 2 — **different crates each time, neither
touched by this diff** — then eleven link failures at once on an idle machine (load 0.33).

Root cause, measured:

- `/sys/fs/cgroup/pids.max` = 2048; `pids.current` = 1799 (and still climbing — 1852 when
  @operations re-measured minutes later).
- A `/proc` scan returns **1756 processes in state `Z`**, **all with `PPid: 1`**.
- `/proc/1/comm` = **`sleep`**. Container PID 1 never calls `wait()`, so every orphaned child
  becomes a permanent zombie and they accumulate for the container's lifetime until the cgroup PID
  ceiling is exhausted. `rust-lld` wants one thread per core (32 here) per link job.

**Counter-example confirming it is the environment and not the changeset**: the identical target,
tree and machine link cleanly under
`RUSTFLAGS="-C link-arg=-Wl,--threads=1"`.

**Layer 4 verdict, obtained with that environment workaround and no gate relaxed** — every test
still compiled and executed, nothing skipped, no assertion weakened:

```
STATUS=OK REASON=cargo-test-passed
STATUS=OK REASON=nx-test-passed
```

**Filed, not masked**: `docs/TODO.md` § Devloop Container Resource Hygiene & Build Isolation item
(E), owner `infrastructure` with `operations`. The entry leads with the *misclassification* rather
than the leak: the wrapper reports `STATUS=FAIL REASON=cargo-test-failed` (exit 1, implementer
lane, consumes an attempt) for a fault whose true class is `PRECONDITION_FAILURE` (exit 2, operator
lane, no attempt consumed) — so the pipeline hands an environment fault to the implementer labelled
as their bug, in a crate they never touched. Two independent halves: `--init`/`tini` as PID 1 stops
the failure; a `scripts/lang/rust/test.sh` classifier for link-time `signal 6` stops the
misattribution, and is worth doing regardless. Deliberately not fixed here — ADR-0030's
cannot-self-validate corollary applies exactly: a container fix verified by the container under
test is not a verification.

### DoD evidence — re-run against the shipped tree

The first DoD run predated two late MH changes. Re-run rather than reasoned about, in the gate's own
mode (no `--ignored`, no `--exact`, attribute deleted), so the test is **collected in the default
set** rather than summoned by name:

```
$ cargo test -p env-tests --features all --test 26_mh_quic
test test_mh_forwards_an_audio_datagram_back_to_its_sender ... ok
test result: ok. 9 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 141.77s
```

The two `ignored` are pre-existing component-tier stubs, neither of them this test.

**Gate 2 verdict: PASS.**

---

## Code Review Results

### Gate 3 — all seven verdicts, no escalations

| Reviewer | Verdict | Findings | Fixed | Deferred |
|----------|---------|----------|-------|----------|
| Security | RESOLVED-DEFERRED | 6 | 6 | 2 residuals accepted |
| Test | RESOLVED-FIXED | 2 | 2 | 0 |
| Observability | RESOLVED-DEFERRED | 6 | 6 | 2 |
| Code Quality | RESOLVED-FIXED | 1 | 1 | 0 |
| DRY | RESOLVED-DEFERRED | 3 | 3 | 1 |
| Operations | RESOLVED-FIXED | 2 | 2 | 0 |
| Semantic Guard | CLEAR (native SAFE) | 0 | — | — |

Three reviewers landed on RESOLVED-DEFERRED, so accepted deferrals exist — see §Accepted Deferrals.

**Notable verdict movements**, recorded because each was a reviewer correcting themselves:
- @code-reviewer **withdrew an earlier CLEAR** after @test surfaced a doc-accuracy point in their lens, verified it against `forward_one`, found the `FramesRead` formula omitted three accounting classes rather than one, and closed it at RESOLVED-FIXED.
- @observability **downgraded their own RESOLVED-FIXED to RESOLVED-DEFERRED** after re-reading the protocol against their own verdict: their §Accepted Deferrals section was non-empty, which the protocol says is dispositive. They had reclassified their deferrals as "not really findings" to preserve the stronger verdict — the exact move the protocol names.
- @security held their verdict pending SEC-6, whose two reported instances became **five** once both owning specialists swept their own files for the class.

**Cross-boundary trailer required at commit** (@test, Minor-judgment on `crates/env-tests/tests/26_mh_quic.rs`), granted verbatim:

```
Approved-Cross-Boundary: test — 26_mh_quic.rs doc-comment rewrite replaces the false receive-path-defect prose with the verified diagnosis (MH byte-identical, eviction hole, un-ignore); test-owner reviewed and accepts the wording
```

Co-signs granted on non-GSA `crates/common/src/observability/testing.rs` (docs-only, verified independently by both required approvers): @dry-reviewer and @code-reviewer.

---

## Issues Encountered

**Gate 2 Layer 4 took three runs to yield a verdict, and none of it was this diff.** It failed
twice with `rust-lld` aborting inside `llvm::parallel::TaskGroup::spawn` →
`std::__throw_system_error` — a **thread-creation** failure, not a compile error — first in
`gc-service`, then in `mc-service`. **Neither crate is touched by this changeset.** Root cause,
found by @operations: container PID 1 is `sleep`, which never calls `wait()`, so ~1756 orphaned
processes have accumulated as permanent zombies against a cgroup `pids.max` of 2048; the linker
cannot obtain threads. Proven environmental by counter-example rather than by argument — the
identical target links cleanly under `-Wl,--threads=1`, and Layer 4 then goes green with nothing
skipped and no assertion weakened. Filed by @operations under `docs/TODO.md` §Devloop Container
Resource Hygiene item (E), with the **misclassification as the headline**: the wrapper reports
`FAIL` (implementer lane, consumes an attempt) for a fault whose true class is
`PRECONDITION_FAILURE` (operator lane). @team-lead ruled that neither failure consumed an attempt.

**Gate 2 run 1 was voided by my own edits, and the way it failed is worth recording.** I signalled
"Ready for validation", then two reviewer findings landed (@observability's O-1 and, on my own
re-read, the declined-branch sampling order) and I fixed them while the pipeline was running. I
told @team-lead unprompted, and predicted that a stale-tree failure would arrive **labelled as the
diff's fault**. It then did exactly that, twice, for a different cause — two untouched crates were
blamed before anyone opened `/proc`. The lesson is not "do not edit"; it is **signal ready only
when the tree has stopped moving, and if a finding lands after, say "holding, tree moving" before
the Lead starts anything.** The cost landed in the right place: one voided pipeline run.

**Three test-harness hazards found while implementing, each of which produced a confident green
before it was caught.** All three are recorded with their mechanism in §Implementation Summary,
because the next person writing a `MetricAssertion` test needs the first one: the label filter
selects the first matching series rather than aggregating (and means the *opposite* on histograms,
which @observability found and documented); a `metrics` handle binds to the recorder present when
it is **resolved**, not when it is incremented, so a snapshot taken after the rig starts silently
observes nothing; and a non-vacuity floor validated against one rig fails against a correct
implementation in another.

---

## Accepted Deferrals

- **`MetricAssertion` first-match/sum unification** — `docs/TODO.md` § *`MetricAssertion`'s label filter means TWO different things depending on metric type* (filed @observability; accepted @dry-reviewer, @security).
- **Review-coverage gap** — `docs/TODO.md` § *Review Coverage Gap — every control binds to an ARTIFACT, so a claim split across two conversations has no reader at all* (filed @dry-reviewer).
- **Path-vs-hunk** — `docs/TODO.md` § *Review Coverage Gap — the Cross-Boundary table's unit is a PATH, but a path can have several authors in one changeset* (filed @dry-reviewer).
- **NOT deferred, though the plan proposed it**: the mid-session and post-teardown eviction tails — `transport_receive_dropped` absorbs all three windows; see §Implementation Summary D2′.
- **Cited by TITLE, never by line** — @dry-reviewer's entry moved twice while being reported; a line number is a position citation in a namespace this file does not own. Rationale lives in the filed entry, not here; `validate-todo-tracking` `[inline_debt_body]` enforces that half.

---

## Resume (2026-09-09) — session interrupted between Gate 3 and commit

The 2026-09-08 session reached Gate 3 with all seven verdicts and then ended before Step 8. The
work was fully staged and byte-identical to the §Files Modified table; `HEAD` was still the start
commit `d090c753` and no `.devloop-escalation.json` existed. Per §Recovery's headless exception,
main.md's Loop State was taken as authoritative: the only incomplete phase was **commit**.

The reviewer panel was NOT respawned to re-litigate settled verdicts. One reviewer, @operations,
was respawned for a resume-time finding in their own filed artifact (below).

### The resumed container is a different container, and that is the whole story of Gate 2's Layer 4

| | 2026-09-08 session | 2026-09-09 resume |
|---|---|---|
| `/proc/1/comm` | `sleep` | `podman-init` |
| `/sys/fs/cgroup/pids.max` | 2048 | 8192 |
| zombie count | 1756–1760 | **0** |
| Layer 4 | `rust-lld` abort; green only under `-Wl,--threads=1` | **`cargo-test-passed` natively, no workaround** |

Same tree, opposite result. This confirms the previous session's diagnosis was correct about its
container and wrong about the tree.

### Resume-time finding: TODO item (E) was false when filed, and its remedy already landed

`--init` and `--pids-limit 8192` landed **2026-09-04** in `c01aaf7e` ("Run the devloop dev container
with a reaping init and a higher PID limit"), which is an **ancestor of this devloop's own start
commit** — four days before item (E) was filed on 2026-09-08 prescribing exactly that fix in three
named files (it touched two; `infra/devloop/Dockerfile` never needed to change). `entrypoint.sh`
already documented it. The 2026-09-08 container had been created before `c01aaf7e` and kept its
creation-time `podman run` flags for its whole lifetime.

Routed to @operations as owner of the entry. Corrected in `docs/TODO.md` **only** (18 insertions,
4 deletions), preserving the entire diagnosis record, the zombie-count guidance, the SCOPE passage,
both presentations, and the both-presentations requirement for the classifier. Item (E) now leads on
the misclassification — **the genuinely open half**, re-verified at resume: `scripts/lang/rust/test.sh`
ends at `run_and_emit "cargo-test" cargo test "$@"`, and `run_and_emit` emits `FAIL` for any nonzero
exit without inspecting output, so a thread-spawn exhaustion still routes to the implementer lane.

@operations filed the residual as **new item (F)** rather than folding it into (E), on the ground
that folding would repeat (E)'s original narrow-unit defect in the opposite direction: a long-lived
container keeps the `podman run` flags it was created with, so any `infra/devloop/` runtime fix
never reaches it and nothing detects the mismatch. `c01aaf7e`'s own commit message ends **"Takes
effect on the next container (re)create"** — the deferred-activation property was known at landing
time and nothing acts on it. Generalized past `--init` to every flag, with the `--add-host` DB-IP
pin named as the sharpest sibling. Filed as work, not built: a staleness detector cannot be
validated from inside the container whose staleness it reports.

@operations' original Gate 3 verdict stands (RESOLVED-FIXED, 2 findings, 2 fixed) — the finding is
against their filed artifact, not the reviewed diff; no MH code, config or observability assertion
moved.

### Gate 2, re-run against the settled tree

The first resume run was started before the `docs/TODO.md` correction landed, which repeats this
devloop's own recorded lesson (§Issues Encountered: *signal ready only when the tree has stopped
moving*). Rather than argue that a docs-only edit provably cannot affect layers 1/4/5/7 — an
argument of exactly the narrow-unit shape this devloop is about — the full pipeline was re-run
against the final staged tree. Both runs are recorded below; the second is the verdict.

Run 2 then failed Layer 3 — `STATUS=FAIL REASON=run-story-selftest-failed`, on a suite reporting
**290 passed, 0 failed**. Cause, established by deliberate reproduction rather than by argument:
`scripts/workflow/run-story.test.sh` carries a **containment proof** — it samples the real repo's
`HEAD | sha256(status --porcelain) | sha256(diff HEAD)` before and after, and fails if the real tree
changed during the run, because its cases exercise a runner that can reach `git reset --hard`. The
resume session appended this very section to `main.md` while run 2 was in flight. Re-running the
suite on a quiet tree gives exit 0; re-running it with a one-byte append to `main.md` timed into the
run reproduces the failure exactly:

```
run-story.test.sh: CONTAINMENT-FAILURE — the REAL repository at /work changed during this run.
  (fields: HEAD|sha256(status --porcelain)|sha256(diff HEAD))   EXIT=2
```

**This is the second time in this devloop that a moving tree produced a failure labelled as the
diff's fault, and the second time the lesson had already been written down** — §Issues Encountered
records it from the 2026-09-08 session in as many words. Recorded rather than quietly re-run.

Two things worth carrying forward, neither fixed here (both out of scope for an MH media-path
change, and neither a defect in this diff):

- The suite's diagnostic concludes *"The post-run state is STABLE across two samples, so no
  concurrent writer explains this: the suite is the actor."* Here the suite was **not** the actor —
  an external writer mutated the tree and then stopped, which the post-run stability sample cannot
  distinguish from suite-caused mutation. The check is right to fail and its own text says do not
  weaken it; the *attribution* in the message is what is over-confident, which is precisely the
  Class B shape §Issues Encountered and TODO item (E) both describe.
- The suite exits **2**, and `run_and_emit` flattens it to `STATUS=FAIL` (implementer lane,
  consumes an attempt). Exit 2 is `PRECONDITION_FAILURE`'s code. Same flattening as TODO item (E)'s
  open classifier half, one wrapper over.

**Run 3 — the verdict**, against the settled tree with no concurrent writes.

| Layer | Result | Duration | Note |
|-------|--------|----------|------|
| 1 Compile | OK | 5s | |
| 2 Format | OK | 1s | |
| 3 Guards | OK | 52s | 41 guards, 0 violations; `run-story-selftest-passed` |
| 4 Test | N/A (`test-aggregate-na`) | 234s | rust `cargo-test-passed`, ts `nx-test-passed`; aggregate N/A is proto's intentional-gap placeholder |
| 5 Lint | OK | 2s | |
| 6 Audit | N/A (`audit-aggregate-na`) | 2s | `no-dep-changes` |
| 7 Env-tests | OK | 264s | `env-tests-passed` + `browser-e2e-passed` |

`TOTAL_RESULT=N/A`, `EXIT=0`, and **zero `FAIL` / `PRECONDITION_FAILURE` / `UNKNOWN` statuses
anywhere in the run** — the N/A total is the worst-child aggregation of the two documented
intentional gaps, which the wrapper contract makes self-justifying via their own `REASON=`.

**Layer 4 needed no `-Wl,--threads=1` workaround.** The 2026-09-08 session could only get a Layer 4
verdict with that flag; here the same targets link and run natively. That is the container
difference in the table above, and it is the evidence the resume-time correction to TODO item (E)
rests on.

**Gate 2 verdict on resume: PASS.** Sole edit after this run is the recording of this result — the
unavoidable regress of a document that records its own validation, noted rather than left implicit.

---

## Rollback Procedure

1. Start commit: `d090c753480fdd335580f9d2a231a085d81ab125`
2. Review: `git diff d090c753..HEAD`
3. Soft reset: `git reset --soft d090c753`
4. Hard reset: `git reset --hard d090c753`
