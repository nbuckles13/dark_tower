#!/usr/bin/env bash
# Proto lint: buf lint (layer 5 entry per ADR-0033 §5).
#
# No "$@" pass-through — same lockdown as the other buf wrappers (uniformity).
set -euo pipefail
IFS=$'\n\t'
source "$(dirname "${BASH_SOURCE[0]}")/../_common.sh"

if ! command -v buf >/dev/null 2>&1; then
  emit_status FAIL "buf-binary-missing"
  exit 1
fi

run_and_emit "buf-lint" buf lint proto
