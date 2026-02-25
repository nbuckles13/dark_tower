# Security Navigation

## Architecture & Design
- Service authentication (OAuth 2.0 Client Credentials) → ADR-0003
- Token lifetime & refresh → ADR-0007
- Key rotation (signing keys, grace periods) → ADR-0008
- User auth & meeting access → ADR-0020
- No-panic policy → ADR-0002
- Guard methodology → ADR-0015
- MC session binding & HKDF key hierarchy → ADR-0023 (Section 1)
- Approved algorithms → ADR-0027

## Code Locations
- JWT signing/verification → `crates/ac-service/src/crypto/mod.rs:sign_jwt()`, `verify_jwt()`
- Key encryption at rest → `crates/ac-service/src/crypto/mod.rs:encrypt_private_key()`
- Bcrypt hash/verify → `crates/ac-service/src/crypto/mod.rs:hash_client_secret()`, `verify_client_secret()`
- Token issuance (service) → `crates/ac-service/src/services/token_service.rs:issue_service_token()`
- Token issuance (user) → `crates/ac-service/src/services/token_service.rs:issue_user_token()`
- Security config bounds → `crates/ac-service/src/config.rs`
- JWT size constant → `crates/common/src/jwt.rs:MAX_JWT_SIZE_BYTES`
- Shared claims types → `crates/common/src/jwt.rs:ServiceClaims`, `UserClaims` (PII-redacted Debug)
- Token manager (secure constructor) → `crates/common/src/token_manager.rs:new_secure()`
- GC JWT validation → `crates/gc-service/src/auth/jwt.rs:validate()`
- GC JWKS fetching → `crates/gc-service/src/auth/jwks.rs`
- MC gRPC auth interceptor → `crates/mc-service/src/grpc/auth_interceptor.rs`
- MC session binding actors → `crates/mc-service/src/actors/session.rs`

## Integration Seams
- AC JWKS endpoint consumed by GC → `crates/gc-service/src/auth/jwks.rs`
- GC-to-MC authenticated gRPC → `crates/mc-service/src/grpc/auth_interceptor.rs`
- Token refresh callback (shared-to-service metrics) → `crates/common/src/token_manager.rs:with_on_refresh()`
- GC default scopes (incl. `internal:meeting-token`) → `crates/ac-service/src/models/mod.rs:ServiceType::default_scopes()`
- Credential leak guards → `scripts/guards/simple/no-secrets-in-logs.sh`, `instrument-skip-all.sh`

## Cross-Cutting Gotchas
- Dummy bcrypt hash must match production cost factor → `crates/ac-service/src/services/token_service.rs`
- `.expose_secret()` calls are audit points; grep to find leak sites → `crates/ac-service/src/`
- `#[instrument]` without `skip_all` auto-captures params; `.instrument()` chaining does not
- Silent `return Ok(())` in env-tests is fail-open → `crates/env-tests/tests/`
- Service token `sub` is a string, not UUID; breaks `parse_user_id()` on user endpoints
