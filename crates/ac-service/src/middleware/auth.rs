use crate::crypto;
use crate::errors::AcError;
use crate::repositories::signing_keys;
use axum::{
    extract::{Request, State},
    middleware::Next,
    response::IntoResponse,
};
use sqlx::PgPool;
use std::sync::Arc;

/// Middleware state containing database pool
#[derive(Clone)]
pub struct AuthMiddlewareState {
    pub pool: PgPool,
}

/// Authentication middleware that validates JWT tokens
///
/// Extracts Bearer token from Authorization header, verifies JWT signature,
/// and checks for required scopes.
pub async fn require_admin_scope(
    State(state): State<Arc<AuthMiddlewareState>>,
    mut req: Request,
    next: Next,
) -> Result<impl IntoResponse, AcError> {
    // Extract Authorization header
    let auth_header = req
        .headers()
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .ok_or(AcError::InvalidToken(
            "Missing Authorization header".to_string(),
        ))?;

    // Extract Bearer token
    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or(AcError::InvalidToken(
            "Invalid Authorization header format".to_string(),
        ))?;

    // Get active signing key for verification
    let signing_key = signing_keys::get_active_key(&state.pool)
        .await?
        .ok_or_else(|| AcError::Crypto("No active signing key available".to_string()))?;

    // Verify JWT
    let claims = crypto::verify_jwt(token, &signing_key.public_key)?;

    // Check if token has required scope (admin:services)
    let required_scope = "admin:services";
    let token_scopes: Vec<&str> = claims.scope.split_whitespace().collect();

    if !token_scopes.contains(&required_scope) {
        return Err(AcError::InsufficientScope {
            required: required_scope.to_string(),
            provided: token_scopes.iter().map(|s| s.to_string()).collect(),
        });
    }

    // Store claims in request extensions for downstream handlers
    req.extensions_mut().insert(claims);

    // Continue to next handler
    Ok(next.run(req).await)
}
