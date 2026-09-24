# Devloop Output: GC multi-party join stickiness and robustness (R-6)

**Date**: 2026-09-24
**Task**: Reuse path in `assign_meeting_with_mh` returns the existing MC assignment without MH selection; `mh_selection` optional; log lines relocated; unit + integration tests (story 2 task 5, R-6)
**Specialist**: global-controller
**Mode**: Agent Teams (v2) — full panel; Gate-1 RUN (tier=light escalated to full: task edits `tracing::` instrumentation, a `--light` exclusion surface); paired with test
**Branch**: `feature/hear-each-other`
**Duration**: ~3h

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `e1a7957aafc8e206ed7fb8ac03eeefc806d0c257` |
| Branch | `feature/hear-each-other` |
| Lead Model | `claude-opus-5-5` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` |
| Implementing Specialist | `global-controller` |
| Tier | `full — escalated from light (manifest tier=light): plan edits tracing:: instrumentation in handlers/meetings.rs (--light exclusion surface), so Gate-1 runs per Step 5 escalation rule` |
| Iteration | `1` |
| Security | `security` |
| Test | `paired-test` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Semantic Guard | `semantic-guard` |

---

## Task Overview

### Objective
See the task-5 prompt (docs/user-stories/2026-09-21-hear-each-other.md, R-6): on the existing-healthy-assignment path in `assign_meeting_with_mh`, return the already-assigned MC without calling `select_mhs_for_meeting`; the reuse path must not depend on a live MH pool.

### Scope
- **Service(s)**: gc-service
- **Schema**: No
- **Cross-cutting**: No

### Debate Decision
NOT NEEDED - contained GC behavior change; design settled in story decomposition.

---

## Cross-Boundary Classification

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `crates/gc-service/src/services/mc_assignment.rs` | Mine | — |
| `crates/gc-service/src/handlers/meetings.rs` | Mine | — |
| `crates/gc-service/tests/meeting_tests.rs` | Mine | — |
| `crates/gc-service/tests/mc_assignment_rpc_tests.rs` | Mine | — |
| `crates/gc-service/tests/meeting_assignment_tests.rs` | Mine | — |
| `docs/observability/metrics/gc-service.md` | Not mine, Minor-judgment (scope notes: `gc_mh_selections_total` / `gc_mh_selection_duration_seconds` new-assignment-only; `gc_mc_assignment_duration_seconds` reuse path loses a DB round trip; stickiness-rate PromQL example under `gc_mc_assignments_total`; no name/label/bucket change) | observability (on panel; review-only) |
| `docs/runbooks/gc-incident-response.md` | Not mine, Minor-judgment (Scenario 3: MH-pool-empty triage note — new-meeting joins fail while existing meetings keep joining; distinguishing query; §9 design reason) | operations (on panel; review-only) |
| `crates/gc-service/src/routes/mod.rs` | Mine (Gate 3, security F1: guest-route comment corrected to "rate limiting NOT implemented") | — |
| `crates/gc-service/tests/grpc_mc_call_metrics_integration.rs` | Mine (Gate 3, dry F/observability R4-1+R4-2: boundary-literal note; new `mc_method_constant_matches_emitted_literal` pin so the restated literals are load-bearing; header cites emission sites by function, not stale line numbers) | — |
| `crates/gc-service/src/services/mc_client.rs` | Mine (Gate 3, operations R-4: four `record_grpc_mc_call` literals now read `MC_METHOD_ASSIGN_MEETING_WITH_MH` — one encoding of the `method` label value, so a proto RPC rename cannot silently desync it) | — |
| `crates/gc-service/src/observability/metrics.rs` | Mine (Gate 3, observability OBS-3 / operations R-3: gRPC MC cluster test derives `method` from `GRPC_MC_METHODS` instead of the never-emitted literal `assign_meeting`) | — |
| `docs/runbooks/gc-deployment.md` | Not mine, Minor-judgment (Gate 3, operations R-2 sibling: Issue 5 Fix step 1 "Scale MC pods" corrected to restore the singleton Deployment + pointer to Scenario 3 capacity levers; fixed rather than tracked so "MC scales by replicas" survives nowhere) | operations (on panel; review-only) |
| `docs/runbooks/mh-deployment.md` | Not mine, Minor-judgment (operations OPS-1/OPS-2: correct three now-false "no MH => every join fails" claims — ~463, ~532 page justifications keep the page but restate the reason; ~666-676 kill-switch section: blast radius becomes "all NEW meetings fleet-wide + media on every meeting", conclusion "don't use it; redeploy" restated on the true reason) | operations (on panel; review-only; sentences originally media-handler's) |
| `docs/user-stories/2026-09-21-hear-each-other.md` | Not mine, Domain-judgment (operations OPS-4: add an 8th entry to the "Premises this story falsifies" list at ~141 **and update its count word "seven"→"eight"**; append a dated correction note to the R-29 task's mandated `mh-deployment.md` wording at ~584 so the verbatim instruction is post-task-5 true) | story owner = `@team-lead` — pulled in at Gate 1 by this row; "Plan approved" is the confirmation |
| `infra/docker/prometheus/rules/gc-alerts.yaml` | Not mine, Minor-judgment (operations OPS-5/OPS-7: **comments only** above `GCMCAssignmentSlow` (:67) **and** `GCHighJoinLatency` (:415), observability's verbatim wording. No expression, threshold, label, severity or `for:` change; both rules fire exactly as today) | operations (on panel; review-only) |
| `docs/observability/alerts.md` | Not mine, Minor-judgment (one "Population caveat (R-6)" sentence after the PromQL block of `GCMCAssignmentSlow` ~:143 and `GCHighJoinLatency` ~:584, observability's verbatim wording) | observability (on panel; review-only) |
| `docs/TODO.md` | Mine (de-duplicate the byte-identical block `:699-701` against `:676-678` — verified identical with `cmp` — and delete the stale strict-prefix copy `:315` of `:287` (OPS-6); dated sentence on the existing duplicate-detection entry ~`:1011`; append ONE dated "trigger fired / re-deferred" line to the surviving `no_mcs_available` entry (operations P-1, security's option 1); new §Observability Debt entry for the `GCMCAssignmentSlow` population dilution (observability Ask 2)) | — |
| `docs/specialist-knowledge/global-controller/INDEX.md` | Mine (pointer update only if needed) | — |

No GSA path touched (no proto, no migration, no auth routing, no route/response shape change).

---

## Planning

### Production change (`services/mc_assignment.rs`)
1. `AssignmentWithMh.mh_selection: MhSelection` -> `Option<MhSelection>`. Doc: `Some` iff this call made a NEW assignment (the selection that was sent to MC in the once-per-meeting `assign_meeting` RPC); `None` on the reuse path, where the handler set is owned by MC (frozen at first join) and GC does not re-read it.
2. Reuse branch (`get_healthy_assignment` -> `Some(existing)`): delete the `select_mhs_for_meeting` call; amend the existing "Found existing healthy assignment" debug in place to "Existing healthy assignment reused; handler owned by MC, no MH selection performed" (same target, `meeting_id` + `mc_id` only); keep `metrics::record_mc_assignment("success", None, start.elapsed())` unchanged; return `mh_selection: None`. Add a comment encoding the ceiling pairing (see Test notes) so nobody "fixes" the missing ceiling re-check.
3. New-assignment branch: unchanged except `mh_selection: Some(mh_selection)` at the Accepted return.
4. Update module doc steps (1-6) and the fn doc (`# Returns`, `# Errors`: reuse path cannot fail on MH pool state).

### Handler / log change (revised after Gate-1 input: observability O-1/O-2, dry #4/#5, security S1)
- The per-join records "User joined meeting" (`join_meeting`) and "Guest joined meeting" (`guest_token`) stay **unconditional, same message, same target, same fields minus `mh_ids`** — every join (reuse included) keeps one info-level correlation record with `mc_id`. Both handlers get identical treatment (no partial invariant).
- The `mh_ids` info detail moves to the service's new-assignment branch where the selection is live: `mh_ids = ?mh_ids` is added to the existing `info!(target: "gc.service.assignment", ..., "Meeting assigned to MC with MH")` in the `Accepted` arm, reusing the `mh_ids` Vec already built at the top of that branch (no third extraction, no handler helper). The pre-RPC debug "Selected MHs for meeting" stays (with a code comment saying why): it is a different event (fires before the RPC, useful when every MC rejects), not a restatement.
- Reuse debug: the existing `debug!(target: "gc.service.assignment", meeting_id, mc_id, "Found existing healthy assignment")` is **amended** (not duplicated) to "Existing healthy assignment reused; handler owned by MC, no MH selection performed". Fields stay `meeting_id` + `mc_id` only.
- `#[instrument(skip_all, ...)]` unchanged. No new PII fields. `JoinMeetingResponse::new` untouched — no response shape change. No route/request/proto/migration change; join stays GET.
- Fail-closed shape (operations P-5): the new-assignment branch keeps a concrete `MhSelection` binding passed to `assign_meeting` / `selection_is_malformed` exactly as today; `Some(..)` is only applied at `AssignmentWithMh` construction, so `None` is structurally unreachable from the RPC. Reuse returns `None`, never `Some(empty)` — `MhSelection`'s non-empty invariant and `selection_is_malformed`'s premise stay intact (dry #3).

### Tests
- **Unit / service-level** (`#[sqlx::test]` in `tests/mc_assignment_rpc_tests.rs`, placed next to `test_assign_meeting_with_mh_no_mhs` / `_returns_existing` so it reuses `setup_mcs`/`setup_mhs` and no seeding SQL lands in `src/` (dry F1); both it and `_no_mhs` get a one-line doc naming the branch each exercises — empty pool FAILS a new assignment, SUCCEEDS a sticky join): healthy MC + MHs, assign once (shared `MockMcClient`), then flip every MH `Unhealthy` via `MediaHandlersRepository::update_load_report`. Under `MetricAssertion::snapshot()` taken AFTER the first (new) assignment: reuse call is Ok, same `mc_id`, `mh_selection.is_none()`, `gc_mh_selections_total{status=success|error}` delta 0 (proof by observation that no selection ran), `gc_mc_assignments_total{status=success}` delta 1 (preserved emit), `call_count` unchanged at 1. **Positive control**: in the same empty-pool state, a different unassigned meeting fails `ServiceUnavailable` ("media handlers") and `call_count` does not move — proves the pool really is empty.
- **Metric assertions (observability O-5)**: the integration reuse test also wraps the reuse join in `MetricAssertion::snapshot()` asserting `gc_mc_assignments_total{status=success}` +1 and `gc_mh_selections_total` +0, with the new-assignment join in the same test asserting `gc_mh_selections_total{status=success}` +1 as the positive control (current-thread `#[sqlx::test]` runtime serves the spawned axum task on the same thread). If the HTTP path proves not to route through the thread-local recorder, the metric assertions stay at the unit level (already there) and I'll say so.
- **Integration** (`tests/meeting_tests.rs`): add `TestMeetingServer::spawn_with_mc_client(pool, Arc<MockMcClient>)`; `spawn()` delegates to it. (a) two different users GET-join one meeting via one shared mock -> both 200, identical `mcAssignment` (mcId + grpcEndpoint), `call_count()==1`; two healthy MCs registered so "same MC" is non-trivial. (b) join 1 succeeds, all MHs flipped unhealthy, user join 2 AND a guest-token join both 200 with the same MC (guest `mcId` == user `mcId`), `call_count()==1` (covers both relocated log sites' callers). (c) ceiling flavour: MHs set `current_streams = max_streams` instead of unhealthy -> sticky join still 200, `call_count()==1`; positive control in the same state: a join into a different, unassigned meeting returns 503 with `call_count` unmoved (catches a `<=` off-by-one). (a) also checks the response `mcId` against the persisted row (`McAssignmentService::get_assignment`).
- **Existing tests updated**: `mc_assignment_rpc_tests.rs` / `meeting_assignment_tests.rs` asserts on `.mh_selection.handlers` become `.mh_selection.as_ref().expect("new assignment carries a selection")` (no `unwrap_or_default` vacuity); `test_assign_meeting_with_mh_returns_existing` switches to ONE shared mock asserting `call_count()==1` and `mh_selection.is_none()` on the reuse result; `test_service_assign_meeting_with_mh_reuses_healthy` asserts `is_none()`.
- No dedicated env-test: cluster-level stickiness is proven transitively by the multi-party browser scenario (N+1 participants hearing each other share one handler set and one MC).

### Test notes — ceiling-enforcement pairing (R-19, §9) — encoded in the reuse-branch comment and the reuse test doc comments
GC placement enforcement against the MH-advertised `max_streams` (R-19) is **SOFT and new-meeting-only**: `get_candidate_mhs` excludes at-or-over-ceiling handlers (`current_streams < max_streams`), `select_mhs_for_meeting` returns `ServiceUnavailable` when none remain, there is no reservation, and GC can over-place between load reports. The reuse path **deliberately** waves a sticky join through with no ceiling re-check, because a meeting cannot be split across handlers in story 2 (§9) — re-checking could only fail the join, never move it. Over-ceiling growth is bounded by **MH admission, the HARD backstop**. Non-regression (security S4): before this change the reuse path's re-selection was discarded by `JoinMeetingResponse::new`, so the soft ceiling was **never** enforced on a sticky join — removing the call weakens nothing. GC consumes `max_streams` unchanged: `register_mh` stores it, `update_load_report` stores MH-reported `current_streams`, `get_candidate_mhs` filters on it — no GC code change for the egress chain.

### Observability / ops notes
- `gc_mh_selections_total` / `gc_mh_selection_duration_seconds`: now new-assignment-only (exactly one `select_mhs_for_meeting` callsite). Catalog Description lines amended; note that `status="error"` can no longer fire for a join into an existing meeting, and that the rate steps down at the GC roll (not an incident). Dashboard panels are `increase()`/latency views with no per-join denominator, and no alert keys on these metrics — no panel/alert change.
- `gc_mc_assignment_duration_seconds` (observability Ask 1 — the consequence, not just the shift): the reuse path loses a DB round trip, so the histogram is strongly bimodal with **no label separating the modes** (both branches emit `status="success"`; `GCMCAssignmentSlow` quantiles p95 over the unfiltered population with no `status` filter). New assignments are ~1/N of joins, so at mean meeting size >= 20 they fall below the p95 cut and the alert becomes structurally incapable of firing on new-assignment slowness — detection degrades as meetings grow. Catalog gets a **Population note (R-6)** saying: read the p95 drop at the R-6 roll as a population shift, NOT a latency improvement, and do not tighten the 20 ms threshold against the diluted population; mode separation tracked in `docs/TODO.md` §Observability Debt. Surviving detection for the new-assignment path is **error-side, not latency-side**: `GCMCAssignmentFailures` (`gc-alerts.yaml:211`) and `GCHighJoinFailureRate` (`:368`), neither touched by this diff — `GCHighJoinLatency` (`:415`, unfiltered p95 over `gc_meeting_join_duration_seconds`) inherits the identical dilution and is **not** a surviving latency control (observability Additions A + B).
- Deferred with justification (observability, not accepted as silent): the metric fix itself — a `branch`/`assignment_type` label would change `record_mc_assignment`'s signature, directly contradicting the task's preserve-`record_mc_assignment("success", None, ..)` constraint, and cascade across ~8 callsites, two metric tests, the cardinality table, `label-taxonomy.md`, two dashboards, `alerts.md` and the runbook legend; the SLO-redefinition option is an ADR-0010 target change. TODO entry (Ask 2) names owners `observability` + `operations`, filed 2026-09-24 at this devloop, the three candidate resolutions (label / split histogram / new-assignment-only SLO + filtered alert), the trigger (next devloop touching `gc_mc_assignment_*`, `gc_meeting_join_duration_*`, the GC SLO panel, or either rule's threshold), the fact that both rules currently quantile over an unfiltered population, and **both** affected rules by name — `GCMCAssignmentSlow` (`gc-alerts.yaml:67-81`) **and** `GCHighJoinLatency` (`:415`) — so a future branch-label fix scoped to only `gc_mc_assignment_duration_seconds` cannot look complete while `gc_meeting_join_duration_seconds` stays diluted.
- Stickiness SLI (O-4): PromQL example under `gc_mc_assignments_total`: `1 - sum(rate(gc_mh_selections_total{status="success"}[5m])) / sum(rate(gc_mc_assignments_total{status="success"}[5m]))` (approximate in a **known direction**: a selection followed by all-MC-rejection increments the numerator without a matching `gc_mc_assignments_total{status="success"}`, so the expression **under-reports** the reuse rate — the safe direction for a stickiness SLI). ANCHOR comment at the single `select_mhs_for_meeting` callsite in `mc_assignment.rs` naming this dependency.
- `gc_mc_assignments_total{status=success}` unchanged on reuse.
- Runbook (operations P-2): Scenario 3 gains an MH-pool-empty triage note: post-change symptom is new-meeting joins 503 ("No media handlers available in this region") while existing meetings keep joining; distinguishing signal `gc_mh_selections_total{status="error"}` rising while `gc_mc_assignments_total{status="success"}` stays healthy; the §9 design reason and MH admission as HARD backstop.
- TODO (operations P-1): the `no_mcs_available` conflation defer-trigger fires on this devloop (it edits `mc_assignment.rs`). Re-deferred — cross-owner design decision (GC `Err`-branch + observability taxonomy + operations Scenario D + two tests), orthogonal to the reuse branch; a dated line appended to both duplicate copies.

- MH runbook (operations OPS-1/OPS-2): `docs/runbooks/mh-deployment.md` claims at ~463, ~532 and ~666-676 that with no MH every join fails. After R-6 a GC join into an already-assigned meeting succeeds with zero MHs (the participant then gets no media because its handler is gone); only NEW meetings fail at GC. Pages stay pages (no new meetings can start, all in-progress media dead); the kill-switch table's "actual blast radius" becomes "every new meeting fleet-wide, plus media on every existing meeting"; the "redeploy instead" conclusion survives on that true reason.

- Story spec (operations OPS-4): R-29's mandated wording ("lowering `MH_EGRESS_BUDGET_BPS` … produces a join outage **by the same mechanism as scaling MH to zero**") is specified "in these words" at story line ~584 and would re-ship the premise this task retires, in the file OPS-1 fixes. The mechanism claim survives (a starved budget empties `get_candidate_mhs`, so `select_mhs_for_meeting` 503s — identical to MH being gone); only the consequence changes. Fix in this loop, not by TODO (R-29 runs inside this same story): a dated correction note in the repo's existing convention (as used for the ADR-0036 §4 correction at ~305), so the R-29 implementer sees *why* the words moved. Plus the 8th entry in the falsified-premises list at ~141, with the count word updated — that list exists because nothing mechanical catches prose, and its own ADR-0031 obligation is what surfaced OPS-1.
- OPS-1 wording: the "succeeds but silent" consequence is stated explicitly in the prose, not left implicit in the blast-radius cell — a sticky join returns 200 into a meeting whose handler is gone, which is a worse operator surprise than a clean failure.

- Alert-rule annotation (operations OPS-5): one comment on `GCMCAssignmentSlow` (`gc-alerts.yaml:67-81`) and one line on GC Scenario 3 — whose Symptoms block currently asserts "MC assignment p95 latency >20ms" as a live signal, partly falsified here — each one sentence plus a pointer to the single TODO entry (no restating observability's three candidate resolutions, to avoid a third copy of the reasoning). The rule fires exactly as today.
- Pattern note for the OPS-4 premise-list entry: this sweep ("when a requirement retires a behaviour, grep runbooks and dashboards for text asserting it") turned up **three** instances — `mh-deployment.md` prose, the story's own R-29 task spec, and a paging **alert rule**. The obligation as written names two artifact classes; the 8th list entry gains a clause noting alert rules are a third.

- TODO de-duplication, both pairs (operations OPS-6, supersedes the earlier "leave `:315`" call): `docs/TODO.md:315` is a **stale pre-amendment copy** of `:287` — `:287` strictly contains `:315` (5584 vs 3494 bytes, verified by prefix check) plus the 2026-09-03 "SECOND DEMONSTRATED INSTANCE" amendment, whose revised conclusion `:315` lacks, so the stale copy actively misinforms. Both copies sit under `### From DRY Reviewer (Ongoing)`. Deleting `:315` is a single contiguous line deletion with nothing unique to preserve, in a file already in this diff — fix-now profile. `:287` is kept untouched (no rewording). Same for `:699-701` (byte-identical to `:676-678`). Post-deletion check: no orphaned heading, no doubled blank line.
- New `docs/TODO.md` entry (security, Gate 1): `annotation_hygiene` scans a hardcoded three-key list (`summary`/`description`/`impact` at `alert_rules.rs:1237`); every other annotation key is served on `/api/v1/rules` **unscanned**. Latent, not live — I re-swept `infra/docker/prometheus/rules/*.yaml` and every rule uses exactly those three plus the separately-validated `runbook_url`. **Count correction: 61 rules, not 60** (`grep -c "^\s*- alert:"` across all five files); the entry will say 61. Preferred fix recorded as an **allowlist** of permitted annotation keys (makes the scanned set and the permitted set one object, so drift is structurally impossible) over widening the scan loop; needs dt-guard fixtures for new-key-rejected and new-key-scanned. Owner security (policy content) + infrastructure (matcher shape). Defer trigger: next devloop adding an annotation key to any rule file, or the next `alert_rules.rs` change. This diff adds no annotations, so there is nothing here to fix.
- The existing duplicate-detection entry (`docs/TODO.md` ~`:1011`, "Concurrent reviewers ... nothing detecting that two of them describe one gap") gains a dated sentence (2026-09-24, `2026-09-24-gc-join-stickiness`): this loop found **byte-identical** and **strict-prefix** duplicates, which an exact/prefix match catches with no heuristic and no merge judgment, lowering the cost of a first useful check. Folded into that entry, NOT a new entry.

### Population-dilution marker set (observability final set + operations OPS-7) — six sites, one ANCHOR, verbatim wording
One home per kind of content; everything else is a one-sentence pointer. Wording is applied **verbatim from @observability** (no paraphrase — a paraphrase would be a seventh variant on day one); file calls are operations'.
1. **Analysis home** — `docs/observability/metrics/gc-service.md`, "Population note (R-6)" under `gc_mc_assignment_duration_seconds`, headed by an `ANCHOR (DRY)` comment listing the five mirror sites below (edit in lockstep), and carrying: both GC latency rules are diluted (`GCMCAssignmentSlow`, `GCHighJoinLatency` — `gc_meeting_join_duration_seconds` mixes the same populations for the same 1/N reason); surviving detection is **error-side only** (`GCMCAssignmentFailures`, `GCHighJoinFailureRate`, neither touched); do not build replacement latency coverage — what is lost is sensitivity to *slow* new assignments, not *failed* ones.
2. `gc-alerts.yaml` comment above `GCMCAssignmentSlow` (:67) — POPULATION CAVEAT block (R-6, 2026-09-24).
3. `gc-alerts.yaml` comment above `GCHighJoinLatency` (:415) — short POPULATION CAVEAT pointing at #2; one TODO entry covers both rules.
4. `docs/runbooks/gc-incident-response.md` Scenario 3 Symptoms — **amend, not delete**, the "MC assignment p95 latency >20ms" bullet: reliable only below ~20 mean participants per meeting; triage `GCMCAssignmentFailures` / `GCHighJoinFailureRate` first; pointer to the Population note. The Scenario 3 `**Severity**` line ("Warning (latency >20ms)") oversells the same way — gets the same qualifier (operations: correct, not delete).
5–6. `docs/observability/alerts.md` — one "Population caveat (R-6)" sentence after the PromQL block of each of `GCMCAssignmentSlow` (~:143) and `GCHighJoinLatency` (~:584).
**Guard note (security relay, verified):** sites 2-3 are YAML `#` **comments** above the rules, NOT `annotations:` entries. `crates/dt-guard/src/alert_rules.rs` parses the file with `serde_norway` (comments dropped) and runs `annotation_hygiene`'s secret scan over parsed annotation strings only; its raw-line pass (`:175`) matches `# guard:ignore(...)` markers exclusively. Prometheus serves **annotations** on `/api/v1/rules`, not comments — so the published-surface consideration does not arise, and observability's exact wording stands. `inventory_expr_drift` is unaffected because `expr`/`for` are untouched on both rules (that is also why nobody widens a window "while we're here": it would be two files). `scripts/guards/simple/validate-alert-rules.sh` runs locally before Gate 2.

**Writing cautions for the comment text (operations OPS-8/OPS-9, verified in `alert_rules.rs`):** (a) the text must NOT contain the literal ignore-marker token — `load_ignore_lines` (`:169-175`) scans every raw line including prose comments, so spelling it out either trips `lazy_ignore_reason` at Gate 2 or, worse, registers a **real suppression that reads as prose** and no reviewer sees (the masked-failure shape CLAUDE.md prohibits). Refer to it as "an ignore marker". (b) the text must NOT reproduce a `- alert: <Name>` stanza at line start — `approximate_rule_line` (`:154`) takes the FIRST such match as the reported line for every finding on that rule, so a quoted stanza would shift future findings' line numbers onto the comment. Ordinary prose like `# GCMCAssignmentSlow: population diluted …` is safe; do not paste the stanza.
**Why comment and runbook are both needed (operations, record so neither is later dropped as redundant):** they serve different readers. The `#` comment reaches whoever **edits or tunes** the rule — the person who would otherwise tighten 20 ms against the diluted population. The **paged** operator never sees it: they read the notification's `annotations` and follow `runbook_url`, which resolves into `gc-incident-response.md`, so Scenario 3 is their only reachable marker. Since annotations are deliberately untouched, the runbook line is load-bearing, not a courtesy copy. Operations verified no existing annotation on either rule is falsified by this change, so none needs correcting.

**Resolution options** live ONLY in the `docs/TODO.md` §Observability Debt entry, which names **both** rules explicitly (a fix labelling only `gc_mc_assignment_duration_seconds` would leave `GCHighJoinLatency` diluted and pass its own review looking complete).
Gate-3 check (operations): all six markers landed and say the same thing — five-of-six is the same defect as none.

### Rollout / rollback (operations P-4)
Mixed-version fleet during the GC rolling update is safe: the reuse path writes nothing (returns `existing` straight from `get_healthy_assignment`), makes no MC RPC, and its `mh_selection` was always discarded by `JoinMeetingResponse::new`; old and new pods share no state and differ only in whether a discarded read-only query ran. No flag, no ordering constraint vs MC/MH rollouts. `git revert` is a complete rollback (no schema/proto/config/migration); reverting restores the old spurious-failure bug.

---

## Gate 1 — Plan Confirmations

Lead decision (Gate 1): story-file edits to `docs/user-stories/2026-09-21-hear-each-other.md` (dated R-29 correction note in the task-18 prompt + 8th falsified-premise entry) APPROVED as Domain-judgment with Lead as owner, conditioned on no non-prompt manifest field changing and `dt-story validate` passing.

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test (paired) | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Semantic Guard | confirmed |

Classification-sanity guard: `STATUS=OK`.

---

## Implementation Summary

**Production (`crates/gc-service`)**
- `services/mc_assignment.rs`: `AssignmentWithMh.mh_selection` is now `Option<MhSelection>` (`Some` iff this call made the new assignment; `None` is the only spelling of "no selection"). The reuse branch no longer calls `select_mhs_for_meeting`; its existing debug line is amended in place to "Existing healthy assignment reused; handler owned by MC, no MH selection performed" (`meeting_id`, `mc_id` only); `record_mc_assignment("success", None, ..)` is unchanged and there is no `?` between branch entry and that emit. The reuse branch carries the ceiling-pairing + non-regression comment. The new-assignment branch keeps a concrete `MhSelection` through the RPC and `selection_is_malformed`, wrapping in `Some` only at construction; an ANCHOR at the single `select_mhs_for_meeting` callsite names the catalog stickiness expression that depends on it; the pre-RPC "Selected MHs" debug carries a non-collapse comment; `mh_ids` + `mh_count` added to the existing Accepted-arm info "Meeting assigned to MC with MH". Module doc, fn doc (`# Returns` / `# Errors`) and field doc updated. The field carries `#[allow(dead_code)]` with a reason: no production code reads it (the join response carries only `mc_assignment`); it is the service's return contract, asserted by the integration tests. `#[instrument(skip_all, ..)]` untouched.
- `handlers/meetings.rs`: "User joined meeting" / "Guest joined meeting" info lines stay unconditional on every join, same message and fields minus `mh_ids`. No route, request, response, proto or migration change; join stays GET.

**Tests**
- `tests/mc_assignment_rpc_tests.rs`: new `test_reuse_path_skips_mh_selection_on_empty_pool` (MHs flipped unhealthy via `update_load_report`; snapshot after the first assignment; reuse Ok, same `mc_id`, `mh_selection.is_none()`, `call_count` stays 1, `gc_mh_selections_total{success,error}` +0, `gc_mc_assignments_total{success,none}` +1; positive control: unassigned meeting fails "media handlers", no MC call, `gc_mh_selections_total{error}` +1), placed beside `_no_mhs` with branch-naming docs on both. `_returns_existing` extended in place to one shared mock (`call_count()==1`, `is_none()`). Existing `.mh_selection` asserts use `as_ref().expect(..)`.
- `tests/meeting_assignment_tests.rs`: `_success` adapted; `_reuses_healthy` asserts `is_some()` then `is_none()`.
- `tests/meeting_tests.rs`: `TestMeetingServer::spawn_with_mc_client(pool, Arc<MockMcClient>)` (`spawn()` delegates); `register_healthy_mc(pool, id, region)` (`register_healthy_mc_for_region` delegates, endpoints unchanged). Three HTTP tests: `test_join_sticky_two_users_same_mc_single_assign_rpc` (2 MCs, 2 users, same `mcId`/`grpcEndpoint`, matches persisted row, `call_count()==1`); `test_join_sticky_succeeds_with_zero_healthy_mhs` (user + guest-token joins 200 with same MC after MHs go unhealthy; HTTP-through `MetricAssertion` with the new-join `+1` as the capture control, commented as current-thread-only); `test_join_sticky_succeeds_with_all_mhs_at_ceiling` (sticky 200; new meeting 503, no MC call). Section header carries the ceiling-pairing test notes and the no-env-test rationale.
- Mutation check: re-inserting `select_mhs_for_meeting` on the reuse path fails `test_reuse_path_skips_mh_selection_on_empty_pool`, `test_join_sticky_succeeds_with_zero_healthy_mhs` and `test_join_sticky_succeeds_with_all_mhs_at_ceiling` (reverted).

**Docs / config (comments and prose only)**
- `docs/observability/metrics/gc-service.md`: `gc_mh_selections_total` / `gc_mh_selection_duration_seconds` scoped to new-assignment-only (+ error-arm note, rate expectation); `gc_mc_assignments_total` success-on-both-paths note + stickiness-rate example with ANCHOR and stated under-report direction; `gc_mc_assignment_duration_seconds` "Population note (R-6)" with ANCHOR listing the five mirrors.
- Population-marker mirrors (observability's wording verbatim): `infra/docker/prometheus/rules/gc-alerts.yaml` `#` comments above `GCMCAssignmentSlow` and `GCHighJoinLatency` (no expr/for/label/severity change; no ignore-marker token, no quoted rule stanza); `docs/observability/alerts.md` caveat in both sections; `docs/runbooks/gc-incident-response.md` Scenario 3 Symptoms bullet + Severity line qualified (not deleted).
- `docs/runbooks/gc-incident-response.md` Scenario 3: MH-pool-empty triage note (signature, distinguishing query, §9 design reason, "200 but no media").
- `docs/runbooks/mh-deployment.md`: three "no MH ⇒ every join fails" claims corrected (pages kept; "succeeds but silent" stated in prose).
- `docs/user-stories/2026-09-21-hear-each-other.md`: premise list "seven"→"eight" + 8th entry (incl. alert rules as a third artifact class); dated correction in the task-18 prompt block, indentation unchanged, no other manifest field touched.
- `docs/TODO.md`: removed byte-identical `:699-701` and stale strict-prefix `:315` (+ its trailing blank); dated trigger-fired/re-deferred sentence on the surviving `no_mcs_available` entry; new §Observability Debt entries for the latency-rule population dilution (both rules named) and the `annotation_hygiene` hardcoded key list (61 rules); dated evidence sentence on the existing duplicate-detection entry.
- `docs/specialist-knowledge/global-controller/INDEX.md`: stickiness pointers folded into existing lines (75-line cap).

---

## Devloop Verification Steps

- `./scripts/layer-fast.sh`: exit 0 — L1 OK, L2 OK, L3 OK, L4 N/A (`cargo-test-passed`, `nx-test-passed`; aggregate N/A is the known `not-applicable-to-this-lang` roll-up), L5 OK, L6 N/A (`audit-aggregate-na`). First run failed on `cargo fmt` and `validate-knowledge-index` (INDEX 76 > 75 lines); both fixed, rerun green.
- `cargo test -p gc-service --test meeting_tests --test mc_assignment_rpc_tests --test meeting_assignment_tests`: 41 + 14 + 18 passed.
- `scripts/guards/simple/validate-alert-rules.sh`: `STATUS=OK` (inventory expr/for unchanged).
- `target/release/dt-story validate docs/user-stories/2026-09-21-hear-each-other.md`: clean (exit 0); diff touches only lines 141 and 584.

---

## Code Review Results

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-DEFERRED | 3 | 3 | 3 tracked | F1 false guest-token rate-limit/captcha comments corrected to NOT IMPLEMENTED (implementation tracked); F2 stale ANCHOR line; F3 TODO re-home |
| Test (paired) | RESOLVED-FIXED | 1 | 1 | 0 | ceiling-variant 503 now attributed to MH selection via metric |
| Observability | RESOLVED-FIXED | 6 | 6 | 0 | Scenario 2 diluted rule-out query + 4b substitute; never-emitted `method` label value; vacuous drift test → pin test |
| Code Quality | CLEAR | 0 | 0 | 0 | `#[allow(dead_code)]` on `mh_selection` verified required (bin vs lib lint contexts); re-checked post-verdict R-4 delta: unchanged |
| DRY | RESOLVED-DEFERRED | 9 | 8 (incl. Gate-1) | 2 (ADR-0019 extraction) | reused `MockMcClient`; ceiling paragraph canonicalised under ANCHOR |
| Operations | RESOLVED-DEFERRED | 13 | 13 | 2 | stale "no MH ⇒ every join fails" prose (runbooks, story R-29 spec); "scale MC pods" advice corrected; label SSoT constant |
| Semantic Guard | CLEAR (SAFE) | 0 | 0 | 0 | |

Process note: two edits (operations R-4 constant, then obs R4-1..3 pin test) landed after initial verdicts; Lead routed the post-verdict delta back to observability, code-reviewer and DRY for re-check before final Gate 2. A code-reviewer `git checkout` briefly reverted `mc_assignment.rs` mid-review; implementer restored from backup, final Gate 2 ran on the frozen tree.

---

## Accepted Deferrals

- `docs/TODO.md` §Rate Limiting — GC guest-token rate limiting middleware still not wired (comments now say so)
- `docs/TODO.md` §Rate Limiting — guest-token captcha not validated; ADR-0020 says it is
- `docs/TODO.md` §Guard Coverage Gap — annotation_hygiene scans a hardcoded annotation-key list
- `docs/TODO.md` §Observability Debt — GCMCAssignmentSlow/GCHighJoinLatency p95 diluted by reuse joins
- `docs/TODO.md` §Observability Debt — `no_mcs_available` conflation re-deferred (trigger fired, dated)
- `docs/TODO.md` §From DRY Reviewer (Ongoing) — "empty the MH candidate pool" test helper written twice
- `docs/TODO.md` §Cross-Service Duplication (DRY) — TestMeetingServer/TestGcServer harness fork (folded into AppState-builder entry)

---

## Rollback Procedure

1. `git diff e1a7957aafc8e206ed7fb8ac03eeefc806d0c257..HEAD`
2. `git reset --soft e1a7957aafc8e206ed7fb8ac03eeefc806d0c257` (or `--hard`). No schema/infra state changes.

---

## Gate 2 — Lead Validation (attempt 1)

`DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` → `TOTAL_RESULT=N/A` (no FAIL/PRECONDITION lines).
L1 OK (5s) · L2 OK (4s) · L3 OK (94s) · L4 N/A roll-up (231s; cargo-test + nx-test OK, proto `not-applicable-to-this-lang`) · L5 OK · L6 N/A roll-up (audit-aggregate-na, no dep changes) · L7 OK (1019s; env-tests + browser E2E).

## Gate 2 — Lead Validation (final, frozen post-review tree)

`DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` → `TOTAL_RESULT=N/A`, no FAIL/PRECONDITION, no FMT_APPLIED.
L1 OK · L2 OK · L3 OK (94s) · L4 N/A roll-up (243s; cargo-test + nx-test OK; proto placeholder) · L5 OK · L6 N/A (no dep changes) · L7 OK (1020s; env-tests + browser E2E). An intermediate re-run was stopped when the post-verdict R-4 delta landed.

## Gate 3

All verdicts CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED; no ESCALATED. RESOLVED-DEFERRED (Security, DRY, Operations) reflect TODO-tracked items listed under §Accepted Deferrals.

