//! R-27 publish leg (story 4; obligation inherited from gate zero, I-1): UDP
//! from the env-test RUNNER to the host gateway on each serving slot's Kind
//! UDP extraPortMapping is delivered end to end to THAT slot's MH.
//!
//! Gate zero (`41_gate_zero_hostport_policy.rs`) proved the hostPort DNAT +
//! policy leg node-direct; the host-publish leg (host gateway -> Kind node,
//! podman/WSL2 port publishing) was unproven until this test.
//!
//! # Evidence
//!
//! A bare datagram to MH is not observable, so the probe is a WebTransport
//! (QUIC) connect to the slot's `address` — exactly what a browser dials, read
//! from `mh.sh list --json` (whose address is the slot ConfigMap's; R-28,
//! `42_mh_address_invariant.rs`, proves the MH process advertises that value):
//!
//! - the CLIENT completing the handshake proves both directions of UDP;
//! - MH-side, `mh_webtransport_connections_total{status="accepted"}` (counted at
//!   endpoint accept, before auth: `crates/mh-service/src/webtransport/server.rs`)
//!   rising on `instance=<that slot's pod IP>:8083` proves the datagrams reached
//!   THAT slot's MH process — per-entity evidence, never a shared gauge;
//! - every OTHER serving MH's counter not moving across the probe proves no
//!   crossed mapping (template vs table drift) delivered it elsewhere.
//!
//! OWN TEST BINARY, ONE test fn, so nothing in this binary connects to an MH
//! concurrently (the not-moved check depends on it; cargo runs test binaries
//! one at a time).
//!
//! # Verdicts
//!
//! `R27 slot-design-failure` (handshake failed, or the MH never counted it, or
//! another MH did): the hostPort slot design does not hold on this substrate.
//! ESCALATE — do not work around it (no node-direct fallback, no retry beyond
//! the scrape-convergence poll). `R27 not-host-published`: the slot address is
//! the node or pod IP, so this run would not exercise the publish leg
//! (PRECONDITION). Unoccupied slots' mappings are covered only by the helper's
//! template == table drift test, not by delivery (residual).
//!
//! Edge-sum (concurrent Σ P×(N+M)): 0

#![cfg(feature = "flows")]

use std::collections::BTreeMap;
use std::time::Duration;

use env_tests::cluster::ClusterConnection;
use env_tests::fixtures::kube::{settled_serving_pods, MH_POD_SETTLE, MH_SELECTOR};
use env_tests::fixtures::mc_session::try_connect_wt;
use env_tests::fixtures::metrics::{
    format_instance_map, instance_for_pod_ip, poll_until_pinned_instance,
    service_job_scrape_settle, settled_baseline, BASELINE_SETTLE_BOUND, PINNED_METRIC_BOUND,
};
use env_tests::fixtures::mh_address::address_authority;
use env_tests::fixtures::mh_pool;
use env_tests::fixtures::PrometheusClient;
use env_tests::NAMESPACE;
use serde_json::Value;

const ACCEPTED_PROMQL: &str =
    r#"sum by (instance) (mh_webtransport_connections_total{status="accepted"})"#;

/// Bound on one client handshake (a lost Initial is retransmitted well inside it).
const HANDSHAKE_BOUND: Duration = Duration::from_secs(15);

/// One serving slot to probe: its address and the pod (kubectl-resolved IP)
/// that must count the connection.
struct Target {
    slot: u32,
    address: String,
    pod: String,
    pod_ip: String,
}

fn value_on(map: &env_tests::fixtures::metrics::InstanceCounters, pod_ip: &str) -> f64 {
    // An absent series is a counter never incremented on that pod: zero.
    instance_for_pod_ip(map, pod_ip).map_or(0.0, |(_, v)| v)
}

#[tokio::test]
async fn udp_to_each_serving_slot_reaches_that_slots_mh() {
    let (doc, _) =
        settled_serving_pods(MH_SELECTOR, MH_POD_SETTLE.timeout(), "mh-pods-unserving").await;
    let pool = mh_pool::list();
    let serving = pool.serving();
    assert!(
        pool.baseline >= 1 && serving.len() >= pool.baseline as usize,
        "PRECONDITION: {} serving slot(s) for baseline {}; nothing (or too little) to probe",
        serving.len(),
        pool.baseline
    );

    // Node and pod IPs by pod name, from kubectl (not mh.sh).
    let ips: BTreeMap<String, (String, String)> = doc
        .get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|p| {
            Some((
                p.pointer("/metadata/name")?.as_str()?.to_string(),
                (
                    p.pointer("/status/hostIP")?.as_str()?.to_string(),
                    p.pointer("/status/podIP")?.as_str()?.to_string(),
                ),
            ))
        })
        .collect();
    let targets: Vec<Target> = serving
        .iter()
        .map(|s| {
            let pod = s.pod.clone().expect("serving slots have a pod");
            let (host_ip, pod_ip) = ips.get(&pod).cloned().unwrap_or_else(|| {
                panic!("PRECONDITION: mh.sh lists pod {pod} (slot {}) but kubectl -n {NAMESPACE} -l {MH_SELECTOR} does not", s.slot)
            });
            assert_eq!(
                s.pod_ip.as_deref(),
                Some(pod_ip.as_str()),
                "PRECONDITION: mh.sh and kubectl disagree on pod {pod}'s IP"
            );
            let (host, _) = address_authority(&s.address).unwrap_or_else(|e| {
                panic!("PRECONDITION: slot {}'s address is not dialable: {e}", s.slot)
            });
            assert!(
                host != host_ip && host != pod_ip,
                "R27 not-host-published: slot {}'s address {} dials the node ({host_ip}) or pod \
                 ({pod_ip}) directly, so this run would not exercise the Kind host-publish leg",
                s.slot,
                s.address
            );
            Target {
                slot: s.slot,
                address: s.address.clone(),
                pod,
                pod_ip,
            }
        })
        .collect();

    let cluster = ClusterConnection::new()
        .await
        .expect("Failed to connect to cluster - ensure port-forwards are running");
    let prom = PrometheusClient::new(&cluster.prometheus_base_url);

    for Target {
        slot,
        address,
        pod,
        pod_ip,
    } in &targets
    {
        let baseline = settled_baseline(
            &prom,
            ACCEPTED_PROMQL,
            BASELINE_SETTLE_BOUND,
            "the R-27 probe",
        )
        .await;
        let before = value_on(&baseline, pod_ip);

        let conn = match tokio::time::timeout(HANDSHAKE_BOUND, try_connect_wt(address)).await {
            Ok(Ok(c)) => c,
            Ok(Err(e)) => panic!(
                "R27 slot-design-failure: slot {slot} ({address}, pod {pod}): the client \
                 handshake failed: {e}. UDP from the runner to the host gateway on this slot's \
                 extraPortMapping did not complete a round trip. ESCALATE; do not work around."
            ),
            Err(_) => panic!(
                "R27 slot-design-failure: slot {slot} ({address}, pod {pod}): no handshake \
                 within {HANDSHAKE_BOUND:?}. ESCALATE; do not work around."
            ),
        };

        poll_until_pinned_instance(
            &prom,
            ACCEPTED_PROMQL,
            pod_ip,
            PINNED_METRIC_BOUND,
            Duration::from_secs(2),
            &format!(
                "R27 slot-design-failure: slot {slot} ({address}): the handshake completed but \
                 pod {pod} never counted an accepted connection — the datagrams reached some \
                 OTHER listener. ESCALATE"
            ),
            &format!("> {before}"),
            |v| v > before,
        )
        .await;
        drop(conn);

        // Nobody else counted it: one scrape later, every other MH is unmoved.
        tokio::time::sleep(service_job_scrape_settle(&prom).await).await;
        let after = prom.instance_counter_map(ACCEPTED_PROMQL).await;
        for other in targets.iter().filter(|t| t.slot != *slot) {
            let (b, a) = (
                value_on(&baseline, &other.pod_ip),
                value_on(&after, &other.pod_ip),
            );
            assert!(
                a <= b,
                "R27 slot-design-failure: probing slot {slot} ({address}) also moved slot \
                 {}'s MH (pod {}) accepted count {b} -> {a}: a crossed \
                 mapping. baseline {} / after {}. ESCALATE",
                other.slot,
                other.pod,
                format_instance_map(&baseline),
                format_instance_map(&after)
            );
        }
    }
    println!(
        "R27 PASS: host-published UDP delivered to each of {} serving slot(s)' own MH: {:?}",
        targets.len(),
        targets
            .iter()
            .map(|t| (t.slot, &t.address))
            .collect::<Vec<_>>()
    );
}
