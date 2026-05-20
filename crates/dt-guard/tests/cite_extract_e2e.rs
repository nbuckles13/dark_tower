//! End-to-end fixture catalog for the cite-extract kernel.
//!
//! Per @test Amendment 2 (2026-05-19) — pins `line_no` + `is_ignored`
//! end-to-end through `extract_cites` against checked-in markdown fixtures.
//! The 4-column `cite_extract_parity.rs` table pins WHAT is extracted at the
//! regex layer; this harness pins WHERE (row) and WHETHER (ignore state)
//! through the full extraction kernel.
//!
//! ## Fixture catalog (3 positive + 6 negative = 9 fixtures)
//!
//! Per `scripts/guards/lib/doc_cite_extract.py:94-101` semantics, mirrored
//! into the canonical Rust `LAZY_REASON_RE` at `src/ignore.rs`. Flat
//! `pos_<slug>.md` / `neg_<slug>.md` layout — the slug matches the
//! `rule_id` emitted in `--explain` output per @observability commitment
//! (main.md row 44), so a future fixture-vs-EXPLAIN drift is one grep.
//!
//! - `pos_bare_line_simple.md` — baseline bare-line cite, not ignored.
//! - `pos_lazy_ignore_accepted.md` — third branch of `is_lazy_reason`:
//!   reason ≥10 chars + not in vocab → cite IS ignored.
//! - `neg_file_missing.md` — symbol cite, path doesn't exist.
//! - `neg_path_escape.md` — symbol cite, path escapes repo root.
//! - `neg_symbol_not_found.md` — symbol cite, path resolves but symbol
//!   doesn't exist in target file.
//! - `neg_lazy_ignore_vocab.md` — first branch of `is_lazy_reason`:
//!   reason in vocab denylist → cite NOT ignored, `lazy_ignore_reason` fires.
//! - `neg_lazy_ignore_short.md` — second branch of `is_lazy_reason`:
//!   reason < 10 chars → cite NOT ignored, `lazy_ignore_reason` fires.
//! - `pos_md_heading_case_insensitive.md` — md symbol cite resolves via
//!   case-insensitive heading-word match (Python `re.IGNORECASE` parity).
//! - `neg_md_body_only.md` — md symbol cite where target's symbol appears
//!   only in body prose, NOT in any heading → does not resolve.
//!
//! ## Design — direct library invocation, no `assert_cmd`
//!
//! `dt-guard cite-no-line-numbers` walks an entire `repo_root` and scans
//! only paths under `IN_SCOPE_DIRS` (`docs/runbooks`, `.claude/skills`).
//! Building a fake repo_root tree per fixture would add ~3× the LoC for no
//! additional coverage — the kernel under test is `extract_cites`, and the
//! library function takes `(file: &str, text: &str)` directly. The wire-
//! format / scope-walking layer is exercised by smoke tests in `cargo test
//! --workspace` and by the production-data run-guards.sh smoke at Gate 2.

use dt_guard::cite_extract::{extract_cites, Cite};
use std::fs;
use std::path::PathBuf;

#[derive(Debug)]
struct Expectation {
    /// Path under `tests/fixtures/cite_extract/`.
    rel_path: &'static str,
    /// Total cites `extract_cites` must return.
    expected_count: usize,
    /// Per-cite expectations, in `extract_cites` return order. Length must
    /// equal `expected_count`.
    per_cite: &'static [CiteAssertion],
}

#[derive(Debug)]
struct CiteAssertion {
    kind: &'static str,
    path: &'static str,
    extra: &'static str,
    /// 1-based line number the cite was extracted from.
    line_no: usize,
    is_ignored: bool,
}

fn catalog() -> Vec<Expectation> {
    vec![
        // ---- Positive fixtures (cite is valid / no rule_id violation) ----
        Expectation {
            rel_path: "pos_bare_line_simple.md",
            expected_count: 1,
            per_cite: &[CiteAssertion {
                kind: "bare-line",
                path: "crates/mc-service/src/lib.rs",
                extra: "42",
                line_no: 5,
                is_ignored: false,
            }],
        },
        Expectation {
            rel_path: "pos_lazy_ignore_accepted.md",
            expected_count: 1,
            per_cite: &[CiteAssertion {
                kind: "bare-line",
                path: "crates/mc-service/src/old.rs",
                extra: "99",
                line_no: 6,
                is_ignored: true, // reason ≥10 + non-vocab → third branch accepts ignore
            }],
        },
        // ---- Negative fixtures — cite extracted, downstream resolver vetoes ----
        Expectation {
            rel_path: "neg_file_missing.md",
            expected_count: 1,
            per_cite: &[CiteAssertion {
                kind: "symbol",
                path: "crates/nonexistent/src/lib.rs",
                extra: "no_such_function",
                line_no: 9,
                is_ignored: false,
            }],
        },
        Expectation {
            rel_path: "neg_path_escape.md",
            expected_count: 1,
            per_cite: &[CiteAssertion {
                kind: "symbol",
                path: "crates/../../../escape/target.rs",
                extra: "root",
                line_no: 8,
                is_ignored: false,
            }],
        },
        Expectation {
            rel_path: "neg_symbol_not_found.md",
            expected_count: 1,
            per_cite: &[CiteAssertion {
                kind: "symbol",
                path: "crates/dt-guard/src/lib.rs",
                extra: "definitely_not_a_real_symbol_xyz123",
                line_no: 8,
                is_ignored: false,
            }],
        },
        Expectation {
            rel_path: "neg_lazy_ignore_vocab.md",
            expected_count: 1,
            per_cite: &[CiteAssertion {
                kind: "bare-line",
                path: "crates/mc-service/src/foo.rs",
                extra: "11",
                line_no: 8,
                is_ignored: false, // reason "test" hits LAZY_REASON_RE vocab branch
            }],
        },
        Expectation {
            rel_path: "neg_lazy_ignore_short.md",
            expected_count: 1,
            per_cite: &[CiteAssertion {
                kind: "bare-line",
                path: "crates/mc-service/src/bar.rs",
                extra: "22",
                line_no: 6,
                is_ignored: false, // reason "short" < 10 chars hits length branch
            }],
        },
        // ---- Md branch — per @code-reviewer 2026-05-19 follow-up ----
        // Heading-case-insensitive resolution + body-only-not-heading
        // negation. Pins the `re.IGNORECASE` + heading-scope semantics
        // separately from the extraction layer.
        Expectation {
            rel_path: "pos_md_heading_case_insensitive.md",
            expected_count: 1,
            per_cite: &[CiteAssertion {
                kind: "symbol",
                path: "docs/runbooks/sample.md",
                extra: "foo",
                line_no: 7,
                is_ignored: false,
            }],
        },
        Expectation {
            rel_path: "neg_md_body_only.md",
            expected_count: 1,
            per_cite: &[CiteAssertion {
                kind: "symbol",
                path: "docs/runbooks/body_only_sample.md",
                extra: "specific_function_name",
                line_no: 8,
                is_ignored: false,
            }],
        },
    ]
}

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cite_extract")
}

/// One `#[test]` walking the full catalog so a failure points at the named
/// fixture. Counter-pattern to one-test-per-fixture: with 7 fixtures (and
/// growing as Wave 2 lands), the table-driven shape keeps the harness one
/// signature update away from adding a new row.
#[test]
fn fixture_catalog_pins_line_no_and_is_ignored() {
    let root = fixtures_root();
    let entries = catalog();
    assert_eq!(entries.len(), 9, "catalog count must stay at 9");

    for entry in entries {
        let abs = root.join(entry.rel_path);
        let text = fs::read_to_string(&abs).expect("read fixture");
        let cites: Vec<Cite> = extract_cites(entry.rel_path, &text);

        assert_eq!(
            cites.len(),
            entry.expected_count,
            "fixture `{}`: expected {} cites, got {} — cites={:?}",
            entry.rel_path,
            entry.expected_count,
            cites.len(),
            cites,
        );

        for (idx, expected) in entry.per_cite.iter().enumerate() {
            let actual = &cites[idx];
            assert_eq!(
                actual.kind, expected.kind,
                "fixture `{}` cite[{idx}].kind",
                entry.rel_path,
            );
            assert_eq!(
                actual.path, expected.path,
                "fixture `{}` cite[{idx}].path",
                entry.rel_path,
            );
            assert_eq!(
                actual.extra, expected.extra,
                "fixture `{}` cite[{idx}].extra",
                entry.rel_path,
            );
            assert_eq!(
                actual.line_no, expected.line_no,
                "fixture `{}` cite[{idx}].line_no (1-based row)",
                entry.rel_path,
            );
            assert_eq!(
                actual.is_ignored, expected.is_ignored,
                "fixture `{}` cite[{idx}].is_ignored",
                entry.rel_path,
            );
        }
    }
}
