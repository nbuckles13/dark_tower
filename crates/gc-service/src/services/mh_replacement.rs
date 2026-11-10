//! `AcquireMediaHandler`: the MC→GC replacement grant (ADR-0039 §3, R-23).
//!
//! MC removes a failed handler first, then asks GC for ONE replacement. GC
//! checks the caller is the meeting's controller (anti-misroute only), derives
//! the region from that assignment row, and selects with the SAME function as
//! initial allocation (`MhSelectionService::select`, count 1, the request's
//! exclude list). GC persists nothing per meeting: a grant has no write side
//! effect.
//!
//! `trigger`, `replaced_handler_id` and GC's own view of the replaced handler
//! (`gc_view`) are EVIDENCE ONLY: labels and the `gc.mh.replacement_report`
//! event. None of them reaches `select` (ADR-0039 §9), and `gc_view` is never
//! a veto.
//!
//! Layering: request validation (shape, ids, cap, trigger) is done by the gRPC
//! handler (`grpc/mc_service.rs`), which owns the existing id validators; this
//! module takes an already-validated [`Validated`] request. The handler
//! records the metrics and the event ONCE per request from the
//! `(Result, Report)` pair returned here.

use crate::config::MhRegistryConfig;
use crate::errors::GcError;
use crate::observability::metrics::{GcView, ReplacementOutcome, ReplacementTrigger};
use crate::repositories::{HealthStatus, MediaHandlersRepository, MeetingAssignmentsRepository};
use crate::services::mh_selection::{is_fresh, MhAssignmentInfo, MhSelectionService};
use crate::services::weighted_pick::SelectionRng;
use proto_gen::dark_tower::internal::v1::ReleaseCause;
use sqlx::PgPool;
use std::time::Duration;

/// Project a wire `ReleaseCause` (raw `i32`: proto3 enums are open) onto the
/// replacement `trigger`. The ONE projection: the counter label, the event
/// field and the request-validity check all come from it.
///
/// `None` (an INVALID trigger for this rpc) for UNSPECIFIED, MEETING_ENDED (a
/// meeting end is not a handler replacement) and any unknown number.
#[must_use]
pub fn project_trigger(raw: i32) -> Option<ReplacementTrigger> {
    let cause = ReleaseCause::try_from(raw).ok()?;
    // Exhaustive, no wildcard: a new ReleaseCause value must be classified here.
    match cause {
        ReleaseCause::HeartbeatTimeout => Some(ReplacementTrigger::HeartbeatTimeout),
        ReleaseCause::ApplyLag => Some(ReplacementTrigger::ApplyLag),
        ReleaseCause::PlannedTransfer => Some(ReplacementTrigger::PlannedTransfer),
        ReleaseCause::Unspecified | ReleaseCause::MeetingEnded => None,
    }
}

/// GC's view of the replaced handler, from its registry row (a pure function;
/// the label only).
///
/// Precedence: no row → `unregistered`; not fresh (`mh_selection::is_fresh`,
/// the one freshness rule selection also uses) → `stale` whatever the status
/// (a stale draining row is effectively gone); otherwise the status.
#[must_use]
pub fn classify(row: Option<(HealthStatus, Duration)>, staleness_bound: Duration) -> GcView {
    let Some((status, heartbeat_age)) = row else {
        return GcView::Unregistered;
    };
    if !is_fresh(heartbeat_age, staleness_bound) {
        return GcView::Stale;
    }
    match status {
        HealthStatus::Healthy => GcView::Healthy,
        HealthStatus::Degraded => GcView::Degraded,
        HealthStatus::Unhealthy => GcView::Unhealthy,
        HealthStatus::Draining => GcView::Draining,
        HealthStatus::Pending => GcView::Pending,
    }
}

/// A request that passed validation: every id well-formed, the exclude list
/// within the cap, the trigger a replacement cause.
#[derive(Debug, Clone)]
pub struct Validated {
    /// Meeting being repaired.
    pub meeting_id: String,
    /// The caller's SELF-ASSERTED controller id (anti-misroute key only).
    pub controller_id: String,
    /// Handler ids never to grant (current ∪ removed ∪ refused-at-admission).
    pub exclude: Vec<String>,
    /// The projected (valid) trigger.
    pub trigger: ReplacementTrigger,
    /// The handler MC removed, if it named one (evidence only).
    pub replaced_handler_id: Option<String>,
}

/// Why a request was not granted. `Granted` is not representable here, so
/// "granted ⇔ Ok" holds by construction (see [`outcome`]).
#[derive(Debug)]
pub enum Rejection {
    /// INVALID_ARGUMENT: the validator's own (bounded, value-free) status,
    /// returned unchanged.
    InvalidRequest(tonic::Status),
    /// FAILED_PRECONDITION: no active assignment pairs (meeting, controller).
    NotOwner,
    /// RESOURCE_EXHAUSTED: no eligible candidate after exclusion.
    NoCapacity,
    /// INTERNAL: carries the underlying error (logged server-side only).
    Error(GcError),
}

/// The one Result → metric outcome mapping.
#[must_use]
pub fn outcome(result: &Result<MhAssignmentInfo, Rejection>) -> ReplacementOutcome {
    match result {
        Ok(_) => ReplacementOutcome::Granted,
        Err(Rejection::InvalidRequest(_)) => ReplacementOutcome::InvalidRequest,
        Err(Rejection::NotOwner) => ReplacementOutcome::NotOwner,
        Err(Rejection::NoCapacity) => ReplacementOutcome::NoCapacity,
        Err(Rejection::Error(_)) => ReplacementOutcome::Error,
    }
}

/// What the metrics and the `gc.mh.replacement_report` event say about one
/// request. Every id here has passed validation; a field that failed (or was
/// never read) is `None`.
#[derive(Debug, Clone)]
pub struct Report {
    /// `trigger` label (`Invalid` only with `invalid_request`).
    pub trigger: ReplacementTrigger,
    /// `gc_view` label (`Unevaluated` until the replaced row is read).
    pub gc_view: GcView,
    /// The replaced handler id, if present AND well-formed.
    pub replaced_handler_id: Option<String>,
    /// The replaced handler's endpoint from GC's row (only if the row exists).
    pub webtransport_endpoint: Option<String>,
    /// The caller's controller id, if well-formed (self-asserted).
    pub controller_id: Option<String>,
}

/// Grant one replacement handler for a validated request.
///
/// Order: ownership (which supplies the region) → the replaced handler's row
/// (`gc_view`, after ownership so a non-owner cannot probe it) → `select`.
pub async fn acquire(
    pool: &PgPool,
    registry: &MhRegistryConfig,
    req: &Validated,
    rng: &dyn SelectionRng,
) -> (Result<MhAssignmentInfo, Rejection>, Report) {
    let mut report = Report {
        trigger: req.trigger,
        gc_view: GcView::Unevaluated,
        replaced_handler_id: req.replaced_handler_id.clone(),
        webtransport_endpoint: None,
        controller_id: Some(req.controller_id.clone()),
    };

    // Ownership: ANTI-MISROUTE ONLY, NOT AUTHENTICATION (ADR-0039 §3, recorded
    // gap). `controller_id` is self-asserted: any meeting-controller service
    // token may name any controller. The region comes ONLY from this row (the
    // request has no region field). No row, an ended row and another
    // controller's row are indistinguishable to the caller.
    let region = match MeetingAssignmentsRepository::get_active_assignment_region(
        pool,
        &req.meeting_id,
        &req.controller_id,
    )
    .await
    {
        Ok(Some(region)) => region,
        Ok(None) => return (Err(Rejection::NotOwner), report),
        Err(e) => return (Err(Rejection::Error(e)), report),
    };

    // GC's view of the replaced handler: a label, never a veto. A lookup
    // failure fails the request (outcome=error) rather than silently reading
    // as `unregistered`.
    let bound = registry.staleness_bound();
    report.gc_view = match &req.replaced_handler_id {
        None => GcView::Unregistered,
        Some(id) => match MediaHandlersRepository::get_handler(pool, id).await {
            Ok(None) => GcView::Unregistered,
            Ok(Some(row)) => {
                // GC's clock against the DB-written heartbeat timestamp: a
                // label-only approximation of the SQL's `NOW()` comparison.
                let age = (chrono::Utc::now() - row.last_heartbeat_at)
                    .to_std()
                    .unwrap_or(Duration::ZERO);
                report.webtransport_endpoint = Some(row.webtransport_endpoint);
                classify(Some((row.health_status, age)), bound)
            }
            Err(e) => return (Err(Rejection::Error(e)), report),
        },
    };

    // The SAME selection function as initial allocation (R-23). Its signature
    // carries none of trigger / replaced_handler_id / gc_view.
    let result = match MhSelectionService::select(pool, &region, bound, 1, &req.exclude, rng).await
    {
        Ok(mut picked) => match picked.pop() {
            Some(handler) => Ok(handler),
            None => Err(Rejection::NoCapacity),
        },
        Err(e) => Err(Rejection::Error(e)),
    };
    (result, report)
}

/// Emit the `gc.mh.replacement_report` INFO event: once per request whose
/// `replaced_handler_id` was present and well-formed, on EVERY outcome.
///
/// This event, not any in-memory window, is the corroboration source for the
/// gray-failure log query (GC runs 2+ replicas; a per-replica window would
/// undercount). EVIDENCE RULE (one statement, mirrored verbatim in the
/// catalog, the dashboard panel, docs/TODO.md and the telemetry contract):
/// only events that PASSED the ownership check count, i.e. `outcome` =
/// `granted` | `no_capacity`, or `error` with `gc_view` != `unevaluated` (an
/// evaluated view means ownership and the replaced-row read succeeded). An
/// `error` with `gc_view="unevaluated"` may be a failed ownership read, and
/// `not_owner` / `invalid_request` events can be emitted by any
/// meeting-controller token holder naming any handler. Even owner-checked
/// events carry a self-asserted `controller_id`, so "distinct MCs" is
/// spoofable by anyone who knows a live (meeting_id, controller_id) pair: why
/// automatic quarantine waits on authenticated per-instance MC identity
/// (docs/TODO.md). `caller_sub` is the validated service token's subject
/// (absent when no claims were injected). Today that is the SHARED MC client
/// id, identical for every MC replica: it proves the caller held a
/// meeting-controller credential but does NOT distinguish MC instances, so it
/// must never be used to count distinct MCs.
///
/// `trigger`, `gc_view` and `outcome` use the counter's `label()` strings, so
/// log and metric queries share tokens.
pub fn emit_report(report: &Report, outcome: ReplacementOutcome, caller_sub: Option<&str>) {
    let Some(handler_id) = report.replaced_handler_id.as_deref() else {
        return;
    };
    tracing::info!(
        target: "gc.mh.replacement_report",
        handler_id = %handler_id,
        webtransport_endpoint = report.webtransport_endpoint.as_deref(),
        controller_id = report.controller_id.as_deref(),
        caller_sub = caller_sub,
        trigger = report.trigger.label(),
        gc_view = report.gc_view.label(),
        outcome = outcome.label(),
        "MH replacement requested"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_trigger_is_total_over_release_cause() {
        assert_eq!(
            project_trigger(ReleaseCause::HeartbeatTimeout as i32),
            Some(ReplacementTrigger::HeartbeatTimeout)
        );
        assert_eq!(
            project_trigger(ReleaseCause::ApplyLag as i32),
            Some(ReplacementTrigger::ApplyLag)
        );
        assert_eq!(
            project_trigger(ReleaseCause::PlannedTransfer as i32),
            Some(ReplacementTrigger::PlannedTransfer)
        );
        assert_eq!(project_trigger(ReleaseCause::Unspecified as i32), None);
        assert_eq!(project_trigger(ReleaseCause::MeetingEnded as i32), None);
        assert_eq!(project_trigger(99), None);
        assert_eq!(project_trigger(-1), None);
    }

    #[test]
    fn classify_table() {
        let b = Duration::from_millis(3750);
        let fresh = Duration::from_millis(3749);
        let at_bound = b;
        assert_eq!(classify(None, b), GcView::Unregistered);
        for (status, view) in [
            (HealthStatus::Healthy, GcView::Healthy),
            (HealthStatus::Degraded, GcView::Degraded),
            (HealthStatus::Unhealthy, GcView::Unhealthy),
            (HealthStatus::Draining, GcView::Draining),
            (HealthStatus::Pending, GcView::Pending),
        ] {
            assert_eq!(classify(Some((status, fresh)), b), view, "{status:?} fresh");
            assert_eq!(classify(Some((status, Duration::ZERO)), b), view);
            // Stale wins over every status, including draining and unhealthy.
            assert_eq!(
                classify(Some((status, at_bound)), b),
                GcView::Stale,
                "{status:?} at the bound"
            );
        }
    }

    #[test]
    fn outcome_mapping() {
        let granted: Result<MhAssignmentInfo, Rejection> = Ok(MhAssignmentInfo {
            mh_id: "a".into(),
            webtransport_endpoint: "https://a:443".into(),
            grpc_endpoint: "grpc://a:50051".into(),
        });
        assert_eq!(outcome(&granted), ReplacementOutcome::Granted);
        assert_eq!(
            outcome(&Err(Rejection::InvalidRequest(
                tonic::Status::invalid_argument("x")
            ))),
            ReplacementOutcome::InvalidRequest
        );
        assert_eq!(
            outcome(&Err(Rejection::NotOwner)),
            ReplacementOutcome::NotOwner
        );
        assert_eq!(
            outcome(&Err(Rejection::NoCapacity)),
            ReplacementOutcome::NoCapacity
        );
        assert_eq!(
            outcome(&Err(Rejection::Error(GcError::Internal("x".into())))),
            ReplacementOutcome::Error
        );
    }
}
