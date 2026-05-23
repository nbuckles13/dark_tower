//! `alert-rules-policy` subcommand — port of `scripts/guards/simple/validate-alert-rules.sh`
//! Python kernel.
//!
//! Five per-alert checks (ADR-0034 §1 + ADR-0031 Prereq #1):
//! 1. `annotations.runbook_url` present.
//! 2. runbook_url repo-relative under `docs/runbooks/` AND target exists on disk
//!    (consumes [`path_safety::resolve_cited_path`] per ADR §6 single SoT — per
//!    @security commitment #19 alert-rules MUST NOT re-implement containment).
//! 3. `labels.severity` in {page, warning, info}.
//! 4. `for:` duration ≥ 30s OR `expr` contains a qualifying expr-window
//!    (`rate`/`increase`/`sum_over_time(...[≥30s])`).
//! 5. Annotation hygiene (text-secret scan; ignore-hatch scoped here only).
//!
//! `# guard:ignore(<reason>)` markers on the alert's line (or the line
//! immediately above) bypass check 5 only, with the reason rejected as lazy
//! via the shared [`crate::ignore::is_lazy_reason`] kernel — per ADR §6,
//! this is the single SoT for lazy-reason vocabulary across cite-extract,
//! alert-rules, and metric-labels.
//!
//! Template files (`_template-*.yaml`) are skipped entirely.

use crate::common::duration::parse_prometheus_duration;
use crate::common::path_safety::{resolve_cited_path, to_repo_relative};
use crate::common::status::emit_ok;
use crate::ignore::{is_lazy_reason, IGNORE_MARKER_HASH_RE};
use crate::secret_patterns::{HYGIENE_PATTERNS, IPV4_ALLOWLIST, IPV4_REGEX, TEMPLATE_EXPR};
use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

const ALERTS_SUBDIR: &str = "infra/docker/prometheus/rules";
const RUNBOOKS_SUBDIR: &str = "docs/runbooks";
const MIN_FOR_SECONDS: u64 = 30;
const MIN_EXPR_WINDOW_SECONDS: u64 = 30;

const ALLOWED_SEVERITIES: &[&str] = &["page", "warning", "info"];

pub const RUNBOOK_URL_RULE_ID: &str = "runbook_url";
pub const SEVERITY_RULE_ID: &str = "severity";
pub const FOR_DURATION_RULE_ID: &str = "for_duration";
pub const ANNOTATION_HYGIENE_RULE_ID: &str = "annotation_hygiene";
pub const LAZY_IGNORE_REASON_RULE_ID: &str = "lazy_ignore_reason";

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static EXPR_WINDOW_FUNC_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\b(rate|increase|sum_over_time)\s*\(").expect("static pattern compiles")
});

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static RANGE_SELECTOR_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\[\s*((?:\d+[smhdwy])+)\s*\]").expect("static pattern compiles"));

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static URL_SCHEME_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^(https?:|//|file:)").expect("static pattern compiles"));

// `- alert: <name>` opener — used by `approximate_rule_line` to locate a
// rule's source line. Consumer iterates captures, equality-checks group 1
// against the alert name. (b)-shape per @code-reviewer 2026-05-19.
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static ALERT_LINE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?m)^\s*-\s*alert:\s*['"]?([A-Za-z_][\w.\-]*)['"]?\s*$"#)
        .expect("static pattern compiles")
});

// -----------------------------------------------------------------------------
// YAML schema. `deny_unknown_fields` is OFF intentionally — alert-rule YAML
// has many vendor-specific fields (interval, partial_response_strategy, etc.)
// that we don't care about. Strict schema validation is promtool's job
// (Wave 4 D-1); under Wave 1 we only need the fields we check.
// -----------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct AlertRulesDoc {
    #[serde(default)]
    groups: Vec<Group>,
}

#[derive(Debug, Deserialize)]
struct Group {
    #[serde(default)]
    rules: Vec<Rule>,
}

#[derive(Debug, Deserialize, Default)]
struct Rule {
    #[serde(default)]
    alert: Option<String>,
    #[serde(default, rename = "for")]
    for_duration: Option<serde_norway::Value>,
    #[serde(default)]
    expr: Option<serde_norway::Value>,
    #[serde(default)]
    labels: HashMap<String, serde_norway::Value>,
    #[serde(default)]
    annotations: HashMap<String, serde_norway::Value>,
}

// -----------------------------------------------------------------------------
// Helpers
// -----------------------------------------------------------------------------

/// Coerce a yaml scalar to a string. Returns None if value is a mapping/sequence/null.
fn yml_str(v: Option<&serde_norway::Value>) -> Option<String> {
    match v? {
        serde_norway::Value::String(s) => Some(s.clone()),
        serde_norway::Value::Number(n) => Some(n.to_string()),
        serde_norway::Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// Approximate the line number of `- alert: <name>` in the raw YAML text.
/// Returns 0 if not found. Quotes around the name are accepted.
fn approximate_rule_line(raw_lines: &[&str], alert_name: &str) -> usize {
    for (idx, line) in raw_lines.iter().enumerate() {
        if ALERT_LINE_RE
            .captures(line)
            .and_then(|c| c.get(1))
            .is_some_and(|m| m.as_str() == alert_name)
        {
            return idx + 1;
        }
    }
    0
}

/// Build `{line_no: reason}` for each `# guard:ignore(<reason>)` in the file.
/// Lazy reasons emit a `lazy_ignore_reason` violation and are NOT recorded.
fn load_ignore_lines(
    rel_path: &str,
    raw: &str,
    findings: &mut Vec<Finding>,
) -> HashMap<usize, String> {
    let mut out = HashMap::new();
    for (idx, line) in raw.lines().enumerate() {
        let line_no = idx + 1;
        if let Some(caps) = IGNORE_MARKER_HASH_RE.captures(line) {
            let Some(reason_m) = caps.get(1) else {
                continue;
            };
            let reason = reason_m.as_str().trim().to_string();
            if is_lazy_reason(&reason) {
                findings.push(Finding {
                    file: rel_path.to_string(),
                    alert: String::new(),
                    line: line_no,
                    rule_id: LAZY_IGNORE_REASON_RULE_ID,
                    message: format!(
                        "guard:ignore reason too short or too vague: {reason:?} \
                         (require >=10 chars, not test/tmp/todo/fixme/wip)"
                    ),
                    secret_pattern: None,
                });
            } else {
                out.insert(line_no, reason);
            }
        }
    }
    out
}

fn rule_is_ignored(rule_line: usize, ignore_lines: &HashMap<usize, String>) -> Option<String> {
    if let Some(r) = ignore_lines.get(&rule_line) {
        return Some(r.clone());
    }
    if rule_line > 0 {
        if let Some(r) = ignore_lines.get(&(rule_line - 1)) {
            return Some(r.clone());
        }
    }
    None
}

/// Scan `expr` for rate/increase/sum_over_time(...[<window>]) where window ≥ 30s.
/// Returns Some(max_window_seconds) or None if no qualifying window found.
fn find_qualifying_expr_window(expr: &str) -> Option<u64> {
    let mut best: Option<u64> = None;
    for m in EXPR_WINDOW_FUNC_RE.find_iter(expr) {
        let open_paren = m.end() - 1; // the `(`
        let mut depth: i32 = 0;
        let bytes = expr.as_bytes();
        let mut end_opt: Option<usize> = None;
        for (i, &b) in bytes.iter().enumerate().skip(open_paren) {
            match b {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        end_opt = Some(i);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(end) = end_opt else {
            continue;
        };
        let body = &expr[open_paren + 1..end];
        for rng in RANGE_SELECTOR_RE.captures_iter(body) {
            let Some(dur_str) = rng.get(1) else {
                continue;
            };
            let Some(secs) = parse_prometheus_duration(dur_str.as_str()) else {
                continue;
            };
            if secs >= MIN_EXPR_WINDOW_SECONDS {
                best = Some(best.map_or(secs, |b| b.max(secs)));
            }
        }
    }
    best
}

// -----------------------------------------------------------------------------
// Checks
// -----------------------------------------------------------------------------

fn validate_runbook_url(url: Option<&String>, repo_root: &Path) -> Result<(), String> {
    let url = url
        .map(|s| s.as_str())
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| "annotations.runbook_url is missing or empty".to_string())?;

    if URL_SCHEME_RE.is_match(url) {
        return Err(format!(
            "runbook_url must be repo-relative docs/runbooks/... (got: {url})"
        ));
    }
    if !url.starts_with("docs/runbooks/") {
        return Err(format!(
            "runbook_url must start with docs/runbooks/ (got: {url})"
        ));
    }
    // Strip fragment for filesystem check.
    let path_part = url.split('#').next().unwrap_or(url);
    let resolved = resolve_cited_path(repo_root, path_part).ok_or_else(|| {
        format!("runbook_url target cannot be resolved or escapes docs/runbooks/: {path_part}")
    })?;
    let runbooks_real = match std::fs::canonicalize(repo_root.join(RUNBOOKS_SUBDIR)) {
        Ok(p) => p,
        Err(_) => return Err(format!("runbooks dir {RUNBOOKS_SUBDIR} cannot be resolved")),
    };
    if !(resolved == runbooks_real || resolved.starts_with(&runbooks_real)) {
        return Err(format!(
            "runbook_url target escapes docs/runbooks/ via traversal or symlink: {path_part}"
        ));
    }
    if !resolved.is_file() {
        return Err(format!(
            "runbook_url target does not exist on disk: {path_part}"
        ));
    }
    Ok(())
}

fn validate_severity(sev: Option<&String>) -> Result<(), String> {
    let s = sev
        .map(|v| v.as_str())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| "labels.severity is missing or empty".to_string())?;
    if !ALLOWED_SEVERITIES.contains(&s) {
        let allowed = ALLOWED_SEVERITIES.join(", ");
        return Err(format!(
            "labels.severity must be in {{{allowed}}} (got: {s})"
        ));
    }
    Ok(())
}

fn validate_for_field(for_val: Option<&String>, expr: Option<&String>) -> Result<(), String> {
    let s = for_val.ok_or_else(|| "for: is missing".to_string())?;
    let Some(secs) = parse_prometheus_duration(s) else {
        return Err(format!(
            "for: is not a valid Prometheus duration (got: {s})"
        ));
    };
    if secs < MIN_FOR_SECONDS {
        if let Some(expr) = expr {
            if find_qualifying_expr_window(expr).is_some() {
                return Ok(());
            }
        }
        return Err(format!(
            "for: must be >= {MIN_FOR_SECONDS}s OR expr must contain a \
             rate/increase/sum_over_time(...[>= {MIN_EXPR_WINDOW_SECONDS}s]) \
             window (got for: {s} = {secs}s, no qualifying expr-window)"
        ));
    }
    Ok(())
}

fn check_hygiene(text: &str) -> Option<(&'static str, String)> {
    let scrubbed = TEMPLATE_EXPR.replace_all(text, "<<TEMPLATED>>");
    for m in IPV4_REGEX.find_iter(&scrubbed) {
        if !IPV4_ALLOWLIST.contains(&m.as_str()) {
            return Some(("public-or-private IPv4", m.as_str().to_string()));
        }
    }
    for (name, regex) in HYGIENE_PATTERNS.iter() {
        if let Some(m) = regex.find(&scrubbed) {
            return Some((name, m.as_str().to_string()));
        }
    }
    None
}

// -----------------------------------------------------------------------------
// Finding + entry point
// -----------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Finding {
    file: String,
    alert: String,
    line: usize,
    rule_id: &'static str,
    message: String,
    /// For `annotation_hygiene` only: the redacted pattern descriptor
    /// (e.g. `"AWS access key"`, `"bearer token"`). When `Some`, the
    /// finding is routed through `print_secret_finding` so the matched
    /// secret bytes are NOT echoed to stdout — per @semantic-guard Wave-2
    /// Q1 credential-leak fix. `None` for non-secret rules (runbook_url,
    /// severity, for_duration, lazy_ignore_reason) which continue to use
    /// `print_finding`.
    secret_pattern: Option<&'static str>,
}

impl Finding {
    fn print(&self, explain: bool) {
        if let Some(pattern) = self.secret_pattern {
            // Secret-redacted path. Both VIOLATION and EXPLAIN omit the raw
            // matched bytes; we emit only the pattern descriptor + the alert
            // name as a safe-to-echo hint.
            if explain {
                let policy = format!("alert-rules-policy::{}", self.rule_id);
                crate::common::explain::print_secret_finding(
                    &crate::common::explain::SecretFinding {
                        file: &self.file,
                        row: self.line,
                        col: 0,
                        policy: &policy,
                        pattern_name: pattern,
                        extras: &[("alert", &self.alert)],
                        src_file: file!(),
                        src_line: line!(),
                    },
                );
            } else {
                println!(
                    "VIOLATION: {}:{} [{}] annotation contains suspected {} (redacted)",
                    self.file, self.line, self.alert, pattern
                );
            }
            return;
        }
        if explain {
            let policy = format!("alert-rules-policy::{}", self.rule_id);
            crate::common::explain::print_finding(&crate::common::explain::Finding {
                file: &self.file,
                row: self.line,
                col: 0,
                policy: &policy,
                matched: &self.message,
                extras: &[("alert", &self.alert)],
                src_file: file!(),
                src_line: line!(),
            });
        } else {
            println!(
                "VIOLATION: {}:{} [{}] {}",
                self.file, self.line, self.alert, self.message
            );
        }
    }
}

pub fn run(repo_root: &Path, explain: bool) -> Result<()> {
    let alerts_dir = repo_root.join(ALERTS_SUBDIR);
    let mut yaml_files: Vec<std::path::PathBuf> = Vec::new();
    if alerts_dir.is_dir() {
        for entry in std::fs::read_dir(&alerts_dir)
            .with_context(|| format!("read alerts dir {ALERTS_SUBDIR}"))?
        {
            let entry = entry.context("read alerts dir entry")?;
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !(name.ends_with(".yaml") || name.ends_with(".yml")) {
                continue;
            }
            if name.starts_with("_template-") {
                continue;
            }
            yaml_files.push(path);
        }
    }
    yaml_files.sort();

    if yaml_files.is_empty() {
        emit_ok("alert-rules-no-files");
        return Ok(());
    }

    let mut all_findings: Vec<Finding> = Vec::new();
    for yaml_path in &yaml_files {
        let rel_path = to_repo_relative(repo_root, yaml_path);
        let raw = std::fs::read_to_string(yaml_path)
            .with_context(|| format!("read alert-rule file {rel_path}"))?;

        let raw_lines: Vec<&str> = raw.lines().collect();
        let ignore_lines = load_ignore_lines(&rel_path, &raw, &mut all_findings);

        let doc: AlertRulesDoc = serde_norway::from_str(&raw)
            .with_context(|| format!("parse alert-rule YAML {rel_path}"))?;

        for group in &doc.groups {
            for rule in &group.rules {
                let Some(alert_name) = rule.alert.as_ref().filter(|s| !s.is_empty()) else {
                    continue; // recording rule or malformed
                };
                let rule_line = approximate_rule_line(&raw_lines, alert_name);

                let runbook_url = rule
                    .annotations
                    .get("runbook_url")
                    .and_then(|v| yml_str(Some(v)));
                if let Err(msg) = validate_runbook_url(runbook_url.as_ref(), repo_root) {
                    all_findings.push(Finding {
                        file: rel_path.clone(),
                        alert: alert_name.clone(),
                        line: rule_line,
                        rule_id: RUNBOOK_URL_RULE_ID,
                        message: msg,
                        secret_pattern: None,
                    });
                }

                let severity = rule.labels.get("severity").and_then(|v| yml_str(Some(v)));
                if let Err(msg) = validate_severity(severity.as_ref()) {
                    all_findings.push(Finding {
                        file: rel_path.clone(),
                        alert: alert_name.clone(),
                        line: rule_line,
                        rule_id: SEVERITY_RULE_ID,
                        message: msg,
                        secret_pattern: None,
                    });
                }

                let for_str = yml_str(rule.for_duration.as_ref());
                let expr_str = yml_str(rule.expr.as_ref());
                if let Err(msg) = validate_for_field(for_str.as_ref(), expr_str.as_ref()) {
                    all_findings.push(Finding {
                        file: rel_path.clone(),
                        alert: alert_name.clone(),
                        line: rule_line,
                        rule_id: FOR_DURATION_RULE_ID,
                        message: msg,
                        secret_pattern: None,
                    });
                }

                // Check 5 — hygiene with ignore-hatch.
                if let Some(reason) = rule_is_ignored(rule_line, &ignore_lines) {
                    eprintln!(
                        "WARN: alert {alert_name} bypassed annotation_hygiene check — reason: {reason}"
                    );
                } else {
                    for field in &["summary", "description", "impact"] {
                        let Some(text) =
                            rule.annotations.get(*field).and_then(|v| yml_str(Some(v)))
                        else {
                            continue;
                        };
                        if let Some((kind, _hit)) = check_hygiene(&text) {
                            // _hit (raw matched bytes) is intentionally
                            // discarded — Wave-2 @semantic-guard Q1 credential
                            // -leak fix. `kind` is the redacted descriptor.
                            all_findings.push(Finding {
                                file: rel_path.clone(),
                                alert: alert_name.clone(),
                                line: rule_line,
                                rule_id: ANNOTATION_HYGIENE_RULE_ID,
                                message: format!(
                                    "annotations.{field} contains suspected {kind} (redacted)"
                                ),
                                secret_pattern: Some(kind),
                            });
                        }
                    }
                }
            }
        }
    }

    if all_findings.is_empty() {
        emit_ok(format!("alert-rules-clean-{}-files", yaml_files.len()));
        return Ok(());
    }
    for f in &all_findings {
        f.print(explain);
    }
    anyhow::bail!(
        "alert-rules: {} violation(s) across {} file(s)",
        all_findings.len(),
        yaml_files.len()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_qualifying_expr_window_basic() {
        // The falsifiable port test from ADR §Negative — 1.03× LoC target.
        let expr = r#"sum(rate(gc_http_requests_total[5m])) > 0.01"#;
        assert_eq!(find_qualifying_expr_window(expr), Some(300));
    }

    #[test]
    fn find_qualifying_expr_window_below_floor_rejected() {
        let expr = r#"sum(rate(gc_http_requests_total[10s])) > 0.01"#;
        // 10s < 30s floor → None.
        assert_eq!(find_qualifying_expr_window(expr), None);
    }

    #[test]
    fn find_qualifying_expr_window_picks_max() {
        let expr = r#"sum(rate(x[5m])) + sum(rate(y[10m]))"#;
        assert_eq!(find_qualifying_expr_window(expr), Some(600));
    }

    #[test]
    fn find_qualifying_expr_window_balanced_paren_only() {
        // `[5m]` belongs to `rate(...)`; `[10s]` is outside and unrelated.
        let expr = r#"sum(rate(x[5m])) + outside[10s]"#;
        assert_eq!(find_qualifying_expr_window(expr), Some(300));
    }

    #[test]
    fn validate_severity_pass_fail() {
        assert!(validate_severity(Some(&"page".to_string())).is_ok());
        assert!(validate_severity(Some(&"warning".to_string())).is_ok());
        assert!(validate_severity(Some(&"info".to_string())).is_ok());
        assert!(validate_severity(Some(&"critical".to_string())).is_err());
        assert!(validate_severity(None).is_err());
        assert!(validate_severity(Some(&"".to_string())).is_err());
    }

    #[test]
    fn validate_for_pass_below_floor_with_window() {
        // for=5s below 30s floor, but rate(...[5m]) saves it.
        assert!(validate_for_field(
            Some(&"5s".to_string()),
            Some(&"sum(rate(x[5m]))".to_string())
        )
        .is_ok());
    }

    #[test]
    fn validate_for_fail_below_floor_no_window() {
        assert!(validate_for_field(Some(&"5s".to_string()), Some(&"sum(x)".to_string())).is_err());
    }

    #[test]
    fn check_hygiene_redacts_templates() {
        // Bearer {{ $labels.x }} is legitimate templating — must NOT trip.
        assert!(check_hygiene("Bearer {{ $labels.token }}").is_none());
        // Real bearer token MUST trip.
        assert!(check_hygiene("Bearer abc123def456ghi789").is_some());
    }

    #[test]
    fn check_hygiene_ipv4_allowlist() {
        assert!(check_hygiene("see 127.0.0.1 here").is_none());
        assert!(check_hygiene("see 10.0.5.7 here").is_some());
    }
}
