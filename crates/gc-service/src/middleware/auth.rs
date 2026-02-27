//! Authentication middleware for protected routes.
//!
//! Provides two middleware functions:
//! - `require_auth` - For service-to-service authentication (validates `Claims`)
//! - `require_user_auth` - For user authentication (validates `UserClaims`)
//!
//! Both extract Bearer token from Authorization header, validate JWT using
//! the JWKS client, and inject the appropriate claims into request extensions.

use crate::auth::{Claims, JwtValidator};
use crate::errors::GcError;
use axum::{
    extract::{Request, State},
    middleware::Next,
    response::IntoResponse,
};
use std::sync::Arc;
use tracing::instrument;

/// State for the authentication middleware.
#[derive(Clone)]
pub struct AuthState {
    /// JWT validator with JWKS client.
    pub jwt_validator: Arc<JwtValidator>,
}

/// Extract Bearer token from the Authorization header.
///
/// Shared helper used by both `require_auth` and `require_user_auth`.
fn extract_bearer_token(req: &Request) -> Result<&str, GcError> {
    let auth_header = req
        .headers()
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .ok_or_else(|| {
            tracing::debug!(target: "gc.middleware.auth", "Missing Authorization header");
            GcError::InvalidToken("Missing Authorization header".to_string())
        })?;

    auth_header.strip_prefix("Bearer ").ok_or_else(|| {
        tracing::debug!(target: "gc.middleware.auth", "Invalid Authorization header format");
        GcError::InvalidToken("Invalid Authorization header format".to_string())
    })
}

/// Authentication middleware for service tokens.
///
/// Validates JWT and deserializes into `Claims` (with `scope`, `service_type`).
/// Used for service-to-service authenticated endpoints.
///
/// # Response
///
/// - Returns 401 Unauthorized if token is missing or invalid
/// - Continues to next handler with `Claims` in extensions if token is valid
#[instrument(skip_all, name = "gc.middleware.auth")]
pub async fn require_auth(
    State(state): State<Arc<AuthState>>,
    mut req: Request,
    next: Next,
) -> Result<impl IntoResponse, GcError> {
    let token = extract_bearer_token(&req)?;

    // Validate JWT as service token
    let claims = state.jwt_validator.validate(token).await?;

    // Store claims in request extensions for downstream handlers
    req.extensions_mut().insert(claims);

    Ok(next.run(req).await)
}

/// Authentication middleware for user tokens.
///
/// Validates JWT and deserializes into `UserClaims` (with `org_id`, `roles`, `email`, `jti`).
/// Used for user-facing authenticated endpoints.
///
/// # Response
///
/// - Returns 401 Unauthorized if token is missing or invalid
/// - Continues to next handler with `UserClaims` in extensions if token is valid
#[instrument(skip_all, name = "gc.middleware.user_auth")]
pub async fn require_user_auth(
    State(state): State<Arc<AuthState>>,
    mut req: Request,
    next: Next,
) -> Result<impl IntoResponse, GcError> {
    let token = extract_bearer_token(&req)?;

    // Validate JWT as user token
    let user_claims = state.jwt_validator.validate_user(token).await?;

    // Store user claims in request extensions for downstream handlers
    req.extensions_mut().insert(user_claims);

    Ok(next.run(req).await)
}

/// Extension trait for extracting claims from request.
///
/// Provides a convenient method for handlers to get the authenticated claims.
#[allow(dead_code)] // API for handlers that need claims from request
pub trait ClaimsExt {
    /// Get the authenticated claims from request extensions.
    ///
    /// Returns `None` if auth middleware was not applied to this request.
    fn claims(&self) -> Option<&Claims>;
}

#[allow(dead_code)] // Implementation for ClaimsExt trait
impl<B> ClaimsExt for axum::extract::Request<B> {
    fn claims(&self) -> Option<&Claims> {
        self.extensions().get::<Claims>()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    // Note: Full middleware tests require mocking JWKS endpoint
    // which is done in integration tests. Unit tests here focus on
    // helper functions and types.

    use super::*;

    #[test]
    fn test_auth_state_is_clone() {
        fn assert_clone<T: Clone>() {}
        assert_clone::<AuthState>();
    }
}
