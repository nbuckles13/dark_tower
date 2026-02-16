# Security Specialist Gotchas

Security pitfalls, edge cases, and warnings discovered in the Dark Tower codebase.

---

## Gotcha: Bcrypt Library vs OWASP Requirements
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/crypto/mod.rs`

Bcrypt crate accepts cost 4-31, but OWASP 2024 requires minimum 10. Library validation is insufficient for compliance. Always enforce security-aware bounds in application code.

---

## Gotcha: Dummy Hash Must Match Production Cost
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/services/token_service.rs`

Timing-safe authentication uses dummy hash for non-existent users. Dummy hash MUST use same cost factor as production. If default cost changes, regenerate dummy or timing attack possible.

---

## Gotcha: Clock Skew Creates Pre-Authentication Window
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/crypto/mod.rs`

JWT `iat` validation with clock skew allows tokens up to N seconds in the future. 300s skew = 5 minute pre-generation window. Necessary for distributed systems but enables token pre-computation attacks.

---

## Gotcha: Test Coverage Hides Timing Issues
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/services/token_service.rs`

Coverage instrumentation adds overhead that masks timing differences. Timing-sensitive tests must be `#[cfg_attr(coverage, ignore)]`. Manual verification required for timing-critical code.

---

## Gotcha: SecretBox Doesn't Derive Clone
**Added**: 2026-01-12
**Related files**: `crates/ac-service/src/config.rs`, `crates/ac-service/src/crypto/mod.rs`

`SecretBox<T>` from `secrecy` crate doesn't implement `Clone` via derive. If your struct contains `SecretBox` and needs Clone, you must implement it manually with `SecretBox::new(Box::new(self.field.expose_secret().clone()))`. Forgetting this causes compile errors, but the fix pattern must maintain secret protection.

---

## Gotcha: Serde Serialize Bypasses SecretString Protection
**Added**: 2026-01-12
**Related files**: `crates/ac-service/src/models/mod.rs`

If you derive `Serialize` on a struct with `SecretString`, the default serialization will NOT expose the secret (it serializes the wrapper). For API responses that MUST return secrets (registration, rotation), implement custom `Serialize` with explicit `.expose_secret()`. Document this as intentional - it's the one place secrets should be exposed.

---

## Gotcha: grep for .expose_secret() During Reviews
**Added**: 2026-01-12
**Related files**: `crates/ac-service/src/`

Every call to `.expose_secret()` is a potential leak point. During security reviews, grep for all `.expose_secret()` calls and verify each is: (1) necessary for crypto operations, (2) intentional API exposure, or (3) test code. Any other usage is suspicious. This is the primary benefit of SecretBox - it makes secret access auditable.

---

## Gotcha: CVE-2018-0114 - Embedded JWK in JWT Header
**Added**: 2026-01-13
**Related files**: `crates/env-tests/tests/25_auth_security.rs`

Some JWT libraries trust public keys embedded in the token's `jwk` header parameter, allowing attackers to sign tokens with their own key. Always validate against keys from a trusted JWKS endpoint only, NEVER from the token header. Test by embedding a fake `jwk` in the header and verifying signature validation still uses the server's JWKS.

---

## Gotcha: SSRF via JWT jku Header
**Added**: 2026-01-13
**Related files**: `crates/env-tests/tests/25_auth_security.rs`

The `jku` (JWK Set URL) header tells the validator where to fetch public keys. If the validator follows this URL, attackers can: (1) exfiltrate internal data via SSRF, (2) serve their own keys to forge tokens. Never fetch keys from URLs specified in token headers. Test vectors should include: external URLs, internal services, localhost, cloud metadata endpoints (169.254.169.254, metadata.google.internal).

---

## Gotcha: Rate Limit Testing May Not Trigger
**Added**: 2026-01-13
**Related files**: `crates/env-tests/tests/10_auth_smoke.rs`

Rate limit tests that send N requests expecting a 429 may not trigger if: (1) rate limits are per-IP and test runs through different IPs, (2) rate limit thresholds are very high, (3) rate limiting is per-client-id and test varies credentials. Consider checking metrics endpoints for rate limit counters as alternative validation.

---

## Gotcha: Typed Deserialization May Miss JWKS Leakage
**Added**: 2026-01-13
**Related files**: `crates/env-tests/tests/25_auth_security.rs`

When testing JWKS endpoints for private key leakage, DON'T rely on typed deserialization alone. A struct without a `d` field will silently ignore `d` in the JSON. Use raw JSON (`serde_json::Value`) to check if forbidden fields exist. Pattern: `jwks_value.get("keys")[i].get("d").is_none()` catches fields that typed structs would skip.

---

## Gotcha: Custom Debug Insufficient for Error Response Bodies
**Added**: 2026-01-18
**Related files**: `crates/env-tests/src/fixtures/gc_client.rs`

Custom Debug implementations only activate when `{:?}` formatting is used. Credentials stored in error enum variants can leak through: (1) `assert_eq!` comparisons (uses Debug but also compares values), (2) `Display` impl that includes the body, (3) Direct string interpolation `format!("{}", body)`. The semantic guard flagged this as HIGH risk. Solution: Sanitize bodies BEFORE storing in error variants, not just in Debug output. This is defense-in-depth - never assume callers will use the "safe" formatting path.

---

## Gotcha: Service Tokens in Registration Structs Often Missed
**Added**: 2026-01-24 (Updated: 2026-01-31)
**Related files**: `crates/gc-service/src/grpc/mh_service.rs`, `crates/gc-service/src/repositories/media_handlers.rs`

When implementing service registration (handlers, workers, external services), the authentication token field is often stored as plain `String` because the focus is on the registration logic rather than data protection. This is especially common in: (1) Registry structs that cache registered services, (2) DTO structs for registration requests, (3) Handler metadata stored in HashMaps. Pattern: When reviewing registration flows, explicitly check for token/secret fields and verify they use `SecretString`. The field names vary: `service_token`, `auth_token`, `bearer_token`, `api_key`, `secret`.

---

## Gotcha: Token Comparison Must Use Constant-Time Operations
**Added**: 2026-01-25 (Updated: 2026-02-10)
**Related files**: `docs/decisions/adr-0023-meeting-controller-architecture.md`, `docs/specialist-knowledge/security/approved-crypto.md`

Direct byte comparison of tokens (`==`) leaks timing information that can reveal valid tokens character-by-character. For HMAC tokens, use `ring::hmac::verify()` which performs constant-time comparison internally. For non-HMAC tokens, use `ring::constant_time::verify_slices_are_equal()` or `subtle::ConstantTimeEq`.

**Common mistake**: Computing expected HMAC tag and comparing with `==`:
```rust
// WRONG: Timing leak
let expected_tag = hmac::sign(&key, message);
if expected_tag.as_ref() == received_tag { ... }

// CORRECT: Constant-time
hmac::verify(&key, message, received_tag)?;
```

**Why ring::hmac::verify() is safe**: It calls `constant_time::verify_slices_are_equal()` internally, preventing timing attacks even if tag length mismatches.

This applies to: binding tokens (ADR-0023), session tokens, CSRF tokens, password reset tokens, any security-sensitive byte comparison. Timing attacks can reveal secrets over network even with TLS—attack measures CPU time, not network latency.

---

## Gotcha: Error Messages Leaking Internal Identifiers
**Added**: 2026-01-25
**Related files**: `docs/decisions/adr-0023-meeting-controller-architecture.md`

Error messages returned to clients should never include internal identifiers (session IDs, user IDs, meeting IDs, participant IDs). These identifiers: (1) Enable enumeration attacks - probe which IDs exist, (2) Aid correlation attacks - link sessions across requests, (3) Leak implementation details. Pattern: Use typed error variants internally (e.g., `ParticipantNotFound(participant_id)`) but convert to generic messages at the API boundary: "Participant not found" without the ID. Log the full error server-side with the ID for debugging. Applies to: 401/403/404 responses, WebSocket/WebTransport error frames, error bodies in any client-facing response.

---

## Gotcha: Connection URLs with Embedded Credentials in Logs
**Added**: 2026-01-25
**Related files**: `crates/mc-service/src/main.rs`, `crates/gc-service/src/main.rs`

Database and cache connection URLs often contain credentials (e.g., `redis://user:password@host:6379`). These URLs are commonly logged during startup for debugging ("Connecting to redis://..."). Never log the full URL. Pattern:

1. **Parse before logging**: Extract host/port only, not userinfo
2. **Use placeholder**: Log "Connecting to Redis at {host}:{port}" instead of full URL
3. **Structured logging**: If using structured logs, never include `url` field with credentials

Common locations where this appears: (1) `main.rs` startup logs, (2) Connection pool initialization, (3) Health check failure messages, (4) Configuration dump on startup. The `url` crate's `Url::host_str()` and `Url::port()` methods are safe; `Url::as_str()` or `to_string()` are not.

---

## Gotcha: Validation Scope for SecretBox Size Checks
**Added**: 2026-01-28
**Related files**: `crates/mc-service/src/actors/session.rs`

When validating SecretBox size (e.g., asserting minimum length for HKDF), call `.expose_secret().len()` ONLY for the validation. The exposed bytes should not be stored, manipulated, or copied outside the validation context. Pattern:

```rust
// CORRECT: Validation only
assert!(master_secret.expose_secret().len() >= 32, "...");

// INCORRECT: Storing or manipulating exposed bytes
let raw_bytes = master_secret.expose_secret(); // Don't store this!
```

**Why**: Calling `.expose_secret()` returns a reference valid only for the current scope. If stored in a local variable, the reference could be reused after the SecretBox mutates or drops, creating a use-after-free. Keep `.expose_secret()` calls inline with their usage (validation, HKDF input, HMAC computation).

---

## Gotcha: Cross-Crate Metrics Dependencies Create Observability Gaps
**Added**: 2026-02-09 (Updated: 2026-02-15)
**Related files**: `crates/gc-service/src/observability/metrics.rs`, `crates/common/src/token_manager.rs`

Shared crates (e.g., `common`) cannot depend on service-specific observability modules (e.g., `global-controller/observability`). This creates gaps where security-relevant operations in shared code lack metrics.

**Resolved for TokenManager**: The callback mechanism was implemented (TD-GC-001 closed). `TokenManagerConfig::with_on_refresh()` accepts an `Arc<dyn Fn(TokenRefreshEvent) + Send + Sync>`. GC injects a closure in `main.rs` that calls `metrics::record_token_refresh()`.

**Critical security boundary**: The `error_category()` function in `token_manager.rs` maps `TokenError` variants to `&'static str` constants. This is the security boundary between raw error messages (which could contain URLs, status codes, or response body fragments) and bounded Prometheus labels. When reviewing future changes to `TokenError`, verify that `error_category()` is updated to match and still returns only `&'static str`.

**For other shared crates**: The same pattern applies. Solutions by complexity:
1. **Callback mechanism**: Pass a metrics callback to shared code (proven pattern)
2. **Metrics trait**: Define trait in `common`, implement in service
3. **Feature flag**: Service-specific metrics behind compile-time feature

**Security implication**: Missing metrics means no alerting on token refresh failures, which could indicate credential compromise or AC unavailability. When reviewing shared crate changes, explicitly ask: "What security-relevant operations lack observability?"

---

## Gotcha: Credential Fallbacks Bypass Fail-Fast Security
**Added**: 2026-01-31
**Related files**: `crates/gc-service/src/main.rs`

Using `.unwrap_or_default()`, `.unwrap_or("")`, or similar fallback patterns on required credentials silently allows services to start with empty/invalid authentication. Pattern to avoid:

```rust
// DANGEROUS: Service starts with empty token if env var missing
let token = std::env::var("SERVICE_TOKEN").unwrap_or_default();

// DANGEROUS: Option<SecretString> with fallback to empty
let token = config.service_token.unwrap_or(SecretString::from(""));
```

**Impact**: Violates zero-trust architecture by allowing unauthenticated service-to-service calls. The service appears healthy but all outbound requests will fail authentication.

**Correct pattern**:
```rust
let token = std::env::var("SERVICE_TOKEN")
    .map_err(|_| "SERVICE_TOKEN is required")?;
```

**Detection during review**: Search for `.unwrap_or`, `.unwrap_or_default()`, `.unwrap_or_else(|| ...)` near `SecretString`, `service_token`, `api_key`, `bearer_token`, `GC_SERVICE_TOKEN`, `MC_SERVICE_TOKEN`. Each instance needs verification that the fallback value is acceptable (usually it's not for credentials).

---

## Gotcha: JWT Size Constants Must Be Consistent Across Services
**Added**: 2026-02-10
**Related files**: `crates/common/src/jwt.rs`, `crates/ac-service/src/crypto/mod.rs`

`MAX_JWT_SIZE_BYTES` is defined in `common::jwt` (8KB) and should be used consistently by all services. AC's `crypto/mod.rs` imports and uses this constant. Gotcha: If services define their own JWT size limits, they can diverge—AC might accept 8KB tokens while GC rejects them at 4KB, causing hard-to-debug failures.

**Pattern**: Always import from `common::jwt::MAX_JWT_SIZE_BYTES`, never redefine locally:
```rust
// CORRECT
use common::jwt::MAX_JWT_SIZE_BYTES;
if token.len() > MAX_JWT_SIZE_BYTES { ... }

// WRONG: Local redefinition can diverge
const MAX_JWT_SIZE: usize = 4096; // Different from common!
```

**Why 8KB?** Balance security (prevent DoS) with functionality (allow reasonable claim expansion). Typical JWTs are 200-500 bytes; 8KB is 16x typical size. Changed from 4KB to 8KB to align all services (AC, GC, MC).

**Detection during review**: Grep for `MAX_JWT_SIZE` or similar constants. Verify all JWT size checks use `common::jwt::MAX_JWT_SIZE_BYTES`.

---

## Gotcha: Staleness Threshold u64-to-i64 Cast Can Bypass Health Checks
**Added**: 2026-02-12
**Related files**: `crates/gc-service/src/tasks/generic_health_checker.rs`, `crates/gc-service/src/tasks/health_checker.rs`

The health checker functions accept `staleness_threshold_seconds: u64` but cast to `i64` before passing to the repository (`staleness_threshold_seconds as i64`). If the value exceeds `i64::MAX` (2^63 - 1), the cast wraps to a negative number, which would cause the SQL `NOW() - INTERVAL '{threshold} seconds'` comparison to look into the future, effectively marking ALL entities as stale (or none, depending on the query logic).

**Current risk**: LOW — the threshold comes from configuration, not user input, and realistic values are small (5-60 seconds). However, if configuration is ever loaded from environment variables without validation, a malformed value could trigger this.

**Proper fix** (future tech debt): Validate at the boundary:
```rust
let threshold: i64 = staleness_threshold_seconds
    .try_into()
    .map_err(|_| GcError::BadRequest("staleness threshold too large".into()))?;
```

Or use `i64` throughout the API. This is pre-existing tech debt, not introduced by the generic refactoring.

---

## Gotcha: `.instrument()` vs `#[instrument]` — Different Security Implications
**Added**: 2026-02-12
**Related files**: `crates/gc-service/src/tasks/health_checker.rs`, `crates/gc-service/src/tasks/mh_health_checker.rs`

These two tracing patterns have different security behavior:

- **`#[instrument]`** (proc macro): Automatically captures ALL function parameters as span fields unless `skip_all` or `skip(param)` is used. This is where credential leakage happens if `skip_all` is forgotten on functions receiving `PgPool`, tokens, or secrets.

- **`.instrument(info_span!("name"))`** (method chaining): Creates a named span and attaches it to a future. Does NOT auto-capture any variables from the surrounding scope. Only captures what you explicitly put in the `info_span!()` macro.

**Implication for security reviews**: When a function has NO `#[instrument]` attribute but uses `.instrument()` chaining on an inner call, there is no parameter auto-capture risk. The absence of `#[instrument]` is itself sufficient — you do not need `skip_all` because there is nothing to skip.

**Common false alarm**: Seeing a function that takes `PgPool` without `#[instrument(skip_all)]` and flagging it. If the function never has `#[instrument]`, parameters are not captured. Only flag if `#[instrument]` is present WITHOUT `skip_all`.

**Risk if someone adds `#[instrument]` later**: If a developer adds `#[instrument]` to the wrapper for debugging and forgets `skip_all`, `PgPool` would be captured. This is LOW risk because `PgPool`'s `Debug` impl does not contain credentials, but it's worth noting in code review comments when this pattern is used.

---

## Gotcha: Service Rename Breaks 4-Layer Credential Chain
**Added**: 2026-02-16
**Related files**: `infra/kind/scripts/setup.sh`, `infra/services/gc-service/secret.yaml`, `infra/services/gc-service/deployment.yaml`, `infra/services/mc-service/secret.yaml`, `infra/services/mc-service/deployment.yaml`

Renaming a service's OAuth client_id requires synchronized changes across 4 layers, and a mismatch at any layer causes silent authentication failures:

1. **SQL seed** (`setup.sh`): `client_id` column + new bcrypt hash for new plaintext secret
2. **K8s Secret** (`secret.yaml`): Plaintext secret value matching what was hashed
3. **K8s Deployment** (`deployment.yaml`): `CLIENT_ID` env var matching the SQL `client_id`
4. **K8s Deployment** (`deployment.yaml`): `CLIENT_SECRET` env var referencing the correct Secret key

**Why this is dangerous**: If the `client_id` in the deployment doesn't match the SQL seed, the service starts successfully but all OAuth token requests fail with `invalid_client`. The service appears healthy (health probes pass) but cannot authenticate to other services. This violates zero-trust silently.

**What happened**: The semantic guard caught a mismatch between the credential SQL and the secret.yaml values during this rename. Without that guard, the credentials would have been inconsistent.

**Detection during review**: For any rename touching OAuth credentials, verify the chain: `deployment CLIENT_ID` == `setup.sh INSERT client_id` AND `secret.yaml plaintext` hashes to `setup.sh client_secret_hash`. The bcrypt hashes MUST be regenerated when the plaintext secret changes.

---
