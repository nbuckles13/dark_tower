# Grafana Dashboards Catalog

This document catalogs all Grafana dashboards for Dark Tower services.

## Dashboard Organization

Dashboards are organized by:
- **Service**: Per-service operational dashboards (AC, GC, MC, MH)
- **Function**: Cross-service functional dashboards (SLOs, Security, Platform)
- **Environment**: Same dashboards used in dev, staging, and production

All dashboard JSON files are stored in `infra/grafana/dashboards/` and auto-loaded via Grafana provisioning.

---

## Global Controller Dashboards

### GC Overview

**File**: `infra/grafana/dashboards/gc-overview.json`
**UID**: `gc-overview`
**Tags**: `gc-service`, `service-overview`

**Purpose**: Primary operational dashboard for Global Controller service.

**Panels**:
1. **HTTP Request Rate by Endpoint** - Request rate (req/sec) across all endpoints
2. **HTTP Request Latency (P50/P95/P99)** - Latency percentiles with 200ms SLO line
3. **HTTP Error Rate (%)** - Gauge showing error rate with thresholds (green <0.1%, yellow 0.1-1%, red >1%)
4. **HTTP Status Codes** - Breakdown by 2xx/4xx/5xx status codes
5. **MC Assignment Rate by Status** - Assignment rate (ops/sec) by success/rejected/error status
6. **MC Assignment Latency (P50/P95/P99)** - Assignment latency with 20ms SLO line
7. **MC Assignment Success Rate (%)** - Gauge showing success rate (green >99%, yellow 95-99%, red <95%)
8. **Database Query Rate by Operation** - Query rate by operation type
9. **Database Query Latency (P50/P95/P99)** - DB query latency with 50ms SLO line
10. **Service Status** - Up/down status gauge
11. **Pod Count** - Number of running GC pods
12. **Memory Usage** - Memory consumption per pod
13. **CPU Usage** - CPU utilization per pod
14. **Token Refresh Rate by Status** - Token refresh attempts by success/error status
15. **Token Refresh Latency (P50/P95/P99)** - Token refresh latency percentiles
16. **Token Refresh Failures by Type** - Token refresh failures by error type
17. **AC Request Rate by Operation** - Requests to Auth Controller by operation and status
18. **AC Request Latency (P50/P95/P99)** - AC client request latency percentiles
19. **gRPC MC Call Rate** - gRPC calls to Meeting Controllers by method and status
20. **gRPC MC Call Latency (P50/P95/P99)** - gRPC call latency to Meeting Controllers
21. **MH Selection Rate by Status** - Media Handler selection attempts by status
22. **MH Selection Latency (P50/P95/P99)** - MH selection latency percentiles
23. **Meeting Creation Rate by Status** - Meeting creation attempts by success/error status
24. **Meeting Creation Latency (P50/P95/P99)** - Meeting creation latency percentiles (p50, p95, p99)
25. **Meeting Creation Failures by Type** - Meeting creation failures by error type (bad_request, forbidden, db_error, etc.)
26. **Meeting Join Rate by Status** - Meeting join attempts by success/error status
27. **Meeting Join Latency (P50/P95/P99)** - Meeting join latency percentiles (p50, p95, p99)
28. **Meeting Join Failures by Type** - Meeting join failures by error type (not_found, mc_assignment, ac_request, etc.)
29. **Meeting Join Success Rate (%)** - Gauge showing join success rate (green >99%, yellow 95-99%, red <95%)
30. **Registered Controllers by Type & Status** - Fleet health by controller type and status
31. **Errors by Operation & Type** - Error rate by operation and error type

**Metrics Used**:
- `gc_http_requests_total`
- `gc_http_request_duration_seconds`
- `gc_mc_assignments_total`
- `gc_mc_assignment_duration_seconds`
- `gc_db_queries_total`
- `gc_db_query_duration_seconds`
- `up{job="gc-service"}`
- `container_memory_usage_bytes`
- `container_cpu_usage_seconds_total`
- `gc_token_refresh_total`
- `gc_token_refresh_duration_seconds`
- `gc_token_refresh_failures_total`
- `gc_ac_requests_total`
- `gc_ac_request_duration_seconds`
- `gc_grpc_mc_calls_total`
- `gc_grpc_mc_call_duration_seconds`
- `gc_mh_selections_total`
- `gc_mh_selection_duration_seconds`
- `gc_meeting_creation_total`
- `gc_meeting_creation_duration_seconds`
- `gc_meeting_creation_failures_total`
- `gc_meeting_join_total`
- `gc_meeting_join_duration_seconds`
- `gc_meeting_join_failures_total`
- `gc_registered_controllers`
- `gc_errors_total`

**Default Time Range**: Last 1 hour
**Refresh**: 10 seconds

**When to Use**: Day-to-day operations, investigating performance issues, capacity planning

---

### GC SLOs

**File**: `infra/grafana/dashboards/gc-slos.json`
**UID**: `gc-slos`
**Tags**: `gc-service`, `slo`

**Purpose**: SLO compliance tracking and error budget monitoring for Global Controller.

**Panels**:
1. **Availability SLO - Error Budget Remaining** - Gauge showing remaining error budget for 30-day window (99.9% target)
2. **Error Budget Burn Rate** - Burn rate over time (1h and 6h windows) with sustainable rate line
3. **Availability Trend (7d / 28d)** - 7-day and 28-day availability percentage with 99.9% SLO line
4. **HTTP Latency SLO - Current p95** - Gauge showing current p95 latency vs 200ms target
5. **HTTP Latency SLO - Compliance %** - Percentage of requests meeting p95 <200ms
6. **HTTP Latency Distribution (Histogram)** - Request distribution across latency buckets
7. **MC Assignment SLO - Current p95** - Gauge showing current assignment p95 vs 20ms target
8. **MC Assignment SLO - Compliance %** - Percentage of assignments meeting p95 <20ms

**Metrics Used**:
- `gc_http_requests_total`
- `gc_http_request_duration_seconds_bucket`
- `gc_mc_assignment_duration_seconds_bucket`

**Default Time Range**: Last 1 hour
**Refresh**: 10 seconds

**When to Use**: Weekly SLO reviews, incident post-mortems, performance trend analysis

**Related Alerts**:
- `GCHighErrorRate` (fires when availability SLO violated)
- `GCHighLatency` (fires when latency SLO violated)
- `GCMCAssignmentSlow` (fires when assignment SLO violated)
- `GCErrorBudgetBurnRateCritical` (fires when error budget burning >10x)

---

## Authentication Controller Dashboards

### AC Overview

**Status**: ✅ Exists
**File**: `infra/grafana/dashboards/ac-overview.json`

**Panels**:
- Token issuance rate
- Token validation rate
- Token issuance latency — SLO target and quantile per `slos.md` (authoritative; do not restate a
  threshold here)
- Token validation latency — SLO target and quantile per `slos.md` (authoritative; do not restate a
  threshold here)
- Key rotation status
- JWKS cache hit rate

---

## Meeting Controller Dashboards

### MC Overview

**File**: `infra/grafana/dashboards/mc-overview.json`
**UID**: `mc-overview`
**Tags**: `mc-service`, `service-overview`

**Purpose**: Primary operational dashboard for Meeting Controller service.

**Rows**: Traffic Summary, Security Events, Redis & Recovery, Fencing & Heartbeat Latency, Join Flow,
MH Coordination. The MH Coordination row carries meeting teardown (EndMeeting outcomes, push
quiesce, teardown fence backstop) beside RegisterMeeting, which is where a responder triaging MC to
MH already looks. MC's ADR-0036 media-path rows live in **MC Media Path** below; the two dashboards
link to each other.

**Panels and their metrics**: read them from the JSON — it is the source of truth (ADR-0031). This
page deliberately carries no per-panel list: a hand-maintained copy drifted several rows behind the
JSON before it was removed (O-21, `docs/TODO.md`).

**Default Time Range**: Last 1 hour
**Refresh**: 10 seconds

**Related Alerts** (Join Flow):
- `MCHighJoinFailureRate` (fires when join failure rate >5%)
- `MCHighJoinLatency` (fires when join p95 >2s)
- `MCHighWebTransportRejections` (fires when rejection rate >10%)
- `MCHighJwtValidationFailures` (fires when JWT failure rate >10%)

**When to Use**: Day-to-day MC operations, investigating join failures, WebTransport issues, capacity planning

### MC Media Path

**File**: `infra/grafana/dashboards/mc-media.json`
**UID**: `mc-media`
**Tags**: `mc-service`, `media`
**ConfigMap**: `grafana-dashboards-mc-media` (its own; see *Why it is a separate file* below)

**Purpose**: MC's side of the ADR-0036 media path — the counterpart of **MH Media Path**.

**Rows**: Media Routing (ADR-0036 §8), Client Media Signalling (ADR-0036 §5, §6), Slots and Mute by
Kind (story 3), Video Send Directives (story 3), Media Connectivity (ADR-0036 §9), KEK Lifecycle
(ADR-0036 §4 Rotation).

**Story 3 (OBS-B) layout and alert-linked panels.** Every alert-linked panel draws its alert's
threshold: a gauge-derived threshold as a second plotted series, a fixed policy number or an
any-occurrence rule as a red threshold line.

| Panel | Split / alert | Note |
|---|---|---|
| Pushes Counted by MCMediaGenerationDivergence (ALERTED, page) | `MCMediaGenerationDivergence`'s negated selector, line at 0 | per-stream `transport_mode_mismatch` since story 3 |
| Send Directive Failures (ALERTED: defect pages, rest warns) | `MCMediaSendDirectiveDefect` and `MCMediaSendDirectiveFailures` selectors, line at 0 | classes: `DirectiveOutcome::alert_class()`; withdrawals never read here |
| Slot States by Kind | `by (media_kind, slot_state)` | `source_muted` is in no query; unreachable is NOT a slot state here |
| Receive Slot Fill vs Cap | per `media_kind` (fixed: previously summed both kinds against per-kind caps) | |
| Receive Slot Cap | per `media_kind` | video at 0 = the R-26 lever applied |
| Unreachable Senders Named | kind-free by design | read beside Not-Yet-Connected Senders |
| Client Mute Requests by Kind and Outcome / Server Mute Requests by Kind | `by (media_kind, ...)`, SEPARATE panels | never one panel summing client and server mute (`label-taxonomy.md`, `action`) |
| Send Directive Withdrawals by Kind and Reason | `by (media_kind, reason)` | withdrawals are healthy |
| Directed Video Minimum Keyframe Interval | stat | threshold source for A1, A3 and A9 |
| Join Refusals - Story 3 Media Join Values | `by (error_type)` | `reconnect_publish_set_mismatch` not production-reachable today |
| Mean Send Targets per Directive | per-kind numerator over the kind-free directive denominator (`ignoring(media_kind) group_left`) | |

**Panels and their metrics**: read them from the JSON, as for MC Overview (ADR-0031).

**Why KEK Lifecycle is here, when there is no KEK row on MH Media Path.** Do not read this dashboard
as a row-for-row mirror of MH's: MH never holds media keys, by ADR-0036 §4's design, so MH has no
key-custody row at all. KEK Lifecycle is here because §4 key custody is the key half of the media
path. **A responder asking "why can't this participant decrypt?" opens this dashboard, not MC
Overview.**

**Why it is a separate file, and in its own ConfigMap.** Client-side apply stores each ConfigMap as
JSON-escaped text in the `last-applied-configuration` annotation, which Kubernetes caps at
262,144 bytes. By story 2 task 12, `mc-overview.json` alone escaped to 99.7% of that, and the
shared MC ConfigMap failed cluster setup outright. Moving the media-path rows out, into a ConfigMap
of their own, gives both files real headroom. `dt-guard kustomize` now fails Layer 3 when any
dashboard ConfigMap passes its headroom threshold, and (R-21) when a generated dashboard ConfigMap is
not a source of Grafana's projected `dashboards` volume (which would make it silently invisible).

**Default Time Range**: Last 6 hours
**Refresh**: 1 minute

**Related Alerts**: `MCMediaGenerationDivergence`, `MCMediaSendDirectiveDefect`, `MCMediaSendDirectiveFailures`, `MCMediaMissingKeyMaterial` (its per-kind ratio is drawn on **Client SDK Media Path**),
`MCKekRotationOverdue`, `MCKekPushFailureRate`, `MCKekRotationStorm`, `MCClientKekConflictingKey`
and `MCClientRosterKeyRebind` (both client-origin tripwires, panelled on **Client SDK Media Path**) (for the last two named on
key issuance, and for `MCMediaMissingKeyMaterial`, issuance itself is read on MC Overview; see below).

**Cross-dashboard reads.** Meeting teardown stays on MC Overview (MH Coordination) while policy
generations are here (Media Routing), so a wedged teardown reads both. `mc_media_server_muted_sources`
(Client Media Signalling) is read against MH's `server_muted` drops on **MH Media Path**. KEY
ISSUANCE is on MC Overview, not here: **Meeting KEK Issuance by Trigger**
(`mc_meeting_kek_generated_total{trigger}`, Join Flow row) is a join-time event, so step 1 of "why
can't this participant decrypt?" — did the meeting ever get a key? — is read there before this
dashboard's KEK Lifecycle row.

**When to Use**: "I can't hear anyone", one-way audio, a participant who cannot decrypt, server mute
not taking effect (including "camera light still on after a server mute": Send Directive Withdrawals
`reason="server_muted"` is MC's side of it), key rotation stalls.

---

## Media Handler Dashboards

### MH Overview

**Status**: ✅ Exists
**File**: `infra/grafana/dashboards/mh-overview.json`

**Panels**: see `infra/grafana/dashboards/mh-overview.json` — the JSON is the list.

> **This section used to enumerate five panels and all five were fiction.** None of "Packet
> forwarding rate", "Audio forwarding latency", "Video forwarding latency", "Packet loss rate" or
> "Codec usage distribution" existed in the file, which carried 23 entirely different panels at the
> time — those five were an aspiration for the media path that has not landed. A hand-maintained
> copy of a panel list drifts silently, because nothing fails when the JSON changes and this list
> does not; the same defect was removed from `crates/mh-service/src/observability/mod.rs` in the
> same devloop, and the remedy there and here is a pointer rather than a corrected copy. Aspirational
> panels belong in the story that will build them, not in a list a reader will take for present tense.
> **§AC Overview has the identical defect** (6 aspirational bullets against 31 real panels) and is
> tracked in `docs/TODO.md` §Observability Debt.

**SLO-bearing panels**: SLO targets and measurement points live in `slos.md`, which is
authoritative — do not restate a threshold here.

> **Jitter panel removed.** The MH audio-jitter objective is **struck as unmeasurable** — MH forwards
> and does not buffer, and perceived jitter is a client-side jitter-buffer property (ADR-0011
> amendment 2026-09-02; ADR-0036 amendments table). No `*jitter*` metric is emitted by any service,
> so a jitter panel could only ever render "No data". Do not re-add one; a client-side successor is
> deferred with jitter-buffer design. See `slos.md` §Struck.

---

### MH Media Path

**File**: `infra/grafana/dashboards/mh-media.json` | **UID**: `mh-media`
**ConfigMap**: `grafana-dashboards-mh-media` (its own since story 3 MH-A2: the stream-path panels took the shared MH group past `dt-guard kustomize`'s 80% annotation headroom, the same split `mc-media.json` got)

The media-path **triage** board: where an operator goes to answer *why*, after
`mh-overview.json` has told them *whether*.

| Panel | Question it answers |
|---|---|
| Media Forward Latency by Phase (p95 / p99) | Which of three phases is slow — receive-buffer, processing, or transmit-buffer |
| Ingress / Egress Drops by Reason | Which condition dropped frames, on which side |
| Zero-Forever Invariant Drops | Whether an invariant that should never fire has fired |
| Egress Delivery Ratio (per-edge, egress-internal) | What fraction of per-edge egress attempts were forwarded: egress forwards over egress forwards plus per-edge egress drops. Egress-internal since story 2 (R-28): one accepted ingress frame becomes E egress attempts, so egress is never compared with ingress here. `no_subscriber` counts per FRAME, so it is excluded from the per-edge denominator |
| Mean Fan-out (egress forwards per ingress forward) | Egress over ingress forwards, with the denominator guarded. Its expected value is the mean subscriber count per forwarded ingress frame. This is where egress and ingress ARE compared, as a ratio of units rather than a delivery rate |
| Egress Stream Admission Decisions by Outcome | Is MH refusing new streams for capacity, and how often (story 2 R-19) |
| Admission Rejection Ratio vs Threshold | Is the windowed refusal share above the configured threshold — the same bare gauge-to-gauge comparison the exhaustion alert makes |
| Installed Egress Streams vs Stream Ceiling | How close each handler is to its stream ceiling (the comparison `MHMediaEgressEdgeHeadroomLow` reads; fork LEAK vs LOAD first), and whether that ceiling is still the unsized placeholder (below the advisory recommended minimum). Saturation is against the ceiling, never the limit. Streams are released when a meeting's MC calls `EndMeeting` (MC calls it from story 2 task 12); installed streams rising with pod uptime rather than load are meetings nobody ended — recovery under "The ratchet" in `docs/observability/metrics/mh-service.md` |
| Egress Edges vs Edge-Limit Backstop | The `MH_MAX_TOTAL_EGRESS_EDGES` backstop per pod. It is not the saturation denominator: the ceiling binds first, and MH refuses to start otherwise |
| Registered Meetings vs Registration Cap | How many meetings each handler holds registered against `MH_MAX_REGISTERED_MEETINGS` (story 2 R-21) — registrations held, not meetings in progress |
| Meeting Teardowns by Outcome | Are meetings being released, is MC ending meetings this handler never held, and is any MC ending a meeting it does not own (story 2 R-20) |
| Egress Budget (bytes/s) and Stream Ceiling | What budget the ceiling is derived from, in bytes (the key is in bits) |
| Late Applies Refused (Released Meeting) | Is the released-meeting guard working — how many queued policy applies MH's session actor refused because `EndMeeting` had already released the meeting (the only signal for that guard; a flat 0 is healthy, an ABSENT series is a broken pipeline) |
| Registration Policy Bounds | What per-registration pre-allocation bounds the running process loaded (config reflection; a step means a pod started on different config) |
| Policy Apply Timeout | What config-apply reply timeout the running process loaded, in seconds (config reflection; a step means a pod started on different config) |
| Per-Stream Egress Costs (bytes/s) | What enforced per-stream audio and video costs the running process loaded, in bytes (config reflection; a step means a pod started on different config) |
| Stream-Path Group Starts by Direction (stream-per-group) | Stream-per-group churn (story 3 MH-A2): attributed publisher groups in, downlink groups opened out. MH is type-blind; MC's assignment is what makes stream-transport traffic video |
| Stream-Path Group Resets by Reason (per destination) | Which downlink groups MH abandoned for one destination, and why (`subscriber_backlog`, `publisher_reset`, `publisher_stall`, `source_ended`, `edge_removed`). `increase()`; one per group abandoned for a destination, whether or not its stream opened |
| Live Stream-Path Ingress Groups and Declared Stream Sources | Publisher group streams being read now (process-wide; `MHVideoKeyframeStorm`'s denominator) and `STREAM_PER_GROUP` sources declared across installed policies (NOT a denominator: it counts paused, muted and unwatched sources) |
| Uni Streams per Connection Limit / Egress Backlog Cap per Destination / Mid-Frame Stall Bound (config) | The per-connection uni-stream limit (count), the per-destination backlog cap (bytes) and the mid-frame stall bound (seconds) the running process enforces — one panel each, in its own unit (config reflection). The per-connection limit is never drawn as a line over the process-wide groups gauge |
| Group Starts per Open Ingress Group vs Keyframe Ceiling | `MHVideoKeyframeStorm`'s ratio against its threshold series `0.5 / min(mc_media_video_keyframe_min_interval_seconds)` |
| Subscriber-Backlog Reset Share | `MHVideoSubscriberBacklogResets`' ratio, line at 10%; over-reads, and a single slow path can cross it on a small fleet |
| Stream-Path Backlog In Use vs Charge Bound | `mh_media_egress_stream_backlog_bytes` (added at story 3 OBS-B; tick-sampled every 10 s from per-destination budgets) per pod against cap x stream ceiling, the R-21 boot-check charge term — not memory |
| Stream-Path Ingress Drops (attribution, rate limit, partial frame) | `unmatched_stream`, `transport_mismatch`, `stream_rate_limited` (once per refused stream), `partial_frame_discard`; per-path drops are not alerted |
| MHTransportInconsistent Arms (ALERTED) | both arms, line at 0 |
| Paused-Source Arrivals / Paused-Source Share of Ingress (ALERTED) | arrivals per pod; `MHPausedSourceIngress`' ratio, line at 1% |
| Keyframe Requests Sent / Recovery Requests by Outcome / Keyframe Request Write Failures | MH's keyframe demand by trigger, receiver requests by outcome (`duplicate` is the dedupe bound working), undeliverable requests |
| Pending Edges / Oldest Pending Edge / Pending Duration / Pending Exits by Outcome | Oldest Pending Edge draws `MHVideoEdgeStuckPending`'s threshold series `10 * max(mc_media_video_keyframe_min_interval_seconds)`; Pending Duration is the server-side decomposition of the slot-active-to-first-render open objective |

**Rows** (story 3 OBS-B): Forward Path and Frame Drops; Egress Admission and Policy Bounds; Stream Transport (stream-per-group); Keyframes, Recovery and Pending Edges. **Type-blind**: no MH panel title or legend claims a series is "video" from an MH label (MH carries no `media_kind`); stream-transport panels say in their descriptions that MC's assignment is what makes that traffic video. **Entry-point split**: there is no `entry_point` label — codec reject tokens on Ingress Drops by Reason are the decode entry point, `relay_rewrite_failed` on Egress Drops by Reason is the `rewrite_relay_region` entry point.

Two conventions this board depends on, both easy to break by well-meaning edit:

- **The latency phases are never aggregated here**, and `total` is excluded by selector.
  The three phases have different remedies; a total does not say which to pursue. `total`
  additionally spans a client-influenced phase — see `docs/observability/slos.md`
  §Open: which series the objective attaches to.
- **The drop panels use `increase()`, not `rate()`**, and carry **no `or vector(0)`**. At
  any realistic scrape cadence a single discrete drop under `rate()` renders as a near-zero
  rate, which reads identical to healthy on the one panel whose purpose is that a single drop is visible;
  and `or vector(0)` would fabricate a series and flatten the by-reason breakdown.
- **The egress-admission panels preserve the "empty = healthy" reading.** Every series
  they read is registered eagerly at MH startup: the admission counter's outcomes are
  zero-initialised, the budget / ceiling / recommended-min / threshold gauges are published
  right after the recorder installs, and the ratio and installed-streams gauges are published
  when the session actor is built. A new series added to this board must be registered the
  same way, or it breaks this board-wide reading. The admission counter panel uses
  `increase()` for the same single-event reason as the drop panels.

### Client SDK Media Path

**File**: `infra/grafana/dashboards/client-media.json` | **UID**: `client-media`

Receive- and send-path counters from the browser SDK, organised around the receive-path
accounting identity `received = accepted + sum(drops by reason)`.

> **This board is live.** The collector's `prometheus` exporter and the `otel-collector`
> scrape job landed together with the GC filter widening; verified end to end from
> sdk-core's own built bundle through the GC proxy on the cluster. A previous note here
> described the board as non-functional in its entirety — that was true when written and
> is recorded rather than silently dropped, because the *reason* it was built before it
> could work still governs it: ADR-0036 §11 makes the client drop counters the only
> signal for a join or rotation path that has silently stopped delivering keys.
>
> **An empty panel here is AMBIGUOUS, and that ambiguity is new.** It no longer means
> "unwired"; it now spans *no drops occurred*, *no browser connected recently*, and *the
> pipeline broke*. The board carries a three-step discriminator in its READ FIRST panel:
> is the collector scraped (`up{job="otel-collector"}`), has any browser emitted within
> the collector's `metric_expiration` window, and only then does empty mean zero.
>
> **Step two is the only liveness signal the client fleet has.** There is no per-browser
> `up` and there structurally cannot be one — a per-session `service.instance.id` is
> barred by §11 — so series presence is the whole of it. That is a standing property of
> the design, not a gap awaiting work.
>
> **The empty-panel meaning on this board is still the OPPOSITE of `mh-media.json`'s, and
> the distinction got SHARPER rather than going away.** There, empty means no drops
> occurred — unambiguously healthy, because MH registers its media series eagerly at
> startup and they exist at zero whether or not anything flows. Here, client series are
> created lazily on first emission, so absence and zero are genuinely different states.
> Do not copy wording between the two boards in either direction.

**Story 2 task 16 decided an operator surface for every counter task 7 exported**, because a panel nobody browses is not a surface for a counter whose whole value is that something watches it:

- **Group A, alert plus panel.** The rows are marked ALERTED in their titles, and the rules are presence-shaped:
  - `dt_client_media_kek_retention_violations_total` → `ClientKekRetentionViolation`, in `client-alerts.yaml`;
  - `dt_client_media_kek_install_refusals_total{outcome="conflicting_key"}` → `MCClientKekConflictingKey`;
  - `dt_client_media_roster_key_rebinds_total{outcome="rebind"}` → `MCClientRosterKeyRebind`.

  These panels plot the raw summed value, not `increase()`, because the series appears already at ≥ 1.
- **Group B, panel only:**
  - `kek_retention_anomalies_total` (`floor_substituted` and `ceiling_clamped` are expected-non-zero configuration and rollback states, NEVER alert inputs);
  - the other two install-refusal arms;
  - `decode_queue_dropped_total` (post-accept, never part of the accounting identity);
  - **KEK Rotations vs Generations Retained**, which reads `kek_generations_retained_total` AGAINST `kek_updates_total{source="kek_update"}`. Rotations rising while retentions stay flat is the signal, and neither series alone says anything.
  - story 3 C-A2's video send-path series, panel only BY DECISION (story 3 OBS-B; reasons in `docs/observability/alerts.md` §Deliberate absences): `keyframes_produced_total{source}` (the diagnosis panel for `MHVideoKeyframeStorm`), `send_groups_abandoned_total`, `send_groups_stopped_total{outcome}`, `encode_queue_dropped_total`, `encoder_errors_total`, `send_directive_encoding_refused_total`, and `keyframe_min_interval_clamped_total`.
  - story 3 C-B's video receive series: `recovery_requests_sent_total{source}`, `incoming_streams_refused_total`, `slots_unfulfilled` (single-writer gauge), `active_source_intervals_total` (distinct active video sources), `time_to_first_render_ms` and `slot_active_to_first_render_ms` (open objectives, never gated; `docs/observability/slos.md`) are panel only BY DECISION. `slot_unfulfilled_duration_ms` is ALERTED (below).
- **Story 3 OBS-B alert-linked panels**, each drawing its alert's threshold: Video From Server-Muted Senders (`ClientVideoServerMutedSenderFrames`, line at 0), Video Decoder Failures per Accepted Frame (`ClientVideoDecodeErrorRatio`, line at 1%), Missing-Key-Material Drop Share by Kind (`MCMediaMissingKeyMaterial` per kind, line at 5%), and Video Slot Unfulfilled Duration (p95) with `ClientVideoSlotUnfulfilled`'s threshold series `10 * 1000 * max(mc_media_video_keyframe_min_interval_seconds)`.

**Rows** (story 3 OBS-B): Receive Path (per kind); Send Path (per kind); Key Lifecycle (kind-free); Video Send; Video Receive.

**`media_kind` split** (SSoT: `docs/observability/metrics/client.md` §`media_kind`). Split by `media_kind`: Frames Received / Accepted, Frame Drops by Reason, Non-Dropping Wrapped-Key Outcomes, Downlink Continuity and Decode, Undeclared Stream IDs, Silent Active Sources, Decode Queue Drops, Datagrams Sent, Send Drops by Reason, Egress Queue Depth, Time to First Media Frame, the mute half of Key and Mute Lifecycle, Directive Faults' kind-missing series, and every video panel. **Kind-free by design** (session-level series; each description says so): Capture Source, the three Key Tripwires, KEK Retention Anomalies, KEK Install Refusals, KEK Rotations vs Generations Retained, Keyframe Requests Without a Kind, MH Control Channel Ended, Directive Faults' header-version series, and the KEK half of Key and Mute Lifecycle.

The capture-source panel's value is not a browser count: the only signal is the presence of `mode="test_tone"`. The panel list itself is in the JSON.

## Platform Dashboards

### Service Health Overview

**Status**: 🚧 To be created
**File**: `infra/grafana/dashboards/platform-overview.json` (planned)

**Purpose**: Single-pane-of-glass view of all Dark Tower services.

**Planned Panels**:
- Service status (up/down) for AC, GC, MC, MH
- Request rate across all services
- Error rate across all services
- Latency percentiles across all services
- Kubernetes pod health

---

### Database Performance

**Status**: 🚧 To be created
**File**: `infra/grafana/dashboards/database-performance.json` (planned)

**Purpose**: PostgreSQL performance monitoring across all services.

**Planned Panels**:
- Query latency by service
- Connection pool utilization
- Database CPU/memory usage
- Slow query count
- Replication lag

---

## Dashboard Standards

All dashboards must follow these standards (per ADR-0011):

### 1. PromQL Query Requirements

- ✅ Use cardinality-safe labels only (no unbounded values like UUIDs)
- ✅ Aggregate with `sum by(label)` to control cardinality
- ✅ Use rate() for counters, histogram_quantile() for histograms
- ✅ Validate queries against actual metrics (cross-reference `crates/*/src/observability/metrics.rs`)

### 2. Panel Configuration

- ✅ Add descriptive panel descriptions (what metric measures, why it matters)
- ✅ Set appropriate units (seconds, bytes, percent, requests/sec)
- ✅ Configure color thresholds (green=good, yellow=warning, red=critical)
- ✅ Include SLO threshold lines where applicable

### 3. Time Range and Refresh

- ✅ Default time range: Last 1 hour
- ✅ Auto-refresh: 10 seconds (adjustable)
- ✅ Time range selector visible

### 4. Privacy

- ❌ No PII in panel titles, queries, or annotations
- ❌ No unbounded labels (user_id, meeting_id, email, etc.)
- ✅ Use hashed or aggregated identifiers only

### 5. Legends and Tooltips

- ✅ Use meaningful legend labels (template with `{{label}}`)
- ✅ Show mean and last values in legend (calcs: ["mean", "lastNotNull"])
- ✅ Enable multi-series tooltips

---

## Dashboard Deployment

### Local Development

Dashboards are automatically loaded via Docker Compose:

```yaml
# docker-compose.yml
services:
  grafana:
    volumes:
      - ./infra/grafana/dashboards:/etc/grafana/provisioning/dashboards
```

### Kubernetes (Staging/Production)

Dashboards are **content-addressed ConfigMaps mounted into the Grafana pod** (ADR-0038 §2):

**How it works:**
1. `infra/grafana/kustomization.yaml` holds a **static** `configMapGenerator` list
   (`grafana-dashboards-ac`, `-gc`, `-mc`, `-mc-media`, `-mh`, `-client`, `-errors`).
   Nothing is auto-discovered. **Grouping is size-bounded, not prefix-bound**: dashboards
   are grouped by service prefix only while the group's ConfigMap fits the annotation cap
   (see the comment above the groups in `kustomization.yaml`); `-mc-media` is MC's media
   path split out of `-mc` for that reason.
2. Each generated ConfigMap carries a **content hash in its name**. Kustomize rewrites the
   Grafana Deployment's references, so editing any dashboard changes the pod template:
   **applying the environment root rolls Grafana, with no manual restart step.**
3. `infra/grafana/deployment.yaml` mounts every group into `/var/lib/grafana/dashboards`
   through **one `projected` volume**. The file provider in
   `infra/grafana/provisioning/dashboards/dashboards.yaml` reads that path. The sources are
   not `optional:`, so a group missing at runtime fails pod start loudly. There is no sidecar,
   no label selection and no Kubernetes API access. Grafana does not automount a
   ServiceAccount token.

**Adding a new dashboard:**

1. Add the JSON to `infra/grafana/dashboards/`, named `{prefix}-{name}.json` (the filename
   must be unique across groups, since all groups mount into one directory).
2. Register the basename in `infra/grafana/kustomization.yaml` under the matching
   `grafana-dashboards-{prefix}` group. `dt-guard kustomize` R-20 enforces this
   bidirectionally: an unregistered dashboard and a registered-but-absent file both turn
   Layer 3 red.
3. **If you created a new group**, add it as a source of the `dashboards` projected volume
   in `infra/grafana/deployment.yaml`. `dt-guard kustomize` R-21 fails the build on a
   generated ConfigMap that no pod template references, so forgetting this step is red,
   not silent.
4. Deploy by applying the environment root (Kind: `./infra/kind/scripts/deploy.sh`, or
   `dev-cluster deploy` from the devloop container — it applies the whole root and waits for
   every workload). The new hash rolls Grafana.

---

## Dashboard Validation

Before deploying dashboards, validate:

1. **JSON Syntax**: Use `jq` to validate JSON
   ```bash
   jq empty infra/grafana/dashboards/gc-overview.json
   ```

2. **Datasource References**: Ensure datasource UIDs match environment
   ```bash
   grep -r "prometheus" infra/grafana/dashboards/
   ```

3. **Metric Existence**: Verify all metrics exist in codebase
   ```bash
   # Cross-reference dashboard queries against metrics.rs files
   grep "gc_http_requests_total" crates/gc-service/src/observability/metrics.rs
   ```

4. **Cardinality**: Check for unbounded labels
   ```bash
   # Search for user_id, meeting_id, etc. in dashboard queries
   grep -E "(user_id|meeting_id|participant_id)" infra/grafana/dashboards/*.json
   ```

---

## Requesting New Dashboards

To request a new dashboard:

1. Create Jira ticket with label `dashboard-request`
2. Specify:
   - Service or function to monitor
   - Key metrics to display
   - SLO thresholds (if applicable)
   - Target users (SRE, developers, leadership)
3. Tag **Observability Specialist** for review

---

## Dashboard Ownership

| Dashboard | Owner | Reviewer |
|-----------|-------|----------|
| GC Overview | Observability | GC Team |
| GC SLOs | Observability | Operations |
| AC Overview | Observability | AC Team |
| MC Overview | Observability | MC Team |
| MC Media Path | Observability | MC Team |
| MH Overview | Observability | MH Team |
| MH Media Path | MH Team (ADR-0031) | Observability |
| Client SDK Media Path | Client Team (ADR-0031) | Observability |

**Update Frequency**: Review quarterly or after major service changes.

---

**Maintained By**: Observability Specialist
**Related Documents**:
- [ADR-0011: Observability Framework](../decisions/adr-0011-observability-framework.md)
- [Metrics Catalog](./metrics/)
- [Alert Catalog](./alerts.md)
