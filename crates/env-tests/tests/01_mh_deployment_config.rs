//! P0 Smoke Tests: MH Deployment Configuration
//!
//! Story Test-Plan item F (validates R-21): the deployed
//! `MH_TERMINATION_GRACE_SECONDS` equals the pod spec's
//! `terminationGracePeriodSeconds`.
//!
//! # What this asserts, and why it is not a tautology
//!
//! Both sides come off **one live pod object** fetched from the API server:
//!
//! - side (a) `.spec.containers[name=mh-service].env[name=MH_TERMINATION_GRACE_SECONDS].value`
//! - side (b) `.spec.terminationGracePeriodSeconds` **of that same pod**
//!
//! Nothing here runs `kustomize`. The two sides are equal only if the kustomize
//! `replacements:` block in `infra/services/mh-service/kustomization.yaml` was
//! written correctly, survived the Kind overlay build, *and* was applied to the
//! cluster. That is the "does it apply" half; the static half (is the key
//! declared at all) belongs to `dt-guard env-config`, which reads base YAML and
//! never runs kustomize.
//!
//! # Honest limitation — read this before citing the test as stronger than it is
//!
//! The strongest possible reading would be the MH **process's own** environment
//! (`printenv`, `/proc/1/environ`). That is not available: the MH image is
//! distroless with `readOnlyRootFilesystem: true` and no shell, so
//! `kubectl exec` has no binary to run. The pod spec **as stored by the API
//! server** is the strongest reading available. It is still the *deployed
//! artifact* rather than a local render, but it is not proof that the container
//! process observed the value.
//!
//! # Why `-o json` + `serde_json`, not `-o jsonpath`
//!
//! The filter expression needed here (`env[?(@.name=='...')]`) is
//! quoting-fragile in a shell argv and returns an **empty string** on a typo
//! rather than failing. An empty probe compared against an empty probe reads as
//! "equal", so a typo would make this test pass vacuously — the precise failure
//! mode it exists to prevent. Parsing structured output makes an absent field a
//! distinguishable, loud condition.
//!
//! # Why `-n dark-tower`, not `-n default`
//!
//! Every Dark Tower workload runs in `dark-tower`. Nothing runs in `default`.
//!
//! # Diagnostic output policy (PII)
//!
//! Failure messages may print ONLY: pod name, namespace, label selector, env
//! var **names**, the `MH_TERMINATION_GRACE_SECONDS` value, and
//! `.spec.terminationGracePeriodSeconds`. Never the parsed `serde_json::Value`,
//! never the env array itself, never raw `kubectl` stdout. This is safe *today*
//! only because MH's sole credential (`MH_CLIENT_SECRET`) arrives via
//! `secretKeyRef`, so the pod object holds a reference rather than a value —
//! one future literal-valued env var would turn a diagnostic dump into a
//! credential leak in CI logs. Extract the scalars first, then format.

#![cfg(feature = "smoke")]

use env_tests::NAMESPACE;
use serde_json::Value;
use std::process::Command;

const CONTAINER: &str = "mh-service";
const GRACE_ENV: &str = "MH_TERMINATION_GRACE_SECONDS";

/// Fetches the single pod matching `instance=<instance>` as parsed JSON.
///
/// Never skips. Missing `kubectl`, a failed call, or an empty pod set are all
/// hard failures with distinct messages — a probe that can return "nothing" and
/// be read as "equal" is what this test exists to rule out.
fn fetch_pod(instance: &str) -> Value {
    let selector = format!("instance={instance}");
    let args = [
        "get",
        "pods",
        "-n",
        NAMESPACE,
        "-l",
        selector.as_str(),
        "-o",
        "json",
    ];

    let output = Command::new("kubectl")
        .args(args)
        .output()
        .unwrap_or_else(|e| {
            panic!(
                "kubectl not available - cannot verify the deployed drain window \
                 for {instance}. env-tests require kubectl installed and \
                 configured: {e}"
            )
        });

    assert!(
        output.status.success(),
        "`kubectl {}` failed for {instance} (exit status {}): {}",
        args.join(" "),
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    let parsed: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!("could not parse `kubectl get pods -o json` output for {instance}: {e}")
    });

    let items = parsed
        .get("items")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("kubectl output for {instance} has no `items` array"));

    assert!(
        !items.is_empty(),
        "no pods matched -n {NAMESPACE} -l instance={instance}. The drain-window \
         assertion would have nothing to compare and would pass vacuously. \
         Either MH is not deployed or the label selector has drifted - fix the \
         selector, do not delete this assertion."
    );

    // Ignore pods that are already terminating. During a rolling update both the
    // old and the new ReplicaSet's pods match the `instance=` selector, and the
    // OLD pod predates this manifest change - it carries no MH_TERMINATION_GRACE_SECONDS
    // at all. Taking items[0] unfiltered would fail against a pod that is on its
    // way out, which is a false negative on the Layer-7 retry path rather than a
    // real defect. Filtering on deletionTimestamp (not on phase) is the precise
    // test: it means "the API server has accepted this pod's deletion".
    let live: Vec<&Value> = items
        .iter()
        .filter(|p| p.pointer("/metadata/deletionTimestamp").is_none())
        .collect();

    assert!(
        !live.is_empty(),
        "every pod matching -n {NAMESPACE} -l instance={instance} is terminating \
         ({} total). Nothing is serving this instance - the assertion is not \
         meaningful and must not pass vacuously.",
        items.len()
    );

    // Pick the NEWEST live pod, not an arbitrary one.
    //
    // `deletionTimestamp` alone is not sufficient. These are 1-replica
    // Deployments with the default RollingUpdate strategy, which resolves to
    // maxSurge=1 / maxUnavailable=0 — so the replacement pod is created and must
    // become Ready BEFORE the outgoing pod is marked for deletion. That leaves a
    // window where two pods match `instance=mh-N` and NEITHER has a
    // `deletionTimestamp`: the old one (which predates this manifest change and
    // carries no MH_TERMINATION_GRACE_SECONDS at all) and the new one. Taking an
    // arbitrary element would panic against a pod on its way out — a false
    // negative on the Layer-7 retry path, not a real defect.
    //
    // The surged pod is always the newer, and it is the one carrying the value
    // under test. RFC 3339 timestamps sort lexicographically, so a string
    // comparison is the correct ordering here.
    let newest = live
        .iter()
        .max_by_key(|p| {
            p.pointer("/metadata/creationTimestamp")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string()
        })
        .expect("live is non-empty, asserted above");

    (*newest).clone()
}

/// Asserts one instance's deployed env value equals its own pod spec's grace.
fn assert_grace_matches_pod_spec(instance: &str) {
    let pod = fetch_pod(instance);

    // Extract every scalar BEFORE formatting anything, so no `Value` and no env
    // array can reach a panic message. See the PII policy in the module doc.
    let pod_name = pod
        .pointer("/metadata/name")
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("pod matched by instance={instance} has no metadata.name"))
        .to_string();

    let spec_grace = pod
        .pointer("/spec/terminationGracePeriodSeconds")
        .and_then(Value::as_i64)
        .unwrap_or_else(|| {
            panic!(
                "pod {pod_name} (-n {NAMESPACE}) has no readable \
                 .spec.terminationGracePeriodSeconds. This is the SOURCE side of \
                 the kustomize replacement; without it the derivation has no \
                 single source of truth."
            )
        });

    let containers = pod
        .pointer("/spec/containers")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("pod {pod_name} has no .spec.containers array"));

    let container = containers
        .iter()
        .find(|c| c.get("name").and_then(Value::as_str) == Some(CONTAINER))
        .unwrap_or_else(|| {
            panic!("pod {pod_name} has no container named {CONTAINER:?}");
        });

    let env = container
        .get("env")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("container {CONTAINER} in pod {pod_name} has no `env` array"));

    let env_entry = env
        .iter()
        .find(|e| e.get("name").and_then(Value::as_str) == Some(GRACE_ENV));

    let Some(env_entry) = env_entry else {
        // Names only - never the values. See the PII policy in the module doc.
        let names: Vec<&str> = env
            .iter()
            .filter_map(|e| e.get("name").and_then(Value::as_str))
            .collect();
        panic!(
            "pod {pod_name} has no {GRACE_ENV} env entry. This is the TARGET side \
             of the kustomize replacement. Present env names: {names:?}"
        );
    };

    let raw = env_entry
        .get("value")
        .and_then(Value::as_str)
        .unwrap_or_else(|| {
            panic!("pod {pod_name}: {GRACE_ENV} has no literal `value` (is it a valueFrom?)")
        });

    // Branch 1: the sentinel survived. This is a BUILD-PATH failure, not a
    // drifted value, and "expected 35, got 0" would send a responder to the
    // wrong file - so it gets its own message.
    assert_ne!(
        raw, "0",
        "pod {pod_name}: {GRACE_ENV} is still the unreplaced sentinel \"0\". The \
         kustomize `replacements:` block in \
         infra/services/mh-service/kustomization.yaml did NOT run - these \
         manifests were applied without kustomize (`kubectl apply -f` bypasses \
         it). Redeploy through the Kind overlay. This is a build-path problem, \
         not a wrong grace period."
    );

    let env_grace: i64 = raw.parse().unwrap_or_else(|e| {
        panic!("pod {pod_name}: {GRACE_ENV} value {raw:?} is not an integer: {e}")
    });

    // Branch 2: both sides present and parseable, but they disagree.
    assert_eq!(
        env_grace, spec_grace,
        "pod {pod_name} (-n {NAMESPACE}): {GRACE_ENV}={env_grace} does not match \
         its own .spec.terminationGracePeriodSeconds={spec_grace}. These must be \
         equal by construction - the env value is DERIVED from the pod spec by \
         the kustomize replacement (R-21). A mismatch means the replacement \
         targeted the wrong Deployment, or the value was hand-edited."
    );
}

#[tokio::test]
async fn test_mh_0_termination_grace_matches_pod_spec() {
    assert_grace_matches_pod_spec("mh-0");
}

#[tokio::test]
async fn test_mh_1_termination_grace_matches_pod_spec() {
    assert_grace_matches_pod_spec("mh-1");
}
