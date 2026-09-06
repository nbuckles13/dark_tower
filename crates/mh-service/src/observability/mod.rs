//! Observability module for Media Handler service
//!
//! Implements metrics and instrumentation per ADR-0011 (Observability Framework).
//!
//! # Privacy by Default
//!
//! All instrumentation uses `#[instrument(skip_all)]` and explicit safe field
//! allow-listing. Metric labels are bounded to prevent cardinality explosion.
//!
//! # Metrics
//!
//! **`docs/observability/metrics/mh-service.md` is the catalog of record** for
//! every MH metric: its type, labels, permitted label values, cardinality and
//! meaning. This module deliberately keeps no second copy.
//!
//! There used to be a table here. It drifted seven metrics behind the code
//! (`mh_active_connections`, `mh_webtransport_connections_total`,
//! `mh_webtransport_handshake_duration_seconds`, `mh_jwt_validations_total`,
//! `mh_caller_type_rejected_total`, `mh_mc_notifications_total` and
//! `mh_register_meeting_timeouts_total` were all absent), and the label-bounds
//! list above it disagreed with `metrics.rs`'s own docstring about how many
//! values `method` had — neither of them correctly. A pointer cannot drift; an
//! enumeration copied into four files always does.

pub mod health;
pub mod metrics;
pub mod per_frame_trace;

// Re-exports for convenience
pub use health::{health_router, HealthState};
pub use metrics::{
    init_metrics_recorder, record_error, record_gc_heartbeat, record_gc_heartbeat_latency,
    record_gc_registration, record_gc_registration_latency, record_grpc_request,
    record_media_policy_apply, record_token_refresh, PolicyApplyOutcome,
};
