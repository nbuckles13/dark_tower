# Auth Controller Navigation

## Architecture & Design
- Service-to-service OAuth 2.0 flow -> ADR-0003
- gRPC auth scopes & two-layer auth model -> ADR-0003 (Component 6)
- Token lifetime & expiry rules -> ADR-0007
- Key rotation strategy -> ADR-0008
- Integration test infrastructure -> ADR-0009
- User auth & meeting access claims -> ADR-0020
- Observability framework -> ADR-0011
- Metric testability (AC backfill is Cat C test-only; no accept-loop-style work) -> ADR-0032

## Code Locations
- Config loading & validation -> `crates/ac-service/src/config.rs:Config::from_vars()`
- Rate limit config parsing -> `crates/ac-service/src/config.rs:Config::parse_rate_limit_i64()`
- JWT sign/verify (service+user), key encrypt/decrypt, bcrypt hash/verify -> `crates/ac-service/src/crypto/mod.rs` (`sign_jwt` / `sign_user_jwt` / `verify_jwt` / `verify_user_jwt` / `encrypt_private_key` / `hash_client_secret`)
- Service token issuance -> `crates/ac-service/src/services/token_service.rs:issue_service_token()`
- User token issuance -> `crates/ac-service/src/services/token_service.rs:issue_user_token()`
- Service registration -> `crates/ac-service/src/services/registration_service.rs:register_service()`
- User registration -> `crates/ac-service/src/services/user_service.rs:register_user()`
- Key rotation -> `crates/ac-service/src/services/key_management_service.rs:rotate_signing_key()`
- Key init at startup -> `crates/ac-service/src/services/key_management_service.rs:initialize_signing_key()`
- JWKS generation -> `crates/ac-service/src/services/key_management_service.rs:get_jwks()`
- Route definitions -> `crates/ac-service/src/routes/mod.rs:build_routes()`
- JWT primitives (common) -> `crates/common/src/jwt.rs` (`JwksClient` / `JwtValidator::validate` / `HasIat` / `JwtError` / `verify_token` / `ServiceClaims` / `UserClaims` / `MeetingTokenClaims` / `GuestTokenClaims` / `ParticipantType` / `MeetingRole` — 2-variant lowercase variants)
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
- Request types (`MeetingTokenRequest`, `GuestTokenRequest`) shared via `common::meeting_token`; AC re-exports from `crate::models`
- Note: `common::meeting_token::{ParticipantType, MeetingRole}` (3-variant, snake_case) vs `common::jwt::{ParticipantType, MeetingRole}` (2-variant, lowercase) — wire-compatible, separate Rust types; unification pending.

## Scope Data (ADR-0003)
- Default scopes per service type -> `crates/ac-service/src/models/mod.rs:ServiceType::default_scopes()`
- DB seed scopes -> `infra/kind/scripts/setup.sh:457-459`
- Token issuance (scopes from DB, service_type claim) -> `crates/ac-service/src/services/token_service.rs:111-148`
- ServiceClaims (scope + service_type) -> `crates/common/src/jwt.rs:ServiceClaims`

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

## Metric tests (ADR-0032 Step 4)
- Per-cluster integration tests under `crates/ac-service/tests/` + in-src `tests` module of `crates/ac-service/src/observability/metrics.rs`; shared fixtures at `crates/ac-service/tests/common/test_state.rs` (`make_app_state` / `seed_signing_key` / `seed_service_credential` / `TEST_CLIENT_SECRET`) and `crates/ac-service/tests/common/jwt_fixtures.rs` (`sign_service_token` / `sign_user_token`).
- Ground rule: drive sites via direct handler calls + `#[sqlx::test]`, NOT `TestAuthServer` harness — spawned tasks bypass the per-thread `MetricAssertion` recorder; `flavor = "current_thread"` is load-bearing. Histogram-first in mixed-kind snapshots; partial-label `assert_delta(0)` adjacency on every multi-label test.
- `assert_unobserved` API (counter/gauge/histogram, kind-mismatch hardening) -> `crates/common/src/observability/testing.rs` (closes ADR-0032 §F4 gauge absence gap). WRAPPER-CAT-C framing pattern example above `metrics_module_emits_token_validation_cluster` in `crates/ac-service/src/observability/metrics.rs`.
- Audit-log fault-injection seams: `break_auth_events_inserts` (CHECK NOT VALID — preserves pre-INSERT SELECTs) vs `break_auth_events_table` (DROP); `ALL_EVENT_TYPES` doc-comment in `crates/ac-service/tests/audit_log_failures_integration.rs` is authoritative. `expire_old_keys` seed shifts BOTH `valid_from` AND `valid_until` (CHECK requires `valid_until > valid_from`). `register_user` chained dual-emission: audit-broken path emits BOTH `user_registered` AND `user_login` (auto-login via `issue_user_token`).
- Discipline: plan-stage commitment fidelity is load-bearing — treat any "X of Y" footnote as a blocker for "Ready for validation", not a deferral. Deferred follow-ups -> `docs/TODO.md`.
