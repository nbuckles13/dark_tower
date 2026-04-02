# DRY Reviewer Navigation

## Architecture & Design
- Blocking vs tech-debt classification -> ADR-0019 (`docs/decisions/adr-0019-dry-reviewer.md`)
- Fix-or-defer review model -> ADR-0024 (`docs/decisions/adr-0024-agent-teams-workflow.md`)

## JWT Validation (Common + Thin Wrappers)
- Common JWT code (all shared logic) -> `crates/common/src/jwt.rs`
  - JwtValidator, JwksClient, JwtError, HasIat, Claims types, ParticipantType, MeetingRole
- GC thin wrapper -> `crates/gc-service/src/auth/jwt.rs` (ServiceClaims, UserClaims)
- MC thin wrapper -> `crates/mc-service/src/auth/mod.rs` (MeetingTokenClaims, GuestTokenClaims)
  - Post-validation domain checks: token_type=="meeting" (line 66), GuestTokenClaims::validate() (line 89)

## MC WebTransport Layer
- WebTransport server (accept loop) -> `crates/mc-service/src/webtransport/server.rs`
- Connection handler + bridge loop -> `crates/mc-service/src/webtransport/connection.rs`
- ParticipantActor -> `crates/mc-service/src/actors/participant.rs`

## Per-Service Observability (Metrics & Dashboards)
- AC/GC/MC/MH metrics -> `crates/*/src/observability/metrics.rs` (per-service, not duplication)
- MC/GC join alert rules -> `infra/docker/prometheus/rules/{mc,gc}-alerts.yaml`
- Dashboard metric presentation → ADR-0029
- Grafana dashboards + configMapGenerator → `infra/grafana/dashboards/`, `infra/kubernetes/observability/grafana/`

## MC Integration Test Coverage (Task 15)
- Join flow tests (11 tests) -> `crates/mc-service/tests/join_tests.rs`
  - TestServer: self-signed TLS WebTransport, wiremock JWKS, real actor hierarchy
  - T1-T2: happy path, empty roster | T3-T4: expired/garbage token | T5: meeting not found
  - T6-T7: invalid protobuf, wrong first message | T8: wrong signing key
  - T9-T11: actor-level join, roster visibility, name length | Bridge: ParticipantJoined
- JWT test fixtures (shared) -> `crates/mc-test-utils/src/jwt_test.rs`

## GC Integration Test Coverage (Task 14)
- Join/guest/settings tests -> `crates/gc-service/tests/meeting_tests.rs`
- Meeting creation tests -> `crates/gc-service/tests/meeting_create_tests.rs`
- Auth tests (service token JWKS) -> `crates/gc-service/tests/auth_tests.rs`
- Shared GC test harness -> `crates/gc-test-utils/src/server_harness.rs`

## Tech Debt Registry
- Active duplication tech debt → `docs/TODO.md` (Cross-Service Duplication section)

## Successful Extractions (Reference)
- ServiceClaims/UserClaims/JWKS/JwtValidator to common::jwt -> `crates/common/src/jwt.rs`
- TestKeypair + JWKS mock to mc-test-utils -> `crates/mc-test-utils/src/jwt_test.rs`

## Health Endpoints (Cross-Service Consistency)
- MC health routes -> `crates/mc-service/src/observability/health.rs:health_router()`
- MH health routes -> `crates/mh-service/src/observability/health.rs:health_router()` (duplicates MC)
- GC health routes -> `crates/gc-service/src/routes/mod.rs:64-65`

## Per-Service Config Parsing
- AC config -> `crates/ac-service/src/config.rs:Config::from_vars()`
- GC config -> `crates/gc-service/src/config.rs:Config::from_vars()`
- MC config -> `crates/mc-service/src/config.rs:Config::from_vars()`
- MH config -> `crates/mh-service/src/config.rs:Config::from_vars()`

## gRPC Auth Interceptors (Cross-Service)
- MC auth interceptor -> `crates/mc-service/src/grpc/auth_interceptor.rs:McAuthInterceptor`
- MH auth interceptor -> `crates/mh-service/src/grpc/auth_interceptor.rs:MhAuthInterceptor` (duplicates MC)
- Shared constant -> `common::jwt::MAX_JWT_SIZE_BYTES`

## MH GC Client (MH->GC Registration + Load Reports)
- GC client -> `crates/mh-service/src/grpc/gc_client.rs:GcClient`
- MH gRPC stub service -> `crates/mh-service/src/grpc/mh_service.rs:MhMediaService`
- MH error types -> `crates/mh-service/src/errors.rs:MhError`

## False Positive Boundaries
- Per-service error mapping (GcError vs McError vs MhError) -> required, not duplication
- MC GcClient (GlobalControllerService) vs MH GcClient (MediaHandlerRegistryService) -> different RPCs
- AC rate limiting (DB-backed lockout) vs GC rate limiting (middleware RPM) -> different mechanisms

## Infrastructure & Integration Seams
- Common crate as extraction target → `crates/common/src/`
- JWT thin wrapper pattern (GC + MC) → `crates/{gc,mc}-service/src/auth/`
- Test fixture pattern → `crates/*-test-utils/`
