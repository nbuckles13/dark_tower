# Devloop Output: MH-A2 — stream-per-group cut-through video forwarding

**Date**: 2026-10-21
**Task**: MH-A2 (story 2026-10-07-see-each-other task #9): stream-per-group cut-through video forwarding, edge state machine, per-destination abandonment with reset, stream limits, memory startup check
**Specialist**: media-handler
**Mode**: Agent Teams (v2) — full, Gate-1 present
**Branch**: `feature/user-see-each-other`
**Duration**: ~6h (2026-10-21)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `b070e8709bc63817858fcf36bf9ccfc152f2e5a7` |
| Branch | `feature/user-see-each-other` |
| Lead Model | `claude-opus-5-5[1m]` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` |
| Implementing Specialist | `media-handler` |
| Tier | `full` |
| Iteration | `2` (Gate 2 L7 attempt 1 failed: env-test 38 hang) |
| Security | `security (paired) — RESOLVED-FIXED` |
| Test | `test (paired) — RESOLVED-FIXED` |
| Observability | `observability — RESOLVED-FIXED` |
| Code Quality | `code-reviewer — RESOLVED-FIXED` |
| DRY | `dry-reviewer — RESOLVED-FIXED` |
| Operations | `operations — RESOLVED-FIXED` |
| Semantic Guard | `semantic-guard — RESOLVED-FIXED` |
| Paired Protocol | `paired-protocol — RESOLVED-FIXED` |

---

## Task Overview

### Objective
Task description: exact contents of `/home/dev/.cache/devloop/story-runs/story-runner/2026-10-07-see-each-other/task-9.prompt` (run-story task #9). Pairings per the task: protocol (media-protocol parser consumption), security (reset-never-finish, stream-path limits), test (transport-seam shim, gates, env-tests).

### Scope
- **Service(s)**: mh-service (+ mh-test-utils, env-tests, dt-guard env_config, infra config/dashboard, docs)
- **Schema**: No
- **Cross-cutting**: Yes

### Debate Decision
NOT NEEDED - governed by ADR-0036 (as amended by task 1) and the story's Design §Wire freeze.

---

## Cross-Boundary Classification

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `crates/mh-service/src/transport/mod.rs` | Mine | — |
| `crates/mh-service/src/webtransport/media_transport.rs` | Mine | — |
| `crates/mh-service/src/webtransport/server.rs` | Mine | — |
| `crates/mh-service/src/webtransport/connection.rs` | Mine | — |
| `crates/mh-service/src/media/mod.rs` | Mine | — |
| `crates/mh-service/src/media/caps.rs` | Mine | — |
| `crates/mh-service/src/media/edge.rs` (new) | Mine | — |
| `crates/mh-service/src/media/downlink.rs` (new) | Mine | — |
| `crates/mh-service/src/media/stream_ingress.rs` (new) | Mine | — |
| `crates/mh-service/src/media/forward.rs` | Mine | — |
| `crates/mh-service/src/media/forwarder.rs` | Mine | — |
| `crates/mh-service/src/media/ingress.rs` | Mine | — |
| `crates/mh-service/src/routing/mod.rs` | Mine | — |
| `crates/mh-service/src/routing/sources.rs` | Mine | — |
| `crates/mh-service/src/media/edge_runtime.rs` (new) | Mine | — |
| `crates/mh-service/src/routing/tests.rs` | Mine | — |
| `crates/mh-service/src/session/mod.rs` | Mine | — |
| `crates/mh-service/src/grpc/mh_service.rs` (test-only constructor rename) | Mine | — |
| `crates/mh-service/src/config.rs` | Mine | — |
| `crates/mh-service/src/main.rs` | Mine | — |
| `crates/mh-service/Cargo.toml` (serde_json to [dependencies] for the JSON refusal line) | Mine | — |
| `crates/mh-service/src/observability/metrics.rs` | Mine | — |
| `crates/mh-service/tests/**` | Mine | — |
| `crates/mh-test-utils/src/**` | Mine | — |
| `docs/specialist-knowledge/media-handler/INDEX.md` | Mine | — |
| `infra/services/mh-service/config.env` (comments only: "Enforced by" lines) | Mine | — |
| `crates/env-tests/tests/38_mh_video_forwarding.rs` (new) | Not mine, Minor-judgment | test |
| `crates/env-tests/src/fixtures/video/subscriber.rs` (held-reader knob for the slow-subscriber row) | Not mine, Minor-judgment | test |
| `crates/env-tests/tests/37_mh_stream_attribution.rs` (switch to hoisted helpers) | Not mine, Minor-judgment | test |
| `crates/env-tests/src/fixtures/metrics.rs` (hoisted PromQL helpers; the one home of the baseline/pin failure bounds) | Not mine, Minor-judgment | test |
| `crates/env-tests/src/fixtures/injection.rs` (hoisted baseline / rise / install-all helpers, declared-reading edge/source adapters, shared failure bounds) | Not mine, Minor-judgment | test |
| `crates/env-tests/src/fixtures/participant.rs` (shared `VIEW_MARGIN`) | Not mine, Minor-judgment | test |
| `crates/env-tests/tests/26_mh_quic.rs` (use the shared `VIEW_MARGIN` and metric bounds) | Not mine, Mechanical | — |
| `crates/env-tests/tests/27_mc_slot_placement.rs` (use the shared `VIEW_MARGIN`) | Not mine, Mechanical | — |
| `crates/env-tests/tests/35_mc_server_mute_teardown.rs` (use the shared `VIEW_MARGIN`) | Not mine, Mechanical | — |
| `crates/env-tests/tests/36_mc_media_kind_labels.rs` (use the shared `VIEW_MARGIN`) | Not mine, Mechanical | — |
| `crates/env-tests/tests/31_gc_telemetry.rs` (use the shared `BASELINE_SETTLE_BOUND`) | Not mine, Mechanical | — |
| `docs/observability/metrics/mh-service.md` | Not mine, Minor-judgment | observability |
| `infra/grafana/dashboards/mh-media.json` (basic panels only) | Not mine, Minor-judgment | observability |
| `infra/grafana/kustomization.yaml` (split `mh-media.json` into its own ConfigMap: the guard's annotation-size headroom) | Not mine, Minor-judgment | infrastructure |
| `infra/grafana/deployment.yaml` (mount the new dashboard ConfigMap) | Not mine, Minor-judgment | infrastructure |
| `docs/observability/dashboards.md` (new ConfigMap + panel rows) | Not mine, Minor-judgment | observability |
| `docs/specialist-knowledge/security/INDEX.md` (pointers) | Not mine, Mechanical | — |
| `docs/decisions/adr-0036-media-flow.md` (one anchor paragraph: downlink reset / STOP_SENDING code semantics, requested by protocol) | Not mine, Minor-judgment | protocol |
| `infra/services/mh-service/mh-0-deployment.yaml` (comments only: MH-A2 keys now read) | Not mine, Minor-judgment | infrastructure |
| `infra/services/mh-service/mh-1-deployment.yaml` (comments only: MH-A2 keys now read) | Not mine, Minor-judgment | infrastructure |
| `docs/runbooks/mh-deployment.md` (Configuration Reference row for MH_POD_MEMORY_LIMIT_BYTES: point at the boot check) | Not mine, Minor-judgment | operations |

---

## Planning

### Mechanism restatement (and where the task framing is narrower than the problem)

Instance: "forward video groups cut-through on uni streams". Mechanism: **every egress edge is a per-(subscriber connection, egress_stream_id) destination with its own runtime state — gate, hop counter, live downlink streams, byte-capped backlog — that survives re-push, is the ONLY place a downlink stream is opened, finished or reset, and is abandoned per destination without ever back-pressuring its source.** Two places the task's framing is narrower than that mechanism, both pulled in here rather than shipped partial:

1. **Hop sequence home (both transports).** Today `HopSequence` lives in the PUBLISHER's `ConnectionForwarder`, keyed by `egress_stream_id`. ADR §2 says per (subscriber connection, media stream). MC keeps `egress_stream_id` stable across a refill (`mc-service/src/media_routing/assignment.rs::egress_stream_id`), so an AUDIO slot refilled A→C today restarts its hop from C's counter — the same defect protocol flags for video switches. The hop moves to the edge runtime cell for BOTH paths (one home; `HopSequence` reused, made atomic so the datagram path takes it lock-free). The per-publisher `StreamForwarder`/`take_hop_for` is deleted.
2. **quinn `send_window` is undeclared** (quinn default 10 MB/connection). With stream egress it sits UNDER the backlog cap: MH-side backlog would never trip and the memory check would certify nothing. Declared explicitly as a constant (`CONNECTION_SEND_WINDOW_BYTES = 512 KiB`) in `build_transport_config`, asserted in the rendered-config test, and included as a term in the memory check. Not a new knob.

### 1. Transport seam (`transport/mod.rs`, `webtransport/media_transport.rs`, shim)
- `MediaTransport` gains `type RecvStream: MediaRecvStream` and `accept_uni() -> impl Future<Output = Result<Self::RecvStream, TransportError>> + Send`.
- New `MediaRecvStream`: `read_chunk(max) -> Result<Option<Bytes>, StreamReadError>` (`Ok(None)` = FIN; `StreamReadError::{Reset(code), ConnectionClosed}`), and `stop(code)`. Real impl: `quic_stream_mut().read_chunk(max, true)` — `Bytes` slices of quinn's buffers, no copy; error mapping keeps the source (`#[source]`/message, never `map_err(|_| ..)`).
- `MediaSendStream` gains `reset(code)` and `set_priority(i32)` (both sync, as in wtransport).
- **Drop contract (structural, security #1):** dropping a send stream that was neither finished nor reset RESETS it. `WtSendStream` gets a `Drop` that calls `reset(GROUP_ABANDONED)` unless `finish()`/`reset()` already ran (quinn's own Drop would FIN). The shim models the CONTRACT (drop ⇒ recorded `Reset`), and a real-QUIC test in `tests/transport_real_impl.rs` proves the production Drop resets (peer sees RESET_STREAM, not FIN) so the shim is not certifying a property production lacks.
- Codes, one home in `transport/mod.rs`: `GROUP_ABANDONED` (every MH downlink reset — ONE code, all reasons; the reason is MH-local telemetry, never wire-visible) and `INGRESS_REFUSED` (every MH STOP_SENDING on a publisher stream). Values documented there; T-A's fixture already treats any MH STOP_SENDING as an observable outcome.
- Priority: `priority_group` → `set_priority(-(i32::try_from(group).unwrap_or(i32::MAX)))` (quinn: higher = more urgent; MC: lower = higher). Unit test: group 1 outranks group 2.
- Shim (`mh-test-utils/src/transport_shim.rs`, with test): `push_inbound_uni()` returns an `InboundUniHandle` to feed caller-chosen `Bytes` chunks (any split, header included), `finish()`, `reset(code)`, or just stop feeding (stall); records whether MH `stop()`ped it and with what code. Send streams record a terminal state `Open | Finished | Reset(code)` (drop ⇒ `Reset(GROUP_ABANDONED)` per the contract) plus bytes written; a per-connection credit knob (`set_stream_credit(bytes)`/`grant`) makes `write_all` pend — the slow subscriber. Counters: inbound streams offered/accepted/stopped, egress streams opened, current/max concurrent. Control envelope both ways (`push_control_inbound` / `control_outbound`) as task-10 plumbing only — no MH consumer in this task. All time on tokio paused time; no wall-clock sleeps.

### 2. Stream ingress (`media/stream_ingress.rs`, new; under the `media/` deny)
- **Accept loop** (one task per publisher connection): `accept_uni` → `caps::StreamOpenLimiter::admit(now)` (token bucket, capacity = rate = `MH_STREAM_OPENS_PER_CONNECTION_PER_SEC`, refill per second, loop-owned state, `now` passed in from `tokio::time::Instant`) BEFORE any byte is read. Refused → `stop(INGRESS_REFUSED)`, `dropped{stream_rate_limited}` +1 (one per refused STREAM; catalog says "lower bound on frames lost"). Admitted → spawned into a loop-owned `JoinSet` (one task per group), so a stalled stream never blocks the next accept.
- **Group task**: one `HeaderReader` per stream. `select!` over cancel, `sleep_until(stall_deadline)` (armed only while mid-frame — mid-HEADER via `is_mid_header()` OR tail `remaining > 0` — and re-armed on each chunk of progress), and `read_chunk`. Per chunk: feed reader; on `Complete{header, consumed}` handle the header, then stream `chunk.slice(..)` sub-ranges of the tail (tracked from `header.tail_len()`), and at `remaining == 0` `reader.reset()` and keep feeding the SAME chunk (multi-frame chunks; tested). Nothing is sized from `payload_length`; per stream MH holds the inline header and the chunk in flight.
- **Per header**: `snapshot.attribute(meeting, bound sender, header.stream_id(), StreamPerGroup)` — task 8's lookup, no second rule — re-run per frame (never cached):
  - `Route{paused, edges}` → `forwarded{ingress}` +1 at header completion (once per frame, never per chunk/edge); `paused_source_frames` overlay; `group_starts{ingress}` +1 on the stream's first attributed frame; then fan-out (§3).
  - `ServerMuted` → `server_muted` per frame, tail discarded, stream KEPT reading (still through `HeaderReader`, still re-attributed per frame, still under the stall timer), so a lifted mute resumes on that stream at the next group start.
  - `NoPolicy` → per frame, discard, keep reading (policy may arrive).
  - `Unmatched` / `TransportMismatch` → +1 (one per stream), `stop(INGRESS_REFUSED)`, end.
  - Codec reject (`Err` from the reader, sticky) → `codec_dropped(reason)` +1, `stop(INGRESS_REFUSED)`, end. No new reject tokens.
  - **Label change mid-stream** (a later frame's label ≠ the stream's first attributed label): the stream is one group of one source; treated as `unmatched_stream` (+1), `stop`, downstream reset `publisher_reset`. No new token. (security #5e — please confirm.)
- Group start = `header.flags().independently_decodable`, never stream position. A stream whose first frame lacks it forwards nothing to Waiting edges (publisher fault, gated like any non-start).
- Concurrent ingress groups gauge: RAII guard (`Gauge::increment/decrement`, process-wide), decremented on every exit incl. panic/abort.

**Exit-path table (one terminal function maps state → (downstream action, counter); every exit goes through it, incl. the group task's `Drop` guard):**

| Publisher stream end | Ingress counter | Downstream streams this group fed | `group_resets` reason |
|---|---|---|---|
| FIN at a frame boundary | — (normal group end) | FINISH (frame-boundary checked) | — |
| FIN mid-header | `partial_frame_discard` +1 (direction re-classified **ingress**: the frame never reached `forwarded{ingress}`) | RESET | `publisher_reset` |
| FIN mid-tail | — (frame already counted `forwarded{ingress}`) | RESET | `publisher_reset` (doc: "group ended mid-frame by the publisher — RESET_STREAM or a truncating FIN") |
| RESET_STREAM (any position) | `partial_frame_discard` iff mid-header | RESET | `publisher_reset` |
| Stall past `MH_STREAM_MID_FRAME_STALL_MS` | `partial_frame_discard` iff mid-header | RESET + `stop` the ingress stream | `publisher_stall` |
| Codec reject / unmatched / mismatch / label change | that token, +1 | RESET | `publisher_reset` |
| Rate-limited at accept | `stream_rate_limited` +1 | none (nothing read) | — |
| Publisher connection closed / MH shutdown (task aborted; Drop guard) | `partial_frame_discard` iff mid-header | RESET | `source_ended` |

Ingress identity: every frame whose first byte MH read ends as exactly one of `forwarded{ingress}` (at header completion), a codec token, an attribution token, `no_policy`, or `partial_frame_discard`.

### 3. Cut-through egress (R-21)
- Delivery set for a frame = its source stream's edges from the routing snapshot, where (new) a stream edge stays indexed under the source it is still FORWARDING during a switch (§4), so A keeps reaching the edge until B's group start. Datagram edges are unaffected.
- Per edge (under that edge's `std::Mutex`, never held across `.await`): the edge machine decides `Skip | Continue | StartGroup`. Start/Continue: take the edge's hop, copy `reader.header_bytes()` into a group-task arena (amortised zero alloc), `codec::rewrite_relay_region_header(copy, slot, hop)` (failure ⇒ `relay_rewrite_failed`), push the frozen header to the edge's current downlink. `forwarded{egress}` +1 per edge at that push (same instant the hop is consumed: "what the transmitter sent").
- Tail chunks: the group task holds `(edge, downlink)` leases for the frame in flight and pushes each chunk `Bytes` clone (refcount, zero copy) to each — **never awaits** a subscriber (semantic-guard #1).
- **Downlink** (`media/downlink.rs`): a per-group, per-destination byte queue + a writer. Push is non-blocking: it charges a byte budget SHARED by all of that edge's live downlinks (`MH_EGRESS_STREAM_BACKLOG_BYTES` per destination; counts slice length). Over budget ⇒ the edge is abandoned (`subscriber_backlog`). Bytes are released when `write_all` accepts them.
- **Who writes:** a per-SUBSCRIBER-connection stream-egress task (generic over the transport, spawned in `start_media_session`) receives new downlinks over an unbounded channel registered in `LocalSubscribers` beside the datagram queue, and runs one writer per downlink in a connection-owned `JoinSet`. The writer `select!`s `open_uni` / `write_all` against the downlink's abandon signal, so a subscriber withholding MAX_STREAMS credit or stream credit never blocks fan-out, and an abandon resets promptly. `group_starts{egress}` +1 when `open_uni` returns. `open_uni` failing (connection gone) or a peer STOP_SENDING ⇒ downlink closed, edge back to Waiting at its next frame, counted `connection_closed` (catalog widened to "subscriber connection or stream closed") — never silent.
- One downlink carries one slot; every new group or source change opens a new one. `set_priority` from the edge's `priority_group` before the first write.
- Per edge at most TWO live downlinks (current + the predecessor finishing its in-flight frame). A group start finding an older predecessor still live resets it (`subscriber_backlog`). This is the application bound on concurrent unfinished egress groups: ≤ 2 × the subscriber's stream edges, below the peer's declared QUIC ceiling.
- Finish only at a frame boundary: the downlink tracks `in_frame`; a finish requested mid-frame is deferred to the frame end (detach), and if the frame never completes it becomes a RESET. Every abandonment resets; Drop resets.

### 4. Edge state machine (`media/edge.rs`, pure, `now` passed in)
States: `Waiting{target, seen_at}`, `Forwarding{source, group}`, `SwitchPending{current, current_group, target, seen_at}`, `Paused{source}`. Inputs: `assign(target: Option<Source>, paused)` (reconcile), `on_frame(source, group_token, independently_decodable, now)`, `on_abandoned()`, `on_source_ended(source)`. `group_token` is the ingress group (QUIC stream) id, so a stale earlier group of the same source can never `Continue` onto a newer group's downlink.
- Waiting: target's group start ⇒ StartGroup → Forwarding. Anything else Skip (records `seen_at`).
- Forwarding(A): same group ⇒ Continue; A's new group start ⇒ StartGroup (old downlink finishes at its frame boundary).
- assign B≠A (FORWARD) ⇒ SwitchPending{A→B}: A keeps forwarding; B's group start ⇒ StartGroup on a new downlink, A's downlink finishes at its frame boundary. Revert to A ⇒ Forwarding(A) (abandoned). B ended (`on_source_ended(B)`: B's publisher connection closed, or B undeclared at reconcile) ⇒ Forwarding(A), A kept. A second switch to C supersedes.
- assign to a PAUSED source ⇒ Paused immediately ("forward nothing"): cancels the wait, detaches the previous source's downlink (finish at its frame boundary). A paused source's frames still forward from its next group start (resume beat MC). Pausing the source being FORWARDED changes nothing (its frames, if any, keep flowing).
- `on_abandoned` ⇒ Waiting on the assigned target. No give-up timeout anywhere (R-4).
- Audio-shaped sources (every frame independently decodable) never wait — one parameterised test covers both.
- `waiting_with_source_seen(now) -> Option<seen_at>` exposed for task 10.
- Runtime cell (`media/edge_runtime.rs`, per the Gate-1 layering delta): `EdgeRuntime { hop: HopSequence (atomic), stream: Mutex<StreamEdge{machine, live downlinks, backlog budget}> }`, keyed by `egress_stream_id` and carried from the previous `MeetingRoutes` into the new one at `install` (identical or unrelated re-push ⇒ the SAME `Arc`, state and hop untouched). Reconcile runs in the session actor's apply, BEFORE the snapshot swap, never awaits a task: removed edge ⇒ reset its downlinks `edge_removed`; source undeclared ⇒ `source_ended`; candidate/disposition change ⇒ `assign`. `RoutingTable::remove` (EndMeeting) ⇒ `edge_removed` for all. Publisher connection teardown (sibling, `connection.rs`) calls `source_ended(meeting, sender)`.

### 5. Counting units (observability #1–#5)
- `mh_media_group_resets_total{reason=subscriber_backlog|publisher_reset|publisher_stall|source_ended|edge_removed}`: one per DOWNSTREAM stream reset (per destination), counted in exactly one place (the downlink's idempotent `abandon`), even if the transport `reset()` errors. No direction label. Enum with `ALL`, resolved at setup (zero at boot).
- `mh_media_group_starts_total{direction}`: ingress = per accepted+attributed publisher stream (refused/failed streams never count); egress = per downlink opened.
- Abandoned groups' frames are NOT frame-counted again (group counter is the signal); frames withheld from Waiting/SwitchPending edges are uncounted by design — catalog states both, and that video "mean fan-out" reads below subscriber count while healthy.
- Gauges, each set from the value enforcement reads: `mh_media_stream_ingress_groups` (live, inc/dec), `mh_media_max_concurrent_uni_streams` (the `QuicTransportParams` field quinn gets), `mh_media_egress_stream_backlog_bytes`, `mh_media_stream_mid_frame_stall_seconds`, `mh_media_stream_sources_declared` (STREAM_PER_GROUP source streams across installed policies, published by the session actor beside `mh_media_egress_edges`). Names final at review with observability.
- Latency: stream path observes `processing` (header complete → last rewritten header pushed), `transmit_buffer` (push → header `write_all` returned), `total` (header complete → written), on the same random sampler; never `receive_buffer`; payload chunks never timed. Catalog phase table updated.
- No identity/kind/gop/size labels; no macro under `media/`; `caps.rs` and both tokens' "UNREACHABLE" text rewritten to the live enforcement lines (metrics.rs + catalog).

### 6. Memory startup check (`config.rs`; logged in `main.rs`)
`required = backlog × stream_ceiling + (CONNECTION_RECEIVE_WINDOW_BYTES + CONNECTION_SEND_WINDOW_BYTES) × MH_MAX_CONNECTIONS ≤ limit × 1/2` in checked u64 (overflow ⇒ refuse). Backlog is per DESTINATION (shared across that edge's ≤2 downlinks), so the multiplier is exactly the derived stream ceiling, enforced at `session/mod.rs` admission (`projected_total_edges > stream_ceiling`). `MEMORY_CHECK_FRACTION` (1/2) is a named constant documenting the slack it reserves (process, allocator, quinn per-connection state, datagram buffers, `Bytes` parent-allocation retention: a held quinn slice can pin its packet buffer, so accounting by slice length under-counts by at most that — accepted, inside the slack). Refusal: `ConfigError::PodMemoryCheck{…}` naming every term, its key, the limit, fraction, requirement and the levers; `main` logs it at ERROR (target `mh.config.memory`) and exits non-zero; success logs the same figures + headroom at INFO.
The memory prose in `config.env` (MH_MAX_CONNECTIONS block), the `CONNECTION_RECEIVE_WINDOW_BYTES` docstring and the runbook row all point at this check as the single statement; with the send term, MH_MAX_CONNECTIONS has ~1.3x headroom at 1 GiB, which is now the binding constraint on raising it. `CONNECTION_SEND_WINDOW_BYTES` documents its throughput consequence (512 KiB/RTT ≈ 21 Mbps at 200 ms vs. worst-case per-subscriber downlink of video slots × MEDIA_VIDEO_MAX_BITRATE_BPS + audio).
Shipped figures (1 GiB limit, half = 536.9 MB): base ceiling ≈12 → 3.1 + 131.1 + 262.1 = **396.4 MB (37%)**; Kind (budget 82.64 Mbps, ceiling ≈99) → **419.3 MB (39%)**. A unit test reads the shipped `config.env` + Kind patch + deployment limit and asserts both pass.

### 7. Config (R-23)
`MH_STREAM_OPENS_PER_CONNECTION_PER_SEC` 1..=100, `MH_EGRESS_STREAM_BACKLOG_BYTES` 16384..=4194304, `MH_STREAM_MID_FRAME_STALL_MS` 100..=10000: **REQUIRED reads** through the existing `parse_required`/bounds helpers (story-2 R-23 precedent: a required read is what `dt-guard env-config` check 1 sees; the 10 / 262144 / 1000 defaults are the shipped ConfigMap values INF-A already landed; the startup log reports every value and its provenance). Out-of-range ⇒ startup refusal naming the key, never a clamp. `MH_POD_MEMORY_LIMIT_BYTES` required, no default, `> 0`, absent/empty/unparseable/zero ⇒ refusal; no cgroup fallback. No key renamed or retired; `config.env` "Enforced by: once MH-A2 lands" lines become live statements. Image rollback stays safe (older images ignore these keys).

### 8. Tests (gate → test)
Unit: `edge.rs` (every transition incl. switch keep/abandon/supersede, paused, audio-never-waits parameterised, idempotent re-assign); `downlink.rs` (budget, finish-only-at-boundary, abandon idempotent+counted once); `caps.rs` limiter on paused time; config bounds + memory table (just under / at / over the fraction, overflow, absent/zero limit, shipped values pass).
Integration (shim, `start_paused`, runtime without an I/O driver): `tests/media_stream_forward_integration.rs` — frames in = frames out over EVERY split point of a multi-frame stream incl. header splits (positive control > 0), exactly one `forwarded{ingress}` per frame, zero-copy (payload `as_ptr` identity across N subscribers upstream of the seam), first forwarded frame is a group start, keep-A-until-B, switch abandoned when B leaves, switch-to-PAUSED immediate, hop continuity across ≥2 group streams and a source switch with an unforwarded frame consuming no number, re-push (identical + unrelated) preserves state and next hop, priority mapping, the 5 stream attribution rows (mixed audio+video, label 0 over stream ⇒ `transport_mismatch`, undeclared ⇒ `unmatched_stream`, DROP_AT_INGRESS ⇒ `server_muted` while audio flows, PAUSED forwards + overlay). `tests/media_stream_abandonment_integration.rs` — slow subscriber reset at the cap while fast subscriber / publisher / audio unaffected (`assert_unobserved` on the other reasons), concurrent unfinished groups ≤ bound < declared ceiling, stall at bound−1 ms none / at bound reset, publisher reset propagates, source_ended, edge_removed, connection/meeting teardown and task cancel: every path RESET and zero FINs on a mid-frame stream, normal FIN ⇒ Finished, `stream_rate_limited` and `partial_frame_discard` reachable. `transport_real_impl.rs`: Drop ⇒ RESET_STREAM on a real QUIC pair. `media_metrics_integration.rs` walker already covers new `media/` files (asserted non-vacuous). MetricAssertion deltas throughout.
Env-test `crates/env-tests/tests/38_mh_video_forwarding.rs` (policy via `fixtures/mh_grpc.rs` as MC, media via `fixtures/video`; header records the concurrent edge sum, planned ≤ 30): mixed audio+video each to own slots; late subscriber receives nothing until the next `Periodic` group start and its first frame is independently decodable; server video mute drops at ingress at every receiver while audio forwards; `transport_mismatch` for audio over a uni stream; PAUSED video forwards; a held-reader subscriber is reset (`subscriber_backlog` delta) and resumes at a later group while a second subscriber sees every group. Per-entity deltas, `eventual.rs` convergence, no wall-clock gate. Story-2 audio env-tests untouched.

### Gate-1 deltas (reviewer feedback, folded in)
- **Errors (semantic-guard A):** `TransportError::ConnectionClosed(TransportCause)` / `StreamReadError::{Reset(code), ConnectionClosed(TransportCause)}`: `TransportCause` wraps `Arc<dyn Error + Send + Sync>` exposed as `#[source]`; `PartialEq` is variant-only. Error paths only, never per frame.
- **Counting sites (semantic-guard B, C):** `Downlink::abandon(reason)` is the ONLY counting site (idempotent). A detached predecessor whose in-flight frame never completes is abandoned with the upstream cause by the group task's terminal function. A `Drop`-time reset (writer aborted because the subscriber connection is gone) is deliberately UNCOUNTED, stated in the catalog row. The new-downlink channel is unbounded but bounded in practice by 2 × the subscriber's stream edges (comment at the channel); reconcile only calls sync `assign`/`abandon`/notify.
- **`publisher_reset` catalog definition (observability A):** defined by cause side, i.e. "the publisher's group ended mid-group for a publisher-side reason: RESET_STREAM, a truncating FIN, or MH refusing the rest of the group for a sender fault (that fault counted on `mh_media_frames_dropped_total` by its own token)". A refusal on a stream's FIRST frame has no downstream yet, so no group reset fires.
- **`connection_closed` (observability B):** counted on the NEXT frame for that edge that discovers the dead downlink (that frame is not pushed and not `forwarded{egress}`). The edge then goes to Waiting. The catalog defines `forwarded{egress}` on the stream path as "rewritten header pushed to the edge's downlink (MH-side)".
- **MH-initiated cancellation (observability C):** shutdown / EndMeeting abort does NOT count `partial_frame_discard`. That counter covers only publisher-caused ends (FIN, RESET_STREAM, stall, publisher connection close). The downstream `source_ended` reset is unchanged.
- **Gauge names:** `mh_media_uni_streams_per_connection_limit` (per connection; not the limit of the process-wide groups gauge, stated in both rows), `mh_media_egress_stream_backlog_limit_bytes` (per destination), `mh_media_stream_mid_frame_stall_seconds`, `mh_media_stream_ingress_groups`, `mh_media_stream_sources_declared`. The latency phase table warns that stream and datagram populations share series (no `receive_buffer` on streams; `transmit_buffer` includes stream-credit waits), so the phases must not be summed and the two paths' totals must not be compared.
- **Reset/stop codes (protocol F):** ONE ADR-0036 anchor beside §2's hop-local relay paragraph: MH abandons a downlink group with RESET_STREAM (never FIN), using a single reasonless code; receivers treat any reset as "group abandoned" and MUST NOT branch on the value; MH STOP_SENDINGs a refused publisher stream with a single code. The constants' doc points there and records that wtransport 0.7.2 passes the VarInt straight to quinn (no WebTransport-over-H3 code mapping), so browsers may surface a null or unmapped code. Neither code is 0.
- **Hop (protocol C):** wrapping `fetch_add`; "no reset API" and "consumed at rewrite" docs move with it; a test that the hop continues across a source switch while A's old downlink and B's new downlink are both live.
- **Tests (test A–H):** an audio refill A→C hop-continuity regression (fails at `b070e870`), with existing datagram tests unchanged. Split driver covers a single split at every offset, 1-byte chunking, a seeded random multi-split (seed printed on failure) and a frame-end + next-header chunk, asserting the `forwarded{ingress}` delta == frames and byte-identical payloads. Stall tests: slow-but-progressing (chunk every bound−1 ms, no reset), mid-header (`partial_frame_discard` +1), mid-tail (`publisher_stall` +1, no partial). Rate limiter: N admitted, N+1 refused (zero bytes read, stopped with the code), admitted again after `advance(1s)`. Concurrency bound derived from production constants (no literals). Ingress-groups gauge returns to 0 table-driven over every exit row incl. abort. Label change mid-stream and first-frame-not-a-group-start each get an integration test. current_thread runtimes, one positive control per file.
- **Held-reader knob (test-approved, I implement):** `VideoSubscriber::spawn_held()` / `release()`, opt-in; the default stays byte-identical. Per-stream terminal outcome plus per-group gop_counter recorded. Primary evidence is client-observed: the held subscriber sees Reset on ≥1 group and after release a LATER group whose first frame is independently decodable; the second subscriber sees every group, contiguous, all Finished. The `subscriber_backlog` delta is secondary (≥1). Payload sizing is derived from deployed config (backlog + send window + client stream receive window, 2× margin), with a loud precondition failure. The held subscriber's audio keeps arriving if cheap.
- **Slice pinning (security A, blocker resolved): copy-on-backlog.** A tail chunk is enqueued zero-copy only when its downlink's queue is empty AND the writer is parked idle. Otherwise it is copied (`Bytes::copy_from_slice`, coalesced into the downlink's MH-owned buffer) before it is retained, so the backlog charge equals real memory. The only uncopied retained slice per downlink is the one inside a pending `write_all` (quinn copies on accept). Pinned parents are therefore at most 2 downlinks × stream_ceiling × one recv batch, which is documented at the downlink and in the memory-check comment, as is the ingress chunk in flight (≤ 64 streams × 1 chunk × 1 batch). Test: sub-slices of a large parent fed to a held subscriber, then `try_into_mut` on the parent succeeds after enqueue. The fan-out refcount gate is asserted on the idle-writer path upstream of the seam.
- **Stall semantics (security B):** inactivity timer, re-armed on progress, matching the task's "stalled mid-frame". The doc at the stall site records that frame.rs obligation #2 ("time-to-completion") is satisfied in inactivity form, and why a trickler only hurts its own media (64 streams × 64 KiB window, ≤2 downlinks per edge, copy-on-backlog).
- **Layering (code-reviewer 1, option a):** the cell, the machine, reconcile, abandonment and counting all live under `media/` (`media/edge_runtime.rs`, replacing the planned `routing/runtime.rs`). The session actor's apply calls `media::edge_runtime::reconcile(previous routes, &policy)` before `RoutingTable::install`. `MeetingRoutes` only stores the opaque `Arc<EdgeRuntime>` per edge plus the extended delivery index. No macros in either place.
- **Bounded downlink channel (code-reviewer 2, security C):** a bounded `mpsc` per subscriber connection, capacity 2 × the derived stream ceiling, fed by `try_send`. `Full` abandons that destination (`subscriber_backlog`); `Closed` is the dead-subscriber path (`connection_closed` on the discovering frame). The bound argument is also commented at construction.
- **Code-reviewer 3–6 adopted:** `unwrap_or_else(PoisonError::into_inner)`; `CONNECTION_SEND_WINDOW_BYTES` and `MEMORY_CHECK_FRACTION` in `config.rs` beside the receive windows, with a pure `check_pod_memory(..) -> Result<MemoryBudget, ConfigError>`; `Relaxed` hop `fetch_add` documented as unique-not-ordered across concurrent publishers, with the width assert and wrap tests moved intact; the group task split into per-chunk, per-header and terminal-exit methods on a group-state struct.
- **DRY (dry-reviewer):** (1) a generic `TokenBucket { rate, burst, admit(now) }` in `caps.rs` that MH-B's recovery limiter will reuse. It is not `governor`: tokio's paused `Instant` cannot implement governor's foreign `Reference` trait without a newtype adapter whose arithmetic would itself need testing, and that reason is recorded at the definition. (2) ONE `per_connection_memory_bound_bytes(&config)` summing receive window + datagram receive buffer + send window + datagram send buffer, used by the memory check and the boot log; the `CONNECTION_RECEIVE_WINDOW_BYTES` ledger and the config.env table cite it. (3) `reject_below` beside `reject_zero`/`reject_above`. (4) The env-test helpers from 37 that 38 needs are hoisted into `fixtures/` and 37 is switched to them. (5) The shim imports the codes from `transport/mod.rs`; env-test 38 asserts reset-not-finish, never the value. (6) Policies come from `mh-test-utils/src/media_policy.rs` and frames from `tests/common/media_frame.rs`, extended there.

### Implementation notes (deviations from the plan text, for review)
- **Reconcile call site:** `edge_runtime::reconcile` runs inside `RoutingTable::install` (via `MeetingRoutes::build`), still before the successor snapshot is published and still on the session actor (the table's only writer). This keeps ONE entry point that tests and the actor both use, so no install can skip reconcile. Routing calls into `media::edge_runtime` and stores `Arc<EdgeRuntime>`; all transitions, resets and counting stay under `media/`.
- **Grafana:** the new basic panels pushed the shared `grafana-dashboards-mh` ConfigMap past `dt-guard kustomize`'s 80% annotation headroom, so `mh-media.json` now has its own ConfigMap (`grafana-dashboards-mh-media`), mounted in Grafana's projected volume (the `mc-media.json` precedent).
- **Real-QUIC finding:** a `RESET_STREAM` that overtakes the WebTransport uni-stream header leaves the peer unable to attribute the stream to the session, so it never surfaces at the peer's `accept_uni` at all. Harmless for MH (an abandoned group the subscriber never saw), but the real-impl tests deliver the header first. Recorded for C-B (the client reader must not assume every MH group stream is observable).
- **Env-test 38 not run here** (needs the Kind cluster; Layer 7 is the Lead's).

### Gate-3 fixes (review round 1)
- **Egress open amplification (security F1, protocol F1):** `edge.rs` starts a new downlink only for an independently-decodable frame whose ingress group token is strictly NEWER than the current one; a same-group ID frame continues, an older group's frame is skipped. Tests: unit + `an_all_group_start_stream_opens_exactly_one_downlink`, `alternating_group_starts_across_two_streams_open_at_most_one_more`.
- **One attribution kernel (DRY F1/F2, protocol F2, SG F2, CQ-2):** `routing::classify` (sender-mute first, then declaration) behind both `attribute()` and the reconcile status; `Attribution::Route` carries the narrowed `StreamNumber`; `media::attribution_drop_reason` is the one drop-token mapping for both paths; header bytes are read before `forwarded{ingress}` is counted.
- **Downlink memory (security F2):** exact header allocations; the coalescing buffer is taken whole and dropped on abandon; `BACKLOG_ALLOCATION_FACTOR = 2` in the boot check (base ≈404 MB, Kind ≈450 MB of 537 MB).
- **Switch target leaves (CQ-1):** the edge stays `SwitchPending` (A kept) so a returning B still completes the switch.
- **`partial_frame_discard` (SG F1, reverses observability C):** counted on every non-clean end incl. MH's abort — a publisher disconnect reaches the group as an abort or a read error by scheduling; one event, one count.
- **Counting unit (observability refinement):** group resets are one per downlink group abandoned for a destination, opened or not (1 + N on a full channel).
- **Rewrite-failure arm (CQ-3)** withdraws the fresh downlink; `deliver_header` split into `open_downlink` / `push_header` (CQ-4); test-only constructors renamed `RoutingTable::unbounded_for_tests`, `SessionManagerHandle::new_for_tests` / `new_with_parts_for_tests`, `Default` removed (CQ-5).
- **Ops:** deployment / config.env / runbook comments now say MH-A2's keys are read; the refusal line is built with `serde_json`.
- **Tests:** real-QUIC reset tests fail closed on `Reset(GROUP_ABANDONED)`; backlog boundary on both sides; concurrency positive control; torn-down subscriber pins zero resets + the discovering `connection_closed`; FIN-mid-header and codec-reject rows; paused-resume negative; mid-frame switch hop test; shim peak of two; env-test 38 phase (b) asserts C skipped the in-flight group and phase (f) runs ONE publisher across hold → release with evidence from C's full gop set; the send-window literal is drift-guarded from `config.rs`; DRY test-helper hoists.

### Gate-2 (Layer 7) fix, attempt 1
- **Root cause (test bug, not MH):** env-test 38's `drain()` read until the subscriber was quiet for 500 ms. In phase (f) the publisher sends continuously until told to stop, so the drain never returned and every surrounding deadline check was starved — the test hung instead of failing. `drain()` is now bounded by `DRAIN_SLICE` (2 s) as well, so each caller's deadline check runs. Per-phase `[38] phase …` markers on stderr name where a run is. No assertion was weakened; the held-reader path works as designed (B's held streams are accepted, reset past the cap, and B resumes at a later group).
- Re-ran 38 against the deployed Kind cluster 3× (seeded org): green each time, ~80 s.

### Questions for reviewers
- security: label-change-mid-stream ⇒ `unmatched_stream` + stop + downstream `publisher_reset`; single reset code; codec reject ⇒ downstream `publisher_reset`. OK?
- observability: `partial_frame_discard` re-classified ingress (mid-header only), mid-tail truncation under `publisher_reset`; `connection_closed` widened for downlink stream loss; gauge names.
- operations: 512 KiB send window, fraction 1/2, figures above.
- test: shim API shape above; held-reader knob on `VideoSubscriber` for the backlog row (yours to approve).
- protocol: no `media-protocol` edits planned; hop moves to the edge cell for both transports.

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
| Paired Protocol | confirmed |

Classification-sanity guard: `STATUS=OK`. Plan approved by Lead 2026-10-21.

---

## Pre-Work

None

---


---

## Gate 3 — Reviewer Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security (paired) | RESOLVED-FIXED | 3 | 3 | 0 | F1 egress stream-open amplification (newer-ingress-group-only StartGroup); F2 memory check undercount (exact header allocs + BACKLOG_ALLOCATION_FACTOR=2); F3 real-QUIC reset tests fail-closed |
| Test (paired) | RESOLVED-FIXED | 10 | 10 | 0 | env-38 slow-subscriber/late-subscriber evidence reworked; non-vacuous controls added |
| Observability | RESOLVED-FIXED | 5 | 5 | 0 | panel units/titles, catalog precision, serde_json refusal line; accepted partial_frame_discard-on-abort reversal |
| Code Quality | RESOLVED-FIXED | 8 | 8 | 0 | CQ-1 abandoned-switch pinning bug; silent exits; *_for_tests renames |
| DRY | RESOLVED-FIXED | 5 | 5 | 0 | single classify() kernel; attribution_drop_reason(); env-test helpers hoisted |
| Operations | RESOLVED-FIXED | 3 | 3 | 0 | stale "image ignores" comments; JSON refusal line; formula prose points at check_pod_memory |
| Semantic Guard | RESOLVED-FIXED (SAFE) | 2 | 2 | 0 | [metrics-path-completeness] stream_ingress.rs abort-path partial_frame_discard; uncounted refusal arm removed |
| Paired Protocol | RESOLVED-FIXED | 3 | 3 | 0 | same-group ID frame no longer opens a downlink; StreamNumber carried in Route; A-mid-frame/B-group-start hop test added. No GSA edits. |

## Accepted Deferrals

- (none surfaced in this devloop)

---

## Gate 2 — Full Validation

| Attempt | Result | Notes |
|---------|--------|-------|
| 1 | FAIL (Layer 7, consumed L7 attempt 1/2) | L1-6 OK; env-test 38 `test_stream_per_group_video_forwarding` hung → suite timeout (exit 124). Root cause: test `drain()` waited for 500 ms quiet under a continuous publisher (test bug; MH unchanged). Fixed with DRAIN_SLICE cap + phase markers; test reviewer re-verified (no assertion weakened). |
| 2 | PASS — `TOTAL_RESULT=OK` | L1-6 OK; L7 `env-tests-passed` (incl. 38) + `browser-e2e-passed`. |

## Rollback

Start commit `b070e8709bc63817858fcf36bf9ccfc152f2e5a7`. **Safe-revert unit**: the whole commit — the send_window declaration, boot memory check, edge-runtime hop move (audio+video) and the Grafana ConfigMap split move together; a partial revert of the memory check without send_window (or vice versa) would leave the bound uncertified.
