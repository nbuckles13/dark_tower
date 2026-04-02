//! Observability module for Media Handler service
//!
//! Implements metrics and instrumentation per ADR-0011 (Observability Framework).
//!
//! # Privacy by Default
//!
//! All instrumentation uses `#[instrument(skip_all)]` and explicit safe field allow-listing.
//! Metric labels are bounded to prevent cardinality explosion:
//! - `status`: success/error (2 values)
//! - `method`: bounded by gRPC methods (~3 values)
//! - `error_type`: bounded by `MhError` variants (~6 values)
//! - `operation`: bounded by code paths (~5 values)
//!
//! # Metrics (ADR-0011)
//!
//! | Metric | Type | Labels | Purpose |
//! |--------|------|--------|---------|
//! | `mh_gc_registration_total` | Counter | `status` | RegisterMH call outcomes |
//! | `mh_gc_registration_duration_seconds` | Histogram | none | Registration RPC latency |
//! | `mh_gc_heartbeats_total` | Counter | `status` | SendLoadReport outcomes |
//! | `mh_gc_heartbeat_latency_seconds` | Histogram | none | Heartbeat RPC latency |
//! | `mh_token_refresh_total` | Counter | `status` | Token refresh attempts |
//! | `mh_token_refresh_duration_seconds` | Histogram | none | Token refresh latency |
//! | `mh_token_refresh_failures_total` | Counter | `error_type` | Failure breakdown |
//! | `mh_grpc_requests_total` | Counter | `method`, `status` | Incoming gRPC from MC |
//! | `mh_errors_total` | Counter | `operation`, `error_type`, `status_code` | Global error counter |

pub mod health;
pub mod metrics;

// Re-exports for convenience
pub use health::{health_router, HealthState};
pub use metrics::{
    init_metrics_recorder, record_error, record_gc_heartbeat, record_gc_heartbeat_latency,
    record_gc_registration, record_gc_registration_latency, record_grpc_request,
    record_token_refresh,
};
