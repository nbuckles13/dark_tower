# Devloop Output: OBS-ALERTS — story-4 alert rules, inventory, SLO line, cross-service panels

**Date**: 2026-11-13
**Task**: Story 4 task #29 (OBS-ALERTS): R-35 alert rules across mc/mh/gc rule files, alerts.md inventory, slos.md open items, mc-media.json panels, env-test 33 coverage
**Specialist**: observability
**Mode**: Agent Teams (v2) — full, Gate-1 present <!-- panel mode + Gate-1 tier (ADR-0037 §D2); see the Tier row in Loop State -->
**Branch**: `feature/mh-failure-transfer`
**Duration**: ~2h

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `8eb5cbbf6ea2bf79bba853b48c27e9af79f12ac0` |
| Branch | `feature/mh-failure-transfer` |
| Lead Model | `claude-opus-5-5` |

---

## Loop State (Internal)

<!-- This section is maintained by the Lead for state recovery after interruption. -->
<!-- Do not edit manually - the Lead updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` |
| Implementing Specialist | `observability` |
| Tier | `full` |
| Iteration | `2` |
| Security | `security — RESOLVED-FIXED` |
| Test | `replaced by paired-test (--paired-with=test)` |
| Observability | `observability — RESOLVED-FIXED` |
| Code Quality | `code-reviewer — RESOLVED-FIXED` |
| DRY | `dry-reviewer — RESOLVED-FIXED` |
| Operations | `operations — RESOLVED-FIXED` |
| Semantic Guard | `not spawned (no check surface: alert rules/docs/dashboard/env-test)` |
| Paired Test | `paired-test — RESOLVED-FIXED` |
| Meeting Controller (rule-file owner) | `meeting-controller — RESOLVED-FIXED` |
| Media Handler (rule-file owner) | `media-handler — RESOLVED-FIXED` |
| Global Controller (rule-file owner) | `global-controller — RESOLVED-FIXED` |

<!-- LEAD REMINDER:
     - Update this table at EVERY phase transition
     - Capture teammate IDs AS SOON as you spawn them
     - When phase is review and all reviewers approve, advance to complete and proceed to Step 8 (Commit)
     - Only mark complete after Gate 3 approval
     - Use /devloop-status to check state
     - If interrupted, restart the devloop; main.md records start commit for rollback
     - Tier (ADR-0037 §D2): set the Tier row at setup from the manifest tier; under `light`, Step 5 SKIPS the Gate-1 plan round and writes a `### Gate 1 — SKIPPED (tier=light; reason: …)` marker instead of pending rows. A resumed devloop reads the tier from THIS Tier row (not the manifest, not the confirmations table); no Tier row ⇒ full (fail-safe).
-->

---

## Task Overview

### Objective
R-35: a single MH death never pages by itself; real user impact pages, the rest of the mechanism warns. Plus R-33/R-34 panel presentation and the R-38 SLO line (story 4, task #29 OBS-ALERTS).

### Scope
- **Service(s)**: MC, MH, GC alert rules; dashboards; env-tests; one test-only addition in mc-service
- **Schema**: No
- **Cross-cutting**: Yes

### Debate Decision
NOT NEEDED - governed by ADR-0039 and the 2026-10-28 telemetry contract

`docs/debates/2026-10-28-mh-replacement-and-migration/telemetry-contract.md`

---

## Cross-Boundary Classification

<!-- List EVERY planned file change. For each, classify per ADR-0024 §6.2:
     - Mine — in the implementing specialist's domain (trivial, the common case)
     - Not mine, Mechanical — cross-boundary, sed-test clean, guard-pipeline covered
     - Not mine, Minor-judgment — cross-boundary, bounded impact; owner must review & confirm at Gate 1 + Gate 3
     - Not mine, Domain-judgment — needs owner-implements or --paired-with=<owner>

     For Guarded Shared Area paths (ADR-0024 §6.4), Mechanical is disallowed; Owner must be filled.
     Fill Owner (if not mine) for cross-boundary rows.

     Path column convention: backtick-quoted paths. Globs (`*`, `?`, `[]`,
     trailing `/`, `/**`) and parenthetical annotations like `foo.rs` (regen)
     are tolerated by the `validate-cross-boundary-scope` parser at
     scripts/guards/common.sh, and are recommended where they clarify intent
     — use `dir/**` (or `dir/`, which the parser canonicalizes to
     `dir/**`) to scope a whole tree, `*.svelte` for a filename glob,
     and `(regen)` / `(cleanup)` /
     `(skeleton-only)` suffixes for per-row context. Prefer the simplest
     form that is accurate: if a literal path conveys the same information,
     use that; reach for a glob when enumerating every file would be noise,
     and reach for a parenthetical when the row's nature (regen, cleanup,
     new-vs-modify) materially changes how a reviewer reads it. Longer-form
     file-shape context (rationale, scope qualifiers, "why this shape") still
     belongs in § Implementation Summary or § Files Modified — the table
     answers one question per row: whose domain is this, and how stringent
     is the involvement. -->

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `infra/docker/prometheus/rules/mc-alerts.yaml` | Not mine, Minor-judgment | meeting-controller (ADR-0031) |
| `infra/docker/prometheus/rules/mh-alerts.yaml` | Not mine, Minor-judgment | media-handler (ADR-0031) |
| `infra/docker/prometheus/rules/gc-alerts.yaml` | Not mine, Minor-judgment | global-controller (ADR-0031) |
| `infra/grafana/dashboards/mc-media-handlers.json` (description text only, panel 222) | Not mine, Minor-judgment | meeting-controller (ADR-0031) |
| `docs/runbooks/mc-incident-response.md` (Alert: lines name the landed rules) | Not mine, Minor-judgment | operations |
| `docs/runbooks/mh-incident-response.md` (Alert: lines; Scenario 1 "until OBS-ALERTS lands" sentence) | Not mine, Minor-judgment | operations |
| `docs/runbooks/gc-incident-response.md` (Alert: lines) | Not mine, Minor-judgment | operations |
| `docs/runbooks/mc-deployment.md` (drop "lands with OBS-ALERTS" placeholders) | Not mine, Mechanical | operations |
| `docs/runbooks/mh-deployment.md` (drop "lands with OBS-ALERTS" placeholders) | Not mine, Mechanical | operations |
| `docs/runbooks/gc-deployment.md` (drop "lands with OBS-ALERTS" placeholders) | Not mine, Mechanical | operations |
| `crates/env-tests/tests/40_resilience.rs` (use the hoisted owner-silence gauge-name const) | Not mine, Mechanical | test (paired) |
| `crates/mc-service/src/media_routing/pusher.rs` (test only: bridge-window precondition beside `initial_registration_bound`; Gate 3 observability F6) | Not mine, Minor-judgment | meeting-controller |
| `docs/observability/alerts.md` | Mine | — |
| `docs/observability/metrics/mh-service.md` (retired re-assert prose sweep) | Mine | — |
| `docs/observability/metrics/mc-service.md` (per-migration completion ratio excludes planned_transfer on both sides; Gate 3 O1) | Mine | — |
| `infra/grafana/dashboards/mh-media.json` (description text only, panels 12, 14: retired re-assert prose) | Not mine, Minor-judgment | media-handler (ADR-0031) |
| `docs/observability/slos.md` | Mine | — (observability + operations joint; operations reviews) |
| `docs/TODO.md` (close "Missing alert: MC RegisterMeeting Coordination Failures") | Mine | — |
| `crates/env-tests/tests/33_alert_rules_loaded.rs` | Not mine, Minor-judgment | test (paired) |
| `crates/env-tests/src/fixtures/alert_rules_loaded.rs` | Not mine, Minor-judgment | test (paired) |
| `crates/env-tests/src/fixtures/metrics.rs` (`parse_prometheus_duration` -> pub) | Not mine, Mechanical | test (paired) |
| `crates/env-tests/src/fixtures/gc_fleet.rs` (factor a per-replica all-agree staleness reader) | Not mine, Minor-judgment | test (paired) |
| `crates/env-tests/src/fixtures/metric_hygiene.rs` (story-4 eager-series list) | Not mine, Minor-judgment | test (paired) |

---

## Planning

### Mechanism restatement

R-35 in mechanism terms: a single MH death produces M per-meeting declarations, but only ONE `mc_media_handler_first_declarations_total` increment per MC, and the meetings land on survivors or a grant within the replacement retry budget. Nothing per-pod and nothing that counts meetings may page. Paging happens only when a meeting has had no handler for longer than the budget (`MCMediaMeetingsAwaitingHandler`), when policy is unconfirmed (`MCMediaGenerationDivergence`), when a region has no healthy MH at all (`MHDown`), or when two live processes are evicting each other (`GCMhRegistrationPingPong`). **Invariant to complete:** every `for:` literal that stands in for a published config gauge carries `ANCHOR (DRY):` and is checked against the live gauge by env-test 33. There are THREE of these, not two: awaiting-handler, address_conflict and meetings-without-clients. Env-test 33 enforces the set as CLOSED. The prerequisite check passed: every series, label value and §J anchor exists at 8eb5cbbf (operations confirmed the anchors independently).

### Rules (name -> severity -> runbook anchor; expr; for)

**mc-alerts.yaml** (meeting-controller)
1. `MCMediaMeetingsAwaitingHandler` -> page -> mc#scenario-25-meetings-awaiting-a-media-handler. `max(mc_media_meetings_without_handlers) > 0`, `for: 3m`. ANCHOR (DRY): `mc_media_handler_replacement_retry_budget_seconds` (139 s shipped). Rounding rule: the smallest whole minute >= budget. The env-test asserts budget <= for < budget + 60 s. Description: some meeting has been awaiting continuously past the budget. Because the gauge is a per-MC count, overlapping churn can hold it up. MC keeps retrying at backoff_max until the per-meeting attempt bound. A meeting that is only Registering does not count. "No self-repair" is NOT claimed.
2. `MCMediaGenerationDivergence` -> page (kept) -> mc#15. The expr is unchanged and verified free of `handler_id_mismatch`. Rewrite the "QUIESCENT stays blackholed / force a structural change" comment, description and alerts.md Response steps 1 and 4. Since story 4 a lagging pair is re-published on every heartbeat, and lag past L = 2N declares it Impaired (apply_lag) and replaces it. A contract-fault refusal suppresses the lag clock. `transport_mode_mismatch` is invisible to the generation heartbeat and can still stay dark.
3. `MCMediaHandlerMigrationFailures` -> warning -> **mc#25** (operations-directed deviation from the task's "Scenario 24": the triage for these outcomes lives in Scenario 25, items 1, 3 and 5). `sum by (trigger, outcome) (increase(mc_media_handler_migrations_total{outcome=~"no_capacity|gc_unavailable|admission_refused|timed_out"}[15m])) > 0 unless on (trigger, outcome) sum by (trigger, outcome) (increase(mc_media_handler_migrations_total{trigger="planned_transfer", outcome="timed_out"}[15m]))`, `for: 0m`.
    - This keeps the contract's full outcome set and carves out only the drain-shed cell, so the shed has one home (rule 4). It is FAIL-CLOSED: if a crash-path timed_out ever appears, it alerts here (observability + code-reviewer).
    - The carve-out is recorded in the ADR-0031 block.
    - "While a handler survives" is not expressible from MC series; the dark case pages via rule 1.
    - Counts are per acquisition attempt.
4. `MCMediaHandlerDrainShed` -> warning -> mc#26. `sum(increase(mc_media_handler_migrations_total{trigger="planned_transfer", outcome="timed_out"}[15m])) > 0`, `for: 0m`. It never reads `mh_drain_meetings_shed_total`.
5. `MCMediaHandlerOwnPathSignature` -> warning -> mc#27. Per instance: `sum by (instance) (increase(first_declarations{trigger="heartbeat_timeout"}[15m])) >= 2` AND that increase / `sum by (instance) (max_over_time(mc_media_handlers_in_use[15m])) >= 0.5`. Window w = 15m, matching Scenario 27's diagnosis window. The max_over_time denominator is the PRE-loss census, because in_use drops in the same actor turn as the removal. Fraction 0.5 means at least half the handlers this MC used in the window. Derivation lives in the ADR-0031 block. `for: 0m`.
6. `MCMediaHandlerCrashRate` -> warning -> mc#24. `max(rate(mc_media_handler_first_declarations_total{trigger="heartbeat_timeout"}[1h])) > 3 / 3600`. `max` across MCs makes it per (handler, MC) and independent of fleet size: a `sum` would count one death once per MC that used the handler. A threshold of 3 per hour sits above one death plus one in-place restart, and below CrashLoopBackOff's ~12/h. `for: 0m`.
7. `MCMediaHandlerClientLossDisagreement` -> warning -> mc#28. `sum(increase(...disagreements_total[15m])) > 0`, `for: 0m`.
8. `MCMediaHandlerReplacementRateLimited` -> warning -> **mc#25** (operations-directed deviation: the triage is Scenario 25, item 5). `sum(increase(migrations{outcome="rate_limited"}[15m])) > 0`, `for: 15m` (sustained). The description cites `GC_MH_MAX_EXCLUDE_HANDLER_IDS` by name: "about 14 lifetime replacements per meeting at the shipped default".
9. `MCMediaHandlerConnectedSetCorrections` -> warning -> mc#24. A new Scenario 24 bullet covers two cases: benign, a lossy MH→MC notification path (cross-check MHMCNotificationFailures); and forged empty list (go to Scenario 28). `sum(increase(corrections[15m])) > 0`, `for: 15m`. The unit is per (handler, connection) retired, and a leaked departed binding also counts.
10. `MCMediaHandlerHeartbeatMisroute` -> warning -> mc#29. `sum by (outcome) (increase(heartbeats_total{outcome=~"released_unassigned|owner_mismatch"}[15m])) > 0`, `for: 0m`. The description names owner_mismatch after an MC crash restart as expected (Scenario 29).
11. `MCMediaHandlerRemovalReleaseRejected` -> warning -> mc#29. `sum by (outcome) (increase(removal_releases_total{outcome=~"rejected_ownership|invalid_argument"}[15m])) > 0`, `for: 0m`. It never selects unavailable_exhausted, error or unimplemented.
12. `MCMediaHandlerHeartbeatAnomalies` -> warning -> mc#29. `sum by (reason) (increase(heartbeat_anomalies_total[15m])) > 0`, `for: 0m`. The magnitude counts heartbeats, not incidents. apply_refused_invalid and apply_refused_transport_inconsistent are MC contract faults, never a replacement. The env-test `Injection` fixture is a known benign writer of applied_ahead_of_sent in the dev cluster.
13. `MCRegisterMeetingFailureRate` -> warning -> mc#12. `(sum(rate(mc_register_meeting_total{status="error"}[5m])) / sum(rate(mc_register_meeting_total[5m]))) > 0.10 and sum(rate(mc_register_meeting_total[5m])) > 0`, `for: 5m`. This is MCHighJoinFailureRate's shape. The denominator covers all statuses, and handler_replaced is not in the numerator. This closes the TODO entry.

**mh-alerts.yaml** (media-handler)
14. `MHDown` -> page (re-scoped) -> mh#1. `max by (region) (gc_mh_available{state="healthy", region!="other"}) == 0`, `for: 1m` (kept). Two deviations from the contract's literal text, recorded in the ADR-0031 block. `region!="other"` is needed because GC zero-writes the "other" cell, which would page forever in a single-region deployment. `max by (region)` dedupes GC replicas. Absent series (GC down) means silence BY DESIGN, because GCDown pages. `absent()` is not added. Operations, GC and MH agree. The runbook expr text is aligned to this.
15. `MHPodRestartingFrequently` stays. Its impact text gains "until MC migrates their meetings or the handler re-registers in place". It gets an inventory entry because it changed.
16. `MHMCNotificationFailures` reworded per event_type. A failed `connected` DECLINES the session (MC_UNAVAILABLE) and the client redials. A lost `disconnected` self-corrects within about two heartbeat intervals (N = `mc_media_handler_heartbeat_deadline_seconds`). The false "fire-and-forget, not retried" text is removed.
17. `MHFalseDeclarations` -> warning -> mc#24. `sum(increase(mh_media_handler_releases_total{trigger="heartbeat_timeout", clients="present"}[15m])) > 0`, `for: 15m`.
18. `MHOwnerSilenceReleaseWithClients` -> warning -> mh#26. The same selector with `trigger="owner_silence"`, `for: 0m`.
19-22. Per-reason media-dial warnings -> mh#24, all `sum(increase(mh_jwt_validations_total{token_type="media_dial", failure_reason="X"}[15m])) > 0`:
    - `MHMediaDialTokenExpired`, `for: 15m`. Causes: a broken refresh push, OR clock skew past `mh_media_dial_clock_skew_seconds`.
    - `MHMediaDialNoMeetingKey`, `for: 15m`. A dial that lands after a release is transient. A sustained rate is the register-before-token bug.
    - `MHMediaDialWrongTokenType`, `for: 0m`.
    - `MHMediaDialWrongHandler` (aud_mismatch), `for: 15m`.
23. `MHMeetingsHeldWithoutClients` -> warning -> mh#25. `max(mh_media_meetings_without_clients) > 0`. The `for:` follows the ONE convention (see "ONE convention" below): ANCHOR (DRY) to `mh_media_meeting_owner_silence_timeout_seconds` (900 s) + MH-sourced release-granularity/gauge-tick terms + mh scrape + warning-group interval, rounded up to the minute (expected ~16-17m). There is no bare margin, and the env-test range-asserts it. The description names both causes: (a) owner-silence release is broken, or (b) a live owning MC keeps answering OWNED for a meeting on this MH that no client reaches. Authority: contract §Alerts WARNING line 132, and §DASHBOARD ONLY line 136 ("alerted only when sustained past the owner-silence timeout"). The contract wins over §E.

**gc-alerts.yaml** (global-controller)
24. `GCMhRegistrationPingPong` -> page -> gc#14. The contract expr verbatim, `for: 10m`. The derivation cites `ANCHOR (DRY): gc_mh_load_report_interval_seconds` and gives a validity range: it holds while the interval is at most ~30 s (>= 40 supersedes per 10m, twice the floor). Re-derive above that. Config allows up to 60 s.
25. `GCMhUnknownHandlerLoadReports` -> warning -> gc#14. `sum(increase(gc_mh_load_reports_total{outcome="unknown_handler"}[10m])) > 0`, `for: 0m`. Causes: a live duplicate, a lost registry, or a retention-deleted row.
26. `GCMhAddressConflictPersisting` -> warning -> gc#14. `sum(increase(gc_mh_registrations_total{outcome="address_conflict"}[1m])) > 0`, `for: 4m`.
    - Derivation (revised at Gate 1 with global-controller and media-handler): for = B (`gc_mh_staleness_threshold_seconds`, 30 s, live gauge) + one MH registration retry (`BACKOFF_MAX`, 30 s, `crates/mh-service/src/grpc/gc_client.rs`) + expr window w (1m) + gc-service scrape_interval (5 s, `infra/kubernetes/observability/prometheus.yml`) + the warning group's evaluation `interval:` (60 s, gc-alerts.yaml). The sum is 185 s, rounded up to the next whole minute: 4m.
    - Why each term. GC answers FAILED_PRECONDITION, so the retry happens inside GcClient::register's backoff, not REGISTRATION_RETRY_DELAY. The re-register-after-NOT_FOUND path retries at the load-report interval (10 s), which is shorter, so BACKOFF_MAX bounds both paths. A fast FAILED_PRECONDITION keeps GC_RPC_TIMEOUT out of the sum, and the rounding absorbs the rest. `increase([w]) > 0` stays true for w after the last conflict. The scrape and eval quantization add one interval each.
    - Deviation from the contract's "for = B + one retry", for the reasons above.
    - The ANCHOR (DRY) names the gauge and BACKOFF_MAX. BACKOFF_MAX has no gauge, so the env-test reads it from source. The inventory Threshold Rationale states the same caveat.
    - The env-test asserts derived <= for < derived + 60 s. Every term comes from its source: live B, BACKOFF_MAX from source, w from the committed expr, the scrape interval from prometheus.yml, and the group interval from the rule file.
27. `GCMhRegistrationEndpointRejected` -> warning -> gc#14. `sum by (outcome) (increase(gc_mh_registrations_total{outcome=~"endpoint_changed|invalid_endpoint"}[10m])) > 0`, `for: 0m`.
28. `GCMhReplacementNoCapacity` -> warning -> mc#scenario-25 (a cross-file runbook, as the task specifies). `sum(increase(gc_mh_replacement_requests_total{outcome="no_capacity"}[15m])) > 0`, `for: 15m`.
No alert on gc_view or on the replacement-report evidence.

Exactly four pages: rules 1, 2, 14 and 24. Every expr is identity-free. The only grouping labels are trigger, outcome, reason and region, plus `instance` on rule 5, where the contract says "by instance" and instance is a target label, not an identity.


### ONE convention for config-derived `for:` literals (code-reviewer #4)
`for:` = ceil_to_whole_minute(Σ named delay terms). Every term comes from its source: a live gauge, a source const, the expr window, the job's scrape_interval in prometheus.yml, or the rule group's `interval:`. A term with no machine-readable source is a named fixture const with a BIDIRECTIONAL ANCHOR to the rule's ADR-0031 block. The env-test asserts derived <= for < derived + 60 s for all three rules. The terms include scrape and eval because quantization extends how long the expr stays truthy for a legitimate transient; leaving them out would make the `for:` fire early at the boundary.
- Awaiting-handler: retry budget (139) + mc scrape (5) + page-group interval (30) = 174 s -> `for: 3m`.
- Address_conflict: B (30) + BACKOFF_MAX (30) + w (60) + gc scrape (5) + warning-group interval (60) = 185 s -> `for: 4m`. A further constraint: w >= BACKOFF_MAX. Otherwise `increase([w]) > 0` drops to false between retries and resets `for:`. This is stated in the ADR-0031 block and asserted by the env-test. The BACKOFF_MAX read matches exactly one `const BACKOFF_MAX: Duration = Duration::from_secs(<int>);`. Zero or several matches, or another constructor, panic SOURCE-CONST-MISSING, which is distinct from FOR-DRIFT, and a Layer-4 adverse test covers it.
- Meetings-without-clients: owner-silence timeout (900) + the owner-silence release granularity and gauge-tick terms, read from MH source at implementation (the "5m margin" is replaced by named terms; any unsourceable residue is a named fixture const) + mh scrape (5) + warning-group interval (60), rounded up. The `for:` value is finalised from those terms, expected ~16-17m, and media-handler reviews the terms.

Test-side conditions for the convention (paired-test):
- Every summed term has a named, fail-closed home, never a fixture literal. The scrape interval is read live via `metrics::max_service_job_scrape_interval` (status/config). The eval interval is read from the LOADED group's `interval` in /api/v1/rules; a missing field is COULD-NOT-EVALUATE. Gauges use the live readers, and source consts use `const_duration_secs`.
- The env-test's per-rule term table mirrors the ADR-0031 block's term list one-to-one, so a reviewer can diff them.
- Unit tests: ceil_to_minute at an exact minute, +1 s and 0. The verdict at for == derived−1 s is FOR-DRIFT with a distinct lower-bound ("fires before self-repair") message, and at derived+60 s it is FOR-DRIFT (stale upward). `w >= BACKOFF_MAX` gets its own assert message.

### Alert thresholds (cross-cutting review required)
| Metric | Condition | For | Severity | Runbook |
|---|---|---|---|---|
| mc_media_meetings_without_handlers | max > 0 | 3m (ANCHOR retry budget) | page | mc#25 |
| mc_media_policy_pushes_total{outcome!="match"} | increase[15m] > 0 (unchanged) | 0m | page | mc#15 |
| mc_media_handler_migrations_total{no_capacity\|gc_unavailable\|admission_refused\|timed_out} unless {planned_transfer, timed_out} | increase[15m] > 0 | 0m | warning | mc#25 |
| mc_media_handler_migrations_total{planned_transfer, timed_out} | increase[15m] > 0 | 0m | warning | mc#26 |
| mc_media_handler_first_declarations_total{heartbeat_timeout} / max_over_time(mc_media_handlers_in_use) by instance | increase[15m] >= 2 and ratio >= 0.5 | 0m | warning | mc#27 |
| mc_media_handler_first_declarations_total{heartbeat_timeout} | max rate[1h] > 3/3600 | 0m | warning | mc#24 |
| mc_media_handler_client_loss_disagreements_total | increase[15m] > 0 | 0m | warning | mc#28 |
| mc_media_handler_migrations_total{rate_limited} | increase[15m] > 0 | 15m | warning | mc#25 |
| mc_media_handler_connected_set_corrections_total | increase[15m] > 0 | 15m | warning | mc#24 |
| mc_media_handler_heartbeats_total{released_unassigned\|owner_mismatch} | increase[15m] > 0 | 0m | warning | mc#29 |
| mc_media_handler_removal_releases_total{rejected_ownership\|invalid_argument} | increase[15m] > 0 | 0m | warning | mc#29 |
| mc_media_handler_heartbeat_anomalies_total | increase[15m] > 0 | 0m | warning | mc#29 |
| mc_register_meeting_total{status="error"} / all | ratio[5m] > 0.10 and volume > 0 | 5m | warning | mc#12 |
| gc_mh_available{healthy, region!="other"} | max by region == 0 | 1m | page | mh#1 |
| kube_pod_container_status_restarts_total{mh-service} | unchanged (impact text only) | 5m | warning | mh#11 |
| mh_mc_notifications_total | unchanged (text only) | 5m | warning | mh#10 |
| mh_media_handler_releases_total{heartbeat_timeout, present} | increase[15m] > 0 | 15m | warning | mc#24 |
| mh_media_handler_releases_total{owner_silence, present} | increase[15m] > 0 | 0m | warning | mh#26 |
| mh_jwt_validations_total{media_dial, expired \| no_meeting_key \| aud_mismatch} | increase[15m] > 0 | 15m | warning | mh#24 |
| mh_jwt_validations_total{media_dial, wrong_type} | increase[15m] > 0 | 0m | warning | mh#24 |
| mh_media_meetings_without_clients | max > 0 | ANCHOR owner-silence (convention above) | warning | mh#25 |
| gc_mh_registrations_total{superseded} and gc_mh_load_reports_total{unknown_handler} | >= 20 and >= 3 in 10m | 10m | page | gc#14 |
| gc_mh_load_reports_total{unknown_handler} | increase[10m] > 0 | 0m | warning | gc#14 |
| gc_mh_registrations_total{address_conflict} | increase[1m] > 0 | 4m (ANCHOR B + BACKOFF_MAX) | warning | gc#14 |
| gc_mh_registrations_total{endpoint_changed\|invalid_endpoint} | increase[10m] > 0 | 0m | warning | gc#14 |
| gc_mh_replacement_requests_total{no_capacity} | increase[15m] > 0 | 15m | warning | mc#25 |

### Retired re-assert prose sweep (code-reviewer #1)
- mh-media.json panel 12 "held ones keep re-asserting": FIX to "held ones keep being re-pushed (a re-registration of a held meeting is never refused)". No periodic cadence exists.
- mh-media.json panel 14 "story 4's re-assert fence ... a re-assert re-registers": FIX. Story 4 shipped no periodic re-assert. A late registration of a released meeting is fenced by MC's RELEASED heartbeat verdict (`mc_media_handler_heartbeats_total{outcome=~"released_ended|released_removed"}`). This panel still cannot show that fence, and the orthogonality is kept.
- metrics/mh-service.md:428 "§8's cadence re-asserts every meeting every ≤10 s": FIX. Equal-generation re-pushes still happen (an unconfirmed push is re-sent; a lagging pair is re-published each heartbeat), so counting them as applied stays right.
- :458 "depends on MC's re-assert cadence, deferred": FIX. Apply lag is detected on MC per heartbeat (apply_lag), so the MH-side balance stays unalerted by design.
- :472 "(no re-assert cadence yet)" and the ratchet paragraph: FIX. A lost EndMeeting is now reclaimed by the RELEASED verdict, and an MC crash by owner-silence release (15 min).
- :487 "Story 4 must bring its own signal": FIX. Name the RELEASED-verdict signal and keep the prohibition.
- :569 "re-asserts a smaller policy": FIX wording to "re-pushes".
- STILL TRUE, recorded: mh-overview.json panel 32 and mh-service.md:419/:470 "idempotent / equal-generation re-assert". Equal-generation pushes still occur (re-send of an unconfirmed push, lag re-publish). These are untouched.
- slos.md:420 is deleted by this task anyway.

### Security description requirements (Gate 1, folded in)
- S1: `MHMediaDialNoMeetingKey`. no_meeting_key is decided BEFORE the signature check, so it is UNAUTHENTICATED and attacker-drivable. Read it as the L4-2 ordering bug only when it correlates with `mc_register_meeting_total{status="error"}` or with legitimate clients failing to join; otherwise treat it as probing. The same caveat goes in alerts.md.
- S2: expired, wrong_type and aud_mismatch are post-signature, so only a holder of the meeting's MC-minted key can move them. wrong_type has two named causes: an MC mint defect, or the meeting key having signed a non-media_dial token (a key-custody issue).
- S3: the adversarial hypotheses stay in the text. ClientLossDisagreement is "forged heartbeat vs client network path"; RateLimited is "flap OR forged loop".
- S4: owner_mismatch is benign only within about one owner-silence timeout (`mh_media_meeting_owner_silence_timeout_seconds`, 900 s shipped) after that MC restarted. Persistence beyond that, or with no MC restart, is a routing or contract defect, and mc_id is self-asserted. The anomalies Injection note is scoped to the dev cluster: "in production every reason is a fault".
- S5: unknown_handler / address_conflict. Handler identity is self-asserted (shared MH OAuth client), so a live duplicate may be a second process presenting the MH credential, not only a misconfiguration.

### Docs
- alerts.md: an entry for every new or changed rule, each with Severity, Condition, Impact, Runbook, PromQL byte-identical to the rule, for:, and Threshold Rationale. The rationale POINTS at the rule's ADR-0031 block rather than re-deriving. Also: update the MC page convenience list, rewrite the MCMediaGenerationDivergence response steps, add a GC section and add MH inventory-list names.
- slos.md: replace the applied-generation open item with the MH replacement SLI line (target OPEN, observed, not gated, ADR-0036 §10). Delete the MCRegisterMeetingFailureRate row.
- TODO.md: mark "Missing alert: MC RegisterMeeting Coordination Failures" closed.
- Runbooks (operations): rewrite the Alert lines of mc Scenarios 24, 25 and 26 to name the committed rules with their committed selectors. Drop timed_out from the failures regex, and have Scenario 26 name only MCMediaHandlerDrainShed. Add the Scenario 24 corrections bullet and the two Scenario 29 bullets. Replace each "rule name assigned by OBS-ALERTS (lands with OBS-ALERTS)" with the rule name. Add the removal-release and anomalies rules to Scenario 29's Alert line. Drop MH Scenario 1's "Until OBS-ALERTS lands…" sentence and align its expr.

### Dashboard
The R-33/R-34 panels ALREADY exist, on `mc-media-handlers.json`. The contract §137 moved the MC handler rows there at MC-F. Panel 203 is "N", panel 221 is "Interpretability: N vs MH QUIC Idle Timeout", and panels 219, 220 and 222 are the loss share and its count, adjacent at y = 69-77. Each reads only existing series. The only change is to add the interpretability sentence to panel 222's description ("meaningful only when client detection time ≈ N"). Duplicating the panels onto mc-media.json would create a second home, so **deviation from the task text: no mc-media.json edit**. meeting-controller CONFIRMED this at Gate 1, and Gate 3 should read it as deliberate.

### Env-tests (paired with test)
- 33: add a `STORY_4_ALERTS` const listing every new or changed rule. Check on-disk first, with its own token, then loaded, then expr parity.
- Generalise the story-3 APPLY loop into a helper taking (alerts, eager classifier). Eager series get one home in `metric_hygiene.rs`. Lazy combinations are reported ACKNOWLEDGED-VACUOUS by name. Pure literal regex alternations are probed per alternative.
- New `#[tokio::test]` for: vs live gauge. The anchored set is STRUCTURAL: only an `ANCHOR (DRY):` comment inline on, or immediately above, the `for:` line counts. PingPong's derivation ANCHOR is not on its `for:`, so it is excluded, and unit tests cover both sides. A PingPong validity assert checks that every live GC replica's `gc_mh_load_report_interval_seconds` is <= the derivation bound (30 s), with token DERIVATION-OUT-OF-RANGE. Fixture copies of rule-owned numbers (the 30 s PingPong bound, and any unsourceable residue term) say in their rustdoc that the RULE is the home. The BACKOFF_MAX reader is a new `const_duration_secs(text, name)` sibling of `const_str_array` in gc_fleet.rs. It has a distinct NOT-FOUND token and fails closed on a missing declaration or any non-`from_secs(<int>)` form, with adverse tests for a renamed const and for from_millis. The page-liveness static check asserts it saw MHDown among the page rules (positive control), with an adverse unit test. It reads the LOADED rule `duration` (/api/v1/rules) and the committed `for:`. Tokens: STALE-DEPLOY when they differ, FOR-DRIFT when the value is off the derivation. The anchored set is CLOSED: every rule whose `for:` carries `ANCHOR (DRY):` naming a `*_seconds` gauge must equal the checked table, with a positive control.
  - Gauges are read with `metrics::mc_replacement_retry_budget`, `gauge_by_pods` over `live_mh_pods()` (all pods equal), and a GC per-replica all-agree staleness reader factored out of `gc_fleet.rs`. A missing gauge is a PRECONDITION panic.
- APPLY for gc_mh_available (not boot-initialised): a bounded, failure-only poll. The bound is gc_fleet::gc_health_tick(source) + ConsistencyCategory::MetricsScrape.timeout(), and a timeout reports the same SELECTOR-DOES-NOT-APPLY token naming the bound. Boot-zeroed series keep the instant query.
- A static check that no `severity: page` rule selects `up` with a `job="mh-service"` matcher, or `kube_pod_container_status_restarts_total`. It matches structurally, by metric identifier plus matcher, via `vector_selectors`. Positive control: the old `up{job="mh-service"} == 0` and a spaced variant must hit.
- The module rustdoc states that 33 proves loaded/expr/apply, not fire, and that R-35 is a property of the rule design. No fire assertions anywhere.
- Unit tests (Layer 4) for every new pure helper: for:/duration parse, loaded-duration extraction, alternation split (metacharacter adverse case), the source-const reader and the expr-window extraction.
- `mc_media_meetings_without_handlers` is a boot gauge (set to 0 at boot), so test-gates line 10 is covered by the eager APPLY.

### Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test (paired) | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Meeting Controller | confirmed |
| Media Handler | confirmed |
| Global Controller | confirmed |

Classification-sanity guard: `STATUS=OK REASON=cross-boundary-classification-clean-1-files`. Plan approved.

---

## Pre-Work

{Any pending changes committed before starting, dependencies resolved, etc.}

{Or "None" if no pre-work was required}

---

## Implementation Summary

### Rules (27 new or changed; exactly four pages)
- `mc-alerts.yaml`: `MCMediaMeetingsAwaitingHandler` (page, anchored; Gate 3: a 1m `max_over_time` bridges Registering dips, so the `for:` gained the bridge term → 4m). `MCMediaGenerationDivergence` keeps its expr, and its quiescent/re-push premise is rewritten to the story-4 heartbeat + apply_lag mechanism. Eleven warnings, per plan, under a shared ADR-0031 block.
- `mh-alerts.yaml`:
  - `MHDown` is re-scoped to `max by (region) (gc_mh_available{state="healthy", region!="other"}) == 0`.
  - `MHMCNotificationFailures` and `MHPodRestartingFrequently` are reworded.
  - Seven new warnings, including `MHMeetingsHeldWithoutClients` (anchored `for:`; Gate 3: per instance, `max by (instance)`).
  - The header's teardown-never-arrives note is updated for story 4.
- `gc-alerts.yaml`: `GCMhRegistrationPingPong` (page, contract expr verbatim, validity range ≤ 30 s interval) and four warnings, including `GCMhAddressConflictPersisting` (`for: 4m`, anchored).
- **Deviation found at implementation**: `MCRegisterMeetingFailureRate`'s denominator is `status=~"success|error"`, not all statuses. The OPS-landed mc Scenario 12 states that `handler_replaced` is excluded from the denominator so that an MH restart storm cannot dilute real errors. That reasoning holds, and it is recorded in the rule's block. meeting-controller is asked to confirm at Gate 3, since it had suggested all statuses at Gate 1.

### Docs
- `alerts.md`: 27 inventory entries generated from the rule files, with PromQL byte-identical (`dt-guard alert-rules-policy`: 83 inventory pairs). The MC page list and the MH inventoried list are updated, and the MCMediaGenerationDivergence Response steps 1, 4 and 5 are rewritten.
- `slos.md`: the open item is replaced and the row is deleted. `TODO.md`: the RegisterMeeting entry is closed.
- Runbooks: every "rule name assigned by / lands with OBS-ALERTS" placeholder is replaced with the committed rule name and selector.
  - mc Scenarios 24, 25 and 26 Alert lines are rewritten; Scenario 26 names only DrainShed.
  - Added: the Scenario 24 corrections bullet and the two Scenario 29 bullets.
  - mh Scenario 1's interim sentence is dropped, and Scenario 10 now uses "within N, normally the next heartbeat".
- Retired re-assert prose sweep, as planned: `metrics/mh-service.md` and `mh-media.json` panels 12 and 14.

### Dashboard
- `mc-media-handlers.json` panel 222 description gains the interpretability condition. `mc-media.json` is untouched, per the confirmed deviation.

### Env-tests (paired with test)
- Fixture kernels, unit-tested:
  - `alert_rules_loaded.rs`: `committed_rules`, structural `anchored_for_alerts`, `loaded_rule_timings`, `range_windows`, `literal_alternatives`, `apply_probes`, `ceil_to_minute`, `for_verdict`, `per_pod_mh_liveness_selectors`.
  - `gc_fleet.rs`: `const_duration_secs` (SOURCE-CONST-MISSING), `gc_replica_values` (now also behind `removal_settle_budget_for`), `agreed_value`, `gc_config_gauge_agreed`.
  - `metrics.rs`: `parse_prometheus_duration` is now pub. Added `MH_OWNER_SILENCE_TIMEOUT_GAUGE` (40 uses it) and `mh_owner_silence_timeout` (all pods agree).
- `33`:
  - `STORY_4_ALERTS` is chained into the on-disk, loaded and expr controls.
  - The APPLY loop is generalised (`assert_rule_selectors_apply`, a classifier with a gap panic, a bounded poll for `gc_mh_available`) and adds `story_4_rule_selectors_apply_to_real_series`.
  - `for_literals_match_their_config_derivation` covers the closed anchored set, STALE-DEPLOY, FOR-DRIFT in both directions, DERIVATION-OUT-OF-RANGE for w ≥ BACKOFF_MAX, and the PingPong interval.
  - The static test `no_page_rule_selects_per_pod_mh_liveness` has a positive control.

## Files Modified

```
 .../env-tests/src/fixtures/alert_rules_loaded.rs   | 892 +++++++++++++++++++--
 crates/env-tests/src/fixtures/gc_fleet.rs          | 157 +++-
 crates/env-tests/src/fixtures/metric_hygiene.rs    |  51 ++
 crates/env-tests/src/fixtures/metrics.rs           | 119 ++-
 crates/env-tests/tests/33_alert_rules_loaded.rs    | 575 ++++++++++---
 crates/env-tests/tests/40_resilience.rs            |   5 +-
 crates/mc-service/src/media_routing/pusher.rs      |  79 ++
 docs/TODO.md                                       |   4 +-
 docs/observability/alerts.md                       | 451 ++++++++++-
 docs/observability/metrics/mc-service.md           |   2 +-
 docs/observability/metrics/mh-service.md           |  12 +-
 docs/observability/slos.md                         |   3 +-
 docs/runbooks/gc-deployment.md                     |   2 +-
 docs/runbooks/gc-incident-response.md              |   9 +-
 docs/runbooks/mc-deployment.md                     |   2 +-
 docs/runbooks/mc-incident-response.md              |  89 +-
 docs/runbooks/mh-deployment.md                     |   6 +-
 docs/runbooks/mh-incident-response.md              |  25 +-
 infra/docker/prometheus/rules/gc-alerts.yaml       | 172 ++++
 infra/docker/prometheus/rules/mc-alerts.yaml       | 465 ++++++++++-
 infra/docker/prometheus/rules/mh-alerts.yaml       | 254 +++++-
 infra/grafana/dashboards/mc-media-handlers.json    |   2 +-
 infra/grafana/dashboards/mh-media.json             |   6 +-
 23 files changed, 3093 insertions(+), 289 deletions(-)
```

### Key Changes by File
See §Implementation Summary (rules, docs, dashboard, env-tests) and the Gate-3 fix list.

---

## Devloop Verification Steps

Gate 2: `DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` (unattended, run-all) — `TOTAL_RESULT=OK`, TOTAL_DURATION=2523s. Log: `/tmp/layer-all.log`.

| Layer | Result | Duration |
|-------|--------|----------|
| 1 Compile | OK | 7s |
| 2 Format | OK | 6s |
| 3 Guards | OK (incl. dt-guard alert-rules: inventory_expr_drift 83 pairs OK, cross-boundary classification/scope) | 139s |
| 4 Test | OK (incl. new Layer-4 static checks: closed anchored-for: set, page posture, source-const reads, MC bridge-window precondition) | 143s |
| 5 Lint | OK | 5s |
| 6 Audit | OK | 5s |
| 7 Env-tests | OK (Rust env-tests incl. 33_alert_rules_loaded live for:/gauge derivations + APPLY probes, and browser E2E) | 2218s |

Fast check before review: `layer-fast.sh` L1-6 OK (301s).

---

## Code Review Results

### Security Specialist
**Verdict**: RESOLVED-FIXED
**Findings**: 1 found, 1 fixed, 0 deferred

Gate-1 points S1-S5 (pre/post-signature JWT reasons, forged-heartbeat/loop hypotheses kept, bounded benign cases, shared-credential duplicate) folded into descriptions/alerts.md/runbooks. F1: MCMediaHandlerHeartbeatMisroute benign owner_mismatch window ignored the 15m increase tail — description and alerts.md now say "judge by the counter, not the alert". No identity labels, no ids/endpoints/tokens in annotations, runbook_urls under docs/runbooks/.


### Test Specialist
**Verdict**: RESOLVED-FIXED (paired-test, --paired-with=test)
**Findings**: 6 found, 6 fixed, 0 deferred

F1 APPLY probe dropped positive matchers beside a negation (MHDown state="healthy" unprobed) — fixed with unit tests. F2 static checks moved from Layer 7 to Layer 4. F3 MC budget input now requires pod agreement (`agreed_value`). F4 anchored_for_alerts returns Result (orphan anchor = Err). F5 docs/TODO.md stale gauge_by_live_pod entry marked resolved. F6 poll bound computed once via named helper. `cargo test -p env-tests --lib`: 378 passed. No env-test asserts an alert fires (or stays silent) on a single MH death.


### Observability Specialist
**Verdict**: RESOLVED-FIXED
**Findings**: 6 found, 6 fixed, 0 deferred

Gate 1 asks (both adopted into the plan): MCMediaHandlerMigrationFailures keeps the contract's full outcome set and carves out only the drain-shed cell with `unless on (trigger, outcome)`, so it fails closed. GCMhAddressConflictPersisting states and asserts `w >= BACKOFF_MAX`, and its source-const read fails closed (SOURCE-CONST-MISSING).

Gate 3 findings, all fixed:
- F1 `MHMeetingsHeldWithoutClients`: the fleet `max(...)` deviated from the contract's unaggregated expr while the rule block claimed it was the contract's. It also let different pods alternately satisfy `for:`. Now `max by (instance)`.
- F2 slos.md completion SLI: the numerator lacked `trigger!="planned_transfer"`, but MC-F records a `completed` terminal for planned transfers, so the ratio could exceed 1. Both sides now exclude planned transfers.
- F3 `MCMediaHandlerHeartbeatAnomalies`: `apply_refused_*` were called "MC/MH" contract faults. The task and the code (`liveness.rs`, `record_heartbeat_anomaly`) say MC. Corrected in the rule comment, the description, alerts.md and mc runbook :4383.
- F4 env-test 33 `apply_probes`: a selector with any negated matcher was probed as its bare name, dropping positive matchers, so a misspelled `state="healthy"` on MHDown would pass APPLY. Now only negated matchers are dropped, with Layer-4 cases.
- F5 `MCMediaHandlerCrashRate` comment: said "3 is reached", but `> 3/3600` fires on the 4th death. Reworded.
- F6 `MCMediaMeetingsAwaitingHandler`: the Registering-dip bridge window (`max_over_time[1m]`, added at meeting-controller's Gate 3) had an unchecked precondition, w >= `initial_registration_bound()` + scrape. That is the same class as address_conflict's asserted precondition. Now enforced by a Layer-4 test beside the formula (`pusher.rs::awaiting_handler_bridge_window_covers_the_registration_bound`, BRIDGE-WINDOW-OUT-OF-RANGE, fails closed), and the precondition is stated in the rule block.

Verified independently:
- **Inventory:** all 28 new or changed rules have PromQL byte-identical to alerts.md (own extraction plus dt-guard alert-rules, 83 pairs), and for:, severity and runbook agree between rule and entry.
- **Anchored for: values:** the three reproduce from their terms (234 s → 4m, 185 s → 4m, 975 s → 17m), and the anchored set is closed.
- **Label hygiene and zero-init:** no identity grouping (only trigger, outcome, reason and region; instance on the own-path and held-without-clients rules). Every selected MC/GC counter is zero-initialised.
- **Pages:** exactly four.
- **Gate-1 deviations:** MHDown `region!="other"` (GC zero-writes that cell) and the panels staying on mc-media-handlers.json (203/219-222 already exist) were checked against code.

### Code Quality Reviewer
**Verdict**: RESOLVED-FIXED
**Findings**: 5 found, 5 fixed, 0 deferred (plus 6 Gate-1 plan concerns, all resolved in plan)

F1 hand-rolled poll -> `eventual::poll_within`. F2 duplicate agreed_value -> one home in metrics.rs. F3 `MatchOp::as_str` with round-trip test. F4 190-line derivation test split (derivation_terms + DerivationCtx). F5 doc comments narrowed (STORY_4_PAGES containment-only; regex-matcher residual stated). ADR-0031 thresholds table and retired-prose sweep present.


### DRY Reviewer
**Verdict**: RESOLVED-FIXED

**True duplication findings** (entered fix-or-defer flow):
F1 two pod-agreement kernels -> `agreed_value`; F2 three rule-YAML readers -> `committed_rules`; F3 three /api/v1/rules walks -> `loaded_alerting_rules`; F4 duplicate mounted-files walk; F5 hand-rolled poll -> `poll_within`; F6 MatchOp spelling table -> `as_str`; F7 unguarded copies of anchored for:/thresholds in alerts.md rationale and runbooks -> replaced with pointers to the rule; F8 MC pod-lookup prelude -> `mc_config_gauge_values`. All fixed.

**Extraction opportunities** (appended to `docs/TODO.md`):
None


### Operations Reviewer
**Verdict**: RESOLVED-FIXED
**Findings**: 2 found, 2 fixed, 0 deferred

P1 (Gate 1): three rules linked to mc Scenario 24 without triage — MigrationFailures and ReplacementRateLimited now link mc#25 (operations-directed deviation recorded in ADR-0031 blocks); ConnectedSetCorrections keeps #24 with a new triage bullet. O1: slos.md MH-replacement completion ratio excluded planned_transfer only in the denominator — both sides now exclude it; mc-service.md:1205 tightened. Runbook URLs resolve (mechanical check), alerts.md severity/runbook fields match rules, exactly 4 pages.


### Semantic Guard Reviewer
Not spawned: no check surface in the diff (alert rules, docs, dashboard descriptions, env-test/test-only Rust; no credential types, Debug/Display over payloads, or client credential-lifetime handling).


### Meeting Controller Reviewer (rule-file owner: mc-alerts.yaml, MC dashboards)
**Verdict**: RESOLVED-FIXED
**Findings**: 3 found, 3 fixed, 0 deferred

Every MC series, label value and outcome used was verified against MC code at 8eb5cbbf (`observability/metrics.rs`, `media_routing/{replacement,liveness,teardown,handler_health}.rs`, `actors/meeting/replacement.rs`, `grpc/{mh_client,media_coordination}.rs`). Planning inputs folded in at Gate 1:
- awaiting-handler wording (MC keeps retrying at backoff_max until the attempt bound, so the page does not claim "no self-repair");
- the divergence comment and description no longer rely on the quiescent-blackhole premise (heartbeat re-publish while lagging, apply_lag at L = 2N);
- `timed_out` is planned_transfer-only, so the drain-shed cell has one home;
- crash rate is per (handler, MC) via `max`;
- the own-path denominator is the pre-loss census, via `max_over_time`;
- the anomaly magnitude counts heartbeats, not incidents.

Panel deviation confirmed: the R-33/R-34 panels already live on the MC-owned `mc-media-handlers.json` (203, 219-222). The interpretability sentence was added to panel 222 and nothing was duplicated onto `mc-media.json`. The `MCRegisterMeetingFailureRate` denominator deviation (`success|error`, per OPS Scenario 12) is accepted.

- **Finding F1**: `MCMediaMeetingsAwaitingHandler` under-paged. The gauge is written from `handlers.is_empty()` (`actors/meeting/replacement.rs:522`), so a grant still Registering takes a dark meeting out of it for up to 48 s and resets `for:`. A meeting cycling grant -> failed registration could not page until rate_limited, about 14 cycles later.
  - **Fix**: `max(max_over_time(mc_media_meetings_without_handlers[1m])) > 0` with `for: 4m`, plus a bridge-window term in the derivation and in the env-test 33 check. The description, alerts.md and Scenario 25 now state the consequence.
  - **Status**: Fixed.
- **Finding F2**: Scenario 25 Resolution said the meeting "self-recovers when a grant arrives".
  - **Fix**: it now says "when a grant acks its first RegisterMeeting (becomes Active)", and notes that an admission_refused grant does not recover the meeting.
  - **Status**: Fixed.
- **Finding F3**: two sustained warnings said "has kept rising for 30 minutes".
  - **Fix**: the wording now states the real `increase([15m]) > 0` + `for: 15m` semantics, applied to every such rule and mirrored in alerts.md.
  - **Status**: Fixed.

Post-verdict addition reviewed (observability Gate-3 F6): the test-only `awaiting_handler_bridge_window_covers_the_registration_bound` in `crates/mc-service/src/media_routing/pusher.rs`.
- It reads the committed rule and the `mc-service` scrape job and asserts `w >= initial_registration_bound() + scrape`. It fails closed.
- Green at 48 s + 5 s against the 1m window.
- Adverse check: with the window temporarily set to 50s it went red with `BRIDGE-WINDOW-OUT-OF-RANGE`; the file was then restored.
- No production code change. Verdict unchanged.

### Global Controller Reviewer (rule-file owner: gc-alerts.yaml)
**Verdict**: RESOLVED-FIXED
**Findings**: 1 found, 1 fixed, 0 deferred

Gate 1: MHDown re-scope (`region!="other"`, `max by (region)`), PingPong derivation interval range, unknown_handler causes, address_conflict `for: 4m` derivation. F1: GCMhReplacementNoCapacity description/inventory omitted that no_capacity is computed after the request's exclude list (healthy > 0 AND free slots can still yield it) — fixed in rule and alerts.md.

### Media Handler Reviewer (rule-file owner: mh-alerts.yaml)
**Verdict**: RESOLVED-FIXED
**Findings**: 3 found, 3 fixed, 0 deferred

F1 MHMediaDialWrongHandler comment gave an impossible aud_mismatch cause — now sequential address reuse. F2 mh-service.md catalog RELEASED vs owner-silence fates corrected to match docs/TODO.md. F3 stale "until the pod restarts" text removed from mh-media.json (2 panels) and mh-service.md. Accepted observability's `max by (instance)` change to MHMeetingsHeldWithoutClients.

### Gate 3 Verdict Summary

| Reviewer | Verdict | Findings | Fixed | Deferred |
|----------|---------|----------|-------|----------|
| Security | RESOLVED-FIXED | 1 | 1 | 0 |
| Test (paired) | RESOLVED-FIXED | 6 | 6 | 0 |
| Observability | RESOLVED-FIXED | 6 | 6 | 0 |
| Code Quality | RESOLVED-FIXED | 5 | 5 | 0 |
| DRY | RESOLVED-FIXED | 8 | 8 | 0 |
| Operations | RESOLVED-FIXED | 2 | 2 | 0 |
| Meeting Controller | RESOLVED-FIXED | 3 | 3 | 0 |
| Media Handler | RESOLVED-FIXED | 3 | 3 | 0 |
| Global Controller | RESOLVED-FIXED | 1 | 1 | 0 |

---

## Accepted Deferrals

**Each entry here is an issue the devloop chose NOT to fix.** Every bullet is a cost shift: the implementer didn't pay the fix-now cost, so a future reader will pay fix-later cost + tracking overhead. List only what was actually deferred — not "follow-ups" or "future improvements" or "potential extractions." If something was fixed, it doesn't belong here.

**Tech debt entries themselves live in `docs/TODO.md`. This section holds only pointers to those entries.** Do not create a `TODO.md` at the repo root or anywhere else — there is exactly one `docs/TODO.md` for the whole project. Do not inline the debt body here — multi-line entries belong in `docs/TODO.md`, not in this section.

Each pointer is exactly one bullet of the form `- \`docs/TODO.md\` §SECTION-NAME — one-line hook (≤80 chars)`. If you wrote more than one line per entry, you're writing it in the wrong file — move the body to `docs/TODO.md` and leave only the pointer here.

- (none surfaced in this devloop)

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `8eb5cbbf6ea2bf79bba853b48c27e9af79f12ac0`
2. Review all changes: `git diff {start_commit}..HEAD`
3. Soft reset (preserves changes): `git reset --soft {start_commit}`
4. Hard reset (clean revert): `git reset --hard {start_commit}`
5. For schema changes: rollback requires a forward migration — `git reset` alone is insufficient if migrations were applied
6. For infrastructure changes: may require `skaffold delete` or `kubectl delete -f` if manifests were applied
7. **Safe-revert unit** (answer explicitly, even if "the whole commit"): can any part of this diff be reverted or cherry-picked on its own, or does a partial revert reconstruct a state worse than either endpoint (e.g. a security fix split from the change that made it necessary, or a client/server/alert-rule set that must move together)? If partial reverts are unsafe, name the unit and the safe direction. **Answer**: `the whole commit` — rule files, alerts.md inventory (inventory_expr_drift guard), runbook Alert lines and env-test 33's closed anchored-for: set must move together; a partial revert reds Layer 3/4/7. (This converts silence into a visible unanswered slot; it cannot distinguish a checked answer from a reflexive one.)

---

## Issues Encountered & Resolutions

### Issue 1: Contract literal exprs not deployable as written
**Problem**: The contract's MHDown expr would page forever on GC's zero-written region="other" cell; "for = B + one retry" for address_conflict would fire on routine conflicts.
**Resolution**: Recorded deviations in ADR-0031 blocks (region!="other" + max by region; for: 4m from sourced terms), confirmed by GC/MH/observability.

### Issue 2: Task named a deleted helper and a dashboard that no longer hosts the panels
**Problem**: `gauge_by_live_pod` was deleted earlier; R-33/R-34 panels live on mc-media-handlers.json, not mc-media.json.
**Resolution**: Used `gauge_by_pods` + `agreed_value`; edited panel 222's description only (MC confirmed). TODO entry marked resolved.

### Issue 3: Teammates could not address `team-lead`
**Problem**: Lead was reachable only as `main` in this harness.
**Resolution**: Teammates re-pointed to `main`; no gate impact.

---

## Lessons Learned

1. Gauge-backed `for:` literals need a single derivation convention (sum of sourced terms incl. scrape+eval, ceil to minute) enforced as a closed set by tests.
2. Owner reviewers caught behavioral under-paging (Registering dips) that series-existence checks cannot.
3. Static rule checks belong in Layer 4, not cluster-gated Layer 7.

---

## Appendix: Verification Commands

```bash
# Commands used for verification
./scripts/verify-completion.sh --layer full

# Individual steps
cargo check --workspace
cargo fmt --all --check
./scripts/guards/run-guards.sh
DATABASE_URL=... cargo test --workspace
DATABASE_URL=... cargo clippy --workspace --lib --bins -- -D warnings
./scripts/guards/semantic/credential-leak.sh path/to/file.rs
```

## Gate 3 fixes (iteration 1)
- security F1: misroute wording now says "judge by the counter, not the alert".
- global-controller: no_capacity means "after the exclude list"; the triage applies only when healthy handlers exist AND have free slots.
- media-handler:
  - F1: the WrongHandler comment now describes sequential address reuse.
  - F2: corrected the fates (ii) vs (i)/(iii) disposition.
  - F3: replaced the stale "until the pod restarts" text in mh-media.json (2 panels) and mh-service.md.
- meeting-controller:
  - F1: the awaiting-handler page bridges the Registering dips with a 1m `max_over_time`, and its `for:` is 4m with the bridge term.
  - F2: Scenario 25 recovery now reads "when a grant acks its first RegisterMeeting".
  - F3: "sustained" wording is exact for every `for: 15m` rule.
- operations O1 + observability F2: the SLO completion ratio, and the mc-service.md catalog line, now exclude planned_transfer on both sides.
- observability:
  - F1: MHMeetingsHeldWithoutClients is per instance.
  - F3: the heartbeat faults are called MC contract faults (4 sites).
  - F4: same fix as paired-test F1.
  - F5: crash-rate wording now says "exceeded".
- code-reviewer:
  - F1: uses poll_within, and the bound is computed once.
  - F2: agreed_value moved to metrics.rs and is shared.
  - F3: added MatchOp::as_str.
  - F4: extracted derivation_terms.
  - F5: doc fixes (the STORY_4_PAGES containment wording; the residual stated for the regex case).
- dry-reviewer:
  - F1: one agreement kernel; the term label uses the gauge const.
  - F2: committed_rules is the one YAML kernel.
  - F3: one /api/v1/rules walker.
  - F4: one on-disk walker, in the fixture.
  - F5: poll_within.
  - F6: MatchOp::as_str.
  - F7: worked arithmetic dropped from alerts.md; literal for:/threshold values in runbooks replaced by rule-name + meaning.
- paired-test:
  - F1: APPLY keeps positive matchers beside a negation.
  - F2: the static checks (closed anchored set, source consts, page posture) are Layer-4 unit tests over the real files; the shared consts live in the fixture.
  - F3: the MC budget is read as agreed across MC pods.
  - F4: anchored_for_alerts fails closed on an orphan anchor or a name the YAML parse does not declare.
  - F5: TODO.md:1346 resolved.
  - F6: covered by poll_within.

## Gate 3 fixes (iteration 2)
- dry-reviewer F8: `mc_config_gauge_values` is now the one MC pod lookup behind `mc_config_gauge_max` and `mc_config_gauge_agreed`.
- observability F6: the bridge-window precondition (w >= `initial_registration_bound()` + MC scrape) is asserted by a Layer-4 test beside the formula in mc-service (option (b): option (a) would need the ladder formula copied), and stated in the rule block.
