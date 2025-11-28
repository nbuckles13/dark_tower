use crate::errors::AcError;
use crate::models::Jwks;
use crate::services::key_management_service;
use axum::{extract::State, Json};
use std::sync::Arc;

use super::auth_handler::AppState;

/// Handle JWKS request
///
/// GET /.well-known/jwks.json
///
/// Returns all active public keys in JWKS format (RFC 7517)
pub async fn handle_get_jwks(State(state): State<Arc<AppState>>) -> Result<Json<Jwks>, AcError> {
    let jwks = key_management_service::get_jwks(&state.pool).await?;

    Ok(Json(jwks))
}
