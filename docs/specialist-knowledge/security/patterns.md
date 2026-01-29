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
**Related files**: `crates/global-controller/src/main.rs`

Prevent hung queries and DoS attacks by setting database statement_timeout at connection time, not per-query. Pattern: append `?options=-c%20statement_timeout%3D{seconds}` to the PostgreSQL connection URL. This ensures ALL queries timeout after N seconds, preventing resource exhaustion. Combine with application-level request timeout for defense-in-depth. Set timeout low enough (e.g., 5 seconds) to catch expensive operations, high enough for legitimate slow queries.

---

## Pattern: JWK Field Validation as Defense-in-Depth
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/auth/jwt.rs`

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
**Added**: 2026-01-24
**Related files**: `crates/global-controller/src/services/media_handler_registry.rs`

When services register external resources (handlers, endpoints, callback URLs), validate both identifier format AND URL security:

1. **Identifier format validation**: Use allowlist regex patterns (e.g., `^[a-zA-Z0-9_-]+$` for handler IDs). Reject inputs with path traversal, null bytes, or injection characters. Short max lengths (64-128 chars) prevent DoS via long identifiers.

2. **Endpoint URL validation**: Require HTTPS scheme (reject HTTP, FTP, file://). Validate URL parsability. Consider allowlisting domains/IP ranges for internal services. Reject localhost/127.0.0.1 in production to prevent SSRF to internal services.

This pattern applies to: Media Handler registration, webhook callbacks, federation endpoints, any user-supplied URLs stored for later use.

---

## Pattern: Authorization State Separation with Audit Trail
**Added**: 2026-01-25
**Related files**: `docs/decisions/adr-0023-mc-architecture.md`

When multiple actors can affect the same state (e.g., mute), maintain separate state per actor:
1. **Self-initiated state**: User controls (e.g., `self_muted: bool`)
2. **Host-initiated state**: Admin/host controls (e.g., `host_muted: bool`)
3. **Effective state**: Computed from both (muted if either is true)

Benefits: (1) Clear audit trail - who caused the mute, (2) Proper restoration - self-unmute doesn't override host-mute, (3) Authorization clarity - different permission checks per actor. Store `muted_by` enum or field for audit: `Self`, `Host`, `System`. This pattern applies to: mute/unmute, visibility, permissions, feature access controlled by multiple authorities.

---

## Pattern: HKDF Key Derivation for Scoped Tokens
**Added**: 2026-01-25
**Related files**: `docs/decisions/adr-0023-mc-architecture.md`

When generating tokens scoped to a resource (meeting, session, room), derive per-resource keys using HKDF rather than using a single master key directly:

1. **Master secret**: `MC_BINDING_TOKEN_SECRET` - service-level secret
2. **Key derivation**: HKDF-SHA256 with resource ID as info parameter: `HKDF(master, salt=nil, info=meeting_id)`
3. **Token generation**: HMAC-SHA256 with derived key over (session_id || user_id)

Benefits: (1) Compromise of one meeting's tokens doesn't reveal master secret, (2) Key material is deterministic - can regenerate without storage, (3) Follows cryptographic best practices for key hierarchy. Use `ring::hkdf` for derivation. Include TTL in token payload for expiration enforcement.

---

## Pattern: gRPC Interceptor for Authorization Validation
**Added**: 2026-01-25
**Related files**: `crates/meeting-controller/src/grpc/interceptor.rs`

Use gRPC interceptors (tonic middleware) to enforce authorization on incoming service-to-service calls. Pattern:

1. **Interceptor struct**: `McAuthInterceptor` holds AC client reference and required scopes
2. **Metadata extraction**: Extract `authorization` header from gRPC request metadata
3. **Token validation**: Validate JWT signature + claims via AC JWKS endpoint
4. **Scope enforcement**: Verify token contains required scope (e.g., `mc:assign`)
5. **Early rejection**: Return `Status::unauthenticated()` or `Status::permission_denied()` before handler runs

Benefits: (1) Centralized auth logic - not scattered in handlers, (2) Defense-in-depth - even if handler forgets auth, interceptor catches it, (3) Consistent error responses. Apply interceptor via `Server::builder().layer()` or per-service via `ServiceBuilder`.

---

## Pattern: Token Size Limits for DoS Prevention
**Added**: 2026-01-25
**Related files**: `crates/meeting-controller/src/grpc/interceptor.rs`

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
**Related files**: `crates/meeting-controller/src/actors/controller.rs`, `crates/meeting-controller/src/actors/meeting.rs`, `crates/meeting-controller/src/actors/session.rs`

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
**Related files**: `crates/global-controller/src/auth/jwt.rs`, `crates/global-controller/src/middleware/auth.rs`, `crates/global-controller/src/services/ac_client.rs`

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

## Pattern: Server-Side Error Context with Generic Client Messages
**Added**: 2026-01-28
**Related files**: `crates/global-controller/src/errors.rs`, `crates/global-controller/src/handlers/meetings.rs`, `crates/global-controller/src/grpc/mc_service.rs`

Preserve error context for debugging via server-side logging while returning generic messages to clients. Pattern:

1. **Error type with context**: Enum variant accepts String: `Internal(String)`, `Database(String)`
2. **Server-side logging**: In `IntoResponse` impl, log the full error with `tracing::error!(reason = %reason, ...)`
3. **Client response**: Return only generic message: `"An internal error occurred"`
4. **Context in logs**: Include operational context (parse error type, service name) but NEVER secrets

**Example**:
```rust
GcError::Internal(reason) => {
    tracing::error!(target: "gc.internal", reason = %reason, "Internal error");
    (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR", "An internal error occurred".to_string())
}
```

**Benefits**: Debugging gets full context, clients get minimal info (prevents enumeration/info disclosure). Common pattern for database errors, service communication failures, parsing errors.

---
