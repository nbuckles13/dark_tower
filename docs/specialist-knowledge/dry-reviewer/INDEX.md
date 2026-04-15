# DRY Reviewer Navigation

## Architecture & Design
- Blocking vs tech-debt classification -> ADR-0019 (`docs/decisions/adr-0019-dry-reviewer.md`)
- Fix-or-defer review model -> ADR-0024 (`docs/decisions/adr-0024-agent-teams-workflow.md`)

## JWT Validation (Common + Thin Wrappers)
- Common JWT code -> `crates/common/src/jwt.rs` | Token types -> `crates/common/src/meeting_token.rs`
- GC thin wrapper -> `crates/gc-service/src/auth/jwt.rs` | MC -> `crates/mc-service/src/auth/mod.rs`
- MH JWKS config -> `infra/services/mh-service/configmap.yaml:AC_JWKS_URL`

## Per-Service Observability (Metrics & Dashboards)
- AC/GC/MC/MH metrics -> `crates/*/src/observability/metrics.rs` (per-service, not duplication)
- Alert rules -> `infra/docker/prometheus/rules/{mc,gc}-alerts.yaml` | Dashboards -> `infra/grafana/dashboards/` | ADR-0029

## Integration Test Coverage
- MC join flow (11 tests) -> `crates/mc-service/tests/join_tests.rs`
- GC join/guest/settings -> `crates/gc-service/tests/meeting_tests.rs`
- MH GC integration -> `crates/mh-service/tests/gc_integration.rs`
- MH MC notification integration -> `crates/mh-service/tests/mc_client_integration.rs`
- Shared fixtures -> `crates/mc-test-utils/src/jwt_test.rs`, `crates/gc-test-utils/src/server_harness.rs`

## Per-Service Config Parsing
- AC/GC/MC/MH config -> `crates/*/src/config.rs:Config::from_vars()` (per-service, not duplication)
- Advertise addresses (MC + MH) -> per-instance ConfigMaps; devloop patches via `setup.sh` DT_HOST_GATEWAY_IP guard
- StatefulSet ordinal parsing -> `crates/common/src/config.rs:parse_statefulset_ordinal()` (shared)
- Extraction candidate: `generate_instance_id(prefix)` -> 4-line pattern in GC + MC + MH config

## gRPC Auth (Cross-Service)
- MC auth -> `crates/mc-service/src/grpc/auth_interceptor.rs:McAuthLayer` (JWKS, R-22) + `:McAuthInterceptor` (legacy, dead)
- MH auth -> `crates/mh-service/src/grpc/auth_interceptor.rs:MhAuthLayer` (JWKS) + `:MhAuthInterceptor` (legacy, dead)
- Shared constant -> `common::jwt::MAX_JWT_SIZE_BYTES`
- McAuthLayer/MhAuthLayer are near-identical tower Layer/Service patterns (extraction candidate in TODO.md)

## MC gRPC Services (GC→MC + MH→MC)
- MC assignment service (GC→MC) -> `crates/mc-service/src/grpc/mc_service.rs:McAssignmentService`
- MC media coordination (MH→MC, R-15) -> `crates/mc-service/src/grpc/media_coordination.rs:McMediaCoordinationService`
- MH connection registry -> `crates/mc-service/src/mh_connection_registry.rs:MhConnectionRegistry`
- MAX_ID_LENGTH constant -> `mh_connection_registry.rs` (single source, imported by media_coordination.rs)

## gRPC Clients (Cross-Service)
- MC GcClient -> `crates/mc-service/src/grpc/gc_client.rs:GcClient` (bounded retries, fast/comprehensive heartbeats)
- MC MhClient -> `crates/mc-service/src/grpc/mh_client.rs:MhClient` (per-call channels, no retries)
- MH GcClient -> `crates/mh-service/src/grpc/gc_client.rs:GcClient` (unbounded retries, load reports)
- MH McClient -> `crates/mh-service/src/grpc/mc_client.rs:McClient` (per-call channels, 3-attempt retry, best-effort)
- Shared patterns: channel creation, `add_auth`, backoff constants (acceptable structural similarity)
- Extraction candidate: `add_auth` (~10 lines identical, 4 call sites: MC GcClient, MC MhClient, MH GcClient, MH McClient) -> extract crate-local in each crate's `grpc/mod.rs`; `common` when third crate needs it
- MH gRPC stub service -> `crates/mh-service/src/grpc/mh_service.rs:MhMediaService`

## Redis Abstractions (MC)
- MhAssignmentStore trait -> `crates/mc-service/src/redis/client.rs:MhAssignmentStore` (testability seam for join flow)
- FencedRedisClient -> `crates/mc-service/src/redis/client.rs:FencedRedisClient` (fenced writes, implements MhAssignmentStore)

## Health Endpoints (Cross-Service Consistency)
- MC health routes -> `crates/mc-service/src/observability/health.rs:health_router()`
- MH health routes -> `crates/mh-service/src/observability/health.rs:health_router()` (duplicates MC)
- GC health routes -> `crates/gc-service/src/routes/mod.rs:64-65`

## Per-Service Infrastructure (K8s, Docker, Kind)
- Kustomize bases -> `infra/services/{ac,gc,mc,mh}-service/kustomization.yaml`; MC/MH per-pod Services + ConfigMaps
- Network policies -> `infra/services/{ac,gc,mc,mh}-service/network-policy.yaml`; MH↔MC on TCP 50052
- Dockerfiles -> `infra/docker/{ac,gc,mc,mh}-service/Dockerfile`; Kind -> `infra/kind/`; setup/teardown -> `infra/kind/scripts/`

## False Positive Boundaries
- Per-service error mapping (GcError vs McError vs MhError) -> required, not duplication
- MC GcClient vs MH GcClient -> different RPCs, retry strategies, heartbeat models (reviewed 2026-04-01)
- MH McClient vs MC MhClient -> channel-per-call is justified (different endpoints per meeting); McClient adds retry + auth short-circuit
- AC rate limiting (DB-backed lockout) vs GC rate limiting (middleware RPM) -> different mechanisms
- common::jwt::{ParticipantType,MeetingRole} vs common::meeting_token -> JWT enums intentionally narrower
- env-tests GuestTokenRequest vs common::meeting_token::GuestTokenRequest -> different types

## Tech Debt & Extractions
- Active duplication tech debt -> `docs/TODO.md` (Cross-Service Duplication section)
- Common crate (extraction target) -> `crates/common/src/` (jwt, config, meeting_token, secret, token_manager)
- Test fixtures -> `crates/mc-test-utils/src/jwt_test.rs`, `crates/gc-test-utils/`
