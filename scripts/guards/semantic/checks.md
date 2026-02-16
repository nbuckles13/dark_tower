# Semantic Guard Checks

These checks are used by the `semantic-guard` agent during devloop validation. Each check describes a class of issues that pattern-based guards cannot catch reliably.

The agent analyzes the current diff (added/changed code only, excluding test files) against each check below.

---

## Check: Credential Leak

Look for these patterns in added/changed production code:

1. **Secrets in logs**: Passwords, tokens, secrets, or keys logged via `info!`, `debug!`, `warn!`, `error!`, `trace!`, or tracing macros.

2. **Missing `skip_all` in `#[instrument]`**: Functions with sensitive parameters (password, token, secret, key, credential) that use `#[instrument]` without `skip_all`. The `skip()` denylist approach is unsafe because new fields leak by default.

3. **Debug formatting secrets**: Structs containing secrets being formatted with `{:?}` in logs or errors.

4. **Error message leaks**: Error messages (`Err`, `anyhow!`, `bail!`) that include secret values.

---

## Check: Actor Blocking

In actor-based code (files in `actors/` directory or with "Actor" in struct names):

**Context**: Actors use a main `select!` loop pattern:
```rust
loop {
    tokio::select! {
        Some(msg) = self.receiver.recv() => { /* handle msg */ }
        _ = cancel.cancelled() => { break; }
    }
}
```

**SAFE patterns** (do not flag):
- Awaiting in `select!` branches (this IS the actor pattern)
- Awaiting `mpsc::Sender::send()` (backpressure, nearly instant)
- Awaiting oneshot for request-response within same message handling
- `tokio::spawn()` wrapping long operations (fire-and-forget)

**UNSAFE patterns** (flag these):
- Helper methods called by the actor that await external responses (blocks the message loop)
- `timeout(Duration::from_secs(N))` where N > 1 in non-`select!` context
- Awaiting `task_handle.await` (waiting for child task completion)
- Awaiting Redis/gRPC calls directly without `spawn()`

**Key insight**: The danger is when async methods CALLED BY the actor block the message loop. The actor can't process new messages while waiting.

---

## Check: Error Context Preservation

Look for `.map_err(|e| ...)` patterns where error context may be lost:

**UNSAFE patterns** (flag these):

1. **Error logged but not included in returned error**:
```rust
.map_err(|e| {
    tracing::error!("Operation failed: {}", e);
    MyError::Internal  // Error context logged but not in returned error
})
```

2. **Generic error message without original context**:
```rust
.map_err(|e| MyError::Crypto("Encryption failed".to_string()))  // No context from e
```

3. **Error variable captured but not used**:
```rust
.map_err(|e| MyError::Internal("Something failed".to_string()))  // e captured but unused
```

**SAFE patterns** (do not flag):

1. **Error context included in returned error**:
```rust
.map_err(|e| MyError::Internal(format!("Operation failed: {}", e)))
```

2. **Error context in structured error type**:
```rust
.map_err(|e| MyError::CryptoError {
    msg: "Encryption failed".to_string(),
    source: e.to_string()
})
```

**Key principle**: The error variable `e` should be included in the RETURNED error type, not just logged and discarded. Client-facing errors can use generic messages, but the underlying error should capture full context.

---

## Check: Metrics Path Completeness

When a function records metrics (counter!, histogram!, gauge!) on some code paths,
verify that ALL exit paths record equivalent metrics.

UNSAFE patterns (flag these):
1. Early return via `?` that bypasses metric recording when other paths in the
   same function record metrics
2. `match`/`if let` branches where some arms record metrics and others don't
3. Error paths that `return Err(...)` before reaching metric recording calls

SAFE patterns (do not flag):
- Pure metrics functions (functions whose only purpose is recording metrics)
- Functions that record metrics unconditionally (all paths go through recording)
- Test code
- Functions where the early return is before any business logic (e.g., input
  validation at function start, before the operation being measured)

Key insight: Look for functions where `histogram!`, `counter!`, or `gauge!` calls
appear deep in the function body, then check whether earlier `?` or `return`
statements can exit the function before reaching those calls.
