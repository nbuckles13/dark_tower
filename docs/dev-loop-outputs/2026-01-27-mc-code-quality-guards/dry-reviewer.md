# DRY Reviewer Checkpoint

**Task**: Fix code quality issues in meeting-controller (error hiding, instrument skip-all, actor blocking)

**Date**: 2026-01-27

**Reviewed Files**:
- `crates/meeting-controller/src/actors/connection.rs`
- `crates/meeting-controller/src/actors/controller.rs`
- `crates/meeting-controller/src/actors/meeting.rs`
- `crates/meeting-controller/src/errors.rs`
- `crates/meeting-controller/src/grpc/gc_client.rs`
- `crates/meeting-controller/src/grpc/mc_service.rs`
- `crates/meeting-controller/src/redis/client.rs`

---

## Pattern Analysis

### 1. Error Pattern: `Internal(String)`

**Meeting Controller Pattern**:
```rust
// crates/meeting-controller/src/errors.rs:76-77
#[error("Internal error: {0}")]
Internal(String),
```

**Common Crate Pattern**:
```rust
// crates/common/src/error.rs:37-38
#[error("Internal error: {0}")]
Internal(String),
```

**AC Service Pattern**:
```rust
// crates/ac-service/src/errors.rs:41-42
#[error("Internal server error")]
Internal,  // Unit variant without context
```

**Analysis**:
- `common::error::DarkTowerError::Internal(String)` exists and is identical in pattern
- However, `McError` is a domain-specific error type with additional meeting-controller-specific variants (SessionBinding, Draining, Migrating, FencedOut)
- MC cannot simply re-export `DarkTowerError` because it needs these domain-specific variants
- The `Internal(String)` pattern is duplicated but there's no `common` abstraction to reduce this duplication

**Verdict**: NOT A BLOCKER - The `Internal(String)` variant in `common` is part of a general error type, not a reusable component. MC legitimately needs its own error enum with domain-specific variants. This is acceptable pattern replication, not code that should be shared.

### 2. Instrument Pattern: `#[instrument(skip_all, fields(...))]`

**Meeting Controller Pattern**:
```rust
#[instrument(skip_all, name = "mc.actor.controller", fields(mc_id = %self.mc_id))]
async fn run(mut self) { ... }
```

**Global Controller Pattern** (for comparison):
```rust
#[instrument(skip_all, name = "gc.grpc.register_mc")]
async fn register_mc(...) { ... }
```

**AC Service Pattern** (for comparison):
```rust
#[instrument(skip_all)]
pub fn sign_claims(...) { ... }
```

**Analysis**:
- The `skip_all` pattern is used consistently across all services
- Each service has its own naming convention (`mc.`, `gc.`, `ac.`)
- This is a standard tracing pattern, not duplicated business logic
- No abstraction in `common` could simplify this - it's intrinsic to how tracing works

**Verdict**: NOT A FINDING - This is standard tracing instrumentation, not code duplication. Each service must define its own spans with appropriate context.

### 3. Background Spawn Pattern: `tokio::spawn(async move { ... cleanup ... })`

**Meeting Controller Pattern** (new):
```rust
// crates/meeting-controller/src/actors/controller.rs:441-469
tokio::spawn(async move {
    match tokio::time::timeout(Duration::from_secs(5), managed.task_handle).await {
        Ok(Ok(())) => { debug!(...); }
        Ok(Err(e)) => { warn!(...); }
        Err(_) => { warn!(...); }
    }
});
```

**Global Controller Pattern** (for comparison):
```rust
// crates/global-controller/src/main.rs:121-124
let health_checker_handle = tokio::spawn(async move {
    start_health_checker(...).await;
});
```

**Analysis**:
- The GC uses `tokio::spawn` for long-running background tasks (health checker, cleanup)
- The MC uses `tokio::spawn` for fire-and-forget cleanup of meeting actor task handles
- These are fundamentally different use cases:
  - GC: Long-running daemon tasks
  - MC: Fire-and-forget cleanup with timeout
- No shared abstraction exists or would be beneficial

**Verdict**: NOT A FINDING - Different use cases, not duplicated business logic. The spawn patterns serve different architectural purposes.

---

## Summary

| Pattern | Location in Common | Used by MC | Severity |
|---------|-------------------|------------|----------|
| `Internal(String)` error variant | `common::error::DarkTowerError` | No - MC has own error enum | N/A - Acceptable |
| `skip_all` instrument | N/A (tracing pattern) | Yes | N/A - Standard pattern |
| Background spawn cleanup | N/A (not applicable) | Yes | N/A - Different use case |

---

## Findings

### BLOCKER: None

No code exists in `common` that MC should be using but isn't.

### TECH_DEBT: None

While there are similar patterns across services (error enums with string variants, instrument patterns), these are:
1. Standard Rust idioms (error enums)
2. Standard tracing patterns (instrument macros)
3. Service-specific by nature (domain errors, span naming)

Extracting these to `common` would not provide meaningful value and could reduce code clarity.

---

## Verdict

**APPROVED**

The implementation correctly uses service-specific error types and standard patterns. No duplication of business logic was detected. The `Internal(String)` variant pattern exists in `common` but is part of a general error type (`DarkTowerError`) that doesn't fit MC's domain-specific needs.

---

## Recommendations

None required for this implementation. The patterns used are appropriate for the meeting-controller domain.

---

# DRY Reviewer Checkpoint - Iteration 2 (SecretBox Fix)

**Date**: 2026-01-28

**Task**: Verify SecretBox migration for `master_secret` from `Vec<u8>` to `SecretBox<Vec<u8>>` maintains consistency with other services.

**Reviewed Files**:
- `crates/meeting-controller/src/actors/session.rs` (line 29, 44, 236, 322, 364)
- `crates/meeting-controller/src/actors/meeting.rs` (line 26, 342, 353)
- `crates/meeting-controller/src/actors/controller.rs` (line 25, 63, 206, 364)

---

## Pattern Analysis

### SecretBox Import Consistency

**Meeting Controller Pattern**:
```rust
// crates/meeting-controller/src/actors/session.rs:16
use common::secret::{ExposeSecret, SecretBox};

// crates/meeting-controller/src/actors/meeting.rs:26
use common::secret::SecretBox;

// crates/meeting-controller/src/actors/controller.rs:25
use common::secret::{ExposeSecret, SecretBox};
```

**AC Service Pattern** (for comparison):
```rust
// crates/ac-service/src/config.rs:2
use common::secret::{ExposeSecret, SecretBox};

// crates/ac-service/src/crypto/mod.rs:7
use common::secret::{ExposeSecret, SecretBox, SecretString};

// crates/ac-service/src/services/token_service.rs:9
use common::secret::SecretBox;
```

**Verdict**: ✅ CONSISTENT - All services import `SecretBox` and `ExposeSecret` from `common::secret`, not directly from `secrecy`. Pattern matches AC service exactly.

---

### SecretBox Construction Pattern

**Meeting Controller Pattern**:
```rust
// crates/meeting-controller/src/actors/session.rs:236 (test)
SecretBox::new(Box::new(vec![0u8; 32]))

// crates/meeting-controller/src/actors/controller.rs:364 (production)
let meeting_secret = SecretBox::new(Box::new(self.master_secret.expose_secret().clone()));
```

**AC Service Pattern** (for comparison):
```rust
// crates/ac-service/src/services/token_service.rs:multiple locations
SecretBox::new(Box::new(signing_key.private_key_encrypted))
SecretBox::new(Box::new(signing_key_model.private_key_encrypted.clone()))
```

**Verdict**: ✅ CONSISTENT - Pattern matches how AC service constructs SecretBox. Using `Box::new()` wrapper is standard across both services.

---

### Secret Exposure Pattern

**Meeting Controller Pattern**:
```rust
// crates/meeting-controller/src/actors/session.rs:46
master_secret.expose_secret().len() >= 32

// crates/meeting-controller/src/actors/session.rs:159
self.master_secret.expose_secret()

// crates/meeting-controller/src/actors/controller.rs:364
self.master_secret.expose_secret().clone()
```

**AC Service Pattern**:
```rust
// crates/ac-service/src/crypto/mod.rs (multiple locations)
// Used for accessing wrapped secrets before cryptographic operations
```

**Verdict**: ✅ CONSISTENT - All uses of `expose_secret()` are immediately before cryptographic operations (HKDF, length validation). No secrets are logged or held after operation completes. Pattern matches security guidance in `common/src/secret.rs`.

---

### Master Secret Propagation Pattern

**Meeting Controller Hierarchy**:
```
MeetingControllerActor (owns SecretBox<Vec<u8>>)
  ↓ clone via expose_secret() + Box::new
MeetingActor::spawn(master_secret: SecretBox<Vec<u8>>)
  ↓ passed to
SessionBindingManager::new(master_secret: SecretBox<Vec<u8>>)
```

**Analysis**:
- Controller clones the exposed secret bytes before wrapping in new SecretBox (line 364)
- This is necessary because `SecretBox<Vec<u8>>` cannot be directly cloned (would expose memory pattern)
- Alternative: Use `Arc<SecretBox<Vec<u8>>>` - not done, which is appropriate (adds complexity)
- Each actor level gets its own SecretBox instance wrapping cloned bytes
- No secret is ever exposed in logs or error messages

**Verdict**: ✅ CORRECT DESIGN - The clone-expose-rewrap pattern is necessary and appropriate for distributing secrets to child actors while maintaining Secret wrapper semantics.

---

## Duplication Check

### Is This Code Already in `common`?

**Question**: Could `master_secret: SecretBox<Vec<u8>>` handling be abstracted to `common`?

**Analysis**:
- `common::secret` already exports `SecretBox` and `ExposeSecret` - REUSED ✅
- The pattern of wrapping cryptographic keys in `SecretBox<Vec<u8>>` is used by:
  - AC Service: Ed25519 signing keys
  - Meeting Controller: HMAC-SHA256 derivation keys
- Both services follow identical import + construction + exposure patterns
- The hierarchical propagation pattern (parent → child actors) is MC-specific and shouldn't be in `common`

**Verdict**: ✅ NO DUPLICATION - Both services correctly use the abstraction that already exists in `common::secret`. No new code should be extracted.

---

### Consistency with Global Controller

**Global Controller Pattern**:
```rust
// crates/global-controller/src/services/mc_client.rs
use common::secret::{ExposeSecret, SecretString};
```

Note: GC uses `SecretString` for OAuth secrets (text), not `SecretBox<Vec<u8>>` (binary). This is appropriate for the domain.

**Verdict**: ✅ DOMAIN-APPROPRIATE - Each service uses the right secret type for its data:
- AC: `SecretBox<Vec<u8>>` for Ed25519 key bytes
- MC: `SecretBox<Vec<u8>>` for HMAC key bytes
- GC: `SecretString` for OAuth client secret strings

---

## Summary

| Aspect | Status | Notes |
|--------|--------|-------|
| Import path | ✅ APPROVED | Uses `common::secret`, not direct `secrecy` |
| Construction | ✅ APPROVED | `SecretBox::new(Box::new(...))` matches AC pattern |
| Exposure pattern | ✅ APPROVED | Only exposed immediately before crypto ops |
| Propagation | ✅ APPROVED | Correct clone-expose-rewrap for child actors |
| Abstraction reuse | ✅ APPROVED | Correctly delegates to existing `common::secret` |
| Consistency | ✅ APPROVED | Identical patterns across AC, MC, GC (domain-appropriate) |

---

## Findings

### BLOCKER: None

No violations of DRY principle detected. All secret handling correctly uses existing `common::secret` abstractions.

### TECH_DEBT: None

The SecretBox migration properly leverages the existing secret type infrastructure. No opportunities for further abstraction that wouldn't add complexity.

---

## Verdict

**APPROVED**

The SecretBox migration maintains consistency with AC Service patterns and correctly uses the `common::secret` abstraction layer. Import statements, construction, and exposure patterns are all appropriate for security-sensitive cryptographic operations. No code duplication detected.

**Confidence**: HIGH (matched against 3 services + common crate patterns)
