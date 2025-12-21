//! Secret types for protecting sensitive values from accidental logging.
//!
//! This module re-exports types from the [`secrecy`] crate with Dark Tower-specific
//! guidance. Use these types for all sensitive values like passwords, tokens,
//! API keys, and cryptographic material.
//!
//! # Compile-Time Safety
//!
//! The key insight is that `SecretBox<T>` and `SecretString` implement `Debug`
//! with redaction, so any code that derives `Debug` on a struct containing secrets
//! will automatically get safe logging behavior. This makes it **impossible** to
//! accidentally log secrets via `{:?}` or tracing.
//!
//! # Memory Safety
//!
//! Secrets are automatically zeroized when dropped, preventing sensitive
//! data from lingering in memory after use.
//!
//! # Example
//!
//! ```rust
//! use common::secret::SecretString;
//! use secrecy::ExposeSecret;
//!
//! #[derive(Debug)]
//! struct LoginRequest {
//!     username: String,
//!     password: SecretString,  // Safe: Debug shows "[REDACTED]"
//! }
//!
//! let req = LoginRequest {
//!     username: "alice".to_string(),
//!     password: SecretString::from("hunter2"),
//! };
//!
//! // This is safe - password is redacted
//! println!("{:?}", req);
//! // Output: LoginRequest { username: "alice", password: Secret([REDACTED alloc::string::String]) }
//!
//! // To access the actual value, you must explicitly call expose_secret()
//! let password: &str = req.password.expose_secret();
//! ```
//!
//! # Dark Tower Usage Guidelines
//!
//! Use `SecretString` for:
//! - User passwords
//! - OAuth client secrets
//! - API keys
//! - Bearer tokens
//! - Encryption keys (as base64 strings)
//!
//! Use `SecretBox<T>` for:
//! - Custom secret types (e.g., `SecretBox<[u8]>` for binary keys)
//!
//! # Serde Integration
//!
//! With the `serde` feature enabled, secrets can be deserialized from JSON:
//!
//! ```rust
//! use serde::Deserialize;
//! use common::secret::SecretString;
//!
//! #[derive(Debug, Deserialize)]
//! struct ServiceCredentials {
//!     client_id: String,
//!     client_secret: SecretString,
//! }
//!
//! let json = r#"{"client_id": "svc-123", "client_secret": "secret-key"}"#;
//! let creds: ServiceCredentials = serde_json::from_str(json).unwrap();
//!
//! // Debug output is safe
//! println!("{:?}", creds);
//! // client_id is visible, client_secret is redacted
//! ```

// Re-export the main types from secrecy
pub use secrecy::{ExposeSecret, SecretBox, SecretString};

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[test]
    fn test_debug_is_redacted() {
        let secret = SecretString::from("hunter2");
        let debug_str = format!("{secret:?}");

        assert!(debug_str.contains("REDACTED"));
        assert!(!debug_str.contains("hunter2"));
    }

    #[test]
    fn test_expose_secret_returns_inner_value() {
        let secret = SecretString::from("password123");
        assert_eq!(secret.expose_secret(), "password123");
    }

    #[test]
    fn test_struct_with_secret_is_safe() {
        #[allow(dead_code)]
        #[derive(Debug)]
        struct UserCredentials {
            username: String,
            password: SecretString,
        }

        let creds = UserCredentials {
            username: "alice".to_string(),
            password: SecretString::from("super-secret"),
        };

        let debug_str = format!("{creds:?}");

        // Username should be visible
        assert!(debug_str.contains("alice"));
        // Password should be redacted
        assert!(debug_str.contains("REDACTED"));
        assert!(!debug_str.contains("super-secret"));
    }

    #[test]
    fn test_deserialize() {
        #[allow(dead_code)]
        #[derive(Debug, Deserialize)]
        struct Credentials {
            username: String,
            password: SecretString,
        }

        let json = r#"{"username": "bob", "password": "my-secret-value"}"#;
        let creds: Credentials = serde_json::from_str(json).expect("deserialize");

        // Verify we can access the secret
        assert_eq!(creds.password.expose_secret(), "my-secret-value");

        // Verify debug doesn't expose the value
        let debug = format!("{creds:?}");
        assert!(!debug.contains("my-secret-value"));
        assert!(debug.contains("REDACTED"));
    }

    #[test]
    fn test_clone_works() {
        let secret = SecretString::from("cloneable");
        let cloned = secret.clone();
        assert_eq!(cloned.expose_secret(), "cloneable");
    }
}
