//! `ts-name-guard-dt-client` subcommand — port of
//! `scripts/guards/simple/ts/name-guard-dt-client.sh`.
//!
//! R-26 client metric-name convention (per @observability) — the COMPILED,
//! authoritative form (see `R26_NAME_RE` below):
//! ```text
//! ^dt_client_[a-z]([a-z0-9_]{0,52}[a-z0-9])?$
//! ```
//! Literal prefix `dt_client_`; lowercase-letter start (no leading digit/
//! underscore); snake_case body; NO trailing underscore; max total length 64
//! (Prometheus/OTel default; per @observability S4 this is a HARD FAIL — the
//! regex itself encodes the cap, matching
//! `metric_labels.rs:MAX_LITERAL_VALUE_LENGTH`). NOTE: an older `{0,53}`
//! shorthand appears in some docs; it is NOT equivalent — `{0,53}` would accept
//! a trailing underscore (`dt_client_foo_`), which the compiled form REJECTS
//! (see `r26_rejects_trailing_underscore`). The compiled form is authoritative.
//!
//! Scope: **POSITIVE include-list** — `packages/(sdk-core|web-app)/src/**`.
//! Other packages are out-of-scope by definition (not by exemption). Per
//! @paired-client task-#37 ruling sdk-svelte stays deferred.

use crate::common::explain::{print_finding, Finding};
use crate::common::git_changes::get_all_changed_files;
use crate::common::kind_labels::{KIND_LABEL_KEYS, TS_PROJECTION_HOME};
use crate::common::status::emit_ok;
use crate::common::ts_lex::{lex, skip_ws, Lexed};
use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use std::path::{Path, PathBuf};

pub const METRIC_NAME_RULE_ID: &str = "metric_name";
/// A `media_kind` / `content_kind` value spelled as a literal outside the TS
/// projection home (`common::kind_labels::TS_PROJECTION_HOME`).
pub const KIND_LABEL_LITERAL_RULE_ID: &str = "kind_label_literal";
/// The kind-literal rule could not evaluate its input (home missing or holding
/// no value literals). Never "fixed" by loosening the content rule.
pub const KIND_LABEL_VACUOUS_RULE_ID: &str = "kind_label_vacuous";

const EXTENSIONS: &[&str] = &[".ts", ".tsx", ".svelte"];

const SCOPE_PREFIXES: &[&str] = &["packages/sdk-core/src/", "packages/web-app/src/"];

/// Extra scope for the KIND-LITERAL rule ONLY, never the R-26 name rule: the
/// acceptance harness builds PromQL read-backs here (`mcMetrics.ts`,
/// `clientMetricNames.ts`), and a hand-written `media_kind="audio"` there is
/// exactly the drift the rule closes. Kept out of [`SCOPE_PREFIXES`] on
/// purpose: adding it there would silently widen metric-NAME checking to e2e.
const KIND_LITERAL_EXTRA_SCOPE: &[&str] = &["packages/web-app/e2e/"];

const EXCLUDED_PATH_PATTERNS: &[&str] = &["/node_modules/", "/dist/", "/__tests__/"];

const EXCLUDED_FILE_SUFFIXES: &[&str] =
    &[".d.ts", ".test.ts", ".spec.ts", ".test.tsx", ".spec.tsx"];

/// Opener for `.createCounter(` / `.createHistogram(` / etc. The match ends
/// at the `(`; a byte-walker then scans across newlines for the first string
/// literal inside the call.
///
/// Captures: group 1 = factory method name (informational; not currently
/// surfaced in findings but useful for future per-kind classification).
///
/// Per @team-lead 2026-05-23: multi-line `meter.createCounter(\n  "name",\n  {...}\n)`
/// invocations are matched via the balanced-paren walker below — same shape as
/// `metric_labels::find_macro_invocations`. The opener is single-line by
/// design (the `.createX(` token must appear on one line by real-world TS
/// formatting conventions); the BODY of the call is walked across lines.
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static METER_OPENER_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"\.(createCounter|createHistogram|createUpDownCounter|createGauge|createObservableCounter|createObservableGauge|createObservableUpDownCounter)\s*\(",
    )
    .expect("static pattern compiles")
});

/// R-26 metric-name regex (the COMPILED, authoritative form). Per @observability
/// S4 the length cap is a HARD FAIL the regex itself encodes: the `dt_client_`
/// prefix (10 chars) plus a `[a-z]` head, up to a `{0,52}` body, and a non-`_`
/// `[a-z0-9]` tail caps the total at 64 chars, and the required non-underscore
/// tail FORBIDS a trailing `_`. (An older `{0,53}` shorthand in some docs is NOT
/// equivalent: it would accept a trailing underscore, which this compiled form
/// rejects. See `r26_rejects_trailing_underscore` and the cross-language lock
/// `r26_pattern_is_locked_to_the_sdk_runtime_guard`.)
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static R26_NAME_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^dt_client_[a-z]([a-z0-9_]{0,52}[a-z0-9])?$").expect("static pattern compiles")
});

// ---------------------------------------------------------------------------
// `// dt-metric-name-dynamic:` escape hatch (task #12, R-24).
//
// ISOLATED ADDITION: the canonical generic `MetricsSink`
// (`packages/sdk-core/src/telemetry/OtelMetricsSink.ts`) forwards a VARIABLE
// metric name to `meter.createX(name)` because it is the one legitimate
// dynamic caller — and it validates the name at runtime via
// `assertClientMetricName(name, mode)` (the SAME `dt_client_*` regex this guard
// enforces statically) on the line above each call. Without an escape hatch the
// byte-walker reports that variable as a `None` (non-literal) name and hard-
// fails. This marker lets that ONE runtime-validated case opt out.
//
// SAFETY INVARIANT (load-bearing): the marker rescues ONLY the dynamic /
// non-literal case. A LITERAL name that fails `R26_NAME_RE` (e.g.
// `createCounter("mc_join_total")`) STILL hard-fails even with the marker
// present — so the guard's real job (catch bad literals at call sites) is
// untouched. See `scan_file` + tests.
//
// Reuses the shared reason-quality bar (`crate::ignore::is_lazy_reason`,
// ≥10 chars + not test/tmp/todo/fixme/wip) — NOT a forked bar — and the
// `pii-safe` "on or above the call's opener line" placement convention, so
// there is one mental model across escape hatches. This block is deliberately
// self-contained so the guard-mechanism change can be spun out to an
// infra-owned follow-up if Gate-3 asks, leaving the annotated sink lines.
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static DYNAMIC_NAME_MARKER_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"//\s*dt-metric-name-dynamic\s*:\s*(.+?)\s*$").expect("static pattern compiles")
});

/// Parse `// dt-metric-name-dynamic: <reason>` markers from `src`. Returns the
/// 1-based line numbers carrying a marker with a NON-lazy reason (lazy reasons
/// are ignored, so they do not grant an exemption — mirrors the `pii-safe`
/// marker's reason-quality gate). A bare or too-vague marker grants nothing.
fn load_dynamic_name_marker_lines(src: &str) -> std::collections::HashSet<usize> {
    let mut lines = std::collections::HashSet::new();
    for (idx, line) in src.lines().enumerate() {
        let Some(caps) = DYNAMIC_NAME_MARKER_RE.captures(line) else {
            continue;
        };
        let Some(reason) = caps.get(1) else {
            continue;
        };
        // Reuse the shared bar — a lazy/short reason does NOT grant exemption.
        if crate::ignore::is_lazy_reason(reason.as_str().trim()) {
            continue;
        }
        lines.insert(idx + 1);
    }
    lines
}

/// True if the dynamic-name call whose opener is on `start_lineno` carries a
/// valid marker ON that line or the line directly ABOVE (the `pii-safe`
/// "on or above" convention).
fn dynamic_name_exempt(
    marker_lines: &std::collections::HashSet<usize>,
    start_lineno: usize,
) -> bool {
    if marker_lines.contains(&start_lineno) {
        return true;
    }
    start_lineno > 1 && marker_lines.contains(&(start_lineno - 1))
}

fn is_in_scope(path: &Path) -> bool {
    is_in_scope_of(path, SCOPE_PREFIXES)
}

/// The same exclusions as the name rule (tests, specs, `dist`, declarations),
/// over an explicit prefix list.
fn is_in_scope_of(path: &Path, prefixes: &[&str]) -> bool {
    let Some(s) = path.to_str() else {
        return false;
    };
    if EXCLUDED_PATH_PATTERNS.iter().any(|pat| s.contains(pat)) {
        return false;
    }
    if EXCLUDED_FILE_SUFFIXES.iter().any(|suf| s.ends_with(suf)) {
        return false;
    }
    prefixes.iter().any(|prefix| s.starts_with(prefix))
}

/// Is `path` in the KIND-LITERAL rule's scope: the name rule's scope plus
/// [`KIND_LITERAL_EXTRA_SCOPE`].
fn is_in_kind_scope(path: &Path) -> bool {
    is_in_scope_of(path, SCOPE_PREFIXES) || is_in_scope_of(path, KIND_LITERAL_EXTRA_SCOPE)
}

// ---------------------------------------------------------------------------
// Kind-label literal rule (2026-10-26; label-taxonomy.md §"Carried:
// `media_kind` and `content_kind`"). The values of the two kind labels are
// spelled ONLY in the TS projection home; every other site reads them from
// `projectMediaKind` / `MEDIA_KIND_LABEL_PROJECTION`.
//
// Two literal positions, both read through `common::ts_lex` (comments
// stripped, string spans known), so a literal in a comment is green:
//  * OBJECT form — a kind key (bare identifier or quoted) outside any string,
//    followed by `:`, `=`, `==` or `===` / `!=` / `!==`, then a string or
//    template literal: `{ media_kind: 'audio' }`, `l.content_kind === 'main'`.
//  * PROMQL form — inside a string or template literal, a kind key followed by
//    `=`, `!=`, `=~` or `!~` and a `"` whose next char is a letter:
//    `media_kind="audio"`. `media_kind="${MEDIA_KIND_AUDIO}"` is green (the
//    value comes from the projection through a constant).
// ---------------------------------------------------------------------------

/// One kind-literal hit: 1-based line and the key it was spelled for.
#[derive(Debug, Clone, PartialEq, Eq)]
struct KindHit {
    line: usize,
    key: &'static str,
}

/// Length of the comparison/assignment operator at `i` in the OBJECT form.
fn object_op_len(text: &str, i: usize) -> Option<usize> {
    let rest = text.get(i..)?;
    ["===", "!==", "==", "!=", ":", "="]
        .iter()
        .find(|op| rest.starts_with(**op) && !rest.starts_with("::") && !rest.starts_with("=>"))
        .map(|op| op.len())
}

/// Every kind-literal hit in TS source, over the given key set (the seam for
/// the neutered-matcher proof).
fn kind_literal_hits(src: &str, keys: &[&'static str]) -> Vec<KindHit> {
    let lexed: Lexed = lex(src);
    let text = &lexed.text;
    let mut hits = Vec::new();
    for key in keys {
        for (pos, _) in text.match_indices(key) {
            let after_key = pos + key.len();
            let before = text.get(..pos).and_then(|t| t.chars().next_back());
            let after = text.get(after_key..).and_then(|t| t.chars().next());
            let ident_char = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '$';
            if before.is_some_and(ident_char) || after.is_some_and(ident_char) {
                continue;
            }
            match lexed.string_at(pos) {
                // OBJECT form, quoted key: the whole literal IS the key.
                Some(lit) if lit.body(text) == *key => {
                    if object_value_is_literal(&lexed, lit.end) {
                        hits.push(KindHit {
                            line: lexed.line_of(pos),
                            key,
                        });
                    }
                }
                // PROMQL form: inside a string.
                Some(_) => {
                    if promql_value_is_literal(text, after_key) {
                        hits.push(KindHit {
                            line: lexed.line_of(pos),
                            key,
                        });
                    }
                }
                // OBJECT form, bare key.
                None => {
                    if object_value_is_literal(&lexed, after_key) {
                        hits.push(KindHit {
                            line: lexed.line_of(pos),
                            key,
                        });
                    }
                }
            }
        }
    }
    hits.sort_by_key(|h| (h.line, h.key));
    hits
}

/// After a key at `from` (optionally closing a computed access `x['key']`):
/// an object operator, then a string/template literal.
fn object_value_is_literal(lexed: &Lexed, from: usize) -> bool {
    let mut i = skip_ws(&lexed.text, from);
    if lexed.text.as_bytes().get(i) == Some(&b']') {
        i = skip_ws(&lexed.text, i + 1);
    }
    let Some(op) = object_op_len(&lexed.text, i) else {
        return false;
    };
    let v = skip_ws(&lexed.text, i + op);
    lexed.string_starting_at(v).is_some()
}

/// After a key inside a string at `from`: a PromQL matcher operator, then `"`
/// and a letter (a hand-written value, not `${…}`).
fn promql_value_is_literal(text: &str, from: usize) -> bool {
    let i = skip_ws(text, from);
    let Some(rest) = text.get(i..) else {
        return false;
    };
    let op = ["=~", "!~", "!=", "="]
        .iter()
        .find(|op| rest.starts_with(**op))
        .map_or(0, |op| op.len());
    if op == 0 {
        return false;
    }
    // PromQL string values may be `"…"`, `'…'` or `` `…` `` (inside a TS
    // template a backtick arrives escaped as `` \` ``).
    let v = skip_ws(text, i + op);
    let rest = text.get(v..).unwrap_or("");
    let rest = rest.strip_prefix('\\').unwrap_or(rest);
    let mut chars = rest.chars();
    matches!(chars.next(), Some('"' | '\'' | '`'))
        && chars.next().is_some_and(|c| c.is_ascii_alphabetic())
}

/// Every in-scope TS file of the kind rule, repo-relative and sorted: the FULL
/// tree, not the changed set (a literal must not survive because its file is
/// untouched).
fn kind_scope_files(repo_root: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for prefix in SCOPE_PREFIXES.iter().chain(KIND_LITERAL_EXTRA_SCOPE) {
        let dir = repo_root.join(prefix.trim_end_matches('/'));
        if !dir.is_dir() {
            continue;
        }
        for entry in walkdir::WalkDir::new(&dir)
            .sort_by_file_name()
            .into_iter()
            .filter_entry(|e| e.file_name() != "node_modules" && e.file_name() != "dist")
        {
            // An unreadable entry is an error, never a silently smaller scope.
            let entry = entry.with_context(|| format!("walk {prefix}"))?;
            let Ok(rel) = entry.path().strip_prefix(repo_root) else {
                continue;
            };
            let rel = PathBuf::from(rel.to_string_lossy().replace('\\', "/"));
            let is_source = EXTENSIONS
                .iter()
                .any(|ext| rel.to_string_lossy().ends_with(ext));
            if entry.file_type().is_file() && is_source && is_in_kind_scope(&rel) {
                out.push(rel);
            }
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

/// One kind-rule finding: `(rule, file, line, message)`.
type KindFinding = (&'static str, String, usize, String);

/// The kind rule over the tree: its findings and the number of literals SEEN
/// in the home (the positive control: zero is vacuity).
fn kind_findings(repo_root: &Path, keys: &[&'static str]) -> Result<(Vec<KindFinding>, usize)> {
    let mut out = Vec::new();
    let home_seen = match std::fs::read_to_string(repo_root.join(TS_PROJECTION_HOME)) {
        Ok(src) => kind_literal_hits(&src, keys).len(),
        Err(_) => 0,
    };
    if home_seen == 0 {
        out.push((
            KIND_LABEL_VACUOUS_RULE_ID,
            TS_PROJECTION_HOME.to_string(),
            0,
            format!(
                "the TS kind-label projection home {TS_PROJECTION_HOME} is missing or holds no \
                 kind-label value literals; update TS_PROJECTION_HOME (common::kind_labels) in \
                 the same change that moves it"
            ),
        ));
    }
    for rel in kind_scope_files(repo_root)? {
        let rel_s = rel.to_string_lossy().to_string();
        if rel_s == TS_PROJECTION_HOME {
            continue;
        }
        let src = std::fs::read_to_string(repo_root.join(&rel))
            .with_context(|| format!("reading {rel_s}"))?;
        for hit in kind_literal_hits(&src, keys) {
            out.push((
                KIND_LABEL_LITERAL_RULE_ID,
                rel_s.clone(),
                hit.line,
                format!(
                    "`{}` value spelled as a literal; take it from the projection \
                     (`projectMediaKind` / `MEDIA_KIND_LABEL_PROJECTION` in {TS_PROJECTION_HOME})",
                    hit.key
                ),
            ));
        }
    }
    Ok((out, home_seen))
}

struct Hit {
    file: PathBuf,
    line: usize,
    name: String,
}

/// Extracted call: the literal first-arg string (or `None` if no static
/// literal — interpolated `` `dt_client_${foo}` `` returns `None`, which
/// the caller treats as a HARD FAIL since interpolated names violate R-26).
///
/// `start_line` is 1-based and refers to the source line of the OPENER
/// (`.createX(`), not the literal — bash today's same convention.
struct MeterCall {
    name: Option<String>,
    start_line: usize,
}

/// Walk `src` for `.createX(` openers; for each, byte-scan from the `(` for
/// the first string-literal arg and the matching `)`. Returns one [`MeterCall`]
/// per opener.
///
/// Multi-line shapes (split across newlines, with intervening comments / blank
/// lines / nested parens / parens-inside-literals) all collapse to the same
/// extraction. Reference shape: `metric_labels::find_macro_invocations`.
#[expect(
    clippy::indexing_slicing,
    reason = "every `bytes[i]` is bounds-checked by the enclosing `while i < bytes.len()`"
)]
fn find_meter_calls(src: &str) -> Vec<MeterCall> {
    let mut out = Vec::new();
    let bytes = src.as_bytes();
    for m in METER_OPENER_RE.find_iter(src) {
        let paren_open_idx = m.end() - 1;
        let start_lineno = count_newlines_until(src, m.start()) + 1;

        let mut i = paren_open_idx + 1;
        let mut depth: i32 = 1;
        let mut found_name: Option<String> = None;
        let mut found_first_arg = false;

        while i < bytes.len() && depth > 0 {
            let c = bytes[i];
            // Skip whitespace.
            if c.is_ascii_whitespace() {
                i += 1;
                continue;
            }
            // Line comment `//` — skip to newline.
            if c == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            // Block comment `/* ... */` — skip to `*/`.
            if c == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                if i + 1 < bytes.len() {
                    i += 2;
                }
                continue;
            }
            // String literal (`"..."`, `'...'`, or `` `...` ``). The first
            // string literal encountered IS the metric-name arg. Template
            // literals with `${...}` interpolation are NOT considered a
            // static literal — `found_name` stays `None` and the caller
            // HARD-FAILs the call (interpolation violates R-26 anyway).
            if matches!(c, b'"' | b'\'' | b'`') {
                let quote = c;
                let lit_start = i + 1;
                let mut j = lit_start;
                let mut has_interpolation = false;
                while j < bytes.len() && bytes[j] != quote {
                    if bytes[j] == b'\\' && j + 1 < bytes.len() {
                        j += 2;
                        continue;
                    }
                    if quote == b'`'
                        && bytes[j] == b'$'
                        && j + 1 < bytes.len()
                        && bytes[j + 1] == b'{'
                    {
                        has_interpolation = true;
                    }
                    j += 1;
                }
                if !found_first_arg {
                    found_first_arg = true;
                    if !has_interpolation && j > lit_start && j <= bytes.len() {
                        if let Ok(s) = std::str::from_utf8(&bytes[lit_start..j]) {
                            found_name = Some(s.to_string());
                        }
                    }
                    // else: interpolated or non-utf8 literal → leave
                    // `found_name = None`; caller HARD-FAILs.
                }
                i = j.saturating_add(1);
                continue;
            }
            // Paren balance (only count after we're past any literal — the
            // literal-handling branch above continues without touching depth).
            if c == b'(' {
                depth += 1;
            } else if c == b')' {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            i += 1;
        }

        out.push(MeterCall {
            name: found_name,
            start_line: start_lineno,
        });
    }
    out
}

/// Count newlines in `src` before byte position `pos`. Used to convert a
/// byte offset to a 1-based line number for finding output. `pos` is always
/// a regex-match start (≤ `src.len()`); use `get(..pos)` defensively rather
/// than direct slicing.
fn count_newlines_until(src: &str, pos: usize) -> usize {
    src.as_bytes()
        .get(..pos)
        .unwrap_or(&[])
        .iter()
        .filter(|&&b| b == b'\n')
        .count()
}

/// Pure per-call violation decision (filesystem-free, so it is directly
/// unit-testable). Returns `Some(surfaced_name)` if the call is a violation, or
/// `None` if it is compliant OR a marker-exempted dynamic name.
///
/// SAFETY INVARIANT (load-bearing — see the marker block above): the
/// `// dt-metric-name-dynamic:` exemption applies ONLY when the call's name is
/// non-literal (`call.name == None`). A LITERAL name that fails `R26_NAME_RE`
/// is reported as a violation even with a valid marker present.
fn call_violation(
    call: &MeterCall,
    marker_lines: &std::collections::HashSet<usize>,
) -> Option<String> {
    let name_for_check = call.name.as_deref().unwrap_or("");
    if R26_NAME_RE.is_match(name_for_check) {
        return None; // compliant literal
    }
    if call.name.is_none() && dynamic_name_exempt(marker_lines, call.start_line) {
        return None; // runtime-validated dynamic name, opted out via marker
    }
    Some(match &call.name {
        Some(n) => n.clone(),
        None => "<non-literal or interpolated metric name>".to_string(),
    })
}

fn scan_file(repo_root: &Path, path: &Path) -> Result<Vec<Hit>> {
    let abs = repo_root.join(path);
    let content =
        std::fs::read_to_string(&abs).with_context(|| format!("reading {}", abs.display()))?;
    let mut hits: Vec<Hit> = Vec::new();
    let marker_lines = load_dynamic_name_marker_lines(&content);

    for call in find_meter_calls(&content) {
        if let Some(surfaced_name) = call_violation(&call, &marker_lines) {
            hits.push(Hit {
                file: path.to_path_buf(),
                line: call.start_line,
                name: surfaced_name,
            });
        }
    }

    Ok(hits)
}

/// The run's verdict over both rules: fail ONCE naming every count, so
/// neither rule's findings mask the other's.
fn verdict(name_count: usize, kind_count: usize) -> Result<()> {
    match (name_count, kind_count) {
        (0, 0) => Ok(()),
        (n, 0) => anyhow::bail!("ts-metric-name-violation-{n}"),
        (0, k) => anyhow::bail!("ts-kind-label-violation-{k}"),
        (n, k) => anyhow::bail!("ts-metric-name-violation-{n}-and-kind-label-violation-{k}"),
    }
}

pub fn run(repo_root: &Path, explain: bool) -> Result<()> {
    // The kind-literal rule runs FIRST, over the FULL scope, independent of
    // the changed set the name rule reads — and before the scaffold exit
    // below, so a wrong root or a moved package (projection home missing)
    // reds as `kind_label_vacuous` instead of reporting OK.
    let (kind, _) = kind_findings(repo_root, &KIND_LABEL_KEYS)?;
    for (rule, file, line, message) in &kind {
        if explain {
            let policy = format!("ts-name-guard-dt-client::{rule}");
            print_finding(&Finding {
                file,
                row: *line,
                col: 0,
                policy: &policy,
                matched: message,
                extras: &[],
                src_file: file!(),
                src_line: line!(),
            });
        } else {
            println!("VIOLATION: {file}:{line} ({rule}) {message}");
        }
    }
    // Both rules always run and report; the run fails ONCE via `verdict`.
    let kind_count = kind.len();

    // Scaffold-now / fire-later: if neither target package exists yet, the
    // NAME rule has nothing to read.
    let any_target_present = SCOPE_PREFIXES
        .iter()
        .any(|prefix| repo_root.join(prefix.trim_end_matches('/')).is_dir());
    if !any_target_present {
        verdict(0, kind_count)?;
        emit_ok("ts-name-guard-dt-client-targets-absent");
        return Ok(());
    }

    let changed = get_all_changed_files(repo_root, Path::new("."), EXTENSIONS)
        .context("collecting changed TS files")?;

    let in_scope: Vec<PathBuf> = changed
        .into_iter()
        .filter(|p| is_in_scope(p))
        .filter(|p| repo_root.join(p).is_file())
        .collect();

    if in_scope.is_empty() {
        verdict(0, kind_count)?;
        emit_ok("ts-name-guard-dt-client-no-files");
        return Ok(());
    }

    let mut all_hits: Vec<Hit> = Vec::new();
    for path in &in_scope {
        let hits = scan_file(repo_root, path)?;
        all_hits.extend(hits);
    }

    if all_hits.is_empty() {
        verdict(0, kind_count)?;
        emit_ok(format!(
            "ts-name-guard-dt-client-clean-{}-files",
            in_scope.len()
        ));
        return Ok(());
    }

    for hit in &all_hits {
        let file_disp = hit.file.display().to_string();
        if explain {
            // Metric name itself is NOT secret — `print_finding` is correct
            // here. The `matched` field carries the offending name so the
            // operator can fix it.
            print_finding(&Finding {
                file: &file_disp,
                row: hit.line,
                col: 0,
                policy: "ts-name-guard-dt-client::metric_name",
                matched: &hit.name,
                extras: &[],
                src_file: file!(),
                src_line: line!(),
            });
        } else {
            // Reference `R26_NAME_RE.as_str()` directly so the operator-facing
            // message can NEVER drift from the compiled rule (was a hardcoded
            // `{0,53}` form that diverged from the authoritative `{0,52}…[a-z0-9]`).
            println!(
                "VIOLATION: {}:{} metric name {:?} does not match {}",
                file_disp,
                hit.line,
                hit.name,
                R26_NAME_RE.as_str()
            );
        }
    }

    verdict(all_hits.len(), kind_count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn r26_accepts_canonical_names() {
        assert!(R26_NAME_RE.is_match("dt_client_request_count"));
        assert!(R26_NAME_RE.is_match("dt_client_a"));
        assert!(R26_NAME_RE.is_match("dt_client_a1"));
        assert!(R26_NAME_RE.is_match("dt_client_websocket_round_trip_seconds"));
    }

    #[test]
    fn r26_rejects_missing_prefix() {
        assert!(!R26_NAME_RE.is_match("client_request_count"));
        assert!(!R26_NAME_RE.is_match("dt_server_x"));
    }

    #[test]
    fn r26_rejects_trailing_underscore() {
        // Per @observability F-OBS-1: trailing underscore is forbidden.
        assert!(!R26_NAME_RE.is_match("dt_client_bad_"));
    }

    #[test]
    fn r26_rejects_leading_digit_or_underscore() {
        assert!(!R26_NAME_RE.is_match("dt_client_1bad"));
        assert!(!R26_NAME_RE.is_match("dt_client__double"));
    }

    #[test]
    fn r26_length_cap_hard_fail() {
        // 64-char total cap: prefix `dt_client_` (10) + body (max 54).
        // Body = `[a-z]([a-z0-9_]{0,52}[a-z0-9])?` → max len 54.
        let body_54 = "a".repeat(53) + "b";
        let name_64 = format!("dt_client_{body_54}");
        assert_eq!(name_64.len(), 64);
        assert!(R26_NAME_RE.is_match(&name_64));

        // 65-char name — should reject.
        let body_55 = "a".repeat(54) + "b";
        let name_65 = format!("dt_client_{body_55}");
        assert_eq!(name_65.len(), 65);
        assert!(!R26_NAME_RE.is_match(&name_65));
    }

    #[test]
    fn r26_rejects_uppercase() {
        assert!(!R26_NAME_RE.is_match("dt_client_BadName"));
    }

    #[test]
    fn r26_pattern_is_locked_to_the_sdk_runtime_guard() {
        // CROSS-LANGUAGE LOCKSTEP (task #12, observability F2): the SDK's runtime
        // `nameGuard` (packages/sdk-core/src/telemetry/nameGuard.ts) pins THIS
        // exact pattern string, and `nameGuard.test.ts` asserts the SDK side
        // equals this literal. Asserting the Rust `R26_NAME_RE` source equals the
        // SAME literal here closes the loop: a Rust-side edit to the regex now
        // trips a RUST test (previously only the SDK-vs-its-own-literal was
        // checked, so a Rust-only edit could silently reopen the drift). If this
        // pattern legitimately changes, update BOTH this literal AND
        // nameGuard.ts's `DT_CLIENT_NAME_RE` + its test in the same commit.
        const SDK_PINNED_PATTERN: &str = r"^dt_client_[a-z]([a-z0-9_]{0,52}[a-z0-9])?$";
        assert_eq!(
            R26_NAME_RE.as_str(),
            SDK_PINNED_PATTERN,
            "Rust R26_NAME_RE drifted from the SDK nameGuard's pinned pattern; \
             update both sides + nameGuard.test.ts in the same commit"
        );
    }

    #[test]
    fn find_meter_calls_extracts_metric_name() {
        let src = r#"const c = meter.createCounter("dt_client_foo");"#;
        let calls = find_meter_calls(src);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name.as_deref(), Some("dt_client_foo"));
        assert_eq!(calls[0].start_line, 1);
    }

    #[test]
    fn opener_handles_all_factory_variants() {
        for factory in &[
            "createCounter",
            "createHistogram",
            "createUpDownCounter",
            "createGauge",
            "createObservableCounter",
            "createObservableGauge",
            "createObservableUpDownCounter",
        ] {
            let src = format!(r#"meter.{factory}("dt_client_x")"#);
            assert!(
                METER_OPENER_RE.is_match(&src),
                "factory {factory} should match"
            );
            let calls = find_meter_calls(&src);
            assert_eq!(calls.len(), 1, "factory {factory} should yield one call");
            assert_eq!(calls[0].name.as_deref(), Some("dt_client_x"));
        }
    }

    // -------------------------------------------------------------------
    // Multi-line tests — per @team-lead 2026-05-23 directive. Each test
    // names the concrete shape it exercises so future readers can audit
    // the coverage matrix.
    // -------------------------------------------------------------------

    #[test]
    fn single_line_baseline_regression() {
        // Regression: single-line invocation still works after the walker
        // port. This is the shape `meter_call_extracts_metric_name` covers,
        // re-asserted here as the baseline of the multi-line matrix.
        let src = r#"meter.createCounter("dt_client_foo", { description: "x" })"#;
        let calls = find_meter_calls(src);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name.as_deref(), Some("dt_client_foo"));
    }

    #[test]
    fn multi_line_2_line_invocation() {
        let src = "meter.createCounter(\n  \"dt_client_bar\",\n  { description: \"x\" }\n)";
        let calls = find_meter_calls(src);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name.as_deref(), Some("dt_client_bar"));
        assert_eq!(calls[0].start_line, 1);
    }

    #[test]
    fn multi_line_5_plus_lines_with_blanks() {
        let src = "\
meter.createHistogram(

  // some leading prose

  \"dt_client_baz_seconds\",

  { description: \"x\" }
)";
        let calls = find_meter_calls(src);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name.as_deref(), Some("dt_client_baz_seconds"));
    }

    #[test]
    fn block_comment_inside_call() {
        let src = "meter.createHistogram(\n  /* explanatory */\n  \"dt_client_block\"\n)";
        let calls = find_meter_calls(src);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name.as_deref(), Some("dt_client_block"));
    }

    #[test]
    fn line_comment_inside_call() {
        let src = "meter.createCounter(\n  \"dt_client_qux\", // comment\n)";
        let calls = find_meter_calls(src);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name.as_deref(), Some("dt_client_qux"));
    }

    #[test]
    fn chained_call_shape() {
        let src = r#"const c = meter.createCounter("dt_client_chain").bind({})"#;
        let calls = find_meter_calls(src);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name.as_deref(), Some("dt_client_chain"));
    }

    #[test]
    fn nested_in_conditional() {
        let src = r#"if (cond) { meter.createCounter("dt_client_in_if"); }"#;
        let calls = find_meter_calls(src);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name.as_deref(), Some("dt_client_in_if"));
    }

    #[test]
    fn parens_inside_string_literal_do_not_close_outer_call() {
        // Regression: `()` inside the string-literal arg must NOT close the
        // outer call. The walker continues past the literal correctly.
        let src = r#"meter.createCounter("dt_client_foo()bar")"#;
        let calls = find_meter_calls(src);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name.as_deref(), Some("dt_client_foo()bar"));
        // The R-26 regex then HARD-FAILs the name (parens violate snake_case body).
        assert!(!R26_NAME_RE.is_match(calls[0].name.as_deref().unwrap()));
    }

    #[test]
    fn whitespace_before_dot_factory() {
        // Chained-call newlines before the dot — `.createCounter(` opener
        // can be preceded by a newline + whitespace from `meter`.
        let src = "meter\n  .createCounter(\n  \"dt_client_ws\"\n)";
        let calls = find_meter_calls(src);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name.as_deref(), Some("dt_client_ws"));
    }

    #[test]
    fn negative_bad_name_multi_line_hard_fails() {
        let src = "meter.createCounter(\n  \"wrong_prefix_metric\",\n)";
        let calls = find_meter_calls(src);
        assert_eq!(calls.len(), 1);
        let name = calls[0].name.as_deref().unwrap();
        assert!(
            !R26_NAME_RE.is_match(name),
            "bad name should HARD FAIL R-26"
        );
    }

    #[test]
    fn negative_interpolated_template_literal_hard_fails() {
        // `` `dt_client_${foo}` `` — interpolated name violates R-26 by
        // construction (not a static prefix-matchable string). Walker
        // returns `name: None`; caller HARD-FAILs.
        let src = r#"meter.createCounter(`dt_client_${foo}`)"#;
        let calls = find_meter_calls(src);
        assert_eq!(calls.len(), 1);
        assert!(
            calls[0].name.is_none(),
            "interpolated name should yield None"
        );
    }

    #[test]
    fn negative_string_concatenation_unsupported_documented_limitation() {
        // `meter.createCounter("dt_client_" + "a".repeat(60))` — two literals
        // concatenated. The walker extracts the FIRST literal only
        // (`dt_client_`), which itself fails R-26 (no body). HARD-FAILs as
        // expected, but the diagnostic shows just the prefix. Bash today's
        // same limitation; documented as residual behavior.
        let src = r#"meter.createCounter("dt_client_" + "a".repeat(60))"#;
        let calls = find_meter_calls(src);
        assert_eq!(calls.len(), 1);
        // First literal is the static prefix; lacks body → fails R-26.
        assert_eq!(calls[0].name.as_deref(), Some("dt_client_"));
        assert!(!R26_NAME_RE.is_match(calls[0].name.as_deref().unwrap()));
    }

    #[test]
    fn start_line_tracks_opener_not_literal() {
        // Per docstring: `start_line` is the source line of the OPENER, not
        // the literal. Verifies the line-no calculation walks newlines BEFORE
        // the opener's start position.
        let src =
            "// padding line 1\n// padding line 2\nmeter.createCounter(\n  \"dt_client_line3\"\n)";
        let calls = find_meter_calls(src);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].start_line, 3);
    }

    #[test]
    fn multiple_calls_in_same_source() {
        let src = "\
meter.createCounter(\"dt_client_a\");
meter.createHistogram(
  \"dt_client_b\"
);
meter.createGauge(\"dt_client_c\");";
        let calls = find_meter_calls(src);
        assert_eq!(calls.len(), 3);
        assert_eq!(calls[0].name.as_deref(), Some("dt_client_a"));
        assert_eq!(calls[1].name.as_deref(), Some("dt_client_b"));
        assert_eq!(calls[2].name.as_deref(), Some("dt_client_c"));
    }

    #[test]
    fn scope_predicate_admits_sdk_core_and_web_app() {
        assert!(is_in_scope(Path::new("packages/sdk-core/src/index.ts")));
        assert!(is_in_scope(Path::new("packages/web-app/src/page.svelte")));
        assert!(!is_in_scope(Path::new("packages/test-utils/src/x.ts")));
        assert!(!is_in_scope(Path::new("packages/sdk-svelte/src/x.ts")));
    }

    #[test]
    fn scope_predicate_drops_in_scope_test_files() {
        assert!(!is_in_scope(Path::new("packages/sdk-core/src/foo.test.ts")));
        assert!(!is_in_scope(Path::new(
            "packages/sdk-core/src/__tests__/foo.ts"
        )));
    }

    // --- `// dt-metric-name-dynamic:` escape hatch (task #12, R-24) ---
    //
    // The decision is exercised through the pure `call_violation` so these are
    // filesystem-free. `marker_lines` is computed from the same source the
    // calls come from, matching `scan_file`.

    const VALID_REASON: &str =
        "// dt-metric-name-dynamic: runtime-validated by assertClientMetricName on the line above";

    fn violations(src: &str) -> Vec<String> {
        let marker_lines = load_dynamic_name_marker_lines(src);
        find_meter_calls(src)
            .iter()
            .filter_map(|c| call_violation(c, &marker_lines))
            .collect()
    }

    #[test]
    fn dynamic_name_with_valid_marker_is_exempt() {
        // (a) dynamic name + valid annotation on the call's opener line → clean.
        let src = format!("const c = meter.createCounter(name); {VALID_REASON}");
        assert!(violations(&src).is_empty());
    }

    #[test]
    fn dynamic_name_with_marker_on_line_above_is_exempt() {
        // "on or above" convention: marker on the preceding line also exempts.
        let src = format!("{VALID_REASON}\nconst c = meter.createCounter(name);");
        assert!(violations(&src).is_empty());
    }

    #[test]
    fn dynamic_name_without_marker_still_hard_fails() {
        // (b) dynamic name + NO annotation → still a violation.
        let src = "const c = meter.createCounter(name);";
        let v = violations(src);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0], "<non-literal or interpolated metric name>");
    }

    #[test]
    fn bad_literal_with_marker_present_still_hard_fails() {
        // (c) THE load-bearing invariant: a marker NEVER rescues a bad LITERAL.
        // The exemption is dynamic-name-only.
        let src = format!(r#"const c = meter.createCounter("mc_join_total"); {VALID_REASON}"#);
        let v = violations(&src);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0], "mc_join_total");
    }

    #[test]
    fn interpolated_literal_with_marker_present_still_hard_fails() {
        // Interpolated template literals are non-static names. They DO get the
        // dynamic exemption when marked (same `None`-name class as a variable).
        // Without a marker they hard-fail; this asserts the marker path for the
        // interpolated shape mirrors the variable shape.
        let unmarked = r#"const c = meter.createCounter(`dt_client_${suffix}`);"#;
        assert_eq!(violations(unmarked).len(), 1);
        let marked =
            format!(r#"const c = meter.createCounter(`dt_client_${{suffix}}`); {VALID_REASON}"#);
        assert!(violations(&marked).is_empty());
    }

    #[test]
    fn dynamic_name_with_lazy_reason_marker_still_fails() {
        // (d) a lazy/short reason does NOT grant the exemption (reuses the
        // shared `is_lazy_reason` bar — ≥10 chars, not test/tmp/todo/fixme/wip).
        let src = "const c = meter.createCounter(name); // dt-metric-name-dynamic: wip";
        let v = violations(src);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0], "<non-literal or interpolated metric name>");
    }

    #[test]
    fn marker_does_not_leak_to_an_unrelated_distant_call() {
        // A marked dynamic call, then a blank line, then an UNMARKED dynamic
        // call: the blank line means the second call's "on or above" window
        // (its own line + the line directly above = blank) does NOT see the
        // marker, so only the first is exempt. (The "on or above" window is
        // deliberately one line, matching the `pii-safe` convention — a marker
        // does not reach a call two lines down.)
        let src = format!(
            "const a = meter.createCounter(name); {VALID_REASON}\n\
             \n\
             const b = meter.createHistogram(other);"
        );
        let v = violations(&src);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0], "<non-literal or interpolated metric name>");
    }

    // -- Kind-label literal rule (2026-10-26) ---------------------------------

    fn kind_lines(src: &str) -> Vec<(usize, &'static str)> {
        kind_literal_hits(src, &KIND_LABEL_KEYS)
            .into_iter()
            .map(|h| (h.line, h.key))
            .collect()
    }

    /// Each red form with the exact (line, key) it must produce.
    const KIND_RED: &[(&str, usize, &str)] = &[
        ("const q = `x{media_kind='audio'}`;\n", 1, "media_kind"),
        (
            "const q = `x{content_kind=\\`main\\`}`;\n",
            1,
            "content_kind",
        ),
        (
            "if (labels['media_kind'] === 'video') {}\n",
            1,
            "media_kind",
        ),
        (
            "const a = 1;\nconst l = { media_kind: 'audio' };\n",
            2,
            "media_kind",
        ),
        (
            "const l = {\n  'content_kind': \"main\",\n};\n",
            2,
            "content_kind",
        ),
        ("const l = { media_kind: `video` };\n", 1, "media_kind"),
        ("if (x.content_kind === 'main') {}\n", 1, "content_kind"),
        (
            "const q = `sum(x{media_kind=\"audio\"})`;\n",
            1,
            "media_kind",
        ),
        (
            "const q = 'x{content_kind=~\"main|screen\"}';\n",
            1,
            "content_kind",
        ),
    ];

    #[test]
    fn kind_literal_hits_every_red_form_at_its_line() {
        for (src, line, key) in KIND_RED {
            assert_eq!(kind_lines(src), vec![(*line, *key)], "{src}");
        }
    }

    #[test]
    fn projection_reads_interpolation_comments_and_longer_names_stay_green() {
        for src in [
            "const l = { media_kind: p.media_kind, content_kind: p.content_kind };\n",
            "const q = `x{media_kind=\"${MEDIA_KIND_AUDIO}\"}`;\n",
            "// { media_kind: 'audio' }\n/* content_kind: 'main' */\n",
            "const o = { media_kind_unsupported: 'x', outcome: 'media_kind' };\n",
            "const s = 'by (media_kind, content_kind)';\n",
        ] {
            assert_eq!(kind_lines(src), vec![], "{src}");
        }
    }

    /// The neutered-matcher proof: an empty key set turns every red green.
    #[test]
    fn red_kind_fixtures_go_green_with_the_key_set_neutered() {
        for (src, _, _) in KIND_RED {
            assert!(kind_literal_hits(src, &[]).is_empty(), "{src}");
        }
    }

    /// The e2e widening is the kind rule's own scope: an e2e helper is in it,
    /// and NOT in the R-26 name rule's scope.
    #[test]
    fn e2e_helpers_are_in_the_kind_scope_only() {
        let e2e = Path::new("packages/web-app/e2e/mcMetrics.ts");
        assert!(is_in_kind_scope(e2e));
        assert!(!is_in_scope(e2e));
        assert!(!is_in_kind_scope(Path::new(
            "packages/web-app/e2e/multi-party-hear.spec.ts"
        )));
        let src = "const q = `x{media_kind=\"audio\"}`;\nmeter.createCounter('not_r26');\n";
        assert_eq!(kind_lines(src), vec![(1, "media_kind")]);
    }

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    /// Positive control on the REAL tree: no literal outside the home, the
    /// home's six values (3 entries x 2 keys) SEEN, and real files scanned.
    #[test]
    fn the_real_tree_is_kind_clean_and_the_home_is_seen() {
        let (findings, home_seen) = kind_findings(&repo_root(), &KIND_LABEL_KEYS).unwrap();
        assert!(findings.is_empty(), "{findings:?}");
        assert_eq!(home_seen, 6, "the projection record's 3 x 2 value literals");
        let files = kind_scope_files(&repo_root()).unwrap();
        assert!(
            files.iter().any(|f| f.starts_with("packages/web-app/e2e")),
            "e2e scanned"
        );
        assert!(
            files.iter().any(|f| f.starts_with("packages/sdk-core/src")),
            "sdk scanned"
        );
    }

    /// A literal planted into a copy of the REAL e2e helper must hit.
    #[test]
    fn a_literal_planted_in_a_real_e2e_helper_hits() {
        let real =
            std::fs::read_to_string(repo_root().join("packages/web-app/e2e/mcMetrics.ts")).unwrap();
        let planted = format!("{real}\nconst PLANTED = `x{{media_kind=\"video\"}}`;\n");
        assert_eq!(kind_lines(&real), vec![]);
        assert_eq!(kind_lines(&planted).len(), 1);
    }

    #[test]
    fn a_missing_or_valueless_home_is_vacuous() {
        let dir = tempfile::tempdir().unwrap();
        let (f, seen) = kind_findings(dir.path(), &KIND_LABEL_KEYS).unwrap();
        assert_eq!(seen, 0);
        assert_eq!(
            f.iter().map(|x| (x.0, x.1.as_str())).collect::<Vec<_>>(),
            vec![(KIND_LABEL_VACUOUS_RULE_ID, TS_PROJECTION_HOME)]
        );
        // Present but holding no kind value literals.
        let home = dir.path().join(TS_PROJECTION_HOME);
        std::fs::create_dir_all(home.parent().unwrap()).unwrap();
        std::fs::write(&home, "export const x = { media_kind: kind };\n").unwrap();
        let (f, seen) = kind_findings(dir.path(), &KIND_LABEL_KEYS).unwrap();
        assert_eq!(seen, 0);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].0, KIND_LABEL_VACUOUS_RULE_ID);
    }

    /// A wrong root (no target package at all) is NOT ok: the kind rule runs
    /// before the scaffold exit and reds on the missing home.
    #[test]
    fn a_run_over_an_empty_root_is_not_ok() {
        let dir = tempfile::tempdir().unwrap();
        let err = run(dir.path(), false).unwrap_err().to_string();
        assert_eq!(err, "ts-kind-label-violation-1");
    }

    /// Neither rule masks the other: the verdict names both counts.
    #[test]
    fn the_verdict_names_both_rules_counts() {
        assert!(verdict(0, 0).is_ok());
        assert_eq!(
            verdict(2, 0).unwrap_err().to_string(),
            "ts-metric-name-violation-2"
        );
        assert_eq!(
            verdict(0, 3).unwrap_err().to_string(),
            "ts-kind-label-violation-3"
        );
        assert_eq!(
            verdict(2, 3).unwrap_err().to_string(),
            "ts-metric-name-violation-2-and-kind-label-violation-3"
        );
    }
}
