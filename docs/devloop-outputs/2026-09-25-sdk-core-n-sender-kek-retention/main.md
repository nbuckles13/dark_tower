# Devloop Output: sdk-core N-sender receive path, KEK rotation retention, roster-rebind key purge

**Date**: 2026-09-25
**Task**: Story 2 (hear-each-other) task 7 — sdk-core per-sender decoders + slot-edge gate, bounded KEK retention (current + ≤1 previous for W/2), sender-side R-13 transmit-key rotation, roster rebind purge, peer-key validation, new client metrics + export decisions, frame-v2 vector reject reason (R-5, R-13, R-14, R-17, R-18; ADR-0036 §4)
**Specialist**: client
**Mode**: Agent Teams (v2) — full, Gate-1 present
**Branch**: `feature/hear-each-other`
**Duration**: ~14h wall-clock (2026-09-25 → 2026-09-26; Gate 1 ~2.5h, implementation ~45m, Gate 2 ×2 ~25m each, review ~2h)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `21228100d648501cb6b79c98111b17b2e703c005` |
| Branch | `feature/hear-each-other` |
| Lead Model | `claude-opus-5-5` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` |
| Implementing Specialist | `client` |
| Tier | `full` |
| Iteration | `1` |
| Security | `paired-security` (paired, replaces security slot) |
| Test | `paired-test` (paired, replaces test slot) |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Semantic Guard | `semantic-guard` (spawned: key-material lifetime / zeroize surface) |
| Protocol (conditional, GSA owner) | `protocol` (proto/test-vectors/frame-v2.vectors.json) |
| Infrastructure (conditional, Domain-judgment owner) | `infrastructure` (infra/kind/scripts/setup.sh) |
| Media Handler (conditional, GSA co-owner) | `media-handler` (crates/media-protocol/src/codec.rs; mh-service tests) |

---

## Task Overview

### Objective
See task prompt (story-runner task-7.prompt); summary in header.

### Scope
- **Service(s)**: client (`packages/sdk-core`), otel-collector configmap, client metrics catalog, frame-v2 test vectors
- **Schema**: No
- **Cross-cutting**: Yes (GSA: `proto/test-vectors/**` → protocol + security)

### Debate Decision
NOT NEEDED — ADR-0036 §4 and story R-5/R-13/R-14/R-17/R-18 fix the design.

---

## Cross-Boundary Classification

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `packages/sdk-core/src/media/pipeline/ingress.ts` | Mine | |
| `packages/sdk-core/src/media/pipeline/receiveLanes.ts` (new) | Mine | |
| `packages/sdk-core/src/media/pipeline/playbackSink.ts` | Mine | |
| `packages/sdk-core/src/media/lifecycle/AudioPipeline.ts` | Mine | |
| `packages/sdk-core/src/media/lifecycle/transmitKeys.ts` | Mine | |
| `packages/sdk-core/src/media/pipeline/egress.ts` | Mine | |
| `packages/sdk-core/src/media/setup/kekSource.ts` | Mine | |
| `packages/sdk-core/src/media/setup/rosterKeys.ts` | Mine | |
| `packages/sdk-core/src/media/setup/mediaMetrics.ts` | Mine | |
| `packages/sdk-core/src/media/setup/seams.ts` | Mine | |
| `packages/sdk-core/src/media/frame/receivePath.ts` | Mine | |
| `packages/sdk-core/src/media/frame/rejectReason.ts` | Mine | |
| `packages/sdk-core/src/config/clientConfig.ts` | Mine | |
| `packages/sdk-core/src/signaling/kekIntake.ts` | Mine | |
| `packages/sdk-core/src/signaling/SignalingClient.ts` | Mine | |
| `packages/sdk-core/src/session/MeetingSession.ts` | Mine | |
| `packages/sdk-core/src/index.ts` | Mine | |
| `packages/sdk-core/src/config/__tests__/**` | Mine | |
| `packages/sdk-core/src/media/__tests__/**` | Mine | |
| `packages/sdk-core/src/media/frame/__tests__/**` | Mine | |
| `packages/sdk-core/src/media/lifecycle/__tests__/**` | Mine | |
| `packages/sdk-core/src/media/pipeline/__tests__/**` | Mine | |
| `packages/sdk-core/src/media/setup/__tests__/**` | Mine | |
| `packages/sdk-core/src/session/__tests__/**` | Mine | |
| `packages/sdk-core/src/signaling/__tests__/**` | Mine | |
| `packages/sdk-core/src/media/lifecycle/receiveTransports.ts` (new; split out of `AudioPipeline` so the class stays under `ts_retained_credentials`' 3x declaration-scan margin) | Mine | |
| `packages/sdk-core/src/signaling/rosterKeyFeed.ts` (new; split out of `SignalingClient` for the same guard margin) | Mine | |
| `packages/sdk-core/src/session/mediaWiring.ts` (new; the rebind listener + KEK observer, shared by the session and its integration test) | Mine | |
| `packages/test-utils/src/media/index.ts` | Mine | |
| `packages/test-utils/src/index.ts` | Mine | |
| `packages/test-utils/src/__tests__/FakeAudioCodecs.test.ts` (new; the per-instance codec double's own contract, @paired-test Gate-3 F4) | Mine | |
| `docs/runbooks/client-dev-local.md` | Mine | |
| `docs/specialist-knowledge/client/INDEX.md` | Mine | |
| `crates/media-vector-gen/src/inventory.rs` | Not mine, Domain-judgment | protocol |
| `crates/media-vector-gen/src/json.rs` | Not mine, Domain-judgment | protocol |
| `proto/test-vectors/frame-v2.vectors.json` (regen; **GSA**; security co-sign) | Not mine, Domain-judgment | protocol |
| `crates/media-protocol/src/codec.rs` (doc de-enumeration only; **GSA**; co-owner media-handler + security co-sign) | Not mine, Domain-judgment | protocol |
| `crates/mh-service/tests/media_metrics_integration.rs` | Not mine, Minor-judgment | media-handler |
| `crates/mh-service/src/observability/metrics.rs` | Not mine, Minor-judgment | media-handler |
| `crates/gc-service/src/services/telemetry_filter.rs` (comment only; no allowlist/filter change) | Not mine, Mechanical | global-controller |
| `infra/docker/prometheus/rules/mc-alerts.yaml` | Not mine, Domain-judgment | operations |
| `docs/runbooks/mc-incident-response.md` | Not mine, Minor-judgment | operations |
| `docs/runbooks/gc-deployment.md` | Not mine, Minor-judgment | operations |
| `docs/runbooks/mc-deployment.md` | Not mine, Minor-judgment | operations |
| `docs/runbooks/ac-service-deployment.md` (two-site R-55 -> R-54 citation fix; authored and swept by @operations as runbook owner at Gate 3, referred by @infrastructure) | Not mine, Minor-judgment | operations |
| `infra/kind/scripts/setup.sh` (collector rollout restart + status in `deploy_otel_collector()`; Lead ruled route 1) | Not mine, Domain-judgment | infrastructure |
| `docs/observability/alerts.md` | Not mine, Minor-judgment | observability |
| `docs/observability/metrics/client.md` | Not mine, Minor-judgment | observability |
| `docs/observability/metrics/mh-service.md` | Not mine, Minor-judgment | observability |
| `docs/observability/label-taxonomy.md` (`outcome` corollary sentence only; the partition-section bullets for both new tokens are OBSERVABILITY's to write, not mine) | Not mine, Domain-judgment | observability |
| `infra/grafana/dashboards/client-media.json` | Not mine, Minor-judgment | observability |
| `infra/services/otel-collector/configmap.yaml` | Not mine, Minor-judgment | observability |
| `scripts/otel-collector/acceptance_driver.py` | Not mine, Mechanical | infrastructure |
| `docs/TODO.md` (D5 oracle-list addition ONLY; the collector entry is operations', the by(outcome) guard entry is observability's) | Not mine, Mechanical | observability |
| `docs/user-stories/2026-09-21-hear-each-other.md` (two Lead-AUTHORISED edits: the four-site gauge->counter rename, and task 16's prompt gaining the Group A/B surfaces for the six new counters, the name-the-known-non-zero-reasons condition, the `unwrap_failed` selector-membership decision, and the derive-vs-hand-maintain question. Lead's own task-7/13 amendment paragraphs untouched; `dt-story validate` clean) | Mine | |

---

## Planning

### Restated as mechanism
Four mechanisms, each with a wider class than the prompt names:
1. **Receive authority = MC's slot assignment.** Today the only receive-side authority is "has a roster key". The mechanism is "a frame is decodable iff its key-id sender (after verify) is in the MC-stated assignment set"; decoders are *children* of that set. One home (`ReceiveLanes`, fed only from `StreamAssignments`), never frame-observed state (the hop monitor stays diagnostic).
2. **Bounded KEK custody with derived retention.** Current + <=1 previous, previous expires at `min(W/2, ceiling)` from the W on the message that caused the demotion; send side can only reach *current*.
3. **Key-change invalidation.** Any time the roster forgets or changes the bytes bound to a sender id, that sender's cached unwrapped transmit keys are purged (rebind: counted; LRU eviction: silent hygiene). Replay state is never touched by any roster path.
4. **Token-set widening.** Two new receive-drop tokens (`kek_generation_stale`, `sender_not_assigned`) touch ~19 mirror sites (DRY D-2); all are in this diff.

### Token set (final, for protocol + security)
| Token | layer | drops_frame | has_vector | alert selector |
|---|---|---|---|---|
| `kek_generation_stale` | `key` | true | false | inside `MCMediaMissingKeyMaterial` |
| `sender_not_assigned` | `assignment` (new layer value) | true | false | none this story (task 16 inventory) |

Classification in `openVerifiedFrame`, only when an unwrap is needed and no transmit key is cached (existing cached arms unchanged): generation > newest held -> `no_kek_for_generation`; otherwise not held (below current and not the retained previous, incl. expired previous, gaps) -> `kek_generation_stale`. `ReceiverKeys` gains `isOlderThanRetained(generation)`. No generation in any message. Non-collapse note vs `kek_generation_not_held` at inventory.rs / receivePath.ts / rejectReason.ts (wording corrected: *not held but transmit key cached -> accepted, wrap outcome `kek_generation_not_held`; not held and no cached transmit key -> dropped: newer -> `no_kek_for_generation`, older than retained -> `kek_generation_stale`*).

### Receive path (R-5, slot-edge gate)
- New `media/pipeline/receiveLanes.ts` `ReceiveLanes`: the ONE slot->sender view, fed by `AudioPipeline.setReceiveAssignments(assignments)` (replaces `setReceiveHandlers`; URLs derived from the same list). Holds `Map<slotId, senderId>` and `Map<senderId, DecodeLane>`. Re-map diff: senders leaving the set -> close only their lane; senders entering -> new lane (own `AudioDecoderSeam` + own playback lane); unchanged -> untouched. Never touches replay/transmit-key/roster state. Lane count bounded by declared slots (G4); plus own config bound `ingress.maxDecodeLanes` (D-7).
- Ingress order: parse -> hop observe -> roster resolve (`no_roster_entry`) -> **verify** -> **gate** (`lanes.laneFor(sender)` undefined -> `sender_not_assigned`, before `openVerifiedFrame` so no replay advance, no wrap cache, no decrypt) -> open -> accepted -> `lane.decode`. Gate keys on the verified key-id sender only; relay `stream_id` stays quarantined (G2). "(slot, sender) regardless of transport": membership is sender-in-assignment-set, transport never consulted.
- Decoder fault isolation: each lane's `onError` counts `decoderError()` (existing base-label counter, event-once per decoder instance), closes that decoder only, raises a NON-fatal `decoder` fault once, and lazily replaces it (rate-bounded: at most one replacement per `ingress.decoderRestartBackoffMs` per lane, so a hostile sender cannot make it per-frame telemetry). Frames arriving while a lane's decoder is being created/replaced go to a small bounded per-lane pending queue (`ingress.decoderPendingFrames`); overflow drops oldest - that loss is in the accepted->audible segment which `decoder_errors_total` covers, as documented today.
- Pending-queue evictions are COUNTED on `dt_client_media_decode_queue_dropped_total` (F2). The earlier claim that `decoder_errors_total` covered this loss was wrong: that counter fires on the decoder's terminal error callback, a queue eviction fires no callback, and `client.md`'s "covered only partially" sentence is a statement of a known gap rather than a licence to widen it.
- Mixing: `PlaybackSink` gains `openLane(): PlaybackLane` (per-sender playhead, same destination node -> WebAudio sums = mixing). Today's single playhead would serialise N senders into one timeline (unbounded latency growth) - that is the actual mixing bug. `RecordingPlaybackSink` gets per-lane records.
- `FakeAudioCodecs` gets per-instance decoder records (`decoders[]` with `decoded`, `closed`, `fail()`), keeping aggregates (paired-test #1).

### KEK holder (R-14, `kekSource.ts`)
- `JoinResponseKekSource` -> `MeetingKekHolder` (public export renamed; no web-app consumer). `install(kek, generation, debounceSeconds, source)`: K1 rules exactly as paired-security listed (g>c demote+zeroize dropped p+arm expiry; g==c equal bytes no-op, no re-arm, no rotate; g==c different bytes refuse+count+WARN, fixed-time compare; g<c ignore+count). K2 generation 0..=65535, width, all-zero -> refuse+scrub (both arrival paths). u16 wrap not handled (MC never wraps within a meeting) - stated.
- Retention: ONE function `deriveKekRetention(wSeconds, rewrapLatencyMs)` in `config/clientConfig.ts` beside `KEK_RETENTION_FLOOR_MS` / `KEK_RETENTION_CEILING_MS` constants (not config knobs - the client never configures retention) with `ANCHOR (DRY)` to the proto field. Returns `{retentionMs, outcome}` with outcome `nominal | floor_substituted | ceiling_clamped | below_rewrap_latency`. T = `transmitRewrapLatencyMs(media)` = `(egress.maxQueueFrames + egress.transportOutgoingHighWaterMarkFrames + 1) * audio.frameDurationMs` (with R-13 the sender rotates synchronously on KEK receipt, so the only old-KEK frames still leaving are those already queued) - derived from existing config, never 0. `validateMediaConfig` asserts FLOOR > T and FLOOR <= CEILING. Below-T at runtime: counted + WARN naming the relationship, retention NOT raised (raising could exceed W).
- Expiry: timer seam (injected clock/setTimeout/clearTimeout) fills+drops previous; lazy backstop in `kekForGeneration`; `clear()` zeroizes both and cancels timer.
- K6: send side reads `currentForWrapping(): {kek, generation} | undefined` at MINT time (fixes today's construction-time `kekGeneration` capture); `AudioPipelineOptions.kekGeneration` removed.
- Retention guard: holder stores `#held` newest-first as an array; `enforceRetentionBound(held, onViolation)` (module export, not package export; pure over an array it is handed) runs after EVERY install incl. idempotent, trims+zeroizes any excess and calls `onViolation` -> `kekRetentionViolation()`. Live-path proof via a monotone `retentionChecks` getter (no injection door): install through the real `SignalingClient` intake path and assert it advanced once per install; verified red by deleting the call site.
- Observer hooks (metrics + WARN logger) injected by `MeetingSession`; `MediaMetrics` built at join once orgId is known and reused by `startMedia` (so join-response arrival is counted at arrival, not at media start).

### Sender R-13
Holder notifies `onCurrentGenerationChanged` only on a demoting install (never on equal). `AudioPipeline` subscribes: install happens first, then `transmitKeys.rotate()`; next mint wraps under the new current. rotate() comment "three call sites" -> four. In-flight mint orphaning kept; race test added.

### Roster rebind (R-18) / peer keys (R-17)
`RosterIdentityKeys` stores `{key, bytes}` (bytes copied, public). Decision is synchronous at `upsert` entry: equal bytes -> no-op (LRU touch only); new sender or keyless->key -> first binding, not a rebind; present key -> different bytes -> REBIND; present key -> empty/malformed -> downgrade (treated as rebind: purge + count, entry known-keyless). On rebind the entry is set keyless synchronously (frames fail closed as `no_roster_entry` during import), then the import installs only if its per-sender sequence is still latest (R2). Import failure -> known-keyless, never leaves the old key. `onTransmitKeysInvalidated(senderId, cause)` -> session -> `pipeline.purgeTransmitKeys(senderId)` -> new `TransmitKeyCache.purgeSender` (zeroize). LRU eviction (and `remove()`) also purge (R5, uncounted hygiene). `ReplayWindow` untouched on every roster path (R4). Stale "AC attestation ... is story 2" comment repointed to "the attestation story"; no attested/verified/trusted wording.

### Metrics (all base labels + at most one of the existing keep_keys discriminators; no sender/meeting/stream)
| Metric | Type | Labels | Notes |
|---|---|---|---|
| `dt_client_media_kek_retention_violations_total` | counter | base | reads zero forever; alertable >0; no alert today (task 16 inventory) |
| `dt_client_media_kek_retention_anomalies_total` | counter | `outcome` = `floor_substituted`/`ceiling_clamped`/`below_rewrap_latency` | per KEK message; floor = rollback detector (O5) |
| `dt_client_media_kek_install_refusals_total` | counter | `outcome` = `conflicting_key`/`older_generation`/`malformed` | K1/K2. `outcome`, NOT `reason` (F1): `label-taxonomy.md:86` scopes `reason` to per-FRAME drops with exactly two homes, and a third family there breaks cross-end `sum by(reason)`, silently voids frame-token selectors on this metric, and puts a vocabulary outside guard G6's `invalid`-sentinel read. `outcome` is the documented choice for a metric-local value set (`label-taxonomy.md:88`) |
| `dt_client_media_kek_generations_retained_total` | counter | base | one per RETAINING install (a previous generation was demoted and kept). Watches the opposite failure from the violations counter: retaining too FEW. Fleet signal is the PAIR - `kek_updates_total{source="kek_update"}` rising while this stays flat means rotations are happening and nothing is retained (an audio gap at every rotation, R-14's headline). Replaces the story's `kek_generations_retained` GAUGE, which is last-writer-wins over a {1,2} domain |
| `dt_client_media_decode_queue_dropped_total` | counter | base | one per frame evicted from a lane's pending queue while its decoder is absent/being replaced (F2). POST-ACCEPT, so deliberately outside `received = accepted + sum(drops)` - it must NOT ride `frames_dropped_total` or one frame lands on both sides of the identity. Mirrors the send-side precedent: the bounded egress queue counts its evictions BECAUSE the platform exposes no event, and a receive-side queue that evicts silently is that construct with the counter removed |
| `dt_client_media_roster_key_rebinds_total` | counter | base | one per rebind EVENT (O7) |
| `dt_client_media_kek_updates_total` | existing | `source` += `kek_update` | `KekArrival` collapsed into `MediaKekSource` (D-6) |
| `dt_client_media_frames_dropped_total` | existing | `reason` += 2 tokens | |
All six new names: collector name allowlist + `Exported: yes` marker + catalog entry; client.md counts 14/19 -> 20/25 and "fifteen of sixteen" recounted. No keep_keys / GC filter change (outcome, reason, source already admitted). The story's `dt_client_media_kek_generations_retained` GAUGE is not added; @observability ruled the gauge out AND ruled that the violations counter does not supersede it, so the retaining-install COUNTER above takes its place. Flagged to @paired-test: the story text (~line 549) names the retained-generations gauge as task 19's S6 assertion target, so S6 asserts on the counter instead.

### Out of scope, with reason
`dt_client_media_capture_source{mode}` - all five sites move to task 13 with its emitter per @observability O2 (adding the name now reds G3; `mode` alone reds G4). Not a deferral of this task's work: the emitter does not exist yet.

### Alert selector (O1)
`mc-alerts.yaml` selector -> `no_kek_for_generation|kek_generation_stale|no_roster_entry`, description three arms, comment that threshold re-derivation is task 16; mirrored in `alerts.md`, `mc-incident-response.md`, `client-media.json` description, collector/GC/driver comments (tense fixed).

### Tests
- `media/setup/__tests__/kekSource.test.ts` - K1-K5 table, expiry both sides of the boundary via fake timers, zeroization by held buffer reference, guard unit test on fabricated array, W=0 floor + log seam, ceiling, below-T.
- `config/__tests__/kekRetention.test.ts` (expected values from `deriveKekRetention`, never restated W/2).
- `media/setup/__tests__/rosterKeys.rebind.test.ts` - rebind/no-op/first-binding/downgrade/out-of-order imports/eviction purge.
- `media/lifecycle/__tests__/audioPipeline.multiSender.test.ts` over `MockWebTransport` (2 transports, >=3 senders, real `buildTestFrame` crypto, distinct plaintext per sender): N concurrent; `it.each` bad-sender isolation (signature, replay, missing key, decoder fault); KEK switch continuity with in-flight gen0; second rotation stale/newer boundaries; S7 leaver rig fed frames from the REAL egress after `onKekUpdate` (R-13 generation bump + wrapped kek_generation asserted); joiner after rotation (pre-rotation gen0 frame -> `kek_generation_stale`); rebind purge + rebind-back replay rejected; malformed peer key `it.each` (empty, 31, 33; unimportable row only if a 32-byte value actually fails raw Ed25519 import - otherwise dropped with a stated reason); unassigned sender with positive control; remap closes only the affected decoder; identity `received = accepted + sum(drops)` checked every scenario; label hygiene over all recorded `dt_client_media_*` (non-empty); live-path retention guard via SignalingClient intake.
- Existing suites updated: rejectReason set equality, vectors conformance (`ReceiverKeys` shape), playbackSink lanes, transmitKeys KEK-change race, hotPathLayout.

### Review addenda folded in (planning round)
- `sender_not_assigned` is EXCLUDED from `MCMediaMissingKeyMaterial` by decision (misrouting remedy lives in MH/MC placement, not KEK/roster delivery); both new tokens DO join the receiver-state-dependent oracle lists (`label-taxonomy.md`, `docs/TODO.md` D5).
- `crates/mh-service/tests/media_metrics_integration.rs`: both literals added to the hand-typed `.chain([...])` (stays green if skipped - the invisible item); `metrics.rs` prohibition list corrected per @media-handler: NINE crypto/key tokens (`kek_generation_stale` joins), with `sender_not_assigned` barred by a SEPARATE sentence and a different reason (see below); count eighteen total. Hand-typed half kept (recorded non-collapse).
- paired-test (a)-(f): identical-datagram positive control for the gate; decoder backoff/pending-queue cases on injected clock; separate timer vs lazy-backstop expiry tests; refused installs leave held state working; guard trim asserts zeroed excess buffers and `retentionChecks` advances on every intake path; orphaned-mint frame at pre-rotation generation opens on a retaining receiver.
- paired-security A/B: `codec.rs` (GSA `crates/media-protocol/**`) and `json.rs` (layer schema) reclassified Domain-judgment.
- paired-security C: `SignalingClient` keeps a bounded participant_id -> sender_id map from the roster it already feeds; `ParticipantLeft` resolves to the sender id and calls `RosterIdentityKeys.remove()` -> leaver drops at `no_roster_entry` before verify AND its transmit keys are purged (independent of the slot gate). Replay state survives remove().
- paired-security D-G: comment beside `deriveKekRetention` that T covers only this client's queue (push skew + transit are covered by FLOOR/CEILING); rebind-race residual comment in the purge path (a cached transmit key opens only frames that verify under the CURRENT roster key); telemetry_filter.rs is comment-only; WARN lines carry durations only.
- @observability F1/F2/F3 and the retained-counter ruling: all adopted (two further exported counters; see the metrics table). F3: `client.md`'s `decoder_errors_total` entry gains a counting-point note in the same form as `frames_sent_total`'s per-target-handler note - one decoder per client becomes one per ACTIVE SENDER, so one fault class can advance it up to N times and its magnitude is not comparable with pre-story-2 history.
- Anomaly/refusal catalog entries state: `floor_substituted` reads zero forever EXCEPT during a deliberate one-version MC rollback (so nobody pages during a planned one); `below_rewrap_latency` can only fire if MC's W/2 is under the client's T, and `validateMediaConfig` already asserts FLOOR > T at startup, so its remedy is in MC (raise `MC_KEK_ROTATION_DEBOUNCE_SECONDS`) - the one anomaly arm pointing at another service; these counters carry no denominator of their own, the denominator is `kek_updates_total`, so the ratio needs the non-zero-denominator guard.
### `assignment` layer + reconciled non-collapse wording (protocol A/B + observability rulings)
SPELLING IS `assignment`, NOT `routing` - observability's taxonomy ruling, since confirmed by protocol. `routing` collides with `label-taxonomy.md:86`'s second `reason` family, the "relay transport-and-ROUTING family bounded by MH's `MediaDropReason`", so a client token at `layer:"routing"` would read as a member of the relay family it is definitionally not in - and this diff is already extending the prohibition lists that exist to keep relay-versus-receiver membership unambiguous. `admission` is worse (MH ships `mh_media_stream_admission_*`). `assignment` continues the STAGE series: `codec` the bytes, `crypto` the verification, `key` the held key material, `assignment` the held slot-assignment set. Constructor is `assignmentReject()`.

**RETRACTED PREMISE, recorded so no comment restates it.** Observability introduced and has now withdrawn "`layer` answers which subsystem owns the remedy", and I had adopted it. Verified against the tree: `json.rs:179` and `rejectReason.ts:137` both say STAGE, not remedy, and `label-taxonomy.md:248` records that `no_transmit_key` is `layer:"codec"` yet fires on key-store membership, stating in terms that "the layer-based version of this rule was wrong". So `layer` does NO alert-selector work and nothing may claim it does. What protects `MCMediaMissingKeyMaterial` is (a) the selector being an explicit three-token enumeration, never a pattern over layer or prefix, and (b) the byte-determined versus receiver-state-dependent partition at `label-taxonomy.md:235-245`. `assignment` is correct for taxonomy hygiene only.

The `inventory.rs` entries carry a concise layer/has_vector rationale POINTING to `label-taxonomy.md` §partition, never restating its content (single-home rule). Four sites move together: `inventory.rs` (`entry("sender_not_assigned", "routing", true, false, None, None)`), `json.rs:179` doc (`codec, crypto, key or assignment`), `frameVectors.ts:124` layer union (`| 'assignment'`), and `rejectReason.ts` (`RejectLayer` gains `'assignment'`, new `assignmentReject()` beside `keyReject()`, token in both the union and `ALL_REJECT_REASONS`).

**Ruling B reconciled — the existing `receivePath.ts:120-124` doc is IMPRECISE and gets broadened.** The premise "a cached transmit key for generation G implies you once held the KEK for G" conflates TWO different generations: `TransmitKeyCache` is keyed by the 8-byte KEY ID (`sender|stream|transmit-key generation`), never by the wrap's `kek_generation`. One transmit key is re-wrapped under each successive KEK, so several wrap blocks for ONE key id exist on the wire, and `matchesCachedWrap` compares BLOCK BYTES. So the accepted case is reachable from both directions:
- NEWER (what the current doc describes): sender got the new KEK push first and re-wrapped the same transmit key under it; our push has not landed. Block differs, `kekForGeneration(newer)` undefined, key id cached -> accepted.
- OLDER (undocumented today, and rarer before retention existed): we cached this key id from a gen-N wrap; a reordered in-flight frame carries the SAME key id wrapped under gen N-2, which retention no longer keeps. Block differs, KEK not held, key id cached -> accepted.
So direction does NOT discriminate the accepted case at all - it only splits the two DROP tokens. Final one-liner for the three sites:
> A wrap announcing a KEK generation this receiver does not hold splits on ONE question: is a usable transmit key for this frame's key id already cached? CACHED -> the frame is ACCEPTED and counted as the wrap outcome `kek_generation_not_held` (reachable from either direction - newer because the sender re-wrapped under a generation whose push has not landed, older because the announced generation has aged out of retention). NOT CACHED -> the frame is DROPPED and counted as a reject reason, and only there does direction matter: `no_kek_for_generation` newer than the newest held, `kek_generation_stale` older than retention keeps.

`receivePath.ts:120-124` is corrected in the same diff (it currently states the newer case as the only cause) and gains the cache-keyed-by-key-id note, so the blessed sentence and its neighbouring doc agree. A test covers the OLDER accepted case, which nothing exercises today.
### paired-test round 2 (items 2-4 adopted; item 1 escalated)
- `kek_generations_retained_total` tests: first install at join does NOT increment (otherwise the counter conflates "installed" with "held a previous" and S6 greens on a client that never retained); every increment assertion is PAIRED in the same test with a previous-generation frame that actually opens; the counter does not decrease after expiry (monotone, not a gauge renamed); the increment lands on the INSTALL, not on first use of the previous generation, so task 19 can use `increase()` over the rotation window.
- Decode-queue eviction gets a SECOND-SEGMENT per-lane identity, since the frame is already counted `accepted` and `decode_queue_dropped_total` reading zero is indistinguishable from an uncounted eviction path: in no-fault scenarios `decoded + evicted + stillPending == acceptedForThatSender` EXACTLY; in the decoder-fault scenario only, the weaker `<=` plus `decoder_errors_total == 1` for that lane (frames in flight in a closing decoder are legitimately lost), with that scoping stated at the site so nobody "fixes" the inequality into an equality. One eviction counted per evicted frame (evict 3, expect +3).
- Exact label KEY SET asserted by SET EQUALITY (not `toContain`) on all six new counters: base labels plus at most the one named discriminator, `key_custody=operator` present, nothing else - so a later sender/meeting label is a red build rather than a silent §11 violation.
- RESOLVED by @observability: KEEP `outcome`. It now carries two disjoint per-metric vocabularies (`floor_substituted|ceiling_clamped|below_rewrap_latency` on anomalies, `conflicting_key|older_generation|malformed` on refusals), so `sum by(outcome)` across client media metrics returns a mixed domain. `label-taxonomy.md:88-90` says to use `outcome` exactly when the value set is metric-local, and the asymmetry with F1 is the point: `reason` is declared one space with two families BECAUSE `sum by(reason)` across the two ends of a hop is an intended query, while `outcome` carries no such contract and `key_wrap_outcomes_total` already contributes a third client-side vocabulary - so the mixed domain predates this diff and nothing regresses. No new label key (that would need the full three-list chain to buy a property the taxonomy does not want). REQUIRED ADDITION, because the question was asked twice in one devloop: a standing-ruling corollary at `label-taxonomy.md:88` in that file's "asked twice, answered once" form - `outcome` is a PER-METRIC vocabulary with NO cross-metric aggregation contract, `sum by(outcome)` across metrics is meaningless by design, and no panel or alert may aggregate it across two metric names. Plus the per-metric domain in each of the three catalog entries.
- ESCALATED to @team-lead, NOT silently deferred: the story doc names this metric as a GAUGE at FOUR sites, and my first count of two was wrong (my grep was truncated by `head`; @paired-test caught it). All four move together or the contradiction lands INSIDE the story doc with the wrong half in the S6 prompt - the shipped-partial-invariant shape:
  - **98** - the "Names fixed by this plan" list, explicitly the single source for tasks that run without each other. Needs gauge -> counter + `_total`.
  - **140** - the observability plan. Same rename, AND the `(1 or 2)` parenthetical must go: a monotone counter has no {1,2} domain, and leaving it is how the gauge reasoning gets reconstructed later.
  - **553** - task 14's browser env-tests prompt, S6: "the rotation is observed on the remaining clients' `dt_client_media_kek_updates_total{source="kek_update"}` and retained-generations gauge". THIS IS THE S6 ASSERTION TARGET the item was raised about and it says "gauge" in so many words.
  - **612** - task 16's F16 runbook prompt: "the client retained-generations and kek-updates series". Neutral wording that survives the rename on its face; in the sweep so it is confirmed rather than assumed.
  553 and 612 sit inside prompts for tasks that HAVE NOT RUN, which is what makes this expensive rather than cosmetic: it is not documentation lagging code, it is an instruction to a future task to assert against a metric that will not exist. The Lead instructed me not to edit `docs/user-stories/2026-09-21-hear-each-other.md` myself, so the Lead makes the edit or authorises mine; the classification row already exists.
  @paired-test's Gate-3 position on each way the Lead can rule, recorded so it is not sprung later: (1) all four sites move -> verified clean; (2) the Lead carves the doc edit out of task 7 -> accepted as a scope ruling by the agent owning the file, ON CONDITION the follow-up is recorded where task 19/20 will read it, NAMING ALL FOUR SITES AND LINE NUMBERS (a tracking entry reading "fix story doc metric name" without the site list reproduces the exact miss that cost two rounds here); (3) PARTIAL - 98 and 140 corrected while 553 still says "gauge" -> Gate-3 finding, shipped partial invariant, verdict held. Outcome 3 is the one that happens by accident rather than by decision, which is why it is named in advance.
### @dry-reviewer A-D
- **A (blocker) was already resolved by Lead ruling** before this arrived: `codec.rs` is Domain-judgment and @media-handler is pulled into the devloop as co-signing reviewer. Its owners are `[protocol, media-handler]` per `cross-boundary-ownership.yaml:34`, confirmed. I am putting its OPTION 2 to the co-owners rather than deciding for them, because its DRY argument is the stronger one: the sentence's CLAIM stays true with the new tokens (it is an incomplete illustration, not a false statement), so the choice is (i) extend the enumeration, (ii) DE-ENUMERATE it - "the cryptography, key and assignment layers add their own tokens; the vectors file is the full set" - which permanently retires the site from every future sweep, or (iii) no edit. I recommend (ii) now that media-handler is on the team; (iii) if the co-owners would rather not spend a GSA hop. Either of (ii)/(iii) is strictly better than (i).
- `crates/mh-service/**` is NOT in the GSA manifest (verified: no `mh-service` key), but Mechanical does not hold there either - no guard catches an omission, and `media_metrics_integration.rs` is the invisible item that stays green if skipped. Both rows upgraded to Minor-judgment. Classification is upgrade-only per §6.2, so set rather than challenged later.
- **B: the `layer` vocabulary has FIVE hand-written encodings and the fifth is the one I would have missed** - the group-divider comments in `rejectReason.ts` at :81/:91/:97 (`// --- codec layer` / `crypto layer` / `key layer`), which need a fourth `// --- assignment layer` divider. All five are now in scope: `json.rs:179` doc, `rejectReason.ts:138` `RejectLayer`, `frameVectors.ts:124` `RejectReasonSpec.layer`, the dividers, and the `inventory.rs` entry. Good news confirmed: `validate-frame-vectors.sh:459` filters `.layer!="codec"`, a NEGATIVE filter, so a new layer value inherits the g16 check automatically rather than escaping it - fail-safe direction, no guard change.
- **C**: done, and it was four sites not three (98, 140, 553, 612 - `:549` is four lines above the S6 text). Lead-authorised, applied, `dt-story validate` clean. S6 at 553 is repointed to `increase()` over the counter, which will exist.
- **D: accepted, with a distinction that matters, because @observability's ruling (1) pulls the opposite way on one site.** Two different kinds of number are in play. (a) DECORATIVE set-correspondence counts sitting beside real enforcement carry no load and are replaced by member-naming per the `gc-service.md:852` ruling ("state the correspondence by naming members, never by restating a count"): `rejectReason.ts`'s header "all sixteen" and `mh-service/src/observability/metrics.rs`'s "all sixteen tokens" both become "every token in `frame-v2.vectors.json`", retiring two sweep sites permanently. (b) The `label-taxonomy.md` partition count is LOAD-BEARING and stays a number: it asserts that the two families EXHAUST the vocabulary, and the sentence immediately after it - that an unclassified token is read as byte-determined and therefore safe to slice - is only sound if the count is exact. So eight-and-eight-is-sixteen becomes eight-and-ten-is-eighteen rather than being de-numbered. `client.md`'s catalog counts are bookkeeping, not a correspondence claim, so they are simply recounted.

### `label-taxonomy.md` §235-260 - @observability specifies, I TYPE, they verify at review
Recorded because @observability and @protocol each said "I carry" this section, which is how a section gets written by neither. Four edits, content supplied by observability:
1. Partition count eight-and-eight-is-sixteen -> eight-and-ten-is-eighteen (the receiver-state-dependent list gains both new tokens); the following unclassified-token sentence stays verbatim.
2. A `kek_generation_stale` bullet in the `replay_detected` form: it fires only when the receiver no longer retains the frame's generation, so a replayed pre-rotation frame reads as "is this receiver still retaining generation N?", and because retention derives from W a positive answer also times the receiver against the last rotation; aggregate counting with no sender dimension is what stops it being a per-sender oracle.
3. A `sender_not_assigned` bullet: decided entirely by receiver-held slot-assignment state, so an injected frame reads as "does this receiver hold sender X in its assignment set?" - the clearest member of the family, since the frame parsed, verified AND decrypted before the gate ran, so nothing about the bytes selects it.
4. The one observability would call a FINDING if absent: append to the "layer is not the discriminator" bullet that `layer:"assignment"` aligning with this family is a COINCIDENCE OF NAMING rather than a rule, that the axis still names the processing STAGE (`codec` bytes, `crypto` verification, `key` held key material, `assignment` held slot-assignment set), that `no_transmit_key` remains the standing counterexample, and to classify by that section and never by `layer`. Adding a semantically-tidy layer value three bullets from the file's own "layer is not the discriminator" warning is a trap with a short fuse.
Relatedly, the `MCMediaMissingKeyMaterial` rule comment says the selector is an EXPLICIT TOKEN ENUMERATION and that a future key-delivery token must be added deliberately - nothing implying a layer or prefix pattern would have caught it, which sits beside the rule's existing note about `dt-guard application-metrics` not matching `dt_client_*` at a word boundary.
### @media-handler: a correctness error in my plan, corrected
My plan said "ten crypto/key tokens", which MISCATEGORISED `sender_not_assigned`, and the reasoning I inherited for barring it was false. Verified in the tree: `MediaDropReason::NoSubscriber` (`metrics.rs:1186`, emitted as `no_subscriber`) is documented as *"A policy is installed but no egress edge names this sender. The remedy is MC's assignment."* That is precisely the server-side analogue of the client's slot-edge gate. So MH is **NOT** "keyless and structurally incapable of observing" an unassigned sender - it already emits exactly that condition - and the crypto/key prohibition's stated reason does not reach this token.
The correct bar is a DIFFERENT one: `sender_not_assigned` is the CLIENT's spelling of a condition MH already names `no_subscriber`, so MH re-spelling it would conflate two drops from different layers under `sum by(reason)`. This is the sharpest instance of the `no_roster_entry`/`no_subscriber` sibling trap the existing doc already flags, and it gets equal care.
Three sites, corrected:
- `metrics.rs:1111-1119`: `kek_generation_stale` joins the crypto/key list -> NINE, not ten. A SEPARATE sentence bars `sender_not_assigned` as the routing/assignment sibling of `no_subscriber`, explicitly NOT under "MH is keyless". "all sixteen tokens" at :1119 -> member-naming per @dry-reviewer D (and eighteen where a count is kept).
- `media_metrics_integration.rs:360-367`: the comment "the eight crypto and key tokens are literals" becomes "the non-codec tokens (crypto, key, and the assignment token `sender_not_assigned`) are literals because they have no `ALL_REJECT_REASONS` home". BOTH literals go into the `.chain([...])` or the `tokens.len() >= required.len()` floor does not pin their spelling - the invisible item.
- `codec.rs:152-156`: see 3(b) below.
**Answer to 3(b): DE-ENUMERATE, which is also @dry-reviewer's preferred option (ii).** The sentence becomes "the cryptography, key and assignment layers add their own tokens; the vectors file is the full set" rather than growing the list. This is the only option satisfying BOTH co-owners' constraints at once: it cannot miscategorise `sender_not_assigned` as crypto/key (media-handler's condition) because it names no token at all, and it retires the site from every future sweep (dry-reviewer's D, and the `gc-service.md:852` name-members-never-counts ruling). Fallback if either co-owner objects to the wider edit: add only `kek_generation_stale` to the crypto/key list and leave assignment to the five `layer` sites, which media-handler already blessed as minimal-correct.
NOTE for @media-handler: the layer value is now `assignment`, not `routing` - observability's taxonomy ruling, confirmed by protocol, after media-handler's message was drafted. Its categorisation argument is unaffected; only the spelling moves.
Classification: both `mh-service` rows already upgraded to Minor-judgment on @dry-reviewer's identical point, before this arrived.
### @operations O-A to O-E
**O-A VERIFIED AND ESCALATED TO @team-lead - it is a real silent-failure path, not a theoretical one.** I checked every link: `infra/services/otel-collector/kustomization.yaml` lists `configmap.yaml` under `resources`, NOT a `configMapGenerator`, so the ConfigMap name carries no content hash; `deployment.yaml` has no `checksum/config` annotation (no `annotations` key at all); `deploy_otel_collector()` (`setup.sh:697-705`) does `apply -k` then `kubectl wait --for=condition=Ready`, which an already-Ready pod satisfies instantly; and setup.sh DOES issue `rollout restart` for ac/gc/mc/mh at :1225/:1252/:1316/:1389 - the collector is the only service that does not get one. otelcol reads `--config` at startup and the file provider does not watch. So a config-only change leaves the pod serving the OLD in-memory filter list while `dt-guard client-metrics-export` reads FILES and goes green: six names marked `Exported: yes`, series still dropped in-cluster. That reproduces exactly the mode the ConfigMap's own comment names ("fails CLOSED AND SILENTLY - indistinguishable from 'no browser is running'"), with the file correct, so neither the guard nor the operator can see it.
My recommendation to the Lead: **option 1 AND option 3 together**, not either alone. Option 1 is two lines in `deploy_otel_collector()` (`rollout restart` + `rollout status`) exactly matching what setup.sh already does for the other four services - trivial and precedented, but `infra/kind/scripts/setup.sh` is infrastructure-owned and there is no @infrastructure on this team, so it is the Lead's call whether to pull them in or rule the row. Option 3 alone is explicitly insufficient (operations' own words) because it leaves the failure live and documents around it. Option 2 (`configMapGenerator` so the Deployment rolls exactly when content changes, as the Prometheus rule files already mount) is the structurally right fix, is task-sized, and I propose it as a spin-out with a TODO entry rather than absorbing it here.
In-loop verification either way: `scripts/otel-collector/acceptance.sh` extracts `config.yaml` from the COMMITTED ConfigMap and `acceptance_driver.py:192` already injects `reason="kek_generation_stale"` as its token-survives case, so it is a real file-level positive control for this diff. It does NOT cover the stale-pod case, which is why O-A stands on its own.
- **O-B accepted**: `docs/runbooks/gc-deployment.md:1598`'s row "any metric outside the 14-name media allowlist" is operator-facing triage prose that drifts on every addition. Taking the SSoT option rather than renumbering to 20: it becomes "outside the media-metric allowlist", with no number for nothing to derive and nothing to check. `client.md:52`'s "before adding a **15th** name" is a TEST sentence, not a count, so it is reworded rather than renumbered.
- **O-C accepted**, and the reason it matters is sharp: `floor_substituted` is the rollback detector, but the person who CAUSES it is doing an MC rollback at 3am, and the explanatory sentence currently lives only in the client catalog. `mc-deployment.md` §Deployment Steps step 0 gains one line saying `kek_rotation_debounce_seconds` is forward- AND backward-tolerant so it imposes NO ordering constraint (explicitly, so nobody infers task 20's SDK-first rule applies); §Rollback Procedure gains the statement that rolling MC below the field flips every live client to the floor and climbs `floor_substituted` fleet-wide, that this is the SUPPORTED path and not an incident, and not to chase it or roll further back. The "log loudly" half reaches a browser console no operator has, so the counter is the only cluster-visible evidence - which is what makes O-A load-bearing for this exact signal: a stale collector pod means the rollback detector does not exist.
- **O-D accepted, including that Arm 2 is now FALSE rather than merely incomplete.** `mc-incident-response.md:2357` asserts "the join response is the only KEK source in this build (`...{source="join_response"}`)" and its numbered check 2 rests on that; this diff adds `source="kek_update"`, so an operator following that step queries a selector excluding the rotation path - the very case the new arm exists for. Fixed here. New Arm 3 for `kek_generation_stale` with its own remedy: Arm 2 is newer-than-held (MC's push has not arrived, so MC delivery path), Arm 3 is older-than-retention-keeps with the remedy `MC_KEK_ROTATION_DEBOUNCE_SECONDS` (`mc-service/configmap.yaml:238` is 60 today, so W/2 = 30s) - still MC-side, which is what keeps the rule's "lives here because the remedy is in MC's KEK-and-roster delivery path" honest and stops Arm 3 looking misfiled. `client-dev-local.md:789` symptom table and `:1374` F15 both get the new reason plus a pointer to the anomaly/refusal counters, since F15 is where a client-side operator lands.
- **O-E accepted, and it is the inverse shape of `floor_substituted`**: if an operator raises `MC_KEK_ROTATION_DEBOUNCE_SECONDS` so W/2 exceeds `KEK_RETENTION_CEILING_MS`, `ceiling_clamped` is PERMANENTLY non-zero on a correctly-configured fleet. So (1) the catalog entry says it is a configuration state, not an incident, and that nothing may alert on it, in the `mh-media.json` unsized-egress-budget framing; (2) a comment at `KEK_RETENTION_CEILING_MS` names `MC_KEK_ROTATION_DEBOUNCE_SECONDS` and the consequence of W/2 exceeding it, because this is a cross-service coupling between an MC env var and a client constant with no guard and no shared home. At W=60 it reads zero today, and that comment is what stops the first operator who raises W reading a permanent non-zero counter as a fault.

### `outcome` corollary carries `[reviewer-only]`
@observability's addendum, raised by @paired-test: `label-taxonomy.md` already tags R1 `[guard-enforced, bypassable]` and R2/R3 `[reviewer-only]` so nobody cites the weaker form as coverage, and the corollary is prose with nothing mechanical behind it. It lands with the `[reviewer-only]` tag and observability's verbatim text, keeping the closing pointer to `docs/TODO.md` §Guard Coverage Gaps, which observability files at verdict time. A standing ruling that reads as enforced is worse than one admitting it is not, because the first stops anyone checking.

### @media-handler co-sign bound on the de-enumeration
Co-sign is bounded to a doc-comment-only change that REMOVES only the enumerated examples ("Signature, decrypt, replay, `no_kek_for_generation` and `no_roster_entry`") and KEEPS both load-bearing pieces: the "structural / parse subset" framing (`codec.rs:152`) and the invariant "No downstream layer may reuse one of these tokens with a different meaning" (`codec.rs:155-156`). That invariant is what the `metrics.rs` prohibition and the collision test enforce downstream, so it must survive. `assignment` is carried through the `metrics.rs` sentence and the test comment too (media-handler's draft wording said routing).
### Gate-1 close-out (Lead rulings) + final verifications
- **Story-doc gauge sites: exhaustively re-verified per Lead ruling 5.** Four sites named the metric and all four are edited (98, 140, 553, 612); `dt-story validate` clean. @dry-reviewer's `:549` is confirmed NOT a site - it is four lines above the S6 text and names nothing. One further hit, `:403`, is task 7's OWN prompt using the word "gauge" inside the counter-not-gauge argument for the violations counter ("with N browsers at one identity a gauge is last-writer-wins"); that is correct as written, is the Lead's task-7 amendment territory, and is deliberately NOT edited.
- **@dry-reviewer's open question answered: TASK 7 EMITS IT**, on the retaining-install path in `MeetingKekHolder`, which is this task's code; no later task would naturally own it. Its read of option (1) is right. The bookkeeping is already in and its "four names" quote is from an earlier revision: the plan has carried SIX new names since @observability's F1/F2 ruling, and `client.md` recounts 14/19 -> 20/25, not 18/23.
- **O-A route 1 (Lead)**: `deploy_otel_collector()` gains `rollout restart` + `rollout status` matching the ac/gc/mc/mh pattern; @infrastructure pulled in as owner. PLUS route 3's runbook half: `gc-deployment.md` §OTel Collector Upgrade Discipline gains a CONFIG-ONLY change path, which it lacks today (steps 1-5 assume an image change and §Rollback only covers the image+ConfigMap atomic unit).
- **@observability's new finding against its own file, accepted**: `client.md`'s header claims the `Exported:` marker is "machine-checked" without qualification, which @operations' O-A shows is true of the COMMITTED files and false of the RUNNING collector - so a metric can be marked exported, pass the guard, and be absent from Prometheus, rendering identically to "no browser is running", the exact ambiguity that same header warns about. The header is qualified with observability's verbatim text: machine-checked AGAINST THE COMMITTED COLLECTOR CONFIG, nothing checks the running collector is serving it, and the marker means *catalogued as exported*, not *observed in Prometheus*. That two-word difference is what the header's own 14-of-19 claim, task 19's read-back and task 16's inventory all rest on.
- **Verification steps state an EXPLICIT collector rollout**, not `apply -k` plus a readiness wait an unchanged pod satisfies instantly - that is mechanism 5 on the review protocol's vacuity list (a step that observes nothing and reports clean). `scripts/otel-collector/acceptance.sh` is RUN in-loop rather than cited, and I will check that `acceptance_driver.py:192`'s injected `reason="kek_generation_stale"` spelling matches what actually lands.
- `configMapGenerator` (O-A route 2) is NOT in this task: @observability costed it and it moves `client_metrics_export.rs`'s `COLLECTOR_CONFIG` path constant with the config file, making it infrastructure machinery plus guard machinery with neither owner on this team. Observability files it with the dependency named.
### Stated intent for the operations/infrastructure edits (so @infrastructure reviews against intent, not inference)
**1. `infra/kind/scripts/setup.sh`, `deploy_otel_collector()`** - after the `apply -k`: `kubectl rollout restart deployment/otel-collector -n dark-tower`, then `kubectl rollout status` on the SAME Deployment. Two things are stated at the site, because the next reader will ask why this one service needs a restart when `apply -k` is declarative:
  (a) the ConfigMap name carries no content hash and the pod template has no `checksum/config`, so nothing in the apply changes the pod spec - the existing pod is already Ready and `kubectl wait --for=condition=Ready` returns immediately against it.
  (b) `rollout status` carries a BOUNDED `--timeout=180s`, matching the convention every other `rollout status` in this file uses (`:1229` ac, `:1256` gc, `:1320`/`:1321` mc-0/mc-1, `:1393`/`:1394` mh-0/mh-1). Unbounded, it blocks forever on a stalled rollout, which in a headless story run converts a fail-loud config rejection into a HUNG TASK - the failure stays real but presents as a runner timeout rather than as a collector problem, which is the diagnosis-destroying form. (@operations' catch.) 180s is also the operationally CORRECT budget, not just the convention, and 120s would be the wrong symmetry: `:703`'s 120s wait budgets for ONE pod becoming Ready, whereas `rollout status` after a restart budgets for a new pod becoming Ready AND THEN the old one terminating (`replicas: 1`, default maxSurge 1 / maxUnavailable 0, and `terminationGracePeriodSeconds: 30` verified at `deployment.yaml:21`) - strictly longer, serialized. Copying the smaller number onto the longer operation manufactures a flaky timeout that fires under node load and looks like a collector fault. A comment at the site states the relationship (rollout budget must exceed readiness budget plus termination grace) so the two differing literals are not later "tidied" into one.
  (c) `rollout status` rather than `wait --for=condition=Ready` is the FAIL-LOUD requirement, and I verified the mechanism: `deployment.yaml` has `replicas: 1` and NO `strategy:` block, so the default RollingUpdate `maxUnavailable` (25% of 1) rounds to 0 - a bad config therefore STALLS the rollout with the OLD pod still Ready, and a `wait` on the label selector would match that old pod and report success over a rejected config. Do not collapse it back to a `wait`.
**2. `docs/runbooks/gc-deployment.md`** - two edits:
  - O-A route 3: a CONFIG-ONLY CHANGE subsection under §OTel Collector Upgrade Discipline, which has no such procedure today (steps 1-5 assume an image change; §Rollback covers only the image+ConfigMap atomic unit). Contents: run `scripts/otel-collector/acceptance.sh` (it reads the COMMITTED ConfigMap, so it validates what actually shipped); the restart is now automatic via `deploy_otel_collector()` but is MANUAL on any cluster not brought up through setup.sh, with the skip symptom named - a name present in the committed allowlist and absent from Prometheus, which reads identically to "no browser is running"; a config-only change is NOT subject to the separate-change-window rule governing image upgrades (that rule exists for R-54 fail-hard-at-init, which a stalling config rollout does not reach); and one line that §Rollback's "image tag and ConfigMap are one atomic unit" still binds - a config-only REVERT is safe, an image revert is not.
  - O-B: `:1598`'s "any metric outside the 14-name media allowlist" loses the literal count entirely ("outside the media-metric allowlist") rather than being renumbered to 20, since nothing derives or checks it and it has now drifted twice. `client.md:52`'s "before adding a 15th name" is a TEST sentence, so it is reworded rather than renumbered.
**3. `docs/runbooks/mc-deployment.md`** - O-C, two sites: §Deployment Steps step 0 gains the note that `kek_rotation_debounce_seconds` imposes NO SDK<->MC ordering constraint (the floor makes the client tolerant in both directions), stated explicitly so nobody generalises task 20's SDK-first rule to this field; §Rollback Procedure gains the consequence - rolling MC below the field flips every live client to the retention floor, `..._anomalies_total{outcome="floor_substituted"}` climbs fleet-wide, this is the SUPPORTED path and not an incident, do not chase it and do not roll further back. Audience is the point: the correct sentence already exists in the CLIENT catalog, but the person who causes the signal is doing an MC rollback.
**O-D confirmed as a full third ARM SECTION, not only a widened PromQL**: Scenario 16 gains Arm 3 for `kek_generation_stale` with its own remedy (`MC_KEK_ROTATION_DEBOUNCE_SECONDS`, 60 today so W/2 = 30s - still MC-side, which keeps the rule's "the remedy is in MC's KEK-and-roster delivery path" honest), AND the `:2357` Arm 2 correction, which is a FALSE statement after this diff rather than merely incomplete ("the join response is the only KEK source in this build") with its numbered check 2 resting on it.
**O-E confirmed as both halves**: the catalog entry carries the "configuration state, never an alert input" framing in `mh-media.json`'s unsized-egress-budget form, AND a comment at `KEK_RETENTION_CEILING_MS` naming `MC_KEK_ROTATION_DEBOUNCE_SECONDS` and the consequence of W/2 exceeding it. That coupling is cross-service with no guard and no shared home, so the comment is the only thing between the first operator who raises W and a permanently non-zero counter read as a fault.
### `client.md` header qualifier - SUPERSEDED TEXT, use this version
@observability replaced its own first draft after @operations caught that it would ship FALSE in this same story. Two defects in the original: (i) it said "the deploy's readiness wait is satisfied instantly by an already-Ready pod", which route 1 makes false in the very diff that ships it - writing a stale premise into a catalog header while fixing a different stale premise in the same file; (ii) its closing "until then, a collector ConfigMap edit requires an explicit pod restart" named the remedy in a way that invites the next reader to narrow the gap to "except on clusters not deployed through setup.sh", reading as a CLOSED exception when it is an OPEN class.
The key distinction: **route 1 closes the COMMON PATH, not the CLASS.** A bare `kubectl apply -k`, a hand-edited ConfigMap, or a pod rescheduled without a restart each still serve stale config with the committed file correct and the guard green. So the honest form is the unqualified gap plus a pointer, structurally like the header's existing five-controls absence paragraph.
Final text (the closing bolded sentence must survive verbatim; the text describes the restart as EXISTING rather than as a remedy-to-come, so it stays true whichever order the two hunks land in):
> ...authoritative and machine-checked **against the committed collector config** (`dt-guard client-metrics-export` asserts set-equality against the name allowlist in both directions, so a metric marked exported and absent from the collector, or vice versa, is a red build). **Nothing ties that green to the collector that is actually running.** The ConfigMap is not content-hashed, so no mechanism makes a config edit and a pod restart one unit: the scripted deploy restarts the collector, but a bare `kubectl apply -k`, a hand-edited ConfigMap, or a pod rescheduled without a restart all leave the previous filter list in memory with the committed file correct and this guard green. A metric marked `Exported: yes` that is absent from Prometheus for that reason renders identically to "no browser is running" - the same ambiguity this block warns about below, reached by a different route. **So this marker means *catalogued as exported*, not *observed in Prometheus*.** The content-hashed-ConfigMap fix that would close the class is filed in `docs/TODO.md` (owner operations).

### `docs/TODO.md`: I write NEITHER entry
Two distinct entries, neither mine, recorded so I do not reconcile them into one or double up: @operations owns the collector/`configMapGenerator` entry (their finding, their tracking, per the spin-out rule) including the guard-path dependency, the `deployment.yaml:156` nameReference detail, the `33_alert_rules_loaded.rs` precedent and the route-1 residual; @observability files a SEPARATE entry under §Guard Coverage Gaps for the `by (outcome)` aggregation guard. My `docs/TODO.md` row stays for the D5 oracle-list addition only.
### @dry-reviewer: accounting-boundary note + `outcome` const maps
- `decode_queue_dropped_total` carries an explicit ACCOUNTING-BOUNDARY paragraph at its definition site, in the established idiom of the `wrap_key_id_mismatch` non-dropping note and the `MEDIA_SEND_DROP_REASONS`-versus-`rejectReason.ts` header: it measures the ACCEPTED -> AUDIBLE segment, DOWNSTREAM of the accounting boundary, and is deliberately NOT a `frames_dropped_total{reason}` value. The hazard is specific and plausible: a counter literally named `..._dropped_total` sitting one file from the drop counter reads like a `reason` someone forgot to add, and a well-intentioned "fix" folding it in would put frames already counted `accepted` on both sides of `received = accepted + sum(drops by reason)` - breaking the identity silently and only in aggregate, which is exactly what `wrap_key_id_mismatch`'s `drops_frame: false` exists to prevent.
- All three metric-local `outcome` vocabularies stay bounded `MEDIA_*`-style const maps in `mediaMetrics.ts` (anomalies, install refusals, and the existing reported `WrapOutcome` set), never inline literals, so their disjointness is inspectable in one place. Legitimate per `label-taxonomy.md`'s metric-local definition, and disjoint so no cross-metric `sum by(outcome)` can silently merge them.
### @infrastructure ruling on `setup.sh` (items 1-6) + @operations' correction to item 6
**1. CONDITIONAL via the existing `apply_reports_configmap_changed` detector (`setup.sh:1203`), NOT an unconditional restart.** Accepted, and it is better than what I planned: a second competing restart convention in one script is the drift CLAUDE.md bars, and unconditional costs two collector pods at 512Mi *request* each (Guaranteed QoS) on one Kind node during the rollout, for a fresh bring-up where the apply reports `created` and the pod already carries the new config. Shape matches `deploy_gc_service`, keeping the declare-then-assign split (`local apply_out` on its own line) because `local x=$(...)` masks the apply's exit status under `set -e`.
**2. `rollout status --timeout=180s` REPLACES the `wait --for=condition=Ready` at `:703` - not both.** A label-selector Ready wait is satisfied by the OUTGOING pod, so keeping it in front of `rollout status` leaves a fail-open step in the gate for no benefit; `rollout status` is also more robust on the fresh-create path (no "no matching resources" race). 180s confirmed by both owners, with the budget-relationship note at the site.
**3. Two stale comment blocks updated in the same hunk**: (a) `:1183`'s "SCOPE IS THE FOUR APPLICATION SERVICES (AC, GC, MC, MH). Deliberately NOT postgres or redis" becomes false the moment the collector uses the detector - the collector joins with its reason (stateless, no connection pool or migration to race, `--config` read once at startup with no file watcher) and joins AC/GC/MC on the backstop question as having NO env-test staleness twin, with the uniquely bad silent-stale symptom. (b) `:690`'s "THE READINESS GATE IS LOAD-BEARING NOW ... Do not shorten or skip this wait" now points at a wait that no longer exists; restated against `rollout status`, adding that a ConfigMap the pinned 0.161.0 rejects now STALLS whole-cluster bring-up loudly rather than being silently ignored - the intended trade, not a regression. Ordering hazard checked by @infrastructure: `deploy_otel_collector` runs before AC/GC/MC in `main()`.
**4. A REAL CATCH that invalidates a control I had recorded.** `acceptance_driver.py:192` injects `reason="kek_generation_stale"` on `dt_client_media_frames_dropped_total` - an ALREADY-allowlisted NAME. It proves the reason token survives `keep_keys` and proves NOTHING about the six new allowlist ENTRIES. @operations and I had both cited it as this diff's positive control; for the names it is not one. A presence check for a genuinely new NAME (`dt_client_media_kek_retention_violations_total`) is added, paired with the existing `(a) unlisted name absent` negative control at `:198`, so the allowlist edit has a control that actually runs and BOTH directions are covered by the driver: an added name arrives, an unlisted name does not.
  **ONE name is sufficient, and the reasoning is recorded so nobody later "completes" it to six** (@operations): the driver check proves the MECHANISM - that a newly-added entry in `filter/client_metric_names` passes the strict include and reaches :8889 - and six checks would test one mechanism six times. The other five rest on the guard chain, whose bounds I verified: `export_set_mismatch` (`client_metrics_export.rs:82`) pins the catalog markers set-equal to the collector list in BOTH directions, and `exported_without_emitter` (`:84`) pins each name to an actual emitter in the TypeScript source - so the five are covered against DIVERGENCE between the three encodings.
  **Known residual, stated as a bound rather than left to be found later**: a CONSISTENT misspelling across all three encodings (emitter, catalog marker, collector list) is self-consistent and passes the whole chain, because `exported_without_emitter` checks that an emitter with that name exists, not that the name is the intended one. Accepted, and the distinction is why: that is a REVIEW-VISIBLE CONTENT error (a wrong name sits in a catalog entry a human reads) rather than a SILENT MECHANISM failure (a name correct everywhere and dropped in the cluster, which is O-A). Different class, different control - the reviewer is the right control for the first, and no number of driver cases would be.
**5. Stream budget computed with the real value sets** (worst case, all reject reasons observed; per per-run `org_id` with `client_version`/`key_custody` fixed; a histogram is one stream): **14 names / 34 streams -> 20 names / 47 streams, +38%**. Against `max_streams: 500` that is **~14 -> ~10 concurrent runs** of headroom. @infrastructure's rough figure was ~40 -> ~50, so materially the same and comfortably above its "tell me if headroom drops below a few runs" bar - no escalation. Recorded at the `max_streams` site WITH ITS DENOMINATOR per `deployment.yaml`'s own quote-the-denominator rule. Realistic counts are well below worst case, since most reject reasons never fire in a run.
**6. Verification, in the CORRECTED form** (@operations caught that @infrastructure's original grep-only control was fail-open in the same direction as O-A itself; @infrastructure agreed and superseded it, and I verified the mechanism: `deployment.yaml:143-159` mounts the ConfigMap at `/etc/otelcol-contrib` via `configMap:` + `items:` with NO `subPath`, and only `subPath` mounts are frozen at pod creation - so the kubelet resyncs the projection in place and `grep <new name> .../config.yaml` PASSES on a pod that never restarted, i.e. on exactly the stale state being detected). `rollout status` supplies no identity evidence either: with no rollout in progress it returns "successfully rolled out" immediately, so it cannot distinguish "restarted and converged" from "the branch never fired". The sound form, agreed word-for-word between operations and infrastructure:
  - pod uid via `kubectl get pod -l app=otel-collector -n dark-tower -o jsonpath='{.items[*].metadata.uid}'` captured BEFORE THE APPLY and again after, asserted changed (before the apply, so a pod replaced by the apply itself is covered);
  - the new pod's `status.startTime` asserted to postdate the ConfigMap's last-change timestamp (`--show-managed-fields`, the same source `crates/env-tests/tests/01_mh_deployment_config.rs` uses);
  - THEN the grep against the new pod's mounted file.
  Pod-identity change is structural and fail-closed; the grep is neither alone. Together they license "the process started after the content was written".
  **A green layer 7 is SILENT about this whole mechanism** and that goes in the runbook subsection as well as the verification row: a fresh cluster yields `created`, not `configured`, so the restart branch never fires. Layer 7 is what someone will reach for to confirm O-A is fixed, and it cannot confirm it - an operator who edits the ConfigMap and reasons "layer 7 was green" has learned nothing about whether their change is live.
  **HOST-SIDE REMAINING ACTION** (not doable from this container): `infra/kind/scripts/setup.sh --only otel`, because `crates/devloop-helper/src/protocol.rs:22`'s `Service` enum is `{Ac,Gc,Mc,Mh}` so `dev-cluster deploy otel` does not exist. Verified.

### `capture_source` scope answer (@infrastructure's question)
It lands in **task 13**, not here - Lead ruling, and task 13's prompt is being amended to carry all six of its export sites (GC `MEDIA_DATAPOINT_EXTRA`, collector `keep_keys`, the collector charset shape check, the collector name allowlist, the `Exported:` marker, and the `mode` row in `label-taxonomy.md`). So it is re-homed, not dropped: its pre-ruled `Exported: yes` decision ships with its emitter. Consequence for this hunk: **`keep_keys` stays exactly seven keys and the two-owned-groups framing at ~`:166` is untouched here** - no eighth key, no recount. @observability pre-ruled the membership (`mode` in {`microphone`,`test_tone`}, bounded and identity-free) so task 13 need not re-ask.

### `docs/TODO.md` - one infrastructure entry, pasted verbatim
@infrastructure reduced its ask to ONE entry (the collector-staleness window is @operations', who is folding the detector option into it - no parallel entry). The infrastructure-owned entry is pasted as given, covering the missing `--only otel` route from inside the container, why it is task-sized (`Service` also drives `ALL`, `as_str()`, `image_tag()` and pod-health/port-map paths that all assume a first-party buildable image), and that `setup.sh` itself DOES accept `--only otel` so the gap is purely the helper's enum and NDJSON surface.
Plus the RECIPROCAL cross-reference sentence @infrastructure supplied, so the pair points both ways (@operations adds the mirror to theirs; a one-way reference is how the second entry is orphaned when the first closes - `TODO.md:488`'s own lesson): *"Pairs with the collector config-staleness-window entry (owner: operations) - until an `otel` route exists, every collector ConfigMap edit inherits this host-side gap, so the two want one owner and one sitting; task 13's collector ConfigMap edit is the next trigger for both."*
Two implementation notes from @infrastructure, neither a new requirement: (i) the `local apply_out` split is NOT stylistic - `local x=$(...)` makes `local` the command whose exit status `set -e` sees, so a failing apply would be swallowed; the existing explanation at `deploy_ac_service` covers it and is not repeated at the collector site. (ii) When restating the `deploy_otel_collector()` readiness paragraph, KEEP the existing sentence explaining why the gate is ordered before AC/GC/MC (R-54 fail-hard-at-init) - unchanged by this edit, and the reason the restart is safe here at all; a reader who loses it may later think the collector deploy can be reordered.

---

## Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security (paired) | confirmed (GSA co-owner confirm for vectors) |
| Test (paired) | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Semantic Guard | confirmed |
| Protocol | confirmed (GSA owner confirm for vectors) |
| Media Handler (GSA co-owner codec.rs, added) | confirmed (GSA co-sign codec.rs) |
| Infrastructure (Domain-judgment owner setup.sh, added) | confirmed |

---

## Implementation Summary

Implemented as planned; deviations below, each forced by a finding during implementation rather than chosen.

**Delivered.** Per-sender decode lanes with a slot-edge gate (verify -> gate -> open; authority MC's `StreamAssignments`, declared slots only) and mixing playback lanes; `MeetingKekHolder` (current + <=1 previous, W-derived retention from the demoting message, timer + lazy-backstop expiry, zeroize on every drop, current-only send side, K1/K2 install rules); R-13 rotate-on-KEK-receipt (after install, never on equal); R-18 synchronous rebind decision with sequence-guarded import, transmit-key purge, replay state untouched; leave removes the roster key; two reject tokens via the generator (`kek_generation_stale` layer `key`, `sender_not_assigned` layer `assignment`); six exported counters with collector allowlist + catalog markers; `MCMediaMissingKeyMaterial` widened with every restatement site; collector restart in `setup.sh` via the existing detector with bounded `rollout status`.

**Deviations from the plan, and why.**
1. **The retention tripwire's live-path test was VACUOUS as first written, and a mutation proved it.** Counting beside the guard call meant deleting only the call left the count advancing and all 32 tests green. Fixed structurally: `RetentionGuard` fuses the count into `evaluate()`. Re-mutated: deleting the call site now reds the live-path test; gutting enforcement reds the fabricated-state test.
2. **The retained counter was incremented by the code path, not the held state** — so a "retain nothing" refactor would have kept it climbing. Now counted from `#held`; under that mutation the counter test reds.
3. **`receiveTransports.ts` and `rosterKeyFeed.ts` extracted** because `dt-guard`'s `ts_retained_credentials` requires its scan cap >= 3x the largest real declaration: `AudioPipeline` reached 723 lines, then `SignalingClient` 672. Split rather than raising the cap (a guard-policy change owned elsewhere); both are genuinely separable responsibilities.
4. **`session/mediaWiring.ts`** holds the rebind listener and KEK observer, so the integration test drives the SAME wiring the session installs rather than a copy.
5. **Removed dead state** (`#slotSenders` in `receiveLanes.ts`), caught by lint: it was written, never read, and documented as "THE authority for the gate" — a comment that would have sent a reader to the wrong place. The authority is the lane set.
6. **The "unimportable 32-byte key" row was dropped, not faked.** On Node 22 every 32-byte value — including a point not on the curve — imports as a raw Ed25519 key. Replaced with the property that branch exists for: such a key never lets a frame through (dropped as `signature_invalid` here, `no_roster_entry` on a rejecting platform).
7. **Replay precedence surfaced by a test ordering error**: a pre-rotation frame arriving AFTER a newer transmit generation is `replay_detected` (the per-(sender, stream) generation high-water runs before the KEK lookup). Correct precedence; the joiner test now delivers in the realistic order and says why.
8. **`PlaybackSink` is a breaking seam change** (`enqueue` -> `openLane()`): a single playhead serialises N senders with unbounded latency growth. No consumer outside sdk-core/test-utils.
9. **Runbook corrections beyond plan, all from review during implementation**: the collector verification substitutes a ConfigMap-data read for the file grep (the image is distroless) and names the stale-but-valid-projection residual; the behavioural-probe hazard is stated as DENOMINATOR dilution (a numerator-only injection cannot fire the ratio alert — an earlier sentence of mine said it could, and was wrong); §Rollback and §Mitigation are scoped to image vs config-only changes (the latter was a direct contradiction), with a generalising note.

**Host-side remaining action (cannot be done from the devloop container):** `infra/kind/scripts/setup.sh --only otel`. The live Kind collector is still on the OLD config (new names absent from its ConfigMap). The devloop helper's `Service` enum has no `otel` variant (TODO filed). A green Layer 7 does NOT prove this is live — a fresh cluster reports `created`, not `configured`, so the restart branch never fires there.

---

## Devloop Verification Steps

### Gate 2 (Lead) — `DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh`, attempt 1: PASS
`L1 OK · L2 OK · L3 OK (114s) · L4 N/A (cargo+nx tests passed; proto placeholder N/A) · L5 OK · L6 N/A (audits passed; buf-breaking passed) · L7 OK (env-tests-passed, browser-e2e-passed; 1125s)` — `TOTAL_RESULT=N/A`, exit 0. N/A layers are wrapper-self-justified (`not-applicable-to-this-lang`, `audit-aggregate-na`).
**Not evidenced by Gate 2 (remaining host-side action at merge):** the collector `configured` → `rollout restart` branch in `deploy_otel_collector()` — a fresh Layer-7 cluster reports `created`, and there is no in-container otel deploy route. Run `infra/kind/scripts/setup.sh --only otel` on the host against an existing cluster and check pod UID change + `startTime` postdating the ConfigMap change.

### Post-review re-validation (Lead)
- Full `layer-all.sh` re-run after Gate-3 fixes: L1/L2/L4/L5/L6 OK or self-justified N/A, **L7 OK (env-tests-passed, browser-e2e-passed)**; L3 reported `run-story-selftest-failed` although the self-test printed `493 passed, 0 failed`. The run overlapped teammates' concurrent comment-only edits (protocol + implementer, three-site non-collapse note). The failure **did not reproduce**: `scripts/workflow/run-story.test.sh` standalone → exit 0 (493/0), and `layer1..6.sh` re-run on the final tree → all OK / N/A, exit 0. This diff touches nothing under `scripts/workflow/`. Changes after the L7 run were comment-only (typecheck + 839/839 tests re-run by implementer and protocol).
- **Final full `layer-all.sh` on the frozen, staged tree: PASS** — L1 OK · L2 OK · L3 OK · L4 N/A · L5 OK · L6 N/A · L7 OK (922s) · `TOTAL_RESULT=N/A`, exit 0. This is the verdict the commit rests on.

- **REMAINING HOST-SIDE ACTION, and no guard or test can show it is undone**: this diff edits `infra/services/otel-collector/configmap.yaml`, so the RUNNING collector must be rolled before the six new names pass its allowlist. Every guard and test here is green either way, because they all read FILES — which is exactly the gap @operations' O-A TODO entry describes. Run `infra/kind/scripts/setup.sh --only otel` on the host, then the three-leg check in `docs/runbooks/gc-deployment.md` §Config-only changes (pod uid before/after the apply, `startTime` postdating the ConfigMap change, then the ConfigMap-data grep). The live Kind collector was still on the OLD config at the end of this loop. A green Layer 7 does NOT show this: a fresh cluster reports `created`, not `configured`, so the restart branch never fires there.
- `./scripts/layer-fast.sh`: EXIT 0. L1 OK, L2 OK, L3 OK, L4 N/A, L5 OK, L6 N/A. **L4's N/A is not a masked failure**: both applicable runners report `cargo-test-passed` and `nx-test-passed`; one language reports `not-applicable-to-this-lang`, and `aggregate_worst_status` ranks N/A above OK by design.
- sdk-core: `tsc --noEmit` 0 errors; `eslint src/` clean; vitest **60 files / 830 tests** (baseline was 55 / 714). Prettier clean on sdk-core and test-utils.
- Vectors: regenerated with `cargo run -p media-vector-gen --bin generate-frame-vectors` (+2 entries only, hand-edit free); `validate-frame-vectors.sh` passes.
- Guards run individually: `client-metrics-export`, `knowledge-index` (client INDEX at its 75-line cap), `validate-cross-boundary-scope`, `ts_retained_credentials` tests (38), `mh-service` collision test (14), `gc-service` telemetry_filter tests (26), `acceptance.sh` against the committed collector config including the new-name presence check (red with the name removed).
- **Mutation checks, each run and restored** — the suite must fail when the property is removed:
  | Mutation | Result |
  |---|---|
  | Delete the R-13 rotate-on-KEK subscription | 2 tests red (R-13, S7 leaver) |
  | Disable the slot-edge gate | 1 red |
  | Collapse `kek_generation_stale` into `no_kek_for_generation` | 3 red |
  | Retain nothing on demotion | 15 red, incl. continuity and the retained counter |
  | Delete the retention guard's live call site | 1 red (was 0 before the fusion fix) |
  | Gut enforcement inside the guard, keep its count | 1 red |

---

## Code Review Results

### Gate 3 verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security (paired) | RESOLVED-FIXED | 6 | 6 | 0 | R-13 subscribe-before-capture; sender-id remap removes retired key; malformed-width KEK counted; `kek_generation_not_held` reasoning corrected (5 sites); rotate() count; `conflicting_key` basis recorded. GSA co-sign for vectors confirmed. Verified purge mutants red. |
| Test (paired) | RESOLVED-FIXED | 5 | 5 | 0 | R-18 purge made observable (keyBearing:false probes; 5 mutants red); KEK-switch ordering via sequence payloads; derived decoder-fault counts; FakeAudioCodecs self-test; downgrade probe. |
| Observability | RESOLVED-FIXED | 4 | 4 | 0 | threshold-direction note; task-16 prompt gains operator surfaces for new counters (Lead-authorised); false "raise W" remedy corrected (W=60 already at 30s ceiling); install-refusals wording. |
| Code Quality | CLEAR | 0 | 0 | 0 | Ownership Lens PASS for GSA/Domain-judgment rows. |
| DRY | RESOLVED-FIXED | 1 | 1 | 0 | stale "sixteen" token counts de-numbered (3 sites). Extraction opportunity filed (not a deferral). |
| Operations | RESOLVED-DEFERRED | 7 | 6 | 1 | O-A..O-E + inert Arm-3 remedy fixed; O-A route 2 (content-hashed collector ConfigMap) spun out. Also fixed pre-existing R-55→R-54 citation in ac-service-deployment.md. |
| Semantic Guard | CLEAR (SAFE) | 0 | 0 | 0 | Client Credential Lifetime, Credential Leak 5-10, TOFU naming: clean. |
| Protocol (GSA owner) | CLEAR | 0 | 0 | 0 | Vectors regenerated byte-identical from inventory.rs; `assignment` layer propagated to all sites. |
| Media Handler (GSA co-owner) | CLEAR | 0 | 0 | 0 | codec.rs co-sign granted; MH prohibition list + test chain correct (floor 18). |
| Infrastructure (Domain-judgment owner) | RESOLVED-FIXED | 5 | 5 | 0 | 4 planning + 1 Gate-3 (production-port caveat for collector). Ownership Lens: owner present in plan + review. |

**RESOLVED-DEFERRED present**: Operations (one accepted spin-out — see §Accepted Deferrals).

**Remaining host-side action at merge (not verification performed):** `infra/kind/scripts/setup.sh --only otel` against an existing cluster; confirm pod UID change and `status.startTime` postdating the ConfigMap change (procedure: `docs/runbooks/gc-deployment.md` §Config-only changes). Until then the running collector drops the six new metric names. Gate 2 / Layer 7 cannot evidence the `configured` → restart branch.

---

## Accepted Deferrals

- `docs/TODO.md` §Infrastructure Validation in Devloops — collector config-staleness window (O-A route 2, content-hashed ConfigMap)

---

## Rollback Procedure

1. Start commit: `21228100d648501cb6b79c98111b17b2e703c005`
2. `git diff 21228100..HEAD`; `git reset --soft|--hard 21228100`

---

## Issues Encountered & Resolutions

**A worked example for "can a selector change wait for the downstream task?" (@observability).** At Gate 1, observability argued the `MCMediaMissingKeyMaterial` widening had to land HERE rather than in task 16, on principle: a coverage reduction introduced by this diff is repaired in this diff. Gate 3 produced the concrete instance. The `older_generation` KEK-epoch mismatch drops frames as `kek_generation_stale`, which is inside the alert ONLY because this diff widened it. Had the widening waited for task 16, that failure would have been invisible to the key-material alert during the gap. (Honest scope: that path is unreachable with today's client, which has no reconnect; the point is the rule, and this is the example of why it holds.)

**Reviewer-found defects in my implementation (all fixed, each with a test that fails on the old code):**
- The retention-tripwire live-path test was vacuous (count beside the call); fused into `RetentionGuard.evaluate`.
- The retained counter was counted by code path, not held state.
- The R-13 rotation subscription came AFTER `capture.start`, leaving a mint window where a KEK demotion never rotated (@paired-security F1).
- A participant re-issued a new sender id left the old key resolvable until LRU eviction (@paired-security F2).
- A non-empty wrong-width KEK was scrubbed but never counted (@paired-security F3).
- The R-18 purge was unobservable above the cache unit: key-bearing frames re-unwrap after a purge, so two purge mutants survived the whole suite (@paired-test F1). Fixed with `keyBearing: false` probes, including the forget path and a "purge only on rebind" mutant.
- An ordering assertion could not observe order (@paired-test F2), and a hand-typed expectation had a vacuous bound (@paired-test F3).

**Reviewer-found false statements in text I wrote (all corrected):**
- The `kek_generation_not_held` doc claimed honest senders re-wrap one key id under successive KEKs. This SDK never does (wrap once at mint; rotate on every KEK change), so the outcome signals a non-conforming sender, not rotation lag (@paired-security F4). Corrected at every site that made the claim: `receivePath.ts`, `client.md`, the dashboard, and two tests.
- Arm 3, `alerts.md` and `client.md` said raising W lengthens retention. At the default W=60, W/2 equals the 30 s ceiling exactly, so it changes nothing (@operations, @observability F-3).
- The runbook said a numerator-only injection could fire the ratio alert. It cannot; the real hazard is denominator dilution.

**A real design gap found, not reachable today, filed as a TODO with its constraint**: a KEK holder outliving the MC actor that issued its KEK would turn K1's equal-generation refusal into permanent silent deafness. That is unreachable only because the client has no reconnect; the constraint recorded for the reconnect story is that a new epoch means a new session (@operations, @paired-security F6).


