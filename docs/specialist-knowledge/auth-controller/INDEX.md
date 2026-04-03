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
- Rate limit config parsing -> `crates/ac-service/src/config.rs:Config::parse_rate_limit_i64()`
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
- JWKS client (common) -> `crates/common/src/jwt.rs:JwksClient`
- JWT validator (common, generic) -> `crates/common/src/jwt.rs:JwtValidator::validate()`
- HasIat trait (iat access for generic validation) -> `crates/common/src/jwt.rs:HasIat`
- JWT error types (common) -> `crates/common/src/jwt.rs:JwtError`
- JWT verify token (EdDSA signature check) -> `crates/common/src/jwt.rs:verify_token()`
- Service claims (common) -> `crates/common/src/jwt.rs:ServiceClaims`
- User claims (common) -> `crates/common/src/jwt.rs:UserClaims`
- Meeting token claims (common, JWT) -> `crates/common/src/jwt.rs:MeetingTokenClaims`
- Guest token claims (common, JWT) -> `crates/common/src/jwt.rs:GuestTokenClaims`
- Participant type enum (common, JWT, 2-variant) -> `crates/common/src/jwt.rs:ParticipantType`
- Meeting role enum (common, JWT, 2-variant) -> `crates/common/src/jwt.rs:MeetingRole`
- Meeting token request (shared GC->AC) -> `crates/common/src/meeting_token.rs:MeetingTokenRequest`
- Guest token request (shared GC->AC) -> `crates/common/src/meeting_token.rs:GuestTokenRequest`
- Participant type enum (shared, 3-variant) -> `crates/common/src/meeting_token.rs:ParticipantType`
- Meeting role enum (shared, 3-variant) -> `crates/common/src/meeting_token.rs:MeetingRole`
- AC re-exports shared types -> `crates/ac-service/src/models/mod.rs` (`pub use common::meeting_token::...`)
- Internal token response (AC-local) -> `crates/ac-service/src/models/mod.rs:InternalTokenResponse`
- Error types -> `crates/ac-service/src/errors.rs:AcError`
- Metrics recording -> `crates/ac-service/src/observability/metrics.rs:record_token_issuance()`
- Correlation hashing -> `crates/ac-service/src/observability/mod.rs:hash_for_correlation()`

## Internal Token Endpoints (ADR-0020)
- Meeting token handler -> `crates/ac-service/src/handlers/internal_tokens.rs:handle_meeting_token()`
- Guest token handler -> `crates/ac-service/src/handlers/internal_tokens.rs:handle_guest_token()`
- Request types (`MeetingTokenRequest`, `GuestTokenRequest`) are shared via `common::meeting_token`
- AC re-exports them from `crate::models` so handler imports are unchanged
- Note: `common::meeting_token::{ParticipantType, MeetingRole}` (3-variant, snake_case) differs from `common::jwt::{ParticipantType, MeetingRole}` (2-variant, lowercase). Wire-compatible but separate Rust types. Unification is a future cleanup item.

## Integration Seams
- Auth middleware (service tokens) -> `crates/ac-service/src/middleware/auth.rs:require_service_auth()`
- Admin scope guard -> `crates/ac-service/src/middleware/auth.rs:require_admin_scope()`
- Org extraction (subdomain) -> `crates/ac-service/src/middleware/org_extraction.rs:require_org_context()`
- HTTP metrics (outermost layer) -> `crates/ac-service/src/middleware/http_metrics.rs:http_metrics_middleware()`
- Token manager (consumer side) -> `crates/common/src/token_manager.rs:spawn_token_manager()`
- Test server harness -> `crates/ac-test-utils/src/server_harness.rs`
- DB: credentials repo -> `crates/ac-service/src/repositories/service_credentials.rs`
- DB: signing keys repo -> `crates/ac-service/src/repositories/signing_keys.rs`
- K8s configmap (rate limits) -> `infra/services/ac-service/configmap.yaml`
- K8s statefulset -> `infra/services/ac-service/statefulset.yaml`
- Deployment runbook -> `docs/runbooks/ac-service-deployment.md`

