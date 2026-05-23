#!/usr/bin/env bash
# Dashboard-Panels Validation Guard (ADR-0031 Prereq #2) — flipped to dt-guard per ADR-0034 §3.
# Full policy logic lives in `crates/dt-guard/src/dashboard_panels.rs`.
# shellcheck source=./_dt_guard_wrapper.sh
source "$(dirname "${BASH_SOURCE[0]}")/_dt_guard_wrapper.sh" dashboard-panels
