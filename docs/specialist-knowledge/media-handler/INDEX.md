# Media Handler Navigation

## Architecture & Design
- SFU architecture, MH registration/load reports → ADR-0010 (Section 4a)
- Actor pattern for concurrency (handle/task, message passing, no locks) → ADR-0001
- MH assignment, selection algorithm, cross-region coordination → ADR-0023 (Section 5)
- Service authentication (MH→GC OAuth, MC→MH Bearer) + gRPC auth scopes (two-layer: JWKS+scope server-wide, service_type per-path) → ADR-0003
- Observability pattern (metrics crate facade) → ADR-0011; dashboard presentation (counters vs rates) → ADR-0029; service-owned dashboards and alerts → ADR-0031
- Metric testability (component tests drive accept loop; `MetricAssertion` snapshots) → ADR-0032
- Fuzz testing for media frames → ADR-0006
- Client-to-MH QUIC connection user story (closed) → `docs/user-stories/2026-04-12-mh-quic-connection.md`

## Code Locations
- Service entry point → `crates/mh-service/src/main.rs`
- Library root (module declarations) → `crates/mh-service/src/lib.rs`
- Config (TLS, advertise addrs, AC_JWKS_URL, register_meeting_timeout) + ADR-0036 §1 transport: 5 REQUIRED env vars (presence-check literals are guard-load-bearing — never extract a `require_var` helper), the compile-time half (idle timeout, receive windows, datagram receive buffer, egress-queue bound, shutdown constants), 3 fielded-`ConfigError` startup validations (load precedes subscriber init, so the variant IS the operator surface), drain = `min(SETTLE_TARGET, grace − MARGIN)` + `DrainWindowSource`, and `NOMINAL_AUDIO_FRAME_BYTES` (236; frames→bytes from `media-protocol` components, sized at the bitrate range FLOOR because the send buffer is a LATENCY CEILING not a capacity guarantee — NOT `MAX_PAYLOAD_BYTES`) → `crates/mh-service/src/config.rs`
- Error types (MhError hierarchy) → `crates/mh-service/src/errors.rs`
- gRPC: GC client (registration, heartbeats, re-registration) → `crates/mh-service/src/grpc/gc_client.rs`
- gRPC: MC client (Notify connect/disconnect, retry with backoff, auth short-circuit); `notify_participant_connected` returns the participant's `sender_id` (the binding source) → `crates/mh-service/src/grpc/mc_client.rs`
- gRPC: MH service — RegisterMeeting IS the ADR-0036 §8 control plane (validate → upsert/promote → apply policy → echo the APPLIED generation, read from the live snapshot never from the request) → `crates/mh-service/src/grpc/mh_service.rs`
- gRPC: auth layer (MhAuthLayer JWKS+scope+Layer-2 routing, ADR-0003; `classify_jwt_error` → bounded failure_reason) → `crates/mh-service/src/grpc/auth_interceptor.rs`
- JWT validation (MhJwtValidator wrapping common JwtValidator, token_type=meeting) → `crates/mh-service/src/auth/mod.rs`
- Subscriber registry (`LocalSubscribers`, composite `SubscriberKey{meeting,sender}` unconstructible without a `MeetingKey`, ArcSwap, read per frame) + `SenderBindings` — the participant→`sender_id` map (`bind()`/lookup, incumbent-wins on collision); WRITER is now `resolve_sender_binding` at connect (see connection.rs below); ADR-0036 §11 → `crates/mh-service/src/session/mod.rs`
- Session management (SessionManagerActor/Handle, pending promotion via Notify) + config-apply mailbox: a SECOND mpsc into the same actor, `select!` over both (§8: config-apply must not route through the bounded lifecycle mailbox); apply semantics stale/equal-no-swap/greater → `crates/mh-service/src/session/mod.rs`
- Forwarding-policy types + lock-free ArcSwap snapshot; structural rejects (counts FIRST, whole registration, no state mutation); scoped newtypes `SenderId`/`SlotId`/`StreamNumber` with widths derived from `media-protocol`, never literals → `crates/mh-service/src/routing/mod.rs`
- Meeting-scoped sender resolution — the ONLY sender lookup, unconstructible without a `MeetingKey`, BORROWING and non-allocating because the forward path calls it per frame (the `Vec`-returning `sources_for` was removed, not kept beside it); cross-tenant guard, `docs/TODO.md` §Media Path Obligations (a) → `crates/mh-service/src/routing/mod.rs:for_each_source`
- Process-incarnation epoch for MC's restart detector — sampled ONCE in `main`, never per call → `crates/mh-service/src/process.rs`
- Policy bounds + hard ceilings (`PolicyLimits`; resource exhaustion, NEVER capacity, never advertised to GC) → `crates/mh-service/src/config.rs:PolicyLimits`
- WebTransport server (TLS 1.3, accept-time exhaustion guard) + explicit `quinn::TransportConfig` via `with_custom_transport` (needs the `wtransport/quinn` feature) → `crates/mh-service/src/webtransport/server.rs:build_transport_config`
- WebTransport connection handler (framed JWT, provisional accept, MC notifications); `resolve_sender_binding()` asks MC for `sender_id` and writes `SenderBindings`, `start_media_session` spawns the loops only past that fail-closed gate (`SenderBindingOutcome`, `MediaSessionStartOutcome`) → `webtransport/connection.rs:resolve_sender_binding()`; REAL `MediaTransport` impl, where `WtSendStream.finished` disambiguates dead-stream from dead-connection because wtransport COLLAPSES quinn's `ClosedStream` into `NotConnected` → `webtransport/media_transport.rs`
- Health + readiness endpoints → `observability/health.rs`; Prometheus metric recorders, `mh_media_policy_applies_total{outcome,key_custody}` 5 bounded outcomes → `crates/mh-service/src/observability/metrics.rs:PolicyApplyOutcome`
- Shared `key_custody` label vocabulary (hoisted to common at its 2nd consumer) → `crates/common/src/observability/labels.rs`
- MH metrics catalog → `docs/observability/metrics/mh-service.md`

## Media Protocol (v2 frame codec — ADR-0036 §2)
- Frame v2 layout, exported size constants, decoded `MediaFrameView` → `crates/media-protocol/src/frame.rs`
- Forward-path buffer-sizing constants (`MAX_HEADER_BYTES`, `MAX_FRAME_BYTES`, `MAX_PAYLOAD_BYTES`) → `frame.rs` (derived; do not recompute in MH)
- Sequence-reset asymmetry: `hop_sequence` resettable / `stream_sequence` MUST NEVER reset (AEAD nonce input — reset = GCM auth-key recovery) → `frame.rs` accessors. **CORRECTED at task 16**: `HopSequence` did NOT land here. @protocol ruled it MH runtime state, not wire format — MH-generated, never publisher-set, and the contract is already complete (`HOP_SEQUENCE_FIELD_BYTES` + the `u32` parameter on `rewrite_relay_region`), so the GSA surface stays unwidened. It lives at `crates/mh-service/src/media/forwarder.rs`, width derived by `const` assertion, no reset API.
- Single layout parser + four entry points (`decode_datagram`, `decode_stream_frame`, `peek_frame_len`, `rewrite_relay_region`); reject reasons via enum-derived `ALL_REJECT_REASONS` → `crates/media-protocol/src/codec.rs`
- Relay-region rewrite (offset derived per frame, NEVER a constant) → `codec.rs:rewrite_relay_region()`; reader-side pre-alloc bound → `codec.rs:peek_frame_len()`
- Extension TLV registry + salience `0x01` selector input (value `0..=100`) → `crates/media-protocol/src/extensions.rs` (story-5 selector)
- Fuzz: decode / roundtrip → `crates/media-protocol/fuzz/fuzz_targets/`

## Proto Definitions
- MC↔MH / MH↔GC / MH→MC RPCs + assignment + DisconnectReason → `proto/dark_tower/internal/v1/internal.proto`
- Client signaling (MediaStream, StreamAssignment, MediaConnectionUpdate, layout) → `proto/dark_tower/signaling/v1/signaling.proto`
- Generated Rust code → `crates/proto-gen/build.rs`

## Integration Seams
- MH → AC token management → `crates/common/src/token_manager.rs`; MH → AC JWKS (meeting + service token validation) → `crates/common/src/jwt.rs:JwksClient`
- Client → MH WebTransport (QUIC/TLS 1.3, framed JWT first) → `crates/mh-service/src/webtransport/server.rs`
- MH depends on common, proto-gen, media-protocol crates

## Media Forward Path (ADR-0036 §2/§7/§11 — task 16)
- HOT PATH ONLY — the directory boundary IS the hot-path boundary: no `tracing`/`log`/`metrics`/`println!`/`event!`/span/`#[instrument]` macro under it, setup/lifecycle/teardown are SIBLINGS, handles injected → `crates/mh-service/src/media/mod.rs`
- `forward_one`, the Tier-1a pure function: decode-then-rewrite (two entry points, ONE `parse_layout`), routing re-read per frame with NO cache (a cached edge list is a §7 server-mute bypass), single-pass fan-out where the LAST edge is zero-copy and earlier edges take arena copies, fail-closed with a distinct token per cause. Clock split named at each site: latency MEASUREMENT on `std::time::Instant` (never paused), deadlines on `tokio::time` (`start_paused` drives them), no `Clock` trait. **The relay offset is DERIVED PER FRAME by `rewrite_relay_region`, never a constant** — it moves with the key-bearing flag and the extension TLV, both live on audio, and a hardcoded offset is a remotely triggerable cross-participant corruption primitive whose ONLY signal is client-side `signature_invalid` → `crates/mh-service/src/media/forward.rs`
- `HopSequence` (MH-local, per (connection, MEDIA stream) keyed by `egress_stream_id`, wrapping, NO reset API, consumed at rewrite time so a shed frame leaves a real gap) + `ConnectionForwarder` (sender bound at spawn AFTER the JWT gate, fan-out arena) → `crates/mh-service/src/media/forwarder.rs`
- `BoundedDropOldest<T>`/`SharedQueue<T>` — ONE ring used twice; drop-OLDEST makes it a latency ceiling not a buffer; returns the evicted item, the CALLER counts it → `media/queue.rs`. Pre-allocation DoS caps (whole-datagram byte cap vs the codec's declared-length check: same constant, two checks, two tokens) + stream creation-rate limiter → `media/caps.rs`. Three per-connection loops, and why egress is its OWN task (otherwise the egress ring never fills and `egress_queue_overflow` has no firing path) → `media/ingress.rs`. Random one-in-N sampler; modulo/hash/identity-seeded/fixed-period all FORBIDDEN (deterministic within a stream reconstructs the voice-activity trace) → `media/sampler.rs`
- Five handles pre-resolved at setup, `MediaDropReason` (13 MH-local tokens, one direction each) + the codec family iterated from `ALL_REJECT_REASONS`, buckets, provisional objective → `crates/mh-service/src/observability/metrics.rs:resolve_media_handles`; datagram receive-path accounting — teardown reconciles `quic_datagram_frames_received` vs `FramesRead.unread_since` through `record_media_frames_dropped` (task 26, R-15) → `webtransport/connection.rs` + `media/ingress.rs:FramesRead`; `per-frame-trace` `compile_error!` gate (mechanism copied from mc-service, reasoning cross-referenced) → `crates/mh-service/src/lib.rs`

## Testing
- Integration, control plane and auth: GC mock → `tests/gc_integration.rs`; MC client retry + auth short-circuit → `tests/mc_client_integration.rs`; MhAuthLayer over real tonic (alg:none + HS256, Layer 2 routing) → `tests/auth_layer_integration.rs`; RegisterMeeting over real gRPC → `tests/register_meeting_integration.rs`; policy-apply labels + counting-boundary denominator invariant → `tests/policy_apply_integration.rs`
- ADR-0036 §10 Tier-1b gates (apply-failure does not advance the echo, monotonicity, re-assert idempotency, gen-0) → `mh_service.rs` tests; multi-meeting cross-tenant pin (both arms) → `routing/mod.rs` tests
- Integration: WebTransport accept path/provisional timeout/MC notify → `tests/webtransport_integration.rs`, `tests/webtransport_accept_loop_integration.rs`; real-vs-double parity (finish-then-write via ONE shared generic assertion; recv-after-close; reliable-stream round-trip — NO datagram-delivery assertion, QUIC datagrams are unreliable) → `tests/transport_real_impl.rs`
- Integration rigs (JWKS mock, mock MC, gRPC rig, WT rig, token minters) → `crates/mh-service/tests/common/`; `RegisterMeetingRequest`/`EgressStream` fixture builders — ONE home, reachable from `src/` unit tests and `tests/` binaries alike, alongside the `MediaTransport` loss/delay shim → `crates/mh-test-utils/src/media_policy.rs`
- Tier-1a forward-path gates (relay-region-only rewrite byte-identical elsewhere, per-media-stream hop advance, an unforwardable edge consuming NO hop number, N>=2 zero-copy fan-out, no-per-frame-allocation WITH proof-of-trap, fail-closed trio, two-meeting both-arms) → `tests/media_forward_integration.rs`; Tier-1b back-pressure written against `EGRESS_QUEUE_FRAMES`, separating correct shedding from a wedged queue and from drop-NEWEST (which fails only on frame identity) → `tests/media_backpressure_integration.rs`; metric gates + the 16-token collision test (reads the vector file as DATA — the crypto tokens have no Rust home) + the in-crate macro-deny walker with its absent/empty-scope anti-false-green → `tests/media_metrics_integration.rs`; sender-binding firing paths + connect-flood + receive-path drop accounting → `tests/media_session_binding_integration.rs`
- MEASURED, not assumed: `Datagram::payload()` IS uniquely owned in MH's `recv_datagram` — but only because the `Datagram` is dropped before returning; holding it would silently move every frame onto the arena copy → `tests/transport_real_impl.rs:measure_whether_a_received_datagram_payload_is_uniquely_owned`
- Env-tests: full Kind cluster MH QUIC flow (R-33 scenarios) → `crates/env-tests/tests/26_mh_quic.rs`

## Infrastructure & Operations
- K8s deployment (ports, probes, env, downward API, advertise addresses) → `infra/services/mh-service/mh-0-deployment.yaml`, `infra/services/mh-service/mh-1-deployment.yaml`
- K8s configmap (bind addresses, region, GC URL, AC_JWKS_URL) → `infra/services/mh-service/configmap.yaml`
- MH↔MC network policy → `infra/services/mh-service/network-policy.yaml`, `infra/services/mc-service/network-policy.yaml`
- Grafana dashboard + kustomization → `infra/grafana/dashboards/mh-overview.json`, `infra/grafana/kustomization.yaml`
- Deployment runbook (post-deploy checklist) → `docs/runbooks/mh-deployment.md`; incident response → `docs/runbooks/mh-incident-response.md`
