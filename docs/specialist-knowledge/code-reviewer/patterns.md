# Code Reviewer - Patterns

High-value, Dark Tower-specific code quality patterns. Generic Rust idioms are omitted (clippy catches those).

---

## Pattern: Configuration Value Pattern (Constants + Validation + Tests)
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/config.rs`, `crates/global-controller/src/config.rs`

For configurable parameters with security or operational impact, use this four-part pattern: (1) define module-level constants with DEFAULT/MIN/MAX bounds and OWASP/NIST references, (2) add config struct field, (3) implement parsing with range validation and actionable error messages, (4) add comprehensive boundary tests. Makes security boundaries explicit and provides audit trail.

---

## Pattern: Defense-in-Depth Validation
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/config.rs`, `crates/ac-service/src/crypto/mod.rs`

Validate security-critical values at multiple layers: config parsing time AND at point of use. Even if config validation ensures valid ranges, crypto functions should independently verify inputs. Example: bcrypt cost validated in Config::from_vars AND in hash_client_secret. Prevents bugs if validation is bypassed or config is constructed programmatically.

---

## Pattern: SecretBox Custom Debug/Clone Implementation
**Added**: 2026-01-12
**Related files**: `crates/ac-service/src/config.rs`, `crates/meeting-controller/src/config.rs`

When a struct contains `SecretBox<T>` or `SecretString` fields, implement custom `Debug` with `&"[REDACTED]"` for sensitive fields. Document which fields are redacted in the doc comment. `SecretBox` doesn't implement Clone; implement manually when needed and document why Clone is required in the struct doc comment.

---

## Pattern: ADR References in Doc Comments
**Added**: 2026-01-25
**Related files**: `crates/meeting-controller/src/`, `crates/ac-service/src/crypto/mod.rs`

Document code implementing ADR requirements with explicit references in doc comments: `/// See ADR-0023 Section 4.2 for state machine requirements.` Creates bidirectional traceability between ADRs and code. Makes compliance audits easier.

---

## Pattern: Actor Handle/Task Separation (ADR-0001)
**Added**: 2026-01-25
**Related files**: `crates/meeting-controller/src/actors/meeting.rs`, `crates/meeting-controller/src/actors/controller.rs`

Separate public API (`Handle`) from private implementation (`Actor`). Handle contains `mpsc::Sender` + `CancellationToken`, provides async methods that send messages and await responses via oneshot channels. Actor owns all state and runs the message loop. Ensures all state mutations happen within actor task.

---

## Pattern: #[allow(clippy::expect_used)] with ADR-0002 Justification
**Added**: 2026-01-25
**Related files**: `crates/meeting-controller/src/actors/session.rs`

When `expect()` is unavoidable (CSPRNG operations, HKDF with fixed parameters), use `#[allow(clippy::expect_used)]` with inline comment explaining why this is an unreachable invariant per ADR-0002. Comment should explain the technical reason the operation cannot fail (e.g., "CSPRNG fill on 16 bytes is unreachable - SystemRandom only fails if OS is catastrophically broken").

---

## Pattern: SecretBox Performance Trade-off for Type Safety
**Added**: 2026-01-28
**Related files**: `crates/meeting-controller/src/actors/session.rs`, `crates/meeting-controller/src/actors/controller.rs`

`SecretBox<T>` intentionally doesn't implement Clone. For per-entity secret storage (meeting-specific secrets), minimal cloning at creation time is acceptable. Document pattern with ADR-0023 reference. Escalate to tech debt if cloning happens at multiple hot-path callsites. Balances type safety (compile-time leak prevention) with pragmatism (accepting minimal clones at strategic points).

---

## Pattern: Error Context Preservation with Security-Aware Logging
**Added**: 2026-01-29
**Related files**: `crates/ac-service/src/crypto/mod.rs`, `crates/ac-service/src/handlers/auth_handler.rs`

Use `tracing::error!` for internal failures (crypto, database, network). Use `tracing::debug!` for expected input validation failures. Always include `error = %e` for structured logging. Client-facing message should be generic and non-revealing. Balances debugging (detailed server-side logs) with security (generic client messages).

---

## Pattern: Unified Task Ownership (No Arc for Single Consumer)
**Added**: 2026-01-31
**Related files**: `crates/meeting-controller/src/main.rs`

When a task is the sole owner of a resource, let it own the value directly instead of wrapping in Arc. Simplifies code, removes reference counting overhead, makes ownership clear. Refactoring opportunity: multiple separate tasks → single unified task.

---

## Pattern: Never-Exit Resilience for Critical Background Tasks
**Added**: 2026-01-31
**Related files**: `crates/meeting-controller/src/main.rs`

Background tasks managing critical infrastructure (GC registration, health monitoring) should never exit on transient failures. Only exit on explicit cancellation signal. Log failures at warn level, implement fixed delay between retries, re-register on NOT_FOUND. Protects active meetings/sessions during infrastructure outages.

---

## Pattern: MockBehavior Enum for Test Configuration
**Added**: 2026-01-31
**Related files**: `crates/meeting-controller/tests/gc_integration.rs`

For mock servers simulating different behaviors, use a `MockBehavior` enum instead of boolean flags. Enables semantic variant names (Accept, Reject, NotFound, NotFoundThenAccept), stateful behaviors via atomic counters, single configuration point. Apply when mock needs 3+ distinct behaviors or stateful transitions.

---

## Pattern: Spawn-and-Wait Function API with (JoinHandle, Receiver) Tuple
**Added**: 2026-02-02
**Related files**: `crates/common/src/token_manager.rs`

For background tasks producing continuously-updated values, spawn the task and wait for first value before returning `(JoinHandle<()>, Receiver)` tuple. Function only returns after first valid value (no "not ready yet" state). Caller controls task lifetime via handle. Clearer ownership than struct with internal task.

---

## Pattern: OnceLock for Test Watch Channel Senders
**Added**: 2026-02-02
**Related files**: `crates/meeting-controller/src/grpc/gc_client.rs`, `crates/meeting-controller/tests/gc_integration.rs`

When tests need `watch::Receiver` that stays valid for test duration, use `OnceLock` to hold sender statically instead of `mem::forget`. No intentional memory leak, thread-safe initialization, self-documenting intent. Replaces `mem::forget(tx)` patterns in test code.

---

## Pattern: Metrics Cardinality Control via Path Normalization
**Added**: 2026-02-04
**Related files**: `crates/global-controller/src/observability/metrics.rs`

When recording HTTP metrics with path labels, normalize dynamic path segments to prevent label cardinality explosion. Replace UUIDs, meeting codes, and other high-cardinality values with placeholders:

```rust
fn normalize_endpoint(path: &str) -> String {
    match path {
        "/" | "/health" | "/metrics" | "/api/v1/me" => path.to_string(),
        _ if path.starts_with("/api/v1/meetings/") => {
            let parts: Vec<&str> = path.split('/').collect();
            match parts.len() {
                5 => "/api/v1/meetings/{code}".to_string(),
                6 if parts[5] == "guest-token" => "/api/v1/meetings/{code}/guest-token".to_string(),
                6 if parts[5] == "settings" => "/api/v1/meetings/{id}/settings".to_string(),
                _ => "/other".to_string(),
            }
        }
        _ => "/other".to_string(),  // Unknown paths normalized to bound cardinality
    }
}
```

**Key properties:**
- Known static paths returned as-is (exact match)
- Dynamic segments replaced with `{placeholder}` format
- Unknown paths fall through to `/other` (bounded cardinality)
- Tests should verify all known routes are normalized correctly

**ADR-0011 compliance:**
- Maximum unique label combinations per metric: 1,000
- Total cardinality budget: 5,000,000 time series
- Use indexed values instead of UUIDs for high-cardinality identifiers

This pattern applies to all services implementing metrics (AC, GC, MC, MH).

---

## Pattern: Module-Level Prometheus Documentation
**Added**: 2026-02-05
**Related files**: `crates/meeting-controller/src/actors/metrics.rs`

When adding Prometheus integration to internal tracking structs, document which types ARE wired, which metrics each produces, and which types are NOT wired. Prevents assumptions when a struct has increment methods but isn't actually wired to Prometheus. See `ControllerMetrics` (internal-only, not Prometheus) vs `ActorMetrics` (wired).

---

## Pattern: Complete Metric Instrumentation for Async RPC Calls
**Added**: 2026-02-10
**Related files**: `crates/meeting-controller/src/grpc/gc_client.rs`

For async RPC calls (gRPC, HTTP), follow the complete observability pattern: record both counter and histogram metrics in both success and error branches, measure duration before the async call:

```rust
pub async fn heartbeat(&self, ...) -> Result<(), McError> {
    // Start timer BEFORE the async call (captures total latency including network)
    let start = Instant::now();

    match client.heartbeat(request).await {
        Ok(response) => {
            let duration = start.elapsed();
            // Record success metrics: both counter and histogram
            record_heartbeat("success", "fast");
            record_heartbeat_latency("fast", duration);
            // ... process response
            Ok(())
        }
        Err(e) => {
            let duration = start.elapsed();
            // Record error metrics: both counter and histogram
            record_heartbeat("error", "fast");
            record_heartbeat_latency("fast", duration);
            // ... handle error
            Err(McError::Grpc(format!("Heartbeat failed: {e}")))
        }
    }
}
```

**Key properties:**
- Timer starts before the async call (captures network latency)
- Counter metric records result status (success/error)
- Histogram metric records latency for both paths
- Pattern works for gRPC, HTTP, and any async operation
- Enables SLO tracking (p99 latency, error rate)

**ADR-0011 compliance:**
- Labels are bounded (`status`: 2 values, `type`: 2-3 values)
- Histogram buckets should match SLO targets (e.g., 0.1s target → buckets include 0.05, 0.1, 0.5)

This pattern ensures complete observability for external dependencies, critical for debugging latency and reliability issues.
## Pattern: Generic Background Task with Closure + Plain Parameters
**Added**: 2026-02-12
**Updated**: 2026-02-12
**Related files**: `crates/global-controller/src/tasks/generic_health_checker.rs`, `crates/global-controller/src/tasks/health_checker.rs`, `crates/global-controller/src/tasks/mh_health_checker.rs`

When multiple background tasks share the same loop structure (interval tick, select with cancellation, error handling) but differ in the operation performed, extract a generic function parameterized by `Fn(PgPool, i64) -> Fut` closure. Pass entity-specific metadata as plain parameters (e.g., `entity_name: &'static str`) rather than a config struct -- a struct is overhead when there is only one field. The generic function should NOT carry `#[instrument]`; instead, callers chain `.instrument(tracing::info_span!("span.name"))` on the returned future. This gives each wrapper full control over its span name without nested spans. Wrapper functions handle startup/shutdown lifecycle logs with literal `target:` values. Zero-cost (monomorphized at compile time), no trait object overhead.

---

## Pattern: Wrapper Function Preserving Public API During Refactoring
**Added**: 2026-02-12
**Related files**: `crates/global-controller/src/tasks/health_checker.rs`, `crates/global-controller/src/tasks/mh_health_checker.rs`

When extracting shared logic into a generic function, keep thin wrapper functions with identical signatures to the originals. This means: no changes to `main.rs` call sites, no changes to test call sites, existing integration tests exercise the full pipeline through the wrapper. The wrapper is responsible for: providing the closure, chaining `.instrument(info_span!(...))` for tracing span context, and emitting lifecycle log lines with literal `target:` values. Tests stay in the wrapper module (not the generic module) since they test the specific entity behavior.

---

## Pattern: `.instrument()` Chaining for Caller-Controlled Spans
**Added**: 2026-02-12
**Related files**: `crates/global-controller/src/tasks/health_checker.rs`, `crates/global-controller/src/tasks/mh_health_checker.rs`

When a generic/shared async function is called by multiple wrappers that each need different span names, prefer `.instrument(tracing::info_span!("caller.span.name"))` chaining on the `.await` over `#[instrument(skip_all, name = "...")]` on the generic function. Benefits: (1) no nested spans (generic + wrapper), (2) each caller fully controls its own span name, (3) the `Instrument` trait import (`use tracing::Instrument`) is lightweight, (4) avoids the `instrument-skip-all` validation guard on the generic function since `.instrument()` is a runtime method call, not a proc-macro attribute. Use `tracing::info_span!` for the span level.

---
