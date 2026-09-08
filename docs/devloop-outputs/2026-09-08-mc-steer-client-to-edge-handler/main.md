# Devloop Output: Steer client media to the forwarding-assignment placement

**Date**: 2026-09-08
**Task**: Couple client media steering to the forwarding-assignment placement (ADR-0036 §5) — the assignment output becomes the single source of truth for BOTH the send directive's target set and the stream assignment's handler address, so steering and placement cannot diverge. `media_servers` stays as bootstrap data, explicitly non-authoritative. Env-test fixture follows the directive/stream-assignment handler and its ordering gate keys on the SAME instance the client is steered to.
**Specialist**: meeting-controller
**Mode**: Agent Teams (v2)
**Branch**: `feature/hear-yourself-through-handler`
**Duration**: TBD

Story: `2026-08-27-hear-yourself-through-handler`, task 25.
Task prompt: `/tmp/devloop/story-runner/2026-08-27-hear-yourself-through-handler/task-25.prompt`

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `a2adc44062ec00a9b210ce1a66ed841b432b0136` |
| Branch | `feature/hear-yourself-through-handler` |
| Lead Model | `claude-opus-5` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `spawned` |
| Implementing Specialist | `meeting-controller` |
| Iteration | `1` |
| Security | `spawned` |
| Test | `spawned` |
| Observability | `spawned` |
| Code Quality | `spawned` |
| DRY | `spawned` |
| Operations | `spawned` |
| Semantic Guard | `spawned` |
| Paired Protocol | `spawned` |

---

## Task Overview

### Objective

Make the forwarding assignment the single source of truth for where a client sends its media.
MC directs where a client sends (ADR-0036 §5); the client must be steered to the handler the
assignment actually placed its edges on, never left to pick from an unordered list.

**The defect** (diagnosed at task 24's escalation, third session —
`docs/devloop-outputs/2026-09-05-sender-id-binding-contract/main.md` §Resume): edge placement
picks the lexicographically smallest shared handler (`edge_handler`,
`crates/mc-service/src/media_routing/assignment.rs` — `shared.sort(); shared.first()`, i.e. mh-0),
while the `JoinResponse.media_servers` list is built in UNSORTED Redis order
(`crates/mc-service/src/webtransport/connection.rs` ~2252) and the client and env-test connect to
`.first()`. A client can therefore be steered to a handler holding a correct-by-design EMPTY edge
set; on the live cluster three of four media connections landed on mh-1 while every edge sat on mh-0.

### Scope
- **Service(s)**: mc-service (implementer), env-tests fixture
- **Schema**: No
- **Cross-cutting**: No proto shape change expected; pair with protocol only if a message field
  needs a clarifying comment.

### Explicit non-goals (from the task prompt)
- Do NOT implement multi-handler `connectAll` fanout — §9 multi-handler send is a later story.
  One client, one directed handler is story-1 scope.
- Leave `test_mh_forwards_an_audio_datagram_back_to_its_sender` `#[ignore]`d. Un-ignoring it is
  task 26's definition of done (the MH datagram receive-path gap is still open there).
- MC telemetry rules unchanged: `key_custody=operator`, no meeting id on media metrics.

### Debate Decision
NOT NEEDED — ADR-0036 §5 already states the contract; this is an implementation-fidelity fix.

---

## Cross-Boundary Classification

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `crates/mc-service/src/media_routing/assignment.rs` | Mine | — |
| `crates/mc-service/src/webtransport/connection.rs` | Mine | — |
| `crates/mc-service/src/media_signaling/directive.rs` | Mine | — |
| `crates/mc-service/src/media_signaling/assignments.rs` | Mine | — |
| `crates/mc-service/tests/media_client_signaling_integration.rs` | Mine | — |
| `proto/dark_tower/signaling/v1/signaling.proto` (comment-only) | Not mine, Domain-judgment | protocol |
| `docs/runbooks/mc-incident-response.md` (one paragraph, ~:1884-1891) | Not mine, Minor-judgment | operations |
| `crates/env-tests/tests/26_mh_quic.rs` | Not mine, Minor-judgment | test |
| `crates/env-tests/src/fixtures/metrics.rs` | Not mine, Minor-judgment | test |
| `docs/TODO.md` (R-15 entry, task-25 clause only) | Mine | — |
| `docs/TODO.md` (`mc_media_send_directives_total` fired-trigger correction, OPS-10) | Not mine, Minor-judgment | operations |
| `docs/runbooks/devloop-validation.md` (written by @operations, not by me) | Not mine, Minor-judgment | operations |
| `docs/devloop-outputs/2026-09-08-mc-steer-client-to-edge-handler/main.md` | Mine | — |

`proto/**` is a Guarded Shared Area (ADR-0024 §6.4) and Mechanical is disallowed there even for
comment-only edits. The `signaling.proto` hunk states *where a client is permitted to send*, so it
carries the §6.4 intersection. The ownership manifest lists only `protocol` for `proto/**` (the
`[protocol, auth-controller, security]` triple is `internal.proto`-specific), so the table row reads
`protocol`. **Per @main's ruling 2 and @security's N2, a trailer does NOT discharge Domain-judgment
(ADR-0024 §6.3), so I do not write this hunk**: `paired-protocol` owns it. I agree the wording with
them so the proto comment and my Rust-side comment cannot drift. @security has pre-authorised the
security half of the intersection for one sentence in substance — non-authoritative bootstrap,
selection is the send directive.

`docs/runbooks/mc-incident-response.md` is in scope per @main's ruling 3 and carries @operations'
Gate-1 hunk-ACK for one paragraph and that wording only (OPS-9).

`docs/TODO.md` is touched in **two separate hunks with two different owners**, so it gets two rows
(@code-reviewer, Gate 2): the R-15 task-25 clause is meeting-controller's own entry, while the
`mc_media_send_directives_total` fired-trigger correction is @operations' OPS-10 and is
Minor-judgment / operations exactly as the runbook paragraph is. A single "Mine" row would
under-scope the file and leave operations' hunk-ACK for that hunk untracked by the table the
Ownership Lens keys on. The `Approved-Cross-Boundary: operations` trailer therefore covers **both**
the runbook paragraph and this TODO hunk.

**Open ownership question, deliberately not pre-empted** (@main's instruction): that TODO entry is
owned "`observability` for the rule, `operations` for the runbook wiring". My edit corrects only the
entry's **trigger state** — the alert rule stays deferred to story task 21 and stays
`observability`-owned, and nothing in this diff proposes one — so @operations reads an
operations-only trailer as right. @code-reviewer has routed to @observability whether the joint
ownership pulls their co-sign. The row says `operations` pending that answer; **if @observability
claims it, the row and the trailer take the upgrade.** I had briefly written both owners in and
backed it out: an upgrade is the safe direction, but asserting someone else's co-sign before they
have given it is not mine to do.

`docs/runbooks/devloop-validation.md` is **written by @operations, not by me** — @main ruled the
correction rides in this commit. Its row is here so the table describes the whole commit; I am not
editing that file, to avoid two authors on one hunk.

No other touched path is a GSA. `crates/env-tests/**` is `test`-owned but not Guarded;
`crates/dt-guard/**` is not touched.

**Not touched, deliberately**: `packages/sdk-core/**` and `packages/web-app/**`. Verified against
the tree after @operations' OPS-5: story task 19 landed, and
`packages/sdk-core/src/media/lifecycle/AudioPipeline.ts:428-462` already publishes to
`directive.targets[0]` with the empty-target-set case handled per §5. `MeetingSession` still
`connectAll`s `joined.mediaServers`, but that is transport bootstrap, not selection. **The browser
fleet is therefore already steered by the directive; `media_servers` ordering steers only the
env-tests.** No client change is needed and none is in scope. `packages/web-app/e2e/mcMetrics.ts` is
the TS mirror of the Rust poll fixture; the new predicate is Rust-only and the reason is stated at
its definition, following the `instance_maps_equal` precedent.

**Touched after a mid-planning ruling — this row moved from "flagged" to "in scope"**:
`docs/runbooks/mc-incident-response.md:1884-1891` asserted the browser SDK does not yet honour the
directive, which task 19 inverted. I first proposed raising it to @operations for Gate 3 rather than
editing it. **@main ruled it in scope** (fix-don't-defer: ~3 lines, and it is an instruction *not to
act* in the section an on-call reaches when a participant is silent), and @operations granted an
`Approved-Cross-Boundary: operations` hunk-ACK scoped to that one paragraph and their supplied
wording. So it IS edited here, and the Classification table carries the row. Recorded this way
rather than silently rewritten, so the change of position is visible (@semantic-guard, Gate 2).

---

## Planning

### The mechanism, restated (and the wider class it names)

Instance language: *`media_servers` is unsorted and the client takes `.first()`, so it can land on
a handler with an empty edge set.*

Mechanism language: **MC produces exactly one artifact that decides where a participant's media is
placed — the `MeetingAssignment`. Any value MC hands a client that names a media handler must be
READ OUT of that artifact. A second list that also names handlers, ordered by something else, is
not a bug in its ordering; it is a second selection mechanism, and the ordering is only how the
divergence became visible.**

Restated that way the class is wider than the one site the task names, and every sibling is
same-owner (meeting-controller) and in files I am already editing:

1. **`JoinResponse.media_servers`** — the named instance. Redis order, consumed as a selection.
2. **The four `connectAll()` comments** — `assignment.rs:~129`, `connection.rs:~1275`, `~2297`,
   `~2343`. One assumption, four homes. The `:1275` copy is not a note, it is the *soundness
   argument* for treating a per-connection cache as a meeting-wide fact; a stale premise under a
   soundness argument is worse than a stale note. (@dry-reviewer point 2 — agreed, fixing the class.)
3. **Three `media_servers.first()` sites in `26_mh_quic.rs`**, not one: `join_with_registered_mh`
   (~:554), the media-connection-update test (~:926), and R-15 (~:1352). (@dry-reviewer point 4.)
   These are NOT all the same defect, and the distinction is the fix: `.first()` is legitimate when
   the caller needs *"some handler on which this meeting is registered"* (sites 1 and 2 — JWT
   accept/reject and a status report) and is a defect when it needs *"the handler carrying my
   edges"* (site 3). I will encode that distinction in prose at each site rather than mechanically
   rewriting all three, because rewriting sites 1-2 to follow the directive would make three tests
   depend on the §5 signalling path to test something else.

**Where I am NOT extending the class**: the browser SDK. `MediaTransport.connectAll` opens a
transport to every listed url and performs only the auth handshake; it selects nothing and
publishes no media yet. Its follow-through is `client`-owned, already filed at `docs/TODO.md` §993.

### Finding 0 — the MC server side is already correct, and the plan's first job is to PIN that

Verified against the tree, not assumed (the Lead asked me to confirm rather than trust):

- `build_send_directive` (`media_signaling/directive.rs:411`) walks `assignment.per_handler` and
  emits a target only for a handler whose `EgressStreamPlan.candidate_sources` contains the
  publisher. `HandlerUrls` (`directive.rs:351`) is an id→url *lookup*, not a selection, and is
  built once at `connection.rs:2011` from server-derived Redis data.
- `build_stream_assignments` (`media_signaling/assignments.rs:125`) walks the same
  `MeetingAssignment` and fills `media_handler_url` from the handler carrying the plan.
- Both fail closed: `DirectiveOutcome::HandlerUrlUnresolved` rather than an empty url;
  `SlotState::SourceUnreachable` with no `sender_id` rather than an empty-url "active" slot.

So the SSoT property **holds today and is untested**. The substantive MC-side deliverable is
therefore tests that would go red if it regressed, plus removing the second selection mechanism.
Per @dry-reviewer point 1: I add **no** new "which handler carries this participant's edges"
derivation. Everything reads `assignment.per_handler`, following the `planned_audio_slot_for`
(`connection.rs:2006`) precedent.

### Work item A — `media_servers` stops being a selection mechanism

**REVISED at planning round 2. `media_servers` is left UNSORTED; the change is the comment only.**

My first draft took @security's and @observability's "sort by `mh_id` AND comment". @security then
withdrew their own ask with an argument I could not answer, and I am taking it:

1. **No `mh_id`-keyed order can avoid the correspondence.** `edge_handler` selects the
   lexicographically smallest shared `mh_id`. Any total order on `media_servers` also keyed on
   `mh_id` therefore produces a positional correspondence with the authoritative rule — ascending
   makes `.first()` right today, descending just relocates the same defect to `.last()`. The only
   order that structurally cannot correspond is one keyed on something unrelated to placement,
   which is what Redis order already is. Unsorted is not the absence of a decision; it is the only
   order that keeps the two mechanisms independent.
2. **My proposed mitigation was false.** I had argued "after this task no in-repo consumer selects
   off the head". That is not true under work item D: env-test sites `~:554` and `~:926` keep
   `.first()`, legitimately. So the head-selection *pattern* stays live in the same file as the site
   being fixed, and under an ascending sort those two become templates that return the right answer
   for the wrong reason. Unsorted, a copy of one into a steering context fails — which is exactly
   how this defect surfaced (three of four connections on mh-1 was the system saying so loudly).
3. **The determinism buys nothing here.** Verified rather than assumed: `AudioPipeline.#applyDirective`
   never reads `mediaServers`; `MediaTransport.connectAll` opens transports to all of them so order
   is irrelevant to bootstrap; `join_tests.rs:316`'s fixture is single-handler; and env-test sites
   `~:554`/`~:926` work with either handler because **both** have the meeting registered — so there
   is no flake for sorting to remove there either (this retires @operations' OPS-2 flake concern on
   its facts, not by overruling it). No consumer diffs `JoinResponse` for byte-identity. Contrast
   `build_send_directive`, which sorts for a reason that *has* a consumer: generation
   change-detection asserts byte-identity.
4. **CLAUDE.md §Fail loudly; never mask is the tiebreak.** An ascending sort makes a misuse pattern
   produce correct output — a mask that suppresses the signal until per-participant placement lands
   and flips it silently.

**@dry-reviewer's addition, taken — the REFUSAL is recorded at the construction site.** Without it
the reversal has a half-life: what a future reader sees at `connection.rs:~2251` is nondeterminism
in a test-facing surface with no stated reason, and the obvious "fix" is to sort it. That pressure
is not hypothetical — @operations put the Redis-order flake argument on the record in this very
devloop. So the comment says in terms that the order is **deliberately not sorted**, that any
`mh_id`-keyed order would correlate with `edge_handler`'s placement rule and make `.first()`
accidentally correct, and that the list is left in an order unrelated to placement **precisely so
that selecting off it fails loudly**. It names this as a refusal, not an omission. It also
pre-answers the objection that would otherwise reopen it: if a handler-sensitivity flake ever does
appear at the two `.first()` test sites, the fix belongs at those sites (wait for registration, or
assert against whichever handler was picked), **not** at the list order.

**@observability's addition — the unsorted order is what gives C1 its teeth, and that is a two-way
dependency.** This is the sharpest reason found for the reversal and it postdates the ruling: C1
(`steering_follows_edge_placement_not_redis_order`) seeds `[mh-1, mh-0]` **precisely so that
list-order selection yields `mh-1` while the assignment places on `mh-0`**. Sorting the production
list would collapse those two answers into one and **silently disarm C1** — the test would keep
passing while no longer able to fail for the reason it exists, and nothing else in the repo would
notice. So the comment's job is now narrower and more concrete than "prevent a reintroduced
`.first()`": unsorted, **C1 itself is a real control on the reintroduction**, and the comment exists
to protect C1. The dependency is recorded in **both directions** — C1's comment names the unsorted
`media_servers` order as its precondition and points at `connection.rs:~2251`; the builder comment
names C1. Either artifact edited alone then reads as incomplete, which is this repo's established
shape for a control whose complement lives elsewhere (`media-telemetry-deny.yaml` §INCOMPLETE BY
DESIGN and its walker). One-directional pointers are how a complement gets retired by someone who
never opened the other file.

**And an honest cost line, per @observability point 5 — do NOT claim unsorted is free.** It leaves
the two legitimate `.first()` sites picking a Redis-order-arbitrary handler. @operations verified
that is not a real flake (both handlers answer those tests' question identically) and that the
arbitrariness buys cross-pod coverage — but the comment states the cost rather than presenting the
choice as an unclaimed win, because an unstated cost is what makes the next reader rediscover the
sort as obvious.

So: `connection.rs:~2251` keeps its Redis-order build, and gains a doc comment saying (a) it is
connection **bootstrap** data — the set of handlers this meeting is on; (b) **selection lives in
`SendDirective.targets` / `StreamAssignment.media_handler_url`**, read out of the forwarding
assignment; (c) this list has **no meaningful order**, `.first()` is not a selection, and its head
carries no relationship to placement. Per @security, the comment points at "no meaningful order"
rather than at explaining away an order I chose to add.

@observability and @operations both asked for the sort; I have put the reversal to them explicitly
rather than deciding it silently, and will not implement until both have seen the argument.

Consequence checked: `crates/mc-service/tests/join_tests.rs:316` asserts `media_servers[0]` against
a single-handler fixture (`mh-test-1`), so it is unaffected either way. No edit needed there.

### Work item B — the four `connectAll()` comments

They ARE one contract, so I correct the class (@dry-reviewer point 2), and I state the distinction
@dry-reviewer asked for, because the four are not made *identical* by the fix:

The routing input still describes every participant as reachable on every assigned handler. That
does not change, and the reason is not `connectAll()` — it is that **MC has no per-participant
placement input**: `MhAssignmentData` is a property of the meeting, not of a participant. What
changes is the *steering* half: the client is no longer left to pick from the list, it is directed
to exactly the handler the assignment placed its edges on. So each site gets: the fill rule
(unchanged), its real justification (no per-participant placement input, not client `connectAll`),
and what breaks when placement lands (`:1275`'s soundness argument keeps its "stops being sound
when…" paragraph, with the premise corrected). `assignment.rs:~129` additionally gains the
directed-handler contract sentence the task asks for by name.

### Work item C — tests that can actually fail

@test point 1, @security point 7 and @dry-reviewer's addendum all land on the same hazard, and I
agree with the addendum's diagnosis: **an order-inversion test written over a hand-built
`MeetingRoutingInput` is vacuous.** `per_handler` is a `BTreeMap`, `edge_handler` already sorts, and
`build_send_directive` accumulates into a `BTreeMap` — permuting there asserts a property the
*types* guarantee, and `assignment.rs:613`'s existing `canonical_ordering_is_stable_under_input_permutation`
is the shape a reader would wrongly copy.

The ordering that can actually vary is the `MhAssignmentData.handlers` **`Vec`** at the Redis
boundary, and it reaches the outcome by two distinct paths — `build_routing_input`/`routing_input_for`
(private, `connection.rs:2296`/`:2342`) into the assignment, and `HandlerUrls::from_pairs`
(`:2011`) into the urls. Both are private to `connection.rs`, so the only way to drive the real seam
is **through a real join**. Hence:

**C1 — the primary test, at component tier**, in `crates/mc-service/tests/media_client_signaling_integration.rs`,
using the existing `seed_meeting_with_handlers` + `AcceptLoopRig` + `Session::expect_directive/expect_assignments`:

- `steering_follows_edge_placement_not_redis_order` — seed **two** handlers in **inverted** Redis
  order (`[mh-1, mh-0]`, so `media_servers.first()` before the sort was `mh-1`, the handler holding
  the empty edge set — this is the live-cluster defect reproduced). Join, declare `{slot 0, AUDIO}`,
  read the `SendDirective` and `StreamAssignments`. Assert:
  - exactly one stream, exactly one target;
  - the target url `== "wt://mh-0:4433"` — a **literal**, independently pinned, not recomputed by
    any shared helper (@test point 1). It is the url of the handler the assignment places the edges
    on; the *nameable wrong answer this rejects* is `"wt://mh-1:4433"`, which is what list-order
    selection produced on the cluster, and the test comment says so;
  - `StreamAssignment.media_handler_url` for the active slot `==` the same literal — the two client
    -facing sides agreeing is the SSoT property;
  - `assert_ne!(target_url, "wt://mh-1:4433")` with a message naming the defect, as an explicit
    negative control.
- `redis_enumeration_order_cannot_change_where_the_client_is_steered` — run the same join twice
  against the two permutations of the seeded handler `Vec` and assert the two `SendDirective`s are
  **byte-identical** (`encode_to_vec`) *and* that both name `mh-0`. Equality alone is green when both
  are empty; the literal is what gives it teeth. This drives `MhAssignmentData.handlers` — the
  artifact whose order genuinely varies — exactly as @dry-reviewer's addendum requires.

**C2 — the focused unit tests the task names by name**, in `media_signaling/directive.rs` and
`media_signaling/assignments.rs`. N=2 handlers, one participant, both sides asserted against the
`mh-0` literal. These are deliberately *not* the order-inversion proof (see above — at this tier
inversion is normalised away by the map types, and I will say that at the test rather than write a
test that cannot fail). They are the correspondence proof at the smallest tier: directive target
handler == handler carrying the edges == stream-assignment handler.

### Work item D — the env-test fixture follows the directive

`crates/env-tests/tests/26_mh_quic.rs`:

- `mc_join` currently drops the WebTransport connection after reading `JoinResponse`, and the
  directive is only emitted from `compose_and_emit` in response to a `ReceiveCapability`
  (@security point 6a). So: extract `mc_join_session` returning a live session (conn + streams +
  `JoinResponse`); `mc_join` stays as a thin wrapper for the callers that only want the response.
- New `declare_capability_and_read_steering(&mut session)` sends `ReceiveCapability { slots: [{0, AUDIO}] }`
  and reads framed `ServerMessage`s until it has both the `SendDirective` and the `StreamAssignments`,
  skipping roster broadcasts, under a timeout. The steered url is
  `directive.streams[0].targets[0].media_handler_url`, asserted equal to the active slot's
  `media_handler_url`. **The expected value is MC's own output, never recomputed test-side** — no
  copy of `shared.sort(); shared.first()` anywhere in the fixture (@security point 6a).
- The other two `.first()` sites (@dry-reviewer point 4, @operations OPS-2) keep `.first()` — they
  want "any assigned handler", and both handlers have the meeting registered, so either answer is
  correct and there is no flake to remove — @operations verified this against the manifests rather
  than defending the claim (`mh-{0,1}-configmap.yaml`, both deployments, `service.yaml`'s two
  dedicated NodePorts on the same `targetPort: 4434`, and both host-mapped in `kind-config.yaml` AND
  `kind-config.yaml.tmpl`) and withdrew the flake argument. **And it runs the other way**: because
  the pods are symmetric and the order is arbitrary, these two tests hit mh-0 and mh-1 across runs,
  which is *coverage of both pods over time*. A per-pod drift (stale image on one instance, a netpol
  that landed on one Service, one pod not ready) surfaces as an intermittent red under Redis order
  and would be **invisible** under an ascending sort pinning every test to mh-0 forever. That is an
  independent reason for the unsorted ruling and it goes in work item A's comment: the arbitrariness
  is not merely tolerated, it is load-bearing. **The note wording tracks the unsorted ruling** (@main):
  NOT "deterministic by the `mh_id` ordering" — that property no longer exists and the note would
  assert something the code does not have. Each note says instead: this is *any* assigned handler,
  taken in **arbitrary Redis order**, deliberately non-authoritative, and **not** the steered one; a
  caller that needs the steered handler must read the send directive. Per @dry-reviewer's ask,
  each also says why the two answers may *appear* to agree and when that stops: the steered handler
  is always the lexicographically smallest assigned `mh_id`, so an arbitrary pick coincides with it
  some of the time and by accident — never because the two are the same question. A copy of either
  into a steering context therefore fails loudly rather than passing for the wrong reason.
- **No fallback** (@security point 2): zero streams, zero targets, an empty url, or >1 target each
  panic with their own message. >1 target specifically means §9 multi-handler send landed and this
  fixture must be revisited — it is not silently `.first()`-ed.

### Work item E — the ordering gate (the mechanism, named)

@security point 6b, @observability point 1 and @dry-reviewer point 5 all correctly conclude there is
**no shared key** between the steered url (MH NodePort advertise address) and Prometheus `instance`
(pod IP:8083). I have no mapping source they missed. I reject the port→ordinal table for the reasons
all three gave: it re-encodes `mh-{0,1}-configmap.yaml`, and it fails **open** — a stale mapping
picks the wrong instance and the gate silently returns to being satisfiable by the handler the
client is not on.

**Chosen mechanism — @observability's route (a) / @security's suggestion: identify the instance
empirically, from the connection the test itself just made.** Nothing is computed from topology;
the instance is *observed*. Sequence:

1. `applied_baseline` = per-instance `mh_media_policy_applies_total{outcome="applied"}`, taken
   **before** the MC join (unchanged from today).
2. MC join → follow the directive to the steered url (work item D).
3. `connect_baseline` = per-instance `sum by (instance) (mh_webtransport_connections_total)`, taken
   immediately before connecting. **No `status` selector**: the instance that accepted the
   connection increments regardless of what happens downstream, so the identity probe stays valid
   even when a binding declines — which keeps "I could not identify the instance" separate from "the
   client was refused".
4. `connect_wt(steered_url)` + `send_jwt_on_bi_stream`.
5. Poll until **exactly one** instance exceeds its own `connect_baseline`. That instance is the
   steered one. Zero → panic; more than one → panic. Never `.first()` of the risen set — that would
   rebuild the original defect one layer up.
6. Poll until `applied[steered_instance] > applied_baseline[steered_instance]`.
7. Only then send datagrams.

**Fail-closed with distinct reason tokens** (@observability point 3): step 5 and step 6 have
different owners and different remedies, so they get separate assertions with separate messages —
step 5's names the steered **url**, the risen set and the full baseline/current maps via
`format_instance_map`, and says "could not identify the steered instance"; step 6's names the
identified instance and says "the steered instance never applied a policy". They are never folded
into one message, because a broken probe triaged as a steering bug gets "fixed" by relaxing back to
instance-agnostic.

**Positive control** (§Assertion Vacuity mechanism 5): step 5 *is* the positive control, and it is
its own assertion. It cannot pass unless our own connection was observed on some instance, so a
wholesale-empty Prometheus reading is structurally distinguishable from "present, not incremented"
— `format_instance_map` already renders the empty case self-describingly and I reuse it rather than
hand-rolling.

**Bounding misattribution** (@observability's caveat on route (a)): the window between steps 3 and 5
is one connect plus one poll interval, the test is `#[serial_test::serial(mh_notifications)]`, and
the ambiguity case panics rather than guessing. That is the bound; I do not claim it is airtight
against a genuinely concurrent connector, and the failure is loud when it breaks.

**Where the helper lives** (@dry-reviewer point 3, @observability point 4): in
`crates/env-tests/src/fixtures/metrics.rs`, beside `poll_until_any_instance_above` — the deliberate
ONE Rust poll loop. Shape: one pure predicate/selector pair with unit coverage
(`instances_exceeding_baseline` returning the sorted risen set; `instance_exceeds_baseline` for the
named-instance case) plus one generic poll the existing `poll_until_any_instance_above` is
re-expressed on top of, with **no behaviour change** to it. No `loop { query; check; sleep }` in
`26_mh_quic.rs`. The two `InstanceCounters` trap paragraphs and the residual-false-pass note on
`any_instance_exceeds_baseline` are preserved verbatim. TS-mirror question is answered explicitly at
the new definitions (Rust-only — the TS specs have no instance-keyed gate), following the
`instance_maps_equal` precedent rather than skipping the question.

**Accepted refinements from planning round 2:**

- **OPS-6 (@operations, blocking) — the probe's premise must be ENFORCED, not coincidental.**
  Accepted. `test_mh_accepts_valid_meeting_jwt` (:578), `test_mh_rejects_forged_jwt` (:618) and
  `test_mh_rejects_oversized_jwt` (:650) all open MH connections and do NOT carry R-15's serial key,
  so `cargo test` runs them concurrently and their increments land in the same
  `mh_webtransport_connections_total` series my no-`status`-selector query reads. The risen set in
  step 5 could therefore consist entirely of someone else's connection — mechanism 5 verbatim: a
  positive control that passes on ambient state and in the author's hands. **Fix: all three join
  R-15's existing `#[serial_test::serial(mh_notifications)]` key** (one key rather than a second, to
  avoid multi-key semantics), with a note at each saying the key is load-bearing for R-15's
  instance-discovery probe and not decorative. Three attribute lines; without them the mechanism
  does not establish what its failure message claims. Note the unsorted decision does not retire
  this — it changes *which* handler those tests hit, not whether they pollute the series.
- **OPS-7 (@operations) — the budget carries its derivation.** The 45s is written as
  `3 x Prometheus scrape_interval (15s, infra/kubernetes/observability/prometheus-config.yaml)` in a
  comment, so a future scrape-interval change has a findable dependant. Same single-source reasoning
  as OPS-1.
- **OPS-8 (@operations) — step 5's two failure modes get two messages.** Zero risen = our connection
  was never observed (environment: scrape gap, MH unscraped, Prometheus behind) → operator lane.
  More than one risen = test isolation broke (OPS-6's class, or a genuinely concurrent connector) →
  implementer lane, and the message says in terms that the remedy is isolation, **not** a wider
  budget. Folding them is the same hazard I refused between steps 5 and 6.
- **E-1 (@observability)** — discovery budget is **45s**, not 30s. `scrape_interval` is 15s
  (`infra/kubernetes/observability/prometheus-config.yaml:27`), so 30s is only two scrape
  opportunities and a slow scrape would spend the budget and panic with the token that reads as a
  steering defect — reintroducing the confusion the two-token split exists to prevent. The timeout
  message names the 15s scrape interval and points at scrape lag / target health before steering.
- **@security round-3 — the third direction, closed structurally rather than documented.** The
  ambiguity panics cover zero-risen and >1-risen, but not **"exactly one rose and it was not ours"**:
  a foreign connection in the stale pre-baseline window shows as a rise, and if our own increment is
  not yet scraped at that instant the risen set has exactly one member — theirs — and every guard
  passes. That is a false *positive* identification, the opposite direction from the ambiguity case,
  and it silently restores the instance-agnostic weakness this task exists to remove. **Taking the
  structural close, not the note**: before connecting, poll `connect_baseline` until two consecutive
  reads are identical (`instance_maps_equal` already exists and is exactly this comparison). A stable
  baseline means every prior connection has been scraped, so any subsequent rise is attributable to
  us. One extra poll in the common case, no new predicate, and it is a *condition* rather than a
  guess — so it does not reintroduce the sleep this design is right to avoid. This subsumes E-2: the
  residual becomes a precondition. Per @operations, the stabilise loop gets its **own budget and its
  own THIRD reason token**: "the baseline never stabilised" means connection churn or a scrape
  problem — operator lane — and must never be reported as "could not identify the steered instance".
  **Reachable single-threaded, which is what makes it worth closing rather than noting** (@security,
  round 4): the residual-false-pass note at `fixtures/metrics.rs:283-305` records that an instance
  merely *missing* from a baseline read — target unscraped beyond `query.lookback-delta`, or a
  transient empty result — and then reappearing with its full prior value shows as a rise it did not
  earn. Applied to this probe, "exactly one instance rose" can be satisfied by an instance our
  connection never touched **with zero concurrency involved**, through a documented window on the
  exact rule the probe is built on.
  **Ordering**: stabilise runs **before** `connect_baseline` is read — its whole job is to make that
  baseline complete. So: stabilise → read `connect_baseline` → connect → discover risen instance.
  **Numbers, each derived at the call site in its own words rather than copied**: settle interval
  **16s**, because two equal reads separated by more than one 15s-SLA scrape interval prove every
  target has been scraped at least once in the window — which is precisely what makes "absent from
  baseline" mean "genuinely not there" rather than "not yet scraped". Budget **90s** — @security argued 60s
  ("this waits for quiescence, not an event; non-stabilisation means something else is actively
  connecting, so 90s buys a conclusion already available at 30s") and @observability argued 90s
  (convergence needs two reads one scrape apart, and one foreign scrape landing mid-window costs
  another full round; under-sizing times out on a merely-busy cluster and panics with a message the
  reader attributes to steering — E-1's failure mode one layer up). **Taking 90s**: @security's is a
  preference about diagnosis speed on failure, @observability's is about avoiding a FALSE red, and
  the common case returns at ~16s either way, so the budget is only ever spent on the path where
  being wrong is expensive. The 16 coinciding with `wait_for_notification_counter_stable`'s 16 is a
  coincidence of two independent derivations, not a shared constant, and must not be hoisted — and
  after this change **both** the 16s and the 90s coincide with `wait_for_notification_counter_stable`'s
  values, in two callers of one loop in one file. That adjacency is closer than the case that
  prompted the in-tree rule, and a shared `DEFAULT_SETTLE` is exactly what the next tidy-up reaches
  for. Both parameters therefore land in the register at
  `crates/mc-service/src/media_routing/assignment.rs:299-305`, closing with its phrase verbatim:
  **"Same number, different reasons, opposite drift obligations."** A reader who recognises the
  pattern stops.
  **Residual stated at the probe, per @dry-reviewer**: the freshness gate and OPS-6's serial keys are
  **disjoint controls on one hazard, and neither closes it alone** — the serial keys close in-suite
  concurrency, the freshness precondition closes the stale-scrape window, and a connector outside
  this test binary is closed by neither. Saying so at the site stops the next person finding one
  inconvenient and removing it as redundant.
  **@dry-reviewer's follow-on, accepted**: the freshness gate is not just a reused predicate, it is
  a reused LOOP — `26_mh_quic.rs:318` `wait_for_notification_counter_stable` is already
  read → settle → read → `instance_maps_equal` → retry-to-deadline. Writing mine inline would put
  two stabilise loops in one file, which is point 3's shape applied to the stabilise idiom. So the
  stabilise loop is generalised into `crates/env-tests/src/fixtures/metrics.rs` beside the delta
  poll, and `wait_for_notification_counter_stable` is re-expressed on it. Two constraints taken:
  (a) **settle interval and budget are PARAMETERS, not shared constants** — 16s/90s are the
  notification path's decisions with their own argued reasons (one 15s-SLA scrape must land between
  reads; MH's fire-and-forget notify → gRPC → MC counter → scrape chain exceeds 30s), and my
  connect-baseline gate observes a connection the test itself is about to make, a different chain
  deserving its own numbers with its own reason. One loop, N callers each making their own decision:
  the loop is the SSoT, the budgets are not. (b) **The rustdoc splits where it argues** — the
  churn-robustness paragraph (why per-instance map comparison survives a stale series expiring
  between reads) is about the LOOP and moves to the shared home; the scrape-chain budget paragraph
  is about the CALLER and stays at `wait_for_notification_counter_stable`. Same fail-loud shape:
  panic naming both maps via `format_instance_map`, with a message distinct from the delta poll's so
  a stabilise timeout is never triaged as "the counter did not increase".
- **E-2 (@observability)** — the misattribution residual is **wider than I first wrote it**: it is
  connect + one poll interval **plus one scrape interval before the baseline read**, because
  `connect_baseline` reflects the last scrape rather than the instant it is read. A connection to
  another instance in that ~15s window appears as "risen" and trips the ambiguity panic even when
  steering is fine. Taking **document, not close**: the ambiguity panic message offers "something
  else connected within the 15s scrape window before the baseline read" as a candidate cause, so
  triage lands there rather than on steering. The `wait_for_notification_counter_stable`
  double-read would close it but costs 16s on a path already at ~105s, and @observability did not
  require it.
- **E-3 (@observability)** — the gate must not overclaim. One sentence at the site says what it does
  and does not prove: it proves *some* policy application succeeded on the instance the client is
  connected to, after baseline — **not** that this meeting's policy did. A concurrent meeting
  registering on the same handler satisfies it. That gap is a deliberate consequence of ADR-0036
  §11's flat no-meeting-identifier rule, not an oversight to be "fixed" with a label.
- **@dry-reviewer must-fix — ONE comparison rule, three read shapes.** The
  "an instance absent from `baseline` counts as `0.0`" convention (and its documented residual
  false-pass window) gets exactly one home: `instances_exceeding_baseline(baseline, current) -> Vec<String>`
  (sorted, for deterministic diagnostics). `any_instance_exceeds_baseline` becomes
  `!instances_exceeding_baseline(..).is_empty()`; the named-instance form becomes a membership check
  against the same set — **and I will check whether it is needed at all** before adding it, since
  the risen-set selector may serve both call sites. Both `InstanceCounters` trap paragraphs and the
  residual-false-pass note move onto whichever function is the rule, with a one-line pointer on the
  wrappers. Pinned by a unit test that the wrapper and the rule agree on the rollover case already
  covered at `metrics.rs:407-417`, because `any_instance_exceeds_baseline` has a TS mirror
  (`packages/web-app/e2e/instanceCounters.ts:110`) and re-expression is safe only if it is a true
  wrapper rather than a second implementation that happens to agree today.
- **N1 (@security)** — the re-expressed `poll_until_any_instance_above` passes the **existing**
  `any_instance_exceeds_baseline` function as its predicate, not a re-derived equivalent, so the
  three live call sites (`26_mh_quic.rs` :354, :807, :1165) keep their semantics **by
  construction**. "No behaviour change" is exactly the claim that reads true at review and goes
  quietly wrong — a weakened gate does not fail, it passes earlier.
- **@test (a)** — as much logic as possible lives in the unit-tested pure predicates; the
  `#[ignore]`d test body holds only thin orchestration that cannot be unit-tested.
- **@test (b)** — an at-site note at the gate records that **the orchestration is unexercised until
  task 26 un-ignores R-15**, so the residual-false-pass window is documented where the next reader
  will see it, beside the residual note already preserved.

**Budgets and worst-case wall clock** (@operations OPS-4). Step 5 (discover the steered instance)
gets **45s** (three scrapes at the 15s `scrape_interval`) — it observes a connection the test has already made, so a slow answer is an
environment fault, not a legitimate wait; it panics on ambiguity *immediately* rather than spinning
out the budget. Step 6 keeps today's **60s**. Worst case per attempt is therefore **~215s**, up from 60s — and note the number is **not** the sum of the three
budgets. `poll_until_stable` evaluates its deadline AFTER a completed round, so its effective
ceiling is `timeout + settle + 2 query RTTs` ~= 106s rather than 90s (@operations OPS-11); the two
delta polls overshoot only by their 2s interval. So: ~106s stabilise + ~47s discovery + ~62s apply.
Both baselines stabilise CONCURRENTLY under one `tokio::join!`, so adding the second one costs no
extra wall clock. Up and only once task 26 un-ignores the test. In the overwhelmingly common case step 6 returns on
its first poll, because `RegisterMeeting` fires at MC join — well before the client connects.

**Reviewability is the only control here** (@security's closing note): R-15 stays `#[ignore]`d, so
Layer 7 will not execute this path. I will additionally exercise the new fixture helpers' pure
predicates under `cargo test -p env-tests --lib`, which does run.

### Gate FIRE / APPLY accounting (ADR-0036 §"A control's coverage must be demonstrated, not asserted")

Not strictly triggered — the ADR routes this through ADR-0031's block, whose trigger is a plan
touching metrics or alerts, and this plan adds neither. Written anyway on @security's ask, because
the answers otherwise exist only scattered across five reviewer threads and none of them is the
plan. Both failure directions are silent and both read as coverage.

| Gate | Does it FIRE? | Does it APPLY? |
|---|---|---|
| **Steered-instance discovery** (step 5, new) | Zero risen → panic; >1 risen → panic. Both reachable, both with their own message and lane. | Premise: a rise is attributable to *our* connection. **This is the half with a known hole** — the `lookback-delta` window at `fixtures/metrics.rs:283-305` makes an unearned rise reachable **single-threaded**, which is what the freshness gate closes. |
| **Baseline freshness** (new) | Non-stabilisation within **90s** → panic, third distinct reason token. It fires on **failure to converge across rounds**, never on a single inequality: counters are monotonic, so the other cause of a non-equal round is an instance *appearing* between reads — which is the gate's own success path (absent from read 1 because unscraped beyond `lookback-delta`, present in read 2), producing exactly one non-equal round **by construction**. A non-equal round is routine operation, not evidence of a concurrent connector; pod rollover mid-test is the same shape. Naming this is what stops the next reader concluding "an idle cluster converges immediately, so the budget is padding" and trimming the margin the gate's normal operation consumes. | Premise: a stable pair of reads separated by more than one scrape interval means every target has been scraped, so "absent from baseline" means genuinely absent. Rests on the 15s scrape SLA — which is why the inter-read wait is a correctness precondition and not a budget (E-4). |
| **Policy-applied** (step 6, existing, now instance-keyed) | Timeout → panic naming the identified instance. | **This is the half the task exists to fix.** The old gate fired correctly and *did not apply*: instance-agnostic meant mh-0 applying satisfied it for a client on mh-1. The ADR's "alive, never applied" row exactly — it went green through fifteen seconds of a timeout and nobody could tell, because the APPLIES half was never stated. Recording that is what stops the next instance-agnostic gate being written. |

**Residual stated at the probe, not implied-covered**: OPS-6's serial keys and the freshness gate are
**disjoint** controls on one hazard and neither closes it alone — serial keys close in-suite
concurrency, freshness closes the stale-scrape window, and a connector from **outside this test
binary** is closed by neither. The sliver is named so the two do not read as belt-and-braces and get
pruned as redundant.

**C1's `assert_ne!(target_url, "wt://mh-1:4433")` is labelled as a FIRE demonstration**, not as an
extra assertion: it is the injected adverse condition — the answer list-order selection produced on
the live cluster — and the test comment says so.

### Filed, not fixed (@dry-reviewer)

Framed-`ServerMessage` decode reaches a fourth home with work item D
(`26_mh_quic.rs:513`, `:918`, `24_join_flow.rs:135`, plus mine). The shared home is structurally
unreachable: `proto-gen`/`wtransport`/`prost`/`bytes` are `[dev-dependencies]` of `env-tests`, so
`crates/env-tests/src/fixtures/` cannot name `ServerMessage`, and there is no `tests/common/`
precedent there. Promoting those dev-dependencies to fix a comment-level concern would make every
consumer of the lib link `wtransport` — not worth it. My helper stays local to `26_mh_quic.rs`;
@dry-reviewer files the extraction under `docs/TODO.md` §Cross-Service Duplication as a `test`-owned
design call.

### Work item F — R-15 prose, `#[ignore]` retained

The `#[ignore]` attribute is **not** removed. Its reason text and the doc comment currently describe
task 25's defect as open; both are rewritten to say task 25 landed (steering now follows the
assignment placement; the gate keys on the steered instance) and that **only task 26's datagram
receive-path gap remains** — `mh_media_frames_forwarded_total` at 0 with every
`mh_media_frames_dropped_total{reason}` series also 0. The `docs/TODO.md` R-15 entry gets the same
correction to its task-25 clause and stays open.

### Work item G — `signaling.proto` comment — NOT MINE TO WRITE

@security's N2 and @main's ruling 2: a trailer is the Minor-judgment remedy and does not discharge
Domain-judgment; ADR-0024 §6.3 requires owner-implements or `--paired-with`. `paired-protocol` is in
the loop and **owns that hunk**. My job is to agree the wording with them so the proto comment and
the Rust-side comment at the `JoinResponse` builder cannot drift — one sentence in substance:
*non-authoritative bootstrap; selection is the send directive / stream assignment.* The row stays in
the Classification table with Owner `protocol`; it leaves my edit list.

**Final wording + trailers — supplied by `paired-protocol` (owns this hunk).** Placement:
`JoinResponse.media_servers` (replacing the terse `// Multiple handlers`), extending the existing
"one name for one concept" cross-ref across the three `media_handler_url` fields. Comment-only — no
field/tag/message/enum/option change, no `buf breaking` impact. Agreed with @implementer (Rust
comment at `connection.rs:~2251` shares the substance; the `mh_id`-correlation *mechanism* is kept
Rust-side because it names a mutable MC-internal placement fact and its audience is the list's
builder, not the wire consumer) and co-signed by @security. Comment text:

```proto
  // Connection bootstrap only: the media handlers this meeting is registered on,
  // for the client to open transports to. NON-AUTHORITATIVE for send placement —
  // this list does not decide where a client sends. Selection is the send
  // directive: `SendTarget.media_handler_url` (matching the active slot's
  // `StreamAssignment.media_handler_url`), both read out of the forwarding
  // assignment. This list is deliberately left unordered and is not the
  // selection mechanism: relying on its order — selecting off its first
  // element — silently diverges from the directive. Its first element is not a
  // selection and carries no relationship to placement.
  repeated MediaServerInfo media_servers = 4;
```

Trailers for the commit (§6.4 intersection — proto × auth-routing-policy):

```
Approved-Cross-Boundary: protocol media_servers comment on JoinResponse documents it as non-authoritative bootstrap; handler selection authority is the send directive / stream assignment per ADR-0036 §5; comment-only, no wire-shape change
Approved-Cross-Boundary: security media_servers non-authoritative comment; selection authority is the send directive per ADR-0036 §5, and the anti-order clause forecloses the head-selection defect this task fixes
```

### Work item H — runbook and TODO staleness that this path created (in scope per @main)

**H1 — `docs/runbooks/mc-incident-response.md` (~:1884-1891), @operations OPS-9, hunk-ACK granted.**
The paragraph tells on-call the browser SDK does not honour the directive, that failures below are
invisible to users, and that none of it is pageable. Task 19 inverted all three, in exactly the path
this task changes. That is not a stale note — it is an instruction *not to act*, in the section an
on-call reaches when someone reports silence. Replaced with @operations' supplied wording verbatim
(this path is live and user-visible; `AudioPipeline` publishes to `directive.targets[0]`; a client
never told to send produces nothing with no error and no absent-frame signal; these counters are the
only evidence; no alert rule keys on them yet, owed at task 21; absence of a page is not absence of
the failure). **That paragraph only** — if the edit needs to grow, back to @operations.

**H2 — `docs/TODO.md` §Media Path Obligations, the `mc_media_send_directives_total` alert entry,
@operations OPS-10.** Its own `TRIGGER: story task 19` has FIRED, but the entry still reads "No
alert was added, and that is correct TODAY: the browser SDK does not yet honour the directive". That
justification is now false, so task 21's implementer would read a live obligation as a pending one.
Two sentences recording that the trigger fired at task 19, that the "correct today" justification no
longer holds, and that the gap is live. The rule itself stays deferred to task 21 and stays
`observability`-owned — genuinely task-sized, and not asked for here. I am already editing this file
for the R-15 clause.

### Telemetry

**Zero new metrics, zero new labels, zero new log or metric macros inside `media_routing/`,
`media_signaling/` or `media_admission/`** — those modules are deliberately telemetry-free and I am
not making the steering fix the first exception (@observability points 5 and 6).
`mc_media_send_directives_total{outcome,key_custody}` already covers directive emission. No meeting
id, no participant id, no `sender_id`, no url-valued label anywhere. No change to the scrape config
and no `labelmap` (@observability point 2). `key_custody=operator` unchanged.

### Security invariants I am holding to

- The url source does not widen. `SendTarget` / `StreamAssignment` / `media_servers` all resolve
  from server-derived `MhAssignmentData` via `HandlerUrls`. `ParticipantActor::mh_statuses` (the
  client-supplied `MediaConnectionUpdate` url) stays confined to `participant.rs` and never becomes
  an input here. The existing `a_client_supplied_handler_url_never_reaches_a_send_target` test
  (`media_client_signaling_integration.rs:1071`) is the standing guard and stays green.
- No fail-open fallback to `media_servers.first()` when the target set is empty or a url is
  unresolved — in MC (already `HandlerUrlUnresolved`, fails closed) or in the env-test (work item D).
  An empty target set stays a *specified success* per §5.

### Verification

`scripts/layer1.sh` … `layer6.sh` (`layer-all.sh` locally), plus:
`cargo test -p mc-service`, `cargo test -p env-tests --lib` (new fixture predicates),
`cargo check -p env-tests --tests --features all` and `cargo clippy -p env-tests --tests
--features all -- -D warnings` (the `-D warnings` matters — see below), and `scripts/guards/simple/validate-cross-boundary-classification.sh` on this
main.md.

**The `--features` flag is load-bearing, not incidental.** `crates/env-tests/tests/26_mh_quic.rs`
carries `#![cfg(feature = "flows")]` and `env-tests` declares no default features, so a featureless
`cargo build -p env-tests --tests` cfg-strips the whole file away and reports success having
compiled none of it. **This plan originally listed that featureless form as the verification** —
see §Devloop Verification Steps for what it cost.

### Open questions for reviewers

1. **@security / @observability / @code-reviewer** — sort `media_servers` (my plan) or leave it
   unsorted with the comment only? Sorting manufactures a coincidence between a non-authoritative
   order and the authoritative placement rule. I lean sort + explicit trap comment; I will take
   either.
2. **@test / @dry-reviewer** — do you accept that the order-inversion proof lives at component tier
   (C1, driving `MhAssignmentData.handlers`) rather than as a unit test, given that the unit tier's
   `BTreeMap`s normalise the permutation away and a unit-tier inversion test could not fail?
3. **@team-lead** — the `signaling.proto` comment needs a `protocol` co-sign. Is `protocol` on this
   team, or do I raise it as a pairing request?

---

## Implementation Summary

### The featureless verification command was VACUOUS, and it hid a broken tree

`crates/env-tests/tests/26_mh_quic.rs:72` carries `#![cfg(feature = "flows")]` and `env-tests`
declares **no default features**. So the command this plan originally listed —
`cargo build -p env-tests --tests` — **cfg-strips the entire file away and reports success having
compiled none of it**: exit 0 in 0.13s against a tree where that file does not parse.

That is exactly what happened. A `tokio::join!` introduced by a review fix was never closed, so the
whole three-step gate sat inside the macro's argument list, and every cheap check I had reported
clean. Layer 1 cannot catch it either — its lint lane is `cargo clippy --workspace --all-targets`
and its compile lane `cargo build --workspace`, neither passing a feature flag. **The only thing in
the pipeline that compiles this file is Layer 7's `cargo test -p env-tests --features all`, which
needs a live cluster.** Found by @observability, who ran the featured command rather than trusting
the featureless one.

Review-protocol §Assertion Vacuity **mechanism 5** verbatim: the subject ran where it could observe
nothing and reported clean, indistinguishable from a real pass. It is worse than the textbook case
in two ways — the blind command was written into the *plan* as the verification, so the next person
inherits it; and it defeated a check introduced precisely to be careful, because R-15 stays
`#[ignore]`d and would not otherwise be exercised.

**Every env-test verification command must carry `--features all` (or `--features flows`).** The
featureless form is not a weaker check; it is not a check.

**And it must carry `-D warnings`, for the same reason one level down.** `scripts/lang/rust/lint.sh`
runs `cargo clippy --workspace --all-targets -- -D warnings`, so `-D warnings` IS the project's
standard. A recorded command that omits it passes where the gate would fail — which is the same
defect class as the featureless command it replaced: a verification that is quietly weaker than the
thing it stands in for. Caught by @observability, and it was live: the refactor orphaned two
functions (`mh_notification_counter`, `policy_apply_counter`) and the file failed `-D warnings`
under the feature flag while passing everything I had run. Both deleted, with the load-bearing
`instance`-trap prose re-homed onto `notification_promql`, which survives.

### Positive control on the verification command itself

Not asserted that the corrected command works — demonstrated, by making it fail on purpose. A
deliberate type error (`let _x: u32 = "not a u32";`) was appended to
`crates/env-tests/tests/26_mh_quic.rs`, then both commands were run against that identical tree:

| Command | Result on the deliberately-broken tree |
|---|---|
| `cargo check -p env-tests --tests --features all` (corrected) | **RED — exit 101** |
| `cargo check -p env-tests --tests` (the original, featureless) | **GREEN — "Finished" in 0.30s** |

The file was then restored and the featured command re-ran green, with `git diff` confirming no
residue. That side-by-side is the whole finding in two lines: the two commands disagree completely
on a tree that does not compile, and the one this plan originally recorded is the one that says
everything is fine.

@security ran the same control independently on the reviewer side before re-affirming their verdict.
Both runs are recorded because the method is the reusable part, not the result.

### Mutation evidence — the tests are demonstrated, not asserted

Flipping `edge_handler`'s `shared.first()` to `shared.last()` reds **all four** new unit tests and
the component-tier `steering_follows_edge_placement_not_redis_order`.

**The detail that matters more than the count**: on
`redis_enumeration_order_cannot_change_where_the_client_is_steered`, the mutation reds the
**literal** arm — `left: "wt://mh-1:4433", right: "wt://mh-0:4433"` — while the **byte-identity arm
stays green**, because both permutations produce the same *wrong* answer and therefore still encode
identically. So the byte-identity assertion alone would have been vacuous under exactly the defect
it exists to catch. That is the demonstration that the literal is load-bearing rather than
decorative, and it is why the plan refused to let equality carry the test.

**The one vector mutation cannot reach**, raised by @dry-reviewer: whether the expected url flows
from the *seeding* path, which would make the assertion "MC echoed the url we gave it". It does not,
and the reason is stated at the test site — the seed supplies **both** urls, so MC is handed a
two-element choice set whose members differ and the assertion pins which element it selected. There
is no single "the url" to echo. The residual coupling runs the right way: if `mh_handler`'s format
changes, the literals go red rather than silently tracking it.

### Finding 0 confirmed, and it shaped the work

MC's server side already satisfied the SSoT property: `build_send_directive` and
`build_stream_assignments` both read the handler out of `assignment.per_handler`, and
`HandlerUrls` is an id→url lookup rather than a selection. **No production derivation changed.**
No `directed_handler_for()`-style helper was added; nothing recomputes placement. What the property
lacked was any test that would go red if it regressed, and the second selection mechanism
(`media_servers` + `.first()` consumers) was still live.

Verified by mutation rather than asserted: flipping `edge_handler`'s `shared.first()` to
`shared.last()` turns all four new unit tests and the component-tier
`steering_follows_edge_placement_not_redis_order` RED. Reverted after.

### `media_servers` — comment only, deliberately unsorted

The builder gained a doc comment covering: bootstrap-only; selection is
`SendTarget.media_handler_url` / the active slot's `StreamAssignment.media_handler_url`; the order
is **deliberately not sorted** and that is a refusal, not an omission (any `mh_id`-keyed order
correlates with `edge_handler`'s rule — ascending makes `.first()` accidentally correct, descending
relocates it to `.last()`); sorting would **silently disarm** the component test, which seeds
`[mh-1, mh-0]` precisely so list-order selection differs from placement; and the honest cost, namely
that two legitimate env-test `.first()` sites get an arbitrary handler.

The dependency is **two-way**: the builder names the test, the test names the builder as its
precondition. Either edited alone reads as incomplete.

### The four `connectAll()` comment homes, corrected as a class

`assignment.rs::RoutingParticipant::handlers`, `connection.rs::MediaSignalingContext::handlers`,
`build_routing_input`, `routing_input_for`. The fill rule did not change; its **justification** did.
It is not "clients `connectAll()`" — it is that **MC has no per-participant placement input**
(`MhAssignmentData` is a meeting property; nothing on the roster records which handler a participant
reached). Each site now separates the routing INPUT from steering, and `RoutingParticipant::handlers`
carries the directed-handler contract by name. The `MediaSignalingContext` copy keeps its
"stops being sound when…" soundness argument with the premise corrected.

### Tests

- **Component tier (load-bearing)**, `media_client_signaling_integration.rs`:
  `steering_follows_edge_placement_not_redis_order` and
  `redis_enumeration_order_cannot_change_where_the_client_is_steered`. Both drive a real join with
  `MhAssignmentData.handlers` seeded in inverted order — the seam whose order genuinely varies,
  reachable only through a join because `routing_input_for` and `HandlerUrls::from_pairs` are
  private. Expected urls are **written literals** with the nameable wrong answer asserted against;
  the `assert_ne!` is labelled as the FIRE demonstration.
- **Unit tier**, `directive.rs` and `assignments.rs`: the N=2 correspondence tests the task names,
  each with a premise check that the assignment really placed the edges on one handler and that the
  other handler is present-but-empty. Their order-inversion siblings carry an at-site note saying
  they are deliberately **not** the inversion proof and why (`BTreeMap` + `edge_handler`'s sort
  normalise the permutation away at this tier, so such a test could not fail).

### Env-test fixture and the ordering gate

`mc_join` was split: `mc_join_session` keeps the WebTransport session OPEN (the directive is only
emitted in response to a `ReceiveCapability`, never at join), and `mc_join` stays a thin wrapper.
`follow_mc_steering` declares one audio slot, reads the `SendDirective`, cross-checks that the
active `StreamAssignment` names the same handler, and panics distinctly on no-directive / zero
targets / >1 target / empty url. **No fallback to `media_servers.first()`, and no copy of the
placement rule anywhere in the fixture.**

The gate is three steps with three distinct reason tokens:

1. **Baseline freshness** (`poll_until_stable`, 16s settle / 90s budget) — makes "every target has
   been scraped" a precondition, closing the residual-false-pass window that makes an unearned rise
   reachable **single-threaded**. Operator lane.
2. **Steered-instance discovery** (45s = 3× the 15s `scrape_interval`, derivation in the comment) —
   observes which instance's `mh_webtransport_connections_total` rose. No `status` selector, so the
   probe survives a declined binding. Panics on zero (operator lane) and on >1 (implementer lane /
   isolation, explicitly "not a wider budget"); never `.first()`s the risen set. This is also the
   mechanism-5 positive control.
3. **Policy applied on that instance** (`poll_until_instance_above`, 60s) — with a sentence saying
   what it does and does not prove, since §11 forbids a meeting id on the metric.

**OPS-6**: the three MH-connecting tests that lacked R-15's serial key now carry it, each with a
note that the key is load-bearing for the probe and not decoration. Steps 1–2 rest on **disjoint**
controls over one hazard and the residual sliver (a connector outside this binary) is named at the
site, so neither gets pruned as redundant.

### Shared fixture (`env-tests/src/fixtures/metrics.rs`)

One comparison rule: `instances_exceeding_baseline` (sorted), carrying both `InstanceCounters` trap
paragraphs and the residual-false-pass note. `any_instance_exceeds_baseline` became a true
`!…is_empty()` wrapper over it. **No separate named-instance predicate** — the membership read
serves both call sites, so there are two functions, not three. Pinned by an agreement test with a
`true` arm (rollover) and a `false` arm, so it cannot pass against a wrapper hardcoded to `true`.

`poll_until_any_instance_above`'s **body is untouched**, so its three live gates keep their
semantics by construction. Two new siblings: `poll_until_instance_above` and `poll_until_stable`,
both panicking inside the helper rather than returning a value a call site could drop.

**A deferred extraction closed because its own trigger fired.** `26_mh_quic.rs` already had two
hand-rolled stabilize loops with the extraction deliberately deferred; the recorded trigger was
"the next touch of either helper, **a third stability-wait**, or a change to the scrape SLA". This
task added the third, so both are now re-expressed on `poll_until_stable` and all three callers
share one loop. Settle and budget stay per-caller parameters — the two 16s and the two 90s are
independent derivations that happen to agree: *same numbers, different reasons, opposite drift
obligations*.

### Prose corrections this path created

R-15's `#[ignore]` is **retained**; its reason text, doc comment and the module header now say task
25 landed and only task 26's receive-path gap remains. `docs/TODO.md`'s R-15 task-25 clause records
what landed while the entry stays OPEN. `docs/TODO.md`'s `mc_media_send_directives_total` alert
entry had a **fired trigger still reading as pending** (task 19 landed; the SDK does honour the
directive) — corrected, with the rule itself still task 21's. `docs/runbooks/mc-incident-response.md`
told on-call the media path was invisible and not pageable, which task 19 inverted — replaced with
@operations' wording, that paragraph only.

### Telemetry

Zero new metrics, zero new labels, no log or metric macro added to `media_routing/`,
`media_signaling/` or `media_admission/` (all three remain macro-free). No meeting id, participant
id, `sender_id` or url-valued label anywhere. No scrape-config change. `key_custody=operator`
untouched. No delta-*magnitude* assertion on `mh_webtransport_connections_total`, which can move by
2 for one connection.

---

## Files Modified

| Path | What changed |
|------|--------------|
| `crates/mc-service/src/webtransport/connection.rs` | `media_servers` builder doc (bootstrap-only, non-authoritative, refusal-to-sort + why, two-way pointer to the component test, honest cost); three of the four `connectAll()` premise corrections |
| `crates/mc-service/src/media_routing/assignment.rs` | `RoutingParticipant::handlers` premise correction + the directed-handler contract |
| `crates/mc-service/src/media_signaling/directive.rs` | N=2 correspondence test with premise check and named wrong answer; unit-tier order pin with its "not the inversion proof" note |
| `crates/mc-service/src/media_signaling/assignments.rs` | Receive-side equivalents; `loopback()` generalised to `assignment_over(handlers)` |
| `crates/mc-service/tests/media_client_signaling_integration.rs` | `join_with_handlers` / `join_seeded`; the two load-bearing component-tier steering tests |
| `crates/env-tests/src/fixtures/metrics.rs` | `instances_exceeding_baseline` as the one rule; `any_instance_exceeds_baseline` as a true wrapper; `poll_until_instance_above`; `poll_until_stable`; four new unit tests |
| `crates/env-tests/tests/26_mh_quic.rs` | `McSession` / `mc_join_session` / `follow_mc_steering`; three-step instance-keyed gate; serial keys on three tests; notes at the two legitimate `.first()` sites; both stabilize loops re-expressed; R-15 prose (`#[ignore]` retained) |
| `docs/TODO.md` | R-15 task-25 clause; `mc_media_send_directives_total` fired-trigger correction |
| `docs/runbooks/mc-incident-response.md` | One paragraph, @operations' wording (`Approved-Cross-Boundary: operations`) |
| `proto/dark_tower/signaling/v1/signaling.proto` | **Not mine** — `paired-protocol` owns and wrote that hunk; wording agreed for no substance drift |

---

## Devloop Verification Steps

Gate 2 is the **Lead's** run of `./scripts/layer-all.sh`, on a frozen tree, with a
`git status --short` snapshot taken before and after so a run mutated underneath itself
announces itself instead of impersonating a test failure. Both Lead attempts were
`TREE_QUIESCENT=yes`.

### Attempt 1 — `TOTAL_RESULT=FAIL` (Layer 4), NOT diff-caused

```
LAYER=1 OK 12   LAYER=2 OK 2    LAYER=3 OK 53   LAYER=4 FAIL 45
LAYER=5 OK 13   LAYER=6 N/A 1   LAYER=7 OK 537
TOTAL_DURATION=663 TOTAL_RESULT=FAIL
```

Layer 4 reported `STATUS=FAIL REASON=cargo-test-failed`. The cause was **not a test
assertion** — it was the linker aborting:

```
collect2: fatal error: ld terminated with signal 6 [Aborted], core dumped
error: linking with `cc` failed: exit status: 1
```

13 `mc-service` integration test binaries failed to LINK, `signal 6` x13. The set includes
binaries this diff does not touch (`gc_integration`, `heartbeat_tasks`,
`token_refresh_integration`, `redis_metrics_integration`), which is what rules out the
changeset: a diff-caused failure does not take down link steps for unrelated binaries.
Machine had 16 GB total with ~1 GB free at the time; parallel `rust-lld` invocations
exhausted it.

**Lane call**: SKILL.md §Gate 2's discriminator is reproduce-on-retry, not the wrapper's
exit code. Re-running `./scripts/layer4.sh` alone on the same frozen tree gave
`RESULT=N/A REASON=not-applicable-to-this-lang` with `cargo-test-passed`,
`nx-test-passed` and **zero** `signal 6` occurrences. Non-reproducing → operator lane →
**does not consume a Gate 2 attempt**, and was not routed to the implementer.

Recorded rather than absorbed, per `CLAUDE.md` §Fail loudly: this is a real
resource-exhaustion mode of the local pipeline, and the next person to hit it should find
this entry rather than re-derive it from a linker backtrace.

### Attempt 2 — `TOTAL_RESULT=N/A`, exit 0, ALL layers evaluated (AUTHORITATIVE)

```
LAYER=1 OK 7    LAYER=2 OK 2    LAYER=3 OK 50   LAYER=4 N/A 235
LAYER=5 OK 2    LAYER=6 N/A 1   LAYER=7 OK 325
TOTAL_DURATION=622 TOTAL_RESULT=N/A
```

`grep -c "STATUS=(FAIL|PRECONDITION_FAILURE|UNKNOWN)"` → **0**. No layer rendered
`NOT-RUN`; this session is headless, so the run-all contract applies and every layer was
evaluated.

| Layer | Result | Evidence |
|---|---|---|
| 1 Compile | OK | `buf-build`, `cargo-build`, `cargo-build-dt-guard`, `cargo-build-dt-story`, `release-feature-gate`, `nx-typecheck` all `STATUS=OK` |
| 2 Format | OK | `buf-format`, `cargo-fmt`, `nx-format` |
| 3 Guards | OK | `guards-passed` + every control self-test (`layer7-selftest`, `run-guards-selftest`, `media-telemetry-deny-selftest`, `frame-vectors-guard-selftest`, …) |
| 4 Test | N/A (aggregate) | `cargo-test-passed`, `nx-test-passed`; the `N/A` is proto's registered intentional-gap placeholder (`not-applicable-to-this-lang`), self-justifying per ADR-0033 §6 |
| 5 Lint | OK | `buf-lint`, `cargo-clippy`, `nx-lint` |
| 6 Audit | N/A (aggregate) | `cargo-audit-passed`; `pnpm audit` `SKIPPED-NO-DIFF REASON=no-dep-changes` (the wrapper's own dep-manifest gate); `buf-breaking-passed` |
| 7 Env-tests | OK | `env-tests-passed` **and** `browser-e2e-passed` — both Phase-2 suites ran |

`buf-breaking-passed` is the one that matters for the GSA row: the `signaling.proto` edit
is comment-only and provably not a wire break.

Attempts consumed: **1 of 3** (attempt 1 was operator-lane and did not count).

### Attempt 3 — post-review-fix re-run on the FINAL frozen tree (AUTHORITATIVE)

The Gate 3 fix round moved the tree (the `tokio::join!` compile break, D4/D5 across three sites,
`observability` Finding 4, OPS-11/12/13, the CBC reconciliation, `operations`' two
`devloop-validation.md` hunks, the `-D warnings` verification correction and the two dead functions
it exposed), so attempt 2's verdict no longer described the tree being committed. Re-run:

```
PIPELINE_EXIT=0   TREE_QUIESCENT=yes   link aborts: 0
STATUS=(FAIL|PRECONDITION_FAILURE|UNKNOWN) occurrences: 0
LAYER=1 OK 5    LAYER=2 OK 2    LAYER=3 OK 48   LAYER=4 N/A 233
LAYER=5 OK 4    LAYER=6 N/A 1   LAYER=7 OK 534
TOTAL_DURATION=827 TOTAL_RESULT=N/A
```

This is the run the commit rests on. Attempts consumed remains **1 of 3** — attempt 3 was occasioned
by review fixes, which is the normal Gate 2 → review → Gate 2 cycle, not a failed attempt.

### An additional red, reported and not reproduced (`operations`)

While validating their own doc edits, `operations` saw `./scripts/layer3.sh` come back
`STATUS=FAIL REASON=run-story-selftest-failed` (rc 1) on its first run; three subsequent runs were
green, including `run-story.test.sh` standalone at 290/290. Not attributable to their edits (two
markdown files; the self-test exercises `scripts/workflow/run-story.sh`). **The failing assertion was
not captured before the re-run destroyed it**, so the cause is unknown and is recorded as unknown.

Deliberately NOT filed as a `docs/TODO.md` entry: one unreproduced observation with no captured
assertion invites someone to burn a devloop chasing it. Recorded here instead, with the actionable
part — if `run-story-selftest-failed` recurs, capture the full Layer 3 output **before** re-running,
and file it against the same misclassification family as the two entries below. Note the closed
`Guard Timeout vs Violation` entry does **not** cover this shape: that reclassified guard
timeout/kill to `PRECONDITION_FAILURE` on the operator lane, whereas this reported `FAIL` on the
implementer lane.

That makes **three** pipeline reds in this one devloop for reasons that were not the diff — the
Layer 4 linker abort, the tree-contamination the implementer hit twice, and this. That pattern is
what both deferred `docs/TODO.md` entries are about.

### Attempt 4 — FINAL, on the committed tree

`security`'s S-2 (the STEP 1 comment orphaned from its mechanism by the compile fix) and the three
§Lessons Learned entries landed after attempt 3, so attempt 3 no longer described the tree being
committed either. Re-run:

```
PIPELINE_EXIT=0   TREE_QUIESCENT=yes   link aborts: 0
STATUS=(FAIL|PRECONDITION_FAILURE|UNKNOWN) occurrences: 0
LAYER=1 OK 7    LAYER=2 OK 2    LAYER=3 OK 47   LAYER=4 N/A 241
LAYER=5 OK 1    LAYER=6 N/A 2   LAYER=7 OK 511
TOTAL_DURATION=811 TOTAL_RESULT=N/A
```

**This is the run the commit rests on.** Attempts consumed: **1 of 3** — attempts 2, 3 and 4 were
each occasioned by review fixes moving the tree, which is the Gate 2 → review → Gate 2 cycle working
rather than failed attempts. The Lead re-ran rather than carrying an earlier verdict forward,
because a pipeline verdict is an assertion about a specific tree state and three separate reviewers
had verdicts invalidated in this devloop by exactly that gap.

**Honest scope of attempt 4**: the Lead edited THIS file (§Code Review Results, §Accepted Deferrals)
after attempt 4 finished. Those edits are confined to `docs/devloop-outputs/**`, which only Layer 3's
doc guards read — no compiled source moved. Rather than assert that attempt 4 covered them, Layer 3
was re-run against the edited file: `RESULT=OK REASON=guards-passed`, after
`validate-todo-tracking` correctly rejected a first draft that inlined prose into §Accepted Deferrals
(`inline_debt_body`). Recording the narrower claim, since "the pipeline was green earlier" is the
exact reasoning this devloop spent three attempts unlearning.

---

## Code Review Results

**Gate 3: all eight verdicts in, none ESCALATED.** Every reviewer verified against the FINAL tree —
three of them (`security`, `test`, `code-reviewer`) had an earlier verdict invalidated when
`observability` found the tree did not compile under `--features all`, and each re-ran their checks
rather than re-affirming from a description.

| Reviewer | Verdict | Findings | Fixed | Deferred |
|----------|---------|----------|-------|----------|
| Security | RESOLVED-FIXED | 2 (S-1, S-2) + 3 backed | 5 | 0 |
| Test | CLEAR | 0 | — | 0 |
| Observability | RESOLVED-FIXED | 5 + 1 compile blocker | 6 | 0 |
| Code Quality | RESOLVED-FIXED | 1 | 1 | 0 |
| DRY | RESOLVED-FIXED | 5 (D1-D5) | 5 | 0 |
| Operations | **RESOLVED-DEFERRED** | 13 (OPS-1..13) | 13 | 0 in-diff; **2 `docs/TODO.md` pipeline-gap entries** |
| Semantic Guard | CLEAR (native: SAFE) | 0 | — | 0 |
| Paired Protocol | CLEAR | 0 | — | 0 |

**`operations` is RESOLVED-DEFERRED and that is surfaced deliberately, not averaged away.** All
thirteen of their findings were fixed in the diff; the verdict is DEFERRED because their §Accepted
Deferrals is non-empty — two `docs/TODO.md` entries for pipeline gaps this devloop *surfaced* rather
than created. `operations` declined to round that down, which is the split-verdict rule doing exactly
what it exists for. The Validation Coverage Gap entry is likewise unattributed to any reviewer:
pinning a pre-existing gap on someone to tidy the bookkeeping makes it read as this task's debt and
get deprioritised accordingly.

### Security — RESOLVED-FIXED
S-1: `poll_until_stable` documented as closing a hazard it half-closes (two equal reads 16s apart
close a transient empty result, not a target unscraped beyond `query.lookback-delta`, whose 5m
default is ~20x the settle). Fixed across three sites with one derivation and two bounded referrals.
S-2: the compile fix moved the stabilise into the `tokio::join!` and orphaned STEP 1's comment from
its mechanism — a correct claim detached from what it explains, which is why three reviewers read
past it. Fixed. Re-verified the two non-negotiables on the landed tree: the mutation probe is
reverted (`shared.sort(); shared.first()`, `assignment.rs:296-297`) and `mh_statuses` containment
holds.

### Test — CLEAR
Zero test-domain findings; per-hunk ACK re-affirmed on both test-owned env-test files. Verified the
non-vacuity of both required tests including the two vectors mutation cannot reach — expected values
are inline literals, not read back from the seed, and `mh_handler` yields distinct urls per handler
so MC cannot pass by echoing "the url". Independently re-verified OPS-6: every `connect_wt(&mh_url)`
site sits in a test carrying `#[serial(mh_notifications)]`.

### Observability — RESOLVED-FIXED
Found the compile blocker and the finding underneath it. Five findings: the `applied_baseline`
stabilise asymmetry (the gate's own deciding counter was the one left unprotected), the drift
obligation excluding the site with most to lose, two mangled panic literals, the third stale-claim
site, and two orphaned functions that failed `-D warnings` under the feature flag. Confirmed zero
new metrics, zero new labels, no scrape-config change, `key_custody` absent from the diff entirely,
and the three MC media modules still telemetry-macro-free.

### Code Quality — RESOLVED-FIXED
One finding: the CBC table under-scoped `docs/TODO.md` as "R-15 clause only, Mine" while the diff
also edits the operations-owned `mc_media_send_directives_total` hunk — a table contradicting the
document it sits in, toward *less* oversight. Split and fixed. Verified the SSoT claim holds: no
production derivation changed and no third "which handler" encoding was introduced.

### DRY — RESOLVED-FIXED
D1 moved the exactly-one-rose trichotomy out of the `#[ignore]`d body into a unit-tested
`RisenOutcome`, whose `Many` arm asserts it never degrades to `One` — the degradation that would
rebuild this task's defect one layer up. D2-D5: the 90s derivation, `#[must_use]`, a stale count,
and the pre-S-1 overclaim.

### Operations — RESOLVED-DEFERRED
Thirteen findings, all fixed. OPS-1 (sort on `mh_id`, never the URL — a URL sort agrees with
placement only under Kind's topology and re-inverts in production with the env-test still green) was
the sharpest catch of Gate 1. OPS-6 (serial keys) was the sharpest of Gate 2. Authored two hunks in
their own files: the `mc-incident-response.md` paragraph and the `devloop-validation.md` §6.4/§8 rows.

### Semantic Guard — CLEAR (native SAFE)
Five checks clean on the final tree, re-read after the diagnostic surface changed twice. Confirmed
`follow_mc_steering` still has no `media_servers` fallback after the `join!` restructuring, and took
an extra pass hunting a fourth instance of the implementer's over-extension reflex — found none.

### Paired Protocol — CLEAR
Comment-only `signaling.proto` hunk, owner-written per ADR-0024 §6.3. Re-confirmed no substance
drift against the Rust-side comment after the fix round moved it.

---

## Accepted Deferrals

- `docs/TODO.md` §Polyglot Pipeline Follow-ups (ADR-0033 Wave 1 #1) — linker abort mislabelled `cargo-test-failed`
- `docs/TODO.md` §Polyglot Pipeline Follow-ups (ADR-0033 Wave 1 #1) — verdict not bound to the tree it ran over
- `docs/TODO.md` §Validation Coverage Gap — no env-test is type-checked below Layer 7
- `docs/TODO.md` §Cross-Service Duplication (DRY) — framed `ServerMessage` decode has four homes
- `docs/TODO.md` §Cross-Service Duplication (DRY) — three per-instance poll loops, no discharge trigger

---

## Rollback Procedure

1. Start commit: `a2adc44062ec00a9b210ce1a66ed841b432b0136`
2. `git diff a2adc440..HEAD`
3. `git reset --soft a2adc440` (or `--hard` for a clean revert)

---

## Issues Encountered & Resolutions

### A red Layer 4 that meant nothing: `layer-all.sh` requires a QUIESCENT tree

**Symptom.** A backgrounded `./scripts/layer-all.sh` reported `LAYER=4 RESULT=FAIL DURATION=17`
and `TOTAL_RESULT=FAIL`, while `./scripts/layer4.sh` run standalone against the same tree moments
later went green end to end in **236s** (`STATUS=OK REASON=cargo-test-passed`,
`STATUS=OK REASON=nx-test-passed`), as did @team-lead's independent run.

**Cause — run hygiene, not the diff.** The pipeline was launched in the background and the tree was
then edited while it ran (a module doc comment in `crates/env-tests/tests/26_mh_quic.rs`, and later
an inlining of test literals). A source edit landing under a running `cargo test` produces exactly
this shape: an early non-zero exit at ~17s against a ~236s standalone run. The 17-versus-236 gap is
the tell; the *sequence* is the evidence.

**Resolution.** Diagnosed by @team-lead from the edit sequence rather than from the duration —
which matters, because a duration is not a diagnosis and the first instinct is to hunt a
non-existent test failure. The contaminated run (and a second one, contaminated the same way by the
literal-inlining fix) was killed and re-run on a quiescent tree.

**Worth knowing next time.** A backgrounded `layer-all.sh` concurrent with further edits produces a
red that carries no information, and it costs an hour to chase. Let it finish, or do not start it
until editing has stopped. Two self-justifying statuses that should NOT be defended as skips:
`LAYER=6 RESULT=N/A` is the audit dep-manifest gate reporting that no dependency manifest changed,
and a per-language `LAYER=4 ... RESULT=N/A REASON=not-applicable-to-this-lang` is the intentional-gap
placeholder.

**Evidence the original red was contamination, not the diff.** A subsequent run on a quiescent tree
returned `LAYER=4 RESULT=N/A DURATION=247` — the healthy full-run value, against the contaminated
run's `FAIL DURATION=17`. Layers 1-3 and 5-6 all OK/N-A. (Layer 7 then reported
`PRECONDITION_FAILURE REASON=cluster-rebuild-failed` because that run was **deliberately killed**
mid-Layer-7 to apply the queued comment fix — a chosen stop, not a contamination.)

**The `git status` before/after snapshot is NOT a property of the pipeline.** It was an ad-hoc
wrapper around one invocation, living only in the shell command. `layer-all.sh` itself does not
self-certify a quiescent tree, and this document should not imply it does. Making a contaminated run
announce itself rather than impersonate a test failure would be a real "fail loudly, never mask"
improvement to the pipeline — recorded here as an observation for `test`/`infrastructure`, not as
something this task landed.

### A green from a command you have not proven can go red is not evidence

Two people hit this in one devloop, at opposite ends of the review, and the pairing is the point —
it is not a story about a feature flag.

- **Implementer side.** `cargo build -p env-tests --tests` was written into the plan *as* the
  verification step. `#![cfg(feature = "flows")]` plus no default features meant it compiled none of
  the file: exit 0 in 0.13s against a tree that did not parse. It hid a `tokio::join!` break through
  several rounds of "verified".
- **Reviewer side.** @security re-reviewed the ordering property by reading the diff and reasoning
  about it, and re-affirmed a verdict over a tree that did not compile. Their own words: they
  "reasoned about the ordering from the diff and never made the compiler agree."

Same failure, opposite ends. **The remedy in both directions is a positive control**: before
believing a green, make the check go red on purpose. @security did exactly that on the re-review —
appended a deliberate type error, confirmed `cargo check -p env-tests --tests --features all` went
red (exit 101, E0308 at the injected line), restored the file, re-ran green on a clean tree. That is
the control neither of us had the first time.

Cheap, and it distinguishes the two things that look identical from the outside: a check that
passed, and a check that ran where it could observe nothing (review-protocol §Assertion Vacuity
mechanism 5).

### Read an amended block as a UNIT, not as a sequence of edits

A review method rather than a fact about this diff, and it found a defect **twice** here:

- **S-1's third site.** The "every target has been scraped" overclaim had three homes. Writing the
  correction as one pass was necessary and not sufficient — a single edit still has to reach every
  site making the claim, and the referral sentence on `instances_exceeding_baseline` was the one
  missed. Found because @main had @security re-read the block as a unit.
- **S-2, and the STEP 2 gap inside it.** @security found the `STEP 1` label orphaned from the code
  that moved. The *second* defect — that the vacated position had no heading at all, so the
  three-step spine had a hole in the middle rather than a stray marker at the top — surfaced only
  because fixing S-2 forced a decision about what the vacated position should say.

The common shape: each individual edit was correct, and the defect lived in what the edits added up
to. Nothing about reading them one at a time could have caught either. Worth doing deliberately at
the end of a round with several edits to one block — which is exactly when it is most tempting to
skip, because every piece has just been checked.

### Disclosure belongs where the FUTURE ACTOR looks, not only where the current reader is

@operations named this three times in one devloop — the runbook paragraph, the OPS-10 fired trigger,
and OPS-13's slice (d) — before I saw it as one pattern:

- The runbook told on-call the media path was invisible and unpageable; task 19 had inverted that.
- The `mc_media_send_directives_total` entry said "no alert, and that is correct TODAY" after its own
  trigger had fired.
- Slice (d)'s residual was documented thoroughly *in the code*, and absent from `docs/TODO.md`'s R-15
  entry — which is the thing task 26's implementer is told to read before deleting the `#[ignore]`.

In each case the disclosure existed and was accurate, sitting where someone already in that file
would find it. The person it protects arrives from somewhere else. For slice (d) the concrete cost
would have been task 26 triaging a wrong-instance failure as their own receive-path bug — this
task's residual handed forward with the wrong label.

**Ask: who acts on this next, and what will they be reading when they do?**

### The recurring reflex behind three of this task's errors

Three self-caught errors in this diff share one mechanism, and naming it is worth more than the
individual fixes:

> **Take a rule someone applied to one relationship and extend it to the neighbours without
> re-deriving which neighbours it describes.**

It fired at a *constant* (hoisting two tests' expected urls into shared symbols, coupling two tests
that exist to be independent) and twice at a *comment* (asserting `egress_stream_id`'s "same number,
different reasons, opposite drift obligations" register over two stabilize helpers that are one
decision at two sites, and then again at the shared loop's own rustdoc where it read as
authoritative for all three callers).

**Why it is hard to self-catch** (@dry-reviewer's observation, and the part that generalises): in
each case the rule had just been *given* by a reviewer, so extending it **felt like compliance
rather than invention**. The second application looks like consistency. The cheap check that would
have caught all three is one question — *which specific relationship did this rule describe?* — and
it costs nothing to ask before generalising.

Recorded because the tell is portable: a comment that quotes the rule it is breaking
("do not hoist these to a shared constant", written above two hoisted constants) reads as
compliance and therefore survives review.

### Caught hoisting the expected urls to constants, against an explicit reviewer ask

@dry-reviewer and @test both asked that C1/C2's expected handler urls stay **written literals** and
not be hoisted. I wrote them as file-scoped `const`s anyway, with a doc comment saying "do not hoist
these to a shared constant" — quoting the rule while breaking it. My reasoning was that a
*test-local* constant cannot track the production symbol, so the cited hazard did not apply. That is
wrong for a reason the original ask did not have to spell out: **two tests sharing one symbol move
together** when someone edits it to make one of them pass, so the second stops being an independent
statement — and C1/C2 are deliberately two angles on one property. Self-reported and fixed: inline
literals at every assertion site, with the repetition's justification recorded at the top of the
section.

