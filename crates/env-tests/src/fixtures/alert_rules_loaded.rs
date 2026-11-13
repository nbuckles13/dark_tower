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
    Some(
        committed_rules(content)?
            .into_iter()
            .map(|r| r.alert)
            .collect(),
    )
}

/// The `expr` of the alerting rule named `alert` in one rules file's YAML.
///
/// Lets a test DERIVE a probe from the rule rather than hand-copy its PromQL,
/// so the probe cannot keep testing a stale expression after the rule changes.
/// `None` when the document does not parse, or declares no such alert.
pub fn alert_expr_in_yaml(content: &str, alert: &str) -> Option<String> {
    committed_rules(content)?
        .into_iter()
        .find(|r| r.alert == alert)
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
    Some(
        loaded_alerting_rules(body)?
            .into_iter()
            .map(|r| r.name)
            .collect(),
    )
}

/// One loaded ALERTING rule from `/api/v1/rules`, with its group's interval.
struct LoadedAlertingRule {
    name: String,
    group_interval_seconds: Option<f64>,
    rule: serde_json::Value,
}

/// THE `/api/v1/rules` envelope walk (status == success, data.groups, the
/// `type == "alerting"` filter, the name): every loaded-rule reader below is a
/// projection of this. `None` when the body is not a successful, well-formed
/// rules envelope — distinct from an empty set.
fn loaded_alerting_rules(body: &str) -> Option<Vec<LoadedAlertingRule>> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    if v.get("status").and_then(serde_json::Value::as_str) != Some("success") {
        return None;
    }
    let mut out = Vec::new();
    for g in v.get("data")?.get("groups")?.as_array()? {
        let interval = g.get("interval").and_then(serde_json::Value::as_f64);
        let Some(rules) = g.get("rules").and_then(serde_json::Value::as_array) else {
            continue;
        };
        for r in rules {
            if r.get("type").and_then(serde_json::Value::as_str) != Some("alerting") {
                continue;
            }
            if let Some(name) = r.get("name").and_then(serde_json::Value::as_str) {
                out.push(LoadedAlertingRule {
                    name: name.to_string(),
                    group_interval_seconds: interval,
                    rule: r.clone(),
                });
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

    /// The operator's PromQL spelling: the inverse of [`scan_matchers`]'
    /// text-to-operator mapping, so one type owns both directions.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Eq => "=",
            Self::Ne => "!=",
            Self::Re => "=~",
            Self::Nre => "!~",
        }
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
    Some(
        loaded_alerting_rules(body)?
            .into_iter()
            .map(|r| {
                let q = r
                    .rule
                    .get("query")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string);
                (r.name, q)
            })
            .collect(),
    )
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

// =============================================================================
// Story 4 (OBS-ALERTS): `for:` derivations, APPLY probes, page-posture check
// =============================================================================
//
// Pure kernels for env-test 33's story-4 tests, unit-tested below for the same
// reason as everything else in this module: a cluster-gated file gets no unit
// coverage. Every one fails CLOSED (an `Err` / `None` the caller reports as
// COULD-NOT-EVALUATE), never "nothing found".

/// One alerting rule as committed, with its group's evaluation interval.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommittedRule {
    /// `alert:`
    pub alert: String,
    /// `labels.severity`, if present.
    pub severity: Option<String>,
    /// `expr:` (`None` if the rule has none; callers that need it report
    /// COULD-NOT-EVALUATE).
    pub expr: Option<String>,
    /// `for:` as written, if present.
    pub for_: Option<String>,
    /// The enclosing group's `interval:` as written, if present.
    pub group_interval: Option<String>,
}

/// Every alerting rule in one rules file's YAML: THE rule-file YAML kernel
/// (`alert_names_in_yaml` and `alert_expr_in_yaml` are projections of it).
/// `None` when the document does not parse (a broken extraction, not an empty
/// file).
#[must_use]
pub fn committed_rules(content: &str) -> Option<Vec<CommittedRule>> {
    #[derive(serde::Deserialize)]
    struct Doc {
        groups: Vec<Group>,
    }
    #[derive(serde::Deserialize)]
    struct Group {
        #[serde(default)]
        interval: Option<String>,
        #[serde(default)]
        rules: Vec<Rule>,
    }
    #[derive(serde::Deserialize)]
    struct Rule {
        #[serde(default)]
        alert: Option<String>,
        #[serde(default)]
        expr: Option<String>,
        #[serde(default, rename = "for")]
        for_: Option<String>,
        #[serde(default)]
        labels: std::collections::BTreeMap<String, String>,
    }
    let doc: Doc = serde_norway::from_str(content).ok()?;
    let mut out = Vec::new();
    for g in doc.groups {
        for r in g.rules {
            let Some(alert) = r.alert.filter(|a| !a.is_empty()) else {
                continue;
            };
            out.push(CommittedRule {
                alert,
                severity: r.labels.get("severity").cloned(),
                expr: r.expr,
                for_: r.for_,
                group_interval: g.interval.clone(),
            });
        }
    }
    Some(out)
}

/// The marker a `for:` literal that stands in for a config gauge carries.
pub const FOR_ANCHOR_MARKER: &str = "ANCHOR (DRY):";

/// The alerts whose `for:` line is ANCHORED: `ANCHOR (DRY):` inline on the
/// `for:` line, or in the contiguous comment lines immediately above it.
///
/// STRUCTURAL by design (test review, Gate 1): an `ANCHOR (DRY):` elsewhere in a
/// rule's ADR-0031 block (e.g. `GCMhRegistrationPingPong` citing the
/// load-report interval in its derivation, which is not a `for:` proxy) is NOT
/// an anchored `for:`, so the closed-set check never sweeps it in.
///
/// # Errors
/// FAIL-CLOSED: an anchored `for:` that resolves to no `- alert:` name (an
/// orphan, or an alert written in a form this line scan does not read), or a
/// name the YAML parse ([`committed_rules`]) does not know — the line scan and
/// the parse must agree, never silently differ.
pub fn anchored_for_alerts(content: &str) -> Result<BTreeSet<String>, String> {
    let lines: Vec<&str> = content.lines().collect();
    let mut current: Option<String> = None;
    let mut out = BTreeSet::new();
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim_start();
        if let Some(name) = t.strip_prefix("- alert:") {
            current = Some(name.trim().to_string());
            continue;
        }
        if !t.starts_with("for:") {
            continue;
        }
        let inline = t
            .split_once('#')
            .is_some_and(|(_, c)| c.contains(FOR_ANCHOR_MARKER));
        let above = lines[..i]
            .iter()
            .rev()
            .take_while(|l| l.trim_start().starts_with('#'))
            .any(|l| l.contains(FOR_ANCHOR_MARKER));
        if inline || above {
            let name = current.clone().ok_or_else(|| {
                format!(
                    "anchored `for:` at line {} resolves to no `- alert:` name",
                    i + 1
                )
            })?;
            out.insert(name);
        }
    }
    let parsed: BTreeSet<String> = committed_rules(content)
        .ok_or("the rules YAML does not parse")?
        .into_iter()
        .map(|r| r.alert)
        .collect();
    if let Some(stray) = out.iter().find(|n| !parsed.contains(*n)) {
        return Err(format!(
            "anchored `for:` resolved to `{stray}`, which the YAML parse does not declare"
        ));
    }
    Ok(out)
}

/// A loaded alerting rule's timing as Prometheus evaluates it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoadedTiming {
    /// The rule's `duration` (its `for:`), seconds; `None` if absent.
    pub for_seconds: Option<f64>,
    /// The group's evaluation `interval`, seconds; `None` if absent.
    pub group_interval_seconds: Option<f64>,
}

/// Every loaded alerting rule's [`LoadedTiming`] from a `/api/v1/rules` body,
/// keyed by alert name. `None` if the body is not a successful envelope.
#[must_use]
pub fn loaded_rule_timings(body: &str) -> Option<std::collections::BTreeMap<String, LoadedTiming>> {
    Some(
        loaded_alerting_rules(body)?
            .into_iter()
            .map(|r| {
                let t = LoadedTiming {
                    for_seconds: r.rule.get("duration").and_then(serde_json::Value::as_f64),
                    group_interval_seconds: r.group_interval_seconds,
                };
                (r.name, t)
            })
            .collect(),
    )
}

/// Every range window (`[5m]`) in `expr`, in source order.
///
/// # Errors
/// An unterminated range, a subquery (`[1h:5m]`, outside the subset the rule
/// files use), or a window [`crate::fixtures::metrics::parse_prometheus_duration`]
/// rejects.
pub fn range_windows(expr: &str) -> Result<Vec<std::time::Duration>, String> {
    let bytes = expr.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(&b) = bytes.get(i) {
        match b {
            b'"' | b'\'' | b'`' => i = scan_string(bytes, i)?.0,
            b'[' => {
                let end = bytes[i..]
                    .iter()
                    .position(|c| *c == b']')
                    .ok_or_else(|| format!("unterminated range at byte {i}"))?;
                let raw = &expr[i + 1..i + end];
                if raw.contains(':') {
                    return Err(format!("subquery `[{raw}]` is outside the scanned subset"));
                }
                out.push(
                    crate::fixtures::metrics::parse_prometheus_duration(raw)
                        .ok_or_else(|| format!("unparseable range `[{raw}]`"))?,
                );
                i += end + 1;
            }
            _ => i += 1,
        }
    }
    Ok(out)
}

/// The alternatives of a regex matcher value that is a PURE literal
/// alternation (`a|b|c`, every arm `[A-Za-z0-9_]+`). `None` for anything with
/// a regex metacharacter, which must NOT be split (`a.*|b`, `(a|b)`), or for a
/// single literal (no `|`).
#[must_use]
pub fn literal_alternatives(value: &str) -> Option<Vec<String>> {
    if !value.contains('|') {
        return None;
    }
    let arms: Vec<&str> = value.split('|').collect();
    arms.iter()
        .all(|a| !a.is_empty() && a.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_'))
        .then(|| arms.iter().map(|a| (*a).to_string()).collect())
}

/// The APPLY probes for one selector: one `name{...}` per combination of its
/// literal alternations (`outcome=~"a|b"` becomes `outcome="a"` and
/// `outcome="b"`), because a `count()` over the whole regex stays green when one
/// arm is misspelled. NEGATED matchers are dropped (a negated set is
/// legitimately empty on a healthy cluster) but every POSITIVE matcher is kept,
/// so `gc_mh_available{state="healthy", region!="other"}` probes
/// `gc_mh_available{state="healthy"}` and a misspelled `state` still fails.
/// Only a selector with no positive matcher left probes its bare NAME. A
/// selector with only plain matchers probes its own text.
#[must_use]
pub fn apply_probes(sel: &VectorSelector, expr: &str) -> Vec<String> {
    let positive: Vec<&LabelMatcher> = sel.matchers.iter().filter(|m| !m.op.is_negated()).collect();
    let has_alternation = positive
        .iter()
        .any(|m| m.op == MatchOp::Re && literal_alternatives(&m.value).is_some());
    if positive.len() == sel.matchers.len() && !has_alternation {
        return vec![sel.text(expr).to_string()];
    }
    if positive.is_empty() {
        return vec![sel.name.clone()];
    }
    let mut combos: Vec<Vec<String>> = vec![Vec::new()];
    for m in positive {
        let choices: Vec<String> = match (m.op, literal_alternatives(&m.value)) {
            (MatchOp::Re, Some(arms)) => arms
                .iter()
                .map(|a| format!("{}=\"{a}\"", m.label))
                .collect(),
            _ => vec![format!("{}{}\"{}\"", m.label, m.op.as_str(), m.value)],
        };
        combos = combos
            .into_iter()
            .flat_map(|c| {
                choices.iter().map(move |ch| {
                    let mut n = c.clone();
                    n.push(ch.clone());
                    n
                })
            })
            .collect();
    }
    combos
        .into_iter()
        .map(|c| format!("{}{{{}}}", sel.name, c.join(", ")))
        .collect()
}

/// Round `d` UP to a whole minute (an exact minute stays put; zero stays zero).
#[must_use]
pub fn ceil_to_minute(d: std::time::Duration) -> std::time::Duration {
    let s = d.as_secs() + u64::from(d.subsec_nanos() > 0);
    std::time::Duration::from_secs(s.div_ceil(60) * 60)
}

/// The verdict of one anchored `for:` against its derivation (the ONE
/// convention: `for = ceil_to_minute(sum of the sourced terms)`, so a correct
/// `for:` sits in `[derived, derived + 60 s)`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForVerdict {
    /// `derived <= for < derived + 60 s`.
    Ok,
    /// `for < derived`: the UNSAFE direction — the alert fires before the
    /// mechanism it waits on (replacement, staleness, owner silence) could have
    /// repaired the condition.
    TooShort,
    /// `for >= derived + 60 s`: stale upward (a config value dropped and the
    /// literal was not re-derived), so the alert is later than it needs to be.
    TooLong,
}

/// PURE: [`ForVerdict`] of `for_` against `derived`.
#[must_use]
pub fn for_verdict(for_: std::time::Duration, derived: std::time::Duration) -> ForVerdict {
    if for_ < derived {
        ForVerdict::TooShort
    } else if for_ >= derived + std::time::Duration::from_secs(60) {
        ForVerdict::TooLong
    } else {
        ForVerdict::Ok
    }
}

/// The selectors in `expr` that read PER-POD MH liveness: `up` with a `job`
/// matcher naming `mh-service`, or `kube_pod_container_status_restarts_total`
/// with any matcher naming `mh-service`. R-35: no PAGE may select these, since
/// a single MH death is routine and healed by replacement. Structural (the
/// selector scanner), so spacing or an extra matcher cannot evade it.
///
/// RESIDUAL (stated, not hidden): `=` and regex matchers whose value CONTAINS
/// `mh-service` are covered; a regex that matches `mh-service` without
/// containing it (`job=~"mh-.*"`) is not. Regex evaluation is fenced to
/// canonical homes by `clippy.toml`, so this kernel does not evaluate one.
///
/// # Errors
/// As [`vector_selectors`].
pub fn per_pod_mh_liveness_selectors(expr: &str) -> Result<Vec<String>, String> {
    Ok(vector_selectors(expr)?
        .into_iter()
        .filter(|s| {
            let names_mh = |label: Option<&str>| {
                s.matchers.iter().any(|m| {
                    !m.op.is_negated()
                        && label.is_none_or(|l| m.label == l)
                        && m.value.contains("mh-service")
                })
            };
            (s.name == "up" && names_mh(Some("job")))
                || (s.name == "kube_pod_container_status_restarts_total" && names_mh(None))
        })
        .map(|s| s.text(expr).to_string())
        .collect())
}

/// The rule files actually MOUNTED in-cluster (the `configMapGenerator`
/// list), each with its content: THE on-disk walk every rule reader in this
/// crate builds on.
///
/// # Errors
/// COULD-NOT-EVALUATE reasons: the kustomization or a mounted file cannot be
/// read, or no rule-file entry is found — an empty mounted set would make
/// every assertion built on it pass vacuously, so it is a broken extraction,
/// never a clean result.
pub fn mounted_rule_file_contents() -> Result<Vec<(String, String)>, String> {
    let root = crate::repo_root();
    let kust_path = root.join(RULES_KUSTOMIZATION);
    let kust = std::fs::read_to_string(&kust_path)
        .map_err(|e| format!("cannot read {}: {e}", kust_path.display()))?;
    let mounted = mounted_rules_files(&kust).ok_or_else(|| {
        format!(
            "no `configMapGenerator` rule-file entries found in {RULES_KUSTOMIZATION}. An empty \
             mounted set makes every assertion built on it pass vacuously, so this is a \
             broken-extraction failure, NOT a clean result."
        )
    })?;
    mounted
        .into_iter()
        .map(|f| {
            let path = root.join(RULES_DIR).join(&f);
            std::fs::read_to_string(&path)
                .map(|c| (f.clone(), c))
                .map_err(|e| {
                    format!(
                        "{RULES_KUSTOMIZATION} mounts {f}, but it cannot be read at {}: {e}",
                        path.display()
                    )
                })
        })
        .collect()
}

/// Every committed rule across the mounted files (with its file), and the
/// anchored-`for:` set.
#[derive(Debug, Clone)]
pub struct OnDiskRules {
    /// `(file, rule)` in file order.
    pub rules: Vec<(String, CommittedRule)>,
    /// [`anchored_for_alerts`] over every mounted file.
    pub anchored: BTreeSet<String>,
}

/// [`OnDiskRules`] for the mounted rule files.
///
/// # Errors
/// As [`mounted_rule_file_contents`], plus a file that does not parse as
/// alert rules, or an [`anchored_for_alerts`] error.
pub fn on_disk_rules() -> Result<OnDiskRules, String> {
    let mut rules = Vec::new();
    let mut anchored = BTreeSet::new();
    for (file, content) in mounted_rule_file_contents()? {
        let parsed = committed_rules(&content)
            .ok_or_else(|| format!("{file} did not parse as alert-rules YAML"))?;
        rules.extend(parsed.into_iter().map(|r| (file.clone(), r)));
        anchored.extend(anchored_for_alerts(&content).map_err(|e| format!("{file}: {e}"))?);
    }
    Ok(OnDiskRules { rules, anchored })
}

/// The story-4 page set (R-35). Positive control for [`page_posture`]: each
/// must be SEEN as a page rule (containment only; other pages, e.g. `MCDown`,
/// legitimately exist).
pub const STORY_4_PAGES: &[&str] = &[
    "MCMediaMeetingsAwaitingHandler",
    "MCMediaGenerationDivergence",
    "MHDown",
    "GCMhRegistrationPingPong",
];

/// The rules whose `for:` stands in for a config gauge (`ANCHOR (DRY):` on the
/// `for:` line). The anchored set on disk must EQUAL this (closed): a Layer-4
/// unit test checks it statically, and env-test 33 derives each live.
pub const ANCHORED_FOR_RULES: &[&str] = &[
    "MCMediaMeetingsAwaitingHandler",
    "GCMhAddressConflictPersisting",
    "MHMeetingsHeldWithoutClients",
];

/// Home of `ADMISSION_WINDOW_BUCKET`, the MH session actor's rotation tick
/// that republishes `mh_media_meetings_without_clients` (a term of
/// `MHMeetingsHeldWithoutClients`' `for:`).
pub const MH_ADMISSION_SRC: &str = "crates/mh-service/src/session/admission.rs";
/// Home of `BACKOFF_MAX`, MH's registration-retry backoff cap (a term of
/// `GCMhAddressConflictPersisting`' `for:`).
pub const MH_GC_CLIENT_SRC: &str = "crates/mh-service/src/grpc/gc_client.rs";

/// PURE page posture of a rule set: the page rules seen, and every page-rule
/// selector that reads per-pod MH liveness ([`per_pod_mh_liveness_selectors`]).
/// Only `severity: page` is inspected; the same selector at warning is not a
/// finding.
///
/// # Errors
/// A page rule with no expr, or an expr the scanner cannot read.
pub fn page_posture(rules: &[CommittedRule]) -> Result<(BTreeSet<String>, Vec<String>), String> {
    let mut pages = BTreeSet::new();
    let mut hits = Vec::new();
    for r in rules
        .iter()
        .filter(|r| r.severity.as_deref() == Some("page"))
    {
        pages.insert(r.alert.clone());
        let expr = r
            .expr
            .as_deref()
            .ok_or_else(|| format!("page rule `{}` has no expr", r.alert))?;
        let found = per_pod_mh_liveness_selectors(expr)
            .map_err(|e| format!("`{}` expr did not scan: {e}", r.alert))?;
        hits.extend(found.into_iter().map(|s| format!("{}: {s}", r.alert)));
    }
    Ok((pages, hits))
}

#[cfg(test)]
mod tests {

    #[test]
    fn match_op_as_str_round_trips_through_the_parser() {
        for op in [
            super::MatchOp::Eq,
            super::MatchOp::Ne,
            super::MatchOp::Re,
            super::MatchOp::Nre,
        ] {
            let expr = format!("m{{a{}\"v\"}}", op.as_str());
            let sels = super::vector_selectors(&expr).unwrap();
            assert_eq!(sels[0].matchers[0].op, op, "{expr}");
        }
    }

    #[test]
    fn apply_probes_keep_positive_matchers_beside_a_negation() {
        let expr = r#"x{a="1", b!="2"} or y{a=~"p|q", b!~"z"}"#;
        let sels = super::vector_selectors(expr).unwrap();
        assert_eq!(
            super::apply_probes(&sels[0], expr),
            vec![r#"x{a="1"}"#.to_string()]
        );
        assert_eq!(
            super::apply_probes(&sels[1], expr),
            vec![r#"y{a="p"}"#.to_string(), r#"y{a="q"}"#.to_string()]
        );
        // MHDown's shape: the positive `state` survives.
        let md = r#"max by (region) (gc_mh_available{state="healthy", region!="other"}) == 0"#;
        let s = super::vector_selectors(md).unwrap();
        assert_eq!(
            super::apply_probes(&s[0], md),
            vec![r#"gc_mh_available{state="healthy"}"#.to_string()]
        );
    }

    #[test]
    fn anchored_for_orphan_or_unparsed_name_is_an_error() {
        let orphan = "groups:\n  - name: g\n    rules: []\n# ANCHOR (DRY): x\nfor: 3m\n";
        assert!(super::anchored_for_alerts(orphan).is_err());
        assert!(super::anchored_for_alerts("not: [yaml").is_err());
    }

    #[test]
    fn page_posture_flags_liveness_only_at_page() {
        let yaml = r#"groups:
  - name: g
    rules:
      - alert: PagedPerPod
        expr: up{job="mh-service"} == 0
        labels:
          severity: page
      - alert: WarnedPerPod
        expr: up{job="mh-service"} == 0
        labels:
          severity: warning
"#;
        let rules = super::committed_rules(yaml).unwrap();
        let (pages, hits) = super::page_posture(&rules).unwrap();
        assert_eq!(
            pages.into_iter().collect::<Vec<_>>(),
            vec!["PagedPerPod".to_string()]
        );
        assert_eq!(hits.len(), 1, "page flagged, warning not: {hits:?}");
        assert!(hits[0].starts_with("PagedPerPod:"));
    }

    // ---- story 4: the REAL committed files (Layer 4, no cluster) -----------

    #[test]
    fn committed_anchored_for_set_is_closed() {
        let on_disk = super::on_disk_rules().unwrap_or_else(|e| panic!("COULD-NOT-EVALUATE: {e}"));
        let want: std::collections::BTreeSet<String> = super::ANCHORED_FOR_RULES
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        assert!(
            !on_disk.anchored.is_empty(),
            "positive control: anchors found"
        );
        assert_eq!(
            on_disk.anchored, want,
            "FOR-ANCHOR-SET: the rules whose `for:` carries `ANCHOR (DRY):` must equal \
             ANCHORED_FOR_RULES (add a derivation row in env-test 33 for a new one)"
        );
    }

    #[test]
    fn no_committed_page_rule_selects_per_pod_mh_liveness() {
        let on_disk = super::on_disk_rules().unwrap_or_else(|e| panic!("COULD-NOT-EVALUATE: {e}"));
        let rules: Vec<_> = on_disk.rules.into_iter().map(|(_, r)| r).collect();
        let (pages, hits) = super::page_posture(&rules).unwrap();
        for name in super::STORY_4_PAGES {
            assert!(
                pages.contains(*name),
                "positive control: `{name}` seen as a page rule"
            );
        }
        assert!(
            hits.is_empty(),
            "R-35 VIOLATION: a page rule selects per-pod MH liveness, so a single MH death \
             (routine, healed by replacement) would page:\n{}",
            hits.join("\n")
        );
    }

    #[test]
    fn derivation_source_consts_are_readable() {
        for (src, name) in [
            (super::MH_GC_CLIENT_SRC, "BACKOFF_MAX"),
            (super::MH_ADMISSION_SRC, "ADMISSION_WINDOW_BUCKET"),
        ] {
            let text = std::fs::read_to_string(crate::repo_root().join(src))
                .unwrap_or_else(|e| panic!("COULD-NOT-EVALUATE: read {src}: {e}"));
            crate::fixtures::gc_fleet::const_duration_secs(&text, name)
                .unwrap_or_else(|e| panic!("{e} (in {src})"));
        }
    }

    // ---- story 4 (OBS-ALERTS) kernels --------------------------------------

    const ANCHOR_YAML: &str = r#"groups:
  - name: g-page
    interval: 30s
    rules:
      # Derivation block. ANCHOR (DRY): some_interval_seconds (not a for: proxy)
      - alert: PingPongShaped
        expr: |
          sum(increase(a_total[10m])) >= 20
        for: 10m
        labels:
          severity: page
      - alert: AnchoredAbove
        expr: |
          max(b) > 0
        # ANCHOR (DRY): b_bound_seconds + scrape + eval
        for: 3m
        labels:
          severity: page
  - name: g-warn
    interval: 60s
    rules:
      - alert: AnchoredInline
        expr: |
          max(c) > 0
        for: 17m # ANCHOR (DRY): c_timeout_seconds
        labels:
          severity: warning
      - alert: Unanchored
        expr: |
          max(d) > 0
        for: 5m
        labels:
          severity: warning
"#;

    #[test]
    fn anchored_for_is_structural_not_anywhere_in_the_block() {
        let set = super::anchored_for_alerts(ANCHOR_YAML).unwrap();
        assert_eq!(
            set.into_iter().collect::<Vec<_>>(),
            vec!["AnchoredAbove".to_string(), "AnchoredInline".to_string()],
            "an ANCHOR in a derivation block (PingPong-shaped) is NOT an anchored for:"
        );
    }

    #[test]
    fn committed_rules_carry_severity_for_and_group_interval() {
        let rules = super::committed_rules(ANCHOR_YAML).expect("parses");
        let a = rules.iter().find(|r| r.alert == "AnchoredInline").unwrap();
        assert_eq!(a.severity.as_deref(), Some("warning"));
        assert_eq!(a.for_.as_deref(), Some("17m"));
        assert_eq!(a.group_interval.as_deref(), Some("60s"));
        assert_eq!(rules.len(), 4);
        assert!(super::committed_rules("groups: [").is_none());
    }

    #[test]
    fn loaded_rule_timings_reads_duration_and_group_interval() {
        let body = r#"{"status":"success","data":{"groups":[{"name":"g","interval":60,
            "rules":[{"type":"alerting","name":"A","duration":240,"query":"x"},
                     {"type":"alerting","name":"B","query":"y"},
                     {"type":"recording","name":"r","query":"z"}]}]}}"#;
        let t = super::loaded_rule_timings(body).unwrap();
        assert_eq!(t["A"].for_seconds, Some(240.0));
        assert_eq!(t["A"].group_interval_seconds, Some(60.0));
        assert_eq!(
            t["B"].for_seconds, None,
            "missing duration is None, never 0"
        );
        assert!(!t.contains_key("r"));
        assert!(super::loaded_rule_timings(r#"{"status":"error"}"#).is_none());
    }

    #[test]
    fn range_windows_parse_and_refuse_subqueries() {
        use std::time::Duration;
        assert_eq!(
            super::range_windows(r#"sum(increase(x{a="[q]"}[1m])) > 0 and rate(y[1h30m]) > 0"#)
                .unwrap(),
            vec![Duration::from_secs(60), Duration::from_secs(5400)]
        );
        assert!(super::range_windows("max_over_time(x[1h:5m])").is_err());
        assert!(super::range_windows("rate(x[5q])").is_err());
        assert!(super::range_windows("rate(x[5m)").is_err());
        assert!(super::range_windows("max(x) > 0").unwrap().is_empty());
    }

    #[test]
    fn literal_alternatives_split_only_pure_literals() {
        assert_eq!(
            super::literal_alternatives("no_capacity|gc_unavailable"),
            Some(vec![
                "no_capacity".to_string(),
                "gc_unavailable".to_string()
            ])
        );
        assert_eq!(super::literal_alternatives("single"), None);
        assert_eq!(
            super::literal_alternatives("a.*|b"),
            None,
            "metachar: never split"
        );
        assert_eq!(super::literal_alternatives("(a|b)"), None);
        assert_eq!(super::literal_alternatives("a||b"), None);
    }

    #[test]
    fn apply_probes_expand_alternations_and_keep_negations_bare() {
        let expr = r#"sum(increase(m_total{outcome=~"a|b", trigger="t"}[5m])) unless n{x!="y"} or p{k="v"}"#;
        let sels = super::vector_selectors(expr).unwrap();
        assert_eq!(
            super::apply_probes(&sels[0], expr),
            vec![
                r#"m_total{outcome="a", trigger="t"}"#.to_string(),
                r#"m_total{outcome="b", trigger="t"}"#.to_string()
            ]
        );
        // Negated-only: no positive matcher left, so the bare name.
        assert_eq!(super::apply_probes(&sels[1], expr), vec!["n".to_string()]);
        assert_eq!(
            super::apply_probes(&sels[2], expr),
            vec![r#"p{k="v"}"#.to_string()]
        );
    }

    #[test]
    fn ceil_to_minute_boundaries() {
        use std::time::Duration;
        assert_eq!(super::ceil_to_minute(Duration::ZERO), Duration::ZERO);
        assert_eq!(
            super::ceil_to_minute(Duration::from_secs(180)),
            Duration::from_secs(180)
        );
        assert_eq!(
            super::ceil_to_minute(Duration::from_secs(181)),
            Duration::from_secs(240)
        );
        assert_eq!(
            super::ceil_to_minute(Duration::from_millis(180_001)),
            Duration::from_secs(240)
        );
        assert_eq!(
            super::ceil_to_minute(Duration::from_secs(174)),
            Duration::from_secs(180)
        );
    }

    #[test]
    fn for_verdict_both_sides() {
        use super::ForVerdict;
        use std::time::Duration;
        let derived = Duration::from_secs(185);
        assert_eq!(
            super::for_verdict(Duration::from_secs(240), derived),
            ForVerdict::Ok
        );
        assert_eq!(super::for_verdict(derived, derived), ForVerdict::Ok);
        assert_eq!(
            super::for_verdict(derived - Duration::from_secs(1), derived),
            ForVerdict::TooShort
        );
        assert_eq!(
            super::for_verdict(derived + Duration::from_secs(60), derived),
            ForVerdict::TooLong
        );
    }

    #[test]
    fn per_pod_mh_liveness_is_structural() {
        // Positive controls: the retired MHDown expr, spaced, and with an extra matcher.
        assert_eq!(
            super::per_pod_mh_liveness_selectors(r#"up{job="mh-service"} == 0"#)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            super::per_pod_mh_liveness_selectors(r#"up{ job = "mh-service", x="y" } == 0"#)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            super::per_pod_mh_liveness_selectors(
                r#"rate(kube_pod_container_status_restarts_total{container="mh-service"}[1h]) > 0.016"#
            )
            .unwrap()
            .len(),
            1
        );
        // Benign: the re-scoped fleet expr and other jobs.
        assert!(super::per_pod_mh_liveness_selectors(
            r#"max by (region) (gc_mh_available{state="healthy", region!="other"}) == 0"#
        )
        .unwrap()
        .is_empty());
        assert!(
            super::per_pod_mh_liveness_selectors(r#"up{job="mc-service"} == 0"#)
                .unwrap()
                .is_empty()
        );
    }

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
