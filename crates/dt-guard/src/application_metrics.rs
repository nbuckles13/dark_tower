//! `application-metrics` subcommand — port of validate-application-metrics.sh.
//!
//! Seven checks (Python kernel L57-738):
//! 1. Service registration — every `crates/*/src/observability/metrics.rs`
//!    sits under a directory present in the canonical mapping.
//! 2. Metric-prefix correctness — `<prefix>_*` in `<prefix>-service`.
//! 3. Dashboard metrics exist in source code.
//! 4. Alert rule metrics exist in source code.
//! 5. Every defined metric appears in at least one Grafana dashboard.
//! 6. Every defined metric is documented in a catalog file.
//! 7. All Prometheus targets have explicit `editorMode` + (`range` | `instant`).
//!
//! Per @observability commitment #13 + @dry-reviewer F-DRY-1/2/3 2026-05-19:
//! shared canonical-home regexes live in `metric_macros::` and `common::`
//! (services / metric_catalog / grafana). This module consumes them
//! read-only; no duplicate `Lazy<Regex>` re-inlines remain.

use crate::common::metric_catalog::CATALOG_HEAD_RE;
use crate::common::scan::warn_skip;
use crate::common::services::{CANONICAL_SERVICES, SERVICE_METRIC_PREFIX_RE};
use crate::common::status::emit_ok;
use crate::metric_macros::MACRO_INVOCATION_WITH_FIRST_ARG_RE;
use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

const CRATES_SUBDIR: &str = "crates";
const DASHBOARDS_SUBDIR: &str = "infra/grafana/dashboards";
const CATALOG_SUBDIR: &str = "docs/observability/metrics";
const ALERTS_SUBDIR: &str = "infra/docker/prometheus/rules";

const HIST_SUFFIXES: &[&str] = &["_bucket", "_count", "_sum"];

pub const SERVICE_REGISTRATION_RULE_ID: &str = "service_registration";
pub const METRIC_PREFIX_RULE_ID: &str = "metric_prefix";
pub const DASHBOARD_METRIC_MISSING_RULE_ID: &str = "dashboard_metric_missing";
pub const ALERT_METRIC_MISSING_RULE_ID: &str = "alert_metric_missing";
pub const METRIC_NO_DASHBOARD_RULE_ID: &str = "metric_no_dashboard";
pub const METRIC_NO_CATALOG_RULE_ID: &str = "metric_no_catalog";
pub const TARGET_QUERY_FIELDS_RULE_ID: &str = "target_query_fields";

// CANONICAL_SERVICES, CATALOG_HEAD_RE, SERVICE_METRIC_PREFIX_RE moved to
// canonical-home modules per @dry-reviewer F-DRY-1/2/3 2026-05-19. See
// `common::services`, `common::metric_catalog`, `metric_macros`.

// -----------------------------------------------------------------------------
// Helpers
// -----------------------------------------------------------------------------

fn extract_metric_names_from_source(src: &str) -> HashSet<String> {
    let mut names = HashSet::new();
    // Capture group 3 of MACRO_INVOCATION_WITH_FIRST_ARG_RE is the metric name.
    // (Group 1 = optional `metrics::` prefix; group 2 = macro kind.)
    for caps in MACRO_INVOCATION_WITH_FIRST_ARG_RE.captures_iter(src) {
        if let Some(m) = caps.get(3) {
            names.insert(m.as_str().to_string());
        }
    }
    names
}

fn extract_catalog_metrics(repo_root: &Path) -> HashSet<String> {
    let mut out = HashSet::new();
    let catalog_dir = repo_root.join(CATALOG_SUBDIR);
    if !catalog_dir.is_dir() {
        return out;
    }
    let entries = match std::fs::read_dir(&catalog_dir) {
        Ok(e) => e,
        Err(e) => {
            warn_skip("catalog dir read", &catalog_dir, &e);
            return out;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.ends_with(".md") {
            continue;
        }
        let src = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                warn_skip("catalog file read", &path, &e);
                continue;
            }
        };
        for caps in CATALOG_HEAD_RE.captures_iter(&src) {
            if let Some(m) = caps.get(1) {
                out.insert(m.as_str().to_string());
            }
        }
    }
    out
}

/// Recursively flatten dashboard panels (descend into row-panel `.panels`).
fn collect_panels<'a>(panels: &'a Value, out: &mut Vec<&'a Value>) {
    let Some(arr) = panels.as_array() else { return };
    for p in arr {
        out.push(p);
        if p.get("type").and_then(Value::as_str) == Some("row") {
            if let Some(nested) = p.get("panels") {
                collect_panels(nested, out);
            }
        }
    }
}

fn extract_dashboard_metric_refs(repo_root: &Path) -> Vec<(String, HashSet<String>)> {
    let mut out = Vec::new();
    let dashboards_dir = repo_root.join(DASHBOARDS_SUBDIR);
    if !dashboards_dir.is_dir() {
        return out;
    }
    let entries = match std::fs::read_dir(&dashboards_dir) {
        Ok(e) => e,
        Err(e) => {
            warn_skip("dashboards dir read", &dashboards_dir, &e);
            return out;
        }
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
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
                warn_skip("dashboard file read", &path, &e);
                continue;
            }
        };
        let json = match serde_json::from_str::<Value>(&src) {
            Ok(j) => j,
            Err(e) => {
                warn_skip("dashboard JSON parse", &path, &e);
                continue;
            }
        };
        let mut metrics = HashSet::new();
        let mut panels = Vec::new();
        if let Some(p) = json.get("panels") {
            collect_panels(p, &mut panels);
        }
        for panel in panels {
            let Some(targets) = panel.get("targets").and_then(Value::as_array) else {
                continue;
            };
            for t in targets {
                if let Some(expr) = t.get("expr").and_then(Value::as_str) {
                    for caps in SERVICE_METRIC_PREFIX_RE.captures_iter(expr) {
                        if let Some(m) = caps.get(1) {
                            metrics.insert(m.as_str().to_string());
                        }
                    }
                }
            }
        }
        out.push((name.to_string(), metrics));
    }
    out
}

fn extract_alert_expr_metric_refs(repo_root: &Path) -> Vec<(String, HashSet<String>)> {
    let mut out = Vec::new();
    let alerts_dir = repo_root.join(ALERTS_SUBDIR);
    if !alerts_dir.is_dir() {
        return out;
    }
    let entries = match std::fs::read_dir(&alerts_dir) {
        Ok(e) => e,
        Err(e) => {
            warn_skip("alerts dir read", &alerts_dir, &e);
            return out;
        }
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !(name.ends_with(".yaml") || name.ends_with(".yml")) {
            continue;
        }
        let src = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                warn_skip("alert file read", &path, &e);
                continue;
            }
        };
        let doc = match serde_norway::from_str::<serde_norway::Value>(&src) {
            Ok(d) => d,
            Err(e) => {
                warn_skip("alert YAML parse", &path, &e);
                continue;
            }
        };
        let json = yaml_value_to_json(doc);
        let mut metrics = HashSet::new();
        let groups = json.get("groups").and_then(|v| v.as_array());
        if let Some(groups) = groups {
            for g in groups {
                let Some(rules) = g.get("rules").and_then(|v| v.as_array()) else {
                    continue;
                };
                for r in rules {
                    if let Some(expr) = r.get("expr").and_then(|v| v.as_str()) {
                        for caps in SERVICE_METRIC_PREFIX_RE.captures_iter(expr) {
                            if let Some(m) = caps.get(1) {
                                metrics.insert(m.as_str().to_string());
                            }
                        }
                    }
                }
            }
        }
        out.push((name.to_string(), metrics));
    }
    out
}

/// Convert serde_norway::Value → serde_json::Value (shallow, lossy for tags/aliases).
fn yaml_value_to_json(v: serde_norway::Value) -> Value {
    match v {
        serde_norway::Value::Null => Value::Null,
        serde_norway::Value::Bool(b) => Value::Bool(b),
        serde_norway::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Number(i.into())
            } else if let Some(u) = n.as_u64() {
                Value::Number(u.into())
            } else if let Some(f) = n.as_f64() {
                serde_json::Number::from_f64(f)
                    .map(Value::Number)
                    .unwrap_or(Value::Null)
            } else {
                Value::Null
            }
        }
        serde_norway::Value::String(s) => Value::String(s),
        serde_norway::Value::Sequence(seq) => {
            Value::Array(seq.into_iter().map(yaml_value_to_json).collect())
        }
        serde_norway::Value::Mapping(map) => {
            let mut o = serde_json::Map::new();
            for (k, val) in map {
                let key = match k {
                    serde_norway::Value::String(s) => s,
                    other => match serde_norway::to_string(&other) {
                        Ok(s) => s.trim().to_string(),
                        Err(_) => continue,
                    },
                };
                o.insert(key, yaml_value_to_json(val));
            }
            Value::Object(o)
        }
        serde_norway::Value::Tagged(_) => Value::Null,
    }
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
            let policy = format!("application-metrics::{}", self.rule_id);
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
    let crates_dir = repo_root.join(CRATES_SUBDIR);
    let mut findings: Vec<Finding> = Vec::new();

    // Step 1 + 2: scan canonical mapping; collect per-service metric sets.
    let mut service_metrics: HashMap<String, HashSet<String>> = HashMap::new();
    let mut all_metrics: HashSet<String> = HashSet::new();
    let mut canonical_dirs: HashMap<String, String> = HashMap::new();
    for (prefix, dir) in CANONICAL_SERVICES {
        canonical_dirs.insert((*dir).to_string(), (*prefix).to_string());
    }

    // Step 1: any metrics.rs in crates/ under a non-canonical dir = ERROR.
    for entry in WalkDir::new(&crates_dir)
        .max_depth(4)
        .into_iter()
        .filter_map(Result::ok)
    {
        if entry.file_name() != "metrics.rs" {
            continue;
        }
        let path = entry.path();
        // Expect crates/<dir>/src/observability/metrics.rs
        let Ok(rel) = path.strip_prefix(repo_root) else {
            continue;
        };
        let comps: Vec<&str> = rel
            .components()
            .filter_map(|c| c.as_os_str().to_str())
            .collect();
        if comps.len() < 5
            || comps.first() != Some(&"crates")
            || comps.get(2) != Some(&"src")
            || comps.get(3) != Some(&"observability")
        {
            continue;
        }
        let Some(dir_name) = comps.get(1).copied() else {
            continue;
        };
        if !canonical_dirs.contains_key(dir_name) {
            findings.push(Finding {
                file: rel.to_string_lossy().into_owned(),
                rule_id: SERVICE_REGISTRATION_RULE_ID,
                message: format!(
                    "service directory {dir_name:?} has metrics.rs but is not in CANONICAL_SERVICES \
                     — add to scripts/guards/common.sh"
                ),
            });
        }
    }

    // Step 2: per-service prefix correctness + collect metrics.
    for (prefix, dir) in CANONICAL_SERVICES {
        let metrics_path = crates_dir.join(dir).join("src/observability/metrics.rs");
        if !metrics_path.is_file() {
            continue; // optional service
        }
        let src = std::fs::read_to_string(&metrics_path)
            .with_context(|| format!("read metrics.rs for {prefix}"))?;
        let names = extract_metric_names_from_source(&src);
        for name in &names {
            let actual_prefix = name.split('_').next().unwrap_or("");
            if actual_prefix != *prefix {
                let rel = metrics_path
                    .strip_prefix(repo_root)
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|_| metrics_path.to_string_lossy().into_owned());
                findings.push(Finding {
                    file: rel,
                    rule_id: METRIC_PREFIX_RULE_ID,
                    message: format!(
                        "metric {name:?} uses prefix {actual_prefix:?}_ but should use {prefix:?}_ \
                         (directory: crates/{dir}/)"
                    ),
                });
            }
        }
        all_metrics.extend(names.iter().cloned());
        service_metrics.insert((*prefix).to_string(), names);
    }

    let has_metric = |m: &str| -> bool {
        if all_metrics.contains(m) {
            return true;
        }
        for suf in HIST_SUFFIXES {
            if let Some(base) = m.strip_suffix(suf) {
                if all_metrics.contains(base) {
                    return true;
                }
            }
        }
        false
    };

    // Step 3: dashboard metrics exist in source.
    let dashboard_refs = extract_dashboard_metric_refs(repo_root);
    for (dashboard_name, metrics) in &dashboard_refs {
        for m in metrics {
            if !has_metric(m) {
                findings.push(Finding {
                    file: dashboard_name.clone(),
                    rule_id: DASHBOARD_METRIC_MISSING_RULE_ID,
                    message: format!(
                        "dashboard uses metric {m:?} which is not defined in any crates/*/src/observability/metrics.rs"
                    ),
                });
            }
        }
    }

    // Step 4: alert metrics exist in source (expr only — annotations/labels excluded).
    let alert_refs = extract_alert_expr_metric_refs(repo_root);
    for (alert_name, metrics) in &alert_refs {
        for m in metrics {
            if !has_metric(m) {
                findings.push(Finding {
                    file: alert_name.clone(),
                    rule_id: ALERT_METRIC_MISSING_RULE_ID,
                    message: format!(
                        "alert uses metric {m:?} which is not defined in any crates/*/src/observability/metrics.rs"
                    ),
                });
            }
        }
    }

    // Step 5: every defined metric appears in at least one dashboard.
    let mut dashboard_set: HashSet<String> = HashSet::new();
    for (_, set) in &dashboard_refs {
        for m in set {
            dashboard_set.insert(m.clone());
            for suf in HIST_SUFFIXES {
                if let Some(base) = m.strip_suffix(suf) {
                    dashboard_set.insert(base.to_string());
                }
            }
        }
    }
    for (prefix, names) in &service_metrics {
        for m in names {
            if !dashboard_set.contains(m) {
                let dir = canonical_dirs
                    .iter()
                    .find_map(|(d, p)| if p == prefix { Some(d.as_str()) } else { None })
                    .unwrap_or("?");
                findings.push(Finding {
                    file: format!("crates/{dir}/src/observability/metrics.rs"),
                    rule_id: METRIC_NO_DASHBOARD_RULE_ID,
                    message: format!("metric {m:?} defined but not used in any Grafana dashboard"),
                });
            }
        }
    }

    // Step 6: every defined metric documented in catalog.
    let catalog_metrics = extract_catalog_metrics(repo_root);
    for (prefix, names) in &service_metrics {
        for m in names {
            if !catalog_metrics.contains(m) {
                let dir = canonical_dirs
                    .iter()
                    .find_map(|(d, p)| if p == prefix { Some(d.as_str()) } else { None })
                    .unwrap_or("?");
                findings.push(Finding {
                    file: format!("crates/{dir}/src/observability/metrics.rs"),
                    rule_id: METRIC_NO_CATALOG_RULE_ID,
                    message: format!(
                        "metric {m:?} defined but not documented in docs/observability/metrics/"
                    ),
                });
            }
        }
    }

    // Step 7: target editorMode + range|instant for Prometheus targets.
    let dashboards_dir = repo_root.join(DASHBOARDS_SUBDIR);
    if dashboards_dir.is_dir() {
        let entries = match std::fs::read_dir(&dashboards_dir) {
            Ok(e) => e,
            Err(e) => {
                warn_skip("dashboards dir read (step 7)", &dashboards_dir, &e);
                return finalize(findings, explain);
            }
        };
        let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
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
                    warn_skip("dashboard file read (step 7)", &path, &e);
                    continue;
                }
            };
            let json = match serde_json::from_str::<Value>(&src) {
                Ok(j) => j,
                Err(e) => {
                    warn_skip("dashboard JSON parse (step 7)", &path, &e);
                    continue;
                }
            };
            let mut panels = Vec::new();
            if let Some(p) = json.get("panels") {
                collect_panels(p, &mut panels);
            }
            for panel in panels {
                let panel_ds_type = panel
                    .get("datasource")
                    .and_then(|ds| ds.get("type"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let Some(targets) = panel.get("targets").and_then(Value::as_array) else {
                    continue;
                };
                for t in targets {
                    if t.get("expr").is_none() {
                        continue;
                    }
                    let target_ds_type = t
                        .get("datasource")
                        .and_then(|ds| ds.get("type"))
                        .and_then(Value::as_str)
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| panel_ds_type.clone());
                    if target_ds_type != "prometheus" {
                        continue;
                    }
                    let pid = panel.get("id").and_then(Value::as_i64).unwrap_or(0);
                    let ptitle = panel
                        .get("title")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    let refid = t
                        .get("refId")
                        .and_then(Value::as_str)
                        .unwrap_or("?")
                        .to_string();
                    if t.get("editorMode").is_none() {
                        findings.push(Finding {
                            file: name.to_string(),
                            rule_id: TARGET_QUERY_FIELDS_RULE_ID,
                            message: format!(
                                "panel {pid} ({ptitle}) target refId={refid}: missing editorMode"
                            ),
                        });
                    }
                    if t.get("range").is_none() && t.get("instant").is_none() {
                        findings.push(Finding {
                            file: name.to_string(),
                            rule_id: TARGET_QUERY_FIELDS_RULE_ID,
                            message: format!(
                                "panel {pid} ({ptitle}) target refId={refid}: missing range or instant"
                            ),
                        });
                    }
                }
            }
        }
    }

    finalize(findings, explain)
}

fn finalize(findings: Vec<Finding>, explain: bool) -> Result<()> {
    if findings.is_empty() {
        emit_ok("application-metrics-clean");
        return Ok(());
    }
    for f in &findings {
        f.print(explain);
    }
    anyhow::bail!("application-metrics: {} violation(s)", findings.len());
}
