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

// Allow dead code for new assignment flow - will be wired into handlers in future phase.
#![allow(dead_code)]

use crate::errors::GcError;
use crate::repositories::{weighted_random_select, McAssignment, MeetingAssignmentsRepository};
use crate::services::mc_client::{McAssignmentResult, McClientTrait, McRejectionReason};
use crate::services::mh_selection::{MhSelection, MhSelectionService};
use sqlx::PgPool;
use std::sync::Arc;
use tracing::instrument;

/// Maximum number of retry attempts for MC rejection per ADR-0010.
const MAX_MC_ASSIGNMENT_RETRIES: usize = 3;

/// Service for MC assignment operations.
pub struct McAssignmentService;

/// Result of an assignment with MH information.
#[derive(Debug, Clone)]
pub struct AssignmentWithMh {
    /// MC assignment info.
    pub mc_assignment: McAssignment,
    /// MH selection info (primary + optional backup).
    pub mh_selection: MhSelection,
}

impl McAssignmentService {
    /// Assign a meeting to a meeting controller in a region.
    ///
    /// This implements the ADR-0010 assignment flow:
    /// 1. Check for existing healthy assignment (return immediately if found)
    /// 2. Get candidate MCs via load balancing query
    /// 3. Select MC using weighted random (prefers lower load)
    /// 4. Atomic assignment with race condition handling
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `meeting_id` - Meeting to assign
    /// * `region` - Region to assign in
    /// * `gc_id` - ID of this GC instance (for auditing)
    ///
    /// # Returns
    ///
    /// Returns `McAssignment` with the assigned MC's endpoints.
    ///
    /// # Errors
    ///
    /// - `GcError::ServiceUnavailable` - No healthy MCs available
    /// - `GcError::Database` - Database operation failed
    #[instrument(skip_all, fields(meeting_id = %meeting_id, region = %region, gc_id = %gc_id))]
    pub async fn assign_meeting(
        pool: &PgPool,
        meeting_id: &str,
        region: &str,
        gc_id: &str,
    ) -> Result<McAssignment, GcError> {
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
            return Ok(existing);
        }

        // Step 2: Get candidate MCs via load balancing
        let candidates = MeetingAssignmentsRepository::get_candidate_mcs(pool, region).await?;

        if candidates.is_empty() {
            tracing::warn!(
                target: "gc.service.assignment",
                meeting_id = %meeting_id,
                region = %region,
                "No healthy MCs available for assignment"
            );
            return Err(GcError::ServiceUnavailable(
                "No meeting controllers available in this region".to_string(),
            ));
        }

        tracing::debug!(
            target: "gc.service.assignment",
            meeting_id = %meeting_id,
            region = %region,
            candidate_count = candidates.len(),
            "Found candidate MCs for assignment"
        );

        // Step 3: Select MC using weighted random
        let selected_mc = weighted_random_select(&candidates).ok_or_else(|| {
            // This shouldn't happen since we checked candidates.is_empty()
            GcError::ServiceUnavailable(
                "No meeting controllers available in this region".to_string(),
            )
        })?;

        tracing::debug!(
            target: "gc.service.assignment",
            meeting_id = %meeting_id,
            mc_id = %selected_mc.controller_id,
            load_ratio = selected_mc.load_ratio,
            "Selected MC for assignment"
        );

        // Step 4: Atomic assignment with race condition handling
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
            "Meeting assigned to MC"
        );

        Ok(assignment)
    }

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
    pub async fn assign_meeting_with_mh<C: McClientTrait>(
        pool: &PgPool,
        mc_client: Arc<C>,
        meeting_id: &str,
        region: &str,
        gc_id: &str,
    ) -> Result<AssignmentWithMh, GcError> {
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

            return Ok(AssignmentWithMh {
                mc_assignment: existing,
                mh_selection,
            });
        }

        // Step 2: Select MHs for the meeting
        let mh_selection = MhSelectionService::select_mhs_for_meeting(pool, region).await?;

        tracing::debug!(
            target: "gc.service.assignment",
            meeting_id = %meeting_id,
            primary_mh = %mh_selection.primary.mh_id,
            backup_mh = mh_selection.backup.as_ref().map(|b| b.mh_id.as_str()),
            "Selected MHs for meeting"
        );

        // Build MH assignments list
        let mut mh_assignments = vec![mh_selection.primary.clone()];
        if let Some(backup) = &mh_selection.backup {
            mh_assignments.push(backup.clone());
        }

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
                .assign_meeting(&mc_endpoint, meeting_id, &mh_assignments, gc_id)
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

                    return Ok(AssignmentWithMh {
                        mc_assignment: assignment,
                        mh_selection,
                    });
                }
                Ok(McAssignmentResult::Rejected(reason)) => {
                    tracing::warn!(
                        target: "gc.service.assignment",
                        meeting_id = %meeting_id,
                        mc_id = %mc_id,
                        rejection_reason = ?reason,
                        attempt = attempt,
                        "MC rejected assignment, will retry"
                    );
                    tried_mcs.push(mc_id);
                    last_rejection_reason = Some(reason);
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

        // All retries exhausted
        let reason_str = match last_rejection_reason {
            Some(McRejectionReason::AtCapacity) => "All meeting controllers are at capacity",
            Some(McRejectionReason::Draining) => "All meeting controllers are draining",
            Some(McRejectionReason::Unhealthy) => "All meeting controllers are unhealthy",
            _ => "No meeting controllers available",
        };

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
    // Integration tests are in the tests/ directory
    // since they require database access via #[sqlx::test]
}
