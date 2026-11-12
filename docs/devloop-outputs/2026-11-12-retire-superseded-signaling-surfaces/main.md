# Devloop Output: Retire superseded signaling/connect surfaces (Story 4 task P3)

**Date**: 2026-11-12
**Task**: Story 4 task P3 — delete JoinResponse.media_servers, SendTarget/StreamAssignment.media_handler_url, MhConnectionStatus.mh_url, MhConnectRequest.join_token from wire, generated code, production code, tests, env-test fixtures and docs (run-story task #22)
**Specialist**: protocol
**Mode**: Agent Teams (v2) — full, Gate-1 present
**Branch**: `feature/mh-failure-transfer`
**Duration**: ~3h

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `20aef3fe3a8e2b384811b03158eba8fd5a9b5d4a` |
| Branch | `feature/mh-failure-transfer` |
| Lead Model | `claude-opus-5-5` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` |
| Implementing Specialist | `protocol` |
| Tier | `full` |
| Iteration | `2` |
| Paired | `paired-client` (client) |
| Meeting Controller (owner reviewer) | `meeting-controller` |
| Security | `security` |
| Test | `test` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Semantic Guard | `semantic-guard` (spawned: build.rs skip_debug note on MhConnectRequest credential field) |

---

## Task Overview

### Objective
See run-story task #22 prompt (Story 4 task P3). Story: `docs/user-stories/2026-10-31-survive-handler-loss.md`; ADR-0039 §4–5. R-36, R-17.

### Scope
- **Service(s)**: proto, proto-gen, MC, sdk-core, web-app, env-tests, docs
- **Schema**: No
- **Cross-cutting**: Yes

### Debate Decision
NOT NEEDED — ADR-0039 (Accepted) governs.

---

## Cross-Boundary Classification

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `proto/dark_tower/signaling/v1/signaling.proto`  | GSA (wire deletion + reserved tag/name; credential field removal); reviewers: security co-review | protocol |
| `proto/dark_tower/internal/v1/internal.proto` (comment sweep, L1481) | GSA | protocol |
| `crates/proto-gen/build.rs` (MhConnectRequest skip_debug note; entry KEPT) | GSA, credential; reviewers: security + semantic-guard | protocol |
| `crates/proto-gen/src/lib.rs` (hand Debug impls: JoinResponse, SendTarget/StreamAssignment, MhConnectRequest) | GSA, credential; reviewers: security | protocol |
| `crates/proto-gen/tests/signaling_roundtrip.rs`  | GSA; reviewers: test | protocol |
| `packages/proto-gen/scripts/verify-codegen.sh`  | GSA; reviewers: test | protocol |
| `crates/mc-service/src/webtransport/connection.rs` (media_servers population; mh_url key; RE-HOME of the mc_participant_mh_status_total emission to the boundary) | Domain-judgment (alert-input re-home); reviewers: observability | meeting-controller |
| `crates/mc-service/src/actors/participant.rs` (delete url-keyed  map + RecordMhStatuses + BoundedMhStatus; doc sweep) | Domain-judgment | meeting-controller |
| `crates/mc-service/src/actors/messages.rs` (delete url-keyed  map + RecordMhStatuses + BoundedMhStatus; doc sweep) | Domain-judgment | meeting-controller |
| `crates/mc-service/src/actors/meeting_media.rs` (delete url-keyed  map + RecordMhStatuses + BoundedMhStatus; doc sweep) | Domain-judgment | meeting-controller |
| `crates/mc-service/src/actors/join_park.rs` (delete url-keyed  map + RecordMhStatuses + BoundedMhStatus; doc sweep) | Domain-judgment | meeting-controller |
| `crates/mc-service/src/observability/metrics.rs` (drop the MH_STATUS_DROP_CAP reason + zero-init; doc text) | Domain-judgment (label-value removal); reviewers: observability | meeting-controller |
| `crates/mc-service/src/media_signaling/directive.rs`  | Minor-judgment (population-line removal; lookups + handler_url_unresolved fail-loud KEPT) | meeting-controller |
| `crates/mc-service/src/media_signaling/assignments.rs`  | Minor-judgment (population-line removal; lookups + handler_url_unresolved fail-loud KEPT) | meeting-controller |
| `crates/mc-service/src/media_signaling/outcome.rs`  | Minor-judgment (population-line removal; lookups + handler_url_unresolved fail-loud KEPT) | meeting-controller |
| `crates/mc-service/src/media_routing/placement.rs`  | Minor-judgment (population-line removal; lookups + handler_url_unresolved fail-loud KEPT) | meeting-controller |
| `crates/mc-service/src/media_routing/slots.rs`  | Minor-judgment (population-line removal; lookups + handler_url_unresolved fail-loud KEPT) | meeting-controller |
| `crates/mc-service/src/webtransport/handler.rs`  | Minor-judgment (population-line removal; lookups + handler_url_unresolved fail-loud KEPT) | meeting-controller |
| `crates/mc-service/tests/common/media_session.rs` (re-point url asserts to refs resolved through the set) | Minor-judgment (re-point url asserts to refs resolved via the set); reviewers: test | meeting-controller |
| `crates/mc-service/tests/handler_set_integration.rs` (re-point url asserts to refs resolved through the set) | Minor-judgment (re-point url asserts to refs resolved via the set); reviewers: test | meeting-controller |
| `crates/mc-service/tests/join_tests.rs` (re-point url asserts to refs resolved through the set) | Minor-judgment (re-point url asserts to refs resolved via the set); reviewers: test | meeting-controller |
| `crates/mc-service/tests/media_client_signaling_integration.rs` (re-point url asserts to refs resolved through the set) | Minor-judgment (re-point url asserts to refs resolved via the set); reviewers: test | meeting-controller |
| `crates/mc-service/tests/media_connection_update_integration.rs` (re-point url asserts to refs resolved through the set) | Minor-judgment (re-point url asserts to refs resolved via the set); reviewers: test | meeting-controller |
| `crates/mc-service/tests/slot_placement_integration.rs` (re-point url asserts to refs resolved through the set) | Minor-judgment (re-point url asserts to refs resolved via the set); reviewers: test | meeting-controller |
| `crates/mc-service/tests/video_slots_integration.rs` (re-point url asserts to refs resolved through the set) | Minor-judgment (re-point url asserts to refs resolved via the set); reviewers: test | meeting-controller |
| `packages/sdk-core/src/media/MediaTransport.ts` (join_token removed end-to-end: arm, #reconcile, #dial) | Domain-judgment, credential; reviewers: security + semantic-guard | client |
| `packages/sdk-core/src/session/MeetingSession.ts` (media.arm takes no token) | Domain-judgment, credential; reviewers: security + semantic-guard | client |
| `packages/sdk-core/src/validation/limits.ts` (anchor/doc comment only; names unchanged) | Minor-judgment | client |
| `packages/sdk-core/src/signaling/SignalingClient.ts` (comments / empty mh_url) | Minor-judgment | client |
| `packages/sdk-core/src/signaling/events.ts` (comments / empty mh_url) | Minor-judgment | client |
| `packages/sdk-core/src/media/lifecycle/AudioPipeline.ts` (comments / empty mh_url) | Minor-judgment | client |
| `packages/sdk-core/src/media/__tests__/helpers.ts` (test edits) | Minor-judgment | client |
| `packages/sdk-core/src/media/__tests__/media-transport.test.ts` (test edits) | Minor-judgment | client |
| `packages/sdk-core/src/media/__tests__/mhControlChannel.test.ts` (test edits) | Minor-judgment | client |
| `packages/sdk-core/src/media/__tests__/reconcile.test.ts` (test edits) | Minor-judgment | client |
| `packages/sdk-core/src/media/lifecycle/__tests__/audioPipeline.degradation.test.ts` (test edits) | Minor-judgment | client |
| `packages/sdk-core/src/media/lifecycle/__tests__/audioPipeline.deviceAndCounts.test.ts` (test edits) | Minor-judgment | client |
| `packages/sdk-core/src/media/lifecycle/__tests__/audioPipeline.intervalSignals.test.ts` (test edits) | Minor-judgment | client |
| `packages/sdk-core/src/media/lifecycle/__tests__/audioPipeline.test.ts` (test edits) | Minor-judgment | client |
| `packages/sdk-core/src/session/__tests__/meeting-session.test.ts` (test edits) | Minor-judgment | client |
| `packages/sdk-core/src/signaling/__tests__/handlerSetIntake.test.ts` (test edits) | Minor-judgment | client |
| `packages/sdk-core/src/signaling/__tests__/helpers.ts` (test edits) | Minor-judgment | client |
| `packages/sdk-core/src/signaling/__tests__/signaling-client.test.ts` (test edits) | Minor-judgment | client |
| `packages/sdk-core/src/validation/__tests__/limits.test.ts` (test edits) | Minor-judgment | client |
| `packages/web-app/src/globals.d.ts` (comment sweep) | Minor-judgment | client |
| `packages/web-app/src/lib/e2eBus.ts` (comment sweep) | Minor-judgment | client |
| `packages/web-app/e2e/README.md` (comment sweep) | Minor-judgment | client |
| `packages/web-app/e2e/fixtures.ts` (comment sweep) | Minor-judgment | client |
| `packages/web-app/e2e/partial-connectivity.spec.ts` (comment sweep) | Minor-judgment | client |
| `crates/env-tests/src/fixtures/mc_session.rs` (slot/target url reads -> ref resolved through the set; MH connect join_token gone) | Domain-judgment, credential (MH connect); reviewers: security | test |
| `crates/env-tests/src/fixtures/participant.rs` (slot/target url reads -> ref resolved through the set; MH connect join_token gone) | Domain-judgment, credential (MH connect); reviewers: security | test |
| `crates/env-tests/tests/26_mh_quic.rs` (slot/target url reads -> ref resolved through the set; MH connect join_token gone) | Domain-judgment, credential (MH connect); reviewers: security | test |
| `crates/env-tests/tests/27_mc_slot_placement.rs` (slot/target url reads -> ref resolved through the set; MH connect join_token gone) | Domain-judgment, credential (MH connect); reviewers: security | test |
| `crates/env-tests/tests/34_mc_kek_rotation.rs` (slot/target url reads -> ref resolved through the set; MH connect join_token gone) | Domain-judgment, credential (MH connect); reviewers: security | test |
| `crates/env-tests/tests/35_mc_server_mute_teardown.rs` (slot/target url reads -> ref resolved through the set; MH connect join_token gone) | Domain-judgment, credential (MH connect); reviewers: security | test |
| `crates/env-tests/tests/36_mc_media_kind_labels.rs` (slot/target url reads -> ref resolved through the set; MH connect join_token gone) | Domain-judgment, credential (MH connect); reviewers: security | test |
| `crates/env-tests/tests/40_resilience.rs` (slot/target url reads -> ref resolved through the set; MH connect join_token gone) | Domain-judgment, credential (MH connect); reviewers: security | test |
| `crates/env-tests/tests/46_mh_media_dial_admission.rs` (slot/target url reads -> ref resolved through the set; MH connect join_token gone) | Domain-judgment, credential (MH connect); reviewers: security | test |
| `crates/mh-service/src/webtransport/connection.rs` (comment only: join_token history) | Minor-judgment | media-handler |
| `crates/mh-service/tests/common/wt_client.rs` (MhConnectRequest.join_token gone; old-client arms send a raw vacated tag 1) | Domain-judgment, credential; reviewers: security | media-handler |
| `crates/mh-service/tests/media_dial_admission_integration.rs` (MhConnectRequest.join_token gone; old-client arms send a raw vacated tag 1) | Domain-judgment, credential; reviewers: security | media-handler |
| `crates/mh-service/tests/media_keyframe_integration.rs` (MhConnectRequest.join_token gone; old-client arms send a raw vacated tag 1) | Domain-judgment, credential; reviewers: security | media-handler |
| `crates/mc-service/src/media_dial/status.rs` (doc sweep; ClientHandler re-export dropped; local rename) | Minor-judgment | meeting-controller |
| `crates/mc-service/src/media_routing/connectivity.rs` (doc sweep; ClientHandler re-export dropped; local rename) | Minor-judgment | meeting-controller |
| `crates/mc-service/src/media_routing/mod.rs` (doc sweep; ClientHandler re-export dropped; local rename) | Minor-judgment | meeting-controller |
| `crates/mc-service/tests/otel_grpc_outbound_integration.rs` (doc sweep; ClientHandler re-export dropped; local rename) | Minor-judgment | meeting-controller |
| `docs/runbooks/mh-deployment.md` (P3 rollback coupling; MH-1 floor) | Minor-judgment | operations |
| `docs/runbooks/mc-deployment.md` (P3 rollback coupling; MH-1 floor) | Minor-judgment | operations |
| `scripts/lang/rust/lint.sh` (add --all-features so feature-gated env-tests targets are linted) | Not mine, Minor-judgment | infrastructure |
| `.githooks/pre-commit` (clippy argv aligned with lint.sh: add --workspace; keep-in-sync note) | Not mine, Minor-judgment | infrastructure |
| `scripts/guards/semantic/checks.md` (drop dual-population window text; rule kept) | Domain-judgment (guard policy content) | semantic-guard |
| `infra/grafana/dashboards/mc-overview.json` (drop-reason panel, only if it enumerates ) | Minor-judgment | observability |
| `docs/API_CONTRACTS.md`  | in-domain (protocol docs) / TODO ticks | — |
| `docs/WEBTRANSPORT_FLOW.md`  | in-domain (protocol docs) / TODO ticks | — |
| `docs/ARCHITECTURE.md`  | in-domain (protocol docs) / TODO ticks | — |
| `docs/TODO.md`  | in-domain (protocol docs) / TODO ticks | — |
| `docs/observability/metrics/mc-service.md`  | Domain-judgment (catalog) | observability |
| `docs/runbooks/mc-incident-response.md`  | Minor-judgment | operations |
| `docs/runbooks/client-dev-local.md`  | Minor-judgment | operations |
| `docs/specialist-knowledge/protocol/INDEX.md`  | in-domain / pointer fix | — |
| `docs/specialist-knowledge/security/INDEX.md`  | in-domain / pointer fix | — |

---

## Planning

### Wire (signaling.proto)
Delete + `reserved <tag>; reserved "<name>";` (CONVENTIONS Rule 5):
- `JoinResponse.media_servers` (4); `SendTarget.media_handler_url` (1); `StreamAssignment.media_handler_url` (9) — and add the PRE-ADR-0036 name `media_handler_url` to StreamAssignment's name reservation (old tag 5 + new tag 9 both vacated), deleting the "deliberately ABSENT" exception text and the ServerMessage cross-ref ("one of exactly two" -> the only one); `MhConnectionStatus.mh_url` (1); `MhConnectRequest.join_token` (1).
- The 256-byte client-string truncation rule moves onto `MhConnectionStatus.failure_reason` (its new single home); `MediaKindState.server_muted_by` and `UnmuteRequest.participant_id` repoint to it.
- Sweep every comment naming a deleted field (MediaServerInfo "same spelling" note, StreamAssignment per-slot text -> stated on `handler_ref`, MediaConnectionUpdate, internal.proto L1481 -> `StreamAssignment.handler_ref`). `JoinRequest.join_token` untouched.

### Codegen / tests
- build.rs: KEEP both skip_debug entries; MhConnectRequest note names `media_dial_token` as the one credential. proto-gen lib.rs Debug impls drop deleted fields.
- verify-codegen.sh: `assert_message_field_absent` for the 5 deleted fields; PRESENCE of `JoinRequest.joinToken` + positive control (`MhConnectRequest.mediaDialToken` present) so the absence/presence helpers cannot pass vacuously.
- signaling_roundtrip.rs: drop deleted-field roundtrips; add a test that hand-encodes each vacated tag and asserts it decodes as unknown (no field populated, message otherwise intact).
- Regenerate TS via codegen.sh.

### MC (meeting-controller review) — one non-mechanical item, see below
- Delete media_servers population (connection.rs), SendTarget/StreamAssignment url population (directive.rs, assignments.rs); KEEP `client_handler()` lookup + `handler_url_unresolved` fail-loud (label value not renamed). `ClientHandler.url` deleted if unread; `ClientHandlerEntry.media_handler_url` (feeds the set) stays.
- **Behaviour found (STOP condition; Lead ruled PROCEED IN-LOOP, MC/observability/code-reviewer/dry-reviewer concur):** the `mh_url`-keyed participant map is write-only, but its handler is the sole emitter of `mc_participant_mh_status_total{state}` (drives `MCMediaConnectionAllFailed` paging alert, panel, deployment gates) and `mc_participant_mh_status_dropped_total{reason="cap"}`. Proposed (MC's suggestion): emit `record_participant_mh_status(state)` once per status at the boundary inside the existing `take(MAX_MH_STATUSES_PER_UPDATE)` (same count-every-status semantics incl. `unspecified`, before the rate limiter); delete the map, `RecordMhStatuses`, `BoundedMhStatus`, `MAX_MH_STATUSES_PER_PARTICIPANT` and their unit tests (moving the per-state counter coverage to the boundary/integration test); remove `reason="cap"` (existed only for url keys) from code, zero-init, catalog, panel, runbook (obligation c). ONE emission site, at the `MediaConnectionUpdate` boundary using the total `map_connection_state` — independent of the ref!=0 filter and the rate-limited forward (so `unspecified` keeps counting; identical to production today, where the SDK sends mh_url="" and every entry lands). Name, label domain, alert expr unchanged; no ref/participant/version labels. NOT re-keyed on handler_ref (would duplicate the meeting actor's ref store). Consequence stated: MC no longer reads `failure_reason`/`failure_code` at all, so their truncation goes too (no dead truncate; `MAX_CLIENT_STRING_BYTES` kept only if it has another caller, else deleted). Counting tests (participant.rs unit tests, media_connection_update_integration.rs) re-pointed to the boundary, not deleted. Alternative if Lead prefers: HOLD the mh_url deletion.
- Unit/integration tests asserting url emptiness re-pointed to `handler_ref` (0 vs member of the set).

### SDK (paired-client review)
- MediaTransport: remove the JWT end-to-end — `arm()`, `#reconcile()`, `#dial(rec)` take no token; no placeholder; JSDoc says media_dial token is the only dial credential. MeetingSession calls `media.arm()`. `JoinRequest` joinToken path in SignalingClient STAYS.
- MediaConnectionError `mhUrl` KEPT (paired-client ruling: SDK error field filled from the SET url, not the deleted wire field); nothing new enters message/cause/toJSON.
- limits.ts: `MAX_MH_URL_BYTES` and `capMhUrl` KEPT (names unchanged); ANCHOR rewritten to point ONLY at the proto `MhConnectionStatus.failure_reason` 256-byte rule text (no claim that MC truncates); the set-url bound in handlerSetIntake is SDK-own (no proto/MC bound on `MediaServerInfo.media_handler_url`), stated as such.
- Test helpers: drop `legacyMediaServers`/`legacyTargetUrls`/`legacyUrl` and the "legacy disagrees with set" / "mh_url left empty" tests; `arm('jwt')` -> `arm()`; connectRequest assertions lose joinToken.

### Env-tests
- `slot_url`/`targets_of` and test reads of `StreamAssignment/SendTarget.media_handler_url` resolve `handler_ref` through the participant's current `HandlerSets` (existing ref->url map), failing loudly on a non-member ref.
- MH connect: `mh_connect_frame_with` deleted; 46 arm (ii) becomes "old client still sends tag 1 (raw-encoded unknown field carrying an invalid AC token): MH admits on dial token" — keeps S3 coverage. `join_request*` builders (JoinRequest) STAY.

### Gate-1 wording carry-ins
- Truncation rule (security a / MC / paired-client): proto `failure_reason` states untrusted client input any reader must bound (256 B); MC drops `failure_reason`/`failure_code` unread (stated in connection.rs ~L957/~L1113 docs); limits.ts anchor names only the proto rule text (MC `MAX_CLIENT_STRING_BYTES` survives for UnmuteRequest.participant_id but is NOT cited).
- Fake values (security b): env-test 46 tag-1 payload is a fixed non-JWT-shaped sentinel; SDK absence test uses a distinctive sentinel JWT with mediaDialToken must-hit control.
- checks.md (semantic-guard): also sweep other window-tense text (MH-1 decode note, story-4 emission bullets); keep "not retained" + "JWT held for redial is a Gate 1 finding".
- Ops (observability): P3 hazard detection worded as fleet-level connected-rate dip; per-client via client-dev-local triage. mc-incident-response:1580 rewritten (MC no longer truncates/stores those strings).
- Ops: mc-incident-response 1580 rewritten to "MC records only the per-state count; reason strings are client-side"; 1582 drops cap/MAX_MH_STATUSES_PER_PARTICIPANT, over_limit remains the abuse discriminator; sweep mc-deployment.md checklists for dropped_total `cap` expectations; P3 rollback entry notes the MCMediaConnectionAllFailed input keeps name/labels/expr across a mixed fleet and on revert.
- Red run: record each of the 5 failure lines verbatim.

### Test discipline (test)
- verify-codegen.sh (story-4 block, stale "P3 adds absence checks" comment rewritten): 5 × `assert_message_field_absent` each with a same-message positive control (JoinResponse mediaHandlerSet; SendTarget/StreamAssignment/MhConnectionStatus handlerRef; MhConnectRequest mediaDialToken); presence `JoinRequest joinToken` and `MediaServerInfo mediaHandlerUrl`. Field list lives only here (test-gates.md not rewritten; main.md maps gate items to these lines).
- signaling_roundtrip.rs: one shared helper (LEN-typed, non-empty payload) -> decode ok, `== T::default()`, re-encode empty; positive control on a live LEN tag of the same message. Written FIRST against the current proto and run red (expected red for all 5; controls green), red run recorded in main.md, then fields deleted -> green. `debug_never_prints_the_mh_connect_tokens` kept (asserts media_dial_token redacted); tag-1 keep-test switched to media_dial_token tag 2.
- Coverage table in main.md: each deleted url/token assertion -> surviving ref-based assertion (added where none exists). SDK: JWT-only-on-join-dial asserts replaced by "JWT string never appears in any framed connect request" with mediaDialToken positive control.

### Ops docs (operations)
- mh-deployment.md §Rollback: rewrite MH-1 "degrades until P3" -> MH rollback floor is MH-1 unconditionally (below it = full admission outage); add P3 coupling entry (deploy order after MC-A/C1/MH-1/T-1; revert unit = this one commit, wire-safe both directions; hazard: pre-C1 / stale cached SDK bundle dials nothing silently (empty fan-out, detect via client-dev-local triage / `mc_participant_mh_status_total` absence per join), remedy reload; floors MH>=MH-1, SDK>=C1). Mirror in mc-deployment.md §Coordination if MC-A is described there.
- mc-incident-response.md: invert "not stranded while SDK still sends join_token" (≈1711, 1804); fix ≈3485 C1/media_servers remedy; untrusted-input + label text -> failure_reason/failure_code, client identity = handler_ref + handler_set_version; drop `cap` mentions. client-dev-local.md empty-fan-out triage repointed at `MediaHandlerSet.handlers` (logic unchanged) + `record_mh_statuses` mention.
- mc-overview.json drop-reason panel description: remove cap sentence and "legacy url-keyed recording still happens". Catalog rows updated (cardinality 3->2, Recorded-in lines).
- Final residual grep pasted in main.md.

### Docs / guards
- API_CONTRACTS, WEBTRANSPORT_FLOW, ARCHITECTURE, runbooks (client-dev-local, mc-incident-response), observability catalog, TODO (P3 items ticked), checks.md window text, specialist INDEX pointers.
- NOT edited: historical records (devloop-outputs, debates, user-story bodies, accepted ADR text) — they describe history, not current contract.
- Security S3: MH `webtransport/connection.rs` comment-only (join_token as history); verify nothing there Debug/logs raw envelope or unknown-field bytes (prost unknown fields are discarded, not retained). S4: checks.md:164 window sentence dropped, rule "JWT not retained by SDK" kept (semantic-guard review); MH connection.rs:7 doc fixed.
- dt-guard ts_retained_credentials (S5): confirm its `joinToken` must-hit fixture is independent of the removed call site; add a fixture only if not.

---

## Gate 1 Confirmations

Classification-sanity guard: STATUS=OK (after Owner-cell normalization).

| Reviewer | Plan Status |
|----------|-------------|
| Paired Client | confirmed |
| Meeting Controller | confirmed |
| Security | confirmed |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Semantic Guard | confirmed |

---

## Pre-Work

None

---

## Implementation Summary

### Red-before-green: vacated tags decode as unknown (`crates/proto-gen/tests/signaling_roundtrip.rs`)
Written first and run against the PRE-deletion proto. All five failed on the
`assert_eq!(decoded, T::default())` line (signaling_roundtrip.rs:1783 at the time;
decode succeeded, so the helper and byte builder were sound), each showing the
surviving field claiming the bytes:

| Test | Failure line (left = decoded) |
|------|------------------------------|
| vacated_join_response_tag_4_is_unknown | JoinResponse { … media_servers: [MediaServerInfo { media_handler_url: "x", … }] … } |
| vacated_send_target_tag_1_is_unknown | SendTarget { media_handler_url: "\n\u{1}x", … } |
| vacated_stream_assignment_tag_9_is_unknown | StreamAssignment { … media_handler_url: "\n\u{1}x", … } |
| vacated_mh_connection_status_tag_1_is_unknown | MhConnectionStatus { mh_url: "\n\u{1}x", … } |
| vacated_mh_connect_request_tag_1_is_unknown | MhConnectRequest { join_token: <redacted 3 B>, … } |

After the deletion all five pass, including the positive control on a live tag
of the same message (JoinResponse 16, SendTarget 3, StreamAssignment 12,
MhConnectionStatus 6, MhConnectRequest 2).

### test-gates.md §Protocol → verify-codegen.sh (the one list; P3 block)
| Gate item | Check |
|-----------|-------|
| JoinResponse.media_servers absent | assert_message_field_absent "${SIG}" JoinResponse mediaServers mediaHandlerSet |
| SendTarget.media_handler_url absent | assert_message_field_absent "${SIG}" SendTarget mediaHandlerUrl handlerRef |
| StreamAssignment.media_handler_url absent | assert_message_field_absent "${SIG}" StreamAssignment mediaHandlerUrl handlerRef |
| MhConnectionStatus.mh_url absent | assert_message_field_absent "${SIG}" MhConnectionStatus mhUrl handlerRef |
| MhConnectRequest.join_token absent | assert_message_field_absent "${SIG}" MhConnectRequest joinToken mediaDialToken |
| CAUTION: `JoinRequest.join_token` stays | `assert_message_field "${SIG}" JoinRequest joinToken` |
| addresses stay in the set (R-17) | `assert_message_field "${SIG}" MediaServerInfo mediaHandlerUrl` |
(`process_start_epoch_ms`, `connection_id` legacy-empty: other tasks.)

### Coverage preserved (deleted assertion → surviving ref-based assertion)
| Deleted | Surviving |
|---------|-----------|
| MC unit: slot url empty iff edgeless (`slots.rs` world test) | same test: `handler_ref == 0` iff edgeless, and non-zero ref is a member of `client_snapshot()` (added) |
| MC unit: assignment/directive names `mh-0`'s url (`assignments.rs`, `directive.rs`) | `handler_ref == ref_of(set, "mh-0")`, `!= ref_of("mh-1")` (added helpers) |
| MC unit: `response.media_servers == set.handlers` | deleted (property gone); set-only asserts kept |
| MC unit: participant-actor per-state counts + cap | `connection.rs` `every_reported_status_is_counted_by_state_at_the_boundary`, `an_oversized_update_counts_only_its_bounded_prefix`, rate-limited update still counted (added) |
| MC integration: slot/target url == seeded/registered url | `Session::url_of(handler_ref)` resolves through the sets the session was told (fails loud on 0/non-member) |
| MC integration: cap drop (17 urls → 16 + cap 1) | `many_refs_all_counted` (17 → 17, no drop); over-limit 70 → 64 counted + over_limit 1 |
| MC integration: attacker url never in target/assignment | forged `handler_ref` never in target/assignment (existing member asserts) |
| SDK: superseded url fields never read (poison) | fields gone; invalid-ref drop/undefined already covered in `handlerSetIntake.test.ts` (StreamAssignment/SendTarget non-member tests) |
| SDK: JWT only on armed dial | `reconcile.test.ts` every dial carries exactly its ref's token; `meeting-session.test.ts` JWT never in any MH bytes (positive controls: JWT IS in MC JoinRequest bytes; each dial token IS in its MH bytes) |
| Env: slot/target url reads | `Participant::url_of` / `slot_url` / `targets()` resolve refs through `HandlerSets` (fail loud on 0/non-member) |
| Env 46 / MH integ: join_token ignored | old client's raw vacated tag 1 (fixed non-JWT sentinel in env 46) admitted on the dial token; valid AC token on tag 1 + empty dial still DECLINED |

### Residual grep (proto/ crates/ packages/ scripts/ infra/ living docs) after the change
Only intended hits: the `reserved` lines and their "Was …" comments in
`signaling.proto`; the verify-codegen absence checks; API_CONTRACTS reserved
lines; the runbook P3 coupling/hazard text naming the retired fields;
`MediaConnectionError.mhUrl` (SDK error field carrying a SET url — kept per
paired-client). Historical records (devloop-outputs, debates, user stories,
ADRs) not rewritten.

---

## Files Modified

```
81 files changed, 1731 insertions(+), 1411 deletions(-)
```

Per-file classification: see §Cross-Boundary Classification.

---

## Devloop Verification Steps

Gate 2 attempt 1 (TOTAL_RESULT=OK) was followed by a pre-commit clippy refusal (Issue 2); after the fix, Gate 2 attempt 2 (`DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh`, Lead, final tree) — TOTAL_DURATION=2029 TOTAL_RESULT=OK, all 7 layers OK (L7 1750s). Attempt 1 table:

| Layer | Result | Duration (s) |
|-------|--------|--------------|
| 1 Compile | OK | 7 |
| 2 Format | OK | 6 |
| 3 Guards | OK | 126 |
| 4 Test | OK | 138 |
| 5 Lint | OK | 1 |
| 6 Audit | OK | 5 |
| 7 Env-tests (Rust env-tests + browser E2E) | OK | 1819 |

`TOTAL_DURATION=2102 TOTAL_RESULT=OK`. Fast check (Lead, pre-review): TOTAL_RESULT=OK.

---

## Code Review Results

### Gate 3 Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Paired Client | CLEAR | 0 | 0 | 0 | |
| Meeting Controller | RESOLVED-FIXED | 3 | 3 | 0 | incl. 1 nit |
| Security | CLEAR | 0 | 0 | 0 | |
| Test | CLEAR | 0 | 0 | 0 | |
| Observability | RESOLVED-FIXED | 5 | 5 | 0 | |
| Code Quality | RESOLVED-FIXED | 9 | 9 | 0 | |
| DRY | RESOLVED-FIXED | 4 | 4 | 0 | |
| Operations | RESOLVED-FIXED | 1 | 1 | 0 | |
| Semantic Guard | CLEAR | 0 | 0 | 0 | |
| Infrastructure (late, lint.sh/pre-commit only) | RESOLVED-DEFERRED | 3 | 2 | 1 | F3 clippy argv parity guard spun out |


### DRY Review

**Summary**: The deletion is clean on the cross-encoding axis. Wire, generated TS, MC, SDK, env-tests and docs carry no residual read of a deleted field. The superseded url-keyed MC map was deleted rather than re-keyed, which would have created a second ref-keyed client-status store. The SDK 256-byte anchor now names one home, the proto `MhConnectionStatus.failure_reason` rule text. The absence list lives only in `verify-codegen.sh`.

**Gate-1 inputs (adopted)**: kept `MAX_MH_URL_BYTES`/`capMhUrl` (still load-bearing for set-url intake and the failure string cap) and re-pointed the anchor off the deleted `mh_url` comment; deleted `ParticipantActor::mh_statuses` instead of re-keying it, with ONE emit site for `mc_participant_mh_status_total` and `reason="cap"` retired together; one field list in verify-codegen.sh; the anchor does not cite MC `MAX_CLIENT_STRING_BYTES` once MC drops the strings unread.

**Findings**
- F1: `crates/proto-gen/tests/signaling_roundtrip.rs` `encode_field` hand-rolled a varint/key encoder, which re-implements `prost::encoding`. **Fixed** (uses `encode_key`/`encode_varint`).
- F2: the vacated-tag-1 MhConnectRequest builder was added twice (`crates/env-tests/src/fixtures/mc_session.rs:mh_connect_frame_with_vacated_tag_1`, `crates/mh-service/tests/common/wt_client.rs:write_mh_connect_with_vacated_tag_1`). ADR-0028 layering forbids a shared home, so this is a deliberate pair; each definition must name its twin. **Fixed**; the pair is recorded in the DRY INDEX.
- F3: runbook SSoT drift. The `mh-deployment.md` "P3 coupling" authority omitted the MC>=MC-A floor that its `mc-deployment.md` mirror stated. **Fixed**: the authority holds THE floor list, and mc-deployment points to it.
- F4: `crates/env-tests/tests/26_mh_quic.rs` doc cited the deleted `ParticipantActor::record_mh_statuses` and `participant.rs::tests` cap coverage. **Fixed** (re-pointed to the boundary emit site and its test).

**Not findings**: the env-tests `HandlerSets::url_of` and the mc-service test `MediaSession::url_of` sit on opposite sides of the ADR-0028 layering and have different semantics. `kind_targets` in env-tests 27/35 is pre-existing, only retyped.

**Verdict**: RESOLVED-FIXED


### Observability Review

**Summary**: Obligation (c)/(e) met. `mc_participant_mh_status_total{state}` (the `MCMediaConnectionAllFailed` input) is re-homed, not removed: ONE emit site in `webtransport/connection.rs::handle_media_connection_update`, per status over the bounded `take(MAX_MH_STATUSES_PER_UPDATE)` prefix via the total `map_connection_state` (`unspecified` kept), independent of the ref filter and before the rate-limited forward — identical to pre-P3 production counts (SDK already sent `mh_url=""`). Name, label domain, zero-init and alert expr unchanged; no series added; no `handler_ref`/`handler_set_version`/participant labels. `reason="cap"` on `mc_participant_mh_status_dropped_total` (existed only for the url-keyed map) retired as one unit: const, emission, `MH_STATUS_DROP_REASONS` zero-init, tests, catalog, panel, runbooks. No SDK metric carried a deleted field.

**ADR Compliance**: ADR-0011 (bounded labels, no PII/ids), ADR-0031 (catalog/panel/alert move together), ADR-0032 (emission covered by `every_reported_status_is_counted_by_state_at_the_boundary` + rate_limited co-assert + integration test) — compliant.

**Findings** (all Fixed):
- F1 catalog cardinality table + total line still listed `cap` — `docs/observability/metrics/mc-service.md` — Fixed.
- F2 env-test on-call text pointed at deleted `ParticipantActor::record_mh_statuses` / participant cap tests — `crates/env-tests/tests/26_mh_quic.rs` — Fixed.
- F3 false claim "rate_limited never together with over_limit" (both can fire for one oversized, over-rate update) — `metrics.rs` doc + `mc-overview.json` panel — Fixed (wording only, behaviour unchanged).
- F4 stale map-cap comments + "before the ref filter" mis-ordering — `connection.rs` — Fixed.
- F5 stale "url-keyed maps" section header — `crates/mc-service/tests/media_client_signaling_integration.rs` — Fixed.

**Verdict**: RESOLVED-FIXED

---

## Accepted Deferrals

- `docs/TODO.md` §Guard Coverage Gap — clippy-lane argv parity (pre-commit vs lint.sh)

---

## Rollback Procedure

1. Start commit: `20aef3fe3a8e2b384811b03158eba8fd5a9b5d4a`
2. `git diff 20aef3fe..HEAD`
3. `git reset --soft 20aef3fe` / `git reset --hard 20aef3fe`
4. **Safe-revert unit**: the whole commit. Wire deletion, generated TS, MC/SDK/MH/env-test consumers, the re-homed `mc_participant_mh_status_total` emission and the `reason="cap"` retirement (code + catalog + panel + runbooks) must move together; a partial revert would either reintroduce a field without its consumers or leave the paging-alert input unfed. Deploy floors after this commit: MC >= MC-A, SDK >= C1, MH >= MH-1 (see `docs/runbooks/mh-deployment.md` §Rollback "P3 coupling").

---

## Issues Encountered & Resolutions

### Issue 1: MhConnectionStatus.mh_url still fed a paging metric (task STOP condition)
**Problem**: The url-keyed participant map was write-only, but its handler was the sole emitter of `mc_participant_mh_status_total{state}` (MCMediaConnectionAllFailed alert input) and `mc_participant_mh_status_dropped_total{reason="cap"}`.
**Resolution (Lead, Gate 1)**: Residual behaviour is metric emission only (no routing/connectivity), so completing the consumer move in-loop is the most complete option within one session and crosses no hard safety line. Re-home the per-state counter to the connection.rs boundary before the rate limiter/ref filter (identical counting semantics, same name/label/zero-init, no new series); delete the map; retire `reason="cap"` everywhere. Concurred by meeting-controller, observability, code-reviewer.

- Lead deviation: INDEX.md + review-protocol injected by reference (teammates read the files as a mandatory first action) rather than pasted inline, to keep the 10-teammate spawn payload tractable.

---


### Issue 2: pre-commit clippy refused the commit after Gate 2 (Layer 5 gate gap)
**Problem**: Deleting `media_handler_url` let prost derive `Copy` on `StreamAssignment`, so two env-test `.clone()` calls (34_mc_kek_rotation.rs:407, 35_mc_server_mute_teardown.rs:1624) tripped `clippy::clone_on_copy`. Layer 5 (`scripts/lang/rust/lint.sh:7`) ran clippy without `--all-features`, so the feature-gated env-tests targets were never linted; only `.githooks/pre-commit` (`--all-features`) caught it. My own env-tests clippy spot-check was also vacuous (grep anchored on `^error` over ANSI-coloured output).
**Resolution**: both sites deref instead of clone (no `#[allow]`); `lint.sh` now passes `--all-features` (matching the hook), with a comment. Spot-checks re-run with `--color never` and exit codes. Adverse proof: a planted `clone_on_copy` at 34_mc_kek_rotation.rs:407 makes `scripts/lang/rust/lint.sh` exit 1 (`STATUS=FAIL REASON=cargo-clippy-failed`); reverted, it exits 0. Infra review: TODO "NO env-test is type-checked below Layer 7" marked resolved; the release-profile constraint documented in lint.sh; hook argv aligned (`--workspace`) with a keep-in-sync note; the parity GUARD is a spin-out (docs/TODO.md "clippy-lane argv parity") because delegating the hook to lint.sh would break the hook's bash-3.2 tolerance.
## Lessons Learned

1. A "dead" write-only map can still be the sole emitter of a paging-alert input; retiring a wire field needs a metric-path walk, not just a reader search.
2. Owner cells in the classification table must be a single manifest token; reviewer annotations belong in the Classification column.

## Operations Review

### Summary
Runbook, alert, dashboard, catalog and infra references to the five deleted fields are removed or repointed in the same commit (obligation b). The rollback text that depended on "until P3" is inverted: the MH rollback floor is MH-1 unconditionally. P3 has its own coupling entry stating the deploy order, the single commit as the wire-safe revert unit, the stale-bundle hazard and the raised floors. The `reason="cap"` retirement and the alert-input re-home are documented as continuous across a mixed fleet and on revert.

### ADR Compliance
ADR-0039 §4/§5 (deploy order and rollback floors documented); ADR-0031 (service-owned dashboard/runbook rows updated with the series change).

### Findings

- **Finding**: P3 Hazard bullet named a misleading fleet signal ("dip in the connected rate of `mc_participant_mh_status_total`"). A stale pre-C1 client reports nothing, so no state moves, the failed share is flat, and a raw rate only tracks traffic. - `docs/runbooks/mh-deployment.md:1489`
  - **Fix**: Name the normalized signal (connected statuses per successful join) and state explicitly that `MCMediaConnectionAllFailed` will not page for this.
  - **Status**: Fixed

### Verdict
**RESOLVED-FIXED**

### Accepted Deferrals
None.
