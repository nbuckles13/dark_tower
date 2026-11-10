//! Env-test: the per-meeting MH -> MC heartbeat against the REAL MC (story 4
//! MH-2; ADR-0039 §1, §6; R-7, R-10, R-34).
//!
//! One live meeting. Its holders are resolved from PER-ENTITY evidence — the
//! participant's `JoinResponse.media_handler_set` urls, mapped to pods through
//! the slot ConfigMaps (`mh_address::pods_serving`) — never "all MH pods".
//!
//! - EVERY holder's `mh_mc_heartbeats_total{outcome="success"}` rises above its
//!   own pinned, settled baseline. A shared counter rising is weak evidence
//!   alone (README evidence rule), so:
//! - the meeting is proven STILL HELD by each holder after more than two
//!   heartbeat intervals, per entity: a wrong-owner `EndMeeting` for THIS
//!   meeting id (cause `MEETING_ENDED`, so the cause check passes and the
//!   ownership check is what answers) is refused exactly `FAILED_PRECONDITION`
//!   — an unknown meeting would be acknowledged instead. MC answered OWNED and
//!   MH released nothing.
//! - Secondary only: no `mh_media_handler_releases_total` series of a holder
//!   rose while the meeting was owned. Suites share pods and others may release
//!   meetings, so this is read as "did not rise for any trigger with clients",
//!   never as the per-entity proof.
//!
//! The zombie leg (a declared-dead handler answered RELEASED) belongs to MC-B2.
//!
//! # Timing
//!
//! The heartbeat interval is DERIVED from MC's published
//! `mc_media_handler_heartbeat_deadline_seconds` (N; interval = N/3), never a
//! literal and never a config file. Every bound below is a failure bound
//! applied only on failure, never a latency target.
//!
//! Edge-sum (concurrent Σ P×(N+M)): 0

#![cfg(feature = "flows")]

use std::time::{Duration, Instant};

use env_tests::cluster::ClusterConnection;
use env_tests::fixtures::gc_client::GcClient;
use env_tests::fixtures::metrics::{
    instance_for_pod_ip, mc_heartbeat_interval, poll_until_pinned_instance,
    service_job_scrape_settle, settled_baseline, BASELINE_SETTLE_BOUND, PINNED_METRIC_BOUND,
};
use env_tests::fixtures::mh_address::{live_mh_addresses_by_pod, pods_serving};
use env_tests::fixtures::mh_grpc::{self, authed};
use env_tests::fixtures::participant::meeting_of;
use env_tests::fixtures::{AuthClient, PrometheusClient};
use proto_gen::dark_tower::internal::v1::{EndMeetingRequest, ReleaseCause};
use tonic::Code;

const SUCCESS_PROMQL: &str = "sum by (instance) (mh_mc_heartbeats_total{outcome=\"success\"})";
const RELEASES_PROMQL: &str = "sum by (instance) (mh_media_handler_releases_total)";

async fn cluster() -> ClusterConnection {
    let cluster = ClusterConnection::new()
        .await
        .expect("Failed to connect to cluster - ensure port-forwards are running");
    cluster.check_ac_health().await.expect("AC must be running");
    cluster.check_gc_health().await.expect("GC must be running");
    cluster
}

#[tokio::test]
async fn every_holder_heartbeats_the_real_mc_and_keeps_the_owned_meeting() {
    let cluster = cluster().await;
    let prom = PrometheusClient::new(&cluster.prometheus_base_url);
    let interval = mc_heartbeat_interval(&prom).await;
    let scrape = service_job_scrape_settle(&prom).await;

    // Baselines first. The success counter moves every interval on any pod
    // holding ANY live meeting, so it never "settles": one read, and the rise
    // below is supporting evidence only (the per-entity proof is step 2). The
    // release counter is settled, because "did not rise" needs a stable base.
    let success_before = prom.instance_counter_map(SUCCESS_PROMQL).await;
    let releases_before = settled_baseline(
        &prom,
        RELEASES_PROMQL,
        BASELINE_SETTLE_BOUND,
        "the release count under test",
    )
    .await;

    let auth = AuthClient::new(&cluster.ac_base_url);
    let gc = GcClient::new(&cluster.gc_base_url);
    let ps = meeting_of(&auth, &gc, "MH Meeting Heartbeat", &["A"]).await;
    let owner = &ps[0];
    let meeting_id = owner.gc.meeting_id.to_string();
    let owner_mc = owner.gc.mc_assignment.mc_id.clone();
    let holder_pods = pods_serving(owner.handler_set(), &live_mh_addresses_by_pod().await);
    assert!(
        !holder_pods.is_empty(),
        "PRECONDITION: the meeting's handler set is empty on a healthy cluster"
    );
    let holders: Vec<mh_grpc::Handler> = mh_grpc::handlers()
        .await
        .into_iter()
        .filter(|h| holder_pods.contains(&h.name))
        .collect();
    assert_eq!(holders.len(), holder_pods.len(), "every holder pod is live");

    // (1) Every holder's success counter rises above its own baseline.
    let rise_bound = interval * 3 + scrape * 2;
    for h in &holders {
        let before = instance_for_pod_ip(&success_before, &h.pod_ip).map_or(0.0, |(_, v)| v);
        poll_until_pinned_instance(
            &prom,
            SUCCESS_PROMQL,
            &h.pod_ip,
            rise_bound.max(PINNED_METRIC_BOUND),
            Duration::from_secs(2),
            "HEARTBEAT-RISE",
            &format!("> baseline {before} on holder {}", h.name),
            |v| v > before,
        )
        .await;
    }

    // (2) Still held after more than two intervals: per entity, by the
    // ownership check's answer for THIS meeting id.
    let held_after = Instant::now() + interval * 2 + interval / 2;
    while Instant::now() < held_after {
        tokio::time::sleep(interval / 4).await;
    }
    let token = mh_grpc::mc_service_token(&cluster.ac_base_url).await;
    for h in &holders {
        let mut client = mh_grpc::connect(h).await;
        let refused = client
            .end_meeting(authed(
                &token,
                EndMeetingRequest {
                    meeting_id: meeting_id.clone(),
                    mc_id: format!("{owner_mc}-not-the-owner"),
                    cause: ReleaseCause::MeetingEnded as i32,
                },
            ))
            .await
            .expect_err("HELD: a wrong-owner EndMeeting must be refused, never acknowledged");
        assert_eq!(
            refused.code(),
            Code::FailedPrecondition,
            "HELD on {}: FAILED_PRECONDITION means the meeting is still registered (an \
             unknown meeting is acknowledged); a released meeting would answer OK",
            h.name
        );
    }

    // (3) Secondary: no holder's release counter rose while the meeting was
    // owned (other suites may release on shared pods; this is not the proof).
    let releases_now = settled_baseline(
        &prom,
        RELEASES_PROMQL,
        BASELINE_SETTLE_BOUND,
        "the release count under test",
    )
    .await;
    for h in &holders {
        let before = instance_for_pod_ip(&releases_before, &h.pod_ip).map_or(0.0, |(_, v)| v);
        let now = instance_for_pod_ip(&releases_now, &h.pod_ip).map_or(0.0, |(_, v)| v);
        assert!(
            now <= before,
            "RELEASES on {}: mh_media_handler_releases_total rose ({before} -> {now}) while the \
             meeting was owned. Check the MH log for 'Meeting released' and its release_kind",
            h.name
        );
    }

    // Positive arm: the REAL owner's EndMeeting is what ends it (MC's own
    // teardown on leave sends the same; here it is driven directly).
    for h in &holders {
        let mut client = mh_grpc::connect(h).await;
        let ack = client
            .end_meeting(authed(
                &token,
                EndMeetingRequest {
                    meeting_id: meeting_id.clone(),
                    mc_id: owner_mc.clone(),
                    cause: ReleaseCause::MeetingEnded as i32,
                },
            ))
            .await
            .expect("the registered owner's EndMeeting is acknowledged")
            .into_inner();
        assert!(ack.acknowledged, "OWNER-END on {}", h.name);
    }
    drop(ps);
}
