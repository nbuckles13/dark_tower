//! Telemetry proxy handlers (R-2).
//!
//! `POST /api/v1/telemetry/v1/{metrics,traces}` — a sanitizing OTLP reverse
//! proxy with a deny-by-default attribute allowlist. Behind `require_user_auth`.
//!
//! Pipeline (fail-cheap-first):
//! 1. size cap (`telemetry_proxy_max_bytes`) → 413. Two-tier (Option 1): this
//!    handler's explicit `body.len() > max_bytes` check is the user-facing 413
//!    at the exact configured limit (and emits `rejected_size`); the route's
//!    `DefaultBodyLimit` layer is a higher `2×` hard-ceiling DoS backstop that
//!    rejects pathological bodies before unbounded buffering.
//! 2. Content-Type `application/x-protobuf` → 415 otherwise
//! 3. per-JWT-`sub` rate limit (`governor`) → 429
//! 4. decode OTLP-proto (malformed → 400) + 12-key allowlist PII filter +
//!    re-serialize
//! 5. forward filtered bytes to the collector → 502 on failure
//! 6. 202 Accepted
//!
//! Every exit path records `gc_telemetry_ingest_total{status,payload_kind}` +
//! `gc_telemetry_ingest_duration_seconds{status}` exactly once via the
//! [`IngestGuard`] record-on-drop guard, so no path can forget to emit.
//!
//! `/v1/logs` is intentionally NOT routed this story (reserved for a future
//! story); the path simply does not exist (deny-by-default).

use crate::errors::GcError;
use crate::observability::metrics;
use crate::services::telemetry_filter;
use crate::services::telemetry_forwarder::{Signal, TelemetryForwarder};
use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    Extension,
};
use common::jwt::UserClaims;
use governor::clock::DefaultClock;
use governor::state::keyed::DefaultKeyedStateStore;
use governor::RateLimiter;
use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use prost::Message;
use std::sync::Arc;
use std::time::Instant;
use tracing::instrument;

/// Keyed (per-`sub`) GCRA rate limiter type for the telemetry proxy.
pub type UserRateLimiter = RateLimiter<String, DefaultKeyedStateStore<String>, DefaultClock>;

/// Required request Content-Type for OTLP/HTTP-proto payloads.
const PROTOBUF_CONTENT_TYPE: &str = "application/x-protobuf";

/// Multiple of `telemetry_proxy_max_bytes` used for the route's
/// `DefaultBodyLimit` hard ceiling (the DoS backstop above the user-facing
/// limit). See [`body_limit_ceiling`].
const BODY_LIMIT_CEILING_MULTIPLE: usize = 2;

/// The `DefaultBodyLimit` ceiling for the telemetry routes.
///
/// Two-tier size cap (Option 1, security-reviewed):
/// - The HANDLER's explicit `body.len() > max_bytes` check is the load-bearing
///   USER-FACING 413 at the EXACT configured limit, and it emits
///   `rejected_size` via the `IngestGuard` (one emission path, route-derived
///   `payload_kind`).
/// - This layer ceiling (`2 ×` the configured limit) is only a DoS backstop:
///   a truly pathological body is rejected by the tower layer without being
///   fully buffered. It is a MODEST fixed multiple that scales with config —
///   never unbounded — and is always `>= max_bytes` (`saturating_mul` guards a
///   near-`usize::MAX` configured limit from overflowing/inverting).
pub fn body_limit_ceiling(max_bytes: usize) -> usize {
    max_bytes.saturating_mul(BODY_LIMIT_CEILING_MULTIPLE)
}

/// Shared state for the telemetry proxy handlers: the per-user rate limiter and
/// the collector forwarder. Held in `AppState` as cheap-to-clone `Arc`s.
#[derive(Clone)]
pub struct TelemetryState {
    pub rate_limiter: Arc<UserRateLimiter>,
    pub forwarder: TelemetryForwarder,
    /// Maximum accepted payload size in bytes — the EXACT user-facing 413 limit
    /// enforced by the handler (the route's `DefaultBodyLimit` sits above this at
    /// a `2×` hard ceiling; see [`body_limit_ceiling`]).
    pub max_bytes: usize,
}

impl TelemetryState {
    /// Build telemetry state from config: a per-`sub` keyed GCRA limiter at
    /// `rate_limit_per_minute` and a forwarder pointed at the bare-base
    /// collector endpoint.
    pub fn from_config(
        otel_collector_endpoint: String,
        max_bytes: usize,
        rate_limit_per_minute: u32,
    ) -> Result<Self, GcError> {
        // rate_limit_per_minute is validated > 0 at config load; guard anyway so
        // a programming error can't panic on NonZeroU32.
        let quota_per_min = std::num::NonZeroU32::new(rate_limit_per_minute).ok_or_else(|| {
            GcError::Internal("telemetry rate limit must be greater than 0".to_string())
        })?;
        let quota = governor::Quota::per_minute(quota_per_min);
        let rate_limiter = Arc::new(RateLimiter::keyed(quota));
        let forwarder = TelemetryForwarder::new(otel_collector_endpoint)?;

        Ok(Self {
            rate_limiter,
            forwarder,
            max_bytes,
        })
    }

    /// Reclaim fully-replenished (idle) per-`sub` cells from the keyed limiter,
    /// bounding the keyspace to ~active-users-per-window. Driven by a periodic
    /// task spawned at startup (see `spawn_rate_limiter_eviction`).
    pub fn retain_recent(&self) {
        self.rate_limiter.retain_recent();
    }
}

/// Spawn a lifecycle task that periodically reclaims idle rate-limiter cells.
///
/// Fire-and-forget at startup; bounds the keyed-limiter memory to active users.
pub fn spawn_rate_limiter_eviction(
    state: TelemetryState,
    period: std::time::Duration,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(period);
        // Skip the immediate first tick.
        interval.tick().await;
        loop {
            interval.tick().await;
            state.retain_recent();
        }
    })
}

/// Records the ingest outcome metrics on drop, so EVERY return path emits
/// exactly once. The handler sets `status` before returning; `payload_kind` is
/// fixed from the route (never from payload content), so it is a bounded literal
/// even on a pre-decode reject.
///
/// Status taxonomy (intentional collapse): the size/rate rejects carry distinct
/// statuses (`rejected_size`, `rejected_rate`), but the 415 (content-type), 400
/// (decode), 502 (collector), and 503 (disabled) paths all share `status=error`.
/// This is deliberate — `gc_telemetry_ingest_total` is a RATE/health metric, and
/// the HTTP status code (recorded separately on `gc_http_requests_total`) is the
/// discriminator for those failure classes. Distinct error sub-statuses
/// (rejected_ct / rejected_decode / bad_gateway / disabled) are an observability
/// taxonomy decision deferred to @observability / task #16, not an oversight.
struct IngestGuard {
    start: Instant,
    payload_kind: &'static str,
    status: &'static str,
}

impl IngestGuard {
    fn new(payload_kind: &'static str) -> Self {
        Self {
            start: Instant::now(),
            payload_kind,
            // Pessimistic default: if we somehow return without setting a status
            // the metric still records (as an error), never silently dropped.
            status: "error",
        }
    }

    fn set_status(&mut self, status: &'static str) {
        self.status = status;
    }
}

impl Drop for IngestGuard {
    fn drop(&mut self) {
        metrics::record_telemetry_ingest(self.status, self.payload_kind, self.start.elapsed());
    }
}

/// `POST /api/v1/telemetry/v1/metrics`
#[instrument(skip_all, name = "gc.handlers.telemetry.metrics")]
pub async fn ingest_metrics(
    State(state): State<TelemetryState>,
    Extension(user_claims): Extension<UserClaims>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, GcError> {
    ingest(state, user_claims, headers, body, PayloadKind::Metric).await
}

/// `POST /api/v1/telemetry/v1/traces`
#[instrument(skip_all, name = "gc.handlers.telemetry.traces")]
pub async fn ingest_traces(
    State(state): State<TelemetryState>,
    Extension(user_claims): Extension<UserClaims>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, GcError> {
    ingest(state, user_claims, headers, body, PayloadKind::Trace).await
}

/// Which OTLP signal this request carries. Drives the bounded `payload_kind`
/// metric label and the forwarder signal — derived from the route, never from
/// payload content.
#[derive(Clone, Copy)]
enum PayloadKind {
    Metric,
    Trace,
}

impl PayloadKind {
    fn label(self) -> &'static str {
        match self {
            PayloadKind::Metric => "metric",
            PayloadKind::Trace => "trace",
        }
    }

    fn signal(self) -> Signal {
        match self {
            PayloadKind::Metric => Signal::Metrics,
            PayloadKind::Trace => Signal::Traces,
        }
    }
}

async fn ingest(
    state: TelemetryState,
    user_claims: UserClaims,
    headers: HeaderMap,
    body: Bytes,
    kind: PayloadKind,
) -> Result<StatusCode, GcError> {
    let mut guard = IngestGuard::new(kind.label());

    // 0. Disabled-state: empty collector endpoint → telemetry proxy is off.
    if !state.forwarder.is_enabled() {
        guard.set_status("error");
        return Err(GcError::ServiceUnavailable(
            "Telemetry proxy is disabled".to_string(),
        ));
    }

    // 1. Size cap on REAL bytes — the USER-FACING 413 gate at the EXACT
    //    configured limit (Option 1). This is where `rejected_size` is emitted
    //    (via the IngestGuard), so it MUST run in the handler. The route's
    //    `DefaultBodyLimit` layer sits ABOVE this at a 2× hard ceiling (DoS
    //    backstop) — a body in (max_bytes, 2×max_bytes] reaches here and gets
    //    this precise 413 + metric; a body > 2×max_bytes is rejected by the layer
    //    (413, no metric, no decode). Fires before prost::decode either way.
    //
    //    SECURITY INVARIANT (do not remove): because the `DefaultBodyLimit` layer
    //    is at `2× max_bytes` (above the decode cap), THIS check is the sole
    //    pre-decode oversize gate for bodies in (max_bytes, 2×max_bytes]. Removing
    //    it — or raising the layer further without a matching pre-decode check —
    //    would let prost::decode run on oversize bytes. Keep this check whenever
    //    the layer ceiling exceeds `max_bytes`.
    if body.len() > state.max_bytes {
        guard.set_status("rejected_size");
        return Err(GcError::PayloadTooLarge(
            "Telemetry payload exceeds the maximum allowed size".to_string(),
        ));
    }

    // 2. Content-Type must be application/x-protobuf.
    let content_type = headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    // Tolerate a trailing `; charset=...` parameter.
    let base_content_type = content_type.split(';').next().unwrap_or("").trim();
    if base_content_type != PROTOBUF_CONTENT_TYPE {
        guard.set_status("error");
        return Err(GcError::UnsupportedMediaType(
            "Content-Type must be application/x-protobuf".to_string(),
        ));
    }

    // 3. Per-`sub` rate limit. `sub` comes from the validated user claims in
    //    request extensions, never a client-supplied header.
    if state.rate_limiter.check_key(&user_claims.sub).is_err() {
        guard.set_status("rejected_rate");
        metrics::record_telemetry_rate_limited("per_user");
        return Err(GcError::RateLimitExceeded);
    }

    // Record accepted payload size (real bytes).
    metrics::record_telemetry_payload_bytes(kind.label(), body.len());

    // 4. Decode + filter + re-serialize. Malformed OTLP → 400 (generic client
    //    message; the prost error is logged server-side WITHOUT the body bytes).
    let filtered_bytes = match kind {
        PayloadKind::Metric => {
            let mut req = ExportMetricsServiceRequest::decode(body).map_err(|e| {
                guard.set_status("error");
                tracing::debug!(target: "gc.handlers.telemetry", error = %e, "OTLP metrics decode failed");
                GcError::BadRequest("Malformed OTLP payload".to_string())
            })?;
            let counts = telemetry_filter::filter_metrics(&mut req);
            // Total is a bounded count (no key/value content) — safe to log.
            tracing::debug!(target: "gc.handlers.telemetry", dropped = counts.total(), "metrics PII filter applied");
            counts.emit();
            req.encode_to_vec()
        }
        PayloadKind::Trace => {
            let mut req = ExportTraceServiceRequest::decode(body).map_err(|e| {
                guard.set_status("error");
                tracing::debug!(target: "gc.handlers.telemetry", error = %e, "OTLP traces decode failed");
                GcError::BadRequest("Malformed OTLP payload".to_string())
            })?;
            let counts = telemetry_filter::filter_traces(&mut req);
            tracing::debug!(target: "gc.handlers.telemetry", dropped = counts.total(), "traces PII filter applied");
            counts.emit();
            req.encode_to_vec()
        }
    };

    // 5. Forward the re-encoded (filtered) bytes only. 502 on collector failure.
    state
        .forwarder
        .forward(kind.signal(), filtered_bytes)
        .await
        .inspect_err(|_| {
            guard.set_status("error");
        })?;

    // 6. Success.
    guard.set_status("success");
    Ok(StatusCode::ACCEPTED)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_limit_ceiling_is_double_the_limit() {
        assert_eq!(body_limit_ceiling(256 * 1024), 512 * 1024);
        assert_eq!(body_limit_ceiling(1), 2);
    }

    #[test]
    fn body_limit_ceiling_always_at_least_max_bytes() {
        // The ceiling must never be below the configured limit, or the handler's
        // user-facing gate would be unreachable. saturating_mul guards overflow:
        // even a near-usize::MAX configured limit yields a ceiling >= the limit.
        for max_bytes in [
            0usize,
            1,
            256 * 1024,
            usize::MAX / 2,
            usize::MAX - 1,
            usize::MAX,
        ] {
            assert!(
                body_limit_ceiling(max_bytes) >= max_bytes,
                "ceiling {} < max_bytes {}",
                body_limit_ceiling(max_bytes),
                max_bytes
            );
        }
    }

    #[test]
    fn body_limit_ceiling_saturates_no_overflow() {
        // Must not panic / wrap on huge configured limits.
        assert_eq!(body_limit_ceiling(usize::MAX), usize::MAX);
    }
}
