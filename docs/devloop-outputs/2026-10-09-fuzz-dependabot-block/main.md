# Devloop Output: Fix fuzz-only Dependabot coverage (failed cargo fuzz block)

**Date**: 2026-10-09
**Task**: The second cargo Dependabot block (`/crates/ac-service/fuzz` + `/crates/media-protocol/fuzz`, added in `ed9f8d6e`) fails with `dependency_file_not_resolvable` (run 37724549271). Make fuzz-only dependency updates work or replace the block, keep dt-guard workspace-deps R6 consistent, and fix comments that claim the block works.
**Specialist**: infrastructure
**Mode**: Agent Teams (v2) — full, Gate-1 present
**Branch**: `feature/fuzz-dependabot-block`
**Duration**: ~95m

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `5253aae058cd7e5fe81766746f78ed65d691f036` |
| Branch | `feature/fuzz-dependabot-block` |
| Lead Model | `claude-opus-5-5[1m]` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` |
| Implementing Specialist | `infrastructure` |
| Tier | `full` |
| Iteration | `1` |
| Security | `security` |
| Test | `test` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Semantic Guard | `semantic-guard` |

---

## Task Overview

### Objective
Fuzz-only cargo deps (today only `libfuzzer-sys`) must have a working update path, R6 must match it, and every comment describing the fuzz block must be true.

### Scope
- **Service(s)**: none (CI config + dt-guard)
- **Schema**: No
- **Cross-cutting**: No

### Debate Decision
NOT NEEDED - one of three enumerated options, chosen in-loop with justification.

---

## Cross-Boundary Classification

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `.github/dependabot.yml` | Mine (CI config) — remove fuzz block, rewrite root-block comment | — |
| `.github/workflows/fuzz-nightly.yml` | Mine (CI config) — top-level `permissions: contents: read` | — |
| `crates/dt-guard/src/workspace_deps.rs` | Rewrite R6 (forbid a cargo block over root-`exclude` crates): machinery mine; *what the guard requires* changes → **Domain-judgment** (policy content) | security + operations + test (all on panel) |
| `crates/dt-guard/tests/workspace_deps_e2e.rs` | Same as above (R6 controls + fixture `dependabot.yml`) | security + operations + test |
| `scripts/lang/rust/audit.sh` | Audit **scope** widened (machinery mine; scope is security policy, ADR-0033 §11) → **Minor-judgment** (widens coverage, no suppression/threshold change) | security (on panel) |
| `scripts/lang/_audit_gate.test.sh` | Mine — hermetic wrapper fixture + new fuzz-graph assertions | — |
| `Cargo.toml` (root) | Mechanical — comment on the `libfuzzer-sys` entry only (R3 binding unchanged) | — |
| `scripts/lang/_audit_gate.sh` (comment ~94-98) | Mine — prose only | — |
| `scripts/lang/_common.sh` | Mine — optional cleanup hook on `install_wrapper_exit_trap` (review: dry/test/code-reviewer) | — |
| `scripts/lang/_wrapper_trap.test.sh` | Mine — hook unit cases | — |
| `scripts/lang/rust/audit-remediation.md` | Minor-judgment — one line on fuzz-graph remediation | security (on panel) |
| `docs/TODO.md` (~1516) | Minor-judgment — one sentence, entry stays open | test (on panel) |
| `docs/runbooks/devloop-validation.md` (R6 text in the workspace-deps row ~527, fuzz-block sentence ~880, §6.6 `cargo-audit-failed` row) | Minor-judgment | operations (on panel) |
| `docs/contributor/audit-suppressions.md` (§Dependabot) | Minor-judgment — one bullet: fuzz crates have no Dependabot block, why, and what covers them | security (on panel) |
| `docs/specialist-knowledge/infrastructure/INDEX.md` | Mine | — |

No GSA path is touched: `crates/media-protocol/**` is NOT edited (option 2/3 would have required it).

---

## Planning

### Problem in mechanism language
dependabot-core's cargo fetcher cannot reconstruct a Cargo workspace whose path dependencies inherit from a *different* workspace root that is not on the walk it performs. Root cause verified in upstream source (`cargo/lib/dependabot/cargo/file_fetcher.rb`, `find_workspace_root`): the parent walk's depth is `current_dir.scan("/").length` over the path dep's **job-relative** name. For `media-protocol` fetched from `/crates/media-protocol/fuzz` the name is `../Cargo.toml` → `current_dir = ".."`, depth 0 → one step to `../..` = `crates/`, which has no `Cargo.toml`; the repo root is never fetched, so cargo fails `failed to find a workspace root` (exactly run 37724549271). From `/crates/ac-service/fuzz`, `proto-gen` is `../../proto-gen/Cargo.toml` (depth 2) and *does* reach the root — so the failure is layout-dependent, not a config typo. Upstream: open issue dependabot/dependabot-core#11861 (multi-workspace `failed to find a workspace root`, open since 2025-03), open PR dependabot/dependabot-core#14985 (fixes adjacent root/sibling fetch bugs; does not touch the depth computation). Wider class: **every Cargo dependency graph this repo builds should be reachable by some update/advisory channel** — today the two fuzz graphs reach none (lockfiles untracked, `scripts/lang/rust/audit.sh` scans only the root `Cargo.lock`; Lead's observation verified).

### Key fact that decides it
Even a *working* fuzz block yields almost nothing. The fuzz lockfiles are untracked (`.gitignore` `**/fuzz/Cargo.lock`), and dependabot-core cargo uses `BumpVersionsIfNecessary` with no lockfile (`update_checker.rb#requirements_update_strategy`, `requirements_updater.rb#update_version_requirement_if_needed`: returns the requirement unchanged when it already admits the target). `libfuzzer-sys = "0.4"` admits every 0.4.x, so the block could only ever open a PR on a **0.5** release. Meanwhile every 0.4.x patch/minor already reaches us with no PR: each fuzz build (Nightly Fuzz Testing, fresh checkout, no lock) resolves the newest semver-compatible version (verified locally: resolves 0.4.13 = latest).

### Choice: Option 1 — drop the fuzz block; close the audit gap in the existing wrapper
- **Option 3 rejected**: the only workaround the fetcher honours is an explicit `package.workspace = "../.."` in `crates/media-protocol/Cargo.toml` (GSA edit) to steer `find_workspace_root` past its bug — a manifest change made for a third-party bug, unverifiable before merge (Dependabot runs only on GitHub), for a block whose maximum yield is one PR per libfuzzer-sys major.
- **Option 2 rejected**: removing the fuzz crates' `[workspace]` (GSA edit to `crates/media-protocol/fuzz/Cargo.toml`), pulling libFuzzer's C++ build into every `cargo build/clippy/test --workspace` and coverage run (Layer 1/4 time), workspace clippy lints (`unwrap_used` etc.) applied to fuzz targets, and it is the scope of the open `docs/TODO.md` "Neither fuzz workspace is compiled by the pipeline" item (test + infra, story task 23). Not silently expanded into.

### Changes
1. `.github/dependabot.yml`: delete the second cargo block. Rewrite the root block's comment: fuzz crates have no Dependabot block (upstream bug + links; and even working it would PR only on a major), shared crates still move with the root PR (R3 unchanged), fuzz-only crates (`libfuzzer-sys`) float within their requirement at every fuzz build, advisories are caught by the audit (below), majors are bumped by hand (root entry + both fuzz manifests in one PR, R3 enforces).
2. `scripts/lang/rust/audit.sh` (extend existing tooling, no new mechanism): audit **every Cargo workspace root tracked in git** (discovered from the tree via `cargo locate-project` — see round 2; today: root + the two fuzz crates). A workspace whose `Cargo.lock` is tracked is audited as-is (never regenerated). One whose lock is untracked is resolved fresh with `cargo generate-lockfile --manifest-path` (exactly what every build of it does) and audited with `cargo audit -f <lock>`. All graphs are scanned (no short-circuit), each named in output. Failure tokens, discovery predicate and controls: **superseded by round 2** (`cargo-audit-discovery-failed` / `cargo-lockfile-resolve-failed` / `cargo-audit-failed`). Suppressions unchanged (`.cargo/audit.toml`, read natively). Gate unchanged: `crates/*/Cargo.toml` already matches `crates/*/fuzz/Cargo.toml` (`*` crosses `/`, pinned by `_audit_gate.test.sh:44`). Weekly Scheduled Audit (force-run) therefore now covers the fuzz graphs with no workflow edit. Verified locally: both fuzz graphs resolve and audit clean today (129 + 19 crates). A developer's local fuzz `Cargo.lock` is preserved (round 2).
3. `crates/dt-guard/src/workspace_deps.rs` + e2e: **rewrite R6** to forbid any cargo block over a root-`exclude` crate (see Gate-1 amendments); delete the `allow:`/fuzz-only machinery. R1–R5 and `EXCLUDED_MANIFEST_MISSING` (still serves R3 and R6) unchanged.
4. Root `Cargo.toml` `libfuzzer-sys` comment: no Dependabot PR reaches it; hand-bump on a major; advisories via the fuzz-graph audit.
5. Docs: runbook workspace-deps row (R6 text replaced by the rewritten rule's token `dependabot-cargo-block-names-excluded-crate`; `workspace-deps-dependabot-config-unreadable` kept), line ~880 (drop the fuzz-block PR sentence), §6.6 rows (see round 2); contributor doc §Dependabot bullet; INDEX.md.
6. Tests: see round 2 (audit wrapper) and Gate-1 amendments + round 2 (R6).

### Gate-1 amendments (reviewer pre-plan input folded in)
- **R6 is rewritten, not just deleted** (@test: a test must fail on the exact run-37724549271 misconfiguration). New R6 = `dependabot-cargo-block-names-excluded-crate`: any cargo block whose `directory`/`directories` names a root-`exclude` crate fails, with a message naming the file, the block's dirs, the upstream bug (dependabot-core#11861) and the fix (remove it; fuzz-only crates float + are audited). Positive control (the shipped block MUST fire), negative (root-only passes), the unreadable-config precondition kept with its test. The `allow:`/fuzz-only-set machinery (`DEPENDABOT_FUZZ_*` ×3, `fuzz_only`, `member_packages`, `Excluded.packages`) is deleted; the R3 tests' `with_fuzz_block(...)` wrapper is replaced by the root-only config. This is not a vacuous rule (@security's concern): it forbids a construct that is known not to work, and it has a MUST-fire case. If upstream fixes the fetcher, the rule is the one place to relax.
- **Fuzz lockfiles stay untracked; generated fail-loud at audit time** (@security preferred committing). Committing would freeze the nightly graph with nothing able to bump it: Dependabot cannot process these directories (the bug above), so every 0.4.x patch would become a hand edit — strictly less coverage than today's float. Generation failure fails loudly, never a skip (token: `cargo-lockfile-resolve-failed`, round 2). Consequently no lockfile glob is added to the gate (manifests already match, `_audit_gate.test.sh:44`).
- ~~**Graph set derivation**: tracked manifests with a `[workspace]` table~~ — superseded by round 2 (`cargo locate-project`).
- **No new staleness checker** (@observability asked for a scheduled crates.io check; @operations and the user's standing "prefer existing tooling" guidance argue against). Residual written where people look: `dependabot.yml` root-block comment, root `Cargo.toml` `libfuzzer-sys` comment, contributor doc §Dependabot, runbook line ~880. Lead to rule if observability holds the point.
- **`fuzz-nightly.yml`: add top-level `permissions: contents: read`** (@security, one-line, same class — it builds an unlocked third-party graph with the default token). Cross-boundary row: mine (CI config).
- **Post-merge remaining action** (@operations): Dependabot is only validated on GitHub. After merge, run "Check for updates" on the cargo job and record the run ID showing no `dependency_file_not_resolvable` (the root block alone must succeed).

### Gate-1 amendments, round 2 (reviewers' Gate-1 conditions; Lead ruling: no staleness checker)
`scripts/lang/rust/audit.sh` final shape:
- **cwd = repo root**, set explicitly at the top (`cd` to the root the wrapper already computes). cargo-audit reads `.cargo/audit.toml` from the cwd, not from `-f`, so this is what makes the one suppression SSoT apply to every graph; stated in a comment. The only new argument is `-f <discovered lock>`; still no `$@` pass-through, no `--ignore`.
- **Discovery** (Lead ruling, supersedes the header regex): for every `git ls-files` `Cargo.toml`, `cargo locate-project --workspace --manifest-path <m> --message-format plain --offline`; the distinct results (made repo-relative) are the workspace roots. Cargo's own parser decides membership, `exclude` and what a workspace table is, so no second encoding of either exists (answers @dry-reviewer's exclude-parsing question). Members map to the root; the fuzz crates map to themselves. Verified on this tree: 19 tracked manifests → 17 × `Cargo.toml`, 1 × each fuzz crate, 0.5 s, no network. Every tracked manifest passes through cargo, so a crate that is neither a member nor its own root is a cargo error, not a silent drop.
- **Positive controls → `STATUS=FAIL REASON=cargo-audit-discovery-failed`**, before any scan: any `locate-project` call fails (the failing manifest is named); or the repo-root `Cargo.toml` is not among the discovered roots, or its `Cargo.lock` is not tracked (zero-found is a special case). The separate "every exclude entry discovered" check is dropped: it existed only to back-stop a hand-written predicate, and with cargo deciding it would need a second `exclude` parser. The discovered count and list are printed (`AUDIT_GRAPHS=<n> <paths…>`).
- **Per graph**, all graphs run (no short-circuit): tracked lock → `cargo audit` on it as-is, never regenerated; untracked → an existing local lock is copied to a `mktemp` dir (path printed), `cargo generate-lockfile --manifest-path` resolves fresh (what every unlocked build of it compiles; runs no build scripts), `cargo audit -f`, then the original lock is restored byte-for-byte (or the fresh one removed if there was none) right after that graph's audit, AND by an EXIT trap chained in front of `install_wrapper_exit_trap`'s handler (`rc=$?` captured first, restore pending backups, re-raise rc, delegate) so a `set -e` abort still restores (@code-reviewer); only SIGKILL can leave it, and the printed backup path recovers it (@operations (a); the "side effect" line is dropped). Each graph prints one stderr line `AUDIT_GRAPH=<lock> LOCK=tracked|resolved RESULT=ok|audit-failed|resolve-failed`.
- **One STATUS line**, aggregated (no per-graph `run_and_emit`): any `resolve-failed` → `FAIL cargo-lockfile-resolve-failed` (a graph went unevaluated: repair, not remediation — outranks); else any advisory → `FAIL cargo-audit-failed`; else `OK cargo-audit-passed`.

Tests (`_audit_gate.test.sh`, real `git init` repo, root lock `git add`ed, fuzz lock not; PATH-stubbed cargo logs `$PWD` + argv, delegates ONLY `locate-project` to the real cargo (offline, no network — so discovery semantics are cargo's, not the stub's) and exits non-zero on any other unexpected subcommand): a member (`edition.workspace = true`, `{ workspace = true }`) maps to the root and is not audited separately; a `# [workspace]` comment-only member likewise; a `[workspace.dependencies]`-only independent root and a bare `[workspace]` root are both discovered; root-not-discovered, root-lock-untracked and a manifest cargo rejects → `cargo-audit-discovery-failed`; root gets `audit` only (no `generate-lockfile`), fuzz gets `generate-lockfile --manifest-path fuzz/Cargo.toml` + `audit -f fuzz/Cargo.lock`, all from the repo root with no other args (suppression parity, stub-level); first graph failing still audits the later ones and fails; resolve failure → `cargo-lockfile-resolve-failed`; advisory → `cargo-audit-failed` with the failing graph named in its `AUDIT_GRAPH` line; a pre-existing untracked fuzz lock is byte-identical afterwards — on the clean path, on a failing fuzz audit, and on a resolve failure. Existing Parts 3/4 fixtures gain the root `[workspace]` manifest.

R6 unit cases (@test (g)): fires on singular `directory: "/crates/x/fuzz"` and on a `directories:` list, each with leading/trailing-slash variants (`crates/x/fuzz`, `/crates/x/fuzz/`; `norm_dir` kept); fires on the exact shipped two-dir block; does NOT fire on an npm block naming the same dir, nor on a cargo block whose dir is only a prefix (`/crates/x`); root-only config passes; unreadable config → precondition.

Docs (in addition to the earlier list):
- Runbook workspace-deps row: R6 text replaced by the new token `dependabot-cargo-block-names-excluded-crate` (meaning + fix); `workspace-deps-dependabot-config-unreadable` kept. (Corrects Changes item 5.)
- Runbook §6.6: rows for `cargo-lockfile-resolve-failed` and `cargo-audit-discovery-failed`; `cargo-audit-failed` row names the graph via `AUDIT_GRAPH`, gives the local repro (`cargo generate-lockfile --manifest-path crates/<x>/fuzz/Cargo.toml && cargo audit -f crates/<x>/fuzz/Cargo.lock`, from the root), remediation (a fix inside the requirement is picked up on the next run; otherwise root `[workspace.dependencies]` + both fuzz manifests in one PR, R3 enforces; suppression only in `audit-suppressions.toml`), and extends the ambient-advisory note to the fuzz graphs.
- `scripts/lang/rust/audit-remediation.md`: one line — a fuzz-graph advisory is not fixed with `cargo update --precise` (untracked lock); raise the requirement as above.
- `dependabot.yml` header NOTE: Dependabot security updates/alerts cannot see the fuzz graphs either; the audit is their only advisory channel. Residual stated at the site (dependabot.yml + root `Cargo.toml`): no automated signal for a new `libfuzzer-sys` major absent an advisory; the first advisory against 0.4 fails the audit (audit-drift issue).
- `scripts/lang/_audit_gate.sh:94-98`, `_audit_gate.test.sh:43`, and the `audit.sh` header: a fuzz-manifest change now audits that fuzz graph, not "the unchanged root lock".
- `docs/TODO.md:1516` (stays open): one sentence — Layer 6 now resolves and audits both fuzz graphs; nothing compiles them yet.

### For the PR: how `libfuzzer-sys` updates reach us, and how we'd know they stopped
- **0.4.x patch/minor**: automatically, every fuzz build — no committed fuzz lockfile, so cargo resolves the newest compatible release each run.
- **Security fixes**: `scripts/lang/rust/audit.sh` now resolves and `cargo audit`s both fuzz graphs — per PR on any dep-manifest change (Layer 6) and weekly in Scheduled Audit, whose failure opens/updates the `audit-drift` issue. An advisory fixed only in 0.5 surfaces there.
- **A new major with no advisory**: nothing automatic tells us — stated honestly as the residual. It costs nothing while 0.4 receives fixes; once 0.4 stops being fixed, the first advisory against it fails the audit. This is the same residual the dropped block would have closed, at the cost of a GSA edit for a third-party bug.
- **If the updates stop** (e.g. the fuzz workspaces stop resolving): the audit's fresh resolution fails → `cargo-lockfile-resolve-failed` in Layer 6 and the weekly scan (audit-drift issue), loudly; a discovery that loses a fuzz workspace → `cargo-audit-discovery-failed`.

---

### Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed (re-confirmed after locate-project discovery) |
| Test | confirmed (re-confirmed) |
| Observability | confirmed |
| Code Quality | confirmed (re-confirmed) |
| DRY | confirmed (re-confirmed) |
| Operations | confirmed |
| Semantic Guard | confirmed |

Lead rulings: no crates.io staleness checker (residual documented at site); workspace discovery via `cargo locate-project --workspace --offline` (no second `exclude` parser). Classification guard: `STATUS=OK`.

---

## Pre-Work

None

---

## Implementation Summary

Option 1, as amended through Gate-1 round 2.

- **`.github/dependabot.yml`**: the second cargo block is deleted. The root block's comment records why the fuzz crates get no block: the `find_workspace_root` depth bug (run 37724549271; dependabot/dependabot-core#11861), and the fact that a working block would PR only on a major. It also says shared crates still move with the root PR (R3), and states the residual. The header NOTE says security updates/alerts cannot see the fuzz graphs and that the audit is their only advisory channel.
- **`crates/dt-guard/src/workspace_deps.rs` R6 rewritten** as `dependabot-cargo-block-names-excluded-crate`: any cargo block whose `directory`/`directories` (slash-normalised) names a root-`exclude` crate fires, and the message gives the file, the dirs, the upstream bug and the fix. Deleted: `DEPENDABOT_FUZZ_*` ×3, the fuzz-only set, `Excluded`, `member_packages`. Kept: `DEPENDABOT_CONFIG_UNREADABLE`, `EXCLUDED_MANIFEST_MISSING`, `norm_dir`. R2 still collects excluded crates' literal pins (`record_literal_pin` in `check_excluded`, unchanged). The directory read unions `directory` and `directories`; a glob directory is a hit (cannot be compared with `exclude`); a non-string entry is a `DEPENDABOT_CONFIG_UNREADABLE` precondition, never dropped. Unit tests cover the shipped two-dir block (MUST fire), globs, both keys present, non-string entries, both key shapes × three slash variants, the root block widened to a fuzz dir, no fire on npm or a prefix dir, root-only passes, and the preconditions. The R3/R2 tests use a root-only config. The e2e gets an R6 positive control and an unreadable-config precondition.
- **`scripts/lang/rust/audit.sh`**: cd to the repo root (the suppressions SSoT is read from the cwd). Discovery runs `cargo locate-project --workspace --offline` per git-tracked `Cargo.toml` and dedups canonical repo-relative roots. Discovery checks, each with its own `AUDIT_DISCOVERY:` line and all reported as `cargo-audit-discovery-failed`: empty ls-files, a locate failure (manifest named), a root outside the repo, the root not discovered, or the root lock untracked. A tracked lock gets `cargo audit -f` as-is. An untracked lock is saved (if present), resolved with `generate-lockfile`, audited, then restored inline; on any abort the restore runs from the new `install_wrapper_exit_trap restore_pending_lock` cleanup hook in `_common.sh` (handler: `rc=$?`, `set +e`, cleanup, then the task-#50 STATUS fallback). Every in-loop cargo call reads `</dev/null`, and a scanned-vs-discovered count check fails `cargo-audit-discovery-failed` if a graph went missing. There is one `AUDIT_GRAPH=` line per graph, every graph is scanned, and there is one aggregated STATUS (`cargo-lockfile-resolve-failed` > `cargo-audit-failed` > OK). Real tree: `AUDIT_GRAPHS=3 . crates/ac-service/fuzz crates/media-protocol/fuzz`, all `RESULT=ok`, no lock left behind.
- **`scripts/lang/_audit_gate.test.sh`**: the shared fixture is now a real Cargo tree (root with an inheriting member and a comment-only member; a `[workspace]` root and a `[workspace.dependencies]`-only root). The stub cargo fakes every subcommand, `locate-project` included, from a hand-written table; an unmapped manifest or any other argument shape exits 97 (Lead ruling: Layer 3 invokes cargo zero times — real `locate-project` semantics run on the real tree in Layer 6 every time, behind the root positive control; security verified by hand that it returns the root for a member and exits 101 for an orphan crate). Part 5 covers: the discovered set; root audited as-is and never regenerated; both independent roots resolved and audited; members not graphs; every audit run from the root with `-f` only; no short-circuit with the failing graph named; resolve failure gets its own token and outranks an advisory; a dev lock comes back byte-identical (clean, advisory, unresolvable); canonical-path dedup (`m/../Cargo.toml`); the five discovery failures (orphan manifest, root outside the repo, root not discovered, root lock untracked, no manifests), each scanning nothing; a set -e abort mid-scan keeps `STATUS=FAIL REASON=wrapper-aborted-early-exit-1`; SIGTERM mid-scan leaves the dev lock byte-identical. Mutation-checked: adding `--ignore` breaks the parity/OK assertions, and removing the restores breaks the byte-identical assertions.
- **`fuzz-nightly.yml`**: top-level `permissions: contents: read`.
- **Docs**: root `Cargo.toml` `libfuzzer-sys` comment (residual); runbook workspace-deps row (new R6, precondition kept) + line on Dependabot PRs + §6.6 (`cargo-audit-failed` row with repro/remediation, two new token rows, ambient residual and scheduled scan extended to fuzz graphs); contributor doc §Dependabot bullet; `audit-remediation.md` fuzz-graph line; `_audit_gate.sh` gate comment; `INDEX.md`; `docs/TODO.md` fuzz entry (one sentence, still open).

**Dropped from round 2:** the "every root `exclude` entry is discovered" control. It back-stopped a hand-written workspace predicate; with `cargo locate-project` cargo decides membership and `exclude` itself (an excluded crate without its own workspace is a cargo error → `cargo-audit-discovery-failed`), and keeping it would need a second `exclude` parser in bash.

**For the PR — how `libfuzzer-sys` updates reach us and how we'd know they stopped:** see §Planning "For the PR". In short: 0.4.x floats in at every fuzz build. Advisories fail `scripts/lang/rust/audit.sh` (Layer 6 + weekly Scheduled Audit, which opens the `audit-drift` issue). A fuzz graph that stops resolving reports `cargo-lockfile-resolve-failed`. There is no automated signal for a non-advisory major (stated residual).

---

## Files Modified

`.github/dependabot.yml`, `.github/workflows/fuzz-nightly.yml`, `Cargo.toml`, `crates/dt-guard/src/workspace_deps.rs`, `crates/dt-guard/tests/workspace_deps_e2e.rs`, `scripts/lang/rust/audit.sh`, `scripts/lang/rust/audit-remediation.md`, `scripts/lang/_audit_gate.sh`, `scripts/lang/_audit_gate.test.sh`, `scripts/lang/_common.sh`, `scripts/lang/_wrapper_trap.test.sh`, `docs/runbooks/devloop-validation.md`, `docs/contributor/audit-suppressions.md`, `docs/specialist-knowledge/infrastructure/INDEX.md`, `docs/TODO.md`.

---

## Devloop Verification Steps

- `./scripts/layer-fast.sh`: green (L1–L6 OK). Layer 6 ran the new wrapper on the real tree: `AUDIT_GRAPHS=3 . crates/ac-service/fuzz crates/media-protocol/fuzz`, all three `RESULT=ok`, `STATUS=OK REASON=cargo-audit-passed`, no fuzz lock left behind.
- `cargo test -p dt-guard` workspace-deps unit + e2e: green, including the real-repo e2e (R6 clean over the new `dependabot.yml`).
- `scripts/lang/_audit_gate.test.sh`: 112/112 (50 consecutive green runs); `_wrapper_trap.test.sh` 35/35. Mutation checks (reverted): `--ignore` injected → parity/OK assertions red; both lock restores removed → byte-identical assertions red.
- **Gate 2** (`DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh`, Lead, final tree): `TOTAL_RESULT=OK` — L1 6s, L2 3s, L3 140s, L4 150s, L5 1s, L6 5s (`AUDIT_GRAPHS=3`, all `RESULT=ok`, `cargo-audit-passed`), L7 871s (env-tests + browser E2E OK). First Gate-2 attempt aborted by the Lead (a test-file edit landed mid-run), not a failure.
- Not verifiable here: Dependabot itself (see Rollback §3, post-merge action).

---

## Code Review Results

### Gate 3 verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 3 | 3 | 0 | F1 stdin swallow, F2 RESULT label, F3 discovery wording |
| Test | RESOLVED-FIXED | 2 | 2 | 0 | abort-path STATUS loss; stub-vacuity; re-confirmed after flake fix |
| Observability | RESOLVED-FIXED | 2 | 2 | 0 | RESULT label; discovery wording; flagged the flake |
| Code Quality | RESOLVED-FIXED | 7 | 7 | 0 | F1–F7 |
| DRY | RESOLVED-FIXED | 1 | 1 | 0 | hand-rolled EXIT trap → `_common.sh` hook |
| Operations | CLEAR | 0 | 0 | 0 | re-confirmed after fix round |
| Semantic Guard | RESOLVED-FIXED | 1 | 1 | 0 | R6 overclaim (native: SAFE after fix) |

### Findings log

| Reviewer | Finding | Resolution |
|----------|---------|------------|
| security F1 | in-loop cargo calls inherit the here-string stdin; a stdin reader would drop graphs and still read OK | Fixed: `</dev/null` on all four in-loop cargo calls; plus a scanned-vs-discovered count check (`cargo-audit-discovery-failed`). |
| security F2 | same as observability's | Fixed (same change). |
| test (re-review) | the shared handler's `set +e` was not shown load-bearing by `_audit_gate` (its abort's cleanup is a no-op) | Added `_wrapper_trap.test.sh` cases: a cleanup returning 7 during an rc-3 abort keeps `wrapper-aborted-early-exit-3` + exit 3; the cleanup touches a marker on abort AND on a normal exit; an undefined cleanup returns 1 with its message and installs no trap. Mutation: deleting `set +e` turns `trap-cleanup-fails:{rc,status}` and `trap-cleanup-abort:status` red. |
| observability (re-review) | same as security F3, plus: the printed `AUDIT_GRAPH=` lines are real results | Fixed with F3; header + runbook say to read them; source column no longer says "before any scan". |
| observability (FYI, flake) | one `_audit_gate.test.sh` run missed `AUDIT_LOCK_BACKUP=`; reproduced separately as `[wrapper no-dep -> scanner NOT invoked]` | Root cause: `run_wrapper`'s `env -i` dropped `DEVLOOP_TMP`, so every wrapper run shared the default `/tmp/devloop/changed-files.layer-shared` cache with concurrent layer runs/tests, and a neighbour's cache could flip the gate (pre-existing in Part 3; Part 5's extra runs widened the window). Fixed: per-repo `DEVLOOP_TMP=<dir>/tmp/devloop`. Proof (Lead Gate-2 blocker): with the old `run_wrapper` and a background process writing a docs-only list into `/tmp/devloop/changed-files.layer-shared` (what any concurrent pre-fix run of this file does), the unresolvable dev-lock case failed 30/30 as `SKIPPED-NO-DIFF`; with the fix, 0/60. Anti-vacuity added: `scan_case` fails any Part-5 case whose gate SKIPPED (mutation: drop the fix + interference → 64 red, all "gate must RUN"); each dev-lock case asserts `AUDIT_GRAPH=fz/Cargo.lock LOCK=resolved RESULT=<expected>`; the SIGTERM case asserts fd's resolve was reached. 50 consecutive runs of the file: 50/50 green (112/112). |
| security F3 | header + runbook said discovery-failed means "nothing scanned", but `scanned N of M` fires after some scans | Fixed: both now say "could not be established (nothing scanned), or not every discovered graph was scanned"; the runbook cause list includes `scanned N of M`. |
| dry-reviewer / test / code-reviewer F1 | hand-rolled EXIT-trap chaining; under `set -e` the `(exit "$rc")` re-raise killed the handler, so an abort lost the task-#50 STATUS line | Fixed in `_common.sh`: `install_wrapper_exit_trap [cleanup_fn]` (defined-function check, nothing eval'd); the handler runs `set +e` after `rc=$?`, then the cleanup, then the fallback. audit.sh's own trap deleted. Tests: `_wrapper_trap.test.sh` (cleanup on abort with a failing cleanup keeps the STATUS + rc; cleanup on OK; undefined cleanup refused), `_audit_gate.test.sh` set -e abort (uid-independent: backup deleted) and SIGTERM. Mutation-checked: no `set +e` → hook test red; no hook → SIGTERM test red. |
| Lead / operations / code-reviewer F6 | Layer 3 must invoke cargo zero times; the test ran real `locate-project`/`rustup which` | Fixed by stubbing `locate-project` (Lead overruled amending the invariant); REAL_CARGO plumbing removed; stub fails closed on unmapped manifests; labels claim only wrapper behaviour; root-outside-repo case added. |
| semantic-guard / code-reviewer F2 | R6 justification overclaimed ("cannot resolve them / fails every run") — ac-service/fuzz does reach the root | Fixed at every site (module doc, rustdoc, hit detail, dependabot.yml, runbook, contributor doc, INDEX): resolution is layout-dependent (media-protocol/fuzz fails) + majors-only yield. |
| semantic-guard note | round 2's exclude-entry control was dropped silently | Recorded in Implementation Summary. |
| code-reviewer F3 | fuzz remediation text assumed a root-pinned crate | Fixed: raise the requirement where it is written (+ root entry when R3 binds it); transitive → bump its parent. Runbook + audit-remediation.md. |
| code-reviewer F4 | `printf '%b'` over accumulated string | Fixed: bash array + `printf '%s\n'`. |
| code-reviewer F5 | "Cleared once restored" comment inaccurate | Fixed: cleared before the restore is attempted; not retried; the backup path is the recovery. |
| code-reviewer F7 | R6 fails open on globs, on both keys present, on non-string entries | Fixed (union of keys; glob = hit; non-string = precondition) with unit cases for each. |
| observability | `RESULT=advisory` mislabels cargo-audit's own failures (exit 1 for both a vuln and e.g. a DB fetch failure) | Fixed: per-graph value renamed `RESULT=audit-failed`; header enum and §6.6 `cargo-audit-failed` row say "an advisory, or cargo-audit itself failed — its output above the AUDIT_GRAPH line says which"; test expectation updated. STATUS token unchanged. |

---

## Accepted Deferrals

- (none surfaced in this devloop)

---

## Rollback Procedure

1. Start commit: `5253aae058cd7e5fe81766746f78ed65d691f036`
2. **Safe-revert unit**: the single commit (dependabot.yml + fuzz-nightly.yml + dt-guard R6 + audit.sh + tests + docs) reverts together.
3. **Post-merge remaining action**: Dependabot is not observable by Layers 1-7. Run "Check for updates" on the cargo job; confirm only the root cargo job runs, green, with no `dependency_file_not_resolvable`; record the run ID here.

---

## Issues Encountered & Resolutions

### Issue 1: Flaky `_audit_gate.test.sh`
**Problem**: intermittent missing `AUDIT_LOCK_BACKUP=` — wrapper runs under `env -i` shared `/tmp/devloop`'s changed-files cache with concurrent runs (reviewers running the suite), flipping the gate to SKIPPED-NO-DIFF.
**Resolution**: per-repo `DEVLOOP_TMP`; anti-vacuity assertions that the scan ran. 50/50 + Lead 40/40 green.

### Issue 2: Gate-2 run invalidated by a mid-run edit
**Problem**: a test-file edit crossed the Lead's freeze message 90s into Gate 2.
**Resolution**: run killed, implementer confirmed DONE, all reviewers told to run nothing, Gate 2 re-run green.

### Issue 3 (observation, pre-existing): a Layer-3 selftest rewrites a tracked file
**Problem**: `scripts/audit-suppressions-check.test.sh` swaps the real `.github/dependabot.yml` and restores it via its EXIT trap, so the file's mtime changes during every Layer 3 (bytes unchanged). It explained an unexplained mtime during Gate 2.
**Resolution**: none in this devloop (pre-existing, out of scope); surfaced to the operator.
