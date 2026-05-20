//! `metric-labels` subcommand — port of validate-metric-labels.sh Python
//! kernel (ADR-0031 Prereq #3, ADR-0011 cardinality budget).
//!
//! Verbatim port per @observability commitment #13:
//! * `PII_TOKENS_CATEGORY_A` non-bypassable; bare `token` included per Lead
//!   ruling 2026-04-17.
//! * `CATEGORY_A_ALLOWLIST = {token_type}` (security co-owner sign-off required).
//! * `PII_TOKENS_CATEGORY_B` + `HASHED_SUFFIXES` exemption (Cat B only).
//! * `PII_PREFIX_DENYLIST = (raw_,)` fires regardless of suffix.
//! * `LABEL_ALLOWLIST` (Cat B substring false-positive suppression).
//! * `MAX_LITERAL_VALUE_LENGTH = 64`; `UNBOUNDED_VALUE_PATTERNS`.
//! * Match order: prefix → Cat A → Cat B (per Python L531-572).
//! * Escape-hatch `# pii-safe` or `// pii-safe`: ≥10 chars, not in `LAZY_REASON_RE`.
//!   Lazy reasons emit `lazy_pii_safe_reason` AND discard the marker.
//!   Suppresses Cat B only (NOT Cat A, NOT Rule 2 cardinality, NOT Rule 3 naming).

use crate::common::path_safety::to_repo_relative;
use crate::common::scan::warn_skip;
use crate::common::status::emit_ok;
use crate::ignore::is_lazy_reason;
use crate::metric_macros::MacroKind;
use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

const CRATES_SUBDIR: &str = "crates";
const MAX_LITERAL_VALUE_LENGTH: usize = 64;

// --- PII Category A (secrets, non-bypassable) — Python L96-120 verbatim ---
const PII_TOKENS_CATEGORY_A: &[&str] = &[
    "password",
    "passwd",
    "api_key",
    "apikey",
    "secret",
    // Bare `token` per Lead ruling 2026-04-17.
    "token",
    "bearer_token",
    "access_token",
    "refresh_token",
    "session_token",
    "id_token",
    "private_key",
    "privkey",
    "signing_key",
    "jwt",
    "auth_header",
    "authorization",
];

// CATEGORY_A_ALLOWLIST per Python L128-135 — additions require security sign-off.
const CATEGORY_A_ALLOWLIST: &[&str] = &["token_type"];

// --- PII Category B (user-PII, hashed-suffix exempt) — Python L139-171 ---
const PII_TOKENS_CATEGORY_B: &[&str] = &[
    "email",
    "phone",
    "phone_number",
    "display_name",
    "user_id",
    "name",
    "username",
    "nickname",
    "handle",
    "address",
    "postal_code",
    "zip",
    "zipcode",
    "ip",
    "ip_addr",
    "ipv4",
    "ipv6",
    "device_id",
    "user_agent",
    "fingerprint",
    "ssn",
    "dob",
    "passport",
    "driver_license",
    "credit_card",
    "card_number",
    "cvv",
    "latitude",
    "longitude",
    "geolocation",
    "geoip",
];

const PII_PREFIX_DENYLIST: &[&str] = &["raw_"];

const HASHED_SUFFIXES: &[&str] = &["_hash", "_hashed", "_id_hash", "_sha256", "_digest"];

const LABEL_ALLOWLIST: &[&str] = &[
    "hostname",
    "filename",
    "pathname",
    "typename",
    "nameservice",
];

pub const LABEL_SECRET_RULE_ID: &str = "label_secret";
pub const LABEL_PII_RULE_ID: &str = "label_pii";
pub const LABEL_NAMING_RULE_ID: &str = "label_naming";
pub const LITERAL_VALUE_LENGTH_RULE_ID: &str = "literal_value_length";
pub const UNBOUNDED_VALUE_RULE_ID: &str = "unbounded_value";
pub const LAZY_PII_SAFE_REASON_RULE_ID: &str = "lazy_pii_safe_reason";
pub const METRIC_NAME_LENGTH_RULE_ID: &str = "metric_name_length";
pub const METRIC_NAME_NAMING_RULE_ID: &str = "metric_name_naming";
pub const PARSE_ERROR_RULE_ID: &str = "parse_error";

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static MACRO_OPENER_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?:\bmetrics\s*::\s*)?\b(describe_counter|describe_gauge|describe_histogram|counter|gauge|histogram)!\s*\(",
    )
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
// Macro invocation finder (balanced-paren walker, string-literal aware).
// -----------------------------------------------------------------------------

#[derive(Debug)]
struct MacroInvocation {
    kind: MacroKind,
    start_lineno: usize,
    body: Option<String>, // None on unterminated parse error
}

fn count_newlines_until(src: &str, idx: usize) -> usize {
    src[..idx.min(src.len())].matches('\n').count()
}

#[expect(
    clippy::indexing_slicing,
    reason = "balanced-paren walker — `paren_open_idx = whole.end() - 1` is a regex-match end, always within `src` bounds; subsequent indexing tracks `i < bytes.len()`"
)]
fn find_macro_invocations(src: &str) -> Vec<MacroInvocation> {
    let mut out = Vec::new();
    let bytes = src.as_bytes();
    for caps in MACRO_OPENER_RE.captures_iter(src) {
        let Some(macro_m) = caps.get(1) else { continue };
        let Some(whole) = caps.get(0) else { continue };
        let paren_open_idx = whole.end() - 1;
        let start_lineno = count_newlines_until(src, whole.start()) + 1;

        let mut depth: i32 = 1;
        let mut i = paren_open_idx + 1;
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
            if c == b'(' {
                depth += 1;
            } else if c == b')' {
                depth -= 1;
                if depth == 0 {
                    found_close = Some(i);
                    break;
                }
            }
            i += 1;
        }
        // Regex group 1 in MACRO_OPENER_RE is the macro name; if it's not a
        // known variant the parser drops the invocation (defensive — regex
        // alternation only emits known names but the boundary stays explicit).
        let Some(kind) = MacroKind::parse(macro_m.as_str()) else {
            continue;
        };
        match found_close {
            Some(end) => {
                let body = src[paren_open_idx + 1..end].to_string();
                out.push(MacroInvocation {
                    kind,
                    start_lineno,
                    body: Some(body),
                });
            }
            None => out.push(MacroInvocation {
                kind,
                start_lineno,
                body: None,
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

pub fn run(repo_root: &Path, explain: bool) -> Result<()> {
    let rs_files = find_metric_files(repo_root);
    if rs_files.is_empty() {
        emit_ok("metric-labels-no-files");
        return Ok(());
    }

    let mut findings: Vec<Finding> = Vec::new();
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
    }

    if findings.is_empty() {
        emit_ok(format!("metric-labels-clean-{}-files", rs_files.len()));
        return Ok(());
    }
    for f in &findings {
        f.print(explain);
    }
    anyhow::bail!(
        "metric-labels: {} violation(s) across {} file(s)",
        findings.len(),
        rs_files.len()
    );
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
                message: format!(
                    "could not find matching ')' for {}! invocation",
                    inv.kind.as_str()
                ),
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
                                     — the `raw_` prefix signals an unsanitized identifier; \
                                     rename or add `# pii-safe: <reason>`"
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
}
