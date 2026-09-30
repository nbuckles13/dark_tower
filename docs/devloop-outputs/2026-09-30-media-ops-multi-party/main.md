# Devloop Output: Media operations for multi-party (story hear-each-other task 18)

**Date**: 2026-09-30
**Task**: Runbook scenarios (MH 18/19, MC 17 + MC 16 rotation arm, client F16/F17), MH deployment runbook additions, MHMediaEgressBudgetExhausted alert, 24h watch list (R-9, R-19, R-20, R-21, R-24, R-27, R-29)
**Specialist**: operations
**Mode**: Agent Teams (v2) — full panel; tier=light escalated to Gate-1 round (K8s-manifest comment edits)
**Branch**: `feature/hear-each-other`
**Duration**: ~2h

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `c185acf683b7859aa224d2b785551ee3a62a30ea` |
| Branch | `feature/hear-each-other` |
| Lead Model | `claude-opus-5-5[1m]` |

---

## Loop State (Internal)

<!-- This section is maintained by the Lead for state recovery after interruption. -->
<!-- Do not edit manually - the Lead updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `spawned` |
| Implementing Specialist | `operations` |
| Tier | `light — run-story manifest tier=light; ESCALATED to Gate-1 panel round at implementation (plan reaches --light exclusion surface: comment-only edits under infra/services/mh-service/ K8s manifests)` |
| Iteration | `1` |
| Security | `spawned` |
| Test | `spawned` |
| Observability | `spawned` |
| Code Quality | `spawned` |
| DRY | `spawned` |
| Operations | `spawned` |
| Semantic Guard | not spawned (docs + Prometheus rule; no check surface) |

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
| `docs/runbooks/mh-incident-response.md` | Mine | — |
| `docs/runbooks/mc-incident-response.md` | Mine | — |
| `docs/runbooks/mh-deployment.md` | Mine | — |
| `docs/runbooks/client-dev-local.md` | Mine | — |
| `infra/docker/prometheus/rules/mh-alerts.yaml` | Mine | — |
| `infra/docker/prometheus/rules/mc-alerts.yaml` (comment-only) | Mine | — |
| `docs/user-stories/2026-09-21-hear-each-other.md` | Mine | — |
| `docs/TODO.md` | Mine | — |
| `docs/devloop-outputs/2026-09-30-media-ops-multi-party/main.md` | Mine | — |
| `docs/observability/alerts.md` | Not mine, Minor-judgment | observability |
| `docs/observability/metrics/mh-service.md` (alert-status + teardown-rule re-points) | Not mine, Minor-judgment | observability |
| `docs/observability/metrics/mc-service.md` (one MH 19 forward reference) | Not mine, Minor-judgment | observability |
| `infra/grafana/dashboards/mh-media.json` (panel descriptions only, three panels) | Not mine, Minor-judgment | observability |
| `infra/services/mh-service/kustomization.yaml` (comment-only) | Not mine, Minor-judgment | infrastructure |
| `infra/services/mh-service/mh-0-deployment.yaml` (comment-only, undo premise) | Not mine, Minor-judgment | infrastructure |
| `infra/services/mh-service/mh-1-deployment.yaml` (comment-only, undo premise) | Not mine, Minor-judgment | infrastructure |
| `infra/services/mh-service/config.env` (comment-only; comments are not ConfigMap data, so the content hash and pods are unchanged) | Not mine, Minor-judgment | infrastructure |
| `crates/mh-service/src/config.rs` (doc comment only) | Not mine, Minor-judgment | media-handler |
| `crates/env-tests/tests/28_mh_egress_admission.rs` (ALERT-JOIN-EMPTY assertion, @test's Gate-1 ask) | Not mine, Minor-judgment | test |
| `crates/env-tests/src/fixtures/alert_rules_loaded.rs` (`alert_expr_in_yaml` + unit test, so the probe is derived from the rule; Gate-3 finding from test/code-reviewer/dry) | Not mine, Minor-judgment | test |
| `docs/runbooks/gc-incident-response.md` (Scenario 3 ratchet-recovery clause, ops Gate-3 F5) | Mine | — |
| `docs/observability/metrics/client.md` (reviewer edit by @observability: stale `kek_updates_total` Labels bullet) | Not mine, Minor-judgment | observability |

---

## Planning

### Gate 1 — ESCALATED from tier=light (plan reaches K8s-manifest exclusion surface, comment-only)

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |

**Lead rulings (2026-09-30):** (1) teardown-never-arrives rule — not in the task prompt nor story line 138's alert list; do NOT invent it; re-point all five forward references at existing coverage and record the deliberate absence (implementer default accepted). (2) kustomization.yaml comment-only edit accepted; invariant completion also covers the stale undo premise in `infra/services/mh-service/mh-{0,1}-deployment.yaml` comments.

**Problem, in mechanism language.** Story 2 shipped the behaviours below, and the
runbooks and alert rules do not yet describe them. (1) MH's egress admission refuses
a generation-advancing policy apply when installed edges would exceed the derived
stream ceiling. `mh_media_stream_admission_rejection_ratio` exists with its threshold
gauge, and no rule reads them. (2) MH drops a server-muted sender at ingress
(`server_muted`), programmed by the MC registration snapshot. That contract authorizes
on caller class only: scope plus service_type, with no per-meeting binding. (3) MC
rotates the KEK on leave (debounced per W) and on sender-space exhaustion (immediate).
(4) ADR-0038 made ConfigMaps content-addressed, which changes what
`kubectl rollout undo` restores.

**Plan (docs + one alert rule; no code, no manifest semantics, no GSA):**
1. `mh-incident-response.md`: add Scenario 18, Media Egress Budget Exhaustion. It gets
   a scoped leading-indicator statement (edge occupancy leads the ceiling arms;
   nothing leads congestion or measured bandwidth until story 5; nothing on edges
   leads the empty-meeting arm) and three arms with three remedies. It carries the
   never-tune-away rule and cross-references Scenario 13. Add Scenario 19, Server Mute
   Not Taking Effect At Ingress, with four arms in cost order and MC 21 as the MC-side
   companion. Update the TOC. Correct Scenario 17's "no egress budget in this build"
   premise, which task 8 falsified.
2. `mc-incident-response.md`: expand Scenario 17 in place (trigger fork, recorded
   no-eviction decision and residual, `mc_meeting_sender_ids_issued_max`). The namespace
   size is cited from `sender_id.rs`, not restated, per task 17's correction. Add to
   Scenario 16 a rotation arm (reason-first; leave vs join correlation; never-sent vs
   sent-not-acknowledged push outcomes; a client retention-expiry rung) and the
   slot-cap / static-fill forks. Re-point Scenario 20's "task 18's rule" at the real
   coverage.
3. `mh-deployment.md`: add a Config-failure triage section. It covers the MH triad and
   the key-ordering rule, re-derived against ADR-0038 §2: undo points the template back
   at the previous hash-suffixed ConfigMap, which exists only because nothing prunes
   generations. It also records the MH_MAX_STREAMS two-deploy retirement. Fold the
   story-2 required keys into the table. Add the multi-party blast radius and the
   mandated operator-lever statement (corrected wording). Fix the two stale
   "undo never restores the ConfigMap" sentences.
4. `client-dev-local.md`: add F16 (rotation signature) and F17 (static fill).
5. `mh-alerts.yaml`: add `MHMediaEgressBudgetExhausted` (warning, 10m, bare gauge vs
   gauge) with its ADR-0031 block and fire/apply table. Update the header omission
   note and the stale "pending" / "join outage" wording in the headroom block. Make
   the matching wording fixes in `alerts.md` (promote to `####`, triage-step sentence),
   the MH catalog, and the dashboard panel description.
6. Story Operations section: add the 24h watch list in the mandated order. `docs/TODO.md`:
   close the gauge-to-gauge entry and correct the MH_MAX_STREAMS premise.

**Invariant completed across all instances:** every "task 18's rule / pending" forward
reference is resolved or re-pointed. Every "undo restores refs, not the ConfigMap"
sentence is re-derived.

**Scope question raised to Lead:** the "teardown-never-arrives rule" that five sites
attribute to task 18 is not in the task prompt. The default is no rule and re-pointed
references (see Issues).

**Gate-1 feedback folded in (security, code-reviewer, observability, test, operations, dry-reviewer):**
the classification rows above; every task-18 forward reference and every "undo never restores the
ConfigMap" premise is enumerated and fixed (see Implementation Summary). The teardown-never-arrives
rule is recorded as an absence per the Lead's ruling (1) and, per @observability, as an OPEN
obligation in `docs/TODO.md` §Observability Debt ("No leading indicator for registered-meeting
exhaustion"), not a design decision. The leading-indicator statement has one home (MH 18), which the
inventory points at. The ADR-0038 re-derivation cites `mc-deployment.md`'s "Fast rollback lever" and
ADR-0038's step-3 notes rather than forking a copy. The runbook_url fragment gap is filed in
`docs/TODO.md` (@test 2c). ALERT-JOIN-EMPTY was added to env-test 28 (@test 3).

**Heading → anchor pairs (GFM slug, derived from the written headings; verified by script against every changed file):**

| Heading | Anchor |
|---|---|
| `### Scenario 18: Media Egress Budget Exhaustion` (mh-incident-response.md) | `#scenario-18-media-egress-budget-exhaustion` (the new rule's `runbook_url`) |
| `### Scenario 19: Server Mute Not Taking Effect At Ingress` | `#scenario-19-server-mute-not-taking-effect-at-ingress` (MC 21 and mh-deployment link) |
| `#### Rotation arm — silence after someone left` (mc-incident-response.md) | `#rotation-arm--silence-after-someone-left` |
| `## Config-failure triage: three signals, and \`kubectl logs\` is wrong for all three` (mh-deployment.md) | `#config-failure-triage-three-signals-and-kubectl-logs-is-wrong-for-all-three` |
| `### Operator levers: there are none for media` | `#operator-levers-there-are-none-for-media` |

Unchanged headings (anchors intact): MH 13, MH 17, MC 15, MC 16, MC 17, MC 20 and MC 21.


---

## Pre-Work

None

---

## Implementation Summary

### Runbooks (R-29)
| Item | Change |
|------|--------|
| MH Scenario 18 (new) | Egress budget exhaustion. The leading-indicator statement is scoped and has its single home here: edge occupancy leads the ceiling arms; congestion-withholding, the ADR-designated indicator, is uninstrumented until story 5; nothing leads the cap arm. Three arms: A load, B unreclaimed-edge floor (never tuned away), C empty-meeting growth. Cross-refs Scenario 13 |
| MH Scenario 19 (new) | Server mute not taking effect at ingress. Four arms in cost order; arm 2 is the ownership gap (caller class, no per-meeting binding, plaintext channel) |
| MH Scenario 13 / 17 | Re-pointed the task-18 forward references; corrected Scenario 17's "no budget in this build" premise |
| MC Scenario 17 | Expanded in place. Trigger fork against 1/W, the recorded no-eviction decision and its residual, `mc_meeting_sender_ids_issued_max`. The namespace size is cited, not restated. No host-removal lever exists (verified) |
| MC Scenario 16 | Added a first fork (slot-cap rejection, static fill) and a rotation arm (reason + leave/join correlation, never-sent vs sent-not-acknowledged, client-only retention-expiry rung) |
| MC Scenario 20 / 21 | Re-pointed the teardown rule; MC 21 links to MH 19 |
| mh-deployment.md | Config-failure triage (MH triad, key-ordering rule re-derived against ADR-0038 §2, MH_MAX_STREAMS). Story-2 keys folded into the required-keys table. Multi-party blast radius. Operator-lever statement. Pre-ADR-0038 undo premise fixed at the two stale sites |
| client-dev-local.md | F16 (rotation silence) and F17 (static fill) |

### Alerting
`MHMediaEgressBudgetExhausted` (warning, 10m, bare gauge vs gauge) with its ADR-0031 block and FIRE/APPLY table, plus the `alerts.md` `####` entry (byte-identical PromQL). The stale "pending" and "join outage" wording is fixed in the rule header, the headroom block, the catalogs and three dashboard panels. The teardown-never-arrives rule is not invented; it is an open obligation in `docs/TODO.md` §Observability Debt. Env-test 28 gained ALERT-JOIN-EMPTY, which proves both gauge-vs-gauge alert joins are non-empty on every MH instance.

### Additional Changes
The undo-premise invariant is completed in the comments of `config.env`, `mh-{0,1}-deployment.yaml` and `kustomization.yaml`, and in `docs/TODO.md`. The gauge-to-gauge TODO is closed. The runbook-fragment guard gap is filed. The 24h watch list is in the story's Operations section.

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

Gate 2 (`DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh`, final tree): exit 0.

| Layer | Result | Duration |
|-------|--------|----------|
| 1 Compile | OK | 5s |
| 2 Format | OK | 4s |
| 3 Guards | OK | 165s |
| 4 Test | N/A (proto intentional-gap placeholder; cargo + nx tests passed) | 258s |
| 5 Lint | OK | 2s |
| 6 Audit | N/A (proto placeholder; cargo/pnpm audit + buf breaking passed) | 2s |
| 7 Env-tests | OK (Rust env-tests incl. 28_mh_egress_admission + browser E2E) | 862s |

TOTAL_RESULT=N/A (worst child = documented proto placeholder N/A). Fast check before review: same shape.

---

## Code Review Results

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 2 | 2 | 0 | S1 false "encrypted key id" claim (MH 19 + story R-9); S2 inverted W security direction (MC 17) |
| Test | RESOLVED-DEFERRED | 3 | 2 | 1 | ALERT-JOIN-EMPTY probe added; probe now derived from rule expr via `alert_expr_in_yaml`; runbook_url fragment guard deferred to TODO |
| Observability | RESOLVED-FIXED | 3 | 3 | 0 | MC 17 fleet-average caveat; MC 16 Rung 0; MH 19 Arm 3 per-meeting evidence (no meeting label) |
| Code Quality | RESOLVED-FIXED | 3 | 3 | 0 | env-test SSoT; MC 16 step order; manifest comment wrap. ADR-0031 / ADR-0036 §11 / ADR-0038 §2 compliant |
| DRY | RESOLVED-FIXED | 5 | 5 | 0 | probe SSoT; MH 19 vs MC 21 dedup; leading-indicator single home (MH 18); MH_MAX_STREAMS row; MC 17 multipliers. No extraction opportunities |
| Operations | RESOLVED-FIXED | 5 | 5 | 0 | ADR-0038 undo re-derivation (retention is the whole safety); join-outage siblings; budget-raise bound; per-pod restart (MH 18, mh-service catalog, GC 3) |

Semantic Guard: not spawned (docs + Prometheus rule + comments + env-test fixture; no check surface).

Gate 1 (escalated from tier=light): all six confirmed; classification guard `STATUS=OK`.

---

## Accepted Deferrals

- `docs/TODO.md` §Observability Debt — runbook_url fragments are not resolved by any guard

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

### Issue 1: Prompt premises that shipped code falsifies
**Problem**: Three mandated statements did not match the shipped code. (a) "healthy meetings reinstall on the next tick": MC's §8 re-assert cadence has not shipped (story 4), so after a restart meetings reinstall only on a rejoin. (b) "all three present identically at the rejection ratio": `rejected_meeting_cap` is decided on the register path and never enters the ratio. (c) "self-limited to once per 65,536 admissions": task 17 corrected this to the allocator's namespace size.
**Resolution**: Each is written as shipped behaviour with a dated premise-correction note (MH 18 Arm B and Step 1; MC 17).

### Issue 2: "The teardown-never-arrives rule is task 18's" at five-plus sites
**Problem**: The rule was assigned by earlier observability text, not by the story or the prompt.
**Resolution**: Lead ruling (1): no rule was invented. Every site is re-pointed at one `docs/TODO.md` §Observability Debt entry that records it as an open obligation (@observability).

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
