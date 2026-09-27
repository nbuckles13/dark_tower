//! Env-test: a roster departure rotates the meeting KEK on the live Kind cluster
//! (story 2 task 9 — scenario S6, MC half; R-12, R-14, R-26; ADR-0036 §4
//! Rotation).
//!
//! Two real users join one meeting through GC and MC's real WebTransport path.
//! B leaves with a clean close. A must then receive a `MeetingKekUpdate` at the
//! NEXT generation carrying W — and the rotation must be visible on MC's own
//! counters in Prometheus.
//!
//! # No assertion is made on wall-clock W
//!
//! The rotation fires W after the departure (the debounce is measured from the
//! oldest un-rotated removal). The test does NOT assert WHEN it fires. W is
//! read from `mc_meeting_kek_rotation_window_seconds` — the value the timer
//! enforces, never a literal — and used only to bound the wait, plus a margin.
//! A ConfigMap change to W therefore cannot erode the margin.
//!
//! # The frame is the positive control, and its absence is a HARD failure
//!
//! Waiting past the deadline without a `MeetingKekUpdate` panics. It never
//! falls through to a skipped or softened assertion: the counters below are
//! read only after the frame has arrived.
//!
//! # Counters on a shared cluster
//!
//! Other suites may rotate KEKs concurrently, so the counters are asserted as
//! "some instance rose above its OWN settled baseline", never as an exact
//! delta. The frame at the right generation is what attributes the rotation to
//! THIS meeting; the counters prove it was recorded.
//!
//! Registration cost: two AC registrations per run.

#![cfg(feature = "flows")]

use env_tests::cluster::ClusterConnection;
use env_tests::fixtures::auth_client::UserRegistrationRequest;
use env_tests::fixtures::gc_client::{CreateMeetingRequest, GcClient, JoinMeetingResponse};
use env_tests::fixtures::mc_session::{self, McSession};
use env_tests::fixtures::metrics::{
    format_instance_map, gauge_by_instance_present, poll_until_any_instance_above,
    poll_until_stable, service_job_scrape_settle, InstanceCounters, PrometheusClient,
};
use env_tests::fixtures::AuthClient;
use proto_gen::dark_tower::signaling::v1::{server_message, JoinResponse, MeetingKekUpdate};
use std::time::{Duration, Instant};

/// Margin over W before the rotation frame must have arrived: covers the
/// actor's wake, the push fan-out and the stream write. A rig bound, not a
/// performance assertion.
const ROTATION_MARGIN: Duration = Duration::from_secs(30);

/// How long to wait for MC's window gauge to be scraped.
const GAUGE_SCRAPE_BOUND: Duration = Duration::from_secs(60);

/// Per-read bound while waiting for frames; each read that times out is quiet,
/// not a failure, until the overall deadline.
const READ_BOUND: Duration = Duration::from_secs(5);

/// Counter stability budget. The SETTLE is not a local constant: both KEK
/// counters are MC series, scraped by the `mc-service` job, so the wait uses
/// `SERVICE_JOB_SCRAPE_SETTLE` through `service_job_scrape_settle` (one scrape
/// interval of that job plus margin, checked against the LIVE config — a settle
/// inside one scrape would make the baseline vacuous). The budget is a
/// failure-only ceiling: several non-equal rounds of that settle plus a
/// cluster-load allowance; it is not scaled with the interval, because a faster
/// scrape does not make a loaded cluster converge sooner.
const COUNTER_BUDGET: Duration = Duration::from_secs(90);

const ROTATIONS_PROMQL: &str =
    "sum by(instance) (mc_meeting_kek_generated_total{trigger=\"participant_left\"})";
const DELIVERED_PROMQL: &str =
    "sum by(instance) (mc_meeting_kek_pushes_total{outcome=\"delivered\"})";

async fn cluster() -> ClusterConnection {
    let cluster = ClusterConnection::new()
        .await
        .expect("Failed to connect to cluster - ensure port-forwards are running");
    cluster.check_ac_health().await.expect("AC must be running");
    cluster.check_gc_health().await.expect("GC must be running");
    cluster
}

/// W as MC enforces it, from the gauge MC publishes from the same value.
async fn rotation_window(prom: &PrometheusClient) -> Duration {
    let by_instance = gauge_by_instance_present(
        prom,
        "mc_meeting_kek_rotation_window_seconds",
        1,
        GAUGE_SCRAPE_BOUND,
    )
    .await;
    let seconds = by_instance.values().copied().fold(0.0_f64, f64::max);
    assert!(
        seconds > 0.0,
        "mc_meeting_kek_rotation_window_seconds must be positive (W is required and bounded at \
         MC config load); got {by_instance:?}"
    );
    Duration::from_secs_f64(seconds)
}

async fn settled_baseline(prom: &PrometheusClient, promql: &'static str) -> InstanceCounters {
    let settle = service_job_scrape_settle(prom).await;
    poll_until_stable(prom, promql, settle, COUNTER_BUDGET, |v1, v2| {
        format!(
            "{promql} did not settle within {COUNTER_BUDGET:?} (last reads: v1={}, v2={})",
            format_instance_map(v1),
            format_instance_map(v2),
        )
    })
    .await
}

async fn join(
    gc: &JoinMeetingResponse,
    display_name: &str,
    label: &str,
) -> (McSession, JoinResponse) {
    let url = gc
        .mc_assignment
        .webtransport_endpoint
        .clone()
        .expect("MC assignment must include webtransport_endpoint");
    let mut session = McSession::connect(&url).await;
    let response = mc_session::mc_join(
        &mut session,
        &gc.meeting_id.to_string(),
        &gc.token,
        display_name,
        label,
    )
    .await;
    (session, response)
}

/// Read frames until a `MeetingKekUpdate` arrives or `deadline` passes.
async fn await_kek_update(session: &mut McSession, deadline: Instant) -> Option<MeetingKekUpdate> {
    while Instant::now() < deadline {
        if let Some(msg) = session.try_read(READ_BOUND).await {
            if let Some(server_message::Message::MeetingKekUpdate(update)) = msg.message {
                return Some(update);
            }
        }
    }
    None
}

#[tokio::test]
async fn a_departure_rotates_the_meeting_kek_for_the_remaining_member() {
    let cluster = cluster().await;
    let prom = PrometheusClient::new(&cluster.prometheus_base_url);
    let w = rotation_window(&prom).await;
    let auth = AuthClient::new(&cluster.ac_base_url);
    let gc = GcClient::new(&cluster.gc_base_url);

    // Baselines FIRST, settled, so a later rise is attributable to this run's
    // action rather than to a pre-existing instance missing from a single read.
    let rotations_before = settled_baseline(&prom, ROTATIONS_PROMQL).await;
    let delivered_before = settled_baseline(&prom, DELIVERED_PROMQL).await;

    let mut users = Vec::new();
    for label in ["kek-a", "kek-b"] {
        let request = UserRegistrationRequest::unique(format!("KEK rotation {label}"));
        let display = request.display_name.clone();
        let registered = auth
            .register_user(&request)
            .await
            .expect("AC should register a test user");
        users.push((registered.access_token, display));
    }
    let created = gc
        .create_meeting(
            &users[0].0,
            &CreateMeetingRequest::new("KEK rotation on leave"),
        )
        .await
        .expect("GC should create the meeting");
    let gc_a = gc
        .join_meeting(&created.meeting_code, &users[0].0)
        .await
        .expect("GC should issue A a meeting token");
    let gc_b = gc
        .join_meeting(&created.meeting_code, &users[1].0)
        .await
        .expect("GC should issue B a meeting token");

    let (mut a, a_join) = join(&gc_a, &users[0].1, "kek-a").await;
    let (b, _b_join) = join(&gc_b, &users[1].1, "kek-b").await;

    assert_eq!(
        a_join.meeting_kek.len(),
        32,
        "PRECONDITION: A was admitted with a provisioned AES-256 KEK"
    );
    assert_eq!(
        Duration::from_secs(u64::from(a_join.kek_rotation_debounce_seconds)),
        w,
        "the JoinResponse carries W from the SAME value the window gauge publishes"
    );

    // B leaves with a clean close: MC removes it immediately (no grace), and
    // the removal schedules the debounced rotation.
    b.connection()
        .close(wtransport::VarInt::from_u32(0), b"leave");

    let deadline = Instant::now() + w + ROTATION_MARGIN;
    let update = await_kek_update(&mut a, deadline).await.unwrap_or_else(|| {
        panic!(
            "A received NO MeetingKekUpdate within W ({w:?}) + {ROTATION_MARGIN:?} of B's \
             departure. The leave-debounced rotation did not fire or did not reach the remaining \
             member. Check mc_meeting_kek_rotation_pending_age_seconds against \
             mc_meeting_kek_rotation_overdue_threshold_seconds, \
             mc_meeting_kek_rotation_failures_total{{reason}} and \
             mc_meeting_kek_pushes_total{{outcome}} (mc-incident-response.md Scenario 19)."
        )
    });

    assert_eq!(
        update.kek_generation,
        a_join.kek_generation + 1,
        "a departure advances the KEK generation by exactly one"
    );
    assert_eq!(update.meeting_kek.len(), 32, "the rotated KEK is AES-256");
    assert_ne!(
        update.meeting_kek, a_join.meeting_kek,
        "a rotation that keeps the key is not a rotation"
    );
    assert_eq!(
        Duration::from_secs(u64::from(update.kek_rotation_debounce_seconds)),
        w,
        "W rides every MeetingKekUpdate, from the value the timer enforces"
    );

    poll_until_any_instance_above(
        &prom,
        ROTATIONS_PROMQL,
        &rotations_before,
        COUNTER_BUDGET,
        Duration::from_secs(5),
        |current| {
            format!(
                "the rotation A observed was never recorded: {ROTATIONS_PROMQL} rose on no \
                 instance (baseline {}, now {})",
                format_instance_map(&rotations_before),
                format_instance_map(current),
            )
        },
    )
    .await;
    poll_until_any_instance_above(
        &prom,
        DELIVERED_PROMQL,
        &delivered_before,
        COUNTER_BUDGET,
        Duration::from_secs(5),
        |current| {
            format!(
                "the push A received was never recorded: {DELIVERED_PROMQL} rose on no instance \
                 (baseline {}, now {})",
                format_instance_map(&delivered_before),
                format_instance_map(current),
            )
        },
    )
    .await;

    a.connection()
        .close(wtransport::VarInt::from_u32(0), b"leave");
}
