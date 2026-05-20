#!/usr/bin/env bash
# Alert-Rules Validation Guard (ADR-0031 Prereq #1) — flipped to dt-guard per ADR-0034 §3.
# Full policy logic lives in `crates/dt-guard/src/alert_rules.rs`.
# shellcheck source=./_dt_guard_wrapper.sh
source "$(dirname "${BASH_SOURCE[0]}")/_dt_guard_wrapper.sh" alert-rules-policy "$@"
