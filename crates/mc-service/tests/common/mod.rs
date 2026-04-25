//! Shared test infrastructure for `mc-service` integration tests.
//!
//! - [`MockMhAssignmentStore`] / [`MockMhRegistrationClient`]: in-memory
//!   trait-impl mocks used by both the accept-loop rig and the legacy
//!   join-flow tests. Per @dry-reviewer, these live next to their consumers
//!   (in `tests/common/`) rather than in `mc-test-utils`, mirroring MH's
//!   `tests/common/mock_mc.rs` pattern.
//!
//! Submodules:
//! - [`accept_loop_rig`]: byte-identical wrapper over
//!   `WebTransportServer::bind() → accept_loop()` for component-tier metric
//!   tests (ADR-0032 Step 3 §Deliverable 3).

#![allow(dead_code)]

pub mod accept_loop_rig;

use std::pin::Pin;
use std::sync::{Arc, Mutex};

use ::common::jwt::JwksClient;
use ::common::secret::SecretBox;
use mc_service::actors::{ActorMetrics, ControllerMetrics, MeetingControllerActorHandle};
use mc_service::auth::McJwtValidator;
use mc_service::errors::McError;
use mc_service::grpc::MhRegistrationClient;
use mc_service::mh_connection_registry::MhConnectionRegistry;
use mc_service::redis::{MhAssignmentData, MhAssignmentStore, MhEndpointInfo};
use mc_test_utils::jwt_test::{mount_jwks_mock, TestKeypair};
use wiremock::MockServer;

// =============================================================================
// MockMhAssignmentStore — in-memory `get_mh_assignment`, no Redis.
// =============================================================================

pub struct MockMhAssignmentStore {
    data: Mutex<std::collections::HashMap<String, MhAssignmentData>>,
}

impl MockMhAssignmentStore {
    pub fn new() -> Self {
        Self {
            data: Mutex::new(std::collections::HashMap::new()),
        }
    }

    pub fn insert(&self, meeting_id: &str, data: MhAssignmentData) {
        self.data
            .lock()
            .expect("MockMhAssignmentStore mutex poisoned")
            .insert(meeting_id.to_string(), data);
    }
}

impl Default for MockMhAssignmentStore {
    fn default() -> Self {
        Self::new()
    }
}

impl MhAssignmentStore for MockMhAssignmentStore {
    fn get_mh_assignment<'a>(
        &'a self,
        meeting_id: &'a str,
    ) -> Pin<
        Box<
            dyn std::future::Future<Output = Result<Option<MhAssignmentData>, McError>> + Send + 'a,
        >,
    > {
        let result = self
            .data
            .lock()
            .expect("MockMhAssignmentStore mutex poisoned")
            .get(meeting_id)
            .cloned();
        Box::pin(async move { Ok(result) })
    }
}

// =============================================================================
// MockMhRegistrationClient — records `register_meeting` calls; configurable result.
// =============================================================================

#[derive(Debug, Clone)]
pub struct RegisterMeetingCall {
    pub mh_grpc_endpoint: String,
    pub meeting_id: String,
    pub mc_id: String,
    pub mc_grpc_endpoint: String,
}

pub struct MockMhRegistrationClient {
    calls: Mutex<Vec<RegisterMeetingCall>>,
    /// If set, `register_meeting()` returns this result.
    result: Result<(), McError>,
}

impl MockMhRegistrationClient {
    pub fn new() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            result: Ok(()),
        }
    }

    pub fn with_error(err: McError) -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            result: Err(err),
        }
    }

    pub fn calls(&self) -> Vec<RegisterMeetingCall> {
        self.calls
            .lock()
            .expect("MockMhRegistrationClient mutex poisoned")
            .clone()
    }
}

impl Default for MockMhRegistrationClient {
    fn default() -> Self {
        Self::new()
    }
}

impl MhRegistrationClient for MockMhRegistrationClient {
    fn register_meeting<'a>(
        &'a self,
        mh_grpc_endpoint: &'a str,
        meeting_id: &'a str,
        mc_id: &'a str,
        mc_grpc_endpoint: &'a str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), McError>> + Send + 'a>> {
        self.calls
            .lock()
            .expect("MockMhRegistrationClient mutex poisoned")
            .push(RegisterMeetingCall {
                mh_grpc_endpoint: mh_grpc_endpoint.to_string(),
                meeting_id: meeting_id.to_string(),
                mc_id: mc_id.to_string(),
                mc_grpc_endpoint: mc_grpc_endpoint.to_string(),
            });
        let result = match &self.result {
            Ok(()) => Ok(()),
            Err(e) => Err(McError::Grpc(e.to_string())),
        };
        Box::pin(async move { result })
    }
}

// =============================================================================
// Shared bring-up — single source of truth for the MC component-test stack.
//
// `TestServer::start` (`tests/join_tests.rs`) and `start_rig`
// (`tests/webtransport_accept_loop_integration.rs`) previously each built the
// same wiremock-JWKS + JwtValidator + ActorMetrics + ControllerHandle + mock
// stores stack inline (~25 LoC duplicated). Per @dry-reviewer F-DRY-1, those
// bring-ups now both call `build_test_stack` and `seed_meeting_with_mh` and
// reduce to thin wrappers around `AcceptLoopRig::start_with`.
// =============================================================================

pub struct TestStackHandles {
    pub controller_handle: Arc<MeetingControllerActorHandle>,
    pub jwt_validator: Arc<McJwtValidator>,
    pub mh_store: Arc<MockMhAssignmentStore>,
    pub mh_reg_client: Arc<MockMhRegistrationClient>,
    pub mock_server: MockServer,
    pub keypair: TestKeypair,
}

/// Builds the wiremock-JWKS + `McJwtValidator` + actor-handle + mock-stores
/// stack used by both the accept-loop rig and the legacy join-flow tests.
///
/// `keypair_label` is the only meaningful axis of variation across callers
/// (test logs cite the label on JWT-validation paths). Everything else is
/// fixed: 300s clock skew on the validator, zero-byte master secret,
/// fresh `MhConnectionRegistry`, mc_id `"mc-test"`.
pub async fn build_test_stack(keypair_label: &str) -> TestStackHandles {
    let mock_server = MockServer::start().await;
    let keypair = TestKeypair::new(42, keypair_label);
    let jwks_url = mount_jwks_mock(&mock_server, &keypair).await;

    let jwks_client = Arc::new(JwksClient::new(jwks_url).expect("JwksClient::new"));
    let jwt_validator = Arc::new(McJwtValidator::new(jwks_client, 300));

    let master_secret = SecretBox::new(Box::new(vec![0u8; 32]));
    let metrics = ActorMetrics::new();
    let controller_metrics = ControllerMetrics::new();
    let controller_handle = Arc::new(MeetingControllerActorHandle::new(
        "mc-test".to_string(),
        metrics,
        controller_metrics,
        master_secret,
        Arc::new(MhConnectionRegistry::new()),
    ));

    let mh_store: Arc<MockMhAssignmentStore> = Arc::new(MockMhAssignmentStore::new());
    let mh_reg_client: Arc<MockMhRegistrationClient> = Arc::new(MockMhRegistrationClient::new());

    TestStackHandles {
        controller_handle,
        jwt_validator,
        mh_store,
        mh_reg_client,
        mock_server,
        keypair,
    }
}

/// Seed an MH assignment for `meeting_id` and create the meeting on the
/// controller actor. Used by tests that need both the MC controller and an
/// MH assignment in place before issuing a JoinRequest.
pub async fn seed_meeting_with_mh(handles: &TestStackHandles, meeting_id: &str) {
    handles.mh_store.insert(
        meeting_id,
        MhAssignmentData {
            handlers: vec![MhEndpointInfo {
                mh_id: "mh-test-1".to_string(),
                webtransport_endpoint: "wt://mh-test-1:4433".to_string(),
                grpc_endpoint: Some("http://mh-test-1:50053".to_string()),
            }],
            assigned_at: "2026-04-25T00:00:00Z".to_string(),
        },
    );
    handles
        .controller_handle
        .create_meeting(meeting_id.to_string())
        .await
        .expect("create_meeting");
}
