# Meeting Controller Patterns

Reusable patterns discovered and established in the Meeting Controller codebase.

---

## Pattern: Actor Handle/Task Separation
**Added**: 2026-01-25
**Related files**: `crates/meeting-controller/src/actors/controller.rs`, `crates/meeting-controller/src/actors/meeting.rs`, `crates/meeting-controller/src/actors/connection.rs`

Separate each actor into a Handle struct and internal Task. The Handle exposes the public API (async methods with oneshot channels for request-response), owns the `mpsc::Sender`, and can be cheaply `Clone`d. The Task owns the receiver, runs the message loop, and holds all mutable state. Use `spawn()` to create both and return `(Handle, JoinHandle<()>)`. This enables: safe cloning of handles for broadcasting, clear ownership of actor state, and monitoring via `JoinHandle::is_finished()`.

---

## Pattern: CancellationToken Parent-Child Hierarchy
**Added**: 2026-01-25
**Related files**: `crates/meeting-controller/src/actors/controller.rs`, `crates/meeting-controller/src/actors/meeting.rs`

Use `tokio_util::sync::CancellationToken` with parent-child relationships for graceful shutdown. The controller owns the root token; meetings get `cancel_token.child_token()`; connections get the meeting's child token. When parent cancels, all children cancel automatically. In the message loop, use `tokio::select!` with `cancel_token.cancelled()` as the first branch to handle shutdown. This ensures orderly cleanup: controller cancels, meetings drain, connections close.

---

## Pattern: HMAC-SHA256 with HKDF for Session Binding
**Added**: 2026-01-25
**Related files**: `crates/meeting-controller/src/actors/session.rs`

For session binding tokens per ADR-0023: derive meeting-specific keys via `HKDF-SHA256(master_secret, salt=meeting_id, info="session-binding")`, then compute `HMAC-SHA256(meeting_key, correlation_id || participant_id || nonce)`. Use `ring::hmac::verify()` for constant-time validation. Store nonce and binding together in `StoredBinding` with 30s TTL. On reconnect, validate then rotate by generating new correlation ID and token.

---

## Pattern: Async State Queries via Oneshot Channels
**Added**: 2026-01-25
**Related files**: `crates/meeting-controller/src/actors/controller.rs`, `crates/meeting-controller/src/actors/meeting.rs`

For operations requiring the actor's current state (e.g., `get_status()`, `get_meeting()`), send a message containing a `oneshot::Sender<Result<T, E>>` and await the response. This ensures the response reflects the actual actor state at processing time, not a potentially stale cache. Pattern: `let (tx, rx) = oneshot::channel(); sender.send(Message::Query { respond_to: tx }).await?; rx.await?`

---

## Pattern: tokio::time::pause for Deterministic Time Tests
**Added**: 2026-01-25
**Related files**: `crates/meeting-controller/src/actors/meeting.rs` (tests)

Use `#[tokio::test(start_paused = true)]` for tests involving time-based behavior like grace periods or TTLs. This pauses the Tokio runtime's internal clock, then use `tokio::time::advance(Duration)` to fast-forward time deterministically. Essential for testing the 30-second disconnect grace period without actually waiting. Remember: `tokio::time::sleep()` still works - it advances time instantly.

---

## Pattern: Proto Evolution for Session Binding
**Added**: 2026-01-25
**Related files**: `proto/signaling.proto`, `crates/proto-gen/src/`

When adding session recovery capabilities to protos, extend existing request/response messages rather than creating new RPCs. For `JoinRequest`, add optional `session_token` and `last_sequence_number` fields. For `JoinResponse`, add `session_token`, `expiry_timestamp`, and `recovery_data` fields. This allows clients to opportunistically provide recovery data without breaking existing flows - servers ignore unknown fields, and new clients work with old servers.

---

## Pattern: Two-Tier Mute State Model
**Added**: 2026-01-25
**Related files**: `proto/signaling.proto`, `crates/meeting-controller/src/session/`

Implement mute state with two distinct tiers: self-mute (informational) and host-mute (enforced). Self-mute is a client preference that can be toggled freely - the MC tracks it but doesn't enforce it. Host-mute is authoritative and overrides self-mute - when host-muted, the client MUST be muted regardless of self-mute state. Proto fields: `is_self_muted: bool` (client-controlled), `is_host_muted: bool` (host-controlled). UI should show different indicators for each state.

---

## Pattern: Config Following GC Structure
**Added**: 2026-01-25
**Related files**: `crates/meeting-controller/src/config.rs`, `crates/global-controller/src/config.rs`

Follow the established GC config pattern for consistency:
1. Group required secrets in dedicated section with `SecretString` types
2. Implement `Debug` manually with `[REDACTED]` for sensitive fields
3. Provide `from_vars()` method that loads from environment variables for production
4. Provide `for_testing()` method that returns safe defaults for tests
5. Use typed duration fields (e.g., `session_timeout: Duration`) not raw integers

---

## Pattern: Error Hierarchy with Nested Domain Errors
**Added**: 2026-01-25
**Related files**: `crates/meeting-controller/src/error.rs`

Define top-level `McError` enum containing nested domain-specific error types. Example: `McError::SessionBinding(SessionBindingError)` where `SessionBindingError` has variants like `TokenExpired`, `SequenceGap`, `InvalidNonce`. This provides both high-level error categorization for HTTP/gRPC status mapping and detailed domain errors for logging/debugging. Map to appropriate status codes at API boundaries.

---

## Pattern: Test Utils Builder for Mock Services
**Added**: 2026-01-25
**Related files**: `crates/mc-test-utils/src/lib.rs`

Create dedicated `mc-test-utils` crate with builder pattern for mock services. `MockRedisBuilder` should support:
- `with_session(session_id, session_data)` - pre-populate sessions
- `with_fencing_token(meeting_id, token)` - support fencing scenarios
- `with_nonce(participant_id, nonce)` - replay protection testing
- `build()` -> `MockRedis`

This mirrors `ac-test-utils` pattern and enables both unit and integration tests to use the same mock infrastructure without `#[cfg(test)]` limitations.

---

## Pattern: Fenced Redis Writes with Lua Scripts
**Added**: 2026-01-25
**Related files**: `crates/meeting-controller/src/redis/lua_scripts.rs`, `crates/meeting-controller/src/redis/client.rs`

For split-brain prevention in distributed systems, use Lua scripts for atomic fenced operations. Each write includes a monotonically-increasing generation counter (fencing token). The Lua script atomically: (1) reads current generation, (2) compares with provided generation, (3) writes only if generation is current or newer. Return codes indicate success (1), fenced-out (0), or error (-1). Store scripts as `const &str` and precompile with `redis::Script::new()` at client construction time.

---

## Pattern: AtomicU32/AtomicBool for Lock-Free Capacity Checks
**Added**: 2026-01-25
**Related files**: `crates/meeting-controller/src/grpc/mc_service.rs`

For capacity-based accept/reject decisions in gRPC handlers, use atomic types (`AtomicU32`, `AtomicBool`) with `Ordering::SeqCst`. Pattern: check draining state first (instant rejection), then meeting capacity, then estimate participant headroom using a constant (e.g., `ESTIMATED_PARTICIPANTS_PER_MEETING = 10`). Return specific `RejectionReason` enum variants so the calling service can make informed retry decisions. This avoids mutex contention on the hot path.

---

## Pattern: Cheaply Cloneable Connection Types (Channel, MultiplexedConnection)
**Added**: 2026-01-29
**Related files**: `crates/meeting-controller/src/grpc/gc_client.rs`, `crates/meeting-controller/src/redis/client.rs`

Both tonic `Channel` and redis-rs `MultiplexedConnection` are designed to be cheaply cloneable and used concurrently without external locking. Do NOT wrap them in `Arc<Mutex>` or `Arc<RwLock>`. Instead, store directly and clone for each request. For `GcClient`, create the channel eagerly at startup (fail fast) and make the constructor async/fallible. For `FencedRedisClient`, derive `Clone` on the struct so actors can own their own copy. These types handle reconnection internally.

---

## Pattern: Eager vs Lazy Connection Initialization
**Added**: 2026-01-29
**Related files**: `crates/meeting-controller/src/grpc/gc_client.rs`

For critical infrastructure connections (like MC→GC), prefer eager initialization: create the connection at startup and fail fast if unreachable. This reveals configuration issues immediately and simplifies code (no `Option<T>` or lazy init logic). The constructor becomes `async fn new(...) -> Result<Self, Error>`. For non-critical connections where startup shouldn't block, lazy init may still be appropriate.

---

## Pattern: gRPC Auth Interceptor for Bearer Token Validation
**Added**: 2026-01-25
**Related files**: `crates/meeting-controller/src/grpc/auth_interceptor.rs`

Implement `tonic::service::Interceptor` for authorization validation on incoming gRPC requests. Pattern: extract `authorization` metadata, validate `Bearer ` prefix (case-sensitive), check token is non-empty and within size limits (8KB max). Return generic error messages (e.g., "Invalid token") to prevent information leakage. Include `#[cfg(test)] pub fn disabled()` constructor for testing without auth.

---

## Pattern: SecretBox with expose_secret().clone() for Non-Clone Types
**Added**: 2026-01-28
**Related files**: `crates/meeting-controller/src/config.rs`, `crates/meeting-controller/src/actors/session.rs`

When storing non-Clone types (like `ring::hkdf::Prk`) in `SecretBox<T>`, the standard pattern of deriving Clone fails. Solution: (1) Don't derive Clone on the config struct, or (2) Manually implement Clone with `expose_secret().clone()` to access the inner value. This is intentionally explicit and grep-able. Pattern: `SecretBox::new(prk)` for storage, then `config.master_secret.expose_secret().clone()` for cloning. This maintains security (debug redaction) while working with non-Clone cryptographic types. Import `secrecy::ExposeSecret` trait to access the method.

---

## Pattern: Mock gRPC Server for Integration Tests
**Added**: 2026-01-31
**Related files**: `crates/meeting-controller/tests/gc_integration.rs`

For testing gRPC client code, create a mock server implementing the service trait. Pattern: (1) Bind `TcpListener::bind("127.0.0.1:0")` to get a random port, (2) Wrap listener with `tokio_stream::wrappers::TcpListenerStream`, (3) Use `Server::builder().add_service(...).serve_with_incoming_shutdown(incoming, token)`, (4) Spawn in background task, (5) Create client pointing to `listener.local_addr()`. Use channels (`mpsc`, `AtomicU32`) to track calls and verify behavior. Add `tokio-stream` as dev-dependency.

---

## Pattern: MockBehavior Enum for Stateful Mock Servers
**Added**: 2026-01-31
**Related files**: `crates/meeting-controller/tests/gc_integration.rs`

For testing complex interaction flows (like re-registration), use a behavior enum to control mock responses. Define states like `Accept`, `Reject`, `NotFound`, `NotFoundThenAccept`. Use atomic counters to track call count and switch behavior based on state + count. Example: `NotFoundThenAccept` returns NOT_FOUND on first heartbeat, then accepts subsequent ones - perfect for testing recovery flows. This avoids separate mocks for each test scenario and enables testing state transitions.

---

## Pattern: Unified Service Integration Task (Never-Exit Resilience)
**Added**: 2026-01-31
**Related files**: `crates/meeting-controller/src/main.rs`, `crates/meeting-controller/src/grpc/gc_client.rs`

For critical service dependencies (like MC→GC), create a single unified task that owns the client directly (no Arc). Pattern: (1) Initial registration with infinite retry loop (never exits), (2) Dual operations in single `tokio::select!` (e.g., fast/comprehensive heartbeats), (3) Detect NOT_FOUND errors and re-register automatically, (4) Never exit on connectivity issues - protects active state. This provides production-grade resilience: service survives dependency restarts, network partitions, and rolling updates without manual intervention.

---

## Pattern: Atomic Metrics Snapshot for Consistent Reporting
**Added**: 2026-01-31
**Related files**: `crates/meeting-controller/src/actors/metrics.rs`

For reporting multiple related metrics atomically, provide a `snapshot()` method that returns a struct with all values read in sequence. While individual atomics with `SeqCst` ordering are consistent, reading multiple atomics separately can see inconsistent intermediate states during concurrent updates. A snapshot struct (with `meetings` and `participants`) ensures both counters are read together, providing cleaner API and consistent reporting in heartbeats or logs.

---

## Pattern: OnceLock for Test Helper Singletons
**Added**: 2026-02-02
**Related files**: `crates/meeting-controller/src/grpc/gc_client.rs` (tests), `crates/meeting-controller/tests/gc_integration.rs`

When tests need a long-lived sender (e.g., `watch::Sender`) that outlives the test function to keep receivers valid, use `std::sync::OnceLock` instead of `mem::forget`. Pattern: `static SENDER: OnceLock<watch::Sender<T>> = OnceLock::new(); let sender = SENDER.get_or_init(|| { let (tx, _rx) = watch::channel(initial_value); tx }); receiver_from(sender.subscribe())`. This avoids memory leaks, is more idiomatic, and the static ensures the sender lives for the program duration. Works well with `TokenReceiver::from_test_channel()` pattern.

---

## Pattern: Feature-Gated Test Constructors
**Added**: 2026-02-02
**Related files**: `crates/common/src/token_manager.rs`, `crates/meeting-controller/Cargo.toml`

When a type needs a test-only constructor that bypasses normal initialization (e.g., `TokenReceiver` without spawning `TokenManager`), gate it behind a feature flag: `#[cfg(any(test, feature = "test-utils"))] pub fn from_test_channel(rx: watch::Receiver<SecretString>) -> Self`. Consumers add the feature in dev-dependencies: `common = { path = "../common", features = ["test-utils"] }`. This keeps test utilities out of production builds while allowing integration tests in other crates to use them.

---

## Pattern: Timeout-Wrapped Startup for Fail-Fast Behavior
**Added**: 2026-02-02
**Related files**: `crates/meeting-controller/src/main.rs`

For critical startup dependencies (like TokenManager acquiring initial token), wrap the operation in `tokio::time::timeout()` to fail fast rather than hang indefinitely. Pattern: `let token_rx = tokio::time::timeout(Duration::from_secs(30), token_manager.subscribe()).await.map_err(|_| "Timeout acquiring token")?.map_err(|e| format!("Token error: {e}"))?;`. This reveals configuration issues (wrong endpoint, invalid credentials) immediately at startup rather than during first request.

---

## Pattern: Metrics Crate Facade with Wrapper Functions
**Added**: 2026-02-05
**Related files**: `crates/meeting-controller/src/observability/metrics.rs`

Use the `metrics` crate facade pattern (matching AC per ADR-0011) instead of the `prometheus` crate directly. Create thin wrapper functions for each metric: `pub fn record_websocket_connection() { counter!("mc_websocket_connections_total").increment(1); }`. This provides: (1) type-safe metric recording, (2) centralized metric naming, (3) consistent labels, (4) easy testing (mock the wrapper, not the crate). Requires `PrometheusBuilder::new().install()` at startup before any metric recording.

---

## Pattern: Fail-Fast Server Binding Before Task Spawn
**Added**: 2026-02-05
**Related files**: `crates/meeting-controller/src/observability/health.rs`, `crates/meeting-controller/src/main.rs`

For internal servers (health endpoints, metrics), bind the TCP listener synchronously BEFORE spawning the server task. Pattern: `let listener = TcpListener::bind(addr).await?; tokio::spawn(async move { serve(listener).await });`. This ensures startup fails immediately if the port is unavailable, rather than spawning a task that silently fails later. The bound listener is passed to the spawned task, guaranteeing the port is reserved.

---

## Pattern: Health Endpoints Matching AC Structure
**Added**: 2026-02-05
**Related files**: `crates/meeting-controller/src/observability/health.rs`, `crates/ac-service/src/health.rs`

For consistency across services, use AC's health endpoint pattern: `/health` for liveness (always returns 200 OK if server is running) and `/ready` for readiness (checks dependencies like GC registration, Redis connection). Do NOT use Kubernetes-style paths like `/health/live` and `/health/ready`. Health responses use JSON: `{"status": "healthy"}` or `{"status": "ready", "registered": true}`. The `HealthState` struct tracks readiness and is shared via `Arc<HealthState>` across tasks.

---

## Pattern: Integration Tests with tower::util::ServiceExt::oneshot()
**Added**: 2026-02-05
**Related files**: `crates/meeting-controller/tests/health_integration.rs`

For testing HTTP services without starting a full server, use tower's `ServiceExt::oneshot()` method. Pattern: `let response = app.clone().oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap()).await.unwrap();`. This executes a single request through the service stack synchronously. Requires `tower = { version = "0.5", features = ["util"] }` and `http-body-util` for body collection. This is faster and more deterministic than spawning a server and making HTTP requests.

---

## Pattern: Shared Metrics Propagation Through Actor Hierarchy
**Added**: 2026-02-05
**Related files**: `crates/meeting-controller/src/actors/controller.rs`, `crates/meeting-controller/src/actors/meeting.rs`, `crates/meeting-controller/src/main.rs`

When metrics need to be updated from child actors but aggregated at a higher level (e.g., participant count for GC heartbeats), pass `Arc<Metrics>` through the actor hierarchy. Pattern: create shared metrics in main.rs, pass to controller constructor, controller passes clone to each meeting actor's spawn(). Each actor stores its own `Arc` clone. This enables child actors to call `increment()`/`decrement()` methods that update the shared counter atomically. Works for both `ActorMetrics` (Prometheus emission) and `ControllerMetrics` (GC heartbeat reporting).

---

## Pattern: Module Alias for Observability Calls
**Added**: 2026-02-05
**Related files**: `crates/meeting-controller/src/actors/metrics.rs`, `crates/meeting-controller/src/observability/metrics.rs`

When internal metrics structs have methods like `meeting_created()` that also need to emit to Prometheus, use a module alias to distinguish the observability call: `use crate::observability::metrics as prom;`. Then internal tracking and Prometheus emission are clearly separated: `self.active_meetings.fetch_add(1, Ordering::Relaxed); prom::set_meetings_active(count as u64);`. This makes code self-documenting and grep-able - all Prometheus emissions use the `prom::` prefix.

---

## Pattern: Comprehensive Module-Level Metric Documentation
**Added**: 2026-02-11
**Related files**: `crates/meeting-controller/src/actors/metrics.rs`, `crates/meeting-controller/src/observability/metrics.rs`

Document metric relationships and integration patterns at the module level, not just in individual function docs. The `actors/metrics.rs` module doc explains three key integrations: (1) How ActorMetrics maps to Prometheus gauges/counters, (2) How MailboxMonitor thresholds map to ADR-0023 alerting, (3) That ControllerMetrics is GC-only (no Prometheus). This prevents confusion when maintaining either system - the module doc is the source of truth for "which metrics go where". Individual function docs focus on per-metric semantics (labels, cardinality, SLO targets).

---
