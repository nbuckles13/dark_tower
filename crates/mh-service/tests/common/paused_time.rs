//! Paused-time stepping for the Tier 1b rigs (heartbeat, drain, owner silence,
//! GC task). Time moves only through these helpers — small steps with yields,
//! never a sleep — so each due instant is visited on its own.
//!
//! (`stream_rig.rs` keeps its own 200-yield `settle`: its per-group tasks need
//! more turns to quiesce, and sharing the smaller count would make it flaky.)

use std::future::Future;
use std::time::Duration;

/// Time step: fine enough that every due instant is visited on its own.
pub const STEP: Duration = Duration::from_millis(10);

/// Let every runnable task run to quiescence (no time passes).
pub async fn settle() {
    for _ in 0..50 {
        tokio::task::yield_now().await;
    }
}

/// Advance paused time by `total` in [`STEP`]s, settling after each.
pub async fn run_for(total: Duration) {
    let mut elapsed = Duration::ZERO;
    while elapsed < total {
        tokio::time::advance(STEP).await;
        settle().await;
        elapsed += STEP;
    }
}

/// Step paused time until `done` holds, failing LOUDLY (never hanging) once
/// `bound` of paused time has passed without it — a regression that never
/// produces the awaited event fails with `what` named.
pub async fn step_until<F, Fut>(mut done: F, bound: Duration, what: &str)
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    let mut waited = Duration::ZERO;
    while !done().await {
        assert!(
            waited < bound,
            "{what}: not reached within {bound:?} of paused time"
        );
        tokio::time::advance(STEP).await;
        settle().await;
        waited += STEP;
    }
}
