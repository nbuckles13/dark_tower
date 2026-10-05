//! Inbound HTTP trace-context extraction middleware (R-56).
//!
//! Thin axum wrapper around [`common::observability::otel_http::extract_trace_context`]
//! — the receive half of GC→AC trace continuity (GC's `AcClient` injects
//! `traceparent` on every call). `common` has no `axum` dependency, so the
//! axum glue lives here, mirroring `gc-service/src/middleware/otel.rs`.
//!
//! # Placement is load-bearing
//!
//! `routes/mod.rs::build_routes` adds this layer as the FIRST `.layer()` on
//! the merged router — physically above `TraceLayer` — so it is MORE INNER
//! than `TraceLayer` (axum: last-added layer is outermost) and runs after
//! `TraceLayer` has created and entered the request span, but before any
//! route-level middleware (auth) or handler creates a child span. Ordering
//! before children matters: tracing-opentelemetry 0.32+ starts a span's OTel
//! context once a child exists, after which the parent cannot be attached.

use axum::{extract::Request, middleware::Next, response::Response};
use common::observability::otel_http::extract_trace_context;

/// Extract the inbound W3C trace context (`traceparent`/`tracestate`) and
/// attach it as the parent of the current request span.
pub async fn extract_trace_context_middleware(request: Request, next: Next) -> Response {
    extract_trace_context(request.headers());
    next.run(request).await
}
