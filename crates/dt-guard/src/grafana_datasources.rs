//! `grafana-datasources` subcommand — port of grafana-datasources.sh bespoke half.
//!
//! Two production rules (Wave 4 D-2 `grafana cli --dry-run` covers the
//! vendor-native UID-uniqueness/name-validity half — out of scope here per
//! ADR-0034 §8 + @observability commitment #14):
//! 1. **UID dedup**: every `datasource.uid` referenced in dashboards must be
//!    defined in `infra/grafana/provisioning/datasources/datasources.yaml`.
//!    Template references (`$var`, `${var}`) skip — they're resolved at
//!    render time by Grafana, handled by validate-dashboard-panels.sh
//!    template-var checks.
//! 2. **Loki-label consistency**: every label in a Loki query in dashboards
//!    is defined in Promtail's `relabel_configs` or `pipeline_stages.labels`.
//!    `level` is allowlisted (pipeline-extracted, not relabel-injected).

use crate::common::grafana::GRAFANA_TEMPLATE_VAR_RE;
use crate::common::scan::warn_skip;
use crate::common::status::emit_ok;
use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::Value;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

const DASHBOARDS_SUBDIR: &str = "infra/grafana/dashboards";
const DATASOURCES_CONFIG: &str = "infra/grafana/provisioning/datasources/datasources.yaml";
const PROMTAIL_CONFIG: &str = "infra/kubernetes/observability/promtail-config.yaml";

pub const UNDEFINED_DATASOURCE_UID_RULE_ID: &str = "undefined_datasource_uid";
pub const INVALID_LOKI_LABEL_RULE_ID: &str = "invalid_loki_label";

// GRAFANA_TEMPLATE_VAR_RE moved to canonical-home `common::grafana::GRAFANA_TEMPLATE_VAR_RE`
// per @dry-reviewer F-DRY-4 2026-05-19.

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static LOKI_LABEL_IN_BRACES_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\{[^}]+\}").expect("static pattern compiles"));

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static LABEL_NAME_EQ_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"([A-Za-z_][A-Za-z0-9_]*)\s*=").expect("static pattern compiles"));

// -----------------------------------------------------------------------------
// Datasource UID extraction.
// -----------------------------------------------------------------------------

fn extract_defined_uids(repo_root: &Path) -> Result<HashSet<String>> {
    let cfg_path = repo_root.join(DATASOURCES_CONFIG);
    let raw = std::fs::read_to_string(&cfg_path)
        .with_context(|| format!("read datasources config {DATASOURCES_CONFIG}"))?;
    let mut uids = HashSet::new();
    // Naive line-prefix scan for `uid:` matches the production behavior of
    // the shell guard's `grep -E '^\s+uid:'`. A proper YAML parse over the
    // datasources.yaml file would also work — we keep parity with the shell
    // guard for now.
    for line in raw.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("uid:") {
            let val = rest.trim().trim_matches('"').trim_matches('\'');
            if !val.is_empty() {
                uids.insert(val.to_string());
            }
        }
    }
    Ok(uids)
}

fn extract_referenced_uids(dashboards_dir: &Path) -> Result<HashSet<String>> {
    let mut uids = HashSet::new();
    let entries = match std::fs::read_dir(dashboards_dir) {
        Ok(e) => e,
        Err(e) => {
            warn_skip("dashboards dir read", dashboards_dir, &e);
            return Ok(uids);
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.ends_with(".json") {
            continue;
        }
        let src = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                warn_skip("dashboard file read", &path, &e);
                continue;
            }
        };
        let json: Value = match serde_json::from_str(&src) {
            Ok(j) => j,
            Err(e) => {
                warn_skip("dashboard JSON parse", &path, &e);
                continue;
            }
        };
        collect_datasource_uids(&json, &mut uids);
    }
    Ok(uids)
}

fn collect_datasource_uids(v: &Value, out: &mut HashSet<String>) {
    match v {
        Value::Object(obj) => {
            if let Some(ds) = obj.get("datasource") {
                if let Some(uid) = ds.get("uid").and_then(Value::as_str) {
                    // Skip Grafana built-in and template refs.
                    if uid != "-- Grafana --" && !GRAFANA_TEMPLATE_VAR_RE.is_match(uid) {
                        out.insert(uid.to_string());
                    }
                }
            }
            for (_, child) in obj {
                collect_datasource_uids(child, out);
            }
        }
        Value::Array(arr) => {
            for x in arr {
                collect_datasource_uids(x, out);
            }
        }
        _ => {}
    }
}

// -----------------------------------------------------------------------------
// Promtail Loki-label extraction.
// -----------------------------------------------------------------------------

fn extract_valid_loki_labels(repo_root: &Path) -> Option<HashSet<String>> {
    let cfg_path = repo_root.join(PROMTAIL_CONFIG);
    let raw = std::fs::read_to_string(&cfg_path).ok()?;
    let mut labels = HashSet::new();
    for doc in serde_norway::Deserializer::from_str(&raw) {
        let value: Value = match serde::Deserialize::deserialize(doc) {
            Ok(v) => v,
            Err(e) => {
                warn_skip("promtail YAML doc deserialize", &cfg_path, &e);
                continue;
            }
        };
        if value.get("kind").and_then(Value::as_str) != Some("ConfigMap") {
            continue;
        }
        let Some(prom_yml) = value
            .get("data")
            .and_then(|d| d.get("promtail.yaml"))
            .and_then(Value::as_str)
        else {
            continue;
        };
        let promtail: Value = match serde_norway::from_str(prom_yml) {
            Ok(p) => p,
            Err(e) => {
                warn_skip("promtail.yaml inner parse", &cfg_path, &e);
                continue;
            }
        };
        let Some(scrape_configs) = promtail.get("scrape_configs").and_then(Value::as_array) else {
            continue;
        };
        for sc in scrape_configs {
            if let Some(relabels) = sc.get("relabel_configs").and_then(Value::as_array) {
                for r in relabels {
                    let action = r.get("action").and_then(Value::as_str).unwrap_or("replace");
                    if action != "replace" {
                        continue;
                    }
                    if let Some(target) = r.get("target_label").and_then(Value::as_str) {
                        if !target.starts_with("__") && !target.is_empty() {
                            labels.insert(target.to_string());
                        }
                    }
                }
            }
            if let Some(stages) = sc.get("pipeline_stages").and_then(Value::as_array) {
                for stage in stages {
                    if let Some(label_map) = stage.get("labels").and_then(Value::as_object) {
                        for k in label_map.keys() {
                            labels.insert(k.clone());
                        }
                    }
                }
            }
        }
    }
    Some(labels)
}

// -----------------------------------------------------------------------------
// Loki query traversal in dashboards.
// -----------------------------------------------------------------------------

fn collect_loki_exprs(v: &Value, out: &mut Vec<(String, String)>, current_title: &mut String) {
    if let Value::Object(obj) = v {
        if let Some(title) = obj.get("title").and_then(Value::as_str) {
            *current_title = title.to_string();
        }
        let is_loki = obj
            .get("datasource")
            .map(|ds| {
                ds.get("type").and_then(Value::as_str) == Some("loki")
                    || ds.get("uid").and_then(Value::as_str) == Some("loki")
            })
            .unwrap_or(false);
        if is_loki {
            if let Some(expr) = obj.get("expr").and_then(Value::as_str) {
                out.push((current_title.clone(), expr.to_string()));
            }
            if let Some(stream) = obj
                .get("query")
                .and_then(|q| q.get("stream"))
                .and_then(Value::as_str)
            {
                out.push((current_title.clone(), stream.to_string()));
            }
        }
        for (_, child) in obj {
            collect_loki_exprs(child, out, current_title);
        }
    } else if let Value::Array(arr) = v {
        for x in arr {
            collect_loki_exprs(x, out, current_title);
        }
    }
}

fn extract_labels_from_logql(expr: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    for braces in LOKI_LABEL_IN_BRACES_RE.find_iter(expr) {
        for caps in LABEL_NAME_EQ_RE.captures_iter(braces.as_str()) {
            if let Some(m) = caps.get(1) {
                out.insert(m.as_str().to_string());
            }
        }
    }
    out
}

// -----------------------------------------------------------------------------
// Finding
// -----------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Finding {
    file: String,
    rule_id: &'static str,
    message: String,
}

impl Finding {
    fn print(&self, explain: bool) {
        if explain {
            let policy = format!("grafana-datasources::{}", self.rule_id);
            crate::common::explain::print_finding(&crate::common::explain::Finding {
                file: &self.file,
                row: 0,
                col: 0,
                policy: &policy,
                matched: &self.message,
                extras: &[],
                src_file: file!(),
                src_line: line!(),
            });
        } else {
            println!(
                "VIOLATION: {} ({}) {}",
                self.file, self.rule_id, self.message
            );
        }
    }
}

// -----------------------------------------------------------------------------
// Entry point
// -----------------------------------------------------------------------------

pub fn run(repo_root: &Path, explain: bool) -> Result<()> {
    let dashboards_dir = repo_root.join(DASHBOARDS_SUBDIR);
    if !dashboards_dir.is_dir() {
        emit_ok("grafana-datasources-no-dashboards");
        return Ok(());
    }
    let datasources_path = repo_root.join(DATASOURCES_CONFIG);
    if !datasources_path.is_file() {
        emit_ok("grafana-datasources-no-config");
        return Ok(());
    }

    let mut findings: Vec<Finding> = Vec::new();

    // Rule 1: UID dedup.
    let defined_uids = extract_defined_uids(repo_root)?;
    let referenced_uids = extract_referenced_uids(&dashboards_dir)?;
    for uid in &referenced_uids {
        if !defined_uids.contains(uid) {
            findings.push(Finding {
                file: DATASOURCES_CONFIG.to_string(),
                rule_id: UNDEFINED_DATASOURCE_UID_RULE_ID,
                message: format!(
                    "dashboard references undefined datasource UID {uid:?} \
                     — add to {DATASOURCES_CONFIG}"
                ),
            });
        }
    }

    // Rule 2: Loki label consistency.
    if let Some(valid_loki_labels) = extract_valid_loki_labels(repo_root) {
        let mut paths: Vec<PathBuf> = std::fs::read_dir(&dashboards_dir)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .collect();
        paths.sort();
        for path in paths {
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !name.ends_with(".json") {
                continue;
            }
            let src = match std::fs::read_to_string(&path) {
                Ok(s) => s,
                Err(e) => {
                    warn_skip("loki dashboard file read", &path, &e);
                    continue;
                }
            };
            let json: Value = match serde_json::from_str(&src) {
                Ok(j) => j,
                Err(e) => {
                    warn_skip("loki dashboard JSON parse", &path, &e);
                    continue;
                }
            };
            let mut loki_exprs: Vec<(String, String)> = Vec::new();
            let mut title = String::new();
            collect_loki_exprs(&json, &mut loki_exprs, &mut title);
            for (panel_title, expr) in loki_exprs {
                let labels = extract_labels_from_logql(&expr);
                for label in labels {
                    // `level` is pipeline-extracted, not in relabel_configs.
                    if label == "level" {
                        continue;
                    }
                    if !valid_loki_labels.contains(&label) {
                        findings.push(Finding {
                            file: name.to_string(),
                            rule_id: INVALID_LOKI_LABEL_RULE_ID,
                            message: format!(
                                "panel [{panel_title}] uses invalid Loki label {label:?} \
                                 (not defined in promtail-config.yaml relabel_configs \
                                 or pipeline_stages.labels)"
                            ),
                        });
                    }
                }
            }
        }
    }

    if findings.is_empty() {
        emit_ok("grafana-datasources-clean");
        return Ok(());
    }
    for f in &findings {
        f.print(explain);
    }
    anyhow::bail!("grafana-datasources: {} violation(s)", findings.len());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templated_ref_detected() {
        assert!(GRAFANA_TEMPLATE_VAR_RE.is_match("$datasource"));
        assert!(GRAFANA_TEMPLATE_VAR_RE.is_match("${datasource}"));
        assert!(GRAFANA_TEMPLATE_VAR_RE.is_match("${datasource:raw}"));
        assert!(!GRAFANA_TEMPLATE_VAR_RE.is_match("prometheus"));
        assert!(!GRAFANA_TEMPLATE_VAR_RE.is_match("loki-uid-1"));
    }

    #[test]
    fn logql_label_extraction() {
        let labels =
            extract_labels_from_logql(r#"{namespace="dark-tower", pod=~"gc.*"} |= "error""#);
        assert!(labels.contains("namespace"));
        assert!(labels.contains("pod"));
        assert_eq!(labels.len(), 2);
    }
}
