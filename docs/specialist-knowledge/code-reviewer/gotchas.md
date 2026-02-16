# Code Reviewer - Gotchas

Dark Tower-specific anti-patterns. Generic issues (unwrap in production, etc.) are caught by clippy.

---

## Gotcha: Single-Layer Security Validation
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/crypto/mod.rs`

Security parameters validated only at config parse time can be bypassed via programmatic construction. Always validate at point of use too. Example: bcrypt cost checked in Config::from_vars AND in hash_client_secret (defense-in-depth per ADR-0002).

---

## Gotcha: Magic Numbers Without Constants
**Added**: 2026-01-11
**Updated**: 2026-01-27
**Related files**: `crates/mc-service/src/grpc/mc_service.rs`

Numeric values with domain meaning (security parameters, capacity estimates) need named constants with doc comments explaining derivation. Example: `ESTIMATED_PARTICIPANTS_PER_MEETING` should document: "Based on P50=4, P90=8. Using 10 provides 20% headroom." Explains "why" and consequences if wrong.

---

## Gotcha: Deriving Debug on Structs with SecretBox Fields
**Added**: 2026-01-12
**Related files**: `crates/ac-service/src/config.rs`

Do NOT use `#[derive(Debug)]` on structs containing `SecretBox<T>` or `SecretString`. Derived Debug may expose sensitive context (database URLs with credentials). Always implement Debug manually. Look for structs with secret fields that derive Debug.

---

## Gotcha: Inconsistent Redaction Placeholder Strings
**Added**: 2026-01-12
**Related files**: All services

Use consistent `"[REDACTED]"` across all Debug implementations. Inconsistent placeholders (`"***"`, `"<hidden>"`, `"[SECRET]"`) make log analysis harder. Grep for redaction patterns to verify consistency.

---

## Gotcha: Health Check HTTP 200 vs Error Status
**Added**: 2026-01-14
**Related files**: `crates/gc-service/src/handlers/health.rs`

Returning error status (500) when probe fails causes HTTP request failure and probe timeout. K8s expects to parse response body. BAD: `.map_err(|_| GcError::DatabaseUnavailable)`. GOOD: `let db_healthy = sqlx::query().await.is_ok()` then return 200 with status field.

---

## Gotcha: Confusing Service Layer vs Repository Layer Errors
**Added**: 2026-01-15
**Related files**: `crates/ac-service/src/services/`, `crates/ac-service/src/repositories/`

Repository errors are internal details. Service layer should wrap in domain-specific errors. Don't leak repository errors. Pattern: repository returns `DatabaseError::UniqueViolation`, service wraps in `UserServiceError::EmailAlreadyExists`.

---

## Gotcha: #[expect(dead_code)] vs #[allow(dead_code)]
**Added**: 2026-01-22
**Related files**: All services

Use `#[allow(dead_code)]` with comment explaining future use, not `#[expect(dead_code)]`. `#[expect]` causes "unfulfilled lint expectation" warnings when code is used. Common when scaffolding future phases.

---

## Gotcha: Duplicate Logging Between Repository and Service Layers
**Added**: 2026-01-22
**Related files**: `crates/gc-service/src/`

Choose ONE layer for logging. Repository: database-specific details. Service: business operations. Typically prefer service layer (business context). Duplicate logging clutters observability.

---

## Gotcha: Silent Config Fallback to Defaults
**Added**: 2026-01-25
**Updated**: 2026-01-28
**Related files**: `crates/mc-service/src/config.rs`, `crates/gc-service/src/config.rs`

Config parsing that silently falls back to defaults when env vars are invalid masks misconfiguration in production. For security-critical settings (JWT clock skew, bcrypt cost), always fail. For operational settings, log warning minimum. GC's JWT clock skew parsing is reference: returns ConfigError on invalid input.

---

## Gotcha: std::sync::Mutex in Async Test Mocks
**Added**: 2026-01-25
**Related files**: `crates/mc-test-utils/src/mock_redis.rs`

`std::sync::Mutex` in async mock implementations can deadlock or perform poorly. Use `tokio::sync::Mutex`. Flag as tech debt if found in test-utils crates.

---

## Gotcha: Wrong Error Variant for Communication Type
**Added**: 2026-01-27
**Related files**: `crates/mc-service/src/grpc/gc_client.rs`

Use semantically correct error variants for protocol (Redis, gRPC, HTTP). Wrong variant (e.g., `McError::Redis` for gRPC call) confuses debugging and breaks error handling. Matters for observability dashboards and retry strategies.

---

## Gotcha: Synchronous get_* Methods in Actor Handles
**Added**: 2026-01-25
**Related files**: `crates/mc-service/src/actors/controller.rs`

Actor handle methods retrieving state from child actors MUST be async. Synchronous getters return stale cached values, leading to incorrect status (e.g., participant_count always 0). Query child actor asynchronously for live state.

---

## Gotcha: Missing Graceful Fallback When Actor Communication Fails
**Added**: 2026-01-25
**Related files**: `crates/mc-service/src/actors/controller.rs`

When querying child actors that may have shut down, handle error gracefully. Returning error breaks status endpoints during graceful shutdown. Log warning and return safe default (e.g., participant_count: 0).

---

## Gotcha: SecretBox Clone Performance vs Type Safety Trade-off
**Added**: 2026-01-28
**Related files**: `crates/mc-service/src/actors/controller.rs`

Don't immediately flag `.expose_secret().clone()` as waste. `SecretBox` prevents cloning to protect against leaks. Occasional clones at initialization (per-entity) acceptable if: (1) not hot path, (2) comment explains pattern with ADR reference, (3) Security approved. Red flag: many clones across hot-path callsites - escalate for `Arc<SecretBox<T>>` consideration.

---

## Gotcha: unwrap_or_default() Discards Error Context
**Added**: 2026-02-02
**Related files**: `crates/common/src/token_manager.rs`

`.unwrap_or_default()` on `Result` silently discards error, violating ADR-0002. Use `unwrap_or_else` with trace/warn logging to preserve diagnostic context.

---

## Gotcha: Hardcoded Placeholder Secrets with TODO Comments
**Added**: 2026-02-02
**Related files**: All services

Placeholder secrets (e.g., `vec![0u8; 32]` with TODO) compile but are insecure. Flag as MINOR requiring immediate fix: load from config with base64 decode, minimum length validation, constants for magic numbers.

---

## Gotcha: Unexpressed Metric Availability Assumptions
**Added**: 2026-02-05
**Related files**: `crates/mc-service/src/actors/metrics.rs`

Structs with increment methods naturally suggest Prometheus wiring. Not all metrics are wired - some are internal-only. Document at module level which types ARE wired, which are NOT. See `ControllerMetrics` (internal, not Prometheus) vs `ActorMetrics` (wired). Prevents dashboard confusion.

---

## Gotcha: tracing `target:` Requires String Literal
**Added**: 2026-02-12
**Related files**: `crates/gc-service/src/tasks/generic_health_checker.rs`

The `target:` argument in tracing macros (`info!`, `warn!`, `error!`) must be a **string literal** at compile time. Even `&'static str` variables will not compile. This blocks a common generic/reusable function pattern where you want to parameterize log targets. Workarounds: (1) drop custom `target:` and rely on `#[instrument(name = "...")]` spans on caller for filtering, (2) keep `target:` log lines in caller wrappers and only extract non-logging logic into the generic, (3) use `tracing::span!` with dynamic name at caller level. Discovered during TD-13 health checker extraction.

---

## Gotcha: `display_name` Fields with Baked-In Formatting (RESOLVED)
**Added**: 2026-02-12
**Resolved**: 2026-02-12
**Related files**: `crates/gc-service/src/tasks/generic_health_checker.rs`

Config struct fields used in `format!` strings that require trailing spaces or specific formatting (e.g., `display_name: "MH "` with trailing space, vs `display_name: ""` for no prefix) are fragile API design. **Resolution**: TD-13 iteration 2 removed the config struct and `display_name` entirely, using `entity_name: &'static str` as a plain parameter. Entity differentiation now comes from the structured `entity` field in log events and the parent span set by the wrapper's `.instrument()`. Prefer this approach when a "display prefix" field is the only differentiator.

---

## Gotcha: `#[instrument]` Attribute vs `.instrument()` Method -- Guard Behavior
**Added**: 2026-02-12
**Related files**: `crates/gc-service/src/tasks/generic_health_checker.rs`

The `instrument-skip-all` validation guard pattern-matches on `#[instrument(` proc-macro attributes only. The `.instrument(info_span!(...))` runtime method call (from `tracing::Instrument` trait) is NOT detected by the guard. This means removing `#[instrument]` from a generic function and having callers use `.instrument()` chaining will NOT trigger the guard. Important distinction when choosing between attribute-based and method-based instrumentation in generic functions.

---

## Gotcha: `status_code()` Semantic Divergence Across Services
**Added**: 2026-02-16
**Related files**: `crates/mc-service/src/errors.rs`, `crates/gc-service/src/errors.rs`

`GcError::status_code()` and `McError::status_code()` share identical signatures (`&self -> u16`) for metrics recording consistency, but return semantically different values. GC returns HTTP status codes (200-503) because it serves HTTP/REST. MC returns signaling error codes (2-7) because it uses WebTransport. MC's `status_code()` wraps the existing `error_code() -> i32` with an `i32 as u16` cast. A reviewer seeing `status_code` on an MC error might mistakenly expect HTTP codes. The doc comment on `McError::status_code()` explains this, but when reviewing metrics dashboards, verify that `status_code` label values are interpreted correctly for each service (6 = INTERNAL_ERROR in MC, 500 = Internal Server Error in GC).

---

## Gotcha: Crate Rename vs Domain-Level Identifiers
**Added**: 2026-02-16
**Related files**: `crates/ac-service/src/models/mod.rs`, `crates/env-tests/tests/40_resilience.rs`

When renaming crate directories/packages (e.g., `global-controller` → `gc-service`), distinguish between **crate-level names** (Cargo.toml package, lib, bin, `use` paths, directory paths) and **domain-level identifiers** (AC `service_type` values like `"global-controller"` stored in the database, used in JWT claims, validated in API handlers). Crate names change; domain identifiers do NOT -- they are part of the API contract and DB schema. However, **K8s labels** (`app=gc-service`) referenced in test code (env-tests canary pods) MUST match the renamed K8s manifests. The env-tests are an easy-to-miss location because they're Rust code containing K8s label strings, not YAML manifests.

---

## Gotcha: Inconsistent `job=` Filter Convention Across Service Dashboards
**Added**: 2026-02-16
**Related files**: `infra/grafana/dashboards/ac-overview.json`, `infra/grafana/dashboards/gc-overview.json`, `infra/grafana/dashboards/mc-overview.json`

AC dashboard panels include `job="ac-service"` filters in PromQL expressions for application metrics, but GC and MC dashboards omit `job=` filters entirely on application metrics. Both conventions are valid (service-prefixed metric names like `gc_*` are already scoped to a single service, making `job=` redundant). This is pre-existing -- when reviewing new dashboard panels, follow the existing convention for that specific dashboard file rather than enforcing cross-dashboard uniformity. If a future PR standardizes this, it should be a separate cleanup across all dashboards simultaneously.

---