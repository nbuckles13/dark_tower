//! Wrapper-invocation Cat C tests for `mc_redis_latency_seconds` and
//! `mc_fenced_out_total` per ADR-0032 Step 3 §Cluster J.
//!
//! # Why wrapper invocation, not real Redis
//!
//! `record_redis_latency` has 16 production sites in `redis/client.rs`
//! (`get` ×4, `hset` ×4, `eval` ×4, `incr` ×2, `del` ×2). `record_fenced_out`
//! has 2 production sites (both `reason=stale_generation`, at
//! `redis/client.rs:312,516`). The existing in-`src/redis/client.rs::tests`
//! mod is pure-data serde — never instantiates `FencedRedisClient`. Driving
//! the production path through a real `redis::aio::ConnectionManager` would
//! require either:
//!   (a) a `ConnectionManager` trait abstraction in `redis/client.rs` to
//!       allow fake injection in `client.rs::tests`, or
//!   (b) a real Redis fixture in `tests/`.
//!
//! Both are scope creep beyond ADR-0032 Step 3. Tracked in
//! `docs/TODO.md §Observability Debt`. This file covers each label combo
//! emitted in production via the wrapper, satisfying the guard's
//! `tests/**/*.rs` scan with the operation labels actually emitted by the
//! production code (verified via `grep` on `redis/client.rs`):
//! `get`, `hset`, `eval`, `incr`, `del` — NOT `set`, which is a phantom from
//! the `metrics.rs:163-165` doc-comment example list.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use ::common::observability::testing::MetricAssertion;
use mc_service::observability::metrics::{record_fenced_out, record_redis_latency};

/// The 5 distinct `operation` labels actually emitted from `redis/client.rs`.
/// Asserting on a label not in this list would be wrapper-only theater (the
/// production code can never emit it).
const PRODUCTION_REDIS_OPS: &[&str] = &["get", "hset", "eval", "incr", "del"];

#[test]
fn record_redis_latency_emits_per_operation_with_adjacency() {
    for op in PRODUCTION_REDIS_OPS {
        let snap = MetricAssertion::snapshot();
        record_redis_latency(op, Duration::from_micros(500));

        snap.histogram("mc_redis_latency_seconds")
            .with_labels(&[("operation", *op)])
            .assert_observation_count_at_least(1);
        // Adjacency on every other emitted operation — catches a label-swap
        // bug where a future refactor changes "get" to "hget" etc.
        for sibling in PRODUCTION_REDIS_OPS {
            if sibling == op {
                continue;
            }
            snap.histogram("mc_redis_latency_seconds")
                .with_labels(&[("operation", *sibling)])
                .assert_observation_count(0);
        }
    }
}

#[test]
fn record_fenced_out_emits_stale_generation_reason() {
    // The only `reason` value emitted in production: `redis/client.rs:312,516`
    // both call `record_fenced_out("stale_generation")`. There is no
    // production `concurrent_write` site at HEAD, so wrapper-invocation here
    // mirrors the production fact: only `stale_generation` actually fires.
    let snap = MetricAssertion::snapshot();
    record_fenced_out("stale_generation");

    snap.counter("mc_fenced_out_total")
        .with_labels(&[("reason", "stale_generation")])
        .assert_delta(1);
}
