# Code Reviewer Navigation

## Architecture & Design
- Actor handle/task separation → ADR-0001 (Section: Pattern)
- No-panic policy, `#[allow(clippy::expect_used)]` justification → ADR-0002
- Error handling, service-layer wrapping → ADR-0003
- Observability naming, label cardinality, SLO targets → ADR-0011
- Guard pipeline methodology → ADR-0015
- DRY cross-service duplication → ADR-0019
- User auth, three-tier token architecture → ADR-0020
- Agent teams validation pipeline → ADR-0024

## Code Locations — AC Service
- Clippy deny list (unwrap, expect, panic, indexing) → `Cargo.toml:34-42`
- Config constants + defense-in-depth → `crates/ac-service/src/config.rs:from_vars()`
- Crypto (EdDSA, AES-256-GCM, bcrypt) → `crates/ac-service/src/crypto/mod.rs:sign_jwt()`
- Error type reference → `crates/ac-service/src/errors.rs:AcError`
- Handler pattern (auth flow) → `crates/ac-service/src/handlers/auth_handler.rs:handle_service_token()`
- Metrics wiring reference → `crates/ac-service/src/observability/metrics.rs:init_metrics_recorder()`
- Route composition → `crates/ac-service/src/routes/mod.rs:build_routes()`
- Repository layer (sqlx queries) → `crates/ac-service/src/repositories/signing_keys.rs`
- Service layer (business logic) → `crates/ac-service/src/services/key_management_service.rs`

## Code Locations — GC Service
- Error type reference → `crates/gc-service/src/errors.rs:GcError`
- Generic JWT verify → `crates/gc-service/src/auth/jwt.rs:verify_token()`
- User JWT validation → `crates/gc-service/src/auth/jwt.rs:validate_user()`
- User auth middleware → `crates/gc-service/src/middleware/auth.rs:require_user_auth()`
- Bearer extraction helper → `crates/gc-service/src/middleware/auth.rs:extract_bearer_token()`
- Meeting creation handler → `crates/gc-service/src/handlers/meetings.rs:create_meeting()`
- Meeting code CSPRNG → `crates/gc-service/src/handlers/meetings.rs:generate_meeting_code()`
- Meeting request/response models → `crates/gc-service/src/models/mod.rs:CreateMeetingRequest`
- Meetings repository (atomic CTE) → `crates/gc-service/src/repositories/meetings.rs:create_meeting_with_limit_check()`
- Meeting creation metrics → `crates/gc-service/src/observability/metrics.rs:record_meeting_creation()`
- Route composition (user auth layer) → `crates/gc-service/src/routes/mod.rs:build_routes()`

## Code Locations — Common
- Shared JWT claims (ServiceClaims, UserClaims) → `crates/common/src/jwt.rs:ServiceClaims`, `crates/common/src/jwt.rs:UserClaims`
- SecretString/SecretBox → `crates/common/src/secret.rs`
- TokenManager (spawn-and-wait, callback) → `crates/common/src/token_manager.rs:spawn_token_manager()`

## Guard Scripts
- Guard runner → `scripts/guards/run-guards.sh`
- Instrument skip-all → `scripts/guards/simple/instrument-skip-all.sh`
- Metrics validation → `scripts/guards/simple/validate-application-metrics.sh`
- Semantic checks → `scripts/guards/semantic/checks.md`

## Integration Seams
- Review protocol (fix-or-defer) → `.claude/skills/devloop/review-protocol.md`
- AC handler-to-service boundary → `crates/ac-service/src/handlers/` calls `crates/ac-service/src/services/`
- AC service-to-repository boundary → `crates/ac-service/src/services/` calls `crates/ac-service/src/repositories/`
- GC handler-to-repository boundary → `crates/gc-service/src/handlers/` calls `crates/gc-service/src/repositories/`
- GC middleware-to-auth boundary → `crates/gc-service/src/middleware/auth.rs` calls `crates/gc-service/src/auth/jwt.rs`
- Common crate shared by all services → `crates/common/src/`
- ADR lookup → `docs/decisions/`
