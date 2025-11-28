//! Builder patterns for test data construction
//!
//! Provides fluent APIs for creating test tokens and requests.

use chrono::{Duration, Utc};
use serde_json::json;

/// Builder for creating test JWT claims
///
/// # Example
/// ```rust,ignore
/// let token = TestTokenBuilder::new()
///     .for_user("alice")
///     .with_scope("meeting:create meeting:read")
///     .expires_in(3600)
///     .build();
/// ```
pub struct TestTokenBuilder {
    sub: String,
    scope: String,
    exp: i64,
    iat: i64,
}

impl TestTokenBuilder {
    /// Create a new token builder with defaults
    pub fn new() -> Self {
        let now = Utc::now();
        Self {
            sub: "test-subject".to_string(),
            scope: "".to_string(),
            exp: (now + Duration::seconds(3600)).timestamp(),
            iat: now.timestamp(),
        }
    }

    /// Set the subject (user/service)
    pub fn for_user(mut self, subject: &str) -> Self {
        self.sub = subject.to_string();
        self
    }

    /// Set the scope (space-separated)
    pub fn with_scope(mut self, scope: &str) -> Self {
        self.scope = scope.to_string();
        self
    }

    /// Set expiration in seconds from now
    pub fn expires_in(mut self, seconds: i64) -> Self {
        self.exp = (Utc::now() + Duration::seconds(seconds)).timestamp();
        self
    }

    /// Set issued-at timestamp
    pub fn issued_at(mut self, timestamp: i64) -> Self {
        self.iat = timestamp;
        self
    }

    /// Build the claims as a JSON value
    pub fn build(self) -> serde_json::Value {
        json!({
            "sub": self.sub,
            "scope": self.scope,
            "exp": self.exp,
            "iat": self.iat,
        })
    }
}

impl Default for TestTokenBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_creates_valid_claims() {
        let claims = TestTokenBuilder::new()
            .for_user("alice")
            .with_scope("meeting:create")
            .build();

        assert_eq!(claims["sub"], "alice");
        assert_eq!(claims["scope"], "meeting:create");
        assert!(claims["exp"].as_i64().unwrap() > 0);
    }

    #[test]
    fn test_builder_default() {
        let claims = TestTokenBuilder::default().build();
        assert_eq!(claims["sub"], "test-subject");
    }
}
