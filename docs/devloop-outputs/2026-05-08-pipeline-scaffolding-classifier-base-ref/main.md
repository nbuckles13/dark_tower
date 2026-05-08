# Devloop Output: Pipeline scaffolding + classifier + base-ref helper

**Date**: 2026-05-08
**Task**: R-62, ADR-0033 Wave 1 #1 — pipeline scaffolding (`scripts/layerN.sh`), per-language wrappers (`scripts/lang/`), classifier, base-ref helper, per-verb dispatchers, refactor of `scripts/test.sh` and `scripts/verify-completion.sh`
**Specialist**: infrastructure (paired with operations)
**Mode**: Agent Teams (v2), full
**Branch**: `feature/browser-client-join-task32`
**Duration**: TBD

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `b552b91c139c96a5379f4719daec5f1217a8948e` |
| Branch | `feature/browser-client-join-task32` |
| User Story | `docs/user-stories/2026-05-02-browser-client-join.md` task #32 |
| ADR | `docs/decisions/adr-0033-polyglot-validation-pipeline.md` Wave 1 #1 |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@devloop-2026-05-08-pipeline-scaffolding` |
| Implementing Specialist | `infrastructure` |
| Iteration | `1` |
| Security | `RESOLVED` |
| Test | `RESOLVED` |
| Observability | `CLEAR` |
| Code Quality | `CLEAR` |
| DRY | `CLEAR` |
| Operations (paired) | `CLEAR` |

**Gate 1 disposition (2026-05-08)**: All 6 reviewers confirmed plan after multi-round
pair-design conversations. Layer B classification-sanity guard
(`scripts/guards/simple/validate-cross-boundary-classification.sh`) ran clean
(no GSA paths, no Mechanical-in-GSA violations). Plan approved 2026-05-08.

---

## Task Overview

### Objective

Land the scaffolding for the polyglot validation pipeline (ADR-0033 Wave 1 #1):

1. `scripts/layerN.sh` skeletons for layers 1–7, each with `set -euo pipefail`,
   STATUS aggregation (worst-child wins), streaming child STATUS lines verbatim,
   and a `LAYER=N START=<ts> END=<ts> RESULT=<enum>` summary line to stderr.
2. `scripts/layer-all.sh` orchestrator: runs `layer1.sh`..`layer7.sh` sequentially,
   `tee /tmp/devloop/layer-N.log` per layer, final summary table, and 90s p95
   wall-clock budget enforcement (warn on per-layer breach).
3. `scripts/lang/_common.sh`, `_dispatch.sh`, `_changed_helpers.sh`,
   `_get_base_ref.sh` shared helpers, plus `_get_base_ref.test.sh` (matrix:
   local-clean, local-dirty, local-with-untracked, CI-PR, CI-push, first-commit)
   and `_test_changed_predicates.sh` meta-test.
4. `scripts/lang/rust/{changed.sh, changed.test.sh}` — Rust changed-classifier
   plus locality self-test.
5. Per-verb dispatchers `scripts/{audit,lint,test,fmt,build}.sh` using
   `_dispatch.sh::for_each_lang_with_verb` (loud-on-missing-verb via
   `STATUS=SKIPPED-NO-VERB`).
6. Refactor: move `scripts/test.sh` body into `scripts/lang/rust/test.sh`;
   `scripts/test.sh` becomes a thin shim preserving external contract.
7. Refactor: `scripts/verify-completion.sh` calls `scripts/layer-all.sh`.
8. **Behavior-equivalence test required**: same exit code on the same Rust-only
   diff before and after the refactor.

### Scope

- **Service(s)**: None (build/CI tooling only)
- **Schema**: No
- **Cross-cutting**: Yes — affects every devloop's validation pipeline

### Debate Decision

NOT NEEDED — debate was already held; ADR-0033 is the spec. This devloop implements Wave 1 #1.

---

## Cross-Boundary Classification

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `scripts/layer1.sh` | Mine | — |
| `scripts/layer2.sh` | Mine | — |
| `scripts/layer3.sh` | Mine | — |
| `scripts/layer4.sh` | Mine | — |
| `scripts/layer5.sh` | Mine | — |
| `scripts/layer6.sh` | Mine | — |
| `scripts/layer7.sh` | Mine | — |
| `scripts/layer-all.sh` | Mine | — |
| `scripts/audit.sh` | Mine | — |
| `scripts/lint.sh` | Mine | — |
| `scripts/test.sh` | Mine | — |
| `scripts/fmt.sh` | Mine | — |
| `scripts/build.sh` | Mine | — |
| `scripts/lang/_common.sh` | Mine | — |
| `scripts/lang/_dispatch.sh` | Mine | — |
| `scripts/lang/_changed_helpers.sh` | Mine | — |
| `scripts/lang/_get_base_ref.sh` | Mine | — |
| `scripts/lang/_get_base_ref.test.sh` | Mine | — |
| `scripts/lang/_test_changed_predicates.sh` | Mine | — |
| `scripts/lang/rust/changed.sh` | Mine | — |
| `scripts/lang/rust/changed.test.sh` | Mine | — |
| `scripts/lang/rust/test.sh` | Mine | — |
| `scripts/lang/rust/compile.sh` | Mine | — |
| `scripts/lang/rust/fmt.sh` | Mine | — |
| `scripts/lang/rust/lint.sh` | Mine | — |
| `scripts/lang/rust/audit.sh` | Mine | — |
| `scripts/lang/rust/behavior-equivalence.test.sh` | Mine | — |
| `scripts/lang/rust/fixtures/cargo-shim` | Mine | — |
| `scripts/lang/rust/fixtures/equivalence-rust-only.patch` | Mine | — |
| `scripts/lang/_common.test.sh` | Mine | — |
| `scripts/lang/_dispatch.test.sh` | Mine | — |
| `scripts/lang/_layer_skeleton.test.sh` | Mine | — |
| `scripts/verify-completion.sh` | Mine | — |

All paths fall under `scripts/` — owned by infrastructure. None hit GSA paths
(`proto/`, `crates/common/{jwt,meeting_token,token_manager,secret}.rs`,
`crates/common/webtransport/`, `crates/ac-service/src/{jwks,token,crypto,audit}/`,
`crates/media-protocol/`, `db/migrations/`). Operations is paired-with for
collaboration on the layer-script contract and SKILL.md alignment, not for
ownership transfer.

---

## Planning

### Architecture Overview

We are landing the polyglot validation pipeline scaffolding per ADR-0033. The shape:

```
scripts/
  layer-all.sh                   # orchestrator, tee logs, summary table, p95 budget warn
  layer1.sh ... layer7.sh        # per-layer scripts; each emits worst-child STATUS
  build.sh fmt.sh lint.sh        # per-verb dispatchers (iterate scripts/lang/*/)
  test.sh audit.sh
  verify-completion.sh           # refactored to call layer-all.sh, preserves --layer/--format/--verbose
  lang/
    _common.sh                   # cache paths, color helpers, emit_status, aggregate_worst_status
    _dispatch.sh                 # for_each_lang_with_verb <verb>
    _changed_helpers.sh          # diff_touches_path, diff_touches_root_files
    _get_base_ref.sh             # env-aware base ref + normative BASE_REF= stderr line
    _get_base_ref.test.sh        # matrix self-test (local-clean, local-dirty, untracked, CI-PR, CI-push, first-commit)
    _test_changed_predicates.sh  # meta-test against fixture diffs
    rust/
      changed.sh                 # exit 0 if rust touched, 1 if untouched
      changed.test.sh            # locality self-test for changed.sh
      test.sh                    # body migrated from scripts/test.sh (cargo test + DB bring-up)
```

Wave 2 will add `lang/ts/` and `lang/proto/`. Their absence in Wave 1 is correct — `for_each_lang_with_verb` only iterates directories that exist; with only `lang/rust/` present, Layer 1/2/4/5 dispatchers emit a single Rust line plus (for verbs Rust doesn't yet have) `STATUS=SKIPPED-NO-VERB`. This is the design.

### Layer-script contract (ADR-0033 §4 + §6)

Per dry-reviewer §1, the lifecycle boilerplate (steps 4+5+6) is factored into `_common.sh` helpers — `layerN.sh` bodies are 1-2 *real* lines, not 30. **No `date +%s` calls, no `LAYER=...` echo, no STATUS aggregation, no exit-code arithmetic in `layerN.sh` bodies.**

`_common.sh` provides:

```bash
# Begin layer lifecycle: capture start time, init STATUS list, install EXIT trap.
# CRITICAL (observability O2): the EXIT trap fires even on `set -e` abort or signal,
# so the LAYER=... stderr line is GUARANTEED to emit — even when a child wrapper
# kills the layer mid-flight. 3am debug case: runbook reader always sees which layer
# failed and how long it ran.
layer_lifecycle_begin <layer-num>
#   - sets __LAYER_NUM, __LAYER_START, __LAYER_STATUSES=(), __LAYER_RESULT="UNKNOWN"
#   - trap '__layer_lifecycle_end' EXIT

# Stream wrapper output and append parsed STATUS lines to lifecycle's collector.
# Reads stdin, echoes verbatim to stdout, side-effects __LAYER_STATUSES.
tee_collect_statuses

# (called from EXIT trap installed by layer_lifecycle_begin — fires on set -e abort too)
__layer_lifecycle_end
#   1. end = now
#   2. result = aggregate_worst_status "${__LAYER_STATUSES[@]}"  (UNKNOWN if empty)
#   3. stdout: STATUS=<result> REASON=layer<n>-<reason>
#   4. stderr: LAYER=<n> START=<s> END=<e> DURATION=<d> RESULT=<r> REASON=<reason>
#   5. exit code per result (FAIL → 1, UNKNOWN → 2 [dispatcher bug], else 0)
```

**Observability O2 — LAYER line emission on FAIL/abort guarantee.** The EXIT trap is the load-bearing mechanism. Even if a child wrapper exits non-zero and trips `set -e` mid-pipe, the trap still fires, `__LAYER_RESULT` defaults to `UNKNOWN`, and the runbook reader sees a LAYER stderr line. No silent disappearance.

Then `layerN.sh` becomes:

```bash
#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "$0")/lang/_common.sh"
layer_lifecycle_begin 4
"$(dirname "$0")/test.sh" | tee_collect_statuses
```

A future Layer 8 lands as 4 lines, not 30. Any fix to the LAYER= line shape lives in one helper, not 7 copies.

Layer-to-verb map (the 1-2 real lines per layer):
| Layer | Body |
|------:|------|
| 1 | `scripts/build.sh` (Stage 1 proto-first guard + Stage 2 code; per paired-operations §6 / ADR-0033 §5) |
| 2 | `scripts/fmt.sh` |
| 3 | `scripts/guards/run-guards.sh` (existing) + `_test_changed_predicates.sh` invocation per ADR-0033 implementation note |
| 4 | `scripts/test.sh` |
| 5 | `scripts/lint.sh` |
| 6 | `scripts/audit.sh` (always-run; per-language wrappers control their own skip) |
| 7 | placeholder body — Wave 2 fills env-tests; emits `STATUS=N/A REASON=wave2-pending` for now |

Wrapper contract (unchanged from ADR-0033 §6): exit 0 on OK / SKIPPED / N/A; exit 1 on FAIL; exit 2 on dispatcher bug. The lifecycle helper enforces this via the EXIT trap.

Layer-to-verb map:
| Layer | Body |
|------:|------|
| 1 | `scripts/build.sh` (proto would run first when proto/ exists; for Wave 1 just runs rust+ts dispatch) |
| 2 | `scripts/fmt.sh` |
| 3 | `scripts/guards/run-guards.sh` (existing) → wrap output as a single STATUS line; also invokes `_test_changed_predicates.sh` per ADR-0033 implementation note |
| 4 | `scripts/test.sh` |
| 5 | `scripts/lint.sh` |
| 6 | `scripts/audit.sh` (always-run; per-language wrappers control their own skip) |
| 7 | placeholder body — Wave 2 fills env-tests; emits `STATUS=N/A REASON=wave2-pending` for now |

### Per-verb dispatcher contract

`scripts/{audit,lint,test,fmt,build}.sh`:
- Each sources `scripts/lang/_dispatch.sh` and calls `for_each_lang_with_verb "<verb>"`
- `_dispatch.sh::for_each_lang_with_verb`:
  1. At startup, lints that every `scripts/lang/<X>/` (excluding underscore-prefixed) has executable `changed.sh` — fails loud if missing
  2. For each language, runs `changed.sh` (skip-if-untouched short-circuit)
  3. If `changed.sh` says "untouched" → emit `STATUS=SKIPPED-NO-DIFF REASON=<lang>-no-diff`
  4. If touched and `<verb>.sh` exists+executable → invoke; stream its STATUS verbatim
  5. If touched but `<verb>.sh` missing/non-executable → emit `STATUS=SKIPPED-NO-VERB REASON=<lang>/<verb>.sh-missing-or-not-executable`
  6. Audit dispatcher special-cases "always-run": skips the `changed.sh` short-circuit (each lang's audit.sh runs regardless of diff)

**STATUS line emission rule (paired-operations Q3 delta — load-bearing comment in `_dispatch.sh`):**

The dispatcher streams each child wrapper's STATUS verbatim. The dispatcher itself does NOT synthesize an additional STATUS line when only ONE language is iterated — that would double-emit (one from dispatcher + one from `lang/rust/test.sh`). When multiple languages are iterated, the dispatcher emits an aggregated final STATUS (worst-child wins) AFTER streaming each child's STATUS. The layer script then reads the LAST STATUS line and aggregates again at layer level.

Concrete:
- 1 lang touched, 1 STATUS streamed → final stdout line is that single STATUS. Dispatcher does not re-emit.
- 2+ langs (each emitting their own STATUS) → dispatcher streams each then emits ONE aggregated STATUS as the final line.
- 0 langs touched (everything SKIPPED-NO-DIFF) → each lang's SKIPPED-NO-DIFF streamed; dispatcher emits aggregated `STATUS=SKIPPED-NO-DIFF REASON=all-langs-untouched`.

This rule is documented as a comment near `for_each_lang_with_verb` so future-us doesn't add a duplicate emission path.

### `scripts/test.sh` strategy (decision)

Going with **option (b)**: `scripts/test.sh` IS the dispatcher; rust args flow through. Rationale:

- Existing CI / muscle memory: `./scripts/test.sh --workspace` and `./scripts/test.sh -p ac-service --lib`. Both are *cargo args*. If we forward them only when `lang/rust/test.sh` runs, behavior equivalence holds.
- ADR-0033 §1 lists `scripts/test.sh` as the per-verb test dispatcher (`KEEP NAME — refactored: per-verb dispatcher (test verb)`). Option (b) honors that literally.
- The thin shim approach: `scripts/test.sh` sources `_dispatch.sh` and calls `for_each_lang_with_verb "test" "$@"`. The dispatcher passes the args to the lang's `test.sh`. Rust's `test.sh` interprets cargo args; future TS `test.sh` would translate as needed. With only Rust present in Wave 1, args land in `lang/rust/test.sh` exactly as if invoked directly — bit-identical exit code.

The behavior-equivalence test (point 7 below) verifies this empirically.

### `_get_base_ref.sh` (ADR-0033 §7)

```bash
# Pseudocode
validate_ref_name(ref):                              # security ask §1: defense-in-depth
  if ref !~ ^[A-Za-z0-9._/-]+$:
    echo "ERROR: ref name contains unexpected characters: $ref" >&2
    exit 2

detect_environment:
  if GITHUB_ACTIONS set:
    if GITHUB_EVENT_NAME=pull_request:
      validate_ref_name "$GITHUB_BASE_REF"           # block bad input before git fetch
      git fetch --no-tags origin "$GITHUB_BASE_REF" || die loud
      BASE=origin/$GITHUB_BASE_REF; SOURCE=ci-pr; MODE=three-dot
    else (push):
      if git rev-parse --verify HEAD~1 succeeds:
        BASE=HEAD~1; SOURCE=ci-push-main; MODE=two-dot
      else:
        BASE=HEAD; SOURCE=ci-push-first-commit; MODE=two-dot
  else (local):
    validate_ref_name "origin/main"                  # consistency: same gate on hardcoded literal
    BASE=$(git merge-base origin/main HEAD); SOURCE=local-mergebase; MODE=two-dot

count files:
  if MODE=three-dot: git diff --name-only "$BASE...HEAD" | wc -l
  else: (git diff --name-only "$BASE"; if local: git ls-files --others --exclude-standard) | sort -u | wc -l

emit to stderr: BASE_REF=<sha> BASE_SOURCE=<src> DIFF_MODE=<mode> FILES_CHANGED=<n>
print to stdout: <BASE>      # for callers using "$(_get_base_ref.sh)"
```

Per security §1: `$GITHUB_BASE_REF` is validated against `^[A-Za-z0-9._/-]+$` before being passed to `git fetch`. Strict superset of valid git ref characters. Same gate applied on the local `origin/main` literal for helper consistency. Three lines, no runtime impact.

Per observability §2: `BASE_REF=<sha>` uses the **full 40-char sha** from `git rev-parse "$base"`. Unambiguous; runbook readers can copy-paste into `git show`. `BASE_SOURCE=<src>` uses the literal hyphenated tokens documented in the table (`local-mergebase`, `local-no-mergebase`, `ci-pr`, `ci-push-main`, `ci-push-first-commit`) — no spaces inside any value.

**Observability §5 — token redaction guardrails.** `_get_base_ref.sh` reads `$GITHUB_TOKEN` indirectly via `git fetch` (which authenticates against `origin`'s URL in CI, where the URL may include the token). Three guardrails:

1. **No `set -x`** anywhere in `_get_base_ref.sh`. Top-of-file comment: `# DO NOT enable trace mode; this script reads GITHUB_BASE_REF / GITHUB_TOKEN env in CI`.
2. **`git fetch` stderr is suppressed** (`git fetch ... 2>/dev/null`); helper emits its own error string with just the ref name on failure. Never echo the raw fetch output (which could include token-bearing redirected URLs).
3. **8th test case asserts no token leak**: in the `ci-pr-base-ref-unreachable` case, set `GITHUB_TOKEN=test-token-DO-NOT-LEAK` in the test env and assert the stderr error string does NOT contain that token. Easy regression-prevention.

Untracked files unioned in via `git ls-files --others --exclude-standard` only in the local case.

`_get_base_ref.test.sh` matrix — **8 cases** (test §1):
| Test | Setup | Assertion |
|------|-------|-----------|
| local-clean | git init temp; commit; checkout -b feat; commit | exits 0; BASE_SOURCE=local-mergebase; FILES_CHANGED=1 |
| local-dirty | local-clean + uncommitted edit | FILES_CHANGED reflects working-tree diff |
| local-with-untracked | local-clean + new untracked file | FILES_CHANGED includes the untracked file |
| local-no-mergebase | git init; ONE commit; no `origin/main` reachable | BASE_SOURCE=local-no-mergebase; falls back to HEAD (conservative) |
| ci-pr | GITHUB_ACTIONS=1 GITHUB_EVENT_NAME=pull_request GITHUB_BASE_REF=main | BASE_SOURCE=ci-pr; DIFF_MODE=three-dot |
| ci-push | GITHUB_ACTIONS=1 GITHUB_EVENT_NAME=push (HEAD~1 exists) | BASE_SOURCE=ci-push-main |
| ci-push-first-commit | GITHUB_ACTIONS=1 GITHUB_EVENT_NAME=push (single commit) | BASE_SOURCE=ci-push-first-commit |
| ci-pr-base-ref-unreachable | GITHUB_ACTIONS=1 GITHUB_EVENT_NAME=pull_request GITHUB_BASE_REF=does-not-exist | exits non-zero; stderr names `GITHUB_BASE_REF` |

**`local-no-mergebase` decision**: per test §1b, when no `origin/main` is reachable in local mode, fall back to `HEAD` (parallel to `ci-push-first-commit`) and emit `BASE_SOURCE=local-no-mergebase` so it's distinguishable from the success path. Conservative — classifies everything as touched.

Each test runs in `mktemp -d` with a fresh `git init`; the script chdir's there, exports relevant `GITHUB_*` env, invokes `_get_base_ref.sh`, asserts on stderr line and stdout sha presence. **Hermeticity (test §A)**: all tests use a local `file:///tmp/...` for `origin` (or no remote at all in unreachable cases) — no real `git fetch` against actual `origin`. Each case asserts via `assert_base_ref_line_well_formed()` (test §B) that stderr contains `BASE_REF=<sha>` AND `BASE_SOURCE=...` AND `DIFF_MODE=...` AND `FILES_CHANGED=<n>`. Schema drift fails every test, never silently.

### `_changed_helpers.sh`

```bash
# Caches the diff list once per script invocation, sourced from _get_base_ref.sh output.
__cached_changed_files() {
  if [ -z "${__DEVLOOP_CHANGED_FILES_CACHE+x}" ]; then
    local base diff_mode
    base=$("$(dirname "${BASH_SOURCE[0]}")/_get_base_ref.sh")
    # detection of mode handled inside _get_base_ref.sh; export the file list via $DEVLOOP_TMP/changed-files
    export __DEVLOOP_CHANGED_FILES_CACHE
    __DEVLOOP_CHANGED_FILES_CACHE=$(...)
  fi
  printf '%s\n' "$__DEVLOOP_CHANGED_FILES_CACHE"
}
diff_touches_path() {  # arg: prefix, e.g. "crates/"
  __cached_changed_files | grep -q "^$1" && return 0 || return 1
}
diff_touches_root_files() {  # args: file paths at repo root
  for f in "$@"; do __cached_changed_files | grep -qx "$f" && return 0; done
  return 1
}
```

Note: cache lives across function invocations within the same shell process; per-script invocation it's recomputed once. To keep `_get_base_ref.sh` stderr-emission single-shot per layer, we cache via `DEVLOOP_TMP/changed-files.<pid>` file written on first call. This is a minor implementation detail — final code may inline rather than cache if the diff is fast enough on the repo (it is).

### `_test_changed_predicates.sh` meta-test

Hand-curated fixture set — **expanded per test §2** (each row carries a `# rationale:` comment in the fixture file):
| Fixture path | rust expects | (future) ts expects | (future) proto expects | Rationale |
|--------------|:-:|:-:|:-:|------|
| `crates/foo/src/lib.rs` | touched | untouched | untouched | happy-path Rust src |
| `Cargo.toml` | touched | untouched | untouched | root cargo manifest |
| `Cargo.lock` | touched | untouched | untouched | exercises root-files arm of predicate (§2 example lists 3 root files) |
| `rust-toolchain.toml` | touched | untouched | untouched | exercises root-files arm — proves helper iterates all listed files |
| `crates/common/Cargo.toml` | touched | untouched | untouched | manifest under `crates/` — covered via `crates/` prefix arm |
| `crates/foo/README.md` | touched | untouched | untouched | **intentional over-classification per ADR-0033 §3 — refine later if it bothers anyone** |
| `.cargo/config.toml` | untouched | untouched | untouched | **gap flagged for discussion**: Rust-config outside `crates/` and not in §2 root-files list. Documented as `untouched` so the gap is visible; revisit if `cargo` config drift proves load-bearing |
| `packages/foo/src/index.ts` | untouched | touched | untouched | TS happy path |
| `proto/foo.proto` | untouched | untouched | touched | proto happy path |
| `docs/x.md` | untouched | untouched | untouched | docs outside any lang |
| `infra/y.yaml` | untouched | untouched | untouched | infra outside any lang |
| `scripts/test.sh` | untouched | untouched | untouched | **negative case (test §2)**: validation pipeline itself is not in any language's footprint; classifier doesn't accidentally classify our own infra as Rust |
| `scripts/lang/rust/test.sh` | untouched | untouched | untouched | **same negative case**: Rust-the-language footprint vs Rust-related-files distinction (the wrapper script is infra, not a Rust crate) |

For Wave 1, only the rust column is asserted. Wave 2 will add ts/proto assertions without rewriting the fixture. Implementation: write fixture paths to a tempdir; mock `_get_base_ref.sh` (via env override emitting fixture file list to `${DEVLOOP_TMP}/changed-files`); invoke each lang's `changed.sh`; assert exit code matches expectation.

**Failure mode (test §C)**: When a predicate disagrees with a fixture row, the meta-test prints:
- The fixture row (path + expected)
- Expected vs actual `changed.sh` exit code
- Pointer to the relevant `lang/<X>/changed.sh` line containing the predicate

Per ADR-0033 Implementation Notes: failure messages name the unhandled file path and point at the relevant `lang/<X>/changed.sh`.

### `scripts/lang/rust/test.sh` migration

Move the **body** of `scripts/test.sh` (DB bring-up, migration check, `cargo test "$@"`) into `lang/rust/test.sh`. Wrap it with the STATUS contract:
- On success: `STATUS=OK REASON=cargo-test-passed`, exit 0
- On cargo failure: `STATUS=FAIL REASON=cargo-test-failed`, exit 1
- On DB bring-up failure: `STATUS=FAIL REASON=db-bringup-failed`, exit 1

The cargo args (`--workspace`, `-p ac-service --lib`, etc.) flow through `"$@"`.

`scripts/lang/rust/changed.sh`: exactly the ADR-0033 §2 example — 4 lines, sources `_changed_helpers.sh`, checks `crates/` prefix or root cargo files.

`scripts/lang/rust/changed.test.sh`: locality self-test that uses the same fixture-injection trick as `_test_changed_predicates.sh` but only asserts Rust's predicate.

### `scripts/verify-completion.sh` refactor (paired-operations Q5)

Preserve CLI surface: `--layer {quick,standard,full}`, `--format {text,json}`, `--verbose`, `<path>`.

Mapping to layers — anchored to verb names (per paired-operations Q5 nit), so future layer renumbers don't break the runbook:

```bash
# --layer mapping (per-verb-name, not per-layer-number):
#   quick    → L1 compile, L2 format, L3 guards         (no tests/lint/audit/env-tests)
#   standard → quick + L4 test                          (no lint/audit/env-tests)
#   full     → all layers L1-L7 (canonical pipeline)    (matches scripts/layer-all.sh)
```

**`--layer full` IS `scripts/layer-all.sh`** (paired-operations Q5 emphasis): identical layer scripts run, identical exit-code semantics. Implementation: `verify-completion.sh --layer full` literally invokes `scripts/layer-all.sh` and post-processes its output for the `--format text|json` formatter. The layer loop is NOT reimplemented in two places. Runbook anchor: "verify-completion.sh --layer full IS layer-all.sh + the pretty formatter".

For `quick` and `standard`, verify-completion.sh iterates the selected `layerN.sh` scripts directly (since `layer-all.sh` always runs all 7), tees each into `${DEVLOOP_TMP}/layer-N.log`, and post-processes for text/json output.

Body shrinks to:
1. Parse args.
2. If `--layer full`: invoke `scripts/layer-all.sh` (one call); capture exit code and the machine-parseable `LAYER_SUMMARY_BEGIN/END` block.
3. Else: iterate selected layer scripts (`layer1..3.sh` for quick; `layer1..4.sh` for standard); tee each.
4. Output text (existing format: failures listed with name/message/hint) or JSON summary, parsed from STATUS lines + machine block.
5. Exit 0 if no FAIL across selected layers, 1 otherwise.

`<path>` arg becomes informational — layers run repo-wide. Documented in `--help` as deprecated/cosmetic; existing callers don't break.

### `scripts/layer-all.sh`

Incorporates paired-operations feedback (§2, §3, §4, §5):
- Truncates layer logs at start (no append accumulation across runs).
- `/tmp/devloop/` namespace distinct from ADR-0030's `/tmp/devloop-{slug}/` cluster-helper namespace; commented in `_common.sh`.
- Per-layer budget threshold from `${DEVLOOP_LAYER_BUDGET_SECS:-20}`; warn-only (no hard fail).
- `WARN BUDGET_BREACH LAYER=<n> DURATION=<s> BUDGET=<s>` greppable token on per-layer breach.
- `WARN BUDGET_TOTAL_BREACH ALWAYS_RUN_DURATION=<s> BUDGET=90` if layers 3+6 wall-clock exceeds 90s.
- Machine-parseable `LAYER_SUMMARY_BEGIN/END` block + `TOTAL_DURATION=`/`TOTAL_RESULT=` line for CI.

```bash
set -euo pipefail
source "$(dirname "$0")/lang/_common.sh"
init_devloop_tmp                                      # security §2: 700-perm cache dir

# Truncate prior run's layer logs (cross-run history belongs in main.md, not /tmp).
rm -f "$DEVLOOP_TMP"/layer-*.log "$DEVLOOP_TMP"/layer-*.stderr.log

declare -a layer_status layer_dur
budget_secs_per_layer="${DEVLOOP_LAYER_BUDGET_SECS:-20}"  # observability O6: env override
total_budget_secs=90  # ADR-0033 §4: 90s p95 wall-clock for the always-run set (layers 3 + 6)
final_exit=0          # observability O3: explicit init (don't rely on :-0 default)

for n in 1 2 3 4 5 6 7; do                           # observability O3: explicit list, not 1..7
  start=$(date +%s)
  # Observability O3: stderr append-redirect (atomic per-write, no process-sub race)
  if ! "$(dirname "$0")/layer$n.sh" \
        2>>"$DEVLOOP_TMP/layer-$n.stderr.log" \
        | tee "$DEVLOOP_TMP/layer-$n.log"; then
    final_exit=1  # continue running subsequent layers — give user full picture per layer
  fi
  end=$(date +%s); dur=$((end-start))
  # dry-reviewer (C) + observability O4: parse_status_line in _common.sh —
  # the STATUS= line-format definition lives in exactly one place (also reused by
  # verify-completion.sh). One edit if line shape ever changes.
  status=$(parse_status_line "$DEVLOOP_TMP/layer-$n.log")
  layer_status[$n]="${status:-UNKNOWN}"
  layer_dur[$n]=$dur
  if [ "$dur" -gt "$budget_secs_per_layer" ]; then
    echo "WARN BUDGET_BREACH LAYER=$n DURATION=$dur BUDGET=$budget_secs_per_layer" >&2
  fi
done

# Always-run subset budget check (layers 3 + 6 per ADR-0033 §4 footnote)
always_run_dur=$((${layer_dur[3]:-0} + ${layer_dur[6]:-0}))
if [ "$always_run_dur" -gt "$total_budget_secs" ]; then
  echo "WARN BUDGET_TOTAL_BREACH ALWAYS_RUN_DURATION=$always_run_dur BUDGET=$total_budget_secs" >&2
fi

# Machine-parseable summary block for CI
total_dur=0
total_result="OK"
for n in 1 2 3 4 5 6 7; do
  total_dur=$((total_dur + ${layer_dur[$n]:-0}))
  total_result=$(aggregate_worst_status "$total_result" "${layer_status[$n]:-UNKNOWN}")
done

printf '\n=== LAYER_SUMMARY_BEGIN ===\n'
for n in 1 2 3 4 5 6 7; do
  printf 'LAYER=%d RESULT=%s DURATION=%s\n' "$n" "${layer_status[$n]:-UNKNOWN}" "${layer_dur[$n]:-0}"
done
printf '=== LAYER_SUMMARY_END ===\n'
printf 'TOTAL_DURATION=%s TOTAL_RESULT=%s\n\n' "$total_dur" "$total_result"

# Human-readable table (stdout, after machine block — fine to keep both per paired-ops §H)
printf '%-8s %-22s %s\n' "Layer" "Status" "Duration(s)"
for n in 1 2 3 4 5 6 7; do
  printf '%-8s %-22s %s\n' "$n" "${layer_status[$n]:-UNKNOWN}" "${layer_dur[$n]:-0}"
done
exit "$final_exit"
```

Reviewer-panel cost is excluded from the budget per ADR-0033 §4 — `layer-all.sh` does not invoke reviewers.

### `BASE_REF=` emission timing (paired-operations §F)

ADR-0033 §7 normative: every invocation of `_get_base_ref.sh` emits one stderr line. Operations clarified the runbook anchor is **per-layer**, not per-run.

Decided model: each `layerN.sh` calls `_get_base_ref.sh` exactly once near the top, before invoking any wrappers. The script emits `BASE_REF=<sha> BASE_SOURCE=... DIFF_MODE=... FILES_CHANGED=...` to that layer's stderr (caught by `tee` into `layer-N.stderr.log`). The resolved sha is exported via env `DEVLOOP_BASE_REF` and the file list cached at **`${DEVLOOP_TMP}/changed-files.layer-${n}`** (per-layer namespace, paired-operations Round 3 nit) so a future parallelization of layers 3+6 (always-run pair) doesn't trample. Child wrappers (`changed.sh`, etc.) read the cache file rather than re-invoking `_get_base_ref.sh` — so emission is exactly 7 times per `layer-all.sh` run (once per layer, in each layer's stderr log). Fresh BASE_REF= per layer, no "cached" placeholder line.

Layer 3's invocation of `_test_changed_predicates.sh` runs the meta-test independently of the cache (it injects fixture file lists) — that's by design; the meta-test is hermetic.

### Layer 1 proto-first slot stub (paired-operations §D)

Per ADR-0033 §5: proto-first ordering encoded in `scripts/layer1.sh`. Wave 1 has no `lang/proto/`, so the stage-1 step is a comment + dormant block:

```bash
# scripts/layer1.sh — Compile
set -euo pipefail
source "$(dirname "$0")/lang/_common.sh"

# Stage 1: contract (proto compile/lint via lang/proto/{compile,lint}.sh).
# Wave 2 #4 lands lang/proto/ wrappers; this stage becomes meaningful then.
# Ordering discipline must be visible in source even when the work is empty —
# Wave 2 must call this stage BEFORE stage 2 (ADR-0033 §5).
if [ -d "$(dirname "$0")/lang/proto" ]; then
  "$(dirname "$0")/build.sh" --lang-only proto || stage1_exit=$?
  [ "${stage1_exit:-0}" -eq 1 ] && exit 1
fi

# Stage 2: code (rust + ts; ts wrappers land in Wave 2 #5)
"$(dirname "$0")/build.sh" --lang-not proto
```

The `--lang-only` / `--lang-not` filter is a small extension to `for_each_lang_with_verb` accepting an optional language filter. Without filters present (which is the Wave 1 case), it's a no-op argument. Wave 2 #4 starts using it.

### Behavior-equivalence test fixture (paired-operations §E + test §3)

Plan: commit a checked-in fixture rather than rely on a synthetic ad-hoc edit. Per test §3, also use a **`cargo` PATH-shim** so the test is hermetic + fast (no real cargo compile, no DB bring-up).

Files:
- `scripts/lang/rust/fixtures/equivalence-rust-only.patch` — committed unified-diff patch touching `crates/common/src/lib.rs` (e.g. benign comment line)
- `scripts/lang/rust/fixtures/cargo-shim` — committed shell shim that records its argv to `$CARGO_SHIM_ARGV_LOG` and exits 0
- `scripts/lang/rust/behavior-equivalence.test.sh` — orchestrates

`behavior-equivalence.test.sh` flow:
1. Set up tempdir; trap to clean up.
2. Verify clean working tree; `git stash --include-untracked` if not (restore in trap).
3. Apply fixture patch (`git apply scripts/lang/rust/fixtures/equivalence-rust-only.patch`).
4. Set `PATH="$(dirname "$0")/fixtures:$PATH"` so `cargo-shim` shadows real `cargo`. Set `CARGO_SHIM_ARGV_LOG=$tempdir/argv-A.log`.
5. **Test arg shape 1**: `--workspace`. Invoke OLD path (directly `scripts/lang/rust/test.sh --workspace`) → records argv to `argv-A1.log`, exit code A1. Invoke NEW path (`scripts/test.sh --workspace`) with `CARGO_SHIM_ARGV_LOG=argv-B1.log` → exit code B1. Assert A1 == B1 AND `diff argv-A1.log argv-B1.log` is empty.
6. **Test arg shape 2**: `-p ac-service --lib`. Same procedure with `argv-A2.log`/`argv-B2.log`. Assert equality.
7. Revert fixture (`git checkout -- crates/`) and pop stash.

This is hermetic (no real cargo, no DB), fast (sub-second), and tests the **dispatcher contract directly**: that the same arg vector lands at the same `cargo` invocation through both code paths.

If the cargo-shim approach proves invasive in implementation, fall back to test §3's secondary option: `cargo test --no-run` for a fast surrogate. The primary plan is the shim.

Devloop verification step section will state "verified A == B for arg shapes `--workspace` and `-p ac-service --lib`"; PR description will include the same.

### Updated `LAYER=` stderr line shape (paired-operations §1)

Confirmed format:

```
LAYER=<n> START=<unix-ts> END=<unix-ts> DURATION=<secs> RESULT=<enum> REASON=<short-no-spaces-or-dash>
```

`RESULT` enum identical to `STATUS` enum: `OK | FAIL | SKIPPED-NO-DIFF | SKIPPED-NO-VERB | N/A`. `REASON` is the worst-child wrapper's reason verbatim (or `worst-child-fail` if multiple; or `-` if RESULT=OK).

### Behavior-equivalence test (mandatory)

Replaced by the expanded "Behavior-equivalence test fixture" section above (which incorporates paired-operations §E + test §3). Net behavior: same exit code AND same recorded `cargo` argv on the same Rust-only fixture diff, before and after the refactor — proven for two arg shapes (`--workspace`; `-p ac-service --lib`).

### Additional test deliverables (test §A–§F)

Net-new test files added beyond the original brief, per test reviewer:

**§D — `_common.test.sh`** — STATUS aggregation precedence test. Encodes the precedence as a spec test that fails if anyone reorders.

**Final precedence (per code-reviewer pushback)**: `FAIL > N/A > SKIPPED-NO-DIFF > SKIPPED-NO-VERB > OK`. Conceded; original ordering had SKIPPED-NO-VERB above SKIPPED-NO-DIFF based on my "missing verb is a config error" intuition, but code-reviewer's argument is correct:

1. ADR-0033 §6 lists SKIPPED-NO-VERB as a *success* exit (exit 0). Aggregating it above OK is fine; aggregating it above SKIPPED-NO-DIFF inverts a real signal — "no relevant work + no tooling" (no-diff + no-verb) should aggregate close to OK, not be promoted to "config error" at the parent level.
2. N/A semantics are *stronger* than SKIPPED-NO-VERB: N/A is a deliberate, documented decision (e.g. layer 7 wave2-pending); SKIPPED-NO-VERB is just polyglot reality during ramp-up.
3. **Operational consequence**: a Wave 1 run with only `lang/rust/` + Rust passing should aggregate as `OK` even with SKIPPED-NO-VERB rows for absent ts/proto verbs. The original ordering would have made the Wave 1 success path emit a non-OK status by default — bad signal hygiene.

The `loud-on-missing-verb` requirement (ADR-0033 §6) is satisfied by the *per-child STATUS line surviving the stream-verbatim contract* — visibility, not aggregated promotion.

Cases:
| Inputs | Expected `aggregate_worst_status` output | Note |
|--------|------------------------------------------|------|
| `OK` + `OK` | `OK` | trivial |
| `OK` + `SKIPPED-NO-VERB` | `SKIPPED-NO-VERB` | beats OK by 1 step |
| `OK` + `SKIPPED-NO-DIFF` | `SKIPPED-NO-DIFF` | beats SKIPPED-NO-VERB |
| `SKIPPED-NO-VERB` + `SKIPPED-NO-DIFF` | `SKIPPED-NO-DIFF` | NO-DIFF wins (revised) |
| `SKIPPED-NO-DIFF` + `N/A` | `N/A` | N/A is "documented gap" |
| `N/A` + `OK` | `N/A` | |
| `N/A` + `SKIPPED-NO-VERB` | `N/A` | |
| `OK` + `FAIL` | `FAIL` | FAIL always wins |
| `FAIL` + `N/A` | `FAIL` | |

`aggregate_worst_status()` in `_common.sh` will carry a comment block above it documenting the precedence + the reasoning above so the next reviewer doesn't have to re-derive it.

This aggregation flows to the layer's stdout final STATUS line and exit code (FAIL → 1; UNKNOWN → 2 [dispatcher bug]; else → 0). SKIPPED-NO-VERB exit code stays 0 per ADR-0033 §6.

**§E — Dispatcher loud-fail-on-missing-changed.sh test** (`scripts/lang/_dispatch.test.sh`):
1. **Create a copy of `_dispatch.sh` and a synthetic `lang/` tree in a tempdir** — never mutate the live `scripts/lang/` (test reviewer hermeticity refinement).
2. Add `<tempdir>/lang/fakeland/` with no `changed.sh` (intentionally empty).
3. Invoke the copied dispatcher's `for_each_lang_with_verb "test"` against the tempdir's lang tree (override the iteration root via env var or argv — TBD in implementation; whichever is cleanest).
4. Assert non-zero exit + stderr contains `fakeland/changed.sh`.
5. Trap-cleanup the tempdir.

Loud-absence is the contract per ADR-0033 §2 + Implementation Notes; without this test, regressions slip in silently. Hermeticity rule applies to the dispatcher test itself — a flaky run leaving `fakeland/` behind in `scripts/lang/` would break subsequent runs.

**§F — Layer3-invokes-meta-test integrity check** (one-liner in `_test_changed_predicates.sh` or as a separate `scripts/lang/_layer3_invocation.test.sh`):
- `grep -q '_test_changed_predicates' scripts/layer3.sh` must succeed.
- Asserted in CI; easy to drop, easy to lose.

**§A — Hermeticity invariant** applied universally:
- No test invokes `git fetch` against a real `origin`. All tests use local `file:///tmp/...` remotes or no remote.
- No test runs `git merge-base origin/main HEAD` against `/work`. Each test runs in its own `mktemp -d` with a fresh `git init`.
- No test reads from the actual workspace state (`/work/crates/`, `/work/Cargo.toml`, etc.). Tests synthesize their own fixtures.
- No test depends on `cargo`/`pnpm`/`buf`/`sqlx` being installed beyond the shim where applicable.

**§B — `assert_base_ref_line_well_formed()` helper** lives in `_get_base_ref.test.sh` and is reused by every matrix case. Asserts presence of `BASE_REF=`, `BASE_SOURCE=`, `DIFF_MODE=`, `FILES_CHANGED=` on a single stderr line. Schema drift fails every test, never passes silently.

**§C — `_test_changed_predicates.sh` failure-message format** documented above (in the meta-test section).

### Updated cross-boundary classification (test additions)

Adding these net-new test files to the table:
| Path | Classification | Owner |
|------|----------------|-------|
| `scripts/lang/_common.test.sh` | Mine | infrastructure |
| `scripts/lang/_dispatch.test.sh` | Mine | infrastructure |
| `scripts/lang/_layer3_invocation.test.sh` (or merged into `_test_changed_predicates.sh`) | Mine | infrastructure |
| `scripts/lang/rust/fixtures/equivalence-rust-only.patch` | Mine | infrastructure |
| `scripts/lang/rust/fixtures/cargo-shim` | Mine | infrastructure |

### `_common.sh` helpers — DRY summary (dry-reviewer §1 + §3)

Single source of truth for cross-script primitives. **No `echo STATUS=...`, no `date +%s`, no exit-code arithmetic outside `_common.sh`.**

```bash
# scripts/lang/_common.sh — sourced by every layer/dispatcher/wrapper

# --- paths + permissions (security §2) ---
DEVLOOP_TMP="${DEVLOOP_TMP:-/tmp/devloop}"
init_devloop_tmp() {
  mkdir -p "$DEVLOOP_TMP"
  chmod 700 "$DEVLOOP_TMP"
}

# --- STATUS emission (dry-reviewer §3) ---
emit_status() {  # args: status reason
  printf 'STATUS=%s REASON=%s\n' "$1" "$2"
}
run_and_emit() {  # args: reason-prefix command...
  local prefix="$1"; shift
  if "$@"; then emit_status OK "$prefix-passed"; return 0
  else emit_status FAIL "$prefix-failed"; return 1; fi
}

# --- STATUS parsing (dry-reviewer (C)) ---
# Parse the LAST STATUS= line from a log file, return just the enum value.
# Args: $1=log-file
# Outputs: stdout=enum value (empty string if no STATUS= line found)
# Returns: 0 always (caller checks for empty)
# Single source of truth for STATUS= line shape — used by layer-all.sh,
# verify-completion.sh, and (in Wave 2) CI YAML's grep.
parse_status_line() {
  grep '^STATUS=' "$1" | tail -n1 | sed -n 's/^STATUS=\([^ ]*\).*/\1/p'
}

# --- STATUS aggregation (test §D) ---
# Precedence (code-reviewer locked): FAIL > N/A > SKIPPED-NO-DIFF > SKIPPED-NO-VERB > OK
# Reasoning: SKIPPED-NO-VERB is a §6 success-exit; loud visibility is via the child
# STATUS line surviving the stream-verbatim contract, not via aggregated promotion.
# N/A is a deliberate documented gap (e.g. layer 7 wave2-pending); SKIPPED-NO-DIFF
# is "world state didn't change for this lang"; SKIPPED-NO-VERB is "polyglot reality
# during ramp-up". A Wave 1 run with only lang/rust/ + Rust OK aggregates to OK, not
# SKIPPED-NO-VERB — so the success path stays clean.
aggregate_worst_status() { ... }  # impl details in implementation phase

# --- layer lifecycle (dry-reviewer §1) ---
layer_lifecycle_begin() { ... }   # captures start time, installs EXIT trap
tee_collect_statuses() { ... }    # streams stdin, side-effects __LAYER_STATUSES
__layer_lifecycle_end() { ... }   # called from EXIT trap
```

This factoring means:
- Layer scripts are 4 lines of *real* shell each (shebang + strict + source + 1 dispatch invocation through the lifecycle).
- Per-language wrappers use `run_and_emit` for trivial cargo-passthrough verbs; only `test.sh` (with DB bring-up) is more substantial.
- Adding a 4th language adds wrappers with the same `run_and_emit` 1-liner shape.
- LAYER= line shape, STATUS= enum, exit-code semantics — all live in ONE place. Drift impossible.

### Shell-script style (code-reviewer ask #7 + addenda 1-13)

Universal style enforced across every `.sh` in this devloop:

1. **Shebang** = `#!/usr/bin/env bash` (portable; `/bin/bash` differs across hosts).
2. **Strict mode header** at line 2-3:
   ```bash
   set -euo pipefail
   IFS=$'\n\t'    # prevents word-splitting surprises in `for f in $files`
   ```
3. **Source idiom** in helpers uses `${BASH_SOURCE[0]}`, not `$0` (since `$0` reflects the *caller* under `source`):
   ```bash
   source "$(dirname "${BASH_SOURCE[0]}")/_common.sh"
   ```
   The `__cached_changed_files` snippet earlier in the plan was already correct on this; layer scripts (which are exec'd) may use `$0`.
4. **Idempotent sourcing** — every helper guards against double-source:
   ```bash
   [[ -n "${__DEVLOOP_COMMON_SH:-}" ]] && return 0
   readonly __DEVLOOP_COMMON_SH=1
   ```
   Prevents re-defining `readonly` vars when both a layer and an inner wrapper source `_common.sh`.
5. **Quoting absolutism**: `"$var"` always; `"${var:-default}"` for optional; `"$@"` not `$*`; `--` separator before user-controlled paths in commands that take options (`grep -- "$pattern" "$file"`).
6. **`shellcheck -x` clean** is a Gate 2 hard requirement (the `-x` follows `source` directives — without it SC1091 noise drowns real findings).
7. **Trap discipline for the LAYER stderr line.** Already adopted via `layer_lifecycle_begin` (dry-reviewer §1 + observability O2). Trap on EXIT guarantees the stderr line emits even on `set -e` abort. Factored into `_common.sh::layer_lifecycle_begin`.
8. **Function naming**: `lower_snake_case`. Internal/private functions (sourced helpers, not for external callers) prefixed `__` (double underscore) — already applied for `__cached_changed_files`, `__layer_lifecycle_end`. Apply consistently.
9. **No top-level side effects in helpers.** Sourced files only define functions and set readonly cache vars. Caller chooses when to invoke (e.g., `init_devloop_tmp` is *defined* in `_common.sh`, but only *called* from `layer-all.sh` / standalone `layerN.sh`).
10. **`pipefail` + `PIPESTATUS` for the layer-all.sh tee construct.** The `2>>"$DEVLOOP_TMP/layer-$n.stderr.log" | tee "$DEVLOOP_TMP/layer-$n.log"` line: `set -o pipefail` propagates the layer's non-zero exit through the `tee`. Test: a unit-test for layer-all.sh injects a forced-FAIL stub layer and asserts `final_exit=1` AND that the layer log is fully captured (no truncation from short read on the failing layer's stdout). Will write this test alongside the behavior-equivalence test under `scripts/lang/_layer_all_pipefail.test.sh`.
11. **`local` declaration discipline** — separate declaration from assignment when RHS can fail under `set -e`:
    ```bash
    local base
    base=$(_get_base_ref.sh)   # exit code propagates
    # NOT: local base=$(_get_base_ref.sh)  — exit code masked by `local`
    ```
12. **Function input documentation** — every public helper in `_common.sh`/`_dispatch.sh`/`_changed_helpers.sh` gets a 2-3 line block:
    ```bash
    # Args: $1=verb-name, $@=verb-args
    # Outputs: stdout=streamed STATUS lines, stderr=BASE_REF= line
    # Returns: 0=ok|skipped, 1=fail, 2=dispatcher bug
    for_each_lang_with_verb() { ... }
    ```
    No man-page prose — just signature.
13. **DRY scaffolding via `init_layer <n>`** — already adopted as `layer_lifecycle_begin` (dry-reviewer §1). Each `layerN.sh` body = shebang + strict + source + `layer_lifecycle_begin N` + dispatch + done.

### `init_layer` / `layer_lifecycle_begin` sketch (code-reviewer end-of-message ask)

Sketched here for code-reviewer sanity-check before implementation:

```bash
# scripts/lang/_common.sh

# Begin layer lifecycle: install EXIT trap, capture start, init STATUS collector.
# Args: $1=layer-num
# Outputs (deferred to __layer_lifecycle_end via EXIT trap):
#   stdout: final STATUS=<result> REASON=<reason> line
#   stderr: LAYER=<n> START=<s> END=<e> DURATION=<d> RESULT=<r> REASON=<reason>
# Returns: 0 (sets up trap; mutates module state)
# CRITICAL: trap fires on `set -e` abort, signal, or normal exit — LAYER stderr line GUARANTEED.
layer_lifecycle_begin() {
  __LAYER_NUM="$1"
  __LAYER_START=$(date +%s)
  __LAYER_STATUSES=()
  __LAYER_RESULT="UNKNOWN"   # mid-flight abort default — distinguishable in runbook
  trap '__layer_lifecycle_end' EXIT
}

# Stream stdin → stdout verbatim AND append STATUS lines to __LAYER_STATUSES.
# Args: (none — reads stdin)
# Outputs: stdout=verbatim copy
# Returns: 0
tee_collect_statuses() {
  local line
  while IFS= read -r line; do
    printf '%s\n' "$line"
    if [[ "$line" =~ ^STATUS=([^[:space:]]+) ]]; then
      __LAYER_STATUSES+=("${BASH_REMATCH[1]}")
    fi
  done
}

# Internal: emit STATUS+LAYER lines, exit with mapped code.
# Args: (none — reads __LAYER_* module vars)
# Outputs: stdout=STATUS= line, stderr=LAYER= line
# Returns: 0=OK/SKIPPED/N/A, 1=FAIL, 2=UNKNOWN (dispatcher bug)
__layer_lifecycle_end() {
  local end duration result reason rc
  end=$(date +%s)
  duration=$((end - __LAYER_START))
  if [ "${#__LAYER_STATUSES[@]}" -eq 0 ]; then
    result="$__LAYER_RESULT"   # UNKNOWN if no children ran
  else
    result=$(aggregate_worst_status "${__LAYER_STATUSES[@]}")
  fi
  reason="layer${__LAYER_NUM}-${result,,}"   # lowercased status as reason token
  printf 'STATUS=%s REASON=%s\n' "$result" "$reason"
  printf 'LAYER=%s START=%s END=%s DURATION=%s RESULT=%s REASON=%s\n' \
    "$__LAYER_NUM" "$__LAYER_START" "$end" "$duration" "$result" "$reason" >&2
  case "$result" in
    OK|SKIPPED-NO-DIFF|SKIPPED-NO-VERB|N/A) rc=0 ;;
    FAIL)                                    rc=1 ;;
    *)                                       rc=2 ;;   # UNKNOWN → dispatcher bug
  esac
  # Exit with rc — but EXIT trap is already running, so use `exit` not `return`.
  trap - EXIT     # un-install before exiting (otherwise infinite recursion)
  exit "$rc"
}
```

Open question for code-reviewer: the `trap - EXIT; exit "$rc"` pattern — preferable to `kill -INT $$` or `_exit`? My read: `trap - EXIT` + `exit` is the idiomatic shell pattern; `_exit` requires bash 4.x with `enable -f` and isn't portable.

### Behavior-equivalence test fixture path correction (code-reviewer note)

Per code-reviewer suggestion, fixture relocates to `scripts/lang/rust/fixtures/equivalence-rust-only.patch` (already named that in plan §"Behavior-equivalence test fixture"). NOT under `tests/fixtures/devloop/...` — keeping under `scripts/lang/rust/fixtures/` because:
- Co-locates with the test that consumes it (`scripts/lang/rust/behavior-equivalence.test.sh`).
- Pattern matches Wave 2 — each language's fixtures live under that language's lang/ dir.
- `tests/` at repo root is currently a different convention (Rust integration tests).

If code-reviewer prefers `tests/fixtures/devloop/...`, can move; flagging as a small style choice.

### Cache-path correction (code-reviewer note on `__cached_changed_files`)

Code-reviewer flagged that `$DEVLOOP_TMP/changed-files.<pid>` keyed on each layer's pid would diverge across layers. Then paired-operations Round 3 noted that a single shared `changed-files` would trample under future parallel-layer execution (layers 3+6 always-run pair). **Resolution**: cache path is `$DEVLOOP_TMP/changed-files.layer-${n}` (per-layer namespace, layer number taken from `layer_lifecycle_begin`'s `__LAYER_NUM`). Cheap insurance against future-us parallelizing — no behavioral cost in Wave 1 sequential execution. Read once per layer at top; child wrappers in the same layer read the same per-layer file. Or, since `git diff --name-only` is fast on this repo, drop cross-process caching entirely and recompute per wrapper. Will benchmark in implementation; default to per-layer-namespaced cache unless we see >10% layer overhead, in which case drop to recompute-per-wrapper.

### Security hardening (security §1, §2)

**§1 — `$GITHUB_BASE_REF` shape validation.** Already incorporated above in the `_get_base_ref.sh` pseudocode: `validate_ref_name` regex check `^[A-Za-z0-9._/-]+$` runs before any `git fetch` or `git merge-base origin/...` invocation. Defense in depth against composite-action env clobber / malicious workflow injection.

**§2 — `$DEVLOOP_TMP` permissions.** `_common.sh` will lock the cache dir to mode 700 on creation:

```bash
# scripts/lang/_common.sh
# DEVLOOP_TMP — pipeline cache namespace.
# Default /tmp/devloop is distinct from ADR-0030's /tmp/devloop-{slug}/ namespace.
# 700 perms: layer logs may incidentally capture token-bearing env (a misconfigured
# cargo test printing $GITHUB_TOKEN, RUST_LOG=trace surfacing auth headers, etc.).
# Cheap to lock the dir; expensive to retrofit if a leak occurs.
DEVLOOP_TMP="${DEVLOOP_TMP:-/tmp/devloop}"
init_devloop_tmp() {
  mkdir -p "$DEVLOOP_TMP"
  chmod 700 "$DEVLOOP_TMP"
}
```

Per-file chmod unnecessary — files inherit dir-level access control. `init_devloop_tmp` called by `layer-all.sh` and (defensively) any `layerN.sh` invoked standalone before its first write.

**Rust audit.sh stub — REAL implementation (dry-reviewer §2 clarification).** Per security ask + dry-reviewer §2: "stub" means **real implementation**, not placeholder echo. Wave 1 ships real `cargo audit`; threshold/allowlist edits remain security's domain. Wrapper uses `_common.sh::run_and_emit` (per dry-reviewer §3) — no inline `echo STATUS=...`:

```bash
#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "$0")/../_common.sh"
run_and_emit "cargo-audit" cargo audit
```

Same shape for the other Rust verbs (per dry-reviewer §2 — real implementations, not echo placeholders):

```bash
# scripts/lang/rust/compile.sh
run_and_emit "cargo-check" cargo check --workspace --quiet

# scripts/lang/rust/fmt.sh
run_and_emit "cargo-fmt" cargo fmt --all -- --check

# scripts/lang/rust/lint.sh
run_and_emit "cargo-clippy" cargo clippy --workspace --all-targets -- -D warnings
```

Each is genuinely different (different cargo subcommand + flags); they're not duplicate placeholders. With `run_and_emit` doing the OK/FAIL exit-code mapping + STATUS line emission, the wrappers are 1 real line each.

`scripts/lang/rust/test.sh` follows the same pattern (with the DB bring-up logic preserved from the original `scripts/test.sh` body):

```bash
#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "$0")/../_common.sh"
# ... DB bring-up (preserved from old scripts/test.sh) ...
run_and_emit "cargo-test" cargo test "$@"
```

Argv flows through `"$@"`. Behavior-equivalence test (with cargo PATH-shim) verifies same argv lands at same invocation in old vs new path.

**`"$@"` argv discipline.** Confirmed throughout dispatcher chain: `for_each_lang_with_verb "test" "$@"` propagates with quoted-array form; `lang/rust/test.sh` ends in `exec cargo test "$@"`. Never `$@` (unquoted) or `$*`.

**Strict mode universal.** Every `.sh` file in this devloop, including `*.test.sh` files, has `set -euo pipefail` as line 2 (line 1 = shebang).

### Open questions / asks for reviewers

1. **operations** (paired): does the layer-script `LAYER=N START=... END=... RESULT=...` line shape look right? Any objection to `RESULT=` enum being identical to `STATUS=` (`OK|FAIL|SKIPPED-NO-DIFF|SKIPPED-NO-VERB|N/A`)? Or do you want a layer-result enum distinct from the status one? I lean identical for simplicity.
2. **operations** (paired): per-layer budget threshold for warn — I propose 20s default per-layer (90s/7 ≈ 13, rounded up; layer 4 test will dominate). OK, or want it externalized via env (`DEVLOOP_LAYER_BUDGET_SECS`)?
3. **test**: the meta-test fixture set above — additions you want for Rust now, or wait for ts/proto?
4. **test**: `_get_base_ref.test.sh` matrix complete? I have local-clean, local-dirty, local-with-untracked, ci-pr, ci-push, ci-push-first-commit. Missing the "PR base ref unreachable" failure-loud test — I plan to add it as a 7th case asserting non-zero exit + stderr message.
5. **security**: anything to call out on `_get_base_ref.sh` stderr emission (BASE_REF sha is git metadata, not secrets) or on `cargo test`'s args flowing through (no shell-injection concern since they're argv, not eval)?
6. **observability**: the stderr `LAYER=...` line is the structural log signal. Want me to use a different separator (e.g. `key=value` JSON-like) or is the space-separated kv shape fine?
7. **code-reviewer**: shell-script style — bash strict mode `set -euo pipefail` everywhere, `local` vars, `[[...]]` over `[...]`, no eval, 2-space indent. OK?
8. **dry-reviewer**: `_changed_helpers.sh` is the de-duplication primitive — every language's `changed.sh` should be 3–5 lines using its primitives. Anything to flag?
9. **code-reviewer/operations**: scope — should I stub the other rust verbs (`compile.sh`, `fmt.sh`, `lint.sh`, `audit.sh`) so dispatchers find verbs and emit OK rather than SKIPPED-NO-VERB on every layer? My read: stubbing them is small and makes the pipeline output more meaningful in Wave 1 (Layer 1 actually compiles, etc.). Will do unless someone objects.

### Files I will touch (Cross-Boundary Classification — all "Mine")

Already enumerated in the table above; expanded for the optional rust verb stubs:

| Path | Classification | Owner |
|------|----------------|-------|
| `scripts/layer{1..7}.sh` | Mine | infrastructure |
| `scripts/layer-all.sh` | Mine | infrastructure |
| `scripts/{audit,lint,test,fmt,build}.sh` | Mine | infrastructure |
| `scripts/lang/_{common,dispatch,changed_helpers,get_base_ref}.sh` | Mine | infrastructure |
| `scripts/lang/_get_base_ref.test.sh` | Mine | infrastructure |
| `scripts/lang/_test_changed_predicates.sh` | Mine | infrastructure |
| `scripts/lang/rust/{changed,changed.test,test}.sh` | Mine | infrastructure |
| `scripts/lang/rust/{compile,fmt,lint,audit}.sh` (optional stubs) | Mine | infrastructure |
| `scripts/lang/rust/behavior-equivalence.test.sh` | Mine | infrastructure |
| `scripts/verify-completion.sh` | Mine | infrastructure |

**Will NOT touch**: `scripts/lang/ts/`, `scripts/lang/proto/`, `.claude/skills/devloop/SKILL.md`, `.github/workflows/ci.yml`, `scripts/guards/run-guards.sh` body, layer A scope-drift parser. All Wave 2/3 scope.

No GSA paths touched. No proto/, no security-critical Rust, no DB migrations.

---

## Pre-Work

None.

---

## Implementation Summary

Landed the polyglot validation pipeline scaffolding per ADR-0033 Wave 1 #1.

**Shared helpers** (`scripts/lang/`):
- `_common.sh` — DEVLOOP_TMP (700-perm), color helpers, `emit_status`, `run_and_emit`, `parse_status_line`, `aggregate_worst_status` (precedence FAIL > N/A > SKIPPED-NO-DIFF > SKIPPED-NO-VERB > OK), `layer_lifecycle_begin` + `tee_collect_statuses` + `__layer_lifecycle_end` (EXIT-trap so LAYER stderr line emits even on `set -e` abort). `shopt -s lastpipe` enabled so `tee_collect_statuses` mutates `__LAYER_STATUSES` in the parent shell, not a subshell.
- `_get_base_ref.sh` — env-aware base-ref resolver. Validates `$GITHUB_BASE_REF` against `^[A-Za-z0-9._/-]+$` before any `git fetch`. Suppresses fetch stderr (no GITHUB_TOKEN leak). Emits canonical normative stderr line `BASE_REF=<full-40-char-sha> BASE_SOURCE=<src> DIFF_MODE=<mode> FILES_CHANGED=<count>`. Caches per-layer at `${DEVLOOP_TMP}/changed-files.layer-${DEVLOOP_LAYER}`.
- `_changed_helpers.sh` — declarative `diff_touches_path` and `diff_touches_root_files` primitives.
- `_dispatch.sh` — `for_each_lang_with_verb` iterates `lang/<X>/`, lints each has executable `changed.sh` (loud-fail on missing), runs predicate (skip-if-untouched short-circuit), invokes verb script or emits `STATUS=SKIPPED-NO-VERB`. Single-lang case: streams child STATUS verbatim (no double-emit). 2+ langs: emits aggregated final STATUS.

**Rust per-language wrappers** (`scripts/lang/rust/`):
- `changed.sh` — 4-line predicate: `crates/` prefix or root cargo files (`Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`).
- `compile.sh` / `fmt.sh` / `lint.sh` / `audit.sh` — real `cargo` invocations via `run_and_emit` (1 line each).
- `test.sh` — body migrated verbatim from old `scripts/test.sh` (DB bring-up + sqlx migrations + `cargo test "$@"`), wrapped with `run_and_emit "cargo-test" cargo test "$@"`.

**Per-verb dispatchers** (`scripts/`):
- `build.sh`, `fmt.sh`, `lint.sh`, `test.sh`, `audit.sh` — each sources `_dispatch.sh` and calls `for_each_lang_with_verb "<verb>"`. `audit.sh` sets `DEVLOOP_DISPATCH_ALWAYS_RUN=1` (no skip-if-untouched).
- `scripts/test.sh` is now a thin shim (5 lines); the original 173-line body lives at `lang/rust/test.sh`.

**Layer scripts** (`scripts/`):
- `layer1.sh` through `layer7.sh` — each is 4-7 lines using `layer_lifecycle_begin <n>` + `tee_collect_statuses`. `layer1.sh` carries the proto-first stub (Stage 1 guarded by `[[ -d lang/proto ]]`, Stage 2 fallback) per ADR-0033 §5. `layer3.sh` calls `scripts/guards/run-guards.sh` (untouched per scope) AND invokes `_test_changed_predicates.sh` per the implementation note. `layer7.sh` emits `STATUS=N/A REASON=wave2-pending`.
- `layer-all.sh` — orchestrator: cleans prior logs, iterates layers 1-7 with atomic stderr append-redirect (no process-sub race), reads STATUS via `parse_status_line`, emits per-layer `WARN BUDGET_BREACH` and aggregate `WARN BUDGET_TOTAL_BREACH` (greppable tokens), final `LAYER_SUMMARY_BEGIN/END` machine block + `TOTAL_DURATION/TOTAL_RESULT` line + human table.

**`scripts/verify-completion.sh`** — refactored. `--layer full` literally invokes `scripts/layer-all.sh` (no reimplementation); `quick`/`standard` iterate selected layer scripts. Layer mapping comment anchors to verb names (compile/format/guards/test/lint/audit/env-tests) so layer-number renumbers don't break the runbook. Preserves `--format text|json` and `--verbose`.

**Tests** (62 total assertions, all passing):
- `_common.test.sh` — 18 assertions on STATUS precedence + emit/parse primitives.
- `_get_base_ref.test.sh` — 8-case matrix (local-clean, local-dirty, local-with-untracked, local-no-mergebase, ci-pr, ci-push, ci-push-first-commit, ci-pr-base-ref-unreachable). Every case asserts BASE_REF= line shape via `assert_base_ref_line_well_formed`. Token-leak regression test: `ci-pr-base-ref-unreachable` sets `GITHUB_TOKEN=test-token-DO-NOT-LEAK` and asserts stderr contains no leak. 18 assertions.
- `_test_changed_predicates.sh` — 13 fixture rows with `# rationale:` comments; expands to ts/proto in Wave 2 without rewriting.
- `_dispatch.test.sh` — 4 assertions: dispatcher fails loud on missing `changed.sh`; single-lang case doesn't double-emit STATUS. Hermetic via tempdir copy of `_dispatch.sh` + synthetic lang tree.
- `lang/rust/changed.test.sh` — 5 locality assertions for the rust predicate.
- `lang/rust/behavior-equivalence.test.sh` — 4 assertions: same exit code AND identical recorded `cargo` argv between OLD path (`lang/rust/test.sh`) and NEW path (`scripts/test.sh`) for two arg shapes (`--workspace`; `-p ac-service --lib`). Hermetic via PATH-shim recording argv to `$CARGO_SHIM_ARGV_LOG`. Sub-second runtime.

All tests run in `mktemp -d` with synthetic git fixtures and synthetic env (no real `git fetch` against actual `origin`, no reads of /work workspace state).

**Smoke test of full pipeline** (`bash scripts/layer-all.sh`, run on this branch):
- L1, L2, L4, L5: SKIPPED-NO-DIFF (no Rust touched in branch's diff vs main — correct)
- L3: OK (24 guards pass + meta-test passes; cross-boundary table includes all the new files)
- L6: cargo audit finds 6 pre-existing vulnerabilities — pipeline correctly hard-blocks (FAIL). Out of scope for Wave 1 to fix; security owns advisory thresholds (ADR-0033 §11).
- L7: N/A (Wave 2 placeholder)
- TOTAL_RESULT correctly aggregates to FAIL due to L6 audit advisories.

---

## Files Modified

**New** (28):
- `scripts/lang/_common.sh`
- `scripts/lang/_dispatch.sh`
- `scripts/lang/_changed_helpers.sh`
- `scripts/lang/_get_base_ref.sh`
- `scripts/lang/_get_base_ref.test.sh`
- `scripts/lang/_test_changed_predicates.sh`
- `scripts/lang/_common.test.sh`
- `scripts/lang/_dispatch.test.sh`
- `scripts/lang/rust/changed.sh`
- `scripts/lang/rust/changed.test.sh`
- `scripts/lang/rust/compile.sh`
- `scripts/lang/rust/fmt.sh`
- `scripts/lang/rust/lint.sh`
- `scripts/lang/rust/audit.sh`
- `scripts/lang/rust/test.sh`
- `scripts/lang/rust/behavior-equivalence.test.sh`
- `scripts/lang/rust/fixtures/cargo-shim`
- `scripts/lang/rust/fixtures/equivalence-rust-only.patch`
- `scripts/layer1.sh`
- `scripts/layer2.sh`
- `scripts/layer3.sh`
- `scripts/layer4.sh`
- `scripts/layer5.sh`
- `scripts/layer6.sh`
- `scripts/layer7.sh`
- `scripts/layer-all.sh`
- `scripts/build.sh`
- `scripts/fmt.sh`
- `scripts/lint.sh`
- `scripts/audit.sh`

**Modified** (2):
- `scripts/test.sh` — refactored to thin dispatcher shim; old body migrated to `lang/rust/test.sh`.
- `scripts/verify-completion.sh` — refactored to call layer scripts; preserves CLI surface.

**No GSA paths touched. No proto/, no security-critical Rust, no DB migrations, no SKILL.md, no CI YAML, no `scripts/guards/run-guards.sh` body.**

---

## Devloop Verification Steps

Reproducible smoke test:

```bash
# Test suite (62 assertions, sub-second runtime).
for t in scripts/lang/_common.test.sh \
         scripts/lang/_get_base_ref.test.sh \
         scripts/lang/_test_changed_predicates.sh \
         scripts/lang/_dispatch.test.sh \
         scripts/lang/rust/changed.test.sh \
         scripts/lang/rust/behavior-equivalence.test.sh; do
  bash "$t" || { echo "FAIL: $t"; exit 1; }
done

# Full pipeline.
bash scripts/layer-all.sh

# Layer in isolation (e.g. just env-tests placeholder).
bash scripts/layer7.sh

# verify-completion.sh contract preserved.
bash scripts/verify-completion.sh --layer quick
bash scripts/verify-completion.sh --format json --layer standard
```

Behavior-equivalence proof for the `scripts/test.sh` refactor: `bash scripts/lang/rust/behavior-equivalence.test.sh` confirms `./scripts/test.sh --workspace` and `./scripts/test.sh -p ac-service --lib` produce identical exit code AND identical recorded cargo argv between old and new code paths.

`_get_base_ref.sh` direct invocation prints the resolved sha to stdout and the canonical `BASE_REF=` line to stderr — runbook anchor for "what diff did the validation actually see?".

### Gate 2 Validation (Lead, 2026-05-08)

| Layer | Command | Status | Notes |
|------:|---------|--------|-------|
| 1 | `cargo check --workspace` | PASS | Finished in 15.02s |
| 2 | `cargo fmt --all -- --check` | PASS | No format drift |
| 3 | `./scripts/guards/run-guards.sh` | PASS | 22 guards, 0 failed, 7.21s |
| 4 | `./scripts/test.sh --workspace` (via new dispatcher) | SKIPPED-NO-DIFF | Correct ADR-0033 §3 behavior — scripts-only diff, no Rust touched |
| 4* | `bash scripts/lang/rust/test.sh --workspace` (forced) | PASS | Direct invocation runs full Rust test suite; STATUS=OK REASON=cargo-test-passed |
| 5 | `cargo clippy --workspace -- -D warnings` | PASS | Finished in 5.29s |
| 6 | `cargo audit` | 6 advisories (PRE-EXISTING) | See triage below |
| 7 | env-tests | N/A | Justification: scripts-only diff, no `crates/` or `infra/` paths touched. ADR-0033 §3 classification principle: env-tests requires source change in `crates/env-tests/` or `infra/` to require re-run. None present. Build-tooling-only devloop. |
| Self-tests | 6 test files, 62 assertions | PASS | _common (18), _get_base_ref (18), _test_changed_predicates (13), _dispatch (4), rust/changed (5), rust/behavior-equivalence (4) |
| Semantic-guard | spawn-as-team-member | SAFE | All 9 checked items clean: ADR-0033 §4/§6/§7 contracts, behavior-equivalence for test.sh + verify-completion.sh (with documented intentional `exec`→`run_and_emit` semantic shift), no scope creep, no credential leakage, $GITHUB_BASE_REF shell-injection-safe, idempotent helper sources, bash-4.0 tripwire correctly placed. |
| `shellcheck -x` | tool not installed in environment | N/A | Pre-existing tool gap — not a finding against this devloop. Implementer planned shellcheck-clean as Gate 2 hard requirement; install + re-verify is a follow-up CI gap (Wave 2 scope). |

**Layer 6 audit triage**: Six advisories all on transitive dependencies (`rustls-pemfile` 1.0.4 / 2.2.0, `ring` 0.16.20, etc.) inherited via `wtransport`/`quinn`. `git diff --name-only HEAD` confirms zero `Cargo.toml`/`Cargo.lock` changes in this devloop — these advisories pre-date the work. Per ADR-0033 §11, audit-policy ownership belongs to security; security has confirmed in plan review that Wave 1 #1 explicitly does not touch audit thresholds. Findings will be carried forward to security's follow-up devloop (tracked in `docs/TODO.md`). The new Layer 6 wrapper correctly hard-blocks on these — that's the intended ADR-0033 §3 always-run audit gate behavior; the pipeline is working as designed.

**Layer 4 SKIPPED-NO-DIFF** is the correct ADR-0033 §3 behavior, NOT a regression. The new dispatcher correctly identifies that no Rust files changed in this branch, so the cargo test suite skips. Behavior-equivalence test (4/4 passed) proves that when Rust files DO change, the dispatcher's exit code and cargo argv are bit-identical to the pre-refactor `scripts/test.sh`. To prove no regression in this devloop's correctness, the Lead invoked `lang/rust/test.sh --workspace` directly — full Rust test suite passed.

---

## Code Review Results

### Security Specialist

#### Summary
Plan and code reviewed against ADR-0033 §6/§7/§11/§12 + standard shell-injection / secret-leak / loud-skip checklist. Three findings raised; all fixed in implementation. No deferrals. Carried-forward `cargo audit` advisories triaged as pre-existing tech debt under §11 ownership.

#### Findings

- **Finding 1**: `lang/rust/audit.sh` forwarded `"$@"` to `cargo audit`, allowing CLI-time bypass of advisories (`scripts/audit.sh --ignore=RUSTSEC-...`) without the silencing showing up in the layer log — a runtime bypass of ADR-0033 §11's security-owned audit-config gate. — `scripts/lang/rust/audit.sh:8`
  - **Fix**: Dropped the `"$@"` pass-through at the leaf wrapper. Implementer added a multi-line comment block documenting intent so the next reader can't innocently re-add the pass-through. Future legitimate-arg use cases require an explicit allowlist mechanism.
  - **Status**: Fixed.

- **Finding 2**: `_changed_helpers.sh::diff_touches_path` and `diff_touches_root_files` used `grep "^${prefix}"` and `grep -qx "$f"` — regex matching on what callers semantically treat as literals. Current callers are accidentally regex-safe; a future `c++` or `c#` lang directory would silently match wrong files. — `scripts/lang/_changed_helpers.sh:50,59`
  - **Fix**: Implementer adopted `awk -v p="$prefix" 'index($0, p) == 1 ...'` for prefix matching and `grep -qxF -- "$f"` for exact-match. Both helpers carry comments crediting the security finding. All 13 fixture rows in `_test_changed_predicates.sh` and 5 cases in `lang/rust/changed.test.sh` still green.
  - **Status**: Fixed.

- **Finding 3**: `_get_base_ref.sh:99` calls `__validate_ref_name "origin/main"` on a hardcoded literal — reads as dead code without a comment explaining the consistency rationale. — `scripts/lang/_get_base_ref.sh:99`
  - **Fix**: Implementer added the suggested explanatory comment.
  - **Status**: Fixed.

#### Pre-existing audit advisories (security-owned triage per ADR-0033 §11)

`cargo audit` surfaced 6 vulnerabilities + 3 unmaintained warnings on transitive deps (`wtransport 0.1.14` chain + `sqlx-mysql` Marvin Attack). Confirmed via `git log -1 -- Cargo.toml Cargo.lock` (last changed in `d918343`, predates this devloop). Triage:

- Not introduced by this devloop. Carried-forward.
- §12 status-quo applies; 14-day MTTR tripwire is the gate.
- §11 ownership: security claims follow-up triage of the wtransport upgrade chain (`rustls-webpki 0.101.7` advisories alone justify scheduling — RUSTSEC-2026-0098/0099/0104).
- NOT a blocker for this devloop's verdict; pipeline is correctly hard-blocking on the always-run audit gate, which is the §3 designed behavior.

Tracked in `## Tech Debt References` below.

#### Praise (above-and-beyond)

- `_get_base_ref.test.sh` test case 8 explicitly asserts `GITHUB_TOKEN=test-token-DO-NOT-LEAK` is NOT echoed in stderr on unreachable base ref — a regression test for future-edits accidentally adding verbose error output. Documented as "Per security §5". Above and beyond the planning ask.
- `git fetch ... 2>/dev/null` (`_get_base_ref.sh:79`) deliberately suppresses fetch error output because git fetch errors can include token-bearing remote URLs in some configurations.
- `chmod 700` on `DEVLOOP_TMP` lands with the rationale comment block requested at planning time (`_common.sh:39-43`).
- `__validate_ref_name` regex `^[A-Za-z0-9._/-]+$` applied at all CI-PR + local entry points consistently.
- Strict-mode discipline (`set -euo pipefail` line 2 of every script including all `*.test.sh` files) confirmed across all 30 new/modified shell scripts.

#### Verdict
**RESOLVED** — All findings fixed in implementation; no deferrals. Carried-forward audit advisories accepted as pre-existing under §11 ownership.

### Test Specialist

#### Summary
7 test files (post-review additions), 103 assertions, all hermetic, all pass. Strong adherence to the Gate 1 testability contract: 8-case `_get_base_ref` matrix complete (incl. token-leak regression beyond planning ask), expanded fixture set with rationale comments, behavior-equivalence via cargo PATH-shim covering `--workspace` and `-p ac-service --lib`. Implementer fixed all 3 findings (the 2 originally-deferred ones became fixed-anyway). Q9 deviation (real cargo invocations rather than echo-stub OK) accepted as an improvement on the planning-time ask (better signal hygiene; audit honestly emits FAIL on transitive vulns instead of stub-masking).

#### Findings
- **Finding 1**: Multi-lang stream-verbatim invariant untested. — `scripts/lang/_dispatch.test.sh:139` (`test_stream_verbatim_contract`).
  - **Fix**: 3 assertions exercising the 2-lang case where one child emits SKIPPED-NO-DIFF and another emits SKIPPED-NO-VERB. Confirms per-child STATUS lines stream verbatim AND aggregated final STATUS honors locked precedence. Status: Fixed.
- **Finding 2**: §F layer3-invokes-meta-test integrity check missing (regression-insurance).
  - **Fix**: Added at `scripts/lang/_layer_skeleton.test.sh:73-81`. Status: Fixed (was originally deferred-accepted; implementer applied anyway).
- **Finding 3**: `tee_collect_statuses` not directly tested.
  - **Fix**: Added at `scripts/lang/_common.test.sh:83-117` with subshell-vs-lastpipe knowledge-transfer comment block at `:88-91`. Status: Fixed (was originally deferred-accepted; applied anyway).

#### Verdict
**RESOLVED** — All findings Fixed; no deferred items remaining.

### Observability Specialist

#### Summary
All 7 observability asks (O1-O7) verified in code. No blocking findings. Two informational findings flagged for Wave 3 runbook awareness; both accepted as designed.

#### Findings
- **Finding**: Precedence reorder (NO-DIFF > NO-VERB) is a deliberate Wave 1 ramp-up trade-off. Per-wrapper STATUS=SKIPPED-NO-VERB streams verbatim to stdout (preserved at wrapper level); only the layer-summary masks it. Documented at `scripts/lang/_common.sh:113-122`. Status: Deferred (accepted) — flag for Wave 2/3 reconsider once all wrappers ship.
- **Finding**: stdout `LAYER_SUMMARY_BEGIN/END` block carries `RESULT=`+`DURATION=` only; per-layer stderr `LAYER=...` line carries all five fields. Two surfaces serve different runbook idioms. Status: Deferred (accepted) — note for Wave 3 #8 runbook author to document both surfaces.

Bonus quality items shipped beyond asks: `__validate_ref_name` defense-in-depth, "DO NOT enable trace mode" comment with security rationale at `_get_base_ref.sh:16`, bash 4.0 tripwire + idempotent-source guard, always-run-subset hard-90s budget separately enforced from per-layer warn.

#### Verdict
**CLEAR**

### Code Quality Reviewer

#### Summary
Clean implementation; 13 style addenda + 3 late nits + locked precedence all materialized in code. Runtime smoke tests confirm load-bearing pieces (EXIT trap on `set -e` abort → UNKNOWN→exit 2 with LAYER stderr line; lastpipe-based STATUS array propagation; ADR-0033 §4/§6/§7 contracts) work as specified, not just as planned. Two minor cleanup findings deferred-accepted. Test suite 7/7 green: 97 assertions across 7 files (verified before late additions; final count 103).

#### ADR Compliance
- **ADR-0033 §4 (wrapper contract + LAYER stderr emission)**: PASS. `set -euo pipefail`+`IFS=$'\n\t'` at top of every script. LAYER stderr via EXIT trap installed in `layer_lifecycle_begin`. Smoke-test confirmed trap fires on `set -e` abort. Precedence locked at `FAIL > N/A > SKIPPED-NO-DIFF > SKIPPED-NO-VERB > OK`. `_layer_skeleton.test.sh` (35→36 assertions) structurally enforces no future `layerN.sh` reintroduces direct `date +%s` / `LAYER=` echo / `STATUS=` echo / aggregate / trap calls.
- **ADR-0033 §6 (exit codes 0/1/2 + STATUS line format)**: PASS. Mapping at `_common.sh:216-220`. `emit_status` is single STATUS-line emission primitive. `parse_status_line` is single parser. `run_and_emit` wraps cargo invocations uniformly. SKIPPED-NO-DIFF on untouched, SKIPPED-NO-VERB on missing-verb, loud FAIL with stderr message naming offending path on missing changed.sh.
- **ADR-0033 §7 (base-ref resolution)**: PASS. All four detection paths encoded; untracked-file inclusion only in local mode; normative `BASE_REF=` stderr line via `__emit_base_ref_line`; full 40-char sha; security guardrails (`__validate_ref_name`, fetch stderr suppression, top-of-file warning against `set -x`); 18-case test coverage incl. unreachable-base-ref + token-leak-prevention case.
- **ADR-0033 Implementation Notes (behavior-equivalence)**: PASS. Cargo PATH-shim approach captures argv; both old and new paths invoked on identical args; argv + exit code parity asserted. Two arg shapes (`--workspace`, `-p ac-service --lib`). Hermetic via `env -i` + PATH-shim, no real cargo/podman/docker/sqlx contact.

#### Late nits (Gate 1 → implementation)
- **(a) bash ≥ 4 tripwire**: PRESENT at `_common.sh:18-22`. `shopt -s lastpipe` at `:33` is the load-bearing reason; without it, `tee_collect_statuses`'s array mutations would be lost in subshell.
- **(b) drop redundant `${result_lower}` from REASON**: APPLIED at `_common.sh:194-206`.
- **(c) UNKNOWN → exit 2 comment**: APPLIED at `_common.sh:208-215`.

#### Ownership Lens
- All 23 paths in Cross-Boundary Classification table are `Mine` (infrastructure-owned `scripts/`).
- No GSA paths touched.
- `scripts/test.sh` and `scripts/verify-completion.sh` modified (refactor); both retain external CLI contract per behavior-equivalence test + verify-completion preserves `--layer/--format/--verbose`.

#### Findings (cleanup, deferred)
- **Finding (cleanup)**: `scripts/layer1.sh:19-24` two branches that both call `build.sh` identically — Wave 2 #4 will fill the proto-first stage. Status: Deferred (accepted) — cosmetic; flag as cleanup ticket for Wave 2 #4.
- **Finding (cleanup)**: `scripts/lang/_dispatch.sh:106` uses `_ignored_rc=$rc # avoid unused-var lint`. Either validate STATUS-vs-exit-code consistency or drop the capture. Status: Deferred (accepted) — does not affect correctness; STATUS-line-as-source-of-truth is right semantics, just plumbed awkwardly.

shellcheck not installed in environment (pre-existing tool gap). Code-reviewer ran `bash -n` syntax checks (all clean), spot-checked for `[` vs `[[` and unquoted-var smells, and ran the full self-test suite (97 assertions). CI-installation of shellcheck is a Wave 1 #2 / Wave 1 #3 follow-up.

#### Verdict
**CLEAR**

### DRY Reviewer

#### Summary
Implementer landed all three plan-stage asks (lifecycle helper, real cargo wrappers, single `emit_status`/`run_and_emit`/`parse_status_line` source of truth) plus the bonus skeleton meta-test (36 assertions). Per-verb dispatchers are clean one-liners. `lang/rust/changed.sh` is 4 lines using helpers. `lang/rust/test.sh` is a faithful migration. Three true-duplication findings raised mid-review and applied; one extraction opportunity carried forward.

#### True duplication findings (fix-or-defer)
- **Finding 1**: Status→exit-code mapping triplicated across `_dispatch.sh:128`, `_dispatch.sh:136`, `_common.sh:217`. Fix: `status_to_exit_code` helper added at `_common.sh:157`, called from all three sites. **Status: Fixed**.
- **Finding 2**: `scripts/layer3.sh:14-30` reimplemented `run_and_emit` inline with two if/else `emit_status` blocks. Fix: replaced with `run_and_emit "guards" ...` + `run_and_emit "predicate-meta-test" ...` piped through `tee_collect_statuses`. Net 17→4 lines. **Status: Fixed**.
- **Finding 3**: `DEVLOOP_LAYER=N` env-var prefix duplicated 21 times across 7 layer scripts. Fix: `layer_lifecycle_begin` exports `DEVLOOP_LAYER="$1"` at `_common.sh:188`. All 7 layer scripts reduced to no `DEVLOOP_LAYER=` prefix (verified `grep -c` returns 0 for each). **Status: Fixed**.

#### Extraction opportunities (TODO.md)
- Layer-runner-loop duplicated between `layer-all.sh:32-51` and `verify-completion.sh:102-113` (~12 lines each). Wave-2-or-later refactor — both files are correct, the `--layer full` path already delegates to `layer-all.sh`, so duplication is bounded to quick/standard branch. Appended to `docs/TODO.md` § Cross-Service Duplication.

#### Verdict
**CLEAR**

### Operations Reviewer (Paired)

#### Summary
All 8 paired-design contracts and the late-closed cache-namespace nit are present in code with the locked grep-token shapes. Live-verified the `LAYER=...` stderr line emission. Bonus catches beyond asks: EXIT-trap-installed `__layer_lifecycle_end`, single-source `parse_status_line`, 700-perm cache dir, ref-name validation against env injection.

#### Pair-design notes
All 8 pair-design asks (LAYER stderr shape with DURATION+REASON, externalized budget + greppable WARN tokens, /tmp namespace + comment distinguishing from ADR-0030, log truncation + cleanup, `LAYER_SUMMARY_BEGIN/END` machine-parseable block, Layer-1 proto-first stub, behavior-equivalence committed fixture, per-layer `BASE_REF=` emission) plus late-closed cache-namespace nit landed.

#### Findings (deferred, non-blocking)
- **Finding (deferred)**: Layer 1 proto-first stub is comment-only; ordering relies on bash glob alphabetical iteration. The implementer's plan-time proposal of a `--lang-only proto` filter on the dispatcher was not implemented. Works in Wave 1 (only rust present), but Wave 2 #4 will need to either add the filter or rely on alphabet. Status: Deferred (accepted) — flag for Wave 2 #4.
- **Finding (deferred)**: `scripts/verify-completion.sh --layer full` discards per-layer durations (`layer_dur[$n]=0`); text/json output shows duration=0 for full mode. `--verbose` users lose timing signal. Status: Deferred (accepted) — cosmetic UX gap, not a contract violation. Could be fixed in Wave 3 #7.

#### Verdict
**CLEAR**

---

## Tech Debt References

**Carried-forward `cargo audit` advisories (NOT introduced by this devloop):**
6 vulnerabilities + 3 unmaintained-warnings on the `wtransport 0.1.14` transitive
chain (`quinn 0.10.2` → `quinn-proto 0.10.6` → `ring 0.16.20` + `rustls-webpki 0.101.7`)
and `sqlx-mysql` (Marvin Attack via `rsa 0.9.10`). `Cargo.toml` / `Cargo.lock`
last touched in commit `d918343` — predates this devloop. Security owns the
upgrade plan per ADR-0033 §11; ADR-0033 §12 14-day MTTR tripwire applies on a
per-advisory basis (clock starts whenever an advisory gets a usable upstream fix).

**`shellcheck -x` not yet wired into the pipeline** — Gate 2 found `shellcheck`
not installed in the environment. Tool-install + Layer 5 wiring is a follow-up
(likely lands with Wave 2 #5 alongside TS lint wrappers).

**Wave 2 task #4 (proto wrappers) lands BEFORE R-61 task #31** per ADR-0033 —
intentional wire-breaks need `buf breaking` available locally. The proto-first
stub in `scripts/layer1.sh` (Stage 1 dormant block) is structurally ready; Wave 2
#4 fills `lang/proto/{compile,format,lint,breaking}.sh` against it.

**Tech debt for security follow-ups** — `lang/rust/audit.sh` deliberately drops
`"$@"` to prevent CLI-args downgrading advisories at runtime (per security
finding 1). When the audit-policy file lands (security-owned, ADR-0033 §11),
the wrapper may need an explicit allowlist of safe flags (e.g. `--db /custom/path`)
rather than re-introducing blanket pass-through.
