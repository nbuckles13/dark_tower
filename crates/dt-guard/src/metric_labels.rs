//! `metric-labels` subcommand — port of validate-metric-labels.sh Python
//! kernel (ADR-0031 Prereq #3, ADR-0011 cardinality budget).
//!
//! Verbatim port per @observability commitment #13:
//! * `PII_TOKENS_CATEGORY_A` non-bypassable; bare `token` included per Lead
//!   ruling 2026-04-17.
//! * `CATEGORY_A_ALLOWLIST = {token_type}` (security co-owner sign-off required).
//! * `PII_TOKENS_CATEGORY_B` + `HASHED_SUFFIXES` exemption (Cat B only).
//! * `PII_PREFIX_DENYLIST` fires regardless of suffix, scanned before Cat A/B
//!   and before `is_hashed_label()`; membership and per-entry meaning live in
//!   `pii_vocabulary.rs` and `docs/observability/label-taxonomy.md`.
//! * `LABEL_ALLOWLIST` (Cat B substring false-positive suppression).
//! * `MAX_LITERAL_VALUE_LENGTH = 64`; `UNBOUNDED_VALUE_PATTERNS`.
//! * Match order: prefix → Cat A → Cat B (per Python L531-572).
//! * Escape-hatch `# pii-safe` or `// pii-safe`: ≥10 chars, not in `LAZY_REASON_RE`.
//!   Lazy reasons emit `lazy_pii_safe_reason` AND discard the marker.
//!   Suppresses Cat B only (NOT Cat A, NOT Rule 2 cardinality, NOT Rule 3 naming).

use crate::common::kind_labels::{
    is_projection_value, KindKeys, LABELS_RS, MH_CRATE_PREFIX, RUST_PROJECTION_HOME,
};
use crate::common::path_safety::to_repo_relative;
use crate::common::scan::warn_skip;
use crate::common::status::emit_ok;
use crate::common::test_code_filter::{
    compute_test_block_ranges, is_line_in_test_block, is_scan_exempt,
};
use crate::ignore::is_lazy_reason;
use crate::metric_macros::{MacroKind, MACRO_NAME_ALTERNATION};
use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

const CRATES_SUBDIR: &str = "crates";
const MAX_LITERAL_VALUE_LENGTH: usize = 64;

// PII / secret-identifier vocabularies are sourced from the canonical home at
// crate::common::pii_vocabulary (Wave-2 promotion per @security F1 +
// @observability Q1 + @code-reviewer item 2). Wave-2 widening adds 4 CATEGORY_A
// + 5 CATEGORY_B tokens; this module's behavior strictly broadens vs. Wave-1.
use crate::common::pii_vocabulary::{
    CATEGORY_A_ALLOWLIST, HASHED_SUFFIXES, LABEL_ALLOWLIST, PII_PREFIX_DENYLIST,
    PII_TOKENS_CATEGORY_A, PII_TOKENS_CATEGORY_B,
};

pub const LABEL_SECRET_RULE_ID: &str = "label_secret";
pub const LABEL_PII_RULE_ID: &str = "label_pii";
pub const LABEL_NAMING_RULE_ID: &str = "label_naming";
pub const LITERAL_VALUE_LENGTH_RULE_ID: &str = "literal_value_length";
pub const UNBOUNDED_VALUE_RULE_ID: &str = "unbounded_value";
pub const LAZY_PII_SAFE_REASON_RULE_ID: &str = "lazy_pii_safe_reason";
pub const METRIC_NAME_LENGTH_RULE_ID: &str = "metric_name_length";
pub const METRIC_NAME_NAMING_RULE_ID: &str = "metric_name_naming";
pub const PARSE_ERROR_RULE_ID: &str = "parse_error";
/// A `media_kind` / `content_kind` label whose value is not a projection
/// `.label()` call, outside the Rust projection home (label-taxonomy.md
/// §"Carried: `media_kind` and `content_kind`").
pub const KIND_LABEL_LITERAL_RULE_ID: &str = "kind_label_literal";
/// A kind label on an `mh-service` metric: MH is type-blind (ADR-0036 §7).
pub const KIND_LABEL_ON_MH_RULE_ID: &str = "kind_label_on_mh";
/// The kind-label rules could not evaluate their input (key constants
/// unresolved, projection home missing or holding no values, no kind-labelled
/// site seen). Never "fixed" by loosening a content rule.
pub const KIND_LABEL_VACUOUS_RULE_ID: &str = "kind_label_vacuous";

/// Opener for a `metrics`-crate macro invocation.
///
/// # The name list is DERIVED, not re-inlined (2026-09-08)
///
/// Until this date the six names were spelled out here as a literal — the
/// sole holdout among four consumers of `metric_macros`, in a file that
/// already imports [`MacroKind`], and while `metric_macros`' own module doc
/// listed `metric_labels` among the subcommands that consume the canonical
/// home "instead of each re-inlining its own `Lazy<Regex>`". **The doc said
/// it was converted and the code said it was not**, which is worse than an
/// honest duplicate: it was a false claim of SSoT coverage. A seventh
/// `MacroKind` variant widened every sibling and silently not this one.
///
/// The complementary guard against that regression returning is
/// [`tests::every_macro_kind_is_discovered_through_the_opener`]. The frozen
/// pin at the source of truth cannot catch a consumer un-deriving; only a
/// derivation test here can.
///
/// # Both anchor halves are closed (2026-09-08) — and the second half was
/// # deferred first, on a cost claim that did not survive being checked
///
/// The anchor here was `!\s*\(`, narrower than Rust's grammar in two
/// independent ways (see `telemetry_macros::LOG_MACRO_RE`):
///
/// * **Whitespace** (`!` → `\s*!`). `find_macro_invocations` locates the
///   delimiter as `whole.end() - 1`, which lands on it however much
///   whitespace precedes the `!`. Free.
/// * **Delimiter** (`\(` → `[\(\[\{]`). This one needed the body scanner to
///   learn the matching closer, and was **initially deferred as task-sized.
///   That was a misprice, corrected in review**: the scanner reads one opener
///   byte and derives its closer, and the depth loop compares against those
///   two variables instead of two literals. Six lines, in a file already open
///   in the same changeset.
///
/// The general seam is still real and still worth knowing — **line gates
/// versus body scanners**: an `is_match` gate widens for free, a scanner that
/// parses from the delimiter needs the closer. But "needs the closer" is not
/// the same as "is expensive", and the entry that priced it never opened the
/// loop to check. Note this static is used BOTH ways: as a body-scanner
/// opener at `find_macro_invocations`, and as a pure file-discovery gate in
/// `run()`.
///
/// # Why this matters beyond cataloguing
///
/// This opener is the discovery loop feeding the label PII checks, so an
/// invocation that evades it is **never label-checked at all** — and the
/// file-discovery use means a file whose metric macros are all brace-form
/// never entered the scan set. `counter!{"m", "email" => user_email}` would
/// ship an unbounded, PII-bearing label with nothing firing (ADR-0029). That
/// was a live leak path, not a tidy-up, which is why it was fixed here rather
/// than carried as debt.
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static MACRO_OPENER_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(&format!(
        r"(?:\bmetrics\s*::\s*)?\b({})\s*!\s*[\(\[\{{]",
        *MACRO_NAME_ALTERNATION
    ))
    .expect("static pattern compiles")
});

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static STRING_LITERAL_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"^"((?:[^"\\]|\\.)*)"$"#).expect("static pattern compiles"));

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static STRING_LITERAL_TO_STRING_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"^"((?:[^"\\]|\\.)*)"\s*\.\s*(?:to_string|to_owned|into)\s*\(\s*\)\s*$"#)
        .expect("static pattern compiles")
});

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static STRING_FROM_LITERAL_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"^String\s*::\s*from\s*\(\s*"((?:[^"\\]|\\.)*)"\s*\)\s*$"#)
        .expect("static pattern compiles")
});

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static SNAKE_CASE_IDENT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[a-z_][a-z0-9_]*$").expect("static pattern compiles"));

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static PII_SAFE_MARKER_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?:#|//)\s*pii-safe\s*:\s*(.+?)\s*$").expect("static pattern compiles")
});

// Unbounded-value heuristics — Python L204-210.
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static UNBOUNDED_VALUE_PATTERNS: Lazy<Vec<(&'static str, Regex)>> = Lazy::new(|| {
    vec![
        (
            "Uuid::* — per-request UUIDs blow cardinality",
            Regex::new(r"\bUuid\s*::\s*(?:new_v4|from_u128|parse_str)\b")
                .expect("static pattern compiles"),
        ),
        (
            "Uuid::to_string() — per-request UUIDs blow cardinality",
            Regex::new(r"\bUuid\s*\(\s*\)\s*\.\s*to_string\b").expect("static pattern compiles"),
        ),
        (
            "raw request_path — use a normalizer to bounded set",
            Regex::new(r"\brequest_path\b").expect("static pattern compiles"),
        ),
        (
            "user_email in label value — PII and unbounded",
            Regex::new(r"\buser_email\b").expect("static pattern compiles"),
        ),
        (
            "SystemTime::now() — unbounded time values",
            Regex::new(r"\bSystemTime\s*::\s*now\b").expect("static pattern compiles"),
        ),
    ]
});

// -----------------------------------------------------------------------------
// Comment-stripping (preserves line numbers + column offsets).
// -----------------------------------------------------------------------------

/// Replace Rust comment contents with spaces, preserving line numbers and
/// columns. String and char literals are respected. Block comments preserve
/// newlines.
///
/// Rust char literals (`'X'`, `'\n'`, `'\\'`, `'\''`) and lifetime-style
/// identifiers (`'a`) coexist; we recognize `'<char>'` only when the closer
/// is a `'` within ~4 bytes (including escape), and leave anything else as
/// regular tokens. This is strictly stronger than the Python kernel's
/// "treat single-quote as any other char" — Python misses cases like
/// `assert!(body.starts_with('"'));` where the middle `"` would otherwise
/// enter string mode and never exit (the original Python guard would have
/// the same gap; the parity-stronger fix here is intentional).
#[expect(
    clippy::indexing_slicing,
    reason = "byte walker — `i` is the loop induction var bounded by `while i < bytes.len()`; all `bytes[i]` / `bytes[i+1]` reads guarded by the loop condition or explicit length checks"
)]
fn strip_comments_preserve_layout(src: &str) -> String {
    let bytes = src.as_bytes();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    let mut in_str = false;
    while i < bytes.len() {
        let c = bytes[i];
        if in_str {
            out.push(c as char);
            if c == b'\\' && i + 1 < bytes.len() {
                out.push(bytes[i + 1] as char);
                i += 2;
                continue;
            }
            if c == b'"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        if c == b'"' {
            in_str = true;
            out.push('"');
            i += 1;
            continue;
        }
        // Rust char literal `'X'` or `'\X'`. Recognize by looking for a
        // matching `'` within 5 bytes (covers `'X'`, `'\X'`, `'\u{NN}'` up
        // to 6 chars — bound at 7 for safety). Anything longer is treated
        // as a lifetime-like ident and not consumed as a char literal.
        if c == b'\'' {
            if let Some(end) = find_char_lit_close(bytes, i) {
                for &b in bytes.iter().take(end + 1).skip(i) {
                    out.push(b as char);
                }
                i = end + 1;
                continue;
            }
            // Lifetime `'a` etc. — just emit and move on.
            out.push(c as char);
            i += 1;
            continue;
        }
        // Line comment.
        if c == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
            let mut j = i;
            while j < bytes.len() && bytes[j] != b'\n' {
                j += 1;
            }
            for _ in i..j {
                out.push(' ');
            }
            i = j;
            continue;
        }
        // Block comment.
        if c == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
            let mut j = i + 2;
            while j + 1 < bytes.len() && !(bytes[j] == b'*' && bytes[j + 1] == b'/') {
                j += 1;
            }
            let end = (j + 2).min(bytes.len());
            for &b in bytes.iter().take(end).skip(i) {
                if b == b'\n' {
                    out.push('\n');
                } else {
                    out.push(' ');
                }
            }
            i = end;
            continue;
        }
        out.push(c as char);
        i += 1;
    }
    out
}

/// Find the closing `'` of a char literal starting at `start` (the opening `'`).
/// Returns the index of the closing `'`. Bounded search prevents misclassifying
/// lifetimes (`'a`, `'static`) as char literals.
#[expect(
    clippy::indexing_slicing,
    reason = "bounded lookahead — `limit = (start + max_lookahead + 1).min(bytes.len())` clamps every `bytes[idx]` read; indexing cannot panic"
)]
fn find_char_lit_close(bytes: &[u8], start: usize) -> Option<usize> {
    // Standard char literal shapes (after the opening `'`):
    //   X'           single char (most common)
    //   \X'          escape (1 char + closing)
    //   \u{NNNN}'    unicode escape up to ~6 hex chars + closing
    let max_lookahead = 10;
    let limit = (start + max_lookahead + 1).min(bytes.len());
    if start + 1 >= bytes.len() {
        return None;
    }
    let first = bytes[start + 1];
    if first == b'\\' {
        // Escape sequence. Scan for the closing `'`.
        for (j, &b) in bytes.iter().enumerate().take(limit).skip(start + 2) {
            if b == b'\'' {
                return Some(j);
            }
            // Lifetimes never start with `\`, so any other byte is fine.
        }
        None
    } else {
        // Plain char: `'X'` — closing `'` should be at start+2.
        if start + 2 < bytes.len() && bytes[start + 2] == b'\'' {
            Some(start + 2)
        } else {
            // Could be a lifetime like `'a`, `'static`, or a multi-byte UTF-8
            // char literal. We refuse to consume — let the cursor advance one
            // byte and re-examine. This is the conservative choice that keeps
            // lifetimes intact.
            None
        }
    }
}

// -----------------------------------------------------------------------------
// Macro invocation finder (delimiter-generic balanced walker, string-literal
// aware). Handles `(`, `[` and `{` openers; see `find_macro_invocations`.
// -----------------------------------------------------------------------------

#[derive(Debug)]
struct MacroInvocation {
    kind: MacroKind,
    start_lineno: usize,
    body: Option<String>, // None on unterminated parse error
    /// The closer this invocation's opener called for, carried so the
    /// parse-error message can NAME it. Hardcoding `')'` was wrong for two
    /// thirds of the delimiters once brace and bracket forms became
    /// discoverable, and this is operator-facing output on a FAIL path.
    /// `None` when the opener itself was unrecognised.
    expected_close: Option<char>,
}

fn count_newlines_until(src: &str, idx: usize) -> usize {
    src[..idx.min(src.len())].matches('\n').count()
}

#[expect(
    clippy::indexing_slicing,
    reason = "delimiter-generic balanced walker — `open_idx = whole.end() - 1` is a regex-match end and the final class member is a single ASCII byte, so `bytes[open_idx]` is always within `src` bounds; subsequent indexing tracks `i < bytes.len()`"
)]
fn find_macro_invocations(src: &str) -> Vec<MacroInvocation> {
    let mut out = Vec::new();
    let bytes = src.as_bytes();
    for caps in MACRO_OPENER_RE.captures_iter(src) {
        let Some(macro_m) = caps.get(1) else { continue };
        let Some(whole) = caps.get(0) else { continue };
        let open_idx = whole.end() - 1;
        let start_lineno = count_newlines_until(src, whole.start()) + 1;
        // Regex group 1 in MACRO_OPENER_RE is the macro name; if it's not a
        // known variant the parser drops the invocation (defensive — regex
        // alternation only emits known names but the boundary stays explicit).
        let Some(kind) = MacroKind::parse(macro_m.as_str()) else {
            continue;
        };

        // The opener is whichever of `(`, `[`, `{` the regex matched, so the
        // depth counter must track THAT pair rather than parens specifically.
        // A macro body is delimiter-balanced in its own kind, and the other
        // two kinds are then correctly ignored as ordinary body text:
        // `counter!("m", if x { 1 } else { 2 })` counts only parens, and
        // `counter!{"m", "k" => f(a, b)}` counts only braces.
        let open = bytes[open_idx];
        let close = match open {
            b'(' => Some(b')'),
            b'[' => Some(b']'),
            b'{' => Some(b'}'),
            _ => None,
        };
        // UNREACHABLE via MACRO_OPENER_RE's character class today — and it
        // FAILS LOUD rather than being trusted to stay that way.
        //
        // An earlier draft wrote `_ => continue` under a comment claiming it
        // would "red instead of mis-parse". `continue` does neither: it drops
        // the invocation from the scan set entirely, so widening the regex's
        // delimiter class without widening this match would remove every
        // invocation using the new delimiter from the label PII checks with
        // the guard still reporting STATUS=OK. That is precisely the
        // clean-verdict-over-an-unchecked-scope failure this crate exists to
        // prevent, and the comment asserted the opposite of the behaviour.
        //
        // Routing it into the existing `body: None` path instead makes the
        // claim true: it surfaces as PARSE_ERROR_RULE_ID, which is a real
        // FAIL, and needs no new mechanism (CLAUDE.md §Fail loudly).
        let Some(close) = close else {
            out.push(MacroInvocation {
                kind,
                start_lineno,
                body: None,
                expected_close: None,
            });
            continue;
        };

        let mut depth: i32 = 1;
        let mut i = open_idx + 1;
        let mut in_str = false;
        let mut found_close: Option<usize> = None;
        while i < bytes.len() && depth > 0 {
            let c = bytes[i];
            if in_str {
                if c == b'\\' && i + 1 < bytes.len() {
                    i += 2;
                    continue;
                }
                if c == b'"' {
                    in_str = false;
                }
                i += 1;
                continue;
            }
            if c == b'"' {
                in_str = true;
                i += 1;
                continue;
            }
            // Line comment.
            if c == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            if c == open {
                depth += 1;
            } else if c == close {
                depth -= 1;
                if depth == 0 {
                    found_close = Some(i);
                    break;
                }
            }
            i += 1;
        }
        match found_close {
            Some(end) => {
                let body = src[open_idx + 1..end].to_string();
                out.push(MacroInvocation {
                    kind,
                    start_lineno,
                    body: Some(body),
                    expected_close: Some(close as char),
                });
            }
            None => out.push(MacroInvocation {
                kind,
                start_lineno,
                body: None,
                expected_close: Some(close as char),
            }),
        }
    }
    out
}

// -----------------------------------------------------------------------------
// Body parsing — top-level comma split + `=>` finder + string-literal parser.
// -----------------------------------------------------------------------------

#[expect(
    clippy::indexing_slicing,
    reason = "byte walker — `i` is the loop induction var bounded by `while i < bytes.len()`; depth counters (`dp`/`db`/`dbr`) track bracket balance, not slice indices"
)]
fn split_top_level_args(body: &str) -> Vec<String> {
    let bytes = body.as_bytes();
    let mut args = Vec::new();
    let mut cur: Vec<u8> = Vec::new();
    let (mut dp, mut db, mut dbr) = (0i32, 0i32, 0i32);
    let mut in_str = false;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if in_str {
            cur.push(c);
            if c == b'\\' && i + 1 < bytes.len() {
                cur.push(bytes[i + 1]);
                i += 2;
                continue;
            }
            if c == b'"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        if c == b'"' {
            in_str = true;
            cur.push(c);
            i += 1;
            continue;
        }
        match c {
            b'(' => dp += 1,
            b')' => dp -= 1,
            b'[' => dbr += 1,
            b']' => dbr -= 1,
            b'{' => db += 1,
            b'}' => db -= 1,
            b',' if dp == 0 && db == 0 && dbr == 0 => {
                let s = String::from_utf8_lossy(&cur).trim().to_string();
                args.push(s);
                cur.clear();
                i += 1;
                continue;
            }
            _ => {}
        }
        cur.push(c);
        i += 1;
    }
    let tail = String::from_utf8_lossy(&cur).trim().to_string();
    if !tail.is_empty() {
        args.push(tail);
    }
    args
}

#[expect(
    clippy::indexing_slicing,
    reason = "byte walker — `i + 1 < bytes.len()` loop guard ensures both `bytes[i]` and `bytes[i+1]` reads are in-bounds"
)]
fn find_top_level_fatarrow(arg: &str) -> Option<usize> {
    let bytes = arg.as_bytes();
    let (mut dp, mut db, mut dbr) = (0i32, 0i32, 0i32);
    let mut in_str = false;
    let mut i = 0;
    while i + 1 < bytes.len() {
        let c = bytes[i];
        if in_str {
            if c == b'\\' && i + 1 < bytes.len() {
                i += 2;
                continue;
            }
            if c == b'"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        if c == b'"' {
            in_str = true;
            i += 1;
            continue;
        }
        match c {
            b'(' => dp += 1,
            b')' => dp -= 1,
            b'[' => dbr += 1,
            b']' => dbr -= 1,
            b'{' => db += 1,
            b'}' => db -= 1,
            b'=' if bytes[i + 1] == b'>' && dp == 0 && db == 0 && dbr == 0 => return Some(i),
            _ => {}
        }
        i += 1;
    }
    None
}

fn parse_string_literal(expr: &str) -> Option<String> {
    let s = expr.trim();
    for rx in [
        &*STRING_LITERAL_RE,
        &*STRING_LITERAL_TO_STRING_RE,
        &*STRING_FROM_LITERAL_RE,
    ] {
        if let Some(caps) = rx.captures(s) {
            if let Some(m) = caps.get(1) {
                // Lightweight unescape — handle common Rust escapes.
                let raw = m.as_str();
                return Some(unescape_str(raw));
            }
        }
    }
    None
}

fn unescape_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some('\\') => out.push('\\'),
                Some('"') => out.push('"'),
                Some('\'') => out.push('\''),
                Some('0') => out.push('\0'),
                Some(other) => out.push(other),
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

// -----------------------------------------------------------------------------
// PII classification.
// -----------------------------------------------------------------------------

#[derive(Debug, PartialEq, Eq)]
enum PiiCategory {
    A,
    B,
    Prefix,
}

fn is_hashed_label(label: &str) -> bool {
    HASHED_SUFFIXES.iter().any(|s| label.ends_with(s))
}

fn token_hit_in_set(lower: &str, set: &[&str]) -> Option<String> {
    if set.contains(&lower) {
        return Some(lower.to_string());
    }
    // Multi-word tokens (`ip_addr`, `display_name`): substring match.
    for tok in set {
        if tok.contains('_') && lower.contains(tok) {
            return Some((*tok).to_string());
        }
    }
    // Single-word tokens: split on `_`.
    let parts: std::collections::HashSet<&str> = lower.split('_').collect();
    for tok in set {
        if tok.contains('_') {
            continue;
        }
        if parts.contains(tok) {
            return Some((*tok).to_string());
        }
    }
    None
}

fn pii_token_hit(label: &str) -> Option<(String, PiiCategory)> {
    if label.is_empty() {
        return None;
    }
    let lower = label.to_ascii_lowercase();

    // Prefix denylist (raw_*).
    for prefix in PII_PREFIX_DENYLIST {
        if lower.starts_with(prefix) {
            return Some(((*prefix).to_string(), PiiCategory::Prefix));
        }
    }

    // Category A unless explicitly allowlisted.
    if !CATEGORY_A_ALLOWLIST.iter().any(|a| *a == lower) {
        if let Some(tok) = token_hit_in_set(&lower, PII_TOKENS_CATEGORY_A) {
            return Some((tok, PiiCategory::A));
        }
    }

    // Category B: allowlist + hashed-suffix exemptions apply.
    if LABEL_ALLOWLIST.contains(&label) {
        return None;
    }
    if is_hashed_label(label) {
        return None;
    }
    if let Some(tok) = token_hit_in_set(&lower, PII_TOKENS_CATEGORY_B) {
        return Some((tok, PiiCategory::B));
    }
    None
}

fn naming_hygiene_issues(label: &str) -> Vec<String> {
    let mut out = Vec::new();
    if label.is_empty() {
        return out;
    }
    if label.chars().any(|c| c.is_ascii_uppercase()) {
        out.push(format!(
            "label key {label:?} contains uppercase; must be snake_case lowercase"
        ));
    }
    if label.contains('-') || label.contains('.') || label.contains(' ') {
        out.push(format!(
            "label key {label:?} contains disallowed character(s); use snake_case [a-z0-9_]"
        ));
    }
    if !SNAKE_CASE_IDENT_RE.is_match(label) {
        out.push(format!(
            "label key {label:?} is not a valid snake_case identifier"
        ));
    }
    out
}

fn unbounded_value_hit(value_expr: &str) -> Option<(&'static str, String)> {
    if value_expr.is_empty() {
        return None;
    }
    for (label, rx) in UNBOUNDED_VALUE_PATTERNS.iter() {
        if let Some(m) = rx.find(value_expr) {
            return Some((label, m.as_str().to_string()));
        }
    }
    None
}

fn literal_value_too_long(value_expr: &str) -> Option<usize> {
    parse_string_literal(value_expr).and_then(|lit| {
        if lit.len() > MAX_LITERAL_VALUE_LENGTH {
            Some(lit.len())
        } else {
            None
        }
    })
}

// -----------------------------------------------------------------------------
// Marker scanning.
// -----------------------------------------------------------------------------

fn load_pii_safe_markers(src: &str) -> (HashMap<usize, String>, Vec<(usize, String)>) {
    let mut markers = HashMap::new();
    let mut diagnostics = Vec::new();
    for (idx, line) in src.lines().enumerate() {
        let line_no = idx + 1;
        let Some(caps) = PII_SAFE_MARKER_RE.captures(line) else {
            continue;
        };
        let Some(reason_m) = caps.get(1) else {
            continue;
        };
        let mut reason = reason_m.as_str().trim().to_string();
        if reason.ends_with("*/") {
            reason = reason[..reason.len() - 2].trim().to_string();
        }
        if is_lazy_reason(&reason) {
            diagnostics.push((
                line_no,
                format!(
                    "pii-safe reason too short or too vague: {reason:?} \
                     (require >=10 chars, not test/tmp/todo/fixme/wip)"
                ),
            ));
            continue;
        }
        markers.insert(line_no, reason);
    }
    (markers, diagnostics)
}

fn invocation_is_pii_safe(markers: &HashMap<usize, String>, start_lineno: usize) -> Option<String> {
    if let Some(r) = markers.get(&start_lineno) {
        return Some(r.clone());
    }
    if start_lineno > 0 {
        if let Some(r) = markers.get(&(start_lineno - 1)) {
            return Some(r.clone());
        }
    }
    None
}

// -----------------------------------------------------------------------------
// Finding
// -----------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Finding {
    file: String,
    line: usize,
    rule_id: &'static str,
    message: String,
}

impl Finding {
    fn print(&self, explain: bool) {
        if explain {
            let policy = format!("metric-labels::{}", self.rule_id);
            crate::common::explain::print_finding(&crate::common::explain::Finding {
                file: &self.file,
                row: self.line,
                col: 0,
                policy: &policy,
                matched: &self.message,
                extras: &[],
                src_file: file!(),
                src_line: line!(),
            });
        } else {
            println!(
                "VIOLATION: {}:{} ({}) {}",
                self.file, self.line, self.rule_id, self.message
            );
        }
    }
}

// -----------------------------------------------------------------------------
// File enumeration.
// -----------------------------------------------------------------------------

fn find_metric_files(repo_root: &Path) -> Vec<PathBuf> {
    let crates_dir = repo_root.join(CRATES_SUBDIR);
    let mut primary: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&crates_dir) {
        for entry in entries.flatten() {
            let mp = entry.path().join("src/observability/metrics.rs");
            if mp.is_file() {
                primary.push(mp);
            }
        }
    }
    primary.sort();
    let primary_set: std::collections::HashSet<PathBuf> = primary.iter().cloned().collect();

    // Secondary: any *.rs that invokes a metrics macro, excluding target/vendor.
    let mut secondary: Vec<PathBuf> = Vec::new();
    if crates_dir.is_dir() {
        for entry in WalkDir::new(&crates_dir)
            .into_iter()
            .filter_entry(|e| {
                let name = e.file_name().to_string_lossy();
                name != "target" && name != "vendor"
            })
            .filter_map(Result::ok)
        {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("rs") {
                continue;
            }
            if primary_set.contains(path) {
                continue;
            }
            // Guard-internal + test paths are exempt — guards' literal pattern
            // catalogs and macro-shaped test fixtures self-match otherwise.
            let rel = to_repo_relative(repo_root, path);
            if is_scan_exempt(Path::new(&rel)) {
                continue;
            }
            let src = match std::fs::read_to_string(path) {
                Ok(s) => s,
                Err(e) => {
                    warn_skip("secondary metric source read", path, &e);
                    continue;
                }
            };
            if MACRO_OPENER_RE.is_match(&src) {
                secondary.push(path.to_path_buf());
            }
        }
    }
    secondary.sort();
    let mut all = primary;
    all.extend(secondary);
    all
}

// -----------------------------------------------------------------------------
// Entry point
// -----------------------------------------------------------------------------

/// Per-run tallies of the kind-label rules: the positive control that the
/// matcher saw real sites (zero is vacuity, not a clean pass).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct KindStats {
    /// Metric-macro label args whose key is a kind key, outside test code.
    pub sites: usize,
}

/// The kind-label rules' inputs: the resolved keys, or the vacuity finding
/// that stands in for them.
fn load_kind_keys(repo_root: &Path, findings: &mut Vec<Finding>) -> Option<KindKeys> {
    let vacuous = |message: String| Finding {
        file: LABELS_RS.to_string(),
        line: 0,
        rule_id: KIND_LABEL_VACUOUS_RULE_ID,
        message,
    };
    let keys = match std::fs::read_to_string(repo_root.join(LABELS_RS)) {
        Ok(src) => match KindKeys::load(&src) {
            Ok(k) => Some(k),
            Err(e) => {
                findings.push(vacuous(e));
                None
            }
        },
        Err(e) => {
            findings.push(vacuous(format!("cannot read {LABELS_RS}: {e}")));
            None
        }
    };
    // The Rust projection home must exist and hold the projection: a moved or
    // emptied home would leave the allowance pointing at nothing.
    let home_ok = std::fs::read_to_string(repo_root.join(RUST_PROJECTION_HOME))
        .is_ok_and(|src| rust_home_holds_the_projection(&src));
    if !home_ok {
        findings.push(Finding {
            file: RUST_PROJECTION_HOME.to_string(),
            line: 0,
            rule_id: KIND_LABEL_VACUOUS_RULE_ID,
            message: format!(
                "the Rust kind-label projection home {RUST_PROJECTION_HOME} is missing or \
                 holds no `label()` value literals; update RUST_PROJECTION_HOME \
                 (common::kind_labels) in the same change that moves it"
            ),
        });
    }
    keys
}

/// The `pub const fn label(self) -> &'static str` value functions of the Rust
/// projection home, as the number of string-literal match arms in each.
fn rust_home_label_arms(src: &str) -> Vec<usize> {
    const LABEL_FN: &str = "pub const fn label(self) -> &'static str {";
    src.match_indices(LABEL_FN)
        .map(|(at, _)| {
            let body = src.get(at + LABEL_FN.len()..).unwrap_or("");
            // The fn body ends at the first line that closes it at the
            // method's own indentation (`    }`).
            let end = body.find("\n    }\n").unwrap_or(body.len());
            body.get(..end).unwrap_or("").matches("=> \"").count()
        })
        .collect()
}

/// Whether the Rust projection home holds the projection: `struct
/// KindLabels`, its `from_proto`, and ONE value function per kind key, each
/// with at least two value arms (a closed set has at least two values).
fn rust_home_holds_the_projection(src: &str) -> bool {
    let arms = rust_home_label_arms(src);
    src.contains("pub struct KindLabels")
        && src.contains("pub const fn from_proto(")
        && arms.len() == crate::common::kind_labels::KIND_LABEL_KEYS.len()
        && arms.iter().all(|n| *n >= 2)
}

/// Every finding over the real tree, plus the files scanned and the kind
/// tallies (for the positive-control tests).
fn collect(repo_root: &Path) -> Result<(Vec<Finding>, usize, KindStats)> {
    let rs_files = find_metric_files(repo_root);
    let mut findings: Vec<Finding> = Vec::new();
    let mut stats = KindStats::default();
    let kind = load_kind_keys(repo_root, &mut findings);
    for rs_path in &rs_files {
        let rel_path = to_repo_relative(repo_root, rs_path);
        let src = std::fs::read_to_string(rs_path)
            .with_context(|| format!("read metrics source {rel_path}"))?;

        let (markers, lazy_diags) = load_pii_safe_markers(&src);
        for (lineno, msg) in lazy_diags {
            findings.push(Finding {
                file: rel_path.clone(),
                line: lineno,
                rule_id: LAZY_PII_SAFE_REASON_RULE_ID,
                message: msg,
            });
        }

        let stripped = strip_comments_preserve_layout(&src);
        check_source(&stripped, &rel_path, &markers, &mut findings);
        if let Some(kind) = &kind {
            check_kind_labels(&stripped, &rel_path, kind, &mut findings, &mut stats);
        }
    }
    if kind.is_some() && stats.sites == 0 {
        findings.push(Finding {
            file: "crates".to_string(),
            line: 0,
            rule_id: KIND_LABEL_VACUOUS_RULE_ID,
            message: "no metric-macro label arg keyed by a kind label was recognised in the \
                      tree; MC's per-kind recorders carry several, so the matcher is not \
                      seeing them"
                .to_string(),
        });
    }
    Ok((findings, rs_files.len(), stats))
}

pub fn run(repo_root: &Path, explain: bool) -> Result<()> {
    let (findings, files, _) = collect(repo_root)?;
    if files == 0 && findings.is_empty() {
        emit_ok("metric-labels-no-files");
        return Ok(());
    }

    if findings.is_empty() {
        emit_ok(format!("metric-labels-clean-{files}-files"));
        return Ok(());
    }
    for f in &findings {
        f.print(explain);
    }
    anyhow::bail!(
        "metric-labels: {} violation(s) across {} file(s)",
        findings.len(),
        files
    );
}

/// The kind-label rules over one comment-stripped file. Test code is exempt
/// (`#[cfg(test)]` blocks via `common::test_code_filter`; test PATHS never
/// reach here, `find_metric_files` drops them).
fn check_kind_labels(
    stripped_src: &str,
    rel_path: &str,
    kind: &KindKeys,
    findings: &mut Vec<Finding>,
    stats: &mut KindStats,
) {
    let test_blocks = compute_test_block_ranges(stripped_src);
    for inv in find_macro_invocations(stripped_src) {
        if inv.kind.is_describe() || is_line_in_test_block(&test_blocks, inv.start_lineno) {
            continue;
        }
        let Some(body) = inv.body.as_ref() else {
            continue; // reported as parse_error by `check_source`
        };
        for arg in split_top_level_args(body).iter().skip(1) {
            let Some(idx) = find_top_level_fatarrow(arg) else {
                continue;
            };
            let (key_expr, val_expr) = (
                arg.get(..idx).unwrap_or(""),
                arg.get(idx + 2..).unwrap_or(""),
            );
            let Some(key) = kind.key_of(key_expr) else {
                continue;
            };
            stats.sites += 1;
            if rel_path.starts_with(MH_CRATE_PREFIX) {
                findings.push(Finding {
                    file: rel_path.to_string(),
                    line: inv.start_lineno,
                    rule_id: KIND_LABEL_ON_MH_RULE_ID,
                    message: format!(
                        "`{key}` on an MH metric: MH is type-blind (ADR-0036 §7) and labels \
                         by behaviour, never by kind — remove the label"
                    ),
                });
            } else if rel_path != RUST_PROJECTION_HOME && !is_projection_value(val_expr) {
                findings.push(Finding {
                    file: rel_path.to_string(),
                    line: inv.start_lineno,
                    rule_id: KIND_LABEL_LITERAL_RULE_ID,
                    message: format!(
                        "`{key}` value `{}` is not a projection value: take a `KindLabels` \
                         (crates/mc-service/src/media_kind.rs) and pass `….{key}.label()`; \
                         kind-label values are spelled only in the projection home",
                        val_expr.trim()
                    ),
                });
            }
        }
    }
}

#[expect(
    clippy::indexing_slicing,
    reason = "consumes pre-validated `MacroInvocation` records from `find_macro_invocations`; body/arg-slice indices are bounded by the parsed invocation spans"
)]
fn check_source(
    stripped_src: &str,
    rel_path: &str,
    markers: &HashMap<usize, String>,
    findings: &mut Vec<Finding>,
) {
    let invocations = find_macro_invocations(stripped_src);
    for inv in &invocations {
        let Some(body) = inv.body.as_ref() else {
            findings.push(Finding {
                file: rel_path.to_string(),
                line: inv.start_lineno,
                rule_id: PARSE_ERROR_RULE_ID,
                message: match inv.expected_close {
                    Some(close) => format!(
                        "could not find matching '{}' for {}! invocation",
                        close,
                        inv.kind.as_str()
                    ),
                    // Unrecognised opener: MACRO_OPENER_RE's delimiter class
                    // was widened without teaching the walker its closer.
                    None => format!(
                        "unrecognised opening delimiter for {}! invocation — \
                         MACRO_OPENER_RE accepts a delimiter `find_macro_invocations` \
                         cannot match a closer for; widen the `close` match in the \
                         same commit as the regex",
                        inv.kind.as_str()
                    ),
                },
            });
            continue;
        };

        let pii_safe = invocation_is_pii_safe(markers, inv.start_lineno);
        let is_describe = inv.kind.is_describe();

        let args = split_top_level_args(body);
        if args.is_empty() {
            continue;
        }

        // First arg = metric name.
        let metric_name_arg = &args[0];
        if let Some(lit) = parse_string_literal(metric_name_arg) {
            if lit.len() > MAX_LITERAL_VALUE_LENGTH {
                findings.push(Finding {
                    file: rel_path.to_string(),
                    line: inv.start_lineno,
                    rule_id: METRIC_NAME_LENGTH_RULE_ID,
                    message: format!(
                        "metric name {lit:?} is {} chars (> {MAX_LITERAL_VALUE_LENGTH} char limit)",
                        lit.len()
                    ),
                });
            }
            if !lit.is_empty() && !SNAKE_CASE_IDENT_RE.is_match(&lit) {
                findings.push(Finding {
                    file: rel_path.to_string(),
                    line: inv.start_lineno,
                    rule_id: METRIC_NAME_NAMING_RULE_ID,
                    message: format!(
                        "metric name {lit:?} is not snake_case (must match ^[a-z_][a-z0-9_]*$)"
                    ),
                });
            }
        }

        // Subsequent args = `"key" => value` pairs (for non-describe macros).
        for arg in args.iter().skip(1) {
            let Some(idx) = find_top_level_fatarrow(arg) else {
                // No `=>` found.
                // Describe macros take (name, description) — second arg is a
                // description string, not a label. Skip silently in that case
                // OR when the trailing arg is itself a string literal (variadic
                // counter form like `counter!("m")`).
                let trimmed = arg.trim();
                let is_str_lit = STRING_LITERAL_RE.is_match(trimmed);
                if !is_describe && !arg.is_empty() && !is_str_lit {
                    findings.push(Finding {
                        file: rel_path.to_string(),
                        line: inv.start_lineno,
                        rule_id: PARSE_ERROR_RULE_ID,
                        message: format!("could not find '=>' in label arg {arg:?}"),
                    });
                }
                continue;
            };
            if is_describe {
                continue;
            }

            let key_expr = arg[..idx].trim();
            let val_expr = arg[idx + 2..].trim();
            let key_literal = parse_string_literal(key_expr);

            if let Some(ref key) = key_literal {
                for msg in naming_hygiene_issues(key) {
                    findings.push(Finding {
                        file: rel_path.to_string(),
                        line: inv.start_lineno,
                        rule_id: LABEL_NAMING_RULE_ID,
                        message: msg,
                    });
                }

                if let Some((tok, cat)) = pii_token_hit(key) {
                    match cat {
                        PiiCategory::A => findings.push(Finding {
                            file: rel_path.to_string(),
                            line: inv.start_lineno,
                            rule_id: LABEL_SECRET_RULE_ID,
                            message: format!(
                                "label key {key:?} matches secret denylist token {tok:?} \
                                 (Category A, non-bypassable per ADR-0011) — never \
                                 place credentials in metric labels; remove the label"
                            ),
                        }),
                        PiiCategory::Prefix if pii_safe.is_none() => findings.push(Finding {
                            file: rel_path.to_string(),
                            line: inv.start_lineno,
                            rule_id: LABEL_PII_RULE_ID,
                            message: format!(
                                "label key {key:?} has denylisted prefix {tok:?} \
                                     — denylisted prefixes mark identifiers that must \
                                     not reach a metric label; rename or remove the \
                                     label. `# pii-safe: <reason>` suppresses this \
                                     check, but whether a suppression is legitimate \
                                     depends on the prefix: see \
                                     docs/observability/label-taxonomy.md §Prefix denylist"
                            ),
                        }),
                        PiiCategory::B if pii_safe.is_none() => findings.push(Finding {
                            file: rel_path.to_string(),
                            line: inv.start_lineno,
                            rule_id: LABEL_PII_RULE_ID,
                            message: format!(
                                "label key {key:?} matches PII denylist token {tok:?} \
                                     — use a hashed/opaque form (e.g., {key}_hash) or add \
                                     `# pii-safe: <reason>` on or above this line"
                            ),
                        }),
                        _ => {}
                    }
                }
            }

            // Rule 2: cardinality — runs regardless of pii_safe.
            if let Some(too_long) = literal_value_too_long(val_expr) {
                findings.push(Finding {
                    file: rel_path.to_string(),
                    line: inv.start_lineno,
                    rule_id: LITERAL_VALUE_LENGTH_RULE_ID,
                    message: format!(
                        "label value literal is {too_long} chars (> {MAX_LITERAL_VALUE_LENGTH}) \
                         — label values are series dimensions; keep short"
                    ),
                });
            }
            if let Some((pat_label, match_text)) = unbounded_value_hit(val_expr) {
                findings.push(Finding {
                    file: rel_path.to_string(),
                    line: inv.start_lineno,
                    rule_id: UNBOUNDED_VALUE_RULE_ID,
                    message: format!("label value expression contains {match_text:?}: {pat_label}"),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// DERIVATION, not equality — every `MacroKind` name must still arrive
    /// through THIS consumer's real discovery path with zero edits here.
    ///
    /// # Why this exists, and why it is not redundant with the SoT pin
    ///
    /// `metric_macros::macro_name_alternation_is_frozen_against_narrowing`
    /// pins the SoT STRING's membership and order. It cannot catch a
    /// **consumer un-deriving**: if someone re-inlines the six names as a
    /// literal in `MACRO_OPENER_RE` — the exact state G6 found and fixed on
    /// 2026-09-08, when this file was the sole holdout among four consumers —
    /// the SoT pin still passes (the SoT string is untouched) and the spelling
    /// tests still pass (an identical literal matches all the same inputs).
    /// Nothing would red. This is the test that would.
    ///
    /// The two oracle types are deliberately different and are meant to sit
    /// side by side, exactly as `telemetry_macros` holds
    /// `level_group_membership_is_frozen` (frozen literal) next to
    /// `every_all_member_is_reachable_through_its_group` (derivation). The
    /// anti-narrowing warning against self-comparison applies to the FROZEN
    /// oracle only; self-comparison is correct here, because the property
    /// under test is reachability, not membership.
    ///
    /// Asserted through `find_macro_invocations` rather than against
    /// `MACRO_OPENER_RE.as_str()`, so it exercises the opener AND the walker —
    /// a delimiter or whitespace regression reds this too. Modelled on
    /// `media_telemetry_deny::tests::metrics_family_is_derived_from_macro_kind_all`,
    /// the sibling consumer that already had this test. (@infrastructure F13.)
    #[test]
    fn every_macro_kind_is_discovered_through_the_opener() {
        for kind in MacroKind::ALL {
            let src = format!(r#"{}!("m", "k" => v);"#, kind.as_str());
            let invs = find_macro_invocations(&src);
            assert_eq!(
                invs.len(),
                1,
                "`{}!` is in MacroKind::ALL but metric_labels does not discover it — \
                 the derivation is cosmetic. Either MACRO_OPENER_RE stopped deriving \
                 from MACRO_NAME_ALTERNATION (the G6 regression: a re-inlined literal \
                 passes every other test in this file), or the anchor narrowed.",
                kind.as_str()
            );
            assert_eq!(invs[0].kind, *kind, "discovered as the wrong kind");
        }
    }

    /// BOTH halves of the 2026-09-08 anchor widening: whitespace before the
    /// `!`, and the non-paren delimiters.
    ///
    /// `counter !("m")` and `counter!{"m", …}` are both legal Rust that the
    /// pre-2026-09-08 anchor missed. Matching is only half the property — see
    /// [`brace_and_bracket_invocation_bodies_are_actually_parsed`], which
    /// covers the body scanner. A widened opener whose scanner still hunted
    /// for `)` would match here and then silently yield no body, which is the
    /// same missed label check wearing a different hat.
    #[test]
    fn macro_opener_accepts_space_before_bang_and_all_three_delimiters() {
        for s in [
            r#"counter !("m", "k" => v);"#,
            r#"counter  !("m");"#,
            r#"metrics :: describe_counter !("m", "d");"#,
            r#"counter!{"m", "k" => v}"#,
            r#"counter!["m"]"#,
            r#"gauge !{"m"}"#,
        ] {
            assert!(
                MACRO_OPENER_RE.is_match(s),
                "must match after widening: {s}"
            );
        }
        // Must not reach past the name boundary.
        assert!(!MACRO_OPENER_RE.is_match("my_counter !(x)"));
        // `!=` is not an invocation: the char after `!` must be a delimiter.
        assert!(!MACRO_OPENER_RE.is_match("if counter != (0) {"));
    }

    /// The body scanner half of the delimiter widening.
    ///
    /// This is the assertion that would have caught a half-done fix: the
    /// opener regex and the depth loop are two encodings of "what delimits a
    /// macro body", and widening only the first yields an invocation with
    /// `body: None` — no labels, no PII check, and no finding either way, so
    /// nothing reds. Each case asserts the body was RECOVERED, not merely that
    /// the invocation was discovered.
    ///
    /// The nesting cases matter for the same reason: the loop counts only its
    /// OWN delimiter kind, so a paren inside a brace body (and vice versa)
    /// must be ignored rather than throwing the depth off.
    #[test]
    fn brace_and_bracket_invocation_bodies_are_actually_parsed() {
        for (src, want_kind) in [
            (r#"counter!{"m", "k" => v};"#, MacroKind::Counter),
            (r#"gauge!["m", "k" => v];"#, MacroKind::Gauge),
            // Foreign delimiters inside the body are ordinary text.
            (r#"counter!{"m", "k" => f(a, b)};"#, MacroKind::Counter),
            (
                r#"counter!("m", "k" => if x { 1 } else { 2 });"#,
                MacroKind::Counter,
            ),
            // Same-kind nesting must still balance.
            (
                r#"counter!{"m", "k" => g(|| { h() })};"#,
                MacroKind::Counter,
            ),
        ] {
            let invs = find_macro_invocations(src);
            assert_eq!(invs.len(), 1, "expected one invocation in: {src}");
            assert_eq!(invs[0].kind, want_kind, "wrong kind for: {src}");
            // `assert!` + a plain read, NOT `unwrap_or_else(|| panic!(...))`:
            // `clippy::panic` is denied workspace-wide (`Cargo.toml`) and
            // `clippy.toml` grants `allow-expect-in-tests` / `-unwrap-in-tests`
            // but deliberately NOT `allow-panic-in-tests`. The assertion form
            // keeps `{src}` in the message, which `.expect()` could not.
            assert!(
                invs[0].body.is_some(),
                "body must be recovered, not None, for: {src}"
            );
            let body = invs[0].body.as_deref().unwrap_or_default();
            assert!(
                body.starts_with('"'),
                "body should start at the first label arg, got {body:?} for: {src}"
            );
        }
    }

    /// The unterminated-invocation message NAMES the delimiter it wanted.
    ///
    /// It was hardcoded `')'` until 2026-09-08, which became wrong for two of
    /// three delimiters the moment brace and bracket forms were discoverable.
    /// Operator-facing output on a FAIL path: telling someone looking at a `{`
    /// that a `)` is missing costs a real incident minute.
    #[test]
    fn unterminated_invocation_names_the_delimiter_it_expected() {
        let name: &str = "counter";
        for (src, want) in [
            (format!(r#"{name}!("m", "k" => x"#), ')'),
            (format!(r#"{name}!{{"m", "k" => x"#), '}'),
            (format!(r#"{name}!["m", "k" => x"#), ']'),
        ] {
            let invs = find_macro_invocations(&src);
            assert_eq!(invs.len(), 1, "expected one invocation in: {src}");
            assert!(invs[0].body.is_none(), "should be unterminated: {src}");
            assert_eq!(
                invs[0].expected_close,
                Some(want),
                "wrong expected closer for: {src}"
            );
        }
    }

    /// A brace-delimited invocation carrying an unbounded PII-bearing label is
    /// FOUND, which is why the delimiter half was not left as debt (ADR-0029).
    ///
    /// Before 2026-09-08 this shipped with nothing firing: the opener never
    /// matched, so the invocation was never discovered and its labels were
    /// never checked.
    #[test]
    fn brace_form_unbounded_label_is_not_invisible() {
        let src = r#"counter!{"mh_frames_total", "email" => user_email.to_string()};"#;
        let invs = find_macro_invocations(src);
        assert_eq!(invs.len(), 1, "brace-form invocation must be discovered");
        let body = invs[0].body.as_ref().expect("body must be recovered");
        assert!(
            body.contains("user_email"),
            "the label value must reach the PII checks; got {body:?}"
        );
    }

    #[test]
    fn cat_a_token_fires_without_allowlist() {
        let hit = pii_token_hit("token").unwrap();
        assert_eq!(hit.0, "token");
        assert_eq!(hit.1, PiiCategory::A);
    }

    #[test]
    fn cat_a_allowlist_token_type_exempt() {
        // token_type is in CATEGORY_A_ALLOWLIST; should NOT match Cat A.
        assert!(pii_token_hit("token_type").is_none());
    }

    #[test]
    fn cat_b_token_fires() {
        let hit = pii_token_hit("email").unwrap();
        assert_eq!(hit.0, "email");
        assert_eq!(hit.1, PiiCategory::B);
    }

    /// Wave-3 cohort. These execute the matcher-shape reasoning the vocabulary
    /// entries' comments assert in prose — the module's own `accessToken` note
    /// records that this class of derivation was previously gotten backwards.
    #[test]
    fn wave3_media_path_tokens_fire_in_expected_categories() {
        let kek = pii_token_hit("meeting_kek").expect("meeting_kek must fire");
        assert_eq!(kek.1, PiiCategory::A);

        let sender = pii_token_hit("sender_id").expect("sender_id must fire");
        assert_eq!(sender.1, PiiCategory::B);

        // The claim most worth executing: the multi-word substring pass is what
        // makes the entry cover compounds, which the word-boundary consumers do
        // NOT (an underscore is a word character, so `\bsender_id\b` misses this).
        let compound = pii_token_hit("pinned_sender_id").expect("compound must fire");
        assert_eq!(compound.1, PiiCategory::B);
    }

    /// Pins the documented GAP rather than a capability: `sender_id_hash` is
    /// exempted by `is_hashed_label()` and is therefore NOT covered by the
    /// CATEGORY_B `sender_id` entry. Recorded as a test so the gap is visible in
    /// test output rather than only in prose, and so it fails loudly if someone
    /// changes the hashed-suffix exemption without revisiting the reasoning.
    #[test]
    fn sender_id_hash_is_not_covered() {
        assert!(
            pii_token_hit("sender_id_hash").is_none(),
            "sender_id_hash is expected to be EXEMPT — if this now fires, the \
             label-taxonomy 'Enforcement reality' trigger has been discharged \
             and that note must be updated"
        );
    }

    #[test]
    fn hashed_suffix_exempts_cat_b_only() {
        assert!(pii_token_hit("email_hash").is_none());
        assert!(pii_token_hit("user_id_hash").is_none());
        // Cat A is NOT exempted by hashed suffix.
        let hit = pii_token_hit("password_hash").unwrap();
        assert_eq!(hit.1, PiiCategory::A);
    }

    #[test]
    fn prefix_denylist_fires() {
        let hit = pii_token_hit("raw_anything").unwrap();
        assert_eq!(hit.0, "raw_");
        assert_eq!(hit.1, PiiCategory::Prefix);
    }

    /// ADR-0036 §11 — "no meeting identifier on any metric anywhere in this
    /// design" — as a guard rather than a review note. The partition is
    /// load-bearing, not incidental:
    ///
    /// * `PII_PREFIX_DENYLIST` has exactly ONE consumer (`pii_token_hit`, and
    ///   through it only the metric-label scanner), so this bars `meeting_id`
    ///   on Rust *metric labels* without firing on the legitimate
    ///   control-plane `meeting_id = %meeting_id` *log* fields that a
    ///   CATEGORY_A or CATEGORY_B entry would break across MC, GC and MH —
    ///   both of those have three consumers each, reaching the log and
    ///   `#[instrument]` scanners.
    /// * The prefix loop runs FIRST inside `pii_token_hit`, so neither
    ///   `is_hashed_label()` nor `LABEL_ALLOWLIST` can rescue
    ///   `meeting_id_hash`. (§11's grandfathered exception is the TS client
    ///   SDK, structurally out of reach of a `crates/`-only scanner.)
    ///
    /// Asserting `PiiCategory::Prefix` SPECIFICALLY is the whole point: a
    /// later move of the term into CATEGORY_B would keep a weaker
    /// "something fired" assertion green while silently restoring both the
    /// hashed-suffix exemption and `LABEL_ALLOWLIST` suppression.
    ///
    /// KNOWN LIMIT — recorded so this is not mistaken for non-bypassability:
    /// the `PiiCategory::Prefix` arm at the finding site is gated on
    /// `pii_safe.is_none()`, so `# pii-safe: <reason>` still suppresses it.
    /// Only CATEGORY_A is non-bypassable. Making Prefix non-bypassable is a
    /// change to the emission `match` — guard machinery, infrastructure-owned,
    /// deliberately not done here.
    #[test]
    fn meeting_id_fires_as_prefix_on_metric_labels() {
        let bare = pii_token_hit("meeting_id").expect("meeting_id must fire");
        assert_eq!(bare.0, "meeting_id");
        assert_eq!(
            bare.1,
            PiiCategory::Prefix,
            "must be Prefix, not Cat B — Cat B would re-expose it to the \
             hashed-suffix exemption and to LABEL_ALLOWLIST"
        );

        // The hashed spelling is what a well-meaning author reaches for, and
        // catching it is precisely why the Prefix partition was chosen.
        let hashed = pii_token_hit("meeting_id_hash").expect("meeting_id_hash must fire");
        assert_eq!(hashed.1, PiiCategory::Prefix);
    }

    /// The scope limit R1 documents, pinned as a test rather than left in prose.
    ///
    /// The match is `starts_with`, not segment-splitting, so a **trailing**
    /// compound does not fire. That is a real gap in R1's stated scope ("any
    /// metric anywhere in this design") and it is tracked as `docs/TODO.md` D4,
    /// whose real fix is a segment-splitting matcher — guard machinery,
    /// infrastructure-owned, deliberately not done here.
    ///
    /// Asserted rather than described for the same reason
    /// `sender_id_hash_is_not_covered` exists: a documented gap that no test
    /// pins is invisible in test output, so the day the matcher gains segment
    /// splitting nothing tells the author that this case changed behaviour.
    /// **If this assertion starts failing, that is good news** — delete it and
    /// close D4's `starts_with` bullet.
    #[test]
    fn trailing_meeting_id_compound_is_not_covered() {
        assert!(
            pii_token_hit("x_meeting_id").is_none(),
            "documented scope limit: the prefix match is `starts_with`, so a trailing \
             compound is NOT caught — see docs/TODO.md D4"
        );
    }

    #[test]
    fn label_allowlist_suppresses_cat_b() {
        // `hostname` contains `name` substring but is allowlisted.
        assert!(pii_token_hit("hostname").is_none());
    }

    #[test]
    fn strip_comments_preserves_layout() {
        // Build the macro-name token by concatenation so the guard's macro-opener
        // regex doesn't see `counter!(...` literally in this test fixture.
        let macro_name: &str = "counter";
        let src_template = "fn foo() {\n// MACRO!(\"x\")\n}\n";
        let src = src_template.replace("MACRO", macro_name);
        let stripped = strip_comments_preserve_layout(&src);
        assert!(!stripped.contains("counter!"));
        assert_eq!(stripped.lines().count(), src.lines().count());
    }

    #[test]
    fn strip_comments_respects_strings() {
        let src = r#"let x = "// not a comment";"#;
        let stripped = strip_comments_preserve_layout(src);
        assert!(stripped.contains("// not a comment"));
    }

    #[test]
    fn split_top_level_handles_nested() {
        let body = r#""metric_name", "key" => x.to_string(), "k2" => "v""#;
        let args = split_top_level_args(body);
        assert_eq!(args.len(), 3);
        assert_eq!(args[0], r#""metric_name""#);
        assert_eq!(args[1], r#""key" => x.to_string()"#);
    }

    #[test]
    fn parse_string_literal_handles_forms() {
        assert_eq!(parse_string_literal(r#""hello""#).as_deref(), Some("hello"));
        assert_eq!(
            parse_string_literal(r#""hello".to_string()"#).as_deref(),
            Some("hello")
        );
        assert_eq!(
            parse_string_literal(r#"String::from("hello")"#).as_deref(),
            Some("hello")
        );
        assert!(parse_string_literal("some_var").is_none());
    }

    #[test]
    fn find_macro_invocations_balanced_parens() {
        let src = r#"counter!("m", "k" => x.to_string());"#;
        let invs = find_macro_invocations(src);
        assert_eq!(invs.len(), 1);
        assert_eq!(invs[0].kind, MacroKind::Counter);
        let body = invs[0].body.as_ref().unwrap();
        assert!(body.starts_with('"'));
    }

    #[test]
    fn find_macro_invocations_unterminated() {
        // Build the macro-name token by concatenation so the guard's
        // macro-opener regex doesn't see `counter!(...` literally in this fixture.
        let macro_name: &str = "counter";
        let src = format!(r#"{macro_name}!("m", "k" => x"#);
        let invs = find_macro_invocations(&src);
        assert_eq!(invs.len(), 1);
        assert!(invs[0].body.is_none());
    }

    #[test]
    fn unbounded_value_detects_uuid() {
        let hit = unbounded_value_hit("Uuid::new_v4().to_string()").unwrap();
        assert!(hit.0.contains("Uuid"));
    }

    #[test]
    fn literal_value_too_long_detects() {
        let long = format!(r#""{}""#, "a".repeat(65));
        assert_eq!(literal_value_too_long(&long), Some(65));
        let short = r#""short""#;
        assert_eq!(literal_value_too_long(short), None);
    }

    // -- Kind-label rules (2026-10-26) ----------------------------------------

    const KIND_LABELS_RS: &str = r#"
pub const MEDIA_KIND_LABEL: &str = "media_kind";
pub const CONTENT_KIND_LABEL: &str = "content_kind";
"#;
    const MC: &str = "crates/mc-service/src/observability/metrics.rs";

    fn kind_findings_with(
        src: &str,
        path: &str,
        keys: &KindKeys,
    ) -> (Vec<(&'static str, String, usize)>, KindStats) {
        let mut f = Vec::new();
        let mut stats = KindStats::default();
        check_kind_labels(
            &strip_comments_preserve_layout(src),
            path,
            keys,
            &mut f,
            &mut stats,
        );
        (
            f.into_iter().map(|x| (x.rule_id, x.file, x.line)).collect(),
            stats,
        )
    }

    fn kind_findings(src: &str, path: &str) -> (Vec<(&'static str, String, usize)>, KindStats) {
        kind_findings_with(src, path, &KindKeys::load(KIND_LABELS_RS).unwrap())
    }

    /// Each red form, with the exact (rule, file, line) it must produce.
    const RED: &[(&str, usize)] = &[
        // const key, literal value
        ("fn a() {\n    counter!(\"m\", MEDIA_KIND_LABEL => \"audio\").increment(1);\n}\n", 2),
        // literal key, literal value
        ("fn a() {\n    counter!(\"m\", \"content_kind\" => \"main\").increment(1);\n}\n", 2),
        // path-qualified const key, bare variable value
        ("fn a(v: &str) {\n\n    gauge!(\"m\", common::observability::labels::MEDIA_KIND_LABEL => v).set(1.0);\n}\n", 3),
        // brace form, `.to_string()` literal
        ("fn a() {\n    counter!{\"m\", CONTENT_KIND_LABEL => \"screen\".to_string()}.increment(1);\n}\n", 2),
    ];

    #[test]
    fn kind_label_literal_hits_every_red_form_at_its_line() {
        for (src, line) in RED {
            let (f, stats) = kind_findings(src, MC);
            assert_eq!(
                f,
                vec![(KIND_LABEL_LITERAL_RULE_ID, MC.to_string(), *line)],
                "{src}"
            );
            assert_eq!(stats.sites, 1, "{src}");
        }
    }

    #[test]
    fn projection_values_home_comments_and_tests_stay_green() {
        let green = "fn a(l: KindLabels) {\n    counter!(\"m\", MEDIA_KIND_LABEL => l.media_kind.label(), CONTENT_KIND_LABEL => l.content_kind.label()).increment(1);\n    // counter!(\"m\", MEDIA_KIND_LABEL => \"audio\");\n}\n#[cfg(test)]\nmod tests {\n    fn t() { counter!(\"m\", MEDIA_KIND_LABEL => \"audio\").increment(1); }\n}\n";
        let (f, stats) = kind_findings(green, MC);
        assert!(f.is_empty(), "{f:?}");
        assert_eq!(
            stats.sites, 2,
            "both projection-valued keys are seen; test code is not"
        );
        // The home may spell values.
        let (f, _) = kind_findings(RED[0].0, RUST_PROJECTION_HOME);
        assert!(f.is_empty(), "{f:?}");
    }

    #[test]
    fn any_kind_key_on_mh_is_kind_label_on_mh() {
        let mh = "crates/mh-service/src/observability/metrics.rs";
        let src = "fn a(l: KindLabels) {\n    counter!(\"mh_media_x\", MEDIA_KIND_LABEL => l.media_kind.label()).increment(1);\n}\n";
        let (f, _) = kind_findings(src, mh);
        assert_eq!(f, vec![(KIND_LABEL_ON_MH_RULE_ID, mh.to_string(), 2)]);
    }

    /// The neutered-matcher proof: with an EMPTY key set every red fixture goes
    /// green and nothing is seen, so the reds above come from the matcher.
    #[test]
    fn red_kind_fixtures_go_green_with_the_key_set_neutered() {
        let neutered = KindKeys::with_keys(KIND_LABELS_RS, &[]).unwrap();
        for (src, _) in RED {
            let (f, stats) = kind_findings_with(src, MC, &neutered);
            assert!(f.is_empty(), "{src}: {f:?}");
            assert_eq!(stats.sites, 0);
        }
    }

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    /// Positive control on the REAL tree: clean of kind findings, the home
    /// resolved, and the matcher demonstrably saw MC's kind-labelled sites.
    #[test]
    fn the_real_tree_is_kind_clean_and_non_vacuous() {
        let (findings, files, stats) = collect(&repo_root()).unwrap();
        let kind: Vec<_> = findings
            .iter()
            .filter(|f| f.rule_id.starts_with("kind_label"))
            .collect();
        assert!(kind.is_empty(), "{kind:?}");
        assert!(files > 0);
        assert!(
            stats.sites >= 10,
            "MC's per-kind recorders + zero-init: saw {}",
            stats.sites
        );
    }

    /// A literal planted into a copy of REAL `metrics.rs` must hit.
    #[test]
    fn a_literal_planted_in_real_mc_metrics_hits() {
        let real = std::fs::read_to_string(repo_root().join(MC)).unwrap();
        let planted = real.replacen(
            "MEDIA_KIND_LABEL => labels.media_kind.label(),",
            "MEDIA_KIND_LABEL => \"audio\",",
            1,
        );
        assert_ne!(
            planted, real,
            "the plant site moved; re-anchor this control"
        );
        let (f, _) = kind_findings(&planted, MC);
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].0, KIND_LABEL_LITERAL_RULE_ID);
    }

    fn write(root: &Path, rel: &str, body: &str) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }

    const HOME_OK: &str = "pub struct KindLabels;\nimpl KindLabels {\n    pub const fn from_proto(k: K) -> Option<Self> { None }\n}\nimpl A {\n    pub const fn label(self) -> &'static str {\n        match self {\n            Self::A => \"audio\",\n            Self::V => \"video\",\n        }\n    }\n}\nimpl C {\n    pub const fn label(self) -> &'static str {\n        match self {\n            Self::M => \"main\",\n            Self::S => \"screen\",\n        }\n    }\n}\n";

    #[test]
    fn missing_labels_or_home_is_vacuous() {
        let dir = tempfile::tempdir().unwrap();
        let mut f = Vec::new();
        assert!(load_kind_keys(dir.path(), &mut f).is_none());
        let rules: Vec<_> = f.iter().map(|x| (x.rule_id, x.file.as_str())).collect();
        assert_eq!(
            rules,
            vec![
                (KIND_LABEL_VACUOUS_RULE_ID, LABELS_RS),
                (KIND_LABEL_VACUOUS_RULE_ID, RUST_PROJECTION_HOME)
            ]
        );
    }

    /// A home that exists but lost the projection (value arms removed, or the
    /// constructs gone) is vacuous, not a pass.
    #[test]
    fn a_present_home_without_the_projection_is_vacuous() {
        assert!(rust_home_holds_the_projection(HOME_OK));
        let real = std::fs::read_to_string(repo_root().join(RUST_PROJECTION_HOME)).unwrap();
        assert!(
            rust_home_holds_the_projection(&real),
            "the real home holds it"
        );
        for broken in [
            HOME_OK.replace("            Self::V => \"video\",\n", ""),
            HOME_OK.replace("pub struct KindLabels", "pub struct Other"),
            HOME_OK.replace(
                "impl C {\n    pub const fn label",
                "impl C {\n    pub const fn name",
            ),
            "fn x() -> &'static str { match 1 { _ => \"y\" } }\n".to_string(),
        ] {
            assert!(!rust_home_holds_the_projection(&broken), "{broken}");
            let dir = tempfile::tempdir().unwrap();
            write(dir.path(), LABELS_RS, KIND_LABELS_RS);
            write(dir.path(), RUST_PROJECTION_HOME, &broken);
            let mut f = Vec::new();
            assert!(load_kind_keys(dir.path(), &mut f).is_some());
            assert_eq!(
                f.iter()
                    .map(|x| (x.rule_id, x.file.as_str()))
                    .collect::<Vec<_>>(),
                vec![(KIND_LABEL_VACUOUS_RULE_ID, RUST_PROJECTION_HOME)]
            );
        }
    }

    /// Keys and home present, but no kind-keyed site anywhere: vacuous.
    #[test]
    fn zero_kind_sites_is_vacuous() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), LABELS_RS, KIND_LABELS_RS);
        write(dir.path(), RUST_PROJECTION_HOME, HOME_OK);
        write(
            dir.path(),
            "crates/gc-service/src/observability/metrics.rs",
            "pub fn f() { counter!(\"gc_x_total\", \"region\" => region).increment(1); }\n",
        );
        let (f, files, stats) = collect(dir.path()).unwrap();
        assert_eq!(files, 1);
        assert_eq!(stats.sites, 0);
        assert_eq!(
            f.iter()
                .map(|x| (x.rule_id, x.file.as_str()))
                .collect::<Vec<_>>(),
            vec![(KIND_LABEL_VACUOUS_RULE_ID, "crates")]
        );
    }
}
