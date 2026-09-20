#!/usr/bin/env bash
# Rust format lane (ADR-0037 §D7): APPLY locally (only on an explicit DEVLOOP_FMT_APPLY opt-in, under
# the check-default inversion), CHECK in CI / at attesting gates / by default. The apply-vs-check LANE
# is decided ONCE by fmt_mode() in _common.sh; this wrapper only ACTS on the verdict.
#
# REVERSES the earlier "check-only per code-reviewer #2" choice, FOR APPLY CONTEXTS ONLY: `cargo fmt`
# is a deterministic, semantically-empty transform, so auto-applying it locally masks nothing (unlike
# a lint autofix, which could). CHECK contexts stay read-only and fail loud.
#
# S-12 (security): with APPLY on, layer 2 rewrites the tree mid-pipeline, so the pipeline no longer
# validates ONE frozen tree (layer 1 compiled pre-format source; later layers + the Gate-2 verdict see
# post-format). That is acceptable ONLY because the transform is semantically empty — which is exactly
# why a `clippy --fix` could NOT reason "fmt auto-applies mid-run, so I may too."
set -euo pipefail
IFS=$'\n\t'
source "$(dirname "${BASH_SOURCE[0]}")/../_common.sh"
install_wrapper_exit_trap  # task #50: emit STATUS=FAIL if we abort before emitting

__verdict="$(fmt_mode)"
fmt_mode_emit "$__verdict"          # greppable FMT_MODE=/SOURCE= anchor on stderr
__mode="${__verdict%% *}"

case "$__mode" in
  INVALID)
    # Misconfigured DEVLOOP_FMT_{APPLY,CHECK_ONLY}. A config/wiring fault, marked as such. Kept a
    # leaf-wrapper STATUS=FAIL/exit 1 (not an exit-2 enum): the dispatcher re-derives from the STATUS
    # line, and no exit-2 enum cleanly means "config-invalid knob".
    emit_status FAIL "fmt-mode-invalid-knob"
    exit 1
    ;;
  CHECK)
    # Plain --check, NO -l: CI needs the `Diff in <file>` hunks for legibility (-l would print only paths).
    # Reciprocal of .githooks/pre-commit's inline `cargo fmt --all -- --check`: both are check-only tree
    # attestations and must stay in sync — keep this form and the hook's identical (the fmt-lane-ssot guard
    # enforces both stay check-only). See the pre-commit hook's cross-reference back to this branch.
    run_and_emit "cargo-fmt" cargo fmt --all -- --check "$@"
    ;;
  APPLY)
    # A `--check -l` pre-pass yields the EXACT mismatched-file list (rustfmt is silent on apply), then
    # apply. Four arms from the verified rustfmt 1.9.0 behavior:
    #   rc=0            -> already clean (skip the 2nd pass)
    #   rc=1, non-empty -> drift; that list is what `cargo fmt --all` will rewrite
    #   rc=1, EMPTY     -> rustfmt PARSE error (goes to stderr); do NOT apply
    #   rc>=2           -> rustfmt internal error; do NOT apply
    # `cargo fmt --all` is itself fail-closed on a parse error (verified: rc=1, file unchanged), so the
    # apply is a backstop even if this pre-pass model ever drifted from a future rustfmt.
    __root="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
    __preerr="$(mktemp)"
    set +e
    __list="$(cargo fmt --all -- --check -l "$@" 2>"$__preerr")"
    __rc=$?
    set -e
    if [[ "$__rc" -eq 0 ]]; then
      rm -f "$__preerr"
      emit_status OK "cargo-fmt-passed"
    elif [[ "$__rc" -eq 1 && -n "$__list" ]]; then
      # Relativize to repo-root (rustfmt may print absolute paths — never leak $HOME into the committed
      # layer-2.log / devloop-outputs records) and comma-join for the greppable FMT_APPLIED= anchor.
      __applied="$(printf '%s\n' "$__list" | while IFS= read -r __f; do
        [[ -z "$__f" ]] && continue; printf '%s\n' "${__f#"$__root"/}"; done | paste -sd, -)"
      rm -f "$__preerr"
      if cargo fmt --all "$@"; then
        # Distinct REASON so an operator can tell "auto-fix rewrote the tree" from "already clean".
        printf 'FMT_APPLIED=%s\n' "${__applied:-<list-unavailable>}"
        emit_status OK "cargo-fmt-applied"
      else
        emit_status FAIL "cargo-fmt-failed"
        exit 1
      fi
    elif [[ "$__rc" -eq 1 ]]; then
      # rc=1 with an EMPTY list == a rustfmt parse error, NOT drift. Do not apply; surface the parse
      # error (it carries file:line) and fail with a DISTINCT token, never a blank FMT_APPLIED=.
      cat "$__preerr" >&2
      rm -f "$__preerr"
      emit_status FAIL "cargo-fmt-unparseable"
      exit 1
    else
      cat "$__preerr" >&2
      rm -f "$__preerr"
      emit_status FAIL "cargo-fmt-failed"
      exit 1
    fi
    ;;
  *)
    # Defense-in-depth (obs F1): fmt_mode returns only INVALID/CHECK/APPLY today, but an unrecognized verdict
    # must NOT fall out of the case with no STATUS — the EXIT trap would then report
    # `wrapper-aborted-early-exit-0`, a confidently-wrong diagnosis ("crashed before emitting" when the truth
    # is "verdict unrecognized"). Fail closed + loud with the accurate token, per layer-all.sh:140-144's
    # fail_fast_mode precedent ("fail-closed, loud, per 'fail loudly; never mask'").
    emit_status FAIL "fmt-mode-unknown-verdict"
    exit 1
    ;;
esac
