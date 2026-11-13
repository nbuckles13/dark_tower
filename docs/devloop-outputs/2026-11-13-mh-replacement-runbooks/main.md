# Devloop Output: OPS — MH replacement runbooks (deployment procedures, incident scenarios, TODO dispositions)

**Date**: 2026-11-13
**Task**: Story 4 task #28 (OPS): ADR-0039 runbooks — mh/mc deployment procedures, MC/MH/GC incident scenarios with OBS-ALERTS anchors, docs/TODO.md dispositions
**Specialist**: operations
**Mode**: Agent Teams (v2) — full panel; Gate-1 SKIPPED — tier=light (run-story manifest; documentation-only task) <!-- panel mode + Gate-1 tier (ADR-0037 §D2); see the Tier row in Loop State -->
**Branch**: `feature/mh-failure-transfer`
**Duration**: ~2h (resumed session: verification, 2 review rounds, Gate 2 incl. ~35 min Layer 7)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `78120d4b01ae4bfff139c7dd11af637863c66f83` |
| Branch | `feature/mh-failure-transfer` |
| Lead Model | `claude-opus-5-5` |

---

## Loop State (Internal)

<!-- This section is maintained by the Lead for state recovery after interruption. -->
<!-- Do not edit manually - the Lead updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer (respawned, iteration 2)` |
| Implementing Specialist | `operations` |
| Tier | `light — run-story manifest tier=light (documentation-only task)` |
| Iteration | `2 (resumed after interruption)` |
| Security | `RESOLVED-FIXED` |
| Test | `RESOLVED-DEFERRED` |
| Observability | `RESOLVED-FIXED` |
| Code Quality | `RESOLVED-FIXED` |
| DRY | `RESOLVED-FIXED` |
| Operations | `RESOLVED-FIXED` |
| Semantic Guard | `not spawned — docs-only diff, no check surface` |
| Infrastructure (task-named reviewer) | `RESOLVED-FIXED` |
| Media Handler (task-named reviewer) | `RESOLVED-FIXED` |
| Meeting Controller (task-named reviewer) | `RESOLVED-DEFERRED` |
| Global Controller (task-named reviewer) | `RESOLVED-DEFERRED` |

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
{What was the goal of this task?}

### Scope
- **Service(s)**: {Which services were affected}
- **Schema**: {Database schema changes? Yes/No}
- **Cross-cutting**: {Does this affect multiple services? Yes/No}

### Debate Decision
{NEEDED/NOT NEEDED} - {Brief justification}

{If debate was needed, link to debate record: `docs/debates/YYYY-MM-DD-{topic}.md`}

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
| `docs/runbooks/mh-deployment.md` | Mine | — |
| `docs/runbooks/mc-deployment.md` | Mine | — |
| `docs/runbooks/gc-deployment.md` | Mine | — |
| `docs/runbooks/mh-incident-response.md` | Mine | — |
| `docs/runbooks/mc-incident-response.md` | Mine | — |
| `docs/runbooks/gc-incident-response.md` | Mine | — |
| `docs/runbooks/ac-service-deployment.md` (`--tail=-1` on selector `kubectl logs`) | Mine | — |
| `docs/runbooks/devloop-validation.md` (`--tail=-1` on selector `kubectl logs`) | Mine | — |
| `docs/TODO.md` | Not mine, Minor-judgment | security (S6 + MH-ownership wording), global-controller (gray-failure merge), meeting-controller (closed O-11/connectivity entries) |
| `docs/devloop-outputs/2026-11-13-mh-replacement-runbooks/main.md` | Mine | — |

---

## Planning

### Gate 1 — SKIPPED (tier=light; reason: run-story manifest tier=light, documentation-only task)

Implementer plans inline below before implementation.

**Plan (operations, documentation only; no code, config, alert-rule or wire changes).**

Verification sources: ADR-0039 (§1-§10, Consequences, TODO dispositions); runbook-alert-changes.md B/C/D/F/G/H/I/J (Superseded/DROPPED items skipped, §I wins); mh-sh-contract.txt + `infra/kind/scripts/mh.sh` `MH_TOKENS` (exit codes/tokens), `deploy.sh:converge_mh_pool()`; telemetry-contract.md + `docs/observability/metrics/*.md` for every metric name; config.rs of MC/MH/GC for ranges/defaults.

Part A — `mh-deployment.md`:
1. §Rollout With Media Flowing: keep the landed MH-3 drain sequence; rewrite it to the 6-step story order (`mh.sh up` → Available = GC-selectable; SIGTERM → DRAINING to GC + per-meeting DRAINING heartbeat with `drain_remaining_ms`; MC make-before-break or survivors; old MH keeps forwarding on its own hostPort; exit at zero meetings or grace − margin; slot frees when pod is gone). Replace "the gap until MC-F" (MC-F landed) with the client consequence (small blip, no reconnecting, no rejoin), shed signal on MC `planned_transfer`/`timed_out` + final `mh.drain` log line, causes (deadline too short, GC unavailable with no survivors; no per-MC limiter), one-value remedy (terminationGracePeriodSeconds), roll-all-at-once moves some meetings twice, upgrade-stuck = check GC (`not-available-timeout` / `mh-rollout-failed`, pod left for diagnosis).
2. Rollback: rollback = converge/`mh.sh upgrade` from previous checkout, meetings migrate; remove MH `rollout undo` and MH content-addressed undo-safety rationale (§Config-failure triage "What undo does now", rollback-layer 1 MH half, rollback code-block comments); state image + ConfigMap retention (live MH pods + previous revision; generations referenced by live MH pods never pruned); first slot-pool deploy and any rollback across it = cluster recreate (R-30); no schema change.
3. New subsections under §Deployment Procedure: node maintenance; address pool exhausted (exit 3 `mh-no-free-slot`, K=6, recreate); mh.sh refusals table (`last-serving-mh`/`stuck-terminating`/`foreign-owner` exit 4, `mh-pool-busy` exit 5, `drain-timeout` exit 1, plus `not-available-timeout`/`mh-rollout-failed`); force-delete ban + crash drill; replacement attempt budget (GC_MH_MAX_EXCLUDE_HANDLER_IDS 16 ≈ 14, `rate_limited`). Scale-to-zero section updated (draining one MH moves meetings; all-to-zero visible as `mc_media_meetings_without_handlers`, paged by MCMediaMeetingsAwaitingHandler).
4. Checklist: remove re-assert caveats and `mc_mh_notifications_without_connection_id_total` remnants; add the 24-hour MH-replacement watch block.
6. Config relationships: verify rows; add relationship notes on owner silence vs N, idle ≈ N (MH table); GC rows live in `gc-deployment.md` (exclude cap ≈ 14; load-report interval → staleness bound + GCMhRegistrationPingPong derivation; fix the stale "rolling sheds every session" text there).

Part A item 5 — `mc-deployment.md`: "MC rollout and MH meetings" subsection in the rollout section, plus relationship notes on `MC_MH_HEARTBEAT_TIMEOUT_MS`.

Part B — new scenarios with the exact §J anchors (verified by rendering headings the way `validate-alert-rules.sh` slugs them), each in TOC with Alert/Severity/Runbook Section/Symptoms/Diagnosis/Resolution/Escalation: MC 24-29, MH 24-26, GC 14-15; MH Scenario 1 re-scoped (heading kept); updates to MC 12/13/15/20 and MH 10/15/17/18/Recovery Procedures/severity table. Alert lines for rules not yet landed say "(lands with OBS-ALERTS)"; names only where the contract names them (asked @observability for the rest).

Part C — `docs/TODO.md`: close O-11 and the MH half of connectivity repair; moot policy_generation-reset and EndMeeting-fence; narrow never-sends-EndMeeting (owner-silence + RELEASED(MEETING_ENDED), residual capacity held up to the silence timeout); S6 + MH-ownership appended with @security's wording (item (1) narrowing verified against code first); gray-failure: merge the duplicate entries (no third entry); MC surge and CI-no-Kind/Layer-7: point to existing entries, add only if absent. Leave "Missing alert: MC RegisterMeeting" untouched (OBS-ALERTS).

Validation: `./scripts/layer-fast.sh`; grep for inbound links before touching any heading; metric/token/anchor cross-check scripts recorded in §Implementation Summary.

---

## Resume (Iteration 2)

**Feedback**: "This devloop was interrupted before completion. Resume from main.md state: finish incomplete phases, then gates and commit as normal." (run-story headless resume; Tier row = light ⇒ Gate 1 stays skipped). Implementer respawned to verify the in-tree implementation against the task prompt, then fast check → full-panel review → Gate 2.

---

## Pre-Work

{Any pending changes committed before starting, dependencies resolved, etc.}

{Or "None" if no pre-work was required}

---

## Implementation Summary

Documentation only (runbooks + `docs/TODO.md`). No code, config, alert-rule, dashboard or wire change.

### Part A — deployment procedures
| Item | Where | Change |
|------|-------|--------|
| A1 drain model | `mh-deployment.md` §Rollout With Media Flowing | Rewritten to the 6-step order (up → Available; SIGTERM → DRAINING to GC + per-meeting heartbeat; MC make-before-break or survivors; keeps forwarding on own hostPort; exit at 0 or grace − 5 s; slot frees when pod gone). "Gap until MC-F" text removed (MC-F landed). New subsections: §Meetings shed at the drain deadline (MC `planned_transfer`/`timed_out`, `mh.drain` "Drain finished"; causes: window too short, GC down + no survivor; no per-MC limiter), §Upgrade stuck = check GC. All "sheds until MC-F" remnants in the file reworded. |
| A2 rollback | `mh-deployment.md` §Rollback, Story-3 rollback layer 1, §Rollback criteria block, §Config-failure triage | Rollback = upgrade from previous checkout, meetings migrate; MH `rollout undo` and the content-addressed undo-safety rationale removed for MH (MC's kept); retention of live-pod images/ConfigMap generations + previous revision; first slot-pool deploy / rollback across it = cluster recreate (R-30); no schema change. "Always `kubectl apply -k`" for MH replaced with "only via deploy.sh". |
| A3 new subsections | `mh-deployment.md` §Operating the MH pool | Node maintenance; address pool exhausted (exit 3); refusals table (`last-serving-mh`/`stuck-terminating`/`foreign-owner` 4, `mh-pool-busy` 5, `drain-timeout` 1, `not-available-timeout`/`mh-rollout-failed` 1); force-delete ban + crash drill; replacement attempt budget. Scale-to-zero updated. Stale "Status: Stub, follow mc-deployment" banner replaced. |
| A4 checklist | `mh-deployment.md` §24-hour check | New "Story 4 MH replacement: the 24-hour watch" (7 checks). Re-assert caveats removed. (`mc_mh_notifications_without_connection_id_total` item had already been removed by MC-B1.) |
| A5 MC rollout | `mc-deployment.md` §MC rollout and MH meetings | New subsection under Deployment Steps. |
| A6 config relationships | `mh-deployment.md` owner-silence row; `mc-deployment.md` `MC_MH_HEARTBEAT_TIMEOUT_MS` row; `gc-deployment.md` `GC_MH_LOAD_REPORT_INTERVAL_MS` row | Relationship notes added. All rows already existed (no missing rows): MH_MEETING_OWNER_SILENCE_TIMEOUT_SECONDS, MH_MAX_IDLE_TIMEOUT_MS, MH_MEDIA_DIAL_CLOCK_SKEW_SECONDS (mh); MC_MH_HEARTBEAT_TIMEOUT_MS, MC_MEDIA_DIAL_TOKEN_TTL_SECONDS, MC_MH_REPLACEMENT_* (mc); GC_MH_* (gc-deployment.md, which is where GC keys live, not mh/mc-deployment). GC row's stale "rolling the MH fleet sheds every session" fixed. |

### Part B — incident scenarios (anchors verified with a GitHub-slug renderer, all 13 §J anchors resolve; §J unchanged)
- MC 24–29 added (+TOC); MC 12 (MCRegisterMeetingFailureRate, TODO comment and "alert candidate" removed, exclusion + re-acquire, retired "RegisterMeeting Timeouts" panel ref removed), 13 (out-of-set/removed Connected expected), 15 (generation lag → APPLY_LAG → 24), 20 (lost EndMeeting retried by RELEASED; never-sends bounded by owner silence) updated.
- MH 1 re-scoped (heading kept), 24 updated in place (Alert/Runbook Section/Symptoms/Resolution/Escalation; aud_mismatch warning added per observability), 25/26 added (+TOC), 10/15/17/18/27 re-assert and legacy text rewritten, Recovery Procedures (planned drain = `mh.sh drain`, crash drill, no force-delete, no hand edits to GC rows).
- GC 14/15 added (+TOC).
- Alert lines: only the five contract-fixed names are used; every other warning is written as its expression + "rule name assigned by OBS-ALERTS (lands with OBS-ALERTS)" (observability ruling).

### Part C — `docs/TODO.md`
- Closed: O-11 ("MC cannot see a lost NotifyParticipantDisconnected on a LIVE MH"); MH half of "Connectivity repair after an MH or MC restart" (MC half out of scope).
- Moot: "MC's policy_generation resets to 1…". "`EndMeeting` has no fence…" was already resolved by MH-2 (no edit).
- Narrowed (open): "A meeting whose MC never sends EndMeeting…" — residual = capacity held up to the silence timeout.
- S6 and MH-ownership: @security's wording appended (item (1) uses security's corrected, code-verified text: SameMc takeover stays silent at MC because of `clamp_to_sent`).
- Gray-failure: no third entry; the duplicate 2026-10-30 entry merged into the GC-3 entry (now with runbook anchor), replaced by a one-line [x] pointer.
- MC surge: existing entry (§MC Rollout Address Sharing) pointed to from mc-deployment.md; CI-no-Kind/Layer-7: pointer sentence added to existing entry G.
- "Missing alert: MC RegisterMeeting Coordination Failures" untouched (OBS-ALERTS).

### Prompt vs shipped differences (code/ADR win)
1. MH is Ready once GC **accepts RegisterMH** (`gc_task.rs` `set_ready` after `register_until_accepted`), then sends a load report immediately — not "once GC has accepted its first load report".
2. MC's gRPC advertise address is the **pod IP**, so after a pod-replacing MC rollout MHs see heartbeat **transport errors** (`mh_mc_heartbeats_total{outcome=error|timeout}`), not FAILED_PRECONDITION; owner mismatch appears for an in-place MC container restart or IP reuse. Owner-silence still releases both at ~15 min. Documented both readings (mc-deployment §MC rollout, MH 25, MC 29). Note for OBS-ALERTS: MC's `owner_mismatch` WARNING will fire on an in-place MC restart.
3. MH Scenario 24 heading already existed from MH-1 as "Media-Dial Credential Rejections"; its slug equals the §J anchor, so it was updated in place, not re-added.
4. Owner-silence floor: code states ≥ 30 × largest heartbeat interval (N/3); the prompt's "≥ 10× largest N" is the same bound (300 s vs N ≤ 30 s). Both stated.
5. `validate-alert-rules.sh` (dt-guard `alert_rules.rs`) resolves only the runbook FILE, not the fragment; anchors were checked with a separate slug renderer.
6. `mh.sh signal` is test-only/owner-gated; the operator crash drill provisions its own MH (`mh.sh up --owner drill-<you>`) and signals that one.
7. MH Scenario 13 cross-ref was not present in MC 12 (MH-1 already moved it); nothing to remove.
8. **A graceful MC rollout releases its own meetings** (meeting-controller review F1): on SIGTERM MC sends `EndMeeting` for every meeting within `SHUTDOWN_RELEASE_BUDGET` (25 s, `teardown.rs`), so after a normal rolling update owner-silence releases are ≈ 0. Part A item 5 and the 24-hour-watch line ("owner_silence about 15 min after an MC rollout") assumed no graceful release. The ~15-min owner-silence path applies only to meetings whose release did not complete (crash/OOM/SIGKILL incl. in-place crash restarts, teardowns cut off at the shutdown deadline, `unavailable_exhausted`). mc-deployment §MC rollout, MC 29, MH 25/26 and the 24h watch line are worded that way.
9. `mh_mc_heartbeats_total{outcome="error"}` rises for BOTH owner-mismatch and new-IP transport failures; `mh_mc_heartbeat_verdicts_total{outcome="owner_mismatch"}` is the discriminator (test review F3).

### For OBS-ALERTS (observability request)
- Backfill rule names into the Alert lines of mc-incident-response 24–29, mh-incident-response 24–26 and gc-incident-response 14 (and 15's no_capacity warning) when the rules land.
- `infra/grafana/dashboards/mh-overview.json` panel description still references the retired "RegisterMeeting Timeouts" panel 25 (dashboard, not touched here).

## Files Modified

```
 docs/TODO.md                          |  24 +-
 docs/runbooks/gc-deployment.md        |   2 +-
 docs/runbooks/gc-incident-response.md | 142 +++++++++++
 docs/runbooks/mc-deployment.md        |  41 +++-
 docs/runbooks/mc-incident-response.md | 429 ++++++++++++++++++++++++++++++++--
 docs/runbooks/mh-deployment.md        | 429 ++++++++++++++++++++++++----------
 docs/runbooks/mh-incident-response.md | 249 ++++++++++++++++----
 7 files changed, 1117 insertions(+), 199 deletions(-)
```
(plus this `main.md`, untracked)

### Key Changes by File
| File | Changes |
|------|---------|
| `docs/runbooks/mh-deployment.md` | Part A: "slot pool, not a Deployment you roll" banner; new §Operating the MH pool (node maintenance, `mh-no-free-slot` exit 3 / K=6, refusals table for `last-serving-mh`/`stuck-terminating`/`foreign-owner` 4, `mh-pool-busy` 5, `drain-timeout`/`not-available-timeout`/`mh-rollout-failed` 1, force-delete ban + crash drill via `mh.sh signal` on a drill-owned MH, replacement attempt budget ≈14); §Rollout With Media Flowing rewritten to the 6-step drain model + client consequence + §Meetings shed at the drain deadline + §Upgrade stuck = check GC; scale-to-zero updated (one-MH drain is not a kill switch; all-to-zero → `mc_media_meetings_without_handlers`); §Rollback = upgrade from previous checkout, image/ConfigMap retention, R-30 recreate, no schema change; MH `rollout undo` and content-addressed undo rationale removed; 24-hour MH-replacement watch (7 checks); owner-silence row relationship note (≥ 10× N, never shorten toward N); re-assert wording removed. |
| `docs/runbooks/mc-deployment.md` | Part A5: §MC rollout and MH meetings (ADR-0039 §6, "incidental, not an MC-failover mechanism"; owner_mismatch vs transport-error readings; ~15 min owner-silence release; PromQL; MC-surge TODO pointer). `MC_MH_HEARTBEAT_TIMEOUT_MS` row: owner-silence floor and idle-timeout ≈ N (R-33) relationship notes. |
| `docs/runbooks/gc-deployment.md` | `GC_MH_LOAD_REPORT_INTERVAL_MS` row: stale "rolling the MH fleet sheds every session" replaced by the drain model; `GCMhRegistrationPingPong` / `address_conflict` derivation note. (`GC_MH_MAX_EXCLUDE_HANDLER_IDS` row already carried ≈14 / `rate_limited`.) |
| `docs/runbooks/mc-incident-response.md` | Part B: Scenarios 24–29 (+TOC) at the §J anchors; Scenario 12 → `MCRegisterMeetingFailureRate` (lands with OBS-ALERTS), exclusion + re-acquire; 13 (out-of-set/removed Connected expected, `sender_id` 0); 15 (generation lag → APPLY_LAG → 24); 20 (lost EndMeeting retried by RELEASED; never-sends bounded by owner silence). |
| `docs/runbooks/mh-incident-response.md` | Part B: Scenario 1 re-scoped to fleet/region MHDown (heading kept); 24 updated in place (heading already at the §J slug from MH-1); 25, 26 added (+TOC); 10 (remove-only reconcile, `mc_media_handler_connected_set_corrections_total`); 15/17/18/27 re-assert and legacy text rewritten; Recovery Procedures (`mh.sh drain`, crash drill, no zero-grace force delete). |
| `docs/runbooks/gc-incident-response.md` | Part B: Scenarios 14 (two live MHs on one address) and 15 (gray failure, known gap) (+TOC). |
| `docs/TODO.md` | Part C: O-11 closed; MH half of connectivity repair closed; policy_generation-reset moot; never-sends-EndMeeting narrowed with residual; S6 and MH-ownership updated (security wording); gray-failure duplicate merged into the GC-3 entry (runbook anchor added); CI-no-Kind/Layer-7 pointer on the existing entry. "Missing alert: MC RegisterMeeting" untouched. |

### Resume verification (iteration 2)
Every Part A/B/C item re-checked against the working tree; nothing was missing or half-done, no edits needed beyond this file. Checks run:
- 13 §J anchors: each heading renders (GitHub slug) to the exact slug, appears once, and is in its file's TOC.
- All 61 `mc_`/`mh_`/`gc_`/`dt_client_` names on added lines resolve in `docs/observability/metrics/*.md` (exceptions are intentional: `_bucket` suffixes of catalogued histograms, the never-built `mc_media_connectivity_divergence_total` quoted in a closed TODO, the `mh_grpc_endpoint` log field, and `mh_connected_client_not` quoted from old TODO text). Every label value in an added `{…}` selector checked against its catalog section.
- Every `mh.sh` verb/flag, exit code and REASON token against `infra/kind/scripts/mh.sh` `MH_TOKENS`; `MH_DRAIN_MARGIN_S`=30, K=`MH_SLOT_COUNT`=6, PDB `minAvailable: 1`, grace 35 / margin 5, `GC_MH_MAX_EXCLUDE_HANDLER_IDS`=16, log strings ("Drain finished" target `mh.drain`, "GC rejected registration", "Failed to connect to GC", "RegisterMeeting retries exhausted") against code.
- Leftovers: remaining `re-assert` hits all state it is retired; remaining `rollout undo` hits are MC/GC only (or say MH has none); `mc_mh_notifications_without_connection_id_total` remains only in mc-deployment.md's MC-B1 cross-version pre-check (read on the OLD MC before rolling — still valid for that upgrade path).
- Cross-boundary table: dropped the conditional `runbook-alert-changes.md` (§J) row — no anchor changed, so §J was correctly not edited (`validate-cross-boundary-scope` flagged it as planned-untouched).
- Inbound links: every `<runbook>.md#anchor` reference repo-wide and every intra-file `](#…)` in the six runbooks resolves.

---

## Devloop Verification Steps

Gate 2 (Lead, final tree after review round 2): `DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` → `TOTAL_RESULT=OK`.

| Layer | Result | Duration (s) |
|-------|--------|--------------|
| 1 Compile | OK | 7 |
| 2 Format | OK | 8 |
| 3 Guards | OK | 132 |
| 4 Test | OK | 146 |
| 5 Lint | OK | 1 |
| 6 Audit | OK (dep audit SKIPPED-NO-DIFF no-dep-changes) | 5 |
| 7 Env-tests | OK | 2104 |

Fast check (Lead, before review): layers 1-6 OK. Earlier fast-check failure (validate-cross-boundary-scope on a planned-but-untouched §J row) fixed by the implementer by removing the row.

---

## Code Review Results

Gate 3 (iteration 2, resumed). Review→implementation rounds: 2. Semantic Guard not spawned (docs-only, no check surface).

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 4 | 4 | 0 | MC 28 attribution/evidence, forger containment security-led, empty-list shapes, TODO S6 wording |
| Test | RESOLVED-DEFERRED | 3 | 3 | 1 | psql class (~67 pre-existing sites) deferred; new instances fixed |
| Observability | RESOLVED-FIXED | 3 | 3 | 0 | `by (pod)`→`by (instance)`; MC 12 single Alert line; `gc_mh_available` region filter |
| Code Quality | RESOLVED-FIXED | 3 | 3 | 0 | "sheds every session" sites restated; duplicate Alert line; TOC |
| DRY | RESOLVED-FIXED | 2 | 2 | 0 | TODO not_connected/O-11 paragraph; MH runbook-half bullet marked DONE |
| Operations | RESOLVED-FIXED | 3 | 3 | 0 | `--owner` on mh.sh calls; `--tail=-1` sweep; `manual` MH cleanup |
| Infrastructure | RESOLVED-FIXED | 6 | 6 | 0 | stuck-terminating per verb, cordon/drain flags, GC 14 duplicate via kubectl, MH_TOKEN cases, timeout row |
| Media Handler | RESOLVED-FIXED | 4 | 4 | 0 | upgrade-stuck grep, `shed_completed`, orphan shed in MH 25 relief |
| Meeting Controller | RESOLVED-DEFERRED | 3 (+3 siblings, round 2) | 3 | 1 | graceful vs crash MC rollout split; F2 code comments/log string deferred (Lead ruling) |
| Global Controller | RESOLVED-DEFERRED | 4 (+1 sibling, round 2) | 4 | 1 | supersede rule, per-line fields, postgres-0 psql, `--tail=-1` incl. ac-service-deployment.md; psql class deferred |

Note: the GC and MC panel reviewers could not receive implementer replies (messaging failure); fresh GC/MC verifier sessions checked the fixes in tree, escalated 5 residual sibling sentences/lines, which were fixed in round 2 and verified by the Lead.

Lead ruling (MC F2 code part): deferred — task is documentation-only (tier=light, Gate 1 skipped on that basis); the ERROR string is a `tracing::` light-exclusion surface; fixing only the comments would split comment from log string. Accepted by meeting-controller.

---

## Accepted Deferrals

**Each entry here is an issue the devloop chose NOT to fix.** Every bullet is a cost shift: the implementer didn't pay the fix-now cost, so a future reader will pay fix-later cost + tracking overhead. List only what was actually deferred — not "follow-ups" or "future improvements" or "potential extractions." If something was fixed, it doesn't belong here.

**Tech debt entries themselves live in `docs/TODO.md`. This section holds only pointers to those entries.** Do not create a `TODO.md` at the repo root or anywhere else — there is exactly one `docs/TODO.md` for the whole project. Do not inline the debt body here — multi-line entries belong in `docs/TODO.md`, not in this section.

Each pointer is exactly one bullet of the form `- \`docs/TODO.md\` §SECTION-NAME — one-line hook (≤80 chars)`. If you wrote more than one line per entry, you're writing it in the wrong file — move the body to `docs/TODO.md` and leave only the pointer here.

Examples:

```
- `docs/TODO.md` §Observability Debt — orphan recording-site audit follow-up
- `docs/TODO.md` §Cross-Service Duplication (DRY) — extract record_token_refresh_metrics
```

or:

```
- (none surfaced in this devloop)
```

**This devloop:**
- `docs/TODO.md` §Documentation Hygiene — runbook psql via distroless GC / `$DATABASE_URL` (~67 sites)
- `docs/TODO.md` §Code Quality — MC cut-off log/comment say "until handler restarts" (MC F2)

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `{start_commit}`
2. Review all changes: `git diff {start_commit}..HEAD`
3. Soft reset (preserves changes): `git reset --soft {start_commit}`
4. Hard reset (clean revert): `git reset --hard {start_commit}`
5. For schema changes: rollback requires a forward migration — `git reset` alone is insufficient if migrations were applied
6. For infrastructure changes: may require `skaffold delete` or `kubectl delete -f` if manifests were applied
7. **Safe-revert unit** (answer explicitly, even if "the whole commit"): can any part of this diff be reverted or cherry-picked on its own, or does a partial revert reconstruct a state worse than either endpoint (e.g. a security fix split from the change that made it necessary, or a client/server/alert-rule set that must move together)? If partial reverts are unsafe, name the unit and the safe direction. **Answer**: `{the whole commit | <unit + safe direction>}` (This converts silence into a visible unanswered slot; it cannot distinguish a checked answer from a reflexive one.)

---

## Issues Encountered & Resolutions

### Issue 1: {Brief title}
**Problem**: {What went wrong}
**Resolution**: {How it was fixed}

### Issue 2: {Brief title}
**Problem**: {What went wrong}
**Resolution**: {How it was fixed}

{Add more issues as needed, or "None" if no issues}

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
