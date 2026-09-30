# Devloop Output: Browser multi-party test infrastructure (N+1 provisioning, tone detector, S1 diagnostic)

**Date**: 2026-09-30
**Task**: Story 2 (hear-each-other) task #14 — N+1 provisioning helper, receive-side tone detector, S1 diagnostic, `instanceCounters` note (R-30)
**Specialist**: test
**Mode**: Agent Teams (v2) — full panel; Gate-1 SKIPPED — harness-only: provisioning helper, detector, diagnostic; no contract
**Branch**: `feature/hear-each-other`
**Duration**: ~2h

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `9ca6393a77e22b3d42161b9a8228096c5b6e852d` |
| Branch | `feature/hear-each-other` |
| Lead Model | `claude-opus-5-5[1m]` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` |
| Implementing Specialist | `test` |
| Tier | `light — harness-only: provisioning helper, detector, diagnostic; no contract` |
| Iteration | `1` |
| Paired | `paired-client`, `paired-infrastructure` |
| Security | `security` |
| Test | `test` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Semantic Guard | not spawned (test-harness diff; no check surface in `scripts/guards/semantic/checks.md`) |

---

## Task Overview

### Objective
See `/home/dev/.cache/devloop/story-runs/story-runner/2026-09-21-hear-each-other/task-14.prompt` (story `docs/user-stories/2026-09-21-hear-each-other.md` task 14, R-30).

### Scope
- **Service(s)**: `packages/web-app/e2e`, `packages/test-utils`
- **Schema**: No
- **Cross-cutting**: No

### Debate Decision
NOT NEEDED - harness-only work within story plan.

---

### Gate 1 — SKIPPED (tier=light; reason: harness-only: provisioning helper, detector, diagnostic; no contract)

---

## Cross-Boundary Classification

Every file in the diff has a row. No GSA path, no `--light` exclusion surface (no Cargo/pnpm deps, no manifests, no instrumentation code, nothing under `crates/common`).

| Path | Classification | Owner | Notes |
|------|----------------|-------|-------|
| `packages/web-app/e2e/cohort.ts` (new) | Mine | test | pure: suite N, cohort size, cohort env hand-off codec, AC auth-rate-limit parse from `infra/services/ac-service/config.env`, window-fit check |
| `packages/web-app/e2e/toneDetector.ts` (new) | Mine | test | pure: spectrum tone detector + two-sided per-receiver-per-sender verdict; imports nothing from sdk-core |
| `packages/web-app/e2e/s1Diagnostic.ts` (new) | Mine | test | pure: MH admission metric names + budget-rejection / misrouting / unobservable classification |
| `packages/web-app/e2e/mcMetrics.ts` | Mine | test | MH admission PromQL (the one PromQL home) + `diagnoseMissingSender` |
| `packages/web-app/e2e/fixtures.ts` | Mine | test | `registerCohort`, `authAsCohortMember`, own-tone + lane-analysis readers, `expectHearsSender` poll, 429 names the auth-rate budget |
| `packages/web-app/e2e/global-setup.ts` | Mine | test | budget-fit check + one-per-run cohort registration, hand-off via env |
| `packages/web-app/e2e/instanceCounters.ts` | Mine | test | comment only (deliverable 3) |
| `packages/web-app/e2e/README.md` | Mine | test | §Budgets rewritten in the correct unit |
| `packages/web-app/playwright.config.ts` | Mine | test | comment only: workers=1 coupling now names the global-setup cohort |
| `packages/web-app/tests/cohort.test.ts` (new) | Mine | test | node tier |
| `packages/web-app/tests/toneDetector.test.ts` (new) | Mine | test | detector self-test over synthetic PCM + no-shared-code guard with positive control |
| `packages/web-app/tests/s1Diagnostic.test.ts` (new) | Mine | test | classification + metric-name drift check against `crates/mh-service/src/observability/metrics.rs` |
| `docs/devloop-outputs/2026-09-30-browser-multiparty-test-infra/main.md` | Mine | test | this file |

---

## Planning

Facts established before planning (verified in code; the infra ones confirmed by @paired-infrastructure, the bus ones by @paired-client):

- **The AC "registration" limit counts successful token issues, not registrations.** `crates/ac-service/src/services/user_service.rs` `count_registrations_from_ip` counts `auth_events` rows with `event_type='user_login' AND success=true` for the peer IP in the window; `register` and every `signInViaUi` write one. So every sign-in spends budget, the bucket is per source IP (shared with the Rust env-tests that run from the same host just before this suite), and the existing README "2 registrations/run" is the wrong unit (and already wrong on its own terms: auth-rejection registers a third account). Keys: `AC_REGISTRATION_RATE_LIMIT_{MAX_ATTEMPTS,WINDOW_MINUTES}` in `infra/services/ac-service/config.env`.
- **Playwright restarts the worker after a failing test**, so the module-level `sharedRegistration` memo is a per-run singleton only on a green run; a cohort memo there would re-register N+1 accounts after every failure. Playwright runs plugin (webServer) setup BEFORE `globalSetup` (`createGlobalSetupTasks` order in the installed 1.62.1), so global setup can drive the real sign-up UI. **Decision: the cohort registers in `global-setup.ts`** ("move to global setup if N forces it" — the restart behaviour forces it at any N), handed to workers via `process.env` (Playwright's documented globalSetup→worker channel). V stays where it is; the README records its +1-per-restart cost.
- **Layer 3 on the bus is a band-clipped dB magnitude spectrum per lane** (`receiveAnalysis.lanes[]`, AnalyserNode `getFloatFrequencyData`, Blackman window, smoothing 0), keyed by VERIFIED sender, not slot; no PCM exists on the bus by design (@security). So the FFT already ran in the browser, over the signal; the detector runs over that spectrum. The self-test closes the loop from PCM: it synthesises PCM (own `Math.sin`), runs an independent reference analyser (Blackman-windowed Goertzel per bin, the WebAudio AnalyserNode definition, anchored by an absolute-level check) into the SAME record shape, and feeds the detector.
- Expected tones come from each participant's own `captureSource.toneHz` on the bus — never from the SDK formula (`testTone.ts` is neither imported nor restated).
- MH admission series (SSoT `crates/mh-service/src/observability/metrics.rs` `resolve_session_handles`/`publish_egress_admission`): `mh_media_stream_admission_total{outcome="rejected_stream_ceiling"}` (zero-registered from boot, so an EMPTY result means "not scraped", never zero), `mh_media_stream_admission_rejection_ratio`, context gauges `mh_media_egress_stream_ceiling`, `mh_media_egress_edges`. TS cannot import a Rust const; the names live once in `s1Diagnostic.ts` and a node test reads `metrics.rs` and fails on drift.

### Deliverable 1 — N+1 cohort provisioning
- `cohort.ts`: `SUITE_RECEIVE_SLOTS = 3` (the suite's one N), `cohortSize(n)` (integer ≥ 1 or throw), `encodeCohort`/`decodeCohort` (fail loud on missing / malformed / wrong size / duplicate email), `parseAcAuthRateLimit(configEnvText)` (both keys required, positive integers, else throw naming the file+key), `assertCohortFitsAuthWindow(n, limit)`: the cohort's registration burst (N+1) plus one multi-party test's sign-in burst (N+1) can share one window, so `2(N+1) ≤ max attempts` or throw at setup rather than as mid-suite 429s. Necessary, not sufficient — documented as such.
- `global-setup.ts`: after the existing preconditions, read `config.env`, run the fit check, launch Chromium, register `cohortSize(SUITE_RECEIVE_SLOTS)` accounts through `signUpViaUi` (one encoding of sign-up) in one context each, set the env hand-off.
- `fixtures.ts`: `suiteCohort()` (decode or throw with remediation), `authAsCohortMember(page, index)` (1 sign-in, 0 registrations); `captureAccessToken` names the budget unit + SSoT on HTTP 429.
- README §Budgets rewritten in the correct unit; shards share the per-IP bucket unless each shard targets its own cluster; workers stay 1.

### Deliverable 2 — tone detector + two-sided assertion
- `toneDetector.ts` (pure, no sdk-core import): `toneLevelDb(spectrum, hz, tolBins)` (max within ±1 bin of the nearest bin; `null`/`-Infinity` bins read as -Infinity), `assertCohortTonesSeparable(tones, binHz)` (pairwise ≥ Blackman main-lobe half-width 3 + tolerance 1 bins, else a PRECONDITION error distinct from misrouting), `evaluateSenderLane(...)` → verdict `ok | lane_missing | not_ready | silent | expected_absent | foreign_present` plus an out-of-band harness error (throws). Present = expected level ≥ floor (band median) + 20 dB AND the band's global peak lies within tolerance of the expected tone; foreign = any other cohort tone (receiver's own included) within 20 dB of the expected peak. Carrier-relative, not floor-relative, for foreign: codec frame-rate sidebands sit far below the carrier but far above a clean floor.
- `fixtures.ts`: `readOwnTone(page)` (asserts `mode === 'test_tone'`, returns senderId + toneHz), `latestLaneAnalyses(page)`, `expectHearsSender(page, receiver, sender, cohort)` bounded poll to `ok`, failing with the last verdict (a wait for `ready`, not a latency gate).
- Self-test (`tests/toneDetector.test.ts`): reference analyser absolute anchor; every synthetic tone at 44.1/48 kHz incl. off-bin-centre; adjacent tones 25 Hz apart resolve; mixed equal-amplitude foreign → `foreign_present`; −30 dB leak → `ok`; wrong tone → `expected_absent`; silence, not-ready, lane missing, out-of-band, collision precondition; seeded noise (deterministic). No-shared-code guard over the new e2e modules + the test, with a positive control.

### Deliverable 3 — `instanceCounters.ts` comment
Client series carry `instance` = the collector pod (`honor_labels: false`, a security control); per-instance delta math stays sound with the one collector replica (`infra/services/otel-collector/deployment.yaml`), a collector rollover resets client history; no assertion may read a client series' `instance` as a participant dimension (R-28). Points at `prometheus.yml` and `docs/observability/metrics/client.md` for the shape.

### S1 diagnostic
- `s1Diagnostic.ts`: metric-name constants + `classifyMissingSender({baselineRejected, currentRejected, ratio, ceiling, edges})` → `config_budget_rejection` (rejected counter moved on any instance in-window, per-instance delta via `anyInstanceExceedsBaseline`) | `misrouting` (series present, no instance moved) | `unobservable` (empty rejected result — never folded into misrouting). Report text carries ratio/ceiling/edges per instance and names `MH_EGRESS_BUDGET_BPS` in the Kind overlay patch, without restating a number.
- `mcMetrics.ts`: `mhAdmissionRejectionsByInstance()` baseline + `diagnoseMissingSender(baseline, what)` which waits up to a few MH scrapes for an in-window rejection before concluding misrouting (MH job `scrape_interval: 5s`).

### Not touched, and why
- `packages/test-utils`: web-app does not depend on it (adding a workspace dep is a lockfile/manifest change for no gain), and it is sdk-core's devDependency — keeping the detector out of the package the SDK's tests import makes "no shared code with the send side" structural rather than conventional.
- `webServer` env (`DT_TEST_TONE`, `VITE_DT_RECEIVE_SLOTS`): the specs that need them are task 15's; this task exports `SUITE_RECEIVE_SLOTS` for that wiring and `readOwnTone` asserts tone mode rather than assuming it.

---

## Implementation Summary

Implemented as planned, with changes from the pairing round:
- **Cohort** (`e2e/cohort.ts`, `global-setup.ts`, `fixtures.ts`): `SUITE_RECEIVE_SLOTS = 3`. The N+1 cohort registers once per run in global setup through `signUpViaUi` (one context per member) and reaches the workers via `E2E_COHORT_CREDENTIALS`. `decodeCohort` fails loudly on a missing, short, malformed or duplicate cohort, and never echoes a password. `parseAcAuthRateLimit` reads both `AC_REGISTRATION_RATE_LIMIT_*` keys from `config.env`. `cohortWindowSpend` names its two terms (N+1 registrations, N+1 first-test sign-ins) instead of a bare factor of 2 (@paired-infrastructure a). `captureAccessToken` turns a 429 into a named, unretried budget error (@paired-infrastructure b).
- **Detector** (`e2e/toneDetector.ts`):
  - A tone is present when it is above the floor AND within 20 dB of the band peak, so Blackman leakage from an adjacent tone does not count.
  - A foreign tone is any other cohort tone within 20 dB of the expected one, the receiver's own tone included. This check runs before dominance, so a comparable intruder is diagnosed as foreign rather than by whichever tone won the peak.
  - After that the expected tone must also be the dominant one.
  - Verdicts: `not_assigned` (from the latest `slotAssignments`, @paired-client note 1), `lane_missing`, `not_ready`, `silent`, `expected_absent`, `foreign_present`.
  - Out-of-band tones and cohort collisions are thrown as separate error classes.
  - `expectHearsSender` polls in a plain loop at the bus's own `E2E_FRAME_COUNT_SAMPLE_INTERVAL_MS`, imported rather than restated. It uses a plain loop because harness errors must fail at once, and on timeout it reports the last verdict (liveness bound, no latency gate).
- **Self-test** (`tests/toneDetector.test.ts`):
  - Its own PCM goes through a Blackman-windowed Goertzel reference analyser, anchored at 20log10(0.21) for a unit sine at a bin centre.
  - Runs at 44.1 and 48 kHz, with seeded noise.
  - Covers each sender heard two-sidedly by every receiver, an off-centre tone, an adjacent tone at 25 Hz, the receiver's own tone mixed in, a third tone at -10 dB (fails) and at -30 dB (passes), a louder non-cohort tone, noise, silence, not-ready, lane missing, not-assigned or muted, a JSON null round-trip, out-of-band, collision, and a self or outsider sender.
  - The no-shared-code import guard runs with a positive control.
- **S1** (`e2e/s1Diagnostic.ts`, `mcMetrics.ts`): three verdicts. The per-instance delta decides. Ratio, ceiling and edges are included as context. The settle wait for a late scrape defaults to three MH scrapes. A metric-name drift test reads `crates/mh-service/src/observability/metrics.rs`, with a positive control.
- **Docs**:
  - README §Multi-party harness is new.
  - README §Budgets is rewritten in the correct unit: sign-ins count, the bucket is shared with the env-tests, V costs +1 per failing test, and shards only help when each targets its own cluster.
  - The `instanceCounters.ts` comment and the `playwright.config.ts` comment are updated.

Validation: `./scripts/layer-fast.sh` passes (L1/2/3/5 OK; L4/L6 aggregate N/A with every lane OK). The web-app node tier passes (135 tests). `pnpm lint` shows 0 errors, and `playwright test --list` loads the fixtures.

---

## Devloop Verification Steps

Gate 2 — `DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh`, exit 0, TOTAL_RESULT=N/A. Layers 4 and 6 aggregate N/A; every check inside them was OK.

| Layer | Result | Duration (s) |
|-------|--------|--------------|
| 1 | OK | 5 |
| 2 | OK | 4 |
| 3 | OK | 167 |
| 4 | N/A | 270 |
| 5 | OK | 1 |
| 6 | N/A | 3 |
| 7 | OK (env-tests-passed, browser-e2e-passed) | 661 |

Layer 7 exercised the new global-setup cohort registration live ("registered the N+1 cohort once for this run (N=3, 4 accounts)"). The multi-party specs that use `expectHearsSender`/`diagnoseMissingSender` against the cluster are task 15's.

---

## Code Review Results

### Gate 3 Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred |
|----------|---------|----------|-------|----------|
| Security | CLEAR | 0 | 0 | 0 |
| Test | RESOLVED-FIXED | 3 | 3 | 0 |
| Observability | RESOLVED-FIXED | 3 | 3 | 0 |
| Code Quality | RESOLVED-FIXED | 4 | 4 | 0 |
| DRY | RESOLVED-FIXED | 2 | 2 | 0 |
| Operations | RESOLVED-FIXED | 2 | 2 | 0 |
| Paired client | RESOLVED-FIXED | 3 | 3 | 0 |
| Paired infrastructure | RESOLVED-FIXED | 2 | 2 | 0 |

DRY extraction opportunities: none.

Iteration 1 findings, all FIXED (none deferred):

| Reviewer | Finding | Fix |
|----------|---------|-----|
| paired-infrastructure F1 / operations F1 / observability F1 / test F1 | S1 read a partial scrape or an empty baseline as a confident verdict | `unobservable` for an empty baseline (Layer 7's Rust S9 leaves a non-zero lifetime counter), an empty current read, any MH target not `up{job="mh-service"}` (new `scrapeUp` evidence), and any baseline instance missing now. Positive evidence (an in-window delta on a visible instance) still wins. Unit cases for each. |
| paired-infrastructure F2 | the "Kind overlay does not patch the AC limit" claim was unguarded | `tests/cohort.test.ts` walks `infra/kubernetes/overlays/kind/**` for `AC_REGISTRATION_RATE_LIMIT_`, with non-empty-walk and base-config positive controls |
| code-reviewer F1 / observability F2 | the missing-sender hint pointed at a diagnostic that needs a pre-join baseline | `expectHearsSender(..., { admissionBaseline })` runs `diagnoseMissingSender` on not_assigned/lane_missing/silent and appends its report; without a baseline it says the diagnostic was unavailable |
| code-reviewer F2 / test F2 | two `page.evaluate`s, each serialising the whole growing bus buffer | `latestBusEventsOf`: one reverse scan in the page, returning only the latest event of each requested type (one consistent snapshot) |
| code-reviewer F3 | `Partial_` name | `SineComponent` |
| code-reviewer F4 | duplicate peak scan | single `peakBin()` returning hz + dB |
| paired-client F1 / dry-reviewer F1 | suite N and browser N unlinked | `webServer.env.VITE_DT_RECEIVE_SLOTS` derived from `SUITE_RECEIVE_SLOTS`, and `expectDeclaredReceiveSlots` (called from `readOwnTone`) asserts declared N and cap at runtime (a reused server bypasses the env). Existing solo/two-party specs are unaffected at N=3 (they assert slot 0 only). |
| paired-client F2 | the no-shared-code guard covered one file | name/path rule over every `e2e/*.ts` (readdir, non-empty, includes toneDetector + fixtures), with a multi-name sdk-core import positive control; the strict rule stays on the detector |
| paired-client F3 | a sender in two slots read as ok | `duplicate_assignment` verdict + unit case |
| test F3 | collision message said "re-run" | now names sequential sender-id allocation, id churn, or a derivation/announcement defect; "investigate, do not re-run" |
| dry-reviewer F2 | key-const names read as the other AC limit | `AC_REGISTRATION_RATE_LIMIT_{MAX,WINDOW}_KEY` |
| observability F3 | `up{job="mh-service"}` hard-coded the scrape job | `MH_SCRAPE_JOB` const in `s1Diagnostic.ts`, drift-tested against `prometheus.yml` `job_name` with a positive control |
| operations F2 | cohort registration wall-clock not recorded | README §Budgets bullet (estimate ~10 s at N=3, flagged as unmeasured, scales with N) |

Disagreements resolved in favour of the stricter reading: observability asked that an empty baseline stay `misrouting`, and test asked not to key on baseline instances. Infra's evidence decided against both. Layer 7's S9 env-test leaves a non-zero lifetime rejection counter on these pods, so an empty baseline would yield a false budget verdict. A rollover with no visible rejection may also hide a rejection on the pod that left. Both cases are therefore `unobservable`; a rejection seen on a fresh pod still counts.

paired-client ruled on `packages/test-utils` and accepted leaving it untouched. test-utils is sdk-core's devDependency, one import away from the SDK's tests, so web-app/e2e gives the stronger structural separation the independence rule asks for.

---

## Accepted Deferrals

- (none surfaced in this devloop)

---

## Rollback Procedure

1. Start commit: `9ca6393a77e22b3d42161b9a8228096c5b6e852d`
2. `git diff 9ca6393a..HEAD`
3. **Safe-revert unit**: the whole diff; harness and docs only, no production code or deploy surface.
