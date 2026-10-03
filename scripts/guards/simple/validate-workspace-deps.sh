#!/usr/bin/env bash
# Workspace-Dependency SSoT Guard — dt-guard subcommand.
# Full policy logic lives in `crates/dt-guard/src/workspace_deps.rs`.
# shellcheck source=./_dt_guard_wrapper.sh
source "$(dirname "${BASH_SOURCE[0]}")/_dt_guard_wrapper.sh" workspace-deps
