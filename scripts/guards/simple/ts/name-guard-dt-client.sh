#!/usr/bin/env bash
# R-26 Client Metric-Name Convention Guard — flipped to dt-guard per ADR-0034 §3.
# Full policy logic lives in `crates/dt-guard/src/ts_metric_naming.rs`.
# shellcheck source=../_dt_guard_wrapper.sh
source "$(dirname "${BASH_SOURCE[0]}")/../_dt_guard_wrapper.sh" ts-name-guard-dt-client
