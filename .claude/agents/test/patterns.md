# Test Specialist - Patterns

Testing patterns worth documenting for Dark Tower codebase.

---

## Pattern: Config Boundary Testing
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/config.rs`

Test all valid values (min, default, max) plus invalid values on both sides of boundaries. The bcrypt cost tests demonstrate this well: test 10 (min), 11, 12 (default), 13, 14 (max), then test 9 (below min) and 15 (above max). Always include the exact boundary values.

---

## Pattern: Defense-in-Depth Validation Tests
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/crypto/mod.rs`

When a function validates input that config already validated, test that the function still rejects invalid inputs. In `hash_client_secret()`, cost validation exists both in config AND the function. Test both layers independently. This catches bugs if callers bypass config.

---

## Pattern: Cross-Version Verification Tests
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/crypto/mod.rs`

For migration scenarios (bcrypt cost changes, algorithm upgrades), test that old artifacts verify correctly with new code. The `test_hash_verification_works_across_cost_factors` test creates hashes at costs 10-14 and verifies ALL of them work regardless of current config. Essential for zero-downtime deployments.

---

## Pattern: Constant Assertion Tests
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/config.rs`, `crates/ac-service/src/crypto/mod.rs`

Document security-critical constants with dedicated assertion tests. Tests like `test_bcrypt_cost_constants_are_valid()` verify DEFAULT >= MIN and DEFAULT <= MAX. Self-documenting and catch copy-paste errors in constant definitions.

---

## Pattern: Handler Integration with Config Propagation
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/handlers/admin_handler.rs`

When config values flow through handlers to service functions, test the full chain. The `handle_register_service` and `handle_rotate_client_secret` handlers pass `state.config.bcrypt_cost` to crypto functions. Integration tests verify config actually reaches the crypto layer.

---

## Pattern: Hash Format Verification
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/crypto/mod.rs`

For bcrypt/argon2/etc, verify hash structure matches expected format. Parse the hash string (e.g., `$2b$12$...`) and assert version and cost separately. This catches silent algorithm downgrades.

---

## Pattern: Error Message Content Tests
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/config.rs`

When testing invalid inputs, verify error messages contain useful context. Tests like `test_bcrypt_cost_rejects_too_low` check that the error message mentions the valid range (10-14). Helps users self-diagnose config issues.

---

## Pattern: SecretBox Debug Redaction Tests
**Added**: 2026-01-12
**Related files**: `crates/ac-service/src/crypto/mod.rs`, `crates/ac-service/src/models/mod.rs`, `crates/ac-service/src/config.rs`

When struct contains `SecretBox<T>` or `SecretString`, test the Debug impl:
```rust
#[test]
fn test_struct_debug_redacts_secret() {
    let s = MyStruct { secret: SecretString::from("hunter2"), public: "visible" };
    let debug = format!("{:?}", s);
    assert!(debug.contains("[REDACTED]"), "Secret should be redacted");
    assert!(!debug.contains("hunter2"), "Actual value must not appear");
    assert!(debug.contains("visible"), "Public fields should appear");
}
```
This prevents accidental credential leaks in logs.

---

## Pattern: SecretBox Value Access Tests
**Added**: 2026-01-12
**Related files**: `crates/ac-service/src/crypto/mod.rs`

When testing functions that return `SecretString` or use `SecretBox`, always access via `.expose_secret()`:
```rust
#[test]
fn test_generate_secret() {
    let secret = generate_client_secret().unwrap();
    // WRONG: secret.as_str() - won't compile
    // RIGHT: explicit exposure
    assert!(!secret.expose_secret().is_empty());
}
```
The compiler enforces this, making accidental exposure impossible.

---

## Pattern: Custom Clone Tests for SecretBox Structs
**Added**: 2026-01-12
**Related files**: `crates/ac-service/src/crypto/mod.rs`

Structs with `SecretBox` fields require manual Clone impl. Test that Clone produces correct deep copies:
```rust
#[test]
fn test_encrypted_key_clone() {
    let original = EncryptedKey {
        encrypted_data: SecretBox::new(Box::new(vec![1,2,3])),
        nonce: vec![4,5,6],
        tag: vec![7,8,9],
    };
    let cloned = original.clone();
    assert_eq!(cloned.encrypted_data.expose_secret(), original.encrypted_data.expose_secret());
    assert_eq!(cloned.nonce, original.nonce);
}
```

---

## Pattern: Wrapper Type Refactor Verification
**Added**: 2026-01-12
**Related files**: Integration test files

When refactoring raw types to wrapper types (e.g., `Vec<u8>` to `SecretBox<Vec<u8>>`):
1. Search all usages of the struct being modified
2. Update construction sites to wrap values: `SecretBox::new(Box::new(value))`
3. Update access sites to unwrap: `.expose_secret()`
4. **Verify test files are included in mod.rs** - orphaned tests won't catch type errors
5. Run `cargo test` and verify expected test count executes
