# Code Reviewer Navigation

## Architecture & Design
- Actor handle/task separation → ADR-0001 (Section: Pattern)
- No-panic policy, `#[allow(clippy::expect_used)]` justification → ADR-0002
- Error handling, service-layer wrapping → ADR-0003
- Observability naming, label cardinality, SLO targets → ADR-0011
- Guard pipeline methodology → ADR-0015
- DRY cross-service duplication → ADR-0019
- Agent teams validation pipeline → ADR-0024

## Code Locations
- Clippy deny list (unwrap, expect, panic, indexing) → `Cargo.toml:34-42`
- Config constants + defense-in-depth → `crates/ac-service/src/config.rs:from_vars()`
- Crypto (EdDSA, AES-256-GCM, bcrypt) → `crates/ac-service/src/crypto/mod.rs:sign_jwt()`
- Error type reference → `crates/ac-service/src/errors.rs:AcError`
- Handler pattern (auth flow) → `crates/ac-service/src/handlers/auth_handler.rs:handle_service_token()`
- Metrics wiring reference → `crates/ac-service/src/observability/metrics.rs:init_metrics_recorder()`
- Route composition → `crates/ac-service/src/routes/mod.rs:build_routes()`
- Repository layer (sqlx queries) → `crates/ac-service/src/repositories/signing_keys.rs`
- Service layer (business logic) → `crates/ac-service/src/services/key_management_service.rs`
- SecretString/SecretBox → `crates/common/src/secret.rs`
- TokenManager (spawn-and-wait, callback) → `crates/common/src/token_manager.rs:spawn_token_manager()`

## Guard Scripts
- Guard runner + common utilities → `scripts/guards/run-guards.sh`, `scripts/guards/common.sh`
- Instrument skip-all enforcement → `scripts/guards/simple/instrument-skip-all.sh`
- Metrics dashboard + catalog coverage → `scripts/guards/simple/validate-application-metrics.sh`
- No-panic/no-secrets guards → `scripts/guards/simple/no-hardcoded-secrets.sh`
- Semantic checks catalog → `scripts/guards/semantic/checks.md`

## Integration Seams
- Review protocol (fix-or-defer) → `.claude/skills/devloop/review-protocol.md`
- Handler-to-service boundary → `crates/ac-service/src/handlers/` calls `crates/ac-service/src/services/`
- Service-to-repository boundary → `crates/ac-service/src/services/` calls `crates/ac-service/src/repositories/`
- Common crate shared by all services → `crates/common/src/`
- ADR lookup → `docs/decisions/`
