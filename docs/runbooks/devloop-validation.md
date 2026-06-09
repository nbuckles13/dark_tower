# Runbook: Devloop Validation Pipeline Failures

**Pipeline**: Polyglot Validation (`scripts/layer-all.sh` + Layers 1-7)
**Owner**: Operations Team (with infrastructure pairing for wrapper-script edits)
**Last Updated**: 2026-05-14

> **Scope**: Authoritative triage for failures in the local devloop and CI validation pipeline. Documents observable behavior of `scripts/layer-all.sh`, `scripts/layer{1..7}.sh`, the per-verb dispatchers (`scripts/{audit,build,fmt,lint,test}.sh`), the per-language wrappers under `scripts/lang/<X>/`, and the shared helpers under `scripts/lang/_*.sh`.
>
> **Design spec**: ADR-0033 (`docs/decisions/adr-0033-polyglot-validation-pipeline.md`). This runbook documents *what fails and where it emits from*; the ADR documents *why the pipeline is shaped this way*.

---

## 1. Quick Triage (30 seconds)

You ran `./scripts/layer-all.sh` and it exited non-zero. Find the failing layer in three jumps:

1. **Read the final summary block** in the layer-all stdout:
   ```
   === LAYER_SUMMARY_BEGIN ===
   LAYER=1 RESULT=OK             DURATION=2
   LAYER=2 RESULT=SKIPPED-NO-DIFF DURATION=0
   LAYER=3 RESULT=OK             DURATION=4
   LAYER=4 RESULT=FAIL           DURATION=18
   ...
   === LAYER_SUMMARY_END ===
   TOTAL_DURATION=27 TOTAL_RESULT=FAIL
   ```
2. **Find the layer with `RESULT=FAIL`** (or `RESULT=UNKNOWN` — see §3).
3. **Jump to that layer's section in §6** (Layer 1 → §6.1, Layer 2 → §6.2, …). Each layer subsection terminates at the wrapper-script path so you can `cat scripts/lang/<X>/<verb>.sh` and see the failure source in seconds.

If the pipeline exited with `PRECONDITION_FAILURE:` at startup (before any layer ran), jump to §4 (two-token convention).

If you don't see a `LAYER_SUMMARY_BEGIN` block at all, the orchestrator aborted mid-flight; check the last entry in `${DEVLOOP_TMP:-/tmp/devloop}/layer-*.stderr.log` for a `LAYER=<n> … RESULT=…` line — the EXIT trap installed by `_common.sh::__layer_lifecycle_end` guarantees this stderr line emits even under `set -e` abort.

---

## 2. Pipeline Entry Points

| Entry point | Use case |
|-------------|----------|
| `./scripts/layer-all.sh` | Full validation (every layer in order). Default for Gate 2. |
| `./scripts/layerN.sh` | Re-run a single layer (e.g. `./scripts/layer4.sh` to re-run only Layer 4 on a failing diff). Each layer is independently invocable. |
| `./scripts/{audit,build,fmt,lint,test}.sh` | Per-verb dispatcher (e.g. `scripts/test.sh --workspace`). Iterates `scripts/lang/<X>/<verb>.sh` via `_dispatch.sh::for_each_lang_with_verb`. Preserves muscle-memory: `scripts/test.sh` keeps its original CLI shape. |
| `bash scripts/lang/<X>/<verb>.sh` | Direct invocation of a single language's wrapper. Bypasses the skip-if-untouched short-circuit and the dispatcher's aggregation logic — useful for isolating "is the wrapper itself broken?" from "is the dispatcher routing correctly?". |

`scripts/verify-completion.sh` is the historical entry point; post-Wave-1 it calls `scripts/layer-all.sh` for the body (router-drift between local and CI is structurally eliminated).

---

## 3. Exit-Code & STATUS Enum Reference

### Exit codes (ADR-0033 §6 wrapper contract)

| Exit | Meaning | Maps to STATUS |
|------|---------|----------------|
| **0** | PASS or SKIPPED (success-exit class) | `OK | SKIPPED-NO-DIFF | N/A | SKIPPED-NO-VERB`† |
| **1** | FAIL — work ran and detected a problem | `FAIL` |
| **2** | Wrapper/dispatcher/orchestrator bug — investigate the script itself (dispatcher misconfig; a verb wrapper that should exist is missing/non-executable; OR a pre-layer guardrail tripped, e.g. shallow CI clone) | `UNKNOWN`; `SKIPPED-NO-VERB`† with an `*-UNEXPECTED-verb-missing-or-not-executable` REASON |

† **`SKIPPED-NO-VERB` exit code is REASON-dependent (task #50, 2026-06-08).** It maps to
**exit 0** for an *intentional* gap (`<lang>-<verb>-sh-missing-or-not-executable`, e.g.
proto's deliberately-absent `test.sh`/`audit.sh`) or for `all-langs-filtered`; it maps to
**exit 2** (the wiring-fault class, alongside `UNKNOWN`) when the REASON carries the
`UNEXPECTED` marker (`<lang>-<verb>-UNEXPECTED-verb-missing-or-not-executable`) — a verb
wrapper that should exist is missing or not executable. See §7 for the
intentional-vs-unexpected split, and ADR-0033 §6 (2026-06-08 amendment) for the contract.

### `STATUS=` enum (ADR-0033 §6)

Every wrapper emits a final stdout line of the form:

```
STATUS=<enum> REASON=<short-token-no-spaces>
```

The enum values are exactly:

| STATUS | Meaning | Typical REASON examples |
|--------|---------|-------------------------|
| `OK` | Work ran cleanly | `cargo-check-passed`, `buf-build-passed`, `guards-passed` |
| `FAIL` | Work ran and detected a problem | `cargo-clippy-failed`, `buf-breaking-failed`, `predicate-meta-test-failed` |
| `SKIPPED-NO-DIFF` | `lang/<X>/changed.sh` returned 1 (lang untouched) | `<lang>-no-diff` |
| `SKIPPED-NO-VERB` (intentional gap → **exit 0**) | `lang/<X>/<verb>.sh` is a documented intentional gap; the dispatcher records it rather than silently skipping | `proto-test-sh-missing-or-not-executable`, `proto-audit-sh-missing-or-not-executable` |
| `SKIPPED-NO-VERB` (UNEXPECTED → **exit 2**) | a verb wrapper that SHOULD exist is missing/non-executable (deleted, `chmod`-stripped, or a new lang dir missing a verb) — a wiring fault, not a benign skip | `rust-test-UNEXPECTED-verb-missing-or-not-executable`, `ts-audit-UNEXPECTED-verb-missing-or-not-executable` |
| `N/A` | Documented gap (e.g. `layer7.sh` `wave2-pending`) | `wave2-pending`, `no-languages-registered`, `<verb>-aggregate-na` |

The two `SKIPPED-NO-VERB` rows above share the `missing-or-not-executable` suffix but are
distinguished by the `UNEXPECTED` infix: grep `missing-or-not-executable` to catch BOTH
flavors, grep `UNEXPECTED` to isolate only the bug case (task #50).

`UNKNOWN` is **not** a wrapper-emitted enum — it appears in the aggregator when a child wrapper crashes before emitting `STATUS=` AND its EXIT trap did not fire (e.g. killed `-9`), or when stdout streaming breaks. Since task #50 the 14 verb wrappers self-emit `STATUS=FAIL REASON=wrapper-aborted-early-exit-<rc>` via an EXIT trap when they abort before emitting, so a true `UNKNOWN` is now genuinely exceptional. `UNKNOWN` ranks above `FAIL` in the precedence ladder because it signals a dispatcher/wrapper bug, not a real-work failure; it maps to exit 2.

### Worst-child STATUS aggregation

Each layer collects every child `STATUS=` line that came across stdout via `_common.sh::tee_collect_statuses`, then aggregates with `_common.sh::aggregate_worst_status` using the rank:

```
SKIPPED-NO-VERB (0)  <  SKIPPED-NO-DIFF (1)  <  OK (2)  <  N/A (3)  <  FAIL (4)  <  UNKNOWN (5)
```

The intuition (locked in ADR-0033 §1 by the comment block above `_common.sh::aggregate_worst_status`): *"if any child did real work and passed, the layer passed; otherwise the SKIPPED-\* state is informative. N/A propagates above OK because it signals 'this verb is not yet wired' — distinct from 'ran cleanly'. UNKNOWN ranks above FAIL — surface dispatcher bugs loud, not silent."*

**Worked example — Layer 1 stage-2 (multi-lang)**:

```
STATUS=OK REASON=cargo-check-passed         (rust)
STATUS=SKIPPED-NO-DIFF REASON=ts-no-diff    (ts)
STATUS=FAIL REASON=buf-build-failed         (proto, stage 1)
→ aggregate_worst_status OK SKIPPED-NO-DIFF FAIL = FAIL
→ Layer 1 final STATUS=FAIL REASON=layer1-summary
→ exit code 1 (status_to_exit_code FAIL)
```

### `LAYER=…` stderr summary line

Every layer emits (via the EXIT trap installed by `_common.sh::layer_lifecycle_begin`):

```
LAYER=<n> START=<unix-ts> END=<unix-ts> DURATION=<s> RESULT=<enum> REASON=<reason>
```

This is the **layer-level anchor** for greppable triage in `${DEVLOOP_TMP:-/tmp/devloop}/layer-<n>.stderr.log`. EXIT-trap emission is guaranteed even under `set -e` abort or signal-kill — a runbook reader who hits "the orchestrator died mid-layer" still sees the partial layer state in stderr.

**REASON field — stderr carries the cause, stdout carries the summary (task #50).** The
`REASON=` on THIS stderr `LAYER=` line is the **worst-child attributable cause** — e.g.
`<lang>-<verb>-UNEXPECTED-verb-missing-or-not-executable`, `wrapper-aborted-early-exit-<rc>`,
`buf-build-failed` — NOT the generic `layer<n>-summary`. The matching **stdout** `STATUS=`
summary line keeps `REASON=layer<n>-summary` (a stable machine-parseable token that
`layer-all.sh` / `verify-completion.sh` read for the enum only). So a non-zero layer names
WHY on stderr for the on-call, while stdout stays a stable summary for parsers. If you
grepped `REASON=layer<n>-summary` on stderr before task #50 and now see a specific cause,
that's the intended improvement, not format drift or a parser bug.

### Per-layer & total budget warnings (`layer-all.sh`)

Budget targets (ADR-0033 §4): **90-second p95 wall-clock for the always-run subset (layers 3 + 6)** + a soft per-layer warn threshold of 20s. Two greppable warn tokens on stderr (paired-operations §2):

```
WARN BUDGET_BREACH LAYER=<n> DURATION=<s> BUDGET=<s>           (per-layer; default budget 20s)
WARN BUDGET_TOTAL_BREACH ALWAYS_RUN_DURATION=<s> BUDGET=90     (always-run subset, layers 3 + 6; ADR-0033 §4 budget)
```

`WARN BUDGET_*` is informational only — it does not change exit code. A breach is the signal to revisit budgets (ADR-0033 §4 budget target; §14 flake-rate budget for adjacent context) or investigate a regression.

---

## 4. The Two-Token Convention (`ERROR:` vs `PRECONDITION_FAILURE:`)

Operational triage hinges on a one-grep distinction: did the resolver fail (`ERROR:`) or did a pipeline precondition fail (`PRECONDITION_FAILURE:`)?

### `ERROR:` — In-resolver emissions

Emitted only by `scripts/lang/_get_base_ref.sh`. Indicates the resolver could not produce a usable `BASE_REF`:

| Line | Emission | Cause |
|------|----------|-------|
| `_get_base_ref.sh::__validate_ref_name` | `ERROR: ref name contains unexpected characters: <ref>` | env-injection defense-in-depth (ADR-0033 §7 security) |
| `_get_base_ref.sh::main` — the `ERROR: could not compute merge-base` emission (CI-PR branch) | `ERROR: could not compute merge-base for GITHUB_BASE_REF=<ref>` | CI-PR `git merge-base` failed (ref unreachable / corrupt pack / shallow clone) |
| `_get_base_ref.sh::main` — the `ERROR: could not resolve base ref to sha` emission | `ERROR: could not resolve base ref to sha: <ref>` | ref resolved but commit unreachable in the local pack |

All three sites `exit 2` after emitting.

### `PRECONDITION_FAILURE:` — Pre-layer guardrail emissions

Emitted only by `scripts/layer-all.sh` — the `PRECONDITION_FAILURE:` emission near the top of the script (no enclosing function). Indicates a precondition for the layer pipeline is not met:

```
PRECONDITION_FAILURE: merge-base(<ref>, HEAD) unreachable — CI clone too shallow.

Fix: set actions/checkout fetch-depth: 0 in .github/workflows/ci.yml
See docs/runbooks/devloop-validation.md (this file).
```

Mode-dispatched (per task #42):
- **CI-PR**: checks `origin/$GITHUB_BASE_REF`.
- **Local**: checks `origin/main`.
- **CI-push**: skips (resolver uses `HEAD~1`, no remote pack lookup).

`layer-all.sh` exits 2 after emitting. The mode dispatch mirrors `_get_base_ref.sh`'s resolution branches — adding a third long-lived branch elsewhere requires zero change here.

### Greppable in one pass

```bash
grep -E '^(ERROR|PRECONDITION_FAILURE):' "${DEVLOOP_TMP:-/tmp/devloop}"/layer-*.stderr.log
```

### Convention extends to future precondition checks

Future precondition checks added to `layer-all.sh` (disk-space, env-var presence, container-runtime availability, etc.) inherit the `PRECONDITION_FAILURE:` token. This runbook is the canonical home for the convention; task #42 §Tech Debt Pointers entry 4 is the source.

---

## 5. `_get_base_ref.sh` Troubleshooting Playbook

### The runbook anchor: `BASE_REF=…` stderr line

Every invocation of `_get_base_ref.sh` emits exactly one stderr line of the form (ADR-0033 §7 normative requirement, emitted by `_get_base_ref.sh::__emit_base_ref_line`):

```
BASE_REF=<40-char-sha> BASE_SOURCE=<source> DIFF_MODE=<mode> FILES_CHANGED=<count>
```

This is the **runbook anchor**: every layer log carries one such line, so "what diff did the validation actually see?" is greppable.

### Token meanings

| Token | Values | Meaning |
|-------|--------|---------|
| `BASE_REF` | 40-char SHA | The resolved base commit. Always a full SHA (never a symbolic ref). |
| `BASE_SOURCE` | `local-mergebase` | Local devloop, `git merge-base origin/main HEAD` succeeded. |
|              | `local-no-mergebase` | Local devloop, no reachable `origin/main` — fell back to `HEAD` (over-classifies all files as changed; correct conservative behavior). |
|              | `ci-pr` | CI on a pull_request event — `git merge-base origin/$GITHUB_BASE_REF HEAD` (post-task-#42: merge-base, not main tip). |
|              | `ci-push-main` | CI push to a long-lived branch — `HEAD~1`. |
|              | `ci-push-first-commit` | CI push, `HEAD~1` does not exist (first commit on branch) — fell back to `HEAD`. |
| `DIFF_MODE` | `two-dot` | Always `two-dot` post-task-#42 (CI-PR previously was `three-dot`; collapsed to two-dot under uniform merge-base resolution). |
| `FILES_CHANGED` | integer ≥ 0 | Count of paths in `${DEVLOOP_TMP}/changed-files.layer-<n>` (committed + staged + unstaged + untracked in local mode; committed-only in CI). |

### Common failure modes (anchored at the canonical stderr line)

- **`FILES_CHANGED=0` but you expected a diff** → Check `BASE_SOURCE`:
  - `local-no-mergebase` means your local clone has no reachable `origin/main`. Fix: `git fetch origin main`.
  - `ci-push-first-commit` means you're on the first commit of a branch (expected; everything classifies as "touched" via `HEAD` fallback).
  - Any other source: inspect `${DEVLOOP_TMP:-/tmp/devloop}/changed-files.layer-<n>` and compare against `git diff --name-only $(./scripts/lang/_get_base_ref.sh)`.

- **`BASE_REF=` missing entirely from a layer's stderr log** → the resolver did not run. Layer wrapper bug — escalate (the canonical layer-script shape calls `_get_base_ref.sh >/dev/null` immediately after `layer_lifecycle_begin`; a missing call is a regression).

- **`PRECONDITION_FAILURE: merge-base(<ref>, HEAD) unreachable` at `layer-all.sh` startup** → CI shallow clone. Fix: `actions/checkout@v4` with `fetch-depth: 0` in `.github/workflows/ci.yml`. The guardrail catches both ref-missing (empty/wrong clone) AND merge-base-outside-depth-window (depth-N shallow case).

- **`ERROR:` tokens from the resolver** (canonical token meaning + emission sites in §4): resolver-specific remediation only.
  - `ref name contains unexpected characters` → env-injection attempt OR malformed `$GITHUB_BASE_REF`.
  - `could not compute merge-base` → same remediation as the `PRECONDITION_FAILURE:` case above (CI fetch-depth 0).
  - `could not resolve base ref to sha` → force-pushed base branch or local-pack corruption.

### CI-PR scope shift (post-task-#42)

Post-task-#42, `BASE_REF` in CI-PR mode is **`merge-base(origin/$GITHUB_BASE_REF, HEAD)`**, NOT base-branch tip. This narrows what every diff-aware guard sees — semantically asks "what did this PR add?" instead of "what is in main + this PR?". Operators or dashboards that previously assumed base-tip semantics will see scope narrowing. ADR-0033 §7 + `docs/devloop-outputs/2026-05-13-base-ref-unification-task42/main.md` §Security explain why this is correctness-preserving.

### Diagnosing predicate-vs-resolver disagreement

The resolver writes `${DEVLOOP_TMP}/changed-files.layer-<n>` — the cache-write block in `_get_base_ref.sh::main`; per-language `lang/<X>/changed.sh` predicates read it via `_changed_helpers.sh::__changed_files` (which lazy-invokes the resolver if the cache is missing).

To inspect what a layer actually saw:

```bash
cat "${DEVLOOP_TMP:-/tmp/devloop}/changed-files.layer-<n>"
```

To re-run the resolver and a single predicate hermetically:

```bash
DEVLOOP_LAYER=manual bash scripts/lang/_get_base_ref.sh >/dev/null     # populates cache + emits BASE_REF= line
DEVLOOP_LAYER=manual bash scripts/lang/rust/changed.sh; echo "rc=$?"   # 0 = lang IS affected; 1 = lang is untouched
```

(See §7 for the reversed-from-typical-shell exit-code convention on predicates.)

### Known cost concern (informational)

`_get_base_ref.sh` runs on every layer-entry AND on every guard that calls `get_diff_base` (the 1-line forwarder in `scripts/guards/common.sh`). Total invocations per pipeline run: ~24-36; CPU cost ~1.2-1.8s. Each invocation re-emits the `BASE_REF=` stderr line — observability dashboards that count "pipeline runs" by `BASE_REF=` emission will multiply runs by ~30×. Mitigation is tracked but not yet implemented (cache + `__emit_base_ref_line` suppression-sentinel). See `docs/devloop-outputs/2026-05-13-base-ref-unification-task42/main.md` §Tech Debt Pointers entry 2.

---

## 6. Layer-by-Layer Failure Modes

Each subsection covers one layer: what it runs, its always-run / skip-if-untouched character, common failure modes (each anchored at the emitting wrapper script + the REASON token), and the canonical fix vocabulary.

### 6.1 Layer 1 — Compile (`scripts/layer1.sh`)

Two-stage compile (ADR-0033 §5):
- **Stage 1**: proto-only via `scripts/build.sh` with `DEVLOOP_DISPATCH_INCLUDE_LANGS=proto` → `lang/proto/compile.sh` (`buf build proto`). Runs first so contract failures surface ahead of Rust/TS type-error cascades.
- **Stage 2**: rust + ts via `DEVLOOP_DISPATCH_EXCLUDE_LANGS=proto` → `lang/rust/compile.sh` (`cargo check --workspace`) + `lang/ts/compile.sh` (`nx affected -t typecheck`).

Both stages route through the dispatcher (`scripts/build.sh` → `_dispatch.sh::for_each_lang_with_verb "compile"`) so changed.sh short-circuit, STATUS aggregation, and missing-verb signalling apply uniformly.

**Skip-if-untouched**: rust, ts, proto (per ADR-0033 §3).

**Common failures**:

| REASON token | Wrapper | Cause / Fix |
|--------------|---------|-------------|
| `buf-build-failed` | `lang/proto/compile.sh` | Malformed `.proto`. Run `buf build proto` locally; the error names the file + line. |
| `buf-binary-missing` | `lang/proto/compile.sh` (also `fmt.sh`, `lint.sh`, `breaking.sh`) | `buf` CLI not installed locally. Install via the project's documented setup; CI has it baked into the runner image. |
| `cargo-check-failed` | `lang/rust/compile.sh` | Type / borrow / use error. Run `cargo check --workspace` locally for the full error chain. |
| `nx-typecheck-failed` | `lang/ts/compile.sh` | `tsc --noEmit` error reported via `nx affected -t typecheck`. Run `pnpm exec nx affected -t typecheck --base=$(./scripts/lang/_get_base_ref.sh)` locally. |
| `nx: command not found` | `lang/ts/compile.sh` (also Layer 2 / 4 / 5 TS wrappers) | Local-only failure mode — CI has `corepack` / `pnpm install` in setup. Fix: `pnpm install` from repo root (nx is a project-local dev dep, not a global tool). |

**Worked example — proto fail + rust untouched + ts untouched**:

```
STATUS=FAIL REASON=buf-build-failed             (proto, stage 1; per-child stdout)
STATUS=SKIPPED-NO-DIFF REASON=rust-no-diff      (stage 2, rust untouched)
STATUS=SKIPPED-NO-DIFF REASON=ts-no-diff        (stage 2, ts untouched)
STATUS=FAIL REASON=layer1-summary               (aggregated stdout summary line)
LAYER=1 ... RESULT=FAIL REASON=buf-build-failed (stderr anchor; worst-child cause)
```

Two distinct final lines (task #50): the **stdout** `STATUS=` summary keeps the generic
`REASON=layer<n>-summary` (downstream parsers read it for the enum only); the **stderr**
`LAYER=…RESULT=…REASON=` anchor now carries the WORST-CHILD reason (`buf-build-failed`
here), not `layer<n>-summary` — so a non-zero layer names its real cause for 3am triage
(observability P2). The stage-2 dispatch runs unconditionally even on stage-1 fail —
observability O2 (one run reveals the full picture; don't force a second invocation).

### 6.2 Layer 2 — Format (`scripts/layer2.sh`)

`scripts/fmt.sh` → `for_each_lang_with_verb "fmt"` → `lang/{rust,ts,proto}/fmt.sh`.

**Skip-if-untouched**: rust, ts, proto.

| REASON token | Wrapper | Cause / Fix |
|--------------|---------|-------------|
| `cargo-fmt-failed` | `lang/rust/fmt.sh` (`cargo fmt --all -- --check`) | Format drift. Fix: `cargo fmt --all` locally. The check-only wrapper never reformats. |
| `nx-format-failed` | `lang/ts/fmt.sh` (`nx affected -t format`) | Prettier drift. Fix per nx project's documented `format:write` target. |
| `buf-format-failed` | `lang/proto/fmt.sh` (`buf format --diff --exit-code proto`) | Proto format drift. Run `buf format -w proto` to fix. |
| `buf-binary-missing` | `lang/proto/fmt.sh` | See §6.1. |

### 6.3 Layer 3 — Guards (always-run; `scripts/layer3.sh`)

Three `run_and_emit` invocations:
- `scripts/guards/run-guards.sh` — iterates every `scripts/guards/simple/**/*.sh` (excluding `fixtures/`). Each guard self-classifies per-file via path globs. Includes the Layer A scope-drift parser and Layer B classification-sanity guards (ADR-0024 cross-boundary), AND the always-run **audit-suppressions** guard (`scripts/guards/simple/audit-suppressions.sh` → `scripts/audit-suppressions-check.sh`, read-only) per task #47.
- `scripts/lang/_test_changed_predicates.sh` — meta-test for each lang's `changed.sh` predicate. Hermetic — synthesizes a cache under `mktemp`, invokes each lang's predicate against fixture rows under `env -i`.
- `scripts/audit-suppressions-check.test.sh` — self-test for the suppressions-check (drives its FAIL branches with fixtures; wired here because there is no `*.test.sh` auto-runner). Task #47.

Layer 3 also carries a **CI-sentinel-leak runtime assertion** (mirrored in `layer-all.sh`): if `GITHUB_ACTIONS` and `DEVLOOP_TEST` are both set, the layer hard-fails early — see `test-sentinel-set-in-ci` below.

**Always-run**: yes — guards self-classify, predicate meta-test is hermetic. Layer 3 is one of the two layers (with Layer 6) inside the **90s p95 always-run wall-clock budget (ADR-0033 §4)** — a sustained `WARN BUDGET_TOTAL_BREACH` here is the operational signal to investigate.

| REASON token | Origin | Cause / Fix |
|--------------|--------|-------------|
| `guards-failed` | `scripts/guards/run-guards.sh` (via `run_and_emit`) | A specific guard tripped. The runner prints `FAILED: <guard-name>` + grep-extracted violation lines (`VIOLATION|violation|ERROR|error`). Jump to that guard's source under `scripts/guards/simple/`. |
| `predicate-meta-test-failed` | `scripts/lang/_test_changed_predicates.sh` | A `lang/<X>/changed.sh` predicate disagrees with its fixture row. Output prints `[<lang>] path=… expected_rc=… actual_rc=… rationale: …  see: scripts/lang/<lang>/changed.sh`. Fix by correcting the predicate OR amending the fixture (with rationale). See §7 for drift-detection workflow. |
| `suppression-past-due` | `scripts/audit-suppressions-check.sh` (Layer-3 guard) | A suppression in `audit-suppressions.toml` is past its `expires`. **This red is INTENTIONAL** — CI goes red on the expiry day by design, not an outage/flake. Output names each past-due id + its days-past. **Action is NOT bypass:** renew the `expires` after re-verifying the justification (e.g. `cargo tree -p rsa --invert` for RUSTSEC-2023-0071), OR fix the advisory. Renewal procedure: `docs/contributor/audit-suppressions.md`. (Red-on-expiry is the FIRST signal — no warn-ahead window — so renew proactively per the contributor-doc cadence.) |
| `suppression-drift` | `scripts/audit-suppressions-check.sh` (sync-check) | The generated derived files (`.cargo/audit.toml` / `.pnpm-audit-ignore.json`) drifted from `audit-suppressions.toml` (hand-edited derived file, or a forgotten `--fix`). Fix: `scripts/audit-suppressions-check.sh --fix`, then commit BOTH the manifest and the regenerated derived files. |
| `suppression-malformed` | `scripts/audit-suppressions-check.sh` (parser) | The manifest (or a derived file) is present but unparseable — missing required field, bad `expires`, duplicate id, etc. The `MALFORMED:` lines name the offending line/field. Fix the manifest; never degrade a malformed entry into "0 suppressions". |
| `suppression-quality` | `scripts/audit-suppressions-check.sh` (quality-check) | An entry has an empty `reason`/`ticket`, a non-`YYYY-MM-DD` `expires`, or an `ecosystem` outside {rust, js}. Output names the offending id + field. |
| `suppression-override-without-test-sentinel` | `scripts/audit-suppressions-check.sh` (trust-boundary guard) | A test-injection override env (`DEVLOOP_SUPPRESSIONS_MANIFEST` / `AUDIT_SUPPRESSIONS_NOW` / derived-path overrides) is set but `DEVLOOP_TEST` is not exactly `"1"`. **Tamper / misconfig signal** — a non-test environment set an override that would redirect the check. INVESTIGATE what set the env (CI step, reusable action); do NOT just unset-and-rerun. |
| `test-sentinel-set-in-ci` | `scripts/layer-all.sh` / `scripts/layer3.sh` (CI-leak assertion) | `DEVLOOP_TEST` is set in a CI job (`GITHUB_ACTIONS=true`). The test sentinel must NEVER be set in CI — it would let the always-run check honor ambient override envs repo-wide. **Pipeline-integrity incident** — find what exported `DEVLOOP_TEST` (workflow step, reusable action) and remove it; do NOT unset-and-rerun blindly. |
| `dependabot-ignore-present` | `scripts/audit-suppressions-check.sh` (SSOT-integrity check) | `.github/dependabot.yml` has a non-empty `ignore:` block — a shadow suppression surface that fragments the single source of truth. **Dependabot `ignore:` is not a suppression channel** — remove it; if an advisory genuinely needs suppressing, add it to `audit-suppressions.toml` (reviewed PR, ADR-0033 §11). Dependabot is for bump PRs only. |

**Two distinct suppression-drift surfaces — on-call note.** Advisory problems surface in TWO places, and they live in different runbook sections:
- **Per-PR expiry / hygiene** → Layer-3 `suppression-past-due` / `-drift` / `-malformed` / `-quality` red run (this section). Fires on every devloop + CI run.
- **Scheduled-scan drift** (a NEW advisory against an UNCHANGED lockfile) → a red `Scheduled Audit` workflow run + an open `audit-drift` GitHub Issue (auto-closes when a later scheduled run is clean). See §6.6 (Layer 6 / scheduled scan). On-call should check the `audit-drift` issue, not just CI, for between-PR drift.

**Suppression renewal exception (security-reviewed wording — ADR-0033 §11 ownership boundary).** The audit-config ownership reminder (§6.6 below) says operators should not modify suppressions during failure triage and should escalate to security. There is ONE sanctioned exception:

> **Exception — suppression renewal on expiry:** when a Layer-3 `suppression-past-due` failure fires, editing `audit-suppressions.toml` to renew the `expires` date (then `scripts/audit-suppressions-check.sh --fix` to regenerate derived files) IS the sanctioned remediation — NOT the prohibited ad-hoc allowlist edit. The prohibition targets SILENT, incident-time suppression of a LIVE advisory via CLI flags or hand-edited derived files. Renewal flows through the tracked manifest, regenerates derived files deterministically, and lands as a reviewed commit. Security ownership (ADR-0033 §11) is preserved: the renewed `reason`/`expires` MUST go through normal PR review — security reviews the renewal justification (e.g. the re-run `cargo tree -p rsa --invert` build-time-only re-verification for RUSTSEC-2023-0071). An operator MUST NOT extend an `expires` date as part of live incident triage to make CI green; that remains prohibited and escalates to security.

Load-bearing distinction: RENEWAL = reviewed manifest commit with re-verified justification (sanctioned); AD-HOC SILENCING = CLI flag / hand-edited derived file / unreviewed expires-bump-to-unblock-CI / Dependabot alert dismissal (prohibited, escalate).

#### 6.3.1 `dt-guard` triage (ADR-0034 §10 Wave 3)

Eight of the simple guards (cite-no-line-numbers / cite-symbol-resolves / alert-rules-policy / dashboard-panels / metric-labels / application-metrics / infrastructure-metrics / grafana-datasources) are ≤5-line shell wrappers around the Rust binary `target/release/dt-guard`. The wrapper resolves the binary path, asserts it is executable, and `exec`s `dt-guard <subcommand> --root "$REPO_ROOT"`. There are three distinct failure shapes:

1. **Stale or missing binary** — `STATUS=FAIL REASON=dt-guard-binary-missing`. The wrapper exits 1 before invoking any subcommand because `target/release/dt-guard` is not present (or not `-x`).
    - **Diagnostic**: `ls -la target/release/dt-guard`.
    - **Resolution**: `cargo build --release -p dt-guard` (or re-run `scripts/layer1.sh`, which builds it as part of `compile.sh`). The wrapper produces no `VIOLATION:` lines because the policy kernel never runs.

2. **Subcommand not found** — clap exits non-zero with its own diagnostic on stderr (typically `error: unrecognized subcommand <foo>`). `STATUS=` may surface as `clap-error` or omit entirely depending on which subcommand the wrapper invoked; the canonical signal is the clap-formatted stderr line.
    - **Diagnostic**: `dt-guard --help` to list registered subcommands.
    - **Resolution**: typo in the wrapper, or a subcommand-rollout-not-yet-landed across two PRs. Re-build to pick up newly registered subcommands.

3. **Bona-fide policy violation** — `STATUS=FAIL REASON=<policy-token>` after the subcommand runs to completion, paired with one or more `VIOLATION: <path>:<line> — <rule_id> — <message>` lines on stdout.
    - **Diagnostic**: re-run the subcommand with `--explain` for a single-line `EXPLAIN:` record per finding (span + policy + source location). Example: `target/release/dt-guard alert-rules-policy --root . --explain`.
    - **Resolution**: fix the input file at the cited path:line, or — if a true false positive — add `# guard:ignore(<reason>)` per the inline guidance in each subcommand's source-doc header. `<reason>` must be ≥10 characters and not match the `LAZY_REASON_RE` vocabulary denylist.

dt-guard also emits `WARN dt-guard auxiliary skip: <path> (<error-kind>)` to stderr when its auxiliary index-scan loops swallow an IO/parse failure (per ADR-0034 §F-SG-2 mitigation). `run-guards.sh` surfaces those WARN lines alongside `VIOLATION` / `ERROR` in non-verbose CI logs — a sudden uptick indicates a corrupted catalog or dashboard file that the policy kernel skipped silently.

### 6.4 Layer 4 — Test (`scripts/layer4.sh`)

`scripts/test.sh` → `for_each_lang_with_verb "test"` → `lang/rust/test.sh` + `lang/ts/test.sh`. Proto has no `test.sh` — dispatcher emits `STATUS=SKIPPED-NO-VERB REASON=proto-test-sh-missing-or-not-executable` (informative, expected).

**Skip-if-untouched**: rust, ts. Proto is naturally skipped via verb-discovery.

| REASON token | Wrapper | Cause / Fix |
|--------------|---------|-------------|
| `cargo-test-failed` | `lang/rust/test.sh` (`cargo test "$@"`) | A test failed. The wrapper brings up the test postgres container (podman / docker), applies pending sqlx migrations, then runs `cargo test`. Failure output is the cargo test stderr — fix the test. |
| `wrapper-aborted-early-exit-<rc>` (runtime missing) | `lang/rust/test.sh:detect_runtime` | `Neither podman nor docker found. Please install one.` — install a container runtime. The wrapper aborts before reaching `run_and_emit`; since task #50 its EXIT trap emits `STATUS=FAIL REASON=wrapper-aborted-early-exit-<rc>` so the layer aggregates **FAIL** (not the old silent `UNKNOWN`). `<rc>` is the abort code. |
| `wrapper-aborted-early-exit-<rc>` (db-bringup failure) | `lang/rust/test.sh:wait_for_db` / `run_migrations_if_needed` | `Database did not become ready within ${MAX_WAIT_SECONDS}s` (or a migration failure) — container started but pg never accepted connections. Same EXIT-trap path: STATUS=FAIL emitted before exit. Check the test container logs. |
| `nx-test-failed` | `lang/ts/test.sh` (`nx affected -t test:unit test:component`) | A TS unit/component test failed. Run the offending project's test target locally. |
| `proto-test-sh-missing-or-not-executable` | `_dispatch.sh::for_each_lang_with_verb` | Expected — proto has no `test.sh` per ADR-0033 §1. SKIPPED-NO-VERB ranks below OK, so a co-running OK lang dominates. |

### 6.5 Layer 5 — Lint (`scripts/layer5.sh`)

`scripts/lint.sh` → `for_each_lang_with_verb "lint"` → `lang/{rust,ts,proto}/lint.sh`.

**Skip-if-untouched**: rust, ts, proto.

| REASON token | Wrapper | Cause / Fix |
|--------------|---------|-------------|
| `cargo-clippy-failed` | `lang/rust/lint.sh` (`cargo clippy --workspace --all-targets -- -D warnings`) | Clippy lint or warning (warnings are denied — `-D warnings`). Run `cargo clippy --workspace --all-targets` locally for the full output. |
| `nx-lint-failed` | `lang/ts/lint.sh` (`nx affected -t lint`) | TS lint (eslint) violation. Run the project's lint target locally. |
| `buf-lint-failed` | `lang/proto/lint.sh` (`buf lint proto`) | Proto STANDARD-lint violation. Inspect output for the file + finding; `proto/buf.yaml` controls policy. |
| `buf-binary-missing` | `lang/proto/lint.sh` | See §6.1. |

### 6.6 Layer 6 — Audit (dep-change-gated as of task #47; `scripts/layer6.sh`)

`scripts/audit.sh` is an **orchestrator** that combines two gates:
1. `_dispatch.sh::for_each_lang_with_verb "audit"` with `DEVLOOP_DISPATCH_ALWAYS_RUN=1` → `lang/rust/audit.sh` (`cargo audit`) + `lang/ts/audit.sh` (`pnpm audit --audit-level=high`). Proto has no `audit.sh` — dispatcher emits `STATUS=SKIPPED-NO-VERB REASON=proto-audit-sh-missing-or-not-executable` (expected).
2. `lang/proto/breaking.sh` invoked unconditionally separately (`buf breaking proto --against ".git#ref=<sha>,subdir=proto"`). Proto's audit-class gate is `breaking.sh`, not `audit.sh` (ADR-0033 §1 + §10:397).

The orchestrator returns the **worst of `(dispatch_rc, breaking_rc)`** — `set -e` short-circuit would mask the second invocation and silently break the always-run guarantee; the explicit RC capture block in `scripts/audit.sh` (no enclosing function; flat script) preserves both gates.

**DEP-CHANGE-GATED as of task #47 (ADR-0033 §3 amendment).** `DEVLOOP_DISPATCH_ALWAYS_RUN=1` is RETAINED so the dispatcher still invokes each `audit.sh` wrapper unconditionally — but the wrapper now runs a fail-closed dep-manifest gate internally. When no dependency manifest changed, the wrapper emits `STATUS=SKIPPED-NO-DIFF REASON=no-dep-changes` (expected, non-dominating) instead of scanning. The wrapper runs the scan on any doubt (indeterminate diff). When the scan runs and suppressed advisories are filtered, the wrapper emits a `SUPPRESSED=<ids>` stderr line (the configured suppression set in effect this run). `buf breaking` remains always-run. The always-run audit GUARANTEE moved to the Layer-3 `audit-suppressions-check` guard (§6.3) + the **weekly scheduled full scan** (below).

**Scheduled full scan (the drift-catcher).** `.github/workflows/audit-scheduled.yml` runs weekly, forcing the gate ON (`DEVLOOP_AUDIT_FORCE_RUN=1`, force-run-only — bypasses the GATE, not suppressions) so it scans the full FROZEN lockfile against newly-published advisories — the diff-less vector the per-PR gate intentionally skips. On unsuppressed drift it goes red AND opens-or-updates a single rolling **`audit-drift` GitHub Issue** (auto-closes on a later clean run). On-call: between-PR drift shows up as the `audit-drift` issue + a red `Scheduled Audit` run, NOT a per-PR red — see the two-surfaces note in §6.3.

| REASON token | Wrapper | Cause / Fix |
|--------------|---------|-------------|
| `cargo-audit-failed` | `lang/rust/audit.sh` (`cargo audit`) | RUSTSEC advisory. **Triage decision**: fix-the-dep (preferred) vs suppress-in-manifest (security-owned). The ONLY sanctioned suppression channel is `audit-suppressions.toml` → generated `.cargo/audit.toml` (cargo-audit reads it natively); add an entry + `scripts/audit-suppressions-check.sh --fix` + reviewed PR per `docs/contributor/audit-suppressions.md`. Operators MUST NOT silence advisories ad-hoc via CLI flags, hand-edited `.cargo/audit.toml`, or Dependabot alert dismissals — the wrapper blocks `--ignore=…` pass-through (see the `IMPORTANT (security finding 1)` comment at the top of `lang/rust/audit.sh`). For a transitive-dep advisory we don't own, escalate to security. |
| `pnpm-audit-failed` | `lang/ts/audit.sh` (`pnpm audit --audit-level=high` + post-hoc filter) | High-severity npm advisory not in the suppression list. Same triage discipline — suppression goes in `audit-suppressions.toml` → generated `.pnpm-audit-ignore.json` (the wrapper's post-hoc filter reads it), security-owned (ADR-0033 §11). Wrapper blocks `--audit-level=critical` and `--ignore=…` pass-through. A malformed `.pnpm-audit-ignore.json` makes the FILTER apply zero suppressions and proceed (fail-safe + stderr WARN — the scan still reds on real advisories); the Layer-3 check hard-fails the malformed file separately (`suppression-drift`). |
| `buf-breaking-failed` | `lang/proto/breaking.sh` (`buf breaking … --against .git#ref=<base-sha>,subdir=proto`) | Wire-breaking change against the resolved base ref. For intentional wire-breaks, the override mechanism is deferred to ADR-0033 Wave 3 #10 (task #41) — no CLI bypass exists, by design. |
| `base-ref-unresolved` | `lang/proto/breaking.sh` — the `base-ref-unresolved` emission (no enclosing function; flat script) | `_get_base_ref.sh` exited non-zero before reaching `buf breaking`. The wrapper distinguishes this from `buf-breaking-failed` so operators don't chase a wire-break issue when the actual problem is a degraded git state. Jump to §5. |
| `buf-binary-missing` | `lang/proto/breaking.sh` | See §6.1. |
| `proto-audit-sh-missing-or-not-executable` | `_dispatch.sh::for_each_lang_with_verb` | Expected — proto has no `audit.sh`; `breaking.sh` is the proto audit-class gate, wired separately in `scripts/audit.sh`. |
| `audit-gate-wrapper-missing-<lang>` | `scripts/audit.sh` (post-dispatch SECURITY guard, task #50) | A `<lang>/audit.sh` that SHOULD exist is missing/non-executable (deleted, `chmod`-stripped, or a new lang dir added without it) — so that lang's dependency-vuln scan silently did NOT run. Without this guard the failure would be MASKED by a sibling lang's passing audit (the rank-0 `SKIPPED-NO-VERB` is dominated by the sibling's `OK`), fail-OPENing a security gate. The guard scans the dispatch output for the `-UNEXPECTED-verb-missing-or-not-executable` marker and emits `STATUS=FAIL` so the layer reds. **Fix**: restore `<lang>/audit.sh` + `chmod +x`, or (if the gap is genuinely intended) add `<lang>:audit` to `__intentional_missing_verbs` in `_dispatch.sh`. Distinct from proto's intentional `-sh-missing-` gap, which never trips the guard. |

**Exit code for `audit-gate-wrapper-missing-<lang>` — FAIL / exit 1 on both paths (task #50).**
The guard emits `STATUS=FAIL`, so the IN-PIPELINE path (`layer6.sh` keys the layer exit on the
STATUS stream) aggregates to a layer FAIL → exit 1; the STANDALONE `./scripts/audit.sh` path folds
its own rc to 1 to match. `FAIL` ("a security gate that should have run did not execute") is the
honest enum. We deliberately do NOT emit `STATUS=UNKNOWN` to push the layer to exit 2 — UNKNOWN is
reserved for "no status emitted / child crashed before STATUS" (§3), and a synthetic UNKNOWN with a
precise REASON would corrupt that meaning. Both paths agree at exit 1; non-zero is fail-closed and
`layer-all.sh` folds any non-zero → `final_exit=1`.

The one asymmetry that DOES remain is audit-vs-general: an UNEXPECTED missing wrapper for the
**audit** verb reds as `STATUS=FAIL REASON=audit-gate-wrapper-missing-<lang>` (exit 1) — because
`scripts/audit.sh` emits an explicit `STATUS=FAIL` to close the security fail-open — whereas a
general (non-audit) UNEXPECTED verb-missing surfaces as `STATUS=SKIPPED-NO-VERB` with a
`*-UNEXPECTED-verb-missing-or-not-executable` REASON (exit 2 when it's the aggregate winner; see §7),
and its cross-lang-masking tail stays in the deferred residual (docs/TODO.md). So: audit gate missing
→ always reds (FAIL/1, masking closed); general verb missing → reds as SKIPPED-NO-VERB/2 only when it
wins the aggregate. The audit path is hardened beyond the general path precisely because it guards a
security control.

**Audit-config ownership reminder**: audit-config changes (`audit-suppressions.toml`, the generated `.cargo/audit.toml` / `.pnpm-audit-ignore.json`, audit-level thresholds, advisory exemptions) are **security-owned** per ADR-0033 §11. Operators should not modify allowlists or suppression flags as part of failure triage — escalate to security. **EXCEPTION — suppression renewal on `suppression-past-due`:** editing `audit-suppressions.toml` to renew an `expires` date (then `--fix` + reviewed PR) IS the sanctioned remediation, NOT a prohibited ad-hoc edit — see the full security-reviewed exception note in §6.3. The prohibition targets silent incident-time silencing (CLI flags, hand-edited derived files, Dependabot alert dismissals, or an unreviewed expires-bump just to unblock CI).

**Worked example — twin-rc collection**:

```
# dispatch stage (DEVLOOP_DISPATCH_ALWAYS_RUN=1):
STATUS=OK REASON=cargo-audit-passed       (rust)
STATUS=OK REASON=pnpm-audit-passed        (ts)
STATUS=SKIPPED-NO-VERB REASON=proto-audit-sh-missing-or-not-executable  (proto)
→ dispatcher aggregates: STATUS=OK REASON=audit-all-langs-ok
→ dispatch_rc = 0

# breaking stage (separate):
STATUS=FAIL REASON=buf-breaking-failed    (proto/breaking.sh)
→ breaking_rc = 1

# scripts/audit.sh exit:
→ exit max(0, 1) = 1
→ Layer 6 collects both STATUS lines; aggregate_worst_status OK FAIL = FAIL
→ Layer 6 stdout:  STATUS=FAIL REASON=layer6-summary        (generic summary; enum-only)
→ Layer 6 stderr:  LAYER=6 ... RESULT=FAIL REASON=buf-breaking-failed  (worst-child cause, task #50)
```

The proto `SKIPPED-NO-VERB REASON=proto-audit-sh-missing-or-not-executable` line is the
INTENTIONAL gap (proto has no `audit.sh`; `breaking.sh` is its audit gate) — exit 0, it
does not red the layer; the FAIL here is `breaking.sh`. As in §6.1, the stderr `LAYER=`
anchor names the worst-child cause (`buf-breaking-failed`), not `layer6-summary`.

### 6.7 Layer 7 — Env-tests (`scripts/layer7.sh`)

Currently emits `STATUS=N/A REASON=wave2-pending` — the env-tests wiring is deferred to a future task (ADR-0033 §1 / §3 / §14 layer-7 contract). When wired, Layer 7 covers dev-cluster bring-up + Rust env-tests + Playwright `@smoke`.

| REASON token | Wrapper | Cause / Fix |
|--------------|---------|-------------|
| `wave2-pending` | `scripts/layer7.sh` — the `emit_status N/A "wave2-pending"` line (no enclosing function; flat script) | Expected — Layer 7 body not yet implemented. Aggregates to N/A above OK so the layer signals "this verb is not yet wired" distinct from "ran cleanly". |

When Layer 7 lands, its failure modes will document here. ADR-0030 (host-side cluster helper, renumbered from Layer 8) is the canonical contract.

---

## 7. Per-Language Wrapper Triage (Cross-Cutting)

### `STATUS=SKIPPED-NO-VERB` — interpreting the verb-discovery skip

`_dispatch.sh::for_each_lang_with_verb` emits this — the `SKIPPED-NO-VERB` branch inside that function — when `lang/<X>/<verb>.sh` is missing or not executable. Since task #50 (2026-06-08) the REASON token splits this into **three** distinct states, and the exit code now depends on which (see §3 table + ADR-0033 §6 amendment):

1. **Intentional gap → exit 0** (most common): the `<lang>:<verb>` is on the documented allowlist (`proto:test`, `proto:audit` — proto has no `test.sh`/`audit.sh` per ADR-0033 §1; `breaking.sh` is proto's audit gate). REASON = `<lang>-<verb>-sh-missing-or-not-executable`. **Informative, not a failure** — ranks below OK so a co-running OK lang dominates, and `status_to_exit_code` maps it to 0. Triaging Layer 4 / 6 you'll see `proto-test-sh-missing-…` / `proto-audit-sh-missing-…` — these are expected, not a regression.
2. **Unexpected verb-missing → exit 2** (a bug, NOT self-justifying): a verb wrapper that SHOULD exist is missing or not executable (deleted, `chmod`-stripped, or a new lang dir added without the verb). REASON = `<lang>-<verb>-UNEXPECTED-verb-missing-or-not-executable` (the UPPERCASE `UNEXPECTED` infix shouts in logs). The layer **reds** (exit 2, the wiring-fault class with UNKNOWN). **Action**: restore the wrapper and `chmod +x` it — do NOT rationalize this as a deliberate skip. If the gap is genuinely intentional, add `<lang>:<verb>` to the allowlist (`__intentional_missing_verbs` in `_dispatch.sh`; the `DEVLOOP_INTENTIONAL_MISSING_VERBS` env override is test-only, gated behind `DEVLOOP_TEST=1`).
3. **All langs filtered → exit 0**: `DEVLOOP_DISPATCH_INCLUDE_LANGS` / `EXCLUDE_LANGS` cleared the whole set (operator intent). REASON = `all-langs-filtered`.

### `STATUS=SKIPPED-NO-DIFF` — diagnosing predicate output

`_dispatch.sh::for_each_lang_with_verb` emits this — the `SKIPPED-NO-DIFF` branch inside that function — when `lang/<X>/changed.sh` returned exit code 1 (lang untouched). Three-step triage:

1. **Read the `BASE_REF=` line** in the same layer's stderr log. Was the diff what you expected?
2. **Inspect the cache**: `cat "${DEVLOOP_TMP:-/tmp/devloop}/changed-files.layer-<n>"`. Is the file you cared about listed?
3. **Re-run the predicate manually** (hermetically):
   ```bash
   DEVLOOP_LAYER=manual bash scripts/lang/<X>/changed.sh; echo "rc=$?"
   ```
   **Predicate exit-code convention (reversed from typical shell)**: `rc=0` means *"this lang IS affected"* (the diff touches it); `rc=1` means *"this lang is provably untouched"*. This inversion matches the dispatcher's `if ! changed.sh; then SKIPPED-NO-DIFF` semantics but is easy to misread at 3am — explicit callout.

### `STATUS=N/A` — documented gap vs. wrapper bug

Documented gaps: `layer7.sh` `wave2-pending`, `_dispatch.sh` `no-languages-registered` (would mean every lang directory got filtered out — possible operator-error with `DEVLOOP_DISPATCH_INCLUDE_LANGS=<nonexistent>`).

Unexpected `N/A` outside the documented placeholders is a wrapper bug — escalate. The enum ranks above OK precisely so an unexpected `N/A` does not silently pass as "ran cleanly".

### `_changed_helpers.sh` debugging

Predicates use two helpers (`scripts/lang/_changed_helpers.sh`):
- **`diff_touches_path <prefix>`** (lines 52-55): awk + fixed-string `index($0, p) == 1`. Matches files whose path *starts with* `<prefix>`. Fixed-string by design — a future `c++` or `c#` lang dir would silently regex-match wrong files under naive `grep "^prefix"`.
- **`diff_touches_root_files <file…>`** (lines 63-71): `grep -qxF` (fixed-string, exact-line). Matches root-level files exactly by name.

When a predicate misfires:
1. Inspect the predicate source (`scripts/lang/<X>/changed.sh`) — most are 3-5 lines.
2. Inspect `_changed_helpers.sh` to confirm helper semantics.
3. Re-run the predicate manually with `DEVLOOP_LAYER=manual` (see above). Same hermetic shape, real cache.
4. If the predicate reads the wrong cache, `DEVLOOP_LAYER` is not exported — a layer-script bug (the layer-skeleton ought to export `DEVLOOP_LAYER` via `_common.sh::layer_lifecycle_begin`).

### `_test_changed_predicates.sh` drift detection

Runs every devloop in Layer 3 alongside the simple guards. Hermetic — `mktemp` cache, `env -i` invocation. Failure mode (`_test_changed_predicates.sh::__assert_predicate`):

```
[<lang>] path=<fixture-row> expected_rc=<0|1> actual_rc=<0|1>
  rationale: <fixture rationale>
  see: scripts/lang/<lang>/changed.sh
```

Reversed-from-shell exit-code convention (per above): `expected_rc=0` means the fixture asserts the lang IS affected by the path; `expected_rc=1` means the fixture asserts the lang is provably untouched. A mismatch means either (a) the predicate is broken — fix `scripts/lang/<lang>/changed.sh`, or (b) the fixture row is stale — amend the fixture (with rationale). When in doubt, prefer correcting the predicate; the fixture was written down for a reason.

**Adding a new language** triggers a `predicate-meta-test-failed` until the meta-test gains rows for the new lang. The fix is mechanical: add fixture rows under §"Wave N — \<Lang\> predicate fixtures" in `_test_changed_predicates.sh`, mirroring the existing Rust and Proto sections.

---

## 8. Symptom → Resolution Catalogue (Cross-Reference Index)

Grep-driven entry point. Match the symptom, jump to the section.

| Symptom (greppable) | Likely cause | Jump |
|--------------------|--------------|------|
| `RESULT=FAIL` on a layer; first hit | Read the per-layer subsection | §6 |
| `PRECONDITION_FAILURE:` at startup | CI shallow clone (or other layer-all precondition) | §4 + §5 |
| `ERROR:` in a layer stderr log | `_get_base_ref.sh` resolver failure | §4 + §5 |
| `STATUS=SKIPPED-NO-VERB REASON=proto-test-sh-…` | Expected — proto has no test.sh (intentional gap, exit 0) | §7 |
| `STATUS=SKIPPED-NO-VERB REASON=proto-audit-sh-…` | Expected — proto has no audit.sh; breaking.sh is the gate (intentional gap, exit 0) | §6.6 + §7 |
| `REASON=…-UNEXPECTED-verb-missing-or-not-executable` | A verb wrapper that should exist is missing/`chmod`-stripped — now **reds the layer (exit 2)**, was silently exit 0 pre-task-#50. Restore the wrapper + `chmod +x`. | §7 |
| `REASON=wrapper-aborted-early-exit-<rc>` | A verb wrapper crashed BEFORE emitting STATUS (e.g. `set -e` abort, `exit 1` in a helper); previously surfaced as a silent `UNKNOWN`. The EXIT trap now emits FAIL with the abort code `<rc>`. | §6.4 + §7 |
| `REASON=audit-gate-wrapper-missing-<lang>` | A `<lang>/audit.sh` (dependency-vuln scan) is missing/non-executable and would otherwise be MASKED by a sibling lang's passing audit — a fail-OPEN on a SECURITY gate. The `scripts/audit.sh` guard emits STATUS=FAIL so the layer reds (exit 1, both in-pipeline and standalone). Restore the wrapper or allowlist `<lang>:audit`. | §6.6 |
| `STATUS=N/A REASON=wave2-pending` | Expected — Layer 7 placeholder | §6.7 |
| `predicate-meta-test-failed` | Lang predicate vs fixture drift | §6.3 + §7 |
| `predicate-meta-test-failed` after adding a new lang | Missing fixture row in `_test_changed_predicates.sh` | §7 |
| Layer 1 fails on a docs-only PR | `lang/<X>/changed.sh` over-classification (e.g. `crates/foo/README.md` → rust per ADR-0033 §3 trade-off); cargo check is cheap. | §6.1 |
| Layer 1 fails locally with `nx: command not found` | Local-only failure — CI has corepack/pnpm install in setup. | §6.1 (run `pnpm install`) |
| Layer 4 fails with `Neither podman nor docker found` | Missing container runtime — `lang/rust/test.sh` bring-up. | §6.4 |
| Layer 6 fails on every CI run, local clean | Likely CI base-ref shift (post-#42) — check `BASE_SOURCE=ci-pr` `BASE_REF=` is merge-base, not main tip. | §5 (CI-PR scope shift) |
| Layer 6 `cargo-audit-failed` / `pnpm-audit-failed` from a transitive dep | Triage: fix vs suppress-in-`audit-suppressions.toml` (security-owned, ADR-0033 §11); never CLI flag / hand-edit / Dependabot dismissal. | §6.6 |
| Layer 3 `suppression-past-due` | A suppression hit its `expires` — INTENTIONAL red. Renew (re-verify + `--fix` + reviewed PR) or fix the advisory. | §6.3 |
| Layer 3 `suppression-drift` | Derived files (`.cargo/audit.toml` / `.pnpm-audit-ignore.json`) drifted from the manifest — run `scripts/audit-suppressions-check.sh --fix`. | §6.3 |
| Layer 3 `suppression-malformed` | Manifest/derived file unparseable — fix the offending line named in the `MALFORMED:` output. | §6.3 |
| Layer 3 `suppression-quality` | Empty reason/ticket, bad `expires`, or bad `ecosystem` in the manifest. | §6.3 |
| Layer 3 `suppression-override-without-test-sentinel` | A test-injection override env set without `DEVLOOP_TEST=1` — tamper/misconfig; investigate, do NOT unset-and-rerun. | §6.3 |
| `test-sentinel-set-in-ci` (layer-all / layer3) | `DEVLOOP_TEST` leaked into a CI job — pipeline-integrity incident; find + remove what exported it. | §6.3 |
| Layer 3 `dependabot-ignore-present` | `.github/dependabot.yml` has a non-empty `ignore:` block — move suppressions to `audit-suppressions.toml`; Dependabot `ignore:` is not a suppression channel. | §6.3 |
| Open `audit-drift` GitHub Issue / red `Scheduled Audit` run | Between-PR drift: a new advisory against an UNCHANGED lockfile, caught by the weekly scheduled scan. Triage like a Layer-6 advisory; the issue auto-closes when a later scheduled run is clean. | §6.6 |
| Layer 6 `buf-breaking-failed` on an intentional wire-break | No override exists yet; deferred to ADR-0033 Wave 3 #10 (task #41). | §6.6 |
| Layer 6 `base-ref-unresolved` | Degraded git state ahead of `buf breaking` — jump to resolver playbook. | §5 |
| Layer N missing `BASE_REF=` line entirely | Wrapper bug — resolver did not run | escalate |
| Every pipeline emits dozens of `BASE_REF=` lines | Known cost concern (task #42 Tech Debt Pointer #2); cache + suppression-sentinel mitigation tracked, not yet implemented. | §5 (Known cost concern) |
| `WARN BUDGET_BREACH LAYER=<n>` | A single layer exceeded its budget (default 20s) | §3 |
| `WARN BUDGET_TOTAL_BREACH` | Always-run subset (layers 3 + 6) exceeded 90s p95 — ADR-0033 §4 budget. | §3 |
| `❌ Gate-2: no validation verdict found` (at commit) | Devloop-completion commit but no verdict — pipeline never run this session. Run `./scripts/layer-all.sh`. | §8.5 |
| `❌ Gate-2: verdict is FAIL` (at commit) | The recorded pipeline run did not pass — fix the failing layer(s) and re-run. | §8.5 |
| `❌ Gate-2: ... signature mismatch` + `modified after validation: <path>` | A validated file changed after the run — re-stage + re-run. | §8.5 |
| `❌ Gate-2: ... staged but not in verdict: <path>` | A new validated file was staged the verdict never saw — re-run. | §8.5 |
| `❌ Gate-2: ... validated but not staged: <path>` | A validated (often untracked/transient, e.g. a dirtied lockfile) file is in the verdict but not staged — re-run, or `--no-verify` if intentional. | §8.5 |
| `❌ Gate-2: verdict is for a different devloop` | Stale verdict from another devloop in the same `/tmp` session — re-run for THIS one. | §8.5 |
| `❌ Gate-2: shared library missing` | Broken checkout (`scripts/lang/_gate2_binding.sh` absent) — restore it, or `--no-verify` if intentional. | §8.5 |

---

## 8.5 Gate-2 Authority Verdict (commit-time enforcement)

The pre-commit hook (`.githooks/pre-commit`, wired via `core.hooksPath`) enforces a
**Gate-2 authority verdict** for devloop-completion commits. This closes skip-vector
**C**: a Lead (or agent) asserting a pipeline pass *in lieu of* actually running it.
The producer is `scripts/layer-all.sh`, which emits `/tmp/devloop/gate2-verdict` as
its final step via an `EXIT` trap (`scripts/lang/_gate2_binding.sh::emit_gate2_verdict`);
the validator is `scripts/lang/_gate2_binding.sh::gate2_validate_commit`. Design:
`docs/devloop-outputs/2026-06-09-gate2-authority-verdict-gate/main.md`.

### Threat model — READ THIS before trusting the hook

The local hook is **anti-drift / anti-laziness, NOT anti-forgery, and NOT a security
boundary.** Two facts make that explicit:

1. **Bypassable.** `git commit --no-verify` skips the hook entirely. It is also the
   intended escape hatch (see Rollback below) — so a developer who knows the verdict
   is wrong/transient can still commit deliberately.
2. **Forgeable.** An agent (or human) with filesystem write can hand-author a
   well-formed `gate2-verdict` whose signature matches the staged tree. The verdict is
   a flat text file in `/tmp`; nothing cryptographically binds it to a *real* run.

The only **non-bypassable** enforcement is **CI**: `.github/workflows/ci.yml` re-runs
`./scripts/layer-all.sh` from scratch on every push/PR and **never reads** the `/tmp`
artifact. That independent re-run is the forgery defense — which is also why the
artifact is ephemeral in `/tmp` and is never committed. Treat the local hook as a fast
"did you actually run it / has the tree drifted since" check, and CI as the source of
truth.

### When the hook requires a verdict (trigger)

Both must hold (`gate2_validate_commit`):
1. a **staged** `docs/devloop-outputs/<slug>/main.md` is at **Phase=complete**, AND
2. the **staged changeset minus exclusions** contains ≥1 file.

Exclusions (mirror `crates/dt-guard/src/cross_boundary_scope.rs::is_symmetric_exclusion`,
plus a tree-wide `docs/devloop-outputs/**` widening): `docs/devloop-outputs/**`,
`docs/TODO.md`, `docs/specialist-knowledge/*/INDEX.md`, `docs/user-stories/*.md`. A
main.md-only edit (validated set empty) therefore no-ops; a non-devloop commit no-ops.
The hook is **FAIL-CLOSED**: any error evaluating the trigger or verdict blocks.

### Failure shapes (all block the commit)

| Hook message | Meaning | Fix |
|--------------|---------|-----|
| `no validation verdict found` | No verdict file — pipeline never ran this session. | `./scripts/layer-all.sh`, then commit. |
| `verdict is FAIL` | The run did not pass (`GATE2=FAIL`, emitted even on early-exit via the trap). | Fix the failing layer(s) (§6), re-run. |
| `signature mismatch` → `modified after validation: <path>` | A validated file changed after the run. | Re-stage + re-run. |
| `signature mismatch` → `staged but not in verdict: <path>` | A new validated file was staged the verdict never bound. | Re-run so the new file is bound. |
| `signature mismatch` → `validated but not staged: <path>` | A path the verdict bound is not in the staged commit. Common benign cause: a layer transiently **dirtied a lockfile** (`Cargo.lock`/`pnpm-lock.yaml`) the producer saw in the worktree but you never staged. | If the lockfile change is real, `git add` it and re-run. If transient/unwanted, revert it and re-run — or `git commit --no-verify` if you are sure the staged tree is correct. (Lockfiles are deliberately NOT excluded: a *real* committed lockfile change must stay bound.) |
| `verdict is for a different devloop` | Stale verdict (`SLUG=` mismatch) from another devloop in the same `/tmp` session. | Re-run for the devloop you are committing. |
| `shared library missing` | `scripts/lang/_gate2_binding.sh` absent — broken checkout. | Restore the file, or `--no-verify` if intentional. |

### Rollback / escape hatch

If a hook bug ever **wedges** a legitimate commit, `git commit --no-verify` bypasses it.
That is the documented escape hatch while diagnosing — but remember CI still re-runs the
full pipeline, so a genuinely failing tree will be caught there regardless.

### Isolation self-test

`scripts/guards/simple/selftest-gate2-verdict.sh` drives the producer + validator against
synthetic staged trees in throwaway temp git repos, covering the verification matrix
(a–f) plus slug-mismatch and extra-staged-file drift. It runs in Layer 3 (guards) every
devloop and in CI, and emits the ADR-0033 `STATUS=` contract line. Run it standalone to
reproduce a hook-logic regression without touching your real index:
`./scripts/guards/simple/selftest-gate2-verdict.sh`.

---

## 9. Escalation & Related References

### When to escalate

| Escalation target | Trigger |
|-------------------|---------|
| operations | A layer reports `RESULT=UNKNOWN` (dispatcher / wrapper bug — child crashed before STATUS line AND its EXIT trap did not fire, e.g. killed `-9`). Since task #50 this is rarer: the 14 verb wrappers self-emit `STATUS=FAIL REASON=wrapper-aborted-early-exit-<rc>` via an EXIT trap when they abort before emitting, so a pre-emit crash now surfaces as `FAIL`, not `UNKNOWN`. A genuine `UNKNOWN` therefore points at a non-trap path. |
| operations | A layer reports `RESULT=SKIPPED-NO-VERB` with an `*-UNEXPECTED-verb-missing-or-not-executable` REASON (exit 2 — task #50). A verb wrapper that should exist is missing/non-executable (deleted, `chmod`-stripped, or a new lang dir missing a verb). Restore + `chmod +x` it, or add `<lang>:<verb>` to the `__intentional_missing_verbs` allowlist if the gap is genuinely intended. |
| operations | `BASE_REF=` line missing from a layer's stderr log (resolver did not run; layer-skeleton regression). |
| operations **AND** security | Layer 6 reports `REASON=audit-gate-wrapper-missing-<lang>` (task #50 SECURITY guard). A dependency-vuln scan wrapper is missing/non-executable — a fail-OPEN on a security gate that the guard caught. Operations: restore `<lang>/audit.sh` + `chmod +x` (or allowlist `<lang>:audit` if the gap is intended). Security: confirm whether the lang's dep-audit coverage lapsed in any prior green run and whether a manual scan is warranted. |
| infrastructure | A `scripts/lang/_*.sh` helper or `scripts/lang/<X>/<verb>.sh` wrapper itself is broken (not just reporting a real failure). |
| security | An audit advisory needs an `[advisories.ignore]` entry — policy is security-owned (ADR-0033 §11). |
| security | An advisory's mean-time-to-resolution exceeds **14 days** — tripwire (ADR-0033 §12). |
| protocol | `buf breaking` fires on an intentional wire-break and no override mechanism exists yet (ADR-0033 Wave 3 #10, task #41). |

### Related documentation

- **ADR-0033** (`docs/decisions/adr-0033-polyglot-validation-pipeline.md`) — canonical design spec for the polyglot pipeline. §3 always-run / skip-if-untouched matrix + classifying principle; §4 layer-script contract; §6 wrapper contract + STATUS enum; §7 base-ref resolution.
- **ADR-0030** (`docs/decisions/adr-0030-*.md`) — host-side cluster helper, Layer 7 contract (renumbered from Layer 8).
- **`.claude/skills/devloop/SKILL.md`** — devloop workflow. Step 6 (Gate 2 — Validation) is the entry point; pointer back to this runbook lands there.
- **`docs/devloop-outputs/2026-05-12-skill-step6-rewrite-task38/main.md`** — task #38 (Layer 8→7 renumber + SKILL.md Step 6 rewrite).
- **`docs/devloop-outputs/2026-05-13-base-ref-unification-task42/main.md`** — task #42 (base-ref unification + redundant CI fetch removal). §Tech Debt Pointers entry 2 documents the `BASE_REF=` multiplication concern; entry 4 documents the two-token convention canonicalized in §4 of this runbook.
- **`docs/runbooks/TEMPLATE.md`** — parent template for service-incident runbooks. This pipeline runbook adapts where appropriate (no Prometheus alert; pipeline-failure runbook differs from service-incident runbook).

---

## 10. Cite Convention

Cites in this runbook (and other long-lived docs under `docs/runbooks/**` and `.claude/skills/**`) follow a durable shape, enforced by `scripts/guards/simple/validate-doc-citations-no-line-numbers.sh` (Guard A) and `validate-doc-citations-symbol-resolves.sh` (Guard C):

- **Code anchor**: `<file>::<symbol>` — preferred form. Guard C verifies the symbol exists in the file via per-language regex (rs/sh/toml/yaml/md/proto).
- **Section reference**: `<file> § "<header>"` for cites into markdown/YAML where the anchor is a section/alert name rather than a callable symbol.
- **Prose fallback**: `<file>::<symbol> — <near-clause>` when a function contains multiple distinct cite-worthy blocks (comments, error emissions, code branches). For flat scripts with no top-level function, use `<file> — <near-clause> (no enclosing function; flat script)`. Examples in this runbook: the `ERROR: …` emission cites in §4, the `RC capture block in scripts/audit.sh` cite in §6.6.

Bare `:<NN>` line cites are forbidden — they drift the moment a file grows by one line. Guard A enforces. If a cite truly requires a line number (e.g., the cited LOC is the entire constant), annotate the line with `<!-- guard:ignore(<reason ≥10 chars, not test/tmp/todo/fixme/wip>) -->`.

---

## 11. Changelog

| Date | Author | Changes |
|------|--------|---------|
| 2026-05-14 | operations (task #39) | Initial creation. Documents `scripts/layer-all.sh` + Layers 1-7 + shared helpers as of commit `0130ce8`. Closes ADR-0033 Wave 3 #8. |
| 2026-05-14 | operations (doc-citation guards devloop) | Sweep: converted ~21 bare-line cites to function-name anchors. Added §10 "Cite Convention" documenting the form Guards A+C enforce. |
| 2026-06-09 | infrastructure (task #51) | Added §8.5 "Gate-2 Authority Verdict" — commit-time verdict enforcement (skip-vector C), threat model (local = anti-drift/bypassable, CI = forgery backstop), trigger + failure shapes + `--no-verify` escape hatch + isolation self-test. Added catalogue rows for the `❌ Gate-2:` messages. |
