#!/usr/bin/env bash
# Insecure-Browser-Settings Guard — mechanises the standing prohibition on
# settings that disable certificate validation, disable web security, or force
# an insecure origin. Full policy logic (and the vocabulary, which is its only
# home) lives in `crates/dt-guard/src/no_insecure_browser_flags.rs`.
# shellcheck source=./_dt_guard_wrapper.sh
source "$(dirname "${BASH_SOURCE[0]}")/_dt_guard_wrapper.sh" no-insecure-browser-flags
