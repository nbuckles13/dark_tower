//! A statement lexer for `.proto` files: comment stripping, a typed refusal on
//! block comments, and normalised statements with explicit scope tokens.
//!
//! POLICY-FREE on purpose (@dry-reviewer / @infrastructure condition 3-4): it
//! knows the proto STATEMENT grammar only -- `;` ends a statement, `{` opens a
//! scope, `}` closes one -- and nothing about what any guard looks for. Its
//! consumers are the three proto guards: `proto_per_kind_fields`,
//! `proto_no_any` and `internal_proto_no_key_material` (the latter two ported
//! from bash 2026-10-26, retiring the bash copies of this strip/normaliser).
//! File discovery is `common::proto_files`.
//!
//! # Deltas from the retired bash normaliser (stated so nobody "restores
//! # parity" by loosening)
//!
//! - String-aware: `//` and `/*` inside a `"..."` literal are text, neither
//!   stripped nor refused (bash's `sed 's://.*::'` stripped them).
//! - NEW refusals, both fail-closed: a `'` outside strings and comments, and
//!   unbalanced braces.
//! - Tokens carry real 1-based line numbers (bash cited statement text).
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

/// One scope of a lexed file: the file root, or a `{ ... }` declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scope {
    /// The normalised header (`message Foo`, `rpc X(A) returns (B)`, ...), or
    /// `None` for the file root.
    pub header: Option<String>,
    /// 1-based line the header starts on (1 for the root).
    pub line: usize,
    /// Declared scopes enclosing this one: 0 for the root AND for a
    /// top-level declaration (tell them apart by `header`).
    pub nesting: usize,
    /// The statements DIRECTLY in this scope (not in nested ones), as
    /// `(line, normalised text)`.
    pub statements: Vec<(usize, String)>,
}

/// Group `tokens` into [`Scope`]s: the root first, then each declaration in
/// source order. The ONE scope walker the proto guards share (@dry-reviewer
/// C): callers filter by `header` / `nesting`, never re-track depth.
#[must_use]
pub fn scopes(tokens: &[Token]) -> Vec<Scope> {
    let mut out = vec![Scope {
        header: None,
        line: 1,
        nesting: 0,
        statements: Vec::new(),
    }];
    let mut stack = vec![0usize];
    for t in tokens {
        match t {
            Token::Open { line, header } => {
                out.push(Scope {
                    header: Some(header.clone()),
                    line: *line,
                    nesting: stack.len() - 1,
                    statements: Vec::new(),
                });
                stack.push(out.len() - 1);
            }
            Token::Close { .. } => {
                // `lex` guarantees balance; never pop the root.
                if stack.len() > 1 {
                    stack.pop();
                }
            }
            Token::Statement { line, text } => {
                let at = stack.last().copied().unwrap_or(0);
                if let Some(scope) = out.get_mut(at) {
                    scope.statements.push((*line, text.clone()));
                }
            }
        }
    }
    out
}

/// The value NAMES of the TOP-LEVEL `enum <enum_name>` (a depth-0 declaration;
/// a same-named enum nested in a message is a different type and is never
/// returned), as `(line, name)` in source order: the text before `=` of each
/// statement directly in that scope. Policy-free: which values a caller keeps
/// (prefixes, the zero value) is the caller's.
#[must_use]
pub fn enum_values(scopes: &[Scope], enum_name: &str) -> Vec<(usize, String)> {
    let header = format!("enum {enum_name}");
    scopes
        .iter()
        .filter(|s| s.nesting == 0 && s.header.as_deref() == Some(header.as_str()))
        .flat_map(|s| s.statements.iter())
        .filter_map(|(line, text)| {
            let (name, _) = text.split_once('=')?;
            let name = name.trim();
            (!name.is_empty()).then(|| (*line, name.to_string()))
        })
        .collect()
}

/// If normalised statement `text` starts with keyword `kw` on an identifier
/// boundary (followed by a non-identifier byte, or the end), the rest after
/// it; else `None`. `import"x"` and `import "x"` both start with `import`;
/// `imports x` does not.
#[must_use]
pub fn starts_with_keyword<'a>(text: &'a str, kw: &str) -> Option<&'a str> {
    let rest = text.strip_prefix(kw)?;
    (!rest
        .bytes()
        .next()
        .is_some_and(crate::common::ident::is_ident_byte))
    .then_some(rest)
}

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

    #[test]
    fn scopes_group_direct_statements_with_nesting() {
        let tokens = lex("package p;\nmessage A {\n  int32 a = 1;\n  enum B {\n    X = 0;\n  }\n}\nenum C { Y = 0; }\n").unwrap();
        let s = scopes(&tokens);
        let shape: Vec<(Option<&str>, usize, usize, Vec<&str>)> = s
            .iter()
            .map(|sc| {
                (
                    sc.header.as_deref(),
                    sc.line,
                    sc.nesting,
                    sc.statements.iter().map(|(_, t)| t.as_str()).collect(),
                )
            })
            .collect();
        assert_eq!(
            shape,
            vec![
                (None, 1, 0, vec!["package p"]),
                (Some("message A"), 2, 0, vec!["int32 a = 1"]),
                (Some("enum B"), 4, 1, vec!["X = 0"]),
                (Some("enum C"), 8, 0, vec!["Y = 0"]),
            ]
        );
    }

    #[test]
    fn enum_values_reads_only_the_top_level_enum() {
        let src = "enum MediaKind {\n  MEDIA_KIND_UNSPECIFIED = 0;\n  MEDIA_KIND_AUDIO = 1;\n  option allow_alias = true;\n}\n\
                   message M {\n  enum MediaKind {\n    NESTED_ONLY = 0;\n  }\n}\n";
        let sc = scopes(&lex(src).unwrap());
        let names: Vec<String> = enum_values(&sc, "MediaKind")
            .into_iter()
            .map(|(_, n)| n)
            .collect();
        assert_eq!(
            names,
            vec![
                "MEDIA_KIND_UNSPECIFIED",
                "MEDIA_KIND_AUDIO",
                "option allow_alias"
            ]
        );
        assert_eq!(enum_values(&sc, "MediaKind")[1].0, 3);
        assert!(enum_values(&sc, "Missing").is_empty());
        // Nested-only declaration: nothing.
        let nested = scopes(&lex("message M {\n  enum MediaKind {\n    A = 0;\n  }\n}\n").unwrap());
        assert!(enum_values(&nested, "MediaKind").is_empty());
    }

    #[test]
    fn starts_with_keyword_needs_an_identifier_boundary() {
        assert_eq!(
            starts_with_keyword("import \"a.proto\"", "import"),
            Some(" \"a.proto\"")
        );
        assert_eq!(
            starts_with_keyword("import\"a.proto\"", "import"),
            Some("\"a.proto\"")
        );
        assert_eq!(starts_with_keyword("import", "import"), Some(""));
        assert_eq!(starts_with_keyword("imports x", "import"), None);
        assert_eq!(starts_with_keyword("import_x y", "import"), None);
        assert_eq!(starts_with_keyword("x import", "import"), None);
    }
}
