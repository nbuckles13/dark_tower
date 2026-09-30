#!/usr/bin/env bash
# Client metrics export drift control (R-27) — catalog <-> collector name sets,
# `Exported:` markers across all non-test sdk-core, emitters, emitted label keys
# at exported emissions vs GC's filter AND keep_keys, keep_keys <-> GC's forwarded
# sets, the identity-label policy (label-taxonomy.md §R4), `dt_client_*` names and
# labels in LOADED alert rules, the zero-forever tripwire alert shape, the
# frame-reject-token partition against MCMediaMissingKeyMaterial, the
# metric_expiration/max_stale relations, and the rewrite-sentinel collision.
# Every input is required: a missing file is `extractor_empty_input`, never OK.
#
# Exists because every control on the collector's metrics path fails by making a
# series ABSENT, which on a live cluster is indistinguishable from "no browser is
# connected". Full policy logic lives in
# `crates/dt-guard/src/client_metrics_export.rs`.
# shellcheck source=./_dt_guard_wrapper.sh
source "$(dirname "${BASH_SOURCE[0]}")/_dt_guard_wrapper.sh" client-metrics-export
