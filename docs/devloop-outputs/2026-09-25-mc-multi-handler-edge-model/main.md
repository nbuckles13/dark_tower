# Devloop Output: MC multi-handler edge model (story 2 task 20, corrective)

**Date**: 2026-09-25
**Task**: Replace task 6's one-handler-per-participant round-robin placement with the ADR-0036 §9 shared-handler edge model driven by MH-observed connectivity (story 2 task 20; R-33 rev. 2026-09-24, R-2, R-4)
**Specialist**: meeting-controller
**Mode**: Agent Teams (v2) — full, Gate-1 present
**Branch**: `feature/hear-each-other`
**Duration**: iteration 1 (2026-09-25, ended in L7 escalation) + iteration 2 operator restart 18:06Z–21:47Z

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `8dd3afd906160af258bdfa9494ebc18a5fda915e` |
| Branch | `feature/hear-each-other` |
| Lead Model | `claude-opus-5-5` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` |
| Implementing Specialist | `meeting-controller` |
| Tier | `full` |
| Iteration | `2` (operator restart) |
| Security | `security` (paired — connectivity source, redirect-primitive invariant) |
| Test | `test` (paired) |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Semantic Guard | `semantic-guard` |
| Paired Client | `paired-client` |
| Paired Media Handler | `paired-media-handler` |

---

## Task Overview

### Objective
Task description: exact contents of `/home/dev/.cache/devloop/story-runs/story-runner/2026-09-21-hear-each-other/task-20.prompt` (story `docs/user-stories/2026-09-21-hear-each-other.md`, task 20).

### Scope
- **Service(s)**: MC (primary), client sdk-core (verify/fix multi-target send + multi-transport receive), env-tests, docs/runbooks
- **Schema**: No
- **Cross-cutting**: Yes

### Debate Decision
NOT NEEDED — model fixed by ADR-0036 §9 and story R-33 (rev. 2026-09-24).

---

## Cross-Boundary Classification

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `crates/mc-service/src/media_routing/slots.rs` | Mine | — |
| `crates/mc-service/src/media_routing/edges.rs` (new: edge-handler chooser) | Mine | — |
| `crates/mc-service/src/media_routing/connectivity.rs` (new: observed connectivity, tombstone, settle window) | Mine | — |
| `crates/mc-service/src/media_routing/placement.rs` (round-robin removed; resolve and ConnectedHandlers) | Mine | — |
| `crates/mc-service/src/media_routing/mod.rs` | Mine | — |
| `crates/mc-service/src/media_routing/generation.rs` (doc) | Mine | — |
| `crates/mc-service/src/actors/meeting_media.rs` | Mine | — |
| `crates/mc-service/src/actors/meeting.rs` | Mine | — |
| `crates/mc-service/src/actors/messages.rs` | Mine | — |
| `crates/mc-service/src/actors/controller.rs` (registry teardown removed) | Mine | — |
| `crates/mc-service/src/actors/participant.rs` (mh_statuses doc) | Mine | — |
| `crates/mc-service/src/webtransport/connection.rs` | Mine | — |
| `crates/mc-service/src/webtransport/server.rs` | Mine | — |
| `crates/mc-service/src/grpc/media_coordination.rs` | Mine | — |
| `crates/mc-service/src/grpc/mh_client.rs` (test fixture) | Mine | — |
| `crates/mc-service/src/grpc/gc_client.rs` (test fixture) | Mine | — |
| `crates/mc-service/src/mh_connection_registry.rs` (deleted) | Mine | — |
| `crates/mc-service/src/main.rs` | Mine | — |
| `crates/mc-service/src/lib.rs` (compile_error clause trimmed, needle line kept) | Mine | — |
| `crates/mc-service/src/config.rs` (MC_MEDIA_CONNECT_SETTLE_MS) | Mine | — |
| `crates/mc-service/src/media_admission/binding_response.rs` (registry_full retired) | Mine | — |
| `crates/mc-service/src/media_signaling/mod.rs` | Mine | — |
| `crates/mc-service/src/media_signaling/assignments.rs` | Mine | — |
| `crates/mc-service/src/media_signaling/directive.rs` | Mine | — |
| `crates/mc-service/src/observability/metrics.rs` | Mine | — |
| `crates/mc-service/Cargo.toml` (rand dev-dependency, workspace crate) | Mine | — |
| `crates/mc-service/tests/**` | Mine | — |
| `crates/mc-test-utils/src/media.rs` | Mine | — |
| `docs/specialist-knowledge/meeting-controller/INDEX.md` | Mine | — |
| `docs/TODO.md` (task-6 entry rewritten in place; O-11 entry; user_ambiguous note) | Mine | — |
| `Cargo.lock` (regen) | Not mine, Mechanical | infrastructure |
| `proto/dark_tower/internal/v1/internal.proto` (connection_id; author paired-protocol; security GSA reviewer) | Not mine, Domain-judgment | protocol |
| `proto/dark_tower/signaling/v1/signaling.proto` (comment text only, eight sites incl. a widened MUST trigger set; author paired-protocol; security GSA reviewer) | Not mine, Domain-judgment | protocol |
| `crates/proto-gen/tests/internal_roundtrip.rs` (roundtrip pin; author paired-protocol; security GSA reviewer) | Not mine, Domain-judgment | protocol |
| `crates/mh-service/src/grpc/mc_client.rs` (connection_id threading; author paired-media-handler) | Not mine, Domain-judgment | media-handler |
| `crates/mh-service/src/webtransport/connection.rs` (connection_id threading and decline-rationale text; author paired-media-handler) | Not mine, Domain-judgment | media-handler |
| `crates/mh-service/src/observability/metrics.rs` (registry_full doc; author paired-media-handler) | Not mine, Minor-judgment | media-handler |
| `crates/mh-service/tests/**` (connection_id pins; author paired-media-handler) | Not mine, Domain-judgment | media-handler |
| `infra/services/mh-service/configmap.yaml` (comment text only; author paired-media-handler) | Not mine, Minor-judgment | media-handler |
| `crates/env-tests/tests/27_mc_slot_placement.rs` (rework: real partial connections) | Not mine, Minor-judgment | test |
| `crates/env-tests/tests/28_mh_egress_admission.rs` (sizing model-1 clause retired) | Not mine, Minor-judgment | test |
| `crates/env-tests/src/fixtures/mc_session.rs` (fallible try_mh_connect) | Not mine, Minor-judgment | test |
| `crates/env-tests/tests/29_mh_meeting_teardown.rs` (iteration 2: release proven by MH direct replies; shared-gauge value assertions removed) | Not mine, Minor-judgment | test |
| `crates/mh-service/tests/end_meeting_integration.rs` (iteration 2: exact edge/registered-meeting accounting after release) | Not mine, Minor-judgment | media-handler |
| `crates/env-tests/src/fixtures/metrics.rs` (iteration 2: `poll_until_pinned_instance` doc — never assert a pod-wide occupancy gauge value; doc only) | Not mine, Minor-judgment | test |
| `docs/observability/dashboards.md` (Gate 3 obs F5: MC Overview per-panel list replaced by row names; O-21 closed) | Not mine, Minor-judgment | observability |
| `docs/runbooks/mh-deployment.md` (Gate 3 ops F4: `connection_id` rollback paragraph + 30-minute check line) | Not mine, Minor-judgment | operations |
| `infra/grafana/dashboards/mh-media.json` (Gate 3 ops F1c: fleet-wide skew note in the installed-streams panel description) | Not mine, Minor-judgment | observability |
| `infra/services/mc-service/configmap.yaml` | Not mine, Minor-judgment | infrastructure |
| `infra/services/mc-service/mc-0-deployment.yaml` | Not mine, Minor-judgment | infrastructure |
| `infra/services/mc-service/mc-1-deployment.yaml` | Not mine, Minor-judgment | infrastructure |
| `docs/observability/metrics/mc-service.md` | Not mine, Minor-judgment | observability |
| `docs/observability/metrics/mh-service.md` (author paired-media-handler) | Not mine, Minor-judgment | observability |
| `docs/observability/metrics/client.md` (frames_sent unit and not_connected arm) | Not mine, Minor-judgment | observability |
| `docs/observability/slos.md` (settle-window floor) | Not mine, Minor-judgment | observability |
| `docs/observability/dashboard-conventions.md` (author observability) | Not mine, Minor-judgment | observability |
| `docs/observability/label-taxonomy.md` (author observability) | Not mine, Minor-judgment | observability |
| `infra/grafana/dashboards/mc-overview.json` | Not mine, Minor-judgment | observability |
| `infra/grafana/dashboards/mh-overview.json` (author paired-media-handler) | Not mine, Minor-judgment | observability |
| `infra/grafana/dashboards/client-media.json` (legend wording) | Not mine, Minor-judgment | observability |
| `docs/runbooks/mc-incident-response.md` | Not mine, Minor-judgment | operations |
| `docs/runbooks/mc-deployment.md` | Not mine, Minor-judgment | operations |
| `docs/runbooks/mh-incident-response.md` | Not mine, Minor-judgment | operations |
| `docs/runbooks/client-dev-local.md` | Not mine, Minor-judgment | operations |
| `docs/runbooks/devloop-validation.md` | Not mine, Minor-judgment | operations |
| `docs/runbooks/gc-incident-response.md` | Not mine, Minor-judgment | operations |
| `docs/specialist-knowledge/security/INDEX.md` (text supplied by security) | Not mine, Minor-judgment | security |
| `docs/specialist-knowledge/operations/INDEX.md` (registry pointer) | Not mine, Mechanical | operations |
| `docs/specialist-knowledge/test/INDEX.md` (registry pointer) | Not mine, Mechanical | test |
| `docs/specialist-knowledge/code-reviewer/INDEX.md` (registry pointer) | Not mine, Mechanical | code-reviewer |
| `docs/specialist-knowledge/dry-reviewer/INDEX.md` (authored by dry-reviewer at Gate 3) | Not mine, Minor-judgment | dry-reviewer |
| `docs/specialist-knowledge/client/INDEX.md` (author paired-client) | Not mine, Minor-judgment | client |
| `packages/sdk-core/src/media/lifecycle/AudioPipeline.ts` (author paired-client) | Not mine, Domain-judgment | client |
| `packages/sdk-core/src/media/lifecycle/transmitKeys.ts` (author paired-client) | Not mine, Domain-judgment | client |
| `packages/sdk-core/src/media/lifecycle/muteState.ts` (author paired-client) | Not mine, Minor-judgment | client |
| `packages/sdk-core/src/media/lifecycle/__tests__/` (author paired-client) | Not mine, Domain-judgment | client |
| `packages/sdk-core/src/media/pipeline/egress.ts` (author paired-client) | Not mine, Domain-judgment | client |
| `packages/sdk-core/src/media/pipeline/ingress.ts` (author paired-client) | Not mine, Domain-judgment | client |
| `packages/sdk-core/src/media/pipeline/__tests__/` (author paired-client) | Not mine, Domain-judgment | client |
| `packages/sdk-core/src/media/setup/__tests__/mediaMetrics.test.ts` (author paired-client) | Not mine, Domain-judgment | client |
| `packages/sdk-core/src/session/__tests__/` (author paired-client) | Not mine, Domain-judgment | client |
| `packages/sdk-core/src/signaling/SignalingClient.ts` (R6; author paired-client) | Not mine, Domain-judgment | client |
| `packages/sdk-core/src/signaling/events.ts` (R6; author paired-client) | Not mine, Domain-judgment | client |
| `packages/sdk-core/src/signaling/__tests__/` (author paired-client) | Not mine, Domain-judgment | client |
| `packages/sdk-svelte/src/stores/MediaStore.svelte.ts` (R6; author paired-client) | Not mine, Domain-judgment | client |
| `packages/sdk-svelte/src/__tests__/MediaStore.test.ts` (author paired-client) | Not mine, Domain-judgment | client |
| `packages/web-app/e2e/media-loopback.spec.ts` (author paired-client) | Not mine, Domain-judgment | client |
| `packages/web-app/e2e/fixtures.ts` (author paired-client) | Not mine, Minor-judgment | client |
| `packages/web-app/src/__tests__/inMeeting.test.ts` (author paired-client) | Not mine, Domain-judgment | client |

---

## Planning

### Problem restated as mechanism

Task 6 bound each participant to ONE handler in the data structure (`slots::Member.handler`, `SlotTable::admit(place: FnOnce(JoinRank) -> HandlerId)`, `MeetingHandlers::place`) and derived both the MH snapshot and every client-facing url from that binding. The mechanism-level error is that **visibility was an input MC configured, not a fact MC observed**. The corrected class: *every routing decision MC makes about a participant's handler must derive from server-observed connectivity, and every client-facing url from the one frozen `MeetingHandlers` value*. Same-owner siblings the task did not name, all in scope here: (1) the connectivity source itself is a second store (`mh_connection_registry.rs`) keyed by token `sub` with no connection identity, so a stale disconnect makes a live handler vanish permanently (MH hazard 1 / security S8); (2) the sender-binding decision reads that registry while routing would read something else (DRY D1); (3) the registry cap is a hardcoded literal whose `false` would now silently delete connectivity (DRY D3 / OPS-1); (4) the one-handler rule is encoded in ~10 prose/dashboard/proto/e2e sites (DRY D4 + addenda). All four are closed in this loop.

### Design

**E1. Connectivity has ONE home: the meeting actor. `MhConnectionRegistry` is deleted.** The actor already holds the roster (sub → participant), the frozen `MeetingHandlers` (S1a) and the slot table, so recording connectivity there makes the sender-binding answer and the routing input the SAME actor turn — they cannot drift (DRY D1's real concern). The registry's only live consumer was the 1000-connection cap gating `RegistryFull`; the actor's bound is structurally tighter (only rostered, unambiguous participants × handlers in the frozen set, which GC caps at two), so the literal cap and `SenderBindingOutcome::RegistryFull` retire with it (catalog/dashboard/runbook rows tombstoned, MH-side doc updated). Per-participant state in `MeetingMedia`:
`Connectivity { live: BTreeMap<HandlerId, BTreeSet<ConnectionKey>>, phase: NotConnected | Establishing{deadline} | Settled }`, where `HandlerId`s are only ever obtained from `MeetingHandlers::resolve(&str) -> Option<&HandlerEndpoint>` (exact byte match, no normalisation) — a `ConnectedHandlers` newtype in `placement.rs` whose only insert takes a `&HandlerEndpoint`, so an unresolved wire string is unconstructible in it (S1a, structural). The per-(participant, handler) connection-key set is bounded by a Rust const (`MAX_LIVE_MEDIA_CONNECTIONS_PER_HANDLER = 4`; a legitimate client holds one, two across a transport reconnect; overflow REFUSES the new key and is counted — fail-closed per S13(b)) — a memory-safety bound with no operator story, same register as `SLOT_VIEW_FLUSH_BATCH`.

**E2. Connection identity (MH hazard 1 / security S8) — proto + MH; APPROVED by the Lead as R1.** Add `string connection_id` to `NotifyParticipantConnectedRequest` and `NotifyParticipantDisconnectedRequest` (opaque, MH-generated; MH already mints a per-session uuid). MC keys `live` on it: duplicate connects are idempotent, a disconnect removes only its own key, a disconnect for an unknown key is a counted no-op (this is also every declined connection's disconnect). A handler is in connected(P) iff its key set is non-empty. Disconnect attribution resolves the participant from `sub` FIRST and then removes only a `(handler_id, connection_id)` key THAT participant holds (S12) — so no cross-participant probe exists, and a reconnect on the same handler cannot erase the other connection's key. **Rollout compatibility**: an empty `connection_id` (old MH) maps to one legacy key per (participant, handler) — exactly today's set semantics, documented as degraded; an old MC ignores the new field. Refcounting was considered and rejected (security S8: a retried Connected inflates it → phantom connectivity, fail-open). The **key of connected(P) is the connection, never (participant, handler)** — stated once, at the type.

**E3. Notification path.** `NotifyParticipantConnected` → validate (unchanged) → `controller.get_meeting_handle` (miss → `unknown_meeting`, sender 0) → ONE actor message `MediaConnected { token_sub, handler_id, connection_id } -> SenderLookup` that resolves `sub` exactly as today (NotFound → `participant_unknown`; two roster entries → `user_ambiguous`, connectivity applied to NO entry — S2), resolves `handler_id` against the frozen set (miss → `handler_not_in_set`, sender binding still answered as today so MH's binding contract is unchanged, but connectivity NOT recorded — S1a), records the key, and runs the structural-change path. `GetSenderIdForUser` is replaced by this message (not left dead). `NotifyParticipantDisconnected` → `MediaDisconnected { token_sub, handler_id, connection_id }` → resolve `sub` to the participant, then remove that key from THAT participant only; a key it does not hold is `unknown_connection`, counted, no-op (S12; there is no `sub_mismatch` reason and no cross-participant lookup). A connectivity change is a structural change: `reconcile(Affected::All)` → render → per-handler `next_generation` (advance-only-on-change) → publish to the existing push workers → bounded flush. MH-side: nothing awaited on I/O inside the actor (semantic-guard lens 2). `ParticipantActor::mh_statuses` is NOT read (security S3); one sentence at its definition names the actor's connectivity as the authority and says why the two are not one map (DRY D2).

**E4. Settle rule — I recommend ONE, pessimistic, and here is the concrete failure without it.** Edges are stable and never re-consolidate (task rule). Take an all-connected meeting with A and C settled, all edges co-located on h0. B joins; its h1 connect happens to land first (MH reports ~100 ms apart in Kind). Without a settle rule B becomes routable on {h1} alone, so A→B, C→B, B→A and B→C are all placed on h1. B's h0 connect then arrives, but every edge is still valid on h1, so none moves. **Steady state: every sender sends to both handlers in a meeting where everyone is connected everywhere.** That breaks the task's required property "all-connected ⇒ one handler per sender", doubles uplink (§9 congestion finding) and splits egress for no reason. The same race also permanently reorders R-2 join-order fill (a later joiner takes the slot during an earlier joiner's gap). This is not UX flicker. The loss is permanent, and it is the headline case. The rule, meeting every constraint @security and @test set:
- **Pessimistic only.** A participant in `Establishing` has NO edges and is NOT unreachable. No connectivity is ever assumed; the window only WITHHOLDS routing, and routing always uses the observed set.
- **Server-anchored, single-shot.** The deadline = the first MH connect notification of the episode (`NotConnected → Establishing`) + `MC_MEDIA_CONNECT_SETTLE_MS`. Later connects in the episode never extend it. Nothing client-controlled can touch it (`MediaConnectionUpdate` is not read). An integration test asserts repeated connects do not re-arm it.
- **Ends early** the moment the connected set equals the full frozen set, so an all-connected participant settles at its last connect with no added latency.
- **Fail to a defined state.** On expiry the participant settles with whatever it has observed. If every connection drops before or after settling, it returns to `NotConnected` (no edges, not unreachable), and the next connect starts a new episode. Such a cycle only delays the participant itself.
- **Config.** `MC_MEDIA_CONNECT_SETTLE_MS` is required, bounded `100..=10_000` both sides at load, `1500` in the ConfigMap (inside the env-test budget), and injectable in tests via tokio paused time (the actor gets a `sleep_until(earliest deadline)` select arm).
- **Counted.** `mc_media_connect_settles_total{outcome=complete|window_elapsed}` (O-8). `window_elapsed` is the partial-connectivity signal.
- **OPS-8 arithmetic.** With the rule, a two-handler participant usually makes ONE routing change at settle instead of two, which halves formation churn. Burst bound per meeting is ≤2N notifications, each O(N) render, and the push workers coalesce latest-wins. Mailbox depth is bounded by the notification rate, which MH's accept path bounds. No new alert noise is expected; I will confirm with a formation-burst integration test.
If the Lead rules against the settle rule, the fallback is the pure observed model plus a documented known defect (non-consolidation). I am stating that trade explicitly rather than choosing it quietly.

**E5. Edge assignment — `slots.rs` + `edges.rs`.** `Member { sender, connected: Option<ConnectedHandlers> /* None = not routable */, slot_ids, slots: Vec<Option<Edge>> }`, `Edge { source: SenderId, handler: HandlerId }`.
- `shares(s, p)` = `s != p` && both routable && connected(s) ∩ connected(p) ≠ ∅. This is the ONE eligibility rule, and it stays non-reflexive (R-3).
- **admit(sender)**: gets a rank and is not routable. Nothing moves. The placement closure is gone, so no one-handler binding survives in the type (DRY D4.3).
- **set_connectivity(sender, Option<ConnectedHandlers>)** is the only new mutation. For every edge touching `sender` whose handler has left the shared set: if the shared set is still non-empty, re-choose within it (the slot keeps its sender and `egress_stream_id`; only the handler moves). Otherwise free the slot (unreachable) and refill it earliest-rank-first. Then refill every subscriber's empty slots in the usual way. Edges not touching `sender` are untouched (the stability invariant).
- **remove / set_demand**: unchanged rules. A fill picks the edge handler through the chooser.
- **Chooser (`edges.rs`)**: a pure fn `choose<'a>(candidates: &'a [HandlerId], ctx: &EdgeContext) -> &'a HandlerId`. The lifetime means it can only return a candidate. The production policy `colocate`: prefer the handler carrying most of the sender's existing out-edges (fewest targets), then the handler carrying most of the meeting's edges (all-connected ⇒ one handler), then a deterministic tiebreak. **Tests never depend on the tiebreak.** The model check runs under `colocate` AND an adversarial chooser (reverse/rotating) to show correctness for ANY choice.
- `unreachable_for(s)`: empty if `s` is not routable. Otherwise the routable members `p != s` with connected(p) ∩ connected(s) = ∅, sorted. Not-routable members never appear. The "no slot" case (N+2th sharer) still does not appear.
- `render()`: per-slot `edge.handler`, not the subscriber's handler. Every handler still gets an entry, possibly empty. **Registration of every handler at first join is kept**, because MH only reports connectivity for a registered meeting (paired-MH), so an empty snapshot to a zero-edge handler is load-bearing.
- `send targets(s)` = handlers owning ≥1 of s's out-edges. That is exactly what `build_send_directive` already derives from the render. No directive change is needed beyond docs.
- **Invariants** (model check): 1 no self; 2 no duplicate; 3 every edge's handler ∈ connected(sub) ∩ connected(src); 4 no empty slot while an eligible unheld sharer exists; 5 slot vector length = declared; 6 unreachable ⇔ both routable and disjoint; 7 targets = owners of out-edges; 8 an edge's handler changes only if that handler left one party's set (transition rule); 9 a filled slot's sender changes only on leave, redeclare, or the pair becoming unreachable (R-4).

**E6. Client-facing addresses.** `JoinResponse.media_servers` = every handler of the frozen set (via `MeetingHandlers::iter()`). `StreamAssignment.media_handler_url` = `url_of(edge.handler)`. `SendTarget` = `url_of` of each owner. All three come from one value, byte-identical (S4). `JoinResult.media_handler` is removed. The `ReconnectResult` doc now says "the full frozen set". The placement INFO log is replaced (O-5, OPS-4b) by an INFO line at each recompute that changed a participant's routing, carrying `meeting_id`, `participant_id`, sorted connected `mh_id`s, the phase, and the `mh_id` owning each of its in-edges. No urls, no sender ids. The latest line per participant is its current state, which gives Scenario 18 its Step 0.

**E7. Seams.** `MeetingSeams::placement_pins` is deleted (the in-process tests drive real notifications through `McMediaCoordinationService`). `sender_id_cursor` and `slot_view_flush_batch` stay. The `lib.rs` `compile_error!` loses only the placement-pin clause, and the needle line is not reflowed (OPS-5, S9). No connectivity-injection seam is added (S5).

**E8. Telemetry.** All metrics carry `key_custody="operator"` and no handler, sender, meeting or url identity (O-3, S7). Each has 4 artifacts per ADR-0032.
- `mc_mh_notifications_unapplied_total{reason}`, where reason ∈ `unknown_meeting | participant_unknown | user_ambiguous | handler_not_in_set | unknown_connection | connection_bound_refused` (O-1, O-12). Per-reason expectation is documented: `unknown_connection` is routine because every declined connection sends one.
- `mc_media_not_yet_connected_senders_total`, incremented by the count of non-routable roster members INCLUDING the subscriber itself on each SENT `StreamAssignments` (O-2). Emission-weighted, not expected-empty, with a persistence-is-the-signal note.
- `mc_media_send_targets_total` (O-6).
- `mc_media_edge_moves_total{reason=connectivity_change|unexpected}`, where `unexpected` is expected-empty. It is detected by diffing the previous and new render in `reconcile`, independently of the mutation code (O-7).
- `mc_media_connect_settles_total{outcome}` (E4).
- Semantics rewrites (O-4): the unreachable entry and panel, the slot-states `source_unreachable` reasoning (it stays unemitted), `send_directives` "co-handler" wording plus the frequency note, handler-set divergence wording, and `mc_participant_mh_status_total` ("NOT the connectivity source").
- "Permanently zero on a single-handler deployment" keeps its claim with the new justification: with one handler, every routable participant shares it, and not-yet-connected ≠ unreachable.
- **O-11 (client-vs-MH divergence counter) — proposed deferral.** It needs client status forwarded into the actor plus a noise design: at every join the client reports connected before MH's notification lands, so a naive counter fires on every join. The real detector for its target case (a crashed MH never sends disconnects) is story 4's §8 restart detection. I will record it in that obligation's TODO entry with the trigger. Fix-now cost is a new actor message plus a debounce design; fix-later cost is the same work with the story-4 epoch available to anchor it.

**E9. Out of scope, stated (OPS-2, MH hazard 2).** (i) A crashed MH sends no disconnects, and its restarted process registers a NEW `handler_id` that is not in the frozen set, so stale connectivity to the dead id persists until story 4's handler-restart detection and re-registration. This is pre-existing: task 6 also routed to a dead handler indefinitely. Notifications naming the new id are counted `handler_not_in_set`, which is the Scenario 18 discriminator. It is recorded against the existing story-4 obligation, not as a new section. (ii) An MC restart loses connectivity with the actor. Clients rejoin a new actor with new sender ids and must re-dial MH (question to @paired-client to confirm). (iii) Handler identity is MH-asserted within one shared service identity (S1b). The blast radius is confined to the meeting's frozen set. One sentence goes in the connectivity module doc, and the TODO entry sits beside task 6's.

**E10. Client (author @paired-client, per their audit).** **SEAL ONCE, UPSTREAM OF THE FANOUT (security S14 — the one item in this loop whose failure mode is a key compromise, not a routing error).** Each frame is sealed, signed and built exactly ONCE: one `streamSequence` taken from the ONE shared `TransmitKeyManager`, one signature, one payload. The identical immutable bytes are then handed to each target lane, whose ONLY per-lane mutation is `writeHopSequence` at its own dequeue. Per-lane hop sequences are correct (each counts what actually left on that transport). What must never happen is a per-target pipeline that takes its own `streamSequence`: two lanes holding separate managers would both start at 0 under the same transmit key and seal different plaintexts under one (key, nonce) — AES-GCM nonce reuse, leaking the plaintext XOR and the GHASH key. Lanes sharing a buffer must copy per lane or write-then-send synchronously, because the single-flight `#drain` guarantee is per pipeline and does not cover cross-lane interleaving. Required tests (paired-client): output byte-identical across targets except the hop-sequence field; the shared manager advances exactly once per frame regardless of target count; two lanes never emit the same `(keyId, streamSequence)` for different payloads. `signaling.proto`'s `SendStream.targets` comment states this as a SAFETY rule, not an efficiency preference. Connect-to-all already holds. Send-to-every-target is broken (`const [target] = directive.targets`), and the fix is per-target lanes with independent queues and hop sequences. Receive-on-owning-transport is broken (two task-6 FAULTs), and the fix is one read loop per distinct url with a per-transport downlink hop monitor. The e2e `mediaServers.length === 1` assertions and banner are also fixed. Rollout: a new SDK works against the old MC. An old SDK against the new MC works all-connected (co-location ⇒ one url per subscriber, one target per sender) but FAULTs on multi-url assignments, (superseded by E15: this is a version-skew window, not a deploy ordering). The rollback coupling note goes in `mc-deployment.md` (OPS-6). Question to @paired-client: the client must not latch the unreachable state (OPS-8b).

**E11. Tests** (properties only, never which handler).
- **`slots.rs` unit tests.** The canonical A{0,1}/B{0}/C{1} case, all-connected, a disconnect moving only that participant's edges, edge stability across an unrelated recompute (prior state lives in the table), slot stability across a connectivity change, the not-yet-connected participant (no edges, not unreachable), and a leave that frees only the leaver's slot.
- **Exhaustive model check** with `SetConnectivity` ops, run under two choosers, asserting invariants 1–9 plus the transition rules.
- **Property test** over fixed-seed generated connectivity (`rand` is already a workspace dep, seeded, no clock): the reachable ⇔ intersect rule, exactly one edge per reachable admitted pair on a shared handler, the unreachable set exact, targets = owners, determinism.
- **Integration (`slot_placement_integration.rs` rewritten, notifications through the real `McMediaCoordinationService`):**
  - re-push on connect or disconnect with generation advance-only-on-change, both arms (a duplicate connect or a no-op change → no RPC, generation unchanged);
  - the stale-disconnect regression (C1, C2, C1-disconnect keeps the handler);
  - an ambiguous `sub` applies nothing;
  - an out-of-set `handler_id` → no url anywhere, counted (S4 pin b), plus a client url never reaching a per-edge `media_handler_url` (S4 pin a);
  - settle: early-complete, window-elapsed under paused time, no re-arm on repeated connects;
  - `media_servers` = the full set;
  - formation-burst convergence.
- **`media_coordination_integration.rs`** is updated for the actor-held path.
- **Env-test 27 rework** (with @test). The PRECONDITION is 2 distinct handlers in `media_servers` (`TRIAGE_HANDLER_SET` kept). Then:
  - each mock opens exactly its assigned sessions, and a failed open on an assigned handler hard-fails with an ENVIRONMENT token;
  - CANONICAL phase: A→both, B→one, C→the other, picked arbitrarily from the observed set. A's positive frame controls from B and C both arrive on the transport its assignment names, and that url ∈ connected(A) ∩ connected(sender). Only then assert that B and C are mutually unreachable, hear only A, and receive A's frame, and that A's targets = both urls;
  - ALL-CONNECTED phase in the same run: every pair's frame arrives, every sender has exactly one target, and no one is unreachable. Over-subscription and relay-region binding checks carry forward.


**E12. Runbook truth (OPS-9).** Today sdk-core does not read `unreachable_sender_ids`: `SignalingClient` maps four fields per assignment, and no UI renders it. Browser rendering is the story's task 15 scope, not this task's. Scenario 18 and every other "rendered distinctly (never a spinner)" claim in runbooks and the catalog are rewritten to say unreachability is **server-side evidence only today**: the wire field plus the recompute INFO line. The gap gets a named pointer to task 15. Once paired-client's unconnected-target transport fault lands, Scenario 18 also carries it as the discriminator for "some peers hear me, some don't".

**E13. MH-side follow-through (paired-media-handler).**
- **Label vocabulary.** The new unapplied counter REUSES the existing sender-binding tokens: `unknown_meeting` and `participant_unknown` keep their current names, and the ambiguous case uses `user_ambiguous`. The only sender-binding label change is that `registry_full` is retired.
- **MH text rewritten for the registry retirement.** Six sites: `metrics.rs`, the `mc_may_hold_a_registration` rationale (notify-on-uncertainty now prevents phantom connectivity), the `mh-service.md` row, the `mh-overview.json` description, the `mh-incident-response.md` row, and a sweep for anything else.
- **Settle window must cover RegisterMeeting fan-out.** MH sends Connected only after the meeting is registered on that handler. The ConfigMap comment will say the window must exceed first-join RegisterMeeting fan-out latency.
- **Documented residuals** (in the connectivity module doc and the const doc):
  - A late Connected applied after MH has already given up on the RPC. This needs more than 10 s of actor backlog, and the result is fail-closed silence.
  - Evict-oldest can cause at worst one false removal, which the client's next connect repairs.
- **Test coverage kept.** First-join empty-snapshot registration to a handler that owns no edges stays pinned by an MC test.

**E14. Observability Gate-1 amendments.**
- **O-5 INFO line.** The recompute INFO line carries `meeting_id`, `participant_id`, the sorted connected `mh_id`s, the phase, and a multiset of owning handler ids for the participant's in-edges (e.g. `in_edges="mh-0:2,mh-1:1"`). It never carries per-edge sender identity (§11 R2).
- **O-8 gauge.** A new gauge `mc_media_connect_settle_window_seconds` echoes the enforced `MC_MEDIA_CONNECT_SETTLE_MS`. It is set in `WebTransportServer::new` from the same field, following the `mc_media_receive_slot_cap` pattern.
- **O-8 SLO note.** `docs/observability/slos.md` §Join-to-first-media gains a sentence: the window is an additive floor only for participants that connect to a subset of handlers. It ends early at full connectivity, so the all-connected case adds nothing.
- **Why "compose now, recompose on settle" was not chosen.** It would move edges that are still valid, which the task's edge-stability rule forbids.
- **O-11 deferral recorded as its own `docs/TODO.md` obligation, not under story-4 restart detection.**
  - Causes restart detection does not cover: `MeetingReleased` sends no disconnect by design, and a disconnect whose retries are exhausted leaves MH up.
  - Planned shape: a three-way comparison (absent / reported-connected / reported-not). Only the `mh_connected_client_not` arm is required.
  - Restart detection is listed as a partial mitigation only.
- **O-1 dashboard thresholds.** Per-reason override pattern: routine reasons are neutral, and MC-defect reasons are red.
- **O-2 catalog note.** The catalog entry states that the subscriber's own contribution and its peers' contribution are mixed in one series.

**E15. Client Gate-1 amendments (paired-client).**
- **Unreachable rendering (SUPERSEDED by Lead ruling R6).** Client CONSUMPTION is IN this task (mapper → `StreamAssignmentsEvent` → `MediaStore`, with tests, paired-client); DOM rendering stays task 15's. The original text follows for the record: `unreachable_sender_ids` stays unconsumed in this task. Rendering it is story task 15's explicit scope ("render a rostered-but-unreachable participant distinctly"), so it is recorded as owned by task 15, not as a TODO. paired-client adds an sdk-svelte test that a slot flickering to `source_unreachable` and back renders normally (live no-latch assertion).
- **Rollout: a version-skew window, not a deploy-ordering rule** (rewrites E10's rollout note). The exposure is the new MC plus any cached old SDK.
  - Old-SDK send drops every target after the first with no fault and no metric. That is SILENT, and it leads the note.
  - Old-SDK receive FAULTs on multi-url assignments. That is loud.
  - All-connected meetings are unaffected either way.
- **MC failover.** The client builds a fresh MeetingSession/MediaTransport, which re-dials every MH, so connectivity rebuilds from new notifications.
- **Known pre-existing limit** (goes in `mc-deployment.md` and Scenario 18). A single MH transport that drops mid-session is not re-dialled. MC moves the affected edges correctly, but the client stays narrowed to the surviving handlers until it rejoins.

**E16. Security Gate-1 amendments (S10-S13).**
- **E4's rationale goes in the code.** The non-consolidation argument is written as the code comment at the settle site, so the window does not read as a debounce someone can delete.
- **S10: episode floor.** A new connectivity episode (`NotConnected → Establishing`) cannot start sooner than `MC_MEDIA_CONNECT_SETTLE_MS` after the participant's previous episode started. It reuses the same duration with no new key.
  - A connect that arrives early is still recorded as a key. The phase transition waits until the floor.
  - Effect: SETTLE-driven recomputes (new episodes) are capped at one per window per participant, and only a churning participant is delayed. *(Corrected at Gate 3, security S-1: a single-handler flap by a participant that keeps another live key is not an episode and is not floored — see the residual named at the floor in `connectivity.rs`.)*
  - A test covers it: a rapid drop-all/reconnect cycle yields at most one episode per window.
- **S11: legacy path counted.** An empty `connection_id` (legacy path) is counted on `mc_mh_notifications_without_connection_id_total` (cardinality 1).
  - The retirement condition is written at the branch: delete it once every MH in the fleet sends the field.
- **S12: participant-scoped disconnect lookup.** Disconnect resolves `sub` → participant first, then removes the key from that participant's map only. A miss there is `unknown_connection`.
  - `sub_mismatch` is removed from the vocabulary. An ambiguous `sub` also lands on `unknown_connection`.
- **S13: bound chain and overflow policy.**
  - (a) The module doc states the bound chain: |roster| ≤ `SenderId` space (65535) × |handlers| ≤ 2 by GC assignment × `MAX_LIVE_MEDIA_CONNECTIONS_PER_HANDLER` (4). The MC-wide participant bound holds.
  - (b) Overflow policy is **refuse-new** (fail-closed: MC never forgets a tracked live connection). The refusal is counted as `connection_bound_refused`, and the sender binding is still answered.
  - Residual, named at the const: a 5th concurrent session from one participant to one handler is untracked. Its disconnect is a counted no-op. This is self-scoped.
- **S9: security INDEX line 52.** Replaced verbatim with @security's text, adjusted only to the symbol names as they land.

**E17. DRY and test Gate-1 amendments.**
- **D1a: `registry_full` REMOVED outright, not renamed.** The actor never declines a binding for capacity. All 12 encoding sites move in this loop: MC code, MC+MH catalogs, both dashboards, both runbooks, and MH `metrics.rs` (paired-media-handler).
  - The MC catalog keeps a tombstone line for the retired value.
  - MH's union definition drops to three members.
  - The "capacity / raise the cap" triage rows are replaced by a pointer. The actor's per-(participant, handler) refusal is `mc_mh_notifications_unapplied_total{reason="connection_bound_refused"}`. It is self-scoped, does not decline the binding, and has no cap to raise.
- **D1b: exact-match `handler_id` check.** This is the S1a check (`MeetingHandlers::resolve`, `ConnectedHandlers`). It is tested by the S4 pin (b) integration test and a unit test.
- **D1c: `MAX_ID_LENGTH` moves** to `grpc/media_coordination.rs` beside `validate_id_field`, its only consumer.
- **D1d: teardown prose rewritten.** The two-teardown-path contract prose in `controller.rs` (×2) and `generation.rs` is rewritten on the surviving call (policy-generation eviction). The reason is kept: both teardown paths must release it.
- **D1e: both directions of the binding/connectivity invariant are unit-tested at the actor.**
  - Forward: no ordinal is handed out for untracked state.
  - Inverse: no connectivity is recorded for a participant answered `0`.
  - The docs where `participant_unknown` is described as an "expected low-rate race" state that a connect arriving before the join is now forgotten for connectivity too. It self-clears via MH's decline and the client's reconnect.
- **Test (a): unconditional characterization test.** A participant whose set drops to empty stays not-connected: absent from peers' slots and unreachable sets, with no re-gain and no generation re-advance, even after paused time is advanced past the window. The (A)-conditional stale-disconnect regression stacks on top.
- **Test (b): settle-aware env-test waits.** Env-test 27 derives its settle wait from the published gauge `mc_media_connect_settle_window_seconds` (read via the metrics fixture) plus a margin, not from a hardcoded QUIET.
- **Test (c): superseded task-6 tests.** All three `placement.rs` round-robin tests, the round-robin, pin and `media_servers.len()==1` assertions in `slot_placement_integration.rs`, `media_client_signaling_integration.rs` and `directive.rs`, and the model-check `round_robin` helper are removed or rewritten.

**E18. Lead rulings (Gate 1) and operations conditions.**
- **R1 — `connection_id`: included.**
  - Semantics, stated in the proto comments and at MC ingestion:
    - A handler leaves connected(P) only when no live id remains for that (meeting, participant, handler).
    - An unknown-id disconnect is a counted no-op.
    - A retried Connected is idempotent.
    - An empty id degrades to per-handler set semantics, and that degradation is counted (S11).
  - Authors: paired-protocol writes the proto, paired-media-handler writes the MH half and its tests.
  - Both properties the MH text relies on hold on the MC side: MC records connectivity before the RPC returns, and an unheld-entry disconnect is a no-op, never an error.
- **R2 — settle rule: yes, as in E4, with S10's episode floor.**
- **R3 — repair after an MH or MC restart: out of scope.**
  - The sticky-silence hazard is stated at the ingestion site in `media_coordination.rs`.
  - It goes in ONE `docs/TODO.md` §Media Path Obligations entry, extending task 6's entry, together with the S1b handler-identity residual.
  - Out-of-set handler ids are ignored and counted.
  - Connectivity is purged with the actor on EndMeeting or meeting release.
- **R4 — O-11: its own TODO entry** (per E14).
- **Operations conditions:**
  1. The runbook states that the window delays time-to-first-audio for partial-connectivity participants by up to `MC_MEDIA_CONNECT_SETTLE_MS`, and that it never appears in `mc_session_join_duration_seconds`, which stops at JoinResponse.
  2. The ConfigMap comment gives the derivation of 1500: about 15× the observed Kind inter-connect gap, and it must cover the RegisterMeeting fan-out. It also explains the 100..=10000 bounds.
  3. Scenario 18 Step 0 uses `mc_media_connect_settles_total{outcome="window_elapsed"}` as its population signal, with the PromQL. The existing unreachable-counter caveat points to it.
  4. Scenario 18 carries `handler_not_in_set`, its PromQL, and the reading "MH restarted, story-4 re-registration pending, stale connectivity will not self-clear".
  5. Per-reason expectations are written per value. `connection_bound_refused` (the renamed evict reason, per S13) is expected-empty and alertable at > 0; `unknown_connection` is routine.
  6. `mc-deployment.md` gets a numbered deploy step: roll the SDK first. It names the stale-bundle window, which is safe for all-connected meetings, and the old-MH degraded legacy-key window with its signal counter.
- **MC catalog and dashboard:** `registry_full` is tombstoned in the same "version skew" terms as MH's runbook.

**E19. Consolidated test requirements (test, after the rulings). All unconditional.**
- **Connection identity (R1):**
  - A stale Disconnected naming a superseded `connection_id` keeps the live handler (C1, C2, C1-disconnect).
  - A retried or duplicate Connected with the same key is idempotent: the connected set is unchanged and the generation does not re-advance.
  - An empty `connection_id` maps to one legacy key per (participant, handler), and a legacy disconnect clears it. The event is counted on `mc_mh_notifications_without_connection_id_total`.
- **R3 in-scope behaviours:**
  - No connectivity survives EndMeeting or meeting teardown.
  - An out-of-set `handler_id` is ignored and counted, while the sender binding is still answered.
  - The hazard comment sits at the ingestion site in `media_coordination.rs`, and there is a single TODO entry.
- **Characterization (always present):**
  - A connected set that drops to EMPTY stays not-connected under paused time advanced past the window.
  - Such a participant is absent from peers' slots and unreachable sets, with no re-gain and no generation re-advance.
  - This is distinct from the R1 regression.
- **Env-test settle wait:** derived from the published `mc_media_connect_settle_window_seconds` plus a margin, not a hardcoded QUIET.
- **S10 episode-floor test:** repeated disconnect/reconnect cycles yield at most one recompute per window per participant, on the same paused-time harness as the no-re-arm test.
- **Settle-site tripwire:** the code comment at the settle site carries the non-consolidation argument AND names the all-connected exactly-one-target paused-time test as the deletion tripwire, so the comment and the test point at each other.
- **Task-6 tests superseded tree-wide:**
  - the three `placement.rs` `place()` tests;
  - the round-robin, pin, and `media_servers.len()==1` assertions in `slot_placement_integration.rs`, `media_client_signaling_integration.rs` and `directive.rs`;
  - the model-check `round_robin` and `one_handler` helpers.

**E20. Protocol Gate-1 amendments (paired-protocol P1–P5).**
- **P1 — field shape.**
  - `string connection_id = 4` on `NotifyParticipantConnectedRequest` and `= 5` on `NotifyParticipantDisconnectedRequest`. Bare string: empty means legacy.
  - The contract comment lives on the Connected field; the Disconnected field points to it. Clauses (a)–(g) as proposed. (b) states that a Disconnected is terminal for its id.
  - (d) is phrased to match S12: a disconnect is attributed within the participant resolved from `participant_id`. A key that participant does not hold is an unknown-id no-op, counted `unknown_connection`. There is no cross-participant lookup.
- **P2 — validation home.** `MAX_ID_LENGTH` moves to `grpc/media_coordination.rs` as the one bound (with D1c). `connection_id` gets its own rule: empty is allowed; non-empty values are capped at the same bound. Unit tests cover empty accepted, 256 accepted and 257 rejected.
- **P3 — tombstone the reorder hazard instead of carrying it as a residual.**
  - Mechanism: per participant, a bounded FIFO of recently retired (handler, connection_id) pairs, capped at a Rust const `RETIRED_CONNECTION_TOMBSTONES = 16`. A Connected naming a tombstoned id is ignored and counted `retired_connection`. The FIFO is dropped with the participant.
  - This closes the fail-open phantom that a delayed Connected applied after its Disconnected would otherwise leave. The residual is only an id that has already been evicted from the FIFO; that is named at the const.
  - Needs security sign-off.
- **P4 — table row.** A `crates/proto-gen/tests/internal_roundtrip.rs` row is added.
- **P5 — signaling.proto comment sweep, comment text only.**
  - Sites: `SLOT_STATE_SOURCE_UNREACHABLE`, the `StreamAssignments` header (re-emit on connectivity change), the membership bullets, `media_servers`, `StreamAssignment.media_handler_url`, and `SendStream.targets` ("send to EVERY target; encrypt once, transmit N").
  - New membership bullet: participants not yet connected to any handler are EXCLUDED.
  - The "client marks distinctly" MUST stays.
  - A grep sweep of `signaling.proto` and `internal.proto` (around `MediaCoordinationService`) for remaining one-handler-per-participant prose.
- **Three added doc-comment clauses.**
  - (h) `connection_id` is NOT an identity or an authorization input.
  - (i) the same value rides every retry and the Disconnected.
  - (j) the empty-id legacy branch has a named RETIREMENT CONDITION: it is retired once every MH in the fleet sends the field, and the degraded mode is counted so retirement is observable rather than assumed. Empty-means-legacy is a rollout affordance with an end date, not an optional field.
- **Terminal rule stated as SEND order, not delivery order.** MH never SENDS a Connected after SENDING its Disconnected. Retiring an abandoned Connected that arrives late is MC's job (the P3 tombstone). The MC-side comment AT THE TOMBSTONE restates this, so nobody reads the proto's terminal rule as a delivery guarantee and deletes the tombstone as redundant.
- **Authorship (pending Lead ruling).** paired-protocol authors both proto edits and the roundtrip test if the Lead approves; otherwise the implementer authors them and paired-protocol reviews.

**E21. P3 sign-off refinements and the R1/R2 couplings (security, dry-reviewer).**
- **P3 approved as option (a) — implement the tombstone.** Refinements:
  - It is keyed on the PAIR `(handler_id, connection_id)`, never the id alone. Stated at the type, because id-only keying would let one handler's retirement suppress a live connection on another.
  - The residual is named with its trigger: more than `RETIRED_CONNECTION_TOMBSTONES` retirements for that participant between an abandoned Connected and its delayed arrival. The consequence is bounded to the S1b class (misrouting inside the meeting's own frozen set, never cross-meeting).
  - The FIFO joins S13's bound chain in the same sentence: `|roster| × 16 × ≤256 bytes`.
  - Test: a Connected naming a tombstoned id is ignored and counted `retired_connection`.
- **`validate_id_field` is NOT parameterised.** `connection_id` gets a SEPARATE `validate_optional_id_field` (empty allowed, same 256-byte cap). An explicit test asserts empty `meeting_id`, `participant_id` and `handler_id` are each still rejected, and it exists to fail if the two helpers are ever unified.
- **`connection_id` is a log field only, never a metric label** (unbounded cardinality, §11). `retired_connection` is a bounded reason value on the existing counter.
- **`user_ambiguous` is NOT resolved by `connection_id`. Two code sites are POINTED at the SSoT, not reworded.** R1 landing beside a note awaiting it would leave a documented purpose falsely fulfilled.
  - The accurate statement, narrower than my first framing: `connection_id` ALONE cannot disambiguate two joins behind one `sub`, because MC cannot map an MH connection to one of its own joins. `docs/TODO.md`'s candidate (a) (a per-join identity in the meeting token) remains the remedy; candidate (b)'s wire half now exists, so (b) is cheaper than the entry currently implies. That nuance goes in the TODO entry, so nobody re-costs (b) as if the proto work were still ahead.
  - **Both wrong-remedy code copies point at `docs/TODO.md`'s entry** rather than each carrying a corrected sentence — a remedy restated inline in two files is how this drifted in the first place. The sites: `media_admission/binding_response.rs` and **`actors/messages.rs`** (`SenderLookup::Ambiguous`, "needs a per-connection identifier on the wire"). Both runbook rows already point at the TODO and need nothing; they are the model.
  - `tests/media_coordination_integration.rs`'s "a contract change rather than a retry" stays true as written; its surrounding comment is checked but not edited.
  - The pointer names what the real remedy would fix — joins, not connections — so a future reader can tell whether their change is it. Without that, the next person to see `connection_id` land would read the arm as dead code and delete it, reopening S2 (connectivity attributed to the wrong roster entry) with the old doc as justification.
  - `user_ambiguous` therefore STAYS in the vocabulary — not a second retirement candidate, so the 12-site sweep is `registry_full` only. Its rows keep their meaning.
- **`SenderBindingOutcome::ALL` length literal** drops from 5 to 4 with `registry_full`, along with the cardinality assertion its doc names.
- **`MC_MEDIA_CONNECT_SETTLE_MS` follows MH's `_MS` precedent**: required, bounded, with the key literal inline in `ConfigError::MissingEnvVar` so `dt-guard env-config` discovers it. The struct field carries the unit (`..._ms`). This is MC's first `_MS` key and first required time window.
- **Two encodings of the ConfigMap value** in `mc-deployment.md`: the config-reference table row AND the annotated ConfigMap listing. Both move. The INDEX required-key family line gains the §9 key.
- **The coordination checklist names the crash-loop shape**: a new REQUIRED key means the new MC image against a stale in-cluster ConfigMap crash-loops on `MissingEnvVar`, and that has previously presented as a port-allocation error (`docs/TODO.md` records it). MC has no deployment-config env-test, so nothing catches it automatically.
- **`docs/specialist-knowledge/dry-reviewer/INDEX.md` is dropped from my Mechanical row** — dry-reviewer authors it themselves at Gate 3.

**E22. The unreachable-rendering gap is recorded by its real cause, not as a rationale (dry-reviewer).**
- Client CONSUMPTION of `unreachable_sender_ids` is paired-client's lane and is before the Lead as a scope question. DOM rendering stays task 15's either way.
- Whatever the ruling, the record names the actual situation: `signaling.proto` states client marking as a MUST, and this task RETIRES the premise that made ignoring it costless. The catalog's "permanently zero on a single-handler deployment" now holds only because not-yet-connected is not unreachable, and a non-empty set is the rare, wholly silent case.
- **The catalog and dashboard rewording must not leave any sentence readable as a justification for a client ignoring the field.** Mine and observability's to get right.
- Frequency, stated precisely: a non-empty unreachable set requires ASYMMETRIC per-client connectivity. It is rare in production and wholly silent when it happens — and the canonical three-participant test manufactures it deliberately.

**E23. Protocol text requirements from security's sign-off (paired-protocol).**
- **Site 2 is a contract change.** Widening the `StreamAssignments` MUST's trigger set (re-emit on connectivity change) means an implementation that satisfied the old text is now non-conformant. The table row says so.
  - Security explicitly does NOT raise a classification upgrade, because protocol authorship or review plus security co-review is already in place.
  - Their conditional: if the Lead rules that I author with no protocol involvement beyond a glance, the upgrade question goes live. That is a second reason the authorship ruling matters.
- **Site 5 (`media_servers`) names `SendTarget` as the SOLE source of a transmit address.** It does not merely stop pointing at the subscriber's own slot url. Deriving a SEND address from a RECEIVE-side field is one step from deriving it from client input, against the url-provenance discipline this task rests on.
- **Site 7 (`SendStream.targets`) CITES `signaling.proto:779-785` as the normative home of the encrypt-once rule** rather than restating it (a second wording of a nonce-reuse rule is a second thing that can drift). What that home already says, and why it is safety rather than efficiency:
  - The stream sequence is allocated ONCE PER FRAME per (sender, stream), BEFORE fanout.
  - The IDENTICAL sealed bytes go to every target.
  - A per-target sequence counter is a nonce repeat under one key. ADR-0036 §2's consequence is named: the authentication subkey leaks and forgery becomes possible.
  - Protocol verified the mechanism independently: `nextStreamSequence` is per-manager, defaults to 0, and is taken once per frame then fed to `sframeNonce`.
  - This is the normative home of E10's S14 invariant: the lanes MUST share one `TransmitKeyManager`.
- **Terminal rule: the MC tombstone is named as the receive-order defence AND as REQUIRED.** The dependency is written in both directions, contract to implementation and back, so nobody deletes the tombstone as redundant on reading "terminal".
- **Site 3's new bullet is the normative home of "not-yet-connected is not unreachable"** — it lands in the contract, not only in MC's implementation.

---

## Gate 1 — Plan Confirmations

Lead rulings: R1 connection_id IN (protocol spawned as paired GSA owner); R2 settle rule YES (pessimistic, MC_MEDIA_CONNECT_SETTLE_MS); R3 MH-crash/MC-restart repair OUT (story 4); R4 O-11 deferral accepted; R5 paired-protocol authors proto; R6 client consumption of unreachable_sender_ids IN (rendering = task 15). Classification guard STATUS=OK. Plan approved.


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
| Paired Media Handler | confirmed |
| Paired Protocol | confirmed |

---

## Implementation Summary

### The model (ADR-0036 §9; R-33 rev. 2026-09-24)
| Item | Before (task 6) | After (task 20) |
|------|-----------------|-----------------|
| Who a participant is routed through | ONE handler, round-robin by join rank (`MeetingHandlers::place`) | Every handler it is OBSERVED connected to; an edge exists iff the pair shares one, assigned to exactly one shared handler |
| Source of connectivity | none (configured placement) | MH `NotifyParticipantConnected`/`Disconnected`, identity = token `sub`, handler exact-matched against the frozen set; recorded in the meeting actor in the SAME turn as the sender binding |
| Second connectivity store | `MhConnectionRegistry` (keyed by `sub`, no connection identity, literal 1000 cap) | Deleted. `media_routing/connectivity.rs` keyed by `(handler, connection_id)`, refuse-new bound, receive-order tombstone, legacy empty-id path counted |
| `JoinResponse.media_servers` | the one placed handler | the FULL frozen handler set |
| `StreamAssignment.media_handler_url` | the subscriber's handler | the handler owning THAT edge (may differ per slot) |
| `SendTarget`s | the sender's handler | every handler owning one of the sender's edges |
| Which shared handler | n/a | `edges.rs::colocate` (fewest targets per sender, then meeting co-location); correctness proven for ANY chooser |
| Staggered connects | n/a | pessimistic settle window `MC_MEDIA_CONNECT_SETTLE_MS` (required, 100..=10000, 1500 in Kind): single-shot from the first MH connect, ends early at full connectivity, episode floor anchored on the last settle |
| Test seam | `placement_pins` | removed; tests report real connectivity through `McMediaCoordinationService` |

### Core (pure, unit-tested)
- `slots.rs`: `Member.connected: Option<ConnectedHandlers>` (None = not routable), `slots: Vec<Option<Edge{source, handler}>>`; `set_connectivity` is the only connectivity mutation (repairs only edges touching the changed member whose handler left the shared set, then refills); `unreachable_for` excludes not-yet-connected members; `render` places each stream on its edge's handler. Named tests for the canonical case, all-connected, disconnect-moves-only-its-edges, stability across unrelated recomputes, slot stability across a connectivity change, not-yet-connected ≠ unreachable; exhaustive model check (4 people × 2 handlers × 4 connectivity states × slot counts, depth 6, ~16k canonical states per chooser, run under `colocate` AND an adversarial chooser, transition rules for edge and slot stability); fixed-seed property test (400 cases × 3 choosers, determinism replay). Mutation-checked: disabling the "still valid → never touched" rule fails the model check.
- `edges.rs`: `for<'a> fn(&'a [HandlerId], &EdgeContext) -> Option<&'a HandlerId>` — a chooser cannot return a non-candidate.
- `placement.rs`: `place()` and its round-robin tests removed; `MeetingHandlers::resolve` (exact byte match, near-miss test), `ConnectedHandlers` (insert only from a resolved `&HandlerEndpoint`).
- `connectivity.rs`: per-participant state machine (NotConnected → Establishing{not_before, deadline} → Settled), 13 unit tests (early complete, window elapsed, no re-arm, stale disconnect keeps live session, idempotent retry, unknown key, pair-keyed tombstone, bounded FIFO, refuse-new bound, legacy semantics, no spontaneous regain, reconnect-storm floor).

### Actor and ingestion
- `meeting_media.rs`: connectivity per `View`; `media_connected`/`media_disconnected`/`settle_due`/`next_settle_wake`; `sync_routing` pushes the settled set into the slot table only on change (advance-only-on-change at the source) and logs the connectivity INFO line; `reconcile` diffs renders for `mc_media_edge_moves_total` and logs per-participant in-edge summaries; `admit` takes no placement.
- `meeting.rs`: `MediaConnected`/`MediaDisconnected` messages (replacing `GetSenderIdForUser`), `resolve_user` (ambiguous → nothing recorded), settle timer `select!` arm on `tokio::time` (paused-time testable), `JoinResult.media_handlers` = frozen set.
- `media_coordination.rs`: registry gone; `resolve_and_record`; separate `validate_optional_id_field` for `connection_id` (identity fields still reject empty, pinned); hazard comment at the ingestion site (R3).
- `connection.rs`: `media_servers` built from the frozen set.
- `config.rs`/`server.rs`: `MC_MEDIA_CONNECT_SETTLE_MS` (inline `MissingEnvVar` literal kept on one line for `dt-guard env-config`), gauge echo.
- `binding_response.rs`/`messages.rs`: `registry_full` retired (ALL 5→4); both wrong `user_ambiguous` remedy sentences now point at the TODO entry.

### Telemetry (all `key_custody=operator`, no identity labels)
New: `mc_mh_notifications_unapplied_total{reason}`, `mc_mh_notifications_without_connection_id_total`, `mc_media_not_yet_connected_senders_total`, `mc_media_send_targets_total`, `mc_media_edge_moves_total{reason}`, `mc_media_connect_settles_total{outcome}`, `mc_media_connect_settle_window_seconds`. Catalog entries, a new "Media Connectivity" dashboard row (7 panels, per-value colours), and rewrites of the unreachable, slot-state, send-directive, sender-binding, handler-set-divergence and client-reported-status entries/panels. `slos.md` records the settle window as an additive floor for partially-connected participants.

### Tests
- MC integration (`slot_placement_integration.rs`, 21 tests): canonical partial connectivity end to end, all-connected one-target, advance-only-on-change on connect/disconnect/duplicate, edge move keeps slot + counted, stale disconnect keeps live handler, settle TRIPWIRE `staggered_connects_settle_to_one_target_per_sender` (mutation-checked: removing the window yields 2 targets), window-elapsed, no re-arm, reconnect-storm floor, no spontaneous regain, formation burst ≤ one push per settle, gauges; task-6 round-robin/pin/split tests superseded.
- `media_coordination_integration.rs`: out-of-set handler (S4 pin b), unknown meeting, unknown connection, legacy id, retired connection, ambiguous applies nothing, label emittability.
- `media_client_signaling_integration.rs`: full set offered in either Redis order with per-edge urls, and a per-edge redirect/no-narrowing pin (S4 pin a, S3).
- Shared harness: `join_as` reports connections to every handler through the real coordination service; `join_as_on` for partial.
- Env-test 27 rewritten (canonical + all-connected, REACH/positive controls before unreachability, settle wait derived from the published gauge, two ENVIRONMENT triage literals); env-test 28's retired sizing clause removed (P 13 → 11 in Kind).

### Other lanes (authored by their owners; see the table)
- paired-protocol: `connection_id` (tags 4/5) with the full contract comment; eight signaling.proto comment sites; roundtrip pins. `buf lint`/`buf breaking` clean.
- paired-media-handler: `connection_id` threaded at all three notify sites with four pins and a proof-of-trap; `registry_full` retired across MH text; MH configmap comments.
- paired-client: multi-target send (seal once, per-lane hop sequence, S14), multi-transport receive (per-url read loops + hop monitors, task-6 faults removed), first `not_connected` emitter, R6 consumption of `unreachable_sender_ids` into `MediaStore`, e2e `mediaServers.length` assertions and banner.

### Iteration 2 (operator restart directive: env-test 29 shared-gauge anti-pattern)
- `29_mh_meeting_teardown.rs`: release is proven for THIS meeting from MH's direct replies — register at `HELD_GENERATION` (5); generation-1 re-register (owner `mc_id`) is refused as stale (`applied_generation == 5`) after register AND after the wrong-owner reject; owner `EndMeeting` acknowledged; generation-1 re-register then returns `applied_generation == 1` (`RELEASE-NOT-PROVEN` otherwise). Every value assertion on `mh_media_registered_meetings` / `mh_media_egress_edges` removed (RISE/FALL/EDGES-NOT-RETURNED/REJECT-RELEASED-SOMETHING phases gone); presence kept. Kept: LIMITS gauge values (published once from the ConfigMap; no suite moves them) and teardown COUNTER `>= own baseline + 1` (monotonic; concurrency cannot falsify). Module docs (what it proves, containment — gen 5 is held only until the test's own release, Drop guard unchanged — and the metrics section) rewritten.
- `end_meeting_integration.rs`: new `a_release_returns_registered_meetings_and_edges_to_their_previous_values` — with background load held, the full 29 lifecycle moves both gauges and the routing snapshot by exactly this meeting's share and every release returns them to their previous values.
- Sweep: env-tests 27/28 (and 01) read only config-published gauges (settle window, ceilings, ratios, thresholds) and counter deltas — no occupancy-gauge value assertions; nothing to change. `poll_until_pinned_instance` (still used by 29) docs now state the rule; no fixture helper became unused.

### Iteration 2b (Gate 3, Lead ruling on operations F1: per-meeting tiebreak seed)
- `edges.rs`: `meeting_seed()` (FNV-1a 64 over the meeting id — stable across processes and versions, unlike `DefaultHasher`; no clock, counter or RNG), `EdgeContext.meeting_seed`, and `colocate` now rotates the candidate order by `seed % |candidates|`, resolving a tie to the meeting's own starting point. Load precedence (fewest targets > meeting co-location) and edge stability are untouched: the rotation can only decide a genuine tie.
- `slots.rs`: `SlotTable::for_meeting(meeting_id)` is the ONLY production constructor (via `MeetingMedia::new`); the seed-0 `new()`/`Default` are now `#[cfg(test)]` and `with_chooser` is private, so an unrotated live table is a compile error rather than a documented hazard (operations' non-blocking item, taken). `mc-test-utils`' one-handler render fixture moved to `for_meeting` with it (a single candidate leaves the rotation nothing to choose). The seed is policy, not state — excluded from `PartialEq`/`Debug` like the chooser.
- Why in this loop (CLAUDE.md "fix, don't defer"): task 6's round-robin spread load across handlers, so shipping the unseeded tiebreak would have been a fleet-capacity regression — every all-connected meeting on `mh-0`, aggregate egress effectively halved, whole-registration refusals beside an idle sibling.
- Tests: `first_edges_of_distinct_meetings_spread_over_both_handlers` (64 ids, both handlers, ≥8 each; unseeded it is 64/0), `one_meeting_id_always_yields_the_same_choice`, `the_seed_never_overrides_a_load_difference` (64 seeds), `the_seed_is_fnv_1a_64` (published vectors, so a refactor cannot silently re-place every meeting), plus `distinct_meetings_do_not_all_place_their_edges_on_one_handler` and `one_meeting_id_places_reproducibly` in `slots.rs` pinning the production wiring. The model check and the fixed-seed property test are unchanged and green.
- Prose corrected from the unseeded reading in the same change (operations' point 4): `edges.rs` module doc (the tiebreak IS relied on for spread, never for correctness), `docs/TODO.md` item 1 (capacity awareness still open, with its trigger), Scenario 18 (c), `metrics/mh-service.md`, `mh-media.json`. All four now say: roughly balanced pods, read each against its OWN ceiling, one meeting's egress is never split, and the idle sibling is not spare capacity for a refused meeting.

### Iteration 2c (Gate 2 iteration 2 attempt 2: env-test 29 refused on SHARED CAPACITY)
- Symptom: 29 failed `PRECONDITION ... did not install the test meeting's generation-5 policy`, 0 vs 5. mh-0 was at 39 installed streams against a ceiling of 40 (suites 26/27/28 draining), `rejected_stream_ceiling` = 4 on that pod; the edges drained to 0 minutes later, so nothing leaked. MH refuses a registration WHOLE over the ceiling, so 29's two-edge policy was refused — the same cross-suite interference the operator directive removed for gauges, reached through admission.
- Fix (same principle: do not depend on a shared resource at all): 29 registers an EMPTY policy at every generation. The proof is about generation and ownership, and needs no edges. Confirmed with @paired-media-handler against MH code before changing: empty `egress_streams` at gen N is an ordinary apply echoing N (per-element validation is inside the loop; the snapshot entry is inserted unconditionally); gen 1 against held gen 5 still short-circuits to `RejectedStale` above both capacity arms; after `EndMeeting` an empty gen-1 register echoes 1; and **a zero-edge policy is structurally unrefusable on capacity** — the projection equals the pod's current total and both arms compare with strict `>`, while installed can never exceed the pod's own ceiling. `two_edges()`/`TWO_EDGES` deleted with their proto imports.
- Distinct, self-explaining capacity failures (Lead + MH caveat A): a `RESOURCE_EXHAUSTED` on the first register fails with its own `CAPACITY` token naming `MH_MAX_REGISTERED_MEETINGS` (the only shared-capacity REFUSAL left), and the `applied_generation` mismatch now says a zero-edge policy cannot be refused on edge capacity and points at config-apply back-pressure (`config_apply_mailbox_full` / `config_apply_timeout`, an OK reply carrying the PRIOR generation) — so the next Gate-2 failure is not misread as the ceiling again.
- Module docs rewritten: a "This test registers NO edges, and that is load-bearing" section (why, the Gate-2 evidence, the structural argument, the second benefit — 29 can no longer pollute 28's `rejected_stream_ceiling` signal, and "do not strengthen this test by giving it edges"), plus MH caveat B (no edges ⇒ `transport_mode` decodes `UNSPECIFIED`; never assert one) and the stale two-edge/"its edges" references.
- Gate 3 (@test): step 6's post-release re-register had step 1's diagnostics missing — on a busy pod config-apply back-pressure makes MH echo 0 (the just-released meeting's prior generation), which failed the `RELEASE-NOT-PROVEN` assert and pointed the reader at a teardown regression. Now split: the RPC error takes the same `RESOURCE_EXHAUSTED` → `CAPACITY` fork, and an echo of 0 fails as `BACK-PRESSURE` (explicitly not teardown; the release was already acked and counted) while only an echo of `HELD_GENERATION` reads as a real teardown regression.
- Sweep of the other Layer-7 suites: 28's ceiling breach and 27's real edges are each FORCED by what the test proves, and 28's assertions are monotonic counter deltas that shared load can only help. Nothing to change functionally; 27's non-convergence panic now tells the reader to rule out shared-capacity refusal on the carrying handler (with the two PromQL series) before reading it as an MC routing defect, which is the one way that suite could be misdiagnosed the way 29 was.

### Deferred (recorded, not dropped)
- Capacity-aware spreading, restart repair (R3), self-asserted handler identity (S1b): one rewritten `docs/TODO.md` §Media Path Obligations entry (task 6's entry, in place).
- O-11 client-vs-MH divergence detector: its own TODO entry, with the two causes restart detection does not cover and observability's interim-signal paragraph (R4).
- Unreachable DOM rendering: story task 15 (R6).


---

## Files Modified

See the Cross-Boundary Classification table for every path and its owner. Key MC files:

| File | Change |
|------|--------|
| `crates/mc-service/src/media_routing/slots.rs` | shared-handler edges, `set_connectivity`, model check + property test |
| `crates/mc-service/src/media_routing/edges.rs` (new) | edge-handler chooser; iteration 2b: `meeting_seed` + per-meeting tiebreak rotation |
| `crates/mc-service/src/media_routing/connectivity.rs` (new) | observed connectivity, tombstone, settle window |
| `crates/mc-service/src/media_routing/placement.rs` | round-robin removed; `resolve`, `ConnectedHandlers` |
| `crates/mc-service/src/actors/meeting_media.rs`, `meeting.rs`, `messages.rs`, `controller.rs` | connectivity ingestion, settle timer, edge-move diff, registry removal |
| `crates/mc-service/src/grpc/media_coordination.rs` | notifications → actor; optional-id validator; hazard comment |
| `crates/mc-service/src/mh_connection_registry.rs` | deleted |
| `crates/mc-service/src/config.rs`, `webtransport/server.rs`, `webtransport/connection.rs` | settle key + gauge; full `media_servers` |
| `crates/mc-service/src/observability/metrics.rs` | 7 new emitters, zero-init |
| `crates/mc-service/tests/slot_placement_integration.rs`, `media_coordination_integration.rs`, `media_client_signaling_integration.rs`, `common/media_session.rs` | rewritten / extended as above |
| `crates/env-tests/tests/27_mc_slot_placement.rs`, `28_mh_egress_admission.rs`, `src/fixtures/mc_session.rs` | env-test rework |
| `crates/env-tests/tests/29_mh_meeting_teardown.rs`, `src/fixtures/metrics.rs` | iteration 2: release proven from MH direct replies; no shared-gauge value assertions |
| `crates/env-tests/tests/29_mh_meeting_teardown.rs`, `27_mc_slot_placement.rs` | iteration 2c: empty policy (no shared-capacity dependency), CAPACITY/back-pressure diagnostics, 27 triage hint |
| `crates/mh-service/tests/end_meeting_integration.rs` | iteration 2: exact occupancy accounting after release |
| `infra/services/mc-service/{configmap,mc-0-deployment,mc-1-deployment}.yaml` | `MC_MEDIA_CONNECT_SETTLE_MS` |
| `docs/observability/metrics/mc-service.md`, `infra/grafana/dashboards/mc-overview.json`, `docs/observability/slos.md` | catalog, Media Connectivity row, settle floor |
| `docs/runbooks/{mc-incident-response,mc-deployment,client-dev-local,devloop-validation,gc-incident-response,mh-incident-response}.md` | Scenario 13/18 rewrites, deploy step 0, carve-out #4, config row, triage literals, OPS-10 hunks |
| `docs/TODO.md`, specialist INDEXes | as described |


---

## Devloop Verification Steps

Final Gate 2 (iteration 2, attempt 4): `DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` → GATE2=PASS, LAYER_ALL_EXIT=0; L1-3/5 OK, L4/L6 N/A (proto-lane placeholders; cargo-test 4401 passed / 0 failed, nx-test, audits, buf-breaking OK), L7 OK (env-tests incl. 27/28/29 on the seeded tiebreak; browser-e2e 10/10). Verdict `/tmp/devloop/gate2-verdict` RUN_AT 2026-09-25T21:46:40Z.

---

## Code Review Results

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 2 | 2 | 0 | S-1 S10 floor gap documented (no mechanism change requested); S-2 invariant-violation helper. GSA lens satisfied. |
| Test | RESOLVED-FIXED | 1 | 1 | 0 | env-tests 27/28/29 + end_meeting_integration property/parallel-safety confirmed; iteration-2c finding: step-6 post-release re-register lacked step-1's CAPACITY/back-pressure diagnostic split (echo 0 = config-apply back-pressure, not teardown) — fixed |
| Observability | RESOLVED-FIXED | 7 | 7 | 0 | client per-lane queue semantics, registry_full doc, mc-overview panels, INDEX, dashboards.md (O-21 closed), TODO arity; F7 (post-seed) single-home deferral in `edges.rs` |
| Code Quality | CLEAR | 0 | 0 | 0 | ADR-0002/0003/0011/0032/0036 compliant |
| DRY | RESOLVED-FIXED | 4 | 4 | 0 | registry_full 13th site, try_mh_open_connect single home, ANCHOR(DRY)+pin test, stale TODO entries. make_controller hoist pre-existing TODO, not a finding deferral |
| Operations | RESOLVED-FIXED | 7 | 7 | 0 | F1 fleet-wide co-location — **fixed in code** per Lead ruling (per-meeting tiebreak seed) plus the four operator-facing sites; registry_full; required-key count; MH `connection_id` rollback + gate item; ConfigMap ORDERING; INDEX route; Scenario 18 alert decision homed. Both non-blocking items also taken (`SlotTable::new` gated `#[cfg(test)]`; 18(c) wording) |
| Semantic Guard | CLEAR | 0 | 0 | 0 | |
| Paired Client | CLEAR | 0 | 0 | 0 | |
| Paired Media Handler | RESOLVED-FIXED | 2 | 2 | 0 | test-29 string continuations; derived edge counts. Also confirmed MH semantics for the iteration-2 generation proof and the iteration-2c empty policy (3 caveats, all folded in) and re-checked both |
| Paired Protocol | CLEAR | 0 | 0 | 0 | GSA owner; buf lint, 16 roundtrip pins |

---

## Accepted Deferrals

None — no reviewer finding was deferred (all verdicts CLEAR or RESOLVED-FIXED). Scope decisions ruled at Gate 1 (R3 restart repair, R4 O-11 detector, R6 DOM rendering → task 15) and capacity-aware edge spreading (task scope) remain tracked in `docs/TODO.md` §Media Path Obligations; the `make_controller` hoist is a pre-existing `docs/TODO.md` entry, not a Gate-3 deferral.

---

## Rollback Procedure

1. Start commit: `8dd3afd906160af258bdfa9494ebc18a5fda915e`
2. `git diff 8dd3afd9..HEAD`; `git reset --soft|--hard 8dd3afd9`

---

## Issues Encountered & Resolutions

### Issue 1: Gate 2 attempt 1 — env-test 27 failed twice, both test-observation bugs, not MC behaviour
**Problem**: (a) CANONICAL asserted "C is unreachable to B" on a view that predated C's settle. B and C each settle one window after their OWN first connect, and C connects after B, so B's view first converges on "hears A" (at B's settle) and only later gains C in `unreachable_sender_ids` (at C's settle). The test stopped reading at the first matching view. (b) ALL-CONNECTED's `bind_until_received` counted ANY marker toward completion, so a receiver holding several senders returned on a frame still in flight from an earlier pair's phase and then indexed a missing marker.
**Resolution**: (a) after the positive controls, B's and C's views are read on (bounded by the published window + margin) until they carry the settled unreachable set, then asserted — ordering and assertions unchanged. (b) `bind_until_received` counts only the markers it waits for, tolerates markers of senders the receiver legitimately holds (`also_held`), and **fails immediately on any other marker** — a positive, non-timed observation of a cross-handler leak that did not exist before, so the fix strengthens the test. Verified against the live Kind cluster: 5/5 green; `mc_media_connect_settle_window_seconds` = 1.5 on both MC pods and `mc_media_connect_settles_total` shows `window_elapsed` = 10 (B and C × 5 runs) beside `complete` = 24, i.e. the settle path genuinely engaged.


---

## Gate 2 — Validation Log (Lead)

| Attempt | L1-6 | L7 | Detail |
|---------|------|----|--------|
| 1 | OK (L4/L6 N/A proto-lane only) | FAIL `env-tests-failed` | env-test 27: both tests failed on test-observation bugs (asserted the first view before C's settle; `bind_until_received` counted any marker). Implementer fixed the test only; no MC change. 27 then passed 5/5 on the live cluster. |
| 2 | OK | FAIL `env-tests-failed` | env-test 27 passed (2/2). env-test 29 `a_long_running_mh_pod_releases_an_ended_meeting_and_admits_like_a_fresh_one` failed `RISE-NOT-OBSERVED` at `crates/env-tests/src/fixtures/metrics.rs:705`: on pod 192.168.91.226 `mh_media_egress_edges` did not reach >= 33 (baseline 31 + two edges) within 120s; last value 2 (all instances {.226=2, .227=0}). The baseline was captured while ~31 edges from earlier suites (26/27/28) were still live, and those edges drained during the window. Attempt 1 never evaluated test 29, because it came after the failing 27. |

**Outcome**: the Layer-7 budget (2 attempts) is exhausted, so this is escalated headless per the devloop skill (`.devloop-escalation.json`, reason `validation-attempts-exhausted`). Gate 3 review was not started, and nothing was committed.

---

## Human Review (Iteration 2) — Operator Restart Directive

**Feedback** (run-story --restart, authoritative operator diagnosis): Env-test 29 failed because it asserts on pod-wide gauges (`mh_media_egress_edges`, `mh_media_registered_meetings`) that other suites change concurrently. The gauge drained from 31 to exactly the test's own 2 edges, so ended meetings DO release edges under the new model — a test-design problem, not a teardown regression. Fix: (1) do NOT make 29 wait for gauges to settle; (2) prove release for THIS meeting from MH direct replies — register at generation 5; re-register at generation 1 must be refused as stale; EndMeeting from the owning MC is acknowledged; re-register at generation 1 then returns `applied_generation == 1`; keep wrong-owner and unknown-meeting checks; (3) remove every assertion on the VALUE of a shared gauge from 29 (series-existence checks OK); (4) put exact accounting (edges and registered meetings return to previous values after release) in in-process `crates/mh-service/tests/end_meeting_integration.rs`.

**Lead handling**: Gate 1 plan stands (confirmed iteration 1); this directive is an operator-authored plan amendment, scoped to test files. Gate 2 attempt budget resets per operator restart. Full Gate-3 panel reviews the whole diff.

### Gate 2 — Iteration 2 (post operator restart; attempt budget reset)

| Attempt | L1-6 | L7 | Detail |
|---------|------|----|--------|
| 1 | OK (L4/L6 aggregate N/A from proto-lane placeholders; cargo-test, nx-test, cargo/pnpm audit, buf-breaking all OK) | OK | env-tests-passed incl. 29 (`a_long_running_mh_pod_releases_an_ended_meeting_and_admits_like_a_fresh_one` ok, 92s) and 27; browser-e2e-passed (10). Log: `/tmp/gate2-it2-a1.log`. |

**RESIDUAL — L7 PREDATES A RUNTIME CHANGE (raised by @operations, recorded rather than assumed).** This attempt ran BEFORE the Gate-3 per-meeting tiebreak seed (Iteration 2b), which changes WHICH handler carries each edge at runtime — exactly what `crates/env-tests/tests/27_mc_slot_placement.rs` exercises on the live two-handler cluster, with `28_mh_egress_admission.rs` observing the per-pod occupancy the rotation redistributes. It should pass by construction (27 asserts properties, never handler identities, and the model check proves correctness for ANY chooser, including the rotation) — but that is the argument, not the evidence. L1-6 are green post-seed (`layer-fast` rc=0; `cargo test -p mc-service -p mh-service -p mc-test-utils` green; `media_routing` 86/86). **Re-run L7, or at minimum 27 and 28, before commit.** Lead's call if the attempt budget blocks it; unrun is auditable here, an assumed pass would not be.
| 2 | OK | FAIL `env-tests-failed` | Re-run after Gate-3 fixes (per-meeting tiebreak seed, constructor gating). env-test 29 PRECONDITION at :458 — first gen-5 RegisterMeeting on mh-0 returned applied_generation 0: refused whole by the stream-ceiling admission (`rejected_stream_ceiling`=4 on .226; `max_over_time(mh_media_egress_edges)`=39 vs ceiling 40, from earlier suites' meetings still draining; drained to 0 minutes later — no leak). Same cross-suite shared-state class as the operator directive, via capacity instead of gauges. Routed: 29 registers with zero edges. (Note: an earlier launch of this attempt was killed by the tool timeout after L4 and is not counted.) |
| 3 | OK | OK (verdict invalidated) | Post zero-edge 29 + step-6 diagnostics. env-tests-passed (incl. 27, 28, 29 on the seeded tiebreak) and browser-e2e-passed (10), but a concurrent teammate `layer-fast` deleted the shared `/tmp/devloop/layer-*.log` mid-run, so the orchestrator could not parse L7 and wrote GATE2=FAIL/LAYER_ALL_EXIT=2. Test outcome was a pass; not counted as an L7 failure. Re-run clean as attempt 4 with no concurrent builds. Log `/tmp/gate2-it2-a3.log`. |
| 4 | OK | OK | Clean run, no concurrent builds. GATE2=PASS, LAYER_ALL_EXIT=0. env-tests-passed (27, 28, 29 on the final tree incl. seed + zero-edge 29), browser-e2e-passed. Log `/tmp/gate2-it2-a4.log`. |
