//! Custom test assertions for expressive tests
//!
//! Provides trait-based assertions for token validation.

/// Custom assertions for token responses
///
/// # Example
/// ```rust,ignore
/// token
///     .assert_valid_jwt()
///     .assert_has_scope("meeting:create")
///     .assert_signed_by("test-key-2025-01");
/// ```
pub trait TokenAssertions {
    /// Assert that the token is a valid JWT format
    fn assert_valid_jwt(&self) -> &Self;

    /// Assert that the token contains the specified scope
    fn assert_has_scope(&self, scope: &str) -> &Self;

    /// Assert that the token was signed by the specified key
    fn assert_signed_by(&self, key_id: &str) -> &Self;

    /// Assert that the token expires within the specified seconds
    fn assert_expires_in(&self, seconds: u64) -> &Self;

    /// Assert that the token is for the specified subject
    fn assert_for_subject(&self, subject: &str) -> &Self;
}

// Implementation will be added when we have the actual TokenResponse type
// For now, this is a placeholder to establish the API

#[cfg(test)]
mod tests {
    // Tests will be added when implementing assertions for TokenResponse
}
