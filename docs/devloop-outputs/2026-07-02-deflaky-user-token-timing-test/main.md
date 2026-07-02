# Devloop Output: De-flake test_issue_user_token_timing_attack_prevention

**Date**: 2026-07-02
**Task**: Fix the flaky `test_issue_user_token_timing_attack_prevention` (ac-service) — a single-sample timing comparison that flakes under load. Surfaced during the 2026-06-30/07-01 env-test effort (cost a Layer-4 retry).
**Specialist**: auth-controller (Lead-driven direct fix, user-directed)
**Mode**: direct fix (test-only; no production behavior change)
**Branch**: `feature/browser-client-join-task-57`

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `a346a72` |
| Branch | `feature/browser-client-join-task-57` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete (GATE2=PASS)` |
| Implementer | `team-lead` (Lead-driven; user-directed direct fix) |

---

## Origin & Root Cause

`test_issue_user_token_timing_attack_prevention` (`crates/ac-service/src/services/token_service.rs:2952`)
flaked during the commit-2 `layer-all` of the 2026-06-30 infra-fix effort (Layer 4 `passed=388 failed=1`),
then passed on a quiet standalone re-run — a load-triggered flake, unrelated to any diff in that effort.

**Root cause (code + empirical):** the test took a SINGLE timing sample per path (one existing-user
wrong-password call, one nonexistent-user call) with no warmup, and compared those two lone measurements
against a 30% variance threshold. Empirically (6 quiet runs), the true difference is tiny and stable —
existing 394–401 ms vs nonexistent 389–393 ms, **diff 0.3–2.2%** (a ~15× margin under 30%) — with a small
consistent first-call bias (existing, measured first, ~1–9 ms slower = cold-start). So it is rock-solid
at rest; the flake is a **single-sample outlier under load**: one lone bcrypt-cost-12 call (~400 ms)
descheduled by >120 ms (>30%) during the concurrent `layer-all`, with nothing to average it out.

The sibling `test_timing_attack_prevention_invalid_client_id` (service-token, same file `:390`) was
already hardened against exactly this — `ITERATIONS=5` samples + warmup + unique client-ids +
compare-the-minimum ("CI noise only makes things slower, not faster"). The user-token test never got it.

## Fix

Mirror the sibling: warmup both paths once, take `ITERATIONS=5` samples per path with **unique emails per
iteration** (rate limit is 5 attempts/15 min per email — reuse would early-return and corrupt timings),
and compare the **minimums**. Test-only; the production timing-attack-prevention behavior (dummy-hash
verify on the non-existent path) is unchanged. `MAX_TIMING_VARIANCE_PERCENT` (30%) is untouched.

---

## Cross-Boundary Classification

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `crates/ac-service/src/services/token_service.rs` (MODIFY: min-of-N/warmup test hardening) | Not mine, Minor-judgment | auth-controller — test-only de-flake mirroring the sibling test in the same file; no production behavior change. Lead-driven direct fix per user direction. |

(Not a Guarded Shared Area — `services/token_service.rs` is NOT under `ac-service/src/{token,crypto,jwks}/**`.)

---

## Validation

`test_issue_user_token_timing_attack_prevention` passed 4/4 standalone `layer4.sh` runs (the full
parallel-test load where it flaked) + a single-test run. Gate 2 to produce `GATE2=PASS`.
