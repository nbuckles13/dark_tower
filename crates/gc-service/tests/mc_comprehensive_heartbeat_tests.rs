// Every `#[sqlx::test]` is current_thread; `MetricAssertion` is thread-bound,
// so that pinning is LOAD-BEARING.
//
//! MC `ComprehensiveHeartbeat` usage readings, driven through the real
//! `McService::comprehensive_heartbeat`: unset readings are accepted (no
//! reading this interval), and an invalid present reading is discarded field
//! by field — counted in `gc_usage_readings_discarded_total{source="mc"}` —
//! while the heartbeat itself (capacity, liveness) still applies.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "common/mod.rs"]
mod test_common;

use ::common::observability::testing::MetricAssertion;
use gc_service::grpc::McService;
use gc_service::repositories::MeetingControllersRepository;
use proto_gen::dark_tower::internal::v1::global_controller_service_server::GlobalControllerService;
use proto_gen::dark_tower::internal::v1::{
    ComprehensiveHeartbeatRequest, ControllerCapacity, HealthStatus,
};
use sqlx::PgPool;
use std::time::Duration;
use test_common::mc_grpc::{mc_app_state, seed_mc};
use tonic::Request;

const REGION: &str = "us-east-1";
const MC: &str = "mc-1";
const METRIC: &str = "gc_usage_readings_discarded_total";

fn svc(pool: &PgPool) -> McService {
    McService::new(mc_app_state(pool.clone(), None))
}

fn heartbeat(
    current_meetings: u32,
    cpu: Option<f32>,
    memory: Option<f32>,
) -> Request<ComprehensiveHeartbeatRequest> {
    Request::new(ComprehensiveHeartbeatRequest {
        controller_id: MC.to_string(),
        capacity: Some(ControllerCapacity {
            max_meetings: 100,
            current_meetings,
            max_participants: 1000,
            current_participants: 0,
        }),
        health: HealthStatus::Healthy.into(),
        cpu_usage_percent: cpu,
        memory_usage_percent: memory,
    })
}

#[sqlx::test(migrations = "../../migrations")]
async fn unset_readings_are_accepted(pool: PgPool) {
    seed_mc(&pool, MC, REGION).await;
    let snap = MetricAssertion::snapshot();

    let response = svc(&pool)
        .comprehensive_heartbeat(heartbeat(3, None, None))
        .await
        .unwrap();

    assert!(response.into_inner().acknowledged);
    snap.counter(METRIC).assert_delta(0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn invalid_reading_is_discarded_and_the_heartbeat_still_applies(pool: PgPool) {
    seed_mc(&pool, MC, REGION).await;
    let before = MeetingControllersRepository::get_controller(&pool, MC)
        .await
        .unwrap()
        .unwrap();
    tokio::time::sleep(Duration::from_millis(5)).await;

    for (bad, reason) in [(f32::NAN, "non_finite"), (100.5, "out_of_range")] {
        let snap = MetricAssertion::snapshot();
        let response = svc(&pool)
            .comprehensive_heartbeat(heartbeat(7, Some(bad), Some(40.0)))
            .await
            .unwrap();
        assert!(
            response.into_inner().acknowledged,
            "{bad}: heartbeat accepted"
        );
        snap.counter(METRIC)
            .with_labels(&[("source", "mc"), ("field", "cpu"), ("reason", reason)])
            .assert_delta(1);
        snap.counter(METRIC).assert_delta(1);
    }

    let after = MeetingControllersRepository::get_controller(&pool, MC)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after.current_meetings, 7, "capacity still applied");
    assert!(
        after.last_heartbeat_at > before.last_heartbeat_at,
        "liveness still refreshed"
    );
}
