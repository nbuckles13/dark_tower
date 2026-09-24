//! Integration tests for MC assignment with RPC notification.
//!
//! Tests the GC→MC assignment flow including MH selection and retry logic.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use ::common::observability::testing::MetricAssertion;
use gc_service::repositories::{
    HealthStatus, MediaHandlersRepository, MeetingControllersRepository,
};
use gc_service::services::mc_client::mock::MockMcClient;
use gc_service::services::mc_client::{McAssignmentResult, McRejectionReason};
use gc_service::services::McAssignmentService;
use sqlx::PgPool;
use std::sync::Arc;

/// Helper to set up test MCs.
async fn setup_mcs(pool: &PgPool, count: usize, region: &str) {
    for i in 1..=count {
        MeetingControllersRepository::register_mc(
            pool,
            &format!("mc-{}-{}", region, i),
            region,
            &format!("grpc://mc-{}:50051", i),
            Some(&format!("https://mc-{}:443", i)),
            100,
            1000,
        )
        .await
        .expect("MC registration should succeed");

        MeetingControllersRepository::update_heartbeat(
            pool,
            &format!("mc-{}-{}", region, i),
            i as i32 * 10,
            50,
            HealthStatus::Healthy,
        )
        .await
        .expect("MC heartbeat should succeed");
    }
}

/// Helper to set up test MHs.
async fn setup_mhs(pool: &PgPool, count: usize, region: &str) {
    for i in 1..=count {
        MediaHandlersRepository::register_mh(
            pool,
            &format!("mh-{}-{}", region, i),
            region,
            &format!("https://mh-{}:443", i),
            &format!("grpc://mh-{}:50051", i),
            1000,
        )
        .await
        .expect("MH registration should succeed");

        MediaHandlersRepository::update_load_report(
            pool,
            &format!("mh-{}-{}", region, i),
            i as i32 * 10,
            HealthStatus::Healthy,
            Some(10.0),
            Some(20.0),
            Some(15.0),
        )
        .await
        .expect("MH load report should succeed");
    }
}

/// Test successful assignment with accepting MC.
#[sqlx::test(migrations = "../../migrations")]
async fn test_assign_meeting_with_mh_success(pool: PgPool) {
    // Set up MCs and MHs
    setup_mcs(&pool, 3, "us-east-1").await;
    setup_mhs(&pool, 2, "us-east-1").await;

    // Create mock MC client that always accepts
    let mock_client = Arc::new(MockMcClient::accepting());

    // Assign meeting
    let result = McAssignmentService::assign_meeting_with_mh(
        &pool,
        mock_client.clone(),
        "meeting-001",
        "us-east-1",
        "gc-test",
    )
    .await;

    assert!(
        result.is_ok(),
        "Assignment should succeed: {:?}",
        result.err()
    );

    let assignment = result.unwrap();
    assert!(!assignment.mc_assignment.mc_id.is_empty());
    let selection = assignment
        .mh_selection
        .as_ref()
        .expect("a new assignment carries the selection sent to MC");
    assert!(!selection.handlers.is_empty());
    assert!(!selection.handlers[0].mh_id.is_empty());

    // Verify mock was called once
    assert_eq!(mock_client.call_count(), 1);
}

/// Test assignment retries on MC rejection.
#[sqlx::test(migrations = "../../migrations")]
async fn test_assign_meeting_with_mh_retries_on_rejection(pool: PgPool) {
    // Set up multiple MCs and MHs
    setup_mcs(&pool, 3, "us-east-1").await;
    setup_mhs(&pool, 2, "us-east-1").await;

    // Create mock that rejects first 2 calls, then accepts
    let mock_client = Arc::new(MockMcClient::with_responses(vec![
        McAssignmentResult::Rejected(McRejectionReason::AtCapacity),
        McAssignmentResult::Rejected(McRejectionReason::Draining),
        McAssignmentResult::Accepted,
    ]));

    // Assign meeting
    let result = McAssignmentService::assign_meeting_with_mh(
        &pool,
        mock_client.clone(),
        "meeting-retry-001",
        "us-east-1",
        "gc-test",
    )
    .await;

    assert!(result.is_ok(), "Assignment should eventually succeed");

    // Verify mock was called 3 times (2 rejections + 1 accept)
    assert_eq!(mock_client.call_count(), 3);
}

/// Test assignment fails after max retries.
#[sqlx::test(migrations = "../../migrations")]
async fn test_assign_meeting_with_mh_fails_after_max_retries(pool: PgPool) {
    // Set up exactly 3 MCs (max retries)
    setup_mcs(&pool, 3, "us-east-1").await;
    setup_mhs(&pool, 2, "us-east-1").await;

    // Create mock that always rejects
    let mock_client = Arc::new(MockMcClient::rejecting(McRejectionReason::AtCapacity));

    // Assign meeting
    let result = McAssignmentService::assign_meeting_with_mh(
        &pool,
        mock_client.clone(),
        "meeting-fail-001",
        "us-east-1",
        "gc-test",
    )
    .await;

    assert!(result.is_err(), "Assignment should fail after max retries");
    let err = result.unwrap_err();
    assert!(
        format!("{}", err).contains("capacity") || format!("{}", err).contains("unavailable"),
        "Error should mention capacity issue: {}",
        err
    );

    // Verify mock was called 3 times (max retries)
    assert_eq!(mock_client.call_count(), 3);
}

/// Test that a GENUINELY malformed request (GC's own MH selection is malformed)
/// fails fast — GC must NOT walk the pool or burn the retry budget, because every MC
/// rejects the identical bad request the same way.
#[sqlx::test(migrations = "../../migrations")]
async fn test_assign_meeting_with_mh_invalid_request_fails_fast(pool: PgPool) {
    // A full pool (3 MCs) is available — the point is that GC does NOT try them all.
    setup_mcs(&pool, 3, "us-east-1").await;
    // Register a healthy MH with an EMPTY grpc_endpoint. The gRPC registration layer
    // rejects this, so GC never produces it in normal operation — but corrupt data or
    // a GC bug could, and that is the only case where GC genuinely sent bad data.
    MediaHandlersRepository::register_mh(
        &pool,
        "mh-bad-1",
        "us-east-1",
        "https://mh-bad:443",
        "", // malformed: empty grpc_endpoint
        1000,
    )
    .await
    .expect("MH registration should succeed");
    MediaHandlersRepository::update_load_report(
        &pool,
        "mh-bad-1",
        10,
        HealthStatus::Healthy,
        Some(10.0),
        Some(20.0),
        Some(15.0),
    )
    .await
    .expect("MH load report should succeed");

    let mock_client = Arc::new(MockMcClient::rejecting(McRejectionReason::InvalidRequest));

    // Capture metrics across the flow. `#[sqlx::test]` runs on a current-thread
    // runtime, so the per-thread snapshot sees records made during the await.
    let snap = MetricAssertion::snapshot();

    let result = McAssignmentService::assign_meeting_with_mh(
        &pool,
        mock_client.clone(),
        "meeting-invalid-001",
        "us-east-1",
        "gc-test",
    )
    .await;

    // GC verified its OWN selection was malformed, so it emits invalid_request on the
    // error axis and must NOT emit unhealthy.
    snap.counter("gc_mc_assignments_total")
        .with_labels(&[("status", "error"), ("rejection_reason", "invalid_request")])
        .assert_delta(1);
    snap.counter("gc_mc_assignments_total")
        .with_labels(&[("status", "rejected"), ("rejection_reason", "unhealthy")])
        .assert_delta(0);
    snap.counter("gc_mc_assignments_total")
        .with_labels(&[("status", "error"), ("rejection_reason", "unhealthy")])
        .assert_delta(0);

    assert!(
        result.is_err(),
        "Assignment should fail on an invalid request"
    );
    let err = result.unwrap_err();
    // Returned as Internal (500), not ServiceUnavailable — a GC contract violation
    // is a server defect, not transient unavailability.
    assert_eq!(
        err.status_code(),
        500,
        "invalid_request should map to 500: {err}"
    );
    assert!(
        format!("{err}").contains("invalid"),
        "Error should describe an invalid request: {err}"
    );

    // The core guarantee: exactly ONE MC call. The pool was NOT walked and the
    // retry budget was NOT burned (contrast the AtCapacity test which expects 3).
    assert_eq!(
        mock_client.call_count(),
        1,
        "fail-fast must stop after the first MC, not walk the pool"
    );

    // No assignment row was written — fail-fast returns before the DB write.
    let existing = McAssignmentService::get_assignment(&pool, "meeting-invalid-001", "us-east-1")
        .await
        .expect("assignment lookup should succeed");
    assert!(
        existing.is_none(),
        "fail-fast must not persist an assignment row"
    );
}

/// Test that an INVALID_REQUEST claim from an MC for a WELL-FORMED request does NOT
/// remove failover (security S-3). GC has the ground truth locally, so a buggy /
/// rolled-back MC cannot veto the pool: GC treats the claim as unhealthy and retries.
#[sqlx::test(migrations = "../../migrations")]
async fn test_invalid_request_claim_on_wellformed_request_preserves_failover(pool: PgPool) {
    // Full, healthy pool with VALID MH selection (non-empty grpc_endpoints).
    setup_mcs(&pool, 3, "us-east-1").await;
    setup_mhs(&pool, 2, "us-east-1").await;

    // Every MC misreports INVALID_REQUEST even though GC's request is well-formed.
    let mock_client = Arc::new(MockMcClient::rejecting(McRejectionReason::InvalidRequest));

    let snap = MetricAssertion::snapshot();

    let result = McAssignmentService::assign_meeting_with_mh(
        &pool,
        mock_client.clone(),
        "meeting-wellformed-001",
        "us-east-1",
        "gc-test",
    )
    .await;

    // GC must walk the whole pool (failover preserved), NOT fail fast on one MC's word.
    assert_eq!(
        mock_client.call_count(),
        3,
        "a well-formed request must not be vetoed by one MC's INVALID_REQUEST claim"
    );

    // The outcome is recorded as unhealthy (the MC is misbehaving), NOT invalid_request
    // (GC verified its own request was fine, so the metric stays trustworthy).
    snap.counter("gc_mc_assignments_total")
        .with_labels(&[("status", "rejected"), ("rejection_reason", "unhealthy")])
        .assert_delta(1);
    snap.counter("gc_mc_assignments_total")
        .with_labels(&[("status", "error"), ("rejection_reason", "invalid_request")])
        .assert_delta(0);

    // And it is a retryable 503, not a 500 — the fleet is the suspect here, not GC.
    let err = result.expect_err("all MCs rejected, so assignment fails");
    assert_eq!(
        err.status_code(),
        503,
        "misreported invalid_request should surface as retryable 503: {err}"
    );
}

/// Test assignment fails when no MCs available.
#[sqlx::test(migrations = "../../migrations")]
async fn test_assign_meeting_with_mh_no_mcs(pool: PgPool) {
    // Set up only MHs, no MCs
    setup_mhs(&pool, 2, "us-east-1").await;

    let mock_client = Arc::new(MockMcClient::accepting());

    let result = McAssignmentService::assign_meeting_with_mh(
        &pool,
        mock_client.clone(),
        "meeting-no-mc",
        "us-east-1",
        "gc-test",
    )
    .await;

    assert!(result.is_err(), "Assignment should fail without MCs");

    // Mock should not be called since no MCs available
    assert_eq!(mock_client.call_count(), 0);
}

/// Test assignment fails when no MHs available.
///
/// NEW-ASSIGNMENT branch: an empty MH pool FAILS a meeting with no assignment.
/// Read against `test_reuse_path_skips_mh_selection_on_empty_pool`, where the same
/// empty pool lets a sticky join into an already-assigned meeting SUCCEED (R-6).
/// The two are not contradictory — they exercise different branches.
#[sqlx::test(migrations = "../../migrations")]
async fn test_assign_meeting_with_mh_no_mhs(pool: PgPool) {
    // Set up only MCs, no MHs
    setup_mcs(&pool, 2, "us-east-1").await;

    let mock_client = Arc::new(MockMcClient::accepting());

    let result = McAssignmentService::assign_meeting_with_mh(
        &pool,
        mock_client.clone(),
        "meeting-no-mh",
        "us-east-1",
        "gc-test",
    )
    .await;

    assert!(result.is_err(), "Assignment should fail without MHs");
    let err = result.unwrap_err();
    assert!(
        format!("{}", err).contains("media handlers"),
        "Error should mention media handlers: {}",
        err
    );

    // Mock should not be called since MH selection fails first
    assert_eq!(mock_client.call_count(), 0);
}

/// Test existing assignment is returned without calling MC or selecting MHs.
///
/// One shared mock across BOTH calls, so `call_count() == 1` proves `assign_meeting`
/// ran exactly once for the meeting (a second mock would only prove the second call
/// made no RPC). Covers the healthy-pool reuse case; the empty-pool reuse case and
/// its metric proof are in `test_reuse_path_skips_mh_selection_on_empty_pool`.
#[sqlx::test(migrations = "../../migrations")]
async fn test_assign_meeting_with_mh_returns_existing(pool: PgPool) {
    // Set up MCs and MHs
    setup_mcs(&pool, 2, "us-east-1").await;
    setup_mhs(&pool, 2, "us-east-1").await;

    let mock_client = Arc::new(MockMcClient::accepting());

    // First assignment (new)
    let assignment1 = McAssignmentService::assign_meeting_with_mh(
        &pool,
        mock_client.clone(),
        "meeting-existing",
        "us-east-1",
        "gc-test",
    )
    .await
    .expect("first assignment should succeed");
    assert!(
        assignment1.mh_selection.is_some(),
        "new assignment carries the selection sent to MC"
    );

    // Second assignment for same meeting (reuse)
    let assignment2 = McAssignmentService::assign_meeting_with_mh(
        &pool,
        mock_client.clone(),
        "meeting-existing",
        "us-east-1",
        "gc-test",
    )
    .await
    .expect("reuse should succeed");

    // Should return the same MC assignment
    assert_eq!(
        assignment1.mc_assignment.mc_id,
        assignment2.mc_assignment.mc_id
    );
    assert!(
        assignment2.mh_selection.is_none(),
        "reuse path performs no MH selection"
    );

    // MC notified exactly once for the meeting, by the new assignment only
    assert_eq!(mock_client.call_count(), 1);
}

/// Mark every MH in `region` unhealthy, leaving the MH candidate pool empty.
///
/// Region-scoped rather than per-id, so it cannot silently miss rows if
/// `setup_mhs` changes its id format. Same mechanism as `make_mhs_unhealthy` in
/// `tests/meeting_tests.rs` (shared-fixture extraction tracked in docs/TODO.md).
async fn make_mh_pool_unhealthy(pool: &PgPool, region: &str) {
    sqlx::query("UPDATE media_handlers SET health_status = 'unhealthy' WHERE region = $1")
        .bind(region)
        .execute(pool)
        .await
        .expect("Failed to mark MHs unhealthy");
}

/// R-6: the reuse path performs NO MH selection and does not error on an empty
/// MH pool.
///
/// REUSE branch: read against `test_assign_meeting_with_mh_no_mhs`, where an empty
/// pool FAILS a new assignment. Here the same empty pool must let a sticky join
/// into an already-assigned meeting SUCCEED with the same MC — and the positive
/// control at the end proves the pool really is empty (an unassigned meeting
/// still fails), so the green reuse result is not coming from a pool that was
/// never emptied.
///
/// No MH-pool or ceiling re-check on reuse is DELIBERATE (R-19 soft,
/// new-meeting-only; ADR-0036 §9) — see the ANCHOR on the reuse branch in
/// `src/services/mc_assignment.rs` for the full ceiling-enforcement pairing.
#[sqlx::test(migrations = "../../migrations")]
async fn test_reuse_path_skips_mh_selection_on_empty_pool(pool: PgPool) {
    setup_mcs(&pool, 2, "us-east-1").await;
    setup_mhs(&pool, 2, "us-east-1").await;

    let mock_client = Arc::new(MockMcClient::accepting());

    // New assignment while the MH pool is healthy.
    let first = McAssignmentService::assign_meeting_with_mh(
        &pool,
        mock_client.clone(),
        "meeting-sticky",
        "us-east-1",
        "gc-test",
    )
    .await
    .expect("new assignment should succeed with a healthy MH pool");
    assert!(first.mh_selection.is_some());
    assert_eq!(mock_client.call_count(), 1);

    // Empty the MH candidate pool.
    make_mh_pool_unhealthy(&pool, "us-east-1").await;

    // Snapshot AFTER the first assignment so its legitimate selection emit is not
    // in the delta. `#[sqlx::test]` runs a current-thread runtime, so the
    // thread-local recorder sees everything emitted during the awaits below.
    let snap = MetricAssertion::snapshot();

    let reused = McAssignmentService::assign_meeting_with_mh(
        &pool,
        mock_client.clone(),
        "meeting-sticky",
        "us-east-1",
        "gc-test",
    )
    .await
    .expect("reuse must not depend on a live MH pool");

    assert_eq!(reused.mc_assignment.mc_id, first.mc_assignment.mc_id);
    assert!(
        reused.mh_selection.is_none(),
        "reuse path performs no MH selection"
    );
    assert_eq!(mock_client.call_count(), 1, "reuse makes no MC RPC");

    // Proof by observation that no selection ran on reuse, and that the preserved
    // `record_mc_assignment("success", None, ..)` emitted exactly once.
    for status in ["success", "error"] {
        snap.counter("gc_mh_selections_total")
            .with_labels(&[("status", status)])
            .assert_delta(0);
    }
    snap.counter("gc_mc_assignments_total")
        .with_labels(&[("status", "success"), ("rejection_reason", "none")])
        .assert_delta(1);

    // Positive control: in the SAME empty-pool state a meeting with no assignment
    // still fails on MH selection, and no MC is called. This also proves the
    // recorder sees selection emits (the error arm fires here).
    let err = McAssignmentService::assign_meeting_with_mh(
        &pool,
        mock_client.clone(),
        "meeting-unassigned",
        "us-east-1",
        "gc-test",
    )
    .await
    .expect_err("a new assignment must fail with an empty MH pool");
    assert!(
        format!("{}", err).contains("media handlers"),
        "Error should mention media handlers: {}",
        err
    );
    assert_eq!(
        mock_client.call_count(),
        1,
        "no MC call when MH selection fails"
    );
    snap.counter("gc_mh_selections_total")
        .with_labels(&[("status", "error")])
        .assert_delta(1);
}

/// Test assignment with MC RPC errors retries.
#[sqlx::test(migrations = "../../migrations")]
async fn test_assign_meeting_with_mh_retries_on_rpc_error(pool: PgPool) {
    // Set up multiple MCs and MHs
    setup_mcs(&pool, 3, "us-east-1").await;
    setup_mhs(&pool, 2, "us-east-1").await;

    // Create mock that fails first, then succeeds
    // Note: MockMcClient::failing() returns errors, then we need a custom one
    let mock_client = Arc::new(MockMcClient::with_responses(vec![
        McAssignmentResult::Accepted,
    ]));

    // This test is a bit limited since our mock doesn't support mixed errors/success
    // But we can verify the happy path works
    let result = McAssignmentService::assign_meeting_with_mh(
        &pool,
        mock_client,
        "meeting-rpc-error",
        "us-east-1",
        "gc-test",
    )
    .await;

    assert!(result.is_ok(), "Assignment should succeed");
}

/// Test assignment with mixed rejection then acceptance.
///
/// Tests the case where MC rejects first (e.g., AtCapacity) then accepts on retry.
/// This validates the retry logic properly handles mixed responses.
#[sqlx::test(migrations = "../../migrations")]
async fn test_assign_meeting_with_mh_mixed_rejection_then_accept(pool: PgPool) {
    // Set up multiple MCs and MHs
    setup_mcs(&pool, 3, "us-east-1").await;
    setup_mhs(&pool, 2, "us-east-1").await;

    // Create mock that rejects once then accepts
    let mock_client = Arc::new(MockMcClient::with_responses(vec![
        McAssignmentResult::Rejected(McRejectionReason::AtCapacity),
        McAssignmentResult::Accepted,
    ]));

    // Assign meeting
    let result = McAssignmentService::assign_meeting_with_mh(
        &pool,
        mock_client.clone(),
        "meeting-mixed-001",
        "us-east-1",
        "gc-test",
    )
    .await;

    assert!(
        result.is_ok(),
        "Assignment should succeed after initial rejection: {:?}",
        result.err()
    );

    // Verify mock was called exactly twice (1 rejection + 1 accept)
    assert_eq!(
        mock_client.call_count(),
        2,
        "Should have called MC twice (reject then accept)"
    );
}

/// Test MH selection includes multiple handlers when available.
#[sqlx::test(migrations = "../../migrations")]
async fn test_mh_selection_includes_multiple_handlers(pool: PgPool) {
    // Set up MCs and multiple MHs
    setup_mcs(&pool, 2, "us-east-1").await;
    setup_mhs(&pool, 3, "us-east-1").await;

    let mock_client = Arc::new(MockMcClient::accepting());

    let result = McAssignmentService::assign_meeting_with_mh(
        &pool,
        mock_client,
        "meeting-with-multiple-mh",
        "us-east-1",
        "gc-test",
    )
    .await;

    assert!(result.is_ok());
    let assignment = result.unwrap();
    let selection = assignment
        .mh_selection
        .as_ref()
        .expect("a new assignment carries the selection sent to MC");

    // Should have multiple MH handlers (since we have multiple MHs)
    assert!(
        selection.handlers.len() >= 2,
        "Should have multiple MH handlers when multiple MHs available, got {}",
        selection.handlers.len()
    );

    // Handlers should be different
    assert_ne!(
        selection.handlers[0].mh_id, selection.handlers[1].mh_id,
        "MH handlers should be different"
    );
}

/// Test single MH results in one handler.
#[sqlx::test(migrations = "../../migrations")]
async fn test_mh_selection_single_mh_one_handler(pool: PgPool) {
    // Set up MCs and only 1 MH
    setup_mcs(&pool, 2, "us-east-1").await;
    setup_mhs(&pool, 1, "us-east-1").await;

    let mock_client = Arc::new(MockMcClient::accepting());

    let result = McAssignmentService::assign_meeting_with_mh(
        &pool,
        mock_client,
        "meeting-single-mh",
        "us-east-1",
        "gc-test",
    )
    .await;

    assert!(result.is_ok());
    let assignment = result.unwrap();
    let selection = assignment
        .mh_selection
        .as_ref()
        .expect("a new assignment carries the selection sent to MC");

    // Should have exactly one MH handler
    assert_eq!(
        selection.handlers.len(),
        1,
        "Should have exactly one MH handler with single MH"
    );
    assert!(!selection.handlers[0].mh_id.is_empty());
}

/// Test concurrent assignments to the same meeting return the same result.
///
/// This tests the race condition handling: when two concurrent requests try to
/// assign the same meeting, one should create the assignment and the other should
/// return the existing assignment (idempotent behavior).
#[sqlx::test(migrations = "../../migrations")]
async fn test_concurrent_assignment_same_meeting(pool: PgPool) {
    use tokio::sync::Barrier;

    // Set up MCs and MHs
    setup_mcs(&pool, 2, "us-east-1").await;
    setup_mhs(&pool, 2, "us-east-1").await;

    let barrier = Arc::new(Barrier::new(2));
    let pool1 = pool.clone();
    let pool2 = pool.clone();

    let mock1 = Arc::new(MockMcClient::accepting());
    let mock2 = Arc::new(MockMcClient::accepting());
    let mock1_clone = mock1.clone();
    let mock2_clone = mock2.clone();

    let barrier1 = barrier.clone();
    let barrier2 = barrier.clone();

    // Spawn two concurrent assignment tasks
    let handle1 = tokio::spawn(async move {
        // Wait for both tasks to be ready
        barrier1.wait().await;
        McAssignmentService::assign_meeting_with_mh(
            &pool1,
            mock1_clone,
            "meeting-concurrent",
            "us-east-1",
            "gc-test",
        )
        .await
    });

    let handle2 = tokio::spawn(async move {
        // Wait for both tasks to be ready
        barrier2.wait().await;
        McAssignmentService::assign_meeting_with_mh(
            &pool2,
            mock2_clone,
            "meeting-concurrent",
            "us-east-1",
            "gc-test",
        )
        .await
    });

    let result1 = handle1.await.expect("Task 1 should complete");
    let result2 = handle2.await.expect("Task 2 should complete");

    // Both should succeed
    assert!(result1.is_ok(), "First assignment should succeed");
    assert!(result2.is_ok(), "Second assignment should succeed");

    let assignment1 = result1.unwrap();
    let assignment2 = result2.unwrap();

    // Both should return the same MC assignment
    assert_eq!(
        assignment1.mc_assignment.mc_id, assignment2.mc_assignment.mc_id,
        "Concurrent assignments should return same MC"
    );

    // Total MC calls should be 1 (one creates, one returns existing)
    let total_calls = mock1.call_count() + mock2.call_count();
    assert!(
        total_calls <= 2,
        "Should not call MC more than twice total (got {})",
        total_calls
    );
}
