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

/// Emit a single-line `SCOPE: <detail>` record describing what the guard
/// actually examined.
///
/// # Why this exists as a shared emitter
///
/// Two guards already emitted a scanned-scope line as a bare `println!` with
/// a bespoke format string and no shared home: `release_build_profile` and
/// `no_insecure_browser_flags`. The convention exists to guard against silent
/// vacuity — a guard that scanned nothing and a guard that scanned everything
/// and found nothing both print "clean". `media_telemetry_deny` is the third
/// consumer and its entire purpose is making scope non-vacuous, so a third
/// bare `println!` would have been the same duplication in the guard least
/// entitled to it.
///
/// # What this helper does and does NOT own
///
/// # WHERE THIS LINE IS AND IS NOT VISIBLE — read before relying on it
///
/// **`SCOPE:` does NOT reach an operator in the pipeline.** `run-guards.sh`
/// runs guards non-verbose and re-emits only lines matching
/// `(VIOLATION|violation|ERROR|error|WARN)` on the failure arm, and `^WARN `
/// on the pass arm. `SCOPE: ` carries none of those substrings, so in the mode
/// `scripts/layer3.sh` actually uses it is **captured and discarded** — for
/// all three consumers.
///
/// It is a **direct-invocation and `--verbose` diagnostic**: real value at
/// `dt-guard <subcommand> --root . --explain`, which the runbook §6.3.1 names
/// as the triage step, and none in Layer 3.
///
/// This is stated because the paragraph above describes what the convention is
/// *for*, and a reader could otherwise conclude "the SCOPE line would have
/// caught it" about a pipeline run. It would not have. The gap is tracked in
/// `docs/TODO.md`; widening the filter or renaming the prefix touches two
/// other guards and is deliberately not done here. (@operations F4, 2026-09-07.)
///
/// # What this helper owns
///
/// It owns the `SCOPE: ` prefix and the one-physical-line contract. It
/// deliberately does **not** impose a common field shape: the call sites
/// count genuinely different things (Dockerfiles and premise channels;
/// candidate files and enumerated settings; directories and `.rs` files), and
/// flattening them into a shared schema would be a value-changing edit
/// wearing an extraction's clothes. Each site passes its own already-formatted
/// detail and pins that string by equality in its own tests.
///
/// # Never file content
///
/// `detail` carries counts, labels and configured paths. It must never carry
/// text read out of a scanned file — a guard that echoes what it detected has
/// moved the finding into CI logs and their retention window rather than
/// closing it (@semantic-guard, ADR-0036 §11).
pub fn emit_scope(detail: impl Display) {
    println!("SCOPE: {detail}");
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
