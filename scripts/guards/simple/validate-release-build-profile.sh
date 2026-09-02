#!/usr/bin/env bash
# Release-Build Premise Guard (ADR-0036 §11 "does it apply") — dt-guard subcommand.
# Full policy logic lives in `crates/dt-guard/src/release_build_profile.rs`.
# shellcheck source=./_dt_guard_wrapper.sh
source "$(dirname "${BASH_SOURCE[0]}")/_dt_guard_wrapper.sh" release-build-profile
