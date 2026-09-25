//! Integration tests for `EndMeeting` and the registered-meeting cap over real
//! gRPC transport (story 2 R-20, R-21).
//!
//! # Integration value over unit tests
//!
//! `grpc/mh_service.rs::tests` and `session/mod.rs::tests` cover the decision
//! logic by calling the handler and the actor directly. This file proves the
//! same outcomes survive the real stack — `MhAuthLayer` → tonic HTTP/2 →
//! `MhMediaService` → session actor — which is where the status codes become a
//! WIRE contract MC depends on: `FAILED_PRECONDITION` (never
//! `PERMISSION_DENIED`) on an ownership mismatch, `acknowledged: true` for both
//! a release and the unknown-meeting no-op, `RESOURCE_EXHAUSTED` for the cap.
//!
//! # Every snapshot is taken BEFORE the service is built
//!
//! The teardown counters and the occupancy gauges are handles the session actor
//! resolves once, at construction, bound to whichever recorder is current then.
//! A snapshot taken afterwards reads 0 whatever the actor does, so every
//! assertion on those series would pass vacuously. The rig is therefore always
//! started after `MetricAssertion::snapshot()`.
//!
//! # The release proof is a TRIPLE, not the counter
//!
//! `outcome="released"` alone passes while the map still holds the entry. The
//! release test asserts the registered-meetings gauge FALLS (with a two-meeting
//! positive control: it must first read 2), the routes entry is gone, and the
//! edges are back in the budget.
//!
//! # Exact occupancy accounting lives HERE, not in env-test 29
//!
//! On a live pod the occupancy gauges are shared with every concurrent suite,
//! so env-test 29 proves release for its own meeting from MH's direct replies
//! (stale-then-fresh generation-1 re-registration) and never asserts a gauge
//! value. That registered meetings and edges return EXACTLY to their previous
//! values after a release — with unrelated load held — is proved by
//! `a_release_returns_registered_meetings_and_edges_to_their_previous_values`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "common/mod.rs"]
mod test_common;

use std::time::Duration;

use common::observability::testing::{MetricAssertion, MetricSnapshot};
use mh_service::config::PolicyLimits;
use mh_service::routing::MeetingKey;
use mh_service::session::SessionManagerHandle;
use mh_test_utils::admission::fixture_policy_limits;
use mh_test_utils::media_policy::{egress, register_request};
use proto_gen::dark_tower::internal::v1::media_handler_service_client::MediaHandlerServiceClient;
use proto_gen::dark_tower::internal::v1::EgressStream;
use proto_gen::dark_tower::internal::v1::{EndMeetingRequest, RegisterMeetingRequest};
use tonic::metadata::MetadataValue;
use tonic::transport::{Channel, Endpoint};
use tonic::{Code, Request};

use test_common::grpc_rig::GrpcRig;
use test_common::jwks_rig::JwksRig;
use test_common::tokens::mint_valid_mc_token;

const KEY_CUSTODY: (&str, &str) = ("key_custody", "operator");

struct Rig {
    // Held for its wiremock server's lifetime.
    _jwks: JwksRig,
    // Held so the server outlives the test body (its Drop stops it).
    _grpc: GrpcRig,
    session_manager: SessionManagerHandle,
    client: MediaHandlerServiceClient<Channel>,
    token: String,
}

impl Rig {
    /// Start the stack. Call AFTER taking the `MetricAssertion` snapshot — see
    /// the module docs.
    async fn start(limits: PolicyLimits) -> Self {
        let jwks = JwksRig::start(42, "mh-end-meeting-integ-01").await;
        let session_manager = SessionManagerHandle::new(mh_test_utils::admission::never_binding());
        let grpc =
            GrpcRig::start_with_limits(jwks.jwks_client(), session_manager.clone(), limits).await;
        let channel = Endpoint::from_shared(grpc.url())
            .expect("endpoint url parses")
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(2))
            .connect()
            .await
            .expect("connect to mh-service test gRPC server");
        let token = mint_valid_mc_token(&jwks.keypair);
        Self {
            _jwks: jwks,
            _grpc: grpc,
            session_manager,
            client: MediaHandlerServiceClient::new(channel),
            token,
        }
    }

    fn authed<T>(&self, message: T) -> Request<T> {
        let mut request = Request::new(message);
        let value: MetadataValue<_> = format!("Bearer {}", self.token)
            .parse()
            .expect("authorization header parses");
        request.metadata_mut().insert("authorization", value);
        request
    }

    async fn register(
        &mut self,
        meeting_id: &str,
        mc_id: &str,
        generation: u64,
        streams: Vec<EgressStream>,
    ) -> Result<u64, tonic::Status> {
        let mut req: RegisterMeetingRequest = register_request(meeting_id, generation, streams);
        req.mc_id = mc_id.to_string();
        let request = self.authed(req);
        self.client
            .register_meeting(request)
            .await
            .map(|r| r.into_inner().applied_generation)
    }

    async fn end(&mut self, meeting_id: &str, mc_id: &str) -> Result<bool, tonic::Status> {
        let request = self.authed(EndMeetingRequest {
            meeting_id: meeting_id.to_string(),
            mc_id: mc_id.to_string(),
        });
        self.client
            .end_meeting(request)
            .await
            .map(|r| r.into_inner().acknowledged)
    }
}

fn registered_meetings(snap: &MetricSnapshot, expected: f64) {
    snap.gauge("mh_media_registered_meetings")
        .with_labels(&[KEY_CUSTODY])
        .assert_value(expected);
}

fn teardown_delta(snap: &MetricSnapshot, outcome: &'static str, expected: u64) {
    snap.counter("mh_media_meeting_teardowns_total")
        .with_labels(&[("outcome", outcome), KEY_CUSTODY])
        .assert_delta(expected);
}

// ---------------------------------------------------------------------------
// Release
// ---------------------------------------------------------------------------

/// The load-bearing proof: releasing one of two meetings makes the
/// registered-meetings gauge FALL (2 -> 1), removes that meeting's routes,
/// returns its edges, and counts `released` — while the OTHER meeting is
/// untouched.
#[tokio::test]
async fn release_over_grpc_drops_the_gauge_the_routes_and_the_edges() {
    let snap = MetricAssertion::snapshot();
    let mut rig = Rig::start(fixture_policy_limits()).await;

    rig.register(
        "m-release",
        "mc-a",
        3,
        vec![egress(1, 5, 0, 6), egress(2, 6, 0, 5)],
    )
    .await
    .unwrap();
    rig.register("m-keep", "mc-a", 1, vec![egress(1, 7, 0, 8)])
        .await
        .unwrap();
    // POSITIVE CONTROL: the gauge demonstrably moved UP first, so the fall
    // below cannot be a gauge that never tracked anything.
    registered_meetings(&snap, 2.0);
    assert_eq!(rig.session_manager.routing_snapshot().total_edges(), 3);

    assert!(rig.end("m-release", "mc-a").await.unwrap(), "acknowledged");

    registered_meetings(&snap, 1.0);
    let routes = rig.session_manager.routing_snapshot();
    assert!(routes.routes_for(&MeetingKey::new("m-release")).is_none());
    assert_eq!(
        routes.total_edges(),
        1,
        "the released meeting's two edges came back"
    );
    assert!(routes.routes_for(&MeetingKey::new("m-keep")).is_some());
    snap.gauge("mh_media_egress_edges")
        .with_labels(&[KEY_CUSTODY])
        .assert_value(1.0);
    teardown_delta(&snap, "released", 1);
    teardown_delta(&snap, "unknown_meeting", 0);
    teardown_delta(&snap, "rejected_ownership", 0);
    snap.counter("mh_grpc_requests_total")
        .with_labels(&[("method", "end_meeting"), ("status", "success")])
        .assert_delta(1);
}

/// The exact occupancy accounting env-test 29 cannot make on a shared pod:
/// with OTHER load already held, a meeting's full lifecycle (register at a
/// held generation, stale probe, foreign release, release, re-create, release)
/// moves `mh_media_registered_meetings` and `mh_media_egress_edges` by exactly
/// this meeting's share, and every release returns both to their PREVIOUS
/// values — the gauges AND the routing snapshot they are published from.
/// Nothing else runs against this rig, so equality is sound here.
#[tokio::test]
async fn a_release_returns_registered_meetings_and_edges_to_their_previous_values() {
    let snap = MetricAssertion::snapshot();
    let mut rig = Rig::start(fixture_policy_limits()).await;
    // One count, two readings: the gauge and the routing snapshot it is
    // published from are both checked against the same `usize`.
    let edges = |rig: &Rig, snap: &MetricSnapshot, expected: usize| {
        snap.gauge("mh_media_egress_edges")
            .with_labels(&[KEY_CUSTODY])
            .assert_value(f64::from(u32::try_from(expected).unwrap()));
        assert_eq!(
            rig.session_manager.routing_snapshot().total_edges(),
            expected
        );
    };
    let two_edges = || vec![egress(1, 5, 0, 6), egress(2, 6, 0, 5)];
    let this_meeting_edges = two_edges().len();

    // Background load the lifecycle must never disturb.
    let background = vec![egress(1, 7, 0, 8)];
    let prev_edges = background.len();
    rig.register("m-background", "mc-other", 1, background)
        .await
        .unwrap();
    let prev_meetings = 1.0;
    registered_meetings(&snap, prev_meetings);
    edges(&rig, &snap, prev_edges);

    let held = |rig: &Rig, snap: &MetricSnapshot| {
        registered_meetings(snap, prev_meetings + 1.0);
        edges(rig, snap, prev_edges + this_meeting_edges);
    };
    let back_to_previous = |rig: &Rig, snap: &MetricSnapshot| {
        registered_meetings(snap, prev_meetings);
        // Exactly this meeting's edges came back.
        edges(rig, snap, prev_edges);
        let routes = rig.session_manager.routing_snapshot();
        assert!(routes.routes_for(&MeetingKey::new("m-test")).is_none());
        assert!(
            routes
                .routes_for(&MeetingKey::new("m-background"))
                .is_some(),
            "the background meeting is untouched"
        );
    };
    assert_eq!(
        rig.register("m-test", "mc-a", 5, two_edges())
            .await
            .unwrap(),
        5
    );
    held(&rig, &snap);

    // Stale probe (as env-test 29 does): OK, echoes 5, occupancy unchanged.
    assert_eq!(
        rig.register("m-test", "mc-a", 1, two_edges())
            .await
            .unwrap(),
        5
    );
    held(&rig, &snap);

    // A foreign release changes nothing.
    assert_eq!(
        rig.end("m-test", "mc-intruder").await.unwrap_err().code(),
        Code::FailedPrecondition
    );
    held(&rig, &snap);

    assert!(rig.end("m-test", "mc-a").await.unwrap(), "acknowledged");
    back_to_previous(&rig, &snap);

    // A repeat release is a no-op: still exactly the previous values.
    assert!(
        rig.end("m-test", "mc-a").await.unwrap(),
        "no-op acknowledged"
    );
    back_to_previous(&rig, &snap);

    // Re-created at generation 1: installs fresh and is counted exactly once
    // again (the release did not leave a phantom share behind).
    assert_eq!(
        rig.register("m-test", "mc-a", 1, two_edges())
            .await
            .unwrap(),
        1
    );
    held(&rig, &snap);
    assert!(rig.end("m-test", "mc-a").await.unwrap());
    back_to_previous(&rig, &snap);

    teardown_delta(&snap, "released", 2);
    teardown_delta(&snap, "rejected_ownership", 1);
    teardown_delta(&snap, "unknown_meeting", 1);
}

/// A released meeting re-created under the same id at generation 1 installs
/// fresh — exactly as it would on a pod that never saw it.
#[tokio::test]
async fn a_released_meeting_reregisters_at_generation_one_as_on_a_fresh_pod() {
    let mut rig = Rig::start(fixture_policy_limits()).await;
    assert_eq!(
        rig.register("m-1", "mc-a", 9, vec![egress(1, 5, 0, 6)])
            .await
            .unwrap(),
        9
    );
    rig.end("m-1", "mc-a").await.unwrap();
    assert_eq!(
        rig.register("m-1", "mc-a", 1, vec![egress(1, 5, 0, 6)])
            .await
            .unwrap(),
        1,
        "not rejected_stale: release forgot the generation"
    );
}

// ---------------------------------------------------------------------------
// Ownership reject
// ---------------------------------------------------------------------------

/// A different `mc_id` gets FAILED_PRECONDITION on the wire — never
/// PERMISSION_DENIED (MhAuthLayer's real-authorization code), never an
/// acknowledgement — and NOTHING is released: the gauge, routes and edges are
/// all where they were.
#[tokio::test]
async fn a_mismatched_mc_id_is_failed_precondition_and_releases_nothing() {
    let snap = MetricAssertion::snapshot();
    let mut rig = Rig::start(fixture_policy_limits()).await;
    rig.register("m-1", "mc-owner", 2, vec![egress(1, 5, 0, 6)])
        .await
        .unwrap();
    registered_meetings(&snap, 1.0);

    let status = rig
        .end("m-1", "mc-intruder")
        .await
        .expect_err("a mismatch is an error status, never an acknowledgement");

    assert_eq!(status.code(), Code::FailedPrecondition);
    assert_ne!(status.code(), Code::PermissionDenied);
    assert!(
        !status.message().contains("mc-owner"),
        "the wire status must not disclose the registering mc_id: {}",
        status.message()
    );
    registered_meetings(&snap, 1.0);
    let routes = rig.session_manager.routing_snapshot();
    assert!(routes.routes_for(&MeetingKey::new("m-1")).is_some());
    assert_eq!(routes.total_edges(), 1);
    teardown_delta(&snap, "rejected_ownership", 1);
    teardown_delta(&snap, "released", 0);
    snap.counter("mh_grpc_requests_total")
        .with_labels(&[("method", "end_meeting"), ("status", "error")])
        .assert_delta(1);
}

// ---------------------------------------------------------------------------
// Unknown meeting
// ---------------------------------------------------------------------------

/// Unknown and already-released meetings acknowledge, count
/// `unknown_meeting`, and change nothing — distinguishable from both the
/// release and the reject.
#[tokio::test]
async fn unknown_and_already_released_meetings_acknowledge_as_a_no_op() {
    let snap = MetricAssertion::snapshot();
    let mut rig = Rig::start(fixture_policy_limits()).await;
    rig.register("m-1", "mc-a", 1, vec![]).await.unwrap();
    rig.register("m-other", "mc-a", 1, vec![]).await.unwrap();
    rig.end("m-1", "mc-a").await.unwrap();
    registered_meetings(&snap, 1.0);

    assert!(rig.end("m-1", "mc-a").await.unwrap(), "already released");
    assert!(rig.end("never", "mc-a").await.unwrap(), "never registered");

    registered_meetings(&snap, 1.0);
    teardown_delta(&snap, "unknown_meeting", 2);
    teardown_delta(&snap, "released", 1);
    teardown_delta(&snap, "rejected_ownership", 0);
}

// ---------------------------------------------------------------------------
// Registered-meeting cap
// ---------------------------------------------------------------------------

/// A new meeting past `MH_MAX_REGISTERED_MEETINGS` is RESOURCE_EXHAUSTED on
/// the wire and counted by its OWN reason, exactly once; a held meeting still
/// re-asserts at the cap.
#[tokio::test]
async fn the_meeting_cap_refuses_a_new_meeting_loudly_and_counts_it_once() {
    let limits = PolicyLimits {
        max_registered_meetings: 2,
        ..fixture_policy_limits()
    };
    let snap = MetricAssertion::snapshot();
    let mut rig = Rig::start(limits).await;
    rig.register("m-1", "mc-a", 1, vec![]).await.unwrap();
    rig.register("m-2", "mc-a", 1, vec![]).await.unwrap();

    let status = rig.register("m-3", "mc-a", 1, vec![]).await.unwrap_err();
    assert_eq!(status.code(), Code::ResourceExhausted);
    assert!(!rig.session_manager.is_meeting_registered("m-3").await);
    registered_meetings(&snap, 2.0);
    snap.counter("mh_media_policy_applies_total")
        .with_labels(&[("outcome", "rejected_meeting_cap"), KEY_CUSTODY])
        .assert_delta(1);
    snap.counter("mh_grpc_requests_total")
        .with_labels(&[("method", "register_meeting"), ("status", "error")])
        .assert_delta(1);

    // A held meeting is never refused at the cap.
    assert_eq!(rig.register("m-1", "mc-a", 2, vec![]).await.unwrap(), 2);
}

// ---------------------------------------------------------------------------
// The two-value method label
// ---------------------------------------------------------------------------

/// Both `MediaHandlerService` RPCs are counted under their OWN `method` value
/// over real transport, and neither under the other's.
#[tokio::test]
async fn both_rpcs_are_counted_under_their_own_method_label() {
    let snap = MetricAssertion::snapshot();
    let mut rig = Rig::start(fixture_policy_limits()).await;

    rig.register("m-1", "mc-a", 1, vec![]).await.unwrap();
    rig.end("m-1", "mc-a").await.unwrap();
    rig.end("m-1", "mc-a").await.unwrap();

    snap.counter("mh_grpc_requests_total")
        .with_labels(&[("method", "register_meeting"), ("status", "success")])
        .assert_delta(1);
    snap.counter("mh_grpc_requests_total")
        .with_labels(&[("method", "end_meeting"), ("status", "success")])
        .assert_delta(2);
}
