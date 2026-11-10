#!/usr/bin/env bash
# Vendored-crate integrity guard — dt-guard subcommand. Each root `[patch.*]`
# path target (vendor/<crate>) is exactly its upstream source plus the recorded
# additive patch in vendor/patches/<crate>/, excluded from the workspace and
# resolved by Cargo.lock. Hermetic: no network, no cargo, no `patch` binary.
# Full policy logic lives in `crates/dt-guard/src/vendor_patches.rs`.
# shellcheck source=./_dt_guard_wrapper.sh
source "$(dirname "${BASH_SOURCE[0]}")/_dt_guard_wrapper.sh" vendor-patches
