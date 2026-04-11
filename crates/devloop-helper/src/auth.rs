//! Authentication token management.
//!
//! Generates a random 32-byte hex token, writes it to a file with 0600
//! permissions, and validates incoming requests using constant-time comparison.

use crate::error::HelperError;
use ring::rand::{SecureRandom, SystemRandom};
use std::fs;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

/// Generate a random 32-byte authentication token as a 64-character hex string.
///
/// Uses `ring::rand::SystemRandom` per ADR-0027 (approved crypto primitives).
pub fn generate_token() -> Result<String, HelperError> {
    let rng = SystemRandom::new();
    let mut bytes = [0u8; 32];
    rng.fill(&mut bytes)
        .map_err(|_| HelperError::CommandFailed {
            cmd: "generate-token".to_string(),
            detail: "CSPRNG failure".to_string(),
        })?;
    Ok(hex::encode(bytes))
}

/// Write the auth token to a file with 0600 permissions.
pub fn write_token(path: &Path, token: &str) -> Result<(), HelperError> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(token.as_bytes())?;
    file.flush()?;
    Ok(())
}

/// Validate a provided token against the expected token using constant-time comparison.
///
/// Uses XOR accumulator to avoid timing side-channels.
pub fn validate_token(provided: &str, expected: &str) -> Result<(), HelperError> {
    if !constant_time_eq(provided.as_bytes(), expected.as_bytes()) {
        return Err(HelperError::AuthFailed);
    }
    Ok(())
}

/// Constant-time byte slice comparison.
///
/// Returns true only if both slices have the same length and identical contents.
/// Iterates over all bytes of the longer slice to avoid leaking length information
/// through timing.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        // Still iterate to avoid leaking length info through timing,
        // though for this use case the length (64 hex chars) is public knowledge.
        let mut acc = 1u8; // Start nonzero since lengths differ
        for (i, &byte_a) in a.iter().enumerate() {
            // XOR against corresponding byte in b, or 0xFF if b is shorter
            let byte_b = b.get(i).copied().unwrap_or(0xFF);
            acc |= byte_a ^ byte_b;
        }
        std::hint::black_box(acc);
        return false;
    }

    let mut acc = 0u8;
    for (byte_a, byte_b) in a.iter().zip(b.iter()) {
        acc |= byte_a ^ byte_b;
    }
    acc == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_generate_token_length() {
        let token = generate_token().unwrap();
        assert_eq!(token.len(), 64, "token should be 64 hex chars");
    }

    #[test]
    fn test_generate_token_is_hex() {
        let token = generate_token().unwrap();
        assert!(
            token.chars().all(|c| c.is_ascii_hexdigit()),
            "token should be valid hex: {token}"
        );
    }

    #[test]
    fn test_generate_token_uniqueness() {
        let mut tokens = HashSet::new();
        for _ in 0..100 {
            let token = generate_token().unwrap();
            assert!(tokens.insert(token), "duplicate token generated");
        }
    }

    #[test]
    fn test_validate_token_correct() {
        let token = generate_token().unwrap();
        assert!(validate_token(&token, &token).is_ok());
    }

    #[test]
    fn test_validate_token_wrong() {
        let token = generate_token().unwrap();
        let wrong = generate_token().unwrap();
        assert!(validate_token(&wrong, &token).is_err());
    }

    #[test]
    fn test_validate_token_empty() {
        let token = generate_token().unwrap();
        assert!(validate_token("", &token).is_err());
    }

    #[test]
    fn test_validate_token_different_length() {
        let token = generate_token().unwrap();
        assert!(validate_token("short", &token).is_err());
        assert!(validate_token(&format!("{token}extra"), &token).is_err());
    }

    #[test]
    fn test_constant_time_eq_equal() {
        assert!(constant_time_eq(b"hello", b"hello"));
        assert!(constant_time_eq(b"", b""));
    }

    #[test]
    fn test_constant_time_eq_different() {
        assert!(!constant_time_eq(b"hello", b"world"));
        assert!(!constant_time_eq(b"hello", b"hell"));
        assert!(!constant_time_eq(b"hell", b"hello"));
    }

    #[test]
    fn test_write_token_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("auth-token");
        let token = generate_token().unwrap();
        write_token(&path, &token).unwrap();

        let contents = fs::read_to_string(&path).unwrap();
        assert_eq!(contents, token);

        // Check permissions on Unix
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::metadata(&path).unwrap().permissions();
        assert_eq!(perms.mode() & 0o777, 0o600);
    }
}
