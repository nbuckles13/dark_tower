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

If you don't see a `LAYER_SUMMARY_BEGIN` block at all, the orchestrator aborted mid-flight; check the last entry in `${DEVLOOP_TMP:-/tmp/devloop}/layer-*.stderr.log` for a `LAYER=<n> … RESULT=…` line — the EXIT trap (`_common.sh:225` `__layer_lifecycle_end`) guarantees this stderr line emits even under `set -e` abort.

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
| **0** | PASS or SKIPPED (success-exit class) | `OK | SKIPPED-NO-DIFF | SKIPPED-NO-VERB | N/A` |
| **1** | FAIL — work ran and detected a problem | `FAIL` |
| **2** | PRECONDITION_FAILURE — wrapper/orchestrator bug, dispatcher misconfig, OR pre-layer guardrail tripped (e.g. shallow CI clone) | aggregator treats as `UNKNOWN` |

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
| `SKIPPED-NO-VERB` | `lang/<X>/<verb>.sh` missing-or-not-executable; the dispatcher records the gap rather than silently skipping | `proto-test-sh-missing-or-not-executable`, `proto-audit-sh-missing-or-not-executable` |
| `N/A` | Documented gap (e.g. `layer7.sh` `wave2-pending`) | `wave2-pending`, `no-languages-registered`, `<verb>-aggregate-na` |

`UNKNOWN` is **not** a wrapper-emitted enum — it appears in the aggregator when a child wrapper crashes before emitting `STATUS=`, or when stdout streaming breaks. `UNKNOWN` ranks above `FAIL` in the precedence ladder because it signals a dispatcher/wrapper bug, not a real-work failure.

### Worst-child STATUS aggregation

Each layer collects every child `STATUS=` line that came across stdout (`_common.sh:208` `tee_collect_statuses`), then aggregates with `aggregate_worst_status` using the rank:

```
SKIPPED-NO-VERB (0)  <  SKIPPED-NO-DIFF (1)  <  OK (2)  <  N/A (3)  <  FAIL (4)  <  UNKNOWN (5)
```

The intuition (locked in ADR-0033 §1 by `_common.sh:113-126`): *"if any child did real work and passed, the layer passed; otherwise the SKIPPED-\* state is informative. N/A propagates above OK because it signals 'this verb is not yet wired' — distinct from 'ran cleanly'. UNKNOWN ranks above FAIL — surface dispatcher bugs loud, not silent."*

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

Every layer emits (via the EXIT trap installed by `layer_lifecycle_begin`, `_common.sh:200`):

```
LAYER=<n> START=<unix-ts> END=<unix-ts> DURATION=<s> RESULT=<enum> REASON=<reason>
```

This is the **layer-level anchor** for greppable triage in `${DEVLOOP_TMP:-/tmp/devloop}/layer-<n>.stderr.log`. EXIT-trap emission is guaranteed even under `set -e` abort or signal-kill — a runbook reader who hits "the orchestrator died mid-layer" still sees the partial layer state in stderr.

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
| `_get_base_ref.sh:36` | `ERROR: ref name contains unexpected characters: <ref>` | `__validate_ref_name` — env-injection defense-in-depth (ADR-0033 §7 security) |
| `_get_base_ref.sh:79` | `ERROR: could not compute merge-base for GITHUB_BASE_REF=<ref>` | CI-PR `git merge-base` failed (ref unreachable / corrupt pack / shallow clone) |
| `_get_base_ref.sh:114` | `ERROR: could not resolve base ref to sha: <ref>` | ref resolved but commit unreachable in the local pack |

All three sites `exit 2` after emitting.

### `PRECONDITION_FAILURE:` — Pre-layer guardrail emissions

Emitted only by `scripts/layer-all.sh:40-43`. Indicates a precondition for the layer pipeline is not met:

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

Every invocation of `_get_base_ref.sh` emits exactly one stderr line of the form (ADR-0033 §7 normative requirement, emitted by `__emit_base_ref_line` at `_get_base_ref.sh:51`):

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

The resolver writes `${DEVLOOP_TMP}/changed-files.layer-<n>` (`_get_base_ref.sh:120-126`); per-language `lang/<X>/changed.sh` predicates read it via `_changed_helpers.sh::__changed_files` (which lazy-invokes the resolver if the cache is missing).

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
STATUS=FAIL REASON=buf-build-failed             (proto, stage 1)
STATUS=SKIPPED-NO-DIFF REASON=rust-no-diff      (stage 2, rust untouched)
STATUS=SKIPPED-NO-DIFF REASON=ts-no-diff        (stage 2, ts untouched)
→ Layer 1 RESULT=FAIL REASON=layer1-summary   (worst-child wins)
```

The stage-2 dispatch runs unconditionally even on stage-1 fail — observability O2 (one run reveals the full picture; don't force a second invocation).

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

Two `run_and_emit` invocations:
- `scripts/guards/run-guards.sh` — iterates every `scripts/guards/simple/**/*.sh` (excluding `fixtures/`). Each guard self-classifies per-file via path globs. Includes the Layer A scope-drift parser and Layer B classification-sanity guards (ADR-0024 cross-boundary).
- `scripts/lang/_test_changed_predicates.sh` — meta-test for each lang's `changed.sh` predicate. Hermetic — synthesizes a cache under `mktemp`, invokes each lang's predicate against fixture rows under `env -i`.

**Always-run**: yes — guards self-classify, predicate meta-test is hermetic. Layer 3 is one of the two layers (with Layer 6) inside the **90s p95 always-run wall-clock budget (ADR-0033 §4)** — a sustained `WARN BUDGET_TOTAL_BREACH` here is the operational signal to investigate.

| REASON token | Origin | Cause / Fix |
|--------------|--------|-------------|
| `guards-failed` | `scripts/guards/run-guards.sh` (via `run_and_emit`) | A specific guard tripped. The runner prints `FAILED: <guard-name>` + grep-extracted violation lines (`VIOLATION|violation|ERROR|error`). Jump to that guard's source under `scripts/guards/simple/`. |
| `predicate-meta-test-failed` | `scripts/lang/_test_changed_predicates.sh` | A `lang/<X>/changed.sh` predicate disagrees with its fixture row. Output prints `[<lang>] path=… expected_rc=… actual_rc=… rationale: …  see: scripts/lang/<lang>/changed.sh`. Fix by correcting the predicate OR amending the fixture (with rationale). See §7 for drift-detection workflow. |

### 6.4 Layer 4 — Test (`scripts/layer4.sh`)

`scripts/test.sh` → `for_each_lang_with_verb "test"` → `lang/rust/test.sh` + `lang/ts/test.sh`. Proto has no `test.sh` — dispatcher emits `STATUS=SKIPPED-NO-VERB REASON=proto-test-sh-missing-or-not-executable` (informative, expected).

**Skip-if-untouched**: rust, ts. Proto is naturally skipped via verb-discovery.

| REASON token | Wrapper | Cause / Fix |
|--------------|---------|-------------|
| `cargo-test-failed` | `lang/rust/test.sh` (`cargo test "$@"`) | A test failed. The wrapper brings up the test postgres container (podman / docker), applies pending sqlx migrations, then runs `cargo test`. Failure output is the cargo test stderr — fix the test. |
| (no REASON; runtime missing) | `lang/rust/test.sh:detect_runtime` | `Neither podman nor docker found. Please install one.` — install a container runtime. Wrapper aborts before reaching `run_and_emit`, so the STATUS line never emits; the layer aggregates `UNKNOWN`. |
| (db-bringup failure) | `lang/rust/test.sh:wait_for_db` | `Database did not become ready within ${MAX_WAIT_SECONDS}s` — container started but pg never accepted connections. Check the test container logs. |
| `nx-test-failed` | `lang/ts/test.sh` (`nx affected -t test:unit test:component`) | A TS unit/component test failed. Run the offending project's test target locally. |
| `proto-test-sh-missing-or-not-executable` | `_dispatch.sh:149` | Expected — proto has no `test.sh` per ADR-0033 §1. SKIPPED-NO-VERB ranks below OK, so a co-running OK lang dominates. |

### 6.5 Layer 5 — Lint (`scripts/layer5.sh`)

`scripts/lint.sh` → `for_each_lang_with_verb "lint"` → `lang/{rust,ts,proto}/lint.sh`.

**Skip-if-untouched**: rust, ts, proto.

| REASON token | Wrapper | Cause / Fix |
|--------------|---------|-------------|
| `cargo-clippy-failed` | `lang/rust/lint.sh` (`cargo clippy --workspace --all-targets -- -D warnings`) | Clippy lint or warning (warnings are denied — `-D warnings`). Run `cargo clippy --workspace --all-targets` locally for the full output. |
| `nx-lint-failed` | `lang/ts/lint.sh` (`nx affected -t lint`) | TS lint (eslint) violation. Run the project's lint target locally. |
| `buf-lint-failed` | `lang/proto/lint.sh` (`buf lint proto`) | Proto STANDARD-lint violation. Inspect output for the file + finding; `proto/buf.yaml` controls policy. |
| `buf-binary-missing` | `lang/proto/lint.sh` | See §6.1. |

### 6.6 Layer 6 — Audit (always-run; `scripts/layer6.sh`)

`scripts/audit.sh` is an **orchestrator** that combines two gates:
1. `_dispatch.sh::for_each_lang_with_verb "audit"` with `DEVLOOP_DISPATCH_ALWAYS_RUN=1` → `lang/rust/audit.sh` (`cargo audit`) + `lang/ts/audit.sh` (`pnpm audit --audit-level=high`). Proto has no `audit.sh` — dispatcher emits `STATUS=SKIPPED-NO-VERB REASON=proto-audit-sh-missing-or-not-executable` (expected).
2. `lang/proto/breaking.sh` invoked unconditionally separately (`buf breaking proto --against ".git#ref=<sha>,subdir=proto"`). Proto's audit-class gate is `breaking.sh`, not `audit.sh` (ADR-0033 §1 + §10:397).

The orchestrator returns the **worst of `(dispatch_rc, breaking_rc)`** — `set -e` short-circuit would mask the second invocation and silently break the always-run guarantee; the explicit RC capture in `scripts/audit.sh:13-23` preserves both gates.

**Always-run**: yes. Vulnerability advisories and wire-break detection both depend on external state that can change without a diff in the toolchain's footprint (ADR-0033 §3 classifying principle). Layer 6 is the second of the two layers (with Layer 3) inside the **90s p95 always-run wall-clock budget (ADR-0033 §4)** — `cargo audit` + `pnpm audit` + `buf breaking` collectively count toward that budget.

| REASON token | Wrapper | Cause / Fix |
|--------------|---------|-------------|
| `cargo-audit-failed` | `lang/rust/audit.sh` (`cargo audit`) | RUSTSEC advisory. **Triage decision**: fix-the-dep (preferred) vs ignore-via-config (security-owned). The audit-config file (`.cargo/audit.toml` when one is added) is the documented location for `[advisories.ignore]` entries; the policy is security-owned (ADR-0033 §11). Operators MUST NOT silence advisories ad-hoc via CLI flags — the wrapper deliberately blocks `--ignore=…` pass-through (security finding 1 at `lang/rust/audit.sh:6-12`). For a transitive-dep advisory we don't own, escalate to security. |
| `pnpm-audit-failed` | `lang/ts/audit.sh` (`pnpm audit --audit-level=high`) | High-severity npm advisory. Same triage discipline — threshold and ignore-list edits are security-owned (ADR-0033 §11). Wrapper blocks `--audit-level=critical` and `--ignore=…` pass-through. |
| `buf-breaking-failed` | `lang/proto/breaking.sh` (`buf breaking … --against .git#ref=<base-sha>,subdir=proto`) | Wire-breaking change against the resolved base ref. For intentional wire-breaks, the override mechanism is deferred to ADR-0033 Wave 3 #10 (task #41) — no CLI bypass exists, by design. |
| `base-ref-unresolved` | `lang/proto/breaking.sh:48` | `_get_base_ref.sh` exited non-zero before reaching `buf breaking`. The wrapper distinguishes this from `buf-breaking-failed` so operators don't chase a wire-break issue when the actual problem is a degraded git state. Jump to §5. |
| `buf-binary-missing` | `lang/proto/breaking.sh` | See §6.1. |
| `proto-audit-sh-missing-or-not-executable` | `_dispatch.sh:149` | Expected — proto has no `audit.sh`; `breaking.sh` is the proto audit-class gate, wired separately in `scripts/audit.sh`. |

**Audit-config ownership reminder**: audit-config changes (`.cargo/audit.toml`, audit-level thresholds, advisory exemptions) are **security-owned** per ADR-0033 §11. Operators should not modify allowlists or suppression flags as part of failure triage — escalate to security.

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
→ Layer 6 RESULT=FAIL REASON=layer6-summary
```

### 6.7 Layer 7 — Env-tests (`scripts/layer7.sh`)

Currently emits `STATUS=N/A REASON=wave2-pending` — the env-tests wiring is deferred to a future task (ADR-0033 §1 / §3 / §14 layer-7 contract). When wired, Layer 7 covers dev-cluster bring-up + Rust env-tests + Playwright `@smoke`.

| REASON token | Wrapper | Cause / Fix |
|--------------|---------|-------------|
| `wave2-pending` | `scripts/layer7.sh:7` (`emit_status N/A "wave2-pending"`) | Expected — Layer 7 body not yet implemented. Aggregates to N/A above OK so the layer signals "this verb is not yet wired" distinct from "ran cleanly". |

When Layer 7 lands, its failure modes will document here. ADR-0030 (host-side cluster helper, renumbered from Layer 8) is the canonical contract.

---

## 7. Per-Language Wrapper Triage (Cross-Cutting)

### `STATUS=SKIPPED-NO-VERB` — interpreting the verb-discovery skip

`_dispatch.sh:148-150` emits this when `lang/<X>/<verb>.sh` is missing or not executable. Two valid reasons:

1. **Intentional gap** (most common): proto has no `test.sh` (Layer 4) or `audit.sh` (Layer 6) per ADR-0033 §1. The `STATUS=SKIPPED-NO-VERB REASON=<lang>-<verb>-sh-missing-or-not-executable` line is **informative, not a failure** — it ranks below OK so a co-running OK lang dominates.
2. **Recently-deleted or chmod-stripped wrapper**: verify against ADR-0033 §1 directory listing. If the wrapper should exist, restore it and `chmod +x`.

Operators triaging Layer 4 / 6 see `proto-test-sh-missing-…` / `proto-audit-sh-missing-…` and may worry these indicate a regression. They don't — proto's verb set is deliberately incomplete; `breaking.sh` covers proto's audit-class concerns.

### `STATUS=SKIPPED-NO-DIFF` — diagnosing predicate output

`_dispatch.sh:124-128` emits this when `lang/<X>/changed.sh` returned exit code 1 (lang untouched). Three-step triage:

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
4. If the predicate reads the wrong cache, `DEVLOOP_LAYER` is not exported — a layer-script bug (the layer-skeleton ought to export `DEVLOOP_LAYER` via `layer_lifecycle_begin` at `_common.sh:199`).

### `_test_changed_predicates.sh` drift detection

Runs every devloop in Layer 3 alongside the simple guards. Hermetic — `mktemp` cache, `env -i` invocation. Failure mode (`_test_changed_predicates.sh:56-61`):

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
| `STATUS=SKIPPED-NO-VERB REASON=proto-test-sh-…` | Expected — proto has no test.sh | §7 |
| `STATUS=SKIPPED-NO-VERB REASON=proto-audit-sh-…` | Expected — proto has no audit.sh; breaking.sh is the gate | §6.6 + §7 |
| `STATUS=N/A REASON=wave2-pending` | Expected — Layer 7 placeholder | §6.7 |
| `predicate-meta-test-failed` | Lang predicate vs fixture drift | §6.3 + §7 |
| `predicate-meta-test-failed` after adding a new lang | Missing fixture row in `_test_changed_predicates.sh` | §7 |
| Layer 1 fails on a docs-only PR | `lang/<X>/changed.sh` over-classification (e.g. `crates/foo/README.md` → rust per ADR-0033 §3 trade-off); cargo check is cheap. | §6.1 |
| Layer 1 fails locally with `nx: command not found` | Local-only failure — CI has corepack/pnpm install in setup. | §6.1 (run `pnpm install`) |
| Layer 4 fails with `Neither podman nor docker found` | Missing container runtime — `lang/rust/test.sh` bring-up. | §6.4 |
| Layer 6 fails on every CI run, local clean | Likely CI base-ref shift (post-#42) — check `BASE_SOURCE=ci-pr` `BASE_REF=` is merge-base, not main tip. | §5 (CI-PR scope shift) |
| Layer 6 `cargo-audit-failed` from a transitive dep | Triage: fix vs ignore-via-config (`.cargo/audit.toml`); audit-config is security-owned (ADR-0033 §11). | §6.6 |
| Layer 6 `buf-breaking-failed` on an intentional wire-break | No override exists yet; deferred to ADR-0033 Wave 3 #10 (task #41). | §6.6 |
| Layer 6 `base-ref-unresolved` | Degraded git state ahead of `buf breaking` — jump to resolver playbook. | §5 |
| Layer N missing `BASE_REF=` line entirely | Wrapper bug — resolver did not run | escalate |
| Every pipeline emits dozens of `BASE_REF=` lines | Known cost concern (task #42 Tech Debt Pointer #2); cache + suppression-sentinel mitigation tracked, not yet implemented. | §5 (Known cost concern) |
| `WARN BUDGET_BREACH LAYER=<n>` | A single layer exceeded its budget (default 20s) | §3 |
| `WARN BUDGET_TOTAL_BREACH` | Always-run subset (layers 3 + 6) exceeded 90s p95 — ADR-0033 §4 budget. | §3 |

---

## 9. Escalation & Related References

### When to escalate

| Escalation target | Trigger |
|-------------------|---------|
| operations | A layer reports `RESULT=UNKNOWN` (dispatcher / wrapper bug — child crashed before STATUS line). |
| operations | `BASE_REF=` line missing from a layer's stderr log (resolver did not run; layer-skeleton regression). |
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

## 10. Changelog

| Date | Author | Changes |
|------|--------|---------|
| 2026-05-14 | operations (task #39) | Initial creation. Documents `scripts/layer-all.sh` + Layers 1-7 + shared helpers as of commit `0130ce8`. Closes ADR-0033 Wave 3 #8. |
