# DRY Reviewer Navigation

## Architecture & Design
- Blocking vs tech-debt classification -> ADR-0019 (`docs/decisions/adr-0019-dry-reviewer.md`)
- Fix-or-defer review model -> ADR-0024 (`docs/decisions/adr-0024-agent-teams-workflow.md`)

## Shared Code (Duplication Prevention)
- Common crate modules -> `crates/common/src/lib.rs`
- JWT utilities (extracted TD-1/TD-2) -> `crates/common/src/jwt.rs:extract_kid()`
- ServiceClaims (extracted from AC) -> `crates/common/src/jwt.rs:ServiceClaims`
- UserClaims (extracted from AC, ADR-0020) -> `crates/common/src/jwt.rs:UserClaims`
- MeetingTokenClaims (ADR-0020) -> `crates/common/src/jwt.rs:MeetingTokenClaims`
- GuestTokenClaims + validate() (ADR-0020) -> `crates/common/src/jwt.rs:GuestTokenClaims`
- ParticipantType enum -> `crates/common/src/jwt.rs:ParticipantType`
- MeetingRole enum -> `crates/common/src/jwt.rs:MeetingRole`
- Token management (prevents static token dup) -> `crates/common/src/token_manager.rs:TokenManagerConfig`
- Secret types -> `crates/common/src/secret.rs`
- Domain IDs and shared types -> `crates/common/src/types.rs`

## Tech Debt Registry
- Active duplication tech debt → `docs/TODO.md` (Cross-Service Duplication section)

## Successful Extractions (Reference)
- ServiceClaims to common::jwt (AC re-exports as `Claims`) -> `crates/ac-service/src/crypto/mod.rs:23`
- UserClaims to common::jwt (AC re-exports) -> `crates/ac-service/src/crypto/mod.rs:29`
- GC uses UserClaims from common::jwt (not reimplemented) -> `crates/gc-service/src/handlers/meetings.rs`, `crates/gc-service/src/auth/jwt.rs`
- Generic verify_token<T> (service + user tokens, single JWK path) -> `crates/gc-service/src/auth/jwt.rs:verify_token()`
- Shared bearer token extraction (both auth middlewares) -> `crates/gc-service/src/middleware/auth.rs:extract_bearer_token()`
- Shared map_row_to_meeting (handler + repo, single definition) -> `crates/gc-service/src/repositories/meetings.rs:map_row_to_meeting()`
- Parameterized audit logging (action + optional user_id) -> `crates/gc-service/src/repositories/meetings.rs:log_audit_event()`
- Closure-based generic extraction -> `crates/gc-service/src/tasks/generic_health_checker.rs:start_generic_health_checker()`
- Thin wrappers after extraction -> `crates/gc-service/src/tasks/health_checker.rs`, `crates/gc-service/src/tasks/mh_health_checker.rs`

## False Positive Boundaries
- Actor vs controller metrics (different consumers) -> `crates/mc-service/src/actors/metrics.rs`
- Service-prefixed metric names (convention) -> per-service `observability/metrics.rs`
- Per-operation metric functions (record_meeting_creation, record_meeting_join, etc.) -> per-service `observability/metrics.rs`
- AC API ParticipantType (has Guest variant) vs common::jwt::ParticipantType (no Guest) -> `crates/gc-service/src/services/ac_client.rs`

## Integration Seams
- Common crate as extraction target -> `crates/common/src/`
- GC repositories (shared row mappers) -> `crates/gc-service/src/repositories/`
- GC ParticipantsRepository -> `crates/gc-service/src/repositories/participants.rs`
- GC dual auth middleware (service + user) -> `crates/gc-service/src/middleware/auth.rs`
- GC JWKS/JWT (potential extraction to common) -> `crates/gc-service/src/auth/jwks.rs`, `crates/gc-service/src/auth/jwt.rs`
- NetworkPolicy + ServiceMonitor cross-refs -> `infra/services/{ac,gc,mc}-service/`
- Metric names in runbooks must match code -> `crates/gc-service/src/observability/metrics.rs`, `docs/runbooks/gc-incident-response.md`
- Dev cert generation (shared helper, single CA) -> `scripts/generate-dev-certs.sh:generate_service_cert()`
- MC TLS secret + volume mount -> `infra/services/mc-service/tls-secret.yaml`, `deployment.yaml`
