#!/usr/bin/env bash
# TypeScript format lane (ADR-0037 §D7): APPLY locally (only on an explicit DEVLOOP_FMT_APPLY opt-in,
# under the check-default inversion), CHECK in CI / at attesting gates / by default. The apply-vs-check
# LANE is decided ONCE by fmt_mode() in _common.sh — the SAME single home the rust and proto lanes
# consume; this wrapper only ACTS on the verdict (no bespoke TS gating, no reading the knob directly).
#
# WHAT THIS REPLACES: the prior wrapper ran `nx run-many -t format --all`, but NO TypeScript project had
# a `format` target — the only one was proto-gen's `buf format`. So the "TS fmt layer" formatted ZERO
# TypeScript and re-ran proto's check (double-format), reporting STATUS=OK over an empty set every run.
# TS was actually format-checked from the `lint` lane via a `prettier --check` clause duplicated across
# four packages. This lane now really formats TS: real per-package `format` targets run prettier over
# each package's sources, and the duplicated lint clauses are removed (prettier's single home is here).
#
# PATHS ARE REPO-ROOT-RELATIVE BY CONSTRUCTION: the `format` targets omit nx's `cwd` (so nx runs them
# from the workspace root) and use repo-relative globs, so prettier emits `packages/<pkg>/...` paths.
# That (a) makes FMT_APPLIED= comparable to the rust/proto lanes' repo-relative contract and safe to
# transcribe into committed devloop records, and (b) resolves the ROOT .prettierignore (prettier reads
# it relative to CWD and does NOT walk up) — which is why packages/sdk-core/.prettierignore (a cwd=package
# workaround that duplicated the root proto-ignore rule) was deleted in this same change.
#
# NO "$@" PASS-THROUGH (same lockdown as the proto buf wrappers): the only flag this lane forwards to
# prettier is the mode flag it computes (--check / --write --list-different). Splicing arbitrary caller
# args into `nx ... -- <flag> $@` → prettier is a footgun, and no caller passes meaningful fmt args.
set -euo pipefail
IFS=$'\n\t'
__here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${__here}/../_common.sh"
install_wrapper_exit_trap  # task #50: a set -e abort before we emit a STATUS must still emit one

__verdict="$(fmt_mode)"
fmt_mode_emit "$__verdict"          # greppable FMT_MODE=/SOURCE= anchor on stderr (shared emitter)
__mode="${__verdict%% *}"

# POSITIVE CONTROL (review-protocol §Assertion-Vacuity #1/#5): the original defect was a control that
# reported OK over an EMPTY set. `nx run-many -t format --all` exits 0 when ZERO projects match the
# target — indistinguishable from a real clean pass. So before dispatching, assert the `format` target
# exists on >=1 project; if not, FAIL LOUD with a DISTINCT token (never a silent green over nothing).
# `nx show projects --with-target format --json` prints a JSON array; a populated array contains a
# quoted name, `[]` (or an error) does not — so "no quote" == zero targets. Shared by the CHECK and
# APPLY arms; the INVALID / unknown-verdict arms exit before reaching it (no nx call on those paths).
__assert_format_targets() {
  local wt
  wt="$(pnpm exec nx show projects --with-target format --json 2>/dev/null || true)"
  printf '%s' "$wt" | grep -q '"' && return 0
  emit_status FAIL "no-ts-format-targets"
  exit 1
}

# Dispatch on the fmt_mode verdict. A `case` with an EXPLICIT `APPLY` arm and a FAIL-CLOSED `*)` — NOT
# `if CHECK … else <APPLY>` (obs F1): APPLY is the one branch that WRITES the tree, so it must be
# reached only by an explicit `APPLY` match, never as the fall-through for "anything not CHECK". A
# future verdict token, a typo in fmt_mode, or a half-finished refactor must fail LOUD, not auto-write —
# the same apply-as-fallthrough shape the check-default inversion removed from fmt_mode() itself.
case "$__mode" in
  INVALID)
    # Misconfigured DEVLOOP_FMT_{APPLY,CHECK_ONLY}. Config/wiring fault, marked as such — a leaf-wrapper
    # STATUS=FAIL/exit 1 (the dispatcher re-derives from the STATUS line). Matches the rust lane.
    emit_status FAIL "fmt-mode-invalid-knob"
    exit 1
    ;;
  CHECK)
    __assert_format_targets
    # Plain --check: prettier lists offending files as `[warn] <repo-relative path>` and exits non-zero,
    # which nx propagates. run_and_emit maps that to nx-format-passed / nx-format-failed.
    run_and_emit "nx-format" pnpm exec nx run-many -t format --all -- --check
    ;;
  APPLY)
    __assert_format_targets
    # `--write --list-different` applies AND prints ONLY the changed files (empty on a clean tree), so
    # FMT_APPLIED= names exactly what was rewritten. `--output-style=stream-without-prefixes` strips
    # nx's per-task banner so the changed-file lines are the raw prettier output.
    set +e
    __out="$(pnpm exec nx run-many -t format --all --output-style=stream-without-prefixes -- --write --list-different 2>&1)"
    __rc=$?
    set -e
    printf '%s\n' "$__out"   # preserve nx/prettier output in the layer log
    if [[ "$__rc" -ne 0 ]]; then
      emit_status FAIL "nx-format-failed"
      exit 1
    fi
    # Extract the changed-file list: whole-line, repo-relative, .ts/.svelte. The `> prettier "<glob>" ...`
    # command-echo lines start with `> ` and carry quotes, so `^packages/[^"]*\.(ts|svelte)$` excludes
    # them; `tr -d '\r'` guards a trailing CR on the echo lines from defeating the `$` anchor.
    # `|| true` is load-bearing: on a CLEAN tree prettier changes nothing, so grep matches nothing and
    # exits 1 — under `set -e` + pipefail (from _common.sh) that would abort the wrapper before it emits
    # a STATUS (the EXIT trap would then report wrapper-aborted). An empty list is NOT self-evidently
    # "clean" — see the environment-sanity check below.
    __applied="$(printf '%s\n' "$__out" | tr -d '\r' \
      | grep -E '^packages/[^"]*\.(ts|svelte)$' | sort -u | paste -sd, - || true)"
    if [[ -n "$__applied" ]]; then
      # Distinct REASON so an operator can tell "auto-fix rewrote the tree" from "already clean".
      printf 'FMT_APPLIED=%s\n' "$__applied"
      emit_status OK "nx-format-applied"
    elif printf '%s\n' "$__out" | grep -qE '(^|[[:space:]])prettier[[:space:]].*--write'; then
      # ENVIRONMENT-SANITY (obs F2, §Assertion-Vacuity #5): an EMPTY changed-file list is ambiguous —
      # it means "clean" ONLY if the extraction actually observed prettier's output. `--write
      # --list-different` gives TS no independent drift signal (unlike rust's pre-pass rc=1 or proto's
      # rc=100), so if nx's output shape moves (a flag rename, an output-style change), the grep yields
      # nothing on a run that DID rewrite files. We resolve the ambiguity toward CLEAN only when the
      # prettier `--write` invocation is present in the output (proof the extraction read a real run).
      # This also keeps FMT_APPLIED= trustworthy for the wireConstants.drift.test.ts discriminator that
      # consumes it.
      emit_status OK "nx-format-passed"
    else
      # Empty list AND no recognizable prettier `--write` invocation in the output → the extraction had
      # nothing trustworthy to read (nx output shape changed). Do NOT report "clean" over an unobserved
      # set — FAIL LOUD with a distinct token (the shape of rust's rc>=2 / proto's post-condition arm).
      #
      # DELIBERATE FALSE-RED DIRECTION (obs, do NOT "simplify away"): if nx ever stops echoing
      # `prettier … --write`, a genuinely CLEAN run also lands here — a noisy false red. That is the
      # INTENDED trade: a false red is loud and gets fixed; the alternative (a silent green over an
      # unobserved set) is the pre-fix vacuity bug and also breaks the FMT_APPLIED contract the
      # wireConstants discriminator consumes. On a mysterious red here, FIX the marker to match nx's new
      # echo — do NOT delete this arm. (Same rationale as the fail-closed INVALID knob: loud for
      # legibility, kept even though it "can't cause harm".)
      emit_status FAIL "nx-format-output-unrecognized"
      exit 1
    fi
    ;;
  *)
    # FAIL-CLOSED, LOUD (obs F1; `layer-all.sh:140-144` is the in-tree precedent for fail_fast_mode's
    # verdict). Reaching here means fmt_mode returned a token this wrapper does not recognize.
    emit_status FAIL "fmt-mode-unknown-verdict"
    exit 1
    ;;
esac
