//! Tests for heartbeat task behavior.
//!
//! Uses tokio's test-util time control features to verify:
//! - Heartbeat tasks spawn and run correctly
//! - Shutdown propagation via CancellationToken
//! - Heartbeat interval timing

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use meeting_controller::actors::ControllerMetrics;
use tokio_util::sync::CancellationToken;

// ============================================================================
// Heartbeat Task Simulation Tests
// ============================================================================

/// Simulates the fast heartbeat loop logic for testing.
/// Uses Burst mode to ensure all ticks are counted for test assertions.
async fn run_fast_heartbeat_loop(
    cancel_token: CancellationToken,
    metrics: Arc<ControllerMetrics>,
    heartbeat_count: Arc<AtomicU32>,
    interval: Duration,
) {
    let mut ticker = tokio::time::interval(interval);
    // Use Burst for testing to ensure predictable tick counts
    // (Production uses Skip, but that makes assertions flaky with simulated time)
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Burst);

    loop {
        tokio::select! {
            () = cancel_token.cancelled() => {
                break;
            }
            _ = ticker.tick() => {
                let _meetings = metrics.meetings();
                let _participants = metrics.participants();
                heartbeat_count.fetch_add(1, Ordering::SeqCst);
            }
        }
    }
}

#[tokio::test(start_paused = true)]
async fn test_heartbeat_task_runs_at_interval() {
    let cancel_token = CancellationToken::new();
    let metrics = ControllerMetrics::new();
    let heartbeat_count = Arc::new(AtomicU32::new(0));

    let token_clone = cancel_token.clone();
    let metrics_clone = Arc::clone(&metrics);
    let count_clone = Arc::clone(&heartbeat_count);

    // Spawn heartbeat task with 1 second interval
    tokio::spawn(async move {
        run_fast_heartbeat_loop(
            token_clone,
            metrics_clone,
            count_clone,
            Duration::from_secs(1),
        )
        .await;
    });

    // Initial tick happens immediately
    tokio::time::advance(Duration::from_millis(10)).await;
    tokio::task::yield_now().await;
    assert_eq!(heartbeat_count.load(Ordering::SeqCst), 1);

    // Advance 1 second - should trigger another heartbeat
    tokio::time::advance(Duration::from_secs(1)).await;
    tokio::task::yield_now().await;
    assert_eq!(heartbeat_count.load(Ordering::SeqCst), 2);

    // Advance 3 more seconds - should trigger 3 more heartbeats
    tokio::time::advance(Duration::from_secs(3)).await;
    tokio::task::yield_now().await;
    assert_eq!(heartbeat_count.load(Ordering::SeqCst), 5);

    // Cancel and verify task exits
    cancel_token.cancel();
    tokio::time::advance(Duration::from_millis(100)).await;
    tokio::task::yield_now().await;

    // No more heartbeats after cancellation
    let final_count = heartbeat_count.load(Ordering::SeqCst);
    tokio::time::advance(Duration::from_secs(5)).await;
    tokio::task::yield_now().await;
    assert_eq!(heartbeat_count.load(Ordering::SeqCst), final_count);
}

#[tokio::test(start_paused = true)]
async fn test_heartbeat_task_shutdown_propagation() {
    let parent_token = CancellationToken::new();
    let child_token = parent_token.child_token();
    let heartbeat_count = Arc::new(AtomicU32::new(0));
    let metrics = ControllerMetrics::new();

    let token_clone = child_token.clone();
    let metrics_clone = Arc::clone(&metrics);
    let count_clone = Arc::clone(&heartbeat_count);

    // Task completed flag
    let task_completed = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let completed_clone = Arc::clone(&task_completed);

    tokio::spawn(async move {
        run_fast_heartbeat_loop(
            token_clone,
            metrics_clone,
            count_clone,
            Duration::from_secs(1),
        )
        .await;
        completed_clone.store(true, Ordering::SeqCst);
    });

    // Let it run a bit
    tokio::time::advance(Duration::from_secs(2)).await;
    tokio::task::yield_now().await;
    assert!(!task_completed.load(Ordering::SeqCst));

    // Cancel parent - should propagate to child
    parent_token.cancel();
    tokio::time::advance(Duration::from_millis(100)).await;
    tokio::task::yield_now().await;

    assert!(task_completed.load(Ordering::SeqCst));
}

#[tokio::test(start_paused = true)]
async fn test_heartbeat_reads_current_metrics() {
    let cancel_token = CancellationToken::new();
    let metrics = ControllerMetrics::new();
    let captured_meetings = Arc::new(AtomicU32::new(0));
    let captured_participants = Arc::new(AtomicU32::new(0));

    let token_clone = cancel_token.clone();
    let metrics_clone = Arc::clone(&metrics);
    let meetings_clone = Arc::clone(&captured_meetings);
    let participants_clone = Arc::clone(&captured_participants);

    // Modified heartbeat loop that captures metrics
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(1));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                () = token_clone.cancelled() => {
                    break;
                }
                _ = ticker.tick() => {
                    meetings_clone.store(metrics_clone.meetings(), Ordering::SeqCst);
                    participants_clone.store(metrics_clone.participants(), Ordering::SeqCst);
                }
            }
        }
    });

    // Initial tick
    tokio::time::advance(Duration::from_millis(10)).await;
    tokio::task::yield_now().await;
    assert_eq!(captured_meetings.load(Ordering::SeqCst), 0);
    assert_eq!(captured_participants.load(Ordering::SeqCst), 0);

    // Update metrics
    metrics.set_meetings(5);
    metrics.set_participants(50);

    // Next tick should capture new values
    tokio::time::advance(Duration::from_secs(1)).await;
    tokio::task::yield_now().await;
    assert_eq!(captured_meetings.load(Ordering::SeqCst), 5);
    assert_eq!(captured_participants.load(Ordering::SeqCst), 50);

    cancel_token.cancel();
}

#[tokio::test(start_paused = true)]
async fn test_multiple_heartbeat_tasks_independent() {
    let parent_token = CancellationToken::new();
    let fast_token = parent_token.child_token();
    let comprehensive_token = parent_token.child_token();

    let fast_count = Arc::new(AtomicU32::new(0));
    let comprehensive_count = Arc::new(AtomicU32::new(0));
    let metrics = ControllerMetrics::new();

    // Fast heartbeat (1 second interval)
    let token = fast_token.clone();
    let m = Arc::clone(&metrics);
    let count = Arc::clone(&fast_count);
    tokio::spawn(async move {
        run_fast_heartbeat_loop(token, m, count, Duration::from_secs(1)).await;
    });

    // Comprehensive heartbeat (3 second interval)
    let token = comprehensive_token.clone();
    let m = Arc::clone(&metrics);
    let count = Arc::clone(&comprehensive_count);
    tokio::spawn(async move {
        run_fast_heartbeat_loop(token, m, count, Duration::from_secs(3)).await;
    });

    // Initial ticks
    tokio::time::advance(Duration::from_millis(10)).await;
    tokio::task::yield_now().await;
    assert_eq!(fast_count.load(Ordering::SeqCst), 1);
    assert_eq!(comprehensive_count.load(Ordering::SeqCst), 1);

    // After 3 seconds: fast=4, comprehensive=2
    tokio::time::advance(Duration::from_secs(3)).await;
    tokio::task::yield_now().await;
    assert_eq!(fast_count.load(Ordering::SeqCst), 4);
    assert_eq!(comprehensive_count.load(Ordering::SeqCst), 2);

    // After 6 more seconds: fast=10, comprehensive=4
    tokio::time::advance(Duration::from_secs(6)).await;
    tokio::task::yield_now().await;
    assert_eq!(fast_count.load(Ordering::SeqCst), 10);
    assert_eq!(comprehensive_count.load(Ordering::SeqCst), 4);

    // Cancel parent - both should stop
    parent_token.cancel();
    tokio::time::advance(Duration::from_millis(100)).await;
    tokio::task::yield_now().await;

    let final_fast = fast_count.load(Ordering::SeqCst);
    let final_comprehensive = comprehensive_count.load(Ordering::SeqCst);

    // No more updates
    tokio::time::advance(Duration::from_secs(10)).await;
    tokio::task::yield_now().await;
    assert_eq!(fast_count.load(Ordering::SeqCst), final_fast);
    assert_eq!(
        comprehensive_count.load(Ordering::SeqCst),
        final_comprehensive
    );
}
