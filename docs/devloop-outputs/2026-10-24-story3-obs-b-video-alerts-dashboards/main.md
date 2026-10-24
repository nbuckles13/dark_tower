# Devloop Output: Story 3 OBS-B — video dashboards, alerts A1–A9, directive alert, inventories, SLO open objectives

**Date**: 2026-10-24
**Task**: Story 3 "see each other" OBS-B — video dashboard rows (mc-media, mh-media, client-media), alerts A1–A9 + send-directive rule, `MCMediaGenerationDivergence` refresh, alert/dashboard inventories, catalog cross-references, SLO open objectives, hygiene applies-evidence, env-test 33 extension. Full task text: `~/.cache/devloop/story-runs/story-runner/2026-10-07-see-each-other/task-20.prompt` (run-story task OBS-B).
**Specialist**: observability
**Mode**: Agent Teams (v2) — full, Gate-1 present
**Branch**: `feature/user-see-each-other`
**Duration**: ~3h10m

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `6a2fce835cd7da3c501ab213b099d2aaa29c5a0d` |
| Branch | `feature/user-see-each-other` |
| Lead Model | `claude-opus-5-5[1m]` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` (observability) |
| Implementing Specialist | `observability` |
| Tier | `full` |
| Iteration | `2` (Gate 2 L7 attempt 1 failed; attempt 2 passed) |
| Security | `security` |
| Test | `test` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `paired-operations` (paired: owns every runbook_url/annotation target) |
| Semantic Guard | not spawned (no credential/Debug/client credential-lifetime surface: rules, dashboards, docs, env-test) |
| Media Handler (conditional domain) | `media-handler` |
| Meeting Controller (conditional domain) | `meeting-controller` |
| Client (conditional domain) | `client` |

---

## Task Overview

### Objective
See Task line.

### Scope
- **Service(s)**: observability artifacts for MC, MH, client (rules, dashboards, docs); env-tests
- **Schema**: No
- **Cross-cutting**: Yes

### Debate Decision
NOT NEEDED — governed by ADR-0036 §11 and ADR-0031.

---

## Cross-Boundary Classification

Implementer: observability. Alert rules are authored here by story assignment (task OBS-B: "observability owns every rule"), although ADR-0031 §Ownership split makes `<svc>-alerts.yaml` service-owned, so each rules file is Domain-judgment with the service specialist as owner-reviewer. Operations owns every `runbook_url` target and makes its own runbook edits in-loop.

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `infra/docker/prometheus/rules/mh-alerts.yaml` | Not mine, Domain-judgment | media-handler |
| `infra/docker/prometheus/rules/mc-alerts.yaml` | Not mine, Domain-judgment | meeting-controller |
| `infra/docker/prometheus/rules/client-alerts.yaml` | Not mine, Domain-judgment | client |
| `infra/grafana/dashboards/mc-media.json` | Mine | — |
| `infra/grafana/dashboards/mh-media.json` | Mine | — |
| `infra/grafana/dashboards/client-media.json` | Mine | — |
| `docs/observability/alerts.md` | Mine | — |
| `docs/observability/dashboards.md` | Mine | — |
| `docs/observability/slos.md` | Mine | — |
| `docs/observability/metrics/mc-service.md` | Not mine, Minor-judgment | meeting-controller |
| `docs/observability/metrics/mh-service.md` (alert cross-refs: Minor; new backlog-bytes row: Domain) | Not mine, Domain-judgment | media-handler |
| `docs/observability/metrics/client.md` | Not mine, Minor-judgment | client |
| `crates/mc-service/src/media_signaling/outcome.rs` (reclass of `AssignmentFailed` to MC defect + `#[cfg(test)]` exhaustive `alert_class()` + partition drift test) | Not mine, Domain-judgment | meeting-controller |
| `crates/mh-service/src/media/edge_runtime.rs` (read-only `backlog_used()` accessor on the edge cell; no telemetry in `media/`) | Not mine, Domain-judgment | media-handler |
| `crates/mh-service/src/routing/mod.rs` (`RoutingTable::publish_pending` sums `backlog_used()` into `mh_media_egress_stream_backlog_bytes` once per 10 s tick) | Not mine, Domain-judgment | media-handler |
| `crates/mh-service/src/observability/metrics.rs` (new gauge resolved eagerly in `resolve_media_handles`) | Not mine, Domain-judgment | media-handler |
| `crates/mc-service/src/observability/metrics.rs` (`record_send_directive` rustdoc: reclassification) | Not mine, Domain-judgment | meeting-controller |
| `crates/mc-service/Cargo.toml` (`serde_norway` dev-dependency for the partition test's structural YAML parse) | Not mine, Domain-judgment | meeting-controller |
| `Cargo.lock` (regen) | Not mine, Mechanical | infrastructure |
| `crates/env-tests/src/fixtures/metric_hygiene.rs` (shared story-3 eager-series const) | Not mine, Minor-judgment | test |
| `crates/env-tests/src/fixtures/alert_rules_loaded.rs` | Not mine, Minor-judgment | test |
| `crates/env-tests/tests/33_alert_rules_loaded.rs` | Not mine, Minor-judgment | test |
| `crates/env-tests/src/fixtures/metrics.rs` (`PrometheusClient::format_query`) | Not mine, Minor-judgment | test |
| `crates/env-tests/src/fixtures/injection.rs` (`Injection::register_refused` for the A8 FIRE) | Not mine, Minor-judgment | test |
| `crates/mh-service/tests/media_stream_abandonment_integration.rs` (backlog-bytes gauge assertions, metric-coverage) | Not mine, Domain-judgment | media-handler |
| `crates/env-tests/tests/31_gc_telemetry.rs` | Not mine, Minor-judgment | test |
| `crates/env-tests/tests/32_media_metric_hygiene.rs` | Not mine, Minor-judgment | test |
| `crates/env-tests/tests/37_mh_stream_attribution.rs` | Not mine, Minor-judgment | test |
| `crates/env-tests/tests/39_mh_video_keyframes.rs` | Not mine, Minor-judgment | test |
| `packages/web-app/e2e/clientMetricNames.ts` | Mine | — |
| `packages/web-app/tests/clientMetricNames.test.ts` | Not mine, Minor-judgment | client |
| `docs/TODO.md` | Mine | — |
| `docs/runbooks/mc-incident-response.md` (operations' in-loop edits: directive partition, `#send-directive-defect-page`, client-window `[5m]`) | Not mine, Domain-judgment | operations |
| `docs/runbooks/mh-incident-response.md` (operations' in-loop edits: Scenario 20 A1 shape, Scenario 22 window) | Not mine, Domain-judgment | operations |
| `docs/runbooks/mh-deployment.md` (operations' in-loop edit: A8 named `MHTransportInconsistent`) | Not mine, Domain-judgment | operations |

---

## Planning

### Mechanism restatement
The task, in instance language, is "add A1–A9 + directive rule". In mechanism language: every story-3 media series that an operator must act on gets exactly one rule whose threshold is derived from a server-side config gauge (never a client-reported or literal copy), whose selectors are proven to match a real series (APPLY) and whose adverse condition is induced at least once (FIRE), and every series an operator must NOT act on is recorded as a no-alert decision. The wider class this exposes: (a) every catalog row still saying "Alert: none yet (story 3 OBS-B)" must be resolved one way or the other — not only the rows the prompt names (kind_missing, encoding_refused, send_groups_stopped, encode_queue_dropped, mh_control_channel_ended, keyframe_requests_unattributed, incoming_streams_refused, keyframe write failures); (b) env-test 33 today proves names loaded, not expressions, so "loaded with the committed expression" needs an expression comparison for every rule, not just the new ones; (c) the vocabulary partition idiom (negated selector + mechanical drift check) applies to the directive counter as it already does to `MCMediaGenerationDivergence` and `MCMediaMissingKeyMaterial`.

### Landed-name / spec differences (catalog/code wins; recorded)
1. **A1 denominator.** `mh_media_stream_sources_active` does not exist (MH-B did not land it). `mh_media_stream_sources_declared` exists but counts PAUSED, DROP_AT_INGRESS and unwatched sources, so it under-reads (masks a storm in proportion to idle sources). Use `mh_media_stream_ingress_groups` (RAII true count of publisher group streams being read; ~1 per actively-sending source per handler, since a group stays open until the next keyframe), averaged over the window: `sum(rate(mh_media_group_starts_total{direction="ingress"}[10m])) / sum(avg_over_time(mh_media_stream_ingress_groups[10m])) > 0.5 / min(mc_media_video_keyframe_min_interval_seconds)`. The ratio reads as 1 / mean group lifetime; the threshold is "mean group lifetime < 2 × directed interval". `min()` not the prompt's `max()`: A1's threshold falls as the interval rises, so `min()` is the conservative side under an MC config skew (meeting-controller point 6a). Decision (media-handler concurs): a separate `mh_media_stream_sources_active` gauge is REDUNDANT, not a TODO — `mh_media_stream_ingress_groups` is the live active-source measure; a declared-FORWARD gauge would still count unwatched sources. Caveats in the A1 block: the gauge rises at stream accept (before attribution) and overlaps at group boundaries; both transient, bounded by the per-connection uni-stream limit, and err toward not firing. Guard sits on the same averaged expression.
2. **Backlog in-use gauge** did not exist (MH-A2 landed only the cap). Added in-loop as `mh_media_egress_stream_backlog_bytes` (Gauge, `key_custody`). FINAL SHAPE (Gate 3, superseding the per-chunk version): tick-sampled every 10 s by `RoutingTable::publish_pending` summing each edge cell's `backlog_used()` (`BacklogBudget::used()`, one Acquire load); `media/downlink.rs` unchanged; `.set(0.0)` at resolve. Panel bound = `max(mh_media_egress_stream_backlog_limit_bytes) * max(mh_media_egress_stream_ceiling)` (the R-21 boot-check charge term; not memory).
3. **"Declared limit" for concurrent ingress groups** is per-connection (`mh_media_uni_streams_per_connection_limit`), not a process-wide limit; it is not drawn as a threshold line over the process-wide gauge.
4. **`rewrite_relay_region` entry-point split** has no label: codec tokens are `direction="ingress"`; a rewrite failure is `relay_rewrite_failed` (`direction="egress"`). Paneled that way.
5. **Client label names:** capture failures `outcome` (7 values), recovery requests `source` (6 values), v2-directive reject = `dt_client_media_send_directive_header_version_refused_total` (kind-free).
6. **Client windows vs collector expiration (G5).** `metric_expiration: 10m` must strictly exceed every `dt_client_*` window; A6 `[10m]` and A9 `[15m]` violate it. Raising expiration/max_stale (to 20m/35m) more than doubles per-run-org stream overlap against a cap whose honest headroom is already ~3.0 runs. Decision: A6 and A9 use `[5m]` (as `MCMediaMissingKeyMaterial` does) with A6 keeping its specified `for: 10m` and A9 departing to `for: 5m` (Implementation Summary, recorded departure 4); the rate-window-shorter-than-`for:` shape is the documented anti-shape idiom and is stated in each block.
7. **Directive rule — two rules, page + warning** (meeting-controller argued, operations agreed): `assignment_failed` is an MC defect (only reachable through `EgressOrdinalOverflow`, unreachable given config-validated caps), not environmental; corrected in `outcome.rs` rustdoc and the catalog.

### Alerts (all `severity: warning` unless stated; no `by` on any identity; client rules never `by (instance)`)
- **A1 `MHVideoKeyframeStorm`** (mh) — above, guard `sum(mh_media_stream_ingress_groups) > 0`, `for: 10m`, `#scenario-20-video-keyframe-storm`. Annotation: check pod age first (post-roll burst expected).
- **A2 `MHVideoSubscriberBacklogResets`** (mh) — `sum(rate(resets{reason="subscriber_backlog"}[15m])) / sum(rate(starts{direction="egress"}[15m])) > 0.10`, guarded, `for: 15m`, `#scenario-21-...`. Block states resets are not a subset of egress starts (1+N counting), so the ratio over-reads the true share.
- **A3 `MHVideoEdgeStuckPending`** (mh) — `max(mh_media_edge_pending_oldest_seconds) > 10 * max(mc_media_video_keyframe_min_interval_seconds)`, `for: 5m`, `#scenario-22-...`. Blind spot (publisher not on this handler) → A9.
- **A4 `MHPausedSourceIngress`** (mh) — paused / forwarded{ingress} > 0.01, guarded, `for: 15m`, `#scenario-23-...`; cites forward.rs / stream_ingress.rs increment sites (paused ⊆ forwarded{ingress}).
- **A5 `ClientVideoServerMutedSenderFrames`** (client) — `sum(rate(dt_client_media_frames_dropped_total{reason="sender_server_muted", media_kind="video"}[5m])) > 0`, `for: 10m`, MH `#scenario-19-...`. Warning not page (client-inflatable; a patched client can also suppress it); proof is MH `server_muted`.
- **A6 `ClientVideoDecodeErrorRatio`** (client) — decoder_errors{video} / frames_accepted{video} `[5m]` > 0.01, guarded, `for: 10m`, MC `#scenario-22-client-video-decode-failures`. Block states the events-per-frame unit and the reachability ceiling: under TOTAL decode failure the ratio is ≈ 1 / (fps × max(interval, decoderRestartBackoffMs)), so above ~3.3 s interval at 30 fps total failure never reaches 1%; the compensating control is A9 (a totally failed decoder leaves the slot unfulfilled, and A9's bound scales with the interval). A6 covers partial failure; A9 covers total.
- **A7 `MCMediaMissingKeyMaterial`** (mc, existing) — numerator, denominator AND guard become `sum by (media_kind)` over `received` (never `accepted`); video re-derivation recorded (ratio is frame-rate-free; video's healthy duty cycle is also skew/W; post-key decoder waits land on decode_queue_dropped, not this counter; `no_transmit_key` is not in the selector), 5% / `for: 15m` stand. clientMetricNames drift test asserts the `by (media_kind)` grouping.
- **A8 `MHTransportInconsistent`** (mh) — either arm `> 0` over `[5m]`, `for: 15m`, `mh-deployment.md#mixed-transport-rollback-hazard`.
- **A9 `ClientVideoSlotUnfulfilled`** (client) — `histogram_quantile(0.95, sum by (le) (rate(..._bucket[5m]))) > 10 * 1000 * max(interval)`, guard on `_count`, `for: 5m` (Gate 1: `for:` must not exceed the [5m] window; see Implementation Summary departure 4), MC `#scenario-23-client-video-slot-unfulfilled`. Buckets verified (top 300000 ms > 100 s; 5000 ms default threshold is a bucket boundary); every exit incl. teardown observed — so a whole-session stuck slot surfaces once, at leave.
- **Directive PAGE `MCMediaSendDirectiveDefect`** — `sum(increase(mc_media_send_directives_total{outcome=~"unknown_stream_number|transport_mode_unspecified|handler_url_unresolved|assignment_failed"}[15m])) > 0`, `for: 0m`, `#send-directive-wiring`.
- **Directive WARNING `MCMediaSendDirectiveFailures`** — negated `outcome!~"emitted|emitted_empty_targets|unknown_stream_number|transport_mode_unspecified|handler_url_unresolved|assignment_failed"`, same window, so `meeting_state_unavailable` and any unclassified future token fire here. Drift: exhaustive wildcard-free `DirectiveOutcome::alert_class()` in `outcome.rs` + a unit test pinning both rule regexes to it. Block names the `DirectiveOutcome::ALL` zero-init dependency.
- **`MCMediaGenerationDivergence`** — expr unchanged (three-site sync untouched); description/impact refreshed: per-stream `transport_mode_mismatch` (absent_from_echo / echoed_unspecified / mode_differs, first mismatching stream on the `mc.grpc.mh_client` ERROR line fields `echo_arm`, `echo_stream_number`), mis-ordered cutover case, "audio and/or video".
- Every rule using the MC gauge records that it goes silent if no MC target is scraped (MCDown covers it) and why min/max is the safe direction.

### Dashboards
Layout + video rows as the prompt lists, re-using emitter panels (moved/regrouped, never duplicated); alert-linked panels draw the alert threshold: gauge-derived thresholds as a second plotted target (A1, A3, A9), fixed-policy ratios as an absolute threshold line (A2, A4, A6). MH panels titled by behaviour (stream transport), descriptions note MC's assignment makes it video. Client panels split by `media_kind`. Every target `editorMode` + `range`/`instant`.

### Tests
- 33: expected set extended (A1–A9, both directive rules, divergence); every loaded rule's `query` compared to its committed `expr` normalised through `/api/v1/format_query` (pure kernel + unit fixtures; missing `query` = COULD-NOT-EVALUATE); APPLY: every vector selector extracted from each new rule (pure lexer kernel + unit fixtures) returns ≥1 series — eager MH series and the MC boot gauge gated; lazy MC/client series either induced first or reported ACKNOWLEDGED-VACUOUS, never silent. Stale rustdoc ("no dt_client_* reaches Prometheus") corrected.
- FIRE (rule's own expr from YAML as an instant query, never waiting out `for:`; baseline/marker control): A5, A6, A9 by authenticated OTLP posts on the 31 path with a per-run `client_version` marker injected into the client selectors (two posts so `rate()` sees an increase; A9's observation value derived from the gauge); A3 in 39 (the existing never-sending publisher's pending edge crosses the gauge-derived bound); A8 in 37 (registration fixture with mixed candidate transport modes → `rejected_transport_inconsistent`). No wall-clock latency gate.
- 32: explicit present-list of the new MH/MC series (as MH_EAGER_ADMISSION_SERIES).

### Docs
alerts.md (A1–A9, directive pair, refreshed divergence, no-alert decisions incl. the "none yet" rows), dashboards.md, slos.md (two open objectives + MH pending-duration decomposition), catalog alert cross-refs, TODO closure.

---

## Pre-Work

None

---

## Implementation Summary

### Alert rules (every rule carries its ADR-0031 block in the rules file)
| Code | Rule | File | Severity | Runbook |
|---|---|---|---|---|
| A1 | `MHVideoKeyframeStorm` | mh-alerts.yaml (group `mh-media-video-warning`) | warning | MH `#scenario-20-video-keyframe-storm` |
| A2 | `MHVideoSubscriberBacklogResets` | mh-alerts.yaml | warning | MH `#scenario-21-slow-video-subscriber-stream-credit-stall` |
| A3 | `MHVideoEdgeStuckPending` | mh-alerts.yaml | warning | MH `#scenario-22-video-slot-stuck-pending` |
| A4 | `MHPausedSourceIngress` | mh-alerts.yaml | warning | MH `#scenario-23-client-sending-while-client-muted` |
| A5 | `ClientVideoServerMutedSenderFrames` | client-alerts.yaml | warning | MH `#scenario-19-server-mute-not-taking-effect-at-ingress` |
| A6 | `ClientVideoDecodeErrorRatio` | client-alerts.yaml | warning | MC `#scenario-22-client-video-decode-failures` |
| A7 | `MCMediaMissingKeyMaterial` (re-derived per kind) | mc-alerts.yaml | warning | MC `#scenario-16-missing-key-material` |
| A8 | `MHTransportInconsistent` | mh-alerts.yaml | warning | `mh-deployment.md#mixed-transport-rollback-hazard` |
| A9 | `ClientVideoSlotUnfulfilled` | client-alerts.yaml | warning | MC `#scenario-23-client-video-slot-unfulfilled` |
| directive | `MCMediaSendDirectiveDefect` | mc-alerts.yaml (page group) | **page** (MC argued, operations agreed; security confirmed no token is client-reachable) | MC `#send-directive-defect-page` |
| directive | `MCMediaSendDirectiveFailures` | mc-alerts.yaml | warning | MC `#send-directive-wiring` |
| refresh | `MCMediaGenerationDivergence` (description/impact only; expr unchanged) | mc-alerts.yaml | page | unchanged |

### Recorded departures from the task text (each also in its rule block)
1. **A1 denominator** `sum(avg_over_time(mh_media_stream_ingress_groups[10m]))` (guard on the same expression) instead of the never-landed `mh_media_stream_sources_active`. Media-handler decision: the separate gauge is redundant (ingress_groups is the live active-source count; a declared-FORWARD gauge would still count unwatched sources). `_sources_declared` rejected (under-reads). No TODO, by the Lead's ruling.
2. **A1 `min()`** of the MC interval gauge (task: `max()`): A1's threshold falls as the interval rises; `min()` is the conservative side under an MC config skew. A3/A9 keep `max()`.
3. **A6/A9 windows `[5m]`** (task: `[10m]`/`[15m]`): G5 (`metric_expiration` 10m must strictly exceed every dt_client_* window). Raising expiration/max_stale was rejected (stream-cap headroom ~3 runs). collector.yaml unchanged.
4. **A9 `for: 5m`** (task: 15m): exit-time observations are sparse; `for:` must not exceed the window (observability Gate 1).
5. **Directive rule split into page + warning** with `assignment_failed` reclassified as an MC defect (the only render error, `EgressOrdinalOverflow`, is unreachable from valid input; it is meeting-wide). Partition home: `DirectiveOutcome::alert_class()` (`#[cfg(test)]`, exhaustive, wildcard-free) + unit test reading both regexes from mc-alerts.yaml.
6. **No `entry_point` label** for the rewrite split: panelled as codec tokens on ingress (decode) vs `relay_rewrite_failed` on egress (`rewrite_relay_region`).
7. **"Declared limit" for concurrent ingress groups** is per connection; shown as its own stat, never a line over the process-wide gauge.
8. **Backlog in-use gauge** (MH-A2 gap) ADDED in-loop as `mh_media_egress_stream_backlog_bytes`: tick-sampled in `routing/mod.rs::RoutingTable::publish_pending` (session actor, single writer) from `edge_runtime.rs` cell accessor `backlog_used()`; no per-chunk telemetry (Gate 3: a per-chunk process-wide atomic was rejected as hot-path contention by media-handler and operations). Blind spot: a writer still draining after its edge left the snapshot (short tail). Tested in `media_stream_abandonment_integration.rs` (>0 with queued bytes after a `publish_pending`, 0 after every exit). Catalog row; panel against `limit x stream ceiling` (the R-21 charge bound, not memory).

9. **A8 mismatch arm `increase(mh_media_frames_dropped_total{reason="transport_mismatch"}[20m]) > 0`** (task: `rate()[5m] > 0`; Gate 3 media-handler finding 8): on the stream path a refused stream can be a one-shot event (the SDK does not re-key after MH's STOP_SENDING; MH requests no keyframe for a datagram-declared source), and no other rule backstops it (its edges are datagram edges, never pending), so one refusal must hold the condition past `for: 15m`. The apply arm stays `rate()[5m]` (MC re-pushes; `MCMediaGenerationDivergence` backstops).

### Accepted residuals (recorded in blocks and alerts.md)
- **A6**: total video decode failure at directed intervals above ~3.3 s (30 fps) is unalerted while the slot is open; surfaces at slot exit on the unfulfilled histogram. The deployed 500 ms default is within reach (~3.3% vs 1%).
- **A2**: on fleets with fewer than ~30 active stream destinations (Kind) one slow subscriber can fire it (3p bound). No floor, by observability's ruling.
- **A3/A9**: an unreported capture that never starts (open permission prompt) holds until A3/A9 fire (story amendment 4's accepted residual).

### Dashboards (OBS-B layout; alert thresholds drawn on every alert-linked panel)
**mc-media.json** rows: Media Routing; Client Media Signalling; Slots and Mute by Kind (story 3); Video Send Directives (story 3); Media Connectivity; KEK Lifecycle.
| Panel (id) | Change | Threshold |
|---|---|---|
| Pushes Counted by MCMediaGenerationDivergence (191, new) | negated selector | red line 0 |
| Send Directive Failures (ALERTED) (192, new) | defect + negated selectors | red line 0 |
| Send Directives by Outcome (59) | retired "assignment_failed environmental" prose replaced by `alert_class()` pointer | — |
| Slot States by Kind (60) | `by (media_kind, slot_state)` | — |
| Receive Slot Fill vs Cap (187) | **fixed**: per-kind numerator/denominator/guard | — |
| Client Mute Requests by Kind and Outcome (62) / Server Mute Requests by Kind (178) | `by media_kind`, separate panels | — |
| Mean Send Targets per Directive (167) | per-kind numerator, kind-free denominator (`ignoring(media_kind) group_left`) | — |
| Join Refusals - Story 3 Media Join Values (195, new) | `mc_session_join_failures_total{error_type=~5 values}` | — |
| Unreachable Senders Named (160), Receive Slot Cap (158) | kind-free note; video=0 lever note | — |

**mh-media.json** rows: Forward Path and Frame Drops; Egress Admission and Policy Bounds; Stream Transport (stream-per-group, story 3); Keyframes, Recovery and Pending Edges (story 3). Type-blind titles (script-asserted: no "video" in any title).
| Panel (id) | Change | Threshold |
|---|---|---|
| Group Starts per Open Ingress Group vs Keyframe Ceiling (196, new) | A1 ratio | series `0.5 / min(mc gauge)` |
| Subscriber-Backlog Reset Share (197, new) | A2 ratio | red line 0.10 |
| Stream-Path Backlog In Use vs Charge Bound (198, new) | new gauge per pod | series `limit * ceiling` |
| Stream-Path Ingress Drops (20) | + `stream_rate_limited`, `partial_frame_discard`; per-stream counting note | — |
| MHTransportInconsistent Arms (201, new) | A8 both arms | red line 0 |
| Paused-Source Share of Ingress (202, new) | A4 ratio | red line 0.01 |
| Oldest Pending Edge (31) | + A3 threshold series | series `10 * max(mc gauge)` |
| Ingress/Egress Drops by Reason (4/5) | entry-point split note | — |
| Pending Duration (33) | SLO decomposition note | — |

**client-media.json** rows: Receive Path (per kind); Send Path (per kind); Key Lifecycle (kind-free); Video Send (story 3); Video Receive (story 3).
| Panel (id) | Change | Threshold |
|---|---|---|
| 2, 3, 4, 5, 6, 7, 8, 9, 11, 13, 20; mute half of 10 | split `by media_kind` | — |
| 12, 15, 16, 17, 18, 19, 21, 24, 25; KEK half of 10; header-version half of 14 | "kind-free by design" in description | — |
| Video From Server-Muted Senders (40, new) | A5 selector | red line 0 |
| Video Decoder Failures per Accepted Frame (41, new) | A6 ratio | red line 0.01 |
| Missing-Key-Material Drop Share by Kind (42, new) | A7 per-kind ratio | red line 0.05 |
| Video Receive Drops (story 3 tokens) (43, new) | 5 new tokens + `payload_length_exceeds_max` | — |
| Video Slot Unfulfilled Duration (p95) (35) | `media_kind="video"` + A9 threshold | series `10 * 1000 * max(mc gauge)` |
| 26, 27, 28, 22, 36, 37, 38, 39 | alert/no-alert decision and SLO notes | — |

### Docs
- `docs/observability/alerts.md`: A1–A9, both directive rules, divergence refresh, MH stream-path no-alert decisions (§Deliberate absences (MH) item 5), every client "none yet — story 3 OBS-B" row resolved; byte-identical PromQL (inventory guard: 54 pairs).
- `docs/observability/dashboards.md`: rows + panel tables for the three boards, `media_kind` split list.
- `docs/observability/slos.md`: two open objectives (join-to-first-rendered-video, slot-active-to-first-render) with histograms, measurement points, never-gated, closing evidence (Kind + real-network, story 8), MH pending-duration as server-side decomposition; Open items row.
- Catalog cross-refs: an **Alerts** bullet on every row an alert reads (mh: group starts/resets, ingress groups, sources_declared (not a denominator), pending oldest, paused, forwarded, policy applies, frames dropped; mc: interval gauge, send directives, withdrawals; client: dropped, received, accepted, decoder errors, unfulfilled); every client "Alert: none yet (story 3 OBS-B)" resolved; stale MH "alerting is story task 21's scope" corrected; MC `assignment_failed` reclassified (pointer to `alert_class()`, no restated list).
- `docs/TODO.md`: directive entry closed (DRY's new §Cross-Service Duplication entry left intact).
- Not changed: `infra/services/otel-collector/collector.yaml` (G5 resolved by windows).

### Hygiene applies-evidence (R-30, ADR-0036 §11)
- **MH forward/re-framing code is in the deny scope**: `scripts/guards/simple/media-telemetry-deny.yaml` denies `crates/mh-service/src/media/`, which holds `forward.rs`, `stream_ingress.rs`, `downlink.rs`, `edge_runtime.rs`, `keyframe.rs` (all stream forward and re-framing); `media-telemetry-deny.sh` → `STATUS=OK ... 12-files-1-dirs` after the new gauge (handle-only calls, no telemetry import).
- **SDK video encode/decode/receive loops are under `packages/sdk-core/src/media/pipeline/`** (`videoEgress.ts`, `videoIngress.ts`, `videoSlotPipeline.ts`, `streamReader.ts`, `renderDecision.ts`, `decoderRestartGate.ts`). They emit only through argument-free typed handle methods (`MediaKindMetrics`, e.g. `metrics.decoderError()`, `metrics.decodeQueueDropped()`), whose labels are fixed by the allow-list projection in `mediaMetrics.ts`; grep of those files finds no attribute bag, logger, span or test-bus call carrying a size, timing, GoP value or sender/stream id. The R-35 overlay counts (keyframes reached a slot) are never exported. NOTE: this is review evidence, not a mechanical scope — no TS equivalent of `media-telemetry-deny` covers `pipeline/` (the existing TS guards are `client-metrics-export` and `no-pii-in-logs-ts`).
- **env-test 32** sweeps every scraped series of every service job (`check_series`: no identity label, `key_custody=operator`, no `media_kind` on `mh_*`) and now also asserts presence of `STORY_3_EAGER_SERIES` (14 series incl. the new backlog gauge) so those checks cannot pass over an absence. Client series are checked via 31's GC path. Security-listed tokens: client `self_sender`, `sender_server_muted`, `decode_over_bound`, `stream_partial_reset`, `stream_slot_mismatch` are `reason` values on `dt_client_media_frames_dropped_total` (per-kind base: `client_version`, `org_id`, `key_custody`, `media_kind`); MH `unmatched_stream`, `transport_mismatch` (`reason` on `mh_media_frames_dropped_total`) and `source_mismatch`, `stale`, `counter_ahead`, `duplicate` (`outcome` on `mh_media_recovery_requests_received_total`) — all eager on MH, so 32's sweep sees them on every MH pod.
- **No identity in new panels/rules**: label sets used in `by`/`on`/`ignoring`/legends across the three boards = {action, direction, error_type, instance (MC/MH only), le, media_kind, mode, outcome, payload_kind, phase, reason, slot_state, source, surface, trigger}; client-media has no `instance` anywhere (script-asserted). New rule annotations template only `{{ $value }}`. Render-latency and pending-duration histograms are panelled `by (le[, media_kind])` only.

### Tests
- `crates/env-tests/src/fixtures/alert_rules_loaded.rs`: hand-rolled selector scanner `vector_selectors()` (byte spans; rustdoc records why and the three-way-extractor TODO), span-based `inject_marker()` (fail-closed recount, byte round-trip test, non-prefix selectors untouched), `formatted_query` / `loaded_alert_queries` / `compare_loaded_expr` (Match / Drift / CouldNotEvaluate); unit tests incl. a scan of every mounted rule expr.
- 33: stale "no dt_client_* reaches Prometheus" rustdoc corrected; `STORY_3_ALERTS` (12) positive control; `every_loaded_rule_expression_is_the_committed_one` (format_query both sides; EXPR-DRIFT vs COULD-NOT-EVALUATE); `story_3_rule_selectors_apply_to_real_series` (MH + eager MC gated; negated selectors by bare name; dt_client_* ACKNOWLEDGED-VACUOUS, proven by 31).
- FIRE (rule expr from YAML, instant query, no `for:` wait, no latency gate): 31 `test_story3_client_video_alerts_fire_and_apply` (A5, A6, A9; per-rule marker, FIRE-PRECONDITION-NOT-EMPTY / FIRE-NOT-OBSERVED; A9 value derived from the live MC gauge; buckets read from `SLOT_UNFULFILLED_BUCKETS_MS`); 37 phase (f) (A8, both arms); 39 phase (f) (A3, continuation-sending publisher that never starts a group).
- MC `send_directive_alert_rules_match_the_alert_class_partition` (also pins mc-media panel 192's two selectors); MH backlog gauge integration assertions; web-app `clientMetricNames.test.ts` asserts all three A7 sums are `by (media_kind)`.
- Not run here: every cluster env-test (no Kind cluster in this container) — the Lead's Layer 7.

### Gate 3 fixes (all findings fixed; none deferred)
| Reviewer | Finding | Fix |
|---|---|---|
| meeting-controller | F1 `meeting_state_unavailable` mis-described (6 sites) | rule block/description/impact, alerts.md, mc-service.md (2 lines), outcome.rs (2 docs) now: post-join re-declaration the actor did not register; previous view stays; shipped SDK never re-declares; runbook line by operations. Residual sentence on `ALL` completeness added to the warning block |
| security | S1 client-drivable A1/A3/A8 | a CLIENT-DRIVABLE line in each block and annotation; A8 "check one-client before any rollback" |
| media-handler | 1 `set(0)` on delta gauge; 7 per-chunk contention (with operations F-OPS-3) | gauge reworked to tick-sampled (`publish_pending`), downlink.rs reverted; finding 1 moot |
| media-handler | 2 per-pod reset view | Group Resets panel `by (instance, reason)`; A2 annotation names it |
| media-handler | 3 `partial_frame_discard` wording; 4 write-failures recovery claim; 5 paused increment site; 6 cutover wording | panel 20; alerts.md + A3 annotation + catalog (Alerts line names A3); catalog; "out of the required order (config -> MH -> MC -> SDK)" |
| code-reviewer | F1 allow→expect | moot (downlink revert); `#[expect]` with reason in `publish_pending` |
| code-reviewer | F2/test F4 eager-series SSoT; F3/test F6 helper duplication | one const in `fixtures/metric_hygiene.rs` used by 32 and 33 (every eager story-3 series, lazy ones named); `committed_alert_expr`, `mc_video_min_keyframe_interval_seconds`, `eventually_returns_series` helpers |
| code-reviewer | F4 A8 arm by index | arms selected by content; non-discriminating whole-expr check removed |
| code-reviewer | F5 main.md A9 `for:`; F6 "do not restate" claim; F7 TODO superseded guidance; F8–F11 naming/docs/keyword lists; F12 per-rule two-half tables | all fixed (F12: a DEMONSTRATION table per rule in all three files) |
| test | T1 circular client APPLY | `clientMetricNames.test.ts` binds every client-alerts.yaml name, `reason` and `media_kind` value to the SDK vocabularies (positive controls); 31/33 wording cites the real carriers |
| test | T2 A3 timeout; T3 A3 attribution; T5 mixed-mode registration | timeout = bound + PINNED_METRIC_BOUND; asserted FIRE-PRECONDITION; second refused registration with inconsistent candidate transports |
| observability | F1 A8 sparse arms | per-arm recurrence/backstop in the block and alerts.md |
| media-handler | 8 A8 stream-path backstop claim false (datagram edges never pend; one-shot) | false claim removed; mismatch arm widened to `increase()[20m] > 0` (departure 9); panel 201, alerts.md updated |
| observability | F2 MC metrics.rs stale rustdoc; F3 TODO; F4 client APPLY overclaim; F5 healthy reading of the warning; F6 A7 impact | fixed (F4 with test T1; F5: healthy reading is zero, argued in the block) |
| dry-reviewer | D1 panel 192 unpinned; D2 MC kind-free notes; D3 line-scanned YAML; D4 `now_unix_ns` dup; D5 `DirectiveOutcome::alert_class()` citation | outcome.rs test also reads panel 192; notes on 59/192/179/166; `serde_norway` structural parse (dev-dep); helper reused; `#[cfg(test)] impl DirectiveOutcome { const fn alert_class(self) }` |
| operations | F-OPS-1 superseded TODO guidance; F-OPS-2 rules ConfigMap headroom; F-OPS-3 hot path | inline SUPERSEDED marker; TODO filed (§Observability Debt, owner infrastructure, trigger before the next rules change); gauge tick-sampled |

### Gate 2 Layer 7 attempt 1 fix
`test_story3_client_video_alerts_fire_and_apply` (31) failed closed with `COULD-NOT-EVALUATE: SLOT_UNFULFILLED_BUCKETS_MS parsed as []`: the extractor took the first `[...]` after the name, which on the real typed declaration (`export const SLOT_UNFULFILLED_BUCKETS_MS: readonly number[] = [`) is the TYPE's `[]`. Replaced with a pure kernel `fixtures/alert_rules_loaded.rs::ts_numeric_array_const` that anchors on the declaration's `=`, requires an array-literal initialiser, reads to its `]`, strips line comments, accepts numeric separators, and errors on any non-numeric token, an empty list, a non-increasing list, a missing/duplicate declaration or a prefix-name match. Unit tests: the exact failing shape, comments/junk, the fail-closed cases, and a positive control against the REAL `mediaMetrics.ts` (last bound > 100 s). Also fixed a `clone_on_copy` clippy error in 37 (Layer 5 does not lint `--all-features`; caught locally).

### Observation for operations/infrastructure (not deferred work)
The `prometheus-rules` ConfigMap is now ~75% of the annotation cap (`dt-guard kustomize` fails at 80%; it reached 82% before this change's rule comments were condensed). Superseded at Gate 3: filed as a TODO at operations' request (F-OPS-2), because ~3% of runway would fail the next rules devloop on a budget it did not fill.

---

## Gate 1 — Plan Confirmations

Classification-sanity guard: `STATUS=OK REASON=cross-boundary-classification-clean-1-files`. Lead rulings: A1 denominator substitution (`avg_over_time(mh_media_stream_ingress_groups)`) accepted as a media-handler decision (no TODO); backlog-bytes gauge added in-loop; departures (A1 min(), A6/A9 [5m] under G5, directive split with PAGE agreed by operations) accepted.

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations (paired) | confirmed |
| Media Handler | confirmed |
| Meeting Controller | confirmed |
| Client | confirmed |

---

## Gate 2 — Full Validation (Lead)

| Attempt | Result | Notes |
|---------|--------|-------|
| 1 | TOTAL_RESULT=FAIL (L1–6 OK; L7 FAIL `env-tests-failed`) | `test_story3_client_video_alerts_fire_and_apply` (31): bucket extractor read the TS type annotation `readonly number[]` → `COULD-NOT-EVALUATE ... parsed as []` (failed closed). Fixed via `ts_numeric_array_const` kernel + unit fixtures; test + client re-confirmed. Consumed 1 of 2 L7 attempts. |
| 2 | TOTAL_RESULT=OK (L1 5s, L2 4s, L3 181s, L4 156s, L5 1s, L6 8s, L7 1480s) | `env-tests-passed`, `browser-e2e-passed`; gate2-verdict GATE2=PASS RUN_AT=2026-10-24T14:44:23Z |

## Code Review Results

Gate 3 (all verdicts final; per-finding detail in §Implementation Summary "Gate 3 fixes" table):

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 1 | 1 | 0 | S1 client-drivable A1/A3/A8 annotations; re-confirmed after A8 reshape; validate-alert-rules OK |
| Test | RESOLVED-FIXED | 6 | 6 | 0 | APPLY circularity → SDK-vocabulary binding; A3 timeout/precondition; shared eager list; 37 both producers |
| Observability | RESOLVED-FIXED | 7 | 7 | 0 | incl. F7 after A8 reshape |
| Code Quality | RESOLVED-FIXED | 12 | 12 | 0 | ADR-0002/0024/0029/0031/0036 §11 compliant; no GSA touched |
| DRY | RESOLVED-FIXED | 5 | 5 | 0 | 2 extraction opportunities filed to `docs/TODO.md` (ADR-0019, outside fix-or-defer) |
| Operations (paired) | RESOLVED-FIXED | 3 | 3 | 0 | 57 runbook_urls resolve; ConfigMap-split TODO filed (infra-owned); re-confirmed after A8 reshape with runbook alignment |
| Media Handler | RESOLVED-FIXED | 8 | 8 | 0 | backlog gauge tick-sampled off hot path; A8 mismatch arm `increase[20m]` |
| Meeting Controller | RESOLVED-FIXED | 1 | 1 | 0 | `meeting_state_unavailable` description at 6 sites |
| Client | CLEAR | 0 | 0 | 0 | no SDK emission defect |
| Semantic Guard | — | — | — | — | not spawned (no check surface) |

DRY extraction opportunities (`docs/TODO.md` §Cross-Service Duplication, "From DRY Reviewer — story 3 OBS-B (2026-10-24)"): PromQL selector scanners (3 Rust + 1 TS); alert-linked panels restating rule selectors/thresholds unguarded (panel 192 pinned).

---

## Accepted Deferrals

- (none surfaced in this devloop — every finding fixed)

Filed follow-up (not a deferred finding; the fix-now action was filing it, infra-owned):
- `docs/TODO.md` §Observability Debt — split prometheus-rules ConfigMap (~77% of annotation cap)

---

## Rollback Procedure

1. Start commit: `6a2fce835cd7da3c501ab213b099d2aaa29c5a0d`
2. `git diff 6a2fce83..HEAD`
3. `git reset --soft 6a2fce83` / `git reset --hard 6a2fce83`
4. No schema changes.
5. Prometheus rules/Grafana dashboards are applied by `dev-cluster deploy`; redeploy after revert.
6. **Safe-revert unit**: the whole commit. Rules, dashboards, env-test 33's committed-expression list, the `alert_class()` partition test (reads mc-alerts.yaml + mc-media.json) and the operations runbook anchors/text move together; a partial revert leaves tests asserting rules that are absent or runbooks naming alerts that do not exist.

---

## Issues Encountered & Resolutions

- `mh_media_stream_sources_active` (MH-B) never landed → A1 uses `avg_over_time(mh_media_stream_ingress_groups)` (media-handler decision: separate gauge redundant).
- MH-A2 had no backlog in-use gauge → `mh_media_egress_stream_backlog_bytes` added in-loop; first per-chunk design rejected (hot-path contention), reworked to tick-sampled in `RoutingTable::publish_pending`.
- `assignment_failed` mislabelled environmental → reclassified MC defect; directive rule split into PAGE (`MCMediaSendDirectiveDefect`) + WARNING (`MCMediaSendDirectiveFailures`, negated selector), operations agreed.
- G5 collector-expiry rule forced A6/A9 to `[5m]`.
- prometheus-rules ConfigMap reached 82% of the annotation cap (guard 80%) → rule comments condensed to ~77%; split TODO filed.
- Lead note: specialist INDEX.md and review-protocol were passed by path (teammates read them in full) rather than inlined.
