# Devloop Output: MC-E — Required loss metric, client-loss disagreement, HandlerCensus

**Date**: 2026-11-13
**Task**: Story 4 task MC-E (run-story task #25) — R-33/R-34 loss metric at Dead/Impaired declarations, client-loss disagreement counter, HandlerCensus (first declarations, handlers in use). Full task text: `/home/dev/.cache/devloop/story-runs/story-runner/2026-10-31-survive-handler-loss/task-25.prompt`.
**Specialist**: meeting-controller
**Mode**: Agent Teams (v2) — full panel; Gate-1 ESCALATED to full (manifest tier=light, but the diff is instrumentation code — a `--light` exclusion surface)
**Branch**: `feature/mh-failure-transfer`
**Duration**: ~3h (Gate 2 incl. Layer 7 ~42m)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `e709e90582b95e1c28505389cb262692fa3ead61` |
| Branch | `feature/mh-failure-transfer` |
| Lead Model | `claude-opus-5-5` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` |
| Implementing Specialist | `meeting-controller` |
| Tier | `full — escalated from manifest tier=light (reason: MC-internal instrumentation over existing MC-D/MC-B2 state) because the plan is instrumentation code (metrics::), a --light exclusion surface; Gate-1 round runs` |
| Iteration | `1` |
| Security | `security` |
| Test | `test` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Protocol | `protocol` (pulled in at review for comment-only GSA edit to proto/dark_tower/internal/v1/internal.proto, from SEC-1) |
| Semantic Guard | `not spawned — no check surface (no credential/secret types, no Debug over payloads/keys)` |

---

## Task Overview

### Objective
At every dead/impaired declaration, operators can read how many clients MC believed connected to that handler had already reported losing it; MC exposes a handler-level own-path signal (disagreement counter, HandlerCensus).

### Scope
- **Service(s)**: mc-service (+ env-tests, dashboard, docs)
- **Schema**: No
- **Cross-cutting**: No

### Debate Decision
NOT NEEDED — governed by ADR-0039 §8 and `docs/debates/2026-10-28-mh-replacement-and-migration/telemetry-contract.md` §MC.

---

## Cross-Boundary Classification

(Implementer fills in during planning.)

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `crates/mc-service/src/media_routing/handler_health.rs` | Mine | — |
| `crates/mc-service/src/media_dial/status.rs` | Mine | — |
| `crates/mc-service/src/actors/meeting_media.rs` | Mine | — |
| `crates/mc-service/src/actors/meeting.rs` | Mine | — |
| `crates/mc-service/src/actors/meeting/health.rs` | Mine | — |
| `crates/mc-service/src/actors/meeting/planned_transfer.rs` | Mine | — |
| `crates/mc-service/src/actors/meeting/replacement.rs` | Mine | — |
| `crates/mc-service/src/actors/controller.rs` | Mine | — |
| `crates/mc-service/src/config.rs` | Mine | — |
| `crates/mc-service/src/main.rs` | Mine | — |
| `crates/mc-service/src/webtransport/server.rs` | Mine | — |
| `crates/mc-service/src/media_routing/mod.rs` | Mine | — |
| `crates/mc-service/src/observability/metrics.rs` | Mine | — |
| `crates/mc-service/tests/handler_loss_metric_integration.rs` | Mine | — |
| `crates/mc-service/tests/planned_transfer_integration.rs` | Mine | — |
| `crates/mc-service/tests/common/mod.rs` | Mine | — |
| `crates/mc-service/tests/common/replacement_rig.rs` | Mine | — |
| `crates/mc-service/tests/common/accept_loop_rig.rs` | Mine | — |
| `crates/mc-service/tests/gc_integration.rs` | Mechanical | — |
| `crates/mc-service/tests/otel_grpc_outbound_integration.rs` | Mechanical | — |
| `crates/mc-service/src/grpc/gc_client.rs` | Mechanical | — |
| `infra/services/mc-service/config.env` | Mechanical | — |
| `infra/services/mc-service/mc-0-deployment.yaml` | Mechanical | — |
| `infra/services/mc-service/mc-1-deployment.yaml` | Mechanical | — |
| `infra/grafana/dashboards/mc-media-handlers.json` | Mechanical | — |
| `docs/observability/metrics/mc-service.md` | Mechanical | — |
| `docs/runbooks/mc-deployment.md` | Mechanical | — |
| `crates/env-tests/tests/40_resilience.rs` | Mine | — |
| `crates/env-tests/tests/02_mc_deployment_config.rs` | Mechanical | — |
| `docs/specialist-knowledge/meeting-controller/INDEX.md` | Mine | — |
| `docs/observability/dashboards.md` (panel inventory) | Mechanical | — |
| `proto/dark_tower/internal/v1/internal.proto` (comment-only, no wire change; Gate-3 SEC-1) | Not mine, Domain-judgment | protocol |
| `docs/decisions/adr-0039-mh-replacement-and-migration.md` (§8 + residual-risk S1b text; Gate-3 SEC-1) | Not mine, Domain-judgment | security |
| `crates/common/src/observability/testing.rs` (histogram drain scoped to the querying histogram; Gate-3 test finding) | Not mine, Minor-judgment | — (crates/common; reviewed by DRY + code-reviewer) |
| `crates/gc-service/tests/meeting_join_metrics_integration.rs` (comment naming the corrected doc section) | Mechanical | — |
| `crates/mc-service/tests/handler_replacement_integration.rs` | Mine | — |
| `crates/mc-service/tests/handler_detection_integration.rs` | Mine | — |
| `docs/TODO.md` (SEC-1 spin-out entry) | Mine | — |
| `crates/ac-service/src/observability/metrics.rs` (test comments made true after the drain fix) | Mechanical | — |
| `crates/mh-service/tests/media_metrics_integration.rs` (test comment made true after the drain fix) | Mechanical | — |
| `crates/gc-service/tests/mh_replacement_tests.rs` (test comment made true after the drain fix) | Mechanical | — |
| `crates/mc-service/src/media_admission/rotation.rs` (test comment made true after the drain fix) | Mine | — |
| `crates/mc-service/tests/kek_rotation_integration.rs` (module doc made true after the drain fix) | Mine | — |
| `docs/user-stories/2026-10-31-survive-handler-loss.md` (SEC-1 caveat in residual-risk line and scenario 28 plan) | Mechanical | — |
| `crates/mc-service/src/media_routing/liveness.rs` (N-derived lead buckets) | Mine | — |

---

## Planning

### Problem restated (mechanism)

Three things are missing: (1) at the instant a (meeting, handler) pair is declared Dead/Impaired, MC never compares the MH-reported connected set for that handler against the client-asserted loss store (MC-D's `ClientHandlerStatuses`); (2) `assess` takes a client-loss input hard-wired to 0, so the opposite case (clients say gone, heartbeats say fine) is invisible; (3) every handler series is per (meeting, handler), so "this MC lost N distinct handlers" cannot be read. No wider class surfaced: the only other declaration site (planned transfer) is excluded from (1) by contract and joins (3) through the same recorder.

### Design

**A. One client-loss snapshot, fed to `assess` (handler_health.rs).**
- `Observations.client_loss_reports: u32` becomes `client_loss: ClientLoss { connected: u32, reported: u32, first_report_at: Option<Instant> }` (Copy). `reported ⊆ connected` by construction.
- Built by ONE actor function `MeetingActor::client_loss(&HandlerId) -> ClientLoss`:
  - denominator = participants with ≥1 live MH-reported connection key on the handler (`ParticipantConnectivity::keys_on`, i.e. MH-reported, heartbeat-reconciled). A participant who reported loss but is not in that set is in neither count.
  - numerator = of those, participants whose LATEST `ClientHandlerStatuses` entry for `handlers.client_handler(id)` (the Active ref — attribution by ref, never URL) is DISCONNECTED or FAILED. Out-of-set / post-removal reports never enter the store (MC-D intake), a later CONNECTED supersedes, version is correlation-only.
  - `first_report_at` = min over counted participants of the entry's `loss_since`.
- `status.rs`: `LatestReport` gains `loss_since: Option<tokio::time::Instant>` = MC receipt instant (actor `Instant::now()` at intake) of the first report of the current loss run (DISCONNECTED→FAILED keeps it, CONNECTED clears it). Client time is never read (the proto carries none we use). `intake` takes a tokio `Instant` (converted for the limiter).
- Assessment call sites (`assess_after_heartbeat`, deadline arm) compute the snapshot per member and pass it to `MeetingMedia::assess_handler(id, now, client_loss)`; `assess_all` becomes an actor-side loop over ids (borrow split: statuses live on the actor, connectivity on `MeetingMedia`). The snapshot is stored in `HealthStep.observations` — the SAME snapshot `assess` judged is the one the loss metric records: "ONE snapshot at the transition".

**B. Required loss metric (health.rs `act_on_health`).** On a `None → Some(declaration)` transition whose trigger is `heartbeat_timeout|apply_lag` (planned transfer never enters `act_on_health`; it removes via `planned_transfer.rs`, so exclusion is structural, plus a trigger match for defence) and the removal succeeded:
- pure `LossObservation::from(ClientLoss, now) -> { reported, not_reported, ratio: Option<f64>, lead: Option<Duration> }` (table unit-tested);
- `mc_media_handler_loss_clients_total{reported=before_declaration}` += reported, `{reported=not_reported}` += connected − reported;
- `mc_media_handler_loss_prior_report_ratio` observed once = reported/connected; **connected = 0 → ratio NOT observed** (no 0/0; the counters add 0); documented in catalog;
- `mc_media_handler_loss_client_lead_seconds` = now − first_report_at, observed only when reported ≥ 1.
- Snapshot computed BEFORE `remove_handler_completely` (which purges connectivity and prunes refs).

**C. Client-loss disagreement (assess's second-opinion output).**
- `HealthConfig` gains `disagreement: DisagreementThreshold { fraction: f64, min_clients: u32 }` (from `MediaRoutingDeps`, built from Config).
- quorum(c) = `c.reported >= min_clients && f64(reported) >= fraction * f64(connected)` (connected ≥ 1 implied by min ≥ 1).
- `LiveHealth` gains `client_quorum_since: Option<Instant>` and the shape gains `client_disagreement: bool` (= quorum held ≥ N, via the ONE `expired(since, timeout, now)` boundary). `next_deadline` includes `expiry(since, timeout)` so the deadline arm fires at exactly N.
- Episode: starts at the first assessment that sees quorum while Live; COUNTED once on the `false → true` edge of `client_disagreement` (`DisagreementOutcome::of(prev, next)`, like `SuspicionOutcome`), logged once (transition). **Ends** when an assessment sees quorum lost (reports superseded by CONNECTED, MH reports the clients Disconnected so they leave the denominator, participants leave) or the pair leaves Live (declared/removed — health record dropped). Re-entry after an end is a new episode (+1). Further reports in the same episode add nothing.
- "Heartbeats stay healthy for ≥ N" = the pair stays Live (not Dead/Impaired) for the whole window; a Dead declaration at N ends it. Lagging/draining do not end it (heartbeats are arriving).
- Never acts: no removal, no set change, `declarations_total` unaffected. Granularity: quorum is noticed at assessment instants (every accepted heartbeat, ≈N/3, and deadline wakes); a client report does not itself trigger an assessment.
- `mc_media_handler_client_loss_disagreements_total` (no labels beyond key_custody).

**D. HandlerCensus (meeting_media.rs, MC-wide `std::sync::Mutex`, no await).** Not a `MeetingCensus<V>` (V must be Copy); a sibling struct, same doc'd recompute-and-set contract:
- `sets: HashMap<meeting_id, BTreeSet<HandlerId>>` (every member, Registering + Active), `declared: HashSet<HandlerId>`, `cap`.
- `set(meeting, ids)`: absolute write (empty forgets the meeting) + prune `declared` to ∪sets + set `mc_media_handlers_in_use` = |∪sets|. Written at actor start (initial set), after every membership change (grant add in replacement.rs, `remove_handler_for`, planned cutover — next to every `publish_handlerless`), and emptied in `hand_off_teardown`.
- `retain(live)`: the controller's `check_meeting_health` walk prunes dead meetings (a panicked actor never writes empty), prunes `declared`, re-sets the gauge — LOAD-BEARING, exactly like the other censuses.
- `declared(id, trigger)`: if `id ∉ declared` → `first_declarations_total{trigger}` +1, then insert ONLY if `id ∈ ∪sets` and `len < cap`. Called BEFORE the removal's set write. Bound: `declared ⊆ ∪sets` structurally (≤ handlers in use ≤ MC_MAX_MEETINGS × set size), plus hard cap `MAX_RECENTLY_DECLARED = 4096` (const; constructor-injectable for the test). **Overflow fails toward OVER-counting** (not inserted → a later meeting declaring the same handler counts again), one WARN per overflow transition; never unbounded.
- ONE declaration recorder on the actor, `record_declaration(id, trigger, detection)` = `metrics::record_handler_declaration` + `census.declared`, used by both health.rs and planned_transfer.rs, so `first_declarations{planned_transfer}` exists too (trigger vocabulary is shared).

**E. Config.** `MC_CLIENT_LOSS_DISAGREEMENT_FRACTION` (f64, finite, (0,1]; NaN/inf/0/negative/>1 refused, key named) and `MC_CLIENT_LOSS_DISAGREEMENT_MIN_CLIENTS` (u32, `[1, MAX=1000]` — `MC_MAX_PARTICIPANTS` scale) — REQUIRED keys per the story-4 convention (MissingEnvVar, no Rust default, named MIN_/MAX_ consts); ConfigMap ships 0.5 / 3 ("default" = shipped value); hard configMapKeyRef in mc-0 AND mc-1. Added to the missing-key and out-of-range table tests (positive arms 1.0 and 1). Config gauges `mc_media_handler_client_loss_disagreement_fraction` and `mc_media_handler_client_loss_disagreement_min_clients`, set at server construction from the same `DisagreementThreshold` the actors read (SSoT). Env-test 02 `CONFIG_GAUGES` gains both (a float conversion variant for the fraction).

**F. Metrics.** All with `key_custody="operator"`, no id labels. Ratio buckets `[0, .1, .25, .5, .75, .9, 1]` (contract); lead buckets DERIVED from N (`HeartbeatTiming::lead_histogram_buckets`: N/4, N/2, N, 1.5N, 2N, 3N, 5N — client loss can precede a lag-held Impaired by up to ~2N+). Zero-init: `loss_clients_total{reported}` both cells, `client_loss_disagreements_total`, `first_declarations_total{trigger}` for heartbeat_timeout|apply_lag|planned_transfer, `handlers_in_use` = 0 at boot. Histograms are not zero-initialised (repo convention).

**G. Dashboard / docs / alerts.** New row "Client Loss Evidence & Handler Census (ADR-0039 §8)" in `mc-media-handlers.json` (where every story-4 handler row lives; the task text says `mc-media.json` — flagging the deviation): loss clients by `reported` (rate) + client-weighted share, ratio distribution, lead p50/p95 vs N, disagreements, first declarations by trigger vs handlers in use, the two config echoes (with N beside, for the interpretability condition). Catalog entries per series. **No alert in this task** (evidence telemetry; the MC-own-path warning is OBS-ALERTS' rule, task #29), so no runbook scenario here; runbook Configuration Reference rows (Required=Yes, range, refuse-boot, gauges named, observational only — tuning changes only the counter, rollout/rollback, value edit rolls mc-0 and mc-1) + story-4 rollout key list.

### Tests (folds in test's design input)
- Pure: `LossObservation` table (n=5,k=2 → 2/3/0.4/lead; k=0 → no lead; connected=0 → no ratio); `assess` rows for disagreement with non-default thresholds (fraction 0.6, min 2): exactly met → disagreement at N+1ms not before; min−1 → never; fraction just below → never; quorum lost → episode ends; re-entry → second edge; Dead ends it; client loss never declares; `next_deadline` includes the quorum expiry. `status.rs`: `loss_since` keeps across DISCONNECTED→FAILED, cleared by CONNECTED, receipt-time only.
- HandlerCensus unit: 3 meetings declare H → +1 (other trigger cells unobserved); after H leaves every set and is declared again → +1 again; in_use distinct (2 meetings sharing H + 1 other = 2); retain after a meeting dies → recomputed absolute; cap reached → over-counts, size never exceeds cap.
- Integration (new `tests/handler_loss_metric_integration.rs` on the handler_detection rig, paused time, real actor): n=5 connected (MH-reported), k=2 → deltas 2/3, ratio count 1, lead count 1; (a) reporter not MH-connected → in neither; (b) DISCONNECTED then CONNECTED → not counted; (c) report after ref removal → ignored; (d) stale version, in-set ref → counted; (e) unsuperseded DISCONNECTED → counted; shared URL two refs → attributed by ref; both triggers (heartbeat_timeout and apply_lag) observe; planned transfer → all loss series `assert_unobserved` (before any count read) with a positive heartbeat_timeout control in the same rig; disagreement with non-default config → +1 once, never a removal (set unchanged, declarations unobserved), episode end + re-trigger → +1; census 3 meetings one handler → first_declarations +1, handlers_in_use recomputed after a meeting ends and the health walk runs.
- Config gauges value test; zero-init render test covers every new counter cell.
- Env-test 40 crash leg: `settled_baseline` + `rose` for `sum by (instance)(mc_media_handler_loss_clients_total)`, `sum by (instance)(mc_media_handler_loss_prior_report_ratio_count)`, and `first_declarations_total{trigger="heartbeat_timeout"}` — consts beside the others; rises only.


### Gate-1 notes folded in
- Security (confirmed): catalog (`client_loss_disagreements_total`, loss series) and runbook config rows each state "client-asserted, forgeable by ≥ min_clients colluding participants; not a trust signal"; disagreement/overflow logs carry meeting_id, handler_id, counts and thresholds only (no participant lists, no client ref/version); HandlerCensus recovers a poisoned lock (no unwrap/expect); a client report never triggers assess or a census write; fraction check `v.is_finite() && v > 0.0 && v <= 1.0`, table adds "NaN", "inf", "-0.0", "1e-400"; `loss_since` from the actor clock at intake; no new release-build seam.
- Observability (conditions accepted): (1c) BOTH paths build a fresh `ClientLoss` at the assessing instant — the deadline arm's actor loop computes it per member at the wake, never cached from a heartbeat; catalog states ratio `_count` < declarations gap = "declared with nobody connected", the numerator rule, and that clients MH reported dropped before the declaration are outside the denominator. (2) lead buckets from a `HeartbeatTiming::loss_lead_histogram_buckets()` beside the detection/interval derivations (live N, one source), covered by the existing derived-from-N unit test; lead uses `saturating_duration_since`. (3) catalog: unit = one (meeting, handler) episode (a forged-heartbeat MH in k meetings → k), re-entry counts again (≤ 1 per N per pair), "healthy" = Live incl. Lagging/Draining; boundary is `expired` = strictly LONGER than N (≥ N + 1 ms). (4) `HandlerCensus` doc states the bound (declared ⊆ handlers in some meeting's current set, pruned at every set write and on the walk, plus hard cap); a handler pruned then re-added and re-declared counts again; a Declared member counts in `handlers_in_use` until its removal's set write in the SAME actor turn. (5) fraction gauge renamed `mc_media_handler_client_loss_disagreement_ratio`; both config gauges carry key_custody=operator. (6) deviation to `mc-media-handlers.json` recorded; `docs/observability/dashboards.md` inventory updated; extra panel N vs `mh_media_transport_idle_timeout_seconds` in the loss row; client-weighted share = `sum(increase(…{reported="before_declaration"}[$__range])) / sum(increase(…[$__range]))`. No cause or id label on any series.
- Test (accepted): (1) 40_resilience planned legs: `PlannedBaselines.loss_clients` (settled sum of `mc_media_handler_loss_clients_total`) asserted `unchanged` in `completed()` and the without-target leg; section doc rewritten so the real series is the assertion (declaration proxy kept only as supporting evidence); `first_declarations{planned_transfer}` rose. (2) crash leg asserts `rose` on the loss_clients SUM (A and B pinned, MH killed: a new incarnation has a new handler_id, so nothing reconciles the old pair's connected set; connected ≥ 1) and on the ratio count. (3) after-removal arm built as a same-URL replacement (R1 at U removed, loss report naming R1 after removal, R2 granted at U, R2 declared → R1 report not counted). (4) census arm asserts declarations_total{heartbeat_timeout} delta 3 beside first_declarations delta 1; disagreement boundary pinned at expiry − 1 ms (no count) and expiry (counted).
- Dashboard: `/work/infra/grafana/dashboards/mc-media-handlers.json` (deviation from the task's `mc-media.json`, observability-agreed); plus `docs/observability/dashboards.md` and `crates/env-tests/tests/40_resilience.rs` planned-leg edits added to the file list.
- Code-reviewer + DRY (reconciled): HandlerCensus = `MeetingCensus<BTreeSet<HandlerId>>` (bound relaxed to `Default + PartialEq`; set/retain/poison-recovering lock reused) + a bounded `declared` set under its OWN lock, never nested: membership is read then the `declared` lock is taken; a stale entry (handler left its last set between the two) is pruned at the next set write or walk and is still under the cap — documented as the real reason for the shape. ONE membership hook: `publish_handlerless` → `publish_handler_censuses` writes both censuses from `self.media.handlers()`; called at actor start (new call to the hook, not census-only), grant add, `remove_handler_for` (covers planned cutover); `hand_off_teardown` and the controller walk forget/prune both. Quorum uses `reported / connected >= fraction` (division correctly rounded; 3/5 == 0.6), commented; fraction is a validated newtype `DisagreementFraction` (constructed only by config validation, NaN-safe `is_finite() && > 0 && <= 1`, manual PartialEq/Eq) so `HealthConfig` keeps `Eq`. `ClientLoss` has private fields, one constructor enforcing reported ≤ connected, accessors `not_reported()` / `ratio() -> Option<f64>`. ONE actor helper runs the per-member assessment (heartbeat path and deadline arm). Loss recording extracted to `record_loss_observation(&ClientLoss, trigger, now)` with the trigger match. Lead buckets: `HeartbeatTiming::loss_lead_histogram_buckets()` in liveness.rs registered via `set_buckets_for_metric` with the others; ratio buckets a named const; first_declarations zero-init iterates `RemovalTrigger::ALL`; MIN_CLIENTS through `bounded()`; one `set_client_loss_disagreement(threshold)` setter. Overflow WARN: no ids, cap + count, once per overflow transition; catalog says "over-counts past the cap". tokio→std Instant conversion in one place (intake).
- Code-reviewer (confirmed) note: HandlerCensus doc states the load-bearing ORDER — `declared()` precedes the declaring actor's removal set write (H is then in that actor's own set at insert; the following write prunes it if no other meeting holds it); a unit test pins the order (declare-then-write vs write-then-declare outcomes).

---

## Gate 1 — Plan Confirmation

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed (6 implementation notes, folded in) |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |

---

## Pre-Work

None

---

## Implementation Summary

- `handler_health.rs`: `ClientLoss` (private fields, `tally` the only builder, `not_reported`/`ratio`/`lead`), `DisagreementFraction` newtype (NaN-safe), `DisagreementThreshold::quorum` (division form), `HealthConfig::new(timing, disagreement)`, `LiveHealth.client_quorum` episode + shape bit, `client_disagreement_began`, quorum expiry in `next_deadline`. Client loss never declares (read after every declaring check).
- `status.rs`: `LatestReport.loss_since` (MC receipt of the loss run's first report; CONNECTED clears), `loss_since()` accessor; `intake` takes the actor's tokio instant (one std conversion).
- `meeting/health.rs`: `assess_member` (fresh snapshot per member, heartbeat path and deadline arm), `client_loss` (MH-reported denominator, ref-attributed numerator), `declare_and_remove` (census claim BEFORE removal write; declarations + first declarations counted only on removal; used by health and planned cutover), `record_loss_observation` (trigger match excludes planned_transfer), disagreement counted on its edge; transition log carries `client_disagreement`, `clients_connected`, `clients_reporting_loss`.
- `meeting_media.rs`: `MeetingCensus<V>` bound relaxed to `Default + PartialEq`; `HandlerCensus` (= `MeetingCensus<BTreeSet<HandlerId>>` + bounded recently-declared set, own lock never nested, cap 4096 over-counting with a once-per-overflow id-free WARN, poison-recovering); `believed_connected`; `assess_all` removed.
- `replacement.rs`: `publish_handlerless` → `publish_handler_censuses` (THE membership hook; actor start now calls it); `hand_off_teardown` and the controller walk forget/prune the handler census.
- Config: two REQUIRED keys (`parse_unit_fraction`, `bounded` for the minimum), `Config::client_loss_disagreement()`, deps + `WebTransportServer::new` param, config gauges `mc_media_handler_client_loss_disagreement_ratio` / `_min_clients`.
- Metrics: `record_handler_loss`, `record_client_loss_disagreement`, `record_first_handler_declaration`, `set_handlers_in_use`, `set_client_loss_disagreement`; ratio buckets const, N-derived lead buckets (`HeartbeatTiming::loss_lead_histogram_buckets`); zero-init of every new counter cell, both histograms and the gauge.
- Tests: pure tables (snapshot, quorum boundaries at 0.6/2, fraction validation, disagreement episodes incl. N vs N+1 ms), status `loss_since`, census (distinct/walk, once per handler + re-count after prune, the ORDER pin, cap over-count), config bounds (NaN/inf/-0/1e-400), metric recorders, zero-init; integration `handler_loss_metric_integration.rs` (k of n with a/b/c/d/e arms, after-removal at the same URL with R2 positive arm, apply_lag, planned unobserved + crash control in the same rig, disagreement once per episode + new episode + never a removal, below-either-threshold with positive arm, census across three meetings with declarations delta 3 control, boot config echo); `planned_transfer_integration.rs` proxy upgraded to the real loss counters.
- Deviation from Gate 1 (test item 3): two CONCURRENT set members can never share a URL (`HandlerSetError::DuplicateUrl` refuses it at hand-off and grant), so "sharing a URL" is covered across incarnations (the same-URL replacement arm) — the concurrent arm was not constructible and was dropped.
- Note: every MetricAssertion read drains EVERY histogram, so each test window reads at most one histogram first; the zero-initialised loss counters carry the rest.
- Env-tests: 40 crash leg `rose` on loss clients SUM, ratio `_count`, first_declarations{heartbeat_timeout}; planned legs `unchanged` on loss clients + `rose` on first_declarations{planned_transfer}, section doc rewritten; 02 required-key rows + config echo (min clients via `CONFIG_GAUGES`, fraction via a 1e-6 round-trip table).
- Infra/docs: ConfigMap keys (0.5 / 3) + hard refs in mc-0 and mc-1; runbook Configuration Reference rows, rollout key list, rollback note, verification checkbox; catalog section; dashboard row (9 panels) in `mc-media-handlers.json`; `dashboards.md` inventory; INDEX.

---

## Files Modified

Tracked diff vs start commit:  44 files changed, 2759 insertions(+), 235 deletions(-) (plus new `crates/mc-service/tests/handler_loss_metric_integration.rs` and this main.md). See the Cross-Boundary Classification table for the per-file list.

## Gate 2 — Full Validation

`DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` — TOTAL_RESULT=OK (L1 OK, L2 OK, L3 OK, L4 OK, L5 OK, L6 OK, L7 OK 2227s), attempt 1.

---

## Gate 3 — Review Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-DEFERRED | 1 | 1 (doc) | 1 (design spin-out) | SEC-1: disagreement counter's denominator can be emptied by an empty-connection_ids forger; docs/ADR/proto comment corrected; erasure-proof denominator spun out to docs/TODO.md §Media Path Obligations. Confirmed ADR-0039 edit (Domain-judgment owner). |
| Test | RESOLVED-FIXED | 1 | 1 | 0 | MetricAssertion drained every histogram on every read (vacuous unobserved checks); fixed per-name drain in crates/common testing.rs + proof-of-trap tests; deviations (DuplicateUrl arm, histogram reads) ruled on. |
| Observability | RESOLVED-FIXED | 3 | 3 | 0 | per-instance own-path panel; ratio count unit; census claim leak on refused removal. |
| Code Quality | RESOLVED-FIXED | 5 | 5 | 0 | F1 census claim leak (release_claim); F2 impl placement; F3-F5 testing.rs follow-ups incl. full workspace test run (0 failed). |
| DRY | RESOLVED-FIXED | 4 (+nit) | 4 (+nit) | 0 | shipped() config reader, shared health(), quorum via ratio(), present_gauge(), NonZeroU32::MIN. |
| Operations | RESOLVED-FIXED | 1 | 1 | 0 | census claim leak on refused removal. |
| Protocol (GSA, pulled in at review) | CLEAR | 0 | 0 | 0 | internal.proto comment-only hunk; no wire change. |

---

## Accepted Deferrals

- Security SEC-1 (spin-out): erasure-proof denominator for `mc_media_handler_client_loss_disagreements_total` — design change to the telemetry contract; see docs/TODO.md §Media Path Obligations. Must land before client loss becomes a trigger or the counter is relied on as the S1b control.

---

## DRY Reviewer

### Extraction opportunities

- MC `parse_unit_fraction` vs MH `parse_unit_ratio` — docs/TODO.md §Cross-Service Duplication ("From DRY Reviewer — story 4 MC-E (2026-11-13)").
