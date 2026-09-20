#!/usr/bin/env bash
# Proto compile: buf build (ADR-0033 §5 stage 1 of layer 1).
#
# No internal skip short-circuit: since 2026-08-20 the dispatcher always-runs every verb
# (the per-lang skip-if-untouched classifier was retired, ADR-0033 §3), so this wrapper
# simply runs whenever its stage is dispatched (INCLUDE for stage 1, EXCLUDE for stage 2).
# All proto wrappers stay structurally uniform — no special-case shape.
#
# No "$@" pass-through to buf — same lockdown as the other buf wrappers
# (uniformity; also avoids --config /tmp/evil-buf.yaml-style attacks against
# the contract-time gate). Future override mechanism is deferred to
# ADR-0033 §10 Wave 3.
set -euo pipefail
IFS=$'\n\t'
source "$(dirname "${BASH_SOURCE[0]}")/../_common.sh"
source "$(dirname "${BASH_SOURCE[0]}")/_buf.sh"
install_wrapper_exit_trap  # task #50: emit STATUS=FAIL if we abort before emitting

# COLLAPSE branch (ADR-0037 §D7): buf via `pnpm exec buf`. The preflight replaces the old bare-`buf`
# `command -v` guard with the four-token taxonomy + the writer==checker version assertion (DiD here:
# compile is read-only, so a version mismatch is degraded feedback, not an escape — but the check is
# uniform across the four wrappers so a stale toolchain reds specifically, not as a build failure).
proto_buf_preflight || exit 1

run_and_emit "buf-build" pnpm exec buf build proto
