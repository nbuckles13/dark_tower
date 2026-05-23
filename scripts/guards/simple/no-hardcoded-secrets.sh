#!/usr/bin/env bash
# No-Hardcoded-Secrets Guard (Rust) — flipped to dt-guard per ADR-0034 §3.
# Full policy logic lives in `crates/dt-guard/src/rust_secrets.rs`.
# Closes the ADR-0034 §6 cross-stack HYGIENE_PATTERNS dupe.
# shellcheck source=./_dt_guard_wrapper.sh
source "$(dirname "${BASH_SOURCE[0]}")/_dt_guard_wrapper.sh" rust-no-hardcoded-secrets
