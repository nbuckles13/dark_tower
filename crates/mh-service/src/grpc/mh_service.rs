//! `MediaHandlerService` gRPC server implementation.
//!
//! Implements the MC→MH gRPC service from `internal.proto`.
//!
//! **One RPC by design** (ADR-0036 §8): `RegisterMeeting` *is* the MC→MH control
//! plane and gains fields rather than sibling RPCs. The `Register`, `RouteMedia`
//! and `StreamTelemetry` stubs were retired with the 2026-09-01 `internal.proto`
//! reshape; see the tombstone block in that file.
//!
//! `register_meeting` is integrated with `SessionManagerHandle` for registration
//! and pending-connection promotion. It does **not** yet apply forwarding policy
//! — that is story task 5, and until it lands this handler reports
//! `applied_generation: 0` ("nothing applied"), which is the truth.
//!
//! # Security
//!
//! All incoming requests are validated by `MhAuthLayer` before
//! reaching these handlers.

use std::time::Instant;

use crate::observability::metrics;
use crate::session::{MeetingRegistration, SessionManagerHandle};
use proto_gen::dark_tower::internal::v1::media_handler_service_server::MediaHandlerService;
use proto_gen::dark_tower::internal::v1::{RegisterMeetingRequest, RegisterMeetingResponse};
use proto_gen::dark_tower::signaling::v1::TransportMode;
use tonic::{Request, Response, Status};
use tracing::instrument;

/// Maximum allowed length for `meeting_id` and `mc_id` fields.
/// Prevents `HashMap` key bloat from malicious or buggy callers.
const MAX_ID_LENGTH: usize = 256;

/// Maximum allowed length for `mc_grpc_endpoint`.
/// 2048 bytes is generous for any legitimate gRPC endpoint URL.
const MAX_ENDPOINT_LENGTH: usize = 2048;

/// Media Handler gRPC service.
///
/// Handles the single MC→MH RPC, `register_meeting`, which is integrated with
/// `SessionManagerHandle` for registration and pending-connection promotion.
pub struct MhMediaService {
    session_manager: SessionManagerHandle,
}

impl MhMediaService {
    /// Create a new media handler service with the given session manager handle.
    #[must_use]
    pub fn new(session_manager: SessionManagerHandle) -> Self {
        Self { session_manager }
    }
}

impl Default for MhMediaService {
    fn default() -> Self {
        Self::new(SessionManagerHandle::new())
    }
}

#[tonic::async_trait]
impl MediaHandlerService for MhMediaService {
    /// Register a meeting with the media handler.
    ///
    /// Called by MC when a participant is assigned to this MH instance.
    /// Stores the meeting registration in `SessionManagerHandle` and promotes
    /// any pending WebTransport connections that were waiting for this meeting.
    #[instrument(skip_all)]
    async fn register_meeting(
        &self,
        request: Request<RegisterMeetingRequest>,
    ) -> Result<Response<RegisterMeetingResponse>, Status> {
        let req = request.into_inner();

        // Validate required fields
        if req.meeting_id.is_empty() {
            metrics::record_grpc_request("register_meeting", "error");
            return Err(Status::invalid_argument("meeting_id is required"));
        }
        if req.mc_id.is_empty() {
            metrics::record_grpc_request("register_meeting", "error");
            return Err(Status::invalid_argument("mc_id is required"));
        }
        if req.mc_grpc_endpoint.is_empty() {
            metrics::record_grpc_request("register_meeting", "error");
            return Err(Status::invalid_argument("mc_grpc_endpoint is required"));
        }

        // Validate field lengths to prevent HashMap key bloat
        if req.meeting_id.len() > MAX_ID_LENGTH {
            metrics::record_grpc_request("register_meeting", "error");
            return Err(Status::invalid_argument(
                "meeting_id exceeds maximum length",
            ));
        }
        if req.mc_id.len() > MAX_ID_LENGTH {
            metrics::record_grpc_request("register_meeting", "error");
            return Err(Status::invalid_argument("mc_id exceeds maximum length"));
        }

        // Validate mc_grpc_endpoint length
        if req.mc_grpc_endpoint.len() > MAX_ENDPOINT_LENGTH {
            metrics::record_grpc_request("register_meeting", "error");
            return Err(Status::invalid_argument(
                "mc_grpc_endpoint exceeds maximum length",
            ));
        }

        // Validate mc_grpc_endpoint scheme
        if !req.mc_grpc_endpoint.starts_with("http://")
            && !req.mc_grpc_endpoint.starts_with("https://")
            && !req.mc_grpc_endpoint.starts_with("grpc://")
        {
            metrics::record_grpc_request("register_meeting", "error");
            return Err(Status::invalid_argument(
                "mc_grpc_endpoint must use http://, https://, or grpc:// scheme",
            ));
        }

        let promoted = self
            .session_manager
            .register_meeting(
                req.meeting_id.clone(),
                MeetingRegistration {
                    mc_id: req.mc_id.clone(),
                    mc_grpc_endpoint: req.mc_grpc_endpoint.clone(),
                    registered_at: Instant::now(),
                },
            )
            .await;

        let promoted_count = promoted.len();

        tracing::info!(
            target: "mh.grpc.service",
            meeting_id = %req.meeting_id,
            mc_id = %req.mc_id,
            promoted_pending_count = promoted_count,
            "Meeting registered"
        );

        tracing::debug!(
            target: "mh.grpc.service",
            meeting_id = %req.meeting_id,
            mc_grpc_endpoint = %req.mc_grpc_endpoint,
            "Meeting registration details"
        );

        metrics::record_grpc_request("register_meeting", "success");

        // ADR-0036 §8: the response reports what MH's LIVE FORWARD PATH
        // reflects, never what it received.
        //
        // TODO(story task 5, media-handler): apply the request's forwarding
        // policy through a config-apply mailbox and report the generation the
        // session actor has actually installed, plus this handler's identity and
        // process-start epoch.
        //
        // Until then these values are TRUTHFUL, not placeholders: this handler
        // applies no policy, so its forward path reflects generation 0; it holds
        // no handler id to report; it records no process-start instant; and it
        // has applied no transport mode. `applied_generation: req.policy_generation`
        // is deliberately NOT written even transitionally — that single line IS
        // the §8 bug, and a transitional lie here is exactly the kind that
        // survives into task 5 unnoticed.
        Ok(Response::new(RegisterMeetingResponse {
            accepted: true,
            applied_generation: 0,
            handler_id: String::new(),
            process_start_epoch_ms: 0,
            transport_mode: TransportMode::Unspecified as i32,
        }))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::session::PendingConnection;

    fn make_service() -> (MhMediaService, SessionManagerHandle) {
        let sm = SessionManagerHandle::new();
        let svc = MhMediaService::new(sm.clone());
        (svc, sm)
    }

    fn make_register_request(
        meeting_id: &str,
        mc_id: &str,
        mc_grpc_endpoint: &str,
    ) -> Request<RegisterMeetingRequest> {
        Request::new(RegisterMeetingRequest {
            meeting_id: meeting_id.to_string(),
            mc_id: mc_id.to_string(),
            mc_grpc_endpoint: mc_grpc_endpoint.to_string(),
            egress_streams: Vec::new(),
            selection_rules: None,
            policy_generation: 0,
        })
    }

    #[tokio::test]
    async fn test_register_meeting_valid_request_stores_registration() {
        let (svc, sm) = make_service();

        let resp = svc
            .register_meeting(make_register_request(
                "meeting-1",
                "mc-1",
                "http://mc:50052",
            ))
            .await
            .unwrap();

        assert!(resp.into_inner().accepted);
        assert!(sm.is_meeting_registered("meeting-1").await);
        assert_eq!(
            sm.get_mc_endpoint("meeting-1").await.unwrap(),
            "http://mc:50052"
        );
    }

    #[tokio::test]
    async fn test_register_meeting_empty_meeting_id_rejected() {
        let (svc, _sm) = make_service();

        let err = svc
            .register_meeting(make_register_request("", "mc-1", "http://mc:50052"))
            .await
            .unwrap_err();

        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("meeting_id"));
    }

    #[tokio::test]
    async fn test_register_meeting_empty_mc_id_rejected() {
        let (svc, _sm) = make_service();

        let err = svc
            .register_meeting(make_register_request("meeting-1", "", "http://mc:50052"))
            .await
            .unwrap_err();

        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("mc_id"));
    }

    #[tokio::test]
    async fn test_register_meeting_empty_endpoint_rejected() {
        let (svc, _sm) = make_service();

        let err = svc
            .register_meeting(make_register_request("meeting-1", "mc-1", ""))
            .await
            .unwrap_err();

        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("mc_grpc_endpoint"));
    }

    #[tokio::test]
    async fn test_register_meeting_invalid_endpoint_scheme_rejected() {
        let (svc, _sm) = make_service();

        let err = svc
            .register_meeting(make_register_request("meeting-1", "mc-1", "ftp://mc:50052"))
            .await
            .unwrap_err();

        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("scheme"));
    }

    #[tokio::test]
    async fn test_register_meeting_accepts_valid_schemes() {
        let (svc, _sm) = make_service();

        // http://
        assert!(svc
            .register_meeting(make_register_request("m-1", "mc-1", "http://mc:50052"))
            .await
            .is_ok());

        // https://
        assert!(svc
            .register_meeting(make_register_request("m-2", "mc-1", "https://mc:50052"))
            .await
            .is_ok());

        // grpc://
        assert!(svc
            .register_meeting(make_register_request("m-3", "mc-1", "grpc://mc:50052"))
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn test_register_meeting_id_too_long_rejected() {
        let (svc, _sm) = make_service();
        let long_id = "x".repeat(MAX_ID_LENGTH + 1);

        let err = svc
            .register_meeting(make_register_request(&long_id, "mc-1", "http://mc:50052"))
            .await
            .unwrap_err();

        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("meeting_id"));
    }

    #[tokio::test]
    async fn test_register_meeting_mc_id_too_long_rejected() {
        let (svc, _sm) = make_service();
        let long_id = "x".repeat(MAX_ID_LENGTH + 1);

        let err = svc
            .register_meeting(make_register_request(
                "meeting-1",
                &long_id,
                "http://mc:50052",
            ))
            .await
            .unwrap_err();

        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("mc_id"));
    }

    #[tokio::test]
    async fn test_register_meeting_endpoint_too_long_rejected() {
        let (svc, _sm) = make_service();
        let long_endpoint = format!("http://{}", "x".repeat(MAX_ENDPOINT_LENGTH));

        let err = svc
            .register_meeting(make_register_request("meeting-1", "mc-1", &long_endpoint))
            .await
            .unwrap_err();

        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("mc_grpc_endpoint"));
    }

    #[tokio::test]
    async fn test_register_meeting_promotes_pending_connections() {
        let (svc, sm) = make_service();

        // Add pending connections before RegisterMeeting
        sm.add_pending_connection(PendingConnection {
            connection_id: "conn-1".to_string(),
            meeting_id: "meeting-1".to_string(),
            participant_id: "user-1".to_string(),
            connected_at: Instant::now(),
        })
        .await;

        sm.add_pending_connection(PendingConnection {
            connection_id: "conn-2".to_string(),
            meeting_id: "meeting-1".to_string(),
            participant_id: "user-2".to_string(),
            connected_at: Instant::now(),
        })
        .await;

        assert_eq!(sm.active_connection_count().await, 0);

        // RegisterMeeting should promote pending connections
        let resp = svc
            .register_meeting(make_register_request(
                "meeting-1",
                "mc-1",
                "http://mc:50052",
            ))
            .await
            .unwrap();

        assert!(resp.into_inner().accepted);
        assert_eq!(sm.active_connection_count().await, 2);
    }

    #[tokio::test]
    async fn test_register_meeting_duplicate_updates_registration() {
        let (svc, sm) = make_service();

        // First registration
        svc.register_meeting(make_register_request(
            "meeting-1",
            "mc-1",
            "http://mc-1:50052",
        ))
        .await
        .unwrap();

        assert_eq!(
            sm.get_mc_endpoint("meeting-1").await.unwrap(),
            "http://mc-1:50052"
        );

        // Second registration overwrites
        svc.register_meeting(make_register_request(
            "meeting-1",
            "mc-2",
            "http://mc-2:50052",
        ))
        .await
        .unwrap();

        assert_eq!(
            sm.get_mc_endpoint("meeting-1").await.unwrap(),
            "http://mc-2:50052"
        );
    }

    #[tokio::test]
    async fn test_default_creates_working_service() {
        let svc = MhMediaService::default();

        let resp = svc
            .register_meeting(make_register_request(
                "meeting-1",
                "mc-1",
                "http://mc:50052",
            ))
            .await
            .unwrap();

        assert!(resp.into_inner().accepted);
    }
}
