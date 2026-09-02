#!/usr/bin/env bash
# Layer 3 — Guards (always-run: no skip-if-untouched).
#
# Wraps existing scripts/guards/run-guards.sh with the STATUS contract.
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
# SELF-TEST is wired explicitly here (task #47) — there is no *.test.sh auto-runner, so an
# unrun test is untested. This drives the check's FAIL branches (past-due/drift/malformed/
# sentinel) every devloop + CI.
{
  run_and_emit "guards" "${__here}/guards/run-guards.sh" || true
  run_and_emit "changed-helpers-test" "${__here}/lang/_changed_helpers.test.sh" || true
  run_and_emit "audit-suppressions-selftest" "${__here}/audit-suppressions-check.test.sh" || true
  run_and_emit "audit-gate-test" "${__here}/lang/_audit_gate.test.sh" || true
  run_and_emit "layer7-selftest" "${__here}/layer7.test.sh" || true
  # Org-subdomain drift guard SELF-TEST (story R-7, task #3). The guard itself is
  # auto-discovered by run-guards.sh above and runs on every devloop — but a PASSING guard
  # exercises none of its failure branches, and those branches are its entire value: a check
  # that greps, finds nothing, compares nothing and reports success is the exact bug it was
  # written to prevent. This drives them (drifted literal, renamed site, a seventh encoding,
  # a skewed pinned count, and the zero-hit vacuity case) against synthetic trees via the
  # DEVLOOP_TEST-gated SUBDOMAIN_GUARD_ROOT seam. Deliberately NOT under guards/simple/:
  # run-guards.sh's `find -name '*.sh'` matches `*.test.sh` too, so it would be auto-run as a
  # guard as well as here. No cluster.
  run_and_emit "subdomain-regex-guard-selftest" "${__here}/guards/validate-subdomain-regex-sync.test.sh" || true
  # Slug-class sync self-test (story task #4, R-8): same shape and same reason as the
  # subdomain guard above. validate-slug-class-sync.sh pins /close-story's read-time slug
  # regex to the canonical `manifest::SLUG_PATTERN`; in-tree it always passes, so its drift
  # branch — the only branch with any value — is exercised here against synthetic pairs via
  # the SLUG_SYNC_RUST_SRC / SLUG_SYNC_SKILL seams. Also covers the vacuity cases (marker
  # missing, canonical declaration renamed), where a grep-based guard would otherwise compare
  # nothing and report success. Deliberately NOT under guards/simple/ for the same
  # find -name '*.sh' reason. No cluster.
  run_and_emit "slug-class-sync-guard-selftest" "${__here}/guards/validate-slug-class-sync.test.sh" || true
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
  # Guard-runner self-test (fast-fail-guards devloop): pins run-guards.sh's timeout
  # emission + the violation/timeout counter split + the ladder-mirror exit precedence
  # (PRECONDITION>0 → exit 2 dominant), all via a PATH-stubbed `timeout` (hermetic, no real
  # sleep). The exit-precedence mirror in run-guards.sh (it sources guards/common.sh, not
  # lang/_common.sh, so cannot call status_to_exit_code) has NO other mechanical drift guard;
  # this test derives the expected exit from status_to_exit_code at test time. Deliberately
  # NOT under guards/simple/ — run-guards.sh's `find simple -name '*.sh'` would auto-run it as
  # a guard too. No cluster.
  run_and_emit "run-guards-selftest" "${__here}/guards/run-guards.test.sh" || true
  # Frame-vector drift guard SELF-TEST (story task 8, R-7). The guard itself is
  # auto-discovered by run-guards.sh above and passes on every devloop — but a
  # passing guard exercises none of its failure branches, and those branches are
  # its entire value. This drives all 33: every vacuity PRECONDITION, every
  # VIOLATION, and g14's four arms (flag-true-marker-absent, claim-true-codec-
  # ungated, the task-8 state that PASSES with a banner, and the task-15 state
  # that passes without one). Two of those arms exist because the obvious
  # "fixes" are wrong: making g14 exit nonzero at task 8 would make DELETING the
  # guard the fastest route to green, and a guard that redded on the task-15
  # state would only be discovered at task 15. It also pins that the banner
  # keeps its `^WARN ` prefix, which run-guards.sh's exit-0 arm greps for —
  # run-guards.test.sh's stub is synthetic and cannot reach that leg. Hermetic
  # via the DEVLOOP_TEST-gated FRAME_VECTORS_ROOT seam; no cluster, no network,
  # no cargo. Deliberately NOT under guards/simple/ for the same
  # `find -name '*.sh'` reason as its neighbours above.
  run_and_emit "frame-vectors-guard-selftest" "${__here}/guards/validate-frame-vectors.test.sh" || true
  # dev-web preflight contract self-test (release-premise-and-preflight-guards devloop):
  # pins the WARN→HARD-FAIL escalation of the MC/MH WebTransport checks, which an
  # exit-code assertion structurally CANNOT pin — `check_port` sets HARD_FAIL without
  # exiting, so `dev-web.sh --check` exits nonzero in any cluster-less environment
  # regardless of what those branches decided. Asserts the per-branch ✗/! markers
  # instead, so reverting the escalation flips it red.
  #
  # It also carries the only automated link in the chain
  #   client-dev-local.md §3 Step 0 -> `--help` -> script header -> script body
  # after §3 Step 0's duplicate severity enumeration was deleted: the header-vs-body
  # drift assertions and the `--help` last-line sentinel. Without this file wired here,
  # that deletion would remove redundancy without adding a forcing function.
  #
  # Hermetic: PATH-stubbed ss/curl/getent, temp repo root, AC_PORT/GC_PORT overrides.
  # No cluster, no network. Deliberately NOT under guards/simple/ — run-guards.sh's
  # `find simple -name '*.sh'` would auto-run it as a guard.
  run_and_emit "dev-web-preflight-selftest" "${__here}/dev-web.test.sh" || true
} | tee_collect_statuses
