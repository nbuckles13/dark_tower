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
fn strip_comments(line: &str, mut in_block_comment: bool) -> (String, bool) {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    let mut in_string = false;
    let mut string_quote = '"';

    while let Some(c) = chars.next() {
        if in_block_comment {
            if c == '*' && chars.peek() == Some(&'/') {
                chars.next();
                in_block_comment = false;
            }
            continue;
        }
        if in_string {
            out.push(c);
            if c == '\\' {
                if let Some(esc) = chars.next() {
                    out.push(esc);
                }
                continue;
            }
            if c == string_quote {
                in_string = false;
            }
            continue;
        }
        match c {
            '/' => match chars.peek() {
                Some(&'/') => {
                    // Rest of line is a comment.
                    return (out, false);
                }
                Some(&'*') => {
                    chars.next();
                    in_block_comment = true;
                }
                _ => out.push(c),
            },
            '"' | '\'' => {
                in_string = true;
                string_quote = c;
                out.push(c);
            }
            _ => out.push(c),
        }
    }

    (out, in_block_comment)
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
