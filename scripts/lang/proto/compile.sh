#!/usr/bin/env bash
# Proto compile: buf build (ADR-0033 §5 stage 1 of layer 1).
#
# No internal changed.sh short-circuit: the dispatcher handles skip-if-untouched
# uniformly for both stages of layer1 (INCLUDE for stage 1, EXCLUDE for stage 2).
# All proto wrappers stay structurally uniform — no special-case shape.
#
# No "$@" pass-through to buf — same lockdown as the other buf wrappers
# (uniformity; also avoids --config /tmp/evil-buf.yaml-style attacks against
# the contract-time gate). Future override mechanism is deferred to
# ADR-0033 §10 Wave 3.
set -euo pipefail
IFS=$'\n\t'
source "$(dirname "${BASH_SOURCE[0]}")/../_common.sh"

if ! command -v buf >/dev/null 2>&1; then
  emit_status FAIL "buf-binary-missing"
  exit 1
fi

run_and_emit "buf-build" buf build proto
