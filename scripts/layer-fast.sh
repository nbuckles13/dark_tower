#!/usr/bin/env bash
# layer-fast.sh — developer inner-loop validation: every layer EXCEPT the last one
# (LAYER_MAX, scripts/lang/_common.sh — Layer 7's shared-cluster bring-up).
#
# This is the check an implementer runs to verify its own work before signalling
# "Ready for review" (and after review fixes), and the Lead's fast check before review. It is NON-DESTRUCTIVE and touches NO shared cluster, so
# concurrent implementer runs can never thrash the one named Layer-7 cluster the
# Lead's Gate 2 is using (the collision this script exists to prevent).
#
# It is NOT the authority Gate 2. It emits no Gate-2 verdict, so a commit still
# requires a full `./scripts/layer-all.sh` — the Lead's Gate 2, which alone runs
# Layer 7. See ADR-0033 §4 and .claude/skills/devloop/SKILL.md §Gate 2.
#
# Thin wrapper by design: the pipeline engine (loop, aggregation, LAYER_SUMMARY,
# D8 triage) lives ENTIRELY in layer-all.sh — the single source of truth. A shortened
# `--max-layer` is refused there in any attesting context (CI / run-story gate / pre-commit), so this
# can never be used to shorten an authority run.
set -euo pipefail
IFS=$'\n\t'

__here="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=lang/_common.sh
source "${__here}/lang/_common.sh"
__fast_max=$(( LAYER_MAX - 1 ))

if (( $# )); then
  printf 'layer-fast.sh takes no arguments — it always runs layers 1-%d. For a different range use: ./scripts/layer-all.sh --max-layer N\n' "$__fast_max" >&2
  exit 2
fi

exec "${__here}/layer-all.sh" --max-layer "$__fast_max"
