# Devloop Output: C2 — client MH-replacement UX, receive continuity, R-42 telemetry

**Date**: 2026-11-13
**Task**: Story 4 task #26 (C2): client-local reconnecting slot state, empty-set UI, debug panel, e2e bus events, resume_ms, replay cross-handler token, R-42 series + full export path, key-continuity tests
**Specialist**: client
**Mode**: Agent Teams (v2) — full, Gate-1 present (paired: security, test) <!-- panel mode + Gate-1 tier (ADR-0037 §D2); see the Tier row in Loop State -->
**Branch**: `feature/mh-failure-transfer`
**Duration**: ~4h

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `60c44d704491c2d5929af062d11f49ff86f6ffac` |
| Branch | `feature/mh-failure-transfer` |
| Lead Model | `claude-opus-5-5` |

---

## Loop State (Internal)

<!-- This section is maintained by the Lead for state recovery after interruption. -->
<!-- Do not edit manually - the Lead updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer (client)` |
| Implementing Specialist | `client` |
| Tier | `full` |
| Iteration | `1` |
| Security | `security (paired)` |
| Test | `test (paired)` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Semantic Guard | `semantic-guard` |

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
Story 4 task #26 (C2): client migration UX (client-local reconnecting slot state, empty-set "connecting media"), receive-side continuity (duplicate_cross_transport replay token, key-continuity tests), debug panel / e2e bus, resume_ms, the R-42 connection & disruption series and the non-member count, all with the full export path. Requirements R-16, R-20, R-18 (UI half), R-42.

### Scope
- **Service(s)**: client packages (sdk-core, sdk-svelte, web-app); GC telemetry filter; otel collector; dt-guard; media-vector-gen; dashboards/docs
- **Schema**: No
- **Cross-cutting**: Yes (telemetry export path across client → GC → collector; GSA proto/test-vectors via generator)

### Debate Decision
NOT NEEDED - governed by ADR-0039 §4/§8 and the frozen telemetry-contract §Client (docs/debates/2026-10-28-mh-replacement-and-migration/).

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
| `packages/sdk-core/src/**` | Mine | — |
| `packages/sdk-svelte/src/**` | Mine | — |
| `packages/web-app/src/**` | Mine | — |
| `packages/web-app/tests/**` | Mine | — |
| `packages/web-app/README.md` | Mine | — |
| `docs/specialist-knowledge/client/INDEX.md` | Mine | — |
| `docs/runbooks/client-dev-local.md` | Mine | — |
| `crates/media-vector-gen/src/inventory.rs` | Not mine, Domain-judgment (vector-file SSoT generator, new reject token) | protocol (+ security) |
| `proto/test-vectors/frame-v3.vectors.json` (regen) | Not mine, Domain-judgment (GSA `proto/**`: new `has_vector:false` reject token `duplicate_cross_transport`, regenerated, never hand-edited) | protocol |
| `crates/gc-service/src/services/telemetry_filter.rs` | Not mine, Domain-judgment (new forwarded datapoint keys `phase`, `cause`; drop dead `dt.mh_index_bucket`) | global-controller (+ security) |
| `crates/gc-service/tests/telemetry_proxy_tests.rs` | Not mine, Domain-judgment (same change, tests) | global-controller |
| `infra/services/otel-collector/collector.yaml` | Not mine, Domain-judgment (allowlist +7 names, keep_keys +`phase`,`cause`, charset rule, prose, cardinality recompute; NEW stale-tab filter dropping `dt_client_mh_connection_total` datapoints without `outcome`/`phase` — machinery infrastructure) | observability (policy) / infrastructure (file) |
| `crates/dt-guard/src/client_metrics_export.rs` | Not mine, Minor-judgment (`DECLARED_WRAPPERS` +`#sessionHistogram`; G7 `NOT_KEY_DELIVERY` +`duplicate_cross_transport` — membership = content) | observability + security (content), infrastructure (machinery) |
| `crates/dt-guard/tests/client_metrics_export_fixtures.rs` | Not mine, Minor-judgment (fixtures for new keys/wrapper) | infrastructure |
| `crates/env-tests/src/fixtures/metric_hygiene.rs` | Not mine, Minor-judgment (`MEDIA_PATH_PREFIXES` += `dt_client_mh_`, `dt_client_time_to_first_mh_connected_ms`; pinned to taxonomy block) | observability / test |
| `docs/observability/metrics/client.md` | Not mine, Domain-judgment (catalog entries, roster shrink 5→3, export markers) | observability |
| `docs/observability/label-taxonomy.md` | Not mine, Domain-judgment (prefix block, prose flips, `phase`/`cause` rows, identity-label-cases pass row) | observability |
| `scripts/otel-collector/acceptance.sh` | Not mine, Minor-judgment (stale-tab shape-drop arm: old-shaped writer ABSENT, new-shaped PRESENT, same run) | infrastructure |
| `scripts/otel-collector/sdk_writers.mjs` | Not mine, Minor-judgment (old/new-shaped `dt_client_mh_connection_total` writers) | infrastructure |
| `scripts/otel-collector/acceptance_driver.py` | Not mine, Minor-judgment (only if the arm needs driver support) | infrastructure |
| `infra/services/mc-service/config.env` | Not mine, Minor-judgment (comment only on `MC_MH_HEARTBEAT_TIMEOUT_MS`: the client caps are sized on N, pinned by `packages/web-app/tests/replacementCapsSync.test.ts`; no value change) | meeting-controller |
| `scripts/guards/validate-frame-vectors.test.sh` | Not mine, Minor-judgment (self-test sandbox copies every `spec_anchor` the vectors list, so the new anchor does not break it) | infrastructure |
| `docs/debates/2026-10-28-mh-replacement-and-migration/telemetry-contract.md` | Not mine, Domain-judgment (§Client: non-member line, `duplicate_cross_transport` rule, C1/C2 ownership line — edited BY the owner during Gate 1) | observability |
| `infra/grafana/dashboards/client-media.json` | Not mine, Minor-judgment (new panels) | observability |

---

## Planning

Full plan as sent to reviewers (Gate 1). Mechanism restatement: the client's per-ref transport lifecycle (C1) becomes the ONE source of three derived facts — per-dial/per-session telemetry (per ref), a per-slot "continuity episode" (reconnecting overlay + resume_ms), and the e2e/debug projections — plus a receive-side duplicate classification keyed on the arriving transport. Wider class surfaced: every "client-observed handler fact" must come from MediaTransport events, never from frame silence (ADR-0036 §6).

**A. Transport telemetry (MediaTransport.ts, closeCode.ts, mediaMetrics.ts).**
- `closeCode.ts` gains the ONE token projection `closeCodeToken(code)` (total `never` switch over generated `MhCloseCode`; 0/unknown → `contract_fault`; no code → caller decides `transport_failure` (dial) / `transport_loss` (disconnect)). Test derives expected tokens from `MhCloseCode[code]` names (lowercased, prefix stripped) — same rule MH's `CloseCode::as_label()` uses.
- RefRecord gains `dialPhase` (`join` = dials started by `arm()`; `added` = reconcile after arm; `redial` = redial timer, park release, in-flight-refresh dial), `dialStartedAt`, `established`, `lossStartedAt`. Every dial reaches exactly one outcome while the session lives: `timeout`, `transport_failure`, `aborted_by_removal`, a close-code token, or `connected`. A handshake is PROVISIONALLY connected; the outcome finalises at the first of an MH close (→ close-code token, never a disconnect) or one admission window = `media.handlers.admissionWindowMs` after handshake (→ `connected`, session established). Public `connected` event/status report timing is unchanged (C1 behaviour). `dial_duration_ms{phase, outcome=connected|failed}` dial start → handshake / failure.
- `disconnects_total{cause}` once per end of an ESTABLISHED session: transport_loss (no code), directed_removal (`#removeRef` from a set), handler_released, retry codes, declined, contract_fault. Local teardown (`disconnect()`) emits NOTHING on connection/disconnect series (observability ruling; negative tests).
- `reconnect_duration_ms{outcome}` per ref: starts at loss of an in-set established handler (cause ∈ {transport_loss, handler_released} — the one `isHandlerLoss(cause)` predicate, also the lost-handler count), ends `reconnected` (same ref's next dial established), `removed` (ref left set), `abandoned` (session end or `reconnectAbandonMs` on the injected scheduler).
- `time_to_first_mh_connected_ms` moves off `MeetingSession.#histogram` (join set) to the session bag; join start → handshake of the first dial that FINALISES connected (a refused first dial is never observed), once per session.
- All via `MediaSessionMetrics` (kind-free 3-key bag) with a new `#sessionHistogram` wrapper (added to dt-guard `DECLARED_WRAPPERS`). Vocab consts `MH_DIAL_PHASES`, `MH_DIAL_OUTCOMES`, `MH_DISCONNECT_CAUSES`, `MH_RECONNECT_OUTCOMES`, `MEDIA_RESUME_OUTCOMES`, `MH_NON_MEMBER_SOURCES` + bucket SSoTs in mediaMetrics.ts. `MediaTransportOptions.metricLabels` and `#emitMetric`/`mhIndexBucket` deleted (spread comment flips to "no spread remains"; pins in media-transport / meeting-session / joinLabelSpread / nameGuard / sinks tests flip). Zero-init at join: connection_total 3×10, disconnects 8, non-member 2 (ADR-0032: histograms unobserved = absent).

**B. Slot continuity (new `session/slotContinuity.ts`, one per-slot record).** Inputs: latest StreamAssignments, MediaTransport `handlerLost(ref)` (new event, same `isHandlerLoss` predicate), `handlerRemoved` (set-driven only), `connected`, per-slot presentation evidence `slotPresented(slotId, ref, senderId)` (audio: first played frame per slot via the arriving ref; video: decoded frame), video render active sender, session end. One per-slot record drives BOTH:
- Overlay: set when the slot's latest assignment names the lost/removed ref (planned transfer: edges already moved → nothing set). Keeps the previous occupant (verified live sender or assignment sender). Clears when the latest assignment names a CONNECTED ref (on assignment or on `connected`), or at `reconnectingCapMs` (then the latest assignment renders). Emits `slotReconnectingChanged {slotId, reconnecting, previousSenderId}` (transitions only). C1's `#removeRef` cleanup: the removed ref's per-ref linkage in the tracker is dropped (no per-ref state survives); slots still pointing at it stay reconnecting until reassigned or cap (ADR text "clear the reconnecting state on its slots" read as the per-ref state — flagged for confirmation).
- resume_ms episode: only for slots `active` with a ref at episode start. End: `resumed` = first presentation via a ref ≠ lost ref (same sender); `reassigned_away` = next assignment not `active` or different sender (video: different verified sender at first presentation); `abandoned` = session end or `resumeAbandonMs`. Per-kind via `MediaKindMetrics.resumeMs(outcome, ms)` (this.#base).

**C. Replay token (receivePath.ts ReplayWindow, receivePolicy.ts, frameAccounting.ts, rejectReason.ts, vectors JSON).** `admit(scope, parts, seq, transportId)` → `admitted | replay | duplicate_cross_transport`. Only an exact in-window bit hit whose recorded first-copy transport differs gets the new token; high-water / below-window stay `replay_detected`. Decision unchanged (still dropped). Per-context transport ids in a fixed `windowBits` array (bounded), opaque local numeric id per receive transport, never emitted. Token `duplicate_cross_transport` (no `replay_` prefix) in RejectReason/ALL_REJECT_REASONS + vectors `has_vector:false, layer crypto, drops_frame true` (GSA: protocol) + dt-guard G7 NOT_KEY_DELIVERY. Branch table rows for all arms; "decoded/played once" overlap test.

**D. Non-member count.** `dt_client_mh_non_member_refs_total{source=send_directive|stream_assignment}` (observability's proposal), one per non-member ref in a FRESH message (ref 0 on no-edge states and stale re-checks not counted); SignalingClient gets an injected callback, counted in the session layer; WARN stays once.

**E. Config (clientConfig.ts HandlerSetConfig + HANDLER_SET_CONFIG_RANGES).** `reconnectingCapMs` 30_000 [5_000, 120_000]; `resumeAbandonMs` 60_000 [10_000, 300_000]; `reconnectAbandonMs` 60_000 [10_000, 300_000]. Relations (throw): `reconnectingCapMs <= resumeAbandonMs`; `reconnectAbandonMs >= MH_KEEPALIVE_FLOOR_MS + redialCapMs`. Default arithmetic in doc comment: client detect ≈ MH QUIC idle (6 s) / MC N 6 s + GC acquire + set push + dial (≤10 s connect timeout) + keyframe ≈ 20–25 s worst ordinary → 30 s overlay, 60 s abandonment. Log line `[dt-media-handlers-config]` extended; README table + docs test; runbook. Buckets: disruption buckets top finite ≥ range max (test reads range), dial buckets ≥ connect timeout.

**F. Stores / web-app.** SlotCell gains a `reconnecting` cell; MediaStore gains `mediaConnecting` (latest set empty) and `handlersOverdue` latch cells; bindMeetingSession wires `slotReconnectingChanged`, `mediaHandlerSet`, `mediaHandlersOverdue`. slotState.ts: one client-local row `reconnecting` (like `awaiting-assignment`, in one exhaustive display table) + one `displaySlotState()` used by tile and audio rows; tile renders avatar (canvas hidden) while reconnecting. InMeeting: "connecting media" status and non-fatal overdue notice (ResumeNotice pattern, copy in slotState/errorText). Debug panel: handler set version + {ref,url} by explicit projection from the token-free view. e2e bus: `reconnectingOverlays` [{slotId, reconnecting}] and `transportFaults` [{stage}] scalars only.

**G. Continuity tests (security K1/K2).** Identity + single construction of SessionMedia/TransmitKeyManager across add and remove; keyId→key uniqueness map across the run with positive arm (simulated rebuild collides); generation monotonic, video new-target keyframe gen > pre-swap; audio crash arm strictly increases, planned arm non-decreasing; new egress target awaiting_group + keyframe; slot decoder/jitter/replay identity survives swap; duplicate via two handlers decoded/played once.

**H. Export path.** Collector allowlist +7, keep_keys +`phase`,`cause`, charset rule, prose ("five join-flow" → three, "NOT A PREFIX RULE"), stale-tab note (old labels dropped by keep_keys; meeting_id_hash never kept), cardinality recompute (~56 new kind-free streams/org/version), and a stale-tab shape filter that drops `dt_client_mh_connection_total` datapoints lacking `outcome` or `phase` ("ABSENT, not WRONG"; ANCHOR client.md) with an acceptance-harness arm (old-shaped writer ABSENT + new-shaped PRESENT, same run). GC MEDIA_DATAPOINT_EXTRA +2, drop `dt.mh_index_bucket`. Taxonomy prefix block +2 lines + hygiene const. Catalog: roster 5→3, entries rewritten, teardown exclusions, replay-alert exclusion note. Dashboard panels per observability item 9.

**ADR interpretation notes (security-confirmed).**
- ADR-0039 §4 "Slot states" lists "told to drop" as a reconnecting trigger, and task item 1 says a removed ref renders "reconnecting". So the removal-cleanup line "clear the reconnecting state on its slots" is read as: `#removeRef` drops all per-ref state, including the tracker's linkage for that ref. Slots that still name the ref stay "reconnecting" until they are reassigned or the cap expires.
- Tests pin three things: the tracker holds no entry for the ref after removal; a slot still naming the ref stays reconnecting; and a planned transfer, where the slots have already moved, sets no overlay.
- The previous occupant always comes from the verified sender or the MC-assigned sender.
- Audio on a planned transfer does not rotate. Its directive goes from non-empty to non-empty, and only empty to non-empty rotates (AudioPipeline). So the planned arm asserts non-decreasing generation plus no (sender, stream, generation) reuse with a different key. The crash arm, where the directive goes empty and then non-empty, asserts a strict increase (security K2(d)).
- `connection_total{outcome="connected"}` means "survived the admission window" (catalog note).

**Gate-1 amendments (folded in).**
- Admission window (observability conditions 1-4): it gets its own key, `media.handlers.admissionWindowMs` (default 2_000, range [250, 10_000]), logged on the config line. It is never `MH_KEEPALIVE_FLOOR_MS`.
- Measurement points:
  - `dial_duration{connected}` is measured to the handshake. `{failed}`, including a provisional close, is measured to the failure or close.
  - `time_to_first_mh_connected` and `reconnect_duration{reconnected}` are observed only when a dial FINALISES connected. Both are valued to that dial's handshake.
  - A refused dial produces no observation.
- Outcomes during the window: removal gives `aborted_by_removal`, no-code loss gives `transport_failure`, a 0 or unknown code gives `contract_fault`, and teardown gives nothing. None of them also emits a disconnect.
- C1 has no admission ack and no inbound-evidence hook, so the window is fixed. The catalog notes this, and notes that a refusal arriving after the window is recorded as connected followed by `disconnects{cause}`. That is misattributed between the two series but never lost.
- Test amendments 1-5 are adopted. Lead ruling on removal-cleanup: §4 "until the slot has a source again or a clientConfig cap expires" means slots still naming a removed ref stay reconnecting. This is pinned by three tests.
- Protocol: the vectors row comes from `crates/media-vector-gen/src/inventory.rs:reject_reasons()` (`entry("duplicate_cross_transport", "crypto", true, false, None, None)` after `replay_detected`), regenerated with `cargo run -p media-vector-gen --bin generate-frame-vectors`. There is no mutate.rs row. `spec_anchor` points at telemetry-contract.md if that file names the token.
- Observability owns the telemetry-contract §Client edit and made it itself during Gate 1; it is in the diff (table row above, owner-authored).
- Operations:
  - Every new panel aggregates `by (phase, outcome)` / `by (cause)` / `by (outcome)`, so stale-tab label-less samples don't inflate ratios. This is stated in the collector stale-tab note.
  - Before `dt.mh_index_bucket` is dropped from GC ALLOWLIST, the span path is grepped for remaining emitters.
- DRY: the `connected|failed` projection is derived from `MH_DIAL_OUTCOMES`. Every `mh_index_bucket` consumer is grepped in the same edit. The unfulfilledSlots fit is reported at implementation.

- Observability stale-tab follow-up: a collector filter (Minor-judgment, infrastructure) drops `dt_client_mh_connection_total` datapoints that have no `outcome` or no `phase`. This follows the "ABSENT, not WRONG" principle and carries an ANCHOR to client.md. No rule is needed for `time_to_first_mh_connected_ms`, because once stripped an old row has the same shape and meaning as a new one. The collector acceptance harness covers it with an old-shaped writer that must be absent and a new-shaped writer that must be present, in the same run. The catalog notes this. Panels still group by outcome.

- Code-reviewer:
  - The per-ref telemetry state machine is extracted to `media/transportTelemetry.ts`. It is a per-ref tracker with an injected clock and scheduler plus `MediaSessionMetrics`, and exposes hooks for dialStarted, handshake, closed (code | no-code), removed and teardown. RefRecord holds one reference to its tracker state. Its invariants are unit-tested without MockWebTransport.
  - `closeCodeToken` takes a discriminated input. "No code" has its own named handling at the dial and disconnect call sites, and the `never` check stays.
  - The `reconnecting` row is handled only through the exhaustive table and `displaySlotState()`.

**Rollout/rollback**: server side (collector, GC) before bundle; revert bundle first. Safe-revert unit: GC+collector key changes together with the bundle's label change (a partial revert merges series silently).

### Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security (paired) | confirmed |
| Test (paired) | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Semantic Guard | confirmed |
| Protocol (GSA owner, added) | confirmed |
| Global Controller (Domain-judgment owner, added) | confirmed |

Classification-sanity guard: STATUS=OK (after Lead normalised the vectors row Owner field to `protocol`). Lead ruling: removal-cleanup per ADR-0039 §4 "until the slot has a source again" (see interpretation notes).

### Gate 3 — Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security (paired) | RESOLVED-FIXED | 3 | 3 | 0 | F1 dup of Code Quality F1 (transportRecency fail-open) |
| Test (paired) | RESOLVED-FIXED | 6 | 6 | 0 | F5 = planned-transfer order (with Observability F1) |
| Observability | RESOLVED-FIXED | 5 | 5 | 0 | |
| Code Quality | RESOLVED-FIXED | 7 | 7 | 0 | |
| DRY | RESOLVED-FIXED | 5 | 5 | 0 | no extraction opportunities |
| Operations | RESOLVED-FIXED | 2 | 2 | 0 | collector acceptance run live on Kind |
| Semantic Guard | CLEAR (native SAFE) | 0 | 0 | 0 | |
| Protocol (GSA owner) | CLEAR | 0 | 0 | 0 | |
| Global Controller (owner) | RESOLVED-FIXED | 1 | 1 | 0 | |

Lead ruling at Gate 3 (planned-transfer ordering, Test F5 / Observability F1): client-side bounded overlay hold (`media.handlers.removalGraceMs`); MC order unchanged because ADR-0039 §4 specifies set-first; resume_ms still records the planned blip.

---

## Pre-Work

None.

---

## Implementation Summary

### SDK core
| Item | Before | After |
|------|--------|-------|
| `dt_client_mh_connection_total` / `time_to_first_mh_connected_ms` | join label set (`meeting_id_hash`, `status`, `mh_index_bucket`), not exported | session bag + `{phase, outcome}` via `MediaSessionMetrics`, exported; spread comment removed |
| R-42 new series | — | `dial_duration_ms`, `disconnects_total{cause}`, `reconnect_duration_ms{outcome}`, `non_member_refs_total{source}` (tracker `media/transportTelemetry.ts`, admission window `media.handlers.admissionWindowMs`) |
| Close-code tokens | — | `closeCode.ts:closeCodeToken` (one never-checked switch; 0/unknown = `contract_fault`) |
| Reconnecting overlay + `resume_ms` | — | `session/slotContinuity.ts` (one per-slot record), session event `slotReconnectingChanged`, `MediaTransport` `handlerLost` |
| Replay drop | `replay_detected` only | `ReplayWindow.admitVia` splits an in-window cross-transport duplicate (recent transport) into `duplicate_cross_transport`; decision unchanged |
| Config | 4 handler-set keys | + `admissionWindowMs`, `reconnectingCapMs`, `resumeAbandonMs`, `reconnectAbandonMs` (ranges, two relations, log line, README) |

### Stores / web-app
SlotCell `reconnecting` cell; MediaStore handler set / `mediaConnecting` / overdue latch; VideoTile hides the canvas while reconnecting; `slotState.ts` client-local table (`awaiting-assignment`, `reconnecting`) + `displaySlotState`; InMeeting connecting-media + overdue notice; DebugPanel handler-set rows (explicit projection); e2e bus `reconnectingOverlays` + `transportFaults`.

### Export path (delegated sub-agent, reviewed)
Collector allowlist/keep_keys/charset/stale-tab filter/cardinality; GC `MEDIA_DATAPOINT_EXTRA` +`phase`,`cause`, `dt.mh_index_bucket` dropped; dt-guard wrapper + G7; vectors regen; hygiene prefixes; taxonomy; catalog; dashboard; runbook F31.

### Additional Changes
`MeetingSession` declaration was at the dt-guard 1000-line cap before C2: the transport-options builder moved to a module-level function (999 lines).

---

### Gate-3 fixes (iteration 1)
- Lead ruling on planned-transfer ordering: MC sends the set before the moved assignments (`health.rs` `remove_handler_for`), so a directed removal of a still-connected ref with another member connected HOLDS the overlay for the new `media.handlers.removalGraceMs` (default 2000, [250, 10000], `< reconnectingCapMs`, log line, README). The resume episode still starts at the removal and records the planned blip. An observed loss, or a loss during a hold, overlays at once. New `MediaTransport` event `handlerDropped {ref, wasConnected, otherConnected}` (set-driven removals only).
- `presented` ends an episode only via a CONNECTED transport, so no per-ref state is kept; frames still draining from a lost or removed transport never end it.
- `TransportRecency` fails closed: positive membership plus a never-pruned retired set (security F1 / code-review F1); `#record` clears the position when an admission carries no arrival (security F2).
- One close projection home (`closeCode.ts:disconnectCauseOf`, `dialCloseOutcomeOf`); `MH_DISCONNECT_CAUSES` spreads `MH_CLOSE_CODE_TOKENS`; `DEFAULT_TRANSPORT_SCHEDULER` shared; `MhNonMemberSource` used in SignalingClient; `isClientLocal` derived from the table; `displaySender` helper; boolean `isReconnecting` params.
- Dial buckets extended to 30 s (connect timeout + admission max, pinned).
- MC N ↔ caps drift guard: `packages/web-app/tests/replacementCapsSync.test.ts` + `REPLACEMENT_AFTER_DETECTION_BUDGET_MS` + comment in MC `config.env`.
- New tests: VideoReceiver and AudioPipeline swap survival (decoder identity, replay window across forget, arriving ref), session end-to-end crash→`resume_ms{resumed}` with a real frame, session-level planned transfer in MC's order (no overlay; crash positive arm), video K2(c) generation + wrap uniqueness, recency fail-closed and boundary arms.

### Collector acceptance harness (live Kind, `kind-devloop-mh-failure-transfer`, committed image otelcol-contrib 0.161.0)
- `scripts/otel-collector/acceptance.sh "" stale_tab` → RESULT: PASS (old-shaped and half-shaped rows absent, new-shaped present with exact value, no row lacks phase/outcome, `meeting_id_hash` never stored, phase/cause charset rewrite, gate is name-scoped).
- `scripts/otel-collector/acceptance.sh "" all` → RESULT: PASS (S1, S2, S4, S5, S3 filter controls, S8 stale-tab; 52 PASS assertions).
- `scripts/otel-collector/acceptance.sh "" sdk` → RESULT: PASS (SDK-S1, S2, S5, S6, S7 stale-tab with real MeterProviders).

---

## Files Modified

```
 crates/dt-guard/src/client_metrics_export.rs       |  30 +-
 .../tests/client_metrics_export_fixtures.rs        | 155 +++-
 crates/env-tests/src/fixtures/metric_hygiene.rs    |   8 +-
 crates/gc-service/src/services/telemetry_filter.rs |  92 ++-
 crates/gc-service/tests/telemetry_proxy_tests.rs   | 188 ++++-
 crates/media-vector-gen/src/inventory.rs           |  65 +-
 .../telemetry-contract.md                          |   6 +-
 docs/observability/label-taxonomy.md               |  39 +-
 docs/observability/metrics/client.md               | 335 ++++++--
 docs/runbooks/client-dev-local.md                  |  74 +-
 docs/specialist-knowledge/client/INDEX.md          |   8 +-
 infra/grafana/dashboards/client-media.json         | 856 ++++++++++++++++++++-
 infra/services/mc-service/config.env               |   4 +
 infra/services/otel-collector/collector.yaml       | 165 +++-
 .../sdk-core/src/__tests__/joinLabelSpread.test.ts |  11 +-
 .../src/config/__tests__/handlerSetConfig.test.ts  |  58 +-
 packages/sdk-core/src/config/clientConfig.ts       | 109 ++-
 packages/sdk-core/src/index.ts                     |  12 +
 packages/sdk-core/src/media/MediaTransport.ts      | 142 ++--
 .../src/media/__tests__/media-transport.test.ts    |  84 +-
 .../sdk-core/src/media/__tests__/reconcile.test.ts |  96 ++-
 packages/sdk-core/src/media/closeCode.ts           |  56 ++
 packages/sdk-core/src/media/events.ts              |  53 +-
 .../src/media/frame/__tests__/receivePath.test.ts  |  93 +++
 .../src/media/frame/__tests__/rejectReason.test.ts |   1 +
 packages/sdk-core/src/media/frame/receivePath.ts   | 121 ++-
 packages/sdk-core/src/media/frame/rejectReason.ts  |  10 +
 .../sdk-core/src/media/lifecycle/AudioPipeline.ts  |  21 +-
 .../sdk-core/src/media/lifecycle/VideoReceiver.ts  |  22 +-
 .../__tests__/audioPipeline.multiSender.test.ts    |  37 +
 .../lifecycle/__tests__/videoReceiver.test.ts      |  49 +-
 .../src/media/lifecycle/receiveTransports.ts       |  38 +-
 .../__tests__/receivePolicy.branchTable.test.ts    |  83 +-
 .../media/pipeline/__tests__/videoEgress.test.ts   |  46 ++
 packages/sdk-core/src/media/pipeline/ingress.ts    |  55 +-
 .../sdk-core/src/media/pipeline/receivePolicy.ts   |  16 +-
 .../sdk-core/src/media/pipeline/videoIngress.ts    |  14 +-
 .../src/media/pipeline/videoSlotPipeline.ts        |   9 +-
 .../src/media/setup/__tests__/mediaMetrics.test.ts |  41 +-
 packages/sdk-core/src/media/setup/mediaMetrics.ts  | 235 +++++-
 packages/sdk-core/src/session/MeetingSession.ts    | 131 +++-
 .../src/session/__tests__/meeting-session.test.ts  | 412 +++++++++-
 packages/sdk-core/src/session/events.ts            |  13 +
 packages/sdk-core/src/session/mediaStart.ts        |   5 +
 packages/sdk-core/src/signaling/SignalingClient.ts |  21 +-
 .../signaling/__tests__/handlerSetIntake.test.ts   |  44 ++
 .../sdk-svelte/src/__tests__/MediaStore.test.ts    |  54 ++
 .../helpers/CellGranularityHarness.svelte          |   5 +-
 .../sdk-svelte/src/components/VideoTile.svelte     |   4 +-
 packages/sdk-svelte/src/index.ts                   |   1 +
 .../sdk-svelte/src/stores/MediaStore.svelte.ts     |  62 ++
 .../sdk-svelte/src/stores/bindMeetingSession.ts    |   5 +
 .../sdk-svelte/src/stores/mediaCells.svelte.ts     |  34 +-
 packages/web-app/README.md                         |  11 +
 packages/web-app/src/__tests__/debugPanel.test.ts  |  41 +-
 packages/web-app/src/__tests__/e2eBus.test.ts      |  41 +
 packages/web-app/src/__tests__/inMeeting.test.ts   |  99 +++
 packages/web-app/src/__tests__/slotState.test.ts   |  32 +-
 packages/web-app/src/globals.d.ts                  |   6 +
 packages/web-app/src/lib/debugStats.ts             |  28 +
 packages/web-app/src/lib/e2eBus.ts                 |  54 +-
 packages/web-app/src/lib/slotState.ts              |  88 ++-
 packages/web-app/src/views/DebugPanel.svelte       |  30 +-
 packages/web-app/src/views/InMeeting.svelte        |  33 +-
 packages/web-app/src/views/VideoSlotTile.svelte    |  37 +-
 packages/web-app/tests/debugStats.test.ts          |  29 +-
 proto/test-vectors/frame-v3.vectors.json           |   8 +
 scripts/guards/validate-frame-vectors.test.sh      |   9 +
 scripts/otel-collector/acceptance.sh               |   9 +-
 scripts/otel-collector/acceptance_driver.py        |  56 ++
 scripts/otel-collector/sdk_writers.mjs             |  32 +-
 71 files changed, 4501 insertions(+), 470 deletions(-)

New files:
docs/devloop-outputs/2026-11-13-client-mh-replacement-ux-telemetry/main.md
packages/sdk-core/src/media/__tests__/transportTelemetry.test.ts
packages/sdk-core/src/media/lifecycle/__tests__/transportRecency.test.ts
packages/sdk-core/src/media/lifecycle/transportRecency.ts
packages/sdk-core/src/media/pipeline/__tests__/ingress.overlap.test.ts
packages/sdk-core/src/media/transportTelemetry.ts
packages/sdk-core/src/session/__tests__/slotContinuity.test.ts
packages/sdk-core/src/session/slotContinuity.ts
packages/web-app/tests/replacementCapsSync.test.ts
```

### Key Changes by File
| File | Changes |
|------|---------|
| `packages/sdk-core/src/media/transportTelemetry.ts` | New per-ref R-42 tracker (admission window, outcomes, disconnect causes, reconnect duration) |
| `packages/sdk-core/src/session/slotContinuity.ts` | New per-slot record: reconnecting overlay (with removal hold), resume_ms episodes |
| `packages/sdk-core/src/media/lifecycle/transportRecency.ts` | Fail-closed membership/recency for the duplicate_cross_transport token |
| `packages/sdk-core/src/media/frame/receivePath.ts` | Replay window records arriving transport per entry |
| `packages/sdk-core/src/config/clientConfig.ts` | New caps: reconnectingCapMs, resumeAbandonMs, reconnectAbandonMs, admissionWindowMs, removalGraceMs |
| `packages/web-app/src/lib/slotState.ts` | `reconnecting` client-local row, displaySlotState/displaySender |
| `crates/gc-service/src/services/telemetry_filter.rs` | +phase/cause datapoint keys; -dt.mh_index_bucket |
| `infra/services/otel-collector/collector.yaml` | Allowlist +7, keep_keys, charset, stale-tab shape filter |
| `crates/media-vector-gen/src/inventory.rs` + `proto/test-vectors/frame-v3.vectors.json` (regen) | duplicate_cross_transport reject reason |

---

## Devloop Verification Steps

Gate 2: `DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` (all layers, unattended) — **TOTAL_RESULT=OK**, TOTAL_DURATION=2418s.

| Layer | Result | Duration |
|-------|--------|----------|
| 1 Compile | OK | 7s |
| 2 Format | OK | 5s |
| 3 Guards | OK | 123s |
| 4 Test | OK | 141s |
| 5 Lint | OK | 1s |
| 6 Audit | OK (dep audit SKIPPED-NO-DIFF no-dep-changes) | 5s |
| 7 Env-tests | OK (env-tests-passed, browser-e2e-passed) | 2136s |

Fast check before review: `layer-fast.sh` TOTAL_RESULT=OK. Collector acceptance harness run live on Kind (stale_tab, all, sdk: PASS) — see Implementation Summary.

(Semantic-guard relocated to the Gate 2 reviewer panel per ADR-0033 Wave 3 #9. See § Code Review Results → Semantic Guard Reviewer below for its findings.)

---

## Code Review Results

See §Gate 3 — Verdicts for the table. Summary per reviewer:

### Security Specialist
**Verdict**: RESOLVED-FIXED
**Findings**: 3 found, 3 fixed, 0 deferred

F1 transportRecency fail-open (also flagged by code-reviewer) → positive membership + retired set; F2 replay-window stale transport id → cleared; F3 video key-generation/wrap-uniqueness assertion added.

### Test Specialist
**Verdict**: RESOLVED-FIXED
**Findings**: 6 found, 6 fixed, 0 deferred

resume_ms ending on removed-ref frame; missing swap-survival tests; collector acceptance unrun (now run live); window edge pin; planned-transfer real MC order (removal hold); vacuous session-level negative (FakeScheduler).

### Observability Specialist
**Verdict**: RESOLVED-FIXED
**Findings**: 5 found, 5 fixed, 0 deferred

Planned-transfer order; resume_ms end wording; dial buckets to 30s; dashboard count axes; stale-tab wording.

### Code Quality Reviewer
**Verdict**: RESOLVED-FIXED
**Findings**: 7 found, 7 fixed, 0 deferred

F1 transportRecency (dup with security F1); orphan JSDoc; close-code cause helpers; MH_DISCONNECT_CAUSES reuse; MhNonMemberSource; isClientLocal derived; shared scheduler.

### DRY Reviewer
**Verdict**: RESOLVED-FIXED

**True duplication findings** (entered fix-or-defer flow):
5 found, 5 fixed (close-code projections, cause token list, isClientLocal, displaySender, slotContinuity non-collapse note).

**Extraction opportunities** (appended to `docs/TODO.md`):
None

### Operations Reviewer
**Verdict**: RESOLVED-FIXED
**Findings**: 2 found, 2 fixed, 0 deferred

Collector filter unloaded → acceptance run live; caps vs MC heartbeat timeout drift test (replacementCapsSync.test.ts).

### Semantic Guard Reviewer
**Verdict**: CLEAR
**Native verdict**: SAFE
**Findings**: 0 found, 0 fixed, 0 deferred

No findings.

### Protocol (GSA owner)
**Verdict**: CLEAR — 0 findings.

### Global Controller (Domain-judgment owner)
**Verdict**: RESOLVED-FIXED — 1 found (stale doc comment), 1 fixed.

---

## Accepted Deferrals

**Each entry here is an issue the devloop chose NOT to fix.** Every bullet is a cost shift: the implementer didn't pay the fix-now cost, so a future reader will pay fix-later cost + tracking overhead. List only what was actually deferred — not "follow-ups" or "future improvements" or "potential extractions." If something was fixed, it doesn't belong here.

**Tech debt entries themselves live in `docs/TODO.md`. This section holds only pointers to those entries.** Do not create a `TODO.md` at the repo root or anywhere else — there is exactly one `docs/TODO.md` for the whole project. Do not inline the debt body here — multi-line entries belong in `docs/TODO.md`, not in this section.

Each pointer is exactly one bullet of the form `- \`docs/TODO.md\` §SECTION-NAME — one-line hook (≤80 chars)`. If you wrote more than one line per entry, you're writing it in the wrong file — move the body to `docs/TODO.md` and leave only the pointer here.

- (none surfaced in this devloop)

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `60c44d704491c2d5929af062d11f49ff86f6ffac`
2. Review all changes: `git diff 60c44d704491c2d5929af062d11f49ff86f6ffac..HEAD`
3. Soft reset (preserves changes): `git reset --soft 60c44d704491c2d5929af062d11f49ff86f6ffac`
4. Hard reset (clean revert): `git reset --hard 60c44d704491c2d5929af062d11f49ff86f6ffac`
5. For schema changes: rollback requires a forward migration — `git reset` alone is insufficient if migrations were applied
6. For infrastructure changes: may require `skaffold delete` or `kubectl delete -f` if manifests were applied
7. **Safe-revert unit** (answer explicitly, even if "the whole commit"): can any part of this diff be reverted or cherry-picked on its own, or does a partial revert reconstruct a state worse than either endpoint (e.g. a security fix split from the change that made it necessary, or a client/server/alert-rule set that must move together)? If partial reverts are unsafe, name the unit and the safe direction. **Answer**: `the whole commit for the label change` — the SDK bundle's `phase`/`cause` labels, GC `MEDIA_DATAPOINT_EXTRA`, collector `keep_keys`/allowlist/stale-tab filter move together: a partial revert of only the GC or collector key change silently merges per-phase/cause series. Safe direction: roll out GC + collector before the bundle; roll back the bundle first, server side last. (This converts silence into a visible unanswered slot; it cannot distinguish a checked answer from a reflexive one.)

---

## Issues Encountered & Resolutions

### Issue 1: Planned-transfer event order
**Problem**: MC sends the reduced set before moved assignments (ADR-0039 §4), so the initial client design overlaid on every planned transfer.
**Resolution**: Lead ruling — bounded client-side overlay hold (`media.handlers.removalGraceMs`); resume_ms still records the planned blip.

### Issue 2: Replay-token laundering
**Problem**: transportRecency failed open after pruning, letting real replays be labelled `duplicate_cross_transport`.
**Resolution**: Fail-closed membership with a never-pruned retired set; tests for never-seen, prune-revival, straggler.

### Issue 3: Lead address
**Problem**: Teammates could not resolve `team-lead`.
**Resolution**: Lead addressed as `main`.

---

## Lessons Learned

1. Unit fixtures for cross-service ordering must cite the producer's real order (MC health.rs) — the Gate-1 plan didn't check it.
2. Delegated sub-agent work needs explicit full-panel scrutiny; reviewers found it sound.
3. Owner reviewers (protocol, GC) should be identified at setup when the task names their files.

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
