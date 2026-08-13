#!/usr/bin/env bash
# Layer 3 — Guards (always-run: no skip-if-untouched).
#
# Wraps existing scripts/guards/run-guards.sh with the STATUS contract.
# Per ADR-0033 implementation note: also invokes _test_changed_predicates.sh
# meta-test so predicate drift is detected on every devloop, not just at PR time.
#
# Failure triage: docs/runbooks/devloop-validation.md §6.3 (Layer 3 + §4 two-token convention).
set -euo pipefail
IFS=$'\n\t'
__here="$(dirname "$0")"
source "${__here}/lang/_common.sh"

# CI-SENTINEL-LEAK runtime assertion (task #47, §J/C — security trust boundary).
# Shared single-locus check in _common.sh; also called from layer-all.sh. Fires here so
# it ALSO catches a DEVLOOP_TEST leak when layer3 runs standalone (not via layer-all).
assert_no_ci_sentinel_leak

layer_lifecycle_begin 3
"${__here}/lang/_get_base_ref.sh" >/dev/null

# Use run_and_emit (dry-reviewer F2) instead of inline if/else — single source
# of truth for STATUS line emission + exit-code mapping.
# The audit-suppressions guard is auto-discovered by run-guards.sh (simple/*.sh); its
# SELF-TEST is wired explicitly here (task #47) alongside the predicate meta-test —
# there is no *.test.sh auto-runner, so an unrun test is untested. This drives the
# check's FAIL branches (past-due/drift/malformed/sentinel) every devloop + CI.
{
  run_and_emit "guards" "${__here}/guards/run-guards.sh" || true
  run_and_emit "predicate-meta-test" "${__here}/lang/_test_changed_predicates.sh" || true
  run_and_emit "changed-helpers-test" "${__here}/lang/_changed_helpers.test.sh" || true
  run_and_emit "audit-suppressions-selftest" "${__here}/audit-suppressions-check.test.sh" || true
  run_and_emit "audit-gate-test" "${__here}/lang/_audit_gate.test.sh" || true
  run_and_emit "layer7-selftest" "${__here}/layer7.test.sh" || true
  # Disk-guard self-test (envtest-infra-reliability devloop): forces
  # check_build_disk_space's trip branch so the `REASON=insufficient-disk` operator contract
  # (§6.7) is covered — Gate-2's rebuild only hits the guard's pass path. No cluster needed
  # (real df, absurd DEVLOOP_MIN_DISK_GB).
  run_and_emit "setup-disk-guard-selftest" "${__here}/setup.test.sh" || true
  # Orchestrator lane-integrity (task #56): exercises layer-all's exit-code/summary/budget/
  # bypass-closure behavior end-to-end via the DEVLOOP_TEST-gated LAYER_SCRIPT_DIR stub seam
  # — the layer-all-level seams layer7.test.sh structurally cannot reach. No cluster.
  run_and_emit "layer-all-orchestrator-test" "${__here}/layer-all.test.sh" || true
  # Story-runner self-test (ADR-0035 §12): the runner drives unattended
  # skip-permissions sessions and reaches `git reset --hard` + `git clean -fdq`
  # from a classification decision, with zero tests before this file. Hermetic —
  # real runner, throwaway fixture repo via the DEVLOOP_TEST-gated
  # STORY_REPO_ROOT/DT_STORY seams; stub claude/sleep/date, real git, real
  # dt-story. No cluster, no network, no real sessions.
  run_and_emit "run-story-selftest" "${__here}/workflow/run-story.test.sh" || true
} | tee_collect_statuses
