# DRY Reviewer Navigation

## Architecture & Design
- Blocking vs tech-debt classification -> ADR-0019 (`docs/decisions/adr-0019-dry-reviewer.md`)
- Fix-or-defer review model -> ADR-0024 (`docs/decisions/adr-0024-agent-teams-workflow.md`)

## Proto (Signaling + Internal)
- Signaling proto -> `proto/signaling.proto` (MediaServerInfo, MediaConnectionFailed, ClientMessage)
- Internal proto -> `proto/internal.proto` (MhAssignment, RegisterMeeting, MediaCoordinationService, DisconnectReason)
- Proto codegen -> `crates/proto-gen/build.rs`, `crates/proto-gen/src/lib.rs`

## JWT Validation (Common + Thin Wrappers)
- Common JWT code (all shared logic) -> `crates/common/src/jwt.rs`
- Internal token request types (GC->AC contract) -> `crates/common/src/meeting_token.rs`
- GC thin wrapper -> `crates/gc-service/src/auth/jwt.rs` (ServiceClaims, UserClaims)
- MC thin wrapper -> `crates/mc-service/src/auth/mod.rs` (MeetingTokenClaims, GuestTokenClaims)

## Per-Service Observability (Metrics & Dashboards)
- AC/GC/MC/MH metrics -> `crates/*/src/observability/metrics.rs` (per-service, not duplication)
- Alert rules -> `infra/docker/prometheus/rules/{mc,gc}-alerts.yaml`; dashboards -> `infra/grafana/dashboards/`; ADR-0029

## Integration Test Coverage
- MC join flow (11 tests) -> `crates/mc-service/tests/join_tests.rs`
- GC join/guest/settings -> `crates/gc-service/tests/meeting_tests.rs`
- MH GC integration -> `crates/mh-service/tests/gc_integration.rs`
- Shared fixtures -> `crates/mc-test-utils/src/jwt_test.rs`, `crates/gc-test-utils/src/server_harness.rs`

## Per-Service Config Parsing
- AC/GC/MC/MH config -> `crates/*/src/config.rs:Config::from_vars()` (per-service, not duplication)
- Env-test cluster config -> `crates/env-tests/src/cluster.rs:ClusterPorts::from_env()` (different domain)
- Advertise addresses (MC + MH) -> POD_IP downward API + per-instance ConfigMap; devloop patches via `setup.sh` DT_HOST_GATEWAY_IP
- StatefulSet ordinal -> `crates/common/src/config.rs:parse_statefulset_ordinal()`; extraction candidate: `generate_instance_id(prefix)` (3 services)

## gRPC Auth Interceptors (Cross-Service)
- MC auth interceptor -> `crates/mc-service/src/grpc/auth_interceptor.rs:McAuthInterceptor`
- MH auth interceptor -> `crates/mh-service/src/grpc/auth_interceptor.rs:MhAuthInterceptor` (duplicates MC)
- Shared constant -> `common::jwt::MAX_JWT_SIZE_BYTES`

## GC Clients (MC + MH -> GC Registration)
- MC GcClient -> `crates/mc-service/src/grpc/gc_client.rs:GcClient` (bounded retries, fast/comprehensive heartbeats)
- MH GcClient -> `crates/mh-service/src/grpc/gc_client.rs:GcClient` (unbounded retries, load reports)
- Shared patterns: channel creation, `add_auth`, backoff constants (acceptable duplication, <2 call sites)
- Extraction candidate: `add_auth` (~10 lines identical) -> extract to `common` if third service needs it

## MC<->MH Coordination (gRPC)
- MH gRPC stub service -> `crates/mh-service/src/grpc/mh_service.rs:MhMediaService` (Register, RegisterMeeting, RouteMedia, StreamTelemetry)
- GC MH assignment info -> `crates/gc-service/src/services/mh_selection.rs:MhAssignmentInfo` (mh_id, webtransport_endpoint, grpc_endpoint)
- GC->MC assignment construction -> `crates/gc-service/src/services/mc_client.rs:assign_meeting()` (MhAssignment with grpc_endpoint)
- MC Redis MH data -> `crates/mc-service/src/redis/client.rs:MhAssignmentData`

## Health Endpoints (Cross-Service Consistency)
- MC health routes -> `crates/mc-service/src/observability/health.rs:health_router()`
- MH health routes -> `crates/mh-service/src/observability/health.rs:health_router()` (duplicates MC)
- GC health routes -> `crates/gc-service/src/routes/mod.rs:64-65`

## Per-Service Infrastructure (K8s, Docker, Kind)
- Kustomize bases -> `infra/services/{ac,gc,mc,mh}-service/kustomization.yaml`; Dockerfiles -> `infra/docker/*/Dockerfile`
- MC/MH per-pod Services -> `infra/services/{mc,mh}-service/service.yaml`; Kind config -> `infra/kind/kind-config.yaml{,.tmpl}`
- setup.sh/teardown.sh -> `infra/kind/scripts/`; ConfigMap patching -> `setup.sh:deploy_{mc,mh}_service()`
- devloop helper -> `crates/devloop-helper/src/commands.rs`; dev-cluster display -> `infra/devloop/dev-cluster:display_cluster_info()`
- TLS certs -> `scripts/generate-dev-certs.sh`; Prometheus -> `infra/kubernetes/observability/prometheus-config.yaml`

## False Positive Boundaries
- Per-service error mapping (GcError vs McError vs MhError) -> required, not duplication
- MC GcClient vs MH GcClient -> different RPCs, retry strategies, heartbeat models
- MhJwtValidator vs McJwtValidator -> thin wrappers with per-service error types, not duplication
- AC rate limiting (DB-backed) vs GC rate limiting (middleware RPM) -> different mechanisms
- common::jwt enums (2 variants) vs common::meeting_token (3 variants) -> intentionally narrower

## Tech Debt Registry
- Active duplication tech debt -> `docs/TODO.md` (Cross-Service Duplication section)

## Successful Extractions & Integration Seams
- Common crate -> `crates/common/src/` (jwt, config, meeting_token, secret, token_manager)
- Test fixtures -> `crates/mc-test-utils/src/jwt_test.rs`; JoinMeetingResponse::new() -> `crates/gc-service/src/handlers/meetings.rs`
