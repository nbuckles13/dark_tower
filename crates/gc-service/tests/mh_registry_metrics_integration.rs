// Every `#[sqlx::test]` is implicitly `flavor = "current_thread"` per the
// sqlx-test macro contract, and the pinning is LOAD-BEARING — `MetricAssertion`
// binds a per-thread recorder. See
// `crates/common/src/observability/testing.rs` for the isolation model.
//
//! Component tests (ADR-0032) for the GC MH-registry series (ADR-0039 §8, §10):
//!
//! - `gc_mh_load_reports_total{outcome=accepted|unknown_handler}` — driven
//!   through the real `MhService::send_load_report` recording sites.
//! - `gc_mh_available{region, state}` — driven through the real health-checker
//!   tick (`mh_registry_tick`): per-state values, stale → `unhealthy`, the
//!   `other` region bucket, and recompute-and-set (a state that empties reads
//!   0; nothing accumulates across ticks).
//! - `gc_mh_load_report_interval_seconds`, `gc_mh_staleness_threshold_seconds`
//!   — equal the `MhRegistryConfig` values they are set from.

#[path = "common/mod.rs"]
mod test_common;

use std::sync::Arc;

use ::common::observability::testing::{MetricAssertion, MetricSnapshot};
use gc_service::grpc::MhService;
use gc_service::observability::metrics;
use gc_service::repositories::MediaHandlersRepository;
use gc_service::tasks::mh_health_checker::mh_registry_tick;
use proto_gen::dark_tower::internal::v1::media_handler_registry_service_server::MediaHandlerRegistryService;
use proto_gen::dark_tower::internal::v1::SendLoadReportRequest;
use sqlx::PgPool;
use test_common::mh_registry::{freshness_registry, ms, seed};
use tonic::Request;

const OWN_REGION: &str = "test-region";
const STATES: [&str; 5] = ["pending", "healthy", "degraded", "unhealthy", "draining"];

fn load_report(id: &str) -> Request<SendLoadReportRequest> {
    Request::new(SendLoadReportRequest {
        handler_id: id.to_string(),
        current_streams: 3,
        health: 1,
        cpu_usage_percent: 0.0,
        memory_usage_percent: 0.0,
        bandwidth_usage_percent: 0.0,
    })
}

fn assert_available(snap: &MetricSnapshot, region: &str, expected: [f64; 5]) {
    for (state, value) in STATES.iter().zip(expected) {
        snap.gauge("gc_mh_available")
            .with_labels(&[("region", region), ("state", state)])
            .assert_value(value);
    }
}

// ---------------------------------------------------------------------------
// gc_mh_load_reports_total
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn load_report_for_registered_handler_counts_accepted(pool: PgPool) {
    let service = MhService::new(
        Arc::new(pool.clone()),
        freshness_registry(2).load_report_interval_ms(),
    );
    seed(&pool, "live-mh", OWN_REGION, "healthy", 0).await;

    let snap = MetricAssertion::snapshot();
    service
        .send_load_report(load_report("live-mh"))
        .await
        .unwrap();

    snap.counter("gc_mh_load_reports_total")
        .with_labels(&[("outcome", "accepted")])
        .assert_delta(1);
    snap.counter("gc_mh_load_reports_total")
        .with_labels(&[("outcome", "unknown_handler")])
        .assert_unobserved();
}

/// A row deleted by retention: the next load report is NOT_FOUND, counts
/// `unknown_handler` (a live process GC has no row for), and recreates nothing.
#[sqlx::test(migrations = "../../migrations")]
async fn load_report_after_retention_delete_counts_unknown_handler(pool: PgPool) {
    let reg = freshness_registry(2);
    let service = MhService::new(Arc::new(pool.clone()), reg.load_report_interval_ms());
    seed(
        &pool,
        "gone-mh",
        OWN_REGION,
        "unhealthy",
        ms(reg.retention()) + 5_000,
    )
    .await;
    let deleted = MediaHandlersRepository::delete_expired_handlers(&pool, reg.retention())
        .await
        .unwrap();
    assert_eq!(deleted, 1);

    let snap = MetricAssertion::snapshot();
    let status = service
        .send_load_report(load_report("gone-mh"))
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::NotFound);
    assert_eq!(status.message(), "Handler not registered", "no id echoed");

    snap.counter("gc_mh_load_reports_total")
        .with_labels(&[("outcome", "unknown_handler")])
        .assert_delta(1);
    snap.counter("gc_mh_load_reports_total")
        .with_labels(&[("outcome", "accepted")])
        .assert_unobserved();

    let (count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM media_handlers WHERE handler_id = 'gone-mh'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 0, "a load report never recreates a deleted row");
}

/// An invalid-argument load report increments neither outcome.
#[sqlx::test(migrations = "../../migrations")]
async fn invalid_load_report_counts_neither_outcome(pool: PgPool) {
    let service = MhService::new(
        Arc::new(pool),
        freshness_registry(2).load_report_interval_ms(),
    );

    let snap = MetricAssertion::snapshot();
    let status = service
        .send_load_report(load_report("bad id with spaces"))
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::InvalidArgument);

    snap.counter("gc_mh_load_reports_total")
        .with_labels(&[("outcome", "accepted")])
        .assert_unobserved();
    snap.counter("gc_mh_load_reports_total")
        .with_labels(&[("outcome", "unknown_handler")])
        .assert_unobserved();
}

// ---------------------------------------------------------------------------
// gc_mh_available
// ---------------------------------------------------------------------------

/// One row per state plus one stale healthy row, one row in a foreign region,
/// one expired row. After one tick: fresh rows count under their state, the
/// stale row counts as unhealthy, the foreign row lands in `other` (its region
/// string is never a label), the expired row is in no bucket.
#[sqlx::test(migrations = "../../migrations")]
async fn tick_sets_available_per_state_with_stale_as_unhealthy(pool: PgPool) {
    let reg = freshness_registry(2);
    let fresh = ms(reg.staleness_bound()) / 4;
    for state in STATES {
        seed(&pool, &format!("own-{state}"), OWN_REGION, state, fresh).await;
    }
    seed(
        &pool,
        "own-stale-healthy",
        OWN_REGION,
        "healthy",
        ms(reg.staleness_bound()) * 2,
    )
    .await;
    seed(
        &pool,
        "foreign-healthy",
        "mh-asserted-region-xyz",
        "healthy",
        fresh,
    )
    .await;
    seed(
        &pool,
        "own-expired",
        OWN_REGION,
        "healthy",
        ms(reg.retention()) + 5_000,
    )
    .await;

    let snap = MetricAssertion::snapshot();
    mh_registry_tick(&pool, &reg, OWN_REGION).await.unwrap();

    // pending, healthy, degraded, unhealthy (fresh unhealthy + swept stale), draining
    assert_available(&snap, OWN_REGION, [1.0, 1.0, 1.0, 2.0, 1.0]);
    assert_available(
        &snap,
        metrics::MH_AVAILABLE_OTHER_REGION,
        [0.0, 1.0, 0.0, 0.0, 0.0],
    );
    snap.gauge("gc_mh_available")
        .with_labels(&[("region", "mh-asserted-region-xyz")])
        .assert_unobserved();
}

/// Recompute-and-set, not accumulate: a second tick after a state change and
/// a deletion reflects the new registry exactly, and an emptied combination
/// (here `draining`, and the whole `other` bucket) reads 0.
#[sqlx::test(migrations = "../../migrations")]
async fn second_tick_recomputes_and_zeroes_emptied_series(pool: PgPool) {
    let reg = freshness_registry(2);
    let fresh = ms(reg.staleness_bound()) / 4;
    seed(&pool, "a", OWN_REGION, "healthy", fresh).await;
    seed(&pool, "b", OWN_REGION, "draining", fresh).await;
    seed(&pool, "c", "elsewhere", "healthy", fresh).await;

    let first = MetricAssertion::snapshot();
    mh_registry_tick(&pool, &reg, OWN_REGION).await.unwrap();
    assert_available(&first, OWN_REGION, [0.0, 1.0, 0.0, 0.0, 1.0]);
    assert_available(
        &first,
        metrics::MH_AVAILABLE_OTHER_REGION,
        [0.0, 1.0, 0.0, 0.0, 0.0],
    );

    // The draining MH turns healthy; the foreign row disappears.
    sqlx::query("UPDATE media_handlers SET health_status = 'healthy' WHERE handler_id = 'b'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM media_handlers WHERE handler_id = 'c'")
        .execute(&pool)
        .await
        .unwrap();

    let second = MetricAssertion::snapshot();
    mh_registry_tick(&pool, &reg, OWN_REGION).await.unwrap();
    assert_available(&second, OWN_REGION, [0.0, 2.0, 0.0, 0.0, 0.0]);
    assert_available(&second, metrics::MH_AVAILABLE_OTHER_REGION, [0.0; 5]);
}

/// Startup zero-init: the full closed label set exists at 0 before any MH.
#[test]
fn startup_zero_init_sets_full_label_set_to_zero() {
    let snap = MetricAssertion::snapshot();
    metrics::set_mh_available(OWN_REGION, &[]);
    assert_available(&snap, OWN_REGION, [0.0; 5]);
    assert_available(&snap, metrics::MH_AVAILABLE_OTHER_REGION, [0.0; 5]);
}

// ---------------------------------------------------------------------------
// Config gauges
// ---------------------------------------------------------------------------

#[test]
fn config_gauges_equal_configured_values() {
    let reg = freshness_registry(2);
    let snap = MetricAssertion::snapshot();
    metrics::set_mh_registry_config_gauges(&reg);

    snap.gauge("gc_mh_load_report_interval_seconds")
        .assert_value(reg.load_report_interval().as_secs_f64());
    snap.gauge("gc_mh_staleness_threshold_seconds")
        .assert_value(reg.staleness_bound().as_secs_f64());
    // And the derivation itself: 1.25 s × 3 = 3.75 s (not truncated).
    snap.gauge("gc_mh_staleness_threshold_seconds")
        .assert_value(3.75);
}
