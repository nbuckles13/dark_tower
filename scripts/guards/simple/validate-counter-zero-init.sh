#!/usr/bin/env bash
# Counter zero-init coverage guard (ADR-0036 story-1 counter-visibility fix) —
# dt-guard subcommand per ADR-0034 §3. Full policy logic lives in
# `crates/dt-guard/src/counter_zero_init.rs`.
# shellcheck source=./_dt_guard_wrapper.sh
source "$(dirname "${BASH_SOURCE[0]}")/_dt_guard_wrapper.sh" counter-zero-init
