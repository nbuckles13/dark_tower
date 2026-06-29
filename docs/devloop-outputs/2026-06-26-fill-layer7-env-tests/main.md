# Devloop Output: Fill scripts/layer7.sh — restore env-tests in the validation pipeline

**Date**: 2026-06-26
**Task**: Implement `scripts/layer7.sh` (currently a `wave2-pending` stub) so Layer 7 actually runs the `crates/env-tests` suite against the live Kind cluster; trim duplicated protocol prose from SKILL.md Step 6; update runbook §6.7; confirm/amend ADR-0033 §4 budget framing for Layer 7.
**Specialist**: infrastructure (paired with operations + test)
**Mode**: Agent Teams (v2) — full
**Branch**: `feature/browser-client-join-task-56`
**Duration**: Planning + implementation 2026-06-26; live validation + close-out 2026-06-29 (spanned an API session-limit interruption; the agent team was stopped and the Lead completed final review, live validation, and commit directly).

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `c4c6138239fbe356c4695d3c0bf217d9768c4387` |
| Branch | `feature/browser-client-join-task-56` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@session-e8d804c3` |
| Implementing Specialist | `infrastructure` |
| Iteration | `1` |
| Security | `security@session-e8d804c3` |
| Test | `test@session-e8d804c3 (paired collaborator)` |
| Observability | `observability@session-e8d804c3` |
| Code Quality | `code-reviewer@session-e8d804c3` |
| DRY | `dry-reviewer@session-e8d804c3` |
| Operations | `operations@session-e8d804c3 (paired collaborator)` |
| Semantic Guard | `semantic-guard@session-e8d804c3` |

---

## Task Overview

### Objective
Layer 7 of the polyglot validation pipeline (`scripts/layer-all.sh`) has been a no-op stub emitting `STATUS=N/A REASON=wave2-pending` since 2026-05-04 (ADR-0033 Wave 1). When SKILL.md Step 6 collapsed to "run `./scripts/layer-all.sh`" on 2026-05-12, env-tests stopped running entirely — yet the SKILL.md narrative continued to describe a 4-step env-test protocol as if it executed. ~6 weeks of devloops (tasks #29 onward) silently skipped env-test verification. This task (task #56) fills the stub so env-tests run, and removes the misleading narrative.

### Scope
- **Service(s)**: pipeline tooling (`scripts/`), process docs (`.claude/skills/devloop/SKILL.md`, `docs/runbooks/`, `docs/decisions/adr-0033`)
- **Schema**: No
- **Cross-cutting**: Yes — affects the validation pipeline used by all devloops + CI

### Debate Decision
NOT NEEDED — implementation of an already-decided contract (ADR-0033 Layer 7, ADR-0030 host-side cluster helper, ADR-0028 §7 amendment). No new design.

---

## Cross-Boundary Classification

<!-- Filled by implementer during planning; reviewers may upgrade at Gate 1. -->

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `scripts/layer7.sh` | Mine | infrastructure |
| `scripts/layer7.test.sh` (new) | Mine | infrastructure |
| `scripts/layer3.sh` (wire layer7 self-test, +1 line) | Mine | infrastructure |
| `docs/runbooks/devloop-validation.md` (§6.7 + `wave2-pending` refs) | Mine | infrastructure / operations |
| `docs/specialist-knowledge/infrastructure/INDEX.md` (Layer 7 pointer) | Mine | infrastructure |
| `infra/devloop/dev-cluster` (one `# COUPLED:` cross-ref comment by the status echo lines — @operations condition 5; no behavior change) | Mine | infrastructure |
| `scripts/layer-all.sh` — per-layer WARN exclusion for L7 + `final_exit` floor (orchestrator) | Mine / infrastructure | reviewed by @operations + @test (orchestrator) |
| `scripts/layer-all.sh` — `LAYER_SCRIPT_DIR` test seam, gated `DEVLOOP_TEST=1` (NOT a GSA) | Mine / infrastructure | @security co-reviews |
| `scripts/layer7.sh` — FOUR redirect/execute test seams ALL gated `DEVLOOP_TEST=1` (`DEVLOOP_ENV_TEST_CMD` suite-cmd, `DEVLOOP_DEV_CLUSTER_BIN`, `DEVLOOP_HELPER_SOCKET`, `DEVLOOP_PORTS_JSON`) — @operations-flagged, same class as `LAYER_SCRIPT_DIR`; production uses fixed ADR-0030 canonical paths | Mine / infrastructure | @security co-reviews (boundary call) |
| `scripts/lang/_common.sh` — TWO new enums (`SKIPPED-NO-CLUSTER` rank 2/exit 0, `PRECONDITION_FAILURE` rank 6/exit 2) inserted into the worst-wins ladder + `layer_now`/`emit_step_duration` timing helpers | **Shared (status-contract) — Minor-judgment** (the worst-wins precedence ladder is @operations' aggregation contract per the ADR-0033 debate; inserting enums is a semantic change to it, not a silent "Mine") | @operations co-sign (ladder + §6 amendment) + @test (locked `_common.test.sh` assertions) (+ @code-reviewer) |
| `scripts/lang/_common.sh` — `assert_no_ci_sentinel_leak` `LAYER_SCRIPT_DIR`-in-CI rejection clause (CI-SENTINEL-LEAK control, task #47 §J/C boundary; NOT a GSA, but the security-control TIER requires the trailer per ADR-0024 §6.3) | **Minor-judgment** | @security co-sign — `Approved-Cross-Boundary: security` on this hunk, co-review at Gate 3 |
| `scripts/lang/_common.test.sh` — additive precedence/exit-code assertions for both new enums | **Shared (status-contract)** | @test |
| `scripts/lang/_layer_skeleton.test.sh` — glob skips `*.test.sh` (fixes false-fail on `layer7.test.sh`) | Mine | infrastructure |
| `scripts/layer-all.test.sh` (new — @test-OWNED orchestrator fixture "Test B"; runs real `layer-all.sh` via the `LAYER_SCRIPT_DIR` seam, wired into `layer3.sh`) | Not mine, Domain (test) | @test (owner-implements) |
| `docs/TODO.md` (Loki cold-start no-retry deferral pointer — env-tests crate hard-asserts `is_loki_available` with no retry; out of scope per §I) | Mine | infrastructure (deferral logged by @dry-reviewer) |
| `.claude/skills/devloop/SKILL.md` (§Step 6 protocol trim + `SKIPPED-NO-CLUSTER no-cluster-ci` one-liner — Layer 7 row + :419 STAY per user ruling) | Shared (process doc) | @team-lead / process — reviewed by @operations, @test |
| `docs/decisions/adr-0033-polyglot-validation-pipeline.md` (§4 envelope sentence; §3 row/table UNCHANGED per user ruling; **§6 enum-enumeration amendment — AUTHORIZED + LANDED** (Lead ruling 2026-06-26): added `SKIPPED-NO-CLUSTER` (exit-0 row) + `PRECONDITION_FAILURE` (exit-2 row) to the §6 exit-code table + canonical enum list + a task-#56 amendment block (mirroring task-#52's FAIL-MISSING-VERB) + the @security assurance-boundary/backstop-scope note. §6 must enumerate what we ship (coherence), per @code-reviewer's B1; §3 stays unchanged) | Shared (ADR) | co-signers: @operations, @security (+ @test, @observability, @client per consensus block) |

None of these are Guarded Shared Areas. `_common.sh`, SKILL.md and ADR-0033 are shared contracts — I will not merge edits to them without @team-lead sign-off and reviewer acknowledgement. The `_common.sh` enum extension (§J item 2 option B) is a **shared status-contract change**: it requires @test concurrence (the `_common.test.sh` assertions are theirs) and @team-lead adjudication at Gate 1.

---

## Planning

### A. Problem restated in mechanism-language

The visible defect is "`layer7.sh` is a one-line stub". The *mechanism* defect is wider: Layer 7 silently produces `N/A REASON=wave2-pending`, which `aggregate_worst_status` ranks **above OK** — so the pipeline's top-line stayed green while env-test coverage was zero for ~6 weeks. The fix is therefore not "emit a different token" but "make Layer 7 do real work AND fail loud when it can't" — i.e. it must distinguish four lanes the stub collapsed into one:

1. **OK** — env-tests ran green.
2. **test FAIL** (exit 1, *implementer* lane) — a test regression in the diff.
3. **infra / precondition failure** (exit 2, *operator* lane) — a cluster was **expected** (helper present) but couldn't be brought up, or the test run hit `connection refused|timed out|connection reset|broken pipe`. Must NOT be masked as a skip and must NOT consume an implementer attempt.
4. **SKIPPED-NO-CLUSTER REASON=no-cluster-ci** (exit 0, **CI ONLY**) — `GITHUB_ACTIONS` is set, so no Kind cluster exists and none is provisioned. The ONLY clean-skip case; CI stays green. A **local** run with no/dead helper is NOT this lane — it's a loud `PRECONDITION_FAILURE` (exit 2, the silent-skip-hole closer; §B.2/§J.2). This replaces `wave2-pending`. **Layer 7 remains always-run** — there is NO diff-relevance gate; the only suppressor is "running in CI with no cluster".

The "fill one script" framing is too narrow on the *failure-laning* axis (four lanes, not one), but — per the user ruling — NOT on the classification axis: Layer 7 stays always-run, so no ADR-0033 §3 / SKILL.md reclassification (see §D).

### B. layer7.sh design (the script) — REVISED per user ruling (always-run, no relevance gate)

Structure mirrors layers 3/6 (`set -euo pipefail`, source `_common.sh`, `layer_lifecycle_begin`, `tee_collect_statuses`) and adds a sourcing-guard (`[[ "${BASH_SOURCE[0]}" == "$0" ]] && __layer7_main`) so the pure helpers are unit-testable. **No relevance gate, no `DEVLOOP_ENV_TEST_FORCE_RUN` flag** (removed — Layer 7 is always-run; the only suppressor is "no cluster to run against"). `_changed_helpers.sh` IS sourced — but only for the Phase-1b `diff_touches_path "infra/kind/"` check (DRY #6), not a relevance gate.

Execution order:

1. Source `_common.sh`; `layer_lifecycle_begin 7` (lifecycle DURATION covers bring-up + tests; every exit path below emits a STATUS line through `tee_collect_statuses` — no silent edge).
2. **Environment / cluster gate — FAIL-CLOSED, SOCKET-PRESENT-FIRST** (the silent-skip-hole closer; §J.2). The clean-skip lane (`SKIPPED-NO-CLUSTER`, exit 0) is reachable ONLY when the socket is **absent AND** `GITHUB_ACTIONS` is set. **Order matters** (operations/team-lead's "GITHUB_ACTIONS + *no usable cluster*" qualifier): socket-presence (`[[ -S ]]`, not a dev-cluster call) is checked FIRST, so a present socket proceeds regardless of CI (forward-compatible — a future CI that provisions a helper runs env-tests with zero code change). `GITHUB_ACTIONS` only disambiguates the socket-ABSENT case.
   - **(a) socket PRESENT + reachable** → cluster **expected** → Phase 1 preconditions (regardless of CI). See §C.2.
   - **(b) socket PRESENT but DEAD** (`dev-cluster status` connection-refused) ⇒ `precondition_fail helper-unreachable` (exit 2). NOT a skip.
   - **(c) socket ABSENT + `GITHUB_ACTIONS`** ⇒ `emit_status SKIPPED-NO-CLUSTER no-cluster-ci` → exit 0. The ONLY skip case. CI green.
   - **(d) socket ABSENT + local** ⇒ `precondition_fail local-helper-not-running` (exit 2, LOUD) — **the regression-closer**: a local devloop expects a cluster; a missing helper is never a silent pass. (Run only layers 1-6 via the individual `layerN.sh`.)
   - **Safety property (operations):** the only exit-0 no-cluster skip is CI; any LOCAL devloop with no reachable helper is loud `PRECONDITION_FAILURE`, never a silent `SKIP` — so env-tests cannot silently skip on a local code devloop. (Tested: `layer7.test.sh` pins all four, incl. the present+CI→runs forward-compat case.)
3. `BASE_REF=$(lang/_get_base_ref.sh)` (emits the `BASE_REF=` anchor + populates `changed-files.layer-7`). Source `_changed_helpers.sh` for `diff_touches_path`. Cluster-present path only.

**TWO-PHASE CLASSIFIER (team-lead ruling #3 — the log-grep is RETIRED).** Grepping suite output for `connection refused|timed out|…` reintroduces reverse-masking (a real test FAIL whose output contains "connection refused" swallowed as infra) — the 6-week-skip bug one level down. Replaced by a deterministic pre-suite gate. `precondition_fail "<token>" "<cause>" "<fix>"` emits a greppable `PRECONDITION_FAILURE: <cause>` stderr banner AND `emit_status PRECONDITION_FAILURE "<token>" | tee_collect_statuses`, then exits — the lifecycle maps the enum to exit 2 (no `trap` disarming → `LAYER=`/`DURATION=` anchors preserved; resolves @observability #1).

4. **PHASE 1 — pre-suite preconditions (deterministic; the ONLY infra lane → PRECONDITION_FAILURE/exit 2 on any failure):**
   - (a) **Cluster readiness**: `dev-cluster status` (parse health §C.1); if not ready and `Setup in progress: true`, poll until idle (bounded), re-check; still not ready → `dev-cluster setup`. Non-zero `setup` → `precondition_fail cluster-setup-failed`.
   - (b) **Infra-change detection**: `diff_touches_path "infra/kind/"` (DRY #6, catches untracked) → if touched: log triggering files, `dev-cluster teardown` then `setup`. Failure → `precondition_fail cluster-rebuild-failed`.
   - (c) **Rebuild services**: `dev-cluster rebuild-all`. Failure → `precondition_fail cluster-rebuild-failed`.
   - (d) **Ports + URLs**: read `/tmp/devloop/ports.json` (missing → `precondition_fail ports-json-missing`), export `ENV_TEST_{AC,GC,PROMETHEUS,GRAFANA,LOKI}_URL` from `.container_urls.*` (Loki optional/omit-if-empty — §J.7).
   - (e) **Service-health confirm**: **bounded POLL** (`__wait_cluster_ready`, `DEVLOOP_HEALTH_BUDGET=300s`) for pods-ready BEFORE the suite — NOT a one-shot. (The first live run surfaced that `rebuild-all` issues `kubectl rollout restart` and returns *before* pods settle, so a one-shot check spuriously tripped `cluster-unhealthy`; see §Issues Encountered.) Still-unhealthy after the budget → `precondition_fail cluster-unhealthy`. This is what makes Phase 2's "any non-zero = the diff's fault" sound.
   Each Phase-1 step emits a `STEP=<name> DURATION=<secs>` stderr line (@observability #2 — not a bespoke `*_SECS=`).
5. **PHASE 2 — run the suite (cluster confirmed healthy; NO grep):** `timeout 600 ${DEVLOOP_ENV_TEST_CMD:-cargo test -p env-tests --features all} 2>&1 | tee /tmp/devloop/env-test-output.log`; `rc=${PIPESTATUS[0]}` under `set +e`. (`DEVLOOP_ENV_TEST_CMD` = documented **test seam** so the self-test drives Phase 2 deterministically — `cargo test` already runs arbitrary code, so no new trust boundary; flagged to @security.)
6. **Classify (deterministic, no grep):** `rc==0` → `emit_status OK env-tests-passed`; `rc!=0` → `emit_status FAIL env-tests-failed` (exit 1, implementer lane — incl. `timeout`'s 124: a hang on a confirmed-healthy cluster is the diff's problem). **Load-bearing asymmetry:** when uncertain, default to FAIL/loud, never infra/swallow.

**Four terminal STATUS lines, all through the lifecycle:** `SKIPPED-NO-CLUSTER no-cluster-ci` (exit 0) / `OK` (exit 0) / `FAIL` (exit 1) / `PRECONDITION_FAILURE` (exit 2).

### C. Two cluster seams (need @operations)

**C.1 — dev-cluster status parsing.** `dev-cluster status` exit 0 means only "the status query ran", **not** "cluster healthy"; and the structured `data` JSON is consumed inside the client and re-emitted as **human-readable lines on stderr** (`Cluster exists:  true`, `Pods healthy:  true`, `Setup in progress:  false`). There is no `--json`. So readiness detection must grep that stderr text (`grep -qE 'Pods healthy: +true'` etc.). This is the only non-hermetic seam. Adding `--json` to the helper is **out of scope** (ADR-0030 redesign). I will pin the parse with a fake-`dev-cluster`-on-PATH fixture in the self-test. **@operations**: confirm this is the sanctioned contract or point me at a cleaner readiness signal.

**C.2 — "no cluster in CI" vs "cluster expected but down" detection (team-lead's explicit question).**

> **DECISION (task #56, team-lead-arbitrated — this records the ACTUAL outcome implemented in `layer7.sh`, NOT a superseded draft):** the no-cluster lane is discriminated by **`GITHUB_ACTIONS`**, *not* socket-presence alone. The ONLY exit-0 skip is CI (`GITHUB_ACTIONS` set + socket absent). A LOCAL run with a missing or dead helper is a LOUD `PRECONDITION_FAILURE` (exit 2) — env-tests **never silently skip on a local devloop**. A socket-presence-only predicate was explicitly **REJECTED** because it silently skipped the local no-helper case (the 6-week silent-skip regression this task closes). (If a reader ever sees §C.2 say socket-absence alone ⇒ a clean skip "incl. local", that is a stale pre-arbitration draft — the on-disk text below is authoritative.)

Four-way, with **socket-presence checked FIRST** (`[[ -S ]]`, not a `dev-cluster` call — so a PRESENT socket runs even in CI, forward-compatible) and `GITHUB_ACTIONS` splitting the socket-ABSENT tail. Socket-absence ALONE cannot distinguish "CI legitimately has no cluster" (a clean skip) from "a local dev forgot to start the helper" (the silent-skip regression this task closes), so the absent case must split on `GITHUB_ACTIONS`:
- **Socket PRESENT + reachable** ⇒ a cluster is expected ⇒ **RUN** (Phase 1 bring-up / Phase 2 suite), regardless of CI — forward-compatible (a future CI that provisions a helper runs env-tests with no code change).
- **Socket PRESENT but DEAD** (`dev-cluster status` connection-refused) ⇒ **PRECONDITION_FAILURE `helper-unreachable` (exit 2)** — a cluster was expected but the helper crashed; NOT a clean skip.
- **Socket ABSENT + `GITHUB_ACTIONS` (CI)** ⇒ **SKIPPED-NO-CLUSTER `no-cluster-ci` (exit 0)** — the ONLY skip lane; normal CI has no socket so it lands here (preserving the "no `dev-cluster` call in normal CI" property).
- **Socket ABSENT + local** ⇒ **PRECONDITION_FAILURE `local-helper-not-running` (exit 2, LOUD)** — a local devloop ALWAYS expects a cluster; never a silent skip. (To run only layers 1-6, invoke the individual `scripts/layerN.sh`.)

**RESOLVED (team-lead arbitration; + @security + @operations):** the only exit-0 no-cluster skip is CI (`GITHUB_ACTIONS` + socket absent); any local devloop with no reachable helper is loud `PRECONDITION_FAILURE` — **env-tests cannot silently skip on a local code devloop.** An earlier "socket-presence is the single predicate; `GITHUB_ACTIONS` dropped" framing was SUPERSEDED — socket-absence alone silently skipped a local missing-helper (the regression). The "real Gate-2 helper always present ⇒ a drift/down cluster lands in PRECONDITION_FAILURE, never a silent skip" property holds. All four cases + a present-socket-in-CI→runs case are tested in `layer7.test.sh`.

### D. Classification — USER RULING: keep Layer 7 always-run (no reclassification)

**Resolved (2026-06-26, user ruling relayed by @team-lead).** Layer 7 **stays always-run**. The earlier reclassification proposal (deployment-surface-gated → N/A `no-relevant-changes`) is **REJECTED**. Consequences:
- ADR-0033 §3 + the §3 always-run/skip table + SKILL.md:419 "always-run" prose **stay unchanged** — *less* doc surgery than originally scoped.
- The relevance gate (`__env_test_relevant_surface`, `diff_touches_path`, the `DEVLOOP_ENV_TEST_FORCE_RUN` flag) is **removed entirely**. There is no N/A `no-relevant-changes` lane; `wave2-pending` is simply deleted, not replaced by a diff-gated N/A.
- Layer 7 always *attempts* to run env-tests; the only suppressor is "no cluster to run against" → the **SKIPPED-NO-CLUSTER no-cluster-ci (exit 0)** CI lane (§B step 2, §C.2). This is the env-awareness that keeps `ci.yml` (no Kind cluster, not provisioned this round) green without masking real failures.
- The original §D conflict (a scripts-only diff being "always-run per §3" vs "N/A per the rule") **dissolves**: always-run wins; a scripts-only diff on a cluster-equipped devloop *does* run env-tests (which is why item C below is now automatic).

### E. Budget framing (scope 4) + per-layer WARN fix (§J.B)

Confirmed in code: `layer-all.sh:114` computes the 90s p95 *total* budget as `layer_dur[3] + layer_dur[6]` only — Layer 7 is **already excluded from the 90s total**. ADR-0033 §4 prose says "always-run set (Layers 3, 6 + reviewer panel cost is excluded)" but never names Layer 7's own envelope → amend §4 with one sentence noting Layer 7's **separate ~10–15 min envelope** outside the 90s budget.

**Two `layer-all.sh` edits (team-lead rulings #4 + B):**
- **(B) per-layer WARN exclusion:** the per-layer WARN at `layer-all.sh:108` fires inside the unconditional `for n in 1..7` loop, so a real Layer-7 run (~900s) emits `WARN BUDGET_BREACH LAYER=7 DURATION=~900 BUDGET=20` every run — noise that trains operators to ignore the token. Guard it: `[[ $n -ne 7 && $dur -gt $budget_secs_per_layer ]]`. §4 amendment documents Layer 7's exemption from both the 90s total and the 20s per-layer warn (its own ~10–15 min envelope).
- **(#4) final-exit propagation (team-lead-approved form):** `final_exit = status_to_exit_code(total_result)` — the single-source enum→exit mapping (dry-reviewer F1), cleaner than a `worst(rc)` fold — with a **non-demotion guard**: the loop sets a `final_exit=1` flag if any layer process exited non-zero, and if the aggregate maps to 0 while that flag is set, the pipeline stays non-zero (a detected failure is never silently demoted to success). Net: OK/N/A/SKIPPED-* → 0; FAIL → 1; PRECONDITION_FAILURE/FAIL-MISSING-VERB/UNKNOWN → 2. So a Layer-7 PRECONDITION_FAILURE reaches `LAYER_ALL_EXIT=2` (which `emit_gate2_verdict` derives GATE2 from). Also repairs the latent `FAIL-MISSING-VERB`(exit2)→reported-exit1 collapse. **Verified end-to-end** via the `LAYER_SCRIPT_DIR` seam (§H): a fake setup-failing layer-7 → `LAYER_ALL_EXIT=2` + summary cell `PRECONDITION_FAILURE` + `GATE2=FAIL`.

The §4 ADR amendment frames Layer 7's ~10–15 min envelope as the **COMMON case** for devloops (@operations' framing — `/devloop` is for code tasks, which run env-tests), exempt from BOTH the 90s always-run total AND the 20s per-layer warn; on CI/no-cluster it contributes ~0s (`SKIPPED-NO-CLUSTER no-cluster-ci`).

### F. SKILL.md trim (scope 2) — REVISED (always-run row kept)

Remove the 4-step protocol (lines 417–425, now duplicated in `layer7.sh`); keep ~3 Lead-policy lines: Layer 7 = 2 attempts (separate from layers 1–6's 3); infra failures don't consume attempts, test failures do; first-run cluster setup (~7 min) doesn't count toward attempts. **The Layer 7 row (line 399) + the :419 always-run prose STAY as-is** (user ruling §D — no reclassification). Add a one-liner that on CI/no-cluster Layer 7 cleanly skips (`SKIPPED-NO-CLUSTER REASON=no-cluster-ci`, exit 0) so the always-run row isn't misread as "must have a cluster in CI." Leave surrounding tables/sections intact.

### G. Runbook (scope 3) — REVISED lanes

Rewrite §6.7 to the real failure-mode → wrapper → REASON-token → fix table (matching §6.1–§6.6 format), documenting the **four lanes**:
- `SKIPPED-NO-CLUSTER` REASON=`no-cluster-ci` — expected on CI ONLY (GITHUB_ACTIONS); exit 0; clean skip, NOT a failure.
- `env-tests-failed` — test lane, exit 1, implementer.
- `PRECONDITION_FAILURE` REASON ∈ {`cluster-setup-failed`,`cluster-rebuild-failed`,`ports-json-missing`,`cluster-unhealthy`} — operator lane, exit 2; Phase-1 pre-suite gate failed (cluster expected but bring-up/health failed); does NOT consume implementer attempts. (No `env-tests-infra-error` token — the grep lane is retired per ruling #3.)
- `env-tests-passed` — OK, exit 0.
Grep-fix the other `wave2-pending` references (lines 86, 465, 469, 559 + the §enum table) so the runbook stops documenting a stub that no longer exists. **Do NOT introduce an `N/A no-relevant-changes` row** (that lane was removed by the user ruling).

### H. Self-verification (scope 5) — REVISED

New `scripts/layer7.test.sh` (follows `audit-suppressions-check.test.sh` + `_test_helpers.sh` convention), wired into `layer3.sh` (no `*.test.sh` auto-runner). Tests the pure cores + the env-gate **without a real cluster**:
- **cluster-availability gate**: `DEVLOOP_HELPER_SOCKET` → non-existent path ⇒ `SKIPPED-NO-CLUSTER no-cluster-ci` + exit 0 (the CI-green case — replaces the voided "clean diff → N/A" case).
- **Phase-2 classification (two-phase, NO grep)**: with the socket "present" (dummy file) + a fake `dev-cluster` whose Phase-1 all-succeeds, drive Phase 2 via the `DEVLOOP_ENV_TEST_CMD` seam: `=true` ⇒ `OK`/exit 0; `=false` ⇒ `FAIL env-tests-failed`/exit 1; (optionally `='sh -c "echo connection refused; exit 1"'` ⇒ STILL `FAIL` — the load-bearing proof that the retired grep no longer swallows a test FAIL containing infra words).
- **Phase-1 precondition**: fake `dev-cluster setup` exits non-zero ⇒ `PRECONDITION_FAILURE cluster-setup-failed`/exit 2.

**shellcheck (code-reviewer P4 — corrected):** code-reviewer confirmed shellcheck is NOT wired into the pipeline (no guard, not in `run-guards.sh`; SKILL.md's "Shell scripts | shellcheck" row is a *manual* Gate-2 Lead checklist item), so the earlier "Layer 3 lints `*.sh`" claim was false assurance. shellcheck is also not installable in this environment (no network). Mitigation: `bash -n` clean on all touched scripts + careful manual review (the SC2155 declare-and-assign split is applied in `__cluster_ready`/`__env_test_export_urls`; all expansions quoted; array used for the suite command). Flagged as a **Gate-2 manual shellcheck run** (needs the binary) — not a self-claimed clean.

**Skeleton-linter fix (@test catch — a real regression):** `scripts/lang/_layer_skeleton.test.sh` collects layer scripts via `layer[0-9]*.sh`, which also matched the new `layer7.test.sh` (false-failing on its legitimate `trap`/`STATUS=`/`aggregate_worst_status`). AND `layer7.sh`'s own per-step `date +%s` tripped the linter's "lifecycle owns timestamps" rule. Both fixed: (1) the glob now skips `*.test.sh`; (2) per-step timing moved to `_common.sh::layer_now` + `emit_step_duration` (the `date` call lives in the lifecycle helper, as the linter's own remediation message directs), emitting `LAYER=7 STEP=<name> DURATION=<s>`. `_layer_skeleton.test.sh` 36/36.

**Orchestrator-integrity — TWO distinct tests (per @test's Test-A/Test-B split):**
- **Test A — layer7-direct (MINE, in `layer7.test.sh`):** the fake-`dev-cluster` setup-fails case → asserts `layer7.sh` in isolation emits `STATUS=PRECONDITION_FAILURE`/exit 2 + banner. This pins layer7's OWN behavior; it does NOT (and structurally cannot) exercise `layer-all.sh`'s `final_exit` aggregation.
- **Test B — orchestrator-integrity (@test-OWNED, in `scripts/layer-all.test.sh`):** runs the REAL `layer-all.sh` via the `DEVLOOP_TEST`-gated `LAYER_SCRIPT_DIR` stub seam (fast + hermetic; trivial stub layers, no real layers 1-6) and pins the `final_exit` aggregation that Test A can't reach — PRECONDITION→`LAYER_ALL_EXIT=2`, the FMV-exit-2-not-collapsed regression, SKIPPED-NO-CLUSTER→exit-0/TOTAL=OK, the STATUS=OK+exit1 non-demotion FLOOR, the seam's DEVLOOP_TEST-gating bypass-closure, and worst-wins. **24/24 green against my `_common.sh`/`layer-all.sh`.** I removed my earlier redundant layer-all-smoke from `layer7.test.sh` so there's no double-build — `layer-all.test.sh` is the canonical orchestrator test.

**Live env-test run at Gate 2 is now AUTOMATIC (§J.C, per user ruling).** Because Layer 7 is always-run, *this* devloop's Gate 2 — run inside the devloop container where the helper + cluster are eagerly up — **will** bring up/rebuild the cluster and run `cargo test -p env-tests --features all` for real, even though the diff is `scripts/`+`docs/` only. No force flag needed. The user's "env-tests may fail" caution (6 weeks unrun; GC telemetry-proxy zero coverage = task #57) therefore materializes directly at Gate 2 — the real signal we want. **No masking green** — I report ACTUAL results to @team-lead before review; we triage fix-here vs task-#57 gap together. The precondition/infra **exit-2 paths (matrix d/e)** won't occur on a healthy cluster, so those I still exercise via the fake-`dev-cluster` fixture above. Recorded in §Devloop Verification Steps.

### I. Out of scope
Browser E2E/Playwright (task #19); changing the env-tests crate; redesigning ADR-0030's helper or adding `--json`.

### J. Gate-1 plan-blocking resolutions

**ADJUDICATION RECORD (@team-lead, 2026-06-26, 8 rulings — all folded into the plan above):**
1. **Keep Layer 7 always-run** (user ruling) — no §3/§3-table/SKILL.md:419 reclassification; env-aware cluster gate instead (CI/no-cluster → clean SKIP exit 0). → §A, §B.2, §C.2, §D, §F.
2. **TWO new enums (team-lead FINAL arbitration):** `SKIPPED-NO-CLUSTER` (exit 0, rank 2) + `PRECONDITION_FAILURE` (exit 2, rank 6 — above FAIL, below FAIL-MISSING-VERB). The enum label churned (two→one→two); the arbitration is closed. **Gate: land `_common.sh` only after @test acks `_common.test.sh` additivity.** → §J.2 below.
3. **Two-phase classifier; RETIRE the log-grep** (the task spec's "grep infra patterns" is SUPERSEDED). Phase 1 (pre-suite, deterministic) = only infra lane → PRECONDITION_FAILURE; Phase 2 (healthy cluster) → ANY non-zero = FAIL. Asymmetry: uncertain ⇒ FAIL/loud. → §B.4–6.
4. **`layer-all.sh` in scope:** exclude L7 from per-layer WARN + `final_exit = worst(final_exit, rc)` (2>1>0) so `LAYER_ALL_EXIT=2` survives. → §E.
5. **Self-validation hole closed by always-run** (this Gate 2 runs live). `DEVLOOP_LAYER7_FORCE` dropped (not load-bearing). Operations' raw-live-evidence-at-Gate-2 requirement stands. → §H.
6. **DRY:** `diff_touches_path "infra/kind/"` not raw `git diff`. → §B.4b.
7. **Observability:** per-step timing = `STEP=`/`DURATION=` (not `*_SECS=`); Loki — confirm whether the dev cluster deploys it; absent-Loki hard-fail surfaces at Gate 2 as triage (do NOT edit env-tests crate). → §B.4, §J.7.
8. **GREENLIT** to start `layer7.sh` + `layer7.test.sh` + `layer3.sh` wiring + runbook §6.7 now; §4 envelope amendment in scope; shared-contract edits proceed (hold `_common.sh` enum per #2 gate).

The two original crux decisions, with rationale, follow.

#### J.1 — N/A-on-no-relevant-diff vs Layer-7-always-run → **RESOLVED by user ruling: keep always-run, NO reclassification**

**RESOLVED — user-CONFIRMED always-run (2026-06-26, via @team-lead, FINAL).** Keep Layer 7 **always-run**; do NOT reclassify ADR-0033 §3 / the §3 table / SKILL.md:419 (they stay "always-run"). The relevance gate, the `N/A no-relevant-changes` lane, and any skip-override (`DEVLOOP_ENV_TEST_FORCE_RUN`) are removed — a skip toggle would re-open the silent-skip this task exists to kill.

**User's rationale (the §J.1 justification of record):** `/devloop` is only invoked for *meaty / code* tasks; docs-only changes don't go through devloop at all. So always-run costs ~nothing in practice (the devloops that run env-tests are exactly the ones where integration coverage matters) while *guaranteeing* coverage on every code devloop. The only conditional is the **environment/cluster gate** (not a diff gate): CI / no-cluster-provisionable → clean SKIP (exit 0, never attempt `dev-cluster setup` — GHA has no Kind); local cluster up → run; local cluster down → `dev-cluster setup` (doesn't count toward attempts) → run or `PRECONDITION_FAILURE`.

> *(Rejected alternative, for the record only)* — a deployment-surface gate (`crates/`/`proto/`/`infra/` → run, else N/A) would have saved 10–15 min on the rare docs-in-a-code-devloop case; the user judged the guaranteed-coverage value higher and the cost negligible given how `/devloop` is actually used. Not implemented.

#### J.2 — STATUS-enum changes → **FINAL (team-lead ARBITRATION, CLOSED): TWO new enums**

The enum label churned (two → one → two) across several Lead messages; the team-lead's **final arbitration** (2026-06-26) is binding and closed: **TWO new enums.** @test's semantic-tier argument is decisive — `SKIPPED-*` = "the verb APPLIES but a condition blocked it" (Layer 7 is always-run, so env-tests apply; a missing CI cluster blocked them — same tier as `SKIPPED-NO-DIFF`); `N/A` = "the verb doesn't APPLY to this lang" (proto placeholders), the wrong tier. The summary table renders **enum-only** (REASON is stderr), so `SKIPPED-NO-CLUSTER` self-documents the lane where a bare `N/A` would be ambiguous. *(If any reviewer reopens N/A-vs-SKIPPED: "Lead arbitrated — closed.")*

**(i) `SKIPPED-NO-CLUSTER`** (exit 0, `__status_rank` **rank 2**, below OK): the CI clean-skip lane. `status_to_exit_code` → exit-0 arm. Below OK so a green CI run reports `TOTAL_RESULT=OK`. REASON `no-cluster-ci`.

**(ii) `PRECONDITION_FAILURE`** (exit 2, `__status_rank` **rank 6** — @test/team-lead DECISION 2): precedence `UNKNOWN(8) > FAIL-MISSING-VERB(7) > PRECONDITION_FAILURE(6) > FAIL(5) > N/A(4) > OK(3) > SKIPPED-NO-CLUSTER(2) > SKIPPED-NO-DIFF(1) > SKIPPED-NO-VERB(0)`. It outranks `FAIL` but sits BELOW `FAIL-MISSING-VERB` — **rationale (team-lead, corrects my earlier "sick env invalidates all gates"):** the Kind cluster is needed ONLY by Layer 7, so a cluster-down PRECONDITION does NOT invalidate layers 1-6; a missing verb wrapper is a persistent pipeline-machinery defect, more fundamental than a transient cluster-down. (Integer 6 not 5: inserting `SKIPPED-NO-CLUSTER` below OK shifts the contiguous integers up by one; a literal 5 would collide with FAIL.) `status_to_exit_code` → exit-2 arm (with `FAIL-MISSING-VERB|UNKNOWN`).

**Four-way fail-closed discriminator (the silent-skip-hole closer — @operations/@security blocker, team-lead-confirmed).** `SKIPPED-NO-CLUSTER` (exit 0) is reachable **ONLY in CI** — a local devloop NEVER exit-0-skips env-tests (the original hole was "local, forgot the helper → silent skip"). `GITHUB_ACTIONS` is the required discriminator (socket-absence ALONE cannot tell "CI has no cluster" from "local forgot the helper"). The four cases, all tested in `layer7.test.sh`:
1. `GITHUB_ACTIONS` set → `SKIPPED-NO-CLUSTER` (exit 0) — the ONLY skip; before any dev-cluster call.
2. local, socket present + healthy/setup-ok → run.
3. local, socket present + DEAD (`dev-cluster status` connection-refused) → `PRECONDITION_FAILURE helper-unreachable` (exit 2).
4. local, socket ABSENT (not CI) → `PRECONDITION_FAILURE local-helper-not-running` (exit 2, LOUD) — **the regression-closer**. Verified directly: absent+CI→SKIPPED-NO-CLUSTER/0, absent+local→PRECONDITION/2, present+dead→PRECONDITION/2.

**Bonus — (B) also simplifies the script:** because `status_to_exit_code(PRECONDITION_FAILURE)=2`, `precondition_fail` just `emit_status PRECONDITION_FAILURE … | tee_collect_statuses` and the **lifecycle** computes exit 2 — no `trap - EXIT` disarming, so the `LAYER=` stderr anchor is preserved on the precondition lane (resolves my observability note). Every Layer-7 exit path is then a uniform `emit_status … | tee_collect_statuses`.

**Blast radius (bounded, sanctioned by @team-lead):** `_common.sh` (`__status_rank` + `status_to_exit_code` + the doc comment block), `_common.test.sh` (additive assertions — verified it uses *relative* `assert_aggregate` comparisons, not absolute rank numbers, so insertion is **non-breaking**), runbook §6.7 + the §enum table, and the SKILL.md "Layer N/A justification" enum list. The STATUS-line regex in `tee_collect_statuses`/`parse_status_line` already accepts any non-space token, so no parser change. Marked **Shared (status-contract)** in the Cross-Boundary table; @test owns the locked tests, so their concurrence is required before I land it.

**The "mirror layer-all.sh's PRECONDITION_FAILURE pattern" instruction is honored in spirit:** same name, same exit 2, same operator-lane semantics, same greppable stderr banner — upgraded to a first-class enum because the per-layer summary-row context demands it. If @test/@operations object to touching `_common.sh`, the fallback is (a) with `STATUS=UNKNOWN REASON=cluster-precondition-failed` + a runbook §6.7 disambiguation note that "Layer-7 UNKNOWN + a `cluster-*`/`env-tests-infra-error` REASON = infra lane, not a script bug" — workable but strictly worse for triage.

**ORCHESTRATOR INTEGRITY — does the operator lane actually SURVIVE `layer-all.sh`? (team-lead item A; @operations raised it).** I traced it. With enum (B): `layer7.sh` emits stdout final line `STATUS=PRECONDITION_FAILURE REASON=layer7-summary`; the loop's `parse_status_line(layer-7.log)` reads it → `layer_status[7]=PRECONDITION_FAILURE` ✓ (summary table renders it, NOT UNKNOWN); the TOTAL aggregation `aggregate_worst_status total PRECONDITION_FAILURE` → `TOTAL_RESULT=PRECONDITION_FAILURE` ✓. So the summary the Lead reads to apply "infra failures don't consume attempts" **does** distinguish the operator lane. **The one residual the team-lead is right about:** `layer-all.sh`'s loop does `if ! layer7.sh; then final_exit=1`, and the tail `exit "$final_exit"` collapses the process exit to **1** — so the *exit code* (not the summary) loses the 1-vs-2 distinction. This is actually a **pre-existing latent bug**: today `FAIL-MISSING-VERB`/`UNKNOWN` layers (exit 2) also collapse to `final_exit=1`. **Fix:** after computing `total_result`, propagate a principled exit — `mapped=$(status_to_exit_code "$total_result")`, then `final_exit` = `mapped`, but **never demote a loop-detected failure to 0** (`if mapped==0 && a layer exited non-zero → final_exit=1`) so the existing "any non-zero layer ⇒ non-zero pipeline" safety net is preserved. Net: OK/N/A/SKIPPED → 0; FAIL → 1; PRECONDITION_FAILURE/FAIL-MISSING-VERB/UNKNOWN → 2. CI (`ci.yml`) only checks non-zero, so exit 2 still fails the job — no CI regression, just added resolution. This puts `scripts/layer-all.sh` in the changeset (also needed for §J.B). End-to-end coverage is the orchestrator-integrity test in §H. **@operations + @test: please confirm the exit-2 propagation + the non-demotion guard are what you want** (the alternative is leaving `final_exit` as any-nonzero→1 and relying solely on `TOTAL_RESULT`/`layer_status[7]` in the summary — which already distinguishes the lane, so the exit-code fix is robustness, not strictly required for the Lead's policy).

---

## Pre-Work

None.

---

## Implementation Summary

Implemented per the resolved design (always-run, four lanes, two-phase classifier, no log-grep). All self-tests green (`layer7.test.sh`: 20/20; `_common.test.sh`: 45/45 incl. the new-enum cases).

- **`scripts/layer7.sh`** — replaced the `wave2-pending` stub with the full layer. **Four-way env gate** (§B.2): `GITHUB_ACTIONS`→`SKIPPED-NO-CLUSTER no-cluster-ci` (exit 0, CI-only); local+no-socket→`PRECONDITION_FAILURE local-helper-not-running` (exit 2, the silent-skip-hole closer); local+dead-socket→`PRECONDITION_FAILURE helper-unreachable` (exit 2); local+live→run. Phase 1 (deterministic pre-suite: readiness/setup, `infra/kind/` teardown+setup via `diff_touches_path`, `rebuild-all`, ports.json→`ENV_TEST_*` URLs, **bounded health POLL**) → `precondition_fail` → `PRECONDITION_FAILURE` (exit 2). Phase 2 runs `cargo test -p env-tests --features all`; any non-zero → `FAIL` (exit 1). `BASH_SOURCE`-guard for testability; `DEVLOOP_TEST`-gated `DEVLOOP_ENV_TEST_CMD` seam + path seams. Per-step `STEP=/DURATION=` stderr timing. dev-cluster by repo path (no login shell in `podman exec`).
- **`scripts/lang/_common.sh`** — TWO new STATUS enums: `SKIPPED-NO-CLUSTER` (rank 2, below OK; exit-0 arm) + `PRECONDITION_FAILURE` (rank 6 — above `FAIL`, below `FAIL-MISSING-VERB`; exit-2 arm), precedence/exit-code comment block updated. **Pending @test additivity ack** (locked-test owner) — verified non-breaking (original assertions intact; relative `assert_aggregate`). (A teammate also added a `LAYER_SCRIPT_DIR` CI-presence-rejection to `assert_no_ci_sentinel_leak` here — security hardening, left intact.)
- **`scripts/lang/_common.test.sh`** — additive aggregate + exit-code assertions for both enums (+10 cases).
- **`scripts/layer-all.sh`** — (B) per-layer 20s WARN excludes Layer 7; (#4) `final_exit = worst(final_exit, rc)` via `PIPESTATUS[0]` so the operator lane reaches `LAYER_ALL_EXIT=2` (also repairs the latent exit-2→1 collapse).
- **`scripts/layer3.sh`** — wired `layer7.test.sh` as a `run_and_emit` self-test (no `*.test.sh` auto-runner).
- **`scripts/layer7.test.sh`** (new) — 20 hermetic assertions: cluster-availability skip, Phase-1 precondition (setup/rebuild/ports), orchestrator-integrity (operator lane survives `parse_status_line`/`aggregate_worst_status`/`status_to_exit_code`), Phase-2 OK/FAIL, and the **load-bearing retired-grep proof** (a test FAIL whose output contains "connection refused/reset/timed out/broken pipe" STILL → FAIL).
- **Docs** — runbook §6.7 rewritten (four lanes + two-phase) and all `wave2-pending` refs purged (§enum table, §gap section, §quick-ref); SKILL.md §Step-6 protocol trimmed to Lead-policy (always-run row + :419 kept per user ruling); ADR-0033 §4 Layer-7 envelope sentence; infrastructure INDEX.md pointer.

Loki note (ruling #7): the dev cluster **does** deploy Loki (`infra/kind/scripts/setup.sh` deploys Prometheus/Loki/Promtail/Grafana), so absent-Loki is not a concern; `ENV_TEST_LOKI_URL` is wired from `ports.json` and omitted only if empty.

---

## Files Modified

| File | Change |
|------|--------|
| `scripts/layer7.sh` | Stub → full layer (four lanes, two-phase classifier) |
| `scripts/layer7.test.sh` | **new** — 20-assertion hermetic self-test |
| `scripts/layer3.sh` | +1 line: wire `layer7.test.sh` self-test |
| `scripts/layer-all.sh` | per-layer WARN excl. L7; `final_exit` exit-2 propagation |
| `scripts/lang/_common.sh` | +2 enums (`SKIPPED-NO-CLUSTER` rank 2 / exit 0; `PRECONDITION_FAILURE` rank 6 / exit 2) — pending @test ack |
| `scripts/lang/_common.test.sh` | +10 additive assertions for the new enums |
| `docs/runbooks/devloop-validation.md` | §6.7 rewrite + enum table + purge `wave2-pending` |
| `.claude/skills/devloop/SKILL.md` | §Step-6 protocol trim → Lead policy |
| `docs/decisions/adr-0033-polyglot-validation-pipeline.md` | §4 Layer-7 envelope sentence |
| `docs/specialist-knowledge/infrastructure/INDEX.md` | Layer 7 pointer → layer7.sh + runbook |

---

## Devloop Verification Steps

**Self-tests (hermetic, every devloop via Layer 3):**
- `scripts/layer7.test.sh` — 41/41 pass (final): the four-way env gate (CI→SKIPPED-NO-CLUSTER; local+absent→PRECONDITION `local-helper-not-running`; local+dead→PRECONDITION `helper-unreachable`; present→run), Phase-1 preconditions (setup/rebuild/ports), Phase-2 OK/FAIL, and the retired-grep proof (infra-words-in-output still FAIL).
- `scripts/layer-all.test.sh` (@test-owned, wired into layer3) — 24/24: orchestrator integrity (PRECONDITION_FAILURE→LAYER_ALL_EXIT=2 + cell + total; FMV-not-collapsed; SKIPPED-NO-CLUSTER→exit0/TOTAL=OK; the STATUS=OK+exit1 rc FLOOR; budget excl.; LAYER_SCRIPT_DIR seam DEVLOOP_TEST-gating).
- `scripts/lang/_common.test.sh` — 49/49 pass (final; 35 pre-existing + new enum cases).
- `scripts/lang/_layer_skeleton.test.sh` — 36/36 pass.
- `bash -n` clean on all modified scripts; shellcheck deferred to Layer 3 artifact-check (not installed locally).
- **Close-out (2026-06-29):** all four suites re-run green by the Lead — `_common` 49, `layer7` 41, `layer-all` 24, `_layer_skeleton` 36 = **150 assertions, 0 failures**.

**Live cluster run (forced Gate-2, this devloop):**
- Run 1 (initial): exercised the full Phase-1 path against the real Kind cluster — `STEP=cluster-ready 1s`, `STEP=infra-change 539s` (infra/kind/ touched → teardown+setup), `STEP=rebuild 320s` (rebuild-all exit 0), then `PRECONDITION_FAILURE cluster-unhealthy` (exit 2). Surfaced the Phase-1e timing bug (see §Issues Encountered) — fixed with the readiness poll.
- Run 2 (suite, post-health-fix): `cargo test -p env-tests --features all` against the now-healthy cluster. **51, 7, 5, 5, 12, 3, 6, 9, 9, 6(+1 ignored) all pass** across the test files; ONE failure: `test_all_services_have_logs_in_loki` (`30_observability.rs:116`), `SUITE_RC=101`. Root cause = **cold-start flake**: the test hard-asserts `is_loki_available()` (Loki `/ready`==200) with no retry, and Loki wasn't warm the instant after bring-up (curling `/ready` directly → HTTP 200; re-running `--test 30_observability` with Loki warm → **2 passed, RC=0**). Pre-existing (my changes touch neither service code nor the env-tests crate — both out of scope); Layer 7 surfaced it correctly as `STATUS=FAIL` (loud, no masking — design working). Reported raw to @team-lead; user ruled **(a)** — implement the Phase-1f observability-readiness poll now (landed; see §Accepted Deferrals #2).

**Final close-out live run (2026-06-29, Lead, post-Phase-1f):** full `layer7.sh` end-to-end against a fresh cluster. Phase 1 brought the cluster up (`cluster-ready 430s`, `infra-change 610s`, `rebuild 306s`, `health-confirm 13s`), then the **Phase-1f Prometheus `/-/ready` poll timed out at 300s → `PRECONDITION_FAILURE observability-prometheus-not-ready` (exit 2, operator lane)** — the suite did not run. Diagnosis (cluster left up): the Prometheus pod was **Ready in 1 second** (`created 00:43:08Z, Ready 00:43:09Z`, 0 restarts, 36m healthy), Service/endpoints/NodePort all correct, no NetworkPolicy — so this was **not** pod init. It was a transient in the **host→kind-node→NodePort** path (`host.containers.internal:20100`) that self-healed within ~3 min of the budget expiring (most likely kube-proxy NodePort iptables churn during `rebuild-all`'s 8-pod rollout on a resource-starved single node). That is **correct operator-lane behavior** (it refused to run the suite against an unreachable Prometheus rather than emit a false test-FAIL) and an **infrastructure** issue, not a task-#56 defect. To get the suite signal decoupled from the transient, the Lead then ran `cargo test -p env-tests --features all` directly against the now-reachable cluster: **118 passed, 0 failed, 3 ignored** — including `30_observability` (Loki warmed 503→200 in time; the cold-Loki flake did NOT recur). **The env-tests are healthy; no 6-week rot.**

---

## Code Review Results

**Process note:** the panel completed **Gate-1 plan confirmation**; the formal **Gate-3 verdict round was interrupted** when the user stopped the agent team (the back-half of the loop had degraded into stale-read churn against the uncommitted tree). The **Lead then completed final verification directly** — full code review of `layer7.sh`/`layer-all.sh`/`_common.sh` (no blocking findings), the four hermetic suites (150 assertions green), and the live env-test run (118/0/3). Reviewer states below are their last-recorded positions, all on the current tree.

| Reviewer | State | Notes |
|----------|-------|-------|
| Security | Plan confirmed (Gate-1); **RESOLVED-FIXED** | Silent-skip/CI blocker resolved (four-way discriminator + `GITHUB_ACTIONS` CI-skip). Trust boundary preserved + extended (`assert_no_ci_sentinel_leak` rejects the `LAYER_SCRIPT_DIR` seam in CI; all 4 layer7 seams `DEVLOOP_TEST`-gated — caught a real test-isolation bug). `_common.sh:204` comment fixed + verified. `Approved-Cross-Boundary: security` trailer applied on the commit. |
| Operations | Plan confirmed (Gate-1, **CLEAR**) | Operator/implementer lanes unambiguous + survive the orchestrator (`LAYER_ALL_EXIT=2`); budget envelope honest (L7 excluded from both budgets); silent-skip structurally closed; status-grep fail-closed positive-match. `Approved-Cross-Boundary: operations` trailer applied on the ladder hunk. |
| Test | Plan confirmed; **additivity ACK granted** | Owns `_common.test.sh` (non-breaking, additions-only) + `layer-all.test.sh` (24/24 orchestrator integrity, incl. the rc-floor mismatch pin). Enforced DECISION-2 (`FMV>PRECONDITION`) against the landed code. Retired-grep regression guard mandatory + present. |
| Observability | Plan confirmed (Gate-1) | Q1/Q2 resolved better than asked (first-class `PRECONDITION_FAILURE` enum via the lifecycle, not a FAIL+exit-2 workaround). Scoped the Phase-1f probes (Prometheus HARD `/-/ready`, Loki SOFT, Grafana skipped); withdrew the suite-output classifier (would reopen reverse-masking). Flagged a 2nd no-retry assertion (`up{}`) → TODO. |
| Code Quality | Holding on B1 at interrupt → **resolved** | B1 (ADR-0033 §6 must enumerate the new enums) was Lead-authorized and landed (§6 + exit-code table amended). B2 (stale Cross-Boundary table) reconciled. B3 (backstop-scope posture) ACK'd by security. |
| DRY | Plan confirmed (**CLEAR**) | Helper reuse strong (no hand-rolled STATUS/base-ref); prose decomposition correct (protocol lives only in `layer7.sh`); Cross-Boundary table GSA-clean. |
| Semantic Guard | Accepted | Error-context preservation sound (no FAIL/PRECONDITION masked to N/A/OK); reverse-masking resolution (two-phase, grep retired) accepted; per-probe Phase-1f disposition (no blanket `|| true`) at its request. |

**Lead final verdict:** code correct and complete; env-tests proven green against the live cluster. The one live-run wrinkle (Phase-1f Prometheus operator-lane trip) was diagnosed to a host→NodePort networking transient after `rebuild-all` (the Prometheus pod was Ready in 1s) — correct operator-lane behavior, not a defect; recorded as a follow-up (see §Accepted Deferrals / §Issues).

---

## Accepted Deferrals

- `docs/TODO.md` §Env-Test Resilience — cold/absent-Loki retry for `test_all_services_have_logs_in_loki` (env-tests/observability owner)
- Phase-1e observability-readiness poll — RESOLVED in-loop via `layer7.sh` Phase-1f (not deferred); see §Issues


---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `c4c6138239fbe356c4695d3c0bf217d9768c4387`
2. Review all changes: `git diff c4c6138..HEAD`
3. Soft reset (preserves changes): `git reset --soft c4c6138`
4. Hard reset (clean revert): `git reset --hard c4c6138`

---

## Issues Encountered & Resolutions

**Phase-1e eager health-check (caught by the forced live run — the whole point of running it).** The first end-to-end run reached `STATUS=PRECONDITION_FAILURE REASON=cluster-unhealthy` (exit 2) — but this was a **false negative in my code, not a sick cluster**. `dev-cluster rebuild-all` issues `kubectl rollout restart` and returns *before* the new pods finish rolling (run log: `deployment.apps/mh-0 restarted` → `rebuild-all completed` → instant unhealthy). My Phase-1e did a **one-shot** `__cluster_ready` immediately after, so every real run would have spuriously tripped the operator lane. **The lane itself behaved correctly** (loud exit-2, banner, no masking) — exactly the design intent — it just fired on a timing race. **Fix:** Phase-1e now `__wait_cluster_ready` (bounded poll every 10s, `DEVLOOP_HEALTH_BUDGET=300s`). Confirmed the diagnosis: `dev-cluster status` shows pods healthy once the rollout settled. This is the class of bug only a live run can surface — hermetic tests can't model rollout timing.

---

## Lessons Learned

1. **A `wave2-pending` placeholder's consumers must keep calling it a placeholder until the body lands.** The root bug wasn't the stub — it was that SKILL.md described a 4-step protocol as if the stub ran it, and `N/A` ranked *above* `OK` so the pipeline stayed green with zero coverage. A placeholder that aggregates to "pass" + narrative that claims it works = a silent six-week skip.

2. **The forced live run earned its cost twice.** Hermetic tests (150 assertions) could not have caught either real issue: (a) the Phase-1e one-shot health check racing `rebuild-all`'s rollout, and (b) the host→NodePort networking transient. Cluster-dependent layers need a real run; "tests green" was necessary but not sufficient.

3. **"Pods healthy" ≠ "endpoints reachable."** The helper's readiness signal only inspects the `dark-tower` namespace, and even a Ready-in-1-second pod can be unreachable from the host while the NodePort/port-mapping path converges. Phase-1f (polling the actual HTTP `/ready` the suite uses) is the truer gate — and it correctly attributed the unreachable endpoint to the operator lane instead of a false test-FAIL.

4. **Diagnose, don't assume.** "Slow container" and "Prometheus init" were both wrong — the pod was Ready in 1s. Only looking at actual pod state/events/timestamps revealed it was a host-path transient. The two-phase + operator-lane design held up precisely because it doesn't guess.

5. **Multi-agent coordination needs an immutable substrate.** Most of the back-half churn was *stale-read drift*: reviewers diffing different mid-edit snapshots of an uncommitted, fast-moving tree and re-flagging already-fixed code, plus a value-neutral enum label that oscillated ~4× via crossed messages. The fixes (for a future SKILL.md amendment): checkpoint-commit before review + review against SHAs (`git show`), an append-only single-writer DECISIONS log, and deciding value-neutral questions instantly + hard-closing them. The collaboration itself was valuable (it caught the silent-skip hole, the reverse-masking risk, and the orchestrator exit-lane bug) — it was the substrate, not the scrutiny, that drifted.
