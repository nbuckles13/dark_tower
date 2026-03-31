# Code Reviewer Navigation

## Architecture & Design
- Actor handle/task separation → ADR-0001 (Section: Pattern)
- No-panic policy, `#[expect]` over `#[allow]` → ADR-0002
- Error handling, service-layer wrapping → ADR-0003
- Observability naming, label cardinality, SLO targets → ADR-0011
- Guard pipeline methodology → ADR-0015
- DRY cross-service duplication → ADR-0019
- User auth, three-tier token architecture → ADR-0020
- Infrastructure architecture, K8s manifests → ADR-0012; Local dev environment → ADR-0013
- Agent teams validation pipeline → ADR-0024

## Code Locations — AC Service
- Clippy deny list (unwrap, expect, panic, indexing) → `Cargo.toml:34-42`
- Config constants + defense-in-depth → `crates/ac-service/src/config.rs:from_vars()`
- Crypto (EdDSA, AES-256-GCM, bcrypt) → `crates/ac-service/src/crypto/mod.rs:sign_jwt()`
- Error type reference → `crates/ac-service/src/errors.rs:AcError`
- Handler pattern (auth flow) → `crates/ac-service/src/handlers/auth_handler.rs:handle_service_token()`
- Metrics wiring reference → `crates/ac-service/src/observability/metrics.rs:init_metrics_recorder()`
- Route composition → `crates/ac-service/src/routes/mod.rs:build_routes()`
- Repository layer (sqlx queries) → `crates/ac-service/src/repositories/signing_keys.rs`
- Service layer (business logic) → `crates/ac-service/src/services/key_management_service.rs`

## Code Locations — GC Service
- Error type reference → `crates/gc-service/src/errors.rs:GcError`
- `From<JwtError> for GcError` → `crates/gc-service/src/errors.rs`
- JWT validator/JWKS client (thin wrappers) → `crates/gc-service/src/auth/jwt.rs`, `jwks.rs`
- Auth middleware → `crates/gc-service/src/middleware/auth.rs:require_user_auth()`, `extract_bearer_token()`
- Meeting handlers → `crates/gc-service/src/handlers/meetings.rs:create_meeting()`, `join_meeting()`, `get_guest_token()`, `update_meeting_settings()`
- Meetings repository (atomic CTE, activation, audit) → `crates/gc-service/src/repositories/meetings.rs`
- Participants repo/model/tests → `crates/gc-service/src/repositories/participants.rs`, `models/mod.rs`, `tests/participant_tests.rs`
- Meeting join metrics → `crates/gc-service/src/observability/metrics.rs:record_meeting_join()`
- AC/MC clients → `crates/gc-service/src/services/ac_client.rs:AcClient`, `mc_client.rs:McClientTrait`
- Route composition (user auth layer) → `crates/gc-service/src/routes/mod.rs:build_routes()`
- Meeting integration tests → `crates/gc-service/tests/meeting_tests.rs`
- Metrics catalog → `docs/observability/metrics/gc-service.md`
- Dashboard (join panels id 35-38) → `infra/grafana/dashboards/gc-overview.json`
- Alert rules (join: GCHighJoinFailureRate, GCHighJoinLatency) → `infra/docker/prometheus/rules/gc-alerts.yaml`

## Code Locations — MC Service
- Error type reference → `crates/mc-service/src/errors.rs:McError`
- Error type labels (bounded cardinality) → `crates/mc-service/src/errors.rs:error_type_label()`
- `From<JwtError> for McError` → `crates/mc-service/src/errors.rs`
- JWT validator + token type enforcement → `crates/mc-service/src/auth/mod.rs:McJwtValidator`
- gRPC auth interceptor (structural) → `crates/mc-service/src/grpc/auth_interceptor.rs:McAuthInterceptor`
- Config (ac_jwks_url) → `crates/mc-service/src/config.rs:Config`
- Startup wiring (JwksClient + McJwtValidator) → `crates/mc-service/src/main.rs:168-189`
- WebTransport server (accept loop) → `crates/mc-service/src/webtransport/server.rs:WebTransportServer::accept_loop()`
- Connection handler (join flow) → `crates/mc-service/src/webtransport/connection.rs:handle_connection()`
- MC metrics (join, WebTransport, JWT, init) → `crates/mc-service/src/observability/metrics.rs`
- Dashboard (join panels id 29-33) → `infra/grafana/dashboards/mc-overview.json`
- Alert rules (join: MCHighJoinFailureRate, MCHighWebTransportRejections, MCHighJwtValidationFailures, MCHighJoinLatency) → `infra/docker/prometheus/rules/mc-alerts.yaml`
- Metrics catalog → `docs/observability/metrics/mc-service.md`
- Health probes (liveness/readiness) → `crates/mc-service/src/observability/health.rs:health_router()`
- MC K8s deployment (probes on port 8081) → `infra/services/mc-service/deployment.yaml`

## Code Locations — Common
- JWT (errors, claims, validator, JWKS, HasIat) → `crates/common/src/jwt.rs`
- SecretString/SecretBox → `crates/common/src/secret.rs`
- TokenManager → `crates/common/src/token_manager.rs:spawn_token_manager()`

## Infrastructure & Guards
- Standard health endpoints (`/health`, `/ready`) → ADR-0012 (Section: Standard Operational Endpoints)
- MC TLS cert generation → `scripts/generate-dev-certs.sh`
- GC K8s deployment (probe reference pattern) → `infra/services/gc-service/deployment.yaml`
- Guard runner → `scripts/guards/run-guards.sh`; Review protocol → `.claude/skills/devloop/review-protocol.md`

## Kustomize Bases
- Per-service bases → `infra/services/{ac,gc,mc}-service/kustomization.yaml`
- Data stores → `infra/services/{postgres,redis}/kustomization.yaml`
- Observability stack → `infra/kubernetes/observability/kustomization.yaml`
- Grafana (extracted from setup.sh) → `infra/kubernetes/observability/grafana/kustomization.yaml`
- Kind setup script → `infra/kind/scripts/setup.sh`
