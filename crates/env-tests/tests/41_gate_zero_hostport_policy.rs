//! Gate zero (story 4 R-27; ADR-0039 Implementation Guidance "First", §7):
//! the real MH NetworkPolicy is enforced on the UDP hostPort / CNI-portmap
//! DNAT path the MH slots will use.
//!
//! OWN TEST BINARY, ONE test fn: nothing else runs alongside it, and it holds
//! node hostPorts 30830/30530 while it runs.
//!
//! # Evidence shape (security ruling)
//!
//! The MH policy admits UDP 4434 from ANY source (browsers), so a "disallowed
//! source" cannot be the test. The gate proves the policy is EVALUATED on the
//! DNAT path: a never-ready canary selected by the LIVE `networkpolicy/
//! mh-service` (labels derived from its podSelector, never retyped) gets an
//! allowed-port datagram and never gets a non-allowed-port one, through the
//! same hostPorts. A crossover (phase A / phase B swap the hostPorts) makes
//! each hostPort both deliver and drop, so the only variable is the policy.
//! Attribution: `mh-service` must be the ONLY k8s NetworkPolicy selecting the
//! canary, and no Calico-native (Global)NetworkPolicy may exist.
//!
//! Discriminating power was checked ONCE, by hand, at planning time (NOT
//! re-run by this test): a pod WITHOUT the MH labels, same hostPorts and
//! listener, received UDP 9999 — so the drop seen here is the policy's.
//!
//! # Scope
//!
//! Datagrams go to the canary's NODE IP (`.status.hostIP`) on the hostPort:
//! node PREROUTING -> CNI-HOSTPORT-DNAT (portmap) -> Calico policy, exactly as
//! a host-published datagram arrives. The Kind host-publish leg (host gateway
//! -> node) is NOT exercised here: 30830/30530 have no Kind mapping (I-2
//! deleted the old TCP ones). The UDP slot mappings' end-to-end leg is
//! `43_mh_slot_publish_udp.rs`. The PASS line says so.
//!
//! # Verdicts
//!
//! `GATE0 <token>` (see `env_tests::fixtures::gate_zero::GateZeroError`):
//! precondition / node-unreachable / control-failed / hostport-delivery-failed
//! / bracket-failed / policy-not-enforced / cleanup-failed. Silence on an arm
//! before its control passes is a FAIL, never a pass.
//!
//! Edge-sum (concurrent Σ P×(N+M)): 0

#![cfg(feature = "resilience")]

use std::net::{IpAddr, SocketAddr};
use std::time::{Duration, Instant};

use env_tests::canary::{canary_name, graceful_delete_command, short_nonce, CanaryPod};
use env_tests::fixtures::gate_zero::{
    canary_manifest, declared_container_ports, derive_policy, drop_window, hostport_claims,
    leftover_canaries, log_has, log_lines_with_payload_prefix, nodeport_claims, phases,
    refuse_calico_policies, selecting_policies, selecting_services, slices_serving_ip, string_map,
    Arm, GateZeroError, Mapping, Phase, PolicyEvidence, ALLOWED_ATTEMPTS, ALLOWED_RECV_WAIT,
    CANARY_PREFIX, CONTROL_TIMEOUT, DROP_DATAGRAMS, DROP_PORT, HOST_PORTS, LOG_POLL_INTERVAL,
    LOG_SETTLE, MH_POLICY_NAME, NODE_PROBE_TCP_PORT, NODE_PROBE_TIMEOUT, RUNNING_TIMEOUT,
};
use env_tests::fixtures::kube::kubectl_get_json;
use env_tests::NAMESPACE;
use tokio::net::UdpSocket;

type GateResult<T> = Result<T, GateZeroError>;

const LOG_TAIL_LINES: usize = 30;

fn precondition(e: impl std::fmt::Display) -> GateZeroError {
    GateZeroError::Precondition(e.to_string())
}

/// A precondition raised inside a phase: carries `phase=` (and the mapping
/// tag where one applies) like every other GATE0 line.
fn precondition_in(tag: &str, e: impl std::fmt::Display) -> GateZeroError {
    GateZeroError::Precondition(format!("{tag} {e}"))
}

/// `kubectl logs` for failure evidence; a failed read says so instead of
/// reading as an empty (silent) log.
async fn evidence_log(canary: &CanaryPod) -> String {
    canary
        .logs()
        .await
        .unwrap_or_else(|e| format!("(log read failed: {e})"))
}

fn tail(log: &str) -> String {
    let lines: Vec<&str> = log.lines().collect();
    let start = lines.len().saturating_sub(LOG_TAIL_LINES);
    format!("canary log tail:\n{}", lines[start..].join("\n"))
}

#[tokio::test]
async fn gate_zero_mh_policy_enforced_on_udp_hostport_dnat_path() {
    match run_gate().await {
        Ok(summary) => println!("{summary}"),
        Err(e) => panic!("{e}"),
    }
}

async fn run_gate() -> GateResult<String> {
    // --- The SAME policy object, read live --------------------------------
    let policy = kubectl_get_json(&["networkpolicy", MH_POLICY_NAME, "-n", NAMESPACE])
        .map_err(|e| precondition(format!("networkpolicy/{MH_POLICY_NAME} unreadable: {e}")))?;
    let ev = derive_policy(&policy, DROP_PORT)
        .map_err(|e| precondition(format!("networkpolicy/{MH_POLICY_NAME} shape: {e}")))?;

    let all_policies =
        kubectl_get_json(&["networkpolicy", "-n", NAMESPACE]).map_err(precondition)?;
    let selecting = selecting_policies(&all_policies, &ev.labels).map_err(precondition)?;
    if selecting != [MH_POLICY_NAME] {
        return Err(precondition(format!(
            "the canary labels {:?} are selected by policies {selecting:?}, not exactly \
             [{MH_POLICY_NAME}]; a drop could not be attributed to the MH policy",
            ev.labels
        )));
    }

    // --- No Calico-native policy could also be dropping traffic -----------
    // Their selectors are expression strings (not evaluated): any item fails.
    // A kubectl error (e.g. the CRD is absent) fails too: Calico is the CNI
    // this gate asserts.
    for (kind, args) in [
        (
            "globalnetworkpolicies.crd.projectcalico.org",
            vec!["globalnetworkpolicies.crd.projectcalico.org"],
        ),
        (
            "networkpolicies.crd.projectcalico.org",
            vec!["networkpolicies.crd.projectcalico.org", "-A"],
        ),
    ] {
        let list = kubectl_get_json(&args).map_err(|e| {
            precondition(format!(
                "Calico policy kind {kind} unreadable (is Calico installed?): {e}"
            ))
        })?;
        refuse_calico_policies(kind, &list).map_err(precondition)?;
    }

    // --- Never an endpoint: no selecting Service publishes not-ready ------
    let ns_services = kubectl_get_json(&["services", "-n", NAMESPACE]).map_err(precondition)?;
    let svc_selecting = selecting_services(&ns_services, &ev.labels);
    if let Some((name, _)) = svc_selecting.iter().find(|(_, pnr)| *pnr) {
        return Err(precondition(format!(
            "Service {name} selects the canary labels and sets publishNotReadyAddresses; \
             a never-ready canary would receive its traffic"
        )));
    }

    // --- Collision + leftover preflight ------------------------------------
    preflight()?;

    // --- The crossover ------------------------------------------------------
    let mut outcomes = Vec::new();
    for phase in phases(ev.allowed_udp_port) {
        // Phase B starts only after phase A's canary is asserted NotFound
        // (run_phase returns only after an asserted cleanup).
        outcomes.push(run_phase(&phase, &ev).await?);
    }

    Ok(format!(
        "GATE0 PASS policy-on-DNAT: PASS (phases A+B; networkpolicy/{MH_POLICY_NAME} labels={:?}, \
         allowed UDP {} delivered and UDP {DROP_PORT} dropped via EACH hostPort {:?}); \
         publish-layer UDP leg: NOT EXERCISED here (43_mh_slot_publish_udp.rs). {}",
        ev.labels,
        ev.allowed_udp_port,
        HOST_PORTS,
        outcomes.join("; ")
    ))
}

fn preflight() -> GateResult<()> {
    let all_pods = kubectl_get_json(&["pods", "-A"]).map_err(precondition)?;
    let claims = hostport_claims(&all_pods, &HOST_PORTS);
    let ns_pods = kubectl_get_json(&["pods", "-n", NAMESPACE]).map_err(precondition)?;
    let leftovers = leftover_canaries(&ns_pods);
    if !leftovers.is_empty() {
        let cmds: Vec<String> = leftovers
            .iter()
            .map(|(n, phase)| {
                let why = if phase == "Failed" {
                    " (Failed: an aborted prior run whose activeDeadlineSeconds fired)"
                } else {
                    " (a prior run did not clean up)"
                };
                format!(
                    "{n} phase={phase}{why}: {}",
                    graceful_delete_command(n, NAMESPACE)
                )
            })
            .collect();
        return Err(precondition(format!(
            "leftover gate-zero canaries exist; delete them (never --force):\n{}",
            cmds.join("\n")
        )));
    }
    if !claims.is_empty() {
        return Err(precondition(format!(
            "pods already declare hostPort {HOST_PORTS:?}: {claims:?}"
        )));
    }
    let all_services = kubectl_get_json(&["services", "-A"]).map_err(precondition)?;
    let np = nodeport_claims(&all_services, &HOST_PORTS);
    if !np.is_empty() {
        return Err(precondition(format!(
            "Services use {HOST_PORTS:?} as nodePort ({np:?}); kube-proxy NodePort DNAT and \
             the hostPort DNAT would both match, so the evidence would be ambiguous"
        )));
    }
    Ok(())
}

async fn run_phase(phase: &Phase, ev: &PolicyEvidence) -> GateResult<String> {
    let ptag = format!("phase={}", phase.name);
    let name = canary_name(CANARY_PREFIX);
    let manifest = canary_manifest(&name, NAMESPACE, &ev.labels, phase);
    let declared = declared_container_ports(&manifest);
    // No port the policy admits from a restricted source (derived live: MC ->
    // gRPC, Prometheus -> metrics). Declaring the metrics port would make the
    // Prometheus `mh-service` job (keep app=mh-service + that containerPort)
    // scrape the canary and page MHDown.
    if let Some(p) = ev.restricted_ports.iter().find(|p| declared.contains(p)) {
        return Err(precondition_in(
            &ptag,
            format!("canary manifest declares forbidden port {p}"),
        ));
    }

    eprintln!(
        "[gate0] phase={} creating {name} ({}, {})",
        phase.name,
        phase.tag(&phase.allowed),
        phase.tag(&phase.drop)
    );
    let canary =
        match CanaryPod::deploy_manifest(NAMESPACE, &name, &manifest, RUNNING_TIMEOUT).await {
            Ok(c) => c,
            Err(e) => {
                let events = kubectl_get_json(&[
                    "events",
                    "-n",
                    NAMESPACE,
                    "--field-selector",
                    &format!("involvedObject.name={name}"),
                ])
                .map(|v| {
                    v["items"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|e| format!("{} {}: {}", e["type"], e["reason"], e["message"]))
                        .collect::<Vec<_>>()
                        .join("\n")
                })
                .unwrap_or_else(|e| format!("(events unreadable: {e})"));
                return Err(precondition_in(
                    &ptag,
                    format!(
                        "canary {name} did not start (FailedScheduling here usually means a \
                 hostPort conflict): {e}\nevents:\n{events}"
                    ),
                ));
            }
        };

    let body = phase_body(&canary, phase, ev).await;
    let cleanup = canary.cleanup().await;
    match (body, cleanup) {
        (Ok(o), Ok(())) => Ok(o),
        (Ok(_), Err(e)) => Err(GateZeroError::Cleanup(format!("phase={} {e}", phase.name))),
        (Err(e), Ok(())) => Err(e),
        (Err(e), Err(c)) => {
            eprintln!("GATE0 cleanup-failed: phase={} {c}", phase.name);
            Err(e)
        }
    }
}

async fn phase_body(canary: &CanaryPod, phase: &Phase, ev: &PolicyEvidence) -> GateResult<String> {
    let ptag = format!("phase={}", phase.name);
    let pod = canary
        .pod_json()
        .await
        .map_err(|e| precondition_in(&ptag, e))?;

    // Exactly the policy's labels; no owner (no ReplicaSet adopted it).
    let live_labels = string_map(&pod["metadata"]["labels"]);
    if live_labels != ev.labels {
        return Err(precondition_in(
            &ptag,
            format!(
                "canary labels {live_labels:?} != policy podSelector {:?}",
                ev.labels
            ),
        ));
    }
    if pod["metadata"]["ownerReferences"]
        .as_array()
        .is_some_and(|o| !o.is_empty())
    {
        return Err(precondition_in(
            &ptag,
            format!(
                "canary {} was adopted: ownerReferences={}",
                canary.name(),
                pod["metadata"]["ownerReferences"]
            ),
        ));
    }
    let host_ip: IpAddr = pod["status"]["hostIP"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| {
            precondition_in(&ptag, format!("canary has no hostIP: {}", pod["status"]))
        })?;
    let pod_ip = pod["status"]["podIP"]
        .as_str()
        .ok_or_else(|| precondition_in(&ptag, "canary has no podIP"))?
        .to_string();

    // Runner -> node reachability (TCP only; UDP is not independently probed).
    let probe = SocketAddr::new(host_ip, NODE_PROBE_TCP_PORT);
    tokio::time::timeout(NODE_PROBE_TIMEOUT, tokio::net::TcpStream::connect(probe))
        .await
        .map_err(|_| format!("no answer within {NODE_PROBE_TIMEOUT:?}"))
        .and_then(|r| r.map_err(|e| e.to_string()))
        .map_err(|d| {
            GateZeroError::NodeUnreachable(format!("phase={} TCP {probe}: {d}", phase.name))
        })?;

    let nonce = short_nonce();

    // --- Positive controls: both listeners, before any external datagram ---
    for m in [phase.allowed, phase.drop] {
        let payload = format!("ctl-{nonce}-{}-{}", phase.name, m.container_port);
        let script = format!(
            "printf %s {payload} | nc -u -w1 127.0.0.1 {}",
            m.container_port
        );
        if let Err(e) = canary.exec_sh(&script).await {
            return Err(GateZeroError::ControlFailed {
                tag: phase.tag(&m),
                arm: m.arm,
                detail: format!("self-send exec failed: {e}"),
            });
        }
        if let Err(log) = wait_for_log(canary, m.container_port, &payload, CONTROL_TIMEOUT).await {
            return Err(GateZeroError::ControlFailed {
                tag: phase.tag(&m),
                arm: m.arm,
                detail: format!(
                    "self-send {payload} not logged within {CONTROL_TIMEOUT:?} (pod {})\n{}",
                    canary.name(),
                    tail(&log)
                ),
            });
        }
    }

    // --- Allowed arm --------------------------------------------------------
    let rtt = match allowed_arm(
        canary,
        phase,
        host_ip,
        &format!("ext-{nonce}-{}-a", phase.name),
    )
    .await
    {
        Ok(rtt) => rtt,
        Err(detail) => {
            return Err(GateZeroError::HostPortDelivery {
                tag: phase.tag(&phase.allowed),
                detail: format!("node {host_ip}, pod {}: {detail}", canary.name()),
            })
        }
    };

    // --- Drop arm -----------------------------------------------------------
    let window = drop_window(rtt);
    let per = window / DROP_DATAGRAMS;
    let drop_prefix = format!("ext-{nonce}-{}-d-", phase.name);
    let target = SocketAddr::new(host_ip, phase.drop.host_port);
    for i in 0..DROP_DATAGRAMS {
        let payload = format!("{drop_prefix}{i}");
        // Fresh socket per datagram: no conntrack entry is reused.
        let sock = fresh_socket()
            .await
            .map_err(|e| precondition_in(&phase.tag(&phase.drop), e))?;
        let src = sock.local_addr().map(|a| a.port()).unwrap_or(0);
        sock.send_to(payload.as_bytes(), target)
            .await
            .map_err(|e| precondition_in(&phase.tag(&phase.drop), e))?;
        eprintln!(
            "[gate0] {} src_port={src} sent {payload} -> {target} (drop arm, wait {per:?})",
            phase.tag(&phase.drop)
        );
        let mut buf = [0u8; 2048];
        if let Ok(recv) = tokio::time::timeout(per, sock.recv_from(&mut buf)).await {
            // A recv error ends this datagram's share of the window early:
            // fail loudly rather than count it as a drop.
            let (n, from) = recv.map_err(|e| {
                precondition_in(
                    &phase.tag(&phase.drop),
                    format!("recv error during drop window for {payload}: {e}"),
                )
            })?;
            let log = evidence_log(canary).await;
            return Err(GateZeroError::PolicyNotEnforced {
                tag: phase.tag(&phase.drop),
                detail: format!(
                    "echo {:?} from {from} for {payload} (pod {})\n{}",
                    String::from_utf8_lossy(&buf[..n]),
                    canary.name(),
                    tail(&log)
                ),
            });
        }
    }

    // --- Bracket: the allowed path still delivers AFTER the drop window ----
    if let Err(detail) = allowed_arm(
        canary,
        phase,
        host_ip,
        &format!("ext-{nonce}-{}-b", phase.name),
    )
    .await
    {
        return Err(GateZeroError::BracketFailed {
            tag: phase.tag(&phase.allowed),
            detail: format!("node {host_ip}, pod {}: {detail}", canary.name()),
        });
    }

    // The bracket's log line is present, so this read covers the whole drop
    // window. The drop-arm control line must still be there (log read works).
    let log = canary.logs().await.map_err(|e| precondition_in(&ptag, e))?;
    // Any port: a drop-arm datagram on the OTHER listener counts too.
    let leaked = log_lines_with_payload_prefix(&log, &drop_prefix);
    if !leaked.is_empty() {
        return Err(GateZeroError::PolicyNotEnforced {
            tag: phase.tag(&phase.drop),
            detail: format!(
                "canary logged drop-arm datagrams {leaked:?}\n{}",
                tail(&log)
            ),
        });
    }
    let drop_ctl = format!("ctl-{nonce}-{}-{}", phase.name, phase.drop.container_port);
    if !log_has(&log, phase.drop.container_port, &drop_ctl) {
        return Err(GateZeroError::ControlFailed {
            tag: phase.tag(&phase.drop),
            arm: Arm::Drop,
            detail: format!(
                "the drop-arm control line vanished from the post-window log read\n{}",
                tail(&log)
            ),
        });
    }

    // --- Never ready, never a serving endpoint ------------------------------
    let pod = canary
        .pod_json()
        .await
        .map_err(|e| precondition_in(&ptag, e))?;
    let ready = pod["status"]["conditions"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|c| c["type"] == "Ready" && c["status"] == "True");
    if ready {
        return Err(precondition_in(
            &ptag,
            format!("canary {} became Ready", canary.name()),
        ));
    }
    let slices = kubectl_get_json(&["endpointslices", "-n", NAMESPACE])
        .map_err(|e| precondition_in(&ptag, e))?;
    let serving = slices_serving_ip(&slices, &pod_ip);
    if !serving.is_empty() {
        return Err(precondition_in(
            &ptag,
            format!("canary IP {pod_ip} is a ready/serving endpoint in {serving:?}"),
        ));
    }

    let summary = format!(
        "phase={} node={host_ip} pod={}: hostport {} -> {} delivered (rtt {rtt:?}, bracketed), \
         hostport {} -> {} dropped ({DROP_DATAGRAMS} datagrams over {window:?})",
        phase.name,
        canary.name(),
        phase.allowed.host_port,
        phase.allowed.container_port,
        phase.drop.host_port,
        phase.drop.container_port,
    );
    eprintln!("[gate0] {summary}");
    Ok(summary)
}

async fn fresh_socket() -> std::io::Result<UdpSocket> {
    UdpSocket::bind("0.0.0.0:0").await
}

/// Up to ALLOWED_ATTEMPTS datagrams, each on a fresh socket; passes on the
/// first echo whose log line also appears. Returns the RTT.
async fn allowed_arm(
    canary: &CanaryPod,
    phase: &Phase,
    host_ip: IpAddr,
    prefix: &str,
) -> Result<Duration, String> {
    let m: Mapping = phase.allowed;
    let target = SocketAddr::new(host_ip, m.host_port);
    for i in 0..ALLOWED_ATTEMPTS {
        let payload = format!("{prefix}-{i}");
        let sock = fresh_socket().await.map_err(|e| e.to_string())?;
        let src = sock.local_addr().map(|a| a.port()).unwrap_or(0);
        let sent = Instant::now();
        sock.send_to(payload.as_bytes(), target)
            .await
            .map_err(|e| format!("send to {target}: {e}"))?;
        eprintln!(
            "[gate0] {} src_port={src} sent {payload} -> {target}",
            phase.tag(&m)
        );
        let mut buf = [0u8; 2048];
        let Ok(recv) = tokio::time::timeout(ALLOWED_RECV_WAIT, sock.recv_from(&mut buf)).await
        else {
            eprintln!(
                "[gate0] {} no echo for {payload} within {ALLOWED_RECV_WAIT:?}",
                phase.tag(&m)
            );
            continue;
        };
        let Ok((n, from)) = recv.inspect_err(|e| {
            eprintln!("[gate0] {} recv error for {payload}: {e}", phase.tag(&m));
        }) else {
            continue;
        };
        if buf[..n] != *payload.as_bytes() {
            return Err(format!(
                "unexpected echo {:?} from {from} for {payload}",
                String::from_utf8_lossy(&buf[..n])
            ));
        }
        let rtt = sent.elapsed();
        return wait_for_log(canary, m.container_port, &payload, LOG_SETTLE)
            .await
            .map(|()| rtt)
            .map_err(|log| {
                format!(
                    "{payload} echoed but never logged within {LOG_SETTLE:?}\n{}",
                    tail(&log)
                )
            });
    }
    let log = evidence_log(canary).await;
    Err(format!(
        "no echo for any of {ALLOWED_ATTEMPTS} datagrams to {target}/udp ({prefix}-*)\n{}",
        tail(&log)
    ))
}

/// Poll the canary log for the exact line; Err(last log) on timeout.
async fn wait_for_log(
    canary: &CanaryPod,
    port: u16,
    payload: &str,
    bound: Duration,
) -> Result<(), String> {
    let start = Instant::now();
    loop {
        let log = canary
            .logs()
            .await
            .unwrap_or_else(|e| format!("(log read failed: {e})"));
        if log_has(&log, port, payload) {
            return Ok(());
        }
        if start.elapsed() > bound {
            return Err(log);
        }
        tokio::time::sleep(LOG_POLL_INTERVAL).await;
    }
}
