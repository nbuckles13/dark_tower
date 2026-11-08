//! R-28 (story 4; ADR-0039 §7 "Kind"): every live MH pod's client-facing
//! address is UNIQUE and is ITS SLOT's, on the live cluster.
//!
//! The pure verdict, its evidence model (why `mh.sh list` is only an
//! enumeration, never the evidence) and every violation token live in
//! `env_tests::fixtures::mh_address`, unit-tested at Layer 4 against adverse
//! inputs. This file only gathers the live inputs:
//!
//! - the pool, `mh.sh list --json` (path from `DT_MH_SH`, exported by
//!   `scripts/layer7.sh`; unset is a loud PRECONDITION);
//! - every non-terminating `app=mh-service` pod (kubectl), after the pool has
//!   settled (every MH pod Ready, none Terminating);
//! - the slot ConfigMaps, by label (kubectl, not mh.sh);
//! - each pod's current-container log, for the address the PROCESS advertises;
//! - `networkpolicy/mh-service`'s podSelector (security S1: the policy governs
//!   every pool pod).
//!
//! Whether each address actually ROUTES to its pod is R-27
//! (`43_mh_slot_publish_udp.rs`). Steady state only: the invariant during
//! upgrade / drain / crash restart is T-1's resilience legs.
//!
//! # Residual
//!
//! The process-observed address comes from the MH startup log line. On a pod
//! that has lived through enough log volume for kubelet to rotate that line
//! away, this fails with `startup-line-missing` — loudly, and distinct from an
//! address mismatch. Remedy: re-incarnate the pool (`mh.sh upgrade` after a
//! template change, or drain + up a slot). MH publishes its address nowhere
//! else a black-box test can read.
//!
//! # Diagnostic output policy (PII)
//!
//! Pod names, slot numbers, ConfigMap names, addresses (dev-cluster host
//! gateway IP:port, not personal data), label key=value pairs. Never a log
//! line, never a raw kubectl document.
//!
//! Edge-sum (concurrent Σ P×(N+M)): 0

#![cfg(feature = "smoke")]

use env_tests::fixtures::kube::{
    container_log, kubectl_get_json, settled_serving_pods, MH_POD_SETTLE, MH_SELECTOR,
};
use env_tests::fixtures::mh_address::{
    advertised_in_log, check_address_invariant, policy_match_labels, slot_configmaps, ObservedPod,
    MH_CONTAINER, SLOT_LABEL,
};
use env_tests::fixtures::mh_pool;
use env_tests::NAMESPACE;
use serde_json::Value;

const MH_POLICY: &str = "mh-service";

#[tokio::test]
async fn every_live_mh_pod_advertises_its_own_unique_slot_address() {
    // Settle first (every MH pod Ready, none Terminating), then read the pool:
    // the invariant is a steady-state claim here.
    let (doc, serving) =
        settled_serving_pods(MH_SELECTOR, MH_POD_SETTLE.timeout(), "mh-pods-unserving").await;
    let pool = mh_pool::list();

    let pods: Vec<Value> = doc
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    // Positive controls: a pool exists, and at least the baseline is running.
    assert!(
        pool.baseline >= 1,
        "PRECONDITION: mh.sh list reports baseline {} — nothing would be checked",
        pool.baseline
    );
    assert!(
        pods.len() >= pool.baseline as usize && serving.len() == pods.len(),
        "PRECONDITION: {} MH pod(s) ({} serving) for baseline {}: deploy.sh tops the pool up \
         to the baseline; inspect `$DT_MH_SH list`",
        pods.len(),
        serving.len(),
        pool.baseline
    );

    let cms = slot_configmaps(
        &kubectl_get_json(&["configmaps", "-n", NAMESPACE, "-l", SLOT_LABEL])
            .unwrap_or_else(|e| panic!("PRECONDITION: {e}")),
    )
    .unwrap_or_else(|e| panic!("PRECONDITION (slot-configmaps): {e}"));
    let policy_labels = policy_match_labels(
        &kubectl_get_json(&["networkpolicy", MH_POLICY, "-n", NAMESPACE])
            .unwrap_or_else(|e| panic!("PRECONDITION: {e}")),
    )
    .unwrap_or_else(|e| panic!("PRECONDITION (policy-selector): {e}"));

    let observed: Vec<ObservedPod> = pods
        .into_iter()
        .map(|pod| {
            let name = pod
                .pointer("/metadata/name")
                .and_then(Value::as_str)
                .expect("settled pods are named")
                .to_string();
            let log = container_log(&name, MH_CONTAINER)
                .unwrap_or_else(|e| panic!("PRECONDITION (mh-log): {e}"));
            let logged_address =
                advertised_in_log(&log).unwrap_or_else(|e| panic!("R28 {e} (pod {name})"));
            ObservedPod {
                pod,
                logged_address,
            }
        })
        .collect();

    let (violations, checked) = check_address_invariant(&pool, &observed, &cms, &policy_labels);
    assert!(
        violations.is_empty(),
        "R-28 address invariant violated ({} slot ConfigMaps, {} MH pods):\n  {}",
        cms.len(),
        observed.len(),
        violations.join("\n  ")
    );
    // Positive control: every pod went through every per-pod check.
    assert_eq!(
        checked,
        observed.len(),
        "only {checked} of {} MH pods were fully checked",
        observed.len()
    );
    println!(
        "R28 PASS: {} MH pod(s) on {} slot(s), each advertising its own slot address",
        observed.len(),
        cms.len()
    );
}
