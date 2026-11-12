#!/usr/bin/env bash
# Rust lint: cargo clippy with -D warnings (matches existing rust standard).
set -euo pipefail
IFS=$'\n\t'
source "$(dirname "${BASH_SOURCE[0]}")/../_common.sh"
install_wrapper_exit_trap  # task #50: emit STATUS=FAIL if we abort before emitting
# --all-features: parity with .githooks/pre-commit's clippy step. env-tests has
# no default features and every test target is feature-gated, so without it no
# layer below 7 type-checked them (docs/TODO.md gap filed 2026-09-08; closed by
# story 4 P3). CONSTRAINT: --all-features enables mc-service `test-seams` and
# mh-service `per-frame-trace`, whose compile_error! fires without
# debug_assertions — "$@" must NEVER carry --release or a release-inheriting
# --profile.
run_and_emit "cargo-clippy" cargo clippy --workspace --all-targets --all-features "${CARGO_LOCKED[@]}" "$@" -- -D warnings
