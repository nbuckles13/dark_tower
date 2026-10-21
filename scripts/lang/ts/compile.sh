#!/usr/bin/env bash
# TS compile: nx run-many -t typecheck (ADR-0033 §6 + §9).
set -euo pipefail
IFS=$'\n\t'
__here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${__here}/../_common.sh"
source "${__here}/../_pnpm.sh"
install_wrapper_exit_trap  # task #50: a set -e abort before run_and_emit must still emit a STATUS
pnpm_deps_fresh || exit 1  # stale node_modules is its own token, not an nx-* failure

# Peer skew (e.g. vitest 5 with @vitest/browser 4, PR #78) is an install error only when pnpm
# RESOLVES (strictPeerDependencies, pnpm-workspace.yaml); a --frozen-lockfile install skips that, so
# a committed skewed lockfile would reach typecheck green. `pnpm peers check` reads the installed
# graph, offline. `|| exit 1`: the dispatcher reads this wrapper's LAST STATUS line, so a FAIL here
# must end the wrapper rather than be followed by nx-typecheck's line.
run_and_emit "pnpm-peers" pnpm peers check || exit 1

run_and_emit "nx-typecheck" pnpm exec nx run-many -t typecheck --all "$NX_PIPELINE_OUTPUT_STYLE" "$@"  # _pnpm.sh
