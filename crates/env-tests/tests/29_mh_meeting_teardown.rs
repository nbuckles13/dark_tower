//! Env-test: MH meeting teardown on the live Kind cluster (story 2 task 11;
//! R-20, R-21; ADR-0036 §8, §11).
//!
//! # What this proves that nothing else can
//!
//! That a LONG-RUNNING handler pod, after a meeting ends, admits that meeting
//! exactly as a fresh pod would, on the deployed image and ConfigMap: the
//! registering MC's release is acknowledged, a foreign `mc_id` is refused with
//! `FAILED_PRECONDITION`, a repeat release is an acknowledged no-op, and the
//! teardown counter records each outcome on the real scrape path. The exact
//! occupancy accounting (registered meetings and egress edges return to their
//! previous values after a release) is proved in-process by
//! `crates/mh-service/tests/end_meeting_integration.rs`, where nothing else
//! shares the gauges.
//!
//! # This test registers NO edges, and that is load-bearing
//!
//! `egress_streams` is EMPTY in every registration here. The whole proof below
//! is about a meeting's GENERATION and its ownership, and neither needs a
//! forwarding policy: an empty policy at generation N is an ordinary apply that
//! echoes `applied_generation == N`.
//!
//! Edges would make this test depend on FREE SHARED CAPACITY. MH's egress
//! admission refuses a registration WHOLE when the projected installed streams
//! would exceed the pod's derived stream ceiling, and that projection sums every
//! meeting on the pod. Suites 26/27/28 share these pods, so a two-edge
//! registration is refused whenever they happen to be near the ceiling while
//! their meetings drain — which is exactly how this test failed at Gate 2
//! (mh-0 at 39 installed streams against a ceiling of 40,
//! `mh_media_policy_applies_total{outcome="rejected_stream_ceiling"}` = 4 on
//! that pod, `applied_generation` 0 instead of 5; the edges drained to 0 minutes
//! later, so nothing had leaked). With zero edges the projection is unchanged by
//! this test, so the ceiling can never refuse it: `projected == installed`, and
//! installed is never above the ceiling. The same class of cross-suite
//! interference the gauge assertions were removed for, reached through
//! admission instead — so the answer is the same: do not depend on the shared
//! resource at all. **Do not "strengthen" this test by giving it edges.**
//!
//! Second benefit, in the other direction: with no edges this test cannot push
//! a shared pod toward its ceiling either, so it never pollutes suite 28's
//! `rejected_stream_ceiling` signal.
//!
//! One consequence to leave alone: with no edges the registration declares no
//! transport mode, so `RegisterMeetingResponse.transport_mode` decodes as
//! `TRANSPORT_MODE_UNSPECIFIED`. That is correct for an empty policy — **do not
//! assert a transport mode anywhere in this test.** (MH's §8 declared-vs-applied
//! disagreement WARN also stays silent, since nothing was declared.)
//!
//! # The release proof is from MH's own replies, never a shared gauge
//!
//! `mh_media_registered_meetings` and `mh_media_egress_edges` are POD-WIDE, and
//! other suites register and release meetings on the same pods concurrently,
//! so their values move in both directions for reasons unrelated to this test.
//! No assertion here reads their value (Gate 2 attempt 2 of task 20 failed on
//! exactly that: the edge gauge drained from 31 to this test's own edges while
//! the test waited for "baseline + 2"). Instead, THIS meeting's state is observed
//! through the generation monotonicity MH already enforces on `RegisterMeeting`:
//!
//! 1. register at generation [`HELD_GENERATION`] with an empty policy;
//! 2. re-register at generation 1: refused as stale (`applied_generation`
//!    echoes [`HELD_GENERATION`]) — the meeting's state exists;
//! 3. the foreign `mc_id` release is refused, and the stale probe repeats —
//!    the reject released nothing;
//! 4. the owning MC's `EndMeeting` is acknowledged;
//! 5. re-register at generation 1: `applied_generation == 1`, which MH returns
//!    only if the held generation is gone — i.e. the meeting was released.
//!
//! The gauges are still checked for PRESENCE on the pinned pod (they are
//! published at 0 from process start, so absence is a failure). The LIMITS
//! gauges are asserted by value because they are published once from the
//! deployed ConfigMap and no suite can move them. Teardown COUNTERS are
//! asserted as `>= own baseline + 1`: a monotonic counter cannot be pushed
//! below that by concurrent suites.
//!
//! # How it reaches MH
//!
//! Directly, as the meeting-controller service principal: Layer 7
//! (`scripts/layer7.sh`, step (i)) forwards each MH POD's gRPC port and exports
//! `ENV_TEST_MH_{0,1}_GRPC_URL` plus `ENV_TEST_MH_{0,1}_POD_IP`. MC's own
//! `EndMeeting` is proved end to end through public APIs by env-test 35. This
//! suite drives the arms MC can never produce: a mismatched `mc_id` is
//! unreachable through MC at all (MC always sends its own), and the release is
//! observed at a generation this test controls, on a long-running pod. So both
//! arms need a direct call.
//! The token comes from AC's client-credentials endpoint for the Kind dev
//! `meeting-controller` client — never minted locally — narrowed to
//! `service.write.mh`. It is used only against MH and never printed.
//! The MH client is generated by `proto-gen` (a wire-contract crate): ADR-0028's
//! layering is unchanged; only the credential convention is.
//!
//! # Containment — this test must never touch a meeting it does not own
//!
//! Every meeting id is a fresh uuid, which is the containing key (the ownership
//! state is keyed on `meeting_id`). The only generation above 1 is
//! [`HELD_GENERATION`], and it is held only until this test's own release: a
//! generation cannot be taken back within a process EXCEPT by `EndMeeting`, so
//! an unreleased high generation would wedge that id on that pod for the
//! cluster's lifetime. Cleanup is STRUCTURAL —
//! a `Drop` guard releases everything this test registered, so a mid-test panic
//! cannot leak a registration into every later suite on the
//! cluster. `mc_grpc_endpoint` is a never-dialled placeholder: MH dials it only
//! for WebTransport clients of the meeting, and none joins a test meeting.
//!
//! # Reading metrics: pinned, relative, no wall clock
//!
//! Metrics are read through Prometheus — the suite's one metrics path — pinned
//! to the `instance` of the pod the test CALLED (resolved by Layer 7 from the
//! same lookup that feeds the forward), never "any instance". Prometheus
//! samples on an interval, so every read is a bounded poll until a condition,
//! never a sleep-then-assert. Each wait fails with its own phase token
//! (PRECONDITION / LIMITS / REJECT-COUNT / RELEASE-COUNT / UNKNOWN-COUNT), so a
//! slow scrape never reads as a reclamation bug. A registration refused for
//! shared CAPACITY (the registered-meeting cap — the only such refusal a
//! zero-edge policy can still meet) fails with its own `CAPACITY` token, never
//! the generic `PRECONDITION` one, so it is never triaged as a teardown bug.
//!
//! What stays SYNCHRONOUS, read off the gRPC response and never re-expressed as
//! a lagged metric: the reject's status code, the acknowledgements, and every
//! `applied_generation` (STALE-PROBE / RELEASE-NOT-PROVEN / FRESH-INSTALL).

#![cfg(feature = "flows")]

use std::time::Duration;

use env_tests::cluster::ClusterConnection;
use env_tests::fixtures::kube::{configmap_name_for_all, configmap_u64};
use env_tests::fixtures::metrics::{gauge_by_instance_present, poll_until_pinned_instance};
use env_tests::fixtures::mh_grpc::{
    authed, connect, handlers, mc_service_token, Handler, ReleaseOnDrop,
};
use env_tests::fixtures::PrometheusClient;
use proto_gen::dark_tower::internal::v1::media_handler_service_client::MediaHandlerServiceClient;
use proto_gen::dark_tower::internal::v1::{EndMeetingRequest, RegisterMeetingRequest};
use tonic::transport::Channel;
use tonic::Code;

/// Scrape-convergence bound for one metric wait (same bound as test 28).
const METRIC_BOUND: Duration = Duration::from_secs(120);
const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// Occupancy gauges: POD-WIDE and moved concurrently by other suites, so read
/// for PRESENCE only, never for a value (see the module docs).
const REGISTERED: &str = "max by (instance) (mh_media_registered_meetings)";
const EDGES: &str = "max by (instance) (mh_media_egress_edges)";

/// The generation the test meeting is registered at, so a generation-1
/// re-registration is observably STALE while the meeting is held and
/// observably FRESH once it is released. Held only until this test's own
/// release (see "Containment").
const HELD_GENERATION: u64 = 5;

fn teardowns(outcome: &str) -> String {
    format!("sum by (instance) (mh_media_meeting_teardowns_total{{outcome=\"{outcome}\"}})")
}

// Structural cleanup: `env_tests::fixtures::mh_grpc::ReleaseOnDrop` (hoisted
// at story 2 task 10; see that module for the mechanism).

// ---------------------------------------------------------------------------
// The test
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_long_running_mh_pod_releases_an_ended_meeting_and_admits_like_a_fresh_one() {
    let cluster = ClusterConnection::new()
        .await
        .expect("PRECONDITION: cluster connection (ENV_TEST_* URLs)");
    let prom = PrometheusClient::new(&cluster.prometheus_base_url);
    let handlers = handlers();

    // PRECONDITION — the AC-issued meeting-controller token, never minted here,
    // from the credentials MC itself is deployed with. A refusal names the
    // CREDENTIAL, so a rotation mismatch is not triaged as a broken forward.
    let token = mc_service_token(&cluster.ac_base_url).await;
    let cleanup = ReleaseOnDrop::new(token.clone());

    // LIMITS — both resource-guard limits are published from the value
    // enforcement reads, i.e. equal the DEPLOYED ConfigMap (never a literal here).
    // The live, content-addressed generation the MH pod runs (ADR-0038 §2).
    let mh_config = configmap_name_for_all(&["mh-0", "mh-1"], "mh-service-config");
    let edge_limit = configmap_u64(&mh_config, "MH_MAX_TOTAL_EGRESS_EDGES");
    let meeting_limit = configmap_u64(&mh_config, "MH_MAX_REGISTERED_MEETINGS");
    for (metric, deployed) in [
        ("mh_media_egress_edges_limit", edge_limit),
        ("mh_media_registered_meetings_limit", meeting_limit),
    ] {
        gauge_by_instance_present(&prom, metric, handlers.len(), METRIC_BOUND).await;
        for handler in &handlers {
            #[allow(clippy::cast_precision_loss)]
            let expected = deployed as f64;
            poll_until_pinned_instance(
                &prom,
                &format!("max by (instance) ({metric})"),
                &handler.pod_ip,
                METRIC_BOUND,
                POLL_INTERVAL,
                "LIMITS",
                &format!("{expected} (the deployed ConfigMap value)"),
                |v| (v - expected).abs() < f64::EPSILON,
            )
            .await;
        }
    }

    for handler in &handlers {
        exercise_one_handler(&prom, &cleanup, &token, handler).await;
    }
}

/// The whole lifecycle on ONE pod: register, prove held, reject a foreign
/// release, prove still held, release, no-op on repeat, then admit the same id
/// afresh — every per-meeting fact read off MH's own replies.
async fn exercise_one_handler(
    prom: &PrometheusClient,
    cleanup: &ReleaseOnDrop,
    token: &str,
    handler: &Handler,
) {
    let run = uuid::Uuid::new_v4().simple().to_string();
    let meeting_id = format!("env-test-teardown-{run}");
    let owner_mc = format!("env-test-mc-{run}");
    let intruder_mc = format!("env-test-intruder-{run}");
    let pinned = |promql: String,
                  phase: &'static str,
                  expectation: String,
                  pred: Box<dyn Fn(f64) -> bool + Send>| {
        let pod_ip = handler.pod_ip.clone();
        async move {
            poll_until_pinned_instance(
                prom,
                &promql,
                &pod_ip,
                METRIC_BOUND,
                POLL_INTERVAL,
                phase,
                &expectation,
                pred,
            )
            .await
        }
    };
    let name = &handler.name;
    let mut client = connect(handler).await;

    // PRESENCE on THIS pod. Every series here is published at 0 from process
    // start, so ABSENT is a failure, not zero. The occupancy gauges' VALUES are
    // never asserted (other suites move them); the counters' values are this
    // pod's baselines for the monotonic `>= baseline + 1` checks below.
    let any = |_: f64| true;
    for gauge in [REGISTERED, EDGES] {
        pinned(
            gauge.into(),
            "PRECONDITION",
            "presence".into(),
            Box::new(any),
        )
        .await;
    }
    let base_rejected = pinned(
        teardowns("rejected_ownership"),
        "PRECONDITION",
        "presence".into(),
        Box::new(any),
    )
    .await;
    let base_released = pinned(
        teardowns("released"),
        "PRECONDITION",
        "presence".into(),
        Box::new(any),
    )
    .await;
    let base_unknown = pinned(
        teardowns("unknown_meeting"),
        "PRECONDITION",
        "presence".into(),
        Box::new(any),
    )
    .await;

    // 1. Register at HELD_GENERATION with an EMPTY policy (module docs: "This
    //    test registers NO edges"). The auth positive control: an error status
    //    here is the token or the forward, never teardown logic.
    cleanup.record(&handler.grpc_url, &meeting_id, &owner_mc);
    let registered = client
        .register_meeting(authed(
            token,
            register(&meeting_id, &owner_mc, HELD_GENERATION),
        ))
        .await
        .unwrap_or_else(|s| {
            // RESOURCE_EXHAUSTED is SHARED CAPACITY, not a teardown fault: with
            // zero edges the only capacity refusal left is the registered-meeting
            // cap, which other suites' meetings can fill. Its own token, so it is
            // never triaged as a teardown bug.
            let phase = if s.code() == Code::ResourceExhausted {
                "CAPACITY"
            } else {
                "PRECONDITION"
            };
            panic!(
                "{phase}: {name} refused the first RegisterMeeting ({:?}: {}). CAPACITY means the \
                 pod holds MH_MAX_REGISTERED_MEETINGS registrations (shared with every other \
                 suite); PRECONDITION means the token or the forward. Neither is teardown.",
                s.code(),
                s.message()
            )
        })
        .into_inner();
    assert_eq!(
        registered.applied_generation, HELD_GENERATION,
        "PRECONDITION: {name} echoed applied_generation {} for a ZERO-EDGE generation-\
         {HELD_GENERATION} register. A zero-edge policy cannot be refused on edge capacity (the \
         projection is unchanged, and both admission arms compare with strict `>`), so suspect \
         config-apply back-pressure on a busy shared pod — MH WARNs with \
         reason=config_apply_mailbox_full or config_apply_timeout and answers OK with the PRIOR \
         generation (0 when nothing was ever installed). Not the egress stream ceiling, and not \
         teardown.",
        registered.applied_generation
    );

    // 2. HELD: generation 1 is refused as stale. The positive control for step
    //    6 — without it, "generation 1 installs" could be a pod that never
    //    retained the meeting at all.
    assert_stale(
        &mut client,
        token,
        name,
        &meeting_id,
        &owner_mc,
        "after register",
    )
    .await;

    // 3. A DIFFERENT mc_id: FAILED_PRECONDITION (never PERMISSION_DENIED, never an
    //    acknowledgement), counted, and nothing released. EndMeeting ONLY: an
    //    intruder RegisterMeeting would rebind ownership.
    let rejected = client
        .end_meeting(authed(
            token,
            EndMeetingRequest {
                meeting_id: meeting_id.clone(),
                mc_id: intruder_mc,
            },
        ))
        .await
        .expect_err("REJECT: a mismatched mc_id must be an error status, never an acknowledgement");
    assert_eq!(
        rejected.code(),
        Code::FailedPrecondition,
        "REJECT on {name}"
    );
    assert_ne!(rejected.code(), Code::PermissionDenied, "REJECT on {name}");
    assert!(
        !rejected.message().contains(&owner_mc),
        "REJECT on {name}: the status must not disclose the registering mc_id"
    );
    // Still held: the rejected release released nothing.
    assert_stale(
        &mut client,
        token,
        name,
        &meeting_id,
        &owner_mc,
        "after the rejected release",
    )
    .await;
    pinned(
        teardowns("rejected_ownership"),
        "REJECT-COUNT",
        format!(">= {}", base_rejected + 1.0),
        Box::new(move |v| v >= base_rejected + 1.0),
    )
    .await;

    // 4. The registering MC releases: acknowledged and counted.
    let released = client
        .end_meeting(authed(
            token,
            EndMeetingRequest {
                meeting_id: meeting_id.clone(),
                mc_id: owner_mc.clone(),
            },
        ))
        .await
        .unwrap_or_else(|s| panic!("RELEASE on {name}: {:?}: {}", s.code(), s.message()))
        .into_inner();
    assert!(released.acknowledged, "RELEASE on {name}: not acknowledged");
    cleanup.forget(&meeting_id);
    pinned(
        teardowns("released"),
        "RELEASE-COUNT",
        format!(">= {}", base_released + 1.0),
        Box::new(move |v| v >= base_released + 1.0),
    )
    .await;

    // 5. Releasing again: an acknowledged no-op, counted as unknown_meeting.
    let repeat = client
        .end_meeting(authed(
            token,
            EndMeetingRequest {
                meeting_id: meeting_id.clone(),
                mc_id: owner_mc.clone(),
            },
        ))
        .await
        .unwrap_or_else(|s| panic!("UNKNOWN on {name}: {:?}: {}", s.code(), s.message()))
        .into_inner();
    assert!(
        repeat.acknowledged,
        "UNKNOWN on {name}: a released meeting must acknowledge"
    );
    pinned(
        teardowns("unknown_meeting"),
        "UNKNOWN-COUNT",
        format!(">= {}", base_unknown + 1.0),
        Box::new(move |v| v >= base_unknown + 1.0),
    )
    .await;

    // 6. THE RELEASE PROOF, for THIS meeting: the same id at generation 1
    //    installs, exactly as on a pod that never saw it. Step 2 showed the
    //    same request refused as stale while the meeting was held, so an
    //    install here happens only if the release dropped the meeting's state.
    cleanup.record(&handler.grpc_url, &meeting_id, &owner_mc);
    let again = client
        .register_meeting(authed(token, register(&meeting_id, &owner_mc, 1)))
        .await
        .unwrap_or_else(|s| {
            // Same install mechanics as step 1, so the same split: shared
            // capacity is never reported as a teardown fault.
            let phase = if s.code() == Code::ResourceExhausted {
                "CAPACITY"
            } else {
                "FRESH-INSTALL"
            };
            panic!(
                "{phase}: {name} refused the post-release RegisterMeeting ({:?}: {}). CAPACITY \
                 means the pod holds MH_MAX_REGISTERED_MEETINGS registrations (shared with every \
                 other suite), not that the release failed.",
                s.code(),
                s.message()
            )
        })
        .into_inner();
    // The two wrong answers mean OPPOSITE things, so they are separated: only an
    // echo of HELD_GENERATION is evidence about teardown.
    assert_ne!(
        again.applied_generation, 0,
        "BACK-PRESSURE on {name}: the post-release generation-1 register echoed 0, i.e. MH \
         installed NOTHING and reported the prior generation of a just-released meeting. On a \
         busy shared pod that is config-apply back-pressure (MH WARNs with \
         reason=config_apply_mailbox_full or config_apply_timeout), NOT a teardown fault — the \
         release was already acknowledged and counted above. Re-run; if it persists, read MH's \
         config-apply WARNs on this pod."
    );
    assert_eq!(
        again.applied_generation, 1,
        "RELEASE-NOT-PROVEN on {name}: a released meeting re-created at generation 1 must install \
         as on a fresh pod. An echo of {HELD_GENERATION} means the release left the meeting's \
         state behind — a real teardown regression. (An echo of 0 is back-pressure, not teardown, \
         and is caught by the assertion above.)"
    );
    let done = client
        .end_meeting(authed(
            token,
            EndMeetingRequest {
                meeting_id: meeting_id.clone(),
                mc_id: owner_mc,
            },
        ))
        .await
        .unwrap_or_else(|s| panic!("CLEANUP on {name}: {:?}: {}", s.code(), s.message()))
        .into_inner();
    assert!(done.acknowledged, "CLEANUP on {name}");
    cleanup.forget(&meeting_id);
}

/// Re-register the HELD meeting at generation 1 under its OWNER (a foreign
/// `mc_id` would rebind ownership) and require MH to refuse it as stale: an
/// OK reply echoing [`HELD_GENERATION`]. That echo exists only while MH holds
/// this meeting's state.
async fn assert_stale(
    client: &mut MediaHandlerServiceClient<Channel>,
    token: &str,
    name: &str,
    meeting_id: &str,
    owner_mc: &str,
    when: &str,
) {
    let stale = client
        .register_meeting(authed(token, register(meeting_id, owner_mc, 1)))
        .await
        .unwrap_or_else(|s| {
            panic!(
                "STALE-PROBE on {name} ({when}): a stale re-registration is an OK reply, not {:?}: {}",
                s.code(),
                s.message()
            )
        })
        .into_inner();
    assert_eq!(
        stale.applied_generation, HELD_GENERATION,
        "STALE-PROBE on {name} ({when}): generation 1 must be refused as stale while the meeting is held at generation {HELD_GENERATION}; any other echo means MH does not hold this meeting's state"
    );
}

/// Test 29's probe: the shared fixture (see `probe_registration` for the
/// empty-edges and never-generation-0 decisions this test documents).
fn register(meeting_id: &str, mc_id: &str, generation: u64) -> RegisterMeetingRequest {
    env_tests::fixtures::mh_grpc::probe_registration(meeting_id, mc_id, generation)
}
