#!/usr/bin/env bash
# GSA Enumeration Sync Guard (ADR-0024 §6.8 item #2) — flipped to dt-guard per ADR-0034 §3.
# Full policy logic lives in `crates/dt-guard/src/gsa_sync.rs`.
# Module header lists all 5 mirrors that must update together.
# shellcheck source=./_dt_guard_wrapper.sh
source "$(dirname "${BASH_SOURCE[0]}")/_dt_guard_wrapper.sh" gsa-sync
