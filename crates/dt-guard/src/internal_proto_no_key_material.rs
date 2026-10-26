//! `internal-proto-no-key-material`: NO KEY MATERIAL CROSSES THE MC->MH CONTRACT
//! (ADR-0036 §4), mechanised for `proto/dark_tower/internal/v1/internal.proto`.
//! Ported 2026-10-26 from
//! `scripts/guards/simple/validate-internal-proto-no-key-material.sh`
//! (ADR-0034), policy content unchanged except the one tightening named below.
//!
//! Rule content (allow-list MEMBERSHIP): @security. Machinery (the construct
//! and its evaluation): @infrastructure.
//!
//! # Three checks, each a mechanical PROXY for the rule, with its bound
//!
//! Scanned over `common::proto_lex` tokens of internal.proto -- statements AND
//! scope headers, so an rpc signature (`rpc X(google.protobuf.BytesValue)
//! returns (Y) {`) is covered as the bash `{`-split covered it.
//!
//! 1. `internal-proto-bytes-field` -- a `bytes` token (`\bbytes([\s>]|$)`:
//!    bare, `repeated`, `optional`, oneof member, `map<…, bytes>`) or
//!    `BytesValue`. The trailing class keeps `total_bytes` and
//!    `bytes_sent_label` green. Every piece of key material in this tree is
//!    `bytes`.
//! 2. `internal-proto-cross-package-type` -- any dotted reference with a
//!    capitalised last segment (relative, leading-dot and `google.protobuf.*`
//!    spellings all caught), compared by EXACT equality to [`ALLOWED_TYPES`].
//!    Check 1 alone is defeated by embedding a signaling MESSAGE
//!    (`MeetingKekUpdate` carries `meeting_kek`). Fully-qualified spelling is
//!    REQUIRED: a red on a relative spelling of an allow-listed enum is
//!    intended -- fix the spelling, never loosen the comparison.
//!    `internal-proto-allowlist-not-enum` -- an [`ALLOWED_TYPES`] entry is a
//!    TOP-LEVEL `message` in signaling.proto: the allow-list is enums-only,
//!    because a message type is exactly how key material would arrive.
//! 3. `internal-proto-import-not-allowed` -- every `import` must equal an
//!    [`ALLOWED_IMPORTS`] entry in the exact plain form; `public`, `weak` and
//!    `import"…"` red. BOUND: direct imports only.
//!
//! # Vacuity (`internal-proto-scan-vacuous`)
//!
//! Refuses to report clean unless it demonstrably read the right, non-empty
//! files: both protos exist and are non-empty and lex (no `/*`, no `'` outside
//! strings, balanced braces); internal.proto carries its identity markers as
//! STRUCTURE (a top-level `package dark_tower.internal.v1` statement and a
//! top-level `service MediaHandlerService` scope); every allow-list entry is
//! declared at top level in signaling.proto.
//!
//! # Policy tightening vs bash (2026-10-26, acked by @security)
//!
//! Bash matched `enum <Name> {` at ANY depth. An [`ALLOWED_TYPES`] entry is an
//! FQN of a top-level type, so it must now be a TOP-LEVEL enum: a nested-only
//! declaration is vacuity (a stray nested enum can no longer shadow the real
//! one); a top-level enum plus a nested message of the same name is green.
//!
//! NO SUPPRESSION PATH: this module does not consult `crate::ignore`, honours
//! no marker, env var or flag. A legitimate need is a re-derivation of the
//! rule with @security, not an edit here.

use crate::common::ident::is_ident_byte;
use crate::common::proto_lex::{lex, scopes, starts_with_keyword, Scope};
use crate::common::status::{emit_ok, emit_scope};
use anyhow::{bail, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use std::path::Path;

/// The contract this guard protects.
pub const INTERNAL_PROTO: &str = "proto/dark_tower/internal/v1/internal.proto";
/// Where the allow-listed enums are declared.
pub const SIGNALING_PROTO: &str = "proto/dark_tower/signaling/v1/signaling.proto";

// ---- security-owned CONTENT under a machinery-owned construct ----------------
/// Cross-package types permitted on the internal contract. TOP-LEVEL ENUMS
/// ONLY: an enum cannot carry a `bytes` field, so it needs no per-entry
/// judgement; a MESSAGE type is exactly the hole check 2 closes. Adding an
/// entry requires @security sign-off, and the guard reds a non-enum regardless
/// (`internal-proto-allowlist-not-enum`). Canonical FQN.
pub const ALLOWED_TYPES: &[&str] = &["dark_tower.signaling.v1.TransportMode"];
/// Imports permitted, as the exact path (plain form only). @security content.
pub const ALLOWED_IMPORTS: &[&str] = &["dark_tower/signaling/v1/signaling.proto"];
// -----------------------------------------------------------------------------

/// Any dotted reference with a capitalised last segment, leading dot included
/// (verbatim from the bash guard's `grep -oE`).
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static QUALIFIED_REF_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\.?\b([A-Za-z_][A-Za-z0-9_]*\.)+[A-Z][A-Za-z0-9_]*")
        .expect("static pattern compiles")
});

/// A finding's rule. Exhaustive `id()`, no wildcard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    /// Check 1.
    BytesField,
    /// Check 2.
    CrossPackageType,
    /// Check 2's enums-only boundary.
    AllowlistNotEnum,
    /// Check 3.
    ImportNotAllowed,
    /// The guard could not evaluate its input.
    Vacuous,
}

impl Rule {
    /// The stable rule id (the `VIOLATION [...]` / `REASON=` token).
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::BytesField => "internal-proto-bytes-field",
            Self::CrossPackageType => "internal-proto-cross-package-type",
            Self::AllowlistNotEnum => "internal-proto-allowlist-not-enum",
            Self::ImportNotAllowed => "internal-proto-import-not-allowed",
            Self::Vacuous => "internal-proto-scan-vacuous",
        }
    }
}

/// One finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// The rule.
    pub rule: Rule,
    /// 1-based line in the file the rule is about (0 = file-level).
    pub line: usize,
    /// The offending statement / reference / entry, or the vacuity detail.
    pub subject: String,
}

/// What one evaluation saw.
#[derive(Debug, Default)]
pub struct Report {
    /// Every finding (vacuity alone when a precondition failed).
    pub findings: Vec<Finding>,
    /// internal.proto statements scanned.
    pub statements: usize,
    /// internal.proto scope headers scanned.
    pub headers: usize,
    /// `import` statements seen.
    pub imports: usize,
    /// Qualified references extracted.
    pub refs: usize,
}

/// The three content matchers. A parameter so the neutered-matcher proof can
/// swap them; [`MATCHERS`] is the real set.
#[derive(Clone, Copy)]
pub struct Matchers {
    /// Check 1 over one statement or header.
    pub bytes: fn(&str) -> bool,
    /// Check 2's extractor over one statement or header.
    pub refs: fn(&str) -> Vec<String>,
    /// Check 3's recogniser: is this statement an `import`?
    pub is_import: fn(&str) -> bool,
}

/// The real matchers.
pub const MATCHERS: Matchers = Matchers {
    bytes: is_bytes,
    refs: qualified_refs,
    is_import,
};

/// `\bbytes([\s>]|$)|BytesValue`.
fn is_bytes(text: &str) -> bool {
    if text.contains("BytesValue") {
        return true;
    }
    let b = text.as_bytes();
    text.match_indices("bytes").any(|(at, m)| {
        let before = at.checked_sub(1).and_then(|i| b.get(i)).copied();
        let after = b.get(at + m.len()).copied();
        !before.is_some_and(is_ident_byte)
            && after.is_none_or(|c| c.is_ascii_whitespace() || c == b'>')
    })
}

fn qualified_refs(text: &str) -> Vec<String> {
    QUALIFIED_REF_RE
        .find_iter(text)
        .map(|m| m.as_str().to_string())
        .collect()
}

/// `^import\b`.
fn is_import(text: &str) -> bool {
    starts_with_keyword(text, "import").is_some()
}

fn vacuous(line: usize, subject: impl Into<String>) -> Finding {
    Finding {
        rule: Rule::Vacuous,
        line,
        subject: subject.into(),
    }
}

fn lexed(name: &str, content: Option<&str>) -> Result<Vec<Scope>, Finding> {
    let Some(content) = content else {
        return Err(vacuous(0, format!("file not found: {name}")));
    };
    if content.trim().is_empty() {
        return Err(vacuous(0, format!("file is empty: {name}")));
    }
    lex(content)
        .map(|t| scopes(&t))
        .map_err(|e| vacuous(0, format!("{name}: {e}")))
}

/// Evaluate the two files' contents (`None` = missing) with `m` against the
/// allow-lists.
#[must_use]
pub fn check_with(
    internal: Option<&str>,
    signaling: Option<&str>,
    m: Matchers,
    allowed_types: &[&str],
    allowed_imports: &[&str],
) -> Report {
    let mut report = Report::default();
    let (internal, signaling) = match (
        lexed(INTERNAL_PROTO, internal),
        lexed(SIGNALING_PROTO, signaling),
    ) {
        (Ok(i), Ok(s)) => (i, s),
        (i, s) => {
            report
                .findings
                .extend([i.err(), s.err()].into_iter().flatten());
            return report;
        }
    };

    report.findings.extend(identity_markers(&internal));
    if !report.findings.is_empty() {
        return report;
    }
    scan_internal(&internal, m, allowed_types, allowed_imports, &mut report);
    report
        .findings
        .extend(enum_boundary(&signaling, allowed_types));
    report
}

/// The identity markers, by STRUCTURE (not text): a top-level
/// `package dark_tower.internal.v1` statement and a top-level
/// `service MediaHandlerService` scope. Each absence is vacuity.
fn identity_markers(internal: &[Scope]) -> Vec<Finding> {
    let has_package = internal.first().is_some_and(|root| {
        root.statements
            .iter()
            .any(|(_, t)| t == "package dark_tower.internal.v1")
    });
    let has_service = internal
        .iter()
        .any(|s| s.nesting == 0 && s.header.as_deref() == Some("service MediaHandlerService"));
    let mut out = Vec::new();
    if !has_package {
        out.push(vacuous(
            0,
            "identity marker `package dark_tower.internal.v1` (top-level statement) absent — the \
             scanned file is not the internal contract",
        ));
    }
    if !has_service {
        out.push(vacuous(
            0,
            "identity marker `service MediaHandlerService` (top-level scope) absent — the \
             scanned file is not the internal contract",
        ));
    }
    out
}

/// Checks 1-3 over every statement AND scope header of internal.proto, in
/// line order (imports are statements only).
fn scan_internal(
    internal: &[Scope],
    m: Matchers,
    allowed_types: &[&str],
    allowed_imports: &[&str],
    report: &mut Report,
) {
    let mut texts: Vec<(usize, &str, bool)> = Vec::new();
    for s in internal {
        if let Some(h) = &s.header {
            report.headers += 1;
            texts.push((s.line, h, false));
        }
        for (line, t) in &s.statements {
            report.statements += 1;
            texts.push((*line, t, true));
        }
    }
    texts.sort_by_key(|(line, _, _)| *line);
    for (line, text, is_statement) in texts {
        if (m.bytes)(text) {
            report.findings.push(Finding {
                rule: Rule::BytesField,
                line,
                subject: text.to_string(),
            });
        }
        for r in (m.refs)(text) {
            report.refs += 1;
            if !allowed_types.contains(&r.as_str()) {
                report.findings.push(Finding {
                    rule: Rule::CrossPackageType,
                    line,
                    subject: r,
                });
            }
        }
        if is_statement && (m.is_import)(text) {
            report.imports += 1;
            if !allowed_imports
                .iter()
                .any(|imp| text == format!("import \"{imp}\""))
            {
                report.findings.push(Finding {
                    rule: Rule::ImportNotAllowed,
                    line,
                    subject: text.to_string(),
                });
            }
        }
    }
}

/// The enums-only boundary, resolved at TOP LEVEL of signaling.proto: a
/// top-level `enum X` passes, a top-level `message X` is
/// [`Rule::AllowlistNotEnum`], no top-level declaration is vacuity.
fn enum_boundary(signaling: &[Scope], allowed_types: &[&str]) -> Vec<Finding> {
    let mut out = Vec::new();
    for t in allowed_types {
        let short = t.rsplit('.').next().unwrap_or(t);
        let top = |kw: &str| {
            signaling.iter().find(|s| {
                s.nesting == 0 && s.header.as_deref() == Some(format!("{kw} {short}").as_str())
            })
        };
        if top("enum").is_some() {
            continue;
        }
        if let Some(msg) = top("message") {
            out.push(Finding {
                rule: Rule::AllowlistNotEnum,
                line: msg.line,
                subject: (*t).to_string(),
            });
        } else {
            out.push(vacuous(
                0,
                format!("allowlist entry '{t}' is not declared at top level in {SIGNALING_PROTO}"),
            ));
        }
    }
    out
}

const RULE_TEXT: &str = "ADR-0036 §4 \"No key material crosses the MC->MH contract\"";
const ROUTE_TEXT: &str = "This guard is a mechanical proxy for that rule. A legitimate need is a \
    re-derivation of the rule with @security — not a field deletion, and not an edit to this \
    guard (it has no suppression path).";

fn read_optional(root: &Path, rel: &str) -> Result<Option<String>> {
    let p = root.join(rel);
    if !p.is_file() {
        return Ok(None);
    }
    Ok(Some(std::fs::read_to_string(p)?))
}

/// Run the guard over `root`.
///
/// # Errors
///
/// Any finding (the token names the first rule, vacuity first), or an
/// unreadable file.
pub fn run(root: &Path, explain: bool) -> Result<()> {
    let internal = read_optional(root, INTERNAL_PROTO)?;
    let signaling = read_optional(root, SIGNALING_PROTO)?;
    let report = check_with(
        internal.as_deref(),
        signaling.as_deref(),
        MATCHERS,
        ALLOWED_TYPES,
        ALLOWED_IMPORTS,
    );
    emit_scope(format!(
        "{INTERNAL_PROTO}: {} statements, {} scope headers, {} imports, {} qualified refs; {} \
         findings",
        report.statements,
        report.headers,
        report.imports,
        report.refs,
        report.findings.len()
    ));
    if report.findings.is_empty() {
        emit_ok("internal-proto-no-key-material-clean");
        return Ok(());
    }
    let mut ordered: Vec<&Finding> = report.findings.iter().collect();
    ordered.sort_by_key(|f| usize::from(f.rule != Rule::Vacuous));
    for f in &ordered {
        match f.rule {
            Rule::BytesField | Rule::CrossPackageType | Rule::ImportNotAllowed => println!(
                "VIOLATION [{}] in {INTERNAL_PROTO}:{}: {}\n  Enforces {RULE_TEXT}. {ROUTE_TEXT}",
                f.rule.id(),
                f.line,
                f.subject
            ),
            Rule::AllowlistNotEnum => println!(
                "VIOLATION [{}] allowlist entry '{}' is declared as a MESSAGE in \
                 {SIGNALING_PROTO}:{}. The allowlist is top-level-enums-only: a message type can \
                 carry a bytes field, which is the hole check 2 closes. Remove it from \
                 ALLOWED_TYPES; widening the boundary is a re-derivation with @security.",
                f.rule.id(),
                f.subject,
                f.line
            ),
            Rule::Vacuous => println!(
                "VIOLATION [{}] {} — the guard could not evaluate its input, so it refuses to \
                 report clean. This is NOT a content violation: do not loosen a pattern; \
                 restore the input.",
                f.rule.id(),
                f.subject
            ),
        }
        if explain {
            println!(
                "EXPLAIN: {INTERNAL_PROTO}:{}:1 policy={} matched=\"{}\" src={}:{}",
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

    const ANCHOR: &str = "  repeated SourceStream source_streams = 8;";

    fn real(rel: &str) -> String {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        std::fs::read_to_string(root.join(rel)).unwrap()
    }

    /// Copies of the real protos (the bash `reset_tree`).
    fn tree() -> (String, String) {
        (real(INTERNAL_PROTO), real(SIGNALING_PROTO))
    }

    /// Insert `decl` right after the last pre-existing field of
    /// `RegisterMeetingRequest` (the bash `plant_field`). Returns the 1-based
    /// line the planted text starts on.
    fn plant(internal: &str, decl: &str) -> (String, usize) {
        let at = internal
            .lines()
            .position(|l| l == ANCHOR)
            .expect("precondition: anchor present in internal.proto");
        let mut lines: Vec<&str> = internal.lines().collect();
        lines.insert(at + 1, decl);
        (lines.join("\n") + "\n", at + 2)
    }

    fn eval(internal: Option<&str>, signaling: Option<&str>) -> Report {
        check_with(
            internal,
            signaling,
            MATCHERS,
            ALLOWED_TYPES,
            ALLOWED_IMPORTS,
        )
    }

    fn planted(decl: &str) -> (Report, usize) {
        let (i, s) = tree();
        let (i, line) = plant(&i, decl);
        (eval(Some(&i), Some(&s)), line)
    }

    fn rules(r: &Report) -> Vec<Rule> {
        r.findings.iter().map(|f| f.rule).collect()
    }

    /// bash `p0-real-tree-passes` (no root override: the real repo through
    /// `run`'s own reader) and `p1-synthetic-baseline-passes`: clean AND
    /// non-vacuous.
    #[test]
    fn the_real_tree_is_clean_and_non_vacuous() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let i = read_optional(&root, INTERNAL_PROTO).unwrap();
        let s = read_optional(&root, SIGNALING_PROTO).unwrap();
        let r = eval(i.as_deref(), s.as_deref());
        assert!(r.findings.is_empty(), "{:?}", r.findings);
        assert!(r.statements > 0 && r.headers > 0, "{r:?}");
        assert_eq!(r.imports, 1);
        assert!(r.refs > 0, "the canonical TransportMode refs are SEEN");
    }

    #[test]
    fn synthetic_baseline_is_clean() {
        let (i, s) = tree();
        let (i, _) = plant(&i, "  uint32 extra = 90;");
        assert!(eval(Some(&i), Some(&s)).findings.is_empty());
    }

    /// bash `c1-pos-1..6` + `c1-oneof`; each asserts the line and that it is
    /// a content red, not vacuity.
    #[test]
    fn every_bytes_spelling_hits() {
        for decl in [
            "  optional bytes k = 90;",
            "  bytes k = 90;",
            "  repeated bytes k = 90;",
            "  map<string, bytes> k = 90;",
            "  map<string,bytes> k = 90;",
            "  google.protobuf.BytesValue k = 90;",
        ] {
            let (r, line) = planted(decl);
            let bytes: Vec<&Finding> = r
                .findings
                .iter()
                .filter(|f| f.rule == Rule::BytesField)
                .collect();
            assert_eq!(bytes.len(), 1, "{decl}: {:?}", r.findings);
            assert_eq!(bytes[0].line, line, "{decl}");
            assert!(!rules(&r).contains(&Rule::Vacuous), "{decl}");
        }
        let (r, line) = planted("  oneof choice {\n    bytes k = 90;\n  }");
        assert_eq!(
            r.findings,
            vec![Finding {
                rule: Rule::BytesField,
                line: line + 1,
                subject: "bytes k = 90".into()
            }]
        );
    }

    /// NEW (security A2): a `BytesValue` in an rpc SIGNATURE is a scope header.
    #[test]
    fn bytes_value_in_an_rpc_header_hits() {
        let (i, s) = tree();
        let i = i.replacen(
            "service MediaHandlerService {",
            "service MediaHandlerService {\n  rpc Probe(google.protobuf.BytesValue) returns (RegisterMeetingRequest) { option deprecated = true; }",
            1,
        );
        let r = eval(Some(&i), Some(&s));
        assert!(
            r.findings.iter().any(|f| f.rule == Rule::BytesField
                && f.subject
                    == "rpc Probe(google.protobuf.BytesValue) returns (RegisterMeetingRequest)"),
            "{:?}",
            r.findings
        );
    }

    /// bash `c1-neg-total-bytes`, `-bytes-sent-label`, `-comment-prose`.
    #[test]
    fn byte_counters_and_prose_stay_green() {
        for decl in [
            "  uint64 total_bytes = 90;",
            "  string bytes_sent_label = 90;",
            "  // a prose mention: never add a bytes k = 1; field here",
        ] {
            assert!(planted(decl).0.findings.is_empty(), "{decl}");
        }
    }

    /// bash `c2-pos-1..6`: each a cross-package red naming the reference,
    /// and NOT a bytes red.
    #[test]
    fn every_cross_package_spelling_hits() {
        for (decl, name) in [
            (
                "  dark_tower.signaling.v1.MeetingKekUpdate k = 90;",
                "dark_tower.signaling.v1.MeetingKekUpdate",
            ),
            (
                "  signaling.v1.MeetingKekUpdate k = 90;",
                "signaling.v1.MeetingKekUpdate",
            ),
            (
                "  .dark_tower.signaling.v1.MeetingKekUpdate k = 90;",
                ".dark_tower.signaling.v1.MeetingKekUpdate",
            ),
            (
                "  google.protobuf.Timestamp t = 90;",
                "google.protobuf.Timestamp",
            ),
            ("  google.protobuf.Any a = 90;", "google.protobuf.Any"),
            (
                "  signaling.v1.TransportMode m = 90;",
                "signaling.v1.TransportMode",
            ),
        ] {
            let (r, line) = planted(decl);
            assert_eq!(
                r.findings,
                vec![Finding {
                    rule: Rule::CrossPackageType,
                    line,
                    subject: name.into()
                }],
                "{decl}"
            );
        }
    }

    /// NEW (security A2 / protocol B1): a cross-package type in an rpc header.
    #[test]
    fn cross_package_in_an_rpc_header_hits() {
        let (i, s) = tree();
        let i = i.replacen(
            "service MediaHandlerService {",
            "service MediaHandlerService {\n  rpc Foo(.dark_tower.signaling.v1.MeetingKekUpdate) returns (RegisterMeetingRequest) {}",
            1,
        );
        let r = eval(Some(&i), Some(&s));
        assert!(
            r.findings.iter().any(|f| f.rule == Rule::CrossPackageType
                && f.subject == ".dark_tower.signaling.v1.MeetingKekUpdate"),
            "{:?}",
            r.findings
        );
    }

    /// bash `c2-neg-canonical-transport-mode`, `c2-neg-comment-prose`.
    #[test]
    fn canonical_allowlisted_enum_and_prose_stay_green() {
        for decl in [
            "  dark_tower.signaling.v1.TransportMode extra_mode = 90;",
            "  // see dark_tower.signaling.v1.MeetingKekUpdate for why this is absent",
        ] {
            assert!(planted(decl).0.findings.is_empty(), "{decl}");
        }
    }

    /// bash `enum-only`, plus @security's depth-0 resolution cases.
    #[test]
    fn allowlisted_name_declared_as_message_hits() {
        let (i, s) = tree();
        let as_message = s.replacen("enum TransportMode {", "message TransportMode {", 1);
        assert_ne!(as_message, s, "precondition: mutation applied");
        let r = eval(Some(&i), Some(&as_message));
        assert_eq!(rules(&r), vec![Rule::AllowlistNotEnum], "{:?}", r.findings);
        assert_eq!(r.findings[0].subject, ALLOWED_TYPES[0]);

        // Nested-only declaration: vacuity, not green (the tightening).
        let renamed = s.replacen("enum TransportMode {", "enum RenamedMode {", 1);
        let nested_only =
            format!("{renamed}\nmessage Outer {{\n  enum TransportMode {{\n    X = 0;\n  }}\n}}\n");
        let r = eval(Some(&i), Some(&nested_only));
        assert_eq!(rules(&r), vec![Rule::Vacuous], "{:?}", r.findings);
        assert!(r.findings[0].subject.contains("not declared at top level"));

        // Top-level enum plus a nested message of the same name: green.
        let shadowed = format!("{s}\nmessage Outer {{\n  message TransportMode {{}}\n}}\n");
        assert!(eval(Some(&i), Some(&shadowed)).findings.is_empty());
    }

    /// bash `c3-import`, `c3-import-public-form`, plus `weak`.
    #[test]
    fn non_allowlisted_imports_hit() {
        let (i, s) = tree();
        let plain = "import \"dark_tower/signaling/v1/signaling.proto\";";
        for (replacement, subject) in [
            (
                format!("{plain}\nimport \"dark_tower/internal/v1/keys.proto\";"),
                "import \"dark_tower/internal/v1/keys.proto\"",
            ),
            (
                "import public \"dark_tower/signaling/v1/signaling.proto\";".to_string(),
                "import public \"dark_tower/signaling/v1/signaling.proto\"",
            ),
            (
                "import weak \"dark_tower/signaling/v1/signaling.proto\";".to_string(),
                "import weak \"dark_tower/signaling/v1/signaling.proto\"",
            ),
        ] {
            let m = i.replacen(plain, &replacement, 1);
            assert_ne!(m, i, "precondition: mutation applied");
            let r = eval(Some(&m), Some(&s));
            assert_eq!(
                r.findings
                    .iter()
                    .map(|f| (f.rule, f.subject.as_str()))
                    .collect::<Vec<_>>(),
                vec![(Rule::ImportNotAllowed, subject)]
            );
        }
    }

    /// bash `ws-split-bytes` (pins the SPACE-join: `bytes` + `raw` must not
    /// fuse), `ws-split-type`, `ws-spaced-type`, `ws-spaceless-import`.
    #[test]
    fn whitespace_bypasses_hit() {
        let (r, _) = planted("  bytes\n    raw = 90;");
        assert_eq!(rules(&r), vec![Rule::BytesField]);
        assert_eq!(r.findings[0].subject, "bytes raw = 90");
        for decl in [
            "  dark_tower.signaling.v1\n    .MeetingKekUpdate split_type = 90;",
            "  dark_tower . signaling . v1 . MeetingKekUpdate spaced_type = 90;",
        ] {
            let (r, _) = planted(decl);
            assert_eq!(rules(&r), vec![Rule::CrossPackageType], "{decl}");
            assert_eq!(
                r.findings[0].subject,
                "dark_tower.signaling.v1.MeetingKekUpdate"
            );
        }
        let (i, s) = tree();
        let plain = "import \"dark_tower/signaling/v1/signaling.proto\";";
        let m = i.replacen(
            plain,
            &format!("{plain}\nimport\"dark_tower/internal/v1/keys.proto\";"),
            1,
        );
        assert_eq!(
            rules(&eval(Some(&m), Some(&s))),
            vec![Rule::ImportNotAllowed]
        );
    }

    /// bash `vac-*`, plus the lexer's new fail-closed refusals and a nested
    /// (non-top-level) service. Each is vacuity and ONLY vacuity.
    #[test]
    fn vacuity_cases() {
        let (i, s) = tree();
        let only_vacuous = |r: Report, needle: &str| {
            assert!(!r.findings.is_empty(), "{needle}");
            assert!(
                r.findings.iter().all(|f| f.rule == Rule::Vacuous),
                "{needle}: {:?}",
                r.findings
            );
            assert!(
                r.findings.iter().any(|f| f.subject.contains(needle)),
                "{needle}: {:?}",
                r.findings
            );
        };
        only_vacuous(eval(None, Some(&s)), "file not found");
        only_vacuous(eval(Some(""), Some(&s)), "file is empty");
        only_vacuous(eval(Some(&i), None), "file not found");
        let pkg = i.replacen(
            "package dark_tower.internal.v1;",
            "package dark_tower.other.v1;",
            1,
        );
        only_vacuous(eval(Some(&pkg), Some(&s)), "package dark_tower.internal.v1");
        let nested_service = i.replacen(
            "service MediaHandlerService {",
            "message Wrap { }\nservice Elsewhere {\n}\nmessage Hold {\n  message MediaHandlerService {\n  }\n}\nservice Renamed {",
            1,
        );
        only_vacuous(
            eval(Some(&nested_service), Some(&s)),
            "service MediaHandlerService",
        );
        let renamed = s.replacen("enum TransportMode {", "enum RenamedMode {", 1);
        only_vacuous(eval(Some(&i), Some(&renamed)), "not declared at top level");
        let (block, _) = plant(&i, "  /* block comment the line strip cannot see into */");
        only_vacuous(eval(Some(&block), Some(&s)), "block comment");
        let (quote, _) = plant(&i, "  string n = 90 [json_name = 'ab//c'];");
        only_vacuous(eval(Some(&quote), Some(&s)), "single quote");
        let (brace, _) = plant(&i, "  }");
        only_vacuous(eval(Some(&brace), Some(&s)), "unbalanced braces");
    }

    /// No suppression path: a `guard:ignore` marker on a planted field is
    /// not honoured.
    #[test]
    fn guard_ignore_marker_is_not_honoured() {
        let (r, _) = planted("  bytes k = 90; // guard:ignore(needed for the probe here)");
        assert_eq!(rules(&r), vec![Rule::BytesField]);
    }

    /// Neutered-matcher proof: with the three matchers swapped for ones that
    /// never fire, the red fixtures go green -- so the red tests above detect
    /// a broken matcher.
    #[test]
    fn neutered_matcher_goes_green() {
        let (i, s) = tree();
        let (i, _) = plant(
            &i,
            "  bytes k = 90;\n  dark_tower.signaling.v1.MeetingKekUpdate u = 91;",
        );
        let i = i.replacen(
            "import \"dark_tower/signaling/v1/signaling.proto\";",
            "import public \"dark_tower/signaling/v1/signaling.proto\";",
            1,
        );
        let armed = eval(Some(&i), Some(&s));
        assert_eq!(
            rules(&armed),
            vec![
                Rule::ImportNotAllowed,
                Rule::BytesField,
                Rule::CrossPackageType
            ]
        );
        let never = Matchers {
            bytes: |_| false,
            refs: |_| Vec::new(),
            is_import: |_| false,
        };
        let r = check_with(Some(&i), Some(&s), never, ALLOWED_TYPES, ALLOWED_IMPORTS);
        assert!(r.findings.is_empty(), "{:?}", r.findings);
    }
}
