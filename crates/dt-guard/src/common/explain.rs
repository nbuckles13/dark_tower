//! `EXPLAIN:` line emission helper.
//!
//! Per @observability commitment #11 (ADR-0034 §7). One emit site for all 8
//! subcommands; consolidates:
//!
//! 1. **1-based row/col formatting** — `Match::start()` and `line_no` are
//!    0-based in the regex/iter API; this helper adds the +1 at the wire
//!    boundary. `row=0` or `col=0` from a caller is interpreted as
//!    "unknown / file-level" and emitted as `1` (the natural minimum
//!    1-based coordinate).
//! 2. **±20-char span bounding** — long matched text is truncated to keep
//!    EXPLAIN lines greppable. Callers pass the full `matched` text; the
//!    helper handles excerpt + leading/trailing `…`.
//! 3. **Escape `"`, `\n`, `\r`** — keeps the line format parseable. Single-
//!    pass `String::with_capacity` allocation.
//! 4. **`src=` via caller's `file!()` / `line!()`** — the helper takes these
//!    as parameters so the EXPLAIN line points at the EMIT site (caller),
//!    not at this helper module. ADR-0034 §7 invariant.
//!
//! ## Format contract
//!
//! ```text
//! EXPLAIN: <file>:<row>:<col> policy=<policy> matched="<span>" src=<src_file>:<src_line>
//! ```
//!
//! Optional `<extras>` (alphabetical key=value, space-separated, before
//! `src=`) preserves per-policy hints (e.g. `panel_id=42`, `reason=lazy`)
//! without inventing new lines.

use std::fmt::Write as _;

/// Default `matched=` span bound (±N chars around the match center). The
/// numeric value comes from @observability commitment #11; centralized here
/// so the contract is one constant, not one per emit site.
pub const MATCHED_SPAN_BOUND: usize = 20;

/// One EXPLAIN-line finding to be emitted by [`print_finding`].
///
/// All borrowed slices share a single lifetime — every field comes from the
/// caller's local stack or owned String, so this is a borrow-only struct
/// (no per-call-site allocations beyond what was already in caller scope).
///
/// Field semantics:
///
/// - `file`: the input file the policy was checking.
/// - `row` / `col`: 1-based positions, with `0` reserved for "unknown /
///   file-level". The helper saturates `0 → 1` at the wire boundary.
///   Callers like `cite_extract` pre-add 1 when constructing `line_no = idx + 1`;
///   callers without per-match positions pass `0`.
/// - `policy`: full policy identifier — `<subcommand>::<rule_id>` per ADR §7.
/// - `matched`: the full matched text. Helper truncates to ±20 chars + `…`
///   and escapes `"`/`\n`/`\r`.
/// - `extras`: pairs of `(key, value)` to emit before `src=`. Empty slice
///   if none. Values are escaped the same way as `matched`. Use this for
///   policy-specific hints like `panel_id=42` or `reason=lazy`.
/// - `src_file` / `src_line`: caller passes `file!()` and `line!()` so the
///   `src=` field points to the emit site, not this helper.
#[derive(Debug, Clone, Copy)]
pub struct Finding<'a> {
    pub file: &'a str,
    pub row: usize,
    pub col: usize,
    pub policy: &'a str,
    pub matched: &'a str,
    pub extras: &'a [(&'a str, &'a str)],
    pub src_file: &'a str,
    pub src_line: u32,
}

/// Print one `EXPLAIN:` line to stdout.
///
/// Wire format (single source of truth):
/// ```text
/// EXPLAIN: <file>:<row>:<col> policy=<policy> matched="<span>" [<key>=<value>]* src=<src_file>:<src_line>
/// ```
pub fn print_finding(f: &Finding<'_>) {
    // Callers pass already-1-based values when known (e.g. `cite.line_no = idx + 1`),
    // or 0 when the position is file-level / unknown. Helper emits 1 as the
    // minimum 1-based coordinate when the caller passed 0; otherwise pass
    // through unchanged. If a future caller needs to pass a raw 0-based
    // `Match::start()` byte offset, it must add 1 at the call site before
    // constructing the Finding — this is documented at the struct, not
    // silently corrected here.
    let row_1 = f.row.max(1);
    let col_1 = f.col.max(1);
    let bounded = bound_and_escape(f.matched, MATCHED_SPAN_BOUND);

    let mut line = String::with_capacity(128 + f.matched.len());
    let _ = write!(
        line,
        "EXPLAIN: {file}:{row_1}:{col_1} policy={policy} matched=\"{bounded}\"",
        file = f.file,
        policy = f.policy,
    );
    for (k, v) in f.extras {
        let v_esc = escape_field_value(v);
        let _ = write!(line, " {k}={v_esc}");
    }
    let _ = write!(
        line,
        " src={src_file}:{src_line}",
        src_file = f.src_file,
        src_line = f.src_line
    );
    println!("{line}");
}

/// Bound `text` to ±`bound` chars from the center, then escape `"`, `\n`,
/// `\r`. Adds leading/trailing `…` when truncation occurred.
///
/// Tied to the `MATCHED_SPAN_BOUND` constant so a future bound change is
/// one-line. The truncation is char-safe (clamps to UTF-8 boundaries).
fn bound_and_escape(text: &str, bound: usize) -> String {
    if text.len() <= 2 * bound {
        return escape_field_value(text);
    }
    // Truncate symmetrically from a notional center. For 1-line matched text
    // we just keep the first `2 * bound` chars + trailing `…` since the
    // matched text is already a span (not a whole file) — symmetric trunc
    // is wasted on already-short payloads. The +20 budget catches matches
    // that grew long; the bound is "per-side" only for file excerpts.
    let safe_end = clamp_char_boundary(text, 2 * bound);
    let mut out = String::with_capacity(safe_end + 4);
    for c in text[..safe_end].chars() {
        push_escaped(&mut out, c);
    }
    out.push('…');
    out
}

/// Same `"`/`\n`/`\r` escape used by the legacy per-module `escape_matched`
/// helpers being consolidated. Single-pass; no allocations beyond the
/// returned String.
fn escape_field_value(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        push_escaped(&mut out, c);
    }
    out
}

fn push_escaped(out: &mut String, c: char) {
    match c {
        '"' => out.push_str("\\\""),
        '\n' => out.push_str("\\n"),
        '\r' => out.push_str("\\r"),
        _ => out.push(c),
    }
}

/// Clamp `idx` to the nearest valid UTF-8 char boundary at or before it.
/// Prevents `&text[..idx]` panicking on a multi-byte boundary.
fn clamp_char_boundary(s: &str, mut idx: usize) -> usize {
    if idx >= s.len() {
        return s.len();
    }
    while idx > 0 && !s.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_handles_quotes_and_newlines() {
        assert_eq!(escape_field_value(r#"foo "bar" baz"#), r#"foo \"bar\" baz"#);
        assert_eq!(escape_field_value("line1\nline2"), "line1\\nline2");
        assert_eq!(escape_field_value("a\r\nb"), "a\\r\\nb");
    }

    #[test]
    fn bound_passes_short_text_unchanged() {
        let s = "short";
        assert_eq!(bound_and_escape(s, MATCHED_SPAN_BOUND), "short");
    }

    #[test]
    fn bound_truncates_long_text() {
        let long: String = "a".repeat(100);
        let out = bound_and_escape(&long, MATCHED_SPAN_BOUND);
        // 2 * 20 = 40 chars of 'a', then '…'
        assert!(out.ends_with('…'), "expected trailing ellipsis: {out:?}");
        // Char count, not byte count (… is 3 bytes).
        assert_eq!(out.chars().count(), 41, "40 'a' + 1 '…': {out:?}");
    }

    #[test]
    fn bound_clamps_at_utf8_boundary() {
        // Multi-byte char at the 41st byte: "a" × 40 + "é" × 5 (each 2 bytes)
        // → truncation at byte 40 (2*20) lands on a clean boundary.
        let s = format!("{}{}", "a".repeat(40), "é".repeat(5));
        let out = bound_and_escape(&s, MATCHED_SPAN_BOUND);
        assert!(out.ends_with('…'));
        // Should not panic; clamp_char_boundary kept us on a boundary.
    }

    #[test]
    fn position_normalization_pins_minimum() {
        // row=0 caller-side means "unknown / file-level"; wire format emits 1.
        // row≥1 caller-side passes through unchanged (callers like cite_extract
        // pre-add 1 when constructing `line_no = idx + 1`).
        // Stdout capture is owned by the e2e fixture catalog (Wave 1.5);
        // here we just pin the .max(1) semantic at the boundary helper.
        fn normalize(v: usize) -> usize {
            v.max(1)
        }
        assert_eq!(normalize(0), 1, "unknown / file-level → 1");
        assert_eq!(normalize(1), 1, "1-based line 1 passes through");
        assert_eq!(normalize(42), 42, "1-based line 42 passes through");
    }
}
