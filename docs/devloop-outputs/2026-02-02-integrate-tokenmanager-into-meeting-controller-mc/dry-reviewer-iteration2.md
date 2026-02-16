# DRY Review Iteration 2 (ADR-0019)

**Reviewer**: DRY Reviewer
**Date**: 2026-02-02
**Iteration**: 2 (post-fix)
**Verdict**: APPROVED

## Summary

Review of iteration 2 fixes (OnceLock pattern for mock token receivers, master secret loading pattern). The iteration 2 changes do not introduce any new BLOCKER-level duplication. The OnceLock pattern was applied to consolidate the existing mock_token_receiver() duplication identified in TD-16 (dry-reviewer.md), but the core pattern itself (mock_token_receiver helper function) still exists in two locations within MC. The master secret loading pattern is new and currently has only one occurrence. No changes to blocking status.

---

## Iteration 2 Changes Reviewed

| File | Change Type | DRY Assessment |
|------|-------------|----------------|
| `crates/meeting-controller/src/main.rs:145-170` | Master secret loading (base64 + validation) | No BLOCKER - single occurrence |
| `crates/meeting-controller/src/grpc/gc_client.rs:642-659` | OnceLock pattern for test helper | No BLOCKER - refinement of TD-16 |
| `crates/meeting-controller/tests/gc_integration.rs:267-279` | OnceLock pattern for test helper | No BLOCKER - refinement of TD-16 |

---

## New Findings

### BLOCKER (Safety/Security/Correctness Duplication)

None

### TECH_DEBT (Non-Blocking Duplication)

#### TD-18: Master Secret Loading Pattern

| Field | Value |
|-------|-------|
| New code | `crates/meeting-controller/src/main.rs:145-170` |
| Existing code | None (first occurrence) |
| Occurrences | 1 |
| Severity | TECH_DEBT (non-blocking) |

**Description**: MC now has a master secret loading pattern that:
1. Reads base64-encoded secret from config (`binding_token_secret`)
2. Decodes base64 using `base64::engine::general_purpose::STANDARD`
3. Validates minimum length (32 bytes for HMAC-SHA256)
4. Wraps in `SecretBox<Vec<u8>>`

**Rationale for non-blocking**: This is the first occurrence of this pattern. Currently:
- GC does not use session binding tokens (different architecture)
- Media Handler is skeleton only
- Pattern may be service-specific (session binding is MC-specific per ADR-0023)

**Monitoring**: If GC or MH needs similar secret loading in the future, consider:
1. Extracting `load_master_secret()` to `common::secret` module
2. Creating a `SecretLoader` utility with configurable validation
3. Evaluating if secret types differ enough to warrant separate patterns

**Follow-up action**: Document during GC/MH development if similar pattern emerges.

---

## Updated Findings from Iteration 1

### TD-16: Mock TokenReceiver Test Utility Duplication (Updated)

| Field | Updated Value |
|-------|---------------|
| Occurrences | 2 (unchanged) |
| Pattern improvement | Both locations now use OnceLock pattern |
| Severity | TECH_DEBT (non-blocking) |

**Iteration 2 changes**: Both `mock_token_receiver()` functions now use the `OnceLock<watch::Sender<SecretString>>` pattern to avoid memory leaks from `mem::forget`. This is a code quality improvement but does not change the duplication status.

**Current state**:
```rust
// gc_client.rs (unit tests)
fn mock_token_receiver() -> TokenReceiver {
    use std::sync::OnceLock;
    static TOKEN_SENDER: OnceLock<watch::Sender<SecretString>> = OnceLock::new();
    let sender = TOKEN_SENDER.get_or_init(|| {
        let (tx, _rx) = watch::channel(SecretString::from("test-token"));
        tx
    });
    TokenReceiver::from_test_channel(sender.subscribe())
}

// gc_integration.rs (integration tests)
fn mock_token_receiver() -> TokenReceiver {
    use std::sync::OnceLock;
    static TOKEN_SENDER: OnceLock<watch::Sender<SecretString>> = OnceLock::new();
    let sender = TOKEN_SENDER.get_or_init(|| {
        let (tx, _rx) = watch::channel(SecretString::from("test-service-token"));
        tx
    });
    TokenReceiver::from_test_channel(sender.subscribe())
}
```

**Difference**: Token value is `"test-token"` vs `"test-service-token"` (minor, could be parameterized).

**Follow-up action**: Same as iteration 1 - consider extracting to `common::token_manager::test_helpers` when GC integrates TokenManager.

---

## Not Duplication (Justified Patterns)

### OnceLock Pattern Itself

The `OnceLock<watch::Sender<T>>` pattern for creating test fixtures is a standard Rust idiom for:
- Avoiding memory leaks in test helpers
- Ensuring channel sender stays alive across test invocations

This is NOT duplication to be concerned about - it's appropriate use of standard library primitives.

### MIN_SECRET_LENGTH Constant

The constant `MIN_SECRET_LENGTH: usize = 32` in `main.rs:60` is:
- Service-specific (MC's security requirement for binding tokens)
- Related to HMAC-SHA256 key size requirement
- Not duplicated elsewhere currently

**Note**: If similar secret length validation appears in other services, consider extracting to `common::secret::MIN_HMAC_KEY_LENGTH` or similar.

---

## Cross-Reference: Previous Tech Debt Status

| ID | Pattern | Status After Iteration 2 |
|----|---------|--------------------------|
| TD-16 | Mock TokenReceiver Helper | Active, improved (OnceLock pattern) |
| TD-17 | OAuth Config Fields | Active (unchanged) |
| TD-18 | Master Secret Loading | **NEW** (single occurrence) |

---

## Files Reviewed (Iteration 2 Changes Only)

| File | Lines Changed | DRY Assessment |
|------|---------------|----------------|
| `crates/meeting-controller/src/main.rs` | 145-170 (secret loading) | TECH_DEBT (TD-18) - single occurrence |
| `crates/meeting-controller/src/grpc/gc_client.rs` | 642-659 (OnceLock) | No new duplication - TD-16 improvement |
| `crates/meeting-controller/tests/gc_integration.rs` | 267-279 (OnceLock) | No new duplication - TD-16 improvement |

---

## Verdict Rationale

**APPROVED** because:

1. **No BLOCKER findings**:
   - Master secret loading is first occurrence (TD-18)
   - OnceLock pattern is standard Rust idiom, not problematic duplication
   - No code exists in `common` that should have been used

2. **TECH_DEBT appropriately documented**:
   - TD-16 updated with OnceLock improvement
   - TD-18 added for monitoring when GC/MH might need similar pattern

3. **Iteration 2 changes are improvements**:
   - OnceLock pattern fixes potential memory leak
   - Master secret loading is appropriately placed (service-specific startup)

---

## Tech Debt Registry (Updated)

| ID | Pattern | Locations | Follow-up Action | Timeline |
|----|---------|-----------|------------------|----------|
| TD-16 | Mock TokenReceiver Helper | mc/grpc/gc_client.rs (tests), mc/tests/gc_integration.rs | Consider `common::token_manager::test_helpers` | When GC integrates TokenManager |
| TD-17 | OAuth Config Fields | mc/config.rs | Evaluate extraction if GC config >90% similar | When GC integrates TokenManager |
| TD-18 | Master Secret Loading | mc/main.rs:145-170 | Monitor for duplication in GC/MH | When GC/MH need secret loading |
