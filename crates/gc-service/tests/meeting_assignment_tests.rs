//! Meeting assignment integration tests.
//!
//! Tests the MC assignment functionality including:
//! - Load balancing MC selection
//! - Atomic assignment with race condition handling
//! - Existing assignment reuse (MC LIVENESS, not status)
//! - The incarnation fence (`assignment_id`) and the accept->commit budget
//! - Assignment cleanup

use gc_service::config::McAssignmentConfig;
use gc_service::repositories::{
    AssignmentId, AtomicAssignOutcome, EndAssignmentOutcome, HealthStatus, McAssignment,
    McCandidate, MeetingAssignmentsRepository, MeetingControllersRepository,
};
use gc_service::services::mc_client::{McAssignmentResult, McRejectionReason};
use gc_service::services::{McAssignmentService, MockMcClient};
use sqlx::PgPool;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The MC liveness threshold these tests run with.
const STALENESS: Duration = Duration::from_secs(30);

/// Assignment config with the shipped values.
fn mc_cfg() -> McAssignmentConfig {
    gc_test_utils::reference_mc_assignment()
}

/// A deadline comfortably in the future (the budget is not under test).
fn far_deadline() -> Instant {
    Instant::now() + Duration::from_secs(5)
}

/// `atomic_assign` with a fresh id, unpacking `Assigned`. Returns the row's
/// MC, whether this call wrote it, and the id this call offered.
async fn assign(
    pool: &PgPool,
    meeting_id: &str,
    region: &str,
    mc: &McCandidate,
    gc_id: &str,
) -> Result<(McAssignment, bool, AssignmentId), anyhow::Error> {
    let id = AssignmentId::new_v4();
    match MeetingAssignmentsRepository::atomic_assign(
        pool,
        meeting_id,
        region,
        mc,
        gc_id,
        id,
        STALENESS,
        far_deadline(),
    )
    .await?
    {
        AtomicAssignOutcome::Assigned {
            assignment,
            written,
        } => Ok((assignment, written, id)),
        AtomicAssignOutcome::BudgetExceeded => {
            anyhow::bail!("unexpected BudgetExceeded with a 5 s deadline")
        }
    }
}

/// The (controller, assignment_id) of the row for (meeting, region).
async fn row_fence(
    pool: &PgPool,
    meeting_id: &str,
    region: &str,
) -> Result<(String, uuid::Uuid), anyhow::Error> {
    Ok(sqlx::query_as(
        "SELECT meeting_controller_id, assignment_id FROM meeting_assignments \
         WHERE meeting_id = $1 AND region = $2",
    )
    .bind(meeting_id)
    .bind(region)
    .fetch_one(pool)
    .await?)
}

/// Make an MC's heartbeat older than the liveness threshold (it is dead to
/// GC), leaving its `health_status` as it was.
async fn make_stale(pool: &PgPool, mc_id: &str) -> Result<(), anyhow::Error> {
    sqlx::query(
        "UPDATE meeting_controllers SET last_heartbeat_at = NOW() - INTERVAL '100 seconds' \
         WHERE controller_id = $1",
    )
    .bind(mc_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Set an MC's `health_status` directly, heartbeat untouched (fresh).
async fn set_status(pool: &PgPool, mc_id: &str, status: &str) -> Result<(), anyhow::Error> {
    sqlx::query("UPDATE meeting_controllers SET health_status = $2 WHERE controller_id = $1")
        .bind(mc_id)
        .bind(status)
        .execute(pool)
        .await?;
    Ok(())
}

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
    // Register first MH peer
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
    .bind(format!("mh-1-{}", region))
    .bind(region)
    .bind(format!("https://mh-1-{}.example.com:443", region))
    .bind(format!("grpc://mh-1-{}.example.com:50051", region))
    .execute(pool)
    .await?;

    // Register second MH peer
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
    .bind(format!("mh-2-{}", region))
    .bind(region)
    .bind(format!("https://mh-2-{}.example.com:443", region))
    .bind(format!("grpc://mh-2-{}.example.com:50051", region))
    .execute(pool)
    .await?;

    Ok(())
}

// ============================================================================
// Repository Tests
// ============================================================================

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_live_assignment_none_when_empty(pool: PgPool) -> Result<(), anyhow::Error> {
    let result = MeetingAssignmentsRepository::get_live_assignment(
        &pool,
        "meeting-1",
        "us-east-1",
        STALENESS,
    )
    .await?;
    assert!(
        result.is_none(),
        "Should return None when no assignments exist"
    );
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_candidate_mcs_empty_when_no_healthy(pool: PgPool) -> Result<(), anyhow::Error> {
    let candidates =
        MeetingAssignmentsRepository::get_candidate_mcs(&pool, "us-east-1", STALENESS).await?;
    assert!(
        candidates.is_empty(),
        "Should return empty when no healthy MCs"
    );
    Ok(())
}

/// Placement takes only healthy AND live MCs: a fresh unhealthy/draining MC
/// and a stale healthy-status MC are both excluded.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_candidate_mcs_returns_healthy_live_only(
    pool: PgPool,
) -> Result<(), anyhow::Error> {
    register_healthy_mc(&pool, "mc-healthy-1", "us-east-1", 10, 100).await?;
    register_healthy_mc(&pool, "mc-healthy-2", "us-east-1", 20, 100).await?;
    register_healthy_mc(&pool, "mc-unhealthy", "us-east-1", 0, 100).await?;
    MeetingControllersRepository::update_heartbeat(
        &pool,
        "mc-unhealthy",
        0,
        0,
        HealthStatus::Unhealthy,
    )
    .await?;
    register_healthy_mc(&pool, "mc-draining", "us-east-1", 0, 100).await?;
    set_status(&pool, "mc-draining", "draining").await?;
    register_healthy_mc(&pool, "mc-stale", "us-east-1", 0, 100).await?;
    make_stale(&pool, "mc-stale").await?;

    let candidates =
        MeetingAssignmentsRepository::get_candidate_mcs(&pool, "us-east-1", STALENESS).await?;

    assert_eq!(candidates.len(), 2, "Should return only healthy, live MCs");
    assert!(candidates
        .iter()
        .all(|c| c.controller_id.starts_with("mc-healthy")));
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_candidate_mcs_ordered_by_load(pool: PgPool) -> Result<(), anyhow::Error> {
    register_healthy_mc(&pool, "mc-heavy", "us-east-1", 90, 100).await?;
    register_healthy_mc(&pool, "mc-light", "us-east-1", 10, 100).await?;
    register_healthy_mc(&pool, "mc-medium", "us-east-1", 50, 100).await?;

    let candidates =
        MeetingAssignmentsRepository::get_candidate_mcs(&pool, "us-east-1", STALENESS).await?;

    let order: Vec<&str> = candidates
        .iter()
        .map(|c| c.controller_id.as_str())
        .collect();
    assert_eq!(order, vec!["mc-light", "mc-medium", "mc-heavy"]);
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_candidate_mcs_excludes_full(pool: PgPool) -> Result<(), anyhow::Error> {
    register_healthy_mc(&pool, "mc-full", "us-east-1", 100, 100).await?;
    register_healthy_mc(&pool, "mc-available", "us-east-1", 50, 100).await?;

    let candidates =
        MeetingAssignmentsRepository::get_candidate_mcs(&pool, "us-east-1", STALENESS).await?;

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

    let us = MeetingAssignmentsRepository::get_candidate_mcs(&pool, "us-east-1", STALENESS).await?;
    let eu = MeetingAssignmentsRepository::get_candidate_mcs(&pool, "eu-west-1", STALENESS).await?;

    assert_eq!(
        us.iter()
            .map(|c| c.controller_id.as_str())
            .collect::<Vec<_>>(),
        vec!["mc-us-1"]
    );
    assert_eq!(
        eu.iter()
            .map(|c| c.controller_id.as_str())
            .collect::<Vec<_>>(),
        vec!["mc-eu-1"]
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

    let (assignment, written, id) =
        assign(&pool, "meeting-123", "us-east-1", &candidate, "gc-test-001").await?;

    assert!(written);
    assert_eq!(assignment.mc_id, "mc-1");
    assert_eq!(assignment.grpc_endpoint, "https://mc-1.example.com:50051");
    assert_eq!(
        assignment.webtransport_endpoint,
        Some("https://mc-1.example.com:443".to_string())
    );
    assert_eq!(
        row_fence(&pool, "meeting-123", "us-east-1").await?,
        ("mc-1".to_string(), id.as_uuid()),
        "the row carries the id the MC accepted"
    );
    Ok(())
}

/// A lost race returns the winner's row and leaves the winner's id on it.
#[sqlx::test(migrations = "../../migrations")]
async fn test_atomic_assign_lost_race_keeps_the_winners_row_and_id(
    pool: PgPool,
) -> Result<(), anyhow::Error> {
    register_healthy_mc(&pool, "mc-1", "us-east-1", 10, 100).await?;
    register_healthy_mc(&pool, "mc-2", "us-east-1", 5, 100).await?;

    let (first, _, first_id) = assign(
        &pool,
        "meeting-123",
        "us-east-1",
        &candidate("mc-1"),
        "gc-1",
    )
    .await?;
    assert_eq!(first.mc_id, "mc-1");

    let (second, written, _) = assign(
        &pool,
        "meeting-123",
        "us-east-1",
        &candidate("mc-2"),
        "gc-2",
    )
    .await?;

    assert!(!written, "the loser writes nothing");
    assert_eq!(second.mc_id, "mc-1", "Should return existing assignment");
    assert_eq!(
        row_fence(&pool, "meeting-123", "us-east-1").await?,
        ("mc-1".to_string(), first_id.as_uuid()),
        "the winner's incarnation id is untouched"
    );
    Ok(())
}

/// End is keyed on (meeting, region, controller, id): ending the US row
/// leaves the same meeting's EU row live.
#[sqlx::test(migrations = "../../migrations")]
async fn test_end_assignment_by_region(pool: PgPool) -> Result<(), anyhow::Error> {
    register_healthy_mc(&pool, "mc-1", "us-east-1", 10, 100).await?;
    register_healthy_mc(&pool, "mc-2", "eu-west-1", 10, 100).await?;

    let (_, _, us_id) = assign(
        &pool,
        "meeting-123",
        "us-east-1",
        &candidate("mc-1"),
        "gc-1",
    )
    .await?;
    assign(
        &pool,
        "meeting-123",
        "eu-west-1",
        &candidate("mc-2"),
        "gc-1",
    )
    .await?;

    let outcome = MeetingAssignmentsRepository::end_assignment(
        &pool,
        "meeting-123",
        "us-east-1",
        "mc-1",
        us_id,
    )
    .await?;
    assert_eq!(outcome, EndAssignmentOutcome::Ended);

    assert!(MeetingAssignmentsRepository::get_live_assignment(
        &pool,
        "meeting-123",
        "us-east-1",
        STALENESS
    )
    .await?
    .is_none());
    assert!(MeetingAssignmentsRepository::get_live_assignment(
        &pool,
        "meeting-123",
        "eu-west-1",
        STALENESS
    )
    .await?
    .is_some());
    Ok(())
}

/// THE FENCE. A notify ends only the row naming its controller AND id.
/// Every other case is answered without ending anything, and is classified
/// in the same statement: `fenced` when a live row of another incarnation
/// exists, `already_ended` when none does.
#[sqlx::test(migrations = "../../migrations")]
async fn test_end_assignment_is_fenced_by_controller_and_assignment_id(
    pool: PgPool,
) -> Result<(), anyhow::Error> {
    register_healthy_mc(&pool, "mc-1", "us-east-1", 10, 100).await?;
    register_healthy_mc(&pool, "mc-2", "us-east-1", 10, 100).await?;
    let (_, _, id) = assign(&pool, "m-fence", "us-east-1", &candidate("mc-1"), "gc-1").await?;

    // Wrong id (another incarnation's, or an orphan's): nothing ended.
    let other = AssignmentId::new_v4();
    assert_eq!(
        MeetingAssignmentsRepository::end_assignment(&pool, "m-fence", "us-east-1", "mc-1", other)
            .await?,
        EndAssignmentOutcome::Fenced
    );
    // Right id, wrong controller (a misrouted / foreign MC): nothing ended.
    assert_eq!(
        MeetingAssignmentsRepository::end_assignment(&pool, "m-fence", "us-east-1", "mc-2", id)
            .await?,
        EndAssignmentOutcome::Fenced
    );
    assert!(
        ended_at_is_null(&pool, "m-fence", "us-east-1").await?,
        "row still live"
    );

    // The matching notify ends it; a re-send is harmless.
    assert_eq!(
        MeetingAssignmentsRepository::end_assignment(&pool, "m-fence", "us-east-1", "mc-1", id)
            .await?,
        EndAssignmentOutcome::Ended
    );
    assert!(!ended_at_is_null(&pool, "m-fence", "us-east-1").await?);
    assert_eq!(
        MeetingAssignmentsRepository::end_assignment(&pool, "m-fence", "us-east-1", "mc-1", id)
            .await?,
        EndAssignmentOutcome::AlreadyEnded
    );
    Ok(())
}

/// A notify for a PREVIOUS incarnation that lands after a revive — even on
/// the SAME MC — cannot end the new incarnation's live row.
#[sqlx::test(migrations = "../../migrations")]
async fn test_late_notify_of_a_previous_incarnation_cannot_end_the_revived_row(
    pool: PgPool,
) -> Result<(), anyhow::Error> {
    register_healthy_mc(&pool, "mc-1", "us-east-1", 10, 100).await?;
    let (_, _, old_id) = assign(&pool, "m-late", "us-east-1", &candidate("mc-1"), "gc-1").await?;
    MeetingAssignmentsRepository::end_assignment(&pool, "m-late", "us-east-1", "mc-1", old_id)
        .await?;
    let (_, written, new_id) =
        assign(&pool, "m-late", "us-east-1", &candidate("mc-1"), "gc-2").await?;
    assert!(written, "the ended row is revived");
    assert_ne!(old_id, new_id);

    assert_eq!(
        MeetingAssignmentsRepository::end_assignment(&pool, "m-late", "us-east-1", "mc-1", old_id)
            .await?,
        EndAssignmentOutcome::Fenced,
        "the old incarnation's notify is fenced"
    );
    assert!(
        ended_at_is_null(&pool, "m-late", "us-east-1").await?,
        "the revived incarnation stays live"
    );
    Ok(())
}

/// TODO.md residual (2), closed: MC-A goes stale, a join fails the row over
/// to MC-B, then MC-A (still alive) finishes its teardown and notifies. Its
/// notify must not end MC-B's live row (a split meeting).
#[sqlx::test(migrations = "../../migrations")]
async fn test_failed_over_mcs_notify_cannot_end_the_new_mcs_row(
    pool: PgPool,
) -> Result<(), anyhow::Error> {
    register_healthy_mc(&pool, "mc-a", "us-east-1", 10, 100).await?;
    register_healthy_mc(&pool, "mc-b", "us-east-1", 10, 100).await?;
    let (_, _, a_id) = assign(&pool, "m-split", "us-east-1", &candidate("mc-a"), "gc-1").await?;
    make_stale(&pool, "mc-a").await?;
    let (failed_over, written, b_id) =
        assign(&pool, "m-split", "us-east-1", &candidate("mc-b"), "gc-2").await?;
    assert!(written);
    assert_eq!(failed_over.mc_id, "mc-b");
    assert_eq!(
        row_fence(&pool, "m-split", "us-east-1").await?,
        ("mc-b".to_string(), b_id.as_uuid()),
        "re-point writes the new MC's id"
    );

    assert_eq!(
        MeetingAssignmentsRepository::end_assignment(&pool, "m-split", "us-east-1", "mc-a", a_id)
            .await?,
        EndAssignmentOutcome::Fenced
    );
    assert!(ended_at_is_null(&pool, "m-split", "us-east-1").await?);
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_cleanup_old_assignments(pool: PgPool) -> Result<(), anyhow::Error> {
    register_healthy_mc(&pool, "mc-1", "us-east-1", 10, 100).await?;

    let (_, _, id) = assign(
        &pool,
        "meeting-old",
        "us-east-1",
        &candidate("mc-1"),
        "gc-1",
    )
    .await?;
    MeetingAssignmentsRepository::end_assignment(&pool, "meeting-old", "us-east-1", "mc-1", id)
        .await?;

    sqlx::query(
        "UPDATE meeting_assignments SET ended_at = NOW() - INTERVAL '10 days' WHERE meeting_id = $1"
    )
    .bind("meeting-old")
    .execute(&pool)
    .await?;

    let cleaned = MeetingAssignmentsRepository::cleanup_old_assignments(&pool, 7, None).await?;
    assert_eq!(cleaned, 1);
    Ok(())
}

// ============================================================================
// MC liveness, not status (2026-11-15): a row whose MC still heartbeats is
// never re-pointed or ended, whatever the MC's `health_status`.
// ============================================================================

/// Join reuse and the failover arm both key on liveness. For each non-healthy
/// status with a FRESH heartbeat the row is reused and NOT re-pointed (that
/// would split a live meeting); a STALE heartbeat is re-pointed even when the
/// last status read `healthy`.
#[sqlx::test(migrations = "../../migrations")]
async fn test_reuse_and_failover_key_on_liveness_not_status(
    pool: PgPool,
) -> Result<(), anyhow::Error> {
    register_healthy_mc(&pool, "mc-new", "us-east-1", 0, 100).await?;
    for status in ["healthy", "degraded", "draining", "pending", "unhealthy"] {
        for stale in [false, true] {
            let mc = format!("mc-{status}-{}", if stale { "stale" } else { "fresh" });
            let meeting = format!("m-{mc}");
            register_healthy_mc(&pool, &mc, "us-east-1", 10, 100).await?;
            let (_, _, first_id) =
                assign(&pool, &meeting, "us-east-1", &candidate(&mc), "gc-1").await?;
            set_status(&pool, &mc, status).await?;
            if stale {
                make_stale(&pool, &mc).await?;
            }

            let reused = MeetingAssignmentsRepository::get_live_assignment(
                &pool,
                &meeting,
                "us-east-1",
                STALENESS,
            )
            .await?;
            assert_eq!(
                reused.map(|a| a.mc_id),
                (!stale).then(|| mc.clone()),
                "{mc}: join reuse must follow liveness"
            );

            let (after, written, _) =
                assign(&pool, &meeting, "us-east-1", &candidate("mc-new"), "gc-2").await?;
            assert_eq!(written, stale, "{mc}: re-point iff stale");
            assert_eq!(
                after.mc_id,
                if stale {
                    "mc-new".to_string()
                } else {
                    mc.clone()
                },
                "{mc}: a live MC keeps its meeting"
            );
            if !stale {
                assert_eq!(
                    row_fence(&pool, &meeting, "us-east-1").await?,
                    (mc.clone(), first_id.as_uuid()),
                    "{mc}: the live row is untouched"
                );
            }
        }
    }
    Ok(())
}

// ============================================================================
// The accept->commit budget
// ============================================================================

/// A spent budget writes no row (the elapsed check precedes any write).
#[sqlx::test(migrations = "../../migrations")]
async fn test_atomic_assign_spent_budget_writes_no_row(pool: PgPool) -> Result<(), anyhow::Error> {
    register_healthy_mc(&pool, "mc-1", "us-east-1", 10, 100).await?;
    let outcome = MeetingAssignmentsRepository::atomic_assign(
        &pool,
        "m-budget",
        "us-east-1",
        &candidate("mc-1"),
        "gc-1",
        AssignmentId::new_v4(),
        STALENESS,
        Instant::now(),
    )
    .await?;
    assert!(matches!(outcome, AtomicAssignOutcome::BudgetExceeded));
    let (rows,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM meeting_assignments WHERE meeting_id = $1")
            .bind("m-budget")
            .fetch_one(&pool)
            .await?;
    assert_eq!(rows, 0, "no row is written past the budget");
    Ok(())
}

/// The statement_timeout path (SQLSTATE 57014): the upsert blocks on a row
/// lock held by another transaction past the remaining budget, PostgreSQL
/// cancels it, and nothing is written. Positive control: once the lock is
/// released, the same write succeeds.
#[sqlx::test(migrations = "../../migrations")]
async fn test_atomic_assign_statement_timeout_writes_no_row(
    pool: PgPool,
) -> Result<(), anyhow::Error> {
    register_healthy_mc(&pool, "mc-1", "us-east-1", 10, 100).await?;
    register_healthy_mc(&pool, "mc-2", "us-east-1", 10, 100).await?;
    let (_, _, old_id) = assign(&pool, "m-lock", "us-east-1", &candidate("mc-1"), "gc-1").await?;
    MeetingAssignmentsRepository::end_assignment(&pool, "m-lock", "us-east-1", "mc-1", old_id)
        .await?;

    // Hold the (ended) row's lock so the revive's upsert must wait.
    let mut blocker = pool.begin().await?;
    sqlx::query("SELECT 1 FROM meeting_assignments WHERE meeting_id = $1 FOR UPDATE")
        .bind("m-lock")
        .execute(&mut *blocker)
        .await?;

    let outcome = MeetingAssignmentsRepository::atomic_assign(
        &pool,
        "m-lock",
        "us-east-1",
        &candidate("mc-2"),
        "gc-2",
        AssignmentId::new_v4(),
        STALENESS,
        Instant::now() + Duration::from_millis(300),
    )
    .await?;
    assert!(
        matches!(outcome, AtomicAssignOutcome::BudgetExceeded),
        "a write blocked past the budget is cancelled, got {outcome:?}"
    );
    blocker.rollback().await?;
    assert!(
        !ended_at_is_null(&pool, "m-lock", "us-east-1").await?,
        "the cancelled revive wrote nothing"
    );

    // Positive control: unblocked, the same revive commits.
    let (revived, written, _) =
        assign(&pool, "m-lock", "us-east-1", &candidate("mc-2"), "gc-2").await?;
    assert!(written);
    assert_eq!(revived.mc_id, "mc-2");
    Ok(())
}

// ============================================================================
// Reviving an ENDED assignment (story 2 task 12)
//
// Once MC reports a meeting ended (`notify_meeting_ended`), a rejoin of that
// meeting id must be assignable again. The row is soft-deleted and keeps the
// (meeting_id, region) key, so `atomic_assign` must REVIVE it in place. These
// tests read `ended_at` straight from the table: gc-service's queries are
// runtime strings, so nothing checks their `ended_at` semantics at compile
// time and these are the only guard.
// ============================================================================

fn candidate(id: &str) -> McCandidate {
    McCandidate {
        controller_id: id.to_string(),
        grpc_endpoint: format!("https://{id}.example.com:50051"),
        webtransport_endpoint: None,
        load_ratio: 0.1,
    }
}

async fn ended_at_is_null(
    pool: &PgPool,
    meeting_id: &str,
    region: &str,
) -> Result<bool, anyhow::Error> {
    let (null,): (bool,) = sqlx::query_as(
        "SELECT ended_at IS NULL FROM meeting_assignments WHERE meeting_id = $1 AND region = $2",
    )
    .bind(meeting_id)
    .bind(region)
    .fetch_one(pool)
    .await?;
    Ok(null)
}

/// An ended row with a live MC is revived: the new MC and its new id are
/// written, the row is live again, and a lookup finds it.
#[sqlx::test(migrations = "../../migrations")]
async fn test_atomic_assign_revives_an_ended_assignment(pool: PgPool) -> Result<(), anyhow::Error> {
    register_healthy_mc(&pool, "mc-1", "us-east-1", 10, 100).await?;
    register_healthy_mc(&pool, "mc-2", "us-east-1", 10, 100).await?;
    let (_, _, id) = assign(&pool, "m-revive", "us-east-1", &candidate("mc-1"), "gc-1").await?;
    MeetingAssignmentsRepository::end_assignment(&pool, "m-revive", "us-east-1", "mc-1", id)
        .await?;
    assert!(
        !ended_at_is_null(&pool, "m-revive", "us-east-1").await?,
        "precondition: ended"
    );

    let (revived, written, new_id) =
        assign(&pool, "m-revive", "us-east-1", &candidate("mc-2"), "gc-2").await?;
    assert!(written);
    assert_eq!(
        revived.mc_id, "mc-2",
        "the new incarnation's MC wins the revive"
    );
    assert_eq!(
        row_fence(&pool, "m-revive", "us-east-1").await?,
        ("mc-2".to_string(), new_id.as_uuid()),
        "revive writes the new incarnation's id"
    );
    assert!(
        ended_at_is_null(&pool, "m-revive", "us-east-1").await?,
        "the row is live again"
    );
    assert!(
        MeetingAssignmentsRepository::get_live_assignment(
            &pool,
            "m-revive",
            "us-east-1",
            STALENESS
        )
        .await?
        .is_some(),
        "and a lookup finds it"
    );
    Ok(())
}

/// The FAILOVER half: an ended row pointing at a DEAD (stale) MC. The
/// discriminating assertion is that the row is also made LIVE (`ended_at`
/// cleared), which a predicate-only fix would miss.
#[sqlx::test(migrations = "../../migrations")]
async fn test_atomic_assign_failover_of_an_ended_row_clears_ended_at(
    pool: PgPool,
) -> Result<(), anyhow::Error> {
    register_healthy_mc(&pool, "mc-1", "us-east-1", 10, 100).await?;
    register_healthy_mc(&pool, "mc-2", "us-east-1", 10, 100).await?;
    let (_, _, id) = assign(&pool, "m-fail", "us-east-1", &candidate("mc-1"), "gc-1").await?;
    MeetingAssignmentsRepository::end_assignment(&pool, "m-fail", "us-east-1", "mc-1", id).await?;
    make_stale(&pool, "mc-1").await?;

    let (revived, _, _) = assign(&pool, "m-fail", "us-east-1", &candidate("mc-2"), "gc-2").await?;
    assert_eq!(revived.mc_id, "mc-2");
    assert!(
        ended_at_is_null(&pool, "m-fail", "us-east-1").await?,
        "a failover of an ended row must also clear ended_at, or lookups keep returning nothing"
    );
    Ok(())
}

/// No steal after a revive: once revived onto a live MC, another GC's
/// assign for the same id returns the revived winner and does NOT overwrite
/// it. Guards the `OR`'s parenthesisation.
#[sqlx::test(migrations = "../../migrations")]
async fn test_a_revived_assignment_is_not_stolen(pool: PgPool) -> Result<(), anyhow::Error> {
    register_healthy_mc(&pool, "mc-1", "us-east-1", 10, 100).await?;
    register_healthy_mc(&pool, "mc-2", "us-east-1", 10, 100).await?;
    register_healthy_mc(&pool, "mc-3", "us-east-1", 10, 100).await?;
    let (_, _, id) = assign(&pool, "m-steal", "us-east-1", &candidate("mc-1"), "gc-1").await?;
    MeetingAssignmentsRepository::end_assignment(&pool, "m-steal", "us-east-1", "mc-1", id).await?;
    assign(&pool, "m-steal", "us-east-1", &candidate("mc-2"), "gc-2").await?;

    let (loser, written, _) =
        assign(&pool, "m-steal", "us-east-1", &candidate("mc-3"), "gc-3").await?;
    assert!(!written);
    assert_eq!(loser.mc_id, "mc-2", "the live revived assignment is kept");
    Ok(())
}

/// Cleanup is keyed on (meeting_id, region): an old ENDED row in one region
/// is deleted, and a LIVE row of the same meeting id in another region is not.
#[sqlx::test(migrations = "../../migrations")]
async fn test_cleanup_never_deletes_a_live_row_in_another_region(
    pool: PgPool,
) -> Result<(), anyhow::Error> {
    register_healthy_mc(&pool, "mc-a", "us-east-1", 10, 100).await?;
    register_healthy_mc(&pool, "mc-b", "eu-west-1", 10, 100).await?;
    let (_, _, id) = assign(&pool, "m-2r", "us-east-1", &candidate("mc-a"), "gc-1").await?;
    assign(&pool, "m-2r", "eu-west-1", &candidate("mc-b"), "gc-1").await?;
    MeetingAssignmentsRepository::end_assignment(&pool, "m-2r", "us-east-1", "mc-a", id).await?;
    sqlx::query(
        "UPDATE meeting_assignments SET ended_at = NOW() - INTERVAL '10 days' \
         WHERE meeting_id = $1 AND region = 'us-east-1'",
    )
    .bind("m-2r")
    .execute(&pool)
    .await?;

    let cleaned = MeetingAssignmentsRepository::cleanup_old_assignments(&pool, 7, None).await?;
    assert_eq!(cleaned, 1, "only the old ended row");
    assert!(
        ended_at_is_null(&pool, "m-2r", "eu-west-1").await?,
        "the live row in the other region survives"
    );
    Ok(())
}

/// Stale-ending is keyed on (meeting_id, region) and re-checks the MC on the
/// row it ends: an old row whose MC is DEAD in one region is ended, and a
/// live row of the same meeting id in another region is not.
#[sqlx::test(migrations = "../../migrations")]
async fn test_end_stale_never_ends_a_live_row_in_another_region(
    pool: PgPool,
) -> Result<(), anyhow::Error> {
    register_healthy_mc(&pool, "mc-a", "us-east-1", 10, 100).await?;
    register_healthy_mc(&pool, "mc-b", "eu-west-1", 10, 100).await?;
    assign(&pool, "m-stale", "us-east-1", &candidate("mc-a"), "gc-1").await?;
    assign(&pool, "m-stale", "eu-west-1", &candidate("mc-b"), "gc-1").await?;
    sqlx::query(
        "UPDATE meeting_assignments SET assigned_at = NOW() - INTERVAL '48 hours' WHERE meeting_id = $1",
    )
    .bind("m-stale")
    .execute(&pool)
    .await?;
    make_stale(&pool, "mc-a").await?;

    let ended =
        MeetingAssignmentsRepository::end_stale_assignments(&pool, 24, STALENESS, None).await?;
    assert_eq!(ended, 1, "only the old row whose MC is dead");
    assert!(!ended_at_is_null(&pool, "m-stale", "us-east-1").await?);
    assert!(
        ended_at_is_null(&pool, "m-stale", "eu-west-1").await?,
        "the live row in the other region is untouched"
    );
    Ok(())
}

/// Service level: a meeting that ended is re-assigned as a NEW incarnation —
/// a fresh MH selection and one more `assign_meeting` to MC with a NEW id —
/// and the call after that reuses it (one `assign_meeting` per incarnation).
#[sqlx::test(migrations = "../../migrations")]
async fn test_service_reassigns_an_ended_meeting_as_a_new_incarnation(
    pool: PgPool,
) -> Result<(), anyhow::Error> {
    register_healthy_mc(&pool, "mc-1", "us-east-1", 10, 100).await?;
    register_healthy_mhs_for_region(&pool, "us-east-1").await?;
    let mc_client = Arc::new(MockMcClient::accepting());

    let first = McAssignmentService::assign_meeting_with_mh(
        &pool,
        mc_client.clone(),
        "m-incarnation",
        "us-east-1",
        "gc-1",
        &gc_test_utils::reference_mh_registry(),
        &mc_cfg(),
    )
    .await?;
    assert!(first.mh_selection.is_some());
    let first_id = mc_client.received_assignment_ids()[0];
    assert_eq!(
        McAssignmentService::end_assignment(&pool, "m-incarnation", "us-east-1", "mc-1", first_id)
            .await?,
        EndAssignmentOutcome::Ended
    );

    let second = McAssignmentService::assign_meeting_with_mh(
        &pool,
        mc_client.clone(),
        "m-incarnation",
        "us-east-1",
        "gc-1",
        &gc_test_utils::reference_mh_registry(),
        &mc_cfg(),
    )
    .await?;
    assert!(
        second.mh_selection.is_some(),
        "an ended meeting gets a NEW selection"
    );
    let ids = mc_client.received_assignment_ids();
    assert_eq!(ids.len(), 2, "and a second assign_meeting to MC");
    assert_ne!(ids[0], ids[1], "as a new incarnation id");
    assert_eq!(
        row_fence(&pool, "m-incarnation", "us-east-1").await?,
        ("mc-1".to_string(), ids[1].as_uuid())
    );

    let third = McAssignmentService::assign_meeting_with_mh(
        &pool,
        mc_client.clone(),
        "m-incarnation",
        "us-east-1",
        "gc-1",
        &gc_test_utils::reference_mh_registry(),
        &mc_cfg(),
    )
    .await?;
    assert!(
        third.mh_selection.is_none(),
        "within an incarnation, the assignment is reused"
    );
    assert_eq!(mc_client.call_count(), 2);
    Ok(())
}

// ============================================================================
// Service Tests (using assign_meeting_with_mh with MockMcClient)
// ============================================================================

#[sqlx::test(migrations = "../../migrations")]
async fn test_service_assign_meeting_with_mh_no_healthy_mcs(
    pool: PgPool,
) -> Result<(), anyhow::Error> {
    register_healthy_mhs_for_region(&pool, "us-east-1").await?;

    let mc_client = Arc::new(MockMcClient::accepting());
    let result = McAssignmentService::assign_meeting_with_mh(
        &pool,
        mc_client,
        "meeting-1",
        "us-east-1",
        "gc-1",
        &gc_test_utils::reference_mh_registry(),
        &mc_cfg(),
    )
    .await;

    assert!(result.is_err());
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
        &gc_test_utils::reference_mh_registry(),
        &mc_cfg(),
    )
    .await?;

    assert_eq!(assignment.mc_assignment.mc_id, "mc-1");
    assert!(!assignment.mc_assignment.grpc_endpoint.is_empty());
    let selection = assignment
        .mh_selection
        .as_ref()
        .expect("a new assignment carries the selection sent to MC");
    assert!(!selection.handlers.is_empty());
    assert!(!selection.handlers[0].mh_id.is_empty());
    Ok(())
}

/// Each MC attempt carries its OWN id, and the row carries the id of the
/// attempt that MC accepted (a rejected attempt's id is never written).
#[sqlx::test(migrations = "../../migrations")]
async fn test_service_mints_a_fresh_assignment_id_per_mc_attempt(
    pool: PgPool,
) -> Result<(), anyhow::Error> {
    register_healthy_mc(&pool, "mc-1", "us-east-1", 10, 100).await?;
    register_healthy_mc(&pool, "mc-2", "us-east-1", 10, 100).await?;
    register_healthy_mhs_for_region(&pool, "us-east-1").await?;
    let mc_client = Arc::new(MockMcClient::with_responses(vec![
        McAssignmentResult::Rejected(McRejectionReason::AtCapacity),
        McAssignmentResult::Accepted,
    ]));

    let assignment = McAssignmentService::assign_meeting_with_mh(
        &pool,
        mc_client.clone(),
        "m-attempts",
        "us-east-1",
        "gc-1",
        &gc_test_utils::reference_mh_registry(),
        &mc_cfg(),
    )
    .await?;

    let ids = mc_client.received_assignment_ids();
    assert_eq!(ids.len(), 2);
    assert_ne!(ids[0], ids[1], "a fresh id per MC attempt");
    assert_eq!(
        row_fence(&pool, "m-attempts", "us-east-1").await?,
        (assignment.mc_assignment.mc_id, ids[1].as_uuid()),
        "the row carries the accepted attempt's id"
    );
    Ok(())
}

/// An MC that answers after the accept->commit budget: no row is written
/// for it, and GC moves on (here every MC is slow, so the join fails
/// retryably and nothing is left behind).
#[sqlx::test(migrations = "../../migrations")]
async fn test_service_budget_exceeded_writes_no_row_and_tries_next_mc(
    pool: PgPool,
) -> Result<(), anyhow::Error> {
    register_healthy_mc(&pool, "mc-1", "us-east-1", 10, 100).await?;
    register_healthy_mc(&pool, "mc-2", "us-east-1", 10, 100).await?;
    register_healthy_mhs_for_region(&pool, "us-east-1").await?;
    let mc_client = Arc::new(MockMcClient::accepting().with_delay(Duration::from_millis(150)));
    let cfg = McAssignmentConfig::new(STALENESS, Duration::from_millis(50))?;

    let result = McAssignmentService::assign_meeting_with_mh(
        &pool,
        mc_client.clone(),
        "m-slow",
        "us-east-1",
        "gc-1",
        &gc_test_utils::reference_mh_registry(),
        &cfg,
    )
    .await;

    assert!(result.is_err(), "every MC answered past the budget");
    assert_eq!(mc_client.call_count(), 2, "both MCs were tried");
    let (rows,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM meeting_assignments WHERE meeting_id = $1")
            .bind("m-slow")
            .fetch_one(&pool)
            .await?;
    assert_eq!(rows, 0, "no row past the budget");
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_service_assign_meeting_with_mh_reuses_live(
    pool: PgPool,
) -> Result<(), anyhow::Error> {
    register_healthy_mc(&pool, "mc-1", "us-east-1", 10, 100).await?;
    register_healthy_mc(&pool, "mc-2", "us-east-1", 5, 100).await?;
    register_healthy_mhs_for_region(&pool, "us-east-1").await?;

    let mc_client = Arc::new(MockMcClient::accepting());

    let first = McAssignmentService::assign_meeting_with_mh(
        &pool,
        mc_client.clone(),
        "meeting-1",
        "us-east-1",
        "gc-1",
        &gc_test_utils::reference_mh_registry(),
        &mc_cfg(),
    )
    .await?;
    let first_mc = first.mc_assignment.mc_id.clone();

    let second = McAssignmentService::assign_meeting_with_mh(
        &pool,
        mc_client,
        "meeting-1",
        "us-east-1",
        "gc-2",
        &gc_test_utils::reference_mh_registry(),
        &mc_cfg(),
    )
    .await?;

    assert_eq!(
        second.mc_assignment.mc_id, first_mc,
        "Should reuse existing live assignment"
    );
    assert!(first.mh_selection.is_some());
    assert!(
        second.mh_selection.is_none(),
        "reuse path performs no MH selection (R-6)"
    );
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_service_end_assignment(pool: PgPool) -> Result<(), anyhow::Error> {
    register_healthy_mc(&pool, "mc-1", "us-east-1", 10, 100).await?;
    register_healthy_mhs_for_region(&pool, "us-east-1").await?;

    let mc_client = Arc::new(MockMcClient::accepting());
    McAssignmentService::assign_meeting_with_mh(
        &pool,
        mc_client.clone(),
        "meeting-1",
        "us-east-1",
        "gc-1",
        &gc_test_utils::reference_mh_registry(),
        &mc_cfg(),
    )
    .await?;
    let id = mc_client.received_assignment_ids()[0];

    let outcome =
        McAssignmentService::end_assignment(&pool, "meeting-1", "us-east-1", "mc-1", id).await?;
    assert_eq!(outcome, EndAssignmentOutcome::Ended);

    let assignment =
        McAssignmentService::get_assignment(&pool, "meeting-1", "us-east-1", STALENESS).await?;
    assert!(assignment.is_none());
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_service_get_assignment(pool: PgPool) -> Result<(), anyhow::Error> {
    register_healthy_mc(&pool, "mc-1", "us-east-1", 10, 100).await?;
    register_healthy_mhs_for_region(&pool, "us-east-1").await?;

    let none =
        McAssignmentService::get_assignment(&pool, "meeting-1", "us-east-1", STALENESS).await?;
    assert!(none.is_none());

    let mc_client = Arc::new(MockMcClient::accepting());
    McAssignmentService::assign_meeting_with_mh(
        &pool,
        mc_client,
        "meeting-1",
        "us-east-1",
        "gc-1",
        &gc_test_utils::reference_mh_registry(),
        &mc_cfg(),
    )
    .await?;

    let some =
        McAssignmentService::get_assignment(&pool, "meeting-1", "us-east-1", STALENESS).await?;
    assert_eq!(some.as_ref().map(|a| a.mc_id.as_str()), Some("mc-1"));
    Ok(())
}

// ============================================================================
// Race Condition and Liveness Transition Tests
// ============================================================================

/// Concurrent assignment attempts: one row, every caller gets the winner.
#[sqlx::test(migrations = "../../migrations")]
async fn test_concurrent_assignment_race_condition(pool: PgPool) -> Result<(), anyhow::Error> {
    use std::collections::HashSet;
    use tokio::sync::Barrier;

    register_healthy_mc(&pool, "mc-1", "us-east-1", 10, 100).await?;
    register_healthy_mc(&pool, "mc-2", "us-east-1", 20, 100).await?;
    register_healthy_mc(&pool, "mc-3", "us-east-1", 30, 100).await?;
    register_healthy_mhs_for_region(&pool, "us-east-1").await?;

    let pool = Arc::new(pool);
    let num_concurrent = 10;
    let barrier = Arc::new(Barrier::new(num_concurrent));

    let handles: Vec<_> = (0..num_concurrent)
        .map(|i| {
            let pool = Arc::clone(&pool);
            let barrier = Arc::clone(&barrier);
            let mc_client = Arc::new(MockMcClient::accepting());
            tokio::spawn(async move {
                barrier.wait().await;
                McAssignmentService::assign_meeting_with_mh(
                    &pool,
                    mc_client,
                    "meeting-concurrent-test",
                    "us-east-1",
                    &format!("gc-{}", i),
                    &gc_test_utils::reference_mh_registry(),
                    &mc_cfg(),
                )
                .await
            })
        })
        .collect();

    let results: Vec<_> = futures::future::join_all(handles)
        .await
        .into_iter()
        .map(|r| r.expect("task should not panic"))
        .collect();

    let successful: Vec<_> = results.iter().filter(|r| r.is_ok()).collect();
    assert_eq!(
        successful.len(),
        num_concurrent,
        "All concurrent assignments should succeed"
    );

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

/// Per ADR-0010 §3, when an assigned MC stops heartbeating, the next join
/// re-points the row (in place) to a live MC.
#[sqlx::test(migrations = "../../migrations")]
async fn test_dead_mc_fails_over_on_next_join(pool: PgPool) -> Result<(), anyhow::Error> {
    register_healthy_mc(&pool, "mc-healthy-1", "us-east-1", 10, 100).await?;
    register_healthy_mc(&pool, "mc-healthy-2", "us-east-1", 20, 100).await?;
    register_healthy_mhs_for_region(&pool, "us-east-1").await?;

    let mc_client = Arc::new(MockMcClient::accepting());
    let first = McAssignmentService::assign_meeting_with_mh(
        &pool,
        mc_client.clone(),
        "meeting-health-test",
        "us-east-1",
        "gc-1",
        &gc_test_utils::reference_mh_registry(),
        &mc_cfg(),
    )
    .await?;
    let first_mc = first.mc_assignment.mc_id.clone();
    make_stale(&pool, &first_mc).await?;

    let second = McAssignmentService::assign_meeting_with_mh(
        &pool,
        mc_client,
        "meeting-health-test",
        "us-east-1",
        "gc-2",
        &gc_test_utils::reference_mh_registry(),
        &mc_cfg(),
    )
    .await?;
    assert_ne!(
        second.mc_assignment.mc_id, first_mc,
        "Should assign to a different MC after the first stopped heartbeating"
    );

    let total_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM meeting_assignments WHERE meeting_id = $1")
            .bind("meeting-health-test")
            .fetch_one(&pool)
            .await?;
    assert_eq!(total_count.0, 1, "re-pointed in place: one row total");
    Ok(())
}

/// A DRAINING MC that still heartbeats keeps its meeting: a join reuses it
/// (no MC call, no new selection) instead of splitting the meeting.
#[sqlx::test(migrations = "../../migrations")]
async fn test_draining_live_mc_keeps_its_meeting_on_join(
    pool: PgPool,
) -> Result<(), anyhow::Error> {
    register_healthy_mc(&pool, "mc-1", "us-east-1", 10, 100).await?;
    register_healthy_mhs_for_region(&pool, "us-east-1").await?;
    let mc_client = Arc::new(MockMcClient::accepting());

    McAssignmentService::assign_meeting_with_mh(
        &pool,
        mc_client.clone(),
        "m-drain",
        "us-east-1",
        "gc-1",
        &gc_test_utils::reference_mh_registry(),
        &mc_cfg(),
    )
    .await?;
    register_healthy_mc(&pool, "mc-2", "us-east-1", 0, 100).await?;
    set_status(&pool, "mc-1", "draining").await?;

    let joined = McAssignmentService::assign_meeting_with_mh(
        &pool,
        mc_client.clone(),
        "m-drain",
        "us-east-1",
        "gc-2",
        &gc_test_utils::reference_mh_registry(),
        &mc_cfg(),
    )
    .await?;
    assert_eq!(
        joined.mc_assignment.mc_id, "mc-1",
        "the draining MC keeps it"
    );
    assert!(joined.mh_selection.is_none(), "reuse path");
    assert_eq!(mc_client.call_count(), 1, "no second MC was asked");
    Ok(())
}

// ============================================================================
// Outcome exactness, schema enforcement, and the liveness THRESHOLD boundary
// ============================================================================

/// `already_ended` is exact: only a notify whose OWN incarnation's row exists
/// (and was already ended) is `already_ended`. No row at all (an orphan that
/// never committed) and another incarnation's ENDED row are `fenced`.
#[sqlx::test(migrations = "../../migrations")]
async fn test_end_assignment_already_ended_is_exact(pool: PgPool) -> Result<(), anyhow::Error> {
    register_healthy_mc(&pool, "mc-1", "us-east-1", 10, 100).await?;

    // No row at all for (meeting, region).
    assert_eq!(
        MeetingAssignmentsRepository::end_assignment(
            &pool,
            "m-none",
            "us-east-1",
            "mc-1",
            AssignmentId::new_v4()
        )
        .await?,
        EndAssignmentOutcome::Fenced,
        "an orphan's notify (no row) is fenced"
    );

    let (_, _, id) = assign(&pool, "m-exact", "us-east-1", &candidate("mc-1"), "gc-1").await?;
    MeetingAssignmentsRepository::end_assignment(&pool, "m-exact", "us-east-1", "mc-1", id).await?;
    // Another incarnation's id against an ENDED row.
    assert_eq!(
        MeetingAssignmentsRepository::end_assignment(
            &pool,
            "m-exact",
            "us-east-1",
            "mc-1",
            AssignmentId::new_v4()
        )
        .await?,
        EndAssignmentOutcome::Fenced,
        "another incarnation against an ended row is fenced, not already_ended"
    );
    // Our own, already ended.
    assert_eq!(
        MeetingAssignmentsRepository::end_assignment(&pool, "m-exact", "us-east-1", "mc-1", id)
            .await?,
        EndAssignmentOutcome::AlreadyEnded
    );
    Ok(())
}

/// The schema enforces the fence: a writer that omits `assignment_id` fails
/// loudly (NOT NULL, no default — SQLSTATE 23502) instead of minting an id no
/// MC knows. Positive control: the same insert WITH an id succeeds.
#[sqlx::test(migrations = "../../migrations")]
async fn test_assignment_id_is_required_by_the_schema(pool: PgPool) -> Result<(), anyhow::Error> {
    register_healthy_mc(&pool, "mc-1", "us-east-1", 10, 100).await?;
    let err = sqlx::query(
        "INSERT INTO meeting_assignments (meeting_id, meeting_controller_id, region, assigned_by_gc_id) \
         VALUES ('m-schema', 'mc-1', 'us-east-1', 'gc-1')",
    )
    .execute(&pool)
    .await
    .expect_err("an insert without assignment_id must fail");
    assert_eq!(
        err.as_database_error().and_then(|d| d.code()).as_deref(),
        Some("23502"),
        "not_null_violation, got {err:?}"
    );

    sqlx::query(
        "INSERT INTO meeting_assignments \
         (meeting_id, meeting_controller_id, region, assigned_by_gc_id, assignment_id) \
         VALUES ('m-schema', 'mc-1', 'us-east-1', 'gc-1', $1)",
    )
    .bind(AssignmentId::new_v4().as_uuid())
    .execute(&pool)
    .await?;
    Ok(())
}

/// Set an MC's heartbeat age exactly.
async fn set_heartbeat_age(pool: &PgPool, mc_id: &str, age_secs: i64) -> Result<(), anyhow::Error> {
    sqlx::query(
        "UPDATE meeting_controllers SET last_heartbeat_at = NOW() - ($2 || ' seconds')::INTERVAL \
         WHERE controller_id = $1",
    )
    .bind(mc_id)
    .bind(age_secs.to_string())
    .execute(pool)
    .await?;
    Ok(())
}

/// The liveness predicate honours the CONFIGURED threshold, not a constant:
/// (threshold, heartbeat age) → alive? Checked for all three consumers —
/// join reuse, the failover arm, and the cleanup backstop.
#[sqlx::test(migrations = "../../migrations")]
async fn test_liveness_follows_the_configured_threshold(pool: PgPool) -> Result<(), anyhow::Error> {
    register_healthy_mc(&pool, "mc-new", "us-east-1", 0, 100).await?;
    let cases: &[(u64, i64, bool)] = &[(30, 25, true), (30, 35, false), (200, 100, true)];
    for (i, &(threshold, age, alive)) in cases.iter().enumerate() {
        let staleness = Duration::from_secs(threshold);
        let mc = format!("mc-t{i}");
        let meeting = format!("m-t{i}");
        register_healthy_mc(&pool, &mc, "us-east-1", 10, 100).await?;
        assign(&pool, &meeting, "us-east-1", &candidate(&mc), "gc-1").await?;
        set_heartbeat_age(&pool, &mc, age).await?;
        sqlx::query(
            "UPDATE meeting_assignments SET assigned_at = NOW() - INTERVAL '2 hours' \
             WHERE meeting_id = $1",
        )
        .bind(&meeting)
        .execute(&pool)
        .await?;

        // Reuse.
        let reused = MeetingAssignmentsRepository::get_live_assignment(
            &pool,
            &meeting,
            "us-east-1",
            staleness,
        )
        .await?;
        assert_eq!(reused.is_some(), alive, "case {i}: reuse");

        // Backstop (age floor 1 h is met).
        let ended =
            MeetingAssignmentsRepository::end_stale_assignments(&pool, 1, staleness, None).await?;
        assert_eq!(ended, u64::from(!alive), "case {i}: backstop ends iff dead");

        // Failover arm: only reachable for a still-live row, so re-run on a
        // fresh meeting in the same state.
        let fo_meeting = format!("m-fo{i}");
        sqlx::query(
            "INSERT INTO meeting_assignments \
             (meeting_id, meeting_controller_id, region, assigned_by_gc_id, assignment_id) \
             VALUES ($1, $2, 'us-east-1', 'gc-1', gen_random_uuid())",
        )
        .bind(&fo_meeting)
        .bind(&mc)
        .execute(&pool)
        .await?;
        let outcome = MeetingAssignmentsRepository::atomic_assign(
            &pool,
            &fo_meeting,
            "us-east-1",
            &candidate("mc-new"),
            "gc-2",
            AssignmentId::new_v4(),
            staleness,
            far_deadline(),
        )
        .await?;
        let AtomicAssignOutcome::Assigned {
            assignment,
            written,
        } = outcome
        else {
            anyhow::bail!("case {i}: unexpected {outcome:?}");
        };
        assert_eq!(written, !alive, "case {i}: re-point iff dead");
        assert_eq!(
            assignment.mc_id,
            if alive {
                mc.clone()
            } else {
                "mc-new".to_string()
            },
            "case {i}"
        );
    }
    Ok(())
}
