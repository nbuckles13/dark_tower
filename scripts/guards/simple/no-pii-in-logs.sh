#!/usr/bin/env bash
# No-PII-in-Logs Guard (Rust) — flipped to dt-guard per ADR-0034 §3.
# Full policy logic lives in `crates/dt-guard/src/rust_pii.rs`.
# Consumes `common::pii_vocabulary::PII_TOKENS_CATEGORY_B` canonical SoT.
# shellcheck source=./_dt_guard_wrapper.sh
source "$(dirname "${BASH_SOURCE[0]}")/_dt_guard_wrapper.sh" rust-no-pii-in-logs
