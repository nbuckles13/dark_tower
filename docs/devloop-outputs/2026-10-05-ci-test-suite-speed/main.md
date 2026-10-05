# Devloop Output: Cut CI Test Suite from ~29 min to ~10 min

**Date**: 2026-10-05
**Task**: Cut the CI Test Suite job from ~29 min (cold cache) to ~10 min — single-compile service crates (thin `main.rs` over lib), parallel test binaries (nextest evaluation), working Rust cache, job split preserving the Gate-2 backstop, drop duplicate setup steps, Rust release notification. Closes docs/TODO.md §Polyglot Pipeline Follow-ups "CI Test Suite takes ~29 min cold; target ~10 min".
**Specialist**: infrastructure
**Mode**: Agent Teams (v2) — full, Gate-1 present
**Branch**: `feature/5-ci-speed`
**Duration**: ~3h20m (19:50–23:30)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `3651ebf78b868cb673fc1a3d81cafe256b5b8ae5` |
| Branch | `feature/5-ci-speed` |
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
| Paired Auth Controller | `paired-auth-controller` (GSA owner: `crates/ac-service/src/crypto/mod.rs` stale-allow removal + ac main.rs) |
| Semantic Guard | `semantic-guard` (included: service `main.rs`/config visibility changes are production code near credential-bearing config types) |

Lead note: INDEX.md / review-protocol.md contents were passed to teammates as mandatory first-read paths rather than inlined verbatim (size: ~136 KB across the roster).

---

## Task Overview

### Objective
See Task line. Measured baseline (run 36923344871, cold cache): setup ~3.5 min (sqlx-cli 51s, dt-guard release build 65s), L1 236s, L3 171s, L4 Rust ~14.5 min (test compile 3m54s + ~620s serial test binaries), then L4 TS/L5/L6.

### Out of scope
Layer 3 self-test speed (TODO "Layer 3 still ~131s").

---

## Planning

Revision 3 — folds in every reviewer's Gate-1 input, @paired-auth-controller's additions, the Lead rulings and the user's local-timing decision (§Reviewer input disposition at the end).

### Mechanism restatement

Instance framing: "CI is slow". Mechanism: the CI Test Suite is one 4-vCPU runner doing three
independent cargo graphs (dev build + release for L1/L3, test profile for L4, clippy for L5) in
series, and it re-does work at three levels: (a) the same source is compiled and tested twice
(ac/gc `main.rs` re-declare the lib modules); (b) test binaries run one at a time; (c) artifacts
are rebuilt every run (the cache is never warm, dt-guard is built twice, sqlx-cli is compiled
every run).

Wider-than-task findings, completed here:
1. **Tool pins.** CI installs `cargo-audit` (ci.yml, audit-scheduled.yml) and `cargo-llvm-cov`
   unpinned, while the devloop image pins them (0.22.2 / 0.8.7). Adding nextest under "one pinned
   version, one source" is the same invariant, so all three tools move to it (Lead ruling 2).
2. **Stale `#[allow(dead_code)]`.** About 66 allows in ac/gc exist because the bin's private copy
   of the modules saw pub items as dead. They are removed (Change 1).

### Measured baseline (local, before)

The local box emulates the CI runner with `taskset -c 0-3` (4 CPUs), a cold `target/`, and
`CARGO_BUILD_JOBS=4`.

| Measure | Value |
|---|---|
| `layer-fast.sh` cold, 4 CPUs | **929 s**: L1 160, L2 4, L3 129, L4 568, L5 62, L6 6 |
| L4 breakdown | test-profile compile 126 s, Rust tests ~390 s (179 binaries in series), TS ~40 s |
| `cargo test --workspace` (already built), 4 CPUs | 395 s |
| `cargo nextest run`, 4 CPUs, 4 threads | 337 s |
| `cargo nextest run`, 4 CPUs, 12 threads | 210 s |
| same, `ci` profile, 3 consecutive runs (flake evidence, test F1) | **4908/4908 pass ×3**: 201.6 s / 196.4 s / 200.9 s, 0 retries (retries = 0) |
| `cargo nextest run`, 32 CPUs, default threads | 135 s (long pole: one 55 s mh test) |
| CI run 36923344871 (baseline) / last green 37171005734 | 29 min (cancelled) / 27.3 min: pipeline step 1408 s, sqlx-cli 53 s, guard build 67 s |

CI is about 1.5x slower than the 4-CPU emulation. With changes 1-3 and 5 but one job, L4 Rust
goes from ~390 s to ~170 s, which is ~690 s locally and **~17 min of CI cold**. Only running in
parallel across runners reaches ~10 min.

### Change 1: compile and test each service once

**Scope.** Only **ac-service** (10 `mod`s) and **gc-service** (12) re-declare their modules. mc,
mh, dt-guard, dt-story and media-vector-gen already use their lib; I checked every crate that has
both `lib.rs` and `main.rs`. mc and mh are **not touched**. devloop-helper has no lib.

**Edit.** In `main.rs`, `mod x;` becomes `use ac_service::{…}` / `use gc_service::{…}`. Every
module is already `pub mod` in lib.rs, so **zero visibility changes** are expected. The body of
`main()` is not moved into a lib `run()`; it stays in main.rs byte-for-byte. That keeps:
- OTel init, then subscriber, then `init_metrics_recorder`, then zero-init, in that order;
- the signal handling and drain;
- every `counter-zero-init` call site in main.rs as text (the guard reads main.rs literally);
- the redacting `impl Debug for Config` and the `%field` startup logs, unchanged.

Only the bin calls `init_metrics_recorder` or installs a subscriber. No lib code gains a path to
either. If any `pub(crate)` item turns out to be needed, the plan stops and lists it here; a
lib-level entry fn is preferred over widening a key-bearing type. Neither main.rs has a
`#[cfg(test)]` block. No test-utils or `#[cfg(test)]` item becomes reachable from a non-test
build. The mc/mh `compile_error!` release seams and `release-feature-gate.test.sh` are untouched,
and L1 re-proves them.

**Log-target change (AC only; @observability, corrected by @paired-auth-controller).** The AC bin
is `auth-controller`, so the module tree main.rs re-declared logged with `target: "auth_controller::…"`;
after the change it is `ac_service::…`. **Log volume does not rise**: every `debug!` in ac-service
sets an explicit target (`crypto`, `org_extraction`, `audit`), all `#[instrument]` spans are INFO,
and there is no `trace!`/`Level::DEBUG`. Only the JSON `target` field value changes; nothing in
infra/, dashboards, alerts, runbooks or scripts keys on `auth_controller`. GC's bin `gc-service`
maps to `gc_service`, so GC targets do not change. Recorded in the commit message. (No runbook
lever: three reviewers verified volume is unchanged, so a lever for a non-event would mislead.)

**Fallback filter (@paired-auth-controller A1).** main.rs's own events (startup, shutdown/drain)
keep target `auth_controller`, which the fallback `ac_service=debug,tower_http=debug` never
matched. It becomes `concat!(env!("CARGO_CRATE_NAME"), "=debug,ac_service=debug,tower_http=debug")`,
derived so it cannot drift. The bin is not renamed (Dockerfiles and manifests reference
`auth-controller`). This is the one intentional edit to main()'s body.

**Stale line citations (@paired-auth-controller A2).** Removing `crypto/mod.rs:401` shifts the
`verify_user_jwt` clock-skew site cited as `:439` in `tests/token_validation_integration.rs`
(~:12-15, :52), `src/observability/metrics.rs` (~:471, :710-711) and `docs/TODO.md:897`. Those
citations become symbol references ("the iat-skew branches of `crypto::verify_jwt` /
`verify_user_jwt`"). The clause saying `record_token_validation` "is `#[allow(dead_code)]`" is
dropped, because the sweep removes that allow. TODO.md:897 option (b) is marked done. TODO.md:899
is a closed historical entry and is left alone.

**Stale suppressions (@code-reviewer).** Each `#[allow(dead_code)]` in ac/gc is replaced by
`#[expect(dead_code)]`, then the crate is built. This includes the multi-line
`#[allow(\n dead_code, …)]` lists in `ac-service/src/models/mod.rs` (~206/213/291/297), which a
single-line grep misses:
- If the expect is unfulfilled (the item is pub in the lib, so the lint can't fire), the
  attribute is **deleted**.
- If it is fulfilled (a private item that really is dead), it stays as
  `#[expect(dead_code, reason = "…")]`.

No allows are left behind, so the cleanup is complete. Afterwards,
`grep -rn dead_code crates/{ac,gc}-service/src` shows only `expect`s with reasons. Gate 3 gets
the full before/after list of every removed or kept allow in ac-service (for
@paired-auth-controller). One allow is in GSA
`crates/ac-service/src/crypto/mod.rs:401` (`verify_user_jwt`). It is included so the cleanup is
not partial, and is classified GSA → Domain-judgment with auth-controller as owner and security
reviewing. **Approved** by @paired-auth-controller (a reachable pub item, a lint attribute only,
and its "used by GC and MC" comment was wrong anyway) and by @security (`#[instrument(skip_all)]`
stays).

**Regression guard: `dt-guard bin-lib-single-compile`** (machinery, infrastructure). For every
workspace member crate whose `Cargo.toml` has a lib and a bin target, the guard checks the bin
root. Crates are discovered from the root `[workspace] members` (via the existing
`common/cargo_manifest.rs` parse) plus each member's manifest, honouring explicit
`[lib]`/`[[bin]]` paths and the `src/lib.rs`/`src/main.rs` defaults. There is no hardcoded
service list and `common/services.rs` is not used, so the next crate that gains a lib is covered.
For each such crate, the guard fails if the bin's root
file declares a **file module** (`mod x;`, with any visibility or attributes) that the lib root
also declares.

Decisions, each with a unit test:
- An inline `mod x { … }` is not a file module, so it does not fire.
- `#[path = "…"] mod x;` and `mod x;` are treated alike: both fire if they resolve to the same
  file as a lib module (compared by resolved path, not name). The module doc says so.
- `pub(crate) mod`, `pub mod` and `#[cfg(…)] mod` fire (cfg does not exempt; a cfg-gated
  duplicate still double-compiles in that cfg).
- A main-only module that lib.rs does not declare does not fire.
- Vacuity: the real-tree run must discover at least one lib+bin crate, otherwise it reports
  `PRECONDITION_FAILURE` (`no-lib-bin-crates-found`).
- Adverse input: a fixture copy of 3651ebf7's ac main.rs and lib.rs fires.

Wrapper: `scripts/guards/simple/validate-bin-lib-single-compile.sh`.

**Per-binary test counts before** (`cargo nextest list --workspace --run-ignored all`; doctests
via `cargo test --doc --workspace -- --list`):
- 164 binaries, 4908 tests, 0 ignored. The 3 `cfg_attr(coverage, ignore)` tests are ignored only
  under `cfg(coverage)`; they are in the lib binary and unaffected.
- `ac-service` lib 402, `ac-service::bin/auth-controller` 402.
- `gc-service` lib 393, `gc-service::bin/gc-service` 393.
- Doctests: 40 listed (3 run: common/secret.rs ×2 and env-tests/eventual.rs no_run; 37 `ignore`).
- **Expected after:** 4908 − 402 − 393 = **4113** with both bin-unit binaries at 0 and every other
  binary unchanged, ignored counts included; doctests 40 → 40 in the new lane.

The full per-binary before/after diff goes in §Test Counts.

### Change 2: cargo-nextest for Layer 4 Rust tests

**Runner.** `scripts/lang/rust/test.sh` runs two `run_and_emit` lanes. Each lane maps a nonzero
exit to `STATUS=FAIL`; no output grep is involved.
1. `cargo-nextest`: `cargo nextest run --profile layer4 --show-progress none "${CARGO_LOCKED[@]}" "$@"`.
2. `cargo-doctest`: `cargo test --doc --no-fail-fast "${CARGO_LOCKED[@]}" <package-selection args from "$@">`.

Notes on the lanes:
- `--doc` cannot be combined with target selectors such as `--lib`, so the doctest lane gets only
  the package-selection subset of `"$@"`: `--workspace`/`--all`, `-p`/`--package`, `--exclude`
  and the feature flags. It never widens to workspace-wide when a package was named.
- Under `test.sh -p ac-service --lib`, the doctest lane still runs ac-service's doctests. That is
  a strict superset of what was asked for, never a drop.
- Production callers (layer4) pass no args.
- Args after `--` go to nextest (filters plus libtest-compatible flags). A `cargo test`-only
  form that nextest rejects (e.g. `-- --test-threads=1`) **fails loudly** through nextest's own
  usage error, which is exit 2, i.e. STATUS=FAIL. It is never dropped silently. `scripts/test.sh`'s
  usage comment `-- --test-threads=1` becomes `--test-threads 1` (nextest's flag). No production
  caller passes args (layer4 calls test.sh bare); the only arg shapes in use are in the usage
  comment and `behavior-equivalence.test.sh` (`--workspace`, `-p ac-service --lib`). Both are
  valid for nextest, and their package subset is valid for `--doc`.

No fallback. If `cargo-nextest` is missing, the run fails before any test with
`STATUS=FAIL REASON=cargo-nextest-missing` and the hint `infra/devloop/devloop.sh --rebuild`. If
its version differs from the pin, it fails with `CARGO_NEXTEST_VERSION_MISMATCH`. Both mirror
the sqlx-cli check.

Exit 4 (no tests run) stays loud: `no-tests` is left at its default, which is "fail". Layer 7
env-tests stay on `cargo test`, because `#[serial]` is an in-process lock and needs one process
against the live cluster. In Layer 4 `--workspace`, env-tests compiles without features to its
unit tests only (142 today, unchanged).

**Config: `.config/nextest.toml`**, one profile `layer4` that test.sh always selects (this is
the "ci" profile the Lead's user input asked for, named for its one consumer and used identically
in CI and locally), so CI and
local run identical settings. Every key below was checked against the 0.9.146 `--help` and the
config parser by actually running it (3 runs above):

| Key | Value | Why |
|---|---|---|
| `fail-fast` | `false` | same as today's `--no-fail-fast`; one red run reports every failure |
| `retries` | `0` | ADR-0028 zero-retry; a flake reds, never retries to green |
| `test-threads` | `12` | slow binaries are DB/sleep-bound: 337 s → ~200 s on 4 CPUs, 3/3 clean runs; one value everywhere (local 32-CPU runs are slightly slower than default, which is accepted for identical settings) |
| `slow-timeout` | `{ period = "60s", terminate-after = 5 }` | a hung test is named at 60 s and killed at 5 min (the slowest test is 55 s), leaving margin under the shard timeout so the kill names the test (@operations) |
| `status-level` | `"slow"` | print failures and SLOW warnings live; no per-pass lines |
| `final-status-level` | `"slow"` | the summary lists failures plus slow tests |
| `failure-output` | `"immediate-final"` | failing output shown live and repeated in the final summary |
| `success-output` | `"never"` | quiet success |

`--show-progress none` is the only CLI flag. Progress display is not a repo-profile key (it is
user config, CLI or env), so it is passed explicitly rather than relying on TTY detection.
`--hide-progress-bar` is deprecated in 0.9.146. JUnit output was considered and not adopted:
nothing consumes it, and the STATUS lanes and job logs already carry the result.

**Isolation.** Process-per-test is strictly stronger for in-process state:
- The env-mutating tests (gc `assignment_cleanup`; env-tests `cluster.rs` `#[serial]`) and the
  per-process `OnceLock` metrics recorders and MetricAssertion rigs become isolated instead of
  shared.
- No test relies on state carried between tests in one binary. A test that did would have shown
  up as a failure in the 4 full runs at 12 threads plus the runs at 4 and 32 threads.
- `#[sqlx::test]` (61 files) gets a per-test DB, and sqlx 0.9 drops its own DB by name (test F4).
- No test binds a fixed port. `AC_BIND_ADDRESS` and the `:808x` literals are config values that
  are only parsed. The only binds are to `:0`.
- No fixed `/tmp` paths are written.

**Postgres headroom.** max_connections is 100 (both the docker-compose test DB and the CI
`postgres:16` default). Per process, sqlx uses a lazy master pool plus a test pool of at most 5.
At 12 processes that is about 12 × 6 = 72 at worst; the measured peak `pg_stat_activity` during
the runs is recorded in §Implementation.

**Pin single source.** A new data file, `infra/cargo-tools.versions`, holds `<tool> <X.Y.Z>`
lines for cargo-nextest 0.9.146, cargo-audit 0.22.2, cargo-llvm-cov 0.8.7 and cargo-fuzz 0.13.2.
0.8.7 is kept so the coverage comments' "0.8.x" claim stays true. The file header states the
rule's scope: every cargo tool that any workflow or the devloop image installs. Its one reader is
`infra/lib/cargo-tools.sh:cargo_tool_version`, which validates the exact X.Y.Z shape and fails on
an unknown tool. Consumers:
- `devloop.sh`: one `image_build_args` function feeds both `podman build` sites. Today they are
  two copies of the same arg list.
- Dockerfile: `ARG CARGO_NEXTEST_VERSION` etc., with **no defaults** (fail loud). It installs
  with `cargo install cargo-nextest --locked --version "=${…}"`, like its neighbours: no
  installer script and no `latest`.
- ci.yml, audit-scheduled.yml (cargo-audit) and fuzz-nightly.yml (cargo-fuzz, now `cargo install
  --locked --version =`): a step sources the reader and writes the versions to
  `$GITHUB_OUTPUT`, and the SHA-pinned `taiki-e/install-action` (checksum-verified) gets
  `tool: cargo-nextest@<v>` etc. I verified the pinned action SHA ships manifests for all three at
  these versions.
- `test.sh` drift check.
- `setup.test.sh` toolchain block: a no-literal check over **every** `.github/workflows/*.yml` and
  the Dockerfile. It fails on any `tool: cargo-…` or `cargo install <tool>` that is unpinned or
  carries a literal version, except sqlx-cli, whose pin comes from the Cargo.lock reader. A
  derivation check is added too.
- sqlx-cli's install flags (`--locked --no-default-features --features postgres`, 5 sites) are not
  folded into a helper. Both Dockerfiles' build contexts cannot see `infra/lib`, so a helper would
  cover only the script sites. @dry-reviewer files it as an extraction TODO (Lead ruling 4).

Self-tests:
- `behavior-equivalence.test.sh`: the needle is derived from test.sh's argv, not written from
  memory.
- New cases: nextest-absent, nextest-version-mismatch (expected version via the reader), the
  doctest lane receiving the same package args, and `-p … --lib` stripped of `--lib` for the
  doctest lane. Each failure case asserts STATUS=FAIL and that no test ran.
- `layer7.test.sh:1694`: the no-fail-fast pin moves to the new L4 form (`fail-fast = false` in
  the profile plus the doctest `--no-fail-fast`).

**Host action (remaining, not in-tree; blast radius @operations).** After merge, **every** devloop
and run-story container on an old image reds Layer 4 with `cargo-nextest-missing` (or
`CARGO_NEXTEST_VERSION_MISMATCH`) until the host runs `infra/devloop/devloop.sh --rebuild`. The
same happens whenever `infra/cargo-tools.versions` changes. The Lead's Gate-2 container is the
first (the Lead is handling that with the user). The devloop-validation.md symptom catalogue gets
a row for both tokens → `devloop.sh --rebuild`.

### Change 3: Rust cache

**Root cause of "No cache found."** GitHub evicts a cache that has not been used for 7 days. main's
last push before run 36923344871 was 2026-09-17, 14 days earlier. The PR-ref cache was never saved
because every run on that PR was red or cancelled, and rust-cache saves on success only.

**Change.**
- `cache-on-failure: true` on every rust-cache step, so a red PR run warms its own ref for the next
  push (Lead ruling 1: failing runs save).
- PR runs keep saving. GitHub scopes a `pull_request` run's cache to that PR's ref, and main never
  reads it. main's cache, saved on push, is readable by every PR targeting main; the rust-cache
  README says "caches from base branches are available to PRs".
- Real PRs and Dependabot target `main` (the clone's local default branch is not GitHub's), so
  main is the ref that warms the cache. Merges to main save it.
- A comment next to each cache step states the security premise: this holds only while no
  workflow uses `pull_request_target` or `workflow_run`.

**Distinct keys, no save races.** The matrix uses `shared-key: ci-pipeline-${{ matrix.shard }}`,
so each shard saves its own key. Coverage keeps `coverage-cache` and ci-client keeps its own.

**10 GB cap.** rust-cache strips workspace-member artifacts and keeps dependencies, so five Rust
caches fit. A note in the runbook covers what to do if eviction thrash appears.

**Fresh guard binaries.** rust-cache strips workspace crates, so `compile.sh` rebuilds dt-guard
and dt-story from this checkout on every run; a cached binary is never trusted.

No cron cache-warmer: with the split, a cold run is already at target, and a warmer would be new
mechanism for a convenience.

### Change 4: split the job and keep the Gate-2 backstop

One job cannot reach ~10 min (projection above), so the pipeline is split by cargo graph:

| Shard (matrix `shard`) | `layers` | Tools installed (keyed off layer membership, not shard name) |
|---|---|---|
| `guards` | `1,2,3,6,7` | nightly + kubectl assert (L3), cargo-audit (L6). L3 consumes L1's release dt-guard/dt-story, so they share a job and compile.sh stays the producer (O3) |
| `test` | `4` | postgres service, DB env, sqlx-cli, nextest, Playwright |
| `lint` | `5` | none beyond the common toolchain + pnpm |

Projected CI cold: guards ~8 min, test ~9 min, lint ~5 min, so wall-clock is about 9-10 min.

**One orchestrator, partition in one place, drift fails loudly.**
- Each shard runs `./scripts/layer-all.sh --layers <list>` from a fresh checkout and never reads
  a verdict. It does not call `layerN.sh` directly, so the precondition, sentinel-leak,
  merge-base and STATUS logic all still run through layer-all.sh.
- The **only** place the partition is written is the ci.yml matrix.
- A final job named **`Test Suite`** keeps the required-check context, so no admin action is
  needed and the backstop cannot fail open (O1, Lead ruling 4).
  - It has `needs: [pipeline]` and `if: always()`.
  - Its **first step** fails unless `needs.pipeline.result == 'success'`, so skipped, cancelled
    and failure all go red. It does not use `!contains(failure)`.
  - Only then does it download this run's artifacts (`actions/download-artifact` with no `run-id`
    or `github-token`, so only this workflow run) and run `./scripts/layer-all.sh --aggregate <dir>`.

**`layer-all.sh --layers L,…`** (shard mode):
- It is accepted only under `GITHUB_ACTIONS`. A comment states this is **convenience gating, not
  a security control**: the variable is forgeable locally, and that is acceptable only because a
  `--layers` run never writes, removes or overwrites a Gate-2 verdict, since the EXIT trap is not
  installed. Local users keep `--max-layer`, which stays refused in CI.
- It validates the list: each entry is an integer in 1..`LAYER_MAX`, in ascending order, with no
  duplicates.
- It runs those layers with the existing loop, fail-fast semantics and per-layer budget warns.
  The 3+6 fast-tier budget is computed only when both ran in this invocation; otherwise it emits
  nothing (it is not a NOT-RUN case).
- It prints the usual LAYER_SUMMARY block and table, so per-layer timings stay in each job's log.
- It writes `${DEVLOOP_TMP}/layer-summary.txt` with one line per layer run:
  `LAYER=<n> RESULT=<status> DURATION=<s> RC=<rc>`. The upload step is `if: always()`, so a red
  shard's summary still reaches the aggregator, which then reports the real FAIL rather than a
  generic missing-layer PRECONDITION. Both checks stay: the `needs` result gate and
  exactly-once.

**`layer-all.sh --aggregate DIR`** runs no layers.
- It reads `DIR/*/layer-summary.txt`.
- Each line must match the anchored regex
  `^LAYER=[1-9][0-9]* RESULT=(<the known status enum, built from _common.sh's rank table>) DURATION=[0-9]+ RC=[0-9]+$`.
  The file is never sourced, eval'd or declared.
- It requires every layer 1..`LAYER_MAX` to appear **exactly once**. Any of the following is
  `PRECONDITION_FAILURE REASON=aggregate-…` (exit 2):
  - a missing layer
  - a duplicate layer
  - a malformed line
  - an unknown RESULT token
  - zero summary files
  - an unreadable directory
- It then computes TOTAL_RESULT with `aggregate_worst_status`, maps the exit with
  `status_to_exit_code`, and applies the observed-RC floor (max of the mapped exit and every RC).
- It prints LAYER_SUMMARY, the table and FAILURE_TRIAGE lines naming the failing layer and its
  shard.

**`LAYER_MAX=7`** is hoisted into one constant in `scripts/lang/_common.sh`. It is used by:
- `--max-layer` validation, whose regex `^[1-7]$` and error text become a range check over
  `LAYER_MAX`;
- `--layers` validation;
- `--aggregate` completeness;
- `layer-fast.sh`, whose literal `--max-layer 6` and "layers 1-6" text become `LAYER_MAX - 1`
  ("every layer except the last, cluster, layer").

verify-completion.sh encodes no range (checked).

Layer 7 in the `guards` shard emits a real `STATUS=SKIPPED-NO-CLUSTER REASON=no-cluster-ci`
(exit 0), and its summary line records exactly that. An absent line would be "missing", which
fails.

**New `layer-all.test.sh` cases.**
- `--layers`:
  - refused without GITHUB_ACTIONS
  - malformed, out-of-range and duplicate lists refused
  - writes the summary
  - writes no verdict and leaves a pre-existing verdict file byte-identical
- `--aggregate`:
  - all OK → exit 0
  - worst status wins
  - `RESULT=OK` with `RC=1` → exit 1 (RC floor)
  - SKIPPED-NO-CLUSTER ranks below OK
  - missing layer, duplicate layer, malformed line, unknown RESULT, empty dir → each exit 2 with
    its REASON token

**Comments moved with the backstop (semantic-guard 4, code-reviewer 5d).**
- The ci.yml "Gate-2 forgery backstop (task #51)" block moves to the shard step and is restated ("each shard runs its layers from
  scratch and reads no verdict"). The aggregator carries a short note pointing back to it ("reads only this run's shard
  summaries; never a verdict or another run's artifact").
- `_gate2_binding.sh`'s THREAT MODEL text, ADR-0033 (amendment note under §4/CI) and
  `devloop-validation.md` (backstop paragraphs ~1019/1073/1151) say "sharded `layer-all.sh` +
  `--aggregate`".
- Nothing in CI parses layer-all's summary today; the aggregator becomes the one parser.

**Matrix tool provisioning (@dry-reviewer).** Each matrix entry carries explicit boolean flags
(`needs_postgres`, `needs_sqlx`, `needs_nextest`, `needs_playwright`, `needs_nightly`,
`needs_audit`), and steps key off those. There are no `contains(matrix.layers, …)` conditions,
which would re-encode layer contents in YAML. A flag missing for a moved layer fails loudly
(tool-not-found PRECONDITION), never silently. The postgres `services:` block is attached to every
shard: GitHub cannot make `services:` conditional per matrix entry, and it costs ~20 s.

**Permissions and checkout.** Every new job inherits the workflow-level `permissions: contents:
read`, and every checkout keeps `persist-credentials: false`. The guards shard keeps
`fetch-depth: 0`; the others need it too, because layer-all's merge-base precondition runs in
every shard. The `on:` triggers are unchanged.

**Runbook (O1, O9).** `devloop-validation.md` gets:
- a **CI job map**: job → layers → local repro (`./scripts/layerN.sh` / `layer-fast.sh`) → tools;
- the aggregator's failure tokens and how they map to "open which shard job";
- an update to the required-check section: `Test Suite` is the aggregate, and the shard contexts
  need not be required;
- **Rolling back** (per concern; Lead overruled a commit split, @operations):
  - thin main: revert main.rs, the allow sweep **and** the bin-lib guard together (the guard reds
    without the rest);
  - nextest and pins: revert test.sh, `.config/nextest.toml` and the pins, then run
    `devloop.sh --rebuild`;
  - CI split: revert ci.yml alone (`--layers`/`--aggregate` are additive). Never revert
    layer-all.sh without ci.yml.
- a Dependabot rust-toolchain PR line: review it like 3651ebf7 (the 1.99 bump); after merge the
  host runs `devloop.sh --rebuild`.

**New actions** (SHA-pinned with `# vX.Y.Z`, tracked by Dependabot's github-actions block):
`actions/upload-artifact` (unique name per shard: `layer-summary-${{ matrix.shard }}`),
`actions/download-artifact` and `actions/cache` (for sqlx-cli).

**Code Coverage** stays a separate parallel job and is not folded into L4 (task constraint). It
stays on plain `cargo test --workspace --locked` under the llvm-cov env; nextest's per-process
profraws would mean ~4900 files. It gets the pinned cargo-llvm-cov and `cache-on-failure`. Its
cfg=coverage assert lines and RUSTFLAGS comment are untouched. It will be the longest job (~13-15
min), but it is advisory and not `Test Suite`, so a TODO entry is filed.

### Change 5: remove repeated setup (four dt-guard sites, Lead ruling 3)

The rule, applied to every site: a job that runs Layer 1 gets the guard binaries from
`compile.sh`, and a job that runs guards without Layer 1 builds them itself, as the only producer
in that job.

| Site | Disposition |
|---|---|
| `scripts/lang/rust/compile.sh` | the producer for every pipeline job (always-run, L1) |
| `ci.yml` "Build guard binaries" | **deleted**: the guards shard runs L1 before L3 |
| `ci-client.yml:100` | **kept**, with a comment: its Lint job runs ts guards directly, with no layer-all/compile.sh, so this is the only producer in that job, not a duplicate. The TODO entry counts it honestly: a 4th site that legitimately stays |
| `scripts/workflow/preflight-story.sh` | assertion unchanged (story runner) |

A missing binary still fails loudly: `_dt_guard_wrapper.sh` exits 1 and
`validate-story-manifest.sh` reports a PRECONDITION; neither skips. The TODO "Guard binaries…"
entry is updated to this map: CI no longer has a second build site, and Half B stays live. The
`release_build_profile.rs` module doc's scope bullet becomes "ci-client.yml".

**sqlx-cli** (test shard only):
- `actions/cache` of `~/.cargo/bin/{sqlx,cargo-sqlx}`, keyed
  `sqlx-cli-${{ runner.os }}-<rustc version>-<Cargo.lock sqlx version>`. Both values come from
  their existing single sources (`rustc --version` after `rustup toolchain install`, and
  `cargo_lock_version`).
- `cargo install` runs only on a cache miss. There is no binstall or prebuilt download.
- test.sh's existing version-equality check stays the authority, so a stale cached binary fails
  loudly.

### Change 6: Rust release notification

Dependabot supports `package-ecosystem: "rust-toolchain"` (GitHub changelog 2025-08-19). It
updates `channel = "1.xx.yy"` in `rust-toolchain.toml`, and I verified this with WebFetch rather
than assuming it. A weekly block at `/` is added. This adds no workflow and no permissions,
because Dependabot runs with its own scoped token. Dedupe comes from Dependabot's one-PR-per-update
behaviour, and failures show on the repo's Dependabot page.

Its PRs keep the `X.Y.Z` shape that `infra/lib/rust-toolchain.sh` requires, and the images take
`RUST_VERSION` from that file. workspace-deps R6 inspects cargo blocks only, so it is unaffected.

Runbook line: see Change 4 §Runbook. A cold cache is expected on that PR, because the rustc key
changes. Dependabot PRs targeting the default branch, which CI does not trigger on, is a
pre-existing issue that affects every block; it is out of scope (Lead ruling 3). No
`target-branch` is added.

### Timing evidence and timeouts (user decision: measured locally)

Implementation measures each shard **exactly as the ci.yml matrix runs it**
(`GITHUB_ACTIONS=true ./scripts/layer-all.sh --layers …`), one shard at a time, on the 4-CPU
emulation (`taskset -c 0-3`, `CARGO_BUILD_JOBS=4`), plus the coverage job's command sequence:
- **cold**: empty `target/`, sqlx-cli not cached, no nextest/tool binaries beyond the pinned ones
  the image provides;
- **warm**: `target/` and the cargo registry populated from the previous run of the same shard
  (approximating a rust-cache restore; rust-cache keeps dependency artifacts, so warm is
  re-measured after `cargo clean -p` of the workspace members to match what it strips).

§CI Timings records, per shard and for coverage: cold s, warm s, `target/` size (standing in for
the rust-cache size, @operations ask 4) and the projected CI wall-clock.

**Local → CI factor.** From run 37171005734, the last green run: pipeline step 1408 s against the
same pipeline on the local 4-CPU cold emulation, layer-fast 929 s plus the then-separate 67 s guard
build, which is ~1.4x. The setup steps the local run has no counterpart for (~2.5 min: containers,
toolchain, pnpm, Playwright) are added per shard. The factor is restated next to the numbers. The
caveat that it is derived from a single green run is noted.

**Timeouts.** Each shard gets `timeout-minutes` = projected CI cold × 2, rounded up to 5 min; the
current estimate is 20. The ci.yml comment cites the §CI Timings measurements, not a run ID. The
`Test Suite` aggregator gets 5, and coverage stays at 40 with its measured duration recorded
(Lead ruling 3). The user pushes and checks CI after the commit.

### Rollback (O8)

One devloop commit carries the Gate-2 verdict (Lead overruled a split), so rollback is per concern
through the runbook note in Change 4 §Runbook. Startup, config and shutdown behaviour is
byte-equivalent apart from the documented AC log-target and fallback-filter change.

### Reviewer input disposition

| Input | Where handled |
|---|---|
| observability 1 (AC log target) | Change 1, "Runtime behaviour change", and the Cross-Boundary row |
| observability 2-3 (zero-init calls, init order) | Change 1: body stays in main.rs |
| observability 5-6, user input (nextest output, profile) | Change 2 config table; LAYER timings kept per job |
| security 1 (no widening, release seams) | Change 1 |
| security 2 / ops O1-O2 / code-reviewer 5 / dry 5 / semantic 4 (backstop) | Change 4 |
| security follow-ups 1-7 | strict regex, same-run artifacts, verdict untouched, cache premise comments, cargo install, crypto allow, permissions: Changes 1, 3, 4, 2 |
| security 6 (nextest supply chain, doctests, zero tests) | Change 2 |
| dry 2 / Lead 2 (tool pins incl. audit-scheduled.yml) | Change 2, "Pin single source" |
| dry 4 / ops O3 / Lead 3 (four dt-guard sites) | Change 5 table |
| code-reviewer 2 (`expect` sweep) | Change 1 |
| code-reviewer 7 / ops O4 / Lead 1 (cache keys, saves) | Change 3 |
| test 1-7, follow-ups F1-F6 | counts (Change 1), flake runs + PG headroom + args parity + pin tests (Change 2), guard tests (Change 1), aggregate RC-floor case + L7 STATUS (Change 4) |
| ops O5d (fixed ports / shared DB) | Change 2 isolation: no fixed binds; sqlx per-test DB; no test-groups needed |
| ops O6-O9 | Timeouts, Change 6 runbook line, Rollback, runbook job map |
| semantic-guard 2 / note b (redacting Debug, llvm-cov 0.8.7) | Change 1, Change 2 pins |
| code-reviewer follow-ups 1-6 | `_gate2_binding.sh` + ci.yml block (Change 4), upload `if: always()`, `expect` incl. multi-line lists, GSA allow included, guard vacuity + `#[path]`, args mapping (Change 2) |
| operations A2-A5 + rev-2 asks | terminate-after 5, runbook rollback per concern, post-merge blast radius + symptom row, rust-toolchain PR line, per-shard size + local timings |
| dry D1-D5 | fuzz-nightly + audit-scheduled pins and the all-workflow no-literal check; honest 4-site count; workspace-derived discovery; `LAYER_MAX` in `_common.sh` + layer-fast; README toolchain literal; matrix boolean flags; sqlx flags → DRY TODO |
| paired-auth-controller A1-A2 | Change 1 fallback filter; citations become symbols; TODO.md:897 updated |
| user: local timings | §Timing evidence and timeouts |

## Cross-Boundary Classification

One row per path or glob (the scope guard reads the first cell as one path).

| File / glob | Category | Owner | Why |
|-------------|----------|-------|-----|
| `crates/ac-service/src/main.rs` | Minor-judgment | auth-controller | `mod` → `use ac_service::…`; fallback filter adds `CARGO_CRATE_NAME`; **runtime change**: JSON log `target` `auth_controller::…` → `ac_service::…` (volume unchanged) |
| `crates/ac-service/src/crypto/mod.rs` | GSA → Domain-judgment | auth-controller | stale `allow(dead_code)` + wrong comment on `verify_user_jwt` removed (lint attribute only); the two decode-failure `debug!` messages reworded `token`→`JWT` (message text only; the diff-scoped no-secrets-in-logs Check 4 fired on prose — Gate-3 security/auth-controller F1 replaced an interim `guard:ignore`); approved by @paired-auth-controller; security reviews |
| `crates/ac-service/src/middleware/**` | Mechanical | auth-controller | stale `allow(dead_code)` deleted (compiler-verified) |
| `crates/ac-service/src/models/**` | Mechanical | auth-controller | stale `allow`/`expect(dead_code)` incl. multi-line lists deleted (compiler-verified) |
| `crates/ac-service/src/observability/**` | Mechanical | auth-controller | stale `allow(dead_code)` deleted; metrics.rs line citations → symbols |
| `crates/ac-service/src/repositories/**` | Mechanical | auth-controller | stale `allow(dead_code)` deleted (compiler-verified) |
| `crates/ac-service/src/services/**` | Mechanical | auth-controller | stale `allow(dead_code)` deleted (compiler-verified) |
| `crates/ac-service/tests/token_validation_integration.rs` | Mechanical | auth-controller | stale `:439` / "is `#[allow(dead_code)]`" comment citations → symbols |
| `crates/gc-service/src/main.rs` | Minor-judgment | global-controller | `mod` → `use gc_service::…`; no behaviour change (bin name maps to the same target) |
| `crates/gc-service/src/**` | Mechanical | global-controller | stale `allow(dead_code)` deleted (compiler-verified); one kept as `expect(dead_code, reason)` (`AtomicAssignResult` field); stale "main.rs re-declares" comments removed |
| `crates/dt-guard/src/bin_lib_single_compile.rs` | Mine | — | new guard (machinery); lexes via the shared `blank_file` |
| `crates/dt-guard/src/common/test_code_filter.rs` | Mine | — | `blank_file` moved here (whole-file entry point of the shared lexer; DRY-1) |
| `crates/dt-guard/src/media_telemetry_deny.rs` | Mine | — | imports `blank_file` from its new home (no behaviour change) |
| `crates/dt-guard/src/lib.rs` | Mine | — | register module |
| `crates/dt-guard/src/main.rs` | Mine | — | register subcommand |
| `crates/dt-guard/src/release_build_profile.rs` | Mine | — | module-doc scope bullet; stale `ci.yml:170` citation |
| `scripts/guards/simple/validate-bin-lib-single-compile.sh` | Mine | — | wrapper |
| `scripts/lang/rust/test.sh` | Mine | — | nextest + doctest lanes, pin check, lane verdict |
| `scripts/test.sh` | Mine | — | usage comment |
| `scripts/lang/rust/behavior-equivalence.test.sh` | Mine | — | exact-lane argv; nextest pin + red-lane cases |
| `scripts/lang/rust/fixtures/cargo-shim` | Mine | — | `CARGO_SHIM_FAIL_ON` |
| `scripts/layer7.test.sh` | Mine | — | L4 no-fail-fast pins (nextest profile + doctest flag) |
| `scripts/layer-all.sh` | Mine | — | `--layers` (CI only), `--aggregate`, `LAYER_MAX` |
| `scripts/layer-all.test.sh` | Mine | — | (G) shard + (H) aggregate cases |
| `scripts/layer-fast.sh` | Mine | — | range derived from `LAYER_MAX` |
| `scripts/lang/_common.sh` | Mine | — | `LAYER_MAX` |
| `scripts/lang/_gate2_binding.sh` | Mine | — | THREAT MODEL comment text only |
| `scripts/workflow/run-story.test.sh` | Mine | — | three `o1` cases unset an ambient `DEVLOOP_TMP` (non-hermetic; surfaced by the timing runs) |
| `scripts/setup.test.sh` | Mine | — | one devloop build site; tool-pin reader + no-literal/unpinned checks |
| `.config/nextest.toml` | Mine | — | runner policy |
| `infra/cargo-tools.versions` | Mine | — | tool-pin source |
| `infra/lib/cargo-tools.sh` | Mine | — | the one reader |
| `infra/devloop/Dockerfile` | Mine | — | nextest install; tool ARGs without defaults |
| `infra/devloop/devloop.sh` | Mine | — | one `build_devloop_image`; tool pins via the reader |
| `infra/docker/ac-service/README.md` | Mine | — | stale `rust:1.83-slim` literal |
| `infra/kind/scripts/deploy.sh` | Mine | — | `CARGO_CHEF_VERSION` read from `infra/cargo-tools.versions` (DRY-2) |
| `.github/workflows/ci.yml` | Mine | — | shards + `Test Suite` aggregate, caches, pins, timeouts |
| `.github/workflows/ci-client.yml` | Mine | — | why its dt-guard build stays; `cache-on-failure` |
| `.github/workflows/audit-scheduled.yml` | Mine | — | cargo-audit pinned from the source |
| `.github/workflows/fuzz-nightly.yml` | Mine | — | cargo-fuzz pinned from the source, `--locked` |
| `.github/dependabot.yml` | Mine | — | rust-toolchain block |
| `docs/decisions/adr-0033-polyglot-validation-pipeline.md` | Minor-judgment | infrastructure | CI sharding amendment note |
| `docs/runbooks/devloop-validation.md` | Minor-judgment | operations | §8.7 job map/triage/rollback/pins/cache; L4 rows; catalogue rows; backstop + required-check text |
| `docs/runbooks/ac-service-deployment.md` | Minor-judgment | operations | log-target note (lib events `ac_service::…`, bin events `auth_controller`, fallback filter; observability Gate-3 F1) |
| `docs/specialist-knowledge/dry-reviewer/INDEX.md` | Mine (review-only) | — | DRY reviewer navigation update (blank_file kernel, tool-pin SSoTs) — edited by @dry-reviewer |
| `docs/TODO.md` | Mine | — | close CI-speed entry; guard-binaries entry update; coverage-job entry; :897 symbols |

## Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Semantic Guard | confirmed |
| Paired Auth Controller | confirmed (GSA crypto/mod.rs:401 approved) |

## Implementation Summary

**Change 1: compile and test each service once.**
- `crates/{ac,gc}-service/src/main.rs` import from their lib (`use ac_service::…` / `use gc_service::…`). There are zero visibility changes, and `main()`'s body is unchanged.
- One deliberate exception: AC's fallback `EnvFilter` gains `env!("CARGO_CRATE_NAME")=debug`, because the `auth-controller` bin's own events carry target `auth_controller`.
- New guard `dt-guard bin-lib-single-compile`, with wrapper `validate-bin-lib-single-compile.sh`. On the tree it reports `STATUS=OK … clean-crates-7`.
- Adverse-input check: run against an extract of 3651ebf7, the guard fires **22** violations (ac 10 + gc 12).
- 13 unit tests cover:
  - the positive-control fixture;
  - inline mod, nested mod, `#[path]`, visibility and cfg forms;
  - comment and literal masking;
  - declared `[[bin]]` and `src/bin`;
  - vacuity (`bin_lib_no_lib_bin_crates_found`);
  - glob members and a missing manifest;
  - the real tree (≥4 crates).

**Stale `dead_code` suppressions.** All 70 sites in ac/gc were converted to `#[expect(dead_code)]` and built (lib, lib-test, bin; clippy `--all-targets`):
- 69 were unfulfilled, so they were deleted.
- 1 was fulfilled and kept as `#[expect(dead_code, reason = "decoded by query_as from the RETURNING row; only Some/None is read")]` on `gc-service` `AtomicAssignResult::meeting_controller_id`, the one private dead field.
- Two comments that existed only to explain the bin double-compile were removed (`gc errors.rs`, `gc media_handlers.rs`).
- `grep dead_code crates/{ac,gc}-service/src` now shows only that one `expect`.
- AC line citations became symbols: `tests/token_validation_integration.rs`, `observability/metrics.rs`, `docs/TODO.md` :897.

Touching `crypto/mod.rs` brought it into the diff-scoped `no-secrets-in-logs` guard, whose Check 4 matched the word "token" in the `verify_user_jwt` decode-failure message ("User token verification failed"; the sibling "Token verification failed" passed on capitalisation alone). At Gate 3 (security F1, auth-controller F1) the interim `guard:ignore` was replaced by rewording both messages to "User JWT verification failed" / "JWT verification failed" (message text only; `target: "crypto"` is not enabled in any deployed filter). The guard's position imprecision is recorded on the existing docs/TODO.md §Guard Precision entry.

Removed or kept `dead_code` attributes in ac-service (for @paired-auth-controller; line numbers are pre-change). All of these were removed; none were kept:

- `crates/ac-service/src/crypto/mod.rs:401` — #[allow(dead_code)] // Library function - will be used by GC and MC for token validation
- `crates/ac-service/src/middleware/org_extraction.rs:21` — #[allow(dead_code)] // Library type - subdomain used for logging/tracing
- `crates/ac-service/src/models/mod.rs:206` — multi-line expect block (reason = "generated for every string_enum; not every target uses it")
- `crates/ac-service/src/models/mod.rs:213` — multi-line expect block (reason = "generated for every string_enum; not every target uses it")
- `crates/ac-service/src/models/mod.rs:291` — multi-line expect block (reason = "allowed by the CHECK; no emitter yet (Phase 4). Dead only in)
- `crates/ac-service/src/models/mod.rs:297` — multi-line expect block (reason = "allowed by the CHECK; no emitter yet (Phase 4). Dead only in)
- `crates/ac-service/src/models/mod.rs:34` — #[allow(dead_code)] // Will be used in Phase 4 admin endpoints
- `crates/ac-service/src/models/mod.rs:38` — #[allow(dead_code)] // Will be used in Phase 4 admin endpoints
- `crates/ac-service/src/models/mod.rs:42` — #[allow(dead_code)] // Will be used in Phase 4 admin endpoints
- `crates/ac-service/src/models/mod.rs:44` — #[allow(dead_code)] // Will be used in Phase 4 admin endpoints
- `crates/ac-service/src/models/mod.rs:56` — #[allow(dead_code)] // Will be used in Phase 4 admin endpoints
- `crates/ac-service/src/models/mod.rs:58` — #[allow(dead_code)] // Will be used in Phase 4 admin endpoints
- `crates/ac-service/src/models/mod.rs:60` — #[allow(dead_code)] // Will be used in Phase 4 admin endpoints
- `crates/ac-service/src/models/mod.rs:62` — #[allow(dead_code)] // Will be used in Phase 4 admin endpoints
- `crates/ac-service/src/models/mod.rs:64` — #[allow(dead_code)] // Will be used in Phase 4 admin endpoints
- `crates/ac-service/src/models/mod.rs:66` — #[allow(dead_code)] // Will be used in Phase 4 admin endpoints
- `crates/ac-service/src/models/mod.rs:68` — #[allow(dead_code)] // Will be used in Phase 4 admin endpoints
- `crates/ac-service/src/models/mod.rs:75` — #[allow(dead_code)] // Will be used in Phase 4 audit endpoints
- `crates/ac-service/src/models/mod.rs:77` — #[allow(dead_code)] // Will be used in Phase 4 audit endpoints
- `crates/ac-service/src/models/mod.rs:79` — #[allow(dead_code)] // Will be used in Phase 4 user auth
- `crates/ac-service/src/models/mod.rs:81` — #[allow(dead_code)] // Will be used in Phase 4 audit endpoints
- `crates/ac-service/src/models/mod.rs:83` — #[allow(dead_code)] // Will be used in Phase 4 audit endpoints
- `crates/ac-service/src/models/mod.rs:85` — #[allow(dead_code)] // Will be used in Phase 4 audit endpoints
- `crates/ac-service/src/models/mod.rs:87` — #[allow(dead_code)] // Will be used in Phase 4 audit endpoints
- `crates/ac-service/src/models/mod.rs:89` — #[allow(dead_code)] // Will be used in Phase 4 audit endpoints
- `crates/ac-service/src/models/mod.rs:91` — #[allow(dead_code)] // Will be used in Phase 4 audit endpoints
- `crates/ac-service/src/models/mod.rs:93` — #[allow(dead_code)] // Will be used in Phase 4 audit endpoints
- `crates/ac-service/src/observability/metrics.rs:126` — #[allow(dead_code)] // Will be used in Phase 4 token validation endpoints
- `crates/ac-service/src/observability/mod.rs:102` — #[allow(dead_code)]
- `crates/ac-service/src/observability/mod.rs:76` — #[allow(dead_code)]
- `crates/ac-service/src/repositories/auth_events.rs:108` — #[allow(dead_code)] // Library function - will be used in Phase 4 rate limiting
- `crates/ac-service/src/repositories/auth_events.rs:136` — #[allow(dead_code)] // Library function - will be used in Phase 4 analytics/monitoring
- `crates/ac-service/src/repositories/auth_events.rs:54` — #[allow(dead_code)] // Library function - will be used in Phase 4 audit endpoints
- `crates/ac-service/src/repositories/auth_events.rs:81` — #[allow(dead_code)] // Library function - will be used in Phase 4 audit endpoints
- `crates/ac-service/src/repositories/organizations.rs:15` — #[allow(dead_code)] // Library type - fields read in future phases
- `crates/ac-service/src/repositories/organizations.rs:59` — #[allow(dead_code)] // Library function - will be used in future phases
- `crates/ac-service/src/repositories/service_credentials.rs:137` — #[allow(dead_code)] // Library function - will be used in Phase 4 admin endpoints
- `crates/ac-service/src/repositories/signing_keys.rs:103` — #[allow(dead_code)] // Library function - will be used in Phase 4 key rotation
- `crates/ac-service/src/repositories/signing_keys.rs:171` — #[allow(dead_code)] // Library function - will be used in Phase 4 key management
- `crates/ac-service/src/repositories/signing_keys.rs:82` — #[allow(dead_code)] // Library function - will be used in Phase 4 JWKS/admin endpoints
- `crates/ac-service/src/repositories/users.rs:15` — #[allow(dead_code)] // Library type - fields read in future phases
- `crates/ac-service/src/repositories/users.rs:194` — #[allow(dead_code)] // Library function - will be used in future phases
- `crates/ac-service/src/services/key_management_service.rs:108` — #[allow(dead_code)] // Library function - will be used in Phase 4 key rotation endpoints
- `crates/ac-service/src/services/key_management_service.rs:179` — #[allow(dead_code)]
- `crates/ac-service/src/services/key_management_service.rs:344` — #[allow(dead_code)] // Will be used in background tasks/cron jobs in production
- `crates/gc-service/src/auth/claims.rs:59` — #[allow(dead_code)] // Will be used in Phase 3 for scope checking
- `crates/gc-service/src/errors.rs:46` — multi-line expect block (reason = "Conflict has no constructor; all other variants are live")
- `crates/gc-service/src/middleware/auth.rs:101` — #[allow(dead_code)] // API for handlers that need claims from request
- `crates/gc-service/src/middleware/auth.rs:109` — #[allow(dead_code)] // Implementation for ClaimsExt trait
- `crates/gc-service/src/models/mod.rs:139` — #[allow(dead_code)] // Fields used in database queries and future phases
- `crates/gc-service/src/models/mod.rs:15` — #[allow(dead_code)] // Will be used in Phase 2+ for meeting management
- `crates/gc-service/src/models/mod.rs:32` — #[allow(dead_code)] // Will be used in Phase 2+
- `crates/gc-service/src/models/mod.rs:49` — #[allow(dead_code)]
- `crates/gc-service/src/models/mod.rs:93` — #[allow(dead_code)] // Used by integration tests and future join handler
- `crates/gc-service/src/repositories/media_handlers.rs:14` — #![allow(dead_code)]
- `crates/gc-service/src/repositories/media_handlers.rs:342` — #[allow(dead_code)] // Will be used in future phases
- `crates/gc-service/src/repositories/media_handlers.rs:412` — #[allow(dead_code)]
- `crates/gc-service/src/repositories/meeting_assignments.rs:33` — #[allow(dead_code)]
- `crates/gc-service/src/repositories/meeting_controllers.rs:314` — #[allow(dead_code)] // Will be used in future phases
- `crates/gc-service/src/repositories/meeting_controllers.rs:423` — #[allow(dead_code)] // Used by get_controller
- `crates/gc-service/src/repositories/meeting_controllers.rs:55` — #[allow(dead_code)] // Used by get_controller
- `crates/gc-service/src/repositories/meeting_controllers.rs:70` — #[allow(dead_code)] // Will be used for MC lookup
- `crates/gc-service/src/repositories/meetings.rs:418` — #[allow(dead_code)] // Used by integration tests and future join handler
- `crates/gc-service/src/repositories/participants.rs:21` — #[allow(dead_code)] // Used by integration tests and future join handler
- `crates/gc-service/src/repositories/participants.rs:24` — #[allow(dead_code)] // Methods used by integration tests and future join handler
- `crates/gc-service/src/services/mc_assignment.rs:135` — #[allow(dead_code)]
- `crates/gc-service/src/services/mc_assignment.rs:85` — #[allow(dead_code)] // Read only by integration tests (bin target sees no reader)
- `crates/gc-service/src/services/mc_client.rs:251` — #[allow(dead_code)] // Used by mock implementation
- `crates/gc-service/src/services/mc_client.rs:282` — #[allow(dead_code)]

**Change 2: nextest.**
- `scripts/lang/rust/test.sh` runs two `run_and_emit` lanes:
  - `cargo-nextest` (`--profile layer4 --show-progress none "$@"`);
  - `cargo-doctest` (`cargo test --doc --no-fail-fast` plus the package-selection subset of `"$@"`).
- It then closes with `STATUS=FAIL REASON=rust-test-lanes-failed-<lanes>` when either lane is red. The dispatcher reads the LAST STATUS line, so without this a green doctest lane after a red nextest lane would have masked it.
- `check_nextest` runs before migrations. A missing runner gives `cargo-nextest-missing`; a version mismatch gives `cargo-nextest-version-mismatch` plus the `CARGO_NEXTEST_VERSION_MISMATCH:` message.
- `.config/nextest.toml` profile `layer4`: every key was verified against 0.9.146 by running it.
- Tool pins: `infra/cargo-tools.versions` plus `infra/lib/cargo-tools.sh`. Consumers:
  - `devloop.sh` (one `build_devloop_image` for both build paths);
  - the Dockerfile (no ARG defaults; `cargo install --locked --version =`);
  - ci.yml (pipeline + coverage);
  - audit-scheduled.yml;
  - fuzz-nightly.yml (`cargo-fuzz` now `--locked` and pinned);
  - `test.sh`.
- `setup.test.sh` covers the reader matrix, the devloop derivation, and a no-literal/unpinned scan over **every** workflow plus the devloop Dockerfile, with 6 positive controls.
- Self-tests:
  - `behavior-equivalence.test.sh` checks the exact two-lane argv read from test.sh, three arg shapes, a nextest-absent case, a version-drift case and a red-lane-not-masked case (38 pass).
  - `layer7.test.sh` pins the L4 no-fail-fast through the profile.

**Change 3: cache.** `cache-on-failure: true` with the `pull_request_target`/`workflow_run` premise comment on every rust-cache step (pipeline, coverage, ci-client). Each shard has its own `shared-key`. `cache-bin: false` on shards.

**Change 4: split.**
- `layer-all.sh` gains:
  - `LAYER_MAX` (in `_common.sh`; `layer-fast.sh` derives `LAYER_MAX-1`);
  - `--layers L,…`: CI or the test seam only, never a verdict, writes `layer-summary.txt`;
  - `--aggregate DIR`: strict anchored parse, known statuses derived from `__status_rank`, exactly-once over 1..`LAYER_MAX`, worst status plus RC floor, `FAILURE_TRIAGE … SHARD=`.
- ci.yml has three matrix shards with per-shard tool flags, the summary upload set to `if: always()`, and a `Test Suite` aggregator. The aggregator's first step fails unless `needs.pipeline.result == 'success'`; its later steps run `if: always()` for diagnostics only. It downloads this run's artifacts only.
- `layer-all.test.sh` gains (G) shard cases (incl. "pre-existing verdict byte-identical", "real RC recorded", refusals, invalid lists) and (H) aggregate cases (incl. RC floor, operator lane, each malformed REASON, and a shard→aggregate round trip): 167 pass.
- Each shard gets its own `timeout-minutes` (`matrix.timeout_minutes`: guards 20, test 25, lint 10). These are the projected CI cold durations from §CI Timings × 2, rounded up to 5 min, and they replace the single 40.
- actionlint is clean on all workflows.

**Change 5.** The ci.yml dt-guard build is deleted. ci-client.yml's build is kept and commented as the only producer in its job. sqlx-cli is cached via `actions/cache`, keyed on OS + rustc + Cargo.lock sqlx.

**Change 6.** A Dependabot `rust-toolchain` block.

**Docs.**
- ADR-0033 §10 amendment.
- Runbook §8.7 (job map, red `Test Suite` triage, rollback per concern, tool pins/image rebuild, rust-toolchain PR, cache notes), §6.4 rows, the §8 catalogue, the backstop paragraph and the required-check note.
- `_gate2_binding.sh` THREAT MODEL text.
- TODO: CI-speed closed; the guard-binaries entry is updated (4 sites, counted honestly); a new coverage-job entry.
- ac README toolchain literal; `release_build_profile.rs` doc and a stale `ci.yml:170` citation.

**Host action (remaining, after merge):** every devloop/run-story container needs `infra/devloop/devloop.sh --rebuild` before Layer 4 passes (nextest). This container got nextest via `cargo install --locked --version =0.9.146 cargo-nextest`.

**Fast check:** `./scripts/layer-fast.sh` is green (L1 38 s, L2 7, L3 133, L4 175, L5 7, L6 3).

**Gate-3 fixes (iteration 1).**
- **crypto (security F1, auth-controller F1):** `guard:ignore` removed; both messages reworded to JWT; the TODO §Guard Precision entry is updated with Check 4's position imprecision.
- **Pin reads (ops F1, semantic-guard):** every "Read tool pins" step assigns before it writes `$GITHUB_OUTPUT`. A failing reader inside echo's arguments does not trip `bash -e`. The rustc version is checked non-empty. `setup.test.sh` now forbids `echo "x=$(…)"` in workflows, with a positive control.
- **Stale runbook ref (ops F2):** `cargo-test-failed` → `cargo-nextest-failed`.
- **DRY-1 (BLOCKER):** the private lexer in `bin_lib_single_compile.rs` is deleted. The guard now uses the shared `blank_file`, moved from `media_telemetry_deny.rs` into `common/test_code_filter.rs`, and computes brace depth over the blanked text.
- **DRY-2:** `cargo-chef` joins `infra/cargo-tools.versions`, and `deploy.sh` reads it through the reader. The no-literal scan now covers `infra/docker/*/Dockerfile` (sqlx-cli is exempt, being pinned by Cargo.lock), and there are service-image positive and negative controls.
- **DRY-3 / code-reviewer F1 (+ Lead follow-up):** the summary's location lives only in layer-all.sh (`SHARD_SUMMARY_FILE` under `DEVLOOP_TMP`). Shard mode writes `summary-path=` to `$GITHUB_OUTPUT` before any layer runs, and the upload's `path:` is `${{ steps.shard.outputs.summary-path }}`, so ci.yml restates neither the directory nor the file name. Test G1 asserts the output points at the written file.
- **ops F3:** the open docs/TODO.md entries (thread-spawn ~1913-1936, linker abort ~2099) name `cargo-nextest-failed` / `rust-test-lanes-failed-nextest`, with the old token noted for grep.
- **DRY-4:** one result model. A full run, a shard and the aggregate share `layer_status`/`layer_dur`/`layer_rc`, `compute_layer_totals` and `print_layer_summary`; the aggregate only adds a Shard column. New test H15 checks that the aggregate's LAYER_SUMMARY block is byte-identical in shape to a full run's.
- **test F1:** a selection whose packages all lack a lib target (e.g. `-p devloop-helper`) skips the doctest lane EXPLICITLY (`STATUS=OK REASON=cargo-doctest-no-lib-targets`). This is decided from `cargo metadata` via jq, not from cargo's error text. A shim case covers it, with a synthetic metadata answer and the bin-only crate derived from it.
- **test F2:** `retries = 0` is pinned in `layer7.test.sh`, along with no `--retries`/`NEXTEST_RETRIES` at the call site.
- **Runbook:** rows for `cargo-doctest-no-lib-targets` and for nextest's intentional no-tests exit 4; cargo-chef is added to §8.7's pin list.

## Test Counts (before → after, per crate)

Source: `cargo nextest list --workspace --run-ignored all` (per binary); doctests from `cargo test --doc -- --list` and the L4 doctest lane.

| Crate | Before | After | Delta, attributed |
|-------|-------:|------:|-------------------|
| ac-service | 1004 | 602 | −402: `ac-service::bin/auth-controller` 402 → 0 (lib unit binary `ac-service` 402 unchanged) |
| gc-service | 1056 | 663 | −393: `gc-service::bin/gc-service` 393 → 0 (lib unit binary `gc-service` 393 unchanged) |
| dt-guard | 854 | 867 | +13: new `bin_lib_single_compile` tests (lib binary 684 → 697) |
| mc-service | 732 | 732 | — |
| mh-service | 462 | 462 | — |
| common | 252 | 252 | — |
| devloop-helper | 180 | 180 | — |
| env-tests | 142 | 142 | — |
| dt-story | 62 | 62 | — |
| proto-gen | 51 | 51 | — |
| media-protocol | 48 | 48 | — |
| mc-test-utils | 22 | 22 | — |
| media-vector-gen | 19 | 19 | — |
| ac-test-utils | 17 | 17 | — |
| gc-test-utils | 7 | 7 | — |
| mh-test-utils | 0 | 0 | — |
| **Total** | **4908** (164 binaries) | **4126** (164 binaries) | −795 bin duplicates, +13 new |

- Per-binary diff before → after: exactly three binaries changed (the two bin-unit binaries to 0, and dt-guard's lib +13). Every other binary is identical, ignored counts included. Ignored = 0 throughout: the 3 ac `#[cfg_attr(coverage, ignore)]` tests are ignored only under `cfg(coverage)` and live in the lib binary.
- Doctests: 40 → 40 (3 run: common/secret.rs ×2, env-tests/eventual.rs; 37 `ignore`), now in the `cargo-doctest` lane.
- Flake evidence (`layer4`-equivalent settings, 12 threads, `taskset -c 0-3`): 3 consecutive full runs, 4908/4908 each (201.6 s / 196.4 s / 200.9 s). Postgres max_connections is 100; measured peak `pg_stat_activity` during those runs was **31** (1 s sampling), against a worst-case estimate of about 72.

## CI Timings

**Method (user decision: measured locally, not from CI runs).**
- 4-CPU emulation of the `ubuntu-latest` runner: `taskset -c 0-3`, `CARGO_BUILD_JOBS=4`.
- Each shard runs exactly as the matrix runs it (`GITHUB_ACTIONS=true ./scripts/layer-all.sh --layers …`), one shard at a time.
- Script: `measure.sh` in the implementer's scratchpad.
- **cold** = `rm -rf target/` (cargo registry kept: in CI rust-cache restores it with the same key, and on a fully cold key the download is part of each shard's first cargo call).
- **warm** = the previous run's `target/` with every workspace member's own artifacts removed (`cargo clean -p <member>…`), which is what `Swatinem/rust-cache` restores. The `target/` size after that strip stands in for the per-shard cache size.
- Layer 7 was left out of the local guards shard, because a devloop helper socket is present here and L7 would hit the live cluster. In CI it is a ~0 s `SKIPPED-NO-CLUSTER`.

**Two measurement notes.**
- The local guards shard read `rc=1` because Layer 3's `run-story.test.sh` had three cases that read an ambient `DEVLOOP_TMP`, which `measure.sh` relocates. In CI `DEVLOOP_TMP` is unset, and `layer-fast.sh` was green. The three cases now unset it explicitly (`scripts/workflow/run-story.test.sh`, fix-don't-defer), and the suite is 498/498 with a relocated `DEVLOOP_TMP`. The timing is unaffected: the suite ran to completion either way.
- Coverage was measured as its `cargo` commands only (`llvm-cov clean`, `show-env`, `cargo build -p dt-guard`, `cargo test --workspace`), without the bash guard suites or the upload.

**Local → CI factor ≈ 1.4.**
- From run 37171005734 (last green before this change): the "Run polyglot validation pipeline" step took 1408 s.
- The same pipeline cold on the emulation was the 929 s `layer-fast.sh` baseline, plus the separate 67 s "Build guard binaries" step that CI then ran outside the pipeline step: 1408 / 996 ≈ **1.41**.
- Caveat: one run, cache state unknown, so treat it as ±20 %.
- CI setup outside `layer-all.sh` is added per shard from that run's step times: containers ~25 s, apt ~10 s, toolchain(s) ~8-16 s, pnpm/node/install ~20 s, tool installs ~5 s each (taiki-e prebuilt), sqlx-cli ~53 s on a cache miss / ~2 s on a hit, Playwright ~18 s, post-cache save ~20 s. That gives guards ≈ 2.0 min, test ≈ 2.5 min (sqlx miss), lint ≈ 1.5 min.

| Job | Local cold (s) | Local warm (s) | `target/` cache size | Projected CI cold | Projected CI warm | `timeout-minutes` |
|-----|---------------:|---------------:|---------------------:|------------------:|------------------:|------------------:|
| `Pipeline (guards, 1,2,3,6,7)` | 297 (L1 155, L2 5, L3 132, L6 5) | 158 (L1 20, L3 132) | 2.3 G | 297×1.41 + 2.0 min ≈ **9.0 min** | ≈ 5.7 min | 20 |
| `Pipeline (test, 4)` | 366 (compile 144, nextest 177, TS ~40) | 315 (compile 106, nextest 179) | 1.8 G | 366×1.41 + 2.5 min ≈ **11.1 min** | ≈ 9.4 min | 25 |
| `Pipeline (lint, 5)` | 58 | 17 | 0.9 G | ≈ **2.9 min** | ≈ 1.9 min | 10 |
| `Test Suite` (aggregate) | <1 | <1 | — | ≈ 0.5 min after the slowest shard | | 5 |
| `Code Coverage` (unchanged job; benefits from change 1) | 524 | 339 | — | ≈ 14 min (≈ 12.3 + setup) | ≈ 9.5 min | 40 (unchanged) |

**Before → after (Test Suite wall-clock).**

| | Before | After (projected) |
|---|---|---|
| CI cold | ~29 min (run 36923344871, cancelled at the 25-min limit then); 27.3 min (37171005734) | **~11 min** (slowest shard: test) |
| CI warm | 19.5 min (37076908271, main push) | **~9.5 min** |
| Local 4-CPU cold, whole pipeline | 929 s serial (`layer-fast.sh`) | max shard 366 s (shards in parallel) |
| Local 4-CPU, Rust tests alone (built) | 395 s (`cargo test`) | ~177 s (nextest `layer4`, 4126 tests) |

**What remains on the critical path.**
- The test shard is nextest at ~177 s locally. Its long pole is one 55 s mh-service test, `media_session_binding_integration::an_unreachable_mc_declines_on_the_reachability_outcome`, which owns about a third of the run's tail. A config-driven shorter reachability timeout in that test would trim the tail; that is the media-handler owner's test design, so it is not done here.
- `Code Coverage` (~14 min, advisory) becomes the workflow's longest job; there is a TODO entry for it.
- The user pushes and checks CI after the commit; the first real per-shard numbers should be compared against this table.

## Gate 3 — Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 1 | 1 | 0 | GSA guard:ignore removed; guard Check-4 position imprecision appended to existing TODO entry (pre-existing, not a deferral) |
| Observability | RESOLVED-FIXED | 1 | 1 | 0 | AC runbook log-target note |
| Paired Auth Controller | RESOLVED-FIXED | 1 | 1 | 0 | GSA crypto/mod.rs: guard:ignore replaced by message reword; owner-approved (Ownership Lens) |
| Semantic Guard | RESOLVED-FIXED | 1 | 1 | 0 | pin-reader echo-substitution masking fixed; setup.test.sh forbids the pattern |
| Test | RESOLVED-FIXED | 2 | 2 | 0 | doctest lane on lib-less selection; retries=0 pinned |
| Code Quality | RESOLVED-FIXED | 1 | 1 | 0 | hardcoded /tmp/devloop upload path → job-level DEVLOOP_TMP |
| DRY | RESOLVED-FIXED | 4 | 4 | 0 | DRY-1 BLOCKER (private lexer) fixed; sqlx-cli flags filed as extraction opportunity |
| Operations | RESOLVED-FIXED | 3 | 3 | 0 | pin-reader masking; stale runbook token; stale TODO tokens |

## Gate 2 — Full Validation

`DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` on the final tree (attempt 1 of 3; L7 attempt 1 of 2), exit 0:

| Layer | Result | Duration |
|-------|--------|----------|
| 1 Compile | OK | 5 s |
| 2 Format | OK | 3 s |
| 3 Guards | OK | 131 s |
| 4 Test | N/A (proto intentional-gap placeholder; rust `cargo-nextest` 4126/4126 OK + `cargo-doctest` OK; ts OK) | 162 s |
| 5 Lint | OK | 1 s |
| 6 Audit | N/A (proto placeholder; dep-manifest gate) | 3 s |
| 7 Env-tests | OK | 904 s |

`TOTAL_RESULT=N/A` (N/A ranks above OK in the worst-child aggregation; same shape as prior devloops). Run with cargo-nextest 0.9.146 installed in this container by `cargo install --locked --version =0.9.146 cargo-nextest`. The devloop **image** install path (Dockerfile) is not exercised until a host-side `infra/devloop/devloop.sh --rebuild`.

### Remaining actions (host / user)
- After merge: every devloop and story-runner container needs `infra/devloop/devloop.sh --rebuild` (Layer 4 fails loudly with that hint until then).
- Push the branch and confirm CI green; real cold/warm CI job timings were not measured here (local projections only, per the user's decision).

## Accepted Deferrals

None. TODO-filed observations (not deferrals of findings in this diff): Code Coverage job is now the longest workflow job (~14 min projected); sqlx-cli install flags hand-typed at five sites (DRY extraction opportunity); no-secrets-in-logs Check 4 position imprecision (appended to existing §Guard Precision entry).
