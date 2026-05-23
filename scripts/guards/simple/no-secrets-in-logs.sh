#!/usr/bin/env bash
# No-Secrets-in-Logs Guard (Rust) — flipped to dt-guard per ADR-0034 §3.
# Full policy logic lives in `crates/dt-guard/src/rust_log_secrets.rs`.
# Consumes `common::pii_vocabulary::PII_TOKENS_CATEGORY_A` canonical SoT.
# shellcheck source=./_dt_guard_wrapper.sh
source "$(dirname "${BASH_SOURCE[0]}")/_dt_guard_wrapper.sh" rust-no-secrets-in-logs
