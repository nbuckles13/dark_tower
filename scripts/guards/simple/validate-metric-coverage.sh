#!/usr/bin/env bash
# Metric Test-Coverage Validation Guard (ADR-0032) — flipped to dt-guard per ADR-0034 §3.
# Full policy logic lives in `crates/dt-guard/src/metric_coverage.rs`.
# shellcheck source=./_dt_guard_wrapper.sh
source "$(dirname "${BASH_SOURCE[0]}")/_dt_guard_wrapper.sh" metric-coverage
