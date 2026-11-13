# Telemetry contract: MH replacement and migration (observability, final)

Governing rules: ADR-0036 §11; `docs/observability/label-taxonomy.md` R1-R4. Every series below is
identity-free. No meeting, participant, connection, handler_id, handler_ref, handler_set_version or
generation value appears as a label, and ids go in logs only. Every `mc_media_*` / `mh_media_*` /
`dt_client_media_*` series carries `key_custody="operator"` and falls under the R4 hygiene kernel by
prefix. **`handler_id` is never a metric label on MC or GC series.** Per-incarnation ids combined with
replacement would grow the series set without bound over MC uptime. Per-handler views come from the
MH pod's own series, which are per-handler by construction. `internal.proto:801-802` is narrowed to
match.

## Shared vocabulary (single source of truth)

- `trigger`: projected (like `KindLabels::from_proto`) from the one `internal.proto` enum,
  values `heartbeat_timeout | apply_lag | planned_transfer`, plus `meeting_ended` only where
  EndMeeting/RELEASED is counted. It names the **signal**, not the diagnosis, so a future
  `client_connectivity` trigger is a new value. Used identically by MC, GC and MH. No service keeps
  its own list.
- Counting units (round 4, R4-1 per-meeting heartbeat): **detection, declarations and migrations are per (meeting, handler)**, observed by the MC meeting actor that owns the meeting. The only handler-level count is `mc_media_handler_first_declarations_total`.

## MC (`mc_media_handler_*`)

| Series | Type | Labels | Meaning |
|---|---|---|---|
| `mc_media_handler_declarations_total` | counter | `trigger` | A (meeting, handler) declared dead/impaired, or removed for a planned transfer. One MH death yields one per affected meeting. |
| `mc_media_handler_first_declarations_total` | counter | `trigger` | Once per HANDLER per MC, however many meetings declare it: an MC-wide bounded set of recently declared handler_ids, touched only at declaration and pruned when the handler leaves every meeting's set. Feeds the MC-own-path signature and the crash-rate warning. |
| `mc_media_handler_suspicions_total` | counter | `outcome=declared\|cleared` | Lagging → Impaired vs Lagging → Healthy. Near-miss rate, the hysteresis/N-tuning signal. |
| `mc_media_handlers_in_use` | gauge (recompute-and-set) | — | Distinct handlers across this MC's meetings. Denominator for the MC-own-path signature. |
| `mc_media_handler_loss_clients_total` | counter | `reported=before_declaration\|not_reported` | **Required metric, count.** Observed once per declaration with `trigger≠planned_transfer`. The sum ratio gives the fleet client-weighted percent, summed over meetings. |
| `mc_media_handler_loss_prior_report_ratio` | histogram, buckets 0, .1, .25, .5, .75, .9, 1 | — | **Required metric, percent** (base units 0..1). Weighted per (meeting, handler) declaration, and the policy evidence for promoting client connectivity to a trigger. |
| `mc_media_handler_loss_client_lead_seconds` | histogram | — | First client loss report → declaration. Observed only when ≥1 client reported. |
| `mc_media_handler_client_loss_disagreements_total` | counter | — | ≥ a configured fraction (with a configured minimum count, both published) of a handler's believed-connected clients report loss while its heartbeats stay healthy for ≥ N. This is the pure health function's client second-opinion output: the false-negative half of the evidence, and the forged-heartbeat (S1b) detector. The declaration histogram cannot see this case, because a forged heartbeat suppresses the declaration. |
| `mc_media_handler_detection_seconds` | histogram | `trigger` | Last accepted heartbeat (or drain notice) → declaration. |
| `mc_media_handler_migration_duration_seconds` | histogram | `trigger`, `phase=replacement\|registration\|reconnect` | Declaration → grant → RegisterMeeting(new) ack → routable participants connected. |
| `mc_media_handler_migrations_total` | counter | `trigger`, `outcome=completed\|no_capacity\|gc_unavailable\|admission_refused\|rate_limited\|timed_out\|abandoned` | One outcome per ACQUISITION ATTEMPT (one `AcquireMediaHandler` call, or one owed replacement refused at the bound); a retrying migration contributes one failure outcome per failed attempt and at most one `completed`, so per-migration completion is `completed / declarations_total{trigger!="planned_transfer"}` (clarified at story 4 MC-C Gate 1, observability ruling; supersedes "terminal per meeting migration"). `rate_limited` = one owed replacement refused at the per-meeting attempt bound (there is no per-MC limiter); a minimum-interval deferral is NOT counted (logged `reason=min_interval`). GC does not rate-limit, so this is the only place a flap or forged loop shows. `trigger="planned_transfer"` (story 4 MC-F, observability + lead ruling): failed attempts count per attempt as above; a planned grant reaching Active records nothing; each planned transfer records ONE terminal (`completed` = everyone on the target at the old handler's removal, `timed_out` = cut over at drain_remaining − margin with stragglers or shed, `abandoned` = meeting ended or handler declared first). Exact invariant: `completed + timed_out == declarations_total{trigger="planned_transfer"}`; `abandoned` also carries attempt-level discards, so it is never a transfer count. |
| `mc_media_meetings_without_handlers` | gauge (recompute-and-set) | — | Meetings with zero live handlers ("awaiting handler"). |
| `mc_media_handler_heartbeat_interval_seconds` | histogram, buckets around N/3, N/2, N | — | Inter-arrival per (meeting, handler). |
| `mc_media_handler_applied_sequence_lag` | histogram | — | Sent − applied `policy_generation` per entry per heartbeat. **Replaces the `mc_media_generation_divergence` gauge.** |
| `mc_media_handler_heartbeats_total` | counter | `outcome=owned\|released_removed\|released_ended\|released_unassigned\|owner_mismatch` | Verdicts MC returned (L4-6, L4-7). `released_removed` = RELEASED(removal trigger): a zombie or lost EndMeeting being fenced. `released_ended` = no meeting actor, RELEASED(MEETING_ENDED): a lost EndMeeting retried, so this is the lost-EndMeeting rate. `released_unassigned` = live meeting actor that never had this handler (misroute), RELEASED(UNSPECIFIED), expected zero. `owner_mismatch` = the heartbeat's mc_id is not this MC, answered with an error; expected zero. There is no UNKNOWN verdict (R4-4). |
| `mc_media_handler_connected_set_corrections_total` | counter | — (remove-only reconcile) | Each increment is a lost Disconnected that the heartbeat healed. This gives O-11 its first visibility. |
| `mc_media_handler_set_pushes_total` | counter | `reason=join\|change\|token_refresh\|failed_report` | `MediaHandlerSet` messages sent (R4-3 / L4-3). MC signs media_dial locally, with no AC call. `failed_report` = the client FAILED-report fast re-push. Drops reuse `mc_participant_outbound_messages_dropped_total` with a new `payload_kind` for the handler set. |
| `mc_media_dial_token_ttl_seconds` | config gauge | — | `MC_MEDIA_DIAL_TOKEN_TTL_SECONDS` (refresh at TTL/3, jittered, derived). |
| `mc_media_dial_token_refresh_interval_seconds` | config gauge | — | The derived TTL/3, published so a dashboard and env-tests read the live value. |
| `mc_media_handler_heartbeat_deadline_seconds` | config gauge | — | N, the single MC-owned value. Env-tests read it via `gauge_by_live_pod`, and the dashboard shows it beside MH's QUIC idle timeout. |

Required-metric semantics:
- One snapshot is taken at the Dead/Impaired transition.
- Denominator: MC's connected set for that handler at the transition (MH-reported, heartbeat-reconciled).
- Numerator: clients whose `MhConnectionStatus` (DISCONNECTED/FAILED), joined on `handler_ref` + `handler_set_version` by exact match, was received by MC after that client's last connected state for that ref and before the transition. MC receipt time only; client `observed_at` is never used.
- Reports about a superseded set, or arriving after removal, are ignored.
- Not observed for `planned_transfer` (an ADR-0032 unobserved assertion, so the check cannot pass vacuously).
- **Interpretability condition, stated in the ADR next to the metric:** the metric means something only when client loss-detection time (MH QUIC `max_idle_timeout`, config) is roughly MC's N. Both values are published. Otherwise a low percentage reads as "clients don't notice" when it means "clients were configured to notice later".
- The input is client-asserted and diagnostic only. Promoting it to a trigger requires security review (quorum or per-client weighting).

## MH

| Series | Type | Labels | Meaning |
|---|---|---|---|
| `mh_mc_heartbeats_total` | counter | `outcome=success\|error\|timeout\|skipped_in_flight\|dropped_queue_full` | Per meeting since R4-1 (volume rises, cardinality unchanged). Matches the `mh_gc_heartbeats_total` convention. Log state transitions only, never per heartbeat. |
| `mh_mc_heartbeat_latency_seconds` | histogram | — | |
| `mh_media_handler_releases_total` | counter | `trigger` (removal values only), `clients=present\|none` | Removals from a removal-reason EndMeeting or a RELEASED verdict, which carries its ReleaseCause (L4-6). That is what makes the zombie case countable, because there the EndMeeting is the message that got lost. **`trigger=heartbeat_timeout, clients=present` is the false-declaration ground truth**, which only MH can see. `apply_lag, present` is a correct removal of a live-but-slow MH. `planned_transfer, present` is expected. `meeting_ended` stays on `mh_media_meeting_teardowns_total{outcome="released"}`, so each event has one home. |
| (owner-silence release, R-11) | — | `mh_media_handler_releases_total{trigger="owner_silence", clients}` | The one MH-local trigger value outside the ReleaseCause projection, documented in the code enum and the catalog: MH ends a meeting after `MH_MEETING_OWNER_SILENCE_TIMEOUT_SECONDS` (default 900) with no communication from its owning MC. `clients=none` is routine 15 min after every MC rollout (dashboard). `clients=present` is a WARNING. Config gauge `mh_media_meeting_owner_silence_timeout_seconds`. One INFO log per release (meeting_id, owner mc_id, silence duration). |
| `mh_media_meetings_without_clients` | gauge (actor tick) | — | Registered meetings with zero active connections (zombie/leak arm). |
| `mh_drain_duration_seconds` | histogram | — | |
| `mh_drain_meetings_remaining` | gauge | — | |
| `mh_drain_meetings_shed_total` | counter | — | Meetings still held at the drain deadline. |
| session closes | counter | `code` from the signaling.proto close-code enum (WRONG_HANDLER, BINDING_CONFLICT, MC_UNAVAILABLE, HANDLER_RELEASED, DECLINED) | Bounded by the enum. Covers media_dial `aud` mismatch declines (ops). The exact name is MH's call. |
| QUIC `max_idle_timeout` | config gauge | — | Published for the interpretability condition. |
| `mh_jwt_validations_total` (existing) | counter | `result`, `token_type`, `failure_reason` | Client path: `token_type="meeting"` becomes `"media_dial"`, and the single client-path `validation_failed` (`webtransport/connection.rs:323`) splits into `expired\|signature_invalid\|aud_mismatch\|no_meeting_key\|malformed\|wrong_type`. `no_meeting_key` replaces `mh_register_meeting_timeouts_total` (L4-2 deletes the pending-before-registration path). `wrong_type` = a non-media_dial token, the cross-token-confusion signal. These reasons are metric-only: the client always sees a generic DECLINED. |
| media_dial exp skew | config gauge | — | L4-5: the hardcoded 300 s in `crates/mh-service/src/main.rs` becomes config. |
| `mh_media_session_starts_total` (existing) | counter | gains a decline outcome for the L4-8 per-(meeting, sub) pending cap | Expected near-zero; a sustained rate means a client redial storm. |

## GC

- `gc_mh_replacement_requests_total{trigger=heartbeat_timeout|apply_lag|planned_transfer|invalid, outcome=granted|no_capacity|not_owner|invalid_request|error, gc_view=healthy|degraded|unhealthy|draining|pending|stale|unregistered|unevaluated}`. The disagreement (partition) signal is a query: `trigger="heartbeat_timeout", gc_view="healthy"`. There is no second counter. (GC-3 refinements, observability-confirmed: `pending` matches the registry's CHECK set and `gc_mh_available`; `unevaluated` = the request ended before the replaced row was read (`invalid_request`, `not_owner`, or an error before the lookup), so `unregistered` keeps meaning "no row"; `trigger="invalid"` only with `invalid_request`, for a request whose own trigger is not a replacement cause, so no raw value becomes a label. Emitted set: 73 series, enumerated in the catalog.)
- `gc_mh_replacement_duration_seconds{outcome}`.
- `gc_mh_registrations_total{outcome=ok|superseded|address_conflict|endpoint_changed|invalid_endpoint, match=both|one_sided}` (new; GC has no MH-registration counter today). `match` is carried only on `superseded` (security requires it). Zero-initialise all six series. Rules are in the RegisterMH field comments: allowlist, then endpoints fixed for an incarnation's life, then no two LIVE incarnations share an endpoint, with sequential reuse permitted. A single `superseded` is a routine in-place crash restart.
- `gc_mh_load_reports_total{outcome=accepted|unknown_handler}` (new). `unknown_handler` = GC's existing NOT_FOUND "Handler not registered" answer in `send_load_report` (`crates/gc-service/src/grpc/mh_service.rs:252`). A dead process never reports, so any increase means a process GC has no row for is ALIVE: a live duplicate on one address (hostPort lock defeated, e.g. a force-delete), a lost registry, or the evicted side of a ping-pong. This is the discriminator, with no new state. Zero-initialised for every outcome.
- `gc_mh_available{region, state=healthy|degraded|draining|pending|unhealthy}` gauge (recompute-and-set from the registry). Counts rows within the staleness bound, except `unhealthy`. No handler_id. `region` is bounded and is not an identity. Input to the fleet-level MHDown.
- `gc_mh_load_report_interval_seconds` config gauge (GC is the SSoT, pushed to MH at registration), and `gc_mh_staleness_threshold_seconds` config gauge, **derived** as k × interval: the one staleness value, replacing the hardcoded 30 s and the borrowed 60 s.

## Client (R3 set + `media_kind`/`content_kind` only, labels via `mediaMetricLabels`)

R-42 (user ruling 2026-10-31: extend the existing first-connection series, don't duplicate them). The two existing series were grandfathered ADR-0028 join-flow members carrying `meeting_id_hash` and were deliberately **not exported** (`docs/observability/metrics/client.md`, `infra/services/otel-collector/collector.yaml`). They are extended **in place** (same names, never duplicated) and **leave the frozen grandfathered roster**: the roster shrinks, it is never extended. `meeting_id_hash`, `status` and `mh_index_bucket` are dropped. The labels move to `mediaMetricLabels` (`client_version`, `org_id`, `key_custody`), and both series are exported (collector allowlist, GC telemetry filter, client-metrics-export guard). `dt_client_mh_` is added to the label-taxonomy media-path-prefixes block (together with the hygiene kernel's const copy) so R4 enforcement covers the family. The `MediaTransport.ts` spread comment and the media-transport test pin flip. Cost: the browser-console log↔metric correlation by meeting is gone on these series.

- `dt_client_mh_connection_total{phase=join|added|redial, outcome=connected|wrong_handler|binding_conflict|mc_unavailable|handler_released|declined|transport_failure|timeout|aborted_by_removal|contract_fault}`: one count per dial, every dial. Close-code outcomes come from `MhCloseCode` by a total match; a zero or unknown code is `contract_fault`.
- `dt_client_time_to_first_mh_connected_ms`: join → first MH connected (meaning unchanged, reshaped as above).
- `dt_client_mh_dial_duration_ms{phase, outcome=connected|failed}`: dial start → connected or failed, per dial.
- `dt_client_mh_disconnects_total{cause=transport_loss|directed_removal|handler_released|wrong_handler|binding_conflict|mc_unavailable|declined|contract_fault}`: one count per end of an established session.
  - **Lost-handler count = `dt_client_mh_disconnects_total{cause=~"transport_loss|handler_released"}`.** A `HANDLER_RELEASED` disconnect can only be observed while the ref is still in the set. After a directed removal the client has already closed, and that close counts as `directed_removal`, so the server's later close is not a second event.
  - The previously planned `dt_client_media_handler_lost_total` is **deleted**, so one event has one home.
- `dt_client_mh_reconnect_duration_ms{outcome=reconnected|removed|abandoned}`: per-handler disruption timer, from loss of an in-set handler to a successful redial of the same ref, removal from the set, or abandonment (session end or the published cap).
- `dt_client_media_resume_ms{media_kind, content_kind, outcome=resumed|reassigned_away|abandoned}`: per slot, only for slots Active at episode start. Start = transport loss or a set change removing the handler. End = first decoded video / played audio frame via another handler. The abandonment cap is a published clientConfig value. Solo or receive-only clients start no episode. This is the truth for the media-dark window.
- Replay drop gains a bounded reason token for a duplicate arriving on a different transport (make-before-break overlap). Any future replay alert excludes it.
- Client task C1 lands the `dt_client_mh_*` series and their export path. C2 lands `resume_ms` and the replay token.

## Alerts (rule files per ADR-0031; each needs a runbook that resolves)

**PAGE**
- `MCMediaMeetingsAwaitingHandler`: `max(mc_media_meetings_without_handlers) > 0` sustained past a config-derived bound (the replacement retry budget, published as a gauge, not a literal). The meeting is dark with no self-repair.
- `MCMediaGenerationDivergence`: **retained**. Responses keep `applied_generation` and `applied_source_streams`, so its arms keep their home. Rewrite its comments and runbook ladder to remove the re-assert premise. Drop `handler_id_mismatch` from the expression and from its three "one decision" sites.

- `MHDown` **re-scoped** (media-handler rule): the per-pod `up{job="mh-service"} == 0` page goes, because a single MH death is now routine and healed by replacement, and user impact pages via `MCMediaMeetingsAwaitingHandler`. It becomes fleet-level per region: `gc_mh_available{state="healthy"} == 0` (no replacement capacity at all). Its description ("no running pods") becomes true, and runbook scenario 1 is re-scoped to match.

- `GCMhRegistrationPingPong` (security's ping-pong signature): `sum(increase(gc_mh_registrations_total{outcome="superseded"}[10m])) >= 20 and sum(increase(gc_mh_load_reports_total{outcome="unknown_handler"}[10m])) >= 3`, `for: 10m`. Derivation (in the rule's ADR-0031 block):
  - Ping-pong: two live processes alternately evict each other at the MH→GC load-report cadence. GC owns that cadence and pushes it in `RegisterMHResponse.load_report_interval_ms`; it becomes config `GC_MH_LOAD_REPORT_INTERVAL_MS` (default 10 s), published as a gauge. MH's `gc_client.rs:40` const is only the pre-registration fallback. That is about 2 supersedes per 10 s, roughly 120 per 10m, each evictee also producing an `unknown_handler` report.
  - Routine crash restarts: a dead process never reports, so `unknown_handler` stays at 0 however high `superseded` climbs. That makes the conjunction independent of fleet size. Kubelet CrashLoopBackOff caps a looping pod at about 1 supersede per 5 m after a short burst.
  - The `superseded` floor of 20 only rejects a one-off duplicate, which the `unknown_handler` warning already reports.
  - The derivation cites GC's load-report interval gauge. Re-derive if it changes. The MH staleness bound is derived as k × interval (k config, default 3), not an independent number.

**WARNING**
- `mc_media_handler_migrations_total{outcome=~"no_capacity|gc_unavailable|admission_refused|timed_out"}` while a survivor exists (active/active N-1 is degraded, not dark).
- `gc_mh_replacement_requests_total{outcome="no_capacity"}`: capacity headroom gone.
- `gc_mh_load_reports_total{outcome="unknown_handler"}` increase > 0: a live MH process GC has no row for (a live duplicate on one address, or a lost registry). Fail loud.
- `gc_mh_registrations_total{outcome=~"endpoint_changed|invalid_endpoint"}` increase > 0: an MH config or allowlist defect. The handler never becomes capacity.
- `gc_mh_registrations_total{outcome="address_conflict"}` still increasing after `for:` = MH staleness bound + one MH registration retry: past the bound, a stale conflicting row would be superseded (case 2b), so persistence means two configured holders of one address. Severity warning: capacity was not added, but no meeting is dark. The `for:` literal carries an `ANCHOR (DRY)` comment naming `gc_mh_staleness_threshold_seconds`, and an env-test compares the rule's `for:` against the live gauge, because a `for:` cannot read a gauge.
- False declarations: `mh_media_handler_releases_total{trigger="heartbeat_timeout",clients="present"}` sustained.
- MC-own-path signature: by instance, `increase(mc_media_handler_first_declarations_total{trigger="heartbeat_timeout"}[w]) >= 2` and that over `mc_media_handlers_in_use` above a fraction (`w` and the fraction in the rule). Declaration volume counts meetings, and gc_view at request time lags MC by design, so neither can drive it. One MH death is one handler and cannot fire it. Built on MC's `HandlerCensus` (MutedSourceCensus pattern: recompute-and-set, std Mutex, touched only at membership change and declaration). Runbook (ops): the same signature on several MCs at once = real multi-MH loss (node/AZ); only this MC = MC's own path, so leave the handlers alone.
- `mc_media_handler_client_loss_disagreements_total > 0` (forged heartbeat vs client network path).
- `mc_media_handler_migrations_total{outcome="rate_limited"}` sustained (flap or forged loop).
- `mc_media_handler_connected_set_corrections_total` sustained (lossy notification path).
- `mh_media_handler_releases_total{trigger="owner_silence", clients="present"}` increase > 0: people stayed attached to a meeting no MC owned for the whole silence window.
- Drain shed on rollout: reads **MC** `mc_media_handler_migrations_total{trigger="planned_transfer", outcome="timed_out"}`, never `mh_drain_meetings_shed_total` (best-effort; MH also writes the shed count in its final exit log line).
- MH crash rate (per-incarnation "crashlooping" bug that replacement hides): `rate(mc_media_handler_first_declarations_total{trigger="heartbeat_timeout"}[1h])` above a rule-owned threshold (per handler, not per meeting). This is the **primary** crash warning in both branches below, because it measures user-felt handler loss rather than kubelet behaviour (MH concurs). **`MHPodRestartingFrequently` (mh-alerts.yaml:749, `kube_pod_container_status_restarts_total`) becomes dead by construction if a crashed MH is replaced by a NEW pod rather than restarted in place.** An in-place container restart would re-use the old incarnation's address, which the round-3 ruling forbids. **Resolved: infra keeps restartPolicy=Always (in-place restart on the same slot, sequential reuse; GC supersedes the dead row with match=both), so `MHPodRestartingFrequently` stays live and both apply.**
- `MCRegisterMeetingFailureRate`: `mc_register_meeting_total{status="error"}` ratio > 0.10 for 5m (the Scenario 12 breadcrumb). It is an error-ratio rule like `MCHighJoinFailureRate`, not burn-rate, so it is **unblocked** and lands in story 4.
- `mh_jwt_validations_total{token_type="media_dial", failure_reason="expired"}` sustained: MC's TTL/3 refresh push is broken; this is the only signal for it.
- `mh_jwt_validations_total{token_type="media_dial", failure_reason="no_meeting_key"}` sustained: a register-before-token ordering bug (L4-2).
- `mh_jwt_validations_total{token_type="media_dial", failure_reason="wrong_type"}` increase > 0: token-type confusion.
- `mh_jwt_validations_total{token_type="media_dial", failure_reason="aud_mismatch"}` sustained: the WRONG_HANDLER rate (runbook-alert-changes.md §E).
- `mh_media_meetings_without_clients > 0` sustained past the owner-silence timeout plus margin: owner-silence release should have cleared them (`for:` with an ANCHOR to `mh_media_meeting_owner_silence_timeout_seconds`; runbook-alert-changes.md §E).
- `mc_media_handler_heartbeats_total{outcome=~"released_unassigned|owner_mismatch"}` increase > 0: contract or routing defect.

**DASHBOARD ONLY**
- The required client-before-MC metric (research evidence, not a fault), lead time, suspicions, heartbeat interval, applied-sequence lag, detection and migration phase durations, `dt_client_media_resume_ms`, GC `gc_view` breakdown (described as "GC's view at request time, which lags MC by design": a real death is usually requested while GC still says healthy, so this is not a gray-failure signal), the R4-5 evidence panel `gc_mh_replacement_requests_total{trigger=~"heartbeat_timeout|apply_lag"}`, `mh_media_meetings_without_clients` (alerted only when sustained past the owner-silence timeout), and N beside the QUIC idle timeout.
- New panels belong on `mc-media.json` (MC handler rows on `mc-media-handlers.json` since story 4 MC-F), `mh-media.json`, `client-media.json` and `gc-overview.json`, per ADR-0031 ownership.

## SLO (`docs/observability/slos.md`)

- Replace the open item "No SLO for the MC→MH control plane's applied-generation echo" with "MH replacement: migration completes within deadline; media-dark window (MC detection + migration as the server bound, `dt_client_media_resume_ms` as the truth)". The SLI is defined in story 4. The target stays **OPEN**: no invented number, observed and not gated (§10).
- Delete the open-items row "`MCRegisterMeetingFailureRate` re-assert-failure alert" (it lands in story 4).

## Retirements (each in the same commit as its catalog entry, panels, alerts and runbook refs, so `validate-application-metrics` stays green and commits stay self-contained)

- Round 4 deletions: `mh_register_meeting_timeouts_total` and its panels and runbook refs (L4-2); the `unknown` heartbeat verdict outcomes on MC and MH (R4-4); the AC-mint counter and the `token_unavailable` migration outcome (R4-3).

- `mc_media_generation_divergence` gauge: deleted. It was pod-level last-write-wins, documented as unfit for detection (`crates/mc-service/src/observability/metrics.rs:461-495`), and is replaced by `mc_media_handler_applied_sequence_lag`.
- `mc_media_policy_pushes_total{outcome="handler_id_mismatch"}` value and its three expression sites.
- `mc_media_handler_set_divergence_total` (non-first joins stop reading Redis).
- `mc_mh_notifications_without_connection_id_total` and the empty-`connection_id` degraded-mode counter (legacy mode retired).
- `process_start_epoch_ms` restart detection.
- ADR-0036 §11 runbook list: drop "restart media-dark", "sustained re-assert failure" and the re-assert framing of "generation divergence". Add "handler declared dead / migration", "awaiting handler (replacement capacity)", "false declarations / MC↔MH partition", "impaired handler (apply lag)", "drain shed", and "client-loss disagreement (forged heartbeat vs client path)". Operations writes these.
- §8 "Re-assert failures are counted and paged" is superseded by the awaiting-handler page plus the migration-failure warnings.

## TODO.md dispositions

- `:1011` Missing alert, MC RegisterMeeting coordination failures: **close in story 4** (`MCRegisterMeetingFailureRate` + awaiting-handler page).
- `:1470` O-11, lost Disconnected on a live MH: **closed** by the remove-only connected-set reconcile, which is visible as `mc_media_handler_connected_set_corrections_total`.
- `:1539` generation reset before cadence, and `:1681` EndMeeting fence: **moot** (no timer re-push; RELEASED verdicts fence late registrations).
- `:1467` item 2, connectivity repair after an MH restart: **closed by this story** (the MH half).

## Round-3 provisioning requirements (per-incarnation, on-demand MH)

- Scrape discovery is automatic only if every MH pod keeps label `app=mh-service` and metrics port 8083 (`infra/kubernetes/observability/prometheus.yml:123-136`). **Confirmed by infrastructure:** both are properties of the single MH template, guarded by dt-guard. An unscraped MH reads as "no traffic".
- Last-gasp loss: counters an MH increments in its final seconds before exit may never be scraped. **Authoritative drain outcomes live on MC** (`mc_media_handler_migrations_total{trigger="planned_transfer"}`). MH drain counters are best-effort, and the "shed on rollout" warning reads MC. MH does not linger for a scrape, because that would couple its lifetime to Prometheus config.
- Env-tests discover MH pods by label. The hardcoded `MH_INSTANCES` / `PROFILE_READERS` (`crates/env-tests/src/fixtures/kube.rs:75,82`) go.
- The advertised address is never a label or gauge value, and neither is the pod label `dark-tower.io/mh-slot` or `mh-revision` (instance identifiers; per-pod views use the `pod`/`instance` target labels). MH Deployments become `mh-s<k>` on demand, so dashboards, alerts and env-test fixtures select by `app=mh-service`, never by pod name.

## R4-5 evidence: GC replacement reports (L4-9)

- No new metric. `gc_mh_replacement_requests_total{trigger, outcome, gc_view}` already counts every report.
- The per-handler corroboration source is the INFO event `gc.mh.replacement_report` {handler_id, webtransport_endpoint, controller_id, caller_sub, trigger, gc_view, outcome}. (GC-3: `caller_sub` is the validated service-token subject, omitted without claims; today it is the shared MC client id, so it never distinguishes MC instances. Evidence rule: only events that passed the ownership check count, i.e. `outcome` = `granted` | `no_capacity`, or `error` with `gc_view` != `unevaluated`; `not_owner` / `invalid_request` events can be emitted by any MC-token holder naming any handler.) Ids are allowed in logs. The distinct-MC count per handler within W is a log query, exact across GC replicas, with W chosen at query time. **The gray-failure discriminator** is a handler declared dead by ≥2 distinct MCs within W while it is STILL load-reporting healthy to GC after the staleness bound: alive to GC, dead to its MCs. It is never `gc_view` at request time.
- There is no in-memory window: a per-replica window would undercount exactly when corroboration is real.
- Caveat: `controller_id` is self-asserted, so a quorum policy needs authenticated MC identity first.

## Verification

- MC integration test: inject k of n loss reports, trip Dead through the pure clock-injected health function, and assert `MetricAssertion` deltas on the loss counter, ratio histogram and lead histogram.
- One recording site per `trigger`/`outcome` value, each driven through the pure function, so there are no orphan sites.
- Planned-transfer negative uses the ADR-0032 unobserved assertion.
- Env-test (SIGKILL MH pod): assert rises only, never absolute shared values. Covers `declarations_total{trigger="heartbeat_timeout"}`, `migrations_total{outcome="completed"}`, and observations of the loss metric and the detection/migration histograms. Durations are observed, never gated (§10). Waits are derived from the `mc_media_handler_heartbeat_deadline_seconds` gauge.
- Env-test (SIGTERM drain): `trigger="planned_transfer"`, `mh_media_handler_releases_total{trigger="planned_transfer"}` rises, and no loss-metric observation.
- `33_alert_rules_loaded.rs` covers every new rule.
