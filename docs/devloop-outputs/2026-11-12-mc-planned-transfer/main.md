# Devloop Output: MC-F: Planned transfer (make-before-break on a draining handler)

**Date**: 2026-11-12
**Task**: Story 4 task MC-F (run-story task #24): planned transfer make-before-break on a draining MH (ADR-0039 §4/§7)
**Specialist**: meeting-controller
**Mode**: Agent Teams (v2) — full, Gate-1 present; --paired-with=test; conditional reviewer media-handler <!-- panel mode + Gate-1 tier (ADR-0037 §D2); see the Tier row in Loop State -->
**Branch**: `feature/mh-failure-transfer`
**Duration**: ~2 days wall-clock incl. escalation + operator fix; final Gate 2 2619s

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `ca97bbe635fe22308c74797ddecb97e6a0eeb4b4` |
| Branch | `feature/mh-failure-transfer` |
| Lead Model | `claude-opus-5-5` |

---

## Loop State (Internal)

<!-- This section is maintained by the Lead for state recovery after interruption. -->
<!-- Do not edit manually - the Lead updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `spawned` |
| Implementing Specialist | `meeting-controller` |
| Tier | `full` |
| Iteration | `2` |
| Security | `spawned` |
| Test (paired-test) | `spawned` |
| Observability | `spawned` |
| Code Quality | `spawned` |
| DRY | `spawned` |
| Operations | `spawned` |
| Semantic Guard | `spawned` |
| Media Handler (conditional) | `media-handler` |
| Infrastructure (co-review mh_pool.rs) | `infrastructure` |

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
Story 4 task MC-F: when a media handler announces DRAINING, MC moves each of its meetings off it make-before-break (ADR-0039 §4, §7; R-3 MC side).

### Scope
- **Service(s)**: mc-service (primary), mh-service (one boot gauge), env-tests, Grafana dashboards, mc-service manifests
- **Schema**: No
- **Cross-cutting**: Yes (MC↔MH drain interplay)

### Debate Decision
NOT NEEDED - governed by ADR-0039 (debate `docs/debates/2026-10-28-mh-replacement-and-migration/`).

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
| `crates/mc-service/src/media_routing/replacement.rs` (planned owe, transfer bookkeeping, eligibility + cutover pure decisions) | Mine | — |
| `crates/mc-service/src/media_routing/planned_transfer.rs` (new; pure: deadline latch/tighten/clamp, target predicate, terminal outcome) | Mine | — |
| `crates/mc-service/src/media_routing/mod.rs` | Mine | — |
| `crates/mc-service/src/actors/meeting/replacement.rs` | Mine | — |
| `crates/mc-service/src/actors/meeting/health.rs` (Draining transition starts the transfer; planned removal path) | Mine | — |
| `crates/mc-service/src/actors/meeting.rs` (cutover wake arm in `select!`, check after dispatch) | Mine | — |
| `crates/mc-service/src/actors/meeting_media.rs` (routable/on-target read over connectivity, non-draining member count) | Mine | — |
| `crates/mc-service/src/config.rs` (`MC_PLANNED_TRANSFER_MARGIN_MS`) | Mine | — |
| `crates/mc-service/src/main.rs` (boot config gauge) | Mine | — |
| `crates/mc-service/src/observability/metrics.rs` (margin gauge; planned_transfer zero-init for declarations/detection/removal_releases; flipped assertion) | Mine | — |
| `crates/mc-service/tests/planned_transfer_integration.rs` (new) | Mine | — |
| `crates/mc-service/tests/common/mod.rs` (draining fake-MH heartbeat helper, config fixture) | Mine | — |
| `crates/mc-service/tests/common/accept_loop_rig.rs` (server gains the margin) | Mine | — |
| `crates/mc-service/tests/*.rs` (config fixture gains the new key where configs are built literally) | Mine | — |
| `docs/observability/metrics/mc-service.md` | Not mine, Minor-judgment | observability |
| `docs/debates/2026-10-28-mh-replacement-and-migration/telemetry-contract.md` (migrations_total row: planned per-transfer terminal unit) | Not mine, Minor-judgment | observability |
| `infra/grafana/dashboards/mc-media.json` | Not mine, Minor-judgment | observability |
| `infra/services/mc-service/config.env` | Not mine, Minor-judgment | infrastructure |
| `infra/services/mc-service/mc-0-deployment.yaml` | Not mine, Mechanical | infrastructure |
| `infra/services/mc-service/mc-1-deployment.yaml` | Not mine, Mechanical | infrastructure |
| `docs/runbooks/mc-deployment.md` | Not mine, Minor-judgment | operations |
| `crates/env-tests/tests/40_resilience.rs` | Not mine, Domain-judgment (paired) | test |
| `crates/env-tests/src/fixtures/metrics.rs` (margin/grace ceiling readers, if needed) | Not mine, Minor-judgment | test |
| `crates/env-tests/src/fixtures/mh_pool.rs` (no-target leg: forced baseline drain + asserted baseline restore; mh_pool contract change, co-review infrastructure + security) | Not mine, Domain-judgment (paired) | test (co-review: infrastructure, security) |
| `crates/env-tests/tests/02_mc_deployment_config.rs` | Not mine, Minor-judgment | test |
| `crates/mc-service/src/actors/meeting/planned_transfer.rs` (new; actor driver) | Mine | — |
| `crates/mc-service/src/webtransport/server.rs` (margin into deps + boot gauge) | Mine | — |
| `crates/mc-service/src/media_routing/teardown.rs` (quiesce-timeout backstop comment; media-handler Gate 3 F2) | Mine | — |
| `crates/env-tests/src/fixtures/kube.rs` (shared `blocking` helper; dry-reviewer F3, written by paired-test) | Not mine, Domain-judgment (paired) | test |
| `docs/specialist-knowledge/media-handler/INDEX.md` (drain-window gauge pointer) | Not mine, Mechanical | media-handler |
| `crates/mc-service/src/grpc/gc_client.rs` (test Config literals gain the key) | Mine | — |
| `crates/mc-service/tests/common/replacement_rig.rs` (new; MC-C rig moved here, shared by both suites) | Mine | — |
| `crates/mc-service/tests/handler_replacement_integration.rs` (uses the shared rig) | Mine | — |
| `crates/mh-service/src/observability/metrics.rs` (`publish_drain_window`, O9) | Not mine, Domain-judgment | media-handler |
| `crates/mh-service/src/main.rs` (boot call, O9) | Not mine, Domain-judgment | media-handler |
| `crates/mh-service/tests/owner_silence_integration.rs` (ADR-0032 test for the gauge) | Not mine, Domain-judgment | media-handler |
| `docs/observability/metrics/mh-service.md` (`mh_drain_window_seconds`) | Not mine, Domain-judgment | media-handler |
| `infra/grafana/dashboards/mh-media.json` (Planned Drain Window panel) | Not mine, Domain-judgment | media-handler |
| `docs/debates/2026-10-28-mh-replacement-and-migration/mh-sh-contract.txt` (§8(d) forced-baseline-drain amendment; written by paired-test) | Not mine, Domain-judgment | infrastructure |
| `docs/specialist-knowledge/test/INDEX.md` (written by paired-test) | Not mine, Domain-judgment (paired) | test |
| `infra/grafana/dashboards/mc-media-handlers.json` (new; story-4 handler rows split out of mc-media.json) | Not mine, Minor-judgment | observability |
| `infra/grafana/kustomization.yaml` (new generator) | Not mine, Minor-judgment | infrastructure |
| `infra/grafana/deployment.yaml` (projected-volume source, R-21) | Not mine, Minor-judgment | infrastructure |
| `docs/observability/dashboards.md` (inventory) | Not mine, Minor-judgment | observability |
| `docs/specialist-knowledge/observability/INDEX.md` (board list) | Not mine, Mechanical | observability |
| `docs/user-stories/2026-10-31-survive-handler-loss.md` (MC board heading) | Not mine, Mechanical | observability |
| `docs/specialist-knowledge/meeting-controller/INDEX.md` | Mine | — |
| `docs/devloop-outputs/2026-11-12-mc-planned-transfer/main.md` | Mine | — |

------|----------------|---------------------|

---

## Planning

### Mechanism restatement
Instance: "when an MH drains, move its meetings". Mechanism: **a one-way health transition (Draining) on a current member owes that member's departure under a deadline; the departure runs the ONE removal transaction only once a target-coverage predicate holds or the deadline fires.** The wider class this exposes, and which is covered here:
- the drained member can be Active **or Registering** (a grant that starts draining). The same rule applies to both, and a Registering member has no clients, so the predicate decides;
- several members can drain at once (`mh.sh upgrade`, node maintenance). There is one transfer per draining member, and one in-flight Acquire per meeting is shared (MC-C);
- a **draining member must not count toward the meeting's target size**. Without this, MC-C's `on_removal` accounting under-owes when the grant dies mid-transfer. That fix goes in the pure state, not as an actor special case.
No other-domain instance exists. No MH edits are needed (MH-3 already supplies DRAINING, a relative `drain_remaining_ms`, early exit at zero meetings, and EndMeeting(PLANNED_TRANSFER) → Removal).

### P1. Trigger (S-3, O5, MH-7, S-2)
- Gate: the transfer starts only in `act_on_health` on the HealthStep transition `!previous.is_draining() && next.is_draining()`. That step exists only for a heartbeat that passed `MeetingHandlers::heartbeat_member` (owner check + current member; removed/never-in-set → RELEASED, no state touched) AND was `Acceptance::Accepted` (`HandlerLiveness::observe`; a Stale heartbeat mutates nothing, so it cannot set `serving_state`).
- `draining_since` is sticky in `assess`, so DRAINING→SERVING is ignored and the transfer continues to completion. Repeated DRAINING beats are not transitions, so no second transfer starts. This is also guarded idempotently: the transfer map is keyed by handler_id and a second start is a no-op.
- X stays in the set, keeps its push worker and keeps routing. Nothing is removed on the transition.

### P2. Deadline (S-1, O1, O2, MH-1)
- `cutover_at = receipt + min(drain_remaining_ms, ceiling) − margin`, with `receipt = liveness.last_accepted`, the gRPC-arrival stamp of the beat that caused the transition. It is computed from `Instant` + `Duration` with saturating subtraction. When `remaining ≤ margin` the result is `cutover_at = receipt`, which means immediate cutover with outcome timed_out (if anyone is not on a target), with no panic.
- Every later accepted DRAINING beat recomputes from its own receipt and takes `min`, so the deadline only tightens and never extends.
- Ceiling (S-1c): `drain_remaining_ms` is clamped to MC-C's config-derived `ReplacementTiming::retry_budget()` (139 s at defaults; already published as `mc_media_handler_replacement_retry_budget_seconds`). No new constant. Rationale: past the budget the acquisition ladder has made every shaped attempt, so a longer hold buys nothing. A u32::MAX value is tested to give a bounded hold.
- Pure function in `media_routing/planned_transfer.rs`, table-tested: remaining=0, remaining<margin, remaining=margin, u32::MAX, and a later larger value not extending the deadline.

### P3. Target and predicate (S-4, O4, MH-5, DRY-3)
- Eligible targets for X's transfer = **Active** members ≠ X that are not draining and not removed. Dead/Impaired members are already removed in the same turn, so they never appear. Registering grants are not eligible until Active (S-6: clients have no token for them, so nobody can be on them).
- Routable participant = an admitted participant (`views`) that holds at least one live connection key on any member. A participant that has not connected media anywhere is not waited on.
- Predicate `covered(X)` = eligible set non-empty AND every routable participant has a live key on some eligible member. It reads the ONE connectivity home (`connectivity.keys_on`) through a sibling of `reconnected_or_gone` that takes a set. No new per-participant map.
- **Revised at Gate 1 (paired-test Issue 1; option (i), follow the task text).** While the transfer has seen no "no grant" answer, the eligible target set is the grant only, once it is Active. Survivors join the eligible set only after the planned acquisition answers no grant: RESOURCE_EXHAUSTED, GC unavailable, Rejected, or refused at the attempt bound. So a {X,S} meeting with a grant goes {X,S} → {X,S,Y} → {S,Y}. That is make-before-break on Y, never a removal just because S already covers everyone. Once survivors are eligible they stay eligible, so a later grant only adds to the target. Tier-1b pins both arms: everyone on S with a grant available waits for Y; everyone on S after no_capacity removes immediately.
- **At the cutover deadline**, the target is the Active grant ∪ the non-draining Active survivors, whatever the acquisition state (in flight, or held by the interval). X is removed onto that union. The meeting is shed only if the union is empty, which is task #6: shed only with neither. **(Superseded at Gate 3 by observability's ruling on code-review F6 / operations F1:)** the outcome is decided by the coverage OBSERVED at removal, not by the deadline firing. `completed` means a non-empty target with every routable participant on it; `timed_out` means stragglers, or a shed. The deadline shows on the log's `cutover=deadline`. Tier-1b arm: Acquire never answers, S is present and participants are on S, so at `cutover_at` X is removed onto S as `completed` (`timed_out` unobserved), with no shed and no move into awaiting-handler. It is paired with arm (c), no survivor and no grant, where the shed is expected.
- (superseded wording) One rule covers both cases. With a grant Y, Y becomes eligible on Active, so the {X} meeting waits for Connected on Y. With no grant, the survivors are the target. A meeting where everyone is already on survivor S completes immediately (test-gates arm a).
- If Y or S starts draining mid-transfer, it drops out of X's eligible set at once and its own transfer starts. The shed case is reached only via the deadline with an empty eligible set (below), never via an early removal: the predicate requires a non-empty eligible set.

### P4. Acquisition (DRY-1, O3, S-2)
- `ReplacementState::on_draining(X, now, timing)` pushes an explicit `Owed{PlannedTransfer, replaced: Some(X), declared_at: receipt}` (owe-while-present, not a `members_after` hack). It then runs through the existing `on_replacement_due` / `on_acquire_result` / `end_attempt`: one in-flight Acquire, the per-meeting minimum interval, `within_attempt_bound`, the ladder and superseded handling are all unchanged. `AcquireCall.trigger = PlannedTransfer`, `replaced = X` (S-7: GC does not count it as failure evidence).
- `on_removal`'s target accounting counts **non-draining members only** (the caller passes that count). This is the pure-state answer to DRY-2:
  - after cutover with a grant: {Y} meets target, so nothing is owed;
  - after cutover with no grant: the planned owe is still owed or in flight and counts, so there is no second owe;
  - when grant Y dies mid-transfer with X draining: the count excludes X, so a replacement is owed.
- The two timers are independent arms. The ladder keeps attempting until a grant or the bound. Cutover fires at `cutover_at` regardless of the acquisition state, and the target at that instant decides between a survivor/grant cutover and a shed. After cutover the still-owed planned acquisition continues, bounded, as a size restore.

### P5. Removal (DRY-2, S-5, MH-2/3/4, semantic-guard 1/3/4)
- Cutover = `remove_handler_for(X, RemovalTrigger::PlannedTransfer, StartReason::PlannedTransfer)`, the ONE transaction, unchanged:
  1. removed-for-life record + version bump, so from here heartbeats get RELEASED and the binding gate refuses X;
  2. re-render, so edges move;
  3. the set without X goes to clients over the typed `ParticipantMessage::HandlerSet` path (reason `change`);
  4. flush;
  5. bulk binding release;
  6. `spawn_removal_release`: `quiesce` the worker (cancel + await in the spawned task, never on the actor), then `EndMeeting(X, PLANNED_TRANSFER)` once, never retried, after the worker has stopped. This means no RegisterMeeting to X can follow the EndMeeting;
  7. `after_handler_removed`.
- Disconnects from X after the push hit the removed-handler binding gate (`handler_not_in_set`). X's keys were purged in step 1, so a directed-removal close is never a leave or a loss.
- A lost EndMeeting means X's next DRAINING beat gets RELEASED(PLANNED_TRANSFER). This is tested; it is what lets MH exit early.
- **Shed (MH-8 choice): MC sheds at the cutover** through the same transaction when the eligible set is empty (no survivor and no Active grant). Outcome timed_out. The meeting then enters the existing awaiting-handler state, and the planned owe keeps acquiring. Until then X keeps serving.
- Wake: one `select!` arm `sleep_until(min cutover_at)`, built per iteration like the other arms. The predicate is re-checked after every `dispatch` (connect/leave/heartbeat), after `after_registration` (grant Active) and after an Acquire result. The actor never awaits GC, MH or a JoinHandle.

### P6. Counting (observability ruling 1, semantic-guard 5)
I adopt observability's proposal:
- **Per planned transfer: exactly one terminal outcome**, from {completed, timed_out, abandoned}:
  - completed = `covered(X)` held at removal;
  - timed_out = cut over at the deadline with stragglers, including shed;
  - abandoned = the meeting ended, or X was declared Dead/Impaired first.
- Per failed attempt, as in MC-C: no_capacity, gc_unavailable, admission_refused, rate_limited, abandoned(superseded/refused).
- A planned grant reaching Active records **no** `completed`. This is decided from the pure state's data (`Granted::records_on_active`, per code-reviewer 2). After the transfer's terminal, the same planned owe records only failure outcomes. **Invariant (corrected during implementation, observability-approved): `completed + timed_out == declarations_total{trigger="planned_transfer"}`** (one of the two plus one planned declaration per cut-over transfer). `abandoned` also carries MC-C's attempt-level discards (superseded/refused/unusable grant), so `completed + timed_out + abandoned == transfers started` is NOT exact and is not claimed anywhere. Transfer terminals go through ONE recorder, `end_transfer`, after `PlannedTransfers::finish` hands the entry out once; attempt outcomes stay on `end_attempt`.
- X Dead/Impaired mid-transfer: the crash path removes X (declaration{heartbeat_timeout|apply_lag}; MC-E's loss snapshot will fire there, correctly) and the transfer ends `abandoned{planned_transfer}`. The planned owe is not duplicated (accounting), so there is no second acquire. No `completed` for the transfer and no crash migration owe in addition.
- Meeting end mid-transfer: `end_replacement` → abandoned for the transfer plus any open attempt.
- `declarations_total{planned_transfer}` and `detection_seconds{planned_transfer}` are recorded at removal (drain receipt → removal; catalog says "transfer duration" for this trigger). `removal_releases_total{planned_transfer}` comes from the release task (already trigger-labelled). All three are zero-initialised now, and the "MC-F's to zero-initialise" negative assertion flips to present_at_zero.
- Phases: replacement (drain receipt → grant) and registration (grant → ack) are observed on the grant path through the existing code. reconnect (Active → admitted all on Y) is observed if it closes before removal. The survivor path observes no phase.
- Loss metric: it does not exist yet (MC-E, task 25). The planned removal never enters `act_on_health`'s declaration branch, which is the site MC-E will snapshot from. Test now: declarations{heartbeat_timeout, apply_lag} and detection_seconds for those triggers stay **unobserved** across a planned transfer, paired with a positive crash arm. A hand-off note goes in main.md so MC-E adds the real unobserved assertion.

### P7. Config (S-8, O6, O2, obs-6, MH-9)
- `MC_PLANNED_TRANSFER_MARGIN_MS`: REQUIRED (MissingEnvVar), parsed via `bounded` with `MIN_=1000`/`MAX_=30000` consts, so 0, 999 and 30001 are rejected, while 1000 and 30000 are accepted. No code default.
- `config.env` = 5000, with a comment relating it to MH's deadline. Required `configMapKeyRef` in mc-0/mc-1.
- Gauge `mc_media_handler_planned_transfer_margin_seconds{key_custody}` set at boot from `Config`, the same value the actor reads.
- Panel, catalog entry and ADR-0032 test.
- Runbook Configuration Reference row: default, range, gauge, and the relation "MC cuts over at SIGTERM + (grace − MH 5 s) − margin; a margin ≥ the MH window degrades to immediate cutover, outcome timed_out; the margin must cover set push + worker drain + EndMeeting RTT". It also goes in the REQUIRED-key rollout bullet and the config-echo checklist, plus a rollback note (O7): pre-MC-F MC lets a draining MH shed at its deadline, which is then handled on the crash path.

### P8. Logs (obs-7, O8, S-7, semantic-guard 2)
INFO `mc.actor.meeting`:
- "Planned handler transfer started": meeting_id, old_handler_id, `cutover_in_ms`, `remaining_ms`, `margin_ms`.
- "Planned handler transfer ended": meeting_id, old_handler_id, new_handler_id ("" for survivor), outcome, `target=grant|survivor|none`, stragglers, elapsed_ms.
No Debug of any struct, set, grant or token; the transfer state holds no token or key.

### P9. Tier-1b tests (paired-test 1-8)
These go in a new `tests/planned_transfer_integration.rs` (paused time, ScriptedGc, fake MHs):
- ordering on observed messages;
- X kept and routed while draining;
- deadline −1 ms / at deadline (timed_out) plus the no-straggler arm (completed, timed_out unobserved);
- no-grant arms a/b/c × {RESOURCE_EXHAUSTED, UNAVAILABLE};
- EndMeeting failure → removal completes, the next beat is RELEASED;
- binding-gate refusal for X after removal;
- duplicate DRAINING and DRAINING→SERVING flip;
- X Dead mid-transfer (abandoned + heartbeat_timeout declaration, one Acquire);
- grant draining mid-transfer;
- two members draining;
- remaining=0 and u32::MAX;
- config bounds and the gauge.
Pure tables go in `planned_transfer.rs`/`replacement.rs`.

### P10. Env-tests (paired with test)
- Leg A: with a target, as paired-test sketched.
- Leg B: without a target. I endorse test-gates' "drain without up, then restore": a meeting on {b0,b1} (B=2, two handlers per meeting), `--force` drain b1, so b0 is excluded and the pool is empty, giving GC RESOURCE_EXHAUSTED with survivor b0. Then an asserted `up --owner baseline` restore. Non-vacuity: GC no_capacity rose / no new ref appears.
- Ceilings come from the margin gauge and the live pod's terminationGracePeriodSeconds.
The fixture contract change is paired-test's call (infrastructure looped in if mh_pool's baseline verdict changes).

### P11. Gate-1 additions
- **O9 (operations), margin/MH-window drift guard.** Add an env-test assertion, in `02_mc_deployment_config.rs` (paired-test placement call), that `MC_PLANNED_TRANSFER_MARGIN_MS < (MH terminationGracePeriodSeconds − MH SHUTDOWN_MARGIN_SECONDS) × 1000 − one heartbeat interval`.
  - Both sides are read from the deployed artifacts: the ConfigMap and gauge on the MC side, and the live MH pod spec on the other. No literals.
  - Positive control: the check fails if either value was not read.
  - The runbook row names this assertion as the guard.
  - Leg B's forced baseline drain must restore the baseline in the fixture's drop/cleanup path, even when the leg fails (paired-test's fixture).
- **Security residual (recorded per security Gate 1).** A forged heartbeat with DRAINING and `drain_remaining_ms=0` sets `cutover_at = receipt`. On a single-handler meeting whose Acquire has not yet been granted, that is an immediate shed, before any Acquire result.
  - Class: the same as ADR-0039 S6/S1b, where the MC↔MH heartbeat is unauthenticated plaintext.
  - Bounded by: the member-and-Accepted gate, the one-way per-handler transition, the per-meeting attempt bound, and GC not counting `planned_transfer` as failure evidence.
  - Closed by: authenticated MH identity / mTLS on the heartbeat path.
- **Observability.** The catalog's planned_transfer counting paragraph will state that after a shed-at-cutover (`timed_out`), the planned owe keeps acquiring and a later grant records nothing; recovery shows on `mc_media_meetings_without_handlers`. The "transfer ended" log `outcome` uses `MigrationOutcome::label()`, the same token as the metric label.
- **DRY.** `reconnected_or_gone` delegates to the one set-taking connectivity predicate. "Draining" is read only through `HandlerHealth::is_draining`. The eligible-target set and the non-draining count passed to `on_removal` derive from one `MeetingMedia` filter; any deliberate difference (Registering counted toward the target but not eligible as a target) is commented.
- **media-handler (quiesce timeout).** A `RegisterMeeting` from an aborted worker can land after the `EndMeeting`. MH self-heals: its beat is due immediately, X's removed-for-life record answers it RELEASED (the member check runs before liveness, so `applied_generation` cannot change the answer), and MH releases the meeting. A one-line comment at the `quiesce_timed_out` site in `release_removed` will point to this backstop.
- **code-reviewer Gate 1 (all 10 adopted):**
  1. The terminal outcome is `MigrationOutcome`; there is no second vocabulary.
  2. `on_active` keeps its signature. A new `Granted::records_on_active()` decides from `owed.trigger`.
  3. `members_after` is renamed `counted_members_after`, and its doc comment is updated. The count comes from ONE `MeetingMedia::non_draining_member_count()` that EVERY `on_removal` caller uses: crash, Registering-refused and planned. Table test: a crash while another member drains.
  4. A `PlannedTransfers` struct in `planned_transfer.rs` holds the per-handler deadline, started_at and terminal-recorded flag. The actor holds it next to `Replacement`. `ReplacementState` gains only `on_draining`.
  5. ONE post-turn hook, `check_planned_transfers(now)`, sits beside `check_reconnects` in `dispatch` and the other post-arm sites, through one helper.
  6. The cutover arm uses the per-iteration `Option<Instant>` pattern. The deadline handler always consumes the entry, including when X was already removed. Test: X Dead plus the deadline in the same turn leaves the entry gone and nothing re-fires.
  7. New accessor `ReplacementTiming::max_planned_hold()` → `retry_budget()`, which documents the coupling.
  8. A `start_planned_transfer(...)` helper is extracted from `act_on_health`.
  9. `new_handler_id` is emitted only when there is a grant; `target` carries the rest.
  10. The bounds use `pub const MIN_/MAX_PLANNED_TRANSFER_MARGIN_MS`, and the tests use both the consts and the literals 999/1000/30000/30001.
- **paired-test Gate 1:** Issue 1 is resolved by (i) above. Issue 2: a Tier-1b mixed-scenario assertion of the per-transfer invariant (as corrected in P6: completed + timed_out == planned declarations, including a deterministic superseded-Acquire discard), with each terminal recorded exactly once (X Dead mid-transfer records no completed/timed_out; a post-cutover restore grant records no completed). Issue 3: paired-test owns the leg-B scaffold (asserted restore on every exit path, precondition, non-vacuity). `mh_pool.rs` gets infrastructure and security as co-reviewers.
- **Hand-off to MC-E.** MC-E must add the ADR-0032 unobserved assertion on `loss_clients_total`/`loss_prior_report_ratio`/`loss_client_lead_seconds` for a planned transfer. Its snapshot site is `act_on_health`'s declaration branch, which the planned removal does not enter.

---

### Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test (paired) | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Semantic Guard | confirmed |
| Media Handler (conditional) | confirmed |

Classification-sanity guard: `STATUS=OK`. Lead rulings: (1) planned transfers record exactly one terminal outcome each (completed/timed_out/abandoned) at removal; planned grant Active records nothing; (2) MC removes the old handler at its own cutover deadline onto grant+survivors, shedding only when that set is empty; (3) infrastructure added as Gate-3 co-reviewer for `crates/env-tests/src/fixtures/mh_pool.rs` (forced baseline drain contract change), alongside security.

## Pre-Work

None.

---

## Implementation Summary

- **MC planned transfer** (ADR-0039 §4/§7): pure decisions in `media_routing/planned_transfer.rs` (deadline latch/tighten/clamp to `retry_budget`, eligibility, outcome); actor driver `actors/meeting/planned_transfer.rs` (cutover `sleep_until` arm, one post-turn `check_planned_transfers`). Acquisition reuses MC-C's `ReplacementState` (`on_draining`), removal reuses `remove_handler_for` (set-without-X → quiesce → `EndMeeting(PLANNED_TRANSFER)`).
- **Outcome**: `completed` iff target non-empty and stragglers == 0 at removal, else `timed_out` (incl. shed); invariant `completed + timed_out == declarations_total{planned_transfer}`. Log carries `cutover=covered|deadline`.
- **Crash-path fix**: replacement target count excludes draining handlers (`non_draining_member_count`).
- **Config**: `MC_PLANNED_TRANSFER_MARGIN_MS` (default 5000, [1000, 30000], required configMapKeyRef), gauge `mc_media_handler_planned_transfer_margin_seconds`.
- **MH (Domain-judgment, owner media-handler)**: `mh_drain_window_seconds` boot gauge from `Config::drain_window`; env-test 02 O9 drift guard (margin < min serving MH drain window − heartbeat).
- **Dashboards**: story-4 handler rows split from `mc-media.json` into `mc-media-handlers.json` (own ConfigMap + projected volume) — ConfigMap was at 80.2% of the size cap.
- **Tests**: 13 Tier-1b tests in `planned_transfer_integration.rs` (shared rig `tests/common/replacement_rig.rs`); env-test 40 legs A (with target) and B (no target, forced baseline drain with deficit-driven restore); env fixture DRY consolidations (`kube::blocking`, frame-proof helpers).

---

## Files Modified

```
{Output of: git diff --stat HEAD}
```

### Key Changes by File
| File | Changes |
|------|---------|
| `path/to/file.rs` | {Brief description} |

---

## Devloop Verification Steps

### Layer 1: cargo check
**Status**: PASS/FAIL
**Duration**: ~Xs
**Output**: {Any relevant notes}

### Layer 2: cargo fmt
**Status**: PASS/FAIL
**Duration**: ~Xs
**Output**: {Any relevant notes}

### Layer 3: Simple Guards
**Status**: ALL PASS (layer-fast, after the dashboard split)
**Duration**: ~127s

**Dashboard ConfigMap headroom (measured).** `dt-guard kustomize` prints only `STATUS=OK REASON=kustomize-clean-kubeconform-skipped-30-consumed-configmaps-1-resource-field-refs` when it passes; it reports no per-ConfigMap size. The sizes below apply the guard's own byte model (`kustomize_configmaps.rs::last_applied_len` + `METADATA_ALLOWANCE_BYTES` 1024, the same arithmetic as `judge_size`) to the files as committed:

| ConfigMap | Measured | With allowance | % of 262144 cap (fails > 80%) |
|---|---|---|---|
| `grafana-dashboards-mc-media` | 156539 | 157563 | 60.1% |
| `grafana-dashboards-mc-media-handlers` | 56205 | 57229 | 21.8% |

Before the split, the guard's failing output put `grafana-dashboards-mc-media` at 210478 bytes, 80.2%.

| Guard | Status |
|-------|--------|
| api-version-check | PASS/FAIL |
| no-hardcoded-secrets | PASS/FAIL |
| no-pii-in-logs | PASS/FAIL |
| no-secrets-in-logs | PASS/FAIL |
| test-coverage | PASS/FAIL |

{Details on any failures}

### Layer 4: Unit Tests
**Status**: PASS/FAIL
**Duration**: ~Xs
**Output**: {Test counts, any failures}

### Layer 5: All Tests (Integration)
**Status**: PASS/FAIL
**Duration**: ~Xs
**Tests**: {X passed, Y failed}

{Details on any failures}

### Layer 6: Clippy
**Status**: PASS/FAIL
**Duration**: ~Xs
**Output**: {Any warnings}

### Layer 7: Env-tests
**Status**: PASS (attempt 3, after operator intervention)
**Duration**: full pipeline 2619s; 40_resilience 10/10 in 492s
**Output**: `STATUS=OK REASON=env-tests-passed`, `STATUS=OK REASON=browser-e2e-passed`; log `gate2-attempt3.log`

(Semantic-guard relocated to the Gate 2 reviewer panel per ADR-0033 Wave 3 #9. See § Code Review Results → Semantic Guard Reviewer below for its findings.)

---

## Code Review Results

### Gate 3 Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 1 | 1 | 0 | F-1 negative test for removed-handler DRAINING beat; forged-DRAINING residual recorded (ADR-0039 S6/S1b class) |
| Test (paired) | RESOLVED-FIXED | 3 | 3 | 0 | T1 set-before-EndMeeting pinned (red when swapped), T2 UNAVAILABLE arm, T3 margin gauge in 02 |
| Observability | RESOLVED-FIXED | 2 | 2 | 0 | panels 208/209/212; timed_out semantics (F6 ruling); dashboard split |
| Code Quality | RESOLVED-FIXED | 8 | 8 | 0 | F1–F8 |
| DRY | RESOLVED-FIXED | 4 | 4 | 0 | no extraction opportunities |
| Operations | RESOLVED-FIXED | 1 | 1 | 0 | F1 covered-at-deadline counted timed_out |
| Semantic Guard | CLEAR (SAFE) | 0 | 0 | 0 | |
| Media Handler (conditional) | RESOLVED-FIXED | 3 | 3 | 0 | owner of Domain-judgment MH hunk |
| Infrastructure (co-review) | RESOLVED-FIXED | 1 | 1 | 0 | baseline-short restore hint in mh_pool.rs |

Lead ruling change during review: "every deadline cutover is timed_out" superseded — `timed_out` only with stragglers or shed (matches task item 6; telemetry contract).

### DRY Reviewer
**Extraction opportunities**: None.

---

## Accepted Deferrals

- (none surfaced in this devloop)

- `docs/TODO.md` §Env-Test Resilience & Runbook Validation — injection suites orphan a live MC meeting (MC-C replacement)
  - **SUPERSEDED 2026-11-14** by `docs/devloop-outputs/2026-11-14-injection-orphan-meeting-fix/` (commit a1c475ba), per operator ruling (no global-counter coupling between env-tests; no slower suites). The compensating `MhPool::run_leg*` gate (`metrics::await_no_mc_meetings`, token `foreign-mc-meetings`) and its browser twin were removed. The injection suites now end their own meeting via `injection::release_and_end_meeting`, proved from per-meeting MC log lines (`fixtures/meeting_end.rs`). The TODO entry is deleted. References to the gate elsewhere in this file describe the state at the time.

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `ca97bbe635fe22308c74797ddecb97e6a0eeb4b4`
2. Review all changes: `git diff ca97bbe6..HEAD`
3. Soft reset (preserves changes): `git reset --soft ca97bbe6`
4. Hard reset (clean revert): `git reset --hard ca97bbe6`
5. For schema changes: rollback requires a forward migration — `git reset` alone is insufficient if migrations were applied
6. For infrastructure changes: may require `skaffold delete` or `kubectl delete -f` if manifests were applied
7. **Safe-revert unit** (answer explicitly, even if "the whole commit"): can any part of this diff be reverted or cherry-picked on its own, or does a partial revert reconstruct a state worse than either endpoint (e.g. a security fix split from the change that made it necessary, or a client/server/alert-rule set that must move together)? If partial reverts are unsafe, name the unit and the safe direction. **Answer**: the whole commit — MC config key, Deployments, configMapKeyRef and env-test 02 must move together (a partial revert leaves a required key unread or an env-test asserting an absent gauge) (This converts silence into a visible unanswered slot; it cannot distinguish a checked answer from a reflexive one.)

---

## Issues Encountered & Resolutions

### Issue 1: SERVING flip-back pulled the cutover to "now"
**Problem**: `draining_since` is sticky, so a contract-violating SERVING beat still reached the start/tighten path, and its `drain_remaining_ms = 0` tightened the deadline to its receipt, causing an immediate cutover. Caught by the flip-back test.
**Resolution**: only a beat that ITSELF reports DRAINING (`handler_health::is_draining(liveness.serving_state)`) may start or tighten a transfer. The test asserts the flip-back neither aborts nor accelerates the transfer.

### Issue 2: the per-transfer invariant was not exact
**Problem**: `abandoned{planned_transfer}` also carries MC-C's attempt-level discards (a superseded Acquire when the set version changes mid-call). A parallel-run flake showed `completed + timed_out + abandoned == transfers` failing.
**Resolution**: the exact invariant is `completed + timed_out == declarations_total{trigger="planned_transfer"}` (observability-approved). The catalog, the contract row, the panel and P6 were reworded. The supersede path is pinned deterministically (`gc.hold()`) in `a_draining_grant_is_no_target_and_a_superseded_acquire_is_no_terminal`.

### Issue 3: mc-media dashboard ConfigMap at the annotation cap
**Problem**: `validate-kustomize` failed: the `grafana-dashboards-mc-media` ConfigMap was at 80.2% of the 262144-byte last-applied cap (the guard fails above 80%). My first response, trimming panel text, was a masking workaround (observability Gate-2 pre-review).
**Resolution: the SPLIT (fixed in this devloop, per observability). The earlier text trim was reverted, so it is not the fix.** The DASHBOARD was split, following the kustomization comment's prescription. The story-4 handler-lifecycle rows (Handler Heartbeats, Handler Detection, Handler Set & Dial Credential, Handler Replacement: 21 panels, ids unchanged) moved into the new `infra/grafana/dashboards/mc-media-handlers.json` (uid `mc-media-handlers`). It has its own generator `grafana-dashboards-mc-media-handlers`, is a source of Grafana's projected `dashboards` volume (R-21), and the two boards link to each other. The trimmed text was restored. The catalog "Dashboard:" lines for the moved series, the dashboards inventory, the story's board heading, the runbook link, the contract and both INDEXes were repointed. Panel 192 (pinned by `outcome.rs`) stays on mc-media. `dt-guard kustomize` and `application-metrics` both pass. Measured with the guard's byte model: mc-media at 60.1% and mc-media-handlers at 21.8% (see Layer 3).

### Issue 4: O9 needed MH's shutdown margin, a Rust constant
**Resolution**: per the lead ruling (a), MH publishes `mh_drain_window_seconds` at boot from `Config::drain_window`. paired-test wrote the env-test 02 drift guard over it.

### Hand-off to MC-E
MC-E must add the ADR-0032 unobserved assertion on `loss_clients_total` / `loss_prior_report_ratio` / `loss_client_lead_seconds` for a planned transfer. The planned removal never enters `act_on_health`'s declaration branch (MC-E's snapshot site). Until then, `planned_transfer_integration.rs` asserts that no `heartbeat_timeout`/`apply_lag` declaration or detection is observed, with a positive crash arm (`the_draining_handler_dying_mid_transfer_is_abandoned_once`).

---

## Lessons Learned

1. {Key takeaway 1}
2. {Key takeaway 2}
3. {Key takeaway 3}

{Add more as applicable}

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

## Gate 2 — Validation Record

| Attempt | L1-6 | L7 | Notes |
|---------|------|----|-------|
| 1 | OK | FAIL | 40_resilience 8/10: mc_b2_zombie (foreign orphan meeting from suite 39 injection cleanup, latent since MC-C); mc_f leg B (assert_addresses_unique inside drained window — test bug). Env-test fixes applied. |
| 2 | OK | FAIL (rc 124, DEVLOOP_ENV_TEST_TIMEOUT=1800s) | 40_resilience: mc_c_crash, mh_pool_fixture, mc_f leg A, mc_f leg B FAILED (pools dropped without teardown); binary killed before panic output. Pool clean afterwards. |

After attempt 2: Layer-7 budget (2 attempts) exhausted → headless escalation (`.devloop-escalation.json`, reason `validation-attempts-exhausted`). Logs: `gate2-attempt{1,2}.log`, `gate2-attempt2-layer7-env-test.log` in this directory.

### Resume (2026-11-13, headless run-story relaunch)

Operator intervened after the escalation: commit `195b7059` (devloop `2026-11-12-mc-empty-meeting-join-window`) makes MC end meetings that no participant joins (and enforces the disconnect grace). Attempt 2's four fast leg failures all panicked in `run_leg`'s `await_no_mc_meetings` foreign-meeting precheck before the body ran (hence "dropped without teardown"), which is the orphan-meeting class that commit addresses. The Gate-3 verdicts above stand (no code change since review). Gate 2 re-run on the current tree as attempt 3, which opens a fresh Layer-7 budget because the operator intervened (log: `gate2-attempt3.log`).

| Attempt | L1-6 | L7 | Notes |
|---------|------|----|-------|
| 3 (post-operator, HEAD 195b7059) | OK | OK | `TOTAL_RESULT=OK`; 40_resilience 10/10; browser E2E OK. Log: `gate2-attempt3.log` |
