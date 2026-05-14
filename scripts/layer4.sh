#!/usr/bin/env bash
# Layer 4 — Test.
#
# Failure triage: docs/runbooks/devloop-validation.md §6.4 (Layer 4 + §4 two-token convention).
set -euo pipefail
IFS=$'\n\t'
source "$(dirname "$0")/lang/_common.sh"
layer_lifecycle_begin 4
"$(dirname "$0")/lang/_get_base_ref.sh" >/dev/null
"$(dirname "$0")/test.sh" 2>&1 | tee_collect_statuses
