# Devloop Output: Manual test plan for the story-2 N+1 demo (written and run) + observeWindow sampling flake fix

**Date**: 2026-09-30
**Task**: Write and run `docs/user-stories/2026-09-21-hear-each-other-manual-test-plan.md` (R-32, story task #19); fix the `observeWindow` fixed-window vacuity flake in `packages/web-app/e2e/receiveEvidence.ts`
**Specialist**: test
**Mode**: Agent Teams (v2) — full panel; Gate-1 SKIPPED — manual test plan document, run and recorded
**Branch**: `feature/hear-each-other`
**Duration**: ~2h

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `fe96076abf800f195da1dbfdb353b62052733c13` |
| Branch | `feature/hear-each-other` |
| Lead Model | `claude-opus-5-5[1m]` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` (test) |
| Implementing Specialist | `test` |
| Tier | `light — manual test plan document, run and recorded` |
| Iteration | `1` |
| Paired | `paired-client`, `paired-infrastructure` (task: "Pair with client and infrastructure") |
| Security | `security` |
| Test | `test` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Semantic Guard | not spawned — diff is docs + e2e test harness only (no check surface) |

---

## Task Overview

### Objective
See task-19 prompt (story `docs/user-stories/2026-09-21-hear-each-other.md`, task 19, R-32).

### Scope
- **Service(s)**: none (docs + web-app e2e harness)
- **Schema**: No
- **Cross-cutting**: No

### Debate Decision
NOT NEEDED

---

## Cross-Boundary Classification

(Only Domain-judgment / GSA rows are listed; none expected. Implementer fills the planned file list in §Planning.)

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `docs/user-stories/2026-09-21-hear-each-other-manual-test-plan.md` | Mine | — |
| `packages/web-app/e2e/README.md` | Mine | — |
| `packages/web-app/e2e/fixtures.ts` | Mine | — |
| `packages/web-app/e2e/over-subscription.spec.ts` | Mine | — |
| `packages/web-app/e2e/partial-connectivity.spec.ts` | Mine | — |
| `packages/web-app/e2e/receiveEvidence.ts` | Mine | — |
| `packages/web-app/e2e/server-mute.spec.ts` | Mine | — |
| `packages/web-app/e2e/windowSampling.ts` | Mine | — |
| `packages/web-app/tests/windowSampling.test.ts` | Mine | — |
| `scripts/dev-web.sh` | Not mine, Minor-judgment | infrastructure |
| `scripts/dev-web.test.sh` | Not mine, Minor-judgment | infrastructure |
| `docs/runbooks/client-dev-local.md` | Not mine, Minor-judgment | operations |

---

### Gate 1 — SKIPPED (tier=light; reason: manual test plan document, run and recorded)

Lead tier-gate check: planned surfaces are a docs file and web-app e2e harness + unit test — no `--light` exclusion surface and no GSA path. If the inline plan reaches one, Gate 1 runs anyway.

## Planning

Tier light, inline plan (no GSA path, no `--light` exclusion surface: one new docs file, one e2e
harness module edit, one new pure harness module, one new node-tier unit test).

### Problem restated in mechanism terms

1. **Plan document.** The story's N+1 demo (R-32) has no human-runnable script. What a human must
   see that automation structurally cannot: audibility by ear on Windows Chrome, a real-microphone
   pass, and rendered pixels. Everything else in the plan is mechanised by the browser E2E specs
   (`multi-party-hear`, `over-subscription`, `server-mute`, `kek-rotation`, `partial-connectivity`,
   `solo-participant`) and the Rust env-tests (`27_mc_slot_placement.rs`), so the run record maps
   each plan step to its automated equivalent, runs that equivalent here, and records the
   human-only residue as NOT RUN (pending before /close-story) - never PASS.
2. **observeWindow flake.** The loop's only exit is `Date.now() >= end` (a fixed wall-clock window).
   Sample count = window / (read latency + interval). When `Probe.read()` (a `page.evaluate` per
   probe, `Promise.all` across pages) is slow under load, the count falls below
   `MIN_FLAT_WINDOW_SAMPLES` and the vacuity guard fails as HARNESS. The invariant we want: the loop
   exits only when BOTH the window has elapsed AND the sample floor is met; otherwise it continues
   until a hard upper bound, at which point it throws a loud HARNESS error naming the count and
   elapsed time. The vacuity guard stays (defence in depth; still the assertion a stalled sampler
   hits). Completing the invariant: the same fixed-window pattern in `expectCountersFlatOverWindow`
   is NOT the same mechanism (its samples are taken in-page by the bus timer, not by slow
   out-of-page reads, so read latency cannot starve it) - checked, left as is, stated here.

### Planned file changes

| File | Change |
|------|--------|
| `docs/user-stories/2026-09-21-hear-each-other-manual-test-plan.md` | NEW - the plan (phases, expected observations, dashboards/queries by real panel title, F16/F17, proves / does-not-prove, promote-to-E2E verdicts) and the RUN RECORD for this run |
| `packages/web-app/e2e/windowSampling.ts` | NEW - pure (no Playwright, no `./env`) `windowStep()` stop decision + `sampleWindow()` loop with injectable clock/sleep; the ONE home of the stop condition |
| `packages/web-app/e2e/receiveEvidence.ts` | `observeWindow` delegates its loop to `sampleWindow`; vacuity guard kept |
| `packages/web-app/e2e/fixtures.ts` | add `FLAT_WINDOW_MAX_OBSERVE_MS` beside `FLAT_WINDOW_OBSERVE_MS` / `MIN_FLAT_WINDOW_SAMPLES` (their one home) |
| `packages/web-app/tests/windowSampling.test.ts` | NEW node-tier unit tests of the stop condition (window-not-elapsed, floor-not-met, both met, hard-bound overrun throws loudly, fast path unchanged) with a fake clock |
| `packages/web-app/e2e/README.md` | the stop rule + the same-page mover rule, where the README documents observeWindow's window |
| `packages/web-app/e2e/{server-mute,partial-connectivity,over-subscription}.spec.ts` | (added during implementation, @paired-client's vacuity point) custom probes carry `observer`; the two windows with a flat/zero probe on a page with no mover gain one |
| `docs/specialist-knowledge/test/INDEX.md` | pointer to the plan + windowSampling |
| `scripts/dev-web.sh` + `scripts/dev-web.test.sh` | (review, @operations OPS-1) echo `test levers: ON requested / OFF` beside the tone line, with 4 self-test assertions |
| `docs/runbooks/client-dev-local.md` | (review, @paired-client) F16 unblock: a rejoin through the Create/Join navs, not a page reload (a reload drops the in-memory sign-in) |

### Run plan (what executes from this container)
`scripts/dev-web.sh --check` (record verbatim, incl. failures - this container is not the static
topology); full browser E2E suite against the devloop cluster (Layer-7 wiring: per-run org,
`E2E_*`/`VITE_*` from ports.json), plus server-mute.spec repeated to exercise the fixed window;
Prometheus reads (ENV_TEST_PROMETHEUS_URL) of every metric the plan tells the human to read.
Human-only steps: NOT RUN.

---

## Implementation Summary

**(1) Plan written** — `docs/user-stories/2026-09-21-hear-each-other-manual-test-plan.md`. It
follows the 2026-05-02 pattern, plus a proves / does-not-prove section. Contents:

- **Phase 0:** static-cluster bring-up with `DT_TEST_TONE=1 DT_TEST_LEVERS=1 scripts/dev-web.sh`.
  The human records the preflight's N line. Distinct accounts and display names, and profiles.
- **Phases 1-2:** N+1 join and hear; E is F17 static fill.
- **Phase 3:** server mute, own view, request-unmute (no self-clear), and non-host refusal via
  `forceHostControls`.
- **Phase 4:** C leaves. No gap, refill by E, rotation, with an F16 pointer.
- **Phase 5:** E leaves first, so the late joiner F hears all N.
- **Phase 6:** edge occupancy falls.
- **Phase 7:** solo.
- **Phase 8:** optional partial connectivity via `blockHandlers`.
- **Phase 9:** real microphone with headphones, after a dev-server restart without the tone.

Supporting sections:

- **Dashboards table:** every panel title checked against `infra/grafana/dashboards/` and the live
  Grafana.
- **F16/F17 triage:** points into the runbook.
- **Promote-to-E2E verdicts:** maps each step to its automated equivalent.

Two things the task text could not carry verbatim, recorded in the plan:

- **Real mechanism for the human lever.** Set `window.__dt_test_levers__` in DevTools before
  clicking the Join nav, because `JoinMeeting.svelte` reads it once at mount. Take the offered URLs
  from the bus `joined.mediaServers`.
- **Interpretations.** The extra joiner is F17 static fill at the receivers, not a `fewer_sources`
  cell (per @paired-client). The late joiner hears "everyone" only after one more leave; at N+2 it
  would hear the earliest N.

**(2) Plan run** — the Run record in the plan (Run 1). Results:

- **Preflight:** `dev-web.sh --check` echoed N=3 (launcher demo default), and exited 1 in this
  container. Recorded as a FAIL. The cause is the environment: not the static topology, and no `ss`.
- **Full browser suite:** 17/17 passed against the devloop cluster.
- **Repeats:** the observeWindow specs passed 12/12 under `--repeat-each=3`.
- **Prometheus/Grafana/Loki reads** are recorded per step.
- **Human-only steps are NOT RUN, pending before /close-story.** That covers by-ear, rendered UI,
  the real microphone, and the Phase 4 refill/no-gap check, which has no automated browser
  equivalent.

No product defect was found. One observability caveat was found: the client-media KEK panel is
blind to the first increment of a series, which the panel description already documents
(@observability). The plan cites it rather than restating it.

**(3) observeWindow flake fixed.**

- **Stop rule.** A new pure module, `e2e/windowSampling.ts`, holds `windowStep`
  (`continue`/`done`/`overrun`) and the `sampleWindow` loop, with the clock and sleep injected. The
  loop stops only when the window has elapsed AND `MIN_FLAT_WINDOW_SAMPLES` is met. It throws a
  `HARNESS:` error at `FLAT_WINDOW_MAX_OBSERVE_MS`, which is `4 * FLAT_WINDOW_OBSERVE_MS` and lives
  in `fixtures.ts`, their single home.
- **Guards.** The `>=` floor assertion is kept.
- **Same-page mover rule** (adopted from @paired-client's vacuity point). Each read returns a page's
  latest bus snapshot, and a slow-read loop can now take more reads. So `observeWindow` requires
  every page carrying a flat/zero probe to also carry an `advance` probe, and throws before
  sampling otherwise. `Probe.observer` is new.
  - This exposed two pre-existing windows that violated the rule: over-subscription S2 (R had only
    zeros) and the server-mute compose window (the host had only flat-on-B). Both gained a mover.
- **Tests.** `tests/windowSampling.test.ts` has 9 node-tier tests with a fake clock: every arm, the
  boundaries, the slow-read case that used to fail, the loud overrun, and a bad bound.
- **Not changed.** `expectCountersFlatOverWindow` was checked and left as is: its samples come from
  the in-page timer, so read latency cannot starve them.

**Iteration 2 — review fixes (all findings fixed, none deferred).**

- **security F1/F2.** Attribution is reworded as "the sender MC's roster lists for B, given an honest
  MC", and the verify/decrypt behind attribution now sits under "does not prove the crypto". Leaver
  lockout is a new does-not-prove bullet. While doing that, I found that the Phase 4 citation of
  "env-tests leaver lockout" was wrong. The real homes are cited now: sdk-core
  `audioPipeline.multiSender.test.ts` S7 for lockout, `34_mc_kek_rotation.rs` for rotation, and
  `kek_rotation_integration.rs` for coalescing.
- **test F1.** Each read is now raced against the remaining budget, through an injectable,
  cancellable timer. A slow read is cut off at the bound, and a hung read fails there as the same
  HARNESS error. The tests assert elapsed == bound, and there is a never-resolving-read test.
- **test F2.** The mover rule is extracted as the pure `unpairedObservers`, with 4 tests.
- **test F3.** The repeats now cover every observeWindow spec, plus solo for the sibling window, and
  both runs were re-run on the final code.
- **code-reviewer F1.** The bound is `FLAT_WINDOW_MAX_OBSERVE_FACTOR * observeMs`.
- **code-reviewer F2.** The bound's doc is corrected.
- **code-reviewer F3 / observability 1.** Panel titles are now verbatim.
- **dry F1.** `expectCountersFlatOverWindow` uses the same `windowStep` rule after its fixed wait.
  This withdraws my planning claim that it needed no change, because its in-page timer does slip
  under load. The README covers both windows.
- **dry F2.** The restated values are gone: `DEMO_RECEIVE_SLOTS`, `R_SLOTS`, and the
  `kind-config.yaml` hostPorts are cited instead.
- **dry F3 / observability 2 / ops-2.** Fixed the citation to MH runbook Scenario 18 Arm B and MC
  Scenario 20. Added the registered-meetings teardown pair. Added MC Scenarios 19, 20 and 21 to the
  triage list and the References block.
- **observability 3.** The raw KEK counters are read as a before/after delta, recorded in the run
  record.
- **paired-client 1.** The F16 unblock is fixed in both the plan and the runbook.
- **infra I-1 / ops-1.** Phase 0 now has a `buildKnobs` check (`testLevers`, `telemetrySinkActive`)
  and a dev-web.sh levers echo.
- **infra I-2 / ops-3.** The MH log cross-check now greps by `participantId`, with the log message
  and the Loki form.
- **ops-4.** Added how to create the accounts.

**Validation:** `./scripts/layer-fast.sh` green (TOTAL_RESULT=N/A, no FAIL). `pnpm --filter
@darktower/web-app lint` (svelte-check + eslint over `e2e/`) clean.

---

## Code Review Results

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 2 | 2 | 0 | attribution overclaim; leaver-lockout not proven by demo |
| Test | RESOLVED-FIXED | 3 | 3 | 0 | hard bound raced per read; `unpairedObservers` tested; repeat coverage |
| Observability | RESOLVED-FIXED | 5 | 5 | 0 | verbatim panel titles; teardown pair; KEK delta read; MC S19/S21 triage |
| Code Quality | RESOLVED-FIXED | 4 | 4 | 0 | bound scales with `observeMs`; doc accuracy; titles; stale message |
| DRY | RESOLVED-FIXED | 3 | 3 | 0 | `expectCountersFlatOverWindow` completes the stop-rule invariant; cite-don't-restate; broken cite |
| Operations | RESOLVED-FIXED | 4 | 4 | 0 | levers echo in dev-web.sh; triage list; MH log grep; account creation |
| Paired client | RESOLVED-FIXED | 2 | 2 | 0 | F16 unblock = rejoin (reload drops in-memory sign-in); change-log order |
| Paired infrastructure | RESOLVED-FIXED | 2 | 2 | 0 | buildKnobs check; MH log cross-check by participantId |
| Semantic Guard | not spawned | — | — | — | docs + e2e harness only; no check surface |

Details of each fix: see §Implementation Summary, Iteration 2.

Human-only steps of the plan (by-ear, rendered UI, real microphone, Phase 4 refill/no-gap) are NOT RUN in this headless run; R-32 is not met until a human records Run 2 before `/close-story`.

Lead note: implementer observed one non-reproducing Layer-3 failure in `scripts/setup.test.sh [prov-render-deterministic]` mid-review (diff does not touch setup/provision; passed alone 576/0 and in the next full layer-fast). Watched in Gate 2.

---

## Accepted Deferrals

- (none surfaced in this devloop)

---

## Rollback Procedure

1. Start commit: `fe96076abf800f195da1dbfdb353b62052733c13`
2. `git diff fe96076a..HEAD`
3. **Safe-revert unit**: the whole diff, as one unit. The plan doc is independent, and reverting it only removes the doc. The harness change (`windowSampling.ts`, the `observeWindow` delegation, `FLAT_WINDOW_MAX_OBSERVE_MS`, the `Probe.observer` field and its three custom probes, the two added movers, and the unit test) must revert together. `observer` is a required field that the specs' custom probes supply. No product code, config or manifest is touched, so a revert has no deploy step.

---

## Devloop Verification Steps (Gate 2)

`DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` on the final tree: exit 0, `TOTAL_RESULT=N/A`.

| Layer | Result | Notes |
|-------|--------|-------|
| 1 Compile | OK | |
| 2 Format | OK | |
| 3 Guards | OK | `setup.test.sh [prov-render-deterministic]` mid-review flake did not reproduce |
| 4 Test | N/A | proto intentional-gap placeholder (`not-applicable-to-this-lang`); rust/ts OK |
| 5 Lint | OK | |
| 6 Audit | N/A | proto placeholder; no dep-manifest changes |
| 7 Env-tests | OK | Rust env-tests + `browser-e2e-passed` (1 attempt) |
