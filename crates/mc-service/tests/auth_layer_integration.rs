// Every `#[tokio::test]` in this file is pinned to `flavor = "current_thread"`
// and that pinning is LOAD-BEARING — `MetricAssertion` binds a per-thread
// recorder; `McAuthLayer` runs the `JwksClient::validate` future on the
// caller's task. On `current_thread` that task IS the test thread and the
// emissions are captured. See `crates/common/src/observability/testing.rs:60-72`.
//
//! Component tests for `McAuthLayer` driving real
//! `mc_jwt_validations_total{token_type=service,...}` and
//! `mc_caller_type_rejected_total` emissions per ADR-0032 Step 3 §Cluster C/E.
//!
//! # Per-failure-class coverage
//!
//! Every `failure_reason` value emitted by the auth layer's
//! `classify_jwt_error` (`auth_interceptor.rs:46-53`) plus `scope_mismatch`
//! has a per-test reproducer with `assert_delta(1)` on the named label and
//! `assert_delta(0)` on every sibling (label-swap-bug catcher per ADR-0032
//! §Pattern #3, mandated by @test review of the plan).
//!
//! # Surprising mappings (do NOT "fix" by changing the assertion label)
//!
//! Past-`exp` tokens map to `failure_reason="signature_invalid"`, NOT
//! `"expired"`. The `decode::<T>` call at `crates/common/src/jwt.rs:1027-1030`
//! catches expired-validation failures via `validation.validate_exp = true`
//! and folds them into `JwtError::InvalidSignature` (catch-all), which the
//! interceptor at `auth_interceptor.rs:49` maps to `signature_invalid`. The
//! ONLY path to `failure_reason="expired"` is `JwtError::IatTooFarInFuture`
//! (iat in the future, beyond the configured clock skew).
//!
//! # Snapshot scoping
//!
//! `mc_jwt_validations_total` is also emitted by the meeting-token path in
//! `webtransport/connection.rs:215,231`. Cross-pollution between the two
//! recording sites cannot occur under `MetricAssertion`'s label-tuple-scoped
//! query model — every assertion in this file scopes to
//! `("token_type", "service")`. A future maintainer adding a new `token_type`
//! variant should preserve that scoping.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use ::common::jwt::{JwksClient, ServiceClaims};
use ::common::observability::testing::MetricAssertion;
use axum::http;
use chrono::Utc;
use mc_service::grpc::McAuthLayer;
use mc_test_utils::jwt_test::{mount_jwks_mock, TestKeypair};
use tonic::body::BoxBody;
use tower::{Layer, Service, ServiceExt};
use wiremock::MockServer;

const MH_GRPC_PATH: &str =
    "/dark_tower.internal.MediaCoordinationService/NotifyParticipantConnected";
const GC_GRPC_PATH: &str = "/dark_tower.internal.MeetingControllerService/AssignMeetingWithMh";
const REQUIRED_SCOPE: &str = "service.write.mc";
const WRONG_SCOPE: &str = "service.write.gc";

/// All bounded `failure_reason` values emitted by `mc_jwt_validations_total`
/// from the service-token path. Used for `assert_delta(0)` adjacency on every
/// sibling label per ADR-0032 §Pattern #3.
const ALL_FAILURE_REASONS: &[&str] = &[
    "none",
    "signature_invalid",
    "expired",
    "scope_mismatch",
    "malformed",
];

#[derive(Clone)]
struct NoopService;

impl Service<http::Request<BoxBody>> for NoopService {
    type Response = http::Response<BoxBody>;
    type Error = Box<dyn std::error::Error + Send + Sync>;
    type Future =
        Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _request: http::Request<BoxBody>) -> Self::Future {
        Box::pin(async { Ok(http::Response::new(BoxBody::default())) })
    }
}

async fn setup() -> (MockServer, TestKeypair, McAuthLayer) {
    let mock_server = MockServer::start().await;
    let keypair = TestKeypair::new(42, "mc-auth-integ-key-01");
    let jwks_url = mount_jwks_mock(&mock_server, &keypair).await;
    let jwks_client = Arc::new(JwksClient::new(jwks_url).expect("JwksClient"));
    let layer = McAuthLayer::new(jwks_client, 300);
    (mock_server, keypair, layer)
}

fn make_service_claims(
    iat_offset: i64,
    exp_offset: i64,
    scope: &str,
    service_type: Option<&str>,
) -> ServiceClaims {
    let now = Utc::now().timestamp();
    ServiceClaims::new(
        "test-service".to_string(),
        now + exp_offset,
        now + iat_offset,
        scope.to_string(),
        service_type.map(String::from),
    )
}

fn bearer_request(uri: &str, token: &str) -> http::Request<BoxBody> {
    http::Request::builder()
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(BoxBody::default())
        .unwrap()
}

/// Adjacency helper: assert delta=1 on the named (result, token_type, failure_reason)
/// AND delta=0 on every other failure_reason for the same (result, token_type).
///
/// `also_emits_success_none`: pass `true` for label paths that are reached
/// AFTER the JWKS validation success branch (`auth_interceptor.rs:188`), e.g.
/// `scope_mismatch` is recorded at `:213` only AFTER `:188` already emitted
/// `success/none`. For these paths the `none` adjacency check must be skipped.
fn assert_jwt_label_isolated(
    snap: &::common::observability::testing::MetricSnapshot,
    expected_reason: &str,
    also_emits_success_none: bool,
) {
    let result_label = if expected_reason == "none" {
        "success"
    } else {
        "failure"
    };
    snap.counter("mc_jwt_validations_total")
        .with_labels(&[
            ("result", result_label),
            ("token_type", "service"),
            ("failure_reason", expected_reason),
        ])
        .assert_delta(1);
    for sibling in ALL_FAILURE_REASONS {
        if *sibling == expected_reason {
            continue;
        }
        if also_emits_success_none && *sibling == "none" {
            // For paths that emit success/none THEN a downstream failure
            // label, the success/none counter is also delta=1, not 0.
            snap.counter("mc_jwt_validations_total")
                .with_labels(&[
                    ("result", "success"),
                    ("token_type", "service"),
                    ("failure_reason", "none"),
                ])
                .assert_delta(1);
            continue;
        }
        let sibling_result = if *sibling == "none" {
            "success"
        } else {
            "failure"
        };
        snap.counter("mc_jwt_validations_total")
            .with_labels(&[
                ("result", sibling_result),
                ("token_type", "service"),
                ("failure_reason", *sibling),
            ])
            .assert_delta(0);
    }
}

// ---------------------------------------------------------------------------
// Per-failure-reason tests (Cluster C — service token JWT validation)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn auth_layer_records_jwt_success_none_for_valid_mh_token() {
    let (_mock, keypair, layer) = setup().await;
    let mut svc = layer.layer(NoopService);

    let claims = make_service_claims(0, 3600, REQUIRED_SCOPE, Some("media-handler"));
    let token = keypair.sign_token(&claims);

    let snap = MetricAssertion::snapshot();
    let _ = svc
        .ready()
        .await
        .unwrap()
        .call(bearer_request(MH_GRPC_PATH, &token))
        .await
        .unwrap();

    assert_jwt_label_isolated(&snap, "none", false);
}

#[tokio::test(flavor = "current_thread")]
async fn auth_layer_records_jwt_failure_signature_invalid_for_wrong_signing_key() {
    let (_mock, _keypair, layer) = setup().await;
    let mut svc = layer.layer(NoopService);

    // Sign with a key not in JWKS → JwtError::KeyNotFound → "signature_invalid".
    let wrong = TestKeypair::new(99, "wrong-key");
    let claims = make_service_claims(0, 3600, REQUIRED_SCOPE, Some("media-handler"));
    let token = wrong.sign_token(&claims);

    let snap = MetricAssertion::snapshot();
    let _ = svc
        .ready()
        .await
        .unwrap()
        .call(bearer_request(MH_GRPC_PATH, &token))
        .await
        .unwrap();

    assert_jwt_label_isolated(&snap, "signature_invalid", false);
}

#[tokio::test(flavor = "current_thread")]
async fn auth_layer_records_jwt_failure_expired_for_iat_in_future() {
    // Per @code-reviewer F1: the only path to `failure_reason="expired"` is
    // `JwtError::IatTooFarInFuture`. Mint a token with `iat = now + 86400`
    // (one day in the future) against a 300s clock-skew validator.
    let (_mock, keypair, layer) = setup().await;
    let mut svc = layer.layer(NoopService);

    let claims = make_service_claims(86400, 86400 + 3600, REQUIRED_SCOPE, Some("media-handler"));
    let token = keypair.sign_token(&claims);

    let snap = MetricAssertion::snapshot();
    let _ = svc
        .ready()
        .await
        .unwrap()
        .call(bearer_request(MH_GRPC_PATH, &token))
        .await
        .unwrap();

    assert_jwt_label_isolated(&snap, "expired", false);
}

#[tokio::test(flavor = "current_thread")]
async fn auth_layer_records_jwt_failure_malformed_for_oversized_token() {
    // Token over `MAX_JWT_SIZE_BYTES` (8KB) is rejected at the structural
    // pre-check at `auth_interceptor.rs:175-183` BEFORE JWKS validation runs.
    // BUT — that branch returns directly without recording the metric (no
    // `record_jwt_validation` call between :175 and :184). So an oversized
    // token does NOT emit `failure_reason=malformed` via this layer.
    //
    // The `malformed` label IS reachable via `JwtError::TokenTooLarge`,
    // `MalformedToken`, or `MissingKid` from `JwksClient::validate`. We
    // mint a structurally-broken JWT (missing `kid` in the header) by
    // hand-encoding and assert the resulting label.
    let (_mock, _keypair, layer) = setup().await;
    let mut svc = layer.layer(NoopService);

    // A JWT with three parts (header.payload.signature) but NO `kid` in the
    // header. The header is a base64url-encoded `{"alg":"EdDSA","typ":"JWT"}`.
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    let header_json = b"{\"alg\":\"EdDSA\",\"typ\":\"JWT\"}";
    let header_b64 = URL_SAFE_NO_PAD.encode(header_json);
    let payload_b64 = URL_SAFE_NO_PAD.encode(b"{\"sub\":\"x\"}");
    let signature_b64 = URL_SAFE_NO_PAD.encode(b"sig");
    let token = format!("{header_b64}.{payload_b64}.{signature_b64}");

    let snap = MetricAssertion::snapshot();
    let _ = svc
        .ready()
        .await
        .unwrap()
        .call(bearer_request(MH_GRPC_PATH, &token))
        .await
        .unwrap();

    assert_jwt_label_isolated(&snap, "malformed", false);
}

#[tokio::test(flavor = "current_thread")]
async fn auth_layer_records_jwt_failure_scope_mismatch_for_wrong_scope() {
    let (_mock, keypair, layer) = setup().await;
    let mut svc = layer.layer(NoopService);

    // Valid signature, wrong scope → auth_interceptor.rs:213 records
    // `failure_reason=scope_mismatch`.
    let claims = make_service_claims(0, 3600, WRONG_SCOPE, Some("media-handler"));
    let token = keypair.sign_token(&claims);

    let snap = MetricAssertion::snapshot();
    let _ = svc
        .ready()
        .await
        .unwrap()
        .call(bearer_request(MH_GRPC_PATH, &token))
        .await
        .unwrap();

    // scope_mismatch is reached AFTER the JWKS-success branch records
    // `success/none` at auth_interceptor.rs:188; both labels fire on this path.
    assert_jwt_label_isolated(&snap, "scope_mismatch", true);
}

// ---------------------------------------------------------------------------
// `mc_caller_type_rejected_total` (Cluster E)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn auth_layer_records_caller_type_rejected_for_mh_calling_meeting_controller_service() {
    // MH-typed token (service_type=media-handler) calling the GC-only
    // MeetingControllerService path → auth_interceptor.rs:246 records.
    let (_mock, keypair, layer) = setup().await;
    let mut svc = layer.layer(NoopService);

    let claims = make_service_claims(0, 3600, REQUIRED_SCOPE, Some("media-handler"));
    let token = keypair.sign_token(&claims);

    let snap = MetricAssertion::snapshot();
    let _ = svc
        .ready()
        .await
        .unwrap()
        .call(bearer_request(GC_GRPC_PATH, &token))
        .await
        .unwrap();

    snap.counter("mc_caller_type_rejected_total")
        .with_labels(&[
            ("grpc_service", "MeetingControllerService"),
            ("expected_type", "global-controller"),
            ("actual_type", "media-handler"),
        ])
        .assert_delta(1);
    // Adjacency — the dual mismatch direction must NOT have fired.
    snap.counter("mc_caller_type_rejected_total")
        .with_labels(&[
            ("grpc_service", "MediaCoordinationService"),
            ("expected_type", "media-handler"),
            ("actual_type", "global-controller"),
        ])
        .assert_delta(0);
}

#[tokio::test(flavor = "current_thread")]
async fn auth_layer_records_caller_type_rejected_for_gc_calling_media_coordination_service() {
    // GC-typed token calling the MH-only MediaCoordinationService path.
    let (_mock, keypair, layer) = setup().await;
    let mut svc = layer.layer(NoopService);

    let claims = make_service_claims(0, 3600, REQUIRED_SCOPE, Some("global-controller"));
    let token = keypair.sign_token(&claims);

    let snap = MetricAssertion::snapshot();
    let _ = svc
        .ready()
        .await
        .unwrap()
        .call(bearer_request(MH_GRPC_PATH, &token))
        .await
        .unwrap();

    snap.counter("mc_caller_type_rejected_total")
        .with_labels(&[
            ("grpc_service", "MediaCoordinationService"),
            ("expected_type", "media-handler"),
            ("actual_type", "global-controller"),
        ])
        .assert_delta(1);
    snap.counter("mc_caller_type_rejected_total")
        .with_labels(&[
            ("grpc_service", "MeetingControllerService"),
            ("expected_type", "global-controller"),
            ("actual_type", "media-handler"),
        ])
        .assert_delta(0);
}

#[tokio::test(flavor = "current_thread")]
async fn auth_layer_records_caller_type_rejected_unknown_actual_type() {
    // service_type missing → claims.service_type.as_deref().unwrap_or("unknown")
    // produces actual_type="unknown".
    let (_mock, keypair, layer) = setup().await;
    let mut svc = layer.layer(NoopService);

    let claims = make_service_claims(0, 3600, REQUIRED_SCOPE, None);
    let token = keypair.sign_token(&claims);

    let snap = MetricAssertion::snapshot();
    let _ = svc
        .ready()
        .await
        .unwrap()
        .call(bearer_request(MH_GRPC_PATH, &token))
        .await
        .unwrap();

    snap.counter("mc_caller_type_rejected_total")
        .with_labels(&[
            ("grpc_service", "MediaCoordinationService"),
            ("expected_type", "media-handler"),
            ("actual_type", "unknown"),
        ])
        .assert_delta(1);
}
