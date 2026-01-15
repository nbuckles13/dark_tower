//! JWT claims structure.
//!
//! Contains the claims extracted from validated JWTs. The `sub` field is
//! redacted in Debug output to prevent exposure in logs.

use serde::{Deserialize, Serialize};
use std::fmt;

/// JWT Claims structure for validated tokens.
///
/// The `sub` field contains user or client identifiers which should not
/// be exposed in logs. A custom Debug implementation redacts this field.
#[derive(Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Subject (user_id or client_id) - redacted in Debug output.
    pub sub: String,

    /// Expiration timestamp (Unix epoch seconds).
    pub exp: i64,

    /// Issued-at timestamp (Unix epoch seconds).
    pub iat: i64,

    /// Space-separated scopes granted to this token.
    pub scope: String,

    /// Optional service type for service-to-service tokens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_type: Option<String>,
}

/// Custom Debug implementation that redacts the `sub` field.
///
/// The `sub` field contains user/client identifiers which are sensitive
/// and should not be exposed in logs or debug output.
impl fmt::Debug for Claims {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Claims")
            .field("sub", &"[REDACTED]")
            .field("exp", &self.exp)
            .field("iat", &self.iat)
            .field("scope", &self.scope)
            .field("service_type", &self.service_type)
            .finish()
    }
}

impl Claims {
    /// Check if the token has a specific scope.
    ///
    /// Scopes are space-separated in the JWT claims.
    #[allow(dead_code)] // Will be used in Phase 3 for scope checking
    pub fn has_scope(&self, scope: &str) -> bool {
        self.scope.split_whitespace().any(|s| s == scope)
    }

    /// Get all scopes as a vector.
    pub fn scopes(&self) -> Vec<&str> {
        self.scope.split_whitespace().collect()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_claims_debug_redacts_sub() {
        let claims = Claims {
            sub: "secret-user-id".to_string(),
            exp: 1234567890,
            iat: 1234567800,
            scope: "read write".to_string(),
            service_type: None,
        };

        let debug_str = format!("{:?}", claims);

        assert!(
            !debug_str.contains("secret-user-id"),
            "Debug output should not contain actual sub value"
        );
        assert!(
            debug_str.contains("[REDACTED]"),
            "Debug output should contain [REDACTED]"
        );
    }

    #[test]
    fn test_claims_has_scope() {
        let claims = Claims {
            sub: "user".to_string(),
            exp: 1234567890,
            iat: 1234567800,
            scope: "read write admin".to_string(),
            service_type: None,
        };

        assert!(claims.has_scope("read"));
        assert!(claims.has_scope("write"));
        assert!(claims.has_scope("admin"));
        assert!(!claims.has_scope("delete"));
        assert!(!claims.has_scope("rea")); // Partial match should not work
    }

    #[test]
    fn test_claims_scopes() {
        let claims = Claims {
            sub: "user".to_string(),
            exp: 1234567890,
            iat: 1234567800,
            scope: "read write admin".to_string(),
            service_type: None,
        };

        let scopes = claims.scopes();
        assert_eq!(scopes, vec!["read", "write", "admin"]);
    }

    #[test]
    fn test_claims_empty_scope() {
        let claims = Claims {
            sub: "user".to_string(),
            exp: 1234567890,
            iat: 1234567800,
            scope: "".to_string(),
            service_type: None,
        };

        assert!(!claims.has_scope("read"));
        assert!(claims.scopes().is_empty());
    }

    #[test]
    fn test_claims_serialization() {
        let claims = Claims {
            sub: "user123".to_string(),
            exp: 1234567890,
            iat: 1234567800,
            scope: "read write".to_string(),
            service_type: Some("global-controller".to_string()),
        };

        let json = serde_json::to_string(&claims).unwrap();
        let deserialized: Claims = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.sub, claims.sub);
        assert_eq!(deserialized.exp, claims.exp);
        assert_eq!(deserialized.iat, claims.iat);
        assert_eq!(deserialized.scope, claims.scope);
        assert_eq!(deserialized.service_type, claims.service_type);
    }

    #[test]
    fn test_claims_without_service_type_omits_field() {
        let claims = Claims {
            sub: "user".to_string(),
            exp: 1234567890,
            iat: 1234567800,
            scope: "read".to_string(),
            service_type: None,
        };

        let json = serde_json::to_string(&claims).unwrap();
        assert!(
            !json.contains("service_type"),
            "service_type should be omitted when None"
        );
    }
}
