#!/usr/bin/env bash
# Dev-tool binary path home guard — dt-guard subcommand.
# Full policy logic lives in `crates/dt-guard/src/tool_bin_paths.rs`.
# shellcheck source=./_dt_guard_wrapper.sh
source "$(dirname "${BASH_SOURCE[0]}")/_dt_guard_wrapper.sh" tool-bin-paths
