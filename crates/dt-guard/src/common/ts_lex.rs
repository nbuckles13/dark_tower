//! Whole-file TypeScript comment stripper that PRESERVES string literals.
//!
//! # Why not the lexers that already exist
//!
//! * `common::test_code_filter::lex_line` is Rust-lexical: it knows `'a'` as a
//!   char literal and `r#"…"#`, and treats `` ` `` as ordinary text. A TS
//!   template literal spanning lines, or a `'…'` string holding more than one
//!   character, is mis-lexed by it.
//! * `ts_retained_credentials::strip_noise` drops string BODIES. The callers
//!   here need exactly the bodies (metric names and label vocabularies ARE
//!   string literals), so that stripper would erase the thing being read.
//! * `ts_metric_naming::find_meter_calls` carries its own inline TS lexer, but
//!   it is CALL-scoped (it lexes forward from a meter-call anchor), not a
//!   whole-file pass, and yields no string spans a caller could query.
//!
//! # What this does
//!
//! One pass over the whole file:
//!
//! * `'…'`, `"…"` and `` `…` `` are string delimiters. Their bodies are kept
//!   verbatim in the output and each top-level literal is recorded as a
//!   [`StrLit`] span (so a caller can ask "is this offset inside a string?"
//!   without re-lexing). Backslash escapes are honoured.
//! * Template literals may span lines, and `${ … }` interpolations are lexed as
//!   code (nested strings, templates and braces are tracked). The whole outer
//!   template is ONE span, flagged [`StrLit::interpolated`] if it had any `${`.
//! * `//` and `/* */` comments are replaced by spaces, keeping every newline, so
//!   line numbers in the output equal line numbers in the source. `//` inside a
//!   string (`'https://…'`) is string content, not a comment.
//! * A regex literal (`/…/flags`, recognised by the usual "previous significant
//!   token cannot end an expression" rule) is copied through as code and never
//!   opens a string or comment — so `/'/` or `/\/\//` cannot desynchronise the
//!   lexer for the rest of the file.
//!
//! Byte offsets in [`StrLit`] refer to [`Lexed::text`] (NOT the source): a
//! multi-byte character inside a comment becomes one space, so offsets after it
//! shift. Line numbers are preserved exactly.

/// One top-level string/template literal in [`Lexed::text`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrLit {
    /// Offset of the opening delimiter.
    pub start: usize,
    /// Offset one past the closing delimiter (or the end of an unterminated
    /// literal).
    pub end: usize,
    /// Body range, delimiters excluded.
    pub body_start: usize,
    pub body_end: usize,
    /// `'`, `"` or `` ` ``.
    pub quote: char,
    /// A template literal containing at least one `${`.
    pub interpolated: bool,
}

impl StrLit {
    /// The literal's body, delimiters excluded.
    #[must_use]
    pub fn body<'a>(&self, text: &'a str) -> &'a str {
        text.get(self.body_start..self.body_end).unwrap_or("")
    }

    /// A plain string with a statically known value: `'…'`, `"…"`, or a
    /// backtick template with no `${`.
    #[must_use]
    pub fn is_static(&self) -> bool {
        !self.interpolated
    }
}

/// The comment-stripped text plus the top-level string spans inside it.
#[derive(Debug, Clone, Default)]
pub struct Lexed {
    pub text: String,
    /// Sorted by `start`, non-overlapping.
    pub strings: Vec<StrLit>,
}

impl Lexed {
    /// The string literal whose span contains `offset`, if any.
    #[must_use]
    pub fn string_at(&self, offset: usize) -> Option<&StrLit> {
        let idx = self.strings.partition_point(|s| s.end <= offset);
        self.strings
            .get(idx)
            .filter(|s| s.start <= offset && offset < s.end)
    }

    /// The string literal that starts exactly at `offset`, if any.
    #[must_use]
    pub fn string_starting_at(&self, offset: usize) -> Option<&StrLit> {
        let idx = self.strings.partition_point(|s| s.start < offset);
        self.strings.get(idx).filter(|s| s.start == offset)
    }

    /// 1-based line number of a byte offset in [`Self::text`].
    #[must_use]
    pub fn line_of(&self, offset: usize) -> usize {
        self.text
            .get(..offset)
            .map_or(0, |pre| pre.bytes().filter(|b| *b == b'\n').count())
            + 1
    }
}

enum Frame {
    Template,
    /// Inside `${ … }`; the count is the nesting depth of plain `{` braces.
    Interp(u32),
}

/// Lex `src`. See the module docs.
#[must_use]
pub fn lex(src: &str) -> Lexed {
    let chars: Vec<char> = src.chars().collect();
    let at = |i: usize| chars.get(i).copied();
    let mut out = String::with_capacity(src.len());
    let mut strings: Vec<StrLit> = Vec::new();
    let mut stack: Vec<Frame> = Vec::new();
    // Start offset and interpolation flag of the outermost open template.
    let mut outer_tpl: Option<(usize, bool)> = None;
    let mut i = 0usize;

    while let Some(c) = at(i) {
        let in_template_body = matches!(stack.last(), Some(Frame::Template));
        if in_template_body {
            match c {
                '\\' => {
                    out.push(c);
                    if let Some(n) = at(i + 1) {
                        out.push(n);
                    }
                    i += 2;
                }
                '`' => {
                    out.push(c);
                    i += 1;
                    stack.pop();
                    if !stack.iter().any(|f| matches!(f, Frame::Template)) {
                        if let Some((start, interpolated)) = outer_tpl.take() {
                            strings.push(StrLit {
                                start,
                                end: out.len(),
                                body_start: start + 1,
                                body_end: out.len() - 1,
                                quote: '`',
                                interpolated,
                            });
                        }
                    }
                }
                '$' if at(i + 1) == Some('{') => {
                    out.push_str("${");
                    i += 2;
                    if let Some((_, interp)) = outer_tpl.as_mut() {
                        *interp = true;
                    }
                    stack.push(Frame::Interp(0));
                }
                _ => {
                    out.push(c);
                    i += 1;
                }
            }
            continue;
        }

        // ---- code (top level, or inside `${ … }`) ----
        let top_level = stack.is_empty();
        match c {
            '/' if at(i + 1) == Some('/') => {
                while let Some(cc) = at(i) {
                    if cc == '\n' {
                        break;
                    }
                    out.push(' ');
                    i += 1;
                }
            }
            '/' if at(i + 1) == Some('*') => {
                out.push_str("  ");
                i += 2;
                loop {
                    match at(i) {
                        None => break,
                        Some('*') if at(i + 1) == Some('/') => {
                            out.push_str("  ");
                            i += 2;
                            break;
                        }
                        Some('\n') => {
                            out.push('\n');
                            i += 1;
                        }
                        Some(_) => {
                            out.push(' ');
                            i += 1;
                        }
                    }
                }
            }
            '/' if regex_may_start(&out) => {
                // Regex literal: copy through verbatim as code.
                out.push(c);
                i += 1;
                let mut in_class = false;
                while let Some(cc) = at(i) {
                    if cc == '\n' {
                        break;
                    }
                    out.push(cc);
                    i += 1;
                    match cc {
                        '\\' => {
                            if let Some(n) = at(i) {
                                if n != '\n' {
                                    out.push(n);
                                    i += 1;
                                }
                            }
                        }
                        '[' => in_class = true,
                        ']' => in_class = false,
                        '/' if !in_class => break,
                        _ => {}
                    }
                }
            }
            '\'' | '"' => {
                let start = out.len();
                out.push(c);
                i += 1;
                let body_start = out.len();
                let mut body_end;
                loop {
                    body_end = out.len();
                    match at(i) {
                        None | Some('\n') => break,
                        Some('\\') => {
                            out.push('\\');
                            i += 1;
                            if let Some(n) = at(i) {
                                out.push(n);
                                i += 1;
                            }
                        }
                        Some(q) if q == c => {
                            out.push(q);
                            i += 1;
                            break;
                        }
                        Some(other) => {
                            out.push(other);
                            i += 1;
                        }
                    }
                }
                if top_level {
                    strings.push(StrLit {
                        start,
                        end: out.len(),
                        body_start,
                        body_end,
                        quote: c,
                        interpolated: false,
                    });
                }
            }
            '`' => {
                if !stack.iter().any(|f| matches!(f, Frame::Template)) {
                    outer_tpl = Some((out.len(), false));
                }
                out.push(c);
                i += 1;
                stack.push(Frame::Template);
            }
            '{' => {
                if let Some(Frame::Interp(depth)) = stack.last_mut() {
                    *depth += 1;
                }
                out.push(c);
                i += 1;
            }
            '}' => {
                match stack.last_mut() {
                    Some(Frame::Interp(0)) => {
                        stack.pop();
                    }
                    Some(Frame::Interp(depth)) => *depth -= 1,
                    _ => {}
                }
                out.push(c);
                i += 1;
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }

    // An unterminated template at EOF still gets a span, so a caller asking
    // "is this inside a string?" is not told "no".
    if let Some((start, interpolated)) = outer_tpl {
        strings.push(StrLit {
            start,
            end: out.len(),
            body_start: start + 1,
            body_end: out.len(),
            quote: '`',
            interpolated,
        });
    }
    strings.sort_by_key(|s| s.start);
    Lexed { text: out, strings }
}

/// Whether a `/` at the end of `out` starts a regex literal rather than a
/// division: true unless the previous significant token can END an expression
/// (an identifier/number, `)` or `]`), with the usual keyword exceptions.
fn regex_may_start(out: &str) -> bool {
    let trimmed = out.trim_end();
    let Some(prev) = trimmed.chars().last() else {
        return true;
    };
    if prev == ')' || prev == ']' || prev == '}' {
        return false;
    }
    if prev.is_alphanumeric() || prev == '_' || prev == '$' || prev == '#' {
        let word: String = trimmed
            .chars()
            .rev()
            .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$')
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        return matches!(
            word.as_str(),
            "return"
                | "typeof"
                | "instanceof"
                | "in"
                | "of"
                | "new"
                | "delete"
                | "void"
                | "throw"
                | "case"
                | "do"
                | "else"
                | "yield"
                | "await"
        );
    }
    // Closing quotes end an expression too (`'a' / 2` is nonsense in practice,
    // but a string is a complete operand).
    !matches!(prev, '\'' | '"' | '`')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bodies(l: &Lexed) -> Vec<String> {
        l.strings
            .iter()
            .map(|s| s.body(&l.text).to_string())
            .collect()
    }

    #[test]
    fn keeps_all_three_quote_styles() {
        let l = lex("f('single', \"double\", `back`);");
        assert_eq!(bodies(&l), vec!["single", "double", "back"]);
        assert_eq!(
            l.strings.iter().map(|s| s.quote).collect::<Vec<_>>(),
            vec!['\'', '"', '`']
        );
        assert!(l.strings.iter().all(StrLit::is_static));
    }

    #[test]
    fn a_url_literal_is_not_a_comment() {
        let src = "const u = 'https://example.com/x'; // trailing 'not a string'\nnext();";
        let l = lex(src);
        assert!(l.text.contains("'https://example.com/x'"));
        assert!(!l.text.contains("trailing"));
        assert!(l.text.contains("\nnext();"));
        assert_eq!(bodies(&l), vec!["https://example.com/x"]);
    }

    #[test]
    fn multi_line_template_is_one_span_and_keeps_line_numbers() {
        let src = "const t = `line one\n// not a comment\n${a + '}'} tail`;\n/* c1\nc2 */ x;";
        let l = lex(src);
        assert_eq!(l.strings.len(), 1, "{:?}", l.strings);
        let t = &l.strings[0];
        assert!(t.interpolated);
        assert!(t.body(&l.text).contains("// not a comment"));
        assert_eq!(l.text.lines().count(), src.lines().count());
        assert!(!l.text.contains("c1"));
        assert!(l.text.ends_with(" x;"));
    }

    #[test]
    fn strips_line_and_block_comments_including_quotes_inside_them() {
        let src = "a(); // don't `open` a string\n/* it's \"here\" */ b('k');";
        let l = lex(src);
        assert_eq!(bodies(&l), vec!["k"]);
        assert!(!l.text.contains("don't"));
        assert_eq!(l.text.lines().count(), 2);
    }

    #[test]
    fn escapes_do_not_close_a_string() {
        let l = lex(r"x('it\'s', 'b');");
        assert_eq!(bodies(&l), vec![r"it\'s", "b"]);
    }

    #[test]
    fn regex_literal_with_quote_does_not_desync() {
        let l = lex("const re = /'/g; const s = 'real'; const d = a / b / c;");
        assert_eq!(bodies(&l), vec!["real"]);
    }

    #[test]
    fn string_lookup_by_offset() {
        let l = lex("f('ab', x)");
        let s = &l.strings[0];
        assert_eq!(l.string_at(s.start + 1), Some(s));
        assert_eq!(l.string_starting_at(s.start), Some(s));
        assert!(l.string_at(0).is_none());
        assert_eq!(l.line_of(0), 1);
    }
}
