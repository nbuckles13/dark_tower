# Meeting Controller Gotchas

Mistakes to avoid and edge cases discovered in the Meeting Controller codebase.

---

## Gotcha: Borrow Checker Blocks Broadcast After Mutable Update
**Added**: 2026-01-25
**Related files**: `crates/mc-service/src/actors/meeting.rs`

When updating participant state (e.g., mute status) then broadcasting to other participants, you cannot hold a mutable borrow of `self.participants` while calling `self.broadcast_update()`. Solution: extract the update into a local variable inside the `if let Some(participant) = self.participants.get_mut(id)` block, then broadcast after the block closes. Pattern: `let update = if let Some(p) = self.participants.get_mut(id) { p.field = value; Some(Update { ... }) } else { None }; if let Some(u) = update { self.broadcast(u).await; }`

---

## Gotcha: Don't Include IDs in Error Messages (MINOR-002)
**Added**: 2026-01-25
**Related files**: `crates/mc-service/src/actors/meeting.rs`, `crates/mc-service/src/actors/controller.rs`

Avoid including meeting IDs, participant IDs, or user IDs in error messages returned to clients. These can leak information for enumeration attacks. Use generic messages like "Participant not found" or "Meeting already exists" instead of "Participant part-123 not found". Log the full details server-side for debugging.

---

## Gotcha: StoredBinding TTL Uses Instant, Not SystemTime
**Added**: 2026-01-25
**Related files**: `crates/mc-service/src/actors/session.rs`

`StoredBinding::is_expired()` uses `Instant::now()` and `elapsed()` which is monotonic and immune to system clock changes. This is intentional - TTL checks should use monotonic time. However, `tokio::time::pause()` works with Tokio's internal clock, not `std::time::Instant`. For tests needing expired bindings, either: (1) use `#[tokio::test(start_paused = true)]` which affects both, or (2) construct binding with a custom `created_at` in the past.

---

## Gotcha: Clippy Warns on Excessive Bools in Proto-Generated Code
**Added**: 2026-01-25
**Related files**: `crates/proto-gen/src/lib.rs`, `proto/signaling.proto`

Proto messages with multiple boolean fields (e.g., `is_self_muted`, `is_host_muted`, `is_video_enabled`) trigger Clippy's `fn_params_excessive_bools` lint on generated code. Add `#![allow(clippy::fn_params_excessive_bools)]` to `proto-gen/src/lib.rs` since we can't control prost's code generation. This is acceptable - proto field types are dictated by protocol design, not Rust ergonomics.

---

## Gotcha: Dead Code Warnings in Skeleton Crates
**Added**: 2026-01-25
**Related files**: `crates/mc-service/src/config.rs`, `crates/mc-service/src/error.rs`

Skeleton crates defining types for future use trigger dead_code warnings. Add `#[allow(dead_code)]` with explanatory comment: `// Skeleton: will be used in Phase 6b`. Do NOT use `#[expect(dead_code)]` - it warns when the code IS eventually used, requiring removal. The `#[allow(...)]` attribute silently permits unused code without complaining when it becomes used.

---

## Gotcha: Doc Markdown Lint Requires Backticks
**Added**: 2026-01-25
**Related files**: `crates/mc-service/src/*.rs`

Rustdoc markdown linter requires backticks around code identifiers in doc comments. Write `/// Validates the \`session_token\` field` not `/// Validates the session_token field`. Also use backticks for: type names, field names, function names, enum variants, and file paths. The `cargo doc` command will warn but not fail; `cargo clippy` with `warn(rustdoc::all)` makes these errors.

---

## Gotcha: New Crates Must Be Added to Workspace Members
**Added**: 2026-01-25
**Related files**: `Cargo.toml`, `crates/mc-test-utils/Cargo.toml`

When creating new crates like `mc-test-utils`, you must add them to the workspace `members` array in the root `Cargo.toml`. Forgetting this causes: `cargo build` ignores the crate, `cargo test --workspace` skips its tests, and inter-crate dependencies fail to resolve. Always verify with `cargo metadata --no-deps | jq '.workspace_members'` after adding a new crate.

---

## Gotcha: Redis Script Fluent API and Borrow Checker
**Added**: 2026-01-25
**Related files**: `crates/mc-service/src/redis/client.rs`

The `redis::Script` fluent API (`.key().key().arg().arg()`) creates temporary values that conflict with Rust's borrow checker when building complex invocations with many KEYS/ARGV. Solution: For scripts with variable-length arguments (e.g., HSET with multiple field/value pairs), use raw `redis::cmd("EVALSHA")` with `.arg()` in a loop. Try EVALSHA first, fall back to EVAL if script not cached (handling `ErrorKind::NoScriptError`).

---

## Gotcha: Don't Log Redis URLs with Credentials
**Added**: 2026-01-25
**Related files**: `crates/mc-service/src/redis/client.rs`

Redis URLs may contain credentials (e.g., `redis://:password@host:port`). Never include `redis_url` in error logs or tracing spans. Log the error message without the URL: `error!(error = %e, "Failed to connect to Redis")`. Store the URL as `SecretString` in config and avoid exposing it anywhere in logs, even on connection failures where including the URL seems helpful for debugging.

---

## Gotcha: Config Fields Must Be SecretString for Credentials
**Added**: 2026-01-25
**Related files**: `crates/mc-service/src/config.rs`

All config fields containing credentials or secrets must use `SecretString` from `common::secret`, not plain `String`. This includes: `redis_url` (may contain password), `binding_token_secret` (HMAC key), and any future service tokens. Update the manual `Debug` impl to show `[REDACTED]` for these fields. Tests need `ExposeSecret` trait import to access the inner value.

---

## Gotcha: Bearer Token Prefix is Case-Sensitive
**Added**: 2026-01-25
**Related files**: `crates/mc-service/src/grpc/auth_interceptor.rs`

The `Bearer ` prefix in authorization headers is case-sensitive per RFC 6750. Use `strip_prefix("Bearer ")` not case-insensitive matching. Reject `bearer `, `BEARER `, etc. as invalid format. This is important for security - being permissive about case could lead to unexpected behavior if mixed with systems that ARE case-sensitive.

---

## Gotcha: Connection Types Are Not Stateful Components
**Added**: 2026-01-29
**Related files**: `crates/mc-service/src/grpc/gc_client.rs`, `crates/mc-service/src/redis/client.rs`

The project principle "NEVER use `Arc<Mutex<State>>`" applies to actor-owned state, NOT connection handles. Types like tonic `Channel` and redis-rs `MultiplexedConnection` are explicitly designed to be cloned and shared concurrently - they're connection handles, not stateful components. Do not wrap them in `Arc<RwLock>`. The principle prevents lock contention on hot-path actor state; connection types already handle internal synchronization.

---

## Gotcha: sysinfo API Differences Between Versions
**Added**: 2026-01-31
**Related files**: `crates/mc-service/src/system_info.rs`

The `sysinfo` crate API changed between versions. Version 0.30 uses `sys.global_cpu_info().cpu_usage()` (not `sys.global_cpu_usage()` which doesn't exist). The method returns `f32`, not a struct with a field. Always check the specific version's documentation when using sysinfo, especially when upgrading.

---

## Gotcha: MissedTickBehavior::Burst for Deterministic Test Tick Counts
**Added**: 2026-01-31
**Related files**: `crates/mc-service/tests/heartbeat_tasks.rs`

When testing interval-based tasks with `tokio::time::advance()`, using `MissedTickBehavior::Skip` (production default) can cause flaky tests - advancing by 3 seconds doesn't guarantee 3 ticks because missed ticks are skipped. Use `MissedTickBehavior::Burst` in tests to ensure all ticks fire, making assertions predictable. Production code should still use Skip to avoid thundering-herd on wake.

---

## Gotcha: Start gRPC Server BEFORE Client Registration
**Added**: 2026-01-31
**Related files**: `crates/mc-service/src/main.rs`

When integrating services via gRPC, start your inbound gRPC server BEFORE attempting outbound registration with the peer service. If you register with GC before starting the MC's gRPC server, GC may immediately try to call MC (e.g., `AssignMeeting`) and fail because the server isn't ready yet. Correct order: (1) Redis/actors, (2) Start gRPC server, (3) Register with GC, (4) Spawn background tasks. This prevents race conditions during startup.

---

## Gotcha: Base64 Secrets Need Length Validation After Decoding
**Added**: 2026-02-02
**Related files**: `crates/mc-service/src/main.rs`

When loading secrets from environment variables as base64, validate BOTH the base64 format AND the decoded length. A valid base64 string might decode to insufficient bytes for cryptographic use. Pattern: decode first, then check `decoded.len() >= MIN_LENGTH`. For HMAC-SHA256 keys, require at least 32 bytes. Log descriptive errors without revealing the actual secret: `error!("MC_BINDING_TOKEN_SECRET must be at least {MIN_LENGTH} bytes")`. This catches configuration errors at startup rather than cryptographic failures at runtime.

---

## Gotcha: Semantic Guard False Positive on Error Context
**Added**: 2026-02-02
**Related files**: `crates/mc-service/src/main.rs`, `scripts/guards/semantic-guard.sh`

The semantic guard's `error-context-preservation` check flags `e.to_string()` as potentially losing context, but this can false-positive when the string IS preserving context (e.g., `format!("Failed to decode: {e}")`). Similarly, `credential-leak-prevention` may flag error messages containing words like "token" or "invalid" even when no actual credentials are present. Fix by rewording the error message to avoid trigger patterns, or if truly a false positive, document why in a comment. Don't disable the guard entirely - it catches real issues.

---

## Gotcha: Don't Use Kubernetes-Style Health Paths
**Added**: 2026-02-05
**Related files**: `crates/mc-service/src/observability/health.rs`

Use `/health` and `/ready` endpoints (matching AC pattern), NOT Kubernetes-style paths like `/health/live` and `/health/ready`. While Kubernetes probes are configurable, the AC pattern is the established standard across all Dark Tower services. Consistency makes debugging and documentation easier. Kubernetes probe configs can point to `/health` and `/ready` directly.

---

## Gotcha: PrometheusBuilder Must Install Before Metric Recording
**Added**: 2026-02-05
**Related files**: `crates/mc-service/src/main.rs`, `crates/mc-service/src/observability/metrics.rs`

The `metrics_exporter_prometheus::PrometheusBuilder::new().install()` call must execute BEFORE any `counter!()`, `gauge!()`, or `histogram!()` macros are invoked. If metrics are recorded before the exporter is installed, they are silently dropped and won't appear in Prometheus scrapes. In main.rs, call PrometheusBuilder install as one of the first async operations, before creating actors or services that might record metrics on construction.

---

## Gotcha: Arc<HealthState> Required for Cross-Task Sharing
**Added**: 2026-02-05
**Related files**: `crates/mc-service/src/observability/health.rs`, `crates/mc-service/src/main.rs`

The `HealthState` struct must be wrapped in `Arc` when shared between the health server and tasks that update state (like the GC registration task). Pattern: `let health_state = Arc::new(HealthState::new()); let health_clone = Arc::clone(&health_state);`. The health server owns one Arc, and the registration task owns another to call `health_state.set_registered(true)` when GC registration succeeds. Using bare `HealthState` without Arc causes ownership/borrow issues across async task boundaries.

---

## Gotcha: Participant Count Decrement on BOTH Leave AND Disconnect Timeout
**Added**: 2026-02-05
**Related files**: `crates/mc-service/src/actors/meeting.rs`

When tracking participant counts with `ControllerMetrics`, remember to decrement in TWO places: (1) `handle_leave()` for explicit participant leave, and (2) `check_disconnect_timeouts()` when the 30-second grace period expires and participant is removed. Missing either causes count drift. Note: do NOT decrement on reconnect - reconnection reuses the existing participant slot, so the count stays the same. Pattern: call `controller_metrics.decrement_participants()` anywhere `self.participants.remove()` is called.

---

## Gotcha: fetch_sub Returns Previous Value, Not New Value
**Added**: 2026-02-05
**Related files**: `crates/mc-service/src/actors/metrics.rs`

Atomic `fetch_sub(1)` returns the value BEFORE subtraction, not after. When emitting the new count to Prometheus after decrement: `let count = self.active_meetings.fetch_sub(1, Ordering::Relaxed).saturating_sub(1);`. The `saturating_sub(1)` calculates the actual new value. Use `saturating_sub` instead of plain subtraction to handle the edge case where the previous value was already 0 (shouldn't happen in correct code, but prevents underflow panic in debug builds).

---

## Gotcha: Dashboard Metric Names Must Match Code Exactly
**Added**: 2026-02-10
**Related files**: `infra/grafana/dashboards/mc-overview.json`, `crates/mc-service/src/observability/metrics.rs`

Grafana dashboard queries must use the exact metric name defined in code - the `metrics` crate uses the literal string passed to `counter!()`, `gauge!()`, or `histogram!()`. Common mistake: singular vs plural (e.g., `mc_gc_heartbeat_total` vs `mc_gc_heartbeats_total`). When adding new metrics, grep the dashboard JSON for the metric name to ensure consistency. If panels show "No data", verify the query matches the code exactly, including suffixes like `_total` for counters and `_seconds` for duration histograms.

---
