//! `knowledge-index` subcommand — port of
//! `scripts/guards/simple/validate-knowledge-index.sh`.
//!
//! Walks every `docs/specialist-knowledge/*/INDEX.md` and enforces:
//! 1. File-path backtick pointers resolve on disk and aren't gitignored.
//! 2. `ADR-NNNN` references have a matching `docs/decisions/adr-NNNN-*.md`.
//! 3. Each INDEX.md is ≤75 lines.

use crate::common::explain::{print_finding, Finding};
use crate::common::git_changes::is_gitignored;
use crate::common::status::emit_ok;
use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use std::path::{Path, PathBuf};

pub const STALE_POINTER_RULE_ID: &str = "stale_pointer";
pub const GITIGNORED_POINTER_RULE_ID: &str = "gitignored_pointer";
pub const STALE_ADR_RULE_ID: &str = "stale_adr";
pub const SIZE_VIOLATION_RULE_ID: &str = "size_violation";

const MAX_LINES: usize = 75;
const INDEX_DIR: &str = "docs/specialist-knowledge";

const EXTENSIONS: &[&str] = &[
    "rs", "md", "proto", "toml", "sh", "sql", "yaml", "yml", "json",
];

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static BACKTICK_PATH_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"`([^`]+\.(?:rs|md|proto|toml|sh|sql|yaml|yml|json)[^`]*)`")
        .expect("static pattern compiles")
});

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static ADR_REF_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"ADR-(\d+)").expect("static pattern compiles"));

/// Trim function/type/line-number suffixes from a backticked path.
/// Examples:
///   `crates/foo/src/lib.rs:34-42` → `crates/foo/src/lib.rs`
///   `crates/foo/src/lib.rs:Type::method()` → `crates/foo/src/lib.rs`
///   `docs/X.md#anchor` → `docs/X.md`
fn strip_suffixes(path: &str) -> String {
    let mut s = path.to_string();
    // Strip anchor `#...`.
    if let Some(idx) = s.find('#') {
        s.truncate(idx);
    }
    // Strip `:` and everything after, IF the `:` is in the "annotation"
    // position — i.e. after a known file-extension chunk. We walk from the
    // FIRST `:` (not the last) so `Type::method` collapses cleanly.
    //
    // Heuristic: find the first `:` where the character immediately AFTER
    // is either a digit (line range) or an identifier start (symbol ref).
    let bytes = s.as_bytes();
    let mut cut_at: Option<usize> = None;
    for (i, &b) in bytes.iter().enumerate() {
        if b != b':' {
            continue;
        }
        let next = bytes.get(i + 1);
        match next {
            Some(c) if c.is_ascii_digit() => {
                cut_at = Some(i);
                break;
            }
            Some(c) if c.is_ascii_alphabetic() || *c == b'_' => {
                cut_at = Some(i);
                break;
            }
            _ => continue,
        }
    }
    if let Some(idx) = cut_at {
        s.truncate(idx);
    }
    s
}

fn is_in_scope_path(path: &str) -> bool {
    path.starts_with("docs/")
        || path.starts_with("crates/")
        || path.starts_with("proto/")
        || path.starts_with("scripts/")
}

fn is_glob_or_placeholder(path: &str) -> bool {
    path.contains('*') || path.contains("NNNN")
}

fn git_check_ignore(repo_root: &Path, path: &str) -> bool {
    // Per @team-lead 2026-05-21 SoT-only-git rule: route through
    // `common::git_changes::is_gitignored`. Falls back to `false` on
    // git error (same as the prior inline implementation).
    is_gitignored(repo_root, path).unwrap_or(false)
}

fn find_adr_file(repo_root: &Path, adr_num: &str) -> Option<PathBuf> {
    let decisions = repo_root.join("docs/decisions");
    let entries = std::fs::read_dir(&decisions).ok()?;
    let prefix = format!("adr-{adr_num:0>4}-");
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_str()?;
        if name_str.starts_with(&prefix) && name_str.ends_with(".md") {
            return Some(entry.path());
        }
    }
    None
}

#[derive(Debug)]
struct Hit {
    rule_id: &'static str,
    detail: String,
    file: PathBuf,
}

fn check_index(repo_root: &Path, index_path: &Path) -> Result<Vec<Hit>> {
    let content = std::fs::read_to_string(index_path)
        .with_context(|| format!("reading {}", index_path.display()))?;
    let rel = index_path
        .strip_prefix(repo_root)
        .unwrap_or(index_path)
        .to_path_buf();
    let mut hits: Vec<Hit> = Vec::new();

    // Size cap.
    let line_count = content.lines().count();
    if line_count > MAX_LINES {
        hits.push(Hit {
            rule_id: SIZE_VIOLATION_RULE_ID,
            detail: format!("{line_count} lines (max {MAX_LINES})"),
            file: rel.clone(),
        });
    }

    // Stale-pointer check.
    let mut seen_paths: Vec<String> = Vec::new();
    for caps in BACKTICK_PATH_RE.captures_iter(&content) {
        let Some(m) = caps.get(1) else { continue };
        let raw = m.as_str();
        if is_glob_or_placeholder(raw) {
            continue;
        }
        let path = strip_suffixes(raw);
        if !is_in_scope_path(&path) {
            continue;
        }
        if seen_paths.contains(&path) {
            continue;
        }
        seen_paths.push(path.clone());

        let abs = repo_root.join(&path);
        if !abs.exists() {
            hits.push(Hit {
                rule_id: STALE_POINTER_RULE_ID,
                detail: path.clone(),
                file: rel.clone(),
            });
        } else if git_check_ignore(repo_root, &path) {
            hits.push(Hit {
                rule_id: GITIGNORED_POINTER_RULE_ID,
                detail: path.clone(),
                file: rel.clone(),
            });
        }
    }

    // ADR-reference check.
    let mut seen_adrs: Vec<String> = Vec::new();
    for caps in ADR_REF_RE.captures_iter(&content) {
        let Some(m) = caps.get(1) else { continue };
        let num = m.as_str().to_string();
        if seen_adrs.contains(&num) {
            continue;
        }
        seen_adrs.push(num.clone());
        if find_adr_file(repo_root, &num).is_none() {
            hits.push(Hit {
                rule_id: STALE_ADR_RULE_ID,
                detail: format!("ADR-{num}"),
                file: rel.clone(),
            });
        }
    }

    Ok(hits)
}

pub fn run(repo_root: &Path, explain: bool) -> Result<()> {
    let index_dir = repo_root.join(INDEX_DIR);
    if !index_dir.is_dir() {
        emit_ok("knowledge-index-no-dir");
        return Ok(());
    }

    let mut indexes: Vec<PathBuf> = Vec::new();
    for entry in std::fs::read_dir(&index_dir)
        .with_context(|| format!("reading {}", index_dir.display()))?
        .flatten()
    {
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let candidate = entry.path().join("INDEX.md");
        if candidate.is_file() {
            indexes.push(candidate);
        }
    }
    indexes.sort();

    if indexes.is_empty() {
        emit_ok("knowledge-index-no-files");
        return Ok(());
    }

    // EXTENSIONS const is documentation-only — the regex itself encodes the
    // matching set. Reference it to satisfy unused-const lints.
    let _ = EXTENSIONS;

    let mut all_hits: Vec<Hit> = Vec::new();
    for idx in &indexes {
        all_hits.extend(check_index(repo_root, idx)?);
    }

    if all_hits.is_empty() {
        emit_ok(format!("knowledge-index-clean-{}-files", indexes.len()));
        return Ok(());
    }

    for hit in &all_hits {
        let file_disp = hit.file.display().to_string();
        if explain {
            let policy = format!("knowledge-index::{}", hit.rule_id);
            print_finding(&Finding {
                file: &file_disp,
                row: 0,
                col: 0,
                policy: &policy,
                matched: &hit.detail,
                extras: &[],
                src_file: file!(),
                src_line: line!(),
            });
        } else {
            println!("VIOLATION: {} [{}] {}", file_disp, hit.rule_id, hit.detail);
        }
    }

    // Classify the wire token by dominant rule_id.
    let stale_ptr = all_hits
        .iter()
        .filter(|h| h.rule_id == STALE_POINTER_RULE_ID)
        .count();
    let stale_adr = all_hits
        .iter()
        .filter(|h| h.rule_id == STALE_ADR_RULE_ID)
        .count();
    let size_v = all_hits
        .iter()
        .filter(|h| h.rule_id == SIZE_VIOLATION_RULE_ID)
        .count();
    let kind = if size_v > 0 {
        "size-violation"
    } else if stale_adr > stale_ptr {
        "stale-adr"
    } else {
        "stale-pointer"
    };
    anyhow::bail!("knowledge-index-{kind}-{}", all_hits.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_suffixes_drops_line_range() {
        assert_eq!(
            strip_suffixes("crates/foo/lib.rs:34-42"),
            "crates/foo/lib.rs"
        );
        assert_eq!(strip_suffixes("crates/foo/lib.rs:1"), "crates/foo/lib.rs");
    }

    #[test]
    fn strip_suffixes_drops_symbol_ref() {
        assert_eq!(
            strip_suffixes("crates/foo/lib.rs:my_function"),
            "crates/foo/lib.rs"
        );
        assert_eq!(
            strip_suffixes("crates/foo/lib.rs:Type::method"),
            "crates/foo/lib.rs"
        );
    }

    #[test]
    fn strip_suffixes_drops_anchor() {
        assert_eq!(
            strip_suffixes("docs/runbooks/x.md#section"),
            "docs/runbooks/x.md"
        );
    }

    #[test]
    fn in_scope_recognizes_canonical_roots() {
        assert!(is_in_scope_path("docs/x.md"));
        assert!(is_in_scope_path("crates/foo/lib.rs"));
        assert!(is_in_scope_path("proto/x.proto"));
        assert!(is_in_scope_path("scripts/guards/x.sh"));
        assert!(!is_in_scope_path("infra/x.yaml"));
    }

    #[test]
    fn glob_and_placeholder_skipped() {
        assert!(is_glob_or_placeholder("crates/*/lib.rs"));
        assert!(is_glob_or_placeholder("adr-NNNN-foo.md"));
        assert!(!is_glob_or_placeholder("crates/foo/lib.rs"));
    }

    #[test]
    fn backtick_path_re_captures_path() {
        let line = "see `docs/runbooks/foo.md` for details";
        let caps = BACKTICK_PATH_RE.captures(line).unwrap();
        assert_eq!(caps.get(1).unwrap().as_str(), "docs/runbooks/foo.md");
    }

    #[test]
    fn adr_re_extracts_number() {
        let caps = ADR_REF_RE.captures("ADR-0024").unwrap();
        assert_eq!(caps.get(1).unwrap().as_str(), "0024");
    }
}
