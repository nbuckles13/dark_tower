//! The MH pool as `infra/kind/scripts/mh.sh list --json` reports it (ADR-0039
//! §7; machine contract
//! `docs/debates/2026-10-28-mh-replacement-and-migration/mh-sh-contract.txt`
//! §4). Story 4 I-2 lands the READ half only ([`list`]); T-1 extends this
//! module with `up()` / `MhSlot::drain()` / `assert_baseline()` (contract §8).
//!
//! # The mh.sh path has ONE home
//!
//! `scripts/layer7.sh` exports [`DT_MH_SH_ENV`] (an absolute path). This
//! fixture never guesses a fallback path: unset, relative or missing is a loud
//! PRECONDITION panic naming layer7.sh (contract §8, "fails loudly when it is
//! unset"). mh.sh selects the cluster itself (`kind-<DT_CLUSTER_NAME>`, never
//! the current context), so a caller comparing it with `kubectl` reads (which
//! use the current context) must cross-check the two pod sets — see
//! `tests/42_mh_address_invariant.rs`.
//!
//! # What `list` is evidence OF
//!
//! `address` and `host_port` are copied by mh.sh from the per-slot ConfigMaps
//! (mh.sh never computes an address). Comparing `address` against the slot
//! ConfigMap is therefore the same value read twice — a tautology. Use `list`
//! to ENUMERATE the pool and name pods; take evidence about what a pod actually
//! runs from the pod (`kubectl`) or the process (its logs, its metrics).

use std::path::PathBuf;
use std::process::Command;

use serde::Deserialize;
use serde_json::Value;

/// The env var `scripts/layer7.sh` exports with mh.sh's absolute path.
pub const DT_MH_SH_ENV: &str = "DT_MH_SH";

/// The `list --json` schema version this reader understands (contract §4;
/// additive-only, so a higher version is refused rather than half-read).
pub const SCHEMA_VERSION: u64 = 1;

/// The per-slot key set of `list --json` v1, in mh.sh's order. Pinned against
/// mh.sh's `MH_JQ_PUBLIC` by a hermetic unit test below (one home: mh.sh).
/// Every key must be PRESENT (nullable keys as `null`): serde would otherwise
/// read a dropped key as `None`, i.e. "no pod", silently.
pub const SLOT_KEYS: &[&str] = &[
    "slot",
    "host_port",
    "address",
    "pod_ip",
    "pod_uid",
    "state",
    "terminating_past_grace",
    "deployment",
    "pod",
    "revision",
    "stale",
    "owner",
    "restart_count",
];

/// A slot's state (contract §4).
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SlotState {
    Free,
    Starting,
    Serving,
    Draining,
    Terminating,
}

/// One `slots[]` entry of `list --json` v1.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct SlotEntry {
    pub slot: u32,
    pub host_port: u16,
    pub address: String,
    pub pod_ip: Option<String>,
    pub pod_uid: Option<String>,
    pub state: SlotState,
    pub terminating_past_grace: bool,
    pub deployment: Option<String>,
    pub pod: Option<String>,
    pub revision: Option<String>,
    pub stale: bool,
    pub owner: Option<String>,
    pub restart_count: Option<u32>,
}

/// The whole `list --json` document (v1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PoolSnapshot {
    pub template_revision: String,
    pub baseline: u32,
    pub slots: Vec<SlotEntry>,
}

impl PoolSnapshot {
    /// Entries that have a pod (any state but `free`).
    #[must_use]
    pub fn occupied(&self) -> Vec<&SlotEntry> {
        self.slots.iter().filter(|s| s.pod.is_some()).collect()
    }

    /// Entries in state `serving`.
    #[must_use]
    pub fn serving(&self) -> Vec<&SlotEntry> {
        self.slots
            .iter()
            .filter(|s| s.state == SlotState::Serving)
            .collect()
    }
}

/// mh.sh's absolute path, from [`DT_MH_SH_ENV`].
///
/// # Panics
/// PRECONDITION: unset, empty, relative, or not a file.
#[must_use]
pub fn mh_sh_path() -> PathBuf {
    mh_sh_path_from(std::env::var_os(DT_MH_SH_ENV).map(PathBuf::from))
        .unwrap_or_else(|e| panic!("PRECONDITION (mh-sh-unset): {e}"))
}

/// PURE core of [`mh_sh_path`].
///
/// # Errors
/// Unset/empty, relative, or not an existing file — each named.
pub fn mh_sh_path_from(value: Option<PathBuf>) -> Result<PathBuf, String> {
    let hint = "scripts/layer7.sh exports it (an absolute path to infra/kind/scripts/mh.sh); \
                outside Layer 7 set it yourself, e.g. DT_MH_SH=\"$PWD/infra/kind/scripts/mh.sh\"";
    let path = value
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or_else(|| format!("{DT_MH_SH_ENV} is unset or empty; {hint}"))?;
    if !path.is_absolute() {
        return Err(format!(
            "{DT_MH_SH_ENV}={} is not absolute; {hint}",
            path.display()
        ));
    }
    if !path.is_file() {
        return Err(format!(
            "{DT_MH_SH_ENV}={} is not an existing file; {hint}",
            path.display()
        ));
    }
    Ok(path)
}

/// `mh.sh list --json`, parsed and validated ([`parse_list`]).
///
/// # Panics
/// PRECONDITION: mh.sh cannot run, exits non-zero (the final `MH_RESULT` line
/// is quoted), or prints a document [`parse_list`] refuses.
#[must_use]
pub fn list() -> PoolSnapshot {
    let path = mh_sh_path();
    let out = Command::new(&path)
        .args(["list", "--json"])
        .output()
        .unwrap_or_else(|e| {
            panic!(
                "PRECONDITION (mh-sh-list): cannot run {}: {e}",
                path.display()
            )
        });
    let stderr = String::from_utf8_lossy(&out.stderr);
    let result_line = stderr
        .lines()
        .rev()
        .find(|l| l.starts_with("MH_RESULT "))
        .unwrap_or("(no MH_RESULT line)");
    assert!(
        out.status.success(),
        "PRECONDITION (mh-sh-list): `{} list --json` exited {}: {result_line}",
        path.display(),
        out.status
    );
    parse_list(&String::from_utf8_lossy(&out.stdout))
        .unwrap_or_else(|e| panic!("PRECONDITION (mh-sh-list): {e} ({result_line})"))
}

/// PURE: parse a `list --json` stdout document.
///
/// # Errors
/// Not one JSON object; `ok` not `true`; `schema_version` not
/// [`SCHEMA_VERSION`]; a slot entry missing a [`SLOT_KEYS`] key; zero slots
/// (mh.sh's own contract is that every slot always appears, so zero means the
/// read observed nothing); duplicate slot numbers; or a typed field of the
/// wrong shape.
pub fn parse_list(stdout: &str) -> Result<PoolSnapshot, String> {
    let doc: Value = serde_json::from_str(stdout.trim())
        .map_err(|e| format!("mh.sh list --json stdout is not one JSON object: {e}"))?;
    if doc.get("ok") != Some(&Value::Bool(true)) {
        return Err("mh.sh list --json: `ok` is not true".to_string());
    }
    match doc.get("schema_version").and_then(Value::as_u64) {
        Some(SCHEMA_VERSION) => {}
        other => {
            return Err(format!(
                "mh.sh list --json: schema_version {other:?}, this reader understands \
                 {SCHEMA_VERSION}"
            ))
        }
    }
    let template_revision = doc
        .get("template_revision")
        .and_then(Value::as_str)
        .filter(|r| !r.is_empty())
        .ok_or("mh.sh list --json: no template_revision")?
        .to_string();
    let baseline = doc
        .get("baseline")
        .and_then(Value::as_u64)
        .and_then(|b| u32::try_from(b).ok())
        .ok_or("mh.sh list --json: baseline is not a non-negative integer")?;
    let raw = doc
        .get("slots")
        .and_then(Value::as_array)
        .ok_or("mh.sh list --json: no slots array")?;
    if raw.is_empty() {
        return Err("mh.sh list --json: zero slots (every slot always appears)".to_string());
    }
    let mut slots = Vec::with_capacity(raw.len());
    for (i, entry) in raw.iter().enumerate() {
        let obj = entry
            .as_object()
            .ok_or_else(|| format!("mh.sh list --json: slots[{i}] is not an object"))?;
        if let Some(missing) = SLOT_KEYS.iter().find(|k| !obj.contains_key(**k)) {
            return Err(format!(
                "mh.sh list --json: slots[{i}] has no `{missing}` key (schema v1 pins the key set)"
            ));
        }
        let parsed: SlotEntry = serde_json::from_value(entry.clone())
            .map_err(|e| format!("mh.sh list --json: slots[{i}] has a malformed field: {e}"))?;
        slots.push(parsed);
    }
    let mut numbers: Vec<u32> = slots.iter().map(|s| s.slot).collect();
    numbers.sort_unstable();
    numbers.dedup();
    if numbers.len() != slots.len() {
        return Err("mh.sh list --json: duplicate slot numbers".to_string());
    }
    Ok(PoolSnapshot {
        template_revision,
        baseline,
        slots,
    })
}

/// The ConfigMap deploy.sh stores the rendered MH template in (contract §4,
/// "CLUSTER-SIDE INTERFACE"): data keys `deployment.json`, `revision`,
/// `baseline`. Data, never pod-consumed.
pub const TEMPLATE_CONFIGMAP: &str = "mh-template";

/// The rendered MH Deployment template, read from the LIVE
/// [`TEMPLATE_CONFIGMAP`] (what mh.sh instantiates every MH from).
///
/// # Panics
/// PRECONDITION: the ConfigMap is unreadable, or `deployment.json` is absent
/// or not a JSON object.
#[must_use]
pub fn template_deployment() -> Value {
    let cm = crate::fixtures::kube::kubectl_get_json(&[
        "configmap",
        TEMPLATE_CONFIGMAP,
        "-n",
        crate::NAMESPACE,
    ])
    .unwrap_or_else(|e| panic!("PRECONDITION (mh-template): {e}"));
    template_from_configmap(&cm).unwrap_or_else(|e| panic!("PRECONDITION (mh-template): {e}"))
}

/// PURE core of [`template_deployment`].
///
/// # Errors
/// No `data.deployment.json`, unparseable, or not a Deployment with a pod
/// template spec.
pub fn template_from_configmap(cm: &Value) -> Result<Value, String> {
    let raw = cm
        .pointer("/data/deployment.json")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("ConfigMap {TEMPLATE_CONFIGMAP} has no data key deployment.json"))?;
    let dep: Value = serde_json::from_str(raw)
        .map_err(|e| format!("{TEMPLATE_CONFIGMAP} deployment.json is not JSON: {e}"))?;
    if dep.get("kind").and_then(Value::as_str) != Some("Deployment")
        || dep.pointer("/spec/template/spec").is_none()
    {
        return Err(format!(
            "{TEMPLATE_CONFIGMAP} deployment.json is not a Deployment with spec.template.spec"
        ));
    }
    Ok(dep)
}

/// PURE: the template's pod template as a POD-SHAPED object
/// (`{metadata: {name, labels}, spec}`), so every per-pod check in
/// `fixtures::deployment_config` applies to the template unchanged.
///
/// # Panics
/// `dep` was not validated by [`template_from_configmap`].
#[must_use]
pub fn template_as_pod(dep: &Value) -> Value {
    serde_json::json!({
        "metadata": {
            "name": format!("configmap/{TEMPLATE_CONFIGMAP}:deployment.json"),
            "labels": dep.pointer("/spec/template/metadata/labels").cloned().unwrap_or(Value::Null),
        },
        "spec": dep.pointer("/spec/template/spec").cloned().expect("validated template"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn entry(slot: u32, state: &str, pod: Option<&str>) -> Value {
        json!({"slot": slot, "host_port": 4440 + slot, "address": format!("https://127.0.0.1:{}", 4440 + slot),
               "pod_ip": pod.map(|_| "10.244.0.9"), "pod_uid": pod.map(|_| "u"), "state": state,
               "terminating_past_grace": false, "deployment": pod.map(|_| format!("mh-s{slot}")),
               "pod": pod, "revision": pod.map(|_| "r1"), "stale": false,
               "owner": pod.map(|_| "baseline"), "restart_count": pod.map(|_| 0)})
    }

    fn doc(slots: Vec<Value>) -> Value {
        json!({"ok": true, "schema_version": 1, "template_revision": "r1", "baseline": 2, "slots": slots})
    }

    #[test]
    fn a_v1_document_parses_with_occupied_and_serving_views() {
        let d = doc(vec![
            entry(0, "serving", Some("mh-s0-a")),
            entry(1, "free", None),
            entry(2, "starting", Some("mh-s2-b")),
        ]);
        let p = parse_list(&d.to_string()).expect("parses");
        assert_eq!(p.baseline, 2);
        assert_eq!(p.template_revision, "r1");
        assert_eq!(p.slots.len(), 3);
        assert_eq!(p.occupied().len(), 2);
        assert_eq!(p.serving().len(), 1);
        assert_eq!(p.serving()[0].slot, 0);
    }

    #[test]
    fn refusals_are_each_named() {
        let mut not_ok = doc(vec![entry(0, "free", None)]);
        not_ok["ok"] = json!(false);
        assert!(parse_list(&not_ok.to_string())
            .unwrap_err()
            .contains("`ok`"));

        let mut v2 = doc(vec![entry(0, "free", None)]);
        v2["schema_version"] = json!(2);
        assert!(parse_list(&v2.to_string())
            .unwrap_err()
            .contains("schema_version"));

        assert!(parse_list(&doc(vec![]).to_string())
            .unwrap_err()
            .contains("zero slots"));

        let mut dropped = entry(0, "free", None);
        dropped.as_object_mut().unwrap().remove("pod");
        assert!(parse_list(&doc(vec![dropped]).to_string())
            .unwrap_err()
            .contains("no `pod` key"));

        let dup = doc(vec![entry(1, "free", None), entry(1, "free", None)]);
        assert!(parse_list(&dup.to_string())
            .unwrap_err()
            .contains("duplicate"));

        let bad_state = doc(vec![entry(0, "zombie", None)]);
        assert!(parse_list(&bad_state.to_string())
            .unwrap_err()
            .contains("malformed"));

        assert!(parse_list("not json").unwrap_err().contains("not one JSON"));
    }

    #[test]
    fn template_is_read_from_deployment_json_and_projected_as_a_pod() {
        let dep = json!({"kind": "Deployment", "spec": {"template": {
            "metadata": {"labels": {"app": "mh-service"}},
            "spec": {"terminationGracePeriodSeconds": 35}}}});
        let cm =
            json!({"data": {"deployment.json": dep.to_string(), "revision": "r", "baseline": "2"}});
        let got = template_from_configmap(&cm).expect("valid");
        let pod = template_as_pod(&got);
        assert_eq!(
            pod.pointer("/spec/terminationGracePeriodSeconds"),
            Some(&json!(35))
        );
        assert_eq!(
            pod.pointer("/metadata/labels/app"),
            Some(&json!("mh-service"))
        );
        assert!(template_from_configmap(&json!({"data": {}}))
            .unwrap_err()
            .contains("no data key"));
        let svc = json!({"data": {"deployment.json": "{\"kind\":\"Service\"}"}});
        assert!(template_from_configmap(&svc)
            .unwrap_err()
            .contains("not a Deployment"));
    }

    #[test]
    fn mh_sh_path_refuses_unset_relative_and_missing() {
        assert!(mh_sh_path_from(None).unwrap_err().contains("unset"));
        assert!(mh_sh_path_from(Some(PathBuf::new()))
            .unwrap_err()
            .contains("unset"));
        assert!(mh_sh_path_from(Some("infra/kind/scripts/mh.sh".into()))
            .unwrap_err()
            .contains("not absolute"));
        assert!(mh_sh_path_from(Some("/nonexistent/mh.sh".into()))
            .unwrap_err()
            .contains("not an existing file"));
        let real = crate::repo_root().join("infra/kind/scripts/mh.sh");
        assert_eq!(mh_sh_path_from(Some(real.clone())).unwrap(), real);
    }

    /// [`SLOT_KEYS`] == the key list of mh.sh's `MH_JQ_PUBLIC` (the ONE home
    /// of the v1 slot schema), read from the committed script.
    #[test]
    fn slot_keys_match_mh_sh_public_schema() {
        let text = std::fs::read_to_string(crate::repo_root().join("infra/kind/scripts/mh.sh"))
            .expect("read mh.sh");
        let line = text
            .lines()
            .find(|l| l.starts_with("readonly MH_JQ_PUBLIC="))
            .expect("mh.sh defines MH_JQ_PUBLIC");
        let inner = line
            .split_once("def public: {")
            .and_then(|(_, rest)| rest.split_once('}'))
            .map(|(keys, _)| keys)
            .expect("MH_JQ_PUBLIC has the `def public: {...}` shape");
        let keys: Vec<&str> = inner.split(',').map(str::trim).collect();
        assert!(keys.len() > 5, "extraction produced too few keys: {keys:?}");
        assert_eq!(
            keys, SLOT_KEYS,
            "mh.sh's list --json slot key set drifted from SLOT_KEYS"
        );
    }
}
