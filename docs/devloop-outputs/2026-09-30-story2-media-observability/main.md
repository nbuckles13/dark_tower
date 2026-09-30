# Devloop Output: Story-2 media observability (catalog, dashboards, taxonomy, alert inventory, hygiene, guard completeness)

**Date**: 2026-09-30
**Task**: run-story task #16 of `docs/user-stories/2026-09-21-hear-each-other.md` (R-26, R-27, R-28; ADR-0036 §11) — full task text: `~/.cache/devloop/story-runs/story-runner/2026-09-21-hear-each-other/task-16.prompt`
**Specialist**: observability
**Mode**: Agent Teams (v2) — full, Gate-1 present
**Branch**: `feature/hear-each-other`
**Duration**: ~3h (planning ~35m, implementation+review ~1h50m, Gate 2 ~25m)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `339d46343bcc671d28dde0707f7a5a611c5376c8` |
| Branch | `feature/hear-each-other` |
| Lead Model | `claude-opus-5-5[1m]` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` |
| Implementing Specialist | `observability` |
| Tier | `full` |
| Iteration | `1` |
| Security | `security` (paired role: reviews guard rule (1)) |
| Test | `test` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` (paired role: severity + runbook linkage) |
| Semantic Guard | `semantic-guard` |
| Paired Client | `paired-client` |
| Paired Infrastructure | `paired-infrastructure` |

---

## Task Overview

### Objective
See task prompt (linked above). Catalog + dashboards + label taxonomy + alert inventory + hygiene fixtures for story-2 media series; decide operator surface for task-7's six client counters; decide `unwrap_failed` membership in `MCMediaMissingKeyMaterial`; record view on deriving reject-token→alert membership from `frame-v2.vectors.json`; close client-metrics-export guard completeness gaps (rules 1–6).

### Scope
- **Service(s)**: observability docs/dashboards/alert rules (MC, MH, client), dt-guard, env-tests fixtures, sdk-core (possible)
- **Schema**: No
- **Cross-cutting**: Yes

### Debate Decision
NOT NEEDED — decisions are within ADR-0036 §11 / ADR-0031 scope.

---

## Cross-Boundary Classification

No Guarded Shared Area path is touched: `kek_generation_stale` and `unwrap_failed` are already in `proto/test-vectors/frame-v2.vectors.json` (lines 139, 179), so the conditional GSA edit the task names is not needed.

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `docs/observability/metrics/mc-service.md` | Mine | — |
| `docs/observability/metrics/mh-service.md` | Mine | — |
| `docs/observability/metrics/client.md` | Mine | — |
| `docs/observability/label-taxonomy.md` | Mine | — |
| `docs/observability/slos.md` | Mine | — |
| `docs/observability/alerts.md` | Mine | — |
| `docs/observability/alert-conventions.md` | Mine | — |
| `docs/observability/dashboard-conventions.md` | Mine | — |
| `docs/observability/dashboards.md` | Mine | — |
| `infra/grafana/dashboards/mh-media.json` | Mine | — |
| `infra/grafana/dashboards/mc-overview.json` | Mine | — |
| `infra/grafana/dashboards/mc-media.json` | Mine | — |
| `infra/grafana/dashboards/client-media.json` | Mine | — |
| `infra/grafana/dashboards/mh-slos.json` (literal-limit sweep) | Mine | — |
| `infra/grafana/dashboards/mc-slos.json` (literal-limit sweep) | Mine | — |
| `infra/docker/prometheus/rules/mc-alerts.yaml` | Not mine, Minor-judgment | meeting-controller (ADR-0031 rule-file owner), operations |
| `infra/docker/prometheus/rules/mh-alerts.yaml` | Not mine, Minor-judgment | media-handler (ADR-0031 rule-file owner), operations |
| `infra/docker/prometheus/rules/client-alerts.yaml` (new) | Not mine, Domain-judgment | client (paired), operations |
| `infra/docker/prometheus/kustomization.yaml` | Not mine, Mechanical | infrastructure |
| `docs/runbooks/mc-incident-response.md` (Scenario 16 arm 4 + tripwire rungs; Scenario 20 headroom fork) | Not mine, Minor-judgment | operations |
| `docs/user-stories/2026-09-21-hear-each-other.md` (dated correction notes on task 16/17/18 prompts) | Not mine, Minor-judgment | operations (tasks 17/18 prompts), lead |
| `crates/dt-guard/src/client_metrics_export.rs` | Not mine, Domain-judgment | infrastructure (paired, machinery), security (rule 1 + identity content) |
| `crates/dt-guard/src/common/ts_lex.rs` (new) | Not mine, Domain-judgment | infrastructure (paired) |
| `crates/dt-guard/src/common/alert_rule_files.rs` (new; hoisted helpers) | Not mine, Minor-judgment | infrastructure (paired) |
| `crates/dt-guard/src/common/mod.rs` | Not mine, Mechanical | infrastructure |
| `crates/dt-guard/src/alert_rules.rs` (pure move of loadable_rules_files) | Not mine, Minor-judgment | infrastructure (paired) |
| `crates/dt-guard/src/application_metrics.rs` (pure move of the expr walk) | Not mine, Minor-judgment | infrastructure (paired) |
| `crates/dt-guard/tests/client_metrics_export_fixtures.rs` (new) | Not mine, Minor-judgment | infrastructure (paired), test |
| `scripts/guards/simple/client-metrics-export.sh` (header comment) | Not mine, Mechanical | infrastructure |
| `crates/env-tests/src/fixtures/metric_hygiene.rs` | Mine (policy kernel) | test reviews |
| `crates/mc-service/src/observability/metrics.rs` (OUTBOUND_PAYLOAD_KINDS += participant_update_muted, one line) | Not mine, Minor-judgment | meeting-controller |
| `crates/mc-service/src/actors/participant.rs` (two payload_kind consts made pub(crate), so OUTBOUND_PAYLOAD_KINDS is built from them) | Not mine, Minor-judgment | meeting-controller |
| `packages/sdk-core/src/media/setup/mediaMetrics.ts` (comment only: MEDIA_KEK_SOURCES reason corrected) | Not mine, Mechanical | client (paired) |
| `packages/sdk-core/src/signaling/SignalingClient.ts` (comment only: meetingKekUpdate is rotation-only) | Not mine, Mechanical | client (paired) |
| `packages/sdk-core/src/signaling/__tests__/mediaSignaling.test.ts` (comment only) | Not mine, Mechanical | client (paired) |
| `packages/sdk-core/src/media/setup/kekSource.ts` (comment only) | Not mine, Mechanical | client (paired) |
| `packages/web-app/tests/clientMetricNames.test.ts` (header comment only) | Not mine, Mechanical | client (paired) |
| `infra/services/otel-collector/collector.yaml` (comment: cite #emitMetric) | Not mine, Mechanical | infrastructure |
| `infra/services/mh-service/config.env` (comment only) | Not mine, Mechanical | media-handler / operations |
| `docs/decisions/adr-0031-service-owned-dashboards-alerts.md` (dated amendment: named review obligation) | Mine (observability co-owns ADR-0031 guidance) | operations reviews |
| `docs/TODO.md` | Mine | — |
| `docs/devloop-outputs/2026-09-30-story2-media-observability/main.md` | Mine | — |
---

## Planning

### Mechanism restatement (the wider class)

This task describes separate instances. They are one defect class: **an encoding that decides whether an operator sees a signal is hand-maintained apart from the thing it describes, and nothing fails when the two diverge.** The failure is always absence (a stripped label, a dropped name, a selector that matches nothing, a panel description that asserts retired behaviour), and absence looks the same as health. Instances in scope:
- **Code ↔ pipeline:** SDK label keys vs GC tier vs collector `keep_keys` (rules 1 and 3). Names emitted outside `mediaMetrics.ts` vs the export decision (rule 4).
- **Alert ↔ pipeline:** a `dt_client_*` alert selecting a name or label the pipeline drops (rule 2). `MCMediaMissingKeyMaterial` sat dead for exactly this reason.
- **Vocabulary ↔ alert:** a new frame reject token that silently falls outside every key-delivery selector (the drift shape seen four times in task 7). The view I am recording on vectors-derived membership is below. I propose a **partition** check, not derivation.
- **Guard ↔ its own inputs:** hardcoded counts (`< 12`, `< 5`, already stale: EXTRA is 6), silent `is_file()` skips, and the `no-artifacts` OK (rules 5 and 6).
- **Prose ↔ behaviour:** panel and rule text asserting retired loopback behaviour, restated limits, stale "task 16 owes" and "nothing covers a typo" claims. Nothing mechanical catches prose, so this becomes a named ADR-0031 review obligation plus a sweep in this edit.

Complete-the-invariant: every instance above that I found is fixed here. Nothing is left partial.

### Premise corrections (verified against 339d4634)
1. `docs/observability/metric-catalog.md` does not exist. Catalogs are per-service under `docs/observability/metrics/`, and `client.md` is the guard's SSoT. Entries go there. No aggregate file is created.
2. `kek_generation_stale` is already in the selector (mc-alerts.yaml:636) and in `frame-v2.vectors.json` (layer=key, drops_frame=true). No GSA edit is needed. The only open selector question is `unwrap_failed`.
3. `rebind` is **not** zero-forever (paired-client A1). It has three causes: an MC defect, cache poisoning, and a lost `ParticipantLeft` followed by an R-16 reissue. The Group A rule is written for all three.
4. `unwrap_failed` cannot come from a corrupted frame, because the signature covers the wrap block and transit corruption becomes `signature_invalid` (paired-client A2). Its reachable causes are: (a) a KEK-bytes split under one generation (key distribution), (b) a sender wrap or key-schedule bug, (c) an authenticated member emitting bad wraps.
5. The client-media "not wired up yet" banner is already gone. The residue is in the panel 3 and 8 descriptions ("export hop is unwired"). Those get swept.
6. The shared asserted-name constant already exists (`packages/web-app/e2e/clientMetricNames.ts`, task 15). I extend it and create no second home.
7. The TODO TS-aware-guard entry already has a COST RAISED paragraph, and the "cheap test" exists (`joinLabelSpread.test.ts` + `meeting-session.test.ts`). I edit the entry to cite both tests and the static scan's residual. No duplicate sentence and no third check.
8. Alert ownership. Story line 166 carries the documented one-low task numbering. "Headroom warning and inventory (task 15)" means this task, and "`MHMediaEgressBudgetExhausted` (task 17)" means task 18 (task 18's prompt lists it as its own alert, with MH 18 as runbook, and that runbook does not exist yet). So: **I author `MHMediaEgressEdgeHeadroomLow`**. For `MHMediaEgressBudgetExhausted` I record a **pending** inventory entry with no PromQL heading and its binding constraints (warning, 10m, `mh_media_stream_admission_rejection_ratio > mh_media_stream_admission_rejection_ratio_threshold`, no literal, MH 18, owner task 18), per @operations (a heading for a non-existent rule fails `inventory_expr_drift` open). TODO:748 ("alert is task 18's") gets corrected.
9. `alerts.md:640` says task 16 owes ten MC inventory entries. I take all ten, plus the new ones.

### A. Catalog (per-service files)
- **mc-service.md.** Complete or add entries for:
  - `mc_meeting_kek_generated_total{trigger}`
  - `mc_meeting_kek_pushes_total{outcome}` (per recipient; the small-denominator detector)
  - `mc_meeting_kek_rotation_leaves_coalesced`
  - the pending-age, window and overdue-threshold gauges
  - `mc_meeting_kek_rotation_duration_seconds`
  - the server-mute / unmute / suppressed-reply counters
  - the receive-slot-cap gauge
  - `mc_meeting_sender_ids_issued_max`
  - `payload_kind` on `mc_participant_outbound_messages_dropped_total`

  The research agent is confirming which of these already exist. Existing entries are completed, not duplicated.
- **mh-service.md.** Budget, stream ceiling, admission counter, rejection ratio + `_threshold`, `mh_media_egress_stream_ceiling_recommended_min`, edges + `_limit`, registered meetings + `_limit`, teardown counter, and the `server_muted` drop reason.
  - `_recommended_min` is advisory with **no alert**. It is the fourth deliberate-absence inventory entry, and it anchors the new taxonomy suffix rule.
  - The R-28 accounting identity is restated there: one accepted ingress frame yields E egress attempts, so egress forwards are not comparable to ingress forwards.
  - Fix the mh-service.md:835 loopback row.
- **client.md.**
  - Receive-source-deficit and kek-retention-violations are counters, not gauges: N identical-identity browsers make a gauge last-writer-wins. That collision is the §11 corollary, recorded as "it revealed the metrics were shaped wrong".
  - Capture-source gauge: the signal is the *presence* of the `mode="test_tone"` series, never the value.
  - `kek_update` source value, and the deliberate **two-value** KEK source vocabulary (reconnect re-issue and rotation are indistinguishable at the client; the split lives on MC's `trigger`).
  - Split key-generation drop reasons.
  - **No retention gauge exists.** Retention is `min(W/2, KEK_RETENTION_CEILING_MS)` derived at the client, with the floor substituted when W is absent. I will cite the code locations: `clientConfig.ts:deriveKekRetention`, the wire field `kek_rotation_debounce_seconds`, `MC_KEK_ROTATION_DEBOUNCE_SECONDS` in `crates/mc-service/src/config.rs`, and the `mc_meeting_kek_rotation_window_seconds` gauge. All four are in seconds, with no conversion anywhere, and I will verify each rather than restate it.
  - `Exported: no` marker for `dt_client_mh_connection_total` and the other join-flow names that rule 4 will now force a decision on.
  - Per-client identity `received = accepted + sum(drops)` holds for N senders with no sender split. `decode_queue_dropped` is post-accept and never enters it.
  - Retention-violation counter recorded as a **zero-forever tripwire against a future refactor**, not a live-condition detector (the invariant is unit-tested).
  - `unwrap_failed` is reclassified in line with the decision below.
- **label-taxonomy.md.**
  - New label values: `trigger`, push `outcome`s, `payload_kind`, `server_muted`, `mode`, the `kek_update` source, admission outcomes.
  - The rule, stated once: no media-path series carries a meeting, participant, sender, key, slot or stream identity, and every media-path series carries `key_custody=operator`.
  - On a client series, `instance` names the collector pod.
  - Suffix rule: `_threshold` only for values something enforces or alerts on. Advisory values take a descriptive suffix, because the threshold spelling tells a responder that something automatic is watching.
- **slos.md** Open items: rotation latency, explicitly open (no number ratified; story 8).

### B. Dashboards
- **mh-media.json.**
  - New panels: edges vs `mh_media_egress_edges_limit`, registered meetings vs `_limit`, budget, admission by outcome plus rejection ratio vs `_threshold` gauge.
  - Redefine "Egress Delivery Ratio" as egress-internal, `forwarded{egress} / (forwarded{egress} + drops{egress})`. Title and description change, not just the expr.
  - New "Mean Fan-out" panel: `egress forwards / ingress forwards` (rates), denominator guarded `> 0`, expected value = mean subscriber count.
  - Rewrite the "EXPECTED HEALTHY STATE OF THE LOOPBACK" descriptions (~203, ~296, ~456): a solo participant now produces a climbing `no_subscriber` series, and that is healthy. This must stay consistent with the runbook text owned by tasks 17 and 18.
- **mc-overview.json.**
  - Rotation panel (rate by trigger; pending age vs W gauge vs overdue-threshold gauge).
  - KEK issuance panel split `by (trigger)`.
  - Server-mute panel beside client mute.
  - Slot fill ratio read beside the published cap.
- **client-media.json.** Sweep the panel 3 and 8 residue. Panels for all six task-7 counters:
  - **Group A (panel + alert):** `kek_retention_violations_total`; `kek_install_refusals_total{outcome="conflicting_key"}` (sole-observable witness); `roster_key_rebinds_total{outcome="rebind"}` per arm.
  - **Group B (panel only):** the rest of `kek_retention_anomalies_total`; `kek_install_refusals_total`'s other two arms; `decode_queue_dropped_total`; and `kek_generations_retained_total` **on one panel against** `kek_updates_total{source="kek_update"}` (rotations rising while retentions stay flat is the signal).
  - Annotate `floor_substituted` and `ceiling_clamped` as expected-non-zero configuration or rollback states that are NEVER alert inputs. The `ceiling_clamped` text carries "the default W already sits at the ceiling".
  - Capture-mode description states that the value is not a browser count.
- **Literal-limit sweep.** Across mh-media, mc-overview, mc-media and client-media (and mh/mc-slos if hit), every description or annotation that restates a limit as a number is rewritten to name the gauge that publishes it.
- **dashboard-conventions.md.** "Publish the value enforcement reads" and "cite, don't restate" become one written rule.
- **dashboards.md.** Updated to match.

### C. Alerts
**`MCMediaMissingKeyMaterial` (mc-alerts.yaml + alerts.md, byte-identical expr).**
- **Decision: INCLUDE `unwrap_failed`** (security S6, paired-client A2). Cause (a) is key distribution. Causes (b) and (c) are sender bugs whose user symptom is the same "cannot open", and no other rule selects them. Recorded in the rule comment and client.md. `kekSource.ts` ~433-441 is updated either way.
- **Re-derivation, recorded in the rule comment and inventory.** The healthy steady state is a duty cycle.
  - Each rotation (at most one per W per meeting, W ≥ 30 s from `MIN_KEK_ROTATION_DEBOUNCE_SECONDS`) costs `no_kek_for_generation` for the KEK push skew between members.
  - `kek_generation_stale` is ≈0 while retention (≥ 10 s floor) far exceeds that skew.
  - `no_roster_entry` is join-only.
  - `unwrap_failed` has **no** healthy transient: with a usable cached key the same mismatch becomes the non-dropping `wrap_key_id_mismatch`.
  - Healthy ratio ≈ skew/W. Reaching 5% at the W floor needs ≥ 1.5 s of skew on *every* rotation for 15 min (one leave every 30 s, which `MCKekRotationStorm` owns). ~25× margin holds if skew ≤ ~60 ms. **That is an assumption, stated as unmeasured, not a ratified number.**
  - The threshold stays at 5% / `for: 15m`.
- **Compensating control named.** The alert is a fleet-wide ratio: one client in a hundred with a failing push reads 1% forever. MC's per-recipient `mc_meeting_kek_pushes_total{outcome}` (`MCKekPushFailureRate`) is the small-denominator detector, and `conflicting_key` is the small-denominator witness for the unwrap split.
- **Stale text removed.** The "re-deriving is task 16's / WHICH WAY IT LEANS UNTIL THEN" and "NOTHING ELSE COVERS A TYPO ... dt_client_ matches nothing" paragraphs are rewritten, because rule 2 now covers name and label liveness.
- `clientMetricNames.ts`: the name set is unchanged (the reason alternation grows). I will check that `tests/clientMetricNames.test.ts` pins the reasons ⊆ `ALL_REJECT_REASONS` and stays green, and fix its "guard does not check alert → emitter" header.

**Alert thresholds (ADR-0031 plan-template block).** Rules marked NEW are authored here. The others are inventory entries only.

| Metric | Condition | For | Severity | Runbook |
|--------|-----------|-----|----------|---------|
| dt_client_media_frames_dropped_total / received (MCMediaMissingKeyMaterial, MODIFIED) | `(sum(rate(dt_client_media_frames_dropped_total{reason=~"no_kek_for_generation\|kek_generation_stale\|no_roster_entry\|unwrap_failed"}[5m])) / sum(rate(dt_client_media_frames_received_total[5m]))) > 0.05 and sum(rate(...received...[5m])) > 0` | 15m | warning | docs/runbooks/mc-incident-response.md#scenario-16-missing-key-material |
| dt_client_media_kek_install_refusals_total (NEW `MCClientKekConflictingKey`, mc-alerts.yaml) | `sum(increase(dt_client_media_kek_install_refusals_total{outcome="conflicting_key"}[15m])) > 0` | 0m* | warning | mc-incident-response.md Scenario 16, new rung anchor |
| dt_client_media_roster_key_rebinds_total (NEW `MCClientRosterKeyRebind`, mc-alerts.yaml) | `sum(increase(dt_client_media_roster_key_rebinds_total{outcome="rebind"}[15m])) > 0` | 0m* | warning | Scenario 16, new rung anchor |
| dt_client_media_kek_retention_violations_total (NEW `ClientKekRetentionViolation`, new client-alerts.yaml) | `sum(increase(dt_client_media_kek_retention_violations_total[15m])) > 0` | 0m* | warning | Scenario 16, new rung anchor (remedy: SDK rollback) |
| mh_media_egress_edges vs _limit (NEW `MHMediaEgressEdgeHeadroomLow`, mh-alerts.yaml) | `mh_media_egress_edges / mh_media_egress_edges_limit > 0.8` (headroom fraction owned by the alert; no config value restated) | 10m | warning | docs/runbooks/mc-incident-response.md#scenario-20-meeting-teardown-failing--mh-budget-ratchet |
| MCKekRotationOverdue / MCKekPushFailureRate / MCKekRotationStorm / sender-id info rule / six task-12 warnings | as in mc-alerts.yaml (inventory only, byte-identical) | — | as authored | as authored |
| MHMediaEgressBudgetExhausted | PENDING, task 18, inventory placeholder with no PromQL heading | 10m | warning | MH 18 (task 18) |

\*`for: 0m` follows the `MCEndMeetingOwnershipRejected` zero-forever-tripwire precedent. If the ADR-0031 `for ≥ 30s` guard applies, I switch to the guard's minimum. "Ticket tier" maps to `severity: warning`, recorded once in alert-conventions.md so nobody invents `severity: ticket`.

Headroom: if the research agent finds a published headroom-fraction gauge, the rule becomes bare gauge-vs-gauge. Otherwise the ratio uses the alert-owned fraction shown above, and that fraction is not a second copy of any config value.

**Group A rule comments** carry:
- every known legitimate non-zero cause. `rebind`: MC defect, cache poisoning, lost `ParticipantLeft` + reissue, plus the `mc_participant_outbound_messages_dropped_total{payload_kind="participant_update_left"}` correlator, which supports and never confirms. `conflicting_key`: a genuine MC double-issue or KEK split, with the reconnect/stale-tab cases confirmed with paired-client. `retention_violation`: only an SDK regression; the unit test holds the invariant.
- "a new cause is added here; the rule is never retired or silenced for it".
- series-absent semantics: the counter is absent until a browser increments, so `> 0` cannot false-fire and also cannot prove the pipe is alive. The pipe-liveness read-back and `up{job="otel-collector"}` cover that.

Placement: `conflicting_key` and `rebind` go in mc-alerts.yaml because the remedy is MC's KEK and roster delivery (the `MCMediaMissingKeyMaterial` precedent). Retention violation goes in a new `client-alerts.yaml` because its remedy is an SDK regression. It is added to the kustomize `files:` list. The `rule_files` glob and env-test 33 pick it up through the existing predicate.

**Demonstration (ADR-0036 §11, both halves), one row per gate, in each ADR-0031 block:**

| Gate | Fires (inject) | Applies (live selector matches a real series) |
|---|---|---|
| MCKekRotationOverdue | stall the rotation timer; pending-age gauge crosses the overdue-threshold gauge | the gauges exist on a running MC with identical label sets |
| Server mute (no alert, deliberate) | apply a mute; watch **MH's** `server_muted` drop reason move, not just the MC counter | env-test S3 + MH 19 |
| MHMediaEgressEdgeHeadroomLow | lower `MH_MAX_TOTAL_EGRESS_EDGES`; the ratio crosses | the selector matches a real mh container's series |
| Teardown (registered meetings) | end a meeting; `mh_media_registered_meetings` **falls** | the teardown counter increments in the same window |
| Client pipe / MCMediaMissingKeyMaterial / Group A | unit + fixture (guard rule 2 proves names and labels survive the pipe) | a client series name read back from Prometheus by a real browser (task 15's R-27 read-back via the shared constant) |

**Inventory (alerts.md):**
- the ten owed MC entries, the four new rules, the `MCKekRotationStorm` / push / overdue entries
- `MCSenderIdSpaceExhausted` retirement: what a responder remembering the old alert would wrongly do (end a self-repairing meeting), a pointer to the info rule, and the rules-self-roll vs image coupling
- deliberate-absence section: server mute (healthy moderation; the inverse needs a per-meeting correlation that §11 bars; covered by env-test S3 and MH 19; cites the mc-alerts comment rather than restating it), plus `_recommended_min` as the fourth entry
- the pending `MHMediaEgressBudgetExhausted` entry
- the "task 16 owes" paragraphs, closed

**Runbook.** Additive Scenario 16 sub-anchors for the three Group A rules, drafted by me and sent to @operations to confirm or rewrite. Nothing in the task-17 or task-18 territory is touched.

### D. Hygiene kernel (`metric_hygiene.rs`)
- New `Rule::MediaPathIdentity`. It fires when a key on a media-path series (name prefix `mh_media_`, `mc_media_`, `mc_meeting_kek_`, `dt_client_media_`) contains a participant, sender, slot, stream or key-id segment, with `key_custody` as the single named exemption.
- New `Rule::MissingKeyCustody`: a media-path series without `key_custody="operator"`.
- The media-path prefix set is a named const citing label-taxonomy.md.
- FIRE fixtures per rule (e.g. `sender_id`, `slot_index`, `stream_id`, `participant`; missing and wrong-valued `key_custody`) plus a clean fixture over real-shaped story-2 series (`mc_meeting_kek_pushes_total{outcome,key_custody}`, `mh_media_egress_edges{key_custody}`, `dt_client_media_capture_source{mode,...}`).
- **Pre-check before wiring it live:** every story-2 media series must actually carry `key_custody`. If one does not, that is a real finding, fixed at source or escalated, never exempted.
- Tell @test whether 32's name list is derived. It selects by job, so it is not a hand list.

### E. Guard: `client_metrics_export.rs` (machinery with @paired-infrastructure, content with @security)
- **Refactor.** `collect_findings(root) -> Vec<Finding>`, so tests assert `rule_id`, not `is_err()`. **One** comment-stripping sdk-core walker returning `(path, src)` feeds G2, G3, rule 1 and G6. The loadable-rules predicate and the parsed-`expr` walk are hoisted and reused from `alert_rules.rs` / `application_metrics.rs` (serde, not raw lines, so comment mentions are ignored). G5 reuses them. `common/duration.rs` replaces `unit_seconds` / `DURATION_RE`.
- **Rule 1** `label_key_not_forwarded`.
  - Scope: every emission of an **`Exported: yes`** name. Its resolved label keys must be ⊆ (ALLOWLIST ∪ MEDIA_DATAPOINT_EXTRA) ∩ keep_keys.
  - Non-exported names are out of scope: the name filter drops the whole series, and applying the rule to them would demand `meeting_id_hash` in keep_keys, which is the §11 breach. The reason is stated in the doc comment so nobody "fixes" a finding by widening keep_keys.
  - The pre-existing join-flow mismatch (SDK bare `failure_stage` / `close_reason` / `mh_index_bucket` / `status` vs GC's dotted spellings) is harmless while unexported, and rule 1 catches it the day any of them is exported. Recorded in client.md.
  - Extractor: anchor on the dt_client literal as the first argument of `.counter|gauge|histogram(`, then a balanced-brace scan of arg 2. It reads `key:` pairs, quoted keys and **shorthand** properties, and resolves `...this.#base` / bare `this.#base` from `mediaMetricLabels`' return literal (not hardcoded).
  - **Fail closed** with a distinct `unresolved_label_bag` token when an in-scope site has an unresolvable spread, an identifier bag, a computed key, or a non-literal or interpolated name.
  - Positive controls: the resolved `#base` set is non-empty and contains `key_custody`, and every exported name G3 sees emitted has ≥1 parsed call site (otherwise `extractor_empty_input`).
- **Identity counter-control** (security S3) `identity_key_forwarded`. No key in keep_keys or MEDIA_DATAPOINT_EXTRA may have a `_`/`.`-segment in {meeting, participant, sender, user, session, slot, stream, id, hash}, or equal `meeting_id_hash`. `key_custody` is the single explicit exemption constant. The list is a small named const tied to label-taxonomy.md.
- **Rule 2** `dead_alert_reference`.
  - Every `dt_client_*` name in a loaded rule `expr` must be `Exported: yes`.
  - Every label matched in a dt_client selector, and every `by/without/on/ignoring/group_left/group_right` label in an expr touching dt_client, must be ∈ keep_keys ∪ `EXPORTER_SYNTHESISED_LABELS` (`job`, `instance`, `le`, `otel_scope_name`, `otel_scope_version`, `__name__`). That set is a named policy const, never solved by widening keep_keys.
  - `by(instance)` on a client alert is flagged (`instance` = collector pod).
  - Positive control: the loaded dt_client references are non-empty.
- **Rule 3** `forwarded_key_not_kept`: `MEDIA_DATAPOINT_EXTRA ⊆ keep_keys`. The base allowlist is excluded, because it carries trace-only keys.
- **Rule 4.** G2 uses the same walker as G3, across all non-test sdk-core. `dt_client_mh_connection_total` and the MeetingSession names get catalog markers.
- **Rule 5.** Remove the `(false,false)` no-artifacts OK arm (invert the existing test, keep it). Every `is_file` / `is_dir` skip (GC_FILTER, SDK_SRC, MH_METRICS_RS, the three G6 TS files, ALERTS_SUBDIR) becomes `extractor_empty_input`, with a test per input.
- **Rule 6.**
  - The count check becomes "parsed length == declared `[&str; N]`" (compile-checked, like G6's `ALL`) plus named-key positive controls (`org_id` in ALLOWLIST, `reason` in EXTRA).
  - `KEY_CUSTODY_LABEL` is resolved from `crates/common/src/observability/labels.rs`, or fails loud, instead of the hand insert.
  - The TS extractors accept `'…'`, `"…"` and interpolation-free backticks. A backtick with `${` fails closed.
  - Comments are stripped first, so double-quoted prose cannot start matching.
- **G7 (proposed; the vectors view).** I **reject deriving** alert membership from `layer`, because `layer` names the processing stage, not the remedy: `unwrap_failed` is `layer=crypto` yet belongs to key delivery. That is the concrete counter-example. The hand-maintenance defect is fixed by a **partition** check instead. Every `drops_frame: true` token in `frame-v2.vectors.json` must appear in exactly one of `MCMediaMissingKeyMaterial`'s parsed reason alternation or a named `NOT_KEY_DELIVERY` const in the guard (each entry with a one-line reason: `sender_not_assigned` = misrouting, `signature_invalid`, `decrypt_failed`, `replay_detected`, parse-layer tokens, …). A new token then forces a decision and cannot silently fall outside. Positive control: vectors are parsed with ≥1 drops_frame token. **If reviewers judge G7 beyond scope, I record the view only.**
- Every new rule: a distinct `rule_id` const, a fixture that FIRES (asserting the rule_id), a CLEAN variant, and a distinct empty-input test. Fixtures are a tempdir tree driven through `collect_findings`, in `crates/dt-guard/tests/client_metrics_export_fixtures.rs`. **The real tree must be green** after the catalog edits (T6).

### F. Other
- **ADR-0031** gets a dated amendment under §Review gate timing. Named REVIEW obligation, not a guard: "when a requirement retires a behaviour, grep runbooks, dashboards and rule comments for text asserting it". Story 2 falsified five pieces of triage text.
- **TODO.md.**
  - TS-guard entry: cite the existing tests and the residual; the re-derived-priority sentence already exists, so extend it rather than duplicate it.
  - TODO:748: headroom alert landed here.
  - TODO:581 is untouched (task 18).
- **config.env.** Comment fixes on "`MHMediaEgressEdgeHeadroomLow` lands later".

### Validation
`./scripts/layer-fast.sh` green, including `client-metrics-export` on the real tree, alert-rules-policy (`inventory_expr_drift`), dashboard guards, `cargo test -p dt-guard -p env-tests` (kernel fixtures), and the web-app vitest `clientMetricNames.test.ts`.

### Gate-1 revisions (agreed with the panel)
- **Group A shape (observability and operations, blocking):**
  - Presence form, `sum(X{sel}) > 0`, `for: 1m`. The lazy series appears at 1, so `increase()` never sees the first event.
  - The alert clears when the collector's exporter `metric_expiration` drops the series (cited by key, never as a number). While it fires, a second occurrence is visible only in the raw counter.
  - Idle-tab export behaviour: to be verified with paired-client and stated.
  - Execution evidence: no promtool or PromQL lane exists, so a dt-guard **shape** check (`tripwire_rate_wrapped`) forbids `increase()`/`rate()` around the enumerated zero-forever client counters. It has FIRE fixtures for increase, rate and `sum by(...)(increase(...))`, and a distinct `tripwire_list_stale` finding if an enumerated name is in no loaded rule. The appear-at-1 behavioural claim is recorded as **unverified by execution**, and the live browser demo is named.
- **Identity counter-control (security owns content):**
  - Deny segments {meeting, participant, sender, user, session, slot, stream, key, kek, id, hash}. Long nouns match by containment; short tokens match by `_`/`.`/camelCase segment.
  - Named exemptions {key_custody, org_id}, each with a reason.
  - **SSoT:** a fenced block in label-taxonomy.md. dt-guard and env-tests each parse it (positive control: non-empty, contains `participant`) and assert const equality.
  - The kernel adds a separate enumerated stored-series-only service-instance exemption list.
- **Rule 1 (security and paired-client):**
  - Any call with a dt_client literal as its first argument counts as an emission.
  - `DECLARED_WRAPPERS` (MeetingSession `#counter`/`#histogram` → `#metricLabels`, resolved from source).
  - An undeclared non-literal sink call is `unresolved_label_bag`, whether or not the name is exported.
  - Every occurrence of an exported-name literal must be a parsed first argument.
  - Sink implementation files are the only exemption.
- **G7 kept in scope** (security, observability, test and code-reviewer; the lead said keep if the panel agrees). paired-infrastructure's scope objection was noted and withdrawn on the panel call.
  - A true partition in both directions: tokens in the alternation must be `drops_frame: true`.
  - The vectors file is parsed by typed serde.
  - Positive control: `no_kek_for_generation` must be in the parsed alternation.
  - A stale NOT_KEY_DELIVERY entry fails.
  - The mc-alerts comment cites the const.
- **Machinery (infrastructure and code-reviewer):**
  - `common/ts_lex.rs`, a string-preserving TS comment stripper.
  - The `${` fail-closed applies only in metric-name position.
  - `instance` is allowed as a matcher and flagged in grouping (`client_alert_groups_by_instance`).
  - Helpers hoisted into `common/`.
  - `collect_findings -> Result<Vec<Finding>>`, with an `Inputs` struct and a `check_*` function per rule.
- **Cite, don't restate** covers dashboard text AND the annotations of the media rules touched. Carve-out: provenance comments may show a number only when it names its source const or gauge.
- **Media-path series** is defined once in label-taxonomy.md, and every story-2 series is classified in or out.
- **Headroom:** the ADR-0031 block records the alert-owned fraction's provenance, the absence of any headroom gauge, the label-identity caveat and the denominator validation. The description forks LEAK vs LOAD first. The demo lowers `MH_MAX_TOTAL_EGRESS_EDGES`.
- **Validation** adds `pnpm --filter @darktower/sdk-core test` and `pnpm --filter @darktower/web-app test`, run explicitly if layer-fast does not cover them.

---

## Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Semantic Guard | confirmed |
| Paired Client | confirmed |
| Paired Infrastructure | confirmed |

---

## Implementation Summary

**Guard (`dt-guard client-metrics-export`).** The guard was restructured into an `Inputs` struct, `collect_findings() -> Result<Vec<Finding>>` and one `check_*` function per rule. New rule ids:

| Rule id | Checks |
|---|---|
| `label_key_not_forwarded` | Rule 1. Scoped to `Exported: yes` names. |
| `unresolved_label_bag` | Rule 1's fail-closed path. `DECLARED_WRAPPERS` are resolved from source. Also covers undeclared non-literal sink calls, stray exported-name literals, and interpolated names. |
| `dead_alert_reference` | Rule 2. |
| `client_alert_groups_by_instance` | Rule 2, the `instance` grouping case. |
| `forwarded_key_not_kept` | Rule 3. |
| `identity_key_forwarded` | Reads the label-taxonomy §R4 fenced block at runtime. |
| `tripwire_rate_wrapped` | Shape guard for the zero-forever tripwires. |
| `tripwire_list_stale` | Positive control for that shape guard. |
| `reject_token_unclassified` | G7 partition over `frame-v2.vectors.json`. |
| `reject_token_misclassified` | G7 partition, the other direction. |

- Rule 4: G2 now walks all non-test sdk-core source with ONE comment-stripping walker (`common/ts_lex.rs`).
- Rule 5: the `no-artifacts` OK path is gone, and every missing input reports `extractor_empty_input`.
- Rule 6: declared `[&str; N]` lengths plus named-key positive controls. `KEY_CUSTODY_LABEL` is resolved from `common/labels.rs`, and all three TypeScript quote styles are accepted.
- The helpers were hoisted into `common/alert_rule_files.rs` as a pure move.
- 76 fixture tests are in `crates/dt-guard/tests/client_metrics_export_fixtures.rs`. The real tree is clean.

**Hygiene kernel.**
- New rules: `Rule::MediaPathIdentity` and `Rule::MissingKeyCustody`, over `MEDIA_PATH_PREFIXES` (defined in label-taxonomy §R4).
- The identity vocabulary consts are pinned to the taxonomy's fenced block by `identity_vocabulary_matches_the_taxonomy_block`, which has a positive control.
- `key_custody_value_matches_common` pins the restated value.
- There are FIRE fixtures, plus a no-false-positive fixture over real story-2 label shapes.
- Test 32 selects by job regex, not a hand list, so the new rules apply live automatically.

**Alerts.**
- `MCMediaMissingKeyMaterial`: `unwrap_failed` added to the selector. Threshold re-derived and unchanged at 5%/15m. The stale "task 16" and "nothing covers a typo" paragraphs are rewritten.
- New rules:

  | Rule | File | Condition |
  |---|---|---|
  | `MCClientKekConflictingKey` | `mc-alerts.yaml` | presence, `for: 1m` |
  | `MCClientRosterKeyRebind` | `mc-alerts.yaml` | presence, `for: 1m` |
  | `ClientKekRetentionViolation` | new `client-alerts.yaml`, plus a kustomization line | presence, `for: 1m` |
  | `MHMediaEgressEdgeHeadroomLow` | `mh-alerts.yaml` | edges / **stream ceiling** > alert-owned 0.8, `for: 10m` |

- Headroom denominator: the ceiling binds first, and MH refuses to start with the edge limit below it.
- `alerts.md` now inventories all ten owed MC entries plus the new rules, byte-identical to the rule files (42 pairs).
- `alerts.md` also carries a pending `MHMediaEgressBudgetExhausted` entry (no `####` heading), and deliberate-absence sections: server mute, and `_recommended_min` as the fourth entry.

**Dashboards.**
- mh-media:
  - Egress Delivery Ratio is now per-edge and egress-internal.
  - New Mean Fan-out panel, with the denominator guarded.
  - New edge-limit backstop panel.
  - The headroom alert's comparison is on the existing Installed Streams vs Ceiling panel.
  - Loopback-era text rewritten.
- mc-media / mc-overview: KEK-by-trigger, pending age and server mute already existed. Server mute moved beside client mute, and a slot fill vs cap panel was added.
- client-media: panel 3 and 8 residue swept. Group A (raw sums) and Group B panels added, including KEK Rotations vs Generations Retained.
- Literal-limit sweep across six boards.

**Docs.**
- Catalogs completed (per-service; `metric-catalog.md` does not exist).
- label-taxonomy §R4: the rule, the media-path definition, the identity policy block, the `instance` note, the suffix rule, and the new label values.
- slos.md: rotation latency listed as OPEN.
- dashboard-conventions: publish-then-cite is written as one rule, with a carve-out.
- alert-conventions: ticket tier is `warning`; `Client` / `MCClient` naming; presence shape for lazy counters.
- ADR-0031: named retired-behaviour prose-sweep obligation.
- TODO: TS-guard entry narrowed; headroom entry closed; the unwrap_failed reconnect entry updated.
- Runbook MC 16: Arm 4 (`unwrap_failed`) and the three tripwire rungs. MC 20: headroom LEAK/LOAD fork.

**Premise corrections (verified against code).**
1. `metric-catalog.md` does not exist.
2. `kek_generation_stale` and `unwrap_failed` were already in the vectors, so there is no GSA edit.
3. `rebind` is not zero-forever.
4. `unwrap_failed` cannot come from transit corruption.
5. **A solo participant sends nothing** (MC sends `EmittedEmptyTargets`, and `egress.ts` builds no frame on an empty set). So `no_subscriber` is NOT the healthy solo state. Correction notes are on task 16's and task 17's prompts.
6. **Reconnect re-issue is NOT a `MeetingKekUpdate`** (paired-client, verified: the JoinResponse-shaped reconnect result carries it, so it counts as `join_response`). The two-value `source` vocabulary stands; its reason is corrected in the catalogs, the SDK comments and the story file.
7. **The headroom demo lever is the budget, not the edge limit**, which MH refuses to start below the ceiling. It is Kind-only.
8. **Headroom leads the stream-ceiling arm of budget exhaustion**, so there is a scoped correction note on task 18's prompt.

**Fixed out-of-scope on the lead's instruction:** `OUTBOUND_PAYLOAD_KINDS` now includes `participant_update_muted` (MC, one line).

**Left with reason.**
- The mc-overview SLO reference lines (`vector(..)`) are on non-media panels with no gauge, so the cite rule does not reach them.
- MH 17's loopback subsection is task 17's named deliverable, and it now inherits the corrected premise.

**Evidence promised at planning (test P5/P6, observability's question).**
- **key_custody pre-check, STATIC, not live.** Layer 7 is the first time `Rule::MediaPathIdentity` and `Rule::MissingKeyCustody` meet real stored series.
  - Every `mc_media_*`, `mc_meeting_kek_*`, `mc_meeting_sender_ids_*` and `mh_media_*` emission macro in `crates/{mc,mh}-service/src/observability/metrics.rs` passes `KEY_CUSTODY_LABEL => KEY_CUSTODY_OPERATOR`. Checked by grepping each name's macro block. The only apparent miss, `mh_media_forward_latency`, is a `Matcher::Prefix` bucket config, not an emission. Test's independent count was 60 of 60.
  - The client media names, including `dt_client_time_to_first_media_frame_ms`, are all emitted with `this.#base`. The guard's positive control proves that bag resolves to `{client_version, org_id, key_custody}`.
  - The Prometheus scrape adds only `job` and `instance` (no target relabels in `infra/kubernetes/observability/prometheus.yml`), and the no-false-positive fixture covers both.
  - A label found live that the fixtures did not anticipate is a finding to fix at source, never an exemption.
- **`client-alerts.yaml` is loaded.**
  - It is in the `prometheus-rules` configMapGenerator and matches both `rule_files` globs.
  - `dt-guard alert-rules-policy` fails on any difference between those sets, and it is green.
  - `crates/env-tests/tests/33_alert_rules_loaded.rs::every_on_disk_alert_rule_is_loaded_by_prometheus` derives its expected set from the shared loadable-files predicate, so `ClientKekRetentionViolation` goes red at Layer 7 if the file is not mounted.

**Validation.**
- `layer-fast.sh`. The first run failed only on `validate-cross-boundary-scope`: table rows were not one path per row. Fixed, and the guard is now OK.
- Layer 4 (cargo test and nx test) passed.

---

## Code Review Results

### Gate 3 verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security (paired) | RESOLVED-FIXED | 3 | 3 | 0 | rule-1 label-bag resolution fail-open ×2 (`#base` assignment, `#metricLabels` writes); matcher `-` split drift. Residual (documented, not deferred): alias/bracket sink access needs deliberate obfuscation |
| Test | RESOLVED-FIXED | 3 | 3 | 0 | identity matcher drift; `MEDIA_PATH_PREFIXES` SSoT block; key_custody pre-check + client-alerts.yaml load proof recorded |
| Observability | RESOLVED-FIXED | 6 | 6 | 0 | G5 positive control; `add_metric_suffixes` check; rebind triage rule-out (×2); receive-path crypto deliberate-absence record; panel 17 split |
| Code Quality | RESOLVED-FIXED | 10 | 10 | 0 | covered MC one-liner (OUTBOUND_PAYLOAD_KINDS now built from consts + test); function splits; renames; dead code |
| DRY | RESOLVED-FIXED | 5 | 5 | 0 | reuse `strip_comments`, `segments`; shared identity case table; prefixes block; ts_lex rationale |
| Operations (paired) | RESOLVED-FIXED | 3 | 3 | 0 | rebind cause-3 triage (epoch-reset check first; lifetime counter); duplicate bullet |
| Semantic Guard | RESOLVED-FIXED (native SAFE) | 4 | 4 | 0 | `[comment-vs-code]` mc-alerts.yaml:542 suffix-check overclaim; ZERO_FOREVER → TRIPWIRE_CLIENT_COUNTERS; `without(instance)` misfire; matcher drift (also flagged by code-reviewer/test/security) |
| Paired Client | RESOLVED-FIXED | 3 | 3 | 0 | retention-violation is a COUNT bound; rule-1 non-sink callee bypass; reconnect re-issue counts as `join_response` |
| Paired Infrastructure | RESOLVED-FIXED | 3 | 3 | 0 | shared `strip_comments`; shared `is_test_path`; `exporter_suffixing_enabled` rule id |

DRY extraction opportunity (not a deferred finding): `docs/TODO.md` §Cross-Service Duplication — `ts_metric_naming::find_meter_calls` inline lexer onto `common/ts_lex.rs`.

---

## Accepted Deferrals

- (none surfaced in this devloop)

---

## Rollback Procedure

1. Start commit: `339d46343bcc671d28dde0707f7a5a611c5376c8`
2. `git diff 339d4634..HEAD`
3. `git reset --soft|--hard 339d4634`
7. **Safe-revert unit**: the whole commit. The guard rules (dt-guard), the alert rules/inventory (byte-identical PromQL checked by `inventory_expr_drift`), the label-taxonomy fenced blocks (read at runtime by dt-guard and pinned by env-tests) and the catalogs move together; a partial revert of any one reds a guard. Docs-only hunks (runbooks, dashboards prose) could be reverted alone but would re-introduce falsified triage text.

---

## Gate 2 — Full Validation

`DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` on the final tree, attempt 1, exit 0.

| Layer | Result | Duration (s) |
|-------|--------|--------------|
| 1 Compile | OK | 7 |
| 2 Format | OK | 4 |
| 3 Guards | OK | 164 |
| 4 Test | N/A (aggregate; proto `test.sh` intentional-gap placeholder, `not-applicable-to-this-lang`) — rust + ts suites passed | 263 |
| 5 Lint | OK | 2 |
| 6 Audit | N/A (aggregate; proto audit placeholder, no dep-manifest changes) | 2 |
| 7 Env-tests | OK — Rust env-tests (incl. 32 media metric hygiene, 33 alert-rule loading) + browser E2E 17/17 | 1062 |

`TOTAL_RESULT=N/A` (worst-child aggregation of the two self-justifying N/A placeholders); no FAIL/PRECONDITION_FAILURE/NOT-RUN.

## Summary

Story-2 media observability shipped: catalogs, dashboards, alert inventory + new rules (`MHMediaEgressEdgeHeadroomLow`, three Group A client tripwires incl. new `client-alerts.yaml`), `unwrap_failed` joins `MCMediaMissingKeyMaterial`, label-taxonomy identity/key_custody rules with runtime-read SSoT fenced blocks, hygiene kernel extensions, and client-metrics-export guard completeness (rules 1–6 + G7 reject-token partition, all fail-closed with fixtures). Premise corrections recorded in §Implementation Summary and as dated notes on later story tasks (17, 18).
