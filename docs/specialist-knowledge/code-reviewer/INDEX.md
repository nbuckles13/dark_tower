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
- MH selection (active/active peers) → `services/mh_selection.rs:MhSelectionService`, `MhSelection { handlers: Vec<MhAssignmentInfo> }`
- MC assignment with MH → `services/mc_assignment.rs:McAssignmentService::assign_meeting_with_mh()`, `AssignmentWithMh`
- Metrics/dashboard/alerts → `observability/metrics.rs`, `docs/observability/metrics/gc-service.md`, `infra/grafana/dashboards/gc-overview.json`

## Code Locations — MC Service
- Error type (McError, bounded labels, From<JwtError>) → `crates/mc-service/src/errors.rs`
- JWT validator + token type enforcement → `crates/mc-service/src/auth/mod.rs:McJwtValidator`
- gRPC auth interceptor (structural) → `crates/mc-service/src/grpc/auth_interceptor.rs:McAuthInterceptor`
- Config (ac_jwks_url, advertise addresses, ordinal parsing) → `crates/mc-service/src/config.rs:Config`, `parse_statefulset_ordinal()`
- Startup wiring (JwksClient + McJwtValidator) → `crates/mc-service/src/main.rs:168-189`
- WebTransport server (accept loop) → `crates/mc-service/src/webtransport/server.rs:WebTransportServer::accept_loop()`
- Connection handler (join flow) → `crates/mc-service/src/webtransport/connection.rs:handle_connection()`
- MC metrics (join, WebTransport, JWT, init) → `crates/mc-service/src/observability/metrics.rs`
- Dashboard + alerts (join panels, Traffic/Security stat rows) → `infra/grafana/dashboards/mc-overview.json`, `infra/docker/prometheus/rules/mc-alerts.yaml`
- Metrics catalog → `docs/observability/metrics/mc-service.md`
- Health probes (liveness/readiness) → `crates/mc-service/src/observability/health.rs:health_router()`
- K8s (probes on 8081, per-pod NodePort) → `infra/services/mc-service/statefulset.yaml`, `service.yaml`, `network-policy.yaml`

## Code Locations — MH Service
- Config (env vars, SecretString, Debug redaction, advertise addresses, ordinal parsing) → `crates/mh-service/src/config.rs:Config`, `parse_statefulset_ordinal()`
- Error type (thiserror, bounded labels) → `crates/mh-service/src/errors.rs:MhError`
- GC client (RegisterMH, SendLoadReport, re-registration) → `crates/mh-service/src/grpc/gc_client.rs:GcClient`
- gRPC stub service (MC→MH: Register, RouteMedia, StreamTelemetry) → `crates/mh-service/src/grpc/mh_service.rs:MhMediaService`
- gRPC auth interceptor (structural validation) → `crates/mh-service/src/grpc/auth_interceptor.rs:MhAuthInterceptor`
- Startup wiring (TokenManager, health, gRPC, GC task) → `crates/mh-service/src/main.rs`
- Health probes (liveness/readiness, port 8083) → `crates/mh-service/src/observability/health.rs:health_router()`
- Metrics (mh_ prefix, SLO-aligned buckets) → `crates/mh-service/src/observability/metrics.rs:init_metrics_recorder()`
- Metrics catalog + dashboard → `docs/observability/metrics/mh-service.md`, `infra/grafana/dashboards/mh-overview.json`
- K8s StatefulSet (probes on 8083, TLS vol, UDP 4434, per-pod NodePort) → `infra/services/mh-service/statefulset.yaml`, `service.yaml`
- Dockerfile (cargo-chef, protobuf-compiler, distroless) → `infra/docker/mh-service/Dockerfile`
- NetworkPolicy (MC gRPC ingress, client UDP, GC/MC/AC egress) → `infra/services/mh-service/network-policy.yaml`

## Code Locations — Common
- JWT (errors, claims, validator, JWKS, HasIat) → `crates/common/src/jwt.rs`
- SecretString/SecretBox → `crates/common/src/secret.rs`
- TokenManager → `crates/common/src/token_manager.rs:spawn_token_manager()`
- Meeting token shared types (GC↔AC contract, ADR-0020) → `crates/common/src/meeting_token.rs`

## Infrastructure & Guards
- Standard health endpoints (`/health`, `/ready`) → ADR-0012 (Section: Standard Operational Endpoints)
- MC+MH TLS cert generation → `scripts/generate-dev-certs.sh`
- Env-tests cluster module → `crates/env-tests/src/cluster.rs`
- Kind cluster (ADR-0030): `kind-config.yaml.tmpl`, `setup.sh`, `{mc,mh}-{0,1}-configmap.yaml`; Devloop → `devloop-helper/src/commands.rs`
- Service bases + Kind overlay → `infra/services/*/kustomization.yaml`, `infra/kubernetes/overlays/kind/`; Guards → `scripts/guards/run-guards.sh`
