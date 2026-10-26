//! Code spans in Markdown: fenced blocks (any info string) and inline
//! backtick spans. POLICY-FREE: it says where code is and what it holds,
//! nothing about what a guard looks for in it.
//!
//! The crate's ONE fence parser (2026-10-26). Consumers: `alert_rules`'s
//! alert-inventory reader (` ```promql ` fences per `####` entry) and its
//! docs-PromQL kind-label pass (every fence and inline span);
//! `client_metrics_export` (` ```identity-label-cases ` and
//! ` ```identity-label-policy `); `todo_tracking` (fenced regions it skips).
//!
//! # Shape it accepts
//!
//! - A fence opens on a line whose trimmed text starts with three or more
//!   backticks or tildes (indentation tolerated); the rest of that line is the
//!   info string, whose first word is the language. It closes on a line whose
//!   trimmed text is ONLY the same character, at least as many times.
//! - An UNCLOSED fence is still returned (running to end of file, which is how
//!   the inventory reader always treated it) AND recorded in
//!   [`CodeScan::unclosed`], so a caller that must not scan half a document
//!   can refuse it as vacuity.
//! - Inline spans: outside fences, a run of N backticks opens a span that the
//!   next run of exactly N backticks on the SAME line closes (a span that does
//!   not close on its line is not code). Multi-line inline spans are not
//!   recognised; the docs this serves do not use them.

/// What kind of code span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodeKind {
    /// A fenced block; `lang` is the first word of the info string ("" if none).
    Fenced { lang: String },
    /// An inline backtick span.
    Inline,
}

/// One code span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeSpan {
    /// 1-based line of the opening fence, or of the inline span.
    pub line: usize,
    /// 1-based line of the closing fence (== `line` for an inline span; the
    /// last line of the file for an unclosed fence).
    pub end_line: usize,
    pub kind: CodeKind,
    /// The code: a fence's body lines joined with `\n`, or the inline text.
    pub text: String,
}

/// Every code span of a document, in order, plus the line of an unclosed
/// fence if there was one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CodeScan {
    pub spans: Vec<CodeSpan>,
    pub unclosed: Option<usize>,
}

/// `(char, run length, info string)` if `line` opens a fence.
fn fence_open(line: &str) -> Option<(char, usize, &str)> {
    let t = line.trim_start();
    let c = t.chars().next().filter(|c| matches!(c, '`' | '~'))?;
    let run = t.chars().take_while(|x| *x == c).count();
    (run >= 3).then(|| (c, run, t.get(run..).unwrap_or("").trim()))
}

/// Whether `line` closes a fence of `c` x `run`.
fn fence_close(line: &str, c: char, run: usize) -> bool {
    let t = line.trim();
    t.chars().count() >= run && t.chars().all(|x| x == c)
}

/// The inline spans of one non-fence line.
fn inline_spans(line: &str) -> Vec<&str> {
    let bytes = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes.get(i) != Some(&b'`') {
            i += 1;
            continue;
        }
        let run = bytes
            .get(i..)
            .map_or(0, |r| r.iter().take_while(|b| **b == b'`').count());
        let body_start = i + run;
        // Find the next run of exactly `run` backticks.
        let mut j = body_start;
        let mut close = None;
        while j < bytes.len() {
            if bytes.get(j) == Some(&b'`') {
                let r = bytes
                    .get(j..)
                    .map_or(0, |x| x.iter().take_while(|b| **b == b'`').count());
                if r == run {
                    close = Some(j);
                    break;
                }
                j += r;
                continue;
            }
            j += 1;
        }
        match close {
            Some(c) => {
                out.push(line.get(body_start..c).unwrap_or(""));
                i = c + run;
            }
            None => i = body_start,
        }
    }
    out
}

/// Scan a Markdown document. See the module docs.
#[must_use]
pub fn scan(md: &str) -> CodeScan {
    let lines: Vec<&str> = md.lines().collect();
    let mut out = CodeScan::default();
    let mut i = 0;
    while let Some(line) = lines.get(i) {
        if let Some((c, run, info)) = fence_open(line) {
            let lang = info.split_whitespace().next().unwrap_or("").to_string();
            let mut body: Vec<&str> = Vec::new();
            let mut k = i + 1;
            let mut closed = false;
            while let Some(l) = lines.get(k) {
                if fence_close(l, c, run) {
                    closed = true;
                    break;
                }
                body.push(l);
                k += 1;
            }
            if !closed && out.unclosed.is_none() {
                out.unclosed = Some(i + 1);
            }
            out.spans.push(CodeSpan {
                line: i + 1,
                end_line: if closed { k + 1 } else { lines.len() },
                kind: CodeKind::Fenced { lang },
                text: body.join("\n"),
            });
            i = k + 1;
            continue;
        }
        for text in inline_spans(line) {
            out.spans.push(CodeSpan {
                line: i + 1,
                end_line: i + 1,
                kind: CodeKind::Inline,
                text: text.to_string(),
            });
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fences_of_any_language_and_inline_spans_are_found() {
        let md = "intro `a{b}` and ``c ` d``\n```promql\nsum(x)\n```\n  ~~~\nplain\n~~~\n```\nbare\n```\nend `unclosed\n";
        let s = scan(md);
        assert_eq!(s.unclosed, None);
        let got: Vec<(usize, CodeKind, &str)> = s
            .spans
            .iter()
            .map(|x| (x.line, x.kind.clone(), x.text.as_str()))
            .collect();
        assert_eq!(
            got,
            vec![
                (1, CodeKind::Inline, "a{b}"),
                (1, CodeKind::Inline, "c ` d"),
                (
                    2,
                    CodeKind::Fenced {
                        lang: "promql".into()
                    },
                    "sum(x)"
                ),
                (
                    5,
                    CodeKind::Fenced {
                        lang: String::new()
                    },
                    "plain"
                ),
                (
                    8,
                    CodeKind::Fenced {
                        lang: String::new()
                    },
                    "bare"
                ),
            ]
        );
    }

    #[test]
    fn backticks_inside_a_fence_are_not_inline_spans() {
        let s = scan("```\n`x{media_kind}`\n```\n");
        assert_eq!(s.spans.len(), 1);
        assert_eq!(s.spans[0].text, "`x{media_kind}`");
    }

    #[test]
    fn an_unclosed_fence_runs_to_eof_and_is_recorded() {
        let s = scan("a\n```promql\nx > 0\ny\n");
        assert_eq!(s.unclosed, Some(2));
        assert_eq!(s.spans.len(), 1);
        assert_eq!(s.spans[0].text, "x > 0\ny");
        assert_eq!(s.spans[0].end_line, 4);
    }
}
