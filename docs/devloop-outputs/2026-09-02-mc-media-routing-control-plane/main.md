# Devloop Output: MC Media-Routing Control Plane (ADR-0036 §7/§8/§9)

**Date**: 2026-09-02
**Task**: MC computes the meeting forwarding assignment (general visibility-graph, no single-handler special case) and programs the assigned MH via RegisterMeeting: edge set + per-egress behaviours + derived policy_generation; one-shot push with applied-generation confirm and fail-loud divergence.
**Specialist**: meeting-controller
**Mode**: Agent Teams (v2) — full, HEADLESS (run-story task #13)
**Branch**: `feature/hear-yourself-through-handler`
**Duration**: ~5h wall clock across the resumed session (Gate 1 re-confirmation → implementation → 3 Gate-2 runs → Gate 3)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `511e14fdffbb4b6ddf1d8c313815915c5dbeb22b` |
| Branch | `feature/hear-yourself-through-handler` |
| Lead Model | `claude-opus-5` |
| Headless | `DEVLOOP_HEADLESS=1` (run-story task #13) |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` (respawned) |
| Implementing Specialist | `meeting-controller` |
| Iteration | `1` |
| Security | respawned |
| Test | respawned |
| Observability | respawned |
| Code Quality | respawned |
| DRY | respawned |
| Operations | respawned |
| Semantic Guard | respawned |
| Media Handler (conditional, contract co-owner) | respawned |

### Interruption & Resume (2026-09-02)

The first session was interrupted by infra (headless session limit) at the END of the planning
phase: the plan below is complete and every Gate-1 amendment (§11) was raised, answered and — for
the one contested point, O-6 — adjudicated by the Lead. **No code had been written**: at resume,
`git status` showed only this untracked devloop-output directory and `HEAD` was still the recorded
Start Commit `511e14fd`. Per the devloop skill's Recovery clause (headless infra interruption), the
roster was respawned and the loop resumed at Gate 1 rather than restarted from setup — the planning
*output* survived in this file, only the gate mechanics were unfinished.

At resume the Gate-1 classification-sanity guard was run and passed:
`./scripts/guards/simple/validate-cross-boundary-classification.sh` →
`STATUS=OK REASON=cross-boundary-classification-clean-1-files`.

Respawned reviewers were instructed to re-confirm the plan and to raise only defects **not** already
addressed in §11 — settled points (notably the O-6 label drop and the OPS-18 demotion) are not
relitigated.

---

## Gate 1 — Plan Confirmation

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Semantic Guard | confirmed |
| Media Handler (conditional) | confirmed |

---

## Task Overview

### Objective

Implement, in MC, the media-routing control plane that programs MH for the ADR-0036 loopback demo (§7, §8, §9). See
`/tmp/devloop/story-runner/2026-08-27-hear-yourself-through-handler/task-13.prompt` for the verbatim task text.

### Scope
- **Service(s)**: `crates/mc-service` (primary), `crates/env-tests` (env-test), `crates/mh-service` (none expected — contract already landed task 12)
- **Schema**: No
- **Cross-cutting**: MC↔MH gRPC contract (already reshaped by protocol in `2026-09-01-internal-contract-reshape`; this task CONSUMES it)

### Debate Decision
NOT NEEDED — ADR-0036 already decides the design; this is implementation.

### Notes on prior-landed dependencies (verified at setup)
- `proto/dark_tower/internal/v1/internal.proto` already carries `EgressStream` (egress_stream_id, subscriber, candidate_sources, priority_group, supersede_on_independent_frame, transport_mode), `RegisterMeetingRequest.egress_streams` + `policy_generation`, and `RegisterMeetingResponse.{accepted, applied_generation, handler_id, process_start_epoch_ms, transport_mode}`.
- `RouteMedia` is already deleted from the proto and MH; `grep -rn "RouteMedia\|route_media" crates/mc-service/` returns **no hits**, so the task's "update the two MC mocks / drop the imports" item appears already satisfied. Implementer must verify, not assume.

---

## Cross-Boundary Classification

Per ADR-0024 §6.2. **No `proto/**` row exists — this task CONSUMES the already-landed contract
(`2026-09-01-internal-contract-reshape`). Any proto hunk in my diff would be a defect, not a plan item.**

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `crates/mc-service/src/media_routing/mod.rs` (new) | Mine | — |
| `crates/mc-service/src/media_routing/assignment.rs` (new) | Mine | — |
| `crates/mc-service/src/media_routing/generation.rs` (new) | Mine | — |
| `crates/mc-service/src/media_routing/confirm.rs` (new) | Mine | — |
| `crates/mc-service/src/lib.rs` | Mine | — |
| `crates/mc-service/src/grpc/mh_client.rs` | Mine | — |
| `crates/mc-service/src/grpc/gc_client.rs` | Mine (D-14 same-owner-sibling: the 5th `TokenReceiver` copy; 4-line deletion, no behaviour change) | — |
| `crates/mc-service/src/grpc/mod.rs` | Mine | — |
| `crates/mc-service/src/webtransport/connection.rs` | Mine | — |
| `crates/mc-service/src/webtransport/server.rs` (threading only, if needed) | Mine | — |
| `crates/mc-service/src/errors.rs` | Mine | — |
| `crates/mc-service/src/main.rs` | Mine | — |
| `crates/mc-service/src/observability/metrics.rs` | Mine (ADR-0031 service-owned metric definitions) | reviewers: observability + security (label content) |
| `crates/mc-service/tests/common/mod.rs` | Mine | — |
| `crates/mc-service/tests/media_policy_push_integration.rs` (new) | Mine | co-designed with test |
| `crates/mc-service/tests/register_meeting_integration.rs` | Mine | — |
| `crates/mc-service/tests/otel_grpc_outbound_integration.rs` | Mine | — |
| `crates/mc-service/tests/join_tests.rs` | Mine (constructor-signature compile fallout) | — |
| `crates/mc-service/tests/common/accept_loop_rig.rs` | Mine (constructor-signature compile fallout) | — |
| `crates/mc-test-utils/src/mock_mh.rs` | Mine (MC test-utils crate) | reviewers: dry-reviewer, test |
| `crates/mc-service/src/actors/controller.rs` (generation-registry eviction on meeting teardown) | Mine | — |
| `crates/mc-service/tests/gc_integration.rs` (third `TokenReceiver` helper copy, hoisted) | Mine | — |
| `crates/mc-test-utils/src/lib.rs` | Mine | — |
| `crates/mc-test-utils/Cargo.toml` | Mine | — |
| `crates/mc-test-utils/src/media.rs` | Mine (loopback policy fixtures, built by calling MC's real `compute_assignment`) | reviewers: dry-reviewer, test |
| `crates/mc-test-utils/src/token.rs` (new) | Mine (third `TokenReceiver` copy hoisted here) | reviewers: dry-reviewer |
| `crates/env-tests/tests/26_mh_quic.rs` | **Not mine, Minor-judgment** | **test** (paired per task text) |
| `docs/observability/metrics/mc-service.md` | Mine (ADR-0031 service-owned catalog) | reviewer: observability |
| `infra/grafana/dashboards/mc-overview.json` | **Not mine, Minor-judgment** (ADR-0031 makes MC the implementer; the file lives in infra) | **observability** (panel semantics), infrastructure (file) |
| `docs/observability/metrics/mh-service.md` (§`mh_media_policy_applies_total` window prose) | **Not mine, Minor-judgment** | **media-handler** (+ observability, catalog) |
| `infra/grafana/dashboards/mh-overview.json` (`Forwarding-Policy Applies by Outcome` panel `description` only) | **Not mine, Minor-judgment** | **media-handler** (+ observability) |
| `docs/runbooks/mh-incident-response.md` (Scenario 13, second blockquote) | **Not mine, Minor-judgment** | **operations** (ADR-0031 runbook ownership; ACK offered) + media-handler |
| `docs/runbooks/mc-deployment.md` (MC↔MH post-deploy addendum, 2 lines) | Mine | reviewer: operations |
| `docs/runbooks/mc-incident-response.md` (Scenario 12 Alert + Symptoms: the new terminal log string) | **Not mine, Minor-judgment** (entered scope at Gate 3 — the split exit log creates a second greppable string this scenario did not cover; @operations named it and left it my call) | **operations** |
| `crates/dt-guard/src/common/pii_vocabulary.rs` | **Not mine, Minor-judgment** (guard *policy content* per CLAUDE.md `dt-guard` rule; membership in an existing list, no construct/order change → NOT machinery, infra not a co-owner; `dt-guard` is not a GSA) | **security + observability** |
| `crates/dt-guard/src/metric_labels.rs` | **Not mine, Minor-judgment** (the `PiiCategory::Prefix` test module, plus EDIT 12's finding-message string — a `format!` literal changes no construct shape, no guard condition, no `rule_id` and no evaluation order, so it is policy *meaning*, not machinery) | **security + observability** |
| `docs/observability/label-taxonomy.md` (§Enforcement reality R1, §R1 blockquote, hashed-suffix blockquote, Category B `sender_id` row, §Prefix denylist, escape-hatch bullet, both Rule Index rows, and the extension procedure at 4 sites) | **Not mine, Minor-judgment** | **observability** |
| `docs/specialist-knowledge/meeting-controller/INDEX.md` | Mine | — |
| `docs/TODO.md` | Mine | — |
| `docs/devloop-outputs/2026-09-02-mc-media-routing-control-plane/main.md` | Mine | — |

**Row deliberately absent — `crates/env-tests/src/fixtures/**`.** The plan carried a conditional row
("only if a gauge-poll helper is needed"). @test ruled at Gate 1 that the condition is **NOT met**:
`poll_until_any_instance_above` plus `any_instance_exceeds_baseline` cover the env-test as it stands,
and the gauge assertion stays in the component test where `MetricAssertion` gives deterministic
absent-versus-zero. The row is removed rather than left in the table, because a planned-but-untouched
path is itself scope drift; the diff touches that directory zero times, and touching it is a review
finding.

---

## Planning

### Mechanism restatement (not instance)

The task names an instance ("the loopback edge"). The mechanism is: **MC owns a pure function from
meeting state to a per-handler forwarding snapshot, a change-detector that numbers those snapshots,
a one-shot programming call that carries a snapshot, and a total classifier of the handler's reply
into one bounded outcome.** The loopback is the N=1, one-participant evaluation of that function.
Same-owner siblings the wider mechanism will pull in later (structural re-push on join/leave,
cadence, connectivity trigger) are *deliberately* out of scope, and the shape below is chosen so
they are additive: the registry that holds (meeting, handler) → (snapshot, generation) is the exact
state a cadence task needs, and the push path takes a snapshot rather than a join event.

### 1. New module `crates/mc-service/src/media_routing/`

**`assignment.rs` — the general visibility-graph computation (pure, no clock, no I/O, no Redis).**

Input (built by the caller, never read inside):
```
MeetingRoutingInput { participants: Vec<RoutingParticipant>, handlers: Vec<HandlerId> }
RoutingParticipant { sender_id: media_admission::SenderId, handlers: Vec<HandlerId> }
```
Participant→handler membership is an **input**, not an assumption. Today every participant is on
every handler assigned to the meeting (clients `connectAll()`), so the caller fills it that way;
when real per-participant placement lands it is an input change, not an algorithm change.

Algorithm (runs identically at any N; there is no `if handlers.len() == 1` anywhere):
1. Visibility: subscriber S sees publisher P iff `handlers(S) ∩ handlers(P) ≠ ∅` (ADR-0036 §9).
   The relation is **reflexive in this story** — S sees S, which is what makes "hear yourself" the
   N=1 output of the general rule rather than a special case. Expressed as one named predicate
   (`fn subscribes_to(subscriber, publisher) -> bool`) so the future "don't echo yourself" policy is
   a change to a predicate, not a branch added to the graph walk.
2. Edge→handler: each (P→S) edge is assigned to **exactly one** handler — the first shared handler in
   sorted order (deterministic tie-break). This is the property §9 makes MH-obliviousness rest on.
3. Grouping into egress streams: per (handler, subscriber) the subscriber's **one main-audio slot**
   becomes one `EgressStream` whose `candidate_sources` are the publishers visible to that subscriber
   *on that handler*, sorted by `sender_id`. Selection among many candidates is MH's §7 job — this is
   why the loopback yields one candidate rather than needing a different message shape later.
4. Output `MeetingAssignment { per_handler: BTreeMap<HandlerId, HandlerAssignment> }`, canonically
   ordered so structural equality is well-defined (that equality is what the generation derives from).

Per-egress behaviours, all MC-computed, none client-supplied (@security 3):
- `priority_group` = `AUDIO_PRIORITY_GROUP` (MC owns the numbering; a module constant, not config —
  making it an env var would hand an operator a lever on forwarding policy, which §5/§7 bar).
- `supersede_on_independent_frame` = **false** for audio ("every audio frame forwards" is a
  consequence of this flag; MH never learns it is audio).
- `transport_mode` = `TransportMode::Datagram` (§1), from `dark_tower.signaling.v1`, imported — no
  parallel enum, no re-mapping table, one construction site (@dry 2).
- `stream_number` = `MAIN_AUDIO_STREAM_NUMBER` (0), the single MC-side constant story task 14's send
  directive will consume, so the two sides cannot drift.
- Provisional slot policy: one main-audio slot per subscriber until the receive-capability
  declaration lands (task 14 owns it); documented as provisional at the one site that produces it.
- `egress_stream_id` = `u32::from(sender_id) << 8 | egress_ordinal(u8)` — **unique by construction**
  (`sender_id` is unique per participant and never recycled; the ordinal is unique per subscriber),
  stable across a client slot renumber, and fail-loud if a subscriber ever exceeds the 8-bit ordinal
  (@mh 9: uniqueness structural, not by luck of the loopback shape).

Reuse, explicitly: `media_admission::SenderId` for every sender id (@dry 1). **No MC-side slot-id
newtype and no shared 16-bit helper** — `slot_id` here is a plain provisional `u16` produced by MC,
and collapsing it with `SenderId` is barred by name in `internal.proto` (@dry 7). **No mirrored
validator**: MC is the producer and satisfies MH's `MeetingPolicy::from_request` rejections by
construction; the safety net is a unit assertion on the built request, not a runtime mirror (@dry 8).
No `ANCHOR (DRY):` comments — MC encodes neither `STREAM_ID_FIELD_BYTES` (slot rides as `uint32`)
nor `FLAG_INDEPENDENTLY_DECODABLE`; a decorative anchor over a value MC does not hold is worse than
none (@dry anchors).

**`generation.rs` — `policy_generation`, derived from output change.**

`PolicyGenerations`: process-wide `Arc`, `Mutex<HashMap<(MeetingId, HandlerId), Programmed>>` where
`Programmed { assignment: HandlerAssignment, generation: NonZeroU64 }`.
`next_generation(meeting, handler, &new_assignment) -> NonZeroU64`: identical assignment → the
**same** number (so a re-assert is MH's no-op, §8); different → `generation + 1`; first ever → **1**.
Type is `NonZeroU64`, so "0 on the wire" is unrepresentable rather than merely forbidden, and MC
never ships the `policy_generation: 1`-with-empty-policy workaround (@mh 8, @security 2).
A comment at the definition site records the **three-generation non-collapse** — Redis fencing
(`get_generation()`/`increment_generation()`), `JoinResponse.kek_generation`, and this — in the
sibling-boundary style `media_admission/` already uses (@dry 5, 6). Nothing in this module imports
`redis`.

**`confirm.rs` — total classification of the response (pure).**
```
PushExpectation { handler_id, policy_generation: NonZeroU64, transport_mode }
PolicyPushOutcome { Match, HandlerIdMismatch, NoAppliedGeneration, GenerationMismatch, TransportModeMismatch }
   + ::ALL (a sixth value is a compile error, mirroring mh-service's PolicyApplyOutcome) + ::label()
evaluate(&PushExpectation, &RegisterMeetingResponse) -> PolicyPushOutcome
```
Precedence, exactly as @media-handler specified and for the reason they gave (a failed apply on a
never-programmed meeting shows `applied == 0` *and* `UNSPECIFIED` mode; transport-first would
mis-label the actionable truth):
`handler_id_mismatch > no_applied_generation (applied == 0) > generation_mismatch (applied != sent) >
transport_mode_mismatch (generation matched, mode differs) > match`.
Transport mode is compared **`sent != echoed`, unconditionally** — there is no
`if echoed != UNSPECIFIED` escape hatch anywhere in the function (@obs 4, @test trap 1, @mh 5).
`accepted == true` is **not** success; only `applied == sent` is (@mh 2).
`process_start_epoch_ms` is carried into the registry and the log line, compared for **inequality
only**, `0` treated as "restart undetectable" — restart *detection* is the deferred story, so no
outcome and no metric hangs off it here.

### 2. `mh_client.rs` — the push and the fail-loud confirm

`MhRegistrationClient::register_meeting` takes one borrowed struct instead of four positional
`&str`s (`MeetingProgramming { mh_grpc_endpoint, expected_handler_id, meeting_id, mc_id,
mc_grpc_endpoint, assignment: &HandlerAssignment, policy_generation: NonZeroU64 }`) and still returns
`Result<(), McError>`. `MhClient::register_meeting` builds the `RegisterMeetingRequest` from the
assignment (this deletes the `policy_generation: 0` literal and both TODOs above it), calls
`confirm::evaluate`, then:
- emits the two metrics (§ below) on **every** outcome, including `match`;
- on any non-`match`: `error!` with `outcome`, `sent_generation`, `applied_generation`, `handler_id`,
  `meeting_id`, `key_custody=operator` — numbers in the log line, never in a label (@obs 5, 6) — and
  returns `Err(McError::MediaPolicyDivergence { outcome: &'static str })` (new variant, bounded
  `&'static str`, new `error_type_label()` arm). Fail loud, not warn-and-continue.
No key material of any kind is read, logged, or placed on the request; the request builder touches
`meeting_id`/`mc_id`/`mc_grpc_endpoint`/edges/behaviours/generation only (@security 1, 7).

### 3. Trigger — unchanged, deliberately

The existing R-12 first-participant trigger in `connection.rs` stays the trigger; it now computes the
assignment from `join_result` (self `sender_id` + roster) × `mh_data.handlers`, takes a generation per
handler, and pushes. **One-shot push + confirm, exactly as the task scopes it.** Structural re-push on
every join/leave, cadence, connectivity trigger, jitter, cadence-vs-provisional-timeout startup
validation and re-assert paging are NOT in this diff — they are the handler-restart story and land
additively on this registry. The existing bounded retry/backoff around the call is retained, with two
constraints @operations named (OPS-3):
- **the generation is computed once per push, in `connection.rs`, outside the retry loop** — never
  inside `MhClient::register_meeting`, where it would be recomputed per attempt and break §8's
  "unchanged policy carries the same number";
- **retryable vs terminal outcomes are split.** `generation_mismatch` and `no_applied_generation`
  are transient apply failures and consume retries (an identical re-send is an idempotent MH no-op).
  `handler_id_mismatch` and `transport_mode_mismatch` are **terminal** — a misrouted or
  version-skewed handler does not become correct after backoff, so MC fails immediately and loudly
  rather than delaying the loud failure by three attempts.

### 4. Metrics (both in the `observability/metrics.rs` facade; no bare macro anywhere else)

- `mc_media_generation_divergence` — **gauge**, labels `key_custody` only, value =
  `sent.abs_diff(applied)` — an unsigned **magnitude**, not a difference (see amendment S-1/O-7
  below) — computed from the **applied** value in the response. Feeding it from the sent
  value yields a constant 0, which is precisely the silent regression the task names.
  **Semantics chosen deliberately (@obs 2, @ops OPS-2): option (a), last-observed divergence
  magnitude.** Its clearing path is the next push of any (meeting, handler) on this pod — it is a
  value that is overwritten, never a latch, so it cannot wedge above zero for a pod's lifetime the
  way a count-with-no-decrement would. The price, stated rather than discovered: it is pod-level
  last-write-wins, so a healthy push can erase a diverged reading. **Therefore the detection signal
  is the counter, not the gauge**, and I am recommending to @operations (task 21 owns the rule) that
  the page hang off `mc_media_policy_pushes_total{outcome!="match"}`, with the gauge used only as
  the magnitude a responder reads next.
  **OPS-1 obligation, accepted in full**: the gauge is written *only* on a registration push, and
  with no re-assert cadence in this story **it does not observe a handler restart** — a handler can
  lose all policy while the gauge holds its last healthy value. That sentence goes in the Rust doc
  comment, the `mc-service.md` catalog entry, and the dashboard panel description, and this devloop
  record does not claim the gauge covers the restart gap.
- `mc_media_policy_pushes_total` — **counter**, labels `outcome` + `key_custody` (NO `handler_id` —
  see amendment O-6-SUPERSEDED: the label's founding premise turned out false and it was dropped 3-1),
  `outcome` from the proto's canonical five, bounded by `PolicyPushOutcome::ALL` so a sixth value is a
  compile error.
  Label **key** is `outcome`, matching MH's `mh_media_policy_applies_total{outcome,key_custody}`, so
  both ends of one handshake sit side by side in a query.
- No `meeting_id`/`meeting_id_hash`, no `sender_id`/`slot_id`/`egress_stream_id`/`stream_number`, no
  generation number in any label.

### 5. Test plan (co-designed with @test; every point they raised is covered)

Unit (`media_routing`): N=1 loopback yields exactly one edge, self→self, one audio egress, one
candidate; **N=2 on one handler** yields 4 edges; **two handlers with disjoint membership** — each
handler receives only its own edges and every edge lands on exactly one handler; **empty edge set**
for a handler with no participants (the "send nothing" output reachable as a code path only);
`egress_stream_id` and `(sender_id, slot_id)` unique by construction; canonical ordering stable.
Generation: first = 1; identical recomputation → same number; changed output → +1; independent per
(meeting, handler); never sourced from Redis (asserted by absence of the import at review, not by a
test).
Confirm: five tests, one per outcome, including **`applied == 0` → `no_applied_generation`, asserted
to be neither `match` nor `generation_mismatch`** (@obs 3), and **transport mismatch driven by the
UNSPECIFIED echo specifically** (@test trap 1) plus a wrong-but-specified mode as a second case.
Component (`tests/media_policy_push_integration.rs`, new, `flavor = "current_thread"` — the pinning
is load-bearing): real `MhClient` against the configurable stub; (a) **injected apply failure** — MH
echoes a stale `applied_generation` while MC sent a higher one → assert the echo does not advance,
`mc_media_policy_pushes_total{outcome="generation_mismatch"}` delta 1, `mc_media_generation_divergence`
non-zero, call returns `Err`; (b) success → `outcome="match"`, gauge 0.
Env-test (`26_mh_quic.rs`, extended, **paired with @test**): drive the real join against Kind so MC
programs the live handler, then assert `mc_media_policy_pushes_total{outcome="match"}` increases —
that is the positive, non-vacuous signal (`match` is reachable only when the handler echoed the
applied generation MC sent) — and `mc_media_generation_divergence == 0`. Composes with, and does not
duplicate, MH's negative unit gates (@mh 12). @test: I need your ruling on whether the counter-delta
shape (`InstanceCounters` / `poll_until_any_instance_above`) covers this without a new fixture; my
reading is yes for the counter and that the gauge check can be a plain instant read.

### 6. Test-double hoist (@dry 9) — accepted, not deferred

One configurable tonic `MediaHandlerService` impl moves into `crates/mc-test-utils/src/mock_mh.rs`
(named `MediaHandlerStub`, distinct from the existing data-holder `MockMh`), with knobs for accept,
applied-generation behaviour (echo-the-sent value / stall at a stale value / report 0), handler id and
transport-mode echo, plus a `spawn()` returning the bound address. All three MC call sites use it:
`register_meeting_integration.rs`, `otel_grpc_outbound_integration.rs`, and the new push test. The
cost is small (the crate already depends on `tonic` + `proto-gen`); the alternative is three copies of
the echo semantics inside the very fixture whose job is to catch an echo lie. The MC-side
`TokenReceiver` helper triplicate (`make_token_rx` / `mock_token_receiver` / the `gc_integration.rs`
copy) is hoisted into the same crate in the same pass; the MH-side copy is out of scope.

### 7. RouteMedia item — VERIFIED ALREADY SATISFIED, not silently dropped

`grep -rn "RouteMedia\|route_media" crates/mc-service/` returns no hits. The only surviving mentions
in the tree are the proto tombstone comment and two MH-side retired-label references. The task's
"update the two MC mocks and drop the imports" item was completed by the
`2026-09-01-internal-contract-reshape` devloop; there is nothing left to do for it here, and both
files are in my diff for other reasons (the stub hoist).

### 8. Deferred, with the reason

Handler-restart recovery in full: periodic re-assert cadence, connectivity-loss trigger, dispatch
jitter, cadence-vs-provisional-timeout startup validation, re-assert-failure paging, and
`process_start_epoch_ms`-based restart detection. Task-sized, explicitly scoped out by the task text,
and made additive by the registry shape above.

### 9. No alert rule, no SLO, no runbook Scenario 15 — decisions, not omissions

`docs/observability/slos.md` §Open items and `docs/TODO.md` already record that the
applied-generation SLO and the `mc-alerts.yaml` rule are blocked on the handler-restart story (no
cadence, no observed behaviour, so any threshold now is unjustified). **`infra/docker/prometheus/rules/mc-alerts.yaml` is not touched**, and per @operations OPS-7 I am **not** adding
`### Scenario 15: Media Generation Divergence` to `docs/runbooks/mc-incident-response.md` either —
story task 21 owns that heading and its anchors are already referenced elsewhere. This paragraph is
the pointer so Gate 3 does not read the absence as an omission. I *am* adding the two-line MC-side
gate to `docs/runbooks/mc-deployment.md`'s existing post-deploy MC↔MH addendum that @operations
offered, worded with the OPS-1 limitation attached (it reflects only meetings programmed after the
rollout).

### 9b. MH-owned window prose — the task-13 obligation in `docs/TODO.md` §(b)

`docs/TODO.md` §Media Path Obligations (b) makes retiring the gen-0 window prose an explicit task-13
obligation at three MH-owned sites (MH metrics catalog, MH dashboard panel description, MH runbook
Scenario 13). Today all three say *"`no_generation` counting up is expected — do not open an
incident"*, which starts lying the moment MC emits >= 1. Per @operations OPS-5 the replacement is
**not** a flat inversion: a not-yet-upgraded MC pod legitimately emits 0 during a rollout, so the
wording becomes *expected only while an MC rollout is in progress; sustained `no_generation` after
the rollout completes means MC failed to compute a generation*. Three Minor-judgment rows,
@media-handler to ACK the hunks, @observability on the catalog, @operations has offered the runbook
ACK.

### 9c. MH-side code: NOT touched (@ops OPS-4)

This diff contains no `crates/mh-service/**` change. In particular I am **not** flipping MH's
`policy_generation == 0` rejection: MC and MH roll independently, so a same-commit flip lets
whichever pod rolls first decide, and MH-first rejects every registration from a not-yet-upgraded MC
pod — kicking provisional connections at the registration timeout. Enforcement is a separate change
whose precondition is "all MC pods emitting >= 1". I will tell @media-handler that this task
satisfies their precondition, and that `docs/TODO.md`'s MH generation-ratchet entry (any
`service.write.mh` holder can wedge a meeting with `policy_generation: u64::MAX`) becomes live
exposure once generations are load-bearing — filed with owners already; not discharged here.

### 9d. No operator lever (@ops OPS-8, OPS-9)

No `MC_MEDIA_*` env var, no ConfigMap key, no feature flag: not for the empty target set, not for
whether the push happens, and not for transport mode (a mode knob would make the §8 echo compare two
operator knobs instead of catching version skew). Rollback is redeploy-only; no schema, no data
change — `main.md`'s Rollback Procedure already says exactly that and stays as-is.

### 10. Known gap I am recording rather than hiding

`policy_generation` is monotonic per (meeting, handler) **within an MC process**. After an MC restart
the registry is empty and the meeting's next first-push starts again at 1, which MH — correctly —
ignores as lower than what it has applied. That is a real hole in the restart story (it is *loud*
here: the echo diverges, the counter increments `generation_mismatch`, the gauge goes non-zero), but
closing it needs a durable or handler-seeded floor and belongs with the restart work. Recorded in
`docs/TODO.md` §Media Path Obligations with owner `meeting-controller`, not left for a reader to
discover.

### 11. Gate-1 review amendments (all accepted; no design change)

**S-1 (@security) / OPS-11 (@operations) / O-7 (@observability) — the divergence value must not
underflow, and two of the three natural fixes are silent regressions. RESOLVED: `sent.abs_diff(applied)`.**
`sent − applied` on `u64` underflows on a reachable, documented input — MH ignores a lower generation
and echoes the *installed* one, so an MC restart (registry lost, re-derives from 1) against a handler
holding 5 yields `applied > sent`, and the `policy_generation: u64::MAX` ratchet wedge is the same
input adversarially. Debug: panic (ADR-0002). Release: wrap to ~1.8e19, a garbage spike that blows
out any shared panel axis.
Three candidate fixes, and the two rejected ones are rejected *in the doc comment* so neither is
reintroduced as a "fix": (i) `saturating_sub` maps `applied > sent` to **0**, a healthy-looking zero
on the one response shape that proves MC and MH disagree about which policy is live — the same class
of silent regression as feeding the gauge from the sent value, by a different route; (ii) a signed
value is meaningful but any stat threshold written `> 0` renders the negative arm green, and `> 0` is
what anyone will reach for.
So: **unsigned magnitude, `u64`, cannot underflow, cannot panic, and every non-zero value means "the
live policy is not the one MC pushed" regardless of direction.** Direction is not lost — the `error!`
line carries both numbers, so a responder reads *which way* off the log and *how far* off the gauge,
which is the split already chosen. The word "magnitude" (not "difference") is used in the doc comment,
the catalog entry and the panel description, because that word is what stops the sign being
reintroduced. `applied > sent` is its own test input, not only `applied < sent` — it is the arm that
underflows and it is invisible to a suite written against the too-low case only. The arm still lands
on `generation_mismatch`; the five-value vocabulary is unchanged.

**S-2 (@security) — `handler_id` has the same fail-open shape as transport mode, and only one was
inoculated.** `handler_id` is a proto3 `string`, so an absent field decodes to `""` and
`if !echoed.is_empty() { compare }` is the identical fail-open against the identical peer
population, with nothing in the proto warning against it (the warning was written only for the enum).
The comparison is `expected != echoed` **unconditionally, empty string included**, and the
empty-string echo gets its own `confirm.rs` case alongside the UNSPECIFIED-echo case. The precedence
order helps here: `handler_id_mismatch` is first, so an empty echo classifies as the most actionable
outcome rather than falling through to comparisons against a handler MC cannot identify.

**S-3 (@security) — `PolicyGenerations` gets an eviction path.** As planned it grew for the pod's
lifetime, holding a whole `HandlerAssignment` per ended meeting — the leak class already recorded in
`docs/TODO.md` (a floor that ratchets while ended meetings' edges are never released). Eviction hangs
off the existing choke point: `actors/controller.rs::remove_meeting()`, on the same line that already
calls `mh_connection_registry.remove_meeting()`, matching the in-tree precedent where
`redis/client.rs`'s `local_generation` is cleared on `delete_meeting`. **Keyed on meeting teardown,
never on handler removal** — a meeting whose handler set changes must keep its generation, or the
restart-at-1 problem reappears inside a live meeting. Lock is `tokio::sync::RwLock`, the in-tree
idiom for this map shape (`mh_connection_registry.rs`, `redis/client.rs`); a `std::sync::Mutex` would
need poison handling that cannot use `unwrap`/`expect` under the workspace lints.

**S-4 (@security) — `docs/TODO.md` ratchet entry gets a landed-trigger amendment.** That entry names
"story task 13" as the trigger at which the `policy_generation: u64::MAX` wedge stops being
theoretical. This devloop IS task 13, so the entry is updated in place — in the same visible-correction
register the `.proto`-scanner entry uses — to record that the trigger has landed and on which commit.
The *fix* stays deferred (all three options are MH- or AC-side and need a design decision with
different failover consequences); only the tense is corrected. The "it fails loudly here" property is
recorded in the TODO entry itself, not only in this plan document.

**@security also asked for the `AUDIO_PRIORITY_GROUP` rationale at the definition site**, not only
here, because a module constant reads as a config-over-hardcoding violation: the priority group is
the *bound* on how far a self-declared salience signal can promote a publisher (§7); an env var makes
that bound operator-tunable, a misconfiguration flattens it fleet-wide, and the §8 echo cannot catch
it because the echo covers transport mode, not priority. That paragraph goes at the constant.

**D-10 (@dry-reviewer) — record the 8-bit non-collapse.** The ordinal width in
`egress_stream_id = sender_id << 8 | ordinal` is an **independent policy-plane choice** that merely
coincides with `media-protocol`'s `KEY_ID_STREAM_BITS = 8`. It is deliberately NOT derived from it —
deriving would couple MC's policy-plane id packing to the SFrame wire layout. MC will hold two
adjacent 8s in this area (the other, `stream_number`'s bound, genuinely IS anchored to
`KEY_ID_STREAM_BITS`), which is the "two adjacent `= 32`" hoist shape, so a comment at the derivation
site names both and says which is which.

**D-11 (@dry-reviewer) — `PolicyPushOutcome::label()` does owe an `ANCHOR (DRY):`.**
`internal.proto` claims sole definition of the five-value `outcome` vocabulary ("defined here and
deliberately not restated elsewhere") and the Rust arms restate it — N=2 sites holding one value set.
The anchor points at `RegisterMeetingResponse.applied_generation`. (Unguarded anchor, joining the
existing class.) `PolicyPushOutcome` vs MH's `PolicyApplyOutcome` are confirmed NOT duplication —
apply-side outcome vs confirm-side classification, different value sets, shared idiom only.

**@test — precedence-collision tests, REVISED to the OPS-18 ordering**
(`no_applied_generation > generation_mismatch > transport_mode_mismatch > handler_id_mismatch > match`).
The five single-condition tests pin no ordering; every one still passes under any arm order. Four
collision tests, each pinning one adjacent boundary of the NEW chain, named `..._pins_precedence_...`:
- **(i)** applied==0 AND handler_id wrong AND transport UNSPECIFIED → `no_applied_generation`
  (pins no_applied above ALL, including handler_id — the input that under the OLD order returned
  `handler_id_mismatch`; this is the test whose expectation inverts with the reorder).
- **(ii)** applied non-zero and != sent AND handler_id wrong AND transport UNSPECIFIED →
  `generation_mismatch`, asserted NOT `transport_mode_mismatch` and NOT `handler_id_mismatch`.
- **(iii)** transport mismatch AND handler_id wrong, generation matched → `transport_mode_mismatch`,
  asserted NOT `handler_id_mismatch` (pins transport above handler_id).
- **(iv)** handler_id wrong, generation matched, transport agreed → `handler_id_mismatch`, AND the
  call result is `Ok` (non-fatal). This is the former "pins handler_id at TOP" test with its
  expectation inverted to "pins handler_id at BOTTOM, non-fatal" — the single test that most directly
  encodes the OPS-18 decision, so its name says so.
Eight confirm tests total (5 single + 3 boundary + the non-fatal-Ok assertion folded into (iv)).

**@test — env-test verdict shape corrected.** The gauge instant read is OUT of the env-test: a gauge
that materialises no series makes absent-vs-zero ambiguous and a snapshot can be masked by an
intervening scrape (vacuous pass or flake). The verdict is `mc_media_policy_pushes_total{outcome="match"}`
delta ≥ 1 (reachable only on echo==sent + handler id + transport agreeing, so it cannot pass against
an unprogrammed handler), guarded by delta == 0 on the four non-match outcome series, where an absent
series correctly reads as 0. The gauge assertion stays in the component test, where `MetricAssertion`
gives deterministic absent-vs-zero.

**O-3 (@observability) — the log carries every comparison, not just the winning label.** A single
bounded `outcome` reports one failure; a response can be `generation_mismatch` AND
`transport_mode_mismatch` at once. The `error!` line carries sent/applied generation, sent/echoed
transport mode, and expected/echoed handler id as structured fields regardless of which label won.
Metric stays single-label.

**O-4 / O-5 (@observability) — dashboard conformance.** `noValue: 0` stays on the gauge stat panel
(the alternative, "No data" on every pod with no meetings, is worse noise) but it would be the tree's
first use and it renders "never pushed" and "recorder regressed" identically as 0, so the panel
`description` states plainly that an absent series renders as zero and that liveness is read from the
`mc_media_policy_pushes_total` panel beside it. Both panels get
`fieldConfig.defaults.unit = "short"`. The OPS-1 "written only on a push, no cadence" sentence cites
the **scrape-interval config key, never a number** (per `dashboard-conventions.md` §Periodicity), and
the catalog entry records @observability's additional point: MC inherits the global scrape interval,
which is coarse relative to a one-shot-per-registration write, so a divergence can be overwritten by
a later healthy push before it is ever scraped — a second, independent reason the gauge cannot be the
detection signal.

**OPS-11..15 (@operations), all accepted, one refined.** (11) Resolved as `sent.abs_diff(applied)` — see S-1/O-7
above. It satisfies @operations' concern about `saturating_sub` (the wedge reads NON-zero, not
green) without the panel-threshold hazard a signed value carries. (12) The catalog records that the counter counts **response
evaluations, not meetings** — a retried push contributes one sample per attempt, terminal outcomes
exactly one — so a divergence ratio is not misread. (13) Confirmed: **both** metrics get a catalog
entry AND a dashboard panel; the guards are per-metric. (14) The `mc-deployment.md` addendum names
the asymmetric-rollback hazard: sustained `no_applied_generation` with retries exhausted after a
deploy means MH is below the ADR-0036 contract, and **the remedy is to roll MH forward, not to roll
MC back** — rolling MC back returns to gen-0 registrations, which is the pre-change blackhole, not a
fix. (15) The §10 TODO entry names its **trigger**: it must be closed *before* the re-assert cadence
lands, because cadence is what re-pushes a surviving meeting and turns a barely-reachable hole into
one that fires on every MC rolling deploy. Also carrying the "counter is the detection signal, gauge
is the magnitude" sentence into the **catalog entries**, not only this record.

**O-6 — SUPERSEDED (@team-lead adjudication, 3-1 to DROP; the refuted reasoning is kept, not deleted,
because its premise turning out false is the point).**
~~O-6 reversed @observability's point 5 and put `handler_id` back on the counter, on the argument that
`sum by(handler_id) (increase(mc_media_policy_pushes_total{outcome!="match"}[...]))` separates one
handler diverging (cordon that pod) from every handler diverging (MC's computation is broken), with
cardinality "MH pod count, which the proto explicitly blesses as §11's aggregation floor."~~
**That founding premise is false in this tree**: `handler_id` is a *per-incarnation* token (fresh
`Uuid::new_v4()` at process start; `MH_HANDLER_ID` unset in all three manifests), NOT a pod-level
identity — so its cardinality is one retained series per incarnation MC ever pushed to, unbounded and
worst under a crashloop, exactly when the metric must work. @operations (OPS-16), @media-handler and I
all rejected the premise; @team-lead adjudicated between the two mandatory reviewers and DROPPED the
label. **Final shape: `mc_media_policy_pushes_total{outcome, key_custody}` and
`mc_media_generation_divergence{key_custody}`.** The one-handler-vs-all-handlers query O-6 wanted is
answered from the `error!` line, which carries `handler_id` per O-3 — the query is real, the *series*
is what was unaffordable. The label may return under the same query once the spin-out
`2026-09-02-mh-stable-handler-id` makes the id stable and the "pod/handler level" premise actually
true; recorded so the return is a deliberate re-add, not a rediscovery. (The gauge was never going to
carry `handler_id` regardless — last-write-wins with no decrement means one stale series per retired
pod, the option-(b) defect through the label set.)

**OPS-16 / OPS-17 (@operations) — `handler_id` is a per-INCARNATION token, not a handler identity.
I verified both claims and they hold; proposed resolution below, pending Lead adjudication.**

Verified independently: `crates/mh-service/src/config.rs` derives `handler_id` from
`HOSTNAME` + the first 8 chars of a **fresh `Uuid::new_v4()` sampled at process start** whenever
`MH_HANDLER_ID` is unset, and it is unset in all three MH manifests (deliberately, per their
comments). So the value changes on every MH process start. I also traced the assignment-refresh
question @operations flagged as their least-certain point: `gc-service/src/services/mc_assignment.rs::assign_meeting_with_mh`
re-selects MHs on an existing healthy assignment (the comment even says "they may have changed") but
**returns early without calling MC's `AssignMeetingWithMh`**, so MC's Redis `MhAssignmentData` keeps
the handler ids captured at first assignment. Their reading is correct. **@global-controller
calibration (verified against source): the `handler_id_mismatch` root cause is the per-incarnation id
ALONE — it fires regardless of the GC early return.** The GC early return is a *separate* defect that
perpetuates snapshot staleness (endpoint/capacity drift) but is not what makes a correct restarted
handler read as mismatch. Correcting my own earlier wording: the client's real media connection is
built from **MC's Redis snapshot** in the WebTransport `JoinResponse` (`webtransport/connection.rs`),
NOT from GC's freshly-selected list — so the harm is GC-internal-vs-MC divergence, not a fresher list
reaching the client. There is also no TTL on `meeting:{id}:mh` (only `delete_meeting` clears it), so
the snapshot is frozen for the meeting's whole lifetime — which is why a per-incarnation id guarantees
a post-restart mismatch rather than merely risking one.

- **OPS-17 consequence**: with `handler_id_mismatch` terminal, an ordinary MH pod restart terminally
  fails the policy push for every meeting whose assignment predates it — clients then kicked at the
  registration timeout. That is ADR-0036 §8's blackhole through an ordinary rolling deploy, reached
  via the field added to detect misrouting. A correct handler and a misrouted one are
  indistinguishable on this check, and the correct one is the common case.
- **Proposed resolution (b), interim** [**precedence SUPERSEDED by OPS-18: demoted to LOWEST, not
  top — see below**]: implement the comparison exactly as the contract specifies — unconditional,
  empty string included, its own bounded `outcome` value, `error!` log — but make it **non-fatal**: it
  does not by itself fail the push or consume retries.
  The observable lands in full; the outage does not. The comment at the site names the precondition
  for making it fatal (a stable `MH_HANDLER_ID`), and `docs/TODO.md` gets the entry with that trigger.
  Resolution (a) — set `MH_HANDLER_ID` explicitly per deployment — is the real fix, spans
  `crates/mh-service` + three infra manifests + GC's registry semantics, and is task-sized; (c),
  parsing a "stable component" out of an opaque id, is the topology-in-application-code CLAUDE.md
  bars and is rejected.
- **OPS-16 consequence**: as a metric label, `handler_id` is one new permanently-exported series per
  MH incarnation MC has ever pushed to — a crashlooping MH is the worst case and is exactly when the
  metric must work. It would also be the tree's first use of `handler_id` as a label. **This
  contradicts @observability's O-6 reversal**, which was argued on the premise that the field is a
  pod-level identity (the proto's own wording). The premise is false in this tree. My recommendation
  is @operations': **no `handler_id` label on either metric**; the one-handler-vs-all-handlers triage
  question is answered from the `error!` line, which carries `handler_id` per O-3. Flagged to
  @team-lead for adjudication between two mandatory reviewers rather than resolved by me.

**OPS-18 (@operations) — with `handler_id_mismatch` non-fatal, it must also be DEMOTED to lowest
precedence for the interim.** `evaluate()` returns one outcome, so a benign, expected-by-construction
`handler_id_mismatch` sitting at the top of the chain would suppress a real `generation_mismatch`
underneath it for the entire post-restart window — a false positive masking the exact control this
task exists to build, and silently, which is worse than the loud outage it replaced.
@media-handler's handler-first ordering was correct on the premise that a wrong handler makes every
other field meaningless; that premise holds for a trustworthy identity and inverts for a
per-incarnation token. Interim order therefore becomes:
`no_applied_generation > generation_mismatch > transport_mode_mismatch > handler_id_mismatch > match`,
so `handler_id_mismatch` means "policy applied correctly, identity differs" — precisely the benign
restart case — and can never hide a generation fault. The comment sits on the ordering itself and
names the condition for restoring handler-first: a stable `MH_HANDLER_ID`, which is part of (a)'s
definition of done. No enum change, no new metric, no proto change. **Pending @media-handler's
concurrence, since they authored the original order.**
**(ii) Alerting**: a non-fatal `handler_id_mismatch` still increments the counter with a non-`match`
outcome, so task 21's page (`outcome!="match"`) would fire on every MH rollout. The catalog entry —
not just this record — states that until `MH_HANDLER_ID` is stable, `handler_id_mismatch` is a
**diagnostic, not an alerting signal**, and that the alert expression must read
`outcome!~"match|handler_id_mismatch"`. An honest inert observable is fine; an observable that
silently is not alertable is not.
**(a)'s definition of done spans TWO halves, named so the easy one is not closed alone**: stable
`MH_HANDLER_ID` (MH config + three infra manifests) AND GC's refresh path — because with the early
return in `assign_meeting_with_mh` still in place, a stable id leaves MC's snapshot stale for a
*different* reason (a genuinely reassigned handler), producing a check that looks trustworthy and is
not. The `docs/TODO.md` entry names both halves with split owners, and states plainly that MC
deliberately does not obey the contract's literal `handler_id` MUST because that MUST was written
against a premise the tree does not satisfy — otherwise the next reader "fixes" it.
**Related, out of this diff's scope, filed not fixed**: the GC early return leaves MC's Redis snapshot
stale against a genuinely reassigned handler (GC-vs-MC divergence, endpoint/capacity drift). Per
@global-controller this does NOT reach the client as conflicting connect targets — the client builds
its media connection from MC's snapshot — so it is a GC-owned refresh-gap follow-up (@main to file),
not an MC correctness bug and not this diff's problem. It matters here only as the second half of the
spin-out's definition of done: a stable `MH_HANDLER_ID` alone still goes stale against a reassigned
handler, so the check would look trustworthy and not be.

**MH-CONCUR (@media-handler) — the demotion is accepted as the CORRECT posture, with three hard
requirements, and the call-result mapping is now fully pinned.** @media-handler's framing, adopted:
a fatal check on a per-incarnation id reintroduces §8's blackhole-through-rolling-deploy on every MH
restart, so fatal-now would be a *misreading* of the proto's MUST, not an implementation of it;
loud-non-fatal satisfies "fail loud" via the observable. `handler_id_mismatch` stays IN the
five-value enum (contract-faithful to the proto's canonical enumeration and to
`PolicyPushOutcome::ALL`), not surfaced as a side channel.

Requirement 1 — **a `handler_id` mismatch must not suppress a successful program.** Satisfied
structurally by the demotion combined with non-fatal: because `handler_id_mismatch` now ranks below
generation and transport, the outcome is reached ONLY when `applied_generation == sent` and transport
agreed — i.e. the meeting genuinely IS programmed — and it maps to `Ok`, not `Err`. A correct restart
is never turned into a kicked meeting.

Requirement 2 — **the `error!` line on `handler_id_mismatch` must carry whether `applied == sent`.**
Satisfied by O-3 (every comparison's operands logged regardless of the winning label). Worth stating
what the demotion additionally buys, because it is stronger than @media-handler's requirement asked
for: a genuine misroute (wrong handler, so `generation_for(meeting)` is 0 or a different meeting's
state) outranks `handler_id_mismatch` and surfaces as `no_applied_generation` / `generation_mismatch`
— which are `Err` and retried — so under the demoted order a *reported* `handler_id_mismatch` is
already the benign "id differs, policy correct" case by construction. The one residual is a genuine
cross-wire to a handler that happens to hold the same meeting at the same generation (reachable only
multi-handler); the logged `applied == sent` field is what still distinguishes it, which is exactly
why the field is required rather than inferred from the label.

Requirement 3 — **the site comment names the flip precondition and the spin-out slug.** Comment on
the ordering reads: restore handler-first and make fatal once `handler_id` is stable per
deployment (the mechanism is the spin-out's to choose — @global-controller notes MH is a Deployment
so HOSTNAME is unstable across rollouts and the id derives from the `instance:` pod-template label,
not a StatefulSet name), tracked in `2026-09-02-mh-stable-handler-id`. @media-handler's
site-comment checklist (a)/(b)/(c) — interim-and-reverts-only-when-id-stable, why (per-incarnation id
makes the mismatch expected-on-restart so top precedence would mask gen faults), and the slug — is the
same three points, adopted verbatim as the comment's structure.

Per @media-handler's refinement: the `handler_id_mismatch` `error!` line carries a one-word
`(interim)` marker so a responder reads it as the expected stable-id-not-yet-deployed case at a glance.
The demoted ordering already encodes "did generation match" (a gen fault outranks and reports
`generation_mismatch`), so the explicit did-gen-match field is belt-and-suspenders, not load-bearing —
but O-3 logs all operands anyway, so it costs nothing and covers the residual multi-handler cross-wire.

**Lead conditions 1 & 2 — the definition-site comment carries WHY and TIES the two together.** The
comment at the demoted-and-non-fatal site states, in CLAUDE.md's fail-loudly register so it does not
read as a swallowed error: the failure is *fully observable* — `error!` log, `mc_media_policy_pushes_total{outcome="handler_id_mismatch"}`,
and a named bounded outcome — and ONLY the abort is suppressed; masking would be not reporting it,
which this does not do. And: **non-fatal and lowest-precedence are one decision with one cause and
revert together.** When `MH_HANDLER_ID` becomes stable, handler-first ordering AND fatality return
together; neither is independently motivated, and a later reader restoring one must restore the other.
Both sentences cite `2026-09-02-mh-stable-handler-id`.

**Final call-result mapping** (the Ok/Err half, distinct from the five-value observable label):
| outcome | label emitted | call result |
|---|---|---|
| `match` | match | `Ok` |
| `handler_id_mismatch` | handler_id_mismatch | `Ok` (non-fatal advisory — MH-CONCUR req 1) |
| `transport_mode_mismatch` | transport_mode_mismatch | `Err`, **terminal** (genuine two-ends skew; retry cannot fix) |
| `generation_mismatch` | generation_mismatch | `Err`, **retryable** (transient apply failure) |
| `no_applied_generation` | no_applied_generation | `Err`, **retryable** |
This supersedes OPS-3's original "handler_id_mismatch terminal" line: handler_id_mismatch is no
longer an `Err` at all for the interim.

**STABLE-ID FIX IS @media-handler's SPIN-OUT `2026-09-02-mh-stable-handler-id`** (MH config default +
contract, infra manifests, @global-controller for registry keying), recorded in `docs/TODO.md` at
their verdict — my diff stays zero-`mh-service`. **One half must not be lost in the handoff**, and I
am flagging it to them: @operations' OPS-18(a) point is that a stable id ALONE is insufficient while
`gc-service::assign_meeting_with_mh` returns early on an existing assignment, because MC's snapshot
then goes stale against a *genuinely* reassigned handler — a check that looks trustworthy and is not.
The spin-out's definition of done must include GC's refresh path, not only the id and registry keying.

**Q2 label tally is now 3-to-1 to DROP.** @media-handler joins @operations and my recommendation:
drop `handler_id` from `mc_media_policy_pushes_total` (unbounded per-incarnation series, worst under
crashloop), against @observability's O-6. @media-handler adds no `handler_id` label MH-side either.
The label can return once the id is stable (§11 blesses pod/handler-level then). Still @team-lead's
call between mandatory reviewers, but the weight has shifted and O-6's founding premise (the field is
pod-level) is the thing all three dissents reject.

**SEC-META (@security + @observability, co-owned guard-policy CARRIED, not authored by me).** ADR-0036
§11's "no meeting identifier on any metric anywhere in this design" is not mechanically enforced for
Rust today — `meeting_id` is in no guard vocabulary, so `counter!("mc_...", "meeting_id" => id)` passes
`metric-labels`, and THIS devloop adds the first Rust media-path metrics, so the gap is on exactly the
metrics I am adding. @security supplied the exact edit: add `"meeting_id"` to `PII_PREFIX_DENYLIST` in
`crates/dt-guard/src/common/pii_vocabulary.rs` (verified: the cited site is
`PII_PREFIX_DENYLIST: &[&str] = &["raw_"]`, single consumer `metric_labels.rs:563`, matches their
description exactly). The partition choice is theirs and load-bearing: the prefix list has ONE consumer
(`metric_labels`), so it bars `meeting_id` on Rust *metrics* without firing on the legitimate
control-plane `meeting_id = %meeting_id` *log* fields that a Cat B entry (three consumers) would break
across MC/GC/MH. Prefix runs before the `pii-safe` hatch and before `is_hashed_label()`, so
`meeting_id_hash` on a Rust metric is caught too (the §11 grandfathered exception is the TS client SDK,
structurally out of a `crates/`-only scanner's reach). Plus the test @security specified: assert
`PiiCategory::Prefix` specifically (not "a finding was raised" — a later move into Cat B would keep a
weaker test green while silently restoring suppressibility and the hashed-suffix exemption), covering
`meeting_id` and `meeting_id_hash` as label keys, and a `sender_id_hash` Cat-B-still-exempts case to
pin that only one partition changed. @security verified the tree is green for this (zero Rust metric
sites carry `meeting_id*`; the two Rust `meeting_id_hash` hits are a const array and a `/tests/`-exempt
assert-by-absence). **I carry these two hunks verbatim with the two `Approved-Cross-Boundary:` trailers
@security supplied; if anything looks wrong I push back to them rather than tidy it, since drift in
policy content is the failure the co-ownership rule exists to prevent.** @observability supplied the `label-taxonomy.md` wording verbatim (EDIT 1 replacing the R1/R2/R3
scope block, EDIT 2 the Rule Index row — both marking R1 `[guard-enforced]` for Rust metric labels
ONLY, `[reviewer-only]` for TS/client and trailing compounds), plus a `docs/TODO.md` §Observability
Debt trigger entry: the Cat-B `sender_id` term is sound only while no `sender_id_hash` spelling exists,
and if one is proposed the fix (either `is_hashed_label` machinery or a `sender_id` prefix entry) must
land first. All carried verbatim; content stored in the implementer's scratch notes. This is the
strongest enforcement available for the SEC-1/no-meeting-id property my own metrics must honour — it makes the
plan's "no meeting_id on any media metric" a guard rather than a review note, on the metrics being
added in the same commit.

**O-1 (@observability) — all three MH-owned prose sites were already in the table** (added after
@operations' OPS-5, possibly after you read it); owners corrected to media-handler for the dashboard
panel and operations for the runbook blockquote. One sentence, three homes, recognisably the same
sentence.

---

## §12 — Gate-1 amendments raised at RESUME (second confirmation round)

The respawned reviewers were asked to confirm the §11-adjudicated plan and raise only defects **not**
already covered. Six confirmed unconditionally; two confirmed conditional on a prose-only addition.
**Both conditions are ACCEPTED by @team-lead.** No design change, no code-behaviour change, and — the
part that matters for the Layer-A scope-drift guard — **no new row in the Cross-Boundary
Classification table**: every file named below already has one.

### O-8 (@observability) — ACCEPTED. The SEC-META prefix entry falsifies THREE more statements in `label-taxonomy.md`, not two.

SEC-META schedules EDIT 1 (§Enforcement reality R1 bullets) and EDIT 2 (Rule Index row). @observability
re-verified the guard (`pii_token_hit`, `crates/dt-guard/src/metric_labels.rs:556`, runs
`PII_PREFIX_DENYLIST` first and returns before `LABEL_ALLOWLIST` / `is_hashed_label()` / Category B) and
found R1's enforcement status asserted in **five** places in `docs/observability/label-taxonomy.md`, three
of which go from true to false on this same commit:

1. **§R1 blockquote (~lines 280-286)** — *"Do not reach for the vocabulary guard to enforce this … the
   entry would be defeated by the very mechanism it invokes — while reading as coverage."* Narrowly true
   of Category B, but reads as a flat prohibition. Worst case is not confusion: a later reader deletes the
   new prefix entry **as a mistake**, silently un-enforcing R1 on exactly the metrics this devloop adds.
2. **Hashed-suffix exemption blockquote (~lines 484-488)** — *"A hashed meeting id … is waved through by
   `is_hashed_label()` — exempted **by construction**."* Flatly false once the prefix check returns first.
3. **Category B `sender_id` row (~line 425)** — its contrastive argument rests on *"for `meeting_id` the
   realistic spelling is the hashed one, so a plain entry is defeated on arrival"*, which this commit
   falsifies. The row's conclusion about `sender_id` survives; its supporting contrast does not.

This is the file's own documented failure mode — prose that "reads as coverage" while the mechanism
differs — so leaving it is the exact single-source-of-truth drift CLAUDE.md bars. Three clauses in a file
already in the changeset; the deferral math does not come close. @observability supplied verbatim
EDIT 3/4/5 to @implementer, to be carried under the same `Approved-Cross-Boundary: observability` trailer
as EDIT 1/2. **`docs/observability/label-taxonomy.md` is already a Minor-judgment row owned by
observability — the row's scope widens, no new row.**

### OPS-19 (@operations) — ACCEPTED. The MH runbook's Scenario 13 blockquote promises a recovery cadence that does not exist.

Four lines above the gen-0 prose §9b already retires, `docs/runbooks/mh-incident-response.md` Scenario 13
says: *"**Interim remedy: restart the pod**, and it is recoverable rather than a second outage — the
meetings that were healthy re-assert on MC's next ADR-0036 §8 cadence tick and reinstall at their current
generation."* Verified: `crates/mc-service/src/` has **zero** re-assert path (the dozen `§8 cadence`
references are all MH-side), and this diff deliberately defers the cadence (Accepted Deferral #1). So the
runbook directs a 3am responder to restart an MH pod on a recovery mechanism that does not exist — every
live meeting on that pod loses forwarding policy permanently, with **no MC-side signal**, because the
one-shot push already happened and the gauge holds its last healthy value (OPS-1).

Pre-existing rather than introduced, but it is one clause, **inside a hunk already being edited, in a file
@operations owns** — CLAUDE.md fix-don't-defer applies squarely, and the identical "prose starts lying"
reasoning already produced the §9b edit in the same blockquote. Required content (wording flexible, both
facts mandatory): the re-assert cadence is **not yet implemented**; until it lands, restarting the pod
drops forwarding policy for every live meeting on it and they are **not** recovered automatically — treat
the restart as an outage for those meetings, re-established only by participants rejoining. Cites the same
deferral pointer §9b/§8 uses, so the caveat is retired by the cadence task rather than orphaned.
@operations' Ownership-Lens ACK of the Scenario 13 hunk is **conditional on OPS-19 landing in it**;
@media-handler has ACKed their half of the same hunk.

### OPS-20 (@operations) — ACCEPTED. The post-deploy checklist is the THIRD consumer of the `handler_id_mismatch` exclusion, and the rollback paragraph contradicts OPS-14.

(i) §11's OPS-18(ii) requires task 21's alert **and** the metrics catalog to read
`outcome!~"match|handler_id_mismatch"`. The `docs/runbooks/mc-deployment.md` post-deploy checklist is a
third consumer of that same expression and §11 does not mention it. Under OPS-18, any deploy that rolls
MH — or in which an MH pod merely restarts — produces `handler_id_mismatch` **by construction**, so a gate
written "no non-`match` outcomes" false-fails every such deploy. A checklist that cries wolf on every
rollout is one an operator learns to tick without reading, which is what matters: the same gate is what
would otherwise catch a real `no_applied_generation`. The addendum therefore carries the identical
exclusion plus one clause saying **why** (per-incarnation id, expected on any MH restart,
`2026-09-02-mh-stable-handler-id`), so it is removed when the spin-out lands rather than surviving as
unexplained cargo. **Three sites now hold one expression — alert, catalog, checklist — and the text states
plainly that they are one decision with one revert trigger.**

(ii) Same hunk, folded in rather than separate: OPS-14's "roll MH forward, not MC back" must be reachable
from the section's closing **"Rollback (MC half)"** paragraph, which today says
`kubectl rollout undo deployment/mc-service` unqualified. A responder who jumps straight to the rollback
paragraph — which is what a rollback paragraph is *for* — does the exact wrong thing: rolling MC back
returns to gen-0 registrations, i.e. the pre-change blackhole. One inline sentence carving out sustained
`no_applied_generation`, or a pointer to the bullet.

`docs/runbooks/mc-deployment.md` is already a "Mine (meeting-controller), reviewer operations" row;
@operations accepted that classification and did **not** upgrade it.

### D-12 (@dry-reviewer) — ACCEPTED, non-gating. Do-NOT-collapse the MH fixture literal, and say why at the site.

`crates/mh-test-utils/src/media_policy.rs` already houses the `EgressStream` / `RegisterMeetingRequest`
fixture literal (`egress`, `loopback_egress`, `register_request`, `TEST_MC_ID`, `TEST_MC_ENDPOINT`),
consumed at three MH sites. The @dry-8 unit assertion and the §6 `MediaHandlerStub` construct the same
literal. Ruling is **do not collapse**: `mh-test-utils` has a path dependency on `mh-service`, so importing
it from MC's dev-deps pulls `mh-service` into MC's dev graph — the identical layering inversion the DRY
INDEX already records for `env-tests/src/fixtures/media.rs` vs `mc-test-utils/src/media.rs`. The ask is a
~5-line boundary comment at MC's request-builder site in `crates/mc-service/src/grpc/mh_client.rs` (already
a "Mine" row), no code motion, and explicitly **NOT** an `ANCHOR (DRY):` — MH must accept a range of
well-formed registrations, not only what MC emits today, so their equality is not load-bearing and a false
SSoT would hide that fork.

Informational, no action owed: `media_policy::egress` pins `priority_group: 0`, so MH's loopback suite
never exercises a non-zero `AUDIO_PRIORITY_GROUP`. Coverage observation for @test / @media-handler.

### @test §5 ruling — counter-delta shape covers the env-test with NO new fixture.

@test traced `crates/env-tests/src/fixtures/metrics.rs` and the three existing `26_mh_quic.rs` consumers
and ruled the fixture layer already sufficient:
- **Positive** `mc_media_policy_pushes_total{outcome="match"}` delta >= 1 via `poll_until_any_instance_above`
  against `sum by (instance)(...)`, baseline via `instance_counter_map` — the same shape as the existing
  `assert_notification_counter_increases_past`.
- **Negative guard** expressed as `!any_instance_exceeds_baseline(...)` over
  `sum by (instance)(mc_media_policy_pushes_total{outcome!="match"})`. **Do NOT use `instance_maps_equal`
  here**: a stale old-pod non-match series expiring between the baseline and current reads makes the maps
  unequal and produces a rollover-induced false FAIL. The negated increase-predicate tolerates
  disappearing series by construction, and an absent series is an empty map reading as 0.
- **The gauge stays OUT of the env-test** — an unmaterialised series makes absent-vs-zero ambiguous and a
  snapshot can be masked by an intervening scrape. The gauge assertion lives in the component test where
  `MetricAssertion` gives deterministic absent-vs-zero.

Two test-BODY (not fixture) notes for @implementer: add an `outcome`-labelled PromQL builder mirroring
`mh_notification_counter` so reader and assert share one string source; and if any sibling test in the
binary can increment `outcome="match"`, use a `#[serial(...)]` group plus a stabilize step in the
`wait_for_notification_counter_stable` two-reads shape.

**Consequence for the Cross-Boundary table**: @test's ruling is that the
`crates/env-tests/src/fixtures/*` row's stated condition ("only if a gauge-poll helper is needed") is
**NOT met**. That row is expected to produce **zero diff**; if `src/fixtures/*` is touched, @test
re-examines it as a finding at review.

### @media-handler non-blocking flags

**A. Whole-window coherence, not one sentence.** The `mh-service.md` edit must revise the entire window
note: line ~276 ("MC does not emit >= 1 until story task 13") and line ~281 ("shape inverts at story task
13 … `no_generation` should fall to zero") both encode the same task-13-flips-it premise and will
contradict the new rollout-based OPS-5 wording if left. Same coherence check applies to the dashboard
description's "EXPECTED STEADY STATE UNTIL STORY TASK 13 … inverts at task 13" and to the runbook
blockquote. Verified at Gate 3.

**B. Latent drift, correctly OUT of scope (OPS-4), tracked not fixed.** MH's
`PolicyApplyOutcome::NoGeneration` doc comment in `crates/mh-service/src/observability/metrics.rs` says
`no_generation` is "the value the rejection will land on once task 13 turns enforcement on" — but OPS-4
decides task 13 does **not** turn enforcement on. MH-owned code; must not be touched in this
zero-`mh-service` diff. Gets a `docs/TODO.md` §Media Path Obligations note so §9c's separate MH gen-0
enforcement change corrects it.

### Gate-1 guard

`./scripts/guards/simple/validate-cross-boundary-classification.sh docs/devloop-outputs/2026-09-02-mc-media-routing-control-plane/main.md`
-> `STATUS=OK REASON=cross-boundary-classification-clean-1-files`. Re-run after these amendments (no new
rows added, so the result is unchanged).

### @implementer re-validation against the tree (resume)

Verified to still hold: RouteMedia is gone from `crates/mc-service/` (zero hits — §7 / Deferral 5
discharged); the proto contract is fully landed with the field numbers §1 assumes; the SEC-META site is
exactly as described (`pii_vocabulary.rs:364`, single consumer `metric_labels.rs:563`, first check in
`pii_token_hit`, ahead of Category A / `LABEL_ALLOWLIST` / `is_hashed_label` / the `# pii-safe` hatch, and
`lower.starts_with(prefix)` so `meeting_id` catches `meeting_id_hash`; **no Python mirror survives, so the
edit carries no drift-guard obligation**) — independently confirming @observability's O-8 ordering claim;
the S-3 choke point exists at `actors/controller.rs::remove_meeting()`; §9c holds with **zero**
`mh-service` edits (MH's gen-0 arm short-circuits, and any `policy_generation >= 1` already routes to the
real `apply_policy` path that echoes the genuinely-installed generation); and both existing MC mocks
already carry an in-code comment handing this task the job of making the echo deliberate.
`MhEndpointInfo.mh_id` from the Redis assignment snapshot is the right source for `expected_handler_id`.

Two scope notes accepted by @team-lead:
- **(A) `selection_rules` (field 5) stays `None`** — a deliberate declaration, not an omission. The proto
  distinguishes "MC declared no rules" from "MC did not speak about rules", and this story declares none.
  The reasoning goes at the construction site, not only here.
- **(B) `PolicyGenerations` must reach `MeetingControllerActor`** via
  `MeetingControllerActorHandle::new(...)`, which has 14 call sites across 5 files. All are already rows in
  the Cross-Boundary table (`main.rs`, `actors/controller.rs`, `tests/common/mod.rs`, `tests/join_tests.rs`,
  `tests/gc_integration.rs`) — `join_tests.rs` and `gc_integration.rs` simply pick up constructor fallout
  beyond the reasons already listed against them. No table change.

### Content lost in the interruption — re-supplied by owners, NOT reconstructed

§11 SEC-META recorded the verbatim guard-policy content as living in "the implementer's scratch notes".
Those notes did not survive the interruption and `/tmp/devloop/` has no copy. Missing: @observability's
`label-taxonomy.md` EDIT 1 (R1/R2/R3 scope block) and EDIT 2 (Rule Index row); @security's two
`Approved-Cross-Boundary:` trailers for the `pii_vocabulary.rs` + `metric_labels.rs` hunks; and the exact
`docs/TODO.md` §Observability Debt `sender_id_hash` trigger wording. **@team-lead ruling: re-request the
wording from its owners; do not reconstruct it from §11's summary and do not tidy it** — drift in co-owned
policy content is exactly the failure the co-ownership rule exists to prevent. Those hunks land last; if an
owner does not supply the wording it is escalated, not improvised. (EDIT 3/4/5 were re-sent unprompted by
@observability with O-8.)

### O-9 — §11 SEC-META's "non-suppressible" claim is FALSE. Corrected, not defended.

@security and @observability independently found, and @implementer restated while citing the refuting
line, that §11's *"Prefix runs before the `pii-safe` hatch"* conflates two sites:
- `crates/dt-guard/src/metric_labels.rs:556` — inside `pii_token_hit`, the prefix loop genuinely runs
  first, ahead of Category A, `LABEL_ALLOWLIST` and `is_hashed_label()`. **This half is true** and is what
  makes `meeting_id_hash` catchable.
- `crates/dt-guard/src/metric_labels.rs:932` — **finding EMISSION is a separate match**, and the arm reads
  `PiiCategory::Prefix if pii_safe.is_none()`. Only `PiiCategory::A` is unguarded. So a
  `# pii-safe: <reason>` annotation **suppresses** a Prefix finding.

**The only strength claim to be written anywhere** — in the test doc comment, in `label-taxonomy.md`, in
`docs/TODO.md`, and here: *cannot be rescued by `is_hashed_label()` or `LABEL_ALLOWLIST`; **can** be
suppressed by an explicit, diff-visible `# pii-safe: <reason>`.* Never "non-bypassable"; never an
unqualified `[guard-enforced]` tag.

**No design change follows.** @security re-derived the partition from consumer counts rather than
re-asserting it: `PII_TOKENS_CATEGORY_A` has 3 consumers, `PII_TOKENS_CATEGORY_B` has 3 — either fires on
every legitimate `meeting_id = %meeting_id` **log** field across MC/GC/MH. `PII_PREFIX_DENYLIST` has
exactly 1 consumer and is metrics-only. Prefix remains the only partition that gets the property without
collateral breakage.

Scope correction that came with it: `label-taxonomy.md` states R1's or the prefix list's enforcement status
in **seven** places, not the two the plan scheduled nor the five O-8 reported. The three further sites are
§Prefix denylist body, the Rule Index prefix row, and §What the escape hatch suppresses.

Also corrected: **no new `docs/TODO.md` §Observability Debt entry.** The `sender_id_hash` trigger already
exists as D5 (landed at story task 10); what is needed is an amendment to D4 plus a clause in D5. A second
entry would give the trigger two homes — the drift failure the entry itself warns about. And the
`sender_id_hash` pin SEC-META asked for already exists in-tree as
`metric_labels.rs::sender_id_hash_is_not_covered` and stays green under this edit — referenced, not
duplicated.

### EDIT 12 (guard Prefix finding message) — @team-lead ruling: LANDS. Content, not machinery.

With `meeting_id` in the list, the guard's finding message reads *"… has denylisted prefix `meeting_id` —
the `raw_` prefix signals an unsanitized identifier; rename or add `# pii-safe: <reason>`"*. Two defects,
created by the same commit that creates the condition: it asserts `raw_` semantics over a non-`raw_` token
(factually incoherent), and it **names the bypass as the remedy** for a bar ADR-0036 §11 states absolutely.
It does not merely fail to catch — it teaches the author how to get past it.

@observability first classified the fix as machinery and then **withdrew that**, correctly: CLAUDE.md's test
is construct shape and evaluation order versus meaning, and a `format!` literal changes none of the arm's
shape, guard condition, `rule_id`, or scan order — only the meaning the entry conveys, which the same rule
assigns to the policy owner. @security concurred as co-owner. **@team-lead: approved, fix now** — the
decisive point is that the defect ships *inside* the guard this commit adds.

@security's first candidate string was **superseded by their own analysis**: it ended `(e.g. meeting_id,
ADR-0036 §11)`, embedding a per-entry fact in a message shared by every prefix — structurally the same
defect as the `raw_` hardcoding, one entry later. @observability's version lands instead, applying the
"points, never restates" rule the edited file already states for frame-reject `reason` tokens: the message
points at `label-taxonomy.md` §Prefix denylist, where that meaning has its single home. It keeps both
load-bearing corrections and is stronger on the second — it makes the author establish legitimacy rather
than asserting a conclusion they can wave off.

Verified before landing: `metric_labels.rs:937-940` is the message's only copy in the tree (no TS mirror, no
Python remnant, so no drift-guard obligation); no test pins the string; prefix iteration order is not
load-bearing. Deliberate deviation flagged so it is not later "corrected": the string uses the **full path**
where source comments use the bare basename, because it lands in CI output read by someone who may not know
where the file lives. Caveat recorded and deliberately NOT filed: `cite_extract.rs:56` scopes citation
extraction to `docs/runbooks` and `.claude/skills`, so a `docs/…` pointer in Rust is reviewer-maintained and
rots silently on a heading rename — accepted because the alternative drifts *by construction* rather than
*on a rename*, and seven Rust files (one of them `metric_labels.rs:1033` itself) already use this idiom.
Only the general fix — making `PII_PREFIX_DENYLIST` carry per-entry guidance — stays machinery and stays
filed for infrastructure in the D4 amendment, which says in terms that the string edit does **not**
discharge it.

### EDIT 13 / O-10 (stale extension procedure) — @team-lead ruling: LANDS.

`docs/observability/label-taxonomy.md` tells authors in four places that PII-denylist extensions land via a
PR touching that file **and** `scripts/guards/simple/validate-metric-labels.sh`. That script is five lines
(`source _dt_guard_wrapper.sh metric-labels`) since ADR-0034 §3 moved the policy into `crates/dt-guard/`.
The documented procedure points at a file containing no policy.

The failure mode is not an author editing the wrong file; it is a **diligent** author adding denylist logic
to the wrapper, where it never executes — producing a vocabulary that reads as extended in the diff, passes
review because the reviewer sees the paired file the doc demanded, and enforces nothing.

@observability pitched this as "the first vocabulary extension since the flip"; **@security corrected that
and the correction is what carries the ruling**: the wrapper's last commit is `d01c99ac` (2026-05-23) and
`pii_vocabulary.rs` has been extended three times since — most recently `ba102ec8` (2026-09-02), **this
story's own task 12**, which added 133 lines of vocabulary (`meeting_kek`, `sender_id` — the very terms the
SEC-META edit sits beside) and never touched the wrapper. The procedure has been dead three and a half
months and silently violated at least three times, ours yesterday among them. Observed in-flight drift, not
hypothetical.

Two binding refinements from @security: the sibling sweep establishes these four as the **complete live
class** (every other `.md` reference — ADR-0034, ADR-0031 §101/§120, the 2026-05-14 debate docs, six devloop
outputs, two `docs/TODO.md` entries — is historical and correct); and **only three of the four are stale** —
line 22 ("Machine enforcement: … runs on every CI pipeline") is still literally true, since the wrapper IS
the CI entry point, so it needs a wrapper annotation, not repointing. A uniform four-site `sed` — how anyone
would naturally write this edit — would replace one falsehood with another.

### OPS-21 (@operations) — post-deploy gate shape

The mc-deployment gate is **positive + the non-match exclusion**, not a pure negative: a pure negative check
passes on a pod that never programmed a handler, which is the state a broken deploy produces — the same
absent-vs-zero vacuity @test used to keep the gauge out of the env-test. The hazard attached: a
positive-only gate false-fails every deploy window with no organic meetings (off-hours, staging, MC-only
revision) — OPS-20's cry-wolf failure running the other way. Resolution: the gate produces its own signal
rather than waiting for traffic — `docs/runbooks/mc-deployment.md` §Smoke Tests → Test 5 (Join Flow) already
drives a real join and ends in a metrics-verification step, so **zero becomes an instruction** ("unrun — run
Test 5, then re-check"), not a pass and not a fail. Two riders: fold
`mc_media_policy_pushes_total{outcome="match"}` into Test 5's existing Step 5 grep so producer and gate are
wired together; and keep the OPS-1 limitation prominent, because with the positive check load-bearing a
green `match` covers only meetings programmed *after* the rollout and says nothing about meetings already
live on the pod — the deployment-side twin of the OPS-19 cadence caveat.

#### EDIT 13's supporting evidence — corrected twice, by both co-owners, against the code

@security first reported the procedure "silently violated by at least three extensions", then read the
three diffs rather than their stats and narrowed it themselves:
- `889039f2` — genuine extension (`accessToken` → `CATEGORY_A`). It carries
  `Approved-Cross-Boundary: security …` and a Security RESOLVED-FIXED verdict: **the co-ownership gate
  fired.** It skipped the doc pairing and the dead wrapper, not the review.
- `021dac98` — **not an extension.** Zero net new `CATEGORY_A` members; its 22 string literals populate
  four new partition arrays over already-denylisted terms, with the invariant at `pii_vocabulary.rs:507-512`
  forcing every partition entry to be an existing `CATEGORY_A` member. Classifying existing terms into
  buckets is not extending the vocabulary — CLAUDE.md's own content-vs-machinery line.
- `ba102ec8` (task 12) — genuine extension, and it **did** pair `label-taxonomy.md`. It missed only the
  wrapper.

**Corrected tally, agreed by both co-owners and going into the record verbatim: two extensions since the
ADR-0034 flip; wrapper pairing followed 0 of 2 (dead since `d01c99ac`, never once followed); doc pairing
1 of 2; security co-ownership gate 2 of 2.** EDIT 13 rests entirely on the 0-of-2, which survives every
correction — the wrapper half is unfollowable **by construction**, and that is the whole finding. Withdrawn:
any framing that the co-ownership gate is unreliable. It fired both times.

@observability then ran a full census to check @security's own flagged-as-partial evidence, and their first
run reported `redundant` entering `CATEGORY_A` — a parsing artifact (an awk range that grabbed quoted
strings without stripping `//`, catching the term inside a `// KEEP. "Redundant under segment matching" …`
comment). Re-run with comments stripped: 22 before, 22 after, both `comm` directions empty. @security
re-derived it independently to the same result. Had the first run been sent, it would have contradicted a
correct ruling on the strength of a tooling bug.

### Scope cap (@team-lead)

The plan is **§1-§11 + §12 (O-8, OPS-19, OPS-20, OPS-21, D-12, @test's §5 ruling) + EDIT 12 + EDIT 13 + the
O-9 correction. Nothing further.** From the moment implementation started, any new finding is a **Gate-3
review finding against the diff**, not a plan amendment — plan churn is now the larger risk. Files touched
by EDIT 12/13 (`metric_labels.rs`, `label-taxonomy.md`, `docs/TODO.md`, `pii_vocabulary.rs`) all already have
Cross-Boundary rows, so **no new rows** and the Layer-A scope-drift guard is unaffected.

**Gate 1 outcome: PLAN APPROVED**, with O-8, O-9, O-10/EDIT 13, EDIT 12, OPS-19, OPS-20, OPS-21 and D-12
folded in as binding plan content, and @test's §5 ruling binding on the env-test shape.

---

## Pre-Work

None.

---

## Implementation Summary

### The mechanism, as built

MC now owns four separable pieces and the loopback is the N=1 evaluation of the first:

| Piece | Home | What it is |
|---|---|---|
| Pure assignment computation | `media_routing/assignment.rs` | meeting state → per-handler forwarding snapshot; no clock, no I/O, no Redis |
| Change-detector | `media_routing/generation.rs` | numbers those snapshots; identical output → same number |
| Total classifier | `media_routing/confirm.rs` | the handler's reply → exactly one of five bounded outcomes |
| One-shot programming call | `grpc/mh_client.rs` | carries a snapshot, confirms the echo, fails loud |

**No single-handler special case anywhere.** `grep -n "handlers.len() == 1"` over `media_routing/`
returns nothing; the same walk that produces the loopback's single self-edge produces an
N-participant, M-handler assignment. Visibility is one named predicate (`subscribes_to`), reflexive
in this story — which is exactly what makes "hear yourself" the N=1 output of the general rule rather
than a branch.

### Decisions worth naming, because they are the ones a reader would otherwise undo

- **`egress_stream_id = sender_id << 8 | ordinal`** is unique *by construction* (`sender_id` is
  per-meeting unique and never recycled; the ordinal is per-subscriber), and the ordinal width is an
  independent policy-plane 8 that merely coincides with `KEY_ID_STREAM_BITS`. The comment at the
  derivation site names both 8s and says which is anchored and which is not (D-10).
- **`NonZeroU64` for the generation**, so `policy_generation: 0` is unrepresentable rather than
  merely forbidden. Exhaustion **rejects** (`GenerationSpaceExhausted`, modelled on
  `SenderIdSpaceExhausted`) rather than saturating: saturating would give a *changed* assignment the
  *same* number, MH would honour §8 and no-op it, and MC's confirm would read `match` over a policy
  that was never installed.
- **Divergence value is `sent.abs_diff(applied)`** — an unsigned magnitude. Both rejected
  alternatives are rejected *in the doc comment* so neither returns as a "fix": `saturating_sub`
  renders `applied > sent` as a healthy 0 on the one response shape that proves the two ends
  disagree, and a signed value is silently green under any `> 0` panel threshold.
- **`McError::MediaPolicyDivergence { outcome: PolicyPushOutcome }`** carries the enum rather than
  the plan's `&'static str`. Deliberate and strictly tighter: `.label()` still yields the bounded
  `&'static str` the metric needs, and the caller's retryable-vs-terminal split needs
  `.disposition()`, which a string cannot give without a stringly re-parse. Flagged for
  @code-reviewer as a knowing deviation from §2's literal wording, not an oversight.
- **`selection_rules: None`** with the reason at the construction site: the field's *presence*
  distinguishes "MC declared no rules" from "MC did not speak about rules", and this story declares
  none.
- **Two fail-open shapes inoculated together.** Transport mode and `handler_id` are both compared
  unconditionally — no `if echoed != UNSPECIFIED` and no `if !echoed.is_empty()`. Each has its own
  test driven by the proto3-default echo specifically, because that branch is unreachable in any test
  written against a same-version peer and would otherwise be invisible to the suite permanently.
- **`handler_id_mismatch` is non-fatal AND lowest-precedence, as one decision.** The site comment
  carries (a) interim-and-reverts-only-when-the-id-is-stable, (b) why — a per-incarnation token makes
  the mismatch expected on every MH restart, so top precedence would mask a real generation fault —
  and (c) the spin-out slug `2026-09-02-mh-stable-handler-id`, and states plainly that only the abort
  is suppressed while the report (log + counter + named outcome) is not.
- **Generation is taken once per handler, in `connection.rs`, outside the retry loop.** Inside
  `MhClient` it would be recomputed per attempt; it would still return the same number today, but the
  guarantee would rest on that coincidence rather than on where the call sits.

### Where the diff is smaller than a reader might expect

`crates/mh-service/**`: **zero changes.** MH's gen-0 arm already routes any `policy_generation >= 1`
to the real apply path, so MC emitting >= 1 flips MH to real applies with no MH edit. MC deliberately
does **not** flip MH's gen-0 rejection — MC and MH roll independently, and MH-first rejects every
registration from a not-yet-upgraded MC pod.

`proto/**`: **zero changes**, as planned — this task consumes the already-landed contract. Verified
at resume that `grep -rn "RouteMedia\|route_media" crates/mc-service/` returns no hits, so the task's
mock/import cleanup item was already discharged by `2026-09-01-internal-contract-reshape`.

### Guard-policy content carried verbatim

`meeting_id` added to `PII_PREFIX_DENYLIST`, plus the `PiiCategory::Prefix`-specific test, EDIT 12's
finding-message correction, and EDIT 13's extension-procedure repointing — all supplied verbatim by
@security and @observability and carried unreworded. **One §11 claim was corrected in flight and must
not be reintroduced**: the Prefix entry is **not** non-bypassable. It cannot be rescued by
`is_hashed_label()` or `LABEL_ALLOWLIST`, but `# pii-safe: <reason>` *does* suppress it
(`metric_labels.rs:932` gates the arm on `pii_safe.is_none()`; only Category A is unguarded). Every
site says "bypassable"; none says "guard-enforced" unqualified.

### Corrections applied after reviewer ACK (not deferred)

Four defects the owners found in their own hunks, all fixed in place rather than filed:

- **@operations, blocking**: `kubectl rollout status deployment/mc-service` **does not exist** — MC
  ships as per-ordinal Deployments `mc-0`/`mc-1`; `mc-service` is a Service, a PDB and a container
  name. The failure was not a dead end but an *inversion*: `NotFound` reads as "no rollout in
  progress", flipping a responder to actionable during exactly the mixed-version window the note
  exists to protect. Replaced with the distinct-image query over `-l app=mc-service`, which answers
  the predicate the rule actually turns on ("are mixed MC versions live"), plus an explicit
  do-not-reach-for warning naming the trap.
- **@operations**: struck "drain" — there is no session-drain path for MH that preserves forwarding
  policy. The `terminationGracePeriodSeconds` settle window drains in-flight connection teardown and
  does nothing for policy, so naming it would have recreated the OPS-19 defect *inside* the OPS-19
  fix. The clause now says so explicitly.
- **@operations**: `mc-deployment.md`'s Rollback (MC half) line carried the same non-existent
  `deployment/mc-service` in its `rollout undo` command. It is pre-existing text, but my carve-out
  edit landed on that line and it is a command a responder types mid-rollback, so it is fixed to
  `mc-0`/`mc-1` per `mh-deployment.md`'s precedent. **The wider sweep is NOT done here** — 116
  occurrences across four runbooks plus 46 `deployment/ac-service` (a StatefulSet); @operations owns
  that follow-up.
- **@observability**: `metric_labels.rs:9`'s module header restated `PII_PREFIX_DENYLIST = (raw_,)`
  — an eighth site restating list membership, in the file being edited. Replaced with a **pointer**
  rather than an updated restatement, so the ninth entry cannot re-stale it. Also retired
  `pii_vocabulary.rs`'s "mirrors Wave-1 Python `PII_PREFIX_DENYLIST`" claim (`meeting_id` was never
  in that list, so the parity described does not exist), and added
  `trailing_meeting_id_compound_is_not_covered` pinning the documented `starts_with` scope limit —
  the same idiom as the existing `sender_id_hash_is_not_covered`.

- **@operations, second round — my diff made Scenario 13 contradict itself.** HUNK 2 states in bold
  that `deployment/mh-service` does not exist, and four commands in the same scenario then instruct
  that exact form. Uniformly-wrong was a sweep item; *self-contradicting* is not, because a responder
  reads a scenario as one unit and one that argues with itself gets abandoned for guesswork. Fixed
  all four in-scenario: `mh-0`/`mh-1` for the rollback history (both, since the sentence says "some
  pods"), `mh-0` for the two port-forwards, `mc-0` for the `exec` target. **The substantive half was
  not the naming**: `mh_register_meeting_timeouts_total` scraped through one port-forward is one
  pod's counter, so both port-forward sites now say the counter is per-pod, name the fleet-view
  query, and state that a quiet pod proves nothing about a loud one — a kick rate read off the wrong
  ordinal is how this gets closed as "not reproducing". The `grpcurl` address
  `mh-service.dark-tower.svc.cluster.local:50051` is **left alone and annotated**: that is a Service,
  Services genuinely are named `mh-service`, and the rename running past the `exec` target is a real
  trap in the other direction. Everything outside Scenario 13 stays with @operations' sweep.

- **@observability, trailer coverage — a gap in my own bookkeeping, found by the owner.** The eight
  recorded trailers covered the two guard files and EDIT 13's extension procedure, but **not** the
  seven `label-taxonomy.md` *content* edits (EDITs 1, 2, 2b, 3, 4, 5, 6, 7) — a row the table
  classifies Minor-judgment with observability as owner, so owner hunk-ACK is required. @observability
  caught it while verifying my "carried unreworded" claim, which is precisely the claim I had flagged
  as unverifiable by me. Ninth trailer added; the table row's parenthetical is also corrected, since
  it still read "§Enforcement reality R1 + Rule Index row" while the hunks had grown to eight
  sections. @observability confirmed all eleven sites landed and named three deviations from their
  supplied text as acceptable (a dropped "before Category A" from one scan-order list, two paragraphs
  merged into one, and one delegation replaced by the stated rule).

- **@operations, fleet-view refinements — the line I added while fixing this class had the same defect.**
  My per-pod triage comment pointed at `sum by(instance) (mh_register_meeting_timeouts_total)`. Two
  corrections, both verified before applying: (1) **use the rate** — the raw counter is cumulative
  since each pod's start, so across pods of differing uptime the values are incomparable and a
  recently-restarted pod reads *low*, making the likeliest suspect in this scenario look cleanest.
  Now `sum by(instance) (rate(mh_register_meeting_timeouts_total[5m]))`, matching the window and
  shape of the Detection query at line 786 so it reads as that query's per-pod split. (2) **name what
  `instance` looks like** — verified `infra/kubernetes/observability/prometheus-config.yaml`
  `job_name: 'mh-service'` uses `role: pod` keeping `app=mh-service` on port 8083 with **no pod-name
  relabel**, so `instance` returns `<pod-ip>:8083`. The surrounding prose says "use the AFFECTED pod
  (`mh-0` or `mh-1`)", so without the mapping the query identifies the culprit and withholds its
  name; the `kubectl get pods -l app=mh-service -o wide` mapping is now stated. Recorded as Lesson 4:
  this defect was introduced *while fixing this exact class*, in the same file, in the same hour.

- **Owner-confirmation reconciliation — Lesson 3 applied, and it found three rows worse than the one
  it was written about.** @observability asked which file a PromQL line landed in, which forced a
  second diff-vs-table reconciliation. The answer was benign (a runbook comment, @operations' row;
  grepped to confirm zero dashboard occurrences), but the sweep found **three Minor-judgment rows
  whose owners had never been sent anything**: `infra/grafana/dashboards/mc-overview.json`
  (observability), `docs/observability/metrics/mh-service.md` and
  `infra/grafana/dashboards/mh-overview.json` (media-handler + observability), plus the media-handler
  half of `docs/runbooks/mh-incident-response.md` and `crates/env-tests/tests/26_mh_quic.rs` (test).
  **@media-handler owned three edited rows and had received nothing all session**, despite supplying
  MH-CONCUR on the design at Gate 1 — every requirement of which was implemented. All rendered hunks
  have now been sent to @media-handler, @test and @observability. **The distinction that makes this
  more than bookkeeping**: `Approved-Cross-Boundary:` trailers are optional (ADR-0024 §6.7), but owner
  *confirmation* for a Minor-judgment row is required at Gate 1 and Gate 3. The missing ninth trailer
  was the mild form of this defect; three unsent owners is the severe one, from the identical root.

- **@observability withheld confirmation on `mc-overview.json` — two colour defects, both fixed.**
  Descriptions were correct; the **colour config** contradicted them, which is where a panel's
  semantics actually reach a responder's eye. (1) The red override regexp `/mismatch|no_applied_generation/`
  was **unanchored**, so it painted `handler_id_mismatch` red — the one outcome Gate 1 spent OPS-17/OPS-18
  deliberately disarming as expected-by-construction on every MH restart, and which the same panel's
  description calls a diagnostic and excludes from the alert. Alert silent, dashboard red, on an ordinary
  rolling deploy: precisely how a responder learns that red on a panel means nothing. Now anchored to
  `/^(generation_mismatch|transport_mode_mismatch|no_applied_generation)$/` with `handler_id_mismatch`
  **yellow** — visible, expected, not your incident — and the anchor also stops a sixth outcome containing
  the substring silently joining the red set. (2) The divergence stat had one green threshold step with
  `colorMode: "value"`, so a magnitude of 5 rendered as a green 5; with `noValue: "0"` already collapsing
  healthy and never-pushed into a green 0, green carried no information at all. Added `orange` at
  `value: 1` — orange not red, because the counter is the detection signal and this is only the magnitude.
  Both panel descriptions now state their colour rule, so the next reader sees the intent, not just the JSON.

- **@media-handler's cross-domain FYI — a pre-existing wrong port that my edit made self-contradictory.**
  Scenario 13's `kubectl port-forward deployment/mh-0 8080:8080` + `curl localhost:8080/metrics` targets a
  port the pod does not listen on: verified `MH_HEALTH_BIND_ADDRESS: "0.0.0.0:8083"` in
  `infra/services/mh-service/configmap.yaml` and `containerPort: 8083` in both MH deployments, so the
  command yields connection-refused rather than an empty grep. Pre-existing (I had only changed the
  Deployment name), **but I introduced the contradiction** by adding an `instance` note citing `:8083`
  three lines above it. Fixed to `8083:8083` at both port-forwards and all three `curl` sites **inside
  Scenario 13 only**, with a comment naming the config key so the port is explained rather than merely
  changed. The five identical occurrences outside Scenario 13 stay with @operations' sweep, per the
  self-contradicting-versus-uniformly-wrong boundary already agreed.

- **@operations retracted both ACKs over ports, and the second defect was theirs, in the line they
  asked me to edit.** Chasing @media-handler's 8083 flag turned up that the ports are wrong across
  both runbooks, not just the Deployment names. Verified against the manifests: MH health/metrics is
  `0.0.0.0:8083` (`configmap.yaml:15`, `containerPort: 8083`, both probes) with **no 8080 port**; MC
  health/metrics is `0.0.0.0:8081` (`configmap.yaml:13`, `containerPort: 8081`) with **no 8080 port**;
  GC genuinely is 8080, so `mc-deployment.md:795` is correct and deliberately untouched — it is almost
  certainly where the copy-paste originated. Fix 1 (Scenario 13, five lines) was already applied.
  **Fix 2 is the serious one**: `mc-deployment.md`'s Test 5 Step 5 was wrong on *both* name and port
  (`deployment/mc-service 8080:8080`), and that step is the **designated producer** for the OPS-20
  "Forwarding policy confirmed live" gate — whose own text says a zero reading means the gate is unrun
  and to go run Test 5. So the escape hatch routed to a broken tool: an operator would run Test 5, get
  connection-refused indistinguishable from "no increment", see no `outcome="match"`, and loop forever.
  Fixed to `deployment/mc-0 8081:8081`, with the config key named and the gate-producer relationship
  stated at the site. The seven remaining `mc-service 8080:8080` forwards outside Test 5 stay with
  @operations' sweep, which is now "names **and** ports" — a materially different job they are
  recording as such.

- **Gate 2 attempt 1 FAIL — Layer 5 clippy, and it is Lesson 3's shape again.** Two unused imports in
  `tests/gc_integration.rs` (`TokenReceiver`, `watch`) left by the `TokenReceiver` hoist, which removed
  their only consumer. **My clippy runs were scoped `-p mc-service -p mc-test-utils -p dt-guard -p env-tests`
  and predated that edit**; the workspace-wide `--all-targets` run caught it. A scoped check standing in
  for the full one — the same substitution as reconciling against the thread instead of the diff. Fixed,
  and re-verified with `cargo clippy --workspace --all-targets` (exit 0), which is what Layer 5 actually
  runs rather than an approximation of it.

### Gate-3 review findings — all fixed in-diff, none deferred

Seven findings across five reviewers. Each was a real defect; the pattern in six of them is the one
Lesson 4 names.

- **O-11 (@observability)** — `record_register_meeting` keyed `"success"` off `outcome == Match`,
  while `disposition()` in the same file says `handler_id_mismatch` is `Programmed` and the call
  returns `Ok`. So every ordinary MH pod restart recorded a **failure** on the pre-existing
  `mc_register_meeting_total{status}` — a binary success/error series with **no yellow, no caveat and
  no `outcome!~` escape available** — reintroducing three dashboard rows above the panel where
  OPS-17/OPS-18 had just disarmed it. @operations independently confirmed the consequence: that
  counter feeds the 95% deploy gate and a P2 upgrade trigger, both of which would have false-failed on
  every MH rollout. Now keyed on `disposition() == Programmed`, which is exactly what
  `status="success"` means, so the two cannot drift. **Regression test added and verified to fail
  against the original mapping** — neither existing test could distinguish the two, which is why it
  survived review.
- **SEC-5 (@security)** — `PolicyGenerations` was evicted at `remove_meeting()` but **not** at
  `check_meeting_health()`, the second teardown path, which runs every controller-loop iteration and
  whose clean-exit arm logs "Meeting actor exited cleanly" — an ordinary end-of-meeting, not the panic
  case. `meeting_removed()` fired on both paths; the registries observed one. Fixed on both, including
  the pre-existing `mh_connection_registry` sibling in the same hunk per the same-owner-sibling rule.
  Regression test drives the private reaper directly (no public seam exists to end one meeting actor
  except through the path that was already correct — which is precisely why the existing eviction test
  stayed green with the bug present).
- **@operations** — a terminal failure logged `"RegisterMeeting retries exhausted"` with
  `total_attempts = 3`. Both false: the code deliberately fails once. Worse, `mc-incident-response.md`
  Scenario 12 keys on that literal string and reads it as flaky coordination — investigate the
  transport — while a terminal `transport_mode_mismatch` needs the opposite remedy (roll MH forward,
  never MC back). A log asserting a retry history that never happened, routing the responder to the
  wrong branch under incident pressure. Split into two messages, both `error!` on the same target, so
  the existing runbook string still matches the case it was written for.
- **@test** — the terminal-disposition `break` had **zero** coverage: all three existing loop tests
  drive `McError::Grpc`, which is non-terminal, so deleting `if terminal { break }` left the suite
  green. `confirm.rs` pinned what `disposition()` *returns*; nothing pinned that the loop *acts* on it.
  Both arms now pinned by divergence inputs. Also wired the dead `MediaHandlerStub::call_count()` to a
  real assertion (MhClient owns no retry loop).
- **@code-reviewer** — `#[allow(clippy::cast_precision_loss)]` where ADR-0002 requires
  `#[expect(..., reason = ...)]`. Fixed the required site plus the three same-file siblings they
  invited; `#[expect]` is strictly better here because it fails if the lint stops firing.
- **D-13 (@dry-reviewer)** — the sharpest. `MAIN_AUDIO_STREAM_NUMBER`'s doc claimed its bound "**is**
  anchored" to `KEY_ID_STREAM_BITS` and D-10's whole content is "these two 8s have opposite drift
  obligations" — but there was no import and no assert anywhere in `mc-service`. Prose only, which
  made D-10 decorative and, per the INDEX's own instrument test, a false SSoT is worse than none
  because it stops the next reader checking. Added the real `const _` assert in the same shape as
  `sender_id.rs`'s and `mh-service`'s; both D-10 sentences are now true as written.
  **The class, per @dry-reviewer, and it is not "the comment was wrong":** the comment was *right
  about the concept* — the two 8s genuinely do have opposite drift obligations, and that reasoning was
  correct. It described an assert **that had not been written yet**. That is far more common and far
  harder to catch than an incorrect claim, because everything around it reads as careful: the
  distinction is drawn, the rationale is sound, the register is precise, and the only missing thing is
  the mechanism the prose presupposes. Reviewing for *wrongness* does not find it; only checking
  whether the named mechanism exists does.
- **D-14/D-15/D-16 (@dry-reviewer)** — the `TokenReceiver` hoist left two more copies inside `src/`,
  one in this diff's primary file; both removed and `token.rs`'s "three" corrected to "five". D-15's
  note records why the adjacent `loopback_assignment` genuinely *cannot* delegate (rlib vs `--test`
  builds are distinct crate instances, so `mc-service`'s own types do not unify — which is also why
  `test_token_receiver` can, returning a third-crate type). D-16's extraction is **deferred, owner-declined,
  and the operative reason is the artifact not the process**. I offered @test the extraction; they
  declined as owner because the right helper must generalise **both** stability-waits — mine and the
  pre-existing `wait_for_notification_counter_stable` behind tests 4 and 5, which is outside task 13
  entirely — so a second consumer alone does not determine the interface. A task-13-shaped helper
  would be *worse than the documented clone*: a shared home its two siblings do not both use, which
  reads as done and stops the next reader looking. @dry-reviewer amended their own `docs/TODO.md`
  entry on this ruling, correcting "two call sites updated" (itself the partial hoist @test warns
  about) and re-recording the state as **owner-declined rather than implementer-deferred** — different
  states for whoever picks it up, since the first says the routing already happened and the fix shape
  is settled. My earlier ownership-based reason (fixtures scoped to zero diff) was the weaker one and
  is retained only as secondary: it would have been satisfied by @test saying yes; the shape argument
  would not. The load-bearing 16s scrape-interval rationale is carried onto the new copy either way,
  and the env-test's own doc comment now leads with the shape reason so a future extractor hits it
  first.

**One process note against myself.** My first attempt to verify the SEC-5 regression test used an
unasserted string replace to revert the fix; `cargo fmt` had wrapped the call across lines, so the
revert silently matched nothing and the test "passed" against a bug that was still fixed. The test
*also* had a real flaw — it seeded and probed `PolicyGenerations` with equal assignments, and
`next_generation` returns the same number for an identical assignment, so that assertion could not
fail either way. Both found by re-checking rather than by trusting the green. Every
revert-verification in this diff now asserts that the revert applied.

- **@operations corrected my Scenario 12 prose in their own hunk, and the defect was the same class the
  hunk exists to fix.** My terminal bullet asserted `attempts_made = 1`. That holds only when the
  terminal outcome lands on the *first* attempt — a transport error on attempt 1 followed by a
  `transport_mode_mismatch` on attempt 2 reports 2, which is reachable and is exactly the confusing
  case where the discriminator most needs to be reliable. A responder who had internalised
  "terminal means 1" and saw 2 would conclude they were in the retryable class: the wrong branch, from
  a runbook line written to route them to the right one. Corrected in place by @operations to say
  `attempts_made` is whichever attempt hit the terminal outcome and that **`terminal = true` is the
  discriminator, never the attempt count**. Verified: no `attempts_made = 1` claim survives in the
  file. They also ruled the exhaustion arm keeps `total_attempts` alongside `attempts_made` — always
  equal on that path, so redundant rather than wrong, and it keeps existing parsers working.

- **Gate 2 attempt 2 — pre-commit hook BLOCKED on a clippy error Layer 5 structurally cannot see, and
  the lint was a symptom of a worse defect.** `.githooks/pre-commit` runs `cargo clippy --all-targets
  **--all-features**`; `scripts/lang/rust/lint.sh` omits `--all-features`, and `crates/env-tests`
  declares no default features — so every target in that crate is invisible to the pipeline, and my own
  `cargo clippy --workspace --all-targets` reported 0 findings for the same reason. Filed as a
  Gate-3 follow-up (owner infrastructure, consumer test): **the authority gate is weaker than a local
  developer hook**.
  **The lint was not the real defect.** `empty_line_after_doc_comments` at `26_mh_quic.rs:995` was a
  dangling `///` — but it dangled because my Scenario 6 insertion had **severed the R-33 stub test's
  20-line doc comment from its test**. The doc had reattached itself to `policy_push_promql`, the stub
  was left with a one-line pointer, and its body comment "See doc-comment above" pointed at nothing.
  Deleting the stray `///` — the literal instruction, and the whole of what the compiler complained
  about — would have silenced the error and left a function documented by an explanation of a
  different test. Fixed properly: doc reunited with its test, the `Scenario 6 (R-33 #6)` banner
  restored, my section renumbered to 9 (8 was taken), and the module-doc scenario list corrected to
  match. Re-ran the hook's exact command, `cargo clippy --all-targets --all-features -- -D warnings`
  → exit 0.
  **Third instance of this loop's dominant class**, and the most literal: an artifact whose surrounding
  prose read as careful while the mechanism it presupposed was absent — here, a doc comment attached to
  the wrong item, where the compiler's complaint pointed at the whitespace rather than at the
  misattachment. Reviewing the reported error would not have found it; reading what the doc was
  attached to did.

### Verification run during implementation

| Check | Result |
|---|---|
| `cargo check -p mc-service --all-targets` | clean |
| `cargo test -p mc-service -p mc-test-utils` | 340 lib + all integration suites pass, 0 failed |
| `cargo test -p dt-guard --lib` | 429 pass |
| `cargo clippy` (mc-service, mc-test-utils, dt-guard, env-tests, all targets) | clean |
| `validate-metric-labels.sh` | `STATUS=OK` (with the new `meeting_id` entry) |
| `validate-metric-coverage.sh` | `STATUS=OK` |
| `validate-dashboard-panels.sh` | `STATUS=OK` |
| `validate-todo-tracking.sh` | `STATUS=OK` |
| `validate-knowledge-index.sh` | `STATUS=OK` |
| `validate-cross-boundary-scope.sh` | `STATUS=OK` |
| `validate-cross-boundary-classification.sh` | `STATUS=OK` |

---

## Files Modified

### New — MC media-routing control plane
- `crates/mc-service/src/media_routing/mod.rs`
- `crates/mc-service/src/media_routing/assignment.rs` — visibility graph, per-egress behaviours, 12 unit tests
- `crates/mc-service/src/media_routing/generation.rs` — `PolicyGenerations`, 7 unit tests
- `crates/mc-service/src/media_routing/confirm.rs` — 5 outcomes, 8 confirm tests (5 single-condition + 3 precedence-boundary, with the non-fatal `Ok` folded into the fourth) + magnitude tests

### Modified — MC service
- `crates/mc-service/src/lib.rs` — register `media_routing`
- `crates/mc-service/src/errors.rs` — `MediaPolicyDivergence` + its three match arms
- `crates/mc-service/src/grpc/mh_client.rs` — `MeetingProgramming`, request builder (+ D-12 boundary note), `confirm()`, O-3 all-operands `error!`
- `crates/mc-service/src/grpc/mod.rs` — export `MeetingProgramming`
- `crates/mc-service/src/observability/metrics.rs` — `record_media_policy_push`
- `crates/mc-service/src/webtransport/connection.rs` — `build_routing_input`, the R-12 trigger, retryable-vs-terminal split
- `crates/mc-service/src/webtransport/server.rs` — thread `PolicyGenerations`
- `crates/mc-service/src/actors/controller.rs` — eviction at the `remove_meeting()` choke point
- `crates/mc-service/src/main.rs` — construct and share `PolicyGenerations`

### Modified/new — tests
- `crates/mc-service/tests/media_policy_push_integration.rs` (new) — 8 component tests over a real gRPC round trip
- `crates/mc-service/tests/register_meeting_integration.rs` — hoisted stub; the error case is now "accepted but nothing applied"
- `crates/mc-service/tests/otel_grpc_outbound_integration.rs` — hoisted stub wrapped in the capture interceptor; local `MockMh` and token helper deleted
- `crates/mc-service/tests/common/mod.rs` — mock records `expected_handler_id`, `policy_generation`, `egress_stream_count`
- `crates/mc-service/tests/common/accept_loop_rig.rs`, `tests/join_tests.rs`, `tests/gc_integration.rs` — constructor fallout; third `TokenReceiver` copy removed
- `crates/mc-test-utils/src/mock_mh.rs` — `MediaHandlerStub` with the applied-generation echo knobs
- `crates/mc-test-utils/src/media.rs` — loopback policy fixtures built via MC's real `compute_assignment`
- `crates/mc-test-utils/src/token.rs` (new), `src/lib.rs`, `Cargo.toml`
- `crates/env-tests/tests/26_mh_quic.rs` — live-handler policy-push confirm (positive counter delta + negated-increase negative guard)

### Modified — guard policy (verbatim, co-owned)
- `crates/dt-guard/src/common/pii_vocabulary.rs`
- `crates/dt-guard/src/metric_labels.rs`

### Modified — docs and infra
- `docs/observability/metrics/mc-service.md` — both catalog entries
- `docs/observability/label-taxonomy.md` — EDIT 1, 2, 2b, 3, 4, 5, 6, 7, 13 (four sites; `:22` annotate-only)
- `docs/observability/metrics/mh-service.md`, `infra/grafana/dashboards/mh-overview.json`, `docs/runbooks/mh-incident-response.md` — the window prose at all three MH-owned sites, plus OPS-19's restart caveat
- `docs/runbooks/mc-deployment.md` — OPS-20 gates, the Test 5 producer wiring, OPS-14's inline rollback carve-out
- `infra/grafana/dashboards/mc-overview.json` — Media Routing row + two panels
- `docs/TODO.md` — S-4 (A+B), EDIT 10 (D4), EDIT 11 (D5), and three new §Media Path Obligations entries
- `docs/specialist-knowledge/meeting-controller/INDEX.md`

---

## Approved-Cross-Boundary Trailers (for the commit)

Supplied by their owners; carried verbatim.

```
Approved-Cross-Boundary: security meeting_id in PII_PREFIX_DENYLIST enforces ADR-0036 §11 on Rust metric labels; Prefix partition is deliberate — sole consumer is metric_labels, so control-plane meeting_id log fields stay legal
Approved-Cross-Boundary: security test pins PiiCategory::Prefix specifically so a later move to CATEGORY_B cannot keep a weaker assertion green while silently restoring the hashed-suffix and LABEL_ALLOWLIST exemptions
Approved-Cross-Boundary: security prefix finding message must not assert raw_ semantics over a non-raw_ token, nor name # pii-safe as the remedy for a bar ADR-0036 §11 states absolutely; per-entry meaning stays in label-taxonomy.md
Approved-Cross-Boundary: observability finding-message wording is denylist policy meaning per CLAUDE.md guard-crate ownership; cites label-taxonomy.md by heading so per-entry semantics have a single home
Approved-Cross-Boundary: security extension procedure for a security-co-owned denylist must name the file that holds the policy; directing an author at a five-line wrapper invites denylist logic that never executes and reads as enforced
Approved-Cross-Boundary: observability extension procedure is taxonomy-doc content; repointing the paired file to pii_vocabulary.rs matches where the vocabulary has actually lived since ADR-0034 wave 2
Approved-Cross-Boundary: observability meeting_id in PII_PREFIX_DENYLIST makes ADR-0036 §11 R1 a guard on Rust metric labels; prefix partition is chosen because it scans before is_hashed_label() and keeps the bar off control-plane meeting_id log fields
Approved-Cross-Boundary: observability test pins PiiCategory::Prefix specifically so a later move to CATEGORY_B cannot silently restore the hashed-suffix exemption and LABEL_ALLOWLIST; KNOWN LIMIT records the pii-safe bypass rather than overclaiming
Approved-Cross-Boundary: observability label-taxonomy R1 enforcement status, prefix-denylist section, Category B sender_id row, escape-hatch bullet and both Rule Index rows are the taxonomy owner's wording; every statement is [guard-enforced, bypassable], never unqualified
```

**Gap CLOSED.** ADR-0024 §6.4's intersection rule needs both owners on the two SEC-META hunks;
@observability supplied their two after reviewing the hunks (they disclosed reading exactly those two
diffs and nothing else, and independently verified the consumer-count claim the partition argument
rests on: Cat A = `rust_log_secrets` + `instrument_skip_all` + `metric_labels`; Cat B = `rust_pii` +
`ts_pii` + `metric_labels`; Prefix = `metric_labels` alone). All nine trailers are now held.

**On the other Minor-judgment rows** (`crates/env-tests/tests/26_mh_quic.rs` → test; the two runbook
hunks → operations; the three MH-owned prose sites → media-handler): ADR-0024 §6.7 makes the trailer
**optional**; what §6.3 *requires* for a Minor-judgment row is owner **confirmation at Gate 1 and at
Gate 3**, and each of those owners gave both — recorded in their Ownership Lens verdicts above. The
trailers below are the durable audit breadcrumb for the guard-policy hunks specifically, where the
co-ownership intersection rule made a written record worth having.

---

## Devloop Verification Steps

Gate 2 runs `./scripts/layer-all.sh` with `DEVLOOP_FAIL_FAST=0` (headless = unattended caller, so ALL
seven layers are evaluated and no layer renders `NOT-RUN`).

### Attempt 1 — `TOTAL_RESULT=FAIL` (Layer 5)

```
LAYER=1 RESULT=OK    DURATION=38    buf-build, cargo-build, dt-guard, dt-story, nx-typecheck
LAYER=2 RESULT=OK    DURATION=2     buf-format, cargo-fmt, nx-format
LAYER=3 RESULT=OK    DURATION=49    guards-passed + every guard self-test
LAYER=4 RESULT=N/A   DURATION=187   cargo-test PASSED, nx-test PASSED; aggregate N/A
LAYER=5 RESULT=FAIL  DURATION=9     buf-lint OK, nx-lint OK, cargo-clippy FAILED
LAYER=6 RESULT=N/A   DURATION=1     cargo-audit OK, pnpm-audit SKIPPED-NO-DIFF, buf-breaking OK
LAYER=7 RESULT=OK    DURATION=668   env-tests-passed, browser-e2e-passed
TOTAL_DURATION=954 TOTAL_RESULT=FAIL
```

Layers 4 and 6 are the **documented self-justifying aggregate cases**, not gaps: proto registers
intentional-gap placeholders (`REASON=not-applicable-to-this-lang`) for `test`/`audit`, and Layer 6's
`pnpm audit` applies its own dep-manifest gate (`SKIPPED-NO-DIFF no-dep-changes`). Every real leg passed —
including the new env-test against the live Kind cluster and the browser E2E.

**The failure**, and the only thing between the diff and Gate 3:

```
error: unused import: `common::token_manager::TokenReceiver`
  --> crates/mc-service/tests/gc_integration.rs:22:5
error: unused import: `watch`
  --> crates/mc-service/tests/gc_integration.rs:31:25
error: could not compile `mc-service` (test "gc_integration") due to 2 previous errors
```

Leftovers from the `TokenReceiver` hoist: the local `mock_token_receiver` body was removed but not the two
imports it was the sole consumer of. **A `FAIL` (exit 1), so it consumes attempt 1 of 3** — implementer
lane, not the operator lane. Notable as the same class as Lesson 3: the implementer's own scoped
`cargo clippy` reported clean, and the workspace-wide `--all-targets` run in Layer 5 is what caught it — a
narrower check standing in for the full one.

Bundled into the same fix round: @observability's two withheld-confirmation defects on
`infra/grafana/dashboards/mc-overview.json` (see §Code Review Results).

### Attempts 2-4 — `TOTAL_RESULT=N/A`, exit 0 — **PASS**

The pipeline was re-run after each round that changed the tree (Gate-3 review fixes; then the
pre-commit clippy fix), because a verdict computed against a tree that no longer exists is not a
verdict. All three runs are identical in shape; attempt 4 is the one the commit rests on
(`LAYER=1..7` = OK/OK/OK/N/A/OK/N/A/OK, `TOTAL_DURATION=765`, zero
`FAIL`/`PRECONDITION_FAILURE`/`UNKNOWN`).

### Attempt 2 — `TOTAL_RESULT=N/A`, exit 0 — **PASS**

```
LAYER=1 RESULT=OK   DURATION=26
LAYER=2 RESULT=OK   DURATION=2
LAYER=3 RESULT=OK   DURATION=49
LAYER=4 RESULT=N/A  DURATION=164
LAYER=5 RESULT=OK   DURATION=11
LAYER=6 RESULT=N/A  DURATION=2
LAYER=7 RESULT=OK   DURATION=498
TOTAL_DURATION=752 TOTAL_RESULT=N/A
```

`grep -cE "STATUS=(FAIL|PRECONDITION_FAILURE|UNKNOWN)"` over the run log returns **0**. Every real leg
passed: `cargo-test-passed`, `nx-test-passed`, `cargo-clippy-passed`, `guards-passed`,
`cargo-audit-passed`, `buf-breaking-passed`, `env-tests-passed`, `browser-e2e-passed`. The two `N/A`
aggregates are proto's registered intentional-gap placeholders
(`REASON=not-applicable-to-this-lang` for `test` and `audit`) plus Layer 6's own dep-manifest gate
(`SKIPPED-NO-DIFF no-dep-changes`) — the self-justifying set in ADR-0033 §6, not a `FAIL-MISSING-VERB` and
not a `NOT-RUN`. `TOTAL_RESULT=N/A` is worst-child aggregation over those placeholders, and the wrapper
exits 0.

**Gate 2 verdict: PASS.** Fixed in this round: the two orphaned imports in `tests/gc_integration.rs`;
@observability's two `mc-overview.json` colour defects; and the five-plus-one port defects @media-handler
and @operations surfaced (MH health/metrics is `:8083`, MC is `:8081`, neither pod has an 8080 listener;
GC genuinely is 8080 and was correctly left untouched).

---

## Code Review Results

### Gate 3 verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-DEFERRED | 1 | 1 | 0 in-diff | SEC-5 fixed and independently re-verified by the reviewer. Verdict is DEFERRED on the strength of §Accepted Deferrals, not on anything unresolved. |
| Test | RESOLVED-DEFERRED | 2 | 2 | 1 (D-16, owned) | Terminal-disposition `break` had zero coverage; proven non-vacuous by deletion. Dead accessor wired. |
| Observability | RESOLVED-DEFERRED | 10 | 10 | 1 (D5 exit condition) | O-8, O-9, O-10, O-11, stale module header, Wave-1 parity claim, missing trailing-compound test, missing owner trailer, two dashboard colour defects. |
| Code Quality | RESOLVED-FIXED | 2 | 2 | 0 | ADR-0002 `#[allow]`→`#[expect(reason)]`; O-11 shared finding. |
| DRY | RESOLVED-DEFERRED | 4 | 3 | 1 (D-16, owner-declined) | D-13 false anchor, D-14 two surviving `TokenReceiver` copies, D-15 non-collapse note. |
| Operations | RESOLVED-FIXED | 6 | 6 | 0 | OPS-19, OPS-20, two workload-name defects, the metrics-port class, and the retry-exhausted log. |
| Semantic Guard | CLEAR | 0 | — | — | All five applicable checks clear. |
| Media Handler | CLEAR | 0 | — | — | Contract fidelity verified against real MH source, not the plan. |

**No ESCALATED verdicts. Gate 3 passes.** Four reviewers landed on RESOLVED-DEFERRED, so accepted
deferrals exist and are enumerated in §Accepted Deferrals below — see the note there on why that is the
honest verdict rather than RESOLVED-FIXED.

### The substantive result, beyond the individual findings

Two independent reviewers arrived at the same conclusion from different lanes, and it is the durable
output of this loop:

**A change that alters what an outcome *means* has a blast radius over every existing consumer of that
meaning, and the consumers do not announce themselves.** (@observability) Four instances here: the stale
`label-taxonomy.md` sites, the stale `metric_labels.rs` module header, the stale MH window prose, and
O-11 — `record_register_meeting` booking `status="error"` for a push the same file's `disposition()`
classifies as `Programmed`.

**Prose is an executable claim about the system with no executor.** (@operations) Five of their six
defects were in prose — a runbook promising a re-assert cadence that does not exist, a deploy gate that
would false-fail on every MH rollout, `kubectl` commands naming a workload that does not exist, and
port-forwards to ports nothing listens on. **None was reachable by clippy, `cargo fmt`, the 430 dt-guard
checks, or any pipeline layer — all green throughout.** Two were introduced *while fixing the others*,
one by the implementer and one by @operations. The highest-consequence one broke the designated signal
producer for the OPS-20 gate, making it permanently **unrunnable rather than red** — an operator gets
connection-refused, reads it as "no increment", and retries forever. A red gate gets escalated; an
unrunnable one gets retried. And it surfaced only because @media-handler asked a question they explicitly
labelled non-gating: **a control that depends on an optional question being asked is not a control.**

A related class @dry-reviewer named from D-13, worth keeping separate because it defeats a different
review habit: the comment was *right about the concept* — the two adjacent 8s genuinely do have opposite
drift obligations — and merely described a `const _` assert that had never been written. Reviewing for
*wrongness* does not find this, because everything around it reads as careful. Checking "is this claim
true" and checking "does the named mechanism exist" are different acts. The two dashboard defects have
the same shape: correct descriptions, contradicting colour config.

### Attribution, corrected twice by @operations against their own draft

Every defect of the implementer's was found by someone else, including the fleet-view PromQL they
introduced *while fixing that very class* and the `attempts_made = 1` clause inside the hunk written to
stop a responder taking the wrong branch. O-11 — the highest-consequence item — was found by
**@observability and @code-reviewer independently in the same round**, not by the implementer; @operations
initially credited the implementer by inference, did not verify it, and corrected the record. What the
implementer contributed there was declining a legitimate deferral, which is judgment rather than
discovery. **The control that functioned in this loop was review, not authorship** — which is precisely
why @operations filed a guard candidate rather than a lesson.

**Why four reviewers landed on RESOLVED-DEFERRED rather than RESOLVED-FIXED.** @security and
@observability each had RESOLVED-FIXED available on a defensible reading and rejected it. @security's
reason is the one to keep: two of the accepted deferrals are controls the ADR-0036 contract *specifies*
and this diff ships **present but not load-bearing** — misroute detection is disarmed while `handler_id`
is a per-incarnation token, and the ratchet wedge goes live with this commit while detection is
first-push-only. The protocol wants that cost visible at the gate rather than averaged away by the fixes.
@observability put the general form best: taking the paperwork-satisfying reading in a devloop whose
whole lesson was records standing in for the work would have been the wrong note to end on.

---

## Accepted Deferrals

Pointer bullets only — each deferral's body lives in `docs/TODO.md`, so the debt list has one home and no stale second copy.

- Handler-restart recovery in full (re-assert cadence, connectivity-loss trigger, dispatch jitter, cadence-vs-provisional-timeout startup validation, re-assert-failure paging, `process_start_epoch_ms` restart detection) — explicitly scoped out by the task text; made additive by the `PolicyGenerations` registry. Tracked in `docs/TODO.md` §Media Path Obligations under the `2026-09-02-mh-stable-handler-id` and MC-restart entries, and named in ADR-0036 §8.
- Misroute detection is present as an observable but disarmed as a control for the interim (`handler_id_mismatch` non-fatal + lowest precedence) → `docs/TODO.md` §Media Path Obligations, "`handler_id` is a per-INCARNATION token".
- The `policy_generation` ratchet wedge goes LIVE as of this diff; detection is real but first-push-only → `docs/TODO.md`, the `service.write.mh` ownership entry (amended in place with the landed trigger).
- MC restart resets `policy_generation` to 1 for a surviving meeting; closure trigger is "before the re-assert cadence lands" → `docs/TODO.md` §Media Path Obligations.
- MH's `PolicyApplyOutcome::NoGeneration` doc comment is stale by one clause; corrected with the MH gen-0 enforcement change, not here → `docs/TODO.md` §Media Path Obligations.
- `RouteMedia` mock/import cleanup — verified already satisfied by `2026-09-01-internal-contract-reshape`; nothing to do, recorded so the task item is visibly discharged rather than silently dropped.
- D4's vocabulary gap stays open despite the `meeting_id` prefix entry (bypassable, `starts_with`, `crates/`-only) → `docs/TODO.md` D4, amended in place.
- D-16, the env-test stability-wait clone → `docs/TODO.md` §Cross-Service Duplication, owner **test**, recorded there as owner-declined (not implementer-deferred) with the fix shape, mirrored in the site comment.
- The `sender_id_hash_is_not_covered` exit condition → `docs/TODO.md` D5, as a DEFINITION OF DONE clause.
- The `TokenReceiver` fixture octuplicate → `docs/TODO.md` §Cross-Service Duplication, owner auth-controller + security (root enabler is a GSA file).


---

## Gate-3 follow-ups needing owners (not deferrals of findings in this diff)

Pointers only; bodies live in `docs/TODO.md` where noted.

1. A Layer-3 guard for runbook commands as executable claims → `docs/TODO.md` §Infrastructure Validation in Devloops. Owner infrastructure, operations on policy content.
2. The runbook sweep, "names AND ports" → `docs/TODO.md` §Documentation Hygiene. Owner operations.
3. `.claude/skills/devloop/review-protocol.md` §Ownership Lens should say to reconcile owner-ACKs against the diff, not the thread. Shared review machinery — needs an owner assigned; not filed to `docs/TODO.md` because it is a process-doc change, not tech debt.
4. For `/close-story`: fold a `dry-reviewer` INDEX heuristic — *when a comment claims an anchor, check that the mechanism exists, not just that the claim is true.* Must fold, not append (file is at its 75-line cap and has no Cross-Boundary row).
5. MH's counterpart dashboard panel has no semantic colouring (`overrides: []`), so the two ends of one handshake now use divergent conventions. @media-handler's, out of cap.

---

## Rollback Procedure

1. Start commit: `511e14fdffbb4b6ddf1d8c313815915c5dbeb22b`
2. `git diff 511e14fd..HEAD`
3. `git reset --soft 511e14fd` (preserve) or `--hard` (discard)
4. No schema changes; no infra manifests expected.

---

## Issues Encountered & Resolutions

**The session was interrupted at the end of planning and resumed.** Loop State was authoritative: the plan
and every Gate-1 amendment survived in this file, no code existed, `HEAD` was still the Start Commit. The
roster was respawned and the loop resumed at Gate 1 rather than restarting from setup. The scratch notes
holding @security's and @observability's verbatim guard-policy content did **not** survive; the ruling was
to re-request that content from its owners rather than reconstruct it from §11's summary. Re-deriving it
is what surfaced O-8, O-9 and O-10 — see Lesson 3.

**Gate 2 attempt 1 failed at Layer 5** on two imports orphaned by the `TokenReceiver` hoist (§Devloop
Verification Steps). The implementer's own package-scoped `cargo clippy` had reported clean; the
workspace-wide `--all-targets` run Layer 5 actually executes is what caught it.

**A first Gate-2 run was discarded rather than trusted.** It was launched before the post-ACK corrections
landed, so its verdict described a tree that no longer existed. It was allowed to finish only because it
warmed the Layer-7 cluster and image cache; the authoritative verdict came from a clean re-run. A
green-looking result against a stale tree is exactly the masked failure CLAUDE.md bars.

**The pre-commit hook blocked the commit on a clippy error Gate 2 structurally cannot see.**
`.githooks/pre-commit:24` runs `cargo clippy --all-targets **--all-features**`; `scripts/lang/rust/lint.sh:7`
runs `cargo clippy --workspace --all-targets` with **no `--all-features`**. `env-tests` declares no default
features (`Cargo.toml:9-15`), so `26_mh_quic.rs` is never linted by Layer 5 — **the authority gate is
strictly weaker than a developer's local hook**, and such a lint either blocks at the last step after
passing everything meant to catch it, or reaches CI unlinted where no hook runs. Filed as a Gate-3
follow-up, owner infrastructure.

The reported error was `empty line after doc comment` — and the stray `///` was a symptom, not the defect.
The implementer's Scenario-6 insertion had used a line *inside* a 20-line doc block as its anchor, severing
that doc from the R-33 stub test it documented: the explanation had reattached to `policy_push_promql`, the
stub was left with a one-line pointer, its body comment "See doc-comment above" pointed at that pointer,
and the `Scenario 6 (R-33 #6)` banner had been consumed. **Deleting the stray `///` — the literal
instruction, and the entirety of what the compiler objected to — would have produced a green build with all
four defects intact.** Fixed properly instead. This is the third and most literal instance of the loop's
dominant class: the compiler pointed at whitespace; the defect was attachment.

**Two worthless tests were caught by their own author before shipping**: a SEC-5 regression test that
seeded and probed `PolicyGenerations` with *equal* assignments (which return the same generation, so the
assertion could not fail), and a revert-to-verify whose unasserted string replace `cargo fmt` had
invalidated — so it "passed" against a fix that had never been removed. Every revert-verification in this
diff now asserts the revert applied. @security independently found that the SEC-5 regression test's two
halves were not both pinned, reverted each half separately with asserted anchors, and confirmed each
fails on its own.

---

## Lessons Learned

**1. A mechanism consulted carelessly reads as a mechanism checked — and a TOOLING error is the dangerous
form.** Four confidently-stated falsehoods surfaced during planning, none of which reached the commit:
(i) §11's "Prefix runs before the `pii-safe` hatch", refuted by `metric_labels.rs:932`; (ii) @security's
replacement guard string, which hardcoded `meeting_id` into a message shared by every prefix — the exact
defect it was fixing; (iii) "first vocabulary extension since the flip", refuted by `ba102ec8`, landed the
previous day on this branch by this story's own task 12; (iv) a census reporting `redundant` entering
`CATEGORY_A`, an artifact of an awk range that did not strip `//` comments. The first three are reasoning
errors a careful reader catches. The fourth is worse in kind: a tooling error manufactures a confident,
specific, plausible-looking artifact that *survives* careful reading. Only the implausibility of
"redundant" as a credential token stopped it — luck, not method. **Rule adopted: when a check's output
would contradict a peer's finding, re-derive it by a second method before sending.** All four were caught
that way; none by re-reading the original claim.

**2. A citation to the mechanism is not a check of the mechanism, and precise line numbers make an
unverified claim look verified.** `docs/observability/label-taxonomy.md:439` has read
`### Prefix denylist [guard-enforced, bypassable]` since the guard was written. §11 SEC-META asserted the
opposite while citing `pii_vocabulary.rs` and `metric_labels.rs:563` **by line number** — which is exactly
what made it read as verified. Three agents passed over it. A contradiction sitting in the tree, detectable
by diffing two in-tree sources, is a worse failure mode than an undocumented gap.

**3. Reconcile owner-ACKs against the DIFF, never against the THREAD — a record of the work will stand in
for the work, and this failure has no natural detector.** I tracked `Approved-Cross-Boundary:` trailers
against the *messages I received* rather than the *hunks I wrote*. @observability sent EDIT 12 and EDIT 13
with trailers attached, so those were recorded; EDITs 1-7 arrived as content without trailers, and from an
inbox **content-without-a-trailer is indistinguishable from content-that-needs-no-trailer**. Seven
`label-taxonomy.md` content hunks on a Minor-judgment row therefore shipped un-ACKed, and the owner found
it by reading the file — not by re-reading the thread, which looked complete from both ends. The same root
produced a second defect failing in the *opposite* direction: the table row still read
`(§Enforcement reality R1 + Rule Index row)`, its Gate-1 scope, while the hunks had grown to eight
sections — a row understating its own scope makes an owner's ACK cover less than the reader believes.
**This is the same shape as the near-misses in lessons 1-2, one level up**: there, a citation stood in for
the mechanism; here, the exchange stood in for the artifact. It also generalises past trailers to any
ownership bookkeeping — reviewer sign-off, deferral registers, verification checklists. **And past
bookkeeping entirely, to verification**: Gate 2 attempt 1 failed on two unused imports because my
clippy runs were package-scoped and predated the edit that orphaned them, so a *scoped* check stood in
for the full one exactly as a *thread* stood in for the diff. Same substitution, different artifact.
Run the command the gate runs, not a proxy for it.
**Where this rule belongs, per @observability, and it is not here**: a devloop record is read at
Gate 3 and then by almost nobody, whereas `.claude/skills/devloop/review-protocol.md` is read by every
reviewer on every devloop. Its §Fix-or-Defer and §Ownership Lens sections say *what* to ACK; nothing
says to reconcile ACKs against the diff rather than the thread. Deliberately NOT edited here — shared
review machinery, neither the implementer's row nor observability's, and changing it mid-devloop is
the drive-by both declined all thread. **Gate-3 follow-up, owner needed.** Recording the pointer
because a lesson titled "a record of the work is not the work", left only in a record, is the shape
it describes.

**4. The runbook defects in this diff were one failure mode, and it is the same one.** Every operational
correction this devloop made was prose that *described* a mechanism which does not exist: a re-assert
cadence promised as recovery when none is implemented (OPS-19); `kubectl rollout status
deployment/mc-service` naming a workload that is a Service, a PDB and a container name, whose `NotFound`
inverts to "no rollout in progress" during exactly the mixed-version window the note protects; a gen-0
window note that "starts lying the moment MC emits >= 1"; a module header restating a list's membership
for the eighth time; a doc comment claiming parity with a retired Python list `meeting_id` was never in;
and a fleet-view query I *added in response to a finding* that used a cumulative counter across pods of
differing uptime, so the recently-restarted pod — the likeliest suspect — would have read cleanest.
**The last one is the instructive one**: it was introduced while fixing this exact class, by me, in the
same file, in the same hour. Prose describing a mechanism has no compiler and no guard; the only control
is an owner who checks the mechanism. **Corollary adopted: prefer removing a restatement to refreshing
it** — a pointer cannot go stale, and refreshing site N invites site N+1.

**And the control for this class is a human reading prose against the manifests, which is fragile —
name it rather than rely on it.** Two defects of identical shape landed one gate apart, by two
different agents: my fleet-view PromQL using a cumulative counter across pods of differing uptime, and
@operations' Test 5 port-forward to a port nothing listens on — the latter inside the very step their
own OPS-20 gate designates as its producer, so the gate's escape hatch routed to a broken tool and
would have looped an operator forever. **Neither was reachable by any automated control in this
repo**: not `cargo fmt`, not clippy, not the 430 dt-guard checks, not the seven-layer pipeline, which
passed Layers 1-3 and 7 green over both. The first was caught because a reviewer checked a query they
had just prompted; the second only because @media-handler asked a question they explicitly labelled
non-gating. A control that depends on an optional question being asked is not a control. The
generalisable form: **runbook commands are executable claims about the cluster with no executor** —
every `kubectl`, port and endpoint in a runbook is an assertion that nothing in CI evaluates, and the
only cheap mechanical check available today is grepping them against `infra/**` manifests. Worth a
guard; filed as a Gate-3 follow-up rather than built here.

**5. Do not assert a change is done before it is written — and re-verify an absence immediately before
asserting it.** Two halves of one hazard, and this loop hit both. *My half*: I told @operations I had
"taken the runbook half" of their finding in a message sent **before** I made the edit. It was a
statement of intent phrased as a report of completion, and the gap was real, not instantaneous.
*Their half*: they grepped for that edit, correctly found nothing, and began composing a message about
a completion claim that had not landed — then re-checked before sending and found both hunks present.
They had grepped mid-write. **Only their re-check stopped a false accusation, and only their catch
stopped my false claim from standing.** The two failure modes are symmetric and they compound: in a
model where several agents edit one tree concurrently, "verify before believing a report" and "a
concurrent tree is not a stable snapshot" pull against each other, and the resolution is narrow —
**re-verify immediately before asserting an absence, and never report completion in the same message
that plans it.** The second rule is the cheaper one and it is entirely on the author. Recorded at
@operations' suggestion, as a hazard of the review model rather than of this diff.

**The reproducible half, per @dry-reviewer, and the reason it is stated narrowly.** Three times this
loop someone re-opened a statement after committing to it and found it wrong — their own TODO entry's
fix shape, @operations' `attempts_made` clause and their near-miss framing, my two tests that could
not fail. It is tempting to call that willingness the finding of the review. **It is not, because it
is not reproducible**: nothing in the loop caused it and nothing would cause it next time. What *is*
reproducible is the narrower trigger under two of the three: **re-check your own artifact after the
state it described has changed.** @dry-reviewer's verdict was written against a diff that then moved;
my site comment was written against a ruling that @test then superseded. Both are nameable events with
an obvious action, where "be willing to be wrong" is a disposition with neither. Carry the trigger to
the retro, not the disposition.

**6. A resumed devloop should re-verify load-bearing claims against source rather than trust the record's
own citations.** O-8, O-9 and O-10 were all found while *reconstructing* content the interruption
destroyed. Losing the scratch notes forced re-derivation from the tree instead of a re-read of a summary,
and the re-derivation is what exposed the contradiction. Not an argument for losing notes — an argument for
treating a resume's re-verification as mandatory rather than as recovery overhead.
