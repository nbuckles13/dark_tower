//! STATUS line emission per ADR-0033 §6.
//!
//! Wrappers parse the final line of subcommand stdout. Format:
//! `STATUS=<OK|FAIL> REASON=<token-no-spaces>`. `main.rs` catches every
//! `anyhow::Error` and routes it through [`reason_token`] + [`emit_fail`]
//! before exiting non-zero (per semantic-guard watch-point #2).

use std::fmt::Display;

/// Emit `STATUS=OK REASON=<reason>` on a single line to stdout.
///
/// The reason is a kebab-case token (no spaces). Convention: short noun
/// phrases describing the OK outcome (`cite-extract-ok`, `parity-ok`).
pub fn emit_ok(reason: impl Display) {
    println!("STATUS=OK REASON={reason}");
}

/// Emit `STATUS=FAIL REASON=<reason>` on a single line to stdout.
///
/// The reason is a kebab-case token (no spaces). Used both for policy
/// violations (`bare-line-cite-found`) and for guard-level errors caught
/// in `main.rs` (`dashboard-panels-yaml-parse`).
pub fn emit_fail(reason: impl Display) {
    println!("STATUS=FAIL REASON={reason}");
}

/// Slugify an [`anyhow::Error`] chain into a kebab-case `REASON=` token.
///
/// Per semantic-guard watch-point #2: a 3am reader sees the slug, opens the
/// matching source location via the printed error chain (`{e:#}`), and
/// finds the offending file without `--explain`. The slug captures the
/// error kind, not its payload — payload lives in the `.context(...)`
/// chain that prints to stderr.
pub fn reason_token(err: &anyhow::Error) -> String {
    // Walk the chain; concatenate short labels separated by `-`. Cap at
    // 60 chars so the STATUS line stays single-line greppable.
    let chain: Vec<String> = err
        .chain()
        .map(|c| sluggify(&c.to_string()))
        .filter(|s| !s.is_empty())
        .collect();

    let joined = chain.join("-");
    let truncated: String = joined.chars().take(60).collect();
    if truncated.is_empty() {
        "unknown-error".to_string()
    } else {
        truncated
    }
}

/// Convert a free-form error message to a kebab-case slug.
///
/// Lowercases, replaces non-alphanumeric runs with `-`, trims trailing
/// `-`. Pure function for testing.
fn sluggify(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_dash = true;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sluggify_basic() {
        assert_eq!(sluggify("Hello, World!"), "hello-world");
        assert_eq!(sluggify("  multi   space  "), "multi-space");
        assert_eq!(sluggify(""), "");
        assert_eq!(sluggify("!@#$"), "");
        assert_eq!(sluggify("yaml/parse: bad input"), "yaml-parse-bad-input");
    }

    #[test]
    fn reason_token_truncates_at_60_chars() {
        let long = "a".repeat(200);
        let err = anyhow::anyhow!(long);
        let token = reason_token(&err);
        assert_eq!(token.len(), 60);
    }

    #[test]
    fn reason_token_walks_chain() {
        let inner = anyhow::anyhow!("inner cause");
        let outer = inner.context("outer step");
        let token = reason_token(&outer);
        // Chain order is outer→inner.
        assert!(token.starts_with("outer-step"), "got {token}");
        assert!(token.contains("inner-cause"), "got {token}");
    }

    #[test]
    fn reason_token_handles_empty_chain() {
        let err = anyhow::anyhow!("!@#$");
        let token = reason_token(&err);
        assert_eq!(token, "unknown-error");
    }
}
