//! `mh-slot-model` subcommand — the MH slot pool's deploy-time structure
//! (story 4 task I-3; ADR-0039 §7, R-31).
//!
//! # Rule 3 — no MH workload in the environment-root apply set
//!
//! The ONE MH template reaches the cluster only as ConfigMap `mh-template`,
//! which `mh.sh` instantiates per hostPort slot. A Deployment of it in the root
//! apply set would be an MH with no slot: no hostPort lock, an unrewritten
//! advertise reference, and a second process GC could register. The guard does
//! not re-derive the apply set: it runs deploy.sh's own
//! `env_root_apply_list` (what `apply_env_root` applies) over deploy.sh's own
//! wrapper render, and fails on any object in it whose pod template carries the
//! MH template's selector labels or runs the MH template's image repo (both read
//! off the rendered template), and on any template-input document.
//!
//! # Rule 4 — slot-table drift
//!
//! K and the slot ports have ONE home, `crates/devloop-helper/src/ports.rs`.
//! Every downstream copy is compared as a SET, each read from the artifact that
//! owns it (never a retyped value):
//!
//! * K (`PortOffsets::MH_SLOT_COUNT`) vs the static `infra/kind/kind-config.yaml`
//!   slot markers (`MH_SLOT_MARKER`, read from ports.rs) and deploy.sh's
//!   `mh_slot_table` rows; each slot port is a UDP extraPortMapping with
//!   containerPort == hostPort;
//! * `infra/kind/kind-config.yaml.tmpl` carries the slot block ONLY as the one
//!   placeholder ports.rs's `template_env_vars` binds to `mh_slot_port_mappings`
//!   (ports.rs's own unit test pins the rendered block), with no literal marker;
//! * the per-slot advertise-address ConfigMaps (label `MH_SLOT_LABEL`, from the
//!   wrapper render): one per slot, `MH_HOST_PORT` and
//!   `MH_WEBTRANSPORT_ADVERTISE_ADDRESS` equal to the slot table's;
//! * GC's RegisterMH allowlists as GC actually receives them (the rendered
//!   ConfigMap carrying them, after deploy.sh's merge):
//!   `GC_MH_WT_ALLOWED_PORTS` == the slot ports, `GC_MH_WT_ALLOWED_HOSTS` == the
//!   slot hosts, `GC_MH_GRPC_ALLOWED_CIDRS` == `networking.podSubnet` of BOTH
//!   kind-configs, `GC_MH_GRPC_ALLOWED_PORTS` == the port of the template's
//!   `MH_GRPC_ADVERTISE_ADDRESS`, itself a declared TCP containerPort.
//!   `GC_MH_GRPC_ALLOWED_SCHEMES` is policy (base config.env) and not compared.
//!
//! An empty side never passes a comparison: emptiness is checked first, with its
//! own token (`mh-slot-model-vacuous`).
//!
//! # Hermetic
//!
//! The probe sources deploy.sh with every inherited `DT_*` variable removed, so it
//! reads the static kind-config (the one checked into the tree) whatever
//! environment the guard runs in. It needs bash, kubectl and jq; any failure is
//! `mh-slot-model-unverifiable` (fails closed). Secrets are dropped inside the
//! probe; nothing is written to disk but the wrapper kustomization (which
//! deploy.sh itself writes to a temp dir).

use crate::common::explain::{print_finding, Finding};
use crate::common::mh_slot_sources::{
    DEPLOY_SH, SLOT_LABEL_CONST, TEMPLATE_INPUT_LABEL_CONST, TEMPLATE_NAME_CONST,
};
use crate::common::rust_consts::{rust_str_consts, rust_uint_consts};
use crate::common::shell_consts::shell_const;
use crate::common::status::{emit_ok, emit_scope};
use anyhow::Result;
use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

pub const MH_IN_ROOT_APPLY_SET_RULE_ID: &str = "mh_in_root_apply_set";
pub const SLOT_TABLE_DRIFT_RULE_ID: &str = "slot_table_drift";
pub const VACUOUS_RULE_ID: &str = "mh_slot_model_vacuous";
pub const UNVERIFIABLE_RULE_ID: &str = "mh_slot_model_unverifiable";

pub const PORTS_RS: &str = "crates/devloop-helper/src/ports.rs";
/// [`PORTS_RS`] constant: K, the slot count (THE one home).
pub const SLOT_COUNT_CONST: &str = "MH_SLOT_COUNT";
/// [`PORTS_RS`] constant: the kind-config slot marker.
pub const SLOT_MARKER_CONST: &str = "MH_SLOT_MARKER";
/// deploy.sh's slot-table derivation and apply-set derivation (finding text).
const SLOT_TABLE: &str = "deploy.sh:mh_slot_table";
const APPLY_SET: &str = "deploy.sh:env_root_apply_list";
pub const STATIC_KIND_CONFIG: &str = "infra/kind/kind-config.yaml";
pub const KIND_CONFIG_TMPL: &str = "infra/kind/kind-config.yaml.tmpl";

/// GC's allowlist keys compared here (ADR-0039 §7 "GC registration rule").
/// `GC_MH_GRPC_ALLOWED_SCHEMES` is deliberately absent: policy, not derived.
pub const GC_WT_PORTS: &str = "GC_MH_WT_ALLOWED_PORTS";
pub const GC_WT_HOSTS: &str = "GC_MH_WT_ALLOWED_HOSTS";
pub const GC_GRPC_CIDRS: &str = "GC_MH_GRPC_ALLOWED_CIDRS";
pub const GC_GRPC_PORTS: &str = "GC_MH_GRPC_ALLOWED_PORTS";
const GC_KEYS: &[&str] = &[GC_WT_HOSTS, GC_WT_PORTS, GC_GRPC_CIDRS, GC_GRPC_PORTS];

/// Slot ConfigMap keys (deploy.sh:render_env_overlay; mh.sh's interface).
pub const SLOT_HOST_PORT_KEY: &str = "MH_HOST_PORT";
pub const SLOT_ADDRESS_KEY: &str = "MH_WEBTRANSPORT_ADVERTISE_ADDRESS";
/// The template env var whose port GC's gRPC port allowlist carries.
pub const GRPC_ADVERTISE_ENV: &str = "MH_GRPC_ADVERTISE_ADDRESS";

/// The probe: deploy.sh's own derivations over the static kind-config.
/// `$1` = deploy.sh (cleared before sourcing: deploy.sh parses its own
/// arguments at source time). Sections are separated by marker lines.
const PROBE: &str = r##"set -euo pipefail
deploy="$1"
set --
source "$deploy"
cache_renders
printf -v tag 'sha-%0*d' "${CONTENT_TAG_LEN}" 0
for r in $(root_repos); do IMAGE_REFS[$r]="$r:$tag"; done
wrap="$(mktemp -d "${TMPDIR:-/tmp}/dt-guard-mh-slot.XXXXXX")"
trap 'rm -rf "$wrap"' EXIT
render_env_overlay "$wrap"
render="$(render_env_root_json "$wrap" | jq -c 'select(.kind != "Secret")')"
apply="$(printf '%s\n' "$render" | env_root_apply_list)"
slots="$(mh_slot_table)"
echo "@@@ SLOTS"
printf '%s\n' "$slots"
echo "@@@ RENDER"
printf '%s\n' "$render"
echo "@@@ APPLY"
printf '%s\n' "$apply"
echo "@@@ END"
"##;

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at \
        load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static PLACEHOLDER_VAR_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#""([A-Z][A-Z0-9_]*)"\s*\.to_string\(\)\s*,\s*mh_slot_port_mappings\s*\("#)
        .expect("static pattern compiles")
});

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at \
        load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static GRPC_ADVERTISE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^[a-z][a-z0-9+]*://\$\(POD_IP\):([1-9][0-9]{0,4})$")
        .expect("static pattern compiles")
});

/// One finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotFinding {
    pub rule_id: &'static str,
    pub detail: String,
}

fn finding(rule_id: &'static str, detail: impl Into<String>) -> SlotFinding {
    SlotFinding {
        rule_id,
        detail: detail.into(),
    }
}

/// One `mh_slot_table` row: `<slot> <host_port> <advertise_url>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotRow {
    pub slot: u64,
    pub port: u64,
    pub url: String,
}

/// What the deploy.sh probe produced.
#[derive(Debug, Clone, Default)]
pub struct ProbeOutput {
    pub slots: Vec<SlotRow>,
    /// The wrapper render, Secrets dropped.
    pub render: Vec<Value>,
    /// `env_root_apply_list`'s `items`.
    pub apply: Vec<Value>,
}

/// The tree files the rules read.
#[derive(Debug, Clone, Default)]
pub struct TreeInputs {
    pub ports_rs: String,
    pub static_kind_config: String,
    pub kind_config_tmpl: String,
    pub deploy_sh: String,
}

/// Parse the probe's stdout.
///
/// # Errors
/// A missing section or an unparseable line.
pub fn parse_probe(stdout: &str) -> Result<ProbeOutput, String> {
    let mut section = "";
    let mut out = ProbeOutput::default();
    let mut ended = false;
    for line in stdout.lines() {
        if let Some(s) = line.strip_prefix("@@@ ") {
            section = match s {
                "SLOTS" | "RENDER" | "APPLY" => s,
                "END" => {
                    ended = true;
                    ""
                }
                _ => return Err(format!("unknown probe section `{s}`")),
            };
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }
        match section {
            "SLOTS" => {
                let f: Vec<&str> = line.split_whitespace().collect();
                let (Some(k), Some(p), Some(u), None) = (f.first(), f.get(1), f.get(2), f.get(3))
                else {
                    return Err(format!("malformed slot-table row `{line}`"));
                };
                out.slots.push(SlotRow {
                    slot: k
                        .parse()
                        .map_err(|_| format!("slot `{k}` is not a number"))?,
                    port: p
                        .parse()
                        .map_err(|_| format!("slot port `{p}` is not a number"))?,
                    url: (*u).to_string(),
                });
            }
            "RENDER" => out.render.push(
                serde_json::from_str(line).map_err(|e| format!("render line is not JSON: {e}"))?,
            ),
            "APPLY" => {
                let v: Value = serde_json::from_str(line)
                    .map_err(|e| format!("apply list is not JSON: {e}"))?;
                let items = v
                    .get("items")
                    .and_then(Value::as_array)
                    .ok_or("apply list has no `items` array")?;
                out.apply.extend(items.iter().cloned());
            }
            _ => return Err(format!("probe output outside a section: `{line}`")),
        }
    }
    if !ended {
        return Err("probe output has no END marker (truncated)".to_string());
    }
    Ok(out)
}

fn kind_of(v: &Value) -> &str {
    v.get("kind").and_then(Value::as_str).unwrap_or("")
}

fn name_of(v: &Value) -> &str {
    v.pointer("/metadata/name")
        .and_then(Value::as_str)
        .unwrap_or("")
}

fn label_of<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.pointer("/metadata/labels")
        .and_then(|l| l.get(key))
        .and_then(Value::as_str)
}

/// Every pod template an object carries (Deployment-like, CronJob, bare Pod).
fn pod_templates(v: &Value) -> Vec<(&Value, Option<&Value>)> {
    let mut out = Vec::new();
    if kind_of(v) == "Pod" {
        if let Some(spec) = v.get("spec") {
            out.push((spec, v.pointer("/metadata/labels")));
        }
    }
    for base in ["/spec/template", "/spec/jobTemplate/spec/template"] {
        if let Some(spec) = v.pointer(&format!("{base}/spec")) {
            out.push((spec, v.pointer(&format!("{base}/metadata/labels"))));
        }
    }
    out
}

fn image_repo(image: &str) -> &str {
    let tail_start = image.rfind('/').map_or(0, |i| i + 1);
    match image.get(tail_start..).and_then(|t| t.find([':', '@'])) {
        Some(i) => image.get(..tail_start + i).unwrap_or(image),
        None => image,
    }
}

fn container_images(spec: &Value) -> Vec<&str> {
    ["containers", "initContainers"]
        .iter()
        .filter_map(|f| spec.get(*f).and_then(Value::as_array))
        .flatten()
        .filter_map(|c| c.get("image").and_then(Value::as_str))
        .collect()
}

fn the_template<'a>(render: &'a [Value], template_name: &str) -> Result<&'a Value, String> {
    let t: Vec<&Value> = render
        .iter()
        .filter(|d| kind_of(d) == "Deployment" && name_of(d) == template_name)
        .collect();
    match t.as_slice() {
        [one] => Ok(one),
        _ => Err(format!(
            "the environment-root render must contain exactly one Deployment/{template_name} \
                (the MH template), found {}",
            t.len()
        )),
    }
}

/// Rule 3 over the probe output.
#[must_use]
pub fn check_apply_set(
    probe: &ProbeOutput,
    template_name: &str,
    input_label: &str,
) -> Vec<SlotFinding> {
    let mut out = Vec::new();
    let tpl = match the_template(&probe.render, template_name) {
        Ok(t) => t,
        Err(e) => return vec![finding(VACUOUS_RULE_ID, e)],
    };
    let selector: Vec<(&str, &str)> = tpl
        .pointer("/spec/selector/matchLabels")
        .and_then(Value::as_object)
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| Some((k.as_str(), v.as_str()?)))
                .collect()
        })
        .unwrap_or_default();
    let repos: BTreeSet<&str> = pod_templates(tpl)
        .iter()
        .flat_map(|(s, _)| container_images(s))
        .map(image_repo)
        .collect();
    if selector.is_empty() || repos.is_empty() {
        return vec![finding(
            VACUOUS_RULE_ID,
            format!(
                "Deployment/{template_name} has no selector labels or no image — the \
                MH-workload identity cannot be derived"
            ),
        )];
    }
    if probe.apply.is_empty() {
        out.push(finding(
            VACUOUS_RULE_ID,
            "deploy.sh:env_root_apply_list produced an empty apply set",
        ));
    }
    for item in &probe.apply {
        let what = format!("{}/{}", kind_of(item), name_of(item));
        if label_of(item, input_label) == Some("true") {
            out.push(finding(
                MH_IN_ROOT_APPLY_SET_RULE_ID,
                format!(
                    "{what} is an MH template input (label {input_label}=true) but is in \
                    the root apply set (deploy.sh:env_root_apply_list) — template inputs are \
                    rendered, never applied"
                ),
            ));
        }
        for (spec, labels) in pod_templates(item) {
            let by_label = labels.is_some_and(|l| {
                selector
                    .iter()
                    .all(|(k, v)| l.get(*k).and_then(Value::as_str) == Some(*v))
            });
            let by_image = container_images(spec)
                .iter()
                .any(|i| repos.contains(image_repo(i)));
            if by_label || by_image {
                out.push(finding(
                    MH_IN_ROOT_APPLY_SET_RULE_ID,
                    format!(
                        "{what} is an MH workload ({}) in the environment-root apply set \
                            (deploy.sh:env_root_apply_list); MH reaches the cluster only as \
                            ConfigMap mh-template, instantiated per hostPort slot by mh.sh",
                        if by_label {
                            "pod labels match the MH template's \
                            selector"
                        } else {
                            "runs the MH template's image"
                        }
                    ),
                ));
                break;
            }
        }
    }
    out
}

fn set_str(s: &BTreeSet<String>) -> String {
    format!("{{{}}}", s.iter().cloned().collect::<Vec<_>>().join(","))
}

fn split_set(v: &str) -> BTreeSet<String> {
    v.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// `https://host:port` -> (host, port).
fn url_host_port(url: &str) -> Option<(String, u64)> {
    let rest = url.strip_prefix("https://")?;
    let (host, port) = rest.rsplit_once(':')?;
    if host.is_empty() {
        return None;
    }
    Some((host.to_string(), port.parse().ok()?))
}

/// The one `podSubnet:` value of a kind-config (text, so a template with
/// `${...}` placeholders reads too).
fn pod_subnet(text: &str) -> Result<String, String> {
    let vals: Vec<String> = text
        .lines()
        .map(str::trim_start)
        .filter_map(|l| l.strip_prefix("podSubnet:"))
        .map(|v| {
            v.split('#')
                .next()
                .unwrap_or("")
                .trim()
                .trim_matches('"')
                .to_string()
        })
        .collect();
    match vals.as_slice() {
        [v] if !v.is_empty() => Ok(v.clone()),
        _ => Err(format!(
            "expected exactly one non-empty networking.podSubnet, found {}",
            vals.len()
        )),
    }
}

/// Slot ids the marker lines of `text` name.
fn marker_slots(text: &str, marker: &str) -> Vec<String> {
    text.lines()
        .map(str::trim_start)
        .filter_map(|l| l.strip_prefix(marker))
        .map(|k| k.trim().to_string())
        .collect()
}

/// `(containerPort, hostPort, protocol)` of every extraPortMapping.
fn port_mappings(kind_config: &str) -> Result<Vec<(u64, u64, String)>, String> {
    let doc: serde_norway::Value =
        serde_norway::from_str(kind_config).map_err(|e| format!("does not parse as YAML: {e}"))?;
    let mut out = Vec::new();
    for node in doc
        .get("nodes")
        .and_then(|n| n.as_sequence())
        .map(Vec::as_slice)
        .unwrap_or_default()
    {
        for m in node
            .get("extraPortMappings")
            .and_then(|n| n.as_sequence())
            .map(Vec::as_slice)
            .unwrap_or_default()
        {
            out.push((
                m.get("containerPort")
                    .and_then(serde_norway::Value::as_u64)
                    .unwrap_or(0),
                m.get("hostPort")
                    .and_then(serde_norway::Value::as_u64)
                    .unwrap_or(0),
                m.get("protocol")
                    .and_then(serde_norway::Value::as_str)
                    .unwrap_or("TCP")
                    .to_string(),
            ));
        }
    }
    Ok(out)
}

fn drift(
    out: &mut Vec<SlotFinding>,
    what: &str,
    a_name: &str,
    a: &BTreeSet<String>,
    b_name: &str,
    b: &BTreeSet<String>,
) {
    if a != b {
        out.push(finding(
            SLOT_TABLE_DRIFT_RULE_ID,
            format!(
                "{what}: {a_name} = {} but {b_name} = {}",
                set_str(a),
                set_str(b)
            ),
        ));
    }
}

fn nonempty(out: &mut Vec<SlotFinding>, name: &str, s: &BTreeSet<String>) -> bool {
    if s.is_empty() {
        out.push(finding(
            VACUOUS_RULE_ID,
            format!("{name} is empty — nothing to compare, refusing a vacuous pass"),
        ));
        return false;
    }
    true
}

/// Push one drift finding (`rule`) naming both sides.
fn drift_finding(out: &mut Vec<SlotFinding>, detail: String) {
    out.push(finding(SLOT_TABLE_DRIFT_RULE_ID, detail));
}

/// `data.<key>` of a ConfigMap, or "".
fn cm_data<'a>(cm: &'a Value, key: &str) -> &'a str {
    cm.pointer(&format!("/data/{key}"))
        .and_then(Value::as_str)
        .unwrap_or("")
}

/// The slot ids K (from ports.rs) names, and the marker; `None` = unreadable.
fn slot_table_home(ports_rs: &str) -> Option<(BTreeSet<String>, String)> {
    let k = rust_uint_consts(ports_rs).get(SLOT_COUNT_CONST).copied()?;
    let marker = rust_str_consts(ports_rs).get(SLOT_MARKER_CONST).cloned()?;
    Some(((0..k).map(|i| i.to_string()).collect(), marker))
}

/// K vs the static kind-config markers and deploy.sh's slot table; each slot
/// port a UDP extraPortMapping with containerPort == hostPort.
fn static_kind_config_drift(
    out: &mut Vec<SlotFinding>,
    tree: &TreeInputs,
    probe: &ProbeOutput,
    want_ids: &BTreeSet<String>,
    marker: &str,
) {
    let k_side = format!("K from {PORTS_RS}");
    let static_ids = marker_slots(&tree.static_kind_config, marker);
    let static_set: BTreeSet<String> = static_ids.iter().cloned().collect();
    if static_ids.len() != static_set.len() {
        drift_finding(
            out,
            format!("{STATIC_KIND_CONFIG}: duplicate slot marker among {static_ids:?}"),
        );
    }
    let markers = format!("{STATIC_KIND_CONFIG} `{marker}<k>` markers");
    drift(out, "slot ids", &k_side, want_ids, &markers, &static_set);

    let table_ids: BTreeSet<String> = probe.slots.iter().map(|r| r.slot.to_string()).collect();
    let table_ports = slot_ports(probe);
    if nonempty(out, SLOT_TABLE, &table_ids) {
        drift(out, "slot ids", &k_side, want_ids, SLOT_TABLE, &table_ids);
        if table_ports.len() != probe.slots.len() {
            drift_finding(out, format!("{SLOT_TABLE} has a duplicate slot port"));
        }
    }
    match port_mappings(&tree.static_kind_config) {
        Ok(maps) => {
            let udp_same: BTreeSet<String> = maps
                .iter()
                .filter(|(c, h, p)| c == h && p == "UDP" && table_ports.contains(&h.to_string()))
                .map(|(_, h, _)| h.to_string())
                .collect();
            let side =
                format!("{STATIC_KIND_CONFIG} UDP extraPortMappings (containerPort == hostPort)");
            drift(
                out,
                "slot host ports",
                SLOT_TABLE,
                &table_ports,
                &side,
                &udp_same,
            );
        }
        Err(e) => out.push(finding(
            UNVERIFIABLE_RULE_ID,
            format!("{STATIC_KIND_CONFIG}: {e}"),
        )),
    }
}

/// The tmpl carries the slot block only as the ONE placeholder ports.rs fills.
fn tmpl_drift(out: &mut Vec<SlotFinding>, tree: &TreeInputs, marker: &str) {
    match PLACEHOLDER_VAR_RE
        .captures(&tree.ports_rs)
        .and_then(|c| c.get(1))
    {
        None => out.push(finding(
            UNVERIFIABLE_RULE_ID,
            format!(
                "{PORTS_RS}: cannot find the template variable template_env_vars binds to \
                    mh_slot_port_mappings"
            ),
        )),
        Some(var) => {
            let ph = format!("${{{}}}", var.as_str());
            let n = tree
                .kind_config_tmpl
                .lines()
                .filter(|l| l.trim() == ph)
                .count();
            if n != 1 {
                drift_finding(
                    out,
                    format!(
                        "{KIND_CONFIG_TMPL}: the MH slot block must be exactly one `{ph}` line \
                            (rendered from {PORTS_RS}:mh_slot_port_mappings), found {n}"
                    ),
                );
            }
        }
    }
    let tmpl_markers = marker_slots(&tree.kind_config_tmpl, marker);
    if !tmpl_markers.is_empty() {
        drift_finding(
            out,
            format!(
                "{KIND_CONFIG_TMPL}: hand-written slot marker(s) {tmpl_markers:?} — the slot \
                    block is rendered from {PORTS_RS}, K has one home"
            ),
        );
    }
}

/// One slot ConfigMap per slot, applied, carrying the slot table's port and address.
fn slot_configmap_drift(
    out: &mut Vec<SlotFinding>,
    probe: &ProbeOutput,
    want_ids: &BTreeSet<String>,
    slot_label: &str,
) {
    let slot_cms: Vec<&Value> = probe
        .render
        .iter()
        .filter(|d| kind_of(d) == "ConfigMap" && label_of(d, slot_label).is_some())
        .collect();
    let cm_ids: BTreeSet<String> = slot_cms
        .iter()
        .filter_map(|d| label_of(d, slot_label))
        .map(str::to_string)
        .collect();
    let cm_side = format!("slot ConfigMaps (label {slot_label})");
    if nonempty(out, &cm_side, &cm_ids) {
        if cm_ids.len() != slot_cms.len() {
            drift_finding(
                out,
                format!("two slot ConfigMaps carry the same {slot_label} value"),
            );
        }
        drift(
            out,
            "slot ids",
            &format!("K from {PORTS_RS}"),
            want_ids,
            &cm_side,
            &cm_ids,
        );
        let applied: BTreeSet<String> = probe
            .apply
            .iter()
            .filter(|d| kind_of(d) == "ConfigMap")
            .filter_map(|d| label_of(d, slot_label))
            .map(str::to_string)
            .collect();
        drift(
            out,
            "slot ConfigMaps applied",
            "rendered",
            &cm_ids,
            APPLY_SET,
            &applied,
        );
    }
    for row in &probe.slots {
        let k = row.slot.to_string();
        let Some(cm) = slot_cms
            .iter()
            .find(|d| label_of(d, slot_label) == Some(k.as_str()))
        else {
            continue; // reported by the id drift above
        };
        let (host_port, addr, cm_name) = (
            cm_data(cm, SLOT_HOST_PORT_KEY),
            cm_data(cm, SLOT_ADDRESS_KEY),
            name_of(cm),
        );
        if host_port != row.port.to_string() {
            drift_finding(
                out,
                format!(
                    "slot {k}: {SLOT_TABLE} port = {} but {cm_name}'s {SLOT_HOST_PORT_KEY} \
                    = '{host_port}'",
                    row.port
                ),
            );
        }
        if addr != row.url {
            drift_finding(
                out,
                format!(
                    "slot {k}: {SLOT_TABLE} address = {} but {cm_name}'s {SLOT_ADDRESS_KEY} \
                    = '{addr}'",
                    row.url
                ),
            );
        }
        if url_host_port(addr).map(|(_, p)| p) != Some(row.port) {
            drift_finding(
                out,
                format!(
                    "slot {k}: advertise address '{addr}' does not end in the slot's host port {}",
                    row.port
                ),
            );
        }
    }
}

fn slot_ports(probe: &ProbeOutput) -> BTreeSet<String> {
    probe.slots.iter().map(|r| r.port.to_string()).collect()
}

/// The ONE rendered ConfigMap carrying GC's allowlist keys (found by its keys).
fn gc_configmap<'a>(out: &mut Vec<SlotFinding>, render: &'a [Value]) -> Option<&'a Value> {
    let gc: Vec<&Value> = render
        .iter()
        .filter(|d| {
            kind_of(d) == "ConfigMap"
                && GC_KEYS
                    .iter()
                    .any(|k| d.pointer(&format!("/data/{k}")).is_some())
        })
        .collect();
    match gc.as_slice() {
        [one] => Some(*one),
        [] => {
            out.push(finding(
                VACUOUS_RULE_ID,
                format!(
                    "no rendered ConfigMap carries GC's MH allowlists ({}) — GC receives none, \
                        nothing to compare",
                    GC_KEYS.join(", ")
                ),
            ));
            None
        }
        many => {
            let names: Vec<&str> = many.iter().map(|d| name_of(d)).collect();
            drift_finding(
                out,
                format!(
                    "{} rendered ConfigMaps carry GC's MH allowlists ({names:?}) — which one GC \
                        reads is ambiguous; exactly one must",
                    many.len()
                ),
            );
            None
        }
    }
}

/// GC loads each allowlist key through its own env entry, so the ConfigMap's
/// values are what GC runs with only if every workload naming a GC key takes
/// it as a hard `configMapKeyRef` to THAT ConfigMap and key. Workloads are
/// found by the env names, not by a hard-coded name.
fn gc_env_ref_drift(out: &mut Vec<SlotFinding>, render: &[Value], cm_name: &str) {
    let mut seen_workload = false;
    for doc in render {
        for (spec, _) in pod_templates(doc) {
            for c in spec
                .get("containers")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let env: Vec<&Value> = c
                    .get("env")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .collect();
                let named = |k: &str| -> Vec<&Value> {
                    env.iter()
                        .copied()
                        .filter(|e| e.get("name").and_then(Value::as_str) == Some(k))
                        .collect()
                };
                if GC_KEYS.iter().all(|k| named(k).is_empty()) {
                    continue;
                }
                seen_workload = true;
                let who = format!(
                    "{}/{} container {}",
                    kind_of(doc),
                    name_of(doc),
                    c.get("name").and_then(Value::as_str).unwrap_or("?")
                );
                for key in GC_KEYS {
                    let entries = named(key);
                    let [e] = entries.as_slice() else {
                        drift_finding(
                            out,
                            format!(
                                "{who}: expected exactly one env entry {key} \
                            (a configMapKeyRef to {cm_name}), found {}",
                                entries.len()
                            ),
                        );
                        continue;
                    };
                    let r = e.pointer("/valueFrom/configMapKeyRef");
                    let ok = r.is_some_and(|r| {
                        r.get("name").and_then(Value::as_str) == Some(cm_name)
                            && r.get("key").and_then(Value::as_str) == Some(*key)
                            && r.get("optional").and_then(Value::as_bool) != Some(true)
                    });
                    if !ok {
                        drift_finding(
                            out,
                            format!(
                                "{who}: env {key} must be a hard configMapKeyRef {{name: \
                                    {cm_name}, key: {key}}} (the checked allowlist), got {}",
                                e.get("value").map_or_else(
                                    || e.get("valueFrom")
                                        .map_or("nothing".to_string(), Value::to_string),
                                    |v| format!("literal value {v}")
                                )
                            ),
                        );
                    }
                }
            }
        }
    }
    if !seen_workload {
        out.push(finding(
            VACUOUS_RULE_ID,
            format!(
                "no rendered workload takes GC's MH allowlist keys ({}) as env — the \
                checked ConfigMap reaches no GC process",
                GC_KEYS.join(", ")
            ),
        ));
    }
}

/// GC's allowlists as GC receives them vs the slot table and the kind-configs.
fn gc_allowlist_drift(
    out: &mut Vec<SlotFinding>,
    tree: &TreeInputs,
    probe: &ProbeOutput,
    gc: &Value,
) {
    let gc_name = name_of(gc);
    let gc_set = |key: &str| split_set(cm_data(gc, key));
    let slot_hosts: BTreeSet<String> = probe
        .slots
        .iter()
        .filter_map(|r| url_host_port(&r.url))
        .map(|(h, _)| h)
        .collect();
    let table_ports = slot_ports(probe);
    for (key, side_name, side) in [
        (
            GC_WT_PORTS,
            format!("slot host ports ({SLOT_TABLE})"),
            &table_ports,
        ),
        (
            GC_WT_HOSTS,
            format!("slot hosts ({SLOT_TABLE})"),
            &slot_hosts,
        ),
    ] {
        let g = gc_set(key);
        let g_name = format!("{gc_name} {key}");
        if nonempty(out, &g_name, &g) && nonempty(out, &side_name, side) {
            drift(out, key, &g_name, &g, &side_name, side);
        }
    }
    let cidrs = gc_set(GC_GRPC_CIDRS);
    let c_name = format!("{gc_name} {GC_GRPC_CIDRS}");
    if nonempty(out, &c_name, &cidrs) {
        for (file, text) in [
            (STATIC_KIND_CONFIG, &tree.static_kind_config),
            (KIND_CONFIG_TMPL, &tree.kind_config_tmpl),
        ] {
            match pod_subnet(text) {
                Ok(s) => drift(
                    out,
                    GC_GRPC_CIDRS,
                    &c_name,
                    &cidrs,
                    &format!("{file} networking.podSubnet"),
                    &BTreeSet::from([s]),
                ),
                Err(e) => out.push(finding(UNVERIFIABLE_RULE_ID, format!("{file}: {e}"))),
            }
        }
    }
}

/// GC's gRPC port allowlist vs the port the template advertises, itself a
/// declared TCP containerPort.
fn grpc_port_drift(
    out: &mut Vec<SlotFinding>,
    probe: &ProbeOutput,
    gc: &Value,
    template_name: &str,
) {
    let g_name = format!("{} {GC_GRPC_PORTS}", name_of(gc));
    let grpc = split_set(cm_data(gc, GC_GRPC_PORTS));
    if !nonempty(out, &g_name, &grpc) {
        return;
    }
    let tpl = match the_template(&probe.render, template_name) {
        Ok(t) => t,
        Err(e) => return out.push(finding(VACUOUS_RULE_ID, e)),
    };
    let containers: Vec<&Value> = tpl
        .pointer("/spec/template/spec/containers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .collect();
    let adv: Vec<&str> = containers
        .iter()
        .filter_map(|c| c.get("env").and_then(Value::as_array))
        .flatten()
        .filter(|e| e.get("name").and_then(Value::as_str) == Some(GRPC_ADVERTISE_ENV))
        .filter_map(|e| e.get("value").and_then(Value::as_str))
        .collect();
    let port: Option<u64> = match adv.as_slice() {
        [a] => GRPC_ADVERTISE_RE
            .captures(a)
            .and_then(|c| c.get(1))
            .and_then(|m| m.as_str().parse().ok()),
        _ => None,
    };
    let Some(p) = port else {
        return drift_finding(
            out,
            format!(
                "Deployment/{template_name} must carry exactly one {GRPC_ADVERTISE_ENV} of the \
                    form <scheme>://$(POD_IP):<port>, got {adv:?}"
            ),
        );
    };
    let side = format!("Deployment/{template_name} {GRPC_ADVERTISE_ENV} port");
    drift(
        out,
        GC_GRPC_PORTS,
        &g_name,
        &grpc,
        &side,
        &BTreeSet::from([p.to_string()]),
    );
    let declared = containers
        .iter()
        .filter_map(|c| c.get("ports").and_then(Value::as_array))
        .flatten()
        .any(|cp| {
            cp.get("containerPort").and_then(Value::as_u64) == Some(p)
                && cp.get("protocol").and_then(Value::as_str).unwrap_or("TCP") == "TCP"
        });
    if !declared {
        drift_finding(
            out,
            format!(
                "Deployment/{template_name} advertises gRPC on port {p} \
                ({GRPC_ADVERTISE_ENV}) but declares no TCP containerPort {p}"
            ),
        );
    }
}

/// Rule 4.
#[must_use]
pub fn check_slot_drift(
    tree: &TreeInputs,
    probe: &ProbeOutput,
    template_name: &str,
    slot_label: &str,
) -> Vec<SlotFinding> {
    let mut out = Vec::new();
    let Some((want_ids, marker)) = slot_table_home(&tree.ports_rs) else {
        return vec![finding(
            UNVERIFIABLE_RULE_ID,
            format!(
                "{PORTS_RS}: cannot read `{SLOT_COUNT_CONST}` (K, a literal unsigned const) \
                and `{SLOT_MARKER_CONST}`"
            ),
        )];
    };
    if !nonempty(
        &mut out,
        &format!("K ({PORTS_RS} {SLOT_COUNT_CONST})"),
        &want_ids,
    ) {
        return out;
    }
    static_kind_config_drift(&mut out, tree, probe, &want_ids, &marker);
    tmpl_drift(&mut out, tree, &marker);
    slot_configmap_drift(&mut out, probe, &want_ids, slot_label);
    if let Some(gc) = gc_configmap(&mut out, &probe.render) {
        gc_env_ref_drift(&mut out, &probe.render, name_of(gc));
        gc_allowlist_drift(&mut out, tree, probe, gc);
        grpc_port_drift(&mut out, probe, gc, template_name);
    }
    out
}

/// Every inherited `DT_*` variable: the probe must see none of them (deploy.sh
/// and lib/common.sh read many — cluster, port map, image cache, job and
/// settle knobs — and the next one added is covered without editing a list).
fn probe_env_removals(vars: impl Iterator<Item = std::ffi::OsString>) -> Vec<std::ffi::OsString> {
    vars.filter(|k| k.to_string_lossy().starts_with("DT_"))
        .collect()
}

/// Run the probe against the tree at `repo_root`.
///
/// # Errors
/// The probe could not run or failed; the message carries its stderr head
/// (deploy.sh's `log_error` lines; it never prints rendered content there).
pub fn run_probe(repo_root: &Path) -> Result<ProbeOutput, String> {
    let deploy = repo_root.join(DEPLOY_SH);
    let mut cmd = Command::new("bash");
    cmd.arg("-c")
        .arg(PROBE)
        .arg("dt-guard-mh-slot-probe")
        .arg(&deploy)
        .current_dir(repo_root);
    for k in probe_env_removals(std::env::vars_os().map(|(k, _)| k)) {
        cmd.env_remove(k);
    }
    let out = cmd.output().map_err(|e| format!("cannot run bash: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let head: Vec<&str> = err
            .lines()
            .rev()
            .take(6)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        return Err(format!(
            "the deploy.sh probe (cache_renders, render_env_overlay, env_root_apply_list, \
                mh_slot_table over {STATIC_KIND_CONFIG}) failed: {}",
            head.join(" | ")
        ));
    }
    parse_probe(&String::from_utf8_lossy(&out.stdout))
}

fn read_tree(repo_root: &Path) -> Result<TreeInputs, String> {
    let read = |rel: &str| {
        std::fs::read_to_string(repo_root.join(rel))
            .map_err(|e| format!("{rel}: cannot read ({e})"))
    };
    Ok(TreeInputs {
        ports_rs: read(PORTS_RS)?,
        static_kind_config: read(STATIC_KIND_CONFIG)?,
        kind_config_tmpl: read(KIND_CONFIG_TMPL)?,
        deploy_sh: read(DEPLOY_SH)?,
    })
}

fn token(findings: &[SlotFinding]) -> String {
    for (id, tok) in [
        (UNVERIFIABLE_RULE_ID, "mh-slot-model-unverifiable"),
        (VACUOUS_RULE_ID, "mh-slot-model-vacuous"),
        (MH_IN_ROOT_APPLY_SET_RULE_ID, "mh-in-root-apply-set"),
        (SLOT_TABLE_DRIFT_RULE_ID, "mh-slot-table-drift"),
    ] {
        let n = findings.iter().filter(|f| f.rule_id == id).count();
        if n > 0 {
            return format!("{tok}-{n}");
        }
    }
    "mh-slot-model-violations".to_string()
}

/// All rules over the tree and the probe.
#[must_use]
pub fn check(tree: &TreeInputs, probe: &ProbeOutput) -> Vec<SlotFinding> {
    let consts = [
        TEMPLATE_NAME_CONST,
        TEMPLATE_INPUT_LABEL_CONST,
        SLOT_LABEL_CONST,
    ]
    .map(|c| shell_const(&tree.deploy_sh, c).map_err(|e| format!("{DEPLOY_SH}: {e}")));
    let [Ok(template_name), Ok(input_label), Ok(slot_label)] = consts else {
        return consts
            .into_iter()
            .filter_map(Result::err)
            .map(|e| finding(UNVERIFIABLE_RULE_ID, e))
            .collect();
    };
    let mut out = check_apply_set(probe, &template_name, &input_label);
    out.extend(check_slot_drift(tree, probe, &template_name, &slot_label));
    out
}

/// Run the `mh-slot-model` policy.
pub fn run(repo_root: &Path, explain: bool) -> Result<()> {
    let findings = match (read_tree(repo_root), run_probe(repo_root)) {
        (Ok(tree), Ok(probe)) => {
            emit_scope(format!(
                "{} slot-table rows, {} rendered docs, {} applied docs (deploy.sh over \
                    {STATIC_KIND_CONFIG})",
                probe.slots.len(),
                probe.render.len(),
                probe.apply.len()
            ));
            check(&tree, &probe)
        }
        (t, p) => [t.err(), p.err()]
            .into_iter()
            .flatten()
            .map(|e| finding(UNVERIFIABLE_RULE_ID, e))
            .collect(),
    };
    if findings.is_empty() {
        emit_ok("mh-slot-model-clean");
        return Ok(());
    }
    for f in &findings {
        if f.rule_id == UNVERIFIABLE_RULE_ID || f.rule_id == VACUOUS_RULE_ID {
            eprintln!("ERROR: PRECONDITION [{}] {}", f.rule_id, f.detail);
        }
        if explain {
            let policy = format!("mh_slot_model::{}", f.rule_id);
            print_finding(&Finding {
                file: DEPLOY_SH,
                row: 0,
                col: 0,
                policy: &policy,
                matched: &f.detail,
                extras: &[],
                src_file: file!(),
                src_line: line!(),
            });
        } else {
            println!("VIOLATION: [{}] {}", f.rule_id, f.detail);
        }
    }
    anyhow::bail!("{}", token(&findings))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const DEPLOY: &str = "MH_TEMPLATE_DEPLOYMENT=\"mh-template\"\nMH_TEMPLATE_INPUT_LABEL=\"dark-tower.io/mh-template-input\"\nMH_SLOT_LABEL=\"dark-tower.io/mh-slot\"\n";
    const PORTS: &str = "impl PortOffsets {\n    pub const MH_SLOT_COUNT: u16 = 2;\n}\npub const MH_SLOT_MARKER: &str = \"# dt:mh-slot=\";\nfn v() {\n    vars.insert(\n        \"MH_SLOT_PORT_MAPPINGS\".to_string(),\n        mh_slot_port_mappings(alloc, ip),\n    );\n}\n";
    const STATIC: &str = "kind: Cluster\nnodes:\n  - role: control-plane\n    extraPortMappings:\n      - containerPort: 30433\n        hostPort: 4433\n        protocol: UDP\n      # dt:mh-slot=0\n      - containerPort: 4440\n        hostPort: 4440\n        listenAddress: \"127.0.0.1\"\n        protocol: UDP\n      # dt:mh-slot=1\n      - containerPort: 4441\n        hostPort: 4441\n        listenAddress: \"127.0.0.1\"\n        protocol: UDP\nnetworking:\n  podSubnet: \"192.168.0.0/16\"\n";
    const TMPL: &str = "nodes:\n  - role: control-plane\n    extraPortMappings:\n${MH_SLOT_PORT_MAPPINGS}\nnetworking:\n  podSubnet: \"192.168.0.0/16\"\n";

    fn tree() -> TreeInputs {
        TreeInputs {
            ports_rs: PORTS.to_string(),
            static_kind_config: STATIC.to_string(),
            kind_config_tmpl: TMPL.to_string(),
            deploy_sh: DEPLOY.to_string(),
        }
    }

    fn template() -> Value {
        json!({"apiVersion": "apps/v1", "kind": "Deployment", "metadata": {"name": "mh-template", "labels": {"app": "mh-service"}},
            "spec": {"selector": {"matchLabels": {"app": "mh-service", "component": "media"}},
                "template": {"metadata": {"labels": {"app": "mh-service", "component": "media"}},
                    "spec": {"containers": [{"name": "mh-service", "image": "localhost/mh-service:sha-0000",
                        "ports": [{"name": "grpc", "containerPort": 50053, "protocol": "TCP"}],
                        "env": [{"name": "MH_GRPC_ADVERTISE_ADDRESS", "value": "grpc://$(POD_IP):50053"}]}]}}}})
    }

    fn slot_cm(k: u64, port: u64) -> Value {
        json!({"apiVersion": "v1", "kind": "ConfigMap", "metadata": {"name": format!("mh-slot-{k}-config-h"), "labels": {"app": "mh-service", "dark-tower.io/mh-slot": k.to_string()}},
            "data": {"MH_HOST_PORT": port.to_string(), "MH_WEBTRANSPORT_ADVERTISE_ADDRESS": format!("https://127.0.0.1:{port}")}})
    }

    fn gc_cm() -> Value {
        json!({"apiVersion": "v1", "kind": "ConfigMap", "metadata": {"name": "gc-service-config-h"},
            "data": {"GC_MH_WT_ALLOWED_HOSTS": "127.0.0.1", "GC_MH_WT_ALLOWED_PORTS": "4440,4441",
                "GC_MH_GRPC_ALLOWED_CIDRS": "192.168.0.0/16", "GC_MH_GRPC_ALLOWED_PORTS": "50053", "GC_MH_GRPC_ALLOWED_SCHEMES": "grpc"}})
    }

    fn input_cm() -> Value {
        json!({"apiVersion": "v1", "kind": "ConfigMap", "metadata": {"name": "mh-slot-template-config-h", "labels": {"dark-tower.io/mh-template-input": "true"}}, "data": {}})
    }

    fn gc_deployment() -> Value {
        json!({"apiVersion": "apps/v1", "kind": "Deployment", "metadata": {"name": "gc-service"},
            "spec": {"template": {"metadata": {"labels": {"app": "gc-service"}}, "spec": {"containers": [{"name": "gc", "image": "localhost/gc-service:sha-0000",
                "env": GC_KEYS.iter().map(|k| json!({"name": k, "valueFrom": {"configMapKeyRef": {"name": "gc-service-config-h", "key": k}}})).collect::<Vec<_>>()}]}}}})
    }

    fn probe() -> ProbeOutput {
        let render = vec![
            template(),
            input_cm(),
            slot_cm(0, 4440),
            slot_cm(1, 4441),
            gc_cm(),
            gc_deployment(),
        ];
        let apply = vec![slot_cm(0, 4440), slot_cm(1, 4441), gc_cm(), gc_deployment()];
        ProbeOutput {
            slots: vec![
                SlotRow {
                    slot: 0,
                    port: 4440,
                    url: "https://127.0.0.1:4440".to_string(),
                },
                SlotRow {
                    slot: 1,
                    port: 4441,
                    url: "https://127.0.0.1:4441".to_string(),
                },
            ],
            render,
            apply,
        }
    }

    fn assert_finding(f: &[SlotFinding], rule: &str, needle: &str) {
        assert!(
            f.iter()
                .any(|x| x.rule_id == rule && x.detail.contains(needle)),
            "expected {rule} naming `{needle}`, got {f:#?}"
        );
    }

    #[test]
    fn consistent_fixture_passes() {
        assert_eq!(check(&tree(), &probe()), vec![]);
    }

    // ---- rule 3 -------------------------------------------------------------

    #[test]
    fn mh_deployment_in_root_apply_set_fails() {
        let mut p = probe();
        p.apply.push(template());
        assert_finding(
            &check(&tree(), &p),
            MH_IN_ROOT_APPLY_SET_RULE_ID,
            "Deployment/mh-template is an MH workload",
        );
    }

    #[test]
    fn renamed_mh_workload_is_caught_by_labels_or_image() {
        let mut p = probe();
        let mut renamed = template();
        renamed["metadata"]["name"] = json!("mh-0");
        p.apply.push(renamed);
        assert_finding(
            &check(&tree(), &p),
            MH_IN_ROOT_APPLY_SET_RULE_ID,
            "Deployment/mh-0",
        );
        let mut p = probe();
        let mut by_image = template();
        by_image["kind"] = json!("StatefulSet");
        by_image["metadata"]["name"] = json!("other");
        by_image["spec"]["template"]["metadata"]["labels"] = json!({"app": "other"});
        p.apply.push(by_image);
        assert_finding(
            &check(&tree(), &p),
            MH_IN_ROOT_APPLY_SET_RULE_ID,
            "runs the MH template's image",
        );
    }

    #[test]
    fn template_input_in_root_apply_set_fails() {
        let mut p = probe();
        p.apply.push(input_cm());
        assert_finding(
            &check(&tree(), &p),
            MH_IN_ROOT_APPLY_SET_RULE_ID,
            "MH template input",
        );
    }

    #[test]
    fn missing_template_or_empty_apply_set_is_vacuous() {
        let mut p = probe();
        p.render.retain(|d| name_of(d) != "mh-template");
        assert_finding(
            &check(&tree(), &p),
            VACUOUS_RULE_ID,
            "exactly one Deployment/mh-template",
        );
        let mut p = probe();
        p.apply.clear();
        assert_finding(&check(&tree(), &p), VACUOUS_RULE_ID, "empty apply set");
    }

    // ---- rule 4: each drift pair ---------------------------------------------

    #[test]
    fn k_vs_static_markers_and_slot_table() {
        let t = TreeInputs {
            ports_rs: PORTS.replace("= 2;", "= 3;"),
            ..tree()
        };
        let f = check(&t, &probe());
        assert_finding(&f, SLOT_TABLE_DRIFT_RULE_ID, "markers = {0,1}");
        assert_finding(
            &f,
            SLOT_TABLE_DRIFT_RULE_ID,
            "deploy.sh:mh_slot_table = {0,1}",
        );
        assert_finding(
            &f,
            SLOT_TABLE_DRIFT_RULE_ID,
            "slot ConfigMaps (label dark-tower.io/mh-slot) = {0,1}",
        );
    }

    #[test]
    fn slot_port_not_a_udp_same_port_mapping() {
        let t = TreeInputs { static_kind_config: STATIC.replace("containerPort: 4441\n        hostPort: 4441\n        listenAddress: \"127.0.0.1\"\n        protocol: UDP", "containerPort: 4441\n        hostPort: 4441\n        listenAddress: \"127.0.0.1\"\n        protocol: TCP"), ..tree() };
        assert_finding(
            &check(&t, &probe()),
            SLOT_TABLE_DRIFT_RULE_ID,
            "UDP extraPortMappings",
        );
    }

    #[test]
    fn tmpl_placeholder_and_literal_marker() {
        let t = TreeInputs {
            kind_config_tmpl: TMPL.replace("${MH_SLOT_PORT_MAPPINGS}\n", ""),
            ..tree()
        };
        assert_finding(
            &check(&t, &probe()),
            SLOT_TABLE_DRIFT_RULE_ID,
            "exactly one `${MH_SLOT_PORT_MAPPINGS}` line",
        );
        let t = TreeInputs {
            kind_config_tmpl: TMPL.replace(
                "${MH_SLOT_PORT_MAPPINGS}\n",
                "${MH_SLOT_PORT_MAPPINGS}\n      # dt:mh-slot=9\n      - containerPort: 4449\n",
            ),
            ..tree()
        };
        assert_finding(
            &check(&t, &probe()),
            SLOT_TABLE_DRIFT_RULE_ID,
            "hand-written slot marker",
        );
    }

    #[test]
    fn slot_configmap_port_and_address() {
        let mut p = probe();
        p.render[3] = slot_cm(1, 4442);
        p.apply[1] = slot_cm(1, 4442);
        let f = check(&tree(), &p);
        assert_finding(&f, SLOT_TABLE_DRIFT_RULE_ID, "slot 1: deploy.sh:mh_slot_table port = 4441 but mh-slot-1-config-h's MH_HOST_PORT = '4442'");
        assert_finding(
            &f,
            SLOT_TABLE_DRIFT_RULE_ID,
            "slot 1: deploy.sh:mh_slot_table address = https://127.0.0.1:4441",
        );
    }

    #[test]
    fn slot_configmap_not_applied() {
        let mut p = probe();
        p.apply.remove(1);
        assert_finding(
            &check(&tree(), &p),
            SLOT_TABLE_DRIFT_RULE_ID,
            "slot ConfigMaps applied",
        );
    }

    fn with_gc(key: &str, val: &str) -> ProbeOutput {
        let mut p = probe();
        p.render[4]["data"][key] = json!(val);
        p
    }

    #[test]
    fn gc_allowlist_pairs() {
        let f = check(&tree(), &with_gc(GC_WT_PORTS, "4440,4441,4442"));
        assert_finding(
            &f,
            SLOT_TABLE_DRIFT_RULE_ID,
            "GC_MH_WT_ALLOWED_PORTS = {4440,4441,4442} but slot host ports",
        );
        let f = check(&tree(), &with_gc(GC_WT_HOSTS, "10.0.0.1"));
        assert_finding(
            &f,
            SLOT_TABLE_DRIFT_RULE_ID,
            "GC_MH_WT_ALLOWED_HOSTS = {10.0.0.1} but slot hosts",
        );
        let f = check(&tree(), &with_gc(GC_GRPC_CIDRS, "10.244.0.0/16"));
        assert_finding(
            &f,
            SLOT_TABLE_DRIFT_RULE_ID,
            "infra/kind/kind-config.yaml networking.podSubnet",
        );
        assert_finding(
            &f,
            SLOT_TABLE_DRIFT_RULE_ID,
            "infra/kind/kind-config.yaml.tmpl networking.podSubnet",
        );
        let f = check(&tree(), &with_gc(GC_GRPC_PORTS, "50054"));
        assert_finding(
            &f,
            SLOT_TABLE_DRIFT_RULE_ID,
            "MH_GRPC_ADVERTISE_ADDRESS port",
        );
    }

    fn with_gc_env(key: &str, entry: Value) -> ProbeOutput {
        let mut p = probe();
        let env = p.render[5]["spec"]["template"]["spec"]["containers"][0]["env"]
            .as_array_mut()
            .unwrap();
        env.retain(|e| e["name"] != key);
        if !entry.is_null() {
            env.push(entry);
        }
        p
    }

    #[test]
    fn gc_env_must_reference_the_checked_configmap() {
        let lit = with_gc_env(
            GC_WT_PORTS,
            json!({"name": GC_WT_PORTS, "value": "4440,4441"}),
        );
        assert_finding(
            &check(&tree(), &lit),
            SLOT_TABLE_DRIFT_RULE_ID,
            "literal value",
        );
        let other = with_gc_env(
            GC_WT_PORTS,
            json!({"name": GC_WT_PORTS, "valueFrom": {"configMapKeyRef": {"name": "other-config", "key": GC_WT_PORTS}}}),
        );
        assert_finding(
            &check(&tree(), &other),
            SLOT_TABLE_DRIFT_RULE_ID,
            "must be a hard configMapKeyRef {name: gc-service-config-h",
        );
        let wrong_key = with_gc_env(
            GC_WT_PORTS,
            json!({"name": GC_WT_PORTS, "valueFrom": {"configMapKeyRef": {"name": "gc-service-config-h", "key": GC_WT_HOSTS}}}),
        );
        assert_finding(
            &check(&tree(), &wrong_key),
            SLOT_TABLE_DRIFT_RULE_ID,
            "env GC_MH_WT_ALLOWED_PORTS must be",
        );
        let optional = with_gc_env(
            GC_WT_PORTS,
            json!({"name": GC_WT_PORTS, "valueFrom": {"configMapKeyRef": {"name": "gc-service-config-h", "key": GC_WT_PORTS, "optional": true}}}),
        );
        assert_finding(
            &check(&tree(), &optional),
            SLOT_TABLE_DRIFT_RULE_ID,
            "must be a hard configMapKeyRef",
        );
        let missing = with_gc_env(GC_GRPC_CIDRS, Value::Null);
        assert_finding(
            &check(&tree(), &missing),
            SLOT_TABLE_DRIFT_RULE_ID,
            "expected exactly one env entry GC_MH_GRPC_ALLOWED_CIDRS",
        );
        let mut dup = probe();
        let env = dup.render[5]["spec"]["template"]["spec"]["containers"][0]["env"]
            .as_array_mut()
            .unwrap();
        let first = env[0].clone();
        env.push(first);
        assert_finding(&check(&tree(), &dup), SLOT_TABLE_DRIFT_RULE_ID, "found 2");
        let mut none = probe();
        none.render.retain(|d| name_of(d) != "gc-service");
        assert_finding(
            &check(&tree(), &none),
            VACUOUS_RULE_ID,
            "reaches no GC process",
        );
    }

    #[test]
    fn gc_allowlist_order_and_spacing_do_not_matter() {
        assert_eq!(check(&tree(), &with_gc(GC_WT_PORTS, " 4441 ,4440")), vec![]);
    }

    #[test]
    fn tmpl_and_static_pod_subnet_disagree() {
        let t = TreeInputs {
            kind_config_tmpl: TMPL.replace("192.168.0.0/16", "10.244.0.0/16"),
            ..tree()
        };
        assert_finding(
            &check(&t, &probe()),
            SLOT_TABLE_DRIFT_RULE_ID,
            "kind-config.yaml.tmpl networking.podSubnet = {10.244.0.0/16}",
        );
    }

    #[test]
    fn advertised_grpc_port_not_declared() {
        let mut p = probe();
        p.render[0]["spec"]["template"]["spec"]["containers"][0]["ports"][0]["containerPort"] =
            json!(50099);
        assert_finding(
            &check(&tree(), &p),
            SLOT_TABLE_DRIFT_RULE_ID,
            "declares no TCP containerPort 50053",
        );
    }

    #[test]
    fn empty_sides_are_vacuous_not_passing() {
        let f = check(&tree(), &with_gc(GC_WT_PORTS, ""));
        assert_finding(&f, VACUOUS_RULE_ID, "GC_MH_WT_ALLOWED_PORTS is empty");
        let mut p = probe();
        p.slots.clear();
        assert_finding(
            &check(&tree(), &p),
            VACUOUS_RULE_ID,
            "deploy.sh:mh_slot_table is empty",
        );
        let mut p = probe();
        p.render.retain(|d| name_of(d) != "gc-service-config-h");
        assert_finding(
            &check(&tree(), &p),
            VACUOUS_RULE_ID,
            "no rendered ConfigMap carries GC's MH allowlists",
        );
        let mut p = probe();
        let mut second = gc_cm();
        second["metadata"]["name"] = json!("gc-other");
        p.render.push(second);
        assert_finding(&check(&tree(), &p), SLOT_TABLE_DRIFT_RULE_ID, "ambiguous");
    }

    #[test]
    fn unreadable_constants_fail_closed() {
        let t = TreeInputs {
            deploy_sh: String::new(),
            ..tree()
        };
        assert_finding(
            &check(&t, &probe()),
            UNVERIFIABLE_RULE_ID,
            "MH_TEMPLATE_DEPLOYMENT",
        );
        let t = TreeInputs {
            ports_rs: String::new(),
            ..tree()
        };
        assert_finding(&check(&t, &probe()), UNVERIFIABLE_RULE_ID, "MH_SLOT_COUNT");
    }

    #[test]
    fn probe_output_parses_and_rejects_truncation() {
        let ok = "@@@ SLOTS\n0 4440 https://127.0.0.1:4440\n@@@ RENDER\n{\"kind\":\"X\"}\n@@@ APPLY\n{\"kind\":\"List\",\"items\":[{\"kind\":\"X\"}]}\n@@@ END\n";
        let p = parse_probe(ok).unwrap();
        assert_eq!((p.slots.len(), p.render.len(), p.apply.len()), (1, 1, 1));
        assert!(parse_probe(&ok.replace("@@@ END\n", "")).is_err());
        assert!(parse_probe(&ok.replace("0 4440", "zero 4440")).is_err());
    }

    #[test]
    fn probe_removes_every_dt_variable() {
        let vars = [
            "DT_KIND_CONFIG",
            "DT_PREBUILT_IMAGES",
            "DT_SOMETHING_NEW",
            "PATH",
            "HOME",
        ]
        .map(std::ffi::OsString::from);
        let removed = probe_env_removals(vars.into_iter());
        assert_eq!(
            removed,
            ["DT_KIND_CONFIG", "DT_PREBUILT_IMAGES", "DT_SOMETHING_NEW"]
                .map(std::ffi::OsString::from)
        );
    }

    #[test]
    fn image_repo_strips_tag_and_digest_not_registry_port() {
        assert_eq!(
            image_repo("localhost/mh-service:sha-1"),
            "localhost/mh-service"
        );
        assert_eq!(
            image_repo("registry:5000/mh-service@sha256:ab"),
            "registry:5000/mh-service"
        );
        assert_eq!(image_repo("mh-service"), "mh-service");
    }
}
