//! Service layer for Global Controller.
//!
//! This module contains services that interact with external systems
//! and encapsulate business logic.
//!
//! # Components
//!
//! - `ac_client` - HTTP client for Auth Controller internal endpoints
//! - `mc_assignment` - Meeting Controller assignment with load balancing
//! - `mc_client` - gRPC client for GC→MC communication
//! - `mh_selection` - Media Handler selection for meetings

pub mod ac_client;
pub mod mc_assignment;
pub mod mc_client;
pub mod mh_selection;
pub mod telemetry_filter;
pub mod telemetry_forwarder;

/// Build the W3C trace-context headers for the active span (R-56 outbound
/// HTTP surfaces: GC→AC in [`ac_client`] and the telemetry-proxy forward in
/// [`telemetry_forwarder`]). Returns a `HeaderMap` containing ONLY
/// `traceparent`/`tracestate` (if a parent context is active) — merge via
/// `RequestBuilder::headers`, which adds to rather than replaces the
/// request's existing headers (including any `Authorization` header set
/// separately at the call site). Uses `reqwest::header::HeaderMap` (a
/// re-export of `http::HeaderMap` — same type `otel_http::inject_trace_context`
/// takes) so gc-service does not need a direct `http` crate dependency;
/// `reqwest` already provides it.
pub(crate) fn trace_headers() -> reqwest::header::HeaderMap {
    let mut headers = reqwest::header::HeaderMap::new();
    common::observability::otel_http::inject_trace_context(&mut headers);
    headers
}

pub use mc_assignment::McAssignmentService;
// MC client types exposed for external use
pub use mc_client::{McClient, McClientTrait};
// Mock MC client for testing (exposed for integration tests)
#[allow(unused_imports)]
pub use mc_client::mock::MockMcClient;
// MH selection types exposed for external/test use
#[allow(unused_imports)]
pub use mh_selection::{MhAssignmentInfo, MhSelection, MhSelectionService};
