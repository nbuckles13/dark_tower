#!/usr/bin/env bash
# Proto format lane (ADR-0037 §D7) — COLLAPSE branch: buf via `pnpm exec buf`. APPLY (`buf format -w
# proto`) on an explicit DEVLOOP_FMT_APPLY opt-in; CHECK (`buf format --diff --exit-code proto`) in CI /
# at attesting gates / by default. Lane decided ONCE by fmt_mode() in _common.sh.
#
# Naming: fmt.sh (not format.sh) matches the dispatcher verb and the lang/rust/fmt.sh precedent (ADR-0033
# §1's "format.sh" is fixed to "fmt.sh" in this diff). Shape lockdown kept: explicit `proto` target, no
# "$@" pass-through, both branches through run_and_emit/emit_status, install_wrapper_exit_trap.
#
# ┌─ GSA WRITE — ACCEPTED RISK (human ruling, 2026-09-20) ──────────────────────────────────────────────┐
# │ This lane runs `buf format -w proto` on local (opt-in) runs, so the validation pipeline WRITES to     │
# │ proto/**, a Guarded Shared Area (ADR-0024 §6.4, wire format). Deliberate ruling, NOT an oversight:    │
# │ the implementer recommended proto stay check-only (a carve-out) and was OVERRIDDEN in favour of        │
# │ completing the fmt invariant across every lane (ADR-0037 §D7/§D9). Protocol + security co-signed the   │
# │ write under the conditions below. Recorded as ACCEPTED, not resolved.                                 │
# │                                                                                                        │
# │ WHY A PIPELINE MAY WRITE WIRE-CONTRACT SOURCE — the bounding invariant. CI runs this lane with         │
# │ --diff --exit-code, so COMMITTED proto is always already formatted. A local apply can therefore only  │
# │ rewrite UNCOMMITTED proto edits — i.e. proto THIS devloop touched, so @protocol is already in the      │
# │ loop by §6.4. The auto-write CANNOT reach proto this devloop did not touch. This holds ONLY while      │
# │ writer==checker (same buf version) — the preflight version assertion below is what makes it true.      │
# │                                                                                                        │
# │ WHY IT IS TOLERABLE — each CHECKABLE; if one stops holding this acceptance no longer covers the risk:  │
# │   1. ONE buf version everywhere, from the lockfile: `pnpm exec buf` resolves @bufbuild/buf, exact-      │
# │      pinned in package.json (no range) and sha512-pinned for the platform binary in pnpm-lock.yaml.    │
# │   2. Every CI install path uses `pnpm install --frozen-lockfile`, so CI cannot resolve a version the   │
# │      lockfile does not record.                                                                         │
# │   3. The preflight version assertion (_buf.sh: `pnpm exec buf --version` == lockfile pin, EXACT, fail- │
# │      closed derivation) catches a stale local node_modules. It runs BEFORE the format check, so a      │
# │      stale writer reds as `buf-version-mismatch` (remedy: pnpm install), not `buf-format-drift`.       │
# │   4. Tree-attesting gates run this lane CHECK-only. The lane mode DEFAULTS to CHECK; APPLY requires an  │
# │      explicit DEVLOOP_FMT_APPLY opt-in, and the gates set DEVLOOP_FMT_CHECK_ONLY, which OUT-RANKS that  │
# │      opt-in in the precedence (INVALID > check-only-override > CI > apply-opt-in > CHECK-default). So   │
# │      no gate rewrites the wire contract it is attesting, even if an apply opt-in is ambient.           │
# │   5. `buf format` is layout-only — it cannot alter field numbers, types, names, reserved ranges or     │
# │      options. A formatter change cannot alter the wire contract, only bytes that remain under review.  │
# │                                                                                                        │
# │ WHAT INVALIDATES THIS ACCEPTANCE — any one of: a range operator on the pin or a platform dep; an       │
# │   install path (CI or local) skipping --frozen-lockfile; the version assertion being weakened,         │
# │   removed, REORDERED after the format check, or relaxed from exact equality to a substring match; an   │
# │   attesting gate acquiring apply mode, or the precedence being reordered so the apply opt-in out-ranks  │
# │   the CI signal or the check-only override; a @bufbuild/buf bump reaching the writer without a          │
# │   reviewed bump; buf gaining the ability to alter proto semantics rather than layout.                  │
# │                                                                                                        │
# │ A @bufbuild/buf BUMP IS A GSA EVENT, never routine. dependabot.yml runs npm weekly and GROUPS          │
# │ minor/patch bumps; @bufbuild/buf is excluded from that group (`exclude-patterns`) so a buf bump is its │
# │ own reviewable PR, carrying any whole-tree reformat and a regenerated golden fixture reviewed AS the   │
# │ evidence of what the new formatter changed. Treat it as protocol + security, never a Tuesday.          │
# │                                                                                                        │
# │ FORMATTER VERSIONS GENUINELY DISAGREE — measured, not theoretical. buf 1.50.0 and 1.72.0 format an     │
# │ `option` block followed by a leading comment differently. Committed proto conforms to both, so         │
# │ convergence was zero-churn — but that is the tree happening to sit in the intersection, NOT the         │
# │ versions agreeing. A bump can silently start rewriting proto/**; that is why a bump is reviewed.        │
# └────────────────────────────────────────────────────────────────────────────────────────────────────┘
set -euo pipefail
IFS=$'\n\t'
__here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${__here}/../_common.sh"
source "${__here}/_buf.sh"
install_wrapper_exit_trap  # task #50: emit STATUS=FAIL if we abort before emitting

# PRECONDITION (before any buf call): toolchain present + writer==checker version. Four-token taxonomy
# lives in _buf.sh; this precedes the format check so a stale writer reds as version-mismatch, not drift.
proto_buf_preflight || exit 1

__verdict="$(fmt_mode)"
fmt_mode_emit "$__verdict"
__mode="${__verdict%% *}"

case "$__mode" in
  INVALID)
    emit_status FAIL "fmt-mode-invalid-knob"
    exit 1
    ;;
  CHECK)
    run_and_emit "buf-format" pnpm exec buf format --diff --exit-code proto
    ;;
  APPLY)
    # rc mapping is TOOL-SPECIFIC and the INVERSE of rust's (buf 100=drift, 1=parse) — do NOT copy rust's
    # arms. Measured on buf 1.72.0 (the version pnpm exec resolves): check `--diff --exit-code` →
    # clean rc=0 / drift rc=100 (unified diff on stdout) / parse-error rc=1 (Failure: file:line on stderr).
    # apply `-w` → rc=1 + NOTHING written on a parse error (batch-level fail-closed).
    __err="$(mktemp)"
    set +e
    __diff="$(pnpm exec buf format --diff --exit-code proto 2>"$__err")"
    __rc=$?
    set -e
    if [[ "$__rc" -eq 0 ]]; then
      rm -f "$__err"
      emit_status OK "buf-format-passed"
    elif [[ "$__rc" -eq 100 ]]; then
      # Drift. The changed-file list comes from the PRE-PASS diff headers (`+++ <path>`) — apply's rc
      # carries no list. Paths are repo-root-relative from the diff already — this holds ONLY BECAUSE the
      # lane passes the hardcoded RELATIVE `proto` target (above): buf echoes paths relative to that target,
      # so a relative target yields repo-relative `+++` paths. Do NOT "harden" the target to an absolute path
      # — buf would then emit ABSOLUTE paths and leak $HOME into the committed docs/devloop-outputs records
      # (@security). Unlike rust/fmt.sh (which relativizes actively via `git rev-parse --show-toplevel`),
      # proto relies on this invocation-shape precondition; keep the target relative.
      __applied="$(printf '%s\n' "$__diff" | sed -n 's/^+++ //p' | sed 's/\t.*//' | sort -u | paste -sd, -)"
      if ! pnpm exec buf format -w proto; then
        cat "$__err" >&2
        rm -f "$__err"
        emit_status FAIL "buf-format-failed"
        exit 1
      fi
      # POST-CONDITION (@paired-protocol): apply rc=0 regardless of what it wrote, so re-run the check and
      # assert clean (rc=0 AND empty diff) — catches "rewrote some but not all". Post-check drift → FAIL,
      # never -applied.
      set +e
      __post="$(pnpm exec buf format --diff --exit-code proto 2>/dev/null)"
      __prc=$?
      set -e
      rm -f "$__err"
      if [[ "$__prc" -ne 0 || -n "$__post" ]]; then
        emit_status FAIL "buf-format-postcondition-failed"
        exit 1
      fi
      # LOUD (GSA rewrite): a WARN anchor `grep 'WARN '` finds in a green log — proto is the only lane
      # whose auto-apply rewrites a Guarded Shared Area, so it gets a louder notice than rust/TS.
      printf 'WARN PROTO_FMT_APPLIED FILES=%s\n' "${__applied:-<list-unavailable>}" >&2
      printf 'FMT_APPLIED=%s\n' "${__applied:-<list-unavailable>}"
      emit_status OK "buf-format-applied"
    elif [[ "$__rc" -eq 1 ]]; then
      # Parse error (rc=1). Surface buf's Failure: line VERBATIM (it carries file:line:col) but NEVER
      # match on the diagnostic WORDING — it changed across the 1.50→1.72 upgrade; only rc + the
      # `Failure:` prefix are stable. Do not apply.
      cat "$__err" >&2
      rm -f "$__err"
      emit_status FAIL "buf-format-parse-error"
      exit 1
    else
      cat "$__err" >&2
      rm -f "$__err"
      emit_status FAIL "buf-format-failed"
      exit 1
    fi
    ;;
  *)
    # Defense-in-depth (obs F1): fmt_mode returns only INVALID/CHECK/APPLY today, but an unrecognized verdict
    # must NOT fall out of the case with no STATUS — the EXIT trap would then report
    # `wrapper-aborted-early-exit-0`, a confidently-wrong diagnosis. Fail closed + loud with the accurate
    # token, per layer-all.sh:140-144's fail_fast_mode precedent.
    emit_status FAIL "fmt-mode-unknown-verdict"
    exit 1
    ;;
esac
