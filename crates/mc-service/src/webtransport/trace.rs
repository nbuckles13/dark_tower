//! W3C trace-context helpers for the WebTransport signaling boundary (R-57).
//!
//! Symmetric inject/extract over the `ClientMessage`/`ServerMessage`
//! `trace_parent`/`trace_state` fields, built on the GLOBAL bounded propagator
//! installed by `init_otel` (never a fresh `TraceContextPropagator`). Kept local
//! to `mc-service` (not hoisted to `common`) per the DRY adjudication for
//! task #6 — see `docs/TODO.md` §Cross-Service Duplication for the eventual
//! symmetric-helper hoist breadcrumb (MH #27 is the extract-only sibling).
//!
//! The two carrier keys are `otel_grpc::PROPAGATED_HEADERS` (`traceparent`,
//! `tracestate`) — the single source of the allowlist across gRPC + WebTransport.

use std::collections::HashMap;
use tracing_opentelemetry::OpenTelemetrySpanExt;

/// The two W3C carrier keys. Literals (mirroring
/// `mh-service/src/webtransport/connection.rs`) rather than
/// `otel_grpc::PROPAGATED_HEADERS[0]`/`[1]` — slice indexing trips
/// `clippy::indexing-slicing`, and these are the exact same two strings the
/// bounded propagator's set/get allowlist enforces.
const TRACEPARENT_KEY: &str = "traceparent";
const TRACESTATE_KEY: &str = "tracestate";

/// Extract a remote W3C trace context from a message's `trace_parent`/
/// `trace_state` fields.
///
/// Uses the GLOBAL bounded propagator, so W3C length/format bounds apply to this
/// untrusted browser-originated input. Empty fields (proto3 defaults) are
/// skipped → an empty carrier extracts to a parentless root context.
#[must_use]
pub fn extract_parent(trace_parent: &str, trace_state: &str) -> opentelemetry::Context {
    let mut carrier: HashMap<String, String> = HashMap::new();
    if !trace_parent.is_empty() {
        carrier.insert(TRACEPARENT_KEY.to_string(), trace_parent.to_string());
    }
    if !trace_state.is_empty() {
        carrier.insert(TRACESTATE_KEY.to_string(), trace_state.to_string());
    }
    opentelemetry::global::get_text_map_propagator(|propagator| propagator.extract(&carrier))
}

/// Reparent the CURRENT span onto the remote context carried by a message's
/// trace fields. No-op when both are empty (clean root).
pub fn reparent_current_span(trace_parent: &str, trace_state: &str) {
    if trace_parent.is_empty() && trace_state.is_empty() {
        return;
    }
    tracing::Span::current().set_parent(extract_parent(trace_parent, trace_state));
}

/// Inject the CURRENT span's W3C trace context, returning
/// `(trace_parent, trace_state)` for an outbound `ServerMessage`.
///
/// Uses the GLOBAL propagator (symmetric with [`extract_parent`]) and copies out
/// ONLY the two `PROPAGATED_HEADERS` keys — never hand-rolls the W3C string, so
/// the bounded propagator stays the single source of format truth. Returns empty
/// strings when there is no active span context (clean proto3 default).
#[must_use]
pub fn inject_current_context() -> (String, String) {
    let cx = tracing::Span::current().context();
    let mut carrier: HashMap<String, String> = HashMap::new();
    opentelemetry::global::get_text_map_propagator(|propagator| {
        propagator.inject_context(&cx, &mut carrier);
    });
    let trace_parent = carrier.get(TRACEPARENT_KEY).cloned().unwrap_or_default();
    let trace_state = carrier.get(TRACESTATE_KEY).cloned().unwrap_or_default();
    (trace_parent, trace_state)
}
