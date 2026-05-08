# Debate: Polyglot Validation Pipeline Strategy

**Date**: 2026-05-06
**Status**: Complete — ADR-0033 drafted 2026-05-07
**Participants**: security, test, observability, operations, infrastructure, client, protocol

> **Note**: When cross-cutting specialists (Security, Test, Observability, Operations) score < 70 satisfaction at consensus, this requires explicit user risk acceptance — not implicit majority override. See ADR-0024 §5.7.

## Question

How should the devloop validation pipeline (Layers 1-8) and the supporting `scripts/guards/` tree extend to a polyglot workspace (Rust + TypeScript + Protobuf)?

Specifically: (a) the contract between SKILL.md, run-guards.sh, scripts/test.sh, scripts/verify-completion.sh, and CI workflows; (b) how multi-toolchain diffs are dispatched; (c) which validation steps run universally vs diff-conditionally and what principle decides; (d) how the design accommodates a 4th/5th toolchain without re-architecture; (e) the migration path from the current Rust-only state.

## Context

The devloop validation pipeline (Layers 1-8) and the guards in scripts/guards/ were designed for Rust. We now have three toolchains: Rust (cargo), TypeScript (tsc/prettier/eslint/pnpm/vitest/vite), and Protobuf (buf).

We just shipped the first TypeScript package (packages/test-utils/, R-39) and the implementer + Lead had to invent ad-hoc TS-equivalents and mark cargo-only layers N/A. Concrete observed gaps: pnpm audit not in pipeline (3 high vulns latent until TS task surfaced them), Layer A scope-drift parser tripped 2 of 3 times on TS paths, no TS guards exist, buf not wired anywhere, SKILL.md auto-detection is Rust-shaped, scripts are Rust-only or Rust-prioritized.

## Required Outputs Per Specialist

1. **Architectural proposal** including wrapper-script taxonomy (per-function dispatch, per-language, no wrappers, or other)
2. **Always-run vs diff-conditional** position for: cargo audit, pnpm audit, buf lint, buf breaking, cargo check (no Rust changes), tsc --noEmit (no TS changes) — plus the principle that classifies these AND obvious cases (cargo test, clippy, eslint, prettier, vitest, fmt)
3. **Migration path** — rollout shape, what lands first/second/third, which scripts get renamed/refactored vs left alone vs deleted
4. **Failure mode under their own proposal** — what they'd watch for at 6-month review

## Positions

### Round 1 — Initial Positions

| Specialist | Position Summary | Satisfaction | WOULD_ACCEPT |
|------------|------------------|--------------|--------------|
| observability | Defer to test/ops on shape; narrow asks: pnpm audit always-run, PII/secret guards extended to TS globs, explicit decision on whether TS emits metrics. Principle: cost-to-run + cross-boundary failure propagation. | 82 | yes |
| security | Per-function consolidation (`audit-all.sh`) — single registration point closes the minimatch-class registration-fragmentation hole. Always-run audit principle: external event with no diff. 2-stage triage (in-script + nightly watchdog) for audit fatigue. | 80 | yes (conditional on Stage 2 watchdog) |
| test | Function-named pipeline scripts (`pipeline/{compile,lint,test-unit,audit-deps}.sh`) dispatching to per-toolchain modules under `scripts/toolchains/<lang>/`. Principle: world-state vs diff-state. Layer A parser fix in Wave 1. | 78 | yes |
| operations | Umbrella scripts (`audit.sh` etc) with per-language siblings (`audit-rust.sh`/`audit-ts.sh`); diff-routed at entrypoint, per-language under hood. `scripts/dispatch-rules.yaml` as single source of truth for router-drift mitigation. CI-minutes default: diff-gate. | 78 | yes (conditional on dispatch-rules.yaml acknowledged) |
| infrastructure | Per-language wrapper directories (`scripts/lang/{rust,ts,proto}/`) driven by `classify-diff.sh` + `lang-rules.yaml` registry; bash dispatcher; CI splits per workflow not per matrix. Behavior-equivalence test for refactor non-negotiable. | 78 | yes |
| protocol | Per-language wrappers + buf earns dedicated **Layer 0: Contract** (Rust + TS both consume codegen; surface root cause in proto's vocabulary). buf breaking strongly always-run. Audit-level config owned by security not infrastructure. | 78 | yes |
| client | Per-language wrappers; Nx (`nx affected`) wrapped natively inside `lang/ts/*.sh`. R-14 bundle-content guard + exports-map-closed are TS-novel. Open-system vs closed-system principle. | 78 | yes (4 conditions noted) |

### Strong Consensus After Round 1

1. **Per-language wrappers under thin dispatchers** — 6 of 7. Security's per-function concern is satisfied by the dispatcher pattern (dispatcher = single registration point).
2. **Always-run cargo audit + pnpm audit** — universal.
3. **Classifying principle** — synthesis: *Always-run if failure can be triggered by external state change with no diff (advisory DBs, contract evolution against main, source-of-truth integrity); diff-gated if failure requires source change in the toolchain.*
4. **Single diff classifier as load-bearing primitive** — operations + test + infrastructure converged.
5. **Migration in 3–5 devloops, Wave 1 = pnpm audit + dispatcher skeleton + Layer A parser fix.**

### User-Locked Decisions (post-Round 1)

| Decision | Resolution |
|----------|-----------|
| Per-language wrappers vs per-function | **Per-language under dispatchers** (dispatcher satisfies security's registry concern) |
| buf breaking always-run vs diff-gated | **Always-run** (~500ms; catches main-rebase wire-break) |
| Layer 0: Contract — dedicated layer or first step within Layer 1 | **Round 2** — user notes layers run serially anyway; structural elevation may be cosmetic |
| Audit fatigue mitigation (2-stage triage / warn-don't-block) | **Status quo** — always-run, hard-block. Trade-off accepted: devs continue to ignore "I didn't introduce it" findings. |
| N/A semantics anti-vacuous-green | Dispatcher computes diff once; wrappers return enumerated `OK / FAIL / SKIPPED-NO-DIFF / N/A-with-reason`; unknown files = fail loud |
| Nx integration | **Wrap, don't replace** — `lang/ts/*.sh` calls `nx affected -t <target>` natively |

### Round 2 — Final Positions

| Specialist | Δ | Final | WOULD_ACCEPT | Notable shift |
|------------|---|-------|--------------|---------------|
| security | 80→78 | 78 | yes | Accepted dispatcher pattern + status-quo audit fatigue as livable trade-off; asks for one-line documentary tripwire ("revisit if MTTR > 14 days") |
| test | 78→88 | 88 | yes | Locked design preserves world-state principle + N/A mitigation; one-sentence flake budget per tier requested |
| observability | 82→90 | 90 | yes | All Round 1 asks granted; Case A/B (TS metric emission) tracked separately |
| operations | 78→88 | 88 | yes | Wants `dispatch-rules.yaml` SoT, 90s p95 wall-clock budget, runbook as ADR-named deliverable |
| infrastructure | 78→88 | 88 | yes | Accepted always-run buf breaking; classifier as bash artifact (NOT YAML — zero-deps, CI-portable); CI generator deferred to Wave 2 |
| client | 78→88 | 88 | yes | Accepted always-run buf breaking on externality argument |
| protocol | 78→88 | 88 | yes | **Conceded Layer 0 — option (b) inline within Layer 1 captures ~90% of value**; defer override mechanism + cargo-deny to Wave 3 |

**Average: 86.9.** Security is the only specialist below 90 (78), explicitly stating "I do not block consensus." No cross-cutting specialist below 70 → no user risk acceptance required per ADR-0024 §5.7.

### Resolved Open Questions

| Question | Resolution |
|----------|-----------|
| **Layer 0 dedicated vs inline within Layer 1** | **Inline (b)** — protocol (originator) conceded; test + infrastructure agree. Operations + client wanted dedicated for ergonomics/teachability — addressed via SKILL.md ordering assertion + `compile.sh` header comment per protocol's own suggestion. |
| **Lockfile classifier artifact format** | **Bash sourced file** (`scripts/lang/classify-rules.sh`) — infrastructure as implementer prefers zero-deps over YAML. Operations' SoT concern satisfied because dispatchers are the single registration point and CI calls dispatchers. |
| **Base ref for `nx affected` and `buf breaking`** | **`merge-base origin/main HEAD`** — explicit in ADR (security + test) |
| **Wrapper exit semantics** | **Specified**: exit 0 = OK/SKIPPED-NO-DIFF/N/A, exit 1 = FAIL, exit 2 = wrapper bug; status line `STATUS=<enum> REASON=<short>` on final stdout (operations + observability) |
| **Wall-clock budget** | **90s p95 on CI**, ADR amendment to revise with justification (operations) |
| **Audit fatigue ADR mitigation** | **Documentary tripwire only**: "Revisit audit triage workflow if MTTR for high-severity advisories exceeds 14 days" (security ask) |
| **`docs/runbooks/devloop-validation.md`** | **ADR-named deliverable** with acceptance criteria (operations) |
| **Audit-level configuration ownership** | **Security**, not infrastructure — explicit in ADR (protocol) |
| **Intentional wire-break override** | **Requirement only**: "must be explicit in-tree, no CI bypass flags". Mechanism (comment vs ratchet list) deferred to Wave 3 (protocol) |
| **`cargo deny`** | **Out of scope**; follow-up: "evaluate after ≥4 weeks audit-fatigue operational data" (protocol) |
| **Flake-rate per tier** | **Zero-flake target Layers 1-6, <2% rerun rate Layers 7-8** weekly (test) |
| **TS metric emission Case A/B** | **Tracked separately** — TODO entry or stub ADR before this one merges (observability) |
| **Failure message format** | **Names unclassified path + rules artifact location** (infrastructure) |

### Wave 1 Scope (locked)

1. `scripts/{audit,lint,test,fmt,build}.sh` dispatchers + `scripts/lang/{rust,ts,proto}/` skeleton
2. `scripts/lang/classify-diff.sh` + `scripts/lang/classify-rules.sh` + self-test fixtures
3. `pnpm audit` always-run wired into Layer 6
4. Layer A scope-drift parser fix (handle `.ts/.tsx/.svelte/.proto` paths)
5. Behavior-equivalence test for the dispatcher refactor (Rust-side parity check)
6. Land **before** R-61 task #31 (proto rename sweep) so buf breaking gates the rename work

## Consensus

**Reached** at Round 2. Final score average 86.9 (range 78-90); all specialists `WOULD_ACCEPT_CURRENT=yes`; no cross-cutting dissent below 70.

### Round 3 — Architecture Refinement

After ADR-0033 was drafted, user surfaced four architectural improvements warranting Round 3 with `test`, `infrastructure`, `operations` (the three with closest stake in pipeline-script architecture):

| Specialist | R2 → R3 | Final |
|------------|---------|-------|
| test | 88 → **92** | yes |
| infrastructure | 88 → **92** | yes |
| operations | 88 → **92** | yes |

Unanimous yes on all four refinements (described below); ADR-0033 revised in place to fold them in.

**Refinements adopted:**

1. **Layer-script architecture**: `scripts/layerN.sh` (1-7) + `scripts/layer-all.sh` orchestrator. SKILL.md collapses from per-layer prose to "run `scripts/layer-all.sh`". Each `layerN.sh` self-reports STATUS = worst-child STATUS, streams child STATUS lines verbatim, emits `LAYER=N START=<ts> END=<ts> RESULT=<enum>` to stderr.

2. **Verb discovery via convention**: each `lang/<X>/` ships `changed.sh` (mandatory, lint at dispatcher startup) + whichever verb scripts make sense. Dispatchers emit `STATUS=SKIPPED-NO-VERB` for missing verb files, never silently continue. Shared `_changed_helpers.sh` keeps each `changed.sh` 3-5 declarative lines. Meta-test `_test_changed_predicates.sh` catches drift between languages. Replaces centralized `classify-diff.sh`/`classify-rules.sh` (decentralization wins on locality + 4th-language ergonomics).

3. **Diff-base resolution via `_get_base_ref.sh`**: env-aware (local vs CI-PR vs CI-push); local mode includes untracked files; CI uses three-dot diff against `$GITHUB_BASE_REF`; defensive `git fetch` for sparse-checkout safety; first-commit edge case guarded; **normative requirement** to emit `BASE_REF=<sha> BASE_SOURCE=<...> DIFF_MODE=<two|three-dot> FILES_CHANGED=<count>` to stderr every invocation (3am runbook anchor).

4. **Semantic-guard relocation**: moves from Layer 7 (pipeline) to reviewer panel (Gate 2). Keeps layer scripts pure shell with no `claude` CLI runtime dependency. Coexists with code-reviewer (option a — distinct lenses; specific-pattern detection vs general code review). Reviewer panel grows 6 → 7 with deduplication step in aggregator. Layer 8 (env-tests) renumbers to Layer 7.

### Post-Round-2 Refinement

After Round 2 closed, user proposed simplifying the classifier from glob-pattern matching to **directory-based exclusion predicates**: rather than "what runs when the diff matches X", the classifier answers "is the diff provably absent of changes to language Y" (exclusion predicates), and the pipeline defaults to running everything except the language-specific layers we can prove are untouched.

This reframing was adopted into ADR-0033 because:
- It's strictly more conservative (more things run, never fewer; classifier bugs cause spurious work, not silent skipping)
- It accommodates non-language directories (`infra/`, `docs/`, `scripts/`) without "neutral" registration — they just don't trigger any skip optimization
- It strengthens, not weakens, every specialist's locked position (test's world-state principle becomes the default; security's concern about silent gate-bypass disappears)
- It collapses the YAML-vs-bash debate (3 directory prefixes + ~9 root files = ~12 lines of bash; nobody argues for YAML)

No Round 3 needed — refinement was strictly conservative and addressed independently with the user.

## Decision

**ADR-0033 — Polyglot Validation Pipeline Strategy** drafted 2026-05-07. See `docs/decisions/adr-0033-polyglot-validation-pipeline.md`.
