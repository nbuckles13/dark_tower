//! Markdown table parser for cross-boundary guards.
//!
//! Single SoT for table parsing — consumed by:
//! * [`crate::cross_boundary_classification`] (Layer B) — `## Cross-Boundary
//!   Classification` table in devloop main.md.
//! * [`crate::cross_boundary_scope`] (Layer A) — same table for plan-path
//!   expansion (whole-file `docs/user-stories/*.md` exemption per
//!   §Decisions item 4; row-level tightening rolled back).
//!
//! Parses GitHub-flavored markdown pipe tables:
//! ```text
//! | Path | Classification | Owner |
//! |------|----------------|-------|
//! | `foo.rs` | Mine | — |
//! ```
//!
//! Returns one [`TableRow`] per data row. Header + separator are skipped.

/// One data row from a pipe table.
///
/// Cells are trimmed of surrounding whitespace. The first cell has backticks
/// stripped and trailing-`/`-canonicalized to `/**` (so a plan author who
/// writes `crates/foo/` gets the same set-arithmetic semantics as
/// `crates/foo/**`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableRow {
    /// Cell values, in left-to-right order. Always ≥1 element; empty cells
    /// preserved as empty strings.
    pub cells: Vec<String>,
    /// 1-based source line number where this row appears.
    pub line_no: usize,
}

/// Find the table under the given H2 heading and return its data rows.
///
/// Search starts at the line matching `## <heading>`. The next pipe-table
/// (header + separator + ≥0 data rows) under that section is parsed; rows
/// stop at the next `## ` heading or end-of-input.
///
/// Returns an empty vec if the heading is not found, the section has no
/// table, or the table has zero data rows.
pub fn parse_table_under_heading(content: &str, heading: &str) -> Vec<TableRow> {
    let heading_marker = format!("## {heading}");
    let mut in_section = false;
    let mut in_table = false;
    let mut header_seen = false;
    let mut rows: Vec<TableRow> = Vec::new();

    for (idx, raw) in content.lines().enumerate() {
        let line_no = idx + 1;
        let trimmed = raw.trim_end();

        // Enter the section on the heading.
        if trimmed.trim_start() == heading_marker {
            in_section = true;
            in_table = false;
            header_seen = false;
            continue;
        }
        if !in_section {
            continue;
        }
        // Leave the section at the next H2.
        if trimmed.starts_with("## ") {
            break;
        }
        let stripped = trimmed.trim_start();
        if !stripped.starts_with('|') {
            // Non-table line: if we're past the table, blank lines are
            // tolerated; a non-table content line ENDS the table.
            if in_table && !stripped.is_empty() {
                in_table = false;
            }
            continue;
        }
        // Separator row: |---|---|...|
        if is_separator_row(stripped) {
            in_table = true;
            header_seen = true;
            continue;
        }
        if !header_seen {
            // First pipe-line before separator = header; skip.
            continue;
        }
        if !in_table {
            continue;
        }
        // Data row.
        let cells = split_pipe_row(stripped);
        if cells.is_empty() {
            continue;
        }
        rows.push(TableRow {
            cells: canonicalize_path_cell(cells),
            line_no,
        });
    }

    rows
}

/// True if `line` is a markdown pipe-table separator row (`|---|---|...|`),
/// optionally with `:` alignment markers and surrounding whitespace.
#[expect(
    clippy::indexing_slicing,
    reason = "every `bytes[i]` is bounds-checked one line above"
)]
pub fn is_separator_row(line: &str) -> bool {
    let stripped = line.trim();
    if !stripped.starts_with('|') {
        return false;
    }
    // Split on `|` and check every non-empty cell is `:?-+:?` (with optional whitespace).
    stripped
        .split('|')
        .filter(|c| !c.trim().is_empty())
        .all(|c| {
            let s = c.trim();
            let bytes = s.as_bytes();
            if bytes.is_empty() {
                return false;
            }
            let mut i = 0;
            if bytes[i] == b':' {
                i += 1;
            }
            let mut dashes = 0;
            while i < bytes.len() && bytes[i] == b'-' {
                i += 1;
                dashes += 1;
            }
            if i < bytes.len() && bytes[i] == b':' {
                i += 1;
            }
            i == bytes.len() && dashes >= 1
        })
}

/// Split a `| a | b | c |` row into trimmed cells.
///
/// Outer `|`s contribute empty leading/trailing fields after `split('|')`;
/// we drop them. Inner empty cells are preserved as empty strings.
fn split_pipe_row(line: &str) -> Vec<String> {
    let stripped = line.trim();
    if !stripped.starts_with('|') {
        return Vec::new();
    }
    // Trim one leading + one trailing `|` (the trailing one is optional but
    // common). After trimming, split on `|` and trim each cell.
    let without_outer = stripped
        .strip_prefix('|')
        .unwrap_or(stripped)
        .strip_suffix('|')
        .unwrap_or(stripped.strip_prefix('|').unwrap_or(stripped));
    without_outer
        .split('|')
        .map(|c| c.trim().to_string())
        .collect()
}

/// Canonicalize the first cell of a row: strip backticks, drop a single
/// trailing parenthetical annotation, canonicalize trailing-`/` to `/**`.
fn canonicalize_path_cell(mut cells: Vec<String>) -> Vec<String> {
    if let Some(first) = cells.get_mut(0) {
        let raw = first.clone();
        let mut path = raw.replace('`', "");
        // Strip ONE trailing parenthetical annotation (e.g. `foo.ts (regen)`).
        if let Some(open) = path.rfind(" (") {
            if path.ends_with(')') {
                path.truncate(open);
            }
        }
        let path = path.trim();
        let canonical = if path.ends_with('/') && !path.ends_with("/**") {
            let trimmed = path.trim_end_matches('/');
            format!("{trimmed}/**")
        } else {
            path.to_string()
        };
        *first = canonical;
    }
    cells
}

/// Skip-predicate for template-placeholder rows.
///
/// Returns `true` when the first cell looks like a template scaffolding
/// placeholder that callers should ignore (`{path}`, `TBD`, or contains
/// "during planning"). Mirrors the bash
/// `common.sh::parse_cross_boundary_table` skip-list.
pub fn is_template_placeholder_row(row: &TableRow) -> bool {
    let Some(first) = row.cells.first() else {
        return true;
    };
    let s = first.trim();
    if s.is_empty() {
        return true;
    }
    if s == "{path}" || s == "TBD" {
        return true;
    }
    if s.contains("during planning") {
        return true;
    }
    // Header-row guard: if the path cell is literally "Path" the caller's
    // header-detection failed; skip defensively.
    if s == "Path" {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"# Header

## Cross-Boundary Classification

Notes prose here.

| Path | Classification | Owner |
|------|----------------|-------|
| `foo.rs` | Mine | — |
| `bar.rs` | Mechanical | code-reviewer |
| `crates/**` | Mine | — |

## Other Section

ignored
"#;

    #[test]
    fn parses_data_rows_under_heading() {
        let rows = parse_table_under_heading(SAMPLE, "Cross-Boundary Classification");
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].cells[0], "foo.rs");
        assert_eq!(rows[0].cells[1], "Mine");
        assert_eq!(rows[1].cells[1], "Mechanical");
        assert_eq!(rows[1].cells[2], "code-reviewer");
        assert_eq!(rows[2].cells[0], "crates/**");
    }

    #[test]
    fn separator_recognition() {
        assert!(is_separator_row("|---|---|---|"));
        assert!(is_separator_row("| :--- | :---: | ---: |"));
        assert!(is_separator_row(
            "|------|----------------|---------------------|"
        ));
        assert!(!is_separator_row("| Path | Classification | Owner |"));
        assert!(!is_separator_row("| foo | bar | baz |"));
    }

    #[test]
    fn template_placeholder_skip() {
        let row = TableRow {
            cells: vec!["{path}".to_string(), "TBD".to_string(), "TBD".to_string()],
            line_no: 1,
        };
        assert!(is_template_placeholder_row(&row));

        // "during planning" anywhere in the path cell is a placeholder marker
        // (bash awk regex /during planning/).
        let row = TableRow {
            cells: vec![
                "TBD during planning".to_string(),
                "TBD".to_string(),
                "TBD".to_string(),
            ],
            line_no: 1,
        };
        assert!(is_template_placeholder_row(&row));

        // Bare "TBD" matches exactly.
        let row = TableRow {
            cells: vec!["TBD".to_string(), "TBD".to_string(), "TBD".to_string()],
            line_no: 1,
        };
        assert!(is_template_placeholder_row(&row));

        let row = TableRow {
            cells: vec!["foo.rs".to_string(), "Mine".to_string(), "—".to_string()],
            line_no: 1,
        };
        assert!(!is_template_placeholder_row(&row));
    }

    #[test]
    fn trailing_slash_canonicalizes_to_starstar() {
        let md = r#"## H

| Path | C | O |
|---|---|---|
| `crates/foo/` | Mine | — |
"#;
        let rows = parse_table_under_heading(md, "H");
        assert_eq!(rows[0].cells[0], "crates/foo/**");
    }

    #[test]
    fn trailing_parenthetical_stripped() {
        let md = r#"## H

| Path | C | O |
|---|---|---|
| `foo.ts (regen)` | Mine | — |
"#;
        let rows = parse_table_under_heading(md, "H");
        assert_eq!(rows[0].cells[0], "foo.ts");
    }

    #[test]
    fn missing_heading_returns_empty() {
        let rows = parse_table_under_heading(SAMPLE, "Nonexistent");
        assert!(rows.is_empty());
    }

    #[test]
    fn heading_with_no_table() {
        let md = "## Foo\n\nprose only\n";
        let rows = parse_table_under_heading(md, "Foo");
        assert!(rows.is_empty());
    }
}
