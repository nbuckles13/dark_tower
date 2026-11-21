#!/usr/bin/env bash
# _layers.sh — the ONE home of the pipeline's layer range and Layer LAYER_MAX's suite list.
#
# DEPENDENCY-FREE BY DESIGN: plain assignments and one pure function, no shell options, no
# IFS change, no other source. Sourced by _common.sh (every layer, wrapper and layer-all.sh)
# AND by _gate2_binding.sh (which must NOT pull in _common.sh — see its header), so the
# Gate-2 emitter and scripts/workflow/run-story.sh reach LAYER_MAX without a second copy.
# A second `LAYER_MAX=` assignment anywhere under scripts/ fails scripts/layer-all.test.sh
# (case `layer-max-single-home`).
#
# Plain assignments (not readonly): several wrappers source this file more than once.

# The pipeline's layers are 1..LAYER_MAX; the last one is the cluster layer (env-tests +
# browser E2E). layer-all.sh validates --max-layer / --layers against it and checks
# --aggregate completeness against it; layer-fast.sh runs all but the last; the Gate-2
# verdict renders one LAYER line per layer 1..LAYER_MAX; run-story's full gate is 1..LAYER_MAX.
LAYER_MAX=7

# Layer LAYER_MAX's Phase-2 suites, in run order. layer7.sh validates DEVLOOP_LAYER7_SUITE
# against it, and layer-all.sh --aggregate requires the CI shards' SUITE= fields to cover it
# exactly (a suite no shard ran fails the aggregate).
LAYER7_SUITES=(env-tests browser-e2e)

# True iff $1 is exactly one name from LAYER7_SUITES.
# Args: $1=candidate  Returns: 0 if known, 1 otherwise.
layer7_suite_known() {
  local s
  for s in "${LAYER7_SUITES[@]}"; do
    [[ "$1" == "$s" ]] && return 0
  done
  return 1
}
