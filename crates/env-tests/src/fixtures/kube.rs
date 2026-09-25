//! Reads of the LIVE cluster's configuration, for tests that must compare what
//! a service publishes against what is actually deployed.
//!
//! The deployed ConfigMap is the single source of truth for every number these
//! tests check against. Reading it here — rather than restating the value as a
//! literal in the test — is what makes a test go red when a service stops
//! publishing the value it enforces, instead of two literals staying consistent
//! with each other and inconsistent with the cluster.
//!
//! Hoisted from `tests/28_mh_egress_admission.rs` at its second consumer
//! (`tests/29_mh_meeting_teardown.rs`).
//!
//! Every failure here is a PRECONDITION panic, never a skip: a test that cannot
//! read the deployed value has nothing to compare against.
//!
//! The same reasoning covers CREDENTIALS a test presents: reading the deployed
//! Secret, not a copy of it, means a rotated dev credential fails as a
//! credential rather than as a mysterious environment fault.

use std::process::Command;

use crate::NAMESPACE;

/// Read one key of a live ConfigMap in [`NAMESPACE`], never skipping.
///
/// # Panics
///
/// When `kubectl` cannot run, the ConfigMap cannot be read, or the key is
/// absent or empty.
#[must_use]
pub fn configmap_key(configmap: &str, key: &str) -> String {
    let jsonpath = format!("jsonpath={{.data.{key}}}");
    let output = Command::new("kubectl")
        .args([
            "get",
            "configmap",
            configmap,
            "-n",
            NAMESPACE,
            "-o",
            &jsonpath,
        ])
        .output()
        .unwrap_or_else(|e| panic!("PRECONDITION: could not run kubectl: {e}"));
    assert!(
        output.status.success(),
        "PRECONDITION: kubectl get configmap {configmap} -n {NAMESPACE} failed (exit {:?})",
        output.status.code()
    );
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    assert!(
        !value.is_empty(),
        "PRECONDITION: live ConfigMap {configmap} has no key {key}"
    );
    value
}

/// [`configmap_key`], parsed as an unsigned integer.
///
/// # Panics
///
/// As [`configmap_key`], or when the value is not an integer.
#[must_use]
pub fn configmap_u64(configmap: &str, key: &str) -> u64 {
    let raw = configmap_key(configmap, key);
    raw.parse().unwrap_or_else(|e| {
        panic!("PRECONDITION: {configmap}/{key}={raw:?} is not an integer: {e}")
    })
}

/// Read one key of a live Secret in [`NAMESPACE`], DECODED, never skipping.
///
/// The decode is kubectl's own go-template `base64decode`, so the suite takes no
/// extra dependency for it. **The value is never placed in any message**: every
/// failure names the Secret and the key only.
///
/// # Panics
///
/// When `kubectl` cannot run, the Secret or key is absent (kubectl's template
/// errors on a missing key rather than yielding an empty string), or the decoded
/// value is empty.
#[must_use]
pub fn secret_key(secret: &str, key: &str) -> String {
    let template = format!("go-template={{{{index .data \"{key}\" | base64decode}}}}");
    let output = Command::new("kubectl")
        .args(["get", "secret", secret, "-n", NAMESPACE, "-o", &template])
        .output()
        .unwrap_or_else(|e| panic!("PRECONDITION: could not run kubectl: {e}"));
    assert!(
        output.status.success(),
        "PRECONDITION: could not read key {key} of Secret {secret} -n {NAMESPACE} (exit {:?})",
        output.status.code()
    );
    let value = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(
        !value.is_empty(),
        "PRECONDITION: Secret {secret} key {key} decodes to an empty value"
    );
    value
}

/// Read a literal `env` value from a live Deployment's pod template, never
/// skipping — for configuration a Deployment states inline rather than through a
/// ConfigMap (e.g. MC's `MC_CLIENT_ID`).
///
/// # Panics
///
/// When `kubectl` cannot run, the Deployment cannot be read, or no container
/// sets `var` to a literal value.
#[must_use]
pub fn deployment_env_value(deployment: &str, var: &str) -> String {
    let jsonpath =
        format!("jsonpath={{.spec.template.spec.containers[*].env[?(@.name==\"{var}\")].value}}");
    let output = Command::new("kubectl")
        .args([
            "get",
            "deployment",
            deployment,
            "-n",
            NAMESPACE,
            "-o",
            &jsonpath,
        ])
        .output()
        .unwrap_or_else(|e| panic!("PRECONDITION: could not run kubectl: {e}"));
    assert!(
        output.status.success(),
        "PRECONDITION: kubectl get deployment {deployment} -n {NAMESPACE} failed (exit {:?})",
        output.status.code()
    );
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    assert!(
        !value.is_empty(),
        "PRECONDITION: deployment {deployment} sets no literal {var}"
    );
    value
}
