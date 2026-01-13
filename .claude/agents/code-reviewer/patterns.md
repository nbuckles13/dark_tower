# Code Reviewer - Patterns

Reusable code quality patterns observed in Dark Tower codebase.

---

## Pattern: Configuration Value Pattern (Constants + Field + Parsing + Tests)
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/config.rs`

When adding configurable security parameters (e.g., bcrypt cost, JWT clock skew), follow the four-part pattern: (1) define constants with DEFAULT/MIN/MAX bounds, (2) add config struct field with serde defaults, (3) implement parsing with range validation, (4) add comprehensive tests. This ensures consistency and makes security boundaries explicit.

---

## Pattern: Defense-in-Depth Validation
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/config.rs`, `crates/ac-service/src/crypto.rs`

Validate security-critical values at multiple layers: config parsing time AND at point of use. Even if config validation ensures valid ranges, crypto functions should independently verify inputs. Prevents bugs if validation is bypassed or config is constructed programmatically.

---

## Pattern: OWASP/NIST Reference Documentation
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/config.rs`

Document security-critical constants with references to authoritative sources (OWASP, NIST). Example: bcrypt cost factor 12 references OWASP password storage cheat sheet. This provides audit trail and justification for security decisions.

---

## Pattern: No Panic Production Code (ADR-0002)
**Added**: 2026-01-11
**Related files**: `docs/decisions/adr-0002-no-panic-policy.md`

All production code uses `Result<T, E>` for fallible operations. The `.unwrap()`, `.expect()`, and `panic!()` are only allowed in: test code, truly unreachable invariants with proof comments, and development tools. Grep for these patterns during review.

---

## Pattern: SecretBox Custom Debug Implementation
**Added**: 2026-01-12
**Related files**: `crates/ac-service/src/config.rs`, `crates/ac-service/src/crypto/mod.rs`

When a struct contains `SecretBox<T>` fields, implement custom `Debug` using `f.debug_struct()` with `&"[REDACTED]"` for sensitive fields. This is idiomatic Rust and prevents accidental logging:
```rust
impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("database_url", &"[REDACTED]")
            .field("bind_address", &self.bind_address)
            .field("master_key", &"[REDACTED]")
            .finish()
    }
}
```
Document which fields are redacted and why in the doc comment above the impl.

---

## Pattern: SecretBox Custom Clone Implementation
**Added**: 2026-01-12
**Related files**: `crates/ac-service/src/config.rs`, `crates/ac-service/src/crypto/mod.rs`

`SecretBox<T>` intentionally does not implement `Clone` to prevent accidental secret duplication. When cloning is required, implement manually:
```rust
impl Clone for Config {
    fn clone(&self) -> Self {
        Self {
            master_key: SecretBox::new(Box::new(self.master_key.expose_secret().clone())),
            // ... other fields
        }
    }
}
```
Document why Clone is needed in the struct doc comment (e.g., "Clone is manually implemented since SecretBox requires explicit cloning").

---

## Pattern: SecretString Serialize for One-Time Exposure
**Added**: 2026-01-12
**Related files**: `crates/ac-service/src/models/mod.rs`, `crates/ac-service/src/handlers/admin_handler.rs`

For API responses that must expose a secret exactly once (e.g., client_secret at registration), implement custom `Serialize` that calls `.expose_secret()`:
```rust
impl Serialize for CreateClientResponse {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where S: Serializer {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("CreateClientResponse", 3)?;
        state.serialize_field("client_id", &self.client_id)?;
        state.serialize_field("client_secret", self.client_secret.expose_secret())?;
        state.end()
    }
}
```
CRITICAL: Add doc comment stating "This is intentional: the [response type] is the ONLY time the plaintext [secret] is shown to the user."

---

## Pattern: Manual Trait Impl Threshold
**Added**: 2026-01-12
**Related files**: `crates/ac-service/src/handlers/admin_handler.rs`

Manual trait implementations (Debug, Clone, Serialize) are acceptable for up to ~5 similar types. Beyond that, consider a derive macro. Current examples: `RegisterServiceResponse`, `CreateClientResponse`, `RotateSecretResponse` all follow the same pattern - acceptable as 3 types. If pattern proliferates, create `#[derive(SecretSerialize)]` or similar.
