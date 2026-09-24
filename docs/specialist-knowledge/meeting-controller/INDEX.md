# Meeting Controller Navigation

## Architecture & Design
- MC architecture, actor model, session binding, capacity → ADR-0023
- User auth, meeting access, join flow → ADR-0020
- Observability: metrics facade → ADR-0011; testability (`MetricAssertion`, presence guard, rollout SLO) → ADR-0032; service-owned dashboards and alerts → ADR-0031

## Code Locations
- Service entry point → `crates/mc-service/src/main.rs`
- Config (SecretString, env loading, ac_jwks_url, TLS paths, advertise addresses, quic_max_idle_timeout_seconds) → `crates/mc-service/src/config.rs`
- Error types (McError hierarchy, From<JwtError>, MhAssignmentMissing) → `crates/mc-service/src/errors.rs`
- Auth: McJwtValidator, validate_meeting_token, validate_guest_token → `crates/mc-service/src/auth/mod.rs`
- Actors: controller, meeting, participant, messages, session (HMAC/HKDF), metrics → `crates/mc-service/src/actors/`
- Media routing control plane (ADR-0036 §7/§8/§9): per-meeting join-order slot state (non-reflexive `subscribes_to`, fill/refill rules, exhaustive model check) + pure render into `MeetingAssignment` → `crates/mc-service/src/media_routing/slots.rs`; placement (round-robin by join rank over a SORTED `MeetingHandlers`; the one home for every client-facing handler url) → `media_routing/placement.rs`; output types + `egress_stream_id` packing → `media_routing/assignment.rs`; `policy_generation` (advance-only-on-change, `adopt_floor` after an MC restart) → `media_routing/generation.rs`; per-(meeting, handler) latest-wins push worker (meeting-scoped cancel, confirmed-generation no-op, retry/terminal split) → `media_routing/pusher.rs`; the push + fail-loud confirm → `crates/mc-service/src/grpc/mh_client.rs`; generation eviction on meeting teardown → `crates/mc-service/src/actors/controller.rs:remove_meeting()`
- Client-facing media signalling (ADR-0036 §5/§6): capability parse (whole-declaration rejection; `SlotId`; `audio_slot_ids()` = slot demand), send directive (**no mute access — module-level invariant**), slot assignments (the one `StreamAssignments` construction site; `unreachable_sender_ids`), bounded outcome vocabularies incl. `SlotViewEmission` → `crates/mc-service/src/media_signaling/`; the meeting actor is the single composer: slot table, frozen handler set, push workers, per-participant last-emitted views, per-turn deferring flush bound (`SLOT_VIEW_FLUSH_BATCH`) → `crates/mc-service/src/actors/meeting_media.rs`; structural-change drivers (join, leave choke-point, capability, self/server mute, reconnect re-dirty) → `actors/meeting.rs`; connection-side validation/budget/limiters only → `webtransport/connection.rs:handle_receive_capability()`/`handle_mute_request()`; test-only `MeetingSeams` (placement pins, flush bound, sender-id cursor) → `actors/meeting_media.rs`
- Media admission (ADR-0036 §4): meeting KEK + `u16` generation, roster `identity_public_key`, non-recycling `sender_id` allocator → `crates/mc-service/src/media_admission/`; KEK generated in `MeetingActor::spawn`, `sender_id` allocated in `handle_join` (`actors/meeting.rs`); key length-checked at the WebTransport boundary + `JoinResponse` fill (`webtransport/connection.rs`); MH→MC sender-id→user resolution: `SenderBindingOutcome` → `media_admission/binding_response.rs`, `GetSenderIdForUser`/`SenderLookup{Found,NotFound,Ambiguous}` → `actors/messages.rs` + `actors/meeting.rs:get_sender_id_for_user()`, `resolve_sender_binding()` → `grpc/media_coordination.rs`
- Join display-name plumbing: boundary truncate (connection.rs) → `display_name` field on `JoinConnection`/`ConnectionJoin` (messages.rs) → empty-claim fallback sink → `crates/mc-service/src/actors/meeting.rs:handle_join()`
- Disconnect cause / leave latency: `DisconnectCause` enum → `crates/mc-service/src/actors/messages.rs`; skip-grace on clean close + `remove_and_broadcast_left()` choke-point → `crates/mc-service/src/actors/meeting.rs:handle_disconnect()`; cause-cell (AtomicU8) → `crates/mc-service/src/actors/participant.rs`
- WebTransport: server (accept loop, TLS, capacity; config-driven max_idle_timeout/keep_alive for crash detection in `bind()`) → `crates/mc-service/src/webtransport/server.rs`
- WebTransport: connection (join flow: meeting handle + Redis handler set read BEFORE the actor join, `JoinMedia` into the join, `media_servers` scoped to the placed handler; bridge loop, Connection::closed() watch + classify_connection_error, MediaConnectionUpdate handler) → `crates/mc-service/src/webtransport/connection.rs`
- WebTransport: handler (encode_participant_update) → `crates/mc-service/src/webtransport/handler.rs`
- gRPC: GC client (registration, heartbeats, advertise) → `crates/mc-service/src/grpc/gc_client.rs`
- gRPC: MC service (AssignMeetingWithMh) → `crates/mc-service/src/grpc/mc_service.rs`
- gRPC: MH client + MhRegistrationClient trait (RegisterMeeting, per-call Channel) → `crates/mc-service/src/grpc/mh_client.rs`
- gRPC: auth interceptor + McAuthLayer (async JWKS + scope check, R-22) → `crates/mc-service/src/grpc/auth_interceptor.rs`
- gRPC: media coordination service (MH→MC notifications, R-15) → `crates/mc-service/src/grpc/media_coordination.rs`
- MH connection registry (participant→MH state, R-18, lifecycle via controller actor) → `crates/mc-service/src/mh_connection_registry.rs`
- Redis: fenced client + MhAssignmentStore trait + MhAssignmentData (handlers Vec) → `crates/mc-service/src/redis/client.rs`; Lua scripts (atomic fencing) → `crates/mc-service/src/redis/lua_scripts.rs`
- Health/readiness, system info → `crates/mc-service/src/observability/health.rs`, `crates/mc-service/src/system_info.rs`
- Prometheus metric wrappers (record_media_policy_push, record_register_meeting, record_mh_notification, record_webtransport_connection, record_jwt_validation, record_session_join, record_token_refresh_metrics, record_participant_leave, record_participant_disconnect, record_display_name_resolution, record_sender_binding_response, record_slot_view_emission, record_unreachable_senders, record_handler_set_divergence, set_receive_slot_cap) → `crates/mc-service/src/observability/metrics.rs`
- MC metrics catalog → `docs/observability/metrics/mc-service.md`

## Protocols
- Client signaling (join, mute, session recovery, MediaConnectionUpdate, ReceiveCapability, SendDirective, StreamAssignments) → `proto/dark_tower/signaling/v1/signaling.proto`
- Internal service RPCs (RegisterMc, AssignMeeting, MediaCoordinationService, RegisterMeeting) → `proto/dark_tower/internal/v1/internal.proto`

## Integration Seams
- Client -> MC WebTransport (join, signaling) → `crates/mc-service/src/webtransport/server.rs`; MC <-> GC registration/heartbeat → `crates/mc-service/src/grpc/gc_client.rs`
- GC -> MC assignment → `crates/mc-service/src/grpc/mc_service.rs`
- MH -> MC notifications (connect/disconnect) + sender-id binding resolution → `crates/mc-service/src/grpc/media_coordination.rs:resolve_sender_binding()`
- MC -> AC: token management → `crates/common/src/token_manager.rs`; JWKS (meeting token validation) → `crates/common/src/jwt.rs:JwksClient`
- MC -> MH RegisterMeeting RPC (full snapshot + `policy_generation` on every structural change; confirms the applied echo) → `crates/mc-service/src/grpc/mh_client.rs:register_meeting()`, driven by `media_routing/pusher.rs`
- MC -> Redis session/fencing + MH assignment read → `crates/mc-service/src/redis/client.rs:get_mh_assignment()`

## Testing
- Shared bring-up (TestStackHandles, build_test_stack, seed_meeting_with_mh) + mock MH stores → `crates/mc-service/tests/common/mod.rs`
- Accept-loop component rig → `crates/mc-service/tests/common/accept_loop_rig.rs`
- Join flow tests (TestServer, MockMhRegistrationClient.wait_for_calls, multi-MH and skip-grpc-endpoint cases) → `crates/mc-service/tests/join_tests.rs`
- Client media signalling integration (two-party: capability in → directive + assignments out, directive re-sent when first held, solo hears nothing, mute re-emits to holders only, rejection paths, cap boundary, slot-id echo, client-url redirect guard, placement ignores Redis order) → `crates/mc-service/tests/media_client_signaling_integration.rs`; multi-party slot placement (fill/refill, cross-handler no-push re-emit, split meeting per-handler edges, pins, flush bound, grace-expiry re-push, reconnect keeps slots, re-push mismatch, cap gauge) → `crates/mc-service/tests/slot_placement_integration.rs`; shared multi-participant session harness → `tests/common/media_session.rs`
- Media admission integration (KEK in join response, malformed-key reject, sender_id non-recycling + exhaustion, roster attribution) → `crates/mc-service/tests/media_admission_integration.rs`; reconnect continuity is actor-level in `actors/meeting.rs`
- Accept-loop status + per-failure-class drilldown → `crates/mc-service/tests/webtransport_accept_loop_integration.rs`
- gRPC auth-layer per-failure-reason → `crates/mc-service/tests/auth_layer_integration.rs`
- Media coordination notifications + connect/disconnect round-trip + sender-binding resolution arms → `crates/mc-service/tests/media_coordination_integration.rs`; OTel inbound-continuity fixture → `crates/mc-service/tests/otel_grpc_inbound_continuity.rs`; leave/disconnect counter deltas + ParticipantLeft wire frames → `crates/mc-service/tests/disconnect_latency_integration.rs`
- Media policy push outcomes + divergence gauge + wire shape → `crates/mc-service/tests/media_policy_push_integration.rs`; RegisterMeeting metrics → `crates/mc-service/tests/register_meeting_integration.rs`; shared MH gRPC stub (applied-generation echo knobs), loopback policy fixtures, shared `TokenReceiver` → `crates/mc-test-utils/src/mock_mh.rs`
- ActorMetrics / MailboxMonitor metrics → `crates/mc-service/tests/actor_metrics_integration.rs`
- Redis-class wrapper coverage → `crates/mc-service/tests/redis_metrics_integration.rs`
- Token-refresh integration → `crates/mc-service/tests/token_refresh_integration.rs`
- GC integration + heartbeat metrics → `crates/mc-service/tests/gc_integration.rs`
- Heartbeat task tests → `crates/mc-service/tests/heartbeat_tasks.rs`
- Per-cluster MetricAssertion tests + Cat B matrix → `crates/mc-service/src/observability/metrics.rs`
- Test utilities (mock GC/Redis/MH, jwt_test) → `crates/mc-test-utils/src/`
- Env-tests: MC-GC integration → `crates/env-tests/tests/22_mc_gc_integration.rs`; MH QUIC + MC↔MH coordination metrics + live-handler policy-push confirm → `crates/env-tests/tests/26_mh_quic.rs`; five-participant split-handler slot placement (S10b, two-sender binding at MH, over-subscription) → `crates/env-tests/tests/27_mc_slot_placement.rs`; shared env-test MC/MH WebTransport client (`McSession`, `mc_join` positive control) → `crates/env-tests/src/fixtures/mc_session.rs`

## Config
- Advertise addresses `grpc_advertise_address` / `webtransport_advertise_address`; consumed by GC registration + MH RegisterMeeting → `crates/mc-service/src/config.rs`, `crates/mc-service/src/grpc/gc_client.rs`, `crates/mc-service/src/webtransport/connection.rs`
- ADR-0036 §5/§6 client-signalling keys (`MC_MAX_RECEIVE_SLOTS`, `MC_MAX_RECEIVE_CAPABILITY_DECLARATIONS`, `MC_AUDIO_*`): REQUIRED, no Rust defaults, bounded both sides at load, documented values in the ConfigMap; the audio knobs are a documented inequality against `mh-service`'s datagram sizing, not a shared constant → `crates/mc-service/src/config.rs`, `infra/services/mc-service/configmap.yaml`, `docs/TODO.md` §Media Path Obligations

## Infrastructure
- K8s deployment (POD_IP downward API, advertise addresses) → `infra/services/mc-service/mc-0-deployment.yaml`, `infra/services/mc-service/mc-1-deployment.yaml`
- K8s network policy (MH ingress on 50052) → `infra/services/mc-service/network-policy.yaml`
- Grafana dashboard → `infra/grafana/dashboards/mc-overview.json`
- Prometheus alert rules → `infra/docker/prometheus/rules/mc-alerts.yaml`
- Incident runbook (Sc 12 RegisterMeeting, Sc 13 unexpected MH notifications, Sc 14 roster-remove latency budget) → `docs/runbooks/mc-incident-response.md`
- Deployment runbook (post-deploy MC↔MH coordination addendum) → `docs/runbooks/mc-deployment.md`
