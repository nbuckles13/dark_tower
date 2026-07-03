//! Inbound HTTP trace-context extraction middleware (R-56 surface (a)).
//!
//! Thin axum wrapper around [`common::observability::otel_http::extract_trace_context`].
//! `common` has no `axum` dependency (framework-agnostic by design — see
//! `otel_http.rs`'s module doc), so the axum-specific glue lives here.
//!
//! # Placement is load-bearing
//!
//! This middleware MUST be layered so it runs AFTER `TraceLayer::new_for_http()`
//! has created and entered its request span, but BEFORE the handler runs.
//! `axum::Router::layer` semantics: the *last*-added `.layer()` call is
//! *outermost* and runs first (see `Router::layer` docs — for
//! `.layer(one).layer(two).layer(three)`, execution is `three → two → one →
//! handler`). `routes/mod.rs::build_routes` adds this middleware's layer as
//! the physically FIRST `.layer()` call — i.e. even earlier than
//! `TraceLayer::new_for_http()` — which makes it MORE INNER than `TraceLayer`,
//! so it executes AFTER `TraceLayer` enters its span and BEFORE the handler.
//! At that point `tracing::Span::current()` correctly resolves to the
//! request span `TraceLayer` created, and `extract_trace_context`'s
//! `.set_parent()` call attaches the extracted W3C parent to it — so every
//! span the handler creates (including the MC client's `#[instrument]` span)
//! inherits it via the tracing hierarchy.
//!
//! Verified against `tower_http::trace::Trace`'s actual `ResponseFuture::poll`
//! semantics (per @observability, Gate-1): the span-entered guard is
//! re-established on every poll of the wrapped future, not just at the
//! initial `.call()`, so `axum::middleware::from_fn`'s lazy-future
//! construction doesn't break this ordering argument.
//!
//! NOTE: this is the OPPOSITE ordering convention from GC's inbound gRPC
//! server (`main.rs`'s `TonicServer::builder()`), which composes via
//! `tower::ServiceBuilder` semantics (first-added = outermost) — see the
//! comment there. Don't flip one by analogy to the other.

use axum::{extract::Request, middleware::Next, response::Response};
use common::observability::otel_http::extract_trace_context;

/// Extract the inbound W3C trace context (`traceparent`/`tracestate`) and
/// attach it as the parent of the current request span.
///
/// See the module doc for the placement requirement this depends on.
pub async fn extract_trace_context_middleware(request: Request, next: Next) -> Response {
    extract_trace_context(request.headers());
    next.run(request).await
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request as HttpRequest, middleware, routing::get, Router};
    use tower::ServiceExt;

    async fn handler() -> &'static str {
        "OK"
    }

    /// Sanity check: the middleware doesn't break normal request handling
    /// when no `traceparent` header is present (extraction is a no-op, the
    /// request proceeds as a new root span).
    #[tokio::test]
    async fn middleware_passes_through_request_without_traceparent() {
        // Route path is a versioned dummy target (not "/") so the
        // `api-version-check` guard's route-literal scan — which has no
        // cfg(test)-module skip — classifies it as a correctly-versioned
        // route instead of flagging a bare "/" as an unversioned API route.
        let app = Router::new()
            .route("/api/v1/otel-test-target", get(handler))
            .layer(middleware::from_fn(extract_trace_context_middleware));

        let response = app
            .oneshot(
                HttpRequest::builder()
                    .uri("/api/v1/otel-test-target")
                    .body(Body::empty())
                    .expect("valid request"),
            )
            .await
            .expect("request should succeed");

        assert_eq!(response.status(), axum::http::StatusCode::OK);
    }

    /// Middleware-level no-leak test (per @security, Gate-1 item 1): a
    /// request carrying `Authorization`/`Cookie` alongside a valid
    /// `traceparent` must reach the handler with those headers untouched.
    /// This exercises the WIRED middleware (not just the bare
    /// `otel_http` adapter-unit-level tests), closing the gap between "the
    /// extractor never reads those keys" and "the wired middleware never
    /// accidentally forwards/logs the full HeaderMap somewhere."
    #[tokio::test]
    async fn middleware_passes_through_authorization_and_cookie_untouched() {
        async fn assert_headers_untouched(request: Request) -> Response {
            let auth = request
                .headers()
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .map(str::to_string);
            let cookie = request
                .headers()
                .get("cookie")
                .and_then(|v| v.to_str().ok())
                .map(str::to_string);
            assert_eq!(auth.as_deref(), Some("Bearer SECRETxyz"));
            assert_eq!(cookie.as_deref(), Some("session=SECRETcookie"));
            "OK".into_response()
        }

        use axum::response::IntoResponse;

        // See the sibling test above for why this uses a versioned dummy
        // path instead of "/".
        let app = Router::new()
            .route("/api/v1/otel-test-target", get(assert_headers_untouched))
            .layer(middleware::from_fn(extract_trace_context_middleware));

        let response = app
            .oneshot(
                HttpRequest::builder()
                    .uri("/api/v1/otel-test-target")
                    .header("authorization", "Bearer SECRETxyz")
                    .header("cookie", "session=SECRETcookie")
                    .header(
                        "traceparent",
                        "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
                    )
                    .body(Body::empty())
                    .expect("valid request"),
            )
            .await
            .expect("request should succeed");

        assert_eq!(response.status(), axum::http::StatusCode::OK);
    }

    /// Same scenario as above, but additionally captures every span field
    /// recorded during the request (via a real `TraceLayer::new_for_http()`
    /// in the stack, matching production layering) and asserts NONE contain
    /// the `authorization`/`cookie` key names or secret values. Per
    /// @security Gate-1 item 1: "the wired middleware never accidentally
    /// logs/forwards the full HeaderMap somewhere."
    #[tokio::test]
    async fn middleware_never_copies_authorization_or_cookie_into_span_fields() {
        use std::sync::{Arc as StdArc, Mutex};
        use tower_http::trace::TraceLayer;
        use tracing::field::{Field, Visit};
        use tracing::span::{Attributes, Record};
        use tracing::{Id, Subscriber};
        use tracing_subscriber::layer::Context as LayerContext;
        use tracing_subscriber::layer::SubscriberExt;
        use tracing_subscriber::registry::LookupSpan;
        use tracing_subscriber::Layer;

        #[derive(Default, Clone)]
        struct Capture {
            observations: StdArc<Mutex<Vec<(String, String)>>>,
        }

        struct CaptureLayer {
            observations: StdArc<Mutex<Vec<(String, String)>>>,
        }

        struct AttrVisitor {
            out: StdArc<Mutex<Vec<(String, String)>>>,
        }

        impl Visit for AttrVisitor {
            fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
                let mut lock = self
                    .out
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                lock.push((field.name().to_string(), format!("{value:?}")));
            }
            fn record_str(&mut self, field: &Field, value: &str) {
                let mut lock = self
                    .out
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                lock.push((field.name().to_string(), value.to_string()));
            }
        }

        impl<S> Layer<S> for CaptureLayer
        where
            S: Subscriber + for<'a> LookupSpan<'a>,
        {
            fn on_new_span(&self, attrs: &Attributes<'_>, _id: &Id, _cx: LayerContext<'_, S>) {
                let mut visitor = AttrVisitor {
                    out: StdArc::clone(&self.observations),
                };
                attrs.values().record(&mut visitor);
            }

            fn on_record(&self, _id: &Id, values: &Record<'_>, _cx: LayerContext<'_, S>) {
                let mut visitor = AttrVisitor {
                    out: StdArc::clone(&self.observations),
                };
                values.record(&mut visitor);
            }
        }

        async fn handler() -> &'static str {
            "OK"
        }

        let capture = Capture::default();
        let subscriber = tracing_subscriber::registry().with(CaptureLayer {
            observations: StdArc::clone(&capture.observations),
        });
        // `set_default` stays active for the lifetime of `_guard`, including
        // across the `.await` points below (this test runs on tokio's
        // default current-thread flavor, so no cross-thread handoff drops it).
        let _guard = tracing::subscriber::set_default(subscriber);

        // See the module's first test for why this uses a versioned dummy
        // path instead of "/".
        let app = Router::new()
            .route("/api/v1/otel-test-target", get(handler))
            .layer(middleware::from_fn(extract_trace_context_middleware))
            .layer(TraceLayer::new_for_http());

        let response = app
            .oneshot(
                HttpRequest::builder()
                    .uri("/api/v1/otel-test-target")
                    .header("authorization", "Bearer SECRETxyz")
                    .header("cookie", "session=SECRETcookie")
                    .header(
                        "traceparent",
                        "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
                    )
                    .body(Body::empty())
                    .expect("valid request"),
            )
            .await
            .expect("request should succeed");
        assert_eq!(response.status(), axum::http::StatusCode::OK);

        let observations = capture
            .observations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        for (field, value) in observations {
            assert!(
                !field.to_ascii_lowercase().contains("authorization")
                    && !field.to_ascii_lowercase().contains("cookie"),
                "span field name {field:?} suggests an auth/cookie leak"
            );
            assert!(
                !value.contains("SECRETxyz") && !value.contains("SECRETcookie"),
                "span field value {value:?} contains secret header material"
            );
        }
    }
}
