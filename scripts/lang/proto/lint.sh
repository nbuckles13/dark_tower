#!/usr/bin/env bash
# Proto lint: buf lint (layer 5 entry per ADR-0033 §5).
#
# No "$@" pass-through — same lockdown as the other buf wrappers (uniformity).
set -euo pipefail
IFS=$'\n\t'
source "$(dirname "${BASH_SOURCE[0]}")/../_common.sh"
source "$(dirname "${BASH_SOURCE[0]}")/_buf.sh"
install_wrapper_exit_trap  # task #50: emit STATUS=FAIL if we abort before emitting

# COLLAPSE branch (ADR-0037 §D7): buf via `pnpm exec buf`; the preflight replaces the bare-`buf`
# `command -v` guard (four-token taxonomy + version assertion; DiD — lint is read-only).
proto_buf_preflight || exit 1

run_and_emit "buf-lint" pnpm exec buf lint proto
