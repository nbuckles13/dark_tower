# Devloop Output: Post-Deploy Monitoring Checklist (MH WebTransport + MC↔MH Coordination)

**Date**: 2026-05-01
**Task**: User story task 18 — post-deploy monitoring checklist for MH WebTransport + MC↔MH coordination metrics with 30-min/2-hour/4-hour/24-hour windows and rollback criteria
**Specialist**: operations
**Mode**: Agent Teams (full)
**Branch**: `feature/mh-quic-post-deploy`
**User Story**: `docs/user-stories/2026-04-12-mh-quic-connection.md` task 18, requirement R-36

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `f88a240ee127825444999b246b032535b31cde7f` |
| Branch | `feature/mh-quic-post-deploy` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@mh-quic-post-deploy-checklist` |
| Implementing Specialist | `operations` |
| Iteration | `1` |
| Security | CLEAR (re-confirmed post-L4) |
| Test | ACCEPT (re-confirmed post-L4) |
| Observability | APPROVED (post-L4 fixes — `or vector(0)` removed, alert-style guards in rollback) |
| Code Quality | APPROVED (re-confirmed post-L4 + post-DRY-dedup) |
| DRY | RESOLVED (1 finding, fixed — verbatim PromQL duplications removed from MC addendum) |
| Operations | `n/a (implementer)` |
| Gate 1 | `passed (classification-sanity guard OK; all 5 reviewers confirmed)` |
| Gate 2 | `passed (22/22 guards green; layers 1/2/4-8 N/A for docs-only)` |
| Gate 3 | `passed (all 5 verdicts CLEAR/APPROVED/RESOLVED; 1 review-iteration to fix observability L4 + dry-reviewer L4 findings)` |

---

## Task Overview

### Objective
Add a post-deploy monitoring checklist that an on-call engineer follows after deploying the MH QUIC story. The checklist must cover 30-min, 2-hour, 4-hour, and 24-hour windows and define explicit rollback criteria.

### Required Coverage (R-36)
The checklist must check (per user story §operations):
- MH WebTransport handshake success rate >95%
- MH JWT validation success rate >99%
- MH RegisterMeeting timeout count = 0
- MC RegisterMeeting success rate >95%
- MH→MC notification delivery success rate >95%
- MH active connections gauge non-zero
- MC `MediaConnectionFailed(all_failed=true)` count = 0

### Required Rollback Criteria (per user story §operations)
- MH WebTransport failure >10% for 10m
- JWT validation failure >20% for 5m
- RegisterMeeting timeouts >0 sustained for 10m

### Scope
- **Service(s)**: docs only — runbooks for mh-service and mc-service
- **Schema**: No
- **Cross-cutting**: No (runbook docs are operations-owned)

### Debate Decision
NOT NEEDED — this is documentation work that follows the established post-deploy checklist pattern (see `docs/runbooks/mc-deployment.md` §Post-Deploy Monitoring Checklist: Join Flow as the precedent set by the meeting-join story).

---

## Cross-Boundary Classification

Per ADR-0024 §6.2, both files modified are operations-owned runbooks under `docs/runbooks/**`.

| Path | Classification | Owner (if not mine) | Notes |
|------|----------------|---------------------|-------|
| `docs/runbooks/mh-deployment.md` | Mine | — | New file |
| `docs/runbooks/mc-deployment.md` | Mine | — | Additive section only (no edits to existing content) |
| `docs/devloop-outputs/2026-05-01-mh-quic-post-deploy-checklist/main.md` | Mine | — | Devloop output |

No code, manifests, alerts, dashboards, protos, migrations, or tests are touched. Pure docs change owned by operations.

---

## Planning

### Location decision (Option a — chosen)

Create `docs/runbooks/mh-deployment.md` (new) with the full post-deploy checklist and a thin deployment-procedure stub, AND add a brief MC↔MH coordination addendum to `mc-deployment.md` §"Post-Deploy Monitoring Checklist" that points to it.

**Why:** the join-flow precedent at `mc-deployment.md:856-911` lives in the deployment runbook because that is where on-call goes after a deploy. R-34 (the prior task 17) already established the need for an MH-side deployment runbook, and the post-deploy checklist for an MH-WebTransport-touching deployment will be looked up from the MH side: the engineer is deploying mh-service. We then mirror the join-flow precedent by adding a one-paragraph cross-pointer in `mc-deployment.md` so an engineer doing the MC half of the coordination story (e.g. RegisterMeeting client changes) can find the same checklist. This avoids duplicating the checklist in two places (DRY) while keeping it where it will actually be looked up (ergonomics).

Option (b) — adding the checklist to `mh-incident-response.md` — was rejected: incident-response runbooks are for "alarm is firing now"; they are not where on-call goes proactively in a 30-min/2-hour/4-hour/24-hour post-deploy window. The precedent is unambiguous.

### Files to touch

1. **`docs/runbooks/mh-deployment.md`** (new file). Minimal scaffold matching the structure of `gc-deployment.md`/`mc-deployment.md`:
   - Title block, owner, last-updated.
   - One-paragraph Overview noting this is the deployment runbook for the MH WebTransport server + MC↔MH coordination story; deeper deployment procedure is deferred to a future runbook iteration (mark as TBD/stub) so the post-deploy checklist landing here is not orphaned.
   - "Post-Deploy Monitoring Checklist: MH WebTransport + MC↔MH Coordination" section — the deliverable. Mirrors the join-flow style: PromQL block, then checkbox list, repeated for 30-min/2-hour/4-hour/24-hour windows, then rollback criteria, then rollback command.
   - Cross-link to `mh-incident-response.md` for active-incident scenarios and `mc-deployment.md` for the MC half of the deploy.

2. **`docs/runbooks/mc-deployment.md`** — additive addendum after the existing "Post-Deploy Monitoring Checklist: Join Flow" subsection: a small "Post-Deploy Monitoring Checklist: MC↔MH Coordination (RegisterMeeting + Notifications)" subsection that:
   - Links to the canonical checklist in `mh-deployment.md`.
   - Calls out the MC-specific checks (`mc_register_meeting_total`, `mc_mh_notifications_received_total`, `mc_media_connection_failures_total`) inline so MC-only engineers can spot-check without flipping files.
   - Names the MC rollback command (`kubectl rollout undo deployment/mc-service`).

3. **`docs/devloop-outputs/2026-05-01-mh-quic-post-deploy-checklist/main.md`** — update Planning, Cross-Boundary Classification, Implementation Summary, Files Modified.

### Metric-name verification (vs the user-story-listed catalog)

I read `crates/mh-service/src/observability/metrics.rs` and `crates/mc-service/src/observability/metrics.rs`. Two name/label discrepancies between what the user story §operations and the team-lead brief described and what the emitter actually emits:

| Brief / story said | Real emitter | Resolution |
|---|---|---|
| `mc_register_meeting_total{status="success\|failure"}` | `mc_register_meeting_total{status="success\|error"}` (call sites: `crates/mc-service/src/grpc/mh_client.rs:136,144,157` use `"success"` / `"error"`) | Use **`status="error"`** in PromQL — this is what the emitter actually writes. |
| `mh_register_meeting_total{status="success\|failure"}` (MH-side success/failure of RegisterMeeting) | NOT EMITTED. MH only emits `mh_register_meeting_timeouts_total` (a counter, no labels). MH RegisterMeeting RPC success/error is observable via the bounded gRPC counter `mh_grpc_requests_total{method="register_meeting", status="success\|error"}`. | The story's R-36 only requires "MH RegisterMeeting timeout count = 0", which IS covered by `mh_register_meeting_timeouts_total`. The fictional `mh_register_meeting_total{status=success\|failure}` was never required by R-36. No action needed beyond using the real timeout counter. Flag this in the implementation summary. |

For the `mh_jwt_validations_total` checks I will use the `result="success\|failure"` label (real emitter labels per `metrics.rs:226-240`), and for MH→MC notification delivery the success rate uses `mh_mc_notifications_total{status="success\|error"}` (the MH-side emitter — the one that actually knows whether the gRPC call succeeded; the MC-side `mc_mh_notifications_received_total` has no `status` label and only reflects what arrived, not what was lost in flight).

### Checklist content sketch (full PromQL drafted in implementation phase)

**30-minute** — primary signals during initial bake:
- `mh_webtransport_connections_total{status="accepted"}` rate vs. total (target >95%, R-26 SLO).
- `mh_jwt_validations_total{result="success"}` rate vs. total (target >99%).
- `mh_register_meeting_timeouts_total` cumulative since deploy (target = 0; rollback if >0 sustained 10m).
- `mc_register_meeting_total{status="success"}` rate vs. total (target >95%).
- `mh_mc_notifications_total{status="success"}` rate vs. total (target >95%).
- `mh_active_connections` gauge (target >0 once traffic flows; this is the proof clients are connecting).
- `mc_media_connection_failures_total{all_failed="true"}` cumulative (target = 0; any non-zero is a P1 incident).
- No new MH or MC alerts firing (`MHHighJwtValidationFailures`, `MHHighWebTransportRejections`, `MHWebTransportHandshakeSlow`, `MCMediaConnectionAllFailed`).

**2-hour** — trend stability check; same signals, "trend not degrading", no pod restarts since deploy.

**4-hour** — alert-clear check + handshake P95 trend not drifting toward SLO boundary (`mh_webtransport_handshake_duration_seconds`).

**24-hour** — long-tail check (new vs join-flow precedent): cumulative timeout/all-failed counts still 0; JWT failure rate has not crept up (token refresh edge cases); no upward trend in WebTransport rejection. This is the window where slow leaks (e.g. JWKS cache eviction interacting with token rotation) show up.

**Rollback criteria** (verbatim from R-36, in PromQL):
- `mh_webtransport_connections_total{status!="accepted"}` rate / total > 10% sustained 10m.
- `mh_jwt_validations_total{result="failure"}` rate / total > 20% sustained 5m.
- `rate(mh_register_meeting_timeouts_total[10m]) > 0` sustained 10m (any timeouts are a coordination break).

**Rollback command**: `kubectl rollout undo deployment/mh-service -n dark-tower` (and the MC equivalent if MC was also rolled). Note that drained MH pods will sever active sessions — clients reconnect via JWT to the rolled-back pod (per assumption 4, MH state is in-memory and reconnect-tolerant).

### Style alignment with precedent

- Same H3 section title pattern (`### Post-Deploy Monitoring Checklist: <flow>`).
- Same PromQL-block-then-checkbox-list rhythm.
- Same `**N-window check:**` heading style.
- Same rollback block with `kubectl rollout undo` and a note about graceful drain semantics.
- 24-hour window added at the end of the cadence per R-36 (the join-flow precedent goes 15-min/1-hour/4-hour; we go 30-min/2-hour/4-hour/24-hour as the user story requires).

### Out of scope (explicitly NOT touched)

- No new alert rules (R-36 uses existing alerts; new alerts are prior tasks).
- No new metrics (R-36 consumes existing emitters; metrics are prior tasks).
- No dashboard panels (those are story tasks 12-13, already shipped — see `infra/grafana/dashboards/mc-overview.json` MC↔MH section).
- No MH incident-response runbook edits (story task 17, already shipped).
- No code, no proto, no migrations, no manifests.

---

## Pre-Work

None.

---

## Implementation Summary

### What was added

**1. New `docs/runbooks/mh-deployment.md`** (canonical post-deploy checklist for the MH QUIC story)

- Title block + thin Overview pointing the reader at `mh-incident-response.md` for active incidents and at `docs/observability/metrics/{mh,mc}-service.md` for metric definitions (per @dry-reviewer's soft-pointer guidance — does not restate metric definitions inline).
- Stub Deployment Procedure section flagging that the full procedure is a follow-up; the post-deploy checklist is the deliverable for R-36.
- "Post-Deploy Monitoring Checklist: MH WebTransport + MC↔MH Coordination" section with four windows (30-min, 2-hour, 4-hour, 24-hour), each in PromQL-block-then-checkbox-list format mirroring the join-flow precedent at `mc-deployment.md:856-911`.
- Rollback criteria block with the three R-36-mandated triggers expressed as resolvable PromQL, plus `kubectl rollout undo` rollback command and a note about MH state being in-memory only (per assumption 4 of the user story).
- Cross-references to companion runbook + metrics catalog + alert rules + ADRs.

**2. Additive section in `docs/runbooks/mc-deployment.md`** (MC↔MH coordination addendum)

- Added immediately after the existing join-flow rollback block, before `## Emergency Contacts`.
- Header: "Post-Deploy Monitoring Checklist: MC↔MH Coordination (RegisterMeeting + Notifications)".
- One-paragraph cross-pointer to the canonical checklist in `mh-deployment.md` (full path + section title, per @code-reviewer point #4 — no bare anchors).
- Three MC-specific PromQL spot-checks (`mc_register_meeting_total`, `mc_mh_notifications_received_total`, `mc_media_connection_failures_total{all_failed="true"}`) so an MC-only engineer can verify the MC half without flipping files. Per @dry-reviewer this is a different audience subset, not duplication.
- MC-side rollback verb (`kubectl rollout undo deployment/mc-service`) and a back-pointer to the MH-side rollback for MH-related issues.

**3. Devloop output `main.md`**

- Updated Cross-Boundary Classification, Planning, and (this) Implementation Summary sections.

### How reviewer feedback was applied

- **@test** (PromQL precision): all "= 0" cumulative checks use `increase(metric[<window>])` (with `[30m]` / `[2h]` / `[4h]` / `[24h]` per window). Rollback timeout trigger is `increase(mh_register_meeting_timeouts_total[10m]) > 0` (not `rate(...) > 0`).
- **@security** (5 items): no JWT/JWKS/PII content; rollback verb is `kubectl rollout undo` only (no auth-bypass toggles); JWT triage cross-references `mh-incident-response.md#scenario-2-jwt-validation-failures` and the bounded `failure_reason` label taxonomy; zero new alert rules added (only references to existing alerts by name).
- **@observability** (window choices, asymmetry, NaN guards, additional 24h slow-leak queries):
  - All ratio re-checks (30-min and 2-hour) use `[5m]` rate windows to match the existing alerts in `infra/docker/prometheus/rules/mh-alerts.yaml` and the join-flow precedent at `mc-deployment.md:864-873`. `[10m]` is reserved for the rollback criteria where R-36 explicitly says "sustained 10m". (Updated post-validation — initial draft used `[10m]` for 2-hour rate windows; @observability flagged that the trend stability check should match the existing alert window, since "less noise" comes from panel duration, not rate window.)
  - Rollback floors honor R-36's explicit `[5m]` (JWT) vs `[10m]` (handshake / timeout) windows verbatim.
  - 24-hour checks use `sum(increase(...[24h]))` for cumulative-zero counters and `[24h]` ratio queries for averaged success rates.
  - Histogram P95: `histogram_quantile(0.95, sum by(le) (rate(_bucket[5m])))` form for handshake duration at the 4-hour mark, matching the existing `MHWebTransportHandshakeSlow` alert at `mh-alerts.yaml:135-149`. (Updated post-validation — initial draft used `[10m]` here; @observability flagged the alert-matching window for consistency.)
  - `sum(mh_active_connections) > 0` aggregates across pods (gauge sums correctly across replicas).
  - Asymmetry between affirmative success-rate gates (`{result="success"}`) and negative rollback floors (`{result="failure"}`) is called out inline at the top of the post-deploy section.
  - Sparse-traffic 30-min divide-by-zero handling: **bug fixed in L4 review**. The initial draft used `(sum(rate(...)) or vector(0))` denominator guards, which @observability flagged as semantically wrong: when no series match, `sum(rate(...))` returns an empty vector and `or vector(0)` falls through to scalar `0`, so `A / 0 = +Inf` and `+Inf > 0.10` triggers a phantom rollback. Fix applied across all ratio queries:
    - **Dashboard ratios (30-min, 2-hour, 24-hour, MC addendum)**: stripped `or vector(0)` guards. Empty denominator now produces a clean "No data" empty vector that Grafana renders correctly. Sparse-traffic note rewritten to set the right operator expectation.
    - **Rollback ratio criteria**: replaced `or vector(0)` with the alert-style `and sum(rate(...)) > 0` guard, mirroring `MHHighWebTransportRejections` at `mh-alerts.yaml:115-123`. The whole expression becomes empty (= no rollback signal) under no traffic, instead of producing phantom `+Inf > 0.10`.
    - Added a "Why no `or vector(0)` denominator guard?" callout near the top of the post-deploy section so a future maintainer doesn't re-introduce the bug.
  - **L4 nit (Finding 2)**: timeout rollback PromQL changed from `increase(mh_register_meeting_timeouts_total[10m]) > 0` to `sum(increase(mh_register_meeting_timeouts_total[10m])) > 0` for consistency with every other counter check in the document and forward-compatibility with future per-pod label splits.
  - **L4 polish (Finding 3)**: added a "Rollback applies throughout" callout next to the Asymmetry note, making explicit that rollback criteria remain authoritative across all four windows (not just the 30-min check where they're closest in proximity).
  - **Two beyond-R-36 latency-trend queries added at the 24-hour mark**, labelled "*Additional (beyond R-36)*" so the scope-guard understands they are observability-rationale additions, not story-required: `mh_webtransport_handshake_duration_seconds` P95 over trailing-1h, and `mc_register_meeting_duration_seconds` P95 over trailing-1h. Rationale (per @observability): catches gradual JWKS/connection-pool/RegisterMeeting-RPC drift before the timeout/rejection counters fire — leading-indicator slow-leak detection.
- **@code-reviewer** (4 items):
  - **#1** ADR-0029 alignment: ratios use `rate/rate` (Category B), zero-counters use `increase` (Category A), exactly mirroring the join-flow precedent.
  - **#2** `all_failed="true"` quoted-string label syntax preserved (verified against emitter at `crates/mc-service/src/observability/metrics.rs:381-385`).
  - **#3** R-26 cite was wrong — verified via grep that R-26 is the metric-plumbing requirement (defines counters/histograms/gauges, no SLO target), and R-36 is where the 95%/99% SLO targets live. Implementation cites "R-36 §operations" for SLO targets and `mh-alerts.yaml` for alert thresholds, never R-26 for SLO numbers.
  - **#4** Cross-references use full path + section title (e.g. `docs/runbooks/mh-deployment.md §"Post-Deploy Monitoring Checklist: MH WebTransport + MC↔MH Coordination"`) in both directions, no bare `#anchors`.
- **@dry-reviewer** (soft-pointer + L4 deduplication finding): metric-definition prose links to `docs/observability/metrics/{mh,mc}-service.md` rather than restating what each metric measures inline. PromQL queries themselves (deployment-specific) are inlined. **L4 finding fixed**: two PromQL blocks were verbatim copies between the canonical MH runbook and the MC addendum (`mc_register_meeting_total` success-rate ratio and `mc_media_connection_failures_total{all_failed="true"}` 30m-increase). Replaced both with bullet-pointers back to the canonical MH-runbook query (preserving the emitter-label note for `status="success|error"` so the rot-prevention comment stays close to the MC engineer's eyes). The genuinely MC-only `mc_mh_notifications_received_total` rate-by-event_type query stays inline since it has no MH-side equivalent. All four MC-specific checklist items kept; rollback paragraph kept; intro paragraph kept.

### Metric-name discrepancies between user-story brief and real emitters (resolved)

| Brief said | Real emitter | Resolution |
|---|---|---|
| `mc_register_meeting_total{status="success\|failure"}` | `mc_register_meeting_total{status="success\|error"}` (call sites at `crates/mc-service/src/grpc/mh_client.rs:136,144,157`) | Used `status="success"` / `status="error"` in PromQL. Inline footnote in MC addendum so future readers don't re-introduce `failure`. |
| `mh_register_meeting_total{status="success\|failure"}` (MH-side success/failure) | NOT EMITTED. MH only emits `mh_register_meeting_timeouts_total` (no labels). | R-36 only requires "timeout count = 0" — covered by `mh_register_meeting_timeouts_total`. The fictional `mh_register_meeting_total{status=...}` was never required. No PromQL for it; flagged here for the implementation record. |

### What was NOT touched (out of scope per plan)

- No new metrics, alerts, dashboards, code, proto, manifests, migrations, or tests.
- No edits to `mh-incident-response.md` (story task 17, already shipped).

---

## Files Modified

| Path | Change |
|---|---|
| `docs/runbooks/mh-deployment.md` | New file — title block, stub deployment-procedure section, full post-deploy checklist (4 windows + rollback), references. |
| `docs/runbooks/mc-deployment.md` | Additive section (Post-Deploy Monitoring Checklist: MC↔MH Coordination) inserted after the existing join-flow checklist, before `## Emergency Contacts`. No edits to existing content. |
| `docs/devloop-outputs/2026-05-01-mh-quic-post-deploy-checklist/main.md` | Filled Planning, Cross-Boundary Classification, Implementation Summary, Files Modified. |

---

## Devloop Verification Steps

### Layer 1: cargo check
**Status**: N/A (docs-only — no Rust files touched)

### Layer 2: cargo fmt
**Status**: N/A (docs-only)

### Layer 3: Simple Guards
**Status**: ALL PASS (22/22)
**Duration**: ~10s
**Notes**: Layer A scope-drift initially failed because the Cross-Boundary Classification table had path entries `docs/runbooks/mh-deployment.md (new)` and `docs/runbooks/mc-deployment.md (additive section)` — the parenthetical annotations broke literal-path matching against `git diff --name-only`. Fixed by moving annotations to a new "Notes" column. After fix: 22/22.

| Guard | Status |
|-------|--------|
| grafana-datasources | PASS |
| instrument-skip-all | PASS |
| no-hardcoded-secrets | PASS |
| no-pii-in-logs | PASS |
| no-secrets-in-logs | PASS |
| test-coverage | PASS |
| test-registration | PASS |
| test-rigidity | PASS |
| validate-alert-rules | PASS |
| validate-application-metrics | PASS |
| validate-cross-boundary-classification | PASS |
| validate-cross-boundary-scope | PASS (after path-format fix) |
| validate-dashboard-panels | PASS |
| validate-env-config | PASS |
| validate-gsa-sync | PASS |
| validate-histogram-buckets | PASS |
| validate-infrastructure-metrics | PASS |
| validate-knowledge-index | PASS |
| validate-kustomize | PASS |
| validate-metric-coverage | PASS |
| validate-metric-labels | PASS |

### Layer 4: Unit Tests
**Status**: N/A (docs-only)

### Layer 5: All Tests (Integration)
**Status**: N/A (docs-only)

### Layer 6: Clippy
**Status**: N/A (docs-only)

### Layer 7: Semantic Guards
**Status**: N/A (docs-only — no Rust diff for credential-leak / actor-blocking / error-context analysis)

### Layer 8: Env-tests (Integration)
**Status**: N/A (docs-only — zero Rust/proto/manifest changes; no integration-test surface affected)

---

## Code Review Results

### Security Specialist
**Verdict**: CLEAR (re-confirmed post-L4)
**Findings**: 0

No production credentials, no PII, no JWT/JWKS material in shell snippets, kubectl inventory limited to non-mutating + rollback-undo, no auth-bypass emergency toggles, JWT triage cross-references the bounded `failure_reason` label taxonomy in `mh-incident-response.md` Scenario 2. Bonus call-out: the `mh-deployment.md:52` line ("do NOT remove those guards if you adapt the queries elsewhere") prevents a future maintainer from reintroducing the `or vector(0)` antipattern, protecting rollback semantics.

### Test Specialist
**Verdict**: ACCEPT (re-confirmed post-L4)
**Findings**: 0 blocking, 1 nit at planning (applied)

Every metric name + label value pair binds to a real, currently-emitted counter/histogram/gauge in `crates/{mh,mc}-service/src/observability/metrics.rs`. All four cited alerts (`MHHighJwtValidationFailures`, `MHHighWebTransportRejections`, `MHWebTransportHandshakeSlow`, `MCMediaConnectionAllFailed`) resolve in the actual alert-rules files. Test code impact: zero — no new emitters, ADR-0032 metric-coverage debt unchanged.

### Observability Specialist
**Verdict**: APPROVED (after L4 review iteration)
**Findings**: 1 BLOCKING bug + 2 nits, all 3 fixed + bonus

L4 review caught the `(sum(rate(...)) or vector(0))` denominator-guard antipattern: under no traffic, `sum(rate(...))` returns an empty vector and `or vector(0)` collapses to scalar `0`, making `A / 0 = +Inf` and triggering phantom rollback (`+Inf > 0.10` is true). Fixed by:
- Stripping `or vector(0)` from dashboard ratio queries — empty denominator now produces clean "No data" empty vector.
- Replacing `or vector(0)` in rollback ratio queries with the alert-style `and sum(rate(...)) > 0` guard, mirroring `MHHighWebTransportRejections` at `mh-alerts.yaml:115-123`.
- Adding "Why no `or vector(0)` denominator guard?" callout near the top of the post-deploy section to prevent re-introduction.

Bonus: same antipattern in `mc-deployment.md` MC addendum was fixed without prompting. Two related nits also applied: timeout rollback uses `sum(increase(...))` for forward-compat with future per-pod label splits; "Rollback applies throughout" callout added next to the asymmetry note.

### Code Quality Reviewer
**Verdict**: APPROVED (re-confirmed post-L4 + post-DRY-dedup)
**Findings**: 0 blocking, 4 plan-stage items (all applied)

ADR Compliance:
- **ADR-0011**: PASS — metric names follow `<service>_<noun>_<unit>` convention, no high-cardinality labels introduced.
- **ADR-0019 (DRY)**: PASS (strengthened by post-fix dedup) — canonical thresholds + queries live in `mh-deployment.md` only; MC addendum carries pointers + emitter-label rot-prevention note + the genuinely MC-only `mc_mh_notifications_received_total` query.
- **ADR-0024 §6 (Cross-boundary)**: PASS — both touched paths Mine; mc-deployment.md edit verified additive only.
- **ADR-0029 (Counters vs rates)**: PASS — Category A `increase()` for discrete-event "= 0" checks, Category B `rate()/rate()` for ratios, exact mirror of join-flow precedent.

Ownership Lens: both touched paths operations-owned (`docs/runbooks/**`); Mine-only; no cross-team prose touched. ADR-0024 §6.2 satisfied.

### DRY Reviewer
**Verdict**: RESOLVED (1 true-duplication finding — fixed)

**True duplication finding** (entered fix-or-defer flow, fixed):
Two PromQL blocks were verbatim copies between `mh-deployment.md` and the MC addendum in `mc-deployment.md` (the `mc_register_meeting_total{status="success"}` success-rate ratio and the `mc_media_connection_failures_total{all_failed="true"}[30m]` increase). Fixed: removed both duplicate blocks from MC, replaced with bullet-pointers to the canonical queries in `mh-deployment.md`. Added durable maintainer note ("Do not duplicate the queries here; thresholds and emitter-label conventions are owned in one place to avoid silent divergence") at `mc-deployment.md:921`. Cross-section pointers use verbatim section titles in both directions.

**Extraction opportunities** (tech debt observations — non-blocking):
The "asymmetry note" / "sparse-traffic note" / "Why no `or vector(0)` denominator guard?" trio at `mh-deployment.md:48-54` is excellent single-source PromQL guidance. Don't extract yet — premature. Extract trigger: a third post-deploy runbook section (e.g. future `gc-` or `ac-` post-deploy) needing the same guidance — factor into `docs/observability/dashboard-conventions.md` at that point.

---

## Tech Debt

### Deferred Findings

No deferred findings. All findings either fixed inline or were tech-debt observations (DRY extraction opportunity below).

### Cross-Service Duplication (from DRY Reviewer)

| Pattern | New Location | Existing Location | Follow-up Task |
|---------|--------------|-------------------|----------------|
| Asymmetry / sparse-traffic / `or vector(0)` PromQL guidance trio | `docs/runbooks/mh-deployment.md:48-54` | (single occurrence) | Extract to `docs/observability/dashboard-conventions.md` if a third post-deploy runbook (`gc-` or `ac-` post-deploy) needs the same guidance. Trigger: third occurrence. Not blocking. |

### Temporary Code (from Code Reviewer)

No temporary code (docs-only).

### Stub follow-up

`docs/runbooks/mh-deployment.md` includes a thin "Deployment Procedure" stub (placeholder for a future iteration covering rolling deployment commands, pre-flight checks, etc.). The Post-Deploy Monitoring Checklist — the deliverable for R-36 — is complete and not blocked by the stub. Follow-up: a separate operations devloop will fill in the deployment-procedure body alongside the runbook iteration that covers the deployment scenarios beyond R-36's scope.

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `f88a240ee127825444999b246b032535b31cde7f`
2. Review all changes: `git diff f88a240ee127825444999b246b032535b31cde7f..HEAD`
3. Soft reset (preserves changes): `git reset --soft f88a240ee127825444999b246b032535b31cde7f`
4. Hard reset (clean revert): `git reset --hard f88a240ee127825444999b246b032535b31cde7f`

This devloop is docs-only; no schema, manifest, or runtime impact.

---

## Issues Encountered & Resolutions

### Issue 1: Layer A scope-drift guard rejected the Cross-Boundary Classification table format

**Problem**: First Gate-2 run failed `validate-cross-boundary-scope.sh` with 4 violations. The table had path entries `docs/runbooks/mh-deployment.md (new)` and `docs/runbooks/mc-deployment.md (additive section)`. The guard parses column 1 as a literal path and won't strip parenthetical annotations, so the entries didn't match the bare paths from `git diff --name-only`.

**Resolution**: Moved the `(new)` / `(additive section)` annotations to a new "Notes" column (parser-invisible — the guard reads cells 2-4). Path cells now match the diff entries exactly. Re-ran guards: 22/22 pass.

### Issue 2: Brief metric names did not match real emitters

**Problem**: The user-story brief (and team-lead kickoff message) listed `mc_register_meeting_total{status="success|failure"}` and `mh_register_meeting_total{status="success|failure"}`. The MC-side emitter actually uses `status="success|error"` (verified at `crates/mc-service/src/grpc/mh_client.rs:136,144,157`), and the MH-side `mh_register_meeting_total` does NOT exist as an emitter at all — only `mh_register_meeting_timeouts_total` does.

**Resolution**: Used the real emitter names in PromQL throughout. Added an inline emitter-label note at MC's bullet pointer in the addendum (`status="success|error"` not `failure`) so the rot-prevention comment lives at MC engineer's eye level. Documented both discrepancies in §"Metric-name discrepancies between user-story brief and real emitters" above. R-36's "MH RegisterMeeting timeout count = 0" is satisfied by `mh_register_meeting_timeouts_total` directly — the fictional `mh_register_meeting_total` was never required.

### Issue 3: Phantom-rollback bug from `or vector(0)` denominator guard (caught at L4 review)

**Problem**: First implementation used `(sum(rate(...)) or vector(0))` as a sparse-traffic divide-by-zero guard. Observability reviewer flagged at L4: when no series match, `sum(rate(...))` returns an empty vector, `or vector(0)` falls through to scalar `0`, and the division becomes `A / 0 = +Inf`. Then `+Inf > 0.10` evaluates true under low-traffic conditions, triggering phantom rollback on the first rejected connection.

**Resolution**: Two-track fix:
- **Dashboard ratio queries** (30-min, 2-hour, 24-hour, MC addendum): stripped `or vector(0)` entirely. Empty denominator now produces a clean "No data" empty vector that Grafana renders correctly. Sparse-traffic note rewritten to set the right operator expectation ("treat as 'no traffic yet, re-run in a few minutes'").
- **Rollback ratio queries**: replaced `or vector(0)` with the alert-style `and sum(rate(...)) > 0` guard, mirroring `MHHighWebTransportRejections` at `mh-alerts.yaml:115-123`. Whole expression becomes empty (no rollback signal) under no-traffic, instead of producing phantom `+Inf > 0.10`.
- Added "Why no `or vector(0)` denominator guard?" callout at `mh-deployment.md:54` to prevent re-introduction.

### Issue 4: Verbatim PromQL duplication between mh-deployment.md and mc-deployment.md addendum (caught at L4 review)

**Problem**: DRY reviewer flagged at L4 that two PromQL blocks were verbatim copies between the canonical `mh-deployment.md` checklist and the `mc-deployment.md` MC addendum: the `mc_register_meeting_total{status="success"}` success-rate ratio and the `mc_media_connection_failures_total{all_failed="true"}[30m]` increase. Real rot risk: a threshold tuning in one file would silently diverge from the other.

**Resolution**: Removed both duplicate PromQL blocks from the MC addendum, replaced with bullet-pointers to the canonical queries in `mh-deployment.md`. Kept inline: emitter-label rot-prevention note (MC-specific forensic breadcrumb), genuinely MC-only `mc_mh_notifications_received_total` query (no MH-side equivalent), MC-specific checklist items, intro and rollback paragraphs. Added durable maintainer note at `mc-deployment.md:921` ("Do not duplicate the queries here; thresholds and emitter-label conventions are owned in one place to avoid silent divergence").

---

## Lessons Learned

1. **`or vector(0)` is a footgun in rollback contexts**, even though it works fine in dashboard contexts. The asymmetry comes from the comparator: a dashboard panel renders `+Inf` as "No data" and the operator moves on; a rollback comparator evaluates `+Inf > threshold` as `true` and fires. The alert-rule pattern (`and sum(rate(...)) > 0`) gets this right and should be the default for any "ratio compared against threshold" query that drives an automated action. Documented inline in the runbook so the bug doesn't get re-introduced.

2. **Cross-boundary classification table parsing is exact-match**: don't add parenthetical context (`(new)`, `(additive)`) inside the path column. Either keep paths bare and use a Notes column, or stick to backticks-only — the Layer A guard can strip those. (`scripts/guards/common.sh:368` `parse_cross_boundary_table` is the source of truth for the parse rule.)

3. **Emitter ground truth wins over user-story or team-lead phrasing**: when a brief says `status="success|failure"` and the emitter writes `status="success|error"`, use the emitter form and footnote the discrepancy. Saved one round of reviewer findings here by catching this at planning.

4. **The MC-addendum-as-spot-check pattern is right** for cross-service deploys (audience is the MC-half engineer, not the canonical-checklist consumer), but **only if MC-specific PromQL stays MC-only**. Verbatim copies of the canonical queries don't survive their first threshold tuning. The fix is bullet-pointers + emitter-label note + genuinely-MC-only queries — the audience benefit is preserved without the rot.

5. **Observability + DRY reviewers both surfaced post-validation findings that improved the deliverable**: a non-trivial bug (phantom rollback) and a non-trivial DRY violation (verbatim PromQL). Worth the iteration. The pattern of "applied during review" trio (BLOCKING + 2 nits + bonus) is exactly what a fix-or-defer triage round should produce — no escalations, no deferrals.

6. **The 24-hour bake window catches different signals than 30-min/2-hour/4-hour**. R-36 didn't elaborate on why; the implementer added latency-trend P95 queries (`*Additional (beyond R-36)*`) at the 24h mark on observability's recommendation to detect slow JWKS-cache or connection-pool drift before the timeout counter fires. Marking these as `*Additional (beyond R-36)*` keeps scope-fidelity reviewers happy and lets future maintainers tell baseline-required from operationally-prudent.
