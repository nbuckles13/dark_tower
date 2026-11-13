# Devloop Output: Browser MH Replacement Resilience Spec (C3)

**Date**: 2026-11-13
**Task**: C3 — Playwright spec packages/web-app/e2e/mh-replacement.spec.ts (crash, planned-transfer, stranded A/B legs) + README catalog entry
**Specialist**: client (paired with test)
**Mode**: Agent Teams (v2) — full panel; Gate-1 SKIPPED — tier=light (e2e only, no production code)
**Branch**: `feature/mh-failure-transfer`
**Duration**: ~2h40m (incl. ~47m Gate-2 run)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `4188141cbf69375981da791765f95f8176f92472` |
| Branch | `feature/mh-failure-transfer` |
| Lead Model | `claude-opus-5-5` |

---

## Loop State (Internal)

<!-- This section is maintained by the Lead for state recovery after interruption. -->
<!-- Do not edit manually - the Lead updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `a56b41e6 (client)` |
| Implementing Specialist | `client` |
| Tier | `light — e2e only, no production code (manifest tier=light)` |
| Iteration | `1` |
| Security | `a0be2030 — CLEAR` |
| Test | `paired-test — RESOLVED-FIXED` |
| Observability | `aec952f6 — RESOLVED-FIXED` |
| Code Quality | `a4a22947 — RESOLVED-FIXED` |
| DRY | `a381173f — RESOLVED-FIXED` |
| Operations | `a48f241c — RESOLVED-FIXED` |
| Semantic Guard | `not spawned (e2e test code only; no check surface)` |
| Paired | `paired-test (replaces Test slot)` |

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
C3 of story `docs/user-stories/2026-10-31-survive-handler-loss.md`: browser-level proof (R-1, R-2, R-3, R-20, R-39 browser legs) that a meeting survives MH loss — crash (kill -KILL), planned transfer (drain with a target), and stranded A/B (survivor blocked) — per ADR-0039 §4/§9 and test-gates.md "Browser".

### Scope
- **Service(s)**: none (e2e harness + Rust env-test fixture twin + Layer-7 browser budget)
- **Schema**: No
- **Cross-cutting**: No production code; touches test-owned `crates/env-tests` fixture and operations-owned Layer-7 budget (Minor-judgment, reviewed by owners on the panel)

### Debate Decision
NOT NEEDED - design fixed by ADR-0039 and the 2026-10-28 debate's test-gates.md.

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
| `packages/web-app/e2e/mh-replacement.spec.ts` (new) | Mine | — |
| `packages/web-app/e2e/mhLeg.ts` (new) | Mine | — |
| `packages/web-app/e2e/mhLegRules.ts` (new) | Mine | — |
| `packages/web-app/e2e/instanceCounters.ts` | Mine | — |
| `packages/web-app/e2e/partial-connectivity.spec.ts` | Mine | — |
| `packages/web-app/e2e/mhPool.ts` | Mine | — |
| `packages/web-app/e2e/s1Diagnostic.ts` | Mine | — |
| `crates/env-tests/src/fixtures/mh_pool.rs` | Not mine, Minor-judgment | test |
| `scripts/layer7.sh` | Not mine, Minor-judgment | operations |
| `docs/runbooks/devloop-validation.md` | Not mine, Minor-judgment | operations |
| `packages/web-app/e2e/mcMetrics.ts` | Mine | — |
| `packages/web-app/e2e/configEnv.ts` | Mine | — |
| `packages/web-app/e2e/fixtures.ts` | Mine | — |
| `packages/web-app/e2e/README.md` | Mine | — |
| `packages/web-app/tests/mhLeg.test.ts` (new) | Mine | — |
| `docs/TODO.md` (DRY extraction entry) | Mine | — |

---

## Planning

### Gate 1 — SKIPPED (tier=light; reason: e2e only, no production code)

**Problem, in mechanism language.** When the MH process carrying a meeting dies (SIGKILL, in-place restart) or drains (SIGTERM), the browser client must keep or regain pairwise audio through MC's handler-set reconcile alone: crash = MC declares Dead at N, pushes a set without the dead ref (survivor arm), then a set with a GC-granted NEW ref (break-before-make); planned = MC pushes a set adding the grant Y while X is still present, then a set without X (make-before-break), with no client-observed loss. The spec proves both from each participant's OWN bus (`handlerSets`, `reconnectingOverlays`, `transportFaults`) and tone verdicts, against a run-owned MH (mh.sh `signal`/`drain` are owner-gated).

**Plan.**
1. `e2e/mhLeg.ts` (new; the browser twin of `MhPool::run_leg` + `owned_placement.rs` + the removal settle C3 owns per `mhPool.ts`):
   - PURE (node-tested): `placementAttempts(m, s, ceiling)` / `placementHit` / `PLACEMENT_MISS_CEILING` / `OWNED_MH_NOT_PLACED` (twins of owned_placement.rs, drift-pinned); `rebaseline` / `replicaSettled` / `SETTLE_RECOMPUTES` (twins of gc_fleet.rs, drift-pinned); `resilienceCeilings({N, idle, settle, retryBudget, hear})` → `{ survivorMs, replacementMs }` (failure-only: `max(N, idle) + settle + hear` and `+ retryBudget`); `newRefs(pre, set)`; `setSequenceIndices` for add-then-remove.
   - IO: `runLeg(body)`: `assertBaseline(removalSettleBudget)` + `awaitNoMcMeetings` BEFORE; body; on EVERY exit path (primary error captured, never swallowed) the asserted `pool.teardown()`, `waitRemovedMhUnselectable`, `assertBaseline` AFTER; errors aggregated. `upScraped(pool)` (up, then Prometheus `up{job="mh-service"}` for `<pod_ip>:8083` == 1 before any baseline). `placeOnOwned(browser, probeIndex, ownedAddress, attempts)`: fresh context + meeting + recorded `joinMember` per attempt until the join-time set holds the owned address with exactly `s` members; a miss is closed at once; the miss list is printed on `owned-mh-not-placed`.
2. `e2e/mcMetrics.ts`: the new PromQL (MH up per instance, GC staleness/selectable/recompute per replica, GC replica set via `up{job="gc-service"}`, MC active meetings, MC replacement retry budget gauge). Reads only; per-instance maps; no new series.
3. `e2e/configEnv.ts`: `readResilienceTimings()` — `MC_MH_HEARTBEAT_TIMEOUT_MS`, `MC_MEDIA_CONNECT_SETTLE_MS` (mc), `MH_MAX_IDLE_TIMEOUT_MS` (mh), `GC_MH_INITIAL_HANDLERS_PER_MEETING` (gc), GC health tick from `generic_health_checker.rs` (Rust GC_HEALTH_TICK_SRC). Fail closed; Kind overlays patch none of these keys.
4. (as built) The pure kernels live in `e2e/mhLegRules.ts` (no `./env`, node-testable); `instanceForPodIp` (Rust `metrics::instance_for_pod_ip` twin) joins the pure `e2e/instanceCounters.ts`; `connectedUrls` moves from `partial-connectivity.spec.ts` into `fixtures.ts` (one home for the connected-only premise), and that spec calls it.
5. `e2e/fixtures.ts`: export the existing hearing liveness bound (`HEAR_SENDER_TIMEOUT_MS`) so the ceiling composes it rather than restating it.
5. `e2e/mh-replacement.spec.ts`, three tests, each in `runLeg`:
   - CRASH: up X (scraped); place on X via probe A (unblocked); B joins with `blockHandlers=[survivor]` (A–B edges can only be on X; MC colocation keeps C's edges there too — non-vacuity), then C (unblocked). Pre-kill: union of refs per member; prove B connected only to X. `pool.kill(slot)` (restartedInPlace asserted). Assert: overlay observed (hard, on the unblocked members); B↔A, B↔C, A↔C tone within ceilings; every member's final set holds a ref not in its pre-kill union; record (never gate) the dark window.
   - PLANNED: up X, place on X, all three unblocked join; snapshot overlay/fault array lengths (Array.isArray proven); up Y (scraped); `pool.drain(X)` (blocking; mh.sh's own derived bound). Assert per member: index(first set ∋ old ∧ ∋ new) < index(first later set ∌ old), new ref ∉ pre-drain union; final set = pre − old + new; every pair hears within the ceiling; THEN zero new overlay / transportFault entries.
   - STRANDED: up X, place via probe C (unblocked); A and B join with `blockHandlers=[survivor]`; prove A, B connected only to X; kill; A's and B's final sets hold a new ref; A↔B hear within the replacement ceiling.
   - Every leg: `closeMembers` then `awaitMeetingsDrained` inside the body, so teardown never strands a live meeting on a run-owned MH.
6. `tests/mhLeg.test.ts`: pure kernels + drift pins (Rust constants, PromQL tokens).
7. `e2e/README.md`: catalog entry (what each leg asserts), env (`DT_MH_SH`, `DT_CLUSTER_NAME`), wall-clock note.

No production code, no GSA, no manifests, no config keys, no new series, no tokens on the bus (handler URLs + refs only).

---

## Pre-Work

None

---

## Implementation Summary

### Spec (`e2e/mh-replacement.spec.ts`)
Three legs, each wrapped by the `legTest` fixture (`e2e/mhLeg.ts`): placement on a run-owned MH by probe meetings (TS twin of `owned_placement.rs`); CRASH asserts overlay observed, a new handler_ref in every final set, pairwise tone within derived failure-only ceilings, dark window annotated only; PLANNED asserts add-then-remove set sequence, zero overlay/transportFault, tone continues; STRANDED blocks the survivor for A/B and asserts new refs + A↔B hearing.

### Harness
- `mhLeg.ts` / `mhLegRules.ts`: leg scaffold (BEFORE baseline + `runLeakVerdict` + no live MC meetings; AFTER as fixture teardown: leave → meetings drained → owner teardown → removed-MH GC settle → baseline) and pure kernels with Rust drift pins (`tests/mhLeg.test.ts`).
- `configEnv.ts`: `readResilienceTimings` (fail-closed incl. Kind-overlay patch scan), shared `readRustU64Const`.
- `fixtures.ts`: shared `pollConverge` kernel under `pollBus`; `connectedUrls`, `JOIN_LIVENESS_MS`, `HEAR_SENDER_TIMEOUT_MS` exported.
- `mcMetrics.ts` / `s1Diagnostic.ts` / `mhPool.ts` / `instanceCounters.ts`: per-instance PromQL reads (no new series), `servingInstances(job)`, scrape-job names single home, `servingSlotCount`, `runLeakVerdict`, `instanceForPodIp`.

### Additional Changes
- `crates/env-tests/src/fixtures/mh_pool.rs`: `wait_scraped_since` (post-kill scrape must post-date the restart) and scrape-settle spacing for `await_no_mc_meetings` — same holes the review found in the TS twin.
- `scripts/layer7.sh` + runbook §6.7: `DEVLOOP_BROWSER_E2E_TIMEOUT` default 1500 → 3000 s. Measured at Gate 2: browser suite 33 passed in 13.9 min (834 s; C3 file 5.2 min) — well under both budgets; the 3000 s value was set from a pre-run projection (~30 min) and is now over-provisioned (see Issues).
- `e2e/README.md`: catalog section, env (`DT_MH_SH`, `DT_CLUSTER_NAME`), wall-clock projection, mid-leg reclaim recipe.

---

## Files Modified

```
crates/env-tests/src/fixtures/mh_pool.rs          |  52 ++-
 docs/TODO.md                                      |   2 +
 docs/runbooks/devloop-validation.md               |   2 +-
 packages/web-app/e2e/README.md                    | 129 +++++-
 packages/web-app/e2e/configEnv.ts                 | 146 ++++++-
 packages/web-app/e2e/fixtures.ts                  |  56 ++-
 packages/web-app/e2e/instanceCounters.ts          |  16 +
 packages/web-app/e2e/mcMetrics.ts                 | 126 ++++++
 packages/web-app/e2e/mh-replacement.spec.ts       | 457 ++++++++++++++++++++
 packages/web-app/e2e/mhLeg.ts                     | 495 ++++++++++++++++++++++
 packages/web-app/e2e/mhLegRules.ts                | 216 ++++++++++
 packages/web-app/e2e/mhPool.ts                    |  38 +-
 packages/web-app/e2e/partial-connectivity.spec.ts |  39 +-
 packages/web-app/e2e/s1Diagnostic.ts              |   4 +
 packages/web-app/tests/mhLeg.test.ts              | 330 +++++++++++++++
 scripts/layer7.sh                                 |   8 +-
 16 files changed, 2067 insertions(+), 49 deletions(-)
```

---

## Devloop Verification Steps

Gate 2: `DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` → `TOTAL_RESULT=OK` (2844 s). Log: `/tmp/layer-all-c3.log`.

| Layer | Result | Duration (s) |
|-------|--------|--------------|
| 1 Compile | OK | 7 |
| 2 Format | OK | 5 |
| 3 Guards | OK | 127 |
| 4 Test | OK (5520 Rust tests + node tests) | 136 |
| 5 Lint | OK | 1 |
| 6 Audit | OK (dep audit SKIPPED-NO-DIFF no-dep-changes) | 5 |
| 7 Env-tests | OK — `env-tests-passed`, `browser-e2e-passed` (33 passed, 13.9 min; CRASH 2.3m, PLANNED 1.4m, STRANDED 1.5m) | 2563 |

Fast check (pre-review) also green.

---

## Code Review Results

### Security Specialist
**Verdict**: CLEAR
**Findings**: 0 found, 0 fixed, 0 deferred

No findings. No tokens/identity on bus or in annotations; kill via T-1 debug-container injector (R-28, same pod_uid asserted); only run-owned MHs touched; no new exec surface. Pre-existing residual unchanged: retained Playwright traces carry live tokens (documented, gitignored).

### Test Specialist
**Verdict**: RESOLVED-FIXED
**Findings**: 4 found, 4 fixed, 0 deferred

(paired-test) F1 vacuous post-kill scrape wait (in-place restart keeps pod IP) — now requires timestamp(up) after kill, TS + Rust twin `wait_scraped_since`; F2 test timeout extended by placement attempts × named join bounds; F3 teardown as test-scoped fixture so its failure reports separately; F4 Layer-7 browser budget projected, DEVLOOP_BROWSER_E2E_TIMEOUT 1500→3000 s.

### Observability Specialist
**Verdict**: RESOLVED-FIXED
**Findings**: 2 found, 2 fixed, 0 deferred

F1 stale post-kill admission baseline (ADR-0032 re-baseline) — fixed as above; F2 `awaitNoMcMeetings` two reads now one scrape-settle apart (TS + Rust run_leg). Dark window annotated, never gated (ADR-0036 §10); no new series.

### Code Quality Reviewer
**Verdict**: RESOLVED-FIXED
**Findings**: 9 found, 9 fixed, 0 deferred

Poll-loop duplication (pollBus / pollProm), awaitMcMeetingsZero collapse, overlayCount/faultCount rename, options-object for expectPairsResume, carried gc_fleet why-comment, trimmed exports, removed connectedSet pass-through, reclaim fallback carries error, shorthand nit.

### DRY Reviewer
**Verdict**: RESOLVED-FIXED

**True duplication findings** (entered fix-or-defer flow):
6 found, 6 fixed: pollBus reuse, MH_UP_PROMQL reuse, servingInstances(job), readRustU64Const, servingSlotCount, shared pollConverge kernel.

**Extraction opportunities** (appended to `docs/TODO.md`):
- `docs/TODO.md` §Cross-Service Duplication (DRY) — re-point mcMetrics poll loops onto fixtures.pollConverge

### Operations Reviewer
**Verdict**: RESOLVED-FIXED
**Findings**: 3 found, 3 fixed, 0 deferred

F1 MH leak on Playwright timeout — AFTER moved to fixture teardown + BEFORE `runLeakVerdict`; F2 fail-closed check that Kind overlays don't patch resilience keys; F3 wall-clock projection + mid-leg reclaim recipe in README, browser budget 1500→3000 s (layer7.sh + runbook §6.7).

### Semantic Guard Reviewer
Not spawned — diff is e2e/test-harness code only; no semantic-guard check surface.

---

## Accepted Deferrals

- (none surfaced in this devloop)

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `4188141cbf69375981da791765f95f8176f92472`
2. Review all changes: `git diff 4188141cbf69375981da791765f95f8176f92472..HEAD`
3. Soft reset (preserves changes): `git reset --soft 4188141cbf69375981da791765f95f8176f92472`
4. Hard reset (clean revert): `git reset --hard 4188141cbf69375981da791765f95f8176f92472`
5. For schema changes: rollback requires a forward migration — `git reset` alone is insufficient if migrations were applied
6. For infrastructure changes: may require `skaffold delete` or `kubectl delete -f` if manifests were applied
7. **Safe-revert unit** (answer explicitly, even if "the whole commit"): can any part of this diff be reverted or cherry-picked on its own, or does a partial revert reconstruct a state worse than either endpoint (e.g. a security fix split from the change that made it necessary, or a client/server/alert-rule set that must move together)? If partial reverts are unsafe, name the unit and the safe direction. **Answer**: `the whole commit` for the spec + harness; the `mh_pool.rs` scrape fixes and the layer7 budget bump are independently revertible (reverting the budget is safe at the measured 834 s). (This converts silence into a visible unanswered slot; it cannot distinguish a checked answer from a reflexive one.)

---

## Issues Encountered & Resolutions

### Issue 1: `team-lead` not addressable
**Problem**: Teammates could not SendMessage `team-lead` in this session.
**Resolution**: Lead redirected all reports to `main`.

### Issue 2: Layer-7 browser budget raised on projection, measured far lower
**Problem**: 1500 → 3000 s was set from a ~30 min projection; Gate 2 measured 834 s for the whole browser suite.
**Resolution**: Recorded here; 3000 s is safe but over-provisioned (slower failure detection only). Retuning needs a fresh Gate 2, so it is left for the story-close measurement pass rather than re-running a 47-min gate for a non-correctness value.

---

## Lessons Learned

1. An in-place MH restart keeps its pod IP, so any "scraped up" wait must be bounded below by the restart time — the same hole existed in the Rust twin.
2. Playwright per-leg cleanup belongs in fixture teardown (afterHooksSlot), not inline after the body, or a timeout leaks run-owned MHs.
3. Measure before raising a CI budget: the projection overestimated by ~2x.

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
