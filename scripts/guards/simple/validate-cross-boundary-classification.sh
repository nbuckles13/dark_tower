#!/usr/bin/env bash
# Cross-Boundary Classification-Sanity Guard (Layer B) — flipped to dt-guard per ADR-0034 §3.
# Full policy logic lives in `crates/dt-guard/src/cross_boundary_classification.rs`.
#
# Invocation modes:
# - Explicit (Gate 1): `validate-cross-boundary-classification.sh <main.md>`
# - Default  (Gate 2): no arg → scans the diff for modified main.md files.
extra_args=()
if [[ $# -ge 1 && -f "$1" && "$1" == *.md ]]; then
    extra_args+=(--main-md "$1")
fi
# shellcheck source=./_dt_guard_wrapper.sh
source "$(dirname "${BASH_SOURCE[0]}")/_dt_guard_wrapper.sh" cross-boundary-classification "${extra_args[@]}"
