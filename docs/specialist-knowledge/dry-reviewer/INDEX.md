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
- MC/GC join alert rules -> `infra/docker/prometheus/rules/{mc,gc}-alerts.yaml`
- Dashboard metric presentation -> ADR-0029
- Grafana dashboards + configMapGenerator -> `infra/grafana/dashboards/`, `infra/kubernetes/observability/grafana/`

## Integration Test Coverage
- MC join flow (11 tests) -> `crates/mc-service/tests/join_tests.rs`
- GC join/guest/settings -> `crates/gc-service/tests/meeting_tests.rs`
- MH GC integration -> `crates/mh-service/tests/gc_integration.rs`
- Shared fixtures -> `crates/mc-test-utils/src/jwt_test.rs`, `crates/gc-test-utils/src/server_harness.rs`

## Per-Service Config Parsing
- AC/GC/MC/MH config -> `crates/*/src/config.rs:Config::from_vars()` (per-service, not duplication)
- Env-test cluster config -> `crates/env-tests/src/cluster.rs:ClusterPorts::from_env()`, `parse_host_port()` (different domain from service config, not duplication)
- Advertise addresses (MC + MH) -> gRPC uses POD_IP downward API; WebTransport uses per-instance ConfigMap (`mc-{0,1}-config`, `mh-{0,1}-config`); devloop patches via `setup.sh` DT_HOST_GATEWAY_IP guard
- StatefulSet ordinal parsing -> `crates/common/src/config.rs:parse_statefulset_ordinal()` (shared, 5 tests)
- Extraction candidate: `generate_instance_id(prefix)` -> 4-line pattern duplicated in GC + MC + MH config

## gRPC Auth Interceptors (Cross-Service)
- MC auth interceptor -> `crates/mc-service/src/grpc/auth_interceptor.rs:McAuthInterceptor`
- MH auth interceptor -> `crates/mh-service/src/grpc/auth_interceptor.rs:MhAuthInterceptor` (duplicates MC)
- Shared constant -> `common::jwt::MAX_JWT_SIZE_BYTES`

## gRPC Clients (Cross-Service)
- MC GcClient -> `crates/mc-service/src/grpc/gc_client.rs:GcClient` (bounded retries, fast/comprehensive heartbeats)
- MC MhClient -> `crates/mc-service/src/grpc/mh_client.rs:MhClient` (per-call channels, no retries)
- MH GcClient -> `crates/mh-service/src/grpc/gc_client.rs:GcClient` (unbounded retries, load reports)
- Shared patterns: channel creation, `add_auth`, backoff constants (acceptable structural similarity)
- Extraction candidate: `add_auth` (~10 lines identical, 3 call sites: MC GcClient, MC MhClient, MH GcClient) -> extract crate-local in MC first; `common` when third crate needs it
- MH gRPC stub service -> `crates/mh-service/src/grpc/mh_service.rs:MhMediaService`

## Redis Abstractions (MC)
- MhAssignmentStore trait -> `crates/mc-service/src/redis/client.rs:MhAssignmentStore` (testability seam for join flow)
- FencedRedisClient -> `crates/mc-service/src/redis/client.rs:FencedRedisClient` (fenced writes, implements MhAssignmentStore)

## Health Endpoints (Cross-Service Consistency)
- MC health routes -> `crates/mc-service/src/observability/health.rs:health_router()`
- MH health routes -> `crates/mh-service/src/observability/health.rs:health_router()` (duplicates MC)
- GC health routes -> `crates/gc-service/src/routes/mod.rs:64-65`

## Per-Service Infrastructure (K8s, Docker, Kind)
- Kustomize bases -> `infra/services/{ac,gc,mc,mh}-service/kustomization.yaml`; AC/MC/MH StatefulSet, GC Deployment
- MC/MH per-pod Services + per-instance ConfigMaps -> `infra/services/{mc,mh}-service/` (port formula: `base + ordinal*2`)
- Network policies → `infra/services/*/network-policy.yaml`; MH->MC egress + MC<-MH ingress on TCP 50052
- Dockerfiles -> `infra/docker/*/Dockerfile` (cargo-chef multi-stage); Kind -> `infra/kind/`; overlays -> `infra/kubernetes/overlays/kind/`
- setup/teardown (ADR-0030) -> `infra/kind/scripts/{setup,teardown}.sh`; devloop -> `infra/devloop/devloop.sh`
- Helper commands -> `crates/devloop-helper/src/commands.rs`; TLS -> `scripts/generate-dev-certs.sh`

## False Positive Boundaries
- Per-service error mapping (GcError vs McError vs MhError) -> required, not duplication
- MC GcClient vs MH GcClient -> different RPCs, retry strategies, heartbeat models (reviewed 2026-04-01)
- AC rate limiting (DB-backed lockout) vs GC rate limiting (middleware RPM) -> different mechanisms
- common::jwt::{ParticipantType,MeetingRole} (2 variants) vs common::meeting_token (3 variants) -> JWT enums intentionally narrower (no Guest; guests use separate GuestTokenClaims)
- env-tests GuestTokenRequest vs common::meeting_token::GuestTokenRequest -> different types (public API vs internal)

## Tech Debt Registry
- Active duplication tech debt -> `docs/TODO.md` (Cross-Service Duplication section)

## Successful Extractions & Integration Seams
- Common crate (extraction target) -> `crates/common/src/` (jwt, config, meeting_token, secret, token_manager); ServiceClaims/UserClaims/JWKS/JwtValidator -> `jwt.rs`
- TestKeypair + JWKS mock -> `crates/mc-test-utils/src/jwt_test.rs`; MeetingToken types -> `common/src/meeting_token.rs`; StatefulSet ordinal -> `common/src/config.rs`; JoinMeetingResponse -> `crates/gc-service/src/handlers/meetings.rs`; load_image_to_kind -> `infra/kind/scripts/setup.sh`
