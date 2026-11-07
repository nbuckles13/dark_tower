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
use std::time::Duration;

use serde_json::Value;

use crate::fixtures::deployment_config::LivePod;
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

/// The GENERATOR BASE NAME of the shared media codec profile (story 3),
/// read by MC and MH. Its live name is `media-codec-profile-<hash>`
/// (ADR-0038 §2), resolved from a pod spec with [`configmap_name_for`] /
/// [`configmap_name_for_all`]. One home for the deployment-config suites.
pub const PROFILE_CONFIGMAP: &str = "media-codec-profile";

/// The Kind MH instances (the per-instance Deployments `mh-0`, `mh-1`). ONE
/// list for every suite that reads per-instance MH state, so a caller of
/// [`configmap_name_for_all`] cannot pass a partial set. (Runtime Prometheus pins
/// use [`live_mh_pods`] instead: no instance names.)
pub const MH_INSTANCES: &[&str] = &["mh-0", "mh-1"];

/// The Kind MC instances (`mc-0`, `mc-1`). See [`MH_INSTANCES`].
pub const MC_INSTANCES: &[&str] = &["mc-0", "mc-1"];

/// Every reader of [`PROFILE_CONFIGMAP`]: MC and MH, which roll together on a
/// profile change and must resolve ONE generation.
pub const PROFILE_READERS: &[&str] = &["mc-0", "mc-1", "mh-0", "mh-1"];

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

/// `kubectl get pods -n dark-tower -l instance=<instance> -o json`, parsed —
/// the PRECONDITION-panic wrapper over [`kubectl_get_json`] (the ONE
/// kubectl-get-json kernel) for the per-instance pod list (also behind
/// `deployment_config::fetch_live_pod`).
///
/// # Panics
/// `kubectl` cannot run, fails, or prints unparseable output.
#[must_use]
pub fn live_pods_json(instance: &str) -> Value {
    pods_json(&format!("instance={instance}"))
}

/// `kubectl get pods -n dark-tower -l <selector> -o json`, parsed — the ONE
/// pod-list call shape (namespace pinned in the argv, never the context's
/// default). [`live_pods_json`] is its per-instance form.
///
/// # Panics
/// `kubectl` cannot run, fails, or prints unparseable output.
#[must_use]
pub fn pods_json(selector: &str) -> Value {
    kubectl_get_json(&["pods", "-n", NAMESPACE, "-l", selector])
        .unwrap_or_else(|e| panic!("PRECONDITION: {e}"))
}

/// The label every MH pod carries, whatever Deployment, slot or incarnation
/// created it (ADR-0039 §7 template invariant). Env-tests find the MHs to talk
/// to by THIS label from the live pod list — never by a fixed instance name or
/// count.
pub const MH_SELECTOR: &str = "app=mh-service";

/// Is `pod` terminating (it has a `metadata.deletionTimestamp`)? The ONE home
/// of that rule for every pod reader here.
#[must_use]
pub fn is_terminating(pod: &Value) -> bool {
    pod.pointer("/metadata/deletionTimestamp").is_some()
}

/// Does `pod` report the `Ready` condition as `"True"`? A pod with no
/// conditions at all (Pending) or `Ready=False` is not Ready.
#[must_use]
pub fn is_ready(pod: &Value) -> bool {
    pod.pointer("/status/conditions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|c| {
            c.get("type").and_then(Value::as_str) == Some("Ready")
                && c.get("status").and_then(Value::as_str) == Some("True")
        })
}

/// A label-selected pod list, partitioned. `serving` holds the non-terminating,
/// Ready pods (with their IP); the other two buckets name every pod that is NOT
/// serving, so a caller can refuse them instead of silently working on a subset.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct PodPartition {
    pub serving: Vec<LivePod>,
    pub terminating: Vec<String>,
    pub not_ready: Vec<String>,
}

/// PURE: partition a `kubectl get pods -o json` document. Terminating pods go to
/// `terminating` (even if still Ready), non-terminating not-Ready pods to
/// `not_ready`, and the rest to `serving` as label-discovered [`LivePod`]s
/// (`instance: None`). The pod NAME and the pod IP come from the SAME object, so
/// the pod a test calls and the Prometheus `instance` it pins are one pod.
///
/// # Errors
/// No `items` array, a pod without `metadata.name`, or a Ready pod without
/// `status.podIP` (`no-pod-ip`).
pub fn partition_serving(pods: &Value, selector: &str) -> Result<PodPartition, String> {
    let items = pods
        .get("items")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("kubectl output for -l {selector} has no `items` array"))?;
    let mut out = PodPartition::default();
    for p in items {
        let name = p
            .pointer("/metadata/name")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("a pod matching -l {selector} has no metadata.name"))?
            .to_string();
        if is_terminating(p) {
            out.terminating.push(name);
        } else if !is_ready(p) {
            out.not_ready.push(name);
        } else {
            let ip = p
                .pointer("/status/podIP")
                .and_then(Value::as_str)
                .filter(|ip| !ip.is_empty())
                .ok_or_else(|| {
                    format!(
                        "no-pod-ip: pod {name} (-l {selector}) is Ready but has no status.podIP"
                    )
                })?
                .to_string();
            out.serving.push(LivePod {
                instance: None,
                name,
                ip,
            });
        }
    }
    Ok(out)
}

/// PURE verdict on one [`PodPartition`]: EVERY matching pod is serving, and
/// there is at least one. A Terminating or not-Ready pod is refused, never
/// dropped — dropping it would quietly turn "every live MH" into a subset (a
/// crash-looping or still-starting MH the test never injects into or pins).
///
/// # Errors
/// Distinct texts per cause: `zero-pods`, `terminating`, `not-ready`.
pub fn require_all_serving(
    partition: &PodPartition,
    selector: &str,
) -> Result<Vec<LivePod>, String> {
    if partition.serving.is_empty()
        && partition.terminating.is_empty()
        && partition.not_ready.is_empty()
    {
        return Err(format!(
            "zero-pods: no pod matches -n {NAMESPACE} -l {selector} — nothing to test against; \
             the service is not deployed or the label has drifted"
        ));
    }
    if !partition.terminating.is_empty() {
        return Err(format!(
            "terminating: pod(s) {:?} matching -l {selector} are Terminating (serving: {:?}) — \
             the set of handlers is changing under the test",
            partition.terminating,
            names(&partition.serving)
        ));
    }
    if !partition.not_ready.is_empty() {
        return Err(format!(
            "not-ready: pod(s) {:?} matching -l {selector} are not Ready (serving: {:?}) — a \
             starting or crash-looping pod would be silently left out",
            partition.not_ready,
            names(&partition.serving)
        ));
    }
    Ok(partition.serving.clone())
}

fn names(pods: &[LivePod]) -> Vec<&str> {
    pods.iter().map(|p| p.name.as_str()).collect()
}

/// How long [`live_mh_pods`] waits for the MH pod set to settle: an old
/// ReplicaSet's pod can still be inside its termination grace after a converge
/// (`rollout status` does not wait for it), and a starting pod needs time to
/// turn Ready. The K8s-resource-update SLA (60s).
pub const MH_POD_SETTLE: crate::eventual::ConsistencyCategory =
    crate::eventual::ConsistencyCategory::K8sResourceUpdate;

/// EVERY live MH pod, found by [`MH_SELECTOR`] — no fixed name or count.
///
/// [`settled_serving_pods`] bounded by [`MH_POD_SETTLE`]: re-lists until
/// [`require_all_serving`] holds, then returns the serving set from that ONE
/// snapshot.
///
/// # Panics
/// PRECONDITION (`mh-pods-unserving`) at the bound, with the last snapshot's
/// verdict: zero MH pods, or one still Terminating / not Ready. Never a subset.
/// Do NOT loosen this for a test that kills an MH: such a test waits for its
/// replacement to be Ready before anything resolves handlers.
pub async fn live_mh_pods() -> Vec<LivePod> {
    settled_serving_pods(MH_SELECTOR, MH_POD_SETTLE.timeout(), "mh-pods-unserving")
        .await
        .1
}

/// How often [`settled_serving_pods`] re-lists.
const SETTLE_POLL: Duration = Duration::from_secs(1);

/// The ONE settle kernel for a label-selected pod set: re-list `selector`
/// (bounded by `timeout`) until [`require_all_serving`] holds — every matching
/// pod Ready and none Terminating, at least one — and return that ONE
/// snapshot's document together with its serving set, so a caller reading pod
/// specs and a caller pinning pod IPs see the same pods.
///
/// Waiting is for the TRANSIENT states right after a converge (`rollout
/// status` returns while an old ReplicaSet's pod is still inside its
/// termination grace). At the bound the last verdict fails loudly; a pod that
/// stays Terminating or not Ready is never dropped.
///
/// # Panics
/// PRECONDITION (`<reason>`) at the bound, with the last snapshot's verdict.
pub async fn settled_serving_pods(
    selector: &str,
    timeout: Duration,
    reason: &str,
) -> (Value, Vec<LivePod>) {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        let sel = selector.to_string();
        // kubectl blocks: run it on the blocking pool, never on the test's
        // (current-thread) runtime thread that drives live sessions.
        let (doc, verdict) = join_blocking(
            tokio::task::spawn_blocking(move || {
                let doc = pods_json(&sel);
                let verdict =
                    partition_serving(&doc, &sel).and_then(|p| require_all_serving(&p, &sel));
                (doc, verdict)
            })
            .await,
        );
        match verdict {
            Ok(pods) => return (doc, pods),
            Err(e) if std::time::Instant::now() >= deadline => panic!(
                "PRECONDITION ({reason}): after {timeout:?}: {e}. Inspect: kubectl -n {NAMESPACE} \
                 get pods -l {selector} -o wide"
            ),
            Err(_) => tokio::time::sleep(SETTLE_POLL).await,
        }
    }
}

/// PURE: the largest `spec.terminationGracePeriodSeconds` over EVERY pod in a
/// `kubectl get pods -o json` document, Terminating ones included (the pod
/// being waited out carries the grace it was deleted with). Read from the live
/// specs, never a manifest: the API server always stores the field (it
/// defaults it), so its absence is a loud error, not a guess.
///
/// # Errors
/// No `items`, zero pods, or a pod without the field.
pub fn max_termination_grace(pods: &Value, selector: &str) -> Result<Duration, String> {
    let items = pods
        .get("items")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("kubectl output for -l {selector} has no `items` array"))?;
    let mut max: Option<u64> = None;
    for p in items {
        let name = p
            .pointer("/metadata/name")
            .and_then(Value::as_str)
            .unwrap_or("<unnamed pod>");
        let grace = p
            .pointer("/spec/terminationGracePeriodSeconds")
            .and_then(Value::as_u64)
            .ok_or_else(|| {
                format!(
                    "pod {name} (-l {selector}) has no spec.terminationGracePeriodSeconds; the \
                     API server always stores it, so this document is not a live pod list"
                )
            })?;
        max = Some(max.map_or(grace, |m| m.max(grace)));
    }
    max.map(Duration::from_secs).ok_or_else(|| {
        format!("zero-pods: no pod matches -n {NAMESPACE} -l {selector}; no grace to wait out")
    })
}

/// The value of a `spawn_blocking` task, re-raising its panic AS ITSELF (so a
/// PRECONDITION text survives, rather than becoming a bare `JoinError`).
///
/// # Panics
/// The task panicked (re-raised), or was cancelled.
pub(crate) fn join_blocking<T>(joined: Result<T, tokio::task::JoinError>) -> T {
    match joined {
        Ok(v) => v,
        Err(e) if e.is_panic() => std::panic::resume_unwind(e.into_panic()),
        Err(e) => panic!("a blocking env-test fixture task was cancelled: {e}"),
    }
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
// ANCHOR (DRY): `scripts/layer7.sh:__dump_media_codec_profile` is a bash TWIN of this rule
// (with `referenced_configmaps` and `is_generation_of`) for the deployed media-codec-profile;
// bash cannot call this. Change both together; `scripts/layer7.test.sh` mirrors these cases.
pub fn resolve_generation(pods: &Value, instance: &str, base: &str) -> Result<String, String> {
    let items = pods
        .get("items")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("kubectl output for instance={instance} has no `items` array"))?;
    let live: Vec<&Value> = items.iter().filter(|p| !is_terminating(p)).collect();
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

/// `kubectl get <args...> -o json`, parsed. The Result-returning reader for
/// callers that map failures into their own typed error (e.g. the gate-zero
/// `GateZeroError::Precondition`) instead of the PRECONDITION panics above.
///
/// # Errors
///
/// `kubectl` cannot run, exits non-zero, or prints unparseable output.
pub fn kubectl_get_json(args: &[&str]) -> Result<Value, String> {
    let output = Command::new("kubectl")
        .arg("get")
        .args(args)
        .args(["-o", "json"])
        .output()
        .map_err(|e| format!("could not run kubectl: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "kubectl get {} failed (exit {:?}): {}",
            args.join(" "),
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("could not parse kubectl get {} output: {e}", args.join(" ")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn max_termination_grace_takes_the_largest_including_terminating() {
        let pods = json!({"items": [
            {"metadata": {"name": "new"}, "spec": {"terminationGracePeriodSeconds": 35}},
            {"metadata": {"name": "old", "deletionTimestamp": "2026-11-07T00:00:00Z"},
             "spec": {"terminationGracePeriodSeconds": 50}},
        ]});
        assert_eq!(
            max_termination_grace(&pods, "app=x"),
            Ok(Duration::from_secs(50))
        );
    }

    #[test]
    fn max_termination_grace_refuses_missing_field_and_zero_pods() {
        let missing = json!({"items": [{"metadata": {"name": "p"}, "spec": {}}]});
        let e = max_termination_grace(&missing, "app=x").unwrap_err();
        assert!(
            e.contains("pod p") && e.contains("terminationGracePeriodSeconds"),
            "{e}"
        );
        let e = max_termination_grace(&json!({"items": []}), "app=x").unwrap_err();
        assert!(e.starts_with("zero-pods"), "{e}");
        let e = max_termination_grace(&json!({}), "app=x").unwrap_err();
        assert!(e.contains("no `items`"), "{e}");
    }

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

    #[test]
    fn profile_readers_are_exactly_mc_then_mh() {
        let both: Vec<&str> = MC_INSTANCES.iter().chain(MH_INSTANCES).copied().collect();
        assert_eq!(PROFILE_READERS, both.as_slice());
    }

    // ---- label discovery: partition_serving / require_all_serving ---------

    fn mh(name: &str, ip: &str, ready: Option<&str>) -> Value {
        let mut p = json!({"metadata": {"name": name}, "status": {"podIP": ip}});
        if let Some(status) = ready {
            p["status"]["conditions"] = json!([{"type": "PodScheduled", "status": "True"}, {"type": "Ready", "status": status}]);
        }
        p
    }

    fn served(pods: &[Value]) -> PodPartition {
        partition_serving(&json!({"items": pods}), MH_SELECTOR).expect("partitions")
    }

    #[test]
    fn three_ready_pods_all_serve_no_fixed_count() {
        let p = served(&[
            mh("a", "10.0.0.1", Some("True")),
            mh("b", "10.0.0.2", Some("True")),
            mh("c", "10.0.0.3", Some("True")),
        ]);
        let got = require_all_serving(&p, MH_SELECTOR).expect("all serving");
        let names: Vec<&str> = got.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["a", "b", "c"]);
        assert_eq!(got[1].ip, "10.0.0.2", "name and IP come from the same pod");
        assert!(got.iter().all(|p| p.instance.is_none()));
    }

    #[test]
    fn a_terminating_pod_is_excluded_from_serving_even_when_ready() {
        let p = served(&[
            terminating(mh("old", "10.0.0.9", Some("True"))),
            mh("new", "10.0.0.1", Some("True")),
        ]);
        assert_eq!(p.terminating, ["old"]);
        assert_eq!(names(&p.serving), ["new"]);
    }

    #[test]
    fn a_terminating_pod_is_refused_not_dropped() {
        let p = served(&[
            terminating(mh("old", "10.0.0.9", Some("True"))),
            mh("new", "10.0.0.1", Some("True")),
        ]);
        let err = require_all_serving(&p, MH_SELECTOR).unwrap_err();
        assert!(err.starts_with("terminating:"), "{err}");
        assert!(err.contains("old"), "{err}");
    }

    #[test]
    fn not_ready_pods_are_excluded_from_serving() {
        // Ready=False, and a Pending pod with no conditions at all.
        let p = served(&[
            mh("a", "10.0.0.1", Some("True")),
            mh("b", "10.0.0.2", Some("True")),
            mh("starting", "10.0.0.3", Some("False")),
            mh("pending", "", None),
        ]);
        assert_eq!(names(&p.serving), ["a", "b"]);
        assert_eq!(p.not_ready, ["starting", "pending"]);
    }

    #[test]
    fn a_mixed_set_is_refused_naming_the_not_ready_pod() {
        let p = served(&[
            mh("a", "10.0.0.1", Some("True")),
            mh("b", "10.0.0.2", Some("True")),
            mh("starting", "10.0.0.3", Some("False")),
        ]);
        let err = require_all_serving(&p, MH_SELECTOR).unwrap_err();
        assert!(err.starts_with("not-ready:"), "{err}");
        assert!(err.contains("starting"), "{err}");
    }

    #[test]
    fn zero_pods_is_its_own_refusal() {
        let err = require_all_serving(&served(&[]), MH_SELECTOR).unwrap_err();
        assert!(err.starts_with("zero-pods:"), "{err}");
    }

    #[test]
    fn a_ready_pod_without_an_ip_is_refused() {
        let err = partition_serving(&json!({"items": [mh("a", "", Some("True"))]}), MH_SELECTOR)
            .unwrap_err();
        assert!(err.starts_with("no-pod-ip:"), "{err}");
    }

    #[test]
    fn a_pod_list_without_items_is_refused() {
        assert!(partition_serving(&json!({"kind": "PodList"}), MH_SELECTOR)
            .unwrap_err()
            .contains("no `items` array"));
    }

    #[test]
    fn readiness_reads_only_the_ready_condition() {
        assert!(is_ready(&mh("a", "1", Some("True"))));
        assert!(!is_ready(&mh("a", "1", Some("False"))));
        assert!(!is_ready(&mh("a", "1", None)));
        assert!(!is_ready(
            &json!({"status": {"conditions": [{"type": "PodScheduled", "status": "True"}]}})
        ));
        assert!(is_terminating(&terminating(mh("a", "1", None))));
        assert!(!is_terminating(&mh("a", "1", None)));
    }
}
