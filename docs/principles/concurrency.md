# Principle: Concurrency

**All stateful concurrent components MUST use the Actor pattern.** No shared mutable state, no mutexes on hot paths.

**ADRs**: ADR-0001 (Actor Pattern), ADR-0002 (No-Panic)

---

## DO

### Actor Pattern
- **Use actors for stateful concurrent access** - caches, rate limiters, session state
- **Each actor owns its state exclusively** - no `Arc<Mutex<>>` on actor-owned data
- **Communicate via message passing** - Tokio `mpsc` channels for commands
- **Process messages sequentially** - simplifies reasoning, no internal locking
- **Return results via oneshot channels** - request/response pattern

### Actor Structure
- **Create handle types** - `MyActorHandle` wraps `mpsc::Sender`, clients call methods on handle
- **Spawn actor task** - `tokio::spawn(actor.run())` in handle constructor
- **Use message enums** - `enum ActorMessage { Command1 { data, respond_to }, ... }`
- **Include respond_to field** - `oneshot::Sender<Result<T, E>>` for responses

### Channel Configuration
- **Set appropriate buffer sizes** - 100-1000 typical, tune based on load
- **Handle send failures** - channel closed means actor died
- **Consider backpressure** - bounded channels provide natural rate limiting

### Error Handling
- **Return errors via oneshot channels** - never panic in message handlers
- **Log errors internally** - for debugging and alerting
- **Use supervision for recovery** - restart actors on fatal errors

### When to Use Actors
- Stateful components with concurrent access (caches, registries)
- Resource management (connection pools, rate limiters)
- Sequential operations on shared state
- Background tasks with coordination

---

## DON'T

### Shared State
- **NEVER use `Arc<Mutex<State>>`** for hot-path concurrent access
- **NEVER use `Arc<RwLock<State>>`** - same problems as Mutex
- **NEVER hold locks across await points** - causes deadlocks

### Actor Anti-Patterns
- **NEVER panic in message handlers** - return errors via oneshot instead
- **NEVER use actors for stateless operations** - use regular async functions
- **NEVER use actors for one-time initialization** - overkill
- **NEVER block in actor tasks** - use async operations

### Performance
- **NEVER use unbounded channels** - can cause memory exhaustion
- **NEVER ignore channel send errors** - indicates actor failure

---

## Quick Reference

### Actor Pattern Structure

| Component | Purpose |
|-----------|---------|
| `MyActor` struct | Owns state + receiver |
| `MyActorHandle` struct | Exposes API, owns sender |
| `ActorMessage` enum | Commands with respond_to |
| `run()` method | Message loop |

### When to Use What

| Scenario | Pattern |
|----------|---------|
| Concurrent stateful access | Actor |
| Stateless request handling | Async function |
| Pure computation | Regular function |
| One-time init | Lazy static or async init |
| Connection pool | Actor or dedicated pool crate |

### Channel Buffer Sizes

| Use Case | Typical Size |
|----------|--------------|
| Low-volume commands | 32-64 |
| High-throughput operations | 256-1024 |
| Unbounded (avoid) | Never |

### Example Actors in Dark Tower

| Service | Actor | Purpose |
|---------|-------|---------|
| AC | JwksManagerActor | JWKS cache + refresh |
| AC | KeyRotationActor | Key rotation schedule |
| GC | RateLimiterActor | Per-IP/user rate limiting |
| MC | MeetingStateActor | Per-meeting participant state |
| MH | RoutingTableActor | Media forwarding rules |

---

## Guards

**Clippy**: Warn on `Arc<Mutex<>>` in hot paths (manual review)
**Code Review**: Verify actor pattern for new stateful components
