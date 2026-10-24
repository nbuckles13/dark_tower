//! Live-cluster DEPLOYMENT-CONFIG assertions shared by the per-service
//! deployment-config env-tests (`tests/01_mh_deployment_config.rs`,
//! `tests/02_mc_deployment_config.rs`).
//!
//! Hoisted from `01_mh_deployment_config.rs` at its second consumer (story 3):
//! the pod/ConfigMap fetches, the value shapes, and the wiring check move HERE
//! and are deleted there — never a parallel copy.
//!
//! The checks are PURE cores over parsed `kubectl -o json` documents
//! ([`check_key_ref`], [`check_resource_field_ref`], [`check_shape`],
//! [`quantity_to_bytes`]), returning `Err(message)` so every branch is unit
//! tested at Layer 4 rather than only triaged at Layer 7. The thin fetchers
//! panic as PRECONDITION failures, never skip.
//!
//! # Diagnostic output policy (PII)
//!
//! Messages carry only pod names, namespace, env var / key / ConfigMap NAMES,
//! the scalar values of named configuration keys (none is a credential), and
//! resource quantities. Never a parsed `Value`, an env array, a ConfigMap's
//! `data` map, or raw `kubectl` output. Extract scalars first, then format.

use std::collections::BTreeMap;
use std::process::Command;

use serde_json::Value;

use crate::NAMESPACE;

/// How a deployed value must parse — mirrors what the reading image's required
/// read accepts (SHAPE only: bounds stay with their readers, and no test holds
/// a second copy of a deployed number).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueShape {
    /// A count, rate or duration: an unsigned integer strictly above zero.
    PositiveInteger,
    /// An unsigned integer where zero is legal (e.g. the R-26 video-off lever,
    /// `MC_MAX_VIDEO_RECEIVE_SLOTS=0`).
    NonNegativeInteger,
    /// A non-empty token: no whitespace, no quotes (e.g. a codec name). The set
    /// of legal tokens is the reader's, deliberately not restated here.
    Token,
    /// A ratio in `0.0..=1.0`.
    UnitRatio,
}

/// Whether a `configMapKeyRef` must be hard or `optional: true`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefKind {
    /// A missing key must be a loud `CreateContainerConfigError`.
    Hard,
    /// A retiring key (two-deploy retirement, step 1).
    Optional,
}

/// Parse `raw` as an unsigned integer. `home` names where the value lives.
///
/// # Errors
/// Not an unsigned integer.
pub fn parse_u64(key: &str, raw: &str, home: &str) -> Result<u64, String> {
    raw.parse().map_err(|e| {
        format!(
            "{key}={raw:?} in {home} is not an unsigned integer ({e}). Its reader takes it as a \
             REQUIRED read and would refuse to start."
        )
    })
}

/// Parse `raw` as a strictly-positive integer.
///
/// # Errors
/// Not an unsigned integer, or zero.
pub fn parse_positive(key: &str, raw: &str, home: &str) -> Result<u64, String> {
    let v = parse_u64(key, raw, home)?;
    if v == 0 {
        return Err(format!(
            "{key}=0 in {home}: must be strictly positive. Its reader would refuse to start."
        ));
    }
    Ok(v)
}

/// Check `raw` against `shape`.
///
/// # Errors
/// The value does not have the shape, with the key and its home named.
pub fn check_shape(key: &str, raw: &str, shape: ValueShape, home: &str) -> Result<(), String> {
    match shape {
        ValueShape::PositiveInteger => parse_positive(key, raw, home).map(|_| ()),
        ValueShape::NonNegativeInteger => parse_u64(key, raw, home).map(|_| ()),
        ValueShape::Token => {
            let ok = !raw.is_empty()
                && !raw
                    .chars()
                    .any(|c| c.is_whitespace() || c == '"' || c == '\'');
            if ok {
                Ok(())
            } else {
                Err(format!(
                    "{key}={raw:?} in {home} is not a non-empty token (no whitespace or quotes)."
                ))
            }
        }
        ValueShape::UnitRatio => {
            let v: f64 = raw
                .parse()
                .map_err(|e| format!("{key}={raw:?} in {home} is not a number ({e})."))?;
            if v.is_finite() && (0.0..=1.0).contains(&v) {
                Ok(())
            } else {
                Err(format!(
                    "{key}={raw:?} in {home} must lie in 0.0..=1.0. Its reader would refuse to start."
                ))
            }
        }
    }
}

/// The pod's `metadata.name`.
///
/// # Panics
/// The pod has no name.
#[must_use]
pub fn pod_name_of(pod: &Value, instance: &str) -> String {
    pod.pointer("/metadata/name")
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("pod matched by instance={instance} has no metadata.name"))
        .to_string()
}

/// The container named `name` in a pod (containers, then initContainers).
///
/// # Errors
/// No such container.
pub fn container<'a>(pod: &'a Value, pod_name: &str, name: &str) -> Result<&'a Value, String> {
    ["/spec/containers", "/spec/initContainers"]
        .iter()
        .filter_map(|p| pod.pointer(p).and_then(Value::as_array))
        .flatten()
        .find(|c| c.get("name").and_then(Value::as_str) == Some(name))
        .ok_or_else(|| format!("pod {pod_name} has no container named {name:?}"))
}

/// The env entry `env_name` of `container_name`.
fn env_entry<'a>(
    pod: &'a Value,
    pod_name: &str,
    container_name: &str,
    env_name: &str,
) -> Result<&'a Value, String> {
    container(pod, pod_name, container_name)?
        .get("env")
        .and_then(Value::as_array)
        .and_then(|env| {
            env.iter()
                .find(|e| e.get("name").and_then(Value::as_str) == Some(env_name))
        })
        .ok_or_else(|| {
            format!(
                "pod {pod_name} (-n {NAMESPACE}): container {container_name} has no env entry \
                 {env_name}. Every key must be referenced from EVERY instance's Deployment — per \
                 workload, never union; a missing ref crash-loops that pod alone once its \
                 reader requires it."
            )
        })
}

/// PURE wiring check: env `key` of `container_name` is a `configMapKeyRef`
/// to the live ConfigMap generation `expected_cm`, its `key` equals the env
/// name, and its optionality is `kind`.
///
/// # Errors
/// Absent entry, not a `configMapKeyRef`, wrong ConfigMap, crossed key, or
/// optionality mismatch — each with its own message naming the key.
pub fn check_key_ref(
    pod: &Value,
    container_name: &str,
    key: &str,
    expected_cm: &str,
    kind: RefKind,
) -> Result<(), String> {
    let pod_name = pod
        .pointer("/metadata/name")
        .and_then(Value::as_str)
        .unwrap_or("<unnamed pod>");
    let entry = env_entry(pod, pod_name, container_name, key)?;
    let cm_ref = entry.pointer("/valueFrom/configMapKeyRef").ok_or_else(|| {
        format!(
            "pod {pod_name}: env {key} is not a configMapKeyRef (a literal value or another \
             source). Its ConfigMap is the single home for the value; a literal here is a second \
             encoding."
        )
    })?;
    let ref_name = cm_ref.get("name").and_then(Value::as_str);
    if ref_name != Some(expected_cm) {
        return Err(format!(
            "pod {pod_name}: env {key} references ConfigMap {ref_name:?}, expected {expected_cm:?} \
             (the generation this pod is wired to for that key's base)."
        ));
    }
    let ref_key = cm_ref.get("key").and_then(Value::as_str);
    if ref_key != Some(key) {
        return Err(format!(
            "pod {pod_name}: env {key} reads key {ref_key:?} — a CROSSED reference. The pod starts \
             and passes readiness while running another variable's value. The key must equal the \
             env var name (dt-guard env-config's configmap_key_name_mismatch catches this in the \
             manifests; reaching here means the live cluster does not match them)."
        ));
    }
    let optional = cm_ref.get("optional").and_then(Value::as_bool) == Some(true);
    match (kind, optional) {
        (RefKind::Hard, true) => Err(format!(
            "pod {pod_name}: env {key} is `optional: true`. It must be a HARD reference: a missing \
             key must be a loud CreateContainerConfigError, never a pod that starts on a value \
             nobody chose."
        )),
        (RefKind::Optional, false) => Err(format!(
            "pod {pod_name}: env {key} is a HARD reference, but it is a RETIRING key: step 1 of \
             its two-deploy retirement makes every ref `optional: true` so the later key deletion \
             cannot meet a hard ref in any revision `kubectl rollout undo` can reach."
        )),
        _ => Ok(()),
    }
}

/// `data[key]` of a fetched ConfigMap; `base` names it in the message.
///
/// # Errors
/// The key is absent — the `CreateContainerConfigError` case (for a hard ref)
/// or, for an optional ref, an env var silently unset while the current image
/// still requires it.
pub fn configmap_value(cm: &Value, base: &str, key: &str) -> Result<String, String> {
    cm.pointer("/data")
        .and_then(|d| d.get(key))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| {
            format!(
                "live ConfigMap {base} (-n {NAMESPACE}) has no key {key}. A pod holding a hard \
                 configMapKeyRef to it cannot start (CreateContainerConfigError, empty logs); \
                 under an OPTIONAL ref the variable is silently unset, which an image that still \
                 requires it refuses to boot on. Either the manifests were not applied, or the key \
                 was removed ahead of its refs — the rollback trap the two-deploy retirement \
                 exists to avoid."
            )
        })
}

/// [`configmap_value`] or a PRECONDITION panic naming the key.
///
/// # Panics
/// The key is absent.
#[must_use]
pub fn cm_value(cm: &Value, base: &str, key: &str) -> String {
    configmap_value(cm, base, key).unwrap_or_else(|e| panic!("{e}"))
}

/// PURE downward-API check: env `env_name` of `container_name` is a
/// `resourceFieldRef` to `expected_resource` (e.g. `limits.memory`) of
/// `expected_container`, with divisor `1` (or absent, which Kubernetes treats
/// as 1), and the target container DECLARES that resource.
///
/// # Errors
/// Each branch named; the undeclared-resource branch says why it matters.
pub fn check_resource_field_ref(
    pod: &Value,
    container_name: &str,
    env_name: &str,
    expected_container: &str,
    expected_resource: &str,
) -> Result<(), String> {
    let pod_name = pod
        .pointer("/metadata/name")
        .and_then(Value::as_str)
        .unwrap_or("<unnamed pod>");
    let entry = env_entry(pod, pod_name, container_name, env_name)?;
    let rf = entry
        .pointer("/valueFrom/resourceFieldRef")
        .ok_or_else(|| {
            format!(
                "pod {pod_name}: env {env_name} is not a downward-API resourceFieldRef; the limit \
                 must have one home (resources) and be injected, never restated."
            )
        })?;
    let resource = rf.get("resource").and_then(Value::as_str);
    if resource != Some(expected_resource) {
        return Err(format!(
            "pod {pod_name}: env {env_name} reads resource {resource:?}, expected \
             {expected_resource:?}."
        ));
    }
    let target = rf
        .get("containerName")
        .and_then(Value::as_str)
        .unwrap_or(container_name);
    if target != expected_container {
        return Err(format!(
            "pod {pod_name}: env {env_name} reads {expected_resource} of container {target:?}, \
             expected {expected_container:?}."
        ));
    }
    // Kubernetes serialises the divisor as a quantity string; absent means 1.
    let divisor = rf.get("divisor").map(|d| {
        d.as_str()
            .map(str::to_string)
            .unwrap_or_else(|| d.to_string())
    });
    if let Some(d) = divisor {
        if quantity_to_bytes(&d) != Ok(1) {
            return Err(format!(
                "pod {pod_name}: env {env_name} has divisor {d:?}; it must be 1 so the value is \
                 integer bytes (a larger divisor shrinks the limit the reader checks against)."
            ));
        }
    }
    let (section, res) = expected_resource.split_once('.').ok_or_else(|| {
        format!("expected resource {expected_resource:?} is not <section>.<name>")
    })?;
    let declared = container(pod, pod_name, target)?
        .get("resources")
        .and_then(|r| r.get(section))
        .and_then(|s| s.get(res))
        .and_then(Value::as_str);
    match declared {
        Some(q) => quantity_to_bytes(q).map(|_| ()).map_err(|e| {
            format!("pod {pod_name}: container {target} resources.{section}.{res}: {e}")
        }),
        None => Err(format!(
            "pod {pod_name}: container {target} does NOT declare resources.{section}.{res}, yet env \
             {env_name} reads it. Without the declared limit Kubernetes silently resolves the \
             variable to the NODE's allocatable value, so the reader's check passes vacuously \
             (fail-open). dt-guard kustomize's resource_field_ref_undeclared catches this in the \
             manifests; reaching here means the live cluster does not match them."
        )),
    }
}

/// A Kubernetes resource quantity in bytes: a plain integer, a binary suffix
/// (`Ki`, `Mi`, `Gi`, `Ti`) or a decimal one (`k`, `M`, `G`, `T`). Anything
/// else (fractions, `m`, exponents, other suffixes) is an error — never a
/// guess.
///
/// # Errors
/// An unsupported or malformed quantity, or overflow.
pub fn quantity_to_bytes(q: &str) -> Result<u64, String> {
    const SUFFIXES: &[(&str, u64)] = &[
        ("Ki", 1 << 10),
        ("Mi", 1 << 20),
        ("Gi", 1 << 30),
        ("Ti", 1 << 40),
        ("k", 1_000),
        ("M", 1_000_000),
        ("G", 1_000_000_000),
        ("T", 1_000_000_000_000),
    ];
    let (digits, mult) = SUFFIXES
        .iter()
        .find_map(|(s, m)| q.strip_suffix(s).map(|d| (d, *m)))
        .unwrap_or((q, 1));
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return Err(format!(
            "quantity {q:?} is not an integer with an optional Ki/Mi/Gi/Ti/k/M/G/T suffix"
        ));
    }
    let n: u64 = digits.parse().map_err(|e| format!("quantity {q:?}: {e}"))?;
    n.checked_mul(mult)
        .ok_or_else(|| format!("quantity {q:?} overflows u64 bytes"))
}

/// The newest live pod matching `instance=<instance>`, from
/// [`crate::fixtures::kube::live_pods_json`] (the one pod-list call).
///
/// Never skips: an empty pod set or an all-terminating one is a hard failure
/// with its own message. Terminating pods are ignored, and of the rest the
/// NEWEST is taken: these are 1-replica Deployments with
/// maxSurge=1 / maxUnavailable=0, so mid-rollout two pods can match with
/// neither terminating, and the surged (newer) one carries the config under
/// test. RFC 3339 timestamps sort lexicographically.
///
/// # Panics
/// As above, or when `kubectl` fails.
#[must_use]
pub fn fetch_live_pod(instance: &str) -> Value {
    let parsed = crate::fixtures::kube::live_pods_json(instance);
    newest_live_pod(&parsed, instance).unwrap_or_else(|e| panic!("PRECONDITION: {e}"))
}

/// The serving pod of one instance, as a metric pin needs it: the instance
/// label value, the pod name and its `status.podIP` (the Prometheus `instance`
/// label is `pod IP:port`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LivePod {
    /// The `instance` label (e.g. `mh-0`).
    pub instance: String,
    /// The serving pod's `metadata.name`.
    pub name: String,
    /// The serving pod's `status.podIP`.
    pub ip: String,
}

/// [`fetch_live_pod`] reduced to a [`LivePod`].
///
/// # Panics
/// As [`fetch_live_pod`], or the pod has no `status.podIP` (not yet scheduled).
#[must_use]
pub fn live_pod(instance: &str) -> LivePod {
    let pod = fetch_live_pod(instance);
    let name = pod_name_of(&pod, instance);
    let ip = pod
        .pointer("/status/podIP")
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("PRECONDITION: pod {name} ({instance}) has no status.podIP"))
        .to_string();
    LivePod {
        instance: instance.to_string(),
        name,
        ip,
    }
}

/// PURE core of [`fetch_live_pod`].
///
/// # Errors
/// No `items`, no pods, or every pod terminating.
pub fn newest_live_pod(pods: &Value, instance: &str) -> Result<Value, String> {
    let items = pods
        .get("items")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("kubectl output for {instance} has no `items` array"))?;
    if items.is_empty() {
        return Err(format!(
            "no pods matched -n {NAMESPACE} -l instance={instance}: the assertion would have \
             nothing to compare and would pass vacuously. Either the service is not deployed or \
             the label selector has drifted - fix the selector, do not delete the assertion."
        ));
    }
    items
        .iter()
        .filter(|p| p.pointer("/metadata/deletionTimestamp").is_none())
        .max_by_key(|p| {
            p.pointer("/metadata/creationTimestamp")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string()
        })
        .cloned()
        .ok_or_else(|| {
            format!(
                "every pod matching -n {NAMESPACE} -l instance={instance} is terminating ({} \
                 total). Nothing is serving this instance - the assertion must not pass vacuously.",
                items.len()
            )
        })
}

/// Fetches the named (LIVE, hash-suffixed) ConfigMap as parsed JSON. Reading
/// the cluster's copy — not a render — is the point.
///
/// # Panics
/// `kubectl` missing or failing, or unparseable output.
#[must_use]
pub fn fetch_configmap(name: &str) -> Value {
    let args = ["get", "configmap", name, "-n", NAMESPACE, "-o", "json"];
    let output = Command::new("kubectl")
        .args(args)
        .output()
        .unwrap_or_else(|e| {
            panic!(
                "kubectl not available - cannot read ConfigMap {name}. env-tests require kubectl \
             installed and configured: {e}"
            )
        });
    assert!(
        output.status.success(),
        "`kubectl {}` failed (exit status {}): {}",
        args.join(" "),
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!("could not parse `kubectl get configmap {name} -o json` output: {e}")
    })
}

// -----------------------------------------------------------------------------
// Tree-side ConfigMap data (the committed files a live value is compared to)
// -----------------------------------------------------------------------------

/// The Kind TEST codec profile patch (story 3 R-28) — a fixed repo-relative
/// path, resolved under [`crate::repo_root`] (from `CARGO_MANIFEST_DIR`, never
/// from the environment).
pub const KIND_TEST_PROFILE_PATCH: &str =
    "infra/kubernetes/overlays/kind/services/media-codec-profile/configmap-test-profile-patch.yaml";

/// The production codec profile: the generator env file the patch overrides.
pub const PROFILE_BASE_ENV: &str = "infra/services/media-codec-profile/profile.env";

/// The ONE restore command for the Kind sizing unit — the test codec profile
/// AND the egress budget, which roll back together. Verbatim the command in
/// [`KIND_TEST_PROFILE_PATCH`]'s header (step 3), and a hermetic test fails if
/// the two drift.
macro_rules! kind_sizing_restore_cmd {
    () => {
        "git checkout -- infra/kubernetes/overlays/kind/services/media-codec-profile/ \
infra/kubernetes/overlays/kind/services/mh-service/configmap-egress-budget-patch.yaml && dev-cluster deploy"
    };
}

/// See [`kind_sizing_restore_cmd`].
pub const KIND_SIZING_RESTORE_CMD: &str = kind_sizing_restore_cmd!();

/// The ONE recovery sentence every Kind profile/budget failure prints.
pub const KIND_SIZING_RESTORE: &str = concat!(
    "restore BOTH committed Kind patches and converge: `",
    kind_sizing_restore_cmd!(),
    "` (on the host, `infra/kind/scripts/deploy.sh` in place of `dev-cluster deploy`) — the \
     procedure and what a switch costs are in the Kind test-profile patch's header"
);

/// The `data` of a committed ConfigMap source, by file shape: a generator env
/// file (`*.env`: `KEY=VALUE` taken literally, ADR-0038 §2) or a
/// strategic-merge YAML patch (its `data` map, every value a string).
///
/// STRICT, so a malformed file can never make a comparison vacuous: in an env
/// file a non-comment line without `=`, an empty key or value, or a duplicate
/// key fails; in a patch a non-string value or a missing `data` map fails; and
/// either shape yielding ZERO keys fails.
///
/// # Errors
/// The file is unreadable or violates any rule above. The message names the
/// file and the key, never a value map.
pub fn tree_configmap_data(rel_path: &str) -> Result<BTreeMap<String, String>, String> {
    let path = crate::repo_root().join(rel_path);
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let data = if rel_path.ends_with(".env") {
        env_file_data(&text, rel_path)?
    } else {
        patch_data(&text, rel_path)?
    };
    if data.is_empty() {
        return Err(format!(
            "{rel_path} yields no keys: any comparison against it would pass vacuously"
        ));
    }
    Ok(data)
}

/// PURE core of [`tree_configmap_data`] for a generator env file.
///
/// # Errors
/// See [`tree_configmap_data`].
pub fn env_file_data(text: &str, home: &str) -> Result<BTreeMap<String, String>, String> {
    let mut data = BTreeMap::new();
    for (n, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| format!("{home}:{}: not a KEY=VALUE line", n + 1))?;
        if key.is_empty() || value.is_empty() {
            return Err(format!("{home}:{}: empty key or value for `{key}`", n + 1));
        }
        if data.insert(key.to_string(), value.to_string()).is_some() {
            return Err(format!("{home}:{}: duplicate key {key}", n + 1));
        }
    }
    Ok(data)
}

/// PURE core of [`tree_configmap_data`] for a strategic-merge ConfigMap patch.
///
/// # Errors
/// See [`tree_configmap_data`].
pub fn patch_data(text: &str, home: &str) -> Result<BTreeMap<String, String>, String> {
    let doc: serde_norway::Value =
        serde_norway::from_str(text).map_err(|e| format!("{home} is not valid YAML: {e}"))?;
    let map = doc
        .get("data")
        .and_then(serde_norway::Value::as_mapping)
        .ok_or_else(|| format!("{home} has no `data` map"))?;
    map.iter()
        .map(|(k, v)| {
            let key = k
                .as_str()
                .ok_or_else(|| format!("{home}: a `data` key is not a string"))?;
            let value = v
                .as_str()
                .ok_or_else(|| format!("{home}: data.{key} is not a string (quote it)"))?;
            Ok((key.to_string(), value.to_string()))
        })
        .collect()
}

/// One key a Kind patch overrides.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PatchOverride {
    /// The data key.
    pub key: String,
    /// The value the patch sets.
    pub patch: String,
    /// The value the base declares (always different from `patch`).
    pub base: String,
}

/// The ONE patch-vs-base rule for a Kind overlay patch that must change
/// values only: every patch key EXISTS in the base (a strategic merge is
/// additive by key name, so a key the base renamed would be dead and the base
/// value would win) and its value DIFFERS from the base (an unchanged value is
/// a no-op override). Used by the hermetic Layer-4 check and the live env-test.
///
/// # Errors
/// Either side unreadable/empty, a dead key, or a no-op key — each message
/// names `patch_rel`.
pub fn kind_patch_overrides(patch_rel: &str, base_rel: &str) -> Result<Vec<PatchOverride>, String> {
    let patch = tree_configmap_data(patch_rel)?;
    let base = tree_configmap_data(base_rel)?;
    patch_overrides(&patch, &base, patch_rel, base_rel)
}

/// PURE core of [`kind_patch_overrides`].
///
/// # Errors
/// See [`kind_patch_overrides`].
pub fn patch_overrides(
    patch: &BTreeMap<String, String>,
    base: &BTreeMap<String, String>,
    patch_rel: &str,
    base_rel: &str,
) -> Result<Vec<PatchOverride>, String> {
    patch
        .iter()
        .map(|(key, value)| match base.get(key) {
            None => Err(format!(
                "{patch_rel} sets {key}, which {base_rel} does not declare: a DEAD patch key (the \
                 base key was renamed or removed, so the strategic merge only ADDS this key and \
                 the base value wins in Kind). Make the patch follow the base key."
            )),
            Some(b) if b == value => Err(format!(
                "{patch_rel} sets {key} to its base value from {base_rel}: a no-op override. \
                 Remove it from the patch, or give it the intended value."
            )),
            Some(b) => Ok(PatchOverride {
                key: key.clone(),
                patch: value.clone(),
                base: b.clone(),
            }),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn pod(env: Value, resources: Value) -> Value {
        json!({
            "metadata": {"name": "mh-0-abc"},
            "spec": {"containers": [
                {"name": "mh-service", "env": env, "resources": resources},
                {"name": "sidecar"}
            ]}
        })
    }

    fn keyref(key: &str, cm: &str, ref_key: &str, optional: Option<bool>) -> Value {
        let mut r = json!({"name": cm, "key": ref_key});
        if let Some(o) = optional {
            r["optional"] = json!(o);
        }
        json!({"name": key, "valueFrom": {"configMapKeyRef": r}})
    }

    fn err(r: Result<(), String>) -> String {
        r.expect_err("expected an error")
    }

    #[test]
    fn key_ref_passes_hard_and_optional() {
        let p = pod(json!([keyref("K", "cm-abc", "K", None)]), json!({}));
        assert_eq!(
            check_key_ref(&p, "mh-service", "K", "cm-abc", RefKind::Hard),
            Ok(())
        );
        let p = pod(json!([keyref("K", "cm-abc", "K", Some(true))]), json!({}));
        assert_eq!(
            check_key_ref(&p, "mh-service", "K", "cm-abc", RefKind::Optional),
            Ok(())
        );
    }

    #[test]
    fn key_ref_absent_entry_names_the_key_and_the_per_workload_rule() {
        let p = pod(json!([]), json!({}));
        let e = err(check_key_ref(
            &p,
            "mh-service",
            "K",
            "cm-abc",
            RefKind::Hard,
        ));
        assert!(
            e.contains("no env entry K") && e.contains("per workload"),
            "{e}"
        );
    }

    #[test]
    fn key_ref_not_a_configmap_ref() {
        let p = pod(json!([{"name": "K", "value": "1"}]), json!({}));
        let e = err(check_key_ref(
            &p,
            "mh-service",
            "K",
            "cm-abc",
            RefKind::Hard,
        ));
        assert!(e.contains("env K is not a configMapKeyRef"), "{e}");
    }

    #[test]
    fn key_ref_wrong_configmap() {
        let p = pod(json!([keyref("K", "other-abc", "K", None)]), json!({}));
        let e = err(check_key_ref(
            &p,
            "mh-service",
            "K",
            "cm-abc",
            RefKind::Hard,
        ));
        assert!(
            e.contains("\"other-abc\"") && e.contains("\"cm-abc\""),
            "{e}"
        );
    }

    #[test]
    fn key_ref_crossed_key() {
        let p = pod(json!([keyref("K", "cm-abc", "J", None)]), json!({}));
        let e = err(check_key_ref(
            &p,
            "mh-service",
            "K",
            "cm-abc",
            RefKind::Hard,
        ));
        assert!(e.contains("CROSSED") && e.contains("env K"), "{e}");
    }

    #[test]
    fn key_ref_optionality_mismatch_in_both_directions() {
        let p = pod(json!([keyref("K", "cm-abc", "K", Some(true))]), json!({}));
        let e = err(check_key_ref(
            &p,
            "mh-service",
            "K",
            "cm-abc",
            RefKind::Hard,
        ));
        assert!(
            e.contains("env K is `optional: true`") && e.contains("HARD"),
            "{e}"
        );
        let p = pod(json!([keyref("K", "cm-abc", "K", Some(false))]), json!({}));
        let e = err(check_key_ref(
            &p,
            "mh-service",
            "K",
            "cm-abc",
            RefKind::Optional,
        ));
        assert!(e.contains("RETIRING key") && e.contains("env K"), "{e}");
    }

    #[test]
    fn shapes() {
        assert!(check_shape("K", "1", ValueShape::PositiveInteger, "h").is_ok());
        assert!(check_shape("K", "0", ValueShape::PositiveInteger, "h").is_err());
        assert!(check_shape("K", "0", ValueShape::NonNegativeInteger, "h").is_ok());
        assert!(check_shape("K", "-1", ValueShape::NonNegativeInteger, "h").is_err());
        assert!(check_shape("K", "vp8", ValueShape::Token, "h").is_ok());
        for bad in ["", "v p8", "\"vp8\""] {
            assert!(
                check_shape("K", bad, ValueShape::Token, "h").is_err(),
                "{bad:?}"
            );
        }
        assert!(check_shape("K", "0.5", ValueShape::UnitRatio, "h").is_ok());
        assert!(check_shape("K", "1.5", ValueShape::UnitRatio, "h").is_err());
        let e = check_shape("MY_KEY", "x", ValueShape::PositiveInteger, "my-cm").expect_err("bad");
        assert!(e.contains("MY_KEY") && e.contains("my-cm"), "{e}");
    }

    #[test]
    fn configmap_value_absent_key_explains_optional_too() {
        let cm = json!({"data": {"A": "1"}});
        assert_eq!(configmap_value(&cm, "base", "A"), Ok("1".to_string()));
        let e = configmap_value(&cm, "base", "B").expect_err("absent");
        assert!(e.contains("no key B") && e.contains("OPTIONAL"), "{e}");
    }

    fn mem_ref(rf: Value) -> Value {
        json!([{"name": "MH_POD_MEMORY_LIMIT_BYTES", "valueFrom": {"resourceFieldRef": rf}}])
    }

    const MEM: &str = "MH_POD_MEMORY_LIMIT_BYTES";

    #[test]
    fn resource_field_ref_passes_with_divisor_one_or_absent() {
        let lim = json!({"limits": {"memory": "1Gi"}});
        for rf in [
            json!({"containerName": "mh-service", "resource": "limits.memory", "divisor": "1"}),
            json!({"containerName": "mh-service", "resource": "limits.memory"}),
            json!({"resource": "limits.memory"}),
        ] {
            let p = pod(mem_ref(rf), lim.clone());
            assert_eq!(
                check_resource_field_ref(&p, "mh-service", MEM, "mh-service", "limits.memory"),
                Ok(())
            );
        }
    }

    #[test]
    fn resource_field_ref_wrong_divisor_fails() {
        let p = pod(
            mem_ref(
                json!({"containerName": "mh-service", "resource": "limits.memory", "divisor": "1Mi"}),
            ),
            json!({"limits": {"memory": "1Gi"}}),
        );
        let e = err(check_resource_field_ref(
            &p,
            "mh-service",
            MEM,
            "mh-service",
            "limits.memory",
        ));
        assert!(e.contains("divisor \"1Mi\""), "{e}");
    }

    #[test]
    fn resource_field_ref_undeclared_limit_explains_fail_open() {
        let p = pod(
            mem_ref(json!({"containerName": "mh-service", "resource": "limits.memory"})),
            json!({"requests": {"memory": "512Mi"}}),
        );
        let e = err(check_resource_field_ref(
            &p,
            "mh-service",
            MEM,
            "mh-service",
            "limits.memory",
        ));
        assert!(
            e.contains("does NOT declare resources.limits.memory") && e.contains("NODE"),
            "{e}"
        );
    }

    #[test]
    fn resource_field_ref_wrong_resource_or_container() {
        let lim = json!({"limits": {"memory": "1Gi"}});
        let p = pod(mem_ref(json!({"resource": "requests.memory"})), lim.clone());
        assert!(err(check_resource_field_ref(
            &p,
            "mh-service",
            MEM,
            "mh-service",
            "limits.memory"
        ))
        .contains("requests.memory"));
        let p = pod(
            mem_ref(json!({"containerName": "sidecar", "resource": "limits.memory"})),
            lim,
        );
        assert!(err(check_resource_field_ref(
            &p,
            "mh-service",
            MEM,
            "mh-service",
            "limits.memory"
        ))
        .contains("\"sidecar\""));
        let p = pod(json!([{"name": MEM, "value": "1073741824"}]), json!({}));
        assert!(err(check_resource_field_ref(
            &p,
            "mh-service",
            MEM,
            "mh-service",
            "limits.memory"
        ))
        .contains("not a downward-API resourceFieldRef"));
    }

    #[test]
    fn newest_live_pod_skips_terminating_and_takes_newest() {
        let pods = json!({"items": [
            {"metadata": {"name": "old", "creationTimestamp": "2026-10-01T00:00:00Z"}},
            {"metadata": {"name": "new", "creationTimestamp": "2026-10-02T00:00:00Z"}},
            {"metadata": {"name": "dying", "creationTimestamp": "2026-10-03T00:00:00Z",
                          "deletionTimestamp": "2026-10-03T00:01:00Z"}}
        ]});
        let p = newest_live_pod(&pods, "mh-0").expect("live");
        assert_eq!(
            p.pointer("/metadata/name").and_then(Value::as_str),
            Some("new")
        );
        assert!(newest_live_pod(&json!({"items": []}), "mh-0")
            .expect_err("empty")
            .contains("vacuously"));
        let dying = json!({"items": [{"metadata": {"name": "d", "deletionTimestamp": "x"}}]});
        assert!(newest_live_pod(&dying, "mh-0")
            .expect_err("dying")
            .contains("terminating"));
        assert!(newest_live_pod(&json!({}), "mh-0").is_err());
    }

    #[test]
    fn quantities() {
        assert_eq!(quantity_to_bytes("1Gi"), Ok(1 << 30));
        assert_eq!(quantity_to_bytes("512Mi"), Ok(512 << 20));
        assert_eq!(quantity_to_bytes("4Ki"), Ok(4096));
        assert_eq!(quantity_to_bytes("2Ti"), Ok(2 << 40));
        assert_eq!(quantity_to_bytes("1k"), Ok(1000));
        assert_eq!(quantity_to_bytes("3M"), Ok(3_000_000));
        assert_eq!(quantity_to_bytes("1G"), Ok(1_000_000_000));
        assert_eq!(quantity_to_bytes("1073741824"), Ok(1_073_741_824));
        assert_eq!(quantity_to_bytes("1"), Ok(1));
        for bad in ["", "Gi", "1.5Gi", "100m", "1e3", "1Pi", "-1", " 1"] {
            assert!(
                quantity_to_bytes(bad).is_err(),
                "{bad:?} must not be guessed"
            );
        }
        assert!(quantity_to_bytes("99999999999999Ti").is_err(), "overflow");
    }

    fn map(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    #[test]
    fn env_file_data_is_strict() {
        let ok = env_file_data("# c\n\nA=1\nB=x=y\n", "f.env").unwrap();
        assert_eq!(ok, map(&[("A", "1"), ("B", "x=y")]));
        assert!(env_file_data("A\n", "f.env")
            .unwrap_err()
            .contains("not a KEY=VALUE"));
        assert!(env_file_data("A=\n", "f.env")
            .unwrap_err()
            .contains("empty key or value"));
        assert!(env_file_data("=1\n", "f.env")
            .unwrap_err()
            .contains("empty key or value"));
        assert!(env_file_data("A=1\nA=2\n", "f.env")
            .unwrap_err()
            .contains("duplicate key A"));
    }

    #[test]
    fn patch_data_is_strict() {
        let ok = patch_data("data:\n  A: \"1\"\n", "p.yaml").unwrap();
        assert_eq!(ok, map(&[("A", "1")]));
        assert!(patch_data("data:\n  A: 1\n", "p.yaml")
            .unwrap_err()
            .contains("not a string"));
        assert!(patch_data("kind: ConfigMap\n", "p.yaml")
            .unwrap_err()
            .contains("no `data` map"));
        assert!(patch_data(": : :", "p.yaml")
            .unwrap_err()
            .contains("not valid YAML"));
    }

    #[test]
    fn patch_overrides_rejects_dead_and_noop_keys() {
        let base = map(&[("A", "1"), ("B", "2")]);
        assert_eq!(
            patch_overrides(&map(&[("A", "9")]), &base, "p", "b").unwrap(),
            vec![PatchOverride {
                key: "A".to_string(),
                patch: "9".to_string(),
                base: "1".to_string()
            }]
        );
        assert!(patch_overrides(&map(&[("C", "9")]), &base, "p", "b")
            .unwrap_err()
            .contains("DEAD patch key"));
        assert!(patch_overrides(&map(&[("B", "2")]), &base, "p", "b")
            .unwrap_err()
            .contains("no-op override"));
    }

    /// Hermetic twin of `01::test_kind_media_codec_profile_is_the_committed_test_profile`
    /// (story 3 R-28): the COMMITTED Kind test-profile patch overrides only keys
    /// the base profile declares, changes every one, and leaves audio alone.
    #[test]
    fn the_kind_test_profile_patch_overrides_base_keys_only() {
        let overrides = kind_patch_overrides(KIND_TEST_PROFILE_PATCH, PROFILE_BASE_ENV)
            .unwrap_or_else(|e| panic!("{e}; {KIND_SIZING_RESTORE}"));
        assert!(!overrides.is_empty());
        for PatchOverride { key, .. } in &overrides {
            assert!(
                !key.starts_with("MEDIA_AUDIO_"),
                "{KIND_TEST_PROFILE_PATCH} overrides {key}: the Kind test profile changes video \
                 only (R-28) — every MEDIA_AUDIO_* key is inherited from {PROFILE_BASE_ENV}"
            );
        }
    }

    /// The restore command every failure prints is verbatim the one the
    /// Kind test-profile patch header documents (two encodings, one command).
    #[test]
    fn the_restore_command_matches_the_patch_header() {
        let path = crate::repo_root().join(KIND_TEST_PROFILE_PATCH);
        let header =
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert!(
            header.contains("PRODUCTION-QUALITY DEMO"),
            "wrong file read"
        );
        assert!(
            header.contains(KIND_SIZING_RESTORE_CMD),
            "{KIND_TEST_PROFILE_PATCH}'s header no longer carries `{KIND_SIZING_RESTORE_CMD}` \
             verbatim: update KIND_SIZING_RESTORE_CMD and the header together"
        );
    }
}
