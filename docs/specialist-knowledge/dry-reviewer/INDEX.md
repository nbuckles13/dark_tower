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
- Connection handler + bridge loop + framing -> `crates/mc-service/src/webtransport/connection.rs`
  - Bug fix (task 15): send_error() now calls stream.finish().await to flush before drop (line 543)
- Shared encode_participant_update() -> `crates/mc-service/src/webtransport/handler.rs`
- ParticipantActor -> `crates/mc-service/src/actors/participant.rs`

## Per-Service Observability (Metrics & Dashboards)
- AC/GC/MC metrics -> `crates/*/src/observability/metrics.rs` (per-service, not duplication)
- MC/GC join alert rules -> `infra/docker/prometheus/rules/{mc,gc}-alerts.yaml`

## MC Integration Test Coverage (Task 15)
- Join flow tests (11 tests) -> `crates/mc-service/tests/join_tests.rs`
  - TestServer: self-signed TLS WebTransport, wiremock JWKS, real actor hierarchy
  - T1-T2: happy path, empty roster | T3-T4: expired/garbage token | T5: meeting not found
  - T6-T7: invalid protobuf, wrong first message | T8: wrong signing key
  - T9-T11: actor-level join, roster visibility, name length | Bridge: ParticipantJoined
- JWT test fixtures (shared) -> `crates/mc-test-utils/src/jwt_test.rs`
  - TestKeypair, build_pkcs8_from_seed, mount_jwks_mock, make_meeting_claims variants
  - auth/mod.rs #[cfg(test)] inline copy remains (can't import dev-dep from unit tests)

## GC Integration Test Coverage (Task 14)
- Join/guest/settings tests -> `crates/gc-service/tests/meeting_tests.rs`
- Meeting creation tests -> `crates/gc-service/tests/meeting_create_tests.rs`
- Auth tests (service token JWKS) -> `crates/gc-service/tests/auth_tests.rs`
- Shared GC test harness -> `crates/gc-test-utils/src/server_harness.rs`

## Tech Debt Registry
- Active duplication tech debt → `docs/TODO.md` (Cross-Service Duplication section)

## Successful Extractions (Reference)
- ServiceClaims/UserClaims to common::jwt -> `crates/common/src/jwt.rs`
- JWKS + JwtValidator to common::jwt (R-23) -> `crates/common/src/jwt.rs:JwtValidator`
- GC shared helpers -> bearer token, map_row_to_meeting, audit logging, generic health checker
- TestKeypair + JWKS mock to mc-test-utils (task 15) -> `crates/mc-test-utils/src/jwt_test.rs`

## Health Endpoints (Cross-Service Consistency)
- MC health routes -> `crates/mc-service/src/observability/health.rs:health_router()`
- MC health probes (K8s) -> `infra/services/mc-service/deployment.yaml` (livenessProbe, readinessProbe)
- GC health routes -> `crates/gc-service/src/routes/mod.rs:64-65`
- GC health probes (K8s) -> `infra/services/gc-service/deployment.yaml` (livenessProbe, readinessProbe)

## False Positive Boundaries
- Actor vs controller metrics (different consumers) -> `crates/mc-service/src/actors/metrics.rs`
- Per-service error mapping (From<JwtError> for GcError vs McError) -> required, not duplication
- AC API ParticipantType (has Guest) vs common::jwt::ParticipantType (no Guest)

## Integration Seams
- Common crate as extraction target -> `crates/common/src/`
- JWT thin wrapper pattern (GC + MC) -> `crates/{gc,mc}-service/src/auth/`
- Test fixture pattern (mc-test-utils, gc-test-utils) -> `crates/*-test-utils/`
- Metric names in runbooks must match code -> `docs/runbooks/`, alert rule files
