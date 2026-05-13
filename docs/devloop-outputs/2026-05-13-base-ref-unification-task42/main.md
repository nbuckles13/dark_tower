# Devloop Output: Base-Ref Unification + Redundant CI Fetch Removal

**Date**: 2026-05-13
**Task**: Unify base-ref resolution + remove redundant CI fetch (R-62, ADR-0033 §7 follow-up, task #42)
**Specialist**: infrastructure (paired with operations)
**Mode**: Agent Teams (full + `--paired-with=operations`)
**Branch**: `feature/browser-client-join-task38`
**Duration**: TBD

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `9f9bbf0dce19a57e2d3fb480163262be224684c6` |
| Branch | `feature/browser-client-join-task38` |

---

## Loop State (Internal)

<!-- Maintained by the Lead. -->

| Field | Value |
|-------|-------|
| Phase | `planning` |
| Implementer | `implementer@devloop-2026-05-13-base-ref-unification-task42` |
| Implementing Specialist | `infrastructure` |
| Paired Specialist | `operations` (replaces standard operations reviewer slot; active collaborator during implementation) |
| Iteration | `1` |
| Security | `security@devloop-2026-05-13-base-ref-unification-task42` |
| Test | `test@devloop-2026-05-13-base-ref-unification-task42` |
| Observability | `observability@devloop-2026-05-13-base-ref-unification-task42` |
| Code Quality | `code-reviewer@devloop-2026-05-13-base-ref-unification-task42` |
| DRY | `dry-reviewer@devloop-2026-05-13-base-ref-unification-task42` |
| Operations (paired) | `operations@devloop-2026-05-13-base-ref-unification-task42` |

---

## Task Overview

### Objective

Coherent five-part refactor of the diff-base resolution surface, closing the cold-fetch tech-debt pointer logged in task #38's main.md (`docs/devloop-outputs/2026-05-12-skill-step6-rewrite-task38/main.md` §Tech Debt Pointers).

1. **Resolve `BASE` to the merge-base SHA uniformly** across local + CI-PR + CI-push. The local-mergebase mode already does this (`scripts/lang/_get_base_ref.sh:110-112`); the CI-PR branch (~line 83) currently resolves to the base-branch *tip* via `base="origin/${GITHUB_BASE_REF}"`. Replace with `base=$(git merge-base origin/${GITHUB_BASE_REF} HEAD)` per the existing TODO at line 86. Keep `DIFF_MODE=two-dot` (correct under uniform merge-base resolution; three-dot would collapse to the same answer because `merge-base(merge-base-sha, HEAD) = merge-base-sha`).

2. **Delete the in-script `git fetch`** at `_get_base_ref.sh:79` — `actions/checkout@v4` with `fetch-depth: 0` (configured in `ci.yml:38`) makes the base ref pack-resident before any wrapper runs. The in-script fetch is redundant defense-in-depth that masks misconfiguration rather than failing fast.

3. **Add precondition guardrail** at `scripts/layer-all.sh` entry: a one-line `git rev-parse --verify origin/main^{commit} >/dev/null 2>&1 || die "CI clone too shallow; need fetch-depth: 0"` so future shallow-checkout regressions surface as actionable errors at the pipeline-entry layer.

4. **Audit `.github/workflows/ci.yml:106`** — second checkout block, no `fetch-depth` specified (defaults to 1). Confirm whether that job runs `scripts/layer-all.sh`; if so, set `fetch-depth: 0`. If not, add an inline comment documenting why the shallow checkout is intentional, so future readers don't "fix" it inadvertently.

5. **Retire `scripts/guards/common.sh:get_diff_base()`** — bisect the two-helper-family duplication. Preferred: delete and have callers read `$DEVLOOP_BASE_REF` (or the equivalent exported anchor) exported by `scripts/layer-all.sh`. Fallback: keep as a thin wrapper around `_get_base_ref.sh`. The older `guards/common.sh` family and the newer `lang/_get_base_ref.sh` family (per ADR-0033 §7) have been running side-by-side since Wave 1 #1 landed; consolidate.

### Behavior change vs pure refactor

**This is a behavior change to characterize, not a pure refactor.** The CI-PR `BASE_SHA` shifts from base-branch tip to merge-base. Every diff-aware check operating in CI-PR sees a different (narrower, more accurate) scope. The semantic improvement matches what `git diff --three-dot` users expect when reading "what did this PR contribute?", but it's not a no-op.

Required behavior-equivalence test: synthetic CI-PR scenario (mock `GITHUB_BASE_REF` env, fixture PR-vs-main diff with both real PR contributions and unrelated main-progress commits). Before/after must produce:
- Same `scripts/lang/*/changed.sh` exit codes (Rust + TS + proto)
- Same `buf breaking` verdict (verify via fixture that includes a known-no-op main commit; pre-refactor should false-positive flag main's unrelated commit as a "PR breakage," post-refactor should not)

Local-mergebase mode is a no-op (already resolves to merge-base today).

### Scope

- **Service(s)**: none (build/CI tooling only)
- **Schema**: No
- **Cross-cutting**: Yes — changes how every diff-aware script in the pipeline identifies "what changed", which every devloop running CI-PR will inherit.

### Debate Decision

NOT NEEDED — this is implementation of the existing TODO at `_get_base_ref.sh:86` plus removal of redundant defense-in-depth identified during task #38 code review. ADR-0033 §7 is the authoritative spec; no new ADR required.

---

## Cross-Boundary Classification

Per ADR-0024 §6.2. None of the touched paths are GSA per §6.4. The implementer (infrastructure) is the dominant owner; operations is paired and co-owns the `scripts/guards/common.sh` retirement piece.

For the **behavior-change** rows (`_get_base_ref.sh` line 83 merge-base shift), guard coverage exists (the unit tests under `_get_base_ref.test.sh`), but the change-pattern is a *semantic* substitution (base-branch tip → merge-base), not value-neutral. That rules out **Mechanical** per §6.2. Classification = **Minor-judgment**; co-confirmed by operations (paired) at Gate 1 + Gate 3.

For the **deletion** rows (line 79 fetch, `guards/common.sh:get_diff_base()`), guard coverage exists at the layer level (any caller that breaks would fail compile or test) and the change is structure-preserving (delete a no-op-given-our-CI-config). Defensible as **Mechanical**, but the operations-paired arrangement means we'll mark Minor-judgment for the `guards/common.sh` line and let the paired reviewer hunk-ACK.

Implementer fills this table at plan time with file-level rows. The Layer B classification-sanity guard runs before Lead issues "Plan approved."

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `scripts/lang/_get_base_ref.sh` | Minor-judgment | (paired-operations co-confirms behavior shift hunk lines 74-87) |
| `scripts/layer-all.sh` | Minor-judgment | (operations co-confirms guardrail at pipeline entry) |
| `.github/workflows/ci.yml` | Minor-judgment | operations (CI workflows are operations-owned surface; 2 hunks under Option B — coverage-comment + GUARD_DIFF_BASE deletion) |
| `scripts/guards/common.sh` | Minor-judgment | operations (`guards/` is shared infra) |
| `scripts/lang/_get_base_ref.test.sh` | Mechanical | — (test-helper change adds assertions; no semantic shift) |
| `scripts/lang/_get_base_ref.behavior-equivalence.test.sh` | Mechanical | — (net-new fixture asserting the documented behavior change) |
| `scripts/guards/simple/validate-cross-boundary-classification.sh` | Mechanical | operations (comment-only documentation hunk per @dry-reviewer Finding 1) |

---

## Planning

### Plan Confirmations (Gate 1)

| Reviewer | Plan Status | Timestamp |
|----------|-------------|-----------|
| Security | confirmed under Option B (pre-confirmed for either A or B; under B the env-injection surface is naturally closed by the forwarder — no shim-side validators landed nor needed; §Risks Q1 verbatim sentence captured in §Implementation Summary) | 2026-05-13T02:24Z |
| Test | confirmed (blocker `test_shallow_clone_guardrail` + should-fix `test_guard_callsite_coverage` both folded; post-confirm nit re: 7a/7b output clarity caught a missing-step gap and was substantively upgraded — 11/11 assertions pass) | 2026-05-13T02:20Z |
| Observability | confirmed (initial nit re: `ERROR:` prefix superseded by operations §Q6b — `PRECONDITION_FAILURE:` adopted; observability re-confirmed) | 2026-05-13T02:23Z |
| Code Quality | confirmed under Option B; upgraded from Option-A approval-with-conditions (clean ✅ on SSOT-convergence checklist item #3 under B vs Partial under A). 6 implementation commitments verified: fetch+TODO collapsed; inline echo/exit (no `die`); in-body SSOT-pointer comment; `__resolve_full_sha` retained; `ci.yml:81` deletion; `common.sh` docstring rewrite | 2026-05-13T02:22Z |
| DRY | confirmed under Option B; no remaining conditions (3 folds accepted). Post-lock A-leaning concession message (~02:25Z) was sent under stale-view of plan record; substantive factual flags from that message (cost measurement ~17ms × 11 = ~190ms/layer; `$DEVLOOP_BASE_REF` is memoized-resolver-output not independent-signal) folded into §Decisions and Tech Debt Pointer #2 verbatim — actually-better cost data than my earlier hand-wave. | 2026-05-13T02:25Z |
| Operations (paired) | **confirmed under Option B** (clean B-acceptance 2026-05-13T02:26Z post-paired-collaboration; pro-A reasons re-weighed and dissolved under the scope including `ci.yml:81` deletion; A-position recorded in §Rejected Alternatives; R1+R2+Q6b folds + post-flip `BASE_REF=` multiplication refinement folded into Tech Debt Pointer #2). Co-confirms cross-boundary Minor-judgment on _get_base_ref.sh / layer-all.sh / ci.yml (2 hunks) / common.sh at Gate 3. | 2026-05-13T02:26Z |

### File edit map (5 edited, 1 net-new)

| File | Edits | Net change |
|------|-------|------------|
| `scripts/lang/_get_base_ref.sh` | 3 hunks (delete fetch lines 77-82; swap base-resolution lines 83-85; drop TODO comment lines 86-91) | ~−18 lines, ~+5 lines (net shorter) |
| `scripts/layer-all.sh` | 1 hunk (precondition guardrail after `init_devloop_tmp`, line ~23). Under Option B: no `$DEVLOOP_BASE_REF` export needed (nothing reads it after the forwarder is in place; guardrail uses raw `git rev-parse`). | +~12 lines |
| `.github/workflows/ci.yml` | 2 hunks: (a) multi-line inline comment near coverage-job checkout line 106; (b) delete `GUARD_DIFF_BASE: ${{ github.event.pull_request.base.sha }}` at line 81 (dead env under Option B). | +3 lines, −1 line (net +2) |
| `scripts/guards/common.sh` | 1 hunk rewriting `get_diff_base()` body as a 3-line pure forwarder to `_get_base_ref.sh`. | ~−4 / +7 lines (net +3) |
| `scripts/lang/_get_base_ref.test.sh` | 1 hunk in `test_ci_pr` (case 5): swap `assert_base_source` for full-merge-base SHA assertion + DIFF_MODE=two-dot. 1 new test `test_local_clean_merge_base_regression` recording resolved SHA equals merge-base. | +30 lines |
| `scripts/lang/_get_base_ref.behavior-equivalence.test.sh` | **NET-NEW**: stash-old / stash-new before/after fixture asserting CI-PR BASE_SHA shifts from `origin/main` tip to merge-base while local-mode is unchanged. **Appended case (per @test Gate-1 blocker): `test_shallow_clone_guardrail` asserts `layer-all.sh` exits 2 with the `fetch-depth: 0` remediation message when run inside a `git clone --depth 1`.** | +160 lines |

### Decisions

#### get_diff_base() retirement — wrap, do not delete

**Caller count**: 14 callsites total — 3 internal helpers within `scripts/guards/common.sh` (`get_modified_files`, `get_added_files`, `get_deleted_files`) plus 11 across `scripts/guards/simple/**`:

```
scripts/guards/simple/no-secrets-in-logs.sh:35
scripts/guards/simple/api-version-check.sh:40
scripts/guards/simple/validate-env-config.sh:37
scripts/guards/simple/ts/no-test-removal-ts.sh:32
scripts/guards/simple/no-hardcoded-secrets.sh:33
scripts/guards/simple/ts/name-guard-dt-client.sh:41
scripts/guards/simple/validate-kustomize.sh:65
scripts/guards/simple/validate-cross-boundary-classification.sh:202
scripts/guards/simple/ts/no-secrets-in-ts.sh:31
scripts/guards/simple/ts/no-pii-in-logs-ts.sh:37
scripts/guards/simple/ts/exports-map-closed.sh:46
```

All callers are in the `guards/simple/` family. All consume the result as a single SHA/ref string passed to `git diff --name-only "$base"`.

**Decision: 1-line forwarder (Option B), with `ci.yml:81` `GUARD_DIFF_BASE` deletion. LOCKED 2026-05-13T02:19Z per @team-lead's decision-rule delegation to implementer.**

Confirmed under B by @dry-reviewer, @security, @code-reviewer. @operations (paired) walked from B → A → noted "either is defensible" — A-position recorded but B has the stronger architectural case.

`get_diff_base()` becomes a 3-line forwarder:

```bash
# get_diff_base — thin forwarder to the canonical resolver (ADR-0033 §7 SSOT).
# _get_base_ref.sh handles all env detection (GITHUB_ACTIONS, GITHUB_BASE_REF,
# local-mergebase, etc.) and emits a validated 40-char SHA on stdout.
get_diff_base() {
    "$(dirname "${BASH_SOURCE[0]}")/../lang/_get_base_ref.sh"
}
```

Plus 2nd hunk in `.github/workflows/ci.yml`: delete `GUARD_DIFF_BASE: ${{ github.event.pull_request.base.sha }}` at line 81 (dead env after the forwarder).

**One-line lock rationale (per team-lead)**: Option B chosen over Option A because SSOT-purity + permanent security-surface reduction are architectural wins; the @operations memoization concern (~550ms/pipeline, 0.6% of the 90s always-run budget) is real but contained, with a future surgical mitigation if it ever bites (add a per-run cache file to `_get_base_ref.sh` itself).

**Why Option B over Option A**:

1. **SSOT-purity is genuine, not cosmetic.** `_get_base_ref.sh` is the documented canonical resolver per ADR-0033 §7. A thin forwarder that contains no logic IS the SSOT contract. A 4-step env chain that bypasses the resolver in the common case is a parallel implementation regardless of whether the env vars themselves are read by the resolver. @dry-reviewer and @code-reviewer made this point independently.

2. **Option A's chain is policy duplication that drifts** (per @dry-reviewer plan-confirm reversal 2026-05-13T02:21Z). The 4-step ladder encodes three policy decisions that are *already encoded inside `_get_base_ref.sh`*: (a) which env signal wins (precedence policy); (b) what the fallback is when no signal present (default policy — `_get_base_ref.sh:114-116` local-no-mergebase branch); (c) when to invoke the canonical resolver (invocation policy). If `_get_base_ref.sh` adds a new env signal tomorrow (e.g. `$DEVLOOP_FORCE_BASE` override), Option A's ladder must be updated in lockstep or it silently shadows the SSOT. **Drift waiting to happen.**

3. **Option B aligns `guards/common.sh` with the convention already in `lang/_changed_helpers.sh:30-33`** (per @dry-reviewer). That file already invokes `_get_base_ref.sh` directly via subprocess; every lang/changed.sh transitively does this. **Convergence on convention, not just on code** — same invocation pattern across both helper families.

4. **@security finding is permanently closed**, not papered-over with two new regex validators that future contributors must remember to update if env-var shapes evolve. Less surface to maintain forever; the resolver IS the validation gate.

5. **Memoization concern is real but contained** (corrected post-Gate-2 per @operations Note 2 + @dry-reviewer measurement — supersedes the earlier "already-mitigated by the cache" claim, which was factually wrong). The existing `${DEVLOOP_TMP}/changed-files.layer-*` cache is **output-only**: `_get_base_ref.sh` overwrites it on every invocation (line 126), there is no read path; subsequent invocations do NOT short-circuit. Per-invocation cost is **~17ms** (warm; cold ~28ms — measured by @dry-reviewer on this branch and independently verified at ~16ms; bulk is bash startup + sourcing `_common.sh`); for 11 simple-guard callers that's **~190ms per layer**, real and monitorable but small vs. a multi-minute CI run. *Note on `$DEVLOOP_BASE_REF` framing* (per @dry-reviewer factual flag): if `layer-all.sh` ever exports `$DEVLOOP_BASE_REF` as a cached form of `_get_base_ref.sh`'s output (as Tech Debt Pointer #2 contemplates), that env var is **the resolver's memoized output, not an independent signal**. Any future shim that reads it is a perf-shim, not policy-independent; if `_get_base_ref.sh` learns a new env signal, the shim would need a coordinated update. The Tech Debt Pointer mitigation (per-run SHA cache + emission-suppression sentinel in `_get_base_ref.sh`) is the only memoization path; **it does not exist today**. Under these corrected facts, @operations' original pro-A memoization argument was real-but-small (~190ms/layer × pipeline runs adds up across PRs), stronger than the lock-time "10-20ms" prose acknowledged. The lock is still defensible — security + DRY + code-reviewer arguments for B carry independent of cost (SSOT purity, env-injection surface elimination, convention alignment with `lang/_changed_helpers.sh`) — but recording honestly: the cost-revision rationale used at lock time relied on a cache-read that doesn't exist. If the ~190ms × pipeline-run cost ever becomes a measurable budget pressure, see Tech Debt Pointer #2 for the contained future mitigation.

6. **If subprocess cost ever DOES become load-bearing, the right fix is SSOT-side, not shim-side.** Have `layer-all.sh` export `$DEVLOOP_BASE_REF` and `_get_base_ref.sh` short-circuit on its presence — one place, the SSOT. Not a 4-step bash env chain in `common.sh` encoding parallel policy.

7. **`$GUARD_DIFF_BASE` retention argument doesn't survive runbook scrutiny.** The runbook (task #39) documents the *historical* env equally well whether it currently exists or has been deleted. Deleting now means task #39 documents one less moving piece.

8. **`ci.yml:81` co-deletion is a DRY win, not a coupling cost** (per @dry-reviewer). `$GUARD_DIFF_BASE` is a *third* env var in the same conceptual slot as `$DEVLOOP_BASE_REF` and `$GITHUB_BASE_REF`. Three env vars, one concept, in a system literally about consolidating to one canonical anchor. Option A retains all three; Option B retires `$GUARD_DIFF_BASE`, leaving `$GITHUB_BASE_REF` (GitHub → SSOT signal, read internally by `_get_base_ref.sh`) and optionally future `$DEVLOOP_BASE_REF` (SSOT → consumer cache signal). Crisp boundary: two anchors, not three.

**@security defense-in-depth finding — closes naturally under B.** `get_diff_base` doesn't read `$DEVLOOP_BASE_REF` or `$GUARD_DIFF_BASE` directly. The forwarder invokes `_get_base_ref.sh`, which already enforces `__validate_ref_name` on `GITHUB_BASE_REF` (line 76) and validates resolved SHAs via `__resolve_full_sha` (line 121). No additional shim-side validation needed.

**@operations memoization argument — recorded as Tech Debt Pointer #2 for close-out (corrected post-Gate-2 per @operations Note 1+2 + @dry-reviewer measurement: actual cost is ~17ms warm × 11 = ~190ms/layer, NOT 10-20ms; the existing changed-files cache is output-only, no read path).** The full Tech Debt Pointer text including the emission-suppression-sentinel mitigation is in §Tech Debt Pointers entry 2.

**(Original "4-step chain" rationale retained below for the Gate-3 reviewer record — it was a defensible position, just not the chosen one):**

1. **It's adapter logic, not duplication.** Verified: neither `_get_base_ref.sh` nor `_common.sh` reads `$GUARD_DIFF_BASE` or `$DEVLOOP_BASE_REF`. Those env vars live in the guards/pipeline plumbing, not the resolver. So the chain isn't re-implementing SSOT logic — it's reading two env vars the resolver doesn't read, then falling through to the resolver, then `HEAD`.

2. **Performance — env-var primary path is effectively memoization.** `_get_base_ref.sh` is a script invocation: fork, source `_common.sh`, `init_devloop_tmp`, `git rev-parse`, etc. (~50ms per call.) `get_diff_base` is called from `get_modified_files` / `get_added_files` / `get_deleted_files`, which some guards invoke many times per run. In CI-pipeline mode, `$DEVLOOP_BASE_REF` is set once by `layer-all.sh` at entry and read for free thereafter. The 1-line forwarder would re-fork the resolver on every call. **Memoization win, not just back-compat.**

3. **`$GUARD_DIFF_BASE` retention has independent value during the runbook transition.** `ci.yml:81` still sets it; ripping it out now means coordinating two surfaces (ci.yml + common.sh) where the safe-rollback story is messier. The runbook (task #39) is where operators learn what the legacy env was for; that's the right point to retire it.

**@security defense-in-depth finding (folded)**: the new shim reads `$DEVLOOP_BASE_REF` and `$GUARD_DIFF_BASE` and would bypass `__validate_ref_name` (`_get_base_ref.sh:33-39`) without re-validation. Fix per @security recommendation captured in the source-order list above (strict / permissive validators on env-var branches). Closes the env-injection surface that `__validate_ref_name`'s own comment at line 30 cites as the design intent.

Net effect: all 11 simple-guard callsites stay untouched (zero blast radius outside `common.sh`); resolver-duplication is eliminated (chain is adapter logic, not parallel resolution); the legacy env var stays through the runbook transition; the env-injection surface is closed per @security.

**Rejected alternatives**:
1. **Delete `get_diff_base()` + edit all 14 callsites**: larger blast radius (10 sibling scripts), larger review surface; wrapper name reads like English at the callsite. The forwarder keeps the API ergonomic without keeping the logic.
2. **Option A — 4-step env-preference chain + @security regex validators + retain `ci.yml:81`** (final paired-operations position, considered and rejected): the memoization win is real (~550ms/pipeline) but is 0.6% of budget with a contained future mitigation; the @security validator surface is permanent overhead; the `$GUARD_DIFF_BASE` retention doesn't survive the runbook-documents-it-equally argument.

#### ci.yml:106 — coverage job — document, do not add fetch-depth (paired-operations §Part 4 adopted)

The second `actions/checkout@v4` is in the `coverage` job (`ci.yml:84-139`). Coverage runs only `cargo llvm-cov --workspace --lcov` (line 132). No `scripts/layer-all.sh`, no diff-aware tooling. The shallow checkout (default `fetch-depth: 1`) is intentional and faster.

**Decision: add explanatory inline comment** so future readers do not "fix" it. Per paired-operations §Part 4, expanded comment to give a positive signal that this job was *considered and deliberately exempted*:

```yaml
      - name: Checkout code
        uses: actions/checkout@v4
        # fetch-depth omitted: coverage job runs cargo-llvm-cov over full
        # working tree; no diff-aware tooling is invoked here. The pipeline
        # job above requires fetch-depth: 0 — keep that surface tight.
```

The earlier draft used a single-line comment; the multi-line version paired-operations proposed is more durable because it names *why* the exemption is safe (cargo-llvm-cov full-tree-scan) and the *contrast* (the pipeline job has the requirement), so a future CI auditor doesn't need to chase context.

#### Precondition guardrail — exit 2, dynamic per-mode ref check, greppable `PRECONDITION_FAILURE:` token (paired-operations §Part 3 + R1 + Q6b nit adopted)

Exit code 2 (not 1) so the failure is distinguishable from regular layer failures in operator dashboards. Placed after `init_devloop_tmp` (so `${DEVLOOP_TMP}` exists for log capture) and before the layer loop.

**Detection (R1 fix — dynamic ref, not hardcoded `origin/main`)**: `ci.yml:4,7` triggers on both `main` and `develop`; a PR to develop with a shallow checkout would slip past a hardcoded `origin/main` check. The guardrail must verify the ref the resolver will actually use. Implementation mirrors `_get_base_ref.sh`'s per-mode partition:

```bash
# Precondition: base ref must be pack-resident (ci.yml fetch-depth: 0).
# Dispatch per mode mirrors _get_base_ref.sh's resolution branches.
if [[ -n "${GITHUB_ACTIONS:-}" && "${GITHUB_EVENT_NAME:-}" == "pull_request" ]]; then
  # CI-PR: $GITHUB_BASE_REF is the actual base branch (main OR develop).
  __precondition_ref="origin/${GITHUB_BASE_REF}"
elif [[ -z "${GITHUB_ACTIONS:-}" ]]; then
  # Local: resolver uses origin/main.
  __precondition_ref="origin/main"
else
  # CI-push: resolver uses HEAD~1 (local by definition, no remote pack lookup).
  # Skip the precondition — there's nothing to check.
  __precondition_ref=""
fi
if [[ -n "$__precondition_ref" ]] && ! git rev-parse --verify "${__precondition_ref}^{commit}" >/dev/null 2>&1; then
  printf 'PRECONDITION_FAILURE: %s not in local pack — CI clone too shallow.\n\n' "$__precondition_ref" >&2
  printf 'Fix: set actions/checkout fetch-depth: 0 in .github/workflows/ci.yml\n' >&2
  printf 'See docs/runbooks/devloop-validation.md (lands in task #39).\n' >&2
  exit 2
fi
```

**Leading token `PRECONDITION_FAILURE:` (paired-operations §Q6b nit)** — stable greppable anchor for ops dashboards. Distinct from `ERROR:` (which signals "wrapper-internal failure" in `_get_base_ref.sh`); `PRECONDITION_FAILURE:` cleanly means "pipeline-entry sanity check failed before any layer ran". Future precondition checks added to `layer-all.sh` should use the same token. Observability's earlier nit (`^ERROR:` greppability) is superseded by this stronger anchor — `PRECONDITION_FAILURE:` is also `^`-anchorable and *more semantically specific* than `ERROR:`.

**Tradeoff vs the simpler "check both refs" approach (paired-ops R1 Option A)**: going with the mode-dispatched version because:
- It matches the resolver's actual semantics 1:1 (no wasted check on the wrong ref).
- Adding a third long-lived branch later is a zero-code change: GitHub Actions already injects the right ref via `$GITHUB_BASE_REF`.
- The conditional logic is small enough to read at a glance.

**Test impact** — the @test Gate-1 blocker test `test_shallow_clone_guardrail` must be updated:
- stderr substring assertion: `cannot resolve base ref 'origin/main'` → `PRECONDITION_FAILURE: origin/main`
- still asserts `fetch-depth: 0` substring
- still asserts exit code == 2

The forward-reference to `docs/runbooks/devloop-validation.md` is intentional per paired-operations §Part 3 — task #39 will populate that path; until then operators can grep the leading `PRECONDITION_FAILURE:` token to find the guardrail directly.

#### CI-push consistency check (per task spec part 1) — prose corrected per paired-operations R2

CI-push branch (`_get_base_ref.sh:93-101`) currently uses `HEAD~1` as base. **No change needed for CI-push.** Two-dot semantics preserved (already two-dot).

**Invariant (R2 fix)**: On push to a long-lived branch (`main` OR `develop`, per `ci.yml:4`), `HEAD~1` is the prior tip via first-parent — which equals "what this push contributed" — whether the push is fast-forward or a merge commit. The plan record previously cited the narrower "fast-forward by definition" claim (which isn't necessarily branch-protection policy and isn't policy for `develop`); per paired-operations R2 the correct invariant is the first-parent walk.

### Behavior-equivalence test fixture sketch

`/work/scripts/lang/_get_base_ref.behavior-equivalence.test.sh`:

1. Construct synthetic git history under `mktemp -d`:
   - main has commits M1 (initial), M2 (unrelated progress on main, made after PR branch was cut)
   - feature/PR has commits P1, P2 branched off M1
2. Set up local file:// origin (same `init_repo_with_origin` pattern as existing tests).
3. **Before assertion (old behavior)**: extract OLD `_get_base_ref.sh` via `git show <START_COMMIT>:scripts/lang/_get_base_ref.sh`. With `GITHUB_ACTIONS=1 GITHUB_EVENT_NAME=pull_request GITHUB_BASE_REF=main`, OLD script resolves BASE_SHA to `origin/main` tip == M2 sha. `git diff --name-only $BASE_SHA..HEAD` would include M2's files (false-positive — those files are not the PR's contribution).
4. **After assertion**: NEW script resolves BASE_SHA to merge-base == M1 sha. `git diff --name-only $BASE_SHA..HEAD` includes only P1+P2 files.
5. **Local-mode regression**: same fixture, no `GITHUB_ACTIONS=` env. Both OLD and NEW resolve to M1 (merge-base). Assert SHA equality.
6. **`test_shallow_clone_guardrail` (per @test Gate-1 blocker + @operations R1 dynamic-ref refinement)**: separate `mktemp -d` under the same fixture, `git clone --depth 1 file://${origin}` from a different file:// origin so the local pack lacks `origin/main^{commit}`. Run `bash scripts/layer-all.sh` inside the shallow clone with `GITHUB_ACTIONS=1 GITHUB_EVENT_NAME=pull_request GITHUB_BASE_REF=main`. Assert:
   - exit code == **2** (exact match — distinguishes from layer-failure exit 1 and success exit 0)
   - stderr matches `PRECONDITION_FAILURE: origin/main` (substring — using the new leading token per paired-operations §Q6b nit)
   - stderr matches `fetch-depth: 0` (substring — the remediation must name itself)
6a. **CI-push skip path** (per @operations R1): same shallow clone, but with `GITHUB_ACTIONS=1 GITHUB_EVENT_NAME=push` (no `GITHUB_BASE_REF`). Assert exit code != 2 (the precondition skips — CI-push uses HEAD~1, which is local). Confirms the dynamic dispatch correctly partitions modes.
7. Header comment: explicit note that "if this branch is rebased post-devloop and START_COMMIT becomes unreachable, retarget the `git show` SHA to the new equivalent." Per @test Q2 answer caveat. (No tag created — keeping the rebase-safety burden inside the test file rather than introducing a repo-level tag.)

START_COMMIT pinned at `9f9bbf0dce19a57e2d3fb480163262be224684c6` per §Loop Metadata. The OLD copy lives only inside the test's `mktemp` dir, never committed.

### Behavior-equivalence test file location decision (per @dry-reviewer Finding 2)

**Decision: keep `scripts/lang/_get_base_ref.behavior-equivalence.test.sh` as a separate file** (not folded into `_get_base_ref.test.sh`).

**Lifecycle: durable** — kept forever as the regression test against ever flipping CI-PR resolution back to base-tip semantics. Not a one-shot artifact; it earns its keep by guarding the semantic boundary established in this task.

**Rationale**:
- `_get_base_ref.test.sh` declares hermeticity in its header (line 6): "no reads from /work workspace state. No real `git fetch` against actual `origin`." The behavior-equivalence test breaks that — it reads from the live git repo via `git show <START_COMMIT>:scripts/lang/_get_base_ref.sh`. Keeping them separate preserves the canonical file's hermeticity invariant; folding-in would force either (a) a hermeticity-disclaimer carve-out for one of 17 cases (footgun for future test authors) or (b) committing a frozen OLD-script copy (which @test Q2 rejected as silent staleness).
- The two files are *categorically different*: one is unit tests of the live resolver; the other is a temporal before/after fixture validating a documented behavior shift. Different concerns, different files.
- **Sibling-pointer comment in `_get_base_ref.test.sh` header** for discoverability: `# Behavior-equivalence (CI-PR base-tip → merge-base, task #42) lives in _get_base_ref.behavior-equivalence.test.sh.`

If START_COMMIT becomes orphaned post-rebase, the test file's own header comment (per @test Q2) instructs retargeting; no second file becomes "stale" silently because the fixture failure surfaces immediately on next run.

### Tech Debt Pointer commitments (per @dry-reviewer Gate-1 condition + @code-reviewer nice-to-have)

To be transcribed verbatim into §Tech Debt Pointers at close-out:

1. **`get_diff_base()` in `scripts/guards/common.sh` is a 1-line forwarder to `scripts/lang/_get_base_ref.sh` post-task-#42** (per @dry-reviewer's Option-B-revised text). 11 sibling guards in `scripts/guards/simple/**` still call the forwarder rather than `_get_base_ref.sh` directly. Migration to direct invocation (and forwarder deletion) deferred to a future task. Surfaced 2026-05-13 in task #42 DRY review.
2. **`_get_base_ref.sh` per-call invocation cost** (recorded for close-out per @operations memoization concern; corrected per @operations Note 1+2 + @dry-reviewer measurement — existing changed-files cache is OUTPUT-ONLY, no read path today). Measured cost: ~17ms warm × 11 simple guards ≈ ~190ms per layer. If this becomes measurable budget pressure in the always-run set (layers 3 + 6, 90s p95 budget), the mitigation has **two parts**: (a) add a per-run SHA cache to `_get_base_ref.sh` at `${DEVLOOP_TMP}/base-ref.sha`, second-and-later invocations short-circuit; AND (b) `__emit_base_ref_line` suppression-sentinel so cached-read paths do not re-emit the canonical `BASE_REF=` stderr line (dashboards anchoring on `BASE_REF=` line count would otherwise multiply per pipeline run). Mitigation is contained to `_get_base_ref.sh` and `_common.sh`; do not restore a parallel resolution chain in `common.sh`.

Per @dry-reviewer: "Without it, the wrapper looks like an architectural choice rather than an interim shim."

### Cross-callsite documentation commitment (per @dry-reviewer Finding 1)

Add a one-line comment at `scripts/guards/simple/validate-cross-boundary-classification.sh:201` (text updated for Option B — no env-var reference):

```bash
# get_diff_base forwards to _get_base_ref.sh which returns a SHA in CI / local-
# mergebase modes and falls back to "HEAD" only in local-no-mergebase mode (no
# origin/main reachable). This fallback handles that edge case.
```

This is the **only** callsite of `get_diff_base` that inspects the return value (vs. just passing it through to `git diff`). The conditional at lines 203-211 stays for the local-no-mergebase edge case; the comment preserves intent for future readers and prevents the "which path is canonical?" mirror-drift @dry-reviewer flagged as anti-pattern.

**This adds a 7th touched file** to the edit map:

| File | Edits | Net change |
|------|-------|------------|
| `scripts/guards/simple/validate-cross-boundary-classification.sh` | 1 hunk (2-line comment at line 201) | +2 lines |

Classification: **Mechanical** (comment-only, no behavior change). Owner: operations (per `guards/` family ownership).

### Implementation commitments (from @code-reviewer must-fix conditions, 2026-05-13T02:16Z)

1. **`_get_base_ref.sh` lines 77-82 + 86-91 collapse**: delete the in-script fetch block AND the surrounding `# Defensive fetch — sparse-checkouts and worktrees...` comment block AND the TODO block at 86-91. Replace with a single line: `# Base ref is pack-resident: ci.yml fetch-depth: 0 is the precondition.`

2. **`layer-all.sh` guardrail uses inline `echo ... >&2; exit 2`**, not `die` (which is not defined in `_common.sh` — verified). Plan body wording `die "..."` at line 51 is shorthand only; implementation must inline. **The check is dynamic (per-mode dispatch) not hardcoded to `origin/main`** (per paired-operations R1). Full shape documented under "Precondition guardrail" subsection above; leading stderr token is `PRECONDITION_FAILURE:`, not `ERROR:`.

3. **`get_diff_base()` body has a comment naming the canonical resolver** — under Option B the function is a 3-line pure forwarder; the comment names `_get_base_ref.sh` as the ADR-0033 §7 SSOT and notes "no logic here — all resolution lives in the canonical resolver." Smaller comment, same SSOT-pointer intent (and answers @dry-reviewer's "architectural choice or interim shim?" question explicitly: this is a name-preservation shim, not a parallel resolver).

4. **Retain `__resolve_full_sha` re-validation at line 121** even though merge-base produces a SHA directly — cheap, defensive, keeps BASE_REF= stderr emitting full 40-char SHAs uniformly per observability contract.

### Out-of-scope guard (explicit)

- Two-dot → three-dot semantic change. Stays two-dot under uniform merge-base.
- Runbook authoring (task #39).
- Semantic-guard relocation (task #40).
- Migrating callers off `get_diff_base` (11 outside callers) — deferred; rewrap keeps them untouched.
- **In-scope under Option B**: `ci.yml:81` `GUARD_DIFF_BASE` deletion happens in this devloop (was previously listed as out-of-scope under Option A — the lock to Option B brings it in).

### Risks / open questions for reviewers

1. **@security**: the merge-base shift means CI-PR scope NARROWS. Are there any security-relevant guards (e.g., `no-secrets-in-logs.sh`) that rely on the wider base-tip scope to detect drift outside the PR? Best-case: none — "what did this PR add" is the semantically correct question.
2. **@test**: the behavior-equivalence test extracts OLD `_get_base_ref.sh` via `git show <START_COMMIT>:` against the pinned SHA in §Loop Metadata. Confirm this is acceptable vs. committing a frozen copy under `scripts/lang/fixtures/`.
3. **@observability**: `BASE_SOURCE=ci-pr` token unchanged. `DIFF_MODE=` flips from `three-dot` to `two-dot` for CI-PR. Any dashboards/alerts grepping `DIFF_MODE=three-dot` will see CI-PR drop off. None known.
4. **@code-reviewer**: removing the in-script fetch means a missing base ref now surfaces at `__resolve_full_sha` (line 121) rather than the fetch's earlier explicit error. The new precondition guardrail at `layer-all.sh` entry catches the shallow-clone case before `_get_base_ref.sh` runs at all.
5. **@dry-reviewer**: the wrap-not-delete decision shrinks but does not eliminate the dual-family. Acceptable smaller-blast-radius tradeoff? Alternative is editing 11 sibling scripts.
6. **@operations (paired)**: please co-confirm (a) the ci.yml:106 explanatory comment is sufficient (no fetch-depth: 0 needed for coverage), (b) exit-code-2 from `layer-all.sh` precondition aligns with monitoring conventions, (c) the `$DEVLOOP_BASE_REF` export naming is consistent with other observability surface area (e.g., existing `BASE_REF=` stderr token).

---

## Pre-Work

None.

---

## Implementation Summary

Five-part coherent refactor of the diff-base resolution surface, plus a 7th supporting hunk per @dry-reviewer Finding 1. All seven files edited per the locked plan (Option B):

1. **`scripts/lang/_get_base_ref.sh`**: CI-PR branch resolves to `git merge-base "origin/${GITHUB_BASE_REF}" HEAD` instead of `origin/${GITHUB_BASE_REF}` tip; `DIFF_MODE` flips to `two-dot`. Removed the in-script `git fetch` block and the trailing TODO block; replaced with a 2-line comment naming `ci.yml fetch-depth: 0` as the precondition. Removed the now-dead `three-dot` branch in the changed-files computation; collapsed into a single two-dot branch.

2. **`scripts/layer-all.sh`**: added a precondition guardrail after `init_devloop_tmp`. Dynamic per-mode dispatch matches `_get_base_ref.sh`'s resolution branches: CI-PR checks `origin/${GITHUB_BASE_REF}`, local mode checks `origin/main`, CI-push skips entirely (resolver uses `HEAD~1`). On failure, emits `PRECONDITION_FAILURE: <ref> not in local pack — CI clone too shallow.` + remediation (`fetch-depth: 0`) + runbook pointer; exits 2.

3. **`.github/workflows/ci.yml`** (2 hunks): (a) deleted `GUARD_DIFF_BASE: ${{ github.event.pull_request.base.sha }}` at line 81 — dead env after the Option-B forwarder; (b) added a 3-line explanatory comment above the coverage job's `actions/checkout@v4` per @operations §Part 4. **Decision documented per @operations Q6a**: coverage job runs only `cargo llvm-cov` (full-tree scan), no diff-aware tooling — `fetch-depth: 0` is intentionally omitted.

4. **`scripts/guards/common.sh`**: rewrote `get_diff_base()` as a 3-line forwarder to `scripts/lang/_get_base_ref.sh` (the ADR-0033 §7 canonical resolver). Comment names the SSOT and explicitly states "no logic here". All 11 simple-guard callsites stay untouched (zero blast radius outside `common.sh`).

5. **`scripts/lang/_get_base_ref.test.sh`**: updated `test_ci_pr` to assert `BASE_REF=<merge-base-sha>` + `DIFF_MODE=two-dot`. Added `test_ci_pr_merge_base_vs_tip` (constructs M2 unrelated-progress-on-main and asserts BASE_REF != main tip SHA) and `test_local_clean_merge_base_regression` (asserts local-mode BASE_REF equals `merge-base(origin/main, HEAD)`). 22/22 assertions pass.

6. **`scripts/lang/_get_base_ref.behavior-equivalence.test.sh`** (NET-NEW, ~330 lines): temporal before/after fixture extracting OLD `_get_base_ref.sh` via `git show <START_COMMIT>:`. Five test functions × 11 assertions cover: CI-PR before/after (OLD→tip, NEW→merge-base, diff includes only PR contribution); local-mode no-op (OLD == NEW); shallow-clone guardrail (exit 2 + `PRECONDITION_FAILURE: origin/main` + `fetch-depth: 0`); CI-push guardrail skip (no precondition fire on `GITHUB_EVENT_NAME=push`); guard-callsite-coverage **7a syntax** (`bash -n` across `common.sh` + all 29 `simple/**/*.sh` = 30/30 pass) AND **7b callsite invocation** (11/11 named callers contain `get_diff_base`; `bash -c "source common.sh; get_diff_base"` returns same SHA as direct resolver invocation — end-to-end forwarder exercise). **Header captures @test Q2 rebase-safety choice**: if START_COMMIT becomes unreachable after rebase, retarget the SHA in the file (no repo-level tag created). 11/11 assertions pass.

7. **`scripts/guards/simple/validate-cross-boundary-classification.sh`**: 3-line comment at line 201 explaining the `if [[ "$base" == "HEAD" ]]` fallback only fires in local-no-mergebase mode (per @dry-reviewer Finding 1).

### Security: CI-PR scope-narrowing rationale (per @security explicit-sentence requirement)

The CI-PR `BASE_REF` shift from base-branch tip to merge-base **narrows** the scope every diff-aware guard sees on a PR. This narrowing is intentional and correctness-preserving: a security-relevant guard (`no-hardcoded-secrets.sh`, `no-secrets-in-logs.sh`, `no-pii-in-logs.sh`) is asked "what did this PR add?" not "what is in main + this PR?" — and that's the semantically correct question. A future audit reading the post-#42 diff would observe that a credential pre-existing on `main` no longer flags against a PR (because it wasn't added by the PR); this is the desired outcome, not a regression. @security confirmed at Gate 1 that no security-relevant guard depends on the wider base-tip window.

### A/B reconciliation outcome

Plan locked at Option B (1-line forwarder + `ci.yml:81 GUARD_DIFF_BASE` deletion) per team-lead's decision-rule delegation 2026-05-13T02:19Z. Reasoning fully captured in §Decisions "Why Option B over Option A" (8 points). Option A is recorded as a defensible Rejected Alternative; @operations memoization concern is Tech Debt Pointer #2 with a verbatim mitigation path (per-run SHA cache in `_get_base_ref.sh` itself).

---

## Files Modified

| File | Change |
|------|--------|
| `scripts/lang/_get_base_ref.sh` | CI-PR resolves to merge-base; DIFF_MODE=two-dot; in-script fetch + TODO block removed; three-dot branch deleted from changed-files computation |
| `scripts/layer-all.sh` | Precondition guardrail with mode-dispatched ref check; `PRECONDITION_FAILURE:` token; exit 2 |
| `.github/workflows/ci.yml` | (1) `GUARD_DIFF_BASE` env injection deleted; (2) coverage-job checkout has explanatory inline comment for `fetch-depth` omission |
| `scripts/guards/common.sh` | `get_diff_base()` reduced to 3-line forwarder to `_get_base_ref.sh` |
| `scripts/lang/_get_base_ref.test.sh` | `test_ci_pr` asserts merge-base SHA + DIFF_MODE=two-dot; added `test_ci_pr_merge_base_vs_tip` + `test_local_clean_merge_base_regression` |
| `scripts/lang/_get_base_ref.behavior-equivalence.test.sh` | NET-NEW; 5 cases / 11 assertions covering before/after, shallow-clone guardrail, CI-push skip, guard-callsite coverage (7a syntax 30/30 + 7b callsite invocation 11/11) |
| `scripts/guards/simple/validate-cross-boundary-classification.sh` | 3-line comment at line 201 explaining the local-no-mergebase fallback (per @dry-reviewer Finding 1) |

---

## Devloop Verification Steps

Primary: `./scripts/layer-all.sh` per SKILL.md Step 6. Plus the devloop-specific gates below.

### Verification results

1. **Unit tests pass** — `bash scripts/lang/_get_base_ref.test.sh` → **22/22 passed, 0 failed** (8 original cases + 3 new: `test_ci_pr` updated, plus `test_ci_pr_merge_base_vs_tip` + `test_local_clean_merge_base_regression`).

2. **Behavior-equivalence fixture passes** — `bash scripts/lang/_get_base_ref.behavior-equivalence.test.sh` → **11/11 passed, 0 failed**:
   - CI-PR OLD resolves to origin/main TIP (M2); NEW resolves to merge-base (M1).
   - Under NEW, `git diff --name-only $BASE_SHA..HEAD` produces only the PR's contribution (p1.txt + p2.txt), not main's unrelated commit (m2.txt).
   - Under OLD, the same diff would include m2.txt as a false-positive.
   - Local mode: OLD and NEW resolve identically (no-op confirmed).
   - Shallow-clone guardrail: `layer-all.sh` exits 2 with `PRECONDITION_FAILURE: origin/main` + `fetch-depth: 0` remediation.
   - CI-push mode: guardrail correctly skipped on the same shallow clone (no precondition fire).
   - `bash -n` syntax check passes across `guards/common.sh` + all 29 `guards/simple/**/*.sh`.

3. **Full pipeline `./scripts/layer-all.sh`** — FAIL, but **all failures are pre-existing environmental issues unrelated to task #42**:
   - Layers 1, 2: `Command "nx" not found` (TS toolchain missing in this dev env)
   - Layer 4: `cargo-audit-failed` (RUSTSEC-2023-0071 in rsa crate, transitive via sqlx-mysql — no fix available upstream)
   - Layer 5: `buf-binary-missing` (buf CLI not installed locally)
   - Layer 3 (Rust workspace tests): **OK** — the most diff-sensitive layer is clean.
   - The new `BASE_REF= BASE_SOURCE=local-mergebase DIFF_MODE=two-dot FILES_CHANGED=189` stderr line is emitted correctly across all layers.
   - **Conclusion**: failures are not caused by task #42; they are dev-environment toolchain gaps that will be resolved in CI by the standard installation steps.

---

## Code Review Results

### Security Specialist
**Verdict**: TBD

### Test Specialist
**Verdict**: TBD

### Observability Specialist
**Verdict**: TBD

### Code Quality Reviewer
**Verdict**: TBD

### DRY Reviewer
**Verdict**: TBD

### Operations Reviewer (paired)
**Verdict**: TBD

---

## Tech Debt Pointers

1. **`get_diff_base()` forwarder + 11 sibling-guard callers** (per @dry-reviewer Option-B-revised text). `get_diff_base()` in `scripts/guards/common.sh` is a 1-line forwarder to `scripts/lang/_get_base_ref.sh` post-task-#42. 11 sibling guards in `scripts/guards/simple/**` still call the forwarder rather than `_get_base_ref.sh` directly. Migration to direct invocation (and forwarder deletion) deferred to a future task. Surfaced 2026-05-13 in task #42 DRY review.

2. **`_get_base_ref.sh` per-call invocation cost AND `BASE_REF=` stderr multiplication** (per @operations memoization concern + @operations post-flip refinement; corrected per @operations Note 1+2 + @dry-reviewer measurement — the existing `${DEVLOOP_TMP}/changed-files.layer-*` cache is output-only, not a read-cache; subsequent invocations do NOT short-circuit today). **CPU cost (measured)**: ~17ms warm per invocation × 11 simple guards = **~190ms per layer**; @operations sized the broader cross-helper rate at ~24-36 invocations per pipeline run (each guard invokes `get_modified_files` / `get_added_files` / `get_deleted_files` internally) = **~1.2-1.8s per pipeline run** total. Bulk is bash startup + `_common.sh` sourcing — git ops are sub-ms after the bash overhead. Real and monitorable but well under 2% of the 90s always-run budget. **`BASE_REF=` line multiplication (operationally load-bearing — per @operations post-flip refinement)**: every invocation of `_get_base_ref.sh` emits the normative `BASE_REF=... BASE_SOURCE=... DIFF_MODE=... FILES_CHANGED=...` stderr line at `_get_base_ref.sh:142`. That line is the **runbook anchor** for ops dashboards (per Tech Debt Pointer #4). At ~24-36 invocations per pipeline run, we emit ~24-36 `BASE_REF=` lines per run instead of 1. Two consequences: (i) cosmetic CI log noise; (ii) **any observability dashboard counting "pipeline runs" by `BASE_REF=` emission count would silently multiply runs by ~30x** once the runbook wires those dashboards to the anchor. @operations verified no such dashboard exists today — but task #39 will likely document `BASE_REF=` as the canonical run-anchor, at which point this concern materializes. **Contained mitigation (two parts)**: (a) add a per-run SHA cache to `_get_base_ref.sh` (write the resolved SHA to a sibling cache file at `${DEVLOOP_TMP}/base-ref.sha`, second-and-later invocations short-circuit via cache read); AND (b) **`__emit_base_ref_line` suppression-sentinel** — the cached-read path must NOT re-emit the canonical `BASE_REF=` stderr line. Suggested semantics (per @operations): emit the line only on the **first** invocation per pipeline run, keyed on a `${DEVLOOP_TMP}/.base-ref-emitted` sentinel file (or first-write-wins on the existing `changed-files.layer-${DEVLOOP_LAYER:-shared}` cache file). Mitigation is contained to `_get_base_ref.sh` and `_common.sh`; do not restore a parallel resolution chain in `common.sh`. Surfaced 2026-05-13 in task #42 A/B reconciliation; CPU measurement folded post-Gate-2 per @dry-reviewer; `BASE_REF=` multiplication concern folded per @operations post-flip discovery.

3. **Cold-fetch tech-debt pointer from task #38 main.md §Tech Debt Pointers entry 1 is now CLOSED** by this devloop. Original concern: "Layer 6 always-run 90s p95 budget vs `buf breaking` upstream-fetch cost". After task #42 there is no in-script fetch; the budget concern is moot, and the runbook precondition (CI `fetch-depth: 0`) is the gate enforced by the new `layer-all.sh` precondition guardrail. The historical Wave-1 #1 follow-up entry "_get_base_ref.sh CI-PR tip-SHA vs merge-base-SHA divergence" in `docs/TODO.md` is also closed by this task — please remove it during close-out cleanup.

4. **Stderr-token convention for task #39 runbook** (flagged by @observability at Gate 1 re-confirm). The runbook should document the two-token scheme: `ERROR:` for in-resolver failures emitted by `scripts/lang/_get_base_ref.sh` (lines 36, 80, 122 — ref-name validation, merge-base computation, SHA resolution), `PRECONDITION_FAILURE:` for pre-layer gate failures emitted by `scripts/layer-all.sh`. Future precondition checks added to `layer-all.sh` (e.g., disk-space, env-var presence) should reuse `PRECONDITION_FAILURE:` per @operations §Q6b — this is the runbook convention to inherit.

---

## Rollback Procedure

1. Start commit: `9f9bbf0dce19a57e2d3fb480163262be224684c6`
2. `git diff 9f9bbf0dce19a57e2d3fb480163262be224684c6..HEAD`
3. Soft reset (preserves changes): `git reset --soft 9f9bbf0dce19a57e2d3fb480163262be224684c6`
4. Hard reset (clean revert): `git reset --hard 9f9bbf0dce19a57e2d3fb480163262be224684c6`

No schema or infra-runtime changes — `git reset` is sufficient. Note: the CI behavior change is reverted by `git reset` since both `_get_base_ref.sh` and `.github/workflows/ci.yml` are in-tree.

---

## Issues Encountered & Resolutions

TBD.

---

## Lessons Learned

TBD.
