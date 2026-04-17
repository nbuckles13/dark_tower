# Devloop Output: ADR-0031 MH alerts exemplar (first greenfield authoring)

**Date**: 2026-04-17
**Task**: Create `infra/docker/prometheus/rules/mh-alerts.yaml` and `docs/runbooks/mh-incident-response.md` from scratch under the new ADR-0031 regime. Serves as the exemplar-first worked example for future service-specialist alert authoring.
**Specialist**: media-handler (implementer; domain owner of mh-alerts.yaml per ADR-0031 §Ownership split)
**Mode**: Agent Teams (light) — **paired**: observability acts as active collaborator during implementation, not just reviewer
**Branch**: `feature/dashboard-owner-debate`

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `9eabbbf67b9eddc29fd320c11eab874143c7c57e` |
| Branch | `feature/dashboard-owner-debate` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@devloop-mh-alerts-exemplar` |
| Implementing Specialist | `media-handler` |
| Iteration | `1` |
| Security | `CLEAR (security@devloop-mh-alerts-exemplar)` |
| Observability | `APPROVED (observability@devloop-mh-alerts-exemplar) — paired collaborator + formal verdict` |
| Context Reviewer | observability (paired mode — not just review at end) |

---

## Task Overview

### Objective

The "exemplar-first" follow-up captured in ADR-0031. Fills a known operational gap (MH has no alerts today) AND validates the entire ADR-0031 prereq rollout end-to-end by doing the first greenfield service-specialist alert authoring under the new regime.

### Scope

**Two new artifacts**:
- `infra/docker/prometheus/rules/mh-alerts.yaml` — new file, reasonable starter set covering MH's major operational surface
- `docs/runbooks/mh-incident-response.md` — new file, following mc-incident-response.md shape. Every alert's `runbook_url` anchor must resolve to a section in this file.

**No existing code changes.** MH's metrics are already instrumented (`crates/mh-service/src/observability/metrics.rs` + catalog at `docs/observability/metrics/mh-service.md`).

### Pairing model (why this devloop is special)

Per the original ADR-0031 debate, the exemplar-first rollout is a **paired effort**. Media-handler owns the domain judgment (what to alert on, MH-specific severity calls, operational response); observability owns the conventions + guard + template expertise. The pairing amortizes the PromQL/alert-annotation learning curve before other services fly solo.

Concretely:
- Media-handler implementer proposes the alert list EARLY (before writing full file) and shares with observability for structural feedback.
- Observability proactively collaborates during implementation — severity call debates, threshold sanity-check, PromQL shape refinement — not just review at the end.
- Security continues as standard annotation-hygiene reviewer.

### Debate Decision

NOT NEEDED — ADR-0031 §Follow-ups explicitly schedules this as a paired exemplar devloop.

---

## Reference

- Spec: ADR-0031 §Follow-ups (exemplar-first rollout entry)
- Alert conventions: `docs/observability/alert-conventions.md`
- Alert template: `infra/docker/prometheus/rules/_template-service-alerts.yaml`
- Guard: `scripts/guards/simple/validate-alert-rules.sh`
- MH metrics catalog: `docs/observability/metrics/mh-service.md`
- Runbook pattern: `docs/runbooks/mc-incident-response.md` (MH's runbook should mirror shape)
- Existing alerts precedent: `infra/docker/prometheus/rules/gc-alerts.yaml`, `mc-alerts.yaml`

---

## Implementation Summary

First greenfield authoring under ADR-0031. The exemplar-first pattern (paired collaboration between domain owner and conventions expert) paid back in three ways:

1. **Directly produced better alerts** than solo authoring would have. Severity-calibration debates (JWT failures as `warning`-not-page; heartbeat-failure-rate over registration-failure-rate as the continuous signal) each hinged on the observability-media-handler back-and-forth.
2. **Caught three latent issues end-to-end**: `validate-application-metrics.sh` multi-line extractor blindness (4 MH metrics invisible to prior guard); MH dashboard-coverage gap (same 4 metrics never dashboarded); `run-guards.sh` early-abort + silent-skip bug (CI was silently passing when guards failed mid-loop). All three fixed in-devloop.
3. **Pedagogical record** of a clean paired workflow — documented in observability's final verdict, usable as a template for future service specialists authoring their first alerts under ADR-0031.

### MH alerts authored (13 total)

- **page** (1): `MHDown`
- **warning** (11): `MHHighJwtValidationFailures`, `MHGCHeartbeatFailureRate`, `MHHighRegistrationLatency`, `MHHighWebTransportRejections`, `MHWebTransportHandshakeSlow`, `MHCallerTypeRejected`, `MHHighMemory`, `MHHighCPU`, `MHTokenRefreshFailures`, `MHMCNotificationFailures`, `MHPodRestartingFrequently`
- **info** (1): `MHGCHeartbeatLatencyHigh`

Cross-service co-review triggers declared for `MHGCHeartbeat*` (GC co-review) and `MHMCNotificationFailures` (MC co-review).

### MH runbook authored

12 scenarios matching every `runbook_url` anchor. Each with Symptoms / Impact / Immediate Response / Root Cause Investigation / Recovery / Related Alerts sections. No TBD placeholders.

### Scope extensions (both in-devloop)

1. **4 MH dashboard panels** added to `infra/grafana/dashboards/mh-overview.json` — surfaced by the extractor fix unmasking pre-existing dashboard-coverage gap.
2. **Two adjacent guard patches** (observability-implemented):
   - `scripts/guards/simple/validate-application-metrics.sh`: multi-line macro extractor via Python DOTALL regex (line 127).
   - `scripts/guards/run-guards.sh`: `|| true` on pipe-to-grep + extended match pattern to `ERROR|error`; comment rationale preserves the load-bearing fix. CI-lie failure mode closed.

### Deliberate omissions (documented in mh-alerts.yaml header)

- No burn-rate pair — MH lacks an error-rate SLO in ADR-0011; flagged for ADR follow-up rather than fabricating thresholds.
- No actor/mailbox alerts — MH has no such metrics (unlike MC).
- No RegisterMeeting RPC alert — covered transitively via MC-side alerts.

---

## Files Modified

**New** (3):
- `infra/docker/prometheus/rules/mh-alerts.yaml` — 13 alerts
- `docs/runbooks/mh-incident-response.md` — 12 scenarios
- `docs/devloop-outputs/2026-04-17-adr-0031-mh-alerts-exemplar/main.md` — this file

**Modified** (3):
- `infra/grafana/dashboards/mh-overview.json` (+398 lines, 4 new panels)
- `scripts/guards/simple/validate-application-metrics.sh` (~32 lines: multi-line macro extractor)
- `scripts/guards/run-guards.sh` (~13 lines: early-abort + silent-skip fix)

---

## Devloop Verification Steps

- L1 (cargo check): PASS
- L2 (cargo fmt --check): PASS
- L3 (guards): **18/18 PASS end-to-end** — first post-extractor-fix pipeline green. Two failure-injection tests confirmed run-guards.sh now correctly continues past failures AND exits 1 when any guard fails.
- L4/L5 (tests, clippy): trivial — no Rust changes.
- L6 (cargo audit): pre-existing vulnerabilities (not this devloop's concern).
- L7 (semantic): Lead-judgment SAFE — YAML + markdown + shell (observability) + JSON (panels). No Rust/service surface.
- L8 (env-tests): skipped — no Rust/service changes.

---

## Code Review Results

### Security Specialist
**Verdict**: CLEAR
**Findings**: 0

Annotation hygiene clean across all 13 alerts. All 13 runbook_urls repo-relative. All 12 anchor references resolve to real H3 sections. Runbook PII spot-check clean. Guard edits (run-guards.sh + validate-application-metrics.sh) also sanity-reviewed — scope reasonable, no injection surface introduced.

### Observability Specialist (paired-collaborator + reviewer)
**Verdict**: APPROVED
**Findings**: 0 at formal review (pairing during implementation surfaced + resolved all issues before review)

Final state verified: thresholds sensibly SLO-derived or provisional-with-rationale; cardinality-safe exprs; ADR-0029 metric-type presentation correct; `for:` windows match rate windows sensibly; severity-taxonomy anchors applied correctly (not pattern-matched from `critical`); deliberate omissions documented, not silent.

**Pairing reflections captured for the exemplar-first pedagogical record** (see observability's final verdict, preserved in the review transcript): severity-taxonomy back-and-forth produced better decisions than solo reasoning; metric-choice discipline (heartbeat-failure-rate over registration-failure-rate) was a mid-course correction neither party would have made alone; threshold-provenance honesty (provisional vs SLO-derived) is exemplar-quality discipline; stale-review hazard surfaced and handled cleanly by both sides.

---

## Rollback Procedure

1. Start commit: `9eabbbf67b9eddc29fd320c11eab874143c7c57e`
2. Soft reset: `git reset --soft 9eabbbf`
3. Both new files are net-new — deleting them restores repo to prior clean state.
