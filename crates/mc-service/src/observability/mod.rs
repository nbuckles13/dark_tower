//! Observability module for Meeting Controller service
//!
//! Implements metrics and instrumentation per ADR-0011 (Observability Framework)
//! and ADR-0023 Section 11 (MC Metrics Requirements).
//!
//! # Privacy by Default
//!
//! All instrumentation uses `#[instrument(skip_all)]` and explicit safe field allow-listing.
//! Metric labels are bounded to prevent cardinality explosion:
//! - `actor_type`: 3 values (controller, meeting, connection)
//! - `operation`: bounded by code (get, set, del, incr, etc.)
//! - `message_type`: bounded by protobuf message types
//! - `reason`: bounded fencing reasons (stale_generation, concurrent_write)
//!
//! # Metrics (ADR-0023 Section 11)
//!
//! | Metric | Type | Labels | Purpose |
//! |--------|------|--------|---------|
//! | `mc_connections_active` | Gauge | none | Current WebTransport connections |
//! | `mc_meetings_active` | Gauge | none | Current active meetings |
//! | `mc_message_latency_seconds` | Histogram | `message_type` | Signaling message processing latency |
//! | `mc_actor_mailbox_depth` | Gauge | `actor_type` | Backpressure indicator per actor type |
//! | `mc_redis_latency_seconds` | Histogram | `operation` | Redis operation latency |
//! | `mc_fenced_out_total` | Counter | `reason` | Split-brain fencing events |
//! | `mc_recovery_duration_seconds` | Histogram | none | Session recovery time |

pub mod health;
pub mod metrics;

// Re-exports for convenience
pub use health::{health_router, HealthState};
pub use metrics::{
    init_metrics_recorder, record_actor_panic, record_fenced_out, record_gc_heartbeat,
    record_gc_heartbeat_latency, record_message_dropped, record_message_latency,
    record_recovery_duration, record_redis_latency, set_actor_mailbox_depth,
    set_connections_active, set_meetings_active,
};
