# ADR-0033: Polyglot Validation Pipeline Strategy

**Status**: Accepted

**Date**: 2026-05-07

**Deciders**: Lead (synthesis); specialists security, test, observability, operations, infrastructure, client, protocol (debate participants — see `docs/debates/2026-05-06-polyglot-validation-pipeline-strategy/debate.md`)

---

## Context

The Dark Tower devloop validation pipeline (Layers 1-8 defined in `.claude/skills/devloop/SKILL.md`) and the supporting `scripts/guards/` tree were designed for a Rust-only workspace. As of 2026-05, the repository contains three first-class toolchains:

- **Rust**: `cargo {check, fmt, clippy, audit, test}` against `crates/**`
- **TypeScript**: `tsc`, `prettier`, `eslint`, `pnpm audit`, `vitest`, `vite build`, `nx affected` against `packages/**`
- **Protobuf**: `buf {lint, format, breaking}` against `proto/**`

The first TypeScript devloop (`packages/test-utils/`, R-39, 2026-05-06) surfaced concrete pipeline gaps:

- `pnpm audit` was not in the standard pipeline → 3 high-severity transitive ReDoS vulns in `nx@20.3.0`'s `minimatch` chain were latent until the first TS task ran the audit ad-hoc
- The Layer A scope-drift guard's strict-literal path matcher tripped 2 of 3 Gate 2 attempts on TS file paths
- TS-specific guards (no-secrets-in-ts, no-test-removal-ts, no-pii-in-logs-ts, name-guard-dt-client, bundle-content per R-14, exports-map-closed) do not exist
- `buf lint`, `buf format`, `buf breaking` are not wired into the validation pipeline; they were planned to land only in CI per user-story task #17 (which is itself not yet shipped)
- `.claude/skills/devloop/SKILL.md` auto-detection patterns and Layer N/A justification template are Rust-shaped
- `scripts/test.sh`, `scripts/verify-completion.sh`, `scripts/guards/run-guards.sh` are Rust-only or Rust-prioritized

The implementer of the test-utils devloop and the Lead had to invent ad-hoc TS equivalents (`tsc --noEmit`, `vitest run`, `pnpm audit`) and mark cargo-only layers N/A — the kind of per-task improvisation that erodes the pipeline's correctness guarantees over time.

### Constraints

- The pipeline must accommodate a 4th and 5th toolchain (Go, Python, Helm, etc.) without re-architecture
- Local devloop validation and CI must enforce the same gates ("router-drift" between local and CI is a category of bug we want to make structurally hard)
- Existing devloop muscle memory (`./scripts/test.sh --workspace`) must not break
- The polyglot pipeline must close the supply-chain audit gap that produced the minimatch incident
- In-flight user-story work — task #9 (sdk-core), task #15 (web-app), task #17 (ci-client.yml), and the R-61 cleanup chain (#29/#30/#31, especially #31's wire-breaking proto rename sweep) — must not be blocked

## Decision

We adopt a **layer-script-driven pipeline with per-language wrapper convention**: pipeline orchestration lives in shell scripts (`scripts/layerN.sh`), not in `SKILL.md` prose; languages are discovered by directory presence and verb-script existence, not by registry; the diff classifier is decentralized per-language with shared helpers; semantic review is moved from a layer to the reviewer panel.

### 1. Architecture

```
scripts/
  layer-all.sh                  # NEW — orchestrator: runs layer1..7 with logging + budget
  layer1.sh                     # NEW — Compile (proto first, then rust+ts)
  layer2.sh                     # NEW — Format
  layer3.sh                     # NEW — Guards (calls scripts/guards/run-guards.sh)
  layer4.sh                     # NEW — Test
  layer5.sh                     # NEW — Lint
  layer6.sh                     # NEW — Audit (always-run, no skip)
  layer7.sh                     # NEW — Env-tests (renumbered from Layer 8; semantic moved to reviewer panel)

  test.sh                       # KEEP NAME — refactored: per-verb dispatcher (test verb)
  verify-completion.sh          # KEEP NAME — calls layer-all.sh (preserves muscle memory)
  audit.sh                      # NEW — per-verb dispatcher
  fmt.sh                        # NEW — per-verb dispatcher
  lint.sh                       # NEW — per-verb dispatcher
  build.sh                      # NEW — per-verb dispatcher (compile)

  lang/
    _common.sh                  # NEW — sourced helper: cache paths, cross-layer state
    _dispatch.sh                # NEW — sourced helper: language iteration + missing-verb handling
    _changed_helpers.sh         # NEW — sourced helper: declarative diff predicates
    _get_base_ref.sh            # NEW — env-aware base-ref resolver (local vs CI)
    _get_base_ref.test.sh       # NEW — self-test matrix (local-clean, local-dirty, CI-PR, CI-push, first-commit)
    _test_changed_predicates.sh # NEW — meta-test: each language's changed.sh fires on representative paths

    rust/
      changed.sh                # exit 0 if rust touched, 1 if not
      changed.test.sh           # locality self-test for changed.sh
      compile.sh                # cargo check
      fmt.sh                    # cargo fmt
      lint.sh                   # cargo clippy
      test.sh                   # cargo test (called by scripts/test.sh dispatcher)
      audit.sh                  # cargo audit

    ts/
      changed.sh
      changed.test.sh
      compile.sh                # nx affected -t typecheck
      fmt.sh                    # nx affected -t format
      lint.sh                   # nx affected -t lint
      test.sh                   # nx affected -t test:unit test:component
      audit.sh                  # pnpm audit --audit-level=high
      e2e.sh                    # nx affected -t test:e2e (Playwright)

    proto/
      changed.sh
      changed.test.sh
      compile.sh                # buf build
      format.sh                 # buf format --diff --exit-code
      lint.sh                   # buf lint
      breaking.sh               # buf breaking (always-run)
      # Note: no test.sh or audit.sh — proto has no test verb; breaking.sh covers audit

  guards/
    run-guards.sh               # KEEP — universal, runs all simple/*.sh (per-guard self-classification preserved)
    common.sh
    simple/
      rust/                     # MOVED — existing Rust guards
      ts/                       # NEW — TS guards (Wave 2)
      proto/                    # NEW — proto-specific guards if any emerge
      universal/                # cross-cutting (knowledge-index, scope-drift, GSA-sync)
```

### 2. Classifier — Per-Language Convention

The classifier is decentralized: each `lang/<X>/changed.sh` is the sole authority for its language's footprint. `changed.sh` exits **0 if the diff touches that language, 1 if provably untouched.** No centralized rules table.

Each `changed.sh` is 3–5 lines using shared helpers from `scripts/lang/_changed_helpers.sh` (so each language's predicate is declarative intent, not bespoke shell):

```bash
# scripts/lang/rust/changed.sh
#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "$0")/../_changed_helpers.sh"
diff_touches_path "crates/" || diff_touches_root_files "Cargo.toml" "Cargo.lock" "rust-toolchain.toml"
```

```bash
# scripts/lang/ts/changed.sh
#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "$0")/../_changed_helpers.sh"
diff_touches_path "packages/" || diff_touches_root_files \
  "package.json" "pnpm-lock.yaml" "pnpm-workspace.yaml" "nx.json" "tsconfig.base.json" ".nvmrc"
```

```bash
# scripts/lang/proto/changed.sh
diff_touches_path "proto/"
```

**`lang/<X>/changed.sh` is mandatory.** `scripts/lang/_dispatch.sh` lints at startup that every directory under `scripts/lang/` (excluding underscore-prefixed helpers) contains an executable `changed.sh`. Missing predicate fails loud — language is never silently invisible.

**Meta-test (`_test_changed_predicates.sh`)** asserts each language's `changed.sh` fires correctly against a hand-curated fixture set. Drift between predicates (one language stricter than another) is detectable in CI. Each language also ships its own `lang/<X>/changed.test.sh` for predicate self-testing in locality.

### 3. Always-Run vs Skip-If-Untouched

Default: run everything. Skip a language-specific layer only when its `changed.sh` proves the diff is untouched.

| Layer | Verb     | Always-run                              | Skip-if-untouched per `lang/<X>/changed.sh` |
|-------|----------|-----------------------------------------|---------------------------------------------|
| 1     | Compile  | —                                       | rust, ts, proto                             |
| 2     | Format   | —                                       | rust, ts, proto                             |
| 3     | Guards   | ALL guards (each self-classifies)       | —                                           |
| 4     | Test     | —                                       | rust, ts (proto has no `test.sh` — naturally skipped) |
| 5     | Lint     | —                                       | rust, ts, proto                             |
| 6     | Audit    | cargo audit, pnpm audit, buf breaking   | —                                           |
| 7     | Env-tests| dev-cluster + Rust env-tests + Playwright `@smoke` | —                                |

**The classifying principle (so future toolchains classify themselves):**

> A step is **always-run** if its failure mode can be triggered by external state change with no diff in the toolchain's footprint (advisory DB updates, contract evolution against `main`, source-of-truth integrity drift). A step is **skip-if-untouched** if its failure mode requires source change in the corresponding file family (compile errors, type errors, lint violations, formatting drift, test regressions).
>
> When in doubt, always-run.

Applied to the prompted hard cases:
- `cargo audit`, `pnpm audit` → always-run (vulns publish independently of our diff)
- `buf breaking` (vs `main`) → **always-run** despite being proto-related (re-running codegen against a drifted `main` can break wire compat without a `proto/**` diff in the current branch; ~500ms)
- `buf lint`, `buf format` → skip-if-no-proto
- `cargo check` (no Rust changed) → skip
- `tsc --noEmit` (no TS changed) → skip

**Files outside any classified directory** (`infra/`, `docs/`, `scripts/`, `.github/`, `.claude/`) do not trigger any skip optimization. They get the always-run set (Layers 3, 6, 7 + reviewer panel) and skip the language-specific layers — exactly the right behavior. No special "neutral" registration required.

### 4. Layer Scripts as Canonical Pipeline Entry Points

All pipeline orchestration lives in shell scripts. `SKILL.md` Step 6 collapses to:

> Run `scripts/layer-all.sh`, OR `scripts/layer1.sh` through `scripts/layer7.sh` in numeric order. On non-zero exit, capture output and follow Gate 2 protocol. Each script self-reports `STATUS=` lines per the wrapper contract.

**`scripts/layerN.sh` contract** (mandatory for every layer):

- `set -euo pipefail` at top
- Sources `scripts/lang/_common.sh` for cache paths and shared state
- Calls per-verb dispatchers (`scripts/{audit,lint,test,fmt,build}.sh`) or directly invokes per-language wrappers
- Streams every child wrapper's `STATUS=` line verbatim to stdout
- Computes its own `STATUS=` as the **worst child STATUS** (FAIL > N/A > SKIPPED-NO-DIFF > OK) and emits as the final stdout line
- Emits one summary line to **stderr**: `LAYER=N START=<unix-ts> END=<unix-ts> RESULT=<enum>`

**`scripts/layer-all.sh`** is the recommended entry point for full validation:

- Runs `layer1.sh` through `layer7.sh` sequentially
- Redirects each layer's stdout to `tee /tmp/devloop/layer-N.log` (or per-devloop slug equivalent)
- Emits a final summary table: layer, status, duration
- Enforces the **90s p95 wall-clock budget for the always-run set** (Layers 3, 6 + reviewer panel cost is excluded from this budget). Warns when any single layer exceeds its per-layer budget; this is the operational signal that catches budget breach before it becomes a paging incident.

Individual `layerN.sh` remain directly callable for targeted debugging (`scripts/layer4.sh` to re-run only Layer 4's tests on a failing diff).

### 5. Layer 1 Ordering — Buf First Within Compile

A dedicated "Layer 0: Contract" was considered and rejected. The execution-order benefit (proto failure surfaces before Rust/TS type-error cascades) is captured by `scripts/layer1.sh` running proto before rust+ts:

```bash
# scripts/layer1.sh — Compile
set -euo pipefail
source "$(dirname "$0")/lang/_common.sh"

# Stage 1: contract (proto consumed by both downstream)
"$(dirname "$0")/lang/proto/changed.sh" || { /* run if changed */ }
# Stage 2: code (rust + ts can run in any order)
# ... see scripts/build.sh for the dispatch
```

The "proto-first" ordering is encoded in `scripts/layer1.sh`, not in `SKILL.md` prose. If a second contract-layer tool ever lands (OpenAPI breaking checks, etc.), promote to a dedicated Layer 0 then. YAGNI for now.

### 6. Wrapper Contract

Every per-language wrapper (`lang/<X>/<verb>.sh`) honors a uniform exit-and-output contract:

| Exit code | Meaning |
|-----------|---------|
| 0         | OK / SKIPPED-NO-DIFF / SKIPPED-NO-VERB / N/A-with-reason (success) |
| 1         | FAIL (the work ran and detected a problem) |
| 2         | Wrapper / dispatcher bug (unexpected error; investigate the script itself) |

Final stdout line: `STATUS=<OK|FAIL|SKIPPED-NO-DIFF|SKIPPED-NO-VERB|N/A> REASON=<short string, no spaces in value>`. Dispatchers parse this for aggregation; CI summary jobs reuse the same parser.

**Verb discovery via file existence.** Per-verb dispatchers (`scripts/{audit,lint,test,fmt,build}.sh`) iterate `scripts/lang/*/`:

```bash
# scripts/test.sh — illustrative shape
source scripts/lang/_dispatch.sh
for_each_lang_with_verb "test" || exit 1
```

`for_each_lang_with_verb` (defined in `_dispatch.sh`):
1. Iterates `lang/*/` directories (excluding underscore-prefixed)
2. For each language, lints that `changed.sh` exists (fails loud if not)
3. If the requested verb script exists and is executable, invokes it (with skip-if-untouched short-circuit via `changed.sh`)
4. If the verb script is missing or not executable, emits `STATUS=SKIPPED-NO-VERB REASON=<lang>/<verb>.sh missing-or-not-executable` — never silently continues

This means proto's lack of `test.sh` produces a visible `SKIPPED-NO-VERB` entry in the layer log, not silent absence.

### 7. Diff Base Resolution

`scripts/lang/_get_base_ref.sh` resolves the diff base differently per environment:

| Context           | Base ref                                              | Diff form                                  | Includes |
|-------------------|-------------------------------------------------------|--------------------------------------------|----------|
| Local devloop     | `git merge-base origin/main HEAD`                     | `git diff <base>` + untracked files        | committed + staged + unstaged + new files |
| CI on PR          | `origin/$GITHUB_BASE_REF`                             | `git diff <base>...HEAD` (three-dot)       | PR-only commits |
| CI on push to main| `HEAD~1` (with `git rev-parse --verify` fallback to `HEAD` for first-commit edge case) | `git diff <base>` | Push commits |

Untracked files in local mode are unioned via `git ls-files --others --exclude-standard` so brand-new files are visible to the classifier (a new `crates/foo/src/lib.rs` that doesn't compile yet would otherwise be invisible to `git diff`).

**Detection rules** (encoded in `_get_base_ref.sh`):
- CI detection via `[[ -n "${GITHUB_ACTIONS:-}" ]]`
- Branch on `$GITHUB_EVENT_NAME` (pull_request vs push) — `$GITHUB_BASE_REF` is empty on push events, so it cannot be the sole detection signal
- For `pull_request`, defensively `git fetch --no-tags origin "$GITHUB_BASE_REF"` so sparse-checkouts and worktrees that don't have the base ref locally still work; fail loud if the ref is unreachable
- For `push`, `HEAD~1` may not exist on the first commit of a branch or on certain squash-merge fast-forwards; guard with `git rev-parse --verify HEAD~1 2>/dev/null` and fall back to `HEAD` (effectively classifying everything as touched, which is correct conservative behavior)

**Normative requirement**: every invocation of `_get_base_ref.sh` emits one line to stderr:

```
BASE_REF=<sha> BASE_SOURCE=<local-mergebase|ci-pr|ci-push-main|ci-push-first-commit> DIFF_MODE=<two-dot|three-dot> FILES_CHANGED=<count>
```

This makes "what diff did the validation actually see?" greppable in any layer log — load-bearing for 3am debuggability.

A self-test matrix (`_get_base_ref.test.sh`) covers: local-clean, local-dirty (working tree), local-with-untracked, CI-PR, CI-push, first-commit. Lands in Wave 1 alongside `_get_base_ref.sh` itself.

### 8. Reviewer Panel — Semantic Review Moved

Layer 7 in the prior pipeline ran the semantic-guard agent. Spawning agents from shell scripts requires shelling out to the `claude` CLI from bash, which adds a runtime dependency and contaminates the pure-shell layer model.

**Decision**: semantic-guard moves from a pipeline layer to a reviewer slot in the Gate 2 reviewer panel, alongside security, code-reviewer, dry-reviewer, observability, operations, and test reviewers. The reviewer panel grows from **6 → 7 reviewers**.

- Layer 8 (env-tests) renumbers to **Layer 7**. Layer-script set is now `layer1.sh` through `layer7.sh`.
- Pipeline layer scripts stay pure shell with no agent-runtime dependency.
- Semantic-guard runs at the same logical moment it ran before (after all gates pass, before close-out) — the timing is preserved; only the framework moves.

**Overlap with code-reviewer: coexist (option a).** Semantic-guard targets specific patterns (subtle behavioral changes, undocumented contract drift, scope creep beyond stated task); code-reviewer is general (Rust idioms, error handling, ADR compliance, naming). The lenses are distinct and produce visibly different artifacts in practice. Absorbing semantic-guard into code-reviewer's prompt risks dilution of code-reviewer's general focus by a specific checklist; replacement loses the broader review entirely.

**Deduplication step** in the panel summary: if semantic-guard and code-reviewer surface the same finding, the panel aggregator dedupes before presenting to the user. The panel already aggregates verdicts; this is a small extension.

### 9. Nx Integration

The TypeScript wrappers (`lang/ts/*.sh`) call `nx affected -t <target> --base=$(scripts/lang/_get_base_ref.sh)` natively. We wrap Nx, not replace it — Nx's diff-aware project-graph dispatch is more granular than directory-based classification (it understands transitive dependencies between TS packages) and the JS ecosystem already reaches for Nx idioms. The wrapper translates Nx's output into the dispatcher's uniform `STATUS=` schema.

### 10. Single Source of Truth

CI workflows call `scripts/layer-all.sh` (or per-layer `scripts/layerN.sh`) directly rather than reimplementing per-toolchain dispatch in GitHub Actions YAML. This eliminates router-drift between local devloop and CI: there is one set of layer scripts, one set of dispatchers, one set of wrappers — used by both surfaces. A YAML configuration generator (e.g., to emit GHA `paths:` filters) is **not** required for Wave 1; tracked as a Wave 3 follow-up if duplication emerges.

### 11. Audit-Level Configuration Ownership

Audit-level configuration (`cargo audit` allowlists, `pnpm audit --audit-level` thresholds, advisory exemptions) is **owned by the security specialist**, not infrastructure. Infrastructure owns the wrapper plumbing that invokes the tools; security owns the policy thresholds. Edits to `deny.toml` / `audit-config.json` / equivalent require security review.

### 12. Audit Fatigue — Status Quo, with Tripwire

The pipeline ships with `cargo audit` + `pnpm audit` always-run, hard-block on failure. We explicitly do not add a triage workflow, nightly watchdog, or warn-don't-block escape hatch in this round. The known consequence: developers will continue the existing pattern of ignoring audit findings on transitive vulns they did not introduce.

**Tripwire**: Revisit audit triage workflow design if mean-time-to-resolution for high-severity advisories exceeds **14 days**, measured from advisory publication date. This is documentary only — no implementation, no metric, no automated detection. Future-us has a documented threshold to point at.

### 13. Intentional Wire-Breaks

`buf breaking` always-run will fire on PRs that intentionally evolve the wire format (e.g., R-61 task #31). The requirement: **intentional wire-breaks must be acknowledged explicitly in-tree, never via CI bypass flags or environment overrides.** The specific override mechanism (per-line `# buf:breaking:ignore` comment vs an annotated ratchet list at `proto/buf-breaking-allowlist.md`) is deferred to Wave 3, when we have ≥2 real wire-breaking PRs as case studies.

### 14. Flake-Rate Budget

| Layers | Flake target |
|--------|--------------|
| 1-6 (Compile, Format, Guards, Test, Lint, Audit) | **Zero-flake** — any flake is a P1 bug |
| 7 (Env-tests, including Playwright) | **<2% rerun rate** measured weekly |
| Reviewer panel (incl. semantic-guard) | Best effort — agent-based, occasional model variance acceptable |

Flake-rate breaches in Layer 7 trigger quarantine of the offending test (`.skip` or `#[ignore]` with TODO link), not a pipeline-wide block.

## Consequences

### Positive

- **Closes the minimatch class of incident permanently.** `pnpm audit` is always-run; transitive supply-chain drift surfaces every devloop, not weeks later.
- **Pipeline knowledge is greppable, diffable, testable shell.** `SKILL.md` collapses from per-layer prose to "run `scripts/layer-all.sh`" — drift between documentation and runtime behavior becomes structurally impossible.
- **Layer scripts are independently debuggable.** When Layer 4 fails, a dev runs `scripts/layer4.sh` directly and reproduces bit-for-bit what the pipeline saw. Bisecting regressions by replaying layers in isolation becomes trivial.
- **Safe-by-default failure mode.** Default-to-run + skip-only-when-provably-untouched means classifier bugs cause spurious work, never silent skipping.
- **Polyglot extensibility is structural.** A 4th language adds: `mkdir scripts/lang/go && touch changed.sh test.sh lint.sh build.sh fmt.sh audit.sh && chmod +x *.sh` — zero edits to dispatchers, layer scripts, or `SKILL.md`.
- **Single source of truth for dispatch logic.** Local and CI run identical layer scripts; "works on my machine" failures collapse.
- **Surface-root-cause-first ordering.** Proto failures surface as buf errors in `layer1.sh` stage 1, not as Rust E0277 / TS TS2322 cascades downstream.
- **Cross-language schema-drift gate.** `buf breaking` always-run catches main-rebase wire-breaks that no proto-diff would surface.
- **Decentralized classifier strengthens single-source-of-truth.** Each language owns its own footprint definition (`lang/<X>/changed.sh` + `changed.test.sh`); no centralized rules table to drift.
- **Pure-shell layer model.** Removing semantic-guard from layers eliminates the agent-runtime dependency from the pipeline; layers stay testable, hermetic, and CI-portable.
- **Existing scripts preserved.** `scripts/test.sh` keeps its name and external contract; existing CI, runbooks, and muscle memory continue to work.
- **Zero new tooling dependencies.** Bash sourcing throughout — runs in any bash environment including stripped-down CI containers. No `yq`/`jq`/Node required for classification.

### Negative

- **File count grows.** ~25 files in `scripts/` (7 layer scripts + `layer-all.sh` + 5 per-verb dispatchers + 5 underscore helpers + ~18 per-language wrappers across rust/ts/proto + meta-tests). Each is small (10-60 LoC); maintainability remains net-positive but the surface is wider than the Rust-only baseline.
- **Renumbering churn.** Layer 8 → Layer 7 affects `SKILL.md`, `ADR-0030`, this ADR, debate doc, and ≤4 specialist-knowledge `INDEX.md` files (~20 active edits, ~45 min mechanical work). Devloop output history files retain "Layer 8" as historical record.
- **Audit-fatigue debt is codified, not solved.** Developers will continue to ignore audit findings on transitive vulns they did not introduce. We accept this trade-off; the 14-day MTTR tripwire is the only mitigation.
- **`buf breaking` will fire on intentional wire-breaks** until override mechanism lands in Wave 3. R-61 task #31 sequences after Wave 2 #4 (proto wrappers) and possibly Wave 3 (override mechanism) for that reason.
- **Modest CI-minutes increase per PR.** Always-run set adds `pnpm audit` (~5s) and `buf breaking` (~500ms). Within the 90s p95 budget.
- **Mild over-run on docs-inside-code-dirs.** A docs-only change to `crates/foo/README.md` classifies as rust-touched. Cargo check is cheap; refine the predicate later if it bothers anyone.
- **Per-language predicate drift risk.** With each `lang/<X>/changed.sh` owned independently, predicates can drift in strictness across languages. Mitigated by `_changed_helpers.sh` (declarative shared primitives) + `_test_changed_predicates.sh` (meta-test).

### Neutral

- **Layer 0 not adopted.** Operations and client preferred dedicated Layer 0 for ergonomics/teachability; protocol (the originator) and test/infrastructure preferred inline. Inline wins on YAGNI grounds; ergonomics covered by `scripts/layer1.sh` doing proto-first explicitly + greppability of layer-script source.
- **CI generator deferred.** Wave 3 candidate; only worth building if we observe duplication between layer scripts and CI workflow logic.
- **TS metric emission policy (Case A vs Case B per observability Round 1)** is out of scope. Tracked separately; see "Out of Scope" below.
- **Reviewer panel grows 6 → 7.** Parallel spawn cost is unchanged; quorum logic already handles N reviewers.

## Implementation Status

| Wave | Component | Status | Devloop | Notes |
|------|-----------|--------|---------|-------|
| 1 | `scripts/layerN.sh` skeletons + `layer-all.sh` orchestrator | ❌ Pending | TBD | Each layer self-reports STATUS = worst child |
| 1 | `scripts/lang/{rust,ts,proto}/{changed.sh, changed.test.sh}` | ❌ Pending | TBD | Mandatory per language |
| 1 | `scripts/lang/_{common,dispatch,changed_helpers,get_base_ref}.sh` + tests | ❌ Pending | TBD | Shared primitives + self-test matrix |
| 1 | `_test_changed_predicates.sh` meta-test | ❌ Pending | TBD | Drift detection across languages |
| 1 | Per-verb dispatchers `scripts/{audit,lint,test,fmt,build}.sh` | ❌ Pending | TBD | Iterate via `_dispatch.sh::for_each_lang_with_verb` |
| 1 | Refactor `scripts/test.sh` body into `lang/rust/test.sh`; preserve root shim | ❌ Pending | TBD | Behavior-equivalence test required |
| 1 | Refactor `scripts/verify-completion.sh` to call `scripts/layer-all.sh` | ❌ Pending | TBD | |
| 1 | `pnpm audit` always-run wired into Layer 6 | ❌ Pending | TBD | Closes minimatch class |
| 1 | Layer A scope-drift parser fix (handle `.ts/.tsx/.svelte/.proto`) | ❌ Pending | TBD | Closes Gate 2 trip from test-utils |
| 2 | `scripts/lang/proto/{compile,format,lint,breaking}.sh` | ❌ Pending | TBD | Lands BEFORE R-61 task #31 |
| 2 | `scripts/lang/ts/{compile,fmt,lint,test,audit,e2e}.sh` (Nx-wrapping) | ❌ Pending | TBD | |
| 2 | TS guards under `scripts/guards/simple/ts/` (6 guards) | ❌ Pending | TBD | client + security paired |
| 3 | SKILL.md Step 6 rewrite (collapse to "run `scripts/layer-all.sh`") + auto-detection patterns + N/A template | ❌ Pending | TBD | Renumber Layer 8 → 7 across docs |
| 3 | `docs/runbooks/devloop-validation.md` | ❌ Pending | TBD | ADR-named deliverable |
| 3 | Semantic-guard relocated from layer to reviewer panel; deduplication step in aggregator | ❌ Pending | TBD | Coexist with code-reviewer |
| 3 | Intentional wire-break override mechanism | ❌ Pending | TBD | After ≥2 real wire-breaking PRs |
| 3 (opt) | CI dispatch generator | ⏸️ Deferred | — | Only if duplication emerges |

**Status values**: ✅ Done | 🚧 In Progress | ❌ Pending | ⏸️ Deferred

### Wave 1 Spin-Out Devloops (lands first; blocks R-61 task #31)

1. **Pipeline scaffolding + classifier + base-ref helper** — `infrastructure` (paired with `operations` for SKILL.md touch + paired with `test` for meta-test design)
   - `scripts/layerN.sh` (1-7) skeletons with `set -euo pipefail` + STATUS aggregation + LAYER summary stderr
   - `scripts/layer-all.sh` orchestrator with `tee /tmp/devloop/layer-N.log` + summary table + 90s p95 budget enforcement
   - `scripts/lang/_{common,dispatch,changed_helpers,get_base_ref}.sh` + `_get_base_ref.test.sh` + `_test_changed_predicates.sh`
   - `scripts/lang/rust/{changed.sh, changed.test.sh}` (other languages land in Wave 2)
   - Per-verb dispatchers (`scripts/{audit,lint,test,fmt,build}.sh`)
   - Refactor `scripts/test.sh` body into `lang/rust/test.sh`; root becomes thin shim
   - Refactor `scripts/verify-completion.sh` to call `scripts/layer-all.sh`
   - Behavior-equivalence test: same exit code on same Rust-only diff before/after refactor

2. **`pnpm audit` always-run** — `infrastructure` (paired with `security`)
   - Wires `pnpm audit --audit-level=high` into `scripts/lang/ts/audit.sh` (TS lang dir created here)
   - Adds `lang/ts/changed.sh` + `changed.test.sh`
   - Updates `scripts/audit.sh` dispatcher (which already iterates) to find the new wrapper
   - Updates `.github/workflows/ci.yml` to call `scripts/layer-all.sh`

3. **Layer A scope-drift parser fix** — `infrastructure` (or `test` if test owns the parser)
   - Handle `.ts/.tsx/.svelte/.proto` paths
   - Fixture-test the regression that tripped 2 of 3 Gate 2 attempts on test-utils

### Wave 2 Spin-Out Devloops (parallel after Wave 1)

4. **Buf wrappers + Layer 1 stage-1 ordering** — `protocol` (paired with `infrastructure`)
   - `scripts/lang/proto/{changed.sh, compile.sh, format.sh, lint.sh, breaking.sh}` (no `test.sh` or `audit.sh` — verb discovery handles absence)
   - Wire proto invocation as first stage of `scripts/layer1.sh`
   - Wire `buf breaking` into `scripts/audit.sh` always-run (via `lang/proto/breaking.sh` invoked unconditionally)
   - **Lands BEFORE R-61 task #31 starts.**

5. **TS wrappers** — `infrastructure`
   - `scripts/lang/ts/{compile,fmt,lint,test,audit,e2e}.sh` invoking Nx natively
   - `lang/ts/e2e.sh` for Playwright (added when task #15 lands)

6. **TS guards under `scripts/guards/simple/ts/`** — `client` (paired with `security` on no-secrets / closed-exports cases; `observability` on no-pii)
   - `no-secrets-in-ts.sh` (security)
   - `no-pii-in-logs-ts.sh` (observability)
   - `no-test-removal-ts.sh` (test)
   - `name-guard-dt-client.sh` (client; R-26)
   - `bundle-content-r14.sh` (client; R-14 — implementer leans toward Vitest contract test over guard, reconsider in this devloop)
   - `exports-map-closed.sh` (security)

### Wave 3 Spin-Out Devloops

7. **SKILL.md Step 6 rewrite + auto-detection + Layer renumber** — `operations`
   - Replace Layer 6 step table with "run `scripts/layer-all.sh`" + Always-Run / Skip-If-Untouched matrix
   - Add `client|svelte|sdk|tsx?` and `proto|buf` to auto-detection patterns
   - Rewrite Layer N/A justification template
   - Renumber Layer 8 → Layer 7 across `SKILL.md`, `ADR-0030`, this ADR, debate doc, specialist-knowledge `INDEX.md` files (devloop output history files left as-is)

8. **`docs/runbooks/devloop-validation.md`** — `operations`
   - Layer-by-layer failure-mode → wrapper-script mapping
   - Exit-code reference and `STATUS=` parsing
   - `_get_base_ref.sh` troubleshooting playbook (the stderr `BASE_REF=...` line is the runbook anchor)

9. **Semantic-guard relocation** — `operations` (owns reviewer-panel composition; paired with whoever currently owns semantic-guard agent definition)
   - Move agent invocation from layer pipeline to Gate 2 reviewer panel
   - Add deduplication step to panel aggregator
   - Coexist with code-reviewer (option a)

10. **Intentional wire-break override mechanism** — `protocol` (paired with `operations`)
    - Decide between per-line ignore comment vs annotated allowlist
    - Land after ≥2 real wire-breaking PRs as case studies

## Out of Scope (Explicitly)

- **`cargo deny`** as a possible superset/replacement for `cargo audit` (license + duplicate-version + advisory). Re-evaluate after ≥4 weeks of operational data on the audit-fatigue trade-off.
- **TS metric emission Case A (consumer only) vs Case B (browser RUM)** — affects whether `validate-metric-coverage.sh` extends to TS. Tracked separately; needs its own ADR or design discussion before the relevant client telemetry tasks land. **Action**: file `docs/TODO.md` entry before this ADR merges.
- **CI dispatch-rules generator.** Defer until duplication between local layer scripts and CI workflow logic is observable.
- **Audit triage workflow** (2-stage triage menu, nightly watchdog, security rotation queue). Status-quo for now per Section 12. Revisit on tripwire breach.

## Alternatives Considered

- **Per-function consolidation** (one `audit-all.sh` runs cargo + pnpm + buf internally). **Rejected** in favor of dispatcher-with-per-language-wrappers, which closes the same registration-fragmentation hole (the dispatcher is the single registration point) while keeping per-toolchain ownership and exit-semantics isolated.

- **No wrappers — invoke native commands inline in SKILL.md and CI.** **Rejected**: forces every Lead and CI workflow to know toolchain trivia; SKILL.md becomes coupled to per-tool flags; CI duplicates dispatch logic that local validation also implements; SKILL.md drift becomes inevitable.

- **Glob-pattern classifier** (`*.rs → rust`, `*.ts → ts`). **Rejected** in favor of directory-based: the directory rule is dramatically smaller, self-documents architectural conventions ("all Rust under `crates/`"), and produces clearer failure modes for new toolchains.

- **Centralized `classify-diff.sh` + `classify-rules.sh`** (registry of file patterns to languages). **Rejected** in favor of per-language `lang/<X>/changed.sh` decentralization: each language owns its own footprint definition; SPOF self-test moves from one place to N localities (same total surface, better ownership); 4th-language ergonomics improve (no centralized registry to edit).

- **YAML classifier rules** (`scripts/dispatch-rules.yaml`). **Rejected**: requires `yq` or Node to parse; bash-sourced rules with helper functions are zero-dependency and CI-portable.

- **Dedicated Layer 0: Contract** for buf gates (proto runs as separate pre-Layer-1 step). **Rejected** in favor of inline-as-first-stage-of-Layer-1: same execution order encoded in `scripts/layer1.sh`, no structural change, fewer layers in the pipeline. Ergonomics covered by greppability of layer-script source.

- **Inclusion-pattern classifier** (dispatcher = "what runs when the diff matches X"). **Rejected** in favor of exclusion-predicate model: default-to-run + skip-only-when-provably-untouched produces a safer failure mode (classifier bugs cause spurious work, never silent skipping) and accommodates non-language directories without special "neutral" registration.

- **`SKILL.md`-prose-driven pipeline definition** (current state — Step 6 enumerates layers in English). **Rejected** in favor of layer-script-driven pipeline: shell is mechanically auditable and testable; SKILL.md becomes "run scripts/layer-all.sh" — drift impossible.

- **Layer 7 spawns `semantic-guard` agent from script.** **Rejected** in favor of relocating semantic-guard to the reviewer panel: keeps layer scripts pure shell with no `claude` CLI runtime dependency; aligns semantic review timing and framework with existing reviewer slots; supports natural deduplication with code-reviewer.

- **Per-toolchain Always-Run audit only on lockfile touch.** **Rejected**: defeats the purpose — the minimatch incident proved transitive vulns surface independent of lockfile diffs in the current branch (they land via Dependabot or sibling devloops merging to main).

## When to Revisit

Trigger an ADR amendment or successor ADR when any of the following occurs:

- **A 4th language is added to the workspace** (Go, Python, Helm, etc.). Confirm the wrapper count is still manageable and `SKILL.md` doesn't need restructure.
- **A new universally-applicable validation tool emerges** (license scanning, SAST, etc.). Confirm classification (always-run vs skip-if-untouched) by the principle in §3 and that the always-run budget can absorb it.
- **CI cost exceeds the 90s p95 wall-clock budget** for the always-run set. Revisit the budget number, the always-run set membership, or both.
- **Mean-time-to-resolution for high-severity audit advisories exceeds 14 days.** Trigger audit-triage workflow design (Section 12 tripwire).
- **Intentional wire-break override mechanism causes friction** (post-R-61 task #31 retrospective).
- **Layer 7 flake rate exceeds 2% weekly.** Quarantine policy is the immediate response; persistent breach signals deeper restructuring.
- **Per-language `changed.sh` predicates drift** in detectable strictness asymmetry (caught by `_test_changed_predicates.sh` meta-test, but worth ADR amendment if structural).

## Implementation Notes

- **Behavior equivalence is non-negotiable for Wave 1.** The dispatcher refactor must produce bit-identical exit codes for any pre-existing Rust-only devloop diff. Test with at least one Rust-only PR replayed before-and-after; commit the test fixture.
- **Wave 2 must precede R-61 task #31.** The proto rename sweep needs `buf breaking` available locally. Without Wave 2 #4, task #31 cannot self-validate its wire-breaking changes.
- **Task #17 is re-scoped, not blocked.** The `ci-client.yml` workflow becomes thin: it calls `scripts/layer-all.sh` and surfaces the structured summary output. The `pnpm audit`, `buf lint`, `buf breaking` invocations move from inline-in-workflow to per-language wrappers — same gates, fewer places.
- **`_get_base_ref.sh` must emit its `BASE_REF=...` line to stderr on every invocation.** The runbook depends on this for "what diff did the validation actually see?" debugging.
- **`scripts/lang/_dispatch.sh::for_each_lang_with_verb`** must lint at startup that every `lang/<X>/` has an executable `changed.sh`. Loud absence beats silent absence.
- **`_test_changed_predicates.sh`** must be invoked from `scripts/layer3.sh` (Guards layer) so meta-test runs on every devloop, not just at PR time.
- **Wave 1 ships `scripts/lang/_get_base_ref.test.sh`** alongside the helper itself. Self-test matrix: local-clean, local-dirty, local-with-untracked, CI-PR, CI-push, first-commit.
- **Failure messages from any `changed.sh` or dispatch helper** must name the unhandled file path and point at the relevant `lang/<X>/changed.sh` so contributors hit a wall with a clear next step.
- **Untracked-file inclusion in local mode** is the difference between "your branch validates correctly before commit" and "your validation lies until you stage everything." Don't drop it.

## Participants

- **security**: Final 78. Accepted dispatcher-as-single-registration-point. Accepted status-quo audit-fatigue as livable trade-off given pnpm-audit-now-always-run. Asked for the 14-day MTTR documentary tripwire (Section 12). Confirmed always-run buf breaking with `merge-base origin/main HEAD` base ref.

- **test**: Round 1: 78 → Round 2: 88 → Round 3: 92. World-state-vs-diff-state principle adopted (Section 3). Asked for explicit base-ref resolution (Section 7), flake-rate budget per tier (Section 14), STATUS aggregation rules in layer scripts (Section 4), shared `_changed_helpers.sh` for declarative predicates (Section 2), `_test_changed_predicates.sh` meta-test (Section 2), untracked-file inclusion in local mode (Section 7), deduplication for semantic-guard + code-reviewer (Section 8).

- **observability**: Final 90. All Round 1 asks granted. Case A/B (TS metric emission) tracked separately as TODO action. `STATUS=` enum lock confirmed (Section 6).

- **operations**: Round 1: 78 → Round 2: 88 → Round 3: 92. Umbrella-with-per-language-siblings pattern adopted (now layer-script-driven). Asked for `scripts/layer-all.sh` orchestrator with budget enforcement (Section 4), `LAYER=N START=...` summary lines (Section 4), `BASE_REF=...` stderr trace (Section 7 normative requirement), dispatcher loud-absence on missing verb files (Section 6). Wanted dedicated Layer 0 (overruled — addressed via greppable `scripts/layer1.sh`).

- **infrastructure**: Round 1: 78 → Round 2: 88 → Round 3: 92. Authored `_get_base_ref.sh` design with GHA-specific guards (Section 7). Conceded centralized `classify-rules.sh` in favor of per-language `changed.sh`. Confirmed `fetch-depth: 0` already in `.github/workflows/ci.yml`. Asked for `STATUS=SKIPPED-NO-VERB` instead of silent continue (Section 6), `_common.sh` for cross-layer state.

- **client**: Final 88. Nx-wrapped-natively granted (Section 9). Conceded always-run buf breaking on externality argument. Will own R-14 bundle-content guard (Wave 2 #6) and reconsider guard-vs-Vitest-contract-test placement.

- **protocol**: Final 88. Conceded Layer 0 elevation as cosmetic; option (b) inline within Layer 1 adopted (Section 5). Will own Wave 2 buf wrappers (#4) and Wave 3 wire-break override mechanism (#10). Confirmed audit-level config ownership belongs to security (Section 11).

## Debate Reference

See: `docs/debates/2026-05-06-polyglot-validation-pipeline-strategy/debate.md`

## References

- ADR-0024 (Agent Teams workflow + cross-boundary classification)
- ADR-0025 (Containerized devloop)
- ADR-0028 (Client architecture, especially §5 supply chain and §7 testing tiers)
- ADR-0030 (Host-side cluster helper, Layer 8 contract — note: Layer numbering will renumber to Layer 7 in Wave 3)
- ADR-0032 (Metric testability)
- `.claude/skills/devloop/SKILL.md` (current pipeline definition; rewrite in Wave 3 #7)
- `docs/devloop-outputs/2026-05-06-test-utils-package/main.md` (the pain inventory that prompted this debate)
- `docs/devloop-outputs/2026-05-06-client-proto-codegen-pipeline/main.md` (task #7 — surfaced 21 latent buf STANDARD findings)
- `docs/user-stories/2026-05-02-browser-client-join.md` (tasks #2, #7, #17, #29, #30, #31)
