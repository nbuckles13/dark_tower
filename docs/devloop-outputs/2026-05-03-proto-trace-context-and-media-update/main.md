# Devloop Output: Proto trace-context envelope fields + MediaConnectionUpdate redesign + buf.yaml

**Date**: 2026-05-03
**Task**: Proto changes for browser-client-join story Task #2 — R-5 (trace_parent/trace_state on ClientMessage + ServerMessage), R-58 (same on MhClientMessage envelope), R-60 (replace MediaConnectionFailed with MediaConnectionUpdate{repeated MhConnectionStatus} + ConnectionState enum), and create proto/buf.yaml.
**Specialist**: protocol
**Mode**: Agent Teams (full)
**Branch**: `feature/browser-client-join-task2`
**Duration**: TBD

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `11942361908303ce60e7a9053597a981b059efcf` |
| Branch | `feature/browser-client-join-task2` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@devloop-2026-05-03-proto-trace` |
| Implementing Specialist | `protocol` |
| Iteration | `1` |
| Security | `CLEAR` |
| Test | `CLEAR` |
| Observability | `CLEAR` |
| Code Quality | `RESOLVED` |
| DRY | `CLEAR` |
| Operations | `CLEAR` |

---

## Task Overview

### Objective
Land the proto-layer changes that the rest of the browser-client-join story depends on:
- **R-5**: Add `string trace_parent = 20;` and `string trace_state = 21;` to `ClientMessage` and `ServerMessage` envelopes in `proto/signaling.proto`. W3C Trace Context format. Optional/empty when no active trace. Field tags 20/21 are unused today (verified). Wire-additive.
- **R-58**: Add the same `trace_parent = 20` / `trace_state = 21` fields to the `MhClientMessage` envelope. Verified unallocated on `MhClientMessage` (envelope currently allocates only `oneof message = 1`). Wire-additive, proto3 default-empty.
- **R-60 redesign**: Remove the `media_connection_failed = 11` `oneof` variant on `ClientMessage` and the `MediaConnectionFailed` message. Add `MediaConnectionUpdate { repeated MhConnectionStatus statuses; }` + `MhConnectionStatus { string mh_url = 1; ConnectionState state = 2; optional string failure_reason = 3; optional string failure_code = 4; google.protobuf.Timestamp observed_at = 5; }` + `enum ConnectionState { CONNECTION_STATE_UNSPECIFIED = 0; CONNECTION_STATE_CONNECTED = 1; CONNECTION_STATE_FAILED = 2; CONNECTION_STATE_DISCONNECTED = 3; }`. New `oneof` variant `media_connection_update = 11` reuses the freed tag. Wire-breaking from predecessor by design — no on-wire clients exist outside this codebase per Clarification Question 9.
- **buf**: Create `proto/buf.yaml` (v2, `STANDARD` lint + `WIRE_JSON` breaking; module rooted in `proto/`, not repo root). Run `buf lint` locally; `buf breaking` is expected to flag R-60 — that's the approved-and-documented one-time wire break in this commit.

### Scope
- **Service(s)**: `proto/` (Guarded Shared Area — wire format). Downstream consumers (MC service handler, MC metrics, MC tests) WILL break compilation since `MediaConnectionFailed` is referenced. Implementer must decide how to handle the cargo-check failure (see Cross-Boundary Classification below) — Task #6 of the user story is the proper home for the MC handler redesign, but Task #2 must keep the workspace compiling.
- **Schema**: No DB changes.
- **Cross-cutting**: Wire format change. Affects MC service compilation today; future stories rely on these fields.

### Debate Decision
NOT NEEDED — design fully resolved in user story (`docs/user-stories/2026-05-02-browser-client-join.md`) including R-60 redesign in the Design section and Clarification Questions 8, 9, 11. Story-scope debate already happened.

---

## Cross-Boundary Classification

**Revision (2026-05-03)**: Updated per @observability + @code-reviewer + @team-lead Gate-1 inputs (3 rounds) + a Round-4 pivot from the user/team-lead.

- **Round 1 (observability)**: Original "leave alert/runbook/dashboard/catalog for Task #6" plan rejected — silent alert degradation in production is a false-confidence risk. Initial Approach #1 was DISABLE alert + dashboard panel + Sc 11 runbook with banner-and-TODO. Superseded by Round 4.
- **Round 2 (code-reviewer)**: Plan-confirmed otherwise (verifications clean: tags 20/21 unallocated × 3 envelopes; tag 11 reuse correct; `ConnectionState` aligns with buf STANDARD `ENUM_VALUE_PREFIX` + ADR-0011; ADR-0028:278,315 documents snake_case envelope-level placement). Added 4 missing Mechanical rows below.
- **Round 3 (team-lead arbitration + security asks)**: 3 buf-lint/security/orphan-alert questions resolved:
  1. Orphan-alert resolved via Approach #1 (later superseded by Round 4).
  2. Cross-boundary classification stays Minor-judgment for MC stub (no escalation).
  3. buf-lint legacy-enum suppressions: per-enum inline ignores with `// TODO(post-story): rename to ENUM_PREFIX_VARIANT (buf STANDARD)` comment for Task #17 sweep — see Planning > buf.yaml section.
  Also incorporated 3 @security asks: (i) `floor_char_boundary(256)` truncation reminder TODO in MC stub (already in plan); (ii) doc-comment on `MhConnectionStatus` flagging client-controlled string fields + recommending ≤256-char truncation by handler; (iii) doc-comment on `trace_parent`/`trace_state` flagging "VALIDATION DEFERRED — parsing in R-57/R-58/SDK; treat as untrusted on the wire".
- **Round 4 (user/team-lead pivot — supersedes Round 1's Approach #1)**: **DELETE the disable-with-banner artifacts outright. Same principle that drove the test-deletion decision — leftover artifacts from a refactor are exactly what we don't want.** Task #6 reintroduces alarm/dashboard/runbook with the new metric shape; a single `docs/TODO.md` entry is the proper forcing function. Applied uniformly across all 6 disabled surfaces:
  1. `mc-overview.json` panel id 45 — DELETE entirely (no `vector(0)` stub).
  2. `mc-alerts.yaml` `MCMediaConnectionAllFailed` — DELETE the alert rule + comment block.
  3. `mc-incident-response.md` Sc 11 — DELETE the scenario block; version-history entry now reads "Remove Sc 11" not "Disable".
  4. `mc-deployment.md` — DELETE the 3 hits (1 prose, 2 PromQL/checkbox lines).
  5. `mh-deployment.md` — DELETE the 7 hits (1 prose, 3 PromQL queries, 4 checkboxes/alert-firing checks).
  6. `mh-incident-response.md:732` rollback-awareness clause + `mh-incident-response.md:800` Related-Alerts — DELETE the dangling `MCMediaConnectionAllFailed` references.
  This pivot affects @operations' Round-1 hunk-ACK (they ACK'd disable-with-banner; this is delete-outright). @operations adjudicates at Gate 3. The user direction is the authoritative input.
- **INDEX.md scope reconciliation**: Per ADR-0024 §6.6 — different categories handled differently:
  - **Metric-catalog INDEX lines** (observability hunk-ACK authority): `observability/INDEX.md:16` (`mc_media_connection_failures_total`) + `meeting-controller/INDEX.md:29` (`record_media_connection_failed`) → DEFER to /close-story per observability.
  - **Code-symbol INDEX lines** (mechanical surface-staleness — code-reviewer flagged): `meeting-controller/INDEX.md:17` (handler doc), `meeting-controller/INDEX.md:33` (proto doc), `observability/INDEX.md:25` (tracing-target "incl. MediaConnectionFailed") → EDIT in this devloop. If observability extends their /close-story deferral over line 25 too, I'll honor their hunk-ACK and defer it.
  - **Alert-name references**: Per Round-4 pivot, `meeting-controller/INDEX.md` line 72 + `operations/INDEX.md` line 70 still reference `MCMediaConnectionAllFailed`. The alert is now DELETED (not disabled), so the names are stale until Task #6 reintroduces them. Treating these as `/close-story` reflection-phase edits — same alert name will revive in Task #6, so leaving them as a stable cross-reference is preferable to twice-flipping in two devloops.

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `proto/signaling.proto` | Mine | — |
| `proto/internal.proto` | Mine | — |
| `proto/buf.yaml` | Mine | — |
| `crates/proto-gen/build.rs` | Mine | — |
| `infra/devloop/Dockerfile` | Not mine, Mechanical | operations |
| `crates/mc-service/src/webtransport/connection.rs` | Not mine, Minor-judgment | meeting-controller |
| `crates/mc-service/src/webtransport/handler.rs` | Not mine, Mechanical | meeting-controller |
| `crates/mc-service/src/observability/metrics.rs` | Not mine, Mechanical | meeting-controller |
| `crates/mc-service/tests/join_tests.rs` | Not mine, Mechanical | meeting-controller |
| `crates/mc-service/tests/webtransport_accept_loop_integration.rs` | Not mine, Mechanical | meeting-controller |
| `crates/mc-service/tests/media_coordination_integration.rs` | Not mine, Mechanical | meeting-controller |
| `crates/mh-service/tests/webtransport_integration.rs` | Not mine, Mechanical | media-handler |
| `crates/mh-service/tests/common/wt_client.rs` | Not mine, Mechanical | media-handler |
| `crates/env-tests/tests/26_mh_quic.rs` | Not mine, Mechanical | test |
| `crates/env-tests/tests/24_join_flow.rs` | Not mine, Mechanical | test |
| `docs/user-stories/2026-05-02-browser-client-join.md` | Mine | — |
| `infra/docker/prometheus/rules/mc-alerts.yaml` | Not mine, Minor-judgment | observability / operations |
| `infra/grafana/dashboards/mc-overview.json` | Not mine, Minor-judgment | observability |
| `docs/observability/metrics/mc-service.md` | Not mine, Mechanical | observability |
| `docs/runbooks/mc-incident-response.md` | Not mine, Minor-judgment | operations |
| `docs/runbooks/mc-deployment.md` | Not mine, Mechanical | operations |
| `docs/runbooks/mh-deployment.md` | Not mine, Mechanical | operations |
| `docs/runbooks/mh-incident-response.md` | Not mine, Mechanical | operations |
| `docs/TODO.md` | Not mine, Mechanical | observability |
| `docs/specialist-knowledge/observability/INDEX.md` | Not mine, Mechanical | observability |
| `docs/specialist-knowledge/meeting-controller/INDEX.md` | Not mine, Mechanical | meeting-controller |
| `docs/specialist-knowledge/operations/INDEX.md` | Not mine, Mechanical | operations |
| `docs/specialist-knowledge/test/INDEX.md` | Not mine, Mechanical | test |
| `docs/specialist-knowledge/code-reviewer/INDEX.md` | Not mine, Mechanical | code-reviewer |
| `docs/specialist-knowledge/media-handler/INDEX.md` | Not mine, Mechanical | media-handler |
| `docs/specialist-knowledge/protocol/INDEX.md` | Mine | — |

### Scope notes (per-row detail)

- `proto/signaling.proto` — R-5/R-58/R-60 + per-enum buf-lint suppressions on legacy enums + @security doc-comments on trace fields and `MhConnectionStatus`.
- `proto/internal.proto` — per-enum buf-lint suppression on `HealthStatus` (parity with signaling.proto, code-reviewer gap).
- `proto/buf.yaml` — new file, v2 schema (STANDARD lint + WIRE_JSON breaking).
- `crates/proto-gen/build.rs` — comment-block refresh documenting WKT resolution via system protoc include path (libprotobuf-dev). Not mine technically but co-edited with the proto change as a single mechanical doc-comment update.
- `infra/devloop/Dockerfile` — add `libprotobuf-dev` for system WKT include path.
- `crates/mc-service/src/webtransport/connection.rs` — handler arm becomes no-op stub for `MediaConnectionUpdate`; **delete** the 2 old `test_handle_media_connection_failed*` tests outright (Task #6 writes new ones). Removing test coverage on a renamed code path is a coverage decision, not pure mechanical. Also +2 `trace_parent`/`trace_state` struct-literal initializers (Mechanical, prost-exhaustive).
- `crates/mc-service/src/webtransport/handler.rs` — 2 `ServerMessage` struct-literal initializers (`trace_parent`/`trace_state`).
- `crates/mc-service/src/observability/metrics.rs` — delete `record_media_connection_failed()` + `test_record_media_connection_failed` + the 2 calls + 2 assertion blocks in `test_cardinality_bounds` and `metrics_module_emits_mh_coordination_cluster` (code-reviewer gap 1). Test retains standalone value asserting `mc_mh_notifications_received_total`.
- `crates/mc-service/tests/join_tests.rs` — 4 `ClientMessage` struct-literal initializers.
- `crates/mc-service/tests/webtransport_accept_loop_integration.rs` — 2 `ClientMessage` struct-literal initializers.
- `crates/mc-service/tests/media_coordination_integration.rs` — file-level doc-comment refresh removing references to deleted `mc_media_connection_failures_total` / `test_handle_media_connection_failed*` / `MediaConnectionFailed` (code-reviewer gap 2). Tracking-guard fixed-string scan reference removed.
- `crates/mh-service/tests/webtransport_integration.rs` — `MhClientMessage` struct-literal initializer.
- `crates/mh-service/tests/common/wt_client.rs` — `MhClientMessage` struct-literal initializer.
- `crates/env-tests/tests/26_mh_quic.rs` — `MhClientMessage` struct-literal initializer.
- `infra/docker/prometheus/rules/mc-alerts.yaml` — DELETE the `MCMediaConnectionAllFailed` alert rule entirely (was lines 174-186 plus a 7-line `# DISABLED` comment block; both removed). Adjacent rules (`MCCapacityWarning`, `MCHighJoinFailureRate`) unchanged. Round-4 pivot from disable-with-comment.
- `infra/grafana/dashboards/mc-overview.json` — DELETE panel id 45 "Media Connection Failures" entirely (was at `gridPos {h:8, w:12, x:12, y:114}`). Right half of row y=114 is now empty; subsequent panels at y=122 stay in place — clean layout, no reflow needed. Round-4 pivot from `vector(0)` stub.
- `docs/observability/metrics/mc-service.md` — DELETE catalog entry + PromQL example + cardinality row.
- `docs/runbooks/mc-incident-response.md` — DELETE Sc 11 block entirely (header + body); update Version-History to "Remove Sc 11" (not "Disable"). TOC entry + P-upgrade rule + Sc 12 cross-ref already removed in prior pass. Round-4 pivot.
- `docs/runbooks/mc-deployment.md` — DELETE the 3 hits in §"Post-Deploy Monitoring Checklist: MC↔MH Coordination": (i) prose mention of `MediaConnectionFailed reporting` in section intro (line 915); (ii) `mc_media_connection_failures_total{all_failed="true"}` PromQL gate (line 924); (iii) checkbox + `MCMediaConnectionAllFailed` firing-check (lines 938-939). Remaining bullets (RegisterMeeting + notifications gates) intact.
- `docs/runbooks/mh-deployment.md` — DELETE the 7 hits across §"Post-Deploy Monitoring Checklist: MH WebTransport + MC↔MH Coordination": prose mention (line 44), 30-min PromQL + checkbox + alert-firing check (lines 90, 102, 104), 2-hour checkbox + PromQL (lines 111, 130), 24-hour PromQL + checkbox (lines 157, 183). Surrounding gates (handshake, JWT, RegisterMeeting timeouts, MH→MC notifications, active connections) intact.
- `docs/runbooks/mh-incident-response.md` — DELETE the dangling `MCMediaConnectionAllFailed` references at line 800 (Related Alerts) and trim the rollback-awareness paragraph at line 732 to drop the broken cross-ref to mc Sc 11 (kept the rollback awareness itself, just removed the dead link).
- `docs/TODO.md` — single coherent forcing-function entry under "Observability Debt": Task #6 reintroduces (a) the metric, (b) the alert `MCMediaConnectionAllFailed`, (c) Sc 11 runbook scenario, (d) dashboard panel "Media Connection Failures", (e) deploy-runbook acceptance gates and PromQL queries, (f) catalog entry — all atop `mc_participant_mh_status_total{state}`.

### INDEX.md (auto-excluded by Layer A — see scripts/guards/simple/validate-cross-boundary-scope.sh)

`docs/specialist-knowledge/**/INDEX.md` paths are auto-excluded from Layer A's plan-vs-diff comparison (reflection-phase artifacts authored at Step 8). The following INDEX edits ARE made in this devloop and noted here for cross-reviewer transparency:

- **EDITED in this devloop (Round-3 code-reviewer gaps 3+4 + Round-4 + Round-5 security advisory)**:
  - `docs/specialist-knowledge/meeting-controller/INDEX.md` lines 17 + 33 — `MediaConnectionFailed` → `MediaConnectionUpdate`.
  - `docs/specialist-knowledge/observability/INDEX.md` line 25 — `(incl. MediaConnectionFailed)` → `(incl. MediaConnectionUpdate)`.
- **EDITED in this devloop (Round-5 — @security advisory; closes the prior /close-story deferral)**: 5 INDEX-pointer stragglers that pointed at deleted symbols, plus 6 additional INDEX lines flagged during the verification sweep. Same false-confidence-pointer risk class as the alert/runbook hits: a future on-call greps for the symbol, finds the INDEX pointer, navigates to the source file, finds nothing, burns time disambiguating. Surgical line-edits — pure delete where the symbol is gone (alert/scenario/recorder), s/MediaConnectionFailed/MediaConnectionUpdate/ where the symbol has a successor (proto type, handler):
  - `docs/specialist-knowledge/observability/INDEX.md:16` — drop `mc_media_connection_failures_total` from the recording-sites paren-list (was deferred to /close-story under observability hunk-ACK; closed in this commit since the metric is deleted-not-renamed and the pointer is dead).
  - `docs/specialist-knowledge/meeting-controller/INDEX.md:29` — drop `record_media_connection_failed` from the recorder-list (also was deferred to /close-story; closed in this commit, same reason).
  - `docs/specialist-knowledge/meeting-controller/INDEX.md:72` — drop "(incl. MCMediaConnectionAllFailed)" qualifier from the alert-rules pointer.
  - `docs/specialist-knowledge/meeting-controller/INDEX.md:73` — drop "Sc 11 MediaConnectionFailed" from the runbook-scenario paren-list.
  - `docs/specialist-knowledge/operations/INDEX.md:35` — drop "Sc 11 MediaConnectionFailed" from the QUIC-story scenario paren-list.
  - `docs/specialist-knowledge/operations/INDEX.md:70` — drop the `MCMediaConnectionAllFailed alert` clause from the leading line; remaining clauses (token-refresh metric, accept-loop rig) preserved verbatim.
  - `docs/specialist-knowledge/test/INDEX.md:34` — drop "+ `handle_client_message` w/ MetricAssertion for `mc_media_connection_failures_total`" phrase from the join-flow test paren-list (the deleted-tests-outright decision invalidates the MetricAssertion pointer).
  - `docs/specialist-knowledge/code-reviewer/INDEX.md:44` — `(MediaConnectionFailed R-20)` → `(MediaConnectionUpdate stub, browser-client-join Task #6 reseats)` — handler still exists at the same site, just renamed.
  - `docs/specialist-knowledge/media-handler/INDEX.md:44` — s/MediaConnectionFailed/MediaConnectionUpdate/ in the proto-message paren-list (proto type renamed not deleted).
  - `docs/specialist-knowledge/protocol/INDEX.md:50-51` — proto type + handler pointers updated to `MediaConnectionUpdate` with the R-60/Task-#2-stub/Task-#6-reseats annotation.
- **NOT TOUCHED — Round-4 alert references stable across Task #6 reintroduction**: SUPERSEDED by Round-5 closure above. `meeting-controller/INDEX.md:72` and `operations/INDEX.md:70` are now updated this commit (deleting the dead pointer is cleaner than holding it for Task #6 to flip back).

**Compile-fix + ops scope decision (Round-4 pivot)**: **Minimal MC compile-stub + DELETE all six ops surfaces outright** (alert rule, dashboard panel, runbook scenario, mc-deployment hits, mh-deployment hits, mh-incident-response cross-refs). The catalog DELETE was already in scope from Round 1; the alert/panel/runbook/deploy-hits move from DISABLE-with-banner (original Round-1 Approach #1) to DELETE per user direction relayed by team-lead. Task #6 reintroduces all surfaces atop the new `mc_participant_mh_status_total{state}` metric; `docs/TODO.md` is the single coherent forcing function. The original Approach #1 description below is preserved as Round-1 history; the actual implementation follows Round-4.

> **Round 1 decision (superseded by Round 4)**: Approach #1 — Minimal MC compile-stub + DISABLE the four ops artifacts with TODOs pointing at Task #6.

### MC code (cross-boundary, Minor-judgment / Mechanical)

- `connection.rs:handle_client_message()` — replace `MediaConnectionFailed` arm with `MediaConnectionUpdate` arm that is a no-op stub: `tracing::debug!(target: "mc.webtransport.connection", connection_id, statuses_count = msg.statuses.len(), "MediaConnectionUpdate received (Task #6 stub: per-MH state recording deferred)"); // TODO(browser-client-join task #6, owner: meeting-controller): record per-MH state on participant actor + emit mc_participant_mh_status_total{state} metric. Reapply floor_char_boundary(256) truncation when reintroducing client-controlled string field logging (mh_url, failure_reason, failure_code) per security R-20 discipline.` No metric emission, no actor mutation.
- `connection.rs` tests `test_handle_media_connection_failed*` (lines 824-893) — rewrite to drive the new `MediaConnectionUpdate` variant with the stub behavior. Assert decode-success path executes without panic; do NOT assert metric deltas (the stub does not emit). Add a single inline `// TODO(task #6): re-add metric assertions when per-state metric lands` comment. Will collapse the two near-identical adjacency tests into one if @dry-reviewer/@test concur.
- `metrics.rs::record_media_connection_failed()` — **delete entirely** (no callers remain after the arm change). Delete the two unit tests `test_record_media_connection_failed` and the `record_media_connection_failed(true|false)` lines inside `test_cardinality_bounds`. Pure delete = Mechanical per ADR-0024 §6.2.

### Ops (Round-1 plan — superseded by Round 4 DELETE pivot; preserved for review history)

> **Note**: This section captures the Round-1 plan that was DISABLE-with-banner. The Round-4 pivot deletes outright. See **Cross-Boundary Classification** above and **Implementation Summary** below for the as-implemented state. The Round-1 prose remains here so reviewers can audit the decision trail.

- **Alert** `infra/docker/prometheus/rules/mc-alerts.yaml:174-186` — comment out the `- alert: MCMediaConnectionAllFailed` block as `# DISABLED — see Task #6` followed by the original block (each line `#`-prefixed) + a TODO referencing the future `mc_participant_mh_status_total{state="failed"}` metric. Reactivation is owned by Task #6 atop the new metric shape. Why DISABLE rather than DELETE: keeps the alert prose + thresholds + runbook URL in place for Task #6 to revive in one diff, signals to reviewers reading the file that the alert is intentionally paused not lost.
- **Dashboard panel** `infra/grafana/dashboards/mc-overview.json` panel id 45 (lines ~3398-3434) — change panel `title` from `"Media Connection Failures"` to `"Media Connection Failures (DISABLED — Task #6)"` and add a `description` field `"DISABLED in Task #2 of browser-client-join story; metric mc_media_connection_failures_total no longer emits. Task #6 will reactivate against mc_participant_mh_status_total{state}."`. Leave the `expr` in place (zero-data is informative; deleting the panel risks layout reflow). Why preserve the panel rather than delete: dashboard panel ids are referenced by alerts via `runbook_url` + are familiar to operators; preserving id 45 means Task #6 can swap the expr in one diff.
- **Catalog** `docs/observability/metrics/mc-service.md` — DELETE three things in this commit: (a) the `### mc_media_connection_failures_total` block (lines 209-218); (b) the PromQL example block "### Media Connection Failures (All Failed)" (lines 399-405); (c) the cardinality table row `| `all_failed` | 2 | `true`, `false` (media connection failures) |` (line 447). Catalog is the source-of-truth for "what metrics exist today" — leaving stale entries here is the actual lie. Task #6 ADDS new entries; no tombstone.
- **Runbook** `docs/runbooks/mc-incident-response.md` — DISABLE Sc 11 (lines 1155-1249) by replacing the section body with a single banner: `**Status**: DISABLED. The MediaConnectionFailed signaling message and mc_media_connection_failures_total metric were removed in browser-client-join Task #2 (proto/signaling.proto R-60 redesign). Task #6 of the same story redesigns this scenario atop the new mc_participant_mh_status_total{state} metric. See docs/devloop-outputs/2026-05-03-proto-trace-context-and-media-update/main.md for context.` — keep the `### Scenario 11: Media Connection Failures` heading + Alert/Severity/Runbook-Section anchor lines so cross-refs from the alert YAML's `runbook_url` still resolve once Task #6 fills it in. Also: (i) remove TOC entry line 25 `[Scenario 11: Media Connection Failures](#scenario-11-media-connection-failures)`, (ii) remove P-upgrade rule line 55 (the `mc_media_connection_failures_total{all_failed="true"}` upgrade rule), (iii) edit Sc 12 Related-Alerts line 1359 to drop the `MCMediaConnectionAllFailed` mention (keep MH + MCHighMailboxDepthWarning), (iv) append a Version-History entry at line ~1731: `2026-05-03: Disable Sc 11 in browser-client-join Task #2 (proto MediaConnectionFailed removed); Task #6 will reactivate atop mc_participant_mh_status_total{state}.`
- **`docs/TODO.md`** — append a one-line entry under "Observability Debt" pointing at Task #6: `[ ] **Reactivate MCMediaConnectionAllFailed alert + dashboard panel id 45 + mc-incident-response Sc 11**: Disabled in browser-client-join Task #2 when MediaConnectionFailed proto + mc_media_connection_failures_total metric were removed. Reactivation tracked in browser-client-join Task #6 (meeting-controller) atop the new mc_participant_mh_status_total{state} metric per the user story Design > meeting-controller section.`

**INDEX.md scope (refined per code-reviewer gap)**: Three categories handled differently:
- **Code-symbol stale references (EDIT this commit)**: `meeting-controller/INDEX.md` lines 17 + 33 (handler + proto type names) and `observability/INDEX.md` line 25 (tracing-target paren-list) — Mechanical surface-refresh because the symbols literally change under their feet. Owner is the file's specialist; observability owns line 25 hunk-ACK and may defer.
- **Metric-catalog references (DEFER to /close-story)**: `meeting-controller/INDEX.md:29` (`record_media_connection_failed`) + `observability/INDEX.md:16` (`mc_media_connection_failures_total`) — observability has hunk-ACK authority over the metric taxonomy index lines and elected /close-story timing.
- **Alert-name references (NOT TOUCHED)**: `meeting-controller/INDEX.md:72` + `operations/INDEX.md:70` (both `MCMediaConnectionAllFailed`) — the alert is DISABLED not deleted; the alert name remains valid for Task #6 reactivation, so the references stay accurate.

**Open question for `meeting-controller` (not on reviewer roster)**: classification of MC compile-fix as "Not mine, Minor-judgment" vs "Domain-judgment". Per the team-lead spawn, if reviewers challenge toward owner-implements (Domain-judgment) the task escalates to team-lead. I argue Minor-judgment because the stub is genuinely minimal (no new logic, no new metric, just kept-compiling-with-TODO).

---

## Planning

### Approach

**Single proto edit + buf.yaml + minimal MC compile-stub + doc-trim.** No semantic redesign of the MC handler/metric/alert/runbook — that is Task #6.

### Proto edits to `proto/signaling.proto`

1. **Add `import "google/protobuf/timestamp.proto";`** at the top (currently no imports). Required by `MhConnectionStatus.observed_at`.

2. **R-5: trace fields on `ClientMessage` and `ServerMessage`** — add OUTSIDE the `oneof message` block on both wrappers, with security-deferral doc:
   ```proto
   // W3C Trace Context (RFC 9287). Optional/empty when no active trace.
   // VALIDATION DEFERRED: parsing + W3C format checks are downstream
   // (R-57 MC, R-58 MH, SDK side) — owned by observability via
   // TraceContextPropagator. Treat as untrusted client-controlled
   // strings on the wire.
   string trace_parent = 20;
   string trace_state  = 21;
   ```
   Verified tags 20/21 unused in both wrappers (current high tag = 11 on ClientMessage, 11 on ServerMessage). Per @security ask 3.

3. **R-58: trace fields on `MhClientMessage`** — add same two fields + same doc-comment block outside its `oneof message`. Verified MhClientMessage currently allocates only `oneof message = 1`; tags 20/21 free.

4. **R-60: replace `MediaConnectionFailed` with `MediaConnectionUpdate`**:
   - **Delete** the `MediaConnectionFailed` message (currently lines 277-282).
   - **Delete** the `media_connection_failed = 11` `oneof` variant on `ClientMessage` (line 298).
   - **Add** new top-level definitions before `ClientMessage`:
     ```proto
     enum ConnectionState {
       CONNECTION_STATE_UNSPECIFIED  = 0;
       CONNECTION_STATE_CONNECTED    = 1;
       CONNECTION_STATE_FAILED       = 2;
       CONNECTION_STATE_DISCONNECTED = 3;
     }

     // Per-MH connection state reported by the client. ALL string
     // fields below are CLIENT-CONTROLLED and untrusted: handlers
     // MUST `floor_char_boundary(256)`-truncate before logging
     // (predecessor R-20 discipline; see Task #6 for the production
     // handler). Recommend ≤256 chars per field; proto3 does not
     // enforce a max.
     message MhConnectionStatus {
       string mh_url                          = 1;
       ConnectionState state                  = 2;
       optional string failure_reason         = 3;
       optional string failure_code           = 4;
       google.protobuf.Timestamp observed_at  = 5;
     }

     message MediaConnectionUpdate {
       repeated MhConnectionStatus statuses = 1;
     }
     ```
     Catalog-style enum-name prefix on `ConnectionState` values matches `internal.proto::DisconnectReason`/`RejectionReason` style and ADR-0011 cardinality discipline. Doc-comment on `MhConnectionStatus` per @security ask 2.
   - **Add** new `oneof` variant on `ClientMessage` reusing the freed tag:
     ```proto
     MediaConnectionUpdate media_connection_update = 11;  // (reuses freed tag from MediaConnectionFailed; one-time wire break, no on-wire clients)
     ```

### `proto/buf.yaml` (new, v2)

```yaml
version: v2
modules:
  - path: .
lint:
  use:
    - STANDARD
breaking:
  use:
    - WIRE_JSON
```

Module rooted at `proto/` (the file lives at `proto/buf.yaml` and `path: .` is relative to that). NOT `proto/buf.gen.yaml` — that's Task #7 / client-codegen scope.

I will run `buf lint` locally (download the static binary into `/tmp` if not on PATH) and report the output. Note: `buf lint STANDARD` will flag the existing pre-Task-#2 enum names that lack the catalog prefix (`StreamType.AUDIO`, `LeaveReason.VOLUNTARY`, `LayoutType.GRID`, `MediaType.MEDIA_*`, `ErrorCode.UNKNOWN`) — these are pre-existing wire-load-bearing names; renaming them is out of scope for Task #2 and is a separate wire-breaking change.

**Suppression strategy (per @code-reviewer adjudication, confirmed by @team-lead)**: per-enum inline suppressions, NOT file-scoped. Each legacy enum gets a one-line `// buf:lint:ignore ENUM_VALUE_PREFIX` (and `ENUM_ZERO_VALUE_SUFFIX` where applicable) comment immediately above its `enum` declaration, plus a `// TODO(post-story): rename to ENUM_PREFIX_VARIANT (buf STANDARD)` comment so Task #17 can sweep them cleanly when the wire-breaking rename story lands. Format:
```proto
// TODO(post-story): rename to STREAM_TYPE_AUDIO/CAMERA/SCREEN per buf STANDARD ENUM_VALUE_PREFIX (wire-breaking; tracked in docs/TODO.md).
// buf:lint:ignore ENUM_VALUE_PREFIX
// buf:lint:ignore ENUM_ZERO_VALUE_SUFFIX
enum StreamType { ... }
```
New code (`ConnectionState`) passes `STANDARD` clean — no suppression. This keeps the suppression next to the offender (warn-local-don't-blanket Rust idiom per @code-reviewer), survives moves, and prevents accidental new-code suppression.

`buf breaking` is **expected to flag R-60** (the `MediaConnectionFailed` removal + tag reuse). That is approved per Clarification Question 9 and will be called out in the commit message body.

### MC compile-fix + ops-disable (cross-boundary)

Per the Cross-Boundary Classification table above. Summary:
- `connection.rs:540-583`: replace `MediaConnectionFailed` arm with no-op `MediaConnectionUpdate` arm + TODO referencing Task #6 (incl. `floor_char_boundary(256)` reminder per @security).
- `connection.rs:824-893`: **delete** the two `test_handle_media_connection_failed*` tests outright. No stubs, no `#[ignore]`, no decode-only rewrites. Per user direction (final arbiter): the rewritten `assert_delta(0)` shape was useless leftover, and an interim decode+no-panic test on a stubbed handler is theater. Replace with a single in-module comment block flagging that Task #6 (meeting-controller) writes fresh tests against the new `mc_participant_mh_status_total{state}` metric + real per-MH state-recording handler. Task #6's tests live with the production code that asserts something meaningful.
- `metrics.rs:381-386`: delete `record_media_connection_failed()` (no callers remain).
- `metrics.rs:611-615, 676-677`: delete the unit test + the cardinality-bounds invocations.
- **`metrics.rs:904-925`** (code-reviewer gap 1): in `metrics_module_emits_mh_coordination_cluster`, delete the two `record_media_connection_failed(true|false)` call lines (910-911) + the two `mc_media_connection_failures_total` assertion blocks (lines 919-924). Test continues to assert `mc_mh_notifications_received_total` cluster.
- **`crates/mc-service/tests/media_coordination_integration.rs:11,16,17,18,24`** (code-reviewer gap 2): refresh file-level doc-comments. The paragraph documents a metric+test pair that no longer exists; the `tests/**/*.rs` fixed-string scan reference at line 24 (`mc_media_connection_failures_total`) becomes a permanent false-positive once the metric is gone — must be removed. Plan: trim the entire "## `mc_media_connection_failures_total` lives in `connection.rs::tests`" paragraph (lines 14-25) since both the metric AND the test it points to are gone post-Task-#2 (rewritten tests assert no metric).

**Ops-disable (per @observability Gate-1 input — addressed THIS COMMIT)**:
- `infra/docker/prometheus/rules/mc-alerts.yaml:174-186`: comment out `MCMediaConnectionAllFailed` block as `# DISABLED — Task #6 will reactivate atop mc_participant_mh_status_total{state}` + each original line `#`-prefixed.
- `infra/grafana/dashboards/mc-overview.json` panel id 45 (lines ~3398-3434): rename title to `"Media Connection Failures (DISABLED — Task #6)"`, add `description` field marking as disabled. Leave `expr` (zero-data is informative).
- `docs/observability/metrics/mc-service.md`: DELETE catalog block (209-218), DELETE PromQL example block (399-405), DELETE cardinality table row (447) for `all_failed`.
- `docs/runbooks/mc-incident-response.md`: replace Sc 11 body (1155-1249) with DISABLED banner; remove TOC line 25; remove P-upgrade rule line 55; trim Sc 12 Related-Alerts line 1359; append Version-History entry at line ~1731.
- `docs/TODO.md`: append one-line entry under Observability Debt referencing Task #6 reactivation.

**INDEX.md surface-staleness (per @code-reviewer gaps 3+4 — addressed THIS COMMIT)**:
- `docs/specialist-knowledge/meeting-controller/INDEX.md` line 17 (handler reference "MediaConnectionFailed handler R-20") + line 33 (proto reference "(join, mute, session recovery, MediaConnectionFailed)"): refresh `MediaConnectionFailed` → `MediaConnectionUpdate` in both. Surface-staleness — symbol no longer exists.
- `docs/specialist-knowledge/observability/INDEX.md` line 25 (tracing-target "(incl. MediaConnectionFailed)"): refresh to `(incl. MediaConnectionUpdate)`. Same rationale.

### What I am NOT touching

- **Metric-catalog INDEX lines** per @observability hunk-ACK: `meeting-controller/INDEX.md:29` + `observability/INDEX.md:16`. Land at `/close-story`.
- **Alert-name INDEX lines**: `meeting-controller/INDEX.md:72` + `operations/INDEX.md:70` (both `MCMediaConnectionAllFailed`). The alert is DISABLED-not-deleted; name remains valid for Task #6 reactivation, references stay accurate.
- `crates/mc-service/src/actors/participant.rs` — no per-MH state field; that is Task #6 territory.

### Verification

1. `cargo check --workspace` — must pass (this is the load-bearing check that the proto edit + MC stub keep the workspace compiling).
2. `cargo build -p proto-gen` — proto regenerates successfully via `build.rs` (verifies no proto syntax errors that `prost-build` would catch).
3. `cargo test -p mc-service --lib --test '*' -- handle_media_connection 2>&1 | head` — verify rewritten MC test(s) still execute.
4. `cargo test --workspace --no-run 2>&1 | tail -20` — full workspace test compile (no run; this story is proto + stub, deeper test exercises are Task #6 / others).
5. `buf lint proto/` — report findings; fix new code; suppress pre-existing legacy-enum findings per @code-reviewer's chosen approach.
6. `buf breaking proto/ --against '.git#branch=main'` — report; expected to flag R-60; document in commit.
7. `grep -rn "mc_media_connection_failures_total\|record_media_connection_failed\|MediaConnectionFailed" /work/{crates,docs,infra,proto}` — confirm: (a) no stale code references; (b) the only remaining doc references are inside the DISABLED alert YAML comment block + the DISABLED runbook Sc 11 banner; (c) `MCMediaConnectionAllFailed` alert name still appears (commented) in the YAML — that is intentional.
8. `promtool check rules infra/docker/prometheus/rules/mc-alerts.yaml` (if available) — confirm the DISABLED block (commented out) does not break the rule file syntax. If `promtool` not on PATH, manual visual verification is acceptable.
9. `jq . infra/grafana/dashboards/mc-overview.json > /dev/null` — confirm dashboard JSON remains valid after the title/description edit.

### Commit message draft

```
feat(proto): add trace-context envelope fields + MediaConnectionUpdate redesign

R-5: ClientMessage and ServerMessage gain `string trace_parent = 20`
and `string trace_state = 21` (W3C Trace Context format, proto3
default-empty). Tags verified unallocated. Wire-additive.

R-58: MhClientMessage envelope gains the same two fields. Tags
verified unallocated (envelope previously allocated only oneof
message = 1). Wire-additive.

R-60: Replace MediaConnectionFailed (and its `media_connection_failed`
oneof variant) with MediaConnectionUpdate { repeated MhConnectionStatus }
+ ConnectionState enum (CONNECTION_STATE_UNSPECIFIED/CONNECTED/FAILED/
DISCONNECTED, catalog-style prefix per ADR-0011 / internal.proto
convention). New `media_connection_update = 11` oneof variant REUSES
the freed tag — this is a one-time wire break from the predecessor
mh-quic-connection story. Approved per browser-client-join Clarification
Question 9: no on-wire clients exist outside this codebase. `buf breaking
WIRE_JSON` is expected to flag this; the flag is the documentation.

Add proto/buf.yaml (v2, STANDARD lint + WIRE_JSON breaking, module
rooted at proto/). buf.gen.yaml (client codegen config) is Task #7
scope, not landed here.

MC service compile-fix (cross-boundary, Minor-judgment, owner=
meeting-controller, defers semantic territory to Task #6 of this
story): replace MediaConnectionFailed arm in handle_client_message
with a no-op MediaConnectionUpdate arm (TODO refs Task #6); delete
record_media_connection_failed() (no callers); rewrite/collapse the
unit tests against the new variant shape.

Ops disable for the `mc_media_connection_failures_total` retirement
(per observability Gate-1 input — silent alert degradation is a
false-confidence risk that must be addressed this commit, not deferred):
- `MCMediaConnectionAllFailed` alert (mc-alerts.yaml:174-186):
  commented out as DISABLED with TODO; Task #6 reactivates atop
  the new mc_participant_mh_status_total{state} metric.
- mc-overview.json dashboard panel id 45 "Media Connection
  Failures": title prefixed `(DISABLED — Task #6)`, description
  added; expr left as-is (zero-data informative).
- mc-incident-response.md Sc 11 (lines 1155-1249): replaced with
  DISABLED banner; TOC entry, P-upgrade rule, Sc-12 cross-ref
  pruned; Version-History entry appended.
- observability/metrics/mc-service.md: catalog block, PromQL
  example, and cardinality table row for `all_failed` deleted
  (catalog is source-of-truth for currently-emitting metrics).
- TODO.md: one-line Task-#6 reactivation entry under Observability
  Debt.

Refs: docs/user-stories/2026-05-02-browser-client-join.md (Task #2,
R-5, R-58, R-60); docs/devloop-outputs/2026-05-03-proto-trace-context-
and-media-update/main.md
```

### Reviewer questions

@security: any concern with the W3C trace context fields landing as proto3 strings (no validation/parsing in this task — just wire surface)? @observability owns the parser side in R-57/R-58 reads.

@test: the MC test rewrite drops `assert_delta` against `mc_media_connection_failures_total` — agreed that is correct since the stub does not emit, with TODO-pointer to Task #6 to re-add metric assertions on the new metric? Or do you want the tests `#[ignore]`'d instead?

@observability: confirm doc-trim of `docs/observability/metrics/mc-service.md` is desired (vs leaving it as a tombstone with "(removed in browser-client-join Task #2; replaced by `mc_participant_mh_status_total` in Task #6)" pointer)? My read: clean delete is correct because Task #6 will add the new entry; a tombstone for ~3 days is more clutter than signal. Also confirm leaving the `MCMediaConnectionAllFailed` alert orphaned for the Task #2→#6 window.

@code-reviewer: enum-naming — `ConnectionState` follows the catalog-prefix convention (per team-lead spawn instructions and `internal.proto` precedent), but the existing `signaling.proto` enums (`StreamType`, `LeaveReason`, `LayoutType`, `MediaType`, `ErrorCode`) do NOT prefix. `buf lint STANDARD` will flag those legacy ones. Plan: suppress just the pre-existing rule violations in `buf.yaml`, not new code. Acceptable? Or do you want me to inline-suppress with `// buf:lint:ignore` per-enum so the suppression lives next to the offender?

@dry-reviewer: deletion-only on the MC side; new code is the proto edit + buf.yaml. No DRY surface. Confirm no concerns.

@operations: confirm leaving `MCMediaConnectionAllFailed` (mc-alerts.yaml:174) and `mc-incident-response.md` Sc 11 in place is the right call (rather than deleting them). My logic: Task #6 redesigns both atop the new metric in the same commit, so deleting + Task #6 re-adding is more PR churn than holding. The 2-3 day silently-non-firing window is acceptable because no production SDK emits `MediaConnectionFailed` today. If you disagree, alternatives are (a) delete them in Task #2 with a TODO in the user story, or (b) escalate to team-lead.

---

## Pre-Work

None.

---

## Implementation Summary

Landed all three protocol changes (R-5, R-58, R-60) plus `proto/buf.yaml`. MC compile-stub keeps the workspace green; per the Round-4 pivot, **all six orphaned ops surfaces are deleted outright** (alert rule, dashboard panel, mc-incident-response Sc 11 + cross-refs, mc-deployment hits, mh-deployment hits, mh-incident-response cross-refs); the catalog entry was already in delete-scope from Round 1. `docs/TODO.md` is the single coherent forcing function for Task #6 reintroduction. @code-reviewer's 4 surgical gaps closed; @security's 3 doc-comment asks landed in the proto.

**Build-script change discovered during implementation**: `signaling.proto` now imports `google/protobuf/timestamp.proto` (for `MhConnectionStatus.observed_at`). WKT resolution via `libprotobuf-dev` system package (added to `infra/devloop/Dockerfile`); no vendored proto in repo. The system protoc include path (`/usr/include/google/protobuf/`) resolves the `import` at parse time. Codegen via `tonic_build` defaults to mapping `.google.protobuf.Timestamp` → `::prost_types::Timestamp` so no Rust code is regenerated for the WKT. Earlier in this devloop a vendored `proto/google/protobuf/timestamp.proto` shipped as a build-host workaround; that path was retired (per user direction post-implementation) in favor of the system-package approach — it scales to other WKTs (Duration, Empty, etc.) without per-file vendoring decisions. Classified as Mechanical workspace-build-fix, owner protocol (mine for proto edits + build.rs comment refresh) + operations (Dockerfile package addition).

**Cross-boundary expansion discovered during implementation**: adding fields to `ClientMessage`, `ServerMessage`, and `MhClientMessage` envelopes broke 12 struct-literal initializers across MC + MH crates (proto3 prost generates exhaustive struct fields, no `..Default::default()` in caller code). All fixed mechanically with `trace_parent: String::new(), trace_state: String::new()`:
- `crates/mc-service/src/webtransport/connection.rs`: 2 `ServerMessage` literals (line 374 JoinResponse, 661 ErrorMessage).
- `crates/mc-service/src/webtransport/handler.rs`: 2 `ServerMessage` literals (ParticipantJoined, ParticipantLeft).
- `crates/mc-service/tests/join_tests.rs`: 4 `ClientMessage` literals.
- `crates/mc-service/tests/webtransport_accept_loop_integration.rs`: 2 `ClientMessage` literals.
- `crates/mh-service/tests/webtransport_integration.rs`: 1 `MhClientMessage` literal.
- `crates/mh-service/tests/common/wt_client.rs`: 1 `MhClientMessage` literal.
- `crates/env-tests/tests/26_mh_quic.rs`: 1 `MhClientMessage` literal.
All Mechanical surface-fixes per ADR-0024 §6.2 (purely additive new-field defaults to keep the workspace compiling — no semantic change to the values or the tests).

**v2 scope shrink (post-Round-3, per user direction)**: dropped the rewritten MC unit tests entirely. Initial Round-3 plan + my first implementation rewrote `test_handle_media_connection_failed*` against the new `MediaConnectionUpdate` variant (with @test's `assert_delta(0)` shape). User reviewed, called the rewrite useless leftover; team-lead agreed. Final shape: **delete both tests outright** (no stubs, no `#[ignore]`, no decode-only assertions). Replaced with a single in-module comment block flagging Task #6 as the new test home. Net: `cargo test -p mc-service --lib` now reports 242 passed (was 244). `MetricAssertion::snapshot()` removed from MC entirely; Task #6 reseeds. The `metrics_module_emits_mh_coordination_cluster` cluster-test trim from code-reviewer gap 1 remains in place — that test still asserts `mc_mh_notifications_received_total` post-trim.

**v4 ops-delete pivot (post-Layer-3 retry, per user direction relayed by team-lead)**: Round-1 Approach #1 (DISABLE the alert/panel/runbook with banner-and-TODO) superseded by user direction to **delete outright** — same principle that drove the test-deletion decision. Applied uniformly to all six surfaces:
- `mc-alerts.yaml`: `MCMediaConnectionAllFailed` rule + the `# DISABLED` comment block both removed (no comment-out trail).
- `mc-overview.json`: panel id 45 deleted entirely (the `vector(0)` stub from retry 1 also gone). Right half of `gridPos.y=114` is empty; subsequent panels at `y=122` stay in place — clean layout, no reflow needed.
- `mc-incident-response.md`: Sc 11 block deleted entirely (heading + body); version-history entry now reads "Remove Sc 11" not "Disable".
- `mc-deployment.md`: 3 hits deleted (1 prose mention in section intro, 1 PromQL gate, 1 alert-firing checkbox).
- `mh-deployment.md`: 7 hits deleted across 30-min / 2-hour / 24-hour windows (1 prose, 3 PromQL queries, 4 checkboxes/alert-firing checks). Post-deploy windows now check the surrounding gates (handshake, JWT, RegisterMeeting timeouts, MH→MC delivery, active connections) without media-connection coverage.
- `mh-incident-response.md`: dropped `MCMediaConnectionAllFailed` from Related Alerts (line 800) and trimmed the rollback-awareness paragraph at line 732 to remove the now-broken cross-ref to mc Sc 11.

Operations' Round-1 hunk-ACK was for disable-with-banner; the Round-4 pivot is delete-outright. Operations adjudicates at Gate 3.

---

## Files Modified

```
proto/signaling.proto                              | R-5/R-58/R-60 + buf-lint suppressions + security doc-comments
proto/buf.yaml                              [NEW]  | v2 STANDARD lint + WIRE_JSON breaking
infra/devloop/Dockerfile                           | Add libprotobuf-dev (system WKT include path for protoc)

crates/proto-gen/build.rs                          | Doc-comment refresh; tonic-build maps `.google.protobuf.*` to `::prost_types` by default

crates/mc-service/src/webtransport/connection.rs   | Stub MediaConnectionUpdate arm + DELETE 2 old tests outright (Task #6 writes new ones) + ServerMessage literals
crates/mc-service/src/webtransport/handler.rs      | 2 ServerMessage literals
crates/mc-service/src/observability/metrics.rs     | Delete record_media_connection_failed + 2 unit-test sites + cluster-test trim
crates/mc-service/tests/join_tests.rs              | 4 ClientMessage literals
crates/mc-service/tests/webtransport_accept_loop_integration.rs | 2 ClientMessage literals
crates/mc-service/tests/media_coordination_integration.rs       | Trim stale doc-comment paragraph (code-reviewer gap 2)
crates/mh-service/tests/webtransport_integration.rs| 1 MhClientMessage literal
crates/mh-service/tests/common/wt_client.rs        | 1 MhClientMessage literal
crates/env-tests/tests/26_mh_quic.rs               | 1 MhClientMessage literal

infra/docker/prometheus/rules/mc-alerts.yaml       | DELETE MCMediaConnectionAllFailed rule (Round-4 pivot — was DISABLE)
infra/grafana/dashboards/mc-overview.json          | DELETE panel id 45 entirely (Round-4 pivot — was title-prefix + vector(0))

docs/observability/metrics/mc-service.md           | DELETE catalog block + PromQL example + cardinality row
docs/runbooks/mc-incident-response.md              | DELETE Sc 11 block + TOC + P-upgrade + Sc12 cross-ref; Version-History entry reads "Remove" (Round-4 pivot)
docs/runbooks/mc-deployment.md                     | DELETE 3 media-connection hits (prose intro + PromQL gate + alert-firing checkbox)  [Round-4]
docs/runbooks/mh-deployment.md                     | DELETE 7 media-connection hits across 30m/2h/24h post-deploy windows  [Round-4]
docs/runbooks/mh-incident-response.md              | DELETE MCMediaConnectionAllFailed from Related Alerts (line 800) + trim rollback-awareness cross-ref to deleted mc Sc 11 (line 732)  [Round-4]
docs/TODO.md                                       | Single forcing-function entry (Observability Debt) — reintroduces metric+alert+dashboard+Sc11+deploy-gates+catalog atop new metric
docs/specialist-knowledge/meeting-controller/INDEX.md | Lines 17 + 33 refresh MediaConnectionFailed → MediaConnectionUpdate
docs/specialist-knowledge/observability/INDEX.md   | Line 25 same refresh
```

---

## Devloop Verification Steps

```
$ cargo check --workspace --tests
   Finished `dev` profile [unoptimized + debuginfo] target(s)

$ cargo test -p mc-service --lib
test result: ok. 242 passed; 0 failed; 0 ignored; 0 measured
# (244 prior to v2 scope shrink; -2 from deleting test_handle_media_connection_failed*
# outright per user direction; Task #6 writes new tests against the new metric.)

$ cargo test -p mc-service --lib handle_media_connection
test result: ok. 0 passed; 0 failed; 0 ignored
# (intentional — both old tests deleted; Task #6 owns the new test surface.)

$ cargo test -p mc-service --lib metrics_module_emits
test result: ok. 6 passed; 0 failed; 0 ignored
# (incl. metrics_module_emits_mh_coordination_cluster post-trim — still asserts
# mc_mh_notifications_received_total cluster after deletion of the
# mc_media_connection_failures_total halves.)

$ cargo test --workspace --no-run
   Finished `test` profile (all crates compiled)

$ jq . infra/grafana/dashboards/mc-overview.json > /dev/null
valid JSON

$ buf lint   (downloaded buf 1.40.0 to /tmp/buf for verification)
27 findings total — 24 pre-existing in internal.proto (out of scope), 3 in signaling.proto:
  signaling.proto:3:1: Multiple packages within directory "."  [pre-existing structural]
  signaling.proto:3:1: Files with package "dark_tower.signaling" must be within "dark_tower/signaling"  [pre-existing]
  signaling.proto:3:1: Package name should be suffixed with version (e.g. "dark_tower.signaling.v1")  [pre-existing]
NONE of the 5 legacy-enum ENUM_VALUE_PREFIX violations appear (per-enum inline `// buf:lint:ignore` working).
ConnectionState (new code) passes clean — no suppression needed.

$ buf breaking proto --against '.git#branch=feature/client-join-meeting-user-story,subdir=proto'
3 findings — all on field tag 11 of ClientMessage:
  - changed name: media_connection_failed → media_connection_update
  - changed type: MediaConnectionFailed → MediaConnectionUpdate
  - changed json_name: mediaConnectionFailed → mediaConnectionUpdate
EXPECTED + APPROVED per browser-client-join Clarification Question 9. Documented in commit message.
```

### Validation pipeline (Gate 2) — Layers 1-8 disposition

| Layer | Status | Notes |
|---|---|---|
| 1. cargo check --workspace --tests | PASS | clean |
| 2. cargo fmt --all --check | PASS | clean |
| 3. ./scripts/guards/run-guards.sh | PASS | 22/22 green (after delete-outright pivot of stale ops artifacts + table-restructure for parser-clean path cells) |
| 4. ./scripts/test.sh --workspace | PASS | full workspace tests green; mc-service lib 242/242 |
| 5. cargo clippy --workspace -- -D warnings | PASS | clean |
| 6. cargo audit | ACCEPTED-WITH-NOTE | 6 pre-existing transitive vulnerabilities (wtransport tree + sqlx-mysql); zero introduced by this devloop (no `Cargo.lock` change in diff). Acceptance authorized by user (final arbiter); no Task #2 scope to resolve. |
| 7. semantic-guard agent | SAFE | Verdict in agent message: security-sensitive logging strictly improved (stub logs only `connection_id` + usize count vs predecessor's client-controlled URL/reason); no panic risk; no actor blocking; trace fields not logged anywhere; tag-11 reuse acceptable per Clarification Q9; stale-ops deletions coherent across all six surfaces; test deletions semantically clean |
| 8. dev-cluster setup + env-tests | INFRA-BLOCKED (skipped this devloop) | Pre-existing setup.sh bootstrap chain bit-rot — `scripts/generate-dev-certs.sh` invoked from setup.sh runs with wrong CWD, so kubectl secret creation fails reading `${PROJECT_ROOT}/infra/docker/certs/mc-webtransport.crt`. Confirmed not introduced by this devloop (zero cert/setup files in `git diff`; setup.sh untouched ~3 weeks; generate-dev-certs.sh untouched ~5 weeks). Eager-setup also fails at container-init with `dev-cluster: executable not found in $PATH`. Both are operations-owned bootstrap-chain follow-ups (user tracking separately). For this devloop's diff (proto type changes + struct-literal initializers + no-op MC stub + ops-artifact deletes + Dockerfile add), Layer 8 regression risk is bounded — Layers 1-7 already prove cargo build + tests + semantic safety; the missing coverage is end-to-end inter-service handshake, which doesn't exercise the changed surface (the proto changes are wire-additive on the read side; the MC stub is unreachable until Task #6 lands the handler). |
```

---

## Code Review Results

| Reviewer | Verdict | Findings | Fixed | Deferred (accepted) | Notes |
|---|---|---|---|---|---|
| Security | CLEAR | 1 advisory (INDEX-pointer stragglers) | 1 | 0 | All 3 plan-stage asks landed verbatim (truncation reminder TODO; client-controlled-string doc comments on `MhConnectionStatus`; trace-context validation-deferral doc comments). Verified zero stale references across `docs/specialist-knowledge/` post-Round-5 sweep. |
| Test | CLEAR | 0 | 0 | 1 (deletion of `test_handle_media_connection_failed*` instead of rewrite — accepted because rewritten form would assert only prost decode + stub no-panic, not real MC code; Task #6 owns new tests against the new metric) | Coverage net judged sufficient: removed surface genuinely gone, project pattern preserved (no proto-level Rust unit tests in `proto-gen/`). |
| Observability | CLEAR | 5 | 4 (in-commit) | 1 (`docs/TODO.md:97,105` example-stale flag — Task #6 will rewrite when replacement metric lands) | Hunk-ACK on R-5/R-58 trace-context-on-the-wire convention + R-60 metric-removal coherence verified. Anchor closure on Sc 11 verified clean (zero broken inbound `#scenario-11` pointers post-pivot). |
| Code Quality | RESOLVED | 5 (INDEX-pointer dangles) | 5 (in-commit, in Round-5 sweep) | 0 | ADR-0002/0011/0019/0024 §6/0028 all PASS. MC stub semantic boundary held (no actor mutation, no metric emission, no dispatch); Domain-judgment escalation criteria NOT triggered. Cross-Boundary table covers 23/23 diff paths in path-only-first-cell form post Layer A guard fix. |
| DRY | CLEAR | 0 (true duplication) | 0 | 0 | Per-envelope `trace_parent`/`trace_state` is wire-boundary repetition (not duplication). 1 tracker entry appended to `docs/TODO.md` Cross-Service Duplication §From DRY Reviewer (Ongoing): future `TraceContext{}` sub-message extraction question (revisit if a third trace-context field e.g. `baggage` is added). |
| Operations | CLEAR (originally RESOLVED-contingent) | 4 INDEX-stale-refs (Finding 1) + 1 narrative-pointer-loss (Finding 2, accepted as-shipped) | 1 (`test/INDEX.md:34`, in Round-5 sweep) | 3 of Finding 1 withdrawn (alert names will revive verbatim in Task #6; observability metric-catalog hunk-ACK authority covers `obs/INDEX.md:16`) + 1 narrative pointer (Sc 11 cross-link from `mh-incident-response.md:732` lost; Task #6 restores) | Hunk-ACK confirmed across all six Round-4 delete-outright surfaces. Sc 10→Sc 12 flow preserved; deploy-runbooks read coherently post-deletion. |

**Adjudication**: zero ESCALATED verdicts. All findings either fixed in-commit or accepted as deferral. Round-5 INDEX-pointer sweep (11 edits across 7 files) materially exceeded operations' Finding 1 scope of 4 — implementer correctly applied the user's leftover-from-refactor principle to all surface-staleness pointers, including some that earlier rounds had deferred to /close-story under disable-with-banner reasoning that no longer held post Round-4 delete-outright pivot.

**Cross-reviewer convergence note**: observability's Gate-3 verdict deferred 6 INDEX lines to /close-story; implementer's Round-5 sweep then closed 2 of those (`observability/INDEX.md:16`, `meeting-controller/INDEX.md:29`) on the basis that the symbols are deleted-not-renamed (dead pointers, not "twice-flipping" candidates). No reviewer pushed back post-sweep; security explicitly verified zero stragglers remain. Convergence is real.

---

## Tech Debt References

- `docs/TODO.md` §Observability Debt — "Reintroduce media-connection observability surfaces (browser-client-join Task #6)": single coherent forcing-function entry covering all six deleted surfaces (metric, alert, Sc 11, dashboard panel, deploy-runbook gates in mc/mh-deployment.md, catalog entry) — Task #6 reintroduces atop `mc_participant_mh_status_total{state}`.
- `docs/TODO.md` §Cross-Service Duplication (DRY) > "From DRY Reviewer (Ongoing)" — `trace_parent`/`trace_state` envelope-level field pair × 3 envelopes: tracker-only entry capturing the future `TraceContext{}` sub-message extraction question. Story explicitly chose flat tags 20/21; the tracker notes the trigger for revisiting (third trace-context field e.g. `baggage`) and the reason it's correct now (per-envelope wrapping cost, SDK access nesting, fixed tag layout).
- `docs/TODO.md` §Dependency Vulnerabilities — `cargo audit` 6 pre-existing transitive findings (wtransport tree → quinn-proto/ring/rustls-webpki/rustls-pemfile; sqlx-mysql → rsa). Zero introduced by this devloop; acceptance documented at Layer 6.
- **Bootstrap chain bit-rot** (user tracking separately, NOT in `docs/TODO.md`): two pre-existing operations-owned bugs surfaced during Layer 8 attempt — (i) `scripts/generate-dev-certs.sh` runs with wrong CWD when invoked from `setup.sh` (recommended fix: self-locate via `BASH_SOURCE`); (ii) `infra/devloop/devloop.sh` eager-setup invocation of bare `dev-cluster` fails at container-init with `executable not found in $PATH` (recommended fix: absolute path or symlink-into-PATH). Untouched by this devloop's diff (verified via `git diff --name-only`).

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `11942361908303ce60e7a9053597a981b059efcf`
2. Review all changes: `git diff 11942361..HEAD`
3. Soft reset: `git reset --soft 11942361`
4. Hard reset: `git reset --hard 11942361`
5. No DB or infra changes — no additional rollback steps required.
6. No on-wire clients exist outside this codebase (Clarification Question 9), so the R-60 wire break is reversible by reverting the commit.

---

## Issues Encountered & Resolutions

### Issue 1: WKT include path missing on devloop image
**Problem**: `import "google/protobuf/timestamp.proto"` could not be resolved at protoc invocation; `/usr/include/google/protobuf/` missing because base image had `protobuf-compiler` (apt) without companion `libprotobuf-dev`.
**Resolution**: Added `libprotobuf-dev` to `infra/devloop/Dockerfile` apt install line (line 21) with explanatory comment. User installed into running container via `apt-get update && apt-get install -y libprotobuf-dev` as root for this session; next devloop image rebuild bakes it in by default.

### Issue 2: Round-1 disable-with-banner pattern superseded by Round-4 delete-outright
**Problem**: Initial plan was to disable alarm/dashboard/runbook artifacts (the metric removed by R-60) with a banner pointing at Task #6. User direction: "don't keep around old useless dashboards just remove then and keep things clean."
**Resolution**: Round-4 pivot — deleted all six stale ops artifacts outright (alarm + dashboard panel + Sc 11 scenario + 4 deploy-runbook touchpoints + 1 cross-ref). Single `docs/TODO.md` forcing-function entry remains as the breadcrumb for Task #6's reintroduction. Same principle applied to test deletions (`test_handle_media_connection_failed*` removed outright; Task #6 owns new tests).

### Issue 3: Cross-Boundary Classification table parser-mismatch
**Problem**: Layer A scope-drift guard reported 5 missing rows; root cause was the `parse_cross_boundary_table` parser at `scripts/guards/common.sh:368` taking the whole first-cell content as the path, so trailing parentheticals collapsed the match.
**Resolution**: Restructured the table — first cell is a clean path-in-backticks; descriptive context moved to a "Scope notes (per-row detail)" subsection beneath. 23/23 diff paths now appear verbatim in the table.

### Issue 4: Layer 8 env-tests blocked by pre-existing bootstrap bit-rot
**Problem**: `dev-cluster setup` failed at the kubectl secret-creation step because `scripts/generate-dev-certs.sh` (invoked from `setup.sh`) ran with wrong CWD and didn't produce certs at the expected path. Separately, eager-setup at container-init failed with `dev-cluster: executable not found in $PATH`.
**Resolution**: Layer 8 marked INFRA-BLOCKED; both bugs verified pre-existing (zero cert/setup files in `git diff`; setup.sh untouched ~3 weeks; generate-dev-certs.sh untouched ~5 weeks). User tracking the bootstrap-chain fixes as a separate operations follow-up. For this devloop's diff, regression risk is bounded — Layers 1-7 prove cargo build + tests + semantic safety; missing coverage is end-to-end inter-service handshake which doesn't exercise the changed surface (proto changes are wire-additive on the read side; MC stub is unreachable until Task #6).

---

## Lessons Learned

1. **Apply leftover-from-refactor principle uniformly**. If we're deleting the metric/proto-variant, also delete the alert/dashboard/runbook scaffolding pointing at them — don't leave disable-with-banner as scaffolding for the next task to reactivate. The `docs/TODO.md` forcing-function entry is the right breadcrumb size; per-surface scaffolding is anti-pattern. Same principle for tests (delete outright vs rewrite-with-tautological-assertion).
2. **Distinguish "revives verbatim" from "deleted-not-renamed" when scoping INDEX-staleness work**. Alert names that come back identically in Task #6 are stable cross-references; metric/function names that are deleted and replaced with *different* names are dead pointers. The hunk-ACK reasoning differs.
3. **`assert_delta(0)` on a removed metric is tautological**. The metric isn't registered with the recorder anywhere; the assertion can't fail unless someone reintroduces both the recorder call and the same metric name. Real coverage is "new variant decodes + stub doesn't panic"; defensive metric assertions are ornament. `assert_unobserved` similarly applies only to metrics that exist-but-shouldn't-fire.
4. **Cross-Boundary Classification table format**: keep the first cell to a clean path-in-backticks. Descriptive scope in a separate column or subsection. The Layer A scope-drift guard's parser is strict on first-cell content.
5. **Setup bootstrap chain has bit-rotted in two distinct ways**: (a) `generate-dev-certs.sh` is CWD-relative and silently fails when invoked from anywhere except project root; (b) `devloop.sh`'s eager-setup invocation of bare `dev-cluster` fails at container-init due to PATH ordering. Each costs ~7-20 minutes per devloop until fixed. User tracking the bootstrap-chain fix as a separate operations devloop.
6. **Race-condition pattern in concurrent reviewer/team-lead messaging**: when a reviewer is mid-iteration on retry-N and the team-lead sends a pivot direction, the resulting "FYI on small correction" reply may describe pre-pivot work. Disambiguate explicitly with version labels (v3 vs v3-retry-1 vs v3-retry-2) on each ready-for-validation signal.
