# Meeting Controller Navigation

## Architecture & Design
- MC architecture, actor model, session binding, capacity → ADR-0023
- User auth, meeting access, join flow → ADR-0020
- Media flow (admission/KEK §4, client signalling §5/§6, slots + edges §7/§9, policy push + teardown §8, hygiene §11) → ADR-0036
- Content-addressed per-service config env root → ADR-0038
- Observability: metrics facade → ADR-0011; testability (`MetricAssertion`, presence guard, rollout SLO) → ADR-0032; service-owned dashboards and alerts → ADR-0031

## Code Locations
- Service entry point (fleet-gauge samplers) → `crates/mc-service/src/main.rs`
- Config (env loading, TLS, advertise addresses, ADR-0036 media keys, `Config::kek_lifecycle`) → `crates/mc-service/src/config.rs`
- Error types (McError hierarchy) → `crates/mc-service/src/errors.rs`
- Auth: McJwtValidator, validate_meeting_token, validate_guest_token → `crates/mc-service/src/auth/mod.rs`
- Actors: controller, meeting, participant, messages, session (HMAC/HKDF), metrics → `crates/mc-service/src/actors/`

## Meeting Actor (single composer + connectivity home)
- Structural-change drivers (join, leave choke-point `remove_and_broadcast_left`, capability, mute, MH connect/disconnect, reconnect) → `crates/mc-service/src/actors/meeting.rs:handle_join()`/`handle_reconnect()`/`handle_disconnect()`/`handle_media_connected()`/`handle_media_disconnected()`
- Media state (slot table, frozen handler set, connectivity, push workers, slot-view flush, `MutedSourceCensus`, test-only `MeetingSeams`) → `crates/mc-service/src/actors/meeting_media.rs`
- Teardown fence + generation eviction on both teardown paths → `crates/mc-service/src/actors/controller.rs`
- `DisconnectCause`, `SenderLookup` (folded into `MeetingMessage::MediaConnected`) → `crates/mc-service/src/actors/messages.rs`; cause-cell + `handle_kek_update` → `actors/participant.rs`

## Media Routing (ADR-0036 §7/§8/§9)
- Join-order slots, edges, `render` into `MeetingAssignment` → `crates/mc-service/src/media_routing/slots.rs`; shared-handler choice per edge → `media_routing/edges.rs`
- Observed connectivity (keyed by MH `connection_id`, settle window) → `media_routing/connectivity.rs`
- Frozen handler set, `ConnectedHandlers` (client-facing handler urls) → `media_routing/placement.rs`; output types (`HandlerAssignment`) → `media_routing/assignment.rs`
- `policy_generation` → `media_routing/generation.rs`; latest-wins push worker → `media_routing/pusher.rs`; `RegisterMeetingResponse` classification → `media_routing/confirm.rs`
- Meeting end: quiesce → `EndMeeting` plan → outcome classifier → `media_routing/teardown.rs`; exit hand-off → `actors/meeting.rs:hand_off_teardown()`

## Client Media Signalling (ADR-0036 §5/§6)
- Capability parse → `crates/mc-service/src/media_signaling/capability.rs`; send directive → `media_signaling/directive.rs`; `StreamAssignments` construction → `media_signaling/assignments.rs`; outcome vocabularies → `media_signaling/outcome.rs`
- Server mute: dispatch authority + limiters → `crates/mc-service/src/webtransport/connection.rs:handle_server_mute_request()`/`refuse_server_mute()`/`handle_unmute_request()`; actor side → `actors/meeting.rs:handle_server_mute()`/`handle_request_unmute()`/`server_mute_replay()`

## Media Admission (ADR-0036 §4)
- Module root → `crates/mc-service/src/media_admission/`; `IdentityPublicKey` → `identity_key.rs`; `SenderIdAllocator` → `sender_id.rs`; `AdmissionEpoch` → `epoch.rs`; `MeetingKeyState::rotate` → `kek.rs`; `SenderBindingOutcome` → `binding_response.rs`
- KEK rotation lifecycle (`KekLifecycle`, `RotationTrigger`, `KekPushOutcome`) → `crates/mc-service/src/media_admission/rotation.rs`; driver + test-only `force_kek_rotation` → `actors/meeting.rs`

## Transport & gRPC
- WebTransport server (accept loop, TLS, idle timeout) → `crates/mc-service/src/webtransport/server.rs`; connection (join flow, bridge loop, boundary validation) → `webtransport/connection.rs`; `encode_participant_update` → `webtransport/handler.rs`
- GC client (registration, heartbeats, `notify_meeting_ended`, `MeetingEndedQueue`) → `crates/mc-service/src/grpc/gc_client.rs`; GC→MC assignment → `grpc/mc_service.rs`
- MH client (`register_meeting`, `end_meeting`, MhRegistrationClient trait) → `crates/mc-service/src/grpc/mh_client.rs`
- MH→MC notifications → `crates/mc-service/src/grpc/media_coordination.rs:resolve_and_record()`; auth layer (R-22) → `grpc/auth_interceptor.rs`
- Redis fenced client, MhAssignmentStore, `get_mh_assignment` → `crates/mc-service/src/redis/client.rs`; Lua → `redis/lua_scripts.rs`
- Health, system info → `crates/mc-service/src/observability/health.rs`, `crates/mc-service/src/system_info.rs`
- Metric wrappers + Cat B matrix (`KEY_CUSTODY_LABEL` on media emissions) → `crates/mc-service/src/observability/metrics.rs`; catalog → `docs/observability/metrics/mc-service.md`; alert inventory → `docs/observability/alerts.md`

## Protocols
- Client signaling (join, mute, server mute, ReceiveCapability, SendDirective, StreamAssignments, KEK) → `proto/dark_tower/signaling/v1/signaling.proto`
- Internal RPCs (RegisterMc, AssignMeeting, MediaCoordinationService, RegisterMeeting, EndMeeting) → `proto/dark_tower/internal/v1/internal.proto`
- MC→AC: token management → `crates/common/src/token_manager.rs`; JWKS → `crates/common/src/jwt.rs:JwksClient`

## Testing
- Shared bring-up + mock MH stores → `crates/mc-service/tests/common/mod.rs`; media session harness → `tests/common/media_session.rs`; accept-loop rig → `tests/common/accept_loop_rig.rs`
- Join flow → `crates/mc-service/tests/join_tests.rs`; client media signalling → `tests/media_client_signaling_integration.rs`; slots/edges/connectivity → `tests/slot_placement_integration.rs`
- Admission → `crates/mc-service/tests/media_admission_integration.rs`; KEK lifecycle → `tests/kek_rotation_integration.rs`; server mute → `tests/server_mute_integration.rs`; teardown → `tests/meeting_teardown_integration.rs`
- Policy push → `crates/mc-service/tests/media_policy_push_integration.rs`; RegisterMeeting → `tests/register_meeting_integration.rs`; MH coordination → `tests/media_coordination_integration.rs`; disconnect latency → `tests/disconnect_latency_integration.rs`
- Accept loop → `tests/webtransport_accept_loop_integration.rs`; auth layer → `tests/auth_layer_integration.rs`; actor metrics → `tests/actor_metrics_integration.rs`; Redis → `tests/redis_metrics_integration.rs`; token refresh → `tests/token_refresh_integration.rs`; GC → `tests/gc_integration.rs`; heartbeat → `tests/heartbeat_tasks.rs`; OTel → `tests/otel_grpc_inbound_continuity.rs`
- Test utilities: mock MH gRPC stub → `crates/mc-test-utils/src/mock_mh.rs`; test W → `crates/mc-test-utils/src/kek.rs`; others → `crates/mc-test-utils/src/`
- Env-tests: MC-GC → `crates/env-tests/tests/22_mc_gc_integration.rs`; MH QUIC + policy push → `26_mh_quic.rs`; slots/edges → `27_mc_slot_placement.rs`; KEK rotation → `34_mc_kek_rotation.rs`; server mute + teardown → `35_mc_server_mute_teardown.rs`; metric hygiene → `32_media_metric_hygiene.rs`
- Env-test fixtures: `Participant` + triage literals → `crates/env-tests/src/fixtures/participant.rs`; `McSession`/`mc_join` → `crates/env-tests/src/fixtures/mc_session.rs`
- Browser multi-party hear-each-other → `packages/web-app/e2e/multi-party-hear.spec.ts`

## Config & Infrastructure
- MC ConfigMap env (REQUIRED media keys, `MC_KEK_ROTATION_DEBOUNCE_SECONDS`) → `infra/services/mc-service/config.env`; cross-service audio sizing inequality → `docs/TODO.md` §Media Path Obligations
- K8s deployments → `infra/services/mc-service/mc-0-deployment.yaml`, `mc-1-deployment.yaml`; network policy → `infra/services/mc-service/network-policy.yaml`
- Dashboards → `infra/grafana/dashboards/mc-overview.json`, `infra/grafana/dashboards/mc-media.json`; alert rules → `infra/docker/prometheus/rules/mc-alerts.yaml`
- Runbooks → `docs/runbooks/mc-incident-response.md` (Sc 12–21), `docs/runbooks/mc-deployment.md`
