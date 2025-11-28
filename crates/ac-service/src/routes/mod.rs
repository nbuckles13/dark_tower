use crate::handlers::{admin_handler, auth_handler, jwks_handler};
use crate::middleware::auth::{require_admin_scope, AuthMiddlewareState};
use axum::{
    middleware,
    routing::{get, post},
    Router,
};
use std::sync::Arc;
use tower_http::trace::TraceLayer;

pub fn build_routes(state: Arc<auth_handler::AppState>) -> Router {
    // Create auth middleware state
    let auth_state = Arc::new(AuthMiddlewareState {
        pool: state.pool.clone(),
    });

    // Admin routes that require authentication
    let admin_routes = Router::new()
        .route(
            "/api/v1/admin/services/register",
            post(admin_handler::handle_register_service),
        )
        .layer(middleware::from_fn_with_state(
            auth_state.clone(),
            require_admin_scope,
        ))
        .with_state(state.clone());

    // Public routes (no authentication required)
    let public_routes = Router::new()
        // OAuth 2.0 authentication endpoints
        .route(
            "/api/v1/auth/user/token",
            post(auth_handler::handle_user_token),
        )
        .route(
            "/api/v1/auth/service/token",
            post(auth_handler::handle_service_token),
        )
        // JWKS endpoint (RFC 8414 well-known path, no /api/v1 prefix)
        .route("/.well-known/jwks.json", get(jwks_handler::handle_get_jwks))
        // Health check
        .route("/health", get(health_check))
        .with_state(state);

    // Merge routes
    admin_routes
        .merge(public_routes)
        .layer(TraceLayer::new_for_http())
}

async fn health_check() -> &'static str {
    "OK"
}
