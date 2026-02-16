//! Meeting Controller repository integration tests.
//!
//! Tests database operations for the meeting_controllers table using
//! `#[sqlx::test]` for isolated test databases.

use chrono::Utc;
use gc_service::repositories::{HealthStatus, MeetingControllersRepository};
use sqlx::PgPool;

/// Test that register_mc creates a new record.
#[sqlx::test(migrations = "../../migrations")]
async fn test_register_mc_creates_new_record(pool: PgPool) -> Result<(), anyhow::Error> {
    MeetingControllersRepository::register_mc(
        &pool,
        "mc-test-001",
        "us-east-1",
        "https://mc1.example.com:50051",
        Some("https://mc1.example.com:443"),
        100,
        1000,
    )
    .await?;

    // Verify record was created
    let record = MeetingControllersRepository::get_controller(&pool, "mc-test-001").await?;
    let mc = record.expect("Record should exist after registration");

    assert_eq!(mc.controller_id, "mc-test-001");
    assert_eq!(mc.region, "us-east-1");
    assert_eq!(mc.grpc_endpoint, "https://mc1.example.com:50051");
    assert_eq!(
        mc.webtransport_endpoint,
        Some("https://mc1.example.com:443".to_string())
    );
    assert_eq!(mc.max_meetings, 100);
    assert_eq!(mc.max_participants, 1000);
    assert_eq!(mc.health_status, HealthStatus::Pending);
    assert_eq!(mc.current_meetings, 0);
    assert_eq!(mc.current_participants, 0);

    Ok(())
}

/// Test that register_mc with upsert updates an existing record.
#[sqlx::test(migrations = "../../migrations")]
async fn test_register_mc_upsert_updates_existing(pool: PgPool) -> Result<(), anyhow::Error> {
    // Initial registration
    MeetingControllersRepository::register_mc(
        &pool,
        "mc-upsert-001",
        "us-east-1",
        "https://mc1.example.com:50051",
        None,
        100,
        1000,
    )
    .await?;

    // Set the controller to healthy via heartbeat
    MeetingControllersRepository::update_heartbeat(
        &pool,
        "mc-upsert-001",
        10,
        50,
        HealthStatus::Healthy,
    )
    .await?;

    // Re-register with different values (simulating controller restart)
    MeetingControllersRepository::register_mc(
        &pool,
        "mc-upsert-001",
        "us-west-2", // Changed region
        "https://mc1-new.example.com:50051",
        Some("https://mc1-new.example.com:443"),
        200, // Increased capacity
        2000,
    )
    .await?;

    // Verify record was updated (not duplicated)
    let record = MeetingControllersRepository::get_controller(&pool, "mc-upsert-001").await?;
    let mc = record.expect("Record should exist after re-registration");

    assert_eq!(mc.region, "us-west-2");
    assert_eq!(mc.grpc_endpoint, "https://mc1-new.example.com:50051");
    assert_eq!(
        mc.webtransport_endpoint,
        Some("https://mc1-new.example.com:443".to_string())
    );
    assert_eq!(mc.max_meetings, 200);
    assert_eq!(mc.max_participants, 2000);
    // Health status should reset to pending on re-registration
    assert_eq!(mc.health_status, HealthStatus::Pending);

    Ok(())
}

/// Test that update_heartbeat succeeds for registered controller.
#[sqlx::test(migrations = "../../migrations")]
async fn test_update_heartbeat_success(pool: PgPool) -> Result<(), anyhow::Error> {
    // Register controller first
    MeetingControllersRepository::register_mc(
        &pool,
        "mc-heartbeat-001",
        "us-east-1",
        "https://mc1.example.com:50051",
        None,
        100,
        1000,
    )
    .await?;

    let timestamp_before = Utc::now();

    // Send heartbeat with capacity update
    let updated = MeetingControllersRepository::update_heartbeat(
        &pool,
        "mc-heartbeat-001",
        15,
        75,
        HealthStatus::Healthy,
    )
    .await?;

    assert!(
        updated,
        "Heartbeat should succeed for registered controller"
    );

    // Verify values were updated
    let record = MeetingControllersRepository::get_controller(&pool, "mc-heartbeat-001").await?;
    let mc = record.expect("Record should exist");

    assert_eq!(mc.current_meetings, 15);
    assert_eq!(mc.current_participants, 75);
    assert_eq!(mc.health_status, HealthStatus::Healthy);
    assert!(
        mc.last_heartbeat_at >= timestamp_before,
        "last_heartbeat_at should be updated"
    );

    Ok(())
}

/// Test that update_heartbeat returns false for missing controller.
#[sqlx::test(migrations = "../../migrations")]
async fn test_update_heartbeat_returns_false_for_missing(
    pool: PgPool,
) -> Result<(), anyhow::Error> {
    // Try to update heartbeat for a non-existent controller
    let updated = MeetingControllersRepository::update_heartbeat(
        &pool,
        "mc-nonexistent-001",
        10,
        50,
        HealthStatus::Healthy,
    )
    .await?;

    assert!(
        !updated,
        "Heartbeat should fail for non-existent controller"
    );

    Ok(())
}

/// Test that mark_stale_controllers_unhealthy marks stale controllers.
#[sqlx::test(migrations = "../../migrations")]
async fn test_mark_stale_controllers_unhealthy_marks_stale(
    pool: PgPool,
) -> Result<(), anyhow::Error> {
    // Register two controllers
    MeetingControllersRepository::register_mc(
        &pool,
        "mc-stale-001",
        "us-east-1",
        "https://mc1.example.com:50051",
        None,
        100,
        1000,
    )
    .await?;

    MeetingControllersRepository::register_mc(
        &pool,
        "mc-active-001",
        "us-east-1",
        "https://mc2.example.com:50051",
        None,
        100,
        1000,
    )
    .await?;

    // Set both to healthy
    MeetingControllersRepository::update_heartbeat(
        &pool,
        "mc-stale-001",
        5,
        25,
        HealthStatus::Healthy,
    )
    .await?;

    MeetingControllersRepository::update_heartbeat(
        &pool,
        "mc-active-001",
        10,
        50,
        HealthStatus::Healthy,
    )
    .await?;

    // Manually set mc-stale-001 to have an old heartbeat (2 minutes ago)
    sqlx::query(
        "UPDATE meeting_controllers SET last_heartbeat_at = NOW() - INTERVAL '120 seconds' WHERE controller_id = $1"
    )
    .bind("mc-stale-001")
    .execute(&pool)
    .await?;

    // Mark controllers stale after 60 seconds of no heartbeat
    let marked_count =
        MeetingControllersRepository::mark_stale_controllers_unhealthy(&pool, 60).await?;

    assert_eq!(
        marked_count, 1,
        "Should mark exactly one controller as stale"
    );

    // Verify mc-stale-001 is unhealthy
    let stale = MeetingControllersRepository::get_controller(&pool, "mc-stale-001").await?;
    assert_eq!(
        stale.unwrap().health_status,
        HealthStatus::Unhealthy,
        "Stale controller should be marked unhealthy"
    );

    // Verify mc-active-001 is still healthy
    let active = MeetingControllersRepository::get_controller(&pool, "mc-active-001").await?;
    assert_eq!(
        active.unwrap().health_status,
        HealthStatus::Healthy,
        "Active controller should remain healthy"
    );

    Ok(())
}

/// Test that mark_stale_controllers_unhealthy skips draining controllers.
#[sqlx::test(migrations = "../../migrations")]
async fn test_mark_stale_controllers_unhealthy_skips_draining(
    pool: PgPool,
) -> Result<(), anyhow::Error> {
    // Register a controller
    MeetingControllersRepository::register_mc(
        &pool,
        "mc-draining-001",
        "us-east-1",
        "https://mc1.example.com:50051",
        None,
        100,
        1000,
    )
    .await?;

    // Set to draining status
    MeetingControllersRepository::update_heartbeat(
        &pool,
        "mc-draining-001",
        5,
        25,
        HealthStatus::Draining,
    )
    .await?;

    // Manually set old heartbeat
    sqlx::query(
        "UPDATE meeting_controllers SET last_heartbeat_at = NOW() - INTERVAL '300 seconds' WHERE controller_id = $1"
    )
    .bind("mc-draining-001")
    .execute(&pool)
    .await?;

    // Try to mark stale controllers
    let marked_count =
        MeetingControllersRepository::mark_stale_controllers_unhealthy(&pool, 60).await?;

    assert_eq!(
        marked_count, 0,
        "Should not mark draining controller as unhealthy"
    );

    // Verify controller is still draining
    let mc = MeetingControllersRepository::get_controller(&pool, "mc-draining-001").await?;
    assert_eq!(
        mc.unwrap().health_status,
        HealthStatus::Draining,
        "Draining controller should preserve its status"
    );

    Ok(())
}

/// Test that get_controller returns a record when it exists.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_controller_returns_record(pool: PgPool) -> Result<(), anyhow::Error> {
    // Register a controller
    MeetingControllersRepository::register_mc(
        &pool,
        "mc-get-001",
        "eu-west-1",
        "https://mc1.eu.example.com:50051",
        Some("https://mc1.eu.example.com:443"),
        50,
        500,
    )
    .await?;

    // Retrieve and verify
    let record = MeetingControllersRepository::get_controller(&pool, "mc-get-001").await?;
    let mc = record.expect("Should return the registered controller");

    assert_eq!(mc.controller_id, "mc-get-001");
    assert_eq!(mc.region, "eu-west-1");
    assert_eq!(mc.grpc_endpoint, "https://mc1.eu.example.com:50051");
    assert_eq!(
        mc.webtransport_endpoint,
        Some("https://mc1.eu.example.com:443".to_string())
    );
    assert_eq!(mc.max_meetings, 50);
    assert_eq!(mc.max_participants, 500);

    Ok(())
}

/// Test that get_controller returns None for missing controller.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_controller_returns_none_for_missing(pool: PgPool) -> Result<(), anyhow::Error> {
    let record = MeetingControllersRepository::get_controller(&pool, "mc-does-not-exist").await?;

    assert!(
        record.is_none(),
        "Should return None for non-existent controller"
    );

    Ok(())
}

/// Test that multiple heartbeats update values correctly.
#[sqlx::test(migrations = "../../migrations")]
async fn test_multiple_heartbeats_update_correctly(pool: PgPool) -> Result<(), anyhow::Error> {
    // Register controller
    MeetingControllersRepository::register_mc(
        &pool,
        "mc-multi-hb",
        "us-east-1",
        "https://mc1.example.com:50051",
        None,
        100,
        1000,
    )
    .await?;

    // First heartbeat - pending -> healthy
    MeetingControllersRepository::update_heartbeat(
        &pool,
        "mc-multi-hb",
        10,
        50,
        HealthStatus::Healthy,
    )
    .await?;

    let mc = MeetingControllersRepository::get_controller(&pool, "mc-multi-hb")
        .await?
        .unwrap();
    assert_eq!(mc.health_status, HealthStatus::Healthy);
    assert_eq!(mc.current_meetings, 10);

    // Second heartbeat - increase load
    MeetingControllersRepository::update_heartbeat(
        &pool,
        "mc-multi-hb",
        20,
        100,
        HealthStatus::Healthy,
    )
    .await?;

    let mc = MeetingControllersRepository::get_controller(&pool, "mc-multi-hb")
        .await?
        .unwrap();
    assert_eq!(mc.current_meetings, 20);
    assert_eq!(mc.current_participants, 100);

    // Third heartbeat - report degraded
    MeetingControllersRepository::update_heartbeat(
        &pool,
        "mc-multi-hb",
        20,
        100,
        HealthStatus::Degraded,
    )
    .await?;

    let mc = MeetingControllersRepository::get_controller(&pool, "mc-multi-hb")
        .await?
        .unwrap();
    assert_eq!(mc.health_status, HealthStatus::Degraded);

    Ok(())
}
