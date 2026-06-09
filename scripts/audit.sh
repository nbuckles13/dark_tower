#!/usr/bin/env bash
# Audit dispatcher: invokes lang/<X>/audit.sh always-run (ADR-0033 §3 + §6),
# then invokes proto's lang/proto/breaking.sh unconditionally per §10:397.
#
# Proto deliberately has no audit.sh (§1:96 + §6:236) — breaking.sh IS the
# proto audit gate, wired here (not via thin wrapper) so dispatch emits the
# §6 SKIPPED-NO-VERB signal for proto and breaking.sh runs independently.
set -euo pipefail
IFS=$'\n\t'
source "$(dirname "$0")/lang/_dispatch.sh"

dispatch_rc=0
# Capture the dispatch output to a tempfile while streaming it VERBATIM to stdout, so
# the layer's `audit.sh 2>&1 | tee_collect_statuses` (layer6.sh) still sees every STATUS
# line exactly once, AND we can scan the captured copy below. (A `tee /dev/stderr` would
# double the lines into the layer's 2>&1 collector — corrupting its aggregation — so use
# a tempfile, not a second stream.) The pipe would mask the dispatcher's rc, so capture
# it via PIPESTATUS.
__audit_out=$(mktemp)
trap 'rm -f "$__audit_out"' EXIT
DEVLOOP_DISPATCH_ALWAYS_RUN=1 for_each_lang_with_verb "audit" "$@" | tee "$__audit_out"
dispatch_rc=${PIPESTATUS[0]}

# SECURITY fail-closed (task #50, security finding; Lead ruling — fix, not defer): the
# cross-lang-masking residual (docs/TODO.md) lets an UNEXPECTED audit verb-missing be
# masked by a sibling lang's OK — they share the rank-0 SKIPPED-NO-VERB enum and
# `aggregate_worst_status` keeps OK (rank 2) on top, so the dep-vuln scan for the deleted
# lang silently never runs while the layer stays green. The general (non-audit) tail is
# deferred (ladder re-rank, large blast radius); the AUDIT slice is a SECURITY fail-open
# and is closed here WITHOUT touching the ladder.
#
# CRITICAL: layer6.sh runs `audit.sh 2>&1 | tee_collect_statuses`, and
# __layer_lifecycle_end keys the LAYER exit on the collected STATUS stream (the aggregate
# enum), NOT on audit.sh's exit code (the pipe discards it). So forcing only audit.sh's
# own rc would be re-masked by the layer's OK aggregate. We must EMIT a STATUS=FAIL line
# so tee_collect_statuses collects FAIL → layer aggregate = FAIL → layer exits 1, AND
# fold non-zero into dispatch_rc for the standalone `scripts/audit.sh` path.
#
# Match on the shared DEVLOOP_UNEXPECTED_VERB_SUFFIX constant (no new literal) — it never
# matches proto's intentional `-sh-missing-or-not-executable` gap, so proto stays exit 0.
__unexpected_audit_langs=$(
  grep -oE "[a-zA-Z0-9_]+-audit${DEVLOOP_UNEXPECTED_VERB_SUFFIX}" "$__audit_out" \
    | sed -E "s/-audit${DEVLOOP_UNEXPECTED_VERB_SUFFIX}\$//" | sort -u
)
if [[ -n "$__unexpected_audit_langs" ]]; then
  while IFS= read -r __lang; do
    [[ -n "$__lang" ]] || continue
    # Emit a real FAIL line so the LAYER aggregate reds (the load-bearing part), with a
    # token naming the offending lang for triage.
    emit_status FAIL "audit-gate-wrapper-missing-${__lang}"
    echo "SECURITY: ${__lang}/audit.sh is missing/non-executable (UNEXPECTED) — its dependency-vuln scan did NOT run. Restore the wrapper + chmod +x, or add ${__lang}:audit to the intentional-gap allowlist if deliberate. Emitting STATUS=FAIL so the layer fails closed despite the OK aggregate." >&2
  done <<< "$__unexpected_audit_langs"
  # Exit code — FAIL / exit 1 on BOTH paths (Lead ruling, confirmed 2026-06-09). We emit
  # STATUS=FAIL, so the IN-PIPELINE path (layer6.sh `audit.sh 2>&1 | tee_collect_statuses`,
  # which keys the layer exit on the STATUS stream) aggregates FAIL → exit 1. Match the
  # STANDALONE `./scripts/audit.sh` path to it: dispatch_rc=1. FAIL ("a security gate that
  # should have run did not execute") is the honest enum; we deliberately do NOT emit a
  # synthetic STATUS=UNKNOWN to push the layer to exit 2 — UNKNOWN is documented (runbook
  # §3) as "no status emitted / child crashed before STATUS", and a precise-REASON UNKNOWN
  # would corrupt that meaning. Both paths agree at exit 1 (cleaner than a standalone=2 /
  # layer=1 divergence); non-zero is fail-closed, and layer-all folds any non-zero →
  # final_exit=1.
  [[ "$dispatch_rc" -lt 1 ]] && dispatch_rc=1
fi

breaking_rc=0
"$(dirname "$0")/lang/proto/breaking.sh" || breaking_rc=$?

# Worst exit code wins. Numeric max is correct because status_to_exit_code()
# maps 0=OK/SKIPPED, 1=FAIL, 2=UNKNOWN — monotonic, so max == worst per the
# STATUS-precedence contract. A `set -e` short-circuit would mask the second
# invocation and break the always-run guarantee; explicit RC capture preserves
# both gates.
exit "$(( dispatch_rc > breaking_rc ? dispatch_rc : breaking_rc ))"
