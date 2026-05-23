#!/usr/bin/env bash
# Application Metrics Validation Guard — flipped to dt-guard per ADR-0034 §3.
# Full policy logic lives in `crates/dt-guard/src/application_metrics.rs`.
# shellcheck source=./_dt_guard_wrapper.sh
source "$(dirname "${BASH_SOURCE[0]}")/_dt_guard_wrapper.sh" application-metrics
