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
#[cfg(test)]
use crate::common::metric_catalog::parse_annotations;
use crate::common::metric_catalog::{parse_annotations_dir, MetricAnnotations, CATALOG_HEAD_RE};
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
use std::collections::{BTreeSet, HashMap, HashSet};
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
/// A `*_total` counter on a `stat`/`gauge`/`bargauge` panel must use the
/// `$__range` window; on a `timeseries` panel it must use `$__rate_interval`
/// (ADR-0029 Category A — the counter-visibility fix makes stats honest with
/// `increase($__range)`, timeseries with `increase($__rate_interval)`).
pub const COUNTER_WINDOW_RULE_ID: &str = "counter_window";
/// A panel over a catalog-declared expected-empty counter must carry the
/// empty-is-healthy description marker.
pub const EXPECTED_EMPTY_DESCRIPTION_RULE_ID: &str = "expected_empty_description";
/// Asymmetric-vacuity control: dashboards exist but the catalogs yielded ZERO
/// `Expected-empty` annotations — a parse/format drift that would make the
/// expected-empty rule silently vacuous. Distinct token, not a silent pass.
pub const EXPECTED_EMPTY_SET_EMPTY_RULE_ID: &str = "expected_empty_set_empty";
/// Mixed-class panel (carries BOTH a zero-init'd expected-empty ref and an
/// exempt ref, so its description must satisfy both markers): the description
/// must ATTRIBUTE each class to its metrics, so an operator facing an empty
/// panel can tell present-and-genuinely-zero from a broken pipeline. Enforced
/// as set-equality against the panel's OWN exempt refs (each named verbatim,
/// none stray from another panel) plus a `count()` discriminator. ADR-0029 §C
/// mixed-class; observability + operations converged.
pub const MIXED_PANEL_ATTRIBUTION_RULE_ID: &str = "mixed_panel_attribution";

/// Panel types on which the counter-window + expected-empty rules apply.
const COUNTER_PANEL_TYPES: &[&str] = &["stat", "gauge", "bargauge", "timeseries"];
/// Description markers (ADR-0029 §C), matched case-insensitively. TWO variants
/// selected by the counter's `Zero-init: exempt` status (OPS-21): a zero-init'd
/// expected-empty counter reads a flat 0 when healthy and an ABSENT series is a
/// FAULT, so its panels must say `zero is healthy`; an exempt (unbounded /
/// lazily-created) counter is legitimately absent when healthy, so its panels
/// say `empty is healthy`. A guard mandating one phrase across both meanings
/// would enforce a sentence that is false on the zero-init'd majority.
const ZERO_IS_HEALTHY_MARKER: &str = "zero is healthy";
const EMPTY_IS_HEALTHY_MARKER: &str = "empty is healthy";

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

// PromQL label-matcher span `{…}`. Removed before the Category-B `/` test so a
// `/` inside a quoted label value is not misread as a division (INFRA-F1). Label
// matchers do not nest, so `[^}]*` is exact.
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static LABEL_MATCHER_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\{[^}]*\}").expect("static pattern compiles"));

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

/// Per-metric annotations (`Zero-init: exempt`, `Expected-empty: yes`) via the
/// shared `common::metric_catalog::parse_annotations_dir` — ONE directory-level
/// parse + ONE fail-closed merge policy, the SAME the guards share, so
/// `counter-zero-init` and `dashboard-panels` cannot disagree about which
/// counters are exempt / expected-empty (infra F3 — no per-guard `extend()`
/// loop). A catalog read failure propagates (F5): a file we cannot read is a
/// precondition failure, not silently "no annotations".
fn extract_annotations(repo_root: &Path) -> Result<HashMap<String, MetricAnnotations>> {
    parse_annotations_dir(&repo_root.join(CATALOG_SUBDIR))
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

/// Remove PromQL label-matcher `{…}` spans from an expression.
///
/// INFRA-F1: the Category-B test (`is_category_b`) keys on a `/` in the expr, but
/// a `/` can appear INSIDE a quoted label-matcher value (e.g. `{path="/health"}`)
/// where it is not a division. Testing the de-matched expr keeps such a value from
/// misclassifying a Category-A `*_total` stat as a Category-B ratio (which would
/// skip `COUNTER_WINDOW_RULE_ID` — a false negative). PromQL label matchers do not
/// nest, so a non-greedy `\{[^}]*\}` removal is exact. Mirrors the phantom-`[45]`
/// window defense (`regex_label_matcher_not_mistaken_for_window`).
fn strip_label_matchers(expr: &str) -> String {
    LABEL_MATCHER_RE.replace_all(expr, "").into_owned()
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
    let annotations = extract_annotations(repo_root)?;

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

    // Asymmetric-vacuity control (P7): dashboards exist but ZERO `Expected-empty`
    // annotations parsed. The same parse bug fails LOUD in counter-zero-init
    // (every counter becomes required → red) but would be SILENT here (no panel
    // required → green forever), so guard it with its own distinct token.
    let expected_empty_count = annotations
        .values()
        .filter(|a| a.expected_empty.is_present())
        .count();
    if expected_empty_count == 0 {
        findings.push(Finding {
            file: CATALOG_SUBDIR.to_string(),
            panel_id: 0,
            panel_title: String::new(),
            rule_id: EXPECTED_EMPTY_SET_EMPTY_RULE_ID,
            message:
                "dashboards exist but the catalogs yielded ZERO `Expected-empty: yes` annotations — \
                 the expected-empty description rule is vacuous. Either add the annotations or the \
                 catalog marker format has drifted from common::metric_catalog::parse_annotations"
                    .to_string(),
        });
    }

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
            &annotations,
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

#[allow(clippy::too_many_arguments)]
fn check_dashboard(
    dashboard: &Value,
    rel_path: &str,
    is_slo_dashboard: bool,
    metric_types: &HashMap<String, String>,
    catalog_metrics: &HashSet<String>,
    annotations: &HashMap<String, MetricAnnotations>,
    findings: &mut Vec<Finding>,
) {
    let mut panels: Vec<&Value> = Vec::new();
    if let Some(p) = dashboard.get("panels") {
        walk_panels(p, &mut panels);
    }

    // Catalog-wide exempt set (every `Zero-init: exempt` metric), for the
    // mixed-class set-equality check: a mixed panel's description must name
    // exactly its OWN exempt refs, so a stray name lifted from some OTHER
    // panel's exempt metric must be detectable — a subset test would let a
    // description copy-pasted from a future second mixed panel pass.
    let all_exempt_metrics: BTreeSet<&str> = annotations
        .iter()
        .filter(|(_, a)| a.zero_init_exempt.is_present())
        .map(|(k, _)| k.as_str())
        .collect();

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
        let pdesc = p
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        // Counter metric refs on this panel that are catalog-declared
        // expected-empty (accumulated across targets for the description check).
        let mut expected_empty_refs: BTreeSet<String> = BTreeSet::new();
        // Catalog-exempt refs on this panel (any `Zero-init: exempt` metric,
        // whether or not it is also expected-empty). Drives the widened
        // `needs_empty` and the mixed-class attribution check.
        let mut exempt_refs: BTreeSet<String> = BTreeSet::new();
        let panel_type_governed = COUNTER_PANEL_TYPES.contains(&ptype.as_str());

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

            let mut metric_refs: HashSet<String> = HashSet::new();
            for caps in SERVICE_METRIC_PREFIX_RE.captures_iter(expr) {
                if let Some(m) = caps.get(1) {
                    metric_refs.insert(m.as_str().to_string());
                }
            }

            // Category-A window distinction (ADR-0029): does this expr wrap a
            // `*_total` COUNTER in a rate-family fn? If so, the WINDOW verdict is
            // panel-type conditional and owned by COUNTER_WINDOW_RULE_ID; the
            // generic Rule 4 defers to it to avoid opposed advice. A ratio /
            // quantile (Category B) keeps the generic behavior.
            // INFRA-F1: test for division on the expr with label-matcher `{…}`
            // spans stripped, so a `/` inside a quoted label value (`{path=
            // "/health"}`) is not read as a ratio and does not misclassify a
            // Category-A `*_total` stat as Category-B (which would skip the
            // counter-window rule). `histogram_quantile` cannot hide in a matcher.
            let dematched_expr = strip_label_matchers(expr);
            let is_category_b =
                dematched_expr.contains('/') || dematched_expr.contains("histogram_quantile");
            let expr_counter_in_window = !is_category_b
                && metric_refs.iter().any(|m| {
                    m.ends_with("_total")
                        && metric_types.get(m).map(String::as_str) == Some("counter")
                        && metric_inside_fn(expr, m, &["rate", "increase", "irate"])
                });

            // Rule 4: rate window must be $__rate_interval (non-SLO), EXCEPT a
            // `*_total` counter on a governed panel — that window is owned by the
            // panel-type-conditional counter-window rule below.
            if !is_slo_dashboard && ignore_reason.is_none() {
                for caps in RATE_WINDOW_RE.captures_iter(expr) {
                    let (Some(fn_m), Some(win_m)) = (caps.get(1), caps.get(2)) else {
                        continue;
                    };
                    let fn_name = fn_m.as_str();
                    let window = win_m.as_str().trim();

                    if expr_counter_in_window && panel_type_governed {
                        // Counter-window rule owns this verdict.
                        let want = if ptype == "timeseries" {
                            "$__rate_interval"
                        } else {
                            "$__range"
                        };
                        if window != want {
                            findings.push(Finding {
                                file: rel_path.to_string(),
                                panel_id: pid,
                                panel_title: ptitle.clone(),
                                rule_id: COUNTER_WINDOW_RULE_ID,
                                message: format!(
                                    "{fn_name}() over a *_total counter on a {ptype} panel uses \
                                     window [{window}]; ADR-0029 Category A requires [{want}] — {}",
                                    if ptype == "timeseries" {
                                        "a timeseries rates over the scrape-adaptive interval"
                                    } else {
                                        "a stat counts over the selected dashboard range"
                                    }
                                ),
                            });
                        }
                        continue;
                    }

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

                // Accumulate expected-empty counter refs for the panel-level
                // description-marker check (Change 2). Select on the DECLARED
                // type, not the `_total` suffix.
                // OBS-F1: do NOT require the `zero is healthy` / present-at-0
                // marker on a Category-B ratio/quantile panel. A ratio like
                // `forwarded/received` is legitimately empty at 0/0, so forcing
                // "an absent series is a broken pipeline" onto it is false — the
                // window rule already special-cases Category B the same way.
                if declared_type.map(String::as_str) == Some("counter")
                    && !is_category_b
                    && annotations
                        .get(canonical)
                        .is_some_and(|a| a.expected_empty.is_present())
                {
                    expected_empty_refs.insert(canonical.to_string());
                }
                // Accumulate exempt refs regardless of expected-empty status —
                // an exempt ref that is NOT expected-empty (panel 5's
                // gc_http_requests_total / ac_errors_total) still needs the
                // "empty is healthy" statement. Not gated on declared type:
                // `Zero-init: exempt` is a counter-only annotation, matching
                // the catalog.
                if annotations
                    .get(canonical)
                    .is_some_and(|a| a.zero_init_exempt.is_present())
                {
                    exempt_refs.insert(canonical.to_string());
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

        // Change 2: expected-empty description marker, TWO variants selected by
        // the counter's Zero-init-exempt status (ADR-0029 §C, OPS-21; user ruled
        // description-marker only, NO noValue). `row`/`logs`/`table`/unknown panel
        // types fall through. A MIXED-class panel (both a zero-init'd ref and an
        // exempt ref) must satisfy BOTH tokens — not merely carry an exception
        // clause naming the exempt series; a correct-sounding clause with neither
        // literal token reds here, by design.
        //
        // Residual blind spot (corrected — obs+ops, measured not assumed): both
        // this class test and `metric_not_in_catalog` iterate the SAME
        // `metric_refs`, collected by `SERVICE_METRIC_PREFIX_RE` (`ac|gc|mc|mh`).
        // A governed panel carrying a NON-service-prefixed `_total` ref is
        // therefore classified on an incomplete ref set, and `metric_not_in_catalog`
        // does NOT backstop it — the two rules share the extractor, they do not
        // cover each other. Vacuous by measurement today: 0 governed panels carry
        // a non-service-prefixed `_total` ref. Trigger for when it stops being
        // vacuous: the `dt_client_*` panels on client-media.json (already
        // catalogued in client.md) gain a marker once the client exporter lands —
        // they are the first instance.
        if panel_type_governed && ignore_reason.is_none() && !expected_empty_refs.is_empty() {
            let desc_lc = pdesc.to_ascii_lowercase();
            // needs_zero: the panel carries a zero-init'd expected-empty ref (an
            //   expected-empty counter that is NOT exempt) — healthy reads a flat
            //   0 and an ABSENT series is a fault → requires `zero is healthy`.
            // needs_empty: the panel carries ANY exempt ref — legitimately-absent
            //   when healthy → requires `empty is healthy`. WIDENED (obs+ops): an
            //   exempt ref need not itself be expected-empty to need this
            //   statement (panel 5's gc_http_requests_total / ac_errors_total).
            let needs_zero = expected_empty_refs.iter().any(|r| !exempt_refs.contains(r));
            let needs_empty = !exempt_refs.is_empty();

            let missing_zero = needs_zero && !desc_lc.contains(ZERO_IS_HEALTHY_MARKER);
            let missing_empty = needs_empty && !desc_lc.contains(EMPTY_IS_HEALTHY_MARKER);
            if missing_zero || missing_empty {
                let refs: Vec<&str> = expected_empty_refs.iter().map(String::as_str).collect();
                let want = if missing_zero && missing_empty {
                    format!(
                        "both \"{ZERO_IS_HEALTHY_MARKER}\" (its zero-init'd refs) and \
                         \"{EMPTY_IS_HEALTHY_MARKER}\" (its exempt refs)"
                    )
                } else if missing_zero {
                    format!(
                        "\"{ZERO_IS_HEALTHY_MARKER}\" — its expected-empty counter is zero-init'd, \
                         so a healthy reading is a flat 0 and an ABSENT series is a fault, not calm"
                    )
                } else {
                    format!(
                        "\"{EMPTY_IS_HEALTHY_MARKER}\" — it carries an exempt \
                         (unbounded/lazily-created) counter, so an absent series is legitimately healthy"
                    )
                };
                findings.push(Finding {
                    file: rel_path.to_string(),
                    panel_id: pid,
                    panel_title: ptitle.clone(),
                    rule_id: EXPECTED_EMPTY_DESCRIPTION_RULE_ID,
                    message: format!(
                        "panel over expected-empty counter(s) {refs:?} lacks the required \
                         description marker: {want}"
                    ),
                });
            }

            // Mixed-class panel: carries BOTH a zero-init'd expected-empty ref
            // AND an exempt ref, so an empty panel is ambiguous — a missing
            // zero-init'd series is a broken pipeline, a missing exempt series is
            // healthy. The description must attribute each class to its metrics.
            // Enforced as set-equality against the panel's OWN exempt refs plus a
            // `count()` discriminator (obs+ops converged, ADR-0029 §C).
            if needs_zero && needs_empty {
                // (a) every exempt ref on the panel is named verbatim.
                let unnamed: Vec<&str> = exempt_refs
                    .iter()
                    .filter(|m| !pdesc.contains(m.as_str()))
                    .map(String::as_str)
                    .collect();
                if !unnamed.is_empty() {
                    findings.push(Finding {
                        file: rel_path.to_string(),
                        panel_id: pid,
                        panel_title: ptitle.clone(),
                        rule_id: MIXED_PANEL_ATTRIBUTION_RULE_ID,
                        message: format!(
                            "mixed-class panel description must name each exempt ref so an \
                             operator can attribute an empty series; unnamed: {unnamed:?}"
                        ),
                    });
                }
                // (b) set-equality, not subset: no exempt metric named that is
                // NOT on this panel — blocks a description lifted from another
                // mixed panel that carries that panel's exempt names.
                let stray: Vec<&str> = all_exempt_metrics
                    .iter()
                    .filter(|m| !exempt_refs.contains(**m) && pdesc.contains(**m))
                    .copied()
                    .collect();
                if !stray.is_empty() {
                    findings.push(Finding {
                        file: rel_path.to_string(),
                        panel_id: pid,
                        panel_title: ptitle.clone(),
                        rule_id: MIXED_PANEL_ATTRIBUTION_RULE_ID,
                        message: format!(
                            "mixed-class panel description names exempt metric(s) not on this \
                             panel: {stray:?} — description likely copied from another panel"
                        ),
                    });
                }
                // (c) the count() discriminator that tells present-and-zero from
                // a broken pipeline (existing in-tree convention: client-media
                // ids 3, 8).
                if !desc_lc.contains("count(") {
                    findings.push(Finding {
                        file: rel_path.to_string(),
                        panel_id: pid,
                        panel_title: ptitle.clone(),
                        rule_id: MIXED_PANEL_ATTRIBUTION_RULE_ID,
                        message:
                            "mixed-class panel description must name a count() discriminator so an \
                             operator can tell a present-and-genuinely-zero series from a broken \
                             pipeline"
                                .to_string(),
                    });
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

    // ---- GUARD 2: counter-window + expected-empty rules ----

    /// Run `check_dashboard` on a single planted well-formed panel and return the
    /// rule ids that fired. The panel carries a unit + templated datasource so
    /// pre-existing rules (unit, datasource) stay silent and only the rule under
    /// test can fire.
    fn fired_rules(
        ptype: &str,
        expr: &str,
        description: &str,
        metric_types: &[(&str, &str)],
        catalog: &[&str],
        annotations_src: &str,
    ) -> Vec<String> {
        let panel = serde_json::json!({
            "type": ptype,
            "id": 7,
            "title": "T",
            "description": description,
            "fieldConfig": {"defaults": {"unit": "short"}},
            "datasource": {"type": "prometheus", "uid": "$datasource"},
            "targets": [{
                "refId": "A",
                "expr": expr,
                "datasource": {"type": "prometheus", "uid": "$datasource"}
            }]
        });
        let dashboard = serde_json::json!({ "panels": [panel] });
        let mt: HashMap<String, String> = metric_types
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        let cm: HashSet<String> = catalog.iter().map(|s| (*s).to_string()).collect();
        let ann = parse_annotations(annotations_src);
        let mut findings = Vec::new();
        check_dashboard(
            &dashboard,
            "test.json",
            false,
            &mt,
            &cm,
            &ann,
            &mut findings,
        );
        findings.iter().map(|f| f.rule_id.to_string()).collect()
    }

    #[test]
    fn stat_counter_wrong_window_fires_counter_window_only() {
        // Stat over a *_total counter using $__rate_interval — wrong window.
        // counter_misuse stays SILENT (increase() IS present), and rate_window
        // defers to the counter rule, so ONLY counter_window fires.
        let r = fired_rules(
            "stat",
            "sum(increase(gc_x_total[$__rate_interval]))",
            "",
            &[("gc_x_total", "counter")],
            &["gc_x_total"],
            "",
        );
        assert_eq!(r, vec![COUNTER_WINDOW_RULE_ID.to_string()], "got {r:?}");
    }

    #[test]
    fn stat_counter_correct_window_passes() {
        let r = fired_rules(
            "stat",
            "sum(increase(gc_x_total[$__range]))",
            "",
            &[("gc_x_total", "counter")],
            &["gc_x_total"],
            "",
        );
        assert!(r.is_empty(), "expected clean, got {r:?}");
    }

    #[test]
    fn timeseries_counter_wrong_window_fires_counter_window_only() {
        let r = fired_rules(
            "timeseries",
            "sum by (outcome) (increase(gc_x_total[$__range]))",
            "",
            &[("gc_x_total", "counter")],
            &["gc_x_total"],
            "",
        );
        assert_eq!(r, vec![COUNTER_WINDOW_RULE_ID.to_string()], "got {r:?}");
    }

    #[test]
    fn timeseries_counter_correct_window_passes() {
        let r = fired_rules(
            "timeseries",
            "sum by (outcome) (increase(gc_x_total[$__rate_interval]))",
            "",
            &[("gc_x_total", "counter")],
            &["gc_x_total"],
            "",
        );
        assert!(r.is_empty(), "expected clean, got {r:?}");
    }

    #[test]
    fn regex_label_matcher_not_mistaken_for_window() {
        // `status_code=~"[45].."` must not be read as a `[45]` window; the true
        // window `$__range` is correct on this stat, so nothing fires.
        let r = fired_rules(
            "stat",
            r#"sum(increase(gc_x_total{status_code=~"[45].."}[$__range]))"#,
            "",
            &[("gc_x_total", "counter")],
            &["gc_x_total"],
            "",
        );
        assert!(r.is_empty(), "phantom [45] window or other finding: {r:?}");
    }

    #[test]
    fn category_b_ratio_keeps_rate_interval() {
        // A ratio (Category B) over counters keeps $__rate_interval on a stat —
        // the counter-window rule must NOT fire (division present).
        let r = fired_rules(
            "stat",
            "sum(rate(gc_e_total[$__rate_interval])) / sum(rate(gc_t_total[$__rate_interval]))",
            "",
            &[("gc_e_total", "counter"), ("gc_t_total", "counter")],
            &["gc_e_total", "gc_t_total"],
            "",
        );
        assert!(
            !r.contains(&COUNTER_WINDOW_RULE_ID.to_string()),
            "ratio wrongly hit counter_window: {r:?}"
        );
    }

    #[test]
    fn category_a_stat_with_slash_in_label_matcher_still_fires_counter_window() {
        // INFRA-F1: a `/` inside a label-matcher value must NOT make a Category-A
        // `*_total` stat look like a ratio. This stat uses the WRONG window
        // ($__rate_interval; a stat wants $__range), so counter_window MUST fire —
        // a regression that reads the `/` as a division would misclassify it
        // Category-B and silently skip the rule (false negative).
        let r = fired_rules(
            "stat",
            r#"sum(increase(gc_x_total{path="/health"}[$__rate_interval]))"#,
            "",
            &[("gc_x_total", "counter")],
            &["gc_x_total"],
            "",
        );
        assert!(
            r.contains(&COUNTER_WINDOW_RULE_ID.to_string()),
            "counter_window did NOT fire — a `/` in a label matcher was misread as a \
             ratio and the Category-A window rule was skipped: {r:?}"
        );
    }

    #[test]
    fn category_b_ratio_over_expected_empty_counter_does_not_require_marker() {
        // OBS-F1: a Category-B ratio referencing an expected-empty counter is
        // legitimately empty at 0/0, so it must NOT be forced to carry the
        // `zero is healthy` / present-at-0 marker (that would assert "absent =
        // broken pipeline", which is false for a ratio). The expected-empty
        // description rule must NOT fire even with no marker in the description.
        let ann = "### `gc_e_total`\n- **Expected-empty**: yes — no failures on a healthy idle cluster so this reads zero\n";
        let r = fired_rules(
            "stat",
            "sum(rate(gc_e_total[$__rate_interval])) / sum(rate(gc_t_total[$__rate_interval]))",
            "Delivery ratio; 0/0 renders empty and that is fine.",
            &[("gc_e_total", "counter"), ("gc_t_total", "counter")],
            &["gc_e_total", "gc_t_total"],
            ann,
        );
        assert!(
            !r.contains(&EXPECTED_EMPTY_DESCRIPTION_RULE_ID.to_string()),
            "expected_empty_description wrongly required a marker on a Category-B ratio: {r:?}"
        );
    }

    #[test]
    fn expected_empty_missing_marker_fires() {
        let ann = "### `gc_x_total`\n- **Expected-empty**: yes — no failures on a healthy idle cluster so this reads zero\n";
        let r = fired_rules(
            "stat",
            "sum(increase(gc_x_total[$__range]))",
            "Total failures in the range.",
            &[("gc_x_total", "counter")],
            &["gc_x_total"],
            ann,
        );
        assert_eq!(
            r,
            vec![EXPECTED_EMPTY_DESCRIPTION_RULE_ID.to_string()],
            "got {r:?}"
        );
    }

    #[test]
    fn expected_empty_with_marker_passes() {
        // Zero-init'd (non-exempt) expected-empty counter → needs "zero is healthy".
        let ann = "### `gc_x_total`\n- **Expected-empty**: yes — no failures on a healthy idle cluster so this reads zero\n";
        let r = fired_rules(
            "stat",
            "sum(increase(gc_x_total[$__range]))",
            "Zero is healthy: a flat 0 means no failures; an absent series is a fault. See runbook.",
            &[("gc_x_total", "counter")],
            &["gc_x_total"],
            ann,
        );
        assert!(r.is_empty(), "expected clean, got {r:?}");
    }

    #[test]
    fn expected_empty_zero_init_variant_rejects_empty_marker() {
        // OPS-21: a zero-init'd expected-empty counter panel that carries only the
        // exempt-variant "empty is healthy" token FIRES — for a zero-init'd counter
        // an absent series is a FAULT, so "empty is healthy" is the wrong sentence.
        let ann = "### `gc_x_total`\n- **Expected-empty**: yes — reads zero when healthy\n";
        let r = fired_rules(
            "stat",
            "sum(increase(gc_x_total[$__range]))",
            "A zero here means no failures; empty is healthy. See runbook.",
            &[("gc_x_total", "counter")],
            &["gc_x_total"],
            ann,
        );
        assert!(
            r.contains(&EXPECTED_EMPTY_DESCRIPTION_RULE_ID.to_string()),
            "zero-init'd counter must require the 'zero is healthy' token, got {r:?}"
        );
    }

    #[test]
    fn expected_empty_exempt_variant_accepts_empty_marker() {
        // An EXEMPT (unbounded/lazily-created) expected-empty counter panel is
        // legitimately absent when healthy → "empty is healthy" is correct, and
        // the "zero is healthy" token is NOT required.
        let ann = "### `mh_errors_total`\n\
                   - **Zero-init**: exempt — status_code is a raw runtime u16, an unbounded domain\n\
                   - **Expected-empty**: yes — errors read zero when healthy\n";
        let r = fired_rules(
            "timeseries",
            "sum by (error_type) (increase(mh_errors_total[$__rate_interval]))",
            "Empty is healthy: this counter is lazily created, so an absent series means no errors.",
            &[("mh_errors_total", "counter")],
            &["mh_errors_total"],
            ann,
        );
        assert!(
            r.is_empty(),
            "exempt variant with 'empty is healthy' should pass, got {r:?}"
        );
    }

    #[test]
    fn expected_empty_not_required_on_non_governed_panel() {
        // A `table` panel is not governed — no marker required.
        let ann = "### `gc_x_total`\n- **Expected-empty**: yes — reads zero when healthy on an idle cluster\n";
        let r = fired_rules(
            "table",
            "gc_x_total",
            "",
            &[("gc_x_total", "counter")],
            &["gc_x_total"],
            ann,
        );
        assert!(
            !r.contains(&EXPECTED_EMPTY_DESCRIPTION_RULE_ID.to_string()),
            "table should not require the marker: {r:?}"
        );
    }

    // ---- Mixed-class panel (obs+ops converged): a panel carrying BOTH a
    // zero-init'd expected-empty ref AND an exempt ref must satisfy both markers
    // and ATTRIBUTE each class to its metrics (set-equality + count()). ----

    /// stat expr over one zero-init'd expected-empty counter (`mc_dropped_total`)
    /// and two exempt counters (`gc_req_total`, `ac_err_total`), all inside
    /// `increase([$__range])` so counter/window rules stay silent.
    const MIXED_EXPR: &str = "sum(increase(mc_dropped_total[$__range])) + \
         sum(increase(gc_req_total[$__range])) + sum(increase(ac_err_total[$__range]))";
    const MIXED_TYPES: &[(&str, &str)] = &[
        ("mc_dropped_total", "counter"),
        ("gc_req_total", "counter"),
        ("ac_err_total", "counter"),
    ];
    const MIXED_CATALOG: &[&str] = &["mc_dropped_total", "gc_req_total", "ac_err_total"];
    const MIXED_ANN: &str = "### `mc_dropped_total`\n\
         - **Expected-empty**: yes — reads zero when healthy\n\
         ### `gc_req_total`\n\
         - **Zero-init**: exempt — status_code is a raw runtime u16, an unbounded domain\n\
         ### `ac_err_total`\n\
         - **Zero-init**: exempt — free-form error reason, an unbounded label domain\n";

    #[test]
    fn mixed_panel_only_strict_marker_fires() {
        // Fixture 1: mixed panel names its exempt refs + count but carries ONLY
        // the strict token — the exempt series still needs "empty is healthy".
        let r = fired_rules(
            "stat",
            MIXED_EXPR,
            "Zero is healthy for mc_dropped_total. Read count(mc_dropped_total). \
             gc_req_total and ac_err_total are here too.",
            MIXED_TYPES,
            MIXED_CATALOG,
            MIXED_ANN,
        );
        assert!(
            r.contains(&EXPECTED_EMPTY_DESCRIPTION_RULE_ID.to_string()),
            "missing 'empty is healthy' must fire, got {r:?}"
        );
    }

    #[test]
    fn mixed_panel_only_exempt_marker_fires() {
        // Fixture 2: only the exempt token — the zero-init'd ref still needs
        // "zero is healthy".
        let r = fired_rules(
            "stat",
            MIXED_EXPR,
            "Empty is healthy for gc_req_total and ac_err_total. Read count(mc_dropped_total).",
            MIXED_TYPES,
            MIXED_CATALOG,
            MIXED_ANN,
        );
        assert!(
            r.contains(&EXPECTED_EMPTY_DESCRIPTION_RULE_ID.to_string()),
            "missing 'zero is healthy' must fire, got {r:?}"
        );
    }

    #[test]
    fn mixed_panel_both_markers_names_exact_set_passes() {
        // Fixture 3: both tokens, names EXACTLY the panel's exempt set, count()
        // present → clean.
        let r = fired_rules(
            "stat",
            MIXED_EXPR,
            "Zero is healthy: for mc_dropped_total an absent series is a broken pipeline. \
             Empty is healthy: gc_req_total and ac_err_total are lazily created, so an absent \
             series there is healthy. Read count(mc_dropped_total) to tell them apart.",
            MIXED_TYPES,
            MIXED_CATALOG,
            MIXED_ANN,
        );
        assert!(
            r.is_empty(),
            "well-formed mixed panel should pass, got {r:?}"
        );
    }

    #[test]
    fn mixed_panel_names_only_some_exempt_refs_fires() {
        // Fixture 4: both tokens + count, but names only ONE of the two exempt
        // refs — a subset, must fire (an operator can't attribute the unnamed).
        let r = fired_rules(
            "stat",
            MIXED_EXPR,
            "Zero is healthy: mc_dropped_total absent means a broken pipeline. \
             Empty is healthy: gc_req_total is lazily created. Read count(mc_dropped_total).",
            MIXED_TYPES,
            MIXED_CATALOG,
            MIXED_ANN,
        );
        assert!(
            r.contains(&MIXED_PANEL_ATTRIBUTION_RULE_ID.to_string()),
            "naming only some exempt refs must fire attribution, got {r:?}"
        );
    }

    #[test]
    fn mixed_panel_names_exempt_metric_not_on_panel_fires() {
        // Fixture 5 (the copy-paste case): names all its own exempt refs AND a
        // stray exempt metric (`mh_extra_total`) that is NOT on this panel —
        // set-equality, not subset, must fire.
        let ann = "### `mc_dropped_total`\n\
                   - **Expected-empty**: yes — reads zero when healthy\n\
                   ### `gc_req_total`\n\
                   - **Zero-init**: exempt — raw status_code, an unbounded domain\n\
                   ### `ac_err_total`\n\
                   - **Zero-init**: exempt — free-form error reason, unbounded domain\n\
                   ### `mh_extra_total`\n\
                   - **Zero-init**: exempt — a different panel's exempt counter\n";
        let r = fired_rules(
            "stat",
            MIXED_EXPR,
            "Zero is healthy: mc_dropped_total absent means a broken pipeline. \
             Empty is healthy: gc_req_total, ac_err_total, and mh_extra_total are lazily created. \
             Read count(mc_dropped_total).",
            MIXED_TYPES,
            MIXED_CATALOG,
            ann,
        );
        assert!(
            r.contains(&MIXED_PANEL_ATTRIBUTION_RULE_ID.to_string()),
            "naming an exempt metric not on the panel must fire attribution, got {r:?}"
        );
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
