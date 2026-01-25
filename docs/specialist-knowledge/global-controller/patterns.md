# Global Controller Patterns

Reusable patterns discovered and established in the Global Controller codebase.

---

## Pattern: Token Size Check Before Parsing (DoS Prevention)
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/auth/jwt.rs`

ALWAYS check token size in bytes BEFORE any parsing or cryptographic operations. Set MAX_JWT_SIZE_BYTES constant (8KB default), check `token.len() > MAX_JWT_SIZE_BYTES` at function entry. Prevents DoS via oversized tokens consuming CPU/memory. Return generic error message to avoid info leakage.

---

## Pattern: JWK Validation Before Signature Verification
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/auth/jwt.rs`

Validate JWK structure BEFORE using it for signature verification:
- Check `jwk.kty == "OKP"` (reject if not, log warning)
- Check `jwk.alg == "EdDSA"` if present (reject if different, log warning)
- This prevents algorithm confusion attacks where attacker manipulates JWK to use weak algorithms

---

## Pattern: Algorithm Pinning in jsonwebtoken Validation
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/auth/jwt.rs`

Use `Validation::new(Algorithm::EdDSA)` to explicitly set the expected algorithm BEFORE calling `decode()`. Never use `Validation::default()` which accepts multiple algorithms. Pinning prevents algorithm confusion attacks from alg:none or alg:HS256 tokens.

---

## Pattern: JWKS Caching with TTL and Refresh
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/auth/jwks.rs`

Implement JWKS caching with:
- In-memory cache (HashMap<kid, Jwk>) wrapped in Arc<RwLock<Option<CachedJwks>>>
- Expiry time (Instant::now() + cache_ttl) stored with cached data
- Cache miss or expired triggers async refresh_cache()
- Read lock for cache hits, write lock only for updates
- Default 5-minute TTL balances key rotation latency vs AC load

---

## Pattern: kid Extraction Without Full Token Parsing
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/auth/jwt.rs`

Extract kid for key lookup by:
- Split token on '.' to get exactly 3 parts
- Decode header (first part) from base64url
- Parse as JSON (handle parse failure gracefully)
- Extract kid as string from header object
- Return Option to allow error propagation upstream
This avoids full JWT parsing before signature validation - kid selection is data-only.

---

## Pattern: AC Client Service for Internal Endpoints
**Added**: 2026-01-15
**Related files**: `crates/global-controller/src/services/ac_client.rs`

HTTP client for calling AC internal token endpoints. Uses Bearer auth with GC_SERVICE_TOKEN, configurable timeout (default 10s), and proper error mapping (network errors -> ServiceUnavailable, 4xx -> Unauthorized/Forbidden). Client is reusable via Arc in AppState.

---

## Pattern: CSPRNG Guest ID Generation
**Added**: 2026-01-15
**Related files**: `crates/global-controller/src/handlers/meetings.rs`

Generate guest IDs using `ring::rand::SystemRandom` for CSPRNG security. Fill 16-byte buffer, then apply UUID v4 bit manipulation (version nibble = 4, variant bits = 10xx). Format as hyphenated UUID string. Never use thread_rng() for security-critical IDs.

---

## Pattern: Host-Only Authorization Check
**Added**: 2026-01-15
**Related files**: `crates/global-controller/src/handlers/meetings.rs`

For host-only endpoints (settings, kick participant), compare `meeting.created_by_user_id` against `claims.sub`. Return 403 Forbidden if mismatch. This check happens AFTER meeting lookup to avoid leaking meeting existence via 403 vs 404.

---

## Pattern: Testing JWKS Cache with Short TTL
**Added**: 2026-01-18
**Related files**: `crates/global-controller/src/auth/jwks.rs`

To test cache expiration behavior, create JwksClient with very short TTL (1ms) and use `tokio::time::sleep()` to trigger expiration. Use wiremock's `expect(N)` to verify cache hits vs fetches. This avoids flaky time-dependent tests while still exercising cache expiration paths.

---

## Pattern: HTTP Status Code Branch Coverage
**Added**: 2026-01-18
**Related files**: `crates/global-controller/src/services/ac_client.rs`

When testing HTTP client response handling, test ALL status code branches: success (200), client errors (400, 401, 403, 404), server errors (500, 502), and unexpected codes (418). Use wiremock to return each status and verify error mapping. This ensures full branch coverage of `handle_response()` logic.

---

## Pattern: Tower Layer for Async gRPC Auth
**Added**: 2026-01-20
**Related files**: `crates/global-controller/src/grpc/auth_layer.rs`

Use Tower's `Layer` + `Service` traits for async JWT validation on gRPC endpoints instead of tonic's sync interceptor. This pattern:
1. Define `AuthLayer` holding `Arc<JwksClient>` and `Arc<GcConfig>`
2. Define `AuthService<S>` wrapping inner service with auth state
3. Implement `Service<Request<Body>>` with async `call()` that validates JWT
4. Use `PendingTokenValidation` struct to hold state between poll phases
5. Extract Bearer token from `authorization` metadata, validate async, then forward

Benefits: async JWKS fetching, shared cache across requests, proper backpressure via Tower semantics. Tonic interceptors are sync-only and can't await.

---

## Pattern: UPSERT for Service Registration
**Added**: 2026-01-20
**Related files**: `crates/global-controller/src/repositories/meeting_controllers.rs`

Use UPSERT (INSERT ON CONFLICT UPDATE) for MC registration instead of separate check-then-insert:
```sql
INSERT INTO meeting_controllers (...)
VALUES ($1, ...)
ON CONFLICT (hostname) DO UPDATE SET
    grpc_port = EXCLUDED.grpc_port,
    last_heartbeat = NOW(),
    health_status = 'healthy'
RETURNING id
```
Benefits: atomic operation, handles MC restarts cleanly (re-registration updates existing row), eliminates race conditions, returns ID for both insert and update cases.

---

## Pattern: Dual Heartbeat Design (Fast vs Comprehensive)
**Added**: 2026-01-20
**Related files**: `crates/global-controller/src/grpc/mc_service.rs`

Implement two heartbeat types for service health monitoring:
- **Fast heartbeat** (10s interval): Lightweight, sends only capacity (current/max participants). Used for load balancing decisions.
- **Comprehensive heartbeat** (30s interval): Full metrics (CPU, memory, bandwidth, error rates, latency percentiles). Used for observability and alerting.

Both update `last_heartbeat` timestamp. This design reduces network overhead while maintaining fresh capacity data for routing and detailed metrics for monitoring.

---

## Pattern: Background Health Checker with CancellationToken
**Added**: 2026-01-20
**Related files**: `crates/global-controller/src/tasks/health_checker.rs`, `crates/global-controller/src/main.rs`

Decouple health checking from heartbeat processing using a background task:
1. Spawn `tokio::spawn(run_health_checker(pool, config, cancel_token.clone()))`
2. Task loops: sleep interval, mark stale MCs unhealthy, check cancellation
3. Use `tokio::select!` to race sleep vs cancellation for responsive shutdown
4. Staleness threshold (e.g., 60s) is configurable in GcConfig

Benefits: health checking runs even if heartbeats stop arriving, single point of staleness logic, graceful shutdown via CancellationToken.

---

## Pattern: Dual Server Graceful Shutdown
**Added**: 2026-01-20
**Related files**: `crates/global-controller/src/main.rs`

Running HTTP (axum) + gRPC (tonic) servers requires coordinated shutdown:
1. Create `CancellationToken` from `tokio_util::sync`
2. Wrap each server in `tokio::select!` racing with `cancel_token.cancelled()`
3. Use `tokio::select!` at top level to await first completion
4. When any server exits, `cancel_token.cancel()` triggers others
5. Background tasks (health checker) also check the token

This ensures all components shut down cleanly on SIGTERM or server error.

---

## Pattern: Input Validation with Character Whitelist
**Added**: 2026-01-20
**Related files**: `crates/global-controller/src/grpc/mc_service.rs`

Validate gRPC request fields using whitelist characters to prevent injection:
- Hostname: `c.is_ascii_alphanumeric() || c == '-' || c == '.'` (DNS-safe)
- Region: alphanumeric only
- Version: alphanumeric, dot, dash (semver-safe)

Combine with length limits. Return generic validation error without echoing bad input. This prevents log injection and downstream parsing issues.

---

## Pattern: INSERT ON CONFLICT for Atomic Assignment
**Added**: 2026-01-21
**Related files**: `crates/global-controller/src/repositories/meeting_assignments.rs`

Use INSERT ON CONFLICT DO UPDATE for atomic meeting-to-MC assignment instead of CTEs with separate SELECT and INSERT:
```sql
INSERT INTO meeting_mc_assignments (meeting_id, mc_id, assigned_at)
SELECT $1, mc.id, NOW()
FROM meeting_controllers mc
WHERE mc.health_status = 'healthy'
ORDER BY RANDOM() * weight DESC
LIMIT 1
ON CONFLICT (meeting_id) DO UPDATE SET
    mc_id = EXCLUDED.mc_id,
    assigned_at = NOW()
RETURNING mc_id
```
Benefits: Single atomic operation, avoids CTE snapshot isolation issues where separate CTEs don't see each other's modifications, handles re-assignment cleanly.

---

## Pattern: Tonic Channel Caching for gRPC Clients
**Added**: 2026-01-24
**Related files**: `crates/global-controller/src/services/mc_client.rs`

Cache Tonic `Channel` connections to downstream gRPC services instead of creating new connections per-request:
1. Store `Arc<RwLock<HashMap<String, Channel>>>` in client struct
2. On request, check cache for existing channel to target endpoint
3. If miss, create new channel with `Channel::from_shared(endpoint)?.connect_lazy()`
4. `connect_lazy()` defers actual connection until first RPC, avoiding blocking during cache population
5. Channels handle HTTP/2 multiplexing and connection pooling internally

Benefits: Reduces connection overhead, enables HTTP/2 stream reuse, prevents connection exhaustion under load.

---

## Pattern: Mock Trait for Testing gRPC Clients
**Added**: 2026-01-24
**Related files**: `crates/global-controller/src/services/mc_client.rs`

Define async trait for gRPC client operations and implement both real and mock versions:
```rust
#[async_trait]
pub trait McClientTrait: Send + Sync {
    async fn assign_meeting(&self, mc_endpoint: &str, meeting_id: Uuid) -> Result<(), GcError>;
}

pub struct McClient { /* real implementation */ }
pub struct MockMcClient { /* test implementation */ }
```
Use `Arc<dyn McClientTrait>` in service layer. Tests inject `MockMcClient` that returns configured responses without network calls. This avoids `#[cfg(test)]` coupling and works in both unit and integration tests.

---

## Pattern: SecretString for Service Credentials in Clients
**Added**: 2026-01-24
**Related files**: `crates/global-controller/src/services/mc_client.rs`

Wrap service-to-service auth tokens in `secrecy::SecretString` within client structs:
```rust
pub struct McClient {
    service_token: SecretString,
    // ...
}
```
Use `service_token.expose_secret()` only when building the Authorization header. This ensures:
- Tokens never appear in Debug output
- Tokens can't be accidentally logged
- Memory is zeroized on drop (if using zeroize feature)

Consistent with project-wide sensitive data handling per SecretBox/SecretString refactor.

---
