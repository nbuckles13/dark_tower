//! MH template invariants and Kind-only process-namespace sharing — part of the
//! `kustomize` subcommand (story 4 task I-3; ADR-0039 §7 "Template invariants",
//! R-31). Runs over every render the kustomize policy builds.
//!
//! # Rule 1 — the ONE MH template keeps what makes every MH work
//!
//! Every MH in the slot pool is an instance of ONE Deployment (`mh-template`,
//! name read from `infra/kind/scripts/deploy.sh:MH_TEMPLATE_DEPLOYMENT`, never
//! retyped). `mh.sh` changes only slot labels, the webtransport hostPort and the
//! advertise ConfigMap name, so whatever the template loses, every MH loses. The
//! template must be present exactly once in the MH base render and in the Kind
//! environment-root render, and wherever it is rendered it must keep:
//!
//! * **scraped** — the pod labels [`REQUIRED_POD_LABELS`], and every `keep`
//!   relabel of Prometheus job [`PROMETHEUS_JOB`] (read from
//!   `infra/kubernetes/observability/prometheus.yml`, so the guard follows the
//!   scrape config instead of restating it) matches the pod labels and a
//!   declared containerPort;
//! * **reachable by MC, and policy-covered** — the containerPorts
//!   [`REQUIRED_CONTAINER_PORTS`]; a NetworkPolicy in the same render whose
//!   non-empty `podSelector` the pod labels satisfy (a pod no policy selects is
//!   default-allow), with Ingress+Egress, ingress from MC on the MH gRPC port
//!   and egress to MC on [`MC_GRPC_PORT`]; and, in the environment root, MC's
//!   own policy egress to the MH gRPC port selecting these labels;
//! * **slot-addressable** — the 4434/UDP port is the ONE port named
//!   `infra/kind/scripts/mh.sh:MH_WT_PORT_NAME`, the port `mh.sh` sets each
//!   slot's hostPort on. The template itself declares the hostPort through that
//!   name, so no OTHER port may carry a hostPort, and there is no
//!   `hostNetwork` (I-2 design; `scripts/setup.test.sh` pins the template's own
//!   hostPort absence and `mh.test.sh` the instantiation);
//! * **browser-trusted** — the [`TLS_SECRET`] Secret volume, mounted by the
//!   container that serves WebTransport.
//!
//! # Rule 2 — `shareProcessNamespace` is Kind-only test instrumentation
//!
//! POLICY CONTENT: security (CLAUDE.md guard-crate ownership). See
//! [`KIND_PID_NAMESPACE_SHARERS`].
//!
//! Each missing item is its own finding and names the property.

use crate::common::mh_slot_sources::{DEPLOY_SH, MH_SH, TEMPLATE_NAME_CONST, WT_PORT_NAME_CONST};
use crate::common::pod_spec::{containers, meta_str, pod_spec};
use crate::common::services::KIND_ENV;
use crate::common::shell_consts::shell_const;
use serde_norway::Value;
use std::collections::BTreeMap;
use std::path::Path;

/// The template is absent (or duplicated) where it must render exactly once.
pub const MH_TEMPLATE_ABSENT_RULE_ID: &str = "mh_template_absent";
/// A required template property is missing.
pub const MH_TEMPLATE_INVARIANT_RULE_ID: &str = "mh_template_invariant";
/// Rule 2: `shareProcessNamespace` where it may not be, or missing where it must be.
pub const SHARE_PROCESS_NAMESPACE_RULE_ID: &str = "share_process_namespace";
/// An input the rules derive from (deploy.sh, mh.sh, prometheus.yml) could not
/// be read, or an environment exists that this guard does not render. A
/// "could not check" class: fails closed.
pub const MH_TEMPLATE_UNVERIFIABLE_RULE_ID: &str = "mh_template_unverifiable";

/// Rule 1's ids — ONE `kustomize::fail_token` bucket (`kustomize-mh-template`);
/// rule 2's [`SHARE_PROCESS_NAMESPACE_RULE_ID`] is its own bucket.
pub const RULE_IDS: &[&str] = &[MH_TEMPLATE_ABSENT_RULE_ID, MH_TEMPLATE_INVARIANT_RULE_ID];

/// The MH service base (the template's home).
pub const MH_BASE_DIR: &str = "infra/services/mh-service";
/// The Prometheus scrape configuration (the `prometheus.yml` generator source).
pub const PROMETHEUS_YML: &str = "infra/kubernetes/observability/prometheus.yml";
/// The MH scrape job in [`PROMETHEUS_YML`].
pub const PROMETHEUS_JOB: &str = "mh-service";

// ---- ADR-0039 §7 "Template invariants" (policy content; the ADR is the home) --

/// Pod labels every MH carries: the MH and MC NetworkPolicies and Prometheus
/// pod-SD select on them.
pub const REQUIRED_POD_LABELS: &[(&str, &str)] = &[("app", "mh-service"), ("component", "media")];
/// The health/metrics port (Prometheus scrapes it).
pub const HEALTH_PORT: (u64, &str) = (8083, "TCP");
/// MH's gRPC port (MC → MH).
pub const MH_GRPC_PORT: u64 = 50053;
/// The WebTransport port — the one that carries the per-slot hostPort.
pub const WEBTRANSPORT_PORT: (u64, &str) = (4434, "UDP");
/// `(containerPort, protocol)` every MH declares: health/metrics, gRPC, WebTransport.
pub const REQUIRED_CONTAINER_PORTS: &[(u64, &str)] =
    &[HEALTH_PORT, (MH_GRPC_PORT, "TCP"), WEBTRANSPORT_PORT];
/// MC's gRPC port (MH → MC egress).
pub const MC_GRPC_PORT: u64 = 50052;
/// MC's pod label (the peer of both gRPC edges).
pub const MC_POD_LABEL: (&str, &str) = ("app", "mc-service");
/// The shared dev-cert Secret (provisioned by `provision.sh:create_tls_secret`).
pub const TLS_SECRET: &str = "mh-service-tls";

/// Rule 2 membership — the ADR-0038 "Kind-only test instrumentation" parity
/// allowlist entry for `shareProcessNamespace`, as a list. **This IS that
/// table's entry, not a copy of a decision made elsewhere: adding a member is
/// an ADR-0038 change (security owns the content).**
///
/// * Non-Kind renders (every `infra/services/*` base, any other environment):
///   NO pod template may set it.
/// * Kind renders: only the members below may, and the environment-root MH
///   template MUST (the zombie leg's SIGSTOP/SIGCONT injector needs it).
/// * The sharer stays SINGLE-CONTAINER: a second container sharing the PID
///   namespace could read `/proc/<mh-pid>/environ` and memory, including the
///   TLS key MH loads. Ephemeral debug containers are not in the template.
pub const KIND_PID_NAMESPACE_SHARERS: &[PidNamespaceSharer] = &[PidNamespaceSharer::MhTemplate];

/// A workload allowed to share its process namespace in Kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PidNamespaceSharer {
    /// The MH template (`Deployment/<MH_TEMPLATE_DEPLOYMENT>`).
    MhTemplate,
}

/// One finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MhTemplateFinding {
    pub rule_id: &'static str,
    pub detail: String,
}

/// One Prometheus `keep` relabel of [`PROMETHEUS_JOB`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeepRule {
    pub source_label: String,
    pub regex: String,
}

/// Values the rules read from the scripts and the scrape config.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MhTemplateContext {
    pub template_name: String,
    pub wt_port_name: String,
    pub prometheus_keeps: Vec<KeepRule>,
}

/// Which render is being checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderScope {
    /// A render of the Kind environment (its directory is under `overlays/kind`).
    pub is_kind: bool,
    /// The Kind environment root (what deploy.sh renders).
    pub is_env_root: bool,
    /// The MH service base (`infra/services/mh-service`).
    pub is_mh_base: bool,
}

/// Read the context from the tree. Each unreadable input is one error string.
///
/// # Errors
/// Every input that could not be read or parsed, one string each.
pub fn load_context(repo_root: &Path) -> Result<MhTemplateContext, Vec<String>> {
    let mut errs = Vec::new();
    let read =
        |rel: &str, errs: &mut Vec<String>| match std::fs::read_to_string(repo_root.join(rel)) {
            Ok(s) => Some(s),
            Err(e) => {
                errs.push(format!("{rel}: cannot read ({e})"));
                None
            }
        };
    let template_name = read(DEPLOY_SH, &mut errs).and_then(|s| {
        shell_const(&s, TEMPLATE_NAME_CONST)
            .map_err(|e| errs.push(format!("{DEPLOY_SH}: {e}")))
            .ok()
    });
    let wt_port_name = read(MH_SH, &mut errs).and_then(|s| {
        shell_const(&s, WT_PORT_NAME_CONST)
            .map_err(|e| errs.push(format!("{MH_SH}: {e}")))
            .ok()
    });
    let keeps = read(PROMETHEUS_YML, &mut errs).and_then(|s| {
        prometheus_keep_rules(&s, PROMETHEUS_JOB)
            .map_err(|e| errs.push(format!("{PROMETHEUS_YML}: {e}")))
            .ok()
    });
    match (template_name, wt_port_name, keeps) {
        (Some(template_name), Some(wt_port_name), Some(prometheus_keeps)) if errs.is_empty() => {
            Ok(MhTemplateContext {
                template_name,
                wt_port_name,
                prometheus_keeps,
            })
        }
        _ => Err(errs),
    }
}

/// The `keep` relabels of scrape job `job`.
///
/// # Errors
/// The config does not parse, the job is absent or duplicated, or a keep
/// relabel is not a single-source-label rule this guard can evaluate.
pub fn prometheus_keep_rules(prometheus_yml: &str, job: &str) -> Result<Vec<KeepRule>, String> {
    let doc: Value =
        serde_norway::from_str(prometheus_yml).map_err(|e| format!("does not parse: {e}"))?;
    let jobs: Vec<&Value> = doc
        .get("scrape_configs")
        .and_then(Value::as_sequence)
        .map(|s| {
            s.iter()
                .filter(|j| j.get("job_name").and_then(Value::as_str) == Some(job))
                .collect()
        })
        .unwrap_or_default();
    let [j] = jobs.as_slice() else {
        return Err(format!(
            "expected exactly one scrape job `{job}`, found {}",
            jobs.len()
        ));
    };
    let mut out = Vec::new();
    for r in j
        .get("relabel_configs")
        .and_then(Value::as_sequence)
        .map(Vec::as_slice)
        .unwrap_or_default()
    {
        if r.get("action").and_then(Value::as_str) != Some("keep") {
            continue;
        }
        let srcs: Vec<&str> = r
            .get("source_labels")
            .and_then(Value::as_sequence)
            .map(|s| s.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        let [src] = srcs.as_slice() else {
            return Err(format!(
                "job `{job}`: a keep relabel with source_labels {srcs:?} — only single-label \
                    keeps can be evaluated against the template"
            ));
        };
        let regex = r
            .get("regex")
            .and_then(|v| {
                v.as_str()
                    .map(str::to_string)
                    .or_else(|| v.as_u64().map(|n| n.to_string()))
            })
            .unwrap_or_else(|| "(.*)".to_string());
        out.push(KeepRule {
            source_label: (*src).to_string(),
            regex,
        });
    }
    if out.is_empty() {
        return Err(format!(
            "job `{job}` has no keep relabel — it does not select MH pods by anything the \
                template declares"
        ));
    }
    Ok(out)
}

fn kind_of(doc: &Value) -> &str {
    doc.get("kind").and_then(Value::as_str).unwrap_or("")
}

fn name_of(doc: &Value) -> &str {
    meta_str(doc, "name").unwrap_or("")
}

fn str_map(v: Option<&Value>) -> BTreeMap<String, String> {
    v.and_then(Value::as_mapping)
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| Some((k.as_str()?.to_string(), v.as_str()?.to_string())))
                .collect()
        })
        .unwrap_or_default()
}

/// `selector` is non-empty and every pair is in `labels`.
fn selects(selector: &BTreeMap<String, String>, labels: &BTreeMap<String, String>) -> bool {
    !selector.is_empty() && selector.iter().all(|(k, v)| labels.get(k) == Some(v))
}

/// A port's `protocol` (Kubernetes defaults it to TCP).
fn protocol(p: &Value) -> &str {
    p.get("protocol").and_then(Value::as_str).unwrap_or("TCP")
}

fn seq(v: Option<&Value>) -> &[Value] {
    v.and_then(Value::as_sequence).map_or(&[], Vec::as_slice)
}

fn selector_labels(v: &Value) -> BTreeMap<String, String> {
    str_map(v.get("podSelector").and_then(|s| s.get("matchLabels")))
}

/// Does NetworkPolicy rule `rule` (an ingress or egress element) admit
/// `port`/`proto` with a peer (`from`/`to` = `peer_key`) whose podSelector the
/// labels `peer` satisfy?
fn rule_admits(
    rule: &Value,
    peer_key: &str,
    peer: &BTreeMap<String, String>,
    port: u64,
    proto: &str,
) -> bool {
    let port_ok = seq(rule.get("ports"))
        .iter()
        .any(|p| p.get("port").and_then(Value::as_u64) == Some(port) && protocol(p) == proto);
    let peer_ok = seq(rule.get(peer_key))
        .iter()
        .any(|p| selects(&selector_labels(p), peer));
    port_ok && peer_ok
}

fn policy_rules<'a>(np: &'a Value, field: &str) -> &'a [Value] {
    seq(np.get("spec").and_then(|s| s.get(field)))
}

fn policy_types(np: &Value) -> Vec<&str> {
    seq(np.get("spec").and_then(|s| s.get("policyTypes")))
        .iter()
        .filter_map(Value::as_str)
        .collect()
}

/// NetworkPolicies in `docs` whose own podSelector `pred` accepts.
fn policies(docs: &[Value], pred: impl Fn(&BTreeMap<String, String>) -> bool) -> Vec<&Value> {
    docs.iter()
        .filter(|d| kind_of(d) == "NetworkPolicy")
        .filter(|d| d.get("spec").is_some_and(|s| pred(&selector_labels(s))))
        .collect()
}

fn sanitize_label(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

fn full_match(re: &str, value: &str) -> Result<bool, String> {
    regex::RegexBuilder::new(&format!("^(?:{re})$"))
        .build()
        .map(|r| r.is_match(value))
        .map_err(|e| format!("regex `{re}` does not compile: {e}"))
}

/// The parts of a template rule 1 reads, extracted once.
struct TemplateView<'a> {
    pod: &'a Value,
    labels: BTreeMap<String, String>,
    containers: Vec<&'a Value>,
    ports: Vec<&'a Value>,
}

impl<'a> TemplateView<'a> {
    fn new(tpl: &'a Value) -> Option<Self> {
        let pod = pod_spec(tpl)?;
        let labels = str_map(
            tpl.get("spec")
                .and_then(|s| s.get("template"))
                .and_then(|t| t.get("metadata"))
                .and_then(|m| m.get("labels")),
        );
        let containers: Vec<&Value> = seq(pod.get("containers")).iter().collect();
        let ports = containers
            .iter()
            .flat_map(|c| seq(c.get("ports")))
            .collect();
        Some(Self {
            pod,
            labels,
            containers,
            ports,
        })
    }

    fn named(&self, p: &Value, name: &str) -> bool {
        p.get("name").and_then(Value::as_str) == Some(name)
    }
}

fn labels_missing(v: &TemplateView<'_>, missing: &mut Vec<String>) {
    for (k, val) in REQUIRED_POD_LABELS {
        if v.labels.get(*k).map(String::as_str) != Some(*val) {
            missing.push(format!("pod label {k}={val}"));
        }
    }
}

/// Required ports; the WebTransport port is the ONE named for the slot
/// hostPort; no other host binding (hostPort elsewhere, hostNetwork, hostPID,
/// hostIPC — the last two expose the MH process to the node, a strict superset
/// of the Kind-only pod-local PID sharing rule 2 allows).
fn ports_missing(v: &TemplateView<'_>, ctx: &MhTemplateContext, missing: &mut Vec<String>) {
    let has_port = |n: u64, proto: &str| {
        v.ports.iter().any(|p| {
            p.get("containerPort").and_then(Value::as_u64) == Some(n) && protocol(p) == proto
        })
    };
    for (n, proto) in REQUIRED_CONTAINER_PORTS {
        if !has_port(*n, proto) {
            missing.push(format!("containerPort {n}/{proto}"));
        }
    }
    let wt = &ctx.wt_port_name;
    let named: Vec<&&Value> = v.ports.iter().filter(|p| v.named(p, wt)).collect();
    let (wt_n, wt_proto) = WEBTRANSPORT_PORT;
    let is_wt = |p: &Value| {
        p.get("containerPort").and_then(Value::as_u64) == Some(wt_n) && protocol(p) == wt_proto
    };
    if !matches!(named.as_slice(), [p] if is_wt(p)) {
        missing.push(format!(
            "exactly one containerPort named `{wt}` ({MH_SH}:{WT_PORT_NAME_CONST}, the port \
                mh.sh sets the slot hostPort on), and it is {wt_n}/{wt_proto} (found {})",
            named.len()
        ));
    }
    for p in v
        .ports
        .iter()
        .filter(|p| p.get("hostPort").is_some() && !v.named(p, wt))
    {
        let n = p.get("containerPort").and_then(Value::as_u64).unwrap_or(0);
        missing.push(format!(
            "no hostPort on any port but `{wt}` (containerPort {n} declares one)"
        ));
    }
    for (field, why) in [
        ("hostNetwork", "the slot hostPort is the only host binding"),
        (
            "hostPID",
            "it exposes the MH process to the node and every pod on it",
        ),
        ("hostIPC", "it shares the node's IPC namespace"),
    ] {
        if v.pod.get(field).and_then(Value::as_bool) == Some(true) {
            missing.push(format!("no {field} ({why})"));
        }
    }
}

/// The TLS Secret volume, mounted by the container serving WebTransport.
fn tls_missing(v: &TemplateView<'_>, ctx: &MhTemplateContext, missing: &mut Vec<String>) {
    let tls_volumes: Vec<&str> = seq(v.pod.get("volumes"))
        .iter()
        .filter(|vol| {
            vol.get("secret")
                .and_then(|s| s.get("secretName"))
                .and_then(Value::as_str)
                == Some(TLS_SECRET)
        })
        .filter_map(|vol| vol.get("name").and_then(Value::as_str))
        .collect();
    let mounted = v
        .containers
        .iter()
        .filter(|c| {
            seq(c.get("ports"))
                .iter()
                .any(|p| v.named(p, &ctx.wt_port_name))
        })
        .any(|c| {
            seq(c.get("volumeMounts")).iter().any(|m| {
                m.get("name")
                    .and_then(Value::as_str)
                    .is_some_and(|n| tls_volumes.contains(&n))
            })
        });
    if !mounted {
        missing.push(format!(
            "the `{TLS_SECRET}` Secret volume, mounted by the container serving `{}`",
            ctx.wt_port_name
        ));
    }
}

/// Every Prometheus `keep` relabel of the MH job matches the template.
fn prometheus_missing(v: &TemplateView<'_>, ctx: &MhTemplateContext, missing: &mut Vec<String>) {
    let declared: Vec<u64> = v
        .ports
        .iter()
        .filter_map(|p| p.get("containerPort").and_then(Value::as_u64))
        .collect();
    for keep in &ctx.prometheus_keeps {
        let outcome = if let Some(l) = keep
            .source_label
            .strip_prefix("__meta_kubernetes_pod_label_")
        {
            let vals: Vec<&String> = v
                .labels
                .iter()
                .filter(|(k, _)| sanitize_label(k) == l)
                .map(|(_, x)| x)
                .collect();
            match vals.as_slice() {
                [x] => full_match(&keep.regex, x),
                _ => Ok(false),
            }
        } else if keep.source_label == "__meta_kubernetes_pod_container_port_number" {
            declared
                .iter()
                .map(|p| full_match(&keep.regex, &p.to_string()))
                .collect::<Result<Vec<bool>, String>>()
                .map(|r| r.into_iter().any(|b| b))
        } else {
            Err(format!(
                "source label `{}` cannot be evaluated against a pod template",
                keep.source_label
            ))
        };
        match outcome {
            Ok(true) => {}
            Ok(false) => missing.push(format!(
                "Prometheus job `{PROMETHEUS_JOB}` keep relabel {}=~`{}` ({PROMETHEUS_YML}) \
                    matches the template",
                keep.source_label, keep.regex
            )),
            Err(e) => missing.push(format!(
                "an evaluable Prometheus job `{PROMETHEUS_JOB}` keep \
                relabel: {e}"
            )),
        }
    }
}

fn mc_labels() -> BTreeMap<String, String> {
    [(MC_POD_LABEL.0.to_string(), MC_POD_LABEL.1.to_string())].into()
}

/// A NetworkPolicy selecting the template's pods, with both gRPC edges to MC.
fn mh_policy_missing(v: &TemplateView<'_>, docs: &[Value], missing: &mut Vec<String>) {
    let mc = mc_labels();
    let mh_policies = policies(docs, |sel| selects(sel, &v.labels));
    if mh_policies.is_empty() {
        missing.push(
            "a NetworkPolicy whose non-empty podSelector the template's pod labels satisfy \
                (else default-allow on every port)"
                .to_string(),
        );
        return;
    }
    let (mc_k, mc_v) = MC_POD_LABEL;
    let checks = [
        (
            mh_policies.iter().any(|np| {
                let t = policy_types(np);
                t.contains(&"Ingress") && t.contains(&"Egress")
            }),
            "the MH NetworkPolicy's policyTypes Ingress and Egress".to_string(),
        ),
        (
            mh_policies.iter().any(|np| {
                policy_rules(np, "egress")
                    .iter()
                    .any(|r| rule_admits(r, "to", &mc, MC_GRPC_PORT, "TCP"))
            }),
            format!("MH NetworkPolicy egress to {mc_k}={mc_v} on TCP {MC_GRPC_PORT}"),
        ),
        (
            mh_policies.iter().any(|np| {
                policy_rules(np, "ingress")
                    .iter()
                    .any(|r| rule_admits(r, "from", &mc, MH_GRPC_PORT, "TCP"))
            }),
            format!("MH NetworkPolicy ingress from {mc_k}={mc_v} on TCP {MH_GRPC_PORT}"),
        ),
    ];
    missing.extend(checks.into_iter().filter(|(ok, _)| !ok).map(|(_, m)| m));
}

/// MC's own policy (its selector carries MC's pod label) reaches the template.
fn mc_policy_missing(v: &TemplateView<'_>, docs: &[Value], missing: &mut Vec<String>) {
    let mc = mc_labels();
    let reaches = policies(docs, |sel| selects(&mc, sel)).iter().any(|np| {
        policy_rules(np, "egress")
            .iter()
            .any(|r| rule_admits(r, "to", &v.labels, MH_GRPC_PORT, "TCP"))
    });
    if !reaches {
        missing.push(format!(
            "MC NetworkPolicy egress on TCP {MH_GRPC_PORT} to a podSelector the template's pod \
                labels satisfy"
        ));
    }
}

/// Rule 1 over one template document; `docs` is the whole render.
fn check_template(
    tpl: &Value,
    docs: &[Value],
    scope: RenderScope,
    ctx: &MhTemplateContext,
) -> Vec<String> {
    let Some(v) = TemplateView::new(tpl) else {
        return vec!["a pod spec (spec.template.spec)".to_string()];
    };
    let mut missing = Vec::new();
    labels_missing(&v, &mut missing);
    ports_missing(&v, ctx, &mut missing);
    tls_missing(&v, ctx, &mut missing);
    prometheus_missing(&v, ctx, &mut missing);
    mh_policy_missing(&v, docs, &mut missing);
    if scope.is_env_root {
        mc_policy_missing(&v, docs, &mut missing);
    }
    missing
}

/// Does any mapping in `v` (recursively) set `shareProcessNamespace: true`?
fn shares_pid_namespace(v: &Value) -> bool {
    match v {
        Value::Mapping(m) => m.iter().any(|(k, val)| {
            (k.as_str() == Some("shareProcessNamespace") && val.as_bool() == Some(true))
                || shares_pid_namespace(val)
        }),
        Value::Sequence(s) => s.iter().any(shares_pid_namespace),
        Value::Tagged(t) => shares_pid_namespace(&t.value),
        _ => false,
    }
}

fn is_sharer(doc: &Value, ctx: &MhTemplateContext) -> bool {
    KIND_PID_NAMESPACE_SHARERS.iter().any(|s| match s {
        PidNamespaceSharer::MhTemplate => {
            kind_of(doc) == "Deployment" && name_of(doc) == ctx.template_name
        }
    })
}

/// Rules 1 and 2 over one parsed render.
#[must_use]
pub fn check_docs(
    docs: &[Value],
    label: &str,
    scope: RenderScope,
    ctx: &MhTemplateContext,
) -> Vec<MhTemplateFinding> {
    let mut out = Vec::new();
    let templates: Vec<&Value> = docs
        .iter()
        .filter(|d| kind_of(d) == "Deployment" && name_of(d) == ctx.template_name)
        .collect();
    let required = scope.is_env_root || scope.is_mh_base;
    if (required && templates.len() != 1) || templates.len() > 1 {
        out.push(MhTemplateFinding {
            rule_id: MH_TEMPLATE_ABSENT_RULE_ID,
            detail: format!(
                "{label} — expected exactly one Deployment/{} (the MH template; \
                    {DEPLOY_SH}:{TEMPLATE_NAME_CONST}), found {}",
                ctx.template_name,
                templates.len()
            ),
        });
    }
    for tpl in &templates {
        for property in check_template(tpl, docs, scope, ctx) {
            out.push(MhTemplateFinding {
                rule_id: MH_TEMPLATE_INVARIANT_RULE_ID,
                detail: format!(
                    "{label} — Deployment/{} is missing: {property}",
                    ctx.template_name
                ),
            });
        }
    }

    // Rule 2.
    for doc in docs.iter().filter(|d| shares_pid_namespace(d)) {
        let what = format!("{}/{}", kind_of(doc), name_of(doc));
        if !scope.is_kind {
            out.push(MhTemplateFinding {
                rule_id: SHARE_PROCESS_NAMESPACE_RULE_ID,
                detail: format!(
                    "{label} — {what} sets shareProcessNamespace: true in a NON-Kind render; it \
                        is Kind-only test instrumentation (ADR-0038 parity allowlist) and must \
                        be absent from every base and every non-Kind render"
                ),
            });
        } else if !is_sharer(doc, ctx) {
            out.push(MhTemplateFinding {
                rule_id: SHARE_PROCESS_NAMESPACE_RULE_ID,
                detail: format!(
                    "{label} — {what} sets shareProcessNamespace: true; in Kind only the MH \
                        template may (ADR-0038 parity allowlist, KIND_PID_NAMESPACE_SHARERS)"
                ),
            });
        } else {
            // containers + initContainers: a native sidecar (initContainer with
            // restartPolicy: Always) runs beside MH in the shared namespace.
            let n = pod_spec(doc).map_or(0, |p| containers(p).len());
            if n != 1 {
                out.push(MhTemplateFinding {
                    rule_id: SHARE_PROCESS_NAMESPACE_RULE_ID,
                    detail: format!(
                        "{label} — {what} shares its process namespace with {n} containers + \
                            initContainers; the sharer must stay single-container (a \
                            co-container could read the MH process's environ and TLS key)"
                    ),
                });
            }
        }
    }
    if scope.is_env_root {
        for tpl in &templates {
            let set = pod_spec(tpl)
                .and_then(|p| p.get("shareProcessNamespace"))
                .and_then(Value::as_bool)
                == Some(true);
            if !set {
                out.push(MhTemplateFinding {
                    rule_id: SHARE_PROCESS_NAMESPACE_RULE_ID,
                    detail: format!(
                        "{label} — Deployment/{} lacks shareProcessNamespace: true in the Kind \
                            environment root; the Kind overlay patch \
                         (overlays/kind/services/mh-service/share-process-namespace-patch.yaml) \
                         did not apply, and the zombie-leg injector (mh.sh signal) would refuse",
                        ctx.template_name
                    ),
                });
            }
        }
    }
    out
}

/// Rules 1 and 2 over one rendered YAML stream.
#[must_use]
pub fn check_rendered(
    rendered: &str,
    label: &str,
    scope: RenderScope,
    ctx: &MhTemplateContext,
) -> Vec<MhTemplateFinding> {
    match crate::common::pod_spec::parse_docs(rendered) {
        Ok(docs) => check_docs(&docs, label, scope, ctx),
        Err(e) => vec![MhTemplateFinding {
            rule_id: MH_TEMPLATE_UNVERIFIABLE_RULE_ID,
            detail: format!(
                "{label} — the render does not parse as YAML, so the MH template \
                rules could not run: {e}"
            ),
        }],
    }
}

/// Environments under `overlays_dir` other than `kind`: rule 2 says
/// "absent from every non-Kind render", and the kustomize policy renders only
/// Kind, so another environment is unchecked surface — fail closed.
///
/// # Errors
/// The overlays directory exists but cannot be listed.
pub fn unrendered_environments(overlays_dir: &Path) -> std::io::Result<Vec<String>> {
    if !overlays_dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for e in std::fs::read_dir(overlays_dir)? {
        let e = e?;
        let name = e.file_name().to_string_lossy().to_string();
        if e.path().is_dir() && name != KIND_ENV {
            out.push(name);
        }
    }
    out.sort();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEMPLATE: &str = r#"
apiVersion: apps/v1
kind: Deployment
metadata:
  name: mh-template
spec:
  selector:
    matchLabels: {app: mh-service, component: media}
  template:
    metadata:
      labels: {app: mh-service, component: media}
    spec:
      containers:
      - name: mh-service
        ports:
        - {name: health, containerPort: 8083, protocol: TCP}
        - {name: grpc, containerPort: 50053, protocol: TCP}
        - {name: webtransport, containerPort: 4434, protocol: UDP}
        volumeMounts:
        - {name: mh-tls, mountPath: /etc/mh-tls}
      volumes:
      - name: mh-tls
        secret: {secretName: mh-service-tls}
"#;

    const MH_POLICY: &str = r#"
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: mh-service
spec:
  podSelector:
    matchLabels: {app: mh-service, component: media}
  policyTypes: [Ingress, Egress]
  ingress:
  - from:
    - podSelector: {matchLabels: {app: mc-service}}
    ports: [{protocol: TCP, port: 50053}]
  egress:
  - to:
    - podSelector: {matchLabels: {app: mc-service}}
    ports: [{protocol: TCP, port: 50052}]
"#;

    const MC_POLICY: &str = r#"
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: mc-service
spec:
  podSelector:
    matchLabels: {app: mc-service, component: signaling}
  policyTypes: [Ingress, Egress]
  egress:
  - to:
    - podSelector: {matchLabels: {app: mh-service}}
    ports: [{protocol: TCP, port: 50053}]
"#;

    const BASE: RenderScope = RenderScope {
        is_kind: false,
        is_env_root: false,
        is_mh_base: true,
    };
    const ROOT: RenderScope = RenderScope {
        is_kind: true,
        is_env_root: true,
        is_mh_base: false,
    };
    const OTHER_BASE: RenderScope = RenderScope {
        is_kind: false,
        is_env_root: false,
        is_mh_base: false,
    };

    fn ctx() -> MhTemplateContext {
        MhTemplateContext {
            template_name: "mh-template".to_string(),
            wt_port_name: "webtransport".to_string(),
            prometheus_keeps: vec![
                KeepRule {
                    source_label: "__meta_kubernetes_pod_label_app".to_string(),
                    regex: "mh-service".to_string(),
                },
                KeepRule {
                    source_label: "__meta_kubernetes_pod_container_port_number".to_string(),
                    regex: "8083".to_string(),
                },
            ],
        }
    }

    fn docs(parts: &[&str]) -> Vec<Value> {
        crate::common::pod_spec::parse_docs(&parts.join("\n---\n")).unwrap()
    }

    fn kind_template() -> String {
        TEMPLATE.replace(
            "    spec:\n      containers:",
            "    spec:\n      shareProcessNamespace: true\n      containers:",
        )
    }

    fn findings(parts: &[&str], scope: RenderScope) -> Vec<MhTemplateFinding> {
        check_docs(&docs(parts), "t", scope, &ctx())
    }

    fn assert_one(f: &[MhTemplateFinding], rule: &str, needle: &str) {
        assert!(
            f.iter()
                .any(|x| x.rule_id == rule && x.detail.contains(needle)),
            "expected a {rule} finding naming `{needle}`, got {f:?}"
        );
    }

    // ---- positive arms -------------------------------------------------------

    #[test]
    fn base_template_passes() {
        assert_eq!(findings(&[TEMPLATE, MH_POLICY], BASE), vec![]);
    }

    #[test]
    fn kind_root_template_passes() {
        let k = kind_template();
        assert_eq!(findings(&[&k, MH_POLICY, MC_POLICY], ROOT), vec![]);
    }

    // ---- rule 1 negatives: each property --------------------------------------

    #[test]
    fn missing_template_in_base_fails() {
        assert_one(
            &findings(&[MH_POLICY], BASE),
            MH_TEMPLATE_ABSENT_RULE_ID,
            "found 0",
        );
    }

    #[test]
    fn missing_each_label_fails() {
        for (k, v) in REQUIRED_POD_LABELS {
            let kept: Vec<String> = REQUIRED_POD_LABELS
                .iter()
                .filter(|(kk, _)| kk != k)
                .map(|(kk, vv)| format!("{kk}: {vv}"))
                .collect();
            let t = TEMPLATE.replace(
                "      labels: {app: mh-service, component: media}",
                &format!("      labels: {{{}}}", kept.join(", ")),
            );
            assert_one(
                &findings(&[&t, MH_POLICY], BASE),
                MH_TEMPLATE_INVARIANT_RULE_ID,
                &format!("pod label {k}={v}"),
            );
        }
    }

    #[test]
    fn missing_each_port_fails() {
        for (n, proto) in REQUIRED_CONTAINER_PORTS {
            let t = TEMPLATE.replace(
                &format!("containerPort: {n}, protocol: {proto}"),
                &format!("containerPort: {}, protocol: {proto}", n + 1),
            );
            assert_one(
                &findings(&[&t, MH_POLICY], BASE),
                MH_TEMPLATE_INVARIANT_RULE_ID,
                &format!("containerPort {n}/{proto}"),
            );
        }
    }

    #[test]
    fn webtransport_port_not_named_for_the_hostport_fails() {
        let t = TEMPLATE.replace("name: webtransport", "name: wt");
        assert_one(
            &findings(&[&t, MH_POLICY], BASE),
            MH_TEMPLATE_INVARIANT_RULE_ID,
            "exactly one containerPort named `webtransport`",
        );
    }

    #[test]
    fn webtransport_name_on_the_wrong_port_fails() {
        let t = TEMPLATE
            .replace("name: webtransport", "name: wt")
            .replace("name: grpc", "name: webtransport");
        assert_one(
            &findings(&[&t, MH_POLICY], BASE),
            MH_TEMPLATE_INVARIANT_RULE_ID,
            "it is 4434/UDP",
        );
    }

    #[test]
    fn hostport_on_another_port_fails() {
        let t = TEMPLATE.replace(
            "containerPort: 8083, protocol: TCP",
            "containerPort: 8083, hostPort: 8083, protocol: TCP",
        );
        assert_one(
            &findings(&[&t, MH_POLICY], BASE),
            MH_TEMPLATE_INVARIANT_RULE_ID,
            "no hostPort on any port but",
        );
    }

    #[test]
    fn host_network_fails() {
        let t = TEMPLATE.replace(
            "    spec:\n      containers:",
            "    spec:\n      hostNetwork: true\n      containers:",
        );
        assert_one(
            &findings(&[&t, MH_POLICY], BASE),
            MH_TEMPLATE_INVARIANT_RULE_ID,
            "no hostNetwork",
        );
    }

    #[test]
    fn missing_tls_secret_volume_fails() {
        let t = TEMPLATE.replace("secretName: mh-service-tls", "secretName: other");
        assert_one(
            &findings(&[&t, MH_POLICY], BASE),
            MH_TEMPLATE_INVARIANT_RULE_ID,
            "`mh-service-tls` Secret volume",
        );
        let unmounted = TEMPLATE.replace("{name: mh-tls, mountPath", "{name: tmp, mountPath");
        assert_one(
            &findings(&[&unmounted, MH_POLICY], BASE),
            MH_TEMPLATE_INVARIANT_RULE_ID,
            "`mh-service-tls` Secret volume",
        );
    }

    #[test]
    fn prometheus_keep_mismatch_fails() {
        let mut c = ctx();
        c.prometheus_keeps[1].regex = "9999".to_string();
        let f = check_docs(&docs(&[TEMPLATE, MH_POLICY]), "t", BASE, &c);
        assert_one(
            &f,
            MH_TEMPLATE_INVARIANT_RULE_ID,
            "keep relabel __meta_kubernetes_pod_container_port_number",
        );
        let mut c = ctx();
        c.prometheus_keeps[0].regex = "media-handler".to_string();
        let f = check_docs(&docs(&[TEMPLATE, MH_POLICY]), "t", BASE, &c);
        assert_one(
            &f,
            MH_TEMPLATE_INVARIANT_RULE_ID,
            "keep relabel __meta_kubernetes_pod_label_app",
        );
    }

    #[test]
    fn no_policy_covering_the_labels_fails() {
        let p = MH_POLICY.replace(
            "matchLabels: {app: mh-service, component: media}",
            "matchLabels: {app: mh-service, component: other}",
        );
        assert_one(
            &findings(&[TEMPLATE, &p], BASE),
            MH_TEMPLATE_INVARIANT_RULE_ID,
            "a NetworkPolicy whose non-empty podSelector",
        );
    }

    #[test]
    fn policy_without_mc_egress_fails() {
        let p = MH_POLICY.replace("port: 50052", "port: 50099");
        assert_one(
            &findings(&[TEMPLATE, &p], BASE),
            MH_TEMPLATE_INVARIANT_RULE_ID,
            "egress to app=mc-service on TCP 50052",
        );
    }

    #[test]
    fn policy_without_mc_ingress_fails() {
        let p = MH_POLICY.replace(
            "ports: [{protocol: TCP, port: 50053}]",
            "ports: [{protocol: TCP, port: 50099}]",
        );
        assert_one(
            &findings(&[TEMPLATE, &p], BASE),
            MH_TEMPLATE_INVARIANT_RULE_ID,
            "ingress from app=mc-service on TCP 50053",
        );
    }

    #[test]
    fn mc_policy_not_reaching_mh_fails_in_root() {
        let k = kind_template();
        let p = MC_POLICY.replace(
            "matchLabels: {app: mh-service}",
            "matchLabels: {app: media-handler}",
        );
        assert_one(
            &findings(&[&k, MH_POLICY, &p], ROOT),
            MH_TEMPLATE_INVARIANT_RULE_ID,
            "MC NetworkPolicy egress on TCP 50053",
        );
    }

    #[test]
    fn host_pid_and_host_ipc_fail_in_every_render() {
        for field in ["hostPID", "hostIPC"] {
            let t = TEMPLATE.replace(
                "    spec:\n      containers:",
                &format!("    spec:\n      {field}: true\n      containers:"),
            );
            assert_one(
                &findings(&[&t, MH_POLICY], BASE),
                MH_TEMPLATE_INVARIANT_RULE_ID,
                &format!("no {field}"),
            );
            let k = kind_template().replace(
                "    spec:\n      shareProcessNamespace: true",
                &format!("    spec:\n      shareProcessNamespace: true\n      {field}: true"),
            );
            assert_one(
                &findings(&[&k, MH_POLICY, MC_POLICY], ROOT),
                MH_TEMPLATE_INVARIANT_RULE_ID,
                &format!("no {field}"),
            );
        }
    }

    // ---- rule 2 ---------------------------------------------------------------

    #[test]
    fn share_process_namespace_in_base_fails() {
        let k = kind_template();
        assert_one(
            &findings(&[&k, MH_POLICY], BASE),
            SHARE_PROCESS_NAMESPACE_RULE_ID,
            "NON-Kind render",
        );
    }

    #[test]
    fn share_process_namespace_on_any_non_kind_workload_fails() {
        let other = "apiVersion: batch/v1\nkind: CronJob\nmetadata: {name: x}\nspec:\n  jobTemplate:\n    spec:\n      template:\n        spec:\n          shareProcessNamespace: true\n          containers: [{name: a}]\n";
        assert_one(
            &findings(&[other], OTHER_BASE),
            SHARE_PROCESS_NAMESPACE_RULE_ID,
            "CronJob/x",
        );
    }

    #[test]
    fn share_process_namespace_missing_from_kind_root_fails() {
        assert_one(
            &findings(&[TEMPLATE, MH_POLICY, MC_POLICY], ROOT),
            SHARE_PROCESS_NAMESPACE_RULE_ID,
            "lacks shareProcessNamespace",
        );
    }

    #[test]
    fn share_process_namespace_on_another_kind_workload_fails() {
        let k = kind_template();
        let other = "apiVersion: apps/v1\nkind: Deployment\nmetadata: {name: gc-service}\nspec:\n  template:\n    spec:\n      shareProcessNamespace: true\n      containers: [{name: a}]\n";
        assert_one(
            &findings(&[&k, MH_POLICY, MC_POLICY, other], ROOT),
            SHARE_PROCESS_NAMESPACE_RULE_ID,
            "only the MH template may",
        );
    }

    #[test]
    fn kind_template_sharing_with_two_containers_fails() {
        let k = kind_template().replace("      volumes:", "      - name: sidecar\n      volumes:");
        assert_one(
            &findings(&[&k, MH_POLICY, MC_POLICY], ROOT),
            SHARE_PROCESS_NAMESPACE_RULE_ID,
            "single-container",
        );
    }

    #[test]
    fn kind_template_sharing_with_a_native_sidecar_fails() {
        let k = kind_template().replace(
            "      volumes:",
            "      initContainers:\n      - name: sidecar\n        restartPolicy: Always\n      volumes:",
        );
        assert_one(
            &findings(&[&k, MH_POLICY, MC_POLICY], ROOT),
            SHARE_PROCESS_NAMESPACE_RULE_ID,
            "containers + initContainers",
        );
    }

    #[test]
    fn a_non_kind_environment_is_unrendered_and_fails_closed() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join(KIND_ENV)).unwrap();
        assert_eq!(
            unrendered_environments(d.path()).unwrap(),
            Vec::<String>::new()
        );
        std::fs::create_dir_all(d.path().join("prod")).unwrap();
        std::fs::write(d.path().join("README.md"), "x").unwrap();
        assert_eq!(
            unrendered_environments(d.path()).unwrap(),
            vec!["prod".to_string()]
        );
        assert_eq!(
            unrendered_environments(&d.path().join("absent")).unwrap(),
            Vec::<String>::new()
        );
    }

    // ---- inputs ---------------------------------------------------------------

    #[test]
    fn prometheus_keep_rules_read_the_job() {
        let y = "scrape_configs:\n- job_name: other\n- job_name: mh-service\n  relabel_configs:\n  - {source_labels: [__meta_kubernetes_pod_label_app], action: keep, regex: mh-service}\n  - {source_labels: [a], action: replace}\n";
        let k = prometheus_keep_rules(y, "mh-service").unwrap();
        assert_eq!(k.len(), 1);
        assert!(prometheus_keep_rules(y, "absent").is_err());
        assert!(
            prometheus_keep_rules("scrape_configs:\n- job_name: mh-service\n", "mh-service")
                .is_err()
        );
    }
}
