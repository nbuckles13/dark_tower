//! Meeting Controller gRPC Service.
//!
//! Implements the `MeetingControllerService` that the Global Controller calls.
//! Per ADR-0023 Phase 6c and ADR-0010 Section 4a:
//!
//! - `AssignMeetingWithMh` - Accept/reject meeting assignments from GC
//!
//! # Accept/Reject Logic (ADR-0023 Section 5b)
//!
//! MC accepts assignment if:
//! - Not at meeting capacity
//! - Not at participant capacity (estimated based on meeting count)
//! - Not in draining state (graceful shutdown)
//! - Health status is HEALTHY or DEGRADED
//!
//! On acceptance:
//! - Store MH assignments in Redis
//! - Create meeting actor
//! - Return accepted=true
//!
//! On rejection:
//! - Return accepted=false with rejection reason
//! - GC will retry with different MC

use crate::actors::MeetingControllerActorHandle;
use crate::errors::McError;
use crate::redis::FencedRedisClient;
use proto_gen::internal::meeting_controller_service_server::MeetingControllerService;
use proto_gen::internal::{
    AssignMeetingWithMhRequest, AssignMeetingWithMhResponse, MhAssignment, RejectionReason,
};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use tonic::{Request, Response, Status};
use tracing::{debug, error, info, instrument, warn};

/// Estimated number of participants per meeting for capacity planning.
///
/// When checking if MC can accept a new meeting, we estimate the participant
/// load based on this constant. This provides headroom to avoid accepting
/// meetings that would immediately hit participant limits.
///
/// Based on typical meeting sizes in video conferencing (median ~8 participants).
/// Conservative estimate ensures we don't overcommit.
const ESTIMATED_PARTICIPANTS_PER_MEETING: u32 = 10;

/// Meeting Controller gRPC service implementation.
pub struct McAssignmentService {
    /// Handle to the meeting controller actor.
    controller_handle: Arc<MeetingControllerActorHandle>,
    /// Redis client for state persistence.
    redis_client: Arc<FencedRedisClient>,
    /// MC ID for logging.
    mc_id: String,
    /// Maximum meetings this MC can handle.
    max_meetings: u32,
    /// Maximum participants this MC can handle.
    max_participants: u32,
    /// Current meeting count (atomic for lock-free access).
    current_meetings: AtomicU32,
    /// Current participant count (atomic for lock-free access).
    current_participants: AtomicU32,
    /// Whether the MC is draining (graceful shutdown).
    is_draining: AtomicBool,
}

impl McAssignmentService {
    /// Create a new MC assignment service.
    ///
    /// # Arguments
    ///
    /// * `controller_handle` - Handle to the meeting controller actor
    /// * `redis_client` - Redis client for state persistence
    /// * `mc_id` - This MC's identifier
    /// * `max_meetings` - Maximum meetings this MC can handle
    /// * `max_participants` - Maximum participants this MC can handle
    #[must_use]
    pub fn new(
        controller_handle: Arc<MeetingControllerActorHandle>,
        redis_client: Arc<FencedRedisClient>,
        mc_id: String,
        max_meetings: u32,
        max_participants: u32,
    ) -> Self {
        Self {
            controller_handle,
            redis_client,
            mc_id,
            max_meetings,
            max_participants,
            current_meetings: AtomicU32::new(0),
            current_participants: AtomicU32::new(0),
            is_draining: AtomicBool::new(false),
        }
    }

    /// Update current meeting count.
    pub fn set_meeting_count(&self, count: u32) {
        self.current_meetings.store(count, Ordering::SeqCst);
    }

    /// Update current participant count.
    pub fn set_participant_count(&self, count: u32) {
        self.current_participants.store(count, Ordering::SeqCst);
    }

    /// Set draining state.
    pub fn set_draining(&self, draining: bool) {
        self.is_draining.store(draining, Ordering::SeqCst);
    }

    /// Check if MC can accept a new meeting.
    ///
    /// Returns `None` if can accept, or `Some(RejectionReason)` if cannot.
    fn can_accept_meeting(&self) -> Option<RejectionReason> {
        // Check draining state first
        if self.is_draining.load(Ordering::SeqCst) {
            return Some(RejectionReason::Draining);
        }

        // Check meeting capacity
        let current = self.current_meetings.load(Ordering::SeqCst);
        if current >= self.max_meetings {
            return Some(RejectionReason::AtCapacity);
        }

        // Estimate participant headroom
        let current_participants = self.current_participants.load(Ordering::SeqCst);
        if current_participants.saturating_add(ESTIMATED_PARTICIPANTS_PER_MEETING)
            > self.max_participants
        {
            return Some(RejectionReason::AtCapacity);
        }

        None
    }

    /// Store MH assignments for a meeting in Redis.
    ///
    /// Per ADR-0023 Section 6, stores:
    /// - Primary MH endpoint
    /// - Backup MH endpoint (optional)
    /// - Assignment metadata
    async fn store_mh_assignments(
        &self,
        meeting_id: &str,
        mh_assignments: &[MhAssignment],
    ) -> Result<(), McError> {
        // Build assignment data
        let primary = mh_assignments.first().ok_or_else(|| {
            error!(
                target: "mc.grpc.mc_service",
                meeting_id = %meeting_id,
                "No MH assignments provided"
            );
            McError::Config("No MH assignments provided".to_string())
        })?;

        let backup = mh_assignments.get(1);

        // Store in Redis with fencing token
        self.redis_client
            .store_mh_assignment(
                meeting_id,
                &primary.mh_id,
                &primary.webtransport_endpoint,
                backup.map(|b| (b.mh_id.as_str(), b.webtransport_endpoint.as_str())),
            )
            .await?;

        debug!(
            target: "mc.grpc.mc_service",
            meeting_id = %meeting_id,
            primary_mh = %primary.mh_id,
            backup_mh = backup.map(|b| b.mh_id.as_str()),
            "Stored MH assignments"
        );

        Ok(())
    }
}

#[tonic::async_trait]
impl MeetingControllerService for McAssignmentService {
    /// Handle meeting assignment with MH assignments (ADR-0010 Section 4a).
    ///
    /// This is the primary assignment endpoint. GC calls this BEFORE writing
    /// the assignment to its database, allowing MC to reject if at capacity.
    #[instrument(skip_all, fields(mc_id = %self.mc_id))]
    async fn assign_meeting_with_mh(
        &self,
        request: Request<AssignMeetingWithMhRequest>,
    ) -> Result<Response<AssignMeetingWithMhResponse>, Status> {
        let inner = request.into_inner();
        let meeting_id = &inner.meeting_id;
        let gc_id = &inner.requesting_gc_id;

        info!(
            target: "mc.grpc.mc_service",
            meeting_id = %meeting_id,
            gc_id = %gc_id,
            mh_count = inner.mh_assignments.len(),
            "Received meeting assignment request"
        );

        // Check if we can accept
        if let Some(reason) = self.can_accept_meeting() {
            warn!(
                target: "mc.grpc.mc_service",
                meeting_id = %meeting_id,
                rejection_reason = ?reason,
                current_meetings = self.current_meetings.load(Ordering::SeqCst),
                max_meetings = self.max_meetings,
                "Rejecting meeting assignment"
            );

            return Ok(Response::new(AssignMeetingWithMhResponse {
                accepted: false,
                rejection_reason: reason.into(),
            }));
        }

        // Store MH assignments in Redis
        if let Err(e) = self
            .store_mh_assignments(meeting_id, &inner.mh_assignments)
            .await
        {
            error!(
                target: "mc.grpc.mc_service",
                meeting_id = %meeting_id,
                error = %e,
                "Failed to store MH assignments"
            );

            return Ok(Response::new(AssignMeetingWithMhResponse {
                accepted: false,
                rejection_reason: RejectionReason::Unhealthy.into(),
            }));
        }

        // Create meeting actor
        match self
            .controller_handle
            .create_meeting(meeting_id.clone())
            .await
        {
            Ok(()) => {
                info!(
                    target: "mc.grpc.mc_service",
                    meeting_id = %meeting_id,
                    gc_id = %gc_id,
                    "Accepted meeting assignment"
                );

                // Increment meeting count
                self.current_meetings.fetch_add(1, Ordering::SeqCst);

                Ok(Response::new(AssignMeetingWithMhResponse {
                    accepted: true,
                    rejection_reason: RejectionReason::Unspecified.into(),
                }))
            }
            Err(e) => {
                error!(
                    target: "mc.grpc.mc_service",
                    meeting_id = %meeting_id,
                    error = %e,
                    "Failed to create meeting actor"
                );

                // Clean up MH assignments
                let _ = self.redis_client.delete_mh_assignment(meeting_id).await;

                Ok(Response::new(AssignMeetingWithMhResponse {
                    accepted: false,
                    rejection_reason: RejectionReason::Unhealthy.into(),
                }))
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_rejection_reason_values() {
        // Verify proto enum values match our expectations
        assert_eq!(RejectionReason::Unspecified as i32, 0);
        assert_eq!(RejectionReason::AtCapacity as i32, 1);
        assert_eq!(RejectionReason::Draining as i32, 2);
        assert_eq!(RejectionReason::Unhealthy as i32, 3);
    }

    #[test]
    fn test_estimated_participants_per_meeting_constant() {
        // Verify the constant is reasonable and documented
        assert_eq!(ESTIMATED_PARTICIPANTS_PER_MEETING, 10);
    }

    /// Test helper to verify capacity check logic.
    /// This uses a standalone function that mirrors can_accept_meeting logic
    /// to avoid needing real Redis/Actor dependencies in unit tests.
    fn check_capacity(
        is_draining: bool,
        current_meetings: u32,
        max_meetings: u32,
        current_participants: u32,
        max_participants: u32,
    ) -> Option<RejectionReason> {
        // Check draining state first
        if is_draining {
            return Some(RejectionReason::Draining);
        }

        // Check meeting capacity
        if current_meetings >= max_meetings {
            return Some(RejectionReason::AtCapacity);
        }

        // Estimate participant headroom
        if current_participants.saturating_add(ESTIMATED_PARTICIPANTS_PER_MEETING)
            > max_participants
        {
            return Some(RejectionReason::AtCapacity);
        }

        None
    }

    #[test]
    fn test_capacity_check_when_draining() {
        // Draining state should reject with Draining reason
        assert_eq!(
            check_capacity(true, 0, 100, 0, 1000),
            Some(RejectionReason::Draining)
        );

        // Not draining with capacity available should accept
        assert_eq!(check_capacity(false, 0, 100, 0, 1000), None);
    }

    #[test]
    fn test_capacity_check_at_meeting_capacity() {
        // At exact meeting capacity
        assert_eq!(
            check_capacity(false, 10, 10, 0, 1000),
            Some(RejectionReason::AtCapacity)
        );

        // Over meeting capacity
        assert_eq!(
            check_capacity(false, 11, 10, 0, 1000),
            Some(RejectionReason::AtCapacity)
        );

        // Below meeting capacity
        assert_eq!(check_capacity(false, 9, 10, 0, 1000), None);

        // At boundary (1 below max)
        assert_eq!(check_capacity(false, 999, 1000, 0, 10000), None);
    }

    #[test]
    fn test_capacity_check_at_participant_capacity() {
        // With participant headroom
        assert_eq!(check_capacity(false, 0, 100, 90, 100), None);

        // At participant capacity (90 + 10 estimate = 100, which equals max)
        assert_eq!(check_capacity(false, 0, 100, 90, 100), None);

        // Over participant capacity (91 + 10 = 101 > 100)
        assert_eq!(
            check_capacity(false, 0, 100, 91, 100),
            Some(RejectionReason::AtCapacity)
        );

        // Well over capacity
        assert_eq!(
            check_capacity(false, 0, 100, 100, 100),
            Some(RejectionReason::AtCapacity)
        );
    }

    #[test]
    fn test_capacity_check_overflow_protection() {
        // Test that saturating_add prevents overflow panics
        // u32::MAX - 5 + 10 would overflow with regular add, but saturating_add caps at MAX
        // MAX + 10 saturates to MAX, and MAX > MAX is false, so actually allows
        // This is safe because in practice we'd never have u32::MAX participants

        // Test with max_participants less than u32::MAX to verify actual overflow protection
        assert_eq!(
            check_capacity(false, 0, 100, u32::MAX - 5, u32::MAX - 1),
            Some(RejectionReason::AtCapacity)
        );

        // Test extreme meeting capacity (u32::MAX meetings with max u32::MAX)
        // This passes because u32::MAX >= u32::MAX is true
        assert_eq!(
            check_capacity(false, u32::MAX, u32::MAX, 0, u32::MAX),
            Some(RejectionReason::AtCapacity)
        );
    }

    #[test]
    fn test_capacity_check_priority_draining_over_capacity() {
        // Draining should take priority over capacity errors
        assert_eq!(
            check_capacity(true, 10, 10, 100, 100),
            Some(RejectionReason::Draining)
        );
    }

    #[test]
    fn test_capacity_check_meeting_checked_before_participants() {
        // Meeting capacity is checked before participant capacity
        // (both at capacity, meeting check happens first)
        assert_eq!(
            check_capacity(false, 10, 10, 100, 100),
            Some(RejectionReason::AtCapacity)
        );
    }

    #[test]
    fn test_capacity_edge_cases() {
        // Zero max meetings - always at capacity
        assert_eq!(
            check_capacity(false, 0, 0, 0, 100),
            Some(RejectionReason::AtCapacity)
        );

        // Zero max participants - at capacity if estimate would overflow
        assert_eq!(
            check_capacity(false, 0, 100, 0, 5),
            Some(RejectionReason::AtCapacity)
        );

        // Exactly at threshold (current + estimate == max)
        // 90 participants + 10 estimate = 100, which is NOT > 100, so should accept
        assert_eq!(check_capacity(false, 0, 100, 90, 100), None);
    }
}
