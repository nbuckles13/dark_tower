#!/usr/bin/env bash
# Rust compile: cargo check across the workspace.
set -euo pipefail
IFS=$'\n\t'
source "$(dirname "${BASH_SOURCE[0]}")/../_common.sh"
run_and_emit "cargo-check" cargo check --workspace --quiet "$@"
