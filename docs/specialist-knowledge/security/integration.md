# Security Specialist Integration Guide

What other specialists need to know about security requirements in Dark Tower.

---

## Integration: Auth Controller Compliance
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/config.rs`

Bcrypt cost must be 10-14 (OWASP 2024). Default 12. JWT clock skew 1-600 seconds, default 300 (NIST recommendation). Token lifetime 1 hour. Rate limit: 5 failures in 15 min triggers lockout.

---

## Integration: Operations - Security Config
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/config.rs`

Production deployments MUST set: `AC_MASTER_KEY` (32-byte base64), `AC_HASH_SECRET` (not default). Verify `BCRYPT_COST` is at least 12. TLS/SSL required for DATABASE_URL. Alert on bcrypt cost warnings in logs.

---

## Integration: Observability - Security Metrics
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/observability/metrics.rs`

Track: auth failures (rate), rate limit triggers, bcrypt latency (p99), JWT validation errors. Alert thresholds: >10 auth failures/min, bcrypt p99 >500ms, rate limit bursts. Never include secrets in traces.

---

## Integration: Protocol Specialist - Error Contracts
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/errors.rs`

API errors must be generic for security endpoints. Use HTTP 401 for all auth failures (not 404 for missing users). Error bodies: `{"error": "invalid_client"}` - no details. Document allowed error codes only.

---

## Integration: Code Review - SecretBox/SecretString Verification
**Added**: 2026-01-12
**Related files**: `crates/ac-service/src/config.rs`, `crates/ac-service/src/crypto/mod.rs`

When reviewing code with secrets: (1) Grep for `.expose_secret()` - each call is a potential leak, verify necessity, (2) Check custom Debug impls redact with `[REDACTED]`, (3) Verify custom Serialize only on "one-time reveal" response types, (4) Confirm Clone impls re-wrap in SecretBox. Any raw `String`/`Vec<u8>` holding secrets is a finding.

---

## Integration: Infrastructure - Key Management
**Added**: 2026-01-11 (Updated: 2026-02-10)
**Related files**: `crates/ac-service/src/config.rs`, `docs/decisions/adr-0008-key-rotation-strategy.md`, `docs/decisions/adr-0012-infrastructure-architecture.md`

Master keys via environment variables or secrets manager (external-secrets-operator in K8s per ADR-0012). Never in code/config files.

**AC signing key rotation** (ADR-0008): `POST /internal/rotate-keys` with proper scopes (`service.rotate-keys.ac` or `admin.force-rotate-keys.ac`). Old keys valid 1 week post-rotation (not 24 hours—updated per ADR-0008). Rate limits: 1 per 6 days (normal) or 1 per hour (force).

**MC master secret rotation**: Rotates on each deployment via environment variable injection. 1-hour grace period for in-flight tokens during rolling deploys.

**K8s secrets management**: Use external-secrets-operator to sync from cloud provider (AWS Secrets Manager, GCP Secret Manager, Azure Key Vault, HashiCorp Vault). Secrets automatically injected as environment variables or mounted files. Never store plaintext secrets in Git or ConfigMaps.

---

## Integration: Global Controller - JWT/JWKS Validation
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/auth/jwt.rs`, `crates/global-controller/src/auth/jwks.rs`

GC validates JWTs from AC via JWKS. Requirements: (1) Fetch JWKS from AC_JWKS_URL with caching (5 min TTL), (2) Validate token `alg` is `EdDSA`, (3) Extract `kid` and find matching JWK, (4) Validate JWK fields: `kty == "OKP"` and `alg == "EdDSA"`, (5) Verify signature, (6) Check `iat` with clock skew tolerance, (7) Return generic error messages on failure.

---

## Integration: Global Controller - Media Handler Registration Security
**Added**: 2026-01-24 (Updated: 2026-01-31)
**Related files**: `crates/global-controller/src/grpc/mh_service.rs`, `crates/global-controller/src/repositories/media_handlers.rs`

When Media Handlers register with GC, validate: (1) handler_id format (alphanumeric + underscore/hyphen only, max 64 chars), (2) endpoint URL scheme (HTTPS required), (3) service_token stored as SecretString not plain String. Registration endpoints should be authenticated - only services with valid AC tokens can register handlers. Consider IP allowlisting for handler registration in production.

---

## Integration: Meeting Controller - Session Binding Security
**Added**: 2026-01-25 (Updated: 2026-01-25)
**Related files**: `docs/decisions/adr-0023-meeting-controller-architecture.md`

MC binds WebTransport sessions to authenticated users. Requirements:

1. **Key hierarchy**: Use HKDF to derive per-meeting keys from `MC_BINDING_TOKEN_SECRET`. Never use master secret directly for token generation.
2. **Token structure**: HMAC-SHA256 over (session_id || user_id || timestamp). Include TTL for expiration (default 5 minutes, configurable 1-60 min).
3. **Token validation**: Use `ring::hmac::verify()` for constant-time comparison. Never compute expected HMAC and compare with `==`.
4. **Reconnection**: Validate binding token on reconnect attempts. Reject if TTL expired or session_id mismatch.
5. **Host authorization**: Check `is_host` field before allowing privileged actions (mute others, kick). Self-actions (self-mute) don't require host check.
6. **Error sanitization**: Return generic errors to clients ("Invalid session" not "Session abc123 not found").

Service must fail startup if `MC_BINDING_TOKEN_SECRET` is not configured.

---

## Integration: Meeting Controller - GC Communication Security
**Added**: 2026-01-25 (Updated: 2026-01-30)
**Related files**: `crates/meeting-controller/src/grpc/auth_interceptor.rs`, `crates/meeting-controller/src/grpc/gc_client.rs`, `crates/meeting-controller/src/actors/controller.rs`

GC-to-MC communication uses authenticated gRPC. Security requirements:

1. **Inbound (GC → MC)**: Use `McAuthInterceptor` to validate all incoming gRPC calls. Require `mc:assign` scope for AssignMeeting RPC. Reject requests without valid AC-issued JWT.
2. **Token size limit**: Enforce 8KB max on incoming tokens before parsing (DoS prevention).
3. **Outbound (MC → GC)**: Store GC service token as `SecretString`, not plain `String`. Use `SecretString` throughout config and client structs.
4. **Connection URLs**: Never log Redis/database URLs with credentials. Parse URL and log only host:port.
5. **Startup validation**: Fail fast if required secrets (`MC_BINDING_TOKEN_SECRET`, `GC_SERVICE_TOKEN`) are not configured.

The gRPC interceptor pattern ensures authorization is checked before handler code runs, providing defense-in-depth even if individual handlers forget auth checks.

---

## Integration: Meeting Controller - Session Binding Token Security
**Added**: 2026-01-28 (Updated: 2026-02-10)
**Related files**: `crates/meeting-controller/src/actors/session.rs`, `crates/meeting-controller/src/actors/meeting.rs`, `docs/decisions/adr-0023-meeting-controller-architecture.md`

Session binding tokens provide recovery after connection drops per ADR-0023 Section 1. Security requirements:

1. **Master secret storage**: Wrap in `SecretBox<Vec<u8>>`, pass through actor hierarchy. Each actor clones into own SecretBox for isolated lifecycle.
2. **Key derivation**: HKDF-SHA256 with `meeting_id` as salt, `b"session-binding"` as info. Per-meeting keys prevent key reuse across meetings.
3. **Token generation**: `HMAC-SHA256(meeting_key, correlation_id || participant_id || nonce)`. One-time nonce prevents replay.
4. **Token validation**: Use `ring::hmac::verify()` for constant-time comparison, NOT `==` operator.
5. **TTL**: Bind tokens have 30-second TTL. Enforce expiration on reconnect validation. Aligns with participant disconnect grace period.
6. **Nonce grace window**: 5 seconds to handle in-flight retransmits. Nonce storage TTL is 35 seconds (token TTL + grace window).
7. **No secret leakage**: Never log binding tokens, nonces, or master secret. Only log correlation_id and participant_id (safe identifiers).

**Security note**: Binding tokens are defense-in-depth, not primary authentication. Attackers need both a valid binding token AND a valid JWT for the same `user_id` to hijack a session. The 30-second TTL is the primary protection; even with a compromised master secret, attackers have only a 30-second window and still need a valid JWT.

**Master secret rotation**: MC rotates master secret on each deployment (~weekly). 1-hour grace period covers in-flight tokens during rolling deploys. Emergency rotation is possible via forced redeploy.

---

## Integration: Common Crate - TokenManager Security
**Added**: 2026-02-02
**Related files**: `crates/common/src/token_manager.rs`

Services using `TokenManager` for OAuth 2.0 token acquisition must follow these security requirements:

1. **Production configuration**: Use `TokenManagerConfig::new_secure()` which enforces HTTPS. The `new()` constructor allows HTTP but is only appropriate for local development.
2. **Clock synchronization**: TokenManager includes a 30-second clock drift margin, but proper NTP synchronization on all hosts is strongly recommended for production.
3. **Secret handling**: `client_secret` is stored as `SecretString`. The returned `TokenReceiver` provides tokens wrapped in `SecretString` - callers must use `.expose_secret()` explicitly when constructing Authorization headers.
4. **Error handling**: Token acquisition errors are logged server-side with details; error messages returned to callers contain only status codes, no response bodies (prevents credential echo in verbose error messages).
5. **Shutdown**: Services should call `handle.abort()` on the returned `JoinHandle` during graceful shutdown to stop the background refresh task.

Services that require OAuth tokens (GC, MC when calling AC) should initialize TokenManager at startup and fail fast if initial token acquisition fails (don't start serving requests without valid service credentials).

---

## Integration: Meeting Controller - Observability Metrics Security
**Added**: 2026-02-05
**Related files**: `crates/meeting-controller/src/actors/metrics.rs`, `crates/meeting-controller/src/observability/metrics.rs`

MC exposes internal metrics to Prometheus per ADR-0023. Security requirements to prevent PII leakage:

1. **No user-identifiable labels**: Metric labels must use only bounded enum values (`actor_type`: controller, meeting, connection). Never use meeting IDs, user IDs, session IDs, or participant IDs as label values.

2. **Aggregate counts only**: Track total active meetings/connections without per-entity breakdown. Prometheus cardinality explosion from unbounded identifiers can crash monitoring.

3. **Internal state separate**: `ControllerMetrics` (used for GC heartbeats) is intentionally NOT exposed to Prometheus to avoid exposing internal operational state.

4. **Cardinality bounds documented**: Each metric function documents allowed label values and cardinality count. All label sources must be type-safe (enums with `as_str()`).

5. **Tracing logs vs metrics**: Actor IDs (e.g., "meeting-123") appear in structured tracing logs for debugging but NEVER in Prometheus labels.

The separation ensures internal operational metrics go to GC (trusted service boundary) while public `/metrics` endpoint exposes only aggregate, PII-free data.

---
