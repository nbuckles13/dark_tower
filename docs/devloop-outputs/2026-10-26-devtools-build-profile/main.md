# Devloop Output: dt-guard/dt-story devtools build profile + single home for tool binary paths

**Date**: 2026-10-26
**Task**: Build `dt-guard` and `dt-story` with a dedicated non-LTO `[profile.devtools]`, give their binary paths one home (`scripts/lib/tool-bins.sh`), migrate every hard-coded `target/release/dt-{guard,story}` site atomically, and guard against drift.
**Specialist**: infrastructure
**Mode**: Agent Teams (v2) — full, Gate-1 present <!-- panel mode + Gate-1 tier (ADR-0037 §D2); see the Tier row in Loop State -->
**Branch**: `feature/user-see-each-other`
**Duration**: ~3h (2026-10-26 22:55 → 2026-10-27 ~01:55 UTC)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `b38838d461709892787c467a3579f76f3a624431` |
| Branch | `feature/user-see-each-other` |
| Lead Model | `claude-opus-5-5[1m]` |

---

## Loop State (Internal)

<!-- This section is maintained by the Lead for state recovery after interruption. -->
<!-- Do not edit manually - the Lead updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` (infrastructure) |
| Implementing Specialist | `infrastructure` |
| Tier | `full` |
| Iteration | `1` |
| Security | `CLEAR` |
| Test | `RESOLVED-FIXED` |
| Observability | `CLEAR` |
| Code Quality | `RESOLVED-FIXED` |
| DRY | `RESOLVED-FIXED` |
| Operations | `RESOLVED-FIXED` |
| Semantic Guard | not spawned: no checks.md surface (build profile, script paths, CI; no credential/key material) |

---

## Task Overview

### Objective
Cut the dev-tool build cost behind the dt-guard subcommand-sprawl trigger: build `dt-guard` and `dt-story` with a dedicated non-LTO `[profile.devtools]` (the measured cause was the services' fat-LTO `[profile.release]`, not crate size). Because changing the profile moves the binary path, every producer and consumer must resolve that path from one home (`scripts/lib/tool-bins.sh`). All sites migrate in one commit, and a new guard (`dt-guard tool-bin-paths`) keeps the home single and the profile name in sync with `Cargo.toml`.

### Scope
- **Service(s)**: None. Validation tooling only: dt-guard, dt-story, pipeline scripts, pre-commit hook, story runner, CI workflows, docs. Service builds and `[profile.release]` are unchanged.
- **Schema**: No
- **Cross-cutting**: No services are affected. Cross-cutting across the pipeline, though: Layer 1 producer, Layer 3 consumers, story runner, CI, SKILL docs.

### Debate Decision
NOT NEEDED. The TODO sprawl entry asked for exactly this decision to be taken *before* any `/debate`. The profile name was the user's choice, and the subcommand-vs-extension question was settled at Gate 1 under ADR-0034 subcommand-per-policy, with all six reviewers confirming. The remaining split question (count/discoverability) stays open in `docs/TODO.md` with its own re-debate trigger.

---

## Cross-Boundary Classification

<!-- List EVERY planned file change. For each, classify per ADR-0024 §6.2:
     - Mine — in the implementing specialist's domain (trivial, the common case)
     - Not mine, Mechanical — cross-boundary, sed-test clean, guard-pipeline covered
     - Not mine, Minor-judgment — cross-boundary, bounded impact; owner must review & confirm at Gate 1 + Gate 3
     - Not mine, Domain-judgment — needs owner-implements or --paired-with=<owner>

     For Guarded Shared Area paths (ADR-0024 §6.4), Mechanical is disallowed; Owner must be filled.
     Fill Owner (if not mine) for cross-boundary rows.

     Path column convention: backtick-quoted paths. Globs (`*`, `?`, `[]`,
     trailing `/`, `/**`) and parenthetical annotations like `foo.rs` (regen)
     are tolerated by the `validate-cross-boundary-scope` parser at
     scripts/guards/common.sh, and are recommended where they clarify intent
     — use `dir/**` (or `dir/`, which the parser canonicalizes to
     `dir/**`) to scope a whole tree, `*.svelte` for a filename glob,
     and `(regen)` / `(cleanup)` /
     `(skeleton-only)` suffixes for per-row context. Prefer the simplest
     form that is accurate: if a literal path conveys the same information,
     use that; reach for a glob when enumerating every file would be noise,
     and reach for a parenthetical when the row's nature (regen, cleanup,
     new-vs-modify) materially changes how a reviewer reads it. Longer-form
     file-shape context (rationale, scope qualifiers, "why this shape") still
     belongs in § Implementation Summary or § Files Modified — the table
     answers one question per row: whose domain is this, and how stringent
     is the involvement. -->

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `Cargo.toml` (`[profile.devtools]` only) | Mine | — |
| `scripts/lib/tool-bins.sh` (new) | Mine | — |
| `scripts/lang/rust/compile.sh` | Mine | — |
| `scripts/guards/simple/_dt_guard_wrapper.sh` | Mine | — |
| `scripts/guards/simple/validate-story-manifest.sh` | Mine | — |
| `scripts/guards/simple/validate-tool-bin-paths.sh` (new) | Mine | — |
| `crates/dt-guard/src/tool_bin_paths.rs` (new) | Mine | — |
| `crates/dt-guard/src/main.rs` | Mine | — |
| `crates/dt-guard/src/lib.rs` | Mine | — |
| `crates/dt-story/src/manifest.rs` (doc comment only) | Mine | — |
| `.githooks/pre-commit` | Mine | — |
| `scripts/workflow/run-story.sh` | Mine | — |
| `scripts/workflow/preflight-story.sh` | Mine | — |
| `scripts/workflow/run-story.test.sh` | Mine | — |
| `.github/workflows/ci.yml` | Mine | — |
| `.github/workflows/ci-client.yml` | Mine | — |
| `scripts/guards/counter-zero-init.test.sh` (default-path line) | Not mine, Mechanical | observability |
| `scripts/guards/media-telemetry-deny.test.sh` (default-path line) | Not mine, Mechanical | security |
| `docs/runbooks/devloop-validation.md` | Not mine, Minor-judgment | operations |
| `.claude/skills/devloop/SKILL.md` | Not mine, Mechanical | operations |
| `.claude/skills/user-story/SKILL.md` | Not mine, Mechanical | operations |
| `.claude/skills/close-story/SKILL.md` | Not mine, Mechanical | operations |
| `.claude/skills/debate/SKILL.md` | Not mine, Mechanical | operations |
| `docs/decisions/adr-0034-guard-pipeline-as-rust-binary.md` | Not mine, Minor-judgment | code-reviewer |
| `docs/TODO.md` (sprawl entry; path text in two open entries) | Mine | — |
| `docs/specialist-knowledge/infrastructure/INDEX.md` | Mine | — |
| `scripts/lib/tool-bins.test.sh` (new) | Mine | — |
| `scripts/layer3.sh` (wire tool-bins.test.sh) | Mine | — |
| `crates/dt-guard/tests/tool_bin_paths_e2e.rs` (new) | Mine | — |
| `crates/dt-guard/tests/credential_leak_key_custody_fixtures.rs` (drop duplicate `dt_guard_bin()`, use `tests/common`) | Mine | — |
| `crates/dt-guard/tests/common/mod.rs` (the one `dt_guard_bin()` + hoisted hermetic `git()`, DRY F4) | Mine | — |
| `crates/dt-guard/tests/cross_boundary_scope_e2e.rs` (uses hoisted `git()`, DRY F4) | Mine | — |
| `crates/dt-guard/src/common/cargo_cli.rs` (new: shared logical-line joiner + cargo profile/package tokenizer, DRY F2/F3) | Mine | — |
| `crates/dt-guard/src/common/mod.rs` | Mine | — |
| `crates/dt-guard/src/release_build_profile.rs` (machinery only: joiner + tokenizer moved to `common::cargo_cli`) | Mine | — |
| `scripts/guards/list-coverage-suites.sh` (selector word-boundary on `DT_GUARD`) | Not mine, Minor-judgment | test |

---

## Planning

### Mechanism restated
Today the dev-tool binaries are built with the services' `[profile.release]` (fat LTO, 1 CGU), and their path `target/release/dt-{guard,story}` is written literally at ~15 code sites and ~20 doc sites. Changing the profile changes the path, so the profile change and the path migration are one invariant: **every producer and every consumer of a dev-tool binary resolves the path from one place, and that place derives the path from the profile name, which must exist in `Cargo.toml`.** A partial migration (producer writes `target/devtools/`, any consumer still reads `target/release/`) is broken or — worse — silently reads a stale old binary, so the whole thing lands in one commit.

### 1. `[profile.devtools]` (Cargo.toml)
`inherits = "release"`, `lto = false`, `codegen-units = 16`; opt-level 3 and `strip` inherited. `[profile.release]` untouched. `release-build-profile` is unaffected: devtools enables no debug-assertions and no Dockerfile selects it.

### 2. `scripts/lib/tool-bins.sh` — the one home
Bash-3.2-safe, source-safe (definitions only: no `set`, no `IFS`, no `exit`, no `readonly` so re-sourcing is harmless — same discipline as `_gate2_binding.sh`). Does NOT source `_common.sh`.
- `DT_TOOLS_PROFILE=devtools` — the one place the profile name is written outside `Cargo.toml`.
- `DT_TOOLS_ROOT` — repo root derived from the file's own `BASH_SOURCE` (so paths are absolute and CWD-independent; under run-story's `STORY_REPO_ROOT` seam they still point at the real tree's binary, which is exactly that seam's stated intent).
- `DT_GUARD_BIN` / `DT_STORY_BIN` = `$DT_TOOLS_ROOT/target/$DT_TOOLS_PROFILE/dt-{guard,story}`.
- `dt_build_tool <crate> [cargo-args…]` — the ONE cargo invocation shape (`cargo build --profile "$DT_TOOLS_PROFILE" -p <crate>`, run from `DT_TOOLS_ROOT`); refuses a crate that is not `dt-guard`/`dt-story`. `dt_build_tools [cargo-args…]` = both.
- Executable mode (only when run, not sourced): `scripts/lib/tool-bins.sh build` (one-command recovery that every remediation message and doc now names) and `scripts/lib/tool-bins.sh dt-story|dt-guard <args…>` (exec the binary; missing → loud error naming `build`). This is how SKILL docs invoke `dt-story` without restating the path.
- **Stale-binary removal**: after a successful build `dt_build_tool` deletes `target/release/<crate>` (only when the profile is not `release`). Nothing will read that path after this change and the guard forbids new readers, but every existing tree has a stale one, and a human with muscle memory running it would get a silently-old verdict. Deleting it turns that into "no such file" — fail loudly. (Ask reviewers: keep or drop.)

Override semantics unchanged: callers keep `DT_GUARD="${DT_GUARD:-$DT_GUARD_BIN}"`; run-story keeps its sentinel-gated seam (default assigned unconditionally, then the seam value).

### 3. Migration (every site, one commit)
Code: `compile.sh` (two `run_and_emit` tokens `cargo-build-dt-guard`/`cargo-build-dt-story` preserved via `dt_build_tool`), `_dt_guard_wrapper.sh`, `validate-story-manifest.sh`, `.githooks/pre-commit` (sources tool-bins.sh directly — it is bash-3.2-safe by construction — and build-if-missing uses `dt_build_tool dt-guard`), `run-story.sh` (default + header + two remediation strings), `preflight-story.sh` (existence check, three invocations, remediation text), `run-story.test.sh` (`REAL_DT_STORY` + messages), `counter-zero-init.test.sh`, `media-telemetry-deny.test.sh`, `list-coverage-suites.sh` (comments only; its invocation matcher is a different policy and stays), `ci.yml` (the coverage job asserts `DT_GUARD != $DT_GUARD_BIN`, sourcing tool-bins.sh; comments), `ci-client.yml` (`dt_build_tool dt-guard --quiet`). Docs: `crates/dt-story/src/manifest.rs` doc comment, runbook (§6.1 rows, §6.3 stale/missing binary, §8 rows, coverage rows), four SKILL.md files, ADR-0034 (§3 wrapper sample + §Negative + §When-to-Revisit amendment), TODO.md (close the build half of the sprawl entry; update path text in the open stale-binary entry; reword one historical command in the dt-guard-build entry).

### 4. Guard: new subcommand `dt-guard tool-bin-paths`
Two rules, one policy ("dev-tool binaries have one path home"):
- **R1 `tool-bin-path-literal`**: in every tracked + untracked-not-ignored text file, a literal `target/<segment>/dt-guard|dt-story`, or a `cargo build` line naming `-p dt-guard|dt-story` together with `--release` or `--profile`, outside `scripts/lib/tool-bins.sh`. Excluded: `docs/devloop-outputs/**`, `docs/debates/**` (dated records). The coverage job's `cargo build -p dt-guard --locked` (dev profile, instrumented) is not matched — it names no profile. The module's own fixtures build literals with `concat!` so it does not exclude itself.
- **R2 `tool-profile-drift`**: tool-bins.sh must carry exactly one `DT_TOOLS_PROFILE=<name>` assignment and root `Cargo.toml` must have `[profile.<name>]` with an `inherits` key (i.e. a custom profile — `DT_TOOLS_PROFILE=release` would re-introduce the cost silently). Parsed with `common::cargo_manifest`'s reader. PRECONDITION on unreadable/absent tool-bins.sh or Cargo.toml (never a pass).
Machinery reused: `common::git_changes` file enumeration, `common::status`, `common::explain`. Wrapper `scripts/guards/simple/validate-tool-bin-paths.sh` (3-line `_dt_guard_wrapper.sh` form; auto-discovered by `run-guards.sh`). Tests in-module (ADR-0034 §2): positive/negative per rule, exclusion proof, neutered-matcher proof.

**Why a new subcommand (43rd) rather than extending one**: no existing subcommand's policy contains this. `workspace-deps` is dependency-version SSoT; `release-build-profile` is the shipped-artifact debug-assertions premise (a devtools rule there would make its "enumerated inputs clean" token lie about scope); `bin-lib-single-compile` is module double-compilation. `list-coverage-suites.sh`'s grep is a bash CI selector for a different policy (invocations not routed through `$DT_GUARD`), and growing a bash policy contradicts ADR-0034. ADR-0034 is subcommand-per-policy; the sprawl trigger's cost half is what this devloop removes, and the ADR amendment records that count no longer drives build cost.

### 5. ADR-0030
Its only path reference is the devloop-helper binary (out of scope). @team-lead confirmed the brief meant ADR-0034 (incl. its §3 wrapper sample); **ADR-0030 is left untouched.**

### 6. Gate-1 amendments (folded from all six reviewers)
**tool-bins.sh contract** (stated in its header, enforced by `tool-bins.test.sh`):
- Every assignment unconditional — no `${X:-…}`, no `export`, no `readonly`, no `declare -g`, no assoc arrays; sourcing never reads env for a path, never changes shell options/IFS/CWD, never exits (security S1/B-i, test B). Ambient `DT_TOOLS_PROFILE`/`DT_GUARD_BIN`/`DT_STORY_BIN` are overwritten.
- `DT_TOOLS_CRATES="dt-guard dt-story"` — the one crate set; `dt_build_tool` refuses anything else, `dt_build_tools` iterates it, and R1 parses it (dry #1) so a third tool is covered for free.
- `DT_TOOLS_BUILD_CMD="scripts/lib/tool-bins.sh build"` — the one recovery command; every remediation message interpolates it (ops #1, obs #2). Runbook also names `bash scripts/lib/tool-bins.sh build` if the +x bit is lost; +x is committed (ops #6).
- Root: `CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd -P` (code-reviewer #4). Absolute paths.
- `dt_build_tool` runs `( cd "$DT_TOOLS_ROOT" && cargo build --profile "$DT_TOOLS_PROFILE" --target-dir "$DT_TOOLS_ROOT/target" -p <crate> "$@" )` — explicit `--target-dir` so an ambient `CARGO_TARGET_DIR` cannot split producer from consumer (security A/S5). Args forwarded with `shift; "$@"`, all function vars `local`.
- Stale-binary removal kept (all reviewers): after a SUCCESSFUL build only, profile≠release only, crate from the allow-list only, `[ -e ]`-gated `rm -f -- "$DT_TOOLS_ROOT/target/release/$crate"`, one stderr line naming what was removed and where the binary now is; a failed `rm` fails the build (no `|| true`).
- Executable mode only under `[ "${BASH_SOURCE[0]}" = "$0" ]`, with `set -euo pipefail` inside that branch: `build`; `dt-guard|dt-story <args>` execs `DT_*_BIN` — never `$DT_GUARD`/`$DT_STORY` (security B-ii); missing binary → `STATUS=FAIL REASON=<crate>-binary-missing` + the build command; unknown verb → usage, exit 2.

**Consumers:**
- Overrides unchanged: `DT_GUARD="${DT_GUARD:-$DT_GUARD_BIN}"` at the wrapper and the two suites (literal line kept so coverage selection is unchanged; test #1). run-story sources tool-bins.sh from its BASH_SOURCE dir (never the seam-redirectable REPO_ROOT), assigns `DT_STORY="$DT_STORY_BIN"` unconditionally, then applies the seam. No fallback chain anywhere (S3).
- `validate-story-manifest.sh` missing binary → `STATUS=FAIL REASON=dt-story-binary-missing` (obs #1); `dt-guard-binary-missing` string unchanged.
- pre-commit build-if-missing prints a one-time devtools-build line (ops #2).
- ci.yml coverage assertion compares against sourced `$DT_GUARD_BIN` (S6/test #3); error texts reworded.
- compile.sh budget prose updated to describe the devtools profile (obs #4).

**Coverage selector** (test A): `list-coverage-suites.sh` selection tightened to a word-boundary `DT_GUARD` (`(^|[^A-Za-z0-9_])DT_GUARD([^A-Za-z0-9_]|$)`) so naming `DT_GUARD_BIN` alone does not select a suite; its output pasted in main.md (must be exactly the two current suites). One comment at its SECOND GREP names `dt-guard tool-bin-paths` as owner of the path-literal half (dry a).

**Guard `tool-bin-paths`, refined:**
- R1 literal: `target/<segment>/<crate>` for each crate in `DT_TOOLS_CRATES` — applies to tool-bins.sh too (its paths must derive from `$DT_TOOLS_PROFILE`) (code-reviewer #1).
- R1 cargo: a `cargo build` logical line (backslash continuations joined) naming a tool crate via `-p X`/`-pX`/`--package X`/`--package=X` together with `--release`/`-r`/`--profile X`/`--profile=X`. In tool-bins.sh only the exact `--profile "$DT_TOOLS_PROFILE"` form is accepted. The coverage job's `cargo build -p dt-guard --locked` is a non-match control (code-reviewer #2).
- Scope: all tracked + untracked-not-ignored text files (scripts, .githooks, .github, .claude/skills, crates, docs/runbooks, ADRs, TODO.md). Excluded: `docs/devloop-outputs/**`, `docs/debates/**` only. ADR/TODO/runbook prose is reworded without the literal rather than excluded.
- R2: exactly one `DT_TOOLS_PROFILE=` and one `DT_TOOLS_CRATES=` assignment; `[profile.<name>]` exists in Cargo.toml (via `cargo_manifest::read_root_manifest`, no line scan) with `inherits` set; `release`/`dev`/`test`/`bench` refused.
- Vacuity: zero files enumerated, or tool-bins.sh not in the enumerated set, or unreadable Cargo.toml → distinct PRECONDITION token, never clean (obs #2, code-reviewer #3, test D). OK token carries the files-scanned count.
- Tests: in-module per-shape specific-finding asserts (`target/release/dt-guard`, `target/devtools/dt-story`, `target/debug/dt-guard`, `cargo build --release -p dt-story`, `--package`, `-r`, `--profile=…`, continuation line), non-match controls, R2 cases (missing, duplicate, absent profile, no inherits, `release`), exclusion proof both directions (docs/debates passes, docs/runbooks fails), neutered-matcher proof. Fixture literals built with `concat!`. e2e `tests/tool_bin_paths_e2e.rs`: a real `git init` temp repo (production enumeration path incl. untracked), positive control that must hit with token + file:line, and a real-repo PASS with a files-scanned floor.
- `release_build_profile_e2e.rs` + `dt-guard release-build-profile` re-run against the real repo; expectations unchanged (devtools sets no debug-assertions; no Dockerfile selects it).

**`[profile.devtools]` stanza** (ops #5): `inherits = "release"`, `lto = false`, `codegen-units = 16`; `opt-level = 3`, `strip = true`, `debug-assertions = false` inherited from release (not restated).

**run-story** (test #4, ops #7): existing case (A) already refuses an ambient seam var without the sentinel; add a case with ONLY `DT_STORY=/bogus` (no sentinel, no root override) → `SEAM-OVERRIDE-WITHOUT-TEST-SENTINEL`, exit 2, no work done. Resume: `--stop-after` validation (before preflight) and preflight both run before task selection and fail loudly on a missing binary naming the build command; no resume pointer / incident record stores a binary path (to be verified and recorded).

**CI cache** (ops #4): rust-cache v2.9.2 cleaning can't be verified from here. main.md will state the expected cold cost (devtools cold of both crates ~16s at jobs=6 locally) and that the first `ci-client` run on its isolated key is cold once.

### Wider class, checked (dry b)
- `media-vector-gen` is NOT built `--release`: its only in-tree use is `cargo run -p media-vector-gen` (dev profile) in `validate-frame-vectors.sh`.
- `devloop-helper` already has one home (`HELPER_BINARY` in `infra/devloop/devloop.sh`, its own `HELPER_TARGET_DIR`; build line pinned by `scripts/setup.test.sh`). Neither restates a path, so the guard names only the `DT_TOOLS_CRATES` set.

### Wider class surfaced
- `docs/TODO.md` §"dt-guard wrappers exec … gated only on existence, never freshness" stays open: a stale `target/devtools/dt-guard` is still possible; this change only removes the *old-path* staleness class.

### Measurement plan (acceptance)
Same host (32 cores, `build.jobs = 6` repo default, WSL2 devloop container), commands recorded, N≥3 each, ranges:
- Isolated `--target-dir` matrix (supplement): cold / no-op / crate-only rebuild (touch `main.rs`; touch `common/status.rs`), release vs devtools.
- Real `scripts/layer1.sh`: cold (= `target/devtools` and `target/release/dt-*` removed; shared workspace deps warm — stated) and warm, before/after, vs 38s/19s.
- `scripts/layer3.sh` before/after with the resolved binary path shown in the log; note any `BUDGET_BREACH` / `BUDGET_TOTAL_BREACH` change (obs #3).
- Per-consumer default/missing proof (test E): each consumer with no override resolves `target/devtools/…`; with that binary moved aside AND a stale release-path binary present, it fails with its missing-binary token. Pre-commit build-if-missing exercised.

Baseline so far (isolated target-dir, N=1): release cold 37.7s / no-op 0.1s / touch main.rs 18.6s / touch common/status.rs 33.3s; devtools cold 15.6s / 0.1s / 1.0s / 6.7s. Running all 42 subcommands against the repo takes ~3.3s with either binary (LTO off costs no guard runtime). `scripts/layer3.sh` before: 168s.

### Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |

Lead rulings at Gate 1: ADR-0030 left untouched (its only path reference is `devloop-helper`, out of scope — the brief was wrong); the pre-existing duplicate `dt_guard_bin()` in `crates/dt-guard/tests/credential_leak_key_custody_fixtures.rs` is fixed in-loop. `validate-cross-boundary-classification.sh`: STATUS=OK.

---

## Pre-Work

None. Baseline measurements were taken before implementation, in isolated `--target-dir`s with no tree changes (§Planning, §Acceptance measurements).

---

## Implementation Summary

### Build profile + the one home
| Item | Before | After |
|------|--------|-------|
| dt-guard/dt-story profile | `[profile.release]` (fat LTO, 1 CGU) | `[profile.devtools]`: `inherits = "release"`, `lto = false`, `codegen-units = 16` (opt-level 3, strip, no debug-assertions inherited); `[profile.release]` unchanged |
| Binary paths | release-profile path literal at ~15 code + ~20 doc sites | `DT_GUARD_BIN` / `DT_STORY_BIN` from `scripts/lib/tool-bins.sh` (absolute, BASH_SOURCE-rooted) |
| Build | `cargo build --release -p …` at compile.sh, pre-commit, ci-client | `dt_build_tool <crate>` (explicit `--profile "$DT_TOOLS_PROFILE" --target-dir "$DT_TOOLS_ROOT/target"`) |
| Recovery text | `cargo build --release -p dt-guard -p dt-story` (would rebuild the wrong artifact) | `DT_TOOLS_BUILD_CMD` = `scripts/lib/tool-bins.sh build`, interpolated everywhere |
| Docs/SKILL invocation | release-profile path literal | `scripts/lib/tool-bins.sh dt-story <args>` (exec mode; `<crate>-binary-missing` token when absent) |
| Stale release-profile copy | silently runnable | deleted after a successful devtools build, announced on stderr; never read by anything |
| `validate-story-manifest.sh` missing binary | bare text | `STATUS=FAIL REASON=dt-story-binary-missing` + build command |

### Guard `dt-guard tool-bin-paths` (43rd subcommand; rationale in §Planning §4)
`crates/dt-guard/src/tool_bin_paths.rs` + wrapper `scripts/guards/simple/validate-tool-bin-paths.sh`. Rule tokens: `tool-bin-path-literal`, `tool-bin-build-outside-home` (R1 split into its two shapes so the token names the fix), `tool-profile-drift`; preconditions `tool-bins-unreadable`, `tool-bins-assignment-unusable`, `tool-bins-manifest-unreadable`, `tool-bins-enumeration-failed`, `tool-bins-home-not-enumerated`. Crate set + profile name parsed from tool-bins.sh; Cargo.toml via `common::cargo_manifest::read_root_manifest`; files via `common::git_changes` (tracked + untracked-not-ignored). Runbook row added under §6.3.

### Additional Changes
- `scripts/lib/tool-bins.test.sh` (45 assertions, wired in `scripts/layer3.sh`); neutering check: dropping `--target-dir` and env-defaulting `DT_TOOLS_PROFILE` turned it red (6+ failures), restored → green.
- `run-story.test.sh` case A2: ambient `DT_STORY` alone without sentinel → `SEAM-OVERRIDE-WITHOUT-TEST-SENTINEL`, exit 2, no work.
- `list-coverage-suites.sh` selection is whole-word `DT_GUARD` (explicit POSIX ERE); verified with awk and mawk on a `DT_GUARD_BIN`-only line (not selected) and a `DT_GUARD="${DT_GUARD:-…}"` line (selected). Output after the change — exactly the two suites:
  ```
  scripts/guards/counter-zero-init.test.sh
  scripts/guards/media-telemetry-deny.test.sh
  ```
- Duplicate `dt_guard_bin()` removed from `crates/dt-guard/tests/credential_leak_key_custody_fixtures.rs` (uses `tests/common`).
- Review round 1: new `crates/dt-guard/src/common/cargo_cli.rs` holds the one `\`-continuation joiner and the cargo profile/package tokenizer, now shared by `release_build_profile` and `tool_bin_paths` (their private copies deleted). `tool_bin_paths` reuses `common::ident::is_ident_byte`, reports the FIRST offending build on a line, matches multi-segment literals (`target/<triple>/<profile>/<crate>`), and renames its precondition to `tool-bins-home-not-enumerated`. The hermetic `git()` e2e helper is hoisted into `tests/common`. The pre-commit notice no longer quotes a duration.
- ADR-0034: §3 sample, §Negative and §When-to-Revisit amended (dated 2026-10-26). ADR-0030 untouched (@team-lead: brief mis-cited it).
- `docs/TODO.md`: sprawl entry build half closed (count/discoverability half open); path text in the open stale-binary entry and one historical sentence reworded.
- Wider class checked: no other site; `dt-guard tool-bin-paths` on the real repo: `STATUS=OK REASON=tool-bin-paths-clean-files-1582`.
- `dt-guard release-build-profile` and `release_build_profile_e2e.rs` unchanged and green (Layer 4).
- Resume path (ops #7): `--stop-after` validation and preflight both run before task selection and fail loudly on a missing binary; run-story persists no binary path (DT_STORY appears only in log lines).
- CI cache (ops #4, ruled a stated residual, not a deferral): workspace crates (dt-guard, dt-story) always rebuild in CI — rust-cache never retains workspace-member artifacts in any profile, so the old path also rebuilt them every run. Retention of the devtools profile's third-party deps is unverified; worst case is a fully cold devtools build of both crates, 11.7–15.6s at jobs=6, still below the prior ~38s LTO build. ci-client's isolated key is cold once.

### Acceptance measurements
Host: WSL2 devloop container, 32 cores, `build.jobs = 6` (`.cargo/config.toml`). Script: `scratchpad/m/measure.sh` (commands inline below). N=3 each.

| What | Before (release profile) | After (devtools) |
|---|---|---|
| Isolated `--target-dir`, fully cold incl. deps — `cargo build --release\|--profile devtools --locked -q -p dt-guard -p dt-story --target-dir <tmp>` | 37.8–38.5s | 11.7–15.6s |
| Isolated, `touch crates/dt-guard/src/main.rs` | 20.8–21.1s | 1.0s |
| Isolated, `touch crates/dt-guard/src/common/status.rs` | 33.7–34.4s | 7.1–8.2s |
| In-tree `scripts/lang/rust/compile.sh` (HEAD's vs new), tool crates cold (`cargo clean -p dt-guard -p dt-story` for that profile; workspace deps warm) | 43.8–45.5s | 12.4–13.6s |
| In-tree `compile.sh`, warm no-op | 4.1–4.3s | 4.1–4.3s |
| In-tree `compile.sh`, after a dt-guard edit (`touch common/status.rs`) | 38.4–38.7s | 12.1–13.2s |
| Full `scripts/layer1.sh`, tool crates cold (`cargo clean -q --release\|--profile devtools -p dt-guard -p dt-story`; release-path binaries removed for the after runs) | 58.5–74.1s | 14.4–14.9s |
| Full `scripts/layer1.sh`, warm no-op | 6.4–9.5s | 5.5–5.8s |
| Full `scripts/layer1.sh`, after a dt-guard edit (`touch crates/dt-guard/src/common/status.rs`; includes the debug workspace rebuild) | 45.8–67.1s | 14.0–14.8s |
| `scripts/layer3.sh` (release-profile dt-guard via `DT_GUARD=` override vs devtools default; same tree) | 161.7–167.0s | 155.1–161.8s |
| All 43 dt-guard subcommands vs the repo, one pass | 3.75–3.77s | 3.73–3.74s |

Every row is N=3 (range = min–max). Full `layer1.sh` before/after (T1, second session, nothing else building): "before" ran `scripts/layer1.sh` with b38838d4's `scripts/lang/rust/compile.sh` swapped in (the only Layer-1 file this change touches; restored by an EXIT trap, verified by `git diff`), "after" with this tree's; script `scratchpad/m/l1.sh`. Before run #1 (74.1s cold / 67.1s edit) is the high end because it also rebuilt the debug workspace after the review-round dt-guard edits. An earlier session's after-rows (14.1–14.3 / 5.1 / 13.0s) agree. The brief's 38s/19s figures are reproduced by the isolated rows (38s cold; 21s/34s per edit). Isolated "no-op" rows for runs #2/#3 are omitted — the alternating touches made them rebuilds, not no-ops (true no-op: 0.1s both profiles). **LTO off costs no guard runtime**: Layer 3 did not regress (slightly faster, within noise). Budget signalling (obs #3): unchanged — layer-fast still emits the pre-existing `BUDGET_BREACH LAYER=3/4` and `BUDGET_TOTAL_BREACH` (`docs/TODO.md` §BUDGET_BREACH entry); Layer 1 at 5s warm / 13–14s cold is under the 20s per-layer budget, where the release-profile build (38–45s cold) was not.

### Matcher mutation proof for `tool-bin-paths` (test T2)
Each mutation applied to the source, `cargo test -q -p dt-guard --lib -- tool_bin_paths cargo_cli` run, file restored (final run: 19 passed, 0 failed). Script `scratchpad/mut/mut.py`.

| Mutation | Tests that went red |
|---|---|
| (a) `selected_packages` drops `--package`, `--package=`, `-pX` | `cargo_cli::selected_packages_all_forms`, `tool_bin_paths::cargo_build_spellings_hit`, `tool_bin_paths::crate_set_is_read_not_hardcoded` |
| (b) `selected_profile` drops `-r` | `cargo_cli::selected_profile_all_forms`, `cargo_build_spellings_hit`, `crate_set_is_read_not_hardcoded` |
| (c) `logical_lines` stops joining `\` | `cargo_cli::logical_lines_joins_backslash_continuations`, `cargo_build_continuation_lines_are_joined` |
| (d) `docs/` added to `EXCLUDED_PREFIXES` | `exclusion_is_only_the_dated_record_dirs` |
| (e) `find_path_literal` skips the leading word-boundary check | `path_literal_non_matches` |
| (f) the `files.contains(TOOL_BINS_REL)` vacuity check always passes | `vacuity_is_a_precondition_never_clean` |

### Per-consumer default / missing-binary proof (test E)
Section B ran with `target/devtools/dt-{guard,story}` moved aside AND **working** copies placed at the release-profile path — every consumer failed loudly; none ran the stale copy. (The first capture below piped through `tail`, so its run-story `rc=0` was `tail`'s; re-run with the exit code captured directly is in "Section B, rc captured directly" — run-story exits 2 on both paths, the runner suite exits 2.) Preflight's success case ran a copy truncated before the substrate section so it would not patch settings.json.
```
### A. defaults resolve to the devtools binaries (no override set)
DT_GUARD_BIN=/work/target/devtools/dt-guard
DT_STORY_BIN=/work/target/devtools/dt-story
-- scripts/guards/counter-zero-init.test.sh
scripts/guards/counter-zero-init.test.sh: 19 passed, 0 failed
-- scripts/guards/media-telemetry-deny.test.sh
scripts/guards/media-telemetry-deny.test.sh: 92 passed, 0 failed
-- validate-tool-bin-paths.sh (wrapper)
STATUS=OK REASON=tool-bin-paths-clean-files-1582
-- validate-story-manifest.sh
rc=0
-- preflight (binary checks only)
PREFLIGHT-BINARY-CHECKS-PASSED

### B. devtools binaries moved aside, WORKING stale copies at the release-profile path
target/release/dt-guard
target/release/dt-story
-- scripts/guards/counter-zero-init.test.sh
  - [precondition] dt-guard binary missing or not executable at /work/target/devtools/dt-guard

scripts/guards/counter-zero-init.test.sh: 0 passed, 1 failed
rc=1
-- scripts/guards/media-telemetry-deny.test.sh
  - [precondition] dt-guard binary missing or not executable at /work/target/devtools/dt-guard

scripts/guards/media-telemetry-deny.test.sh: 0 passed, 1 failed
rc=1
-- validate-tool-bin-paths.sh (wrapper)
STATUS=FAIL REASON=dt-guard-binary-missing
rc=1
-- validate-story-manifest.sh
STATUS=FAIL REASON=dt-story-binary-missing
dt-story not built at /work/target/devtools/dt-story but manifests exist — run: scripts/lib/tool-bins.sh build
rc=1
-- preflight-story.sh (full script)
PREFLIGHT: FAIL guard binaries not built at /work/target/devtools (run: scripts/lib/tool-bins.sh build)
rc=2
-- run-story.test.sh
run-story.test.sh: PRECONDITION — dt-story not built at /work/target/devtools/dt-story but the runner suite needs it (run: scripts/lib/tool-bins.sh build)
-- tool-bins.sh dt-story exec mode
STATUS=FAIL REASON=dt-story-binary-missing
dt-story not built at /work/target/devtools/dt-story — run: scripts/lib/tool-bins.sh build
rc=1
-- run-story.sh --stop-after (pre-preflight path)
[23:34:14] STORY_RUN: RUN-DIR /home/dev/.cache/devloop/story-runs/story-runner/2026-08-11-story-runner-hardening source=DEVLOOP_STORY_RUN_BASE
[23:34:14] STORY_RUN: STOP-AFTER-UNVERIFIABLE — '/work/target/devtools/dt-story list-tasks' could not answer, so --stop-after cannot be validated. Causes: dt-story not built, a stale binary predating the 'list-tasks' verb, or an unparseable
rc=0
```
Section B, rc captured directly (`cmd >out 2>&1; echo rc=$?`, no pipe; same moved-aside + stale-copy state):
```
-- run-story.test.sh
rc=2
run-story.test.sh: PRECONDITION — dt-story not built at /work/target/devtools/dt-story but the runner suite needs it (run: scripts/lib/tool-bins.sh build)
-- run-story.sh --stop-after=1
rc=2
[01:04:08] STORY_RUN: RUN-DIR /home/dev/.cache/devloop/story-runs/story-runner/2026-08-11-story-runner-hardening source=DEVLOOP_STORY_RUN_BASE
[01:04:08] STORY_RUN: STOP-AFTER-UNVERIFIABLE — '/work/target/devtools/dt-story list-tasks' could not answer, so --stop-after cannot be validated. Causes: dt-story not built, a stale binary predating the 'list-tasks' verb, or an unparseable manifest. To tell them apart run: /work/target/devtools/d
-- run-story.sh (no --stop-after: preflight path)
rc=2
[01:04:08] STORY_RUN: RUN-DIR /home/dev/.cache/devloop/story-runs/story-runner/2026-08-11-story-runner-hardening source=DEVLOOP_STORY_RUN_BASE
PREFLIGHT: FAIL guard binaries not built at /work/target/devtools (run: scripts/lib/tool-bins.sh build)
```
Pre-commit build-if-missing (hook block extracted and run under `/bin/bash -e`, devtools dt-guard absent, stale release-path copy present):
```
     (dt-guard not built at /work/target/devtools/dt-guard — one-time 'devtools'-profile build, may take ~30s cold)
tool-bins: removed stale /work/target/release/dt-guard (dt-guard now builds to /work/target/devtools/dt-guard)
STATUS=OK REASON=todo-tracking-clean
```
Not verified: an actual bash 3.2 interpreter (none available here); tool-bins.sh uses no bash-4 constructs by inspection (no `local -`, `declare -g`, assoc arrays, `${x,,}`, `mapfile`, `[[ -v ]]`).

---

## Files Modified

```
 27 files changed, 185 insertions(+), 101 deletions(-)   (git diff --stat HEAD, tracked only)
 new: crates/dt-guard/src/tool_bin_paths.rs (795), crates/dt-guard/tests/tool_bin_paths_e2e.rs (129),
      scripts/lib/tool-bins.sh (117, +x), scripts/lib/tool-bins.test.sh (226, +x),
      scripts/guards/simple/validate-tool-bin-paths.sh (5, +x)
```

### Key Changes by File
| File | Changes |
|------|---------|
| `Cargo.toml` | `[profile.devtools]` |
| `scripts/lib/tool-bins.sh` | the one home: profile, crates, paths, `dt_build_tool(s)`, `build`/exec CLI, stale removal |
| `scripts/lang/rust/compile.sh` | builds via `dt_build_tool`; tokens unchanged; prose updated |
| `scripts/guards/simple/_dt_guard_wrapper.sh`, `validate-story-manifest.sh` | default from tool-bins.sh; dt-story missing token |
| `.githooks/pre-commit` | sources tool-bins.sh; build-if-missing via `dt_build_tool dt-guard` with a one-time notice |
| `scripts/workflow/{run-story,preflight-story}.sh`, `run-story.test.sh` | default/recovery from tool-bins.sh (sourced from the real root); A2 seam case |
| `scripts/guards/{counter-zero-init,media-telemetry-deny}.test.sh` | `${DT_GUARD:-$DT_GUARD_BIN}` |
| `scripts/guards/list-coverage-suites.sh` | whole-word selector; comments |
| `.github/workflows/ci.yml`, `ci-client.yml` | coverage anti-mask vs sourced `$DT_GUARD_BIN`; ci-client builds via `dt_build_tool` |
| `crates/dt-guard/src/tool_bin_paths.rs`, `main.rs`, `lib.rs`, `tests/tool_bin_paths_e2e.rs` | new guard + e2e |
| `crates/dt-guard/tests/{common/mod.rs,credential_leak_key_custody_fixtures.rs,cross_boundary_scope_e2e.rs}` | one `dt_guard_bin()`, one hermetic `git()` |
| `crates/dt-guard/src/common/cargo_cli.rs` (new), `release_build_profile.rs`, `common/mod.rs` | shared joiner + cargo flag tokenizer |
| `scripts/layer3.sh` | wires tool-bins.test.sh |
| docs (runbook, 4 SKILLs, ADR-0034, TODO, INDEX, manifest.rs comment) | path/command migration, new runbook row, ADR amendment, TODO close-out |

---

## Devloop Verification Steps

Gate 2: `DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` on the frozen tree after Gate 3. TOTAL_RESULT=OK, 2002s. Log: `/tmp/claude-1000/-work/4a4befac-a19a-495e-8563-7f4939000f11/scratchpad/layer-all-devtools.log`. Two earlier Lead fast checks (`layer-fast.sh`), one before review and one after the review fixes, were also green on L1–6.

### Layer 1: compile (workspace debug + dt-guard/dt-story `devtools` + release feature gate)
**Status**: PASS
**Duration**: ~8s
**Output**: Tool crates resolved to `target/devtools/`.

### Layer 2: fmt
**Status**: PASS
**Duration**: ~5s
**Output**: Clean.

### Layer 3: Guards + audit
**Status**: ALL PASS
**Duration**: ~183s

Includes the new `validate-tool-bin-paths.sh` (STATUS=OK, files-scanned floor met), the `tool-bins.test.sh` self-test (45/0), knowledge-index, cross-boundary scope and classification. `WARN BUDGET_TOTAL_BREACH GUARD_AUDIT_DURATION=188 BUDGET=90` is pre-existing and tracked in `docs/TODO.md`; this change did not cause it.

### Layer 4: Unit Tests
**Status**: PASS
**Duration**: ~158s
**Output**: All workspace tests are green, including dt-guard (tool_bin_paths in-module and e2e, release_build_profile, cargo_cli) and dt-story.

### Layer 5: All Tests (Integration)
**Status**: PASS
**Duration**: ~2s
**Tests**: All passed.

### Layer 6: Clippy / audit / buf-breaking
**Status**: PASS
**Duration**: ~5s
**Output**: No warnings.

### Layer 7: Env-tests
**Status**: PASS
**Duration**: ~1641s
**Output**: `STATUS=OK REASON=env-tests-passed`; browser e2e 30/30 passed (`browser-e2e-passed`).

(Semantic-guard was not spawned: no checks.md surface. See Loop State.)

---

## Code Review Results

### Security Specialist
**Verdict**: CLEAR
**Findings**: 0 found, 0 fixed, 0 deferred

No findings. Verified: tool-bins.sh assignments are unconditional, there are no exports and no reads of path values from the environment, and the target dir is pinned; the stale-release delete is allowlisted and only runs after a successful build; the run-story DT_STORY sentinel gate is preserved (new A2 case); the override surface is not widened; the CI coverage assertion is not vacuous.

### Test Specialist
**Verdict**: RESOLVED-FIXED
**Findings**: 6 found, 6 fixed, 0 deferred

- T1: full `scripts/layer1.sh` before/after measured, N=3, ranges (before 58.5–74.1s cold, after 14.4–14.9s).
- T2: matcher mutation proof (a)–(f) recorded; the mislabelled test was renamed `crate_set_is_read_not_hardcoded`.
- T3: the not-a-git-repo case pins the ENUMERATION_FAILED token.
- T4: multi-segment `target/<triple>/release/dt-*` paths are matched, with hit and non-match cases.
- T5: the coverage-selector negative case has a non-vacuity precondition, and both floor suites are asserted.
- T6: stale "~30s" pre-commit notice (also flagged by operations and code-reviewer).

### Observability Specialist
**Verdict**: CLEAR
**Findings**: 0 found, 0 fixed, 0 deferred

No findings. The `<crate>-binary-missing` tokens, the tool-bin-paths tokens and the PRECONDITION tokens all match the runbook rows; the OK token carries a files-scanned count; Layer 1 is now under its 20s budget.

### Code Quality Reviewer
**Verdict**: RESOLVED-FIXED
**Findings**: 5 found, 4 fixed, 0 deferred (F5 closed by the Gate-1 ruling)

- F1: `check_cargo_build` was too long and reported the last offence rather than the first. It now uses the shared `common::cargo_cli` (merged with DRY F3) and returns on the first offence, pinned by a test.
- F2: precondition token renamed to `tool-bins-home-not-enumerated`.
- F3: stale "~30s" notice dropped.
- F4: two doc comments rewrapped.
- F5: the `dt_guard_bin` dedup is in scope per the Gate-1 Lead ruling; no change.
Bash 3.2 safety of tool-bins.sh and the pre-commit hook was checked by reading only, because no bash 3.2 binary is available.

### DRY Reviewer
**Verdict**: RESOLVED-FIXED

**True duplication findings** (entered fix-or-defer flow):
- F1 (blocker): `is_word_byte` duplicated `common::ident::is_ident_byte`. Now imported.
- F2: second `logical_lines()`. Hoisted to `common/cargo_cli.rs`.
- F3: second cargo profile-flag tokenizer. Hoisted to `common/cargo_cli.rs`. `release_build_profile` now also trims wrapping punctuation (`--release;` reads as Release), which is stricter, and the repo is still clean.
- F4: third e2e `fn git`. The hermetic version was hoisted to `tests/common`; three suites use it.

**Extraction opportunities** (appended to `docs/TODO.md`):
None

### Operations Reviewer
**Verdict**: RESOLVED-FIXED
**Findings**: 2 found, 2 fixed, 0 deferred

- F1: the recorded rc=0 for `run-story --stop-after` was `tail`'s exit code. Re-captured directly, every missing-binary path exits 2; there is no fail-open.
- F2: stale "~30s" pre-commit notice dropped.
CI-cache ruling: acceptable as a stated residual, not a deferral. rust-cache never keeps workspace-crate artifacts, so the worst case is 11.7–15.6s cold, which is still below the prior ~38s.

---

## Accepted Deferrals

**Each entry here is an issue the devloop chose NOT to fix.** Every bullet is a cost shift: the implementer didn't pay the fix-now cost, so a future reader will pay fix-later cost + tracking overhead. List only what was actually deferred — not "follow-ups" or "future improvements" or "potential extractions." If something was fixed, it doesn't belong here.

**Tech debt entries themselves live in `docs/TODO.md`. This section holds only pointers to those entries.** Do not create a `TODO.md` at the repo root or anywhere else — there is exactly one `docs/TODO.md` for the whole project. Do not inline the debt body here — multi-line entries belong in `docs/TODO.md`, not in this section.

Each pointer is exactly one bullet of the form `- \`docs/TODO.md\` §SECTION-NAME — one-line hook (≤80 chars)`. If you wrote more than one line per entry, you're writing it in the wrong file — move the body to `docs/TODO.md` and leave only the pointer here.

Examples:

```
- `docs/TODO.md` §Observability Debt — orphan recording-site audit follow-up
- `docs/TODO.md` §Cross-Service Duplication (DRY) — extract record_token_refresh_metrics
```

or:

```
- (none surfaced in this devloop)
```

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `b38838d461709892787c467a3579f76f3a624431`
2. Review all changes: `git diff b38838d4..HEAD`
3. Soft reset (preserves changes): `git reset --soft b38838d4`
4. Hard reset (clean revert): `git reset --hard b38838d4`
5. For schema changes: N/A (no migrations)
6. For infrastructure changes: no cluster manifests touched. After a revert, every checkout needs one `cargo build --release -p dt-guard -p dt-story` (the reverted tree's own recovery command): the devtools build deleted the release-path binaries, and the reverted scripts read that path. They fail loudly (`dt-guard-binary-missing`) until it runs, and Layer 1 rebuilds them anyway. `target/devtools/` is inert after a revert and can be deleted.
7. **Safe-revert unit**: **Answer**: `the whole commit`. The profile, `scripts/lib/tool-bins.sh`, and every producer and consumer site form one invariant: a partial revert where a producer writes one path while any consumer reads the other gives missing-binary failures. Worse, if a stale copy exists at the path being read, it gives a silently stale guard verdict. The one independently revertable piece is the review-round DRY extraction (`common/cargo_cli.rs`, the hoisted test `git()`, the `dt_guard_bin()` dedup), which is behaviour-identical machinery. Reverting it alone would only restore duplication, but it is not worth splitting.

---

## Issues Encountered & Resolutions

### Issue 1: `IFS=$'\n\t'` callers would not split the space-separated crate list
**Problem**: `compile.sh` and the self-tests run with `IFS=$'\n\t'`, so `for c in $DT_TOOLS_CRATES` would have seen one crate named "dt-guard dt-story" and the allow-list would have refused everything.
**Resolution**: `local IFS=' '` inside `dt_build_tool`/`dt_build_tools` (bash-3.2-safe); pinned by tool-bins.test.sh case (9).

### Issue 2: The guard's own fixtures and self-test tripped the guard
**Problem**: The first real-repo run of `tool-bin-paths` reported 45 hits. These included the literal paths and `cargo build` strings in its own unit tests, in `tool-bins.test.sh`, and in a `list-coverage-suites.sh` comment.
**Resolution**: Fixture strings are assembled (`concat!`, `{CB}` placeholders, an `OLD` path variable) and the comments are reworded, with no new exclusions. Same mechanism later: a regex in tool-bins.test.sh contained the bare `DT_GUARD` and pulled the file into the CI coverage selection; the name is now assembled at runtime.

### Issue 3: Mis-captured exit code in the first proof run
**Problem**: The first per-consumer proof piped run-story through `tail`, so it recorded `rc=0` for a path that exits 2. @operations flagged this as a possible fail-open.
**Resolution**: Re-ran with the rc captured directly (`cmd >out 2>&1; echo rc=$?`). All three run-story paths exit 2. Both captures are recorded.

### Issue 4: Layer 3 red on a reviewer's concurrent knowledge-index edit
**Problem**: A layer-fast run failed `validate-knowledge-index` on `docs/specialist-knowledge/dry-reviewer/INDEX.md` (76 lines, plus a `build|dt-story` span parsed as a path). That was an in-progress edit in the shared tree.
**Resolution**: Reported it to the dry-reviewer, who fixed it. The file is theirs and was not edited from this loop; it is included in the commit at their request.

---

## Lessons Learned

1. Measure the cause before the remedy. The sprawl trigger pointed at subcommand count, but the cost was the LTO profile, and a 3-binary split would have kept it. A one-flag experiment (`CARGO_PROFILE_RELEASE_LTO=false`) settled what a debate would have argued about.
2. Changing where an artifact is built moves its path. When the build can leave a working copy at the old path, the danger in a partial migration is a stale verdict, not a crash. Deleting that copy in the build helper turns stale into missing.
3. A guard that scans "every instruction file" must be able to scan itself and its own tests. Build fixture literals from pieces rather than adding exclusions, because each exclusion is a place the next stale path can hide.
4. Capture exit codes directly in evidence runs (`cmd >out 2>&1; echo rc=$?`). A pipeline's rc is the last stage's, and a fail-open claim can't be reviewed from a mis-captured rc.
5. A single sourced shell file can be the SSoT for both shell and Rust: the guard parses `tool-bins.sh`'s two assignments rather than holding a copy, so a new tool crate needs no guard change.

---

## Appendix: Verification Commands

```bash
# Commands used for verification
./scripts/verify-completion.sh --layer full

# Individual steps
cargo check --workspace
cargo fmt --all --check
./scripts/guards/run-guards.sh
DATABASE_URL=... cargo test --workspace
DATABASE_URL=... cargo clippy --workspace --lib --bins -- -D warnings
./scripts/guards/semantic/credential-leak.sh path/to/file.rs
```
