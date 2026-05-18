# Debate: Pick One Guard Toolchain — Reframe the Prior Debate's Outcome

**Date**: 2026-05-17
**Status**: In Progress
**Participants**: observability, infrastructure, operations, security, test, code-reviewer, dry-reviewer
**Predecessor**: `docs/debates/2026-05-14-python-guard-pipeline-strategy/` (the converged outcome of that debate — system-only-stdlib Python + 5 meta-guards + 5 tripwires — is the half-measure being reconsidered)

> **Note**: When cross-cutting specialists (Security, Test, Observability, Operations) score < 70 satisfaction at consensus, this requires explicit user risk acceptance — not implicit majority override. See ADR-0024 §5.7.

## Question

**Pick one guard toolchain. Reject the half-measure outcome of the prior debate.**

The 2026-05-14 debate (`docs/debates/2026-05-14-python-guard-pipeline-strategy/`) produced a "system-only stdlib Python + 5 structural meta-guards + tripwire-to-future-uv" hybrid. On user reflection that outcome is structurally incoherent:

- **5 meta-guards police 1 Python lib module** (355 LoC) — toolchain-overhead-to-code ratio is upside down
- **`Approved-Stdlib-Exception:` trailer is the rule's escape hatch**, not its enforcement — first reasonable ask gets approved and the rule becomes a code-review checklist item
- **Python 3.11 build-time pin makes every Debian point-release an ADR amendment** — pinned to a moving target and called it stable
- **"Tripwire to future uv" defers real plumbing** — we pay maintenance cost now for a posture we'll abandon when convenient
- **"Python sometimes" is the WORST pattern for adding new guards** — every author forks on heredoc-vs-lib, bash-vs-python, does-this-break-a-tripwire. Five structural guards beat one obvious template.

This debate **reframes the prior debate's outcome** — the resulting ADR replaces what the prior debate would have produced. Nothing from the prior converged outcome is sacred — including the in-branch deliverables, `scripts/guards/lib/doc_cite_extract.py`, and task #39's work.

## The User's Three Evaluation Criteria

Specialists score on these explicitly (1-10 each):

1. **Consistent implementation pattern** — one mental model for "how guards are written." Show a sketch.
2. **Ease of adding new guards** — count the decisions a new-guard author makes. Compare to the prior debate's outcome.
3. **Reliability of guard issue detection** — positive (catches real bugs) AND negative (does not false-positive on legitimate code).

## Workload Note

This guard pipeline is **largely string/text parsing**: YAML/JSON structure, Rust source code (metric-macro extraction), Markdown (doc-citations), regex patterns. Evaluate candidates against that workload, not generic-program-style. A toolchain that excels at numeric work or async I/O brings no advantage here; a toolchain that excels at structured-text manipulation does.

## Three Anchor Options (Not Exhaustive)

**(α) Full Python.** `uv` + `ruff` + `pip-audit` + `pytest`. Python becomes equivalent to Rust/TS/Proto. All 8 heredoc guards commit to migrating to lib modules. Pay the ~50MB image cost.

**(β) Rust binary `dt-guard`.** New crate at `crates/dt-guard/` using `regex` + `serde_yaml` + `walkdir`. Replaces `doc_cite_extract.py` and the structured-data guards via subcommands (`dt-guard cite-extract`, `dt-guard alert-rules-policy`, etc.). Shell guards become thin wrappers. Python exits the pipeline entirely. Cargo already has clippy + cargo-audit + cargo-test wired in.

**(γ) Pure bash + jq/yq.** Rewrite all 8 Python-using guards. The `resolve_cited_path` symlink-escape gate becomes `readlink -f` + containment check. Single-language guard codebase; no new image surface.

## (δ) Open Slot — Propose a Fourth Option

Specialists may propose a fourth toolchain option (δ) if α/β/γ are all inadequate. Constraints on any (δ):

- Must already be first-class in the repo (has lint + audit + test wired into the validation pipeline) OR carry a compelling argument for adopting it as a new toolchain
- Must score on the same three criteria as α/β/γ
- Must explicitly argue why it beats all three named options on the user's criteria

**One plausible (δ) the prior debate missed: TypeScript guards via the existing pnpm/Node toolchain.** ADR-0033 already wired `pnpm audit`, `eslint`, `tsc --noEmit`, `vitest` into the pipeline. TS is a string/text-parsing language by design. Writing guards as `*.ts` files invoked from the existing shell wrapper inherits the entire TS toolchain at zero new image cost. Cold start (Node ~100ms) is mitigable (batch invocation, or Bun/Deno with ~10ms startup). Specialists are free to develop this option, propose a different (δ), or argue (δ) is unnecessary.

## Required Outputs Per Specialist

1. **Pick α/β/γ/δ.** No hybrids. No "α-with-some-bash." No "β-with-fallback-Python." No carve-outs, no second-tier framings, no tripwires-to-future-X. Pick one toolchain and own it.
2. **Explicit scores on the three criteria (1-10 each)**, with reasoning per axis.
3. **Argue against the prior debate's outcome**: why is your option strictly better on the user's three criteria?
4. **Migration plan**: which of the prior debate's in-branch deliverables get deleted, kept, added?
5. **6-month failure mode of YOUR option specifically** — what makes your option fail by 2026-11?

## Process Constraints

- The Lead (me) commits to **NOT driving fast convergence at the cost of outcome quality.** If positions stall, surface incoherence to the user rather than manufacturing compromise.
- **The successor ADR is the single ADR of record** for the guard-pipeline language strategy (the prior debate's draft is being replaced, not chained).
- "Maybe we just keep the prior debate's outcome" is out of scope — sunk cost rejected.
- Half-measures, carve-outs, second-tier framings, tripwires-to-future-X, "system-only with exception" patterns: **all rejected by construction.** If you find yourself proposing one, you've misread the prompt.

## Positions

### Round 1 — Initial Positions

| Specialist | Pick | Consistency | Ease | Reliability | Sat | WOULD_ACCEPT |
|------------|------|-------------|------|-------------|-----|--------------|
| infrastructure | **β** Rust dt-guard | 9 | 8 | 9 | 86 | yes |
| security | **β** Rust dt-guard | 9 | 7 | 10 | 84 | yes |
| operations | **β** Rust dt-guard | 9 | 9 | 9 | 86 | yes |
| dry-reviewer | **β** Rust dt-guard | 10 | 6 | 9 | 84 | yes |
| observability | **α** Full Python | 9 | 9 | 8 | 86 | yes |
| test | **α** Full Python | 8 | 9 | 8 | 82 | yes |
| code-reviewer | **δ** TypeScript | 9 | 8 | 8 | 84 | yes |

**Tally: 4β / 2α / 1δ. Average sat ~85.** All WOULD_ACCEPT=yes within their own pick — but **no shared pick** has consensus. This is the opposite of the prior debate's fast-converging shape; specialists are holding principled positions.

### Round 1 — Key Technical Tensions

**T-1: Negative lookbehind in three of eleven `doc_cite_extract.py` patterns** (test). `_PATH_PREFIX` at line 39 + its consumers `BARE_LINE_CITE_RE`/`SYMBOL_CITE_RE` use `(?<![\w./-])`. Rust `regex` rejects lookaround by construction (that's the DFA guarantee). Under β: either (a) restructure patterns (re-derives the extractor — correctness risk on URL false-positive class), or (b) use `fancy-regex` crate (backtracking — forfeits β's headline ReDoS-free argument). This is a **load-bearing factual challenge to β's reliability claim**.

**T-2: Existing working code vs. greenfield rewrite** (observability). ~2000 LoC of Python kernels exist and work today. β rewrites them. Real one-time porting risk vs. ongoing maintenance lower-bound.

**T-3: Structural-duplication: policed vs. structurally impossible** (dry-reviewer). α must use meta-guards to police regex re-inlining (today's failure mode: `LAZY_REASON_RE` re-inlined verbatim at 2 sites). β makes re-inlining a compile-time error by construction. The prior debate's anti-drift machinery exists *because* α can't enforce its own invariants.

**T-4: Structural typing for YAML shapes** (code-reviewer). `tsc --strict` + `no-explicit-any` enforces typed YAML/JSON deserialization at compile time. ruff (α) doesn't give this without mypy bolt-on. Rust `serde_yaml` (β) gives it via typed structs — possibly stronger than TS. Question: does β's typed-deser address code-reviewer's δ rationale?

**T-5: Test ergonomics on the 7-case symlink-escape surface** (test). pytest `parametrize` + `monkeypatch` + `tmp_path` beats unittest. The OSError-mock case (line 245 branch the prior debate's implementation notes flagged) needs `monkeypatch.setattr` cleanly under α; under β requires trait injection. Real ergonomic delta.

## Discussion

Round 1 broadcasts captured in team message log. Round 2 will engage T-1 through T-5 directly.

## Consensus

**Reached: Round 3 (2026-05-18). Unanimous β (Rust dt-guard).** All 7 specialists at WOULD_ACCEPT=yes. Average satisfaction 84.3.

### Final Satisfaction Table

| Specialist | R1 | R2 | R3 | Final | Pick | Trajectory |
|------------|----|----|----|-------|------|------------|
| infrastructure | 86 | **88** | — | 88 | β | held; port test came in 1.03× on `find_qualifying_expr_window` |
| security | 84 | **82** | — | 82 | β | held; booked lookbehind port cost honestly (ease 7→6) |
| operations | 86 | — | — | 86 | β | held R1 |
| dry-reviewer | 84 | — | — | 84 | β | held R1 |
| code-reviewer | 84 (δ) | **87 (β)** | — | 87 | β | **flipped δ→β** after `serde_yaml` typed-enums + `deny_unknown_fields` shown strictly stronger than `tsc --strict` |
| test | 82 (α) | 74 (α) | **81 (β)** | 81 | β | **flipped α→β** after security's lookbehind port + OSError-graceful case verified strictly stronger under Rust |
| observability | 86 (α) | 78 (α) | **82 (β)** | 82 | β | **flipped α→β** after infrastructure's port test landed 1.03× on her named falsifiable function |

Cross-cutting specialists at consensus: security 82, test 81, observability 82, operations 86 — all above ADR-0024 §5.7 70-floor.

### Key Convergence Events

1. **Code-reviewer's δ→β flip (R2)** — driven by *serde_yaml typed-deserialization with `deny_unknown_fields` is strictly stronger than `tsc --strict`* (no `any` escape; exhaustive `match` natively; same toolchain as 5 existing service crates). Code-reviewer explicitly noted their own first criterion (consistency) cuts against δ once the repo-language-count consequence is honest: δ adds TS to the guard pipeline; β reuses Rust which is already there at scale.

2. **Security's port sketch (R2)** — concrete ~15 LoC Rust restructuring of `_PATH_PREFIX`'s negative lookbehind using positive left-boundary class `(?:^|[\s\(\[\{`'"<>])`. Behavioral-equivalence walkthrough on `gc-service.dark-tower.svc.cluster.local:5432`. ~5 LoC caller-side adjustment. Met test's R2 self-stated flip condition.

3. **Test's R3 verification + flip** — ran security's restructuring against the production corpus (`docs/runbooks` + `.claude/skills`) on a 17-case fixture: 13/17 byte-identical, 4/17 diverge (`,foo.sh:42`, `;foo.sh:42`, `=foo.sh:42`, `|foo.sh:42`), **zero present-tense instances of any divergence in production**. Suggests 5-char boundary class expansion lands with port → divergence to 0/17. Test explicitly retracted R1 "pytest mock best-in-class" framing on OSError-graceful: Rust `canonicalize` errors naturally on dangling symlinks — strictly stronger durability than `unittest.mock.patch` mocking a CPython implementation detail.

4. **Infrastructure's port (R2)** — 35 LoC Python → 36 LoC Rust on `find_qualifying_expr_window` = **1.03× ratio** on the function observability specifically named as her falsifiable test. Walking through mechanism: Rust type system collapses Python defensive branches (`isinstance` check, `if x is None` patterns via let-else, two-branch min via `map_or`). Revised aggregate estimate 1.1-1.3× (down from observability's R2 1.5-2×). Total Rust LoC ~1050-1500, not 3900-5200.

5. **Incremental migration is viable** (infrastructure R2): 8 PRs over 2-3 weeks calendar time, each independently revertable. Working end-state on day 2 (cite-extract subcommand). ADR-0033 Layer 4 (`cargo test`) and Layer 6 (`cargo audit`) gate each PR from day 1.

6. **Observability's R3 flip** — explicit honesty: *"R1→R2→R3 trajectory on my LoC estimate: 3× → 1.5-2× → 1.1-1.3×. I was wrong, progressively less wrong, and now the right number is small enough that the rewrite-cost argument doesn't carry α."* Refused to retreat to R2 alternative grounds as "face-saving, not honest."

### What Made This Different From the Prior Debate

- **The Lead committed publicly to NOT optimizing for fast convergence.** Every prompt explicitly invited specialists to hold ground.
- **Every flip was driven by the flipping specialist's own pre-stated falsification condition.** Test named "show <20 LoC of Rust with passing fixture"; security delivered. Observability named "port `find_qualifying_expr_window` and show LoC ratio"; infrastructure delivered. Code-reviewer's flip was triggered by an honest comparison the Lead surfaced (serde_yaml vs tsc).
- **Honest concessions in both directions**: security ease 7→6 (port cost), test pytest-mock framing retracted, observability LoC estimate retracted across two rounds, code-reviewer's "decision count" inflation in R1.
- **No half-measures, carve-outs, or tripwires-to-future-X** in the final convergent position. The Lead's pledge held.

## Decision

**ADR-0034 (Accepted)**: `docs/decisions/adr-0034-guard-pipeline-as-rust-binary.md`. (This ADR replaces the draft the 2026-05-14 predecessor debate would have produced; the two debate records are preserved as the historical context, but there is only one ADR of record for the guard-pipeline language strategy.)

**Migration shape**: 8 PRs over 2-3 weeks calendar time, each independently revertable, with `cargo test` + `cargo audit` gating per PR. Day 2 lands `dt-guard cite-extract` and 3 doc-citation guards flip wrappers; Day 11 removes `python3-yaml` apt from devloop image.

Cross-reference artifact: `docs/debates/2026-05-14-python-guard-pipeline-strategy/guard-vendor-coverage-matrix.md` remains canonical for (D)-complement slate (D-1 promtool + D-2 Grafana provisioning dry-run).
