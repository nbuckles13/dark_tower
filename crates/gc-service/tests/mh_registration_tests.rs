// Every `#[sqlx::test]` is implicitly `flavor = "current_thread"`, and that is
// LOAD-BEARING: `MetricAssertion` binds a per-thread recorder, and the
// concurrency test's spawned tasks stay on the recorder thread.
//
//! The RegisterMH endpoint rule (ADR-0039 §7; stated once above
//! `RegisterMHRequest` in `internal.proto`), driven through the real
//! `MhService::register_mh`, one test per arm, each negative next to its
//! positive arm. Every arm asserts the `gc_mh_registrations_total` cell it
//! records (+1) and that every other cell is unobserved (ADR-0032).
//!
//! Endpoints come from the gc-test-utils allowlist fixture (one home); the
//! staleness bound is `freshness_registry` (B = 3750 ms), so "stale" is
//! exercised against a millisecond-accurate bound, not a retyped number.

#![expect(
    clippy::unwrap_used,
    reason = "integration test: an unexpected Err IS the test failure (ADR-0002 test carve-out)"
)]

#[path = "common/mod.rs"]
mod test_common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use ::common::observability::testing::{MetricAssertion, MetricSnapshot};
use gc_service::grpc::MhService;
use gc_service::observability::metrics::{MhRegistrationOutcome, MhSupersedeMatch};
use gc_service::repositories::MH_REGISTRY_LOCK_KEY;
use gc_test_utils::{
    test_grpc_endpoint, test_grpc_scheme, test_mh_endpoint_allowlist, test_wt_endpoint,
    TEST_GRPC_PORT, TEST_WT_EXACT_HOST, TEST_WT_PORTS,
};
use proto_gen::dark_tower::internal::v1::media_handler_registry_service_server::MediaHandlerRegistryService;
use proto_gen::dark_tower::internal::v1::RegisterMhRequest;
use sqlx::PgPool;
use test_common::mh_registry::{
    freshness_registry, ms, row_count, seed_with_endpoints, stored_row,
};
use tonic::{Code, Request};

const REGION: &str = "test-region";

fn service(pool: &PgPool) -> MhService {
    MhService::new(
        Arc::new(pool.clone()),
        &freshness_registry(2),
        test_mh_endpoint_allowlist(),
    )
}

fn bound_ms() -> i64 {
    ms(freshness_registry(2).staleness_bound())
}

fn req(id: &str, wt: &str, grpc: &str) -> Request<RegisterMhRequest> {
    Request::new(RegisterMhRequest {
        handler_id: id.to_string(),
        region: REGION.to_string(),
        webtransport_endpoint: wt.to_string(),
        grpc_endpoint: grpc.to_string(),
        max_streams: 1000,
    })
}

/// Seed `id` holding (`wt`, `grpc`) with `status`, aged `age_ms`.
async fn seed(pool: &PgPool, id: &str, wt: &str, grpc: &str, status: &str, age_ms: i64) {
    seed_with_endpoints(pool, id, REGION, wt, grpc, status, age_ms).await;
}

fn labels(o: MhRegistrationOutcome) -> Vec<(&'static str, &'static str)> {
    match o {
        MhRegistrationOutcome::Superseded(m) => {
            vec![("outcome", o.outcome_label()), ("match", m.label())]
        }
        _ => vec![("outcome", o.outcome_label())],
    }
}

/// `expected` +1; every OTHER cell unobserved.
fn assert_outcome(snap: &MetricSnapshot, expected: MhRegistrationOutcome) {
    for o in MhRegistrationOutcome::ALL {
        let q = snap
            .counter("gc_mh_registrations_total")
            .with_labels(&labels(o));
        if o == expected {
            q.assert_delta(1);
        } else {
            q.assert_unobserved();
        }
    }
}

fn assert_no_outcome(snap: &MetricSnapshot) {
    for o in MhRegistrationOutcome::ALL {
        snap.counter("gc_mh_registrations_total")
            .with_labels(&labels(o))
            .assert_unobserved();
    }
}

// ---------------------------------------------------------------------------
// Arm 0 — allowlists, per zone, with cross-zone distinguishing inputs
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn arm0_webtransport_zone(pool: PgPool) {
    let svc = service(&pool);
    let grpc = test_grpc_endpoint(0, 1);
    let rejects: Vec<(String, &str)> = vec![
        // Cross-zone: the gRPC zone's address presented as a WT endpoint.
        (format!("https://10.244.0.1:{TEST_GRPC_PORT}"), "host"),
        (
            format!("http://{TEST_WT_EXACT_HOST}:{}", TEST_WT_PORTS[0]),
            "scheme",
        ),
        (format!("https://evil-mh.test:{}", TEST_WT_PORTS[0]), "host"),
        (
            format!("https://{TEST_WT_EXACT_HOST}:{}/", TEST_WT_PORTS[0]),
            "syntax",
        ),
        (
            format!("https://u@{TEST_WT_EXACT_HOST}:{}", TEST_WT_PORTS[0]),
            "syntax",
        ),
        (format!("https://{TEST_WT_EXACT_HOST}"), "syntax"),
        (
            format!(
                "https://{TEST_WT_EXACT_HOST}:{}",
                TEST_WT_PORTS.iter().max().unwrap() + 1
            ),
            "port",
        ),
    ];
    for (i, (wt, component)) in rejects.iter().enumerate() {
        let snap = MetricAssertion::snapshot();
        let err = svc
            .register_mh(req(&format!("mh-{i}"), wt, &grpc))
            .await
            .unwrap_err();
        assert_eq!(err.code(), Code::InvalidArgument, "{wt}");
        assert_eq!(
            err.message(),
            format!("webtransport_endpoint: {component} not allowed"),
            "{wt}"
        );
        assert_outcome(&snap, MhRegistrationOutcome::InvalidEndpoint);
        // The transaction never opens for an allowlist reject.
        snap.counter("gc_db_queries_total")
            .with_labels(&[("operation", "register_mh_txn")])
            .assert_unobserved();
    }
    assert_eq!(row_count(&pool).await, 0, "no row written by any reject");

    // Positive twins: the exact host and a suffix host.
    let snap = MetricAssertion::snapshot();
    svc.register_mh(req(
        "mh-ok-exact",
        &format!("https://{TEST_WT_EXACT_HOST}:{}", TEST_WT_PORTS[1]),
        &grpc,
    ))
    .await
    .unwrap();
    assert_outcome(&snap, MhRegistrationOutcome::Ok);
    let snap = MetricAssertion::snapshot();
    svc.register_mh(req(
        "mh-ok-suffix",
        &test_wt_endpoint("a"),
        &test_grpc_endpoint(0, 2),
    ))
    .await
    .unwrap();
    assert_outcome(&snap, MhRegistrationOutcome::Ok);
    assert_eq!(row_count(&pool).await, 2);
}

/// Arm 0's shape-bounds branch (empty / over the column length) is its own
/// recording site: invalid_endpoint, no transaction, no row.
#[sqlx::test(migrations = "../../migrations")]
async fn arm0_endpoint_bounds(pool: PgPool) {
    let svc = service(&pool);
    let too_long = format!(
        "{}://10.244.0.1:{TEST_GRPC_PORT}{}",
        test_grpc_scheme(),
        "0".repeat(256)
    );
    let cases = [
        (
            String::new(),
            test_grpc_endpoint(0, 1),
            "webtransport_endpoint is required",
        ),
        (test_wt_endpoint("a"), too_long, "grpc_endpoint is too long"),
    ];
    for (i, (wt, grpc, message)) in cases.iter().enumerate() {
        let snap = MetricAssertion::snapshot();
        let err = svc
            .register_mh(req(&format!("mh-{i}"), wt, grpc))
            .await
            .unwrap_err();
        assert_eq!(err.code(), Code::InvalidArgument, "{message}");
        assert_eq!(err.message(), *message);
        assert_outcome(&snap, MhRegistrationOutcome::InvalidEndpoint);
        snap.counter("gc_db_queries_total")
            .with_labels(&[("operation", "register_mh_txn")])
            .assert_unobserved();
    }
    assert_eq!(row_count(&pool).await, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn arm0_grpc_zone(pool: PgPool) {
    let svc = service(&pool);
    let wt = test_wt_endpoint("a");
    let scheme = test_grpc_scheme();
    let rejects: Vec<(String, &str)> = vec![
        // Cross-zone: the WT zone's address presented as a gRPC endpoint.
        (
            format!("{scheme}://{TEST_WT_EXACT_HOST}:{}", TEST_WT_PORTS[0]),
            "host",
        ),
        (format!("{scheme}://10.9.0.1:{TEST_GRPC_PORT}"), "host"),
        (format!("{scheme}://mh.mh.test:{TEST_GRPC_PORT}"), "host"),
        (format!("https://10.244.0.1:{TEST_GRPC_PORT}"), "scheme"),
        (
            format!("{scheme}://10.244.0.1:{}", TEST_GRPC_PORT + 1),
            "port",
        ),
        (format!("{scheme}://10.244.0.1:{TEST_GRPC_PORT}/"), "syntax"),
        (
            format!("{scheme}://[::ffff:10.244.0.1]:{TEST_GRPC_PORT}"),
            "syntax",
        ),
    ];
    for (i, (grpc, component)) in rejects.iter().enumerate() {
        let snap = MetricAssertion::snapshot();
        let err = svc
            .register_mh(req(&format!("mh-{i}"), &wt, grpc))
            .await
            .unwrap_err();
        assert_eq!(err.code(), Code::InvalidArgument, "{grpc}");
        assert_eq!(
            err.message(),
            format!("grpc_endpoint: {component} not allowed"),
            "{grpc}"
        );
        assert_outcome(&snap, MhRegistrationOutcome::InvalidEndpoint);
        snap.counter("gc_db_queries_total")
            .with_labels(&[("operation", "register_mh_txn")])
            .assert_unobserved();
    }
    assert_eq!(row_count(&pool).await, 0, "no row written by any reject");

    let snap = MetricAssertion::snapshot();
    svc.register_mh(req("mh-ok", &wt, &test_grpc_endpoint(0, 1)))
        .await
        .unwrap();
    assert_outcome(&snap, MhRegistrationOutcome::Ok);
}

// ---------------------------------------------------------------------------
// Arm 1 — endpoints fixed for an incarnation's life
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn arm1_endpoint_changed_leaves_row_unchanged(pool: PgPool) {
    let svc = service(&pool);
    let (wt, grpc) = (test_wt_endpoint("own"), test_grpc_endpoint(1, 1));
    // Healthy and aged, so a silent reset to pending/NOW() would show.
    seed(&pool, "mh-own", &wt, &grpc, "healthy", 1_000).await;
    // Another id holds the endpoint the changed request moves onto: arm 1
    // must win before arm 2 runs (no supersede).
    let (other_wt, other_grpc) = (test_wt_endpoint("other"), test_grpc_endpoint(1, 2));
    seed(
        &pool,
        "mh-other",
        &other_wt,
        &other_grpc,
        "unhealthy",
        1_000,
    )
    .await;
    let before = stored_row(&pool, "mh-own").await.unwrap();
    let other_before = stored_row(&pool, "mh-other").await.unwrap();

    for (changed_wt, changed_grpc, why) in [
        (other_wt.as_str(), grpc.as_str(), "webtransport only"),
        (wt.as_str(), other_grpc.as_str(), "grpc only"),
    ] {
        let snap = MetricAssertion::snapshot();
        let err = svc
            .register_mh(req("mh-own", changed_wt, changed_grpc))
            .await
            .unwrap_err();
        assert_eq!(err.code(), Code::FailedPrecondition, "{why}");
        assert_eq!(
            err.message(),
            "endpoint_changed",
            "{why}: no stored endpoint echoed"
        );
        assert_outcome(&snap, MhRegistrationOutcome::EndpointChanged);
        snap.counter("gc_db_queries_total")
            .with_labels(&[("operation", "register_mh_txn"), ("status", "success")])
            .assert_delta(1);
        assert_eq!(stored_row(&pool, "mh-own").await.unwrap(), before, "{why}");
        assert_eq!(
            stored_row(&pool, "mh-other").await.unwrap(),
            other_before,
            "{why}"
        );
    }

    // Positive arm: identical endpoints are accepted and reset to pending.
    let snap = MetricAssertion::snapshot();
    let resp = svc
        .register_mh(req("mh-own", &wt, &grpc))
        .await
        .unwrap()
        .into_inner();
    assert!(resp.accepted);
    assert_outcome(&snap, MhRegistrationOutcome::Ok);
    let after = stored_row(&pool, "mh-own").await.unwrap();
    assert_eq!((&after.0, &after.1), (&wt, &grpc));
    assert_eq!(after.2, "pending");
    assert!(after.3 > before.3, "heartbeat refreshed");
}

// ---------------------------------------------------------------------------
// Arm 2a — both endpoints match: supersede, even a fresh healthy row
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn arm2a_both_match_supersedes_fresh_row(pool: PgPool) {
    let svc = service(&pool);
    let (wt, grpc) = (test_wt_endpoint("slot0"), test_grpc_endpoint(2, 1));
    seed(&pool, "mh-old", &wt, &grpc, "healthy", 0).await;

    let snap = MetricAssertion::snapshot();
    let resp = svc
        .register_mh(req("mh-new", &wt, &grpc))
        .await
        .unwrap()
        .into_inner();
    assert!(resp.accepted);
    assert_outcome(
        &snap,
        MhRegistrationOutcome::Superseded(MhSupersedeMatch::Both),
    );
    assert!(
        stored_row(&pool, "mh-old").await.is_none(),
        "old row deleted"
    );
    assert_eq!(stored_row(&pool, "mh-new").await.unwrap().2, "pending");
    assert_eq!(row_count(&pool).await, 1);
}

// ---------------------------------------------------------------------------
// Arm 2b / 2c — one-sided match: supersedable vs fresh serving
// ---------------------------------------------------------------------------

/// Which endpoint the one-sided match is on (the SQL OR from both sides).
#[derive(Clone, Copy, Debug)]
enum Side {
    Wt,
    Grpc,
}

/// Seed the old row sharing ONE endpoint (`side`) with the new request, then
/// register; returns the result.
async fn one_sided(
    pool: &PgPool,
    side: Side,
    status: &str,
    age_ms: i64,
    tag: u8,
) -> Result<(), tonic::Status> {
    let shared_wt = test_wt_endpoint(&format!("shared{tag}"));
    let shared_grpc = test_grpc_endpoint(3, tag);
    let (old_wt, old_grpc) = match side {
        Side::Wt => (shared_wt.clone(), test_grpc_endpoint(4, tag)),
        Side::Grpc => (test_wt_endpoint(&format!("old{tag}")), shared_grpc.clone()),
    };
    seed(
        pool,
        &format!("old-{tag}"),
        &old_wt,
        &old_grpc,
        status,
        age_ms,
    )
    .await;
    service(pool)
        .register_mh(req(&format!("new-{tag}"), &shared_wt, &shared_grpc))
        .await
        .map(|_| ())
}

#[sqlx::test(migrations = "../../migrations")]
async fn arm2b_one_sided_supersedable_rows_are_superseded(pool: PgPool) {
    let stale = bound_ms() + 2_000;
    let cases = [
        (Side::Wt, "draining", 0, "draining (wt side)"),
        (Side::Grpc, "unhealthy", 0, "unhealthy (grpc side)"),
        (Side::Wt, "healthy", stale, "stale healthy (wt side)"),
        (Side::Grpc, "degraded", stale, "stale degraded (grpc side)"),
    ];
    for (tag, (side, status, age, why)) in cases.into_iter().enumerate() {
        let tag = u8::try_from(tag).unwrap();
        let snap = MetricAssertion::snapshot();
        one_sided(&pool, side, status, age, tag)
            .await
            .map_err(|e| format!("{why}: expected accept, got {e:?}"))
            .unwrap();
        assert_outcome(
            &snap,
            MhRegistrationOutcome::Superseded(MhSupersedeMatch::OneSided),
        );
        assert!(
            stored_row(&pool, &format!("old-{tag}")).await.is_none(),
            "{why}: old row deleted"
        );
        assert!(
            stored_row(&pool, &format!("new-{tag}")).await.is_some(),
            "{why}: new row present"
        );
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn arm2c_one_sided_fresh_serving_rows_reject(pool: PgPool) {
    let cases = [
        (Side::Wt, "pending", "pending (wt side)"),
        (Side::Grpc, "healthy", "healthy (grpc side)"),
        (Side::Wt, "degraded", "degraded (wt side)"),
    ];
    for (tag, (side, status, why)) in cases.into_iter().enumerate() {
        let tag = u8::try_from(tag).unwrap();
        let old_before = {
            let snap = MetricAssertion::snapshot();
            let err = one_sided(&pool, side, status, 0, tag).await.unwrap_err();
            assert_eq!(err.code(), Code::FailedPrecondition, "{why}");
            assert_eq!(
                err.message(),
                "address_conflict",
                "{why}: no id/endpoint echoed"
            );
            assert_outcome(&snap, MhRegistrationOutcome::AddressConflict);
            // The fixture seed's transaction + the rejected call's: a rule
            // reject records `success` (the queries ran; the rule refused).
            snap.counter("gc_db_queries_total")
                .with_labels(&[("operation", "register_mh_txn"), ("status", "success")])
                .assert_delta(2);
            snap.counter("gc_db_queries_total")
                .with_labels(&[("operation", "register_mh_txn"), ("status", "error")])
                .assert_unobserved();
            stored_row(&pool, &format!("old-{tag}")).await
        };
        assert!(old_before.is_some(), "{why}: old row must survive");
        let old = old_before.unwrap();
        assert_eq!(old.2, status, "{why}: old row untouched");
        assert!(
            stored_row(&pool, &format!("new-{tag}")).await.is_none(),
            "{why}: no write for the rejected id"
        );
    }
}

/// The same one-sided shape, fresh vs stale, near the millisecond bound
/// (B = 3750 ms): -3200 ms is fresh (a bound truncated to 3 s would call it
/// stale), -3900 ms is stale (a bound rounded up to 4 s would call it fresh).
/// A do-nothing implementation fails one of the two.
#[sqlx::test(migrations = "../../migrations")]
async fn fresh_vs_stale_same_shape_near_bound(pool: PgPool) {
    assert_eq!(bound_ms(), 3_750, "fixture bound");

    let snap = MetricAssertion::snapshot();
    // Stale first; the fresh row is seeded immediately before its call.
    one_sided(&pool, Side::Wt, "healthy", 3_900, 1)
        .await
        .expect("stale -> superseded");
    assert_outcome(
        &snap,
        MhRegistrationOutcome::Superseded(MhSupersedeMatch::OneSided),
    );

    let snap = MetricAssertion::snapshot();
    let err = one_sided(&pool, Side::Wt, "healthy", 3_200, 2)
        .await
        .unwrap_err();
    assert_eq!(err.message(), "address_conflict");
    assert_outcome(&snap, MhRegistrationOutcome::AddressConflict);
}

/// A new row overlapping TWO existing rows (WT matches X, gRPC matches Y):
/// both supersedable -> both deleted; either fresh serving -> reject, no
/// writes. The case the registry-wide lock exists for.
#[sqlx::test(migrations = "../../migrations")]
async fn new_row_overlapping_two_rows(pool: PgPool) {
    let svc = service(&pool);
    let stale = bound_ms() + 2_000;
    let (wt, grpc) = (test_wt_endpoint("two"), test_grpc_endpoint(5, 1));

    // Reject: X stale, Y fresh healthy.
    seed(&pool, "x", &wt, &test_grpc_endpoint(5, 2), "healthy", stale).await;
    seed(&pool, "y", &test_wt_endpoint("y"), &grpc, "healthy", 0).await;
    let snap = MetricAssertion::snapshot();
    let err = svc.register_mh(req("n", &wt, &grpc)).await.unwrap_err();
    assert_eq!(err.message(), "address_conflict");
    assert_outcome(&snap, MhRegistrationOutcome::AddressConflict);
    assert!(stored_row(&pool, "x").await.is_some(), "no partial delete");
    assert!(stored_row(&pool, "y").await.is_some());
    assert!(stored_row(&pool, "n").await.is_none());

    // Accept: Y now draining -> both superseded in one registration.
    sqlx::query("UPDATE media_handlers SET health_status = 'draining' WHERE handler_id = 'y'")
        .execute(&pool)
        .await
        .unwrap();
    let snap = MetricAssertion::snapshot();
    svc.register_mh(req("n", &wt, &grpc)).await.unwrap();
    // ONE increment per registration, not per deleted row.
    assert_outcome(
        &snap,
        MhRegistrationOutcome::Superseded(MhSupersedeMatch::OneSided),
    );
    assert!(stored_row(&pool, "x").await.is_none());
    assert!(stored_row(&pool, "y").await.is_none());
    assert_eq!(row_count(&pool).await, 1);
}

/// Pre-rule validation failures record no registration outcome.
#[sqlx::test(migrations = "../../migrations")]
async fn pre_rule_validation_records_no_outcome(pool: PgPool) {
    let snap = MetricAssertion::snapshot();
    let err = service(&pool)
        .register_mh(req(
            "bad id",
            &test_wt_endpoint("a"),
            &test_grpc_endpoint(0, 1),
        ))
        .await
        .unwrap_err();
    assert_eq!(err.code(), Code::InvalidArgument);
    assert_no_outcome(&snap);
}

// ---------------------------------------------------------------------------
// Concurrency — deterministic overlap through the registry lock
// ---------------------------------------------------------------------------

/// Hold the registry lock on a dedicated session, start two overlapping
/// registrations, wait until BOTH are queued on the lock (positive control
/// that the transaction takes it: without the lock statement they would not
/// wait), release, join, and check the invariant and the outcomes.
async fn race(
    pool: &PgPool,
    a: (&str, &str, &str),
    b: (&str, &str, &str),
) -> Vec<Result<(), Code>> {
    let mut holder = pool.acquire().await.unwrap();
    sqlx::query("SELECT pg_advisory_lock(hashtext($1))")
        .bind(MH_REGISTRY_LOCK_KEY)
        .execute(&mut *holder)
        .await
        .unwrap();

    let spawn = |(id, wt, grpc): (&str, &str, &str)| {
        let svc = service(pool);
        let r = req(id, wt, grpc);
        tokio::spawn(async move { svc.register_mh(r).await.map(|_| ()).map_err(|e| e.code()) })
    };
    let (ta, tb) = (spawn(a), spawn(b));

    // Failure-only deadline: the loop exits as soon as both wait.
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let (waiting,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM pg_locks l JOIN pg_database d ON d.oid = l.database \
             WHERE d.datname = current_database() AND l.locktype = 'advisory' AND NOT l.granted",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        if waiting == 2 {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "both registrations must queue on the registry lock (waiting={waiting})"
        );
        tokio::task::yield_now().await;
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    sqlx::query("SELECT pg_advisory_unlock(hashtext($1))")
        .bind(MH_REGISTRY_LOCK_KEY)
        .execute(&mut *holder)
        .await
        .unwrap();
    drop(holder);
    vec![ta.await.unwrap(), tb.await.unwrap()]
}

async fn assert_unique_endpoints(pool: &PgPool) {
    for (col, sql) in [
        (
            "webtransport_endpoint",
            "SELECT webtransport_endpoint FROM media_handlers GROUP BY 1 HAVING COUNT(*) > 1",
        ),
        (
            "grpc_endpoint",
            "SELECT grpc_endpoint FROM media_handlers GROUP BY 1 HAVING COUNT(*) > 1",
        ),
    ] {
        let dup: Vec<(String,)> = sqlx::query_as(sql).fetch_all(pool).await.unwrap();
        assert!(dup.is_empty(), "two rows share a {col}: {dup:?}");
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_one_sided_overlap_admits_exactly_one(pool: PgPool) {
    let shared = test_wt_endpoint("race");
    let (g1, g2) = (test_grpc_endpoint(6, 1), test_grpc_endpoint(6, 2));
    let snap = MetricAssertion::snapshot();
    let results = race(&pool, ("ra", &shared, &g1), ("rb", &shared, &g2)).await;

    // The loser sees a FRESH pending row sharing one endpoint: arm 2c.
    let mut sorted = results.clone();
    sorted.sort_by_key(|r| r.is_err());
    assert_eq!(
        sorted,
        vec![Ok(()), Err(Code::FailedPrecondition)],
        "{results:?}"
    );
    assert_eq!(row_count(&pool).await, 1);
    assert_unique_endpoints(&pool).await;
    snap.counter("gc_mh_registrations_total")
        .with_labels(&[("outcome", "ok")])
        .assert_delta(1);
    snap.counter("gc_mh_registrations_total")
        .with_labels(&[("outcome", "address_conflict")])
        .assert_delta(1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_identical_endpoints_supersede(pool: PgPool) {
    let (wt, grpc) = (test_wt_endpoint("race2"), test_grpc_endpoint(7, 1));
    let snap = MetricAssertion::snapshot();
    let results = race(&pool, ("ra", &wt, &grpc), ("rb", &wt, &grpc)).await;

    // The second through the lock matches both endpoints: arm 2a.
    assert_eq!(results, vec![Ok(()), Ok(())]);
    assert_eq!(row_count(&pool).await, 1);
    assert_unique_endpoints(&pool).await;
    snap.counter("gc_mh_registrations_total")
        .with_labels(&[("outcome", "ok")])
        .assert_delta(1);
    snap.counter("gc_mh_registrations_total")
        .with_labels(&[("outcome", "superseded"), ("match", "both")])
        .assert_delta(1);
}
