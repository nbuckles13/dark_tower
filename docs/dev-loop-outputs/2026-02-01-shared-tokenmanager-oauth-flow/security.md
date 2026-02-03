# Security Review: TokenManager Implementation

**Reviewer**: Security Specialist
**Date**: 2026-02-02
**Files Reviewed**:
- `crates/common/src/token_manager.rs`
- `crates/common/Cargo.toml`
- `crates/common/src/lib.rs`

---

## Summary

The TokenManager implementation provides OAuth 2.0 client credentials flow with automatic token refresh. The implementation follows several security best practices including proper secret handling with `SecretString`, custom Debug implementations that redact sensitive data, and appropriate HTTP timeout configuration. However, there are security issues that require attention before approval.

---

## Positive Security Highlights

1. **Proper Secret Handling** (lines 117-119, 177, 441):
   - `client_secret` stored as `SecretString` - prevents accidental logging
   - Token stored as `SecretString` internally in watch channel
   - `TokenReceiver` and `TokenManagerConfig` have custom Debug impls that redact secrets

2. **HTTP Timeout Configuration** (lines 262-264):
   - Both `timeout()` and `connect_timeout()` are set - prevents hanging connections
   - Default 10s request timeout, 5s connect timeout - reasonable values

3. **Logging Safety** (lines 325-345, 352-358, 397-403):
   - Token acquisition/refresh events logged without values
   - Uses structured logging with `target` for filtering
   - `client_id` logged (safe identifier), not secrets

4. **Background Task Design**:
   - Watch channel provides thread-safe access without Arc<Mutex<>>
   - `TokenReceiver::token()` clones immediately to prevent lock contention

5. **TLS by Default** (Cargo.toml line 30):
   - reqwest configured with `rustls-tls` feature only - no native-tls fallback
   - HTTPS enforcement at HTTP client level

---

## Findings

### MAJOR Security Issues

#### 1. **Response Body May Contain Tokens in Error Path** - `token_manager.rs:443-451`

**Issue**: When authentication is rejected (401/400), the response body is read and included in the error message. If AC returns verbose error messages that echo back the token or credentials (common in some OAuth implementations), this could leak sensitive data in logs.

```rust
let body = response.text().await.unwrap_or_default();
warn!(
    target: "common.token_manager",
    status = %status,
    "Authentication rejected by AC"
);
Err(TokenError::AuthenticationRejected(format!(
    "Status {status}: {body}"
)))
```

**Threat**: Error bodies are logged by callers via Display/Debug on `TokenError`. The `body` may contain:
- Echoed credentials in verbose error messages
- Internal AC implementation details
- JWT fragments in "invalid token" errors

**OWASP/CWE**: CWE-209 (Generation of Error Message Containing Sensitive Information)

**Fix**: Do not include the raw response body in errors. Log it server-side at DEBUG level only:
```rust
let body = response.text().await.unwrap_or_default();
debug!(
    target: "common.token_manager",
    status = %status,
    body_len = body.len(),
    "Authentication rejected - body logged at trace level"
);
tracing::trace!(target: "common.token_manager", body = %body);
Err(TokenError::AuthenticationRejected(format!("Status {status}")))
```

---

#### 2. **No HTTPS Enforcement for AC Endpoint** - `token_manager.rs:143-146, 396`

**Issue**: The `ac_endpoint` URL is accepted without validation. A user could configure `http://...` which would transmit `client_secret` in plaintext over the network.

```rust
pub fn new(ac_endpoint: String, client_id: String, client_secret: SecretString) -> Self {
    Self {
        ac_endpoint,  // No validation!
        // ...
    }
}
```

**Threat**: Man-in-the-middle attack can capture client credentials if HTTP is used instead of HTTPS.

**OWASP/CWE**: CWE-319 (Cleartext Transmission of Sensitive Information)

**Fix**: Validate that the endpoint uses HTTPS scheme:
```rust
pub fn new(ac_endpoint: String, client_id: String, client_secret: SecretString) -> Result<Self, TokenError> {
    if !ac_endpoint.starts_with("https://") {
        return Err(TokenError::Configuration(
            "AC endpoint must use HTTPS".into()
        ));
    }
    Ok(Self { /* ... */ })
}
```

Alternatively, allow HTTP only in development via a feature flag or explicit opt-in.

---

### MINOR Security Issues

#### 3. **OAuthTokenResponse Deserializes access_token as Plain String** - `token_manager.rs:218-228`

**Issue**: The `access_token` field is deserialized as `String` before being wrapped in `SecretString`. While short-lived, the token exists as a plain String during JSON parsing.

```rust
#[derive(Debug, Deserialize)]
struct OAuthTokenResponse {
    access_token: String,  // Plain String
    // ...
}
```

**Threat**: If the struct is logged via Debug (currently not done, but possible in future code), the token would be exposed. The derived `Debug` includes `access_token`.

**OWASP/CWE**: CWE-200 (Exposure of Sensitive Information)

**Fix**: Either:
1. Add custom Debug impl that redacts `access_token`:
```rust
impl std::fmt::Debug for OAuthTokenResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OAuthTokenResponse")
            .field("access_token", &"[REDACTED]")
            .field("token_type", &self.token_type)
            .field("expires_in", &self.expires_in)
            .field("scope", &self.scope)
            .finish()
    }
}
```
2. Or deserialize directly into SecretString (requires custom deserializer)

---

#### 4. **Clock Comparison Without Drift Tolerance** - `token_manager.rs:307-311, 431-433`

**Issue**: Token expiration is calculated using `chrono::Utc::now()` without considering clock drift between the client and AC server. If clocks are not synchronized, tokens may be considered valid when expired (or vice versa).

```rust
let now = chrono::Utc::now().timestamp();
let expires_at = now + token_response.expires_in as i64;
```

**Threat**: If AC's clock is ahead of TokenManager's clock, the token may actually expire before the calculated `expires_at`. This could cause authentication failures that are hard to diagnose.

**OWASP/CWE**: CWE-367 (Time-of-check Time-of-use Race Condition)

**Fix**: Apply a safety margin (already partially addressed by refresh_threshold, but consider additional buffer):
- Document that clock synchronization (NTP) is required
- Or reduce `expires_in` by a small margin (e.g., 30 seconds) when calculating `expires_at`

---

### TECH_DEBT (Non-Blocking)

#### 5. **No Token Size Validation** - `token_manager.rs:441`

**Issue**: The received token is not validated for size before storage. While unlikely from a trusted AC, an extremely large token could cause memory issues.

**Recommendation**: Add a size check per JWT principles (8KB max):
```rust
if token_response.access_token.len() > 8192 {
    return Err(TokenError::InvalidResponse("Token exceeds 8KB limit".into()));
}
```

---

#### 6. **Missing #[instrument(skip_all)] on Internal Functions** - `token_manager.rs:295, 392`

**Issue**: The `token_refresh_loop` and `acquire_token` functions handle secrets but don't use `#[instrument(skip_all)]`. While they use manual tracing calls (which is fine), adding the annotation would provide defense-in-depth if tracing is accidentally enabled at a higher level.

**Recommendation**: Add `#[instrument(skip_all)]` to crypto/secret-handling functions per established patterns.

---

## Security Review Checklist

| Category | Status | Notes |
|----------|--------|-------|
| Authentication & Authorization | N/A | This is a client component |
| Cryptography | PASS | Uses SecretString, no custom crypto |
| Input Validation | FAIL | AC endpoint URL not validated for HTTPS |
| Secrets Management | PASS | Proper SecretString usage |
| Error Handling | FAIL | Response body may leak in errors |
| Data Protection | PASS | TLS enforced via rustls-tls |
| Rate Limiting & DoS | PASS | HTTP timeouts configured |
| Timing Attacks | N/A | No secret comparisons |

---

## Recommendation

**VERDICT**: REQUEST_CHANGES

The implementation is well-designed with proper secret handling, but requires fixes for:

1. **MAJOR**: Response body in `AuthenticationRejected` error may leak sensitive data - remove raw body from error message
2. **MAJOR**: No HTTPS enforcement for AC endpoint - validate URL scheme before transmitting credentials
3. **MINOR**: `OAuthTokenResponse` has derived Debug that would expose `access_token` - add custom Debug impl
4. **MINOR**: Clock drift not considered in expiration calculation - document NTP requirement or add safety margin

After addressing MAJOR and MINOR issues, the implementation will be secure for production use.

---

## Verdict Summary

```
verdict: REQUEST_CHANGES
finding_count:
  blocker: 0
  critical: 0
  major: 2
  minor: 2
  tech_debt: 2
checkpoint_exists: true
summary: TokenManager has proper SecretString usage and HTTP timeouts but requires fixes for: (1) response body leak in error messages, (2) no HTTPS enforcement for AC endpoint, (3) OAuthTokenResponse Debug exposes token, (4) clock drift not considered in expiration.
```
