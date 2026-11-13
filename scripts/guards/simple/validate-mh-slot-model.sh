#!/usr/bin/env bash
# MH Slot-Model Guard (story 4 task I-3; ADR-0039 §7, R-31): no MH workload in
# the environment-root apply set, and no drift between the slot table
# (crates/devloop-helper/src/ports.rs), the kind-configs, the per-slot
# ConfigMaps and GC's MH allowlists. Runs deploy.sh's own derivations
# hermetically. Full policy logic lives in `crates/dt-guard/src/mh_slot_model.rs`.
# (Rules 1+2, the MH template invariants and Kind-only shareProcessNamespace,
# run inside `validate-kustomize.sh`.)
# shellcheck source=./_dt_guard_wrapper.sh
source "$(dirname "${BASH_SOURCE[0]}")/_dt_guard_wrapper.sh" mh-slot-model
