//! `proto-no-any`: no proto may use `google.protobuf.Any` -- the premise of
//! `proto/buf.yaml`'s `MESSAGE_NO_DELETE` exception (docs/protocol/CONVENTIONS.md
//! §Enforcement, human decision 2026-10-23). Ported 2026-10-26 from
//! `scripts/guards/simple/validate-proto-no-any.sh` (ADR-0034), policy unchanged.
//!
//! Rule content: @protocol. Machinery: @infrastructure.
//!
//! # Why
//!
//! A message name is on neither the binary nor the proto3 JSON wire, which is
//! why `MESSAGE_NO_DELETE` is excepted. `Any` breaks that premise: it carries
//! the message's fully-qualified name as a type URL, so deleting or renaming a
//! message packed into an `Any` is a real wire break `buf breaking` no longer
//! reports.
//!
//! # Rules, over every `proto/**/*.proto`
//!
//! Scanned over `common::proto_lex` tokens -- statements AND scope headers, so
//! an `rpc X(google.protobuf.Any) returns (Y) {` signature is covered as the
//! bash `{`-split covered it.
//!
//! - `proto-any-import`: an `import` (plain, `public` or `weak`, any spacing,
//!   incl. `import"…"`) of `google/protobuf/any.proto`.
//! - `proto-any-type`: a reference to `google.protobuf.Any` on identifier
//!   boundaries, with or without a leading dot (`AnyThing` stays green).
//! - `proto-any-scan-vacuous`: no proto files, an empty file, or a lexer
//!   refusal (`/*`, a `'` outside strings, unbalanced braces). A vacuous file
//!   does not stop the others being scanned; the run still fails.
//!
//! BOUND, stated honestly: a hand-rolled type-URL carrier (`string type_url` +
//! `bytes value`) is not detectable here; CONVENTIONS records that as part of
//! the same revisit condition.
//!
//! NO SUPPRESSION PATH: this module does not consult `crate::ignore` or any
//! marker. A legitimate need for `Any` is a policy change with @protocol:
//! remove `MESSAGE_NO_DELETE` from `breaking.except` and this guard together.

use crate::common::ident::names_identifier;
use crate::common::proto_files::proto_files;
use crate::common::proto_lex::{lex, scopes, starts_with_keyword};
use crate::common::status::{emit_ok, emit_scope};
use anyhow::{bail, Result};
use std::path::Path;

/// What the guard bans. A parameter (not inlined) so the neutered-matcher
/// proof can pass a target nothing matches.
#[derive(Debug, Clone, Copy)]
pub struct Banned<'a> {
    /// The import path, unquoted.
    pub import: &'a str,
    /// The fully-qualified type name, no leading dot.
    pub type_name: &'a str,
}

/// The real target (@protocol policy).
pub const ANY: Banned<'static> = Banned {
    import: "google/protobuf/any.proto",
    type_name: "google.protobuf.Any",
};

/// A finding's rule. Exhaustive `id()`, no wildcard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    /// An import of `any.proto`.
    Import,
    /// A reference to `google.protobuf.Any`.
    Type,
    /// The guard could not evaluate its input.
    Vacuous,
}

impl Rule {
    /// The stable rule id (the `VIOLATION [...]` / `REASON=` token).
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Import => "proto-any-import",
            Self::Type => "proto-any-type",
            Self::Vacuous => "proto-any-scan-vacuous",
        }
    }
}

/// One finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// The rule.
    pub rule: Rule,
    /// Root-relative file (`proto` for a tree-level finding).
    pub file: String,
    /// 1-based line (0 = file- or tree-level).
    pub line: usize,
    /// The offending statement/header, or the vacuity detail.
    pub subject: String,
}

/// What one evaluation saw.
#[derive(Debug, Default)]
pub struct Report {
    /// Every finding.
    pub findings: Vec<Finding>,
    /// Files evaluated.
    pub files: usize,
    /// Statements scanned.
    pub statements: usize,
    /// Scope headers scanned.
    pub headers: usize,
}

/// `text` is an `import` statement whose path is `path`: `import`, a non-
/// identifier boundary, optional `public`/`weak`, then the quoted path.
fn imports(text: &str, path: &str) -> bool {
    let Some(rest) = starts_with_keyword(text, "import") else {
        return false;
    };
    let rest = rest.trim_start();
    let rest = ["public", "weak"]
        .iter()
        .find_map(|m| rest.strip_prefix(m))
        .unwrap_or(rest)
        .trim_start();
    rest.starts_with(&format!("\"{path}\""))
}

/// Evaluate `files` (root-relative path, content) against `banned`.
#[must_use]
pub fn check_with(files: &[(String, String)], banned: Banned<'_>) -> Report {
    let mut report = Report {
        files: files.len(),
        ..Report::default()
    };
    let vacuous = |file: &str, subject: String| Finding {
        rule: Rule::Vacuous,
        file: file.to_string(),
        line: 0,
        subject,
    };
    if files.is_empty() {
        report.findings.push(vacuous(
            "proto",
            "no .proto files found under proto/".into(),
        ));
        return report;
    }
    for (file, content) in files {
        if content.trim().is_empty() {
            report
                .findings
                .push(vacuous(file, format!("file is empty: {file}")));
            continue;
        }
        let tokens = match lex(content) {
            Ok(t) => t,
            Err(e) => {
                report.findings.push(vacuous(file, e.to_string()));
                continue;
            }
        };
        for scope in scopes(&tokens) {
            if let Some(header) = &scope.header {
                report.headers += 1;
                if names_identifier(header, banned.type_name) {
                    report.findings.push(Finding {
                        rule: Rule::Type,
                        file: file.clone(),
                        line: scope.line,
                        subject: header.clone(),
                    });
                }
            }
            for (line, text) in &scope.statements {
                report.statements += 1;
                if imports(text, banned.import) {
                    report.findings.push(Finding {
                        rule: Rule::Import,
                        file: file.clone(),
                        line: *line,
                        subject: text.clone(),
                    });
                }
                if names_identifier(text, banned.type_name) {
                    report.findings.push(Finding {
                        rule: Rule::Type,
                        file: file.clone(),
                        line: *line,
                        subject: text.clone(),
                    });
                }
            }
        }
    }
    report
}

const RULE_TEXT: &str = "proto/buf.yaml excepts MESSAGE_NO_DELETE on the premise that no proto \
    uses google.protobuf.Any (docs/protocol/CONVENTIONS.md §Enforcement)";
const ROUTE_TEXT: &str = "Using Any is a policy change with @protocol: remove MESSAGE_NO_DELETE \
    from breaking.except and this guard together. Do not edit the guard to go green (it has no \
    suppression path).";

/// Run the guard over `root`.
///
/// # Errors
///
/// Any finding (the token names the first rule, vacuity first), or an
/// unreadable file.
pub fn run(root: &Path, explain: bool) -> Result<()> {
    let files = proto_files(root)?;
    let report = check_with(&files, ANY);
    emit_scope(format!(
        "{} proto files, {} statements, {} scope headers, {} findings",
        report.files,
        report.statements,
        report.headers,
        report.findings.len()
    ));
    if report.findings.is_empty() {
        emit_ok("proto-no-any-clean");
        return Ok(());
    }
    let mut ordered: Vec<&Finding> = report.findings.iter().collect();
    ordered.sort_by_key(|f| usize::from(f.rule != Rule::Vacuous));
    for f in &ordered {
        match f.rule {
            Rule::Import | Rule::Type => println!(
                "VIOLATION [{}] in {}:{}: {}\n  {RULE_TEXT}. {ROUTE_TEXT}",
                f.rule.id(),
                f.file,
                f.line,
                f.subject
            ),
            Rule::Vacuous => println!(
                "VIOLATION [{}] {}: {} — the guard could not evaluate its input, so it \
                 refuses to report clean. This is NOT a content violation: do not loosen a \
                 pattern; restore the input.",
                f.rule.id(),
                f.file,
                f.subject
            ),
        }
        if explain {
            println!(
                "EXPLAIN: {}:{}:1 policy={} matched=\"{}\" src={}:{}",
                f.file,
                f.line.max(1),
                f.rule.id(),
                f.subject,
                file!(),
                line!()
            );
        }
    }
    let first = ordered.first().map_or(Rule::Vacuous, |f| f.rule);
    bail!("{}", first.id())
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "syntax = \"proto3\";\npackage probe.v1;\n";

    fn fixture(body: &str) -> Vec<(String, String)> {
        vec![(
            "proto/probe/v1/probe.proto".to_string(),
            format!("{BASE}{body}\n"),
        )]
    }

    /// `(rule, line, subject)` of every finding.
    fn hits(body: &str) -> Vec<(Rule, usize, String)> {
        check_with(&fixture(body), ANY)
            .findings
            .into_iter()
            .map(|f| (f.rule, f.line, f.subject))
            .collect()
    }

    /// bash `real-tree-clean` (P0): no `--root` override -- the real repo,
    /// clean AND non-vacuous.
    #[test]
    fn the_real_tree_is_clean_and_non_vacuous() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let files = proto_files(&root).unwrap();
        let r = check_with(&files, ANY);
        assert!(r.findings.is_empty(), "{:?}", r.findings);
        assert!(r.files >= 2, "{}", r.files);
        assert!(r.statements > 0 && r.headers > 0);
    }

    /// bash `import-plain` / `-public` / `-weak` / `-no-space`.
    #[test]
    fn any_import_forms_hit() {
        for stmt in [
            "import \"google/protobuf/any.proto\";",
            "import public \"google/protobuf/any.proto\";",
            "import weak \"google/protobuf/any.proto\";",
            "import\"google/protobuf/any.proto\";",
        ] {
            let got = hits(stmt);
            assert_eq!(got.len(), 1, "{stmt}: {got:?}");
            assert_eq!((got[0].0, got[0].1), (Rule::Import, 3), "{stmt}");
        }
    }

    /// bash `type-field` / `-leading-dot` / `-repeated` / `-map-value` /
    /// `-split-lines`: each names the offending field statement.
    #[test]
    fn any_type_spellings_hit() {
        for (body, subject, line) in [
            (
                "message ProbeAny { google.protobuf.Any payload = 1; }",
                "google.protobuf.Any payload = 1",
                3,
            ),
            (
                "message ProbeAny { .google.protobuf.Any payload = 1; }",
                ".google.protobuf.Any payload = 1",
                3,
            ),
            (
                "message ProbeAny { repeated google.protobuf.Any payload = 1; }",
                "repeated google.protobuf.Any payload = 1",
                3,
            ),
            (
                "message ProbeAny { map<string, google.protobuf.Any> payload = 1; }",
                "map<string, google.protobuf.Any> payload = 1",
                3,
            ),
            (
                "message ProbeAny {\n  google\n    .protobuf . Any\n  payload = 1;\n}",
                "google.protobuf.Any payload = 1",
                4,
            ),
        ] {
            assert_eq!(
                hits(body),
                vec![(Rule::Type, line, subject.to_string())],
                "{body}"
            );
        }
    }

    /// NEW (protocol B1 / security B): an `Any` in an rpc SIGNATURE is a
    /// scope header, not a statement, and must still hit.
    #[test]
    fn any_in_an_rpc_header_hits() {
        let body = "service S {\n  rpc Do(google.protobuf.Any) returns (R) {\n    option deprecated = true;\n  }\n}\nmessage R {}";
        assert_eq!(
            hits(body),
            vec![(
                Rule::Type,
                4,
                "rpc Do(google.protobuf.Any) returns (R)".to_string()
            )]
        );
    }

    /// bash `comment-mention-only`, `longer-identifier`.
    #[test]
    fn comments_and_longer_identifiers_stay_green() {
        let r = check_with(
            &fixture(
                "// google.protobuf.Any and google/protobuf/any.proto are banned here\n\
                 message ProbeAny { google.protobuf.AnyThing payload = 1; xgoogle.protobuf.Any x = 2; }",
            ),
            ANY,
        );
        assert!(r.findings.is_empty(), "{:?}", r.findings);
        assert_eq!(r.statements, 4, "both fields SEEN, not skipped");
    }

    /// bash `block-comment`, `no-proto-files`, `empty-proto-file`, plus the
    /// lexer's new fail-closed refusals. A vacuous file does not stop the
    /// scan of the others.
    #[test]
    fn vacuity_cases() {
        let vacuous_about = |files: &[(String, String)], needle: &str| {
            let r = check_with(files, ANY);
            assert!(
                r.findings
                    .iter()
                    .any(|f| f.rule == Rule::Vacuous && f.subject.contains(needle)),
                "{needle}: {:?}",
                r.findings
            );
        };
        vacuous_about(&[], "no .proto files");
        vacuous_about(
            &fixture("/* import \"google/protobuf/any.proto\"; */"),
            "block comment",
        );
        vacuous_about(&[("proto/e.proto".into(), "  \n".into())], "file is empty");
        vacuous_about(
            &fixture("message M { string n = 1 [json_name = 'x//y']; }"),
            "single quote",
        );
        vacuous_about(&fixture("message M {"), "unbalanced braces");

        let mut files = vec![("proto/a.proto".to_string(), String::new())];
        files.extend(fixture("import \"google/protobuf/any.proto\";"));
        let r = check_with(&files, ANY);
        assert!(r.findings.iter().any(|f| f.rule == Rule::Vacuous));
        assert!(r.findings.iter().any(|f| f.rule == Rule::Import));
    }

    /// A `guard:ignore` marker is not honoured: there is no suppression path.
    #[test]
    fn a_guard_ignore_marker_is_not_honoured() {
        assert_eq!(
            hits("import \"google/protobuf/any.proto\"; // guard:ignore(needed for the probe)")
                .len(),
            1
        );
    }

    /// Neutered-matcher proof: with a target nothing matches, the red
    /// fixtures go green -- so the red tests above can detect a broken matcher.
    #[test]
    fn neutered_matcher_goes_green() {
        let red = fixture(
            "import \"google/protobuf/any.proto\";\n\
             message ProbeAny { google.protobuf.Any payload = 1; }",
        );
        assert_eq!(check_with(&red, ANY).findings.len(), 2);
        let never = Banned {
            import: "never/matches.proto",
            type_name: "never.Matches",
        };
        assert!(check_with(&red, never).findings.is_empty());
    }
}
