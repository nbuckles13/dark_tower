#!/usr/bin/env bash
# Bin/Lib Single-Compile Guard — dt-guard subcommand.
# Full policy logic lives in `crates/dt-guard/src/bin_lib_single_compile.rs`.
# shellcheck source=./_dt_guard_wrapper.sh
source "$(dirname "${BASH_SOURCE[0]}")/_dt_guard_wrapper.sh" bin-lib-single-compile
