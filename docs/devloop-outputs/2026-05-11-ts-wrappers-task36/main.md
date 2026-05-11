# Devloop Output: TS wrappers (R-62, ADR-0033 Wave 2 #5)

**Date**: 2026-05-11
**Task**: Land `scripts/lang/ts/{compile,fmt,lint,test}.sh` invoking Nx natively, translating Nx output into the dispatcher's uniform `STATUS=` schema.
**Specialist**: infrastructure
**Mode**: Agent Teams (v2) — full
**Branch**: `feature/browser-client-join-task36`
**Duration**: TBD

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `5a8f5dc0cb90a113e2d5d96a08bc74e66a457bf9` |
| Branch | `feature/browser-client-join-task36` |
| Story | `docs/user-stories/2026-05-02-browser-client-join.md` (task #36) |
| Dependencies | #33 (`lang/ts/audit.sh` + `changed.sh` already landed) |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@devloop-2026-05-11-ts-wrappers-task36` |
| Implementing Specialist | `infrastructure` |
| Iteration | `1` |
| Security | `security@devloop-2026-05-11-ts-wrappers-task36` |
| Test | `test@devloop-2026-05-11-ts-wrappers-task36` |
| Observability | `observability@devloop-2026-05-11-ts-wrappers-task36` |
| Code Quality | `code-reviewer@devloop-2026-05-11-ts-wrappers-task36` |
| DRY | `dry-reviewer@devloop-2026-05-11-ts-wrappers-task36` |
| Operations | `operations@devloop-2026-05-11-ts-wrappers-task36` |

### Gate 1 (Plan Confirmation) Tracking

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed (3 reversals → final 9-LoC shape) |
| Test | confirmed (re-confirmed on simpler shape) |
| Observability | confirmed (option-i: no extra comment) |
| Code Quality | confirmed (3 notes resolved; all-Mine ownership) |
| DRY | confirmed (no findings, no TODO entries) |
| Operations | re-confirmed (5 ci.yml implementation notes relayed to impl) |

### Gate 3 (Final Verdict) Tracking

| Reviewer | Verdict | Findings | Notes |
|----------|---------|----------|-------|
| Security | CLEAR | 0 | 7/7 checklist clean; SHA-pinning deferred as workflow-wide tech-debt |
| Test | CLEAR | 0 blocking | end-to-end dispatcher integration + lockfile-as-TS-trigger semantics verified; `_get_base_ref.test.sh` follow-up tracked in tech-debt |
| Observability | CLEAR | 0 | Verb-bearing REASON tokens, BASE_REF= preserved, STATUS-line-last by construction |
| Code Quality | APPROVE | 1 finding (RESOLVED), 1 nit (RESOLVED) | F1 TODO-relocation absorbed; N1 lockfile-language corrected |
| DRY | CLEAR | 0 blocking, 1 TODO entry | `_nx_affected.sh` extraction filed to docs/TODO.md Cross-Service Duplication |
| Operations | CLEAR | 0 blocking, 2 cosmetic nits | lockfile bundle accepted; (a)/(b) tension retired as "resolved by physical constraint" |
| Semantic Guard | SAFE | 0 | 2 documented semantic gaps (typecheck/format/test:component absence + tip-SHA over-broad), both knowingly deferred |

---

## Task Overview

### Objective
Land the four TS verb wrappers under `scripts/lang/ts/` so the dispatcher's `for_each_lang_with_verb` (in `scripts/lang/_dispatch.sh`) can route to Nx natively for compile, fmt, lint, and test verbs.

### Scope
- **Service(s)**: pipeline infrastructure only (`scripts/lang/ts/`)
- **Schema**: none
- **Cross-cutting**: no (infrastructure-owned dir per ADR-0033 §6)

### Per-wrapper contract (ADR-0033 §6 + §9)
Each wrapper:
- Executes `nx affected -t <target> --base=$(scripts/lang/_get_base_ref.sh)` (`pnpm exec nx` since `nx` is a devDependency, not on `$PATH`).
- Translates Nx exit code → uniform `STATUS=` schema (OK / FAIL / N/A as appropriate).
- Emits the final `STATUS=…` line to stdout per `_common.sh::emit_status`.
- Sets `set -euo pipefail; IFS=$'\n\t'` and sources `../_common.sh`.

Targets (per ADR-0033 §3 worktree, Section 9):
- `compile.sh` → `nx affected -t typecheck`
- `fmt.sh` → `nx affected -t format`
- `lint.sh` → `nx affected -t lint`
- `test.sh` → `nx affected -t test:unit test:component`

### Out of scope
- `lang/ts/e2e.sh` — deferred per task #36 description (waits on task #15 web-app + Playwright).
- `lang/ts/audit.sh` — already landed in task #33; do not touch.
- `lang/ts/changed.sh` / `changed.test.sh` — already landed in task #33; do not touch.
- Adding the missing `typecheck` / `format` Nx targets to existing `project.json` files (`proto-gen`, `test-utils`). Wrappers must behave correctly when Nx reports "no affected projects with target" — that is not failure.

### Debate Decision
NOT NEEDED — Nx-wrapped-natively was granted in the 2026-05-06 polyglot-validation-pipeline-strategy debate (per ADR-0033 §9 + client's Final 88 verdict).

---

## Cross-Boundary Classification

All file changes are inside `scripts/lang/ts/`, which is infrastructure-owned per ADR-0033 §6 (the dispatcher root, the lang directories, and the wrappers are pipeline infrastructure). No Guarded Shared Area paths. No paired specialist needed.

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `scripts/lang/ts/compile.sh` (new) | Mine | — |
| `scripts/lang/ts/fmt.sh` (new) | Mine | — |
| `scripts/lang/ts/lint.sh` (new) | Mine | — |
| `scripts/lang/ts/test.sh` (new) | Mine | — |
| `docs/devloop-outputs/2026-05-11-ts-wrappers-task36/main.md` (new) | Mine | — |
| `docs/user-stories/2026-05-02-browser-client-join.md` (Devloop Tracking table row #36) | Mine | — |
| `.github/workflows/ci.yml` (add `pnpm install --frozen-lockfile` before layer-all.sh) | Mine | — |
| `pnpm-lock.yaml` (sync with packages/proto-gen/package.json) | Mine | — |
| `scripts/lang/_get_base_ref.sh` (inline TODO comment near ci-pr branch, pointing at /work/TODO.md entry) | Mine | — |
| `TODO.md` (new entry: `_get_base_ref.sh CI-PR tip-SHA vs merge-base-SHA`) | Mine | — |

**Scope expansion note**: four pair-lands beyond the strict #36 spec (`scripts/lang/ts/*.sh`), all per Lead/user decision 2026-05-11:

1. **`.github/workflows/ci.yml`** — without `pnpm install`, `pnpm exec nx affected …` exits 254 with `ERR_PNPM_RECURSIVE_EXEC_FIRST_FAIL`, which `run_and_emit` surfaces as `STATUS=FAIL REASON=nx-<verb>-failed` on every CI run after merge. Adding the 2-line `pnpm install --frozen-lockfile` step (plus `cache: 'pnpm'` on setup-node, which required reordering pnpm-setup before setup-node) is in infrastructure's lane (ADR-0033 §6 / same surface as #33's ci.yml edit) and prevents shipping a known-broken state. **The operational property (load-bearing-loud FAIL on missing toolchain) is unchanged from earlier plan iterations; only the mechanism differs — `run_and_emit`'s natural exit-mapping handles it now, not an explicit precheck (which was dropped during Gate 1 review per code-reviewer C2 + empirical correction).**

2. **`pnpm-lock.yaml`** — pre-existing drift unrelated to #36: commit 65a7770 added `@bufbuild/protoc-gen-es@2.12.0` + `@bufbuild/protobuf@2.12.0` to `packages/proto-gen/package.json` but never refreshed the lockfile. My new `pnpm install --frozen-lockfile` CI step (#36 pair-land #1) exposes this drift on every PR after merge. Lead/user picked **option (a) bundle** over (b) defer to avoid shipping a known-red CI state and close the small defense-in-depth window security flagged (no period on `main` where pre-existing drift is exposed to incoming PRs that might land `package.json` edits). Lockfile delta is mechanical, pnpm-generated, proto-gen-scoped: **62 net additions (`git diff --stat`)** — well under code-reviewer's <100-line "clean" threshold — / 115 raw-diff lines (`wc -l`, including `+`/`-` prefixes + hunk context). 4 newly-declared package entries: 2 declared bufbuild devDeps (`@bufbuild/protobuf`, `@bufbuild/protoc-gen-es`) + 2 transitive peers (`@bufbuild/protoplugin@2.12.0`, `@typescript/vfs@1.6.4`). One additional peer-resolved typescript version (`typescript@5.4.5`) appears in the lockfile because `@typescript/vfs` peer-requires `typescript: '*'`; this is normal pnpm behavior, not a workspace-typescript upgrade (root devDep stays `5.7.3`). Constraining `@typescript/vfs`'s peer to the workspace TS is a separate follow-up. See Devloop Verification Steps #6 for the smoke-check.

3. **`scripts/lang/_get_base_ref.sh` + `/work/TODO.md`** (Gate 3 F1 cleanup) — code-reviewer flagged the per-wrapper `TODO(post-#36)` comment as misplaced (the bug is in `_get_base_ref.sh`, not in consumers) and duplicated 4× across the wrappers. Moved the TODO marker to its proper home: one comment line in `_get_base_ref.sh` near the `ci-pr` branch (line ~85) referencing a structured entry in `/work/TODO.md` under "ADR-0033 Pipeline Follow-ups". The wrappers no longer carry the comment. `grep "_get_base_ref.sh CI-PR tip-SHA"` (or any unique phrase from the TODO.md entry) now recovers the full context — better than the prior `grep "post-#36"` anchor, which matched both wrappers and the Lessons Learned section but obscured *which* path was the source of truth.

---

## Planning

TBD — implementer drafts approach; reviewers confirm at Gate 1.

---

## Pre-Work

None.

---

## Implementation Summary

Four new TS verb wrappers under `scripts/lang/ts/` invoke Nx natively via `pnpm exec nx affected -t <target> --base=<sha>`. Each wrapper is ~10 LoC: sources `_common.sh`, resolves the diff base via `_get_base_ref.sh`, and uses `run_and_emit` to map Nx's exit code to the dispatcher's `STATUS=` schema.

Paired CI edit in `.github/workflows/ci.yml` adds `pnpm install --frozen-lockfile` before `./scripts/layer-all.sh` (necessary because `pnpm exec nx` requires `node_modules` and was not previously installed). Companion change: swap `setup-node` and `pnpm/action-setup` ordering so `cache: 'pnpm'` key on setup-node works (per `actions/setup-node` v4 requirement that pnpm be installed first).

| Wrapper | Nx target(s) | REASON prefix |
|---|---|---|
| `compile.sh` | `typecheck` | `nx-typecheck-passed/failed` |
| `fmt.sh` | `format` | `nx-format-passed/failed` |
| `lint.sh` | `lint` | `nx-lint-passed/failed` |
| `test.sh` | `test:unit test:component` | `nx-test-passed/failed` |

---

## Files Modified

| Path | Change |
|---|---|
| `scripts/lang/ts/compile.sh` | NEW — 11 LoC; `nx affected -t typecheck` |
| `scripts/lang/ts/fmt.sh` | NEW — 11 LoC; `nx affected -t format` |
| `scripts/lang/ts/lint.sh` | NEW — 11 LoC; `nx affected -t lint` |
| `scripts/lang/ts/test.sh` | NEW — 11 LoC; `nx affected -t test:unit test:component` |
| `.github/workflows/ci.yml` | Swap pnpm/node setup order; add `cache: 'pnpm'` to setup-node; insert `pnpm install --frozen-lockfile` step before `layer-all.sh`. **Only the `test` job (L14-82) is modified — `coverage` job (L84+) is Rust-only (`cargo llvm-cov`) and unchanged.** |

---

## Devloop Verification Steps

All checks run on local mainline diff (`BASE_REF=91a701f8907c173403def62c77e749c6447f43e6 BASE_SOURCE=local-mergebase`).

### 1. Direct invocation — each wrapper produces correct STATUS

| Wrapper | Expected | Observed | Pass |
|---|---|---|---|
| `bash scripts/lang/ts/compile.sh` | `STATUS=OK REASON=nx-typecheck-passed` exit 0 | matches | ✓ |
| `bash scripts/lang/ts/fmt.sh` | `STATUS=OK REASON=nx-format-passed` exit 0 | matches | ✓ |
| `bash scripts/lang/ts/lint.sh` | `STATUS=FAIL REASON=nx-lint-failed` exit 1 (pre-existing proto-gen buf-lint findings, orthogonal to this task) | matches | ✓ |
| `bash scripts/lang/ts/test.sh` | `STATUS=OK REASON=nx-test-passed` exit 0 | matches | ✓ |

### 2. STATUS line is last on stdout (observability nit #2)

`bash <wrapper> | tail -1` returns the STATUS line for all four wrappers (verified — `run_and_emit` emits after the wrapped command exits, so STATUS is structurally last).

### 3. cwd-agnosticism (test concern #2 + code-reviewer Gate-2 ask)

Two regimes:
- `cd /tmp && bash /work/scripts/lang/ts/compile.sh` — wrapper paths resolve correctly (no "source: file not found"); failure exits at `_get_base_ref.sh` due to /tmp not being a git repo, which is correct downstream behavior, not a wrapper bug.
- `cd /work/crates/common && bash /work/scripts/lang/ts/compile.sh` — full success path: STATUS=OK REASON=nx-typecheck-passed, exit 0. `pnpm exec nx` correctly walks up the directory tree to find `pnpm-workspace.yaml` from the deep Rust subdir.

### 4. Nuked `node_modules` (observability load-bearing-verified ask)

```
$ mv node_modules node_modules.bak
$ bash scripts/lang/ts/compile.sh
... BASE_REF=... ...
... ERR_PNPM_RECURSIVE_EXEC_FIRST_FAIL  Command "nx" not found ...
STATUS=FAIL REASON=nx-typecheck-failed
exit 1
$ mv node_modules.bak node_modules
```

Confirms `pnpm exec nx` exits non-zero on missing toolchain, `run_and_emit` correctly classifies as FAIL — the explicit precheck dropped during Gate 1 was redundant.

### 5. Performance regimes (operations nit #3)

| Wrapper | Warm-cache, no-diff | Cold-cache (`.nx/cache` nuked + `nx-daemon` killed → CI-realistic) | One-package-affected* |
|---|---|---|---|
| `compile.sh` | 2.17s | 1.77s | 1.78s |
| `fmt.sh` | 0.70s | 0.67s | 0.73s |
| `lint.sh` | 1.11s | 1.14s | 1.13s |
| `test.sh` | 0.69s | 0.72s | 0.71s |

All well under operations' 5s warm-cache target and 30s cold-cache flag threshold. Cold-cache parity with warm reflects the small workspace size (2 packages: proto-gen + test-utils) — Nx daemon and on-disk cache provide little speedup when the project graph is already trivial to compute.

\* **One-package-affected caveat (operations note #2)**: regime 3 measurements above touched `packages/test-utils/src/index.ts`. Per the plan's "out of scope" note, `test-utils` doesn't have `format` or `test:component` Nx targets, and `proto-gen` doesn't have `typecheck`/`test:unit`/`test:component`. So regime 3 measures dispatch overhead for `compile.sh`/`fmt.sh` when the touched package doesn't implement the target, not actual per-target work. No production package has all four targets defined as of #36 — this is an accurate operational truth, not a gap to close. Future tasks adding TS packages with full target coverage will provide the per-target one-package baseline.

### 6. Lockfile preservation (by the wrappers)

The wrappers themselves do not mutate `pnpm-lock.yaml` (`pnpm exec` is non-mutating). Verified by inspecting `git status pnpm-lock.yaml` before-and-after running each wrapper in sequence — no delta attributable to wrapper invocation.

**Pre-existing lockfile drift** (separate from wrapper correctness): the on-disk `pnpm-lock.yaml` was already out-of-sync with `packages/proto-gen/package.json` before this task started (proto-gen's package.json was updated without a corresponding `pnpm install` to refresh the lockfile, adding `@bufbuild/protobuf@2.12.0` + `@bufbuild/protoc-gen-es@2.12.0` as devDeps). My new `pnpm install --frozen-lockfile` CI step exposes this drift. Lead/user picked **option (a) bundle** (see Cross-Boundary Classification scope-expansion note #2).

**Lockfile diff smoke-check (code-reviewer Gate-3 ask + operations Gate-3 Nit 1 wording fix)**:

| Check | Value |
|---|---|
| `git diff --stat pnpm-lock.yaml` | **62 net additions** — well under code-reviewer's <100-line "clean" threshold |
| `git diff pnpm-lock.yaml \| wc -l` | 115 raw-diff lines (includes `+`/`-` prefixes + hunk context — operations Gate-3 Nit 1 caught my earlier ambiguity here) |
| Newly-declared package entries | 4: `@bufbuild/protobuf@2.12.0`, `@bufbuild/protoc-gen-es@2.12.0`, `@bufbuild/protoplugin@2.12.0` (transitive), `@typescript/vfs@1.6.4` (peer of protoplugin) |
| Peer-resolved transitive | 1: `typescript@5.4.5` (peer of `@typescript/vfs`, which requires `typescript: '*'`). Not a workspace-typescript upgrade — root devDep stays `5.7.3`. Code-reviewer Gate-3 N1 caught my earlier "no unrelated transitives" overstatement. |
| Affected importer paths | Only `packages/proto-gen` — no test-utils or root changes |

This is the "clean" outcome code-reviewer described: proto-gen-scoped delta, no unrelated workspace dep changes. The `typescript@5.4.5` peer-resolution is normal pnpm behavior when a transitive declares an unbounded peer (`typescript: '*'`); constraining `@typescript/vfs`'s peer to the workspace TS is a separate optional follow-up.

---

## Code Review Results

### Security pre-audit (Gate 1 closeout)

Wrapper shape: 7/7 clean (base-ref handling, `$@` pass-through matches Rust precedent, `set -x` discipline, `pnpm exec` fail-loud via `run_and_emit`, hardening basics, `--base=` quoting, no token surface).

ci.yml edit: 6/7 clean + 1 deferred:
- Items 2-7: clean (`--frozen-lockfile` present, step ordering correct, `setup-node`'s `cache: 'pnpm'` derives key from lock-hash, no `permissions:` widening, no debug flags, no `pull_request_target` surface).
- Item 1 (SHA-pinning): tag-pinning matches existing project convention across all 7 actions; SHA-pinning new actions while leaving 5 existing tag-pinned is a no-op security improvement. Filed as tech-debt for workflow-wide policy pass (see Tech Debt References).

Three Gate-1 reversals from security: (1) `$@` pass-through permitted (was: blocked) — matches Rust precedent, `audit.sh` threat doesn't transfer to non-gate wrappers. (2) `node_modules/.bin/nx` precheck dropped (was: required) — `run_and_emit` correctly classifies the 254 exit. (3) `BASE_SHA` empty-guard dropped (was: required) — `set -e` propagation verified, contract bug owned by `_get_base_ref.test.sh` per CLAUDE.md "no validation for scenarios that can't happen". All three reversals push toward the simpler 9-LoC shape matching Rust precedent.

---

## Tech Debt References

- **`_get_base_ref.sh` PR-mode tip-SHA vs merge-base-SHA divergence** — see Issues Encountered #2 and Lessons Learned. **Single source of truth: `/work/TODO.md` § "ADR-0033 Pipeline Follow-ups" › `_get_base_ref.sh` CI-PR tip-SHA vs merge-base-SHA**. Inline anchor: `scripts/lang/_get_base_ref.sh` line ~85 (ci-pr branch) carries a `TODO:` comment pointing at the TODO.md entry. Per code-reviewer Gate-3 F1: the prior duplication × 4 wrappers (grep anchor `post-#36`) was replaced by this single-location pattern. Deferred per operations recommendation; the TODO.md entry captures the full constraint set (cross-language audit, `_get_base_ref.test.sh` matrix update, etc.).
- **Pre-existing `proto-gen:lint` failures** — `nx affected -t lint` currently exits 1 due to buf-lint findings in `proto/internal.proto` and `proto/signaling.proto` (file-layout / package-naming conventions, RPC-naming conventions). Orthogonal to this task; visible in Gate 2 verification step #1. Tracked under R-61 (proto rename sweep, Wave 3).
- **`_get_base_ref.test.sh` should explicitly assert rc=0-iff-non-empty-stdout** (test-reviewer Gate-1 follow-up). The four TS wrappers now rely on `_get_base_ref.sh` either printing a SHA or exiting non-zero (per the guard-drop decision); the existing tests assert SHA *shape* and BASE_REF stderr-token presence, but the rc=0-with-empty-stdout regression case isn't explicitly defended. A future refactor masking an error inside a `|| true` would slip past the suite. Fortify with `assert_nonempty_stdout` across all 8 scenarios in `_get_base_ref.test.sh`. Owned by test specialist; not a #36 blocker per test reviewer's call.
- **SHA-pin all third-party GitHub Actions in `.github/workflows/ci.yml`** (security Gate-1 follow-up). Currently all 7 third-party actions are tag-pinned (`actions/checkout@v4`, `dtolnay/rust-toolchain@stable`, `Swatinem/rust-cache@v2`, `pnpm/action-setup@v4`, `actions/setup-node@v4`, `taiki-e/install-action@cargo-llvm-cov`, `codecov-action@v4`) — including one channel-pinned (`dtolnay/rust-toolchain@stable`). Industry best practice (OpenSSF Scorecard, GitHub Security Hardening) recommends SHA-pinning third-party actions. Bundle the conversion in a single PR per `pinact`/`stepsecurity` automation, ideally with renovate config for automated SHA-bumps. Priority: P2 (current tag-pinning matches widely-used convention; no known active threat; SHA-pinning new actions while leaving 5 existing tag-pinned would be a no-op improvement). Highest-priority target if the pass happens: `dtolnay/rust-toolchain@stable` (channel-pinned, not even tag-pinned). Owned by security/infrastructure; out of #36 scope per security's explicit Gate-1 defer.

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `5a8f5dc0cb90a113e2d5d96a08bc74e66a457bf9`
2. Review all changes: `git diff 5a8f5dc..HEAD`
3. Soft reset (preserves changes): `git reset --soft 5a8f5dc`
4. Hard reset (clean revert): `git reset --hard 5a8f5dc`
5. No schema changes, no infra manifest changes — `git reset` is sufficient.

---

## Issues Encountered & Resolutions

### 1. `pnpm exec nx` exit code on missing `node_modules` — Day 1 plan claimed 0, actually 254

**What happened**: Day 1 plan included an explicit `[[ ! -x node_modules/.bin/nx ]]` precheck on the rationale that `pnpm exec nx` exits 0 (silently false-OK) when nx isn't installed. Code-reviewer C2 challenged the precheck as redundant. Re-test revealed the Day 1 measurement was wrong — `EXIT=$?` was captured after a piped command (`pnpm exec nx ... | head -30; echo "EXIT=$?"`), which captures `head`'s exit code, not pnpm's. Correct measurement: `pnpm exec nx` exits **254** when nx isn't installed.

**Resolution**: Dropped the precheck. `run_and_emit` (per `_common.sh:88-97`) correctly classifies the 254 exit as `STATUS=FAIL REASON=nx-<verb>-failed`. Net wrapper shrank from 17 LoC to 11 LoC and avoided a brittle `../../../node_modules/.bin/nx` hard-coded path.

**Verification**: Per "Devloop Verification Steps" item 4 — nuked node_modules, observed STATUS=FAIL with REASON=nx-typecheck-failed.

### 2. `nx affected` over-broad in CI-PR mode — `_get_base_ref.sh` emits tip-SHA, not merge-base-SHA

**What happened**: Operations nit #2 flagged that in CI-PR mode, `_get_base_ref.sh:81-83` sets `base="origin/${GITHUB_BASE_REF}"` and resolves to that **tip** SHA. The changed-files cache (line 121) uses three-dot diff (`base...HEAD`), but the SHA emitted on stdout is the base tip. Wrappers passing `--base=$TIP_SHA` to Nx (without `--head`) get effectively two-dot affected-set, which over-includes commits on `origin/main` that aren't in the PR. Behavior is conservative (false-positive on affected, not false-negative) so it's a perf nit, not a correctness bug.

**Resolution (deferred)**: Operations explicitly recommended defer-with-documentation. The fix lives in `_get_base_ref.sh` (emit merge-base-SHA in CI-PR mode), not in my wrappers — out of scope for #36 because (a) it touches cross-language infrastructure, (b) Rust wrappers don't consume the SHA today but might in future, (c) the change requires its own review. **Initial Day-2 plan put a `TODO(post-#36)` comment in all four wrappers; code-reviewer Gate-3 F1 (correctly) flagged that as duplicated + misplaced (the fix happens in `_get_base_ref.sh`, not consumers). Final placement: one comment line in `_get_base_ref.sh` near the ci-pr branch + structured entry in `/work/TODO.md` § "ADR-0033 Pipeline Follow-ups"**. Full context in "Lessons Learned" below.

### 3. `cache: 'pnpm'` on setup-node caches the pnpm store, not `node_modules` itself (operations FYI)

**What happened**: Operations flagged that `actions/setup-node@v4`'s `cache: 'pnpm'` key caches the pnpm global store (`~/.local/share/pnpm/store/v3` or similar), not the workspace's `node_modules/`. So `pnpm install --frozen-lockfile` still has to rebuild `node_modules` from the cached store on each CI run — first-PR-after-merge will pay the cold cost (typically 30-90s for a workspace this size); subsequent PRs hit the store cache and are faster.

**Resolution (informational, no action)**: Not a blocker; just an operational expectation. If cold install >2 min becomes a problem post-merge, the follow-up is `actions/cache@v4` keyed on `pnpm-lock.yaml` hash, scoped to `node_modules/.pnpm/`. Larger surgery than #36 should attempt. No tracker entry; informational only.

---

## Lessons Learned

### Stale Nx cache surfaces as cryptic `ENOENT` from `cache.js`, fix is `nx reset`

Team-lead's Gate 2 `layer-all.sh` run hit a Layer 4 failure (`NX No such file or directory (os error 2)` at `cache.js:119:51`) that was *not* a wrapper bug — it was a stale `.nx/cache` artifact from my earlier Gate 2 testing (I ran with cached results, then nuked `node_modules` and re-installed, leaving the cache pointing at paths that no longer existed). `pnpm exec nx reset` cleared it; re-running passed. **Operator heuristic**: a cryptic ENOENT from `nx`'s `cache.js` is almost never a wrapper issue — try `pnpm exec nx reset` before debugging the wrapper. The wrapper's `STATUS=FAIL REASON=nx-<verb>-failed` is correctly load-bearing-loud; the cause "stale cache" just needs the standard `nx reset` reset.

### `EXIT=$?` after a pipeline captures the rightmost command's exit, not the pipeline's first command

The Day 1 measurement bug (`pnpm exec nx ... | head -30; echo "EXIT=$?"`) is a recurring class of error. Future shell empirical work in this project should either (a) write the test as `cmd >/tmp/out 2>&1; echo "EXIT=$?"; tail /tmp/out` to capture the real exit, or (b) use `set -o pipefail` and chain through `&&` so the pipeline's `$?` reflects any failure. Saved this lesson back during Gate 1; verified at Gate 2 verification step #4 with a load-bearing test.

### Defense-in-depth versus "trust the contract" — when to add guards

Day 1 plan included a `[[ -z "$BASE_SHA" ]]` empty-guard for `_get_base_ref.sh`'s output. Security accepted it as bulletproof; code-reviewer pushed back as dead-code per CLAUDE.md "trust internal code and framework guarantees." Three signals collectively justified dropping: (a) `set -e` empirically propagates through top-level command-substitution assignment, (b) the only case the guard catches is `_get_base_ref.sh` returning rc=0 with empty stdout (a contract bug, not a runtime failure), (c) `_get_base_ref.test.sh` already exists as the right place to catch contract regressions. Principle: **single-source-of-truth guards belong in the source's own test surface, not duplicated across callers as tripwires**.

### `_get_base_ref.sh` PR-mode tip-SHA vs merge-base-SHA divergence (deferred fix)

In CI-PR mode, `_get_base_ref.sh` computes its changed-files cache via three-dot diff (`base...HEAD`, line 121) — correct, captures only the PR's commits. But the SHA it emits on stdout for downstream consumers (line 137) is the **base tip**, not the merge-base. Consumers like `nx affected --base=$SHA` (without `--head`) then do an effectively two-dot diff which over-includes `origin/main` commits that arrived after the PR branched off. Conservative behavior (false-positive on affected, not false-negative), but adds CI runtime cost on PRs against fast-moving branches.

The fix would be in `_get_base_ref.sh`: emit `git merge-base origin/$GITHUB_BASE_REF HEAD` in CI-PR mode instead of the tip. Cross-language change — Rust wrappers don't consume the SHA today but might in future (e.g., `cargo --since`-style flags). Tracked in `/work/TODO.md` § "ADR-0033 Pipeline Follow-ups" › `_get_base_ref.sh` CI-PR tip-SHA vs merge-base-SHA; inline anchor at `scripts/lang/_get_base_ref.sh:~85`.

### "$@" pass-through vs blocking — the security boundary question

Day 1 plan blocked `"$@"` on TS wrappers, mirroring `audit.sh`. Code-reviewer pointed out the audit-policy-bypass threat (`--audit-level=critical`, `--ignore=GHSA-X`) is specific to gates with *runtime-tunable enforcement thresholds* — it doesn't generalize to plain verb wrappers like compile/fmt/lint/test that have no such thresholds. Security agreed on re-review: "the wrapper isn't a security boundary, it's a calling convention." Principle: **block CLI pass-through only where the wrapper exists to enforce a policy knob that pass-through could weaken** — for plain verb wrappers, follow precedent (rust/* all pass `$@`).
