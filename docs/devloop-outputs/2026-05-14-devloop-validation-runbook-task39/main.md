# Devloop Output: docs/runbooks/devloop-validation.md (R-62, ADR-0033 Wave 3 #8)

**Date**: 2026-05-14
**Task**: Authoritative pipeline-failure runbook — layer-by-layer failure-mode → wrapper-script mapping; STATUS= enum reference; `_get_base_ref.sh` troubleshooting; cross-link from SKILL.md and each `layerN.sh` header.
**Specialist**: operations
**Mode**: Agent Teams v2 — full (7 teammates)
**Branch**: `feature/browser-client-join-task38`
**Duration**: TBD

User-story task: #39 in `docs/user-stories/2026-05-02-browser-client-join.md`. Depends on #38 (Completed, commit `8f1f399`). Closes ADR-0033 Wave 3 #8.

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `0130ce82b7aeed0b567cbdffcde0133d98b7aeaf` |
| Branch | `feature/browser-client-join-task38` |
| Team | `devloop-2026-05-14-devloop-validation-runbook-task39` |

---

## Loop State (Internal)

<!-- Maintained by the Lead for state recovery after interruption. -->

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@devloop-2026-05-14-devloop-validation-runbook-task39` |
| Implementing Specialist | `operations` |
| Iteration | 1 |
| Security | `security@devloop-2026-05-14-devloop-validation-runbook-task39` |
| Test | `test@devloop-2026-05-14-devloop-validation-runbook-task39` |
| Observability | `observability@devloop-2026-05-14-devloop-validation-runbook-task39` |
| Code Quality | `code-reviewer@devloop-2026-05-14-devloop-validation-runbook-task39` |
| DRY | `dry-reviewer@devloop-2026-05-14-devloop-validation-runbook-task39` |
| Operations | `operations@devloop-2026-05-14-devloop-validation-runbook-task39` (peer-reviewer; implementer is also operations-trained) |

---

## Task Overview

### Objective

Land `docs/runbooks/devloop-validation.md` as the authoritative runbook for pipeline failures, then cross-link it from the canonical entry points so operators land there from any natural starting point. Per the task spec in `docs/user-stories/2026-05-02-browser-client-join.md:530`:

1. **Layer-by-layer failure-mode → wrapper-script mapping**. For each of the seven layers in `scripts/layer-all.sh`, document the most common failure modes and point at the wrapper script (`scripts/lang/<lang>/<verb>.sh`) where the failure originated.
2. **Exit-code reference**. Document the 0/1/2 exit-code convention (0=PASS/SKIP, 1=FAIL, 2=PRECONDITION_FAILURE) and the `STATUS=` enum vocabulary per ADR-0033 §4 / §6. Wrapper contract: `STATUS=OK|FAIL|SKIPPED-NO-DIFF|SKIPPED-NO-VERB|N/A`. Worst-child STATUS aggregation rule.
3. **`_get_base_ref.sh` troubleshooting playbook**. The stderr `BASE_REF=… BASE_SOURCE=… DIFF_MODE=… FILES_CHANGED=…` line is the runbook anchor (per task #42 Tech Debt Pointer #4). `ERROR:` token (in-resolver) vs `PRECONDITION_FAILURE:` token (orchestrator) — two-token discipline.
4. **Per-language wrapper triage**. `STATUS=SKIPPED-NO-VERB` interpretation (verb-discovery skip for proto's missing `test.sh`/`audit.sh`); `_changed_helpers.sh` debugging; `_test_changed_predicates.sh` drift detection.
5. **Cross-link from SKILL.md Step 6** and **each `layerN.sh` header comment**. Cross-links land as one-liner pointers ("Pipeline failures: see `docs/runbooks/devloop-validation.md`"), not as content duplicates.

### Scope

- **Service(s)**: none (build/CI tooling + runbooks only)
- **Schema**: No
- **Cross-cutting**: Yes — runbook is the operator-facing index for the polyglot pipeline; touches operations + infrastructure surfaces.

### Debate Decision

NOT NEEDED. This is implementation of ADR-0033 Wave 3 #8 (already accepted 2026-05-07). The runbook content is descriptive (documents existing behavior in `scripts/layer-all.sh` + `scripts/lang/_get_base_ref.sh` + `scripts/lang/_dispatch.sh` etc.); no design decisions.

---

## Cross-Boundary Classification

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `docs/runbooks/devloop-validation.md` | Mine | — (new file under `docs/runbooks/**`, operations-owned tree) |
| `.claude/skills/devloop/SKILL.md` | Mine | — (operations owns the devloop skill per task #38 precedent — Step 6 cross-link is a single bullet pointer) |
| `scripts/layer-all.sh` (header comment add) | Mechanical | infrastructure (review-only; value-neutral one-liner pointer per task brief — `bash -n` covers syntax) |
| `scripts/layer1.sh` (header comment add) | Mechanical | infrastructure (review-only; one-liner pointer) |
| `scripts/layer2.sh` (header comment add) | Mechanical | infrastructure |
| `scripts/layer3.sh` (header comment add) | Mechanical | infrastructure |
| `scripts/layer4.sh` (header comment add) | Mechanical | infrastructure |
| `scripts/layer5.sh` (header comment add) | Mechanical | infrastructure |
| `scripts/layer6.sh` (header comment add) | Mechanical | infrastructure |
| `scripts/layer7.sh` (header comment add) | Mechanical | infrastructure |

**GSA check**: neither `scripts/**` nor `.claude/skills/**` is enumerated in `scripts/guards/simple/cross-boundary-ownership.yaml`, so the Layer B classification-sanity guard has no path-rule veto. Mechanical is defensible per the task-lead brief: each pipeline-script edit is a one-line "Pipeline failures: see `docs/runbooks/devloop-validation.md`" pointer, value-neutral, structure-preserving (additive comment line beneath the existing header block), and the guard pipeline's `bash -n`-style syntax cover catches accidental script breakage. SKILL.md Step 6 cross-link is Mine per task #38 precedent (operations owns the devloop skill).

---

## Planning

### Structure of `docs/runbooks/devloop-validation.md`

Sections, in order. Each section is purpose-specific — a 3am operator hits "what failed?" → "where does it emit from?" → "how do I fix it?" in three jumps.

1. **Header** — Title, scope (the polyglot validation pipeline `scripts/layer-all.sh` + Layers 1-7 + supporting helpers), entry-point summary (`./scripts/layer-all.sh` from repo root; per-layer `./scripts/layerN.sh` for targeted debug). Pointer to ADR-0033 (canonical spec).

2. **Quick triage** — One-paragraph "you ran `./scripts/layer-all.sh` and it failed; here's the 30-second decision tree" — read the final `LAYER_SUMMARY_BEGIN`/`END` block + `TOTAL_RESULT=` line; find the layer with `RESULT=FAIL`; jump to that layer's section below.

3. **Exit-code & STATUS enum reference** — `scripts/layer-all.sh` and per-layer/wrapper exit-code semantics:
   - **0** = PASS or SKIPPED (success-exit class — ADR-0033 §6) — `STATUS=OK|SKIPPED-NO-DIFF|SKIPPED-NO-VERB|N/A`.
   - **1** = FAIL — work ran and detected a problem — `STATUS=FAIL`.
   - **2** = PRECONDITION_FAILURE — wrapper/orchestrator bug, dispatcher misconfig, OR pre-layer guardrail tripped (e.g. shallow CI clone). Maps to `STATUS=UNKNOWN` from the aggregator's POV.
   - `STATUS=` enum vocabulary (ADR-0033 §6): `OK | FAIL | SKIPPED-NO-DIFF | SKIPPED-NO-VERB | N/A`. Pinpoint the precedence ladder from `_common.sh:__status_rank` (`FAIL > N/A > OK > SKIPPED-NO-DIFF > SKIPPED-NO-VERB`; `UNKNOWN > FAIL` — dispatcher-bug signal).
   - Worst-child aggregation: `aggregate_worst_status` runs across every collected child `STATUS=` line; the rule is "if any child did real work and passed, the layer passed; otherwise the SKIPPED-* state is informative." Worked example with three children (one OK, one SKIPPED-NO-DIFF, one FAIL → layer aggregates FAIL).
   - `LAYER=<n> START=<ts> END=<ts> DURATION=<s> RESULT=<enum> REASON=<reason>` stderr line (emitted on EXIT trap — guaranteed even under `set -e` abort).

4. **The two-token convention (`ERROR:` vs `PRECONDITION_FAILURE:`)** — Pulled into its own section because it is the most-grepped anchor:
   - **`ERROR:`** = in-resolver emissions from `scripts/lang/_get_base_ref.sh` (line 36 ref-name validation, line 79 merge-base computation, line 114 SHA resolution). Indicate the resolver could not produce a usable BASE_REF.
   - **`PRECONDITION_FAILURE:`** = pre-layer guardrail emissions from `scripts/layer-all.sh:40-43` (CI shallow-clone detection). Indicate a precondition for the layer pipeline is not met.
   - Future precondition checks added to `layer-all.sh` (disk-space, env-var presence, etc.) inherit the `PRECONDITION_FAILURE:` token (task #42 Tech Debt Pointer #4). This runbook is the canonical home for the convention.
   - Greppable: `grep -E '^(ERROR|PRECONDITION_FAILURE):' /tmp/devloop/layer-*.stderr.log` finds both classes in one pass.

5. **`_get_base_ref.sh` troubleshooting playbook** — The `BASE_REF=<sha> BASE_SOURCE=<src> DIFF_MODE=<mode> FILES_CHANGED=<n>` stderr line is the **runbook anchor** (ADR-0033 §7 normative requirement; per-layer cache file at `${DEVLOOP_TMP}/changed-files.layer-<n>`):
   - **Reading the line**: each token's meaning; valid `BASE_SOURCE` values (`local-mergebase | local-no-mergebase | ci-pr | ci-push-main | ci-push-first-commit`); `DIFF_MODE` is always `two-dot` post-task-#42.
   - **Common failure modes** (anchored at the canonical stderr line):
     - `FILES_CHANGED=0` but you expected diff → check `BASE_SOURCE` — if `local-no-mergebase`, your local clone has no `origin/main`; run `git fetch origin main`. If `ci-push-first-commit`, you are on the first commit of a branch (expected).
     - `BASE_REF=` missing entirely from a layer's stderr log → resolver did not run (layer wrapper bug — escalate).
     - `PRECONDITION_FAILURE: merge-base(<ref>, HEAD) unreachable` at `layer-all.sh` startup → CI shallow clone. Fix: `actions/checkout@v4` with `fetch-depth: 0` in `.github/workflows/ci.yml`. The guardrail is mode-dispatched (CI-PR checks `origin/$GITHUB_BASE_REF`, local checks `origin/main`, CI-push skips).
     - `ERROR: ref name contains unexpected characters` → env-injection attempt or malformed `$GITHUB_BASE_REF`; resolver exits 2.
     - `ERROR: could not compute merge-base` → CI clone shallow / ref unreachable / corrupt local pack.
     - `ERROR: could not resolve base ref to sha` → ref resolved but commit unreachable in the local pack.
   - **CI-PR scope shift (post-task-#42)**: `BASE_REF` is `merge-base(origin/$GITHUB_BASE_REF, HEAD)`, NOT base-branch tip. Security/observability dashboards that previously assumed base-tip semantics see a narrower scope; ADR-0033 §7 + task #42 §Security explain why this is correctness-preserving.
   - **Diagnosing predicate-vs-resolver disagreement**: the resolver writes `${DEVLOOP_TMP}/changed-files.layer-<n>` cache; per-layer `lang/<X>/changed.sh` predicates read it via `_changed_helpers.sh`. To inspect what a layer actually saw: `cat ${DEVLOOP_TMP:-/tmp/devloop}/changed-files.layer-<n>`.

6. **Layer-by-layer failure-mode → wrapper-script mapping** — Seven subsections, one per layer. Each subsection:
   - **What it runs** (verb + dispatcher path).
   - **Always-run vs skip-if-untouched** (ADR-0033 §3 matrix row).
   - **Common failure modes**: failure → emitting wrapper script → REASON token → canonical fix.
   - **Sub-sections**:
     - **Layer 1 (Compile)**: stage-1 = proto-only via `scripts/build.sh` with `DEVLOOP_DISPATCH_INCLUDE_LANGS=proto` → `lang/proto/compile.sh` (`buf-build-failed`, `buf-binary-missing`). Stage-2 = rust+ts via `DEVLOOP_DISPATCH_EXCLUDE_LANGS=proto` → `lang/rust/compile.sh` (`cargo-check-failed`), `lang/ts/compile.sh` (`nx-typecheck-failed`). Failure ordering: proto first so wire failures surface ahead of Rust/TS type-error cascades. Worked example: proto FAIL → rust SKIPPED-NO-DIFF → ts SKIPPED-NO-DIFF → layer result FAIL.
     - **Layer 2 (Format)**: `scripts/fmt.sh` → `lang/{rust,ts,proto}/fmt.sh`. Failure tokens: `cargo-fmt-failed` (run `cargo fmt --all` locally), `nx-format-failed`, `buf-format-failed`/`buf-binary-missing`.
     - **Layer 3 (Guards, always-run)**: `scripts/layer3.sh` → `scripts/guards/run-guards.sh` (REASON `guards-failed`) AND `scripts/lang/_test_changed_predicates.sh` (REASON `predicate-meta-test-failed`). Both invoked via `run_and_emit`. Guards self-classify per-file via path globs; failure surfaces a specific guard name in the log. Predicate meta-test failure → drift between a `lang/<X>/changed.sh` predicate and its fixture row; output prints expected/actual rc + pointer to the relevant `changed.sh`.
     - **Layer 4 (Test)**: `scripts/test.sh` → `lang/rust/test.sh` (cargo test, brings up postgres via podman/docker — REASON `cargo-test-failed`; also fails on missing container runtime); `lang/ts/test.sh` (nx affected -t test:unit test:component — REASON `nx-test-failed`). Proto has no `test.sh` → `STATUS=SKIPPED-NO-VERB REASON=proto-test-sh-missing-or-not-executable` (expected).
     - **Layer 5 (Lint)**: `scripts/lint.sh` → `lang/rust/lint.sh` (cargo clippy -D warnings — `cargo-clippy-failed`), `lang/ts/lint.sh` (`nx-lint-failed`), `lang/proto/lint.sh` (`buf-lint-failed`).
     - **Layer 6 (Audit, always-run)**: `scripts/audit.sh` → `lang/rust/audit.sh` (cargo audit — `cargo-audit-failed`), `lang/ts/audit.sh` (pnpm audit --audit-level=high — `pnpm-audit-failed`), AND `lang/proto/breaking.sh` invoked unconditionally separately (`buf-breaking-failed`, `buf-binary-missing`, `base-ref-unresolved`). NOTE the dispatcher returns worst-of `(dispatch_rc, breaking_rc)`; this preserves both gates. Audit-config edits are security-owned (ADR-0033 §11).
     - **Layer 7 (Env-tests)**: `scripts/layer7.sh` currently emits `STATUS=N/A REASON=wave2-pending` until env-tests wiring lands. After that lands, Layer 7 covers dev-cluster + Rust env-tests + Playwright `@smoke`. Cross-link to `ADR-0030` for the cluster-helper contract.

7. **Per-language wrapper triage** — Cross-cutting how-to:
   - **`STATUS=SKIPPED-NO-VERB`**: dispatcher tried to invoke `lang/<X>/<verb>.sh` and the file is missing-or-not-executable. Two valid reasons: (a) intentional gap (proto has no `test.sh` or `audit.sh`); (b) recently-deleted or chmod-stripped wrapper. Verify against ADR-0033 §1 directory listing. If gap is intentional, the SKIPPED-NO-VERB line is informative, not a failure. The aggregator ranks it below `OK` so a co-running OK lang dominates.
   - **`STATUS=SKIPPED-NO-DIFF`**: `lang/<X>/changed.sh` returned 1 (exit code 1 = untouched). Read the `BASE_REF=` line — was diff what you expected? Then inspect `${DEVLOOP_TMP}/changed-files.layer-<n>` to confirm; finally re-run the lang predicate manually: `DEVLOOP_TMP=/tmp/devloop DEVLOOP_LAYER=<n> bash scripts/lang/<X>/changed.sh; echo $?`.
   - **`STATUS=N/A`**: documented gap (e.g. `layer7.sh` `wave2-pending`); aggregates above `OK` to signal "this verb isn't wired" — distinct from "ran cleanly". Unexpected N/A outside the documented placeholder = wrapper bug — escalate.
   - **`_changed_helpers.sh` debugging**: predicates use `diff_touches_path` (awk + fixed-string `index($0,p)==1`) and `diff_touches_root_files` (grep `-qxF`). If a predicate misfires, the helper file is the first stop; pair with `_changed_helpers.sh` + the lang's `changed.sh` to read the exact predicate. The cache file is shared per-layer; if a lang reads the wrong cache, `DEVLOOP_LAYER` is not exported (a layer-script bug).
   - **`_test_changed_predicates.sh` drift detection**: runs every devloop in Layer 3 alongside the simple guards. Hermetic — synthesizes a cache under `mktemp`, invokes each lang's `changed.sh` against fixture paths under `env -i`. Failure prints the failing row + expected vs actual + a pointer to the offending `lang/<X>/changed.sh`. Fix by either correcting the predicate or amending the fixture (with rationale).

8. **Symptom → resolution catalogue (cross-reference index)** — At-a-glance table:
   - "Layer 1 fails on a docs-only PR" → likely `lang/<X>/changed.sh` over-classification (intentional `crates/foo/README.md`-as-rust per ADR-0033 §3 trade-off); cargo check is cheap.
   - "Layer 3 fails with `predicate-meta-test-failed`" → predicate drift, jump to §7 `_test_changed_predicates.sh`.
   - "Layer 6 fails on every CI run, local clean" → CI base-ref shift (post-#42); check `BASE_SOURCE=ci-pr` `BASE_REF=` is merge-base, not main tip.
   - "Layer N missing `BASE_REF=` line entirely" → wrapper bug; the resolver did not run; escalate.
   - "`PRECONDITION_FAILURE:` at startup" → CI fetch-depth issue (jump to §4).
   - "Every pipeline emits dozens of `BASE_REF=` lines" → known cost concern (task #42 Tech Debt Pointer #2); cache+sentinel mitigation tracked but not yet implemented.

9. **Escalation & related references** — When to page operations + infrastructure; pointers to ADR-0033 (canonical spec), ADR-0030 (Layer 7 contract), `.claude/skills/devloop/SKILL.md` (Step 6 entry), prior task outputs (`docs/devloop-outputs/2026-05-12-skill-step6-rewrite-task38/main.md`, `docs/devloop-outputs/2026-05-13-base-ref-unification-task42/main.md`), `docs/runbooks/TEMPLATE.md` (parent template for runbooks).

10. **Changelog** — Initial creation 2026-05-14 (this task).

### Cross-links

- **`.claude/skills/devloop/SKILL.md` Step 6** (line 374): append a one-bullet pointer immediately after the existing "Each `scripts/layerN.sh` is independently callable…" sentence. Wording: `**Pipeline failures**: see ` + backtick path + ` for layer-by-layer failure-mode mapping, exit-code / STATUS enum reference, _get_base_ref.sh troubleshooting (the BASE_REF= stderr line is the anchor), and per-language wrapper triage.` Placed inside Step 6 so it lands where an operator naturally lands after `./scripts/layer-all.sh` fails.

- **`scripts/layerN.sh` header comments** (each of 1-7): one-line addition under the existing layer-description comment block. Wording: `# Failure triage: docs/runbooks/devloop-validation.md (this layer's section + §4 two-token convention).` Each script's existing `# Layer N — …` comment stays first; the pointer is the second comment line. `bash -n` syntax checks (already part of the guards pipeline's coverage of script files) catch any accidental breakage. Inline pointer is intentionally short — full content lives in the runbook.

- **`scripts/layer-all.sh` header**: same one-line pointer as the per-layer headers, placed below the existing `# Greppable warn tokens` block. Wording: `# Failure triage: docs/runbooks/devloop-validation.md (all layers + §4 two-token convention).`

### Non-deliverables (deliberate)

- No new metrics emitted (operator-facing doc).
- No code changes to pipeline scripts beyond the one-line header pointer (no behavior change; reviewers can confirm via `git diff --stat scripts/layer*.sh`).
- No update to `ADR-0033` — the runbook is the operational view; the ADR is the design view; cross-link from runbook → ADR is sufficient.
- No update to `docs/decisions/adr-0030-*.md` or other ADRs.
- No update to `cross-boundary-ownership.yaml` — runbook content does not introduce new GSA paths.

### Why this carve-up

- Section 4 (two-token convention) is broken out so a grep-driven operator who lands on `PRECONDITION_FAILURE:` or `ERROR:` finds a single section, not a buried paragraph. Task #42 Tech Debt Pointer #4 asked for canonicalization here.
- Section 5 (`_get_base_ref.sh`) is the runbook anchor per ADR-0033 §7 + task #42 — the `BASE_REF=` line is the single line every layer log carries. Section 5 covers reading it, common malfunctions, and predicate-cache inspection.
- Section 6 (layer-by-layer) is the bulk; one subsection per `scripts/layerN.sh`. Each subsection terminates at the wrapper-script path so the operator can `cat scripts/lang/<X>/<verb>.sh` and see the failure source in <30s.
- Section 7 (per-language wrapper triage) is cross-cutting — covers what's the same across layers (changed.sh predicate, dispatcher SKIPPED-* semantics, helper debugging).
- Section 8 (symptom catalogue) is short on purpose — it's the at-a-glance jump-table, not the long-form. The jump destination is §4-7.

### Estimated size

- Runbook: ~400-500 lines (single file). Smaller than `gc-deployment.md` (~600 lines), comparable to `ac-service-deployment.md`.
- SKILL.md: +1 line (pointer).
- 7× `scripts/layerN.sh` + 1× `scripts/layer-all.sh`: +1 comment line each = +8 lines total across pipeline scripts.

### Verification at Gate 2

- Most language wrappers report `STATUS=SKIPPED-NO-DIFF` (docs-only change).
- Layer 3 (always-run): guards pass; predicate meta-test passes.
- Layer 6 (always-run, mostly env-blocked): `STATUS=FAIL REASON=buf-binary-missing` if buf is absent locally — pre-existing env gap, not caused by this task.
- Pipeline scripts: `bash -n scripts/layer*.sh` should pass — covered by Layer 3 guards.

---

## Pre-Work

None.

---

## Implementation Summary

Single docs-and-pointers change, ten files touched. No behavior change in the pipeline.

1. **`docs/runbooks/devloop-validation.md`** (NET-NEW, 495 lines): the authoritative pipeline-failure runbook. Ten numbered sections plus header + scope blurb:
   - §1 Quick triage (30-second decision tree)
   - §2 Pipeline entry points
   - §3 Exit-code & STATUS enum reference (0/1/2 mapping + `STATUS=OK|FAIL|SKIPPED-NO-DIFF|SKIPPED-NO-VERB|N/A` table + worst-child precedence ladder `FAIL > N/A > OK > SKIPPED-NO-DIFF > SKIPPED-NO-VERB`, `UNKNOWN > FAIL` + worked-example multi-lang aggregation + `LAYER=…` stderr line + WARN BUDGET_* tokens)
   - §4 Two-token convention `ERROR:` vs `PRECONDITION_FAILURE:` (line-anchored at `_get_base_ref.sh:36/79/114` and `layer-all.sh:40-43`; one-grep extension rule)
   - §5 `_get_base_ref.sh` troubleshooting playbook (anchored on the `BASE_REF=<sha> BASE_SOURCE=<src> DIFF_MODE=<mode> FILES_CHANGED=<n>` stderr line; per-token meaning, common failure modes, CI-PR scope shift post-#42, predicate-cache inspection, known cost concern per task #42 TDP #2)
   - §6 Layer-by-layer failure modes (7 subsections, each terminating at the wrapper-script path + REASON token + canonical fix; Layer 1 worked example for stage-1/stage-2; Layer 6 twin-rc worked example per @operations ask; Layer 6 audit-config ownership guardrail per @security ask; Layer 1 `nx: command not found` per @operations ask; Layer 6 `cargo-audit-failed` triage flow per @operations ask)
   - §7 Per-language wrapper triage (SKIPPED-NO-VERB interpretation, SKIPPED-NO-DIFF triage, predicate exit-code reversed-from-typical-shell callout per @test ask, `_changed_helpers.sh` debugging, `_test_changed_predicates.sh` drift detection)
   - §8 Symptom → resolution catalogue (16-row grep-driven jump table including the three @operations-asked rows)
   - §9 Escalation & related references
   - §10 Changelog
   - Uses "orchestrator" for `scripts/audit.sh` and "dispatcher" for `_dispatch.sh` per @test terminology nit.

2. **`.claude/skills/devloop/SKILL.md`** (+2 lines, Step 6 at line 376): one-bullet pointer after the existing "Each `scripts/layerN.sh` is independently callable…" sentence:
   > **Pipeline failures**: see `docs/runbooks/devloop-validation.md` for layer-by-layer failure-mode mapping, exit-code / `STATUS=` enum reference, `_get_base_ref.sh` troubleshooting (the `BASE_REF=…` stderr line is the anchor), and per-language wrapper triage.

3. **`scripts/layer-all.sh`** (+2 lines, header comment): `# Failure triage: docs/runbooks/devloop-validation.md (all layers + §4 two-token convention).` placed below the existing `# Greppable warn tokens` block.

4. **`scripts/layer{1..7}.sh`** (+2 lines each, header comment): same shape as `layer-all.sh`, pointing at the per-layer subsection in the runbook (Layer 1 → §6.1, …, Layer 7 → §6.7). Each pointer also references §4 (two-token convention) so an operator who hits a `PRECONDITION_FAILURE:` from a specific layer's invocation lands at the convention via the per-layer pointer too.

### Verification at commit time

- `bash -n` syntax-check on all 8 pipeline scripts (`scripts/layer-all.sh` + `scripts/layer{1..7}.sh`) — all 8 OK.
- No behavior change: every script edit is a comment-only additive line beneath the existing header block. `git diff --stat scripts/layer*.sh` shows +2 lines / 0 deletions on each.
- Cross-link symmetry: SKILL.md Step 6 points at the runbook; the runbook §9 points back at SKILL.md; each layer-script header points at the runbook §6.<N>; the runbook §6 sections name the wrapper-script path.

---

## Files Modified

| File | Change | Net lines |
|------|--------|-----------|
| `docs/runbooks/devloop-validation.md` | NET-NEW — ten-section pipeline-failure runbook | +495 |
| `.claude/skills/devloop/SKILL.md` | Step 6 one-bullet pointer to the runbook (after the "independently callable" sentence) | +2 |
| `scripts/layer-all.sh` | Header comment one-line pointer below `# Greppable warn tokens` block | +2 |
| `scripts/layer1.sh` | Header comment one-line pointer → §6.1 | +2 |
| `scripts/layer2.sh` | Header comment one-line pointer → §6.2 | +2 |
| `scripts/layer3.sh` | Header comment one-line pointer → §6.3 | +2 |
| `scripts/layer4.sh` | Header comment one-line pointer → §6.4 | +2 |
| `scripts/layer5.sh` | Header comment one-line pointer → §6.5 | +2 |
| `scripts/layer6.sh` | Header comment one-line pointer → §6.6 | +2 |
| `scripts/layer7.sh` | Header comment one-line pointer → §6.7 | +2 |

---

## Devloop Verification Steps

TBD — Gate 2 runs `./scripts/layer-all.sh`. This is a docs-only task; most language wrappers will report `STATUS=SKIPPED-NO-DIFF`. Key checks: Layer 3 guards (cross-boundary scope + classification-sanity), any runbook/docs guards.

---

## Code Review Results

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | CLEAR | 1 | 1 | 0 | One line-cite nit (`_common.sh:201`→`:200`); fixed in-flight. ADR-0033 §11 audit-config ownership guardrail sentence folded into §6.6 at plan-revision time. |
| Test | CLEAR (was RESOLVED-WITH-FINDING iter-1) | 1 blocker + 5 nits | 1 | 5 | Required fix: stale `(lands in task #39)` parenthetical at `scripts/layer-all.sh:44` — landed iter-2. Two new line-cite nits in §4 lines 142/148 (drift to 42-44; `(this file)` cosmetic mismatch) accepted as deferred into the docs/TODO Code Quality sweep entry. |
| Observability | RESOLVED | 1 | 0 | 1 | Same `scripts/layer-all.sh:44` stale parenthetical (non-blocking nit; the blocking finding was @test's). BASE_REF= anchor + two-token convention + emission-multiplication caveat all verified against source. |
| Code Quality | RESOLVED | 3 | 0 | 3 | F1 line-number drift in §3 + §7 (off-by-1 cites); F2 §6.6 "blocks `--ignore=…`" phrasing precision (block-by-omission vs active reject); F3 §3 STATUS=UNKNOWN wording. All three deferred-accepted; consolidated into the docs/TODO §Code Quality sweep entry implementer filed iter-2. ADR-0033 §3/§4/§6/§7/§13 compliance verified. Ownership Lens: all 8 layer-script header edits stayed strictly one-line; Mechanical holds. |
| DRY | CLEAR | 0 | 0 | 0 | No true duplication; §4↔§5 ERROR-token boundary (plan-flagged risk) verified resolved (§5 backrefs §4). Cross-link consistency uniform across 9 surfaces. |
| Operations (peer) | RESOLVED | 1 | 0 | 1 | Same `scripts/layer-all.sh:44` stale parenthetical (non-blocking; blocking was @test's). Four peer-review additions folded (§6.1 nx row, §6.6 audit-config ownership, §6.6 twin-rc worked example, §8 catalogue rows). Operator-perspective triage quality + cross-link placement + convention-consistency all verified. |

### Iteration counts

- Planning iterations: 1 (no revision rounds; 6 plan confirmations on first circulation)
- Implementation iterations: 2 (iter-1 implementation; iter-2 layer-all.sh:44 fix + TODO entry)
- Gate-2 attempts: 1 (pre-existing env-gap pattern matching task #42 §Verification step 3; Layer 3 OK + Rust `cargo-test-passed`; reviewers concurred on env-gap interpretation, no escalation)

---

## Tech Debt Pointers

1. **`docs/runbooks/devloop-validation.md` §9 escalation table is incomplete.** Specific contact channels (oncall rotation, Slack channels, paging targets) are not yet wired — the runbook names the *target specialist* (operations / infrastructure / security / protocol) but not the *how*. Per `docs/runbooks/TEMPLATE.md` §Escalation, runbooks should name `#incidents`, PagerDuty group, or oncall rotation. Deferred to a follow-up once the team has standardized those channels for non-service runbooks (existing service runbooks under `docs/runbooks/*-incident-response.md` carry placeholders too). Not blocking this devloop.

2. **`.cargo/audit.toml` does not yet exist** at the path the runbook §6.6 names as the documented location for `[advisories.ignore]` entries. The file is the security-owned location *when one is added*; security MUST land the file (with a tracked allowlist entry) ahead of any operator-facing ignore decision. Runbook reads correctly today as "this is the documented location *if* one is added"; concrete file-presence is security's call, deferred to a follow-up task.

3. **Layer 7 sub-section §6.7 will need expansion** when the env-tests body lands (`scripts/layer7.sh` currently emits `STATUS=N/A REASON=wave2-pending`). At that time the runbook §6.7 needs: dev-cluster failure modes (cross-link to ADR-0030), Rust env-tests REASON tokens, Playwright `@smoke` failure modes + cluster-setup-cost edge case. Not blocking now since the layer body is a stub; the §6.7 entry today documents the placeholder behavior.

4. **`BASE_REF=` multiplication concern (carried from task #42 TDP #2)** is referenced from §5 "Known cost concern" as informational. If/when observability dashboards begin anchoring on `BASE_REF=` emission count, the cache + `__emit_base_ref_line` suppression-sentinel mitigation lands as a separate task in `scripts/lang/_get_base_ref.sh` + `_common.sh`. This runbook does not implement the mitigation; it only documents the cost.

5. **Symptom catalogue §8 will grow over time** as new failure modes are observed in the wild. The structure (2-column "Symptom → Jump" table) is appendable. Operators who hit a novel failure during devloop validation should append a row + update the corresponding §6 layer subsection in the same PR; the runbook's §10 changelog tracks expansions.

---

## Rollback Procedure

1. Start commit: `0130ce82b7aeed0b567cbdffcde0133d98b7aeaf`
2. `git diff 0130ce82b7aeed0b567cbdffcde0133d98b7aeaf..HEAD`
3. Soft reset: `git reset --soft 0130ce82b7aeed0b567cbdffcde0133d98b7aeaf`
4. Hard reset: `git reset --hard 0130ce82b7aeed0b567cbdffcde0133d98b7aeaf`

Docs-only — `git reset` is sufficient.

---

## Issues Encountered & Resolutions

TBD.

---

## Lessons Learned

TBD.
