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

use std::collections::BTreeSet;
use std::process::Command;

use serde_json::Value;

use crate::NAMESPACE;

/// The ONE remediation for "the cluster is not running the tree's
/// configuration" (a pod predating a config change, an overlay value that did
/// not land). Every such failure message references this constant rather than
/// spelling a command.
///
/// ConfigMaps are content-addressed (ADR-0038 §2): a config change renames the
/// ConfigMap, which changes the pod template, which rolls the pod on the next
/// `deploy`. So the fix is to CONVERGE the cluster to the tree — never a
/// `kubectl rollout restart` (which re-runs the same, stale template) and never
/// a plain `kubectl apply -k` (the bases carry `:render-required` image
/// placeholders; only `deploy.sh`'s render applies them).
///
/// Spelled in three languages (this constant, `infra/kind/scripts/*.sh`
/// remedies, `docs/runbooks/devloop-validation.md` §6.7) that cannot share a
/// constant; `scripts/guards/simple/validate-dev-cluster-verbs.sh` keeps every
/// `dev-cluster <verb>` spelling on a verb the helper accepts.
pub const REDEPLOY_HINT: &str = "converge the cluster to the tree: `dev-cluster deploy` \
     (from the devloop container) or `./infra/kind/scripts/deploy.sh` (host)";

/// The name of the ONE ConfigMap generation, for generator base name `base`,
/// that the live pod(s) of `instance` (`-l instance=<instance>`) reference —
/// via `env[].valueFrom.configMapKeyRef`, `envFrom[].configMapRef`, or a
/// `configMap` / projected volume, in containers or initContainers.
///
/// Every generated ConfigMap is content-addressed (ADR-0038 §2): its live name
/// is `<base>-<10-char hash>`, and superseded generations are not pruned, so
/// neither the bare base name nor a label selector identifies the config IN
/// EFFECT. The pod spec does. Terminating pods are ignored.
///
/// # Panics
///
/// When `kubectl` cannot run, or [`resolve_generation`] refuses (see there) —
/// never a guess.
#[must_use]
pub fn configmap_name_for(instance: &str, base: &str) -> String {
    resolve_generation(&live_pods_json(instance), instance, base)
        .unwrap_or_else(|e| panic!("PRECONDITION: {e}"))
}

/// [`configmap_name_for`] over SEVERAL instances that must share one
/// generation (e.g. both MH instances read `mh-service-config`). Resolving from
/// one instance and checking another would blame the wrong thing mid-rollout,
/// so every instance is resolved and they must agree.
///
/// # Panics
///
/// As [`configmap_name_for`], when `instances` is empty, or when the instances
/// reference different generations (they run different configuration).
#[must_use]
pub fn configmap_name_for_all(instances: &[&str], base: &str) -> String {
    let names: Vec<String> = instances
        .iter()
        .map(|i| configmap_name_for(i, base))
        .collect();
    agree_on_generation(instances, base, names).unwrap_or_else(|e| panic!("PRECONDITION: {e}"))
}

/// PURE core of [`configmap_name_for_all`]: the one generation every instance
/// resolved to.
///
/// # Errors
///
/// `names` is empty (no instances), or the instances resolved to different
/// generations (they run different configuration).
pub fn agree_on_generation(
    instances: &[&str],
    base: &str,
    names: Vec<String>,
) -> Result<String, String> {
    if names.is_empty() {
        return Err(format!(
            "configmap_name_for_all called with no instances for {base}"
        ));
    }
    if !names.windows(2).all(|w| w[0] == w[1]) {
        return Err(format!(
            "instances {instances:?} reference DIFFERENT generations of {base}: \
             {names:?}. They run different configuration; {REDEPLOY_HINT}."
        ));
    }
    Ok(names.into_iter().next().expect("non-empty, checked above"))
}

/// `kubectl get pods -l instance=<instance> -o json`, parsed.
fn live_pods_json(instance: &str) -> Value {
    let selector = format!("instance={instance}");
    let output = Command::new("kubectl")
        .args([
            "get", "pods", "-n", NAMESPACE, "-l", &selector, "-o", "json",
        ])
        .output()
        .unwrap_or_else(|e| panic!("PRECONDITION: could not run kubectl: {e}"));
    assert!(
        output.status.success(),
        "PRECONDITION: kubectl get pods -n {NAMESPACE} -l {selector} failed (exit {:?})",
        output.status.code()
    );
    serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!("PRECONDITION: could not parse kubectl get pods -l {selector} output: {e}")
    })
}

/// PURE core of [`configmap_name_for`]: from a `kubectl get pods -o json`
/// document, the ONE generation of `base` the live (non-terminating) pods
/// reference.
///
/// # Errors
///
/// - no `items` array, or no live pod;
/// - a pod references the BARE `base` name — it is not content-addressed, so
///   an in-place edit could go unrolled (the case content addressing, and the
///   retired staleness test, depend on being impossible);
/// - zero generations of `base` (the pod is not wired to it) or more than one
///   (a rollout in flight).
pub fn resolve_generation(pods: &Value, instance: &str, base: &str) -> Result<String, String> {
    let items = pods
        .get("items")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("kubectl output for instance={instance} has no `items` array"))?;
    let live: Vec<&Value> = items
        .iter()
        .filter(|p| p.pointer("/metadata/deletionTimestamp").is_none())
        .collect();
    if live.is_empty() {
        return Err(format!(
            "no live pod matches -n {NAMESPACE} -l instance={instance} ({} terminating); \
             cannot resolve which {base} generation is in effect",
            items.len()
        ));
    }
    let refs: Vec<String> = live.iter().flat_map(|p| referenced_configmaps(p)).collect();
    if refs.iter().any(|r| r == base) {
        return Err(format!(
            "a live pod of {instance} references the BARE ConfigMap name {base}: it is not \
             content-addressed (ADR-0038 §2), so a config edit would not roll the pod; \
             {REDEPLOY_HINT}"
        ));
    }
    let names: BTreeSet<String> = refs
        .into_iter()
        .filter(|n| is_generation_of(n, base))
        .collect();
    if names.len() != 1 {
        return Err(format!(
            "the live pod(s) of {instance} reference {} generation(s) of ConfigMap {base} \
             ({names:?}); expected exactly one. Zero: the pod is not wired to {base}. More \
             than one: a rollout is in flight — wait for it, or {REDEPLOY_HINT}.",
            names.len()
        ));
    }
    Ok(names.into_iter().next().expect("exactly one"))
}

/// Every ConfigMap name a pod's spec references.
fn referenced_configmaps(pod: &Value) -> Vec<String> {
    let mut out = Vec::new();
    let str_at = |v: &Value, ptr: &str| v.pointer(ptr).and_then(Value::as_str).map(str::to_string);
    for list in ["/spec/containers", "/spec/initContainers"] {
        for c in pod
            .pointer(list)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            for e in c.get("env").and_then(Value::as_array).into_iter().flatten() {
                out.extend(str_at(e, "/valueFrom/configMapKeyRef/name"));
            }
            for e in c
                .get("envFrom")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                out.extend(str_at(e, "/configMapRef/name"));
            }
        }
    }
    for v in pod
        .pointer("/spec/volumes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        out.extend(str_at(v, "/configMap/name"));
        for s in v
            .pointer("/projected/sources")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            out.extend(str_at(s, "/configMap/name"));
        }
    }
    out
}

/// Length of kustomize's generator hash suffix (`<name>-<hash>`).
const KUSTOMIZE_HASH_LEN: usize = 10;

/// Is `name` a content-addressed generation of generator `base` — exactly
/// `base-` followed by kustomize's 10-character lowercase-alphanumeric hash?
/// (So `mh-service-config` never claims `mh-service-config-extra`, and the bare
/// base is not a generation.)
fn is_generation_of(name: &str, base: &str) -> bool {
    name.strip_prefix(base)
        .and_then(|r| r.strip_prefix('-'))
        .is_some_and(|h| {
            h.len() == KUSTOMIZE_HASH_LEN
                && h.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        })
}

/// Read one key of a live ConfigMap in [`NAMESPACE`], never skipping.
///
/// `configmap` is a LIVE name — for a generated ConfigMap, resolve it first
/// with [`configmap_name_for`]; the bare generator base name no longer exists.
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const BASE: &str = "mh-service-config";
    const GEN_A: &str = "mh-service-config-7f2kd9h4mt";
    const GEN_B: &str = "mh-service-config-b8c5g2k6tm";

    fn pod(cm: &str) -> Value {
        json!({"metadata": {"name": "p"}, "spec": {"containers": [
            {"env": [{"valueFrom": {"configMapKeyRef": {"name": cm}}}]}]}})
    }

    fn terminating(mut p: Value) -> Value {
        p["metadata"]["deletionTimestamp"] = json!("2026-09-29T00:00:00Z");
        p
    }

    #[test]
    fn generation_matching_requires_the_exact_hash_suffix() {
        assert!(is_generation_of(GEN_A, BASE));
        assert!(
            !is_generation_of(BASE, BASE),
            "the bare base is not a generation"
        );
        assert!(!is_generation_of("mh-service-config-extra", BASE));
        assert!(!is_generation_of(
            "mh-service-config-extra-7f2kd9h4mt",
            BASE
        ));
        assert!(!is_generation_of("mh-0-config-7f2kd9h4mt", BASE));
        assert!(!is_generation_of("mh-service-config-", BASE));
        assert!(!is_generation_of("mh-service-config-7F2KD9H4MT", BASE));
    }

    #[test]
    fn references_are_read_from_env_envfrom_volumes_and_init_containers() {
        let p = json!({"spec": {
            "containers": [{"env": [{"valueFrom": {"configMapKeyRef": {"name": "a-1"}}},
                                     {"value": "literal"}],
                            "envFrom": [{"configMapRef": {"name": "b-1"}}]}],
            "initContainers": [{"env": [{"valueFrom": {"configMapKeyRef": {"name": "e-1"}}}]}],
            "volumes": [{"configMap": {"name": "c-1"}},
                        {"projected": {"sources": [{"configMap": {"name": "d-1"}}]}}]
        }});
        let mut got = referenced_configmaps(&p);
        got.sort();
        assert_eq!(got, vec!["a-1", "b-1", "c-1", "d-1", "e-1"]);
    }

    #[test]
    fn one_live_generation_resolves() {
        let pods = json!({"items": [pod(GEN_A)]});
        assert_eq!(resolve_generation(&pods, "mh-0", BASE).unwrap(), GEN_A);
    }

    #[test]
    fn a_terminating_old_generation_is_ignored() {
        let pods = json!({"items": [terminating(pod(GEN_B)), pod(GEN_A)]});
        assert_eq!(resolve_generation(&pods, "mh-0", BASE).unwrap(), GEN_A);
    }

    #[test]
    fn a_reference_from_an_init_container_resolves() {
        let p = json!({"metadata": {"name": "p"}, "spec": {"initContainers": [
            {"envFrom": [{"configMapRef": {"name": GEN_A}}]}]}});
        assert_eq!(
            resolve_generation(&json!({"items": [p]}), "mh-0", BASE).unwrap(),
            GEN_A
        );
    }

    #[test]
    fn zero_generations_is_an_error() {
        let err = resolve_generation(&json!({"items": [pod("other-7f2kd9h4mt")]}), "mh-0", BASE)
            .unwrap_err();
        assert!(err.contains("0 generation"), "{err}");
    }

    #[test]
    fn two_live_generations_is_an_error() {
        let err = resolve_generation(&json!({"items": [pod(GEN_A), pod(GEN_B)]}), "mh-0", BASE)
            .unwrap_err();
        assert!(err.contains("2 generation"), "{err}");
    }

    #[test]
    fn only_terminating_pods_is_an_error() {
        let err = resolve_generation(&json!({"items": [terminating(pod(GEN_A))]}), "mh-0", BASE)
            .unwrap_err();
        assert!(err.contains("no live pod"), "{err}");
    }

    #[test]
    fn missing_items_is_an_error() {
        let err = resolve_generation(&json!({"kind": "PodList"}), "mh-0", BASE).unwrap_err();
        assert!(err.contains("no `items`"), "{err}");
    }

    #[test]
    fn a_bare_base_reference_is_a_not_content_addressed_error() {
        let err = resolve_generation(&json!({"items": [pod(BASE)]}), "mh-0", BASE).unwrap_err();
        assert!(
            err.contains("BARE") && err.contains("content-addressed"),
            "{err}"
        );
    }

    #[test]
    fn instances_on_one_generation_agree() {
        let names = vec![GEN_A.to_owned(), GEN_A.to_owned()];
        assert_eq!(
            agree_on_generation(&["mh-0", "mh-1"], BASE, names).unwrap(),
            GEN_A
        );
    }

    #[test]
    fn instances_on_different_generations_is_an_error() {
        let names = vec![GEN_A.to_owned(), GEN_B.to_owned()];
        let err = agree_on_generation(&["mh-0", "mh-1"], BASE, names).unwrap_err();
        assert!(
            err.contains("DIFFERENT generations") && err.contains(GEN_A) && err.contains(GEN_B),
            "{err}"
        );
    }

    #[test]
    fn no_instances_is_an_error() {
        let err = agree_on_generation(&[], BASE, vec![]).unwrap_err();
        assert!(err.contains("no instances"), "{err}");
    }
}
