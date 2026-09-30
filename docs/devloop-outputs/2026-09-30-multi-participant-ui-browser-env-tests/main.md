# Devloop Output: Multi-participant UI, host server mute, browser env-tests (story 2 task 15)

**Date**: 2026-09-30
**Task**: Multi-participant in-meeting UI with unreachable rendering, host server mute, browser env-tests S1/S2/S3/S6/S10a, R-27 read-back, telemetry credential-scan positive control, loopback test removed (story `docs/user-stories/2026-09-21-hear-each-other.md` task 15). Full task text: run-story `task-15.prompt`.
**Specialist**: client
**Mode**: Agent Teams (v2) — full, Gate-1 present
**Branch**: `feature/hear-each-other`
**Duration**: ~3h

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `4a2f5aa2d7e9f6d15ebd1ed5b3335687d9fa4a95` |
| Branch | `feature/hear-each-other` |
| Lead Model | `claude-opus-5-5[1m]` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` |
| Implementing Specialist | `client` |
| Tier | `full` |
| Iteration | `1` |
| Security | `security` |
| Test | `test` (paired: scenario ownership, fixture discipline) |
| Observability | `observability` (paired: read-back name list) |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Semantic Guard | `semantic-guard` (spawned: telemetry bearer/credential-scan + dev-only build lever surfaces) |

---

## Task Overview

### Objective
See task description (run-story task-15.prompt). Render N slot rows with wire slot state, mute/server-mute indicators, unreachable rendering; host server-mute affordance; browser env-tests S1/S2/S3/S6/S10a; R-27 read-back; telemetry credential-scan positive control; delete story-1 loopback browser test; restore structural client-mute coverage.

### Scope
- **Service(s)**: client (`packages/web-app`, `packages/sdk-svelte`, `packages/sdk-core`)
- **Schema**: No
- **Cross-cutting**: Yes (test + observability paired)

### Debate Decision
NOT NEEDED - designed in story 2 / ADR-0036.

---

## Cross-Boundary Classification

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `crates/dt-guard/src/ts_retained_credentials.rs` | Not mine, Minor-judgment | security |
| `docs/runbooks/client-dev-local.md` | Not mine, Minor-judgment | operations |
| `packages/sdk-core/src/encoding/__tests__/base64url.test.ts` | Mine | — |
| `packages/sdk-core/src/encoding/base64url.ts` | Mine | — |
| `packages/sdk-core/src/globals.d.ts` | Mine | — |
| `packages/sdk-core/src/index.ts` | Mine | — |
| `packages/sdk-core/src/media/frame/ed25519.ts` | Mine | — |
| `packages/sdk-core/src/media/setup/__tests__/testToneCapture.test.ts` | Mine | — |
| `packages/sdk-core/src/media/setup/capture.ts` | Mine | — |
| `packages/sdk-core/src/session/MeetingSession.ts` | Mine | — |
| `packages/sdk-core/src/session/__tests__/meeting-session.test.ts` | Mine | — |
| `packages/sdk-core/src/session/__tests__/meetingRole.test.ts` | Mine | — |
| `packages/sdk-core/src/session/events.ts` | Mine | — |
| `packages/sdk-core/src/session/mediaSelection.ts` | Mine | — |
| `packages/sdk-core/src/session/meetingRole.ts` | Mine | — |
| `packages/sdk-core/src/signaling/SignalingClient.ts` | Mine | — |
| `packages/sdk-core/src/signaling/__tests__/helpers.ts` | Mine | — |
| `packages/sdk-core/src/signaling/__tests__/maps.test.ts` | Mine | — |
| `packages/sdk-core/src/signaling/__tests__/mediaSignaling.test.ts` | Mine | — |
| `packages/sdk-core/src/signaling/__tests__/signaling-client.test.ts` | Mine | — |
| `packages/sdk-core/src/signaling/errorCodeMap.ts` | Mine | — |
| `packages/sdk-core/src/signaling/events.ts` | Mine | — |
| `packages/sdk-core/vite.config.ts` | Mine | — |
| `packages/sdk-svelte/src/__tests__/MediaStore.test.ts` | Mine | — |
| `packages/sdk-svelte/src/__tests__/subscribeSession.test.ts` | Mine | — |
| `packages/sdk-svelte/src/index.ts` | Mine | — |
| `packages/sdk-svelte/src/stores/MediaStore.svelte.ts` | Mine | — |
| `packages/sdk-svelte/src/stores/bindMeetingSession.ts` | Mine | — |
| `packages/sdk-svelte/src/stores/snapshots.ts` | Mine | — |
| `packages/web-app/e2e/README.md` | Mine | — |
| `packages/web-app/e2e/clientMetricNames.ts` | Mine | — |
| `packages/web-app/e2e/cohort.ts` | Mine | — |
| `packages/web-app/e2e/cohortContexts.ts` | Mine | — |
| `packages/web-app/e2e/configEnv.ts` | Mine | — |
| `packages/web-app/e2e/credentialScan.ts` | Mine | — |
| `packages/web-app/e2e/fixtures.ts` | Mine | — |
| `packages/web-app/e2e/global-setup.ts` | Mine | — |
| `packages/web-app/e2e/instanceCounters.ts` | Mine | — |
| `packages/web-app/e2e/join-happy-path.spec.ts` | Mine | — |
| `packages/web-app/e2e/kek-rotation.spec.ts` | Mine | — |
| `packages/web-app/e2e/mcMetrics.ts` | Mine | — |
| `packages/web-app/e2e/multi-party-hear.spec.ts` | Mine | — |
| `packages/web-app/e2e/over-subscription.spec.ts` | Mine | — |
| `packages/web-app/e2e/partial-connectivity.spec.ts` | Mine | — |
| `packages/web-app/e2e/receiveEvidence.ts` | Mine | — |
| `packages/web-app/e2e/server-mute.spec.ts` | Mine | — |
| `packages/web-app/e2e/solo-participant.spec.ts` | Mine | — |
| `packages/web-app/playwright.config.ts` | Mine | — |
| `packages/web-app/src/__tests__/e2eBus.test.ts` | Mine | — |
| `packages/web-app/src/__tests__/helpers/MockMeetingSession.ts` | Mine | — |
| `packages/web-app/src/__tests__/inMeeting.test.ts` | Mine | — |
| `packages/web-app/src/__tests__/joinMeeting.test.ts` | Mine | — |
| `packages/web-app/src/__tests__/participantState.test.ts` | Mine | — |
| `packages/web-app/src/__tests__/testLevers.test.ts` | Mine | — |
| `packages/web-app/src/globals.d.ts` | Mine | — |
| `packages/web-app/src/lib/e2eBus.ts` | Mine | — |
| `packages/web-app/src/lib/participantState.ts` | Mine | — |
| `packages/web-app/src/lib/session.ts` | Mine | — |
| `packages/web-app/src/lib/testLevers.ts` | Mine | — |
| `packages/web-app/src/views/InMeeting.svelte` | Mine | — |
| `packages/web-app/src/views/JoinMeeting.svelte` | Mine | — |
| `packages/web-app/src/views/ParticipantRow.svelte` | Mine | — |
| `packages/web-app/tests/bundle-content.test.ts` | Mine | — |
| `packages/web-app/tests/clientMetricNames.test.ts` | Mine | — |
| `packages/web-app/tests/configEnv.test.ts` | Mine | — |
| `packages/web-app/tests/credentialScan.test.ts` | Mine | — |
| `packages/web-app/tests/instanceCounters.test.ts` | Mine | — |
| `packages/web-app/tests/testDefines.test.ts` | Mine | — |
| `packages/web-app/tests/testToneDefine.test.ts` | Mine | — |
| `packages/web-app/vite.config.ts` | Mine | — |
| `packages/web-app/vite/testDefines.ts` | Mine | — |
| `packages/web-app/vite/testTone.ts` | Mine | — |
| `packages/web-app/vitest.config.ts` | Mine | — |
| `packages/sdk-core/src/media/MediaTransport.ts` | Mine | — |
| `packages/sdk-core/src/media/__tests__/media-transport.test.ts` | Mine | — |
| `packages/web-app/e2e/toneDetector.ts` | Mine | — |
| `packages/web-app/tests/toneDetector.test.ts` | Mine | — |
| `scripts/layer7.sh` | Not mine, Minor-judgment | infrastructure |
| `scripts/dev-web.sh` | Not mine, Mechanical | infrastructure |
| `scripts/dev-web.test.sh` | Not mine, Mechanical | infrastructure |

Notes on the Not-mine rows:
- `crates/dt-guard/src/ts_retained_credentials.rs`: `MAX_DECL_BLOCK_LINES` 2000 -> 2400 plus its measured-figure doc line. The guard's own tree-measuring test (`cap_clears_the_largest_real_declaration_with_margin`) failed because `MeetingSession` grew to 737 lines, past 2000 / 3; before this task it was at 665, exactly at the edge. The test's message prescribes this edit. It is a policy threshold (content), not machinery; @security ruled on it at Gate 3 and approved it; the crate itself is infrastructure's. WHY RAISE RATHER THAN SPLIT: `MeetingSession` was already at 665 lines, exactly at the 2000 / 3 edge, so ANY addition to the session facade would have tripped it. The server-mute surface (`isHost`, `setServerMute`, `requestUnmute`, `currentKekGeneration`) belongs on the public facade. Splitting the facade is a design change to the public SDK class, which is out of this task's scope. The margin is now 3.26x, and @security notes that the NEXT growth should split rather than raise again.
- `docs/runbooks/client-dev-local.md`: three hear-yourself passages + ladder rung 2; F13 left to story task 17
- `scripts/dev-web.sh`: two hear-yourself comments; `vite/testTone.ts` -> `vite/testDefines.ts` path references
- `scripts/layer7.sh`: removed the `VITE_TELEMETRY_ENDPOINT` export (the playwright.config.ts webServer env wins for the server Playwright starts, so it was a dead second copy — @operations F1), leaving a one-line pointer; the relative/same-origin rationale moved beside the one value.
- `scripts/dev-web.test.sh`: one comment path reference `vite/testTone.ts` -> `vite/testDefines.ts`

Excluded from the table by the guard's symmetric exclusions: this main.md; `docs/TODO.md` (a meeting-controller entry for the `server_initiated` classification, @test T7); `docs/specialist-knowledge/{client,test,dry-reviewer,semantic-guard}/INDEX.md` (stale `media-loopback.spec.ts` pointers repointed, @operations F2; Not mine, Mechanical, for the other three); `docs/user-stories/2026-08-27-hear-yourself-through-handler.md` (supersede notes on story-1 env-tests A and B; Not mine, Mechanical, test). `media-loopback.spec.ts` -> `solo-participant.spec.ts` is a git rename, so only its new path is listed. `vite/testTone.ts` -> `vite/testDefines.ts` and `tests/testToneDefine.test.ts` -> `tests/testDefines.test.ts` diff as delete + add and are listed on both sides.

NOT touched, deliberately: `client-dev-local.md` F13 (the "In loopback that is unambiguous" passage) and `mh-incident-response.md` Scenario 17 "The loopback reading" — both are enumerated verbatim in pending story task 17 (operations), which carries specific instructions for them. `proto/`, and `crates/**` apart from the dt-guard cap, are unchanged. The browser timeout in `scripts/layer7.sh` is NOT raised, because the suite measured under 40% of it.

---

## Planning

### Restated mechanism

Instance: "render slots/unreachable/mute, add host mute, write S1/S2/S3/S6/S10a". Mechanism: *every per-participant media fact the wire carries (slot state, `unreachable_sender_ids`, `ParticipantMuteUpdate`) reaches the DOM as its wire token, and every browser claim about one participant is proven from that participant's own bus, two-sided with a positive control in the same window.* Same-owner siblings found by that restatement and folded in: the roster lacks `senderId` (needed to map `unreachable_sender_ids` / slot senders onto roster rows); the SDK drops `ParticipantMuteUpdate` and `UnmuteRequest` on the floor today (`default:` arm); and **a post-join `FORBIDDEN` makes the client close its own signaling connection** (`errorCodeMap.ts` AUTH_CLASS) — MC's ONLY `FORBIDDEN` is the server-mute refusal (`connection.rs:1744`), so today a host whose target just left (UnknownTarget) or a non-host who tries would be disconnected by their own client. Fixed here (client-only): post-join `FORBIDDEN` is a request refusal, surfaced as a session `error`, connection kept; pre-join and `UNAUTHORIZED` behaviour unchanged.

### S10a decision: BUILT (client lever), designed with security

One opt-in build define `__DT_TEST_LEVERS__` (env `DT_TEST_LEVERS=1`) in the `__DT_TEST_TONE__` family. It gates three per-browsing-context levers read from ONE init-script global `window.__dt_test_levers__` (Playwright `context.addInitScript`), read at ONE site `web-app/src/lib/testLevers.ts`, only inside `if (__DT_TEST_LEVERS__)`:
- `blockHandlers: string[]` — S10a. Applied as a wrapper around the web app's injected WebTransport `connect` (`lib/session.ts:makeConnect`), which REFUSES (throws) a dial to an exact-match blocked URL. It can only *narrow*: it never supplies a URL — every dial target still comes from MC's `JoinResponse.media_servers` via the SDK's one dial site (`MediaTransport.connectAll`), and a blocked handler is reported `failed` exactly as an unreachable one would be (MC derives visibility from MH-observed connections, so this is the real-world cause being simulated). No SDK option is added (L4): nothing narrowing exists in sdk-core or in any prod bundle. Loud on misuse (L3): unknown keys / non-array / non-string / empty entries THROW at session build; after join, a blocked URL that was not offered, or a block list that leaves zero offered handlers, disconnects the session and renders `test-lever-error` + bus `testLeverError` — never a silent connect-all/connect-none. Selection is by exact match against the offered list, obtained from A's own `joined.mediaServers` (MC says "order carries no meaning", so never by index, never by pod ordinal).
- `forceHostControls: true` — M2: renders the host affordance in a non-host context so the server's refusal (not a hidden button) is what the test exercises.
- `receiveSlots: string` — S2's per-context N=2, parsed by the SDK's own strict `parseReceiveSlots` (malformed THROWS), overriding `config.receiveSlots` only in that context. `expectDeclaredReceiveSlots` gains the expected N.
Gate machinery (L1/L5, DRY #4): `vite/testTone.ts` becomes `vite/testDefines.ts` with ONE predicate `resolveOptInTestDefine(envName, mode, raw)` (unset=off, exactly `1`, THROW otherwise, THROW in production) feeding both defines; sites: `web-app/vite.config.ts`, `web-app/vitest.config.ts` (`false`), `web-app/src/globals.d.ts`. sdk-core and sdk-svelte never reference it, so they get no declaration (declaring an unused define would be dead config). Bundle absence: `__DT_TEST_LEVERS__` in GATE_IDENTIFIERS, `__dt_test_levers__` and `testLeverError` in FORBIDDEN; the positive-control build opts in `DT_TEST_LEVERS=1`; a "production + DT_TEST_LEVERS=1 throws" test. The Rust mock-client env-test from task 20 remains the routing proof; S10a adds the browser rendering + per-entity hearing proof.

### SDK (`@darktower/sdk-core`)
- `SignalingClient`: `participantMuteUpdate` -> `participantMuteChanged {participantId, audioServerMuted, serverMutedBy?}` (self-mute booleans are informational snapshots per MC and NOT projected; `serverMutedBy` is a participant id, bounded to 256 bytes, rendered only via roster lookup); `unmuteRequest` (MC relay to host) -> `unmuteRequested {participantId}`. `sendServerMuteRequest(targetId, audioMuted)` builds `ServerMuteRequest{participantId: target, audioMuted, videoMuted: false}` — target + action only, no requester, no reason. `sendUnmuteRequest()` builds `UnmuteRequest{requestAudio: true}` (participant_id left empty; MC stamps it).
- `RosterParticipant` gains `senderId?: number` (wire `optional uint32`, never coerced).
- `MeetingSession`: bridges both events; `setServerMute(participantId, muted)`, `requestUnmute()`; `isHost` = the meeting token's `role` claim decoded ONCE in `join()` (payload segment only, bounded, only the bounded role survives — no token retained; UI hint, MC is the authority; any decode failure -> not host + one WARN with no token); `currentKekGeneration` getter (a number already on the wire in every key id; for the S6 per-participant bus evidence).
- errorCodeMap: post-join FORBIDDEN does not close (see above).

### Svelte adapter
`MediaStore`: `serverMutes` (own `$state`: participantId -> {serverMutedBy?}, REPLACED per update, entry removed when unmuted), `unmuteRequests` (own cell; cleared when that participant's server mute lifts). Bound in `bindMeetingSession`'s one subscription. `MeetingStore` roster carries `senderId`.

### Web app UI
- `InMeeting.svelte`: slot GRID of N cells; each cell `slot-${id}` carries `data-slot-state` (wire token), `data-sender-id`, the assigned participant's name via roster lookup (unknown id -> "unknown participant", bounded), and text from `slotState.ts` (no restated strings). `fewer_sources` is an empty cell with its text — an under-filled grid, no spinner. Own server-mute indicator `server-mute-state` (`data-server-muted`, "muted for everyone by <name>") DISTINCT from the client-mute `mute-state`; while server-muted a `request-unmute` button sends the unmute request (never flips anything locally — state only from the wire, M3). Client mute toggle stays independent (composes). Comment header rewritten (no hearing-yourself).
- Roster rows (`participant-${id}`, `ParticipantRow.svelte`): `data-reachability` = `source_unreachable` when the participant's senderId is in `unreachableSenderIds`, else `reachable`, text `SLOT_STATE_TEXT.source_unreachable`; `data-server-muted` + "muted for everyone by <name>"; `data-client-muted` = their assigned slot is `source_muted` and they are not server-muted (self-mute does not fan out on the roster per MC; the slot state is the only wire signal); host-only `server-mute-${id}` button ("Mute for everyone" / "Unmute for everyone" from the wire state) and `unmute-requested-${id}` notice. No "ban" anywhere. Text interpolation only, no `{@html}`. Pure derivation in `lib/participantState.ts` (unit-tested).
- `e2eBus.ts` (whitelist projections, no spread): `slotAssignments` gains `unreachableSenderIds` (stringified); `participantMute {participantId, audioServerMuted, serverMutedBy?}`; `unmuteRequested`; roster `senderId`; `kekGeneration` sampled on the existing tick; `testLeverError`. One command on the bus, gated: `flushMetrics(): Promise<void>` (calls sdk-core `flushMetrics`) — needed for the telemetry positive control and the R-27 read-back. The bus gets no mute command.

### Browser env-tests (Layer 7, `packages/web-app/e2e`)
Discipline (test F1-F6, ops 3/4/6): fresh meeting per test, created with the HOST cohort member's token (GC makes the creator host: `handlers/meetings.rs` `is_host` -> `MeetingRole::Host`); every context closed in `finally`; peak concurrent contexts = 4, never stacked; admission baseline (`mhAdmissionRejectionsByInstance`) before the joins of EVERY multi-party test, passed to `expectHearsSender`; every flat window through `expectCountersFlatOverWindow` (sample floor) paired with a moving positive control in the same window; wire tokens only; no wall-clock asserts; per-participant claims only from that participant's bus, Prometheus only for pipe/name/alert applicability (obs 6). New fixture helpers extend fixtures.ts in place: `openCohortContexts(browser, indices, levers?)`, `expectReceiveLayers(page, receiverLabel, sender, slot)` (L1 keyed and L2 verified advance at the expected (slot, sender), and NO count under any other (slot, sender) key for that sender), per-sender accepted-count reader from `receiveLayers` (`verified` at (slot, sender)), `expectSenderFlatWhileOthersAdvance`.
- **S1** (`multi-party-hear.spec.ts`): 4 participants, N=3; all 12 ordered pairs: L1 at the slot MC assigned, L2, L3 two-sided tone (`expectHearsSender` with baseline). No handler assumption. Includes **R-27 part 1** (send counter `dt_client_media_frames_sent_total` per-instance baseline -> flush every page -> poll above; the exact `_total` string rising is the suffix-survival proof — a solo client sends nothing under R-3, so part 1 runs here, not solo: recorded interpretation) and **R-27 part 2** (loaded rule from Prometheus `/api/v1/rules`, its `dt_client_*` names set-equal to the shared constant — distinct failure for "rule not loaded" vs "drifted"; then `dt_client_media_frames_received_total` by exact name, baseline -> above). Dropped-name live exactness: residual recorded (no deterministic drop trigger); covered by the static drift guard.
- **S2** (`over-subscription.spec.ts`), interpretation recorded (task text has no "third slot" at N=2): R declares N=2 via lever, joins first. Phase 1: one sender -> slot 0 `active`, slot 1 `fewer_sources` (under-filled grid, no spinner). Phase 2: two more senders join in order -> R's slots hold the two earliest; the third is in no assignment and has zero L1/L2 at R, WHILE its framesSent rises and a full-N receiver's accepted-from-third advances.
- **S3 + client structural mute** (`server-mute.spec.ts`): baseline all hear B; host clicks `server-mute-${B}`; every receiver's slot for B reads `source_muted` and its roster row `data-server-muted=true` (distinct attr from `data-client-muted`); accepted-from-B flat at every receiver WHILE accepted-from-another advances AND B's framesSent rises; B's tone absent at every receiver; B's own `mute-state` unchanged and `server-mute-state` true; B clicks `request-unmute` -> host shows `unmute-requested-${B}`, B stays server-muted, accepted-from-B stays flat (R-10); host unmutes -> accepted-from-B advances, tone back. **Named test "client structural mute (restores the coverage task 6 removed)"**: C mutes via `setMuteViaUi`; `expectEgressFlatWhileMuted(C)` WHILE receivers' accepted-from-A advances; receivers see C's slot `source_muted` with `data-client-muted=true`, `data-server-muted=false`; C unmutes -> egress and accepted-from-C resume. Compose: host server-mutes B while B is client-muted; B client-unmutes -> B's egress resumes, receivers stay flat. **Non-host refusal (M2)**: a third lever `forceHostControls: true` (same gate, same global) renders the host affordance in a NON-host context; the non-host clicks `server-mute-${B}`; MC refuses (FORBIDDEN, counted server-side); the non-host shows `last-error` `forbidden:` AND stays joined (meeting-state `joined`, its accepted counters keep advancing — proves the post-join FORBIDDEN fix), and B is NOT server-muted at any receiver.
- **S10a** (`partial-connectivity.spec.ts`): A joins (no lever), reads offered `[u0,u1]` (precondition: >= 2, fail loud); B blocks u1, C blocks u0. Connected sets from `mediaConnected`: B={u0}, C={u1}, disjoint, A=union — proven before any hearing assertion. A hears B and C at all three layers; B and C each hear only A; on B, roster row C `data-reachability=source_unreachable` (polled until stable across the MC settle, R-33) and not `fewer_sources`/`source_muted`; same for C. Two-sided: C's accepted count at B stays 0 WHILE A's advances in the same window; same for B at C.
- **S6** (`kek-rotation.spec.ts`): A, B, C join; C leaves (context close + view teardown); A's and B's bus `kekGeneration` advances (primary per-entity evidence); flush A and B (awaited); Prometheus per-instance delta (baseline before the leave, absent = 0) of `dt_client_media_kek_updates_total{source="kek_update"}` and `dt_client_media_kek_generations_retained_total` >= 2 (remaining clients). Substitution recorded: `increase()` cannot see a series' birth value and both series are typically born by the rotation, so the task's `increase()` would read ~0 on correct behaviour; per-instance baseline->delta keeps "counter, assert a delta". Then D joins after rotation: D's L2 for A and B advances and tone present (D holds only the current generation, so opening their frames proves current frames), A/B hear D. Per-test timeout derived from `MC_KEK_ROTATION_DEBOUNCE_SECONDS` read from `infra/services/mc-service/config.env` (parsed like `cohort.ts` parses AC's config).
- **Solo**: `media-loopback.spec.ts` renamed `solo-participant.spec.ts` (R-3 solo coverage kept, loopback name and COVERAGE-LOSS block gone).
- **Telemetry credential-scan positive control**: pure `e2e/credentialScan.ts` extracted from `assertTokenOnlyJoinTraffic`, returning per-surface counts `{scanned, bearerChecked}`; for `/api/v1/telemetry` requests the `Authorization` header MUST be present and `Bearer ` (T2); needles/order/redact unchanged. Telemetry is ON in this suite as an asserted precondition, with no "when enabled" branch. Every join window forces `flushMetrics()` via the bus inside the window and asserts telemetry `scanned >= 1` and `bearerChecked >= 1`, with its own reason token for "zero telemetry requests recorded". Applied in join-happy-path and every context of the multi-party specs. Node tests: zero-telemetry fails, header-absent fails, needle fails, distinct tokens.
- **Shared names** (`e2e/clientMetricNames.ts`, observability's shape): alert name, the two selected names, send counter, KEK names with source imported as `MEDIA_KEK_SOURCES.KekUpdate`. PromQL only in `mcMetrics.ts`. Drift guard `tests/clientMetricNames.test.ts`: YAML-parse `mc-alerts.yaml`, rule found, names set-equal, reason alternation non-empty and each token in `ALL_REJECT_REASONS`, every name a quoted literal in `mediaMetrics.ts`; each positive control has its own reason text.
- **Budget** (ops 1): current suite 60 s. Estimates: S1+R-27 ~75 s, S2 ~45 s, S3+structural ~100 s, S10a ~45 s, S6 ~120 s (W=60 s), solo 5 s -> ~450 s, 75% of 600 s. So the plan was to raise `DEVLOOP_BROWSER_E2E_TIMEOUT` to 900 s if the measured total came out over ~70% of 600 s. MEASURED: ≈3.5–4 min, under 40%, so it is NOT raised (Implementation Summary (v)). The measured figures are in README §Budgets. Sign-ins: ~22 per run, well under the AC window (test F5).
- `playwright.config.ts` webServer env adds `DT_TEST_TONE=1`, `DT_TEST_LEVERS=1`; specs assert tone mode and lever presence rather than assuming them (a reused server fails loudly).

### Docs
`e2e/README.md` (new specs, budgets, lever, traces/HARs of multi-context runs carry live tokens — gitignored, never promoted); `client-dev-local.md` hear-yourself passages outside task 17's list; `dev-web.sh` two comments; INDEX. Story-1 hear-yourself expectations removed from web-app comments (InMeeting header, MediaStore, fixtures).


### Gate-1 amendments (test G1-G6, semantic-guard a-c)
- G1: the Layer-7 suite ASSERTS telemetry is configured (bus `telemetryConfigured`, like tone mode and lever presence). A build without it fails loudly; the scan never skips.
- G2: leaving is always GRACEFUL: the demo's own teardown (`nav-create` unmounts the join view → `session.disconnect()` → a clean close, the path the existing leave test uses) before `context.close()`, in S6 and in every `finally`. So rotation costs W, not grace + W, and edges do not linger into the next test.
- G3: in S10a, B's and C's own `joined.mediaServers` must set-equal A's offered set before the block lists are trusted.
- G4: M2 gets a mover. After the refusal, accepted-from-B keeps ADVANCING at the receivers, and B's slot stays non-`source_muted` over a sampled window.
- G5: S2 joins are serialised. The next join starts only after each sender's `joined` event AND its first assignment at R.
- G6: S6 gates D's join on A's and B's `kekGeneration` both having advanced AND being equal.
- semantic-guard (a): `meetingRole.ts` keeps only the bounded role enum. The decoded payload is dropped, and the WARN carries no token, payload or claims.
- semantic-guard (b): every new credentialScan failure message goes through `redact()`, the scheme failure included.
- semantic-guard (c): the bus `flushMetrics` is gated, returns void, and puts nothing about the export on the bus.
- security C1-C4, H1-H3: the levers are read ONCE at session build into a validated, frozen copy, and no live reference is kept. There are unit tests for the not-offered and zero-remaining checks and for every throw-at-build case. `receiveSlots` goes through `parseReceiveSlots`, and the SDK's cap refusal is untouched. The control build sets `DT_TEST_LEVERS=1`. The role is decoded only after `validateUserToken`, only the boolean survives, and the role is compared exactly against `MeetingRole::Host`'s serde spelling. S3 uses the REAL host affordance in a real host context. The FORBIDDEN rule is pinned by tests: pre-join FORBIDDEN still closes, post-join UNAUTHORIZED still closes, and post-join FORBIDDEN surfaces an `error` and keeps the connection.
- operations A: the timeout number stops being restated. `docs/runbooks/devloop-validation.md:676` and `e2e/README.md` (:193, :333) cite `scripts/layer7.sh` `BROWSER_E2E_TIMEOUT` instead, and the layer7.sh comment gives the new reason. `devloop-validation.md` is added to Cross-Boundary as Not mine, Minor-judgment, operations.
- operations B: the existing build-knob precondition (`readOwnTone` → `expectDeclaredReceiveSlots`) is extended to also assert lever presence (bus `testLevers: {enabled}`) and telemetry configured. It runs before any media assertion, and its message names the reused-server cause.
- Observability blocker: `playwright.config.ts` webServer env gets `VITE_TELEMETRY_ENDPOINT: '/api/v1/telemetry'` (relative) beside `DT_TEST_TONE` and `DT_TEST_LEVERS`, and telemetry-on is an asserted precondition. The bus `flushMetrics` REJECTS with its own reason (`telemetry_not_configured`) when telemetry is off, and never resolves silently. The positive control is UNCONDITIONAL in this suite. R-27 parts 1 and 2 are separate `test.step`s with distinct failure messages. The S6 comment cites `mc_meeting_kek_rotation_window_seconds` as the running value to check.
- DRY (a): one e2e config.env reader (`parseConfigEnv` + `requirePositiveInt`) extracted from `cohort.ts:parseAcAuthRateLimit`, which the AC read and the MC W read both consume. (b): one sdk-core base64url→bytes helper, used by `ed25519.ts` export and `meetingRole.ts`. (c): all mute and identity copy has ONE home, `lib/participantState.ts`, consumed by InMeeting and ParticipantRow. The e2e tests assert tokens only, and `dt_client_*` literals live only in `clientMetricNames.ts`.
- code-reviewer:
  1/2: as DRY (a)/(b). The base64url helper handles padding and gets its own tests.
  3: the oracle becomes `closesConnection(code, phase: 'joining' | 'joined')`. `isAuthClass` is removed, the header is corrected, and the tests are parametrised over code × phase.
  4: `data-client-muted` is tri-state (`true`/`false`/`unknown` — unknown when no slot carries the participant), an exhaustive union in participantState.ts. The ordering window (the slot goes `source_muted` before `ParticipantMuteUpdate`) is commented, and S3 polls to the settled state.
  5: the new helpers go in `e2e/receiveEvidence.ts` and `e2e/cohortContexts.ts`, and fixtures.ts does not grow except for the in-place signature changes.
  6: one `parseTestLevers()` returns a frozen, typed `TestLevers`, read at ONE gated site. Consumers take the parsed object.
  Names: `serverMutes: ReadonlyMap`, `unmuteRequests: ReadonlySet`, both replaced on update.

### Gate 1

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Semantic Guard | confirmed |

---

## Implementation Summary

**S10a decision: BUILT, the client lever, designed with @security** (Gate 1 L1-L5, C1-C4). Web-app only: `__DT_TEST_LEVERS__` (`DT_TEST_LEVERS=1`, one predicate with `__DT_TEST_TONE__` in `vite/testDefines.ts`, THROW in production). There is no SDK option. `blockHandlers` wraps the injected `connect` and refuses exact-match offered URLs. Bundle absence is proven by the existing `tests/bundle-content.test.ts`, which now uses a positive-control build that opts in, and there is a "prod + opt-in throws" test. Task 20's Rust test (`27_mc_slot_placement.rs::test_canonical_partial_connectivity_across_two_handlers`) remains the routing proof below the browser.

- **sdk-core**
  - `participantMuteUpdate` / `unmuteRequest` intake: server mute only, ids bounded to 256.
  - `sendServerMuteRequest` (target + action) and `sendUnmuteRequest`.
  - `closesConnection(code, phase)`: a post-join FORBIDDEN is a request refusal.
  - `RosterParticipant.senderId`.
  - `MeetingSession.{isHost, currentKekGeneration, setServerMute, requestUnmute}`.
  - `session/meetingRole.ts`: a bounded role read; only the boolean survives.
  - `encoding/base64url.ts`: one decoder, shared with `ed25519.ts`.
  - The test-tone capture channel fix (see Issues).
- **sdk-svelte**: `MediaStore.serverMutes` (a ReadonlyMap) and `unmuteRequests` (a ReadonlySet), each in its own cell and replaced on change; both bound in the one subscription.
- **web-app**
  - `InMeeting` renders the slot grid (sender name, `data-sender-id`, fewer-sources as an empty cell). It adds its own `server-mute-state`, distinct from `mute-state`, and `request-unmute`.
  - `ParticipantRow` carries `data-reachability` / `data-server-muted` / `data-client-muted` (tri-state) plus the host `server-mute-${id}` / `unmute-requested-${id}`. The name is in its own `participant-name-${id}` element.
  - All copy lives in `lib/participantState.ts`.
  - `lib/testLevers.ts` holds the three levers, frozen and read once.
  - The e2e bus gains `participantMute`, `unmuteRequested`, roster `senderId`, `unreachableSenderIds`, `kekGeneration` (sent on change), `buildKnobs`, and a gated `flushMetrics` that rejects `telemetry_not_configured`.
- **e2e (Layer 7)**
  - New specs: `multi-party-hear` (S1 + R-27 parts 1/2), `over-subscription` (S2), `server-mute` (S3, client structural mute, non-host refusal), `partial-connectivity` (S10a), `kek-rotation` (S6 joiner half).
  - `media-loopback.spec.ts` is renamed to `solo-participant.spec.ts` and its COVERAGE-LOSS block is removed.
  - New modules: `cohortContexts.ts` (graceful leave + close in `finally`), `receiveEvidence.ts` (per-entity layers, `observeWindow` with the sample floor and a required mover), `credentialScan.ts` (pure scan, per-surface counts, unconditional telemetry positive control), `clientMetricNames.ts` (observability's shared constant, and YAML-structural drift guard), `configEnv.ts` (one config.env reader, MC W + grace).
  - `mcMetrics.ts` gains the client read-backs, `loadedAlertClientNames` and `minDelta`.
  - The webServer env adds `DT_TEST_TONE`, `DT_TEST_LEVERS` and a relative `VITE_TELEMETRY_ENDPOINT`.
- **Docs**
  - `e2e/README.md`: new multi-party section, a measured wall clock, and the traces/HARs token note.
  - `client-dev-local.md`: three hear-yourself passages plus ladder rung 2. F13 is left to task 17.
  - The comments in `dev-web.sh`.
  - Supersede notes in the story-1 doc.

**Recorded interpretations:**
- (i) **S2.** There is no third slot at N=2, so the scenario runs in two phases (under-fill, then over-subscription with a mover).
- (ii) **R-27 part 1 runs in S1, not solo.** A solo client sends nothing under R-3 (`egress.ts` counts only after a send).
- (iii) **S6 uses a baseline→delta with `minDelta` = remaining clients instead of PromQL `increase()`.** `increase()` cannot see a series born in the window.
- (iv) **Live exactness of `dt_client_media_frames_dropped_total` is RESIDUAL.** There is no deterministic drop trigger. Coverage is the static guard, part 1's verbatim-export proof, and the loaded-rule equality. This is recorded at the spec site too.
- (v) **`DEVLOOP_BROWSER_E2E_TIMEOUT` is NOT raised.** The suite measured ≈3.5–4 min (<40% of 600 s), below the ≥70% line @operations set for a raise. The README records the per-spec figures.

**Layer 7 (run by hand against the live Kind cluster, `/tmp/run-e2e.sh`, which mirrors Phase 1h + 2):** all 17 browser tests green. S6 ≈ 100 s, all others ≤ 16 s.

---

## Code Review Results

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 1 | 1 | 0 | S-1 sync connect throw → bounded TRANSPORT failure; dt-guard cap 2000→2400 approved as policy |
| Test | RESOLVED-DEFERRED | 7 | 6 | 1 | T7: MC clean-disconnect classed server_initiated (30 s grace) — MC-owned, TODO.md §Media Path Obligations |
| Observability | RESOLVED-FIXED | 5 | 5 | 0 | name drift `satisfies`, flush sink check, rules status, minDelta, S6 poll |
| Code Quality | RESOLVED-FIXED | 9 | 9 | 0 | |
| DRY | RESOLVED-FIXED | 2 | 2 | 0 | flat-window constants, laneCarriesTone |
| Operations | RESOLVED-FIXED | 2 | 2 | 0 | telemetry endpoint SSoT, INDEX repoints |
| Semantic Guard | RESOLVED-FIXED | 2 | 2 | 0 | credential-leak in scheme print; comment drift |

---

## Accepted Deferrals

- `docs/TODO.md` §Media Path Obligations — MC classes clean SDK disconnect as server_initiated (30 s grace)

---

## Rollback Procedure

1. Start commit: `4a2f5aa2d7e9f6d15ebd1ed5b3335687d9fa4a95`
2. `git diff 4a2f5aa2..HEAD`
3. **Safe-revert unit**: the whole commit — the e2e specs depend on the SDK/UI wire rendering, the test-lever define and bundle-absence tests move together, and the post-join FORBIDDEN fix is what the non-host refusal spec proves.

---

## Issues Encountered & Resolutions

- **Test-tone builds sent NOTHING (a client bug, fixed here).** `createTestToneCapture`'s `MediaStreamAudioDestinationNode` defaults to STEREO. The encoder is mono, and Chromium's `AudioEncoder.encode` rejects the first 2-channel frame, which is a FATAL encoder fault that stops the pipeline. Every unit tier uses seam doubles, so only Layer 7 could see it. Fix: the tone graph's channel count is pinned to the seam's `channels` (`channelCountMode = 'explicit'`), with a platform-stub regression test (`media/setup/__tests__/testToneCapture.test.ts`).
- **A post-join `FORBIDDEN` made the client close its own connection.** MC's only FORBIDDEN is the server-mute refusal, so it is a request refusal. Fixed in `errorCodeMap.closesConnection(code, phase)`, with code × phase tests. The browser proof is the non-host refusal test.
- **A SYNCHRONOUSLY throwing `connect` left a handler stuck `connecting` (@security S-1, Gate 3).** The throw escaped `MediaTransport.#connectOne` before any failure was recorded, and `allSettled` swallowed it. That affected the S10a lever and also production: a missing WebTransport constructor or a URL the constructor rejects. It is now recorded as a bounded TRANSPORT failure, with a unit test.
- **FOLLOW-UP, not in scope (meeting-controller), tracked in `docs/TODO.md` §Media Path Obligations (@test T7):** MC classifies the SDK's CLEAN disconnect as `cause: server_initiated` and holds the participant through the 30 s disconnect grace. It does not use the immediate `ClientClosed` removal. The SDK closes its MH transports and then signaling; MC's bridge loop ends via "outbound channel closed" and not via `resolve_close_cause`. This is pre-existing: story 1's leave test already budgets for the grace path. Its cost here is that S6 waits grace + W (≈100 s) instead of W. It is task-sized, because it needs MC to decide how a signaling-stream end after MH departures is classified, so it is surfaced to the Lead rather than fixed here.
- **The roster row gained indicators and host controls.** The story-1 exact-name assertion (`expectRosterShows`) now reads a dedicated `participant-name-${id}` element.
- **No YAML library in `@darktower/web-app`.** Adding one changed the lockfile beyond the one package, so it was reverted. The drift guard reads the rule structurally instead (the list item → its `expr:` key → block scalar, by indentation), with a self-test and three distinctly-worded positive controls.

---

## Lead Notes

- Gate 2: `DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` exit 0, TOTAL_RESULT=N/A (proto intentional-gap placeholders only); L7 `env-tests-passed`, `browser-e2e-passed` 17/17 (3.8 m).

- Gate 1 rulings (Lead): (1) post-join FORBIDDEN no-close fix in scope — accepted (security verified). (2) `scripts/layer7.sh` browser timeout 600→900 accepted as Minor-judgment (operations on panel). (3) F13 / MH Scenario 17 loopback text left to story task 17 — manifest assignment, not a deferral. (4) Solo spec renamed (R-3 coverage), story-1 hear-yourself loopback expectation removed — accepted. (5) Host UI hint from token role claim — accepted per security (UI hint only). Classification guard: `STATUS=OK`.

- Teammate INDEX / review-protocol injection: teammates are instructed to Read the exact files (`docs/specialist-knowledge/{name}/INDEX.md`, `.claude/skills/devloop/review-protocol.md`) verbatim as their first action rather than having ~200 KB inlined through the Lead prompt.
