//! A statement lexer for `.proto` files: comment stripping, a typed refusal on
//! block comments, and normalised statements with explicit scope tokens.
//!
//! POLICY-FREE on purpose (@dry-reviewer / @infrastructure condition 3-4): it
//! knows the proto STATEMENT grammar only -- `;` ends a statement, `{` opens a
//! scope, `}` closes one -- and nothing about what any guard looks for. The
//! per-kind field-name guard (`proto_per_kind_fields`) is its first consumer;
//! the two bash proto guards (`validate-proto-no-any.sh`,
//! `validate-internal-proto-no-key-material.sh`) carry the same `//`-strip and
//! normalisation and are tracked to port here (`docs/TODO.md` §Polyglot
//! Pipeline Follow-ups).
//!
//! # What it does
//!
//! - Strips `//` line comments through the crate's shared non-code lexer
//!   (`common::test_code_filter`), never inside a `"..."` string literal.
//! - REFUSES `/*` (parity with the bash guards' "use `//`"), a `'` outside
//!   strings and comments, and unbalanced braces: each is a typed
//!   [`LexError`] (a vacuity condition for the caller), never a finding and
//!   never silently scanned.
//! - Splits the rest into [`Token`]s: a statement (text up to `;`, whitespace
//!   collapsed, spaces around `.` removed), a scope OPEN (`{`, carrying the
//!   normalised header before it, e.g. `message Foo`), and a scope CLOSE
//!   (`}`). A statement spanning several lines is ONE token, reported at the
//!   line it starts on.

use crate::common::test_code_filter::{blank_non_code, strip_comments};

/// One lexical unit of a `.proto` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    /// A statement terminated by `;`, normalised (no trailing `;`).
    Statement {
        /// 1-based line the statement starts on.
        line: usize,
        /// The normalised statement text.
        text: String,
    },
    /// `{`, with the normalised header that precedes it (`message Foo`,
    /// `oneof bar`, `enum Baz`, `service S`, `rpc ...`, ...).
    Open {
        /// 1-based line the header starts on.
        line: usize,
        /// The normalised header.
        header: String,
    },
    /// `}`.
    Close {
        /// 1-based line of the brace.
        line: usize,
    },
}

/// Why a file could not be lexed. A VACUITY condition for the caller: the
/// guard could not evaluate its input, so it must not report clean.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LexError {
    /// A `/*` block comment outside a string literal: a line-oriented strip
    /// cannot see inside one.
    BlockComment {
        /// 1-based line of the `/*`.
        line: usize,
    },
    /// A `'` outside strings and comments: the shared lexer cannot represent
    /// a proto single-quoted string, so the file is refused, not mis-read.
    SingleQuote {
        /// 1-based line of the `'`.
        line: usize,
    },
    /// A `}` with no open scope, or a scope still open at end of file: every
    /// later statement would be mis-scoped.
    UnbalancedBraces {
        /// 1-based line of the stray `}`, or the last line for an unclosed
        /// scope.
        line: usize,
    },
}

impl std::fmt::Display for LexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BlockComment { line } => write!(
                f,
                "line {line}: block comment `/*` (a line-oriented strip cannot see inside one)"
            ),
            Self::SingleQuote { line } => write!(
                f,
                "line {line}: single quote outside a string (use double-quoted strings)"
            ),
            Self::UnbalancedBraces { line } => write!(
                f,
                "line {line}: unbalanced braces (a stray `}}` or an unclosed scope)"
            ),
        }
    }
}

impl std::error::Error for LexError {}

/// Collapse whitespace runs to one space, trim, and join `a . b` to `a.b`.
#[must_use]
pub fn normalise(raw: &str) -> String {
    let collapsed = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.replace(" .", ".").replace(". ", ".")
}

/// Lex `src` into [`Token`]s.
///
/// Built on the crate's ONE non-code lexer
/// (`common::test_code_filter::{strip_comments, blank_non_code}`, @dry-reviewer
/// F1): it supplies comment stripping and string-literal recognition, and this
/// function only splits statements on `;` / `{` / `}` that are CODE (outside
/// strings, located on the blanked twin of the comment-stripped line).
///
/// # Errors
///
/// - [`LexError::BlockComment`] on the first `/*` that opens a block comment
///   (kept as a refusal for parity with the bash proto guards' "use `//`").
/// - [`LexError::SingleQuote`] on a `'` outside strings and comments: the
///   shared lexer reads `'` with Rust char/lifetime rules, which cannot
///   represent a proto single-quoted string such as `'a//b'`, so the file is
///   refused rather than mis-read. (A one-character `'x'` lexes correctly as a
///   char literal and passes.) No in-tree proto uses single quotes.
/// - [`LexError::UnbalancedBraces`] on a `}` at depth 0 or a scope still open
///   at end of file.
pub fn lex(src: &str) -> Result<Vec<Token>, LexError> {
    let mut tokens = Vec::new();
    let mut buf = String::new();
    let mut buf_line: Option<usize> = None;
    let mut depth = 0usize;
    let mut last_line = 1usize;
    for (idx, raw_line) in src.lines().enumerate() {
        let line = idx + 1;
        last_line = line;
        // A `/*` opens a block comment iff the shared lexer is in a block
        // right after it (one inside a string or a `//` comment does not).
        for (pos, _) in raw_line.match_indices("/*") {
            if strip_comments(&raw_line[..pos + 2], false).1 {
                return Err(LexError::BlockComment { line });
            }
        }
        if blank_non_code(raw_line, false).0.contains('\'') {
            return Err(LexError::SingleQuote { line });
        }
        let (code, _) = strip_comments(raw_line, false);
        // Same byte length as `code`, with string bodies blanked: a `;`, `{`
        // or `}` here is structural, never string content.
        let (structure, _) = blank_non_code(&code, false);
        let marks = structure.as_bytes();
        let mut start = 0usize;
        for (i, b) in marks.iter().enumerate() {
            if !matches!(b, b';' | b'{' | b'}') {
                continue;
            }
            let piece = code.get(start..i).unwrap_or("");
            if !piece.trim().is_empty() {
                buf_line.get_or_insert(line);
            }
            buf.push_str(piece);
            start = i + 1;
            let text = normalise(&buf);
            let at = buf_line.unwrap_or(line);
            buf.clear();
            buf_line = None;
            match b {
                b';' => {
                    if !text.is_empty() {
                        tokens.push(Token::Statement { line: at, text });
                    }
                }
                b'{' => {
                    depth += 1;
                    tokens.push(Token::Open {
                        line: at,
                        header: text,
                    });
                }
                _ => {
                    // Text before a `}` with no `;` (never valid proto at
                    // statement level) is kept rather than dropped, so a
                    // consumer sees it.
                    if !text.is_empty() {
                        tokens.push(Token::Statement { line: at, text });
                    }
                    if depth == 0 {
                        return Err(LexError::UnbalancedBraces { line });
                    }
                    depth -= 1;
                    tokens.push(Token::Close { line });
                }
            }
        }
        let rest = code.get(start..).unwrap_or("");
        if !rest.trim().is_empty() {
            buf_line.get_or_insert(line);
        }
        buf.push_str(rest);
        buf.push(' ');
    }
    if depth != 0 {
        return Err(LexError::UnbalancedBraces { line: last_line });
    }
    let tail = normalise(&buf);
    if !tail.is_empty() {
        tokens.push(Token::Statement {
            line: buf_line.unwrap_or(1),
            text: tail,
        });
    }
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stmt(line: usize, text: &str) -> Token {
        Token::Statement {
            line,
            text: text.to_string(),
        }
    }

    #[test]
    fn statements_scopes_and_comments() {
        let src = "syntax = \"proto3\"; // trailing\n\
                   // a whole-line comment\n\
                   message Foo {\n\
                     bool a = 1;\n\
                   }\n";
        assert_eq!(
            lex(src).unwrap(),
            vec![
                stmt(1, "syntax = \"proto3\""),
                Token::Open {
                    line: 3,
                    header: "message Foo".to_string()
                },
                stmt(4, "bool a = 1"),
                Token::Close { line: 5 },
            ]
        );
    }

    #[test]
    fn a_multi_line_statement_is_one_token_at_its_first_line() {
        let src = "message M {\n  repeated\n    Foo   bar\n  = 3;\n}\n";
        let tokens = lex(src).unwrap();
        assert_eq!(tokens[1], stmt(2, "repeated Foo bar = 3"));
    }

    #[test]
    fn a_block_comment_is_a_typed_refusal_not_a_finding() {
        assert_eq!(
            lex("message M {\n  /* bool audio = 1; */\n}\n"),
            Err(LexError::BlockComment { line: 2 })
        );
    }

    #[test]
    fn comment_markers_inside_strings_are_text() {
        let tokens = lex("option go_package = \"a//b/*c\";\n").unwrap();
        assert_eq!(tokens, vec![stmt(1, "option go_package = \"a//b/*c\"")]);
    }

    /// Infra F1 (as re-ruled with DRY F1): a proto single-quoted string is
    /// REFUSED, never mis-read; a `'` inside a double-quoted string or a
    /// comment is not.
    #[test]
    fn a_single_quote_outside_strings_is_refused_and_inside_is_not() {
        assert_eq!(
            lex("message M {\n  string n = 1 [json_name = 'a//b']; bool x = 2;\n}\n"),
            Err(LexError::SingleQuote { line: 2 })
        );
        let ok = lex("message M {\n  string n = 1 [json_name = \"it's\"]; // don't\n}\n").unwrap();
        assert_eq!(ok[1], stmt(2, "string n = 1 [json_name = \"it's\"]"));
    }

    /// A `/*` inside a string or after `//` is text, not a block comment;
    /// structural characters inside strings do not split.
    #[test]
    fn block_markers_and_structure_inside_strings_are_text() {
        let tokens =
            lex("message M {\n  string a = 1 [default = \"x;{}/*\"]; // c /* d\n}\n").unwrap();
        assert_eq!(tokens[1], stmt(2, "string a = 1 [default = \"x;{}/*\"]"));
        assert_eq!(tokens.len(), 3);
    }

    /// Infra F3: unbalanced braces are a typed refusal, in both directions.
    #[test]
    fn unbalanced_braces_are_a_typed_refusal() {
        assert_eq!(
            lex("message M {\n}\n}\n"),
            Err(LexError::UnbalancedBraces { line: 3 })
        );
        assert_eq!(
            lex("message M {\n  bool a = 1;\n"),
            Err(LexError::UnbalancedBraces { line: 2 })
        );
    }

    #[test]
    fn normalise_collapses_whitespace_and_joins_dots() {
        assert_eq!(
            normalise("  google . protobuf .\tAny   x = 1 "),
            "google.protobuf.Any x = 1"
        );
    }

    #[test]
    fn nested_scopes_close_in_order() {
        let tokens = lex("message A { message B { oneof c { int32 d = 1; } } }").unwrap();
        let shape: Vec<&str> = tokens
            .iter()
            .map(|t| match t {
                Token::Open { .. } => "{",
                Token::Close { .. } => "}",
                Token::Statement { .. } => ";",
            })
            .collect();
        assert_eq!(shape, ["{", "{", "{", ";", "}", "}", "}"]);
    }
}
