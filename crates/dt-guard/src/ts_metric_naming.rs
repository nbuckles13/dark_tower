//! `ts-name-guard-dt-client` subcommand — port of
//! `scripts/guards/simple/ts/name-guard-dt-client.sh`.
//!
//! R-26 client metric-name convention (per @observability):
//! ```text
//! ^dt_client_[a-z][a-z0-9_]{0,53}$
//! ```
//! Literal prefix `dt_client_`; lowercase-letter start (no leading digit/
//! underscore); snake_case body; max total length 64 (Prometheus/OTel
//! default; per @observability S4 this is a HARD FAIL — the `{0,53}` regex
//! itself encodes the cap, matching `metric_labels.rs:MAX_LITERAL_VALUE_LENGTH`).
//!
//! Scope: **POSITIVE include-list** — `packages/(sdk-core|web-app)/src/**`.
//! Other packages are out-of-scope by definition (not by exemption). Per
//! @paired-client task-#37 ruling sdk-svelte stays deferred.

use crate::common::explain::{print_finding, Finding};
use crate::common::git_changes::get_all_changed_files;
use crate::common::status::emit_ok;
use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use std::path::{Path, PathBuf};

pub const METRIC_NAME_RULE_ID: &str = "metric_name";

const EXTENSIONS: &[&str] = &[".ts", ".tsx", ".svelte"];

const SCOPE_PREFIXES: &[&str] = &["packages/sdk-core/src/", "packages/web-app/src/"];

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

/// R-26 metric-name regex. Per @observability S4: `{0,53}` upper bound is a
/// HARD FAIL — encodes the 64-char cap (10 + 53 + 1 = 64).
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static R26_NAME_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^dt_client_[a-z]([a-z0-9_]{0,52}[a-z0-9])?$").expect("static pattern compiles")
});

fn is_in_scope(path: &Path) -> bool {
    let Some(s) = path.to_str() else {
        return false;
    };
    if EXCLUDED_PATH_PATTERNS.iter().any(|pat| s.contains(pat)) {
        return false;
    }
    if EXCLUDED_FILE_SUFFIXES.iter().any(|suf| s.ends_with(suf)) {
        return false;
    }
    SCOPE_PREFIXES.iter().any(|prefix| s.starts_with(prefix))
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

fn scan_file(repo_root: &Path, path: &Path) -> Result<Vec<Hit>> {
    let abs = repo_root.join(path);
    let content =
        std::fs::read_to_string(&abs).with_context(|| format!("reading {}", abs.display()))?;
    let mut hits: Vec<Hit> = Vec::new();

    for call in find_meter_calls(&content) {
        let name_for_check = call.name.as_deref().unwrap_or("");
        if !R26_NAME_RE.is_match(name_for_check) {
            // For interpolated/non-static names, surface a marker token so the
            // operator sees WHY R-26 rejected the call (rather than a bare
            // empty-string violation).
            let surfaced_name = match &call.name {
                Some(n) => n.clone(),
                None => "<non-literal or interpolated metric name>".to_string(),
            };
            hits.push(Hit {
                file: path.to_path_buf(),
                line: call.start_line,
                name: surfaced_name,
            });
        }
    }

    Ok(hits)
}

pub fn run(repo_root: &Path, explain: bool) -> Result<()> {
    // Scaffold-now / fire-later: if neither target package exists yet, exit clean.
    let any_target_present = SCOPE_PREFIXES
        .iter()
        .any(|prefix| repo_root.join(prefix.trim_end_matches('/')).is_dir());
    if !any_target_present {
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
        emit_ok("ts-name-guard-dt-client-no-files");
        return Ok(());
    }

    let mut all_hits: Vec<Hit> = Vec::new();
    for path in &in_scope {
        let hits = scan_file(repo_root, path)?;
        all_hits.extend(hits);
    }

    if all_hits.is_empty() {
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
            println!(
                "VIOLATION: {}:{} metric name {:?} does not match ^dt_client_[a-z][a-z0-9_]{{0,53}}$",
                file_disp, hit.line, hit.name
            );
        }
    }

    anyhow::bail!("ts-metric-name-violation-{}", all_hits.len())
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
}
