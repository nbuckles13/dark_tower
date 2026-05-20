//! 17-case Python-vs-Rust extraction parity fixture.
//!
//! Per ADR-0034 §4 + ADR-0024 §5.7 (test veto-blocking). Case names + inputs
//! + expectations supplied by @test 2026-05-19, including the `full_match`
//!   byte-equivalence column cross-flagged by @security 2026-05-19.
//!
//! ## Categories
//!
//! - **Category 1 (8)** — colon-form `BARE_LINE_CITE_RE`.
//! - **Category 2 (6)** — double-colon `SYMBOL_CITE_RE`.
//! - **Category 3 (3)** — boundary-class `,;=|` expansion (covers 4 chars).
//!
//! ## Invariants pinned
//!
//! 1. **Ordering**: `extract_cites` emits symbols first, bare-line second.
//! 2. **`full_match`**: byte-identical to Python's `sm.group(0)` via
//!    `&line[path_match.start() .. caps.get(0).end()]` — boundary char
//!    NOT included; BOL alternative degenerates correctly.
//! 3. **Extension allowlist**: `.local`, `.bar`, etc. suppress.
//!
//! ## Test discipline
//!
//! Per ADR §Implementation Notes: @test owns ongoing case-addition. Adding
//! a 18th case is a one-line append; the table-driven shape makes failure
//! messages point at the named case.

use dt_guard::cite_extract::{extract_cites, Cite};

/// (kind, path, extra, full_match)
type Expected = Vec<(&'static str, &'static str, &'static str, &'static str)>;

struct Case {
    name: &'static str,
    input: &'static str,
    expected: Expected,
}

fn cases() -> Vec<Case> {
    vec![
        // ---- Category 1: BARE_LINE_CITE_RE (8 cases) ----
        Case {
            name: "bare_line_simple",
            input: "see crates/mc-service/src/lib.rs:42",
            expected: vec![(
                "bare-line",
                "crates/mc-service/src/lib.rs",
                "42",
                "crates/mc-service/src/lib.rs:42",
            )],
        },
        Case {
            name: "bare_line_range",
            input: "see foo/bar.rs:120-126",
            expected: vec![("bare-line", "foo/bar.rs", "120-126", "foo/bar.rs:120-126")],
        },
        Case {
            name: "bare_line_word_boundary",
            input: "see foo.rs:420",
            expected: vec![("bare-line", "foo.rs", "420", "foo.rs:420")],
        },
        Case {
            name: "bare_line_basename_only",
            input: "see common.sh:33",
            expected: vec![("bare-line", "common.sh", "33", "common.sh:33")],
        },
        Case {
            name: "bare_line_unrecognized_ext",
            input: "see foo.bar:42",
            expected: vec![],
        },
        Case {
            // Load-bearing security case per ADR §4 + @security commitment #16.
            // `.local` outside EXTENSION_ALLOWLIST → no false positive on
            // hostname:port URL token.
            name: "bare_line_url_port_not_cite",
            input: "connect to gc-service.dark-tower.svc.cluster.local:5432",
            expected: vec![],
        },
        Case {
            name: "bare_line_in_backticks",
            input: "see `foo.rs:42` here",
            // Backtick is in positive boundary class; full_match strips it.
            expected: vec![("bare-line", "foo.rs", "42", "foo.rs:42")],
        },
        Case {
            name: "bare_line_after_paren",
            input: "(foo.rs:42)",
            expected: vec![("bare-line", "foo.rs", "42", "foo.rs:42")],
        },
        // ---- Category 2: SYMBOL_CITE_RE (6 cases) ----
        Case {
            name: "symbol_simple",
            input: "see crates/mc-service/src/observability/metrics.rs::record_register_meeting",
            expected: vec![(
                "symbol",
                "crates/mc-service/src/observability/metrics.rs",
                "record_register_meeting",
                "crates/mc-service/src/observability/metrics.rs::record_register_meeting",
            )],
        },
        Case {
            name: "symbol_basename_only",
            input: "see _common.sh::aggregate_worst_status",
            expected: vec![(
                "symbol",
                "_common.sh",
                "aggregate_worst_status",
                "_common.sh::aggregate_worst_status",
            )],
        },
        Case {
            name: "symbol_inline_code_paren",
            input: "`controller.rs::remove_meeting()`",
            expected: vec![(
                "symbol",
                "controller.rs",
                "remove_meeting",
                "controller.rs::remove_meeting",
            )],
        },
        Case {
            name: "symbol_unrecognized_ext",
            input: "see foo.bar::baz",
            expected: vec![],
        },
        Case {
            name: "symbol_word_boundary_on_sym",
            input: "see foo.rs::bar_baz",
            expected: vec![("symbol", "foo.rs", "bar_baz", "foo.rs::bar_baz")],
        },
        Case {
            // Per @security commitment + Rust regex structure: positive
            // boundary class does NOT include `:`, so `foo.rs:::baz` cannot
            // match — the third `:` fails the left-boundary requirement.
            name: "symbol_triple_colon_rejected",
            input: "see foo.rs:::baz",
            expected: vec![],
        },
        // ---- Category 3: boundary-class `,;=|` (3 cases) ----
        Case {
            name: "bare_line_after_comma",
            input: "foo.rs:42,bar.rs:99",
            expected: vec![
                ("bare-line", "foo.rs", "42", "foo.rs:42"),
                ("bare-line", "bar.rs", "99", "bar.rs:99"),
            ],
        },
        Case {
            name: "bare_line_after_semicolon",
            input: "foo.rs:42; next",
            expected: vec![("bare-line", "foo.rs", "42", "foo.rs:42")],
        },
        Case {
            name: "bare_line_after_eq_or_pipe",
            input: "PATH=foo.rs:42|bar.rs:99",
            expected: vec![
                ("bare-line", "foo.rs", "42", "foo.rs:42"),
                ("bare-line", "bar.rs", "99", "bar.rs:99"),
            ],
        },
    ]
}

/// Run all 17 cases under one `#[test]` so a failure points at the named case.
#[test]
fn python_vs_rust_parity() {
    let cases = cases();
    assert_eq!(cases.len(), 17, "case count must stay at 17");

    for case in cases {
        let actual = extract_cites("test.md", case.input);
        let actual_tuples: Vec<(String, String, String, String)> = actual
            .iter()
            .map(|c: &Cite| {
                (
                    c.kind.clone(),
                    c.path.clone(),
                    c.extra.clone(),
                    c.full_match.clone(),
                )
            })
            .collect();
        let expected_tuples: Vec<(String, String, String, String)> = case
            .expected
            .iter()
            .map(|(k, p, e, f)| {
                (
                    (*k).to_string(),
                    (*p).to_string(),
                    (*e).to_string(),
                    (*f).to_string(),
                )
            })
            .collect();
        assert_eq!(
            actual_tuples, expected_tuples,
            "parity case `{}` failed: input={:?}",
            case.name, case.input
        );
    }
}
