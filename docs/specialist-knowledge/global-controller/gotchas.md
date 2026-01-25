# Global Controller Gotchas

Mistakes to avoid and edge cases discovered in the Global Controller codebase.

---

## Gotcha: AC JWKS URL Must Be Reachable
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/config.rs`

GC validates JWTs using AC's JWKS endpoint. AC_JWKS_URL must be reachable at runtime. In tests, use wiremock to mock the endpoint. In production, ensure network connectivity to AC.

---

## Gotcha: Clock Skew Must Match AC Configuration
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/config.rs`

JWT_CLOCK_SKEW_SECONDS should match AC's value (default 300s). Mismatched skew can cause valid tokens to be rejected. Both services should read from a shared configuration source in production.

---

## Gotcha: kid Extraction Happens BEFORE Signature Validation
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/auth/jwt.rs`

The kid is extracted from JWT header without signature verification. This is correct for key lookup but:
- Never trust kid value
- Always validate JWK (kty, alg) after fetching
- Attacker can claim any kid, but must have valid signature from that key
If JWK doesn't exist, return "invalid or expired" not "kid not found" (info leak prevention).

---

## Gotcha: JWKS Cache TTL Affects Key Rotation Latency
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/auth/jwks.rs`

5-minute cache TTL means AC key rotations take up to 5 minutes to propagate. If AC rotates keys and GC still has old key cached, tokens signed with new key will fail until cache expires. This is intentional tradeoff:
- Shorter TTL (1 min): Faster rotation but higher load on AC
- Longer TTL (10 min): Lower AC load but slower rotation
Verify TTL matches operational requirements during deployment.

---

## Gotcha: Generic Error Messages Hide JWT Validation Details
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/auth/jwt.rs`

All JWT validation failures return "The access token is invalid or expired" to clients. This is intentional - don't leak:
- Whether kid was found
- Why JWK validation failed
- Specific signature error details
Log detailed error internally, but never include in HTTP response. This prevents attackers from probing token format.

---

## Gotcha: Algorithm Confusion Attacks via alg:none
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/auth/jwt.rs`

The jsonwebtoken library defaults to accepting `alg:none` if not explicitly pinned. ALWAYS use `Validation::new(Algorithm::EdDSA)` - never `Validation::default()`. Test for this specifically:
- Token with `alg:none` should be rejected
- Token with `alg:HS256` should be rejected
- Only `alg:EdDSA` should be accepted

---

## Gotcha: GC_SERVICE_TOKEN Required for AC Communication
**Added**: 2026-01-15
**Related files**: `crates/global-controller/src/config.rs`, `crates/global-controller/src/services/ac_client.rs`

GC_SERVICE_TOKEN env var is required for internal AC endpoint calls (meeting tokens, guest tokens). Empty string default causes silent 401 failures from AC. In tests, mock the AC endpoints or provide valid test token. Production MUST set this via secrets management.

---

## Gotcha: Captcha Validation is Placeholder
**Added**: 2026-01-15
**Related files**: `crates/global-controller/src/handlers/meetings.rs`

Guest token endpoint has TODO placeholder for captcha validation. Currently accepts any captcha_token value. Phase 3+ must integrate real captcha provider (reCAPTCHA, hCaptcha). Do not deploy guest access without implementing this security control.

---

## Gotcha: JWT kid Extraction Returns None for Non-String Values
**Added**: 2026-01-18
**Related files**: `crates/global-controller/src/auth/jwt.rs`

The `extract_kid()` function returns `None` (not an error) when the JWT header contains a `kid` that is not a JSON string - including numeric values, null, or empty strings. This is by design: attackers may send malformed headers to probe error handling. Always handle `None` as "key not found" and return generic error message.

---

## Gotcha: PendingTokenValidation Debug Can Expose Tokens
**Added**: 2026-01-20
**Related files**: `crates/global-controller/src/grpc/auth_layer.rs`

Deriving `Debug` on structs holding tokens exposes them in logs/panics. The `PendingTokenValidation` struct holds the raw Bearer token during async validation. Either:
1. Use custom `Debug` impl that redacts the token field
2. Wrap token in `SecretString` from `secrecy` crate
3. Mark field with `#[debug(skip)]` if using `derivative` crate

Current implementation uses derived Debug - this is a [LOW] finding to address.

---

## Gotcha: Capacity Overflow Silently Clamps to i32::MAX
**Added**: 2026-01-20
**Related files**: `crates/global-controller/src/grpc/mc_service.rs`

When converting MC capacity from u32 (proto) to i32 (database), values > i32::MAX are clamped:
```rust
let capacity: i32 = request.max_capacity.min(i32::MAX as u32) as i32;
```
This is intentional (MC can't have 2B+ capacity), but consider logging a warning when clamping occurs. Current implementation silently clamps - unexpected behavior if MC misconfigures capacity.

---

## Gotcha: Runtime vs Compile-Time SQL Query Tradeoff
**Added**: 2026-01-20
**Related files**: `crates/global-controller/src/repositories/meeting_controllers.rs`

The MC registration uses runtime queries (`sqlx::query()`) instead of compile-time macros (`sqlx::query!()`). Tradeoffs:
- **Runtime**: More flexible, no sqlx prepare step needed, but SQL errors only caught at runtime
- **Compile-time**: Catches SQL errors at build, but requires DB connection for `cargo check`

Current choice (runtime) was pragmatic for initial implementation. Consider migrating to compile-time queries for stronger guarantees once schema stabilizes.

---

## Gotcha: Timestamp Casts Use `as` Instead of `try_into()`
**Added**: 2026-01-20
**Related files**: `crates/global-controller/src/grpc/mc_service.rs`

Proto timestamps are i64, but some DB/API contexts use i32 or other types. Using `as` for casting:
```rust
let timestamp = chrono::Utc::now().timestamp() as i64;
```
This works for timestamps (always positive, fits i64), but `try_into()` is safer for untrusted input. For internal timestamps `as` is acceptable; for MC-provided timestamps consider validation.

---

## Gotcha: PostgreSQL CTE Snapshot Isolation
**Added**: 2026-01-21
**Related files**: `crates/global-controller/src/repositories/meeting_assignments.rs`

In PostgreSQL, CTEs with data-modifying statements (INSERT, UPDATE, DELETE) all execute with the same snapshot - they don't see each other's changes. If you have a CTE that updates `health_status` and another CTE that selects healthy MCs, the SELECT won't see the UPDATE's changes. Solution: Use single INSERT ON CONFLICT statements or explicit transactions with separate queries. This is different from standard CTE behavior where read-only CTEs can reference each other.

---

## Gotcha: #[expect(dead_code)] vs #[allow(dead_code)]
**Added**: 2026-01-21
**Related files**: `crates/global-controller/src/repositories/meeting_controllers.rs`

Use `#[allow(dead_code)]` not `#[expect(dead_code)]` for code that's only used in tests. The `#[expect(...)]` attribute generates a warning if the lint would NOT have fired (i.e., if the code IS used). When test modules use helper functions, the code is technically "used" during test compilation, causing `#[expect(dead_code)]` to warn. Use `#[allow(dead_code)]` which silently permits unused code without complaining when it's actually used.

---

## Gotcha: PostgreSQL Dynamic Interval Casting
**Added**: 2026-01-23
**Related files**: `crates/global-controller/src/repositories/meeting_assignments.rs`

PostgreSQL does not allow parameterized intervals directly (e.g., `INTERVAL $1 hours` fails). Use string concatenation with explicit cast: `($1 || ' hours')::INTERVAL` where `$1` is an integer. This pattern works for hours, days, minutes, etc. The parameter must be text or castable to text. Example: `WHERE assigned_at < NOW() - ($1 || ' hours')::INTERVAL` with bind value of integer hours.

---

## Gotcha: prost Generates Simplified Enum Variant Names
**Added**: 2026-01-24
**Related files**: `crates/proto-gen/src/`, `crates/global-controller/src/services/mh_service.rs`

When prost generates Rust code from Protocol Buffers, enum variants are simplified - the enum name prefix is NOT repeated. For example, proto `enum MhRole { MH_ROLE_PRIMARY = 0; }` generates Rust `MhRole::Primary`, not `MhRole::MhRolePrimary`. This catches developers who expect the full proto name. Check generated code in `proto-gen` crate when unsure about variant names.

---

## Gotcha: #[cfg(test)] Helpers Unavailable in Integration Tests
**Added**: 2026-01-24
**Related files**: `crates/global-controller/src/services/mc_client.rs`, `crates/global-controller/tests/`

Functions defined in `#[cfg(test)] mod tests { ... }` within a library crate are NOT visible to integration tests (`tests/*.rs`). Integration tests compile as separate crates and only see the public API. Solutions:
1. Move test helpers to a `-test-utils` crate (preferred for reuse)
2. Define mock traits in main code, implement in integration tests
3. Use feature flags (`#[cfg(feature = "test-helpers")]`) for test-only exports

The mock trait pattern (see patterns.md) avoids this issue entirely by keeping test infrastructure in the public API.

---
