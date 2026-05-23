#!/usr/bin/env bash
# No-Test-Removal TS Guard — flipped to dt-guard per ADR-0034 §3.
# Full policy logic lives in `crates/dt-guard/src/ts_test_removal.rs`.
# shellcheck source=../_dt_guard_wrapper.sh
source "$(dirname "${BASH_SOURCE[0]}")/../_dt_guard_wrapper.sh" ts-no-test-removal
