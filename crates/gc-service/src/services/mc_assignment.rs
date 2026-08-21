//! Meeting Controller Assignment Service.
//!
//! Provides high-level business logic for assigning meetings to MCs
//! using the weighted round-robin algorithm from ADR-0010.
//!
//! # Architecture
//!
//! This service orchestrates the assignment flow:
//! 1. Check for existing healthy assignment
//! 2. If no healthy assignment, select candidate MC via load balancing
//! 3. Select MHs for the meeting via weighted load balancing
//! 4. Call MC via gRPC to notify of assignment (ADR-0010 Section 4a)
//! 5. On acceptance, atomic DB write
//! 6. On rejection, retry with different MC (max 3 attempts)
//!
//! # Security
//!
//! - Uses CSPRNG for weighted random selection
//! - All database operations use parameterized queries
//! - Error messages are generic to prevent information leakage

use crate::errors::GcError;
use crate::observability::metrics;
use crate::repositories::{weighted_random_select, McAssignment, MeetingAssignmentsRepository};
use crate::services::mc_client::{McAssignmentResult, McClientTrait, McRejectionReason};
use crate::services::mh_selection::{MhAssignmentInfo, MhSelection, MhSelectionService};
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Instant;
use tracing::instrument;

/// Maximum number of retry attempts for MC rejection per ADR-0010.
const MAX_MC_ASSIGNMENT_RETRIES: usize = 3;

/// True if GC's own MH selection is malformed — empty, or any handler carrying an
/// empty `grpc_endpoint`. These are exactly the two conditions MC rejects as
/// `INVALID_REQUEST`, so GC can verify that claim against its own request instead of
/// trusting the peer (see the `Rejected` arm of `assign_meeting_with_mh`).
///
/// For a correct GC over clean data this is always `false`:
/// `select_mhs_for_meeting` guarantees a non-empty selection and MH registration
/// validates `grpc_endpoint` non-empty. So a `true` result is a genuine GC-side /
/// data-integrity fault — the only case in which failing fast (rather than retrying
/// the pool) is the right call.
///
/// ANCHOR (DRY): these two conditions are the *producer-side mirror* of MC's
/// validator-side contract check in `store_mh_assignments`
/// (`crates/mc-service/src/grpc/mc_service.rs` — empty `mh_assignments`, empty
/// `grpc_endpoint` → `McError::InvalidArgument`). They are the same wire contract
/// (`AssignMeetingWithMhRequest` validity) on the two sides of the RPC; if MC's
/// definition of "malformed" changes, this predicate must change in lockstep, or GC
/// will retry-vs-fail-fast on the wrong criterion.
fn selection_is_malformed(handlers: &[MhAssignmentInfo]) -> bool {
    handlers.is_empty() || handlers.iter().any(|h| h.grpc_endpoint.is_empty())
}

/// Service for MC assignment operations.
pub struct McAssignmentService;

/// Result of an assignment with MH information.
#[derive(Debug, Clone)]
pub struct AssignmentWithMh {
    /// MC assignment info.
    pub mc_assignment: McAssignment,
    /// MH selection info (active/active peers).
    pub mh_selection: MhSelection,
}

impl McAssignmentService {
    /// End a meeting assignment.
    ///
    /// Called when a meeting ends normally. Marks the assignment as ended
    /// (soft delete) for audit trail.
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `meeting_id` - Meeting to end assignment for
    /// * `region` - Optional region (if None, ends all regional assignments)
    ///
    /// # Returns
    ///
    /// Number of assignments ended.
    #[instrument(skip_all, fields(meeting_id = %meeting_id))]
    pub async fn end_assignment(
        pool: &PgPool,
        meeting_id: &str,
        region: Option<&str>,
    ) -> Result<u64, GcError> {
        let count = MeetingAssignmentsRepository::end_assignment(pool, meeting_id, region).await?;

        if count > 0 {
            tracing::info!(
                target: "gc.service.assignment",
                meeting_id = %meeting_id,
                region = ?region,
                count = count,
                "Ended meeting assignment(s)"
            );
        }

        Ok(count)
    }

    /// Get existing assignment for a meeting.
    ///
    /// Returns the current healthy assignment if one exists.
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `meeting_id` - Meeting to look up
    /// * `region` - Region to look up in
    // Allow: Used in tests; will be used for future status endpoints
    #[allow(dead_code)]
    #[instrument(skip_all, fields(meeting_id = %meeting_id, region = %region))]
    pub async fn get_assignment(
        pool: &PgPool,
        meeting_id: &str,
        region: &str,
    ) -> Result<Option<McAssignment>, GcError> {
        MeetingAssignmentsRepository::get_healthy_assignment(pool, meeting_id, region).await
    }

    /// Assign a meeting with MH selection and MC notification (ADR-0010 Section 4a).
    ///
    /// This is the new assignment flow that:
    /// 1. Selects MHs for the meeting
    /// 2. Notifies MC via gRPC BEFORE writing to DB
    /// 3. Handles MC rejection with retry logic
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `mc_client` - Client for MC gRPC calls
    /// * `meeting_id` - Meeting to assign
    /// * `region` - Region to assign in
    /// * `gc_id` - ID of this GC instance
    ///
    /// # Returns
    ///
    /// Returns `AssignmentWithMh` with MC and MH assignments.
    ///
    /// # Errors
    ///
    /// - `GcError::ServiceUnavailable` - No healthy MCs/MHs or all MCs rejected
    /// - `GcError::Database` - Database operation failed
    #[instrument(skip_all, fields(meeting_id = %meeting_id, region = %region, gc_id = %gc_id))]
    pub async fn assign_meeting_with_mh(
        pool: &PgPool,
        mc_client: Arc<dyn McClientTrait>,
        meeting_id: &str,
        region: &str,
        gc_id: &str,
    ) -> Result<AssignmentWithMh, GcError> {
        // Start timing for metrics (ADR-0011)
        let start = Instant::now();

        // Step 1: Check for existing healthy assignment
        if let Some(existing) =
            MeetingAssignmentsRepository::get_healthy_assignment(pool, meeting_id, region).await?
        {
            tracing::debug!(
                target: "gc.service.assignment",
                meeting_id = %meeting_id,
                mc_id = %existing.mc_id,
                "Found existing healthy assignment"
            );

            // For existing assignments, we need to return MH info too
            // Select MHs (they may have changed since original assignment)
            let mh_selection = MhSelectionService::select_mhs_for_meeting(pool, region).await?;

            // Record success metrics for reusing existing assignment
            metrics::record_mc_assignment("success", None, start.elapsed());

            return Ok(AssignmentWithMh {
                mc_assignment: existing,
                mh_selection,
            });
        }

        // Step 2: Select MHs for the meeting
        let mh_selection = MhSelectionService::select_mhs_for_meeting(pool, region).await?;

        let mh_ids: Vec<&str> = mh_selection
            .handlers
            .iter()
            .map(|h| h.mh_id.as_str())
            .collect();
        tracing::debug!(
            target: "gc.service.assignment",
            meeting_id = %meeting_id,
            mh_ids = ?mh_ids,
            mh_count = mh_selection.handlers.len(),
            "Selected MHs for meeting"
        );

        // Step 3: Get candidate MCs and try assignment with retry
        let mut tried_mcs: Vec<String> = Vec::new();
        let mut last_rejection_reason: Option<McRejectionReason> = None;

        for attempt in 1..=MAX_MC_ASSIGNMENT_RETRIES {
            // Get candidate MCs, excluding ones we've already tried
            let mut candidates =
                MeetingAssignmentsRepository::get_candidate_mcs(pool, region).await?;

            // Filter out already-tried MCs
            candidates.retain(|c| !tried_mcs.contains(&c.controller_id));

            if candidates.is_empty() {
                tracing::warn!(
                    target: "gc.service.assignment",
                    meeting_id = %meeting_id,
                    region = %region,
                    attempt = attempt,
                    tried_mcs = ?tried_mcs,
                    "No more MCs available for assignment"
                );
                break;
            }

            // Select MC using weighted random
            let selected_mc = match weighted_random_select(&candidates) {
                Some(mc) => mc,
                None => break,
            };

            let mc_endpoint = selected_mc.grpc_endpoint.clone();
            let mc_id = selected_mc.controller_id.clone();

            tracing::debug!(
                target: "gc.service.assignment",
                meeting_id = %meeting_id,
                mc_id = %mc_id,
                attempt = attempt,
                "Attempting MC assignment"
            );

            // Step 4: Call MC to notify of assignment BEFORE writing to DB
            let result = mc_client
                .assign_meeting(&mc_endpoint, meeting_id, &mh_selection.handlers, gc_id)
                .await;

            match result {
                Ok(McAssignmentResult::Accepted) => {
                    tracing::info!(
                        target: "gc.service.assignment",
                        meeting_id = %meeting_id,
                        mc_id = %mc_id,
                        "MC accepted assignment"
                    );

                    // Step 5: MC accepted, now write to DB
                    let assignment = MeetingAssignmentsRepository::atomic_assign(
                        pool,
                        meeting_id,
                        region,
                        selected_mc,
                        gc_id,
                    )
                    .await?;

                    tracing::info!(
                        target: "gc.service.assignment",
                        meeting_id = %meeting_id,
                        mc_id = %assignment.mc_id,
                        region = %region,
                        "Meeting assigned to MC with MH"
                    );

                    // Record success metrics (ADR-0011)
                    metrics::record_mc_assignment("success", None, start.elapsed());

                    return Ok(AssignmentWithMh {
                        mc_assignment: assignment,
                        mh_selection,
                    });
                }
                Ok(McAssignmentResult::Rejected(reason)) => {
                    if reason == McRejectionReason::InvalidRequest {
                        // INVALID_REQUEST is a claim from a remote peer. Only act on it
                        // if GC's OWN request was actually malformed — never remove
                        // failover on the MC's word alone. GC has the ground truth in
                        // local scope (`mh_selection.handlers` is what it just sent), so
                        // it verifies rather than trusts: a buggy or rolled-back MC that
                        // returns `4` for a well-formed request must not be able to veto
                        // assignment for the whole pool.
                        if selection_is_malformed(&mh_selection.handlers) {
                            // GC genuinely sent bad data. Every MC rejects the identical
                            // request the same way, so retrying cannot succeed and would
                            // only burn the budget and falsely record a "sick fleet".
                            // Fail fast (adjudicated behaviour) — a GC contract violation.
                            last_rejection_reason = Some(McRejectionReason::InvalidRequest);
                            tracing::error!(
                                target: "gc.service.assignment",
                                meeting_id = %meeting_id,
                                mc_id = %mc_id,
                                attempt = attempt,
                                "MC rejected a genuinely malformed assignment request (GC contract violation); failing fast without retrying the pool"
                            );
                            break;
                        }
                        // GC's request was well-formed, so this MC is misreporting.
                        // Treat it as a controller problem and preserve failover: retry
                        // the rest of the pool rather than letting one peer veto assignment.
                        last_rejection_reason = Some(McRejectionReason::Unhealthy);
                        tracing::error!(
                            target: "gc.service.assignment",
                            meeting_id = %meeting_id,
                            mc_id = %mc_id,
                            attempt = attempt,
                            "MC claimed INVALID_REQUEST for a well-formed request; treating as unhealthy and retrying the pool"
                        );
                        tried_mcs.push(mc_id);
                        continue;
                    }

                    last_rejection_reason = Some(reason);
                    tracing::warn!(
                        target: "gc.service.assignment",
                        meeting_id = %meeting_id,
                        mc_id = %mc_id,
                        rejection_reason = ?reason,
                        attempt = attempt,
                        "MC rejected assignment, will retry"
                    );
                    tried_mcs.push(mc_id);
                    // Continue to next attempt
                }
                Err(e) => {
                    tracing::warn!(
                        target: "gc.service.assignment",
                        meeting_id = %meeting_id,
                        mc_id = %mc_id,
                        error = %e,
                        attempt = attempt,
                        "MC RPC failed, will retry"
                    );
                    tried_mcs.push(mc_id);
                    // Continue to next attempt
                }
            }
        }

        // Loop ended (retries exhausted, pool drained, or fail-fast break) -
        // record the outcome metric. Exactly one emit for every failure path,
        // including the invalid_request fail-fast (which breaks here rather than
        // emitting inside the loop).
        //
        // ANCHOR (DRY): this match is the source of truth for
        // gc_mc_assignments_total{rejection_reason} label values. Mirrors — edit in
        // lockstep (prose AND code):
        //   docs/observability/metrics/gc-service.md (label list :74 + cardinality table)
        //   docs/observability/alerts.md (GCMCAssignmentFailures response)
        //   docs/runbooks/gc-incident-response.md (legend)
        //   infra/docker/prometheus/rules/gc-alerts.yaml (GCMCAssignmentFailures
        //     description — illustrative triage prose, not a complete enumeration)
        //   crates/gc-service/tests/mc_assignment_metrics_integration.rs (ALL_REJECTION_REASONS)
        //   crates/gc-service/src/observability/metrics.rs (metric-cluster test array)
        let (status, rejection_reason) = match last_rejection_reason {
            Some(McRejectionReason::AtCapacity) => ("rejected", Some("at_capacity")),
            Some(McRejectionReason::Draining) => ("rejected", Some("draining")),
            Some(McRejectionReason::Unhealthy) => ("rejected", Some("unhealthy")),
            Some(McRejectionReason::Unspecified) => ("rejected", Some("unspecified")),
            // GC contract violation surfaced by MC — a GC fault, not a rejection
            // of a healthy request, so status="error" (groups with no_mcs_available).
            Some(McRejectionReason::InvalidRequest) => ("error", Some("invalid_request")),
            None => ("error", Some("no_mcs_available")),
        };
        metrics::record_mc_assignment(status, rejection_reason, start.elapsed());

        // Generic, bounded static (log field only — errors.rs discards the payload
        // and returns a fixed client body; no meeting_id/mh_id/endpoint here).
        // Exhaustive (no catch-all), mirroring the metric match above so a future
        // RejectionReason variant compile-fails here too, not just at the metric match.
        let reason_str = match last_rejection_reason {
            Some(McRejectionReason::AtCapacity) => "All meeting controllers are at capacity",
            Some(McRejectionReason::Draining) => "All meeting controllers are draining",
            Some(McRejectionReason::Unhealthy) => "All meeting controllers are unhealthy",
            Some(McRejectionReason::InvalidRequest) => {
                "Meeting assignment request was invalid (GC contract violation)"
            }
            // A controller responded with a reason GC doesn't recognize (not "no MCs").
            Some(McRejectionReason::Unspecified) => {
                "A meeting controller rejected the assignment for an unrecognized reason"
            }
            None => "No meeting controllers available",
        };

        // Fail-fast on a GC contract violation: return an internal (500) error, not
        // ServiceUnavailable (503). A GC-computed bad request is a server defect, not
        // transient unavailability; 503 would invite a retry that recomputes the same
        // bad request. The in-loop error! already logged the cause; the terminal log
        // here must NOT claim "after N attempts" (only one MC was tried).
        if matches!(
            last_rejection_reason,
            Some(McRejectionReason::InvalidRequest)
        ) {
            tracing::error!(
                target: "gc.service.assignment",
                meeting_id = %meeting_id,
                region = %region,
                "Failed to assign meeting: GC sent an invalid assignment request (contract violation); not retried"
            );
            return Err(GcError::Internal(reason_str.to_string()));
        }

        tracing::error!(
            target: "gc.service.assignment",
            meeting_id = %meeting_id,
            region = %region,
            tried_mcs = ?tried_mcs,
            "Failed to assign meeting after {} attempts",
            MAX_MC_ASSIGNMENT_RETRIES
        );

        Err(GcError::ServiceUnavailable(reason_str.to_string()))
    }
}

#[cfg(test)]
mod tests {
    // Loop-level behaviour (fail-fast, failover, metrics) is covered by the
    // #[sqlx::test] integration tests in tests/mc_assignment_rpc_tests.rs.
    use super::*;

    fn handler(grpc_endpoint: &str) -> MhAssignmentInfo {
        MhAssignmentInfo {
            mh_id: "mh-1".to_string(),
            webtransport_endpoint: "https://mh:443".to_string(),
            grpc_endpoint: grpc_endpoint.to_string(),
        }
    }

    #[test]
    fn selection_is_malformed_flags_empty_selection() {
        assert!(selection_is_malformed(&[]));
    }

    #[test]
    fn selection_is_malformed_flags_empty_grpc_endpoint() {
        assert!(selection_is_malformed(&[handler("")]));
        // Malformed even if only one of several handlers is bad.
        assert!(selection_is_malformed(&[
            handler("grpc://mh-1:50051"),
            handler(""),
        ]));
    }

    #[test]
    fn selection_is_malformed_accepts_well_formed_selection() {
        assert!(!selection_is_malformed(&[handler("grpc://mh-1:50051")]));
        assert!(!selection_is_malformed(&[
            handler("grpc://mh-1:50051"),
            handler("grpc://mh-2:50051"),
        ]));
    }
}
