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
/// The Promtail server config itself — the source of the `promtail-config`
/// ConfigMap generator in `infra/kubernetes/observability/kustomization.yaml`.
const PROMTAIL_CONFIG: &str = "infra/kubernetes/observability/promtail.yaml";

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

/// REASON token when the valid-label set cannot be derived. Its own token, and
/// a FAIL: this used to be `Option` + `if let Some`, so a moved or unparseable
/// promtail config silently skipped the whole Loki-label rule.
pub const PROMTAIL_LABELS_UNAVAILABLE_TOKEN: &str =
    "grafana-datasources-promtail-labels-unavailable";

/// Derive the valid Loki label set from the Promtail server config.
///
/// Fails closed: a missing or unparseable file, a file with no
/// `scrape_configs`, or an EMPTY resulting label set is an error. An empty set
/// is never a pass — it would either flag every dashboard label (noise) or,
/// under a softer rule, check nothing (silence); both mean the extraction no
/// longer matches the file's shape.
fn extract_valid_loki_labels(repo_root: &Path) -> Result<HashSet<String>> {
    let cfg_path = repo_root.join(PROMTAIL_CONFIG);
    let raw = std::fs::read_to_string(&cfg_path)
        .with_context(|| format!("reading Promtail config {PROMTAIL_CONFIG}"))?;
    let promtail: Value = serde_norway::from_str(&raw)
        .with_context(|| format!("parsing Promtail config {PROMTAIL_CONFIG}"))?;
    let scrape_configs = promtail
        .get("scrape_configs")
        .and_then(Value::as_array)
        .with_context(|| format!("{PROMTAIL_CONFIG} has no `scrape_configs` list"))?;
    let mut labels = HashSet::new();
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
    if labels.is_empty() {
        anyhow::bail!(
            "{PROMTAIL_CONFIG} yielded no Loki labels (no replace-action relabel target_label, no pipeline labels stage)"
        );
    }
    Ok(labels)
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

    // Rule 2: Loki label consistency. Fails closed if the label set cannot be
    // derived — see `extract_valid_loki_labels`.
    let valid_loki_labels = extract_valid_loki_labels(repo_root).map_err(|e| {
        eprintln!("ERROR: {e:#}");
        anyhow::anyhow!("{PROMTAIL_LABELS_UNAVAILABLE_TOKEN}")
    })?;
    {
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
                                 (not defined in promtail.yaml relabel_configs \
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
    #[test]
    fn real_promtail_config_yields_known_labels() {
        // Positive control on the in-tree file: `app` and `namespace` are
        // relabel `target_label`s in infra/kubernetes/observability/promtail.yaml.
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let labels = super::extract_valid_loki_labels(&root).expect("real promtail config");
        for known in ["app", "namespace"] {
            assert!(labels.contains(known), "{known} missing from {labels:?}");
        }
    }

    #[test]
    fn run_fails_with_its_own_token_when_promtail_config_is_missing() {
        let td = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(td.path().join(super::DASHBOARDS_SUBDIR)).expect("mkdir");
        let ds = td.path().join(super::DATASOURCES_CONFIG);
        std::fs::create_dir_all(ds.parent().expect("parent")).expect("mkdir");
        std::fs::write(&ds, "datasources: []\n").expect("write");
        let err = super::run(td.path(), false).expect_err("must fail closed");
        assert_eq!(
            crate::common::status::reason_token(&err),
            super::PROMTAIL_LABELS_UNAVAILABLE_TOKEN
        );
    }

    #[test]
    fn missing_unparseable_or_empty_promtail_config_fails_closed() {
        let td = tempfile::tempdir().expect("tempdir");
        assert!(
            super::extract_valid_loki_labels(td.path()).is_err(),
            "missing file"
        );
        let dir = td.path().join("infra/kubernetes/observability");
        std::fs::create_dir_all(&dir).expect("mkdir");
        let cfg = dir.join("promtail.yaml");
        std::fs::write(&cfg, "scrape_configs: [unterminated\n").expect("write");
        assert!(
            super::extract_valid_loki_labels(td.path()).is_err(),
            "unparseable"
        );
        std::fs::write(&cfg, "server: {}\n").expect("write");
        assert!(
            super::extract_valid_loki_labels(td.path()).is_err(),
            "no scrape_configs"
        );
        std::fs::write(&cfg, "scrape_configs:\n  - job_name: x\n").expect("write");
        assert!(
            super::extract_valid_loki_labels(td.path()).is_err(),
            "empty label set"
        );
    }

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
