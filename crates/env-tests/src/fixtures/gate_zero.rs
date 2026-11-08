//! Gate zero (ADR-0039 Implementation Guidance "First", story 4 R-27): the
//! PURE parts of the env-test `tests/41_gate_zero_hostport_policy.rs`.
//!
//! The gate proves the REAL MH NetworkPolicy is evaluated on the hostPort /
//! CNI-portmap DNAT path the MH slots will use: a canary selected by the live
//! `networkpolicy/mh-service` gets a datagram on the policy-allowed UDP port
//! and never gets one on a non-allowed port, through the same two hostPorts
//! (a crossover: each hostPort both delivers and drops across two phases).
//!
//! Everything here is hermetic and unit-tested (Layer 4): policy derivation,
//! selector matching, collision/endpoint checks, the canary manifest, the log
//! matcher, and the timing budget. The live calls live in the test file and in
//! `fixtures/kube.rs`.
//!
//! The canary's labels are a NetworkPolicy podSelector, never metric labels:
//! this module emits no metrics.

use std::collections::BTreeMap;
use std::fmt;
use std::time::Duration;

use serde_json::{json, Map, Value};

use crate::canary::{canary_pod_manifest, is_canary_name, DELETE_TIMEOUT_SECONDS};

/// Name of the real MH NetworkPolicy object (`infra/services/mh-service/network-policy.yaml`).
pub const MH_POLICY_NAME: &str = "mh-service";

/// Canary pod-name prefix. NOT `mh-`-prefixed, so it is unmistakable in
/// `mh-logs` (Loki) and never matched by an `mh-*` name glob.
pub const CANARY_PREFIX: &str = "gate-zero-canary";

/// Canary container name. NOT `mh-service`: MH restart alerts/dashboards key
/// on `container="mh-service"`.
pub const CANARY_CONTAINER: &str = "gate-zero-canary";

/// The two node hostPorts: the retired MH-0 health / gRPC Kind containerPorts
/// (story 4 I-2 deleted their extraPortMappings), with nothing listening
/// behind them and outside the MH slot range. The gate sends to them on the
/// NODE IP directly, so no Kind mapping is on its path; the host-publish UDP
/// leg of the real slots is `tests/43_mh_slot_publish_udp.rs`. Preflight
/// asserts no pod declares them as hostPort and no Service uses them as a
/// nodePort.
pub const HOST_PORTS: [u16; 2] = [30830, 30530];

/// The drop-arm container port: in NO MH policy ingress rule (asserted live).
pub const DROP_PORT: u16 = 9999;

/// Kind's control-plane node serves the API on 6443 inside the node; a bounded
/// TCP connect to `<hostIP>:6443` is the runner->node reachability
/// precondition (it does NOT prove UDP; see [`GateZeroError::HostPortDelivery`]).
/// ASSUMES the canary's node is the single Kind control-plane node.
pub const NODE_PROBE_TCP_PORT: u16 = 6443;

// ---------------------------------------------------------------------------
// Timing budget. Every wait is bounded and named; the canary's
// activeDeadlineSeconds is DERIVED from these (see `active_deadline_seconds`).
// ---------------------------------------------------------------------------

/// Bound on the canary reaching Running with its container started (includes
/// scheduling; a hostPort conflict shows here as Pending).
pub const RUNNING_TIMEOUT: Duration = Duration::from_secs(60);
/// Bound on a listener's self-send control line appearing in the canary log.
pub const CONTROL_TIMEOUT: Duration = Duration::from_secs(15);
/// Allowed arm: datagrams sent (each on its own fresh socket), one per
/// `ALLOWED_RECV_WAIT`; the first echo whose log line also appears passes.
/// UDP is lossy, so several attempts — never a retry of the verdict.
pub const ALLOWED_ATTEMPTS: u32 = 5;
pub const ALLOWED_RECV_WAIT: Duration = Duration::from_secs(2);
/// Drop arm: several datagrams spread across the window, each on its own
/// fresh socket (no conntrack entry reused across arms or phases).
pub const DROP_DATAGRAMS: u32 = 5;
/// Drop window = clamp(DROP_RTT_MULTIPLE x observed allowed RTT, floor, cap).
/// The floor dominates on a healthy cluster (RTT is milliseconds); the cap
/// bounds the phase so the activeDeadline derivation holds.
pub const DROP_WINDOW_FLOOR: Duration = Duration::from_secs(5);
pub const DROP_WINDOW_CAP: Duration = Duration::from_secs(30);
pub const DROP_RTT_MULTIPLE: u32 = 50;
/// Bound for the canary log to reflect a delivered datagram (log read lag).
pub const LOG_SETTLE: Duration = Duration::from_secs(10);
/// Log polling interval; a bounded log wait can overshoot by one interval.
pub const LOG_POLL_INTERVAL: Duration = Duration::from_millis(500);
/// Bound on the runner->node TCP reachability probe.
pub const NODE_PROBE_TIMEOUT: Duration = Duration::from_secs(5);
/// Margin on the activeDeadline derivation.
const DEADLINE_MARGIN: Duration = Duration::from_secs(30);

/// Worst-case wall time of ONE phase: schedule/run + node probe + both
/// controls, allowed arm and its log settle, drop window, bracketing allowed
/// arm and its log settle, delete. Every bounded log wait adds one poll
/// interval of overshoot.
#[must_use]
pub const fn worst_case_phase() -> Duration {
    let poll = LOG_POLL_INTERVAL.as_millis() as u64;
    let allowed = ALLOWED_RECV_WAIT.as_millis() as u64 * ALLOWED_ATTEMPTS as u64
        + LOG_SETTLE.as_millis() as u64
        + poll;
    Duration::from_millis(
        RUNNING_TIMEOUT.as_millis() as u64
            + NODE_PROBE_TIMEOUT.as_millis() as u64
            + 2 * (CONTROL_TIMEOUT.as_millis() as u64 + poll)
            + 2 * allowed
            + DROP_WINDOW_CAP.as_millis() as u64
            + DELETE_TIMEOUT_SECONDS * 1000,
    )
}

/// The canary's `activeDeadlineSeconds`: >= 2x the worst-case phase, so it
/// can never fire mid-arm (which would read as drop-arm silence), yet a leaked
/// canary still terminates itself and frees the hostPorts.
#[must_use]
pub const fn active_deadline_seconds() -> u64 {
    2 * worst_case_phase().as_secs() + DEADLINE_MARGIN.as_secs()
}

/// The drop window for an observed allowed-arm RTT.
#[must_use]
pub fn drop_window(observed_rtt: Duration) -> Duration {
    (observed_rtt * DROP_RTT_MULTIPLE).clamp(DROP_WINDOW_FLOOR, DROP_WINDOW_CAP)
}

// ---------------------------------------------------------------------------
// Verdict classes
// ---------------------------------------------------------------------------

/// Which arm a control/verdict concerns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arm {
    Allowed,
    Drop,
}

impl fmt::Display for Arm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Arm::Allowed => "allowed",
            Arm::Drop => "drop",
        })
    }
}

/// One hostPort -> containerPort mapping in a phase.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mapping {
    pub host_port: u16,
    pub container_port: u16,
    pub arm: Arm,
}

/// A gate phase: a canary with one allowed and one drop mapping.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Phase {
    pub name: char,
    pub allowed: Mapping,
    pub drop: Mapping,
}

impl Phase {
    /// `phase=A hostport=.. container_port=..` for every GATE0 line.
    #[must_use]
    pub fn tag(&self, m: &Mapping) -> String {
        format!(
            "phase={} hostport={} container_port={}",
            self.name, m.host_port, m.container_port
        )
    }
}

/// The crossover: phase A maps HOST_PORTS[0]->allowed, HOST_PORTS[1]->drop;
/// phase B swaps them. Each hostPort therefore both delivers and drops, so
/// the only variable between arms is the container port — i.e. the policy.
#[must_use]
pub fn phases(allowed_port: u16) -> [Phase; 2] {
    let m = |host_port, container_port, arm| Mapping {
        host_port,
        container_port,
        arm,
    };
    [
        Phase {
            name: 'A',
            allowed: m(HOST_PORTS[0], allowed_port, Arm::Allowed),
            drop: m(HOST_PORTS[1], DROP_PORT, Arm::Drop),
        },
        Phase {
            name: 'B',
            allowed: m(HOST_PORTS[1], allowed_port, Arm::Allowed),
            drop: m(HOST_PORTS[0], DROP_PORT, Arm::Drop),
        },
    ]
}

/// Gate-zero verdict classes. Each message starts with the greppable
/// `GATE0 <token>`; the token alone tells a portmap/transport fault from a
/// policy fault from an environment fault.
#[derive(Debug, thiserror::Error)]
pub enum GateZeroError {
    /// Environment: policy object missing/changed shape, collision, leftover
    /// canary, kubectl failure, canary not running.
    #[error("GATE0 precondition: {0}")]
    Precondition(String),
    /// Runner cannot reach the node at all (TCP probe).
    #[error(
        "GATE0 node-unreachable: {0} (the probe targets the API server port; it assumes the \
         canary's node is the single Kind control-plane node)"
    )]
    NodeUnreachable(String),
    /// A listener's loopback self-send never appeared in the log.
    #[error("GATE0 control-failed arm={arm} {tag}: {detail}")]
    ControlFailed {
        tag: String,
        arm: Arm,
        detail: String,
    },
    /// Allowed arm silent with controls green.
    #[error(
        "GATE0 hostport-delivery-failed {tag}: {detail}. The listener control passed, so the \
         datagram did not reach the pod via the hostPort/portmap DNAT. This is NOT a policy \
         result. NOTE: the runner->node UDP leg is NOT independently controlled (only TCP \
         reachability is probed) — the Lead must diagnose before adopting the fallback. Check \
         that the Calico CNI chain includes `portmap`: `kubectl get configmap calico-config -n \
         kube-system -o jsonpath='{{.data.cni_network_config}}' | grep portmap` (on the host: \
         `docker|podman exec <node> cat /etc/cni/net.d/10-calico.conflist`)."
    )]
    HostPortDelivery { tag: String, detail: String },
    /// The final allowed datagram after the drop window did not echo.
    #[error(
        "GATE0 bracket-failed {tag}: {detail}. The allowed arm stopped delivering after the \
         drop window, so the drop arm's silence is NOT attributable to policy"
    )]
    BracketFailed { tag: String, detail: String },
    /// A non-allowed-port datagram was echoed or logged.
    #[error("GATE0 policy-not-enforced {tag}: {detail}")]
    PolicyNotEnforced { tag: String, detail: String },
    /// Explicit cleanup failed.
    #[error("GATE0 cleanup-failed: {0}")]
    Cleanup(String),
}

// ---------------------------------------------------------------------------
// Live NetworkPolicy derivation
// ---------------------------------------------------------------------------

/// What the gate takes from the live `networkpolicy/mh-service`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PolicyEvidence {
    /// `.spec.podSelector.matchLabels` — EXACTLY the canary's labels.
    pub labels: BTreeMap<String, String>,
    /// The one UDP port an any-source ingress rule admits.
    pub allowed_udp_port: u16,
    /// Every OTHER port the policy admits (source-restricted rules: MC ->
    /// gRPC, Prometheus -> metrics). The canary must declare none of them:
    /// the metrics port would make Prometheus's `mh-service` job (keep
    /// app=mh-service + that containerPort,
    /// `infra/kubernetes/observability/prometheus.yml`) scrape the canary and
    /// page MHDown.
    pub restricted_ports: Vec<u16>,
}

/// A JSON object of string values as a map (non-strings dropped).
#[must_use]
pub fn string_map(v: &Value) -> BTreeMap<String, String> {
    v.as_object()
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| v.as_str().map(|v| (k.clone(), v.to_string())))
                .collect()
        })
        .unwrap_or_default()
}

/// Does every `k=v` of a label selector map hold in `labels`? The EMPTY-map
/// decision is the caller's: an empty policy podSelector selects every pod, an
/// empty Service selector selects none.
fn selector_selects(sel: &Map<String, Value>, labels: &BTreeMap<String, String>) -> bool {
    sel.iter().all(|(k, v)| {
        v.as_str()
            .is_some_and(|v| labels.get(k).is_some_and(|l| l == v))
    })
}

fn port_number(p: &Value) -> Result<Option<u16>, String> {
    match &p["port"] {
        Value::Null => Ok(None),
        Value::Number(n) => n
            .as_u64()
            .and_then(|n| u16::try_from(n).ok())
            .map(Some)
            .ok_or_else(|| format!("port {n} out of range")),
        other => Err(format!(
            "named/non-numeric port {other} cannot be evaluated against the drop port"
        )),
    }
}

/// Does an ingress `ports` entry admit `port`/`proto`?
fn entry_admits(entry: &Value, port: u16, proto: &str) -> Result<bool, String> {
    let entry_proto = entry["protocol"].as_str().unwrap_or("TCP");
    if entry_proto != proto {
        return Ok(false);
    }
    match port_number(entry)? {
        None => Ok(true), // protocol only: all ports
        Some(p) => {
            let end = entry["endPort"].as_u64().map_or(Ok(p), |e| {
                u16::try_from(e).map_err(|_| format!("endPort {e} out of range"))
            })?;
            Ok((p..=end).contains(&port))
        }
    }
}

/// Does this ingress rule admit `port`/`proto` (ignoring its sources)?
fn rule_admits(rule: &Value, port: u16, proto: &str) -> Result<bool, String> {
    match rule["ports"].as_array() {
        None => Ok(true), // no `ports`: every port
        Some(ps) if ps.is_empty() => Ok(true),
        Some(ps) => {
            for e in ps {
                if entry_admits(e, port, proto)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
    }
}

fn is_any_source(rule: &Value) -> bool {
    match &rule["from"] {
        Value::Null => true,
        Value::Array(a) => a.is_empty(),
        _ => false,
    }
}

/// Derive the gate's evidence from the live policy JSON, refusing any shape
/// that would make the drop attributable to something other than this object.
///
/// # Errors
///
/// No/empty matchLabels; any matchExpressions; Ingress not enforced; not
/// exactly one UDP port admitted from any source; `drop_port` admitted (any
/// protocol, any source) by any rule; unevaluable ports.
pub fn derive_policy(policy: &Value, drop_port: u16) -> Result<PolicyEvidence, String> {
    let sel = &policy["spec"]["podSelector"];
    if sel["matchExpressions"]
        .as_array()
        .is_some_and(|a| !a.is_empty())
    {
        return Err(
            "podSelector has matchExpressions; the canary labels cannot be \
                    derived as exactly the selector"
                .into(),
        );
    }
    let labels = string_map(&sel["matchLabels"]);
    if labels.is_empty() {
        return Err("podSelector.matchLabels is empty/absent (selects every pod)".into());
    }
    let enforces_ingress = policy["spec"]["policyTypes"]
        .as_array()
        .is_some_and(|t| t.iter().any(|t| t == "Ingress"));
    if !enforces_ingress {
        return Err("policyTypes does not include Ingress".into());
    }
    let rules = policy["spec"]["ingress"]
        .as_array()
        .cloned()
        .unwrap_or_default();

    for rule in &rules {
        for proto in ["UDP", "TCP", "SCTP"] {
            if rule_admits(rule, drop_port, proto)? {
                return Err(format!(
                    "an ingress rule admits the drop port {drop_port}/{proto}: {rule}"
                ));
            }
        }
    }

    let mut any_source_udp: Vec<u16> = Vec::new();
    for rule in rules.iter().filter(|r| is_any_source(r)) {
        for e in rule["ports"].as_array().into_iter().flatten() {
            if e["protocol"].as_str() == Some("UDP") {
                match (port_number(e)?, e["endPort"].as_u64()) {
                    (Some(p), None) => any_source_udp.push(p),
                    _ => return Err(format!("any-source UDP entry is not a single port: {e}")),
                }
            }
        }
    }
    any_source_udp.sort_unstable();
    any_source_udp.dedup();

    let mut restricted_ports: Vec<u16> = Vec::new();
    for rule in rules.iter().filter(|r| !is_any_source(r)) {
        for e in rule["ports"].as_array().into_iter().flatten() {
            match (port_number(e)?, e["endPort"].as_u64()) {
                (Some(p), None) => restricted_ports.push(p),
                _ => return Err(format!("restricted-source entry is not a single port: {e}")),
            }
        }
    }
    restricted_ports.sort_unstable();
    restricted_ports.dedup();

    match any_source_udp.as_slice() {
        [p] => Ok(PolicyEvidence {
            labels,
            allowed_udp_port: *p,
            restricted_ports,
        }),
        other => Err(format!(
            "expected exactly one UDP port admitted from any source, found {other:?}"
        )),
    }
}

/// Names of the policies in a `NetworkPolicyList` whose podSelector selects a
/// pod with `labels`.
///
/// # Errors
///
/// A policy uses matchExpressions (not evaluated here; fail rather than guess).
pub fn selecting_policies(
    list: &Value,
    labels: &BTreeMap<String, String>,
) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for p in list["items"].as_array().into_iter().flatten() {
        let name = p["metadata"]["name"].as_str().unwrap_or("?").to_string();
        let sel = &p["spec"]["podSelector"];
        if sel["matchExpressions"]
            .as_array()
            .is_some_and(|a| !a.is_empty())
        {
            return Err(format!(
                "policy {name} uses matchExpressions; cannot decide whether it selects the canary"
            ));
        }
        // Absent/empty podSelector selects EVERY pod in the namespace.
        let matches = sel["matchLabels"]
            .as_object()
            .is_none_or(|m| selector_selects(m, labels));
        if matches {
            out.push(name);
        }
    }
    out.sort();
    Ok(out)
}

/// Services (from a `ServiceList`) whose selector selects `labels`:
/// `(name, publishNotReadyAddresses)`.
#[must_use]
pub fn selecting_services(list: &Value, labels: &BTreeMap<String, String>) -> Vec<(String, bool)> {
    list["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|s| {
            // An empty Service selector selects NOTHING.
            s["spec"]["selector"]
                .as_object()
                .is_some_and(|m| !m.is_empty() && selector_selects(m, labels))
        })
        .map(|s| {
            (
                s["metadata"]["name"].as_str().unwrap_or("?").to_string(),
                s["spec"]["publishNotReadyAddresses"].as_bool() == Some(true),
            )
        })
        .collect()
}

/// EndpointSlices (an `EndpointSliceList`) listing `ip` as ready or serving:
/// slice names.
#[must_use]
pub fn slices_serving_ip(list: &Value, ip: &str) -> Vec<String> {
    list["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|s| {
            s["endpoints"].as_array().into_iter().flatten().any(|e| {
                let has_ip = e["addresses"]
                    .as_array()
                    .is_some_and(|a| a.iter().any(|a| a == ip));
                let c = &e["conditions"];
                has_ip && (c["ready"] == true || c["serving"] == true)
            })
        })
        .map(|s| s["metadata"]["name"].as_str().unwrap_or("?").to_string())
        .collect()
}

/// `namespace/name` of pods (a `PodList`) declaring any of `ports` as hostPort.
#[must_use]
pub fn hostport_claims(list: &Value, ports: &[u16]) -> Vec<String> {
    list["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|p| {
            p["spec"]["containers"]
                .as_array()
                .into_iter()
                .flatten()
                .flat_map(|c| c["ports"].as_array().into_iter().flatten())
                .any(|cp| {
                    cp["hostPort"]
                        .as_u64()
                        .is_some_and(|h| ports.iter().any(|p| u64::from(*p) == h))
                })
        })
        .map(|p| {
            format!(
                "{}/{}",
                p["metadata"]["namespace"].as_str().unwrap_or("?"),
                p["metadata"]["name"].as_str().unwrap_or("?")
            )
        })
        .collect()
}

/// `namespace/name` of Services (a `ServiceList`) using any of `ports` as nodePort.
#[must_use]
pub fn nodeport_claims(list: &Value, ports: &[u16]) -> Vec<String> {
    list["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|s| {
            s["spec"]["ports"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|sp| {
                    sp["nodePort"]
                        .as_u64()
                        .is_some_and(|n| ports.iter().any(|p| u64::from(*p) == n))
                })
        })
        .map(|s| {
            format!(
                "{}/{}",
                s["metadata"]["namespace"].as_str().unwrap_or("?"),
                s["metadata"]["name"].as_str().unwrap_or("?")
            )
        })
        .collect()
}

/// Leftover canaries (from a `PodList`): `(name, phase)`.
#[must_use]
pub fn leftover_canaries(list: &Value) -> Vec<(String, String)> {
    list["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| {
            let name = p["metadata"]["name"].as_str()?;
            is_canary_name(CANARY_PREFIX, name).then(|| {
                (
                    name.to_string(),
                    p["status"]["phase"].as_str().unwrap_or("?").to_string(),
                )
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Canary manifest + log protocol
// ---------------------------------------------------------------------------

/// Fixed prefix of every canary log line.
pub const LOG_MARKER: &str = "gate-zero-canary rx";

/// The ONE listener script. One `udpsvd` per container port (multi-peer;
/// `nc -lu` would bind to the first peer, so the self-send control would
/// capture it). Each datagram is read WHOLE with `dd` (busybox `read` reads a
/// byte at a time, which truncates a datagram to its first byte), logged as
/// `gate-zero-canary rx port=<p> <payload>` on stderr (the container log), and
/// echoed back.
#[must_use]
pub fn listener_script(container_ports: &[u16]) -> String {
    let ports: Vec<String> = container_ports.iter().map(u16::to_string).collect();
    format!(
        "for p in {}; do udpsvd 0 $p sh -c 'd=$(dd bs=2048 count=1 2>/dev/null); \
         echo \"{LOG_MARKER} port='$p' $d\" >&2; printf %s \"$d\"' & done; wait",
        ports.join(" ")
    )
}

/// The exact log line a datagram `payload` to container port `port` produces.
#[must_use]
pub fn log_line(port: u16, payload: &str) -> String {
    format!("{LOG_MARKER} port={port} {payload}")
}

/// Does the canary log contain the exact line for `payload` on `port`?
#[must_use]
pub fn log_has(log: &str, port: u16, payload: &str) -> bool {
    let want = log_line(port, payload);
    log.lines().any(|l| l.trim_end() == want)
}

/// Every canary log line, on ANY port, whose payload starts with `prefix` (the
/// drop arm's per-run nonce prefix). Port-agnostic, so a drop-arm datagram
/// that lands on the OTHER listener (stale DNAT, pod-IP reuse) is caught too.
#[must_use]
pub fn log_lines_with_payload_prefix<'a>(log: &'a str, prefix: &str) -> Vec<&'a str> {
    let marker = format!("{LOG_MARKER} port=");
    log.lines()
        .filter(|l| {
            l.strip_prefix(&marker)
                .and_then(|rest| rest.split_once(' '))
                .is_some_and(|(_, payload)| payload.starts_with(prefix))
        })
        .collect()
}

/// Refuse a non-empty list of Calico-native policies
/// (`globalnetworkpolicies` / `networkpolicies.crd.projectcalico.org`): their
/// selectors are expression strings this gate does not evaluate, and any of
/// them could drop the drop-arm port, so a drop would no longer be
/// attributable to the MH NetworkPolicy alone.
///
/// # Errors
///
/// `list` has any item, or no `items` array.
pub fn refuse_calico_policies(kind: &str, list: &Value) -> Result<(), String> {
    let items = list["items"]
        .as_array()
        .ok_or_else(|| format!("{kind}: no items array"))?;
    if items.is_empty() {
        return Ok(());
    }
    let names: Vec<String> = items
        .iter()
        .map(|i| {
            format!(
                "{}/{}",
                i["metadata"]["namespace"].as_str().unwrap_or("-"),
                i["metadata"]["name"].as_str().unwrap_or("?")
            )
        })
        .collect();
    Err(format!(
        "{kind} exist ({names:?}); a drop could not be attributed to networkpolicy/{MH_POLICY_NAME}"
    ))
}

/// The canary Pod for one phase. Labels = EXACTLY `labels` (the live policy's
/// matchLabels; no `dark-tower.io/mh-slot`, so no MH pool ReplicaSet — whose
/// selector adds that label — adopts it). A readiness probe that cannot pass keeps
/// it out of every Service's ready endpoints. Only the two UDP ports.
#[must_use]
pub fn canary_manifest(
    name: &str,
    namespace: &str,
    labels: &BTreeMap<String, String>,
    phase: &Phase,
) -> Value {
    let maps = [phase.allowed, phase.drop];
    let ports: Vec<Value> = maps
        .iter()
        .map(|m| {
            json!({
                "containerPort": m.container_port,
                "hostPort": m.host_port,
                "protocol": "UDP",
            })
        })
        .collect();
    let container_ports: Vec<u16> = maps.iter().map(|m| m.container_port).collect();
    canary_pod_manifest(
        name,
        namespace,
        &json!(labels),
        json!({
            "name": CANARY_CONTAINER,
            "command": ["sh", "-c", listener_script(&container_ports)],
            "ports": ports,
            "readinessProbe": {
                "exec": { "command": ["false"] },
                "periodSeconds": 10,
            },
        }),
        active_deadline_seconds(),
    )
}

/// Every containerPort a manifest declares.
#[must_use]
pub fn declared_container_ports(manifest: &Value) -> Vec<u16> {
    manifest["spec"]["containers"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|c| c["ports"].as_array().into_iter().flatten())
        .filter_map(|p| {
            p["containerPort"]
                .as_u64()
                .and_then(|p| u16::try_from(p).ok())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(ingress: Value) -> Value {
        json!({
            "spec": {
                "podSelector": {"matchLabels": {"app": "mh-service", "component": "media"}},
                "policyTypes": ["Ingress", "Egress"],
                "ingress": ingress,
            }
        })
    }

    fn mh_ingress() -> Value {
        json!([
            {"from": [{"podSelector": {"matchLabels": {"app": "mc-service"}}}],
             "ports": [{"protocol": "TCP", "port": 50053}]},
            {"from": [], "ports": [{"protocol": "UDP", "port": 4434}]},
            {"from": [{"podSelector": {"matchLabels": {"app": "prometheus"}}}],
             "ports": [{"protocol": "TCP", "port": 8083}]},
        ])
    }

    #[test]
    fn the_mh_policy_shape_derives_labels_and_4434() {
        let ev = derive_policy(&policy(mh_ingress()), DROP_PORT).expect("derive");
        assert_eq!(ev.allowed_udp_port, 4434);
        assert_eq!(ev.labels.len(), 2);
        assert_eq!(ev.labels["app"], "mh-service");
        assert_eq!(ev.restricted_ports, vec![8083, 50053]);
    }

    #[test]
    fn calico_native_policies_are_refused_when_present() {
        assert!(refuse_calico_policies("gnp", &json!({"items": []})).is_ok());
        assert!(refuse_calico_policies(
            "gnp",
            &json!({"items": [{"metadata": {"name": "default-deny"}}]})
        )
        .is_err());
        assert!(refuse_calico_policies("gnp", &json!({})).is_err());
    }

    #[test]
    fn a_policy_admitting_the_drop_port_is_refused() {
        let mut ing = mh_ingress();
        ing.as_array_mut()
            .expect("array")
            .push(json!({"from": [{"ipBlock": {"cidr": "10.0.0.0/8"}}],
                         "ports": [{"protocol": "UDP", "port": 9999}]}));
        assert!(derive_policy(&policy(ing), DROP_PORT).is_err());
    }

    #[test]
    fn a_port_range_or_portless_rule_covering_the_drop_port_is_refused() {
        let mut ing = mh_ingress();
        ing.as_array_mut().expect("array").push(
            json!({"from": [], "ports": [{"protocol": "UDP", "port": 9000, "endPort": 10000}]}),
        );
        assert!(derive_policy(&policy(ing), DROP_PORT).is_err());
        let mut ing = mh_ingress();
        ing.as_array_mut()
            .expect("array")
            .push(json!({"from": [{"podSelector": {}}]}));
        assert!(derive_policy(&policy(ing), DROP_PORT).is_err());
    }

    #[test]
    fn match_expressions_or_empty_selector_is_refused() {
        let mut p = policy(mh_ingress());
        p["spec"]["podSelector"]["matchExpressions"] = json!([{"key": "x", "operator": "Exists"}]);
        assert!(derive_policy(&p, DROP_PORT).is_err());
        let mut p = policy(mh_ingress());
        p["spec"]["podSelector"] = json!({});
        assert!(derive_policy(&p, DROP_PORT).is_err());
    }

    #[test]
    fn zero_or_two_any_source_udp_ports_is_refused() {
        let ing = json!([{"from": [], "ports": [{"protocol": "TCP", "port": 1}]}]);
        assert!(derive_policy(&policy(ing), DROP_PORT).is_err());
        let ing = json!([{"from": [], "ports": [{"protocol": "UDP", "port": 1},
                                                {"protocol": "UDP", "port": 2}]}]);
        assert!(derive_policy(&policy(ing), DROP_PORT).is_err());
    }

    #[test]
    fn selecting_policies_matches_subsets_and_empty_selectors() {
        let labels: BTreeMap<String, String> = [("app", "mh-service"), ("component", "media")]
            .into_iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect();
        let list = json!({"items": [
            {"metadata": {"name": "mh-service"},
             "spec": {"podSelector": {"matchLabels": {"app": "mh-service", "component": "media"}}}},
            {"metadata": {"name": "mc-service"},
             "spec": {"podSelector": {"matchLabels": {"app": "mc-service"}}}},
        ]});
        assert_eq!(
            selecting_policies(&list, &labels).expect("ok"),
            vec!["mh-service"]
        );
        let list = json!({"items": [
            {"metadata": {"name": "default-deny"}, "spec": {"podSelector": {}}},
        ]});
        assert_eq!(
            selecting_policies(&list, &labels).expect("ok"),
            vec!["default-deny"]
        );
    }

    #[test]
    fn services_needing_more_labels_do_not_select_the_canary() {
        let labels: BTreeMap<String, String> = [("app", "mh-service"), ("component", "media")]
            .into_iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect();
        let list = json!({"items": [
            {"metadata": {"name": "mh-service"},
             "spec": {"selector": {"app": "mh-service", "component": "media"}}},
            {"metadata": {"name": "mh-slot-0"},
             "spec": {"selector": {"app": "mh-service", "component": "media", "dark-tower.io/mh-slot": "0"},
                      "publishNotReadyAddresses": true}},
        ]});
        assert_eq!(
            selecting_services(&list, &labels),
            vec![("mh-service".to_string(), false)]
        );
    }

    #[test]
    fn not_ready_endpoints_are_not_serving() {
        let list = json!({"items": [{"metadata": {"name": "s1"}, "endpoints": [
            {"addresses": ["1.2.3.4"], "conditions": {"ready": false, "serving": false}}]}]});
        assert!(slices_serving_ip(&list, "1.2.3.4").is_empty());
        let list = json!({"items": [{"metadata": {"name": "s1"}, "endpoints": [
            {"addresses": ["1.2.3.4"], "conditions": {"ready": false, "serving": true}}]}]});
        assert_eq!(slices_serving_ip(&list, "1.2.3.4"), vec!["s1"]);
    }

    #[test]
    fn collisions_are_found() {
        let pods = json!({"items": [{"metadata": {"namespace": "n", "name": "p"},
            "spec": {"containers": [{"ports": [{"containerPort": 1, "hostPort": 30530}]}]}}]});
        assert_eq!(hostport_claims(&pods, &HOST_PORTS), vec!["n/p"]);
        let svcs = json!({"items": [{"metadata": {"namespace": "n", "name": "s"},
            "spec": {"ports": [{"nodePort": 30830}]}}]});
        assert_eq!(nodeport_claims(&svcs, &HOST_PORTS), vec!["n/s"]);
        let none = json!({"items": [{"metadata": {"namespace": "n", "name": "s"},
            "spec": {"ports": [{"nodePort": 31999}]}}]});
        assert!(nodeport_claims(&none, &HOST_PORTS).is_empty());
    }

    #[test]
    fn leftovers_are_matched_by_prefix_only() {
        let pods = json!({"items": [
            {"metadata": {"name": "gate-zero-canary-abcd1234"}, "status": {"phase": "Failed"}},
            {"metadata": {"name": "mh-s0-7d9f8c6b5-xyz12"}, "status": {"phase": "Running"}},
        ]});
        assert_eq!(
            leftover_canaries(&pods),
            vec![(
                "gate-zero-canary-abcd1234".to_string(),
                "Failed".to_string()
            )]
        );
    }

    #[test]
    fn log_matching_is_exact_on_port_and_nonce() {
        let log = "udpsvd: noise\ngate-zero-canary rx port=4434 ext-abc-1\n";
        assert!(log_has(log, 4434, "ext-abc-1"));
        assert!(!log_has(log, 9999, "ext-abc-1"));
        assert!(!log_has(log, 4434, "ext-abc-"));
        assert!(!log_has(log, 4434, "ext-abd-1"));
        assert_eq!(log_lines_with_payload_prefix(log, "ext-abc-").len(), 1);
        assert!(log_lines_with_payload_prefix(log, "ext-xyz-").is_empty());
        // Port-agnostic: a drop-arm payload on the OTHER listener is found.
        let log = "gate-zero-canary rx port=4434 ext-n-A-d-0\n";
        assert_eq!(log_lines_with_payload_prefix(log, "ext-n-A-d-").len(), 1);
    }

    #[test]
    fn the_crossover_uses_each_host_port_for_both_arms() {
        let [a, b] = phases(4434);
        assert_eq!(a.allowed.host_port, b.drop.host_port);
        assert_eq!(a.drop.host_port, b.allowed.host_port);
        assert_ne!(a.allowed.host_port, a.drop.host_port);
        for p in [&a, &b] {
            assert_eq!(p.allowed.container_port, 4434);
            assert_eq!(p.drop.container_port, DROP_PORT);
        }
    }

    #[test]
    fn the_manifest_declares_only_the_two_udp_ports_and_never_8083_or_50053() {
        let labels: BTreeMap<String, String> =
            [("app".to_string(), "mh-service".to_string())].into();
        let [a, _] = phases(4434);
        let m = canary_manifest("gate-zero-canary-x", "ns", &labels, &a);
        let ports = declared_container_ports(&m);
        assert_eq!(ports, vec![4434, DROP_PORT]);
        let ev = derive_policy(&policy(mh_ingress()), DROP_PORT).expect("derive");
        assert!(ev.restricted_ports.iter().all(|f| !ports.contains(f)));
        assert_eq!(
            m["spec"]["activeDeadlineSeconds"],
            active_deadline_seconds()
        );
        assert_eq!(m["metadata"]["labels"], json!({"app": "mh-service"}));
        assert_eq!(m["spec"]["containers"][0]["name"], CANARY_CONTAINER);
        assert!(m["metadata"].get("annotations").is_none());
    }

    #[test]
    fn the_active_deadline_exceeds_twice_the_worst_case_phase() {
        // The per-step bounds, listed independently of `worst_case_phase()`:
        // a wait added to (or grown in) a phase without updating the budget
        // fails here.
        let steps = [
            RUNNING_TIMEOUT,                             // create -> Running
            NODE_PROBE_TIMEOUT,                          // runner -> node probe
            CONTROL_TIMEOUT + LOG_POLL_INTERVAL,         // allowed-arm control
            CONTROL_TIMEOUT + LOG_POLL_INTERVAL,         // drop-arm control
            ALLOWED_RECV_WAIT * ALLOWED_ATTEMPTS,        // allowed arm sends
            LOG_SETTLE + LOG_POLL_INTERVAL,              // allowed arm log
            DROP_WINDOW_CAP,                             // drop window
            ALLOWED_RECV_WAIT * ALLOWED_ATTEMPTS,        // bracket sends
            LOG_SETTLE + LOG_POLL_INTERVAL,              // bracket log
            Duration::from_secs(DELETE_TIMEOUT_SECONDS), // asserted delete
        ];
        let sum: Duration = steps.iter().sum();
        assert!(
            worst_case_phase() >= sum,
            "{:?} < {sum:?}",
            worst_case_phase()
        );
        assert!(Duration::from_secs(active_deadline_seconds()) >= 2 * sum);
        assert!(DROP_WINDOW_CAP >= DROP_WINDOW_FLOOR);
        assert_eq!(drop_window(Duration::from_millis(1)), DROP_WINDOW_FLOOR);
        assert_eq!(drop_window(Duration::from_secs(10)), DROP_WINDOW_CAP);
    }

    #[test]
    fn verdict_tokens_are_distinct_and_greppable() {
        let t = "phase=A hostport=30830 container_port=4434".to_string();
        let msgs = [
            GateZeroError::Precondition("x".into()).to_string(),
            GateZeroError::NodeUnreachable("x".into()).to_string(),
            GateZeroError::ControlFailed {
                tag: t.clone(),
                arm: Arm::Drop,
                detail: "x".into(),
            }
            .to_string(),
            GateZeroError::HostPortDelivery {
                tag: t.clone(),
                detail: "x".into(),
            }
            .to_string(),
            GateZeroError::BracketFailed {
                tag: t.clone(),
                detail: "x".into(),
            }
            .to_string(),
            GateZeroError::PolicyNotEnforced {
                tag: t.clone(),
                detail: "x".into(),
            }
            .to_string(),
            GateZeroError::Cleanup("x".into()).to_string(),
        ];
        let tokens: Vec<&str> = msgs
            .iter()
            .map(|m| m.split_whitespace().nth(1).expect("token"))
            .collect();
        let mut dedup = tokens.clone();
        dedup.sort_unstable();
        dedup.dedup();
        assert_eq!(dedup.len(), tokens.len(), "{tokens:?}");
        assert!(msgs.iter().all(|m| m.starts_with("GATE0 ")));
        assert!(msgs[3].contains("NOT independently controlled"));
    }
}
