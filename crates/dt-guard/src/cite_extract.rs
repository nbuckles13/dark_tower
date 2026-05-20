//! Doc-citation extraction + resolution.
//!
//! Port of `scripts/guards/lib/doc_cite_extract.py` per ADR-0034 §4-5.
//! Two guard subcommands (cite-no-line-numbers / cite-symbol-resolves)
//! consume `extract_cites`; symbol-resolves additionally consumes the 6
//! per-language resolvers below.
//!
//! ## Position convention
//!
//! Per @observability commitment #11 (corrected 2026-05-19): column convention
//! is **1-based row, 1-based col**. Matches rustc/cargo user-facing diagnostics
//! (`error[E0308]: foo.rs:7:3` — col 3 is the 3rd character, 1-indexed), the
//! existing `VIOLATION:` line convention, and `grep -n`/editor jump-to-location
//! semantics. Rust regex API returns 0-based byte offsets (`Match::start()`),
//! so we add 1 at the EXPLAIN-emit boundary.
//!
//! ## `full_match` parity gotcha
//!
//! Per @security commitment #15: Python's negative lookbehind does NOT
//! consume the boundary char; Rust's positive boundary class DOES. To
//! preserve byte-identical `full_match` parity:
//!
//! ```text
//! let path_match = caps.get(1).unwrap();
//! let full_match = &line[path_match.start() .. caps.get(0).unwrap().end()];
//! ```
//!
//! BOL branch degenerates correctly (`overall.start() == path_match.start()`);
//! boundary-char branch strips the 1-char prefix.

use crate::common::path_safety::resolve_cited_path;
use crate::common::status::emit_ok;
use crate::ignore::{is_lazy_reason, IGNORE_MARKER_HTML_RE};
use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

// -----------------------------------------------------------------------------
// Constants
// -----------------------------------------------------------------------------

/// Extensions that count as file-shaped path tokens. Excludes URL-domain
/// components (`.local`, `.com`, `.cluster`) so hostnames with ports
/// (e.g. `gc-service.dark-tower.svc.cluster.local:5432`) do not trip the
/// bare-line-cite detector. **Load-bearing security property** — see
/// parity-fixture case #6.
const EXTENSION_ALLOWLIST: &[&str] = &[
    "rs", "sh", "toml", "yaml", "yml", "md", "proto", "json", "ts", "tsx", "js",
];

/// Doc trees the new guards walk. Single source of truth — both guards
/// consume identical scope.
const IN_SCOPE_DIRS: &[&str] = &["docs/runbooks", ".claude/skills"];

/// Basename-fallback search roots. Runbook convention is to cite by
/// basename (e.g. `_common.sh::aggregate_worst_status`).
const BASENAME_SEARCH_ROOTS: &[&str] = &["scripts", "crates", "infra", "proto"];

// -----------------------------------------------------------------------------
// §4 lookbehind restructure — positive boundary class.
// -----------------------------------------------------------------------------

/// Positive boundary class: BOL or one of the 5+R3-audit chars. Python's
/// `(?<![\w./-])` accepts any preceding char NOT in `[A-Za-z0-9_./-]`; the
/// positive class is a strict subset (every char listed is non-word/dot/
/// slash/hyphen). Direction of divergence is Rust false-NEGATIVE only
/// (fewer cites flagged) — never false-positive. False-negatives are a
/// coverage gap, not a containment hole (`resolve_cited_path` is the
/// independent gate). Per @security commitment #16.
const PATH_PREFIX: &str = r#"(?:^|[\s\(\[\{`'"<>,;=|])([A-Za-z_][\w./\-]*\.[a-z]{1,5})"#;

pub const BARE_LINE_CITE_RULE_ID: &str = "bare_line_cite";
pub const SYMBOL_CITE_RULE_ID: &str = "symbol_cite";
pub const LAZY_IGNORE_REASON_RULE_ID: &str = "lazy_ignore_reason";

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static BARE_LINE_CITE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(&format!(r"{PATH_PREFIX}:(\d+)(?:-(\d+))?\b")).expect("static pattern compiles")
});

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static SYMBOL_CITE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(&format!(r"{PATH_PREFIX}::([A-Za-z_]\w*)\b")).expect("static pattern compiles")
});

// -----------------------------------------------------------------------------
// Per-language symbol resolvers — static-template + equality check.
// -----------------------------------------------------------------------------
// Per @code-reviewer 2026-05-19 ruling: 6 static `Lazy<Regex>`, one per
// language. Capture group 1 = symbol name; `symbol_resolves_in_file`
// iterates `captures_iter` and string-compares to the cited symbol. Md is
// the one exception — heading text matched word-boundary-aware against the
// cited symbol because the Python kernel uses `\b<sym>\b` against heading
// content (substring-permissive at heading start).

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static RS_RESOLVER: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\b(?:fn|struct|enum|trait|impl|const|static|type)\s+([A-Za-z_]\w*)\b")
        .expect("static pattern compiles")
});

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static SH_FN_PAREN_RESOLVER: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?m)^([A-Za-z_]\w*)\s*\(\s*\)").expect("static pattern compiles"));

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static SH_FN_KEYWORD_RESOLVER: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?m)^function\s+([A-Za-z_]\w*)\b").expect("static pattern compiles"));

// TOML resolution: split into two statics per @code-reviewer 2026-05-19
// vote (i) — mirrors the sh paren/keyword pair shape and lets the consumer
// loop walk one capture per Lazy (no two-group bookkeeping). Per-helper
// grep target: `grep -E 'static [A-Z_]+_RESOLVER' src/cite_extract.rs`
// lists every resolver at a glance.
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static TOML_SECTION_RESOLVER: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?m)^\[([A-Za-z_][\w.\-]*)\]").expect("static pattern compiles"));

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static TOML_KEY_RESOLVER: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?m)^([A-Za-z_][\w.\-]*)\s*=").expect("static pattern compiles"));

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static YAML_RESOLVER: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?m)^([A-Za-z_][\w\-]*)\s*:").expect("static pattern compiles"));

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static MD_HEADING_RESOLVER: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?m)^#+\s+(.*)$").expect("static pattern compiles"));

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static MD_WORD_RESOLVER: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"[A-Za-z_]\w*").expect("static pattern compiles"));

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static PROTO_RESOLVER: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\b(?:message|service|enum|rpc)\s+([A-Za-z_]\w*)\b")
        .expect("static pattern compiles")
});

/// Extensions this module can resolve. Others skip silently per Guard C
/// design (`_PATTERN_BUILDERS` table in Python kernel).
const SUPPORTED_RESOLUTION_EXTENSIONS: &[&str] =
    &["rs", "sh", "toml", "yaml", "yml", "md", "proto"];

// -----------------------------------------------------------------------------
// Types
// -----------------------------------------------------------------------------

/// One extracted cite from a doc line.
///
/// `kind` is `"bare-line"` or `"symbol"`. `extra` is the line-range string
/// (e.g. `"36"` / `"120-126"`) for bare-line; the symbol name for symbol.
/// `full_match` is byte-identical to Python's `sm.group(0)` per the
/// reconstruction formula in this module's doc-comment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cite {
    pub doc_file: String,
    pub line_no: usize,
    pub kind: String,
    pub path: String,
    pub extra: String,
    pub full_match: String,
    pub is_ignored: bool,
}

/// One finding emitted by a cite-extract subcommand. Consumed by the
/// `--explain` formatter in [`common::explain::print_finding`] (Bundle 2.5).
#[derive(Debug, Clone)]
pub struct Finding {
    pub doc_file: String,
    pub row: usize, // 1-based
    pub col: usize, // 1-based (rustc/cargo diagnostic convention; add 1 to regex Match::start())
    pub rule_id: &'static str,
    pub matched: String,
    pub src_file: &'static str,
    pub src_line: u32,
    pub detail: String, // policy-specific suffix for non-EXPLAIN output
}

// -----------------------------------------------------------------------------
// Helpers
// -----------------------------------------------------------------------------

/// Return true if `path`'s extension (after the last `.`) is allowlisted.
fn has_recognized_extension(path: &str) -> bool {
    match path.rsplit_once('.') {
        Some((_, ext)) => EXTENSION_ALLOWLIST
            .iter()
            .any(|e| e.eq_ignore_ascii_case(ext)),
        None => false,
    }
}

/// Reconstruct `full_match` per the §4/§15 byte-equivalence rule.
fn reconstruct_full_match(line: &str, caps: &regex::Captures<'_>) -> String {
    let path_match = match caps.get(1) {
        Some(m) => m,
        None => return String::new(),
    };
    let overall_end = match caps.get(0) {
        Some(m) => m.end(),
        None => return String::new(),
    };
    line[path_match.start()..overall_end].to_string()
}

/// Bounded ±N-char excerpt around a span (per semantic-guard watch-point #4).
/// Used by `--explain` to avoid echoing full file contents.
pub fn span_excerpt(text: &str, start: usize, end: usize, bound: usize) -> String {
    let lo = start.saturating_sub(bound);
    let hi = (end + bound).min(text.len());
    let mut out = String::new();
    if lo > 0 {
        out.push('…');
    }
    // Be byte-safe: clamp to char boundaries.
    let safe_lo = clamp_char_boundary(text, lo);
    let safe_hi = clamp_char_boundary(text, hi);
    out.push_str(&text[safe_lo..safe_hi]);
    if hi < text.len() {
        out.push('…');
    }
    out
}

fn clamp_char_boundary(s: &str, mut idx: usize) -> usize {
    while idx < s.len() && !s.is_char_boundary(idx) {
        idx += 1;
    }
    idx
}

// -----------------------------------------------------------------------------
// Public extraction surface (consumed by parity test + e2e harness).
// -----------------------------------------------------------------------------

/// Extract bare-line and symbol cites from a doc's text.
///
/// Returns `Cite` records with `is_ignored` set per the line's
/// `<!-- guard:ignore(<reason>) -->` annotation, with lazy reasons rejected.
/// Ordering invariant: symbol cites emitted before bare-line cites for each
/// line, matching the Python source loop ordering (per @test commitment).
pub fn extract_cites(doc_file: &str, doc_text: &str) -> Vec<Cite> {
    let mut out = Vec::new();
    for (idx, line) in doc_text.lines().enumerate() {
        let line_no = idx + 1;

        // Ignore status once per line.
        let mut is_ignored = false;
        if let Some(marker_caps) = IGNORE_MARKER_HTML_RE.captures(line) {
            if let Some(reason) = marker_caps.get(1) {
                if !is_lazy_reason(reason.as_str()) {
                    is_ignored = true;
                }
            }
        }

        // Symbol cites first — Python loop ordering. The `::` precludes a
        // bare-line follow-on but the ordering matters for deterministic tests.
        for sm in SYMBOL_CITE_RE.captures_iter(line) {
            let path = match sm.get(1) {
                Some(m) => m.as_str(),
                None => continue,
            };
            let sym = match sm.get(2) {
                Some(m) => m.as_str(),
                None => continue,
            };
            if !has_recognized_extension(path) {
                continue;
            }
            out.push(Cite {
                doc_file: doc_file.to_string(),
                line_no,
                kind: "symbol".to_string(),
                path: path.to_string(),
                extra: sym.to_string(),
                full_match: reconstruct_full_match(line, &sm),
                is_ignored,
            });
        }

        for bm in BARE_LINE_CITE_RE.captures_iter(line) {
            let path = match bm.get(1) {
                Some(m) => m.as_str(),
                None => continue,
            };
            let start = match bm.get(2) {
                Some(m) => m.as_str(),
                None => continue,
            };
            let end = bm.get(3).map(|m| m.as_str());
            if !has_recognized_extension(path) {
                continue;
            }
            let extra = match end {
                Some(e) => format!("{start}-{e}"),
                None => start.to_string(),
            };
            out.push(Cite {
                doc_file: doc_file.to_string(),
                line_no,
                kind: "bare-line".to_string(),
                path: path.to_string(),
                extra,
                full_match: reconstruct_full_match(line, &bm),
                is_ignored,
            });
        }
    }
    out
}

/// Walk `IN_SCOPE_DIRS` under `repo_root`; yield `(rel_path, abs_path)` for
/// each in-scope markdown doc. Sorted for deterministic test output.
pub fn walk_in_scope_docs(repo_root: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    for sub in IN_SCOPE_DIRS {
        let abs_sub = repo_root.join(sub);
        if !abs_sub.is_dir() {
            continue;
        }
        for entry in WalkDir::new(&abs_sub).into_iter().filter_map(Result::ok) {
            if !entry.file_type().is_file() {
                continue;
            }
            let abs_path = entry.into_path();
            let Some(name) = abs_path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !name.ends_with(".md") {
                continue;
            }
            let Ok(rel) = abs_path.strip_prefix(repo_root) else {
                continue;
            };
            out.push((rel.to_string_lossy().into_owned(), abs_path));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

// -----------------------------------------------------------------------------
// Symbol resolution.
// -----------------------------------------------------------------------------

/// Return true if `sym` appears as a definable construct in `file_text`,
/// per the resolver table for `ext`. Extensions outside the supported set
/// return `true` (caller's responsibility to gate by extension).
pub fn symbol_resolves_in_file(file_text: &str, sym: &str, ext: &str) -> bool {
    let ext_lower = ext.to_ascii_lowercase();
    if !SUPPORTED_RESOLUTION_EXTENSIONS
        .iter()
        .any(|e| *e == ext_lower)
    {
        return true;
    }
    let resolvers: &[&Lazy<Regex>] = match ext_lower.as_str() {
        "rs" => &[&RS_RESOLVER],
        "sh" => &[&SH_FN_PAREN_RESOLVER, &SH_FN_KEYWORD_RESOLVER],
        "toml" => &[&TOML_SECTION_RESOLVER, &TOML_KEY_RESOLVER],
        "yaml" | "yml" => &[&YAML_RESOLVER],
        "proto" => &[&PROTO_RESOLVER],
        "md" => return md_symbol_resolves(file_text, sym),
        _ => return true,
    };
    // Every resolver in the table emits exactly one capture group (group 1
    // = symbol). Per @code-reviewer 2026-05-19 vote (i) on the toml branch:
    // splitting the toml alternation into TOML_SECTION_RESOLVER +
    // TOML_KEY_RESOLVER eliminates the prior two-capture-group bookkeeping
    // that was a footgun for future resolver additions.
    for re in resolvers {
        for caps in re.captures_iter(file_text) {
            if let Some(m) = caps.get(1) {
                if m.as_str() == sym {
                    return true;
                }
            }
        }
    }
    false
}

/// Markdown branch: heading text checked for word-boundary substring match
/// against `sym` (case-insensitive). Matches Python `_build_md_pattern`'s
/// `re.MULTILINE | re.IGNORECASE` with `\b<sym>\b`.
fn md_symbol_resolves(file_text: &str, sym: &str) -> bool {
    let sym_lower = sym.to_ascii_lowercase();
    for caps in MD_HEADING_RESOLVER.captures_iter(file_text) {
        let Some(heading) = caps.get(1) else {
            continue;
        };
        for word in MD_WORD_RESOLVER.find_iter(heading.as_str()) {
            if word.as_str().eq_ignore_ascii_case(&sym_lower) {
                return true;
            }
        }
    }
    false
}

// -----------------------------------------------------------------------------
// Basename fallback.
// -----------------------------------------------------------------------------

/// Build a `{basename → [abs_paths…]}` index walking `BASENAME_SEARCH_ROOTS`.
/// Cheap on the production repo (~40ms one-shot in Python; comparable in Rust).
fn build_basename_index(repo_root: &Path) -> HashMap<String, Vec<PathBuf>> {
    let mut index: HashMap<String, Vec<PathBuf>> = HashMap::new();
    for root in BASENAME_SEARCH_ROOTS {
        let abs_root = repo_root.join(root);
        if !abs_root.is_dir() {
            continue;
        }
        for entry in WalkDir::new(&abs_root).into_iter().filter_map(Result::ok) {
            if !entry.file_type().is_file() {
                continue;
            }
            let Some(name) = entry.path().file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            index
                .entry(name.to_string())
                .or_default()
                .push(entry.into_path());
        }
    }
    index
}

/// Resolve basename-only cite. Returns `Some(abs_path)` only on
/// unambiguous single-match; multi-match returns `None` so doc authors must
/// disambiguate via a full repo-relative path.
fn resolve_basename_match(
    cited_path: &str,
    index: &HashMap<String, Vec<PathBuf>>,
) -> Option<PathBuf> {
    if cited_path.contains('/') {
        return None;
    }
    let matches = index.get(cited_path)?;
    if matches.len() == 1 {
        matches.first().cloned()
    } else {
        None
    }
}

// -----------------------------------------------------------------------------
// Subcommand entry points.
// -----------------------------------------------------------------------------

/// Guard A — fail on bare-line cites in long-lived doc trees.
pub fn run_no_line_numbers(repo_root: &Path, explain: bool) -> Result<()> {
    let docs = walk_in_scope_docs(repo_root);
    let mut violations = 0;
    for (rel, abs_path) in &docs {
        let text =
            std::fs::read_to_string(abs_path).with_context(|| format!("read doc {}", rel))?;
        for cite in extract_cites(rel, &text) {
            if cite.kind != "bare-line" || cite.is_ignored {
                continue;
            }
            violations += 1;
            print_violation(&cite, BARE_LINE_CITE_RULE_ID, explain);
        }
    }
    if violations > 0 {
        // Per ADR §6 wrapper contract — STATUS=FAIL goes to stdout from main.rs
        // on a non-zero exit. Returning an error here triggers that path.
        anyhow::bail!(
            "bare-line cites: {violations} violation(s) across {} doc(s)",
            docs.len()
        );
    }
    emit_ok(format!("cite-no-line-numbers-clean-{}-docs", docs.len()));
    Ok(())
}

/// Guard C — fail when a `<path>::<symbol>` cite does not resolve.
pub fn run_symbol_resolves(repo_root: &Path, explain: bool) -> Result<()> {
    let docs = walk_in_scope_docs(repo_root);
    let basename_index = build_basename_index(repo_root);

    let mut violations = 0;
    let mut cites_seen = 0;
    for (rel, abs_path) in &docs {
        let text =
            std::fs::read_to_string(abs_path).with_context(|| format!("read doc {}", rel))?;
        for cite in extract_cites(rel, &text) {
            if cite.kind != "symbol" || cite.is_ignored {
                continue;
            }
            cites_seen += 1;

            let ext = cite
                .path
                .rsplit_once('.')
                .map(|(_, e)| e.to_ascii_lowercase())
                .unwrap_or_default();
            if !SUPPORTED_RESOLUTION_EXTENSIONS.iter().any(|e| *e == ext) {
                continue; // silently allow unsupported extensions
            }

            let resolved = resolve_cited_path(repo_root, &cite.path).or_else(|| {
                resolve_basename_match(&cite.path, &basename_index)
                    .and_then(|abs| std::fs::canonicalize(abs).ok())
            });

            let Some(target_path) = resolved else {
                violations += 1;
                print_symbol_violation(&cite, "path-escape-or-missing", explain);
                continue;
            };
            if !target_path.is_file() {
                violations += 1;
                print_symbol_violation(&cite, "file-missing", explain);
                continue;
            }
            let target_text = std::fs::read_to_string(&target_path)
                .with_context(|| format!("read cited file {}", cite.path))?;
            if !symbol_resolves_in_file(&target_text, &cite.extra, &ext) {
                violations += 1;
                print_symbol_violation(&cite, "symbol-not-found", explain);
            }
        }
    }
    if violations > 0 {
        anyhow::bail!(
            "symbol cites unresolved: {violations} violation(s) across {cites_seen} cite(s) in {} doc(s)",
            docs.len()
        );
    }
    emit_ok(format!(
        "cite-symbol-resolves-clean-{cites_seen}-cites-{}-docs",
        docs.len()
    ));
    Ok(())
}

fn print_violation(cite: &Cite, rule_id: &str, explain: bool) {
    if explain {
        let policy = format!("cite-extract::{rule_id}");
        crate::common::explain::print_finding(&crate::common::explain::Finding {
            file: &cite.doc_file,
            row: cite.line_no,
            col: 0, // Cite doesn't yet carry per-match col; helper bumps 0 → 1.
            policy: &policy,
            matched: &cite.full_match,
            extras: &[],
            src_file: file!(),
            src_line: line!(),
        });
    } else {
        println!(
            "VIOLATION: {} — doc-citations-line-numbers-found — {}:{}: {}",
            cite.doc_file, cite.doc_file, cite.line_no, cite.full_match
        );
    }
}

fn print_symbol_violation(cite: &Cite, reason: &str, explain: bool) {
    if explain {
        let policy = format!("cite-extract::{SYMBOL_CITE_RULE_ID}");
        crate::common::explain::print_finding(&crate::common::explain::Finding {
            file: &cite.doc_file,
            row: cite.line_no,
            col: 0,
            policy: &policy,
            matched: &cite.full_match,
            extras: &[("reason", reason)],
            src_file: file!(),
            src_line: line!(),
        });
    } else {
        println!(
            "VIOLATION: {} — doc-citation-symbol-unresolved — {} — {}",
            cite.doc_file, cite.full_match, reason
        );
    }
}

// -----------------------------------------------------------------------------
// Inline column-offset reporting test (per @test 2026-05-19 nit).
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Pins `m.get(1).unwrap().start()` column-offset reporting. Parity
    /// fixture pins WHAT is extracted; this pins WHERE.
    ///
    /// `Match::start()` is 0-based (Rust regex API contract). The EXPLAIN
    /// wire format is 1-based (rustc/cargo + `VIOLATION:` line + `grep -n`
    /// convention), so the emit boundary adds 1. These tests pin the
    /// pre-conversion API value; downstream `EXPLAIN:` emission is `+1`.
    #[test]
    fn column_offset_line_start_match() {
        let line = "foo.rs:42";
        let caps = BARE_LINE_CITE_RE.captures(line).unwrap();
        let col = caps.get(1).unwrap().start();
        assert_eq!(col, 0, "BOL match: Match::start() = 0 (wire format = 1)");
    }

    #[test]
    fn column_offset_mid_line_after_backtick() {
        let line = "see `foo.rs:42`";
        let caps = BARE_LINE_CITE_RE.captures(line).unwrap();
        let col = caps.get(1).unwrap().start();
        assert_eq!(col, 5, "backtick at 4, path starts at 5 (wire format = 6)");
    }

    #[test]
    fn column_offset_mid_line_after_equals() {
        let line = "PATH=foo.rs:42";
        let caps = BARE_LINE_CITE_RE.captures(line).unwrap();
        let col = caps.get(1).unwrap().start();
        assert_eq!(col, 5, "equals at 4, path starts at 5 (wire format = 6)");
    }

    #[test]
    fn triple_colon_rejected_structurally() {
        // Per @security commitment: positive boundary class does not include
        // `:`, so `foo.rs:::baz` cannot match SYMBOL_CITE_RE — the third `:`
        // fails the left-boundary requirement on a second match attempt.
        let line = "see foo.rs:::baz";
        let cites = extract_cites("test.md", line);
        assert_eq!(cites, Vec::new(), "triple-colon must produce no cites");
    }

    // -------------------------------------------------------------------------
    // Resolver-side parity cases per @security final follow-up 2026-05-19.
    // These exercise `symbol_resolves_in_file` / `md_symbol_resolves` and
    // lock the static-template + `as_str() == sym` byte-equality semantics
    // against three Python `re.escape`-equivalent regression vectors. Live
    // INLINE here (not in `tests/cite_extract_parity.rs`) per ADR-0034 §2
    // — resolver-internal behavior belongs to the unit-test layer.
    // -------------------------------------------------------------------------

    /// `foo.bar` sym against rust file containing `fn foo.bar`: Python
    /// `re.escape("foo.bar")` produces `foo\.bar` literal, but `fn foo.bar`
    /// isn't valid Rust syntax and never appears in source — Python returns
    /// `false`. Rust port: RS_RESOLVER's capture group is `([A-Za-z_]\w*)`
    /// where `\w` excludes `.`, so the regex matches the `foo` token in
    /// `fn foo` and `as_str()` compares `"foo" == "foo.bar"` → false.
    /// **Key parity point**: both paths return false even on impossible
    /// inputs, locking the no-divergence property.
    #[test]
    fn resolver_rs_dot_in_symbol_returns_false_both_paths() {
        let src = "fn foo() {}\n";
        assert!(
            !symbol_resolves_in_file(src, "foo.bar", "rs"),
            "dot-in-symbol cannot match \\w+ capture; Python and Rust both return false"
        );
    }

    /// `_private` sym against rust file containing `fn _private`: leading
    /// underscore is in `[A-Za-z_]\w*` capture (and `re.escape("_private")`
    /// returns `_private` unchanged). Both paths return `true`. Positive
    /// test for the leading-underscore identifier shape.
    #[test]
    fn resolver_rs_leading_underscore_resolves() {
        let src = "fn _private() {}\n";
        assert!(
            symbol_resolves_in_file(src, "_private", "rs"),
            "leading-underscore identifier must resolve via byte-equality"
        );
    }

    /// Prefix-collision invariant: `foo` sym against file containing
    /// `fn foobar` must return `false`. The `\b` word boundary on the
    /// capture group (`([A-Za-z_]\w*)` reads to end-of-identifier) ensures
    /// the captured token is `"foobar"`, not the prefix `"foo"`.
    /// `as_str() == sym` then compares `"foobar" == "foo"` → false. Python
    /// equivalent: `re.escape("foo")` is `foo`, pattern is
    /// `\bfoo\b` which won't match `foobar`. Both paths → false.
    #[test]
    fn resolver_rs_prefix_collision_rejected() {
        let src = "fn foobar() {}\n";
        assert!(
            !symbol_resolves_in_file(src, "foo", "rs"),
            "prefix-not-equal-to-full-symbol must not resolve (locks `\\w*` capture-to-end semantics)"
        );
    }

    /// Md branch word-boundary discipline per Python comment F3(c)
    /// (`_build_md_pattern` line 187). Heading `## Testing Setup` must NOT
    /// resolve symbol `"Test"` — Python uses `\b<sym>\b` on heading text,
    /// so `Test` is rejected because `Testing` is a longer word containing
    /// it as a prefix. Rust port: `MD_HEADING_RESOLVER` captures heading
    /// content, then `MD_WORD_RESOLVER` (`[A-Za-z_]\w*`) tokenizes into
    /// word atoms `Testing` + `Setup`; `eq_ignore_ascii_case` compares
    /// `Testing == Test` → false (word atom, not substring).
    #[test]
    fn resolver_md_word_boundary_rejects_prefix_match() {
        let src = "## Testing Setup\n\nBody text mentions Test elsewhere.\n";
        assert!(
            !md_symbol_resolves(src, "Test"),
            "`Test` is a prefix of `Testing`; word-boundary discipline must reject prefix matches"
        );
    }

    /// Positive control for the md branch: `## Foo Bar` resolves `Foo` and
    /// `bar` (case-insensitive). Locks the `eq_ignore_ascii_case` path.
    #[test]
    fn resolver_md_heading_resolves_case_insensitive_word_atom() {
        let src = "## Foo Bar\n";
        assert!(md_symbol_resolves(src, "Foo"));
        assert!(
            md_symbol_resolves(src, "bar"),
            "case-insensitive on md branch"
        );
        assert!(
            md_symbol_resolves(src, "FOO"),
            "case-insensitive on md branch"
        );
    }
}
