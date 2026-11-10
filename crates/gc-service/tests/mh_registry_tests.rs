//! Integration tests for Media Handlers registry.
//!
//! Tests MH registration, load reports, and health checking.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "common/mod.rs"]
mod test_common;

use gc_service::repositories::{
    HealthStatus, MediaHandlersRepository, MhRegistrationDecision, NewMhRegistration,
};
use gc_service::services::weighted_pick::SystemSelectionRng;
use gc_service::services::MhSelectionService;
use sqlx::PgPool;
use std::collections::BTreeSet;
use std::time::Duration;
use test_common::mh_registry::{freshness_registry, ms, seed};

/// Shipped staleness bound, for tests NOT about freshness (taken from
/// config.env via gc-test-utils, never retyped).
fn shipped_bound() -> Duration {
    gc_test_utils::reference_mh_registry().staleness_bound()
}

/// Test MH registration creates a new handler.
#[sqlx::test(migrations = "../../migrations")]
async fn test_mh_registration_creates_handler(pool: PgPool) {
    MediaHandlersRepository::register_mh(
        &pool,
        &NewMhRegistration {
            handler_id: "test-mh-001",
            region: "us-east-1",
            webtransport_endpoint: "https://mh:443",
            grpc_endpoint: "grpc://mh:50051",
            max_streams: 1000,
        },
        shipped_bound(),
    )
    .await
    .expect("Registration should succeed");

    let handler = MediaHandlersRepository::get_handler(&pool, "test-mh-001")
        .await
        .expect("Query should succeed")
        .expect("Handler should exist");

    assert_eq!(handler.handler_id, "test-mh-001");
    assert_eq!(handler.region, "us-east-1");
    assert_eq!(handler.webtransport_endpoint, "https://mh:443");
    assert_eq!(handler.grpc_endpoint, "grpc://mh:50051");
    assert_eq!(handler.max_streams, 1000);
    assert_eq!(handler.current_streams, 0);
    assert_eq!(handler.health_status, HealthStatus::Pending);
}

/// Re-registration of an existing id. Previously this test asserted the
/// silent endpoint rewrite (`ON CONFLICT DO UPDATE SET ..._endpoint`), which
/// ADR-0039 §7 arm 1 removes: an incarnation's endpoints are FIXED FOR ITS
/// LIFE. Converted, not deleted: identical endpoints update region/capacity
/// and reset to pending; changed endpoints are refused with the row unchanged.
#[sqlx::test(migrations = "../../migrations")]
async fn test_mh_reregistration_updates_handler(pool: PgPool) {
    let reg = |region, wt, grpc, max_streams| NewMhRegistration {
        handler_id: "test-mh-002",
        region,
        webtransport_endpoint: wt,
        grpc_endpoint: grpc,
        max_streams,
    };
    let initial = MediaHandlersRepository::register_mh(
        &pool,
        &reg(
            "us-east-1",
            "https://mh-old:443",
            "grpc://mh-old:50051",
            500,
        ),
        shipped_bound(),
    )
    .await
    .expect("Initial registration should succeed");
    assert_eq!(initial, MhRegistrationDecision::Registered);

    // Update heartbeat to healthy
    MediaHandlersRepository::update_load_report(
        &pool,
        "test-mh-002",
        100,
        HealthStatus::Healthy,
        None,
        None,
        None,
    )
    .await
    .expect("Update should succeed");

    // Changed endpoints: refused, row unchanged (still healthy, old endpoints).
    let changed = MediaHandlersRepository::register_mh(
        &pool,
        &reg(
            "us-west-2",
            "https://mh-new:443",
            "grpc://mh-new:50051",
            1000,
        ),
        shipped_bound(),
    )
    .await
    .expect("Re-registration query should succeed");
    assert_eq!(
        changed,
        MhRegistrationDecision::EndpointChanged {
            webtransport: true,
            grpc: true
        }
    );
    let handler = MediaHandlersRepository::get_handler(&pool, "test-mh-002")
        .await
        .expect("Query should succeed")
        .expect("Handler should exist");
    assert_eq!(handler.region, "us-east-1");
    assert_eq!(handler.webtransport_endpoint, "https://mh-old:443");
    assert_eq!(handler.grpc_endpoint, "grpc://mh-old:50051");
    assert_eq!(handler.max_streams, 500);
    assert_eq!(handler.health_status, HealthStatus::Healthy);

    // Identical endpoints: accepted; region/capacity updated, reset to pending.
    let same = MediaHandlersRepository::register_mh(
        &pool,
        &reg(
            "us-west-2",
            "https://mh-old:443",
            "grpc://mh-old:50051",
            1000,
        ),
        shipped_bound(),
    )
    .await
    .expect("Re-registration should succeed");
    assert_eq!(same, MhRegistrationDecision::Registered);
    let handler = MediaHandlersRepository::get_handler(&pool, "test-mh-002")
        .await
        .expect("Query should succeed")
        .expect("Handler should exist");
    assert_eq!(handler.region, "us-west-2");
    assert_eq!(handler.webtransport_endpoint, "https://mh-old:443");
    assert_eq!(handler.max_streams, 1000);
    assert_eq!(handler.health_status, HealthStatus::Pending);
}

/// Test load report updates handler metrics.
#[sqlx::test(migrations = "../../migrations")]
async fn test_load_report_updates_metrics(pool: PgPool) {
    // Register handler first
    MediaHandlersRepository::register_mh(
        &pool,
        &NewMhRegistration {
            handler_id: "test-mh-003",
            region: "us-east-1",
            webtransport_endpoint: "https://mh:443",
            grpc_endpoint: "grpc://mh:50051",
            max_streams: 1000,
        },
        shipped_bound(),
    )
    .await
    .expect("Registration should succeed");

    // Send load report
    let updated = MediaHandlersRepository::update_load_report(
        &pool,
        "test-mh-003",
        150,
        HealthStatus::Healthy,
        Some(45.0),
        Some(60.0),
        Some(30.0),
    )
    .await
    .expect("Load report should succeed");

    assert!(updated, "Load report should update existing handler");

    let handler = MediaHandlersRepository::get_handler(&pool, "test-mh-003")
        .await
        .expect("Query should succeed")
        .expect("Handler should exist");

    assert_eq!(handler.current_streams, 150);
    assert_eq!(handler.health_status, HealthStatus::Healthy);
    assert_eq!(handler.cpu_usage_percent, Some(45.0));
    assert_eq!(handler.memory_usage_percent, Some(60.0));
    assert_eq!(handler.bandwidth_usage_percent, Some(30.0));
}

/// Test load report for unknown handler returns false.
#[sqlx::test(migrations = "../../migrations")]
async fn test_load_report_unknown_handler(pool: PgPool) {
    let updated = MediaHandlersRepository::update_load_report(
        &pool,
        "unknown-mh",
        50,
        HealthStatus::Healthy,
        None,
        None,
        None,
    )
    .await
    .expect("Load report should not error");

    assert!(
        !updated,
        "Load report for unknown handler should return false"
    );
}

/// A healthy handler older than the bound is marked unhealthy.
#[sqlx::test(migrations = "../../migrations")]
async fn test_mark_stale_handlers_unhealthy(pool: PgPool) {
    let bound = freshness_registry(2).staleness_bound();
    seed(&pool, "stale-mh", "us-east-1", "healthy", ms(bound) * 2).await;

    let count = MediaHandlersRepository::mark_stale_handlers_unhealthy(&pool, bound)
        .await
        .expect("Mark stale should succeed");
    assert_eq!(count, 1, "Should mark one handler as unhealthy");

    let handler = MediaHandlersRepository::get_handler(&pool, "stale-mh")
        .await
        .expect("Query should succeed")
        .expect("Handler should exist");
    assert_eq!(handler.health_status, HealthStatus::Unhealthy);
}

/// A draining handler that stopped reporting goes unhealthy (ADR-0039 §10:
/// the sweep no longer skips DRAINING); a draining handler still reporting
/// stays draining (positive arm).
#[sqlx::test(migrations = "../../migrations")]
async fn test_stale_draining_handler_marked_unhealthy_fresh_one_kept(pool: PgPool) {
    let bound = freshness_registry(2).staleness_bound();
    seed(
        &pool,
        "draining-stale",
        "us-east-1",
        "draining",
        ms(bound) * 2,
    )
    .await;
    seed(
        &pool,
        "draining-fresh",
        "us-east-1",
        "draining",
        ms(bound) / 4,
    )
    .await;

    let count = MediaHandlersRepository::mark_stale_handlers_unhealthy(&pool, bound)
        .await
        .expect("Mark stale should succeed");
    assert_eq!(count, 1, "only the stale draining handler transitions");

    let stale = MediaHandlersRepository::get_handler(&pool, "draining-stale")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stale.health_status, HealthStatus::Unhealthy);
    let fresh = MediaHandlersRepository::get_handler(&pool, "draining-fresh")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fresh.health_status, HealthStatus::Draining);
}

/// Selection eligibility, asserted as a SET of ids: only a fresh healthy row
/// is a candidate; stale healthy, fresh draining, unhealthy, pending and
/// degraded rows never are.
#[sqlx::test(migrations = "../../migrations")]
async fn test_candidate_set_excludes_draining_stale_and_non_healthy(pool: PgPool) {
    let bound = freshness_registry(2).staleness_bound();
    let fresh = ms(bound) / 4;
    seed(&pool, "fresh-healthy", "us-east-1", "healthy", fresh).await;
    seed(
        &pool,
        "stale-healthy",
        "us-east-1",
        "healthy",
        ms(bound) * 2,
    )
    .await;
    seed(&pool, "fresh-draining", "us-east-1", "draining", fresh).await;
    seed(&pool, "fresh-unhealthy", "us-east-1", "unhealthy", fresh).await;
    seed(&pool, "fresh-pending", "us-east-1", "pending", fresh).await;
    seed(&pool, "fresh-degraded", "us-east-1", "degraded", fresh).await;

    let ids: BTreeSet<String> =
        MediaHandlersRepository::get_candidate_mhs(&pool, "us-east-1", bound, &[])
            .await
            .unwrap()
            .into_iter()
            .map(|c| c.handler_id)
            .collect();
    assert_eq!(ids, BTreeSet::from(["fresh-healthy".to_string()]));
}

/// The candidate freshness bound is bound in MILLISECONDS (B = 3750 ms): a
/// row at −3200 ms is a candidate (a bound truncated to 3 s would drop it), a
/// row at −3900 ms is not (a bound rounded to 4 s would keep it). Rows only
/// age between the backdate and the query, so the 550 ms slack on the fresh
/// row is the margin that matters; seeding runs right before the query.
#[sqlx::test(migrations = "../../migrations")]
async fn test_candidate_bound_is_milliseconds(pool: PgPool) {
    let bound = freshness_registry(2).staleness_bound();
    assert_eq!(bound, Duration::from_millis(3750));
    seed(&pool, "ms-stale", "us-east-1", "healthy", 3900).await;
    seed(&pool, "ms-fresh", "us-east-1", "healthy", 3200).await;

    let ids: BTreeSet<String> =
        MediaHandlersRepository::get_candidate_mhs(&pool, "us-east-1", bound, &[])
            .await
            .unwrap()
            .into_iter()
            .map(|c| c.handler_id)
            .collect();
    assert_eq!(ids, BTreeSet::from(["ms-fresh".to_string()]));
}

/// `GC_MH_INITIAL_HANDLERS_PER_MEETING` governs the selection size: N=1 and
/// N=3 against 4 candidates select exactly N distinct handlers (the shipped
/// default 2 alone could not distinguish a hardcoded 2).
#[sqlx::test(migrations = "../../migrations")]
async fn test_selection_size_follows_initial_handlers_config(pool: PgPool) {
    for id in ["sel-a", "sel-b", "sel-c", "sel-d"] {
        seed(&pool, id, "us-east-1", "healthy", 0).await;
    }
    for n in [1_u64, 3] {
        let reg = freshness_registry(n);
        let selection = MhSelectionService::select(
            &pool,
            "us-east-1",
            reg.staleness_bound(),
            reg.initial_handlers_per_meeting(),
            &[],
            &SystemSelectionRng,
        )
        .await
        .expect("selection");
        let ids: BTreeSet<&str> = selection.iter().map(|h| h.mh_id.as_str()).collect();
        assert_eq!(selection.len(), reg.initial_handlers_per_meeting());
        assert_eq!(ids.len(), selection.len(), "selected handlers are distinct");
    }
    // Fewer candidates than N: all of them, never an error.
    let reg = freshness_registry(4);
    sqlx::query("DELETE FROM media_handlers WHERE handler_id IN ('sel-c', 'sel-d')")
        .execute(&pool)
        .await
        .unwrap();
    let selection = MhSelectionService::select(
        &pool,
        "us-east-1",
        reg.staleness_bound(),
        reg.initial_handlers_per_meeting(),
        &[],
        &SystemSelectionRng,
    )
    .await
    .expect("selection");
    assert_eq!(selection.len(), 2);
}

/// Retention: a row older than retention is deleted; a stale-but-younger row
/// is kept. A load report for the deleted id updates nothing and does not
/// recreate the row.
#[sqlx::test(migrations = "../../migrations")]
async fn test_retention_deletes_only_expired_rows(pool: PgPool) {
    let reg = freshness_registry(2);
    seed(
        &pool,
        "expired",
        "us-east-1",
        "unhealthy",
        ms(reg.retention()) + 5_000,
    )
    .await;
    seed(
        &pool,
        "stale-kept",
        "us-east-1",
        "unhealthy",
        ms(reg.staleness_bound()) * 2,
    )
    .await;

    let deleted = MediaHandlersRepository::delete_expired_handlers(&pool, reg.retention())
        .await
        .unwrap();
    assert_eq!(deleted, 1);
    assert!(MediaHandlersRepository::get_handler(&pool, "expired")
        .await
        .unwrap()
        .is_none());
    assert!(MediaHandlersRepository::get_handler(&pool, "stale-kept")
        .await
        .unwrap()
        .is_some());

    let updated = MediaHandlersRepository::update_load_report(
        &pool,
        "expired",
        1,
        HealthStatus::Healthy,
        None,
        None,
        None,
    )
    .await
    .unwrap();
    assert!(!updated, "load report for a deleted row matches nothing");
    let (count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM media_handlers WHERE handler_id = 'expired'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 0, "a load report never recreates a deleted row");
}

/// Test get candidate MHs returns healthy handlers with capacity.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_candidate_mhs(pool: PgPool) {
    // Register multiple handlers
    for i in 1..=5 {
        MediaHandlersRepository::register_mh(
            &pool,
            &NewMhRegistration {
                handler_id: &format!("candidate-mh-{}", i),
                region: "us-east-1",
                webtransport_endpoint: &format!("https://mh{}:443", i),
                grpc_endpoint: &format!("grpc://mh{}:50051", i),
                max_streams: 100,
            },
            shipped_bound(),
        )
        .await
        .expect("Registration should succeed");

        // Set to healthy with varying load
        MediaHandlersRepository::update_load_report(
            &pool,
            &format!("candidate-mh-{}", i),
            i * 10, // 10, 20, 30, 40, 50 streams
            HealthStatus::Healthy,
            None,
            None,
            None,
        )
        .await
        .expect("Load report should succeed");
    }

    // Add an unhealthy handler (should not be returned)
    MediaHandlersRepository::register_mh(
        &pool,
        &NewMhRegistration {
            handler_id: "unhealthy-mh",
            region: "us-east-1",
            webtransport_endpoint: "https://unhealthy:443",
            grpc_endpoint: "grpc://unhealthy:50051",
            max_streams: 100,
        },
        shipped_bound(),
    )
    .await
    .expect("Registration should succeed");

    // Add a handler at capacity (should not be returned)
    MediaHandlersRepository::register_mh(
        &pool,
        &NewMhRegistration {
            handler_id: "full-mh",
            region: "us-east-1",
            webtransport_endpoint: "https://full:443",
            grpc_endpoint: "grpc://full:50051",
            max_streams: 50,
        },
        shipped_bound(),
    )
    .await
    .expect("Registration should succeed");

    MediaHandlersRepository::update_load_report(
        &pool,
        "full-mh",
        50, // At capacity
        HealthStatus::Healthy,
        None,
        None,
        None,
    )
    .await
    .expect("Load report should succeed");

    // Get candidates
    let candidates =
        MediaHandlersRepository::get_candidate_mhs(&pool, "us-east-1", shipped_bound(), &[])
            .await
            .expect("Query should succeed");

    // Should return up to 5 healthy candidates with capacity
    assert!(!candidates.is_empty(), "Should return candidates");
    assert!(candidates.len() <= 5, "Should return at most 5 candidates");

    // Should be ordered by load ratio (least loaded first)
    for window in candidates.windows(2) {
        assert!(
            window[0].load_ratio <= window[1].load_ratio,
            "Candidates should be ordered by load ratio"
        );
    }

    // Should not include unhealthy or full handlers
    for candidate in &candidates {
        assert!(
            !candidate.handler_id.contains("unhealthy"),
            "Should not include unhealthy handler"
        );
        assert!(
            candidate.handler_id != "full-mh",
            "Should not include full handler"
        );
    }
}

/// Test get candidate MHs filters by region.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_candidate_mhs_filters_by_region(pool: PgPool) {
    // Register handlers in different regions
    MediaHandlersRepository::register_mh(
        &pool,
        &NewMhRegistration {
            handler_id: "us-east-mh",
            region: "us-east-1",
            webtransport_endpoint: "https://us-east:443",
            grpc_endpoint: "grpc://us-east:50051",
            max_streams: 100,
        },
        shipped_bound(),
    )
    .await
    .expect("Registration should succeed");

    MediaHandlersRepository::update_load_report(
        &pool,
        "us-east-mh",
        10,
        HealthStatus::Healthy,
        None,
        None,
        None,
    )
    .await
    .expect("Load report should succeed");

    MediaHandlersRepository::register_mh(
        &pool,
        &NewMhRegistration {
            handler_id: "eu-west-mh",
            region: "eu-west-1",
            webtransport_endpoint: "https://eu-west:443",
            grpc_endpoint: "grpc://eu-west:50051",
            max_streams: 100,
        },
        shipped_bound(),
    )
    .await
    .expect("Registration should succeed");

    MediaHandlersRepository::update_load_report(
        &pool,
        "eu-west-mh",
        10,
        HealthStatus::Healthy,
        None,
        None,
        None,
    )
    .await
    .expect("Load report should succeed");

    // Get candidates for us-east-1
    let us_east_candidates =
        MediaHandlersRepository::get_candidate_mhs(&pool, "us-east-1", shipped_bound(), &[])
            .await
            .expect("Query should succeed");

    assert_eq!(us_east_candidates.len(), 1);
    assert_eq!(us_east_candidates[0].handler_id, "us-east-mh");

    // Get candidates for eu-west-1
    let eu_west_candidates =
        MediaHandlersRepository::get_candidate_mhs(&pool, "eu-west-1", shipped_bound(), &[])
            .await
            .expect("Query should succeed");

    assert_eq!(eu_west_candidates.len(), 1);
    assert_eq!(eu_west_candidates[0].handler_id, "eu-west-mh");

    // Get candidates for non-existent region
    let empty_candidates =
        MediaHandlersRepository::get_candidate_mhs(&pool, "ap-south-1", shipped_bound(), &[])
            .await
            .expect("Query should succeed");

    assert!(empty_candidates.is_empty());
}

/// Test load report with Degraded health status (boundary value).
///
/// HealthStatus::Degraded (value 2) is a boundary case - handlers can still
/// receive load reports even when degraded, though they may not be selected
/// for new meetings.
#[sqlx::test(migrations = "../../migrations")]
async fn test_load_report_with_degraded_health_status(pool: PgPool) {
    // Register handler first
    MediaHandlersRepository::register_mh(
        &pool,
        &NewMhRegistration {
            handler_id: "degraded-mh",
            region: "us-east-1",
            webtransport_endpoint: "https://mh:443",
            grpc_endpoint: "grpc://mh:50051",
            max_streams: 1000,
        },
        shipped_bound(),
    )
    .await
    .expect("Registration should succeed");

    // Send load report with Degraded status
    let updated = MediaHandlersRepository::update_load_report(
        &pool,
        "degraded-mh",
        500,
        HealthStatus::Degraded,
        Some(75.0), // High CPU
        Some(80.0), // High memory
        Some(50.0),
    )
    .await
    .expect("Load report should succeed");

    assert!(updated, "Load report should update existing handler");

    let handler = MediaHandlersRepository::get_handler(&pool, "degraded-mh")
        .await
        .expect("Query should succeed")
        .expect("Handler should exist");

    assert_eq!(handler.current_streams, 500);
    assert_eq!(
        handler.health_status,
        HealthStatus::Degraded,
        "Health status should be Degraded"
    );
    assert_eq!(handler.cpu_usage_percent, Some(75.0));
    assert_eq!(handler.memory_usage_percent, Some(80.0));

    // Degraded handler should NOT be returned as candidate for new meetings
    let candidates =
        MediaHandlersRepository::get_candidate_mhs(&pool, "us-east-1", shipped_bound(), &[])
            .await
            .expect("Query should succeed");

    assert!(
        candidates.is_empty(),
        "Degraded handler should not be a candidate"
    );
}

/// Test candidates at maximum capacity (load_ratio >= 1.0).
///
/// When all candidates are at max capacity, get_candidate_mhs should return
/// an empty list since no handlers can accept new streams.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_candidate_mhs_all_at_max_capacity(pool: PgPool) {
    // Register multiple handlers all at max capacity
    for i in 1..=3 {
        MediaHandlersRepository::register_mh(
            &pool,
            &NewMhRegistration {
                handler_id: &format!("full-mh-{}", i),
                region: "us-east-1",
                webtransport_endpoint: &format!("https://mh{}:443", i),
                grpc_endpoint: &format!("grpc://mh{}:50051", i),
                max_streams: 100, // max_streams = 100
            },
            shipped_bound(),
        )
        .await
        .expect("Registration should succeed");

        // Set current_streams == max_streams (at capacity)
        MediaHandlersRepository::update_load_report(
            &pool,
            &format!("full-mh-{}", i),
            100, // At max capacity
            HealthStatus::Healthy,
            None,
            None,
            None,
        )
        .await
        .expect("Load report should succeed");
    }

    // Get candidates - should return empty since all are at capacity
    let candidates =
        MediaHandlersRepository::get_candidate_mhs(&pool, "us-east-1", shipped_bound(), &[])
            .await
            .expect("Query should succeed");

    assert!(
        candidates.is_empty(),
        "Should return no candidates when all handlers are at max capacity"
    );
}

/// Test candidates with load_ratio exactly at 1.0 boundary.
///
/// When current_streams == max_streams - 1, load_ratio < 1.0 so handler
/// should still be a candidate. When current_streams == max_streams,
/// handler should NOT be a candidate.
#[sqlx::test(migrations = "../../migrations")]
async fn test_candidate_selection_load_ratio_boundary(pool: PgPool) {
    // Handler at exactly max capacity (should NOT be candidate)
    MediaHandlersRepository::register_mh(
        &pool,
        &NewMhRegistration {
            handler_id: "exactly-full-mh",
            region: "us-east-1",
            webtransport_endpoint: "https://full:443",
            grpc_endpoint: "grpc://full:50051",
            max_streams: 100,
        },
        shipped_bound(),
    )
    .await
    .expect("Registration should succeed");

    MediaHandlersRepository::update_load_report(
        &pool,
        "exactly-full-mh",
        100, // Exactly at capacity
        HealthStatus::Healthy,
        None,
        None,
        None,
    )
    .await
    .expect("Load report should succeed");

    // Handler at max - 1 (SHOULD be candidate)
    MediaHandlersRepository::register_mh(
        &pool,
        &NewMhRegistration {
            handler_id: "almost-full-mh",
            region: "us-east-1",
            webtransport_endpoint: "https://almost:443",
            grpc_endpoint: "grpc://almost:50051",
            max_streams: 100,
        },
        shipped_bound(),
    )
    .await
    .expect("Registration should succeed");

    MediaHandlersRepository::update_load_report(
        &pool,
        "almost-full-mh",
        99, // One below capacity
        HealthStatus::Healthy,
        None,
        None,
        None,
    )
    .await
    .expect("Load report should succeed");

    let candidates =
        MediaHandlersRepository::get_candidate_mhs(&pool, "us-east-1", shipped_bound(), &[])
            .await
            .expect("Query should succeed");

    // Should have exactly 1 candidate (almost-full-mh)
    assert_eq!(candidates.len(), 1, "Should return exactly one candidate");
    assert_eq!(
        candidates[0].handler_id, "almost-full-mh",
        "Only handler below capacity should be candidate"
    );
    assert!(
        (candidates[0].load_ratio - 0.99).abs() < 0.01,
        "Load ratio should be ~0.99"
    );
}
