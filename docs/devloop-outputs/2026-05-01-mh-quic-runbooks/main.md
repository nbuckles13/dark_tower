# Devloop Output: MH + MC Runbooks for QUIC Connection Story (Task 17)

**Date**: 2026-05-01
**Task**: Operations runbooks for MH WebTransport + MC↔MH coordination — covers R-34 (new MH runbook scenarios) and R-35 (MC runbook updates)
**Specialist**: operations
**Mode**: Agent Teams (full)
**Branch**: `feature/mh-quic-runbooks`
**Duration**: ~26m (setup → commit)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `f88a240ee127825444999b246b032535b31cde7f` |
| Branch | `feature/mh-quic-runbooks` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@mh-quic-runbooks` |
| Implementing Specialist | `operations` |
| Iteration | `1` |
| Security | `CLEAR` |
| Test | `CLEAR` |
| Observability | `RESOLVED` |
| Code Quality | `RESOLVED` |
| DRY | `RESOLVED` |
| Operations | `RESOLVED` |

---

## Task Overview

### Objective
Create operations runbook content for the client-to-MH QUIC connection story:
- **R-34**: New MH runbook scenarios covering WebTransport server failures, JWT validation issues, MC notification delivery failures, and RegisterMeeting timeout scenarios.
- **R-35**: Update MC runbook with MH coordination failure scenarios (RegisterMeeting send/retry/exhaust, MH notification handling, MediaConnectionFailed reports).

### Scope
- **Service(s)**: docs only (operations content for MH and MC services)
- **Schema**: No
- **Cross-cutting**: No (docs only; runbooks are operations-owned)

### Debate Decision
NOT NEEDED — task is documentation following the established runbook pattern; the underlying architectural decisions are already captured in the user story and prior devloops.

### Context for Implementer

**Key constraints / pre-existing state:**

1. `docs/runbooks/mh-incident-response.md` already exists (last updated 2026-04-17) and contains scenarios that overlap R-34 themes:
   - Scenario 2: JWT Validation Failures (covers JWKS, key rotation, clock skew already)
   - Scenario 5: WebTransport Rejections (covers TLS, capacity, QUIC listener)
   - Scenario 10: MH→MC Notification Failures (covers MC unreachable, network policy)
   - **Missing from R-34 ask**: RegisterMeeting timeout / provisional-client-kicked scenario, and an explicit "WebTransport server failed to start / bind / listen" scenario distinct from per-connection rejections.
2. `docs/runbooks/mc-incident-response.md` (last updated 2026-03-27) does not yet have any MH coordination scenarios. R-35 calls for three new ones.
3. Pattern for existing runbooks: `{service}-deployment.md` (deploy workflow) AND `{service}-incident-response.md` (runtime scenarios). MH currently has only the incident-response file — no `mh-deployment.md`.
4. The R-34 ask in the user story labels this as "MH deployment runbook" but the listed scenarios are runtime failure modes. Implementer should decide whether (a) extend `mh-incident-response.md` with the missing scenarios, (b) create a new `mh-deployment.md` matching the `mc-deployment.md` pattern with these as troubleshooting sections, or (c) both. Justify the choice in the plan.

**Authoritative metric/alert names** (from prior devloops, see operations INDEX):
- MH metrics: `mh_webtransport_connections_total`, `mh_webtransport_handshake_duration_seconds`, `mh_active_connections`, `mh_jwt_validations_total`, ~~`mh_register_meeting_total`~~ (does NOT exist — see correction below), `mh_register_meeting_timeouts_total`, `mh_mc_notifications_total`.
- MC metrics: `mc_register_meeting_total`, `mc_register_meeting_duration_seconds`, `mc_mh_notifications_received_total`, `mc_media_connection_failures_total`.
- Alert: `MCMediaConnectionAllFailed` (already in `infra/docker/prometheus/rules/mc-alerts.yaml`).

**Correction (verified 2026-05-01)**: `mh_register_meeting_total` does NOT exist. The MH side has two relevant metrics: (a) `mh_register_meeting_timeouts_total` for provisional-kick events; (b) `mh_grpc_requests_total{method="register_meeting", status}` as the receipt-side success/error counter (see `docs/observability/metrics/mh-service.md` §RegisterMeeting Metrics — it explicitly notes "no separate business-level counter was added since it would duplicate call-site recordings with identical totals"). Runbook will cite both correctly.

**Verify each metric/alert name against the actual emission site** before referencing — runbook PromQL/curl examples reference metric names by string and divergence is silent (operations INDEX warns that production wrapper signatures must stay byte-identical to keep runbooks valid).

**Source-of-truth references for the scenarios:**
- RegisterMeeting timeout (MH side): `crates/mh-service/src/webtransport/connection.rs`, `crates/mh-service/src/session/mod.rs`, config `MH_REGISTER_MEETING_TIMEOUT_SECONDS` (default 15).
- RegisterMeeting trigger (MC side): `crates/mc-service/src/webtransport/connection.rs:register_meeting_with_handlers()` (cancel-aware retry+backoff).
- MediaCoordinationService handlers (MC): `crates/mc-service/src/grpc/media_coordination.rs`.
- MhConnectionRegistry cleanup: `crates/mc-service/src/mh_connection_registry.rs` (controller.rs `remove_meeting()`).

---

## Cross-Boundary Classification

All planned changes are in `docs/runbooks/` and `docs/user-stories/` (tracking-table updates), both operations-owned. No GSA paths touched.

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `docs/runbooks/mh-incident-response.md` | Mine | — |
| `docs/runbooks/mc-incident-response.md` | Mine | — |
| `docs/user-stories/2026-04-12-mh-quic-connection.md` | Mine | — |

---

## Planning

### R-34 placement decision

Recommend **(a) extend `mh-incident-response.md`** with the two genuinely-new scenarios; do **not** create `mh-deployment.md` in this devloop. Rationale:

- All R-34 sub-bullets in the user story (WT server failures, JWT validation, MC notification delivery, RegisterMeeting timeout) describe **runtime failure modes**, not deploy-workflow steps. The existing precedent splits "deploy workflow" (e.g. `mc-deployment.md`) from "runtime scenarios" (e.g. `mc-incident-response.md`). The R-34 content fits the latter.
- The existing `mh-incident-response.md` already covers the JWT-validation (Sc 2) and per-connection WT-rejection (Sc 5) and MH→MC notification (Sc 10) themes. The runtime gaps are exactly two:
  1. **RegisterMeeting timeout / provisional-client kicked** — driven by `mh_register_meeting_timeouts_total` (the only metric in this family; there is **no** `mh_register_meeting_total` in source — the user-story-supplied metric list was overly optimistic and I will only cite what exists).
  2. **WT server startup / bind / listen failure** — distinct from the runtime per-connection rejections in Sc 5. Surfaces as pod CrashLoopBackOff with logs from `webtransport::server::bind()` rather than rising rejection rate.
- A `mh-deployment.md` would today be ~80% boilerplate copied from `mc-deployment.md` with little MH-specific content. Skipping it avoids creating runbook noise; if MH-specific deploy workflow emerges (TLS rotation playbook, GC drain handoff, etc.) it can be added in a future devloop. I will note this gap in the **Tech Debt** section so it is captured rather than dropped.

### R-35 scenarios

Three new scenarios appended to `mc-incident-response.md` (currently ends at Sc 10):

- **Scenario 11: MediaConnectionFailed Reports** — anchor matches the existing `MCMediaConnectionAllFailed` alert's `runbook_url` (`#scenario-11-media-connection-failures`), so we land on the URL the alert is already pointing at.
- **Scenario 12: RegisterMeeting Coordination Failures** — `mc_register_meeting_total{status="error"}` rate / duration p95 / retries-exhausted log signal. No alert yet exists for this; runbook documents how to triage the metric directly. Cross-link to MH Sc 5/13 (WT rejections / RegisterMeeting timeout).
- **Scenario 13: Unexpected MH Notifications** — `mc_mh_notifications_received_total` arriving for `meeting_id` not present locally (registry add returns true silently — there is no first-class "unknown meeting" error today; I will document the warn-log signal `Connection registry limit reached` and the `debug` log on stale-disconnect, plus what to look for in `mc.grpc.media_coordination` traces). Severity: info — diagnostic signal, not an alerting condition.

### Source-of-truth references (verified by grep)

| Asset | Location | Notes |
|---|---|---|
| MH RegisterMeeting timeout metric | `crates/mh-service/src/observability/metrics.rs:188-207` | `mh_register_meeting_timeouts_total`, no labels |
| MH timeout config | `crates/mh-service/src/config.rs:108` | `register_meeting_timeout_seconds`; env `MH_REGISTER_MEETING_TIMEOUT_SECONDS` |
| MH WT bind | `crates/mh-service/src/webtransport/server.rs:96` | `bind()` returns `Result`; default addr `0.0.0.0:4434` |
| MC RegisterMeeting metric + retry | `crates/mc-service/src/observability/metrics.rs:340-347` and `crates/mc-service/src/webtransport/connection.rs:42-45,742-813` | `mc_register_meeting_total{status}`, `mc_register_meeting_duration_seconds`; `MAX_REGISTER_ATTEMPTS=3`, backoffs `[1s, 2s]` |
| MC MH notification metric | `crates/mc-service/src/observability/metrics.rs:355-368` | `mc_mh_notifications_received_total{event_type}` — only emitted on the success path; no failure variant |
| MC MediaConnectionFailed metric + alert | `crates/mc-service/src/observability/metrics.rs:372-386`; `infra/docker/prometheus/rules/mc-alerts.yaml` (`MCMediaConnectionAllFailed`) | `mc_media_connection_failures_total{all_failed}`; alert annotation already references `#scenario-11-media-connection-failures` |
| MhConnectionRegistry add/remove | `crates/mc-service/src/mh_connection_registry.rs:62-115,120-...` | Soft-tolerant: unknown meeting on add becomes a new entry; `MAX_CONNECTIONS_PER_MEETING=1000`; warn-log on cap hit |

### Files to create / modify

| Path | Change | Cross-Boundary Classification |
|---|---|---|
| `docs/runbooks/mh-incident-response.md` | Append Sc 13 (RegisterMeeting timeout — clients kicked), Sc 14 (WT server startup failure); update TOC, bump Last Updated to 2026-05-01 | Mine |
| `docs/runbooks/mc-incident-response.md` | Append Sc 11 (MediaConnectionFailed), Sc 12 (RegisterMeeting coordination), Sc 13 (Unexpected MH notifications); update TOC, bump Last Updated to 2026-05-01, add 2026-05-01 entry to Version History | Mine |
| `docs/user-stories/2026-04-12-mh-quic-connection.md` | Devloop Tracking row 17 → fill in path + status=Completed (table only — no other content edits) | Mine |
| `docs/devloop-outputs/2026-05-01-mh-quic-runbooks/main.md` | Implementation Summary, Files Modified, verification, etc. | Mine |

All paths are operations-owned (`docs/runbooks/`) or the user-story tracking table. **No GSA paths touched.** Cross-Boundary section above is unchanged.

### Style/structure plan

For each new scenario, follow the existing per-scenario template (`### Scenario N: Title` → Alert / Severity / Symptoms / Impact / Immediate Response / Root Cause Investigation / Common Root Causes / Recovery / Related Alerts). PromQL and `kubectl exec` examples will follow the same shape as adjacent scenarios in the same file. Cross-link MC↔MH where coordination is bidirectional (e.g. MC Sc 12 ↔ MH Sc 13; MC Sc 11 ↔ MH Sc 5).

### Out of scope (explicit deferrals)

- New `mh-deployment.md` — not needed for R-34 ask; logged as Tech Debt.
- New alert rules — observability/test scope, not operations. R-35 documents the existing `MCMediaConnectionAllFailed` alert and triages the `mc_register_meeting_total` metric directly without proposing a new alert.
- Postmortem template / diagnostic-command updates — those sections in both files are still accurate; adding scenarios alone is sufficient.

---

## Pre-Work

None.

---

## Implementation Summary

Added five new operational scenarios across the two existing incident-response runbooks; no new runbook files created. All scenario content matches the existing `### Scenario N: Title → Alert / Severity / Symptoms / Impact / Diagnosis or Immediate Response / Common Root Causes / Remediation or Recovery / Related Alerts` template.

**R-34 — `docs/runbooks/mh-incident-response.md`** (extended; was Sc 1–12, now 1–14):
- **Sc 13: RegisterMeeting Timeout — Clients Kicked.** Documents `mh_register_meeting_timeouts_total` triage. Explicitly forbids tuning `MH_REGISTER_MEETING_TIMEOUT_SECONDS` as runtime mitigation (security boundary; bounds stolen-JWT-against-unregistered-meeting exposure). Cross-links to MC Sc 12.
- **Sc 14: WebTransport Server Startup Failure.** Distinct from per-connection rejections (existing Sc 5) — covers bind/listen/TLS-load failures where the pod cannot become Ready at all. PromQL flat-zero pattern + previous-pod log triage.

**R-35 — `docs/runbooks/mc-incident-response.md`** (extended; was Sc 1–10, now 1–13):
- **Sc 11: Media Connection Failures.** Anchor `#scenario-11-media-connection-failures` — matches the existing `MCMediaConnectionAllFailed` alert's `runbook_url` annotation in `infra/docker/prometheus/rules/mc-alerts.yaml`. Explicitly treats `error_reason` and `media_handler_url` from the signaling message as untrusted client input; corroborate against MH metrics before concluding cause.
- **Sc 12: RegisterMeeting Coordination Failures.** No alert today — metric-driven triage on `mc_register_meeting_total{status}` and `mc_register_meeting_duration_seconds`. Cross-links to MH Sc 13.
- **Sc 13: Unexpected MH Notifications.** Two-branch security split per the security review: diffuse pattern → operational drift (info), single-source-identity steady stream → authenticated-MH misbehavior (preserve logs, do not restart, escalate Security).

**Reviewer guidance integrated**:
- @test: all PromQL uses delta-over-window framing (`rate(...[5m])`, `increase(...[5m])`); 5m-vs-1h baseline comparison applied to slow-burn scenarios (MH Sc 13 and MC Sc 12); histograms use the canonical MC SLO shape `histogram_quantile(0.95, sum by(le) (rate(..._bucket[5m])))`.
- @security: Sc 13 has explicit two-branch tampering vs operational triage with "preserve logs / do not restart / escalate Security" wording in the misbehavior branch; Sc 11 phrased so client-reported fields are corroborated, never trusted; Sc 12 explicitly forbids `MH_REGISTER_MEETING_TIMEOUT_SECONDS` tuning. Hygiene rules followed: no `kubectl exec ... -- env`, no `grep` on JWT bodies, `curl` examples use `-H "Authorization: Bearer $TOKEN"` form (not present in this content because none of the new scenarios needed token-bearing curls).
- @observability (Gate 1 + post-Gate-1 follow-ups):
  - anchor `#scenario-11-media-connection-failures` matches the live alert annotation.
  - metric-name asymmetry handled correctly — MH receipt-side success rate now cited via `mh_grpc_requests_total{method="register_meeting"}` per the explicit guidance in `docs/observability/metrics/mh-service.md` §RegisterMeeting Metrics ("no separate business-level counter was added since it would duplicate call-site recordings").
  - `mc_mh_notifications_received_total` `status`-label gap documented in MC Sc 13 with explicit cross-link to MH Sc 10 (the sender-side failure metric); bidirectional cross-link added from MH Sc 10 back to MC Sc 13.
  - First-emission `# Note:` callouts added to MH Sc 13, MC Sc 11, MC Sc 12, and MC Sc 13 — mirroring the GC Sc 5 pattern.
  - Severity vocabulary uses `page` / `warning` / `info` consistently with `docs/observability/alert-conventions.md` and existing `mh-incident-response.md` / `mc-incident-response.md` conventions.
  - Bidirectional cross-links: MH Sc 10 ↔ MC Sc 13, MH Sc 13 ↔ MC Sc 12, MC Sc 11 → MH Sc 5/2/13.
  - `# TODO: alert MCRegisterMeetingFailureRate` placeholder note added to MC Sc 12 to keep alert-rule drift trackable when the next operations devloop picks up alert work.
  - Dashboard panel name references added (no brittle URLs): MH Overview "RegisterMeeting Timeouts (R-26)" and "RegisterMeeting Receipts by Status"; MC Overview "RegisterMeeting RPC Rate by Status" and "RegisterMeeting RPC Latency (P50/P95/P99)".
  - SLO threshold inventions avoided — runbook points at the latency dashboard panel rather than citing a non-existent SLO threshold.
- @code-reviewer / @dry-reviewer: scenarios cross-link rather than duplicate the bidirectional triage; structure matches existing precedent exactly.

**Source-of-truth verification** (grep'd against `crates/`):

| Cited | Source location | Notes |
|---|---|---|
| `mh_register_meeting_timeouts_total` | `crates/mh-service/src/observability/metrics.rs:188-207` | Only MH-side RegisterMeeting metric that exists — `mh_register_meeting_total` (suggested in story brief) does NOT exist; runbook does not cite it |
| `mh_webtransport_connections_total` | `crates/mh-service/src/observability/metrics.rs:162-167` | labels: `status` (accepted/rejected/error) |
| `mh_webtransport_handshake_duration_seconds` | `crates/mh-service/src/observability/metrics.rs:171-176` | histogram |
| `mh_active_connections` | `crates/mh-service/src/observability/metrics.rs:178-184` | gauge — does NOT count provisional connections (called out in MH Sc 13) |
| `mh_jwt_validations_total` | `crates/mh-service/src/observability/metrics.rs:223-240` | labels: `result`, `token_type`, `failure_reason` |
| `mh_mc_notifications_total` | `crates/mh-service/src/observability/metrics.rs:209-221` | labels: `event_type`, `status` |
| `mc_register_meeting_total` + `mc_register_meeting_duration_seconds` | `crates/mc-service/src/observability/metrics.rs:340-347` | labels (counter): `status` |
| `mc_mh_notifications_received_total` | `crates/mc-service/src/observability/metrics.rs:355-368` | label: `event_type` only — no `status`, no `source_id` |
| `mc_media_connection_failures_total` | `crates/mc-service/src/observability/metrics.rs:372-386` | label: `all_failed` (string "true"/"false") |
| `MCMediaConnectionAllFailed` alert | `infra/docker/prometheus/rules/mc-alerts.yaml` | annotation `runbook_url` already pointed at `#scenario-11-media-connection-failures` — anchor matches |
| `MAX_REGISTER_ATTEMPTS=3`, backoffs `[1s, 2s]` | `crates/mc-service/src/webtransport/connection.rs:42-45` | cited as "3 attempts with 1s/2s backoffs" |
| `register_meeting_timeout_seconds` (default 15) | `crates/mh-service/src/config.rs:108` | env var `MH_REGISTER_MEETING_TIMEOUT_SECONDS` |
| `MAX_CONNECTIONS_PER_MEETING=1000` | `crates/mc-service/src/mh_connection_registry.rs:26` | cited in MC Sc 13 root cause #3 |
| MH WT bind default `0.0.0.0:4434` | `crates/mh-service/src/config.rs:27` | UDP/4434 in MH Sc 14 service-port checks |

---

## Files Modified

| Path | Lines | Change |
|---|---|---|
| `docs/runbooks/mh-incident-response.md` | TOC entries +2; Last Updated → 2026-05-01; appended Sc 13 (~80 lines) and Sc 14 (~95 lines) before `## Diagnostic Commands` | content addition |
| `docs/runbooks/mc-incident-response.md` | TOC entries +3; Last Updated → 2026-05-01; Version History entry +1; appended Sc 11 (~110 lines), Sc 12 (~110 lines), Sc 13 (~140 lines) before `## Diagnostic Commands` | content addition |
| `docs/user-stories/2026-04-12-mh-quic-connection.md` | Devloop Tracking row 17 only | filled in path + status |
| `docs/devloop-outputs/2026-05-01-mh-quic-runbooks/main.md` | Planning, Implementation Summary, Files Modified, Tech Debt, Devloop Verification Steps | this devloop's metadata |

No code, schema, infra, or alert-rule changes. No GSA paths touched.

---

## Devloop Verification Steps

1. **Anchor / link integrity**: TOCs in both runbook files reference scenarios that now exist. `MCMediaConnectionAllFailed` alert annotation in `infra/docker/prometheus/rules/mc-alerts.yaml` points at `#scenario-11-media-connection-failures` — matches the new MC Sc 11 anchor exactly.
2. **Metric-name fidelity**: every metric name and label cited in the new scenarios was grep-verified against the actual emission site in `crates/{mh,mc}-service/src/observability/metrics.rs`. The story brief's `mh_register_meeting_total` was identified as nonexistent and replaced with `mh_register_meeting_timeouts_total`.
3. **Structure parity**: each new scenario follows the existing template's section order. No new top-level sections introduced; only TOC + Last Updated + Version History minor edits in addition to scenario content.
4. **Cross-link integrity**: every `(other-runbook.md#anchor)` link in the new scenarios was checked against the anchor it targets:
   - MC Sc 11 → MH Sc 5 (existing), MH Sc 2 (existing), MH Sc 13 (new in this devloop), MC Sc 12 (new).
   - MC Sc 12 → MH Sc 1 (existing), MH Sc 2 (existing), MH Sc 13 (new), MC Sc 1 (existing), MC Sc 10 (existing).
   - MC Sc 13 → no cross-runbook links (intentional; security branch is self-contained).
   - MH Sc 13 → MC Sc 12 (new), MC Sc 1 (existing).
   - MH Sc 14 → MH Sc 1 (existing).
5. **No mock/test scaffolding leaked**: scenarios reference production metric names and `kubectl` workflows only — no `MetricAssertion` / `TestRecorder` mentions (those are test-side primitives).

---

## Code Review Results

To be filled in during Gate 2 / reviewer findings.

---

## Tech Debt

1. **No `mh-deployment.md`**. The R-34 ask in the user story labelled the deliverable "MH deployment runbook" but every sub-bullet was a runtime failure mode. Skipping a new file avoided an ~80%-boilerplate clone of `mc-deployment.md`. If MH-specific deploy workflow emerges (TLS rotation playbook beyond what `mh-incident-response.md` Recovery Procedures currently cover, GC-coordinated drain handoff, multi-region cutover), file a follow-up devloop. Tracking debt here so it is not silently dropped. **For the future devloop**: the env vars introduced by this story that the deploy runbook will need to document are `MH_REGISTER_MEETING_TIMEOUT_SECONDS` (default 15s; security boundary, do not tune at runtime — see MH Sc 13 Recovery), `AC_JWKS_URL` (JWKS endpoint for MH JWT validation), and `MH_WEBTRANSPORT_BIND_ADDRESS` (default `0.0.0.0:4434`; covered in MH Sc 14). Use `mc-deployment.md` as the structural template.
2. **`mc_mh_notifications_received_total` lacks a `source_id` label.** MC Sc 13's tampering branch needs to attribute notifications to a specific calling MH service identity, but the metric today only has `event_type`. Runbook routes oncall to gRPC handler logs (target `mc.grpc.media_coordination`) and the auth interceptor's caller-identity emission as the attribution source. If the security signal becomes load-bearing, observability would need to add a bounded `source_id` (or `source_handler_id`) label — that is an observability scope change, not an operations change.
3. **No alert for MC Sc 12 (RegisterMeeting Coordination Failures).** The runbook documents metric-driven triage on `mc_register_meeting_total{status="error"}` rate and `mc_register_meeting_duration_seconds` p95. If an alert is desired, it would be an observability scope addition — out of scope for this operations-only devloop.
4. **Runbook style-guide candidates** (DRY observations, not action items): the `failure_reason` JWKS triage flow now appears in 5+ scenarios across `mc-incident-response.md` and `mh-incident-response.md` — adding a 6th would warrant extracting it into a shared appendix. The `clamp_min(rate(... [1h]), 0.001)` baseline-ratio idiom is becoming a project-wide template (used in MH Sc 13 and MC Sc 12) and could be canonicalized in a runbook style guide alongside the existing PromQL-shape guidance.

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit: `f88a240ee127825444999b246b032535b31cde7f`
2. Review all changes: `git diff f88a240..HEAD`
3. Soft reset (preserves changes): `git reset --soft f88a240`
4. Hard reset (clean revert): `git reset --hard f88a240`
5. Docs-only changes — no schema/migration/infra rollback needed.

---

## Issues Encountered & Resolutions

### Issue 1: Layer A scope-drift guard failed at first Gate 2 run
**Problem**: The Cross-Boundary Classification table was authored with glob patterns and parenthetical qualifiers (`docs/runbooks/mh-*.md (new and/or existing)`, `... (Devloop Tracking row 17 only)`). The guard expects literal file paths — the parens and globs caused `scope-drift-inbound` violations on the literal diff paths plus `scope-drift-planned-untouched` on the parenthetical entries.
**Resolution**: Replaced with literal paths (`docs/runbooks/mh-incident-response.md`, `docs/runbooks/mc-incident-response.md`, `docs/user-stories/2026-04-12-mh-quic-connection.md`). Re-ran guards: 22/22 passed. No content change implied.

### Issue 2: User-story brief cited a metric (`mh_register_meeting_total`) that doesn't exist in source
**Problem**: The setup brief in `main.md` Context for Implementer listed `mh_register_meeting_total` among the authoritative metrics. Implementer's pre-write grep against `crates/mh-service/src/observability/metrics.rs` confirmed the metric does not exist; only `mh_register_meeting_timeouts_total` is emitted on the MH side.
**Resolution**: Implementer cited `mh_register_meeting_timeouts_total` plus `mh_grpc_requests_total{method="register_meeting", status}` (the receipt-side counter, which DOES exist) for MH Sc 13's "registrations actually arriving" signal. Planning section flagged the correction with a "verified 2026-05-01" line. Reinforces the operations-INDEX warning that runbook PromQL diverges silently when production wrapper signatures are not byte-checked.

---

## Lessons Learned

1. **Cross-Boundary Classification table requires literal paths.** Globs and parentheticals in the table look natural but break the Layer A scope-drift guard. Author rows as `docs/path/to/file.ext` only; if the file doesn't exist yet at plan time, list its eventual path.
2. **Authoritative metric lists in story briefs are still claims, not facts.** The brief's metric list was overly optimistic — `mh_register_meeting_total` looked symmetric with the MH timeout counter but was never emitted. Pre-write grep against the actual emission site is non-negotiable for runbook content; runbook PromQL examples reference metric names by string and silent drift only surfaces during a real incident.
3. **Runtime-vs-deploy split should drive runbook placement.** The R-34 wording said "MH deployment runbook" but every sub-bullet was a runtime failure mode. Following the existing pattern (`{service}-deployment.md` for deploy workflow, `{service}-incident-response.md` for runtime scenarios) and skipping a new deploy file kept the runbook surface area honest. The deferred deploy file is logged as Tech Debt with a concrete env-var checklist for the future devloop.
4. **Bidirectional cross-references reduce restatement.** MH Sc 13 ↔ MC Sc 12 (RegisterMeeting two-sided), MH Sc 10 ↔ MC Sc 13 (notifications two-sided), MC Sc 11 → MH Sc 5/2/13 (downstream user impact funneling to upstream MH causes) — DRY review confirmed this kept the new content additive rather than duplicative. The asymmetric "local symptoms each side, cross-link the other" pattern is worth keeping as a runbook convention.
5. **First-emission `# Note:` blocks pre-empt 3 AM "is this an incident?" confusion.** New metrics that may sit at zero for weeks (`mh_register_meeting_timeouts_total`, `mc_media_connection_failures_total{all_failed="true"}`) get an explicit note that the rate, not the existence of the series, is the actionable signal. Pattern lifted from GC Sc 5.
6. **ADR-0031 canonical lowercase severity is the new convention; old Title Case in MC Sc 1-10 is the precedent, not the standard.** Code-reviewer initially flagged the new lowercase as drift; Version History entry now documents the deliberate divergence so a future "normalize" PR doesn't drag the new content backward to the old precedent.
