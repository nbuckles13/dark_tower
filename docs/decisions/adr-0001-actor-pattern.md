# ADR-0001: Actor Pattern for Concurrency

**Status**: Accepted

**Date**: 2025-01-22

**Deciders**: Security Specialist, Global Controller Specialist, Meeting Controller Specialist, Media Handler Specialist

---

## Context

Dark Tower is a highly concurrent system with multiple components managing shared state:
- JWKS cache accessed by all validation requests
- Meeting state accessed by multiple participants
- Routing tables updated while forwarding media
- Rate limiters tracking per-user/per-IP quotas

**Problems with traditional approaches**:
- **Mutexes/RwLocks**: Lock contention, potential deadlocks, difficult to reason about
- **Shared mutable state**: Race conditions, complex synchronization
- **Global state**: Hard to test, tight coupling

**Requirements**:
- High concurrency (thousands of requests/second)
- No deadlocks
- Testable concurrency patterns
- Clear ownership of state

## Decision

**We adopt the Actor Pattern as the primary concurrency model for Dark Tower.**

### Actor Pattern Principles

1. **Each actor owns its state exclusively** - no shared mutable state
2. **Actors communicate via message passing** - using Tokio channels
3. **Actors process messages sequentially** - simplifies reasoning
4. **Actors are isolated** - failure in one doesn't affect others

### Implementation Pattern

```rust
use tokio::sync::{mpsc, oneshot};

// Message enum for actor communication
enum ActorMessage {
    DoSomething {
        data: SomeData,
        respond_to: oneshot::Sender<Result<Response, Error>>,
    },
    Shutdown,
}

// Actor owns all state
struct MyActor {
    receiver: mpsc::Receiver<ActorMessage>,
    state: MyState,  // Exclusively owned - no Arc, no Mutex
}

impl MyActor {
    async fn run(mut self) {
        while let Some(msg) = self.receiver.recv().await {
            match msg {
                ActorMessage::DoSomething { data, respond_to } => {
                    let result = self.handle_something(data);
                    let _ = respond_to.send(result);
                }
                ActorMessage::Shutdown => break,
            }
        }
    }
}

// Handle for clients to interact with actor
#[derive(Clone)]
struct MyActorHandle {
    sender: mpsc::Sender<ActorMessage>,
}

impl MyActorHandle {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel(100);
        let actor = MyActor::new(receiver);
        tokio::spawn(actor.run());
        Self { sender }
    }

    pub async fn do_something(&self, data: SomeData) -> Result<Response, Error> {
        let (send, recv) = oneshot::channel();
        self.sender.send(ActorMessage::DoSomething { data, respond_to: send }).await?;
        recv.await?
    }
}
```

### When to Use Actors

**Use actors for**:
- ✅ Stateful components with concurrent access
- ✅ Resource management (connection pools, caches, rate limiters)
- ✅ Sequential operations on shared state
- ✅ Background tasks with coordination

**Don't use actors for**:
- ❌ Stateless request handlers
- ❌ Pure computation (use regular async functions)
- ❌ One-time initialization

### Examples Across Dark Tower

**Auth Controller**:
- `JwksManagerActor` - Manages JWKS cache and refresh
- `TokenIssuerActor` - Rate-limited token generation
- `KeyRotationActor` - Manages key rotation schedule

**Global Controller**:
- `MeetingControllerRegistryActor` - Tracks available MCs
- `RateLimiterActor` - Per-IP, per-user rate limiting

**Meeting Controller**:
- `MeetingStateActor` - One per meeting, manages participant state
- `LayoutManagerActor` - Computes layout updates
- `MediaRoutingActor` - Decides routing for media streams

**Media Handler**:
- `RoutingTableActor` - Manages forwarding rules
- `TelemetryAggregatorActor` - Collects metrics

## Consequences

### Positive

- ✅ **No deadlocks**: Message passing eliminates lock-based synchronization
- ✅ **Clear ownership**: Each actor owns its state, no shared mutable state
- ✅ **Testable**: Send messages, verify responses - easy to unit test
- ✅ **Isolated failures**: Actor crash doesn't affect others
- ✅ **Natural rate limiting**: Actor processes messages sequentially
- ✅ **Easy to reason about**: State transitions are explicit (message handlers)

### Negative

- ❌ **Message overhead**: Small allocation/copying cost per message
- ❌ **Learning curve**: Developers must understand actor model
- ❌ **Indirection**: Extra layer compared to direct function calls

### Neutral

- Channel capacity must be tuned (100-1000 typical)
- Supervision strategy needed for actor failures (restart, escalate, ignore)

## Alternatives Considered

### Alternative 1: Mutexes/RwLocks

**Approach**: Use `Arc<Mutex<State>>` or `Arc<RwLock<State>>`

**Pros**:
- Familiar to most Rust developers
- Direct access to state

**Cons**:
- Lock contention under high concurrency
- Potential deadlocks with multiple locks
- Hard to test (need to simulate race conditions)
- Easy to accidentally hold locks across await points

**Why not chosen**: Complexity grows with system scale, deadlock risk too high

### Alternative 2: Lock-Free Data Structures

**Approach**: Use atomic operations and lock-free collections (e.g., `crossbeam`)

**Pros**:
- Maximum performance
- No blocking

**Cons**:
- Very complex to implement correctly
- Limited to specific data structures
- Hard to debug
- Overkill for most use cases

**Why not chosen**: Premature optimization, too complex for most components

### Alternative 3: Single-Threaded Event Loop

**Approach**: Single thread processes all events (like Node.js)

**Pros**:
- No concurrency issues
- Simple mental model

**Cons**:
- Doesn't utilize multiple cores
- Blocking operations stall entire system
- Not suitable for CPU-intensive tasks

**Why not chosen**: Doesn't meet performance requirements

## Implementation Notes

### Actor Patterns

**Pattern 1: Single Actor per Resource**
```rust
// One actor manages one meeting's state
struct MeetingStateActor {
    meeting_id: String,
    participants: HashMap<ParticipantId, Participant>,
}
```

**Pattern 2: Actor Pool**
```rust
// Pool of actors for load distribution
struct ActorPool {
    actors: Vec<mpsc::Sender<Message>>,
}
```

**Pattern 3: Hierarchical Actors**
```rust
// Parent supervises child actors
struct Supervisor {
    children: HashMap<Id, mpsc::Sender<ChildMessage>>,
}
```

### Error Handling

Actors should:
1. Log errors internally
2. Return errors via oneshot channels to callers
3. Not panic (use `?` operator and Result types)
4. Optionally restart on fatal errors (supervision)

### Testing

```rust
#[tokio::test]
async fn test_actor_behavior() {
    let handle = MyActorHandle::new();
    let result = handle.do_something(test_data).await?;
    assert_eq!(result, expected);
}
```

### Migration Strategy

- New components: Use actors from the start
- Existing components: Refactor incrementally
  1. Identify shared state
  2. Create actor to own that state
  3. Replace direct access with message passing
  4. Remove mutexes

## References

- Tokio Actors: https://ryhl.io/blog/actors-with-tokio/
- Actor Model: https://en.wikipedia.org/wiki/Actor_model
- Implementation: See `JwksManagerActor` in Auth Controller
- Related: ADR-0002 (Error Handling), ADR-0003 (Service Authentication)
