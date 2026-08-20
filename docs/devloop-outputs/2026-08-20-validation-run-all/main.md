# Devloop Output: Run every validation layer on every task

**Date**: 2026-08-20
**Task**: Remove change-detection skip machinery so every validation layer runs on every task; drop the now-unconsumed `env_tests` manifest tag.
**Specialist**: infrastructure (paired-with: test)
**Mode**: Agent Teams (v2) — full
**Branch**: `feature/devloop-validation-run-all-opus-48`
**Duration**: ~2h (setup → commit; incl. Gate-1 D1 re-adjudication and two live layer-all runs)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `0216eab9c0a47b7d69b1e551001790328d131d32` |
| Branch | `feature/devloop-validation-run-all-opus-48` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` — Gate 2 PASSED (layer-all.sh exit 0; L7 env-tests-passed + browser-e2e-passed). Gate 3 PASSED unanimous (8/8, zero deferrals): Protocol CLEAR, Semantic Guard CLEAR, Test CLEAR, Security/Code-Quality/Observability/Operations/DRY RESOLVED-FIXED. Post-review re-verify: layers 1-3 OK on doc-only delta (4-7 unchanged code/runtime hold from Gate 2). |
| Implementer | `implementer` (infrastructure) |
| Implementing Specialist | `infrastructure` |
| Iteration | `1` |
| Security | `security` |
| Test (paired) | `paired-test` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Semantic Guard | `semantic-guard` |
| Protocol (conditional domain reviewer — dt-story schema) | `protocol` |

### Gate-1 Lead Rulings (2026-08-20)

- **D1 (changed.sh + lint-at-startup): REMOVE (Option B) — FINAL RULING (supersedes the
  earlier KEEP).** The KEEP ruling was explicitly contingent on no reviewer flagging it as a
  Gate-3 dead-code/DRY finding; that contingency fired pre-implementation. @code-reviewer
  verified (and it answers the Lead's crux): the per-lang footprint declaration has **zero
  live consumer** — `diff_touches_path "crates/"|"packages/"|"proto/"` appears nowhere except
  inside the three `changed.sh` wrappers; the surviving `diff_touches_*` callers (audit gate,
  layer7) use different inputs. Decisive: the `changed.sh` footprint predicate is structurally
  **identical to `env_tests`** — a consumerless encoding of the retired skip policy — so
  deleting one while keeping the other is incoherent; the task's own logic (retire the dead
  skip machinery) demands removing both. "Keep for smaller diff" is the scope-conservatism the
  review protocol names as an anti-pattern for same-owner same-mechanism siblings.
  **Terminal state (B):** delete `lang/{rust,ts,proto}/changed.sh` ×3 + `changed.test.sh` ×3 +
  `_test_changed_predicates.sh` + the lint-at-startup block (`_dispatch.sh:113-121`) +
  `test_missing_changed_sh` + `_test_helpers.sh:run_with_cache` (now orphaned) +
  `_get_base_ref.behavior-equivalence.test.sh:69-72` vestigial force-touch. KEEP
  `_changed_helpers.sh` (live `diff_touches_*`). Amend ADR-0033 §2/§3 to "per-lang changed.sh
  classification retired; footprint detection for surviving consumers lives in
  `_changed_helpers.sh` diff_touches_*." The R8 DRY framing-fix is MOOT under B (files deleted,
  not reworded). Condition: grep-verify NO remaining `changed.sh` references anywhere (CI
  workflows, Makefiles, docs) before deleting.
- **D2 (protocol pairing for `crates/dt-story/**` schema removal): YES** — added @protocol
  as a Gate-1 + Gate-3 domain reviewer (reviewer scope; infrastructure still owns/implements),
  mirroring the 2026-08-17 `branch`-removal precedent. Commit carries
  `Approved-Cross-Boundary: protocol …` trailer. Tier: Minor-judgment (owner-confirmed
  forced field narrowing).

---

## Task Overview

### Objective
Make validation-gate coverage independent of change-detection. Three parts:

1. **Language short-circuit goes.** Every language runs at every layer regardless of
   whether its files were touched. The dispatcher (`scripts/lang/_dispatch.sh`) already
   has an always-run mode (`DEVLOOP_DISPATCH_ALWAYS_RUN`) used by the audit path — this is
   a policy change, not new machinery. The per-language `changed.sh` short-circuit in the
   dispatcher is removed.
2. **Browser E2E runs whenever layer 7 runs.** The `__browser_e2e_triggered` path-matching
   in `scripts/layer7.sh` goes; the browser suite is no longer diff-triggered.
3. **run-story runs layer 7 for every task.** Not only tasks tagged `env_tests`. The
   `env_tests` tag then has no consumer and is removed from the dt-story manifest schema
   (`crates/dt-story`), the template, the user-story SKILL, and every story file / fixture
   still carrying it — struct + files in one commit (serde `deny_unknown_fields`).

### Scope
- **Service(s)**: none (validation pipeline + story runner + dt-story CLI)
- **Schema**: dt-story manifest schema (YAML), not DB. No migration.
- **Cross-cutting**: Yes — pipeline policy, story runner, two ADRs.

### Explicitly OUT of scope
- The layer-7 conditional that rebuilds the Kind cluster when `infra/kind/` changes
  (`scripts/layer7.sh` Phase-1b) **stays exactly as it is**. This is the one skip decision
  deliberately kept.

### Known and accepted (do NOT try to fix here)
- On a branch touching `infra/kind`, always-running layer 7 means the cluster rebuild fires
  on every task rather than once (that decision reads the whole-branch diff). Accepted.

### Debate Decision
NOT NEEDED — implementation of an already-decided policy; ADR-0033 §3 and ADR-0035 §3 are
amended (not re-debated) to match.

---

## Cross-Boundary Classification

Per ADR-0024 §6.2. I am **infrastructure** and own the validation pipeline
(`scripts/**`), the story runner (`scripts/workflow/**`), and the `dt-story`
crate surface — so most rows are **Mine**. `crates/dt-story/**` matches §6.4's
**schema-evolution** criterion (this removes a required manifest field), the
same criterion that put `--paired-with=protocol` on the 2026-08-17 `branch`
removal. **@team-lead ruled D2 = protocol pairing YES** — @protocol is a
Gate-1 + Gate-3 reviewer on the schema rows (already Gate-1-confirmed the
11-file footprint), infrastructure still owns + implements; commit carries an
`Approved-Cross-Boundary: protocol …` trailer. `@paired-test` is my
co-implementer on the four test surfaces
(`_dispatch.test.sh`, `layer7.test.sh`, `run-story.test.sh`, `dt-story/tests/cli.rs`).

**Two rows are not mine**, mirroring the 2026-08-17 split:
- `scripts/lang/ts/audit-remediation.md` — TS-lane template; content-neutral one-line prose edit → **client**.

Historical/append-only records are **NOT edited** (they are dated measurements,
not live schema/policy): `docs/devloop-outputs/*` (except this file),
`docs/debates/*`, `docs/TODO.md` entries, ADR-0035 §4's measured tally (line
~100), and the closed story-runner-hardening narrative prose (env_tests-as-it-was).

| Path | Classification | Owner |
|------|----------------|-------|
| `.claude/skills/devloop/SKILL.md` | Mine (doc) — always-run matrix + Layer-7 description | infrastructure |
| `.claude/skills/user-story/SKILL.md` | Mine (doc) — Part 3: drop --env-tests from add-task template + mention | infrastructure |
| `.github/workflows/ci.yml` | Mine (Mechanical) — Part 1: rewrite the guard-binary-step comment (skip-gated → always-run/belt-and-braces; @code-reviewer finding) | infrastructure |
| `crates/dt-story/src/engine.rs` | Mine (Minor-judgment, §6.4 schema, protocol co-sign) — Part 3: drop RunnableTask/NewTask.env_tests + validate + doc | infrastructure, protocol co-sign |
| `crates/dt-story/src/main.rs` | Mine (Minor-judgment, §6.4 schema, protocol co-sign) — Part 3: drop --env-tests flag + plumbing | infrastructure, protocol co-sign |
| `crates/dt-story/src/manifest.rs` | Mine (Minor-judgment, §6.4 schema, protocol co-sign) — Part 3: drop Task.env_tests + doc | infrastructure, protocol co-sign |
| `crates/dt-story/tests/cli.rs` | Mine (Domain-judgment) — Part 3: fixtures + assertions + new rejection test | infrastructure, reviewed by test |
| `crates/dt-story/tests/fixtures/blocked.md` | Mine (Minor-judgment) — Part 3: drop env_tests lines | infrastructure |
| `crates/dt-story/tests/fixtures/runnable.md` | Mine (Minor-judgment) — Part 3: drop env_tests lines | infrastructure |
| `docs/TODO.md` | Mine (doc) — resolve changed.test.sh-dup TODO; SIGPIPE consumer shift; skew closure; inventory counts | infrastructure |
| `docs/decisions/adr-0033-polyglot-validation-pipeline.md` | Mine (doc) — §2/§3 changed.sh RETIRED amendments + sweep; §4 budget | infrastructure |
| `docs/decisions/adr-0035-story-runner.md` | Mine (doc) — §3 gate policy, rationale, schema list | infrastructure |
| `docs/runbooks/devloop-validation.md` | Mine (doc) — browser + predicate-diagnosis retired; budget rename; dt-guard skew closure | infrastructure |
| `docs/specialist-knowledge/test/INDEX.md` | Not mine (Mechanical) — dangling `__browser_e2e_triggered` pointer repointed to always-run; done by @paired-test (test-owned) | test (via paired-test) |
| `docs/user-stories/2026-05-02-browser-client-join.md` | Mine (doc) — Part 3: env_tests manifest lines (leave closed-story narrative) | infrastructure |
| `docs/user-stories/2026-08-11-story-runner-hardening.md` | Mine (doc) — Part 3: env_tests manifest lines (leave closed-story narrative) | infrastructure |
| `docs/user-stories/_template.md` | Mine (doc) — Part 3: manifest block + prose list | infrastructure |
| `packages/web-app/e2e/README.md` | Not mine (Mechanical, review-only per ADR-0024 §6.3) — Part 2: browser-e2e-no-diff straggler to 'always runs' | client |
| `scripts/audit-suppressions-check.test.sh` | Mine (Mechanical) — Part 1: comment straggler naming changed.test.sh | infrastructure |
| `scripts/audit.sh` | Mine (Mechanical) — Part 1: drop no-op DEVLOOP_DISPATCH_ALWAYS_RUN=1 + comment | infrastructure |
| `scripts/build.sh` | Mine (Mechanical) — Part 1: dispatcher header "with skip-if-untouched" → always-run (@observability finding) | infrastructure |
| `scripts/fmt.sh` | Mine (Mechanical) — Part 1: dispatcher header "with skip-if-untouched" → always-run (@observability finding) | infrastructure |
| `scripts/lint.sh` | Mine (Mechanical) — Part 1: dispatcher header "with skip-if-untouched" → always-run (@observability finding) | infrastructure |
| `scripts/lang/_audit_gate.sh` | Mine (Mechanical) — Part 1: comment rewrite; fail-OPEN insight to ADR-0033 §3 | infrastructure |
| `scripts/lang/_changed_helpers.sh` | Mine (Mechanical) — Part 1: KEPT (live diff_touches_*); header prose reworded to name surviving consumers, not the deleted changed.sh classifiers | infrastructure |
| `scripts/lang/_common.sh` | Mine (Mechanical) — Part 1: comment straggler naming changed.sh predicates | infrastructure |
| `scripts/lang/_dispatch.sh` | Mine (Domain-judgment) — Part 1: remove short-circuit + knob + lint-at-startup; header drops changed.sh; KEEP aggregate arm (all-langs-skipped) | infrastructure |
| `scripts/lang/_dispatch.test.sh` | Mine (Domain-judgment) — Part 1: rework masking, sentinel to verb, delete test_missing_changed_sh, strip changed.sh fixtures | infrastructure, co-impl paired-test |
| `scripts/lang/_layer_skeleton.test.sh` | Mine (Mechanical) — Part 1: remove required-invocation check for deleted meta-test | infrastructure |
| `scripts/lang/_test_changed_predicates.sh` | Mine (Domain-judgment) — Part 1 D1=REMOVE: DELETED meta-test | infrastructure |
| `scripts/lang/_test_helpers.sh` | Not mine (Mechanical) — Part 1: remove orphaned run_with_cache + header | infrastructure (via paired-test) |
| `scripts/lang/proto/breaking.sh` | Mine (Mechanical) — Part 1: comment reworded (no 'its own changed.sh') | infrastructure |
| `scripts/lang/proto/changed.sh` | Mine (Domain-judgment) — Part 1 D1=REMOVE: DELETED (consumerless per always-run) | infrastructure |
| `scripts/lang/proto/changed.test.sh` | Not mine (Mechanical) — Part 1 D1=REMOVE: DELETED | infrastructure (via paired-test) |
| `scripts/lang/proto/compile.sh` | Mine (Mechanical) — Part 1: comment drift (no internal skip short-circuit) | infrastructure |
| `scripts/lang/rust/audit-remediation.md` | Mine (Mechanical) — Part 3: prose 'env_tests false' | infrastructure |
| `scripts/lang/rust/behavior-equivalence.test.sh` | Mine (Mechanical) — Part 1: remove vestigial force-touch + false comment | infrastructure |
| `scripts/lang/rust/changed.sh` | Mine (Domain-judgment) — Part 1 D1=REMOVE: DELETED (consumerless per always-run) | infrastructure |
| `scripts/lang/rust/changed.test.sh` | Not mine (Mechanical) — Part 1 D1=REMOVE: DELETED | infrastructure (via paired-test) |
| `scripts/lang/rust/fixtures/equivalence-rust-only.patch` | Mine (Mechanical) — Part 1: reword fixture comment naming changed.sh | infrastructure |
| `scripts/lang/ts/audit-remediation.md` | Not mine (Mechanical, review-only per ADR-0024 §6.3) — Part 3: same prose, TS lane | client |
| `scripts/lang/ts/changed.sh` | Mine (Domain-judgment) — Part 1 D1=REMOVE: DELETED (consumerless per always-run) | infrastructure |
| `scripts/lang/ts/changed.test.sh` | Not mine (Mechanical) — Part 1 D1=REMOVE: DELETED | infrastructure (via paired-test) |
| `scripts/layer-all.sh` | Mine (Domain-judgment) — budget: rescope 90s to guard+audit fast tier (GUARD_AUDIT_DURATION) | infrastructure |
| `scripts/layer1.sh` | Mine (Mechanical) — Part 1: comment straggler naming changed.sh | infrastructure |
| `scripts/layer3.sh` | Mine (Mechanical) — Part 1: remove meta-test run_and_emit + header line | infrastructure |
| `scripts/layer7.sh` | Mine (Domain-judgment) — Part 2: remove browser trigger + browser-e2e-no-diff lane; Phase-1b UNTOUCHED | infrastructure |
| `scripts/layer7.test.sh` | Mine (Domain-judgment) — Part 2: remove B1/B7, reframe B2-B6, drop trigger plumbing | infrastructure, co-impl paired-test |
| `scripts/workflow/run-story.sh` | Mine (Domain-judgment) — Part 3: layer 7 every task; remove env_tests read + conditional | infrastructure |
| `scripts/workflow/run-story.test.sh` | Mine (Domain-judgment) — Part 3: reframe F4; drop env_tests fixtures | infrastructure, co-impl paired-test |

---

## Planning

### The mechanism (restated so the edit set is honestly scoped)

This is **not** "delete a tag." It is: *make gate coverage independent of
change-detection, and retire the one manifest field that encoded the opposite
policy.* Three code changes remove three skip levers (language short-circuit,
browser diff-trigger, per-task layer-7 tag); the `env_tests` field then has no
consumer and its removal is a serde `deny_unknown_fields` schema change that
**must land in one changeset** with every file that still carries `env_tests:`
(struct + engine + CLI + cli.rs + fixtures + template + user-story SKILL + every
story file), or an intermediate state fails to parse. Docs are amended so the
record matches the code.

### Part 1 — dispatcher (`_dispatch.sh`)

Remove the `if [[ "$always_run" != "1" ]]` short-circuit block (~135–145) that
runs `changed.sh` and emits `SKIPPED-NO-DIFF ${name}-no-diff`. With that gone,
**every caller is effectively always-run**, so the `always_run` local + the
`DEVLOOP_DISPATCH_ALWAYS_RUN` read are dead → removed entirely (no vestigial
knob). `scripts/audit.sh` then drops its now-no-op `DEVLOOP_DISPATCH_ALWAYS_RUN=1`.

**Deliberately KEPT (the TRAP):** the `SKIPPED-NO-DIFF` enum, its rank in
`_common.sh:__status_rank`, and the dispatcher's **aggregate** `SKIPPED-NO-DIFF`
arm (~204). The Layer-6 **audit dep-change gate** (`lang/{rust,ts}/audit.sh` via
`_audit_gate.sh:diff_touches_glob`) still emits `SKIPPED-NO-DIFF no-dep-changes`
for a *different* reason (no dependency-manifest change) — a child can still
surface it, so the aggregate arm is a live handler, not a dead branch. Verified
the audit dep-gate uses `diff_touches_glob`, **not** `changed.sh`, so it is
untouched by this change.

### Part 2 — layer7.sh browser suite

Remove `__browser_e2e_triggered()`, `__BROWSER_E2E_TRIGGER_PATHS`, and the
`DEVLOOP_BROWSER_E2E_TRIGGER`/`BROWSER_E2E_TRIGGER_OVERRIDE` seam. The browser
suite runs whenever Layer 7 runs, **subject to the existing Phase-1g
preconditions** (dev-cert fingerprints, Playwright Chromium) which now run
unconditionally instead of trigger-gated. The `SKIPPED-NO-DIFF
browser-e2e-no-diff` lane + its skip note are deleted. **Unchanged:** two-suite
sequential model, per-suite budgets, and env-test-FAIL-skips-browser-this-attempt.
**Explicitly UNTOUCHED (out of scope):** Phase-1b `diff_touches_path "infra/kind/"`
cluster rebuild.

### Part 3 — run-story.sh + dt-story schema

`run-story.sh`: remove `env_tests="$(jq ...)"` (~947); the per-task gate runs
layers 1–6 then **layer 7 unconditionally** (drop the `[ "$env_tests" = true ]`
guard, ~1158–1179). Story-close `layer-all.sh` (~1383) stays. The
operator/implementer rc split already wraps the layer-7 run and is preserved.

`dt-story`: drop `Task.env_tests`, `RunnableTask.env_tests` + its projection,
`NewTask.env_tests` + plumbing, the `validate_manifest` pending-task env_tests
requirement, and the `--env-tests` CLI flag. `cargo build -p dt-story` +
`cargo test -p dt-story` prove it. Then every file carrying `env_tests:` is
edited in the **same commit** (see the classification table).

### Decisions (RULED by @team-lead)

**D1 — RULED: KEEP (Option A)**, contingent on the R8 DRY framing-fix +
@code-reviewer concurrence (given: `changed.sh` composes the live `diff_touches_*`
primitives rather than duplicating them, so no SSoT-drift). **D2 — RULED: protocol
pairing YES** (Gate-1 + Gate-3 reviewer, `Approved-Cross-Boundary` trailer at
commit). Original framing retained below for the record.

**D1 — `changed.sh` + lint-at-startup: keep or remove?** After Parts 1 & 2 the
per-language `changed.sh` wrappers have **zero runtime consumers** (dispatcher
short-circuit gone; layer7 browser trigger gone; layer1 uses INCLUDE/EXCLUDE
staging, not `changed.sh`; the audit gate + layer7 infra/kind use
`_changed_helpers.sh`'s `diff_touches_{path,glob}`, a *different* family that
stays live). Two options:

- **Option A (my recommendation) — KEEP** `changed.sh` (×3), their
  `changed.test.sh` self-tests (×3), `_test_changed_predicates.sh`, and the
  dispatcher lint-at-startup check; reframe lint-at-startup as a **structural
  well-formedness invariant** and document in the dispatcher header + ADR-0033
  §3 that the pipeline no longer short-circuits on `changed.sh`. Fits the task's
  "policy change, not new machinery" framing, smallest blast radius, retains
  test coverage, and the predicates stay a tested SSoT alongside their live
  `diff_touches_*` siblings. Cost: the wrappers are pipeline-unused (test-covered
  + structurally guarded, so *documented*, not *silent*, dead code).
- **Option B — REMOVE** `changed.sh` (×3) + `changed.test.sh` (×3) +
  `_test_changed_predicates.sh` + the lint-at-startup check (stricter no-dead-code
  reading). Larger blast radius incl. a bigger ADR-0033 §3 "decentralized
  classifier" rewrite and removed test coverage.

I lean A for scope discipline; @dry-reviewer + @code-reviewer input wanted, and
I will not proceed on this axis until @team-lead rules.

**D2 — protocol pairing for the `crates/dt-story/**` schema change?** The
2026-08-17 `branch` removal paired protocol under ADR-0024 §6.4 (schema
evolution). This removal of a *required* field is the same criterion. I was not
assigned protocol pairing. @team-lead: confirm whether protocol should be a
Gate-1/Gate-3 reviewer here, or whether infrastructure-owns-dt-story + @paired-test
review suffices for a field *removal* (strictly narrowing).

### Reviewer-surfaced refinements (folded into the plan)

**R1 — the 90s always-run budget is rescoped, not deleted (@observability).**
`layer-all.sh:139-142` sums `always_run_dur = layer[3]+layer[6]` and warns past
`total_budget_secs=90` (ADR-0033 §4 "90s p95 for the always-run set"). Once
layers 1/2/4/5 also always-run, "always-run set = 3+6" is false. **Decision:
Option (b)** — keep the 90s p95 as a deliberately-scoped **guard+audit fast-tier**
latency budget (layers 3+6 only), because layers 1/2/4/5 (compile/fmt/test/lint)
carry inherently variable multi-minute cost that was never in a p95 budget, and
adding a 1–6 total budget would invent a number to calibrate/maintain for no
signal (rationale: gate is 1–2% of story wall-clock; **quota, not compute, is
binding**). Edits: rename `always_run_dur`→`guard_audit_dur` and the emitted
`ALWAYS_RUN_DURATION=`→`GUARD_AUDIT_DURATION=` (@observability endorsed the rename;
only docs reference the token, no code consumer — I confirm no `layer-all.test.sh`
assertion greps it before renaming). The **rationale inverts, not just the scope**:
reword `layer-all.sh:11/90/130-133/139` + ADR-0033 §4 lines 204/414/546-547 +
runbook 143/331/647 to "1–6 all always-run; 3+6 budgeted as the cheap fast floor;
1/2/4/5 excluded because their cost is inherently large/variable — same reason
layer 7 is excluded." `WARN BUDGET_TOTAL_BREACH` alarm token kept.

**R2 — the dispatcher aggregate `SKIPPED-NO-DIFF` arm is KEPT, with a comment
(@observability challenged; @dry-reviewer + @semantic-guard endorsed keep).**
@observability's premise ("the dispatcher can no longer produce a per-child
SKIPPED-NO-DIFF") is factually incomplete: the Layer-6 audit dep-gate
(`lang/{rust,ts}/audit.sh`) still emits `SKIPPED-NO-DIFF no-dep-changes`, so the
enum is still **child-producible**. The `case` is a total mapping over
`aggregate_worst_status`'s output; removing the arm would route a valid enum to
the `*)`→`FAIL aggregate-unknown` catch-all, misclassifying a benign all-skip as
FAIL. The aggregate arm is currently unreachable only because proto's placeholder
`N/A` dominates the audit verb — a property of the lang *set*, not the dispatcher.
Per this repo's own "forward-compatible handler vs dead lane" doctrine (see
`layer7.sh`'s 429-arm rationale), this is a kept handler, not a dead lane. I add a
comment documenting exactly this so it is *documented*, not *lingering*.

**R2b — the kept arm's REASON is renamed `all-langs-untouched`→`all-langs-skipped`
(@observability's contradiction catch).** Keeping the arm while R3 retires the
`all-langs-untouched` token is a contradiction: `_dispatch.sh:204` literally emits
that token. And it is now a *stale descriptor* — post-change an aggregate
`SKIPPED-NO-DIFF` means every child's verb RAN and returned a non-dominating skip
(all-audit `no-dep-changes`, no N/A lang), so the langs were **not** "untouched"
("untouched" was precisely the deleted short-circuit's semantic). Rename the arm's
REASON to `all-langs-skipped` and make the documenting comment match its
post-change reachability. This makes R3's "`all-langs-untouched` retires" literally
true (gone everywhere, including here) while keeping the forward-compat handler.

**R3 — full REASON-token retirement (@observability, @operations, @dry-reviewer).**
Beyond `browser-e2e-no-diff`, the language short-circuit removal retires the
per-child `<lang>-no-diff` (`rust-no-diff`/`ts-no-diff`) and aggregate
`all-langs-untouched` tokens. After the change, `no-dep-changes` (audit dep-gate)
is the **only** surviving `SKIPPED-NO-DIFF` REASON. Runbook edits (line-precise,
**§6.6 explicitly included** per @observability): legend §4 (83), the
`LAYER=2 RESULT=SKIPPED-NO-DIFF` example (21), the `aggregate_worst_status …
ts-no-diff` worked example (110-112), Layer-1 desc (278), §6.1 rust/ts-no-diff
example (296-297), the §6.6 `STATUS=SKIPPED-NO-DIFF — diagnosing predicate output`
section (554-566, 579/591/594 — reframed to the audit dep-gate producer; under
D1=KEEP the `changed.sh`-manual-invoke bits are reworded to "well-formedness
check, not short-circuit", not deleted), the
audit dep-gate line that says `DEVLOOP_DISPATCH_ALWAYS_RUN=1 is RETAINED` (412 —
now wrong: the dispatcher is unconditionally always-run, so the env var is gone),
browser rows (477, 502, 622), and the docs-only-PR Layer-1 symptom (625). The
rank ladder (101) and `no-dep-changes` RUN/SKIP boundary (414) stay unchanged.

**R4 — side effect worth recording: the dt-guard producer/consumer skew closes.**
Runbook §8 (637) documents that `target/release/dt-guard` goes missing on a
`packages/**`-only diff because `lang/rust/compile.sh`'s `cargo build` sat behind
the skip-gated compile verb while ~15 Layer-3 guards consume it always-run. With
Layer-1 rust compile now **always-run**, `dt-guard` is always built — the skew is
**closed** by this change. I update 637 to say so (a genuine improvement, not a
new risk).

**R5 — positive proof the ordinary path now runs layer 7 (@paired-test).** Add
`assert_marker … 'ran.layer7'` to the run-story.test.sh D1 baseline (a plain,
untagged task). Without it nothing proves the core behavior change; the F4 rc-split
coverage is reframed onto a plain task, not deleted.

**R6 — validate_manifest RELAXES by exactly one field, nothing else
(@security, @semantic-guard).** `env_tests` was *required for pending tasks*
(engine.rs:381-382 pushes a violation when absent) but `Option<bool>` in the
struct (completed stubs omit it). Removing it drops that one requirement; the
`specialist` and `prompt` pending-task requirements are untouched, and
`deny_unknown_fields` now **rejects** a stray `env_tests:` (fail-closed, louder
than before). No other check becomes a no-op.

**R7 — test-load split with @paired-test.** @paired-test takes first cut on
`crates/dt-story/tests/cli.rs` fixtures + `run-story.test.sh` F4/D1; I take
`_dispatch.test.sh` + `layer7.test.sh` + the dt-story source/build loop. All four
are co-reviewed. Test must-fixes @paired-test raised, all adopted: (i) the
`_dispatch.test.sh` masking reframe uses a `changed.sh exit 1` (would-be-untouched)
lang with a **working** verb and asserts `STATUS=OK` — the positive "short-circuit
genuinely gone" proof; do not let all fixtures collapse to `changed.sh exit 0`.
(ii) `layer7.test.sh` B2 keeps its explicit `STATUS=OK REASON=browser-e2e-passed`
assertion (proves the browser suite ran independent of diff); B4/B5 now fire
unconditionally (dev-certs + Chromium demanded on every layer-7) — flagged for
operations in the output doc. (iii) `run-story.test.sh` F4 drops the custom
fixture swap (DEFAULT_TASKS is already loaded) → just `run_story FAKE_LAYER7_RC=2`;
and the **D1 green baseline (~738) gains `assert_marker … 'ran.layer7'`** — the
positive proof an ordinary untagged task runs *and completes* layer 7 (not
redundant with F4's red-split path).

**R8 — mandatory DRY framing-fix under the D1=KEEP ruling (@team-lead cond. (a),
@dry-reviewer, @code-reviewer).** KEEP is contingent on making the retention
LOUD, not buried. Reword every "changed.sh is the **sole authority** for its
footprint" assertion to "per-lang **registration / well-formedness convention**,
**no short-circuit consumer** — compose `diff_touches_*` directly for
diff-triggered behavior (see `_audit_gate.sh` / layer7)": sites are
`_test_changed_predicates.sh:4`, the three `changed.sh` headers, ADR-0033
§2/§3 (lint-at-startup rationale), and `docs/specialist-knowledge/*/INDEX.md`
(dry-reviewer INDEX). The `_dispatch.sh` header (lines 2–14, 42–44) + the
lint-at-startup comment (113) get the same reframe. The **fail-OPEN insight** from
`_audit_gate.sh` ("skip-if-untouched treats non-zero as skip = fail-OPEN, unsafe
for a security scan") is preserved by MOVING it into the ADR-0033 §3 amendment as
the rationale for why always-run is the safe direction — not deleted with the
stale comment.

**R9 — run-story GATE label stays `1-6+7` with an unconditional-comment
(@observability).** Keep `layers=1-6+7` (preserves the "7 is a separate cost
envelope" semantic the 90s budget encodes) but add a one-line comment at the emit
site that layer 7 is now **unconditional** — else a future reader re-derives the
old "this task was env_tests-tagged" meaning from the `+`. `run-story.test.sh:882`
(F4) is the consumer that greps the old label — updated in @paired-test's lane.

**R10 — protocol precision (kept concept vs removed field) (@protocol).** Remove
ONLY the underscored manifest field/flag. The hyphenated "env-test(s) tier"
references (the Layer-7 testing tier) STAY — e.g. `_template.md:62` ("env-test
scenarios") and incidental prose in `AI_DEVELOPMENT.md`, `adr-0032`,
`code-reviewer/INDEX.md`. Same field-vs-tier care applies to the
browser-client-join prose (1524). `validate-story-manifest.sh` scopes to
`docs/user-stories/*.md` only, so the two `docs/devloop-outputs/*` files carrying
`env_tests:` are never parsed — confirming they're safely left as append-only.

### Sequencing & verification

1. Land all source + test + doc edits as one coherent changeset.
2. `cargo build -p dt-story && cargo test -p dt-story` (schema proof).
3. Run affected shell self-tests: `_dispatch.test.sh`, `layer7.test.sh`,
   `run-story.test.sh` (via their Layer-3 wiring), plus `dt-story validate`
   against every edited story file so nothing fails `deny_unknown_fields`.
4. `scripts/guards/simple/validate-story-manifest.sh` green (validates
   `_template.md` + story files repo-wide).

---

## Implementation Summary

Landed as one coherent changeset (44 files: 37 edited, 7 deleted). All three parts + D1=REMOVE + doc amendments.

**Verified green locally:**
- `cargo build -p dt-story` + `cargo test -p dt-story` → **47/0** (incl. `validate_rejects_removed_env_tests_field`, the deny_unknown_fields proof); `cargo fmt`/`clippy -p dt-story` clean.
- Shell self-tests: `_dispatch.test.sh` 51/0, `layer7.test.sh` 190/0, `run-story.test.sh` 182/0, `_layer_skeleton.test.sh` 35/0, `_common.test.sh` 49/0, `_changed_helpers.test.sh` 15/0, `_audit_gate.test.sh` 43/0, `layer-all.test.sh` 26/0.
- Real-tooling layers: **Layer 2 (fmt) OK**, **Layer 3 (guards + all wired self-tests) OK** (both cross-boundary guards green; no `predicate-meta-test` — correctly removed), **Layer 6 (audit) N/A/pass** (dep-gate emits `no-dep-changes` — the surviving SKIPPED-NO-DIFF producer — with the `DEVLOOP_DISPATCH_ALWAYS_RUN` knob removed; buf-breaking OK).
- Real-tree dispatcher smoke: iterates the now-`changed.sh`-free lang tree with no missing-changed.sh error.
- `grep`-verified: no surviving live `changed.sh`/`changed.test.sh`/`_test_changed_predicates` references except dated-history records and explicit "retired/deleted" notes.

**D1=REMOVE terminal state applied:** deleted `lang/{rust,ts,proto}/changed.sh` ×3, `lang/{rust,ts,proto}/changed.test.sh` ×3, `_test_changed_predicates.sh`, the `_dispatch.sh` lint-at-startup block + `test_missing_changed_sh`, `_test_helpers.sh:run_with_cache`, and the `behavior-equivalence.test.sh` force-touch. Kept `_changed_helpers.sh` (live `diff_touches_*`). ADR-0033 §2/§3 amended to "changed.sh classification retired". Comment stragglers swept (`layer1.sh`, `_common.sh`, `proto/breaking.sh`, `proto/compile.sh`, `_audit_gate.sh`, `audit-suppressions-check.test.sh`, `equivalence-rust-only.patch`).

**Side effect recorded (R4):** the dt-guard/dt-story producer/consumer skew is CLOSED — `lang/rust/compile.sh` (which builds both release binaries) now always-runs, so they build on every devloop. Runbook §6.3/§8 + `docs/TODO.md` updated.

---

## Files Modified

**51 files changed, 967 insertions(+), 1073 deletions(-)** — 7 deletions (the `changed.sh`×3
+ `changed.test.sh`×3 + `_test_changed_predicates.sh` under D1=REMOVE). By area: `scripts/` 32,
`docs/` 9, `crates/` 6 (all `dt-story`), `.claude/` 2, `packages/` 1, `.github/` 1. The +6 over
the original 44-file plan are the review-round comment/doc straggler fixes (`ci.yml`,
`build.sh`/`fmt.sh`/`lint.sh` headers, `_changed_helpers.sh` header), each added to the
Cross-Boundary Classification table as Mechanical rows; scope-drift guard verified clean (Gate 2).

See the Cross-Boundary Classification table above for the per-file change list and ownership.

---

## Code Review Results

Gate 3 unanimous (8/8), **zero deferrals, zero escalations** — every finding was a doc/comment
straggler fixed in-diff.

| Reviewer | Verdict | Findings (fixed) |
|----------|---------|------------------|
| Protocol (dt-story schema) | CLEAR | 0 — hunk-ACK'd manifest/engine/main schema hunks; confirmed `Approved-Cross-Boundary: protocol` trailer |
| Semantic Guard | CLEAR (SAFE) | 0 — verified no silent-skip reintroduced; layer7 fail-loud asymmetry + `\|\| true` intact; `validate_manifest` not weakened |
| Test (paired) | CLEAR | 0 — every removed skip has a positive always-run assertion; no dangling test-wiring; new `validate_rejects_removed_env_tests_field` + `[browser-ok-no-skip-lane]` guards |
| Security | RESOLVED-FIXED | 1 — `_changed_helpers.sh` stale header → repointed to real live consumers (audit dep-gate + layer7) |
| Code Quality | RESOLVED-FIXED | 2 — `ci.yml` stale comment; `_changed_helpers.sh` header. ADR-0033/0035/0002/0024 compliance verified |
| Observability | RESOLVED-FIXED | 1 — incomplete skip-if-untouched sweep (3 dispatcher headers + runbook §6 + §6.1 example) |
| Operations | RESOLVED-FIXED | 2 — runbook infra/kind rebuild note (2nd home); `TODO.md` buf-generate entry past-tensed + de-pinned |
| DRY | RESOLVED-FIXED | 2 — `ADR-0033:181` present-tense clause; `test/INDEX.md:61` dangling pointer |

**Gate 2:** `layer-all.sh` exit 0 on the final tree (re-run after review; `GATE2=PASS` verdict).
Layers 1/2/3/5/7 OK, L4/L6 N/A→exit 0 (proto's absent test/audit verbs + audit `no-dep-changes`
dep-gate skip). **Layer 7 OK: `env-tests-passed` + `browser-e2e-passed`** against a live cluster —
the change proving its own thesis (layer 7 + browser E2E run every task, unconditionally, and pass).

---

## Accepted Deferrals

- (none surfaced in this devloop) — every review finding was fixed in-diff; zero findings left in tree. Two forward-looking items (the `TOTAL_RESULT=N/A`-on-green top-line clarity item and the premises-updated buf-generate entry) were recorded in `docs/TODO.md`, not here — they are future improvements, not deferred findings.

---

## Rollback Procedure

1. Start commit: `0216eab9c0a47b7d69b1e551001790328d131d32`
2. `git diff 0216eab..HEAD`
3. `git reset --hard 0216eab` (no schema/infra apply steps in this devloop)
