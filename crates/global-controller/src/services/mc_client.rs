//! Meeting Controller gRPC Client.
//!
//! Provides a client for GC→MC communication per ADR-0010 Section 4a.
//! Includes connection pooling via tonic Channel caching.
//!
//! # Security
//!
//! - Service tokens authenticate GC to MC
//! - Connection reuse via channel pooling
//! - Timeouts prevent hanging connections
//! - Error messages are generic to prevent information leakage

// Allow dead code during incremental development - will be wired into handlers
// in a future phase.
#![allow(dead_code)]

use crate::errors::GcError;
use crate::services::mh_selection::MhAssignmentInfo;
use common::secret::{ExposeSecret, SecretString};
use proto_gen::internal::meeting_controller_service_client::MeetingControllerServiceClient;
use proto_gen::internal::{
    AssignMeetingWithMhRequest, AssignMeetingWithMhResponse, MhAssignment, MhRole,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tonic::transport::{Channel, Endpoint};
use tonic::Request;
use tracing::{error, instrument, warn};

/// Default timeout for MC RPC calls in seconds.
const MC_RPC_TIMEOUT_SECS: u64 = 10;

/// Default connect timeout in seconds.
const MC_CONNECT_TIMEOUT_SECS: u64 = 5;

/// Reason for MC rejecting an assignment (mirrors proto enum).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McRejectionReason {
    /// Unknown/unspecified reason.
    Unspecified,
    /// MC is at capacity.
    AtCapacity,
    /// MC is draining (graceful shutdown).
    Draining,
    /// MC is unhealthy.
    Unhealthy,
}

impl From<i32> for McRejectionReason {
    fn from(value: i32) -> Self {
        match value {
            1 => McRejectionReason::AtCapacity,
            2 => McRejectionReason::Draining,
            3 => McRejectionReason::Unhealthy,
            _ => McRejectionReason::Unspecified,
        }
    }
}

/// Result of an MC assignment attempt.
#[derive(Debug)]
pub enum McAssignmentResult {
    /// MC accepted the assignment.
    Accepted,
    /// MC rejected the assignment.
    Rejected(McRejectionReason),
}

/// MC client with connection pooling.
///
/// Maintains a cache of gRPC channels to avoid connection churn.
pub struct McClient {
    /// Cached channels by endpoint.
    channels: Arc<RwLock<HashMap<String, Channel>>>,
    /// Service token for authenticating to MCs (protected by SecretString).
    service_token: SecretString,
}

impl McClient {
    /// Create a new MC client.
    ///
    /// # Arguments
    ///
    /// * `service_token` - GC's service token for authenticating to MCs
    pub fn new(service_token: SecretString) -> Self {
        Self {
            channels: Arc::new(RwLock::new(HashMap::new())),
            service_token,
        }
    }

    /// Get or create a channel to an MC endpoint.
    async fn get_channel(&self, endpoint: &str) -> Result<Channel, GcError> {
        // Check cache first
        {
            let channels = self.channels.read().await;
            if let Some(channel) = channels.get(endpoint) {
                return Ok(channel.clone());
            }
        }

        // Create new channel
        let channel = Endpoint::from_shared(endpoint.to_string())
            .map_err(|e| {
                error!(target: "gc.services.mc_client", error = %e, endpoint = %endpoint, "Invalid MC endpoint");
                GcError::ServiceUnavailable("Invalid meeting controller endpoint".to_string())
            })?
            .connect_timeout(Duration::from_secs(MC_CONNECT_TIMEOUT_SECS))
            .timeout(Duration::from_secs(MC_RPC_TIMEOUT_SECS))
            .connect()
            .await
            .map_err(|e| {
                warn!(target: "gc.services.mc_client", error = %e, endpoint = %endpoint, "Failed to connect to MC");
                GcError::ServiceUnavailable("Meeting controller unavailable".to_string())
            })?;

        // Cache the channel
        {
            let mut channels = self.channels.write().await;
            channels.insert(endpoint.to_string(), channel.clone());
        }

        Ok(channel)
    }

    /// Assign a meeting to an MC with MH assignments.
    ///
    /// # Arguments
    ///
    /// * `mc_endpoint` - gRPC endpoint of the MC
    /// * `meeting_id` - Meeting being assigned
    /// * `mh_assignments` - List of MH assignments (primary + backup)
    /// * `gc_id` - ID of this GC instance
    ///
    /// # Returns
    ///
    /// Returns `McAssignmentResult::Accepted` if MC accepted, or
    /// `McAssignmentResult::Rejected` with the rejection reason.
    ///
    /// # Errors
    ///
    /// - `GcError::ServiceUnavailable` - MC unreachable or connection failed
    /// - `GcError::Internal` - Unexpected error
    #[instrument(skip(self, mh_assignments), fields(mc_endpoint = %mc_endpoint, meeting_id = %meeting_id, gc_id = %gc_id))]
    pub async fn assign_meeting(
        &self,
        mc_endpoint: &str,
        meeting_id: &str,
        mh_assignments: &[MhAssignmentInfo],
        gc_id: &str,
    ) -> Result<McAssignmentResult, GcError> {
        let channel = self.get_channel(mc_endpoint).await?;

        // Build the request
        let proto_assignments: Vec<MhAssignment> = mh_assignments
            .iter()
            .enumerate()
            .map(|(i, mh)| MhAssignment {
                mh_id: mh.mh_id.clone(),
                webtransport_endpoint: mh.webtransport_endpoint.clone(),
                role: if i == 0 {
                    MhRole::Primary as i32
                } else {
                    MhRole::Backup as i32
                },
            })
            .collect();

        let request = AssignMeetingWithMhRequest {
            meeting_id: meeting_id.to_string(),
            mh_assignments: proto_assignments,
            requesting_gc_id: gc_id.to_string(),
        };

        // Add authorization header (token accessed via ExposeSecret)
        let mut grpc_request = Request::new(request);
        grpc_request.metadata_mut().insert(
            "authorization",
            format!("Bearer {}", self.service_token.expose_secret())
                .parse()
                .map_err(|_| {
                    error!(target: "gc.services.mc_client", "Invalid service token format");
                    GcError::Internal
                })?,
        );

        // Make the RPC call
        let mut client = MeetingControllerServiceClient::new(channel);
        let response = client
            .assign_meeting_with_mh(grpc_request)
            .await
            .map_err(|e| {
                warn!(target: "gc.services.mc_client", error = %e, mc_endpoint = %mc_endpoint, "MC RPC failed");
                GcError::ServiceUnavailable("Meeting controller unavailable".to_string())
            })?;

        let inner: AssignMeetingWithMhResponse = response.into_inner();

        if inner.accepted {
            tracing::info!(
                target: "gc.services.mc_client",
                meeting_id = %meeting_id,
                mc_endpoint = %mc_endpoint,
                "MC accepted meeting assignment"
            );
            Ok(McAssignmentResult::Accepted)
        } else {
            let reason = McRejectionReason::from(inner.rejection_reason);
            tracing::warn!(
                target: "gc.services.mc_client",
                meeting_id = %meeting_id,
                mc_endpoint = %mc_endpoint,
                rejection_reason = ?reason,
                "MC rejected meeting assignment"
            );
            Ok(McAssignmentResult::Rejected(reason))
        }
    }
}

/// Trait for MC client operations (enables mocking).
#[allow(dead_code)] // Used by mock implementation
#[async_trait::async_trait]
pub trait McClientTrait: Send + Sync {
    /// Assign a meeting to an MC.
    async fn assign_meeting(
        &self,
        mc_endpoint: &str,
        meeting_id: &str,
        mh_assignments: &[MhAssignmentInfo],
        gc_id: &str,
    ) -> Result<McAssignmentResult, GcError>;
}

#[async_trait::async_trait]
impl McClientTrait for McClient {
    async fn assign_meeting(
        &self,
        mc_endpoint: &str,
        meeting_id: &str,
        mh_assignments: &[MhAssignmentInfo],
        gc_id: &str,
    ) -> Result<McAssignmentResult, GcError> {
        self.assign_meeting(mc_endpoint, meeting_id, mh_assignments, gc_id)
            .await
    }
}

/// Mock MC client module for testing.
///
/// This module provides mock implementations of the MC client for use in tests.
pub mod mock {

    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Mock MC client for unit testing.
    pub struct MockMcClient {
        /// Responses to return (cycles through them).
        responses: Vec<McAssignmentResult>,
        /// Number of calls made.
        call_count: AtomicUsize,
        /// Whether to return errors.
        return_error: bool,
    }

    impl MockMcClient {
        /// Create a mock that always accepts.
        pub fn accepting() -> Self {
            Self {
                responses: vec![McAssignmentResult::Accepted],
                call_count: AtomicUsize::new(0),
                return_error: false,
            }
        }

        /// Create a mock that always rejects with a reason.
        pub fn rejecting(reason: McRejectionReason) -> Self {
            Self {
                responses: vec![McAssignmentResult::Rejected(reason)],
                call_count: AtomicUsize::new(0),
                return_error: false,
            }
        }

        /// Create a mock that returns custom responses in sequence.
        pub fn with_responses(responses: Vec<McAssignmentResult>) -> Self {
            Self {
                responses,
                call_count: AtomicUsize::new(0),
                return_error: false,
            }
        }

        /// Create a mock that returns errors.
        pub fn failing() -> Self {
            Self {
                responses: vec![],
                call_count: AtomicUsize::new(0),
                return_error: true,
            }
        }

        /// Get the number of calls made.
        pub fn call_count(&self) -> usize {
            self.call_count.load(Ordering::SeqCst)
        }
    }

    #[async_trait::async_trait]
    impl McClientTrait for MockMcClient {
        async fn assign_meeting(
            &self,
            _mc_endpoint: &str,
            _meeting_id: &str,
            _mh_assignments: &[MhAssignmentInfo],
            _gc_id: &str,
        ) -> Result<McAssignmentResult, GcError> {
            let count = self.call_count.fetch_add(1, Ordering::SeqCst);

            if self.return_error {
                return Err(GcError::ServiceUnavailable(
                    "Mock MC client error".to_string(),
                ));
            }

            if self.responses.is_empty() {
                return Ok(McAssignmentResult::Accepted);
            }

            // Cycle through responses
            let idx = count % self.responses.len();
            match &self.responses[idx] {
                McAssignmentResult::Accepted => Ok(McAssignmentResult::Accepted),
                McAssignmentResult::Rejected(reason) => Ok(McAssignmentResult::Rejected(*reason)),
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[tokio::test]
        async fn test_mock_accepting() {
            let mock = MockMcClient::accepting();
            let result = mock
                .assign_meeting("http://mc:50051", "meeting-1", &[], "gc-1")
                .await
                .unwrap();

            assert!(matches!(result, McAssignmentResult::Accepted));
            assert_eq!(mock.call_count(), 1);
        }

        #[tokio::test]
        async fn test_mock_rejecting() {
            let mock = MockMcClient::rejecting(McRejectionReason::AtCapacity);
            let result = mock
                .assign_meeting("http://mc:50051", "meeting-1", &[], "gc-1")
                .await
                .unwrap();

            assert!(matches!(
                result,
                McAssignmentResult::Rejected(McRejectionReason::AtCapacity)
            ));
        }

        #[tokio::test]
        async fn test_mock_failing() {
            let mock = MockMcClient::failing();
            let result = mock
                .assign_meeting("http://mc:50051", "meeting-1", &[], "gc-1")
                .await;

            assert!(result.is_err());
        }

        #[tokio::test]
        async fn test_mock_cycling_responses() {
            let mock = MockMcClient::with_responses(vec![
                McAssignmentResult::Rejected(McRejectionReason::AtCapacity),
                McAssignmentResult::Rejected(McRejectionReason::Draining),
                McAssignmentResult::Accepted,
            ]);

            // First call: AtCapacity
            let r1 = mock
                .assign_meeting("http://mc:50051", "meeting-1", &[], "gc-1")
                .await
                .unwrap();
            assert!(matches!(
                r1,
                McAssignmentResult::Rejected(McRejectionReason::AtCapacity)
            ));

            // Second call: Draining
            let r2 = mock
                .assign_meeting("http://mc:50051", "meeting-1", &[], "gc-1")
                .await
                .unwrap();
            assert!(matches!(
                r2,
                McAssignmentResult::Rejected(McRejectionReason::Draining)
            ));

            // Third call: Accepted
            let r3 = mock
                .assign_meeting("http://mc:50051", "meeting-1", &[], "gc-1")
                .await
                .unwrap();
            assert!(matches!(r3, McAssignmentResult::Accepted));

            // Fourth call: cycles back to AtCapacity
            let r4 = mock
                .assign_meeting("http://mc:50051", "meeting-1", &[], "gc-1")
                .await
                .unwrap();
            assert!(matches!(
                r4,
                McAssignmentResult::Rejected(McRejectionReason::AtCapacity)
            ));

            assert_eq!(mock.call_count(), 4);
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_rejection_reason_from_proto() {
        assert_eq!(McRejectionReason::from(0), McRejectionReason::Unspecified);
        assert_eq!(McRejectionReason::from(1), McRejectionReason::AtCapacity);
        assert_eq!(McRejectionReason::from(2), McRejectionReason::Draining);
        assert_eq!(McRejectionReason::from(3), McRejectionReason::Unhealthy);
        assert_eq!(McRejectionReason::from(99), McRejectionReason::Unspecified);
    }

    #[test]
    fn test_mc_client_new() {
        let client = McClient::new(SecretString::from("test-token"));
        assert_eq!(client.service_token.expose_secret(), "test-token");
    }
}
