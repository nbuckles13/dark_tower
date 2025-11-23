mod auth;
mod admin;
mod jwks;

use axum::{
    Router,
    routing::{get, post},
};
use tower_http::trace::TraceLayer;

pub fn build_routes() -> Router {
    Router::new()
        // OAuth 2.0 authentication endpoints
        .route("/api/v1/auth/user/token", post(auth::user_token))
        .route("/api/v1/auth/service/token", post(auth::service_token))

        // Admin endpoints
        .route("/api/v1/admin/services/register", post(admin::register_service))

        // JWKS endpoint (RFC 8414 well-known path, no /api/v1 prefix)
        .route("/.well-known/jwks.json", get(jwks::get_jwks))

        // Health check
        .route("/health", get(health_check))

        // Add tracing middleware
        .layer(TraceLayer::new_for_http())
}

async fn health_check() -> &'static str {
    "OK"
}
