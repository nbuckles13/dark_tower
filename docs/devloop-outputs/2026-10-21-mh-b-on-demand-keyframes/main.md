# Devloop Output: MH-B on-demand keyframes (story 3 task #10)

**Date**: 2026-10-21
**Task**: MH-B: MH on-demand keyframes, receiver RecoveryRequest remap + counter dedupe, pending visibility, amendment-4 cleanup (full text: run-story task-10.prompt)
**Specialist**: media-handler (paired: client, protocol, infrastructure; security/test/operations paired in their panel seats)
**Mode**: Agent Teams (v2) — full, Gate-1 present
**Branch**: `feature/user-see-each-other`
**Duration**: ~8h (planning ~25m, implementation ~1h, review ~45m, two Gate-2 runs ~25m each)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `ada1fb18ee939b12507da3a432046c8023a64362` |
| Branch | `feature/user-see-each-other` |
| Lead Model | `claude-opus-5-5[1m]` |

---

## Loop State (Internal)

<!-- This section is maintained by the Lead for state recovery after interruption. -->
<!-- Do not edit manually - the Lead updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` |
| Implementing Specialist | `media-handler` |
| Tier | `full` |
| Iteration | `2` |
| Security | `security` (paired seat) |
| Test | `test` (paired seat) |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` (paired seat) |
| Semantic Guard | `semantic-guard` |
| Paired | `paired-client`, `paired-protocol`, `paired-infrastructure` |

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
Story 3 task 10 (MH-B): event-driven keyframes at MH per story Amendment 4. MH requests a keyframe from a publisher at most once per (source stream, GoP counter) and never retries. Receiver `RecoveryRequest`s are remapped to the slot's current source and deduped by counter, sharing one record per source stream with MH's own requests. Each outbound queue keeps only the newest request. "Pending" is visible only as MH metrics. Over-limit streams have their header read before they are stopped. The three MH keyframe/recovery keys and MH's ref to `MEDIA_VIDEO_MIN_KEYFRAME_INTERVAL_MS` are removed, and the landed proto/ADR/runbook/deploy text is corrected. The T-A fixture is extended with a publisher-side abandonment fault.

### Scope
- **Service(s)**: mh-service (code); env-tests (fixture + new env-test 39 + env-test 01 cleanup); deploy config (mh-service, media-codec-profile comments); proto comments; ADR-0036; runbooks; observability catalog + dashboard
- **Schema**: No
- **Cross-cutting**: Yes (proto comments, ADR, deploy config, runbooks; no wire change)

### Debate Decision
NOT NEEDED - Amendment 4 (user, 2026-10-21) already settled the design; this task implements it.

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
| `crates/mh-service/src/media/keyframe.rs` (new) | Mine | — |
| `crates/mh-service/src/media/mod.rs` | Mine | — |
| `crates/mh-service/src/media/edge.rs` | Mine | — |
| `crates/mh-service/src/media/edge_runtime.rs` | Mine | — |
| `crates/mh-service/src/media/stream_ingress.rs` | Mine | — |
| `crates/mh-service/src/media/caps.rs` | Mine | — |
| `crates/mh-service/src/routing/mod.rs` | Mine | — |
| `crates/mh-service/src/session/mod.rs` | Mine | — |
| `crates/mh-service/src/transport/mod.rs` | Mine | — |
| `crates/mh-service/src/transport/framing.rs` (new) | Mine | — |
| `crates/mh-service/src/webtransport/connection.rs` | Mine | — |
| `crates/mh-service/src/webtransport/media_transport.rs` | Mine | — |
| `crates/mh-service/src/observability/metrics.rs` | Mine | — |
| `crates/mh-service/src/config.rs` | Mine | — |
| `crates/mh-service/src/main.rs` | Mine | — |
| `crates/mh-service/tests/**` | Mine | — |
| `crates/mh-test-utils/src/**` | Mine | — |
| `docs/specialist-knowledge/media-handler/INDEX.md` | Mine | — |
| `crates/env-tests/tests/39_mh_video_keyframes.rs` (new) | Not mine, Minor-judgment | test |
| `crates/env-tests/src/fixtures/video/**` | Not mine, Domain-judgment | test |
| `crates/env-tests/tests/38_mh_video_forwarding.rs` (backlog sizing moved to the fixture) | Not mine, Minor-judgment | test |
| `crates/env-tests/tests/01_mh_deployment_config.rs` | Not mine, Minor-judgment | infrastructure |
| `proto/dark_tower/signaling/v1/signaling.proto` (comments only) | Not mine, Domain-judgment | protocol |
| `docs/decisions/adr-0036-media-flow.md` | Not mine, Domain-judgment | protocol |
| `infra/services/mh-service/config.env` | Not mine, Minor-judgment | infrastructure |
| `infra/services/mh-service/mh-0-deployment.yaml` | Not mine, Minor-judgment | infrastructure |
| `infra/services/mh-service/mh-1-deployment.yaml` | Not mine, Minor-judgment | infrastructure |
| `infra/services/media-codec-profile/profile.env` (comments only) | Not mine, Minor-judgment | infrastructure |
| `.env.example` | Not mine, Minor-judgment | infrastructure |
| `docs/DEVELOPMENT.md` | Not mine, Minor-judgment | infrastructure |
| `docs/runbooks/mh-deployment.md` | Not mine, Minor-judgment | operations |
| `docs/runbooks/mc-deployment.md` | Not mine, Minor-judgment | operations |
| `docs/observability/metrics/mh-service.md` | Not mine, Minor-judgment | observability |
| `infra/grafana/dashboards/mh-media.json` | Not mine, Minor-judgment | observability |
| `scripts/layer7.sh` (env-test budget default + comment) | Not mine, Minor-judgment | infrastructure |
| `scripts/layer7.test.sh` (budget drift guard) | Not mine, Minor-judgment | infrastructure |
| `docs/runbooks/devloop-validation.md` (§6.7 budget restatement) | Not mine, Minor-judgment | operations |
| `docs/decisions/adr-0033-polyglot-validation-pipeline.md` (Layer-7 budget pointer) | Not mine, Minor-judgment | infrastructure |
| `packages/web-app/e2e/README.md` (browser budget pointer) | Not mine, Minor-judgment | client |

---

## Planning

### Mechanism restated
Restated as a mechanism, the task is: **MH-held per-(binding, source stream) GoP state is the single source for every keyframe decision.** Seen, newest counter, the dedupe record and the newest-only outbound slot all live in one place. Every path that wants a group start from a source calls one method on it:
- edge entry into Waiting or SwitchPending (initial, abandonment, switch, subscriber connect);
- a remapped receiver request;
- the over-limit stream that MH stopped.

The wider class this exposes, all handled in this plan:
- (a) The landed per-edge `seen_at` / `waiting_with_source_seen` is a second, narrower notion of "seen" (seen *while waiting*). It would miss a source seen before the edge started waiting, such as after an abandonment. It is **deleted**, and the per-source state is authoritative (DRY #4).
- (b) An edge left `Forwarding` after MH refused its source's next group (stream-open limit) can never recover: the publisher does not re-key for MH's refusals. So MH's refusal must count as an MH-decided abandonment of that source's forwarding edges (see Item 2, "SEC-S2(c) tension").
- (c) Every "refused before a byte is read" text instance (ops list).
- (d) Every "MH reads the min interval / MH-B's three keys" instance (infra + ops lists).

### Design

**1. Per-source state: `media/keyframe.rs` (new; pure, under the §11 deny)**
- `PublisherKeyframes` is owned by ONE publisher connection (its binding). It is created in `start_media_session` and held by `Arc` in `StreamIngressContext`.
- It is registered in a meeting-scoped `PublisherKeyframesRegistry` keyed by `(MeetingKey, SenderId)`, with value `(connection_id, Arc<..>)`. Like `LocalSubscribers`, it uses `ArcSwap` + `rcu` with compare-and-remove `unregister(meeting, sender, connection_id)` (SEC-2 pattern).
- Because state hangs off the binding, an R-16 reissue or a same-participant reconnect gets a fresh object by construction (gate iv). Unbind frees it: teardown unregisters, the last `Arc` drops, and the writer task is cancelled.
- Inside it: `Mutex<HashMap<StreamNumber, SourceRecord>>` + `Notify`.
  - `SourceRecord { newest: u32, sent: Option<u32>, queued: Option<(u32, Trigger)> }`.
  - "seen" == the record exists. There is no `0` sentinel (client C).
- **Keying (SEC-S3):** a record is created or updated ONLY for a label that `RoutingSnapshot::attribute` resolves to a declared stream of the bound sender on `STREAM_PER_GROUP`, i.e. `Route` (forwarded / paused / no edges = unrouted) or `ServerMuted`. To get the `StreamNumber` for `ServerMuted` without a second rule, `attribute` gains the narrowed stream on `ServerMuted { stream: Option<StreamNumber> }` (None for the sender-level shortcut on an undeclared label). It creates nothing on NoPolicy / Unmatched / TransportMismatch.
  - The bound therefore holds by construction: at most the sender's declared streams, ≤ `MH_MAX_SOURCE_STREAMS_PER_MEETING`, and ≤ 256 by the `StreamNumber` type. No new cap constant (DRY #6).
- **Unassign:** `RoutingTable::install` gets the registry from `EdgeContext`. After the swap, for each registered publisher of that meeting, it `retain`s only records whose `(sender, stream)` the new policy still declares. `RoutingTable::remove` (EndMeeting) clears the meeting's records. These are the named hooks the gates call.
- **Newest = max(), not last-read (client A; answer to SEC-S6).** With cut-through, group N's stream can still deliver frames after group N+1's header. Last-read would regress, so MH's request would be stale at the publisher and silently freeze. Counters never wrap, so max is safe for an honest publisher.
  - A hostile publisher regressing or jumping its counter only (i) makes its own requests stale or ignored, or (ii) makes its own subscribers' receiver requests `stale` / `counter_ahead`. That harms only its own media, which is documented in the module.
  - Gate: header N+1, then a late frame of N → newest stays N+1.
- **Record path** (`record(stream, counter)`, called per header on the stream path): `newest = max(newest, counter)`. If `queued` holds a counter < newest, it is discarded, because a group start superseded it (the "discarded on group start" gate). No allocation after the first frame of a stream.
- **THE one dedupe method** (code-reviewer #2): `request(stream, trigger) -> RequestOutcome { Queued, AlreadyRequested, NotSeen }`.
  - It requests at `newest`. It is "already requested" iff `sent == Some(newest)` or the queued counter == newest.
  - A newer counter REPLACES a queued older one (newest-only).
  - Item 2 (MH-originated, `Trigger::Initial`) and item 3 (`Trigger::ReceiverRecovery`, after the equality check) both call it.
- **Discard API:** `discard_queued(stream)`, used for edge removal / PAUSED / DROP_AT_INGRESS (below). Discarding an unsent request leaves `sent` untouched, so the same counter can be requested again if wanted again; it was never sent.
- **Writer task** (one per publisher connection; spawned in `start_media_session`). It waits on the `Notify`, takes queued slots under the lock (lock released before any await), encodes `MhServerMessage{KeyframeRequest{stream_number = the source's uplink id, gop_counter = MH's newest}}` with trace tags unset (client F/G), and writes it framed.
  - On Ok: `sent = Some(c)` and `keyframe_requests_sent_total{trigger}` +1.
  - On Err: `keyframe_request_write_failures_total` +1, `sent = Some(c)` (counted, never re-emitted), and the outbox is marked `failed`.
  - After `failed`, every later request that would be queued is marked sent and counted as a write failure immediately, so nothing queues forever (ops Q2). The writer then exits.
  - A publisher that never reads its control stream (today's pre-task-11 SDK, client E) blocks only its own writer task forever. Memory stays at most one slot per declared stream, and forwarding never waits. There is a gate for this.
  - Primitive (code-reviewer #6, semantic-guard 2): **Mutex'd per-stream slot map + `tokio::sync::Notify`**, drained by the writer. Not mpsc, not watch.

**2. MH-originated requests (R-10)**
- **Edge machine (`media/edge.rs`):** `seen_at` is removed from `Waiting`/`SwitchPending`. `waiting_with_source_seen` is replaced by `awaiting_group_start() -> Option<CandidateRef>`, which is `Waiting{target: Some(t)}` → t or `SwitchPending{target}` → target. The machine stays pure.
- **`StreamEdge` keeps `target_forwardable`,** set at reconcile from `SourceStatus::Forward`. An edge *wants* a keyframe iff `awaiting_group_start()` is Some AND the target is forwardable. Paused, muted or undeclared targets never request.
- **One helper `StreamEdge::settle(now, cause)`** runs after EVERY machine transition: `on_frame`, `on_abandoned`, `assign`, `on_source_ended`, release. It does two things:
  - Pending accounting (item 7).
  - If the edge newly wants source S (entered Waiting or SwitchPending, or its target changed), it calls `registry.lookup(meeting, S.sender)?.request(S.stream, Initial)`.
    - The registry and `MeetingKey` reach the cell through `EdgeContext` (the cell stores its meeting key at creation).
    - Lock order is always edge → publisher record, never the reverse.
- **Ingress trigger:** after `record()`, `deliver_header` evaluates each of S's edges that still wants S (cheap check under the edge lock it already holds) and calls `request`. Dedupe makes this once per counter, which covers "source first seen while an edge already waits".
  - It is suppressed when the subscriber has no local downlink channel (`egress == None`). Otherwise an edge whose subscriber is not (yet) connected would request at every new counter, a keyframe loop at the minimum interval.
  - In exchange, `start_media_session` calls a new `RoutingTable::subscriber_connected(meeting, sender)` after registering the subscriber. It runs the same request for that subscriber's wanting edges.
- **Abandonment** (backlog, propagated publisher reset, stall, lost downlink) already goes through `on_abandoned`, which goes through `settle`. That gives a request via Waiting.
- **Never seen:** `request` returns `NotSeen` and nothing is sent. No timer anywhere and no give-up timeout.
- **Queued-request discard:**
  - On group start: the record path.
  - On edge removal, PAUSED or DROP_AT_INGRESS: at the end of `MeetingRoutes::build`, for each declared stream source whose edges no longer have any wanter (or whose status is not Forward), call `discard_queued`.
  - On unbind: the object is dropped.
- **Over-limit stream (`stream_ingress.rs`):**
  - The bucket-refused arm spawns `read_header_then_stop` into the SAME `JoinSet`, so it never blocks the accept loop (semantic-guard 1, SEC-S2a). Live readers are bounded by the declared QUIC concurrent-uni-stream limit (`MH_MAX_CONCURRENT_UNI_STREAMS`, `config.rs`, applied in `webtransport/server.rs:build_transport_config`). The loop's drop aborts them.
  - It reads through the same `HeaderReader` via one shared helper with the group path (code-reviewer #8, DRY #5). It buffers nothing past the header and feeds the tail nowhere.
  - On `Complete` it attributes; if resolved, it calls `record(stream, counter)`. Then `stop(INGRESS_REFUSED)`.
  - A malformed header (or FIN/RESET before the header completes) is stopped with no record.
  - `stream_rate_limited` is counted exactly once per refused stream on every arm. The existing codec-token counters are not counted on this path.
- **SEC-S2(c) tension, raised for security:** recording the counter alone does not recover a subscriber whose edge is `Forwarding` S. Its next group was just refused and the publisher will not re-key for MH's refusal. The edge sits `Forwarding{group: old}` forever with no request, which violates R-12. Proposal: after a refused stream's header resolves to S, `RoutingTable::source_group_refused(meeting, sender, stream)` calls the machine's `on_abandoned` for edges *Forwarding S* whose current downlink is not mid-frame. Their predecessor downlink is left to finish normally. `settle` then requests at the refused group's counter, which the publisher honours (`>= current`).
  - It touches no downlink, `group_starts`, `forwarded` counter or hop. Only machine state and the request.
  - This is exactly "MH repairs what it stopped".
- **Stall timer (SEC-S1):** today it is armed while `is_mid_header()`, including before the stream's first header. The new predicate is `self.first_header_done && self.mid_frame()`, where `first_header_done` is set on the first `Feed::Complete` regardless of attribution (so a muted-first stream is also bounded after its header).
  - A partial header on frame N>1 still resets downstream with `publisher_stall`.
  - Tests for both sides.

**3. Receiver RecoveryRequests (R-13)**
- Logic is in `media/keyframe.rs` (SEC-S5e): `classify_recovery(snapshot, registry, meeting, own_sender, req) -> RecoveryOutcome`. It is a pure function over snapshot + registry, with a closed enum and `as_str()`.
  - **Resolve:** `MeetingRoutes` gains an index `(subscriber SenderId, SlotId) -> edge index`, built in `build` and meeting-scoped. It is resolved with THIS connection's own sender only (SEC-S5f). An unknown slot, a non-stream edge (audio/datagram, client R2), or an edge whose assigned candidate is missing or not `Forward` → `unmapped_slot`.
  - **Current source = MC's assigned candidate** (B during SwitchPending, per client B and the SWITCH-RACE gate). `SenderId::from_wire(req.sender_id)` failing (0) or ≠ candidate.sender → `source_mismatch`.
  - **Counter:** if the publisher is not registered or has no record (never seen) → `counter_ahead` (client C). Lower → `stale`, higher → `counter_ahead`. Equal → `request(stream, ReceiverRecovery)`: `Queued` → `forwarded`, `AlreadyRequested` → `duplicate`.
  - Nothing from the client is forwarded. The upstream message is built only from the record.
- **Control-stream framing:** one module `webtransport/control.rs` (sibling, not media).
  - `read_framed_message` is generalised in place (DRY #1). The connect read and the post-connect loop both use one `FrameReader` over the seam trait.
  - It reads the 4-byte prefix. 0 or > `MAX_MESSAGE_SIZE` is a **framing break**: `malformed` +1, the loop stops reading the control stream, and the connection is KEPT (SEC-S5b). End detection then falls to `connection.closed()` and the close token.
  - A body up to `CONTROL_PARSE_BYTES` (1 KiB, a fixed stack buffer, with no per-message allocation per SEC-S5a) is decoded.
  - A larger body (≤ MAX) is drained in fixed chunks and counted `malformed`. That keeps framing in sync and allocates nothing. A conforming `RecoveryRequest` is < 40 bytes; @paired-client please confirm the SDK sets no trace tags on post-connect messages.
  - Decode error, empty oneof, a second `connect_request` (never re-auths, rebinds or notifies; test) → `malformed`, and the loop continues. Trace tags are ignored and no span is created (SEC-S5d).
  - EOF keeps `ClientClosed`, and a read error keeps `Error`.
  - The writer uses `write_framed` next to the reader, with `u32::try_from` (DRY #2).
- **Seam (DRY #3; test E):** `transport/mod.rs` gains a byte-level `ControlRecv` / `ControlSend` pair:
  - `read(&mut self, &mut [u8]) -> Result<Option<usize>, TransportError>`;
  - `write_all(&mut self, &[u8]) -> Result<(), TransportError>`.
  - The real impl wraps the wtransport `RecvStream`/`SendStream` from `accept_bi` (the send half is no longer discarded at l.220).
  - The shim (`mh-test-utils`) implements both over its existing control queues. It gains `block_control_writes()` / `release_control_writes()` and a "never drains" mode, which is the deterministic blocked-write seam.
  - connection.rs keeps lifecycle only. Its Step-6 select calls `control::next_message` and `keyframe::classify_recovery`, with no per-request log.
- **SEC-S6 audio:** an audio slot on a datagram edge is `unmapped_slot`. A stream-per-group audio edge could forward at most one request per counter, i.e. ≤ the publisher's frame rate, and the publisher's minimum interval bounds the keyframes produced. This is documented, not special-cased (MH is type-blind).

**4. Telemetry (item 7).** Handles are resolved in `resolve_media_handles`, every label is zero-initialised, and no identity/kind/counter/timing appears in labels, logs or spans.
- `mh_media_keyframe_requests_sent_total{trigger=initial|receiver_recovery}`, counted on a successful write only.
  - `initial` = every MH-originated request: first join, switch, subscriber connect, post-abandonment, refused-group repair. The catalog says so (obs #5).
- `mh_media_recovery_requests_received_total{outcome=forwarded|duplicate|stale|counter_ahead|source_mismatch|unmapped_slot|malformed}`, exactly one per message or framing break.
- **NEW** `mh_media_keyframe_request_write_failures_total` (unlabelled), for obs #4 / ops Q2. Catalog row + panel.
- `mh_media_edges_pending` (sum) and `mh_media_edge_pending_oldest_seconds` (max), tick-sampled by the session actor's existing rotation tick via `RoutingTable::sample_pending(now)`. It walks every cell's `pending_since` and is the single writer, so there is no last-writer-wins. Both are set to 0 at setup (present-at-zero, ops Q1), and oldest is 0 when nothing is pending.
- **Pending definition:** a stream edge wants a group start from an assigned, `Forward` source, i.e. Waiting(target) or SwitchPending, seen or not (so a never-sending source raises it).
- `mh_media_edge_pending_duration_seconds` (histogram; buckets `[0.05,0.1,0.25,0.5,1,2.5,5,10,30,60,120,300]` as a named const + `set_buckets_for_metric`), observed once at every exit.
- `mh_media_edge_pending_exits_total{outcome}`: exactly one per exit, from `settle`:
  - `group_start`: the wanted source's group start was forwarded.
  - `abandoned_source_left`: the target became undeclared (its publisher left the policy).
  - `superseded`: MC assigned another source or none (a target change exits and re-enters).
  - `paused_forward_nothing`: the target became PAUSED **or DROP_AT_INGRESS**, so the policy forwards nothing. @observability, please confirm folding mute here; the label set is fixed by the prompt.
  - `edge_removed`: edge removed, no longer a stream edge, or the meeting released (EndMeeting / teardown).
  - Subscriber disconnect does not exit pending, because the edge stays installed until MC removes it.
- Catalog rows for all six series; basic panels in `mh-media.json` (explicit `editorMode`, rates); new prefixes checked against `metric_hygiene.rs` `MEDIA_PATH_PREFIXES`.

**5. Item 8 cleanup and item 9 fixture.** I make ALL edits; there is no owner-implements. Owners collaborate (answering questions, supplying text) and review at Gate 3. Security's involvement in the `signaling.proto` and ADR-0036 edits (a GSA, comments only) is through the standing security seat, so the Owner cells name `protocol` only.
- **`signaling.proto` comments + the two ADR-0036 blockquotes:** @paired-protocol's verbatim text (A-E2). The key name stays; task 12 renames it. Reviewers: protocol + security.
- **`config.env`:** the 3 entries and their comments, the section header, and l.761-763.
- **mh-0/mh-1, kept identical:** the 3 refs, the `MEDIA_VIDEO_MIN_KEYFRAME_INTERVAL_MS` ref, and the comments at l.220-224 and l.255-266.
- **`profile.env`, comments only:** l.44-46 and l.95-110.
- **`.env.example`:** l.128-130 and the l.121-122 comment.
- **`DEVELOPMENT.md`:** l.731-733 and the MH block. The MC block's copy stays.
- **env-test 01:** the key table, the l.396-400 tuple, both relations, the comments, and dropping `profile_cm` if it becomes unused (no hollow test). It also gains an absence assertion: none of the 4 names is in either MH pod's env, and the 3 keys are absent from the live `mh-service-config`. dt-guard env-config does not catch a reintroduction. Reviewer: @paired-infrastructure, who supplies the D(1)-(3) checks I run.
- **Runbooks:** mh-deployment.md rows 719-721, 722 and 727 and the l.693 sentence; mc-deployment.md l.875, in operations' wording. Reviewer: operations.
- **Stale "refused before a byte is read" text:** config.env l.806-808, catalog l.932, caps.rs:11, transport/mod.rs:628, and the abandonment integration test.
- **`config.rs:427` doc:** MH does not read the key; it is MC-only.
- **Item 9 fixture,** per @test's design C:
  - `Fault::AbandonMidGroup`;
  - `GroupScheduler::on_target_abandoned`;
  - the deferred-slot enum `Request(counter) | SelfKey`;
  - `GroupStartCause::TargetAbandoned`;
  - StoppedByPeer never self-keys;
  - all of @test's unit tests;
  - the `ResetMidGroup` and `mod.rs` doc fixes.
  Reviewer: @test, who answers design questions.
- No boot check relates any MH key to the min interval (protocol F, Lead ruling). The superseded "pre-header refusal" boot rule is NOT implemented.

**6. Env-test `39_mh_video_keyframes.rs`** (test A/B/D/F). Header `Edge-sum (concurrent Σ P×(N+M)): N` ≤ 30. Per-entity evidence, with request counts per publisher connection (per pod):
- late joiner: KeyframeRequest for exactly that stream observed before the group start;
- never-sending source: zero requests in the fixture's log, plus `mh_media_edges_pending >= K` (absolute lower bound, not a delta). The absence window is bounded by that event, and there is a positive control afterwards: start the publisher and observe one request;
- recovery outcomes, each counted, with one request per (stream, counter) at the fixture;
- backlog-abandoned subscriber recovers via a request;
- publisher abandonment → RESET then group start at reset_group+1, with cause `TargetAbandoned`, and no wall-clock gate.

**7. Gates (integration, injected clock / paused tokio, MetricAssertion).** Every prompt gate, plus:
- test E(i)-(vi);
- client A (max) and E (never-draining writer);
- SEC-S1 both sides;
- SEC-S5c (second connect = malformed);
- framing break;
- positive controls before every absence assertion (test B).
Exact gauge/exit/histogram accounting is in-process.


### Amendments after reviewer input (Gate 1)
- **Lead ruling:** the story's "Pre-header refusal" boot rule is superseded. MH has no boot check against any keyframe key and reads none.
- **Transitions are settled by construction (code-reviewer a).** Raw `EdgeMachine` transitions are reachable only through `StreamEdge` methods. Each method does the transition and then calls `settle`. The lock order (edge → publisher record, never reverse) is documented on `PublisherKeyframes`. `classify_recovery` and the writer never take an edge lock.
- **Ingress trigger runs AFTER the machine consumes the frame (test Q1).** An edge whose incoming frame is the group start goes to Forwarding and wants nothing. Gate: zero requests in that case. Positive control: exactly one request when the next frame is mid-group.
- **Refused-group repair, both branches (security i, test Q2)**, on edges Forwarding S:
  - If the old group has already ended (`current == None`), the edge goes to Waiting now and requests.
  - If the old group's downlink is still live (mid-frame or between frames), the edge is marked `repair_at_group_end`. When that downlink's group ends, the edge goes to Waiting and requests at the refused counter. On a clean FIN this goes through `group_ended`. An abnormal end already goes through `abandon_downlink`. The old frames are delivered intact.
  - A later group start of S clears the mark.
  - Gates cover both branches.
  - The loop-bound reasoning (each repair is ≤ 1 group per max(directed interval, 100 ms floor) with a strictly newer counter; ≤ 10 opens/s/stream at the floor vs. the per-connection bucket) is written at the call site.
- **Local subscriber presence (observability BLOCKING; security informed).** A stream edge counts as pending, and may request, only while its subscriber has a local downlink channel on this handler. `StreamEdge.subscriber_present` is kept by four things:
  - The deliver path: egress Some/None.
  - Reconcile: initial value from `EdgeContext.subscribers`. `LocalSubscribers` moves into `EdgeContext`, and the session handle reads it from there.
  - `RoutingTable::subscriber_connected`, called after register.
  - `RoutingTable::subscriber_disconnected`, called after a compare-and-remove unregister that actually removed.
  A disconnect exits pending as `edge_removed`, whose catalog definition becomes: edge removed, or its subscriber's connection left this handler. That closes both the false stuck-pending alert and the per-group request loop. Gate: no local subscriber, publisher sends N groups → zero requests, and the pending gauge is unchanged. Positive control: the subscriber connects → one request.
- **Control-loop lifetime (security S5).** The recovery reader is the connection's own Step-6 lifecycle loop, so it dies with that connection. A superseded connection's loop resolves only its own sender's slots. That is the same principal and is documented.
- **`CONTROL_PARSE_BYTES` = 1 KiB.** The rationale is at the constant: RecoveryRequest ≈ 15 B, plus the ignored W3C trace fields at most 55 + 512. Test: a maximal legitimate envelope parses.
- **Env-test 39 never-sending positive control (test Q3).** After the absence window, the test starts the publisher, installs a late subscriber mid-group, and expects exactly one KeyframeRequest on that publisher connection. This is folded with the late-joiner case.
- **"Discarded on group start" gate** covers both a queued counter older than the new header and one equal to it. Equal means discard when an independently-decodable frame arrives at counter > queued. The counter strictly increases on a group start, so "equal" means no group start, and the request stays.
- `tests/media_stream_forward_integration.rs` uses of `waiting_with_source_seen` are updated (DRY).
- If the registry's register/unregister turn out structurally identical to `LocalSubscribers`', a small generic is extracted in `session/` (DRY note).

---

### Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Semantic Guard | confirmed |
| Paired: client | confirmed |
| Paired: protocol (GSA owner) | confirmed |
| Paired: infrastructure | confirmed |

Classification guard (`validate-cross-boundary-classification.sh`): first run FAIL (`owner_not_in_manifest`, proto Owner listed two specialists) → fixed → `STATUS=OK`.
Lead rulings at Gate 1: (1) story l.177 "Pre-header refusal" lead ruling is superseded by the later same-day user rulings (header read before refusal; min interval MC-only) — no MH boot check, no MH read of the interval key. (2) No owner-implements: the implementer makes every edit; paired owners supply text and review at Gate 3.
Note: INDEX.md / review-protocol were passed to teammates by path (read as their first action) rather than inlined.

---

## Pre-Work

{Any pending changes committed before starting, dependencies resolved, etc.}

{Or "None" if no pre-work was required}

---

## Implementation Summary

### MH code (mh-service, mh-test-utils)
| Item | Before | After |
|------|--------|-------|
| Per-source `GoP` state | none (edge-local `seen_at`) | `media/keyframe.rs:PublisherKeyframes`, one per publisher binding: max counter, `sent`, newest-only `queued`. ONE dedupe method `request()` shared by MH's own and remapped receiver requests. Registry is compare-and-remove on the connection id. |
| Edge machine | `seen_at`, `waiting_with_source_seen` | deleted; `awaiting_group_start()` |
| Edge runtime | abandon -> Waiting only | every transition ends in `StreamEdge::settle`: pending entry/exit (one exit + one histogram observation) and MH's request. Requires a FORWARD target, a local subscriber and a bound publisher. Refused-group repair applies to both branches (`repair_at_group_end`). |
| Stream ingress | over-limit stopped before any read; stall armed mid-first-header | the over-limit header is read in the same `JoinSet`, the counter is recorded and the source repaired, then the stream is stopped. The stall is armed only after the first header completes. The counter is recorded from Route and ServerMuted headers. |
| Control stream | post-connect bytes discarded; send half dropped | `transport/framing.rs` is the one framer, both directions, with a fixed 1 KiB parse buffer. `serve_control_message` judges and counts every message. Each publisher has a keyframe writer on the send half. A framing break keeps the connection and falls back to `connection.closed()`. |
| Telemetry | — | `keyframe_requests_sent{trigger}`, `keyframe_request_write_failures`, `recovery_requests_received{outcome}`, `edges_pending` / `edge_pending_oldest_seconds` (tick-sampled by the session actor), `edge_pending_duration_seconds` (buckets to 300 s), `edge_pending_exits{outcome}`. All zero-initialised, with catalog rows and basic panels. |
| Shim | control queues, message-level | `control_halves()` seam halves, blocking, failing and FIN control |

### Amendment-4 cleanup
- **Removed:** the three MH keys (from `config.env`, both mh-N refs, `.env.example`, `DEVELOPMENT.md`, env-test 01 and the runbook rows) and MH's `MEDIA_VIDEO_MIN_KEYFRAME_INTERVAL_MS` ref. env-test 01 now asserts that all four are absent.
- **Rewritten:** the stale "refused before a byte is read" text, `signaling.proto` comments (protocol's text, verbatim), and two ADR-0036 2026-10-21 blockquotes.
- **Fixture:** `AbandonMidGroup` + `TargetAbandoned` + the `Deferred::SelfKey` slot, with unit tests.
- **Shared:** backlog sizing moved to `fixtures/video/backlog.rs`, shared by 38 and 39. The drift guard now reads it there.
- **New env-test:** `39_mh_video_keyframes.rs`.

### Deviations from plan (noted for review)
- The framer lives in `transport/framing.rs` (generic over the seam), not `webtransport/control.rs`.
- Pending also requires the source's PUBLISHER to be bound on this handler, so `abandoned_source_left` has a real trigger (publisher disconnect). A validated policy cannot declare an undeclared candidate.

## Files Modified

```
 .env.example                                       |   8 +-
 crates/env-tests/src/fixtures/video/control.rs     |   6 +-
 crates/env-tests/src/fixtures/video/mod.rs         |  16 +-
 crates/env-tests/src/fixtures/video/publisher.rs   | 184 +++++++++-
 crates/env-tests/src/fixtures/video/schedule.rs    | 247 ++++++++++++-
 crates/env-tests/src/fixtures/video/subscriber.rs  |  59 +++
 crates/env-tests/tests/01_mh_deployment_config.rs  | 119 +++---
 crates/env-tests/tests/38_mh_video_forwarding.rs   |  68 +---
 crates/mh-service/src/config.rs                    |  18 +-
 crates/mh-service/src/main.rs                      |   8 +-
 crates/mh-service/src/media/caps.rs                |   9 +-
 crates/mh-service/src/media/edge.rs                | 277 +++++---------
 crates/mh-service/src/media/edge_runtime.rs        | 404 ++++++++++++++++++---
 crates/mh-service/src/media/mod.rs                 |   4 +-
 crates/mh-service/src/media/stream_ingress.rs      | 148 +++++++-
 crates/mh-service/src/observability/metrics.rs     | 289 ++++++++++++++-
 crates/mh-service/src/routing/mod.rs               | 202 ++++++++++-
 crates/mh-service/src/session/mod.rs               |  57 ++-
 crates/mh-service/src/transport/mod.rs             |   6 +-
 crates/mh-service/src/webtransport/connection.rs   | 244 ++++++++-----
 .../mh-service/src/webtransport/media_transport.rs |  24 ++
 crates/mh-service/tests/common/stream_rig.rs       |  59 ++-
 .../tests/media_session_binding_integration.rs     |  65 ++++
 .../tests/media_stream_abandonment_integration.rs  |  67 +++-
 .../tests/media_stream_forward_integration.rs      |  12 +-
 crates/mh-test-utils/src/stream_shim.rs            |  21 +-
 crates/mh-test-utils/src/transport_shim.rs         | 154 ++++++++
 docs/DEVELOPMENT.md                                |  17 +-
 .../adr-0033-polyglot-validation-pipeline.md       |   2 +-
 docs/decisions/adr-0036-media-flow.md              |  45 +++
 docs/observability/metrics/mh-service.md           | 102 +++++-
 docs/runbooks/devloop-validation.md                |   4 +-
 docs/runbooks/mc-deployment.md                     |   2 +-
 docs/runbooks/mh-deployment.md                     |  11 +-
 docs/specialist-knowledge/media-handler/INDEX.md   |  12 +-
 infra/grafana/dashboards/mh-media.json             | 350 ++++++++++++++++++
 infra/services/media-codec-profile/profile.env     |  25 +-
 infra/services/mh-service/config.env               |  74 +---
 infra/services/mh-service/mh-0-deployment.yaml     |  45 +--
 infra/services/mh-service/mh-1-deployment.yaml     |  45 +--
 packages/web-app/e2e/README.md                     |   4 +-
 proto/dark_tower/signaling/v1/signaling.proto      | 116 ++++--
 scripts/layer7.sh                                  |  19 +-
 scripts/layer7.test.sh                             |  30 ++
 44 files changed, 2937 insertions(+), 741 deletions(-)

New files:
crates/env-tests/src/fixtures/video/backlog.rs
crates/env-tests/tests/39_mh_video_keyframes.rs
crates/mh-service/src/media/keyframe.rs
crates/mh-service/src/transport/framing.rs
crates/mh-service/tests/media_keyframe_integration.rs
docs/devloop-outputs/2026-10-21-mh-b-on-demand-keyframes/main.md
```

### Key Changes by File
See §Implementation Summary (per-area breakdown).

---

## Devloop Verification Steps

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
**Attempt 1 — FAIL** (`env-tests-failed`, suite exit 124). Layers 1-6 OK. All env-test binaries that ran passed (incl. new 39, 78.9s); the suite hit `ENV_TEST_TIMEOUT`=600s during 40_resilience. Diff-caused: the task-9 suite was ~525s; 39 adds ~79s. Routed to implementer: trim 39's wall time + raise the suite budget with documented envelope. Browser E2E not run (shared attempt).

**Attempt 2 — PASS.** Fixes: env-test 39 phase (c) baselines/rise polls run concurrently; `DEVLOOP_ENV_TEST_TIMEOUT` default 600→900 with measured envelope, runbook §6.7 pinned by a `layer7.test.sh` drift guard, explicit rc-124 timeout line (ops F3), ADR-0033/e2e README point at the script. Reviewed by test, operations, paired-infrastructure. `layer-all.sh`: L1-L7 OK, `env-tests-passed`, `browser-e2e-passed`, `TOTAL_RESULT=OK` (1461s; Rust suite ~576s).

---

## Code Review Results

### Gate 3 — Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 1 | 1 | 0 | SEC-R1 refused-stream edge walk coalesced; GSA ownership lens OK |
| Test | RESOLVED-FIXED | 10 | 10 | 0 | T1/T10 env-test-39 races; T2 slot scoping gates |
| Observability | RESOLVED-FIXED | 3 | 3 | 0 | doc-only (blind spot, Recorded-in, "accepted for forwarding") |
| Code Quality | RESOLVED-FIXED | 6 | 6 | 0 | Undeliverable internal outcome → `forwarded` per Lead ruling |
| DRY | RESOLVED-FIXED | 2 | 2 | 0 | shared `remove_if_held`; subscriber drain/mid_group_counter hoisted |
| Operations | RESOLVED-FIXED | 3 | 3 | 0 | DEVELOPMENT.md l.494; env-test 01 assert text; F3 explicit rc-124 timeout line in layer7.sh |
| Semantic Guard | RESOLVED-FIXED | 1 | 1 | 0 | native SAFE; error-context on ControlStep::Failed |
| Paired: client | CLEAR | 0 | 0 | 0 | |
| Paired: protocol (GSA owner) | RESOLVED-FIXED | 1 | 1 | 0 | P-1 RecoveryRequest rules comment; proto diff comment-only |
| Paired: infrastructure | RESOLVED-FIXED | 3 | 3 | 0 | env-test 01 assert continuations; ADR-0033 + e2e README budget restatements |

Lead ruling at Gate 3: the recovery-outcome label set is fixed by the task prompt; a request accepted after the publisher's control stream failed (or replaced in the newest-only queue) counts `forwarded`, documented as "accepted for forwarding" with delivery on `mh_media_keyframe_requests_sent_total{trigger="receiver_recovery"}`. No new label value.

### Security Specialist
**Verdict**: RESOLVED-FIXED — **Findings**: 1 found, 1 fixed, 0 deferred
- SEC-R1: each over-limit refused stream walked all subscriber edges of its source (unbounded CPU per ~30-byte stream). Fixed: `PublisherKeyframes::record_refused()` walks only on the first refusal since the last admitted group; later refusals O(1). Gates `refused_group_walks_are_coalesced_to_one_per_admitted_group`, `coalesced_refusals_request_nothing_when_nothing_waits`.

### Test Specialist
**Verdict**: RESOLVED-FIXED — **Findings**: 10 found, 10 fixed, 0 deferred
- T1 stale counter in env-test 39 phase (c) → counter from publisher log; T2 cross-subscriber slot scoping + unmapped arms untested → 2 gates; T3 DROP_AT_INGRESS pending exit; T4/T8 vacuous absence checks; T5 malformed-header path precondition; T6 wall-clock window → counter event; T7 cause/order linkage in phase (d); T9 doc wrap; T10 log-publication race in phase (d).

### Observability Specialist
**Verdict**: RESOLVED-FIXED — **Findings**: 3 found, 3 fixed, 0 deferred
- Pending-gauge blind spot (publisher not bound on this handler) documented with pointer to client unfulfilled-slot series; "Recorded in" lines; `forwarded` = accepted for forwarding.

### Code Quality Reviewer
**Verdict**: RESOLVED-FIXED — **Findings**: 6 found, 6 fixed, 0 deferred
- Undeliverable internal outcome; dead `discard_queued` removed; unexplained `.rev()` removed; `#[allow]` removed in env-test 39 (ADR-0002); imports hoisted; duplicate publisher lookup per waiting frame removed.

### DRY Reviewer
**Verdict**: RESOLVED-FIXED

**True duplication findings** (entered fix-or-defer flow):
- F1 env-test 38/39 `drain`/`until_mid_group_frame` copies → `VideoSubscriber::drain` / `mid_group_counter` in the shared fixture. Fixed.
- F2 `PublisherKeyframesRegistry` copied `LocalSubscribers` compare-and-remove → shared `session::remove_if_held`. Fixed.

**Extraction opportunities** (appended to `docs/TODO.md`): None

### Operations Reviewer
**Verdict**: RESOLVED-FIXED — **Findings**: 2 found, 2 fixed, 0 deferred
- DEVELOPMENT.md l.494 STATUS line; env-test 01 assert messages garbled by collapsed continuations (3 sites).

### Semantic Guard Reviewer
**Verdict**: RESOLVED-FIXED
**Native verdict**: SAFE
**Findings**: 1 found, 1 fixed, 0 deferred
- [error-context]: crates/mh-service/src/webtransport/connection.rs:592 - `ControlStep::Failed(_)` dropped the read error; now logged with `error = %error`.

### Paired Specialists
- client: CLEAR (0 findings).
- protocol (GSA owner, Ownership Lens): RESOLVED-FIXED — P-1 two RecoveryRequest MH-RULES arms (no counter yet → `counter_ahead`; oversize body → `malformed`) added as comments. Proto diff verified comment-only.
- infrastructure: RESOLVED-FIXED — env-test 01 assert continuations.

---

## Accepted Deferrals

- (none surfaced in this devloop)

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `{start_commit}`
2. Review all changes: `git diff {start_commit}..HEAD`
3. Soft reset (preserves changes): `git reset --soft {start_commit}`
4. Hard reset (clean revert): `git reset --hard {start_commit}`
5. For schema changes: rollback requires a forward migration — `git reset` alone is insufficient if migrations were applied
6. For infrastructure changes: may require `skaffold delete` or `kubectl delete -f` if manifests were applied
7. **Safe-revert unit** (answer explicitly, even if "the whole commit"): can any part of this diff be reverted or cherry-picked on its own, or does a partial revert reconstruct a state worse than either endpoint (e.g. a security fix split from the change that made it necessary, or a client/server/alert-rule set that must move together)? If partial reverts are unsafe, name the unit and the safe direction. **Answer**: the whole commit. Re-render it from the env root; never `kubectl rollout undo` a single Deployment. The config side is independently safe:
   - No MH image ever read the three removed keys, so an old image with new config, or a new image with old config, both boot.
   - `mh-service-config` is a hash-suffixed generator, so old ReplicaSets keep their content-addressed ConfigMap, and no two-step `optional: true` retirement is needed.
   - The profile key stays referenced by mc-0/mc-1, so no orphan is created.
   - The config hash change rolls mh-0/mh-1, which sheds every media session on each pod. That is normal for an MH image change.
   - The three keys are NOT added to the Retired-keys list. (This converts silence into a visible unanswered slot; it cannot distinguish a checked answer from a reflexive one.)

---

## Issues Encountered & Resolutions

### Issue 1: Gate-2 Layer-7 suite timeout
**Problem**: The Rust env-test suite (~605s with new env-test 39) exceeded `ENV_TEST_TIMEOUT`=600s; reported only as "exited 124".
**Resolution**: Budget raised to 900s with measured envelope + drift guard; explicit timeout diagnostic line; env-test 39 phase (c) parallelised. Attempt 2 passed.

### Issue 2: Story-file ruling tension (pre-header refusal)
**Problem**: Story l.177 lead ruling kept an MH boot check on `MEDIA_VIDEO_MIN_KEYFRAME_INTERVAL_MS`; later same-day user rulings removed MH's use of the key.
**Resolution**: Lead ruled the later user rulings supersede; no MH boot rule, no MH read of the key.

---

## Lessons Learned

1. The env-test suite budget is a shared resource; a new env-test can red Layer 7 with every test passing. The explicit rc-124 line now makes that visible.
2. Repair of MH-decided refusals needs its own cost bound (SEC-R1); approving a mechanism is not approving its frequency.

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
