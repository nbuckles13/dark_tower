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
- Advertise addresses (MC + MH) -> `grpc_advertise_address` + `webtransport_advertise_address` (consistent pattern)
- Extraction candidate: `generate_instance_id(prefix)` -> 4-line pattern duplicated in MC + MH config

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
- Dockerfiles -> `infra/docker/{ac,gc,mc,mh}-service/Dockerfile` (cargo-chef multi-stage pattern)
- Kind overlays -> `infra/kubernetes/overlays/kind/services/{ac,gc,mc,mh}-service/`
- Kind config (port mappings) -> `infra/kind/kind-config.yaml`
- setup.sh deploy functions -> `infra/kind/scripts/setup.sh` (one per service, uses shared `build_image`)
- TLS cert generation -> `scripts/generate-dev-certs.sh` (MC + MH WebTransport certs)
- Prometheus scrape targets -> `infra/kubernetes/observability/prometheus-config.yaml`
- Note: image-load-into-kind pattern repeated per deploy function (pre-existing; candidate for `load_image_to_kind` helper)

## False Positive Boundaries
- Per-service error mapping (GcError vs McError vs MhError) -> required, not duplication
- MC GcClient vs MH GcClient -> different RPCs, retry strategies, heartbeat models (reviewed 2026-04-01)
- AC rate limiting (DB-backed lockout) vs GC rate limiting (middleware RPM) -> different mechanisms
- common::jwt::{ParticipantType,MeetingRole} (2 variants) vs common::meeting_token (3 variants) -> JWT enums intentionally narrower (no Guest; guests use separate GuestTokenClaims)
- env-tests GuestTokenRequest vs common::meeting_token::GuestTokenRequest -> different types (public API client vs internal GC->AC request)
- Per-service K8s manifests/Dockerfiles -> structurally similar but service-specific (ports, env, deps)

## Tech Debt Registry
- Active duplication tech debt -> `docs/TODO.md` (Cross-Service Duplication section)

## Successful Extractions (Reference)
- ServiceClaims/UserClaims/JWKS/JwtValidator to common::jwt -> `crates/common/src/jwt.rs`
- TestKeypair + JWKS mock to mc-test-utils -> `crates/mc-test-utils/src/jwt_test.rs`
- MeetingTokenRequest/GuestTokenRequest/TokenResponse/ParticipantType/MeetingRole to common::meeting_token -> `crates/common/src/meeting_token.rs` (AC+GC re-export via `pub use`)

## Infrastructure & Integration Seams
- Common crate as extraction target -> `crates/common/src/`
- JWT thin wrapper pattern (GC + MC) -> `crates/{gc,mc}-service/src/auth/`
