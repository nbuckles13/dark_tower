//! Integration test: GC→AC trace continuity (R-56 surface (b)).
//!
//! With an active span carrying a known W3C parent context, `AcClient`'s
//! outbound HTTP call to AC MUST carry a `traceparent` header whose trace-id
//! matches the active span's parent — proving `otel_http::inject_trace_context`
//! is actually wired into `ac_client.rs`'s request-building path, not just
//! unit-tested in isolation.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "common/mod.rs"]
mod test_common;
use test_common::otel_support::{known_traceparent, setup_otel_test_environment};

use common::secret::SecretString;
use common::token_manager::TokenReceiver;
use gc_service::services::ac_client::{
    AcClient, MeetingRole, MeetingTokenRequest, ParticipantType,
};
use opentelemetry::trace::TraceContextExt;
use opentelemetry::Context;
use tokio::sync::watch;
use tracing_opentelemetry::OpenTelemetrySpanExt;
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn test_token_receiver(token: &str) -> TokenReceiver {
    let (_tx, rx) = watch::channel(SecretString::from(token));
    TokenReceiver::from_watch_receiver(rx)
}

/// Rebuild the fixed `traceparent`'s remote `SpanContext` so we can set it as
/// the active span's parent (mirrors `otel_http.rs`'s own test fixtures).
fn known_remote_span_context() -> opentelemetry::trace::SpanContext {
    use opentelemetry::trace::{SpanContext, SpanId, TraceFlags, TraceId, TraceState};
    SpanContext::new(
        TraceId::from(0x4bf9_2f35_77b3_4da6_a3ce_929d_0e0e_4736_u128),
        SpanId::from(0x00f0_67aa_0ba9_02b7_u64),
        TraceFlags::SAMPLED,
        true,
        TraceState::default(),
    )
}

#[tokio::test]
async fn ac_client_meeting_token_request_carries_active_span_traceparent() {
    let _otel_guard = setup_otel_test_environment();

    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/auth/internal/meeting-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "token": "eyJ.test.token",
            "expires_in": 900
        })))
        .mount(&mock_server)
        .await;

    let token_receiver = test_token_receiver("test-service-token");
    let client =
        AcClient::new(mock_server.uri(), token_receiver).expect("AcClient should construct");

    let request = MeetingTokenRequest {
        subject_user_id: Uuid::from_u128(1),
        meeting_id: Uuid::from_u128(2),
        meeting_org_id: Uuid::from_u128(3),
        home_org_id: Uuid::nil(),
        participant_type: ParticipantType::Member,
        role: MeetingRole::Participant,
        capabilities: vec!["audio".to_string()],
        ttl_seconds: 900,
    };

    // Enter a span whose parent is the fixed remote SpanContext, mirroring
    // an inbound request that already had trace context extracted (surface
    // (a)) before reaching this handler-level call.
    let span = tracing::info_span!("test_gc_join_meeting_handler");
    span.set_parent(Context::new().with_remote_span_context(known_remote_span_context()));
    let _entered = span.enter();

    let result = client.request_meeting_token(&request).await;
    assert!(
        result.is_ok(),
        "meeting token request should succeed: {:?}",
        result.err()
    );

    // Inspect what AC actually received.
    let received = mock_server
        .received_requests()
        .await
        .expect("recording enabled");
    assert_eq!(received.len(), 1, "expected exactly one request to AC");
    let traceparent = received[0]
        .headers
        .get("traceparent")
        .expect("AC request must carry a traceparent header")
        .to_str()
        .expect("traceparent header must be a valid string");

    assert!(
        traceparent.starts_with("00-"),
        "traceparent malformed: {traceparent:?}"
    );
    let received_trace_id = traceparent
        .get(3..35)
        .expect("traceparent has a trace-id segment");
    let expected_trace_id = &known_traceparent()[3..35];
    assert_eq!(
        received_trace_id, expected_trace_id,
        "GC->AC outbound traceparent's trace-id must match the active span's parent trace-id"
    );

    // Security invariant: the Authorization header (also present on this
    // request) must be untouched/distinct from the injected trace headers.
    assert!(received[0].headers.get("authorization").is_some());
}

#[tokio::test]
async fn ac_client_guest_token_request_carries_active_span_traceparent() {
    let _otel_guard = setup_otel_test_environment();

    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/auth/internal/guest-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "token": "eyJ.guest.token",
            "expires_in": 900
        })))
        .mount(&mock_server)
        .await;

    let token_receiver = test_token_receiver("test-service-token");
    let client =
        AcClient::new(mock_server.uri(), token_receiver).expect("AcClient should construct");

    let request = gc_service::services::ac_client::GuestTokenRequest {
        guest_id: Uuid::from_u128(100),
        display_name: "Test Guest".to_string(),
        meeting_id: Uuid::from_u128(2),
        meeting_org_id: Uuid::from_u128(3),
        waiting_room: false,
        ttl_seconds: 900,
    };

    let span = tracing::info_span!("test_gc_guest_token_handler");
    span.set_parent(Context::new().with_remote_span_context(known_remote_span_context()));
    let _entered = span.enter();

    let result = client.request_guest_token(&request).await;
    assert!(result.is_ok(), "guest token request should succeed");

    let received = mock_server
        .received_requests()
        .await
        .expect("recording enabled");
    assert_eq!(received.len(), 1);
    let traceparent = received[0]
        .headers
        .get("traceparent")
        .expect("AC request must carry a traceparent header")
        .to_str()
        .expect("valid header string");
    let received_trace_id = traceparent.get(3..35).expect("has trace-id segment");
    let expected_trace_id = &known_traceparent()[3..35];
    assert_eq!(received_trace_id, expected_trace_id);
}
