# Auth Controller Navigation

## Architecture & Design
- Service-to-service OAuth 2.0 flow -> ADR-0003
- Token lifetime & expiry rules -> ADR-0007
- Key rotation strategy -> ADR-0008
- Integration test infrastructure -> ADR-0009
- User auth & meeting access claims -> ADR-0020
- Observability framework -> ADR-0011

## Code Locations
- Config loading & validation -> `crates/ac-service/src/config.rs:Config::from_vars()`
- JWT signing (service) -> `crates/ac-service/src/crypto/mod.rs:sign_jwt()`
- JWT signing (user) -> `crates/ac-service/src/crypto/mod.rs:sign_user_jwt()`
- JWT verification (service) -> `crates/ac-service/src/crypto/mod.rs:verify_jwt()`
- JWT verification (user) -> `crates/ac-service/src/crypto/mod.rs:verify_user_jwt()`
- Key encrypt/decrypt -> `crates/ac-service/src/crypto/mod.rs:encrypt_private_key()`
- Bcrypt hash/verify -> `crates/ac-service/src/crypto/mod.rs:hash_client_secret()`
- Service token issuance -> `crates/ac-service/src/services/token_service.rs:issue_service_token()`
- User token issuance -> `crates/ac-service/src/services/token_service.rs:issue_user_token()`
- Service registration -> `crates/ac-service/src/services/registration_service.rs:register_service()`
- User registration -> `crates/ac-service/src/services/user_service.rs:register_user()`
- Key rotation -> `crates/ac-service/src/services/key_management_service.rs:rotate_signing_key()`
- Key init at startup -> `crates/ac-service/src/services/key_management_service.rs:initialize_signing_key()`
- JWKS generation -> `crates/ac-service/src/services/key_management_service.rs:get_jwks()`
- Route definitions -> `crates/ac-service/src/routes/mod.rs:build_routes()`
- Error types -> `crates/ac-service/src/errors.rs:AcError`
- Metrics recording -> `crates/ac-service/src/observability/metrics.rs:record_token_issuance()`
- Correlation hashing -> `crates/ac-service/src/observability/mod.rs:hash_for_correlation()`

## Integration Seams
- Auth middleware (service tokens) -> `crates/ac-service/src/middleware/auth.rs:require_service_auth()`
- Admin scope guard -> `crates/ac-service/src/middleware/auth.rs:require_admin_scope()`
- Org extraction (subdomain) -> `crates/ac-service/src/middleware/org_extraction.rs:require_org_context()`
- HTTP metrics (outermost layer) -> `crates/ac-service/src/middleware/http_metrics.rs:http_metrics_middleware()`
- Token manager (consumer side) -> `crates/common/src/token_manager.rs:spawn_token_manager()`
- Test server harness -> `crates/ac-test-utils/src/server_harness.rs`
- DB: credentials repo -> `crates/ac-service/src/repositories/service_credentials.rs`
- DB: signing keys repo -> `crates/ac-service/src/repositories/signing_keys.rs`

## Cross-Cutting Gotchas
- Dummy hash cost must match production cost or timing attack protection breaks -> `services/token_service.rs`
- `rotate_signing_key_tx` does NOT set gauges; caller must -> `services/key_management_service.rs`
- `init_key_metrics()` required at startup or gauges read zero until next rotation -> `main.rs`
- Metrics middleware must be outermost layer to capture framework-level errors -> `middleware/http_metrics.rs`
- Service token `sub` is a string not UUID; handler-level `parse_user_id()` rejects it -> `crypto/mod.rs`
