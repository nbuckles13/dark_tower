# DRY Reviewer Navigation

## Architecture & Design
- Blocking vs tech-debt classification -> ADR-0019 (`docs/decisions/adr-0019-dry-reviewer.md`)
- Fix-or-defer review model -> ADR-0024 (`docs/decisions/adr-0024-agent-teams-workflow.md`)

## JWT Validation (Common + Thin Wrappers)
- Common JWT code (all shared logic) -> `crates/common/src/jwt.rs`
  - JwtValidator (generic), JwksClient, JwtError, HasIat trait
  - verify_token<T>, extract_kid, validate_iat
  - Claims types: ServiceClaims, UserClaims, MeetingTokenClaims, GuestTokenClaims
  - Enums: ParticipantType, MeetingRole
- GC thin wrapper (1st consumer) -> `crates/gc-service/src/auth/jwt.rs`
  - validate() for ServiceClaims, validate_user() for UserClaims
  - From<JwtError> for GcError -> `crates/gc-service/src/errors.rs:177`
- MC thin wrapper (2nd consumer) -> `crates/mc-service/src/auth/mod.rs`
  - validate_meeting_token() for MeetingTokenClaims, validate_guest_token() for GuestTokenClaims
  - Post-validation domain checks: token_type=="meeting" (line 66), GuestTokenClaims::validate() (line 89)
  - From<JwtError> for McError -> `crates/mc-service/src/errors.rs:213`
  - ServiceUnavailable maps to McError::Internal (MC has no ServiceUnavailable; uses signaling codes)

## MC WebTransport Layer
- WebTransport server (accept loop) -> `crates/mc-service/src/webtransport/server.rs`
- Connection handler + bridge loop + framing -> `crates/mc-service/src/webtransport/connection.rs`
- Shared encode_participant_update() -> `crates/mc-service/src/webtransport/handler.rs`
- ParticipantActor -> `crates/mc-service/src/actors/participant.rs`

## Per-Service Observability (Metrics & Dashboards)
- AC metrics -> `crates/ac-service/src/observability/metrics.rs`
- GC metrics -> `crates/gc-service/src/observability/metrics.rs`
- MC metrics -> `crates/mc-service/src/observability/metrics.rs`
  - GC join: `record_meeting_join()` | MC join: `record_webtransport_connection()`, `record_jwt_validation()`, `record_session_join()`
- MC dashboard "Join Flow" row -> `infra/grafana/dashboards/mc-overview.json` (parallel to GC, not duplication)
- MC/GC join alert rules -> `infra/docker/prometheus/rules/{mc,gc}-alerts.yaml` (per-service perspective, not duplication)

## GC Integration Test Coverage
- Join/guest/settings tests (task 14) -> `crates/gc-service/tests/meeting_tests.rs`
  - TestMeetingServer (spawn, spawn_with_ac_failure) — wiremock JWKS + AC internal mocks
  - DB fixtures: create_test_org, create_test_user, create_test_meeting, register_healthy_mc/mh
  - R-18 tests: service token rejected, AC unavailable (503), no MC available (503), active status join
- Meeting creation tests -> `crates/gc-service/tests/meeting_create_tests.rs`
- Auth tests (service token JWKS) -> `crates/gc-service/tests/auth_tests.rs`
- Shared GC test harness (health/E2E only) -> `crates/gc-test-utils/src/server_harness.rs`

## Other Shared Code
- Common crate modules -> `crates/common/src/lib.rs`
- Token management (prevents static token dup) -> `crates/common/src/token_manager.rs`
- Secret types -> `crates/common/src/secret.rs`
- Domain IDs and shared types -> `crates/common/src/types.rs`

## Tech Debt Registry
- Active duplication tech debt -> `docs/TODO.md` (Cross-Service Duplication section)

## Successful Extractions (Reference)
- ServiceClaims to common::jwt (AC re-exports as `Claims`) -> `crates/ac-service/src/crypto/mod.rs:23`
- UserClaims to common::jwt (AC re-exports) -> `crates/ac-service/src/crypto/mod.rs:29`
- JWKS + JwtValidator to common::jwt (R-23) -> `crates/common/src/jwt.rs:JwtValidator`
- GC JWKS re-export -> `crates/gc-service/src/auth/jwks.rs`
- Shared bearer token extraction -> `crates/gc-service/src/middleware/auth.rs:extract_bearer_token()`
- Shared map_row_to_meeting -> `crates/gc-service/src/repositories/meetings.rs:map_row_to_meeting()`
- Parameterized audit logging -> `crates/gc-service/src/repositories/meetings.rs:log_audit_event()`
- Generic health checker extraction -> `crates/gc-service/src/tasks/generic_health_checker.rs`

## False Positive Boundaries
- Actor vs controller metrics (different consumers) -> `crates/mc-service/src/actors/metrics.rs`
- Service-prefixed metric names (convention) -> per-service `observability/metrics.rs`
- Per-service error mapping (From<JwtError> for GcError vs McError) -> required, not duplication
- AC API ParticipantType (has Guest) vs common::jwt::ParticipantType (no Guest) -> `crates/gc-service/src/services/ac_client.rs`

## Integration Seams
- Common crate as extraction target -> `crates/common/src/`
- GC repositories (shared row mappers) -> `crates/gc-service/src/repositories/`
- JWT thin wrapper pattern (GC + MC) -> `crates/{gc,mc}-service/src/auth/`
- Metric names in runbooks must match code -> `docs/runbooks/`, alert rule files
