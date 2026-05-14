#!/usr/bin/env bash
# Layer 3 — Guards (always-run: no skip-if-untouched).
#
# Wraps existing scripts/guards/run-guards.sh with the STATUS contract.
# Per ADR-0033 implementation note: also invokes _test_changed_predicates.sh
# meta-test so predicate drift is detected on every devloop, not just at PR time.
#
# Failure triage: docs/runbooks/devloop-validation.md §6.3 (Layer 3 + §4 two-token convention).
set -euo pipefail
IFS=$'\n\t'
__here="$(dirname "$0")"
source "${__here}/lang/_common.sh"
layer_lifecycle_begin 3
"${__here}/lang/_get_base_ref.sh" >/dev/null

# Use run_and_emit (dry-reviewer F2) instead of inline if/else — single source
# of truth for STATUS line emission + exit-code mapping.
{
  run_and_emit "guards" "${__here}/guards/run-guards.sh" || true
  run_and_emit "predicate-meta-test" "${__here}/lang/_test_changed_predicates.sh" || true
} | tee_collect_statuses
