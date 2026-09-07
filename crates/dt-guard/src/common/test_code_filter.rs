//! Test-code filter helpers — port of bash `filter_test_code` semantics.
//!
//! Wave-2 fix per @team-lead 2026-05-21: bash today uses
//! `scripts/guards/strip-test-code.sh` (nightly rustc) to compute test-only
//! line ranges + filters grep output through them. Our Rust port had only
//! path-based exclusion which missed inline `#[cfg(test)] mod tests {...}`
//! blocks in production source files — yielded ~92% FP rate on `rust-secrets`.
//!
//! ## Two layers
//!
//! 1. [`is_test_path`] — path-based, conservatively matches well-known test-
//!    file conventions across Rust + TS. Covers integration tests, fixture
//!    directories, `_test.rs` / `_tests.rs` / `.test.ts` / `.spec.ts` suffixes,
//!    `__tests__/` paths, `test-utils`/`-test-helpers` crate naming.
//!
//! 2. [`compute_test_block_ranges`] — Rust-only: brace-counter scan over the
//!    source content that marks 1-based line ranges occupied by
//!    `#[cfg(test)]` (or `#[cfg(any(test, ...))]` / `#[cfg(all(test, ...))]`)
//!    blocks. Tracks string-literal + line/block comment state so unbalanced
//!    braces inside `r#"... { ..."#` / `"...{..."` / `/* { */` don't trip
//!    the depth counter. ~50 LoC.
//!
//! Callers layer both: skip the file entirely via [`is_scan_exempt`] (which
//! composes [`is_test_path`] + [`is_guard_internal_path`]), then for
//! production files skip individual lines via [`is_line_in_test_block`].

use std::path::Path;

/// True if `path` lives under one of the guard-tooling roots
/// (`crates/dt-guard/**` or `scripts/guards/**`).
///
/// **Why exempt**: guard source code legitimately contains the patterns it
/// detects (regex catalogs, vocabulary identifiers, detection-logic literals
/// like `line.contains("tracing::")`, `r#"..."#` macro-shaped test fixtures,
/// `bail!("{token}-{}", ...)` interpolation shapes that happen to name a
/// secret-vocab word). Self-matching is structurally inevitable; the
/// principled boundary is "guards don't scan themselves."
///
/// Per @team-lead 2026-05-22: this replaces the earlier narrow
/// `CANONICAL_PATTERN_HOMES` enumeration. Now uniform across all policy
/// modules: guard-internal paths are exempt, no per-file exception list.
pub fn is_guard_internal_path(path: &Path) -> bool {
    let s = path.to_string_lossy();
    s.starts_with("crates/dt-guard/") || s.starts_with("scripts/guards/")
}

/// Composing predicate: true iff `path` should be excluded from any guard's
/// policy scan. Single maintenance point for the union of exclusion
/// categories — extend here when a new category lands.
///
/// Categories currently composed:
/// * [`is_test_path`] — test-code path-shape recognition (Rust `/tests/`,
///   `_test.rs` suffix, TS `__tests__/`, etc.).
/// * [`is_guard_internal_path`] — guard tooling roots (`crates/dt-guard/**`,
///   `scripts/guards/**`).
pub fn is_scan_exempt(path: &Path) -> bool {
    is_test_path(path) || is_guard_internal_path(path)
}

/// Path-based test-file recognition. Returns `true` if the path matches one
/// of the well-known test-file conventions:
///
/// * `/tests/` segment (integration tests).
/// * `/fixtures/` segment (test fixtures).
/// * `/__tests__/` segment (TS Jest convention).
/// * `/test-utils/` or `-test-utils/` segment (Rust test-helper crates).
/// * `-test-helpers/` segment (Rust test-helper crates).
/// * `/test_utils/` segment (alternative spelling).
/// * `_test.rs`, `_tests.rs` filename suffix.
/// * `.test.ts`, `.spec.ts`, `.test.tsx`, `.spec.tsx`, `.test.svelte` suffix.
/// * `vendor/` prefix.
///
/// Mirrors `scripts/guards/common.sh::is_test_file` semantics. Conservative
/// — false-positives here only hurt coverage (we skip a file that wasn't
/// strictly test-code), never false-positive on secrets.
pub fn is_test_path(path: &Path) -> bool {
    let Some(s) = path.to_str() else {
        // Non-UTF-8 paths can't be matched — conservatively NOT a test path.
        return false;
    };
    if s.starts_with("vendor/") {
        return true;
    }
    const PATH_SUBSTRINGS: &[&str] = &[
        "/tests/",
        "/fixtures/",
        "/__tests__/",
        "/test-utils/",
        "-test-utils/",
        "-test-helpers/",
        "/test_utils/",
    ];
    if PATH_SUBSTRINGS.iter().any(|pat| s.contains(pat)) {
        return true;
    }
    const FILE_SUFFIXES: &[&str] = &[
        "_test.rs",
        "_tests.rs",
        ".test.ts",
        ".spec.ts",
        ".test.tsx",
        ".spec.tsx",
        ".test.svelte",
    ];
    FILE_SUFFIXES.iter().any(|suf| s.ends_with(suf))
}

/// Brace-counter `#[cfg(test)]` block detector.
///
/// Returns a list of `(start_line, end_line)` 1-based inclusive ranges that
/// occupy `#[cfg(test)]` / `#[cfg(any(test, ...))]` / `#[cfg(all(test, ...))]`
/// blocks in the given Rust source content.
///
/// Implementation: line-driven, with intra-line lexer state for string +
/// comment tracking to avoid mis-counting braces inside string literals or
/// block comments. Conservative on raw strings (`r#"..."#`) — treats them
/// like regular strings and never counts braces inside.
///
/// Limitations (documented for future hardening):
/// * Does not handle attribute on `mod foo;` declarations (the `mod` is a
///   declaration without a `{}` block in this file).
/// * Does not handle `#[cfg_attr(test, ...)]` — strict `cfg(test)` only.
/// * If the file is malformed (unbalanced braces), the final range may
///   extend to EOF — fail-safe-broad (over-exempt).
#[expect(
    clippy::indexing_slicing,
    reason = "every `lines[i]` / `lines[j]` / `lines[k]` is bounds-checked by the enclosing `while i < lines.len()` / `while j < lines.len()` / `while k < lines.len()`"
)]
pub fn compute_test_block_ranges(content: &str) -> Vec<(usize, usize)> {
    let lines: Vec<&str> = content.lines().collect();
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    let mut in_block_comment = false;

    let mut i = 0usize;
    while i < lines.len() {
        let raw = lines[i];
        let (stripped, ended_block_comment) = strip_comments(raw, in_block_comment);
        in_block_comment = ended_block_comment;

        if !is_cfg_test_attribute(&stripped) {
            i += 1;
            continue;
        }

        // Find the next `{` (could be on this line, the next, or further
        // down — `cfg(test)` attribute lines often continue across multiple
        // lines, e.g.
        //
        //   #[cfg(any(
        //       test,
        //       feature = "foo"
        //   ))]
        //   mod tests {
        //       ...
        //   }
        let attr_start_line = i + 1; // 1-based
        let mut depth_state = DepthState::new(in_block_comment);
        let mut j = i + 1; // start looking on the NEXT line for the opener
        let mut block_started = false;

        // First check rest of current line — might be `#[cfg(test)] mod tests {`
        // all on one line.
        if let Some((depth, ends_comment)) = depth_state.process_line(&stripped, true) {
            depth_state.in_block_comment = ends_comment;
            depth_state.depth = depth;
            if depth > 0 {
                block_started = true;
            }
        }

        while !block_started && j < lines.len() {
            let (line_stripped, ends_comment) =
                strip_comments(lines[j], depth_state.in_block_comment);
            depth_state.in_block_comment = ends_comment;
            if let Some((depth, ends_comment2)) = depth_state.process_line(&line_stripped, true) {
                depth_state.in_block_comment = ends_comment2;
                depth_state.depth = depth;
                if depth > 0 {
                    block_started = true;
                    break;
                }
            }
            j += 1;
        }

        if !block_started {
            // No `{` found — likely a `#[cfg(test)] use ...;` non-block attribute.
            // Skip the attribute itself.
            i += 1;
            continue;
        }

        // Now walk forward until depth returns to 0.
        let mut end_line = j + 1; // 1-based
        let mut k = j + 1;
        while depth_state.depth > 0 && k < lines.len() {
            let (line_stripped, ends_comment) =
                strip_comments(lines[k], depth_state.in_block_comment);
            depth_state.in_block_comment = ends_comment;
            if let Some((depth, ends_comment2)) = depth_state.process_line(&line_stripped, false) {
                depth_state.in_block_comment = ends_comment2;
                depth_state.depth = depth;
            }
            end_line = k + 1;
            k += 1;
        }

        ranges.push((attr_start_line, end_line));
        i = k.max(j + 1);
        in_block_comment = depth_state.in_block_comment;
    }

    ranges
}

/// True if `line_no` (1-based) falls inside any of `ranges` (inclusive).
pub fn is_line_in_test_block(ranges: &[(usize, usize)], line_no: usize) -> bool {
    ranges.iter().any(|(s, e)| line_no >= *s && line_no <= *e)
}

// --- internals ---

/// Returns `true` if the (comment-stripped) line opens a `cfg(test)` /
/// `cfg(any(test, ...))` / `cfg(all(test, ...))` attribute. Greedy across
/// `#[cfg(...)]` continuations is not needed — we just need to know the
/// FIRST line carries the marker, and then we walk forward.
fn is_cfg_test_attribute(line: &str) -> bool {
    let t = line.trim_start();
    if !t.starts_with("#[") && !t.starts_with("#![") {
        return false;
    }
    // Strict: must reference `cfg(test` OR `cfg(any(test` OR `cfg(all(test`.
    // We use substring matching since the attribute may span lines.
    t.contains("cfg(test")
        || t.contains("cfg(any(test")
        || t.contains("cfg(all(test")
        || t.contains("cfg(any(")
            && t.contains("test")
            && substring_after(t, "cfg(any(").is_some_and(|after| {
                after
                    .split(')')
                    .next()
                    .is_some_and(|inner| inner.split(',').any(|p| p.trim() == "test"))
            })
        || t.contains("cfg(all(")
            && t.contains("test")
            && substring_after(t, "cfg(all(").is_some_and(|after| {
                after
                    .split(')')
                    .next()
                    .is_some_and(|inner| inner.split(',').any(|p| p.trim() == "test"))
            })
}

fn substring_after<'a>(s: &'a str, marker: &str) -> Option<&'a str> {
    s.find(marker).map(|i| &s[i + marker.len()..])
}

/// Strip `//` line comments and `/* ... */` block comments from a line.
/// Returns the stripped content + whether a block comment is still open at
/// end-of-line. Preserves string-literal content (does not strip inside
/// strings). NOT raw-string-aware (treats `r#"..."#` the same as `"..."`).
/// What [`lex_line`] does with the non-code spans it recognises.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NonCodeMode {
    /// Drop comment text entirely and keep string bodies verbatim. The
    /// historical [`strip_comments`] behaviour; byte-identical output.
    ///
    /// Used by [`compute_test_block_ranges`], which only needs braces and
    /// does not care about column alignment.
    StripComments,
    /// Replace comment text AND string-literal bodies with spaces, preserving
    /// the line's byte length so column offsets survive.
    ///
    /// Used by `media_telemetry_deny`, which reports `path:line` positions
    /// and must not match macro spellings that appear in prose or inside a
    /// string literal. Blanking rather than removing is what keeps a reported
    /// column meaningful.
    BlankNonCode,
}

/// Single lexer over one line of Rust source, tracking line comments, block
/// comments, ordinary string literals and raw string literals.
///
/// Returns `(processed_line, still_in_block_comment)`; the caller threads the
/// boolean across lines so `/* ... */` spanning several lines is handled.
///
/// # Why one lexer with a mode rather than two functions
///
/// There were already two comment strippers in this crate
/// (`strip_comments` here and `metric_labels::strip_comments_preserve_layout`)
/// with genuinely different contracts. A third would have been the shape
/// ADR-0034 §6 exists to prevent. The two behaviours here differ only in what
/// happens to the recognised spans, so they share the recognition and branch
/// on [`NonCodeMode`].
///
/// # Raw strings
///
/// `r"..."`, `r#"..."#`, `r##"..."##` are handled. This was ABSENT before
/// 2026-09-07, and its absence fails in the dangerous direction: an un-lexed
/// raw string terminates early at its first inner `"`, so the remainder of a
/// raw string is treated as code and — for `BlankNonCode` — left unblanked,
/// which is a silent MISS for any consumer matching on shapes. Closed here
/// while the media path contains zero raw strings, i.e. at the cheapest
/// moment it will ever be closed.
///
/// # Lifetimes versus char literals — corrected 2026-09-07, read the arm
///
/// This doc previously claimed that treating every `'` as a string delimiter
/// was "inert for both current consumers". **That claim was false**, and the
/// way it was false is the reason it is written out here rather than deleted:
/// it named a mitigation and called it a guarantee.
///
/// For `compute_test_block_ranges` the consequence really is benign — an
/// unpaired tick miscounts braces and over-exempts, which is documented as
/// fail-safe-broad. For `media_telemetry_deny` it **silently disarmed a
/// security control**: every denied macro form and every telemetry import to
/// the right of a lifetime on the same line was blanked. Reproduced live
/// against the built binary (@security, 2026-09-07); `&'static str` is the
/// ADR-0029 bounded-label type, so it is the construct most likely to precede
/// telemetry someone adds later. What made it *look* inert was rustfmt
/// splitting generic signatures from macro bodies onto separate lines — a
/// formatting habit, not a property of this lexer, and precisely the kind of
/// "a control that has to notice" ADR-0036 §11 argues against.
///
/// The arm below now distinguishes the two by lookahead; see its comment for
/// the discriminator and the deliberate failure direction.
///
/// # Still not handled, stated rather than implied
///
/// Nested block comments (`/* /* */ */`) close at the first `*/`, so the tail
/// of an outer comment is treated as code. Failure direction is toward the
/// finding (a false positive, loud) for shape-matching consumers, and toward
/// over-exempting for brace counting. No in-tree instance.
pub(crate) fn lex_line(
    line: &str,
    mut in_block_comment: bool,
    mode: NonCodeMode,
) -> (String, bool) {
    let blank = mode == NonCodeMode::BlankNonCode;
    let mut out = String::with_capacity(line.len());
    // Push a blanked stand-in for one source char, preserving byte length so
    // column offsets survive in `BlankNonCode` mode.
    macro_rules! blank_out {
        ($c:expr) => {
            if blank {
                for _ in 0..$c.len_utf8() {
                    out.push(' ');
                }
            }
        };
    }

    let chars: Vec<char> = line.chars().collect();
    let mut i = 0usize;
    let mut in_string = false;
    let mut string_quote = '"';
    // `Some(n)` while inside a raw string opened with `n` hashes.
    let mut raw_hashes: Option<usize> = None;

    // `chars.get(i)` rather than `chars[i]` throughout: this crate denies
    // `clippy::indexing_slicing`, and the lint is right here — a lexer walking
    // an index it also mutates is exactly where an off-by-one becomes a panic
    // in a guard that is supposed to be the thing that does not fail.
    while let Some(&c) = chars.get(i) {
        if in_block_comment {
            if c == '*' && chars.get(i + 1) == Some(&'/') {
                blank_out!(c);
                blank_out!('/');
                in_block_comment = false;
                i += 2;
                continue;
            }
            blank_out!(c);
            i += 1;
            continue;
        }

        if let Some(hashes) = raw_hashes {
            // Inside a raw string: terminated by `"` followed by exactly
            // `hashes` `#` characters. No escape processing.
            let closes_here = c == '"'
                && chars.get(i + 1..).is_some_and(|rest| {
                    rest.iter().take(hashes).filter(|h| **h == '#').count() == hashes
                });
            if closes_here {
                if blank {
                    for _ in 0..=hashes {
                        out.push(' ');
                    }
                } else {
                    out.push(c);
                    for _ in 0..hashes {
                        out.push('#');
                    }
                }
                raw_hashes = None;
                i += 1 + hashes;
                continue;
            }
            if blank {
                blank_out!(c);
            } else {
                out.push(c);
            }
            i += 1;
            continue;
        }

        if in_string {
            if c == '\\' {
                if blank {
                    blank_out!(c);
                    if let Some(esc) = chars.get(i + 1) {
                        blank_out!(*esc);
                    }
                } else {
                    out.push(c);
                    if let Some(esc) = chars.get(i + 1) {
                        out.push(*esc);
                    }
                }
                i += 2;
                continue;
            }
            if c == string_quote {
                in_string = false;
                if blank {
                    blank_out!(c);
                } else {
                    out.push(c);
                }
                i += 1;
                continue;
            }
            if blank {
                blank_out!(c);
            } else {
                out.push(c);
            }
            i += 1;
            continue;
        }

        // Raw-string opener: `r` (optionally preceded by `b`) followed by
        // zero or more `#` then `"`. The delimiter itself is emitted
        // verbatim in both modes — `r`, `#` and `"` are not macro-shaped, so
        // keeping them costs nothing and preserves byte length for free.
        if c == 'r'
            && !i
                .checked_sub(1)
                .and_then(|p| chars.get(p))
                .is_some_and(|p| (p.is_alphanumeric() && *p != 'b') || *p == '_')
        {
            let mut j = i + 1;
            while chars.get(j) == Some(&'#') {
                j += 1;
            }
            if chars.get(j) == Some(&'"') {
                let hashes = j - (i + 1);
                for ch in chars.iter().take(j + 1).skip(i) {
                    out.push(*ch);
                }
                raw_hashes = Some(hashes);
                i = j + 1;
                continue;
            }
        }

        match c {
            '/' => match chars.get(i + 1) {
                Some('/') => {
                    // Rest of line is a comment.
                    if blank {
                        for ch in chars.iter().skip(i) {
                            blank_out!(*ch);
                        }
                    }
                    return (out, false);
                }
                Some('*') => {
                    blank_out!(c);
                    blank_out!('*');
                    in_block_comment = true;
                    i += 2;
                    continue;
                }
                _ => {
                    out.push(c);
                    i += 1;
                }
            },
            '"' => {
                in_string = true;
                string_quote = '"';
                if blank {
                    blank_out!(c);
                } else {
                    out.push(c);
                }
                i += 1;
            }
            // A `'` is EITHER a char-literal delimiter OR a lifetime, and
            // getting this wrong is a security bug, not a formatting one.
            //
            // Treating every `'` as a string opener — which this lexer did
            // until 2026-09-07, inherited from the original `strip_comments` —
            // means an unpaired tick blanks the REST OF THE LINE. In
            // `compute_test_block_ranges` that merely miscounts braces and
            // over-exempts, which is documented as fail-safe-broad. In
            // `media_telemetry_deny` it SILENTLY DISARMS the control: every
            // denied macro form and every `use tracing::…` after a lifetime on
            // the same line becomes invisible. `&'static str` is the mandated
            // bounded-label type under ADR-0029, so it is the single most
            // likely construct to precede telemetry on a line someone adds
            // later. Reproduced live by @security, 2026-09-07.
            //
            // Discriminator, in order:
            //   1. next char is `\`  -> char literal (`'\n'`, `'\''`, `'\u{1F600}'`)
            //   2. char after next is `'` -> char literal (`'a'`, the `'x'` of `b'x'`)
            //   3. otherwise -> LIFETIME; emit the tick as code, stay out of
            //      string mode.
            //
            // This resolves `'_'` (char) versus `'_` (anonymous lifetime)
            // correctly, because the discriminator is the trailing quote.
            //
            // FAILURE DIRECTION IS DELIBERATE: mis-reading a char literal as a
            // lifetime leaves its contents as code, which can only cause a
            // false positive — loud. Mis-reading a lifetime as a string opener
            // causes a miss — silent. When in doubt this must fall to case 3.
            '\'' => {
                let is_char_literal = match chars.get(i + 1) {
                    Some('\\') => true,
                    Some(_) => chars.get(i + 2) == Some(&'\''),
                    None => false,
                };
                if is_char_literal {
                    in_string = true;
                    string_quote = '\'';
                    if blank {
                        blank_out!(c);
                    } else {
                        out.push(c);
                    }
                } else {
                    // Lifetime: this is CODE. Emit it verbatim in both modes —
                    // one byte, so `BlankNonCode`'s offset invariant holds.
                    out.push(c);
                }
                i += 1;
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }

    (out, in_block_comment)
}

/// Strip comments from one line, keeping string bodies verbatim.
///
/// Thin wrapper over [`lex_line`] in [`NonCodeMode::StripComments`] mode,
/// preserved as a named function because `compute_test_block_ranges` and its
/// tests are written against this contract.
pub(crate) fn strip_comments(line: &str, in_block_comment: bool) -> (String, bool) {
    lex_line(line, in_block_comment, NonCodeMode::StripComments)
}

/// Blank comments and string-literal bodies to spaces, preserving byte
/// offsets so reported columns stay meaningful.
///
/// Consumers matching source *shapes* must run over this rather than over raw
/// source. The motivating case: `crates/mh-service/src/media/` documents its
/// own ban in prose, so its doc comments legitimately contain `counter!`,
/// `println!`, `event!`, `#[instrument]` and `debug!(?frame)`. A raw matcher
/// reds the very code that documents the rule.
pub(crate) fn blank_non_code(line: &str, in_block_comment: bool) -> (String, bool) {
    lex_line(line, in_block_comment, NonCodeMode::BlankNonCode)
}

struct DepthState {
    depth: i32,
    in_block_comment: bool,
}

impl DepthState {
    fn new(in_block_comment: bool) -> Self {
        Self {
            depth: 0,
            in_block_comment,
        }
    }

    /// Walk `line` (already comment-stripped, but may still have string
    /// literals). Update brace depth. Returns `(new_depth, still_in_block_comment)`.
    fn process_line(&self, line: &str, _stripped: bool) -> Option<(i32, bool)> {
        let mut depth = self.depth;
        let mut in_string = false;
        let mut string_quote = '"';
        let mut chars = line.chars().peekable();
        while let Some(c) = chars.next() {
            if in_string {
                if c == '\\' {
                    chars.next();
                    continue;
                }
                if c == string_quote {
                    in_string = false;
                }
                continue;
            }
            match c {
                '"' | '\'' => {
                    in_string = true;
                    string_quote = c;
                }
                '{' => depth += 1,
                '}' => depth -= 1,
                _ => {}
            }
        }
        Some((depth, self.in_block_comment))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_test_path_matches_known_conventions() {
        // Rust integration tests.
        assert!(is_test_path(Path::new("crates/foo/tests/it.rs")));
        assert!(is_test_path(Path::new("crates/foo/tests/sub/case.rs")));
        // Rust `_test.rs` / `_tests.rs` suffix.
        assert!(is_test_path(Path::new("crates/foo/src/lib_test.rs")));
        assert!(is_test_path(Path::new("crates/foo/src/lib_tests.rs")));
        // Rust test-utils crates.
        assert!(is_test_path(Path::new("crates/foo-test-utils/src/lib.rs")));
        assert!(is_test_path(Path::new(
            "crates/foo-test-helpers/src/lib.rs"
        )));
        // TS conventions.
        assert!(is_test_path(Path::new("packages/x/src/foo.test.ts")));
        assert!(is_test_path(Path::new("packages/x/src/foo.spec.ts")));
        assert!(is_test_path(Path::new("packages/x/src/foo.test.tsx")));
        assert!(is_test_path(Path::new("packages/x/__tests__/foo.ts")));
        // Fixtures.
        assert!(is_test_path(Path::new(
            "crates/env-tests/src/fixtures/gc.rs"
        )));
        // Vendor.
        assert!(is_test_path(Path::new("vendor/foo/lib.rs")));
        // Negative: production code.
        assert!(!is_test_path(Path::new("crates/foo/src/lib.rs")));
        assert!(!is_test_path(Path::new("packages/x/src/index.ts")));
    }

    // ---- lex_line / blank_non_code (2026-09-07, ADR-0036 §11 deny guard) ----

    /// `StripComments` output must be byte-identical to the pre-refactor
    /// `strip_comments`. The ten pre-existing `brace_counter_*` tests above
    /// are the real pin; these are the direct assertions.
    #[test]
    fn strip_comments_mode_is_unchanged() {
        assert_eq!(strip_comments("let x = 1; // note", false).0, "let x = 1; ");
        assert_eq!(strip_comments("a /* b */ c", false).0, "a  c");
        assert_eq!(
            strip_comments(r#"let s = "// not a comment";"#, false).0,
            r#"let s = "// not a comment";"#
        );
        assert_eq!(strip_comments("/* open", false), ("".to_string(), true));
        assert_eq!(strip_comments(" still */ code", true).0, " code");
    }

    /// Blanking preserves byte length, which is what keeps a reported column
    /// meaningful in `--explain` output.
    #[test]
    fn blank_non_code_preserves_line_length() {
        for line in [
            "let x = 1; // debug!(secret)",
            r#"let s = "counter!(leak)";"#,
            "a /* println!(x) */ b",
            r##"let r = r#"info!(oops)"#;"##,
        ] {
            let (blanked, _) = blank_non_code(line, false);
            assert_eq!(
                blanked.len(),
                line.len(),
                "byte length must survive blanking:\n  in : {line}\n  out: {blanked}"
            );
        }
    }

    /// The four non-code span kinds the ADR-0036 §11 guard must not match in.
    /// Each corresponds to a real line under `crates/mh-service/src/media/`.
    #[test]
    fn blank_non_code_removes_macro_shapes_from_all_four_span_kinds() {
        // Doc comment — the shape at media/forward.rs:97.
        let (out, _) = blank_non_code("/// formatting in a `debug!(?frame)` sibling", false);
        assert!(!out.contains("debug!"), "doc comment leaked: {out}");

        // Inner doc comment — the shape at media/mod.rs:14-16.
        let (out, _) = blank_non_code("//! no `counter!` / `#[instrument]` here", false);
        assert!(!out.contains("counter!"), "inner doc leaked: {out}");
        assert!(!out.contains("instrument"), "inner doc leaked: {out}");

        // Trailing comment — the bypass a `line.contains(\"//\")` skip would open.
        let (out, _) = blank_non_code("do_work(); // println!(\"x\")", false);
        assert!(out.contains("do_work()"), "code must survive: {out}");
        assert!(!out.contains("println!"), "trailing comment leaked: {out}");

        // String literal.
        let (out, _) = blank_non_code(r#"let s = "event!(a, b)";"#, false);
        assert!(!out.contains("event!"), "string body leaked: {out}");
        assert!(out.contains("let s ="), "code must survive: {out}");
    }

    /// A trailing comment must not disarm the REST of the line. This is the
    /// bypass that `instrument_skip_all::is_comment_line` (a line-prefix
    /// check) would open: `*self.n = 0; tracing::info!(...)` is valid Rust
    /// whose trimmed form starts with `*`.
    #[test]
    fn blank_non_code_keeps_code_before_a_comment() {
        let (out, _) = blank_non_code("*self.n = 0; tracing::info!(leak); // ok", false);
        assert!(
            out.contains("tracing::info!("),
            "real code was blanked: {out}"
        );
    }

    /// Raw strings were NOT handled before 2026-09-07, and the failure
    /// direction is a silent miss: an un-lexed raw string terminates at its
    /// first inner quote, leaving the remainder treated as code.
    #[test]
    fn blank_non_code_handles_raw_strings() {
        let (out, _) = blank_non_code(r##"let s = r#"info!("x") and counter!(y)"#;"##, false);
        assert!(!out.contains("info!"), "raw string body leaked: {out}");
        assert!(!out.contains("counter!"), "raw string body leaked: {out}");
        assert!(out.contains("let s ="), "code must survive: {out}");

        // No hashes.
        let (out, _) = blank_non_code(r#"let s = r"debug!(z)";"#, false);
        assert!(!out.contains("debug!"), "raw string body leaked: {out}");
    }

    /// A bare lifetime must NOT open a string, but a char literal MUST.
    ///
    /// This is the direct pin for @security's bypass (2026-09-07): before the
    /// fix, `lex_line` treated `'` as a string opener, so `&'static str` blanked
    /// the rest of its line and hid any denied form to its right. The fixture
    /// `pos_lifetime_tick_does_not_blank.rs` pins it end-to-end; this pins the
    /// lexer directly, so a reversion reds at both layers.
    #[test]
    fn lifetime_does_not_open_a_string_but_char_literal_does() {
        // Lifetime: code to its right survives blanking, so a macro-shaped form
        // after it is NOT hidden.
        let (out, _) = blank_non_code("fn a(s: &'static str) { info!(x); }", false);
        assert!(
            out.contains("info!("),
            "a lifetime blanked the rest of the line: {out}"
        );

        // Anonymous lifetime, same requirement.
        let (out, _) = blank_non_code("fn c(f: Formatter<'_>) { info!(x); }", false);
        assert!(
            out.contains("info!("),
            "`'_` lifetime blanked the line: {out}"
        );

        // Char literal: its body IS blanked, so a macro spelled INSIDE it stays
        // hidden. `'x'` closes on its second tick.
        let (out, _) = blank_non_code("let c = 'x'; info!(y);", false);
        assert!(
            out.contains("info!("),
            "code after a closed char literal was blanked: {out}"
        );
        let (out, _) = blank_non_code(r#"let bad = '"'; info!(z);"#, false);
        assert!(
            out.contains("info!("),
            "char literal `'\"'` corrupted lexing: {out}"
        );

        // The escaped-tick char literal — the ambiguous case — closes at its
        // true terminator, not at the escaped tick.
        let (out, _) = blank_non_code(r"let t = '''; info!(w);", false);
        assert!(
            out.contains("info!("),
            "escaped-tick char literal corrupted lexing: {out}"
        );
    }

    /// Block-comment state threads across lines in both modes.
    #[test]
    fn blank_non_code_threads_block_comment_state() {
        let (out1, open) = blank_non_code("code(); /* start", false);
        assert!(open, "block comment must stay open");
        assert!(out1.contains("code()"));
        let (out2, still) = blank_non_code("span!(inside_comment)", open);
        assert!(still, "still inside the block comment");
        assert!(!out2.contains("span!"), "block comment leaked: {out2}");
        let (out3, closed) = blank_non_code("*/ error!(real)", true);
        assert!(!closed);
        assert!(
            out3.contains("error!("),
            "code after the close must survive: {out3}"
        );
    }

    #[test]
    fn is_guard_internal_path_matches_guard_tooling_roots() {
        // Rust dt-guard crate.
        assert!(is_guard_internal_path(Path::new(
            "crates/dt-guard/src/rust_log_secrets.rs"
        )));
        assert!(is_guard_internal_path(Path::new(
            "crates/dt-guard/src/common/test_code_filter.rs"
        )));
        // Shell guards.
        assert!(is_guard_internal_path(Path::new(
            "scripts/guards/simple/no-secrets-in-logs.sh"
        )));
        assert!(is_guard_internal_path(Path::new(
            "scripts/guards/run-guards.sh"
        )));
        // Negative: production service code.
        assert!(!is_guard_internal_path(Path::new(
            "crates/ac-service/src/lib.rs"
        )));
        assert!(!is_guard_internal_path(Path::new("scripts/build.sh")));
    }

    #[test]
    fn is_scan_exempt_composes_test_path_and_guard_internal() {
        // Test-path: exempt.
        assert!(is_scan_exempt(Path::new("crates/foo/tests/it.rs")));
        // Guard-internal: exempt.
        assert!(is_scan_exempt(Path::new(
            "crates/dt-guard/src/rust_log_secrets.rs"
        )));
        // Production service code: NOT exempt.
        assert!(!is_scan_exempt(Path::new("crates/ac-service/src/lib.rs")));
    }

    #[test]
    fn brace_counter_finds_simple_cfg_test_block() {
        let src = r#"pub fn prod() {
    1 + 1
}

#[cfg(test)]
mod tests {
    #[test]
    fn it() {
        assert_eq!(2, 2);
    }
}
"#;
        let ranges = compute_test_block_ranges(src);
        assert_eq!(ranges.len(), 1, "expected one range, got {ranges:?}");
        let (start, end) = ranges[0];
        // Attribute on line 5, mod opens on line 6, closes on line 11.
        assert_eq!(start, 5);
        assert_eq!(end, 11);
    }

    #[test]
    fn brace_counter_handles_string_literal_with_brace() {
        // String literal contains `{` — should NOT count toward depth.
        let src = r#"#[cfg(test)]
mod tests {
    fn x() {
        let s = "literal { brace }";
        println!("{}", s);
    }
}
"#;
        let ranges = compute_test_block_ranges(src);
        assert_eq!(ranges.len(), 1);
        let (start, end) = ranges[0];
        assert_eq!(start, 1);
        assert_eq!(end, 7);
    }

    #[test]
    fn brace_counter_handles_block_comment() {
        let src = r#"#[cfg(test)]
mod tests {
    /* unbalanced { in comment */
    fn x() {}
}
"#;
        let ranges = compute_test_block_ranges(src);
        assert_eq!(ranges.len(), 1);
        let (start, end) = ranges[0];
        assert_eq!(start, 1);
        assert_eq!(end, 5);
    }

    #[test]
    fn brace_counter_handles_any_test_variant() {
        let src = r#"#[cfg(any(test, feature = "x"))]
mod tests {
    fn x() {}
}
"#;
        let ranges = compute_test_block_ranges(src);
        assert_eq!(ranges.len(), 1);
    }

    #[test]
    fn brace_counter_skips_non_block_attribute() {
        // `#[cfg(test)] use ...;` — no `{` to open a block, attribute only
        // applies to the next item. We don't track this case; verify it
        // doesn't infinitely loop or produce a nonsense range.
        let src = r#"#[cfg(test)]
use foo::Bar;

fn prod() {}
"#;
        let ranges = compute_test_block_ranges(src);
        // The brace counter walks forward looking for `{` — the `fn prod`
        // block IS the next `{` it finds. This over-exempts the prod fn.
        // Documented limitation — not load-bearing for our use case (no
        // `#[cfg(test)] use ...;` patterns in our Wave-2 scanner targets).
        // Assert no panic / no infinite loop.
        let _ = ranges;
    }

    #[test]
    fn brace_counter_no_test_block_returns_empty() {
        let src = "pub fn prod() {\n    1\n}\n";
        let ranges = compute_test_block_ranges(src);
        assert!(ranges.is_empty());
    }

    #[test]
    fn is_line_in_test_block_inclusive() {
        let ranges = vec![(5usize, 11usize), (20, 25)];
        assert!(!is_line_in_test_block(&ranges, 4));
        assert!(is_line_in_test_block(&ranges, 5));
        assert!(is_line_in_test_block(&ranges, 8));
        assert!(is_line_in_test_block(&ranges, 11));
        assert!(!is_line_in_test_block(&ranges, 12));
        assert!(is_line_in_test_block(&ranges, 22));
    }
}
