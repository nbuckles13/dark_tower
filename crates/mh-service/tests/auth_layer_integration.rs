//! Integration tests for `MhAuthLayer` wired into the real mh-service
//! gRPC stack (tonic `Server` + `MhMediaService`).
//!
//! # Integration value over unit tests
//!
//! `auth_interceptor.rs::tests` exhaustively covers the rejection matrix of
//! `MhAuthLayer` as a tower `Service`, calling it directly. This file proves
//! the layer is actually *installed* on the tonic server wired in `main.rs` —
//! Bearer tokens travel over real TCP + HTTP/2, JWKS is fetched over the
//! network, tonic `Status` serialization works end-to-end, and validated
//! claims reach the downstream `MhMediaService` handler (visible via the
//! handler's side-effect on `SessionManagerHandle`).
//!
//! Kept: one proof per outcome class (success, UNAUTHENTICATED,
//! PERMISSION_DENIED) plus two security-critical attack vectors and two
//! non-negotiable Layer 2 routing cases.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "common/mod.rs"]
mod test_common;

use std::time::Duration;

use common::jwt::ServiceClaims;
use common::observability::testing::MetricAssertion;
use mh_service::session::SessionManagerHandle;
use proto_gen::internal::media_handler_service_client::MediaHandlerServiceClient;
use proto_gen::internal::RegisterMeetingRequest;
use tonic::metadata::MetadataValue;
use tonic::transport::{Channel, Endpoint};
use tonic::{Code, Request};

use test_common::grpc_rig::GrpcRig;
use test_common::jwks_rig::JwksRig;
use test_common::tokens::{
    craft_alg_none_token, craft_hs256_key_confusion_token, mint_expired_mc_token,
    mint_no_service_type_token, mint_valid_mc_token, mint_wrong_service_type_token, MC_SCOPE,
    MC_SERVICE_TYPE,
};
use test_common::TestKeypair;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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

fn req_with_bearer(token: Option<&str>) -> Request<RegisterMeetingRequest> {
    let mut request = Request::new(RegisterMeetingRequest {
        meeting_id: "meeting-auth-test".to_string(),
        mc_id: "mc-auth-test".to_string(),
        mc_grpc_endpoint: "http://mc-auth-test:50052".to_string(),
    });
    if let Some(t) = token {
        let value: MetadataValue<_> = format!("Bearer {t}")
            .parse()
            .expect("authorization header parses");
        request.metadata_mut().insert("authorization", value);
    }
    request
}

struct AuthRig {
    jwks: JwksRig,
    grpc: GrpcRig,
    session_manager: SessionManagerHandle,
}

impl AuthRig {
    async fn start() -> Self {
        let jwks = JwksRig::start(42, "mh-auth-integ-01").await;
        let session_manager = SessionManagerHandle::new();
        let grpc = GrpcRig::start(jwks.jwks_client(), session_manager.clone()).await;
        Self {
            jwks,
            grpc,
            session_manager,
        }
    }
}

fn valid_claims() -> ServiceClaims {
    let now = chrono::Utc::now().timestamp();
    ServiceClaims::new(
        "mc-attacker".to_string(),
        now + 3600,
        now,
        MC_SCOPE.to_string(),
        Some(MC_SERVICE_TYPE.to_string()),
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn valid_mc_token_over_grpc_succeeds_and_reaches_handler() {
    let rig = AuthRig::start().await;
    let mut client = connect_client(&rig.grpc).await;

    let token = mint_valid_mc_token(&rig.jwks.keypair);

    let snap = MetricAssertion::snapshot();
    let response = client
        .register_meeting(req_with_bearer(Some(&token)))
        .await
        .expect("valid MC token should be accepted end-to-end");

    assert!(
        response.into_inner().accepted,
        "handler returned accepted=false for happy path"
    );

    // Claims-injection end-to-end: the handler only runs if auth succeeded
    // AND claims reached it. Observe the side-effect on SessionManager.
    assert!(
        rig.session_manager
            .is_meeting_registered("meeting-auth-test")
            .await,
        "RegisterMeeting handler never updated SessionManager — claims likely did not reach it",
    );
    assert_eq!(
        rig.session_manager
            .get_mc_endpoint("meeting-auth-test")
            .await
            .as_deref(),
        Some("http://mc-auth-test:50052"),
    );

    snap.counter("mh_jwt_validations_total")
        .with_labels(&[
            ("result", "success"),
            ("token_type", "service"),
            ("failure_reason", "none"),
        ])
        .assert_delta(1);
}

#[tokio::test]
async fn missing_bearer_returns_unauthenticated() {
    let rig = AuthRig::start().await;
    let mut client = connect_client(&rig.grpc).await;

    let err = client
        .register_meeting(req_with_bearer(None))
        .await
        .expect_err("missing bearer must be rejected at the layer");

    assert_eq!(
        err.code(),
        Code::Unauthenticated,
        "missing bearer should return UNAUTHENTICATED (proves layer is installed)"
    );
}

#[tokio::test]
async fn signature_invalid_via_wrong_key_returns_unauthenticated() {
    let rig = AuthRig::start().await;
    let mut client = connect_client(&rig.grpc).await;

    let wrong_keypair = TestKeypair::new(99, "attacker-key");
    let token = mint_valid_mc_token(&wrong_keypair);

    let snap = MetricAssertion::snapshot();
    let err = client
        .register_meeting(req_with_bearer(Some(&token)))
        .await
        .expect_err("token signed by unknown key must be rejected");

    assert_eq!(err.code(), Code::Unauthenticated);

    snap.counter("mh_jwt_validations_total")
        .with_labels(&[
            ("result", "failure"),
            ("token_type", "service"),
            ("failure_reason", "signature_invalid"),
        ])
        .assert_delta(1);
}

#[tokio::test]
async fn expired_token_returns_unauthenticated() {
    let rig = AuthRig::start().await;
    let mut client = connect_client(&rig.grpc).await;

    let token = mint_expired_mc_token(&rig.jwks.keypair);

    let err = client
        .register_meeting(req_with_bearer(Some(&token)))
        .await
        .expect_err("expired token must be rejected");

    assert_eq!(err.code(), Code::Unauthenticated);
}

#[tokio::test]
async fn wrong_service_type_returns_permission_denied() {
    let rig = AuthRig::start().await;
    let mut client = connect_client(&rig.grpc).await;

    let token = mint_wrong_service_type_token(&rig.jwks.keypair, "global-controller");

    let snap = MetricAssertion::snapshot();
    let err = client
        .register_meeting(req_with_bearer(Some(&token)))
        .await
        .expect_err("GC-typed token must not reach MediaHandlerService");

    assert_eq!(
        err.code(),
        Code::PermissionDenied,
        "wrong service_type must map to PERMISSION_DENIED (ADR-0003 Layer 2)"
    );

    // JWT is cryptographically valid; the Layer 2 routing check rejects it.
    snap.counter("mh_jwt_validations_total")
        .with_labels(&[
            ("result", "success"),
            ("token_type", "service"),
            ("failure_reason", "none"),
        ])
        .assert_delta(1);
    snap.counter("mh_caller_type_rejected_total")
        .with_labels(&[
            ("grpc_service", "MediaHandlerService"),
            ("expected_type", "meeting-controller"),
            ("actual_type", "global-controller"),
        ])
        .assert_delta(1);
}

#[tokio::test]
async fn no_service_type_claim_returns_permission_denied() {
    // Distinct from wrong-service-type: the `unwrap_or("unknown")` branch in
    // auth_interceptor.rs handles tokens that lack the claim entirely.
    let rig = AuthRig::start().await;
    let mut client = connect_client(&rig.grpc).await;

    let token = mint_no_service_type_token(&rig.jwks.keypair);

    let err = client
        .register_meeting(req_with_bearer(Some(&token)))
        .await
        .expect_err("token without service_type must fail closed");

    assert_eq!(
        err.code(),
        Code::PermissionDenied,
        "missing service_type must map to PERMISSION_DENIED (fail-closed)"
    );
}

#[tokio::test]
async fn alg_none_bypass_attempt_returns_unauthenticated() {
    // CVE-2015-9235-class attack: unsigned JWT with `alg: "none"` (Auth0 2015
    // advisory, "Critical vulnerabilities in JSON Web Token libraries").
    // JWKS-pinned EdDSA validators must reject any alg other than EdDSA.
    let rig = AuthRig::start().await;
    let mut client = connect_client(&rig.grpc).await;

    let token = craft_alg_none_token(&rig.jwks.keypair.kid, &valid_claims());

    let err = client
        .register_meeting(req_with_bearer(Some(&token)))
        .await
        .expect_err("alg:none must never authenticate");

    assert_eq!(
        err.code(),
        Code::Unauthenticated,
        "alg:none bypass attempt must be rejected"
    );
}

#[tokio::test]
async fn alg_hs256_key_confusion_attempt_returns_unauthenticated() {
    // Classic algorithm-confusion (key-confusion): attacker signs with HS256
    // using the published JWKS public key as an HMAC secret. Libraries that
    // accept the header's declared algorithm without checking it against the
    // JWK's `alg` value would verify this as a valid HS256 MAC.
    let rig = AuthRig::start().await;
    let mut client = connect_client(&rig.grpc).await;

    let token = craft_hs256_key_confusion_token(&rig.jwks.keypair, &valid_claims());

    let err = client
        .register_meeting(req_with_bearer(Some(&token)))
        .await
        .expect_err("HS256 key-confusion attempt must not authenticate");

    assert_eq!(
        err.code(),
        Code::Unauthenticated,
        "HS256 with JWKS pubkey as secret must be rejected"
    );
}
