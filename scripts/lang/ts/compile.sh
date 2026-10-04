#!/usr/bin/env bash
# TS compile: nx run-many -t typecheck (ADR-0033 §6 + §9).
set -euo pipefail
IFS=$'\n\t'
__here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${__here}/../_common.sh"
source "${__here}/../_pnpm.sh"
install_wrapper_exit_trap  # task #50: a set -e abort before run_and_emit must still emit a STATUS
pnpm_deps_fresh || exit 1  # stale node_modules is its own token, not an nx-* failure


run_and_emit "nx-typecheck" pnpm exec nx run-many -t typecheck --all "$@"
