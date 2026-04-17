# Devloop Output: ADR-0031 alert-rules guard + alert conventions

**Date**: 2026-04-17
**Task**: Implement `scripts/guards/simple/validate-alert-rules.sh`, `docs/observability/alert-conventions.md`, and `infra/docker/prometheus/rules/_template-service-alerts.yaml` per ADR-0031 prerequisite #1 + relevant conventions section of #4.
**Specialist**: operations (implementer; also domain owner per ADR-0031)
**Mode**: Agent Teams (full)
**Branch**: `feature/dashboard-owner-debate`

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `0f09c1676f4813a78b363a00daf5a1faab3372ab` |
| Branch | `feature/dashboard-owner-debate` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@devloop-adr-0031-alert-rules-guard` |
| Implementing Specialist | `operations` |
| Iteration | `1` |
| Security | `CLEAR (security@devloop-adr-0031-alert-rules-guard)` |
| Test | `RESOLVED (test@devloop-adr-0031-alert-rules-guard) — 1 low finding fixed` |
| Observability | `CLEAR (observability@devloop-adr-0031-alert-rules-guard) — 1 accepted tech-debt` |
| Code Quality | `CLEAR (code-reviewer@devloop-adr-0031-alert-rules-guard) — 2 non-blocking nits` |
| DRY | `CLEAR (dry-reviewer@devloop-adr-0031-alert-rules-guard)` |
| Operations | `N/A (implementer is operations)` |

---

## Task Overview

### Objective

Land the first of three ADR-0031 prerequisite guard+conventions bundles:
1. `scripts/guards/simple/validate-alert-rules.sh` — enforces alert-rule structural hygiene
2. `docs/observability/alert-conventions.md` — human-readable conventions + severity taxonomy with user-impact calibration anchors
3. `infra/docker/prometheus/rules/_template-service-alerts.yaml` — starter template

### Scope

- **Service(s)**: cross-cutting (CI guard + docs + infra template)
- **Schema**: No
- **Cross-cutting**: Yes (guard runs against all `*-alerts.yaml` files in `infra/docker/prometheus/rules/`)

### Debate Decision

NOT NEEDED — ADR-0031 already captured consensus on:
- Guard semantics (runbook_url repo-relative, severity ∈ {page,warning,info}, for: ≥ 30s, no hostnames/credentials in annotations)
- Conventions doc scope (threshold patterns, burn-rate shapes, for: conventions, annotation hygiene, severity taxonomy with user-impact calibration anchors)
- Template location (`infra/docker/prometheus/rules/_template-service-alerts.yaml`)
- Owner (operations)

See: `docs/debates/2026-04-17-service-owned-dashboards/debate.md` and `docs/decisions/adr-0031-service-owned-dashboards-alerts.md`.

---

## Planning

**Gate 1: PASSED** 2026-04-17.

### Key scope decisions during planning

1. **Grandfather allowlist, not in-devloop migration.** ADR-0031 makes per-service alert files service-specialist-owned. Severity reclassification (`critical` → `page` vs `warning`) is a GC/MC specialist judgment call, not mechanical. Implementer surfaced the conflict; Resolution 2 (allowlist) adopted after thrash.

2. **Allowlist storage: external file** (`scripts/guards/simple/alert-rules.legacy-allowlist`). Plain-text, exact-match filenames, line-count non-expansion check with `# ALLOWLIST_EXPANSION_APPROVED_BY:` marker required for expansion.

3. **Burn-rate multiplier: ADR-0011's 10x/5x**, not Google SRE's 14.4x. Conventions doc documents ADR-0011 shape; MWMBR noted as future-ADR candidate.

4. **Deadline: 2026-06-30** (per security, tighter than the 90-day floor).

5. **Annotation hygiene enforced in BOTH strict and lenient modes** (stricter than literal Resolution 2 spec; correct).

6. **Lenient severity set: `{page, warning, info, critical}`** explicitly — typo `criitcal` still fails.

### Reviewer confirmations (all v3)

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test | confirmed |
| Observability | confirmed (3 doc-wording items for review-time verification) |
| Code Quality | confirmed |
| DRY | confirmed (SoT lint between alerts.md and alert-conventions.md deferred to code-review time) |
| Operations | N/A (merged with implementer role) |

---

## Pre-Work

None. ADR-0031 just landed in the same branch.

---

## Implementation Summary

Landed three new artifacts plus an external legacy-grandfathered allowlist mechanism, a supporting fixtures directory, and surgical edits to existing docs and one alert rule. All ADR-0031 Prereq #1 invariants enforced in strict mode; legacy `gc-alerts.yaml` + `mc-alerts.yaml` grandfathered in lenient mode until 2026-06-30.

**Guard** (`scripts/guards/simple/validate-alert-rules.sh`, ~450 lines):
- Python3 + PyYAML parser invoked via quoted heredoc; argv-passed path parameters (no shell interpolation into YAML content).
- 6 machine checks: `runbook_url` present, repo-relative under `docs/runbooks/` with `realpath`-bounded existence check (rejects traversal + symlink escape), `severity` in allowed set (strict: `{page,warning,info}`; lenient: `{page,warning,info,critical}`), `for:` ≥ 30s via unit-converting duration parser, annotation hygiene heuristics (IPv4-with-allowlist, bearer/auth/AWS/OpenAI/Stripe/GitHub/Slack keys, JWT, PEM private keys, internal DNS suffixes, prod/stage host fragments), `# guard:ignore(reason)` escape hatch scoped to hygiene only with ≥10-char non-lazy reason validation.
- `_template-*.yaml` skipped at file enumeration. Self-test flag `--self-test` exercises 23 fixtures covering both modes.
- Allowlist integrity: `EXPECTED_ALLOWLIST_COUNT` pin + `# ALLOWLIST_EXPANSION_APPROVED_BY:` marker scanned via `git grep` for expansion; shrinkage requires guard-source edit.
- `[LEGACY]` WARN emission per allowlisted file with deadline `2026-06-30` in every CI run.
- Post-review performance fix (code-reviewer F2): JSON-lines violation formatting consolidated from 4×N `python3 -c` subprocesses to 1 per file.

**Allowlist** (`scripts/guards/simple/alert-rules.legacy-allowlist`):
- Exact-match newline-separated repo-relative filepaths. No globs. Two entries: `gc-alerts.yaml`, `mc-alerts.yaml`. Header documents expansion/shrinkage protocol.

**Conventions doc** (`docs/observability/alert-conventions.md`, ~410 lines):
- Severity taxonomy with user-impact calibration anchors + collapsed pre-migration callout.
- Threshold patterns, burn-rate shapes (ADR-0011 10×/5× with MWMBR footnote), `for:` conventions with 30s floor, annotation hygiene rules + denylist + escape-hatch syntax.
- Mode-coverage table (strict vs lenient per check), severity-label-routing hygiene section, severity-bumping Alertmanager-side note, per-persona PR checklist, machine-enforced-vs-reviewer-only rule index.

**Template** (`infra/docker/prometheus/rules/_template-service-alerts.yaml`):
- One rule per severity (`<Svc>HighErrorRate` page, `<Svc>HighMemory` warning, `<Svc>SlowDependency` info). `<placeholder>` syntax for grep-ability. Comments explain each field. `NOT LOADED BY PROMETHEUS` note. Post-review refinement to the skip-glob comment (test F1).

**Fixtures** (`scripts/guards/simple/fixtures/alert-rules/`, 23 files):
- 9 pass fixtures: severity coverage, anchor-stripping, Go-template redaction (including `Bearer {{...}}` false-positive guard), lenient-legacy grandfathering, production-shaped multi-rule files.
- 14 fail fixtures: missing/malformed annotations, short `for:`, severity typos, hostname/IP/bearer/JWT leaks, absolute URL, nonexistent runbook target, path traversal, lazy `guard:ignore` reason, lenient-mode-still-enforces cases.

**TODO.md** (new):
- ADR-0031 Alert Migration section with GC and MC per-service entries (owner, deadline 2026-06-30, violations enumerated, acceptance criteria, Alertmanager coordination note, post-migration protocol including `EXPECTED_ALLOWLIST_COUNT` decrement).
- ADR-0031 Convention Follow-ups subsection capturing the `for:` floor / expr-window-pattern detection work surfaced by MCActorPanic.

**`infra/docker/prometheus/rules/mc-alerts.yaml`**:
- Single-line edit: MCActorPanic `for: 0m` → `for: 30s`. Team-lead-ruled workaround with semantic-shift acknowledgment (5m rate window continues to suppress flaps; 30s adds detection lag of 1-2 scrapes).

**`docs/observability/alerts.md`**:
- §654-700 (Alert Configuration Standards) replaced with A2-form subsection pointers into `alert-conventions.md`. Titles preserved for discoverability; bodies are one-line pointers. Reconciles manufactured-by-PR contradiction per dry-reviewer + team-lead approval.

---

## Files Modified

| Path | Status |
|---|---|
| `scripts/guards/simple/validate-alert-rules.sh` | NEW (chmod +x) |
| `scripts/guards/simple/alert-rules.legacy-allowlist` | NEW |
| `scripts/guards/simple/fixtures/alert-rules/*.yaml` | NEW (23 fixtures) |
| `docs/observability/alert-conventions.md` | NEW |
| `infra/docker/prometheus/rules/_template-service-alerts.yaml` | NEW |
| `TODO.md` | NEW |
| `infra/docker/prometheus/rules/mc-alerts.yaml` | EDITED (1 line: MCActorPanic `for:` bump) |
| `docs/observability/alerts.md` | EDITED (§654-700 shrunk to pointer subsections) |
| `docs/specialist-knowledge/{media-handler,observability,security}/INDEX.md` | EDITED by Lead during Layer 3 unblock + post-merge cross-references added |

---

## Devloop Verification Steps

### Layer 1: cargo check — PASS (~20s)
Workspace compiles clean.

### Layer 2: cargo fmt --check — PASS
No format violations.

### Layer 3: Simple Guards — PASS (all 16 guards)
Initial run reported 1 failure: `validate-knowledge-index` flagged 3 specialist INDEX files at 76 lines (pre-existing from the earlier debate reflection step, not this devloop's work). Lead trimmed one line from each (media-handler, observability, security INDEXes) by combining adjacent related entries. All 16 guards now pass including the new `validate-alert-rules`.

### Layer 4: Tests (lib) — PASS
All workspace unit tests pass (7+242+19+105+several others).

### Layer 5: Clippy — PASS
No warnings with `-D warnings`.

### Layer 6: cargo audit — PRE-EXISTING FAILURE (not introduced by this devloop)
5 vulnerabilities found in dependencies: quinn-proto 0.10.6 (RUSTSEC-2026-0037), ring 0.16.20 (RUSTSEC-2025-0009), rsa 0.9.10 (RUSTSEC-2023-0071), rustls-webpki 0.101.7 (RUSTSEC-2026-0099, RUSTSEC-2026-0098). Plus 4 warnings (unmaintained/unsound). **None introduced by this devloop** — no Cargo.toml/Cargo.lock changes. These are pre-existing main-branch vulnerabilities requiring a separate dependency-update devloop. Flagged for user follow-up.

### Layer 7: Semantic Guard — SAFE
General-purpose agent analyzed diff. Verdict: no credential leaks, shell injection-proof, python heredoc injection-proof, allowlist pin matches active entries, mc-alerts.yaml `for: 30s` change semantically sound with the `increase([5m])` expr window.

### Layer 8: Env-tests — SKIPPED (lead judgment)
This devloop contains zero Rust/service code changes. Only runtime artifact is Prometheus alert-rule YAML (one-line `for:` bump on MCActorPanic). env-tests exercise service join flow and auth, not Prometheus rule firing semantics. Cluster not currently up; setup + rebuild cost (~15 min) vs zero expected relevant catches. Skipping per Lead judgment; documented as tech-debt trace.

### Artifact-specific: shellcheck
Not available in this sandbox environment (implementer flagged this; CI will catch). Script uses `set -euo pipefail`, quoted expansions, `grep -Fxq --`, `shopt -s nullglob`, quoted python heredoc — low risk.

---

## Code Review Results

### Security Specialist
**Verdict**: CLEAR
**Findings**: 0 found, 0 fixed, 0 deferred

Pre-scan of gc-alerts.yaml + mc-alerts.yaml hygiene denylist returned zero hits — lenient mode isn't grandfathering any actual secrets. Plan-phase hardening asks (IPv4 allowlist addition, AWS pattern broadening, lazy-ignore reject, path-traversal check) all applied. Four-layer defense on runbook_url exfil closure verified.

### Test Specialist
**Verdict**: RESOLVED
**Findings**: 1 found, 1 fixed, 0 deferred

F1 (low): template comment at `_template-service-alerts.yaml:12` claimed "passes strict mode" — technically false because `<svc>` placeholder runbook paths don't exist on disk. Implementer applied option (b): reframed comment to explain skip-glob exemption + copy-substituted output must pass strict. Fix verified. 23/23 fixtures pass self-test.

### Observability Specialist
**Verdict**: CLEAR (with 1 accepted tech-debt entry)
**Findings**: 1 found, 0 fixed, 1 deferred (accepted)

Guard does not honor the expr-window smoothing pattern documented in conventions doc §Anti-patterns. MCActorPanic forced from `for: 0m` → `for: 30s` as workaround. Captured in TODO.md §Convention Follow-ups with guard-enhancement proposal. Owner: operations specialist.

### Code Quality Reviewer
**Verdict**: CLEAR
**Findings**: 2 non-blocking nits sent to implementer (optional follow-up)
**ADR Compliance**: ADR-0031 Prereq #1 rules map 1:1 to guard checks (table in verdict). Lenient mode narrowly relaxes rules 2+3 only. Rules 1, 4, 5 unconditional. Mode-coverage table at `alert-conventions.md:346-361` makes this visible.

F1 (nit): `readonly` on scalar `EXPECTED_ALLOWLIST_COUNT` — correct as-is.
F2 (nit): 4 python3 subprocess spawns per violation for JSON unpacking; fine at shipped scale (0 production violations); consolidation opportunity if anyone hits slow-run report.

### DRY Reviewer
**Verdict**: CLEAR

**True duplication findings** (entered fix-or-defer flow): None.

**Extraction opportunities** (tech debt observations for TODO.md):
1. Secret-pattern list now in 3 places (`no-secrets-in-logs.sh:28`, `no-hardcoded-secrets.sh`, new guard's `HYGIENE_PATTERNS`). Threshold-of-three reached — candidate for shared library in `common.sh`. Owner: operations, future devloop.
2. Two YAML-parsing approaches across guard family (awk in `validate-application-metrics.sh`, PyYAML here). Document precedent when a third arrives; not worth extracting yet.

Plus 1 soft lint on `alerts.md:664-666` pointer wording (mini-restatement of conventions); non-blocking.

### Operations Reviewer
**Verdict**: N/A (merged with implementer role)

---

## Tech Debt

Entries created by this devloop:

1. **ADR-0031 severity + URL migration** (TODO.md §ADR-0031 Alert Migration) — deadline **2026-06-30**. Owners: `global-controller` specialist (gc-alerts.yaml), `meeting-controller` specialist (mc-alerts.yaml). CI emits WARN in every run until resolved. Non-expanding allowlist enforces scope stability.

2. **Guard enhancement: expr-window pattern detection** (TODO.md §ADR-0031 Convention Follow-ups) — the `for: ≥ 30s` floor should exempt rules whose expr already uses a rate/increase/sum_over_time window ≥ 30s. Surfaced when MCActorPanic's intentional `for: 0m` + `increase([5m])` pattern hit the floor. Workaround applied; proper fix deferred. Owner: operations specialist.

3. **Secret-pattern extraction** (dry-reviewer observation; not in TODO.md yet — recommend adding) — three guards now carry near-duplicate secret-regex lists (`no-secrets-in-logs.sh:28`, `no-hardcoded-secrets.sh`, `validate-alert-rules.sh:HYGIENE_PATTERNS`). Threshold-of-three reached; candidate for a shared `common.sh` helper in a future devloop. Owner: operations.

4. **PyYAML dependency declaration** — guard uses `import yaml` but no explicit pip install step in this devloop. CI environment must already have PyYAML (validate-application-metrics.sh has the same implicit dependency). If a clean-environment CI run surfaces an ImportError, either document the requirement in `.github/workflows/ci.yml` or add a `python3 -c 'import yaml'` preflight in the guard-runner.

Pre-existing tech debt surfaced but not introduced:

5. **cargo audit**: 5 CVEs in deps (quinn-proto, ring, rsa, rustls-webpki ×2) — separate dependency-update devloop needed.

---

## Rollback Procedure

1. Start commit: `0f09c1676f4813a78b363a00daf5a1faab3372ab`
2. Soft reset: `git reset --soft 0f09c167`
3. No schema or deployment changes — simple git revert is sufficient.
4. Note: the `mc-alerts.yaml` `for: 0m` → `30s` change is part of this devloop's diff. A revert restores `for: 0m`; if Alertmanager was already running against `for: 30s`, the config reload after revert will simply restore the original behavior (no breaking change either way).

---

## Reflection

### What worked

- **Self-test fixtures as a dev-time accelerator**. The `--self-test` flag + 23 fixtures paid off multiple times: caught a `REPO_ROOT` off-by-one, verified the template's post-substitute shape, verified every late-arriving security hardening, and gave reviewers deterministic pass/fail evidence for their plan-phase asks. Two-for-one: accelerates implementation AND shortens review cycles.

- **Dry-running against sed-transformed legacy files**. Running the guard against `sed 's/critical/page/; s|https://...||' gc-alerts.yaml` before handing off proved the hygiene regex doesn't false-positive on real production-shape text (200ms/500ms numbers, `{{ $value }}x` templating, etc.). This is the kind of pre-merge verification that's cheap in-devloop and expensive post-merge.

- **Non-expansion-by-construction on the allowlist**. The `EXPECTED_ALLOWLIST_COUNT` pin + `ALLOWLIST_EXPANSION_APPROVED_BY` marker is stronger than CODEOWNERS-based gating because it's enforced in every CI run, not just at merge review. The deliberate friction (must edit guard source to change count) is the feature, not a bug.

- **Hygiene-in-both-modes stricter-than-spec decision**. Applying annotation-hygiene regex in lenient mode too (beyond team-lead's literal Resolution 2 spec) removes the risk that grandfathered files silently harbor actual secrets. Security validated this with zero-hit pre-scan. Stricter-than-spec where it costs nothing to apply is the right call.

### What hurt

- **Scope thrash on the allowlist storage mechanism**. The sequence — in-PR migration → grandfather allowlist (hard-pinned array) → external file → hard-pinned array → external file — consumed significant planning cycles before Gate 1. Lead acknowledged the thrash. Future mitigation: when surfacing scope conflicts, explicitly enumerate all options with tradeoffs up front and ask for a one-word decision, rather than re-posing after each round. Did this eventually (MCActorPanic "edit/ignore/relax/block") and it worked immediately.

- **Reviewer messages arriving after implementation completed**. Multiple reviewers' plan-phase feedback landed after I'd already shipped artifacts (their messages were in flight during implementation). Everything happened to be already addressed or close enough that post-facto reconciliation worked, but this created a risk of rework if a non-trivial concern had surfaced. Future mitigation: when implementation starts before all plan-phase confirmations are in, explicitly flag "implementation starting on partial plan confirmation" to the team so reviewers can prioritize their feedback.

- **MCActorPanic `for: 0m` surprise**. Guard design assumed `for:` was the only flap-suppression mechanism; production code uses the `increase([Nm])` expr window instead. Surfaced at verification time, not at plan time. Lesson: when designing a guard against existing code, run the guard against that code AS PART OF PLAN-PHASE research, not implementation-phase verification. Would have changed the plan shape (include the `for:` exemption for expr-window patterns from day one, or explicitly note `for: 0m` as an edge-case to be handled in a follow-up).

- **Date discrepancy (2026-07-17 vs 2026-06-30)**. I repeated the wrong date across multiple plan messages despite the security reviewer and team-lead flagging it twice. Lesson: when an attribution error is flagged, do a literal find-and-replace across *draft* plan messages too, not just committed artifacts. Plan drafts are read as source-of-truth by reviewers cross-checking consistency.

### What surprised me

- **The allowlist mechanism debate was the highest-stakes decision in the devloop**, even though the actual invariants (what checks run, with what values) were settled quickly. Meta-lesson: design decisions about *how* scope is contained can dominate design decisions about *what* is in scope.

- **The `alerts.md` §654-700 duplication was only flagged by DRY-reviewer**, and only at review time (not plan time). Had they missed it, we'd have shipped a doc-vs-doc contradiction that makes the severity taxonomy ambiguous. The A2 pointer pattern is a useful tool for other doc-consolidation work — "preserve titles, replace bodies with pointers" keeps search-engine landing pages working.

### What I'd recommend to the next implementer

- If you're building a guard against existing code: dry-run the guard against that code as part of plan-phase. Don't discover the exceptions at verification time.
- If your guard needs a grandfather mechanism: bake in non-expansion enforcement, not just a "please don't expand" comment. CI-enforced is stronger than convention-enforced.
- If your conventions doc overlaps with an existing doc: decide Source-of-Truth at plan time, not review time. DRY-review will force it anyway; better to pre-empt.
- Fixtures that exercise false-positive edge cases (`Bearer {{...}}` templating, `500ms` near IP-like pattern) catch more bugs per line than positive-case fixtures.

---

## Issues Encountered & Resolutions

| # | Issue | Resolution |
|---|---|---|
| 1 | Scope conflict: "no edits to existing alert files" vs "existing files violate ADR-0031" | Resolution 2 (external allowlist + lenient mode + TODO.md). Final after multiple iterations. |
| 2 | Allowlist storage: external file vs hard-pinned array (team-lead flipped 3×) | Locked on external file + line-count pin + marker-based expansion gate per security's final ACK. |
| 3 | Deadline attribution error: implementer repeated `2026-07-17` (my provisional guess) as if it were security's pick; security's actual pick was `2026-06-30` | Applied literal find-and-replace across all artifacts after second flag from security + team-lead. Verified zero `2026-07-17` leakage with grep. |
| 4 | `MCActorPanic` `for: 0m` violated the guard's ≥30s floor in lenient mode (`for:` is floor-enforced in both modes per security's spec) | Team-lead ruling: bump to `for: 30s` (1-line edit to mc-alerts.yaml). Semantic shift documented (30s detection lag; actor supervision absorbs it). TODO.md convention-follow-up entry captures the proper fix (guard detects expr-window patterns and exempts). |
| 5 | `alerts.md` §654-700 contained a normative standards section that would contradict the new `alert-conventions.md` at merge | A2-form edit: preserve section titles, replace bodies with one-line pointers. Team-lead approved scope expansion because the contradiction was manufactured by this PR, not pre-existing. |
| 6 | Guard self-test crashed on first run due to `REPO_ROOT` computed from `SCRIPT_DIR/../..` instead of `../../..` (one directory level off) | Fixed at implementation time. Caught by self-test before any handoff. |
| 7 | Template-redaction-vs-bearer-regex ordering was implicit (not documented) | Security flagged in review. Ordering was already correct; made it explicit in the comment and added `pass-bearer-templating.yaml` fixture to prove the invariant. |
| 8 | Code-reviewer F2: 4-subprocess JSON unpack per violation | Fixed during review: consolidated to one `python3 -c` per file regardless of violation count. Verified no regression (23/23 still pass). |
| 9 | Test F1: template comment claimed "passes strict mode when dry-run" — technically false because `<placeholder>` runbook paths don't exist | Reworded comment to explain post-substitution invariant (option (b)). |

---

## Lessons Learned

1. **Dry-run guards against real targets during plan-phase**. The expr-window-pattern edge case (MCActorPanic) would have been caught at plan time if I'd run the guard against existing alert rules before committing to the design. Would have cost 10 minutes and saved a blocking decision loop.

2. **Verify date/number attributions before repeating them in plan messages**. When a reviewer is quoted saying "X", the literal string should be copy-pasted into plan drafts, not paraphrased or filled from memory. My `2026-07-17` error happened because I filled from a 90-day-from-today default instead of copying what security actually wrote.

3. **Stricter-than-spec is the correct default when cost is zero**. Annotation hygiene in both modes cost nothing (same regex already running) and eliminated a silent-grandfather risk. When a security-relevant invariant can be enforced for free across a larger surface, enforce it — even if the literal spec says "only in mode X."

4. **Non-expansion mechanisms need CI teeth**. A comment saying "don't expand this" relies on reviewer attention. A CI-enforced count pin + marker protocol eliminates human error. Applies to any grandfather-list / exemption-list pattern in a codebase.

5. **A2 pointer pattern for doc consolidation**. When retiring a normative section from one doc to another, preserve the section titles as pointers rather than deleting them outright. Search engines + bookmarks + reader mental maps all keep working. One-line pointer bodies are almost as good as full content + a visible indicator that the content moved.

6. **Fixture investment pays compound interest**. Each reviewer iteration produced at least one fixture request (test: legacy + anchor-stripping; security: path-traversal + lazy-ignore; observability: production-shape; code-reviewer: per-rule negative coverage). The 23-fixture final state is effectively the review audit trail in executable form. Future guards should plan for ~20 fixtures, not ~5.

7. **Scope thrash is a sign that ownership lines weren't well-framed initially**. The allowlist mechanism ping-pong happened because "who owns this decision?" wasn't settled at kickoff. If I'd asked "is allowlist mechanism my call, security's call, or team-lead's call?" in the first plan broadcast, the decision would have converged in 1 round instead of 5.

8. **Implementation-during-plan-confirmation is risky without explicit framing**. If the loop-state advances to `implementation` before all reviewers have confirmed, flag that explicitly to the team so late-arriving feedback can be prioritized. Doing this silently risks rework if a non-trivial concern surfaces.
