# Devloop Output: ADR-0031 Follow-up #1 — expr-window `for:` floor exemption

**Date**: 2026-04-18
**Task**: Extend `validate-alert-rules.sh` to exempt rules with expr-window flap suppression (rate/increase/sum_over_time with `[Nm]` window where N ≥ 30s) from the `for: ≥ 30s` floor. Update conventions doc. Restore MCActorPanic to `for: 0m` (original intent, workaround no longer needed).
**Specialist**: operations (owns the guard per ADR-0031)
**Mode**: Agent Teams (light)
**Branch**: `feature/dashboard-owner-debate`

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `c6eeeb8df06e24e68e01c40cf9f20868a92cfea1` |
| Branch | `feature/dashboard-owner-debate` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@devloop-fu1-expr-window` |
| Implementing Specialist | `operations` |
| Iteration | `1` |
| Security | `CLEAR (security@devloop-fu1-expr-window)` |
| Observability | `APPROVE (observability@devloop-fu1-expr-window) — 1 non-blocking suggestion (irate exclusion note) folded in` |
| Context Reviewer | observability (conventions + expr-pattern semantics) |

---

## Task Overview

### Objective

Close ADR-0031 Convention Follow-up tracked in `TODO.md`: the alert-rules guard's `for: ≥ 30s` floor currently rejects a legitimate pattern where flap suppression comes from the expr's rate/increase window rather than from `for:`. Workaround shipped in `mc-alerts.yaml:33` (MCActorPanic `for: 0m` → `for: 30s`) adds 30s detection lag that's noise vs actor-supervision restart timing.

### Scope

1. Guard: detect `rate|increase|sum_over_time` calls with `[Nm]`/`[Nh]`/`[Nd]` window ≥ 30s in the rule's `expr:` field. When present, exempt the rule from the `for: ≥ 30s` floor. Still require `for:` to be present (non-empty).
2. Fixtures: `pass-expr-window-increase.rs`-equivalent YAML cases; also a `fail-expr-window-too-short.yaml` case where the window is < 30s to verify the exemption doesn't trigger on trivially small windows.
3. Conventions: `docs/observability/alert-conventions.md` §`for:` conventions — document the expr-window pattern explicitly so reviewers know the shape and the exemption reasoning.
4. Restore MCActorPanic: `infra/docker/prometheus/rules/mc-alerts.yaml` MCActorPanic rule `for: 30s` → `for: 0m`. This is a tiny 1-line revert of our original workaround now that the convention supports it.
5. TODO.md: remove the "ADR-0031 Convention Follow-ups → `for:` floor should recognize expr-window patterns" entry (follow-up closed).

### Posture

Mechanical-is-mechanical as usual. All 5 items in-devloop.

### Debate Decision

NOT NEEDED — this was explicitly captured as a convention follow-up with a clear desired shape.

---

## Reference

- TODO.md "ADR-0031 Convention Follow-ups → `for:` floor should recognize expr-window patterns" (being closed)
- Guard: `scripts/guards/simple/validate-alert-rules.sh`
- Conventions: `docs/observability/alert-conventions.md` §`for:` conventions
- Motivating case: `infra/docker/prometheus/rules/mc-alerts.yaml:33` MCActorPanic

---

## Implementation Summary

Closes ADR-0031 Convention Follow-up #1. Guard now accepts a rule's `for:` being less than 30s when the `expr:` contains a `rate`/`increase`/`sum_over_time` call with `[Nm]` window ≥ 30s (flap suppression at the expr-window layer). `for:` presence still required — rules with no `for:` field fail unchanged. MCActorPanic (the motivating case) restored to `for: 0m`.

### Guard changes
- `find_qualifying_expr_window()` helper: balanced-paren walker identifies `rate|increase|sum_over_time(...)` calls and extracts the max `[window]` duration from within each. Returns the maximum qualifying window ≥ 30s across all matching calls in the expr.
- `validate_for()` extended signature: `(for_val, expr)`. When `for_val` parses below 30s but a qualifying expr-window is present, the rule is exempt. Missing `for:` still fails unconditionally.
- Failure messages updated to mention both paths (`for: ≥ 30s` OR expr-window ≥ 30s).
- `GUARD_DEBUG=1` env var emits a stderr debug line when the exemption applies (useful for reviewer spot-checks).
- `irate()` deliberately excluded (per-observability review note + doc follow-up): its `[W]` is a lookback bound, not a smoothing window — `for: 0m + irate(foo[5m])` would still flap. Documented in conventions doc.

### Fixtures
5 new fixtures locking in both sides of the boundary:
- `pass-expr-window-increase.yaml` — `increase(foo_total[5m]) > 0` + `for: 0m`
- `pass-expr-window-rate.yaml` — `rate(foo_total[2m]) > 0.01` + `for: 30s` (both paths satisfy)
- `pass-expr-window-sum_over_time.yaml` — `sum_over_time(foo_gauge[10m]) > 100` + `for: 0m`
- `fail-expr-window-too-short.yaml` — `rate(foo_total[10s])` + `for: 0m` (window below 30s threshold)
- `fail-missing-for.yaml` — rule with no `for:` field at all (still fails regardless of expr shape)

Self-test: 25/25 pass.

### Conventions doc
`docs/observability/alert-conventions.md` §`for:` Conventions:
- Rewrote floor bullet to state the OR rule (`for: ≥ 30s` OR qualifying expr-window).
- New "Two flap-suppression mechanisms" subsection: when to prefer each (`reviewer-only`), exemption rule (`guard-enforced`), MCActorPanic exemplar, `irate` exclusion rationale.
- Updated single-scrape-trigger anti-pattern text.
- Rule index row updated.

### Alert-file restore
`infra/docker/prometheus/rules/mc-alerts.yaml` MCActorPanic: `for: 30s` → `for: 0m`. Original ADR-0023 intent restored (immediate fire on any detected actor panic; 5m expr-window provides flap suppression).

### TODO.md
Removed the closed follow-up entry. Section header and the MC heartbeat entry preserved.

---

## Files Modified

**New** (5 fixtures + devloop output):
- `scripts/guards/simple/fixtures/alert-rules/pass-expr-window-increase.yaml`
- `scripts/guards/simple/fixtures/alert-rules/pass-expr-window-rate.yaml`
- `scripts/guards/simple/fixtures/alert-rules/pass-expr-window-sum_over_time.yaml`
- `scripts/guards/simple/fixtures/alert-rules/fail-expr-window-too-short.yaml`
- `scripts/guards/simple/fixtures/alert-rules/fail-missing-for.yaml`
- `docs/devloop-outputs/2026-04-18-adr-0031-fu1-expr-window-for/main.md`

**Modified** (4):
- `scripts/guards/simple/validate-alert-rules.sh` (+68 / −4): expr-window detection + validator extension
- `docs/observability/alert-conventions.md` (+67 / −4): §`for:` Conventions rewrite + subsection + irate note
- `infra/docker/prometheus/rules/mc-alerts.yaml` (+1 / −1): MCActorPanic `for:` restore
- `TODO.md` (−25): closed follow-up entry removed

Net diff: 9 files changed (4 modified + 5 new fixtures), +~130 / −~36.

---

## Devloop Verification Steps

- L1 (cargo check): PASS — no Rust changes
- L2 (cargo fmt): PASS — no Rust changes
- L3 (guards): 18/18 PASS; 25/25 self-test fixtures pass
- L4/L5 (tests, clippy): trivial — no Rust changes
- L6 (cargo audit): pre-existing vulnerabilities, not this devloop's concern
- L7 (semantic): Lead-judgment SAFE — shell + YAML + markdown, no new Rust/service surface
- L8 (env-tests): skipped — no Rust/service changes

---

## Code Review Results

### Security Specialist
**Verdict**: CLEAR (0 findings)

Parser injection-safe (yaml.safe_load + compiled regex + balanced-paren walk; no eval/shell/subprocess). ReDoS-safe regexes. `for:` presence still enforced post-change. All 3 production alert files clean. Hygiene checks untouched.

### Observability Specialist
**Verdict**: APPROVE (0 blocking findings; 1 non-blocking suggestion addressed)

Pattern detection correctness verified (word-boundary regex rejects `irate|myrate|rate_limit_*`; balanced-paren walker correctly associates `[window]` with enclosing function call). 30s threshold defensibility confirmed vs 30s scrape-interval alignment. Conventions-doc clarity strong. MCActorPanic restore semantically correct per ADR-0023. ADR-0011 + ADR-0029 alignment verified (exemption privileges the counter-access functions ADR-0029 mandates).

**Suggestion folded in**: one-line note documenting `irate` exclusion rationale (W is lookback bound, not smoothing window).

---

## Rollback Procedure

1. Start commit: `c6eeeb8df06e24e68e01c40cf9f20868a92cfea1`
2. Soft reset: `git reset --soft c6eeeb8`
3. All changes are contained (guard + fixtures + conventions + one-line alert restore + one TODO removal).
