# Devloop Output: MC server mute programmed into MH policy + meeting teardown (EndMeeting)

**Date**: 2026-09-27
**Task**: Story 2 task 12 (hear-each-other): server mute → MH `server_muted_sources`, authority/rate-bounds/late-joiner replay, EndMeeting teardown after push quiescence (R-8, R-9, R-10, R-11, R-20; ADR-0036 §5, §7)
**Specialist**: meeting-controller
**Mode**: Agent Teams (v2) — full, Gate-1 present <!-- panel mode + Gate-1 tier (ADR-0037 §D2); see the Tier row in Loop State -->
**Branch**: `feature/hear-each-other`
**Duration**: ~Xm (approximate total time)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `e6c0a248fce7ad5a80e089d8cbe2ca90b8c031f0` |
| Branch | `feature/hear-each-other` |
| Lead Model | `claude-opus-5-5` |

---

## Loop State (Internal)

<!-- This section is maintained by the Lead for state recovery after interruption. -->
<!-- Do not edit manually - the Lead updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `spawned` |
| Implementing Specialist | `meeting-controller` |
| Tier | `full` |
| Iteration | `1` |
| Security | `spawned` |
| Test | `spawned` |
| Observability | `spawned` |
| Code Quality | `spawned` |
| DRY | `spawned` |
| Operations | `spawned` |
| Semantic Guard | `spawned` |
| Paired media-handler | `spawned` |
| Protocol (conditional, GSA owner) | `spawned` |
| Paired global-controller (added: GC re-assign fix) | `spawned` |
| Database (conditional) | `spawned` |
| Infrastructure (conditional, dt-guard machinery; added Gate 2) | `spawned` |

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

Task description (verbatim, run-story task #12):

> Server mute programmed into MH policy, and meeting teardown, in the Meeting Controller (docs/user-stories/2026-09-21-hear-each-other.md, R-8, R-9, R-10, R-11, R-20; ADR-0036 §5 table, §7). Depends on the protocol task (`MutedSource`, `EndMeeting`), the MC static-fill task (structural re-push path) and the MH runtime-membership task (the env-tests observe MH enforcement). Pair with media-handler, security (authority checks), observability (counter) and test.
>
> Server mute: MC already holds the state (`audio_server_muted` and friends), the host-role gate and `handle_server_mute` in `actors/meeting.rs`, ending at the comment "TODO (Phase 6d): Notify MH to enforce mute" (`webtransport/connection.rs` ~955 consumes `ServerMuteRequest` with no metric). Populate the new `server_muted_sources` set on `RegisterMeetingRequest` from participant server-mute state in EVERY full registration snapshot, filtered per handler (handler H's set is the muted senders that send to H — a sender whose edges span several handlers is in each of their sets — because mute is enforced at ingress and there is no cross-handler forwarding) (so it survives re-assert and MH restart) and treat a mute or unmute change as a structural change: recompute, advance the policy generation, re-push, confirm the applied echo. Authority (R-8): only a participant whose meeting token carries `MeetingRole::Host` (host means the meeting creator, derived by GC and stamped by AC; guests carry a fixed guest role and can never be host; MC reads `claims.role` at accept, `connection.rs` ~484) may apply OR lift a server mute; the requester is identified from the authenticated connection and never from a message field (`ServerMuteRequest` carries no requester); check authority before resolving the target; refuse non-hosts at connection dispatch and again in the actor with a generic error that does not distinguish "no such participant" from "not permitted", counted by bounded reason. The participant-facing `UnmuteRequest` NOTIFIES the host and never clears a server mute (single-writer preserved); client mute and server mute compose so clearing one never clears the other. Server-mute requests are rate-bounded per connection at least as tightly as the existing client-mute limiter, refusals counted. A server mute survives the muted participant's reconnect within the grace window and does not survive a fresh rejoin; no identifier, string, doc or test calls it a ban. Slot-state reflection already works (`media_signaling/assignments.rs` maps a muted source to `SLOT_STATE_SOURCE_MUTED`); on any mute change re-emit updated `StreamAssignments` to affected subscribers (the R-4 third driver). Late joiners: the roster `Participant` message carries no mute state, so immediately after the join response MC replays a `ParticipantMuteUpdate` for each already-server-muted participant so the joiner renders who-muted-whom at once (no proto change). Count `mc_media_server_mute_requests_total{action, outcome}` separately from `mc_media_mute_requests_total` (different decider, enforcement point and remedy); MC logs the decision with target and requester in the signalling path, where identity is legitimate.
>
> Teardown (R-20): on meeting end or empty (the same point that emits `NotifyMeetingEnded` to GC), and only AFTER MC has QUIESCED every RegisterMeeting push for that meeting (no new attempt starts, retries and backoff included, and every attempt already issued has RETURNED rather than been dropped; see `EndMeetingRequest` in `internal.proto`), call the new `EndMeeting` RPC on each handler in the meeting's assigned set with the meeting id and this MC's id (the registering `mc_id`; a mismatch is rejected by MH by design). Count outcomes; rename MC's internal end-meeting function if it collides. Record in code the story-2 assumption (no cadence re-assert exists yet, so ordering suffices) and the story-4 note that a re-assert racing a teardown needs a fence.
>
> Tests: unit tests for authority-before-target, non-host refusal at both layers, UnmuteRequest never clearing, rate bound, compose rule; integration tests for the muted set present in every snapshot and advancing the generation, the mute surviving reconnect and not a rejoin, the late-joiner replay, and EndMeeting called once per handler after pushes stop; Rust env-tests against the live Kind cluster: a host's server mute makes MH's `server_muted` ingress-drop counter move while the muted sender's own transport egress stays non-zero and every other receiver's accepted-from-that-sender count goes flat (scenario S3's server half; the browser half is client's), an authorized unmute restores forwarding within one policy generation, a non-host attempt is refused and counted, and a meeting end produces a teardown observed on MH's registered-meetings gauge. No wall-clock gates.
>
> EndMeeting contract and coordination tests (added 2026-09-25). (1) MC contract, hermetic: unit-test the decision as a pure function — `EndMeeting` goes to EVERY handler the meeting was registered on (under the shared-handler model, the whole frozen set), carries MC's own `mc_id`, names the right meeting, and is sent only after the last policy push has stopped — and an integration test over the `mc-test-utils` mock MH asserting exactly those calls (none extra, none missing, none before pushes stop). No env-test is needed for this half. (2) Coordination, an env-test driven through PUBLIC APIs: participants join through GC/MC with enough structural changes that the meeting reaches policy generation 2 or higher, then all leave, except one straggler that leaves through MC but deliberately keeps its MH WebTransport session open (the realistic case: a crashed tab). Assert (a) MH closes the straggler's connection (its `closed()` resolves with MH's close reason), then (b) for each handler, a probe registration of that meeting id at generation 1, made as a separate MC principal as env-test 29 does, returns `applied_generation == 1`, which is only possible if the real MC's `EndMeeting` released it; clean the probe up afterwards. Never probe while the meeting is live: any validation-passing registration takes over the meeting's ownership (deliberately, for failover, `crates/mh-service/src/grpc/mh_service.rs`) and would hijack it from the real MC. (3) Confirm MH's hermetic `end_meeting_from_the_registering_mc_releases_everything` asserts connection closure and the meeting-keyed maps, not only routes; extend it if not. Env-tests follow the evidence rule in `crates/env-tests/README.md` (drive public APIs; assert per-entity evidence; never the value of a shared pod-level gauge).

### Scope
- **Service(s)**: {Which services were affected}
- **Schema**: {Database schema changes? Yes/No}
- **Cross-cutting**: {Does this affect multiple services? Yes/No}

### Debate Decision
{NEEDED/NOT NEEDED} - {Brief justification}

{If debate was needed, link to debate record: `docs/debates/YYYY-MM-DD-{topic}.md`}

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
| `crates/mc-service/src/actors/meeting.rs` | Mine | — |
| `crates/mc-service/src/actors/meeting_media.rs` | Mine | — |
| `crates/mc-service/src/actors/messages.rs` | Mine | — |
| `crates/mc-service/src/actors/controller.rs` (+ `MeetingEndedSink` seam, rejoin fence) | Mine | — |
| `crates/mc-service/src/actors/participant.rs` (test update: MuteChanged now wire-visible) | Mine | — |
| `crates/mc-service/src/media_routing/assignment.rs` | Mine | — |
| `crates/mc-service/src/media_admission/sender_id.rs` (`Ord` on `SenderId`, for the canonical muted-set order) | Mine | — |
| `crates/mc-service/src/media_signaling/assignments.rs` (render call-site fallout: the now-required muted set) | Mine | — |
| `crates/mc-service/src/media_signaling/directive.rs` (render call-site fallout: the now-required muted set) | Mine | — |
| `crates/mc-test-utils/src/media.rs` (render call-site fallout: the now-required muted set) | Mine | — |
| `crates/mc-service/src/media_routing/slots.rs` | Mine | — |
| `crates/mc-service/src/media_routing/pusher.rs` | Mine | — |
| `crates/mc-service/src/media_routing/teardown.rs` (new: pure EndMeeting plan, `Quiesced` witness, outcome classifier, teardown worker) | Mine | — |
| `crates/mc-service/src/media_signaling/mod.rs` (re-exports for the new outcome types) | Mine | — |
| `crates/env-tests/tests/26_mh_quic.rs` (two owners split by edit: the `mc_join` helper bundling the MC session is Mine per §6b; the S4 premise, attribution, injection-reason and cleanup rewrites, G1-7 items 5-7, are media-handler's) | Mine + Not mine, Minor-judgment | media-handler (paired) |
| `crates/mc-service/tests/media_admission_integration.rs` (meeting-outlives-empty fix, §6b) | Mine | — |
| `crates/mc-service/tests/join_tests.rs` (meeting-outlives-empty fix, §6b) | Mine | — |
| `crates/mc-service/src/media_routing/mod.rs` | Mine | — |
| `crates/mc-service/src/grpc/mh_client.rs` | Mine | — |
| `crates/mc-service/src/grpc/gc_client.rs` (E2 only) | Mine | — |
| `crates/mc-service/src/main.rs` (E2: ended-meeting → GC drain task; teardown worker spawn; OPS-12 derived-bound startup fields; gauge boot init) | Mine | — |
| `crates/mc-service/src/webtransport/connection.rs` | Mine | — |
| `crates/mc-service/src/webtransport/handler.rs` | Mine | — |
| `crates/mc-service/src/media_signaling/outcome.rs` | Mine | — |
| `crates/mc-service/src/errors.rs` (O-19: `McError::MeetingTeardownInProgress`, wire code stays `CONFLICT`) | Mine | — |
| `crates/mc-service/src/observability/metrics.rs` | Mine | — |
| `crates/mc-service/tests/common/mod.rs` | Mine | — |
| `crates/mc-service/tests/server_mute_integration.rs` (new) | Mine | — |
| `crates/mc-service/tests/meeting_teardown_integration.rs` (new; incl. the positive NotifyMeetingEnded test over a `MeetingEndedSink` double) | Mine | — |
| `crates/mc-service/tests/*.rs` (rename fallout `end_meeting` → `close_meeting`) | Mine | — |
| `crates/mc-test-utils/src/mock_mh.rs` (interleaved register/end event log, FAILED_PRECONDITION knob) | Mine | — |
| `crates/env-tests/tests/35_mc_server_mute_teardown.rs` (new) | Mine | — |
| `crates/env-tests/src/fixtures/mh_grpc.rs` (G1-5: hoist test 29's `register(meeting_id, mc_id, generation)` at its second consumer; reuse the existing `ReleaseOnDrop`) | Mine | — |
| `crates/env-tests/src/fixtures/mc_session.rs` (keep-alive variants of the WT and MH connect helpers, for the straggler) | Mine | — |
| `crates/env-tests/src/fixtures/media.rs` (G1-5 hoist of `read_frame` and `drain` from test 26 at their second consumer) | Mine | — |
| `crates/env-tests/src/fixtures/participant.rs` (G1-5 hoist of `open_sessions` from test 26 at its second consumer) | Mine | — |
| `crates/mc-service/src/media_routing/generation.rs` (test: a muted-set change alone advances the generation) | Mine | — |
| `crates/mc-service/src/webtransport/server.rs` (boot zero of the muted-sources gauge) | Mine | — |
| `crates/mc-service/tests/common/media_session.rs` (host join, server-mute and unmute-request frames) | Mine | — |
| `crates/mh-service/src/session/mod.rs` (`handle_config_apply` refuses to install policy for a meeting not in `registered_meetings` → `RejectedStale`; + actor unit test) | Not mine, Domain-judgment | media-handler (paired; drafts) |
| `crates/mh-test-utils/src/session.rs` (new: `apply_registered` — register as the fixture MC, then apply; the ONE home for the precondition the B1 guard adds) | Not mine, Minor-judgment | media-handler (paired) |
| `crates/mh-test-utils/src/lib.rs` (module declaration) | Not mine, Minor-judgment | media-handler (paired) |
| `crates/mh-service/tests/gc_integration.rs` (register-before-apply, forced by the B1 guard) | Not mine, Minor-judgment | media-handler (paired) |
| `crates/mh-service/tests/stream_admission_integration.rs` (register-before-apply, forced by the B1 guard) | Not mine, Minor-judgment | media-handler (paired) |
| `crates/mh-service/tests/media_server_mute_integration.rs` (register-before-apply, forced by the B1 guard) | Not mine, Minor-judgment | media-handler (paired) |
| `crates/mh-service/tests/end_meeting_integration.rs` (integration test for the guard counter; required by `validate-metric-coverage`) | Not mine, Minor-judgment | media-handler (paired) |
| `crates/mh-service/src/grpc/mh_service.rs` (test-only: extend `end_meeting_from_the_registering_mc_releases_everything`, check item 3) | Not mine, Minor-judgment | media-handler (paired) |
| `proto/dark_tower/signaling/v1/signaling.proto` (GSA; additive `UnmuteRequest.participant_id = 3`, Lead-approved F-A) | Not mine, Domain-judgment | protocol |
| `proto/dark_tower/internal/v1/internal.proto` (GSA; COMMENT-ONLY, no wire change — G1-8: `EndMeetingRequest`'s "RESIDUAL, ACCEPTED IN STORY 2" paragraph `:801-805` narrowed into its three fates; pointer to name the TODO entry by title) | Not mine, Domain-judgment | protocol |
| `crates/env-tests/tests/28_mh_egress_admission.rs` (G1-7 item 8: retitle "the ratchet, for meetings nobody ends" + replacement residual) | Not mine, Minor-judgment | media-handler (paired) |
| `crates/env-tests/tests/29_mh_meeting_teardown.rs` (G1-7 item 9: why a direct call is still needed; + the G1-5 `register()` helper hoist, which is Mine) | Mine + Not mine, Minor-judgment | media-handler (paired) for item 9 |
| `crates/mh-service/src/config.rs` (G1-7 Tier B: comment-only "until task 12" premise inversion; wording by @paired-media-handler) | Not mine, Minor-judgment | media-handler (paired) |
| `crates/mh-service/src/observability/metrics.rs` (G1-7 Tier B comment inversion; the `mh_media_released_meeting_apply_refusals_total` recorder + ANCHOR) | Not mine, Minor-judgment | media-handler (paired) |
| `crates/mh-service/src/grpc/gc_client.rs` (G1-7 Tier B: comment-only premise inversion) | Not mine, Minor-judgment | media-handler (paired) |
| `infra/grafana/dashboards/mh-overview.json` (`:1503` — add the story qualifier so a future sweep of this class cannot mis-edit a DIFFERENT story's "task 12") | Not mine, Minor-judgment | media-handler (paired) |
| `docs/observability/dashboard-conventions.md` (G1-7 Tier C: ":252 conversion lands with task 12") | Not mine, Minor-judgment | observability |
| `crates/gc-service/src/repositories/meeting_assignments.rs` (Lead ruling, Option 1: `atomic_assign` revives an ENDED row; `cleanup_old_assignments` and `end_stale_assignments` keyed on meeting and region with their predicates re-asserted; query changes, NOT a migration; I draft, GC reviews, database on the panel) | Not mine, Domain-judgment | global-controller (paired) |
| `crates/gc-service/src/services/mc_assignment.rs` ("once per meeting" becomes "once per meeting INCARNATION" wording) | Not mine, Minor-judgment | global-controller (paired) |
| `crates/gc-service/tests/meeting_tests.rs` (same wording) | Not mine, Minor-judgment | global-controller (paired) |
| `crates/gc-service/tests/mc_assignment_rpc_tests.rs` (same wording) | Not mine, Minor-judgment | global-controller (paired) |
| `docs/observability/metrics/gc-service.md` (stickiness note: a rejoin after meeting end is a new assignment) | Not mine, Minor-judgment | observability |
| `crates/gc-service/tests/meeting_assignment_tests.rs` (notify→re-assign test; failover-with-ended-row test asserting `ended_at IS NULL` afterwards) | Not mine, Domain-judgment | global-controller (paired) |
| `docs/runbooks/gc-deployment.md` (§Coordination: GC's fix is a PREREQUISITE of MC's E2; rollback MC first) | Not mine, Minor-judgment | operations |
| `docs/runbooks/gc-incident-response.md` (notify residuals beside the MC-assignment triage: stuck `meeting_not_found`, the split meeting; the ratchet sentence's task-11 premise takes the shared residual) | Not mine, Minor-judgment | operations |
| `docs/observability/alerts.md` (`:657` destructive-remedy fix, Variant A) | Not mine, Minor-judgment | observability (text by @operations) |
| `scripts/layer7.sh` (one of the five "no production code ever marks a meeting ended" copies; shape-3 narrowing) | Not mine, Minor-judgment | infrastructure |
| `docs/runbooks/devloop-validation.md` (same copy, same narrowing) | Not mine, Minor-judgment | infrastructure |
| `docs/user-stories/2026-08-11-story-runner-hardening.md` (same copy, same narrowing) | Not mine, Minor-judgment | infrastructure |
| `crates/env-tests/src/fixtures/auth_client.rs` (same copy, same narrowing) | Not mine, Minor-judgment | test |
| `packages/web-app/e2e/README.md` (same copy, same narrowing) | Not mine, Minor-judgment | test |
| `docs/observability/metrics/mc-service.md` | Not mine, Minor-judgment | observability |
| `infra/grafana/dashboards/mc-overview.json` | Not mine, Minor-judgment | observability |
| `infra/grafana/dashboards/mc-media.json` (NEW, Gate-2 L7 fix: MC's four ADR-0036 media-path rows split out of `mc-overview.json`, by POSITION; overlap-free re-layout; two-way links) | Not mine, Minor-judgment | observability (boundary endorsed, O-26/O-27 round) |
| `infra/grafana/kustomization.yaml` (NEW `grafana-dashboards-mc-media` generator in its own ConfigMap, carrying the sidecar label) | Not mine, Minor-judgment | infrastructure (generator entry); observability (dashboard content) |
| `crates/dt-guard/src/kustomize_configmaps.rs` (NEW guard module, Lead-directed at Gate 2: `configmap_annotation_size` over every generator under `infra/` using a kubectl-pinned Go-JSON model, 80% headroom derived from the measured single-task growth, superseding the 90% first shipped; `dashboard_configmap_label` reading the sidecar's own LABEL/LABEL_VALUE) | Not mine, Domain-judgment | infrastructure (machinery; added to the panel by the Lead at Gate 2, reviews at Gate 3). Policy content (headroom, label-rule scope) specified by observability (O-26/O-27) and infrastructure (80% derivation, vacuity) |
| `crates/dt-guard/src/kustomize.rs` (wires the two checks into `run`, REASON classes, module header) | Not mine, Minor-judgment | infrastructure |
| `crates/dt-guard/src/lib.rs` (module declaration) | Not mine, Mechanical | infrastructure |
| `docs/observability/dashboards.md` (MC Media Path section incl. the §4 key-custody justification; MC Overview rows list corrected, it had omitted KEK Lifecycle before this diff) | Not mine, Minor-judgment | observability |
| `infra/docker/prometheus/rules/mc-alerts.yaml` | Not mine, Minor-judgment | observability |
| `docs/runbooks/mc-incident-response.md` (two new Scenarios; regex + heading fix; G1-7 Tier D `:2858`) | Not mine, Minor-judgment | operations |
| `docs/runbooks/mh-incident-response.md` (G1-7 Tier D/item 12: `:780` and `:784` — split the residual, clean path vs never-completes path; + two phantom `gc_rpc_duration_seconds` selectors annotated by operations at Gate 3, OPS-25) | Not mine, Minor-judgment | operations |
| `docs/runbooks/mh-deployment.md` (Gate 3 OPS-20: the reciprocal three-service order sentence in §Cross-service ordering) | Not mine, Minor-judgment | operations |
| `infra/services/mc-service/mc-0-deployment.yaml` (comment-only, Gate 3 OPS-18: `terminationGracePeriodSeconds` coupled to `SHUTDOWN_TERMINATION_GRACE_SECONDS`, drift-tested) | Not mine, Minor-judgment | infrastructure |
| `infra/services/mc-service/mc-1-deployment.yaml` (same comment) | Not mine, Minor-judgment | infrastructure |
| `infra/kind/scripts/setup.sh` (Gate 3 infra F3: Grafana `rollout restart` + `rollout status` after the observability apply; DRY D-2: the two remaining R-7 "marks a meeting ended" sites narrowed) | Not mine, Minor-judgment | infrastructure |
| `crates/dt-guard/src/main.rs` (Gate 3 infra F7b: the `kustomize` subcommand's clap doc-comment names the two new checks) | Not mine, Minor-judgment | infrastructure |
| `docs/runbooks/mc-deployment.md` | Not mine, Minor-judgment | operations |
| `docs/observability/label-taxonomy.md` (shared `action` row) | Not mine, Minor-judgment | observability |
| `docs/observability/metrics/mh-service.md` (§`server_muted` bullets 2-3 flip per O-17/O-18, PLUS G1-7 items 10: `:329` and `:413` residual phrase) | Not mine, Minor-judgment | observability |
| `infra/grafana/dashboards/mh-media.json` (G1-7 item 11: `:965` registered-meetings description; `:746` stays true, left alone) | Not mine, Minor-judgment | observability |
| `infra/services/mh-service/configmap.yaml` (comment-only, G1-7 sweep hit: the two "MC begins calling it in story 2 task 12" premises take the shared never-COMPLETES residual; no data key changes) | Not mine, Minor-judgment | media-handler (paired) |
| `docs/TODO.md` (update the never-reclaimed entry in place, split its fence pointer, close the MH-side entry with its residual named, correct the startup-fields entry, add the NotifyMeetingEnded incarnation entry) | Mine | — |
| `docs/specialist-knowledge/meeting-controller/INDEX.md` | Mine | — |

Considered and NOT touched (so not rows): `docs/observability/metrics/client.md` (its `action` row is per-metric, not enumerated); `packages/sdk-core/src/proto/**` (Nx codegen output, untracked); `crates/gc-service/.sqlx/**` (the changed queries are runtime `sqlx::query`, no offline data to regenerate).

---

## Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed (G1-1..G1-8) |
| Operations | confirmed |
| Semantic Guard | confirmed |
| Paired media-handler | confirmed |
| Protocol (GSA owner, both proto rows) | confirmed |
| Paired global-controller (added after GC blocker) | confirmed |
| Database (conditional, added after GC blocker) | confirmed |

Lead rulings: F-A (additive `UnmuteRequest.participant_id`) approved; F-B = E2 (meeting ends when empty) approved; MH `handle_config_apply` guard approved in-scope (paired). GSA Owner cells normalised to the manifest value `protocol` (security is on the panel per the §6.4 intersection rule). Classification guard: `STATUS=OK`. Mid-implementation GC blocker (ended assignment row never re-assignable) ruled Option 1 in-loop (revive query, both halves; cleanup/stale jobs keyed on (meeting_id, region)); global-controller + database added and confirmed before the end-on-empty trigger was wired.

---

## Planning

### Scope findings raised to @team-lead — RULED: F-A approved (additive proto field, @protocol added), F-B = E2 approved

- **F-A (proto, GSA).** The task says `UnmuteRequest` "NOTIFIES the host". The only relay shape, `ServerMessage.unmute_request = UnmuteRequest{request_audio, request_video}`, carries no participant id, so the host cannot tell who is asking. Recommended fix is additive: `string participant_id = 3;` on `UnmuteRequest`. MC stamps it from the authenticated connection and overwrites any client value. This needs @protocol. **Fallback:** MC refuses and counts `UnmuteRequest` (it never clears a mute) and the notify half is filed for protocol+client.
- **F-B (the teardown anchor does not exist).** MC never emits `NotifyMeetingEnded`. No meeting actor ever ends when it empties, and `end_meeting` / `remove_meeting` have no production caller. ADR-0010 §3 says "last participant leaves, MC notifies GC". **Recommended E2:**
  - Last roster removal ends the meeting.
  - The actor quiesces its pushers, then calls `EndMeeting` on every frozen handler, then exits.
  - The controller reaps the meeting and calls GC `NotifyMeetingEnded` (new `GcClient` method; GC's handler already exists), so GC stops reusing the dead assignment.
  - **E1 (smaller):** release MH only when the meeting empties and keep the MC actor and GC assignment alive. Plan below assumes E2.

### Restated mechanism
"Server mute" is a **third input to the per-handler forwarding snapshot**, alongside slots and connectivity. So it rides `HandlerAssignment` and inherits generation, re-push and echo for free. "Teardown" is the **inverse of registration**: every handler the meeting was registered on, after the last push has returned. The instance nouns (mute, EndMeeting) sit on two existing mechanisms, so no parallel machinery is needed. Wider class checked: every `RegisterMeeting` field MC fills is already derived from `HandlerAssignment`. The muted set joins that class; it is not a sibling channel.

### 1. Muted set in every snapshot (R-9)
- `HandlerAssignment` gains `server_muted_sources: BTreeSet<SenderId>`. It is a set, so there are no duplicates, and it is sorted, so snapshots are deterministic.
  - `PolicyGenerations::next_generation` already derives from structural equality of `HandlerAssignment`. A mute change therefore advances the generation, on exactly the handlers whose set changed, with no second "did the mute change" decider.
  - The pusher re-push and applied-echo confirm are unchanged.
- `SlotTable::render(handlers, server_muted: &BTreeSet<SenderId>)`. H's set = the muted senders that are a `candidate_source` of at least one egress stream in H's rendered `egress_streams`. That is the edge-ownership rule already written at `meeting_media.rs:525`, and I delete that note when I implement it.
  - Consequence: `|set| ≤ |egress_streams| ≤ MH_MAX_EGRESS_STREAMS_PER_MEETING ≤ MH_MAX_MUTED_SOURCES_PER_MEETING`, because MH boot refuses `muted < egress` (e6c0a248).
  - The chain is stated in code. So "host mutes everyone" cannot produce a registration MH rejects.
  - A muted sender with no edge on H forwards nothing there. When it gains an edge, the same snapshot carries its mute: atomic with the edge.
- **Membership = `audio_server_muted` ONLY.** Story 2 is audio-only and MH mute is sender-scoped (it drops all streams). Adding a sender for `video_server_muted` alone would drop their audio. A one-line comment at the site says per-kind mute moves to MC's egress edge set in story 3.
- **Not `SourceMuteView` / `RosterEntry.audio_muted`.** Those are `self || server`, the one wire `SOURCE_MUTED` state. Putting a self-muted participant in MH's ingress-drop set would be a bug the user could never lift. A boundary note goes at both definitions.
- The one home for the mute flags stays `Participant` (actor). A new actor helper `reconcile_media(affected)` computes the server-muted `BTreeSet<SenderId>` from `participants` and calls `media.reconcile(affected, &muted)`. Every existing `self.media.reconcile(..)` call site goes through it, so no second mute store exists in `MeetingMedia`.
- `mh_client.rs` maps the set to `server_muted_sources: Vec<MutedSource>` and deletes the "Empty is the TRUTHFUL value today" placeholder. `register_meeting` and `end_meeting` share one private `connect_handler()`, which reuses `MH_CONNECT_TIMEOUT` / `MH_RPC_TIMEOUT` / `add_auth` (DRY-7).
- Mute change = structural change. `handle_server_mute` calls `reconcile_media(Affected::HoldersOf(sender))` when the audio flag moved. That re-renders every handler, so the generation advances where the set changed, the pusher re-pushes, the echo is confirmed, and `StreamAssignments` is re-emitted to holders (the R-4 third driver, already wired). A video-only change updates state and broadcasts but does not recompose.
- **State gauge (O-1, closes `docs/TODO.md` "never as STATE"; shape revised per O-12 + code-reviewer Q2): recompute-and-`set`, never delta arithmetic.**
  - A shared `MutedSourceCensus` (`Arc`, per-meeting count) in the register of `KekLifecycle`. Each meeting actor writes its own **absolute** count on every mute change — there is no delta to get wrong and no remainder to subtract on exit.
  - `mc_media_server_muted_sources` is the pod SUM, recomputed from live state by ONE private fn called from two places: **(b) `check_meeting_health()` (`controller.rs:854`), the LOAD-BEARING caller** — it already walks the meeting registry, so it prunes census entries for meetings that no longer exist and re-`set`s the total, making any missed path or arithmetic error self-correcting on the next pass; and **(a) on every mute change**, which is freshness between health checks only. A comment says which is which: deleting (a) leaves the gauge stale but correct, while deleting (b) silently restores the drift class.
  - `set(0.0)` at boot beside `set_receive_slot_cap`, so absent ≠ zero (O-13); the catalog row says "present at zero from process start".
  - This is what makes the gauge safe to read against MH's `server_muted` drops in the new runbook scenario: an over-reading gauge would manufacture the very MC-vs-MH disagreement it exists to detect.

### 2. Authority, refusal, rate bound (R-8)
- **Connection layer.** `MediaSignalingContext` gains `is_host: bool`, taken from the ONE derivation at `connection.rs:522` (`claims.role == MeetingRole::Host`), never re-derived. It also gains a third `ClientWorkLimiter`, `server_mute_limiter`, built from `MUTE_WORK_BURST` / `MUTE_WORK_REFILL_INTERVAL_MS`, with a `const` assertion that it is no looser than the client-mute bucket. The limiter type doc changes from "two instances" to "three".
- **Order, stated as the security property:** authority → limiter → actor (target resolve → unchanged).
  1. A non-host is refused before any actor hop and before `request.participant_id` is read. Counted `not_permitted`.
  2. The generic error reply is rationed by the existing `rejection_reply_limiter`. The counter is unbounded, the reply is bounded (security-5), and the WARN is latched once per connection (ops-6).
  3. For a host, a token is spent, then the actor hop. The requester is `media.participant_id` from the authenticated connection, never a message field. A comment at the call site says the actor cannot tell a forged `muted_by` from a real one, so this binding is made only here (security-2).
- **Actor layer (defence in depth, AND-composed).** `handle_server_mute(target, requester, audio, video) -> Result<ServerMuteApplied{Applied|Unchanged}, ServerMuteRefused{NotPermitted|UnknownTarget}>`.
  - Authority (`participants[requester].is_host`, fail closed) is checked strictly before the target lookup.
  - A comment records that `handle_reconnect` does not refresh `is_host`, so the two layers can read different vintages of the role. AND-composition makes that fail closed.
- **One client-visible refusal** for both refusal kinds: same `ErrorMessage` code (`PERMISSION_DENIED`) and same static text "Server mute request refused". The distinction survives only in the metric label and the server log (security-1).
  - Replaces today's distinguishable `PermissionDenied("Only hosts…")` / `ParticipantNotFound("Target participant not found")`.
  - The actor returns a typed refusal. The connection maps it to the one wire error.
- `reason` (client-controlled) is **not propagated or logged** (security-6).
- **Decision log** on the signalling path: INFO for applied/unchanged, carrying `requester_participant_id`, `target_participant_id`, `action`, `outcome`. Refusals are WARN, latched once per connection. No `sender_id`/`user_id`/`display_name` (O-10).

### 3. UnmuteRequest notifies, never clears; compose rule (R-10)
- Dispatch arm: its own fourth `ClientWorkLimiter` (tighter: it fans out into another participant's mailbox), then `meeting_handle.request_unmute(participant_id, audio, video)`.
- The actor has **no write path to `*_server_muted`** from this message (single writer = `handle_server_mute`).
  - It relays `ServerMessage::UnmuteRequest{participant_id (F-A), request_audio, request_video}` to every connected `is_host` participant, only if the requester is actually server-muted in a requested kind.
  - Otherwise nothing is relayed. Outcome counted.
  - MC builds a FRESH relay message, so a client-sent `participant_id` can never be forwarded (unit test with a forged value).
- Compose: `handle_self_mute` writes only `*_self_muted`; `handle_server_mute` writes only `*_server_muted` (+ `server_muted_by`). The slot view reads `self || server`; MH reads server only. Unit tests cover both directions.
- **UnmuteResponse:** left unhandled (ignored + debug), and said so in the dispatch doc. The host lifts with `ServerMuteRequest{audio_muted:false}`, so there is one lift path under one authority.

### 4. Who-muted-whom on the wire, live + late joiner (R-11)
- `ParticipantStateUpdate::MuteChanged` gains `server_muted_by: String`. `encode_participant_update` gets a real arm → `ParticipantMuteUpdate`, the ONE serializer (DRY follow-up).
  - New `payload_kind` constant `participant_update_mute`, derived beside the encoder as the metrics doc requires.
- `Participant` gains `server_muted_by: Option<String>`, written only by `handle_server_mute` and cleared in lockstep when both server flags are false. There is no side map.
- **Live:** `handle_server_mute` broadcasts to **all** participants including the target, so the muted participant learns it can't self-clear. Today it excludes the target.
- **Replay:** the join turn snapshots `MuteChanged` values for every already-server-muted participant into `JoinResult.server_mute_replay: Vec<ParticipantStateUpdate>`.
  - `connection.rs` writes them through the same encoder immediately after the `JoinResponse`, before the bridge loop.
  - Later deltas are queued on the outbound channel from the join turn onward, so order is snapshot-then-deltas by construction.
- Self-mute still emits no `ParticipantMuteUpdate` (S-2). A comment at the emitting site says fields 2-3 ride along and are informational, not a self-mute source.
  - The docstrings on the three "no self-mute fan-out" tests that justify themselves by "MuteChanged is not wire-serialized" are corrected; their assertions are unchanged.
  - `participant.rs` / `handler.rs` tests pinning "MuteChanged not serialized" are rewritten to assert the frame.
- **Reconnect/rejoin:** no new mechanism. Mute lives on the roster `Participant`: reconnect keeps it, a fresh join builds `false`. Tests only. No "ban" vocabulary anywhere.

### 5. Metrics (O-2..O-8)
- `mc_media_server_mute_requests_total{action, outcome, key_custody}`.
  - `action ∈ {mute, unmute}`: `mute` if either flag is set, else `unmute`.
  - `outcome ∈ {applied, unchanged, not_permitted, unknown_target, rate_limited, actor_unavailable}`.
  - `ServerMuteOutcome` lives in `media_signaling/outcome.rs` beside `MuteOutcome`, with a non-collapse note.
  - It partitions requests. The failure predicate is positive: `outcome=~"rate_limited|actor_unavailable"`. `not_permitted` is client-inflatable and never in a denominator; the wording is reused from `unchanged` / `accepted_unchanged`.
  - `unknown_target` is reachable only for a host requester.
  - Zero-initialized as the full cross product; unreachable cells named in the catalog.
- `mc_media_unmute_requests_total{outcome, key_custody}`, `outcome ∈ {relayed, not_server_muted, no_host_connected, rate_limited, actor_unavailable}`. A separate family because the decider is different (participant, not host). @observability to confirm.
- `mc_media_end_meeting_total{outcome, key_custody}`. One increment per (meeting, handler) call. No handler label; `handler_id` goes in the log line.
  - `outcome ∈ {released, mc_id_mismatch, unimplemented, unavailable_exhausted, invalid_argument, error}`. `released` = ack (release OR unknown-meeting no-op; indistinguishable by design).
  - Plus `mc_media_push_quiesce_total{outcome ∈ {quiesced, timed_out}}` per meeting teardown, or folded as an EndMeeting pre-outcome, per @observability.
- `action` shared-label row in `docs/observability/label-taxonomy.md` (O-2; @observability owns the text).
- Catalog rows in `docs/observability/metrics/mc-service.md`. `mh-service.md` §`server_muted` bullets 2-3 flipped, with @observability wording (O-9).
- Dashboard: server-mute panel in Client Media Signalling; EndMeeting + quiesce panels in MH Coordination; muted-sources gauge panel.
- Alerts:
  - `MCEndMeetingMcIdMismatch` (`> 0`, MC defect).
  - `MCEndMeetingFailureRate` (sustained, non-zero-denominator guard).
  - Explicit recorded "no alert" on the server-mute counter.
- Runbook (@operations):
  - New Scenario "EndMeeting failing → MH edge budget ratchet".
  - New Scenario "host muted someone and MH still forwards" (MC gauge vs MH `server_muted` drops vs policy-push outcomes).
  - §Client media signalling enumeration regex fixed (O-9 / TODO:1071 trap).
  - `mc-deployment.md` §Coordination/rollback: EndMeeting rollout order (MH first; rolling MC back removes teardown, and MH keeps budget until restart).

### 6. Teardown (R-20) — E2, off the reap path (revised for OPS-1 / code-reviewer Q1)
- **Rename (DRY-9, blast radius confirmed = 4 MC src sites, zero callers):** `MeetingMessage::EndMeeting` / `handle_end_meeting` / `MeetingActorHandle::end_meeting` → `CloseMeeting` / `handle_close_meeting` / `close_meeting`. `EndMeeting` then names ONLY the MH RPC. Every other `.end_meeting(` in the tree (`mh-service/**`, `env-tests/**`, `mc-test-utils/src/mock_mh.rs`) is the MH RPC and is deliberately NOT swept; the commit message says so, so a later "consistency" pass cannot undo the distinction. `LeaveReason::MeetingEnded` is untouched.
- **Pushers get their own token, NOT a child of the actor cancel.** `MeetingMedia` owns a `pushers_cancel: CancellationToken`; `HandlerPusher::spawn` takes a child of THAT. This is what makes a clean drain possible at all: today pushers are children of `cancel_token`, so the old `handle_end_meeting`'s `cancel()` would kill an in-flight attempt, which the proto forbids. An actor that panics still drops `MeetingMedia`, and `HandlerPusher::Drop`'s abort remains the unclean-path stop.
- **Trigger points**, all three converging on one `teardown()` exit path:
  - (a) `remove_and_broadcast_left` leaves `participants` empty (clean leave, clean close, grace expiry; a grace member still counts as present, so a reconnect inside W is unaffected).
  - (b) explicit `close_meeting`.
  - (c) MC graceful shutdown (Lead ruling). Safe against failover: a successor MC that already re-registered makes our release fail `FAILED_PRECONDITION` (`mc_id_mismatch`, counted, never retried), so it can never release a taken-over meeting. GC is NOT notified on this path — the meeting is not over, and GC learns of MC loss from heartbeats.
  - A **panic or SIGKILL** cannot tear down: the recorded residual (§8).
- **The exit path does NO slow work (OPS-1).** In order, with nothing awaited that can block:
  1. `is_shutting_down = true` (further joins get `Draining`).
  2. MOVE the pushers out of `MeetingMedia` (`std::mem::take`) together with `pushers_cancel`, the frozen `MeetingHandlers` and `mc_id`, into a `MeetingTeardown` job. Moving them out is what makes `Drop::abort` unreachable for a pusher with an in-flight attempt (code-reviewer Q1).
  3. Send `MeetingTeardown` to the **teardown worker** (a single long-lived task spawned in `main.rs`, holding `Arc<dyn MhRegistrationClient>` + `Arc<PolicyGenerations>`, observing the shutdown token).
  4. Send `MeetingEnded{meeting_id, cause}` to the controller and break the loop. `cancel_token.cancel()` fires only after the pushers have been moved out, so it cannot reach them.
  The actor is therefore finished, and the controller reaps, in ~one channel hop — the ~36 s rejoin window OPS-1 identified does not exist.
- **The teardown worker** does the slow, ordered work off every actor's loop:
  1. **Quiesce**: cancel `pushers_cancel`, then `await` every `JoinHandle` — never abort, so every issued attempt has RETURNED (the proto MUST (b)). The worker observes cancel only at attempt start, in backoff and while idle, never mid-RPC, so at most ONE attempt can be in flight.
  2. Bound: `PUSH_QUIESCE_BOUND = MH_CONNECT_TIMEOUT + MH_RPC_TIMEOUT + QUIESCE_SLACK`, exposed as ONE derived `pub const` from `grpc/mh_client.rs` (which owns both inputs) rather than re-declared at the use site (OPS-7). `QUIESCE_SLACK` is a named const with a one-line reason: it covers the worker's own post-RPC bookkeeping between the RPC returning and the task ending, so a healthy drain never trips the bound.
  3. On expiry: `mc_media_push_quiesce_total{outcome="timed_out"}`, ERROR, and EndMeeting is **still sent** — a skipped release is a guaranteed leak. But it is sent **ORDERED, not immediately** (paired-MH's Race B): the worker first waits **exactly `PUSH_QUIESCE_BOUND`** (the SAME const, reused rather than a parallel sum of the same terms, so ONE startup field — `push_quiesce_bound_seconds` — names both the drain bound and this wait, and the runbook points at that field instead of carrying a number) so that any attempt still in flight at cancel time has certainly had its client deadline expire and its server task cancelled, after which no further upsert can be enqueued. Without that wait, an in-flight registration's upsert can land AFTER the release on MH's lifecycle mailbox — enqueue order between two independent tonic server tasks is scheduling, not wire order — re-registering the meeting so that its apply installs legitimately, leaving routes and edge budget under this `mc_id` that no MC will ever release. MH cannot detect that case (a re-registered meeting with routes is indistinguishable from a live one), so ordering is MC's obligation. This wait is on the timeout path only; a healthy drain never reaches it.
  4. `end_meeting_plan(meeting_id, mc_id, &MeetingHandlers, Quiesced) -> Vec<EndMeetingCall>` — a pure fn in `media_routing/teardown.rs`, covering EVERY handler in the frozen set including ones never pushed to. The `Quiesced` witness is constructible ONLY by the quiesce step, so "after the last push returned" is a type obligation, not a comment.
  5. `join_all` the calls, classify each, count per (meeting, handler).
  6. **Then** evict `policy_generations` for the meeting — after the quiesce, as the proto requires.
  7. Report completion to the controller.
- **Classification** (`classify_end_meeting`): Ok → `released` (release and unknown-meeting no-op are one outcome by design); FAILED_PRECONDITION → `mc_id_mismatch`, no retry; UNIMPLEMENTED → `unimplemented`, no retry, non-fatal; UNAVAILABLE/transport → bounded retry (`MAX_END_MEETING_ATTEMPTS = 2`, backoff reused from `REGISTER_BACKOFF_DELAYS[0]`) then `unavailable_exhausted`; INVALID_ARGUMENT → `invalid_argument`, no retry. The `tonic::Status` code and message are carried into the returned error, never log-and-discarded (semantic-guard 5).
- **`MhRegistrationClient::end_meeting` is a REQUIRED trait method with no default body** (DRY G1-4), so the compiler enumerates all FOUR implementors: `MhClient`, `MockMhRegistrationClient`, `ScriptedMh`, `ConfirmingMh`. A defaulted `Ok(())` would make the "no EndMeeting before pushes stop" assertion vacuous.
- **Rejoin fence — the race E2 creates, and why a fence is required, not optional.** `EndMeeting` is unconditional by meeting id and carries no generation. If a rejoin re-created the meeting while a teardown for that id were still in flight, the release would land on the NEW registration: routes gone, MC believing it is registered, silent dark meeting until the next structural change. So the controller keeps `tearing_down: HashMap<String, Vec<oneshot::Sender<…>>>`, inserted when it reaps a cleanly-ended meeting and removed when the worker reports completion.
  - A `CreateMeeting` for an id under teardown is QUEUED on that entry (not rejected), and answered when teardown completes — so the common case (teardown is milliseconds) is added join latency, not an error. The queue is capped (`MAX_QUEUED_CREATES`); over the cap answers `Conflict` immediately.
  - The controller never awaits: it stores the `respond_to` and answers from the completion arm, so the message loop is never blocked.
  - Bounded by the worker's own finite worst case, which always reports completion — including on quiesce timeout and on every RPC failure.
- **GC notification (OPS-2).** The controller forwards cause ∈ {empty, closed} to a bounded `meeting_ended` channel; a task in `main.rs` calls the new `GcClient::notify_meeting_ended(meeting_id, region)`.
  - A full channel does NOT silently drop: it counts its OWN series (`mc_gc_meeting_ended_notifications_dropped_total`, distinct from the RPC outcome because "MC is losing events" and "GC is down" are opposite investigations) and logs at ERROR, with a `> 0` warning alert. It is generously sized, so this is expected-empty.
  - Bounded retry with backoff reusing the pusher's delays; a sustained failure leaks an active GC assignment row, so it is alerted (`MCNotifyMeetingEndedFailing`).
  - `mc_gc_notify_meeting_ended_total{status}` (coarse `success`/`error`, matching its `mc_gc_heartbeats_total` sibling), panelled with the GC heartbeat panels.
- **Code notes required by the task**, at the teardown site: (i) the story-2 assumption — no cadence re-assert exists yet, so quiesce-then-release ordering suffices; (ii) the story-4 note — a periodic re-assert racing a teardown needs a fence, cross-referencing `docs/TODO.md` §Media Path Obligations; (iii) a pointer to `docs/TODO.md:1515` for the crash residual; (iv) a cross-reference to paired-MH's `handle_config_apply` guard.

### 6b. Existing tests that assume a meeting outlives its last participant (Lead ask — audited, 11 hits)
Audited `crates/mc-service/{src,tests}` and `crates/env-tests`. A grace-window reconnect is unaffected (the participant stays on the roster). Every hit is fixed in-tree:
- **`actors/meeting.rs` unit tests (3)**: `test_meeting_actor_leave` (:2565), `test_disconnect_grace_period_expires` (:3106), `test_clean_close_skips_grace_removes_immediately` (:3262) each call `get_state()` after the only participant is gone. Fixed by asserting the meeting ENDED (the new, correct post-condition) instead of reading an empty roster — which is a strictly stronger assertion of the same behaviour.
- **`tests/media_admission_integration.rs::sender_id_is_not_recycled_across_a_leave_and_a_later_join` (:462)**: joins, cleanly closes, rejoins the same meeting. Its premise (one meeting spans the leave) now needs a second participant held open for the meeting to survive; the non-recycling property it tests is unchanged.
- **`tests/join_tests.rs` (2)**: `test_first_participant_triggers_register_meeting` (:1066) and `test_join_multiple_mh_handlers_offers_the_full_set_and_registers_all` (:1166) drop the connection and then wait for RegisterMeeting calls — under E2 the push races the teardown. Fixed with `join_keep_open`, the pattern their own sibling tests already use.
- **`env-tests/tests/26_mh_quic.rs` (6)**: the local `mc_join` helper (:426, used via `join_with_registered_mh` :476) DROPS the MC session on return, so tests :522, :576, :623, :661, :706 and :1045 then use a meeting whose only participant has cleanly left. Fixed once, in the helper — and by TYPE, not convention (@paired-media-handler): the helper returns ONE struct bundling the `McSession` with what callers actually want (the `JoinResponse` / MH url), so a caller structurally cannot hold the MH half without the MC half. "Return and hold it" would be convention-dependent, and `#[must_use]` does not save it: `let _ = mc_join(...)` drops immediately AND silences the warning, and that idiom is already in this suite. The helper doc states the MECHANISM, not just "keeps the meeting live": from this task the last participant's clean leave ENDS the meeting, and MC's teardown calls `EndMeeting`, which releases the meeting on the handler and closes its connections — so the next person simplifying the helper does not read the held session as ceremony, and a regression surfaces as a confusing transport error that would not otherwise point back here. The existing `:425` sentence ("the WebTransport connection is dropped ... which is what the `JoinResponse`-only callers want") becomes false for the converted callers and is rewritten to say which callers get which shape.
Nothing in mc-service calls the MC-side end-meeting path today, so the rename has no test fallout.

### 7. Tests (where each lands)
**Unit** (in-module):
- `meeting.rs`:
  - `non_host_refused_even_for_unknown_target` (authority first, same refusal variant for both kinds).
  - `host_unknown_target_is_generic_refusal`.
  - `unmute_request_never_clears_server_mute_and_relays_to_host`.
  - Compose rule, both directions.
  - Mute survives `handle_reconnect`; fresh join starts unmuted.
  - Late-joiner replay contents.
  - Last removal ends the meeting; grace member keeps it alive.
- `connection.rs`:
  - Dispatch refuses non-host with no actor hop (actor mailbox untouched), and the counter moves.
  - `server_mute_limiter` burst/refill, plus a const assertion ≤ client-mute bound.
  - Unmute-request limiter.
  - Generic wire error byte-identical for both refusals.
- `slots.rs`: per-handler filter = edge ownership, a sender spanning two handlers is in both, self-mute is never in the set, dedup/sorted.
- `generation.rs`-level: mute change advances only the changed handler.
- `teardown.rs`: plan covers every frozen handler incl. never-pushed, carries mc_id + meeting_id, and the classify table.
- `pusher.rs`: `quiesce` awaits a gated in-flight RPC (ScriptedMh `gated`) rather than aborting, and no attempt starts after quiesce, including one sitting in backoff.

**Integration:**
- `tests/server_mute_integration.rs`, over the real WebTransport + `MockMhRegistrationClient`:
  - Muted set in every snapshot (first push, re-push, re-assert after reconnect), filtered per handler.
  - Generation advances on mute and on unmute, echo confirmed.
  - `StreamAssignments` `SOURCE_MUTED` re-emitted to holders.
  - Non-host refused over the wire.
  - Late joiner receives `ParticipantMuteUpdate` right after `JoinResponse`.
  - Reconnect keeps / rejoin drops.
- `tests/meeting_teardown_integration.rs`, over the `mc-test-utils` mock MH gRPC stub. The stub gains an **interleaved event log** (register vs end, arrival order) and a FAILED_PRECONDITION knob. Assertions:
  - EndMeeting exactly once per handler, none extra, none missing.
  - No RegisterMeeting after the first EndMeeting.
  - A gated in-flight RegisterMeeting returns before EndMeeting is sent.
  - mc_id / meeting_id correct.
  - Mismatch is counted and not retried.
  - Controller reaps, and a re-created meeting pushes from generation 1.

**Env-test** `crates/env-tests/tests/35_mc_server_mute_teardown.rs`. Public APIs only, per-entity evidence, counters as `≥ own baseline + 1`, no shared-gauge values, bounded poll-until with distinct phase tokens, no wall-clock gates:
- **S3 server half:**
  - Host (meeting creator) + A + B join via GC/MC and connect to MH. Positive control: B's accepted-from-A frame count rises first.
  - Host server-mutes A. Then MH `server_muted` drop counter ≥ baseline+1 on A's handler (SECONDARY evidence only, pod-level and shared with env-test 26 S4; the assertion text says so), A's own transport egress non-zero, and every other receiver's accepted-from-A count flat across two successive own-connection observations.
  - Unmute restores forwarding. "Within one policy generation" is evidenced by B's own `StreamAssignments` returning ACTIVE and B's accepted-from-A count rising again.
- **Non-host:** B sends `ServerMuteRequest(A)` and gets the generic `ErrorMessage`; MC `…{action="mute",outcome="not_permitted"}` ≥ baseline+1.
- **Straggler coordination** (paired-MH B2: ONE straggler per handler in the frozen set, placed as env-test 27 does; each handler's probe is gated on ITS OWN straggler's (a). Never gated on MC's pod-level `end_meeting` counter):
  - Enough joins/declarations to reach generation ≥ 2 (checked via MC's `mc_media_policy_pushes_total` own-baseline delta ≥ 2 per handler, or the logged generation).
  - All leave through MC with a clean close, which skips grace. The straggler leaves MC but keeps its MH session open with keep-alive.
  - (a) the straggler's MH `closed()` resolves as an application/peer close, NOT a local idle timeout. MH closes by drop with no reason string, so no string match (paired-MH-8). A check confirms MC's leave path does not itself close the MH session.
  - Only then (b): ONE probe registration per handler at gen 1 as the separate MC principal (env-test 29's fixture, empty `egress_streams`) returns `applied_generation == 1`.
  - Positive control for (b): before teardown, the meeting's held generation ≥ 2 is established from MC's side, never by probing a live meeting. Echo 0 (config-apply back-pressure) and echo = held-gen (real regression) get split failure messages as in 29.
  - The probe is cleaned up with EndMeeting under the probe `mc_id` via a Drop guard.
- **"Teardown observed on MH's registered-meetings gauge"** conflicts with the README evidence rule ("never the value of a shared pod-level gauge"). I use the gen-1 probe + MH teardown counter `≥ baseline+1` + MC `end_meeting{released}` `≥ baseline+1` instead, and assert gauge PRESENCE only. @test to confirm.
- Compiled with `cargo test -p env-tests --features all --no-run`.

**MH hermetic check (3):** `end_meeting_from_the_registering_mc_releases_everything` already asserts the meeting-keyed registration, routes, generation and edge state (`mh_service.rs:635-638`); what it does NOT assert is connection closure or the muted set. Extended as @paired-media-handler specified: add a connection whose `close` token is cloned, register with a non-empty muted set, then assert `close.is_cancelled()`, `active_connection_count() == 0` and `!is_server_muted`, keeping the four existing assertions. @paired-media-handler drafts; I review.

### 8. Residuals recorded (not solved)
- A **panic or SIGKILL** with live meetings sends no EndMeeting (a clean shutdown now does). MH keeps the meeting's registration until its own restart, and since R-21 that consumes the enforced `MH_MAX_REGISTERED_MEETINGS` — progressive denial of NEW meetings with `RESOURCE_EXHAUSTED`, not slow memory growth. **Already filed at `docs/TODO.md:1515`; updated in place, never re-filed** (OPS-3): its "any MC before story 2 task 12" clause narrows to crash and killed-mid-teardown, and the detector is named as story-2 task 18's MH absent-event rule. A code comment at the teardown site points there.
- Late apply after a release — **two distinct races, and only one is closed MH-side.** Race A (a queued `ApplyPolicy` whose RPC already returned) is refused by paired-MH's B1 guard and is no longer a leak; the teardown-site comment cross-references it. **Its refusal is NOT counted, and saying otherwise was wrong** (@paired-media-handler's own correction): in Race A the handler already hit `MH_POLICY_APPLY_TIMEOUT_MS`, recorded `apply_failed` and returned — which is WHY the apply is still queued — so the guard's `RejectedStale` reply goes to a `oneshot` whose receiver is already dropped and nothing records it. The refusal's only evidence is the actor-side WARN. The carrying RPC was already counted ONCE, as `apply_failed` at timeout (pre-existing, unchanged). A `rejected_stale` count arises only in the in-order case where a release lands between upsert and apply inside one live RPC — which the quiesce exists to prevent, so it is the rare path, not this one. Counting the late refusal too would count one registration TWICE and break `mh_media_policy_applies_total`'s documented exactly-once boundary, so a truthful count needs a taxonomy decision (@observability's, being asked now); WARN-only is the default and is stated as such at every site. Race B (an attempt still genuinely IN FLIGHT when quiesce times out, whose upsert lands after the release and re-registers the meeting) is **invisible to MH by construction** and is ORDERED — NOT CLOSED — MC-side by the wait in §6 step 3. The distinction is deliberate: Race A's refusal is in-tree and unit-tested, whereas Race B's ordering rests on tonic cancelling MH's server handler when the client drops the request, a FOREIGN-LIBRARY property that fails OPEN (if it ever changed, nothing in this tree would go red). No comment on either side may read as a hard closure of B. §8 therefore does NOT claim "the late-apply case is closed", and the MH guard's own doc comment is worded to match the coverage it actually has.
- **Race B's control is UNOBSERVABLE, and that is an accepted story-2 position ONLY because it is written down** (@operations + @observability both asked for this on the record). This diff ships TWO controls over the teardown/apply race: Race A's refusal now notifies (`mh_media_stale_apply_refusals_total`, @observability approved it; an mh-service metric owned by @paired-media-handler), while Race B's ordering **cannot be observed at all** — "the wait was performed" is not the same fact as "the late registration did not land", and there is no counter for the second. It fails open: if the RPC layer stopped cancelling a dropped request's server handler, nothing would turn red. Its only trace is `mh_media_registered_meetings` rising with pod uptime while teardowns stay flat, whose alert is story-2 task 18's and does not exist yet. Cross-referenced to `docs/TODO.md:1515`, whose ratchet signature is the compensating detector. Race B is the more dangerous of the two precisely because it is the one that fails open.
- **The guard counter's name is guard-pinned at its recording, test, catalog and dashboard sites but CONVENTION-ONLY at four prose sites** (`internal.proto:801-805`, `session/mod.rs:911-913`, `docs/TODO.md:1513`'s pointer chain, `mh-incident-response.md`). A silent rename there leaves a PromQL query returning no-data, which is indistinguishable from healthy-and-flat. So the recording site carries an `ANCHOR (DRY): <final name>` comment naming those four consumers, making `grep -rn 'ANCHOR (DRY):'` surface the rename obligation (@protocol + @dry-reviewer; recording site owned by @paired-media-handler). No site lands with a placeholder name.
- **A story-4 SEMANTIC INVERSION to hand to task 18's owner with the absent-event rule.** A non-zero `mh_media_stale_apply_refusals_total` means, in story 2, that the proto's ACCEPTED deadline residual occurred and the guard caught it — informational, deliberately no alert, because alerting would page on a decision taken on purpose. Once story 4's fence lands, the same series means the FENCE FAILED and needs a rule behind it. The two must arrive together, or the rule lands against the story-2 reading. Precedent for why this is filed in advance: `docs/TODO.md:1477` records three sites that today say "counting up is expected — do not open an incident" about `no_generation`, and retiring that prose when the semantics flipped had to be filed as an obligation because nothing fails if it is forgotten.
- Muted set rejected by MH (`too_many_muted_sources`): structurally unreachable per §1's chain. If it happens anyway it surfaces as the push's `invalid_argument` gRPC error, so the pusher's error log gains the status code field (ops-4).


---

## Planning — Gate-1 amendments

Every reviewer item, keyed by its id. Where an amendment conflicts with §1-§8 above, the amendment governs; §6 and §6b were rewritten in place rather than amended.

### Security
- **S-11 (wording, not mechanism).** The comment at the refusal site states: *the ORDERING (authority before any target read) is the control; the single wire error is defence-in-depth.* It collapses the one case where the two layers genuinely disagree (the `handle_reconnect` role-vintage case) and stops the property evaporating if someone reorders the checks. It does NOT claim to close a cross-meeting existence oracle — a non-host already learns nothing because of the ordering, and a host distinguishes refusal from success inherently. Same correction in the catalog row.
- **S-12.** Server-mute refusals get their OWN reply bucket rather than sharing `rejection_reply_limiter`: sharing would let an authorization probe drain the budget that delivers capability rejections, and the shared bucket's rationale ("a correction that converges") is not true of a probe. That makes five `ClientWorkLimiter` instances, so the type doc states the PROPERTY (each bounded path is independent, so no path can suppress another's) instead of enumerating one pair (also DRY G1-2, which correctly counted four before this).
- **S-13 / O-15.** A suppressed refusal reply gets its own series, NOT an `outcome` value — a value would break the partition and under-count `not_permitted` exactly when a probe is most active. `mc_media_refusal_replies_suppressed_total{surface, key_custody}`, `surface ∈ {server_mute, capability}`, incremented at the `try_spend` failure site. **It covers the capability path in the same stroke**: `reject_capability` (`connection.rs:1297`) has the identical invisible control today (a latched `bool`, no counter), and counting one surface but not the other would be the half-applied invariant. ~5 LoC in a file already being edited. Expected-empty, zero-init, no alert (client-inflatable), and the row reuses the outbound-drop catalog's "every drop is counted, only the first is logged" wording, because it is the same argument.
- **S-14.** `participant_id` is overwritten UNCONDITIONALLY at the dispatch boundary — never "use the client value if non-empty", never validate-and-accept — because the message crosses in both directions and a fabricated id would make MC relay "X is asking to be unmuted" to a host who would act on it. The proto comment says "server-stamped on relay, ignored on receive". The Owner cell reads `protocol` (the manifest value); `security` is on the panel per the §6.4 intersection rule rather than in the Owner cell. `packages/sdk-core/src/proto/**` stays Mechanical strictly as byte-for-byte generator output.
- **S-15.** The restored `MuteChanged` roster fan-out is O(roster) awaited sends on the shared actor. The site states the factor AND **names the comparator the argument rests on**, so the claim is falsifiable and so that bounding the membership fan-out later visibly invalidates this premise (the same stale-justification class as "`MuteChanged` has no consumer"): the comparator is `broadcast_update`'s existing unbounded use on `Joined` (`meeting.rs:1222`), `Disconnected` (`:1351`), `Left` (`:1399`), `Reconnected` (`:1647`) and per-participant at meeting end (`:2065`) — none behind a limiter, and `Reconnected` reachable on transport churn. A host-only path at 8 burst / 4 per second sustained sits strictly inside that envelope. Plus one clause that each send lands in a per-participant BOUNDED mailbox that counts its own drops, which is what makes the failure mode bounded and observable rather than unbounded growth. No new mechanism.
- **S-16.** A one-liner at the `server_muted_by` storage site: MC is the PRODUCER (its own UUID from the authenticated connection, never parsed off the wire), so there is no emit-side truncation — adding one would be a no-op that reads as though the receiver obligation had been discharged, and that obligation is the TS client's.
- **S-17 question — confirmed, no authority change.** `MeetingRole::Host` is a claim AC stamps from GC's record of the meeting CREATOR. MC only reads `claims.role`; it never derives or stores host-ness beyond the per-join `is_host` flag. E2 ends the *assignment* row (GC's `end_assignment` soft delete), not the meeting or its creator, and nothing in E2 re-derives a creator. So the last participant to leave and rejoin comes back as host if and only if they created the meeting — unchanged.

### Observability
- **(a) confirmed.** `mc_media_unmute_requests_total` is separate, with the sharper reason recorded in the non-collapse note: merged, `action="unmute"` would name a host LIFTING a mute and a participant ASKING to be lifted in one series. Unit is one increment per REQUEST, not per host relayed to. Success is `relayed`; `not_server_muted` and `no_host_connected` are routine, so the failure predicate is positive (`rate_limited|actor_unavailable`); `not_server_muted` is client-driven and stays out of denominators. Per-host relay failure is the later hop and lives on the outbound-drop / slot-view-emission series, stated as disjoint. No `action` label.
- **(b) confirmed.** `mc_media_push_quiesce_total{outcome, key_custody}` stays separate: the units differ (once per meeting teardown vs once per (meeting, handler)), and "quiesce timed out" and "released everywhere" are routinely both true, so folding would make one unrepresentable. The row records that this one IS a clean denominator (both values server-driven), contrary to the surrounding rows. It gets a warning alert as the leading indicator of the edge-budget ratchet.
- **(c) confirmed, renamed.** `participant_update_muted` (past participle, matching `_joined`/`_left`, so `payload_kind=~"participant_update.*"` still recovers the merged series). Justified on demonstrated CONSUMER NEED, not message-type taxonomy: a dropped `ParticipantMuteUpdate` has no re-sync path — join/leave state is rebuilt from the roster, but who-muted-whom is delivered only by the live broadcast and the late-joiner replay, so a drop leaves that client rendering a permanently wrong indicator until the next mute change on that participant, which may never come. Distinct consequence, distinct remedy. Catalog `payload_kind` value list updated.
- **(d) confirmed.** `mc_gc_notify_meeting_ended_total{status}` with coarse `success`/`error` matching its `mc_gc_heartbeats_total` sibling (fire-and-forget, remedies do not differentiate), panelled with the GC heartbeat panels, plus a `MetricAssertion` test. The channel-full drop is a SEPARATE series (see §6).
- **O-12 / O-13** — see the revised §1 gauge bullet: recompute-and-`set`, load-bearing caller named, boot init kept.
- **O-14.** Four premise-death corrections. The CONCLUSIONS stay; only the stated causes change, because a sentence that stays true while its reason goes false is how the fan-out gets re-added later: `observability/metrics.rs:995` ("`MuteChanged` had no consumer" → the fan-out was removed on S-2's judgment that nothing needed it; it is still not a meeting-wide contention signal), `actors/meeting.rs:1782` (same), and `:2883`/`:2895`, whose docstrings currently JUSTIFY "no self-mute fan-out" by "`MuteChanged` is not wire-serialized" — rewritten to rest on S-2's judgment instead. Replacement text goes to @observability for review, with the `mh-service.md` §`server_muted` bullet 2/3 flip.

### Observability (round 3 — O-16..O-18 on the replacement text, all accepted)
- **O-16.** The fifth premise-death site is `meeting.rs:2883-2885`, the inline comment that sets up the REAL-transition assertion. It is false twice over after this diff, because the broadcast would now deliver real bytes to real clients. It is rewritten with @observability's suggested text. The no-op assertion message at `:2878-2881` keeps its reason and stays as is. Without this fix, the comment and the `:2895` assertion three lines below it would contradict each other.
- **O-17.** The `mh-service.md` item 2 wording "a deployed zero means no mute is in force" is withdrawn. It is a drop counter, so zero has two causes: no mute in force, OR a mute in force over a source that is silent, disconnected, in grace or not yet on that handler. The new wording is: a zero means no mute is being ENFORCED AGAINST AN ACTIVELY SENDING SOURCE, and `mc_media_server_muted_sources` is what separates the two causes. That also gives the gauge a question only it can answer.
- **O-18.** The MC-gauge-vs-MH-counter comparison is stated as **fleet-aggregate and directional**:
  - The comparison is `sum(mc_media_server_muted_sources)` across MC instances against `sum(rate(mh_media_frames_dropped_total{reason="server_muted"}[…]))` across MH instances. Per instance it is not a signal at all, because MC and MH meeting sets do not nest.
  - A level and a counter are NEVER divided. Gauge > 0 while the fleet drop rate is flat is only a candidate disagreement, and only after O-17's silent-source cause is excluded.
  - The text cross-references `mc_media_generation_divergence` and the env-tests README rule, so the reader sees a known class rather than a quirk of this metric.
  - The runbook's mute scenario uses the same framing. "This pod's meetings" is removed from item 2.
- **Closing `docs/TODO.md:1547` WITH its residual named, not as an unqualified close.** The gauge answers "is any mute in force on this MC pod". It does NOT answer "is a mute in force in THIS meeting", because §11 bars that dimension by design. The meeting-scoped question is answered by MC's assignment state and the `mc.actor.meeting` decision log, not by a metric. The catalog row says the same.
- **Noted, not in scope:** the `meeting.rs:2874-2890` tests gate on `sleep(250ms)` wall-clock waits. @observability has raised it with @test. This task only edits the docstrings on those assertions.

### Observability (round 4 — O-19..O-21 on the rejoin fence)
- **O-19 accepted.** A new `McError::MeetingTeardownInProgress` variant, so the over-cap fence refusal gets its own `error_type` value (`teardown_in_progress`) on `mc_session_join_failures_total` automatically — one enum variant plus its `error_code()` / `error_type_label()` arms, no new metric and no new label, and the existing catalog row, panel and `MCHighJoinFailureRate` breakdown pick it up free. Without it, "clients are double-joining" (a client defect, no server action) and "teardowns are wedging and rejoins are refused" (an MC fault denying joins) share the `conflict` series, which are opposite investigations — the same criterion that split the GC-notify drop from its RPC outcome. **The WIRE code deliberately stays `CONFLICT` (5)**, so no client changes and no new client-visible vocabulary; only the operator-facing label splits. The catalog row gains the value with a one-line meaning.
- **O-20 accepted in substance; counter-proposing ONE placement detail.** Agreed it must be counter-visible and must not be ERROR-log-only, agreed it is expected-empty and a legitimate `> 0` alert candidate (severity past @operations), and agreed the hang half is the dangerous one that `mc_actor_panics_total` does not cover. But it cannot be a third value on `mc_media_push_quiesce_total` without breaking that counter's partition: `{quiesced, timed_out}` is recorded by the WORKER at quiesce time, whereas the backstop is the CONTROLLER's fence timer firing later, so a single teardown can legitimately be both `timed_out` and backstop-lifted — folding them either double-counts one teardown or silently drops the `timed_out` record. A "backstop supersedes timed_out" precedence rule is not implementable across two components at two different times without coordination that buys nothing. So: `mc_media_teardown_fence_backstop_total{key_custody}`, cardinality 1, expected-empty, zero-init, `> 0` alert, with the row stating it means the fence was lifted by its deadline rather than by the worker's report, i.e. the worker hung or died without reporting. @observability to confirm the placement; their three substantive requirements are met either way.
- **O-21 accepted, and the symptom framing is the important half.** No queue-latency histogram: queued-then-served latency already lands on `mc_session_join_duration_seconds`, and the timeout path's duration is fully derived from constants, so a histogram would measure arithmetic. What the runbook's EndMeeting scenario owes is that OPS-1's user-visible symptom SURVIVES on the timeout path by a different route: for a bounded, derived window every rejoin of that meeting id is queued or refused. The scenario states the derived worst-case fence-hold in terms of its constants (`PUSH_QUIESCE_BOUND` + one attempt window for the Race-B wait + the EndMeeting attempt budget) and names the signature explicitly — `mc_media_push_quiesce_total{outcome="timed_out"}` rising alongside O-19's new `error_type` IS "the fence is wedged". Constants and metric names are mine; the text is @operations'.

### Operations
- **OPS-1 — resolved by taking your shape.** EndMeeting is off the reap path entirely (§6): the actor moves its pushers to a teardown worker and finishes in one channel hop, so the ~36 s window does not exist. The residual serialization that remains is the rejoin fence, which is a correctness requirement rather than a latency artifact (EndMeeting is by meeting id with no generation, so a release must not cross a re-creation). A rejoin during teardown is QUEUED and answered, not refused, so the common case is milliseconds of added join latency; the cap and the bound are derived and stated, and the runbook scenario names the symptom.
- **OPS-2 — answered in §6**: own series for the channel-full drop with ERROR and a `> 0` alert (never a silent `try_send` drop), bounded retry reusing the pusher delays, and `MCNotifyMeetingEndedFailing` for a sustained failure.
- **OPS-3 — accepted, and I will NOT file a new entry.** `docs/TODO.md:1515` is updated IN PLACE: its "any MC before story 2 task 12 begins calling it" clause narrows to the two surviving cases (crash, killed mid-teardown), and the detector is named as **story-2 task 18's** MH rule (`mh-alerts.yaml:47-56`). The teardown-site comment points at 1515. §8 is corrected to say this consumes the enforced `MH_MAX_REGISTERED_MEETINGS` (progressive denial with `RESOURCE_EXHAUSTED`), not slow memory growth. The runbook scenario says an operator reads `mh_media_registered_meetings` against its limit until task 18 lands, and stops implying an edge-headroom alert already exists.
- **OPS-4.** One line in `mc-deployment.md` §Coordination: mid-roll, a new mc-0 ends empty meetings while an old mc-1 does not, so per-instance active-meeting, MH registered-meeting and GC assignment counts legitimately diverge for the duration — which matters because §Pre-Deployment Verification gates on that count.
- **OPS-5.** The §Client media signalling regex gains `mc_media_server_mute_requests_total` and `mc_media_unmute_requests_total` ONLY. EndMeeting + quiesce go in the new EndMeeting scenario and the gauge in the new mute scenario, respecting the section's own scope boundary. The "Five questions" count is DROPPED from the heading rather than incremented.
- **OPS-6.** The teardown worker observes the shutdown token, so a meeting emptying as SIGTERM lands cannot hold the pod past its grace into SIGKILL; it collapses into the recorded crash residual instead. The worst-case exit duration is written next to the 30 s / 35 s figures so the coupling is visible to whoever changes either.
- **OPS-7.** Done — `QUIESCE_SLACK` is named with a reason, and the bound is exposed as one derived `pub const` from `grpc/mh_client.rs` rather than re-declaring either input.

### Operations (round 2 — OPS-8..11, all accepted)
- **OPS-8 — the fence must not be able to wedge.** Two halves, both structural rather than happy-path:
  1. **Park, never await.** The controller STORES the create's `respond_to` in the `tearing_down` entry and returns from the message handler immediately; the answer is sent from the completion arm of the same loop. A comment at the park site says why the responder is stored rather than awaited: the completion signal arrives on this same select loop, so awaiting it inline would self-deadlock the WHOLE MC instance (every meeting on the pod), not just this id. That is exactly the invariant a later "simplification" reverts because `await` reads as more direct.
  2. **Completion is unconditional.** The worker reports completion from a `Drop` guard, so a panic between taking a job and finishing still lifts the fence (also code-reviewer's Gate-3 check: ONE completion report on ALL exit paths, including quiesce timeout and every RPC classification, not one per happy path). Belt and braces: the fence carries a deadline derived from — and strictly above — the worker's own worst case, so a lost report degrades to "fence expires, creates proceed" rather than "queued forever", counted under its own outcome. The tradeoff is stated at the site: fence expiry re-opens the release-crossing-a-re-creation race, so it is deliberately reachable ONLY when the worker has already exceeded a bound it structurally should not (the `Drop` guard covers panic and every return), and permanent per-meeting unjoinability is the worse of the two failures.
- **SUPERSEDED at implementation (recorded at Gate 3, O-31):** the bounded-channel worker below was replaced by a per-teardown detached task (`hand_off_teardown`), so there is no job queue and `mc_media_teardown_jobs_dropped_total` / `MAX_CONCURRENT_TEARDOWNS` have no referent and were not added. The concurrency bound is answered by the arithmetic at `hand_off_teardown` (OPS-16), accepted by @operations and @observability; graceful shutdown now waits for those tasks (OPS-18).
- **OPS-9 — the teardown job channel gets the same treatment as the GC-notify channel.** A dropped job means the meeting ended, the pushers were quiesced, and EndMeeting is never sent — `docs/TODO.md:1515`'s leak arriving through a new door while looking like normal operation. Own series `mc_media_teardown_jobs_dropped_total{key_custody}`, ERROR, `> 0` alert, zero-init. Concurrency: the worker handles jobs CONCURRENTLY (a `JoinSet`), bounded by `MAX_CONCURRENT_TEARDOWNS`, so N meetings ending against an unreachable MH cannot serialize into an unbounded backlog; the channel is sized against that bound and the worst-case per-job duration, both derived. State the numbers at the definition.
- **OPS-10 — name the worst case, not the common case.** The runbook line carries the DERIVED N (quiesce bound + the Race-B ordered wait + EndMeeting attempts against an unreachable handler; the fence deadline sits above it) and says the symptom presents as "joins to a recently-emptied meeting hang for up to N seconds", with the remedy being to look at MH reachability rather than the join path — because it will be reported as a join outage.
- **OPS-11 — accepted as a wider sweep, and it is this task's.** Six tests encoding the old behaviour is six silent dependants, not a fixture bug. A sweep is running over `crates/env-tests/src/fixtures/**` (a shared helper that drops the MC session may have more callers than the six), the other env-tests, `crates/mc-test-utils/**`, `packages/web-app/e2e/**` and any browser specs, `packages/sdk-core`/`test-utils` docs and tests, and `docs/**` for prose asserting an empty meeting persists. Fixes land in the SHARED helper wherever one is the culprit; per-call-site fixes only where there is no shared home. @test is looped in on the fixture design.

### Test (round 2 — class-(B) outbound fallout, audited)
- **(1) The four named files are unaffected, and I verified it rather than assuming.** Only FOUR MC test files ever join a participant: `media_admission_integration.rs`, `join_tests.rs` (both already in §6b), `media_client_signaling_integration.rs` and `slot_placement_integration.rs` (both audited as holding their connections, so no roster ever empties). `otel_grpc_outbound_integration.rs`, `gc_integration.rs` and `meeting_assignment_metrics_integration.rs` never create or join a meeting at all — their `notify_meeting_ended` is a bare trait stub required to implement the tonic service, and their assertions count `register_mc` and heartbeat calls only. (`mc_assignment_metrics_integration.rs` does not exist.) So there is no vacuous "0 NotifyMeetingEnded" assertion anywhere to become a silent gap.
- **(2) The positive test was missing from the plan; it is now in.** The MC integration stack (`build_test_stack`) wires no `GcClient`, so E2's notify path needs a seam to be testable at all — which is exactly why its absence would have gone unnoticed. The controller takes a `MeetingEndedSink` trait (production: the bounded channel to the `main.rs` GC task), and a new test asserts: a meeting that empties produces EXACTLY ONE notification, carrying the right meeting id; a graceful-shutdown teardown produces NONE (the cause filter); and the notification arrives AFTER the controller reaped the meeting. BOTH branches are DRIVEN on real paths — the shutdown branch through an actual graceful shutdown, never asserted by inspection — so the cause filter cannot silently invert. This is the GC-side counterpart to the mock-MH "exactly once per handler" assertion, and it makes the channel-full drop path (OPS-2) testable too.
- **§6b confirmation 1.** The three `meeting.rs` fixes assert a REAL observable — the meeting actor ended (the `MeetingEnded` signal fired / the handle is closed), not "the roster is no longer readable" — so they cannot pass for the wrong reason if `get_state`'s empty-case behaviour changes.
- **§6b confirmation 2.** Env-tests 28, 30, 32, 33 and 34 are under an in-flight full-tree sweep (also OPS-11) that covers every env-test other than 26, the `env-tests/src/fixtures/**` helpers, `mc-test-utils`, the browser E2E specs and docs prose. 34 joins through MC and is the one I expect to need checking. Any hit is fixed in the SHARED helper where one exists. Findings and fixes land in §6b before implementation starts.

### Observability (round 2 — the `action` row text, authored by @observability)
Added verbatim as supplied, using the **AMENDED** version (their second message supersedes the first): the `| action | ... |` row after `event_type` in §Shared Label Names; the "`action` vs `outcome`" prose, which grounds the row in majority EXISTING usage (`dt_client_media_mute_transitions_total{action}` is a live conforming user) rather than minting a definition for MC, and carries the no-cross-metric-aggregation contract; the paragraph naming the client/MC pair as explicitly NOT a join key despite byte-identical `{mute, unmute}` value sets, because identical values are an invitation in the autocomplete where the general contract is a rule a reader must go and find; and the value-set-drift note under §Non-canonical aliases recording `ac_rate_limit_decisions_total{action}` with its **seven** coupled artifacts and `auth-controller` as owner. MC's catalog row also names the non-join pair rather than leaving it to the general contract (@dry-reviewer's Gate-3 constraint).
- I do NOT touch AC's metric, dashboard or runbook in this task (another specialist's crate, a live dashboard query and a live runbook selector; the taxonomy's own process is a coordinated migration). @observability files that as a `docs/TODO.md` §Observability Debt entry at verdict time.
- Their ruling over G1-3's framing is adopted as written: the row states `action`'s real meaning (the verb asked for, never its result) rather than being widened to cover AC's `{allowed, rejected}`, because a definition spanning both "what was asked" and "what happened" excludes nothing and would make `action` the fallback bucket the table exists to prevent.

### Operations (round 3 — OPS-12, accepted)
- **OPS-12 accepted: emit the derived bound as a structured field on MC's existing startup line** (`main.rs:148`, `Configuration loaded successfully`), rather than letting a seconds figure be written into runbook prose. A literal in prose is a second copy of four constants living in two files: it goes wrong on the first change to any of them, silently, with nothing failing — exactly the single-source-of-truth case. The runbook then states the EXPRESSION and tells the operator to read the live value off the startup line, which cannot drift and is per-pod. Emitting it is right even though the bound is derived from consts rather than config, on the same argument §5 already makes for the ConfigMap case: the startup line is the only record of what the process actually computed. Fields: the quiesce bound and the worst-case fence hold.
- **The third site in the same block** (~6 lines above `mh-incident-response.md:784`, "MC begins calling it in story 2 task 12; before that — and permanently for a meeting whose MC dies without calling it") needs only its tense inverted, since it was written with the clean/crash split already correct. It carries the literal token, so the gate catches it.
- Their severity ruling is recorded: the backstop signal is **warning, `> 0`** rather than a page (the fence self-releases, blast radius one meeting), with the runbook saying to escalate on a repeat signature — because every firing is also a meeting whose `EndMeeting` never went out, i.e. one more permanently-held registration against the enforced `MH_MAX_REGISTERED_MEETINGS`.
- **HARD REQUIREMENT accepted: every series an alert keys on is zero-initialised at process start.** A `> 0` rule over a series that does not exist until its first firing NEVER fires — Prometheus has nothing to evaluate — so the alert would be silently inert on exactly the population of pods where nothing has gone wrong yet, which is every pod until the moment it matters. An expected-empty counter and a `> 0` alert are only a valid pair when the zero is PRESENT (ADR-0032's presence guard; the property `mh_grpc_requests_total{method="end_meeting"}` was given at task 11). This covers `mc_media_teardown_fence_backstop_total`, `mc_media_push_quiesce_total{outcome="timed_out"}` (their discriminator reads it as zero), `mc_media_end_meeting_total{outcome="mc_id_mismatch"}` and `mc_gc_meeting_ended_notifications_dropped_total`. It is also DRY's G1-1 requirement arriving from the alerting side, so the two agree.
- **The two teardown series are NOT mutually exclusive, and their runbook says so.** A wedged teardown can record `timed_out` at quiesce and `backstop_fired` later. `backstop_fired` WITHOUT a matching `timed_out` is a different fault — the worker hung somewhere other than quiesce — from the two together. A responder reading them as alternatives would stop at the first and wrongly conclude the worker finished.
- I owe them pinned metric names at landing time: if the quiesce counter's shape changes (e.g. folded as an EndMeeting pre-outcome), their scenario's selector changes with it, so I confirm the final names before they land the text.

### Observability (round 5 — final; O-20 settled, O-22, and the metric RULE)
- **O-20 settled on the split.** `mc_media_teardown_fence_backstop_total{key_custody}`: cardinality 1, expected-empty, zero-init, `> 0` warning. It carries `key_custody` because it belongs to the media-routing control plane, beside `mc_media_push_quiesce_total` and `mc_media_end_meeting_total`. It is not part of the generic outbound path, which deliberately omits the label. Its row says the fence was lifted by its deadline rather than by the worker's report, meaning the worker hung or never reported.
- **O-21 settled on the startup field** (OPS-12). MC's `Configuration loaded successfully` line (`main.rs:124-149`) already carries fourteen effective-value fields. Its own comment cites the mh-service precedent for logging a DERIVED value in the same line, so the fence-hold bound is one more field on an existing event, not a new mechanism.
- **O-22: `docs/TODO.md:82` is corrected in place, not deleted.** Wording is @observability's; they co-own the field-name convention on that entry. The entry claims effective-config startup logging "exists in MH only" and that MC and GC "never" log resolved configuration. That is false:
  - MC logs fourteen fields at `main.rs:124-149`.
  - GC logs five at `main.rs:115-122`.
  - Only AC's line (`main.rs:88-91`, `jwt_clock_skew_seconds` + `is_default`) is still near-empty.
  The entry narrows to AC, drops MC and GC from its owner list, and keeps its real insight: a ConfigMap cannot be its own evidence. MC's comment at `main.rs:135-141` now states that insight better than the TODO did. The stale entry nearly had a cost in this very loop: had it been believed, OPS-12 would have been rejected for depending on a line that does not exist, and a drifting prose constant would have shipped instead.
- **Startup-field convention, first written form (O-21/O-22 addendum).** OPS-12 adds TWO fields to MC's existing `Configuration loaded successfully` event, not one, because they answer different questions:
  - `push_quiesce_bound_seconds` — the named constant, i.e. the input an operator checks when a drain times out.
  - `teardown_fence_hold_max_seconds` — the derived worst case, i.e. how long a rejoin can be held.
  With only the total logged, an operator cannot tell which term is large; with only the constant, the arithmetic goes back into the runbook.
  The four rules they follow are written into the corrected `docs/TODO.md:82` entry as the convention's first written form, pending ratification (owner: @observability):
  1. snake_case on the ONE startup event, never a second line;
  2. an explicit unit suffix on every quantity;
  3. name the concept the key controls, not the Rust identifier or env-var spelling;
  4. name a derived value for what it BOUNDS, not its formula.
  **Scope, stated because this grows:** two fields plus the four rules in the TODO entry. AC's and GC's startup lines are not touched, and no existing MC field is renamed (the existing fourteen already satisfy the rules; any rename is a log-consumer change that belongs with the ratification).
- **The metric obligations are a RULE OVER THE DIFF, not a list.** This supersedes every count earlier in this document. The set has grown five times during planning, so a fixed enumeration is itself the partial-invariant trap. The rule: *every counter this diff adds is zero-initialised in `zero_initialize_counters()` over its FULL label set (every value of every label, the cross product where there are two — never only the cells an alert names, because a discriminator that reads a denominator such as `released` or `quiesced` staying flat is meaningless if that cell is absent until its first event; unreachable cells are named in the catalog), has a catalog row, has a dashboard panel, has a `MetricAssertion` test, and carries `key_custody` if it is a media-routing-control-plane metric.* The guards (`dt-guard application-metrics`, `dt-guard metric-coverage`) enumerate it. A metric deliberately outside any clause says so AT ITS ROW, with the reason, as the outbound-drop row already does. Example: `mc_gc_notify_meeting_ended_total` and its channel-drop sibling are GC-coordination metrics, not media, so they carry no `key_custody`.

### DRY (round 3 — G1-8: the normative source itself goes stale; a SECOND GSA row)
- `proto/dark_tower/internal/v1/internal.proto:801-805` is the ONE home every other site defers to. `session/mod.rs:908-910` calls it "the source of truth — not restated here", and `docs/TODO.md:1513` calls itself "the copy with a trigger". It states that a deadline-expired attempt "may still be applied by MH afterwards, re-creating the meeting". The B1 guard closes exactly that, so after this diff the normative statement overstates the residual.
- This is the APEX of the class, not one more site. Good pointing discipline makes a wrong source worse, not better: every deferring site reads as correct while inheriting the stale claim, and there is no second copy to disagree and surface the drift.
- **The edit narrows the residual; it does not delete it.** The paragraph currently collapses three fates into one:
  - (A) a deadline-expired attempt whose apply is queued behind the release: refused at apply time by MH's registration check. CLOSED — but the refusal is evidenced by an actor WARN ONLY, not by a `rejected_stale` count (the carrying RPC was already counted `apply_failed` when it timed out; see §8). The proto text must not claim a count.
  - (B) an upsert that lands after the release: passed by the guard by construction, and ORDERED (not closed) MC-side by the wait on the quiesce-timeout path — the ordering depends on tonic cancelling the server handler when the client drops, a foreign-library property that fails open, so the proto text must not claim closure.
  - (C) story 4's periodic re-assert: still OPEN and still needing the fence, because a fresh re-assert re-registers the meeting.
  - "The fence that closes both" must therefore change: after this diff, one of the two is closed by a different mechanism. The text must agree with `:1513`'s verbatim fence requirement. The pointer names the TODO entry by TITLE rather than by section, following @paired-media-handler's rule.
- **Routing.** `proto/**` is an ADR-0024 §6.4 Guarded Shared Area, and "comment-only, no wire change" does NOT exempt it. Mechanical is disallowed inside a GSA, and this story has already done this exact shape twice with owner involvement (`e6c0a248`, `9c664bb4`). The row is Domain-judgment, owner `protocol` (the manifest value, per the Lead's classification-guard normalisation); `security` stays on the panel per the §6.4 intersection rule but is not named in the Owner cell. **Correction to my own earlier statement:** I told @protocol "no other proto/** edits planned". That is now false and I have told them so. @protocol IS already on the panel (the Lead added them for F-A), so both GSA rows are with the same owner.

### Lead ruling on the GC blocker — OPTION 1, in-loop
- **The defect this diff ARMS (it does not introduce it).** After `NotifyMeetingEnded`, GC can never re-assign that meeting id: the assignment row is soft-deleted (`ended_at = NOW()`), nothing ever clears it, the key is the non-partial `PRIMARY KEY (meeting_id, region)`, and `atomic_assign`'s `ON CONFLICT DO UPDATE` fires only for an UNHEALTHY MC. So every later join 503s for `GC_RETENTION_DAYS` (7). It is unreachable today only because nothing calls `NotifyMeetingEnded`; E2 makes the most ordinary sequence in a conferencing product reach it. Dropping the notify does not help: the live row then sends GC down its reuse branch, which never re-sends `assign_meeting`, onto an MC that no longer holds the meeting.
- **The fix has TWO halves, and both are required** (@operations): the predicate gains `OR meeting_assignments.ended_at IS NOT NULL`, AND the SET list gains `ended_at = NULL`. A predicate-only fix leaves a second, quieter copy of the bug on the ordinary MC-failover path: the predicate already fires there (the old MC is unhealthy), but the row would be re-pointed at the new MC while STILL carrying `ended_at`, and `get_healthy_assignment` would keep returning `None`.
- **Why this also preserves R-6.** An ended row is not "healthy", so a rejoin takes the NEW-assignment branch, which re-selects handlers, revives the row and DOES call `assign_meeting` — so MC re-creates the meeting properly. R-6's stickiness invariant survives intact as ONE `assign_meeting` per meeting INCARNATION.
- **Tests (the second one is the one that catches half a fix):** `notify_meeting_ended` followed by a re-assign (no such test exists today in either direction); and the discriminating FAILOVER case — an existing row that is BOTH ended AND pointing at an unhealthy MC, asserting `ended_at IS NULL` afterwards.
- **OPS-14: a query change, never a migration.** Replacing the key with a partial unique index `WHERE ended_at IS NULL` looks tidier, but a schema change to a live table cannot be reversed by rolling the image back and lands in `migrations/**`, a Guarded Shared Area. If @database concludes the schema must change, that is their legitimate call — but it must be deliberate, not aesthetic drift.
- **OPS-13: the rollout is now ordered across THREE services.** GC and MH go first (independent of each other), then MC. Rollback reverses: MC first, then GC/MH. MC is last forward and first back in both pairs. Recorded in BOTH `mc-deployment.md` §Coordination and `gc-deployment.md` §Coordination, since the GC operator must know their deploy is a PREREQUISITE, and a one-sided note is how the pair gets separated later.
- **Residual, recorded not fixed:** if `NotifyMeetingEnded` FAILS, the row stays live, GC reuses it, and joins to that id get `MeetingNotFound` until the row ends or MC restarts. Detector: `MCNotifyMeetingEndedFailing` (OPS-2).
- **GC round 2 (@paired-global-controller) — two more in-file defects the revive makes reachable, fixed in-loop (fix-don't-defer; < 10 LoC each, same file).** Both batch jobs select by `meeting_id` alone and do not re-check their predicate in the outer statement:
  - `cleanup_old_assignments`: `DELETE ... WHERE meeting_id IN (SELECT meeting_id ... WHERE ended_at < retention)`. Under READ COMMITTED, a row revived between the subquery snapshot and the row lock still matches the outer `meeting_id IN (...)`, so a LIVE revived row is deleted; it also deletes that meeting id in EVERY region. Fix: key the outer statement on `(meeting_id, region)` and re-assert `ended_at < retention` on it.
  - `end_stale_assignments`: same shape — the outer UPDATE matches `meeting_id` across regions with no `ended_at IS NULL` and no unhealthy-MC recheck, so it can end a healthy row in another region, or a row revived onto a healthy MC mid-statement. Fix: key on `(meeting_id, region)`, add `ma.ended_at IS NULL`, and re-assert the unhealthy-MC `EXISTS` against `ma`.
  - Tests: one meeting id with an ended+old row in region A and a live row in B — cleanup leaves B; a stale-unhealthy row in A and a healthy live row in B — B keeps `ended_at IS NULL`.
  - Also from GC: qualify `meeting_assignments.ended_at` in the `DO UPDATE WHERE` (a bare `ended_at` is ambiguous with `EXCLUDED`); no-steal-after-revive test guarding the `OR`'s parenthesisation; the service-level test (end → re-assign is a NEW selection with `call_count == 2`, a third call reuses); "once-per-meeting `assign_meeting`" wording in `mc_assignment.rs` and `meeting_tests.rs` becomes "once per meeting INCARNATION"; one sentence in `docs/observability/metrics/gc-service.md`'s stickiness note that a rejoin after meeting end counts as a new assignment. No `.sqlx` regen: gc-service uses only runtime `sqlx::query`/`query_as` (verified: no `query!` macros), and `/.sqlx/sqlx-data.json` is an empty stub.
  - **I draft; @paired-global-controller reviews** (their choice, to avoid parallel edits).
- **The notify send policy (GC concern B) — AT-MOST-ONCE for anything GC may have processed, and sent AFTER teardown completes.** The revive makes a new race reachable: `NotifyMeetingEndedRequest` carries only `(meeting_id, region)` and ends whatever row is ACTIVE, so a retried or late notify for incarnation N that lands after N+1 was assigned ends N+1's LIVE row — and the next join may pick a different MC while participants are still on N+1's, splitting the meeting.
  - **Retry ONLY where the request provably was not processed**: a connect failure before the request is sent, or a definitive error status from GC's handler (it returned an error, so it did not commit). **Never retry** a deadline-exceeded or a mid-flight reset, which are ambiguous. That makes duplicate delivery of a processed notify impossible, so no retry can end a later incarnation's row. This supersedes OPS-2's "bounded retry" wording, narrowing it rather than dropping it.
  - **Sent after teardown COMPLETES, not at reap.** Row N+1 can only be created once N's row has ended, i.e. once our notify has landed, and after teardown there is no MH state left for N+1 to race. It also keeps `mc_media_end_meeting_total{outcome="mc_id_mismatch"}` a genuine MC-defect signal: if GC could re-assign to a DIFFERENT MC while our `EndMeeting` was still in flight, a legitimate takeover would read as a mismatch and fire an alert that means "bug".
  - **Consequence for OPS-10, stated honestly:** a rejoin DURING teardown finds GC's row still live, takes the reuse branch, and gets `MeetingNotFound` from MC (not a held join) until teardown completes and the notify lands. Normally milliseconds; worst case the derived teardown bound. The runbook's join-outage symptom reads "`meeting_not_found` for up to N seconds" rather than "hangs"; the MC fence still queues any create that arrives by another route (defence in depth).
  - **Same-MC re-assign (GC concern A):** MC's `AssignMeetingWithMh` for a meeting id it FULLY tore down is an ordinary create — the controller entry is removed at reap and the fence has lifted by the time GC can re-assign (post-teardown notify), so there is no stale "already exists" and no leftover state.
  - **Residuals, recorded, bounded honestly:** (1) an AMBIGUOUS notify failure that GC did not actually commit leaves the row live, and joins to that id get `MeetingNotFound` until MC restarts or is marked unhealthy — **no bound short of that**, detected by `MCNotifyMeetingEndedFailing`; (2) GC failover (`atomic_assign`'s unhealthy/stale-MC arm re-pointing the assignment to another MC on a join) racing the old MC's in-flight notify could end the new MC's live row. Both are closed by the same follow-up, which is genuinely task-sized and so filed rather than smuggled in: an incarnation-safe notify — `controller_id` (and ideally an assignment incarnation token) on `NotifyMeetingEndedRequest`, GC ending only a matching row, which in turn makes it safe for MC to re-send the notify on a join for a meeting it does not hold (rate-bounded), bounding residual (1) to "the next join attempt". It is a wire change (GSA, @protocol + @security), a GC handler change and an MC join-path change, so it is NOT a one-line fix, and it is new vocabulary on the MC↔GC contract that deserves its own review.
- **GC and DB confirmations (both received; the Lead's gate for wiring the end-on-empty trigger is met).** Conditions carried:
  - @paired-global-controller: the incarnation-safe-notify follow-up gets a `docs/TODO.md` entry (owners protocol + global-controller + meeting-controller), NOT only this record. This is the one new TODO entry this devloop files, and it is genuinely task-sized: a GSA wire change plus GC handler and MC join-path changes. Residual (2), join-time failover (`atomic_assign`'s unhealthy/stale-MC arm) racing the old MC's in-flight notify, is also noted in `docs/runbooks/gc-incident-response.md` beside the `MCNotifyMeetingEndedFailing` triage, so an operator seeing a SPLIT meeting has somewhere to look.
  - @database, (a): concurrent revive serialises on the PK row lock. The winner revives and commits; the loser re-reads a live, healthy row, updates nothing, and falls back to the winner. The caveat goes in a code comment: this holds only when the winner assigned a HEALTHY MC. If the winner's MC is unhealthy, the loser's `EXISTS(unhealthy)` branch legitimately fails it over, as it does today.
  - @database, (b)/(c): revive-in-place keeps the PK, so it is rollback-clean. A partial unique index would force dropping the PK that `ON CONFLICT (meeting_id, region)` needs, breaking an image rollback. No new index: the predicate is evaluated on the already-locked PK row.
  - @database, #2: a code comment at `atomic_assign` records the reverse teardown race the revive makes reachable. `end_assignment` has no mc_id / `assigned_at` / generation fence, so a delayed notify from the PREVIOUS incarnation ends a freshly revived row. The comment ties this to the story-4 fence and to the incarnation-safe-notify follow-up.
  - @database, #3: `.sqlx` is a non-issue, confirmed.
  - @database's cross-region question: ALREADY a real finding, not latent. @paired-global-controller raised the same `meeting_id IN (...)` shape for both batch jobs, and it is fixed in-loop above by keying on `(meeting_id, region)`.
  - Test emphasis from @database: runtime `sqlx::query_as` gets ZERO compile-time checking of the `ended_at` semantics, so the revive test is the only guard. It asserts both the returned MC AND `SELECT ended_at ... IS NULL`. The concurrent test is extended to start from an ended row.
- **Wiring order (Lead):**- **Wiring order (Lead):** the end-on-empty trigger and the GC notify are NOT wired until @paired-global-controller and @database confirm the GC section. Everything else proceeds.
- **Also ruled in:** both destructive operator remedies (`alerts.md:657`, `mc-incident-response.md` Scenario 15) take @operations' Variant A — demote leave-and-rejoin, promote the capability re-declaration and mute toggle that were always offered in the same remedy, because an annotated footgun mid-incident still gets fired. `alerts.md:657` item 1 ("fires on every join, leave, capability declaration and mute") stays: it is what makes the mute-toggle alternative credible. And the five "no production code ever marks a meeting ended" sites are narrowed with their owners — shape 3, not an inversion: GC's `notify_meeting_ended` ends the ASSIGNMENT row, not `meetings.status`, so the org-cap arithmetic they justify may still hold; only the blanket claim becomes false.

### Code quality
- **Q1 confirmed.** Order is: pushers MOVED OUT of the map (so `Drop::abort` is unreachable for an in-flight attempt) → quiesce (await the handles) → EndMeeting → generation eviction; `cancel_token.cancel()` happens only after the move and can no longer reach a pusher, since pushers now hang off `MeetingMedia`'s own token rather than a child of the actor cancel. `Drop::abort` remains reachable only on unclean paths.
- **Q2** — withdrawn in favour of the recompute shape; adopted.
- **Q3 confirmed.** F-A lands only under the Lead's ruling (given) with @protocol and @security in planning and review; the Owner cell now names both.

### DRY
- **G1-6 accepted — sealing the door.** `MeetingMedia::reconcile(&mut self, affected, server_muted: &BTreeSet<SenderId>)` takes the set as a REQUIRED parameter, so the compiler enumerates the eight existing call sites and a ninth cannot compile without supplying it. `reconcile_media` on the actor is the only thing that computes the set. Without this, a missed site would push an EMPTY set, which MH reads as "nobody is muted" — the strictly worse direction.
- **G1-3 accepted.** The `action` row is added in the `outcome` row's form — "closed enum per metric, defined in that metric's catalog entry, deliberately not enumerated here", with the no-cross-metric-aggregation contract — NOT the enumerated `region`/`status` form, because `ac_rate_limit_decisions_total{action}` already emits `{allowed, rejected}` and an enumerated row would assert a meaningless union. No `ACTION_LABEL` const in `common/observability/labels.rs`, whose doc promises a single permitted-value set that `action` cannot honour. The row's text must be true of AC's pre-existing usage too; flagged to @observability.
- **G1-1 accepted.** All five new metrics carry `key_custody` and are present at zero: the two the plan had missed are `mc_media_push_quiesce_total` (label-less `key_custody`-only block, precedent at `metrics.rs:1801`) and the gauge (boot `set(0.0)`, per O-13).
- **G1-2 accepted** — the count is five after S-12, and the doc states the property rather than enumerating a pair.
- **G1-4 accepted** — required trait method, four implementors named (§6).
- **G1-5 accepted.** The probe helper hoist is named now, not conditional: test 29's `register(meeting_id, mc_id, generation)` (`29_mh_meeting_teardown.rs:508`) moves to `fixtures/mh_grpc.rs` beside `connect`/`authed` at its second consumer, following task 11's `configmap_key` precedent. The existing `ReleaseOnDrop` (`fixtures/mh_grpc.rs:149`) is reused for probe cleanup rather than a new guard.
- Rename blast radius confirmed and folded into §6, including the "do not sweep the MH RPC spellings" note for the commit message.

### DRY (round 2 — G1-7, the "task 12 has not landed" premise class; ALL tiers taken in-loop)
The rule this diff establishes — *a premise asserting "task 12 has not landed" is now false* — has ~14 instances, not the 4 the plan covered, and task 11 deliberately pointed several of them at this task by name (`docs/devloop-outputs/2026-09-25-mh-end-meeting-teardown/main.md:211,274`). Declining them would be declining an addressed handoff. Every site gets its REASON replaced, not its mention deleted; a site that is genuinely unaffected says so at the site.
- **Tier A (test premises — re-verified, not reworded; @test reviews):** `26_mh_quic.rs:49,1438,1446` (the directly-injected mute at a high generation now races REAL MC mute pushes: re-check that the stale-generation refusal still isolates it now that MC's set is non-empty); `28_mh_egress_admission.rs:74,77` (its cleanup strategy assumes nothing reclaims its streams — under E2 MC's `EndMeeting` does, so the cleanup premise is re-derived); `29_mh_meeting_teardown.rs:80` (a mismatched `mc_id` becomes reachable in principle once a real MC calls `EndMeeting`; its containment argument is re-checked — I'm in the file for the G1-5 hoist anyway).
- **Tier B (production doc-comments):** MC — `connection.rs:836` and `:1904` (both replaced by the new dispatch arm anyway). MH — `config.rs:167`, `observability/metrics.rs:1307`, `session/mod.rs:601`, `grpc/gc_client.rs:185-186`: comment-only, wording by @paired-media-handler, who is already editing `session/mod.rs`.
- **Tier C (docs):** `docs/observability/dashboard-conventions.md:252` (wording @observability, who already owns the `mh-service.md` §`server_muted` flip).
- Already counted elsewhere in the plan (not double-counted): `mh_client.rs:285`, `meeting_media.rs:525-537`, `meeting.rs:2031`, and the O-14 trio.
- **Round 3 — @paired-media-handler enumerated the class at 13 sites and claimed the MH env-tests. Accepted; the ownership split is theirs, not mine.** I had put 26/28/29 under my own name; they wrote those suites and their soundness arguments, so items 5-9 move to them (rows updated). ONE shared replacement residual is reused at every site so it cannot drift — *a meeting whose MC never COMPLETES `EndMeeting`*, covering (i) crash or kill before/during teardown, (ii) retries exhausted (`unavailable_exhausted`) or an MH predating the RPC (`unimplemented`), and (iii) a rollback to a build without teardown, which my own `mc-deployment.md` note makes real rather than hypothetical. Every site cites the TODO entry by TITLE, never by line number.
- **The load-bearing one is `26_mh_quic.rs:1437-1439`**, and it is a correctness matter rather than wording: S4 claims its `server_muted` counter rise is "attributable to this test" because no other producer exists before task 12. My env-test 35 S3 becomes a second producer on the same shared pods, so the attribution claim goes FALSE, which is exactly the "counter rise as proof" the env-tests README forbids. S4 stays sound (its real proof is per-receiver), so the fix is to demote the counter to SECONDARY — the same shape as m1 on test 35, so both suites say the same thing about the same series.
- **`26_mh_quic.rs:47-49` / `:1446-1448`** likewise lose their REASON, not their validity: "MC sends no server mute" stops being why S4 injects directly. The true remaining reason is that S4 proves survival across a SAME-GENERATION re-assert, and MC sends no RPC for an unchanged snapshot until story 4's cadence — so that case has no public path, which is what the README requires for an internal-API call. It does not duplicate test 35, which does not test same-generation survival.
- **`docs/TODO.md:1513` FINAL FORM: the copy POINTS, it does not restate** (@dry-reviewer's shape, adopted by @protocol as contract owner; this supersedes the three-fates restatement I had proposed). Root cause: the entry declares itself "the copy with a trigger", and a copy that RESTATES a mechanism goes stale every time the mechanism changes — which is how we got here twice in one story. Restating the three fates would fix today's staleness and guarantee the same hand-sync at story 4's fence. In-tree pattern: `label-taxonomy.md:86`'s `reason` row — "Points, never restates — restating either set here would give each token two homes and they would drift."
  - The copy POINTS at `EndMeetingRequest` in `internal.proto` for the mechanism and its current fates, by entry title, reproducing none of them.
  - VERBATIM: the trigger, the owner, the detection signal, and the story-4 fence-required sentence with its resurrection rationale (that is the entry's legitimate LOCAL reason-for-trigger, not a duplicated mechanism).
  - INVERTS: the enforcement-owner clause — the MC-side quiesce MUST is implemented as of this task.
  - The proto keeps the full three fates as the one home, so story 4's fence change touches ONE artifact.
- **REJECTED: "the guard is a partial realization of the teardown tombstone."** @protocol characterised it that way and @dry-reviewer refuted it; the refutation is right and the claim must not land. A tombstone refuses REGISTRATIONS for a RELEASED meeting within a BOUNDED WINDOW; the guard refuses APPLIES for an UNREGISTERED meeting with no window. The proof is the limitation already established: the guard PASSES a periodic re-assert, because a re-assert re-registers the meeting, so it is registered by the time its apply runs — whereas a partial tombstone would by definition catch the re-assert, since refusing the registration IS the mechanism. Why it matters beyond taxonomy: recorded in the one artifact whose job is to say what remains, "the tombstone is partly built" gives story 4's implementer a FALSE HEAD START on an option that is entirely unbuilt and that this control structurally cannot grow into — an error pointing toward UNDER-building the fence, which is the same failure direction as flipping the requirement, arriving as a helpful-looking progress note. The seam is stated instead as: the apply-time check closes fate A and is ORTHOGONAL to both fence options.
- **`docs/TODO.md:1513` — its RESIDUAL SENTENCE must narrow too; "verbatim" was only ever right about the FENCE half** (@protocol's Gate-1 finding, reconciling with @dry-reviewer's earlier "keep it verbatim", which was correct while no guard existed). The entry declares itself "the copy with a trigger" of the proto's normative statement, so leaving its copy asserting the old claim while narrowing the source is the rule-applied-everywhere-except-X shape. Concretely, "The window is bounded by the RPC deadline, is accepted in story 2, and **is closed only by the fence below**" becomes false: fate (A) is closed by the not-registered-at-apply check landing in THIS task — which is itself a partial realization of the "teardown tombstone on MH" the entry lists as a story-4 fence OPTION. The sentence narrows to the three fates; the re-assert paragraph, the "fence is required, not optional" sentence and the entry's trigger/greppability all stay verbatim.
- **`docs/TODO.md:1513` must not read as though the MH guard covers story 4.** Story 2's residual IS closed for Race A (a queued apply is refused MH-side, evidenced by an actor WARN — NOT by a `rejected_stale` count) and ORDERED for Race B (the timeout-path wait), but the story-4 fence stays REQUIRED, because a periodic re-assert is a fresh `RegisterMeeting` whose upsert re-registers the meeting and neither mechanism stops that.
- Deliberately untouched, verified by @paired-media-handler as a different story's task 12: `crates/mh-service/src/transport/mod.rs:249` and `mh-media.json:746`. `mh-overview.json:1503` is the same case but GETS AN EDIT anyway — a story qualifier — so the next sweep of this class cannot mistake it for an in-class site. That is the cheapest available fix for shape 3 ("absence still true, on narrower grounds"): make the out-of-class site say so itself instead of relying on the next reader re-deriving it.
- **Tier D (round 2 — operator-facing, and invisible to a task-number grep):** two runbook paragraphs state the absence in prose without ever naming task 12, and both are the highest-stakes sites in the class because an operator reads them mid-incident. Wording is @operations', who has them:
  - `docs/runbooks/mh-incident-response.md:784` — ends "The durable fix is teardown actually arriving (MC calling `EndMeeting`)". The edit is NOT "the fix has arrived": it SPLITS the residual, because the edge-budget-ratchet advice stays valid for the crash / killed-mid-teardown path and stops being valid for the clean-end path. Left as is, an operator over-estimates the ratchet's reach and may restart a pod they did not need to.
  - `docs/runbooks/mc-incident-response.md:2858` — "and `EndMeeting` is not yet implemented" (a file already in the table for the two new Scenarios).
- **The pre-validation gate is KEYED TO THE CONCEPT, NOT THE TOKEN** (the same forcing-function point as G1-4 and G1-6: a `task 12` grep passes silently on exactly the sites nobody thought to tag). Run before "Ready for validation" over `crates/ docs/ infra/ scripts/ proto/ packages/`, with every surviving hit either inverted or annotated as still-true:
  ```
  grep -rnE "task[ -]12|no production producer|does not yet (program|send|call)|MC sends no|not yet implemented|until MC (calls|begins)|before MC (calls|begins)|zero by construction" crates/ docs/ infra/ scripts/ proto/ packages/
  ```
  The pattern is recorded HERE and in the commit message so the next task in this class inherits a working sweep instead of re-deriving one.
- **THREE SHAPES, not two, and only the first is a tense inversion** (@dry-reviewer + @operations). The sweep's notes carry all three, because two of them are edits that would make the tree WRONG if treated mechanically:
  1. *Absence now false* → invert the tense (most of Tiers A-D).
  2. *Absence asserted without the token* → invisible to a token grep; this is why the gate is concept-keyed (the two runbook paragraphs).
  3. *Absence still true, on NARROWER grounds* → **do not invert.** `docs/runbooks/mc-incident-response.md:2858`'s Scenario (e) is ABOUT the MC-restart path this task deliberately leaves untouched, so a mechanical "EndMeeting has landed" edit makes it wrong in the OPPOSITE direction from the `:784` site. Likewise `docs/TODO.md:1513`: its residual paragraph and its story-4 fence requirement stay VERBATIM and still required — only its enforcement-owner clause (which names "the MC teardown task, story-2 task 12" as owner of the MC-side quiesce MUST) inverts, because I am implementing exactly that. A tracking artifact that read as "the story-4 fence no longer needs building" is the one direction nobody comes back to check.
- **One more site this devloop CREATES, routed to @paired-media-handler:** `crates/mh-service/src/session/mod.rs:911-913` currently says "MH does not enforce it. The accepted residual: an attempt whose client-side deadline expired can still be applied here AFTER this release and re-create the meeting." The B1 guard in the same file makes the first sentence false for Race A. Its STORY-4 NOTE (lines 915-920) is shape 3 and stays verbatim.

### Test
- **(i) confirmed, with the contradiction recorded.** The module doc states plainly: the task text asked for MH's registered-meetings gauge, the README evidence rule forbids asserting a shared pod-level gauge's value, the rule wins, and the gen-1 probe is the per-entity substitute. Counters stay secondary behind the probe.
- **(ii) accepted — the pod-counter positive control is withdrawn.** Generation ≥ 2 is established BY CONSTRUCTION: drive ≥ 2 distinct structural changes through public APIs and confirm EACH applied through the test's own client's `StreamAssignments` (the joiner appearing in a slot, then the mute state). MH's generation monotonicity then guarantees the held generation ≥ the number of confirmed structural changes, with no shared read anywhere and no generation number read at all. The count of confirmed changes IS the positive control. Any confirmatory read, if added, must be a per-entity MC log line carrying this meeting id.
- **(iii) accepted.** The unreachable cell is asserted PRESENT AND STILL ZERO after driving the non-host path (never absent, which would be green on a process that never initialised the recorder), with a comment saying it tests the authority-BEFORE-target ORDERING invariant of R-8 — so it goes red if someone reorders and reintroduces the existence oracle. Driven cells: `applied`, `unchanged`, `not_permitted`, `unknown_target` (host requester only), `rate_limited`. `actor_unavailable` is NOT test-reachable through the public path and is a stays-at-zero cell, named as such.
- **(iv) resolved — already covered, so nothing is added for it.** @paired-media-handler supplied the line refs: `mh_service.rs:635-638` already asserts `total_edges() == 0`, `routes_for(meeting).is_none()`, `generation_for(meeting) == 0` and `!is_meeting_registered(meeting)` after the release, so the leak-the-registration case fails today. @test dropped it as a blocker. The draft keeps all four and ADDS only the connection-closure and mute assertions, which are the genuinely missing half.
- **(v) accepted** — a positive control that the straggler's MH connection was ALIVE immediately before teardown (keep-alives succeeding / a frame exchanged), so a connection that died for an unrelated reason cannot false-pass as teardown evidence. With keep-alive on, an idle close cannot fire inside the bound, so a close within it IS the teardown; that reasoning is stated in the test.
- **(vi) affirmed** — A's own-egress-non-zero assertion sits adjacent to the every-other-receiver-flat assertion, since without it "flat" is ambiguous.

### Operations + Observability (post-implementation — OPS-15/O-24, O-23, OPS-16)
- **OPS-15 / O-24, taken.** Graceful shutdown runs teardown, so `FAILED_PRECONDITION` is reached routinely when a successor MC already re-registered a meeting during a rollout. That population is now its own outcome value, `superseded_by_successor`, decided at the recording site from MC's own `TeardownReason` (never inferred from MH's reply). `rejected_ownership` is the defect again and `MCEndMeetingOwnershipRejected` needs no exclusion; `MCEndMeetingFailureRate` excludes the new value. No alert on it, zero-initialised, row does not overclaim. Integration test drives the shutdown path against a `FAILED_PRECONDITION` stub.
- **O-23.** Both catalog rows name the counterpart. The correspondence is asymmetric (MH's `rejected_ownership` = MC's `rejected_ownership` + `superseded_by_successor`) and for comparison only; different denominators, never summed. Operations' v5 rows and union note landed in `mh-incident-response.md`; Scenario 20 matches.
- **OPS-16, answered with numbers at `hand_off_teardown`.** One teardown holds ≤ 1 MH connection per handler (≤ 2), opened after its push workers drain (or overlapping one still-running worker on the timeout path). The peak is the steady-state push-worker bound, `MC_MAX_MEETINGS` × 2 = 2,000 at the default, and at most twice that on the timeout path. No cap: it would lengthen the fence and rejoin hold without lowering the push-side peak. Detached tasks are DROPPED at process exit, deliberately, so the shutdown sequence cannot hold the pod into SIGKILL; a cut-off teardown collapses into the crash residual.
- **Staleness sub-kind (5)** appended to the existing runbook-staleness entry in `docs/TODO.md` (single writer, operations' text; the one line-number reference was replaced with "the paragraph above").

### Operations + Observability (round 2 — O-25, OPS-17)
- **O-25, taken; the finding was correct.** `MCEndMeetingFailureRate` had a NEGATED selector. `mc-service.md`'s standing rule (on `mc_media_mute_requests_total`) is that a negated predicate is correct only where the SUCCESS set is closed and small, and that a harmonisation pass moves predicates TO positive form. This metric's success set grew 1 → 2 inside this devloop, which is the empirical proof it is not closed — and under the negated form `superseded_by_successor` would have joined the failure set silently and paged on every deploy, i.e. O-24's defect re-entering by the back door. Both selectors are now positive.
- **The fail-quiet risk the positive form creates is answered where a variant is ADDED, not left implicit.** A comment on `EndMeetingOutcome::ALL` states that adding a variant is also an alert edit, that the default is not-a-failure, and which two selectors plus which catalog row to edit — because nothing in Rust connects an enum to a PromQL rule. This is the same "put the obligation at the edit site" move as G1-6's required `server_muted` parameter.
- **Denominator, answered explicitly**: `superseded_by_successor` is in NEITHER numerator nor denominator. It arrives in bursts on every rolling deploy, so in the denominator it would dilute the ratio exactly while deploys happen — fail-quiet, masking a real failure during a rollout. The rule now reads "of the releases that were ours to make, what fraction failed". `released` stays in, per the non-zero-denominator guard.
- **OPS-17 — the two facts, measured rather than assumed, and it closes as an accepted spin-out.** `nofile` on the deployed MC pod is **1,048,576** soft and hard (containerd default; no limit set anywhere in `infra/services/mc-service/**`), measured with `kubectl exec` on the live `mc-0`. MH's gRPC server sets **no** inbound connection bound — `config.max_connections` is passed to the WebTransport server, not to `Server::builder()`. So ~4,000 concurrent connects sit four orders of magnitude inside the ceiling: uncomfortable, not reachable, no process-global collateral.
- **Their correlation correction is right and is recorded as such.** My "this peak exists whether or not a meeting is ending" holds only statistically: pushes follow independent roster events, whereas teardown concurrency during a mass-end is correlated by construction, since one unreachable handler is what ends many meetings into the same window. Same arithmetic peak, far likelier to be approached.
- **Remedy named as channel REUSE, not a cap** (lowers both peaks, lengthens nothing; a semaphore lowers one and lengthens the fence). Recorded at the module doc that makes the per-call claim, a runbook paragraph in Scenario 20 with the `ulimit -n` check and the low-`nofile` collateral case, and a `docs/TODO.md` spin-out whose trigger includes *any* `nofile` limit being set — because the arming change would come from another specialist's hardening work.

### Gate 2, Layer 7 attempt 1 — ConfigMap annotation cap (diff-caused), and O-26/O-27
- **Failure.** Cluster setup failed twice at the observability apply: `The ConfigMap "grafana-dashboards-mc" is invalid: metadata.annotations: Too long: must have at most 262144 bytes`. Client-side apply stores the whole ConfigMap, JSON-escaped, in `last-applied-configuration`. This diff's panels grew `mc-overview.json` from 209,664 to 236,747 bytes, putting the MC ConfigMap at 279,161 bytes. Env-tests never ran.
- **Why "give mc-overview its own ConfigMap" was not the fix.** `mc-overview.json` ALONE escaped to 261,459 bytes, 99.7% of the cap. It would have passed by 685 bytes and failed on the next panel.
- **Fix (structural).** MC's four ADR-0036 media-path rows (§8 Media Routing, §5/§6 Client Media Signalling, §9 Media Connectivity, §4 KEK Lifecycle) moved to a new `mc-media.json`, in its own ConfigMap `grafana-dashboards-mc-media`. Boundary endorsed by @observability, with one correction to the reasoning: KEK is there because §4 key custody is the key half of the media path, NOT as a mirror of `mh-media` (MH holds no keys, so it has no KEK row). MH Coordination, including the teardown panels, stays on MC Overview beside RegisterMeeting. The two dashboards link to each other, with time range and variables kept (@observability overruled the no-links MH precedent as an omission, not a decision).
- **Post-split, as kubectl measures it:** `grafana-dashboards-mc` 160,865 bytes (61.4%); `grafana-dashboards-mc-media` 120,959 (46.1%). Both are well under the 90% threshold.
- **Two defects the split surfaced, both mine, both fixed.**
  - My nine new panels had been APPENDED to the panels array while positioned inside their rows. A row split by array order would have misfiled them, and my first split script did exactly that; its contiguity assertion stopped it before any write. The split groups by position.
  - Six of those panels OVERLAPPED existing ones. HEAD's layout had zero overlaps, and I introduced the overlaps when placing them earlier in this task. Grafana silently re-flows overlaps at render, and no guard checks geometry, so it was invisible. Re-laid out: every pre-existing panel keeps HEAD's geometry relative to its row header, each new panel goes at the end of its row, and the script asserts no overlaps and every panel below its own header.
- **Prose repointed, verified mechanically.** No guard reads the catalog's `**Dashboard**:` lines (checked), so the repointing had no backstop. A script repointed 32 lines and then asserted that every line names a panel that EXISTS in the dashboard it names, and that the row named in parentheses matches.
  - That surfaced 7 stale titles that PREDATE this diff (for example "JWT Validations by Result"). One of them, "Media Generation Divergence", named a moved panel, so leaving it would have made it wrong in a new way. All 7 fixed, plus 2 of my own row names ("GC Heartbeat row").
  - Runbooks: the real count was 2 moved-panel pointers. One title spans a line break, which a title search missed and only a `MC Overview` sweep caught. `alerts.md` had 3 pre-existing stale titles.
- **Guard (Lead-directed; machinery is infrastructure's, recorded in the table as Domain-judgment and NOT paired).** New `crates/dt-guard/src/kustomize_configmaps.rs`, run by `dt-guard kustomize` as local checks:
  - **`configmap_annotation_size` — O-26 taken: measures the ESCAPED, per-ConfigMap size, never raw file bytes.** A file-size check would have PASSED the file that failed. The model is Go `encoding/json` with HTML escaping, as kubectl writes it (`<`, `>`, `&` cost 6 bytes each; PromQL is full of them), pinned BYTE-FOR-BYTE against `kubectl create configmap --save-config --dry-run=client` on three real ConfigMaps and on a fixture covering every escape class. The fixture is a unit test. Pinning also caught a CR-translation bug in my Python harness before it reached Rust.
  - Fails above **80%** of 262,144, plus a fixed 1 KiB metadata allowance for overlay-added labels the guard cannot see. **80%, not the 90% I first shipped, and derived rather than chosen** (@infrastructure proposed 80% from rendered sizes; the arithmetic decides it). Before this diff the MC ConfigMap was ALREADY at 95.2%, and this one task added 11.3% of the cap. So a threshold must sit at or below ~88.7% or it is green on a ConfigMap that the next ordinary task breaks. 90% fails that test; 80% leaves about 1.7 tasks of runway, and a unit test pins that one measured task of growth fits between threshold and cap. Everything in the tree passes today (maximum `grafana-dashboards-mh` at 67%). I also corrected a claim in my own first draft of the constant's doc ("sat at ~88%"), which I had not measured; the measured figure is 95.2%.
  - **Vacuity (review-protocol mechanism 5, @infrastructure):** collecting ZERO generators is a finding, not a pass. The rustdoc records that the cap is an artifact of client-side apply, and names server-side apply as the exit (not adopted here: field-manager migration is its own task). Both over-estimate, the safe direction. The message names the ConfigMap and the remedy.
  - **Scope widened on evidence:** it covers EVERY `configMapGenerator` under `infra/`, not only Grafana's. `prometheus-rules` (every service's alert rules in one ConfigMap) is at 49.1% and `mc-alerts.yaml` grew 11 KB in this diff: same failure class, one ConfigMap over.
  - **`dashboard_configmap_label` — O-27 taken.** Every Grafana generator shipping dashboard JSON must carry the label the sidecar selects on. The label and value are READ from the sidecar's `LABEL` / `LABEL_VALUE` in `infra/grafana/deployment.yaml`, never restated. If they cannot be found, that is itself a finding. `grafana-dashboards-config` (provisioning YAML) is correctly out of scope.
  - **Proven both ways with the rebuilt release binary** (the first green came from a stale binary and was discarded): the real tree is OK; the reconstructed pre-split state fails `configmap_annotation_size` at 280,213 bytes (kubectl's 279,189 plus the allowance); mc-media with its label stripped fails `dashboard_configmap_label`. An unreadable referenced file is a finding rather than an `Err`, so it cannot abort the run and hide R-20's clearer finding.
- **Concurrent-DB collision** filed as instance 3 of the existing `docs/TODO.md` entry "Layer 7 reports environment faults on the implementer lane" (the Lead's instruction; not a new entry), with its discriminator and the lock-versus-classification remedies.

### Semantic guard
- Items 1-6 acknowledged; 4 and 5 are reflected in §6 (exit-path placement, and the `tonic::Status` code and message carried into the error rather than logged and discarded). No `Debug` derive is added anywhere under `media_admission/`.

---

## Resume (Iteration 2 — headless infra interruption)

**Feedback**: "This devloop was interrupted before completion. Resume from main.md state: finish incomplete phases, then gates and commit as normal."

Resumed 2026-09-28 at Phase `review` (Tier `full`; Gate 1 already confirmed, not re-run). No Gate-2 result or reviewer verdict had been recorded, so Gate 2 is re-run on the current tree and the full roster is respawned for Gate 3.

---

## Review fixes (Iteration 2, Gate 3)

- **F-GC1 (@paired-global-controller)**: the `NotifyMeetingEnded` retry was dead code (tonic's reconnecting channel reports ready after the first connect, so `channel_ready()` never failed). Now classified from the returned `Status` (`is_provably_unsent`: `tonic::ConnectError` in the source chain; fail-closed), `GcClient::with_channel` seam, tests on a real refused connect with a same-code negative control and paused-clock retry evidence; mutation-checked both ways. (SEC-1 was a transient mutation probe observed mid-run; resolved.)
- **OPS-18 (+ code-reviewer docstring finding)**: graceful shutdown dropped every detached teardown with the runtime. `controller.shutdown()` now answers only after the actor drain; a `TeardownTracker` (counted before spawn) is settled under ONE deadline, `SHUTDOWN_RELEASE_BUDGET` = pod grace 35 − 2 s pre-drain − 5 s margin (MH's settle/margin pattern), cut-offs logged (not counted: never scraped at exit). Manifests carry the coupling; a unit test reads both and fails on drift. Integration test mutation-checked.
- **OPS-19**: the fence records its cause at reap, so a backstop-lifted fence still tells GC an ENDED meeting ended (suppression made the id unjoinable). **OPS-24**: notifications still queued at shutdown are counted as dropped (biased select, cancel first). **OPS-20/21/22/23**: one total deploy order (GC → MC → MH; back MH → MC → GC) in all three runbooks, rollback pointer, `MAX_QUEUED_CREATES` named, MC Media dashboard listed.
- **@test**: self-mute exclusion from MH's muted set now tested (discriminating, mutation-checked). **O-28**: `teardown_in_progress` zero-initialised; the drift test's `all` list pinned by count + distinct-discriminant check (was vacuous). **O-33**: panel grouped by a non-existent `pod` label → `instance`. O-29/30/32/34/35/36/37/38/39 and paired-MH F1-F7, DRY D-1/D-2/D-4: doc/dashboard/comment fixes.
- **dt-guard (infra F2/F4/F5/F7b/F8/F9/F10, SEC-2, DRY D-3)**: REASON tie precedence, shared `strip_inline_comment`, read failures as findings, path containment through `common::path_safety`, bullet-indent entry boundary (key order and zero-indent sequences). **Infra F3**: Layer 7 and `setup.sh` restart Grafana after an observability apply. F6: failure-map rows.
- **OPS-26 (a defect the OPS-18 fix itself created, caught by @operations' re-review)**: the GC-notify drain's token was a child of the shutdown token, so it died at t=0 — while the new release window is exactly when an ended meeting's teardown completes and produces its `NotifyMeetingEnded`. Every such notification would have been dropped: the stale GC assignment row the revive fix exists to prevent, reached through the window the fix added. Now an independent token cancelled AFTER the settle, plus `SHUTDOWN_NOTIFY_FLUSH_BUDGET` (3 s) carved OUT of the release budget so the grace arithmetic still closes exactly (35 = 2 + 25 + 3 + 5), and the drain FLUSHES what is queued instead of counting it dropped. A send cut off at the flush deadline is AMBIGUOUS — not counted, never retried, per at-most-once — and only the provably-never-sent remainder is counted. Three tests (flushed-not-dropped, post-close counted, remainder-past-budget counted via a new mock-GC delay knob); the first two go red if the flush is reverted.
- **OPS-27 / OPS-28 / OPS-29 / O-41 — one class: the third startup field, and the DRY-9 rename, invalidated artifacts that had counted or cited them.** `docs/TODO.md:82`'s field count 2→3; the `mc_gc_meeting_ended_notifications_dropped_total` row's Description (three causes now) and its `Recorded in`, which named `main.rs::GcNotifyQueue` — a type that exists nowhere (the doc→code phantom class this very loop filed a TODO entry about), now the two real sites; Scenario 20's "two structured fields" (which contradicted its own instruction sixteen lines above) and `mc-deployment.md` §5's presence check, whose "same on every pod" rationale was false for precisely the field it omitted; and `handle_end_meeting` → `handle_close_meeting` in the MC catalog, a rename blast-radius miss no compiler or token grep could see. Runbook text by @operations, applied verbatim.
- **Left, reasoned**: DRY D-4's optional `mh-service/src/main.rs` sites ("never sends/calls", a strict subset of "never completes", on a cap-sizing WARN) — literally true, accepted by @dry-reviewer as optional.

---

## Pre-Work

None.

---

## Implementation Summary

### Server mute (R-8..R-11)
| Item | Before | After |
|------|--------|-------|
| `ServerMuteRequest` | unhandled | host-only at dispatch (authority before any target read), own per-connection limiter (no looser than client mute), own refusal-reply limiter; actor re-checks authority; one generic wire refusal |
| MH programming | `server_muted_sources` always empty | per handler = sources of that handler's egress edges ∩ audio-server-muted; `BTreeSet<SenderId>`; bound `|muted| <= |egress_streams|` by construction; a muted-set change alone advances `policy_generation` |
| `UnmuteRequest` | unhandled | relayed to connected hosts with the server-stamped requester id (new additive field `participant_id = 3`); never clears the mute |
| who-muted-whom | `MuteChanged` not wire-serialized | `ParticipantMuteUpdate` broadcast to all incl. target (`payload_kind=participant_update_muted`); late joiner replay immediately after `JoinResponse` |
| reconnect / rejoin | — | mute survives a grace reconnect, not a fresh join |

### Meeting end + MH teardown (R-20, E2)
| Item | Before | After |
|------|--------|-------|
| last participant leaves | meeting actor lived on | meeting ENDS; exit path hands teardown to a detached task (Drop-guard completion report) |
| teardown | none | cancel pushers (own token), await with `PUSH_QUIESCE_BOUND`, on timeout wait one more bound; `EndMeeting` with own `mc_id` to every frozen handler, bounded retry on transport errors only; outcomes counted |
| controller | reaped on health walk | fence per ending meeting id: creates parked (never awaited; cap 16 → `teardown_in_progress`), backstop deadline above `TEARDOWN_MAX` |
| GC | never notified | `NotifyMeetingEnded` after teardown completes, at most once (retry only when provably unsent), via a bounded queue with a counted drop; graceful shutdown releases handlers but does not notify |
| GC assignment | ended row never revivable | `atomic_assign` revives an ended row (predicate + `ended_at = NULL`); batch jobs keyed on `(meeting_id, region)` with predicates re-asserted |

### Additional Changes
- Metrics (all zero-initialised over their full label sets, catalog row, dashboard panel, `MetricAssertion` test): `mc_media_server_mute_requests_total{action,outcome}`, `mc_media_unmute_requests_total{outcome}`, `mc_media_refusal_replies_suppressed_total{surface}`, `mc_media_server_muted_sources`, `mc_media_end_meeting_total{outcome}`, `mc_media_push_quiesce_total{outcome}`, `mc_media_teardown_fence_backstop_total`, `mc_gc_notify_meeting_ended_total{status}`, `mc_gc_meeting_ended_notifications_dropped_total`; `error_type="teardown_in_progress"`.
- Six warning alerts (`MCEndMeetingOwnershipRejected`, `MCEndMeetingFailureRate`, `MCPushQuiesceTimeouts`, `MCTeardownFenceBackstop`, `MCNotifyMeetingEndedFailing`, `MCMeetingEndedNotificationsDropped`), runbook Scenarios 20/21, three-service rollout order in both deployment runbooks, startup fields `push_quiesce_bound_seconds` / `teardown_fence_hold_max_seconds`.
- Paired MH: apply-time refusal for a released meeting + `mh_media_released_meeting_apply_refusals_total`.
- `mc_id_mismatch` renamed `rejected_ownership` before landing (guard: the old token parsed as a metric name); matches MH's own teardown outcome word.
- G1-7 stale-premise sweep run with the concept-keyed grep; every surviving hit is either inverted, narrowed (shape 3) or a different story's task 12.
- Ignore-free `buf breaking` (FILE) vs `e6c0a248`: clean for both proto files; positive control (a type change in each file) detected in both.

---

## Files Modified

See `git diff --stat` at commit time; every path is a row in §Cross-Boundary Classification (the scope guard is green).

---

## Devloop Verification Steps

### Gate 2 — resume run (iteration 2, 2026-09-28)
Layers 1-6 OK (L4 4557 passed / 0 failed); Layer 7 env-tests OK (incl. env-test 35, 3/3) and browser E2E 10/10. The orchestrator then aborted with `LAYER_ALL_EXIT=2` and no summary block: a concurrent `layer-fast.sh` run by a teammate had `rm -f`'d the shared `/tmp/devloop/layer-*.log` files, so the Layer-7 status parse found no file. This is an environment collision, not caused by the diff; no attempt consumed. Review fixes were landing during the run in any case, so Gate 2 is re-run on the final tree before commit.

### Gate 2 — final run (iteration 2)
**GATE2=PASS**, `LAYER_ALL_EXIT=0`, run on the frozen final tree at 2026-09-28T05:14Z.
- L1 OK, L2 OK, L3 OK (113 s).
- L4 N/A: 4576 passed, 0 failed.
- L5 OK, L6 N/A (no dependency changes; buf breaking passed).
- L7 OK (1645 s): Rust env-tests including env-test 35, plus browser E2E 10/10.

Two earlier runs are not counted:
- A run the Lead stopped because the tree was mid-edit (OPS-26). Stopping it killed `dev-cluster setup` mid-way.
- A Layer-7 run that failed with `PRECONDITION_FAILURE REASON=cluster-setup-failed` in 8 s: Calico `create` hit AlreadyExists on the half-built cluster. The Lead ran `dev-cluster teardown`, the same teardown+setup that Layer 7's `infra/kind/` branch performs for this diff, and retried once. That is the operator lane, so no attempt was consumed.

### Layer 1: cargo check
**Status**: PASS/FAIL
**Duration**: ~Xs
**Output**: {Any relevant notes}

### Layer 2: cargo fmt
**Status**: PASS/FAIL
**Duration**: ~Xs
**Output**: {Any relevant notes}

### Layer 3: Simple Guards
**Status**: ALL PASS / X FAILED
**Duration**: ~Xs

| Guard | Status |
|-------|--------|
| api-version-check | PASS/FAIL |
| no-hardcoded-secrets | PASS/FAIL |
| no-pii-in-logs | PASS/FAIL |
| no-secrets-in-logs | PASS/FAIL |
| test-coverage | PASS/FAIL |

{Details on any failures}

### Layer 4: Unit Tests
**Status**: PASS/FAIL
**Duration**: ~Xs
**Output**: {Test counts, any failures}

### Layer 5: All Tests (Integration)
**Status**: PASS/FAIL
**Duration**: ~Xs
**Tests**: {X passed, Y failed}

{Details on any failures}

### Layer 6: Clippy
**Status**: PASS/FAIL
**Duration**: ~Xs
**Output**: {Any warnings}

### Layer 7: Env-tests
**Status**: PASS/FAIL
**Duration**: ~Xs
**Output**: {Wall-clock time for dev-cluster rebuild + env-test run; pass/fail summary; log path}

(Semantic-guard relocated to the Gate 2 reviewer panel per ADR-0033 Wave 3 #9. See § Code Review Results → Semantic Guard Reviewer below for its findings.)

---

## Code Review Results

Gate 3 (Iteration 2, after the resume). Every reviewer verified the fixes against the tree.

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 2 | 2 | 0 | SEC-1: `is_provably_unsent` short-circuit removed. SEC-2: dt-guard source paths now go through `path_safety` |
| Test | RESOLVED-FIXED | 1 | 1 | 0 | New test: a self-muted participant is never in the MH muted set |
| Observability | RESOLVED-FIXED | 14 | 14 | 0 | O-28..O-41 (vacuous join-failure drift test; `by (pod)` → `by (instance)`). Filed out-of-scope TODOs, which are not deferrals (Lead accepts the classification) |
| Code Quality | RESOLVED-FIXED | 1 | 1 | 0 | Shutdown-release docstrings, made true by the OPS-18 behavioural fix |
| DRY | RESOLVED-DEFERRED | 4 | 4 | 0 | D-1..D-4 fixed. DEFERRED only because of three extraction-opportunity TODO pointers (protocol rule) |
| Operations | RESOLVED-FIXED | 12 | 12 | 0 | OPS-18: shutdown drain + release settle under a pod-grace-derived budget. OPS-19: backstop now notifies GC. OPS-20: one composed rollout order. OPS-21..24. OPS-25 was fixed by operations. OPS-26: notify flush during the settle. OPS-27..29: third startup field docs |
| Semantic Guard | CLEAR (SAFE) | 0 | 0 | 0 | |
| Paired media-handler | RESOLVED-FIXED | 7 | 7 | 0 | F1..F7. Ownership Lens confirmed (session/mod.rs guard) |
| Protocol (GSA) | CLEAR | 0 | 0 | 0 | Additive `UnmuteRequest.participant_id = 3`; internal.proto comment-only. GSA owner confirmed |
| Paired global-controller | RESOLVED-FIXED | 1 | 1 | 0 | F-GC1: the notify retry could never fire; it now retries on `tonic::ConnectError` only. Ownership Lens confirmed |
| Database | CLEAR | 0 | 0 | 0 | Revive/EPQ race safety and index use verified; no migration |
| Infrastructure | RESOLVED-FIXED | 10 | 10 | 0 | Grafana rollout after overlay apply; two fail-open parser shapes; REASON tie-break. Ownership Lens confirmed |

Lead rulings at Gate 3:
- Infrastructure's Ownership-Lens caveat is accepted. The owner was absent from Gate 1 because the dt-guard edit was created by a Gate-2 failure. The owner shaped the policy before landing and reviewed it here, which satisfies §6.3.
- Observability's out-of-scope TODO filings stay RESOLVED-FIXED. They are not findings against this diff.

## Accepted Deferrals

**Each entry here is an issue the devloop chose NOT to fix.** Every bullet is a cost shift: the implementer didn't pay the fix-now cost, so a future reader will pay fix-later cost + tracking overhead. List only what was actually deferred — not "follow-ups" or "future improvements" or "potential extractions." If something was fixed, it doesn't belong here.

**Tech debt entries themselves live in `docs/TODO.md`. This section holds only pointers to those entries.** Do not create a `TODO.md` at the repo root or anywhere else — there is exactly one `docs/TODO.md` for the whole project. Do not inline the debt body here — multi-line entries belong in `docs/TODO.md`, not in this section.

Each pointer is exactly one bullet of the form `- \`docs/TODO.md\` §SECTION-NAME — one-line hook (≤80 chars)`. If you wrote more than one line per entry, you're writing it in the wrong file — move the body to `docs/TODO.md` and leave only the pointer here.

Examples:

```
- `docs/TODO.md` §Observability Debt — orphan recording-site audit follow-up
- `docs/TODO.md` §Cross-Service Duplication (DRY) — extract record_token_refresh_metrics
```

or:

```
- (none surfaced in this devloop)
```

- `docs/TODO.md` §Infrastructure Validation in Devloops — size guard skips literal ConfigMaps
- `docs/TODO.md` §Media Path Obligations — MC fresh gRPC channel per MH call (OPS-17)
- `docs/TODO.md` §Media Path Obligations — `NotifyMeetingEnded` is not incarnation-safe
- `docs/TODO.md` §Cross-Service Duplication (DRY) — `configMapGenerator` parsed twice in `dt-guard`
- `docs/TODO.md` §Cross-Service Duplication (DRY) — three per-meeting registers, no forcing function
- `docs/TODO.md` §Cross-Service Duplication (DRY) — `enum + ALL + label()` sizing update (+6 sites)

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `{start_commit}`
2. Review all changes: `git diff {start_commit}..HEAD`
3. Soft reset (preserves changes): `git reset --soft {start_commit}`
4. Hard reset (clean revert): `git reset --hard {start_commit}`
5. For schema changes: rollback requires a forward migration — `git reset` alone is insufficient if migrations were applied
6. For infrastructure changes: may require `skaffold delete` or `kubectl delete -f` if manifests were applied
7. **Safe-revert unit** (answer explicitly, even if "the whole commit"): can any part of this diff be reverted or cherry-picked on its own, or does a partial revert reconstruct a state worse than either endpoint (e.g. a security fix split from the change that made it necessary, or a client/server/alert-rule set that must move together)? If partial reverts are unsafe, name the unit and the safe direction. **Answer**: the whole commit for MC + GC + MH. Partial reverts are unsafe in one direction: MC's end-on-empty without GC's revive makes an ended meeting id unjoinable for `GC_RETENTION_DAYS`; MC's `EndMeeting` calls without MH's RPC are counted `unimplemented` (safe). Deploy order: forward GC → MC → MH, rollback MH → MC → GC (MC always before GC on the way back; the MC/MH sender_id rule in `mh-deployment.md` wins where the two appear to conflict — Gate 3 OPS-20). The alert rules move with MC. (This converts silence into a visible unanswered slot; it cannot distinguish a checked answer from a reflexive one.)

---

## Issues Encountered & Resolutions

### Issue 1: GC could never re-assign an ended meeting id
**Problem**: E2 made the ordinary leave-then-rejoin sequence reach a latent GC defect (ended row never revivable; 503 for the retention window).
**Resolution**: Lead ruling Option 1 — query-only revive with both halves, plus the two batch-job cross-region fixes; tests incl. the failover case.

### Issue 2: `mc_id_mismatch` read as a metric name by `validate-application-metrics`
**Problem**: the outcome token matched the `mc_` metric-name shape.
**Resolution**: renamed to `rejected_ownership` everywhere (code, catalog, dashboard, alert `MCEndMeetingOwnershipRejected`), which also matches MH's own teardown outcome word.

### Issue 3: Nx served stale TypeScript codegen
**Problem**: `pnpm nx run proto-gen:codegen` reported a local cache hit, but the generated `UnmuteRequest` lacked the new field.
**Resolution**: re-ran with `--skip-nx-cache`; `tsc --noEmit` on sdk-core clean. The generated tree is gitignored, so nothing stale is committed.

---

## Lessons Learned

1. {Key takeaway 1}
2. {Key takeaway 2}
3. {Key takeaway 3}

{Add more as applicable}

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
