# Devloop Output: Split media series by `media_kind` end to end (Story 3 OBS-A)

**Date**: 2026-10-20
**Task**: Story 3 task #2 (OBS-A) — carry `media_kind` {audio, video} on every client and MC media series whose subject has a kind (GC proxy → collector → dt-guard → SDK → MC), keep MH type-blind, prove with env-test + browser read-back
**Specialist**: observability
**Mode**: Agent Teams (v2) — full, Gate-1 present
**Branch**: `feature/user-see-each-other`
**Duration**: ~3h

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `9549780187b17b49a761992f1908d2c372518958` |
| Branch | `feature/user-see-each-other` |
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
| Security | `security` |
| Test | `test` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Semantic Guard | `semantic-guard` |
| Paired | `paired-global-controller`, `paired-infrastructure`, `paired-client`, `paired-meeting-controller` |

---

## Task Overview

### Objective
Full task text: `/home/dev/.cache/devloop/story-runs/story-runner/2026-10-07-see-each-other/task-2.prompt` (story `docs/user-stories/2026-10-07-see-each-other.md`, R-29, R-30 per-kind accounting; ADR-0036 §11).

### Scope
- **Service(s)**: gc-service, mc-service, sdk-core, web-app e2e, otel-collector, dt-guard, env-tests, docs/observability, grafana/prometheus
- **Schema**: No
- **Cross-cutting**: Yes

### Debate Decision
NOT NEEDED - governed by ADR-0036 §11 / §7 and the story plan.

---

## Cross-Boundary Classification

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `crates/common/src/observability/labels.rs` | Not mine, Minor-judgment (`MEDIA_KIND_LABEL` beside `KEY_CUSTODY_LABEL`) | common (observability policy) |
| `crates/dt-guard/src/client_metrics_export.rs` | Not mine, content-only (two `DECLARED_WRAPPERS` entries + synthetic-fixture comments; ruled content by infrastructure) | infrastructure |
| `crates/dt-guard/tests/client_metrics_export_fixtures.rs` | Not mine, Minor-judgment (synthetic fixture gains the session class + wrappers; no rule change) | infrastructure |
| `crates/env-tests/src/fixtures/metric_hygiene.rs` | Not mine, Domain-judgment (R-29 live proofs / hygiene rule) | test |
| `crates/env-tests/tests/31_gc_telemetry.rs` | Not mine, Domain-judgment (R-29 live proofs / hygiene rule) | test |
| `crates/env-tests/tests/32_media_metric_hygiene.rs` | Not mine, Minor-judgment (MH type-blind leg header + diagnostics) | test |
| `crates/env-tests/tests/36_mc_media_kind_labels.rs` | Not mine, Domain-judgment (R-29 live proofs / hygiene rule) | test |
| `crates/gc-service/src/services/telemetry_filter.rs` | Not mine, Domain-judgment (`media_kind` admitted at the browser->cluster trust boundary; datapoint tier) | global-controller (+ security) |
| `crates/gc-service/tests/telemetry_proxy_tests.rs` | Not mine, Domain-judgment (`media_kind` admitted at the browser->cluster trust boundary; datapoint tier) | global-controller (+ security) |
| `crates/mc-service/src/actors/meeting.rs` | Not mine, Domain-judgment (kind-taking recorders, per-kind counting unit, census, server-mute decision) | meeting-controller |
| `crates/mc-service/src/actors/meeting_media.rs` | Not mine, Domain-judgment (kind-taking recorders, per-kind counting unit, census, server-mute decision) | meeting-controller |
| `crates/mc-service/src/actors/messages.rs` | Not mine, Domain-judgment (kind-taking recorders, per-kind counting unit, census, server-mute decision) | meeting-controller |
| `crates/mc-service/src/media_signaling/assignments.rs` | Not mine, Domain-judgment (kind-taking recorders, per-kind counting unit, census, server-mute decision) | meeting-controller |
| `crates/mc-service/src/media_signaling/capability.rs` | Not mine, Domain-judgment (kind-taking recorders, per-kind counting unit, census, server-mute decision) | meeting-controller |
| `crates/mc-service/src/media_signaling/directive.rs` | Not mine, Domain-judgment (kind-taking recorders, per-kind counting unit, census, server-mute decision) | meeting-controller |
| `crates/mc-service/src/media_signaling/mod.rs` | Not mine, Domain-judgment (kind-taking recorders, per-kind counting unit, census, server-mute decision) | meeting-controller |
| `crates/mc-service/src/media_signaling/outcome.rs` | Not mine, Domain-judgment (kind-taking recorders, per-kind counting unit, census, server-mute decision) | meeting-controller |
| `crates/mc-service/src/observability/metrics.rs` | Not mine, Domain-judgment (kind-taking recorders, per-kind counting unit, census, server-mute decision) | meeting-controller |
| `crates/mc-service/src/webtransport/connection.rs` | Not mine, Domain-judgment (kind-taking recorders, per-kind counting unit, census, server-mute decision) | meeting-controller |
| `crates/mc-service/src/webtransport/server.rs` | Not mine, Domain-judgment (kind-taking recorders, per-kind counting unit, census, server-mute decision) | meeting-controller |
| `crates/mc-service/tests/media_client_signaling_integration.rs` | Not mine, Domain-judgment (kind-taking recorders, per-kind counting unit, census, server-mute decision) | meeting-controller |
| `crates/mc-service/tests/server_mute_integration.rs` | Not mine, Domain-judgment (kind-taking recorders, per-kind counting unit, census, server-mute decision) | meeting-controller |
| `crates/mc-service/tests/slot_placement_integration.rs` | Not mine, Domain-judgment (kind-taking recorders, per-kind counting unit, census, server-mute decision) | meeting-controller |
| `docs/TODO.md` | Mine (observability) | - |
| `docs/observability/label-taxonomy.md` | Mine (observability) | - |
| `docs/observability/metrics/client.md` | Mine (observability) | - |
| `docs/observability/metrics/mc-service.md` | Mine (observability) | - |
| `docs/observability/metrics/mh-service.md` | Mine (observability) | - |
| `docs/runbooks/client-dev-local.md` | Not mine, Minor-judgment (counting-unit and cap-selector prose) | operations |
| `docs/runbooks/mc-incident-response.md` | Not mine, Minor-judgment (counting-unit and cap-selector prose) | operations |
| `docs/specialist-knowledge/observability/INDEX.md` | Mine (observability) | - |
| `infra/grafana/dashboards/mc-media.json` | Mine (observability) | - |
| `infra/services/otel-collector/collector.yaml` | Not mine, Domain-judgment (keep_keys + exact value rule + `max_streams` figure) | infrastructure |
| `packages/sdk-core/src/index.ts` | Not mine, Domain-judgment (per-kind / session handle split on the media path) | client |
| `packages/sdk-core/src/media/MediaTransport.ts` | Not mine, Domain-judgment (per-kind / session handle split on the media path) | client |
| `packages/sdk-core/src/media/__tests__/hotPathLayout.test.ts` | Not mine, Domain-judgment (per-kind / session handle split on the media path) | client |
| `packages/sdk-core/src/media/lifecycle/AudioPipeline.ts` | Not mine, Domain-judgment (per-kind / session handle split on the media path) | client |
| `packages/sdk-core/src/media/lifecycle/__tests__/audioPipeline.multiSender.test.ts` | Not mine, Domain-judgment (per-kind / session handle split on the media path) | client |
| `packages/sdk-core/src/media/lifecycle/__tests__/audioPipeline.test.ts` | Not mine, Domain-judgment (per-kind / session handle split on the media path) | client |
| `packages/sdk-core/src/media/lifecycle/intervalSignals.ts` | Not mine, Domain-judgment (per-kind / session handle split on the media path) | client |
| `packages/sdk-core/src/media/pipeline/__tests__/egress.multiTarget.test.ts` | Not mine, Domain-judgment (per-kind / session handle split on the media path) | client |
| `packages/sdk-core/src/media/pipeline/__tests__/ingress.attribution.test.ts` | Not mine, Domain-judgment (per-kind / session handle split on the media path) | client |
| `packages/sdk-core/src/media/pipeline/__tests__/ingress.hopSender.test.ts` | Not mine, Domain-judgment (per-kind / session handle split on the media path) | client |
| `packages/sdk-core/src/media/pipeline/__tests__/ingress.rejectTokens.test.ts` | Not mine, Domain-judgment (per-kind / session handle split on the media path) | client |
| `packages/sdk-core/src/media/pipeline/__tests__/ingress.verification.test.ts` | Not mine, Domain-judgment (per-kind / session handle split on the media path) | client |
| `packages/sdk-core/src/media/pipeline/__tests__/receiveLanes.test.ts` | Not mine, Domain-judgment (per-kind / session handle split on the media path) | client |
| `packages/sdk-core/src/media/pipeline/egress.ts` | Not mine, Domain-judgment (per-kind / session handle split on the media path) | client |
| `packages/sdk-core/src/media/pipeline/ingress.ts` | Not mine, Domain-judgment (per-kind / session handle split on the media path) | client |
| `packages/sdk-core/src/media/pipeline/receiveLanes.ts` | Not mine, Domain-judgment (per-kind / session handle split on the media path) | client |
| `packages/sdk-core/src/media/setup/__tests__/measurement.test.ts` | Not mine, Domain-judgment (per-kind / session handle split on the media path) | client |
| `packages/sdk-core/src/media/setup/__tests__/mediaMetrics.test.ts` | Not mine, Domain-judgment (per-kind / session handle split on the media path) | client |
| `packages/sdk-core/src/media/setup/measurement.ts` | Not mine, Domain-judgment (per-kind / session handle split on the media path) | client |
| `packages/sdk-core/src/media/setup/mediaMetrics.ts` | Not mine, Domain-judgment (per-kind / session handle split on the media path) | client |
| `packages/sdk-core/src/session/MeetingSession.ts` | Not mine, Domain-judgment (per-kind / session handle split on the media path) | client |
| `packages/sdk-core/src/session/mediaSelection.ts` | Not mine, Domain-judgment (per-kind / session handle split on the media path) | client |
| `packages/sdk-core/src/session/mediaWiring.ts` | Not mine, Domain-judgment (per-kind / session handle split on the media path) | client |
| `packages/web-app/e2e/clientMetricNames.ts` | Not mine, Domain-judgment (R-29 browser read-back assertions) | client (+ test) |
| `packages/web-app/e2e/mcMetrics.ts` | Not mine, Domain-judgment (R-29 browser read-back assertions) | client (+ test) |
| `packages/web-app/e2e/multi-party-hear.spec.ts` | Not mine, Domain-judgment (R-29 browser read-back assertions) | client (+ test) |
| `packages/web-app/tests/clientMetricNames.test.ts` | Not mine, Domain-judgment (R-29 browser read-back assertions) | client (+ test) |
| `scripts/otel-collector/acceptance_driver.py` | Not mine, Minor-judgment (hermetic fires cases for the value rule) | infrastructure |

---

## Planning

Story/wire-freeze check: the task prompt and the story's Design §Wire freeze / Telemetry bullets agree (media_kind only new key; datapoint-only; session-level, `send_directives`, unreachable senders kind-free; MH type-blind). One in-story tension raised to the Lead (D1 below): MC-A's dependency text says "every existing call site passing audio" while the OBS-A prompt says "one increment per kind the request names".

### Vocabulary homes (SSoT = `label-taxonomy.md` media_kind row)
- Rust key: `MEDIA_KIND_LABEL = "media_kind"` in `crates/common/src/observability/labels.rs` beside `KEY_CUSTODY_LABEL`, literal pin test; doc says MH must NOT import it (ADR-0036 §7) and names what enforces it (live hygiene rule + TODO gap; nothing at source level).
- Rust values: `MediaKindLabel { Audio, Video }` in `crates/mc-service/src/media_signaling/outcome.rs`, `ALL` + exhaustive `label()`, plus ONE `from_proto(MediaKind) -> Option<Self>`: AUDIO->Audio, VIDEO_CAMERA->Video, VIDEO_SCREEN->Video (deliberate: screen share is `video` + uncarried `content_kind=slides`), UNSPECIFIED->None; no `_` arm. Distinct name from the proto `MediaKind`.
- TS: `MediaKindLabel = 'audio' | 'video'` + `MEDIA_KIND_LABELS` const in `mediaMetrics.ts` (distinct from proto `MediaKind` and from `events.ts`'s `'audio'|'video'|'other'`, which is a wire projection incl. an unknown bucket - different concept, left alone).
- Drift checks per copy: Rust unit test parses the taxonomy `| media_kind | audio, video |` row and compares with `ALL.map(label)`; TS vitest does the same against the same row; collector regex is covered behaviourally (env-test 31 video/bogus + browser audio read-back, fail closed) - a static collector check would be new guard machinery: recorded in TODO, not built. env-tests gets `MEDIA_KIND_KEY` beside `KEY_CUSTODY_KEY` with the same pin-against-labels.rs test. Each copy carries `ANCHOR (DRY):` -> taxonomy section.

### Hop 1 - GC (`telemetry_filter.rs`)
- `MEDIA_DATAPOINT_EXTRA: [&str; 7]` gains `MEDIA_KIND_LABEL` (the const, not a literal; dt-guard resolves `<const:>`). Tier/key_allowed/wrappers/`custody_value_ok`/handlers untouched (user-JWT gate, body limit, governor, PII drop, scalar-shape retain unchanged).
- Array doc + module header: closed {audio, video}, SSoT taxonomy section (new heading). GC bounds the KEY and value SHAPE (scalar) only; precedent is the reason/outcome/action/source/mode group (security S1 - NOT key_custody, which GC value-checks). State: the collector's exact rule is the only value bound and GC deliberately does not drop, so the rewrite stays observable (a GC drop would make the collector rule dead). Header notes media_kind's exact rule DOES bound distinct-value count (<= 3), unlike the charset regex. Drop the "17-key" numeral.
- Tests: rename length tripwire to 7; `media_keys()` + inline fixtures + expected pairs gain media_kind; new unit `media_kind="bogus"` survives byte-identical with datapoint drop count 0 (pins "GC bounds only the key"). Integration (`telemetry_proxy_tests.rs`): add a capture knob on `TelemetryServerOpts` (last body per path) instead of a third harness copy; one test POSTs metrics with media_kind="video" on a Sum datapoint + its exemplar, and traces with media_kind on a span, via real user JWT; asserts 202, datapoint keeps it, exemplar + span stripped; positive control that a body was captured (distinct expect message). dt-guard's `tests/client_metrics_export_fixtures.rs` `[&str; 6]` GC tree is synthetic - left alone.

### Hop 2 - collector (`collector.yaml`)
- keep_keys group 2 += `media_kind`; group comment names it, vocabulary, SSoT, corrects the "MEDIA_DATAPOINT_EXTRA minus key_custody" relation.
- Block (c), after `mode`: `set(attributes["media_kind"], "invalid") where attributes["media_kind"] != nil and (not IsString(attributes["media_kind"]) or not IsMatch(attributes["media_kind"], "^(audio|video)$"))`. The `!= nil` conjunct is what keeps kind-free session series from acquiring `media_kind="invalid"`; parentheses keep the alternation anchored. Comment: why this key is an exact allowlist when block (c) argues for shape (closed two-value domain fixed by ADR/taxonomy; a third kind is a contract change in the same task, so "allowlist falls behind" cannot happen silently).
- `max_streams` figure recomputed: honest today unchanged (audio only); honest once video emits = per-kind streams x2; adversarial = per-kind streams x3 (audio/video/invalid) per org. New headroom stated; whether 500 changes is operations' call (I will state the number and flag it).
- Hermetic fires: `scripts/otel-collector/acceptance_driver.py filtering()` gains bogus->invalid, audio survives, `xaudio`/`audio_x`->invalid, int->invalid, and a kind-free series leaves WITHOUT media_kind (nil-guard list += media_kind and the missing `mode`). Run `acceptance.sh` if the substrate allows; not layer-wired, so env-test 31 is the gating proof.

### Hop 3 - dt-guard
- G4/G6/Rule 1/Rule 2/Rule 3 must pass as-is; no resolver change. Kind-free session bag: `this.#sessionLabels = { client_version, org_id, key_custody }` object literal + wrappers `#sessionCounter` / `#sessionGauge` spreading it, registered as two `DECLARED_WRAPPERS` entries (infrastructure ruled: content). Per-kind bag stays `this.#base = mediaMetricLabels(identity, kind)` returning a spread-free 4-key literal, so Rule 1 sees `media_kind` as a resolved key. I will run `dt-guard client-metrics-export` on the SDK shape early.
- Fixture arrays ~2934/2957: SYNTHETIC parser inputs (line-wrap and unresolved-const counting, declared 5/2 against a real 6); comment "synthetic parser input, deliberately not a mirror of GC's array" - no seventh entry.

### Client SDK
- `MediaKindMetrics` (per kind; ctor `(identity, kind, sink)`, `#base = mediaMetricLabels(identity, kind)`): frames sent/received/accepted/dropped, send dropped, send queue depth, wrap outcomes, gap/reorder, undeclared stream id, decoder errors, mute transitions, decode-queue dropped, receive source deficit, receive slots rejected, time-to-first-media-frame (unit stated in client.md: "first frame of THAT kind", one sample per kind per session).
- `MediaSessionMetrics` (kind-free; ctor `(identity, sink)`): kek updates/violations/anomalies/install refusals/generations retained, roster key rebinds, capture_source. The type takes no kind, so a session series cannot be built per kind (and vice versa) - compile error.
- Container `MediaMetrics { session, audio }` built once in MeetingSession (one construction expression shared by :384 and the :563 fallback). NO video handle is constructed in this task (C-A2/C-B add it), so nothing zero-inits a `media_kind="video"` series that would pre-empt C-B's read-back. Per-kind consumers (egress, ingress, receiveLanes, measurement, mediaSelection) take `MediaKindMetrics`; kekObserver/rosterInvalidationListener take `MediaSessionMetrics`; AudioPipeline/intervalSignals take the container (captureSource session, deficit audio).
- Hot path unchanged in shape: methods on a pre-resolved handle, no names, no label objects built per frame.
- Tests: every construction site (receiveLanes, ingress.hopSender/attribution/verification/rejectTokens, egress.multiTarget, measurement, audioPipeline.* incl. intervalSignals/multiSender/deviceAndCounts/degradation, mediaMetrics.test). mediaMetrics.test iterates ALL per-kind emitters (exact 4-key set, `media_kind` = the kind, key_custody=operator) and ALL session emitters (exact 3-key set, key absent not undefined), positive control on non-empty emitter lists, negative case (ambient extra key cannot reach either), audio and video handles distinct, taxonomy-row drift test. Stale prose fixed (header "TWO STRINGS", identity doc, MediaTransport.ts:423, hotPathLayout message).

### MC
- Recorders take `MediaKindLabel` (never `&str`) and emit `MEDIA_KIND_LABEL => kind.label()`: `record_slot_state(kind, state)`, `record_send_targets(kind, count)`, `set_receive_slot_cap(kind, cap)`, `record_mute_request(kind, outcome)`, `record_server_mute_request(kind, action, outcome)`, `set_server_muted_sources(kind, total)`.
- Slot state kind is structural: `DeclaredSlot` carries the label kind computed at parse (parse already rejects UNSPECIFIED, so total there); `SlotComposition.slot_states` becomes `Vec<(MediaKindLabel, SlotState)>` - the DECLARED slot's kind, never the frame's; no fallible lookup at the recorder.
- Send targets: one pass over `directive.streams`, per-stream increment with that stream's typed kind carried out of composition (not `try_from(i32)` at the call site; if conversion is ever needed, failure is an MC defect counted/logged, never skipped). `count == 0 -> return` kept per stream. Total equals today's sum, so the "Mean Send Targets per Directive" ratio is unchanged.
- Receive slot cap: publish `audio` only; no video zero-init (0 for video will mean "R-26 lever applied" once MC-A publishes it - catalog row says MC-A makes it appear).
- Census: `MutedSourceCensus` keyed per meeting with a fixed `[u64; 2]` indexed by `MediaKindLabel`; all-zero meeting removed; `publish()` sets EVERY kind in `ALL` every time (incl. 0) from one function; std Mutex, no await held, non-blocking as today; key = meeting_id only (no participant/sender id). `publish_mute_census` counts `audio_server_muted` and `video_server_muted` separately; server.rs startup sets both kinds to 0; teardown zeroes both. Doc "how many participants" -> "server-muted (participant, kind) sources; sum across kinds for the MH-comparable reading".
- Mute counting (D1): one helper `named_kinds(audio: Option<bool>, video: Option<bool>) -> [(MediaKindLabel, bool)...]` derived ONCE at the top of each handler and passed to every exit arm. INTERIM rule while P1's shim reads absent as `false`: the shim writes BOTH kinds on every request, so both kinds are acted on and both are counted (each with the value MC applied); MC-B's switch to absent=unchanged narrows the helper to present fields and adds the both-absent rejection on the generic path. So there is no zero-increment hole and no invented kind. The SDK already sends both fields explicitly, so per-outcome rates double vs before - stated in catalog rows, dashboard descriptions and runbook so it is not read as a traffic change.
- Per-kind outcomes: request-level outcomes (`unchanged` tuple short-circuit, `rate_limited`, `actor_unavailable`, `not_permitted`, `unknown_target`) once per named kind. Client-mute success derived per kind locally: audio `applied` iff audio flag moved on a declared connection else `applied_no_recompose`/`unchanged`; video `applied_no_recompose` iff video moved else `unchanged` (video never recomposes). Server mute: `ServerMuteAction::from_muted(bool)` per kind (replaces request-level `from_request`); the actor's `ServerMuteDecision` reports per-kind changed (it already computes `audio_changed`), so each kind gets `applied`/`unchanged` honestly. Decision log keeps audio_muted/video_muted fields.
- Kind-free, documented in recorder docs AND one "kind-free and why" list in mc-service.md: `send_directives` (composed per participant before any stream exists; failure outcomes have no honest kind; kind split lives on send_targets + MC-A's withdrawals counter), `unreachable_senders` and `not_yet_connected_senders` (property of a sender vs a subscriber's handlers/connection), `unmute_requests` (D2), `slot_view_emissions`, `receive_capability_declarations`, policy-push/sender-binding/edge/settle/teardown/KEK families (no kind subject).
- Zero-init: iterate `MediaKindLabel::ALL x` existing ALLs in `zero_initialize_counters()` - mute (MuteOutcome x 2), server mute (2 x 6 x 2 = 24), slot states (SLOT_STATES x 2), send targets (x2). Cardinalities restated in docs/catalog. Every MetricAssertion in `tests/server_mute_integration.rs`, `media_client_signaling_integration.rs`, `actor_metrics_integration.rs` touching these five names gets the kind label (an absent-series `== 0` would pass vacuously).
- Tests: MetricAssertion per changed recorder (label value; per-(request, kind) unit: both-kind request +1 audio +1 video; send_targets per stream; census publishes both kinds incl. 0 after last video-muted meeting ends).

### MH - type-blind
- Live hygiene: new `Rule::MediaKindOnMh` in `check_series` (exact key match on `media_kind`, any `mh_` prefix). Fires: kernel fixtures in `metric_hygiene.rs` tests - mh_media_* with key fires; non-media mh_* with key fires; mc_media_slot_states_total{media_kind="audio"} silent; dt_client_media_frames_received_total{media_kind="audio"} silent; real-shaped clean mh series silent. Applies: 32's existing single live fetch, whose MH anchor + eager-admission presence checks are the positive control (mh_* non-empty) with their own reason token. Residual: no live mh_* series carries media_kind today, so the live leg is a regression net, not a demonstrated fire.
- Source level: `metric_labels.rs` has no per-service deny construct and `media-telemetry-deny.yaml` scopes macro forms, not label keys - expressing "key denied in mh-service only" is new machinery. Not built; `docs/TODO.md` §Guard Coverage Gaps entry, owner infrastructure (dt-guard machinery), shape: per-service label-key deny list keyed off `common/services.rs`, scoped to crates/mh-service, also covering "MH must not import MEDIA_KIND_LABEL".

### Docs
- label-taxonomy: media_kind -> carried by MC + client SDK only; delete "MH will carry"; type-blind rule with ADR-0036 §7 citation; content_kind stays declared-not-carried; slot_state bullet stays count-free -> `slot_state_label`. client.md: label set of every per-kind series, the kind-free list + reason, TTFMF unit, cardinality x2 once video emits. mc-service.md: each changed recorder's labels/cardinality/counting unit; kind-free list.
- Runbook prose (operations): mute counting unit, `applied_no_recompose` row, server-muted-sources reading, cap selectors `{media_kind="audio"}`.

### Dashboards/alerts audit (OBS-B inherits; grep of `on(`, `ignoring(`, `group_left/right`, `and/or/unless`, `by (` across mc-media.json, client-media.json, mc-alerts.yaml, client-alerts.yaml)
- No vector matching between a kind-labelled and kind-free series exists anywhere. Only `on()` is MCKekRotationStorm (`mc_meetings_active`, unaffected). `and` in slot-fill panel is same-series scalar `sum()`s.
- MCMediaMissingKeyMaterial: sum()/sum() - valid now; mixes frame rates once video emits -> OBS-B re-derives per kind.
- client-alerts KEK tripwire, mc-alerts conflicting_key / rebind: session-level, kind-free - unaffected.
- All client-media.json panels are `sum`, `sum by(reason|outcome|action|source|mode|le)` or `max` - aggregate media_kind away; unchanged meaning today (audio only); OBS-B adds per-kind rows.
- mc-media.json changes: Server-Muted Sources -> `sum by (instance, media_kind)`; both receive-slot-cap panels -> `max by (media_kind)`; Mute / Server Mute panel descriptions restate per-(request, kind) unit. Mean Send Targets per Directive: numerator total unchanged (per-stream split sums to the old value); note that a future `by (media_kind)` there needs `ignoring(media_kind)`/`on()` because directives are kind-free. Slot-fill ratio sum()/sum(): mixes kinds once MC-A adds video slots -> OBS-B.

### Proofs
- 31_gc_telemetry (cfg observability): inject on the value-0 gauge `dt_client_media_send_queue_depth` (exported, read by no alert, not frames_received) with unique per-case `client_version` markers; select by marker; video case: exactly one series with media_kind="video"; bogus case: exactly one series with media_kind="invalid" and zero with "bogus" (marker presence is the positive control); int case -> invalid.
- New `36_mc_media_kind_labels.rs` (join-driven, so not in idle-safe 32): per-instance baseline before a two-participant join (27 fixtures), assert rise on `{media_kind="audio"}` for slot_states and send_targets; same fetch: no series of those names lacks media_kind or carries a value outside {audio, video}.
- Browser: `received_audio` ClientCounter in mcMetrics.ts, value imported from the SDK's MEDIA_KIND_LABELS; baseline before joins, waitForClientCounterAbove in its own R-29 step; failure text names the hop (empty = stripped by GC/keep_keys or rewritten to invalid); clientMetricNames drift test updated if a constant is added. C-B handoff: its video read-back must be baseline->rise too. MC-A handoff (same reason): this task zero-inits `media_kind="video"` on `mc_media_slot_states_total` / `mc_media_send_targets_total`, so any later MC video read-back must be baseline->rise; presence is vacuous. In 36, the `{media_kind=""}` / out-of-domain absence checks run over the SAME fetch whose audio rise was just proven, with their own reason token distinct from the did-not-rise failure.

### ADR-0031 block
- Collector value rule - fires: env-test 31 bogus/int -> invalid in live Prometheus (+ hermetic acceptance_driver cases). Applies: the Kind overlay `infra/kubernetes/overlays/kind/services/otel-collector/kustomization.yaml` takes the base, whose content-addressed `configMapGenerator` loads `config.yaml=collector.yaml` (ADR-0038 §2); `deploy_otel_collector()` waits on rollout, so the committed file IS the deployed config and a rejected config fails loudly.
- MH hygiene rule - fires: kernel fixtures (always-on Rust lane). Applies: 32 against live Prometheus with the MH anchor as positive control. Residual stated above.

### Implementation notes carried from Gate 1
- max_streams (operations ruling): keep 500; collector comment gets per-org figures for honest-today / honest-once-video / adversarial with arithmetic + headroom, the per-kind multiplier note (C-A2/C-B recompute), and the trigger "raise the cap in the task that drops honest-once-video headroom below 3 concurrent runs". If acceptance.sh cannot run, say so in Verification Steps.
- Server-mute `action` is now the per-kind direction ({audio:true, video:false} = mute/audio + unmute/video): stated in recorder doc and catalog row; env-test 35's `action="mute"` query re-checked as baseline->rise.
- A test pins the D1 interim (`named_kinds` yields both kinds because P1's shim reads absent as false) and names the shim, so MC-B flips a failing test; `named_kinds` doc says so plainly.
- GC uses `MEDIA_KIND_LABEL` const: dt-guard `parse_gc_sets` resolves any `pub const` in labels.rs (`rust_str_consts`), verified - no extractor change.
- Per-kind `ServerMuteDecision`: the actor's whole-request `Unchanged` short-circuit (meeting.rs ~2253) stays exactly equivalent to "both kinds unchanged" (derived from the same per-kind flags, not a separate comparison).
- VIDEO_SCREEN -> video reason on the match arm; one builder for `{session, audio}` used by both MeetingSession sites; no new per-frame allocation.

### Safe-revert unit
Whole commit. GC-only or collector-only revert is safe only before video emits (afterwards audio/video datapoints collide into one series, silently wrong); MC-only revert is independent.

### Open calls for the Lead
- D1: story text conflict - MC-A dependency says "every existing call site passing audio"; OBS-A prompt says per named kind. I plan the per-named-kind rule with the interim "shim acts on both kinds" definition above (no hole, no invented kind), plus per-kind server-mute decision from the actor (small `ServerMuteDecision` change). Alternative (audio at every call site) is cheaper but leaves video-only changes labelled audio.
- D2: `mc_media_unmute_requests_total` stays kind-free in this task: `UnmuteRequest` has plain bools (no presence), its outcome (relayed / not_server_muted / no_host_connected) is decided once per request across kinds, and a both-false request is legal on the wire, so per-kind would either drop it or invent a kind. Documented as a deliberate kind-free decision; MC-B may revisit when it reshapes mute.


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
| Paired global-controller | confirmed |
| Paired infrastructure | confirmed |
| Paired client | confirmed |
| Paired meeting-controller | confirmed |

---

### Lead rulings at Gate 1

- **D1 (mute counting)**: per (request, named kind), as the OBS-A prompt specifies. Story line 1614 ("every existing call site passing audio") is MC-A's dependency summary, not Design §Wire freeze, so the task prompt's more specific counting rule governs; no wire change involved. Interim (P1 shim, both fields present) counts both kinds; pinned by a test MC-B flips. Security, MC, ops, code-reviewer concur.
- **D2**: `mc_media_unmute_requests_total` stays kind-free with the reason documented in the recorder doc and catalog.
- Classification-sanity guard: `STATUS=OK REASON=cross-boundary-classification-clean-1-files`.
- Note: reviewer prompts injected INDEX.md / review-protocol.md by reference (first-action Read) rather than inline.

---

## Pre-Work

None

---

## Implementation Summary

`media_kind` {audio, video} now rides every client and MC media series whose subject has a kind, end to end, and no MH series:

- **Hop 1 (GC)**: `MEDIA_DATAPOINT_EXTRA` = 7 keys incl. `MEDIA_KIND_LABEL` (const from `common`); datapoint-only tier unchanged; no GC value rule (pinned by `media_kind_value_is_not_checked_here`); integration test proves datapoint keeps / exemplar + span strip via the real user-JWT path (forwarded bodies read back from wiremock's request log, positive control on capture).
- **Hop 2 (collector)**: `keep_keys` group 2 += `media_kind`; exact nil-guarded `^(audio|video)$` -> `invalid` rule; `max_streams` figures recomputed (51 / 88 / 125 per org; 500 kept per operations).
- **Hop 3 (dt-guard)**: two `DECLARED_WRAPPERS` entries for the kind-free session bag (content); synthetic-fixture comments; `client-metrics-export` clean.
- **SDK**: `MediaKindMetrics` (per kind, 4-key bag via `mediaMetricLabels(identity, kind)`), `MediaSessionMetrics` (kind-free, 3-key literal through `#sessionCounter`/`#sessionGauge`), container `MediaMetrics {session, audio}` (no video handle yet); every consumer typed to the side it emits.
- **MC**: `MediaKindLabel` (+ `from_proto`), kind-taking recorders, per-stream send targets, structural slot kind, per-(meeting, kind) census, per-(request, named kind) mute counting with the interim both-kinds `named_kinds`, per-kind server-mute decision; `send_directives`, unreachable / not-yet-connected senders and unmute requests documented kind-free.
- **MH**: `Rule::MediaKindOnMh` in the live hygiene kernel (fires: unit fixtures; applies: 32); source-level gap filed in `docs/TODO.md`.
- **Docs/dashboards/runbooks**: taxonomy section rewritten (carried by MC + SDK only, MH type-blind, content_kind declared); client.md and mc-service.md label sets, counting units and kind-free lists; mh-service.md; mc-media.json (server-muted per kind, caps `max by (media_kind)`, descriptions); runbook counting-unit and cap-selector prose.
- **Proofs**: env-test 31 value rule (video intact, bogus/int -> invalid, bogus never stored, per-case markers); new env-test 36 (baseline -> rise on `media_kind="audio"` for slot states and send targets after a real join, domain check on the same fetch); browser R-29 step (audio frames_received rises from baseline, kind-less does not).

### Dashboard / alert audit (OBS-B inherits)
Grep of `on(`, `ignoring(`, `group_left/right`, `and/or/unless`, `by (` over `mc-media.json`, `client-media.json`, `mc-alerts.yaml`, `client-alerts.yaml`:
- No expression vector-matches a kind-labelled series against a kind-free one. The only `on()` is `MCKekRotationStorm` (`mc_meetings_active`, unaffected).
- `MCMediaMissingKeyMaterial`: `sum()/sum()` over `frames_dropped`/`frames_received` - valid today (audio only); mixes frame rates once video emits -> OBS-B re-derives per kind.
- KEK/roster tripwires (`client-alerts.yaml` retention violations, `mc-alerts.yaml` conflicting_key / rebind): session-level, kind-free - unaffected.
- `client-media.json`: every panel is `sum`, `sum by(reason|outcome|action|source|mode|le)` or `max` - aggregates `media_kind` away; meaning unchanged while audio-only; `max(send_queue_depth)` is last-writer per kind once video emits; TTFMF quantiles will mix kinds -> OBS-B adds per-kind rows.
- `mc-media.json`: slot-state panels `sum by (slot_state)` and the fill ratio `sum()/sum()` mix kinds once MC-A adds video slots -> OBS-B. Mute / Server Mute panels: counting unit now per (request, named kind) - descriptions updated. Server-Muted Sources -> `sum by (instance, media_kind)`. Both cap panels -> `max by (media_kind)`. Mean Send Targets per Directive: numerator total unchanged (per-stream split sums to the old value), ratio unchanged; a per-kind version needs `ignoring(media_kind) group_left` because directives are kind-free.
- Handoffs: C-B's video read-back and any MC-A MC video read-back must be baseline -> rise (video cells are zero-initialised now; synthetic env-test 31 points use `send_queue_depth` with per-case markers so they cannot satisfy a frames read-back).

---

## Files Modified

Every path is listed one per row in §Cross-Boundary Classification (the scope-drift guard's input). New file: `crates/env-tests/tests/36_mc_media_kind_labels.rs`.

---

## Devloop Verification Steps

- `./scripts/layer-fast.sh` (layers 1-6): TOTAL_RESULT=OK (first run failed only `validate-cross-boundary-scope` on multi-path table cells; table regenerated one path per row).
- `scripts/otel-collector/acceptance.sh "" filter` against the pinned collector image on the devloop Kind cluster: RESULT PASS, incl. the new `media_kind` cases (audio kept; bogus / `audio_x` / `xvideo` / int -> invalid; bogus never stored; kind-free series arrives with no `media_kind`). The pinned collector accepts the OTTL statement.
- `dt-guard client-metrics-export`: STATUS=OK REASON=client-metrics-export-clean.
- `cargo test -p gc-service` (filter unit + `telemetry_proxy_tests` 15/15), `cargo test -p mc-service -p common`, `cargo test -p env-tests --lib metric_hygiene` (29/29), clippy `-D warnings` all-features on gc-service/env-tests/mc-service/common.
- sdk-core: tsc, lint, vitest (1070), component (4), build; web-app lint + unit (225); sdk-svelte typecheck.
- NOT run here (Layer 7, the Lead's Gate 2): env-tests 31 (new value-rule case), 32, 36 and the browser `multi-party-hear.spec.ts` R-29 step - they need the deployed images.

### Gate 2 — `DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` (Lead)
- TOTAL_RESULT=OK, TOTAL_DURATION=1387s. L1-L6 OK; L7 OK (1091s): `env-tests-passed`, `browser-e2e-passed`.
- Live proofs observed passing in `layer-7-env-test.log`: `test_metrics_media_kind_value_rule_fires_and_applies` (31), all 3 tests of 32 (MH type-blind rule over the live fetch), `test_mc_slot_states_and_send_targets_carry_media_kind_audio_after_a_join` (36); browser `multi-party-hear.spec.ts` N=3 passed (17/17 specs).

---

## Code Review Results

Round 1 findings, all FIXED (none deferred); `layer-fast.sh` green after the fixes.

| Reviewer | Finding | Resolution |
|---|---|---|
| paired-global-controller | `forwarded_bodies` masked disabled wiremock recording as "GC forwarded nothing" | `.expect(...)` with a distinct reason |
| paired-infrastructure | collector comment "⊇ … exactly" self-contradictory | dropped "exactly" |
| semantic-guard / code-reviewer F2 | send-target kind re-projected from the wire i32 with a log-only `None` arm | per-stream `Vec<MediaKindLabel>` returned by `build_send_directive` (projected once from the policy's typed kind; unmappable = `unknown_stream_number`, counted); `send_stream_kind` deleted |
| code-reviewer F1 | census / decision depended on array position; three repeated kind->field matches | `MediaKindLabel::{index, pick, of, per_kind}`; `ServerMuteDecision { changed: [bool; 2] }`; bijection test replaces the order pin |
| code-reviewer F3 | mute values derived twice | read from `named` via `named_value()` |
| code-reviewer F4 / dry F3 / paired-client F1 | TS `MediaKindLabel` retyped beside the const | type derived from `MEDIA_KIND_LABELS` |
| dry F1 | literal `"media_kind"` in connection.rs tests | `MEDIA_KIND_LABEL` |
| dry F2 / test F2 | unpinned `MEDIA_KINDS` copy in env-test 36 | `MEDIA_KIND_VALUES` / `MEDIA_KIND_AUDIO` in `metric_hygiene.rs`, pinned against the taxonomy row by an always-on unit test; 36 imports them |
| test F1 | live leg did not prove the collector rule's `!= nil` guard | env-test 31 `absent` case: arrives with NO `media_kind` |
| test F3 | no-video-handle test saw only own enumerable keys | `'video' in metrics` + behavioural scan of own fields and prototype getters |
| observability F1 | three operator docs said "sum across kinds" is MH-comparable; MC tells MH only audio-muted senders | `{media_kind="audio"}` today, sum across kinds once MC-A adds video DROP_AT_INGRESS (mc-service.md, mc-media.json, runbook Step 3, code docs) |
| observability F2 | server-mute refusal WARN lost what was asked | `audio_muted` / `video_muted` from `named` |
| paired-meeting-controller F1 | `unchanged` definition stale (now also the unmoved kind of an applied request) | redefined as "this kind's state did not move", both sources named, in code docs, mc-service.md, mc-media.json and the runbook |

---

## Gate 3 — Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred |
|----------|---------|----------|-------|----------|
| Security | CLEAR | 0 | 0 | 0 |
| Test | RESOLVED-FIXED | 3 | 3 | 0 |
| Observability | RESOLVED-FIXED | 2 | 2 | 0 |
| Code Quality | RESOLVED-FIXED | 4 | 4 | 0 |
| DRY | RESOLVED-FIXED | 3 | 3 | 0 |
| Operations | CLEAR | 0 | 0 | 0 |
| Semantic Guard | RESOLVED-FIXED (SAFE) | 1 | 1 | 0 |
| Paired global-controller | RESOLVED-FIXED | 1 | 1 | 0 |
| Paired infrastructure | RESOLVED-FIXED | 1 | 1 | 0 |
| Paired client | RESOLVED-FIXED | 1 | 1 | 0 |
| Paired meeting-controller | RESOLVED-FIXED | 1 | 1 | 0 |

No accepted deferrals. The `docs/TODO.md` §Guard Coverage Gaps entry (MH source-level type-blind deny; collector regex static check) is the task-sanctioned "needs new guard machinery" record, ruled at Gate 1 by infrastructure — not a review deferral.

Fast check after round 1: `layer-fast.sh` TOTAL_RESULT=OK.

---

## Accepted Deferrals

- (none surfaced in this devloop)

---

## Rollback Procedure

1. Start commit: `9549780187b17b49a761992f1908d2c372518958`
2. `git diff 9549780187b17b49a761992f1908d2c372518958..HEAD`
3. Soft reset: `git reset --soft 9549780187b17b49a761992f1908d2c372518958`
4. **Safe-revert unit**: the whole commit. A GC-only or collector-only revert is safe only BEFORE video emits (afterwards the label is stripped and audio/video datapoints with otherwise identical label sets collide into one series - silently wrong counters). An MC-only revert is independent of the client pipeline. The SDK change is safe to revert only together with the browser spec's R-29 step.

---

## Issues Encountered & Resolutions

None yet

---

## Lessons Learned

1. A `team-lead` SendMessage address did not resolve in this harness; teammates had to route to `main`. Lead prompts should name the actual Lead address.
2. Zero-initialised series make presence checks vacuous; every new read-back (env-test 36, browser R-29, later MC-A/C-B video checks) is baseline-to-rise.
