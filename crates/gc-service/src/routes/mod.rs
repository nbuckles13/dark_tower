//! HTTP routes for Global Controller.
//!
//! Defines the Axum router and application state.

use crate::auth::{JwksClient, JwtValidator};
use crate::config::Config;
use crate::handlers::{self, TelemetryState};
use crate::middleware::{
    extract_trace_context_middleware, http_metrics_middleware, require_auth, require_user_auth,
    AuthState,
};
use crate::services::mc_client::McClientTrait;
use axum::{
    extract::DefaultBodyLimit,
    middleware,
    routing::{get, patch, post},
    Router,
};
use common::token_manager::TokenReceiver;
use metrics_exporter_prometheus::PrometheusHandle;
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use tower_http::{timeout::TimeoutLayer, trace::TraceLayer};

/// Application state shared across all handlers.
#[derive(Clone)]
pub struct AppState {
    /// Database connection pool.
    pub pool: PgPool,

    /// Service configuration.
    pub config: Config,

    /// MC client for GC->MC communication.
    pub mc_client: Arc<dyn McClientTrait>,

    /// Token receiver for dynamically refreshed OAuth tokens from TokenManager.
    pub token_receiver: TokenReceiver,

    /// Telemetry proxy state (per-user rate limiter + collector forwarder).
    pub telemetry: TelemetryState,
}

/// Build the application routes.
///
/// Creates an Axum router with:
/// - `/health` - Liveness probe (simple "OK") - public, unversioned
/// - `/ready` - Readiness probe (checks DB + AC JWKS) - public, unversioned
/// - `/metrics` - Prometheus metrics endpoint (ADR-0011) - public, unversioned
/// - `/api/v1/me` - Current user endpoint - requires service authentication
/// - `/api/v1/meetings` - Create meeting (user authenticated)
/// - `/api/v1/meetings/{code}` - Join meeting (user authenticated)
/// - `/api/v1/meetings/{code}/guest-token` - Get guest token (public)
/// - `/api/v1/meetings/{id}/settings` - Update meeting settings (user authenticated, host only)
/// - TraceLayer for request logging
/// - HTTP metrics middleware (ADR-0011)
/// - 30 second request timeout
pub fn build_routes(
    state: Arc<AppState>,
    metrics_handle: PrometheusHandle,
) -> Result<Router, common::jwt::JwtError> {
    // Create JWKS client and JWT validator
    let jwks_client = Arc::new(JwksClient::new(state.config.ac_jwks_url.clone())?);
    let jwt_validator = Arc::new(JwtValidator::new(
        jwks_client,
        state.config.jwt_clock_skew_seconds,
    ));
    let auth_state = Arc::new(AuthState { jwt_validator });

    // Public routes (no authentication required)
    let public_routes = Router::new()
        // Health check endpoints (unversioned operational endpoints)
        .route("/health", get(handlers::health_check))
        .route("/ready", get(handlers::readiness_check))
        // Guest token endpoint (public, rate limited)
        .route(
            "/api/v1/meetings/:code/guest-token",
            post(handlers::get_guest_token),
        )
        .with_state(state.clone());

    // Metrics route with its own state (ADR-0011)
    let metrics_routes = Router::new()
        .route("/metrics", get(handlers::metrics_handler))
        .with_state(metrics_handle);

    // User-authenticated routes (require user JWT with UserClaims)
    let user_auth_routes = Router::new()
        // Meeting creation endpoint
        .route("/api/v1/meetings", post(handlers::create_meeting))
        // Meeting join endpoint
        .route("/api/v1/meetings/:code", get(handlers::join_meeting))
        // Meeting settings endpoint
        .route(
            "/api/v1/meetings/:id/settings",
            patch(handlers::update_meeting_settings),
        )
        .route_layer(middleware::from_fn_with_state(
            auth_state.clone(),
            require_user_auth,
        ))
        .with_state(state.clone());

    // Telemetry proxy routes (R-2). User-authenticated. Two-tier real-bytes size
    // cap (Option 1, security-reviewed): the HANDLER's explicit `body.len() >
    // max_bytes` check is the load-bearing user-facing 413 at the EXACT configured
    // limit (and emits `rejected_size`); this `DefaultBodyLimit` layer is a higher
    // hard-ceiling (`2× max_bytes`) DoS backstop that rejects a pathological body
    // before unbounded buffering. Both are real-bytes checks; neither trusts
    // Content-Length.
    //
    // SECURITY INVARIANT (do not break): this layer is ABOVE the decode cap, so
    // the handler's exact-`max_bytes` check is the sole PRE-DECODE oversize gate
    // for bodies in (max_bytes, 2×max_bytes]. Do NOT raise this ceiling further,
    // and do NOT remove the handler check, without re-adding a pre-decode
    // body-length gate at `max_bytes` — otherwise prost::decode runs on oversize
    // bytes. See `handlers::telemetry::ingest` step 1.
    //
    // The path carries two `v1` segments: the outer `/api/v1` is GC's API
    // version (ADR-0004); the inner `/v1/metrics`|`/v1/traces` is the FIXED
    // OTLP/HTTP spec path (vendored, not our versioning) — compliant, not a
    // double-version smell. `/v1/logs` is intentionally NOT routed this story.
    let telemetry_routes = Router::new()
        .route(
            "/api/v1/telemetry/v1/metrics",
            post(handlers::ingest_metrics),
        )
        .route("/api/v1/telemetry/v1/traces", post(handlers::ingest_traces))
        .layer(DefaultBodyLimit::max(handlers::body_limit_ceiling(
            state.config.telemetry_proxy_max_bytes,
        )))
        .route_layer(middleware::from_fn_with_state(
            auth_state.clone(),
            require_user_auth,
        ))
        .with_state(state.telemetry.clone());

    // Service-authenticated routes (require service JWT with Claims)
    let protected_routes = Router::new()
        // Current user endpoint
        .route("/api/v1/me", get(handlers::get_me))
        .route_layer(middleware::from_fn_with_state(
            auth_state.clone(),
            require_auth,
        ))
        .with_state(state);

    // Merge routes and apply global middleware layers.
    //
    // `axum::Router::layer` semantics: the *last*-added `.layer()` call is
    // *outermost* and runs first on the request path (for
    // `.layer(one).layer(two).layer(three)`, execution is
    // `three → two → one → handler`). So in ADDED order below, execution
    // (bottom-to-top of this list) is:
    // 1. extract_trace_context_middleware - innermost, runs LAST before the
    //    handler (see below for why this placement is required)
    // 2. TraceLayer - creates + enters the request span
    // 3. TimeoutLayer - times out the request
    // 4. http_metrics_middleware - outermost, runs FIRST; records ALL
    //    responses including framework-level errors like 415, 400, 404, 405
    //
    // (Corrected 2026-07-03, R-56 GC OTel wiring devloop: this comment
    // previously had TimeoutLayer/TraceLayer's relative innermost/outer
    // position backwards — TraceLayer, added first, is more inner than
    // TimeoutLayer, added second, not the reverse.)
    //
    // `extract_trace_context_middleware` is placed as the new FIRST `.layer()`
    // call — physically ABOVE `TraceLayer::new_for_http()` — so it is MORE
    // INNER than `TraceLayer` and therefore executes AFTER `TraceLayer` has
    // created and entered its request span, but BEFORE the handler. At that
    // point `tracing::Span::current()` resolves to `TraceLayer`'s span, so
    // the middleware's `.set_parent()` call (via
    // `common::observability::otel_http::extract_trace_context`) correctly
    // attaches the extracted W3C parent to it — every span the handler
    // creates afterward (including the MC client's `#[instrument]` span)
    // inherits it via the tracing hierarchy. See `middleware/otel.rs`'s
    // module doc for the full reasoning; @observability + @code-reviewer
    // confirmed this placement at Gate 1. NOTE: GC's inbound gRPC server
    // (`main.rs`'s `TonicServer::builder()`) composes layers via the
    // OPPOSITE convention (`tower::ServiceBuilder`-style, first-added =
    // outermost) — don't flip this ordering by false analogy to that one.
    Ok(public_routes
        .merge(metrics_routes)
        .merge(user_auth_routes)
        .merge(telemetry_routes)
        .merge(protected_routes)
        .layer(middleware::from_fn(extract_trace_context_middleware))
        .layer(TraceLayer::new_for_http())
        .layer(TimeoutLayer::new(Duration::from_secs(30)))
        // HTTP metrics layer (outermost) - captures ALL responses including
        // framework-level errors like 415, 400, 404, 405
        .layer(middleware::from_fn(http_metrics_middleware)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_state_is_clone() {
        // This test verifies that AppState implements Clone,
        // which is required for Axum's State extractor.
        fn assert_clone<T: Clone>() {}
        assert_clone::<AppState>();
    }

    #[test]
    fn test_config_is_clone() {
        fn assert_clone<T: Clone>() {}
        assert_clone::<Config>();
    }
}
