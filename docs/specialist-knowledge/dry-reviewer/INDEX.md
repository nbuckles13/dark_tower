# DRY Reviewer Navigation

## Architecture & Design
- Blocking vs tech-debt classification -> ADR-0019 (`docs/decisions/adr-0019-dry-reviewer.md`)
- Fix-or-defer review model -> ADR-0024 (`docs/decisions/adr-0024-agent-teams-workflow.md`)

## JWT Validation (Common + Thin Wrappers)
- Common JWT code (all shared logic) -> `crates/common/src/jwt.rs`
- Internal token request types (GC->AC contract) -> `crates/common/src/meeting_token.rs`
- GC thin wrapper -> `crates/gc-service/src/auth/jwt.rs` (ServiceClaims, UserClaims)
- MC thin wrapper -> `crates/mc-service/src/auth/mod.rs` (MeetingTokenClaims, GuestTokenClaims)

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

## GC Clients (MC + MH -> GC Registration)
- MC GcClient -> `crates/mc-service/src/grpc/gc_client.rs:GcClient` (bounded retries, fast/comprehensive heartbeats)
- MH GcClient -> `crates/mh-service/src/grpc/gc_client.rs:GcClient` (unbounded retries, load reports)
- Shared patterns: channel creation, `add_auth`, backoff constants (acceptable duplication, <2 call sites)
- Extraction candidate: `add_auth` (~10 lines identical) -> extract to `common` if third service needs it
- MH gRPC stub service -> `crates/mh-service/src/grpc/mh_service.rs:MhMediaService`

## Health Endpoints (Cross-Service Consistency)
- MC health routes -> `crates/mc-service/src/observability/health.rs:health_router()`
- MH health routes -> `crates/mh-service/src/observability/health.rs:health_router()` (duplicates MC)
- GC health routes -> `crates/gc-service/src/routes/mod.rs:64-65`

## Per-Service Infrastructure (K8s, Docker, Kind)
- Kustomize bases -> `infra/services/{ac,gc,mc,mh}-service/kustomization.yaml`
- AC/MC/MH use StatefulSet; GC uses Deployment -> `infra/services/*/statefulset.yaml` or `deployment.yaml`
- MC/MH per-pod Services (WebTransport NodePort) -> `infra/services/{mc,mh}-service/service.yaml` (headless + ClusterIP + per-pod-0 + per-pod-1; port formula: `base + ordinal*2`)
- Dockerfiles -> `infra/docker/{ac,gc,mc,mh}-service/Dockerfile` (cargo-chef multi-stage pattern)
- Kind config (static + template) -> `infra/kind/kind-config.yaml`, `kind-config.yaml.tmpl`; overlays -> `infra/kubernetes/overlays/kind/`
- setup.sh + teardown.sh (parameterized, ADR-0030) -> `infra/kind/scripts/{setup,teardown}.sh`; ConfigMap patching -> `setup.sh:deploy_mc_service()`, `deploy_mh_service()`
- Helper commands (setup/rebuild/deploy/teardown/status) -> `crates/devloop-helper/src/commands.rs`; port map -> `write_port_map_shell()`; pod health -> `parse_pod_health()`
- dev-cluster display (shared by setup + status) -> `infra/devloop/dev-cluster:display_cluster_info()`
- devloop.sh infra health check + eager setup -> `infra/devloop/devloop.sh` (Infrastructure health check section)
- Devloop validation Layer 8 (env-tests) -> `.claude/skills/devloop/SKILL.md` (Layer 8 section)
- TLS certs -> `scripts/generate-dev-certs.sh`; Prometheus -> `infra/kubernetes/observability/prometheus-config.yaml`

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
- TestKeypair + JWKS mock -> `crates/mc-test-utils/src/jwt_test.rs`; MeetingToken types -> `common/src/meeting_token.rs`; StatefulSet ordinal -> `common/src/config.rs`
- load_image_to_kind to setup.sh helper -> `infra/kind/scripts/setup.sh:load_image_to_kind()`
