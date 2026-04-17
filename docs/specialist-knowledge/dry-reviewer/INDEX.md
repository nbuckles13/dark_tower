# DRY Reviewer Navigation

## Architecture & Design
- Blocking vs tech-debt classification -> ADR-0019 (`docs/decisions/adr-0019-dry-reviewer.md`)
- Fix-or-defer review model -> ADR-0024 (`docs/decisions/adr-0024-agent-teams-workflow.md`)

## JWT Validation (Common + Thin Wrappers)
- Common JWT code (all shared logic) -> `crates/common/src/jwt.rs`
- Internal token request types (GC->AC contract) -> `crates/common/src/meeting_token.rs`
- GC thin wrapper -> `crates/gc-service/src/auth/jwt.rs` (ServiceClaims, UserClaims)
- MC thin wrapper -> `crates/mc-service/src/auth/mod.rs` (MeetingTokenClaims, GuestTokenClaims)
- MH JWKS config -> `infra/services/mh-service/configmap.yaml:AC_JWKS_URL` (shared configmap, mirrors MC pattern in `mc-service-config`)

## Per-Service Observability (Metrics & Dashboards)
- AC/GC/MC/MH metrics -> `crates/*/src/observability/metrics.rs` (per-service, not duplication)
- Alert rules -> `infra/docker/prometheus/rules/{mc,gc}-alerts.yaml`; dashboards -> ADR-0029, `infra/grafana/dashboards/`

## Integration Test Coverage
- MC join flow -> `crates/mc-service/tests/join_tests.rs`; GC join/guest/settings -> `crates/gc-service/tests/meeting_tests.rs`
- MH tests (gc_integration, mc_client_integration, auth_layer_integration, register_meeting_integration, webtransport_integration) -> `crates/mh-service/tests/`
- MH shared test rigs (TestKeypair, mock_mc, jwks_rig, grpc_rig, wt_rig, wt_client, tokens) -> `crates/mh-service/tests/common/`
- Shared fixtures (cross-service) -> `crates/mc-test-utils/src/jwt_test.rs`, `crates/gc-test-utils/src/server_harness.rs`

## Per-Service Config Parsing
- AC/GC/MC/MH config -> `crates/*/src/config.rs:Config::from_vars()` (per-service); ordinal parsing -> `crates/common/src/config.rs:parse_statefulset_ordinal()`
- Extraction candidate: `generate_instance_id(prefix)` -> 4-line pattern in GC + MC + MH config

## gRPC Auth (Cross-Service)
- MC auth layer (async JWKS, R-22) -> `crates/mc-service/src/grpc/auth_interceptor.rs:McAuthLayer` (applied in main.rs)
- MC auth interceptor (legacy structural) -> same file `:McAuthInterceptor` (dead in production)
- MH auth layer (async JWKS) -> `crates/mh-service/src/grpc/auth_interceptor.rs:MhAuthLayer`
- MH auth interceptor (legacy structural) -> same file `:MhAuthInterceptor` (dead in production)
- Shared constant -> `common::jwt::MAX_JWT_SIZE_BYTES`
- McAuthLayer/MhAuthLayer are near-identical tower Layer/Service patterns (extraction candidate in TODO.md)

## MC gRPC Services (GC→MC + MH→MC + MC→MH)
- MC assignment service (GC→MC) -> `crates/mc-service/src/grpc/mc_service.rs:McAssignmentService`
- MC media coordination (MH→MC, R-15) -> `crates/mc-service/src/grpc/media_coordination.rs:McMediaCoordinationService`
- MH connection registry -> `crates/mc-service/src/mh_connection_registry.rs:MhConnectionRegistry`
- MAX_ID_LENGTH constant -> `mh_connection_registry.rs` (single source, imported by media_coordination.rs)
- MhRegistrationClient trait (testability seam) -> `crates/mc-service/src/grpc/mh_client.rs:MhRegistrationClient`
- Async RegisterMeeting trigger (R-12) -> `crates/mc-service/src/webtransport/connection.rs:register_meeting_with_handlers()`

## gRPC Clients (Cross-Service)
- MC GcClient -> `crates/mc-service/src/grpc/gc_client.rs:GcClient` (bounded retries, fast/comprehensive heartbeats)
- MC MhClient -> `crates/mc-service/src/grpc/mh_client.rs:MhClient` (per-call channels, no retries)
- MH GcClient -> `crates/mh-service/src/grpc/gc_client.rs:GcClient` (unbounded retries, load reports)
- Shared patterns: channel creation, `add_auth`, backoff constants (acceptable structural similarity)
- Extraction candidate: `add_auth` (~10 lines identical, 3 call sites) -> see TODO.md
- MH gRPC stub service -> `crates/mh-service/src/grpc/mh_service.rs:MhMediaService`

## Redis Abstractions (MC)
- MhAssignmentStore trait -> `crates/mc-service/src/redis/client.rs:MhAssignmentStore` (testability seam for join flow)
- FencedRedisClient -> `crates/mc-service/src/redis/client.rs:FencedRedisClient` (fenced writes, implements MhAssignmentStore)

## Health Endpoints (Cross-Service Consistency)
- MC health -> `crates/mc-service/src/observability/health.rs:health_router()` | MH -> `crates/mh-service/src/observability/health.rs` (duplicated, see TODO.md)
- GC health -> `crates/gc-service/src/routes/mod.rs:64-65`

## Per-Service Infrastructure (K8s, Docker, Kind)
- Kustomize bases -> `infra/services/{ac,gc,mc,mh}-service/kustomization.yaml`
- MC/MH per-pod Services + ConfigMaps -> `infra/services/{mc,mh}-service/` (port: `base + ordinal*2`)
- Network policies -> `infra/services/{ac,gc,mc,mh}-service/network-policy.yaml`
- Dockerfiles -> `infra/docker/{ac,gc,mc,mh}-service/Dockerfile`; Kind -> `infra/kind/`
- setup/teardown (ADR-0030) -> `infra/kind/scripts/{setup,teardown}.sh`; devloop -> `infra/devloop/devloop.sh`

## False Positive Boundaries
- Per-service error mapping (GcError vs McError vs MhError) -> required, not duplication
- MC GcClient vs MH GcClient -> different RPCs, retry strategies, heartbeat models
- AC rate limiting (DB-backed lockout) vs GC rate limiting (middleware RPM) -> different mechanisms
- common::jwt enums vs common::meeting_token -> JWT enums intentionally narrower

## Tech Debt & Extractions
- Active duplication tech debt -> `docs/TODO.md` (Cross-Service Duplication section)
- Common crate + test fixtures -> `crates/common/src/`, `crates/mc-test-utils/`, `crates/gc-test-utils/`
