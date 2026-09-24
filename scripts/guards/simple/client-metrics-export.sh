#!/usr/bin/env bash
# Client metrics export drift control (R-27) — catalog <-> collector name sets,
# `Exported:` markers, emitters, keep_keys vs the GC filter, the
# metric_expiration/max_stale relations, and the rewrite-sentinel collision.
#
# Exists because every control on the collector's metrics path fails by making a
# series ABSENT, which on a live cluster is indistinguishable from "no browser is
# connected". Full policy logic lives in
# `crates/dt-guard/src/client_metrics_export.rs`.
# shellcheck source=./_dt_guard_wrapper.sh
source "$(dirname "${BASH_SOURCE[0]}")/_dt_guard_wrapper.sh" client-metrics-export
