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
- GC dual auth middleware (service + user) -> `crates/gc-service/src/middleware/auth.rs`
- JWT thin wrapper pattern (GC + MC) -> `crates/{gc,mc}-service/src/auth/`
- NetworkPolicy + ServiceMonitor cross-refs -> `infra/services/{ac,gc,mc}-service/`
- Metric names in runbooks must match code -> `docs/runbooks/gc-incident-response.md`
- Dev cert generation (shared helper, single CA) -> `scripts/generate-dev-certs.sh`
- MC TLS secret + volume mount -> `infra/services/mc-service/tls-secret.yaml`
