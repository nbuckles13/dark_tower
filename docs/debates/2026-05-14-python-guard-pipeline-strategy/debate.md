# Debate: Python in the Guard Pipeline — Formalize, Consolidate, Replace, or Back Away?

> *Historical record. The converged outcome of this debate (system-only-stdlib Python + 5 meta-guards + 5 tripwires) was reconsidered on user reflection and reframed in `docs/debates/2026-05-17-guard-toolchain-supersede/`. The single ADR of record is `docs/decisions/adr-0034-guard-pipeline-as-rust-binary.md`. The vendor-coverage matrix produced here remains canonical.*

**Date**: 2026-05-14
**Status**: Complete (outcome reframed — see note above)
**Participants**: observability, infrastructure, operations, security, test, code-reviewer, dry-reviewer

> **Note**: When cross-cutting specialists (Security, Test, Observability, Operations) score < 70 satisfaction at consensus, this requires explicit user risk acceptance — not implicit majority override. See ADR-0024 §5.7.

## Question

How should the `scripts/guards/` pipeline handle the fact that **Python is a de-facto dependency** (six pre-existing heredoc guards, plus the new committed `scripts/guards/lib/doc_cite_extract.py` consumed by three guards) **but is not a formalized toolchain** (no requirements file, no version pin in shell guards, no lint guard, no supply-chain audit, no import convention)?

This debate must produce **two artifacts**:

1. **Target state** — which of the four options below (or a hybrid) we converge on, with explicit reviewer endorsement.
2. **Migration path** — concrete ordered steps from today: what lands in this branch, what spins out as follow-up tasks, what breaking changes are accepted along the way.

## Context

### Discovery (2026-05-14, mid task #39 follow-up)

Six simple guards under `scripts/guards/simple/` already use **inline Python heredocs** — pre-existing, multi-month convention:

- `validate-alert-rules.sh` (refactored in task #39-followup to consume the lib module)
- `validate-infrastructure-metrics.sh` (also depends on `python3-yaml` apt package)
- `validate-dashboard-panels.sh`
- `validate-application-metrics.sh`
- `validate-metric-labels.sh`
- `grafana-datasources.sh`

Task #39-followup added the **first committed `.py` module** at `scripts/guards/lib/doc_cite_extract.py`, consumed via `sys.path.insert` by three guards (Guard A `validate-doc-citations-no-line-numbers.sh`, Guard C `validate-doc-citations-symbol-resolves.sh`, and `validate-alert-rules.sh`).

So at HEAD on `feature/python-guard-debate`: **8 of ~25 guards** under `scripts/guards/simple/` invoke `python3`. Six via heredoc, three via the lib module (one of which — `validate-alert-rules.sh` — appears in both lists because its heredoc was refactored to import from the lib).

### Current Python footprint

- **`python3` binary**: in devloop image (Debian bookworm system package, **3.11** — confirmed `infra/devloop/Dockerfile`)
- **`python3-yaml`**: apt-installed in image, relied on by `validate-infrastructure-metrics.sh`
- **No `requirements.txt`, `pyproject.toml`, `setup.py`, or `Pipfile`**
- **No interpreter version pin** in shell guards (`#!/usr/bin/env python3` only; `python3.10` and `python3.11` both pass)
- **No Python lint/type-check guard** (cf. `cargo clippy`, `eslint`, `buf lint`, `cargo fmt`)
- **No documented import convention**; `sys.path.insert` is the de-facto pattern at the three lib-consuming guards
- **No Python supply-chain audit** (no analogue of `cargo audit` / `pnpm audit`)
- **`.gitignore` patched** (`__pycache__/`, `*.pyc`) — but no rationale comment

### Relationship to ADR-0033 (2026-05-07)

ADR-0033 ("Polyglot Validation Pipeline Strategy") established the per-language wrapper convention `scripts/lang/<X>/` (Rust, TS, Proto). It explicitly notes a 4th language addition (Go, Python, Helm, etc.) as a "When to Revisit" trigger. This debate is **that revisit conversation forced by reality** — Python is already in-tree as a guard implementation language; the question is whether to recognize it as a formal toolchain under the ADR-0033 convention or to drive it out.

### What this debate is NOT

- **Out of scope**: rewriting individual guard logic, adding new guards.
- **Not a referendum on Python the language.** Multiple options keep Python; the disagreement is about the toolchain plumbing surrounding it.

## Four End States (Hybrids Encouraged)

**(A) Standardize on Python for all complex guards.** Write a proper `scripts/guards/lib/` package, add a Python lint guard (ruff or similar) + interpreter version pin in a shared shell helper + supply-chain audit. Rewrite simple bash guards where Python is cleaner. **Reviewers must name a preferred dependency-management stack — `uv` / `pip+venv` / system-only — and defend the pick.** (None is correct by default; this is the toolchain-pin specificity the ADR-0033 debate forced for Rust/TS.)

**(B) Standardize on bash for all guards.** Rewrite the 6 existing heredoc guards in pure bash (plus `jq` / `yq` for structured data). Revert task #39-followup's Python module to bash. Highest consistency cost (rewrite work + loss of expressiveness) but eliminates the toolchain gap entirely.

**(C) Polyglot, formalized.** Keep both languages with a **written contract** for when to choose each (e.g., "structured data → Python; pattern matching → bash"). Add Python toolchain plumbing — preferred dep stack (`uv` / `pip+venv` / system-only, named explicitly), lint guard, version pin, supply-chain audit, import convention.

**(D) Shrink the surface area first.** Replace Python-driven guards with **vendor-native validators** where they exist — `promtool check rules` for alert rules, Grafana provisioning dry-run for dashboards, `yq` schema-validate for the rest of the structured-YAML cases. **Decide language strategy *after* the surface area shrinks.** This is a language-orthogonal axis A/B/C don't surface; reviewers should endorse or reject it explicitly rather than let it derail mid-debate.

## Required Outputs Per Specialist

1. **Initial position** on which of A/B/C/D (or hybrid) you favor, with explicit reasoning grounded in your domain.
2. **Take an explicit stance on (D)** — endorse, reject, or sequence-before-others.
3. **If you favor (A) or (C)**: name a preferred dependency-management stack (`uv` / `pip+venv` / system-only) and defend.
4. **Migration path proposal** — what lands in this branch on `feature/python-guard-debate`, what spins out as follow-up.
5. **Failure mode you'd watch for at 6-month review** under your proposal.

## Positions

### Round 1 — Initial Positions

| Specialist | Position | Dep stack | (D) stance | Sat | WOULD_ACCEPT |
|------------|----------|-----------|------------|-----|--------------|
| observability | C with partial-D as complement | uv (system-only fallback) | "(D) doesn't shrink enough" — only `grafana-datasources` of her 6 is meaningfully replaceable; the other 5 encode Dark-Tower-specific policy (ADR-0011/0029/0031) no upstream tool covers | 78 | yes (conditional on supply-chain audit follow-up being non-deferrable) |
| infrastructure | D-then-minimal-C | uv | Endorse, sequence-first | 72 | no (needs D-feasibility confirmation from obs/ops before locking) |
| operations | D→C sequenced hybrid | uv | Endorse, sequence parallel-or-first acceptable | 82 | yes (would accept C alone at 85+ or hybrid at 90+ if D slate named explicitly) |
| security | D-first → C-narrow | system-only | Endorse, sequence-first | 55 | no (debate pre-consensus) |
| test | D-first → C-lite | system-only | Endorse, sequence-first | 65 | no (no convergent proposal yet) |
| code-reviewer | D-then-C hybrid (glue-language carve-out) | system-only | Endorse, sequence-first | 55 | no (debate is Round 1) |
| dry-reviewer | D-first-then-A | uv | Endorse, sequence-first (strongest endorsement; deletion > consolidation) | 70 | yes |

**Round 1 average satisfaction: 68.1.** Security and code-reviewer at 55 (lowest); operations at 82 (highest).

### Convergences from Round 1

1. **(D) endorsed by all 7.** Sequence-first by 6 of 7; observability says "complement, not replacement" — partial endorsement.
2. **Pure (B) rejected by all 7.** dry-reviewer's analysis was crisp: rewriting Python heredocs to bash + jq/yq *creates* duplication while claiming to eliminate it (~18 new copies of structured-data parsing across 6 guards).
3. **Pure (A) rejected by all 7.** Code-reviewer: "over-investment." Operations/dry-reviewer want (A) only on the post-(D) residual.
4. **Some Python plumbing accepted for the residual** by all 7 (the question is how much).
5. **`scripts/guards/lib/doc_cite_extract.py` zero-unit-test gap is independently load-bearing.** Both test and security surfaced this — symlink-escape gate (`resolve_cited_path`) is security-sensitive and currently has no in-tree test coverage. Test wants pytest backfill IN-BRANCH regardless of A/B/C/D outcome.

### Disagreements requiring Round 2 resolution

**Disagreement 1 — (D)'s actual leverage:**
- observability (owns 5 of 6 heredoc guards): "(D) shrinks at most 1 of 6 (`grafana-datasources`). The rest encode bespoke policy." Inventory was specific per guard with vendor-tool coverage gaps named.
- dry-reviewer (estimating from source): "promtool deletes ~489 LoC of `validate-alert-rules.sh`; yq schema-validate replaces parts of `validate-infrastructure-metrics`; Grafana dry-run shrinks `validate-dashboard-panels`. Net −2 to −4 whole guards."
- infrastructure/operations: implicitly assume 3-4 shrink.
- **Resolution path**: observability + dry-reviewer must engage directly. dry-reviewer's claim is "delete what vendor covers, keep what it doesn't" (i.e., not full-replacement). Observability's claim is "the bespoke part *is* the guard." If they're both right, the post-(D) residual is "thin wrappers around vendor tools that add the bespoke layer" — that's worth knowing before locking strategy.

**Disagreement 2 — dep stack (4-3 split):**
- `uv`: infrastructure, observability, operations, dry-reviewer (the "we want ruff + pip-audit" camp)
- `system-only`: security, test, code-reviewer (the "our Python is stdlib-only today; importing uv to lint stdlib code is net-negative supply chain" camp)
- **Resolution path**: factual question — does the post-(D) Python residual need third-party deps (ruff, pip-audit) badly enough to justify uv? If the residual is small (security + test's framing — `doc_cite_extract.py` plus 1-2 guards), system-only suffices. If the residual is larger and growing, uv earns its keep.

**Disagreement 3 — in-branch scope** (varies from "ADR + 1 file" to "full lang/python/ scaffold + Dockerfile change"):
- Smallest landing: infrastructure ("ADR-0034 + .gitignore rationale comment only")
- Largest landing: operations (`scripts/lang/python/` wrappers + pyproject.toml + Dockerfile uv install + ruff baseline + ADR-0033 amendment)
- **Resolution path**: this debate question is "language strategy + toolchain plumbing + migration ordering" — not "implement it now." Most plumbing should spin out as ordered follow-ups; this branch should land the *decision* + the *one or two pieces that are independently load-bearing* (e.g., `doc_cite_extract.py` pytest backfill if security calls it veto-blocking; `.gitignore` rationale comment).

## Discussion

### Round 1

All 7 specialists broadcast initial positions to peers + team-lead via SendMessage on 2026-05-14. Full position documents preserved in team message log.

### Round 2 — Cross-Talk & Convergence

**Load-bearing artifact landed**: `docs/debates/2026-05-14-python-guard-pipeline-strategy/guard-vendor-coverage-matrix.md` (observability, 2026-05-14). Per-guard LoC accounting against vendor-native tools. Headline: of 6 Python-using guards, only `grafana-datasources.sh` is a real (D) target (~60% LoC deletable via Grafana provisioning dry-run); the other 5 average ~1% deletable. The "(D) shrinks the surface meaningfully" premise of R1 collapses; (D) is **complement, not replacement**.

**Round 2 satisfaction table**:

| Specialist | R1 | R2 | Shift |
|------------|----|----|-------|
| observability | 78 | 85 | C-with-uv + D-as-complement (bundle promtool + Grafana-provisioning-add in-branch). Wrote the coverage matrix. |
| infrastructure | 72 | 85 | **Flipped to system-only + tripwires.** Cited cross-cutting plurality + zero-pip-today audit. |
| operations | 82 | 88 | **Flipped to system-only-plus-future-uv** with explicit promotion criteria (first PyPI dep \| 3rd .py module \| CI-vs-devloop drift). Named D-slate: D-1 promtool-parallel-gate + D-2 grafana-provisioning-UID-half + acceptance criteria. Drops yq sweep + dashboard-panels-D. |
| security | 55 | 62 | Held system-only firm. **Veto-blocking**: `resolve_cited_path` pytest backfill in-branch. uv-flip Schelling point: "first PyPI dep proposal" not LoC/guard-count. |
| test | 65 | 88 | **"pytest" was a word-error** — actual tool is stdlib `unittest` (zero install). Resolves dep-stack contradiction. Scoped in-branch test set: veto-blocking tier (`resolve_cited_path` × 5 cases + `is_lazy_reason` × 6 + `has_recognized_extension` × 5). Surfaced two strategy-independent items: (a) `run-guards.sh` has no per-guard timeout — 3-line fix in-branch; (b) promtool cannot fully replace `validate-alert-rules.sh` (security path-traversal gates would be lost — confirmed by observability matrix). |
| code-reviewer | 55 | 80 | Dropped ruff/uv requirement. Proposed `validate-python-stdlib-only.sh` + heredoc-elimination guard + `compileall` parse-check as the toolchain. Normalized 4-trigger commitment device: (a) module-count ≥3, (b) cumulative >500 LoC under lib/, (c) non-stdlib import w/o trailer, (d) shell heredoc >5 lines. |
| dry-reviewer | 70 | 75 | **Flipped to system-only**, conceded D-leverage was overstated (~47% LoC reduction on alert-rules, not whole-guard deletion). Proposed 4 constraints: pin python3-yaml in Dockerfile, bash `validate-guards-lib-import-discipline.sh`, defer mypy indefinitely, no pip-audit until non-pyyaml third-party dep appears. |

**Round 2 average satisfaction: 80.4** (up from 68.1).

### Round 2 Convergences

1. **Dep stack: system-only + tripwires (6 of 7).** Operations + dry-reviewer flipped. Observability is the lone uv holdout at 85; her one hard ask (T-2 supply-chain audit non-deferrable) has a compromise proposal: land system-only now, make T-2 the first ordered follow-up devloop, and infrastructure's tripwire (T1: any non-stdlib import in `lib/`) auto-promotes to uv when pip-audit wires in. Effect: uv lands *with* T-2, ~1-2 weeks after this ADR.

2. **(D) is complement, not replacement.** Observability's matrix is authoritative. Named slate is 2 entries: D-1 (`promtool check rules` as parallel gate to `validate-alert-rules.sh`) + D-2 (Grafana provisioning dry-run for `grafana-datasources.sh` UID-half). D-3 (yq sweep) dropped. D-4 (dashboard-panels) deferred contingent on D-2 success.

3. **In-branch deliverables (strategy-independent)** — converged set:
   - ADR-0034 + .gitignore rationale comment
   - `_python_bin.sh` interpreter pin helper
   - `scripts/guards/lib/__init__.py` + replace `sys.path.insert` with `PYTHONPATH=` shell helper
   - `scripts/guards/lib/README.md` stdlib-only contract
   - **`doc_cite_extract.py` unittest backfill (veto-blocking per security)** — `resolve_cited_path` + `is_lazy_reason` + `has_recognized_extension` minimum; full surface preferred
   - `validate-python-stdlib-only.sh` + its own tests
   - `run-guards.sh` per-guard timeout (3-line fix, test owns) + unittest wire-in
   - The guard-vendor-coverage-matrix.md (observability — already landed)

4. **4-trigger commitment device** (code-reviewer's canonical form, accepted by infrastructure + dry-reviewer):
   - (a) Module-count: 3rd `.py` file under `scripts/guards/lib/` ⇒ re-debate
   - (b) LoC: cumulative >500 LoC under `scripts/guards/lib/*.py` ⇒ re-debate
   - (c) Dependency: any non-stdlib import without `Approved-Stdlib-Exception:` trailer ⇒ re-debate (structurally enforced by `validate-python-stdlib-only.sh`)
   - (d) Heredoc: new shell heredoc >5 lines of Python ⇒ blocked (structurally enforced by `validate-no-multiline-python-heredocs.sh`)

   ANY trigger fires the re-evaluation gate.

### Remaining Open Items

1. **Observability's acceptance** of the system-only + T-2-as-first-followup compromise (her R2 sat 85 was uv-conditioned).
2. **Security's path to ≥75**: depends on test's scoped unittest set landing in-branch with the specific resolve_cited_path cases security named.
3. **dry-reviewer's 4 constraints** require security endorsement to lock at 85+.
4. **Vendor-binary SHA256 pinning contract** (security ↔ infrastructure) — both committed; documenting in ADR-0034.

## Consensus

**Reached: Round 3 (2026-05-14).** All 7 specialists at WOULD_ACCEPT_CURRENT = yes. Average satisfaction: 89.9.

### Final Satisfaction Table

| Specialist | R1 | R2 | R3 | Final | WOULD_ACCEPT |
|------------|----|----|----|-------|--------------|
| observability | 78 | 85 | 91 | **93** | yes |
| infrastructure | 72 | 85 | 87 | **88** | yes |
| operations | 82 | 88 | 88 | **90** | yes |
| security | 55 | 62 | 88 | **88** | yes |
| test | 65 | 70 | 88 | **90** | yes |
| code-reviewer | 55 | 70 | 80 | **90** | yes |
| dry-reviewer | 70 | 75 | 75 | **90** | yes |

**Cross-cutting specialists at consensus**: security 88, test 90, observability 93, operations 90. All above the ADR-0024 §5.7 70-floor. **No user risk-acceptance gate triggered.**

### Key Convergences

1. **Target state**: **(C) Polyglot-formalized + (D)-as-complement hybrid**, with deliberately narrow "glue-language" carve-out. Dep stack: **system-only stdlib-only with 5-trigger anti-drift commitment**.

2. **(D) is complement, not replacement.** Observability's per-guard vendor-coverage matrix (`docs/debates/2026-05-14-python-guard-pipeline-strategy/guard-vendor-coverage-matrix.md`) was the load-bearing factual artifact: only `grafana-datasources.sh` of 6 has meaningful (>5%) deletable LoC under vendor-native replacement. The remaining 5 encode Dark-Tower-specific policy (ADR-0011/0029/0031) that no upstream tool carries. (D)'s leverage is **adding new coverage** (PromQL syntax via promtool, JSON schema via Grafana dry-run), not deleting Python.

3. **(B) rejected unanimously**: rewriting Python heredocs to bash + jq/yq creates net +13 to +18 new duplications (dry-reviewer's accounting); reverts task #39's lib module which is pure DRY regression.

4. **(A) rejected unanimously**: rewriting working bash guards into Python is rewrite-for-rewrite's-sake.

5. **`doc_cite_extract.py` test gap is independently load-bearing**: stdlib `unittest` backfill of `resolve_cited_path` (7 cases including symlink-escape and OSError branches), `is_lazy_reason` (6 cases), `has_recognized_extension` (4 cases) lands in this branch regardless of language strategy. Security's veto-blocking precondition; test specialist's framework choice (stdlib `unittest`) collapses the supply-chain concern.

6. **5-trigger commitment device** (code-reviewer + dry-reviewer + infrastructure + operations + security merged): T-1 (first PyPI dep proposal, structurally enforced); T-2 (3rd `.py` module, reviewer); T-3 (>500 LoC under lib/, reviewer); T-4 (shell heredoc >5 lines of Python, structurally enforced); T-5 (CI-vs-devloop interpreter drift, reviewer).

7. **`run-guards.sh` per-guard timeout** (test's strategy-independent finding): closes a Layer-3 hang vulnerability; `GUARD_TIMEOUT_SECS` env var default 30s; exit-124 and exit-137 both emit distinct STATUS REASON tokens.

8. **Vendor binary pinning**: SHA256 for standalone binaries (buf/kubectl precedent); image-digest for container-distributed tools (grafana/grafana@sha256:...); `apt:latest` and `pip install <binary-shadowing-package>` explicitly rejected.

### Resolved Open Questions

| Question | Resolution |
|----------|-----------|
| Dep stack: uv vs system-only | **system-only** with documented promotion criteria. Operations + dry-reviewer flipped after seeing observability's matrix + internalizing security's Debian-backport argument. |
| Schelling point for uv-flip | **"first PyPI dep proposal"** (security's framing) — not LoC, not module count |
| Pytest vs unittest for in-branch backfill | **stdlib `unittest`** (test specialist's reconciliation removed pytest-install supply-chain concern) |
| Module layout (flat vs `__init__.py` package) | **Flat** — `PYTHONPATH=$REPO_ROOT/scripts/guards/lib python3 -m <module>` via `_python_env.sh` helper. `__init__.py` deferred to T-2. |
| Helper-script name | **`_python_env.sh`** (dry-reviewer's name, ratified by observability + code-reviewer) |
| AST walker home | **`scripts/guards/simple/validate-python-stdlib-only.sh`** (matches existing `validate-*` convention) |
| ADR shape: amend ADR-0033 or standalone | **Standalone ADR-0034** + brief amendment to ADR-0033 §When-to-Revisit (code-reviewer's call: "glue-language is a new category worth searching for") |
| (D) acceptance bar | **Fixture-injected bug + zero false-positives on production artifacts** before promotion from advisory to gate |
| T-2 §Design notes | `python3 -I` (isolated) + `-P` (no script-dir prepend) hardening baked in proactively (security + observability ask) |
| (B) reverts in-flight doc-citation work? | (B) rejected, so moot. (C) leaves task #39 commits standing. |

## Decision

**ADR-0034 (Accepted)**: `docs/decisions/adr-0034-python-as-second-tier-guard-language.md`

Cross-reference artifact: `docs/debates/2026-05-14-python-guard-pipeline-strategy/guard-vendor-coverage-matrix.md` (observability)
