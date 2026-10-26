//! Extraction kernels for "are the alert rules Prometheus is meant to load
//! actually loaded", plus their FIRE fixtures.
//!
//! # Why the kernels are here and not inline in the cluster-gated test
//!
//! `cargo test` cannot reach a cluster-gated file, so an extractor written
//! inline in one gets **zero unit coverage** — and this repo has already paid
//! for that. `tests/32_media_metric_hygiene.rs` records an earlier check that
//! line-parsed `/api/v1/status/config`, which Prometheus returns as an
//! **escaped JSON string with zero real newlines**: every extraction matched
//! nothing, and the check passed on every possible input including a real
//! violation. Same split applied here — the kernels are pure and unit-tested
//! against known input; the cluster-gated test supplies only real bytes and
//! renders failures.
//!
//! Both kernels therefore parse **structured data as structured data**: the
//! on-disk rule files as YAML, `/api/v1/rules` as JSON. A `grep 'alert:'` over
//! the YAML would sweep in `mh-alerts.yaml`'s header comment lines
//! (`# - No RegisterMeeting RPC alert: ...`) and produce expectations that can
//! never be satisfied.
//!
//! # There is NO file-set predicate here, deliberately
//!
//! An earlier version of this module hand-wrote one (`is_loadable_rules_file`)
//! and its header claimed the two were "kept honest by `dt-guard
//! alert-rules-policy`'s `rule_file_loading` rule". **That claim was false**:
//! `check_rule_file_loading` reads the on-disk predicate, the two Prometheus
//! `rule_files` globs and the kustomize `files:` list, and never opens anything
//! under `crates/env-tests/`. Nothing bound them, and they already disagreed —
//! for `Foo-alerts.yaml` dt-guard's predicate said loadable and the local copy
//! said not. A comment asserting coverage that does not exist is the exact
//! pattern this whole task exists to close, and it was sitting in the task's own
//! artifact.
//!
//! So the predicate is gone rather than documented. [`mounted_rules_files`]
//! reads the **kustomize `configMapGenerator` file list** — the artifact that
//! decides which rule files are actually in the pod this test queries. That is
//! the thing under test, so the extraction structurally cannot disagree with it.
//!
//! **Why deriving from THIS member of the set is safe** — the reasoning that
//! should stop the next reader "simplifying" it back to a local predicate:
//! `check_rule_file_loading` pins four things as SET-EQUAL — the K8s
//! `rule_files` glob, the docker `rule_files` glob, this `configMapGenerator`
//! list, and the on-disk `loadable_rules_files` predicate — and fails in either
//! direction. Deriving from **any member of that pinned set** closes the loop;
//! writing a fifth predicate here opens it.
//!
//! **Why the ConfigMap list rather than the glob**, which @test and
//! @infrastructure both named as the preferred source: deriving from the glob
//! would require re-implementing Go's `filepath.Match` subset in this crate,
//! which is a *new* duplicate of `dt-guard`'s `glob_matches` — trading one
//! unguarded encoding for another. The ConfigMap list needs no matcher, and it
//! is additionally the most direct answer to "what is in the pod this test
//! queries". Both are members of the pinned set, so the safety argument is
//! identical; this one costs less. Say so if you disagree — the requirement
//! (derive from the pinned set, never hand-write a predicate) is met either way.
//!
//! **What keeps that honest, stated so it is checkable rather than asserted**:
//! `dt-guard alert-rules-policy`'s `rule_file_loading` rule *does* read this
//! list, and asserts set equality between it, `loadable_rules_files()` and both
//! `rule_files` globs, failing in either direction. So a file added to the glob
//! but not the ConfigMap fails Layer 3 before this test can silently skip it —
//! which is the fail-open case a hand-written mirror leaves open.

use std::collections::BTreeSet;
use std::fmt::Write as _;

/// Rules directory, repo-relative. Single on-disk copy; the in-cluster
/// ConfigMap is generated from it.
pub const RULES_DIR: &str = "infra/docker/prometheus/rules";

/// The kustomization whose `configMapGenerator` decides which rule files are
/// mounted into the cluster Prometheus this test queries.
pub const RULES_KUSTOMIZATION: &str = "infra/docker/prometheus/kustomization.yaml";

/// The rule-file basenames that are actually MOUNTED in-cluster, read out of the
/// `configMapGenerator` list rather than decided by a predicate here.
///
/// Kustomize cannot glob, so this list is an enumeration — which makes it the
/// literal answer to "what is in the pod", and therefore the right thing for a
/// cluster test to derive from. Returns `None` if no entry is found, so an
/// unparseable or restructured kustomization is a could-not-evaluate rather than
/// an empty-and-therefore-clean set.
///
/// Anchored on the `- ` bullet and comment-stripped for the same two reasons
/// `dt-guard`'s `extract_declared_generator_files` is: an inline comment
/// otherwise leaves the tail ending in comment text, and a *standalone* comment
/// naming a path otherwise counts as declaring it — the second being fail-open.
pub fn mounted_rules_files(kustomization: &str) -> Option<BTreeSet<String>> {
    let mut out = BTreeSet::new();
    for line in kustomization.lines() {
        let Some(rest) = line.trim().strip_prefix("- ") else {
            continue;
        };
        let rest = rest
            .split_once('#')
            .map_or(rest, |(before, _)| before)
            .trim();
        let Some((_, base)) = rest.rsplit_once('/') else {
            continue;
        };
        if base.ends_with(".yaml") || base.ends_with(".yml") {
            out.insert(base.to_string());
        }
    }
    if out.is_empty() {
        return None;
    }
    Some(out)
}

/// Extract every `alert:` name from one rules file's YAML.
///
/// Returns `None` when the document does not parse or declares no groups —
/// distinct from `Some(empty)`, because "could not read this file" and "this
/// file declares no alerts" have different causes and the caller must not
/// collapse them into a clean result.
pub fn alert_names_in_yaml(content: &str) -> Option<BTreeSet<String>> {
    #[derive(serde::Deserialize)]
    struct Doc {
        groups: Vec<Group>,
    }
    #[derive(serde::Deserialize)]
    struct Group {
        #[serde(default)]
        rules: Vec<Rule>,
    }
    #[derive(serde::Deserialize)]
    struct Rule {
        #[serde(default)]
        alert: Option<String>,
    }
    let doc: Doc = serde_norway::from_str(content).ok()?;
    Some(
        doc.groups
            .into_iter()
            .flat_map(|g| g.rules)
            .filter_map(|r| r.alert)
            .filter(|a| !a.is_empty())
            .collect(),
    )
}

/// The `expr` of the alerting rule named `alert` in one rules file's YAML.
///
/// Lets a test DERIVE a probe from the rule rather than hand-copy its PromQL,
/// so the probe cannot keep testing a stale expression after the rule changes.
/// `None` when the document does not parse, or declares no such alert.
pub fn alert_expr_in_yaml(content: &str, alert: &str) -> Option<String> {
    #[derive(serde::Deserialize)]
    struct Doc {
        groups: Vec<Group>,
    }
    #[derive(serde::Deserialize)]
    struct Group {
        #[serde(default)]
        rules: Vec<Rule>,
    }
    #[derive(serde::Deserialize)]
    struct Rule {
        #[serde(default)]
        alert: Option<String>,
        #[serde(default)]
        expr: Option<String>,
    }
    let doc: Doc = serde_norway::from_str(content).ok()?;
    doc.groups
        .into_iter()
        .flat_map(|g| g.rules)
        .find(|r| r.alert.as_deref() == Some(alert))
        .and_then(|r| r.expr)
}

/// The committed `expr` of `alert` in `RULES_DIR/<file>` — what a FIRE or APPLY
/// probe derives from, never a hand copy.
///
/// # Panics
///
/// With a COULD-NOT-EVALUATE reason when the file cannot be read or declares no
/// such alert with an expr: a broken derivation, never a clean result.
#[must_use]
pub fn committed_alert_expr(file: &str, alert: &str) -> String {
    let path = crate::repo_root().join(RULES_DIR).join(file);
    let rules = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("COULD-NOT-EVALUATE: cannot read {}: {e}", path.display()));
    alert_expr_in_yaml(&rules, alert)
        .unwrap_or_else(|| panic!("COULD-NOT-EVALUATE: no alert `{alert}` with an expr in {file}"))
}

/// Extract every loaded alert name from a Prometheus `/api/v1/rules` response.
///
/// Parsed as JSON, never line-matched. Returns `None` when the envelope is not
/// a successful, well-formed rules response — again distinct from an empty set.
pub fn loaded_alert_names(body: &str) -> Option<BTreeSet<String>> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    if v.get("status").and_then(serde_json::Value::as_str) != Some("success") {
        return None;
    }
    let groups = v.get("data")?.get("groups")?.as_array()?;
    let mut out = BTreeSet::new();
    for g in groups {
        let Some(rules) = g.get("rules").and_then(serde_json::Value::as_array) else {
            continue;
        };
        for r in rules {
            if r.get("type").and_then(serde_json::Value::as_str) == Some("alerting") {
                if let Some(name) = r.get("name").and_then(serde_json::Value::as_str) {
                    out.insert(name.to_string());
                }
            }
        }
    }
    Some(out)
}

// -----------------------------------------------------------------------------
// PromQL vector-selector scanner, marker injection, loaded-expression compare
// (story 3 OBS-B)
// -----------------------------------------------------------------------------
//
// # Why a hand-rolled scanner, and why HERE
//
// The FIRE halves of the story-3 client rules evaluate the COMMITTED expression
// with one extra matcher (a per-run marker) added to each `dt_client_*`
// selector, and the APPLY half queries each selector of each rule on its own.
// Both need every selector's BYTE SPAN in the expression, so the marker is
// inserted structurally (never by string replace) and removing it round-trips
// byte-for-byte. Prometheus v2.48.0's `/api/v1/parse_query` returns an AST
// that, as far as is known, carries no source positions, so it cannot drive
// the insertion; `/api/v1/format_query` re-prints the whole expression. So the
// scan is local. It is hand-rolled because `clippy.toml` bans
// `regex::Regex::new` outside canonical homes, and a regex cannot track quoted
// matcher values containing `}` or `,` anyway.
//
// This is the THIRD PromQL selector extractor in the tree (dt-guard has
// `SERVICE_METRIC_PREFIX_RE` and `client_metrics_export.rs`'s
// `PROMQL_DT_CLIENT_SELECTOR_RE`); env-tests cannot depend on dt-guard. The
// overlap is logged in `docs/TODO.md` §Cross-Service Duplication ("From DRY
// Reviewer — story 3 OBS-B").
//
// SCOPE: the PromQL subset the rule files use. Anything the scanner does not
// understand is an `Err` (the caller reports COULD-NOT-EVALUATE), never a
// silently skipped selector.

/// A label matcher operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchOp {
    /// `=`
    Eq,
    /// `!=`
    Ne,
    /// `=~`
    Re,
    /// `!~`
    Nre,
}

impl MatchOp {
    /// True for the two negated operators (`!=`, `!~`).
    #[must_use]
    pub const fn is_negated(self) -> bool {
        matches!(self, Self::Ne | Self::Nre)
    }
}

/// One `label op "value"` inside a selector's braces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelMatcher {
    /// The label name.
    pub label: String,
    /// The operator.
    pub op: MatchOp,
    /// The unquoted value.
    pub value: String,
}

/// One vector selector found in an expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VectorSelector {
    /// The metric name.
    pub name: String,
    /// The matchers inside its braces (empty if it has none).
    pub matchers: Vec<LabelMatcher>,
    /// Byte span of the whole selector (name through closing `}`), never
    /// including a range `[5m]` or `offset`.
    pub span: std::ops::Range<usize>,
    /// Byte offset of the opening `{`, if the selector has braces.
    pub open_brace: Option<usize>,
}

impl VectorSelector {
    /// The selector's source text.
    #[must_use]
    pub fn text<'a>(&self, expr: &'a str) -> &'a str {
        &expr[self.span.clone()]
    }
}

/// The aggregation modifiers. They take a parenthesised LABEL list, and may
/// also FOLLOW an aggregation's name (`sum by (...) (...)`).
const AGGREGATION_MODIFIERS: &[&str] = &["by", "without"];
/// The other keywords that take a parenthesised LABEL list (vector matching).
const LABEL_LIST_KEYWORDS: &[&str] = &["on", "ignoring", "group_left", "group_right"];
/// Keywords that are never metric names and take no label list.
const OTHER_KEYWORDS: &[&str] = &["bool", "and", "or", "unless", "offset", "atan2"];

/// A word that takes a parenthesised label list to skip.
fn takes_label_list(word: &str) -> bool {
    AGGREGATION_MODIFIERS.contains(&word) || LABEL_LIST_KEYWORDS.contains(&word)
}

/// A word that is never a metric name: the union of the three lists, no word
/// listed twice.
fn is_keyword(word: &str) -> bool {
    takes_label_list(word) || OTHER_KEYWORDS.contains(&word)
}

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_' || b == b':'
}
fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b':'
}

/// Skip a quoted string starting at `i` (the quote byte); returns the index
/// just past the closing quote and the unescaped content.
fn scan_string(bytes: &[u8], i: usize) -> Result<(usize, String), String> {
    let quote = bytes[i];
    let mut j = i + 1;
    let mut out = Vec::new();
    while let Some(&b) = bytes.get(j) {
        if b == b'\\' && quote != b'`' {
            let Some(&next) = bytes.get(j + 1) else {
                return Err(format!("unterminated escape at byte {j}"));
            };
            out.push(next);
            j += 2;
            continue;
        }
        if b == quote {
            return String::from_utf8(out)
                .map(|s| (j + 1, s))
                .map_err(|e| format!("non-UTF-8 string at byte {i}: {e}"));
        }
        out.push(b);
        j += 1;
    }
    Err(format!("unterminated string starting at byte {i}"))
}

fn skip_ws(bytes: &[u8], mut i: usize) -> usize {
    while bytes.get(i).is_some_and(u8::is_ascii_whitespace) {
        i += 1;
    }
    i
}

/// Skip a balanced `(...)` starting at `i` (the `(`), honouring strings.
fn skip_parens(bytes: &[u8], i: usize) -> Result<usize, String> {
    let mut depth = 0usize;
    let mut j = i;
    while let Some(&b) = bytes.get(j) {
        match b {
            b'"' | b'\'' | b'`' => {
                j = scan_string(bytes, j)?.0;
                continue;
            }
            b'(' => depth += 1,
            b')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Ok(j + 1);
                }
            }
            _ => {}
        }
        j += 1;
    }
    Err(format!("unbalanced parenthesis starting at byte {i}"))
}

/// Parse the matchers of a `{...}` starting at `i` (the `{`). Returns the
/// index just past `}`.
fn scan_matchers(bytes: &[u8], i: usize) -> Result<(usize, Vec<LabelMatcher>), String> {
    let mut j = i + 1;
    let mut out = Vec::new();
    loop {
        j = skip_ws(bytes, j);
        match bytes.get(j) {
            Some(b'}') => return Ok((j + 1, out)),
            Some(&b) if is_ident_start(b) => {}
            Some(&b) => {
                return Err(format!(
                    "unexpected byte {:?} in matcher list at {j}",
                    char::from(b)
                ))
            }
            None => return Err(format!("unterminated matcher list starting at byte {i}")),
        }
        let start = j;
        while bytes.get(j).copied().is_some_and(is_ident_char) {
            j += 1;
        }
        let label = String::from_utf8_lossy(&bytes[start..j]).into_owned();
        j = skip_ws(bytes, j);
        let op = match (bytes.get(j), bytes.get(j + 1)) {
            (Some(b'='), Some(b'~')) => {
                j += 2;
                MatchOp::Re
            }
            (Some(b'!'), Some(b'=')) => {
                j += 2;
                MatchOp::Ne
            }
            (Some(b'!'), Some(b'~')) => {
                j += 2;
                MatchOp::Nre
            }
            (Some(b'='), _) => {
                j += 1;
                MatchOp::Eq
            }
            _ => return Err(format!("no matcher operator after `{label}` at byte {j}")),
        };
        j = skip_ws(bytes, j);
        if !matches!(bytes.get(j), Some(b'"' | b'\'' | b'`')) {
            return Err(format!("matcher `{label}` has no quoted value at byte {j}"));
        }
        let (next, value) = scan_string(bytes, j)?;
        out.push(LabelMatcher { label, op, value });
        j = skip_ws(bytes, next);
        match bytes.get(j) {
            Some(b',') => j += 1,
            Some(b'}') => {}
            _ => return Err(format!("expected `,` or `}}` after a matcher at byte {j}")),
        }
    }
}

/// Every vector selector in `expr`, in source order, with byte spans.
///
/// # Errors
///
/// Any construct the scanner does not understand (unbalanced brackets, an
/// unterminated string, a bare `{...}` selector with no name). The caller
/// must treat an error as COULD-NOT-EVALUATE, never as "no selectors".
pub fn vector_selectors(expr: &str) -> Result<Vec<VectorSelector>, String> {
    let bytes = expr.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(&b) = bytes.get(i) {
        match b {
            b'"' | b'\'' | b'`' => i = scan_string(bytes, i)?.0,
            b'[' => {
                // Range / subquery duration: skip to the matching `]`.
                let Some(end) = bytes[i..].iter().position(|c| *c == b']') else {
                    return Err(format!("unterminated range at byte {i}"));
                };
                i += end + 1;
            }
            b'#' => {
                // PromQL line comment.
                while bytes.get(i).is_some_and(|c| *c != b'\n') {
                    i += 1;
                }
            }
            b'{' => return Err(format!("a selector with no metric name at byte {i}")),
            b if b.is_ascii_digit() || b == b'.' => {
                // A number or a duration (`5m` after `offset`).
                while bytes
                    .get(i)
                    .copied()
                    .is_some_and(|c| c.is_ascii_alphanumeric() || c == b'.' || c == b'_')
                {
                    i += 1;
                }
            }
            b if is_ident_start(b) => {
                let start = i;
                while bytes.get(i).copied().is_some_and(is_ident_char) {
                    i += 1;
                }
                let word = &expr[start..i];
                let after = skip_ws(bytes, i);
                if is_keyword(word) {
                    if takes_label_list(word) && bytes.get(after) == Some(&b'(') {
                        i = skip_parens(bytes, after)?;
                    }
                    continue;
                }
                if bytes.get(after) == Some(&b'(') {
                    // A function call: its arguments are scanned in turn.
                    continue;
                }
                if AGGREGATION_MODIFIERS.iter().any(|kw| {
                    expr[after..].starts_with(kw)
                        && !bytes
                            .get(after + kw.len())
                            .copied()
                            .is_some_and(is_ident_char)
                }) {
                    // An aggregation written `sum by (...) (...)`.
                    continue;
                }
                let (end, open_brace, matchers) = if bytes.get(after) == Some(&b'{') {
                    let (end, m) = scan_matchers(bytes, after)?;
                    (end, Some(after), m)
                } else {
                    (i, None, Vec::new())
                };
                out.push(VectorSelector {
                    name: word.to_string(),
                    matchers,
                    span: start..end,
                    open_brace,
                });
                i = end;
            }
            _ => i += 1,
        }
    }
    Ok(out)
}

/// `expr` with `label="value"` added to every selector whose name starts with
/// `prefix`, built from the scanner's spans. Every other byte is unchanged.
/// Returns the new expression and how many selectors were marked.
///
/// # Errors
///
/// The scan failed; `value` is not a plain marker (`[A-Za-z0-9_-]+`); or —
/// FAIL CLOSED — re-scanning the result does not find exactly the prefixed
/// selectors carrying the marker, or any non-prefixed selector changed.
pub fn inject_marker(
    expr: &str,
    prefix: &str,
    label: &str,
    value: &str,
) -> Result<(String, usize), String> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err(format!("marker value {value:?} is not [A-Za-z0-9_-]+"));
    }
    let selectors = vector_selectors(expr)?;
    let mut out = String::with_capacity(expr.len() + 64);
    let mut cursor = 0;
    let mut marked = 0;
    for s in &selectors {
        if !s.name.starts_with(prefix) {
            continue;
        }
        if let Some(brace) = s.open_brace {
            out.push_str(&expr[cursor..=brace]);
            // Writing to a `String` cannot fail.
            let _ = write!(out, "{label}=\"{value}\"");
            if !s.matchers.is_empty() {
                out.push_str(", ");
            }
            cursor = brace + 1;
        } else {
            out.push_str(&expr[cursor..s.span.end]);
            let _ = write!(out, "{{{label}=\"{value}\"}}");
            cursor = s.span.end;
        }
        marked += 1;
    }
    out.push_str(&expr[cursor..]);

    // FAIL CLOSED: the result must carry the marker on exactly the prefixed
    // selectors, and every other selector must be byte-identical.
    let got = vector_selectors(&out)?;
    if got.len() != selectors.len() {
        return Err(format!(
            "marker injection changed the selector count ({} -> {})",
            selectors.len(),
            got.len()
        ));
    }
    let mut carrying = 0;
    for (w, g) in selectors.iter().zip(&got) {
        let has_marker = g
            .matchers
            .iter()
            .any(|m| m.label == label && m.op == MatchOp::Eq && m.value == value);
        if w.name.starts_with(prefix) {
            if !has_marker || g.matchers.len() != w.matchers.len() + 1 {
                return Err(format!("selector `{}` did not receive the marker", w.name));
            }
            carrying += 1;
        } else if g.text(&out) != w.text(expr) {
            return Err(format!(
                "non-`{prefix}` selector `{}` was altered by injection",
                w.name
            ));
        }
    }
    let expected = selectors
        .iter()
        .filter(|s| s.name.starts_with(prefix))
        .count();
    if carrying != expected || marked != expected {
        return Err(format!(
            "injected {marked}/{carrying} markers but the scan found {expected} `{prefix}` selectors"
        ));
    }
    Ok((out, marked))
}

/// The formatted expression out of a `/api/v1/format_query` response body
/// (`{"status":"success","data":"<expr>"}`); `None` if not that envelope.
#[must_use]
pub fn formatted_query(body: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    if v.get("status").and_then(serde_json::Value::as_str) != Some("success") {
        return None;
    }
    v.get("data")?.as_str().map(str::to_string)
}

/// Every loaded alerting rule's `query` from a `/api/v1/rules` body, keyed by
/// alert name. A rule present with no string `query` maps to `None` — a
/// COULD-NOT-EVALUATE, distinct from a mismatch. `None` overall if the body is
/// not a successful rules envelope.
#[must_use]
pub fn loaded_alert_queries(
    body: &str,
) -> Option<std::collections::BTreeMap<String, Option<String>>> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    if v.get("status").and_then(serde_json::Value::as_str) != Some("success") {
        return None;
    }
    let mut out = std::collections::BTreeMap::new();
    for g in v.get("data")?.get("groups")?.as_array()? {
        let Some(rules) = g.get("rules").and_then(serde_json::Value::as_array) else {
            continue;
        };
        for r in rules {
            if r.get("type").and_then(serde_json::Value::as_str) != Some("alerting") {
                continue;
            }
            if let Some(name) = r.get("name").and_then(serde_json::Value::as_str) {
                out.insert(
                    name.to_string(),
                    r.get("query")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string),
                );
            }
        }
    }
    Some(out)
}

/// Verdict of comparing a loaded rule's expression with the committed one.
#[derive(Debug, PartialEq, Eq)]
pub enum ExprCheck {
    /// Byte-identical after both sides went through `format_query`.
    Match,
    /// Both sides formatted, and they differ.
    Drift {
        /// The committed expression, formatted.
        committed: String,
        /// The loaded expression, formatted.
        loaded: String,
    },
    /// One side could not be obtained or formatted (never a pass).
    CouldNotEvaluate(String),
}

/// Compare the committed and loaded expressions of one alert. Both inputs are
/// the FORMATTED forms (`format_query` of each side): Prometheus re-stringifies
/// what it loads, so the raw YAML text never matches literally, and formatting
/// both sides from their own parse is what makes the comparison exact.
#[must_use]
pub fn compare_loaded_expr(
    committed_formatted: Option<&str>,
    loaded_formatted: Option<&str>,
) -> ExprCheck {
    match (committed_formatted, loaded_formatted) {
        (None, _) => ExprCheck::CouldNotEvaluate("committed expr did not format".to_string()),
        (_, None) => ExprCheck::CouldNotEvaluate(
            "loaded rule has no `query`, or it did not format".to_string(),
        ),
        (Some(c), Some(l)) if c == l => ExprCheck::Match,
        (Some(c), Some(l)) => ExprCheck::Drift {
            committed: c.to_string(),
            loaded: l.to_string(),
        },
    }
}

/// The SDK source file that holds the client histogram bucket constants.
pub const SDK_MEDIA_METRICS_TS: &str = "packages/sdk-core/src/media/setup/mediaMetrics.ts";

/// Extract the numeric array literal bound to `export const <name>` in a
/// TypeScript source: the bucket bounds a FIRE post must use so its
/// observation lands in the same `le` buckets the SDK exports.
///
/// Structural, not "first `[` after the name": the declaration is typed
/// (`export const X: readonly number[] = [ ... ];`), and the type annotation's
/// own `[]` sits before the initialiser — reading the first bracket pair after
/// the name returned an EMPTY list on the real file (Gate 2, Layer 7). So the
/// scan anchors on the declaration's `=`, takes the bracket that opens the
/// initialiser, and reads to its matching `]`. Line comments are stripped,
/// numeric separators (`300_000`) are accepted, and EVERY other token is an
/// error, as are an empty list and a non-increasing one — the caller reports
/// COULD-NOT-EVALUATE, never a silent pass on a misread.
///
/// # Errors
///
/// A description of the first structural or numeric problem found.
pub fn ts_numeric_array_const(src: &str, name: &str) -> Result<Vec<f64>, String> {
    let decl = format!("export const {name}");
    let mut found = src.match_indices(&decl).filter(|(i, _)| {
        // The name must end there (not a prefix of a longer identifier).
        src[i + decl.len()..]
            .chars()
            .next()
            .is_some_and(|c| !(c.is_ascii_alphanumeric() || c == '_'))
    });
    let (start, _) = found.next().ok_or_else(|| format!("`{decl}` not found"))?;
    if found.next().is_some() {
        return Err(format!("`{decl}` declared more than once"));
    }
    let after = &src[start + decl.len()..];
    let eq = after
        .find('=')
        .ok_or_else(|| format!("`{decl}` has no `=` initialiser"))?;
    let init = after[eq + 1..].trim_start();
    let body = init
        .strip_prefix('[')
        .ok_or_else(|| format!("`{decl}`'s initialiser is not an array literal"))?;
    // Strip line comments BEFORE looking for the closing bracket, so a `]`
    // inside a comment cannot truncate the list.
    let code: String = body
        .lines()
        .map(|l| l.split_once("//").map_or(l, |(code, _)| code))
        .collect::<Vec<_>>()
        .join("\n");
    let close = code
        .find(']')
        .ok_or_else(|| format!("`{decl}`'s array literal is not closed"))?;
    let inside = &code[..close];
    let mut values = Vec::new();
    for tok in inside.split(',').map(str::trim).filter(|t| !t.is_empty()) {
        let digits = tok.replace('_', "");
        let v = digits
            .parse::<f64>()
            .map_err(|e| format!("`{decl}` element `{tok}` is not a number: {e}"))?;
        values.push(v);
    }
    if values.is_empty() {
        return Err(format!("`{decl}` parsed as an EMPTY list"));
    }
    if !values.windows(2).all(|w| w[0] < w[1]) {
        return Err(format!("`{decl}` is not strictly increasing: {values:?}"));
    }
    Ok(values)
}

#[cfg(test)]
mod tests {

    // ---- ts_numeric_array_const --------------------------------------------

    #[test]
    fn ts_array_typed_declaration_reads_the_initialiser_not_the_type() {
        // The exact shape that returned [] at Gate 2: the type's `[]` precedes
        // the initialiser.
        let src = "export const SLOT_UNFULFILLED_BUCKETS_MS: readonly number[] = [\n  100, 250, 1_000, 300_000,\n];\n";
        assert_eq!(
            ts_numeric_array_const(src, "SLOT_UNFULFILLED_BUCKETS_MS").unwrap(),
            vec![100.0, 250.0, 1000.0, 300_000.0]
        );
    }

    #[test]
    fn ts_array_strips_line_comments_and_rejects_junk() {
        let src = "export const B = [\n  1, // one\n  2,\n];";
        assert_eq!(ts_numeric_array_const(src, "B").unwrap(), vec![1.0, 2.0]);
        let bracket_in_comment = "export const B = [\n  1, // see [x]\n  2,\n];";
        assert_eq!(
            ts_numeric_array_const(bracket_in_comment, "B").unwrap(),
            vec![1.0, 2.0]
        );
        let junk = "export const B = [1, FOO, 3];";
        assert!(ts_numeric_array_const(junk, "B")
            .unwrap_err()
            .contains("not a number"));
    }

    #[test]
    fn ts_array_fails_closed_on_empty_missing_unordered_and_prefix() {
        assert!(
            ts_numeric_array_const("export const B: number[] = [];", "B")
                .unwrap_err()
                .contains("EMPTY")
        );
        assert!(ts_numeric_array_const("const X = [1];", "B").is_err());
        assert!(ts_numeric_array_const("export const B = [2, 1];", "B")
            .unwrap_err()
            .contains("increasing"));
        // `B_EXTRA` is not `B`.
        assert!(ts_numeric_array_const("export const B_EXTRA = [1, 2];", "B").is_err());
        assert!(
            ts_numeric_array_const("export const B = makeBuckets();", "B")
                .unwrap_err()
                .contains("not an array literal")
        );
    }

    #[test]
    fn ts_array_reads_the_real_sdk_slot_unfulfilled_buckets() {
        // Positive control against the REAL declaration the FIRE post uses.
        let path = crate::repo_root().join(SDK_MEDIA_METRICS_TS);
        let src = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("COULD-NOT-EVALUATE: cannot read {}: {e}", path.display()));
        let b = ts_numeric_array_const(&src, "SLOT_UNFULFILLED_BUCKETS_MS")
            .unwrap_or_else(|e| panic!("COULD-NOT-EVALUATE: {e}"));
        // Story 3 amendment 4: the buckets extend past 100 s.
        assert!(b.len() >= 2, "{b:?}");
        assert!(b.last().copied().unwrap_or(0.0) > 100_000.0, "{b:?}");
    }

    use super::*;

    // FIRE fixtures. Every branch below is one the cluster-gated test cannot
    // exercise: on a healthy cluster it passes, so without these the extractors
    // would ship having never been shown to produce the RIGHT answer on known
    // input — only to have produced no complaint on unknown input.

    #[test]
    fn mounted_set_is_read_from_the_configmap_list() {
        let k = concat!(
            "configMapGenerator:\n",
            "  - name: prometheus-rules-mc\n",
            "    files:\n",
            "      # rules/_template-service-alerts.yaml is deliberately absent\n",
            "      - rules/mc-alerts.yaml\n",
            "  - name: prometheus-rules-mh\n",
            "    files:\n",
            "      - rules/mh-alerts.yaml   # media handler\n",
        );
        let got = mounted_rules_files(k).expect("parses");
        assert_eq!(
            got,
            ["mc-alerts.yaml".to_string(), "mh-alerts.yaml".to_string()]
                .into_iter()
                .collect(),
            "the mounted set is the union over EVERY generator (one per rules \
             file since 2026-10-25); a standalone comment must not declare a \
             file (fail-open), and an inline comment must not un-declare one"
        );
    }

    #[test]
    fn mounted_set_distinguishes_unreadable_from_empty() {
        // An empty result is a broken extraction, not a clean one: an empty
        // expected set makes every downstream assertion pass vacuously.
        assert!(mounted_rules_files("resources:\n  - deployment.yaml\n").is_none());
        assert!(mounted_rules_files("").is_none());
    }

    #[test]
    fn yaml_kernel_returns_the_right_names_and_a_non_empty_set() {
        let yaml = concat!(
            "# - No RegisterMeeting RPC alert: this comment must NOT be extracted\n",
            "groups:\n",
            "  - name: mc-service-page\n",
            "    rules:\n",
            "      - alert: MCDown\n",
            "        expr: up == 0\n",
            "      - record: some:recording:rule\n",
            "        expr: vector(1)\n",
            "      - alert: MCActorPanic\n",
            "        expr: increase(x[5m]) > 0\n",
        );
        let names = alert_names_in_yaml(yaml).expect("parses");
        assert_eq!(
            names,
            ["MCActorPanic".to_string(), "MCDown".to_string()]
                .into_iter()
                .collect()
        );
        assert!(!names.is_empty(), "positive control");
    }

    #[test]
    fn expr_kernel_returns_the_named_alerts_expr_and_none_for_unknown() {
        let yaml = concat!(
            "groups:\n",
            "  - name: mh-service-warning\n",
            "    rules:\n",
            "      - alert: MHFirst\n",
            "        expr: |\n",
            "          a > b\n",
            "      - record: some:recording:rule\n",
            "        expr: vector(1)\n",
            "      - alert: MHSecond\n",
            "        expr: c / d > 0.8\n",
        );
        assert_eq!(
            alert_expr_in_yaml(yaml, "MHFirst").as_deref(),
            Some("a > b\n")
        );
        assert_eq!(
            alert_expr_in_yaml(yaml, "MHSecond").as_deref(),
            Some("c / d > 0.8")
        );
        assert!(alert_expr_in_yaml(yaml, "MHAbsent").is_none());
        assert!(alert_expr_in_yaml("this: [is: not: valid", "MHFirst").is_none());
    }

    #[test]
    fn yaml_kernel_distinguishes_unparseable_from_empty() {
        assert!(alert_names_in_yaml("this: [is: not: valid").is_none());
        assert!(alert_names_in_yaml("groups: []\n")
            .expect("parses")
            .is_empty());
    }

    #[test]
    fn json_kernel_returns_the_right_names() {
        let body = r#"{"status":"success","data":{"groups":[
            {"name":"mc-service-page","rules":[
              {"type":"alerting","name":"MCDown"},
              {"type":"recording","name":"some:rule"},
              {"type":"alerting","name":"MCActorPanic"}
            ]}]}}"#;
        let names = loaded_alert_names(body).expect("parses");
        assert_eq!(
            names,
            ["MCActorPanic".to_string(), "MCDown".to_string()]
                .into_iter()
                .collect()
        );
    }

    #[test]
    fn json_kernel_rejects_non_success_and_garbage() {
        // The trap this repo already stepped in, in its /api/v1/rules form: a
        // body that is not the expected envelope must be a COULD-NOT-EVALUATE,
        // never an empty-and-therefore-clean result.
        assert!(loaded_alert_names(r#"{"status":"error","error":"boom"}"#).is_none());
        assert!(loaded_alert_names("global:\n  scrape_interval: 15s\n").is_none());
        assert!(loaded_alert_names("").is_none());
    }

    // ---- PromQL selector scanner, marker injection, expr compare (OBS-B) ----

    fn names(expr: &str) -> Vec<String> {
        vector_selectors(expr)
            .expect("scans")
            .into_iter()
            .map(|s| s.name)
            .collect()
    }

    #[test]
    fn scanner_finds_selectors_and_skips_functions_keywords_and_label_lists() {
        let expr = concat!(
            "(\n  sum by (media_kind) (rate(a_total{reason=~\"x|y\"}[5m]))\n",
            "  /\n  sum by (media_kind) (rate(b_total[5m] offset 5m))\n) > 0.05\n",
            "and on (le) group_left (foo) histogram_quantile(0.95, sum by (le) (rate(c_bucket[5m]))) ",
            "> bool 10 * 1000 * max(d_seconds)\nor\nsum without (instance) (e) unless f",
        );
        assert_eq!(
            names(expr),
            ["a_total", "b_total", "c_bucket", "d_seconds", "e", "f"]
        );
    }

    #[test]
    fn scanner_reads_matchers_with_braces_and_commas_inside_quoted_values() {
        let expr = r#"sum(rate(m_total{a="x}y", b!~"p,q", c!="", d=~'s'}[5m])) > 0"#;
        let sel = vector_selectors(expr).expect("scans");
        assert_eq!(sel.len(), 1);
        let s = &sel[0];
        assert_eq!(s.name, "m_total");
        assert_eq!(
            s.text(expr),
            r#"m_total{a="x}y", b!~"p,q", c!="", d=~'s'}"#,
            "the span is name through closing brace, never the range"
        );
        let got: Vec<(&str, MatchOp, &str)> = s
            .matchers
            .iter()
            .map(|m| (m.label.as_str(), m.op, m.value.as_str()))
            .collect();
        assert_eq!(
            got,
            [
                ("a", MatchOp::Eq, "x}y"),
                ("b", MatchOp::Nre, "p,q"),
                ("c", MatchOp::Ne, ""),
                ("d", MatchOp::Re, "s"),
            ]
        );
        assert!(s.matchers[1].op.is_negated() && !s.matchers[0].op.is_negated());
    }

    #[test]
    fn scanner_bare_name_has_no_brace_and_a_name_only_span() {
        let expr = "max(mc_media_video_keyframe_min_interval_seconds)";
        let sel = vector_selectors(expr).expect("scans");
        assert_eq!(sel.len(), 1);
        assert!(sel[0].open_brace.is_none() && sel[0].matchers.is_empty());
        assert_eq!(
            sel[0].text(expr),
            "mc_media_video_keyframe_min_interval_seconds"
        );
    }

    #[test]
    fn scanner_errors_rather_than_skipping_what_it_cannot_read() {
        assert!(
            vector_selectors(r#"sum(m{a="x)"#).is_err(),
            "unterminated string"
        );
        assert!(
            vector_selectors(r#"sum(m{a})"#).is_err(),
            "matcher with no op"
        );
        assert!(vector_selectors(r#"{a="b"}"#).is_err(), "nameless selector");
        assert!(
            vector_selectors("rate(m[5m)").is_err(),
            "unterminated range"
        );
    }

    #[test]
    fn scanner_reads_every_committed_rule_file() {
        // POSITIVE CONTROL over the real artifact: every expr in every
        // mounted rule file scans and yields at least one selector.
        let root = crate::repo_root();
        let kust = std::fs::read_to_string(root.join(RULES_KUSTOMIZATION)).expect("read");
        let mut exprs = 0;
        for f in mounted_rules_files(&kust).expect("mounted set") {
            let yaml = std::fs::read_to_string(root.join(RULES_DIR).join(&f)).expect("read");
            for alert in alert_names_in_yaml(&yaml).expect("parses") {
                let expr = alert_expr_in_yaml(&yaml, &alert).expect("expr");
                let sel = vector_selectors(&expr)
                    .unwrap_or_else(|e| panic!("{f}:{alert}: scanner failed: {e}"));
                assert!(!sel.is_empty(), "{f}:{alert}: no selector found");
                exprs += 1;
            }
        }
        assert!(exprs > 0, "positive control: some expr was scanned");
    }

    #[test]
    fn marker_injection_marks_only_prefixed_selectors_and_round_trips() {
        let expr = concat!(
            "histogram_quantile(0.95, sum by (le) (rate(dt_client_x_bucket{media_kind=\"video\"}[5m]))) ",
            "> 10 * 1000 * max(mc_media_video_keyframe_min_interval_seconds)\nand\n",
            "sum(rate(dt_client_x_count[5m])) > 0",
        );
        let (marked, n) =
            inject_marker(expr, "dt_client_", "client_version", "et-m1").expect("injects");
        assert_eq!(n, 2);
        assert_eq!(
            marked,
            concat!(
                "histogram_quantile(0.95, sum by (le) (rate(dt_client_x_bucket{client_version=\"et-m1\", media_kind=\"video\"}[5m]))) ",
                "> 10 * 1000 * max(mc_media_video_keyframe_min_interval_seconds)\nand\n",
                "sum(rate(dt_client_x_count{client_version=\"et-m1\"}[5m])) > 0",
            ),
            "the MC gauge selector is untouched; only dt_client_ selectors gain the marker"
        );
        // Round trip: removing exactly the inserted text restores every byte.
        let back = marked
            .replace("client_version=\"et-m1\", ", "")
            .replace("{client_version=\"et-m1\"}", "");
        assert_eq!(back, expr);
    }

    #[test]
    fn marker_injection_handles_empty_braces_and_fails_closed() {
        let (marked, n) = inject_marker("sum(dt_client_y{})", "dt_client_", "k", "v").expect("ok");
        assert_eq!((marked.as_str(), n), ("sum(dt_client_y{k=\"v\"})", 1));
        assert!(
            inject_marker("sum(dt_client_y)", "dt_client_", "k", "a\"b").is_err(),
            "unsafe value"
        );
        assert!(
            inject_marker("sum(dt_client_y{a=\"b)", "dt_client_", "k", "v").is_err(),
            "unscannable"
        );
        // Zero prefixed selectors is not an error by itself; the caller
        // compares the count with what the rule must contain.
        assert_eq!(
            inject_marker("up", "dt_client_", "k", "v").expect("ok").1,
            0
        );
    }

    #[test]
    fn format_query_envelope_is_parsed_or_none() {
        assert_eq!(
            formatted_query(r#"{"status":"success","data":"sum(x) > 0"}"#).as_deref(),
            Some("sum(x) > 0")
        );
        assert!(formatted_query(r#"{"status":"error","error":"parse"}"#).is_none());
        assert!(formatted_query("garbage").is_none());
    }

    #[test]
    fn loaded_queries_keep_a_missing_query_distinct() {
        let body = r#"{"status":"success","data":{"groups":[{"rules":[
            {"type":"alerting","name":"A","query":"up == 0"},
            {"type":"alerting","name":"B"},
            {"type":"recording","name":"r:x","query":"vector(1)"}]}]}}"#;
        let q = loaded_alert_queries(body).expect("envelope");
        assert_eq!(q.len(), 2);
        assert_eq!(q["A"].as_deref(), Some("up == 0"));
        assert_eq!(q["B"], None);
        assert!(loaded_alert_queries(r#"{"status":"error"}"#).is_none());
    }

    #[test]
    fn expr_compare_match_drift_and_could_not_evaluate() {
        assert_eq!(
            compare_loaded_expr(Some("a > 0"), Some("a > 0")),
            ExprCheck::Match
        );
        assert!(matches!(
            compare_loaded_expr(Some("a > 0"), Some("a > 1")),
            ExprCheck::Drift { .. }
        ));
        assert!(matches!(
            compare_loaded_expr(Some("a > 0"), None),
            ExprCheck::CouldNotEvaluate(_)
        ));
        assert!(matches!(
            compare_loaded_expr(None, Some("a > 0")),
            ExprCheck::CouldNotEvaluate(_)
        ));
    }

    #[test]
    fn json_kernel_empty_groups_is_empty_not_none() {
        // A Prometheus that loaded NOTHING answers successfully with zero
        // groups. That is a real, distinguishable state and it must reach the
        // caller as `Some(empty)` so the caller can fail on it — not as `None`,
        // which would be triaged as a broken extractor.
        let names = loaded_alert_names(r#"{"status":"success","data":{"groups":[]}}"#)
            .expect("well-formed envelope");
        assert!(names.is_empty());
    }
}
