# Code Reviewer Navigation

## Architecture & Design
- Actor handle/task separation → ADR-0001 (Section: Pattern)
- No-panic policy, `#[expect]` over `#[allow]` → ADR-0002
- Error handling, service-layer wrapping → ADR-0003
- Host-side cluster helper → ADR-0030
- Observability naming, label cardinality, SLO targets → ADR-0011
- Dashboard metric presentation (counters vs rates, $__rate_interval) → ADR-0029
- Guard pipeline methodology → ADR-0015
- DRY cross-service duplication → ADR-0019
- User auth, three-tier token architecture → ADR-0020
- Infrastructure architecture, K8s manifests → ADR-0012; Local dev environment → ADR-0013
- Agent teams validation pipeline → ADR-0024

## Code Locations — AC Service
- Clippy deny list → `Cargo.toml:34-42`
- Config (rate limits, defense-in-depth) → `crates/ac-service/src/config.rs:from_vars()`, constants at `:32-61`
- Crypto (EdDSA, AES-256-GCM, bcrypt) → `crates/ac-service/src/crypto/mod.rs:sign_jwt()`
- Error type → `crates/ac-service/src/errors.rs:AcError`
- Handlers/routes → `handlers/auth_handler.rs:handle_service_token()`, `routes/mod.rs:build_routes()`
- Metrics → `crates/ac-service/src/observability/metrics.rs:init_metrics_recorder()`
- Repository + service layers → `repositories/signing_keys.rs`, `services/key_management_service.rs`
- K8s wiring → `infra/services/ac-service/configmap.yaml`, `statefulset.yaml`

## Code Locations — GC Service
- Error type, `From<JwtError>` → `crates/gc-service/src/errors.rs:GcError`
- Auth (JWT/JWKS, middleware) → `auth/jwt.rs`, `jwks.rs`, `middleware/auth.rs:require_user_auth()`
- Meeting handlers → `handlers/meetings.rs:create_meeting()`, `join_meeting()`, `get_guest_token()`, `JoinMeetingResponse::new()`
- Repositories → `repositories/meetings.rs` (atomic CTE), `participants.rs`
- AC/MC clients → `services/ac_client.rs:AcClient`, `mc_client.rs:McClientTrait`
- Metrics/dashboard/alerts → `observability/metrics.rs`, `docs/observability/metrics/gc-service.md`, `infra/grafana/dashboards/gc-overview.json`

## Code Locations — MC Service
- Error type (McError, bounded labels, From<JwtError>, MhAssignmentMissing) → `crates/mc-service/src/errors.rs`
- Auth: JWT validator + token type enforcement → `crates/mc-service/src/auth/mod.rs:McJwtValidator`; interceptor → `grpc/auth_interceptor.rs:McAuthInterceptor`; auth layer (async JWKS, no scope — deferred to handlers) → `grpc/auth_interceptor.rs:McAuthLayer`
- MH gRPC client (Channel-per-call, RegisterMeeting RPC) → `grpc/mh_client.rs:MhClient`; trait → `mh_client.rs:MhRegistrationClient`
- MediaCoordinationService (MH→MC notifications, R-15) → `grpc/media_coordination.rs:McMediaCoordinationService`
- MH connection registry (participant→MH tracking, RwLock) → `mh_connection_registry.rs:MhConnectionRegistry`
- Config (ac_jwks_url, advertise addresses, ordinal parsing) → `crates/mc-service/src/config.rs:Config`, `parse_statefulset_ordinal()`
- Startup wiring (JwksClient, McJwtValidator, McAuthLayer, MediaCoordinationService, registry) → `crates/mc-service/src/main.rs`
- Redis (MhAssignmentData, MhAssignmentStore trait, FencedRedisClient) → `crates/mc-service/src/redis/client.rs`
- WebTransport: server (accept loop, redis+mh_client injection) → `webtransport/server.rs:WebTransportServer::accept_loop()`; join flow → `connection.rs:handle_connection()`, `build_join_response()`; async RegisterMeeting trigger (first participant, retry+backoff) → `connection.rs:register_meeting_with_handlers()`; post-join (MediaConnectionFailed R-20) → `connection.rs:handle_client_message()`
- MC metrics (join, WebTransport, JWT, register_meeting, MH notifications, media failures, init) → `crates/mc-service/src/observability/metrics.rs`; catalog → `docs/observability/metrics/mc-service.md`
- Dashboard + alerts → `infra/grafana/dashboards/mc-overview.json`, `infra/docker/prometheus/rules/mc-alerts.yaml`
- Health probes + K8s (8081, per-pod NodePort) → `observability/health.rs:health_router()`, `infra/services/mc-service/`

## Code Locations — MH Service
- Config (ac_jwks_url, max_connections, register_meeting_timeout) → `config.rs:Config`
- Error type (thiserror, bounded labels) → `errors.rs:MhError`
- Auth: JWT validator → `auth/mod.rs:MhJwtValidator`; interceptor → `grpc/auth_interceptor.rs:MhAuthInterceptor`; auth layer (async JWKS, scope `service.write.mh`) → `grpc/auth_interceptor.rs:MhAuthLayer`
- GC client (RegisterMH, SendLoadReport) → `grpc/gc_client.rs:GcClient`
- MC client (MH→MC: NotifyParticipantConnected/Disconnected, per-call channel, retry) → `grpc/mc_client.rs:McClient`
- MC client integration tests → `tests/mc_client_integration.rs`
- gRPC stub service (MC→MH: RegisterMeeting) → `grpc/mh_service.rs:MhMediaService`
- Session manager (registered meetings, pending connections, Notify) → `session/mod.rs:SessionManager`
- WebTransport: server (TLS, capacity, McClient) → `webtransport/server.rs:WebTransportServer`; connection (JWT, provisional accept, MC notifications) → `webtransport/connection.rs:handle_connection()`, `spawn_notify_connected()`; provisional-accept select helper (Registered/Timeout/Cancelled outcomes, owns cleanup + metric) → `webtransport/connection.rs:await_meeting_registration()`
- Startup wiring (JWKS, SessionManager, WebTransport, MhAuthLayer, McClient) → `main.rs`
- Metrics (JWT, WebTransport, handshake, connections, MC notifications, RegisterMeeting timeouts) → `observability/metrics.rs:record_mc_notification()`, `:record_register_meeting_timeout()`; catalog → `docs/observability/metrics/mh-service.md`; timeout fire site → `webtransport/connection.rs:await_meeting_registration()` (timeout arm only, invariant enforced by co-located behavioral tests)
- Health probes (port 8083) → `observability/health.rs`; K8s → `infra/services/mh-service/`
- Dockerfile → `infra/docker/mh-service/Dockerfile`; NetworkPolicy → `infra/services/mh-service/network-policy.yaml`

## Code Locations — Common
- JWT (errors, claims, validator, JWKS, HasIat) → `crates/common/src/jwt.rs`
- SecretString/SecretBox → `crates/common/src/secret.rs`
- TokenManager → `crates/common/src/token_manager.rs:spawn_token_manager()`
- Meeting token shared types (GC↔AC contract, ADR-0020) → `crates/common/src/meeting_token.rs`

## Infrastructure & Guards
- Standard health endpoints (`/health`, `/ready`) → ADR-0012 (Section: Standard Operational Endpoints)
- MC+MH TLS cert generation → `scripts/generate-dev-certs.sh`
- Env-tests cluster module → `crates/env-tests/src/cluster.rs`
- Kind cluster (ADR-0030): `kind-config.yaml.tmpl`, `setup.sh` (`deploy_only_service()`, `DT_HOST_GATEWAY_IP`), `{mc,mh}-{0,1}-configmap.yaml`
- Devloop helper → `crates/devloop-helper/src/commands.rs`; client → `infra/devloop/dev-cluster`; Service bases → `infra/services/*/kustomization.yaml`
- Guards: runner → `scripts/guards/run-guards.sh`; Kustomize (R-15–R-20) → `validate-kustomize.sh`; App metrics → `validate-application-metrics.sh`
