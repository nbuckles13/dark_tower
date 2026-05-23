#!/usr/bin/env bash
# Cross-Boundary Scope-Drift Guard (Layer A) — flipped to dt-guard per ADR-0034 §3.
# Full policy logic lives in `crates/dt-guard/src/cross_boundary_scope.rs`.
# User-story exemption verbatim per bash; row-level tightening rolled back, see crates/dt-guard/src/cross_boundary_scope.rs:9-19.
# shellcheck source=./_dt_guard_wrapper.sh
source "$(dirname "${BASH_SOURCE[0]}")/_dt_guard_wrapper.sh" cross-boundary-scope
