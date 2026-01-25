//! Integration tests for Media Handlers registry.
//!
//! Tests MH registration, load reports, and health checking.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use global_controller::repositories::{HealthStatus, MediaHandlersRepository};
use sqlx::PgPool;

/// Test MH registration creates a new handler.
#[sqlx::test(migrations = "../../migrations")]
async fn test_mh_registration_creates_handler(pool: PgPool) {
    MediaHandlersRepository::register_mh(
        &pool,
        "test-mh-001",
        "us-east-1",
        "https://mh:443",
        "grpc://mh:50051",
        1000,
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

/// Test MH re-registration updates existing handler.
#[sqlx::test(migrations = "../../migrations")]
async fn test_mh_reregistration_updates_handler(pool: PgPool) {
    // Initial registration
    MediaHandlersRepository::register_mh(
        &pool,
        "test-mh-002",
        "us-east-1",
        "https://mh-old:443",
        "grpc://mh-old:50051",
        500,
    )
    .await
    .expect("Initial registration should succeed");

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

    // Re-registration with new endpoints
    MediaHandlersRepository::register_mh(
        &pool,
        "test-mh-002",
        "us-west-2", // Different region
        "https://mh-new:443",
        "grpc://mh-new:50051",
        1000, // Different capacity
    )
    .await
    .expect("Re-registration should succeed");

    let handler = MediaHandlersRepository::get_handler(&pool, "test-mh-002")
        .await
        .expect("Query should succeed")
        .expect("Handler should exist");

    // Verify updated values
    assert_eq!(handler.region, "us-west-2");
    assert_eq!(handler.webtransport_endpoint, "https://mh-new:443");
    assert_eq!(handler.grpc_endpoint, "grpc://mh-new:50051");
    assert_eq!(handler.max_streams, 1000);
    // Re-registration should reset health to pending
    assert_eq!(handler.health_status, HealthStatus::Pending);
}

/// Test load report updates handler metrics.
#[sqlx::test(migrations = "../../migrations")]
async fn test_load_report_updates_metrics(pool: PgPool) {
    // Register handler first
    MediaHandlersRepository::register_mh(
        &pool,
        "test-mh-003",
        "us-east-1",
        "https://mh:443",
        "grpc://mh:50051",
        1000,
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

/// Test marking stale handlers as unhealthy.
#[sqlx::test(migrations = "../../migrations")]
async fn test_mark_stale_handlers_unhealthy(pool: PgPool) {
    // Register a handler
    MediaHandlersRepository::register_mh(
        &pool,
        "stale-mh",
        "us-east-1",
        "https://mh:443",
        "grpc://mh:50051",
        1000,
    )
    .await
    .expect("Registration should succeed");

    // Update to healthy
    MediaHandlersRepository::update_load_report(
        &pool,
        "stale-mh",
        10,
        HealthStatus::Healthy,
        None,
        None,
        None,
    )
    .await
    .expect("Load report should succeed");

    // Backdate heartbeat to make it stale
    sqlx::query(
        "UPDATE media_handlers SET last_heartbeat_at = NOW() - INTERVAL '60 seconds' WHERE handler_id = $1"
    )
    .bind("stale-mh")
    .execute(&pool)
    .await
    .expect("Backdate should succeed");

    // Mark stale handlers (threshold 30 seconds)
    let count = MediaHandlersRepository::mark_stale_handlers_unhealthy(&pool, 30)
        .await
        .expect("Mark stale should succeed");

    assert_eq!(count, 1, "Should mark one handler as unhealthy");

    let handler = MediaHandlersRepository::get_handler(&pool, "stale-mh")
        .await
        .expect("Query should succeed")
        .expect("Handler should exist");

    assert_eq!(handler.health_status, HealthStatus::Unhealthy);
}

/// Test draining handlers are not marked unhealthy.
#[sqlx::test(migrations = "../../migrations")]
async fn test_draining_handler_not_marked_unhealthy(pool: PgPool) {
    // Register a handler
    MediaHandlersRepository::register_mh(
        &pool,
        "draining-mh",
        "us-east-1",
        "https://mh:443",
        "grpc://mh:50051",
        1000,
    )
    .await
    .expect("Registration should succeed");

    // Set to draining with old heartbeat
    sqlx::query(
        "UPDATE media_handlers SET health_status = 'draining', last_heartbeat_at = NOW() - INTERVAL '60 seconds' WHERE handler_id = $1"
    )
    .bind("draining-mh")
    .execute(&pool)
    .await
    .expect("Update should succeed");

    // Mark stale handlers (threshold 30 seconds)
    let count = MediaHandlersRepository::mark_stale_handlers_unhealthy(&pool, 30)
        .await
        .expect("Mark stale should succeed");

    assert_eq!(count, 0, "Should not mark draining handler");

    let handler = MediaHandlersRepository::get_handler(&pool, "draining-mh")
        .await
        .expect("Query should succeed")
        .expect("Handler should exist");

    assert_eq!(
        handler.health_status,
        HealthStatus::Draining,
        "Handler should remain draining"
    );
}

/// Test get candidate MHs returns healthy handlers with capacity.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_candidate_mhs(pool: PgPool) {
    // Register multiple handlers
    for i in 1..=5 {
        MediaHandlersRepository::register_mh(
            &pool,
            &format!("candidate-mh-{}", i),
            "us-east-1",
            &format!("https://mh{}:443", i),
            &format!("grpc://mh{}:50051", i),
            100,
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
        "unhealthy-mh",
        "us-east-1",
        "https://unhealthy:443",
        "grpc://unhealthy:50051",
        100,
    )
    .await
    .expect("Registration should succeed");

    // Add a handler at capacity (should not be returned)
    MediaHandlersRepository::register_mh(
        &pool,
        "full-mh",
        "us-east-1",
        "https://full:443",
        "grpc://full:50051",
        50,
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
    let candidates = MediaHandlersRepository::get_candidate_mhs(&pool, "us-east-1")
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
        "us-east-mh",
        "us-east-1",
        "https://us-east:443",
        "grpc://us-east:50051",
        100,
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
        "eu-west-mh",
        "eu-west-1",
        "https://eu-west:443",
        "grpc://eu-west:50051",
        100,
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
    let us_east_candidates = MediaHandlersRepository::get_candidate_mhs(&pool, "us-east-1")
        .await
        .expect("Query should succeed");

    assert_eq!(us_east_candidates.len(), 1);
    assert_eq!(us_east_candidates[0].handler_id, "us-east-mh");

    // Get candidates for eu-west-1
    let eu_west_candidates = MediaHandlersRepository::get_candidate_mhs(&pool, "eu-west-1")
        .await
        .expect("Query should succeed");

    assert_eq!(eu_west_candidates.len(), 1);
    assert_eq!(eu_west_candidates[0].handler_id, "eu-west-mh");

    // Get candidates for non-existent region
    let empty_candidates = MediaHandlersRepository::get_candidate_mhs(&pool, "ap-south-1")
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
        "degraded-mh",
        "us-east-1",
        "https://mh:443",
        "grpc://mh:50051",
        1000,
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
    let candidates = MediaHandlersRepository::get_candidate_mhs(&pool, "us-east-1")
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
            &format!("full-mh-{}", i),
            "us-east-1",
            &format!("https://mh{}:443", i),
            &format!("grpc://mh{}:50051", i),
            100, // max_streams = 100
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
    let candidates = MediaHandlersRepository::get_candidate_mhs(&pool, "us-east-1")
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
        "exactly-full-mh",
        "us-east-1",
        "https://full:443",
        "grpc://full:50051",
        100,
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
        "almost-full-mh",
        "us-east-1",
        "https://almost:443",
        "grpc://almost:50051",
        100,
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

    let candidates = MediaHandlersRepository::get_candidate_mhs(&pool, "us-east-1")
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
