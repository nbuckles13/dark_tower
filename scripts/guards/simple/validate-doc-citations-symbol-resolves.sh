#!/usr/bin/env bash
# Doc-Citation Symbol-Resolves Guard (Guard C) — flipped to dt-guard per ADR-0034 §3.
# Full policy logic lives in `crates/dt-guard/src/cite_extract.rs`.
# shellcheck source=./_dt_guard_wrapper.sh
source "$(dirname "${BASH_SOURCE[0]}")/_dt_guard_wrapper.sh" cite-symbol-resolves
