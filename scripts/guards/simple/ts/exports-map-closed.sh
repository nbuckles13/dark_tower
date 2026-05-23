#!/usr/bin/env bash
# Exports-Map Closed-World Guard — flipped to dt-guard per ADR-0034 §3.
# Full policy logic lives in `crates/dt-guard/src/ts_exports_map.rs`.
#
# Per @paired-client F4: literal "1" semantics for STRICT_EXPORTS_MAP — no
# generic-truthy widening (matches bash today's `[[ "$STRICT" == "1" ]]`).
extra_args=()
[[ "${STRICT_EXPORTS_MAP:-0}" == "1" ]] && extra_args+=(--strict)
# shellcheck source=../_dt_guard_wrapper.sh
source "$(dirname "${BASH_SOURCE[0]}")/../_dt_guard_wrapper.sh" ts-exports-map-closed "${extra_args[@]}"
