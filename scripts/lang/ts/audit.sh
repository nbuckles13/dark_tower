#!/usr/bin/env bash
# TS audit: pnpm audit --audit-level=high (always-run per ADR-0033 §3 + §6).
# Wave 1 ships threshold=high — security owns the policy threshold (ADR-0033 §11).
#
# IMPORTANT (security finding mirrored from lang/rust/audit.sh): we deliberately
# do NOT pass "$@" through to pnpm audit. Threshold and ignore-list edits are
# security's domain (ADR-0033 §11) and MUST land via a tracked audit-config file
# in a follow-up, not via ad-hoc CLI flags. Allowing CLI pass-through would let a
# caller silence advisories at runtime (e.g. `scripts/audit.sh --audit-level=critical`
# or `--ignore=GHSA-...`) without leaving a trace in the layer log — bypassing the
# always-run gate. If a future caller needs to pass legitimate args (e.g.
# `--registry=...` for a private mirror), add an explicit allowlist of safe flags
# rather than blanket pass-through.
set -euo pipefail
IFS=$'\n\t'
source "$(dirname "${BASH_SOURCE[0]}")/../_common.sh"
run_and_emit "pnpm-audit" pnpm audit --audit-level=high
