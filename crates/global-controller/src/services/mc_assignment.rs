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
//! 3. Atomic operation to prevent race conditions
//!
//! # Security
//!
//! - Uses CSPRNG for weighted random selection
//! - All database operations use parameterized queries
//! - Error messages are generic to prevent information leakage

use crate::errors::GcError;
use crate::repositories::{weighted_random_select, McAssignment, MeetingAssignmentsRepository};
use sqlx::PgPool;
use tracing::instrument;

/// Service for MC assignment operations.
pub struct McAssignmentService;

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
    #[instrument(skip(pool), fields(meeting_id = %meeting_id, region = %region, gc_id = %gc_id))]
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
    #[instrument(skip(pool), fields(meeting_id = %meeting_id))]
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
    #[instrument(skip(pool), fields(meeting_id = %meeting_id, region = %region))]
    pub async fn get_assignment(
        pool: &PgPool,
        meeting_id: &str,
        region: &str,
    ) -> Result<Option<McAssignment>, GcError> {
        MeetingAssignmentsRepository::get_healthy_assignment(pool, meeting_id, region).await
    }
}

#[cfg(test)]
mod tests {
    // Integration tests are in the tests/ directory
    // since they require database access via #[sqlx::test]
}
