//! `infrastructure-metrics` subcommand — port of validate-infrastructure-metrics.sh.
//!
//! Two checks:
//! 1. Docker-label patterns in infra queries (Kubernetes deployment forbids
//!    `name=`, `container_name=`, `image=`).
//! 2. Label references in infra queries that don't exist in the Prometheus
//!    scrape-config's relabel rules or Kubernetes SD defaults.
//!
//! Scope: only "infrastructure metrics" (container_*, kube_*, node_*, `up`).
//! Application metrics (ac_*, gc_*, mc_*, mh_*) are validated by
//! `application-metrics` instead.

use crate::common::status::emit_ok;
use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::Value;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

const PROMETHEUS_CONFIG_PATH: &str = "infra/kubernetes/observability/prometheus-config.yaml";
const DASHBOARDS_SUBDIR: &str = "infra/grafana/dashboards";

const DOCKER_LABELS: &[(&str, &str)] = &[
    ("name", r#"Docker Compose uses "name=" label"#),
    (
        "container_name",
        r#"Docker Compose uses "container_name=" label"#,
    ),
    ("image", r#"Docker Compose uses "image=" label"#),
];

const KUBERNETES_LABELS: &[&str] = &["namespace", "pod", "container", "node", "service"];

pub const DOCKER_LABEL_PATTERN_RULE_ID: &str = "docker_label_pattern";
pub const INVALID_INFRA_LABEL_RULE_ID: &str = "invalid_infra_label";

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static IS_INFRA_QUERY_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\b(?:container_|kube_|node_|up\b)").expect("static pattern compiles")
});

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static LABEL_REF_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\b(\w+)\s*[=~]").expect("static pattern compiles"));

// Brace-anchored label reference: matches `{label=` / `{label~` openers.
// Consumer iterates captures, equality-checks group 1 == target label.
// Replaces the prior dynamic-template `\{\s*<escaped>\s*[=~]` per
// @code-reviewer 2026-05-19 (b)-shape conversion.
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static BRACE_LABEL_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\{\s*(\w+)\s*[=~]").expect("static pattern compiles"));

// -----------------------------------------------------------------------------
// Prometheus schema extraction.
// -----------------------------------------------------------------------------

#[derive(Debug, Default)]
struct PromSchema {
    valid_labels: HashSet<String>,
}

fn extract_prometheus_schema(repo_root: &Path) -> Result<PromSchema> {
    let cfg_path = repo_root.join(PROMETHEUS_CONFIG_PATH);
    let raw = std::fs::read_to_string(&cfg_path)
        .with_context(|| format!("read prometheus config {PROMETHEUS_CONFIG_PATH}"))?;

    let mut valid_labels: HashSet<String> = HashSet::new();
    for doc in serde_norway::Deserializer::from_str(&raw) {
        let Ok(value): std::result::Result<Value, _> = serde::Deserialize::deserialize(doc) else {
            continue;
        };
        if value.get("kind").and_then(Value::as_str) != Some("ConfigMap") {
            continue;
        }
        let Some(prom_yml) = value
            .get("data")
            .and_then(|d| d.get("prometheus.yml"))
            .and_then(Value::as_str)
        else {
            continue;
        };
        let Ok(prom_value): std::result::Result<Value, _> = serde_norway::from_str(prom_yml) else {
            continue;
        };
        let Some(scrape_configs) = prom_value.get("scrape_configs").and_then(Value::as_array)
        else {
            continue;
        };
        for sc in scrape_configs {
            if sc.get("kubernetes_sd_configs").is_some() {
                for kl in KUBERNETES_LABELS {
                    valid_labels.insert((*kl).to_string());
                }
            }
            if let Some(relabels) = sc.get("relabel_configs").and_then(Value::as_array) {
                for r in relabels {
                    if r.get("action").and_then(Value::as_str) != Some("replace") {
                        continue;
                    }
                    if let Some(target) = r.get("target_label").and_then(Value::as_str) {
                        if !target.starts_with("__") && !target.is_empty() {
                            valid_labels.insert(target.to_string());
                        }
                    }
                }
            }
        }
    }
    Ok(PromSchema { valid_labels })
}

// -----------------------------------------------------------------------------
// Finding
// -----------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Finding {
    file: String,
    panel_title: String,
    rule_id: &'static str,
    message: String,
}

impl Finding {
    fn print(&self, explain: bool) {
        if explain {
            let policy = format!("infrastructure-metrics::{}", self.rule_id);
            crate::common::explain::print_finding(&crate::common::explain::Finding {
                file: &self.file,
                row: 0,
                col: 0,
                policy: &policy,
                matched: &self.message,
                extras: &[("panel_title", &self.panel_title)],
                src_file: file!(),
                src_line: line!(),
            });
        } else {
            println!(
                "VIOLATION: {} [{}] ({}) {}",
                self.file, self.panel_title, self.rule_id, self.message
            );
        }
    }
}

// -----------------------------------------------------------------------------
// Entry point
// -----------------------------------------------------------------------------

pub fn run(repo_root: &Path, explain: bool) -> Result<()> {
    let schema = extract_prometheus_schema(repo_root)?;
    let dashboards_dir = repo_root.join(DASHBOARDS_SUBDIR);
    if !dashboards_dir.is_dir() {
        emit_ok("infrastructure-metrics-no-dashboards");
        return Ok(());
    }
    let mut findings: Vec<Finding> = Vec::new();
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dashboards_dir)
        .with_context(|| format!("read dashboards dir {DASHBOARDS_SUBDIR}"))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    paths.sort();

    for path in &paths {
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.ends_with(".json") {
            continue;
        }
        let Ok(src) = std::fs::read_to_string(path) else {
            continue;
        };
        let Ok(dashboard): std::result::Result<Value, _> = serde_json::from_str(&src) else {
            continue;
        };
        let Some(panels) = dashboard.get("panels").and_then(Value::as_array) else {
            continue;
        };
        for panel in panels {
            let panel_title = panel
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("Untitled")
                .to_string();
            let Some(targets) = panel.get("targets").and_then(Value::as_array) else {
                continue;
            };
            for t in targets {
                if t.get("datasource")
                    .and_then(|ds| ds.get("type"))
                    .and_then(Value::as_str)
                    != Some("prometheus")
                {
                    continue;
                }
                let Some(expr) = t
                    .get("expr")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                else {
                    continue;
                };
                if !IS_INFRA_QUERY_RE.is_match(expr) {
                    continue; // application metrics handled by application-metrics
                }

                // Docker-label check: walk `LABEL_REF_RE` captures (canonical
                // home, line 52), test membership against `DOCKER_LABELS`. Per
                // @code-reviewer 2026-05-19: replaces 5 dynamic `Regex::new`
                // sites with capture-iteration + `as_str()` equality — same
                // (b)-shape ruling already applied to the cite-extract resolvers.
                let mut docker_hit: HashSet<&'static str> = HashSet::new();
                for caps in LABEL_REF_RE.captures_iter(expr) {
                    let Some(m) = caps.get(1) else { continue };
                    let captured = m.as_str();
                    let Some((label, desc)) = DOCKER_LABELS.iter().find(|(l, _)| *l == captured)
                    else {
                        continue;
                    };
                    docker_hit.insert(*label);
                    findings.push(Finding {
                        file: name.to_string(),
                        panel_title: panel_title.clone(),
                        rule_id: DOCKER_LABEL_PATTERN_RULE_ID,
                        message: format!(
                            "uses Docker label pattern: {label}= — {desc}. \
                             Kubernetes deployment should use: {k8s}",
                            k8s = KUBERNETES_LABELS.join(", ")
                        ),
                    });
                }

                // Label-ref check: only flag labels inside `{...}` braces.
                for caps in LABEL_REF_RE.captures_iter(expr) {
                    let Some(label_m) = caps.get(1) else { continue };
                    let label = label_m.as_str();
                    if matches!(label, "__name__" | "job" | "instance") {
                        continue;
                    }
                    // Check the label appears after `{`: walk `BRACE_LABEL_RE`
                    // captures and look for an `as_str() == label` match. (b)-
                    // shape per @code-reviewer; no per-iteration regex compile.
                    let brace_matched = BRACE_LABEL_RE
                        .captures_iter(expr)
                        .filter_map(|c| c.get(1))
                        .any(|m| m.as_str() == label);
                    if !brace_matched {
                        continue;
                    }
                    if docker_hit.contains(label) {
                        continue; // already reported as docker pattern
                    }
                    if !schema.valid_labels.contains(label) {
                        findings.push(Finding {
                            file: name.to_string(),
                            panel_title: panel_title.clone(),
                            rule_id: INVALID_INFRA_LABEL_RULE_ID,
                            message: format!(
                                "uses label {label:?} which is not in Prometheus config. \
                                 Valid infrastructure labels: {valid}",
                                valid = {
                                    let mut sorted: Vec<&str> =
                                        schema.valid_labels.iter().map(String::as_str).collect();
                                    sorted.sort();
                                    sorted.join(", ")
                                }
                            ),
                        });
                    }
                }
            }
        }
    }

    if findings.is_empty() {
        emit_ok(format!(
            "infrastructure-metrics-clean-{}-files",
            paths.len()
        ));
        return Ok(());
    }
    for f in &findings {
        f.print(explain);
    }
    anyhow::bail!(
        "infrastructure-metrics: {} violation(s) across {} file(s)",
        findings.len(),
        paths.len()
    );
}
