//! CORS preflight observer middleware (R-1 / R-52).
//!
//! Sits immediately OUTSIDE (added after / more-outer than) the router's
//! [`tower_http::cors::CorsLayer`] and turns a browser-blocked (denied) preflight
//! into an explicit, observable `403` while emitting the
//! `gc_cors_preflight_total{origin_class, status}` metric.
//!
//! # Why an observer instead of re-implementing origin matching
//!
//! `CorsLayer` is the single source of truth for the allowlist decision. For a
//! preflight (`OPTIONS` + `Access-Control-Request-Method`) it short-circuits with
//! `200`, attaching `Access-Control-Allow-Origin` (ACAO) iff the `Origin` is
//! allowed; a denied origin yields `200` with NO ACAO (the browser then blocks
//! silently). This observer classifies purely off that ACAO output — it never
//! re-parses or re-matches origins, so the allowlist has exactly one owner.
//!
//! # Security conditions (locked at planning; @security-owned)
//!
//! 1. The genuine-preflight predicate (`OPTIONS` + `Origin` +
//!    `Access-Control-Request-Method`) is the observer's FIRST action, read from
//!    the REQUEST before `next` consumes it. A non-preflight request (health
//!    check, no-Origin GET/OPTIONS, server-to-server) passes through completely
//!    untouched — never rewritten, never recorded. Without this gate the
//!    "ACAO-absent ⇒ deny" heuristic would 403 all non-CORS traffic.
//! 2. The denied `403` is a FRESH response with a generic empty body and NO CORS
//!    headers — no `Access-Control-Allow-Origin`, no
//!    `Access-Control-Allow-Credentials`, no echoed `Origin`.
//! 3. The observer only ever DOWNGRADES a `200` → `403` (guarded on
//!    `status == 200`); it never upgrades and never manufactures an ACAO, so it
//!    cannot flip a deny into an allow. Per the Fetch spec a denied origin
//!    already learns its status from the missing ACAO today, and both
//!    200-no-ACAO and 403 block identically — the rewrite only moves in the
//!    restrictive direction.
//!
//! The denied-preflight triage fields (`requested_origin`, `requested_method`,
//! `requested_headers`) are emitted as STRUCTURED `tracing` fields on the warn
//! log — never string-interpolated into the message, never metric labels (the
//! metric's `origin_class` is the bucketed `allowed`/`denied`, never the raw
//! Origin, so cardinality stays 2×2).
//!
//! # Placement
//!
//! `routes/mod.rs::build_routes` adds this as a global `.layer()` call
//! immediately AFTER `CorsLayer` (so it is more OUTER and sees CorsLayer's
//! response) and INSIDE `TraceLayer`/`TimeoutLayer`/`http_metrics`. Because
//! `CorsLayer` is a global layer on the merged router it short-circuits a
//! preflight before any route-level auth (`require_user_auth`/`require_auth`),
//! so preflight bypasses auth structurally (R-1).

use axum::{
    extract::Request,
    http::{header, Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::observability::metrics::record_cors_preflight;

/// Observe CorsLayer's preflight decision: record the metric and rewrite a
/// denied preflight to a real `403`. See the module doc for the security
/// conditions this upholds.
pub async fn cors_preflight_observer(request: Request, next: Next) -> Response {
    // SECURITY CONDITION 1 — genuine-preflight predicate, FIRST action, read
    // only from the REQUEST (before `next` consumes it).
    let headers = request.headers();
    let is_preflight = request.method() == Method::OPTIONS
        && headers.contains_key(header::ORIGIN)
        && headers.contains_key(header::ACCESS_CONTROL_REQUEST_METHOD);

    // Capture the denied-warn triage fields NOW (owned copies), before the
    // request is moved into `next.run`. These are used ONLY as structured
    // tracing fields on the denied path.
    let owned = |name: &header::HeaderName| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string)
    };
    let requested_origin = owned(&header::ORIGIN);
    let requested_method = owned(&header::ACCESS_CONTROL_REQUEST_METHOD);
    let requested_headers = owned(&header::ACCESS_CONTROL_REQUEST_HEADERS);

    let response = next.run(request).await;

    // Non-preflight requests are never touched or recorded.
    if !is_preflight {
        return response;
    }

    // Allowed origin: CorsLayer attached ACAO. Pass the 200 through untouched.
    if response
        .headers()
        .contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN)
    {
        record_cors_preflight("allowed", 200);
        return response;
    }

    // Denied origin (no ACAO). SECURITY CONDITION 3: downgrade-only, guarded on
    // `status == 200`. CorsLayer ALWAYS 200-short-circuits a genuine preflight
    // (OPTIONS + Access-Control-Request-Method), so a genuine preflight with no
    // ACAO and `status != 200` is UNREACHABLE — if it ever occurred we record
    // nothing and leave the response as-is rather than manufacture a downgrade.
    if response.status() == StatusCode::OK {
        record_cors_preflight("denied", 403);
        tracing::warn!(
            origin_class = "denied",
            requested_method = ?requested_method,
            requested_headers = ?requested_headers,
            requested_origin = ?requested_origin,
            "CORS preflight denied"
        );
        // SECURITY CONDITION 2: fresh 403, generic empty body, NO CORS headers.
        // Building a new response (not mutating CorsLayer's) guarantees no ACAO
        // / ACAC / echoed Origin survives.
        return StatusCode::FORBIDDEN.into_response();
    }

    response
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request as HttpRequest, middleware, routing::get, Router};
    use std::time::Duration;
    use tower::ServiceExt;
    use tower_http::cors::CorsLayer;

    async fn handler() -> &'static str {
        "OK"
    }

    /// Build a minimal router that mirrors production's observer-OUTSIDE-CorsLayer
    /// stacking over a bare CorsLayer allowing only `http://localhost:5173`.
    /// This isolates the observer's own logic from the full route/auth wiring
    /// (which `tests/cors_integration.rs` exercises end-to-end).
    fn test_app() -> Router {
        let cors = CorsLayer::new()
            .allow_origin(["http://localhost:5173".parse().unwrap()])
            .allow_methods([Method::GET, Method::POST, Method::PATCH, Method::OPTIONS])
            .allow_headers([
                header::AUTHORIZATION,
                header::CONTENT_TYPE,
                header::HeaderName::from_static("traceparent"),
                header::HeaderName::from_static("tracestate"),
            ])
            .allow_credentials(false)
            .max_age(Duration::from_secs(600));

        Router::new()
            .route("/api/v1/thing", get(handler))
            // Observer added AFTER CorsLayer → more outer → sees its response.
            .layer(cors)
            .layer(middleware::from_fn(cors_preflight_observer))
    }

    #[tokio::test]
    async fn allowed_preflight_passes_through_200_with_acao() {
        let app = test_app();
        let response = app
            .oneshot(
                HttpRequest::builder()
                    .method(Method::OPTIONS)
                    .uri("/api/v1/thing")
                    .header(header::ORIGIN, "http://localhost:5173")
                    .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .and_then(|v| v.to_str().ok()),
            Some("http://localhost:5173")
        );
    }

    #[tokio::test]
    async fn denied_preflight_rewritten_to_403_stripped_and_bodyless() {
        let app = test_app();
        let response = app
            .oneshot(
                HttpRequest::builder()
                    .method(Method::OPTIONS)
                    .uri("/api/v1/thing")
                    .header(header::ORIGIN, "http://evil.test")
                    .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        // SECURITY CONDITION 2: no ACAO, no ACAC, empty body.
        assert!(!response
            .headers()
            .contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN));
        assert!(!response
            .headers()
            .contains_key(header::ACCESS_CONTROL_ALLOW_CREDENTIALS));
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert!(body.is_empty(), "denied 403 must have a generic empty body");
    }

    #[tokio::test]
    async fn no_origin_options_is_not_a_preflight_and_passes_through() {
        // OPTIONS without Origin/ACRM is NOT a genuine preflight — the observer
        // must not 403 it (security condition 1).
        let app = test_app();
        let response = app
            .oneshot(
                HttpRequest::builder()
                    .method(Method::OPTIONS)
                    .uri("/api/v1/thing")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_ne!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn no_origin_get_passes_through_untouched() {
        let app = test_app();
        let response = app
            .oneshot(
                HttpRequest::builder()
                    .method(Method::GET)
                    .uri("/api/v1/thing")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn origin_without_request_method_is_not_a_preflight() {
        // Origin present but no Access-Control-Request-Method → not a genuine
        // preflight; a denied-looking origin must NOT be 403'd.
        let app = test_app();
        let response = app
            .oneshot(
                HttpRequest::builder()
                    .method(Method::OPTIONS)
                    .uri("/api/v1/thing")
                    .header(header::ORIGIN, "http://evil.test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_ne!(response.status(), StatusCode::FORBIDDEN);
    }
}
