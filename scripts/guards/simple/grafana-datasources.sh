#!/usr/bin/env bash
# Grafana Datasource Validation — flipped to dt-guard per ADR-0034 §3.
# Bespoke half (UID dedup + Loki-label consistency); vendor-native D-2
# (`grafana cli --dry-run`) is Wave 4 per ADR §8.
# Full policy logic lives in `crates/dt-guard/src/grafana_datasources.rs`.
# shellcheck source=./_dt_guard_wrapper.sh
source "$(dirname "${BASH_SOURCE[0]}")/_dt_guard_wrapper.sh" grafana-datasources
