#!/usr/bin/env bash
# Rust format: cargo fmt --check (never reformats; check-only per code-reviewer #2).
set -euo pipefail
IFS=$'\n\t'
source "$(dirname "${BASH_SOURCE[0]}")/../_common.sh"
run_and_emit "cargo-fmt" cargo fmt --all -- --check "$@"
