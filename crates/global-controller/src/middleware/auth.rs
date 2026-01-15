//! Authentication middleware for protected routes.
//!
//! Extracts Bearer token from Authorization header, validates JWT using
//! the JWKS client, and injects claims into request extensions.

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

/// Authentication middleware that validates JWT tokens.
///
/// Extracts Bearer token from Authorization header, verifies JWT signature
/// and claims, then stores the claims in request extensions for handlers.
///
/// # Authorization Header Format
///
/// ```text
/// Authorization: Bearer <token>
/// ```
///
/// # Response
///
/// - Returns 401 Unauthorized with WWW-Authenticate header if token is missing or invalid
/// - Continues to next handler with claims in extensions if token is valid
#[instrument(skip(state, req, next), name = "gc.middleware.auth")]
pub async fn require_auth(
    State(state): State<Arc<AuthState>>,
    mut req: Request,
    next: Next,
) -> Result<impl IntoResponse, GcError> {
    // Extract Authorization header
    let auth_header = req
        .headers()
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .ok_or_else(|| {
            tracing::debug!(target: "gc.middleware.auth", "Missing Authorization header");
            GcError::InvalidToken("Missing Authorization header".to_string())
        })?;

    // Extract Bearer token
    let token = auth_header.strip_prefix("Bearer ").ok_or_else(|| {
        tracing::debug!(target: "gc.middleware.auth", "Invalid Authorization header format");
        GcError::InvalidToken("Invalid Authorization header format".to_string())
    })?;

    // Validate JWT
    let claims = state.jwt_validator.validate(token).await?;

    // Store claims in request extensions for downstream handlers
    req.extensions_mut().insert(claims);

    // Continue to next handler
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
