#!/usr/bin/env bash
# Test Registration Guard — flipped to dt-guard per ADR-0034 §3.
# Full policy logic lives in `crates/dt-guard/src/test_registration.rs`.
# shellcheck source=./_dt_guard_wrapper.sh
source "$(dirname "${BASH_SOURCE[0]}")/_dt_guard_wrapper.sh" test-registration
