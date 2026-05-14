#!/usr/bin/env bash
# Layer 1 — Compile (ADR-0033 §5).
#
# Both stages route through the dispatcher (`build.sh`); INCLUDE_LANGS and
# EXCLUDE_LANGS select which langs participate per stage. Dispatcher-as-
# single-enforcement-point is the load-bearing pipeline invariant — every
# lang-verb call (including stage 1) routes through it so changed.sh
# short-circuit, STATUS aggregation, and lint-at-startup contract apply
# uniformly. See Wave 2 #4 plan §8 for rationale.
#
# Stage ordering is encoded in source so a future reader sees stage 1 must
# precede stage 2: proto-derived artifacts (codegen) feed rust/ts compile.
#
# Failure triage: docs/runbooks/devloop-validation.md §6.1 (Layer 1 + §4 two-token convention).
set -euo pipefail
IFS=$'\n\t'
source "$(dirname "$0")/lang/_common.sh"
layer_lifecycle_begin 1

# Resolve base ref once per layer (observability §F: per-layer BASE_REF= line).
# DEVLOOP_LAYER is exported by layer_lifecycle_begin (dry-reviewer F3).
"$(dirname "$0")/lang/_get_base_ref.sh" >/dev/null

# Stage 1: contract (proto) — must precede stage 2 per ADR-0033 §5.
# Stage 2 runs unconditionally so the LAYER= line reflects the full picture
# even if stage 1 fails (observability O2: a runbook reader should learn
# proto + rust/ts state in one run, not need a second invocation after
# fixing proto). __layer_lifecycle_end aggregates STATUS via worst-wins, so
# stage-1 FAIL still propagates to layer-final FAIL via __LAYER_STATUSES.
DEVLOOP_DISPATCH_INCLUDE_LANGS=proto "$(dirname "$0")/build.sh" 2>&1 | tee_collect_statuses || true

# Stage 2: code (rust + ts) — depends on proto codegen artifacts from stage 1.
DEVLOOP_DISPATCH_EXCLUDE_LANGS=proto "$(dirname "$0")/build.sh" 2>&1 | tee_collect_statuses || true
