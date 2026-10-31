# Operations: runbook and alert changes for MH replacement and migration (story 4)

Scope: the docs/rules the story must change, with the reason and the new content. Line numbers are as of 2026-10-28.
Context: the ADR-0036 §8 periodic re-assert never shipped. Every "no periodic re-assert yet" caveat below is a promise that was never kept, so it gets rewritten, not just deleted.
Metric names follow observability's contract (`mc_media_handler_*`, `trigger` label key). Observability owns the final names.

## A. ADR-0036 text that runbooks quote (amend in the same ADR change)
- §11 "MH sheds media sessions on restart, and that is the decision" (adr-0036-media-flow.md ~1689-1693): reverse it. MH DRAINS on SIGTERM via migration and sheds only what remains at the derived deadline (grace − SHUTDOWN_MARGIN).
- §11 "Eight media runbook scenarios are required": replace "restart media-dark" and "sustained re-assert failure" with the new scenarios in section D.
- `crates/mh-service/src/config.rs` SHUTDOWN_SETTLE_TARGET_SECONDS doc comment (~1153-1175), and the "This is NOT a drain phase" comment at `crates/mh-service/src/main.rs:658-663`: rewrite in the same commit as the drain code. These comments state the §11 decision.

## B. docs/runbooks/mh-deployment.md
1. §Rollout With Media Flowing (1054-1108): REWRITE.
   - "MH sheds media sessions on restart. That is the decision." → "MH drains on SIGTERM", with the sequence:
     1. not-ready
     2. GC load report health=DRAINING; the GC task stays alive until exit
     3. DRAINING heartbeat to every MC, carrying remaining drain time
     4. MC migrates each meeting make-before-break (to a GC grant, or to a surviving handler if there's no grant)
     5. MH exits at zero meetings or at the deadline
   - Client-visible consequence: a short blip per meeting, NOT "silent until rejoin". Delete "In this build they do not get it back on their own", "Recovery is unchanged: rejoin", and "Do not open an incident for this".
   - New: rollout with meetings on a same-instance surge pod. The live-set address-uniqueness rule (ruling 2) sends the transfer to ANOTHER instance. If every other instance is already in the meeting's set, or none is eligible, the meeting moves onto a surviving handler. Rolling all MH instances at once can therefore move some meetings twice. That's expected, and it shows as `trigger="planned_transfer"` migrations.
   - New: "Rollout stuck with the surge pod not Ready = check GC". MH Ready is gated on the first load report being accepted, so with GC down the old pod keeps serving. That's the safe direction.
   - New: what to check if planned transfers ended `outcome="timed_out"` during a rollout (the shed signal; MH's exit-time counter is not reliably scraped because the pod is gone within one scrape interval, so the signal lives on MC; MH also logs its shed count on its final exit line). Possible causes: drain deadline too short for the meeting count, GC unavailable and no survivors, or the MC limiter saturated. The remedy is to raise terminationGracePeriodSeconds (the drain deadline is derived from it, so change ONE value).
2. §Scaling MH to zero is NOT a media kill switch (1181-): keep the conclusion and update the mechanism. Scaling one instance down now DRAINS its meetings onto the others, so it isn't a kill switch. Scaling ALL to zero still causes new-meeting outage plus silent existing meetings, now visible as `mc_media_handler_meetings_without_handlers` > 0 (paged).
3. §Post-Deploy Monitoring Checklist (207-550) and §Post-rollout verification, media path (1327-):
   - Delete the re-assert caveats at 123, 136 and 248.
   - Delete the `mc_mh_notifications_without_connection_id_total == 0` item (336) and the "connection_id coupling" note (1370). Legacy empty-connection_id mode is retired, along with its metric.
   - Add: migrations{trigger="planned_transfer", outcome="completed"} rose by about the meetings on the rolled pods; `outcome="timed_out"` (the drain-shed signal) and the `no_capacity` outcome unobserved; `mc_media_handler_meetings_without_handlers` == 0.
4. §Required environment keys / MH_MAX_REGISTERED_MEETINGS guidance: add "raising the meeting cap: check the heartbeat bound". MH refuses to boot if its caps' worst-case heartbeat exceeds the shared decode-limit constant minus margin. At 4 MiB that ceiling is about 12k meetings. **[Superseded in round 4: R4-1 removes the MH heartbeat boot check; there's no heartbeat-bound step]]**
5. Capacity headroom (new subsection under deployment procedure): the fleet must absorb one draining MH per concurrently rolling pod, and loss of the largest single MH. GC exports `gc_mh_capacity_streams{region,kind}`. The alert fires when free < K × the largest MH max_streams (K = alert config, default 1).

## C. docs/runbooks/mh-incident-response.md
1. Scenario 10, MH→MC Notification Failures (628-): lost Connected/Disconnected now self-corrects within N via the heartbeat connected set (remove-only reconcile). Rewrite "MC never learns" as "MC reconciles within one heartbeat interval". A sustained notification failure still matters: it now shows as connected-set corrections (observability warning).
2. Scenario 13, RegisterMeeting Timeout — Clients Kicked (785-): **[Superseded in round 4: L4-2 deletes the pending-before-registration path; the scenario is RETIRED, see I]]**
   - Delete the "interim remedy: restart the pod ... the recovery it used to promise does not exist" block (803-830) and its re-assert premise.
   - A restarted MH holds no meetings, sends no heartbeats, and is replaced by MC within N. Add the WRONG_HANDLER and HANDLER_RELEASED close codes as distinct symptoms.
   - Keep the meeting-cap arm.
3. Scenario 15, Sender Binding (1047-): remove the empty-connection_id legacy/unreleasable text. Add: bindings for a removed handler are released by MC after removal. A Connected from an out-of-set or tombstoned handler gets sender_id 0 and records nothing (MC ruling 3). That's expected for a zombie, not a defect.
4. Scenarios 17 and 18 (1618, 1733, 1839): delete the "no periodic re-assert yet / quiescent meeting stays blackholed" caveats. The applied-generation comparison now runs on every heartbeat (APPLY_LAG trigger), so a quiescent diverged meeting is detected within L and migrated.
5. NEW scenario: "MH draining: meetings shed at deadline". The alert keys on MC's `mc_media_handler_migrations_total{trigger="planned_transfer", outcome="timed_out"}`, never on `mh_drain_meetings_shed_total` (best-effort, because the pod exits within a scrape interval); the evidence is MH's final exit log line. Triage: GC reachability, MC limiter saturation, grace sizing.
6. NEW scenario: "MH holding meetings with no clients" (zombie leak; gauge `mh_media_meetings_without_clients`). Explain RELEASED vs UNKNOWN: MH never self-tears-down on MC silence. Safe manual fence: restart the pod (security-confirmed). **[Superseded in round 4: no UNKNOWN verdict (R4-4); see I]]**
7. Recovery Procedures (2606-): add "planned drain of one MH = `kubectl delete pod` / `rollout restart deployment/mh-N`" and "force crash = `--grace-period=0 --force`", each with its expected signals.

## D. docs/runbooks/mc-incident-response.md (these become the §11 required scenarios)
1. NEW: "Media handler declared dead/impaired" (trigger HEARTBEAT_TIMEOUT | APPLY_LAG). Expected signals, the dark window, and the client-detected-first ratio, with the brief's statement that MC detection failure is a SAFE outcome (false declaration = one blip).
2. NEW: "Replacement failing: meetings without handlers" (PAGE). AcquireMediaHandler RESOURCE_EXHAUSTED/UNAVAILABLE; meetings run on survivors or sit awaiting a handler. Triage: GC health, fleet capacity gauge, MH admission refusals. FAILED_PRECONDITION = MC/GC disagree on a current handler, so escalate and do not retry.
3. NEW: "Migration timed out" (PAGE, observability). The make phase didn't complete, so MC fell to break-before-make. Check client WRONG_HANDLER decline rate (routing), media_dial token issuance, and the MH conntrack behaviour on terminating pods. **[Superseded in round 4: media_dial is MC-signed (L4-1); see I]]**
4. NEW: "Many handlers declared dead by one MC" (warning). Read the per-instance declaration increase against GC's view of the same handlers. If GC sees them healthy, suspect MC's own network path first. Not suppressed, by design. **[Superseded in round 4: re-keyed onto handler counts; see I]]**
5. NEW, folded into 1 or standalone: "Address-uniqueness assertion fired" (should never fire). A GC selection bug or MC/GC disagreement. Escalate.
6. Scenario 12, RegisterMeeting Coordination Failures (1691-): replace the "No alert today" TODO with MCRegisterMeetingFailureRate (observability: >0.10 error ratio over 5m). A RegisterMeeting failure against a fresh grant now adds that handler to removed_handler_ids and re-acquires, which is the mitigation to describe.
7. Scenario 13, Unexpected MH Notifications (1820-): Connected from a tombstoned or out-of-set handler is EXPECTED after a declaration (zombie or late notification) and gets sender_id 0. Delete the `mc_mh_notifications_without_connection_id_total` legacy bullet (1835).
8. Scenario 15, Media Generation Divergence (2422-): narrow it to the transport_mode_mismatch echo (observability). Delete "self-correcting, re-asserted on the next tick" (2443) and "no periodic re-assert in this build" (2550). Generation lag is now the APPLY_LAG health input and is handled by migration (scenario D1).
9. Scenario 20, Teardown / MH Budget Ratchet: a failed EndMeeting is now reclaimed by the RELEASED verdict on the next heartbeat (MC's TTL'd ended-meeting record). Update the "budget ratchet" remedy.

## E. Alert rules
- `infra/docker/prometheus/rules/mc-alerts.yaml` MCMediaGenerationDivergence (155-190): narrow the expr to outcome="transport_mode_mismatch" (observability). Rewrite the comment block that relies on "no periodic re-assert (story 4)" (165-170) and the description.
- mc-alerts.yaml ADD: page on terminal replacement failure (`mc_media_handler_meetings_without_handlers` > 0, or migrations outcome no_capacity|gc_unavailable); page on migrations outcome="timed_out"; warning on the one-MC mass-declaration signal; MCRegisterMeetingFailureRate (Scenario 12); the address-uniqueness assertion counter > 0.
- mc-alerts.yaml ADD (moved from MH): drain shed = `mc_media_handler_migrations_total{trigger="planned_transfer", outcome="timed_out"}` increase > 0.
- mh-alerts.yaml MHDown (79): make it fleet-level (zero healthy non-draining MHs, per observability). Per-pod down is routine under replacement and per-incarnation provisioning.
- mh-alerts.yaml ADD: a warning on sustained `mh_media_meetings_without_clients`; a warning on a sustained WRONG_HANDLER decline rate.
- mh-alerts.yaml MHMCNotificationFailures (729): keep it, but reword the impact. Lost notifications now self-correct within N.
- gc-alerts.yaml ADD: fleet capacity headroom (free < K × the largest MH max_streams); GC AcquireMediaHandler RESOURCE_EXHAUSTED rate.
- Every new alert needs its runbook anchor to resolve (validate-alert-rules.sh). The runbook scenarios above must therefore land in the same commit as their alerts.

## F. Config/deploy items with runbook impact
- MC: MC_MH_HEARTBEAT_TIMEOUT_MS (6000, bounded at startup), with the interval derived and sent to MH. MC_MAX_MEETINGS and MC_MAX_PARTICIPANTS become hard-fail parses (today .ok().unwrap_or(), config.rs:565-573). Add them to mc-deployment.md §Configuration Reference.
- MH: no new keys. The drain deadline is derived from MH_TERMINATION_GRACE_SECONDS (DrainWindowSource loses its 2s settle cap).
- One shared decode-limit constant in `common`, read by MH's boot check and MC's MediaCoordinationService server builder.
- GC: one MH staleness value (today the hardcoded 30s candidate window and the reused MC 60s sweep threshold, gc main.rs:245); the stale-draining sweep fix; per-incarnation row hygiene; the initial handler count becomes config (default 2; today hardcoded at mh_selection.rs:48).
- Kind: ~~a third static MH instance (mh-2)~~ superseded by on-demand `mh.sh up` (see G/H). If shareProcessNamespace is adopted for the zombie injector, an env-test asserts graceful delete → GC sees Draining.

## G. Round 3 (user ruling: every MH incarnation has its own address; on-demand provisioning)
These depend on infrastructure's final model; they're written against ops requirements R1-R9.
- mh-deployment.md §Deployment Procedure and §Rollback: REWRITE.
  - Upgrade = provision new incarnation(s) → Ready (= GC-selectable) → SIGTERM old → drain → exit → the slot is free once the pod is gone (no quarantine; see H).
  - Rollback = the same command with the previous image/config. Delete the `kubectl rollout undo` guidance (167-200) and the content-addressed ConfigMap undo-safety rationale, which also lives in infra/services/mh-service/config.env comments.
- NEW scenario: "Upgrade/scale-up refused: address pool exhausted". The safe direction: the old pod keeps serving. The pool size is fixed at Kind cluster creation; in prod it's the address pool.
- NEW scenario: "Upgrade stuck: new incarnation not Ready = check GC" (Ready is gated on GC-selectable).
- NEW procedure: node maintenance = provision replacements elsewhere → retire the incarnations on the node (drain) → drain the node (exact form depends on infra's workload choice).
- Drain step text: the per-incarnation WebTransport Service keeps publishing the endpoint until the pod exits (publishNotReadyAddresses), and GC DRAINING keeps new meetings off.
- Replaces F's "Kind: a third MH instance (mh-2)" with on-demand provisioning commands. The Kind demo provisions an extra MH on demand.
- Freeze item 2 collapse (pending GC): drop the address-exclusion test vector and the MC assertion counter alert. Add "GC register retires other rows at the same endpoint".
- Capacity headroom (B5) gains a second dimension: free address slots ≥ concurrent upgrades.
- GC supersede (agreed with GC): register_mh DELETEs other rows at the same webtransport_endpoint (counted gc_mh_registrations_total{outcome="superseded"}). Expected once per in-place container restart.
  - A superseded registration alone is routine: every in-place crash restart supersedes (match=both). It is NOT a two-live-process signal.
  - gc-alerts.yaml ADD (per telemetry-contract.md): `GCMhRegistrationPingPong` (superseded ≥ 20 in 10m AND `gc_mh_load_reports_total{outcome="unknown_handler"}` increasing); `address_conflict` still increasing past the staleness bound + one retry; `unknown_handler` warning on any increase.
  - NEW gc-incident-response scenario: "Two live MHs on one address" (any of the three signals above). Provisioning has violated the one-process-per-address rule: the hostPort lock was defeated (almost always `kubectl delete pod --force --grace-period=0`) or the spike's fallback is in use. Find the duplicate MH pod on that slot and retire it; don't touch GC.
- Address-reuse quarantine: none (superseded by H).

## H. Round 3 settled against infrastructure's hostPort slot model
- Commands named in runbooks:
  - `infra/kind/scripts/mh.sh up [--slot k]` / `drain <k>` (refuses the last serving MH without --force, and refuses while any MH pod is Terminating past its grace: "investigate, don't force") / `upgrade` (serial, resumable) / `list` (flags stuck-Terminating pods).
  - Upgrade and rollback = deploy.sh → mh_converge, serial start-new-then-drain-old. Failure reasons: `DEPLOY_FAILED REASON=mh-no-free-slot | mh-rollout-failed`, with the old MH still serving.
- FORBID `kubectl delete pod --force --grace-period=0` on MH pods. It's the one way to defeat the hostPort lock: the API object goes while the process may still run, so a second MH can take the slot. A stuck termination gets investigated, not forced.
- The crash drill is a process kill (in-place container restart → new handler_id on the same slot → GC supersede), not a force delete.
- The PDB stays and gates evictions (node drain). The last-MH refusal gates tool-driven drains.
- Slot capacity: K (helper port table, K = 6; baseline B = 2, config). Changing K means recreating the Kind cluster.
- Replaces G's quarantine item: there's no quarantine. Supersede + media_dial aud cover reuse (`mh up` uses next-fit slot selection, so immediate reuse is rare).
- Deploy pruning caveat (asked of infra): image and ConfigMap pruning must keep whatever live MH pods reference, plus the previous revision.
- docs/TODO.md entry (infra): mc-0/mc-1 RollingUpdate surge puts two live MCs behind one NodePort (same bug class, task-sized, MC out of scope).


## I. Round 4 (user rulings R4-1..R4-6, lead rulings L4-1..L4-10)
- Heartbeat is per (meeting, handler) (R4-1). One MH death = M per-meeting Dead declarations and M AcquireMediaHandler calls within ~N, bounded by the shared per-MC limiter. EXPECTED: never alert on raw declaration volume. A single MH death pages only via MCMediaMeetingsAwaitingHandler, if replacement fails past its bound.
- D4 rewritten: "Several handlers declared dead by one MC" keys on `mc_media_handler_first_declarations_total{trigger="heartbeat_timeout"}` (once per handler) over `mc_media_handlers_in_use` (observability's signature: ≥ 2 handlers AND ≥ a fraction, per instance).
  - The same signature on SEVERAL MCs = real multi-MH loss (node/AZ): check `mh.sh list --json` and node state.
  - Only THIS MC = its own network path: leave the handlers alone. A false declaration is a safe outcome (one blip).
- RETIRE mh-incident-response Scenario 13 (RegisterMeeting Timeout — Clients Kicked; 785-) and the severity-table line at 61. L4-2 deletes the pending-before-registration path, `MH_REGISTER_MEETING_TIMEOUT_SECONDS` and `mh_register_meeting_timeouts_total`.
  - A token is only issued after RegisterMeeting is acked. A dial reaching a handler without that meeting is DECLINED, and the lost-registration case is a restarted MH, which is the declared-dead path (D1).
  - Remove the cross-reference from mc-incident-response Scenario 12. Leave the anchor as a one-line pointer only if other docs link to it; otherwise delete it, and keep validate-alert-rules runbook refs resolving.
- D3 "Migration timed out" triage: media_dial is MC-signed with a per-meeting Ed25519 key (L4-1). AC is OFF the migration path, so delete "check token issuance at AC". Check instead: the client FAILED-report re-push, the MH exp skew (`MH` config, ~30 s, L4-5) against clock drift, and MC_MEDIA_DIAL_TOKEN_TTL_SECONDS (default 300, startup-validated to [60, 900], L4-3).
- Zombie scenario (C6) simplified: the verdict is OWNED | RELEASED(cause) only (R4-4, L4-6). RELEASED is the retry for a lost EndMeeting. MH never acts on MC silence or on an error answer.
  - An MH holding meetings whose clients are gone while its heartbeats get ERRORS = its MC endpoint now answers as a different mc_id (L4-7). MH takes no action, so the meetings leak until MH restarts; the gauge `mh_media_meetings_without_clients` shows it. Remedy: `mh.sh drain`.
  - A pod restart remains the safe manual fence.
  - The heartbeat answer is now `FAILED_PRECONDITION` (owner check, ADR §6), counted on MH.
- NEW (mc-deployment.md rollout section): MC rollout. A restarted MC with the same mc_id answers RELEASED(MEETING_ENDED) for its predecessor's meetings, and MHs end them within one heartbeat interval. MH meeting counts drop: EXPECTED. Wording (MC/ops, L4-7): an incidental consequence of "RELEASED = not in my set", not an MC-failover mechanism. Those meetings are already lost when MC restarts (MC failure is out of scope), and releasing them reclaims MH capacity.
- NEW gc-incident-response scenario, gray failure (known gap, R4-5): "GC keeps handing out a handler that MCs keep replacing".
  - Signals: `gc_mh_replacement_requests_total{trigger,outcome,gc_view}` (dashboard, no page) and the `gc.mh.replacement_report` INFO log event (replaced handler_id, webtransport_endpoint, controller_id, trigger, gc_view, outcome).
  - Procedure: group the events by replaced handler_id over the window. ≥ 2 distinct controller_ids AND the handler is still load-reporting healthy past the GC staleness bound (`mh.sh list --json` state=serving, not stale) = likely gray failure: `mh.sh drain <slot>`. A handler that went stale is really dead and already unselectable, so there's nothing to drain. (slot from the endpoint via `mh.sh list --json`). One controller_id = suspect that MC's path and leave the handler.
  - Caveat (L4-9): controller_id is self-asserted, which is fine for a human decision but not yet for an automatic quorum. The TODO entry carries the automatic policy.
- Config reference additions: MC_MEDIA_DIAL_TOKEN_TTL_SECONDS (mc-deployment.md); MH exp-skew key (mh-deployment.md §Required environment keys); the per-(meeting, sub) pending-connection cap (L4-8).
- mh.sh in runbooks uses the merged L4-10 contract. `list --json` fields: slot, address, pod, revision, state, terminating_past_grace, stale, owner, restart_count. Drain failure tokens: `last-serving-mh` and `stuck-terminating` exit 4 REFUSED; `drain-timeout` exits 1. Runbook branches use the final `MH_RESULT STATUS=FAIL REASON=<token>` line. Exact exit codes follow the merged contract.
- ADR "Negative: AC sits on the migration path" no longer applies (R4-3). There's no AC runbook change.
