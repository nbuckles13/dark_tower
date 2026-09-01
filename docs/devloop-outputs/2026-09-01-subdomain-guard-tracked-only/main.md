# Devloop: validate-subdomain-regex-sync.sh — scan git-tracked files only

## Loop Metadata
- **Slug**: 2026-09-01-subdomain-guard-tracked-only
- **Start Commit**: 83280869082d73d190d9b89d41d6e9e1486fd774
- **Branch**: feature/hear-yourself-through-handler
- **Lead Model**: claude-opus-4-8[1m]
- **Specialist**: operations
- **Mode**: light (3 teammates: implementer + security + test)

## Loop State
| Reviewer | Verdict |
|----------|---------|
| security | RESOLVED-FIXED |
| test | CLEAR |

## Phase
complete

## Task

Fix `scripts/guards/simple/validate-subdomain-regex-sync.sh` so its repo-wide sweep
scans **git-tracked files only** (`git ls-files`), making it immune to gitignored build
artifacts (`packages/sdk-core/coverage/**/limits.ts.html`, `.nx/cache/**` copies) that
today add phantom hits and red the guard 15-vs-10 while all 7 enumerated SSoT sites are OK.
This blocks every task of the ADR-0036 story at Layer 3.

### Requirements
1. Drive the sweep from `git ls-files` (tracked-only) — NOT a `--exclude-dir` denylist
   (rejected as whack-a-mole per docs/TODO.md analysis; does nothing for the exact-equality
   false-negative either).
2. Keep the pinned-count exact-equality check (`EXPECTED_TOTAL_OCCURRENCES=10`) and the 7
   enumerated SSoT site checks unchanged in semantics.
3. Single-path shape: the guard's tested path must equal its production path. Do NOT branch
   the scan (git ls-files for real root, find/grep for the seam) — that reintroduces the
   "a green test proves *a* path ran, not *the* path" defect class.
4. Rework the self-test seam in the same change: the fixture builds a synthetic tree with NO
   `git init`, so a naive `git ls-files` returns nothing → total=0 → vacuity branch fires →
   every case reds. Either `git init` + `git add -A` the fixture, or add an injectable
   file-lister seam following the DEVLOOP_TEST sentinel idiom (exact-match semantics; see the
   seam-sentinel entry in docs/TODO.md line ~45). Keep all 8 existing self-test cases green,
   including the injected-8th-encoding red case (case 4) and the vacuity case (case 6).
5. Mark BOTH docs/TODO.md entries resolved with date + pointer per the resolved-entry style:
   - line ~202 ("considered + REVERTED" analysis under the two-DNS-label-families neighbourhood)
   - the "Guard Precision — validate-subdomain-regex-sync scans gitignored build artifacts"
     section entry (line ~1226-1228)
6. Do NOT touch any other guard.

### Verification
- Guard green on a tree that has just run `pnpm test` (coverage/.nx artifacts present).
- Guard red when an 8th regex occurrence is planted in a TRACKED file.
- Pinned count unchanged (still 10 over tracked source).
- All self-test cases green.

## Iterations

### Iteration 1 — implementation + Gate 2
- **Shape chosen**: (a) — git-init the self-test fixture; NO new production seam, guard stays genuinely single-path.
- **Guard**: step-(3) sweep replaced `grep -rn --exclude-dir` with `git -C "$REPO_ROOT" grep -I -n -F -e "$CANONICAL" -- ':/' :(exclude)…`. SWEEP_EXCLUDES now git `:(exclude)` pathspecs (audit-trail carve-out + self-exclusion). Stale grep option-order header comment replaced; vacuity branch now also names non-worktree.
- **Self-test**: mk_tree `git init -q` + `git add -A`; case 4 re-stages the planted 8th-encoding file; case 6 inits empty tree.
- **docs/TODO.md**: both entries marked RESOLVED 2026-09-01 → slug `2026-09-01-subdomain-guard-tracked-only`.
- **Gate 2 (Lead-verified)**:
  - Self-test: 22 passed / 0 failed.
  - Real guard: STATUS=OK, 7 sites / 10 occurrences.
  - Artifact-present scenario: guard stays green with untracked `packages/sdk-core/coverage/**` + `.nx/cache/**` copies of the literal planted (tracked-only by construction).
  - **No regression**: failing-guard set on `feature/hear-yourself-through-handler` is byte-identical with and without this diff — 32 pre-existing failures (api-version-check, no-*-secrets, test-coverage, validate-*, etc.), all unrelated WIP of the ADR-0036 story, none is the subdomain guard. Interactive `layer-all.sh` would fail-fast at Layer 3 on those pre-existing failures; this devloop's target guard passes and introduces zero new failures.
  - Only 3 intended files touched.

### Iteration 2 — security findings (S1-S4) + Gate 3
- **S1 (fix)**: dropped `':/'` from the `git grep` — scan and `:(exclude)` carve-outs now share ONE anchor (REPO_ROOT), matching the old `grep -rn "$REPO_ROOT"` scoping. Under the seam (REPO_ROOT ≠ git toplevel) the mixed anchoring previously evaporated the audit-trail carve-out (count 14).
- **S2 (fix)**: added a `git rev-parse --show-toplevel` + `-ef` identity precondition; sweep nested in its `else`, so "cannot look" (non-worktree) and "looked, found nothing" (vacuity) are mutually exclusive and each names its cause. `-ef` load-bearing (rejects the subdirectory case) — documented inline per security's durable note.
- **S3/S4 (notes, applied)**: header comment states the staged-vs-unstaged coverage trade; `git add -Af` in mk_tree/case 4 for host-gitignore hermeticity.
- **New self-test case 9**: fail-closed on a non-git root with all 10 occurrences physically on disk (a filesystem walk would count them) — reds on the precondition, asserts STATUS=OK absent.
- **Gate 3 verdicts**: Security **RESOLVED-FIXED** (all 4 independently re-verified), Test **CLEAR** (rev 2; case 9 and case 4 both mutation-proven load-bearing). Self-test 26/0; real guard OK 7/10, green with artifacts planted; 3 files touched.

## Accepted Deferrals
- (none surfaced in this devloop)
