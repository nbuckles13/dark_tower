#!/usr/bin/env bash
# Layer 7 — Env-tests (Wave 2 fills body; Wave 1 emits N/A placeholder).
#
# Failure triage: docs/runbooks/devloop-validation.md §6.7 (Layer 7 + §4 two-token convention).
set -euo pipefail
IFS=$'\n\t'
source "$(dirname "$0")/lang/_common.sh"
layer_lifecycle_begin 7
emit_status N/A "wave2-pending" | tee_collect_statuses
