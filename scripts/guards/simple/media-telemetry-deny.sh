#!/usr/bin/env bash
# Media-Path Telemetry Deny Guard (ADR-0036 §11) — dt-guard subcommand.
# Full policy logic lives in `crates/dt-guard/src/media_telemetry_deny.rs`;
# the scanned directory list lives in `media-telemetry-deny.yaml` beside this file.
# shellcheck source=./_dt_guard_wrapper.sh
source "$(dirname "${BASH_SOURCE[0]}")/_dt_guard_wrapper.sh" media-telemetry-deny
