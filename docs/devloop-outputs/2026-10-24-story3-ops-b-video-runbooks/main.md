# Devloop Output: Story 3 OPS-B — video incident scenarios, extended media scenarios, client-dev-local ladder, 24-hour watch list

**Date**: 2026-10-24
**Task**: OPS-B (story `docs/user-stories/2026-10-07-see-each-other.md`, R-31): MH 20–23, MC 22–23, extended MH/MC scenarios, `client-dev-local` F21+ ladder, 24-hour watch list in MH/MC deployment checklists
**Specialist**: operations
**Mode**: Agent Teams (v2) — full panel; Gate-1 SKIPPED — documentation-only task (run-story manifest tier=light)
**Branch**: `feature/user-see-each-other`
**Duration**: ~3h

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `b9346b5c5ec852525fc47bb76d0919fc9bdd0162` |
| Branch | `feature/user-see-each-other` |
| Lead Model | `claude-opus-5-5[1m]` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` |
| Implementing Specialist | `operations` |
| Tier | `light — documentation-only runbook task (manifest tier=light)` |
| Iteration | `1` |
| Security | `security` |
| Test | `test` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Semantic Guard | not spawned (docs-only diff; no check surface) |
| Media Handler (conditional, token semantics) | `media-handler` |
| Meeting Controller (conditional, token semantics) | `meeting-controller` |
| Client (conditional, client tokens) | `client` |

---

## Task Overview

### Objective
Write the operator surface for story 3's video path: new runbook scenarios whose anchors OBS-B's alerts (A1–A9) will link to, extended existing scenarios, the client-dev-local ladder and the 24-hour watch list. Owns no alert rule.

### Scope
- **Service(s)**: runbooks only (MH, MC, client-dev-local)
- **Schema**: No
- **Cross-cutting**: Yes (documentation spanning MH/MC/client)

### Debate Decision
NOT NEEDED - design fixed by story file + ADR-0036.

---

## Cross-Boundary Classification

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `docs/runbooks/**` | Mine | — |
| `docs/devloop-outputs/2026-10-24-story3-ops-b-video-runbooks/main.md` | Mine | — |
| `docs/TODO.md` (two entries: backlog-in-use gap, runbook_url fragment-guard addendum) | Not mine, Minor-judgment | lead (deferral tracking requested by @observability, @test) |
| `docs/observability/metrics/mc-service.md` (`media_kind_unspecified` bullet) | Not mine, Minor-judgment | observability (correction raised by @meeting-controller) |
| `crates/mh-service/src/observability/metrics.rs` (`MediaDropReason::StreamRateLimited` doc comment only) | Not mine, Minor-judgment | media-handler (raised and approved by @media-handler; approved by lead) |
| `crates/mc-service/src/media_signaling/outcome.rs` (`CapabilityOutcome::MediaKindUnspecified` rustdoc only) | Not mine, Minor-judgment | meeting-controller (raised and approved by @meeting-controller; approved by lead) |

---

### Gate 1 — SKIPPED (tier=light; reason: documentation-only runbook task, manifest tier=light)

## Planning

**Inline plan (tier light, Gate 1 skipped).** This touches no Guarded Shared Area and no code. Every edit is under `docs/runbooks/**` plus this file. The Cross-Boundary table above is unchanged.

**Scenario numbers, re-verified 2026-10-24 against `grep -n '^###' docs/runbooks/*.md`:**
- MH: Scenarios 1–19 are used, so **20–23 are free**.
- MC: Scenarios 1–21 are used, so **22–23 are free**.
- `client-dev-local`: **F21 and F22 are already used**. C-B shipped F21 (`media.videoReceive` config error) and F22 (frozen or blank video slots) after planning. The ladder therefore starts at **F23**: F23–F27. This is the re-verification the prompt asks for. The story file says "F21 onward", and continuing past the C-B entries honours that intent. It is not a wire-freeze conflict.

**Wire-freeze check.** The prompt agrees with the story file's Design §Wire freeze and Amendment 4: keyframe causes, recovery outcomes, no MH retry/aggregation/recovery-rate keys, MC-only `MC_VIDEO_MIN_KEYFRAME_INTERVAL_MS`, and the A9 anchor in the MC runbook pointing to MH 22. No disagreement was found, so there is no stop.

**Anchor and `runbook_url` safety.**
- I renamed no existing heading.
- No `runbook_url` targets MC 18 (`grep -rn runbook_url infra/docker/prometheus/rules/`). It is still linked 5× from `client-dev-local.md` and from inside `mc-incident-response.md`. So the heading is KEPT, and a "sees or hears" line is added under it, because the anchor must stay stable.
- No rule file is touched.

**Files to change:**
- `docs/runbooks/mh-incident-response.md`: TOC; new Scenarios 20–23; MH 19 video arm with two sub-arms; MH 18 video extension.
- `docs/runbooks/mc-incident-response.md`: TOC; new Scenarios 22–23; extensions to MC 8, MC 16, MC 18 and MC 21.
- `docs/runbooks/client-dev-local.md`: F23–F27; F22's generic MH link retargeted to MH 22 / MC 23.
- `docs/runbooks/mh-deployment.md`: the 24-hour watch list, the single home for its queries.
- `docs/runbooks/mc-deployment.md`: the watch-list pointer and the MC-side items. The queries are not duplicated, following the file's existing "canonical checklist lives in mh-deployment.md" convention.

**Token / series verification table** (all paths repo-relative):

| Cited | Defined at | Note |
|---|---|---|
| `mh_media_group_starts_total{direction}` | `crates/mh-service/src/observability/metrics.rs:2176` (doc 1979) | |
| `mh_media_group_resets_total{reason}`: `subscriber_backlog`, `publisher_reset`, `publisher_stall`, `source_ended`, `edge_removed` | `metrics.rs:1596-1649` (`GroupResetReason`) | |
| `mh_media_keyframe_requests_sent_total{trigger}`: `initial`, `receiver_recovery` | `metrics.rs:1651-1679` | |
| `mh_media_keyframe_request_write_failures_total` | `metrics.rs` (MH-B) | |
| `mh_media_recovery_requests_received_total{outcome}`: `forwarded`, `duplicate`, `stale`, `counter_ahead`, `source_mismatch`, `unmapped_slot`, `malformed` | `metrics.rs:1681-1735` (`RecoveryOutcome`) | prompt lists 4 of the 7 |
| `mh_media_edges_pending`, `mh_media_edge_pending_oldest_seconds` | `metrics.rs:2214`, `:2218` | |
| `mh_media_edge_pending_exits_total{outcome}`, `mh_media_edge_pending_duration_seconds` | `metrics.rs:1737-1790` (`PendingExit`) | |
| `mh_media_paused_source_frames_total` | `metrics.rs:2168` | |
| `mh_media_frames_dropped_total{reason}`: `server_muted`, `stream_rate_limited`, `partial_frame_discard`, `unmatched_stream`, `transport_mismatch` | `metrics.rs:1311-1540` (`MediaDropReason`) | |
| `mh_media_stream_ingress_groups` (concurrent ingress groups) | `metrics.rs:2189` | |
| `mh_media_uni_streams_per_connection_limit` (declared QUIC concurrency limit) | `metrics.rs:2278` | |
| `mh_media_egress_stream_backlog_limit_bytes` (backlog cap) | `metrics.rs:2283` | **no backlog-IN-USE series exists in the tree**; asked @observability |
| `mh_media_stream_mid_frame_stall_seconds` | `metrics.rs:2288` | |
| `mh_media_stream_admission_rejection_ratio` / `_threshold` | `metrics.rs:746`, `:563` | |
| `mh_media_egress_stream_ceiling_recommended_min` = `EGRESS_STREAM_CEILING_RECOMMENDED_MIN` = 6×(5+4)=54 | `crates/mh-service/src/config.rs:328-349` | |
| `mh_media_policy_applies_total{outcome="rejected_transport_inconsistent"}` | `metrics.rs:396` | |
| `MH_EGRESS_STREAM_BACKLOG_BYTES`, `MH_STREAM_MID_FRAME_STALL_MS`, `MH_STREAM_OPENS_PER_CONNECTION_PER_SEC`, `MH_POD_MEMORY_LIMIT_BYTES` | `crates/mh-service/src/config.rs:937-967`; `infra/services/mh-service/config.env:774,803`; `mh-1-deployment.yaml:285` | |
| `MHHighMemory` | `infra/docker/prometheus/rules/mh-alerts.yaml:677` | existing; anchor unchanged |
| `mc_media_policy_pushes_total{outcome="transport_mode_mismatch"}` | `crates/mc-service/src/media_routing/confirm.rs:165`; `observability/metrics.rs:491` | |
| `mc_media_video_keyframe_min_interval_seconds` | `crates/mc-service/src/observability/metrics.rs:1095` | |
| `MC_VIDEO_MIN_KEYFRAME_INTERVAL_MS` | `docs/runbooks/mc-deployment.md:880` row (MC-A) | |
| `mc_media_send_directives_total{outcome}`, `mc_media_send_directive_withdrawals_total{media_kind,reason}`: `unwatched`, `server_muted` | `metrics.rs:696`, `:1072`; `media_signaling/outcome.rs:710-711` | |
| `mc_media_server_mute_requests_total`, `mc_media_mute_requests_rejected_total{surface,reason}`, `mc_media_refusal_replies_suppressed_total`, `mc_media_server_muted_sources` | `metrics.rs:1197`, `:1249`, `:1272` | |
| `mc_media_slot_states_total{slot_state,media_kind}`, `mc_media_receive_slot_cap{media_kind}` | `metrics.rs:736`, `:1042` | |
| `mc_media_receive_capability_declarations_total{outcome}`: `would_publish_and_receive_nothing`, `media_kind_unsupported`, `media_kind_unspecified` | `crates/mc-service/src/media_signaling/outcome.rs:112-121` | the later-redeclaration refusal token |
| `mc_session_join_failures_total{error_type}`: `header_version_unsupported`, `publish_kind_invalid`, `receive_capability_invalid`, `publishes_and_receives_nothing`, `reconnect_publish_set_mismatch` | `metrics.rs:2080-2096` | already in MC 8 item 12 (MC-A) |
| `MC_MAX_AUDIO_RECEIVE_SLOTS`, `MC_MAX_VIDEO_RECEIVE_SLOTS` | `infra/services/mc-service/config.env:108`, `:127` | |
| `dt_client_media_keyframes_produced_total{source}`: `request`, `directive_start`, `abandonment` | `packages/sdk-core/src/media/setup/mediaMetrics.ts:397-417`; panel "Video Keyframes Produced by Cause" `infra/grafana/dashboards/client-media.json:2456` | prompt says "take the series name from the tree" |
| `dt_client_media_keyframe_requests_received_total{outcome}`: `honoured`, `honoured_counter_ahead`, `stale_counter`, `min_interval`, `not_sending`; `_keyframe_requests_unattributed_total` | `mediaMetrics.ts:338-380` | the client refusal reasons (C-A1/C-A2) |
| `dt_client_media_capture_failures_total{outcome}`: `permission_denied`, …, `ended` | `mediaMetrics.ts:302-324` | |
| `dt_client_media_frames_dropped_total{reason}`: `sender_server_muted`, `self_sender`, `decode_over_bound`, `stream_partial_reset`, `stream_slot_mismatch`, `unknown_version` | `packages/sdk-core/src/media/pipeline/receivePolicy.ts:117-118`; `videoIngress.ts:70-82`; `media/frame/rejectReason.ts:83,105` | |
| `dt_client_media_decoder_errors_total`, `_decode_queue_dropped_total`, `_incoming_streams_refused_total`, `_recovery_requests_sent_total{source}` | `mediaMetrics.ts:819`, `:852`, `:992` | |
| `dt_client_media_send_directive_header_version_refused_total` (v2-directive refusal) | `mediaMetrics.ts:1192` | |
| `dt_client_media_slot_unfulfilled_duration_ms` (histogram), `dt_client_media_slots_unfulfilled` (gauge) | `mediaMetrics.ts:1015`, `:1028`; `docs/observability/metrics/client.md:1031,1061` | |
| `VideoCodecUnsupportedError` / `probeVp8Support` | `packages/sdk-core/src/media/setup/vp8.ts:43,156` | |
| `DT_TEST_PATTERN=1` build-time gate (`__DT_TEST_PATTERN__`) | `packages/web-app/vite.config.ts:49-52`; `scripts/dev-web.sh:151-159` | |
| Debug panel "Show debug options" (R-7 self-check, R-35 stats) | `packages/web-app/src/views/DebugPanel.svelte`, `lib/debugToggle.ts` (C-C2) | |
| A1 `MHVideoKeyframeStorm`, A2 `MHVideoSubscriberBacklogResets`, A3 `MHVideoEdgeStuckPending`, A4 `MHPausedSourceIngress`, A5 `ClientVideoServerMutedSenderFrames`, A6 `ClientVideoDecodeErrorRatio`, A8 `MHTransportInconsistent`, A9 `ClientVideoSlotUnfulfilled` | **introduced by OBS-B (owner: observability)** | not in the rules tree yet; names confirmed by @observability from the OBS-B spec. A7 `MCMediaMissingKeyMaterial` exists |

**Dead keys.** `MH_KEYFRAME_REQUEST_RETRY_MS`, `MH_KEYFRAME_REQUEST_AGGREGATION_MS` and `MH_RECOVERY_REQUESTS_PER_CONNECTION_PER_SEC` have **0 hits** in `docs/runbooks/` at start, so there is nothing to remove. `MEDIA_VIDEO_MIN_KEYFRAME_INTERVAL_MS` survives only in MC-A's historical rollback note in the `mc-deployment.md` row, which is correct and is left alone.

**Shape.** Each new scenario carries `**Alert**` / `**Severity**` / `**Runbook Section**`, then bold **Detection**, **Diagnosis** (forks), **Resolution** and **Escalation** labels. These are bold labels, not `####` headings, so no new anchors collide.

---

## Pre-Work

None

---

## Implementation Summary

**New scenarios.** Each has the exact heading and slug OBS-B was given, confirmed with @observability.
- MH 20 `#scenario-20-video-keyframe-storm`, MH 21 `#scenario-21-slow-video-subscriber-stream-credit-stall`, MH 22 `#scenario-22-video-slot-stuck-pending`, MH 23 `#scenario-23-client-sending-while-client-muted`.
- MC 22 `#scenario-22-client-video-decode-failures`, MC 23 `#scenario-23-client-video-slot-unfulfilled`.
- Each one carries `**Alert**` / `**Severity**` / `**Runbook Section**` and then bold Detection / Diagnosis / Resolution / Escalation (plus Related). Every example query is pod-level or fleet-level, with no identity, frame size, keyframe timing or GoP value.

**Extended scenarios.** No existing heading was renamed.
- MH 19 gains the video arm. It covers per-declared-stream `DROP_AT_INGRESS`, the slot-terms R-16 statement, MH `server_muted` as proof and client `sender_server_muted` / A5 as corroboration, plus two sub-arms: "Camera light still on after server mute" and "Video not resuming after server unmute". Its escalation line is updated to match.
- MH 18 gains a video block: derived costs, the larger cost dividing the budget, recommended min = 6×(5+4) = 54, the R-26 lever with its MC-restart cost, and A2 as the leading indicator.
- MH 17's OPS-B pointer now links to MH 20–22.
- MC 8 gains the later-redeclaration refusals (`would_publish_and_receive_nothing`, `media_kind_unsupported`, `media_kind_unspecified` on `mc_media_receive_capability_declarations_total`) and the skew-table pointer. MC-A had already landed the join tokens as item 12.
- MC 16 gains the per-kind note (A7 per `media_kind`). Video-only points at R-14 rotation, with in-band keys after every key change. Audio-only points at story 2's arms.
- MC 18 keeps its heading. Its anchor is linked 5× from `client-dev-local.md` and from inside `mc-incident-response.md`, though no `runbook_url` targets it. It gains a "sees or hears" line and arm (b-video): per-kind caps, truncation to `unavailable`, receive-only as `avatar_no_video`, the fill rule, stability, cap 0 = video off, and `unreachable_sender_ids`.
- MC 21 gains the four-independent-controls paragraph (R-15) and a pointer to MH 19's video sub-arms.

**`client-dev-local`.** F21 and F22 were already taken by C-B, so the ladder is **F23–F27**:
- F23: camera permission denied.
- F24: "no video in a tab", forking on VP8 unsupported, the test-pattern build gate, cap 0 / `unavailable`, and `avatar_*` reasons.
- F25: a v2 directive refused.
- F26: unfulfilled, pointing at MC 23.
- F27: the R-7 self-check and R-35 overlay, local only and never exported.

F22's generic MH link is retargeted to MC 23 / MH 22. The F-entries point at MC 22 rather than copying its forks.

**24-hour watch list.** All 12 items, in rise order, with their queries, are in `mh-deployment.md` §24-hour check (the new `####` subsection is the single home). `mc-deployment.md` §Post-Deploy Monitoring Checklist: MC↔MH Coordination points there and lists only the MC-side items. This follows that file's existing "canonical checklist lives in mh-deployment.md" convention, so no queries are duplicated.

**Dead keys.** `MH_KEYFRAME_REQUEST_RETRY_MS`, `MH_KEYFRAME_REQUEST_AGGREGATION_MS` and `MH_RECOVERY_REQUESTS_PER_CONNECTION_PER_SEC` have **zero survivors** in `docs/runbooks/`, before and after this change. MH-B and MC-A had already removed them.

**Name differences vs the prompt (the tree wins):**
- "keyframes produced by cause" is `dt_client_media_keyframes_produced_total{source}`. Its panel, *Video Keyframes Produced by Cause*, already exists.
- The "client refusal reasons" are `dt_client_media_keyframe_requests_received_total{outcome}`: `not_sending`, `stale_counter`, `min_interval`, `honoured`, `honoured_counter_ahead`.
- The v2-directive refusal token is the series `dt_client_media_send_directive_header_version_refused_total`.
- MH `rejected_transport_inconsistent` is an `outcome` on `mh_media_policy_applies_total`. MC `transport_mode_mismatch` is an `outcome` on `mc_media_policy_pushes_total`.
- The concurrent-ingress-groups gauge is `mh_media_stream_ingress_groups`, and the declared-limit gauge is `mh_media_uni_streams_per_connection_limit`.
- Recovery outcomes include three values the prompt does not list: `forwarded`, `unmapped_slot` and `malformed`.
- The `client-dev-local` ladder starts at F23, not F21.
- **The A1 denominator (flagged for OBS-B).** The OBS-B spec, and the prompt's "per active source stream", assume `mh_media_stream_sources_active`, which does NOT exist. The tree has `mh_media_stream_sources_declared`, which counts every STREAM_PER_GROUP stream declared in installed policy, PAUSED and DROP_AT_INGRESS included. So declared is at least active, and the per-source ratio reads low. MH 20 and watch-list item 6 use `_declared`. OBS-B's A1 selector must use `_declared`, or raise the missing `_active` gauge; otherwise its selector returns nothing.
- `mc_media_receive_capability_declarations_total{outcome="media_kind_unspecified"}` was documented as "read version skew first" in the MC catalog, the MC runbook home section and the `CapabilityOutcome::MediaKindUnspecified` rustdoc. Since MC-A the header-version gate refuses a pre-ADR-0036 peer first, so it is a v3-client defect. The runbook, catalog and rustdoc are corrected (raised by @meeting-controller).

**Introduced by OBS-B (owner: observability), not yet in the rules tree.** Names were confirmed by @observability from the OBS-B spec:
- A1 `MHVideoKeyframeStorm`
- A2 `MHVideoSubscriberBacklogResets`
- A3 `MHVideoEdgeStuckPending`
- A4 `MHPausedSourceIngress`
- A5 `ClientVideoServerMutedSenderFrames`
- A6 `ClientVideoDecodeErrorRatio` (decoder errors / frames accepted, video)
- A8 `MHTransportInconsistent`
- A9 `ClientVideoSlotUnfulfilled`

A7 `MCMediaMissingKeyMaterial` already exists.

**Gap recorded: no MH backlog-in-use series exists.** MH-A2 shipped only the cap gauge, `mh_media_egress_stream_backlog_limit_bytes`. The OBS-B spec assumes a "backlog cap and in-use" pair and a "backlog-in-use beside backlog-cap panel". The runbooks (MH 21 and watch-list item 3) state that no in-use gauge exists and use the cap plus the `subscriber_backlog` reset rate as the proxy. Memory points at `MHHighMemory`'s expression. **Owners:** `media-handler` (emitting the series) and `observability` (the OBS-B panel must not be built on an empty selector). This is task-sized: it is a new metric with catalog, dashboard and guard coverage, so it is not an in-tree doc fix. **Constraint (security):** any backlog-in-use series must be a pod-level aggregate only. A per-stream or per-destination backlog gauge would be a per-stream size signal under ADR-0036 §11. Tracked in `docs/TODO.md` §Observability Debt, "No MH backlog-in-use series".

---

## Files Modified

- `docs/runbooks/mh-incident-response.md`: TOC; Scenarios 20–23 (new); MH 19 video arm and escalation; MH 18 video block; MH 17 pointer
- `docs/runbooks/mc-incident-response.md`: TOC; Scenarios 22–23 (new); MC 8, MC 16, MC 18, MC 21 extensions
- `docs/runbooks/client-dev-local.md`: F23–F27 (new); F22 link retargeted
- `docs/runbooks/mh-deployment.md`: §24-hour check, "Story 3 video: the 24-hour watch list"
- `docs/runbooks/mc-deployment.md`: watch-list pointer and MC-side items in §Post-Deploy Monitoring Checklist: MC↔MH Coordination
- `docs/TODO.md`: new §Observability Debt entry (backlog-in-use gap); story 3 addendum to the `runbook_url` fragment-guard entry
- `docs/observability/metrics/mc-service.md`: `media_kind_unspecified` reading corrected
- `crates/mc-service/src/media_signaling/outcome.rs`: `MediaKindUnspecified` rustdoc (no longer "skew first")
- `crates/mh-service/src/observability/metrics.rs`: `StreamRateLimited` doc comment (header read before refusal, since MH-B)
- this file

## Devloop Verification Steps

- `grep -rn runbook_url infra/docker/prometheus/rules/`: no existing target heading was changed. A local slug check over every `](file.md#anchor)` link in the five edited runbooks reports 0 unresolved.
- Forbidden keys grep over `docs/runbooks/`: 0 hits.
- No end-to-end, zero-trust or forward-secrecy wording in the added lines, and no identity field in any added query.
- `./scripts/layer-fast.sh`: TOTAL_RESULT=OK (layers 1–6).
- **dt-guard does not check anchors.** Its `runbook_url` check (`crates/dt-guard/src/alert_rules.rs::validate_runbook_url`) strips the `#fragment` and checks only that the file exists. So "dt-guard passes" is NOT proof that OBS-B's anchors resolve. They were verified only by the local slug check above, and by @test's and @observability's independent checks at review. The guard gap is tracked in `docs/TODO.md`, "`runbook_url` fragments are not resolved by any guard" (story 3 addendum).

- **Gate 2** (`DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh`, Lead, after Gate 3): TOTAL_RESULT=OK — L1 OK, L2 OK, L3 OK, L4 OK, L5 OK, L6 OK, L7 OK (env-tests + browser E2E, 1467s).

---

## Code Review Results

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 3 | 3 | 0 | mute wording consistency (watch-list item 8 vs MH 19); MC assignment state does not name slot occupants; MH 21 false boot-validation claim |
| Test | RESOLVED-DEFERRED | 2 | 1 | 1 | dt-guard `validate_runbook_url` ignores `#fragment` — spin-out to infrastructure (TODO addendum); verification note in main.md |
| Observability | RESOLVED-DEFERRED | 4 | 3 | 1 | `dt_client_media_slots_unfulfilled` is single-writer, not a fleet count; MH ingress `unknown_version`; `_active`→`_declared`; backlog-in-use series deferred |
| Code Quality | RESOLVED-FIXED | 4 | 4 | 0 | MH 19 header vs A5; main.md alert-name row; mc-deployment item-12 wording; whitespace |
| DRY | RESOLVED-FIXED | 4 | 4 | 0 | MC 8 copy → link (source corrected in runbook, catalog, rustdoc); thresholds → alert links; 54 → recommended-min gauge; lever text → OPS-A link |
| Operations | RESOLVED-FIXED | 4 | 4 | 0 | MH 21 row; host vs devloop redeploy command; MC 18 remedy costs; items 10/11 gated on item 12. Cosmetic paragraph split applied by Lead |
| Semantic Guard | not spawned | — | — | — | docs + comment-only diff; no check surface |
| Media Handler (conditional) | RESOLVED-FIXED | 4 | 4 | 0 | MH 21 boot claim; recovery-request counting; MH `unknown_version`; stale `StreamRateLimited` doc comment |
| Meeting Controller (conditional) | RESOLVED-FIXED | 5 | 5 | 0 | slot occupancy claim; `media_kind_unspecified` semantics (rustdoc + catalog corrected); unknown-kind case; MC 21 refusal bounding; MC 18 redeclaration wording |
| Client (conditional) | RESOLVED-FIXED | 9 | 9 | 0 | mic metered not played back; tile copy; single-writer gauge; `not_sending` refusal; test-pattern wording; skew-table series; `no_transmit_key`; unmute burst discriminator |

At least one accepted deferral exists (Test, Observability) — see §Accepted Deferrals.

---

## Accepted Deferrals

- `docs/TODO.md` §Observability Debt — No MH backlog-in-use series: OBS-B's spec assumes one (owners media-handler + observability; accepted by @observability, @security)
- `docs/TODO.md` §Observability Debt — `runbook_url` fragments are not resolved by any guard, story 3 addendum (owner infrastructure; accepted by @test)

---

## Rollback Procedure

1. Verify start commit from Loop Metadata: `b9346b5c5ec852525fc47bb76d0919fc9bdd0162`
2. Review all changes: `git diff b9346b5c..HEAD`
3. Soft reset: `git reset --soft b9346b5c`
4. Hard reset: `git reset --hard b9346b5c`
7. **Safe-revert unit**: the whole commit. It is documentation plus two comment-only Rust doc edits (MH `StreamRateLimited`, MC `MediaKindUnspecified`), the MC catalog line and `docs/TODO.md`; no behaviour, config or rule change. A partial revert of the `media_kind_unspecified` trio (runbook / catalog / rustdoc) would re-create the drift the review fixed, so revert them together.

---

## Issues Encountered & Resolutions

- Prompt said the `client-dev-local` ladder continues from F21; C-B had taken F21/F22, so it starts at F23 (renumbering, not a wire-freeze conflict).
- OBS-B spec's A1 denominator `mh_media_stream_sources_active` does not exist; tree has `mh_media_stream_sources_declared` (recorded for OBS-B).
- No MH backlog-in-use series exists (deferred, see §Accepted Deferrals).

---

## Lessons Learned

1. Story-file lines superseded by an amendment (here Amendment 4) leak into runbook prose; owner-reviewers checking claims against shipped code caught every instance.
2. A doc-copy that disagrees with its source can mean the *source* is stale — fix the source (runbook, catalog, rustdoc) rather than the copy.
