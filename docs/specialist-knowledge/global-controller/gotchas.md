# Global Controller Gotchas

Mistakes to avoid and edge cases discovered in the Global Controller codebase.

---

## Gotcha: AC JWKS URL Must Be Reachable
**Added**: 2026-01-14
**Related files**: `crates/gc-service/src/config.rs`

GC validates JWTs using AC's JWKS endpoint. AC_JWKS_URL must be reachable at runtime. In tests, use wiremock to mock the endpoint. In production, ensure network connectivity to AC.

---

## Gotcha: Clock Skew Must Match AC Configuration
**Added**: 2026-01-14
**Related files**: `crates/gc-service/src/config.rs`

JWT_CLOCK_SKEW_SECONDS should match AC's value (default 300s). Mismatched skew can cause valid tokens to be rejected. Both services should read from a shared configuration source in production.

---

## Gotcha: kid Extraction Happens BEFORE Signature Validation
**Added**: 2026-01-14
**Related files**: `crates/gc-service/src/auth/jwt.rs`

The kid is extracted from JWT header without signature verification. This is correct for key lookup but:
- Never trust kid value
- Always validate JWK (kty, alg) after fetching
- Attacker can claim any kid, but must have valid signature from that key
If JWK doesn't exist, return "invalid or expired" not "kid not found" (info leak prevention).

---

## Gotcha: JWKS Cache TTL Affects Key Rotation Latency
**Added**: 2026-01-14
**Related files**: `crates/gc-service/src/auth/jwks.rs`

5-minute cache TTL means AC key rotations take up to 5 minutes to propagate. If AC rotates keys and GC still has old key cached, tokens signed with new key will fail until cache expires. This is intentional tradeoff:
- Shorter TTL (1 min): Faster rotation but higher load on AC
- Longer TTL (10 min): Lower AC load but slower rotation
Verify TTL matches operational requirements during deployment.

---

## Gotcha: Generic Error Messages Hide JWT Validation Details
**Added**: 2026-01-14
**Related files**: `crates/gc-service/src/auth/jwt.rs`

All JWT validation failures return "The access token is invalid or expired" to clients. This is intentional - don't leak:
- Whether kid was found
- Why JWK validation failed
- Specific signature error details
Log detailed error internally, but never include in HTTP response. This prevents attackers from probing token format.

---

## Gotcha: Algorithm Confusion Attacks via alg:none
**Added**: 2026-01-14
**Related files**: `crates/gc-service/src/auth/jwt.rs`

The jsonwebtoken library defaults to accepting `alg:none` if not explicitly pinned. ALWAYS use `Validation::new(Algorithm::EdDSA)` - never `Validation::default()`. Test for this specifically:
- Token with `alg:none` should be rejected
- Token with `alg:HS256` should be rejected
- Only `alg:EdDSA` should be accepted

---

## Gotcha: GC_CLIENT_ID and GC_CLIENT_SECRET Required for AC Communication
**Added**: 2026-01-15, **Updated**: 2026-02-11
**Related files**: `crates/gc-service/src/config.rs`, `crates/gc-service/src/main.rs`

GC_CLIENT_ID and GC_CLIENT_SECRET env vars are required for OAuth 2.0 client credentials flow with AC. TokenManager spawns at startup with 30-second timeout - missing credentials cause startup failure. In tests, create mock TokenReceiver via `TokenReceiver::from_watch_receiver()`. Production MUST set these via secrets management (Kubernetes secrets, AWS Secrets Manager, etc.). The legacy GC_SERVICE_TOKEN static token approach was fully replaced in February 2026.

---

## Gotcha: Captcha Validation is Placeholder
**Added**: 2026-01-15
**Related files**: `crates/gc-service/src/handlers/meetings.rs`

Guest token endpoint has TODO placeholder for captcha validation. Currently accepts any captcha_token value. Phase 3+ must integrate real captcha provider (reCAPTCHA, hCaptcha). Do not deploy guest access without implementing this security control.

---

## Gotcha: JWT kid Extraction Returns None for Non-String Values
**Added**: 2026-01-18
**Related files**: `crates/gc-service/src/auth/jwt.rs`

The `extract_kid()` function returns `None` (not an error) when the JWT header contains a `kid` that is not a JSON string - including numeric values, null, or empty strings. This is by design: attackers may send malformed headers to probe error handling. Always handle `None` as "key not found" and return generic error message.

---

## Gotcha: PendingTokenValidation Debug Can Expose Tokens
**Added**: 2026-01-20
**Related files**: `crates/gc-service/src/grpc/auth_layer.rs`

Deriving `Debug` on structs holding tokens exposes them in logs/panics. The `PendingTokenValidation` struct holds the raw Bearer token during async validation. Either:
1. Use custom `Debug` impl that redacts the token field
2. Wrap token in `SecretString` from `secrecy` crate
3. Mark field with `#[debug(skip)]` if using `derivative` crate

Current implementation uses derived Debug - this is a [LOW] finding to address.

---

## Gotcha: Capacity Overflow Silently Clamps to i32::MAX
**Added**: 2026-01-20
**Related files**: `crates/gc-service/src/grpc/mc_service.rs`

When converting MC capacity from u32 (proto) to i32 (database), values > i32::MAX are clamped:
```rust
let capacity: i32 = request.max_capacity.min(i32::MAX as u32) as i32;
```
This is intentional (MC can't have 2B+ capacity), but consider logging a warning when clamping occurs. Current implementation silently clamps - unexpected behavior if MC misconfigures capacity.

---

## Gotcha: Runtime vs Compile-Time SQL Query Tradeoff
**Added**: 2026-01-20
**Related files**: `crates/gc-service/src/repositories/meeting_controllers.rs`

The MC registration uses runtime queries (`sqlx::query()`) instead of compile-time macros (`sqlx::query!()`). Tradeoffs:
- **Runtime**: More flexible, no sqlx prepare step needed, but SQL errors only caught at runtime
- **Compile-time**: Catches SQL errors at build, but requires DB connection for `cargo check`

Current choice (runtime) was pragmatic for initial implementation. Consider migrating to compile-time queries for stronger guarantees once schema stabilizes.

---

## Gotcha: Timestamp Casts Use `as` Instead of `try_into()`
**Added**: 2026-01-20
**Related files**: `crates/gc-service/src/grpc/mc_service.rs`

Proto timestamps are i64, but some DB/API contexts use i32 or other types. Using `as` for casting:
```rust
let timestamp = chrono::Utc::now().timestamp() as i64;
```
This works for timestamps (always positive, fits i64), but `try_into()` is safer for untrusted input. For internal timestamps `as` is acceptable; for MC-provided timestamps consider validation.

---

## Gotcha: PostgreSQL CTE Snapshot Isolation
**Added**: 2026-01-21
**Related files**: `crates/gc-service/src/repositories/meeting_assignments.rs`

In PostgreSQL, CTEs with data-modifying statements (INSERT, UPDATE, DELETE) all execute with the same snapshot - they don't see each other's changes. If you have a CTE that updates `health_status` and another CTE that selects healthy MCs, the SELECT won't see the UPDATE's changes. Solution: Use single INSERT ON CONFLICT statements or explicit transactions with separate queries. This is different from standard CTE behavior where read-only CTEs can reference each other.

---

## Gotcha: #[expect(dead_code)] vs #[allow(dead_code)]
**Added**: 2026-01-21
**Related files**: `crates/gc-service/src/repositories/meeting_controllers.rs`

Use `#[allow(dead_code)]` not `#[expect(dead_code)]` for code that's only used in tests. The `#[expect(...)]` attribute generates a warning if the lint would NOT have fired (i.e., if the code IS used). When test modules use helper functions, the code is technically "used" during test compilation, causing `#[expect(dead_code)]` to warn. Use `#[allow(dead_code)]` which silently permits unused code without complaining when it's actually used.

---

## Gotcha: PostgreSQL Dynamic Interval Casting
**Added**: 2026-01-23
**Related files**: `crates/gc-service/src/repositories/meeting_assignments.rs`

PostgreSQL does not allow parameterized intervals directly (e.g., `INTERVAL $1 hours` fails). Use string concatenation with explicit cast: `($1 || ' hours')::INTERVAL` where `$1` is an integer. This pattern works for hours, days, minutes, etc. The parameter must be text or castable to text. Example: `WHERE assigned_at < NOW() - ($1 || ' hours')::INTERVAL` with bind value of integer hours.

---

## Gotcha: prost Generates Simplified Enum Variant Names
**Added**: 2026-01-24
**Related files**: `crates/proto-gen/src/`, `crates/gc-service/src/services/mh_service.rs`

When prost generates Rust code from Protocol Buffers, enum variants are simplified - the enum name prefix is NOT repeated. For example, proto `enum MhRole { MH_ROLE_PRIMARY = 0; }` generates Rust `MhRole::Primary`, not `MhRole::MhRolePrimary`. This catches developers who expect the full proto name. Check generated code in `proto-gen` crate when unsure about variant names.

---

## Gotcha: #[cfg(test)] Helpers Unavailable in Integration Tests
**Added**: 2026-01-24
**Related files**: `crates/gc-service/src/services/mc_client.rs`, `crates/gc-service/tests/`

Functions defined in `#[cfg(test)] mod tests { ... }` within a library crate are NOT visible to integration tests (`tests/*.rs`). Integration tests compile as separate crates and only see the public API. Solutions:
1. Move test helpers to a `-test-utils` crate (preferred for reuse)
2. Define mock traits in main code, implement in integration tests
3. Use feature flags (`#[cfg(feature = "test-helpers")]`) for test-only exports

The mock trait pattern (see patterns.md) avoids this issue entirely by keeping test infrastructure in the public API.

---

## Gotcha: Error Variant Migration Requires Test Updates
**Added**: 2026-01-28
**Related files**: `crates/gc-service/src/errors.rs`, `crates/gc-service/src/services/ac_client.rs`

When changing an error variant from unit to tuple (e.g., `Internal` → `Internal(String)`), ALL usages must be updated:
- Production code creating errors: `GcError::Internal` → `GcError::Internal(format!("...", e))`
- Test assertions: `assert!(matches!(err, GcError::Internal))` → `assert!(matches!(err, GcError::Internal(_)))`
- Match arms: `GcError::Internal =>` → `GcError::Internal(_) =>`
- Error construction in tests: `GcError::Internal` → `GcError::Internal("test".to_string())`

Rust compiler catches most of these, but test pattern matching can be subtle. Use `_` wildcard in tests to avoid coupling to exact error messages.

---

## Gotcha: Formatter Splits Long map_err Closures
**Added**: 2026-01-28
**Related files**: Multiple files in `crates/gc-service/src/`

When adding error parameter to `.map_err(|e| ...)`, the line may exceed rustfmt's default 100-char limit:
```rust
// Before formatting
.map_err(|e| GcError::Internal(format!("Long context message: {}", e)))?

// After cargo fmt
.map_err(|e| {
    GcError::Internal(format!("Long context message: {}", e))
})?
```

Always run `cargo fmt` after fixing error hiding violations. The formatter will handle line breaks consistently. Don't fight the formatter - accept the multi-line closure style.

---

## Gotcha: TokenManager Startup Timeout Blocks Server Start
**Added**: 2026-02-11
**Related files**: `crates/gc-service/src/main.rs`

TokenManager spawns during GC startup with a 30-second timeout. If AC is unreachable or credentials are invalid, GC fails to start (returns error before binding HTTP/gRPC ports). This is intentional - GC cannot function without valid service credentials. In local dev, ensure AC is running before starting GC. In production, health checks should verify AC connectivity before routing traffic to GC. The timeout is configurable but should remain < 60s to fail fast during pod startup.

---

## Gotcha: gRPC Channel Cache Has No TTL or Failure Invalidation
**Added**: 2026-02-11
**Related files**: `crates/gc-service/src/services/mc_client.rs`

McClient caches gRPC Channels indefinitely (`Arc<RwLock<HashMap<String, Channel>>>`). If an MC endpoint becomes permanently unreachable, the cached channel persists forever. Current implementation has no:
- TTL-based eviction (channels never expire)
- Failure-based invalidation (connection errors don't remove from cache)
- Health-based eviction (unhealthy MCs in DB but cached channel remains)

Workaround: MC registration with new endpoint creates new channel; old endpoint's cached channel is orphaned but harmless (tonic handles reconnection internally). Future improvement: Add TTL (e.g., 5 minutes) or evict on persistent connection failures. ADR-0010 Section 4a notes actor pattern as potential refactor to handle lifecycle cleanly.

---

## Gotcha: Optional Dependencies with Fallback Hide Production Code in Tests
**Added**: 2026-01-31, **Updated**: 2026-02-11
**Related files**: `crates/gc-service/src/routes/mod.rs`, `crates/gc-service/src/handlers/meetings.rs`

When making a dependency optional (e.g., `mc_client: Option<Arc<dyn McClientTrait>>`) with fallback logic for tests, integration tests may exercise the fallback path instead of production code. This creates false confidence - tests pass but production code is untested. Solution: Make dependencies required in AppState (`mc_client: Arc<dyn McClientTrait>`), inject mocks in tests via the trait. All code paths then use the same logic, just with different implementations. Current GC implementation uses required dependencies - McClient and AcClient are always present in AppState.

---

## Gotcha: Binary vs Library Module Trees Are Separate
**Added**: 2026-02-04
**Related files**: `crates/gc-service/src/main.rs`, `crates/gc-service/src/lib.rs`

Rust binaries (`main.rs`) have their own module tree separate from the library (`lib.rs`). Adding `pub mod observability;` to `lib.rs` does NOT make it available in `main.rs`. You must ALSO add `mod observability;` to `main.rs` for the binary to see the module. Symptom: `unresolved import` errors when trying to use modules that exist in lib.rs. Solution: Declare modules in both files, or import from the library crate (`use gc_service::observability;`).

---

## Gotcha: Cross-Crate Metrics Cannot Use Crate-Local Recording Functions
**Added**: 2026-02-09, **Updated**: 2026-02-09
**Related files**: `crates/gc-service/src/observability/metrics.rs`, `crates/common/src/token_manager.rs`

Metrics recording functions in one crate cannot be called from another crate without creating a dependency cycle. Example: GC's `record_token_refresh` cannot instrument `common::TokenManager` because `common` cannot depend on `global-controller` (would be circular).

**Resolution chosen**: Remove unwireable metric functions rather than leaving dead code. The `record_token_refresh` and `record_token_refresh_failure` functions were deleted from `metrics.rs` since they couldn't be wired without architectural changes.

**Future solutions** (when cross-crate metrics are needed):
1. **Callback mechanism**: Pass a closure/trait object to TokenManager for metrics emission
2. **Metrics trait**: Define a trait in `common`, implement in consuming crates
3. **Event observer**: TokenManager emits events, GC subscribes and records metrics

Tech debt TD-GC-001 tracks this for future implementation. The chosen approach should be decided at the architectural level as it affects all cross-crate metrics.

---

## Gotcha: Duration Import Not Needed with start.elapsed()
**Added**: 2026-02-09
**Related files**: `crates/gc-service/src/services/mc_assignment.rs`

When timing operations with `Instant::now()` + `start.elapsed()`, you do NOT need to import `std::time::Duration`. The `elapsed()` method returns `Duration` automatically. Importing `Duration` when you only use `elapsed()` triggers a `unused_imports` warning:

```rust
// Wrong: causes unused import warning
use std::time::{Duration, Instant};

// Correct: Duration not needed
use std::time::Instant;

let start = Instant::now();
// ...
metrics::record_db_query("op", "success", start.elapsed()); // Returns Duration
```

Only import `Duration` if you're constructing durations explicitly (e.g., `Duration::from_secs(5)`).
## Gotcha: Tracing `target:` Requires String Literal, Not `&'static str` Variable
**Added**: 2026-02-12
**Related files**: `crates/gc-service/src/tasks/generic_health_checker.rs`

The `target:` parameter in tracing macros (`info!`, `warn!`, `error!`) must be a compile-time string literal or `const`. A `&'static str` field from a struct does NOT work — the macro expands `target:` into a `static __CALLSITE` initializer that requires const-evaluable expressions. This means you cannot parameterize log targets via struct fields or function arguments.

```rust
// FAILS: error[E0435]: attempt to use a non-constant value in a constant
info!(target: config.log_target, "message");

// WORKS: string literal
info!(target: "gc.task.health_checker", "message");

// WORKS: const
const TARGET: &str = "gc.task.health_checker";
info!(target: TARGET, "message");
```

Workarounds: (1) Keep `target:` logs in thin wrappers with hardcoded literals, (2) use `macro_rules!` to splice literal tokens, or (3) omit `target:` and rely on `#[instrument]` span names for differentiation.

---

## Gotcha: Custom `gc.task.*` Log Targets Silently Filtered by Default EnvFilter
**Added**: 2026-02-12
**Related files**: `crates/gc-service/src/main.rs`, `crates/gc-service/src/tasks/`

The default `EnvFilter` in main.rs is `"global_controller=debug,tower_http=debug"`, which filters by target prefix using `::` module path hierarchy. Custom dot-separated targets like `target: "gc.task.health_checker"` are in a completely different namespace and do NOT match the `global_controller` directive. This means log events with custom `gc.task.*` targets were silently filtered out and never visible in default configuration.

Using the default `module_path!()` target (e.g., `global_controller::tasks::generic_health_checker`) makes logs visible under the `global_controller` filter directive. When refactoring, prefer module-path targets over custom targets unless the `EnvFilter` is explicitly configured to match them.

---

## Gotcha: Config Structs for 1-2 Fields Are Overkill
**Added**: 2026-02-12
**Related files**: `crates/gc-service/src/tasks/generic_health_checker.rs`

Avoid creating a config struct when a function only needs 1-2 simple parameters for differentiation. For example, `HealthCheckerConfig { display_name: &'static str, entity_name: &'static str }` was unnecessary when `entity_name: &'static str` as a plain parameter suffices. The `display_name` field only existed to prefix log messages (e.g., "MH " prefix), which is fragile (trailing space convention) and better handled by separate log messages in wrappers.

Rule of thumb: If a "config" struct has fewer than 3 fields and all are simple types, pass them as individual parameters. Structs add indirection (import, construction, field access) without proportional benefit.

---

## Gotcha: `#[instrument]` on Both Wrapper and Inner Function Creates Nested Spans
**Added**: 2026-02-12
**Related files**: `crates/gc-service/src/tasks/generic_health_checker.rs`, `crates/gc-service/src/tasks/health_checker.rs`

If a wrapper function has `#[instrument(skip_all, name = "gc.task.health_checker")]` and the inner generic function also has `#[instrument(skip_all)]`, every log event inside the generic function will have TWO nested spans: the wrapper's named span and the generic function's `start_generic_health_checker` span. This adds noise to traces without value.

Fix: Remove `#[instrument]` from the generic function entirely. Either keep it on the wrapper OR (preferred) use `.instrument(info_span!(...))` chaining on the call site, which gives the caller full control and avoids auto-capture of function parameters.

---

## Gotcha: Async Closure Lifetime Issues with `&PgPool` Parameter
**Added**: 2026-02-12
**Related files**: `crates/gc-service/src/tasks/generic_health_checker.rs`

When passing a closure `Fn(&PgPool, i64) -> Fut` where `Fut` is an async future, Rust cannot express the lifetime relationship between the `&PgPool` borrow and the returned future in the `Fn` trait bound. This causes `lifetime may not live long enough` errors.

Fix: Change the closure to take owned `PgPool` instead of `&PgPool`. Since `PgPool` wraps an `Arc` internally, cloning is cheap (reference count bump). The caller clones before each invocation, and the closure can take a reference inside:

```rust
// Generic function signature
F: Fn(PgPool, i64) -> Fut + Send,

// Caller clones
mark_stale_fn(pool.clone(), threshold).await

// Closure creates reference inside
|pool, threshold| async move {
    Repository::mark_stale(&pool, threshold).await
}
```

---
