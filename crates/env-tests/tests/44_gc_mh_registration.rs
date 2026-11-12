//! R-25 (story 4 GC-2): GC's RegisterMH endpoint rule, enforced against the
//! RENDERED Kind allowlists, accepts a real MH on a real slot address.
//!
//! # Why this test starts its own MH
//!
//! GC does not re-validate existing registry rows, and an MH registers only at
//! start (or on a NOT_FOUND heartbeat). A GC-only roll does not restart the
//! baseline MHs: `mh.sh upgrade`'s revision excludes `gc-service-config`. So
//! after a GC-only deploy, every baseline MH still holds a row from the
//! previous GC, and "zero invalid_endpoint" would pass on zero-initialised
//! counters with nothing ever having been decided. The test therefore causes
//! the registration itself: it brings up one run-owned MH (`mh.sh up --owner
//! <run token>`) on the next-fit slot. That MH advertises the slot ConfigMap's
//! WebTransport address and its pod-IP gRPC address, the same values every
//! baseline MH uses.
//!
//! # Evidence
//!
//! - **Per-MH.** MH sets `/ready` only after `register()` returns Ok
//!   (`crates/mh-service/src/grpc/gc_task.rs`, `run`), and `up` returns only
//!   when the Deployment is Available (contract §5). So a returned `up` means
//!   a current-generation GC ACCEPTED that MH's endpoints. A rejected MH retries
//!   forever and never goes Ready. `up` then fails with
//!   `not-available-timeout`, and this test names the GC reject counters in
//!   its failure.
//! - **Counter.** Across the live GC replicas, the delta of
//!   `gc_mh_registrations_total{outcome=~"ok|superseded"}` is exactly 1. The
//!   sum across replicas is correct for a counter EVENT: each registration
//!   lands on exactly one replica. `superseded` is accepted because the slot
//!   may still hold a previous occupant's row (sequential reuse, ADR-0039 §7
//!   arms 2a/2b). The baseline is settled across a scrape.
//! - **No rejects.** On EVERY live replica,
//!   `gc_mh_registrations_total{outcome="invalid_endpoint"}` and
//!   `{outcome="address_conflict"}` are present (zero-initialised at GC start,
//!   which is the presence control) and 0, before and after.
//! - **Replica set stable.** Counters reset when a GC pod restarts, so the
//!   serving GC pod set must be identical before and after
//!   (`gc-replica-set-changed`).
//!
//! # Cleanup
//!
//! Teardown is `mh.sh drain --all --owner <run token>` on EVERY exit path
//! (the body runs in its own task, so a panic is caught). It needs no slot
//! number from any output (the run token is the per-run org subdomain,
//! `MhPool::for_run`), because `up`'s not-available-timeout deliberately
//! leaves its pod in place. A failed teardown fails the test even when the body
//! passed, naming the REASON and the reclaim command. After a successful
//! drain, teardown also waits until no GC replica still counts the removed MH
//! as selectable (`removed-mh-still-selectable`;
//! `gc_fleet::wait_removed_mh_unselectable`). Without that wait, the next
//! suite's meeting could be placed on it inside the staleness bound.
//!
//! # Reason tokens
//!
//! `mh-no-free-slot` (PRECONDITION: B+1 > K), `gc-registration-presence`,
//! `gc-registration-rejects`, `mh-not-registered` (`up` timed out),
//! `gc-registration-not-counted`, `gc-registration-count`,
//! `gc-replica-set-changed`, `removed-mh-still-selectable`, `mh-sh-up` (any other `up` failure, verbatim),
//! `mh-teardown-failed`.
//!
//! OWN TEST BINARY, ONE test fn: it changes the MH pool, and cargo runs test
//! binaries one at a time, so no other suite sees the extra MH.
//!
//! Edge-sum (concurrent Σ P×(N+M)): 0

#![cfg(feature = "flows")]

use std::sync::Arc;
use std::time::{Duration, Instant};

use env_tests::eventual::ConsistencyCategory;
use env_tests::fixtures::deployment_config::LivePod;
use env_tests::fixtures::gc_fleet::live_gc_pods;
use env_tests::fixtures::metrics::{
    format_instance_map, instance_for_pod_ip, poll_until_pinned_instance, settled_baseline,
    BASELINE_SETTLE_BOUND, PINNED_METRIC_BOUND,
};
use env_tests::fixtures::mh_pool::{MhPool, REASON_NOT_AVAILABLE, REASON_NO_FREE_SLOT};
use env_tests::fixtures::PrometheusClient;
use serial_test::serial;

const ACCEPTED_PROMQL: &str =
    r#"sum by (instance) (gc_mh_registrations_total{outcome=~"ok|superseded"})"#;
const INVALID_PROMQL: &str =
    r#"sum by (instance) (gc_mh_registrations_total{outcome="invalid_endpoint"})"#;
const CONFLICT_PROMQL: &str =
    r#"sum by (instance) (gc_mh_registrations_total{outcome="address_conflict"})"#;

const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// `#[serial]`: this binary mutates the MH pool (`crate::resilience_hygiene`
/// requires every test in a pool-mutating binary to be serialized).
#[tokio::test]
#[serial]
async fn test_new_mh_on_a_real_slot_registers_with_no_gc_reject() {
    // The shared leg scaffold: baseline before, the body in its own task, the
    // asserted owner teardown + GC settle on EVERY path (so a removed MH is
    // never handed to the next suite's meetings), baseline after.
    MhPool::run_leg(|owner, prom_url| async move { body(&prom_url, owner).await }).await;
}

async fn body(prom_url: &str, owner: Arc<MhPool>) {
    let prom = PrometheusClient::new(prom_url);

    let gc = live_gc_pods().await;
    assert!(
        !gc.serving.is_empty(),
        "PRECONDITION: zero serving GC pods (live_gc_pods should have refused this)"
    );
    let gc_before = gc.names();

    // Presence + zero rejects BEFORE: a non-zero reject count from before this
    // test is already a failure of the deployed rule ("the cluster shows zero").
    for pod in &gc.serving {
        for (promql, outcome) in [
            (INVALID_PROMQL, "invalid_endpoint"),
            (CONFLICT_PROMQL, "address_conflict"),
            (ACCEPTED_PROMQL, "ok|superseded"),
        ] {
            let v = present_on(&prom, promql, pod, outcome).await;
            if outcome != "ok|superseded" {
                assert_no_rejects(v, pod, outcome, "before this test's MH");
            }
        }
    }

    let baseline = settled_baseline(
        &prom,
        ACCEPTED_PROMQL,
        BASELINE_SETTLE_BOUND,
        "the RegisterMH rule under test",
    )
    .await;
    let accepted_before = sum_over(&baseline, &gc.serving, ACCEPTED_PROMQL);

    // `up` blocks for mh.sh's own derived bound (no test-side timeout); keep
    // it off the runtime thread.
    let up = tokio::task::spawn_blocking(move || owner.up())
        .await
        .expect("mh.sh up task");

    let entry = match up {
        Ok(entry) => entry,
        Err(r) if r.reason == REASON_NO_FREE_SLOT => panic!(
            "PRECONDITION (mh-no-free-slot): no free MH slot for this test's MH: {r}. It needs \
             baseline B + 1 <= K slots; check `mh.sh list` for leaked run-owned MHs."
        ),
        Err(r) if r.reason == REASON_NOT_AVAILABLE => {
            let invalid = sum_over(
                &prom.instance_counter_map(INVALID_PROMQL).await,
                &gc.serving,
                INVALID_PROMQL,
            );
            let conflict = sum_over(
                &prom.instance_counter_map(CONFLICT_PROMQL).await,
                &gc.serving,
                CONFLICT_PROMQL,
            );
            // Both counts were asserted 0 before `up`, so each value IS its delta.
            let cause = if invalid > 0.0 || conflict > 0.0 {
                format!(
                    "gc invalid_endpoint +{invalid} (non-zero: the rendered allowlist does not \
                     admit this slot's address), address_conflict +{conflict} (non-zero: a fresh \
                     live row holds one of this MH's endpoints)"
                )
            } else {
                "no GC reject observed (the MH did not become Ready for another reason: check \
                 its logs)"
                    .to_string()
            };
            panic!(
                "R25 (mh-not-registered): {REASON_NOT_AVAILABLE} on slot {:?}; {cause}. {r}",
                r.slot
            );
        }
        Err(r) => panic!("PRECONDITION (mh-sh-up): mh.sh up failed: {r}"),
    };
    eprintln!(
        "44: run-owned MH up on slot {} ({}, pod {:?})",
        entry.slot, entry.address, entry.pod
    );

    // The accepted registration happened before `up` returned; wait only for
    // the scrape.
    let deadline = Instant::now() + PINNED_METRIC_BOUND;
    let (accepted_after, last_map) = loop {
        let map = prom.instance_counter_map(ACCEPTED_PROMQL).await;
        let now = sum_over(&map, &gc.serving, ACCEPTED_PROMQL);
        if now - accepted_before >= 1.0 || Instant::now() > deadline {
            break (now, map);
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    };

    // Counters reset when a GC pod restarts, so a changed replica set voids every
    // delta above. Check it before judging them.
    let gc_after = live_gc_pods().await.names();
    assert_eq!(
        gc_before, gc_after,
        "R25 (gc-replica-set-changed): the serving GC pod set changed during the test, so the \
         per-replica counters may have reset"
    );

    let delta = accepted_after - accepted_before;
    assert!(
        delta >= 1.0,
        "R25 (gc-registration-not-counted): the MH on slot {} went Ready (GC accepted it), but \
         no live GC replica counted ok|superseded within {PINNED_METRIC_BOUND:?}: before \
         {accepted_before}, after {accepted_after}; instances {}",
        entry.slot,
        format_instance_map(&last_map)
    );
    #[expect(
        clippy::float_cmp,
        reason = "counter sums of small integers are exact in f64"
    )]
    let exactly_one = delta == 1.0;
    assert!(
        exactly_one,
        "R25 (gc-registration-count): one MH start must be counted exactly once across GC \
         replicas, got +{delta} (before {accepted_before}, after {accepted_after}; instances {})",
        format_instance_map(&last_map)
    );

    // No rejects AFTER, on every replica. The same scrape that carried the
    // accepted delta carries these counters.
    for pod in &gc.serving {
        for (promql, outcome) in [
            (INVALID_PROMQL, "invalid_endpoint"),
            (CONFLICT_PROMQL, "address_conflict"),
        ] {
            let v = present_on(&prom, promql, pod, outcome).await;
            assert_no_rejects(v, pod, outcome, "after this test's MH registered");
        }
    }
}

/// The pinned value of `promql` on `pod`, waiting only for the series to be
/// scraped. GC zero-initialises every outcome at start, so ABSENCE is a
/// presence failure (scrape, or the series is not registered), never a zero.
async fn present_on(prom: &PrometheusClient, promql: &str, pod: &LivePod, outcome: &str) -> f64 {
    poll_until_pinned_instance(
        prom,
        promql,
        &pod.ip,
        ConsistencyCategory::MetricsScrape.timeout(),
        POLL_INTERVAL,
        &format!(
            "R25 (gc-registration-presence) pod {}: gc_mh_registrations_total{{outcome={outcome}}} \
             is zero-initialised at GC start, so absence is a scrape/startup or registration \
             problem, NOT a reject count",
            pod.name
        ),
        "present",
        |_| true,
    )
    .await
}

fn assert_no_rejects(value: f64, pod: &LivePod, outcome: &str, when: &str) {
    assert!(
        value == 0.0,
        "R25 (gc-registration-rejects): GC pod {} counts {outcome}={value} {when}. The rendered \
         Kind allowlists or the live address pool disagree with GC's RegisterMH rule (see GC's \
         `gc.grpc.mh_service` warn lines for the field and component)",
        pod.name
    );
}

/// Σ over the LIVE GC pods of each one's pinned value. A live pod with no
/// series is a presence failure (already checked), never a zero.
fn sum_over(
    map: &env_tests::fixtures::metrics::InstanceCounters,
    pods: &[LivePod],
    promql: &str,
) -> f64 {
    pods.iter()
        .map(|p| {
            instance_for_pod_ip(map, &p.ip)
                .map(|(_, v)| v)
                .unwrap_or_else(|| {
                    panic!(
                        "R25 (gc-registration-presence): `{promql}` has no series for GC pod {} \
                         ({}); instances {}",
                        p.name,
                        p.ip,
                        format_instance_map(map)
                    )
                })
        })
        .sum()
}
