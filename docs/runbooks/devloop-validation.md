# Runbook: Devloop Validation Pipeline Failures

**Pipeline**: Polyglot Validation (`scripts/layer-all.sh` + Layers 1-7)
**Owner**: Operations Team (with infrastructure pairing for wrapper-script edits)
**History**: `git log --follow -- docs/runbooks/devloop-validation.md`

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
   LAYER=2 RESULT=OK             DURATION=1
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
| `./scripts/layer-fast.sh` | Layers 1–6, no cluster, no Gate-2 verdict. The implementer runs it before "Ready for review" and after review fixes; the Lead runs it as the fast check before review. |
| `./scripts/layer-all.sh` | Full validation (every layer in order). Gate 2: the Lead runs it once, after Gate 3 approves (ADR-0037 D10). |
| `./scripts/layerN.sh` | Re-run a single layer (e.g. `./scripts/layer4.sh` to re-run only Layer 4 on a failing diff). Each layer is independently invocable. |
| `./scripts/{audit,build,fmt,lint,test}.sh` | Per-verb dispatcher (e.g. `scripts/test.sh --workspace`). Iterates `scripts/lang/<X>/<verb>.sh` via `_dispatch.sh::for_each_lang_with_verb`. Preserves muscle-memory: `scripts/test.sh` keeps its original CLI shape. |
| `bash scripts/lang/<X>/<verb>.sh` | Direct invocation of a single language's wrapper. Bypasses the dispatcher's aggregation logic — useful for isolating "is the wrapper itself broken?" from "is the dispatcher routing correctly?". |

`scripts/verify-completion.sh` is the historical entry point; post-Wave-1 it calls `scripts/layer-all.sh` for the body (router-drift between local and CI is structurally eliminated).
### 2.1 `git checkout <path>` destroys uncommitted work — never use it to undo a temporary edit

**Indexed by the OPERATION, deliberately.** An earlier version of this section was indexed by one
scenario (validating another owner's pending change) and one victim ("another owner's work"). It
was read, and then violated in the same session, by someone undoing their own two-minute edit —
because "someone else's uncommitted work" is not the frame you are in when the file is yours. If
you are about to run `git checkout` on a path you have edited, this section applies to you
regardless of why you edited it.

**The invariant, before any scenario: `git checkout [--] <path>` reverts that path to HEAD.** It
does not revert to "the state before my edit", it cannot see how much of the delta is yours, and
there is no confirmation and no reflog entry for working-tree content. Whatever was uncommitted in
that file is gone. The `--` is optional and its absence changes nothing — `git checkout <path>` and
`git checkout -- <path>` are equally destructive.

**Two triggers, both real, both in this repo:**

**(a) Mutation testing — the common case, and the one that feels safest.** You break something
deliberately to prove a test reds, watch it red, then want the break gone. Your intent genuinely
*is* "undo my edit", `git checkout` is the natural expression of that intent, and nothing in the
moment surfaces the gap between "before my edit" and "HEAD". This is how
`crates/dt-guard/src/metric_labels.rs` lost 293 lines of uncommitted review work on 2026-09-08
(`docs/devloop-outputs/2026-09-08-media-telemetry-deny-policy/` §Issues Encountered) — during the
verification of a test whose purpose was catching silent regressions. **Every review round that
proves a test is not vacuous runs this exact procedure**, which makes it the most frequent way to
reach this hazard, not the rarest.

**(b) Running a layer against a scratch tree state.** You need the pipeline to see a state the tree
is not in yet — applying another owner's pending change locally to validate against the end state,
then putting the tree back. Here the destruction reaches beyond you: in a shared working tree
`git checkout` cannot distinguish your scratch delta from somebody else's **uncommitted** work on
the same file, and it destroys both. Wrapping it in an `EXIT` trap — the obvious way to guarantee
cleanup fires on success, failure *and* interrupt — makes the destruction unconditional and silent
rather than occasional. **The guarantee is the hazard**: a cleanup that always runs, always runs
against whatever is there.

This is not hypothetical. It happened during
`docs/devloop-outputs/2026-09-05-sdk-frame-v2-sframe-stack/`: a `trap "git checkout -- \
crates/media-vector-gen/src/inventory.rs proto/test-vectors/frame-v2.vectors.json" EXIT` cleaned up
a scratch flip and took another specialist's uncommitted, unlanded work with it. Recovery was only
cheap because the destroyed work was deterministically regenerable and a second reviewer re-derived
the crypto over the restored bytes rather than trusting the diff — do not assume your case is that
lucky.

**Do this instead — snapshot the exact prior bytes and restore from the snapshot.**

*One file, one mutation (trigger (a)) — this is the whole remedy, and it is two lines:*

```bash
cp crates/dt-guard/src/metric_labels.rs /tmp/mutation.bak
# ... break it, run the test, watch it red ...
mv /tmp/mutation.bak crates/dt-guard/src/metric_labels.rs
```

Sized deliberately. The multi-file recipe below is correct for trigger (b) but looks
disproportionate next to "I am about to break one file for thirty seconds", and **a remedy that
looks heavier than the task is a remedy that gets skipped** — which is most of how the 2026-09-08
loss happened. There is no case small enough to justify `git checkout`; there is only a smaller
snapshot.

*Multiple files, or a scratch state you will run a whole layer against (trigger (b)):*

```bash
snap="$(mktemp -d)"
cp --parents crates/media-vector-gen/src/inventory.rs \
             proto/test-vectors/frame-v2.vectors.json "$snap"/
trap 'cp -a "$snap"/./* . && rm -rf "$snap"' EXIT
# ... apply the scratch change, regenerate, run the layer or guard ...
```

The restore target is now *the bytes that were there*, which is the thing you actually meant, and it
is correct whether or not anyone else holds uncommitted changes. Keep the `EXIT` trap — the fix is
the restore SOURCE, not the trigger.

**Before you start, check whether you are alone in those files**: `git status --short -- <paths>`.
If another owner holds uncommitted changes there, prefer waiting or a separate worktree
(`.claude/skills/worktree-setup`) over any in-place scratch technique — the snapshot protects
against your own cleanup, not against two writers racing the same file.

---

## 3. Exit-Code & STATUS Enum Reference

### Exit codes (ADR-0033 §6 wrapper contract)

| Exit | Meaning | Maps to STATUS |
|------|---------|----------------|
| **0** | PASS or SKIPPED (success-exit class) | `OK | SKIPPED-NO-DIFF | SKIPPED-NO-CLUSTER | N/A | SKIPPED-NO-VERB` |
| **1** | FAIL — work ran and detected a problem | `FAIL` |
| **2** | Wrapper/dispatcher/orchestrator bug — investigate the script itself (dispatcher misconfig; a verb wrapper that should exist is missing/non-executable; an empty/misdirected lang root; OR a pre-layer guardrail tripped, e.g. shallow CI clone) — and the `PRECONDITION_FAILURE` operator lane | `UNKNOWN`; `FAIL-MISSING-VERB`; `PRECONDITION_FAILURE` |

**`FAIL-MISSING-VERB` (task #52, 2026-06-19)** is the enum for a verb wrapper that should
exist but is missing/non-executable (deleted, `chmod`-stripped, or a new lang dir added
without the verb) — a wiring fault, mapped to **exit 2** alongside `UNKNOWN`. It ranks
ABOVE `OK` in the aggregation ladder, so a missing wrapper can no longer be masked by a
sibling lang's clean run. `SKIPPED-NO-VERB` now means ONLY `all-langs-filtered` (operator
intent, exit 0); an *intentional* gap (proto's absent `test.sh`/`audit.sh`) is registered
as a placeholder wrapper emitting `N/A` (exit 0), NOT a missing verb. A lang root with no
language directory at all is also `FAIL-MISSING-VERB` (`REASON=no-languages-registered`, exit 2
— since 2026-10-06; it used to be `N/A`). See §7 and ADR-0033 §6 (2026-06-19 and 2026-10-06
amendments).

### `STATUS=` enum (ADR-0033 §6)

Every wrapper emits a final stdout line of the form:

```
STATUS=<enum> REASON=<short-token-no-spaces>
```

The enum values are exactly:

| STATUS | Meaning | Typical REASON examples |
|--------|---------|-------------------------|
| `OK` | Work ran cleanly | `cargo-build-passed`, `buf-build-passed`, `guards-passed` |
| `FAIL` | Work ran and detected a problem | `cargo-clippy-failed`, `buf-breaking-failed`, `env-tests-failed`, `browser-e2e-failed`, `provision-failed` / `deploy-failed` (Layer 7: provision / deploy failed on the tree) |
| `SKIPPED-NO-DIFF` | the Layer-6 audit dep-gate when no dependency manifest changed (the COMMON case — every source-only PR emits this; see §6.6). **This is now the ONLY producer**: the language-level `<lang>-no-diff` short-circuit and Layer-7's `browser-e2e-no-diff` lane were both retired 2026-08-20 (every language, and the browser suite, now always-run). The dispatcher's aggregate `all-langs-skipped` fires only if every audit child reports no-dep-changes. | `no-dep-changes`, `all-langs-skipped` |
| `SKIPPED-NO-VERB` (→ **exit 0**) | `all-langs-filtered` — an INCLUDE/EXCLUDE filter cleared the lang set (operator intent). The ONLY producer of this enum since task #52. | `all-langs-filtered` |
| `FAIL-MISSING-VERB` (→ **exit 2**) | a verb wrapper that SHOULD exist is missing/non-executable (deleted, `chmod`-stripped, or a new lang dir missing a verb), or the lang root has no language directory at all — a wiring fault, not a benign skip. Ranks above OK, so it can't be masked. | `rust-test-verb-missing-or-not-executable`, `ts-audit-verb-missing-or-not-executable`, `no-languages-registered` |
| `N/A` | Documented gap: a verb that doesn't apply to a lang — emitted ONLY by the intentional-gap placeholders `proto/test.sh` / `proto/audit.sh` (an allow-list test in `_dispatch.test.sh` pins that). Ranks just below OK, above the SKIPPED-* statuses: beside a real OK it reads OK; a layer reads `N/A` only when no child did real work. | `not-applicable-to-this-lang`, `<verb>-aggregate-na` |
| `SKIPPED-NO-CLUSTER` (→ **exit 0**) | Layer 7 only, **CI only** (`GITHUB_ACTIONS` set): no Kind cluster, provisioning out of scope. The ONLY clean-skip case; ranks BELOW OK so a green CI run reports `TOTAL_RESULT=OK`. A *local* run with no/dead helper is NOT this lane — it's `PRECONDITION_FAILURE` (exit 2). | `no-cluster-ci` |
| `PRECONDITION_FAILURE` (→ **exit 2**) | The ENVIRONMENT a gate needs was unavailable — a machine fact, not a diff defect. **Layer 7**: a Phase-1 pre-suite step (helper liveness / provision / deploy / health / browser-suite preconditions / per-run-org provisioning) failed, OR a LOCAL run with no/dead helper. **Layer 3** (as of 2026-08-21): a guard hit its per-guard timeout (`GUARD_TIMEOUT_SECS`, default 30s) or was SIGKILLed after the grace window — machine contention / OOM, not a code defect. The OPERATOR lane; ranks above `FAIL`, below `FAIL-MISSING-VERB`. Does NOT consume an implementer attempt (task #56) — **but see §6.3: a guard timeout that REPRODUCES on retry or a quiet machine is diff-caused and routes to the implementer.** | **L7**: `local-helper-not-running`, `helper-unreachable`, `cluster-provision-failed`, `cluster-deploy-failed`, `helper-verb-unsupported`, `ports-json-missing`, `cluster-unhealthy`, `cluster-self-heal-failed`, `cluster-self-heal-probe-inconclusive`, `dev-certs-missing`, `playwright-browser-missing`, `org-provision-context-unresolved`, `org-provision-timeout`, `org-provision-failed`, `ac-unreachable`, `ac-auth-bypass-signature`, `org-provision-unverified`, `mh-grpc-port-unallocated`, `mh-pod-unresolved`, `mh-grpc-port-occupied`, `mh-grpc-forward-not-ready`. **L3**: `guard-timeout-<name>`, `guard-timeout-kill-<name>` |

To find a missing-wrapper wiring fault, grep `FAIL-MISSING-VERB` (or the
`verb-missing-or-not-executable` REASON). An intentional gap shows up as `N/A` from a
placeholder wrapper (task #52) — distinct enum, distinct exit code, no infix ambiguity.

**`REASON=` and `SELF_HEAL CASE=` are two deliberately-overlapping token families.** The Layer-7 self-heal classifies a `SELF_HEAL CASE=` (its own case enum) and then, where it escalates, emits a `precondition_fail` `REASON=`. Some spellings appear in BOTH on purpose — `helper-unreachable` is a `SELF_HEAL CASE=` value AND a `REASON=` token (the self-heal reuses the existing REASON), while `CASE=probe-inconclusive` maps to the deliberately DIFFERENT `REASON=cluster-self-heal-probe-inconclusive`. Do not merge or "align" the two families; the full mapping is in §6.7.

`UNKNOWN` is **not** a wrapper-emitted enum — it appears in the aggregator when a child wrapper crashes before emitting `STATUS=` AND its EXIT trap did not fire (e.g. killed `-9`), or when stdout streaming breaks. Since task #50 the verb wrappers self-emit `STATUS=FAIL REASON=wrapper-aborted-early-exit-<rc>` via an EXIT trap when they abort before emitting, so a true `UNKNOWN` is now genuinely exceptional. `UNKNOWN` ranks at the TOP of the precedence ladder (above `FAIL-MISSING-VERB`) because it signals a dispatcher/wrapper bug, not a real-work failure; it maps to exit 2 — the same wiring-fault exit class as `FAIL-MISSING-VERB`.

### Worst-child STATUS aggregation

Each layer collects every child `STATUS=` line that came across stdout via `_common.sh::tee_collect_statuses`, then aggregates with `_common.sh::aggregate_worst_status`, worst wins. **The ladder is `_common.sh::__status_rank` — read it there**; its precedence comment above `aggregate_worst_status` is the rationale. This runbook deliberately does not copy the order (a printed copy drifts silently). The two properties to triage by: every exit-0 status other than `OK` ranks below `OK`, so a green run reports `TOTAL_RESULT=OK`; anything that means "a gate could not run or found a problem" ranks above it, so it can't be masked by a sibling's pass.

**Reading a top-line `TOTAL_RESULT=N/A`** (since 2026-10-06): it means NO layer did real work anywhere — every layer was a placeholder or a skip. On a normal run that is not green; investigate (e.g. a run limited to layers whose only children are placeholders). A single `LAYER=n RESULT=N/A` beside other OK layers is normal: e.g. Layer 6 on a no-dependency-change run (below).

**Worked example — Layer 1 stage-2 (multi-lang)**:

```
STATUS=OK REASON=cargo-build-passed         (rust)
STATUS=OK REASON=nx-typecheck-passed        (ts)
STATUS=FAIL REASON=buf-build-failed         (proto, stage 1)
→ aggregate_worst_status OK OK FAIL = FAIL
→ Layer 1 final STATUS=FAIL REASON=layer1-summary
→ exit code 1 (status_to_exit_code FAIL)
```

### `LAYER=…` stderr summary line

Every layer emits (via the EXIT trap installed by `_common.sh::layer_lifecycle_begin`):

```
LAYER=<n> START=<unix-ts> END=<unix-ts> DURATION=<s> RESULT=<enum> REASON=<reason>
```

This is the **layer-level anchor** for greppable triage in `${DEVLOOP_TMP:-/tmp/devloop}/layer-<n>.stderr.log`. EXIT-trap emission is guaranteed even under `set -e` abort or signal-kill — a runbook reader who hits "the orchestrator died mid-layer" still sees the partial layer state in stderr.

**REASON field — stderr carries the cause, stdout carries the summary (task #50/#52).** The
`REASON=` on THIS stderr `LAYER=` line is the **worst-child attributable cause** — e.g.
`<lang>-<verb>-verb-missing-or-not-executable`, `wrapper-aborted-early-exit-<rc>`,
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
WARN BUDGET_TOTAL_BREACH GUARD_AUDIT_DURATION=<s> BUDGET=90    (guard+audit fast tier, layers 3 + 6; ADR-0033 §4 budget)
```

`WARN BUDGET_*` is informational only — it does not change exit code. A breach is the signal to revisit budgets (ADR-0033 §4 budget target; §14 flake-rate budget for adjacent context) or investigate a regression.

### Fail-fast vs run-all (`layer-all.sh`, 2026-08-21)

`layer-all.sh` runs one of two modes, decided by `_common.sh::fail_fast_mode()` and announced on
stderr as `PIPELINE_MODE=<fail-fast|run-all> SOURCE=<label>` at the start of every run:

- **fail-fast** (interactive default): STOP at the first layer whose process exits non-zero.
  Layers after it render `RESULT=NOT-RUN` and a `STOPPED_EARLY LAYER=<n> RESULT=<enum> NOT_RUN=<list>`
  line is emitted. Purpose: don't spend Layers 4-7 (incl. Layer 7's ~10-15 min cluster bring-up) on a
  red you already know about.
- **run-all** (unattended default): run all seven layers, accumulate worst-wins — one pass reports
  everything broken. This is what CI (`GITHUB_ACTIONS`) and the story runner / headless devloop
  sessions (`DEVLOOP_HEADLESS`) get, and it is the pre-2026-08-21 behavior.

**Knob — `DEVLOOP_FAIL_FAST`** (precedence, top wins; the authoritative table is in
`_common.sh::fail_fast_mode`): a malformed value → exit 2 loud (validity checked first, even in CI);
an explicit `1` under an unattended lane is REFUSED (`WARN FAIL_FAST_OVERRIDE_IGNORED`) — an ambient
`=1` must not shrink an authority run's coverage; otherwise explicit `1`→fail-fast, `0`→run-all;
unattended→run-all; interactive→fail-fast. Empty string = unset.

**Why fail-fast cannot forge a Gate-2 pass** (the property to internalize): a GREEN run is
byte-identical under both modes — there is nothing to stop at, so fail-fast never triggers. The stop
predicate is `rc != 0` (never status-based), un-run layers render `NOT-RUN` and NEVER aggregate or map
to exit 0, and `TOTAL_RESULT`/`LAYER_ALL_EXIT` reflect ONLY the layers that ran (through the failing
one). So fail-fast can report an equal-or-earlier failure, never a greener one, and can never exit 0
over layers that never ran (`emit_gate2_verdict` derives `GATE2=PASS` solely from `LAYER_ALL_EXIT==0`).

**Un-run ≠ skipped.** `RESULT=NOT-RUN` is a display-only marker, distinct from the `SKIPPED-*` enum
family (which are real exit-0 states that DID run their check). An implementer must NOT rationalize a
`NOT-RUN` layer at Gate 2 the way a `SKIPPED-NO-DIFF` can be — it means "we never looked", not "clean".

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

Emitted by `scripts/layer-all.sh` (the pre-loop guardrail near the top of the script, AND — as of 2026-08-21 — its `DEVLOOP_FAIL_FAST`-invalid guardrail and the fail-fast `fail-fast-exit-invariant` assertion), by `scripts/layer7.sh` (its `precondition_fail` helper, on the operator lane — cluster/helper bring-up failures, task #56), and — as of 2026-08-21 — by `scripts/guards/run-guards.sh` (its `classify_guard_exit` 124/137 arms, on a guard timeout/SIGKILL, relayed into `layer-3.stderr.log`). All indicate a precondition is not met. layer-all.sh's pre-loop checks exit 2 with the stderr token ALONE (they abort before the summary table exists); layer7.sh and run-guards.sh's timeout arms additionally emit `STATUS=PRECONDITION_FAILURE` on stdout (a first-class enum, §6 ladder) so the Layer-7 / Layer-3 summary-table row reads PRECONDITION_FAILURE, not a misleading UNKNOWN.

A **further emitter** (not a layer script) is `infra/kind/scripts/deploy.sh`'s `check_build_disk_space` (host-side, run by the ADR-0030 helper's `deploy`), which emits a line-anchored `PRECONDITION_FAILURE: … REASON=insufficient-disk` on stderr + `exit 2` before a doomed cold image build. Unlike the layer emitters it is NOT a layer wrapper, so it does NOT emit a `STATUS=` enum — its banner is *relayed* through the helper into Layer 7's Phase-1 stderr, where it surfaces as a SUB-CAUSE under layer7's own `cluster-deploy-failed` enum (see §6.7). The relayed banner still lands in `${DEVLOOP_TMP:-/tmp/devloop}/layer-7.stderr.log`, so the one-pass grep below finds it. The layer-all guardrail example:

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

**These tokens are line-anchored BECAUSE they are emitted by the pipeline itself.** The anchoring is safe here and is not safe for the underlying tools' own output — see the next section before you write a grep against `cargo`, `pnpm` or `buf`.

### Never triage a tool by grepping its stdout — read the exit code and the STATUS token

**Build tools colourise through a pipe.** `cargo` emits SGR escapes whether or not stdout is a TTY, so a hard failure line is not `error[E0308]` on the wire — it is `\x1b[1m\x1b[91merror\x1b[0m[E0308]`. A `grep -E '^error'` over that matches **zero lines against a failing build**, and zero matches is indistinguishable from zero errors. The same applies to `^warning`, `^test result`, and to `Finished`/`Compiling` progress lines.

This is not theoretical and it is not a beginner's mistake: it produced a false "lint clean" claim during the 2026-09-08 media-telemetry-deny devloop, in a loop whose entire subject was matchers narrower than the thing they match reporting clean. The person who wrote the grep had run the correct command.

**The rule:**

- **Triage by exit code and by the `STATUS=…REASON=…` token.** Both are ASCII, both are the wrapper contract (ADR-0033 §6), and `run_and_emit` produces them for exactly this reason. `status_to_exit_code` in `scripts/lang/_common.sh` is the SSoT.
- **If you must grep tool output, defeat the colour first** — `CARGO_TERM_COLOR=never`, or `--message-format=short`, or pipe through `sed -r 's/\x1b\[[0-9;]*m//g'`. Prefer the first: it removes the escape at the source rather than filtering it afterwards.
- **Never let a zero-match grep stand in for a pass.** If your check cannot distinguish "found nothing wrong" from "matched nothing", it is not a check. Assert on the exit code, which has no such ambiguity.

### Convention extends to future precondition checks

Future precondition checks added to `layer-all.sh` OR to a `layerN.sh` (disk-space, env-var presence, container-runtime availability, cluster bring-up, etc.) inherit the `PRECONDITION_FAILURE:` token. This runbook is the canonical home for the convention; task #42 §Tech Debt Pointers entry 4 is the source, and task #56 (Layer 7) is the first `layerN.sh` emitter — see §6.7 for its REASON tokens.

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

### Diagnosing what a layer's diff-aware gates saw

The resolver writes `${DEVLOOP_TMP}/changed-files.layer-<n>` — the cache-write block in `_get_base_ref.sh::main`. The surviving diff-aware consumers read it via `_changed_helpers.sh::__changed_files` (which lazy-invokes the resolver if the cache is missing): the Layer-6 audit dep-manifest gate (`_audit_gate.sh`'s `diff_touches_glob`); Layer 7 inspects no diff since ADR-0038 step 3. (The per-language `changed.sh` classifiers that used to read this cache were retired 2026-08-20 — ADR-0033 §2/§3.)

To inspect what a layer actually saw:

```bash
cat "${DEVLOOP_TMP:-/tmp/devloop}/changed-files.layer-<n>"
```

To re-populate the cache hermetically:

```bash
DEVLOOP_LAYER=manual bash scripts/lang/_get_base_ref.sh >/dev/null     # populates cache + emits BASE_REF= line
```

### Known cost concern (informational)

`_get_base_ref.sh` runs on every layer-entry AND on every guard that calls `get_diff_base` (the 1-line forwarder in `scripts/guards/common.sh`). Total invocations per pipeline run: ~24-36; CPU cost ~1.2-1.8s. Each invocation re-emits the `BASE_REF=` stderr line — observability dashboards that count "pipeline runs" by `BASE_REF=` emission will multiply runs by ~30×. Mitigation is tracked but not yet implemented (cache + `__emit_base_ref_line` suppression-sentinel). See `docs/devloop-outputs/2026-05-13-base-ref-unification-task42/main.md` §Tech Debt Pointers entry 2.

---

## 6. Layer-by-Layer Failure Modes

Each subsection covers one layer: what it runs (every language, always-run since 2026-08-20), common failure modes (each anchored at the emitting wrapper script + the REASON token), and the canonical fix vocabulary.

### 6.1 Layer 1 — Compile (`scripts/layer1.sh`)

Two-stage compile (ADR-0033 §5):
- **Stage 1**: proto-only via `scripts/build.sh` with `DEVLOOP_DISPATCH_INCLUDE_LANGS=proto` → `lang/proto/compile.sh` (`buf build proto`). Runs first so contract failures surface ahead of Rust/TS type-error cascades.
- **Stage 2**: rust + ts via `DEVLOOP_DISPATCH_EXCLUDE_LANGS=proto` → `lang/rust/compile.sh` (`cargo build --workspace --quiet`, plus release builds of `dt-guard` and `dt-story` and the release-feature gate) + `lang/ts/compile.sh` (`nx run-many -t typecheck --all`).
  `cargo build`, not `cargo check`, is deliberate — the wrapper's own header says it "catches link-time errors `cargo check` misses".

Both stages route through the dispatcher (`scripts/build.sh` → `_dispatch.sh::for_each_lang_with_verb "compile"`) so always-run dispatch, STATUS aggregation, and missing-verb signalling apply uniformly.

**Always-run**: rust, ts, proto compile on every run (per ADR-0033 §3 — the skip-if-untouched short-circuit was retired 2026-08-20).

**Common failures**:

| REASON token | Wrapper | Cause / Fix |
|--------------|---------|-------------|
| `buf-build-failed` | `lang/proto/compile.sh` | Malformed `.proto`. Run `buf build proto` locally; the error names the file + line. |
| `buf-binary-missing` | `lang/proto/compile.sh` (also `fmt.sh`, `lint.sh`, `breaking.sh`) | `buf` CLI not installed locally. Install via the project's documented setup; CI has it baked into the runner image. |
| `cargo-build-failed` | `lang/rust/compile.sh` | Type / borrow / use / link error. Run `cargo build --workspace` locally for the full error chain. |
| `cargo-build-dt-guard-failed` | `lang/rust/compile.sh` | The `target/release/dt-guard` binary did not build. **Layer 3's guard wrappers consume it**, so this is the upstream cause of §8's "a missing guard binary reaches the implementer lane" caveat — fix here, not there. |
| `cargo-build-dt-story-failed` | `lang/rust/compile.sh` | The `target/release/dt-story` binary did not build. Same consumer relationship as `dt-guard` above. |
| `release-feature-gate-failed` | `scripts/release-feature-gate.test.sh` | The ADR-0036 §11 release-build gate self-test failed. **Five distinguishable causes and they do NOT share a fix — read the assertion label before triaging**, see the sub-table below. |
| `nx-typecheck-failed` | `lang/ts/compile.sh` | `tsc --noEmit` error reported via `nx run-many -t typecheck --all`. Run `pnpm exec nx run-many -t typecheck --all` locally. **The `nx affected` form is retired** (2026-08-20 always-run change) — an `--base=…` repro reproduces something the pipeline no longer runs. |
| `nx: command not found` | `lang/ts/compile.sh` (also Layer 2 / 4 / 5 TS wrappers) | Local-only failure mode — CI has `corepack` / `pnpm install` in setup. Fix: `pnpm install` from repo root (nx is a project-local dev dep, not a global tool). |

**Triaging `release-feature-gate-failed`** (ADR-0036 §11 self-test;
`scripts/release-feature-gate.test.sh`, wired here rather than in Layer 3 because
compiler cost belongs outside the fast tier per ADR-0033 §4):

| Failing assertion / token | What it means | Fix |
|---|---|---|
| `…/fire-arm-exits-non-zero` | **The control did not fire.** A release-profile build with `--features per-frame-trace` COMPILED. | Check `[profile.release]` in the root `Cargo.toml` for a `debug-assertions = true` that makes the `not(debug_assertions)` predicate inert, and check the `#[cfg]` at `crates/mh-service/src/lib.rs`. |
| `…/fire-arm-cites-the-compile_error-message` | The build failed but **without the expected diagnostic**. Either the control did not fire and something else broke, or the `compile_error!` message was reworded. | Read the captured build output first — it names the real error. If the message was merely reworded, update `NEEDLE_PHRASE` in the self-test to a phrase lying wholly within one source line. |
| `…/fire-arm-originates-in-the-gate-source` | The diagnostic came from **a dependency, not from mh-service's `lib.rs`**. **The control may be perfectly healthy** — something else in the build broke. | Do NOT go auditing `[profile.release]` or the `#[cfg]`. Read the build output and fix the unrelated breakage; re-run. |
| `…/fire-arm-is-not-a-feature-name-typo` | **A defect in the TEST, not in the gate or the profile.** Cargo rejected the feature name at resolution, so `compile_error!` never expanded. | Fix the feature name in the `GATES` table in `scripts/release-feature-gate.test.sh`. Nothing about mh-service or the release profile is implicated. |
| `…/clean-arm-succeeds-without-feature` | **The tree is broken for an unrelated reason** — mh-service does not compile under `--release` even without the feature. The fire arm proves nothing until this is fixed. | Usually already announced by `cargo-build-failed` from the `cargo-build` invocation earlier in this same wrapper; fix that first. |
| `STATUS=PRECONDITION_FAILURE REASON=release-feature-gate-*` | **Operator lane, not the diff.** `cargo` missing, registry/network/disk failure, or the pinned `compile_error!` phrase no longer found in the gate's source. | Read the accompanying message — it names which. For `…-needle-not-found`, do **not** relax the pin; if the message was merely reflowed, move the pinned phrase to one lying wholly within a single source line. |

**Note the log carries two lines on a precondition exit.** `run_and_emit` maps any
non-zero rc to `STATUS=FAIL REASON=release-feature-gate-failed`, so a
`PRECONDITION_FAILURE` appears alongside a `STATUS=FAIL`. Worst-status aggregation still
lands the layer on `PRECONDITION_FAILURE` and exit 2, so the operator lane is correct —
but an operator who greps `STATUS=FAIL` first has been sent to the implementer lane by an
artifact of the wrapper rather than by a verdict. Read the `PRECONDITION_FAILURE` line.

**Budget note**: Layer 1 exceeds the 20s per-layer warn on any cold cache; this is
expected and dominated by the workspace build, not by the release-feature gate.

**Worked example — proto stage-1 fail; rust + ts still compile (stage 2 always-runs)**:

```
STATUS=FAIL REASON=buf-build-failed             (proto, stage 1; per-child stdout)
STATUS=OK REASON=cargo-build-passed             (stage 2, rust — always-runs)
STATUS=OK REASON=nx-typecheck-passed            (stage 2, ts — always-runs)
STATUS=FAIL REASON=layer1-summary               (aggregated stdout summary line)
LAYER=1 ... RESULT=FAIL REASON=buf-build-failed (stderr anchor; worst-child cause)
```

Stage 2 runs unconditionally even when stage 1 fails (proto-derived codegen may be stale, but rust/ts compile is still attempted so one run reports the full picture) — and since 2026-08-20 rust/ts compile always-run regardless of diff, so there is no `SKIPPED-NO-DIFF` case here any more.

Two distinct final lines (task #50): the **stdout** `STATUS=` summary keeps the generic
`REASON=layer<n>-summary` (downstream parsers read it for the enum only); the **stderr**
`LAYER=…RESULT=…REASON=` anchor now carries the WORST-CHILD reason (`buf-build-failed`
here), not `layer<n>-summary` — so a non-zero layer names its real cause for 3am triage
(observability P2). The stage-2 dispatch runs unconditionally even on stage-1 fail —
observability O2 (one run reveals the full picture; don't force a second invocation).

### 6.2 Layer 2 — Format (`scripts/layer2.sh`)

`scripts/fmt.sh` → `for_each_lang_with_verb "fmt"` → `lang/{rust,ts,proto}/fmt.sh`.

**Always-run**: rust, ts, proto — every language, every run (per ADR-0033 §3; the skip-if-untouched short-circuit was retired 2026-08-20).

**The APPLY-vs-CHECK lane (ADR-0037 §D7).** Layer 2 is tri-state, and the lane is decided in ONE place — `fmt_mode()` in `scripts/lang/_common.sh` (the SSoT; no wrapper re-derives it — the `fmt-lane-ssot` guard enforces that). Every fmt wrapper prints a greppable anchor to stderr:

```
FMT_MODE=<apply|check|invalid> SOURCE=<why>
```

- **CHECK** (the default, and forced in CI and at every tree-attesting gate) is read-only: it fails loud on drift and never rewrites a file.
- **APPLY** rewrites drift in place, prints `FMT_APPLIED=<comma-separated repo-relative paths>` (proto also prints a louder `WARN PROTO_FMT_APPLIED FILES=…`, since it writes a Guarded Shared Area), and **still emits `STATUS=OK`** — auto-applying a semantically-empty formatter is not a failure (ADR-0033 contract). **`FMT_APPLIED=` is an advisory breadcrumb, not an authenticated record**: the authoritative answer to "what did this run write to `proto/**`" is the signed `proto/**` file records in the Gate-2 verdict (`scripts/lang/_gate2_binding.sh`), not this line.

Precedence, high to low: `INVALID > check-only-override (DEVLOOP_FMT_CHECK_ONLY) > CI (GITHUB_ACTIONS|CI) > apply-opt-in (DEVLOOP_FMT_APPLY) > CHECK-default`. So an ambient `DEVLOOP_FMT_APPLY` can never make CI or an attesting gate write the tree — the check-only override and the CI signal both out-rank it (fail-closed).

**Knobs (operator-facing):**

| Env | Effect | Set by |
|-----|--------|--------|
| `DEVLOOP_FMT_APPLY=1` | opt into APPLY (local only; a no-op in CI / under a check-only override) | the interactive devloop's Gate-2 invocation (`SKILL.md`) |
| `DEVLOOP_FMT_CHECK_ONLY=1` | force CHECK regardless of the apply opt-in | the run-story authority/close gates + `.githooks/pre-commit`; out-ranks the opt-in |

A bare local `bash scripts/layer2.sh` is therefore CHECK (`SOURCE=check-default`) — to auto-fix locally, prefix `DEVLOOP_FMT_APPLY=1` (the devloop does this for you).

**Proto note (COLLAPSE, ADR-0037 §D7).** All proto buf calls run via `pnpm exec buf` (lockfile-pinned `@bufbuild/buf`), never a bare PATH `buf`. A preflight (`lang/proto/_buf.sh`) asserts the toolchain is present and the running buf version EXACTLY equals the lockfile pin BEFORE the format check — so a stale local `node_modules` reds as `buf-version-mismatch` (remedy: `pnpm install --frozen-lockfile`), not as spurious format drift. A `@bufbuild/buf` bump is a reviewed GSA event (excluded from dependabot's grouped bumps); it may reformat `proto/**` and requires regenerating the golden fixture as the evidence of what changed.

| REASON token | Wrapper | Cause / Fix |
|--------------|---------|-------------|
| `cargo-fmt-passed` / `buf-format-passed` / `nx-format-passed` | any | CHECK clean, or APPLY found nothing to rewrite. |
| `cargo-fmt-applied` / `buf-format-applied` | rust / proto | APPLY rewrote drift; see the `FMT_APPLIED=` line for the files. Not a failure — review + stage them. |
| `cargo-fmt-failed` | `lang/rust/fmt.sh` (`cargo fmt --all -- --check`) | CHECK-mode format drift. Fix: re-run locally with `DEVLOOP_FMT_APPLY=1`, or `cargo fmt --all`. |
| `cargo-fmt-unparseable` | `lang/rust/fmt.sh` | rustfmt hit a **parse error** (rc=1, empty file list), NOT drift — nothing was applied. Fix the syntax error rustfmt printed (it carries `file:line`). |
| `nx-format-failed` | `lang/ts/fmt.sh` | Prettier drift in CHECK mode. Fix: re-run with `DEVLOOP_FMT_APPLY=1`, or `pnpm exec nx run-many -t format --all -- --write`. |
| `buf-format-failed` | `lang/proto/fmt.sh` (`pnpm exec buf format --diff --exit-code proto`) | CHECK-mode proto drift. Fix: re-run with `DEVLOOP_FMT_APPLY=1`, or `pnpm exec buf format -w proto`. |
| `buf-format-parse-error` | `lang/proto/fmt.sh` | buf hit a **parse error** (rc=1) — invalid `.proto`; nothing applied. Fix the syntax buf printed (`Failure: file:line`). |
| `buf-format-postcondition-failed` | `lang/proto/fmt.sh` | APPLY ran but a re-check still shows drift (buf rewrote some but not all). Investigate — do NOT treat as applied. |
| `pnpm-unavailable` | `lang/proto/_buf.sh` | `pnpm` not on PATH; the proto lane runs `pnpm exec buf` and does NOT fall back to a bare buf. Fix: `pnpm install --frozen-lockfile`. |
| `buf-not-installed` | `lang/proto/_buf.sh` | `pnpm exec buf` did not resolve. Fix: `pnpm install --frozen-lockfile`. |
| `buf-version-underivable` | `lang/proto/_buf.sh` | Could not read the `@bufbuild/buf` pin from `package.json` (fail-closed). The declaration moved/changed shape. |
| `buf-version-mismatch` | `lang/proto/_buf.sh` | Running buf ≠ the lockfile pin (stale local `node_modules`). Fix: `pnpm install --frozen-lockfile` — do NOT reformat. The version probe bypasses pnpm's verify step (`_pnpm.sh:pnpm_exec_unverified`), so a stale buf still lands here, not on `buf-not-installed`. |
| `pnpm-deps-stale` | `lang/_pnpm.sh:pnpm_deps_fresh` (proto preflight state 5; TS compile/lint/test/fmt) | `node_modules` no longer matches the lockfile, and `pnpm-workspace.yaml` `verifyDepsBeforeRun: error` makes `pnpm exec` refuse (`ERR_PNPM_VERIFY_DEPS_BEFORE_RUN`; pnpm's message follows the STATUS line). Typical after pulling a lockfile/`package.json` change into a long-lived devloop container, whose entrypoint installs only when `node_modules/.bin/nx` is absent. Fix: `pnpm install --frozen-lockfile`. |
| `fmt-mode-invalid-knob` | any (via `fmt_mode()`) | `DEVLOOP_FMT_APPLY`/`DEVLOOP_FMT_CHECK_ONLY` set to a non-boolean/empty value. Config fault: set the knob to `1`/`0`/unset. |

> **Cross-ref — `FINISH-DIRTY-REMAINDER` at story close.** If `--finish` refuses with an unstaged file outside the intent set, a prior *interactive* devloop's APPLY may have left formatted-but-unstaged files. That is expected: the attesting gates run CHECK-only and never leave the tree dirty; only a local APPLY does. Review and stage (or discard) the `FMT_APPLIED=` files, then re-run. See §6.7.

### 6.3 Layer 3 — Guards (always-run; `scripts/layer3.sh`)

Key `run_and_emit` invocations:
- `scripts/guards/run-guards.sh` — iterates every `scripts/guards/simple/**/*.sh` (excluding `fixtures/`). Each guard self-classifies per-file via path globs. Includes the Layer A scope-drift parser and Layer B classification-sanity guards (ADR-0024 cross-boundary), AND the always-run **audit-suppressions** guard (`scripts/guards/simple/audit-suppressions.sh` → `scripts/audit-suppressions-check.sh`, read-only) per task #47.
- `scripts/audit-suppressions-check.test.sh` — self-test for the suppressions-check (drives its FAIL branches with fixtures; wired here because there is no `*.test.sh` auto-runner). Task #47.
- plus the other wired self-tests (`_changed_helpers.test.sh`, `_audit_gate.test.sh`, `layer7.test.sh`, the subdomain/slug-class/disk guard self-tests, `layer-all.test.sh`, `run-story.test.sh`, `dev-web.test.sh`, `guards/run-guards.test.sh`, `guards/validate-frame-vectors.test.sh`, `guards/media-telemetry-deny.test.sh`, `guards/validate-internal-proto-no-key-material.test.sh`). (The former `_test_changed_predicates.sh` meta-test was removed 2026-08-20 with the `changed.sh` classifier — §2/§3 amendments.)
  - **This list had drifted and was corrected 2026-09-08** — the last four were wired in `layer3.sh` and missing here. It is hand-maintained with nothing deriving it, so treat it as indicative and read `scripts/layer3.sh` for the authoritative set.
  - **`scripts/release-feature-gate.test.sh` is deliberately NOT here.** It is wired into Layer 1 (`scripts/lang/rust/compile.sh`), not Layer 3, because it invokes `cargo` and ADR-0033 §4 keeps inherently variable compiler cost out of the fast tier. Layer 3 invokes cargo zero times by design. See §6.1.
  - **`scripts/lang/ts/fmt.test.sh` (ADR-0037 §D7 TS fmt lane) — measured ~11.9–12.0s** (three consecutive runs, one box: 11.98 / 11.98 / 11.94; the ~2s over the initial 19-assertion version is the obs-F2 environment-sanity case, which adds one real nx invocation over a prettier-less fixture target), the **second-largest single Layer-3 item** after `validate-frame-vectors.test.sh` (23–27s). **This range is the UNION of independent measurements — widen it, do not overwrite it:** the 10.1–10.3 spread is run-to-run *repeatability* on one box under identical load; a second measurement under contention or on another box (e.g. an operator's Gate-3 pass) extends the range (→ ~10.1–Ns), it does not "correct" the first, exactly as the `validate-frame-vectors` row's 23–27s spans two reviewers rather than one reviewer's spread. **The tight consecutive spread is itself a signal: this test's cost is DETERMINISTIC** — no network, no cache variance, no registry fetch (offline by construction, below) — so the only things that move it are box load and workspace growth. That makes the growth-vector note below *sharp* rather than a general caveat: since cost is deterministic given the workspace, **package count is the ONLY structural lever**, so a step-change in runtime is attributable to a package addition, not to the test becoming flaky. A range, not a point, per this section's reasoning that a bare number produces both false alarms and false comfort. It is **deliberately NOT stubbed** (unlike the rust/proto fmt self-tests' PATH stubs): the TS lane forwards `-- <flag>` through `nx run-many` to prettier, and bare `prettier` with no mode flag exits 0 (a vacuous pass), so only the REAL nx→prettier chain proves the flag arrives — a stub would assert something that passes whether or not forwarding works, the same defect class this task exists to close. Hermetic + **offline**: a synthetic nx workspace under `${DEVLOOP_TMP}`, `node_modules` SYMLINKED to the repo's (no registry fetch), `NX_DAEMON=false` + an isolated per-run `NX_CACHE_DIRECTORY` (no stale project-graph/cache served). It is node tooling, NOT cargo, so the "no cargo" fast-tier invariant (about compiler cost/variance) is satisfied in the letter — **but the cost GROWS with monorepo package count**: `NX_DAEMON=false` recomputes the project graph per invocation and the graph scales with the workspace, so whoever ADDS a package next inherits Layer-3 cost they are not thinking about. **Threshold: if it grows past ~20s, move it to Layer 4** (the TS test layer, outside the fast tier — the `release-feature-gate.test.sh`→Layer-1 precedent establishes that a self-test can live outside Layer 3 when its cost profile justifies). **Shrink lever (the runbook-sanctioned one): consolidate nx INVOCATIONS while keeping per-case isolation** — "group assertions sharing a mutation", exactly as the `validate-frame-vectors` row endorses — and NEVER stub, which reopens the vacuity hole. **Residual (considered, not missed):** the `node_modules` symlink runs the repo's INSTALLED prettier, inheriting the stale-`node_modules` hazard `infra/devloop/entrypoint.sh` gates on nx presence rather than lockfile currency; fine here because the forwarding MECHANISM is under test, not the prettier version (which is exact-pinned in the root `package.json`). **Not `timeout`-wrapped** (only `guards/simple/*.sh` are), so this recorded baseline is the only control.

Layer 3 also carries a **CI-sentinel-leak runtime assertion** (mirrored in `layer-all.sh`): if `GITHUB_ACTIONS` and `DEVLOOP_TEST` are both set, the layer hard-fails early — see `test-sentinel-set-in-ci` below.

**Always-run**: yes — guards self-classify. Layer 3 is one of the two layers (with Layer 6) inside the **90s p95 guard+audit fast-tier wall-clock budget (ADR-0033 §4)** — a sustained `WARN BUDGET_TOTAL_BREACH` here is the operational signal to investigate.

| REASON token | Origin | Cause / Fix |
|--------------|--------|-------------|
| `guards-failed` / `guard-violations` | `scripts/guards/run-guards.sh` | A specific guard found a VIOLATION (implementer lane, exit 1). `run_and_emit` emits `STATUS=FAIL REASON=guards-failed`; **as of 2026-08-21 run-guards.sh ALSO emits its own `STATUS=FAIL REASON=guard-violations` summary line** (a machine trace, since a violation's console line is only the human `FAILED: <name>`). Both are on stdout; the layer aggregate is unchanged (`FAIL`). **Note the stderr `LAYER=3 … REASON=` anchor now reads `guard-violations`** (not `guards-failed`) in a violations-only run — `worst_reason_for_status` returns the FIRST matching FAIL reason and `guard-violations` is printed before `run_and_emit`'s line. The runner prints `FAILED: <guard-name>` + grep-extracted violation lines (`VIOLATION|violation|ERROR|error`). Jump to that guard's source under `scripts/guards/simple/`. |
| `guard-timeout-<name>` / `guard-timeout-kill-<name>` | `scripts/guards/run-guards.sh` (`classify_guard_exit`, exit 124 / 137) | **OPERATOR lane — `STATUS=PRECONDITION_FAILURE` (exit 2), NOT a violation.** A guard exceeded its per-guard timeout (`GUARD_TIMEOUT_SECS`, default 30s) → 124; or was SIGKILLed after the `GUARD_KILL_AFTER_SECS` grace (or OOM-killed) → 137. A machine fact (concurrent load), not a diff defect. run-guards.sh also emits a line-anchored `PRECONDITION_FAILURE: guard <name> timed out …` on stderr (so the §4 one-pass grep finds it). Layer 3 aggregates to `PRECONDITION_FAILURE` (it outranks FAIL) → exit 2 → operator lane, **does NOT consume an implementer attempt.** <br>**BUT — is it diff-caused?** Retry-once is the discriminator: a timeout that does NOT reproduce (passes on retry, or on a quiet machine) is transient contention → operator lane, re-run. A timeout that **REPRODUCES** (times out again on retry and/or a quiet machine) is **DIFF-CAUSED** — the changeset pushed the guard past its budget (a large file set, a pathological scanning input); the timed-out guard rendered NO verdict, so whatever it would have caught went unreported → **route to the implementer, CONSUME an attempt, do NOT escalate to operations.** Second discriminator for a guard sitting right at the boundary (where "reproduces" is a coin-flip): did the guard's runtime JUMP vs its normal cost AND did the diff GROW its input set? A guard that normally takes 2s and now takes 30 is diff-correlated regardless. **Do NOT** raise/remove the timeout as "the fix" — an unbounded guard hangs the pipeline. <br>**Mixed case** (a timeout AND a real violation in the same run): the layer aggregates to `PRECONDITION_FAILURE` (operator lane wins), but the violation is NOT lost — run-guards.sh prints `MIXED_LANE: precondition=<n> violations=<m> …` naming it, and `guards-failed` + `guard-violations` STATUS lines sit in `layer-3.log`. **Still scan for `VIOLATION` / `FAILED:` lines** — a `PRECONDITION_FAILURE` run may also carry a real defect; on the clean re-run (quiet machine) the deterministic violation resurfaces on the implementer lane. |
| `guards-failed` + `FAILED: no-retained-credentials` | `crates/dt-guard/src/ts_retained_credentials.rs` (task #58) | A **retained** client type carries a credential field. `VIOLATION:` lines name the file/line, the rule id (`ts-no-retained-credentials::auth_state_password_with_token` = credential + session token on the same retained type; `::retained_credential_binding` = credential alone), and the type. **Fix: remove the field, or stop retaining the type.** <br>**There is no in-code bypass** — no `guard:ignore` marker is honored, deliberately: a suppression on a credential-retention guard gets applied by whoever is inconvenienced, at the moment of inconvenience, with no security review. If the **guard itself** is wrong, revert the guard commit — it is a separate commit by design and ordered after the client fix precisely so this works (see §Rollback in the task-#58 devloop output). <br>**This guard is FULL-TREE and always-run, so it can fail your devloop for a violation your diff did not introduce.** That is intended: it is a standing invariant, not a diff lint. Do not go hunting your own change for the cause — read the file:line in the VIOLATION line. <br>**If it TIMES OUT** (`guard-timeout-no-retained-credentials`): this now lands on the OPERATOR lane (`STATUS=PRECONDITION_FAILURE`, exit 2 — see the `guard-timeout-*` row above), so first apply the reproduce-on-retry discriminator there. If it reproduces (diff-caused), suspect a **cyclic type alias** in `packages/**` before suspecting scan volume. The declaration closure is cycle-guarded, but a hang presents as a performance problem and sends triage to the wrong place. Measured runtime is ~9ms over 62 files. <br>**A `WARN` from THIS guard is a coverage gap, not an IO skip.** §6.3.1 describes dt-guard WARN lines as a corrupted catalog/dashboard the kernel skipped; `ts-no-retained-credentials` uses the channel differently — `declaration block exceeded line cap` means fields beyond `MAX_DECL_BLOCK_LINES` were never scanned, so the guard may be under-matching on that declaration while still reporting `STATUS=OK`. A clean run must be a WARN-free run. If one appears, either the cap needs raising or a declaration has grown pathological — do not ignore it because the layer passed. <br>**Maintenance**: the guard's zero-false-positive claim is *measured across the current tree*, NOT true by construction. Re-measure against the real tree whenever the credential/token vocabulary, the retention-idiom set, or the alias closure changes — any of those moves the false-positive surface without this guard's code changing, and the breakage lands on whoever's devloop is next. |
| `guards-failed` + `FAILED: validate-frame-vectors` | `scripts/guards/simple/validate-frame-vectors.sh` (story task 8) | The frame-v2 cross-language vectors disagree with the Rust codec, the vendored external anchor, or their own internal relations. **`VIOLATION:` lines name the check id (`g2`..`g16`), the vector row by name, the field, and both values or lengths.** <br>**Which lane:** every failure and every precondition here exits nonzero, so it lands in `classify_guard_exit`'s `*)` arm and reads as the **implementer lane** (`FAILED_GUARDS`). A guard subprocess cannot self-declare a lane — §6.3.1 item 4 documents that asymmetry for dt-guard — so read the token to decide where to start. <br>**`VIOLATION` (a value is wrong): regenerate, do not hand-edit.** `cargo run -p media-vector-gen --bin generate-frame-vectors`. The file is generated then frozen; **a vector that contradicts a recorded ruling is a defect to escalate, not to conform to** (`proto/test-vectors/README.md` names the addressee). Conforming the other codec to a wrong vector makes both consistently wrong with this guard green, which is strictly worse than a divergence because a divergence is loud. <br>**`ERROR: PRECONDITION [<token>]` — two sub-classes needing OPPOSITE first actions.** *(i) An ordinary edit caused it and the message is the fix:* `vectors-unparseable`, `banner-missing`, `zero-rows`, `generator-crate-missing`, `todo-tracking-entry-missing`, `suite-list-empty`, `suite-yields-zero-rows`, `manifest-missing`, `gated-by-key-missing`, `crypto-field-unclassified`, `spec-anchor-not-found`. *(ii) The guard's own extraction broke — start at the `sed`/`jq`, not at the input tree:* `vectors-missing`, `rust-const-not-found`, `rust-version-const-not-found`, `rust-flag-consts-not-found`, `reject-macro-not-found`, `ts-site-not-found`, `codec-list-undeclared`, `key-field-pattern-undeclared`, `provenance-digest-not-found`, `external-vectors-missing`, `jq-missing`, `sha256sum-missing`. Class (ii) means a declaration was renamed or reshaped and the guard is now comparing nothing; **fix the extraction, never delete the check.** <br>**`jq-missing` / `sha256sum-missing` are OPERATOR-lane in substance despite the exit code** — `jq` is at `infra/devloop/Dockerfile:31` and present on `ubuntu-latest`, and this is the first guard to use it, so absence is an image regression rather than a diff defect. <br>**This guard is FULL-TREE and always-run, so it can fail your devloop for a violation your diff did not introduce.** That is intended — g10 (external digest), g11 (workspace membership), g13 (fixture-key pattern) and g14 (TODO anchor) are standing invariants, not diff lints. Do not go hunting your own change for the cause; read the check id and row name. <br>**A `WARN frame-vectors:` line on a PASSING run is a FAULT since story task 15.** Both codecs are gated in tree, so the banner can only mean a codec has been un-gated — `gated_by.<codec>` flipped back to `false`, or a codec added to g14's `declared_codecs` without its conformance marker. Investigate; do not ignore it. *(Before task 15 this banner was the expected steady state while `gated_by.typescript` was `false`. The entry is inverted rather than deleted because a triager who found the old text would wave through exactly the regression it now catches.)* It remains deliberately non-suppressible — no env var, flag or config key — and the guard still must NOT be "fixed" to exit nonzero on the ungated state: redding every devloop would make **deleting** the guard the cheapest path to green, which is the inversion the non-suppressibility argument exists to prevent. <br>**Measured runtime: guard ~0.65–0.72s** (23 rows, one `jq` program per section rather than one spawn per field); **its self-test `scripts/guards/validate-frame-vectors.test.sh` ~23–27s**; the case count is NOT recorded here — the suite pins it itself in `EXPECTED_CASES` and prints its own tally on completion, and that constant's own comment calls itself "deliberately the ONLY home for it". A copy in this row would be a second encoding with nothing deriving it, which is the defect removed from the `media-telemetry-deny` row below. Each case `mktemp`s a one-mutation copy of the real tree; that per-case isolation is load-bearing for validity, so if this ever needs shrinking the lever is **fewer trees (group assertions sharing a mutation), never shallower copies** — a hand-built fixture drifts from what it models and then passes forever. Ranges rather than point estimates: two reviewers measured independently under different load (guard 0.645/0.657/0.645 vs 0.718; self-test 23.3/24.4/24.1 vs 26.5), and a single number would produce both false alarms and false comfort in the reproduce-on-retry triage above. Whole-pool context: Layer 3 ~75s, fast tier (3+6) ~77s against the 90s p95 (measured 2026-09-20 on one box; was ~59s/60s pre-ADR-0037-§D7 at `1a6701b`, +16s on Layer 3 for the newly-wired fmt-lane self-tests — `ts/fmt.test.sh` ~10s, plus `validate-fmt-lane-ssot` + its self-test, `rust/fmt.test.sh`, `proto/fmt.test.sh`, `proto/golden-format.test.sh`, and `_common.test.sh`'s 85 assertions). **Headroom is now ~13s, down from ~30s** — and the budget is a WARN, not a gate, so nothing stops the creep except reading this number; the next addition of this size breaches. Both measurements had **L6 = N/A** (audit did not execute, 1–2s); a run where the audit actually runs has LESS than 13s headroom, so do not read ~77s as the steady-state ceiling. The self-test is the largest single item in Layer 3 and is **not** `timeout`-wrapped (only `guards/simple/*.sh` run under `timeout`), so nothing will ever fail because it got slower — this recorded baseline is the only control, and §6.3's timeout triage forks on whether runtime *jumped* versus its normal cost. If it times out, first apply the reproduce-on-retry discriminator in the `guard-timeout-*` row above; if it reproduces, suspect row-count growth outpacing the per-section batching. |
| `guards-failed` + `FAILED: media-telemetry-deny` | `crates/dt-guard/src/media_telemetry_deny.rs` + `scripts/guards/simple/media-telemetry-deny.yaml` | ADR-0036 §11's media-path telemetry deny. **Nine tokens in three classes, and they need DIFFERENT first actions — read the token before touching anything.** <br>**(a) CONTENT — a real policy violation, go fix the cited line.** `media-telemetry-deny-macro-in-media-path-<n>-of-<m>-findings` = a log, print, metric, event, span or `#[instrument]` macro FORM appears in code position under a configured directory. `media-telemetry-deny-telemetry-crate-import-<n>-of-<m>-findings` = a `use` of `tracing` / `log` / `metrics` / `tracing_subscriber` there. `VIOLATION: [<token>] <path>:<line>:<col> — <macro spelling> — <fix>` names the location and the macro NAME. **It deliberately does NOT print the macro's arguments**: the per-participant and per-stream identifiers §11 exists to contain live in that argument list, so echoing them would move the leak into CI logs and artifact retention. If you need more, re-run with `--explain`; that output is redacted the same way (`SecretFinding` has no `matched` field). **Fix**: resolve a metric handle once at setup and call `.increment` / `.record` / `.set` / `.absolute` / `.decrement` on it — those are allowed and structurally cannot match. `<n>` is the winning rule's hit count, `<m>` the total, because `run-guards.sh` caps re-emission at `head -5` and 8 findings would otherwise show as 5 with nothing saying so. <br>**(b) SCOPE / PARSE — the guard checked nothing.** The scope and manifest tokens (`-scope-directory-missing`, `-scope-directory-empty`, `-scope-directory-escapes-root`, `-scope-no-directories-configured`, `-manifest-missing`, `-manifest-unparseable`) are printed FIRST as `ERROR: PRECONDITION [<token>]` with a body that says `THIS IS A DIFF DEFECT, not a machine fault`; that banner is pinned by the self-test. **`-unparseable-use` is the exception** — it routes through the ordinary finding printer, so it carries the `ERROR: PRECONDITION` prefix on a `<file>:<line>:<col>` finding line with NO banner, and its remedy is rewriting a `use` declaration rather than a one-line path fix. **This prefix INVERTS its meaning relative to the `release-build-profile-*` and `env-config-*` rows below** — there it usually means the environment; here it always means someone edited the tree in this diff. Do NOT re-run on a quiet machine. `…-scope-directory-missing` = a configured directory does not exist: **the directory itself was renamed, moved or deleted.** Restore it, or update the manifest. `…-scope-directory-empty` = the directory exists but holds zero `.rs` files: **the directory survived and its contents did not** — the files were moved or renamed out from under it. Restore them, or update the manifest to where they went. **Both are diff defects with a one-line fix, and they are distinct tokens so the message names the right thing to look for: `-missing` sends you to the parent path, `-empty` sends you to the files.** They are NOT opposites — that pairing is `-missing` versus `-escapes-root` below. Suspect the guard's own walk (`common/scope.rs`) only if the media tree looks intact under both, and treat that as the second hypothesis, never the first. `…-scope-directory-escapes-root` = an entry resolves outside the repo (traversal, absolute path, or a symlink out of the tree). **Do not apply the `-missing` remedy here**: re-pointing the manifest at the symlink's target completes the evasion. `…-scope-no-directories-configured` = the manifest lists none — a one-line total disarm the per-directory tokens cannot see. `…-manifest-missing` / `…-manifest-unparseable` = the manifest is absent or failed to deserialize; it is fail-closed and never defaults to empty. `…-unparseable-use` = a `use` line the classifier could not resolve; reported rather than passed, because "a shape I didn't anticipate" passing silently is the coverage illusion this guard exists to prevent. <br>**Precedence**: several conditions can fire at once but only one reaches `STATUS=…REASON=`. Scope and parse conditions outrank findings; the exact order is `Rule::ORDER` in `media_telemetry_deny.rs`, deliberately **not** restated here — a second copy of an ordering is a second thing to drift, and this row cannot be kept honest by any check. **You are told when it happens rather than having to remember it**: when more than one rule class fires the guard prints `ERROR: MIXED_CONDITIONS: winner=<token> co-firing=<n> …` ahead of every other record. If you see it, do not triage from the STATUS token alone. <br>**Lane**: everything here is implementer-lane `STATUS=FAIL` (exit 1), including the precondition class. That is deliberate and not an oversight: `classify_guard_exit` routes only 124/137 to the operator lane, so a guard exiting 2 would print operator-lane text while being counted as a violation. The token IS the lane (§6.3.1 item 4). <br>**There is NO bypass** — no `guard:ignore` marker, no env var, no flag, and the module does not import `crate::ignore`. An annotation would be a one-line silent disarm sitting in the exact directory the control protects. If the **guard itself** is wrong, revert the guard commit; it is a separate commit by design (see §Rollback in the 2026-09-07 devloop output). <br>**`#[cfg(test)] mod tests` under the scope IS in scope, by decision.** A debug `println!` in a media test module reds the pipeline — that is intended; move it to a sibling. An exemption would be a hole reachable by moving code into a test block. <br>**Diagnostic**: `target/release/dt-guard media-telemetry-deny --root . --explain`. <br>**Measured runtime: guard ~3–4ms** (7 files, one directory); **its self-test `scripts/guards/media-telemetry-deny.test.sh` ~0.22–0.29s**, measured warm in the devloop container on 2026-09-08 across four consecutive runs. Ranges, not point estimates — a bare number cannot distinguish expected variance from a regression, and the reproduce-on-retry triage above forks on exactly that. **The assertion count is deliberately NOT recorded here: the suite prints its own on completion** (`<script>: <N> passed, <M> failed`, via `scripts/lang/_test_helpers.sh::report_results`). A hand-copied count is a second encoding with nothing deriving it — the previous literal had drifted from both the suite and from itself, and re-counting is not even possible by inspection, since at least one case increments the pass counter directly rather than through an assert helper. Read the PASS COUNT from the suite; the runtime baseline is this row. The self-test is **not** `timeout`-wrapped (only `guards/simple/*.sh` are), so nothing will fail because it got slower — this baseline is the only control. It re-uses ONE pristine copy of the media tree across cases rather than copying per case, deliberately avoiding the shape that costs `validate-frame-vectors.test.sh` 23–27s. |
| `guards-failed` + `FAILED: validate-internal-proto-no-key-material` | `scripts/guards/simple/validate-internal-proto-no-key-material.sh` (story 2 protocol contract) | ADR-0036 §4 "No key material crosses the MC→MH contract", mechanised for `proto/dark_tower/internal/v1/internal.proto`. Rule content is @security's; machinery is @infrastructure's. **Five tokens in two classes — read the token before touching anything.** Every record is `VIOLATION [<token>] …` on stderr with the offending `internal.proto` line numbers. <br>**(a) CONTENT — the contract gained a way to carry key material. THIS IS A DIFF DEFECT, not a machine fault.** `internal-proto-bytes-field` = a `bytes` field in any spelling (bare, `repeated`, `optional`, oneof member, `map<…, bytes>`) or `google.protobuf.BytesValue`. `internal-proto-cross-package-type` = a qualified type that is not an allowlisted ENUM spelled by its canonical fully-qualified name. This includes a relative (`signaling.v1.X`) or leading-dot spelling of an allowed enum, and any `google.protobuf.*` type; a red on a relative spelling is intended, so fix the spelling and never loosen the comparison. `internal-proto-import-not-allowed` = an `import` outside the allowlist, or a non-plain form of the allowed one. **Remedy for all three: do not delete the field to go green, and do not edit the guard.** The field contradicts the rule; a legitimate need is a re-derivation of the rule with @security. <br>**(b) COULD NOT EVALUATE — the guard refuses to report clean.** `internal-proto-scan-vacuous` = a proto is missing or empty; internal.proto lost its identity markers (`package dark_tower.internal.v1;`, `service MediaHandlerService`); an allowlist entry is not declared in signaling.proto at all; a `/*` block comment appeared outside a `//` comment (the line strip cannot see inside it — use `//`); or a grep exited outside {0,1}. **Do NOT loosen a pattern to clear it** — restore the input. `internal-proto-allowlist-not-enum` = an allowlist entry is declared as a MESSAGE in signaling.proto. The allowlist is enums-only because a message type is exactly how key material would arrive without the word `bytes`; remove the entry, since widening the boundary is a re-derivation with @security. <br>**Lane**: everything exits 1 → implementer lane (§6.3.1 item 4: the token IS the lane). **There is NO bypass**: no ignore marker, no env var, no flag. The `INTERNAL_PROTO_GUARD_ROOT` seam is honoured only under `DEVLOOP_TEST=1`. <br>**Measured runtime: guard ~9ms; its self-test `scripts/guards/validate-internal-proto-no-key-material.test.sh` ~0.39–0.41s**, measured in the devloop container on 2026-09-22. The suite prints its own assertion count on completion; it is deliberately not copied here. |
| `guards-failed` + `FAILED: validate-workspace-deps` | `crates/dt-guard/src/workspace_deps.rs` (dependency-version SSoT, 2026-10-03) | One version source per crate: root `[workspace.dependencies]`, mirrored by the root-`exclude` fuzz crates, plus the Dependabot coverage those crates need. Every record is `VIOLATION: [<token>] <file> …` or, for preconditions, `ERROR: PRECONDITION [<token>] …` printed first. **Read the token before touching anything; the classes need different first actions.** <br>**(a) CONTENT — fix the named manifest/config.** `workspace-dep-redeclared` (R1) = a member pins a crate that is in `[workspace.dependencies]` → change it to `{ workspace = true }` (keep any `features`). `dep-pinned-in-multiple-members` (R2) = the same registry crate carries a literal version in two or more crates (members or fuzz crates) → hoist it into `[workspace.dependencies]`; members inherit, fuzz crates repeat it. `excluded-workspace-version-drift` (R3) = a fuzz crate's literal differs from the root's, or keeps default features the root disables (the base64 `simd-unsafe` case). **Expected on a Dependabot cargo PR** that moves a requirement the fuzz crates repeat: edit the fuzz `Cargo.toml` literal **in the same PR**; never split or merge half of it. `facade-crate-duplicated` (R4) = `Cargo.lock` holds two versions of `metrics`, `tracing-core` or `opentelemetry`: the second copy's process-global is empty, so instrumented code silently records into nothing (the exporter installs on one copy, the code emits to the other). Move the whole family together (`metrics-stack` / `otel-web-stack` Dependabot groups) and never merge half. `member-default-features-ignored` (R5) = a member sets `default-features = false` on a workspace dep whose workspace entry keeps defaults; cargo ignores it with a warning → disable defaults on the workspace entry. `dependabot-fuzz-block-missing` / `dependabot-fuzz-allow-missing` / `dependabot-fuzz-allow-extra` (R6) = `.github/dependabot.yml` has no cargo block whose `directories` are exactly the root `exclude` list (or another block overlaps them), or that block's `allow:` omits / adds a crate relative to the fuzz-only set (declared by a fuzz crate, used by no member). Fuzz-only crates are not in the root `Cargo.lock`, so only that block can ever offer them; add the missing name in the same diff that added the dependency, or remove the extra one. <br>**(b) PRECONDITION — the guard checked nothing; never a pass.** *Causable by an ordinary edit (the message names the fix):* `workspace-deps-workspace-manifest-unreadable` (root `Cargo.toml` invalid TOML), `workspace-deps-workspace-members-glob` (glob `members`), `workspace-deps-member-manifest-unreadable` (a member or fuzz `Cargo.toml` invalid), `workspace-deps-excluded-manifest-missing` (a root `exclude` entry with no `Cargo.toml` — a fuzz crate moved or renamed; fix `exclude` or restore it), `workspace-deps-dependabot-config-unreadable` (`.github/dependabot.yml` missing/invalid YAML), `workspace-deps-cargo-lock-unreadable` (lockfile missing/invalid). *Pointing at discovery or a stale policy list — start at the guard, not the tree:* `workspace-deps-workspace-members-empty`, `workspace-deps-workspace-dependencies-empty`, `workspace-deps-facade-absent-from-lock` (a `SINGLE_VERSION_CRATES` entry has no lock entry: the lock lost its packages or the list is stale — update the list, never loosen R4). <br>**Lane**: all exit 1 → implementer lane (§6.3.1 item 4). **No bypass**: no ignore marker, env var or flag. Diagnostic: `target/release/dt-guard workspace-deps --root . --explain`. The PASS token carries `members-<N>-excluded-<M>`; the real-repo e2e pins both to the root manifest. |
| `suppression-past-due` | `scripts/audit-suppressions-check.sh` (Layer-3 guard) | A suppression in `audit-suppressions.toml` is past its `expires`. **This red is INTENTIONAL** — CI goes red on the expiry day by design, not an outage/flake. Output names each past-due id + its days-past. **Action is NOT bypass:** renew the `expires` after re-verifying the justification (run the verify command of record named in the entry's comment in `audit-suppressions.toml`), OR fix the advisory. Renewal procedure: `docs/contributor/audit-suppressions.md`. (Red-on-expiry is the FIRST signal — no warn-ahead window — so renew proactively per the contributor-doc cadence.) |
| `suppression-drift` | `scripts/audit-suppressions-check.sh` (sync-check) | The generated derived files (`.cargo/audit.toml` / `.pnpm-audit-ignore.json`) drifted from `audit-suppressions.toml` (hand-edited derived file, or a forgotten `--fix`). Fix: `scripts/audit-suppressions-check.sh --fix`, then commit BOTH the manifest and the regenerated derived files. |
| `suppression-malformed` | `scripts/audit-suppressions-check.sh` (parser) | The manifest (or a derived file) is present but unparseable — missing required field, bad `expires`, duplicate id, etc. The `MALFORMED:` lines name the offending line/field. Fix the manifest; never degrade a malformed entry into "0 suppressions". |
| `suppression-quality` | `scripts/audit-suppressions-check.sh` (quality-check) | An entry has an empty `reason`/`ticket`, a non-`YYYY-MM-DD` `expires`, or an `ecosystem` outside {rust, js}. Output names the offending id + field. |
| `suppression-override-without-test-sentinel` | `scripts/audit-suppressions-check.sh` (trust-boundary guard) | A test-injection override env (`DEVLOOP_SUPPRESSIONS_MANIFEST` / `AUDIT_SUPPRESSIONS_NOW` / derived-path overrides) is set but `DEVLOOP_TEST` is not exactly `"1"`. **Tamper / misconfig signal** — a non-test environment set an override that would redirect the check. INVESTIGATE what set the env (CI step, reusable action); do NOT just unset-and-rerun. |
| `test-sentinel-set-in-ci` | `scripts/layer-all.sh` / `scripts/layer3.sh` (CI-leak assertion) | `DEVLOOP_TEST` is set in a CI job (`GITHUB_ACTIONS=true`). The test sentinel must NEVER be set in CI — it would let the always-run check honor ambient override envs repo-wide. **Pipeline-integrity incident** — find what exported `DEVLOOP_TEST` (workflow step, reusable action) and remove it; do NOT unset-and-rerun blindly. |
| `layer-script-dir-set-in-ci` | `scripts/lang/_common.sh::assert_no_ci_sentinel_leak` (second sentinel, task #56) | `LAYER_SCRIPT_DIR` is set in a CI job (`GITHUB_ACTIONS=true`). It is a LOCAL-ONLY test seam that substitutes stub layer scripts into `layer-all.sh`'s loop (orchestrator lane-integrity test) — in CI it would let a forged stub dir turn the whole pipeline green and FORGE the Gate-2 verdict. **Pipeline-integrity incident** — nothing legitimate sets it in CI; find + remove what exported it; do NOT unset-and-rerun blindly. Defense-in-depth: independent of `DEVLOOP_TEST`, so it reds even if the first sentinel didn't catch the leak. |
| `dependabot-ignore-present` | `scripts/audit-suppressions-check.sh` (SSOT-integrity check) | `.github/dependabot.yml` has a non-empty `ignore:` block — a shadow suppression surface that fragments the single source of truth. **Dependabot `ignore:` is not a suppression channel** — remove it; if an advisory genuinely needs suppressing, add it to `audit-suppressions.toml` (reviewed PR, ADR-0033 §11). Dependabot is for bump PRs only. |

**Two distinct suppression-drift surfaces — on-call note.** Advisory problems surface in TWO places, and they live in different runbook sections:
- **Per-PR expiry / hygiene** → Layer-3 `suppression-past-due` / `-drift` / `-malformed` / `-quality` red run (this section). Fires on every devloop + CI run.
- **Scheduled-scan drift** (a NEW advisory against an UNCHANGED lockfile) → a red `Scheduled Audit` workflow run + an open `audit-drift` GitHub Issue (auto-closes when a later scheduled run is clean). See §6.6 (Layer 6 / scheduled scan). On-call should check the `audit-drift` issue, not just CI, for between-PR drift.

**Suppression renewal exception (security-reviewed wording — ADR-0033 §11 ownership boundary).** The audit-config ownership reminder (§6.6 below) says operators should not modify suppressions during failure triage and should escalate to security. There is ONE sanctioned exception:

> **Exception — suppression renewal on expiry:** when a Layer-3 `suppression-past-due` failure fires, editing `audit-suppressions.toml` to renew the `expires` date (then `scripts/audit-suppressions-check.sh --fix` to regenerate derived files) IS the sanctioned remediation — NOT the prohibited ad-hoc allowlist edit. The prohibition targets SILENT, incident-time suppression of a LIVE advisory via CLI flags or hand-edited derived files. Renewal flows through the tracked manifest, regenerates derived files deterministically, and lands as a reviewed commit. Security ownership (ADR-0033 §11) is preserved: the renewed `reason`/`expires` MUST go through normal PR review — security reviews the renewal justification (e.g. the re-run `cargo tree -p rsa --invert` build-time-only re-verification for RUSTSEC-2023-0071). An operator MUST NOT extend an `expires` date as part of live incident triage to make CI green; that remains prohibited and escalates to security.

Load-bearing distinction: RENEWAL = reviewed manifest commit with re-verified justification (sanctioned); AD-HOC SILENCING = CLI flag / hand-edited derived file / unreviewed expires-bump-to-unblock-CI / Dependabot alert dismissal (prohibited, escalate).

**Do not close a Layer-3 failure on a suite's summary line.** A self-test's own `N passed, 0 failed` count is not the wrapper's verdict: several suites install EXIT traps that set their own exit code *after* `report_results` has already chosen 0/1, deliberately, so that a containment or mutation failure outranks an all-green run. A clean summary beside `STATUS=FAIL` means the exit code came from outside the suite's assertion accounting — read the stderr, not the count.

#### 6.3.1 `dt-guard` triage (ADR-0034 §10 Wave 3)

Most of the simple guards — the eight listed here (cite-no-line-numbers / cite-symbol-resolves / alert-rules-policy / dashboard-panels / metric-labels / application-metrics / infrastructure-metrics / grafana-datasources) plus every wrapper under `simple/ts/` — are ≤5-line shell wrappers around the Rust binary `target/release/dt-guard`. The wrapper resolves the binary path, asserts it is executable, and `exec`s `dt-guard <subcommand> --root "$REPO_ROOT"`. There are three distinct failure shapes:

1. **Stale or missing binary** — `STATUS=FAIL REASON=dt-guard-binary-missing`. The wrapper exits 1 before invoking any subcommand because `target/release/dt-guard` is not present (or not `-x`).
    - **Diagnostic**: `ls -la target/release/dt-guard`.
    - **Resolution**: `cargo build --release -p dt-guard -p dt-story`. The wrapper produces no `VIOLATION:` lines because the policy kernel never runs.
    - **Re-running `scripts/layer1.sh` now rebuilds both binaries** — since 2026-08-20 the rust compile verb is **always-run** (the skip-if-untouched short-circuit was retired, ADR-0033 §3), so `lang/rust/compile.sh`'s `cargo build --release -p dt-guard -p dt-story` steps run on every devloop regardless of what the diff touches. The former producer/consumer skew (compile skip-gated on non-Rust diffs while the guards consume the binary always-run) is **CLOSED** by this change — a plain `packages/**`-only or docs-only devloop now builds both binaries. If the binary is still missing after a full Layer-1 run, that is a genuine build failure (or a fresh checkout that never ran Layer 1), not a skip-skew.

2. **Subcommand not found** — clap exits non-zero with its own diagnostic on stderr (typically `error: unrecognized subcommand <foo>`). `STATUS=` may surface as `clap-error` or omit entirely depending on which subcommand the wrapper invoked; the canonical signal is the clap-formatted stderr line.
    - **Diagnostic**: `dt-guard --help` to list registered subcommands.
    - **Resolution**: typo in the wrapper, or a subcommand-rollout-not-yet-landed across two PRs. Re-build to pick up newly registered subcommands.

3. **Bona-fide policy violation** — `STATUS=FAIL REASON=<policy-token>` after the subcommand runs to completion, paired with one or more `VIOLATION: <path>:<line> — <rule_id> — <message>` lines on stdout.
    - **Diagnostic**: re-run the subcommand with `--explain` for a single-line `EXPLAIN:` record per finding (span + policy + source location). Example: `target/release/dt-guard alert-rules-policy --root . --explain`.
    - **Resolution**: fix the input file at the cited path:line, or — if a true false positive — add `# guard:ignore(<reason>)` per the inline guidance in each subcommand's source-doc header. `<reason>` must be ≥10 characters and not match the `LAZY_REASON_RE` vocabulary denylist.
    - **`.ts` / `.svelte` guards have NO `#` marker — do not reach for one.** `crates/dt-guard/src/ignore.rs` ships only the `#`-comment and HTML-comment flavors, and neither matches a `//` line comment, so a `#` comment added to a TypeScript file silently does nothing. TS-scoped guards either define a bespoke marker (`ts_metric_naming`'s `// dt-metric-name-dynamic:`) or deliberately have none at all (`ts-no-retained-credentials` — see its §6.3 row for what to do instead).

4. **Vacuity — the guard could not check what it is responsible for.** `STATUS=FAIL REASON=<vacuity-token>` where the token names a *discovery* fault rather than a policy finding (`env-config-no-services-checked`, `release-build-profile-no-dockerfiles-discovered-<n>`, `no-insecure-browser-flags-no-candidate-files-<n>`). **This outranks any policy finding and always names the run**: a walk-based guard that finds nothing and reports OK "reads as coverage", so these guards fail closed instead.
    - **Diagnostic**: the vacuity line is printed FIRST, before any `VIOLATION:` lines, and is emitted as `ERROR: PRECONDITION [<token>] …`. The `ERROR:` prefix is load-bearing — see the lane note below.
    - **Resolution — read the token first, because two sub-classes need opposite actions.** *Most* vacuity tokens fire because the guard's **discovery path** broke (a service roster, a directory layout, a manifest it walks), not because an operator edited an input: `release-build-profile-no-dockerfiles-discovered` means the `infra/docker/*/Dockerfile` walk matched nothing, and you cannot cause that by editing a Dockerfile — start at the guard module's discovery code, not the input tree. **But some ARE causable by an ordinary edit**, and for those the environmental advice actively misleads: `release-build-profile-cargo-config-unparseable` (invalid TOML in `.cargo/config.toml`) and `release-build-profile-service-roster-underivable` (glob entries in `[workspace] members`) are both normal things to have just done. Those messages name the offending file and the remedy — follow them. The per-token split is in the §8 catalogue row.
    - **Lane**: these are *precondition* failures on the operator lane by intent, but they arrive as `STATUS=FAIL` (exit 1, implementer lane). That is not a bug in the guard — a guard subprocess **cannot** self-declare a lane; see the mechanism note below. The token and the `ERROR: PRECONDITION` body are the only channels carrying the lane, which is why they are worded the way they are.

dt-guard also emits `WARN dt-guard auxiliary skip: <path> (<error-kind>)` to stderr when its auxiliary index-scan loops swallow an IO/parse failure (per ADR-0034 §F-SG-2 mitigation). `run-guards.sh` surfaces those WARN lines alongside `VIOLATION` / `ERROR` in non-verbose CI logs — a sudden uptick indicates a corrupted catalog or dashboard file that the policy kernel skipped silently.

### 6.4 Layer 4 — Test (`scripts/layer4.sh`)

`scripts/test.sh` → `for_each_lang_with_verb "test"` → `lang/rust/test.sh` + `lang/ts/test.sh`. Proto has no real test phase — its placeholder `lang/proto/test.sh` emits `STATUS=N/A REASON=not-applicable-to-this-lang` (informative, expected) when proto is touched.

**Always-run**: rust, ts — every run (per ADR-0033 §3; skip-if-untouched retired 2026-08-20). Proto is N/A via verb-discovery (no `test.sh`).

| REASON token | Wrapper | Cause / Fix |
|--------------|---------|-------------|
| `cargo-nextest-failed` (closing line `rust-test-lanes-failed-nextest…`) | `lang/rust/test.sh` (`cargo nextest run --profile layer4 "$@"`) | A test failed. The wrapper brings up the test postgres container (podman / docker), applies pending sqlx migrations, then runs every test binary with cargo-nextest, one process per test, in parallel (`.config/nextest.toml`: no retries, no fail-fast). The failing test's output is printed immediately and repeated in nextest's final summary — fix the test. A test that hangs is named `SLOW` at 60 s and killed (`TIMEOUT`) at 5 min. **Read the linker-abort row below FIRST if the output contains one**: this token is also what a machine-resource failure arrives as. |
| `cargo-doctest-failed` (closing line `…-doctest`) | same wrapper (`cargo test --doc`, the package-selection subset of `"$@"`) | A doctest failed (nextest does not run doctests, so they have their own lane). Fix the doc example. |
| `cargo-doctest-no-lib-targets` (STATUS=OK) | same wrapper | Expected: every selected package is bin-only (e.g. `./scripts/test.sh -p devloop-helper`), so there are no doctests; decided from `cargo metadata`, and reported rather than silently skipped. |
| `cargo-nextest-failed` with nextest's `error: no tests to run` (exit 4) | same wrapper, narrowed by caller args | **Intentional and loud**: the caller's `-p`/filter selected zero tests (`cargo test` used to pass this). Fix the selection; the pipeline itself never narrows. |
| `cargo-nextest-missing` / `cargo-nextest-version-mismatch` (`CARGO_NEXTEST_VERSION_MISMATCH:` on stderr) | same wrapper, before any migration or test | **Environment, not the diff.** The container's cargo-nextest is absent or differs from `infra/cargo-tools.versions`. Fix (host): `infra/devloop/devloop.sh --rebuild`. Every container on an image older than the pin hits this after a pin change. There is deliberately no fallback to `cargo test`. Operator lane. |
| `cargo-nextest-failed` **carrying `ld terminated with signal 6`** | same wrapper — **not a test failure at all** | **The linker was killed under memory pressure; the diff is not implicated.** Symptom: `collect2: fatal error: ld terminated with signal 6 [Aborted], core dumped` (or `signal 9`), from `ld` / `rust-lld`. **The tell is the spread, not the message**: several test binaries fail to *link* at once, including binaries the diff does not touch — a real link error (undefined symbol, duplicate symbol) is specific to what changed, this is not. Linking many large binaries in parallel is the pipeline's peak-memory moment. **First action: check free memory, then re-run Layer 4 in isolation** (`./scripts/layer4.sh`) before reading any test output; a clean isolated run confirms it. **Operator lane — does not consume a devloop attempt.** Classifying this automatically is filed in `docs/TODO.md` (owner `infrastructure`); until it lands, this row is the only thing separating it from a genuine test failure. |
| `wrapper-aborted-early-exit-<rc>` (runtime missing) | `lang/rust/test.sh:detect_runtime` | `Neither podman nor docker found. Please install one.` — install a container runtime. The wrapper aborts before reaching `run_and_emit`; since task #50 its EXIT trap emits `STATUS=FAIL REASON=wrapper-aborted-early-exit-<rc>` so the layer aggregates **FAIL** (not the old silent `UNKNOWN`). `<rc>` is the abort code. |
| `wrapper-aborted-early-exit-<rc>` (db-bringup failure) | `lang/rust/test.sh:wait_for_db` / `run_migrations_if_needed` | `Database did not become ready within ${MAX_WAIT_SECONDS}s` (or a migration failure) — container started but pg never accepted connections. Same EXIT-trap path: STATUS=FAIL emitted before exit. Check the test container logs. |
| `nx-test-failed` | `lang/ts/test.sh` (`nx affected -t test:unit test:component`) | A TS unit/component test failed. Run the offending project's test target locally. |
| `not-applicable-to-this-lang` (proto) | `lang/proto/test.sh` (intentional-gap placeholder) | Expected — proto has no real test phase per ADR-0033 §1. The placeholder emits `N/A`, which ranks just below OK, so a clean Layer 4 aggregates to `OK` (rust/ts ran and passed). A genuinely missing `test.sh` would be `FAIL-MISSING-VERB` (exit 2) instead. |

### 6.5 Layer 5 — Lint (`scripts/layer5.sh`)

`scripts/lint.sh` → `for_each_lang_with_verb "lint"` → `lang/{rust,ts,proto}/lint.sh`.

**Always-run**: rust, ts, proto — every language, every run (per ADR-0033 §3; the skip-if-untouched short-circuit was retired 2026-08-20).

| REASON token | Wrapper | Cause / Fix |
|--------------|---------|-------------|
| `cargo-clippy-failed` | `lang/rust/lint.sh` (`cargo clippy --workspace --all-targets -- -D warnings`) | Clippy lint or warning (warnings are denied — `-D warnings`). Run `cargo clippy --workspace --all-targets` locally for the full output. <br>**`--all-targets` is load-bearing and its absence is silent.** Plain `cargo clippy -p <crate>` lints the lib and bins ONLY — measured on `dt-guard` 2026-09-08: bare invocation compiles **zero** `--test` targets, `--all-targets` compiles **ten**. So `#[cfg(test)] mod tests` and every file under `tests/` are entirely unlinted without the flag, and this crate's workspace lints (`Cargo.toml` `[workspace.lints.clippy]`, e.g. `panic = "deny"`) apply to test code too. <br>**"`cargo test` passes" is not evidence about this layer, and neither is a bare `cargo clippy`.** `cargo test` does not run clippy at all; a bare `cargo clippy` runs it over a strictly smaller target set than Layer 5 does. A test added during review can therefore pass 500 assertions, satisfy a self-check, and still red Layer 5 — which happened on 2026-09-08 (`docs/devloop-outputs/2026-09-08-media-telemetry-deny-policy/`, a review-round-2 test reaching `panic!` through an `unwrap_or_else` closure under `panic = "deny"`). If you are self-checking before handing off, run the wrapper (`scripts/lang/rust/lint.sh`) rather than an invocation you composed. |
| `nx-lint-failed` | `lang/ts/lint.sh` (`nx affected -t lint`) | TS lint (eslint) violation. Run the project's lint target locally. |
| `buf-lint-failed` | `lang/proto/lint.sh` (`buf lint proto`) | Proto STANDARD-lint violation. Inspect output for the file + finding; `proto/buf.yaml` controls policy. |
| `buf-binary-missing` | `lang/proto/lint.sh` | See §6.1. |

### 6.6 Layer 6 — Audit (dep-change-gated as of task #47; `scripts/layer6.sh`)

`scripts/audit.sh` is an **orchestrator** that combines two gates:
1. `_dispatch.sh::for_each_lang_with_verb "audit"` with `DEVLOOP_DISPATCH_ALWAYS_RUN=1` → `lang/rust/audit.sh` (`cargo audit`) + `lang/ts/audit.sh` (`pnpm audit --audit-level=high`). Proto has no real dependency-vuln audit — its placeholder `lang/proto/audit.sh` emits `STATUS=N/A REASON=not-applicable-to-this-lang` (always-run, so it always appears; expected).
2. `lang/proto/breaking.sh` invoked unconditionally separately (`buf breaking proto --against ".git#ref=<sha>,subdir=proto"`). Proto's audit-class gate is `breaking.sh`, not `audit.sh` (ADR-0033 §1 + §10:397).

The orchestrator returns the **worst of `(dispatch_rc, breaking_rc)`** — `set -e` short-circuit would mask the second invocation and silently break the always-run guarantee; the explicit RC capture block in `scripts/audit.sh` (no enclosing function; flat script) preserves both gates.

**DEP-CHANGE-GATED as of task #47 (ADR-0033 §3 amendment).** The dispatcher invokes each `audit.sh` wrapper unconditionally — since 2026-08-20 that is simply the dispatcher's default (the `DEVLOOP_DISPATCH_ALWAYS_RUN` opt-in was removed when the per-language changed.sh short-circuit was retired) — but the wrapper then runs a fail-closed dep-manifest gate internally. When no dependency manifest changed, the wrapper emits `STATUS=SKIPPED-NO-DIFF REASON=no-dep-changes` (expected, non-dominating) instead of scanning. The wrapper runs the scan on any doubt (indeterminate diff). When the scan runs and suppressed advisories are filtered, the wrapper emits a `SUPPRESSED=<ids>` stderr line (the configured suppression set in effect this run). `buf breaking` remains always-run. The always-run audit GUARANTEE moved to the Layer-3 `audit-suppressions-check` guard (§6.3) + the **weekly scheduled full scan** (below).

**RUN vs SKIP boundary (narrowed 2026-08-05 — the gate matches ONLY true dep manifests).** On-call reading a `SKIPPED-NO-DIFF REASON=no-dep-changes`: this is EXPECTED whenever the diff touches no dependency manifest — including source-only edits under `crates/**`/`packages/**` (e.g. `crates/*/src/*.rs`, `packages/*/src/*.ts`). Such a source edit provably cannot move the resolved dependency graph, so the ambient scan is skipped. The gate RUNS on: root `Cargo.toml`/`Cargo.lock` or `crates/*/Cargo.toml` (rust); root `package.json`/`pnpm-lock.yaml`/`pnpm-workspace.yaml` or `packages/*/package.json` (ts); ANY indeterminate diff (fail-closed). Prior to this narrowing a source-only edit RAN the scan (a fail-safe over-trigger) and could red on an advisory unrelated to the diff — that over-trigger is intentionally removed. **Residual an operator must know:** a dep-changing devloop still gets a FULL-TREE scan and CAN still red on an ambient advisory unrelated to the specific dep it changed (the scan is whole-lockfile, not attributed to the changed line). Triage that exactly like any Layer-6 advisory — fix-the-dep or suppress in `audit-suppressions.toml` (security-owned, §11); the narrowing does not change the suppression escape hatch. The **weekly scheduled scan** (below) remains the net for the diff-less vector (a new advisory against an UNCHANGED lockfile), which the per-PR gate deliberately skips.

**Scheduled full scan (the drift-catcher).** `.github/workflows/audit-scheduled.yml` runs weekly, forcing the gate ON (`DEVLOOP_AUDIT_FORCE_RUN=1`, force-run-only — bypasses the GATE, not suppressions) so it scans the full FROZEN lockfile against newly-published advisories — the diff-less vector the per-PR gate intentionally skips. It is two jobs: `scan` (read-only token, no persisted git credentials; uploads the output) and `report` (`issues: write`, github-script only). On a failed scan it goes red AND opens-or-updates a single rolling **`audit-drift` GitHub Issue** whose body carries the audit output — read its `STATUS=FAIL REASON=` lines: the cause may be an advisory OR a broken audit tool (the 2026-08/09 issue #70 was `buf-binary-missing` every week). A scan that did not complete (install failure, timeout, cancel) comments "did not complete" and leaves the issue OPEN; only a completed clean scan auto-closes it. Both cases are red runs. On-call: between-PR drift shows up as the `audit-drift` issue + a red `Scheduled Audit` run, NOT a per-PR red — see the two-surfaces note in §6.3.

| REASON token | Wrapper | Cause / Fix |
|--------------|---------|-------------|
| `cargo-audit-failed` | `lang/rust/audit.sh` (`cargo audit`) | RUSTSEC advisory. **Triage decision**: fix-the-dep (preferred) vs suppress-in-manifest (security-owned). The ONLY sanctioned suppression channel is `audit-suppressions.toml` → generated `.cargo/audit.toml` (cargo-audit reads it natively); add an entry + `scripts/audit-suppressions-check.sh --fix` + reviewed PR per `docs/contributor/audit-suppressions.md`. Operators MUST NOT silence advisories ad-hoc via CLI flags, hand-edited `.cargo/audit.toml`, or Dependabot alert dismissals — the wrapper blocks `--ignore=…` pass-through (see the `IMPORTANT (security finding 1)` comment at the top of `lang/rust/audit.sh`). For a transitive-dep advisory we don't own, escalate to security. **Not every red here is an advisory**: `cargo-audit` is pinned (`CARGO_AUDIT_VERSION` in `infra/devloop/Dockerfile`), but the RustSec advisory DB is always fetched fresh, so a DB format change can fail a stale tool with a parse/load error instead of a `RUSTSEC-…` ID. The fix is a deliberate `CARGO_AUDIT_VERSION` bump plus `infra/devloop/devloop.sh --rebuild` (host), not a suppression. |
| `pnpm-audit-unrecognised-output` | `lang/ts/audit.sh` (shape check) | `pnpm audit --json` printed nothing, or no `advisories` object — a pnpm release reshaped its output and the filter would otherwise read zero advisories as clean. Fix: teach `scripts/lang/ts/audit.sh` the new shape (with a case in `audit.test.sh`). NEVER suppress an advisory for this. |
| `pnpm-audit-no-decision` | `lang/ts/audit.sh` (decision step) | The Python decision step printed no `OK`/`FAIL` verdict (python missing or killed). The scan was not evaluated; this is never a pass. Fix the toolchain and re-run. |
| `pnpm-audit-failed` | `lang/ts/audit.sh` (`pnpm audit --audit-level=high` + post-hoc filter) | High-severity npm advisory not in the suppression list. Same triage discipline — suppression goes in `audit-suppressions.toml` → generated `.pnpm-audit-ignore.json` (the wrapper's post-hoc filter reads it), security-owned (ADR-0033 §11). Wrapper blocks `--audit-level=critical` and `--ignore=…` pass-through. A malformed `.pnpm-audit-ignore.json` makes the FILTER apply zero suppressions and proceed (fail-safe + stderr WARN — the scan still reds on real advisories); the Layer-3 check hard-fails the malformed file separately (`suppression-drift`). |
| `buf-breaking-failed` | `lang/proto/breaking.sh` (`buf breaking … --against .git#ref=<base-sha>,subdir=proto`) | Wire-breaking change against the resolved base ref. **No CLI or env bypass exists, by design** — the wrapper does not forward `"$@"`. The ADR-0033 Wave-3 per-finding override (task #41) is still unbuilt, so the only channel for an accepted intentional break is a path-scoped `breaking.ignore` entry in `proto/buf.yaml`, which requires the human acceptance the escalation asks for — an operator MUST NOT add one during triage. When one is present the wrapper prints `SUPPRESSED=<paths>` before the gate runs; see `buf-breaking-passed` + `SUPPRESSED=` below. |
| `buf-breaking-passed` **with a preceding `SUPPRESSED=<paths>` line** | `lang/proto/breaking.sh` — the `SUPPRESSED=` emission (no enclosing function; flat script) | **A green here does NOT mean those paths are break-free.** `proto/buf.yaml` carries a `breaking.ignore` carve-out for the listed paths and buf reports a plain pass for them. Read `proto/buf.yaml`'s comment block for why it exists and its restore condition, and read the listed files' diffs by hand. The line is emitted on every run until the `ignore:` key is deleted — it is the reminder, so do not filter it out. **Not emitted by GitHub CI**: `.github/workflows/ci-client.yml` hand-rolls its own `buf breaking` step, so the carve-out applies there but the warning does not. |
| `base-ref-unresolved` | `lang/proto/breaking.sh` — the `base-ref-unresolved` emission (no enclosing function; flat script) | `_get_base_ref.sh` exited non-zero before reaching `buf breaking`. The wrapper distinguishes this from `buf-breaking-failed` so operators don't chase a wire-break issue when the actual problem is a degraded git state. Jump to §5. |
| `buf-binary-missing` | `lang/proto/breaking.sh` | See §6.1. |
| `not-applicable-to-this-lang` (proto) | `lang/proto/audit.sh` (intentional-gap placeholder) | Expected — proto has no real `audit.sh`; `breaking.sh` is the proto audit-class gate, wired separately in `scripts/audit.sh`. The placeholder emits `N/A` (exit 0). **On a no-dependency-change run Layer 6 reads `N/A` (`audit-aggregate-na`) — that means no audit ran**: rust/ts report `SKIPPED-NO-DIFF no-dep-changes` in their own child lines (the per-run dep-gate fact), proto is the placeholder, and `N/A` ranks above the SKIPPED-* statuses. `TOTAL_RESULT` is still OK. With a dependency change, the real audits run and Layer 6 reads `OK`. |
| `<lang>-audit-verb-missing-or-not-executable` (`FAIL-MISSING-VERB`) | `_dispatch.sh::for_each_lang_with_verb` | A `<lang>/audit.sh` that SHOULD exist is missing/non-executable (deleted, `chmod`-stripped, or a new lang dir added without it) — that lang's dependency-vuln scan did NOT run. The dispatcher emits `FAIL-MISSING-VERB`, which outranks a sibling lang's passing audit (`OK`) in the aggregate → **the layer reds at exit 2** (masking closed by the ladder, task #52 — no separate guard needed). **Fix**: restore `<lang>/audit.sh` + `chmod +x`, or (if the gap is genuinely intended) register an intentional-gap placeholder `<lang>/audit.sh` emitting `N/A`. |

**Exit code for a missing audit wrapper — FAIL-MISSING-VERB / exit 2 (task #52).**
A missing `<lang>/audit.sh` is `FAIL-MISSING-VERB`, which ranks above `OK` in the aggregation
ladder — so the IN-PIPELINE path (`layer6.sh` keys the layer exit on the STATUS stream) aggregates
to `FAIL-MISSING-VERB` → exit 2, and the STANDALONE `./scripts/audit.sh` path returns the dispatcher's
exit 2 directly. **This is an exit-1→exit-2 change from task #50**, which used a post-processor in
`scripts/audit.sh` to emit a synthetic `STATUS=FAIL` (exit 1). Exit 2 is the more honest code: a
missing dep-scan is a WIRING fault ("the gate never ran"), the §6 exit-2 "investigate the script
itself" class — not exit 1 ("work ran and detected a problem"). Both the layer and standalone paths
now agree at 2 (the #50 standalone-vs-layer divergence concern is moot — the ladder, not a
post-processor, drives both). `layer-all.sh` folds any non-zero → `final_exit=1`.

There is no longer an audit-vs-general asymmetry: the audit verb and every other verb
(compile/fmt/lint/test) use the SAME ladder mechanism — a missing wrapper is `FAIL-MISSING-VERB`
(exit 2) and reds the layer regardless of sibling status (masking closed for all verbs, task #52).

**Audit-config ownership reminder**: audit-config changes (`audit-suppressions.toml`, the generated `.cargo/audit.toml` / `.pnpm-audit-ignore.json`, audit-level thresholds, advisory exemptions) are **security-owned** per ADR-0033 §11. Operators should not modify allowlists or suppression flags as part of failure triage — escalate to security. **EXCEPTION — suppression renewal on `suppression-past-due`:** editing `audit-suppressions.toml` to renew an `expires` date (then `--fix` + reviewed PR) IS the sanctioned remediation, NOT a prohibited ad-hoc edit — see the full security-reviewed exception note in §6.3. The prohibition targets silent incident-time silencing (CLI flags, hand-edited derived files, Dependabot alert dismissals, or an unreviewed expires-bump just to unblock CI).

**Worked example — twin-rc collection**:

```
# dispatch stage (DEVLOOP_DISPATCH_ALWAYS_RUN=1):
STATUS=OK REASON=cargo-audit-passed       (rust)
STATUS=OK REASON=pnpm-audit-passed        (ts)
STATUS=N/A REASON=not-applicable-to-this-lang  (proto, placeholder audit.sh)
→ dispatcher aggregates: STATUS=OK REASON=audit-all-langs-ok   (OK outranks the placeholder's N/A)
→ dispatch_rc = 0

# breaking stage (separate):
STATUS=FAIL REASON=buf-breaking-failed    (proto/breaking.sh)
→ breaking_rc = 1

# scripts/audit.sh exit:
→ exit max(0, 1) = 1
→ Layer 6 collects all STATUS lines; aggregate_worst_status OK OK N/A FAIL = FAIL
→ Layer 6 stdout:  STATUS=FAIL REASON=layer6-summary        (generic summary; enum-only)
→ Layer 6 stderr:  LAYER=6 ... RESULT=FAIL REASON=buf-breaking-failed  (worst-child cause)
```

The proto `N/A REASON=not-applicable-to-this-lang` line is the INTENTIONAL gap (proto has
no real `audit.sh`; its placeholder emits `N/A`; `breaking.sh` is its audit gate) — exit 0,
it does not red the layer; the FAIL here is `breaking.sh`. (The dispatch-stage aggregate is `OK`:
since 2026-10-06 the placeholder's N/A ranks below the rust/ts OK.) As in §6.1, the stderr `LAYER=`
anchor names the worst-child cause (`buf-breaking-failed`), not `layer6-summary`.

### 6.7 Layer 7 — Env-tests (`scripts/layer7.sh`)

**Always-run** (ADR-0033 §3): Layer 7 attempts the `crates/env-tests` suite against the live Kind cluster on every devloop — business-logic changes break integration even with no infra/proto diff. The ONLY suppressor is the absence of a cluster to run against (a clean skip, never a failure). Cluster lifecycle is the host-side helper (`infra/devloop/dev-cluster`, ADR-0030).

**Two Phase-2 suites since task #19 (R-48), sequential, same cluster, shared single-attempt budget**: (1) the Rust env-tests (always attempted); (2) the browser E2E (`pnpm --filter @darktower/web-app test:e2e`, the task-#18 Playwright harness) — **also always runs** whenever Layer 7 runs (the diff-trigger `__browser_e2e_triggered` / `__BROWSER_E2E_TRIGGER_PATHS` was retired 2026-08-20; gate coverage is independent of change-detection, so there is no longer a "no diff" case for the browser suite and the `browser-e2e-no-diff` lane is gone). One remaining conditional: env-tests FAIL first → browser suite NOT run that attempt (greppable stderr `browser-e2e-not-run:` note, deliberately NO browser STATUS line — the layer is already FAIL; the browser suite runs on the retry). Browser-suite preconditions (dev-cert fingerprints, Playwright Chromium) are Phase-1(g) checks — operator lane, now run UNCONDITIONALLY on every Layer-7 (a backend-only diff on a workstation lacking dev-certs/Chromium reds here — intended), surfaced BEFORE either suite so they can never masquerade as spec-timeout FAILs. Each suite has its own wall clock (`DEVLOOP_ENV_TEST_TIMEOUT` / `DEVLOOP_BROWSER_E2E_TIMEOUT`, both default 600s). **Operational note**: because Layer 7 now runs on every run-story task, a single missing browser precondition stalls the WHOLE story (operator lane), not just a subset — oncall tracing a stalled story should check for one missing cert/binary, not a task-specific fault. **Known-accepted (do not chase)**: a ~6-minute Layer 7 means `provision` rebuilt the platform, which happens ONLY on a blueprint change (ADR-0038 step 3 — no diff inspection; the `BLUEPRINT ACTION=… REASON=… CHANGED=…` line names why: a changed provision input, a TLS leaf renewal about every 13 days, or a cluster built before step 3 on its first post-merge provision). This is accepted, not a fault to chase.

**Two-phase classifier (the suite-output log-grep is RETIRED — task #56).** Phase 1 (pre-suite: `provision` then `deploy` — ADR-0038, no diff inspection — ports.json/URLs, post-deploy health, observability-stack readiness, per-run-organization provisioning + verification) is the ONLY infra lane. Phase 2 runs the suite on a confirmed-healthy cluster, and **any** non-zero is a test FAIL — we do NOT grep the suite output for `connection refused` etc. (that would let a real test failure whose output contains an infra phrase escape silently as infra — the reverse of the very masking this layer exists to kill). Load-bearing asymmetry: when uncertain, FAIL/loud, never infra/swallow.

**Four terminal lanes** (the `wave2-pending` N/A stub is gone; task #19's browser lane adds REASON tokens within the existing lanes — no new lanes, no aggregation-ladder edits):

| STATUS / REASON token | Origin (in `scripts/layer7.sh`) | Exit | Lane / Cause / Fix |
|-----------------------|----------------------------------|------|--------------------|
| `SKIPPED-NO-CLUSTER` / `no-cluster-ci` | env gate — helper socket ABSENT **and** `GITHUB_ACTIONS` set (CI) | 0 | **Expected on CI ONLY** — GitHub Actions has no Kind cluster and we don't provision one. The ONLY clean-skip case; ranks below OK so a green CI run is `TOTAL_RESULT=OK`. NOT a failure. (Socket-presence is checked FIRST, so a future CI that DID provision a helper would *run*, not skip — forward-compatible. A *local* run with no/dead helper is NOT this lane — it's loud `PRECONDITION_FAILURE`.) The only exit-0 no-cluster skip is CI; a local devloop with no reachable helper is always loud `PRECONDITION_FAILURE`, so env-tests cannot silently skip on a local code devloop. |
| `OK` / `env-tests-passed` | Phase 2, suite exit 0 | 0 | Suite green. |
| `FAIL` / `env-tests-failed` | Phase 2, suite non-zero | 1 | **IMPLEMENTER lane.** A test regression on a confirmed-healthy cluster (incl. a 600s `timeout`/rc-124 hang). Read the full suite output at `${DEVLOOP_TMP:-/tmp/devloop}/layer-7-env-test.log`; fix the failing test/code. Consumes a Layer-7 attempt. **One documented exception to "fix the failing test/code":** if the log carries `Triage Prometheus/port-forward` (the counter-delta helpers' fail-loud panic), Prometheus went away mid-suite after Phase 1f proved it ready — the lane classification stands (Phase 2 never greps suite output), but the fault is not the diff. See §8. |
| `PRECONDITION_FAILURE` / `local-helper-not-running` | env gate — NOT CI and no helper socket (local devloop, helper not started) | 2 | **OPERATOR lane** — a local devloop ALWAYS expects a cluster; a missing helper is loud, NEVER a silent skip (this is the silent-skip hole the task closes). Start the devloop cluster (`devloop.sh`); to run only layers 1-6 invoke the individual `scripts/layerN.sh`. |
| `PRECONDITION_FAILURE` / `helper-unreachable` | helper-liveness probe (Phase-1a) — socket PRESENT but `dev-cluster status` connection-refused (helper crashed / stale socket); OR the Phase-1e self-heal's own bounded probe could not reach the helper | 2 | **OPERATOR lane** — a cluster WAS expected but the helper is gone; this is deliberately NOT a clean skip (masking a crashed helper would re-open the silent-skip). Re-run `devloop.sh` on the host to restart the helper; check `/tmp/devloop/helper.log` + `helper-stderr.log`. **Can ALSO arise mid-self-heal** — a `SELF_HEAL CASE=helper-unreachable` line precedes it when the Phase-1e self-heal's `dev-cluster status` probe errored/timed out (rc≠0/timeout ⇒ "couldn't ask", distinct from `probe-inconclusive` which is rc==0-but-unparseable); the host fix is identical. |
| `PRECONDITION_FAILURE` / `cluster-provision-failed` | Phase 1a — `dev-cluster provision` (the helper runs `infra/kind/scripts/provision.sh`) failed on the **ENVIRONMENT**: its `PROVISION_FAILED REASON=` is one of `scripts/layer7.sh:CLUSTER_ENV_REASONS`, OR provision printed no PROVISION_FAILED line at all | 2 | **OPERATOR lane** — does NOT consume an implementer attempt (provision.sh classifies each failure at the SOURCE; everything else is the implementer lane — see `provision-failed` below). Reasons: `runtime-unreachable` — a bounded `<runtime> info` got no answer (the host container runtime is down or hung); `apiserver-unreachable STEP=<step>` — a step AFTER `kind create cluster` failed and a bounded `/readyz` probe got no answer (never reported for a pre-create step: there is no apiserver yet); `port-held STEP=check_host_ports` — another process holds a host port the kind config maps (the line names `<addr>:<port>`, `/udp` for a UDP mapping; TCP by a bounded connect, UDP from the host's bound-socket table `/proc/net/udp{,6}`; stop the holder — another cluster, a stray port-forward — or pick another `DT_CLUSTER_NAME` slot); `runtime-incapable STEP=create_cluster` — `kind create cluster` failed AND the same privileged, own-hostname container on the kind node image fails too: the runtime answers but cannot run a kind node here (nested podman's `crun: sethostname`, docs/TODO.md §G) — a host-side fix, a retry cannot help; `operator-declined` — a rebuild needed a destroy the operator did not confirm (TTY `N`, or an unnamed cluster under `--yes`); `blueprint-unreadable` — see `ACTION=refuse` below; `prerequisite-missing` — `kind`/`kubectl`/`timeout` (it bounds every environment probe; stock macOS lacks it — `brew install coreutils`)/a container runtime is missing. **No PROVISION_FAILED line** = the helper or its transport failed, or provision.sh was killed: inspect `/tmp/devloop/helper.log`. **Read the relayed `BLUEPRINT ACTION=… REASON=… RECORDED=… CURRENT=… CHANGED=…` line first** (`${DEVLOOP_TMP:-/tmp/devloop}/layer-7.stderr.log`; one per run — `grep '^BLUEPRINT '` across runs answers "why did this gate take ~6 minutes"): `ACTION=none REASON=match` = nothing rebuilt; `ACTION=rebuild REASON=missing` = no cluster, a HALF-BUILT one (the record is written only after a successful build) or one built before ADR-0038 step 3 — **every existing cluster rebuilds ONCE on its first post-merge provision**; `REASON=changed CHANGED=<sections>` names what changed (`tls-renewal-due` = a 14-day WebTransport leaf was renewed — expect one rebuild every ~13 days on a long-lived cluster); `ACTION=refuse REASON=unreadable` = the record (or `kind get clusters`) could not be READ — provision refuses to destroy on uncertainty: check the host container runtime; if the control plane is dead, `dev-cluster recreate` re-confirms before destroying. A failed build leaves no record, so the next provision rebuilds rather than reusing a half-built cluster. Inspect `/tmp/devloop/helper.log`. |
| `FAIL` / `provision-failed` | Phase 1a — `dev-cluster provision` failed on the **TREE**: its `PROVISION_FAILED REASON=` is NOT an environment reason — `step-failed STEP=<step>` while the runtime (and, post-create, the apiserver) answered: `create_cluster` (a kind config the tree changed; the node-container probe ran and passed, or could not run — no local node image, WARNed), `install_calico` (a pin/sha256 mismatch against the fetched manifest, or a Calico rollout timeout), `create_ac_secrets` / `create_tls_secret` (a provision input the diff edited, e.g. `infra/services/postgres/secret.yaml`), `verify_blueprint` (the tree changed during the build), `record_blueprint`. An unrecognised REASON also lands here, fail-closed. | 1 | **IMPLEMENTER lane** — consumes an attempt; nothing after provision runs. Most likely when the diff touched a provision input (`provision.sh`, `lib/common.sh`, the kind config, `scripts/generate-dev-certs.sh`, `infra/services/postgres/secret.yaml`); if it did not, re-run once — a Calico timeout on a loaded host is the one step-failed cause that is not the tree. |
| `PRECONDITION_FAILURE` / `cluster-deploy-failed` | Phase 1b — `dev-cluster deploy` (the helper runs `infra/kind/scripts/deploy.sh`) failed on the **ENVIRONMENT**: its `DEPLOY_FAILED REASON=` is one of `scripts/layer7.sh:CLUSTER_ENV_REASONS` (one list for both verbs), OR deploy printed no DEPLOY_FAILED line at all | 2 | OPERATOR lane (deploy.sh classifies each failure at the SOURCE; everything else is the implementer lane — see `deploy-failed` below). Reasons: `blueprint-changed` / `blueprint-missing` / `blueprint-unreadable` — deploy's own guard (`provision.sh --check`) found the platform stale, absent or unreadable: run `dev-cluster provision` (Layer 7 always provisions first, so on a gate this means the tree changed between the two steps); `runtime-unreachable` — a bounded `<runtime> info` got no answer (the host container runtime is down or hung; the same shared probe as provision, `lib/common.sh:classify_env_failure`); `apiserver-unreachable STEP=<step>` — a step failed and a bounded `/readyz` probe got no answer (the cluster, not the diff); `insufficient-disk` — host container-storage below `DEVLOOP_MIN_DISK_GB` (default 15 GB, a fast-fail floor): reclaim space (`podman image prune -f && podman builder prune -f`; per §4) and re-run; `prerequisite-missing` — `kind`/`kubectl`/`timeout` (it bounds every environment probe; stock macOS lacks it — `brew install coreutils`)/a container runtime is missing on the host; `image-ref-read-failed` — the pre-deploy image refs could not be READ (apiserver/kubectl). **No DEPLOY_FAILED line** ("deploy produced no DEPLOY_FAILED line") — deploy.sh prints one on every failure it survives, so its absence means the helper or its transport failed, or deploy.sh was killed: inspect `/tmp/devloop/helper.log` (its audit entry carries a `deploy_failed` field when there was one). `PRUNE_WARN REASON=image-prune-failed` lines are hygiene WARNs (deploy continues; `WHERE=history` = the rollout history could not be read and that repo was not pruned). |
| `FAIL` / `deploy-failed` | Phase 1b — `dev-cluster deploy` failed on the **TREE**: its `DEPLOY_FAILED REASON=` is NOT an environment reason (ADR-0038: deploy applies the diff on every gate, so a code-caused deploy failure is the common case) | 1 | **IMPLEMENTER lane** — consumes an attempt; nothing after deploy runs. Reasons: `rollout-failed WORKLOADS=<ns/kind/name,…>` — a workload did not roll (a CrashLoop on a new required key, a bad image; diagnostics — pods, describe, events — are dumped above; **observability**: a config Prometheus rejects crash-loops the new pod — `kubectl -n dark-tower-observability logs deploy/prometheus`; a projected-volume source Grafana's dashboards need that does not exist blocks pod start — `kubectl -n dark-tower-observability describe pod -l app=grafana`; PostgreSQL/Redis not finishing their rollout before the migration Job shows `WORKLOADS=dark-tower/statefulset/<postgres\|redis>`); `collector-rollout-failed` — the OTel collector did not roll out within 180s and deploy STOPPED before the services (R-54; `kubectl -n dark-tower logs deploy/otel-collector`; gc-deployment.md §OTel Collector Upgrade Discipline); `step-failed STEP=<step>` — any other failure while the apiserver answered: a manifest the apiserver rejects (`STEP=apply_env_root`), an image build (`build_first_party_images`), a migration (`run_migration_job`, with `MIGRATION_FAILURE: job/db-migrate-<hash> failed (reason=…) REASON=migration-failed` above it — its logs, every attempt, are dumped: `previously applied but has been modified` = an applied migration file was edited — restore it, or recreate the dev cluster (`dev-cluster teardown`, then `dev-cluster provision` + `dev-cluster deploy`; no down-migrations; **never hand-edit `_sqlx_migrations`**); `REASON=migration-timeout` (reason `deploy-poll-timeout`, or the Job's own `DeadlineExceeded`) = the Job never finished: Postgres unreachable (NetworkPolicy, pod not Ready) or the image not loaded (`ErrImageNeverPull`); a Failed Job is re-created on the next deploy; live record: `kubectl get jobs -n dark-tower -l app=db-migrate`); a seed. **An unrecognised REASON** also lands here, fail-closed, and the message says so — if it is genuinely environmental, add it to `CLUSTER_ENV_REASONS` (a drift test in `scripts/layer7.test.sh` extracts every reason deploy.sh, provision.sh and `lib/common.sh` can emit). |
| `PRECONDITION_FAILURE` / `helper-verb-unsupported` | Phase 1a/1b — the host helper answered `invalid command: provision` (or `deploy`) `(invalid_command)`: it was built from a tree OLDER than the devloop's (ADR-0030: the helper cannot validate its own changes) | 2 | OPERATOR/HOST lane — version skew, not a cluster fault. Rebuild and restart the helper on the HOST from the current tree (re-run `infra/devloop/devloop.sh`), then re-run. Never fall back to an older verb. |
| `PRECONDITION_FAILURE` / `ports-json-missing` | Phase 1d — `/tmp/devloop/ports.json` absent/unreadable | 2 | OPERATOR lane. The helper writes `ports.json` on a successful `setup`; ensure setup completed. |
| `PRECONDITION_FAILURE` / `cluster-unhealthy` | Phase 1e — `__wait_cluster_ready` timed out AND the self-heal probe classified the cause as `SELF_HEAL CASE=rollout-wedged` (control-plane container up, apiserver TCP-reachable on its host binding, but pods NOT ready) | 2 | OPERATOR lane, but **most likely the diff's own code** — a wedged rollout (ImagePullBackOff/CrashLoopBackOff) on a *healthy* control plane. Deliberately NOT self-healed: recreating a cluster whose apiserver is reachable would mask a code defect. `dev-cluster status` names the not-ready pods; check pod logs. **Do NOT teardown+setup.** Absence rule: `cluster-unhealthy` *with* a preceding `SELF_HEAL CASE=rollout-wedged` = self-heal ran and correctly declined; `cluster-unhealthy` with **no** `SELF_HEAL CASE=` line = self-heal did not run (a regression) — the absence is the greppable signal. A `SELF_HEAL RESULT=recovered` line NEVER precedes a bare `cluster-unhealthy` (recovered means the re-verify passed — an invariant). |
| `PRECONDITION_FAILURE` / `cluster-self-heal-failed` | Phase 1e self-heal — a bounded, once-per-helper-lifetime fix ran (`SELF_HEAL RESULT=failed …`) and the cluster still did not become healthy on re-verify | 2 | **OPERATOR/HOST lane.** The automated recreate/restore already ran and failed — **do NOT re-run teardown+setup, the self-heal just tried exactly that.** Read the pre-destroy evidence bundle at `${DEVLOOP_TMP:-/tmp/devloop}/self-heal-evidence-<ts>/` (path echoed as `EVIDENCE=` on the `SELF_HEAL` line; on the host, under `/tmp/devloop-<slug>/`) + the helper log. The `DETAIL=` field names which: `recreate-failed`/`restore-failed` (fix ran, still unhealthy — check host disk/resources for a fresh KIND cluster, `podman image prune -f`, §4 disk sub-cause), or `recreate-already-attempted-this-helper-lifetime` (the destroy bound was already spent — the bound is an **in-helper-process flag** (a check-and-set in the helper's own memory, deliberately NOT an on-disk marker: the container↔helper `${DEVLOOP_TMP}` mount is read-write at the same uid, so a file marker would be container-clearable and give no protection). It clears only when the helper PROCESS restarts (`devloop.sh` on the host) — NOT when the operator restarts their devloop run — so a crashed run never poisons the next, but one live helper never destroys twice). Escalate to host/operator (SKILL §Recovery). |
| `PRECONDITION_FAILURE` / `cluster-self-heal-probe-inconclusive` | Phase 1e self-heal — could not safely determine a fix; `SELF_HEAL CASE=probe-inconclusive ACTION=none`, OR the recreate was refused by the host's re-confirmation (`SELF_HEAL RESULT=failed DETAIL=recreate-refused-cluster-alive`) | 2 | **OPERATOR lane, fail-closed** — the self-heal never destroys on an uncertain state. **Two sub-cases, DIFFERENT next actions — read the `SELF_HEAL` line:** (a) `CASE=probe-inconclusive` (Reachability `Unknown` — a TCP connect to the apiserver binding that was **refused / unroutable / timed out**, or an **absent/unparseable apiserver port**, or `Cluster write in progress: true`, or any unmatched combo) → the probe could not positively license a destroy (only a not-running control-plane container does). **The common concrete case: the control-plane container is RUNNING but nothing is listening at `HOST_GATEWAY_IP:HOST_PORT_K8S_API`** — the container is up but the apiserver process inside it crashed or is not yet serving. This is a real, diagnosable state, deliberately NOT auto-recreated (a live container is not a dead cluster): inspect `<runtime> logs <cluster>-control-plane` (podman/docker per `dev-cluster status`), and recreate BY HAND only after confirming the apiserver is genuinely dead. Otherwise determine cluster state via `dev-cluster status` + the helper log. Fail-closed by design — a refused connect is treated as "cannot tell", never as "dead". (b) `DETAIL=recreate-refused-cluster-alive` → the helper's own host-side probe reported the control plane unreachable at classify time (T1) and `Reachable` moments later at the destroy gate (T2) — two host-side observations that DISAGREE (a flap/race, or the cluster came up between them), so the destroy was correctly declined. **NOT a container-visibility or kubeconfig problem** — the container never probes independently; both looks are the same host-side `probe_apiserver_reachable` (inspect-based, kubeconfig-independent), so `restore-kubeconfig` cannot change the verdict. The cluster is very likely fine: re-run `dev-cluster status`; if healthy, re-run the devloop. If it repeats, the control plane is FLAPPING — `helper.log` carries both observations. Do NOT force a destroy. (c) `DETAIL=recreate-refused-inconclusive` → the host could NOT determine the control-plane state (the container runtime is down, or `inspect` errored/timed out) and refused — nothing was confirmed, alive OR dead (fail-closed). **Different next action from (b):** check the HOST CONTAINER RUNTIME (is the podman/docker daemon up?) + `helper.log` (the helper's refusal line records the observed `reachability=`/`cluster_exists=`) — NOT the kubeconfig, which dead-ends for the same reason the refusal fired. Cluster state is UNKNOWN, not alive; do NOT force a destroy until the runtime is back and `dev-cluster status` can answer. |
| `PRECONDITION_FAILURE` / `observability-prometheus-not-ready` | Phase 1f — Prometheus `/-/ready` != 2xx within budget | 2 | OPERATOR lane. The metrics tests one-shot-query Prometheus, so a cold/down Prometheus would flake them — Phase 1f waits for it (HARD probe) and trips loud if it never readies. Check the prometheus pod + logs. |
| `PRECONDITION_FAILURE` / `org-provision-context-unresolved` | Phase 1h — `ports.json` `.cluster_name` empty/absent, or `kubectl` not on PATH | 2 | **OPERATOR lane.** A cluster-ADDRESSING problem, **not** a database problem — read the cause line before reaching for psql. Layer 7 refuses to provision the per-run org when it cannot prove *which* cluster it would write to; there is deliberately no `kubectl config current-context` fallback, because on a host that also has a manually created `dark-tower` cluster that fallback *resolves* and would silently target an unrelated cluster — provisioning into a database the suites never read. Ensure `dev-cluster provision` completed and wrote `/tmp/devloop/ports.json` (`jq .cluster_name /tmp/devloop/ports.json`), and that the container kubeconfig is mounted (re-run `devloop.sh` on the host). |
| `PRECONDITION_FAILURE` / `org-provision-timeout` | Phase 1h — `setup.sh --provision-org` exceeded `DEVLOOP_ORG_PROVISION_TIMEOUT` (default 120s), rc 124 | 2 | **OPERATOR lane.** Phases 1e/1f already passed, so the cluster and the NodePort path were healthy moments earlier — a hang here points at the K8s API server or the `postgres-0` pod, not at the diff. `kubectl -n dark-tower get pods`; check `postgres-0` and its logs. Raise `DEVLOOP_ORG_PROVISION_TIMEOUT` only after ruling out a wedged pod — the budget exists to stop a wedged provisioning step hanging the devloop (and, under `run-story`, the whole story) through no lane at all. |
| `PRECONDITION_FAILURE` / `org-provision-failed` | Phase 1h — `--provision-org` exited non-zero, OR its `PROVISIONED_ORG ` line was absent (fail-closed positive match), OR the subdomain generator's entropy postcondition failed | 2 | **OPERATOR lane.** The suites have no organization to run against. The relayed `setup.sh` output in `${DEVLOOP_TMP:-/tmp/devloop}/layer-7.stderr.log` carries the psql diagnostic — note psql under `-v ON_ERROR_STOP=1` exits **3**, not 1. Verify the postgres pod is up and migrations ran: `kubectl -n dark-tower exec postgres-0 -c postgres -- psql -U darktower -d dark_tower -c "\dt"`. **Entropy sub-cause:** if the cause line names the `/dev/urandom` read rather than SQL, the fault is environmental — check `/dev/urandom` is readable in the container and `od`/`tr` are on PATH; it is *not* a subdomain-format problem. |
| `PRECONDITION_FAILURE` / `ac-unreachable` | Phase 1h — AC `/health` not 2xx within budget, OR `ports.json` has no `.container_urls.ac`, OR the org-resolution probe returned anything other than 401/404/2xx (no response `000`, 5xx, or any other status) | 2 | **OPERATOR lane, and explicitly NOT a provisioning fault** — the org may well be fine; AC simply could not be asked. Unlike `org-provision-unverified`, **re-running Layer 7 is a reasonable action here.** Sub-causes, all named in the cause line: (a) **AC down** — `/health` never went 2xx; check the `ac-service` pod + logs. (b) **no AC address** — `ports.json` lacks `.container_urls.ac`; re-run `dev-cluster provision` (`jq .container_urls /tmp/devloop/ports.json`). (c) **no HTTP response (`000`)** — curl transport failure or the probe's own cap (`DEVLOOP_ORG_PROBE_TIMEOUT`, default 10s). **The most likely real one**, and the only new flake this step introduces: the probe is a POST with its own budget, and `/health` passing moments earlier does not bound the token path's latency. The dominant cost is **not** the database — it is AC running **bcrypt at cost 12 unconditionally**, against a dummy hash, even for the non-existent account this probe posts (a deliberate constant-time mitigation: `crates/ac-service/src/services/token_service.rs`, "Always run bcrypt to prevent timing attacks"). That is **CPU-bound**, so a throttled, cold or contended Kind node stretches it while `kubectl -n dark-tower get pods` shows everything healthy — do not go hunting the `organizations` table or the DB. If it recurs, **raise `DEVLOOP_ORG_PROBE_TIMEOUT`** rather than re-running blind; a one-off is worth one re-run. Note the blast radius of getting this wrong: under `scripts/workflow/run-story.sh` this lands as `PIPELINE-PRECONDITION` → **exit 2, halting the whole story** for operator intervention. (d) **5xx** — AC answered but failed internally; look at AC's own dependencies (database, JWKS), not the `organizations` table, since `org_extraction` returns 404 (not 5xx) when an org does not resolve. **NB — 2xx is NOT a sub-cause of this token.** It has its own lane, `ac-auth-bypass-signature` (next row), because AC answering *successfully* falsifies this token's own claim that AC "could not be asked", and because the one action this row endorses — re-running — is the worst possible response to a bypass signature. **NB — 429 is deliberately not listed as a live sub-cause.** It cannot occur from this probe today: `issue_user_token`'s failed-attempt limiter is inside its `if let Some(ref u) = user` block (returning `AcError::TooManyRequests`) and the probe's email cannot exist; the other 429 sources are `issue_service_token` (the client-credentials path, returning `AcError::RateLimitExceeded`) and `register_user`'s IP-keyed registration limiter, and AC has no rate-limit middleware. If a 429 ever appears here it means AC has GAINED a limiter this check does not model — the catch-all routes it to this token naming the code, and both the probe and this row then need updating. |
| `PRECONDITION_FAILURE` / `ac-auth-bypass-signature` | Phase 1h — the org-resolution probe returned a **2xx**: AC issued a SUCCESS for an account that cannot exist | 2 | **OPERATOR lane, but this is a SECURITY finding, not an environment one — and it is the one Phase-1h lane where re-running is the wrong action.** The probe POSTs `layer7-provision-probe@invalid.test`, an address nothing ever registers, precisely so that the only correct answers are 401 (org resolved, credentials rejected) and 404 (org not resolved). A 2xx means AC accepted credentials for a non-existent user — an authentication-bypass signature in the user-token path. **Do not re-run** (a second green run buries it) and **do not raise any timeout** (nothing here is a latency problem). Note the org-resolution question also went unanswered, so the per-run organization is unverified as well. **Capture the evidence before anything is redeployed or torn down**: re-issue the probe with `curl -i` (the full invocation is in the emitted `Fix:` line, with the run's own Host header substituted), then `kubectl -n dark-tower logs -l app=ac-service --tail=200`. Escalate to the auth-controller and security owners. **Why this is not folded into `ac-unreachable`:** that token asserts "AC could not be asked", which a prompt 2xx falsifies outright — filing a bypass signature under a name meaning "the service was unreachable" is the same misattribution class R-7 exists to remove, and it would inherit that row's "re-running is reasonable" guidance. **Why it is not dead-lane drift** (cf. the 429 note above): unlike 429, a 2xx here is not unreachable by inspection — it is the signature of a regression in AC's auth path. |
| `PRECONDITION_FAILURE` / `org-provision-unverified` | Phase 1h — AC healthy and ANSWERED, but the org-resolution probe returned **404** | 2 | **OPERATOR lane, and NOT a flake — re-running will not change it.** 404 is the *only* status that evidences a provisioning fault, which is why this token now fires on it alone: provisioning reported success but AC will not resolve the org. A token request with `Host: <run-org>.<ac-authority>` returns 401 when the org resolves (credentials rejected) and 404 when AC's `org_extraction` falls through `get_by_subdomain`'s `WHERE subdomain = $1 AND is_active = true` — i.e. the row is missing or inactive. Every other non-401 status routes away from this token — 2xx to `ac-auth-bypass-signature`, everything else to `ac-unreachable` — because this row's "re-running will not change it" claim is only true for a genuine resolution answer; asserting it for a transport failure sent operators to inspect a row that was fine. Inspect it directly: `kubectl -n dark-tower exec postgres-0 -c postgres -- psql -U darktower -d dark_tower -c "SELECT subdomain, is_active FROM organizations WHERE subdomain = '<run-org>'"` (the subdomain is on the `Layer7: provisioning per-run organization subdomain=…` line in `${DEVLOOP_TMP:-/tmp/devloop}/layer-7.stderr.log`; the `org_id` is on the relayed `PROVISIONED_ORG` line in the same file). |
| `OK` / `browser-e2e-passed` | Phase 2 browser lane (task #19), suite exit 0 | 0 | Browser E2E green (only emitted when the diff triggered the lane). |
| `FAIL` / `browser-e2e-failed` | Phase 2 browser lane, suite non-zero | 1 | **IMPLEMENTER lane.** A browser-spec regression on a confirmed-healthy cluster. Suite output: `${DEVLOOP_TMP:-/tmp/devloop}/layer-7-browser-e2e.log`; Playwright traces/screenshots: `packages/web-app/test-results/` — retained on failure, **outside** `DEVLOOP_TMP`'s per-run cleanup, gitignored, **can contain live tokens** (traces record request/response bodies) — local-only, treat as sensitive. Consumes the shared Layer-7 attempt. **One documented exception** (see §8, keyed on `Triage Prometheus/port-forward`): the browser suite's MC metric helpers (`mcMetrics.ts`) fail LOUD if Prometheus goes away *mid-suite* (after Phase-1f proved it ready), which lands here as `browser-e2e-failed` but is substantively an OPERATOR-lane fault — do not chase the diff; the throw text says so. Deliberately NOT auto-classified: Phase 2 does not grep suite output (anti-reverse-masking). |
| `PRECONDITION_FAILURE` / `dev-certs-missing` | Phase 1g — fingerprints JSON missing `MC_CERT_SHA256`/`MH_CERT_SHA256` | 2 | OPERATOR lane. Without both pinned hashes the browser refuses the MC/MH self-signed certs (`serverCertificateHashes`) and every join spec could only time out. Run `scripts/generate-dev-certs.sh` on the host; restart any running Vite dev server (fingerprints are read at Vite config time). |
| `PRECONDITION_FAILURE` / `playwright-browser-missing` | Phase 1g — no Chromium under `PLAYWRIGHT_BROWSERS_PATH` (default `/opt/ms-playwright`) | 2 | OPERATOR lane. `pnpm exec playwright install chromium` (the devloop image is expected to bake it — see `infra/devloop/Dockerfile`). |
| `PRECONDITION_FAILURE` / `mh-grpc-port-unallocated` | Phase 1i — `ports.json` present but has no `.ports.mh_0_grpc` / `.ports.mh_1_grpc` (a different fault from `ports-json-missing`) | 2 | OPERATOR lane. The helper allocates both per slug; re-run `dev-cluster provision` so it rewrites `ports.json`, and inspect `jq .ports /tmp/devloop/ports.json`. |
| `PRECONDITION_FAILURE` / `mh-pod-unresolved` | Phase 1i — not exactly ONE live (non-terminating) pod with label `instance=mh-N` | 2 | OPERATOR lane. The MH teardown env-test calls and reads ONE identifiable handler, so Layer 7 never guesses between pods. Usually a rollout still settling after `deploy`: `kubectl -n dark-tower get pods -l instance=mh-N`, wait, re-run. |
| `PRECONDITION_FAILURE` / `mh-grpc-port-occupied` | Phase 1i — something already accepts on `127.0.0.1:<.ports.mh_N_grpc>` BEFORE the forward starts | 2 | OPERATOR lane. Almost always Layer 7 running OUTSIDE the devloop container: on the host, Kind's `extraPortMappings` hold that number (and accept connections with nothing behind them), which is exactly why the check exists — a readiness probe would otherwise pass against the wrong listener. The check is a bounded poll (`DEVLOOP_MH_FORWARD_BUDGET`) after the stale-forward reap, so a just-reaped forward releasing the port is not a false positive; the reap's `stale MH gRPC port-forwards reaped: N` line says what it killed. Run inside the container. It has no `ss`/`lsof`/`ps`, so list listeners with `cat /proc/net/tcp`. |
| `PRECONDITION_FAILURE` / `mh-grpc-forward-not-ready` | Phase 1i — the `kubectl port-forward` to the MH pod's gRPC port exited, or did not accept within `DEVLOOP_MH_FORWARD_BUDGET` (default 30s), or `/proc` is unreadable so stale forwards cannot be reaped | 2 | OPERATOR lane. If the message says `/proc` is unreadable, there is no forward log: Layer 7 must run inside the devloop container, which has a normal `/proc`. Otherwise read `${DEVLOOP_TMP}/layer-7-mh-N-forward.log` and check the pod is Ready. The forwards are stopped on every exit path by the lifecycle cleanup hook (`_common.sh::layer_register_cleanup`), never by a `trap` in the layer script. A forward dies with its pod and is not re-established: an MH restart DURING the suite surfaces as a connection error in the env-test, not here. |
| *(stderr only)* `browser-e2e-not-run:` | Phase 2 — env-tests failed first, browser lane triggered | (1, from `env-tests-failed`) | Not a lane of its own: the shared single-attempt budget means a red Rust suite skips the browser suite (running it against a possibly-diff-broken backend adds attribution noise). Fix the env-test failures; the browser suite runs on the retry. Deliberately NO browser STATUS line in this state. |

**Self-heal recovery recipe (Phase 1e, `SELF_HEAL_*`) — distinct from the `helper-unreachable` row.** When `__wait_cluster_ready` times out (the sole `cluster-unhealthy` site — `scripts/layer7.sh::__self_heal_cluster`), the self-heal does NOT blindly recreate. It probes the REAL cause via a *positive* host-helper signal (`crates/devloop-helper/src/commands.rs::probe_apiserver_reachable`: control-plane container running + a bounded, **HTTP-status-agnostic TCP connect** to the apiserver's real host binding `HOST_GATEWAY_IP:HOST_PORT_K8S_API` — never a parsed kubectl error string, never `/readyz`; a *listening-but-wedged* apiserver reads as `Reachable`, so we escalate rather than destroy), emits `SELF_HEAL CASE=<…> ACTION=<…>` once the bounded probe returns a verdict (on every reached path, including the no-action ones), and applies at most ONE fix:

| `SELF_HEAL CASE=` | `ACTION=` | Meaning / outcome |
|---|---|---|
| `apiserver-unreachable` | `recreate` | Control-plane **container not running** — the SOLE positive destroy license → `dev-cluster recreate` (a host verb that INDEPENDENTLY re-confirms the control-plane container is down before it destroys, captures a pre-destroy `EVIDENCE=` bundle, and destroys only this slug's cluster). Bounded once per **helper-process lifetime** (an in-helper-memory check-and-set, no on-disk marker). Re-verifies → `SELF_HEAL RESULT=recovered` (proceed) or `RESULT=failed` (→ `cluster-self-heal-failed`). **A TCP-refused / unroutable apiserver does NOT reach here** — a refused connect is `Unknown`, not `Unreachable` (→ `probe-inconclusive`, escalate); only a not-running control-plane container licenses the destroy, so a healthy cluster whose apiserver merely refused one connect is never destroyed. |
| `kubeconfig-stale` | `restore-kubeconfig` | The cluster became ready AFTER `__wait_cluster_ready` expired (reachable + pods healthy + not setup-in-progress) — a **timing/race**, NOT a detected stale kubeconfig: every input is host-side, so nothing here observes (or can detect) the container's kubeconfig. `dev-cluster restore-kubeconfig` is a **prophylactic refresh** of the container kubeconfig the suites consume (`KUBECONFIG=/tmp/devloop/kubeconfig`), NOT a repair of a diagnosed fault — **restore ONLY, reaches no teardown/delete/create path, derives the port from the live cluster**. Re-verifies → recovered/failed. *(The `kubeconfig-stale` CASE name is a known misnomer — the real cause is ready-late; the rename to `ready-late` is a Gate-1-frozen-enum change deferred to a follow-up, see `docs/TODO.md` §Documentation Hygiene.)* |
| `rollout-wedged` | `none` | Apiserver TCP-reachable but pods aren't settling → bare `cluster-unhealthy` (pod-logs row above). Never recreated. |
| `probe-inconclusive` | `none` | Reachability `Unknown` (TCP refused/unroutable/timeout, absent port) / setup-in-progress / unmatched combo → `cluster-self-heal-probe-inconclusive`. Fail-closed — TCP can only demote to `Unknown`, never license a destroy. |
| `helper-unreachable` | `none` | The bounded `dev-cluster status` probe itself couldn't reach the helper → `helper-unreachable` (host lane). |

The terminal state after a fix is ALWAYS `SELF_HEAL RESULT=recovered` or an escalation — a bare `cluster-unhealthy` never follows `recovered`. The destroy is bounded once per container session and re-confirmed host-side, so a healthy cluster is never destroyed (a false "unreachable" fails closed to escalation, not to a destroy). **Positive control:** no `SELF_HEAL CASE=` line on a `cluster-unhealthy` log means the self-heal block never ran (a regression) — the absence is the signal. **Contrast the `helper-unreachable` row:** that lane is a dead/crashed HELPER (host-only, restart via `devloop.sh`); this recipe is for a LIVE helper against an unhealthy cluster. ADR-0030 is the host-side lifecycle contract. **Path spelling:** every `/tmp/devloop/…` path in the `SELF_HEAL` lines (`EVIDENCE=`, log references) is the CONTAINER view; on the HOST the same per-slug files are under `/tmp/devloop-<slug>/` (`infra/devloop/devloop.sh` bind-mounts host `/tmp/devloop-<slug>` → container `/tmp/devloop`). An operator recovering on the host translates `${DEVLOOP_TMP}/self-heal-evidence-*` → `/tmp/devloop-<slug>/self-heal-evidence-*` (full recover-a-dead-cluster steps in SKILL §Recovery). **Two vocabularies overlap on purpose — do not "consistency-fix" either direction:** `SELF_HEAL CASE=` values and pipeline `REASON=` tokens are distinct sets. `helper-unreachable` is deliberately **byte-identical** in both (the self-heal reuses the existing `helper-unreachable` REASON), whereas the `CASE=probe-inconclusive` value maps to the deliberately **different** `REASON=cluster-self-heal-probe-inconclusive` token (grepping `REASON=probe-inconclusive` finds nothing — that is the `CASE=` spelling). A future edit that "aligns" these would break either the reuse or the distinction.

**Phase 1f — observability-stack readiness (per-probe; task #56 user-ruling (a)).** After pods-healthy (1e), Layer 7 waits for the observability HTTP endpoints the suite probes — the same `ENV_TEST_*_URL` the suite uses, so no gate-vs-suite drift — with **per-probe disposition** (NOT a blanket suppressor): **Prometheus HARD** (`/-/ready` → `PRECONDITION_FAILURE observability-prometheus-not-ready` if it never readies, since the metrics tests one-shot-query it) and **Loki SOFT** (`/ready` → on budget-expiry a loud greppable `WARN LOKI_NOT_READY_AFTER=<n>s` that pre-attributes the eventual `test_all_services_have_logs_in_loki` failure to the observability stack — "NOT your diff" — then PROCEEDS, honoring the crate's optional-Loki semantics). Grafana is skipped (no suite queries its HTTP API). This closes the cold-start gap: a cold-but-coming Loki becomes a Phase-1 WAIT (→ suite green), not a Phase-2 FAIL. A **genuinely-ABSENT** Loki still surfaces as a loud Phase-2 FAIL (no masking) until the deferred env-tests-crate fix (`docs/TODO.md` §Env-Test Resilience — `is_loki_available` retry / conditional-skip). The HTTP probe is a `DEVLOOP_TEST`-gated seam (`HTTP_PROBE`, fixed `curl` in production), same class as the other layer7 seams.

**Phase 1h — per-run organization (R-7, story task #3).** After 1f, Layer 7 generates a subdomain (`e2e-<16 lowercase hex>`, no inputs), provisions ONE fresh organization via `infra/kind/scripts/setup.sh --provision-org <sub>` (bounded by `DEVLOOP_ORG_PROVISION_TIMEOUT`, default 120s), then verifies AC resolves it (AC `/health` bounded by `DEVLOOP_HEALTH_BUDGET`, default 300s; the org-resolution probe itself bounded by `DEVLOOP_ORG_PROBE_TIMEOUT`, default 10s), and exports the subdomain to BOTH Phase-2 suites (`ENV_TEST_ORG_SUBDOMAIN` + `E2E_ORG_SUBDOMAIN`). **Why it exists:** no production code marks a meeting ROW ended (MC's meeting-end notify, story 2 task 12, ends only GC's MC-assignment row, never `meetings.status`, which is what the cap counts), so an org's live-meeting count only climbs toward `max_concurrent_meetings` — the browser suite creates ~7 meetings/run against the shared `demo` org's cap of 10, so run 2 against the same cluster used to fail with a 403 the pipeline attributed to the diff. A per-run org makes run N's verdict independent of runs 1..N−1 for everything keyed by `org_id`. (NOT for anything keyed above it: AC's registration limiter counts `auth_events` by IP, org-independent — same mechanism one level up, unchanged by this step.)

**Where to look, and what to grep.** Everything Phase 1h emits goes to **stderr**, captured at `${DEVLOOP_TMP:-/tmp/devloop}/layer-7.stderr.log` — layer7's *stdout* is the `STATUS=` parse channel, so `setup.sh`'s own `log_*` output is captured and relayed to stderr rather than allowed to reach it. Three greppable anchors, in emission order:

| Grep | Emitted | Carries |
|---|---|---|
| `Layer7: provisioning per-run organization` | **unconditionally, BEFORE the provisioning call** — so it survives a hang, a timeout and every later failure note in the same stream | the subdomain + cluster name the run used |
| `PROVISIONED_ORG org_id=<uuid> subdomain=<sub>` | relayed from `setup.sh` on success (machine-readable; `layer7.sh` positive-matches this prefix, fail-closed) | the `org_id` to inspect in the database |
| `LAYER=7 STEP=provision-org DURATION=` / `STEP=org-verify DURATION=` | after each half | **two timers, deliberately.** Verification waits up to the health budget on AC `/health`, so one timer spanning both would bill a cold AC to *provisioning* and send the reader to `postgres-0` — the attribution error the lane tokens exist to avoid |

**Lane discipline:** every Phase-1h failure is `PRECONDITION_FAILURE` exit 2 (OPERATOR lane) and there is deliberately NO fallback to the static `devtest`/`demo` orgs — a fallback would report R-7 fixed while the cluster kept accumulating meetings, i.e. a false green. The six tokens are in the table above; the load-bearing split is that **only a 404 from the AC probe evidences a provisioning fault** (`org-provision-unverified`), while every other non-401 status — including no response at all — routes to `ac-unreachable`, whose remediation does not claim "re-running will not change it". The one exception is a **2xx**, which gets its own lane (`ac-auth-bypass-signature`): AC answering successfully for an account that cannot exist falsifies `ac-unreachable`'s own "could not be asked" claim, and is the single case in Phase 1h where re-running is the wrong action. In general, a lane is correct only when its token **and** its cause line are both true for the failure; a correct remediation under a false cause line is still a misrouted lane.

The `PRECONDITION_FAILURE` enum (exit 2) is the operator lane — it surfaces in the `LAYER_SUMMARY` cell and reaches `LAYER_ALL_EXIT=2` (the per-layer exit code is propagated by `layer-all.sh`, not collapsed to 1). It is a first-class STATUS enum in `_common.sh` (rank between `FAIL` and `FAIL-MISSING-VERB`: an infra precondition dominates a sibling test FAIL, but a missing-wrapper wiring fault outranks it). A stderr `PRECONDITION_FAILURE: <cause>` banner with the fix accompanies every exit-2 lane. Per-step timing rides `LAYER=7 STEP=<name> DURATION=<secs>` stderr lines. ADR-0030 (host-side cluster helper) is the canonical lifecycle contract.

**Backstop scope — env-tests are a LOCAL-host-only gate, NOT part of CI's non-bypassable Gate-2 backstop (security).** CI (`ci.yml` running `layer-all.sh`) has no Kind cluster and no ADR-0030 helper, so Layer 7 self-reports `SKIPPED-NO-CLUSTER` (exit 0) there — env-tests do NOT run in NORMAL CI (which provisions no cluster). (Should a future CI provision a helper+cluster — a deliberate infra change, since the gate is socket-present-FIRST per §C.2 — Layer 7 would run env-tests there, and they would then fall under CI's enforcement. The posture below is therefore configuration-dependent, not a permanent invariant.) Therefore, in the current config, env-test coverage is provided ONLY by the local cluster-equipped devloop; CI's "independent re-run is the only non-bypassable enforcement" (§ci.yml authority note) does NOT cover env-tests. Do not "fix" a CI `SKIPPED-NO-CLUSTER` by trusting the local verdict — that is exactly the authority skip-vector the Gate-2 verdict-binding guards against; the local live run IS the env-test gate, and its raw evidence is what Gate 2 must see.

---

## 7. Per-Language Wrapper Triage (Cross-Cutting)

### Verb-discovery outcomes — `N/A` (intentional gap), `FAIL-MISSING-VERB` (wiring fault), `SKIPPED-NO-VERB` (filtered)

When `lang/<X>/<verb>.sh` is absent, the outcome is one of three DISTINCT enums since task
#52 (2026-06-19) — the enum (not a REASON infix) carries the meaning and the exit code (see
§3 table + ADR-0033 §6 amendment):

1. **Intentional gap → `N/A`, exit 0** (most common): the lang ships a one-line PLACEHOLDER
   `<verb>.sh` emitting `STATUS=N/A REASON=not-applicable-to-this-lang` (e.g. `proto/test.sh`,
   `proto/audit.sh` — proto has no real test/audit phase; `breaking.sh` is proto's audit gate).
   **Informative, not a failure** — N/A ranks just below OK, so the layer reads OK whenever a
   sibling lang did real work and passed, and reads `N/A` only when nothing real ran (e.g. Layer 6
   on a no-dependency-change run). A new placeholder must also be added to the N/A emitter
   allow-list in `scripts/lang/_dispatch.test.sh`.
2. **Missing verb wrapper → `FAIL-MISSING-VERB`, exit 2** (a bug, NOT self-justifying): a verb
   wrapper that SHOULD exist is genuinely missing or not executable (deleted, `chmod`-stripped,
   or a new lang dir added without the verb — and NO placeholder). REASON =
   `<lang>-<verb>-verb-missing-or-not-executable`. The layer **reds** (exit 2, the wiring-fault
   class with UNKNOWN) — and because `FAIL-MISSING-VERB` outranks OK, it reds even when a sibling
   lang's wrapper passed (cross-lang-masking closed, task #52). **Action**: restore the wrapper
   and `chmod +x` it — do NOT rationalize this as a deliberate skip. If the gap is genuinely
   intentional, register a placeholder `<verb>.sh` emitting `N/A` (see ADR-0033 §6).
3. **All langs filtered → `SKIPPED-NO-VERB`, exit 0**: `DEVLOOP_DISPATCH_INCLUDE_LANGS` /
   `EXCLUDE_LANGS` cleared the whole set (operator intent). REASON = `all-langs-filtered`. This is
   now the ONLY producer of `SKIPPED-NO-VERB`.
4. **No language directory at all → `FAIL-MISSING-VERB REASON=no-languages-registered`, exit 2**
   (since 2026-10-06): the lang root itself is empty or misdirected (broken checkout, bad
   `DEVLOOP_LANG_ROOT`) — checked BEFORE any filter, so it is never operator intent. No gate ran.

### `STATUS=SKIPPED-NO-DIFF` — diagnosing the audit dep-gate

Since 2026-08-20 the **only** producer of `SKIPPED-NO-DIFF` is the Layer-6 audit dep-manifest gate (`lang/{rust,ts}/audit.sh` → `no-dep-changes`, and the dispatcher's aggregate `all-langs-skipped`). The language-level `<lang>-no-diff` short-circuit was retired with the `changed.sh` classifier (§2/§3 amendments), so a language layer never emits this any more. Two-step triage for a `no-dep-changes`:

1. **Read the `BASE_REF=` line** in the same layer's stderr log. Was the diff what you expected?
2. **Inspect the cache**: `cat "${DEVLOOP_TMP:-/tmp/devloop}/changed-files.layer-<n>"`. Is the dependency manifest you cared about (`Cargo.toml`/`Cargo.lock`/`crates/*/Cargo.toml`; `package.json`/`pnpm-lock.yaml`/`pnpm-workspace.yaml`/`packages/*/package.json`) listed? If it is and the gate still skipped, that is a `_audit_gate.sh:diff_touches_glob` bug (§6.6 audit gate). If it isn't, the skip is correct — a diff touching no dependency manifest cannot move the resolved dependency graph.

### `STATUS=N/A` — documented gap vs. wrapper bug

`N/A` has exactly two documented emitters: the intentional-gap placeholders `lang/proto/test.sh`
and `lang/proto/audit.sh` (plus the dispatcher's `<verb>-aggregate-na`, which only passes their N/A
through). `no-languages-registered` is no longer N/A — it is `FAIL-MISSING-VERB` (§7 item 4).
(Layer 7's "no cluster in CI" state is NOT N/A either — it is its own enum `SKIPPED-NO-CLUSTER`
(exit 0, CI only); a local no/dead helper is `PRECONDITION_FAILURE` (exit 2). See §6.7.)

**How an unexpected N/A is caught now.** Since 2026-10-06 N/A ranks below OK, so an unexpected
`N/A` (a wrapper that should have run) beside a sibling's OK does NOT show in the layer RESULT —
only in the child `STATUS=` lines. The property that catches it is the **N/A emitter allow-list**
in `scripts/lang/_dispatch.test.sh` (Layer 3): any `N/A` emission in `scripts/` outside the two
placeholders fails `REASON=…` `[n/a-unexpected-emitter]`, and a scan that finds nothing fails
`[n/a-allowlist-scan-vacuous]`. A red allow-list check is a wiring fault — escalate, don't extend
the list to make it pass unless the gap is genuinely intended.

### `_changed_helpers.sh` debugging

The surviving diff-aware consumer — the Layer-6 audit dep-manifest gate (`_audit_gate.sh`; Layer 7 inspects no diff since ADR-0038 step 3) — composes these helpers (`scripts/lang/_changed_helpers.sh`); its self-test is `_changed_helpers.test.sh` (Layer 3):
- **`diff_touches_path <prefix>`** (lines 52-55): awk + fixed-string `index($0, p) == 1`. Matches files whose path *starts with* `<prefix>`. Fixed-string by design — a future `c++` or `c#` path would silently regex-match wrong files under naive `grep "^prefix"`.
- **`diff_touches_glob <glob>`** / **`diff_touches_root_files <file…>`**: anchored glob / `grep -qxF` (fixed-string, exact-line) — the audit dep-manifest gate's predicates.

When a consumer misfires:
1. Inspect the consumer (`_audit_gate.sh`, the audit gate).
2. Inspect `_changed_helpers.sh` to confirm helper semantics; re-populate the cache with `DEVLOOP_LAYER=manual bash scripts/lang/_get_base_ref.sh` and re-check against it.
3. If the consumer reads the wrong cache, `DEVLOOP_LAYER` is not exported — a layer-script bug (the layer-skeleton ought to export `DEVLOOP_LAYER` via `_common.sh::layer_lifecycle_begin`).

---

## 8. Symptom → Resolution Catalogue (Cross-Reference Index)

Grep-driven entry point. Match the symptom, jump to the section.

| Symptom (greppable) | Likely cause | Jump |
|--------------------|--------------|------|
| `FAILED: validate-frame-vectors` | The frame-v2 cross-language vectors disagree with the Rust codec, the vendored external anchor, or their own internal relations. `VIOLATION:` names the check id (`g1`..`g16`), the row and the field. | §6.3 |
| `VIOLATION [internal-proto-bytes-field]` / `[internal-proto-cross-package-type]` / `[internal-proto-import-not-allowed]` | **CONTENT — a diff defect.** `internal.proto` gained a way to carry key material across MC→MH: a `bytes` field, a non-allowlisted or non-canonically-spelled cross-package type, or a disallowed import. Do not delete the field to go green and do not edit the guard; re-derive the rule with @security. | §6.3 |
| `VIOLATION [internal-proto-scan-vacuous]` / `[internal-proto-allowlist-not-enum]` | **COULD NOT EVALUATE — the guard refused to report clean.** A proto is missing or empty, the identity markers are gone, an allowlist entry is absent or declared as a message, a `/*` block comment appeared, or a grep failed. Restore the input; never loosen a pattern. | §6.3 |
| `ERROR: PRECONDITION [<token>]` from `validate-frame-vectors` | A guard input is missing or a declaration moved. **Two sub-classes with opposite first actions** — an ordinary edit whose message is the fix, versus the guard's own `sed`/`jq` extraction having broken. Read the token before touching anything. | §6.3 |
| `WARN frame-vectors: cross-language property NOT established` | **A FAULT since story task 15 — investigate, do not ignore.** Both codecs are gated in tree, so this banner means a codec has been UN-gated: `gated_by.<codec>` flipped back to `false`, or a third codec was added to g14's `declared_codecs` without its conformance marker. Until task 15 it was the expected steady state, which is why the wording is inverted here rather than deleted — a triager who finds the old text ignores a real regression. Non-suppressible by design. | §6.3 |
| `NOTE dt-guard allowlisted mention` | Expected steady state — an allowlist hit is the allowlist working. Emitted below `WARN ` on purpose so it does not bury real coverage holes. | §6.3 |
| `warn: the missing active toolchain` … `has been auto-installed` on the first cargo call; or, offline, a rustup download error naming that toolchain (`could not download file … channel-rust-1.X.Y.toml`) | **The devloop image predates a `rust-toolchain.toml` bump.** rustup selects the pinned toolchain from the file. When the image does not carry it, rustup auto-installs it into the container's `/usr/local/rustup` and cargo proceeds; that works, but costs a download per container. Offline, it fails loudly. **Fix (host): `./infra/devloop/devloop.sh --rebuild --recreate`**: the image's `FROM rust:<v>` is derived from the same file (`infra/lib/rust-toolchain.sh`). A separate `rustup is not installed at '/tmp/cargo-home'` after an explicit `rustup toolchain install` in the container is cosmetic: the toolchain installed, and only rustup's self-check missed its binary under the redirected `CARGO_HOME`. | §6.1 |
| `RESULT=FAIL` on a layer; first hit | Read the per-layer subsection | §6 |
| `REASON=cargo-nextest-missing` / `CARGO_NEXTEST_VERSION_MISMATCH:` | The devloop image predates `infra/cargo-tools.versions` (or a pin change). **Host: `infra/devloop/devloop.sh --rebuild`.** Environment, operator lane. | §6.4 + §8.7 |
| red `Test Suite` in CI; `REASON=aggregate-…` or `FAILURE_TRIAGE LAYER=<n> SHARD=<shard>` | The CI pipeline is sharded; the aggregate names the layer and the shard job to open. | §8.7 |
| `ld terminated with signal 6 [Aborted], core dumped` / `collect2: fatal error:` — arrives as `REASON=cargo-nextest-failed` | **The linker was killed under memory pressure. NOT a test failure, and the diff is not implicated** — do not start by reading test output, which is what the bare `cargo-nextest-failed` row tells you to do. The tell is that several test binaries fail to *link* at once, **including ones the diff does not touch**; a genuine link error is specific to what changed. Check free memory, re-run Layer 4 in isolation to confirm. Operator lane, consumes no attempt. | §6.4 |
| `FAILED: media-telemetry-deny` | A telemetry macro form, or a `tracing`/`log`/`metrics` import, appears under `crates/mh-service/src/media/` (ADR-0036 §11). `VIOLATION:` names path:line and the macro SPELLING — never its arguments. Fix by using a cached metric handle. No bypass marker exists. | §6.3 |
| `media-telemetry-deny-scope-directory-missing` | **A DIFF DEFECT, not a machine fault — do NOT re-run on a quiet machine.** The configured media directory does not exist: you renamed or moved it and did not update `scripts/guards/simple/media-telemetry-deny.yaml`. Restore the directory, or update the manifest. | §6.3 |
| `media-telemetry-deny-scope-directory-empty` | **A DIFF DEFECT, like `-missing` — the two differ in WHAT was edited, not in who caused it.** The configured directory still exists but holds zero `.rs` files: its contents were moved or renamed out from under it. Restore them, or update the manifest to where they went. Suspect the guard's own walk (`common/scope.rs`) only if the media tree looks intact. | §6.3 |
| `media-telemetry-deny-scope-directory-escapes-root` | A configured entry resolves outside the repository (traversal, absolute path, or a symlink out of the tree). **Do not apply the `-missing` remedy**: re-pointing the manifest at the symlink's target completes the evasion. | §6.3 |
| `media-telemetry-deny-scope-no-directories-configured` / `-manifest-missing` / `-manifest-unparseable` | The guard could not read its own scope at all. Fail-closed by design — it never defaults to an empty list and reports OK. | §6.3 |
| `media-telemetry-deny-unparseable-use` | A `use` declaration under the scope could not be classified. Reported rather than passed: an unanticipated shape passing silently is the coverage illusion the guard exists to prevent. Rewrite it as a plain `use <path>;`. | §6.3 |
| `ERROR: PRECONDITION [media-telemetry-deny-…]` | **This prefix INVERTS its usual meaning here.** For `release-build-profile-*` / `env-config-*` it usually means the environment; for `media-telemetry-deny-*` it always means a diff defect. For the scope and manifest tokens the body says so explicitly (`THIS IS A DIFF DEFECT, not a machine fault`) and the fix is one line. **`-unparseable-use` carries the same prefix with NO such body** and is not a one-line fix — see its own row above. | §6.3 |
| `ts-no-retained-credentials::` (either rule id) | A retained client type carries a credential field. No bypass marker exists; fix the type or revert the guard commit. | §6.3 |
| `PRECONDITION_FAILURE:` at startup | CI shallow clone (or other layer-all precondition) | §4 + §5 |
| Uncommitted work vanished — **your own**, or another owner's — after a temporary edit was undone | `git checkout <path>` (with or without `--`) reverts to HEAD, not to the state before your edit, and cannot see whose uncommitted delta it is destroying. **Most often hit while mutation-testing your own file**, not only while staging someone else's change. Snapshot-and-restore instead — `cp` before, `mv` back. | §2.1 |
| `ERROR:` in a layer stderr log | `_get_base_ref.sh` resolver failure | §4 + §5 |
| `STATUS=N/A REASON=not-applicable-to-this-lang` (proto, Layer 4/6) | Expected — proto's intentional-gap placeholder `test.sh`/`audit.sh` (exit 0). N/A ranks below OK, so the layer reads OK when rust/ts passed; Layer 6 reads N/A on a no-dependency-change run (no audit ran). | §6.4 + §6.6 + §7 |
| `TOTAL_RESULT=N/A` (top line) | No layer did real work anywhere — not a green run. Check which layers ran and why each was a placeholder/skip. | §3 |
| `STATUS=FAIL-MISSING-VERB REASON=no-languages-registered` | The dispatcher found no language directory at all — broken checkout or misdirected `DEVLOOP_LANG_ROOT`. Exit 2; no gate ran. | §7 |
| `STATUS=FAIL-MISSING-VERB REASON=…-verb-missing-or-not-executable` | A verb wrapper that should exist is missing/`chmod`-stripped — **reds the layer (exit 2)**, and (task #52) reds even when a sibling lang passed. Restore the wrapper + `chmod +x`, or register a placeholder `<verb>.sh` emitting `N/A` if the gap is intended. | §7 |
| `REASON=wrapper-aborted-early-exit-<rc>` | A verb wrapper crashed BEFORE emitting STATUS (e.g. `set -e` abort, `exit 1` in a helper); previously surfaced as a silent `UNKNOWN`. The EXIT trap now emits FAIL with the abort code `<rc>`. | §6.4 + §7 |
| `STATUS=SKIPPED-NO-CLUSTER REASON=no-cluster-ci` | Expected — CI (`GITHUB_ACTIONS`) has no Kind cluster. Clean skip, exit 0, never reds CI. The ONLY skip case (local runs never skip). | §6.7 |
| `STATUS=PRECONDITION_FAILURE REASON=local-helper-not-running` / `helper-unreachable` / `cluster-*` (incl. `cluster-provision-failed` / `cluster-deploy-failed` = provision / deploy failed on the ENVIRONMENT; on the tree it is `STATUS=FAIL REASON=provision-failed` / `deploy-failed`) / `ports-json-missing` | Layer 7 env-gate / Phase-1 infra failure — **OPERATOR lane (exit 2)**, not a test regression; does not consume an implementer attempt. `local-helper-not-running` = local devloop, no helper (loud, never a silent skip); `helper-unreachable` = socket present but the helper crashed (or, mid-self-heal, the probe couldn't reach it). | §6.7 |
| `SELF_HEAL CASE=<apiserver-unreachable\|kubeconfig-stale\|rollout-wedged\|probe-inconclusive\|helper-unreachable> ACTION=<recreate\|restore-kubeconfig\|none>` (stderr, in `${DEVLOOP_TMP:-/tmp/devloop}/layer-7.stderr.log`) | Layer-7 Phase-1e self-heal classified a `cluster-unhealthy` cause and took/declined exactly one fix. Emitted once the bounded probe returns a verdict, on EVERY reached path (incl. no-action). **ABSENCE on a `cluster-unhealthy` log = self-heal never ran (a regression)** — the greppable positive control. `helper-unreachable` = the probe's own `dev-cluster status` errored/timed out (rc≠0/timeout ⇒ couldn't-ask; rc==0-unparseable ⇒ `probe-inconclusive`). Grep the whole story with `grep 'SELF_HEAL '` — the three anchors (`CASE=`, `RESULT=`, `HELPER_OUTCOME=`) share that one prefix by design. `CASE=` values and pipeline `REASON=` tokens overlap on purpose (`helper-unreachable` is both) — see §3 / §6.7, do not merge them. | §6.7 |
| `SELF_HEAL RESULT=<recovered\|failed> ACTION=… ATTEMPT=<n>/<max> EVIDENCE=<path\|capture-failed:<reason>\|none> [DETAIL=…]` (stderr, same `layer-7.stderr.log`) | Outcome of the one self-heal fix after re-verify. `recovered` = healthy, suite proceeds (a bare `cluster-unhealthy` NEVER follows it — invariant). **REASON mapping:** `failed` → `cluster-self-heal-failed`, **EXCEPT the two refused values (`DETAIL=recreate-refused-cluster-alive` / `recreate-refused-inconclusive`) → `cluster-self-heal-probe-inconclusive`** (a declined destroy is fail-closed, not a failed fix). **Grep `DETAIL=` (NOT `REASON=`)** for why: `recreate-failed`/`restore-failed` (ran, still unhealthy), `recreate-refused-cluster-alive` (the helper's host-side probe disagreed WITH ITSELF — unreachable at classify, `Reachable` at the destroy gate; a flap/race, cluster likely fine → re-run `dev-cluster status`; NOT a kubeconfig problem), `recreate-refused-inconclusive` (the host could NOT determine the state — container runtime down, or `inspect` errored/timed out; check the podman/docker daemon + `helper.log`, NOT the kubeconfig), `recreate-already-attempted-this-helper-lifetime` (destroy bound already spent; an **in-helper-process flag**, NOT a file — clears only when the helper process restarts, so a crashed run never poisons the next), `recreate-unrecognized-outcome` (a **DRIFT DETECTOR, not an environment condition** — the shell received a `HELPER_OUTCOME=` value it has no case arm for, i.e. the Rust `SelfHealOutcome` enum gained a variant the shell + this row never learned; it routes to `cluster-self-heal-failed` but the fault is a **code defect to FIX** — add the missing shell arm + update this row — NOT a host-disk/retry situation; symmetric with `capture-failed:unrecognized-reason`). `EVIDENCE=` = host-side pre-destroy bundle, the shell-composed **container** path (on the host, under `/tmp/devloop-<slug>/`); contrast `HELPER_OUTCOME=`'s `EVIDENCE_LEAF=` below (bare leaf, pre-composition). **`EVIDENCE=none`** = no bundle was captured because none was ATTEMPTED — the `restore-kubeconfig`, refused, and already-attempted arms, where no destroy occurred and so there is no pre-destroy bundle to capture. It is **distinct from `capture-failed:<reason>`**, which means a capture WAS attempted (a destroy was happening) and failed — do not read `none` as "capture produced nothing" and go hunting a broken evidence path that never existed. `capture-failed:<reason>` is a closed enum (see the `HELPER_OUTCOME=` row). | §6.7 |
| `SELF_HEAL HELPER_OUTCOME=<recreated\|restored\|recreate-refused-cluster-alive\|recreate-refused-inconclusive\|recreate-already-attempted-this-helper-lifetime\|recreate-failed\|restore-failed> ATTEMPT=<n>/<max> EVIDENCE_LEAF=<leaf\|capture-failed:<reason>>` (stderr, same `layer-7.stderr.log`) | The `dev-cluster` client's rendering of the helper's structured `data.self_heal` — the **raw helper report, BEFORE the shell composed** the `RESULT=`/`CASE=` lines from it. Read it when you suspect the SHELL misread the helper (a refusal surfacing as a generic failure): `HELPER_OUTCOME=` is the host-side ground truth. Its five failure values are **byte-identical to their `DETAIL=` counterparts** by design, so `grep 'SELF_HEAL '` shows the helper's own report beside the shell's rendering with no near-miss; the two success values `recreated`/`restored` have no `DETAIL=` counterpart (a `RESULT=recovered` carries none). The two sets are **NOT equal size**: `DETAIL=` has a **sixth** value, `recreate-unrecognized-outcome`, that is **shell-only with no `HELPER_OUTCOME=` counterpart by construction** — the shell emits it precisely WHEN it cannot recognize the helper's `HELPER_OUTCOME=`. So `HELPER_OUTCOME=` is 7 (5 failures + `recreated`/`restored`) and `DETAIL=` is 6 (those 5 shared failures + the shell-only drift token). **`EVIDENCE_LEAF=` is a DISTINCT key from line 2's `EVIDENCE=`** — the helper reports the bare directory *leaf*, the shell prepends `${DEVLOOP_TMP}` to compose the container path; a leaf that is NOT the tail of the composed `EVIDENCE=` path means the shell's path composition broke, not the helper. `ATTEMPT=` is identical to line 2's on purpose — a divergence between the two adjacent lines is a real bug, not a duplication to reconcile. **`capture-failed:<reason>` closed 5-set** (shared with line 2's `EVIDENCE=`): `bundle-dir-unwritable`, `timeout-unavailable` (helper-emitted) / `no-evidence-line`, `malformed-leaf`, `unrecognized-reason` (shell-emitted). **`unrecognized-reason` is a DRIFT DETECTOR, not an environment condition** — the shell validates the helper's `<reason>` against this closed set and collapses anything else to `unrecognized-reason`, so its presence means the Rust `capture-failed` enum gained a variant the shell allow-list (and this row) never learned: a defect to FIX, not a state to interpret (fail-closed but lossy — it discards the new detail exactly when someone added it deliberately). `ANCHOR (DRY): capture-failed reasons live in three encodings that must move together — the Rust enum (`crates/devloop-helper/src/commands.rs`), the shell allow-list (`scripts/layer7.sh`), and this row; a 6th member is added to all three in one commit.` | §6.7 |
| `STATUS=PRECONDITION_FAILURE REASON=cluster-self-heal-failed` / `cluster-self-heal-probe-inconclusive` | Layer-7 Phase-1e self-heal escalation — **OPERATOR/HOST lane (exit 2)**, does not consume an implementer attempt. `cluster-self-heal-failed` = a bounded fix ran and the cluster is still unhealthy (do NOT re-run teardown + provision + deploy — the self-heal just tried it; read the `EVIDENCE=` bundle) — **EXCEPT when the `SELF_HEAL RESULT=failed` line carries `DETAIL=recreate-unrecognized-outcome`**: that is a DRIFT DETECTOR (the shell got a `HELPER_OUTCOME=` it has no arm for), a **code defect to fix** (add the shell arm), NOT a host-disk/resource problem — do not chase `podman image prune` for it. `cluster-self-heal-probe-inconclusive` = the probe couldn't classify, OR the host DECLINED a destroy — either its re-confirm disagreed with the classify observation (`DETAIL=recreate-refused-cluster-alive` — a flap/race, cluster likely fine, re-run `dev-cluster status`) or it could not determine the state at all (`DETAIL=recreate-refused-inconclusive` — container runtime down; check the podman/docker daemon + `helper.log`, NOT the kubeconfig). | §6.7 |
| `CLUSTER_NAME_TOO_LONG SUBJECT=<cluster-name\|slug> NAME=<name> LEN=<n> MAX=<49\|41>` | A cluster/slug name whose derived `<cluster>-control-plane` node name would exceed the 63-char DNS-label cap (cluster ≤ 49, devloop slug ≤ 41 — bounds DERIVED from 63, no literal). **Fail-loud, no truncation/hash** — truncation would collide two devloops onto one cluster (an isolation breach). `ERROR:`-prefixed on stderr; two emit sites: `devloop.sh` launch (`SUBJECT=slug MAX=41`, `infra/devloop/devloop.sh`) and `setup.sh` cluster-create as defense-in-depth (`SUBJECT=cluster-name MAX=49`, `infra/kind/scripts/lib/common.sh:validate_cluster_name`, run by every Kind script). `teardown.sh` deliberately does NOT reject — it must still clean up an already-created over-long cluster. Seeing `SUBJECT=cluster-name` from `setup.sh` rather than a `SUBJECT=slug` reject at `devloop.sh` launch means the launcher check was bypassed — itself a finding. | §6.7 |
| `STATUS=PRECONDITION_FAILURE REASON=org-provision-context-unresolved` / `org-provision-timeout` / `org-provision-failed` | Layer 7 Phase-1h — the per-run organization (R-7) could not be provisioned. **OPERATOR lane (exit 2)**; does not consume an implementer attempt. `context-unresolved` = cluster-ADDRESSING problem (`ports.json` `.cluster_name` / `kubectl`), **not** a database problem; `timeout` = `setup.sh --provision-org` exceeded `DEVLOOP_ORG_PROVISION_TIMEOUT` (120s); `failed` = non-zero exit, a missing `PROVISIONED_ORG ` line, or the subdomain generator's entropy postcondition. Diagnostics are the relayed `setup.sh` output in `${DEVLOOP_TMP:-/tmp/devloop}/layer-7.stderr.log`. | §6.7 |
| `STATUS=PRECONDITION_FAILURE REASON=org-provision-unverified` | Layer 7 Phase-1h — AC answered **404** for the newly provisioned org's Host. **OPERATOR lane (exit 2), and NOT a flake — re-running will not change it.** 404 is the only status that evidences a provisioning fault: `org_extraction` fell through `get_by_subdomain`'s `WHERE subdomain = $1 AND is_active = true`. Inspect the row; the subdomain is on the `Layer7: provisioning per-run organization` stderr line. | §6.7 |
| `STATUS=PRECONDITION_FAILURE REASON=ac-unreachable` | Layer 7 Phase-1h — AC could not be ASKED whether it resolves the per-run org (no `.container_urls.ac`, `/health` never 2xx, or the probe returned anything other than 401/404/2xx — including no response `000` or a 5xx). **OPERATOR lane (exit 2), explicitly NOT a provisioning fault** — the org may well be fine, so unlike `org-provision-unverified` **re-running is reasonable**. Most common real cause is `000`: the probe's own cap (`DEVLOOP_ORG_PROBE_TIMEOUT`, default 10s) elapsed against AC's unconditional bcrypt-cost-12 verify, which is CPU-bound — raise the knob, don't hunt the DB. | §6.7 |
| `STATUS=PRECONDITION_FAILURE REASON=ac-auth-bypass-signature` | Layer 7 Phase-1h — the org-resolution probe got a **2xx**: AC issued a SUCCESS for an account that cannot exist. **OPERATOR lane (exit 2) but a SECURITY finding** — the only Phase-1h lane where re-running is the wrong action, because a second green run buries it. Capture the response and AC's logs before redeploying or tearing down; escalate to auth-controller + security. Split out of `ac-unreachable`, whose "could not be asked" claim a prompt 2xx falsifies. | §6.7 |
| `STATUS=FAIL REASON=browser-e2e-failed` | Browser E2E spec regression (implementer lane; consumes the shared Layer-7 attempt). Log: `layer-7-browser-e2e.log`; Playwright traces in `packages/web-app/test-results/` (outside `DEVLOOP_TMP` cleanup, can contain live tokens — local-only). **Not always a spec regression:** if the log carries `Triage Prometheus/port-forward`, Prometheus died mid-suite — see that row. | §6.7 |
| `browser-e2e-not-run:` (stderr) | Env-tests failed first — the browser suite was deliberately skipped this attempt (shared single-attempt budget), NOT a browser problem. Fix the env-test failures; browser suite runs on the retry. | §6.7 |
| `Triage Prometheus/port-forward` (in `layer-7-env-test.log` **or** `layer-7-browser-e2e.log`) | Layer 7 reports `STATUS=FAIL REASON=env-tests-failed` **or** `browser-e2e-failed` (implementer lane, attempt consumed), but this is substantively an OPERATOR-lane fault: the counter-delta helpers in BOTH suites fail LOUD (Rust panic / TS throw) when a Prometheus query errors mid-suite — Phase 1f only proves Prometheus was ready *before* the suites, so a prometheus pod or port-forward that dies mid-run surfaces here. **Do NOT chase the diff.** Check the `prometheus` pod (`kubectl -n dark-tower get pods -l app=prometheus`) and the port-forward behind `ENV_TEST_PROMETHEUS_URL` / `E2E_PROMETHEUS_URL` (both default `localhost:9090`), then re-run. Deliberately not auto-classified — §6.7 Phase 2 does not grep suite output (grepping it would reopen reverse-masking). | §6.7 |
| `WARN LOKI_NOT_READY_AFTER=<n>s` (stderr, Phase 1f) | Loki's `/ready` did not go 2xx within the observability-readiness budget, so Layer 7 **proceeds anyway** (Loki is a SOFT probe — the crate treats it as optional). Not a failure of its own and NOT your diff: it **pre-attributes** the `test_all_services_have_logs_in_loki` failure that WILL then surface in Phase 2 to a cold/absent observability stack. A cold-but-coming Loki is meant to be caught here as a Phase-1 wait; seeing this WARN means the wait budget expired first. Check the `loki` pod (`kubectl -n dark-tower get pods -l app=loki`) and its logs; re-running once Loki is warm clears it. Contrast the Prometheus probe (HARD → `observability-prometheus-not-ready`), which the metrics tests cannot proceed without. | §6.7 |
| `Triage MH scrape/metric-registration` (in `layer-7-env-test.log`) | Layer 7 reports `STATUS=FAIL REASON=env-tests-failed` (implementer lane, attempt consumed), but this is substantively an OPERATOR-lane fault, keyed on the literal emitted by `crates/env-tests/tests/32_media_metric_hygiene.rs` (constant `TRIAGE_SCRAPE`). `mh_media_frames_forwarded_total` is absent from Prometheus. **MH resolves the full media metric label cross-product unconditionally at startup**, so those series exist at zero on any running MH pod with no media flowing — absence means MH is not up, not scraped, or its metric registration path broke. Check the `mh-service` pods and the Prometheus scrape target. **Do NOT satisfy the check by deleting it**: the anchor is what stops every absence assertion in that suite from passing vacuously. **MC is deliberately NOT under this presence check** — MC's media metrics are lazily created on first emission, so their absence on an idle cluster is the expected state; an MC-shaped presence gate would red on every idle run and be muted within weeks. Do not "fix" that asymmetry into symmetry. | §6.7 |
| `Triage metric-label policy violation` (in `layer-7-env-test.log`) | **IMPLEMENTER lane and a genuine policy violation** — keyed on `TRIAGE_VIOLATION` in `32_media_metric_hygiene.rs`. A series scraped from `ac-service`, `gc-service`, `mc-service`, `mh-service` or `otel-collector` (the browser `dt_client_*` series, since R-27) carries a `meeting_id*` label (raw or hashed), or an `e2ee` / `end_to_end` / `zero_trust` label key or value. Barred by ADR-0036 §11 and `docs/observability/label-taxonomy.md` §Media-path identity R1 and §Key custody. **The assertion has no carve-outs by design, so there is no legitimate series it can red on. Fix the METRIC, never the assertion**: remove the label from the emitting `metrics.rs`, or change its value. **Two named non-fixes.** (1) Do NOT add a grandfather allowlist — §11's grandfathered set is the client SDK's ADR-0028 join-flow metrics, and the collector's metric-NAME allowlist (`infra/services/otel-collector/collector.yaml`) deliberately does not export any of them, so over these five jobs the exception has **zero members**; an allowlist would be dead on the day it was written and would swallow the first real violation after it. A `dt_client_*` series carrying `meeting_id*` means that name allowlist or the collector's `keep_keys` was widened — **fix it there**. (2) Do NOT narrow the job selector — `otel-collector` is in it on purpose (without it the client series are invisible to this rule and it passes having examined nothing); if a carve-out seems necessary, the real diagnosis is a widened collector allowlist, not the selector. The same suite's `up{job="otel-collector"}` positive control reports through `Triage MH scrape/metric-registration` when the collector target is down — an environment fault, not a violation. | §6.7 |
| `Triage Prometheus relabel configuration` (in `layer-7-env-test.log`) | Keyed on `TRIAGE_RELABEL` in `32_media_metric_hygiene.rs`. Prometheus now applies `metric_relabel_configs`. That suite evaluates **stored** series, so a drop/replace applied before storage makes it blind to a label the pod still emits — it would keep passing while covering less, which is the silent-narrowing failure ADR-0036 §11 is about. Read the labels **at the pod** for the affected jobs. Do NOT widen the suite's selector and do NOT delete the assertion. (Scoped to `metric_relabel_configs` only, deliberately: the service jobs' `relabel_configs` are `keep`-only target *discovery*, and a `relabel_configs` addition would surface as a new label the suite catches.) | §6.7 |
| `Triage MH handler-set health` (in `layer-7-env-test.log`) | Layer 7 reports `STATUS=FAIL REASON=env-tests-failed` (implementer lane, attempt consumed), but this is substantively an **OPERATOR-lane** fault, keyed on `TRIAGE_HANDLER_SET` (defined in `crates/env-tests/src/fixtures/participant.rs`, emitted by `crates/env-tests/tests/27_mc_slot_placement.rs`). That suite needs each meeting's handler SET to contain BOTH Kind handlers (every participant is offered the whole set; the mock clients then produce partial connectivity by which handlers they open sessions to), and GC returns a two-handler set only when both MH instances are healthy and registered — so a JoinResponse offering fewer than two handlers means the environment, not MC. **Do NOT chase the diff, and do NOT relax the assertion to a single-handler variant** — a suite that adapts to one handler stops testing the shared-handler model it exists for, and would then pass for the rest of the meeting's life on half a cluster. Check `kubectl -n dark-tower get pods -l app=mh-service` (both `mh-0` and `mh-1` Ready), then re-run. The suite hard-fails here by design rather than skipping: a skip on a half-up cluster is the silent-coverage-loss shape §6.7 keeps out of Phase 2. | §6.7 |
| `Triage MH reachability` (in `layer-7-env-test.log`) | Same lane split as the row above, keyed on `TRIAGE_MH_REACH` (defined in `crates/env-tests/src/fixtures/participant.rs`, emitted by `crates/env-tests/tests/27_mc_slot_placement.rs`): a mock client could not open a media session to a handler it was TOLD to use. **An unreachable handler produces exactly the partial-connectivity shape the canonical phase asserts**, so the suite refuses to degrade into it and fails as ENVIRONMENT instead. Check the MH NodePorts (4434/4436), `infra/services/mh-service/network-policy.yaml`, the handler certificate and pod health, then re-run. **Do NOT relax it into a warning** — an unreachable-handler run that passed would be testing the environment's breakage, not MC. | §6.7 |
| `settle-not-above-scrape-interval` (in `layer-7-env-test.log`) | An env-test stability wait refused to run: `SERVICE_JOB_SCRAPE_SETTLE` (`crates/env-tests/src/fixtures/metrics.rs`) does not exceed the LIVE per-job `scrape_interval` of the ac/gc/mc/mh jobs, read from Prometheus' `/api/v1/status/config`. Almost always a cluster running an older Prometheus config than the tree — apply it: `kubectl apply -k infra/kubernetes/overlays/kind/observability/` (Layer 7's Phase 1e2 does this when the diff touches that config). If the interval was raised on purpose, raise `SERVICE_JOB_SCRAPE_INTERVAL_SECS` with it. **NEVER shorten the settle to make this pass** — a settle inside one scrape makes every stability wait compare two reads of the same scrape and pass having observed nothing. | §6.7 |
| `scrape-config-unreadable` (in `layer-7-env-test.log`) | The same precondition could not read or parse the live Prometheus config (unreachable, not the expected JSON envelope, a service job missing, or an unparseable interval). An environment/config fault, deliberately distinct from the row above. **Do not relax the extractor** to make it pass: a parse that finds nothing is exactly the vacuous case the precondition exists to refuse. | §6.7 |
| `STATUS=PRECONDITION_FAILURE REASON=dev-certs-missing` / `playwright-browser-missing` | Layer 7 Phase-1g browser-suite precondition — **OPERATOR lane (exit 2)**. Missing WebTransport cert fingerprints (`scripts/generate-dev-certs.sh`) or missing Playwright Chromium (`pnpm exec playwright install chromium`). | §6.7 |
| `AlreadyExists` on a Calico resource (or any `create`) during a build | Historical symptom of REUSING a half-built cluster, which ADR-0038 step 3 retired: provision records its blueprint only after a successful build, so a half-built cluster reads `BLUEPRINT … REASON=missing` and is deleted and rebuilt, never reused. If you still see it, the provision run's `BLUEPRINT` line will say why — and **diagnose the ORIGINAL abort**, further up the log. | §6.7 |
| Layer 1 runs `cargo check`/`tsc` on a docs-only PR | Expected since 2026-08-20 — every language layer always-runs (ADR-0033 §3); `cargo check` is cheap. (Formerly framed as `changed.sh` over-classification; there is no longer any per-lang classification.) | §6.1 |
| Layer 1 fails locally with `nx: command not found` | Local-only failure — CI has corepack/pnpm install in setup. | §6.1 (run `pnpm install`) |
| Layer 4 fails with `Neither podman nor docker found` | Missing container runtime — `lang/rust/test.sh` bring-up. | §6.4 |
| Layer 6 fails on every CI run, local clean | Likely CI base-ref shift (post-#42) — check `BASE_SOURCE=ci-pr` `BASE_REF=` is merge-base, not main tip. | §5 (CI-PR scope shift) |
| Layer 6 `cargo-audit-failed` / `pnpm-audit-failed` from a transitive dep | Triage: fix vs suppress-in-`audit-suppressions.toml` (security-owned, ADR-0033 §11); never CLI flag / hand-edit / Dependabot dismissal. | §6.6 |
| Dependabot cargo PR red on Layer 3 `excluded-workspace-version-drift` | **Expected.** The PR moved a requirement the fuzz crates repeat (they are independent workspaces and cannot inherit). Edit the fuzz `Cargo.toml` literal to match the root in the **same PR**; never split it. A fuzz-block PR (e.g. `libfuzzer-sys`) red on the same token needs the root `[workspace.dependencies]` entry edited instead. | §6.3 |
| `env-config-{no-workload-manifest,service-inputs-missing,unclassified-manifest-kind,duplicate-configmap-name,workload-missing-pod-spec,unsupported-env-source,malformed-env-reference}-<n>-of-<m>-findings` <br>or `env-config-no-services-checked` | **The guard refused to claim coverage** — it could not check something it is responsible for, which outranks any policy finding and always names the run. `no-workload-manifest` = a service dir has a `config.rs` but no `Deployment`/`StatefulSet`/`DaemonSet` document (check the dir's file layout, not the env wiring). `service-inputs-missing` = `crates/<svc>/src/config.rs` or `infra/services/<svc>/` is gone. `unclassified-manifest-kind` = a manifest `kind` is in neither kind list, or a document has no `kind`; **if it carries a pod template add it to `WORKLOAD_KINDS_WITH_POD_SPEC` and implement its pod-spec path — do NOT silence it by adding it to `NO_POD_SPEC_KINDS`**, which reopens the coverage hole. `workload-missing-pod-spec` = a workload with no `spec.template.spec`. `unsupported-env-source` = a container uses `envFrom`, which the guard cannot reason about (per-key checks are then suppressed only for the ConfigMap that envFrom names, or service-wide if its name is unreadable). `duplicate-configmap-name` = two ConfigMap documents in one service dir share a `metadata.name`; a name-scoped resolver cannot tell them apart (also a kustomize build error). `malformed-env-reference` = an `env` entry with no `name`, or a `configMapKeyRef`/`envFrom configMapRef` missing its `name`/`key` — surfaced rather than dropped, because a dropped ref would misattribute its key as an orphan. **For both `unsupported-env-source` and `malformed-env-reference`: the absence-based checks (1 = required var present, 3 = ConfigMap key referenced) are suppressed for the affected workload/ConfigMap, since a bulk or malformed import could supply what looks missing. Expect `missing-in-manifest` / `orphan-key` findings to appear on the *next* run once the envFrom or malformed ref is fixed — they are not new, they were unprovable while the guard could not see the injection.** The `-<n>-of-<m>-findings` suffix is deliberate: `<n>` counts THIS kind, `<m>` the total across all kinds — never read `<m>` as a count of the named kind. All findings print regardless of which named the run. <br>**`env-config-no-services-checked` is the limiting case and carries NO `-<n>-of-<m>-findings` suffix** — it is a bare `bail!`, not a findings token, because there are no findings: `CANONICAL_SERVICES` resolved empty and the guard verified nothing. It is a fail-closed sentinel against a future refactor emptying or gating the service list, **not** operator drift — you cannot cause it by editing manifests, so do not go looking in `infra/services/`. If it fires, something changed in `crates/dt-guard/src/common/services.rs` or the service-iteration path. Reaching it means the guard would otherwise have reported OK having checked nothing, which is the precise defect this guard was rewritten to make impossible. | §6.3 |
| `env-config-missing-in-manifest-<n>-of-<m>-findings` | `config.rs` has a `MissingEnvVar("VAR")` that a workload does not declare. Checked **per workload**, so the common shape is a var present in `mc-0`/`mh-0` but forgotten in `mc-1`/`mh-1` — that pod CrashLoops at startup while its sibling stays healthy (half capacity, no total outage). The VIOLATION line names the offending workload file. | §6.3 |
| `env-config-configmap-not-found-<n>-of-<m>-findings` | A `configMapKeyRef` names a ConfigMap with no manifest in that service directory. **Runtime-fatal** — kubelet reports `CreateContainerConfigError` and the pod never starts. Usually a typo'd `name:`, or a ConfigMap expected from an overlay (which this guard does not cover — see its module doc and `docs/TODO.md` §Infrastructure Validation in Devloops). | §6.3 |
| `env-config-key-not-in-configmap-<n>-of-<m>-findings` | The named ConfigMap exists but does not declare the requested key. Same runtime consequence as above. Go to that ConfigMap's `data:` block — note resolution is **name-scoped**, so a key present in a *sibling* ConfigMap does not satisfy the reference. | §6.3 |
| `env-config-configmap-key-name-mismatch-<n>-of-<m>-findings` | A `configMapKeyRef`'s `key` differs from the name of the env var it feeds — e.g. env `MH_STREAM_COST_VIDEO_BPS` reading key `MH_STREAM_COST_AUDIO_BPS`. **Not runtime-fatal, which is exactly the danger**: the reference resolves, the pod starts and passes readiness, and the process silently runs another variable's value. The VIOLATION line names the workload and both names; fix the `key:` to equal the env `name:`. **Do NOT add an alias exception to clear it** — `key == env name` is frozen repo-wide for ConfigMaps (zero violations at introduction); a genuine need for an alias is a rule change to argue on its merits (see `CONFIGMAP_KEY_NAME_MISMATCH_RULE_ID`'s doc). Scoped to `configMapKeyRef` only: `secretKeyRef` legitimately differs (`infra/services/redis/statefulset.yaml`, `REDISCLI_AUTH` ← `REDIS_PASSWORD`). | §6.3 |
| `env-config-orphan-key-<n>-of-<m>-findings` | A ConfigMap key (a `data:` key, or a `KEY=` line in a `configMapGenerator` env source such as `infra/services/<svc>/config.env`) that no workload **naming that ConfigMap** references. Not runtime-fatal, but the key is misleading: it looks configurable and changing it does nothing (an operator raising `OTEL_SAMPLE_RATE` mid-incident would see no effect). Fix by referencing it via `configMapKeyRef` in every workload that needs it, or removing it. Per-instance ConfigMaps are handled correctly — a key in `mh-0-config` referenced only by `mh-0-deployment.yaml` is NOT an orphan. | §6.3 |
| `kustomize-content-addressing-unverifiable-<n>` / `kustomize-content-addressing-vacuous-<n>` | **The guard refused to claim coverage** of the ADR-0038 content-addressing invariant (R-21), printed first as `ERROR: PRECONDITION`. `unverifiable` = neither `kustomize` nor `kubectl kustomize` is on PATH, so nothing could be rendered. Unlike R-15/R-17/R-18/R-19, which degrade to a WARN, R-21 FAILs, because it is the only enforcement of the invariant. Layer 3 always has kubectl (the devloop image installs it, and GitHub ubuntu runners ship it), so on those hosts this is an environment fault. `vacuous` = there were zero build targets under `infra/`, or the Kind environment root rendered with zero pod-template ConfigMap references. The second is a broken reference walk (`crates/dt-guard/src/common/pod_spec.rs`), not operator drift. | §6.3 |
| `kustomize-configmap-not-content-addressed-<n>` | A rendered pod template consumes a ConfigMap in a way that breaks ADR-0038 §2. The VIOLATION line gives the rule id. `configmap_not_content_addressed` = the ConfigMap has no kustomize hash suffix (or has a look-alike suffix that no generator declares): make it a `configMapGenerator` entry, because a plain ConfigMap change rolls nothing. `configmap_reference_unresolved` = the reference names a ConfigMap that does not exist in the workload's namespace in the render; usually a generator without `namespace:`, because kustomize silently skips the name rewrite across namespaces. `configmap_unreferenced` = the positive control: a ConfigMap in the render that no pod template references. It has no exemptions. The commonest cause is a new Grafana dashboard group added to `infra/grafana/kustomization.yaml` but not to the `dashboards` projected volume in `infra/grafana/deployment.yaml`. Otherwise it is dead config, or a reference shape the walker cannot see. | §6.3 |
| `grafana-datasources-promtail-labels-unavailable` | The Loki-label rule could not derive its valid label set from `infra/kubernetes/observability/promtail.yaml`. The file is missing or unparseable, has no `scrape_configs`, or yields zero labels. This fails closed. It used to silently skip the whole rule. The `ERROR:` line on stderr names the cause. | §6.3 |
| `release-build-profile-{no-dockerfiles-discovered,workspace-manifest-unreadable,cargo-config-unparseable,dockerfile-no-cargo-build-line,service-roster-underivable,service-crate-without-dockerfile,canonical-services-roster-drift}-<n>` | **The guard refused to claim coverage** — it could not check something it is responsible for, which outranks any policy finding and always names the run. Printed FIRST (before any `VIOLATION:`) as `ERROR: PRECONDITION [<token>] …` so it survives the `head -5` re-emission cap. `<n>` is the hit count for the winning class only. **Read the two sub-classes differently — this is the triage decision:** <br>**(i) Environmental — you did NOT cause it by editing an input.** `no-dockerfiles-discovered` (the `infra/docker/*/Dockerfile` walk matched nothing), `workspace-manifest-unreadable` (root `Cargo.toml` absent or unparseable), `dockerfile-no-cargo-build-line` (a service image with no `cargo build`/`cargo chef cook` line), `service-crate-without-dockerfile` and `canonical-services-roster-drift` (the service roster and the image tree disagree). Do **not** go hunting in `infra/docker/` — start at the guard's discovery code. <br>**(ii) Causable by an ordinary edit — go fix the file the message names.** `cargo-config-unparseable` = `.cargo/config.toml` (or `.cargo/config`) is not valid TOML; breaking it is a normal edit, so the environmental advice above would steer you away from exactly what you just did. `service-roster-underivable` = `[workspace] members` uses glob entries (`crates/*`), which cargo supports but which cannot be resolved to a service roster — without this the coverage floor checked nothing and the guard reported OK. Both messages name the file and the remedy. <br>**Lane note**: all seven are precondition failures by intent but arrive as `STATUS=FAIL` (exit 1) — a guard subprocess cannot self-declare a lane (see the caveats below), so the token IS the lane. <br>**Do not generalise the sub-class split above to every `ERROR: PRECONDITION` token.** `media-telemetry-deny-*` reuses this prefix with the INVERSE triage: there it always means a diff defect, never the environment, and its body says `THIS IS A DIFF DEFECT, not a machine fault`. | §6.3.1 |
| `dockerfile-build-not-release-<n>` / `release-profile-debug-assertions-enabled-<n>` / `cargo-config-release-profile-debug-assertions-<n>` / `release-inheriting-profile-debug-assertions-<n>` / `rustflags-debug-assertions-<n>` / `cargo-profile-env-debug-assertions-<n>` / `dockerfile-cook-profile-mismatch-<n>` | **A shipped artifact could have `debug_assertions` ON**, which silently disarms every `compile_error!`-based compile-time control (ADR-0036 §11). Real policy violations, implementer lane. The token appears verbatim in the `VIOLATION: [<token>] <file>:<line>` line, so grep the token not the STATUS. Six distinct inputs can flip it: a Dockerfile losing `--release` or selecting a non-release `--profile`; `[profile.release]` or a `[profile.release.package.<crate>]` override in the root `Cargo.toml`; the same in `.cargo/config.toml`; a custom profile with `inherits = "release"` that a Dockerfile selects; `RUSTFLAGS=-C debug-assertions`; or a `CARGO_PROFILE_RELEASE_DEBUG_ASSERTIONS` env/ARG — the last of which flips it with every file the guard's name suggests left byte-identical. Fix the cited input; there is no bypass marker. | §6.3.1 |
| `release-build-profile-violation` / `release-build-profile-violations-<n>` / `insecure-browser-setting` | **Guard wiring fault, not a diff defect — and the finding it carries is real but unclassified.** These are the fallback tokens in `release_build_profile.rs:token_for_rule` / `reason_for` and `no_insecure_browser_flags.rs:token_for`. They are unreachable by construction while every rule id is registered, so seeing one means **a rule was added to the guard without being added to `RULE_ORDER` (or to `token_for`'s match)**. Consequence: the hit is reported but lands outside the precedence ladder, so it cannot be triaged from this table and — for the release guard — cannot be recognised as a precondition class. Fix the registration in the guard, then re-run; do not triage the underlying finding from the fallback token, because its lane is exactly what the missing registration lost. | §6.3.1 |
| `no-insecure-browser-flags-no-candidate-files-<n>` | Vacuity twin of the row above for the browser-flag guard — the git-tracked walk matched zero candidate files, so the guard verified nothing. Same fail-closed rationale, same "not operator drift" reading. | §6.3.1 |
| `insecure-browser-cert-validation-flag-<n>` / `insecure-browser-origin-or-websecurity-flag-<n>` / `insecure-browser-config-property-<n>` | A script, config, runbook or test invocation carries a browser/harness setting that disables certificate validation, forces an insecure origin to be treated as trustworthy, or turns off web security. **Standing prohibition (story R-33) — there is no accepted use.** Browser trust here flows exclusively through `serverCertificateHashes` pinning, so any one of these turns the pinning assertion into decoration while every test stays green. The allowlist is literal-path only (never prefix or glob) and each entry carries an inline justification; **do not widen it to clear a finding** — remove the setting instead. If the demo origin is not a secure context the fix is a TLS dev origin, not a flag. | §6.3.1 |
| `kustomize-configmap-annotation-size-<n>` (a ConfigMap named, "estimated … annotation") | **Implementer lane, a real finding.** A generated ConfigMap's modelled `last-applied-configuration` annotation (JSON-escaped data, as kubectl's client-side apply writes it, plus a fixed metadata allowance) is past 80% of Kubernetes' 262,144-byte cap. Past 100%, cluster setup fails outright at `apply -k`. Remedy: split a file out into a generator of its OWN (as `grafana-dashboards-mc-media` was split from `-mc`), and if it ships dashboard JSON, give the new generator the sidecar label too, or the next row fires. Do not raise the threshold to clear it: it is derived from one measured task's growth (`crates/dt-guard/src/kustomize_configmaps.rs`). | §6.3 |
| `kustomize-configmap-annotation-size-<n>` with "ZERO configMapGenerator entries", "parsed ZERO entries", "has no `name`", "cannot be read" or "resolves outside the repository" in the message | **The guard refused to claim coverage — it verified nothing for that input, and this is NOT operator drift.** Same token as the row above, different meaning: the `infra/` walk found no generators at all, or one kustomization's `configMapGenerator:` section parsed to nothing, or an entry lost its name (kustomize requires one, so the parser lost track), or a referenced file is unreadable or escapes the repository. Do **not** go editing `infra/` to make it pass: start at the parser (`parse_generators`) or the walk, unless the message names a file you just moved or renamed. | §6.3 |
| Layer 3 `suppression-past-due` | A suppression hit its `expires` — INTENTIONAL red. Renew (re-verify + `--fix` + reviewed PR) or fix the advisory. | §6.3 |
| Layer 3 `suppression-drift` | Derived files (`.cargo/audit.toml` / `.pnpm-audit-ignore.json`) drifted from the manifest — run `scripts/audit-suppressions-check.sh --fix`. | §6.3 |
| Layer 3 `suppression-malformed` | Manifest/derived file unparseable — fix the offending line named in the `MALFORMED:` output. | §6.3 |
| Layer 3 `suppression-quality` | Empty reason/ticket, bad `expires`, or bad `ecosystem` in the manifest. | §6.3 |
| Layer 3 `suppression-override-without-test-sentinel` | A test-injection override env set without `DEVLOOP_TEST=1` — tamper/misconfig; investigate, do NOT unset-and-rerun. | §6.3 |
| `test-sentinel-set-in-ci` (layer-all / layer3) | `DEVLOOP_TEST` leaked into a CI job — pipeline-integrity incident; find + remove what exported it. | §6.3 |
| `layer-script-dir-set-in-ci` (assert_no_ci_sentinel_leak) | `LAYER_SCRIPT_DIR` (a local-only orchestrator test seam) leaked into CI — could forge the Gate-2 verdict; find + remove what exported it. | §6.3 |
| Layer 3 `STATUS=FAIL REASON=dt-guard-binary-missing` (any `guards/simple/**` wrapper) | `target/release/dt-guard` absent. The former producer/consumer skew (built by `lang/rust/compile.sh` inside a *skip-gated* compile verb, consumed always-run by ~15 guards) is **CLOSED since 2026-08-20**: the compile verb is now always-run (ADR-0033 §3), so both binaries build on every devloop, including `packages/**`-only and docs-only diffs. If this still fires, it is a genuine build failure or a fresh checkout — Fix: `cargo build --release -p dt-guard -p dt-story`, or just re-run Layer 1. **Note the lane**: the wrapper exits **1** (implementer). | §6.3 |
| Layer 3 `dt-story not built but manifests exist` (`validate-story-manifest.sh`) | Same story — `target/release/dt-story` is built by the same now-always-run compile verb and consumed by an always-run guard (and by `scripts/workflow/run-story.test.sh`). The skip-skew is closed. `scripts/workflow/preflight-story.sh` still asserts both binaries at story start (operator lane) as belt-and-braces. | §6.3 |
| Multiple `STATUS=` lines for one Layer-3 child (the most common instance: a **guard timeout** — `PRECONDITION_FAILURE REASON=guard-timeout-<name>` from run-guards.sh **and** `FAIL REASON=guards-failed` from `run_and_emit`; a mixed timeout+violation run adds a third, `FAIL REASON=guard-violations`) | **Expected, not a bug — read the ladder, not the last line.** `run_and_emit` (`lang/_common.sh`) appends its own `STATUS=FAIL <prefix>-failed` whenever a child exits non-zero, *in addition to* any `STATUS=` line(s) the child printed itself. `tee_collect_statuses` collects **all of them** and `aggregate_worst_status` resolves worst-wins, so a child's `PRECONDITION_FAILURE` outranks the appended `FAIL` and the layer's verdict is the operator lane. The child's own REASON token names the real cause; the `-failed` one is just "a child exited non-zero". A child that prints **no** STATUS line of its own has only the appended `FAIL`, which is why a wiring fault from such a child reaches the implementer lane — see the caveat below. | §3 (ladder) + §6.3 |
| Layer 3 `dependabot-ignore-present` | `.github/dependabot.yml` has a non-empty `ignore:` block — move suppressions to `audit-suppressions.toml`; Dependabot `ignore:` is not a suppression channel. | §6.3 |
| Open `audit-drift` GitHub Issue / red `Scheduled Audit` run | Between-PR drift: a new advisory against an UNCHANGED lockfile, caught by the weekly scheduled scan. Read the `STATUS=FAIL REASON=` lines in the issue body first (advisory vs broken tool vs "did not complete"); triage an advisory like a Layer-6 one. The issue auto-closes only when a later scheduled scan completes clean. | §6.6 |
| Layer 6 `buf-breaking-failed` on an intentional wire-break | No per-finding override exists (ADR-0033 Wave 3 #10 / task #41, unbuilt). Escalate to the human; only they can accept a wire break, and acceptance is recorded as a path-scoped `breaking.ignore` in `proto/buf.yaml`. Operators do not add one during triage. | §6.6 |
| Layer 6 `buf-breaking-passed` preceded by `SUPPRESSED=<paths>` | Enforcement is OFF for those paths via `proto/buf.yaml` `breaking.ignore`. Green is not evidence. Check the restore condition in that file's comment block. | §6.6 |
| Layer 6 `base-ref-unresolved` | Degraded git state ahead of `buf breaking` — jump to resolver playbook. | §5 |
| Layer N missing `BASE_REF=` line entirely | Wrapper bug — resolver did not run | escalate |
| Every pipeline emits dozens of `BASE_REF=` lines | Known cost concern (task #42 Tech Debt Pointer #2); cache + suppression-sentinel mitigation tracked, not yet implemented. | §5 (Known cost concern) |
| `WARN BUDGET_BREACH LAYER=<n>` | A single layer exceeded its budget (default 20s) | §3 |
| `WARN BUDGET_TOTAL_BREACH` | Guard+audit fast tier (layers 3 + 6) exceeded 90s p95 — ADR-0033 §4 budget. | §3 |
| `WARN BUDGET_TOTAL_SKIPPED REASON=layers-not-run LAST_RAN=<n>` | Interactive fail-fast stopped before layers 3 AND 6 both ran, so the 90s fast-tier check was SKIPPED (not evaluated against half-measured data). Informational; re-run without a stop (`DEVLOOP_FAIL_FAST=0`) to measure the tier. | §3 |
| Layer 3 `guard-timeout-<name>` / `guard-timeout-kill-<name>` (`STATUS=PRECONDITION_FAILURE`, exit 2) | A guard hit its 30s timeout / was SIGKILLed — **operator lane, does NOT consume an implementer attempt.** Re-running IS reasonable (suspect concurrent machine load / OOM). **BUT** if it REPRODUCES on retry or a quiet machine (or the guard's runtime jumped vs normal + the diff grew its input set), it's DIFF-CAUSED → route to the implementer + consume an attempt. Contrast `org-provision-unverified` etc. (L7), where re-running blindly is wrong. Do NOT raise/remove the timeout as "the fix". | §6.3 |
| `MIXED_LANE: precondition=<n> violations=<m>` (Layer 3) | A guard timed out AND another found a real violation in the same run. Exit 2 (operator lane) — but the `<m>` violation(s) are REAL: scan for `VIOLATION` / `FAILED:` in `layer-3.log`. Re-run on a quiet machine for a clean implementer-lane verdict on the violation. | §6.3 |
| `PIPELINE_MODE=<fail-fast\|run-all> SOURCE=<label>` (stderr, always) | The mode `layer-all.sh` ran in + WHY (`interactive-default` / `headless` / `github-actions` / `fail-fast-env` / `run-all-env` / `unattended-override-refused`). Informational — a fact in the log so "why did it (not) stop early?" needs no round-trip. | §3 |
| `FAILURE_TRIAGE LAYER=<n> LOG=<path> STDERR_LOG=<path> RESULT=<status>` (stderr) | The point-of-failure directive `layer-all.sh` prints (ADR-0037 D8) after the summary table, one per RED layer — it names that layer's log + stderr-log so you read them instead of re-running. NOT itself a failure; it is the triage pointer. Layer 3 appends `FAILED_GUARD_NAMES=`; Layer 7 appends a §6.7 pointer. | §1 + §3 |
| `FMT_MODE=<apply\|check\|invalid> SOURCE=<why>` (stderr) | The Layer-2 APPLY-vs-CHECK lane anchor (ADR-0037 D7), one SSoT emitter (`fmt_mode_emit`, `scripts/lang/_common.sh`). Informational — `apply` rewrites drift locally (needs `DEVLOOP_FMT_APPLY=1`); `check` (the default, incl. CI / attesting gates) never writes. (**Note:** `FMT_LANE` is NOT a real emitted token — it was a mis-name (surfaced in task text and, historically, a since-removed `layer-all.sh` comment); the real anchor is `FMT_MODE=`. This parenthetical is the in-tree record of that mis-name — load-bearing, do not trim as redundant.) | §6.2 |
| `FMT_APPLIED=<comma-sep repo-rel paths>` | An APPLY-lane fmt wrapper rewrote these files in place (still `STATUS=OK` — auto-applying a semantically-empty formatter is not a failure). An advisory breadcrumb, not an authenticated record — review + stage the files. Its absence on an APPLY run = the tree was already clean. | §6.2 |
| `FAILED_GUARD_NAMES=<comma-sep guard names>` | The greppable, closed-vocabulary list of WHICH Layer-3 guards VIOLATED — on stdout only on a violation run (`run-guards.sh`), and surfaced in the D8 `FAILURE_TRIAGE` directive. The one channel that NAMES the guards (the STATUS `REASON=` is the anonymous fused `guard-violations`; see the caveat "a violation's STATUS line names no guard"). Carries no secret by construction. | §6.3 |
| `STOPPED_EARLY LAYER=<n> RESULT=<enum> NOT_RUN=<list>` + `RESULT=NOT-RUN` cells | Interactive fail-fast stopped at the first failing layer; layers in `NOT_RUN` did not run (NOT the same as `SKIPPED-*` or a truncated log). Exit = the failing layer's code. `NOT-RUN` never aggregates and is never OK — it's not a Gate-2 pass over un-run layers. | §3 |
| `WARN FAIL_FAST_OVERRIDE_IGNORED REQUESTED=1 MODE=run-all` | An explicit `DEVLOOP_FAIL_FAST=1` was refused under an unattended lane (`GITHUB_ACTIONS` / `DEVLOOP_HEADLESS`) — the authority gate runs all layers regardless (an ambient `=1` must not shrink its coverage). To fail-fast, run interactively (neither env set). | §3 |
| `PRECONDITION_FAILURE: DEVLOOP_FAIL_FAST=<v> is not a recognized boolean` (layer-all startup, exit 2) | A malformed `DEVLOOP_FAIL_FAST` value (typo). Fail-closed. Use `1/0/true/false/yes/no` (or unset). | §3 |
| `PRECONDITION_FAILURE: … REASON=fail-fast-exit-invariant` | Orchestrator bug — fail-fast broke early but `final_exit==0`. Should be impossible (fail-fast only stops on `rc != 0`); if seen, it's a `layer-all.sh` regression, exit 2 (fail-closed). | §3 |
| `❌ Gate-2: no validation verdict found` (at commit) | Devloop-completion commit but no verdict — pipeline never run this session. Run `./scripts/layer-all.sh`. | §8.5 |
| `❌ Gate-2: verdict is FAIL` (at commit) | The recorded pipeline run did not pass — fix the failing layer(s) and re-run. | §8.5 |
| `❌ Gate-2: ... signature mismatch` + `modified after validation: <path>` | A validated file changed after the run — re-stage + re-run. | §8.5 |
| `❌ Gate-2: ... staged but not in verdict: <path>` | A new validated file was staged the verdict never saw — re-run. | §8.5 |
| `❌ Gate-2: ... validated but not staged: <path>` | A validated (often untracked/transient, e.g. a dirtied lockfile) file is in the verdict but not staged — re-run, or `--no-verify` if intentional. | §8.5 |
| `❌ Gate-2: verdict is for a different devloop` | Stale verdict from another devloop in the same `/tmp` session — re-run for THIS one. | §8.5 |
| `❌ Gate-2: shared library missing` | Broken checkout (`scripts/lang/_gate2_binding.sh` absent) — restore it, or `--no-verify` if intentional. | §8.5 |
| `⚠️  Gate-2: replay skip: <path> …` (at commit) | A conflict-resolved cherry-pick/rebase of a validated devloop commit; no local verdict checked for this commit. Not a failure. | §8.5 Replays |
| `⚠️  Gate-2: replay skip not applied to <path> … [<token>]` (at commit) | A replay that isn't vouched for; the reason token selects the fix. | §8.5 failure table |

### Caveat: a missing guard binary reaches the implementer lane

The two guard-binary rows above are **correct detection on the wrong lane**, and the lane is not
fixable at THOSE guards. A missing build artifact is a wiring fault — the operator class by §4's own
convention — but both emitters `exit 1` and print **no `STATUS=` line of their own**, so the only
STATUS the layer sees is `run_and_emit`'s appended `STATUS=FAIL <prefix>-failed`, which maps to exit 1.

**This is NOT because layer3 "flattens" a guard-level `PRECONDITION_FAILURE`.** But the reason is
narrower than this section claimed before 2026-09-01, and the earlier wording — "in-guard
reclassification survives precisely WHEN the guard emits its own STATUS line" — was **wrong in a way
that would mislead a guard author into building something that silently does not work.** Corrected:

> **Whether a `STATUS=` line reaches Layer 3's aggregation depends on WHO PRINTED IT.**
>
> - **Lines printed by `run-guards.sh` itself** — its 124/137 timeout arms and its
>   `guard-violations` summary — land directly on Layer 3's stdout. They are collected by
>   `scripts/lang/_common.sh:tee_collect_statuses` and participate in BOTH
>   `aggregate_worst_status` (so `PRECONDITION_FAILURE`, which outranks `FAIL`, flips the layer to
>   exit 2) and `scripts/lang/_common.sh:worst_reason_for_status` (called from
>   `__layer_lifecycle_end`), which takes the
>   **FIRST** pair matching the winning enum and puts it on the stderr `LAYER=3 … REASON=` anchor.
> - **Lines printed by an individual guard subprocess are NOT.** `scripts/layer3.sh` invokes
>   `run-guards.sh` with no `--verbose` flag, and `VERBOSE` defaults to false there, so each guard
>   runs under `OUTPUT=$(… "$guard" … 2>&1)` in the non-verbose branch of the guard loop —
>   **its stdout is captured, not passed through.** Only text matching `VIOLATION|violation|ERROR|error|WARN` is re-emitted, capped
>   at `head -5`. `STATUS=PRECONDITION_FAILURE` matches none of those tokens (it contains `FAILURE`,
>   not `ERROR`), so a guard's own STATUS line is **swallowed**.

**Consequences for anyone writing a guard.** Do not emit `STATUS=PRECONDITION_FAILURE` from a guard
binary expecting to reach the operator lane — it will be dropped and you will land on the implementer
lane anyway, with code that *looks* like it handled the lane. A guard surfaces diagnostics through
`VIOLATION:` / `ERROR:` stdout lines (which is why the vacuity classes in §6.3.1 are emitted as
`ERROR: PRECONDITION [<token>] …` — the `ERROR:` prefix is what gets them past the re-emission grep
at all) and through the token it carries in that text. **The token is the only channel that can carry
a lane**, which is why guards that need the distinction encode it in the REASON vocabulary rather
than the enum.

The dt-guard-binary-missing wrappers reach the implementer lane because they emit no STATUS line AND
are the captured subprocess — giving them one would not fix it. The fix has to be at
`classify_guard_exit`, in `run-guards.sh` itself, where an echo is outside the capture. That change
is tracked in `docs/TODO.md` §Guard STATUS Attribution; note it would be a **fourth** hand-rolled
`STATUS=` emission, which `run-guards.sh`'s exit-precedence ANCHOR comment records as the standing
trigger to revisit routing
through `emit_status` (rejected on coupling grounds per ADR-0015 §Pre-commit standalone use). It is a
design decision, not a patch.

### Caveat: a violation's STATUS line names no guard

A second asymmetry at the same emission site, worth knowing before you grep. `run-guards.sh`'s
violations-summary emission is a **fused constant** — `STATUS=FAIL REASON=guard-violations` — once when `FAILED_GUARDS > 0`,
naming neither the failing guard nor its rule. The 124/137 arms, by contrast, carry
`guard-timeout-${GUARD_NAME}`.

> **The operator lane is attributable per-guard; the implementer lane is not.** A timeout tells you
> WHICH guard. A violation tells you only that something violated.

So for a normal violation the stderr `LAYER=3 … REASON=` anchor — the 3am attributable-cause line —
resolves to the bare `guard-violations`. The guard's identity survives only in the human
`FAILED: <name>` line and in the `VIOLATION|ERROR|WARN` text re-emitted from the captured output,
**capped at `head -5`**. That cap is why guards that fail closed on vacuity print their precondition
line first: a coverage refusal buried at line six is invisible. Tracked in `docs/TODO.md`
§Guard STATUS Attribution.

Consequence worth knowing before triaging: `run-story.sh` routes a gate exit 1 to a **task
escalation** and anything else non-zero to the operator lane, so this failure is recorded against
whatever task happened to be running. If a story escalates with `pipeline-red` and the gate log
names either symptom above, the task is not the cause — rebuild the binaries and rerun.

Largely resolved as of 2026-08-20: the compile verb is now **always-run** (ADR-0033 §3 — the
skip-if-untouched short-circuit was retired), so guard-binary production already sits on an
always-run path and the skip-skew that produced these symptoms is gone. What remains as a possible
`docs/TODO.md` follow-up is a `run_and_emit` enum that can carry `PRECONDITION_FAILURE` (the lane,
above). The build sites are down to what each context needs (2026-10-05): `lang/rust/compile.sh`
produces for every pipeline run, local or CI (ci.yml's separate build step is gone — its guards
shard runs Layer 1 first); `ci-client.yml`'s Lint job builds its own, being the only producer in a
job that has no Layer 1; `preflight-story.sh` asserts.

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

The only **non-bypassable** enforcement is **CI**: on PRs into main/develop (see
`.github/workflows/ci.yml` `on:`) it re-runs `./scripts/layer-all.sh` from scratch — sharded
across parallel jobs (`--layers`), with the `Test Suite` aggregate (`--aggregate`) proving
every layer ran exactly once (§8.7) — and **never reads** the `/tmp`
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
The hook is **FAIL-CLOSED**: any error evaluating the trigger or verdict blocks. File lists
are read with `--no-renames` on both sides, so a `git mv` of a validated file (even into
`docs/devloop-outputs/`) still binds its deletion.

**Phase is not a bypass.** A devloop main.md staged at any Phase other than `complete` together
with ≥1 validated file is **refused outright** — see [Phase not complete](#phase-not-complete).
So conjunct 1 can only pull a commit *into* the verdict requirement, never keep it out.

### Phase not complete

Since 2026-10-06 (`gate2_refuse_incomplete_mainmds`, run before everything else in
`gate2_validate_commit`): if the commit stages ANY devloop `docs/devloop-outputs/<slug>/main.md`
(not `_template`, not a deletion) whose staged Loop State row is not `| Phase | \`complete\` |`,
**and** the staged changeset minus exclusions (the same set conjunct 2 uses) is non-empty, the
commit is blocked:

```
❌ Gate-2: [phase-not-complete] devloop main.md staged at Phase≠complete with validated files
   docs/devloop-outputs/<slug>/main.md: Phase reads '`setup`'; the gate needs the Loop State row "| Phase | `complete` |".
   Validated files staged with it (N): <up to 5 paths> [+K more]
   If this is the devloop you are committing: set its Loop State Phase to `complete` — the hook then requires the Gate-2 verdict (devloop SKILL.md Step 8).
   If you are editing an older devloop record: commit the docs/devloop-outputs/ change separately from the code.
```

- Grep for `[phase-not-complete]`. "Not complete" is everything except lowercase `complete` as the cell's leading word (leading/trailing spaces or tabs, backticks and a trailing annotation like `(iteration 2)` are fine): a missing
  row, an empty cell, an empty file, `COMPLETE`, `Complete`, a reformatted row — fail-closed.
- Applies even when another, complete main.md is also staged (mixed commits are refused).
- A commit staging only bookkeeping files (`docs/devloop-outputs/**`, `docs/TODO.md`,
  `docs/specialist-knowledge/*/INDEX.md`, `docs/user-stories/*.md`) is never refused: no verdict
  could be required for it. **This deliberately uses the binding's exclusion set, not a literal
  "outside `docs/devloop-outputs/`"** — the refusal fires exactly when setting Phase=complete
  would make the verdict required.
- **Replays get no exemption.** A conflict-resolved cherry-pick/rebase of a historical commit
  whose record never reached `complete` (27 such commits exist, all ancestors of main) is refused,
  and the message adds: *only if you are replaying such a commit, after resolving the conflict,
  with that main.md byte-identical to the replayed commit, `git commit --no-verify` is your
  decision — it skips ALL pre-commit checks; CI re-runs the pipeline but does not check the
  record.* A clean rebase/cherry-pick does not run the hook at all. The complete-record replay
  skip ([Replays](#replays)) is unchanged.
- `--no-verify` is not otherwise a sanctioned escape for this refusal: the fix is setting Phase
  (current devloop) or splitting the commit (older record).

**Residuals (accepted, stated plainly).** The hook judges the staged delta against HEAD, so it
cannot see (a) code committed with no main.md staged, followed by a main.md-only commit at
`complete`, or (b) `git commit --amend` that adds or advances main.md over an earlier code-only
commit — the ci-speed devloop's own record was fixed by exactly such an amend. **There is no
CI-side record or verdict check**: CI re-runs `layer-all.sh` from scratch on PRs into
main/develop (it validates the tree), but nothing in CI checks that a devloop record reached
`complete` or that a verdict existed. Deleting a main.md is equivalent to not staging one.
Separately, the excluded paths are guard inputs (story manifests → validate-story-manifest, the
`docs/TODO.md` frame-vectors marker, specialist `INDEX.md` → the knowledge-index guard, main.md →
the cross-boundary guards): edits to them after validation are not re-gated locally (except the
hook's dt-guard `todo-tracking` run); CI's re-run covers them.

**Exception — vouched replays.** A staged complete main.md that is a byte-identical replay of
an already-validated devloop commit (conflict-resolved cherry-pick or rebase) does not count
toward conjunct 1. See [Replays](#replays) below.

### Replays

A conflict-resolved replay re-stages a devloop main.md that already passed Gate 2 when the
replayed commit was made. The validated bytes don't travel with the patch, so before this
exception every such replay needed `--no-verify`. Now, while git is replaying, a staged
complete main.md is dropped from the trigger iff **all** hold
(`gate2_replay_source` + `gate2_replay_qualifies` in `scripts/lang/_gate2_binding.sh`):

1. no `MERGE_HEAD` (merges are not vouched for — see below);
2. exactly one of `CHERRY_PICK_HEAD` / `REBASE_HEAD` resolves to a commit, and it is the
   pseudoref itself, not a branch of that name;
3. the staged blob equals that path's blob in the **replayed commit** (the pseudoref's
   commit itself — not an ancestor);
4. the replayed commit has exactly **one parent**, read from the raw object (replace refs and
   grafts are ignored) — a root or a merge commit is never vouched for;
5. the replayed commit **wrote exactly one** complete devloop main.md (added or modified
   against that parent), and it is this path.

The evidence is the commit's tree, not its message; the path carries the slug. A commit that
writes a complete main.md went through this hook when it was made, or through a hook-free
path (`--no-verify`, `git rebase --continue`, a clean pick, `commit-tree`) — CI is the
backstop either way. Exactly-one mirrors the creation-time rule: a commit completing two
devloops can only have bypassed the hook.

Anything else — edited main.md, a record an earlier commit wrote, a root or merge commit, a
commit writing two records, no replay in progress — keeps the verdict requirement. Any git error while checking does too (never a skip).

**What this relaxes — the user's decision, stated plainly.** Only the trigger. When every
staged complete main.md qualifies, the **whole commit** needs no local verdict, including the
bound files the conflict resolution changed — bytes nobody validated locally. A mixed commit
(one vouched replay plus another complete main.md) still requires a verdict. The binding,
signature and exclusions are unchanged. CI on PRs into main/develop (see
`.github/workflows/ci.yml` `on:`) re-runs the full pipeline over those bytes; to validate
earlier, run `./scripts/layer-all.sh` yourself.

**Rebase — don't merge — branches carrying devloop records.** A merge carries a whole branch,
not one replayed commit, so `MERGE_HEAD` present (alone, or with `REBASE_HEAD` as in a
`rebase -r` stopped on a merge) never skips. Know which replays reach the hook at all
(git 2.39, verified):

| Operation | Runs pre-commit? | Replay state the hook sees |
|-----------|------------------|-----------------------------|
| `git cherry-pick --continue` / `git commit` at a cherry-pick stop | yes | `CHERRY_PICK_HEAD` → skip can apply |
| `git commit` at a stopped rebase | yes | `REBASE_HEAD` → skip can apply |
| `git rebase --continue` | **no** (the sequencer passes `-n`) | — Gate 2 is CI-only on this path, independent of this exception |
| `git commit` concluding a conflicted merge | yes | `MERGE_HEAD` → never skips |
| clean cherry-pick / rebase pick / merge (incl. absorb's `git merge --ff-only`) | no | — unaffected |

`absorb-devloop.sh` fast-forwards (no commit, no hook) or cherry-picks (covered above), so the
recommended rebase/absorb paths need no bypass.

**What you see.** When the skip fires, one line per main.md (terminal only — nothing persists
it; keep the replay output if you need the record):

```
⚠️  Gate-2: replay skip: <path> blob <oid> == <REF> <full-sha> (slug <slug>); Gate-2 verdict NOT checked for this commit — <N> bound path(s) not validated locally (<M> differ from the replayed commit) — run ./scripts/layer-all.sh to validate, or rely on CI on PRs into main/develop (.github/workflows/ci.yml `on:`).
```

If the counts can't be computed (a git error), the skip is withdrawn — no audit record, no
skip. In a mixed commit the vouched main.md prints
`⚠️  Gate-2: replay vouched for <path> (<REF> <full-sha>) — but another complete main.md in this commit still requires a verdict.`
instead, because the verdict *is* checked.

When a replay is in progress but the skip does not apply, a
`⚠️  Gate-2: replay skip not applied to <path> (<REF> <sha>|unresolved): [<token>] <reason>`
line prints directly above the usual ❌ block (git errors name the failed step and its rc,
with git's own stderr above). Then one action line per kind of refusal: a restore line naming
each edited path for `[main-md-modified]`, the merge recovery for `[merge-not-vouched]`, and
the validate-or-bypass line for everything else. With no replay in progress nothing new
prints.

**Validating on the host.** `./scripts/layer-all.sh` only yields a PASS verdict where Layer 7
can run, i.e. with the devloop cluster helper; on a bare host (where `absorb-devloop.sh` runs)
Layer 7 is a `PRECONDITION_FAILURE` and the verdict is FAIL. There, the realistic paths for a
refused replay are restoring main.md (`[main-md-modified]`) or a deliberate
`git commit --no-verify` with CI as the check — the user's decision.

**Reconstruct a skip decision afterwards** from the line's `<full-sha>`, `<path>` and `<oid>`:
`git rev-parse <full-sha>:<path>` must print `<oid>`;
`git --no-replace-objects cat-file commit <full-sha>` must show exactly one `parent` line; and
`git --no-replace-objects diff-tree -r --name-only <parent> <full-sha>` must list `<path>` as
the only complete devloop main.md.

The exception adds no state and no kill switch; `git commit --no-verify` remains the escape
hatch, and reverting the commit that introduced it restores the old behaviour.

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
| `[phase-not-complete]` (`devloop main.md staged at Phase≠complete with validated files`) | A staged devloop record is not at Phase=complete and validated files are staged with it ([Phase not complete](#phase-not-complete)). The line names the record, the Phase it read, and the files. | Current devloop: set Phase to `complete`, run `./scripts/layer-all.sh`, `git add -A`, commit. Older record: commit the `docs/devloop-outputs/` edit separately. Replaying a never-complete historical commit: see the replay note under [Phase not complete](#phase-not-complete). |
| `error evaluating the commit trigger (rc=3)` | The staged changeset could not be enumerated, or a present staged devloop main.md could not be read (git's error is above it). Shared by the verdict trigger and the Phase check. | Fix the repository error; the hook never treats an unreadable changeset as empty or an unreadable main.md as "not complete". |
| `error enumerating the staged changeset` (incl. `… for the Phase check`) / `error recomputing the staged-tree signature` | A git error while listing or hashing the staged tree (git's error is above it). Fail-closed. | Fix the repository error and retry. |
| `replay skip not applied … [main-md-modified]` | The staged main.md differs from the replayed commit's (edited during conflict resolution). | Take the incoming side: `git checkout <REF> -- <path>` (absorb: `--theirs` for `docs/devloop-outputs/**`), re-stage, resume. Or `./scripts/layer-all.sh && git add -A`, then resume. |
| `replay skip not applied … [path-absent-in-replayed-commit]` | A complete main.md is staged that the replayed commit doesn't contain. | Unstage it if it doesn't belong to this commit; otherwise validate (`./scripts/layer-all.sh && git add -A`) and resume. |
| `replay skip not applied … [not-written-by-replayed-commit]` / `[root-commit-not-vouched]` / `[replayed-merge-not-vouched]` / `[multiple-records-in-replayed-commit]` | The replayed commit did not write this main.md (an earlier commit did), has no parent, is a merge (`cherry-pick -m`), or wrote two complete records — it is not that record's single validated completion. Restoring main.md can't fix this. | With the devloop cluster helper: `./scripts/layer-all.sh && git add -A`, then resume (`git cherry-pick --continue` / `git commit` / rerun the absorb). On a bare host (no Layer 7): `--no-verify` with CI as the check — your decision. |
| `replay skip not applied … [merge-not-vouched]` | A merge is in progress; merges are never vouched for ([Replays](#replays)). | `git merge --abort` (discards the in-progress resolution), then rebase or rerun `absorb-devloop.sh`; or `git commit --no-verify` if you accept CI as the only check — your decision. |
| `replay skip not applied … [multiple-replay-states]` / `[ref-not-pseudoref]` | Both `CHERRY_PICK_HEAD` and `REBASE_HEAD` exist, or a **branch** is named like a pseudoref. | Finish or abort the stray operation / rename the branch; or validate and resume. |
| `replay skip not applied … [git-error]` | A git command failed while checking (its own error is printed above; the line names the step and rc). The skip is never granted on an error. | Fix the git error and resume, or validate (`./scripts/layer-all.sh && git add -A`, needs the devloop cluster helper). |

### Rollback / escape hatch

If a hook bug ever **wedges** a legitimate commit, `git commit --no-verify` bypasses it.
That is the documented escape hatch while diagnosing — but remember CI on PRs into
main/develop (see `.github/workflows/ci.yml` `on:`) re-runs the full pipeline, so a genuinely
failing tree will be caught there regardless.

### Isolation self-test

`scripts/guards/simple/selftest-gate2-verdict.sh` drives the producer + validator against
synthetic staged trees in throwaway temp git repos, covering the verification matrix
(a–f) plus slug-mismatch and extra-staged-file drift, the Phase≠complete refusal (`f2`, `f2b`,
`s1`–`s13`, incl. rename handling and the accepting Phase forms), and the replay skip against real
conflicted cherry-pick / rebase / merge state (`q*` allow, `r*` refuse — one case per refusal
token). It runs in Layer 3 (guards) every
devloop and in CI, and emits the ADR-0033 `STATUS=` contract line. Run it standalone to
reproduce a hook-logic regression without touching your real index:
`./scripts/guards/simple/selftest-gate2-verdict.sh`.

---

## 8.6 Coverage Lane (CI `coverage` job — instrumented dt-guard)

**This is a standalone CI job, NOT a `layer-all.sh` layer — do not hunt for it in a local
devloop run.** The `Code Coverage` job (`.github/workflows/ci.yml` § "Run tests with coverage")
runs only in GitHub Actions and does **not** invoke `scripts/layer-all.sh`. It uses
cargo-llvm-cov's **external-binary** flow so that the bash guard self-tests — which drive the
`dt-guard` binary as a subprocess — contribute to the same merged `lcov.info` as the Rust
`*_e2e.rs` fixtures. The chain: `cargo llvm-cov clean` → capture `show-env --export-prefix` to a
file and `source` it → build the **instrumented** `dt-guard` → `cargo llvm-cov --no-report`
(runs the cargo tests) → point `$DT_GUARD` at the instrumented binary → run the bash suites the
SSoT discovery script selects → `cargo llvm-cov report --lcov`. The suite list is **derived**, not
hand-maintained, by `scripts/guards/list-coverage-suites.sh` — today exactly
`counter-zero-init.test.sh` + `media-telemetry-deny.test.sh`.

**What the job reds on**: an instrumentation/plumbing fault (empty `show-env`; an instrumentation
assert failing — `RUSTC_WRAPPER` / `-Cinstrument-coverage` / `cfg(coverage)` absent from `show-env`,
i.e. cargo-llvm-cov stopped providing them, NOT a user `RUSTFLAGS` drop; `$DT_GUARD` unresolvable), a
selected bash suite failing, the per-suite profraw-delta assert, the discovery/drift check, or a
Codecov **upload** error (`fail_ci_if_error: true`). There is **no coverage-percentage threshold in
the workflow itself** — a `%` regression only reds if a `codecov.yml` / Codecov project status target
defines one (a separate Codecov-side concern, not enforced by this job).

**Reproduce a CI-only coverage red locally** (no cluster needed — this is why the suites must stay
hermetic; the job's checkout is deliberately shallow, no `fetch-depth: 0`):

```
cargo llvm-cov clean --workspace
cargo llvm-cov show-env --export-prefix > llvm-cov-env.sh && . ./llvm-cov-env.sh
cargo build -p dt-guard && cargo llvm-cov --no-report --workspace
export DT_GUARD="${CARGO_LLVM_COV_TARGET_DIR}/debug/dt-guard"
./scripts/guards/list-coverage-suites.sh > coverage-suites.txt   # fails non-zero on drift/floor
while read -r s; do DT_GUARD="$DT_GUARD" bash "$s"; done < coverage-suites.txt
cargo llvm-cov report --lcov --output-path lcov.info
```

This writes `llvm-cov-env.sh` and `coverage-suites.txt` to the repo root; both are gitignored build
artifacts (alongside `lcov.info` / `*.profraw`), safe to leave in a dirty tree.

Four failure shapes are specific to this job and appear in NO local layer:

| Symptom (in the `Code Coverage` job log) | Meaning | First action |
|------------------------------------------|---------|--------------|
| An **instrumentation precondition** fails: `::error::show-env did not set RUSTC_WRAPPER=cargo-llvm-cov`, or `::error::-Cinstrument-coverage missing …`, or `::error::cfg=coverage missing …`. | cargo-llvm-cov's `show-env` did not carry the expected instrumentation — a version/flow change in cargo-llvm-cov, or `show-env` was captured before the wrapper env was set. **NOT a defect in the code under test.** The three asserts are orthogonal and each fail-closed; each guards a distinct invariant: **RUSTC_WRAPPER** → the binary is actually instrumented (else 0% coverage reads green); **`instrument-coverage`** → lines are counted, not 0%; **`cfg=coverage`** → the three ac-service `#[cfg_attr(coverage, ignore)]` timing tests stay ignored. A lone `cfg=coverage` check would leave the uninstrumented-build mode a silent pass — checks 1-2 close it. | **LANE: infrastructure** (plumbing — see the §9 infra escalation row). Re-run `cargo llvm-cov show-env --export-prefix` and confirm all three tokens are present in the output. **ANTI-MASK: do NOT loosen any pattern to the space-form `--cfg coverage`** — the injected form is `--cfg=coverage`; a `show-env` format shift SHOULD red the job (fail-closed), never be tolerated into a false pass. |
| A selected bash suite is **RED under instrumentation but GREEN in local Layer 3** — it fails only in this job; the same suite against `target/release/dt-guard` (what Layer 3 runs) passes. | The suite runs against the **instrumented** binary via `$DT_GUARD`, which is a *debug* build in a *different* target dir (`${CARGO_LLVM_COV_TARGET_DIR}/debug/dt-guard`), not the release binary Layer 3 uses. The divergence is real: a debug-vs-release behaviour difference, or a suite assumption (a path, a timing, a `--release`-only optimisation) that holds only for the release binary. Reproduce with the local chain above using a plain `cargo build -p dt-guard` (debug) binary as `$DT_GUARD`. **LANE:** the local repro decides — a genuine instrumented-binary behavioural difference is the **implementer** lane (fix the suite or the guard), while a `$DT_GUARD` misresolution or job-env fault is **infrastructure**. **If it reproduces, fix the suite or the guard**, not the coverage job. **ANTI-MASK: do NOT** "fix" it by re-pointing `$DT_GUARD` at `target/release/dt-guard`: that strips instrumentation (defeating the whole job) and is the uninstrumented fallback the profraw-delta assert (next row) exists to forbid. If it reproduces ONLY under `cargo llvm-cov` and not with a plain debug binary, suspect a path the instrumented run relocated. |
| **`::error::<suite> produced NO new profraw (<before> -> <after>)`** followed by `::error::a coverage suite failed or produced no coverage`. | The **per-suite profraw-delta** assert fired: the suite ran but deposited zero new `*.profraw`, so the `dt-guard` child it invoked was **not instrumented** and its lines counted as 0% — which would otherwise read as green. Cause is almost always `$DT_GUARD` resolving to the plain `target/release/dt-guard` (restored by `rust-cache`) instead of the instrumented binary, or `LLVM_PROFILE_FILE` not propagating to the child. (The earlier preconditions catch a dropped `%p`/`%m` pattern and a non-executable `$DT_GUARD` up front, so those fail before the loop.) **LANE: infrastructure** — the child was not instrumented; this is plumbing, not a defect in the code under test. Confirm `echo "$DT_GUARD"` points into `${CARGO_LLVM_COV_TARGET_DIR}/debug/`, is `-x`, and is `!= $PWD/target/release/dt-guard`, and that the child inherits `LLVM_PROFILE_FILE` (pattern carries `%p`/`%m`). This assert is a **strict-growth delta snapshotted before each suite**, deliberately NOT a post-run "≥1 profraw exists" — the cargo tests already deposit many, so a bare existence check is vacuously green. It is per-suite so one silently-uninstrumented suite cannot hide behind the other's profraw. **ANTI-MASK: do NOT relax the strict-growth delta to a "≥1 profraw exists" check** — that is the exact vacuity it exists to catch. |
| A `list-coverage-suites:` error: **`found <N> DT_GUARD-driving suite(s), need >= 2`**, or **`<file> is under scripts/guards/, does not drive $DT_GUARD, and carries no # coverage-exempt: <reason> marker`**, or **`non-$DT_GUARD dt-guard binary invocation`**. | The SSoT discovery / drift check (`scripts/guards/list-coverage-suites.sh` — the SELECTION, FLOOR, DRIFT-MARKER and SECOND-GREP blocks; no enclosing function; flat script) failed. Three sub-cases. **(1) FLOOR** — fewer than 2 driving suites: a coverage suite was renamed/removed or refactored so it stopped matching `DT_GUARD`. **(2) MARKER drift** — a new `scripts/guards/*.test.sh` neither drives `$DT_GUARD` nor carries a `# coverage-exempt: <reason>` line. **(3) SECOND-GREP** — a `*.sh` under `scripts/` invokes `dt-guard <subcommand>` via a hardcoded path or bare command instead of `"$DT_GUARD"` (the message names `file:line`). | **(1) FLOOR** — restore the dropped suite; only lower the pinned `FLOOR=2` deliberately, with a note, if a suite genuinely stops driving the binary. A NEW (3rd) suite does NOT trip the floor — it is a floor, not a count-equality. **LANE:** whoever changed the suite. **(2) MARKER** — add the marker (reason ≥10 chars — **length floor only**, SSoT `ignore.rs::MIN_REASON_LEN`) if the file legitimately is not a coverage suite, or route its invocation through `"$DT_GUARD"` if it should be one. The marker universe is bounded to `scripts/guards/*.test.sh` on purpose — selection is recursive over all of `scripts/**`, so a driver placed elsewhere is still run; the annotation discipline is confined to where guard self-tests belong. **LANE:** whoever added/renamed the self-test. **(3) SECOND-GREP** — route the invocation through `"$DT_GUARD"` so the coverage job can point it at the instrumented binary. **LANE:** infrastructure / whoever added it. **ANTI-MASK:** the ≥10-char floor rejects short lazy tokens like `wip`/`todo`, so write a genuine descriptive reason — the bar is deliberately length-only (weaker than a `guard:ignore` suppression; the bash `is_lazy_reason` vocabulary is NOT mirrored here, since a portable ERE can't match its `\b`-boundaried regex and a copy just forks), so a lazy-but-long reason technically passes but defeats the marker's purpose. **And do NOT lower `FLOOR` just to green the job.** |

### Is `coverage` a required status check?

**A job existing in `ci.yml` does NOT mean its red blocks merge.** Branch protection is a GitHub repo
setting, **not** encoded in `.github/workflows/ci.yml` — a red check only blocks merge if its *context*
is in the branch's required-status-checks list. The check context is `Code Coverage`
(`.github/workflows/ci.yml § "Code Coverage"` — the job's `name:`), **not** the job key `coverage`.
Read the authoritative state with:

```
gh api repos/:owner/:repo/branches/main/protection/required_status_checks --jq '.contexts'
```

If `Code Coverage` is absent from that list, it is advisory.

**Current documented state: ADVISORY (not verified required).** Two reasons: (i) as of 2026-09-17 the
live config could not be read from the devloop environment (no authenticated `gh`); and (ii) GitHub does
**not** auto-add a new job's context to required checks — an admin must add it explicitly, so a
brand-new job defaults to advisory. Operationally, **while advisory a coverage-job red does NOT block
merge** — it only shows in the PR Checks tab, so someone must watch it, because a silently
uninstrumented suite (0% reading as green) is exactly the failure the profraw-delta assert exists to
make loud, and that loudness is wasted if nobody is looking. **Once required**, ALL of its red modes
(plumbing, suite, profraw-delta, discovery-drift, AND a Codecov upload error) block merge.

To make it enforce, a repo admin adds `Code Coverage` to the `main` branch-protection required checks —
a policy call for the repo owner, not this devloop. **Tracked in `docs/TODO.md` under "Polyglot Pipeline
Follow-ups", pending the repo-owner decision**; record the state here once decided.

**`Test Suite` (the pipeline) is the aggregate since 2026-10-05.** The pipeline runs as
`Pipeline (…)` shard jobs plus a `Test Suite` job that fails on any shard result other than
success and on any layer missing from the shards (§8.7). Only `Test Suite` needs to be
required — it kept the pre-split job's name so its branch-protection entry still applies; the
shard contexts need not be added.

## 8.7 CI Pipeline Shards (`.github/workflows/ci.yml` — `Pipeline (…)` jobs + `Test Suite`)

CI runs the same `layer-all.sh` as the devloop, split across parallel jobs so one runner no
longer builds the dev, test and clippy cargo graphs in series. Each **shard** runs
`./scripts/layer-all.sh --layers <list>` from scratch; the **`Test Suite`** job (the required
check, same context name as the old single job) runs `./scripts/layer-all.sh --aggregate` over
the shards' uploaded summaries. The partition is written only in the ci.yml matrix; the
aggregate requires every layer 1..`LAYER_MAX` (`scripts/lang/_common.sh`) exactly once, so a
layer dropped from or duplicated in the matrix reds `Test Suite`.

### Job map

| Job | Layers | Tools it installs (matrix flags) | Local repro |
|-----|--------|----------------------------------|-------------|
| `Pipeline (guards, layers 1,2,3,6,7)` | 1 compile, 2 fmt, 3 guards, 6 audit, 7 (CI clean skip) | nightly, kubectl assert, cargo-audit | `./scripts/layer1.sh` … or `./scripts/layer-fast.sh` |
| `Pipeline (test, layers 4)` | 4 test (cargo-nextest + doctests, TS) | postgres, sqlx-cli (cached by version), cargo-nextest, Playwright | `./scripts/layer4.sh` |
| `Pipeline (lint, layers 5)` | 5 lint (clippy, TS lint) | — | `./scripts/layer5.sh` |
| `Test Suite` | none — aggregates | — | `./scripts/layer-all.sh --aggregate <dir of */layer-summary.txt>` |

L1 and L3 share a shard on purpose: L3's guards consume the release `dt-guard`/`dt-story` that
L1's `scripts/lang/rust/compile.sh` builds. A layer moved to another shard needs its tool flags
moved with it; a missing tool fails loudly (tool not found), never silently.

### Reading a red `Test Suite`

| Symptom (in the `Test Suite` log) | Meaning | First action |
|-----------------------------------|---------|--------------|
| `::error::pipeline shards result is 'failure'` (or `cancelled` / `skipped`) | A shard job did not succeed. This step fails first, before anything is read. | Open the red `Pipeline (…)` job. The aggregate below it usually also prints which layer. |
| `FAILURE_TRIAGE LAYER=<n> SHARD=<shard> RC=<rc> RESULT=<status>` | Layer `<n>` failed in that shard. | Open that shard's job log: its own `FAILURE_TRIAGE` line and layer detail are there. Triage per §6.<n>. |
| `REASON=aggregate-missing-layer` | No shard reported that layer: the matrix stopped covering it, or a shard died before writing its summary (its upload step then also errors). | Check the shard's log first; if every shard ran, the ci.yml matrix lost the layer — restore it. **Never** "fix" by narrowing `LAYER_MAX`. |
| `REASON=aggregate-duplicate-layer` | Two shards ran the same layer. | Fix the matrix `layers` lists. |
| `REASON=aggregate-malformed-line` / `-unknown-status` / `-layer-out-of-range` | A summary line is not the exact shape layer-all writes (`LAYER=n RESULT=s DURATION=d RC=r`), or carries a status that is not a pipeline status (incl. `NOT-RUN` — CI shards always run all their layers). | The writer and parser live in the same file (`scripts/layer-all.sh`); a mismatch is a wiring bug, infrastructure lane. |
| `REASON=aggregate-no-summaries` / `-dir-unreadable` | Nothing was downloaded. | Usually every shard died before upload — open the shard jobs. |

### Rolling back

One devloop commit introduced the split together with its runner changes, so roll back per
concern:

- **CI split**: revert `.github/workflows/ci.yml` alone. `--layers`/`--aggregate` in
  `layer-all.sh` are additive and unused by the single-job form. **Never revert `layer-all.sh`
  without `ci.yml`** — the shards would call a flag that no longer exists.
- **nextest + tool pins**: revert `scripts/lang/rust/test.sh`, `.config/nextest.toml` and
  `infra/cargo-tools.versions`/`infra/lib/cargo-tools.sh` with their consumers, then rebuild the
  devloop image (`infra/devloop/devloop.sh --rebuild`).
- **Thin service binaries**: revert the ac/gc `main.rs`, the `allow(dead_code)` sweep **and**
  the `bin-lib-single-compile` guard together (the guard reds without the rest).

### Tool pins and image rebuilds

`infra/cargo-tools.versions` is the one pin for cargo-nextest, cargo-audit, cargo-llvm-cov,
cargo-fuzz and cargo-chef, read by CI (every workflow), the devloop image build and the service
image builds (`infra/kind/scripts/deploy.sh`, which rebuilds them on every deploy). CI picks a change up on the
next run. **Every existing devloop and run-story container** keeps its old binaries until the host
runs `infra/devloop/devloop.sh --rebuild`; until then Layer 4 reds with
`REASON=cargo-nextest-missing` or `cargo-nextest-version-mismatch` (§8 catalogue).

**When Dependabot opens a `rust-toolchain` PR** (a new stable Rust): review it like the 1.99 bump
(3651ebf7) — it changes only `channel` in `rust-toolchain.toml`, and every image derives its
`rust:<v>` from that file. Expect a cold Rust cache on that PR (rustc is in every cache key).
After merge, hosts run `infra/devloop/devloop.sh --rebuild`.

### Cache notes

Each shard saves its own rust-cache key (`ci-pipeline-<shard>`), also on red runs
(`cache-on-failure`). A `pull_request` run saves to its PR's ref and reads `main`'s cache; main
never reads a PR's (safe only while no workflow uses `pull_request_target`/`workflow_run`).
Caches unused for 7 days are evicted, and the repo holds 10 GB in total; if a first run on a PR
is repeatedly cold although `main` was pushed within the week, check the repo's cache list for
eviction thrash (shards + `coverage-cache` + `ci-client-dt-guard` + PR refs).

---

## 9. Escalation & Related References

### When to escalate

| Escalation target | Trigger |
|-------------------|---------|
| operations | A layer reports `RESULT=UNKNOWN` (dispatcher / wrapper bug — child crashed before STATUS line AND its EXIT trap did not fire, e.g. killed `-9`). Since task #50 this is rarer: the 14 verb wrappers self-emit `STATUS=FAIL REASON=wrapper-aborted-early-exit-<rc>` via an EXIT trap when they abort before emitting, so a pre-emit crash now surfaces as `FAIL`, not `UNKNOWN`. A genuine `UNKNOWN` therefore points at a non-trap path. |
| operations | A layer reports `RESULT=FAIL-MISSING-VERB` (exit 2 — task #52). A verb wrapper that should exist is missing/non-executable (deleted, `chmod`-stripped, or a new lang dir missing a verb). Restore + `chmod +x` it, or register an intentional-gap placeholder `<verb>.sh` emitting `N/A` if the gap is genuinely intended. |
| operations | `BASE_REF=` line missing from a layer's stderr log (resolver did not run; layer-skeleton regression). |
| operations **AND** security | Layer 6 reports `RESULT=FAIL-MISSING-VERB` for an `audit` verb (`<lang>-audit-verb-missing-or-not-executable`). A dependency-vuln scan wrapper is missing/non-executable — a fail-OPEN on a security gate, now caught by the ladder (FAIL-MISSING-VERB outranks the sibling's OK → layer reds at exit 2). Operations: restore `<lang>/audit.sh` + `chmod +x` (or register a placeholder if the gap is intended). Security: confirm whether the lang's dep-audit coverage lapsed in any prior green run and whether a manual scan is warranted. |
| infrastructure | A `scripts/lang/_*.sh` helper or `scripts/lang/<X>/<verb>.sh` wrapper itself is broken (not just reporting a real failure). |
| security | An audit advisory needs an `[advisories.ignore]` entry — policy is security-owned (ADR-0033 §11). |
| security | An advisory's mean-time-to-resolution exceeds **14 days** — tripwire (ADR-0033 §12). |
| protocol | `buf breaking` fires on an intentional wire-break and no override mechanism exists yet (ADR-0033 Wave 3 #10, task #41). |
| infrastructure | The CI `Code Coverage` job reds on **instrumentation / plumbing** (§8.6): empty `show-env`, `--cfg coverage` dropped from the instrumentation flags, `$DT_GUARD` unresolvable to the instrumented binary, the profraw-delta assert firing, or `list-coverage-suites.sh` drift/floor. These are coverage-harness faults, not a defect in the code under test — the external-binary wiring is infrastructure-owned. |
| implementer (consume an attempt) | The CI `Code Coverage` job reds because a **selected bash guard suite genuinely fails** under instrumentation and reproduces locally against a plain debug `dt-guard` (§8.6, first failure row). This is a real test/guard defect, not a harness fault — route it like any other failing suite, do not escalate to infrastructure. |

### Related documentation

- **ADR-0033** (`docs/decisions/adr-0033-polyglot-validation-pipeline.md`) — canonical design spec for the polyglot pipeline. §2/§3 always-run policy + the retired-changed.sh amendments; §4 layer-script contract; §6 wrapper contract + STATUS enum; §7 base-ref resolution.
- **ADR-0030** (`docs/decisions/adr-0030-*.md`) — host-side cluster helper, Layer 7 contract (renumbered from Layer 8).
- **`.claude/skills/devloop/SKILL.md`** — devloop workflow. Step 6 (Fast Check) and Step 7.5 (Gate 2 — Full Validation) are the entry points; pointer back to this runbook lands in Step 7.5.
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
