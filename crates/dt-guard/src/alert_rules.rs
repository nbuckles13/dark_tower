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
pub const RULE_FILE_LOADING_RULE_ID: &str = "rule_file_loading";
pub const INVENTORY_EXPR_DRIFT_RULE_ID: &str = "inventory_expr_drift";

/// Operator-facing alert inventory. Every `#### <AlertName>` heading here that
/// names a real rule must restate that rule's `expr` and `for:` exactly.
const ALERT_INVENTORY: &str = "docs/observability/alerts.md";

/// Where the Prometheus that actually runs reads its config from.
/// `docs/observability/dashboard-conventions.md` makes this one authoritative;
/// `infra/docker/prometheus/prometheus.yml` is the local-compose copy.
const PROMETHEUS_CONFIGS: &[&str] = &[
    "infra/kubernetes/observability/prometheus.yml",
    "infra/docker/prometheus/prometheus.yml",
];

/// The kustomization whose `configMapGenerator` decides which rules files are
/// mounted into the cluster Prometheus. Separate from the observability
/// kustomization because kustomize refuses file sources outside its own
/// directory.
const RULES_KUSTOMIZATION: &str = "infra/docker/prometheus/kustomization.yaml";

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

// -----------------------------------------------------------------------------
// Rule-file LOADING coverage (`rule_file_loading`)
// -----------------------------------------------------------------------------
//
// # What this closes, and why lint-cleanliness never touched it
//
// Until 2026-09-09 nothing in this tree evaluated an alert rule. `rule_files:`
// was commented out in the compose config and absent from the cluster config,
// so every rule ever authored was a file no process read — while this very
// guard reported `STATUS=OK REASON=alert-rules-clean-4-files`. That green was
// **correct for what it measured** (rule *syntax*) and said nothing about
// whether anything loads them. A guard that lints files nobody reads is the
// "alive, never applied" shape, one layer out from the code it validates.
//
// # One predicate, three consumers, and only one of them can derive it
//
// [`loadable_rules_files`] is the single home for *which files Prometheus must
// load*. Three things have to agree with it:
//
// 1. the `rule_files:` glob in the cluster config,
// 2. the `rule_files:` glob in the compose config,
// 3. the `configMapGenerator` `files:` enumeration that decides what is
//    actually mounted in-cluster.
//
// None of the three can call this function — two are Prometheus config files
// and one is kustomize input — so this check **reads each of them and asserts
// set equality**, rather than restating the predicate a fourth time. Kustomize
// in particular *cannot glob*: `files: [rules/*-alerts.yaml]` fails with an
// evalsymlink error on the literal path, so consumer 3 is unavoidably an
// enumeration, and a new rules file that is inside the glob but missing from
// that list is never mounted and never loads.
//
// # The direction is named in the message, because the remedies are opposite
//
// * **linted but never loaded** — a rules file this guard validates that no
//   config loads. Costs **detection**: the alert is authored, reviewed, green
//   in CI, and cannot fire.
// * **loaded but never linted** — a file Prometheus loads that this guard skips.
//   Costs **disclosure as well as detection**: its `runbook_url` is unresolved,
//   its severity unchecked, and — the part that is easy to miss — its
//   annotations have never been through the hygiene secret scan, yet rule
//   evaluation serves every annotation on `/api/v1/rules`, which on the dev
//   cluster is a NodePort mapped to a host port. Unscanned annotation text
//   becomes readable off-host.
//
// "Sets differ" would be true and useless. Which side is short determines who
// is called.

/// **The** predicate for *which files in the rules directory Prometheus loads*.
///
/// Returns bare filenames, sorted. Two exclusions, both deliberate:
///
/// * **not `_`-prefixed** — `_template-service-alerts.yaml` is a starter of
///   `<svc>` placeholders. Its exprs are not parseable PromQL, and Prometheus
///   validates rule files at config load and **exits non-zero**, which
///   in-cluster is CrashLoopBackOff and takes whole-cluster bring-up with it.
///   It is additionally guard-*exempt* by design, so loading it would evaluate
///   and publish annotation text nothing has ever scanned.
/// * **`-alerts.yaml` suffix** — a recording-rules file would want a different
///   glob and a different review; nothing has needed one yet, and admitting it
///   silently is how the set stops meaning anything.
///
/// This is intentionally **narrower** than the set [`run`] lints, which is
/// every `*.yaml`/`*.yml` minus `_template-`. The gap is real and it is
/// reported: [`check_rule_file_loading`] compares the **linted** set against
/// this one and flags every file that falls in between — `Foo-alerts.yaml`,
/// `mc-recording-rules.yaml`, `mh-media-alerts.yml`. Each of those is a file
/// full of live alert rules that this guard lints, passes clean, and Prometheus
/// never loads: the "alive, never applied" defect, occurring inside the guard
/// built to close it.
///
/// **An earlier version of this comment claimed that asymmetry was reported
/// when only half of it was.** `Foo-alerts.yaml` was caught (an uppercase name
/// is outside the glob *and* outside this predicate, so the glob-coverage loop
/// saw it); `mc-recording-rules.yaml` was not, because a file that is not in
/// this predicate is not in `expected` and was therefore never asked about.
/// The comment named the uncaught example. Found at review, and it is the same
/// class as every other overclaim in this changeset — a sentence asserting a
/// protection that did not exist.
fn loadable_rules_files(alerts_dir: &Path) -> Result<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    if !alerts_dir.is_dir() {
        return Ok(out);
    }
    for entry in std::fs::read_dir(alerts_dir).context("read alerts dir for loading coverage")? {
        let entry = entry.context("read alerts dir entry")?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if name.starts_with('_') {
            continue;
        }
        if !name.ends_with("-alerts.yaml") {
            continue;
        }
        out.push(name);
    }
    out.sort();
    Ok(out)
}

/// Expand one Prometheus `rule_files` glob against the rules directory.
///
/// Only the basename pattern is honoured — the directory component is checked
/// separately, since a glob pointing at the wrong directory is its own finding
/// and a silent empty expansion would read identically to "nothing to load".
/// (The commented-out stub this replaced said `alerts/*.yml`: wrong directory
/// *and* wrong extension, so uncommenting it verbatim would have loaded nothing
/// and looked fixed.)
fn expand_rule_glob(glob: &str, present: &[String]) -> Vec<String> {
    let pattern = glob.rsplit('/').next().unwrap_or(glob);
    present
        .iter()
        .filter(|name| glob_matches(pattern, name))
        .cloned()
        .collect()
}

/// Go `filepath.Match` subset: `*`, `?`, and `[a-z]`-style classes.
///
/// Deliberately not a dependency. Prometheus uses Go's matcher and the only
/// patterns this repo has ever used are this subset; a general glob crate would
/// bring semantics Prometheus does not have, which is a worse kind of wrong
/// than a small matcher whose limits are stated.
fn glob_matches(pattern: &str, name: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let n: Vec<char> = name.chars().collect();
    // NO INDEXING OR RANGE-SLICING ANYWHERE BELOW — `split_first` and `get`
    // only. This is index arithmetic over two character vectors with a
    // three-element lookahead for `a-z` ranges, which is exactly the shape
    // where an off-by-one is plausible and where a separate bounds check that
    // must agree with a later access is the weaker construction. Every access
    // here IS its own bound: there is no `i + 2 < len` fact that a subsequent
    // `class[i + 2]` has to stay consistent with.
    fn go(p: &[char], n: &[char]) -> bool {
        let Some((head, p_rest)) = p.split_first() else {
            return n.is_empty();
        };
        match *head {
            '*' => (0..=n.len()).any(|i| n.get(i..).is_some_and(|tail| go(p_rest, tail))),
            '?' => n
                .split_first()
                .is_some_and(|(_, n_rest)| go(p_rest, n_rest)),
            '[' => {
                let Some(close) = p.iter().position(|c| *c == ']') else {
                    return false;
                };
                let (Some(class), Some(p_after)) = (p.get(1..close), p.get(close + 1..)) else {
                    return false;
                };
                let Some((&ch, n_rest)) = n.split_first() else {
                    return false;
                };
                let (negated, class) = match class.split_first() {
                    Some((&('^' | '!'), rest)) => (true, rest),
                    _ => (false, class),
                };
                let mut hit = false;
                let mut i = 0usize;
                while let Some(&lo) = class.get(i) {
                    // A range is three cells — `lo`, `-`, `hi`. Asking for all
                    // three at once is the bound; there is no separate length
                    // test to keep in step with it.
                    match (class.get(i + 1), class.get(i + 2)) {
                        (Some(&'-'), Some(&hi)) => {
                            if (lo..=hi).contains(&ch) {
                                hit = true;
                            }
                            i += 3;
                        }
                        _ => {
                            if ch == lo {
                                hit = true;
                            }
                            i += 1;
                        }
                    }
                }
                (hit != negated) && go(p_after, n_rest)
            }
            c => n
                .split_first()
                .is_some_and(|(&nh, n_rest)| nh == c && go(p_rest, n_rest)),
        }
    }
    go(&p, &n)
}

/// Pull the `rule_files:` entries out of a Prometheus config.
///
/// Line-oriented on purpose and narrowly: the block is `rule_files:` followed
/// by `  - "<glob>"` bullets, and a *commented* `# rule_files:` must NOT count
/// — that state is the whole defect being fixed, so a parser that accepted it
/// would report the fix as already present.
fn extract_rule_file_globs(content: &str) -> Vec<String> {
    let mut globs: Vec<String> = Vec::new();
    let mut in_block = false;
    for line in content.lines() {
        if line.starts_with('#') {
            continue;
        }
        if line.trim_start().starts_with('#') && !in_block {
            continue;
        }
        if line.starts_with("rule_files:") {
            in_block = true;
            continue;
        }
        if in_block {
            let t = line.trim();
            if t.is_empty() || t.starts_with('#') {
                continue;
            }
            if let Some(rest) = t.strip_prefix("- ") {
                globs.push(rest.trim().trim_matches('"').trim_matches('\'').to_string());
                continue;
            }
            in_block = false;
        }
    }
    globs
}

/// Filenames the rules kustomization's `configMapGenerator` declares.
///
/// Shares [`crate::kustomize::extract_declared_generator_files`] with R-20's
/// dashboard-coverage walk rather than re-deriving it. That parser carries two
/// repairs earned the hard way — inline-comment stripping, and the `- ` bullet
/// anchor that closed a **fail-open** where a standalone comment naming a path
/// counted as *declaring* it. A second parser over the same file shape would
/// have re-earned both, and the fail-open direction is the dangerous one here:
/// it would report a rules file as mounted on the strength of a comment.
fn kustomize_declared_rules(content: &str) -> Vec<String> {
    let mut declared = crate::kustomize::extract_declared_generator_files(content, "-alerts.yaml");
    declared.sort();
    declared.dedup();
    declared
}

/// Set equality between [`loadable_rules_files`] and every consumer that
/// decides loading. Returns human-readable findings; empty means agreement.
fn check_rule_file_loading(
    repo_root: &Path,
    alerts_dir: &Path,
    linted: &[String],
) -> Result<Vec<String>> {
    let mut msgs: Vec<String> = Vec::new();
    let expected = loadable_rules_files(alerts_dir)?;

    // THE GAP BETWEEN WHAT THIS GUARD LINTS AND WHAT PROMETHEUS LOADS.
    // `run()` discovers `*.yaml|*.yml` minus `_template-`; `loadable_rules_files`
    // additionally requires a lowercase first character and an `-alerts.yaml`
    // suffix. Anything in between is a file full of live alert rules that is
    // reviewed, linted, green in CI — and that no `rule_files` glob will ever
    // match. `.yml` is the realistic case rather than a contrived one, because
    // `run()` accepts it explicitly, which reads as a sanctioned extension.
    //
    // Checked FIRST and independently of the config loops below, so it is
    // reported even when a Prometheus config is missing or unreadable.
    for name in linted {
        if !expected.contains(name) {
            msgs.push(format!(
                "{name} is LINTED BUT NEVER LOADED — it is inside this guard's discovery set but \
                 outside the loadable-rules predicate (`loadable_rules_files`), so NO `rule_files` \
                 glob will ever match it and its alerts cannot fire. Rename it to \
                 `<svc>-alerts.yaml` (lowercase first character, `-alerts.yaml` suffix), or move it \
                 out of {ALERTS_SUBDIR} if it is not a rules file."
            ));
        }
    }

    // POSITIVE CONTROL / ANTI-VACUITY. An empty expected set makes every
    // "expected ⊆ covered" comparison below trivially true, so a wholesale
    // failure to see the rules directory would read exactly like agreement.
    // Distinct message on purpose: "found no rules files" and "a rules file is
    // not loaded" have different causes and get triaged differently.
    if expected.is_empty() {
        msgs.push(format!(
            "no loadable rules files found under {ALERTS_SUBDIR} — expected at least one \
             `<svc>-alerts.yaml`. This is a COULD-NOT-EVALUATE, not a clean result: with an \
             empty expected set every coverage comparison below passes vacuously."
        ));
        return Ok(msgs);
    }

    for cfg_rel in PROMETHEUS_CONFIGS {
        let cfg_path = repo_root.join(cfg_rel);
        if !cfg_path.is_file() {
            msgs.push(format!(
                "{cfg_rel}: Prometheus config not found; cannot verify that alert rules are loaded"
            ));
            continue;
        }
        let content = std::fs::read_to_string(&cfg_path)
            .with_context(|| format!("read Prometheus config {cfg_rel}"))?;
        let globs = extract_rule_file_globs(&content);
        if globs.is_empty() {
            msgs.push(format!(
                "{cfg_rel}: no active `rule_files:` entry — every alert rule in {ALERTS_SUBDIR} is \
                 a file this Prometheus never reads (a commented-out stanza does not count)"
            ));
            continue;
        }
        for glob in &globs {
            let dir = glob.rsplit_once('/').map_or("", |(d, _)| d);
            if dir != "rules" {
                msgs.push(format!(
                    "{cfg_rel}: `rule_files` glob {glob:?} does not point at the mounted rules \
                     directory (expected a `rules/` prefix; both deployments mount \
                     {ALERTS_SUBDIR} at /etc/prometheus/rules)"
                ));
            }
        }
        let covered: Vec<String> = {
            let mut c: Vec<String> = globs
                .iter()
                .flat_map(|g| expand_rule_glob(g, &expected))
                .collect();
            c.sort();
            c.dedup();
            c
        };
        for name in &expected {
            if !covered.contains(name) {
                msgs.push(format!(
                    "{cfg_rel}: {name} is LINTED BUT NEVER LOADED — it is not matched by any \
                     `rule_files` glob ({globs:?}), so its alerts are reviewed, green in CI, and \
                     cannot fire. Widen the file set by ADDING the file to the predicate's \
                     conditions, not by widening the glob to `*-alerts.yaml` (that re-admits \
                     `_template-service-alerts.yaml`, whose placeholder PromQL crashes Prometheus \
                     at config load and whose annotations have never been hygiene-scanned)."
                ));
            }
        }
        // The other direction, over the whole rules directory rather than the
        // loadable subset: a file the glob picks up that this guard's linting
        // walk skips is an unvalidated rule going live.
        if let Ok(entries) = std::fs::read_dir(alerts_dir) {
            for entry in entries.flatten() {
                let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                    continue;
                };
                let matched = globs.iter().any(|g| {
                    glob_matches(g.rsplit('/').next().unwrap_or(g.as_str()), name.as_str())
                });
                if matched && !expected.contains(&name) {
                    msgs.push(format!(
                        "{cfg_rel}: {name} is LOADED BUT NEVER LINTED — a `rule_files` glob \
                         ({globs:?}) matches it but it is outside the loadable-rules predicate, so \
                         it gets no runbook_url resolution, no severity check, and NO \
                         ANNOTATION-HYGIENE SECRET SCAN. Rule evaluation serves every annotation \
                         on /api/v1/rules, so unscanned text becomes readable to anything that can \
                         reach the Prometheus port."
                    ));
                }
            }
        }
    }

    // Consumer 3: what is actually mounted in-cluster.
    let kust_path = repo_root.join(RULES_KUSTOMIZATION);
    if !kust_path.is_file() {
        msgs.push(format!(
            "{RULES_KUSTOMIZATION}: not found — nothing generates the in-cluster rules ConfigMap, \
             so the cluster Prometheus mounts no rules regardless of its `rule_files` glob"
        ));
    } else {
        let content = std::fs::read_to_string(&kust_path)
            .with_context(|| format!("read {RULES_KUSTOMIZATION}"))?;
        let declared = kustomize_declared_rules(&content);
        for name in &expected {
            if !declared.contains(name) {
                msgs.push(format!(
                    "{RULES_KUSTOMIZATION}: {name} is not in the `configMapGenerator` file list, so \
                     it is NEVER MOUNTED in-cluster and never loads — even though the `rule_files` \
                     glob matches it. Kustomize cannot glob, so this list is an enumeration and a \
                     new file has to be added by hand."
                ));
            }
        }
        for name in &declared {
            if !expected.contains(name) {
                msgs.push(format!(
                    "{RULES_KUSTOMIZATION}: declares {name}, which is not a loadable rules file on \
                     disk — the generated ConfigMap would fail to build, or would mount a file the \
                     glob ignores"
                ));
            }
        }
    }

    Ok(msgs)
}

// -----------------------------------------------------------------------------
// Inventory ↔ rules byte-identity (`inventory_expr_drift`)
// -----------------------------------------------------------------------------
//
// # The mechanism, which is sharper than "someone forgot"
//
// `docs/observability/alerts.md` restates each alert's PromQL for an operator
// who is reading a catalog rather than a YAML tree. That is a second encoding
// of one value, and it drifted — but not through neglect. Commit `d3c30f10`
// ("Fix Prometheus alert selectors that never matched a pod") corrected
// selectors in the RULES files. It was a correct and valuable change; its title
// names ADR-0036 §11's canonical *alive, never applied* instance verbatim. The
// inventory sat three directories away and was not in that diff, so **no review
// could have caught it**: the second encoding drifted *because* someone improved
// the first.
//
// The defect set the mechanism predicts is exactly the set observed. `d3c30f10`
// touched gc, mc and mh rules alike, but GC's are the only inventory entries
// covering *resource* alerts — MC's are all join-flow and were untouched, and
// match byte-for-byte. A causal account that predicts which entries drifted is
// worth more here than one that explains it afterwards, because it says the
// class will recur: the incentive to keep improving rules files is permanent.
//
// # One direction only, deliberately
//
// This checks *heading → rule*: every `#### <Name>` that names a real alert must
// restate it exactly. The *coverage* direction — every rule must have a heading —
// stays deferred, because landing it means first backfilling 26 MC/MH entries,
// which is task-sized. See `docs/TODO.md`.
//
// Note the checked set derives from the inventory's own headings, so there is no
// allow-list and no second membership set to drift — the same principle applied
// to `loadable_rules_files` above.
//
// # ONE CANONICAL SHAPE. NO NORMALISE-ON-READ.
//
// The `for:` line sits AFTER the closing fence, so the fenced block is valid
// PromQL that an operator can paste into Prometheus mid-incident. A `for:`
// inside the fence makes the block a parse error, defeating the block's only
// operational purpose. This parser **fails on the other shape rather than
// accepting it**: a tolerance here is what would later hide real drift, and the
// hazard is not hypothetical — a `for:`-stripping step in a throwaway
// measurement script measured the selector drift correctly while concealing the
// shape defect entirely. If someone later wants tolerance, they change the
// shape, not the parser.
//
// Comparison is over LINE VECTORS, not trimmed strings. A YAML block scalar
// always ends in a newline and a fenced block's content does not; comparing
// `lines()` reconciles that representation difference **without** a trim step
// that could mask a real trailing-whitespace divergence.

/// One inventory finding: `(line, alert name, message)`.
type InventoryFinding = (usize, String, String);

/// `check_inventory_drift`'s result: the findings, and the number of
/// heading↔rule pairs actually compared. The count is not decoration — it goes
/// into the OK reason token so a silently-narrowed checked set is visible.
type InventoryDriftResult = (Vec<InventoryFinding>, usize);

/// The rule index the inventory check reads: alert name → (expr, `for`, source file).
type RuleIndex = HashMap<String, (String, String, String)>;

#[derive(Debug)]
struct InventoryEntry {
    alert: String,
    line: usize,
    /// `None` = the entry has no ```promql block at all. Distinct from
    /// `Some(vec![])`, and the distinction is fixture 1's whole subject.
    promql: Option<Vec<String>>,
    for_line: Option<String>,
    for_inside_fence: bool,
}

/// Parse `#### <AlertName>` sections and the PromQL block each one owns.
///
/// # Four independent extraction properties
///
/// They defend disjoint failures. Written as one "bound entries correctly"
/// instruction, one gets implemented and the rest assumed — which is what
/// happened to two separate measurement scripts during this change's review,
/// each of which had two of these by accident and lacked the others.
///
/// 1. **First fence per entry wins.** Stops an entry that *has* a block from
///    having it overwritten by a later one in the same section (the Response
///    list sometimes carries a second query).
/// 2. **ANY heading resets entry context, not just `####`.** An `h2` or `h3`
///    must stop the scan. This defends the *other* direction — an entry with
///    **no** block absorbing a foreign one from the next section — and
///    property 1 structurally cannot help there, because the first block it
///    reaches *is* the foreign one. Latent today (all live entries have exactly
///    one fence) and it goes live on the deferred inventory backfill, whose
///    natural first form is a stub entry naming an alert and pointing at the
///    rules file instead of restating PromQL. It is here *before* that, not
///    added when someone notices a stub reporting a foreign expression.
/// 3. **Fence detection is indentation-tolerant.** An anchored `^```promql`
///    misses a fence inside a list item; this file has one such. A dropped
///    fence means the entry is never compared and the guard reports no drift
///    on something it never looked at.
/// 4. **The canonical shape is enforced, never normalised** — see
///    [`check_inventory_drift`].
fn parse_alert_inventory(md: &str) -> Vec<InventoryEntry> {
    let lines: Vec<&str> = md.lines().collect();
    let mut out: Vec<InventoryEntry> = Vec::new();
    // NO INDEXING BELOW — every cursor read goes through `lines.get(..)`, so
    // the bound and the access are ONE operation. The previous shape had them
    // as two facts that had to agree (`while m < lines.len()` followed by
    // `lines[m]`), which is the weaker construction for the same reason this
    // devloop's set-equality assertions exist: two encodings of one condition
    // drift, and here the drift is a panic inside a Layer-3 guard.
    for (idx, line) in lines.iter().enumerate() {
        let Some(name) = line.strip_prefix("#### ") else {
            continue;
        };
        let name = name.trim().to_string();
        let heading_line = idx + 1;
        let mut j = idx + 1;
        let mut promql: Option<Vec<String>> = None;
        let mut for_line: Option<String> = None;
        let mut for_inside_fence = false;
        // PROPERTY 2: any ATX heading ends this entry.
        while let Some(cur) = lines.get(j).filter(|l| !is_atx_heading(l)) {
            // PROPERTY 3: trim before comparing, so an indented fence is found.
            // PROPERTY 1: `promql.is_none()` — first fence wins.
            if cur.trim() == "```promql" && promql.is_none() {
                let mut body: Vec<String> = Vec::new();
                let mut k = j + 1;
                while let Some(fence_line) = lines.get(k).filter(|l| l.trim() != "```") {
                    if fence_line.trim_start().starts_with("for:") {
                        for_inside_fence = true;
                    }
                    body.push((*fence_line).to_string());
                    k += 1;
                }
                promql = Some(body);
                // The `for:` line is the next NON-BLANK line after the fence.
                let mut m = k + 1;
                while lines.get(m).is_some_and(|l| l.trim().is_empty()) {
                    m += 1;
                }
                if let Some(rest) = lines
                    .get(m)
                    .and_then(|l| l.trim().strip_prefix("`for: "))
                    .and_then(|rest| rest.strip_suffix('`'))
                {
                    for_line = Some(rest.to_string());
                }
                j = k;
            }
            j += 1;
        }
        out.push(InventoryEntry {
            alert: name,
            line: heading_line,
            promql,
            for_line,
            for_inside_fence,
        });
    }
    out
}

/// An ATX heading of any level (`#` … `######`).
fn is_atx_heading(line: &str) -> bool {
    let t = line.trim_start();
    let hashes = t.chars().take_while(|c| *c == '#').count();
    (1..=6).contains(&hashes) && t.chars().nth(hashes) == Some(' ')
}

/// Compare every inventory entry against the rule it names.
fn check_inventory_drift(repo_root: &Path, rules: &RuleIndex) -> Result<InventoryDriftResult> {
    let mut msgs: Vec<InventoryFinding> = Vec::new();
    let path = repo_root.join(ALERT_INVENTORY);
    if !path.is_file() {
        return Ok((msgs, 0));
    }
    let md = std::fs::read_to_string(&path).with_context(|| format!("read {ALERT_INVENTORY}"))?;
    let entries = parse_alert_inventory(&md);

    // POSITIVE CONTROL. Every heading in the file resolves to a real rule today,
    // so the orphan branch below would otherwise ship having never run. An empty
    // entry list means the markdown parser saw nothing and every comparison
    // passes vacuously — a different failure from drift, with a different cause.
    if entries.is_empty() {
        msgs.push((
            0,
            "-".to_string(),
            format!(
                "{ALERT_INVENTORY}: no `#### <AlertName>` sections with a ```promql block were \
                 parsed. COULD-NOT-EVALUATE, not clean: with no entries every byte-identity \
                 comparison passes vacuously."
            ),
        ));
        return Ok((msgs, 0));
    }

    // Pairs actually compared — an entry whose heading names a real rule. This
    // is reported in the OK token, and the reason is PARTIAL narrowing rather
    // than total: fixture 5 catches an empty entry set, but if a future edit
    // moved some entries out of `#### ` reach the remainder would still compare
    // clean and the token would be byte-identical to today's. 28 -> 12 is
    // obvious at a glance; without the count it is invisible. Same defect this
    // whole rule_id exists to catch, one level up.
    let mut pairs = 0usize;

    for e in &entries {
        let Some((expr, for_dur, src)) = rules.get(&e.alert) else {
            msgs.push((
                e.line,
                e.alert.clone(),
                format!(
                    "{ALERT_INVENTORY}: `#### {}` documents an alert that exists in NO rules file. \
                     An inventory entry reads as coverage; a responder searching for what covers a \
                     failure mode would find this and stop looking. Delete it, or land the rule.",
                    e.alert
                ),
            ));
            continue;
        };
        let Some(body) = e.promql.as_ref() else {
            msgs.push((
                e.line,
                e.alert.clone(),
                format!(
                    "{ALERT_INVENTORY}: `#### {}` names a real alert but carries NO ```promql block, so there is nothing to compare against {src}. Reported with its own reason rather than skipped: a silently-skipped entry is indistinguishable from a matching one, and this is the shape a backfill stub takes.",
                    e.alert
                ),
            ));
            continue;
        };
        pairs += 1;
        if e.for_inside_fence {
            msgs.push((
                e.line,
                e.alert.clone(),
                format!(
                    "{ALERT_INVENTORY}: `#### {}` puts `for:` INSIDE the ```promql fence, which \
                     makes the block invalid PromQL — pasting it into Prometheus during an incident \
                     is a parse error, and that paste is the block's only purpose. Canonical shape: \
                     expr inside the fence, a bare `` `for: <dur>` `` line after it.",
                    e.alert
                ),
            ));
            continue;
        }
        // COMPARISON POLICY, STATED HERE BECAUSE IT IS OTHERWISE DECIDED BY
        // ACCIDENT. "Byte-identical" is underdetermined at exactly one
        // character: a YAML `expr: |` scalar always ends in `\n`, a markdown
        // fence's content does not, so a literal byte compare is impossible and
        // *some* decision gets made whether or not anyone writes it down. Left
        // unwritten the natural reach is `.trim()`, which also swallows
        // indentation and trailing spaces — the seam a `for:`-stripping step in
        // a throwaway measurement script lived in during this change's review,
        // measuring the selector drift correctly while concealing a shape defect
        // entirely.
        //
        // POLICY: LINE-BASED EXTRACTION ON BOTH SIDES, COMPARE THE LINE
        // SEQUENCES, ZERO NORMALISATION. No trim, strip, replace, trailing-
        // whitespace tolerance, or dedent beyond what YAML's own block scalar
        // performs. This DISSOLVES the trailing-newline question rather than
        // answering it. It is empirically achievable, not aspirational: the
        // inventory is at full parity under this exact policy today.
        let expected: Vec<&str> = expr.lines().collect();
        let actual: Vec<&str> = body.iter().map(String::as_str).collect();
        if expected != actual {
            msgs.push((
                e.line,
                e.alert.clone(),
                format!(
                    "{ALERT_INVENTORY}: `#### {}` PromQL is not byte-identical to `expr:` in {src}. \
                     A responder pasting the documented query mid-incident evaluates something \
                     other than what fired.\n    rules: {expected:?}\n    docs:  {actual:?}",
                    e.alert
                ),
            ));
        }
        match e.for_line.as_deref() {
            Some(d) if d == for_dur => {}
            Some(d) => msgs.push((
                e.line,
                e.alert.clone(),
                format!(
                    "{ALERT_INVENTORY}: `#### {}` documents `for: {d}` but {src} says `for: {for_dur}`",
                    e.alert
                ),
            )),
            None => msgs.push((
                e.line,
                e.alert.clone(),
                format!(
                    "{ALERT_INVENTORY}: `#### {}` has a ```promql block with no `` `for: <dur>` `` \
                     line after it",
                    e.alert
                ),
            )),
        }
    }
    Ok((msgs, pairs))
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

    // LOADING coverage runs even when there are no lintable rule files: "there
    // are no rules" and "the rules are not loaded" are different failures and
    // the second must not be masked by the first.
    let mut all_findings: Vec<Finding> = Vec::new();
    // (alert name) -> (expr, for, source file). Fed by the walk below and
    // consumed by the inventory byte-identity check afterwards.
    let mut rule_index: RuleIndex = HashMap::new();
    let linted_names: Vec<String> = yaml_files
        .iter()
        .filter_map(|p| p.file_name().and_then(|n| n.to_str()).map(str::to_owned))
        .collect();
    for msg in check_rule_file_loading(repo_root, &alerts_dir, &linted_names)? {
        all_findings.push(Finding {
            file: ALERTS_SUBDIR.to_string(),
            alert: "-".to_string(),
            line: 0,
            rule_id: RULE_FILE_LOADING_RULE_ID,
            message: msg,
            secret_pattern: None,
        });
    }

    if yaml_files.is_empty() && all_findings.is_empty() {
        emit_ok("alert-rules-no-files");
        return Ok(());
    }
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

                if let (Some(expr_s), Some(for_s)) = (
                    yml_str(rule.expr.as_ref()),
                    yml_str(rule.for_duration.as_ref()),
                ) {
                    rule_index.insert(alert_name.clone(), (expr_s, for_s, rel_path.clone()));
                }

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

    let (inventory_msgs, inventory_pairs) = check_inventory_drift(repo_root, &rule_index)?;
    for (line, alert, msg) in inventory_msgs {
        all_findings.push(Finding {
            file: ALERT_INVENTORY.to_string(),
            alert,
            line,
            rule_id: INVENTORY_EXPR_DRIFT_RULE_ID,
            message: msg,
            secret_pattern: None,
        });
    }

    if all_findings.is_empty() {
        // THE REASON TOKEN NAMES *LOADING*, NOT FIRING, AND THAT IS THE POINT.
        // The old token was a bare `alert-rules-clean-N-files`, and
        // `docs/TODO.md` recorded the cost of it: this guard reported clean
        // while `rule_files:` was commented out and nothing in the tree
        // evaluated a single rule, and its verdict was indistinguishable from a
        // genuine pass. Naming the covered count *and the scope* is the
        // `grafana_datasources.rs` remedy applied here.
        //
        // It stops at "loaded". A rule can be loaded, evaluating, and
        // INCAPABLE OF EVER MATCHING — `MCMediaMissingKeyMaterial` is exactly
        // that today, because `dt_client_*` metrics reach no Prometheus (the
        // OTLP collector's metrics pipeline exports to `debug` and nothing
        // scrapes it). This token must never be read as "the alerts fire".
        // ALL THREE SCOPES NAMED, NOT TWO. Files linted, loadable files
        // covered, and inventory pairs compared. The third was missing in the
        // first version of this token, which is the same omission the comment
        // above describes: the principle was applied to the loading scope and
        // then not to the inventory one.
        emit_ok(format!(
            "alert-rules-clean-{}-files-{}-loadable-covered-{}-inventory-pairs",
            yaml_files.len(),
            loadable_rules_files(&alerts_dir)?.len(),
            inventory_pairs
        ));
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

    // -------------------------------------------------------------------------
    // rule_file_loading — PROOF OF TRAP PER BRANCH
    //
    // Every branch below is one this guard can only ever exercise on synthetic
    // input: in-tree the check passes, so a test suite that only ran against
    // the real repo would ship all of these having never executed once. That is
    // the failure mode this whole rule_id exists to catch, so not reproducing it
    // inside its own tests is not optional.
    // -------------------------------------------------------------------------

    fn tmp_rules_dir(names: &[&str]) -> tempfile::TempDir {
        let d = tempfile::tempdir().expect("tempdir");
        for n in names {
            std::fs::write(d.path().join(n), "groups: []\n").expect("write fixture");
        }
        d
    }

    #[test]
    fn loadable_predicate_excludes_underscore_and_non_alerts() {
        let d = tmp_rules_dir(&[
            "mc-alerts.yaml",
            "_template-service-alerts.yaml",
            "mc-recording-rules.yaml",
            "notes.md",
        ]);
        assert_eq!(
            loadable_rules_files(d.path()).expect("predicate"),
            vec!["mc-alerts.yaml".to_string()]
        );
    }

    /// The gap between what `run()` LINTS and what Prometheus LOADS. Found at
    /// review, where three probes carrying real alert content were dropped into
    /// the rules directory: `Foo-alerts.yaml` failed (it is outside the glob),
    /// but `mc-recording-rules.yaml` and `zz-probe-alerts.yml` both reported
    /// `STATUS=OK` — files full of live alert rules that this guard lints,
    /// passes clean, and Prometheus never loads. `.yml` is the realistic case,
    /// because `run()` accepts that extension explicitly.
    ///
    /// Note `_experimental-alerts.yaml` is covered by the same loop: it is
    /// linted (the lint walk skips only `_template-`) and excluded from the
    /// predicate, and neither the glob-coverage direction nor the
    /// glob-matched-but-unlinted direction would have seen it.
    #[test]
    fn linted_but_outside_the_loadable_predicate_is_reported() {
        let d = tmp_rules_dir(&["mc-alerts.yaml"]);
        let repo = tempfile::tempdir().expect("tempdir");
        for name in [
            "mc-recording-rules.yaml",
            "zz-probe-alerts.yml",
            "_experimental-alerts.yaml",
        ] {
            let linted = vec!["mc-alerts.yaml".to_string(), (*name).to_string()];
            let msgs = check_rule_file_loading(repo.path(), d.path(), &linted).expect("check");
            assert!(
                msgs.iter()
                    .any(|m| m.contains(name) && m.contains("LINTED BUT NEVER LOADED")),
                "{name} must be reported; got {msgs:?}"
            );
        }
        // Control: a file that IS loadable produces no such finding, so the
        // assertion above is not satisfied by the check firing on everything.
        let linted = vec!["mc-alerts.yaml".to_string()];
        let msgs = check_rule_file_loading(repo.path(), d.path(), &linted).expect("check");
        assert!(
            !msgs.iter().any(|m| m.contains("mc-alerts.yaml is LINTED")),
            "{msgs:?}"
        );
    }

    // -------------------------------------------------------------------------
    // COMPOSED-FUNCTION proof-of-trap for `check_rule_file_loading`.
    //
    // Its components are unit-tested above, but component coverage does not
    // cover COMPOSITION: a transposed set-difference (`covered.contains` where
    // `expected.contains` was meant) leaves every component correct and the
    // whole function silently wrong, and in-tree the function only ever runs its
    // clean path. Each fixture asserts the SPECIFIC reason phrase, so a branch
    // that fires for the wrong cause does not pass as coverage.
    // -------------------------------------------------------------------------

    /// Write a Prometheus config, a kustomization, and run the composed check.
    fn run_loading_check(
        rules: &[&str],
        linted: &[&str],
        glob: Option<&str>,
        declared: &[&str],
    ) -> Vec<String> {
        let repo = tempfile::tempdir().expect("tempdir");
        let alerts_dir = repo.path().join(ALERTS_SUBDIR);
        std::fs::create_dir_all(&alerts_dir).expect("mkdir rules");
        for r in rules {
            std::fs::write(alerts_dir.join(r), "groups: []\n").expect("write rule");
        }
        let body = match glob {
            Some(g) => format!("rule_files:\n  - \"{g}\"\n\nscrape_configs: []\n"),
            None => "# rule_files:\n#   - \"alerts/*.yml\"\n\nscrape_configs: []\n".to_string(),
        };
        for cfg in PROMETHEUS_CONFIGS {
            let path = repo.path().join(cfg);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir cfg");
            std::fs::write(&path, &body).expect("write cfg");
        }
        let kust = repo.path().join(RULES_KUSTOMIZATION);
        std::fs::create_dir_all(kust.parent().expect("parent")).expect("mkdir kust");
        let mut k = String::from("configMapGenerator:\n  - name: prometheus-rules\n    files:\n");
        for d in declared {
            k.push_str(&format!("      - rules/{d}\n"));
        }
        std::fs::write(&kust, k).expect("write kust");
        let linted: Vec<String> = linted.iter().map(|s| (*s).to_string()).collect();
        check_rule_file_loading(repo.path(), &alerts_dir, &linted).expect("check")
    }

    /// (a) Empty expected set — its OWN token, never "0 drifts".
    #[test]
    fn composed_empty_expected_is_could_not_evaluate() {
        let msgs = run_loading_check(&[], &[], Some("rules/[a-z]*-alerts.yaml"), &[]);
        assert!(
            msgs.iter()
                .any(|m| m.contains("COULD-NOT-EVALUATE") && m.contains("vacuously")),
            "{msgs:?}"
        );
    }

    /// (b) A loadable rules file outside the glob — LINTED BUT NEVER LOADED.
    #[test]
    fn composed_expected_not_covered_by_glob() {
        let msgs = run_loading_check(
            &["mc-alerts.yaml"],
            &["mc-alerts.yaml"],
            Some("rules/zz*-alerts.yaml"),
            &["mc-alerts.yaml"],
        );
        assert!(
            msgs.iter()
                .any(|m| m.contains("mc-alerts.yaml is LINTED BUT NEVER LOADED")
                    && m.contains("not matched by any `rule_files` glob")),
            "{msgs:?}"
        );
    }

    /// (b2) No ACTIVE `rule_files` at all — the pre-2026-09-09 state. A
    /// commented-out stanza must not read as configured.
    #[test]
    fn composed_commented_out_rule_files_is_reported() {
        let msgs = run_loading_check(
            &["mc-alerts.yaml"],
            &["mc-alerts.yaml"],
            None,
            &["mc-alerts.yaml"],
        );
        assert!(
            msgs.iter()
                .any(|m| m.contains("no active `rule_files:` entry")),
            "{msgs:?}"
        );
    }

    /// (c) Glob-matched but outside the linted set — LOADED BUT NEVER LINTED,
    /// the DISCLOSURE direction: unscanned annotations served on /api/v1/rules.
    #[test]
    fn composed_loaded_but_never_linted() {
        let msgs = run_loading_check(
            &["mc-alerts.yaml", "_template-service-alerts.yaml"],
            &["mc-alerts.yaml"],
            Some("rules/*-alerts.yaml"),
            &["mc-alerts.yaml"],
        );
        assert!(
            msgs.iter().any(|m| m
                .contains("_template-service-alerts.yaml is LOADED BUT NEVER LINTED")
                && m.contains("ANNOTATION-HYGIENE")),
            "{msgs:?}"
        );
    }

    /// (d) A loadable file missing from the kustomization — NEVER MOUNTED.
    #[test]
    fn composed_missing_from_configmap_generator() {
        let msgs = run_loading_check(
            &["mc-alerts.yaml"],
            &["mc-alerts.yaml"],
            Some("rules/[a-z]*-alerts.yaml"),
            &[],
        );
        assert!(
            msgs.iter().any(|m| m.contains("NEVER MOUNTED in-cluster")),
            "{msgs:?}"
        );
    }

    /// (e) The kustomization declares a file that is not on disk.
    #[test]
    fn composed_declared_but_absent_on_disk() {
        let msgs = run_loading_check(
            &["mc-alerts.yaml"],
            &["mc-alerts.yaml"],
            Some("rules/[a-z]*-alerts.yaml"),
            &["mc-alerts.yaml", "ghost-alerts.yaml"],
        );
        assert!(
            msgs.iter()
                .any(|m| m.contains("declares ghost-alerts.yaml")),
            "{msgs:?}"
        );
    }

    /// (f) THE CLEAN PATH — everything agreeing yields NO findings. Without
    /// this the fixtures above are satisfied by a check that fires on
    /// everything, which is the mirror of the vacuity they exist to prevent.
    #[test]
    fn composed_clean_path_is_silent() {
        let msgs = run_loading_check(
            &["mc-alerts.yaml", "mh-alerts.yaml"],
            &["mc-alerts.yaml", "mh-alerts.yaml"],
            Some("rules/[a-z]*-alerts.yaml"),
            &["mc-alerts.yaml", "mh-alerts.yaml"],
        );
        assert!(msgs.is_empty(), "{msgs:?}");
    }

    #[test]
    fn loadable_predicate_positive_control_non_empty() {
        // Guards the guard: if the walk silently saw nothing, every coverage
        // comparison downstream would pass vacuously.
        let d = tmp_rules_dir(&["gc-alerts.yaml", "mh-alerts.yaml"]);
        assert_eq!(loadable_rules_files(d.path()).expect("predicate").len(), 2);
    }

    #[test]
    fn glob_matches_go_subset() {
        assert!(glob_matches("[a-z]*-alerts.yaml", "mc-alerts.yaml"));
        // The whole reason for the character class: the template must NOT match.
        assert!(!glob_matches(
            "[a-z]*-alerts.yaml",
            "_template-service-alerts.yaml"
        ));
        // ...whereas the naive glob does, which is the trap.
        assert!(glob_matches(
            "*-alerts.yaml",
            "_template-service-alerts.yaml"
        ));
        assert!(!glob_matches("[a-z]*-alerts.yaml", "Foo-alerts.yaml"));
        assert!(!glob_matches(
            "[a-z]*-alerts.yaml",
            "mc-recording-rules.yaml"
        ));
    }

    #[test]
    fn commented_out_rule_files_is_not_an_active_glob() {
        // The exact pre-2026-09-09 state. A parser that accepted it would report
        // the defect as already fixed.
        let cfg = "global:\n  scrape_interval: 15s\n# rule_files:\n#   - \"alerts/*.yml\"\n";
        assert!(extract_rule_file_globs(cfg).is_empty());
    }

    #[test]
    fn active_rule_files_glob_is_extracted() {
        let cfg = "rule_files:\n  - \"rules/[a-z]*-alerts.yaml\"\n\nscrape_configs: []\n";
        assert_eq!(
            extract_rule_file_globs(cfg),
            vec!["rules/[a-z]*-alerts.yaml".to_string()]
        );
    }

    #[test]
    fn kustomize_declared_rules_ignores_standalone_comments() {
        // The fail-open direction inherited from R-20: a comment NAMING a path
        // must not count as declaring it, or a file is reported as mounted on
        // the strength of prose.
        let k = concat!(
            "configMapGenerator:\n",
            "  - name: prometheus-rules\n",
            "    files:\n",
            "      # rules/mh-alerts.yaml is deliberately absent\n",
            "      - rules/mc-alerts.yaml\n",
        );
        assert_eq!(
            kustomize_declared_rules(k),
            vec!["mc-alerts.yaml".to_string()]
        );
    }

    #[test]
    fn kustomize_declared_rules_strips_inline_comments() {
        let k = "    files:\n      - rules/mc-alerts.yaml   # meeting controller\n";
        assert_eq!(
            kustomize_declared_rules(k),
            vec!["mc-alerts.yaml".to_string()]
        );
    }

    // -------------------------------------------------------------------------
    // inventory_expr_drift — SEVEN FIXTURES, NOT ASSERTIONS
    //
    // Markdown-and-YAML extraction is a domain where **reading a pattern does
    // not tell you what it matches**. During this change's review, seven
    // tolerances across two reviewers' measurement scripts passed for the wrong
    // reason, and every one was caught by measuring rather than by reading.
    // That is why these are executable fixtures and not a comment asserting the
    // properties hold.
    //
    // Several arms below have ZERO live instances in `alerts.md` today —
    // entries with no block, entries with a second fence, `for:` inside the
    // fence, and orphan headings. Without a fixture each would ship green
    // having never executed once, which is the assertion-vacuity this whole
    // rule_id exists to catch. Cases 1, 2, 5 and 7 in particular produce
    // plausible, well-formed, clean-looking results when the property is
    // missing.
    // -------------------------------------------------------------------------

    fn rules_map(pairs: &[(&str, &str, &str)]) -> RuleIndex {
        pairs
            .iter()
            .map(|(n, e, f)| {
                (
                    (*n).to_string(),
                    (
                        (*e).to_string(),
                        (*f).to_string(),
                        "fixture.yaml".to_string(),
                    ),
                )
            })
            .collect()
    }

    /// FIXTURE 1 — an entry with NO block, followed by a section that has one.
    /// Property 1 cannot help here: the first fence this entry reaches IS the
    /// foreign one. Only property 2 (any heading resets) prevents the theft.
    #[test]
    fn fixture_1_entry_without_block_does_not_absorb_a_foreign_one() {
        let md = concat!(
            "#### AlertNoBlock
",
            "
",
            "See `mc-alerts.yaml` for the expression.
",
            "
",
            "### Some Other Section
",
            "
",
            "```promql
",
            "up == 0
",
            "```
",
            "`for: 1m`
",
        );
        let entries = parse_alert_inventory(md);
        assert_eq!(entries.len(), 1);
        assert!(
            entries[0].promql.is_none(),
            "the h3 must end the entry before the foreign fence"
        );
    }

    /// FIXTURE 1b — and the missing block is REPORTED, with its own reason,
    /// never silently skipped. A skipped entry is indistinguishable from a
    /// matching one.
    #[test]
    fn fixture_1b_missing_block_is_reported_not_skipped() {
        let md = "#### AlertNoBlock

See the rules file.
";
        let entries = parse_alert_inventory(md);
        assert!(entries[0].promql.is_none());
        // The message the compare emits for this shape is distinct from the
        // drift message; assert on the discriminating phrase.
        let rules = rules_map(&[("AlertNoBlock", "up == 0", "1m")]);
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(tmp.path().join("docs/observability")).expect("mkdir");
        std::fs::write(tmp.path().join(ALERT_INVENTORY), md).expect("write");
        let msgs = check_inventory_drift(tmp.path(), &rules).expect("check").0;
        assert_eq!(msgs.len(), 1);
        assert!(
            msgs[0].2.contains("names a real alert but carries NO"),
            "{}",
            msgs[0].2
        );
    }

    /// FIXTURE 2 — an entry with a SECOND fence keeps the first.
    #[test]
    fn fixture_2_first_fence_per_entry_wins() {
        let md = concat!(
            "#### AlertTwoFences
",
            "```promql
",
            "first_expr > 0
",
            "```
",
            "`for: 5m`
",
            "
",
            "Related query:
",
            "```promql
",
            "second_expr > 0
",
            "```
",
        );
        let e = &parse_alert_inventory(md)[0];
        assert_eq!(
            e.promql.as_deref(),
            Some(&["first_expr > 0".to_string()][..])
        );
    }

    /// FIXTURE 3 — an INDENTED fence (inside a list item) is found. An anchored
    /// `^```promql` drops it, the entry is never compared, and the guard
    /// reports no drift on something it never looked at.
    #[test]
    fn fixture_3_indented_fence_is_found() {
        let md = concat!(
            "#### AlertIndented
",
            "1. Run this:
",
            "   ```promql
",
            "   indented_expr > 0
",
            "   ```
",
            "   `for: 5m`
",
        );
        let e = &parse_alert_inventory(md)[0];
        assert_eq!(
            e.promql.as_deref(),
            Some(&["   indented_expr > 0".to_string()][..])
        );
        assert_eq!(e.for_line.as_deref(), Some("5m"));
    }

    /// FIXTURE 4 — `for:` inside the fence FAILS. It is not normalised away.
    /// A YAML `expr:` scalar never contains `for:`, so a fence holding only the
    /// expression maps 1:1 onto it and the compare needs no stripping step at
    /// all. That is what makes this structural rather than disciplinary — and
    /// discipline demonstrably failed here, in the hands of the people arguing
    /// for the guard.
    #[test]
    fn fixture_4_for_inside_fence_fails_and_is_not_normalised() {
        let md = concat!(
            "#### AlertForInside
",
            "```promql
",
            "up == 0
",
            "for: 1m
",
            "```
",
        );
        let e = &parse_alert_inventory(md)[0];
        assert!(e.for_inside_fence);
        let rules = rules_map(&[("AlertForInside", "up == 0", "1m")]);
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(tmp.path().join("docs/observability")).expect("mkdir");
        std::fs::write(tmp.path().join(ALERT_INVENTORY), md).expect("write");
        let msgs = check_inventory_drift(tmp.path(), &rules).expect("check").0;
        assert_eq!(msgs.len(), 1, "must FAIL, not silently accept the shape");
        assert!(
            msgs[0].2.contains("INSIDE the ```promql fence"),
            "{}",
            msgs[0].2
        );
    }

    /// FIXTURE 5 — an empty entry set fails loudly with its OWN reason token.
    /// "0 drifts over 0 entries" is the real-world form of every other fixture's
    /// failure mode, and it is the one that looks most like success.
    #[test]
    fn fixture_5_empty_entry_set_is_could_not_evaluate_not_clean() {
        let md = "# Alerts

No entries here at all.
";
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(tmp.path().join("docs/observability")).expect("mkdir");
        std::fs::write(tmp.path().join(ALERT_INVENTORY), md).expect("write");
        let msgs = check_inventory_drift(tmp.path(), &HashMap::new())
            .expect("check")
            .0;
        assert_eq!(msgs.len(), 1);
        assert!(msgs[0].2.contains("COULD-NOT-EVALUATE"), "{}", msgs[0].2);
        assert!(msgs[0].2.contains("vacuously"), "{}", msgs[0].2);
    }

    /// FIXTURE 6 — trailing whitespace resolves per the stated policy, in BOTH
    /// directions. Under `.trim()` these would both pass, which is precisely
    /// what the policy comment at the compare site refuses.
    #[test]
    fn fixture_6_trailing_whitespace_differs_both_directions() {
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(tmp.path().join("docs/observability")).expect("mkdir");

        // Docs carry a trailing space the rule does not.
        let md = "#### A
```promql
up == 0 
```
`for: 1m`
";
        std::fs::write(tmp.path().join(ALERT_INVENTORY), md).expect("write");
        let msgs = check_inventory_drift(
            tmp.path(),
            &rules_map(&[(
                "A", "up == 0
", "1m",
            )]),
        )
        .expect("check")
        .0;
        assert_eq!(msgs.len(), 1, "docs-side trailing space must be caught");

        // Rule carries one the docs do not.
        let md2 = "#### A
```promql
up == 0
```
`for: 1m`
";
        std::fs::write(tmp.path().join(ALERT_INVENTORY), md2).expect("write");
        let msgs2 = check_inventory_drift(
            tmp.path(),
            &rules_map(&[(
                "A",
                "up == 0 
",
                "1m",
            )]),
        )
        .expect("check")
        .0;
        assert_eq!(msgs2.len(), 1, "rules-side trailing space must be caught");

        // And the exact pair, with the YAML scalar's trailing newline, matches —
        // which is the half that proves the policy is workable and not merely strict.
        std::fs::write(tmp.path().join(ALERT_INVENTORY), md2).expect("write");
        let (msgs3, pairs3) = check_inventory_drift(
            tmp.path(),
            &rules_map(&[(
                "A", "up == 0
", "1m",
            )]),
        )
        .expect("check");
        assert!(msgs3.is_empty(), "{msgs3:?}");
        assert_eq!(
            pairs3, 1,
            "the matching pair must be COUNTED, not merely silent"
        );
    }

    /// FIXTURE 7 — an orphan heading: a `#### <Name>` whose alert exists in no
    /// rules file. Zero live instances today, so without this the arm would
    /// ship having never run.
    #[test]
    fn fixture_7_orphan_heading_detected() {
        let md = "#### AlertThatDoesNotExist
```promql
up == 0
```
`for: 1m`
";
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(tmp.path().join("docs/observability")).expect("mkdir");
        std::fs::write(tmp.path().join(ALERT_INVENTORY), md).expect("write");
        let msgs = check_inventory_drift(tmp.path(), &HashMap::new())
            .expect("check")
            .0;
        assert_eq!(msgs.len(), 1);
        assert!(
            msgs[0].2.contains("exists in NO rules file"),
            "{}",
            msgs[0].2
        );
    }

    #[test]
    fn check_hygiene_ipv4_allowlist() {
        assert!(check_hygiene("see 127.0.0.1 here").is_none());
        assert!(check_hygiene("see 10.0.5.7 here").is_some());
    }
}
