//! Integration tests for `RegisterMeeting` gRPC over real transport.
//!
//! # Integration value over unit tests
//!
//! `grpc/mh_service.rs::tests` already covers the full field-validation
//! matrix (empty id/endpoint, length caps, invalid scheme, duplicate,
//! pending-promotion) by calling the handler directly. This file adds two
//! cases that only an integration tier can prove:
//!
//! 1. **Happy path over real gRPC transport**: the request travels through
//!    the real `MhAuthLayer` → tonic HTTP/2 → `MhMediaService` → `SessionManager`,
//!    and the response comes back with `accepted = true`. Confirms the whole
//!    pipeline is wired, not just the handler logic.
//! 2. **InvalidArgument over gRPC**: confirms tonic `Status` serialization
//!    carries field-validation failures across the wire (one representative
//!    case; the other validation branches are unit-tested).

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "common/mod.rs"]
mod test_common;

use std::time::Duration;

use mh_service::session::SessionManagerHandle;
use proto_gen::internal::media_handler_service_client::MediaHandlerServiceClient;
use proto_gen::internal::RegisterMeetingRequest;
use tonic::metadata::MetadataValue;
use tonic::transport::{Channel, Endpoint};
use tonic::{Code, Request};

use test_common::grpc_rig::GrpcRig;
use test_common::jwks_rig::JwksRig;
use test_common::tokens::mint_valid_mc_token;

async fn connect_client(rig: &GrpcRig) -> MediaHandlerServiceClient<Channel> {
    let channel = Endpoint::from_shared(rig.url())
        .expect("endpoint url parses")
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(2))
        .connect()
        .await
        .expect("connect to mh-service test gRPC server");
    MediaHandlerServiceClient::new(channel)
}

fn authed_request(
    token: &str,
    meeting_id: &str,
    mc_id: &str,
    mc_grpc_endpoint: &str,
) -> Request<RegisterMeetingRequest> {
    let mut request = Request::new(RegisterMeetingRequest {
        meeting_id: meeting_id.to_string(),
        mc_id: mc_id.to_string(),
        mc_grpc_endpoint: mc_grpc_endpoint.to_string(),
    });
    let value: MetadataValue<_> = format!("Bearer {token}")
        .parse()
        .expect("authorization header parses");
    request.metadata_mut().insert("authorization", value);
    request
}

struct RegisterRig {
    jwks: JwksRig,
    grpc: GrpcRig,
    session_manager: SessionManagerHandle,
}

impl RegisterRig {
    async fn start() -> Self {
        let jwks = JwksRig::start(42, "mh-register-integ-01").await;
        let session_manager = SessionManagerHandle::new();
        let grpc = GrpcRig::start(jwks.jwks_client(), session_manager.clone()).await;
        Self {
            jwks,
            grpc,
            session_manager,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn register_meeting_happy_path_over_grpc_transport() {
    let rig = RegisterRig::start().await;
    let mut client = connect_client(&rig.grpc).await;

    let token = mint_valid_mc_token(&rig.jwks.keypair);

    let response = client
        .register_meeting(authed_request(
            &token,
            "meeting-register-1",
            "mc-register-1",
            "http://mc-register-1:50052",
        ))
        .await
        .expect("RegisterMeeting happy path must succeed over real gRPC transport");

    let inner = response.into_inner();
    assert!(
        inner.accepted,
        "RegisterMeetingResponse.accepted must be true for valid request"
    );

    assert!(
        rig.session_manager
            .is_meeting_registered("meeting-register-1")
            .await,
        "SessionManager did not reflect registration"
    );
    assert_eq!(
        rig.session_manager
            .get_mc_endpoint("meeting-register-1")
            .await
            .as_deref(),
        Some("http://mc-register-1:50052"),
        "SessionManager stored the wrong mc_grpc_endpoint"
    );
}

#[tokio::test]
async fn register_meeting_empty_meeting_id_returns_invalid_argument_over_grpc() {
    // One representative field-validation case — proves tonic Status
    // serialization carries InvalidArgument across the wire. The full
    // matrix is unit-tested at `grpc/mh_service.rs::tests`.
    let rig = RegisterRig::start().await;
    let mut client = connect_client(&rig.grpc).await;

    let token = mint_valid_mc_token(&rig.jwks.keypair);

    let err = client
        .register_meeting(authed_request(
            &token,
            /* empty meeting_id */ "",
            "mc-register-1",
            "http://mc-register-1:50052",
        ))
        .await
        .expect_err("empty meeting_id must produce an InvalidArgument status");

    assert_eq!(
        err.code(),
        Code::InvalidArgument,
        "empty meeting_id must map to InvalidArgument over the wire"
    );
    assert!(
        err.message().contains("meeting_id"),
        "status message must mention which field was invalid; got: {}",
        err.message()
    );
}
