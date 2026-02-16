//! Meeting assignment integration tests.
//!
//! Tests the MC assignment functionality including:
//! - Load balancing MC selection
//! - Atomic assignment with race condition handling
//! - Existing assignment reuse
//! - Assignment cleanup

use gc_service::repositories::{
    HealthStatus, McCandidate, MeetingAssignmentsRepository, MeetingControllersRepository,
};
use gc_service::services::{McAssignmentService, MockMcClient};
use sqlx::PgPool;
use std::sync::Arc;

/// Helper to register a healthy MC for testing.
async fn register_healthy_mc(
    pool: &PgPool,
    id: &str,
    region: &str,
    current_meetings: i32,
    max_meetings: i32,
) -> Result<(), anyhow::Error> {
    MeetingControllersRepository::register_mc(
        pool,
        id,
        region,
        &format!("https://{}.example.com:50051", id),
        Some(&format!("https://{}.example.com:443", id)),
        max_meetings,
        1000,
    )
    .await?;

    MeetingControllersRepository::update_heartbeat(
        pool,
        id,
        current_meetings,
        current_meetings * 10,
        HealthStatus::Healthy,
    )
    .await?;

    Ok(())
}

/// Helper to register healthy MHs for testing.
async fn register_healthy_mhs_for_region(pool: &PgPool, region: &str) -> Result<(), anyhow::Error> {
    // Register primary MH
    sqlx::query(
        r#"
        INSERT INTO media_handlers (
            handler_id, region, webtransport_endpoint, grpc_endpoint,
            max_streams, current_streams, health_status, last_heartbeat_at, registered_at
        )
        VALUES ($1, $2, $3, $4, 1000, 0, 'healthy', NOW(), NOW())
        ON CONFLICT (handler_id) DO UPDATE SET
            last_heartbeat_at = NOW(),
            health_status = 'healthy'
        "#,
    )
    .bind(format!("mh-primary-{}", region))
    .bind(region)
    .bind(format!("https://mh-primary-{}.example.com:443", region))
    .bind(format!("grpc://mh-primary-{}.example.com:50051", region))
    .execute(pool)
    .await?;

    // Register backup MH
    sqlx::query(
        r#"
        INSERT INTO media_handlers (
            handler_id, region, webtransport_endpoint, grpc_endpoint,
            max_streams, current_streams, health_status, last_heartbeat_at, registered_at
        )
        VALUES ($1, $2, $3, $4, 1000, 0, 'healthy', NOW(), NOW())
        ON CONFLICT (handler_id) DO UPDATE SET
            last_heartbeat_at = NOW(),
            health_status = 'healthy'
        "#,
    )
    .bind(format!("mh-backup-{}", region))
    .bind(region)
    .bind(format!("https://mh-backup-{}.example.com:443", region))
    .bind(format!("grpc://mh-backup-{}.example.com:50051", region))
    .execute(pool)
    .await?;

    Ok(())
}

// ============================================================================
// Repository Tests
// ============================================================================

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_healthy_assignment_none_when_empty(pool: PgPool) -> Result<(), anyhow::Error> {
    let result =
        MeetingAssignmentsRepository::get_healthy_assignment(&pool, "meeting-1", "us-east-1")
            .await?;

    assert!(
        result.is_none(),
        "Should return None when no assignments exist"
    );

    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_candidate_mcs_empty_when_no_healthy(pool: PgPool) -> Result<(), anyhow::Error> {
    let candidates = MeetingAssignmentsRepository::get_candidate_mcs(&pool, "us-east-1").await?;

    assert!(
        candidates.is_empty(),
        "Should return empty when no healthy MCs"
    );

    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_candidate_mcs_returns_healthy_only(pool: PgPool) -> Result<(), anyhow::Error> {
    // Register MCs with different states
    register_healthy_mc(&pool, "mc-healthy-1", "us-east-1", 10, 100).await?;
    register_healthy_mc(&pool, "mc-healthy-2", "us-east-1", 20, 100).await?;

    // Register an unhealthy MC
    MeetingControllersRepository::register_mc(
        &pool,
        "mc-unhealthy",
        "us-east-1",
        "https://mc-unhealthy.example.com:50051",
        None,
        100,
        1000,
    )
    .await?;
    MeetingControllersRepository::update_heartbeat(
        &pool,
        "mc-unhealthy",
        0,
        0,
        HealthStatus::Unhealthy,
    )
    .await?;

    let candidates = MeetingAssignmentsRepository::get_candidate_mcs(&pool, "us-east-1").await?;

    assert_eq!(candidates.len(), 2, "Should return only healthy MCs");
    assert!(candidates
        .iter()
        .all(|c| c.controller_id.starts_with("mc-healthy")));

    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_candidate_mcs_ordered_by_load(pool: PgPool) -> Result<(), anyhow::Error> {
    // Register MCs with different loads
    register_healthy_mc(&pool, "mc-heavy", "us-east-1", 90, 100).await?;
    register_healthy_mc(&pool, "mc-light", "us-east-1", 10, 100).await?;
    register_healthy_mc(&pool, "mc-medium", "us-east-1", 50, 100).await?;

    let candidates = MeetingAssignmentsRepository::get_candidate_mcs(&pool, "us-east-1").await?;

    assert_eq!(candidates.len(), 3);
    // Should be ordered by load ratio (ascending)
    // Safe to index after asserting length
    assert_eq!(
        candidates.first().map(|c| c.controller_id.as_str()),
        Some("mc-light")
    );
    assert_eq!(
        candidates.get(1).map(|c| c.controller_id.as_str()),
        Some("mc-medium")
    );
    assert_eq!(
        candidates.get(2).map(|c| c.controller_id.as_str()),
        Some("mc-heavy")
    );

    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_candidate_mcs_excludes_full(pool: PgPool) -> Result<(), anyhow::Error> {
    // Register a full MC
    register_healthy_mc(&pool, "mc-full", "us-east-1", 100, 100).await?;
    // Register a MC with capacity
    register_healthy_mc(&pool, "mc-available", "us-east-1", 50, 100).await?;

    let candidates = MeetingAssignmentsRepository::get_candidate_mcs(&pool, "us-east-1").await?;

    assert_eq!(candidates.len(), 1);
    assert_eq!(
        candidates.first().map(|c| c.controller_id.as_str()),
        Some("mc-available")
    );

    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_candidate_mcs_filters_by_region(pool: PgPool) -> Result<(), anyhow::Error> {
    register_healthy_mc(&pool, "mc-us-1", "us-east-1", 10, 100).await?;
    register_healthy_mc(&pool, "mc-eu-1", "eu-west-1", 10, 100).await?;

    let us_candidates = MeetingAssignmentsRepository::get_candidate_mcs(&pool, "us-east-1").await?;
    let eu_candidates = MeetingAssignmentsRepository::get_candidate_mcs(&pool, "eu-west-1").await?;

    assert_eq!(us_candidates.len(), 1);
    assert_eq!(
        us_candidates.first().map(|c| c.controller_id.as_str()),
        Some("mc-us-1")
    );

    assert_eq!(eu_candidates.len(), 1);
    assert_eq!(
        eu_candidates.first().map(|c| c.controller_id.as_str()),
        Some("mc-eu-1")
    );

    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_atomic_assign_creates_assignment(pool: PgPool) -> Result<(), anyhow::Error> {
    register_healthy_mc(&pool, "mc-1", "us-east-1", 10, 100).await?;

    let candidate = McCandidate {
        controller_id: "mc-1".to_string(),
        grpc_endpoint: "https://mc-1.example.com:50051".to_string(),
        webtransport_endpoint: Some("https://mc-1.example.com:443".to_string()),
        load_ratio: 0.1,
    };

    let assignment = MeetingAssignmentsRepository::atomic_assign(
        &pool,
        "meeting-123",
        "us-east-1",
        &candidate,
        "gc-test-001",
    )
    .await?;

    assert_eq!(assignment.mc_id, "mc-1");
    assert_eq!(assignment.grpc_endpoint, "https://mc-1.example.com:50051");
    assert_eq!(
        assignment.webtransport_endpoint,
        Some("https://mc-1.example.com:443".to_string())
    );

    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_atomic_assign_reuses_existing(pool: PgPool) -> Result<(), anyhow::Error> {
    register_healthy_mc(&pool, "mc-1", "us-east-1", 10, 100).await?;
    register_healthy_mc(&pool, "mc-2", "us-east-1", 5, 100).await?;

    let candidate1 = McCandidate {
        controller_id: "mc-1".to_string(),
        grpc_endpoint: "https://mc-1.example.com:50051".to_string(),
        webtransport_endpoint: None,
        load_ratio: 0.1,
    };

    // First assignment
    let first = MeetingAssignmentsRepository::atomic_assign(
        &pool,
        "meeting-123",
        "us-east-1",
        &candidate1,
        "gc-test-001",
    )
    .await?;

    assert_eq!(first.mc_id, "mc-1");

    // Second assignment attempt should return existing (even with different candidate)
    let candidate2 = McCandidate {
        controller_id: "mc-2".to_string(),
        grpc_endpoint: "https://mc-2.example.com:50051".to_string(),
        webtransport_endpoint: None,
        load_ratio: 0.05,
    };

    let second = MeetingAssignmentsRepository::atomic_assign(
        &pool,
        "meeting-123",
        "us-east-1",
        &candidate2,
        "gc-test-002",
    )
    .await?;

    // Should return the first assignment, not create a new one
    assert_eq!(second.mc_id, "mc-1", "Should return existing assignment");

    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_end_assignment_by_region(pool: PgPool) -> Result<(), anyhow::Error> {
    register_healthy_mc(&pool, "mc-1", "us-east-1", 10, 100).await?;
    register_healthy_mc(&pool, "mc-2", "eu-west-1", 10, 100).await?;

    // Create assignments in two regions
    let candidate1 = McCandidate {
        controller_id: "mc-1".to_string(),
        grpc_endpoint: "https://mc-1.example.com:50051".to_string(),
        webtransport_endpoint: None,
        load_ratio: 0.1,
    };
    let candidate2 = McCandidate {
        controller_id: "mc-2".to_string(),
        grpc_endpoint: "https://mc-2.example.com:50051".to_string(),
        webtransport_endpoint: None,
        load_ratio: 0.1,
    };

    MeetingAssignmentsRepository::atomic_assign(
        &pool,
        "meeting-123",
        "us-east-1",
        &candidate1,
        "gc-1",
    )
    .await?;
    MeetingAssignmentsRepository::atomic_assign(
        &pool,
        "meeting-123",
        "eu-west-1",
        &candidate2,
        "gc-1",
    )
    .await?;

    // End only US assignment
    let ended =
        MeetingAssignmentsRepository::end_assignment(&pool, "meeting-123", Some("us-east-1"))
            .await?;
    assert_eq!(ended, 1);

    // US assignment should be gone
    let us_assignment =
        MeetingAssignmentsRepository::get_healthy_assignment(&pool, "meeting-123", "us-east-1")
            .await?;
    assert!(us_assignment.is_none());

    // EU assignment should still exist
    let eu_assignment =
        MeetingAssignmentsRepository::get_healthy_assignment(&pool, "meeting-123", "eu-west-1")
            .await?;
    assert!(eu_assignment.is_some());

    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_end_assignment_all_regions(pool: PgPool) -> Result<(), anyhow::Error> {
    register_healthy_mc(&pool, "mc-1", "us-east-1", 10, 100).await?;
    register_healthy_mc(&pool, "mc-2", "eu-west-1", 10, 100).await?;

    let candidate1 = McCandidate {
        controller_id: "mc-1".to_string(),
        grpc_endpoint: "https://mc-1.example.com:50051".to_string(),
        webtransport_endpoint: None,
        load_ratio: 0.1,
    };
    let candidate2 = McCandidate {
        controller_id: "mc-2".to_string(),
        grpc_endpoint: "https://mc-2.example.com:50051".to_string(),
        webtransport_endpoint: None,
        load_ratio: 0.1,
    };

    MeetingAssignmentsRepository::atomic_assign(
        &pool,
        "meeting-456",
        "us-east-1",
        &candidate1,
        "gc-1",
    )
    .await?;
    MeetingAssignmentsRepository::atomic_assign(
        &pool,
        "meeting-456",
        "eu-west-1",
        &candidate2,
        "gc-1",
    )
    .await?;

    // End all assignments
    let ended = MeetingAssignmentsRepository::end_assignment(&pool, "meeting-456", None).await?;
    assert_eq!(ended, 2);

    // Both should be gone
    let us =
        MeetingAssignmentsRepository::get_healthy_assignment(&pool, "meeting-456", "us-east-1")
            .await?;
    let eu =
        MeetingAssignmentsRepository::get_healthy_assignment(&pool, "meeting-456", "eu-west-1")
            .await?;
    assert!(us.is_none() && eu.is_none());

    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_cleanup_old_assignments(pool: PgPool) -> Result<(), anyhow::Error> {
    register_healthy_mc(&pool, "mc-1", "us-east-1", 10, 100).await?;

    let candidate = McCandidate {
        controller_id: "mc-1".to_string(),
        grpc_endpoint: "https://mc-1.example.com:50051".to_string(),
        webtransport_endpoint: None,
        load_ratio: 0.1,
    };

    // Create and end an assignment
    MeetingAssignmentsRepository::atomic_assign(
        &pool,
        "meeting-old",
        "us-east-1",
        &candidate,
        "gc-1",
    )
    .await?;
    MeetingAssignmentsRepository::end_assignment(&pool, "meeting-old", None).await?;

    // Manually set ended_at to 10 days ago
    sqlx::query(
        "UPDATE meeting_assignments SET ended_at = NOW() - INTERVAL '10 days' WHERE meeting_id = $1"
    )
    .bind("meeting-old")
    .execute(&pool)
    .await?;

    // Cleanup with 7 day retention (None uses default batch size)
    let cleaned = MeetingAssignmentsRepository::cleanup_old_assignments(&pool, 7, None).await?;
    assert_eq!(cleaned, 1);

    Ok(())
}

// ============================================================================
// Service Tests (using assign_meeting_with_mh with MockMcClient)
// ============================================================================

#[sqlx::test(migrations = "../../migrations")]
async fn test_service_assign_meeting_with_mh_no_healthy_mcs(
    pool: PgPool,
) -> Result<(), anyhow::Error> {
    // Register MHs but no MCs
    register_healthy_mhs_for_region(&pool, "us-east-1").await?;

    let mc_client = Arc::new(MockMcClient::accepting());
    let result = McAssignmentService::assign_meeting_with_mh(
        &pool,
        mc_client,
        "meeting-1",
        "us-east-1",
        "gc-1",
    )
    .await;

    assert!(result.is_err());
    // Should be ServiceUnavailable error

    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_service_assign_meeting_with_mh_success(pool: PgPool) -> Result<(), anyhow::Error> {
    register_healthy_mc(&pool, "mc-1", "us-east-1", 10, 100).await?;
    register_healthy_mhs_for_region(&pool, "us-east-1").await?;

    let mc_client = Arc::new(MockMcClient::accepting());
    let assignment = McAssignmentService::assign_meeting_with_mh(
        &pool,
        mc_client,
        "meeting-1",
        "us-east-1",
        "gc-1",
    )
    .await?;

    assert_eq!(assignment.mc_assignment.mc_id, "mc-1");
    assert!(!assignment.mc_assignment.grpc_endpoint.is_empty());
    assert!(!assignment.mh_selection.primary.mh_id.is_empty());

    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_service_assign_meeting_with_mh_reuses_healthy(
    pool: PgPool,
) -> Result<(), anyhow::Error> {
    register_healthy_mc(&pool, "mc-1", "us-east-1", 10, 100).await?;
    register_healthy_mc(&pool, "mc-2", "us-east-1", 5, 100).await?;
    register_healthy_mhs_for_region(&pool, "us-east-1").await?;

    let mc_client = Arc::new(MockMcClient::accepting());

    // First assignment
    let first = McAssignmentService::assign_meeting_with_mh(
        &pool,
        mc_client.clone(),
        "meeting-1",
        "us-east-1",
        "gc-1",
    )
    .await?;
    let first_mc = first.mc_assignment.mc_id.clone();

    // Second assignment should return the same MC
    let second = McAssignmentService::assign_meeting_with_mh(
        &pool,
        mc_client,
        "meeting-1",
        "us-east-1",
        "gc-2",
    )
    .await?;

    assert_eq!(
        second.mc_assignment.mc_id, first_mc,
        "Should reuse existing healthy assignment"
    );

    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_service_end_assignment(pool: PgPool) -> Result<(), anyhow::Error> {
    register_healthy_mc(&pool, "mc-1", "us-east-1", 10, 100).await?;
    register_healthy_mhs_for_region(&pool, "us-east-1").await?;

    // Create assignment using assign_meeting_with_mh
    let mc_client = Arc::new(MockMcClient::accepting());
    McAssignmentService::assign_meeting_with_mh(&pool, mc_client, "meeting-1", "us-east-1", "gc-1")
        .await?;

    // End assignment
    let ended = McAssignmentService::end_assignment(&pool, "meeting-1", Some("us-east-1")).await?;
    assert_eq!(ended, 1);

    // Verify it's gone
    let assignment = McAssignmentService::get_assignment(&pool, "meeting-1", "us-east-1").await?;
    assert!(assignment.is_none());

    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_service_get_assignment(pool: PgPool) -> Result<(), anyhow::Error> {
    register_healthy_mc(&pool, "mc-1", "us-east-1", 10, 100).await?;
    register_healthy_mhs_for_region(&pool, "us-east-1").await?;

    // No assignment yet
    let none = McAssignmentService::get_assignment(&pool, "meeting-1", "us-east-1").await?;
    assert!(none.is_none());

    // Create assignment
    let mc_client = Arc::new(MockMcClient::accepting());
    McAssignmentService::assign_meeting_with_mh(&pool, mc_client, "meeting-1", "us-east-1", "gc-1")
        .await?;

    // Should find it now
    let some = McAssignmentService::get_assignment(&pool, "meeting-1", "us-east-1").await?;
    assert_eq!(some.as_ref().map(|a| a.mc_id.as_str()), Some("mc-1"));

    Ok(())
}

// ============================================================================
// Race Condition and Health Transition Tests (Finding 1 & 2)
// ============================================================================

/// Test concurrent assignment attempts - verifies atomic CTE handles race conditions.
///
/// Multiple "GCs" attempt to assign the same meeting simultaneously.
/// The atomic CTE should ensure only one assignment is created and all
/// concurrent requests return the same MC assignment.
#[sqlx::test(migrations = "../../migrations")]
async fn test_concurrent_assignment_race_condition(pool: PgPool) -> Result<(), anyhow::Error> {
    use std::collections::HashSet;
    use tokio::sync::Barrier;

    // Register multiple healthy MCs and MHs
    register_healthy_mc(&pool, "mc-1", "us-east-1", 10, 100).await?;
    register_healthy_mc(&pool, "mc-2", "us-east-1", 20, 100).await?;
    register_healthy_mc(&pool, "mc-3", "us-east-1", 30, 100).await?;
    register_healthy_mhs_for_region(&pool, "us-east-1").await?;

    let pool = Arc::new(pool);
    let num_concurrent = 10;
    let barrier = Arc::new(Barrier::new(num_concurrent));

    // Spawn concurrent tasks that all try to assign the same meeting
    let handles: Vec<_> = (0..num_concurrent)
        .map(|i| {
            let pool = Arc::clone(&pool);
            let barrier = Arc::clone(&barrier);
            let mc_client = Arc::new(MockMcClient::accepting());
            tokio::spawn(async move {
                // Wait for all tasks to be ready before proceeding
                barrier.wait().await;

                // All tasks attempt to assign the same meeting
                McAssignmentService::assign_meeting_with_mh(
                    &pool,
                    mc_client,
                    "meeting-concurrent-test",
                    "us-east-1",
                    &format!("gc-{}", i),
                )
                .await
            })
        })
        .collect();

    // Collect all results
    let results: Vec<_> = futures::future::join_all(handles)
        .await
        .into_iter()
        .map(|r| r.expect("task should not panic"))
        .collect();

    // All should succeed
    let successful: Vec<_> = results.iter().filter(|r| r.is_ok()).collect();
    assert_eq!(
        successful.len(),
        num_concurrent,
        "All concurrent assignments should succeed"
    );

    // All should return the same MC (the winner)
    let mc_ids: HashSet<String> = results
        .iter()
        .filter_map(|r| r.as_ref().ok())
        .map(|a| a.mc_assignment.mc_id.clone())
        .collect();

    assert_eq!(
        mc_ids.len(),
        1,
        "All concurrent assignments should return the same MC, got: {:?}",
        mc_ids
    );

    // Verify only one assignment exists in database
    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM meeting_assignments WHERE meeting_id = $1 AND ended_at IS NULL",
    )
    .bind("meeting-concurrent-test")
    .fetch_one(pool.as_ref())
    .await?;

    assert_eq!(
        count.0, 1,
        "Only one active assignment should exist in database"
    );

    Ok(())
}

/// Test MC health transition - verifies assignment migrates to healthy MC.
///
/// Per ADR-0010, when an assigned MC becomes unhealthy, subsequent join
/// attempts should create a new assignment to a healthy MC. The unhealthy
/// assignment is deleted (not soft-deleted) to free up the PK constraint.
#[sqlx::test(migrations = "../../migrations")]
async fn test_mc_health_transition_creates_new_assignment(
    pool: PgPool,
) -> Result<(), anyhow::Error> {
    // Register two healthy MCs and MHs
    register_healthy_mc(&pool, "mc-healthy-1", "us-east-1", 10, 100).await?;
    register_healthy_mc(&pool, "mc-healthy-2", "us-east-1", 20, 100).await?;
    register_healthy_mhs_for_region(&pool, "us-east-1").await?;

    let mc_client = Arc::new(MockMcClient::accepting());

    // First assignment goes to one of the healthy MCs
    let first_assignment = McAssignmentService::assign_meeting_with_mh(
        &pool,
        mc_client.clone(),
        "meeting-health-test",
        "us-east-1",
        "gc-1",
    )
    .await?;

    let first_mc = first_assignment.mc_assignment.mc_id.clone();

    // Now mark the assigned MC as unhealthy
    MeetingControllersRepository::update_heartbeat(
        &pool,
        &first_mc,
        10,
        100,
        HealthStatus::Unhealthy,
    )
    .await?;

    // Second assignment attempt should get a different (healthy) MC
    let second_assignment = McAssignmentService::assign_meeting_with_mh(
        &pool,
        mc_client,
        "meeting-health-test",
        "us-east-1",
        "gc-2",
    )
    .await?;

    // The assignment should now be to the remaining healthy MC
    assert_ne!(
        second_assignment.mc_assignment.mc_id, first_mc,
        "Should assign to a different MC after first became unhealthy"
    );

    // Verify exactly one active assignment exists (unhealthy one was deleted)
    let active_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM meeting_assignments WHERE meeting_id = $1 AND ended_at IS NULL",
    )
    .bind("meeting-health-test")
    .fetch_one(&pool)
    .await?;

    assert_eq!(
        active_count.0, 1,
        "Should have exactly one active assignment"
    );

    // Total count should be 1 (unhealthy assignment was deleted, not soft-deleted)
    let total_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM meeting_assignments WHERE meeting_id = $1")
            .bind("meeting-health-test")
            .fetch_one(&pool)
            .await?;

    assert_eq!(
        total_count.0, 1,
        "Should have exactly one assignment total (unhealthy one was deleted)"
    );

    Ok(())
}
