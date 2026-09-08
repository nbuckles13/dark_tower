# Devloop Output: participant → sender_id binding contract (NotifyParticipantConnectedResponse)

**Date**: 2026-09-08
**Task**: Land the participant-to-`sender_id` binding contract so MH can start media sessions — proto field, MC send side, MH receive side, declined-media-session metric, and R-15 end-to-end proof.
**Specialist**: meeting-controller (paired: protocol, media-handler)
**Mode**: Agent Teams (v2) — full, headless (`DEVLOOP_HEADLESS=1`), story task #24
**Branch**: `feature/hear-yourself-through-handler`
**Duration**: five sessions across 2026-09-05..08 (four interrupted or escalated; the fifth carried Gate 2, Gate 3 and the commit)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `f0e5ee678f778455cc670662eff47038a30863fd` |
| Branch | `feature/hear-yourself-through-handler` |
| Lead Model | `claude-opus-5` |

---

## Loop State (Internal)

<!-- This section is maintained by the Lead for state recovery after interruption. -->
<!-- Do not edit manually - the Lead updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Phase | `complete` (fifth session. Gate 2 PASS all seven layers, Gate 3 PASS with ten verdicts, committed.) |
| Implementer | `implementer` (meeting-controller) — spawned |
| Implementing Specialist | `meeting-controller` |
| Iteration | `3` |
| Paired Protocol | `paired-protocol` — spawned, plan confirmed pending |
| Paired Media Handler | `paired-media-handler` — spawned, active |
| Security | `security` — spawned, NOT confirmed (S1/S2 blocking) |
| Auth Controller | `auth-controller` — spawned, **plan confirmed** |
| Test | `test` — spawned, **plan confirmed** (2 conditions) |
| Observability | `observability` — spawned; label ruling delivered |
| Code Quality | `code-reviewer` — spawned, **plan confirmed** |
| DRY | `dry-reviewer` — spawned, pending |
| Operations | `operations` — spawned, pending |
| Semantic Guard | `semantic-guard` — spawned, pending |

**Gate-1 plan-confirmation tracker** (Lead-maintained). **Gate 1 never formally closed** — the
session that ran it was interrupted before every row reached `confirmed`. Rather than lose the
unconfirmed rows with the sessions that raised them, each was carried into its reviewer's Gate-3
prompt and answered against the real diff. The right-hand column records where each row actually
came to rest, which is a stronger disposition than a Gate-1 tick would have been.

| Reviewer | Plan Status at Gate 1 | Where it came to rest |
|----------|-----------------------|-----------------------|
| Security | not confirmed — S1 + S2 blocking | Both closed at Gate 3, verified in code; S2's cross-tenant arm traced by hand against a dropped-meeting-key mutation |
| Auth Controller | confirmed | CLEAR at Gate 3; S1's mitigations verified as trailer conditions |
| Test | confirmed, conditional on the MH side being staffed and the label ruling landing | Both conditions met; DoD 6-11 verified at Gate 3 |
| Observability | not confirmed — MC-side counter ruling owed (§8e item 1) | Ruled at Gate 3, and extended §8a from four outcomes to seven, ratifying two the implementer added |
| Code Quality | confirmed | RESOLVED-FIXED at Gate 3 |
| DRY | not confirmed — owed the ruling on its own `validate_16bit_id()` non-collapse boundary | Confirmed at Gate 3 against the tree, with a fourth independent ground added |
| Operations | not confirmed | RESOLVED-FIXED at Gate 3; found the unfireable alerts and the self-certifying rollback gate |
| Semantic Guard | not confirmed | CLEAR at Gate 3, all five checks |
| Protocol (paired, §6.4 intersection) | not confirmed | RESOLVED-FIXED at Gate 3, with the by-hand breaking read performed and a cross-boundary trailer supplied |
| Media Handler (paired) | not confirmed | RESOLVED-DEFERRED at Gate 3; owned the MH fix lane and confirmed the uncoordinated env-test edit as owner |

**Lead rulings at Gate 1** (recorded here because they resolve tensions in the task text itself):

1. **DEVIATION FROM THE LITERAL TASK TEXT — `validate_16bit_id()`.** The task instructs "validate through common's `validate_16bit_id()` so the bound stays single-sourced". That helper **does not exist**; all six in-tree occurrences of `validate_16bit_id`/`MAX_16BIT_ID` are *prohibitions* (`internal.proto:50`, `signaling.proto:611`, `mc-service/src/media_signaling/capability.rs:39`, `mh-service/src/routing/mod.rs:162`, and two `docs/specialist-knowledge/dry-reviewer/INDEX.md` boundary entries spanning two story tasks). The premise traces to `docs/TODO.md:909`, where @protocol's own Gate-3 ruling misread its own prohibition as a reference. **Ruled: do not create the helper.** `crates/common` has no `media-protocol` dependency and must not gain one, so the helper would hardcode `65535` — a *second, unanchored* home for a bound whose only home today is `media_protocol::frame::KEY_ID_SENDER_ID_BITS`, `const _`-asserted on both consuming sides. The instruction names a mechanism; its stated goal is single-sourcing, and the goal selects non-collapse. Validation routes through the existing `mh_service::routing::SenderId::from_wire()`, which already yields the specified three-way semantics. Ruled by @paired-protocol (author of the original ruling), confirmed independently by @code-reviewer; @dry-reviewer's confirmation owed as owner of the boundary.
2. **Proto diff widened, deliberately, by two comment-only additions** in a file protocol owns outright: a MUST at `NotifyParticipantConnectedRequest.participant_id` that it derive from the validated meeting token's `sub` (@security's S1 — the code does this at `mh-service/src/webtransport/connection.rs:271`, but nothing in the contract requires it), and a note at the `rpc NotifyParticipantConnected` line that the call is now a blocking precondition rather than a fire-and-forget ack.
3. **Accepted behaviour change — and a CORRECTION to how it was first framed.** Making the response load-bearing converts an advisory MH→MC notification into a blocking gate on MH's media accept path: a slow or unavailable MC now closes the connection where previously it stayed up. Every participant in this devloop, @security included, initially described that as *"security bought with availability."* **@security withdrew that framing and I am adopting the withdrawal.** The binding IS the response — so there is no state in which the old behaviour delivered media that the new behaviour refuses. Today MH declines to start the media tasks on every connection, unconditionally, because `bind()` has no caller. What actually changes is that the failure becomes *visible* (connection closed, counted outcome) instead of *invisible* (connection up, `mh_media_*` flat at zero, byte-identical to a healthy idle handler). It is a visibility improvement at near-zero availability cost. **The framing matters beyond bookkeeping**: "we traded availability for security" is precisely the sentence a future reader would cite to argue the gate back off, and it would be citing a trade that was never made.
4. **`proto/buf.yaml`'s `breaking.ignore` is NOT narrowed.** `internal.proto` sits inside it (`proto/buf.yaml:52-54`), so **a green Layer 6 is not evidence this edit is wire-safe**. `breaking.sh` prints `SUPPRESSED=<paths>` so it is not silently green, and the compensating control is @paired-protocol reading the diff by hand at Gate 3, recorded explicitly in their verdict.
5. **A green Layer 7 with the `#[ignore]` still present is NOT done — and the reason is narrower than this devloop first believed.** @operations initially framed the hazard as "a green Layer 7 in CI hides a skipped env-test suite" and then **withdrew that framing on reading the site**, on three counts: `scripts/layer7.sh:437` emits a distinct `SKIPPED-NO-CLUSTER` status token rather than `OK`; the skip lane requires socket-absent *and* `GITHUB_ACTIONS`, so a containerized devloop structurally cannot reach it (it gets `precondition_fail local-helper-not-running` instead); and the comment at `:412-415` records that hole as already closed by a prior operations+security task. The withdrawal propagated into @test's Gate-3 bar before it was caught, and was re-routed through two channels rather than relying on one message landing.

   **The corrected finding is sharper and sits on the diff under review rather than in a CI lane**: `scripts/layer7.sh:828-836` keys on **exit code only**, and a still-`#[ignore]`d test also yields rc 0 — so `env-tests-passed` cannot distinguish "ran and passed" from "was ignored and trivially did not fail." **Lead reading rule, adopted:** `layer7.sh:830` already tees full libtest output to `$ENV_TEST_LOG`. Primary check is the named `test ... ok` line, which is fail-closed (it reads `... ignored` if the attribute survives); secondary is `0 ignored` in the summary, which catches sibling ignores. This stays a Gate-3 eyeball check rather than a committed grep, because a test-name needle turns fail-open on rename. JUnit output was rejected (no nextest; new machinery on the critical path) and a bare pass-count rejected as fail-open.

   Separately, the single-participant loopback test cannot prove R-15 even when it genuinely runs — @security's S2 and @test's DoD item 6 converge here, and @implementer's own mutation run confirmed it empirically. So the multi-participant and two-meeting arms, **shown red against a deliberately wrong binding**, are part of the definition of done.

---

## Task Overview

### Objective

R-15 (*"MH forwards a client's uplink audio datagrams back to that client purely from MC-pushed policy"*) is an OPEN acceptance criterion: MH's `SenderBindings::bind()` has no production caller because no contract carries participant → `sender_id`. This devloop lands that contract end-to-end.

Three sides, one task:

1. **Proto** — `uint32 sender_id = 2;` on `NotifyParticipantConnectedResponse` (tag 1 is `acknowledged`). **USER OVERRIDE** of @protocol's Gate-3 `optional` ruling: bare `uint32`, NOT `optional`. No deployed old MC exists, so the absent-vs-zero distinction has no referent. Semantics: `0` => reject loud (protocol error, counted); `1..=65535` => bind; `>65535` => reject. Validated through `common`'s `validate_16bit_id()` so the bound stays single-sourced. Override rationale recorded at the field.
2. **MC send side** — `NotifyParticipantConnected`'s handler answers with the participant's allocated `sender_id` from the meeting actor (task 10's allocator). A participant MC cannot resolve gets `0` => MH rejects. Never a made-up id.
3. **MH receive side** — the response becomes load-bearing: ordering `JWT gate → NotifyParticipantConnected → bind(sender_id) → start_media_session`. Reject-and-close on `0`/out-of-range with a counted reason. Disconnect needs no new contract (MH drives `unbind` on teardown; `NotifyParticipantDisconnected` exists).

Also in scope:

- **Observability obligation** — `mh_media_session_starts_total` (renamed by @observability; **six** outcome values), with catalog entry and metric-coverage test. NOT folded into `mh_media_frames_dropped_total` (that would corrupt the forwarded+dropped=attempts denominator). @observability owns name/labels via the catalog.
- **R-15 proof** — delete the `#[ignore]` on `crates/env-tests/tests/26_mh_quic.rs::test_mh_forwards_an_audio_datagram_back_to_its_sender`; it must pass against the live cluster.
- **TODO closure** — the R-15 entry's four-part closure condition is met (field lands, `bind()` has a production caller, `#[ignore]` removed, test passes) => delete the entry.

`sender_id` never appears in logs/metrics as a per-participant dimension (ADR-0036 §11 rules unchanged).

### Scope
- **Service(s)**: mc-service (send side), mh-service (receive side + metric), proto contract, common (validation reuse), env-tests
- **Schema**: No
- **Cross-cutting**: Yes — wire format × identity field GSA intersection

### Debate Decision

NOT NEEDED — the design is already ruled. @protocol's Gate-3 ruling (2026-09-05, `docs/devloop-outputs/2026-09-05-mh-audio-datagram-forward-path/`) settled the message, field, shape and GSA routing; the user override narrows only the presence question.

---

## Cross-Boundary Classification

`proto/**` is an ADR-0024 §6.4 Guarded Shared Area (wire format) and `sender_id` is an identity
field, so this diff hits the §6.4 **intersection rule**: protocol + auth-controller + security all
present and confirming, plus the producing and consuming specialists implementing their own sides.
**Mechanical is DISALLOWED on every `proto/**` row** and Owner is filled for every cross-boundary
row.

Three implementers, disjoint trees, one plan:

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `proto/dark_tower/internal/v1/internal.proto` | **Not mine, Domain-judgment** (GSA §6.4 × identity field) | protocol |
| `crates/proto-gen/tests/internal_roundtrip.rs` | **Not mine, Domain-judgment** (GSA `proto-gen/**`) | protocol |
| `crates/mh-service/tests/common/mock_mc.rs` | **Not mine, Domain-judgment** (regen forces it; struct literal must gain the field) | media-handler |
| `crates/mc-service/src/grpc/media_coordination.rs` | Mine | — |
| `crates/mc-service/src/actors/messages.rs` | Mine | — |
| `crates/mc-service/src/media_admission/binding_response.rs` (new) | Mine | — |
| `crates/mc-service/src/media_admission/mod.rs` | Mine | — |
| `crates/mc-service/tests/otel_grpc_inbound_continuity.rs` | Mine | — |
| `crates/mc-service/src/actors/meeting.rs` | Mine | — |
| `crates/mc-service/src/main.rs` (wiring only) | Mine | — |
| `crates/mc-service/src/observability/metrics.rs` | Mine (code) / **Minor-judgment** on the metric's name + label space | observability |
| `crates/mc-service/tests/media_coordination_integration.rs` | Mine | — |
| `docs/observability/metrics/mc-service.md` | **Not mine, Minor-judgment** (catalog is observability's SSoT) | observability |
| `infra/grafana/dashboards/mc-overview.json` (panel 63) | **Mine, Domain-judgment** (per-service dashboard, ADR-0031 — service specialist owns and implements; observability is the mandatory cross-cutting reviewer of the *semantics*) | — |
| `infra/grafana/dashboards/mh-overview.json` (panel 38) | **Not mine, Domain-judgment** (service-owned dashboard, ADR-0031) | media-handler |
| `crates/mh-service/**` (accept path, `bind()`, reject-and-close, metric emission, MH tests) | **Not mine, Domain-judgment** | media-handler |
| `docs/observability/metrics/mh-service.md` | **Not mine, Minor-judgment** | observability |
| `docs/runbooks/mh-deployment.md` (OPS-1: MC-before-MH forward order, MH-before-MC rollback order) | **Not mine, Minor-judgment** | operations |
| `docs/runbooks/mc-deployment.md` (OPS-1: second rollback carve-out, do NOT roll MC back on a decline spike) | **Not mine, Minor-judgment** | operations |
| `docs/runbooks/mh-incident-response.md` (OPS-4: new Scenario 15) | **Not mine, Minor-judgment** | operations |
| `docs/runbooks/mc-incident-response.md` (OPS-4: cross-pointer only, NOT a second scenario) | **Not mine, Minor-judgment** | operations |
| `infra/docker/prometheus/rules/mh-alerts.yaml` (OPS-3/OPS-4: three alerts landed plus MHHighWebTransportRejections description) | **Not mine, Minor-judgment** | operations |
| `crates/env-tests/tests/26_mh_quic.rs` (fifth session: RESTORE `#[ignore]` with a task-25/26 reason + header item 7 + the doc paragraph that claimed the ignore was gone; earlier sessions had deleted it) | **Not mine, Minor-judgment** — shared row; the delete was landed by media-handler by agreement, the restore by meeting-controller with @paired-media-handler unreachable this session (flagged to @team-lead, not silently assumed) | media-handler |
| `scripts/release-feature-gate.test.sh` (add `mc-service` GATES row; update its "deliberately absent" comment) | Mine | — |
| `crates/mc-service/src/lib.rs` (reflow `compile_error!` literal so the needle lands on one source line) | Mine | — |
| `docs/TODO.md` (fifth session: R-15 entry refreshed and kept open; Gate-3 the `validate_16bit_id()` misreading corrected at both sites and the blast-radius `warn!`/blockquote pointer re-pointed; also carries reviewer-filed Gate-3 entries authored by @operations §Observability Debt and @security §Media Path Obligations S6) | Mine for the R-15 entry (names meeting-controller as co-owner); reviewer-filed entries owned by their authors | — (@paired-protocol withdrew their competing edit; see Planning §1) |
| `docs/devloop-outputs/2026-09-05-sender-id-binding-contract/main.md` | Mine | — |

**`crates/proto-gen/**` deliberately has no "regenerated output" row.** That output is
`build.rs`/OUT_DIR-generated and **not tracked in git**; the tracked files are `build.rs`,
`src/lib.rs` and `tests/*.rs`. Only `tests/internal_roundtrip.rs` is edited and it has its own row.
An "output" row would send a future reader hunting for a generated file that does not exist.
`build.rs` and `src/lib.rs` are **not** touched (no `skip_debug` is owed — see Planning §2). TS proto
artifacts are gitignored, so no row is owed for those either.

**Runbook and alert rows are RULED, not provisional.** @operations ruled the alerts **LAND, not
file** — with the reasoning that decides it: *the TODO entry being closed is about "healthy but
forwarding nothing" being undetectable, and a counter only a dashboard reader ever sees does not fix
that blindness — it relocates it.* Deferring the alerts on the very commit claiming the entry's
closure would re-open the entry under a different name, with no TODO telling the next reader so.
The countervailing rule binds too: **no alert without a runbook**, so both alerts land with
`runbook_url` anchors into the new §Scenario 15, and **alert and scenario land together or neither
lands**.

**The log-line sweep has exactly three homes**, established by @operations' grep rather than
assumed: `docs/observability/metrics/mh-service.md:361` (**the one live edit** — inside a blockquote
that also asserts "every metric here reads ZERO on a production pod today" and gives a discriminator
PromQL premised on zero forwarding, *all* of which becomes actively wrong on this commit);
`docs/TODO.md:913` (self-resolving — inside the entry I delete); and
`docs/user-stories/2026-08-27-hear-yourself-through-handler.md:501` (the task prompt itself —
historical record, **leave it**). **No runbook carries the string**, so my earlier worry about
missing a home was unfounded.

**No `crates/common/` row.** See Planning §1 — the task's instruction to add a shared validator
there rests on a false premise, and adding one is barred at five sites.

---

## Planning

### 0. Mechanism restatement (before the file list)

**Instance language** (how the task is written): *"MH cannot learn a participant's `sender_id`."*

**Mechanism language**: *MC-allocated per-participant facts reach MH over two disjoint control-plane
channels with different lifetimes — the **meeting-scoped policy push** (`RegisterMeeting`, sent once
at first-participant join) and the **per-connection notification round-trip**
(`NotifyParticipantConnected`). A fact that only becomes addressable when a **specific connection**
arrives structurally cannot ride the meeting-scoped push, because that push already happened before
the connection existed. `sender_id` is the first such fact, and the notification response is the
only channel in the tree with the right lifetime.*

This reframing earns its keep in one concrete way: it explains why the four rejected alternatives
were rejected as a **class** rather than one at a time. The meeting token is minted by **AC** (GC only
*requests* it) and handed back through GC to the client at join, before MC allocates the ordinal at
media-connect time — so neither the requester nor the minter holds the value at mint time (wrong
*producer*; attribution corrected by @auth-controller); `RegisterMeetingRequest` carries no `participant_id` (wrong *scope*);
`MhConnectRequest` carries only `join_token` and the field that could have carried a server-issued
binding was deleted by the task-4 reshape (wrong *trust direction* — client-presentable); reading it
off the SFrame key id is barred by §4 keyless relay (wrong *layer*). All four fail the same test.

**Honest scope check on the wider class.** I looked for same-owner siblings and did **not** find a
population that justifies building a general envelope:
- **Server-mute is NOT a sibling.** I checked: MH has no mute knowledge at all
  (`grep server_mute crates/mh-service/src` is empty). Mute is enforced by MC *omitting* the muted
  publisher from the pushed candidate set (`crates/mh-service/src/media/forward.rs:179`). It already
  has a channel, and it is meeting-scoped, so it rides the push correctly.
- **`kek_generation` at rotation** is a plausible sibling shape, but rotation is deferred in this
  story and it may well be meeting-scoped too, which would put it on the push.

So the sibling count today is **one confirmed member: `sender_id` itself.** I am therefore **NOT**
proposing a "MC's answer about this participant" envelope message, and **NOT** widening scope. The
one thing the reframing does change: the field should be shaped so a second per-connection fact
lands as an *added field* rather than as new plumbing — which a bare `uint32 sender_id = 2` on the
existing response already is, at zero cost. Nothing to build; recorded so the next reader doesn't
re-derive it.

### 1. DEVIATION FROM THE LITERAL TASK TEXT — `validate_16bit_id()`

> **Recorded as a deviation, not an omission, at @team-lead's direction.**
>
> | | |
> |---|---|
> | **Instruction not followed** | *"validate through `common`'s `validate_16bit_id()` so the bound stays single-sourced"* (task prompt; `docs/TODO.md:909`) |
> | **False premise** | `docs/TODO.md:909` states the helper is *"already referenced by `sender_id` at `internal.proto:51`"*. `internal.proto:50-51` is a **prohibition**, not a reference. The helper **does not exist anywhere in the tree**. |
> | **What we did instead** | Validate via the existing `mh_service::routing::SenderId::from_wire()`, whose bound is already `const _`-pinned to `media_protocol::frame::KEY_ID_SENDER_ID_BITS`. No new helper, no new constant. |
> | **Ruling** | @team-lead, Gate 1 — *"the task text names a mechanism; its stated goal is 'so the bound stays single-sourced', and the goal selects non-collapse."* |
> | **Confirmed by** | @paired-protocol (ruling author; confirms the misreading was theirs), @code-reviewer, @security, @paired-media-handler — four specialists, four independent routes, one conclusion. @dry-reviewer outstanding. |

#### The reasoning in full

The task prompt and `docs/TODO.md:909` both instruct: validate *"through `common`'s
`validate_16bit_id()` so the bound stays single-sourced"*, and the TODO asserts the helper is
*"already referenced by `sender_id` at `internal.proto:51`"*.

**Ground truth, checked rather than assumed.** `validate_16bit_id()` **does not exist** — not in
`crates/common/`, not anywhere in the tree. `grep -rn 'validate_16bit_id\|MAX_16BIT_ID'` returns six
hits and **every one is a prohibition**:

| Site | Says |
|------|------|
| `proto/dark_tower/internal/v1/internal.proto:50-51` | "Do NOT introduce a shared `MAX_16BIT_ID` constant or a shared `validate_16bit_id()` helper collapsing the two" |
| `proto/dark_tower/signaling/v1/signaling.proto:611-612` | same prohibition |
| `crates/mc-service/src/media_signaling/capability.rs:39-40` | same prohibition |
| `crates/mh-service/src/routing/mod.rs:162-163` | same prohibition |
| `docs/specialist-knowledge/dry-reviewer/INDEX.md:70` | records the non-collapse as a deliberate false-positive boundary |
| `docs/specialist-knowledge/dry-reviewer/INDEX.md:71` | restates it *"one story later"* |

So `internal.proto:51` is a **negative** reference, and the Gate-3 ruling read it as a positive one.
The instruction rests on a false premise: there is no helper to route through, and creating one is
barred at five sites across three story tasks.

**Resolution I am following: the non-collapse reading. No `common::validate_16bit_id()` is created.**

The reason is not deference to the comments — it is that **the single-sourcing goal the instruction
states is already met, and met more strongly than the helper would meet it**:

- The 16-bit bound has **exactly one home today**: `media_protocol::frame::KEY_ID_SENDER_ID_BITS`.
- Both consuming types derive from it by `const _` assert, and both files deliberately contain no
  local `65535`/`1 << 16` that could drift: `crates/mc-service/src/media_admission/sender_id.rs`
  (producer) and `crates/mh-service/src/routing/mod.rs` (consumer).

Adding `common::validate_16bit_id()` would therefore **add a second home for the bound** (the thing
single-sourcing exists to prevent) *while* collapsing two deliberately distinct concepts —
`sender_id`'s non-recycling **allocation** bound vs `slot_id`'s per-subscriber, freely-reusable
**validation** bound, whose zero-validity even differs (`slot_id` 0 is valid; `sender_id` 0 is
reserved-invalid). That is a net loss on both counts: strictly worse single-sourcing *and* a
semantic collapse.

**RULED AT GATE 1 BY @paired-protocol** (the author of the original Gate-3 ruling, who confirms
the misreading was theirs): resolution (b), no shared helper, and the `internal.proto:50-51`
prohibition **stays as written** — no comment edit is owed in this diff. They add one argument
stronger than mine, which is now the load-bearing one:

> `crates/common/Cargo.toml` has **no `media-protocol` dependency, and must not gain one** —
> `common` is depended on by every service including AC, which has no business in the media path.
> So a `common::validate_16bit_id()` would have to **hardcode `65535`/`u16::MAX` locally**: a
> second, *unanchored* home for the bound, sitting next to the one home that is currently pinned by
> `const _` assert on both sides. That is the literal inverse of the instruction's purpose.

**The instruction names a mechanism; the goal is what binds, and the goal selects (b).**

**What we do instead — and it turns out to require no new code at all.** `mh-service`'s existing
`routing::SenderId::from_wire(u32)` (`crates/mh-service/src/routing/mod.rs:141`) *already* produces
exactly the three-way semantics the task specifies, 1:1:

| Wire value | `from_wire` result | Task-specified semantics |
|---|---|---|
| `0` | `Err(IdError::SenderIdZero)` | reject loud (protocol error, counted) |
| `1..=65535` | `Ok(SenderId(NonZeroU16))` | bind |
| `>65535` | `Err(IdError::SenderIdOutOfRange)` | reject |

So the task's *"validate through `common`'s `validate_16bit_id()`"* clause is **satisfied as to its
stated purpose** by an existing, already-anchored helper. Nothing new is written on the MH side
beyond wiring the two error variants to counted reject reasons. Zero new constants, zero new homes,
bound still single-sourced at `KEY_ID_SENDER_ID_BITS`.

**Still owed at Gate 1: @dry-reviewer and @code-reviewer confirmation.** @paired-protocol has ruled
and will carry the same statement in their Gate-3 verdict. If either of the remaining two prefers
the literal reading, say so and I take it back to @team-lead rather than pick.

**Sequencing conflict, raised and resolved at Gate 1.** @paired-protocol initially intended to
correct `docs/TODO.md:909`'s false *"already referenced"* claim as part of their diff — reasonably,
since the misstatement is theirs. But **line 909 is inside the R-15 entry, which this devloop
DELETES on completion** (§7.5): two edits to the same doomed lines, for zero durable gain plus a
merge conflict between us. **@paired-protocol has withdrawn that edit**; their diff is exactly two
files (`internal.proto`, `crates/proto-gen/tests/internal_roundtrip.rs`) and I own the single
`docs/TODO.md` edit, which is deleting the entry outright. The durable record of *why the
instruction was not followed as written* lives here in `main.md` and in their Gate-3 verdict —
where a future reader retracing the decision will actually look — and the four in-tree prohibitions
plus two dry-reviewer INDEX entries already warn the next person off `validate_16bit_id()` better
than a seventh copy would.

### 2. Proto side — @paired-protocol owns, I do not touch

`NotifyParticipantConnectedResponse` gains `uint32 sender_id = 2;` (tag 1 is `acknowledged`).
Bare `uint32`, **not** `optional` — user override of the Gate-3 ruling, not re-litigated here.
Rationale recorded at the field: *"optional was specified for an N-1 world that does not exist;
revisit only if a mixed-version rollout ever becomes real."* Documented semantics: `0` => reject
loud (protocol error, counted); `1..=65535` => bind; `>65535` => reject. Additive field on an
existing `v1` message, so no `vN` bump and `buf breaking` passes. Comment shape is @paired-protocol's call and they have
made it: reference form following the `SubscriberSlot` idiom at `internal.proto:41-51`, plus **two
pieces of local semantics the reference cannot carry** — (a) *what `0` means on this specific
response*, since at `JoinResponse.sender_id` 0 is simply never sent whereas here it is a **defined,
legal-on-the-wire signal** ("MC could not resolve this participant"), which is where the
never-a-made-up-id rule belongs; and (b) the user-override rationale verbatim.

**`skip_debug` is NOT owed on this message**, ruled by @paired-protocol against a precedent I would
otherwise have had to guess at: `crates/proto-gen/src/lib.rs:136` and `:181` deliberately print
`sender_id` unredacted in the hand-written redacting `Debug` for `JoinResponse` and `Participant`.
`sender_id` is an identifier, not a credential, and the §11 rule governs **metric label dimensions
and structured log fields**, not `Debug`. So: no `skip_debug`, no `sender_id` metric label anywhere,
no `sender_id = %..` structured log field. Where a counted reject needs to distinguish causes, that
is a bounded `reason` vocabulary (`sender_id_zero` / `sender_id_out_of_range`), **never the value**.

> ### ⚠ Layer 6 green is NOT evidence this edit is wire-safe
>
> `dark_tower/internal/v1/internal.proto` is currently inside `proto/buf.yaml`'s `breaking.ignore`
> (added 2026-09-01 for the ADR-0036 reshape, under a user-granted acceptance covering the whole
> story). **While that key is present, no break of any kind is caught in this file.**
> `scripts/lang/proto/breaking.sh` prints `SUPPRESSED=<paths>` on every Layer-6 run, so the pipeline
> is not silently green — but the compensating control is *a human reading the diff*, which is
> @paired-protocol's Gate-3 job, not a tool's. @team-lead: do not read a green Layer 6 as coverage
> for this row.
>
> **Do NOT narrow the ignore list to "fix" this.** `buf.yaml` says so explicitly at
> `proto/buf.yaml:41-46`: both files are under one user-granted acceptance for the whole reshape,
> and narrowing re-fires a human escalation per task for a decision already made. Precision is
> restored by deleting the whole key after merge, not by trimming it now.

**The regen breaks compilation at exactly two sites, loudly — which is the desired behaviour**
(no `..Default::default()` at either, so neither can silently default to the reject value `0`):
`crates/mc-service/src/grpc/media_coordination.rs:116` (mine) and
`crates/mh-service/tests/common/mock_mc.rs:140` (@paired-media-handler's). **This dictates the
implementation order: proto + regen first, then the two sides in parallel.** Field-level wire
coverage is owed to `crates/proto-gen/tests/internal_roundtrip.rs` (@paired-protocol owns and will
extend). The TS codegen oracle needs no change — `packages/proto-gen/scripts/verify-codegen.sh`
asserts message-type *symbols*, not fields.

### 3. MC send side — mine

Today `McMediaCoordinationService` (`crates/mc-service/src/grpc/media_coordination.rs`) holds only
`Arc<MhConnectionRegistry>` and answers `{acknowledged: true}`. It has no route to the meeting
actor, which is where the allocated id lives.

1. **`McMediaCoordinationService` gains `Arc<MeetingControllerActorHandle>`** (constructor + the one
   wiring line at `crates/mc-service/src/main.rs:378`).
2. **New narrow actor message** `MeetingMessage::GetParticipantSenderId { participant_id,
   respond_to: oneshot::Sender<Option<SenderId>> }` + `MeetingActorHandle::get_participant_sender_id`.
   **Deliberately not `get_state()`**: `GetState` clones the entire roster — every `ParticipantInfo`,
   including display names and identity public keys — to answer a one-field question, on a path that
   runs once per media connection. The narrow message is O(1), allocates nothing, and keeps the
   read surface to the single field the caller is entitled to. That is the §11 read-surface
   discipline applied to an internal path, not just the wire.
3. **Resolution in the handler**, after the existing registry add:
   - `get_meeting_handle(meeting_id)` → `Err(MeetingNotFound)` => **`0`**
   - `get_participant_sender_id(pid)` → `None` (not on the roster) => **`0`**
   - `Some(s)` => `u32::from(s.get().get())`, always in `1..=65535`
   **`0` is a real, honest answer meaning "I do not know this participant."** MC never invents,
   never defaults to a neighbour's id, never reuses a departed one. The type system already helps:
   `SenderId` wraps `NonZeroU16` and has **no production constructor** — only the allocator makes
   one — so a made-up id is not merely discouraged here, it is unconstructible.
4. **The reply is `{acknowledged: true, sender_id}`.** `acknowledged` keeps its existing meaning
   (the notification was received and the registry updated); it is *not* repurposed to mean "and I
   resolved a sender". The two are independent and MH must read `sender_id`, not `acknowledged`.

### 4. MC observability — RULED by @observability

An unresolvable participant must be *countable*, not just loggable. **Ruled, not proposed:**

```
mc_media_sender_binding_responses_total{outcome, key_custody}
```

- **Name accepted as proposed.** @observability considered `..._resolutions_total` for verbal-noun
  symmetry with `mc_media_policy_pushes_total` and rejected it: the counter increments once per
  **response MC sends**, which is the boundary that makes it comparable to MH's counter across the
  wire, and naming it after the thing that crosses the wire makes that pairing legible.
- **`key_custody` is REQUIRED — I had this wrong.** My plan said *"`outcome` is the only label"*.
  That is right about identity fields and **wrong about `key_custody`**: ADR-0036 §11 says every
  service reports `key_custody=operator` in logs and metrics, and both existing `mc_media_*` metrics
  already carry it. Use `common::observability::labels::{KEY_CUSTODY_LABEL, KEY_CUSTODY_OPERATOR}`;
  do not re-spell the literals. **Cardinality 3** (3 outcomes × 1 custody).
- **The three-value split is UPHELD**: `resolved` | `meeting_unknown` | `participant_unknown`. I had
  offered that collapsing to a single `unresolved` was defensible; @observability ruled against it
  and added the argument that decides it — `meeting_unknown` is a routing/registration fault whose
  remedy is in the MC↔MH registration path, `participant_unknown` is an expected low-rate join race.
  Collapsing them **mixes a should-investigate condition with expected background**, and
  `mh-service.md`'s own drop-reason rule ("a counter that mixes expected shedding with an invariant
  violation can never be alerted on") is the in-tree statement of why that is a bad trade.
- **Why it is not redundant with MH's counter** — the test it had to pass: MH sees `sender_id = 0`
  and **structurally cannot** distinguish "MC has no such meeting" from "MC has the meeting but not
  this participant". Only MC can split those, and they triage differently. **The catalog entry must
  say this explicitly**, because it is the sentence that stops a future reader deleting this counter
  as duplicative.
- Bounded at the type level with `ALL: [Self; 3]` + wildcard-free `as_label()`, following the
  existing `PolicyPushOutcome` shape. `outcome`, not `status` — metric-local taxonomy, each value a
  distinct remedy.

**EMISSION SITE — this one bites.** The `counter!` **must** live in a `record_*` function in
`crates/mc-service/src/observability/metrics.rs`, never inline in `grpc/media_coordination.rs`.
`crates/dt-guard/src/metric_coverage.rs` and `application_metrics.rs` scan
`crates/<svc>/src/observability/metrics.rs` **and nothing else** (`application_metrics.rs:366`), so
an inline emission is invisible to `metric_no_catalog`, `metric_no_dashboard` **and** test-coverage
alike — and **all three would report clean**. That is Assertion Vacuity mechanism 1: the check runs
over an empty set and looks identical to a genuine pass. The handler calls
`observability::metrics::record_...`; the macro stays in `metrics.rs`.

**Increment boundary**: once per `NotifyParticipantConnected` response MC emits, on every path that
produces a response. If the RPC fails before a response is formed, **nothing increments here** —
that failure is MH's `declined_mc_unavailable` and MC's transport-level metrics.

**Catalog**: `docs/observability/metrics/mc-service.md` §Media Routing, alongside
`mc_media_policy_pushes_total` (same MC↔MH control plane). Needs the 3-value outcome table with a
responder-remedy column, `key_custody`, `Cardinality: 3`, the increment boundary, `Recorded in:`,
`Dashboard:`, and the cross-reference to MH's counterpart series.

**Dashboard** (mandatory — `application-metrics` check 5; design ruled, not a placeholder):
`infra/grafana/dashboards/mc-overview.json`, row `Media Routing (ADR-0036 §8)` (row id 55), after
panel 57. **Panel id 63** (1-62 and 157 taken). `timeseries`, title
`Sender Binding Responses by Outcome`, query
`sum by(outcome) (increase(mc_media_sender_binding_responses_total[$__rate_interval]))`, legend
`{{outcome}}`. Target **must** carry explicit `editorMode` and one of `range`/`instant` or check 7
(`target_query_fields`) fails.

**Test coverage**: `MetricAssertion` (ADR-0032), iterating the outcome enum's `ALL` rather than a
hand-written value list, **with a positive control that must hit** so an empty result is
distinguishable from a pass.

**§11 confirmations from @observability**, both of which I had right but neither of which I had
evidence for: `sender_id` never a label value / span attribute / log field
(`mc-service.md:371-373` records this reading verbatim); and the existing handler's `info!` carrying
`participant_id` and `meeting_id` **stays as-is, no finding** (`mc-service.md:365-370` records that
§11's flat meeting-id prohibition is scoped to *metrics*, and MC's connection-lifecycle logs carry
`meeting_id` by convention). Do not extend the field set on any line touched.

**Self-check owed by reading, not by guard**: the `\b…\b` matchers treat `_` as a word character,
so `\bsender_id\b` does not see `sender_kid` / `mh_sender_id` / `senderId`. A clean `metric-labels`
run is **not** evidence for this diff.

**MH's metric was renamed in the same ruling**: `mh_media_session_starts_total` (not
`mh_media_session_starts_total`), 4-value outcome space — 5 if `declined_sender_binding_conflict` is admitted.
The old name survives nowhere: its two homes (the `docs/TODO.md` R-15 entry and the
`Obligation (open)` paragraph in `mh-service.md`) are both **deleted** by this diff.

### 5. MH receive side — @paired-media-handler owns, I do not touch

Ordering `JWT gate → NotifyParticipantConnected → bind(sender_id) → start_media_session`; validate
via the existing `routing::SenderId::from_wire()` per §1 (no new validator); reject-and-close on `0` and on out-of-range, with a counted reason;
emit `mh_media_session_starts_total`. `SenderBindings::bind()`
(`crates/mh-service/src/session/mod.rs:822`) finally gets its production caller, retiring the
`"Media forward path not started: no sender_id is bound"` dead end at
`crates/mh-service/src/webtransport/connection.rs:558`. Unbind needs no new contract — MH drives it
on its own teardown and `NotifyParticipantDisconnected` already exists.

**Open question for @observability — NOT a recommendation from anyone else.** The TODO's proposed
two-value label space may be too coarse: `declined_no_sender_binding` covers MC answering `0`, but
an **out-of-range** answer is a different fault (contract violation, not honest ignorance), and
**"MC unreachable / RPC failed"** is a third. Three distinguishable causes, one proposed label value.

**Attribution corrected at @paired-protocol's own request, and the correction is worth recording
because it is a good one.** I had carried their input as *"protocol suggests a richer `reason`
vocabulary, which is a better answer than the TODO's single value."* They asked me to withdraw that:
the catalog is @observability's SSoT for name and label space, and *"a protocol specialist proposing
a richer label set in a planning thread is exactly how an owner gets pre-empted by the time they
arrive."* The correct split:

- **@protocol's §11 constraint (theirs to assert):** the `sender_id` **value** never becomes a label
  value or a structured log field.
- **Whether `declined_no_sender_binding` splits into distinguishable reject reasons: an open
  question for @observability**, framed as a question rather than an answer they must argue down. If
  they rule for the single value, protocol has no objection — the distinguishability can live in the
  `warn!` message, which is not a metric dimension.
- **Capability report, explicitly not a recommendation:** MH *can* distinguish the two id faults,
  because `routing::SenderId::from_wire()` already returns distinct error variants. The information
  exists if @observability wants it.

### 6. The behaviour change — and a CORRECTION to how it was framed

**I framed this wrong, and so did everyone else, until @security disproved it. The correction
matters because the wrong framing is the one that gets cited later to argue the gate back off.**

**What I originally wrote** (and carried to @operations, @security and @team-lead): making the
response load-bearing converts an advisory notification into a blocking gate on MH's media accept
path, so *"a slow or unavailable MC now fails media connections that previously came up"* — security
bought with availability, fail-closed, better diagnosis and worse availability.

**Why that is wrong.** *The binding **is** the response.* A failed `NotifyParticipantConnected`
means there is no `sender_id`, and therefore no forwarding — **under the old behaviour and the new
one alike.** There is no state in which the old behaviour delivered media that the new one refuses.
What the old behaviour actually preserved was **a WebTransport connection to a media handler
structurally incapable of carrying media**: a client that believes it is in a call, a handler that
is green on every dashboard, and silence.

**So this is a VISIBILITY change with near-zero availability cost, not a trade.** The population
that "loses" a connection under the new behaviour is exactly the population that was already getting
no media — they now find out. @team-lead has accepted the change; this section records *why* the
acceptance costs less than the original framing implied, so a future reader does not "restore
availability" by removing the gate.

**@operations was sent the trade framing before this correction landed and has been re-notified.**

**What genuinely does change**, and is the only real cost: `declined_mc_unavailable` closes the
connection, which is a **reconnect amplifier against a flapping MC** — every client retries, and
during an MC outage or rolling deploy that is a synchronized herd against the service that is
already the bottleneck (@security S10). @paired-media-handler is landing bounded jitter on **that arm
only** — the other arms are MC's *answer*, where retrying is pointless and immediacy is correct — as
a named const with the rationale at the site. The client-visible half (a retryable-transient vs
terminal close **code** the client's backoff can read) is a wire-visible change on a surface neither
of us owns, deferred to `docs/TODO.md` at Gate 3 naming **client + operations**, with @security's
agreement that it is a legitimate cross-owner boundary.

Latency: worst case ~48s of existing MC-client retry before a decline, for one connection. **That
budget is repriced by this change, and it happens to be repriced *toward* keeping it** — see §7
item 20. I originally wrote "the pre-existing budget, unchanged", which was right on the number and
wrong on the reasoning. Accept-loop throughput is unaffected (`handle_connection` is spawned per
connection at `webtransport/server.rs:204`).

### 7. Definition of done

**Contract + closure**

1. Proto field lands (@paired-protocol) + proto-gen regenerated.
2. `bind()` has a production caller (media-handler side).
3. `#[ignore]` deleted from `26_mh_quic.rs::test_mh_forwards_an_audio_datagram_back_to_its_sender`
   **and it passes against the live cluster**. A green Layer 7 with the ignore still present is
   explicitly NOT done.
4. Both metrics have catalog entries and pass `validate-metric-coverage.sh`.
5. `docs/TODO.md`: the R-15 entry's own four-part closure condition is met, so the entry is
   **DELETED** (it says so explicitly), and the "Obligation (open)" declined-counter paragraph
   inside it is resolved by the metric landing rather than carried forward.

**Test obligations — folded in from @test's Gate-1 review, all accepted**

6. **POSITIVE CONTROL — the item that decides whether any of this is actually proven.** @test
   raised, and I accept as the single most important finding at Gate 1, that **the env-test we are
   un-`#[ignore]`ing cannot fail if the binding is wrong.** It is a single-participant loopback, so
   *"MH takes the only/first sender in the pushed policy as the binding"* produces output identical
   to the correct implementation. §6 named that blind spot as design rationale; @test's point is
   that naming it in prose does not test it, and R-15's proof would ship green through all seven
   layers with an ordinal-inference defect intact.
   **Required: a test arm that FAILS if the binding is wrong** — two connections with distinct
   `participant_id`s receiving distinct bindings, proving a datagram on connection A is never
   delivered as if from B. **Tier is media-handler's call** (env-tier is ideal but AC's 5/hour rate
   limit and the `SHARED_USER` pattern may make a two-user meeting costly; a component-tier positive
   control in `crates/mh-service/tests/media_forward_integration.rs` is acceptable). **Existence is
   not optional, and existence is not the bar.** Per @team-lead's hard gate on the "Ready for
   validation" signal, ratified by @test as the same standard the MC arm (item 8) was held to:
   these arms must be **shown RED against a deliberately wrong binding (mutation-verified), not
   merely present and green.** The MC side already demonstrated why — an arm that satisfied
   @security's derive-from-the-artifact rule still passed a hardcoded-`1` mutation (§Lessons
   Learned 1), so "the arm exists and is green" is precisely the state that does not prove the
   control works. @paired-media-handler must confirm the mutation was run and the arms went red
   before I signal. This also closes @test's item-7 caveat: the env-test's
   `assert_eq!(view.stream_id(), 0)` passes degenerately because in a one-participant loopback the
   subscriber's own slot 0 coincides with `MAIN_AUDIO_SLOT_ID`'s default.
7. **Both reject arms, each counted** (MH component tier): `sender_id == 0` **and**
   `sender_id > 65535`, each asserting reject-and-close **and** the counted outcome. Wired to
   `routing::SenderId::from_wire`'s `SenderIdZero` / `SenderIdOutOfRange` per §1.
8. **MC arm** (mine — `crates/mc-service/tests/media_coordination_integration.rs` plus unit tests in
   `media_coordination.rs`): meeting-unknown => `0`, participant-unknown => `0`, resolvable =>
   allocated id in `1..=65535`. **The wire value `0` is asserted on both unresolvable paths.** The
   `NonZeroU16`/no-production-constructor argument in §3 is a structural guarantee, not a
   substitute for asserting the observable; @test is right to want both.
9. **The mock MC must not paper over the reject arms.** `crates/mh-service/tests/common/mock_mc.rs`
   gains a configurable `sender_id`, and **its default must not make every MH accept-path test pass
   regardless**. At least one test drives the mock returning `0` and asserts MH rejects.
10. **Metric coverage drives BOTH label values**, not just the happy path — `MetricAssertion`
    component tests per ADR-0032 with partial-label `assert_delta(0)` adjacency discipline, for
    `mh_media_session_starts_total` and for `mc_media_sender_binding_responses_total`.
11. **Ordering-regression coverage.** Making the notification load-bearing turns MH's accept path
    from fire-and-note into await-and-use, so `webtransport_integration.rs` (provisional-accept
    timeout, `await_meeting_registration`) and `mc_client_integration.rs` (retry/backoff) must be
    **updated with coverage that would catch a wedge or timeout regression — not merely made to
    compile.** A slow or unavailable MC must produce an observable reject rather than a hang.

**Operational obligations — from @operations' Gate-1 review (OPS-1..7), all accepted**

12. **Rollout ordering committed to BOTH runbook homes (OPS-1, blocking).** Forward: **MC first,
    then MH** — new MH + old MC decodes `0` and MH declines *every* session, a total media outage
    for the skew window, not a partial degradation. Rollback: **MH first, then MC** — `kubectl
    rollout undo deployment/mc-0` alone returns MC to a binary that never sets `sender_id`, and a
    new MH then declines everything. This is the **identical mechanism** to the existing carve-out
    at `docs/runbooks/mc-deployment.md:1431` for `no_applied_generation`; second instance of one
    shape, so it gets its own line rather than being left for a reader to infer at 3am.
13. **Triage line naming image skew as cause #1 (OPS-2).** @operations verified the user-override
    premise holds — `infra/kubernetes/overlays/` contains only `kind`, no Terraform, PROJECT_STATUS
    Phase 10 is Planned, **there is no deployed old MC anywhere**. Bare-`uint32` and `optional` give
    *identical* fail-closed behaviour, so the override costs no safety; it costs one **diagnostic
    discriminator** (old MC vs unresolvable participant both land on `declined_no_sender_binding`).
    In the Kind devloop cluster that makes *"mc-service image predates the field"* the single most
    likely cause of a decline for the rest of this story, so §Scenario 15 names it **ahead of** the
    join race.
14. **Two alerts LAND (OPS-4), with §Scenario 15, together or not at all.** (a)
    `declined_sender_binding_out_of_range` > 0 sustained 10m ⇒ `warning` — this reads zero forever;
    non-zero means MC violated its own contract **or the control plane answering MH is not MC**, and
    given @security's S6 (plaintext channel, no peer authentication of MC) this counter is the
    *only* signal separating those two. (b) Decline ratio ⇒ `warning`, threshold **> 0.20 for 10m**,
    deliberately reusing the ratified R-60 number rather than inventing one, with the trailing guard
    on the **full** denominator. **No third alert** on `declined_mc_unavailable` — covered by the
    ratio and correlated with the existing `MHMCNotificationFailures`; a third would be fatigue.
15. **The env-test must be shown EXECUTING BY NAME (OPS-7). Owner: media-handler, pending
    @team-lead's confirmation** — recorded with a name on it rather than left open.

    **@operations WITHDREW their first framing of this and I am carrying the corrected one**, at
    their request and under the same do-not-leave-a-withdrawn-finding-next-to-correct-ones rule they
    applied to their own admission-slot claim. The withdrawn version said `layer7.sh:440` skips the
    whole env-test suite in CI so a skip and a genuine pass "read identically on the page." On
    checking the site that is wrong: `layer7.sh:437` emits a **distinct `SKIPPED-NO-CLUSTER`
    STATUS token**, not `OK` (only the exit code is shared), the skip lane requires the helper
    socket absent **and** `GITHUB_ACTIONS` so **this devloop structurally cannot reach it**, and
    `layer7.sh:412-415` records that exit-0 silent-skip hole as already closed (task #56,
    @operations + @security).

    **The accurate finding is narrower, sharper, and actually on this diff.**
    `layer7.sh:828-836` runs `cargo test -p env-tests --features all` and keys on the **exit code
    only** ("no grep" is stated at the site); `rc == 0` ⇒ `OK env-tests-passed`. **A still-`#[ignore]`d
    test also yields rc 0.** So `env-tests-passed` cannot distinguish *"the test ran and passed"*
    from *"the test was ignored and therefore trivially did not fail"* — mechanism 5, live on this
    diff, because the attribute's presence is the one thing being changed.

    **Proof-of-execution artifact** (no new machinery — `layer7.sh:830` tees full libtest output to
    `$ENV_TEST_LOG`):
    - **Primary and only bar**: the literal line
      `test test_mh_forwards_an_audio_datagram_back_to_its_sender ... ok` in that log. Fail-closed —
      a surviving attribute reads `... ignored` instead. Gate-3 eyeball.
    - **NO `0 ignored` secondary.** @operations proposed it to catch a sibling `#[ignore]`; @test
      checked the file and it is **unsatisfiable** — after task 24 the binary still legitimately
      carries two justified ignores (`26_mh_quic.rs:977` otel, `:1216`
      `test_mh_disconnects_unregistered_meeting_after_timeout`), both with named component-tier
      coverage, so `0 ignored` would be RED forever. (A third instance of §Lessons Learned 3: a
      proposal corrected by reading the actual file. A sibling-ignore allowlist guard, if ever
      wanted, is a separate discussion, not this devloop.)
    - **Not** JUnit (no nextest — new machinery on the critical path for one assertion) and **not** a
      bare pass-count (fail-open, never names the test).
    - Kept as a **Gate-3 review check read by eye, not a grep committed into `layer7.sh`**: the
      needle is a test-function name, so a rename would silently turn a committed grep fail-open, and
      a one-shot proof does not warrant a permanent fail-open needle.

    A `precondition_fail cluster-rebuild-failed` is an **operator-lane** escalation, not an
    implementer bug — whoever drives the run must not "fix" it by re-running.

**DRY obligations — from @dry-reviewer's Gate-1 review**

16. **The catalog pointer must be RECIPROCAL.** @observability required the *MC* entry to state why
    the two counters are not duplicates. The **MH** entry needs the mirror sentence — that
    `declined_no_sender_binding` is the union of MC's two unresolved outcomes. Otherwise a reader
    arriving from the MH side (**the likelier direction**, since MH is where the connection visibly
    failed) hits nothing, and the sentence that prevents the deletion guards only one of two doors.
17. **Record the shared-denominator identity at the new counter's definition.**
    `sum(mc_media_sender_binding_responses_total)` equals
    `mc_mh_notifications_received_total{event_type="connected"}` **by construction** — same path,
    same validation gate (`media_coordination.rs:105`). Not duplication (mine carries strictly more
    information), but two series encoding one count will eventually be alerted on twice, or someone
    will "discover" the redundancy and delete the wrong one. One line at the definition.
18. **MEASURE the Layer-1 cost of the new `GATES` row, do not assume it.** @dry-reviewer accepts
    closing their `release-feature-gate` residual **conditional on this**: `GATES` sits in Layer 1
    and a `Cargo.lock` miss turns the row into a **cold release build of that crate's graph**.
    mc-service's graph is larger than mh-service's, and the MH row's ~1s is a *warm* measurement.
    If it lands materially above the MH row, say so at Gate 3 rather than letting Layer 1 quietly
    get slower.
19. **The "no `65535` literal" rule is NARROWED — @dry-reviewer's correction, and it protects a
    required test.** I relayed @code-reviewer's rule as *"no local `65535` or `1<<16` anywhere in the
    new code"*. As stated that is wrong: `crates/mh-service/src/routing/mod.rs:55-61` — the module
    docstring on the file that owns the prohibition — **sanctions boundary-test literals**, because
    *"a test written against the same constant as the code passes no matter what the constant
    becomes."* DoD item 7 requires a `sender_id > 65535` arm; under the blanket rule someone
    "fixes" that literal into a derived constant and the test then passes for **any** value the
    constant takes — a vacuous assertion. **Correct rule: no `65535`/`u16::MAX`/`1 << 16` in
    production code paths or in the proto comment; boundary tests name the literal outright.**

**OPS-3 — REVERSED by @operations after the framing correction; recorded because the reversal is the interesting part**

20. **KEEP the ~48s budget. Do NOT shorten it.** @operations first ruled the binding deadline must
    be bounded to ~10s, then **reversed themselves** once the visibility-not-trade correction landed,
    and the reversal is right: **with no client-side backoff today, the 48s is functioning as an
    accidental rate limiter on the reconnect herd.** Shortening it to 10s does not reduce load on a
    down MC — it **multiplies the retry rate ~5x** against the service that is already the
    bottleneck. A fast fail is only correct when the *client* backs off, and that half is precisely
    what is being deferred. Per arm: `declined_mc_unavailable` keeps 5s connect + 10s RPC × 3 with
    bounded jitter (jitter desynchronizes the herd; the long deadline rate-limits it), and the number
    is **stated in `mh-deployment.md`**; the two "MC answered" arms close immediately with no jitter,
    since retrying an answer is pointless.
    **The coupling MUST be recorded on the deferred TODO entry**: the retryable-vs-terminal close
    code and the binding deadline are **one decision, not two**. Shortening the deadline *before* the
    client can read a retryable close and back off **is** the failure mode. Left as two
    independent-looking items, a future reader tunes the timeout in isolation and turns a contained
    outage into a herd.
21. **Slot pressure IMPROVES under this change** — and the way this was got wrong is worth one
    line, because it happened **twice in opposite directions and both times the fix was to read
    `connection.rs:558`.** My "security bought with availability" framing and @operations'
    admission-slot concern were the same error: **comparing the new behaviour against an imagined
    old one rather than the actual one.** The actual old behaviour is at `:558` — log and proceed.
    Detail follows. — the opposite of a concern raised and then
    Under the **old** behaviour a connection with no binding is
    never closed (`connection.rs:558` logs and proceeds), so a useless connection holds its
    `MH_MAX_CONNECTIONS` admission slot **indefinitely**, to idle timeout. Under the new behaviour it
    holds for ≤48s and is released. Recorded as a property of the change, not as a live finding.
22. **OPS-3b SURVIVES the correction intact, and matters more under the corrected framing.**
    `record_webtransport_handshake_duration` (`connection.rs:281`) runs **before** the new blocking
    wait (`:306`), so up to 48s of media-connect stall is outside
    `mh_webtransport_handshake_duration_seconds` and `MHWebTransportHandshakeSlow` **cannot fire on
    it — a 48s stall reads as a healthy sub-second handshake.** That is this devloop's own
    "green on every dashboard while nothing works" shape reproduced one layer up, inside the diff
    that closes it. The decline-ratio alert covers it, which is a further reason those alerts land
    rather than get filed.

    **The histogram is deliberately NOT moved — @paired-media-handler's ruling, recorded here
    because it would otherwise read as an unfixed finding.** Moving the call past the binding step
    would make a metric catalogued as *"session accept through JWT validation"* silently absorb an
    **MC dependency wait**, so during an MC slowdown `MHWebTransportHandshakeSlow` would page
    pointing at MH's accept path when the fault is MC. **That is precisely the misdirection
    @operations is fixing in `MHHighWebTransportRejections`, reintroduced in a second place in order
    to close the first.**

    **The real residual is a different one and is FILED, not landed**: a binding that **succeeds
    slowly** — MC degraded but answering, so `started` climbs normally, the decline ratio is flat,
    the handshake looks healthy, and media connect is 40× slower with **every signal green**. That
    wants a *new latency instrument on the binding step*, which is cross-owner and Gate-1-shaped.
    Owner **observability + media-handler**.

    **ONE deferred TODO entry, not three.** @operations requires the close code and the binding
    deadline recorded as **one decision** (shortening the deadline before the client can read a
    retryable close *is* the failure mode, and two independent-looking entries invite tuning the
    timeout in isolation). @security requires the mirror clause: **jitter shipping is not grounds to
    close it**, because jitter acts on *phase* and the close code on the *existence of a termination
    condition*. The slow-binding instrument joins the same entry as the third face of one newly
    load-bearing dependency.

23. **No herd constraint is owed for an ordinary MC rolling deploy** — verified, not assumed:
    `mc-0`/`mc-1` are `replicas: 1` with `RollingUpdate` and no explicit `maxUnavailable`/`maxSurge`,
    so the 25%/25% defaults compute to **maxUnavailable 0, maxSurge 1** and the replacement must pass
    readiness before the old pod terminates. **The only rollout constraint is OPS-1's, and it is a
    version constraint, not a capacity one.** Two runbook one-liners: the surge window puts both MC
    versions live simultaneously (harmless *given* OPS-1's ordering, which is why that ordering is
    the only version constraint needed); and MC's readiness is `httpGet /ready` on **8081** while the
    binding call is **gRPC on 50052**, so if `/ready` greens before the gRPC server accepts, the
    surge opens a window where a Ready new MC declines bindings — fail-closed and counted, so it
    self-reports, but it will read as a `declined_mc_unavailable` blip **on every MC deploy** and
    Scenario 15 must say that is expected rather than page-worthy.

### 8. Gate-1 review findings — consolidated

All three sides are staffed and have reported. @observability has **ruled** the metric shape;
@security returned nine findings; @test returned six. Disposition below; every item is either
folded into §7 or has a named owner.

#### 8a. @observability's ruling — SETTLED, not to be re-litigated

`mh_media_session_starts_total{outcome, key_custody}` — `key_custody=operator` mandated by
`label-taxonomy.md` §Key custody. **Four outcomes**: `started` | `declined_no_sender_binding` |
`declined_sender_binding_out_of_range` | `declined_mc_unavailable`.

This **supersedes the TODO's two-value proposal and answers my §5 open question**, and it answers it
the way @security independently argued it must: the contract-violation arm (`>65535`) is *not*
merged into the benign-race arm (`0`). Precedent cited is `MediaDropReason`, which keeps
"should read zero forever" separate from "routine" so the counter is not an unalertable mixture.
`declined_mc_unavailable` covers the RPC-failure case I flagged as a third fault with no home.

Also ruled: `started` counts **at spawn of the three loops**, not at first frame (catalog and
rustdoc must say so *in words*); catalog entry goes in §Media Forward Path as the first `###` entry
and must state explicitly that it is **outside** the `forwarded + dropped = attempts` identity; the
flat-zero-is-not-idle blockquote is **rewritten, not deleted**; and the
`Media forward path not started: no sender_id is bound` log-line quote must be swept from both the
catalog and the TODO in the same commit that makes it wrong. Alerts/dashboard are **filed, not
landed** (@operations to route/threshold).

**My MC-side counter is still awaiting their ruling** — `mc_media_sender_binding_responses_total`
was not covered by the MH ruling.

#### 8b. @security findings — MC-side (mine)

- **S4 — MC hands out a binding for a connection it just refused to register. ACCEPTED, fixing.**
  `media_coordination.rs:94-105`: when `add_connection` returns `false` (per-meeting registry cap of
  1000, `mh_connection_registry.rs:73-81`) MC currently `warn!`s and still answers
  `acknowledged: true`. Under my plan it would also answer a **valid `sender_id`** — so MH would
  forward media for a connection MC is not tracking and will never send a
  `NotifyParticipantDisconnected` for, because there is no registry entry. **The two sides would
  disagree about whether the connection exists, invisibly.** Fix: `!added` ⇒ answer `sender_id: 0`.
  That is the honest answer under my own stated semantics ("I am not tracking this participant"),
  it is fail-closed, and it gets its **own `outcome` value** so cap-exhaustion is never triaged as a
  join race. I had missed this; it is a real hole in §3 as originally written.
- **S7 — the `test-seams` gate that makes my "unconstructible" claim true has no self-test.
  ACCEPTED, fixing, and it needs one extra step @security invited me to report.**
  `scripts/release-feature-gate.test.sh:65` has a single `GATES` row (`mh-service|per-frame-trace`)
  and its own comment records MC's row as deliberately absent. @security's repricing argument is
  correct and is the decisive part: that residual was priced when the seam was an *exhaustion
  bypass*; **this devloop makes the same seam the sole thing standing between the allocator and an
  arbitrary `sender_id` on the send path.**
  **The wrinkle**: the harness greps one fixed `NEEDLE_PHRASE`
  (`must never be enabled in a release build`) that must lie **wholly within one source line** —
  the file's own comment explains at length that this is what dodges the `\`-continuation hazard.
  In MC's literal (`crates/mc-service/src/lib.rs:128-131`) the needle **straddles the continuation**
  (`...must never be enabled \` / `in a release build`), so adding the row alone would yield a
  `PRECONDITION_FAILURE`, not a pass. **This is a two-line reflow of my own crate's literal**, not a
  blocker — reflow to `...bypass and \` / `must never be enabled in a release build`, then add the
  row. Per CLAUDE.md "fix, don't defer", both land here.

#### 8c. @security findings — cross-side, brokered

- **S1 (blocking) — `participant_id` silently changes role from payload to authorization query key.**
  Correct and important: today a wrong `participant_id` means a wrong log line; after this devloop
  it is **the key MC answers an identity question about**, and MH binds the answer to a media route.
  Three cheap fixes, all accepted: a named-invariant comment at
  `mh-service/src/webtransport/connection.rs:270-271` (values are token-derived and are now an
  authorization query key), the same statement at the proto field, and a test asserting MH's
  outbound request carries **the `sub` of the validated token** — not merely "a participant id" —
  with a JWT whose `sub` differs from anything the client sent. Also pinned: the `participant_id`
  passed to `bind()` must be the *same* value used for the request, never re-derived.
- **S2 (blocking) — merges with @test's item 1.** Same hole, found independently by two reviewers,
  which is itself signal. @security adds a second arm @test did not: **(b) two meetings, same
  `participant_id` string** — ordinal 5 exists concurrently in every meeting and the bindings map is
  keyed `(MeetingKey, participant_id)`, so a dropped meeting key is a cross-tenant leak that the
  two-participants-one-meeting arm **cannot** catch. Also: per-participant *correspondence*, not
  mere distinctness (a swap passes a distinctness assertion), and expectations **derived from the
  allocator's actual output**, never literals `1`/`2` that coincide with allocation order.
  Folded into §7 item 6.
- **S3 vs media-handler §4 — GENUINE CONFLICT, needs resolution before implementation.**
  @security wants `start_media_session` to **take** the validated `SenderId` as a parameter, so
  "started without a binding" is structurally unrepresentable and the `resolve`-returns-`None` dead
  end disappears. @paired-media-handler wants it to **keep resolving through the registry**, because
  that keeps the registry — not a local variable — the authority on which sender a connection
  publishes as. Both arguments are real. Brokered separately; see §8d.
- **S5 — `bind()` overwrites silently.** No conflict check on `(meeting, sender)` held by a
  *different* `participant_id` — the exact cross-participant collision the ordinal exists to
  prevent. Media-handler's tree; accepted for their side.
- **S6 — the MC→MH channel is plaintext with no peer authentication of MC.** @security is filing
  this themselves at Gate 3 (owners infrastructure + operations + security) and explicitly is **not**
  asking us to fix it. What we owe: state the trust assumption at the proto field and the bind site
  — **"unspoofable" here means network-isolated plus an MC-attested endpoint, not a
  cryptographically authenticated peer.** A reader who believes the stronger claim will make a worse
  decision later.
- **S8 — endorsed with an addition**: the WebTransport **close reason on the reject path must be a
  bounded `&'static str` carrying no identifier**. A reject arm is the natural place to
  "helpfully" include the offending value, and that value crosses a trust boundary back to the
  client.
- **S9 — agreement, with a sharper argument than mine or @paired-protocol's.** A collapsed helper
  would have to pick one zero-policy; `slot_id` 0 is **valid** and `sender_id` 0 is
  **reserved-invalid and is this contract's reject signal**. So a shared validator that accepts 0
  because `slot_id` allows it is a **fail-open on the exact value this devloop reserves for
  "reject"** — the binding contract's primary defence, defeated by a helper written to satisfy a
  single-sourcing instruction. Three specialists, three independent routes, same conclusion.

#### 8d. Resolutions reached at Gate 1

- **S3 vs media-handler §4 — RESOLVED.** @paired-media-handler concedes to @security:
  `start_media_session` **takes** `sender: SenderId` by value, and the `resolve()`-returns-`None`
  dead end at `connection.rs:551` (with its `"Media forward path not started: no sender_id is
  bound"` warn) is **deleted, not demoted** — *"an unreachable arm that reads like a live safety net
  is worse than no arm."* Their reasoning goes one step past my S5 observation and is better than
  it: S5 does not make the choice a style question, it **relocates the registry's purpose.**
  Post-S3 the registry has no reader at all, and S5 gives it the only one it needs —
  `SenderBindings` exists to make `(meeting, sender_id)` uniqueness **enforceable at the last hop**,
  because MH is the component that would act on a collision. `resolve()` ends up with zero
  production callers and is deleted rather than left as a read API nobody reads.
- **S5 — accepted, both halves**, with the collision check **inside** the `rcu` closure (a check
  outside the CAS loop would be a TOCTOU on the exact concurrent-connect race the `rcu` was written
  for). `unbind` becomes a **compare-and-remove** keyed on `connection_id`, because post-S3 a
  superseded connection's teardown would otherwise silently free that ordinal and **degrade the new
  S5 control**, not merely reproduce the old ordering accident.
- **S8 — met by construction, and better than requested.** `grep -rn '\.close(' crates/mh-service/src/webtransport/`
  is empty: MH never calls `wtransport::Connection::close()`, so **no reason string crosses to the
  client at all** — strictly stronger than the bounded `&'static str` @security asked for. No string
  is added. (The `&'static str` requirement therefore does not apply to my MC side either; MC's
  reject signal is the wire value `0`, not a string.)
- **A fifth `outcome` value is under consideration by @observability**, correctly referred rather
  than decided: `declined_sender_binding_conflict`. Folding it into `declined_sender_binding_out_of_range` fails
  the remedy test — an *impossible* ordinal and a *duplicate live* ordinal are different MC defects,
  and **only the duplicate is a cross-participant media-crossing primitive.**
- **Layer 7 — @paired-media-handler owns it**, pending @team-lead override. Recorded with a name on
  it rather than left open, since an unnamed owner is how the `#[ignore]` survives the devloop meant
  to delete it.
- **@auth-controller has made S1's two mitigations CONDITIONS of their Gate-3
  `Approved-Cross-Boundary` trailer**, and identified the uncovered half precisely: `bind()`'s
  existing docstring guards the *sender value*'s provenance but **not the `participant_id` key's**.

#### 8e. Still open at Gate 1

1. **My MC-side counter name/labels** — awaiting @observability.
2. **@dry-reviewer, @code-reviewer, @operations, @semantic-guard** have not yet reported; the §1
   resolution still owes @dry-reviewer's and @code-reviewer's confirmation per my brief.

#### 8f. What un-`#[ignore]`ing the env-test does and does not prove

Stated plainly here because the plan must not let the env-test's greenness read as coverage it does
not provide. `assert_eq!(view.stream_id(), 0)` in
`26_mh_quic.rs::test_mh_forwards_an_audio_datagram_back_to_its_sender` is satisfied by **any**
implementation, correct or not, because the subscriber's own slot coincides with
`MAIN_AUDIO_SLOT_ID = 0`. Un-`#[ignore]`ing it **closes R-15's stated criterion; it does not close
the injection question.** Component-tier arms (a) and (b) are what close that, and the env-test's
rewritten doc comment will cite them so the next reader finds them.

---

## Pre-Work

None.

---

## Implementation Summary

**MC send side — complete.** `NotifyParticipantConnected` now answers with the participant's
allocated ordinal, or `0` meaning "I do not know this participant".

- **`media_admission/binding_response.rs`** (new) — `SenderBindingOutcome`, the bounded `outcome`
  vocabulary: `resolved` | `meeting_unknown` | `participant_unknown` | `registry_full` |
  `user_ambiguous` (the fifth landed with the identity-translation fix). `ALL` plus a wildcard-free
  `label()`, following the `PolicyPushOutcome` idiom. The compile-force is `label()`'s exhaustive
  match — a variant with no spelling will not build; `ALL`'s completeness is a convention the unit
  test tripwires, not a length guarantee (corrected at Gate 3: an earlier note here claimed the
  array length was the guard, which it is not).
- **`actors/messages.rs` + `actors/meeting.rs`** — `MeetingMessage::GetSenderIdForUser` and
  `MeetingActorHandle::get_sender_id_for_user`, returning `SenderLookup::{Found, NotFound, Ambiguous}`.
  Narrow by design: `get_state()` clones the whole
  roster (display names, identity public keys) to answer a one-field question on a per-connection
  path. Resolution happens **inside one meeting actor**, which is what makes a cross-meeting answer
  unrepresentable rather than merely unlikely.
- **`grpc/media_coordination.rs`** — `McMediaCoordinationService` gains
  `Arc<MeetingControllerActorHandle>`; `resolve_sender_binding()` maps every path to
  `(wire_value, outcome)`. **Exactly one path yields non-zero: a live roster hit.**
  `acknowledged` keeps its existing meaning and is *not* repurposed to mean "and I resolved a
  sender" — MH must read `sender_id`.
- **@security S4 fixed** — a registry-cap refusal now answers `0` under its own `registry_full`
  outcome. Previously it would have handed back a valid ordinal for a connection MC is not tracking
  and will never send `NotifyParticipantDisconnected` for.
- **@security S7 fixed, including the obstacle they predicted.** Added
  `mc-service|test-seams|crates/mc-service/src/lib.rs` to `scripts/release-feature-gate.test.sh`'s
  `GATES`. The row alone would have yielded `PRECONDITION_FAILURE`: the harness greps one fixed
  phrase that must lie wholly within **one source line**, and MC's `compile_error!` literal split
  `must never be enabled in a release build` across the `\`-continuation. Reflowed by one clause,
  with a comment at the site explaining why the break position is load-bearing. **10/10 gate
  assertions now pass** (was 5/5, MH only).
- **Observability** — `record_sender_binding_response` lives in `observability/metrics.rs`, never at
  the call site: `dt-guard`'s `metric_coverage` and `application_metrics` scan only that path, so an
  inline `counter!` would leave three guards reporting clean over an empty scan set. Catalog entry
  and Grafana panel 63 landed per @observability's ruling. `key_custody` is carried from the shared
  constants (my plan's "`outcome` is the only label" was wrong; ADR-0036 §11 requires it).

**Verification beyond "the tests pass".** The positive control @security and @team-lead made a hard
gate was **run, not asserted**. Two wrong-binding mutations were applied to the resolver and the
suite observed to go red:

| Mutation | Caught by |
|---|---|
| "bind the first sender in the roster" | `distinct_participants_resolve_to_their_own_distinct_ids`, `unknown_participant_in_known_meeting_answers_zero` |
| "bind a hardcoded ordinal 1" | `distinct_participants_resolve_to_their_own_distinct_ids`, `resolvable_participant_gets_its_own_allocated_sender_id` |

**The first run found a real vacuity in my own test**, which is the argument for doing this rather
than reasoning about it: `resolvable_participant_gets_its_own_allocated_sender_id` initially
**passed** under the hardcoded-`1` mutation, because the first joiner *is* allocated 1 — so an
expectation correctly derived from the allocator still could not discriminate. Fixed by joining a
decoy first and asserting the id under test is `> 1`. Re-ran both mutations against the hardened
suite: each is now caught by two arms.

---

## Files Modified

**MC send side (mine)**

| File | Change |
|---|---|
| `crates/mc-service/src/media_admission/binding_response.rs` | **new** — `SenderBindingOutcome` + 2 unit tests |
| `crates/mc-service/src/media_admission/mod.rs` | register + re-export |
| `crates/mc-service/src/actors/messages.rs` | `GetParticipantSenderId` variant |
| `crates/mc-service/src/actors/meeting.rs` | handle method + actor arm |
| `crates/mc-service/src/grpc/media_coordination.rs` | controller dep, `resolve_sender_binding`, response fill, S4 fix, unit-test fixtures |
| `crates/mc-service/src/observability/metrics.rs` | `record_sender_binding_response` |
| `crates/mc-service/src/lib.rs` | `compile_error!` reflow (load-bearing for the GATES row) |
| `crates/mc-service/src/main.rs` | wiring |
| `crates/mc-service/tests/media_coordination_integration.rs` | 6 new arms incl. two-participant + two-meeting |
| `crates/mc-service/tests/otel_grpc_inbound_continuity.rs` | fixture for the new dependency |
| `scripts/release-feature-gate.test.sh` | `mc-service` GATES row + comment correction |
| `docs/observability/metrics/mc-service.md` | catalog entry |
| `infra/grafana/dashboards/mc-overview.json` | panel 63 |

**Scope-alignment edits (fifth session, 2026-09-08 — after the story manifest was amended out of
session to add tasks 25 and 26). These two are honesty edits, not functional ones: nothing in the
implementation changes, and no behaviour is added or removed.**

| File | Change |
|---|---|
| `crates/env-tests/tests/26_mh_quic.rs` | `#[ignore = "..."]` **restored** on `test_mh_forwards_an_audio_datagram_back_to_its_sender`, in the file's existing reason-string convention (cf. `:977`, `:1216`). The reason states that the binding contract **landed** at task 24 and is **not** the blocker, names task 25 (steering/placement ordering, `media_routing/assignment.rs` vs `webtransport/connection.rs`, plus the instance-agnostic gate) and task 26 (forwarded=0 **and** every `dropped{reason}`=0), records that deleting the attribute is task 26's DoD, and points at `docs/TODO.md`'s R-15 entry as the durable record. The attribute carries module paths rather than `file:line` anchors — a reason string printed into every lane's output should not carry numbers that go stale on the next edit to those files; the line anchors live in the test's doc comment and in the TODO entry, where a reader is already in a position to check them. Module-header item 7 rewritten (it said the test runs "live against the cluster since task 24 landed"). The doc paragraph that asserted "the `#[ignore]` this test carried until task 24 is gone" replaced with a `# Why it is still #[ignore]d` section carrying the same anchors plus the withdrawn root cause. **No assertion, fixture, or ordering-gate change** — task 25 owns the gate and the steering fixture. |
| `docs/TODO.md` | R-15 entry **refreshed and kept open**. Header re-stated (contract landed; blockers are now steering/placement and the MH receive path) with owners re-pointed to meeting-controller (25) and media-handler (26) and the contract owners marked discharged. The stale claim *"assigned to task 16 alone … and no later task in the manifest supplies the binding"* corrected in place. New `UPDATE 2026-09-08` block records what landed (`resolved` ×4 → `started` ×4; `participant_unknown` ×1 → `declined_no_sender_binding` ×1), what still blocks it with file:line anchors, and **withdraws** the `edge_count: 0` / "assignment computed before the participant exists" root cause explicitly so the next reader does not fund it. The *Obligation (open)* declined-counter paragraph marked **DISCHARGED**, naming the metric that actually landed (`mh_media_session_starts_total`, not the illustrative `mh_media_sessions_total`). Closure condition still names **four** parts and now says parts 1-2 are done and parts 3-4 are task 26's DoD, so **task 26 closes the entry**; task 25 is a dependency of that close. Everything from **The gap.** onward preserved verbatim as the pre-task-24 record. |

**Gate-3 fixes (fifth session, 2026-09-08 — MC-owned reviewer findings, authorized by @team-lead once Gate 3 opened; all on files already in the diff, so no new path drift). All doc/comment/dashboard accuracy; no behaviour, metric name, label value, or query changed.**

| File | Change | Finding |
|---|---|---|
| `crates/mc-service/src/observability/metrics.rs` | drop the restated ordinal ("a fifth value…"); attribute the compile-force to `label()`'s wildcard-free match, not `ALL`'s length | code-reviewer F1, observability F3, semantic-guard |
| `crates/mc-service/src/actors/meeting.rs` | fix the stale intra-doc link `GetParticipantSenderId`→`GetSenderIdForUser`; rewrite the `Ok(None)`/`None` prose to describe `SenderLookup::{Found, NotFound, Ambiguous}` | code-reviewer F3, F4 |
| `crates/mc-service/src/media_admission/binding_response.rs` | "two unresolved arms"→four (named); `ALL` docstring no longer claims its length compile-forces completeness (`label()` does); test renamed `all_labels_are_distinct_and_no_variant_lands_silently` with an exhaustive-match tripwire replacing the unchecked "…is_complete" claim | observability F4, test F3, dry F3 (folded into observability's ruling) |
| `crates/mc-service/tests/media_coordination_integration.rs` | "a fifth variant"→"a new variant" | observability F3 |
| `infra/grafana/dashboards/mc-overview.json` | panel 63: add `user_ambiguous` with the reconnect-is-harmful clause, "union of the two"→"of all four", drop the restated cardinality; re-emit with `ensure_ascii=False`, reverting the 12-panel `\uXXXX` churn a prior scripted run introduced (HEAD used raw UTF-8) | observability F2, F8; operations F7 |
| `docs/observability/metrics/mc-service.md` | §11 line "cardinality 4"→"`SenderBindingOutcome::ALL` cardinality" (observability's SSoT wording) | observability F3 |
| `docs/TODO.md` | R-15 blast-radius: inline SUPERSEDED clause naming `mh_media_session_starts_total{outcome}` as the detector that replaced the deleted `warn!`, and re-point the moved blockquote reference | observability F1 |
| `docs/devloop-outputs/.../main.md` | §Implementation Summary vocabulary corrected to five variants + correct compile-force attribution + `GetSenderIdForUser`; §Accepted Deferrals seeded; this table; §Lessons Learned 5 | operations F7 item 3 |

Convention adopted throughout (observability's Gate-3 ruling): name the members, drop the count, point at the compile-checked `ALL`/`label()` — so a bumped integer is not "fixed" back later.

**Routed, not mine** (see §Lessons Learned 5 and the Gate-3 disposition): all `crates/mh-service/**` findings (security SEC-1/SEC-2/SEC-4, observability F4b/F5/F6/F9, dry F3-MH-half, test F1 out-of-range arm/F2) → @paired-media-handler; all runbook/alert findings (operations F1/F3/F4/F5/F6, observability F7) → @operations; the DoD-22 multi-owner deferral entry → @operations authoring, meeting-controller+protocol contributing the ambiguous-`sub` contract-change bullet.

**Guard + test status (MC side)**

| Check | Result |
|---|---|
| `cargo test -p mc-service` | **all green** (405 lib + 21 suites) |
| `cargo clippy -p mc-service --all-targets` | clean |
| `validate-metric-coverage.sh` | MC clean |
| `validate-metric-labels.sh` | `STATUS=OK` |
| `validate-dashboard-panels.sh` | `STATUS=OK` |
| `validate-cross-boundary-classification.sh` | `STATUS=OK` |
| `release-feature-gate.test.sh` | **10 passed, 0 failed** |

**@dry-reviewer's measurement condition — discharged, and checking it undermined the condition
itself.** Warm run of `release-feature-gate.test.sh` is **7.0s for both rows**. I reported the
unflattering cold figure (**1m06s** for MC's release build in isolation) rather than the comfortable
one, and @dry-reviewer's response is the more interesting half:

1. **The budget the condition named excludes the tier the harness lives in.** They read ADR-0033 §4
   before ruling rather than eyeballing whether 1m06s "feels like a lot": ADR-0033:205 scopes the 90s
   p95 budget to the guard+audit fast tier (**Layers 3+6**) and **explicitly excludes** the language
   layers 1/2/4/5 for carrying "inherently large/variable cost". The harness is Layer 1. Their own
   residual text had already said *"which is why the harness sits in Layer 1 and not Layer 3"* — so
   **the placement WAS the cost decision, and the condition re-litigated it.**
2. **The number I measured overstates the real cost.** `GATES` runs rows in sequence with
   `mh-service` **first**, and the two dependency sets share **23 crates with only 8 unique to MC**
   (`base64`, `hex`, `prost-types`, `redis`, `ring`, `serde_json`, `sysinfo`, `tower-http`). On a
   cold cache the MH row already builds the shared graph in the release profile and the MC row
   **inherits it warm**. My 1m06s was MC's graph *in isolation*, paying for dependencies the
   sequence only pays for once.

**Recorded at the `GATES` table, not just here**, because a future reader who finds "1m06s" against
the MC row would reasonably pull it. Residual **closed** at `docs/TODO.md:108`.

---

## Devloop Verification Steps

Run by the implementer before signalling "Ready for validation". Layer 7 against the live Kind
cluster is @team-lead's (Gate 2) and is **not** included here — see DoD item 15 for why a green lane
is not by itself the R-15 proof.

| Check | Result |
|---|---|
| `cargo build --workspace` | clean |
| `cargo clippy --workspace --all-targets` | clean |
| `cargo test -p mc-service` | **all green** (405 lib + 21 suites) |
| `cargo test -p mh-service` | **all green** (239 lib + 20 suites) |
| `cargo test -p proto-gen` | **all green** (26 + 8) |
| `cargo build -p env-tests --tests --features all` | clean |
| `validate-metric-coverage.sh` | `STATUS=OK metric-coverage-all-covered` |
| `validate-application-metrics.sh` | `STATUS=OK application-metrics-clean` |
| `validate-metric-labels.sh` | `STATUS=OK` |
| `validate-dashboard-panels.sh` | `STATUS=OK` |
| `validate-cross-boundary-classification.sh` | `STATUS=OK` |
| `release-feature-gate.test.sh` | **10 passed, 0 failed** (was 5) |

**Fifth session (2026-09-08) — re-run after the two scope-alignment edits.** Only these were re-run:
the edits are one Rust attribute + comments and one markdown entry, and nothing else in the tree
moved.

| Check | Result |
|---|---|
| `cargo build -p env-tests --tests --features all` | clean |
| `./scripts/guards/simple/validate-cross-boundary-classification.sh docs/devloop-outputs/2026-09-05-sender-id-binding-contract/main.md` | `STATUS=OK REASON=cross-boundary-classification-clean-1-files` |
| `cargo fmt --all -- --check` | clean — the restored reason string is wrapped with `\` line continuations in the same shape the deleted attribute used, so the attribute reads at the file's width rather than as one 900-column line rustfmt would silently tolerate |
| `cargo test -p env-tests --features all --test 26_mh_quic forwards_an_audio` | `0 passed; 0 failed; 1 ignored` — **the attribute is registered, and libtest prints the whole reason string into the lane output**, so a reader of a skipped lane gets the tasks-25/26 diagnosis without opening the file |

**Gate-3 MC-batch re-run (fifth session, 2026-09-08).** After landing the MC-owned reviewer fixes:

| Check | Result |
|---|---|
| `cargo test -p mc-service` | all green (403 lib + suites); the renamed `all_labels_are_distinct_and_no_variant_lands_silently` and the exhaustive-match tripwire pass |
| `cargo clippy -p mc-service --all-targets` | clean |
| `cargo fmt --all -- --check` | clean |
| `cargo doc -p mc-service` (added intra-doc links) | the `GetSenderIdForUser`/`SenderLookup`/`SenderBindingOutcome` links resolve; the pre-existing broken links (`SenderId`, `OtelConfig`, proto `1`/`2`) are untouched and not a CI gate |
| `validate-cross-boundary-classification.sh` / `-scope.sh` | `OK` / `no-drift` |
| doc-citation pair, `todo-tracking`, `dashboard-panels`, `metric-labels`, `metric-coverage`, `application-metrics` | all `STATUS=OK` |
| `mc-overview.json` parses; `\uXXXX` escapes | 0 remaining (reverted to HEAD's raw-UTF-8 convention) |

**Why the ignore verification is a row and not a claim.** The three prior sessions of this devloop
each recorded an assertion the tree disproved (§Lessons Learned 3). "I added `#[ignore]`" is exactly
that shape of claim — the attribute could be on the wrong item, or shadowed — so it was run and the
runner's own output is quoted.

**The two guards that were red mid-flight are green.** `metric-coverage` and `application-metrics`
both failed on `mh_media_session_starts_total` while the MH side was in flight; both now pass. They
were left visibly red rather than papered over, per @team-lead's instruction to read them as
failures only if they survived into Gate 2.

### Positive controls — the part that makes the green mean something

Neither side's suite was believed on the strength of being green. Both were **run against
deliberately wrong implementations and observed red.**

**MC (mine), 2 mutations** — see §Lessons Learned 1 for the vacuity this found in my own arm:

| Mutation | Arms that went RED |
|---|---|
| "bind the first sender in the roster" | `distinct_participants_…`, `unknown_participant_in_known_meeting_…` |
| "bind a hardcoded ordinal 1" | `distinct_participants_…`, `resolvable_participant_gets_its_own_…` |

**MH (@paired-media-handler), 5 mutations**, reported with evidence rather than assertion:

| Mutation | Two-participant | Two-meeting | Also red |
|---|---|---|---|
| M1 — `bind()` always attributes to a fixed participant | **RED** | **RED** | collision, largest-valid |
| M2 — any valid answer replaced by hardcoded `1` | **RED** | **RED** | largest-valid |
| M3 — meeting key dropped; one global ordinal space | **RED** | **RED** | collision, largest-valid |
| M4 — fail-open: gate on `acknowledged`, treat `0` as bindable | — | — | `mc_answering_zero…` |
| M5 — `unbind` ignores `connection_id`, clears unconditionally | — | — | superseded-teardown arm |

**Two things about the MH result worth recording.** First, the MC vacuity finding **changed how the
MH fixture was built**: its ordinals are **77 and 12** — not 1, not 2, and out of connect order — so
"binds the first ordinal it saw", "binds a hardcoded 1" and "binds the other participant's ordinal"
fail three *different* assertions rather than jointly. M2 is that property under test and it is red.
Second, M1 and M3 were **re-run after a late refactor**, because *"a mutation result from before a
refactor is evidence about code that no longer exists."* That is the right instinct and it is the
reason this table is evidence rather than decoration.

### Two gaps the MH side found and closed while implementing

- **`mc_client_integration.rs`'s four retry arms asserted only `result.is_ok()`.** Sufficient when
  the RPC was an ack; **not** sufficient now. A retry loop returning a *default* response after a
  successful retry is still `Ok`, and a defaulted response is `sender_id: 0` — which MH reads as "MC
  has no answer" and turns into a declined session. It would have presented as *"media never starts
  whenever MC blips"*, with the RPC success counter reading healthy.
- **Three pre-existing tests broke on the mock's answer-nobody default and were fixed by stating the
  ordinal, not by weakening the default** — the lazy repair @test warned about at Gate 1, which would
  have disarmed every reject arm at once. That those three broke is itself evidence the await is
  genuinely load-bearing.


### Gate 2 — Attempt 1 (Lead-run, 2026-09-08): `TOTAL_RESULT=FAIL`

`DEVLOOP_FAIL_FAST=0 ./scripts/layer-all.sh` (unattended caller, so all seven layers evaluated — no layer is `NOT-RUN`). Log: `/tmp/gate2-layer-all.log`.

```
LAYER=1 RESULT=OK   DURATION=15
LAYER=2 RESULT=OK   DURATION=3
LAYER=3 RESULT=FAIL DURATION=55
LAYER=4 RESULT=FAIL DURATION=23
LAYER=5 RESULT=FAIL DURATION=7
LAYER=6 RESULT=N/A  DURATION=3
LAYER=7 RESULT=FAIL DURATION=576
TOTAL_DURATION=682 TOTAL_RESULT=FAIL
```

**Attempts consumed**: 1 of 3 (layers 1-6), 1 of 2 (Layer 7). Layer 4's classification is pending a reproduce-on-retry run (below).

#### The root cause, found in one Prometheus query — and the observability half of this task validating itself

Queried against the live Kind cluster immediately after the run:

```
mc_media_sender_binding_responses_total{outcome="participant_unknown", key_custody="operator"}  5
mh_media_session_starts_total{outcome="declined_no_sender_binding", key_custody="operator"}     2 + 3
mh_media_frames_forwarded_total{...}                                                            0
mh_media_frames_dropped_total{... every reason ...}                                             0
```

**The contract is wired end-to-end and fails closed exactly as designed; MC simply cannot resolve the participant MH names.** MH asks about `claims.sub` — the validated meeting token's subject, which is correct per @security's S1 and is now a contract MUST — and MC's roster answers `participant_unknown` on every attempt. That is an **identity-translation defect between MC's roster key and the token `sub`**, at precisely the seam @protocol's Gate-3 ruling flagged as needing care under "identity-translation locality".

**Record this as the strongest single result of the devloop**: *before* this commit, this exact failure was undiagnosable. MH would have been healthy, readiness green, every `mh_media_*` series present and flat at zero — byte-identical to a healthy idle handler — with a per-connection `warn!` as the only detector. That is verbatim the blast-radius description in the R-15 TODO entry. With the two new counters it took one query and about thirty seconds. The entry's blindness thesis was validated on the commit that closes it, which is a stronger result than the counter merely existing, and it is the answer to @operations' OPS-4 argument that a metric only a dashboard reader sees relocates the blindness rather than fixing it.

#### Layer 3 — `validate-cross-boundary-scope`, 5 violations (implementer lane)

The Layer-A scope-drift guard working as designed. 40 of 41 guards passed.

```
[scope_drift_inbound]           crates/mc-service/src/media_admission/binding_response.rs
[scope_drift_inbound]           crates/mc-service/src/media_admission/mod.rs
[scope_drift_inbound]           crates/mc-service/tests/otel_grpc_inbound_continuity.rs
[scope_drift_planned_untouched] docs/runbooks/mc-deployment.md — second rollback carve-out
[scope_drift_planned_untouched] docs/runbooks/mc-incident-response.md — cross-pointer only
```

Three touched-but-unlisted files, and two listed-but-unwritten rows. The latter two are @operations' queued OPS-1/OPS-4 deliverables — **surfacing a process fault the guard caught before a human did**: @operations was holding their own owned files behind "Start Review", which gates reading other specialists' code, not writing files they own. Lead corrected the misread and unblocked them.

#### Layer 5 — clippy, one genuine defect (media-handler tree)

`crates/mh-service/src/webtransport/connection.rs:736`, `clippy::match_same_arms`: `DeclinedMcUnavailable | DeclinedMcEndpointUnknown => false` and `Started => false` have identical bodies. **Clippy's suggested merge is the wrong fix** — folding `Started` into the decline arm destroys the distinction the adjacent comment is drawing ("Not a decline; never reaches the close path"). Correct resolutions are a deliberate merge or a scoped `#[expect(clippy::match_same_arms, reason = "…")]` per ADR-0002's `#[expect]`-over-`#[allow]` rule. It must be a decision, not a merge-to-silence-the-lint.

#### Layer 4 — two rustc ICEs and a linker abort; classification PENDING

```
thread 'coordinator' panicked at rustc_codegen_ssa/src/back/write.rs:1929  (unwrap_failed in spawn_work)
linking with `cc` failed → terminate called after throwing an instance of 'std::system_error'
  from libLLVM.so.22.1 → collect2: fatal error: ld terminated with signal 6 [Aborted], core dumped
```

The linker abort is on `gc-service (test "auth_tests")` — **a crate this diff does not touch**. Shape is resource exhaustion during parallel codegen/linking, not a compile error in the changeset; disk is not the constraint (365 G free on `/work`). Per `.claude/skills/devloop/SKILL.md` §Step 6, **reproduce-on-retry is the discriminator and the default is not "operator lane"**: a re-run of `scripts/layer4.sh` in isolation decides it. Reproducing ⇒ diff-caused, implementer lane, consumes the attempt. Not reproducing ⇒ operator lane, no attempt consumed. Explicitly NOT to be "fixed" by reducing test parallelism, which would mask it.

**CLASSIFIED — operator lane, attempt NOT consumed.** `./scripts/layer4.sh` re-run in isolation on a quiet machine: `STATUS=OK REASON=cargo-test-passed`, `STATUS=OK REASON=nx-test-passed`, exit 0, every suite green. The crates that ICE'd had failed to produce artifacts in the first run, so the retry genuinely re-exercised the same codegen and link steps rather than reading a cache — the non-reproduction is real and not an artifact of a warm build. **Layers 1-6 therefore stand at attempt 1 of 3 UNUSED for this cause**; the Layer-3 and Layer-5 failures are separate, real, and do consume it. No code change is owed for the ICE and none should be made.

#### Layer 7 — 2 of 11 env-tests failed (7 passed, 2 justified ignores)

- **`test_mh_forwards_an_audio_datagram_back_to_its_sender` RAN BY NAME AND FAILED** — `MH returned no datagram within 15s` (`26_mh_quic.rs:1394`). The proof-of-execution gate is therefore *satisfied as an artifact*: the `#[ignore]` is genuinely gone and the test genuinely exercises the path. It fails for the root cause above, not for being quarantined.
- **`test_mh_accepts_valid_meeting_jwt` failed** (`26_mh_quic.rs:593`) — MH closed the session within 2.5s of a valid JWT, `ApplicationClose{code: 0}`. This is a **pre-existing test whose premise the diff changed**: "valid JWT ⇒ session held open" is no longer true by itself now that the binding is a precondition. It may pass once the identity fix lands, but @test rules on whether the assertion still describes the contract. Nobody weakens it to make it green.

**`docs/TODO.md`'s R-15 entry stays.** Its fourth closure condition — the test passes — does not hold. @implementer's decision to hold the deletion until Layer 7 is green was upheld at Gate 2 and is vindicated by this result.

### Gate 2 — Attempt 2 (Lead-run, 2026-09-08 11:26): `TOTAL_RESULT=FAIL`

`DEVLOOP_FAIL_FAST=0 ./scripts/layer-all.sh` (unattended). Verdict: `/tmp/devloop/gate2-verdict`.
Recorded on resume — the prior session was interrupted between the pipeline finishing and this
section being written.

```
LAYER=1 OK   15
LAYER=2 OK    1
LAYER=3 OK   51     <- was FAIL; the 5 scope-drift violations are FIXED
LAYER=4 FAIL 16
LAYER=5 FAIL 10
LAYER=6 N/A   2
LAYER=7 FAIL 421    <- 8 passed, 1 failed (was 2 failed)
```

**Attempts consumed after this run**: layers 1-6 at **2 of 3**; **Layer 7 at 2 of 2 — EXHAUSTED.**

**Two of attempt 1's failures are genuinely closed.**

1. **Layer 3 is green.** The three touched-but-unlisted rows and two listed-but-unwritten runbook
   rows were all resolved; @operations landed `mc-deployment.md` and `mc-incident-response.md`.
2. **`test_mh_accepts_valid_meeting_jwt` passes.** It was collateral from the identity defect, not a
   test whose premise the diff invalidated. Nobody weakened the assertion; the fix made it true.

#### THE IDENTITY-TRANSLATION DEFECT IS FIXED — verified on the live cluster, not asserted

Attempt 1's root cause was MC answering `participant_unknown` on every attempt. Post-fix
(`get_sender_id_for_user`, resolving on the token `sub` with a three-way
`SenderLookup::{Found,NotFound,Ambiguous}` and the new `UserAmbiguous` outcome — MC refuses to guess
when two participants share a `sub` rather than picking one):

```
mc_media_sender_binding_responses_total{outcome="resolved"}             4
mc_media_sender_binding_responses_total{outcome="participant_unknown"}  1
mh_media_session_starts_total{outcome="started"}                        4
mh_media_session_starts_total{outcome="declined_no_sender_binding"}     1
```

**The binding contract this devloop exists to land works end-to-end in production shape.** MC
resolves, MH binds, the media session starts. That is DoD items 1, 2 and the substance of the task.

#### Layer 5 — the clippy defect from attempt 1 is now FIXED (on resume)

`clippy::match_same_arms` at `crates/mh-service/src/webtransport/connection.rs`
(`mc_learned_of_this_connection`) survived attempt 1 unaddressed. Routed to @paired-media-handler on
resume with attempt 1's ruling restated as binding: a deliberate merge or a scoped `#[expect]`, never
a merge-to-silence-the-lint, and never a `_ =>` arm.

**Resolved as a scoped `#[expect(clippy::match_same_arms, reason = ...)]` on the `Started` arm** —
the ADR-0002 `#[expect]`-over-`#[allow]` form, attached to the one arm rather than the function.
The decision, in @paired-media-handler's words: `Started`'s `false` *answers a different question*
than the decline arms' `false` — the declines mean "MC never learned of this connection", `Started`
means "not a decline at all, and never reaches the close path" — so collapsing them would erase a
real split. **The wildcard-free property is preserved** (no `_ =>` introduced), so a seventh outcome
still has to be classified here deliberately.

Verified by the Lead independently of the teammate's report:
`cargo clippy --workspace --all-targets` exits **0 with no errors**; `cargo test -p mh-service`
368 passed / 0 failed. **This is a WORKING-TREE fix, uncommitted** — see the escalation note below.

#### Layer 4 — a STALE artifact, not a live failure

`crates/mc-service/tests/media_coordination_integration.rs:530` referenced
`SenderBindingOutcome::UserAmbiguous` before the variant existed. **The two files were edited
inside the window Layer 4 was compiling** (both mtime 11:18, layer-4.log mtime 11:18): the test was
written against the new variant while the enum edit was still landing. `binding_response.rs:91` now
declares `UserAmbiguous` and `ALL: [Self; 5]` covers it; `cargo test -p mc-service --no-run` links
every suite clean on the current tree. **No code change is owed for this** — but the layer is
UNVERIFIED on the current tree, and an unverified layer is not a green one.

#### Layer 7 — R-15 STILL FAILS, and the cause is NOT the binding contract

`test_mh_forwards_an_audio_datagram_back_to_its_sender` ran by name and failed:
`MH returned no datagram within 15s` (`26_mh_quic.rs:1394`). Suite: 8 passed, 1 failed, 2 justified
ignores. The proof-of-execution gate is satisfied as an artifact — the `#[ignore]` is genuinely gone
and the test genuinely exercises the path.

**Root cause, from MH's own logs on the live cluster:**

```
"Forwarding policy applied", previous_generation: 0, applied_generation: 1, edge_count: 0
"Media forward path started", connection_id: 9c3c4f39-…    (+28 ms)
```

**`edge_count: 0` on EVERY policy push, and no generation-2 push ever follows.** MC computes and
pushes the forwarding policy at `RegisterMeeting` time — before the participant's media connection
exists — and never recomputes it once the connection is established. `compute_assignment`
(`crates/mc-service/src/media_routing/assignment.rs:327`) is correct for N=1 loopback; it is being
called with an input that has no connected participant yet, so it correctly produces zero edges.
MH then holds a valid binding, a started media session, and **an empty edge set**, so there is
nothing to forward to.

Corroborated by the counters — `mh_media_frames_forwarded_total` is 0 on **both** directions and
**every one of the 23 `mh_media_frames_dropped_total{reason}` series is 0**, including `no_policy`
and `no_subscriber`. The env-test's own failure message names this exact signature: *"a flat drop
series with a flat forwarded series means the forward path never started."*

**This is the SAME class of defect as the one this devloop was chartered to fix, one level up.**
§Planning 0 named the mechanism: *a fact that only becomes addressable when a specific connection
arrives cannot ride the meeting-scoped push, because that push already happened before the
connection existed.* That reasoning was applied to `sender_id` and the notification round-trip was
built to carry it. **The forwarding-policy edge set has exactly the same lifetime problem and was
not in this task's scope.** Closing R-15 requires MC to re-push the assignment after the media
connection registers — a change to MC's policy-push lifecycle and generation sequencing, with its
own reviewer panel, tests and mutation controls. It is task-sized work, not a fix to land here.

**`docs/TODO.md`'s R-15 entry therefore STAYS.** Its fourth closure condition — the test passes —
does not hold. Holding the deletion until Layer 7 is green was the right call and is vindicated
twice over.


---

## Code Review Results

**Gate 3 opened for the first time on 2026-09-08 (fifth session), after Gate 2 passed all seven
layers.** No reviewer verdict had ever been collected for this diff in any prior session — the four
earlier sessions all ended before Gate 2 went green, so "Start Review" was never issued. The roster
was respawned per SKILL.md §Recovery, and each reviewer's *unclosed Gate-1 item* was carried into
their Gate-3 prompt so it is answered against the real diff rather than lost with the session that
raised it.

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-DEFERRED | 4 | 4 | 0 (+1 spin-out) | S1+S2 closed at the code. SEC-1/SEC-2 were real behaviour bugs. S6 spun out — the sole reason this is not RESOLVED-FIXED |
| Test | RESOLVED-DEFERRED | 4 | 3 | 1 | DoD 6-11 verified. F2 (55s wall-clock) deferred with a §Test Debt entry they checked before accepting |
| Observability | RESOLVED-FIXED | 9 | 9 | 0 | Withdrew an earlier ESCALATED on learning the cause was a scope hold. Three rulings settled — closes §8e item 1 |
| Code Quality | RESOLVED-FIXED | 4 | 4 | 0 | ADR-0002 clean, zero new `#[allow]`. Verified the `SenderBindingOutcome` exhaustiveness property holds |
| DRY | RESOLVED-FIXED | 3 | 3 | 0 | Confirmed the `validate_16bit_id()` non-collapse boundary against the tree, adding a fourth independent ground |
| Operations | RESOLVED-FIXED | 7 | 7 | 0 | Found two alerts that could not fire and a deploy gate that passes a total media blackout |
| Semantic Guard | CLEAR | 0 | — | — | All five checks. Re-ran the S8 `.close(` grep against the current tree rather than trusting the record |
| Protocol (paired, §6.4) | RESOLVED-FIXED | 1 | 1 | 0 | **By-hand breaking read performed** — `buf breaking` is suppressed for `internal.proto`, so Gate 2 is not evidence. Trailer supplied |
| Media Handler (paired) | RESOLVED-DEFERRED | 1 | 1 | 1 | Owns the MH fix lane. Confirms the uncoordinated `26_mh_quic.rs` edit as owner. F2 is the deferral |
| Auth Controller (§6.4 co-sign) | CLEAR | 0 | — | — | S1's mitigations verified as trailer conditions; co-sign done by reading the auth path, not string-absence. Trailer supplied |

**Gate 3 passes.** Two reviewers land on RESOLVED-DEFERRED, so per the protocol the devloop's overall
disposition is **RESOLVED-DEFERRED**, not RESOLVED-FIXED — see §Accepted Deferrals. Nothing a
reviewer raised remains unaddressed in the diff.

**Convergence worth recording as evidence rather than as praise.** Three independent finds of the
`validate_16bit_id()` misstatement (@paired-protocol from the proto side, @dry-reviewer from the
boundary side, @security from the fail-open side), and two independent finds of the
`LocalSubscribers::unregister` gap (@security and @paired-media-handler, by different reachability
routes). Both were introduced or left by prior sessions and neither was caught by any guard.

**Three reviewers found the same defect class in their own instruments during this Gate 3** —
@observability's checker reported a correct fix as OPEN four times, @dry-reviewer armed a malformed
grep, and @security twice asserted something unmeasured (an elapsed time read off a turn count, and
"the tree builds" from a `--lib` run on a package whose integration targets compile separately). All
three self-reported and corrected. That is the same assertion-vacuity shape the diff's own catalogs
exist to prevent, appearing in the tools used to check them.


<!-- `in-review` rather than `pending`: docs/TODO.md records that dt-guard's devloop-completeness
     check matches the bare word `pending` file-wide rather than scoping to the Loop State table,
     and names `in-review` as the standing constraint while that is open. -->


---

## Accepted Deferrals

- `docs/TODO.md` §Media Path Obligations — MC↔MH gRPC has no cryptographic peer authentication (@security S6); network isolation + MC-attested endpoint today, mTLS deferred; explicitly not this devloop's to fix.
- `docs/TODO.md` §Observability Debt — no in-tree Prometheus loads the `*-alerts.yaml` rules or points at an Alertmanager (@operations); the three sender-binding alerts landed as correct artifacts, pipeline wiring deferred.
- `docs/TODO.md` §Media Path Obligations — retryable-vs-terminal client close code, the ~48s binding deadline, and a slow-binding latency instrument, three faces of one newly load-bearing dependency (@operations).
- `docs/TODO.md` §Media Path Obligations — `user_ambiguous` has no operator remedy; the durable fix is a contract change so one authenticated `sub` resolves to a single sender (meeting-controller + protocol drafters, auth-controller + security co-sign under ADR-0024 §6.4; drafted this session, landed by @operations).
- `docs/TODO.md` §Media Path Obligations — Scenario 15's version-skew "series absent ⇒ old MC image" row must be deleted once no pre-contract mc-service image can be deployed (@operations; documentation-deletion trigger, distinct from the contract changes above).

- `docs/TODO.md` §Test Debt — the 55s wall-clock cost of `an_unreachable_mc_declines_on_the_reachability_outcome` (@test F2, deferred by @paired-media-handler, accepted by @test after reading the entry).
- `docs/TODO.md` §From DRY Reviewer (Ongoing) — `MeetingControllerActorHandle::new(..)` hand-built at nine test sites (@dry-reviewer; ADR-0019 extraction opportunity, explicitly **not** a deferral).
- `docs/TODO.md` §Observability Debt — the media path's first availability SLI has no SLO over it (@observability, filed on their own account; **not** a finding).

**Closed by @team-lead at Gate 3.** Two reviewers (@security, @test) landed on RESOLVED-DEFERRED, on S6 and F2 respectively; the rest is reviewer-filed forward work, not findings left in the diff. The devloop's overall disposition is therefore **RESOLVED-DEFERRED**.

**One entry outranks the others, and its body is in `docs/TODO.md` rather than here**: the Prometheus-wiring entry means the three new alert rules are correct artifacts that nothing in this tree evaluates. @operations found it while discharging their own Gate-1 ruling that the alerts must land rather than be filed, and reported that ruling as therefore only half satisfied — the rules are right, and nothing reads them.



---

## Rollback Procedure

**Corrected at Gate 1 by @operations (OPS-6). The original step 5 was factually wrong** — it said
"Proto changes require regenerating `proto-gen/` after the reset", sending a responder hunting for a
command that does not exist. `crates/proto-gen` has **no in-tree generated files**: `build.rs:15`
emits to `OUT_DIR`, `lib.rs` `include!`s it, and `build.rs:60-61` declares `cargo:rerun-if-changed`
on both `.proto` files.

1. Verify start commit from Loop Metadata: `f0e5ee678f778455cc670662eff47038a30863fd`
2. Review all changes: `git diff f0e5ee67..HEAD`
3. Soft reset (preserves changes): `git reset --soft f0e5ee67`
4. Hard reset (clean revert): `git reset --hard f0e5ee67`
5. **Proto regeneration is automatic** — no manual regen step exists.
6. **The reset does NOT roll back the running cluster.** Both `mc-service` and `mh-service` images
   must be rebuilt and redeployed: `dev-cluster rebuild-all`, or `kubectl rollout undo` on all
   **four** Deployments — `mc-0`, `mc-1`, `mh-0`, `mh-1`. **There is no Deployment named
   `mc-service` or `mh-service`** (`mh-deployment.md:279-282, 335-337`).
7. **Order: MH first, then MC** (OPS-1). **Rolling MC back while MH is new is a total media
   blackout** — old MC sets no field 2, proto3 decodes `0`, and MH declines *every* media session.

---

## Issues Encountered & Resolutions

The substantive ones are written up where they belong rather than duplicated here: §Escalation and the three §Resume sections carry the interruption/escalation history and the two withdrawn root causes; §Gate 3 — Lead rulings carries the scope-hold and the three fix lanes; §Lessons Learned carries the recurring defect shape. Two that have no other home:

1. **A wrong root cause survived two sessions and would have funded the wrong task.** The second session concluded from one pod's logs that MC computes the forwarding assignment before the participant's media connection exists. The third session re-derived it instead of ratifying it and found both halves false — `build_routing_input` chains the joiner on, N=1 yields a reflexive self-edge with a passing unit test, and `edge_count: 0` on mh-1 is correct-by-design. The fourth session re-derived it again rather than trusting the third. Story tasks 25 and 26 were scoped from the corrected diagnosis; scoped from the original, task 25 would have funded a fix to a mechanism that works.

2. **Four sessions ended without a reviewer verdict ever being collected.** Gate 2 never went green, so "Start Review" was never issued and Gate 3 never opened — §Code Review Results sat at `TBD` for four sessions, which is *unanswered*, not empty. When it finally opened, the panel found two real behaviour bugs that no guard catches, two alert rules that could not fire, and a rollback gate that inverts under its own trigger condition. None of that would have been found by a fifth pipeline run.

---

## Lessons Learned

### 1. Deriving an expectation from the artifact is NECESSARY BUT NOT SUFFICIENT when the derived value coincides with the degenerate one

**A proposed extension to the review protocol's §Assertion Vacuity, mechanism 4
("expectation written from memory"), found by running a mutation that reasoning had already
cleared.**

@security's Gate-1 rule was explicit and correct: *"Do not write expected ids as literals `1` and
`2` that happen to match allocation order. Derive each expectation from the allocator's actual
output for that participant."* The MC arm followed it exactly —
`resolvable_participant_gets_its_own_allocated_sender_id` captured `result.sender_id` from the live
join and asserted the handler returned **that** value. By the rule, it was sound. Reviewing the test
against the rule would have passed it. **I would have shipped it.**

Then the mutation ran. Replacing the resolver body with a hardcoded ordinal `1` — the classic
"binds a hardcoded 1" defect — and **the arm still passed.**

**The mechanism.** The allocator issues `1` to the first joiner. The arm seeded exactly one
participant, so the correctly-derived expectation *was* `1`, which is *also* the degenerate constant
the mutation returns. Derivation removed the dependency on a hand-typed literal but **not** the
coincidence with it. The assertion had exactly one bit of discriminating power and the mutation sat
on the one input where that bit is zero.

**Why this is a protocol extension and not just a bug.** Mechanism 4 is about *where the expected
value came from*. This case shows the provenance can be impeccable and the assertion still vacuous,
because vacuity is a property of the **value's position in the space**, not of its derivation. The
generalised rule:

> When an expectation is derived from an artifact whose natural first output is also the value a
> degenerate implementation would return, **the derivation proves nothing.** Move the case off the
> degenerate value — do not merely re-source it.

**The remedy is one line**: join a decoy participant first so the id under test is `> 1`, and assert
that. Re-ran both mutations against the hardened arm; each is now caught by two arms.

**The transferable point is not "mutation testing is good."** It is that *this specific test was
reviewed against a rule written by a security specialist for exactly this hazard, satisfied that
rule, and was still vacuous.* Reasoning about the mutation would have concluded the test was sound.
Only running it did not.

**@dry-reviewer's test for whether this is genuinely a separate mechanism, which it passes: the
cures differ.** Mechanism 4 is about **provenance** (the expectation was written from memory rather
than read from the artifact) and its cure is *re-source the expectation*. This one is about
**position** (provenance impeccable, but the derived value sits on the degenerate point) and its
cure is *move the case off the degenerate value*. **Re-sourcing does not fix it** — which is exactly
what the mutation demonstrated, since the expectation was already correctly sourced.

**PROMOTION RAISED, not filed here — and the two co-owners of the protocol disagree on FORM, which
is theirs to settle.** @dry-reviewer declined it for their own INDEX (scoped to duplication
boundaries; a vacuity mechanism there is somewhere nobody would look) and pushed it to
`.claude/skills/devloop/review-protocol.md` §Assertion Vacuity, the list every reviewer reads at
Step 0. @test accepted it as a distinct **sixth mechanism**. @operations declined a sixth slot and
proposed instead **sharpening mechanism 4** — and their diagnosis is sharper than the one I brought:

> Mechanism 4's remedy already reads *"derive it from the artifact **and run it against a real
> adverse input before believing it**."* Two clauses. The adverse-input run is exactly what caught
> the defect. **So the protocol as written worked; what failed was @security's S2 paraphrase**,
> which kept the derivation clause and silently dropped the adverse-input one. A sixth mechanism
> does not defend against clause-dropping — it creates a sixth thing to paraphrase.

That is the same shape as the finding itself: a rule failing not because it was wrong but because a
restatement dropped the load-bearing half. @operations also asked that the run be stated in its
**falsification form** — *run the mutation and watch it go red*, the observable being the control
firing, never the absence of a complaint (the OPS-7 "show the test executing by name" shape) — and
offered an optional one-line framing sentence at the section head for the broader class that unifies
their own OPS-7 / OPS-3b / OPS-4 findings: **a control that cannot report on the thing it would be
consulted about, and reports clean instead** (which also reaches a production instrument recorded
before the interval it would measure, and an alert routed off the signal it exists to catch — neither
of which a §Assertion-Vacuity *test* item could carry).

**Not my call to pick between them, and not this devloop's diff either way** (co-owned governance
doc, unrelated to the wire contract, and @team-lead ruled the R-15 delete is the only `docs/TODO.md`
edit in the Gate-3 commit). Both owners agree the *finding* is real and must not be dropped on
"one instance" grounds; the form lands as its own test-owned change with an operations co-sign,
tracked at `docs/TODO.md` §Process / Review-Protocol. This section is the provenance record either
way — which is, per @dry-reviewer, exactly what a devloop output is for.

**Both of us hit the same shape from opposite directions this loop**: a rule written against an
*instinct*, meeting a case the instinct did not produce. @security's rule anticipated a hand-typed
literal and defended perfectly against it; the defect arrived as a coincidence instead. @dry-reviewer
and @paired-protocol hit the mirror image — INDEX entries written against a *coding instinct*
("a reader sees two adjacent 16-bit bounds and reaches for a shared helper"), meeting an
*instruction from an authoritative document* that named the helper outright.

#### The same lesson arrived from three directions, which is why it belongs here rather than in one arm's comment

@team-lead's observation at Gate 2. The finding above is one of **three** independent arrivals at the
same discipline in this loop, and the other two are @paired-media-handler's:

1. **Position, not provenance** (MC, above): a correctly-derived expectation was still vacuous
   because the derived value sat on the degenerate point.
2. **Fixture values chosen so defects fail SEPARATELY.** The MH fixture uses ordinals **77 and 12** —
   not 1, not 2, and **out of connect order** — deliberately, so that "binds the first ordinal it
   saw", "binds a hardcoded 1" and "binds the other participant's ordinal" each fail a *different*
   assertion rather than all failing one shared one. That is the MC finding's remedy generalised
   before being needed: it does not merely move the case off *one* degenerate value, it moves it off
   *every* degenerate value the defect space contains. @paired-media-handler adopted it from the MC
   result rather than arriving at it by luck, and said so.
3. **A mutation result is evidence about the code it was run against, and a refactor invalidates
   it.** M1 and M3 were run, then a late refactor landed, then **M1 and M3 were re-run** — on the
   reasoning that *"a mutation result from before a refactor is evidence about code that no longer
   exists."* Nothing would have flagged the stale table; it would have read exactly as convincing.

**The unifying discipline**: a positive control is evidence only about the exact artifact it was run
against, at the exact point in the value space it was run at. Each of the three is a different way
that scope silently shrinks to nothing — degenerate value, overlapping defect signatures, stale
code — while the table still reads green-and-verified. All three were caught by *doing* the run and
thinking about what it did not cover, never by reasoning about whether the tests looked sound.

### 1b. The identity-translation defect — and why my own tests could not have caught it

**Gate 2 attempt 1 was `TOTAL_RESULT=FAIL`, and the root cause was mine.** MC minted a fresh `Uuid`
per join as its `participant_id` (`webtransport/connection.rs:485`) and stored the token `sub` as
`user_id`; MH names the connecting party by `claims.sub`
(`mh-service/src/webtransport/connection.rs:276`) — a contract MUST, because that provenance is the
whole defence against a client asserting another identity (@security S1). **Two disjoint namespaces.**
MC's lookup was keyed on `participant_id`, so it resolved *nothing, ever*: five media connections,
five `participant_unknown`, zero frames forwarded.

**The part worth recording is why ten green component tests did not catch it.** My fixture supplied
**both sides of the identity comparison**: it passed MC's `participant_id` into the request *and*
MC looked up by `participant_id`. One namespace on both sides of an `==`. The comparison was
trivially true for a reason that had nothing to do with production, where the two sides come from
different producers.

> **A test whose fixture supplies both sides of an identity comparison cannot detect that the two
> sides are different namespaces.** It is not weak coverage of the translation — it is *zero*
> coverage, presented as ten passing arms.

This is the same family as §Lessons Learned 1 (an assertion green for a reason unrelated to the
property) but a distinct mechanism: there the expected *value* coincided with the degenerate one;
here the two *identifiers* coincided because one fixture minted both. **Neither the mutation testing
nor the five reviewers found it** — mutations perturbed the *lookup*, and every mutation was
evaluated against the same single-namespace fixture, so they faithfully reported red on the wrong
axis. **Only the live cluster had two real producers.**

**The remedy landed as a test that asserts the namespaces are NOT interchangeable**
(`mcs_own_participant_id_does_not_resolve_only_the_token_sub_does`): the token `sub` must resolve
**and MC's own `participant_id` must return `0`**. The negative half is the load-bearing half — if
MC's internal id ever starts resolving, MC is accepting an identifier whose provenance is not the
validated token, which is the S1 hazard rather than a convenience. Verified by mutation: reverting
to the original keying turns **five** arms red, including this one.

**A second defect fell out of fixing it.** Resolving by `user_id` is ambiguous when one user holds
two participants — MC mints a fresh `participant_id` per join and does not bar the same user joining
twice. MC answers `0` under a new `user_ambiguous` outcome rather than picking: either candidate
would bind MH's connection to an ordinal possibly belonging to the user's *other* participant, and
MH would stamp that participant's `sender_id` onto these frames — **right half the time, and
undetectable when wrong.** The ambiguity is inherent in the contract as ruled (the `sub` is the only
identifier MH holds), so it is made *visible and fail-closed* rather than papered over.

**And the observability half of this task proved itself on its own closing commit.** Before this
devloop the exact production failure was invisible: MH healthy, every `mh_media_*` series flat at
zero, **byte-identical to a healthy idle handler**, with a per-connection `warn!` as the only
detector. @team-lead diagnosed it from the live cluster in **one Prometheus query and about thirty
seconds** — `mc_media_sender_binding_responses_total{outcome="participant_unknown"} 5` against
`mh_media_session_starts_total{outcome="declined_no_sender_binding"} 5`. That is the R-15 entry's
blindness thesis validated by the commit that closes it, which is a stronger result than the
counter merely existing.

### 1b-i. The live failure validated the COUNTER, not the alert — do not tune a threshold from it

@operations' correction, accepted by @team-lead, recorded so nobody later cites the Gate-2 diagnosis
as evidence about alerting. `MHMediaSessionDeclineRate` is `for: 10m` and the failing env-test lives
**seconds**, so the alert **correctly did not fire**. What the incident proves is that the *series*
made an invisible failure diagnosable in one query; it proves nothing about whether the threshold or
the duration are right, in either direction. A future reader who reads "the metrics caught it" as
"the alerting caught it" would tune `for:` down from a false precedent.

### 1c. "It didn't reproduce" is only evidence if the retry re-ran the thing that failed

@team-lead's rule, from classifying Layer 4's two rustc ICEs and a linker abort. The retry came back
`STATUS=OK`, exit 0, every suite green — and a green retry over a **warm cache proves nothing about
a codegen fault**, because the failing step never re-ran. It would read identically on the page to a
genuine non-reproduction.

What made it real evidence: the crates that ICE'd **failed to produce artifacts**, so cargo had to
redo the same codegen and link steps rather than reading them from cache. The failing path was
genuinely re-exercised. Corroborated by the linker abort landing on `gc-service (test "auth_tests")`,
a crate this diff does not touch, and 365 G free on `/work` — machine-level resource exhaustion
during parallel codegen, the §6.3 operator lane.

**Same discipline as @paired-media-handler re-running M1/M3 after each refactor, and the same failure
mode as a stale mutation table**: a result is evidence only about the artifact and the code path it
actually exercised. Check that it exercised them before believing it.

### 1d. A guard whose two failure states are indistinguishable sends you to fix the wrong thing

My five classification rows for @operations' files were written as `` `path` — description (OPS-N) ``
and were **unparseable** by `validate-cross-boundary-scope`:
`crates/dt-guard/src/common/markdown_table.rs::canonicalize_path_cell` strips backticks and *exactly
one trailing parenthetical*, then compares by **exact string equality** — so the cell normalised to
`docs/runbooks/mh-deployment.md — MC-before-MH forward order` and could never match a diff entry.
No amount of writing the files would have cleared it.

**The guard was right by accident.** Those five files were *also* genuinely unwritten, so its verdict
was correct while its reasoning was not. That is the dangerous shape: a reader who writes the files
and sees the violations persist **has no way to distinguish "still unwritten" from "row
unparseable"** — the message is byte-identical. The same class as this devloop's other findings: a
control reporting a true thing for a false reason, with no signal separating the two.

Fixed by @operations moving each annotation inside the parenthetical (mechanical, descriptions
preserved verbatim); filed as a guard-machinery defect owned by **infrastructure**, with a proposed
distinct `scope_drift_unparseable_row` rule id so the two states stop being indistinguishable.
**Interim constraint for plan authors: trailing parenthetical only, never an em dash.** The
requirement is documented nowhere and the template invites the annotation, so this was not an
authoring error — which is exactly why it needed a guard fix rather than a note.

### 2. A guard's own fixture carried the "input present but malformed" vacuity it exists to catch

@security's S7 asked for one table row: add `mc-service|test-seams` to
`scripts/release-feature-gate.test.sh`'s `GATES`. They offered a deferral if an obstacle appeared.
One did.

The harness pins **one fixed `NEEDLE_PHRASE`** — `must never be enabled in a release build` — and
greps for it line-by-line against the crate's `compile_error!` literal. The file's own comment block
explains at length that a **single-line** grep is the whole mechanism, because Rust's `\`-newline
escape strips the newline *and* the next line's leading indentation, so any reconstruction of a
split literal produces a needle carrying a stray backslash and a run of spaces that **can never
match** — and would be **long**, so a minimum-length floor waves it through.

MC's literal split the needle across exactly that continuation:

```
"the `test-seams` feature exposes the sender-id exhaustion bypass and must never be enabled \
 in a release build"
```

So the row alone yields `PRECONDITION_FAILURE`, not a pass. **That is the guard behaving correctly**
— it refuses to report a result it cannot substantiate, which is the same discipline the guard
exists to enforce on others.

**Fixed rather than deferred**, per CLAUDE.md's fix-don't-defer rule and @security's repricing
argument (the residual was filed when `test-seams` was an *exhaustion bypass*; this devloop makes the
same seam the sole thing between the allocator and an arbitrary `sender_id` on the send path). The
literal is reflowed by one clause so the needle lands whole on line 2. **The break position is now
load-bearing**, and a comment at the site says so — otherwise the next person reformatting for width
silently disarms the gate, and the failure mode is a `PRECONDITION_FAILURE` nobody connects to a
line wrap.

### 3. Every wrong claim this loop was corrected by reading the actual line — THREE instances, not one

The pattern showed up three times, from three people, and each time the fix was the same: open the
file the claim is about.

1. **My *"security bought with availability"* framing** and **@operations' *"MH walks toward
   `MH_MAX_CONNECTIONS`"*** concern were the **same mistake in opposite directions**: pricing the
   change against an *imagined* old behaviour. The actual old behaviour is
   `crates/mh-service/src/webtransport/connection.rs:558` — **log and proceed**. Read it and both
   collapse: no state where the old path delivered media the new one refuses (no availability
   trade), and the old path never released the useless connection at all (slot pressure *improves*).
   One overstated a cost, the other overstated a different cost, both corrected by the same three
   lines.
2. **@operations' OPS-7 CI-skip framing** — "a green Layer 7 hides a skipped env-test suite." Read
   `layer7.sh:437` and it's a distinct `SKIPPED-NO-CLUSTER` STATUS token this devloop can't even
   reach, with the exit-0 hole already closed at `:412-415`. The *real* finding was one function
   over at `:828-836` (rc-only keying can't tell an ignored test from a passing one) — sharper, and
   only visible by reading the lines rather than recalling the shape.

**Three wrong claims, three people, one remedy.** When a change is described as a trade-off or a
gap, **locate the line that implements the current behaviour before pricing what is given up or
claiming what is missing.** In every instance this loop, nobody had — and in every instance the
correction was cheaper than the original claim, because the source was shorter than the argument
about it. Worth noting the healthy part too: all three were **withdrawn in the open, written up as
withdrawn next to the correct version**, not quietly deleted — which is the only reason this lesson
has three data points instead of zero.

### 4. Governance — a second observability instance issued conflicting label rulings

Recorded by @team-lead: an observability instance outside the team answered @paired-media-handler's
metric question before the team's @observability ruled, and its answer propagated into a plan
message. @observability **self-reported and retracted rather than averaging the two**, which is the
correct handling and is why the ruling in force is unambiguous. Also recorded: @paired-protocol
asked me to **withdraw an attribution** in which I had carried their §11 constraint as a
recommendation on label *shape* — *"a protocol specialist proposing a richer label set in a planning
thread is exactly how an owner gets pre-empted by the time they arrive."* Both incidents are the
same failure mode (an owner's decision arrived at by someone else first) caught two different ways.

---

### 5. A prior-session charter is invisible to a Gate-3 reviewer — a scope hold reads as an unresponsive implementer (the first instance of this devloop's recurring shape that is about PROCESS, not code)

Recorded at @team-lead's direction. The fifth session opened with an emphatic two-file charter
("change nothing else; any edit beyond these two is Layer-3 scope drift → Gate 2 failure"), scoped
to the pre-Gate-2 alignment pass. Gate 3 then opened for the first time in this devloop and seven
reviewers returned findings across the whole diff, many on MC files the implementer authored in
prior sessions. The implementer **held all edits outside the two charter files and put the scope
question to the Lead** rather than assuming the charter was superseded. During that hold,
@observability read no-reply-plus-no-tree-change as *unreachable* and escalated on it, then withdrew
the escalation on learning the cause and asked that the record show a scope hold, not an
unresponsive implementer.

Three things this establishes, none of them the implementer's fault. (a) **The hold was correct.**
The Lead's ruling: Layer-3 scope drift compares the diff against the plan's file list, and every
file in the MC batch was already in the diff and the Cross-Boundary table — so a Gate-3 fix on an
existing file adds no path and is the opposite of the *widening* the charter forbade. The mechanics
the implementer worried about were real; they just did not apply to fixing findings on files under
review. (b) **The protocol has no channel for it.** Nothing tells a Gate-3 reviewer that an
implementer may be charter-bound from a prior session, so "silence + no tree change" was genuinely
ambiguous between "waiting on authorization" and "gone". The fix is a signal: an implementer holding
on scope should say so to the reviewers, not only to the Lead — a one-line "held pending a scope
ruling, not unreachable" collapses the ambiguity, and the implementer sent exactly that once the
pattern was visible, which is what let the escalation be withdrawn cheaply. (c) It is the **fifth
instance** of this devloop's recurring shape — a claim (here, "the implementer is unreachable")
that the actual state disproves — and the first that is about the review process rather than the
code or a guard. The prior four (§Lessons Learned 3's three source-reading corrections and 1d's
indistinguishable-guard-states) were all "read the artifact"; this one is "make the state legible
to the person reading it", which is the same lesson from the other side.

---

---

## Escalation (headless, 2026-09-08)

**Reason**: `validation-attempts-exhausted` — Layer 7 is at **2 of 2** attempts, both genuine
`STATUS=FAIL` (exit 1, test failures; neither was a `PRECONDITION_FAILURE`, so neither was free).
Per `.claude/skills/devloop/SKILL.md` §Limits, Layer 7 escalates after 2. Under `DEVLOOP_HEADLESS=1`
that is a terminal escalation, not a prompt: **no third pipeline run was made.**

**This is not merely rule-compliance — it is the substantively right call**, and the reason is the
finding itself rather than the budget.

### What this devloop set out to do, it DID

The participant → `sender_id` binding contract works end-to-end in production shape, verified on the
live Kind cluster rather than asserted: MC resolves the token `sub` against its roster
(`resolved` × 4), answers the allocated ordinal on the wire, MH validates and binds it, and the
media session starts (`started` × 4). The fail-closed arms fire correctly and are counted
(`participant_unknown` × 1 → `declined_no_sender_binding` × 1). DoD items 1 and 2 are met. The
`#[ignore]` is gone and the env-test genuinely executes.

### What blocks R-15, and why it is NOT this task

`edge_count: 0` on every forwarding-policy push, with no second generation ever following. MC
computes the assignment at `RegisterMeeting` time — **before** the participant's media connection
exists — and never recomputes it once that connection registers. `compute_assignment` is correct;
it is being handed an input with no connected participant, so it correctly yields zero edges. MH
ends up holding a valid binding, a started session, and an empty edge set.

**This is the same lifetime defect this devloop was chartered to fix, one level up.** §Planning 0
named the mechanism precisely — *a fact that only becomes addressable when a specific connection
arrives cannot ride the meeting-scoped push, because that push already happened before the
connection existed.* That analysis was applied to `sender_id`, and the notification round-trip was
built to carry it. **The forwarding-policy edge set has exactly the same problem and was never in
this task's scope.**

Fixing it means changing MC's policy-push lifecycle and generation sequencing so the assignment is
re-pushed after the media connection registers. That needs its own plan, reviewer panel, tests and
mutation controls. It is task-sized work. Landing it here would be exactly the improvisation past a
limit that headless mode forbids, and it would arrive with none of the review this repo requires.

**The decision a human owes**: widen this task to include MC's policy re-push, or spin it out as the
next story task. That is the "is this task well-scoped?" question, and it is not the Lead's to
answer alone.

### State of the tree at escalation — NOT committed

Per §Headless Mode, no partial commit was made: the gates did not pass. The working tree carries the
full implementation plus two fixes landed on resume.

| Layer | State at escalation |
|---|---|
| 1, 2, 3 | **OK** as of attempt 2 (Layer 3's 5 scope-drift violations are fixed) |
| 4 | Attempt 2's failure was a **stale mid-edit artifact**; `cargo test -p mc-service --no-run` links clean on the current tree. **Unverified by a pipeline run** — unverified is not green. |
| 5 | **Fixed on resume** and verified: `cargo clippy --workspace --all-targets` exits 0. |
| 6 | `N/A` (no dependency-manifest change) |
| 7 | **FAIL** — the `edge_count: 0` root cause above. Attempts exhausted. |

`docs/TODO.md`'s R-15 entry is **deliberately still present**. Its fourth closure condition — the
env-test passes — does not hold, and deleting it would claim a closure the cluster disproves.

**Reviewer verdicts were never collected**: Gate 2 never went green, so "Start Review" was never
issued and Gate 3 did not open. §Code Review Results and §Accepted Deferrals stay `TBD` — they are
unanswered, not empty.

---

## Resume (2026-09-08, third session): the recorded root cause is WRONG

The runner relaunched this task after the prior session was interrupted between writing
§Escalation and writing `.devloop-escalation.json`. On resume the Lead re-derived the Layer-7
root cause instead of ratifying it, and it does not hold.

**No pipeline run was made. Layer 7 remains EXHAUSTED at 2/2 and no gate was re-attempted.**
Everything below is read-only diagnosis: unit tests, source reading, `kubectl logs`, and one
Prometheus query. None of it consumes an attempt, and none of it is a fix.

### What §Escalation claimed, and why it is false

> `edge_count: 0` on EVERY policy push […] MC computes and pushes the forwarding policy at
> `RegisterMeeting` time — before the participant's media connection exists […] so it correctly
> produces zero edges.

Both halves are wrong.

1. **The joiner IS in the routing input.** `build_routing_input`
   (`crates/mc-service/src/webtransport/connection.rs:2321-2325`) chains `join_result.sender_id`
   onto the roster precisely because "the joiner is not yet on the roster the join returned".
   `JoinResult::sender_id` is a non-optional `SenderId` allocated at join
   (`crates/mc-service/src/actors/messages.rs:382`). At the first-participant push the input is
   `participants = [joiner]`, not empty.
2. **N=1 yields an edge, and there is a passing test that says so.** `subscribes_to` is reflexive
   by design (`assignment.rs:247-249`, "Reflexive in this story, and that is the whole of *hear
   yourself*"), and `cargo test -p mc-service --lib media_routing::assignment` passes
   `n1_loopback_yields_one_self_edge_with_one_audio_egress` — 12/12 green on the current tree.
3. **The cluster says the same.** Every `Forwarding policy applied` line on **mh-0** carries
   `edge_count: 1`; only **mh-1** carries `edge_count: 0`, and the two are pushed in pairs at
   identical timestamps (11:24:39.308, 11:25:01.096, 11:25:32.827, 11:25:47.219 on both pods).

```
mh-0: 11:24:23.687 edges=1   11:24:39.308 edges=1   11:25:01.096 edges=1   11:25:47.220 edges=1
mh-1: 11:24:39.308 edges=0   11:25:01.096 edges=0   11:25:32.827 edges=0   11:25:47.219 edges=0
```

`edge_count: 0` on mh-1 is **correct behaviour, not a defect**: `compute_assignment` gives *every*
handler an entry so "this handler forwards nothing" is a policy MC pushes rather than a handler MC
skips (`assignment.rs`, and the passing
`handler_with_no_participants_gets_an_empty_but_present_assignment`).

**The prior session read one pod's logs — the handler that is supposed to be empty — and
generalised "EVERY policy push" from it.** That is the same failure mode §Lessons Learned 3
records three times over, and this is the fourth instance. The recommendation it produced ("widen
this task to include MC's policy re-push lifecycle, or spin it out") would have funded a task to
fix a mechanism that is working as designed.

### What the evidence actually shows — one confirmed defect, one open question

**CONFIRMED: edge placement and handler advertisement use different orderings.**

- `edge_handler` puts each edge on the **lexicographically smallest** shared handler:
  `shared.sort(); shared.first()` (`assignment.rs:269-280`) — deterministically `mh-0` here.
- `media_servers` on the `JoinResponse` is built from `mh_data.handlers` in **unsorted Redis
  order** (`connection.rs:2252-2258`), and the env-test connects to `media_servers.first()`
  (`26_mh_quic.rs`).

Nothing couples those two orderings, so the client can be steered to a handler holding an empty
edge set for its meeting. The cluster confirms this happened: **three of the four client media
connections started on mh-1**, each ~28 ms after mh-1's own zero-edge push.

```
mh-1: 11:24:39.336 / 11:25:01.127 / 11:25:32.859  "Media forward path started"
mh-0: 11:24:23.748                                 "Media forward path started"  (one, earlier test)
```

The `assignment.rs` comment states the standing assumption — *"Today every participant is on every
handler assigned to the meeting (clients `connectAll()`)"* — but the env-test connects to the
first advertised handler only. So the assumption the routing input encodes and the behaviour the
test exercises disagree, and **which side is wrong is a design decision, not a typo**: sort
`media_servers`, make the client `connectAll()`, or advertise the edge-bearing handler.

**OPEN: that alone does not explain the symptom.** A datagram arriving at a handler with an
installed generation-1 policy and no matching edge should increment a `no_subscriber` drop. It did
not. Queried on resume, **every `mh_media_frames_dropped_total{reason}` series is 0 and
`mh_media_frames_forwarded_total` is 0 on both instances** — so the datagram is not reaching the
routing lookup at all. There is a second gap upstream of edge matching, and diagnosing it is
task-sized work that needs the media-handler specialist and a reviewer panel.

### What this changes for the human

The decision is still "is this task well-scoped?", but the input to it is different. R-15 is
blocked by a handler-selection ordering defect plus an unexplained gap in the datagram receive
path — **not** by MC's policy-push lifecycle. A spin-out scoped from §Escalation would have been
scoped wrong.

**Unchanged**: the binding contract this devloop was chartered to land works end-to-end
(`resolved` × 4, `started` × 4, fail-closed arms counted). DoD items 1 and 2 stand. Layers 1-3 and
5 were green or fixed; Layer 4's attempt-2 failure was a stale mid-edit artifact; Layer 7 is
exhausted. `docs/TODO.md`'s R-15 entry stays. No reviewer verdicts were ever collected, so
§Code Review Results and §Accepted Deferrals remain unanswered rather than empty.

---

## Gate 2 — Attempt 3, layers 1-6 only (Lead-run, 2026-09-08, fourth session): all six GREEN

The runner relaunched this task a third time. **Layer 7 was NOT re-run** — it stands EXHAUSTED at
2/2, both consumed by genuine `STATUS=FAIL` (exit 1), so a third run would be improvising past a
limit that §Headless Mode forbids. Layers 1-6 had attempt **3 of 3** unused, and main.md carried two
open questions that only a pipeline run could close — Layer 4 recorded as *"UNVERIFIED on the
current tree, and an unverified layer is not a green one"*, and Layer 5's clippy fix verified only
by a direct `cargo clippy` invocation rather than by the layer. Both are now settled.

Invoked as `./scripts/layer1.sh` … `./scripts/layer6.sh` individually — the ADR-0033 §4
"independently callable" path — precisely so that `layer-all.sh` would not dispatch the exhausted
Layer 7. Logs: `/tmp/devloop/gate2-a3-layer{1..6}.log`.

```
LAYER=1 EXIT=0   compile   buf-build, cargo-build, dt-guard, dt-story, release-feature-gate, nx-typecheck
LAYER=2 EXIT=0   format    buf-format, cargo-fmt, nx-format
LAYER=3 EXIT=0   guards    guards-passed + 13 guard self-tests
LAYER=4 EXIT=0   test      cargo-test-passed, nx-test-passed
LAYER=5 EXIT=0   lint      buf-lint, cargo-clippy-passed, nx-lint
LAYER=6 EXIT=0   audit     cargo-audit-passed, buf-breaking-passed, SKIPPED-NO-DIFF no-dep-changes
```

Two prior-session claims are now **verified rather than asserted**:

1. **Layer 4 is genuinely green.** Attempt 2's failure really was the stale mid-edit artifact
   (`SenderBindingOutcome::UserAmbiguous` referenced before the variant landed); no code was owed
   and none was made. `STATUS=OK REASON=cargo-test-passed` on the current tree.
2. **Layer 5's `#[expect(clippy::match_same_arms)]` resolution holds under the layer**, not just
   under a hand-run clippy: `STATUS=OK REASON=cargo-clippy-passed`.

Two `N/A` aggregates are the documented self-justifying kind, not gaps: Layer 4's and Layer 6's
`not-applicable-to-this-lang` are proto's registered intentional-gap placeholders (proto has no
`test.sh`/`audit.sh` phase), and Layer 6's `SKIPPED-NO-DIFF no-dep-changes` is the within-wrapper
dep-manifest gate — this diff changes no dependency manifest. Neither is a `FAIL-MISSING-VERB` and
neither is a `NOT-RUN`.

**Gate 2 still does not pass**, because Layer 7 is red and out of attempts. Six green layers do not
add up to a green pipeline. **"Start Review" is therefore still not issued and Gate 3 remains
closed** — §Code Review Results and §Accepted Deferrals stay unanswered rather than empty, and no
commit is made (§Headless Mode: do not commit partial work unless the gates passed).

### The §Resume diagnosis re-verified independently before escalating

The Lead did not ratify the third session's correction either — it was re-derived from source. Both
halves hold:

- `edge_handler` picks the **lexicographically smallest** shared handler:
  `shared.sort(); shared.first()` (`crates/mc-service/src/media_routing/assignment.rs:269-280`).
- `media_servers` is built by mapping `mh_data.handlers` in **unsorted Redis order** with no sort
  anywhere on the path (`crates/mc-service/src/webtransport/connection.rs:2251-2258`).
- The env-test takes `media_servers.first()` (`crates/env-tests/tests/26_mh_quic.rs:1351-1356`).

**A third coupling failure the prior sessions did not name, found on this pass**: the test's own
ordering gate is instance-agnostic. It calls `poll_until_any_instance_above` on
`mh_media_policy_applies_total{outcome="applied"}` (`26_mh_quic.rs:1352`) — so **mh-0 applying a
policy satisfies the gate for a client that then connects to mh-1**. The gate exists to prevent a
`no_policy` race and it does prevent that one; it cannot detect that the *wrong handler* was
selected, because it never asks which instance moved. This is why the failure presents as a silent
15s timeout rather than as a race.

`build_routing_input` chaining the joiner on (`connection.rs:2321-2325`) is confirmed as well, so
the §Escalation "MC computes the assignment before the participant exists" story remains withdrawn.


---

## Resume (2026-09-08, fifth session): the escalation was ANSWERED — scope amended out of session

The runner relaunched task #24 a fourth time. **This session is materially different from the
previous three reruns: the question §Escalation posed to the human has been answered, and the
answer is recorded in the story manifest rather than in this file.**

### The evidence that a decision was made, and by whom

`docs/user-stories/2026-08-27-hear-yourself-through-handler.md` was modified at **16:57:26 UTC** —
after the fourth session's last write to this file (**16:51:25 UTC**) and before this session began
(~16:58). The Lead did not make that edit; the fourth session's record ends at §Gate 2 Attempt 3
with no story amendment. The change is an out-of-session actor's, and it is precisely the decision
§Escalation said "is not the Lead's to answer alone":

| Added | Owner | Scope |
|---|---|---|
| **task 25** | `meeting-controller` | Couple client media steering to the assignment placement — the third-session `edge_handler` / `media_servers` ordering defect, *plus* the fourth-session finding that `poll_until_any_instance_above` is instance-agnostic. Cites both by session. |
| **task 26** | `media-handler` | Diagnose and fix the MH datagram receive-path gap (forwarded=0 **and** every dropped-by-reason series=0). **Its stated DoD is deleting the `#[ignore]` and the R-15 env-test passing.** |

Task 20's `deps` also gained `26`. The two prompts quote this file's §Resume and §Gate 2 Attempt 3
verbatim, so the amendment was made *from* the corrected diagnosis, not the withdrawn one — the
third and fourth sessions' refusal to ratify a wrong root cause is what made the spin-out scoped
right.

**The disposition is a spin-out, not a widening.** Task 24 keeps the binding contract. R-15's
end-to-end proof moves to task 26.

### What that changes in this devloop's definition of done

Two DoD items are **superseded by the amendment** — they are not being waived by the Lead, they were
reassigned by the actor who owns story scope:

- **DoD 3** (`#[ignore]` deleted **and it passes against the live cluster**) → **task 26's DoD,
  verbatim.** Task 25 states the complementary half explicitly: *"Leave the R-15 test itself
  `#[ignore]`d — un-ignoring it is task 26's definition of done."*
- **DoD 5** (`docs/TODO.md`'s R-15 entry DELETED) → task 26 closes it, "per its four-part closure
  condition". The entry stays open here, which is what the fourth session already did.

DoD items 1, 2, 4 and 6-15 are unchanged and remain this task's to meet.

**Consequence for the tree, and it is an edit this session owes rather than a note:** the working
tree currently has the `#[ignore]` **removed** (`26_mh_quic.rs:1308` — "The `#[ignore]` this test
carried until task 24 is gone"). Under the amended scope that attribute must be **restored**, with
the reason naming tasks 25 and 26 and the still-open R-15 entry. This is not quarantining a failing
gate: the test covers functionality two named, dependency-ordered tasks are chartered to deliver,
`docs/TODO.md`'s R-15 entry stays open as the durable record, and un-ignoring it is another task's
stated DoD. Leaving it un-ignored would be the actual dishonesty — it would assert that task 24
proved R-15 when the cluster says it did not.

### The Layer 7 attempt budget — restarted, and why that is not improvisation

Layer 7 stood **EXHAUSTED at 2/2**, both consumed by genuine `STATUS=FAIL` on
`test_mh_forwards_an_audio_datagram_back_to_its_sender`. **That budget is not being waived; its
subject has been removed from this task's scope.** The attempt limit exists to force a human
decision instead of letting the Lead thrash on a red gate — the escalation fired, the decision was
made, and the failing test now belongs to task 26. A budget that survived its own escalation's
resolution would make every escalation terminal forever, which is not what §Limits is for.

**Stated plainly so a reader can disagree with it:** the Lead is re-running Gate 2 in full
(`layer-all.sh`, all seven layers — this is an unattended `DEVLOOP_HEADLESS=1` caller, so it runs
all layers rather than fail-fasting). If Layer 7 is red for any reason **other** than the two
findings now owned by tasks 25 and 26, that is a task-24 failure, consumes an attempt, and routes to
the implementer.

### Phases this session must finish

1. **Scope-alignment edits** (implementer): restore the `#[ignore]` + its header entry; refresh the
   `docs/TODO.md` R-15 entry, which is now stale in a specific way — it says *"no later task in the
   manifest supplies the binding"*, and task 24 supplies it.
2. **Gate 2** — full `layer-all.sh`.
3. **Gate 3** — **never opened in any prior session.** No reviewer verdict has ever been collected
   for this diff; §Code Review Results and §Accepted Deferrals are unanswered, not empty. The roster
   is respawned per §Recovery. Gate-1 rows that never reached `confirmed` (security S1/S2,
   observability, DRY's non-collapse boundary, operations, semantic-guard, protocol, media-handler)
   are carried into the Gate-3 prompts as open items so they are answered against the real diff
   rather than lost with the sessions that raised them.
4. **Commit** — only if Gate 2 and Gate 3 pass.

---

## Gate 2 — Attempt 4 (Lead-run, 2026-09-08, fifth session): **PASS**, all seven layers

First full-pipeline run since the scope amendment. Invoked as `DEVLOOP_FAIL_FAST=0
./scripts/layer-all.sh` — the unattended/run-all path (`DEVLOOP_HEADLESS=1`), so every layer is
evaluated and nothing renders `NOT-RUN`. Log: `/tmp/devloop/gate2-s5-all.log`.

```
LAYER=1 RESULT=OK   DURATION=7     compile
LAYER=2 RESULT=OK   DURATION=2     format
LAYER=3 RESULT=OK   DURATION=48    guards + 13 guard self-tests
LAYER=4 RESULT=N/A  DURATION=238   cargo-test-passed, nx-test-passed
LAYER=5 RESULT=OK   DURATION=1     lint
LAYER=6 RESULT=N/A  DURATION=2     cargo-audit-passed, buf-breaking-passed
LAYER=7 RESULT=OK   DURATION=568   env-tests-passed, browser-e2e-passed
TOTAL_DURATION=866 TOTAL_RESULT=N/A
```

**Reading `TOTAL_RESULT=N/A` rather than pattern-matching on it.** No layer reported `FAIL`, and no
layer reported `NOT-RUN`. The two `N/A` aggregates are the documented self-justifying kind
(SKILL.md §Layer N/A justification template) and neither is a `FAIL-MISSING-VERB`: Layer 4's and
Layer 6's `not-applicable-to-this-lang` are proto's registered intentional-gap placeholders (proto
has no `test.sh`/`audit.sh` phase), and Layer 6's `SKIPPED-NO-DIFF no-dep-changes` is the
within-wrapper dep-manifest gate — this diff changes no dependency manifest. Every verb that has a
real implementation on this diff is `STATUS=OK`.

**Layer 7 ran for real and this was checked, not assumed** — Gate-1 ruling #5 exists precisely
because `layer7.sh` keys on exit code, and rc 0 cannot distinguish "ran and passed" from "was
ignored and trivially did not fail". Applying the reading rule that ruling adopted, against
`$ENV_TEST_LOG` rather than the aggregate:

- `Running tests/26_mh_quic.rs` is present and the binary reports
  `test result: ok. 8 passed; 0 failed; 3 ignored` in **121.97s** — a suite that genuinely
  transacted with the cluster, not a no-op.
- `test_mc_programs_live_handler_with_confirmed_forwarding_policy ... ok` — MC's policy push to a
  live handler is exercised and green on this diff.
- **The `0 ignored` half of the rule cannot apply this run and is replaced by an enumeration**, which
  is the stricter check: all **three** ignores in that binary were read by name.
  `test_mc_trace_continuity_end_to_end` and `test_mh_disconnects_unregistered_meeting_after_timeout`
  are pre-existing and carry their component-tier pointers.
  `test_mh_forwards_an_audio_datagram_back_to_its_sender` is the one this session restored, and its
  printed reason names tasks 25 and 26, states that the binding contract landed and is not the
  blocker, and points at the open R-15 entry. **No sibling ignore crept in**, which is the failure
  mode the `0 ignored` check was guarding against.
- `browser-e2e-passed` — the Playwright suite ran too; it is not gated behind a diff trigger.

**Attempt accounting.** Layers 1-6: attempt 4, and green. Layer 7: this is the first attempt since
the amendment moved its failing subject to task 26 — see §Resume (fifth session) for why the budget
restarted rather than being waived, and for the standing commitment that a Layer-7 failure from any
*other* cause would have consumed an attempt and routed to the implementer.

**Gate 2 passes. "Start Review" issued to the full roster; Gate 3 opens for the first time in this
devloop's history.**

---

## Gate 3 — Lead rulings (2026-09-08, fifth session)

Gate 3 opened for the first time in this devloop's history. Ten reviewers examined the diff; the
findings converged hard, which is worth recording as evidence rather than as flattery: **@security
and @paired-media-handler independently found the same `LocalSubscribers::unregister` gap by
different routes, and @dry-reviewer, @protocol and @security independently found the same
`validate_16bit_id()` misstatement.**

### 1. Scope ruling — Gate 3 authorizes fixes across the diff under review

@implementer stopped and asked before touching anything outside their two-file scope-alignment
charter. **That was correct behaviour and the answer is yes.** Layer-3 scope drift compares the diff
against the plan's file list; every file at issue is already in the diff and already in the
Cross-Boundary Classification table, so a Gate-3 fix on an existing file adds no path. What the
charter forbade was *widening* the changeset during an alignment pass — the opposite gesture.

**Recorded because a reviewer was misled by it**: @observability read the resulting silence as an
unresponsive implementer and issued `ESCALATED`, then **withdrew it on learning the cause was a
scope hold**. Their withdrawal is on the record and the fault is the protocol's, not theirs and not
the implementer's — nothing in the Gate-3 protocol tells a reviewer that an implementer may be
charter-bound from a prior session, so silence has only one available reading. See §Lessons Learned
5.

### 2. Three fix lanes, because one implementer could not own them all

The fifth-session implementer is `meeting-controller` and cannot author `crates/mh-service/**`,
runbooks or alert rules. Rather than let those findings become deferrals by default, the owning
specialists were authorized to **implement their own findings**:

| Lane | Owner | Scope |
|---|---|---|
| MC + `docs/TODO.md` + `mc-overview.json` | `implementer` | doc drift, panel 63, the `ALL` docstring overclaim and its non-checking test, `ensure_ascii=False` re-emit |
| `crates/mh-service/**` | `paired-media-handler` | **SEC-1, SEC-2** (the two real behaviour bugs), SEC-4, F4b/F5/F6, F3-MH, @test F1, F9's MH half |
| runbooks + alerts + `docs/TODO.md` DoD-22 | `operations` | F1 (unsatisfiable `for:`), F6 (deploy gate), F3/F4/F5/F7, F9's runbook half, F2 |

### 3. Disputed finding — @observability governs F1

@operations read the `docs/TODO.md:924` phantom `warn!` as already handled by the UPDATE block's
supersession clause; @observability wanted a local marker plus a re-pointed blockquote. **Ruled for
@observability**: locality beats a marker five paragraphs away on an entry that is still open, and
the second stale pointer (the blockquote that moved above `mh_media_session_starts_total` in this
diff) is uncovered on either reading.

### 4. F9 — the finding that outranks its own label question

@operations found that `close_declined_connection` returns `Err`, and `server.rs:220-229` counts any
`Err` as `mh_webtransport_connections_total{status="error"}` — so every deliberate, fail-closed,
*already-counted* decline also lands on the connection **error** series. @observability ruled the
label question and **overrode @operations' proposed `declined` status value at their invitation**
(`accepted` is incremented before `handle_connection` is spawned, so a new value repairs no ratio
and duplicates `mh_media_session_starts_total{outcome}` at lower resolution).

**The part that matters more than the label**: `mh-deployment.md:250`'s immediate-rollback gate
matches `{status!="accepted"}` at >10% for 10m, so every decline enters it — and rolling MH back
restores the pre-contract silent-no-media behaviour, so declines stop, the ratio recovers, and **the
rollback reads as successful.** A control that inverts under precisely the fault it exists to catch:
it removes the component that reports the problem and reads the resulting silence as recovery. Fifth
instance of this devloop's recurring shape, and the first one sitting in a production deploy gate.

### 5. Rulings accepted from reviewers, recorded so they are not re-litigated

- **@observability extended §8a's four outcomes to seven** and MC's to five, flagging it as a ruling
  *change* per §Lessons Learned 4 rather than applying it silently, and stating plainly that **their
  own §8a ruling was short and the implementer was right** to land `declined_mc_auth_rejected` and
  `declined_mc_endpoint_unknown`. `declined_sender_binding_conflict` — the fifth outcome left
  undecided at §8d — is **ADMITTED**. This closes §8e item 1.
- **Convention on restated counts** (@observability, adopted independently by @implementer,
  @dry-reviewer and @operations): **name the members, drop the count, point at the compile-checked
  `ALL`.** The integer at issue had already drifted four times by the catalog's own admission, and
  drifted a fifth time in this diff.
- **@dry-reviewer confirmed the `validate_16bit_id()` non-collapse boundary** against the tree rather
  than deferring to four prior confirmations, and added a ground the others had not: *a shared helper
  that cannot import the value it validates against is not a shared home for that value; it is a copy
  of it.* `internal.proto` was already citing their INDEX as that ground's home while the INDEX did
  not carry it — a dangling citation, now fixed.
- **@paired-media-handler confirmed the uncoordinated `26_mh_quic.rs` edit as owner**, having checked
  every prose claim against the tree.
- **@auth-controller and @security both cleared the `#[ignore]` restoration on the merits**, not on
  the process: the lane's unique coverage is MH-`sub` ↔ MC-`user_id` namespace agreement, which fails
  **closed**, and the one relaxation that would be a security regression has MC component-tier
  coverage over a real actor and allocator.

---

## Gate 2 — Final re-run (Lead-run, 2026-09-08, fifth session): **PASS**

Run after Gate 3's three fix lanes landed, because the tree moved substantially during review —
@security's earlier "integration targets do not compile" caveat was real when written and was fixed
by @paired-media-handler before this run. `DEVLOOP_FAIL_FAST=0 ./scripts/layer-all.sh`, the
unattended run-all path. Log: `/tmp/devloop/gate2-s5-final.log`.

```
LAYER=1 RESULT=OK   DURATION=11    LAYER=5 RESULT=OK   DURATION=9
LAYER=2 RESULT=OK   DURATION=2     LAYER=6 RESULT=N/A  DURATION=2
LAYER=3 RESULT=OK   DURATION=52    LAYER=7 RESULT=OK   DURATION=516
LAYER=4 RESULT=N/A  DURATION=276   TOTAL_DURATION=868 TOTAL_RESULT=N/A
```

**Zero `FAIL`, zero `PRECONDITION_FAILURE`, zero `UNKNOWN`, zero `NOT-RUN`** across the whole run.
The two `N/A` aggregates are proto's registered intentional-gap placeholders plus the Layer-6
dep-manifest gate, as at attempt 4 — self-justifying per SKILL.md, not gaps.
`env-tests-passed` and `browser-e2e-passed`.

**Layer 7's ignore accounting re-checked by enumeration**, per Gate-1 ruling #5 — `layer7.sh` keys on
exit code, and rc 0 cannot distinguish "ran and passed" from "was ignored". `26_mh_quic` reports
`8 passed; 0 failed; 3 ignored` in **81.51s**, and all three ignores were read by name:
`test_mc_trace_continuity_end_to_end` and `test_mh_disconnects_unregistered_meeting_after_timeout`
(pre-existing, component-tier pointers) and `test_mh_forwards_an_audio_datagram_back_to_its_sender`
(restored this session, reason naming tasks 25/26). **No sibling ignore crept in** across either
Gate-2 run. @operations reviewed this substitution and ruled it stronger than the `0 ignored` check
it replaces, since it also answers "did the binary transact at all" — while noting the residual it
does not remove: it is a human read of a log, as durable as the next session's discipline.

---

## Story-scope note

DoD items 3 and 5 were **reassigned to story task 26** by the out-of-session manifest amendment, not
waived — see §Resume (fifth session). `docs/TODO.md`'s R-15 entry stays open by design and is
refreshed rather than closed; task 26's DoD is deleting the `#[ignore]` and that env-test passing.
Per SKILL.md §Step 9, task completion is **not** recorded via `dt-story` here: `DEVLOOP_HEADLESS=1`
is set, so the runner records it after its own gates, which ADR-0035 §3 reserves to the runner.
