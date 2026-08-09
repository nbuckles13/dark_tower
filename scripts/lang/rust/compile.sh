#!/usr/bin/env bash
# Rust compile (per @team-lead 2026-05-19, ADR-0034 Bundle 1):
#   1. Workspace debug build — catches link-time errors `cargo check` misses.
#   2. dt-guard release build — produces `target/release/dt-guard` for the
#      ADR §3 wrappers at `scripts/guards/simple/*.sh` which invoke
#      `${DT_GUARD:-$REPO_ROOT/target/release/dt-guard}`.
#   3. dt-story release build — produces `target/release/dt-story` for the
#      Layer-3 `validate-story-manifest.sh` guard (story-runner manifest
#      schema check). Same build-once-before-guards pattern as dt-guard.
# Release is incremental on top of debug (~5-15s cold, ~0s warm with sccache
# per ADR-0034 §Negative).
set -euo pipefail
IFS=$'\n\t'
source "$(dirname "${BASH_SOURCE[0]}")/../_common.sh"
install_wrapper_exit_trap  # task #50: emit STATUS=FAIL if we abort before emitting
run_and_emit "cargo-build" cargo build --workspace --quiet "$@"
run_and_emit "cargo-build-dt-guard" cargo build --release -p dt-guard --quiet "$@"
run_and_emit "cargo-build-dt-story" cargo build --release -p dt-story --quiet "$@"
