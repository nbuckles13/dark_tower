#!/usr/bin/env bash
# Layer 2 — Format.
#
# Failure triage: docs/runbooks/devloop-validation.md §6.2 (Layer 2 + §4 two-token convention).
set -euo pipefail
IFS=$'\n\t'
source "$(dirname "$0")/lang/_common.sh"
layer_lifecycle_begin 2
"$(dirname "$0")/lang/_get_base_ref.sh" >/dev/null
"$(dirname "$0")/fmt.sh" 2>&1 | tee_collect_statuses || layer_lifecycle_child_rc "$?"
# Normal end (lifecycle). The dispatcher's exit is RECORDED (layer_lifecycle_child_rc), not
# discarded: a dispatcher cut short after a sibling streamed STATUS=OK exits non-zero over
# all-green statuses, and the lifecycle renders that UNKNOWN layer-incomplete. Without this
# mark a killed/aborted layer renders the same.
layer_lifecycle_done
