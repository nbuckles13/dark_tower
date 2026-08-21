#!/usr/bin/env bash
# TS fmt: nx run-many -t format (ADR-0033 §6 + §9).
set -euo pipefail
IFS=$'\n\t'
__here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${__here}/../_common.sh"
install_wrapper_exit_trap  # task #50: a set -e abort before run_and_emit must still emit a STATUS


run_and_emit "nx-format" pnpm exec nx run-many -t format --all "$@"
