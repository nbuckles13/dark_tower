//! Participant and meeting activation repository integration tests.
//!
//! Tests the ParticipantsRepository functionality including:
//! - Adding participants to meetings
//! - Counting active participants for capacity checks
//! - Removing participants (soft delete via left_at)
//! - Unique active participant constraint enforcement
//! - Rejoin after leaving
//! - Capacity check against max_participants
//! - Guest participant support (NULL user_id)
//!
//! Tests the MeetingsRepository::activate_meeting functionality including:
//! - Scheduled → active transition with actual_start_time
//! - Idempotent no-op when already active
//! - No-op for ended and cancelled meetings
//! - Audit log entry creation

#![allow(clippy::unwrap_used, clippy::expect_used)]

use gc_service::repositories::{MeetingsRepository, ParticipantsRepository};
use sqlx::PgPool;
use uuid::Uuid;

/// Insert fixture org, user, and meeting rows for testing.
/// Returns (org_id, user_id, meeting_id).
async fn create_test_fixtures(pool: &PgPool) -> (Uuid, Uuid, Uuid) {
    create_test_fixtures_with_max(pool, 100).await
}

/// Create an additional user in the given org. Returns user_id.
async fn create_extra_user(pool: &PgPool, org_id: Uuid) -> Uuid {
    let user_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO users (user_id, org_id, email, password_hash, display_name)
        VALUES ($1, $2, $3, 'hashed', 'Extra User')
        "#,
    )
    .bind(user_id)
    .bind(org_id)
    .bind(format!("extra-{}@example.com", &user_id.to_string()[..8]))
    .execute(pool)
    .await
    .expect("Failed to insert extra user");
    user_id
}

/// Insert fixture rows with a specific max_participants value.
/// Returns (org_id, user_id, meeting_id).
async fn create_test_fixtures_with_max(pool: &PgPool, max_participants: i32) -> (Uuid, Uuid, Uuid) {
    let org_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let meeting_id = Uuid::new_v4();

    sqlx::query(
        r#"
        INSERT INTO organizations (org_id, subdomain, display_name, plan_tier)
        VALUES ($1, $2, 'Test Org', 'free')
        "#,
    )
    .bind(org_id)
    .bind(format!("test-{}", &org_id.to_string()[..8]))
    .execute(pool)
    .await
    .expect("Failed to insert test org");

    sqlx::query(
        r#"
        INSERT INTO users (user_id, org_id, email, password_hash, display_name)
        VALUES ($1, $2, $3, 'hashed', 'Test User')
        "#,
    )
    .bind(user_id)
    .bind(org_id)
    .bind(format!("test-{}@example.com", &user_id.to_string()[..8]))
    .execute(pool)
    .await
    .expect("Failed to insert test user");

    sqlx::query(
        r#"
        INSERT INTO meetings (meeting_id, org_id, created_by_user_id, display_name,
                              meeting_code, join_token_secret, max_participants, status)
        VALUES ($1, $2, $3, 'Test Meeting', $4, 'secret123', $5, 'active')
        "#,
    )
    .bind(meeting_id)
    .bind(org_id)
    .bind(user_id)
    .bind(format!("CODE{}", &meeting_id.to_string()[..8]))
    .bind(max_participants)
    .execute(pool)
    .await
    .expect("Failed to insert test meeting");

    (org_id, user_id, meeting_id)
}

// ============================================================================
// Repository Tests
// ============================================================================

#[sqlx::test(migrations = "../../migrations")]
async fn test_add_participant(pool: PgPool) -> Result<(), anyhow::Error> {
    let (_org_id, user_id, meeting_id) = create_test_fixtures(&pool).await;

    let participant = ParticipantsRepository::add_participant(
        &pool,
        meeting_id,
        Some(user_id),
        "Test User",
        "member",
        "host",
    )
    .await?;

    assert_eq!(participant.meeting_id, meeting_id);
    assert_eq!(participant.user_id, Some(user_id));
    assert_eq!(participant.display_name, "Test User");
    assert_eq!(participant.participant_type, "member");
    assert_eq!(participant.role, "host");
    assert!(
        participant.left_at.is_none(),
        "New participant should be active"
    );

    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_count_active_participants(pool: PgPool) -> Result<(), anyhow::Error> {
    let (org_id, user_id, meeting_id) = create_test_fixtures(&pool).await;

    // Initially zero
    let count = ParticipantsRepository::count_active_participants(&pool, meeting_id).await?;
    assert_eq!(count, 0);

    // Add first participant (the meeting creator)
    ParticipantsRepository::add_participant(
        &pool,
        meeting_id,
        Some(user_id),
        "Test User",
        "member",
        "host",
    )
    .await?;
    let count = ParticipantsRepository::count_active_participants(&pool, meeting_id).await?;
    assert_eq!(count, 1);

    // Add second participant (different user)
    let user2 = create_extra_user(&pool, org_id).await;
    ParticipantsRepository::add_participant(
        &pool,
        meeting_id,
        Some(user2),
        "Extra User",
        "external",
        "participant",
    )
    .await?;
    let count = ParticipantsRepository::count_active_participants(&pool, meeting_id).await?;
    assert_eq!(count, 2);

    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_remove_participant(pool: PgPool) -> Result<(), anyhow::Error> {
    let (_org_id, user_id, meeting_id) = create_test_fixtures(&pool).await;

    ParticipantsRepository::add_participant(
        &pool,
        meeting_id,
        Some(user_id),
        "Test User",
        "member",
        "participant",
    )
    .await?;
    assert_eq!(
        ParticipantsRepository::count_active_participants(&pool, meeting_id).await?,
        1
    );

    // Remove the participant
    let removed = ParticipantsRepository::remove_participant(&pool, meeting_id, user_id).await?;
    assert!(removed, "Should return true when participant was removed");

    // Count should drop to zero
    let count = ParticipantsRepository::count_active_participants(&pool, meeting_id).await?;
    assert_eq!(count, 0);

    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_remove_nonexistent_participant(pool: PgPool) -> Result<(), anyhow::Error> {
    let (_org_id, _user_id, meeting_id) = create_test_fixtures(&pool).await;

    let removed =
        ParticipantsRepository::remove_participant(&pool, meeting_id, Uuid::new_v4()).await?;
    assert!(
        !removed,
        "Should return false when no active participant found"
    );

    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_duplicate_active_participant_rejected(pool: PgPool) -> Result<(), anyhow::Error> {
    let (_org_id, user_id, meeting_id) = create_test_fixtures(&pool).await;

    // First add succeeds
    ParticipantsRepository::add_participant(
        &pool,
        meeting_id,
        Some(user_id),
        "Test User",
        "member",
        "participant",
    )
    .await?;

    // Second add with same user should fail (unique constraint)
    let result = ParticipantsRepository::add_participant(
        &pool,
        meeting_id,
        Some(user_id),
        "Test User",
        "member",
        "participant",
    )
    .await;
    assert!(
        result.is_err(),
        "Should reject duplicate active participant"
    );

    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_rejoin_after_leaving(pool: PgPool) -> Result<(), anyhow::Error> {
    let (_org_id, user_id, meeting_id) = create_test_fixtures(&pool).await;

    // Join
    ParticipantsRepository::add_participant(
        &pool,
        meeting_id,
        Some(user_id),
        "Test User",
        "member",
        "participant",
    )
    .await?;

    // Leave
    ParticipantsRepository::remove_participant(&pool, meeting_id, user_id).await?;
    assert_eq!(
        ParticipantsRepository::count_active_participants(&pool, meeting_id).await?,
        0
    );

    // Rejoin should succeed (previous row has left_at set, so unique constraint allows new row)
    let participant = ParticipantsRepository::add_participant(
        &pool,
        meeting_id,
        Some(user_id),
        "Test User",
        "member",
        "participant",
    )
    .await?;
    assert!(
        participant.left_at.is_none(),
        "Rejoined participant should be active"
    );
    assert_eq!(
        ParticipantsRepository::count_active_participants(&pool, meeting_id).await?,
        1
    );

    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_capacity_check_against_max_participants(pool: PgPool) -> Result<(), anyhow::Error> {
    let max_participants = 3;
    let (org_id, user_id, meeting_id) =
        create_test_fixtures_with_max(&pool, max_participants).await;

    // Add participants up to capacity
    ParticipantsRepository::add_participant(
        &pool,
        meeting_id,
        Some(user_id),
        "Host User",
        "member",
        "host",
    )
    .await?;
    let user2 = create_extra_user(&pool, org_id).await;
    ParticipantsRepository::add_participant(
        &pool,
        meeting_id,
        Some(user2),
        "User Two",
        "member",
        "participant",
    )
    .await?;
    let user3 = create_extra_user(&pool, org_id).await;
    ParticipantsRepository::add_participant(
        &pool,
        meeting_id,
        Some(user3),
        "User Three",
        "external",
        "participant",
    )
    .await?;

    // Verify count matches max_participants
    let count = ParticipantsRepository::count_active_participants(&pool, meeting_id).await?;
    assert_eq!(count, i64::from(max_participants));

    // Verify the capacity check pattern: count >= max_participants means full
    let max: (i32,) = sqlx::query_as("SELECT max_participants FROM meetings WHERE meeting_id = $1")
        .bind(meeting_id)
        .fetch_one(&pool)
        .await?;
    assert!(
        count >= i64::from(max.0),
        "Meeting should be at capacity: {} active >= {} max",
        count,
        max.0
    );

    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_invalid_participant_type_rejected(pool: PgPool) -> Result<(), anyhow::Error> {
    let (_org_id, user_id, meeting_id) = create_test_fixtures(&pool).await;

    let result = ParticipantsRepository::add_participant(
        &pool,
        meeting_id,
        Some(user_id),
        "Test User",
        "admin", // invalid participant_type
        "participant",
    )
    .await;
    assert!(
        result.is_err(),
        "Should reject invalid participant_type 'admin'"
    );

    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_invalid_role_rejected(pool: PgPool) -> Result<(), anyhow::Error> {
    let (_org_id, user_id, meeting_id) = create_test_fixtures(&pool).await;

    let result = ParticipantsRepository::add_participant(
        &pool,
        meeting_id,
        Some(user_id),
        "Test User",
        "member",
        "moderator", // invalid role
    )
    .await;
    assert!(result.is_err(), "Should reject invalid role 'moderator'");

    Ok(())
}

// ============================================================================
// Guest Participant Tests
// ============================================================================

#[sqlx::test(migrations = "../../migrations")]
async fn test_add_guest_participant(pool: PgPool) -> Result<(), anyhow::Error> {
    let (_org_id, _user_id, meeting_id) = create_test_fixtures(&pool).await;

    let participant = ParticipantsRepository::add_participant(
        &pool,
        meeting_id,
        None, // guest — no user_id
        "Anonymous Guest",
        "guest",
        "guest",
    )
    .await?;

    assert_eq!(participant.meeting_id, meeting_id);
    assert_eq!(participant.user_id, None);
    assert_eq!(participant.display_name, "Anonymous Guest");
    assert_eq!(participant.participant_type, "guest");
    assert_eq!(participant.role, "guest");
    assert!(
        participant.left_at.is_none(),
        "New guest participant should be active"
    );

    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_multiple_guests_allowed(pool: PgPool) -> Result<(), anyhow::Error> {
    let (_org_id, _user_id, meeting_id) = create_test_fixtures(&pool).await;

    // Add multiple guests with user_id = None — the partial unique index
    // treats NULLs as distinct, so this should succeed
    ParticipantsRepository::add_participant(&pool, meeting_id, None, "Guest One", "guest", "guest")
        .await?;

    ParticipantsRepository::add_participant(&pool, meeting_id, None, "Guest Two", "guest", "guest")
        .await?;

    ParticipantsRepository::add_participant(
        &pool,
        meeting_id,
        None,
        "Guest Three",
        "guest",
        "guest",
    )
    .await?;

    let count = ParticipantsRepository::count_active_participants(&pool, meeting_id).await?;
    assert_eq!(count, 3, "All three guest participants should be counted");

    Ok(())
}

// ============================================================================
// Meeting Activation Tests (R-10)
// ============================================================================

/// Insert fixture rows with a specific meeting status.
/// Returns (org_id, user_id, meeting_id).
async fn create_test_fixtures_with_status(pool: &PgPool, status: &str) -> (Uuid, Uuid, Uuid) {
    let org_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let meeting_id = Uuid::new_v4();

    sqlx::query(
        r#"
        INSERT INTO organizations (org_id, subdomain, display_name, plan_tier)
        VALUES ($1, $2, 'Test Org', 'free')
        "#,
    )
    .bind(org_id)
    .bind(format!("test-{}", &org_id.to_string()[..8]))
    .execute(pool)
    .await
    .expect("Failed to insert test org");

    sqlx::query(
        r#"
        INSERT INTO users (user_id, org_id, email, password_hash, display_name)
        VALUES ($1, $2, $3, 'hashed', 'Test User')
        "#,
    )
    .bind(user_id)
    .bind(org_id)
    .bind(format!("test-{}@example.com", &user_id.to_string()[..8]))
    .execute(pool)
    .await
    .expect("Failed to insert test user");

    sqlx::query(
        r#"
        INSERT INTO meetings (meeting_id, org_id, created_by_user_id, display_name,
                              meeting_code, join_token_secret, max_participants, status)
        VALUES ($1, $2, $3, 'Test Meeting', $4, 'secret123', 100, $5)
        "#,
    )
    .bind(meeting_id)
    .bind(org_id)
    .bind(user_id)
    .bind(format!("CODE{}", &meeting_id.to_string()[..8]))
    .bind(status)
    .execute(pool)
    .await
    .expect("Failed to insert test meeting");

    (org_id, user_id, meeting_id)
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_activate_meeting_scheduled_to_active(pool: PgPool) -> Result<(), anyhow::Error> {
    let (_org_id, _user_id, meeting_id) =
        create_test_fixtures_with_status(&pool, "scheduled").await;

    // Activate the meeting
    let result = MeetingsRepository::activate_meeting(&pool, meeting_id).await?;
    assert!(
        result.is_some(),
        "Should return Some when transitioning from scheduled to active"
    );

    let (returned_meeting_id, returned_org_id) = result.unwrap();
    assert_eq!(returned_meeting_id, meeting_id);
    assert_eq!(returned_org_id, _org_id);

    // Verify the meeting is now active with actual_start_time set
    let row: (String, Option<chrono::DateTime<chrono::Utc>>) =
        sqlx::query_as("SELECT status, actual_start_time FROM meetings WHERE meeting_id = $1")
            .bind(meeting_id)
            .fetch_one(&pool)
            .await?;

    assert_eq!(row.0, "active", "Meeting status should be 'active'");
    assert!(
        row.1.is_some(),
        "actual_start_time should be set after activation"
    );

    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_activate_meeting_already_active_noop(pool: PgPool) -> Result<(), anyhow::Error> {
    let (_org_id, _user_id, meeting_id) =
        create_test_fixtures_with_status(&pool, "scheduled").await;

    // First activation succeeds
    let result = MeetingsRepository::activate_meeting(&pool, meeting_id).await?;
    assert!(result.is_some(), "First activation should succeed");

    // Second activation is a no-op (meeting is now 'active', not 'scheduled')
    let result = MeetingsRepository::activate_meeting(&pool, meeting_id).await?;
    assert!(
        result.is_none(),
        "Second activation should be a no-op (already active)"
    );

    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_activate_meeting_ended_noop(pool: PgPool) -> Result<(), anyhow::Error> {
    let (_org_id, _user_id, meeting_id) =
        create_test_fixtures_with_status(&pool, "scheduled").await;

    // Manually set the meeting to 'ended'
    sqlx::query("UPDATE meetings SET status = 'ended' WHERE meeting_id = $1")
        .bind(meeting_id)
        .execute(&pool)
        .await?;

    // Activation should be a no-op
    let result = MeetingsRepository::activate_meeting(&pool, meeting_id).await?;
    assert!(result.is_none(), "Should not activate an ended meeting");

    // Verify status unchanged
    let (status,): (String,) = sqlx::query_as("SELECT status FROM meetings WHERE meeting_id = $1")
        .bind(meeting_id)
        .fetch_one(&pool)
        .await?;
    assert_eq!(status, "ended");

    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_activate_meeting_cancelled_noop(pool: PgPool) -> Result<(), anyhow::Error> {
    let (_org_id, _user_id, meeting_id) =
        create_test_fixtures_with_status(&pool, "scheduled").await;

    // Manually set the meeting to 'cancelled'
    sqlx::query("UPDATE meetings SET status = 'cancelled' WHERE meeting_id = $1")
        .bind(meeting_id)
        .execute(&pool)
        .await?;

    // Activation should be a no-op
    let result = MeetingsRepository::activate_meeting(&pool, meeting_id).await?;
    assert!(result.is_none(), "Should not activate a cancelled meeting");

    // Verify status unchanged
    let (status,): (String,) = sqlx::query_as("SELECT status FROM meetings WHERE meeting_id = $1")
        .bind(meeting_id)
        .fetch_one(&pool)
        .await?;
    assert_eq!(status, "cancelled");

    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_activate_meeting_audit_log(pool: PgPool) -> Result<(), anyhow::Error> {
    let (org_id, user_id, meeting_id) = create_test_fixtures_with_status(&pool, "scheduled").await;

    // Activate the meeting
    let result = MeetingsRepository::activate_meeting(&pool, meeting_id).await?;
    assert!(result.is_some());

    // Log the audit event (fire-and-forget in production, but we verify here)
    MeetingsRepository::log_audit_event(
        &pool,
        org_id,
        Some(user_id),
        meeting_id,
        "meeting_activated",
    )
    .await?;

    // Verify audit log entry exists
    let (action, resource_type, resource_id): (String, String, Option<Uuid>) = sqlx::query_as(
        r#"
        SELECT action, resource_type, resource_id
        FROM audit_logs
        WHERE org_id = $1 AND action = 'meeting_activated'
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .bind(org_id)
    .fetch_one(&pool)
    .await?;

    assert_eq!(action, "meeting_activated");
    assert_eq!(resource_type, "meeting");
    assert_eq!(resource_id, Some(meeting_id));

    Ok(())
}
