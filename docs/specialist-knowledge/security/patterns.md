# Security Specialist Patterns

Security review patterns and best practices for the Dark Tower codebase.

---

## Pattern: Defense-in-Depth Validation
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/crypto/mod.rs`

Security-critical functions should re-validate parameters even when callers are trusted. Example: `hash_client_secret()` checks bcrypt cost is within safe range despite config validation. Prevents misconfiguration if function called from unexpected paths.

---

## Pattern: Configurable Security with Safe Bounds
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/config.rs`

Security parameters should be configurable but bounded. Pattern: MIN (security floor), DEFAULT (recommended), MAX (safety ceiling). Reject values outside range at startup. Warn on values below default but above MIN.

---

## Pattern: Security Review Checklist
**Added**: 2026-01-11
**Related files**: `.claude/agents/security.md`

When reviewing security code, check: (1) Timing attack vectors, (2) Error message information leakage, (3) Input validation at boundaries, (4) Crypto parameter bounds, (5) Key/secret handling, (6) Logging sanitization, (7) `#[instrument(skip_all)]` on crypto functions, (8) Custom Debug on secret-holding types.

---

## Pattern: Tracing-Safe Crypto Functions
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/crypto/mod.rs`

All functions handling secrets MUST use `#[instrument(skip_all)]` to prevent tracing from capturing sensitive parameters in spans. Types holding crypto material need manual Debug impl with `[REDACTED]` fields, or use `secrecy::Secret<T>` wrapper. This is a MANDATORY check when reviewing any crypto-adjacent code.

---

## Pattern: SecretBox/SecretString for Compile-Time Secret Safety
**Added**: 2026-01-12
**Related files**: `crates/ac-service/src/config.rs`, `crates/ac-service/src/crypto/mod.rs`, `crates/ac-service/src/models/mod.rs`

Use `SecretBox<T>` (binary data) and `SecretString` (text) from `secrecy` crate for all secrets. Benefits: (1) Debug auto-redacts as `[REDACTED]`, (2) `.expose_secret()` makes access explicit and grep-able, (3) Zeroization on drop. Use `SecretBox<Vec<u8>>` for keys, `SecretString` for passwords/tokens. Types with derived Debug that contain secrets automatically get safe logging.

---

## Pattern: Intentional Secret Exposure via Custom Serialize
**Added**: 2026-01-12
**Related files**: `crates/ac-service/src/models/mod.rs`, `crates/ac-service/src/handlers/admin_handler.rs`

For "one-time reveal" API responses (registration, secret rotation), implement custom `Serialize` that calls `.expose_secret()`. This is the ONLY place secrets should be exposed. Pattern: (1) Custom Debug that redacts, (2) Custom Serialize that exposes for API response, (3) Document as intentional in comments. Example: `RegisterServiceResponse`, `RotateSecretResponse`.

---

## Pattern: JWKS Private Key Field Validation
**Added**: 2026-01-13
**Related files**: `crates/env-tests/tests/25_auth_security.rs`

When testing JWKS endpoints for private key leakage, check for ALL private key fields that could be present: `d` (private key for RSA/EC/OKP), `p`, `q`, `dp`, `dq`, `qi` (RSA CRT parameters). Use raw JSON parsing rather than typed deserialization to catch any field that shouldn't be there. This validates CWE-321 (cryptographic key exposure).

---

## Pattern: Query Timeout via Connection URL Parameters
**Added**: 2026-01-14
**Related files**: `crates/gc-service/src/main.rs`

Prevent hung queries and DoS attacks by setting database statement_timeout at connection time, not per-query. Pattern: append `?options=-c%20statement_timeout%3D{seconds}` to the PostgreSQL connection URL. This ensures ALL queries timeout after N seconds, preventing resource exhaustion. Combine with application-level request timeout for defense-in-depth. Set timeout low enough (e.g., 5 seconds) to catch expensive operations, high enough for legitimate slow queries.

---

## Pattern: JWK Field Validation as Defense-in-Depth
**Added**: 2026-01-14
**Related files**: `crates/gc-service/src/auth/jwt.rs`

JWT validation includes algorithm pinning (token must have `alg: EdDSA`), but defense-in-depth also requires validating JWK fields: (1) `kty` (key type) must be `"OKP"` (Octet Key Pair) for Ed25519 keys, (2) `alg` field in JWK, if present, must be `"EdDSA"`. This prevents accepting keys from wrong cryptosystems. Pattern: Validate JWK fields at start of token verification before any crypto operations.

---

## Pattern: Error Body Sanitization for Credential Protection
**Added**: 2026-01-18
**Related files**: `crates/env-tests/src/fixtures/gc_client.rs`

HTTP error responses can contain credentials (JWTs in error messages, Bearer tokens in auth headers). Sanitize error bodies at capture time using regex pattern matching:
1. JWT pattern: `eyJ[A-Za-z0-9_-]+\.eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+` -> `[JWT_REDACTED]`
2. Bearer pattern: `Bearer\s+eyJ[A-Za-z0-9_-]+` -> `[BEARER_REDACTED]`
3. Truncate long bodies (>256 chars) to limit info disclosure

This provides defense-in-depth beyond custom Debug implementations, catching credentials in assertion output, Display formatting, and log messages.

---

## Pattern: External Resource Registration Validation
**Added**: 2026-01-24 (Updated: 2026-01-31)
**Related files**: `crates/gc-service/src/grpc/mh_service.rs`, `crates/gc-service/src/repositories/media_handlers.rs`

When services register external resources (handlers, endpoints, callback URLs), validate both identifier format AND URL security:

1. **Identifier format validation**: Use allowlist regex patterns (e.g., `^[a-zA-Z0-9_-]+$` for handler IDs). Reject inputs with path traversal, null bytes, or injection characters. Short max lengths (64-128 chars) prevent DoS via long identifiers.

2. **Endpoint URL validation**: Require HTTPS scheme (reject HTTP, FTP, file://). Validate URL parsability. Consider allowlisting domains/IP ranges for internal services. Reject localhost/127.0.0.1 in production to prevent SSRF to internal services.

This pattern applies to: Media Handler registration, webhook callbacks, federation endpoints, any user-supplied URLs stored for later use.

---

## Pattern: Authorization State Separation with Audit Trail
**Added**: 2026-01-25
**Related files**: `docs/decisions/adr-0023-meeting-controller-architecture.md`

When multiple actors can affect the same state (e.g., mute), maintain separate state per actor:
1. **Self-initiated state**: User controls (e.g., `self_muted: bool`)
2. **Host-initiated state**: Admin/host controls (e.g., `host_muted: bool`)
3. **Effective state**: Computed from both (muted if either is true)

Benefits: (1) Clear audit trail - who caused the mute, (2) Proper restoration - self-unmute doesn't override host-mute, (3) Authorization clarity - different permission checks per actor. Store `muted_by` enum or field for audit: `Self`, `Host`, `System`. This pattern applies to: mute/unmute, visibility, permissions, feature access controlled by multiple authorities.

---

## Pattern: HKDF Key Derivation for Scoped Tokens
**Added**: 2026-01-25 (Updated: 2026-02-10)
**Related files**: `docs/decisions/adr-0023-meeting-controller-architecture.md`

When generating tokens scoped to a resource (meeting, session, room), derive per-resource keys using HKDF rather than using a single master key directly:

1. **Master secret**: `MC_BINDING_TOKEN_SECRET` - service-level secret owned by MC (distinct from AC signing keys)
2. **Key derivation**: HKDF-SHA256 with resource ID as salt and constant info string:
```rust
meeting_key = HKDF-SHA256(
    ikm: master_secret,
    salt: meeting_id,
    info: b"session-binding"
)
```
3. **Token generation**: HMAC-SHA256 with derived key over (correlation_id || participant_id || nonce)
4. **Rotation schedule**: Master secret rotates on each MC deployment (~weekly). 1-hour grace period allows in-flight tokens during rolling deploys.

Benefits: (1) Compromise of one meeting's tokens doesn't reveal master secret, (2) Key material is deterministic - can regenerate without storage, (3) Follows cryptographic best practices for key hierarchy, (4) Per-meeting isolation limits blast radius. Use `ring::hkdf` for derivation.

**Security note (ADR-0023)**: Binding tokens are defense-in-depth, not primary authentication. A stolen binding token alone cannot hijack a session—attacker also needs a valid JWT for the same `user_id`. The 30-second TTL is primary protection; even with compromised master secret, attackers have only 30-second window and still need valid JWT.

---

## Pattern: gRPC Interceptor for Authorization Validation
**Added**: 2026-01-25 (Updated: 2026-01-30)
**Related files**: `crates/mc-service/src/grpc/auth_interceptor.rs`

Use gRPC interceptors (tonic middleware) to enforce authorization on incoming service-to-service calls. Pattern:

1. **Interceptor struct**: `McAuthInterceptor` holds AC client reference and required scopes
2. **Metadata extraction**: Extract `authorization` header from gRPC request metadata
3. **Token validation**: Validate JWT signature + claims via AC JWKS endpoint
4. **Scope enforcement**: Verify token contains required scope (e.g., `mc:assign`)
5. **Early rejection**: Return `Status::unauthenticated()` or `Status::permission_denied()` before handler runs

Benefits: (1) Centralized auth logic - not scattered in handlers, (2) Defense-in-depth - even if handler forgets auth, interceptor catches it, (3) Consistent error responses. Apply interceptor via `Server::builder().layer()` or per-service via `ServiceBuilder`.

---

## Pattern: Token Size Limits for DoS Prevention
**Added**: 2026-01-25 (Updated: 2026-01-30)
**Related files**: `crates/mc-service/src/grpc/auth_interceptor.rs`

Enforce maximum token size before parsing to prevent memory exhaustion attacks. Pattern:

1. **Size check first**: Before Base64 decode or JWT parse, check raw string length
2. **Limit**: 8KB (8192 bytes) is reasonable for JWTs - valid tokens are typically 1-2KB
3. **Early rejection**: Return error immediately if exceeded, before any parsing
4. **Logging**: Log oversized token attempt (without the token itself) for monitoring

```rust
const MAX_TOKEN_SIZE: usize = 8192; // 8KB
if token.len() > MAX_TOKEN_SIZE {
    return Err(Status::invalid_argument("token exceeds maximum size"));
}
```

This prevents: (1) Memory exhaustion from giant Base64 decode, (2) CPU exhaustion from parsing huge JSON claims, (3) Log injection via oversized tokens in error messages.

---

## Pattern: Multiple SecretBox Copies with Isolated Lifecycles
**Added**: 2026-01-28
**Related files**: `crates/mc-service/src/actors/controller.rs`, `crates/mc-service/src/actors/meeting.rs`, `crates/mc-service/src/actors/session.rs`

When distributing a master secret to multiple actor instances, create isolated SecretBox copies for each. Pattern:

1. **Central holder**: Controller or main actor holds original SecretBox
2. **Copy pattern**: When passing to child actors, use: `SecretBox::new(Box::new(self.master_secret.expose_secret().clone()))`
3. **Scope**: The `expose_secret()` call is immediately followed by `.clone()` and re-wrapped - minimal exposure window
4. **Independent lifecycle**: Each SecretBox independently zeroizes on drop
5. **Memory trade-off**: Multiple copies increase memory usage but provide isolation - acceptable for small actor counts (typical <100 concurrent meetings)

**Benefits**:
- Each actor can independently manage its secret copy
- Compromise of one actor's memory doesn't expose controller's master secret
- Each SecretBox independently zeroizes on drop
- Clear separation of concerns

**Cost**: Additional memory proportional to number of actors (typically negligible: N actors × 32 bytes = ~3KB for 100 meetings)

---

## Pattern: Explicit Instrument Field Allowlists for Privacy-by-Default
**Added**: 2026-01-28
**Related files**: `crates/gc-service/src/auth/jwt.rs`, `crates/gc-service/src/middleware/auth.rs`, `crates/gc-service/src/services/ac_client.rs`

Use `#[instrument(skip_all, fields(...))]` with explicit field allowlists rather than `skip_all` alone. Pattern:

1. **Skip sensitive parameters**: Use `skip_all` to prevent automatic parameter capture
2. **Allowlist safe fields**: Explicitly list only safe identifiers: `meeting_id`, `user_id`, `region`, `kid`
3. **Never include**: tokens, credentials, authorization headers, private keys, database URLs
4. **Server-side only**: Even "safe" fields are only for server-side tracing, not client responses

**Example**:
```rust
#[instrument(skip_all, fields(meeting_id = %request.meeting_id, user_id = %request.user_id))]
async fn request_meeting_token(&self, request: &MeetingTokenRequest) -> Result<TokenResponse>
```

**Benefits**: Privacy-by-default prevents accidental credential logging when functions are refactored or parameters are added. Explicit allowlists make it clear which fields are safe to trace.

---

## Pattern: Simple Guards for Fast Credential Leak Detection
**Added**: 2026-02-10
**Related files**: `scripts/guards/simple/no-secrets-in-logs.sh`, `scripts/guards/simple/instrument-skip-all.sh`

Use grep-based simple guards as first-pass checks in CI/development to catch obvious credential leaks before code review. Pattern:

1. **Check #[instrument] without skip**: Flag functions with secret-sounding parameters (`password`, `secret`, `token`, `key`, `credential`) that lack `skip_all` or specific skip annotations
2. **Check expose_secret() in log macros**: Detect `info!`, `debug!`, `warn!`, `error!`, `trace!` calls containing `.expose_secret()`, which defeats `SecretString` protection
3. **Check secret variable names in logs**: Pattern match log macro arguments against common secret variable names (password, token, secret, key, etc.)
4. **Filter test code**: Guards should skip `#[cfg(test)]`, `mod tests {`, and test files to avoid false positives

**Example violation caught**:
```rust
// VIOLATION: password parameter without skip
#[instrument]
pub fn hash_client_secret(secret: &str, cost: u32) -> Result<String> { ... }

// FIX: Add skip_all
#[instrument(skip_all)]
pub fn hash_client_secret(secret: &str, cost: u32) -> Result<String> { ... }
```

**Benefits**: (1) Fast execution (grep-based, no compilation), (2) Catches common mistakes before review, (3) Supplements semantic guards for defense-in-depth, (4) Educational—developers learn patterns by seeing violations. Guards run in CI and can be invoked manually during development.

**Limitation**: Simple guards have false negatives (complex cases) and false positives (names that sound like secrets but aren't). Use semantic guards (AST-based analysis) for comprehensive checks.

---

## Pattern: Server-Side Error Context with Generic Client Messages
**Added**: 2026-01-28 (Updated: 2026-01-29)
**Related files**: `crates/gc-service/src/errors.rs`, `crates/gc-service/src/handlers/meetings.rs`, `crates/gc-service/src/grpc/mc_service.rs`, `crates/ac-service/src/crypto/mod.rs`

Preserve error context for debugging via server-side logging while returning generic messages to clients. Pattern:

1. **Error type with context**: Enum variant accepts String: `Internal(String)`, `Database(String)`
2. **Server-side logging**: In `IntoResponse` impl, log the full error with `tracing::error!(reason = %reason, ...)`
3. **Client response**: Return only generic message: `"An internal error occurred"`
4. **Context in logs**: Include operational context (parse error type, service name) but NEVER secrets

**Example**:
```rust
// Error handling pattern
.map_err(|e| {
    tracing::error!(target: "crypto", error = %e, "Operation failed");
    AcError::Crypto("Generic message".to_string())
})
```

**Safe library errors**: Crypto library errors (ring, bcrypt, jsonwebtoken) are safe to log via `error = %e` because they only indicate operation failure type (KeyRejected, InvalidSignature), not key material or plaintext content. Configuration parsing errors (base64::DecodeError) are also safe - they indicate format issues, not the value being parsed.

**Benefits**: Debugging gets full context, clients get minimal info (prevents enumeration/info disclosure). Common pattern for database errors, service communication failures, parsing errors, crypto operations.

---

## Pattern: Constructor Variants for Security Enforcement
**Added**: 2026-02-02
**Related files**: `crates/common/src/token_manager.rs`

When a configuration accepts potentially insecure values (HTTP URLs, weak parameters), provide two constructors: (1) `new()` - permissive, with clear security warnings in documentation, allows HTTP for local development/testing; (2) `new_secure()` - enforcing, returns `Result` with error on security violations (e.g., non-HTTPS URLs). This pattern enables secure-by-default in production while allowing flexibility in development. The `new_secure()` variant should be the recommended constructor in documentation.

---

## Pattern: Observability Asset Security Review
**Added**: 2026-02-08
**Related files**: `infra/grafana/dashboards/*.json`, `infra/docker/prometheus/rules/*.yaml`, `docs/runbooks/*.md`

When reviewing observability assets (dashboards, alerts, runbooks), check for:

1. **Dashboard queries**: Verify PromQL uses only bounded labels (endpoint, status_code, operation), never unbounded PII labels (user_id, meeting_id, email, IP). Aggregate with `sum by(label)` to control cardinality.

2. **Alert annotations**: Annotations should contain only metric values (`{{ $value }}`) and infrastructure identifiers (pod names). Never include user/session identifiers. Pod names like `global-controller-7d9f5b8c4d-abc12` are infrastructure identifiers, not PII.

3. **Runbook commands**: All shell commands should use environment variables for credentials (`$DATABASE_URL`, `${GC_CLIENT_SECRET}`), never hardcoded values. Password placeholders should be obvious (`<password>`, `REPLACE_PASSWORD`). Check for command injection via variable expansion.

4. **Runbook URLs in alerts**: Use HTTPS, section anchors are safe. Placeholder org names need deployment-time replacement.

5. **Panel descriptions and catalog docs**: Dashboard panel `description` fields and metric catalog Markdown are visible to anyone with Grafana/repo access. Do not explain *why* a security mitigation exists (e.g., "coarse buckets to prevent timing side-channel attacks"). State *what* the metric measures, not the threat model behind its design. Security rationale belongs in ADRs and specialist knowledge files, not in operator-facing dashboards.

6. **Catalog documentation scope**: Metric catalog files (`docs/observability/metrics/*.md`) should document metric type, labels, cardinality, and usage. Do NOT document: rotation grace periods, rate limit thresholds/windows, binding token TTL, key derivation parameters, or any security policy constants. An attacker with repo read access could use these to time exploitation windows.

This pattern applies to all service observability: GC, AC, MC, MH dashboards and runbooks.

---

## Pattern: Metric Label Security for Cardinality and Privacy
**Added**: 2026-02-09
**Related files**: `crates/gc-service/src/observability/metrics.rs`, `crates/gc-service/src/services/mh_selection.rs`

When instrumenting code with metrics (Prometheus counters/histograms), enforce cardinality bounds and privacy at the metric recording site:

1. **Label value bounding**: Use only enum-derived or fixed string values for labels. Never use unbounded strings (user IDs, meeting codes, raw error messages). Example: `status: "success" | "error" | "timeout"`, not `status: error.to_string()`.

2. **Dynamic path normalization**: For endpoint labels, normalize dynamic segments to placeholders BEFORE recording: `/api/v1/meetings/{code}` not `/api/v1/meetings/abc123`. Implement `normalize_endpoint()` function at the metrics layer.

3. **Boolean labels as strings**: Prometheus labels are strings, so boolean flags become `"true"/"false"`. This is inherently bounded (2 values).

4. **Maximum cardinality calculation**: Document expected cardinality per metric. Example: `gc_mh_selections_total` with `status` (3 values) x `has_backup` (2 values) = 6 unique combinations max.

5. **Error semantics preservation**: Record metrics BEFORE error propagation (`?` operator). Pattern:
   ```rust
   let result = operation().await;
   let status = if result.is_ok() { "success" } else { "error" };
   metrics::record_operation(status, start.elapsed());
   result? // Propagate after recording
   ```

**ADR-0011 limits**: Max 1,000 unique label combinations per metric, 5M total time series. Exceeding causes Prometheus scrape failures or memory exhaustion.

---

## Pattern: Test Infrastructure Security (Mock Credentials)
**Added**: 2026-01-31
**Related files**: `crates/mc-service/tests/gc_integration.rs`, `crates/gc-service/tests/meeting_tests.rs`

Test infrastructure (mocks, fixtures, integration tests) should use obviously fake credentials to prevent confusion with production values. Pattern:

1. **Naming convention**: Use prefixes/suffixes that clearly indicate test usage: `test-service-token`, `test-mc-001`, `test-secret`
2. **Base64 test secrets**: If encoding required, use base64 of human-readable strings: `dGVzdC1zZWNyZXQ=` (base64 of "test-secret")
3. **Localhost URLs**: Use `redis://localhost:6379`, `http://127.0.0.1:50051` for connection strings
4. **Mock behavior**: Test mocks should NOT validate actual credentials (appropriate for unit tests), but should validate structural format (token format, URL parsability)
5. **SecretString wrapping**: Even in tests, wrap credentials in `SecretString` to validate production code paths

**Example**:
```rust
Config {
    service_token: SecretString::from("test-service-token"),
    binding_token_secret: SecretString::from("dGVzdC1zZWNyZXQ="),
    redis_url: SecretString::from("redis://localhost:6379"),
    // ...
}
```

**Benefits**: (1) No risk of credential leakage if test code is committed, (2) Clear separation from production values, (3) Validates production code paths without security bypass. Common mistake: using production-like secrets in tests (e.g., realistic-looking JWTs, actual API keys from docs).

---

## Pattern: Bounded Label Values for Cardinality Safety in Observability
**Added**: 2026-02-05 (Updated: 2026-02-10)
**Related files**: `crates/mc-service/src/actors/metrics.rs`, `crates/mc-service/src/observability/metrics.rs`

Prometheus metrics with labels must use bounded, enumerated values to prevent cardinality explosion that can crash monitoring systems. Pattern:

1. **Enum-backed labels**: Use Rust enums for label values, implement `as_str()` returning `&'static str`. Guarantees only valid values exist. Example: `ActorType::Controller`, `ActorType::Meeting`, `ActorType::Connection` → 3 max values. The enum variant methods ensure compile-time enforcement:
```rust
impl ActorType {
    pub const fn as_str(&self) -> &'static str {
        match self { /* bounded variants */ }
    }
}
```

2. **No user-supplied labels**: Never use user IDs, meeting IDs, session IDs, or any unbounded identifiers as label values. These should be stored internally (in tracing logs if needed) but NOT in Prometheus.

3. **Aggregate metrics only**: Track counts and aggregates (active meetings total, connections total) without per-entity labels. Example: `mc_meetings_active: 42` (gauge, no labels) not `mc_meetings_active{meeting_id="abc"}: 1` (unbounded cardinality).

4. **Label documentation**: Document all labels and their allowed values in metric function comments, with cardinality count. Pattern:
```rust
/// Metric: `mc_actor_mailbox_depth`
/// Labels: `actor_type` (controller, meeting, connection)
/// Cardinality: 3 (bounded by ActorType enum)
pub fn set_actor_mailbox_depth(actor_type: &str, depth: usize) { ... }
```

5. **Type-safe callsites**: Callers pass enums, conversion to `&str` happens at callsite: `prom::record_actor_panic(actor_type.as_str())`. This makes valid values explicit at every usage point.

**Benefits**: (1) Prevents memory exhaustion in Prometheus from infinite label combinations, (2) Makes cardinality bounds explicit and auditable, (3) Prevents accidental PII exposure via labels, (4) Improves observability system reliability.

**Common mistake**: Accepting arbitrary string parameters: `record_metric("some_label", value)` where callers pass user IDs. The fix: Use enums with `as_str()` conversion enforced at type level.

**Implementation note**: Internal tracking structures (like `MailboxMonitor`) store actor IDs (e.g., meeting IDs) for tracing logs, but these are NOT passed to Prometheus metric functions. The separation ensures operational debugging without cardinality explosion.

---

## Pattern: Security Review of Generic Abstractions (Refactoring)
**Added**: 2026-02-12 (Updated: 2026-02-12)
**Related files**: `crates/gc-service/src/tasks/generic_health_checker.rs`, `crates/gc-service/src/tasks/health_checker.rs`

When reviewing refactorings that extract generic/shared abstractions from concrete implementations, verify these security properties are preserved:

1. **Tracing instrumentation safety**: Two patterns are security-equivalent for preventing parameter capture:
   - `#[instrument(skip_all)]` on a function — explicitly skips all parameters from the tracing span
   - `.instrument(tracing::info_span!("name"))` chained on the function's future — does NOT auto-capture any variables from the caller's scope (only `#[instrument]` proc macro does auto-capture)

   Either pattern is safe. When reviewing, the key question is: "Is there an `#[instrument]` (without `skip_all`) on a function that receives `PgPool`, tokens, or other infrastructure types?" If no `#[instrument]` attribute exists at all, parameters are not captured regardless of `.instrument()` chaining.

2. **Config field types**: Generic abstractions that accept configuration (log targets, entity names, display strings) should use `&'static str` rather than `String` to ensure compile-time values only. This eliminates any possibility of user-supplied input flowing into log targets or format strings.

3. **Closure capture analysis**: When closures are passed as function parameters, verify they capture nothing sensitive from the enclosing scope. The closure parameters themselves (what the generic function passes in) should also be non-sensitive.

4. **Error type preservation**: The generic function should use the same error type as the concrete implementations. Introducing a new generic error type could change what information is logged via `Display`/`Debug` formatting.

5. **Public API surface**: Verify that wrapper functions maintain identical signatures — the refactoring should be invisible to callers, adding no new attack surface. Removing types from the public API (e.g., config structs) is a net positive.

6. **`.instrument()` span contents**: When `.instrument(info_span!(...))` is used, verify the span macro uses only literal strings with no dynamic fields that could accept user input. Example safe pattern: `tracing::info_span!("gc.task.health_checker")`.

**Quick review checklist for refactoring PRs**:
- [ ] No new `expose_secret()` calls
- [ ] No `#[instrument]` without `skip_all` on functions receiving `PgPool` or sensitive types (`.instrument()` chaining is safe — it does not auto-capture)
- [ ] `.instrument()` spans contain only literal names, no dynamic fields
- [ ] No new public API surface
- [ ] No new SQL or query changes
- [ ] Error handling chain unchanged
- [ ] Test coverage preserved (same tests, testing through wrappers)

---

## Pattern: Bounded Error Labels via `&'static str` Match Arms
**Added**: 2026-02-15
**Related files**: `crates/global-controller/src/errors.rs`, `crates/common/src/token_manager.rs`

When wiring error types into Prometheus metrics, the error variant must be mapped to a bounded `&'static str` label — never the error's `Display`/`to_string()` output, which can contain dynamic content (URLs, status codes, response fragments).

Two implementations of this pattern now exist in the codebase:

1. **`GcError::error_type_label()`** (`errors.rs`): Maps each `GcError` enum variant to a static string like `"database"`, `"forbidden"`, `"service_unavailable"`. Used by `record_error()` in AC client and MC client metric callsites.

2. **`error_category()`** (`token_manager.rs`): Maps each `TokenError` variant to a static string like `"http"`, `"auth_rejected"`. Used by `TokenRefreshEvent` callback.

**Security review rule**: When a new variant is added to `GcError` or `TokenError`, the corresponding `&'static str` match arm MUST be added. A missing arm is a compile error (exhaustive match), but the label value chosen must also be reviewed for information leakage — it should be a generic category name, not a value derived from user input or error content.

**Anti-pattern**: `record_error("operation", &err.to_string(), status)` — this passes the full error message as a Prometheus label, causing both cardinality explosion and potential information leakage.

---
