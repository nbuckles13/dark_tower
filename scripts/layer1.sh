#!/usr/bin/env bash
# Layer 1 — Compile (ADR-0033 §5).
#
# Stage 1: contract (proto compile/lint via lang/proto/{compile,lint}.sh).
# Wave 2 #4 lands lang/proto/ wrappers; this stage becomes meaningful then.
# Ordering discipline must be visible in source even when the work is empty —
# Wave 2 must call this stage BEFORE stage 2 (ADR-0033 §5).
#
# Stage 2: code (rust + ts; ts wrappers land in Wave 2 #5).
set -euo pipefail
IFS=$'\n\t'
source "$(dirname "$0")/lang/_common.sh"
layer_lifecycle_begin 1

# Resolve base ref once per layer (observability §F: per-layer BASE_REF= emission).
# DEVLOOP_LAYER is exported by layer_lifecycle_begin (dry-reviewer F3).
"$(dirname "$0")/lang/_get_base_ref.sh" >/dev/null

# Stage 1: proto first (Wave 2 #4 fills lang/proto/).
if [[ -d "$(dirname "$0")/lang/proto" ]]; then
  "$(dirname "$0")/build.sh" 2>&1 | tee_collect_statuses
else
  # Stage 2: code (rust at Wave 1; rust+ts at Wave 2).
  "$(dirname "$0")/build.sh" 2>&1 | tee_collect_statuses
fi
