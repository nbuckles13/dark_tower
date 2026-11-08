//! R-28: the MH address invariant (ADR-0039 §7 "Kind"): every live MH
//! incarnation owns a unique client-facing address — its slot's — for its
//! whole life. The PURE verdict for `tests/42_mh_address_invariant.rs`, so it is
//! unit-tested at Layer 4 against adverse inputs (every violation token below
//! has a case that makes it fire).
//!
//! # Evidence, and why it is not a tautology
//!
//! `mh.sh list --json` copies `address`/`host_port` from the slot ConfigMaps,
//! so it is used ONLY to enumerate (and is then cross-checked against
//! `kubectl`). Each pod's address is established from sources mh.sh does not
//! write into its own report:
//!
//! 1. the pod object (kubectl): its `dark-tower.io/mh-slot` label, the
//!    `configMapKeyRef` its advertise env var actually resolves through, and its
//!    webtransport `hostPort`;
//! 2. the slot ConfigMaps, read by label with kubectl (not via mh.sh);
//! 3. the MH PROCESS: the `webtransport_advertise_address` field of its startup
//!    log line (`crates/mh-service/src/main.rs`, "Starting Media Handler"
//!    fields) — what MH registers with GC and hence what clients are handed.
//!
//! Whether that address actually ROUTES to that pod is R-27
//! (`tests/43_mh_slot_publish_udp.rs`).
//!
//! # Tokens
//!
//! Every violation starts `R28 <token>:` with a token from [`TOKENS`]; the
//! vacuity cases (no pods, no slot ConfigMaps, an empty policy selector) are
//! PRECONDITIONs in the caller and the parse helpers, never violations, so a
//! "nothing to check" result cannot be triaged as a content bug.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::fixtures::mh_pool::{PoolSnapshot, SlotState};

/// The MH container (template invariant; also the restart-alert key).
pub const MH_CONTAINER: &str = "mh-service";
/// The webtransport container port's name (mh.sh sets its `hostPort`).
pub const WT_PORT_NAME: &str = "webtransport";
/// The advertise env var == the slot ConfigMap key carrying the address.
pub const ADDRESS_KEY: &str = "MH_WEBTRANSPORT_ADVERTISE_ADDRESS";
/// The slot ConfigMap key carrying the slot's host (== node == pod hostPort) port.
pub const HOST_PORT_KEY: &str = "MH_HOST_PORT";
/// The label every slot ConfigMap and every pool pod carries.
pub const SLOT_LABEL: &str = "dark-tower.io/mh-slot";
/// The startup-log field (tracing JSON, under `fields`).
pub const LOG_FIELD: &str = "webtransport_advertise_address";

/// Every violation token, ONE list (a unit test pins that each fires).
pub const TOKENS: &[&str] = &[
    "pod-set-mismatch",
    "not-serving",
    "slot-label",
    "list-mismatch",
    "slot-configmap-missing",
    "ref-mismatch",
    "hostport-mismatch",
    "address-port",
    "advertised-mismatch",
    "duplicate-address",
    "duplicate-hostport",
    "policy-labels",
];

/// One slot ConfigMap, as kubectl reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlotConfigMap {
    pub slot: u32,
    pub name: String,
    pub address: String,
    pub host_port: u16,
}

/// PURE: `kubectl get configmaps -l dark-tower.io/mh-slot -o json` -> one
/// [`SlotConfigMap`] per slot.
///
/// # Errors
/// No `items`, ZERO ConfigMaps (the read observed nothing), a non-decimal slot
/// label, a missing/invalid key, or two ConfigMaps for one slot (mh.sh's
/// `no-template` case; a retired generation left labelled).
pub fn slot_configmaps(doc: &Value) -> Result<BTreeMap<u32, SlotConfigMap>, String> {
    let items = doc
        .get("items")
        .and_then(Value::as_array)
        .ok_or("slot ConfigMap list has no `items`")?;
    if items.is_empty() {
        return Err(format!(
            "no ConfigMap labelled {SLOT_LABEL}: deploy.sh renders one per slot"
        ));
    }
    let mut out = BTreeMap::new();
    for cm in items {
        let name = cm
            .pointer("/metadata/name")
            .and_then(Value::as_str)
            .ok_or("a slot ConfigMap has no metadata.name")?
            .to_string();
        let slot = label(cm, SLOT_LABEL)
            .and_then(|s| s.parse::<u32>().ok())
            .ok_or_else(|| format!("ConfigMap {name}: {SLOT_LABEL} is not a slot number"))?;
        let data = |k: &str| {
            cm.pointer("/data")
                .and_then(|d| d.get(k))
                .and_then(Value::as_str)
                .filter(|v| !v.is_empty())
                .map(str::to_string)
                .ok_or_else(|| format!("ConfigMap {name} (slot {slot}) has no {k}"))
        };
        let address = data(ADDRESS_KEY)?;
        let host_port = data(HOST_PORT_KEY)?
            .parse::<u16>()
            .ok()
            .filter(|p| *p > 0)
            .ok_or_else(|| {
                format!("ConfigMap {name} (slot {slot}): {HOST_PORT_KEY} is not a port")
            })?;
        if let Some(prev) = out.insert(
            slot,
            SlotConfigMap {
                slot,
                name: name.clone(),
                address,
                host_port,
            },
        ) {
            return Err(format!(
                "slot {slot} has two ConfigMaps ({} and {name}); exactly one per slot",
                prev.name
            ));
        }
    }
    Ok(out)
}

/// PURE: the `(host, port)` of an `https://host:port[/...]` address. IPv6
/// hosts lose their brackets. The port is REQUIRED: a portless address is an
/// error, never a default (a client would dial 443, a port no slot owns).
///
/// # Errors
/// Not `https://`, an empty host, or no numeric non-zero port.
pub fn address_authority(address: &str) -> Result<(String, u16), String> {
    let rest = address
        .strip_prefix("https://")
        .ok_or_else(|| format!("address {address:?} is not https://"))?;
    let authority = rest.split('/').next().unwrap_or(rest);
    let (host, port) = authority
        .rsplit_once(':')
        .filter(|(host, _)| !host.ends_with(':'))
        .ok_or_else(|| format!("address {address:?} has no port"))?;
    let port = port
        .parse::<u16>()
        .ok()
        .filter(|p| *p > 0)
        .ok_or_else(|| format!("address {address:?} has no port"))?;
    let host = if let Some(inner) = host.strip_prefix('[') {
        inner
            .strip_suffix(']')
            .ok_or_else(|| format!("address {address:?} has an unclosed IPv6 bracket"))?
    } else if host.contains(':') {
        return Err(format!(
            "address {address:?}: an IPv6 host must be bracketed"
        ));
    } else {
        host
    };
    if host.is_empty() {
        return Err(format!("address {address:?} has an empty host"));
    }
    Ok((host.to_string(), port))
}

/// PURE: the port of an address ([`address_authority`]).
///
/// # Errors
/// As [`address_authority`].
pub fn address_port(address: &str) -> Result<u16, String> {
    address_authority(address).map(|(_, port)| port)
}

/// PURE: the address the MH process logged at startup. Every JSON log line
/// carrying `fields.webtransport_advertise_address` is read (non-JSON lines are
/// skipped); they must agree (one container's log is one process).
///
/// # Errors
/// `startup-line-missing` (no such line: the log was rotated, or the startup
/// line changed shape) or `startup-line-ambiguous` (two values) — distinct
/// from an address MISMATCH, which is a violation.
pub fn advertised_in_log(log: &str) -> Result<String, String> {
    let values: BTreeSet<String> = log
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter_map(|v| {
            v.pointer(&format!("/fields/{LOG_FIELD}"))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect();
    match values.len() {
        1 => Ok(values.into_iter().next().expect("one")),
        0 => Err(format!(
            "startup-line-missing: no JSON log line carries fields.{LOG_FIELD} (the MH startup \
             line, crates/mh-service/src/main.rs). Either kubelet rotated the startup line out of \
             a long-lived pod's log, or the line changed shape. This is NOT an address mismatch"
        )),
        n => Err(format!(
            "startup-line-ambiguous: {n} distinct {LOG_FIELD} values in one container log"
        )),
    }
}

/// A live pool pod, with the process-observed address.
#[derive(Clone, Debug)]
pub struct ObservedPod {
    /// The pod object as kubectl returned it.
    pub pod: Value,
    /// [`advertised_in_log`] of its current container.
    pub logged_address: String,
}

/// PURE R-28 verdict. `pods` are EVERY non-terminating `app=mh-service` pod
/// (kubectl), `pool` is `mh.sh list --json`, `cms` the slot ConfigMaps
/// (kubectl), `policy_labels` the live `networkpolicy/mh-service`
/// `podSelector.matchLabels`. Returns every violation (empty = holds) and the
/// number of pods fully checked (the caller asserts it equals `pods.len()`).
#[must_use]
pub fn check_address_invariant(
    pool: &PoolSnapshot,
    pods: &[ObservedPod],
    cms: &BTreeMap<u32, SlotConfigMap>,
    policy_labels: &BTreeMap<String, String>,
) -> (Vec<String>, usize) {
    let mut v: Vec<String> = Vec::new();
    let mut checked = 0usize;

    // (a) The pool mh.sh reports IS the set of MH pods kubectl sees.
    let kube_names: BTreeSet<&str> = pods.iter().map(|p| pod_name(&p.pod)).collect();
    let list_names: BTreeSet<&str> = pool.slots.iter().filter_map(|s| s.pod.as_deref()).collect();
    if kube_names != list_names {
        v.push(format!(
            "R28 pod-set-mismatch: kubectl -l app=mh-service sees {kube_names:?}, mh.sh list sees \
             {list_names:?}. An MH outside the slot pool (or a different cluster: mh.sh uses \
             kind-$DT_CLUSTER_NAME, kubectl the current context)"
        ));
    }
    let list_slots: BTreeSet<u32> = pool.slots.iter().map(|s| s.slot).collect();
    let cm_slots: BTreeSet<u32> = cms.keys().copied().collect();
    if list_slots != cm_slots {
        v.push(format!(
            "R28 list-mismatch: mh.sh list slots {list_slots:?} != slot ConfigMaps {cm_slots:?}"
        ));
    }
    for s in &pool.slots {
        if s.pod.is_some() && s.state != SlotState::Serving {
            v.push(format!(
                "R28 not-serving: slot {} is {:?} (pod {:?}); the invariant is checked on a \
                 settled pool",
                s.slot, s.state, s.pod
            ));
        }
    }

    // (d) Every SLOT's address and host port are unique (free slots included:
    // the next incarnation there inherits them).
    duplicates(
        cms.values().map(|c| (c.address.clone(), c.slot)),
        "duplicate-address",
        "slot ConfigMap address",
        &mut v,
    );
    duplicates(
        cms.values().map(|c| (c.host_port.to_string(), c.slot)),
        "duplicate-hostport",
        "slot host port",
        &mut v,
    );

    let mut logged: Vec<(String, String)> = Vec::new();
    let mut host_bindings: Vec<(String, String)> = Vec::new();
    for p in pods {
        let before = v.len();
        if let Some(seen) = check_pod(pool, p, cms, policy_labels, &mut v) {
            logged.push((p.logged_address.clone(), seen.name.clone()));
            host_bindings.push((seen.host_binding, seen.name));
        }
        if v.len() == before {
            checked += 1;
        }
    }
    // (d) process-observed addresses and (node, hostPort) bindings are unique.
    duplicates(
        logged.into_iter(),
        "duplicate-address",
        "advertised address",
        &mut v,
    );
    duplicates(
        host_bindings.into_iter(),
        "duplicate-hostport",
        "node hostPort",
        &mut v,
    );
    (v, checked)
}

/// What [`check_pod`] observed of one pod for the cross-pod uniqueness checks.
struct PodSeen {
    name: String,
    /// `<node hostIP>:<webtransport hostPort>`.
    host_binding: String,
}

/// Every per-pod R-28 check on ONE pod, pushing violations onto `v`. `None`
/// when the pod cannot be placed in a slot at all (no label, or a slot with no
/// ConfigMap): nothing slot-relative can then be checked.
fn check_pod(
    pool: &PoolSnapshot,
    p: &ObservedPod,
    cms: &BTreeMap<u32, SlotConfigMap>,
    policy_labels: &BTreeMap<String, String>,
    v: &mut Vec<String>,
) -> Option<PodSeen> {
    let name = pod_name(&p.pod).to_string();
    // (b) slot label -> exactly that slot's ConfigMap.
    let Some(slot) = label(&p.pod, SLOT_LABEL).and_then(|s| s.parse::<u32>().ok()) else {
        v.push(format!(
            "R28 slot-label: pod {name} has no numeric {SLOT_LABEL} label"
        ));
        return None;
    };
    let Some(cm) = cms.get(&slot) else {
        v.push(format!(
            "R28 slot-configmap-missing: pod {name} is labelled slot {slot}, which has no \
             slot ConfigMap"
        ));
        return None;
    };
    if let Some(entry) = pool.slots.iter().find(|s| s.slot == slot) {
        let ip = p.pod.pointer("/status/podIP").and_then(Value::as_str);
        if entry.pod.as_deref() != Some(name.as_str()) || entry.pod_ip.as_deref() != ip {
            v.push(format!(
                "R28 list-mismatch: pod {name} (ip {ip:?}) is labelled slot {slot}, but mh.sh \
                 list reports pod {:?} (ip {:?}) there",
                entry.pod, entry.pod_ip
            ));
        }
    }
    let c = container(&p.pod);
    let env_ref = c
        .and_then(|c| c.get("env"))
        .and_then(Value::as_array)
        .and_then(|e| {
            e.iter()
                .find(|e| e.get("name").and_then(Value::as_str) == Some(ADDRESS_KEY))
        })
        .and_then(|e| e.pointer("/valueFrom/configMapKeyRef"));
    let ref_name = env_ref.and_then(|r| r.get("name")).and_then(Value::as_str);
    let ref_key = env_ref.and_then(|r| r.get("key")).and_then(Value::as_str);
    if ref_name != Some(cm.name.as_str()) || ref_key != Some(ADDRESS_KEY) {
        v.push(format!(
            "R28 ref-mismatch: pod {name} (slot {slot}) reads {ADDRESS_KEY} from ConfigMap \
             {ref_name:?} key {ref_key:?}; slot {slot}'s ConfigMap is {} (key {ADDRESS_KEY})",
            cm.name
        ));
    }
    let wt = c
        .and_then(|c| c.get("ports"))
        .and_then(Value::as_array)
        .and_then(|ps| {
            ps.iter()
                .find(|p| p.get("name").and_then(Value::as_str) == Some(WT_PORT_NAME))
        });
    let host_port = wt.and_then(|p| p.get("hostPort")).and_then(Value::as_u64);
    let proto = wt
        .and_then(|p| p.get("protocol"))
        .and_then(Value::as_str)
        .unwrap_or("TCP");
    if host_port != Some(u64::from(cm.host_port)) || proto != "UDP" {
        v.push(format!(
            "R28 hostport-mismatch: pod {name} (slot {slot}) declares {WT_PORT_NAME} \
             hostPort {host_port:?}/{proto}; slot {slot}'s {HOST_PORT_KEY} is {}/UDP",
            cm.host_port
        ));
    }
    match address_port(&cm.address) {
        Ok(port) if port == cm.host_port => {}
        Ok(port) => v.push(format!(
            "R28 address-port: slot {slot}'s address {} is port {port}, but its \
             {HOST_PORT_KEY} is {} — a client would dial a port this slot does not own",
            cm.address, cm.host_port
        )),
        Err(e) => v.push(format!("R28 address-port: slot {slot}: {e}")),
    }
    // (c) the PROCESS advertises its slot's address.
    if p.logged_address != cm.address {
        v.push(format!(
            "R28 advertised-mismatch: pod {name} (slot {slot}) process advertises {}, but \
             its slot ConfigMap {} carries {}",
            p.logged_address, cm.name, cm.address
        ));
    }
    // S1: the live MH NetworkPolicy selects this pod.
    let missing: Vec<String> = policy_labels
        .iter()
        .filter(|(k, val)| label(&p.pod, k) != Some(val.as_str()))
        .map(|(k, val)| format!("{k}={val}"))
        .collect();
    if !missing.is_empty() {
        v.push(format!(
            "R28 policy-labels: pod {name} lacks networkpolicy/mh-service podSelector \
             labels {missing:?}: the MH policy does not govern it"
        ));
    }
    let host_ip = p
        .pod
        .pointer("/status/hostIP")
        .and_then(Value::as_str)
        .unwrap_or("?");
    Some(PodSeen {
        host_binding: format!("{host_ip}:{}", host_port.unwrap_or(0)),
        name,
    })
}

fn duplicates<K: Ord + std::fmt::Display, W: std::fmt::Debug>(
    items: impl Iterator<Item = (K, W)>,
    token: &str,
    what: &str,
    v: &mut Vec<String>,
) {
    let mut seen: BTreeMap<K, Vec<W>> = BTreeMap::new();
    for (k, w) in items {
        seen.entry(k).or_default().push(w);
    }
    for (k, ws) in seen {
        if ws.len() > 1 {
            v.push(format!("R28 {token}: {what} {k} is shared by {ws:?}"));
        }
    }
}

fn pod_name(pod: &Value) -> &str {
    pod.pointer("/metadata/name")
        .and_then(Value::as_str)
        .unwrap_or("<unnamed pod>")
}

fn label<'a>(obj: &'a Value, key: &str) -> Option<&'a str> {
    obj.pointer("/metadata/labels")
        .and_then(|l| l.get(key))
        .and_then(Value::as_str)
}

fn container(pod: &Value) -> Option<&Value> {
    pod.pointer("/spec/containers")
        .and_then(Value::as_array)?
        .iter()
        .find(|c| c.get("name").and_then(Value::as_str) == Some(MH_CONTAINER))
}

/// PURE: `networkpolicy/mh-service`'s `spec.podSelector.matchLabels`.
///
/// # Errors
/// Absent or EMPTY (an empty selector selects every pod, so a superset check
/// against it would be vacuous), or `matchExpressions` present (not evaluated
/// here, so refused rather than ignored).
pub fn policy_match_labels(policy: &Value) -> Result<BTreeMap<String, String>, String> {
    if policy
        .pointer("/spec/podSelector/matchExpressions")
        .is_some()
    {
        return Err("networkpolicy podSelector uses matchExpressions; not evaluated".to_string());
    }
    let labels: BTreeMap<String, String> = policy
        .pointer("/spec/podSelector/matchLabels")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
        .collect();
    if labels.is_empty() {
        return Err(
            "networkpolicy podSelector.matchLabels is empty: the superset check would be vacuous"
                .to_string(),
        );
    }
    Ok(labels)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::mh_pool::parse_list;
    use serde_json::json;

    const ADDR0: &str = "https://172.18.0.1:24420";
    const ADDR1: &str = "https://172.18.0.1:24421";

    fn cm(slot: u32, name: &str, addr: &str, port: u16) -> Value {
        json!({"metadata": {"name": name, "labels": {SLOT_LABEL: slot.to_string()}},
               "data": {ADDRESS_KEY: addr, HOST_PORT_KEY: port.to_string()}})
    }

    fn cms() -> BTreeMap<u32, SlotConfigMap> {
        slot_configmaps(&json!({"items": [
            cm(0, "mh-slot-0-config-aaaaaaaaaa", ADDR0, 24420),
            cm(1, "mh-slot-1-config-bbbbbbbbbb", ADDR1, 24421),
            cm(2, "mh-slot-2-config-cccccccccc", "https://172.18.0.1:24422", 24422),
        ]}))
        .expect("valid")
    }

    fn pod(name: &str, slot: u32, ip: &str, cm: &str, host_port: u16) -> Value {
        json!({"metadata": {"name": name, "labels": {
                  "app": "mh-service", "component": "media", SLOT_LABEL: slot.to_string()}},
               "status": {"podIP": ip, "hostIP": "172.18.0.2"},
               "spec": {"containers": [{"name": MH_CONTAINER,
                   "env": [{"name": ADDRESS_KEY, "valueFrom": {"configMapKeyRef": {"name": cm, "key": ADDRESS_KEY}}}],
                   "ports": [{"name": "health", "containerPort": 8083},
                             {"name": WT_PORT_NAME, "containerPort": 4434, "protocol": "UDP", "hostPort": host_port}]}]}})
    }

    fn observed(pod: Value, addr: &str) -> ObservedPod {
        ObservedPod {
            pod,
            logged_address: addr.to_string(),
        }
    }

    fn entry(slot: u32, port: u16, addr: &str, pod: Option<(&str, &str)>) -> Value {
        json!({"slot": slot, "host_port": port, "address": addr,
               "pod_ip": pod.map(|p| p.1), "pod_uid": pod.map(|_| "u"),
               "state": if pod.is_some() { "serving" } else { "free" },
               "terminating_past_grace": false, "deployment": pod.map(|_| format!("mh-s{slot}")),
               "pod": pod.map(|p| p.0), "revision": pod.map(|_| "r"), "stale": false,
               "owner": pod.map(|_| "baseline"), "restart_count": pod.map(|_| 0)})
    }

    fn pool() -> PoolSnapshot {
        parse_list(
            &json!({"ok": true, "schema_version": 1, "template_revision": "r", "baseline": 2,
                    "slots": [entry(0, 24420, ADDR0, Some(("mh-s0-a", "10.0.0.1"))),
                              entry(1, 24421, ADDR1, Some(("mh-s1-b", "10.0.0.2"))),
                              entry(2, 24422, "https://172.18.0.1:24422", None)]})
            .to_string(),
        )
        .expect("valid")
    }

    fn policy() -> BTreeMap<String, String> {
        policy_match_labels(&json!({"spec": {"podSelector": {"matchLabels": {
            "app": "mh-service", "component": "media"}}}}))
        .expect("valid")
    }

    fn good_pods() -> Vec<ObservedPod> {
        vec![
            observed(
                pod(
                    "mh-s0-a",
                    0,
                    "10.0.0.1",
                    "mh-slot-0-config-aaaaaaaaaa",
                    24420,
                ),
                ADDR0,
            ),
            observed(
                pod(
                    "mh-s1-b",
                    1,
                    "10.0.0.2",
                    "mh-slot-1-config-bbbbbbbbbb",
                    24421,
                ),
                ADDR1,
            ),
        ]
    }

    fn verdict(pods: &[ObservedPod]) -> (Vec<String>, usize) {
        check_address_invariant(&pool(), pods, &cms(), &policy())
    }

    fn fires(pods: &[ObservedPod], token: &str) -> bool {
        verdict(pods)
            .0
            .iter()
            .any(|m| m.starts_with(&format!("R28 {token}:")))
    }

    #[test]
    fn a_correct_pool_holds_and_every_pod_is_counted() {
        let (v, checked) = verdict(&good_pods());
        assert!(v.is_empty(), "{v:?}");
        assert_eq!(checked, 2);
    }

    #[test]
    fn each_violation_token_fires_on_its_adverse_input() {
        let mut fired: BTreeSet<&str> = BTreeSet::new();
        let mut expect = |pods: Vec<ObservedPod>, token: &'static str| {
            assert!(
                fires(&pods, token),
                "{token} did not fire: {:?}",
                verdict(&pods).0
            );
            fired.insert(token);
        };
        // A rogue MH outside the pool.
        let mut p = good_pods();
        p.push(observed(
            pod("rogue", 2, "10.0.0.9", "mh-slot-2-config-cccccccccc", 24422),
            "https://172.18.0.1:24422",
        ));
        expect(p, "pod-set-mismatch");
        // Crossed ConfigMap ref: slot 1's pod reads slot 0's address.
        let mut p = good_pods();
        p[1].pod = pod(
            "mh-s1-b",
            1,
            "10.0.0.2",
            "mh-slot-0-config-aaaaaaaaaa",
            24421,
        );
        expect(p, "ref-mismatch");
        // hostPort is another slot's.
        let mut p = good_pods();
        p[1].pod = pod(
            "mh-s1-b",
            1,
            "10.0.0.2",
            "mh-slot-1-config-bbbbbbbbbb",
            24420,
        );
        expect(p, "hostport-mismatch");
        // The process advertises something else (and two share one address).
        let mut p = good_pods();
        p[1].logged_address = ADDR0.to_string();
        expect(p.clone(), "advertised-mismatch");
        expect(p, "duplicate-address");
        // Two pods on one node hostPort.
        let mut p = good_pods();
        p[0].pod["spec"]["containers"][0]["ports"][1]["hostPort"] = json!(24421);
        expect(p, "duplicate-hostport");
        // No slot label.
        let mut p = good_pods();
        p[0].pod["metadata"]["labels"]
            .as_object_mut()
            .unwrap()
            .remove(SLOT_LABEL);
        expect(p, "slot-label");
        // Labelled into a slot with no ConfigMap.
        let mut p = good_pods();
        p[0].pod["metadata"]["labels"][SLOT_LABEL] = json!("7");
        expect(p, "slot-configmap-missing");
        // Labelled into the wrong slot vs mh.sh's report.
        let mut p = good_pods();
        p[0].pod["metadata"]["labels"][SLOT_LABEL] = json!("2");
        expect(p, "list-mismatch");
        // Not selected by the MH NetworkPolicy.
        let mut p = good_pods();
        p[0].pod["metadata"]["labels"]
            .as_object_mut()
            .unwrap()
            .remove("component");
        expect(p, "policy-labels");
        // Every token is exercised here or in the dedicated tests below.
        fired.extend(["not-serving", "address-port"]);
        assert_eq!(fired, TOKENS.iter().copied().collect::<BTreeSet<_>>());
    }

    #[test]
    fn a_not_serving_occupied_slot_fires() {
        let mut pl = pool();
        pl.slots[0].state = SlotState::Starting;
        let (v, _) = check_address_invariant(&pl, &good_pods(), &cms(), &policy());
        assert!(v.iter().any(|m| m.starts_with("R28 not-serving:")), "{v:?}");
    }

    #[test]
    fn an_address_on_a_port_the_slot_does_not_own_fires() {
        let mut c = cms();
        c.get_mut(&0).unwrap().address = "https://172.18.0.1:4434".to_string();
        let mut p = good_pods();
        p[0].logged_address = "https://172.18.0.1:4434".to_string();
        let (v, _) = check_address_invariant(&pool(), &p, &c, &policy());
        assert!(
            v.iter().any(|m| m.starts_with("R28 address-port:")),
            "{v:?}"
        );
    }

    #[test]
    fn duplicate_slot_configmap_addresses_fire_even_on_free_slots() {
        let c = slot_configmaps(&json!({"items": [
            cm(0, "mh-slot-0-config-aaaaaaaaaa", ADDR0, 24420),
            cm(1, "mh-slot-1-config-bbbbbbbbbb", ADDR1, 24421),
            cm(2, "mh-slot-2-config-cccccccccc", ADDR1, 24422),
        ]}))
        .unwrap();
        let (v, _) = check_address_invariant(&pool(), &good_pods(), &c, &policy());
        assert!(
            v.iter().any(|m| m.starts_with("R28 duplicate-address:")),
            "{v:?}"
        );
    }

    #[test]
    fn slot_configmap_parsing_refuses_vacuous_and_malformed_input() {
        assert!(slot_configmaps(&json!({"items": []}))
            .unwrap_err()
            .contains("no ConfigMap"));
        let dup = json!({"items": [cm(0, "a", ADDR0, 1), cm(0, "b", ADDR0, 1)]});
        assert!(slot_configmaps(&dup)
            .unwrap_err()
            .contains("two ConfigMaps"));
        let mut bad = cm(0, "a", ADDR0, 1);
        bad["data"][HOST_PORT_KEY] = json!("x");
        assert!(slot_configmaps(&json!({"items": [bad]}))
            .unwrap_err()
            .contains("not a port"));
        let mut nolabel = cm(0, "a", ADDR0, 1);
        nolabel["metadata"]["labels"][SLOT_LABEL] = json!("zero");
        assert!(slot_configmaps(&json!({"items": [nolabel]}))
            .unwrap_err()
            .contains("not a slot number"));
    }

    #[test]
    fn address_authorities_parse_v4_v6_and_refuse_portless() {
        let ok =
            |a: &str, h: &str, p: u16| assert_eq!(address_authority(a), Ok((h.to_string(), p)));
        ok("https://172.18.0.1:24420", "172.18.0.1", 24420);
        ok("https://127.0.0.1:4440", "127.0.0.1", 4440);
        ok("https://[fd00::1]:4440/x", "fd00::1", 4440);
        assert_eq!(address_port("https://172.18.0.1:24420"), Ok(24420));
        for bad in [
            "https://172.18.0.1",
            "http://172.18.0.1:1",
            "172.18.0.1:1",
            "https://[fd00::1]",
            "https://fd00::1",
            "https://[fd00::1:4440",
            "https://:4440",
            "https://172.18.0.1:0",
            "https://172.18.0.1:x",
        ] {
            assert!(address_authority(bad).is_err(), "{bad} was accepted");
        }
    }

    /// The constants this module shares with mh.sh equal mh.sh's own
    /// `readonly` definitions (one home: mh.sh), with a positive control that
    /// every line was found.
    #[test]
    fn shared_names_match_mh_sh_readonly_constants() {
        let text = include_str!("../../../../infra/kind/scripts/mh.sh");
        for (var, ours) in [
            ("MH_WT_PORT_NAME", WT_PORT_NAME),
            ("MH_ADDRESS_KEY", ADDRESS_KEY),
            ("MH_HOST_PORT_KEY", HOST_PORT_KEY),
            ("MH_LABEL_SLOT", SLOT_LABEL),
        ] {
            let prefix = format!("readonly {var}=\"");
            let theirs = text
                .lines()
                .find_map(|l| l.strip_prefix(prefix.as_str()))
                .and_then(|r| r.strip_suffix('"'))
                .unwrap_or_else(|| panic!("mh.sh has no `readonly {var}=\"...\"` line"));
            assert_eq!(
                theirs, ours,
                "{var} drifted between mh.sh and mh_address.rs"
            );
        }
    }

    #[test]
    fn the_startup_line_is_read_from_json_fields_and_absence_is_its_own_token() {
        let log = format!(
            "not json\n{}\n{}\n",
            json!({"level": "INFO", "fields": {"message": "Starting Media Handler"}}),
            json!({"level": "INFO", "fields": {LOG_FIELD: ADDR0, "region": "kind"}})
        );
        assert_eq!(advertised_in_log(&log), Ok(ADDR0.to_string()));
        assert!(advertised_in_log("plain\n")
            .unwrap_err()
            .starts_with("startup-line-missing"));
        let two = format!(
            "{}\n{}\n",
            json!({"fields": {LOG_FIELD: ADDR0}}),
            json!({"fields": {LOG_FIELD: ADDR1}})
        );
        assert!(advertised_in_log(&two)
            .unwrap_err()
            .starts_with("startup-line-ambiguous"));
    }

    #[test]
    fn an_empty_or_expression_policy_selector_is_refused() {
        assert!(policy_match_labels(&json!({"spec": {"podSelector": {}}}))
            .unwrap_err()
            .contains("vacuous"));
        assert!(policy_match_labels(&json!({"spec": {"podSelector": {
            "matchExpressions": [{"key": "app", "operator": "Exists"}]}}}))
        .unwrap_err()
        .contains("matchExpressions"));
    }

    /// The log field name is the one MH's startup line emits (read from the
    /// committed source, so a renamed field fails here, at Layer 4, not as a
    /// perpetual `startup-line-missing` at Layer 7).
    #[test]
    fn the_log_field_is_what_mh_main_logs() {
        let main =
            std::fs::read_to_string(crate::repo_root().join("crates/mh-service/src/main.rs"))
                .expect("read mh-service main.rs");
        assert!(
            main.contains(&format!(
                "{LOG_FIELD} = %config.webtransport_advertise_address"
            )),
            "crates/mh-service/src/main.rs no longer logs `{LOG_FIELD} = \
             %config.webtransport_advertise_address` at startup"
        );
    }
}
