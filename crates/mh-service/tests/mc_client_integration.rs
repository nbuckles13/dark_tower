//! Integration tests for MH→MC notification delivery.
//!
//! Tests the `McClient` retry logic and auth-error short-circuit using
//! the shared `common::mock_mc` rig (`MediaCoordinationService` gRPC mock).

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "common/mod.rs"]
mod test_common;

use common::secret::SecretString;
use common::token_manager::TokenReceiver;
use mh_service::errors::MhError;
use mh_service::grpc::McClient;
use tokio::sync::watch;

use test_common::mock_mc::{start_mock_mc_server, MockBehavior, MockMcServer};

/// Create a mock `TokenReceiver` for testing.
fn mock_token_receiver() -> TokenReceiver {
    let (tx, rx) = watch::channel(SecretString::from("test-service-token"));
    // Keep sender alive by leaking it (test only).
    std::mem::forget(tx);
    TokenReceiver::from_test_channel(rx)
}

// ============================================================================
// Tests
// ============================================================================

#[tokio::test]
async fn test_notify_connected_success() {
    let mc = start_mock_mc_server(MockMcServer::new(MockBehavior::Accept)).await;

    let mc_url = format!("http://{}", mc.addr);
    let client = McClient::new(mock_token_receiver());

    let result = client
        .notify_participant_connected(&mc_url, "meeting-1", "user-1", "mh-1")
        .await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn test_notify_disconnected_success() {
    let mc = start_mock_mc_server(MockMcServer::new(MockBehavior::Accept)).await;

    let mc_url = format!("http://{}", mc.addr);
    let client = McClient::new(mock_token_receiver());

    let result = client
        .notify_participant_disconnected(&mc_url, "meeting-1", "user-1", "mh-1", 1)
        .await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn test_retry_succeeds_after_transient_failure() {
    // Fail the first call, succeed on retry
    let mc = start_mock_mc_server(MockMcServer::new(MockBehavior::FailThenAccept {
        fail_count: 1,
    }))
    .await;

    let mc_url = format!("http://{}", mc.addr);
    let client = McClient::new(mock_token_receiver());

    let result = client
        .notify_participant_connected(&mc_url, "meeting-1", "user-1", "mh-1")
        .await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn test_retry_exhaustion_after_max_attempts() {
    // Fail all 3 attempts (fail_count >= MAX_RETRY_ATTEMPTS)
    let mc = start_mock_mc_server(MockMcServer::new(MockBehavior::FailThenAccept {
        fail_count: 10,
    }))
    .await;

    let mc_url = format!("http://{}", mc.addr);
    let client = McClient::new(mock_token_receiver());

    let result = client
        .notify_participant_connected(&mc_url, "meeting-1", "user-1", "mh-1")
        .await;

    assert!(
        matches!(&result, Err(MhError::Grpc(msg)) if msg.contains("MC notification RPC failed")),
        "Expected Grpc error after exhausting retries, got: {result:?}"
    );
}

#[tokio::test]
async fn test_unauthenticated_error_skips_retry() {
    let mc = start_mock_mc_server(MockMcServer::new(MockBehavior::Unauthenticated)).await;

    let mc_url = format!("http://{}", mc.addr);
    let client = McClient::new(mock_token_receiver());

    let result = client
        .notify_participant_connected(&mc_url, "meeting-1", "user-1", "mh-1")
        .await;

    // Should fail immediately with JwtValidation (mapped from UNAUTHENTICATED)
    assert!(
        matches!(&result, Err(MhError::JwtValidation(_))),
        "Expected JwtValidation error for auth failure, got: {result:?}"
    );
}

#[tokio::test]
async fn test_permission_denied_skips_retry() {
    let mc = start_mock_mc_server(MockMcServer::new(MockBehavior::PermissionDenied)).await;

    let mc_url = format!("http://{}", mc.addr);
    let client = McClient::new(mock_token_receiver());

    let result = client
        .notify_participant_disconnected(&mc_url, "meeting-1", "user-1", "mh-1", 1)
        .await;

    // Should fail immediately with JwtValidation (mapped from PERMISSION_DENIED)
    assert!(
        matches!(&result, Err(MhError::JwtValidation(_))),
        "Expected JwtValidation error for permission denied, got: {result:?}"
    );
}
