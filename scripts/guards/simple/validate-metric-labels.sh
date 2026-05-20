#!/usr/bin/env bash
# Metric-Labels Validation Guard (ADR-0031 Prereq #3) — flipped to dt-guard per ADR-0034 §3.
# Full policy logic lives in `crates/dt-guard/src/metric_labels.rs`.
# shellcheck source=./_dt_guard_wrapper.sh
source "$(dirname "${BASH_SOURCE[0]}")/_dt_guard_wrapper.sh" metric-labels "$@"
