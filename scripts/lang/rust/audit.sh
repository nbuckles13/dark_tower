#!/usr/bin/env bash
# Rust audit: cargo audit (always-run per ADR-0033 §3 + §6).
# Wave 1 ships upstream defaults — no flags, no allowlist.
#
# IMPORTANT (security finding 1): we deliberately do NOT pass "$@" through to
# cargo audit. Threshold/allowlist edits and `--ignore` flags are security's
# domain (ADR-0033 §11) and MUST land via a tracked audit-config file in a
# follow-up, not via ad-hoc CLI flags. Allowing CLI pass-through would let a
# caller silence advisories at runtime (e.g. `scripts/audit.sh --ignore=RUSTSEC-...`)
# without leaving a trace in the layer log — bypassing the always-run gate.
# If a future caller needs to pass legitimate args (e.g. `--db /custom/path`),
# add an explicit allowlist of safe flags rather than blanket pass-through.
set -euo pipefail
IFS=$'\n\t'
source "$(dirname "${BASH_SOURCE[0]}")/../_common.sh"
run_and_emit "cargo-audit" cargo audit
