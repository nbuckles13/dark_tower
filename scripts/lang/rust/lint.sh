#!/usr/bin/env bash
# Rust lint: cargo clippy with -D warnings (matches existing rust standard).
set -euo pipefail
IFS=$'\n\t'
source "$(dirname "${BASH_SOURCE[0]}")/../_common.sh"
install_wrapper_exit_trap  # task #50: emit STATUS=FAIL if we abort before emitting
run_and_emit "cargo-clippy" cargo clippy --workspace --all-targets "$@" -- -D warnings
