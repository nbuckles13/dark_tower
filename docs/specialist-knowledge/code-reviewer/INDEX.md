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
- JWT verify/validation → `crates/gc-service/src/auth/jwt.rs:verify_token()`, `validate_user()`
- JWKS client → `crates/gc-service/src/auth/jwks.rs:JwksClient`
- Auth middleware → `crates/gc-service/src/middleware/auth.rs:require_user_auth()`, `extract_bearer_token()`
- Meeting handlers → `crates/gc-service/src/handlers/meetings.rs:create_meeting()`, `join_meeting()`, `get_guest_token()`, `update_meeting_settings()`
- Meetings repository (atomic CTE) → `crates/gc-service/src/repositories/meetings.rs:create_meeting_with_limit_check()`
- Meeting activation + audit logging → `crates/gc-service/src/repositories/meetings.rs:activate_meeting()`, `log_audit_event()`
- Participants repo, model, tests → `crates/gc-service/src/repositories/participants.rs`, `models/mod.rs:Participant`, `tests/participant_tests.rs`
- Participant migration → `migrations/20260322000001_add_participant_tracking.sql`
- Meeting join metrics → `crates/gc-service/src/observability/metrics.rs:record_meeting_join()`
- AC/MC clients → `crates/gc-service/src/services/ac_client.rs:AcClient`, `mc_client.rs:McClientTrait`
- Route composition (user auth layer) → `crates/gc-service/src/routes/mod.rs:build_routes()`
- Meeting integration tests → `crates/gc-service/tests/meeting_tests.rs`
- Metrics catalog → `docs/observability/metrics/gc-service.md`
- Grafana dashboard → `infra/grafana/dashboards/gc-overview.json`

## Code Locations — Common
- JWT claims & enums → `crates/common/src/jwt.rs:ServiceClaims`, `UserClaims`, `MeetingTokenClaims`, `GuestTokenClaims`, `ParticipantType`, `MeetingRole`
- Guest token validation → `crates/common/src/jwt.rs:GuestTokenClaims::validate()`
- SecretString/SecretBox → `crates/common/src/secret.rs`
- TokenManager → `crates/common/src/token_manager.rs:spawn_token_manager()`

## Infrastructure & Guards
- MC TLS + cert generation → `infra/services/mc-service/tls-secret.yaml`, `scripts/generate-dev-certs.sh`
- Guard runner → `scripts/guards/run-guards.sh`; Review protocol → `.claude/skills/devloop/review-protocol.md`
