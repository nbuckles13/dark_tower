# Devloop Output: Story 3 task 21 — capacity evidence (R-28) and see-each-other manual test plan (R-34)

**Date**: 2026-10-24
**Task**: Task 21 (story `docs/user-stories/2026-10-07-see-each-other.md`): capacity evidence for the video cohort (R-28) and the see-each-other manual test plan, written and its automated equivalents run (R-34)
**Specialist**: test
**Mode**: Agent Teams (v2) — full panel; Gate-1 SKIPPED — documentation/evidence task (run-story manifest tier=light)
**Branch**: `feature/user-see-each-other`
**Duration**: ~2h30m

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `bc82880fea30c9a68dbfa6a08a4d6c4630b11169` |
| Branch | `feature/user-see-each-other` |
| Lead Model | `claude-opus-5-5[1m]` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` |
| Implementing Specialist | `test` |
| Tier | `light — documentation/evidence task (manifest tier=light)` |
| Iteration | `1` |
| Security | `security` |
| Test | `test` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Semantic Guard | not spawned (docs/evidence diff; no check surface) |
| Client (paired per task prompt, UI affordance names) | `paired-client` |

---

## Task Overview

### Objective
Write and run the story-3 manual test plan, including a "Capacity evidence" section with real numbers from a Layer-7 run on this commit.

### Scope
- **Service(s)**: none (documentation + evidence)
- **Schema**: No
- **Cross-cutting**: Yes (plan walks MC/MH/client/dashboards/runbooks)

### Debate Decision
NOT NEEDED - design fixed by story file + ADR-0036.

---

## Cross-Boundary Classification

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `docs/user-stories/2026-10-07-see-each-other-manual-test-plan.md` | Mine | — |
| `docs/devloop-outputs/2026-10-24-story3-see-each-other-manual-test-plan/main.md` | Mine | — |
| `packages/web-app/e2e/capacity.evidence.ts` (new; opt-in evidence scenario, NOT in the default `test:e2e` match) | Mine (test, browser E2E harness) | — |
| `packages/web-app/e2e/processCpu.ts` (new; pure `/proc/<pid>/stat` CPU reader + renderer fingerprint) | Mine (test) | — |
| `packages/web-app/tests/processCpu.test.ts` (new; node-tier test of the helper, one per helper) | Mine (test) | — |
| `packages/web-app/playwright.capacity.config.ts` (new; base config + evidence `testMatch` only) | Mine (test) | — |
| `packages/web-app/package.json` (one script `test:capacity-evidence`; no dependency change) | Mine (test) | — |
| `packages/web-app/tsconfig.json` (include the new config file) | Mine (test) | — |
| `packages/web-app/e2e/README.md` (one paragraph: the evidence run) | Mine (test) | — |

---

### Gate 1 — SKIPPED (tier=light; reason: documentation/evidence task, manifest tier=light)

## Planning

Inline plan (tier=light, Gate 1 skipped). Restated in mechanism language.

### Part 1 — capacity evidence (R-28): a recorded run, not a gate

**Gap found.** No existing spec runs the over-subscription video shape (P=4, N=2, M=2). `multi-party-see.spec.ts` is the see-all shape (P=4, N=3, M=3) but records no CPU, per-slot frame counts or recovery counters. `video-over-subscription.spec.ts` is a different shape (M=cap+1 at one receiver). Process-level CPU per browsing context is not measured anywhere. So the evidence needs a small amount of **test code**, as the brief anticipates. It is test-harness only: no product, instrumentation, SDK, deploy or dependency change.

1. `e2e/processCpu.ts` (pure helpers plus a thin `/proc` reader). It parses `/proc/<pid>/stat` utime+stime (fields 14/15, after the `)` of comm, so a comm containing spaces or parentheses cannot shift fields) and lists Chromium processes by `--type=` from `/proc/<pid>/cmdline`. It maps each browsing context to its renderer pid by a **fingerprint**: while the page is idle (opened, not yet joined), the page busy-loops in-page for a fixed span. The renderer whose CPU delta stands out (≥ a floor, and unique) is that context's renderer. Ambiguity or no match is a loud HARNESS error, never a guess. Shared browser-wide processes (browser, gpu-process, utility) are reported as one separate "shared" row. Mapping completeness check at window end: every renderer with non-trivial window CPU must be a mapped one, else a loud HARNESS error (catches a process swap on navigation).
2. `tests/processCpu.test.ts`: a node-tier test of the pure parts (stat parsing incl. hostile comm, fingerprint pick unique/ambiguous/none, percent arithmetic), per the one-test-per-helper convention.
3. `e2e/capacity.evidence.ts`: two tests, one per shape, through `VideoScenario`, so the test's own slot oracle predicts every slot. Per shape:
   - open 4 contexts, fingerprint each renderer, serialised joins, converge every receiver (`waitForVideoSlotState`);
   - **window start**: CPU snapshot, per-slot `framesDecoded`/`framesRendered` from the bus (`videoSlots`), per-publisher keyframes by cause from the bus (`sendCounts`), per-publisher groups abandoned + per-receiver unfulfilled per slot from the R-35 overlay DOM (opened by `debug-panel-toggle`; the bus does not carry these two). Wall-clock timestamp printed for the Prometheus reads;
   - **in window**: each receiver's `expectVideo` = the plan-7 detector (three layers incl. pattern liveness) over the shared stop rule. A receiver whose judgement fails is recorded as LIVENESS LOST and the test fails at the end, after the evidence is printed;
   - observation span floor (a fixed constant, a minimum span, not a pass/fail bound);
   - **window end**: the same snapshots; deltas printed as one JSON evidence record (`[capacity-evidence]` line + a `testInfo` attachment under gitignored `test-results/`).
   - CPU is never asserted (host-dependent; recorded only). An unfulfilled slot at window end is recorded and fails the test loudly (that is a story-blocking finding by the brief, not a tuning knob).
4. `playwright.capacity.config.ts`: spreads the base config and sets `testMatch: /capacity\.evidence\.ts$/` so the default `test:e2e` never picks the file up and Layer 7's browser budget is unchanged (`.evidence.ts` is outside Playwright's default `*.spec|test` match). Same global setup, cohort registration, retries 0, fake-media flags (no insecure flag). Run: `pnpm --filter @darktower/web-app test:capacity-evidence`.
5. MH side, after the run, from Prometheus with the printed window timestamps: `mh_media_egress_edges` peak per instance against that instance's `mh_media_egress_stream_ceiling` (headroom = ceiling − peak; 10% rule), `mh_media_group_resets_total` by reason (increase over window), `mh_media_edges_pending` at window end. Series names from `docs/observability/metrics/mh-service.md` / `client.md` only. Fleet client series (`dt_client_media_send_groups_abandoned_total`, `dt_client_media_keyframes_produced_total{source}`, `dt_client_media_slots_unfulfilled`) recorded as corroboration only (single-writer / no per-browser identity — observability trap 1), the per-context values come from the bus and overlay.
6. Profile read back from the deployed `media-codec-profile-*` ConfigMap that mc-0/mc-1/mh-0/mh-1 reference (never restated from the patch). Rust side: the `//! Edge-sum` header of each video env-test file, max per file, against the deployed ceiling.
7. CI component-tier VP8 probe CPU: the CI runner is not reachable from the devloop (no `gh` auth). I record the same CPU figures for the component-tier probe (`src/__tests__/vp8Probe.test.ts`, the test CI job `component` runs) on the devloop host with an external `/proc` sampler, and report the CI-runner figure as a gap to the lead rather than estimating it.
8. If the cohort does not fit (liveness lost, a slot unfulfilled at window end, or peak edges ≥ 90% of an instance's ceiling): SendMessage @team-lead with the numbers. No cohort shrink, no profile/ceiling retune.

### Part 2 — the manual plan document

`docs/user-stories/2026-10-07-see-each-other-manual-test-plan.md`, same structure and voice as the story-2 plan: header (story link, status, purpose, references, result convention, cite-don't-restate), "What the demo proves — and what it does not" (identical TS SFrame stack + keyless MH ⇒ self-consistent crypto error invisible; the cross-language vectors and the SFrame-WG anchor are the only independent check; R-18 by-design residual; overlay single-writer caveat), Phase 0 cold start, Phases 1–12, a security acceptance section, dashboards table, triage table, promote-to-E2E verdicts, Capacity evidence, run record. Every step names its automated equivalent (file + test title, verified in the tree). Phases 8 and 10 cost rolls, so each has a "restored and verified" checkbox, and no Layer 7 run may follow a half-restore (operations). Gaps that the tree cannot cover (phase 7 has no browser/env-test; phase 12 has no human-reachable induction in a Windows-Chrome pass; no video-specific non-host refusal spec) are stated in the plan and raised to the lead, never dropped.

Running: one Layer-7 run on this commit (`./scripts/layer7.sh`, Rust env-tests + the full browser suite) + the capacity evidence run + the component-tier probe sample. The human pass is marked "NOT RUN — pending before /close-story".

### Invariants
- No wall-clock pass/fail anywhere (CPU, time-to-first-render and recovery are recorded/observed, never gated).
- No insecure browser flag; test pattern and tone only in dev builds (cited gate + bundle tests).
- No trace/HAR committed; evidence JSON stays in gitignored `test-results/`.
- Nothing in the profile, ceiling or cohort is changed to make the evidence fit.

---

## Pre-Work

None

---

## Implementation Summary

**Part 2 — the manual plan**: `docs/user-stories/2026-10-07-see-each-other-manual-test-plan.md`. It has:
- a header, and "What the demo proves — and what it does not";
- Phase 0 cold start and Phases 1–12, each step naming its automated equivalent (file + title, verified in the tree by a grep sweep of every cited path, `file::fn`, quoted title and italic panel title);
- a security acceptance table, dashboards, triage by OPS-B scenario / F-entry number, promote-to-E2E verdicts, Capacity evidence and the run record.

The human pass is **NOT RUN — pending before /close-story**. Pairing input used:
- security: the R-18 notice wording, quoted from `ResumeNotice.svelte`; the privacy statement; the ordering fail-safe stated as automation-only;
- observability: panel titles, the single-writer gauge and pending blind-spot caveats, the P9 Prometheus check with a positive control;
- operations: the Phase 8/10 procedures cited rather than restated, with explicit restore-and-verify checkboxes;
- paired-client: test ids and copy.

**Part 1 — capacity evidence (R-28)**: the opt-in harness (`e2e/capacity.evidence.ts`, `e2e/processCpu.ts` + `tests/processCpu.test.ts`, `playwright.capacity.config.ts`, the `test:capacity-evidence` script, a tsconfig include, a README section). Recorded run on `bc82880f`, after a green Layer 7 (373 Rust env-tests passed; 29 browser specs passed):

| | see-all P=4 N=3 M=3 | over-sub P=4 N=2 M=2 |
|---|---|---|
| renderer CPU per context (% of one core; run 5 / runs 3–5) | 17.0–17.3 / 16.6–18.2 | 11.9–14.2 / 11.5–14.2 |
| shared: gpu / browser / utility (run 5) | 62.5 / 33.8 / 8.7 | 51.0 / 27.8 / 7.0 |
| frames decoded = rendered per slot | 462 in 30.1 s (= 15 fps profile) | 602–603 in 39.5 s |
| liveness lost / unfulfilled at end | none / none | none / none |
| groups abandoned, MH resets, MH pending | 0 / 0 / 0 | 0 / 0 / 0 |
| peak edges vs that instance's ceiling | 24 / 135 (17.8 %) | 16 / 135 (11.9 %) |

The recorded run is run 5, the final harness. The measured Layer-7 peak was 45 / 135 (33.3 %) on mh-1. It is unattributed to a file and falls in the Rust phase.
Over-subscription: D, the last joiner, is unwatched, so its camera is correctly off (0 sent).
- Rust: the largest per-file edge-sum is 35 (27_mc_slot_placement) against 135. The 72 + 39 design point is 82.2 %.
- Component-tier VP8 probe on this host: renderer 0.15 s CPU over a 0.30 s life, startup included.

**No story-blocking finding.**

**Gaps reported to the Lead** (accepted as reported gaps; in the plan's run record with what closes each):
1. The CI-runner probe CPU is not readable here.
2. Phase 12 has no human-reachable induction.
3. Phase 7 has no browser equivalent.
4. There is no video-kind non-host browser refusal spec.
5. Phase 10 "two machines" is unavailable per dev-web.sh; the plan uses two cameras on one machine.

Recorded disagreements with the prompt: file 27's edge-sum is 35, not ~60, and the ceiling is 135, not ~120. The tree and the gauge are recorded.

---

## Code Review Results

### Gate 3 verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | CLEAR | 0 | 0 | 0 | all security ACs verified against tree |
| Test | RESOLVED-FIXED | 5 | 5 | 0 | |
| Observability | RESOLVED-FIXED | 4 | 4 | 0 | |
| Code Quality | RESOLVED-FIXED | 7 | 7 | 0 | |
| DRY | RESOLVED-FIXED | 3 | 3 | 0 | no TODO.md entries |
| Operations | RESOLVED-FIXED | 2 | 2 | 0 | |
| Client (paired) | RESOLVED-FIXED | 3 | 3 | 0 | optional Phase-7 suggestion declined (not a finding) |
| Semantic Guard | not spawned | — | — | — | no check surface |

### Gaps raised to the lead (not deferrals — recorded in the plan's run record for /close-story)
1. CI runner component-tier VP8 probe CPU unreadable from the devloop (no `gh` auth); devloop-host figure recorded instead.
2. Phase 12: no human-reachable publisher→MH abandonment induction in the tree (SDK backlog caps not exposed by web-app; host shaping does not reach Windows Chrome over WSL mirrored networking). Automated equivalents named.
3. Phase 7 (camera revoked) has only SDK/component unit equivalents, no browser/env-test.
4. No video-kind non-host browser refusal spec; wire refusal covered by story-2 spec + env-test 35.
5. Phase 10 "two machines" unsupported by dev-web.sh; plan uses two cameras on one machine.
6. Prompt-vs-tree figure disagreements recorded as tree/live values: file 27 edge-sum header 35 (prompt: ~60); live `mh_media_egress_stream_ceiling` 135 (prompt: ~120). Measured Layer-7 peak 45/135.

Iteration 1 findings, all FIXED:
- **operations**:
  - F1: MH-did-not-roll check in Phase 8;
  - F2: `DT_HOST_GATEWAY_IP` unset on every deploy, with F8 in the triage hint.
- **observability**:
  - F1: Phase 9 label-name read plus a byte/size name check;
  - F2: PromQL recorded for every MH and fleet row, with the scrape-sampling caveat;
  - F3: the verdict stated as ≥ 90 %;
  - F4: Run 2 records the open-objective readings.
- **dry-reviewer**:
  - F1: R-3 cited rather than paraphrased, with the defaults cited;
  - F2: Phase 10 restore points at the patch header instead of a pasted command;
  - F3: README points at `SHAPES`.
- **code-reviewer**:
  - F1: Phase 2 audio-only check reworded to R-3 behaviour;
  - F2: header literal claim narrowed and the defaults cited;
  - F3: only ENOENT/ESRCH are swallowed in `/proc` reads;
  - F4: `statFieldsAfterComm` extracted;
  - F5: get-or-insert children map;
  - F6: harness and bus errors rethrown, never liveness; `contextEvidence` extracted;
  - F7: rollback text corrected.
- **paired-client**:
  - C1: Phase 9 self-check expectations for the test-pattern build, with camera reuse moved to Phase 10 or the lever;
  - C2: indicator values and copy instead of always-rendered ids;
  - C3: component-tier equivalents for under-fill.
- **test**:
  - F1: window-length Prometheus ranges, with the spurious `request` 1.25 explained;
  - F2: slot-row completeness HARNESS checks on bus and overlay at both ends;
  - F3: runs 3–5 range recorded, run 5 as the record;
  - F4: probe CPU wording ("startup included") and method;
  - F5: measured Layer-7 peak recorded (45 / 135, unattributed).

The evidence was re-run as run 5 on the final harness: passed, and the numbers above are updated.

---

## Accepted Deferrals

- (none surfaced in this devloop)

---

## Rollback Procedure

1. Start commit: `bc82880fea30c9a68dbfa6a08a4d6c4630b11169`
2. `git diff bc82880f..HEAD`
3. **Safe-revert unit**: the whole commit (documentation + the opt-in test harness; no product code).

---

## Gate 2 — Full Validation

`DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` on the final tree: **TOTAL_RESULT=OK** (L1 OK 5s, L2 OK 5s, L3 OK 161s, L4 OK 156s, L5 OK 1s, L6 OK 5s, L7 OK 1476s — Rust env-tests passed + 29-spec browser suite). Attempt 1 (the first invocation was killed by the Lead's tool timeout mid-L7, not a failure).
