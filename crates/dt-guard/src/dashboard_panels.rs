//! `dashboard-panels` subcommand — port of validate-dashboard-panels.sh
//! Python kernel (ADR-0031 Prereq #2).
//!
//! Five per-panel rules:
//! 1. Metric-type classification (ADR-0029) — counter must be inside rate()/
//!    increase(); gauge must NOT be inside; histogram _bucket inside rate(),
//!    _sum/_count inside rate()/increase().
//! 2. Panel unit declared (fieldConfig.defaults.unit).
//! 3. Hard-coded datasource — must be `$var` / `${var}` / `${var:raw}`.
//! 4. `$__rate_interval` (non-SLO dashboards).
//! 5. Metric exists in code + catalog.
//!
//! Per @security commitment #16 — the `\bMETRIC\b` lookbehind site at the
//! Python kernel's metric_inside_fn function collapses cleanly to a positive
//! check using `\b<sym>\b` style via Rust regex word boundaries. No
//! `fancy-regex` adopted (DFA linear-time preserved).

use crate::common::grafana::GRAFANA_TEMPLATE_VAR_RE;
use crate::common::metric_catalog::CATALOG_HEAD_RE;
use crate::common::path_safety::to_repo_relative;
use crate::common::scan::warn_skip;
use crate::common::services::{CANONICAL_SERVICES, SERVICE_METRIC_PREFIX_RE};
use crate::common::status::emit_ok;
use crate::ignore::is_lazy_reason;
use crate::ignore::IGNORE_MARKER_HASH_RE;
use crate::metric_macros::{MacroKind, MACRO_INVOCATION_WITH_FIRST_ARG_RE};
use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

const DASHBOARDS_SUBDIR: &str = "infra/grafana/dashboards";
const CRATES_SUBDIR: &str = "crates";
const CATALOG_SUBDIR: &str = "docs/observability/metrics";
const HIST_SUFFIXES: &[&str] = &["_bucket", "_sum", "_count"];
const SLO_DASHBOARD_SUFFIX: &str = "-slos.json";
const TIME_RANGE_WINDOWS: &[&str] = &["$__range", "$__interval"];

pub const PANEL_UNIT_RULE_ID: &str = "panel_unit";
pub const HARDCODED_DATASOURCE_RULE_ID: &str = "hardcoded_datasource";
pub const RATE_WINDOW_RULE_ID: &str = "rate_window";
pub const COUNTER_MISUSE_RULE_ID: &str = "counter_misuse";
pub const GAUGE_MISUSE_RULE_ID: &str = "gauge_misuse";
pub const HISTOGRAM_MISUSE_RULE_ID: &str = "histogram_misuse";
pub const METRIC_NOT_IN_CODE_RULE_ID: &str = "metric_not_in_code";
pub const METRIC_NOT_IN_CATALOG_RULE_ID: &str = "metric_not_in_catalog";
pub const LAZY_IGNORE_REASON_RULE_ID: &str = "lazy_ignore_reason";

// MACRO_NAME_RE, CATALOG_HEAD_RE, SERVICE_METRIC_RE moved to canonical-home
// modules per @dry-reviewer F-DRY-1/2/3 2026-05-19. See `metric_macros`,
// `common::metric_catalog`, `common::services`.

// `rate(`, `increase(`, `irate(` rate-window function with [window] arg.
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static RATE_WINDOW_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\b(rate|increase|irate)\s*\(\s*[^)]*?\[([^\]]+)\]\s*\)")
        .expect("static pattern compiles")
});

// TEMPLATED_DS_RE moved to canonical-home `common::grafana::GRAFANA_TEMPLATE_VAR_RE`
// per @dry-reviewer F-DRY-4 2026-05-19.

// PromQL fn-call opener: matches `<ident>(`. Consumer iterates captures and
// equality-checks group 1 against a fn-name allowlist. (b)-shape per
// @code-reviewer 2026-05-19 — replaces a prior dynamic `\b<escaped>\s*\(`.
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static FN_CALL_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\b(\w+)\s*\(").expect("static pattern compiles"));

// Word-atom tokenizer: matches `\w+` runs. Used to walk a fn-call's balanced-
// paren span and equality-check each word against a target metric name.
// (b)-shape per @code-reviewer 2026-05-19 — replaces a prior dynamic
// `\b<escaped-metric>\b`.
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static WORD_ATOM_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\b(\w+)\b").expect("static pattern compiles"));

// -----------------------------------------------------------------------------
// Metric source extraction.
// -----------------------------------------------------------------------------

fn extract_metric_types(repo_root: &Path) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let crates_dir = repo_root.join(CRATES_SUBDIR);
    for (_, dir) in CANONICAL_SERVICES {
        let path = crates_dir.join(dir).join("src/observability/metrics.rs");
        if !path.is_file() {
            continue;
        }
        let src = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                warn_skip("metric-types source read", &path, &e);
                continue;
            }
        };
        // MACRO_INVOCATION_WITH_FIRST_ARG_RE captures: g1=metrics::prefix?,
        // g2=macro kind, g3=metric name. Dashboard-panels needs kind+name.
        for caps in MACRO_INVOCATION_WITH_FIRST_ARG_RE.captures_iter(&src) {
            let (Some(kind), Some(name)) = (caps.get(2), caps.get(3)) else {
                continue;
            };
            // describe_* macros declare the metric name but don't emit a
            // recording site; for the metric-type classifier we only want
            // the base macro family (counter / gauge / histogram).
            let Some(parsed) = MacroKind::parse(kind.as_str()) else {
                continue;
            };
            if parsed.is_describe() {
                continue;
            }
            out.insert(name.as_str().to_string(), parsed.as_str().to_string());
        }
    }
    out
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

// -----------------------------------------------------------------------------
// Panel walking + helpers.
// -----------------------------------------------------------------------------

fn walk_panels<'a>(panels: &'a Value, out: &mut Vec<&'a Value>) {
    let Some(arr) = panels.as_array() else { return };
    for p in arr {
        out.push(p);
        if p.get("type").and_then(Value::as_str) == Some("row") {
            if let Some(nested) = p.get("panels") {
                walk_panels(nested, out);
            }
        }
    }
}

fn datasource_uid_from(ds: Option<&Value>) -> Option<String> {
    match ds? {
        Value::String(s) => Some(s.clone()),
        Value::Object(o) => o.get("uid")?.as_str().map(|s| s.to_string()),
        _ => None,
    }
}

fn datasource_type_from(ds: Option<&Value>) -> Option<String> {
    ds?.as_object()?
        .get("type")?
        .as_str()
        .map(|s| s.to_string())
}

fn is_templated_datasource(uid: &str) -> bool {
    GRAFANA_TEMPLATE_VAR_RE.is_match(uid)
}

fn strip_hist_suffix(metric: &str) -> (String, Option<&'static str>) {
    for suf in HIST_SUFFIXES {
        if let Some(stripped) = metric.strip_suffix(suf) {
            return (stripped.to_string(), Some(*suf));
        }
    }
    (metric.to_string(), None)
}

/// Return true iff `metric` appears inside any fn-call from `fn_names`.
/// Balanced-paren walk per Python kernel; word-boundary equality check.
///
/// Per @code-reviewer 2026-05-19 (b)-shape: walks the canonical-home
/// `FN_CALL_RE` over `expr`, filters to `fn_names`-membership via
/// `as_str() == fn_name`, then walks `WORD_ATOM_RE` over each call's
/// balanced-paren span and equality-checks each word against `metric`.
/// No per-iteration regex compile.
#[expect(
    clippy::indexing_slicing,
    reason = "balanced-paren walker — `i` is bounded by `i < bytes.len()` and `start..i.saturating_sub(1)` is bounded by the same walk; indexing cannot panic"
)]
fn metric_inside_fn(expr: &str, metric: &str, fn_names: &[&str]) -> bool {
    let bytes = expr.as_bytes();
    for caps in FN_CALL_RE.captures_iter(expr) {
        let Some(name_m) = caps.get(1) else { continue };
        if !fn_names.iter().any(|n| *n == name_m.as_str()) {
            continue;
        }
        let Some(whole) = caps.get(0) else { continue };
        let start = whole.end(); // after `(`
        let mut depth: i32 = 1;
        let mut i = start;
        while i < bytes.len() && depth > 0 {
            match bytes[i] {
                b'(' => depth += 1,
                b')' => depth -= 1,
                _ => {}
            }
            i += 1;
        }
        let span = &expr[start..i.saturating_sub(1)];
        if WORD_ATOM_RE
            .captures_iter(span)
            .filter_map(|c| c.get(1))
            .any(|m| m.as_str() == metric)
        {
            return true;
        }
    }
    false
}

fn extract_ignore_reason(panel: &Value) -> (Option<String>, Option<String>) {
    let Some(desc) = panel
        .get("description")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    else {
        return (None, None);
    };
    let Some(caps) = IGNORE_MARKER_HASH_RE.captures(desc) else {
        return (None, None);
    };
    let Some(reason_m) = caps.get(1) else {
        return (None, None);
    };
    let reason = reason_m.as_str().trim().to_string();
    if is_lazy_reason(&reason) {
        let diag = format!(
            "guard:ignore reason too short or too vague: {reason:?} \
             (require >=10 chars, not test/tmp/todo/fixme/wip)"
        );
        return (None, Some(diag));
    }
    (Some(reason), None)
}

// -----------------------------------------------------------------------------
// Finding
// -----------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Finding {
    file: String,
    panel_id: i64,
    panel_title: String,
    rule_id: &'static str,
    message: String,
}

impl Finding {
    fn print(&self, explain: bool) {
        if explain {
            let policy = format!("dashboard-panels::{}", self.rule_id);
            let panel_id_str = self.panel_id.to_string();
            crate::common::explain::print_finding(&crate::common::explain::Finding {
                file: &self.file,
                row: 0,
                col: 0,
                policy: &policy,
                matched: &self.message,
                extras: &[("panel_id", &panel_id_str)],
                src_file: file!(),
                src_line: line!(),
            });
        } else {
            println!(
                "VIOLATION: {} panel={} [{}] ({}) {}",
                self.file, self.panel_id, self.panel_title, self.rule_id, self.message
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
        emit_ok("dashboard-panels-no-dir");
        return Ok(());
    }

    let metric_types = extract_metric_types(repo_root);
    let catalog_metrics = extract_catalog_metrics(repo_root);

    let mut json_files: Vec<PathBuf> = Vec::new();
    for entry in std::fs::read_dir(&dashboards_dir)
        .with_context(|| format!("read dashboards dir {DASHBOARDS_SUBDIR}"))?
    {
        let entry = entry.context("read dashboards dir entry")?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.ends_with(".json") {
            continue;
        }
        // Skip `_template-*.json` except for `_template-service-overview.json`
        // (the allow-listed starter that must still pass).
        if name.starts_with("_template-") && name != "_template-service-overview.json" {
            continue;
        }
        json_files.push(path);
    }
    json_files.sort();

    if json_files.is_empty() {
        emit_ok("dashboard-panels-no-files");
        return Ok(());
    }

    let mut findings: Vec<Finding> = Vec::new();
    for json_path in &json_files {
        let rel_path = to_repo_relative(repo_root, json_path);
        let raw = std::fs::read_to_string(json_path)
            .with_context(|| format!("read dashboard {rel_path}"))?;
        let base_name = json_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        let is_slo_dashboard = base_name.ends_with(SLO_DASHBOARD_SUFFIX);

        let dashboard: Value = serde_json::from_str(&raw)
            .with_context(|| format!("parse dashboard JSON {rel_path}"))?;

        check_dashboard(
            &dashboard,
            &rel_path,
            is_slo_dashboard,
            &metric_types,
            &catalog_metrics,
            &mut findings,
        );
    }

    if findings.is_empty() {
        emit_ok(format!("dashboard-panels-clean-{}-files", json_files.len()));
        return Ok(());
    }
    for f in &findings {
        f.print(explain);
    }
    anyhow::bail!(
        "dashboard-panels: {} violation(s) across {} file(s)",
        findings.len(),
        json_files.len()
    );
}

fn check_dashboard(
    dashboard: &Value,
    rel_path: &str,
    is_slo_dashboard: bool,
    metric_types: &HashMap<String, String>,
    catalog_metrics: &HashSet<String>,
    findings: &mut Vec<Finding>,
) {
    let mut panels: Vec<&Value> = Vec::new();
    if let Some(p) = dashboard.get("panels") {
        walk_panels(p, &mut panels);
    }

    for p in &panels {
        let ptype = p
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let pid = p.get("id").and_then(Value::as_i64).unwrap_or(0);
        let ptitle = p
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();

        if ptype == "row" {
            continue;
        }

        let (ignore_reason, lazy_diag) = extract_ignore_reason(p);
        if let Some(diag) = lazy_diag {
            findings.push(Finding {
                file: rel_path.to_string(),
                panel_id: pid,
                panel_title: ptitle.clone(),
                rule_id: LAZY_IGNORE_REASON_RULE_ID,
                message: diag,
            });
        }

        // Rule 2: unit declared (exempt: row, logs).
        if ptype != "logs" {
            let unit = p
                .get("fieldConfig")
                .and_then(|fc| fc.get("defaults"))
                .and_then(|d| d.get("unit"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty());
            if unit.is_none() {
                findings.push(Finding {
                    file: rel_path.to_string(),
                    panel_id: pid,
                    panel_title: ptitle.clone(),
                    rule_id: PANEL_UNIT_RULE_ID,
                    message: "fieldConfig.defaults.unit is missing or empty".to_string(),
                });
            }
        }

        // Rule 3: datasource templated.
        let panel_ds = p.get("datasource");
        let panel_ds_uid = datasource_uid_from(panel_ds);
        let panel_ds_type = datasource_type_from(panel_ds);
        if let Some(ref uid) = panel_ds_uid {
            if !is_templated_datasource(uid) {
                findings.push(Finding {
                    file: rel_path.to_string(),
                    panel_id: pid,
                    panel_title: ptitle.clone(),
                    rule_id: HARDCODED_DATASOURCE_RULE_ID,
                    message: format!(
                        "panel datasource.uid is hard-coded ({uid:?}); use $datasource template variable"
                    ),
                });
            }
        }

        let targets: Vec<&Value> = p
            .get("targets")
            .and_then(Value::as_array)
            .map(|v| v.iter().collect())
            .unwrap_or_default();

        for t in &targets {
            if let Some(t_ds_uid) = datasource_uid_from(t.get("datasource")) {
                if !is_templated_datasource(&t_ds_uid) {
                    let refid = t.get("refId").and_then(Value::as_str).unwrap_or("?");
                    findings.push(Finding {
                        file: rel_path.to_string(),
                        panel_id: pid,
                        panel_title: ptitle.clone(),
                        rule_id: HARDCODED_DATASOURCE_RULE_ID,
                        message: format!(
                            "target refId={refid} datasource.uid is hard-coded \
                             ({t_ds_uid:?}); use $datasource template variable"
                        ),
                    });
                }
            }
        }

        // Effective DS type: panel-level, fallback to first target.
        let effective_ds_type = panel_ds_type.or_else(|| {
            targets
                .iter()
                .find_map(|t| datasource_type_from(t.get("datasource")))
        });

        if ptype == "logs" {
            continue;
        }
        if effective_ds_type
            .as_deref()
            .is_some_and(|t| t.eq_ignore_ascii_case("loki"))
        {
            continue;
        }

        for t in &targets {
            let Some(expr) = t
                .get("expr")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
            else {
                continue;
            };

            // Rule 4: rate window must be $__rate_interval (non-SLO).
            if !is_slo_dashboard && ignore_reason.is_none() {
                for caps in RATE_WINDOW_RE.captures_iter(expr) {
                    let (Some(fn_m), Some(win_m)) = (caps.get(1), caps.get(2)) else {
                        continue;
                    };
                    let fn_name = fn_m.as_str();
                    let window = win_m.as_str().trim();
                    if window == "$__rate_interval" {
                        continue;
                    }
                    if TIME_RANGE_WINDOWS.contains(&window) {
                        continue;
                    }
                    findings.push(Finding {
                        file: rel_path.to_string(),
                        panel_id: pid,
                        panel_title: ptitle.clone(),
                        rule_id: RATE_WINDOW_RULE_ID,
                        message: format!(
                            "{fn_name}() uses hard-coded window [{window}]; \
                             use [$__rate_interval] per ADR-0029"
                        ),
                    });
                }
            }

            let mut metric_refs: HashSet<String> = HashSet::new();
            for caps in SERVICE_METRIC_PREFIX_RE.captures_iter(expr) {
                if let Some(m) = caps.get(1) {
                    metric_refs.insert(m.as_str().to_string());
                }
            }

            for mref in &metric_refs {
                let (base, suf) = strip_hist_suffix(mref);
                let canonical = if suf.is_some() { &base } else { mref };
                let declared_type = metric_types.get(mref).or_else(|| metric_types.get(&base));

                if !metric_types.contains_key(canonical) {
                    findings.push(Finding {
                        file: rel_path.to_string(),
                        panel_id: pid,
                        panel_title: ptitle.clone(),
                        rule_id: METRIC_NOT_IN_CODE_RULE_ID,
                        message: format!(
                            "metric {mref:?} not defined in crates/*/src/observability/metrics.rs"
                        ),
                    });
                    continue;
                }
                if !catalog_metrics.contains(canonical) {
                    findings.push(Finding {
                        file: rel_path.to_string(),
                        panel_id: pid,
                        panel_title: ptitle.clone(),
                        rule_id: METRIC_NOT_IN_CATALOG_RULE_ID,
                        message: format!(
                            "metric {mref:?} not documented in docs/observability/metrics/"
                        ),
                    });
                }

                if ignore_reason.is_some() {
                    continue;
                }

                match declared_type.map(String::as_str) {
                    Some("counter")
                        if !metric_inside_fn(expr, mref, &["rate", "increase", "irate"]) =>
                    {
                        findings.push(Finding {
                            file: rel_path.to_string(),
                            panel_id: pid,
                            panel_title: ptitle.clone(),
                            rule_id: COUNTER_MISUSE_RULE_ID,
                            message: format!(
                                "counter metric {mref:?} used without \
                                 rate()/increase() — violates ADR-0029 §Category A"
                            ),
                        });
                    }
                    Some("gauge")
                        if metric_inside_fn(expr, mref, &["rate", "increase", "irate"]) =>
                    {
                        findings.push(Finding {
                            file: rel_path.to_string(),
                            panel_id: pid,
                            panel_title: ptitle.clone(),
                            rule_id: GAUGE_MISUSE_RULE_ID,
                            message: format!(
                                "gauge metric {mref:?} wrapped in rate()/\
                                 increase() — gauges represent current value, \
                                 not a counting process"
                            ),
                        });
                    }
                    Some("histogram") => match suf {
                        Some("_bucket") if !metric_inside_fn(expr, mref, &["rate", "irate"]) => {
                            findings.push(Finding {
                                file: rel_path.to_string(),
                                panel_id: pid,
                                panel_title: ptitle.clone(),
                                rule_id: HISTOGRAM_MISUSE_RULE_ID,
                                message: format!(
                                    "histogram bucket {mref:?} must be inside \
                                     rate() — use histogram_quantile(..., rate({base}_bucket[...]))"
                                ),
                            });
                        }
                        Some("_sum") | Some("_count")
                            if !metric_inside_fn(expr, mref, &["rate", "increase", "irate"]) =>
                        {
                            findings.push(Finding {
                                file: rel_path.to_string(),
                                panel_id: pid,
                                panel_title: ptitle.clone(),
                                rule_id: HISTOGRAM_MISUSE_RULE_ID,
                                message: format!(
                                    "histogram series {mref:?} must be inside rate()/increase()"
                                ),
                            });
                        }
                        _ => {}
                    },
                    _ => {}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templated_datasource_accepts_all_shapes() {
        assert!(is_templated_datasource("$datasource"));
        assert!(is_templated_datasource("${datasource}"));
        assert!(is_templated_datasource("${datasource:raw}"));
        assert!(!is_templated_datasource("prometheus"));
        assert!(!is_templated_datasource("loki-uid-12345"));
    }

    #[test]
    fn strip_hist_suffix_basic() {
        assert_eq!(
            strip_hist_suffix("gc_http_duration_seconds_bucket"),
            ("gc_http_duration_seconds".to_string(), Some("_bucket"))
        );
        assert_eq!(
            strip_hist_suffix("gc_http_duration_seconds_count"),
            ("gc_http_duration_seconds".to_string(), Some("_count"))
        );
        assert_eq!(
            strip_hist_suffix("gc_request_total"),
            ("gc_request_total".to_string(), None)
        );
    }

    #[test]
    fn metric_inside_fn_detects() {
        let expr = "sum(rate(gc_http_total[5m]))";
        assert!(metric_inside_fn(expr, "gc_http_total", &["rate"]));
        assert!(!metric_inside_fn(expr, "gc_http_total", &["increase"]));
    }

    #[test]
    fn metric_inside_fn_avoids_substring() {
        // metric_inside_fn must use word boundaries — `gc_foo` should not
        // match `gc_foo_total` inside the same fn call.
        let expr = "rate(gc_foo_total[5m])";
        assert!(!metric_inside_fn(expr, "gc_foo", &["rate"]));
        assert!(metric_inside_fn(expr, "gc_foo_total", &["rate"]));
    }
}
