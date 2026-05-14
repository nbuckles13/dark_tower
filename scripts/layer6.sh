#!/usr/bin/env bash
# Layer 6 — Audit (always-run; per-language wrappers control their own skip).
#
# Failure triage: docs/runbooks/devloop-validation.md §6.6 (Layer 6 + §4 two-token convention).
set -euo pipefail
IFS=$'\n\t'
source "$(dirname "$0")/lang/_common.sh"
layer_lifecycle_begin 6
"$(dirname "$0")/lang/_get_base_ref.sh" >/dev/null
"$(dirname "$0")/audit.sh" 2>&1 | tee_collect_statuses
