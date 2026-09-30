# Devloop Output: Test-tone send mode, receive-slot count N, and receive verification test bus

**Date**: 2026-09-30
**Task**: run-story task #13 (docs/user-stories/2026-09-21-hear-each-other.md; R-1, R-7, R-23, R-28, R-30) — test-tone capture seam gated by `__DT_TEST_TONE__`, configurable receive-slot count N with loud cap rejection, three-layer receive verification surface on the test bus, `dt_client_media_receive_source_deficit_total`, and atomic six-site export wiring for `dt_client_media_capture_source{mode}`.
**Specialist**: client
**Mode**: Agent Teams (v2) — full, Gate-1 present
**Branch**: `feature/hear-each-other`
**Duration**: ~3h

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `dfd39c6602498ddfeac715428c72103f53638a24` |
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
| Security | `security` (paired) |
| Test | `test` (paired) |
| Observability | `observability` (paired) |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Semantic Guard | `semantic-guard` |
| Paired | `paired-protocol`, `paired-meeting-controller` (Q1-A GSA), `paired-global-controller` (Domain-judgment: `crates/gc-service/src/services/telemetry_filter.rs`) |

---

## Task Overview

### Objective
See task description (run-story task #13 prompt).

### Scope
- **Service(s)**: `@darktower/sdk-core`, `packages/web-app`, GC telemetry filter, otel-collector config, observability docs
- **Schema**: No
- **Cross-cutting**: Yes

### Debate Decision
NOT NEEDED - design settled in the user story.

---

## Cross-Boundary Classification

Every file in the diff has a row (the scope-drift guard reads this table). Owner involvement per SKILL.md §Cross-Boundary Edits: only the Domain-judgment / GSA rows pull an owner into planning + review; Minor-judgment rows are review-only.

| Path | Classification | Owner | Notes |
|------|----------------|-------|-------|
| `crates/gc-service/src/services/telemetry_filter.rs` | Not mine, Domain-judgment | global-controller (+ security) | `mode` into `MEDIA_DATAPOINT_EXTRA` (datapoint tier only), numeral-free prose, tests pin `mode` datapoint-only |
| `proto/dark_tower/signaling/v1/signaling.proto` | Not mine, Domain-judgment (GSA wire format) | protocol | GSA: security + meeting-controller also in planning/review (intersection rule). Additive `optional uint32 max_receive_slots` on `JoinResponse` so the SDK can expose the server cap and refuse an over-cap declaration locally |
| `crates/mc-service/src/webtransport/connection.rs` (+ join-response builder/test fixtures) | Not mine, Domain-judgment | meeting-controller | populate `max_receive_slots` from the same `ClientMediaConfig::max_receive_slots` the check and `mc_media_receive_slot_cap` read |
| `crates/mc-service/src/config.rs` | Not mine, Domain-judgment | meeting-controller | cap retyped `u8` end to end (meeting-controller amendment 1) |
| `crates/mc-service/src/media_signaling/mod.rs` | Not mine, Domain-judgment | meeting-controller | `ClientMediaConfig.max_receive_slots: u8` |
| `crates/mc-service/src/media_signaling/capability.rs` | Not mine, Domain-judgment | meeting-controller | compare via `usize::from(max_slots)`; test `CAP` retyped |
| `crates/mc-service/src/observability/metrics.rs` | Not mine, Domain-judgment | meeting-controller | `set_receive_slot_cap(u8)` via `f64::from` |
| `crates/mc-service/tests/common/media_session.rs` | Not mine, Domain-judgment | meeting-controller | capture `JoinResponse.max_receive_slots` for the wire tests |
| `crates/mc-service/tests/media_client_signaling_integration.rs` | Not mine, Domain-judgment | meeting-controller | advertised == enforced over the wire; lowered-cap positive control |
| `crates/mc-service/tests/slot_placement_integration.rs` | Not mine, Domain-judgment | meeting-controller | gauge test via `f64::from` |
| `crates/proto-gen/src/lib.rs` | Not mine, Domain-judgment (GSA wire format) | protocol | redacting `Debug` shows the cap; roundtrip tests (Q1-A) |
| `crates/proto-gen/tests/signaling_roundtrip.rs` | Not mine, Domain-judgment (GSA wire format) | protocol | redacting `Debug` shows the cap; roundtrip tests (Q1-A) |
| `docs/API_CONTRACTS.md` | Not mine, Minor-judgment | protocol | JoinResponse rows (fields 11, 12) + normative pointer |
| `docs/observability/label-taxonomy.md` | Not mine, Minor-judgment | observability | sites (5)/(6): catalog entries + `mode` taxonomy row |
| `docs/observability/metrics/client.md` | Not mine, Minor-judgment | observability | sites (5)/(6): catalog entries + `mode` taxonomy row |
| `docs/observability/metrics/mc-service.md` | Not mine, Minor-judgment | meeting-controller | cap now also advertised on JoinResponse |
| `docs/runbooks/client-dev-local.md` | Not mine, Minor-judgment | operations | F18/F19 |
| `docs/runbooks/mc-deployment.md` | Not mine, Minor-judgment | meeting-controller | local SDK refusal vs MC over-cap counter; additive-field skew line |
| `docs/runbooks/mc-incident-response.md` | Not mine, Minor-judgment | meeting-controller | local SDK refusal vs MC over-cap counter; additive-field skew line |
| `infra/grafana/dashboards/client-media.json` | Not mine, Minor-judgment | observability | capture-source, deficit and refusal panels |
| `infra/services/otel-collector/collector.yaml` | Not mine, Minor-judgment | observability | sites (2)-(4): `mode` in keep_keys + shape statement, three names in the allowlist; content pre-ruled by observability |
| `packages/proto-gen/scripts/verify-codegen.sh` | Not mine, Domain-judgment | protocol | codegen presence pin for `maxReceiveSlots` (Q1-A) |
| `packages/sdk-core/src/__tests__/serverMessageSinkScan.test.ts` | Mine | — |  |
| `packages/sdk-core/src/config/__tests__/clientConfig.test.ts` | Mine | — |  |
| `packages/sdk-core/src/config/clientConfig.ts` | Mine | — |  |
| `packages/sdk-core/src/errors/SignalingError.ts` | Mine | — |  |
| `packages/sdk-core/src/globals.d.ts` | Mine | — |  |
| `packages/sdk-core/src/index.ts` | Mine | — |  |
| `packages/sdk-core/src/media/__tests__/helpers.ts` | Mine | — |  |
| `packages/sdk-core/src/media/__tests__/hotPathLayout.test.ts` | Mine | — |  |
| `packages/sdk-core/src/media/lifecycle/AudioPipeline.ts` | Mine | — |  |
| `packages/sdk-core/src/media/lifecycle/__tests__/audioPipeline.degradation.test.ts` | Mine | — |  |
| `packages/sdk-core/src/media/lifecycle/__tests__/audioPipeline.deviceAndCounts.test.ts` | Mine | — |  |
| `packages/sdk-core/src/media/lifecycle/__tests__/audioPipeline.multiSender.test.ts` | Mine | — |  |
| `packages/sdk-core/src/media/lifecycle/__tests__/audioPipeline.test.ts` | Mine | — |  |
| `packages/sdk-core/src/media/pipeline/__tests__/playbackSink.test.ts` | Mine | — |  |
| `packages/sdk-core/src/media/pipeline/__tests__/receiveLanes.test.ts` | Mine | — |  |
| `packages/sdk-core/src/media/pipeline/ingress.ts` | Mine | — |  |
| `packages/sdk-core/src/media/pipeline/playbackSink.ts` | Mine | — |  |
| `packages/sdk-core/src/media/pipeline/receiveLanes.ts` | Mine | — |  |
| `packages/sdk-core/src/media/setup/__tests__/mediaMetrics.test.ts` | Mine | — |  |
| `packages/sdk-core/src/media/setup/capture.ts` | Mine | — |  |
| `packages/sdk-core/src/media/setup/mediaMetrics.ts` | Mine | — |  |
| `packages/sdk-core/src/media/setup/seams.ts` | Mine | — |  |
| `packages/sdk-core/src/session/MeetingSession.ts` | Mine | — |  |
| `packages/sdk-core/src/session/__tests__/meeting-session.test.ts` | Mine | — |  |
| `packages/sdk-core/src/session/events.ts` | Mine | — |  |
| `packages/sdk-core/src/signaling/SignalingClient.ts` | Mine | — |  |
| `packages/sdk-core/src/signaling/__tests__/helpers.ts` | Mine | — |  |
| `packages/sdk-core/src/signaling/__tests__/signaling-client.test.ts` | Mine | — |  |
| `packages/sdk-core/src/signaling/errorCodeMap.ts` | Mine | — |  |
| `packages/sdk-core/src/signaling/events.ts` | Mine | — |  |
| `packages/sdk-core/src/telemetry/telemetryConfig.ts` | Mine | — |  |
| `packages/sdk-core/tests/bundle-content.test.ts` | Mine | — |  |
| `packages/sdk-core/vite.config.ts` | Mine | — |  |
| `packages/sdk-core/vitest.config.ts` | Mine | — |  |
| `packages/sdk-svelte/src/globals.d.ts` | Mine | — |  |
| `packages/sdk-svelte/vitest.config.ts` | Mine | — |  |
| `packages/test-utils/src/media/index.ts` | Mine | — |  |
| `packages/web-app/playwright.config.ts` | Mine | — |  |
| `packages/web-app/src/__tests__/appShell.test.ts` | Mine | — |  |
| `packages/web-app/src/__tests__/authViews.test.ts` | Mine | — |  |
| `packages/web-app/src/__tests__/createMeeting.test.ts` | Mine | — |  |
| `packages/web-app/src/__tests__/e2eBus.test.ts` | Mine | — |  |
| `packages/web-app/src/__tests__/helpers/MockMeetingSession.ts` | Mine | — |  |
| `packages/web-app/src/__tests__/joinMeeting.test.ts` | Mine | — |  |
| `packages/web-app/src/globals.d.ts` | Mine | — |  |
| `packages/web-app/src/lib/config.ts` | Mine | — |  |
| `packages/web-app/src/lib/e2eBus.ts` | Mine | — |  |
| `packages/web-app/src/lib/errorText.ts` | Mine | — |  |
| `packages/web-app/src/lib/session.ts` | Mine | — |  |
| `packages/web-app/src/views/JoinMeeting.svelte` | Mine | — |  |
| `packages/web-app/tests/bundle-content.test.ts` | Mine | — |  |
| `packages/web-app/vite-env.d.ts` | Mine | — |  |
| `packages/web-app/vite.config.ts` | Mine | — |  |
| `packages/web-app/vitest.config.ts` | Mine | — |  |
| `scripts/dev-web.sh` | Not mine, Minor-judgment | infrastructure | header names `DT_TEST_TONE=1 scripts/dev-web.sh` |
| `packages/sdk-core/src/__tests__/joinLabelRoster.ts` | Mine | — |  |
| `packages/sdk-core/src/__tests__/joinLabelSpread.test.ts` | Mine | — |  |
| `packages/sdk-core/src/__tests__/stripComments.ts` | Mine | — |  |
| `packages/sdk-core/src/media/lifecycle/__tests__/audioPipeline.intervalSignals.test.ts` | Mine | — |  |
| `packages/sdk-core/src/media/lifecycle/__tests__/receiveSourceDeficit.test.ts` | Mine | — |  |
| `packages/sdk-core/src/media/lifecycle/receiveSourceDeficit.ts` | Mine | — |  |
| `packages/sdk-core/src/media/pipeline/__tests__/ingress.verification.test.ts` | Mine | — |  |
| `packages/sdk-core/src/media/pipeline/__tests__/receiveVerification.test.ts` | Mine | — |  |
| `packages/sdk-core/src/media/pipeline/receiveVerification.ts` | Mine | — |  |
| `packages/sdk-core/src/media/setup/__tests__/testTone.test.ts` | Mine | — |  |
| `packages/sdk-core/src/media/setup/testTone.ts` | Mine | — |  |
| `packages/web-app/src/__tests__/errorText.test.ts` | Mine | — |  |
| `packages/web-app/src/__tests__/receiveSlotsConfig.test.ts` | Mine | — |  |
| `packages/web-app/src/lib/e2eAnalysis.ts` | Mine | — |  |
| `packages/web-app/tests/testToneDefine.test.ts` | Mine | — |  |
| `packages/web-app/vite/testTone.ts` | Mine | — |  |
| `packages/sdk-core/src/session/mediaWiring.ts` | Mine | — |  |
| `packages/sdk-core/src/media/lifecycle/intervalSignals.ts` | Mine | — |  |
| `packages/sdk-core/src/session/mediaSelection.ts` | Mine | — |  |
| `package.json` | Not mine, Minor-judgment | infrastructure | `pnpm.overrides.brace-expansion` `>=5.0.9` → `>=5.0.11`: GHSA-qhr7-859c-m2p7 / GHSA-6j4f-fj2g-mc7p (high, published 2026-09-29) turned Layer 6 `pnpm audit` red mid-devloop; unrelated to this feature, fixed in-tree rather than deferred |
| `pnpm-lock.yaml` | Not mine, Minor-judgment | infrastructure | lockfile for the override bump |
| `scripts/dev-web.test.sh` | Not mine, Minor-judgment | infrastructure | tone-state preflight line covered both ways (@operations OPS-2) |
| `packages/sdk-core/src/__tests__/sourceFiles.ts` | Mine | — |  |
| `packages/sdk-core/tests/toneMarkers.ts` | Mine | — |  |
| `packages/web-app/src/__tests__/e2eAnalysis.test.ts` | Mine | — |  |

---

## Planning

### Mechanism restatement
The task's instance ("add tone mode, N, a bus") is three mechanisms: (1) a build-time-gated capture-source substitution at the ONE capture seam, (2) a client REQUEST (N) bounded by a server quantity (cap) that must be visible at both ends and fail loudly on mismatch, (3) a test-only, whitelist, sampled observation surface over the receive path at three layers. The export wiring is a fourth: a new datapoint label key crossing five encodings (GC tier, keep_keys, shape, name allowlist, catalog/taxonomy) — all-or-nothing. Wider sibling surfaced: the capture-source gauge and the deficit counter both need an export-interval-cadence emitter; both ride ONE pipeline timer driven from the telemetry export interval SSoT.

### Q1 (Lead ruling needed) — the server cap is not on the client wire
`MC_MAX_RECEIVE_SLOTS` reaches the client nowhere: `JoinResponse` has no field, and MC's over-cap rejection is an `ErrorMessage{INVALID_REQUEST, "receive capability rejected: too many slots"}` indistinguishable by code from other rejections. "Expose the server cap on the SDK diagnostic surface" is impossible without a wire field, and attributing the rejection without one means string-matching an MC message (untyped cross-language coupling). Recommendation **A**: additive `optional uint32 max_receive_slots` on `JoinResponse` (GSA: protocol + security + meeting-controller), MC fills it from the value its check enforces; SDK exposes it and refuses an over-cap declaration locally (typed, counted, visible) before sending. Absent (older MC, the supported rollback) = cap unknown: the SDK declares and relies on MC's rejection, which already surfaces as a session `error`. Alternative **B**: no wire change; diagnostics expose N and `serverCap: undefined`, cap readable only via `mc_media_receive_slot_cap` and dev-web preflight; rejection surfaced by matching MC's static message. B leaves R-23's "slot cap readable at runtime [on the SDK surface]" unmet.

**Lead ruling (2026-09-30): Q1 = A.** Correction to the story's premise (`docs/user-stories/2026-09-21-hear-each-other.md` line 17, "the client↔MC signalling proto is already complete"): it is wrong for this one quantity — `MC_MAX_RECEIVE_SLOTS` has no client-wire representation, and R-23/the task require it on the SDK diagnostic surface. Absent field = cap unknown: the SDK declares and MC's rejection surfaces as a loud session error; MC's message text is NEVER string-matched.

### Design
**Receive-slot count N** — `clientConfig.ts` gains `media.receive.audioSlots` (SDK default **1**: the non-dev fallback; `scripts/dev-web.sh DEMO_RECEIVE_SLOTS=3` is the demo topology — two quantities, stated at both ends) and `parseReceiveSlots(raw)` → `{count, source: 'configured'|'default'}`: strict `^[1-9][0-9]*$` (rejects `0`, `03`, `3.0`, ` 3`, empty, non-numeric; same classes as dev-web.sh), throws `ClientConfigError`, never falls back. Client ceiling = existing `media.ingress.maxDecodeLanes` relation (moved into `validateMediaConfig`, not a new constant). `web-app/src/lib/config.ts` reads `VITE_DT_RECEIVE_SLOTS` through it at load (throws, like `assertTelemetrySharesGcOrigin`) and passes it to `MeetingSession`. `startMedia` declares slots `0..N-1`; `DEFAULT_AUDIO_SLOT_ID` + `StartMediaOptions.slotId` deleted (all consumers: `session/events.ts`, `index.ts`, `MeetingSession.ts`; no sdk-svelte/web-app/e2e consumer). Doc at both ends: N is a REQUEST bounded by `MC_MAX_RECEIVE_SLOTS`; over-cap is rejected WHOLE, never clamped. Diagnostic surface: `MeetingSession.receiveSlots` → `{declared, source, serverCap}`; logged at startMedia with absent-vs-defaulted. Over cap (Q1-A): `startMedia` throws a typed `SignalingError` (`toJSON` allowlist: code, message naming N and cap), increments `dt_client_media_receive_slots_rejected_total` (base labels only), never declares a prefix; web-app renders it in `media-error` and `errorText` names `VITE_DT_RECEIVE_SLOTS`. Record: browser knobs are outside `dt-guard env-config` (reads `infra/services/**` only); the suite asserts N at runtime.

**Test tone** — `__DT_TEST_TONE__` = `mode !== 'production' && process.env.DT_TEST_TONE === '1'`; `DT_TEST_TONE` set to anything other than `1`/unset, or `1` with `mode === 'production'`, THROWS in `vite.config.ts` (opt-in, never default-on, never runtime). Defined in web-app `vite.config.ts` + `vitest.config.ts`, sdk-svelte `vitest.config.ts`, sdk-core `vite.config.ts` (lib dist: hard `false`) + `vitest.config.ts`, and `globals.d.ts` in all three. Derivation `media/setup/testTone.ts`: `testToneFrequencyHz(senderId)` takes ONLY the meeting-scoped sender id (throws on non-integer / outside 1..65535; never a fallback), `f = 600 + ((senderId − 1) mod 24) × 25` Hz — band [600, 1175] Hz (< one octave, Opus-voip-survivable), 25 Hz steps (≥ 4 bins at fftSize 8192/48 kHz). Collisions mod 24 are inevitable; own `toneHz` is published on the bus so the harness reads expected tones (no formula restated in e2e) and treats a cohort collision as a precondition failure. Synthesis `createTestToneCapture` in `media/setup/capture.ts` (the capture seam): `AudioContext` → `OscillatorNode` → `MediaStreamAudioDestinationNode` → track → the SAME `MediaStreamTrackProcessor` pump as the microphone; `deviceId` ignored. ONE substitution point: `MeetingSession.startMedia` picks the capture factory `if (__DT_TEST_TONE__)` (default factory only; an injected factory is respected) — `senderId` is already fail-closed there (no tone before MC assigns; a reissued id is a new join → new session). Nothing downstream knows the mode; tone frames take the identical encode → seal → transport path. The `'test_tone'` literal and `createOscillator` exist only inside that gate; the mode vocabulary is a TYPE (`MediaCaptureSourceMode = 'microphone' | 'test_tone'`) in `mediaMetrics.ts`, not a const object, so a prod bundle cannot spell it (deliberate departure from the `MEDIA_MUTE_ACTIONS` shape). dev-web.sh: header "does not switch it on" → `DT_TEST_TONE=1 scripts/dev-web.sh` (env reaches vite.config at server start); Playwright `webServer` inherits env — task 14 sets it.

**Metrics** (both via `mediaMetrics.ts`, base allow-list labels only, no sender/slot label):
- `dt_client_media_capture_source{mode}` gauge, value 1, only the active mode's series; set at pipeline start and RE-SET on every export-interval tick (delta/last-value gauges export only when recorded), stops at teardown so a finished session does not pin it.
- `dt_client_media_receive_source_deficit_total` counter, `add(0)` at start; one tick per export interval (interval read from a new `getMetricExportIntervalMs()` in `telemetryConfig.ts` = configured value ?? `DEFAULT_METRIC_EXPORT_INTERVAL_MS`; injectable timers). Per tick adds the number of ACTIVE assignments (declared slot, `senderId` present, `slotState === 'active'`) that were already active at the previous tick (grace: one full interval) and whose sender lane DECODED zero frames (decoder output count, per lane, monotone) in the interval. `slotState` threaded into `ReceiveAssignment`. Unit "assignment-intervals"; `rate(...) × interval_s` = mean silent active assignments; premise DTX off.
- `dt_client_media_receive_slots_rejected_total` (Q1-A) counter.
All `Exported: yes`; deficit + rejected need sites 4+5 only (no new key).

**Six-site `mode` wiring (atomic)**: (1) GC `MEDIA_DATAPOINT_EXTRA` `[&str; 6]` + `mode`, numeral-free prose, bounded-enum paragraph names `mediaMetrics.ts`, tests renamed `media_keys_*`, fixtures derive counts from `MEDIA_DATAPOINT_EXTRA.len()` and assert fixture keyset == const, `..._has_exactly_6_keys`; dt-guard `extra.len() < 5` left as-is (a minimum; guard-completeness task owns it); (2) collector `keep_keys` + "eight keys" comment, group 2; (3) `set(attributes["mode"], "invalid") where attributes["mode"] != nil and (not IsString(attributes["mode"]) or not IsMatch(attributes["mode"], "^[a-z_]{1,64}$"))`; (4) name allowlist + recompute the `max_streams` worst-case figure; (5) `client.md` entries for all three metrics with `Exported: yes`, header counts; (6) `label-taxonomy.md` `mode` row (datapoint-only, closed enum, build-time, identity-free). Either half deploys inert (GC-only: collector drops `mode`; collector-only: GC strips it; allowlist-only: series without `mode` never emitted) — revert as one unit. Panels in `infra/grafana/dashboards/client-media.json`: `sum by(mode)(dt_client_media_capture_source)` and deficit rate. No alert (stated). Runbook `docs/runbooks/client-dev-local.md`: "client N above MC cap / hears nobody" and "rising receive-source deficit" (→ MC routing, MH datagram-drop scenarios), cross-link MC capability-rejection counter.

**Test bus (R-30)** — opt-in `MeetingSessionOptions.receiveVerification: true`, set by web-app ONLY inside `if (__E2E_HOOKS__)`; absent = one untaken branch per frame in prod. When on, `IngressPipeline` keeps bounded numeric-keyed counters: layer 1 `(relay stream_id, key-id sender)` counted once the key id parses (pre-verify; stream_id observed, never decision input — same quarantine as the hop monitor, whose baseline state is different in kind so not reused); layer 2 same key after `openVerifiedFrame` succeeds (beside `frameAccepted`); drops `(stream_id, sender|unattributed, RejectReason)` beside each `frameDropped`. Undeclared stream_id → one `undeclared` bucket; hard entry cap with a counted overflow bucket. Exposed as a sampled snapshot getter (plain numbers/strings). Layer 3: seam `PlaybackSink.openLane(senderId)`, scheduled sink gains a per-lane unity `GainNode` bus exposed as `PlaybackLane.output`; web-app (E2E-gated) wraps the now-exported `createAudioContextPlaybackSink` and attaches a native `AnalyserNode` per lane (pre-mix, post-decode), sampled on the existing 250 ms tick into `{slotId, senderId, level, ready/windows, sampleRateHz, fftSize, binHz, bandStartHz, spectrum[≤128 dB magnitudes]}` — spectrum shape per test's option (2), pending security; fallback is dominant bin + level. No detector logic in task 13 (task 14, test's, in `e2e/`, imports neither `testTone.ts` nor `capture.ts`). Bus additions, all projections: `streamAssignments` (slotId, senderId, slotState — expected sender per slot), `captureSource` (mode, own toneHz in tone builds), `receiveVerification` samples, `receiveSlots` (declared, source, serverCap). Own `senderId` already on `joined`. Bus holds no session object reference beyond the existing getter shape; does not import `mediaMetrics`.

**Grandfathered-roster test** (`sdk-core/src/__tests__/joinLabelSpread.test.ts`): static scan of non-test sdk-core src; files spreading the join bag (`...this.#metricLabels` / `...metricLabels`) may name only `dt_client_*` literals on the roster parsed from `client.md`'s "Grandfathered member" table (located via `repoRoot.ts`). Controls: roster non-empty and contains `dt_client_mh_connection_total` (distinct failure message), ≥1 spread site found, negative fixture string proves a non-roster name reds. `stripComments` extracted to one shared test helper; the two existing copies switched to it.

**Bundle proof** — extend `web-app/tests/bundle-content.test.ts` (no second test): FORBIDDEN += `createOscillator` (grep-confirmed absent from prod code), `test_tone`, `__DT_TEST_TONE__`, and the bus's new event-type strings that exist only inside the `__E2E_HOOKS__` block (chosen by the file's own token rule; SDK option names are NOT markers). Positive control: a second build (`--mode development`, `DT_TEST_TONE=1`, separate outDir) must CONTAIN every non-gate-identifier marker, so a misspelled or minifier-renamed marker cannot pass vacuously. Plus a config test that `DT_TEST_TONE=1` with production mode throws.


### Gate-1 amendments accepted
- security A1: sdk-core `tests/bundle-content.test.ts` FORBIDDEN_TOKENS += `__DT_TEST_TONE__`, `createOscillator`, `test_tone`, `createMediaStreamDestination`.
- security A2: unit tests reject sender id 0 and non-integers in `testToneFrequencyHz`.
- security A3: every vitest `__DT_TEST_TONE__` define is `false`; structural test that the default capture factory is the microphone with the flag off.
- security (a): AnalyserNode only in E2E builds AND only when `receiveVerification` is on; dB magnitude only, ≤550–1250 Hz, ≤120 bins, 250 ms; bus-only (never metric/log/span/persisted); web-app FORBIDDEN += `createAnalyser`, `getFloatFrequencyData` with positive control.
- observability: rejected counter `add(0)` at setup, own panel, catalog says deficit is blind to whole-declaration rejection; capture_source catalog notes presence is judged over the lookback window (timer/reader phase); `max_streams` recompute shows arithmetic (`mode` ×2 on capture_source only); negative roster fixture reds for a real violation, distinct messages for empty-roster vs violation.
- operations: the bus exposes the active capture mode (`captureSource` event) so task 14 asserts tone mode rather than assuming it (Playwright `reuseExistingServer`); one-line comment at the webServer block.
- test 1: roster test PRIMARY check is behavioural (recording sink over join → startMedia → leave: every emission carrying `meeting_id_hash` has a roster name; controls: ≥1 such emission, non-empty roster with distinct message, negative non-roster emission through the same helper); static scan is secondary; residual (name via variable) commented.
- test 2: production build with `DT_TEST_TONE=1` must FAIL at the gate (asserted); the env-unset prod build keeps the forbidden-token scan; dev + `DT_TEST_TONE=1` is the positive control.
- test 3: AnalyserNode `smoothingTimeConstant = 0`, explicit `minDecibels`/`maxDecibels`; record carries `smoothingTimeConstant`, `fftSize`, `binHz`, `bandStartHz`, `sampleRateHz`; spectrum is `getFloatFrequencyData` dB.
- test 4: with the opt-in off the ingress holds no counter object and the snapshot getter returns `undefined` (unit-tested); overflow test asserts the bucket increments at cap and no new key is created past it.
- code-reviewer 1: receive verification is an INJECTED recorder (`MeetingSessionOptions.receiveVerification?: ReceiveVerificationRecorder`), constructed by web-app inside `if (__E2E_HOOKS__)`; ingress calls `this.#verify?.…` and owns no test state (supersedes the boolean opt-in above; the "off" test asserts no recorder → no state, getter `undefined`).
- code-reviewer 2: `getMetricExportIntervalMs()` is the ONE function the OTel reader is constructed from and the deficit tick reads.
- code-reviewer 3: deficit decision is a pure function (previous snapshot, current snapshot) → count; timer + emission wrap it.
- code-reviewer 4: `testTone.ts` named constants (band start, step, bucket count); band end derived.
- code-reviewer 5: capture-factory selection and slot-list/cap check are private helpers, not inlined into `startMedia`.
- code-reviewer 6 + test: ONE `DT_TEST_TONE` parser in `packages/web-app/vite/testTone.ts` feeds both the define and the production throw; accepts only unset/`''`/`1`, throws on anything else (e.g. `true`) — pinned by a test.
- dry A: `maxDecodeLanes` ≥ N relation stated at both definitions and enforced in `validateMediaConfig`; dry B: comment at the egress uplink `streamId` site.
- protocol (Q1-A): `optional uint32 max_receive_slots = 12` (tags 1,3,4,6–11 used; 2,5 reserved). Amendments: (1) a PRESENT 0 is a contract violation, NOT absent — SDK logs WARN with the value, diagnostic `serverCap` shows `{ state: 'invalid', value: 0 }` (distinct from `unknown`), then falls through to the cap-unknown path (MC's rejection is the backstop); tested; (2) the cap bounds the TOTAL slot count of one `ReceiveCapability` across media kinds — the SDK compares the total declared count; (3) no restated 1..=64 — ANCHOR to `crates/mc-service/src/config.rs::{MIN,MAX}_RECEIVE_SLOTS`, client hardcodes no 64; (4) per-session value from the latest JoinResponse, no caching across joins; (5) the local check is a UX pre-check that never replaces MC's; refuse, never shrink N; (6) bounded config value: log lines only, never a metric label; (7) the proto doc states MC ALWAYS sets it, so absent = an older MC; wire value `u32::from(u8)`, infallible (supersedes `u32::try_from`). Files: signaling.proto (+ stale `ReceiveCapability.slots` "Enforcement owner: story task 14" comment → pointer), `crates/proto-gen/src/lib.rs` redacting `Debug` gains the field in the clear + test, `crates/proto-gen/tests/signaling_roundtrip.rs` (Some(n), absent→None, present 0→Some(0)), `packages/proto-gen/scripts/verify-codegen.sh` presence pin for `maxReceiveSlots`, MC `build_join_response` param from the same `ClientMediaConfig::max_receive_slots` via `u32::from(u8)` + MC test, `docs/API_CONTRACTS.md` JoinResponse row + normative note + ReceiveCapability line; §Validation records an ignore-free `buf breaking` against `dfd39c66` (0 findings) WITH a positive control (exit 100 on a field deletion) and `buf lint`.
- meeting-controller (Q1-A): (1) conversion is TOTAL — the cap is `u8` end to end (`config.rs` MIN/MAX + `Config.max_receive_slots`, `ClientMediaConfig.max_receive_slots`, `capability.rs` compares `usize::from`, `metrics.rs::set_receive_slot_cap(u8)` via `f64::from` dropping the cast `expect`), wire `Some(u32::from(..))`; no None-on-overflow branch; (2) `build_join_response(&join_result, &client_media_config)`, comment at the assignment; capability check / rejection path / `slot_count_over_cap` unchanged; (3) advertised == enforced over the real wire: `tests/common/media_session.rs` captures `max_receive_slots`, `the_slot_cap_rejects_one_over_and_accepts_exactly_at` asserts and uses it as the boundary, `rejects_a_slot_count_over_a_lowered_configured_cap` asserts `Some(2)` (positive control); (4) proto-gen redacting `Debug` gains the field; (5) MC docs: `docs/observability/metrics/mc-service.md` (~900, ~646), `docs/runbooks/mc-incident-response.md` (~1976, ~2841: current SDK refuses locally → `dt_client_media_receive_slots_rejected_total`; MC `slot_count_over_cap` = pre-field SDK or ignoring client), `docs/runbooks/mc-deployment.md` skew line (additive, no roll order), `docs/specialist-knowledge/meeting-controller/INDEX.md`.

### Planned files

The complete change set (mirrors the classification table, which the scope-drift guard reads):

- `crates/gc-service/src/services/telemetry_filter.rs`
- `crates/mc-service/src/config.rs`
- `crates/mc-service/src/media_signaling/capability.rs`
- `crates/mc-service/src/media_signaling/mod.rs`
- `crates/mc-service/src/observability/metrics.rs`
- `crates/mc-service/src/webtransport/connection.rs`
- `crates/mc-service/tests/common/media_session.rs`
- `crates/mc-service/tests/media_client_signaling_integration.rs`
- `crates/mc-service/tests/slot_placement_integration.rs`
- `crates/proto-gen/src/lib.rs`
- `crates/proto-gen/tests/signaling_roundtrip.rs`
- `docs/API_CONTRACTS.md`
- `docs/observability/label-taxonomy.md`
- `docs/observability/metrics/client.md`
- `docs/observability/metrics/mc-service.md`
- `docs/runbooks/client-dev-local.md`
- `docs/runbooks/mc-deployment.md`
- `docs/runbooks/mc-incident-response.md`
- `docs/specialist-knowledge/client/INDEX.md`
- `docs/specialist-knowledge/meeting-controller/INDEX.md`
- `infra/grafana/dashboards/client-media.json`
- `infra/services/otel-collector/collector.yaml`
- `package.json`
- `packages/proto-gen/scripts/verify-codegen.sh`
- `packages/sdk-core/src/__tests__/joinLabelRoster.ts`
- `packages/sdk-core/src/__tests__/joinLabelSpread.test.ts`
- `packages/sdk-core/src/__tests__/serverMessageSinkScan.test.ts`
- `packages/sdk-core/src/__tests__/sourceFiles.ts`
- `packages/sdk-core/src/__tests__/stripComments.ts`
- `packages/sdk-core/src/config/__tests__/clientConfig.test.ts`
- `packages/sdk-core/src/config/clientConfig.ts`
- `packages/sdk-core/src/errors/SignalingError.ts`
- `packages/sdk-core/src/globals.d.ts`
- `packages/sdk-core/src/index.ts`
- `packages/sdk-core/src/media/__tests__/helpers.ts`
- `packages/sdk-core/src/media/__tests__/hotPathLayout.test.ts`
- `packages/sdk-core/src/media/lifecycle/AudioPipeline.ts`
- `packages/sdk-core/src/media/lifecycle/__tests__/audioPipeline.degradation.test.ts`
- `packages/sdk-core/src/media/lifecycle/__tests__/audioPipeline.deviceAndCounts.test.ts`
- `packages/sdk-core/src/media/lifecycle/__tests__/audioPipeline.intervalSignals.test.ts`
- `packages/sdk-core/src/media/lifecycle/__tests__/audioPipeline.multiSender.test.ts`
- `packages/sdk-core/src/media/lifecycle/__tests__/audioPipeline.test.ts`
- `packages/sdk-core/src/media/lifecycle/__tests__/receiveSourceDeficit.test.ts`
- `packages/sdk-core/src/media/lifecycle/intervalSignals.ts`
- `packages/sdk-core/src/media/lifecycle/receiveSourceDeficit.ts`
- `packages/sdk-core/src/media/pipeline/__tests__/ingress.verification.test.ts`
- `packages/sdk-core/src/media/pipeline/__tests__/playbackSink.test.ts`
- `packages/sdk-core/src/media/pipeline/__tests__/receiveLanes.test.ts`
- `packages/sdk-core/src/media/pipeline/__tests__/receiveVerification.test.ts`
- `packages/sdk-core/src/media/pipeline/ingress.ts`
- `packages/sdk-core/src/media/pipeline/playbackSink.ts`
- `packages/sdk-core/src/media/pipeline/receiveLanes.ts`
- `packages/sdk-core/src/media/pipeline/receiveVerification.ts`
- `packages/sdk-core/src/media/setup/__tests__/mediaMetrics.test.ts`
- `packages/sdk-core/src/media/setup/__tests__/testTone.test.ts`
- `packages/sdk-core/src/media/setup/capture.ts`
- `packages/sdk-core/src/media/setup/mediaMetrics.ts`
- `packages/sdk-core/src/media/setup/seams.ts`
- `packages/sdk-core/src/media/setup/testTone.ts`
- `packages/sdk-core/src/session/MeetingSession.ts`
- `packages/sdk-core/src/session/__tests__/meeting-session.test.ts`
- `packages/sdk-core/src/session/events.ts`
- `packages/sdk-core/src/session/mediaSelection.ts`
- `packages/sdk-core/src/session/mediaWiring.ts`
- `packages/sdk-core/src/signaling/SignalingClient.ts`
- `packages/sdk-core/src/signaling/__tests__/helpers.ts`
- `packages/sdk-core/src/signaling/__tests__/signaling-client.test.ts`
- `packages/sdk-core/src/signaling/errorCodeMap.ts`
- `packages/sdk-core/src/signaling/events.ts`
- `packages/sdk-core/src/telemetry/telemetryConfig.ts`
- `packages/sdk-core/tests/bundle-content.test.ts`
- `packages/sdk-core/tests/toneMarkers.ts`
- `packages/sdk-core/vite.config.ts`
- `packages/sdk-core/vitest.config.ts`
- `packages/sdk-svelte/src/globals.d.ts`
- `packages/sdk-svelte/vitest.config.ts`
- `packages/test-utils/src/media/index.ts`
- `packages/web-app/playwright.config.ts`
- `packages/web-app/src/__tests__/appShell.test.ts`
- `packages/web-app/src/__tests__/authViews.test.ts`
- `packages/web-app/src/__tests__/createMeeting.test.ts`
- `packages/web-app/src/__tests__/e2eAnalysis.test.ts`
- `packages/web-app/src/__tests__/e2eBus.test.ts`
- `packages/web-app/src/__tests__/errorText.test.ts`
- `packages/web-app/src/__tests__/helpers/MockMeetingSession.ts`
- `packages/web-app/src/__tests__/joinMeeting.test.ts`
- `packages/web-app/src/__tests__/receiveSlotsConfig.test.ts`
- `packages/web-app/src/globals.d.ts`
- `packages/web-app/src/lib/config.ts`
- `packages/web-app/src/lib/e2eAnalysis.ts`
- `packages/web-app/src/lib/e2eBus.ts`
- `packages/web-app/src/lib/errorText.ts`
- `packages/web-app/src/lib/session.ts`
- `packages/web-app/src/views/JoinMeeting.svelte`
- `packages/web-app/tests/bundle-content.test.ts`
- `packages/web-app/tests/testToneDefine.test.ts`
- `packages/web-app/vite-env.d.ts`
- `packages/web-app/vite.config.ts`
- `packages/web-app/vite/testTone.ts`
- `packages/web-app/vitest.config.ts`
- `pnpm-lock.yaml`
- `proto/dark_tower/signaling/v1/signaling.proto`
- `scripts/dev-web.sh`
- `scripts/dev-web.test.sh`

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
| Global Controller (paired) | confirmed |
| Protocol (paired, Q1-A GSA) | confirmed |
| Meeting Controller (paired, Q1-A) | confirmed |

**Lead ruling Q1 = A** (2026-09-30): additive `JoinResponse.max_receive_slots`. The task text requires the server cap on the SDK diagnostic surface; no typed path exists without a wire field; story line 17's "client↔MC proto already complete" premise is corrected for this one quantity. GSA ⇒ protocol + security + meeting-controller in planning and review.

---

## Implementation Summary

**Wire (Q1-A).** `JoinResponse.max_receive_slots = 12` (`optional uint32`), canonical proto comment (advisory, total slots across kinds, per session, MC always sets it, absent = unknown, present 0 = loud violation, never a metric label); stale `ReceiveCapability.slots` "Enforcement owner: story task 14" pointer replaced. MC fills it in `build_join_response(&join_result, &client_media_config)` from the one `ClientMediaConfig::max_receive_slots`; the cap is `u8` end to end (config, `ClientMediaConfig`, `capability.rs` via `usize::from`, `set_receive_slot_cap` via `f64::from` — the cast `expect` is gone), wire `Some(u32::from(..))`. Proto-gen redacting `Debug` shows it; roundtrip tests for value / absent / present 0 / Debug; `verify-codegen.sh` presence pin. MC wire tests assert advertised == enforced over the real wire (boundary driven FROM the advertised value; lowered-cap `Some(2)` positive control). No separate `build_join_response` unit test was added (building a `JoinResult` fixture in-crate is heavy); the wire tests are the coverage, which meeting-controller accepted at review. MC catalog/runbooks/deployment/INDEX updated.

**N.** `ReceiveConfig.audioSlots` (SDK default 1), `parseReceiveSlots` (strict `^[1-9][0-9]*$`, throws), `validateMediaConfig` ceiling `audioSlots <= maxDecodeLanes`. `MeetingSession`: `receiveSlots` option, `receiveSlots` diagnostics getter (`declared`, `source`, `serverCap` known/unknown/invalid), `receiveSlotIdsToDeclare` (0..N-1; over a KNOWN cap → `SignalingError(RECEIVE_SLOTS_OVER_CAP)` naming both numbers, `dt_client_media_receive_slots_rejected_total` +1, nothing declared; `invalid` → console WARN then proceed; one `[dt-media-slots]` info line, numbers only). `DEFAULT_AUDIO_SLOT_ID` and `StartMediaOptions.slotId` deleted. web-app `config.ts` parses `VITE_DT_RECEIVE_SLOTS` at load; `errorText` names `VITE_DT_RECEIVE_SLOTS` / `MC_MAX_RECEIVE_SLOTS` for the over-cap error.

**Test tone.** `__DT_TEST_TONE__` from ONE predicate `packages/web-app/vite/testTone.ts:resolveTestTone` (unset/'' → false; '1' → true in dev; '1' in production and any other value THROW); sdk-core lib build hard `false`; `false` in every vitest config; declared in all three `globals.d.ts`. `media/setup/testTone.ts` (sender id only, `600 + ((id-1) mod 24) * 25` Hz, named constants, derived band end, throws on 0/non-integer/out of range). `createTestToneCapture` at the capture seam (oscillator → `MediaStreamAudioDestinationNode` → same processor pump). One selection site `session/mediaSelection.ts:selectCaptureSource` inside `if (__DT_TEST_TONE__)`; injected factories respected; `MICROPHONE_SOURCE` is the one `microphone` literal. (`classifySlotCap` / `receiveSlotIdsToDeclare` live beside it; the session's send-directive listener moved verbatim to `session/mediaWiring.ts:sendDirectiveListener`, and the interval signals to `media/lifecycle/intervalSignals.ts`, so `MeetingSession` and `AudioPipeline` stay inside `dt-guard ts-retained-credentials`' declaration-size bound — 3x margin under `MAX_DECL_BLOCK_LINES`; split, not a raised cap.) `MeetingSession.captureSource` getter (mode, toneHz in tone builds).

**Metrics.** `dt_client_media_capture_source{mode}` (presence gauge, value 1, set at start and re-set every export-interval tick, stops at teardown), `dt_client_media_receive_source_deficit_total` (add(0) at start; pure `receiveSourceDeficit(prev, curr)` with the one-interval grace rule over ACTIVE declared assignments and per-lane decode epochs/counts), `dt_client_media_receive_slots_rejected_total` (add(0) at startMedia). One pipeline timer at `getMetricExportIntervalMs()` — the value the OTel reader is built from. `ReceiveAssignment.active` from `slotState === 'active'`. `MediaCaptureSourceMode` is a TYPE (deliberate departure from the const-object shape so prod cannot spell `test_tone`).

**Six `mode` sites (atomic).** (1) GC `MEDIA_DATAPOINT_EXTRA: [&str; 6]` + `mode`, numeral-free prose, bounded-enum paragraph names `mediaMetrics.ts`, tests `media_keys_*` with fixture keyset == const and derived drop counts, `..._has_exactly_6_keys`; (2) collector `keep_keys` + `mode` (eight keys; hand-verified ⊆ GC `ALLOWLIST ∪ MEDIA_DATAPOINT_EXTRA`, and `dt-guard client-metrics-export` G4 green); (3) `mode` shape statement verbatim in form, both anchors; (4) three names in the metric-name allowlist + `max_streams` arithmetic (47 → 51); (5) `client.md` entries for all three (Exported: yes) + header counts 23 of 28; (6) `label-taxonomy.md` `mode` row.

**Bus (R-30).** Injected `BoundedReceiveVerificationRecorder` (numeric composite keys, `undeclared`/`unparsed` buckets, hard cap 256 + counted overflow, unknown reason → overflow never misfiled; `REASON_SPACE` derived from `ALL_REJECT_REASONS.length`); ingress records layer 1 once the key id parses (observed `stream_id`, never a decision input), layer 2 beside `frameAccepted`, drops beside every `frameDropped`. `PlaybackSink.openLane(senderId)` + per-lane unity `GainNode` exposed as `PlaybackLane.output`; `createAudioContextPlaybackSink` exported. web-app `lib/e2eAnalysis.ts` (all inside `if (__E2E_HOOKS__)`): recorder + wrapped playback factory with a per-lane native `AnalyserNode` (fftSize 8192, smoothing 0, explicit min/max dB, band `E2E_ANALYSIS_BAND_{START,END}_HZ` = 560–1200 Hz fitting ≤ 120 bins at 44.1 and 48 kHz (attach THROWS at a rate where it would not; `bandEndHz` reported), rounded dB, RMS level, `ready` = a full FFT window since the lane's FIRST decoded frame; a lane with no output node THROWS) — created only together with the recorder. Bus events: `receiveSlots`, `slotAssignments` (expected sender per slot), `captureSource` (once), `receiveLayers`, `receiveAnalysis` (sampled on the 250 ms tick). No detector logic (task 14).

**Grandfathered roster.** Behavioural primary (recording sink over join → startMedia → leave; bag-live and media-ran controls; negative non-roster emission) + static secondary (spread-file literal scan; roster non-empty, spread sites found, negative fixture). `stripComments` extracted to one helper, both existing copies switched.

**Bundles.** web-app FORBIDDEN += `__DT_TEST_TONE__`, `test_tone`, `createOscillator`, `createMediaStreamDestination`, `createAnalyser`, `getFloatFrequencyData`, `receiveLayers`, `receiveAnalysis`; positive-control build (dev + `DT_TEST_TONE=1`, separate outDir) must contain every non-gate marker; a production build with `DT_TEST_TONE=1` must FAIL with the gate's message. sdk-core: tone markers forbidden in the runtime artifacts (`.mjs`/`.cjs`/`.map`); `.d.ts` legitimately carries the public mode type.

**Ops/docs.** Dashboard panels (capture source by mode, deficit, refusals), runbook F18/F19 (F16/F17 left to story 2's runbook task), dev-web.sh header names `DT_TEST_TONE=1 scripts/dev-web.sh`, Playwright webServer comment (assert `captureSource.mode`, never assume), API_CONTRACTS JoinResponse rows (also the missing field 11), both INDEX files.

## Validation

**Ignore-free `buf breaking` (signaling.proto is under `breaking.ignore`, so Layer 6 alone proves nothing).** Config derived from `proto/buf.yaml` with the precedent's awk; post-conditions: `- FILE` present, no `.proto` suppression entry survives.

| Run | rc | Findings |
|---|---|---|
| ignore-free, tree vs `dfd39c66` (the RESULT) | 0 | none — additive only |
| POSITIVE CONTROL: scratch copy with `JoinResponse.binding_token = 7` deleted, ignore-free, vs `dfd39c66` | 100 | `Previously present field "7" with name "binding_token" on message "JoinResponse" was deleted.` |
| same scratch copy, REAL repo config | 0 | none — shows the carve-out suppresses exactly this |
| `buf lint` (STANDARD) | 0 | none |

Hand read of the proto diff: one added `optional` field at a fresh tag, comment-only edit elsewhere; nothing renumbered, removed or retyped.

---

## Gate 2 — Full Validation (Lead)

`DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` on the final tree, attempt 1: L1 OK, L2 OK, L3 OK, L4 N/A (proto has no test verb; rust+ts OK), L5 OK, L6 N/A (proto audit placeholder; cargo/pnpm audit + buf-breaking OK), L7 OK (`env-tests-passed`, `browser-e2e-passed`). TOTAL_RESULT=N/A, exit 0, 1182 s.

## Code Review Results

### Gate 3 Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 2 | 2 | 0 | also GSA reviewer for proto/MC (Q1-A) and GC edit |
| Test | RESOLVED-FIXED | 9 | 9 | 0 | |
| Observability | RESOLVED-FIXED | 1 | 1 | 0 | |
| Code Quality | RESOLVED-FIXED | 7 | 7 | 0 | |
| DRY | RESOLVED-FIXED | 7 | 7 | 0 | no extraction opportunities / TODO entries |
| Operations | RESOLVED-FIXED | 2 | 2 | 0 | |
| Semantic Guard | RESOLVED-FIXED | 1 | 1 | 0 | native SAFE; 0 checks.md findings, 1 comment-drift item |
| Global Controller (paired) | RESOLVED-FIXED | 1 | 1 | 0 | Ownership Lens: Domain-judgment, owner in plan+review |
| Protocol (paired, GSA) | CLEAR | 0 | 0 | 0 | ignore-free `buf breaking` vs dfd39c66 reproduced: 0 findings |
| Meeting Controller (paired) | CLEAR | 0 | 0 | 0 | |

Gate-3 findings, all FIXED in tree (no deferrals):

| Reviewer | Finding | Fix |
|---|---|---|
| operations OPS-1 | u8 retype made `MC_MAX_RECEIVE_SLOTS=256` report "must be a non-negative integer" | `bounded()` parse error now names the legal range; test for 256 → "1..=64" |
| operations OPS-2 | dev-web never shows whether the tone is on | preflight prints `test tone: ON requested (DT_TEST_TONE=...)` / `OFF (DT_TEST_TONE unset)`, not re-validated in bash; both covered in `dev-web.test.sh` (run_check made hermetic for `DT_TEST_TONE`) |
| semantic-guard / security F1 | four comments pointed the tone gate at `MeetingSession.ts` | repointed to `session/mediaSelection.ts:selectCaptureSource` |
| security F2 | N ceiling checked after N was used | `validateMediaConfig` in the `MeetingSession` constructor; test: N = maxDecodeLanes+1 throws `ClientConfigError` before any dial |
| observability | `mode=microphone` also covers an injected capture factory, undocumented | catalog bullet + taxonomy values cell |
| code-reviewer 1 | stale "timer lives in AudioPipeline" | points at `intervalSignals.ts` |
| code-reviewer 2 | `declareReceiveSlots` declares nothing | renamed `receiveSlotIdsToDeclare` |
| code-reviewer 3 / DRY / security note | `REASON_SPACE = 64` restated the vocabulary size | derived from `ALL_REJECT_REASONS.length` |
| code-reviewer 4 | `overflow` doc covered one cause | covers both |
| code-reviewer 5 / test 3 / DRY F4 | analysis band silently clipped by the bin cap | band 560–1200 fits at 44.1/48 kHz, `bandEndHz` emitted, attach throws where it cannot fit; bus comment names the constants |
| code-reviewer 6 | full path for `ClientMediaConfig` | imported |
| code-reviewer 7 / DRY F3 | tone capture copied the mic track lifecycle | one `trackCapture()` helper owns reader/track registration and teardown order |
| DRY F1 | third copy of the source walker | `src/__tests__/sourceFiles.ts` (skip dirs + `.d.ts` handling as parameters), three callers switched |
| DRY F2 | tone markers encoded in two bundle tests | one list `packages/sdk-core/tests/toneMarkers.ts`; web-app asserts FORBIDDEN ⊇ it |
| DRY F5 | "exactly eight keys" restated a count | numeral dropped |
| DRY F6 / test 1(b) | nothing checked the analysis band contains the tone band | `e2eAnalysis.test.ts` asserts the EMITTED band covers 600..1175 ± 3 bins at 44.1 and 48 kHz (SDK constants read from source in the test only) |
| DRY F7 | `ANCHOR (DRY):` on a pointer with no copy | reworded to a plain pointer |
| test 1 | `e2eAnalysis.ts` untested | fake-node unit test: explicit analyser config, band/bin mapping, level (-3 dBFS sine, -Infinity silence), ready, close, no-output throw |
| test 2 | `ready` measured from lane open (eager) | from the lane's first decoded frame |
| test 4 | lane without output silently skipped | throws (broken harness) |
| test 5 | no pipeline proof decode activity prevents a tick | multiSender rig: real frames between ticks keep deficit 0; silence then counts 1 |
| test 6 | exclusion tests vacuous if counting broke | positive-control active slot in each |
| test 7 | forged-key test did not pin the reason | pinned `signature_invalid` |
| test 8 | accepted-misrouting case untested | both senders assigned, B on slot 0 accepted, counted `(0, B)` keyed 1 verified 1 |
| test 9 | cap-absent backstop untested | MC `ErrorMessage` after the declaration surfaces as session `error` with `serverCode INVALID_REQUEST`, no assignments |
| paired-global-controller | comment rewrap | done |
| paired-meeting-controller / paired-protocol | none | — (no `build_join_response` unit test was written; wire tests are the coverage, accepted by meeting-controller) |

---

## Accepted Deferrals

None.

---

## Rollback Procedure

1. Start commit: `dfd39c6602498ddfeac715428c72103f53638a24`
2. `git diff dfd39c66..HEAD`
3. Safe-revert unit: the whole commit. The six `mode` export sites (GC `MEDIA_DATAPOINT_EXTRA`, collector `keep_keys`, `mode` shape statement, metric-name allowlist, `client.md`, `label-taxonomy.md`) revert as ONE unit. Deploy order between GC and the collector is safe either way: GC alone forwards `mode` and the collector's `keep_keys` drops it; the collector alone keeps a key GC strips; the name allowlist alone admits series only an SDK emitting them produces — every partial state is ABSENT data, never wrong data. `JoinResponse.max_receive_slots` needs no roll order (additive; an older SDK ignores it, a newer SDK against an older MC treats the cap as unknown and MC's whole-declaration rejection stays the enforcer). The MC `u8` retype is internal to mc-service.
