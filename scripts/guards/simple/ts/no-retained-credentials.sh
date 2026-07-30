#!/usr/bin/env bash
# Client Credential-Retention Guard — dt-guard subcommand per ADR-0034 §3 (task #58).
# Full policy logic lives in `crates/dt-guard/src/ts_retained_credentials.rs`.
# shellcheck source=../_dt_guard_wrapper.sh
source "$(dirname "${BASH_SOURCE[0]}")/../_dt_guard_wrapper.sh" ts-no-retained-credentials
