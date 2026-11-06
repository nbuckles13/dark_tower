//! `internal-proto-no-key-material`: NO SECRET OR MEDIA-KEY MATERIAL ON THE
//! INTERNAL CONTRACT; EXACTLY ALLOW-LISTED PUBLIC VERIFICATION KEYS ONLY
//! (ADR-0036 §4, re-derived by ADR-0039 §5), mechanised for
//! `proto/dark_tower/internal/v1/internal.proto`.
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
//!    `bytes`. The ONE exception path is [`ALLOWED_BYTES_FIELDS`] (below);
//!    no other check consults it.
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
//! # The check-1 key allow-list (`internal-proto-key-allowlist-invalid`)
//!
//! [`ALLOWED_BYTES_FIELDS`] is exact `(fully-qualified message, field)` pairs,
//! matched by string EQUALITY on both components over the shared field reader
//! (`common::proto_lex::fields`): no pattern, prefix, type exemption, marker,
//! env var or flag. An entry exempts exactly ONE statement from check 1 --
//! the single declaration of that field -- and only if it has the plain shape
//! `bytes <field> = <N>`: declared DIRECTLY in a TOP-LEVEL message of
//! internal.proto (not nested, not a oneof member, not inside `extend`), no
//! label (`optional`/`repeated`), no `[options]`; so `map<…, bytes>` and
//! `BytesValue` never qualify. ADR-0039 §5 is one 32-byte public key, not a
//! list of keys. Checks 2 and 3 still run on the exempt statement; scope
//! headers are never exempt. Every entry is validated on every run, and an
//! invalid entry exempts nothing. Its finding subject is
//! `(message=<FQN>, field=<name>): <reason>`, reason one of
//! `message-not-found`, `field-not-found`, `duplicate-declaration`,
//! `non-plain-bytes`, `duplicate-entry`.
//!
//! REASON precedence: `internal-proto-scan-vacuous`, then
//! `internal-proto-key-allowlist-invalid`, then the content tokens (a renamed
//! listed field reds both; the stale entry is the root cause).
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
//! no marker, env var or flag. [`ALLOWED_BYTES_FIELDS`] is compiled-in,
//! security-owned content, not a runtime override. A legitimate need is a
//! re-derivation of the rule with @security, not an edit here.

use crate::common::ident::is_ident_byte;
use crate::common::proto_lex::{
    fields, lex, scopes, starts_with_keyword, top_level, Field, Scope, Token,
};
use crate::common::status::{emit_ok, emit_scope};
use anyhow::{bail, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use std::path::Path;

/// The contract this guard protects.
pub const INTERNAL_PROTO: &str = "proto/dark_tower/internal/v1/internal.proto";
/// Where the allow-listed enums are declared.
pub const SIGNALING_PROTO: &str = "proto/dark_tower/signaling/v1/signaling.proto";
/// internal.proto's package: an identity marker, and the prefix of every
/// [`ALLOWED_BYTES_FIELDS`] message.
pub const INTERNAL_PACKAGE: &str = "dark_tower.internal.v1";

// ---- security-owned CONTENT under a machinery-owned construct ----------------
/// Cross-package types permitted on the internal contract. TOP-LEVEL ENUMS
/// ONLY: an enum cannot carry a `bytes` field, so it needs no per-entry
/// judgement; a MESSAGE type is exactly the hole check 2 closes. Adding an
/// entry requires @security sign-off, and the guard reds a non-enum regardless
/// (`internal-proto-allowlist-not-enum`). Canonical FQN.
pub const ALLOWED_TYPES: &[&str] = &["dark_tower.signaling.v1.TransportMode"];
/// Imports permitted, as the exact path (plain form only). @security content.
pub const ALLOWED_IMPORTS: &[&str] = &["dark_tower/signaling/v1/signaling.proto"];

/// Where [`ALLOWED_BYTES_FIELDS`] is declared: the location an entry finding
/// with no declaration line points the operator at. Captured ABOVE the list's
/// doc block, so entries added to the list (below its first line) never move
/// it; the offset is this item's 4 lines plus the doc block's 9, pinned by
/// `invalid_entry_location_points_at_the_const_or_the_declaration`.
const ALLOWED_BYTES_FIELDS_AT: (&str, u32) = (file!(), line!() + 1 + 9);
/// Check 1's key allow-list: the ONLY `bytes` fields permitted on the internal
/// contract, as exact (fully-qualified message, field) pairs (ADR-0039 §5).
///
/// MEMBERSHIP is @security-owned CONTENT: an entry is a public verification
/// key that a security review has decided may cross MC->MH. The SHAPE of an
/// entry and how it is EVALUATED (exact equality, plain-`bytes` resolution,
/// fail-loud validation of every entry) are @infrastructure MACHINERY.
/// NEVER string-encode a key (base64 in a `string`, hex, ...) to dodge this
/// guard: that is the same key material in a costume (ADR-0039 §5).
pub const ALLOWED_BYTES_FIELDS: &[AllowedBytesField] = &[];
// -----------------------------------------------------------------------------

/// One [`ALLOWED_BYTES_FIELDS`] entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AllowedBytesField {
    /// Fully-qualified message, e.g. `dark_tower.internal.v1.SomeMessage`.
    pub message: &'static str,
    /// The field name on that exact message.
    pub field: &'static str,
}

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
    /// An [`ALLOWED_BYTES_FIELDS`] entry does not resolve to exactly one plain
    /// `bytes` field.
    KeyAllowlistInvalid,
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
            Self::KeyAllowlistInvalid => "internal-proto-key-allowlist-invalid",
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
    /// [`ALLOWED_BYTES_FIELDS`] entries validated.
    pub key_allowlist_entries: usize,
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

fn invalid_entry(line: usize, e: &AllowedBytesField, reason: &str) -> Finding {
    Finding {
        rule: Rule::KeyAllowlistInvalid,
        line,
        subject: format!("(message={}, field={}): {reason}", e.message, e.field),
    }
}

fn lexed(name: &str, content: Option<&str>) -> Result<Vec<Token>, Finding> {
    let Some(content) = content else {
        return Err(vacuous(0, format!("file not found: {name}")));
    };
    if content.trim().is_empty() {
        return Err(vacuous(0, format!("file is empty: {name}")));
    }
    lex(content).map_err(|e| vacuous(0, format!("{name}: {e}")))
}

/// Evaluate the two files' contents (`None` = missing) with `m` against the
/// allow-lists. `allowed_bytes_fields` is a parameter for the fixtures ONLY:
/// `run` passes [`ALLOWED_BYTES_FIELDS`] and nothing else can.
#[must_use]
pub fn check_with(
    internal: Option<&str>,
    signaling: Option<&str>,
    m: Matchers,
    allowed_types: &[&str],
    allowed_imports: &[&str],
    allowed_bytes_fields: &[AllowedBytesField],
) -> Report {
    let mut report = Report::default();
    let (internal, signaling) = match (
        lexed(INTERNAL_PROTO, internal),
        lexed(SIGNALING_PROTO, signaling),
    ) {
        (Ok(i), Ok(s)) => (i, scopes(&s)),
        (i, s) => {
            report
                .findings
                .extend([i.err(), s.err()].into_iter().flatten());
            return report;
        }
    };

    let internal_fields = fields(&internal);
    let internal = scopes(&internal);
    report.findings.extend(identity_markers(&internal));
    if !report.findings.is_empty() {
        return report;
    }
    let (exempt, invalid) =
        resolve_key_allowlist(&internal, &internal_fields, allowed_bytes_fields);
    report.key_allowlist_entries = allowed_bytes_fields.len();
    report.findings.extend(invalid);
    scan_internal(
        &internal,
        m,
        allowed_types,
        allowed_imports,
        &exempt,
        &mut report,
    );
    report
        .findings
        .extend(enum_boundary(&signaling, allowed_types));
    report
}

/// The identity markers, by STRUCTURE (not text): a top-level
/// `package dark_tower.internal.v1` statement and a top-level
/// `service MediaHandlerService` scope. Each absence is vacuity.
fn identity_markers(internal: &[Scope]) -> Vec<Finding> {
    let marker = format!("package {INTERNAL_PACKAGE}");
    let has_package = internal
        .first()
        .is_some_and(|root| root.statements.iter().any(|(_, t)| *t == marker));
    let has_service = top_level(internal, "service", "MediaHandlerService")
        .next()
        .is_some();
    let mut out = Vec::new();
    if !has_package {
        out.push(vacuous(
            0,
            format!(
                "identity marker `package {INTERNAL_PACKAGE}` (top-level statement) absent — the \
                 scanned file is not the internal contract"
            ),
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

/// One statement's IDENTITY: `(scope index, statement index within that
/// scope)`, as `proto_lex::Field::{scope, statement}`. Unique by
/// construction -- never content (line/text), which two distinct statements
/// can share.
type StatementId = (usize, usize);

/// Validate every key allow-list entry against internal.proto. Returns the
/// [`StatementId`] of each VALID entry's one declaration -- the only
/// statements check 1 may skip -- and a finding per invalid entry (which
/// exempts nothing).
fn resolve_key_allowlist(
    internal: &[Scope],
    internal_fields: &[Field],
    entries: &[AllowedBytesField],
) -> (Vec<StatementId>, Vec<Finding>) {
    let mut exempt = Vec::new();
    let mut invalid = Vec::new();
    for (i, e) in entries.iter().enumerate() {
        if entries.iter().take(i).any(|seen| seen == e) {
            invalid.push(invalid_entry(0, e, "duplicate-entry"));
            continue;
        }
        // A TOP-LEVEL message of this package: full-string prefix, then one
        // identifier (a nested path keeps its dot and never resolves).
        let short = e
            .message
            .strip_prefix(INTERNAL_PACKAGE)
            .and_then(|r| r.strip_prefix('.'))
            .filter(|r| !r.is_empty() && !r.contains('.'));
        let declared =
            short.is_some_and(|short| top_level(internal, "message", short).next().is_some());
        if !declared {
            invalid.push(invalid_entry(0, e, "message-not-found"));
            continue;
        }
        let candidates: Vec<&Field> = internal_fields
            .iter()
            .filter(|f| f.message == e.message && f.name == e.field)
            .collect();
        match candidates.as_slice() {
            [] => invalid.push(invalid_entry(0, e, "field-not-found")),
            [f] => {
                let plain =
                    f.direct_in_message && f.label.is_none() && f.ty == "bytes" && !f.has_options;
                // Security SEC-2: the id comes from `fields()` but indexes
                // `scopes()`; confirm against the real scopes that it names
                // THIS declaration directly in THIS top-level message, else
                // exempt nothing (two walkers drifting is a machinery fault,
                // never a silent green).
                let header = short.map(|short| format!("message {short}"));
                let agrees = internal.get(f.scope).is_some_and(|sc| {
                    sc.nesting == 0
                        && sc.header == header
                        && sc.statements.get(f.statement) == Some(&(f.line, f.text.clone()))
                });
                if plain && !agrees {
                    invalid.push(vacuous(
                        0,
                        format!(
                            "key allow-list entry (message={}, field={}): the shared field reader \
                             and scope walker disagree on its declaration's identity (scope {}, \
                             statement {}) — a dt-guard machinery fault; nothing exempted",
                            e.message, e.field, f.scope, f.statement
                        ),
                    ));
                } else if plain {
                    exempt.push((f.scope, f.statement));
                } else {
                    invalid.push(invalid_entry(f.line, e, "non-plain-bytes"));
                }
            }
            [first, ..] => invalid.push(invalid_entry(first.line, e, "duplicate-declaration")),
        }
    }
    (exempt, invalid)
}

/// Checks 1-3 over every statement AND scope header of internal.proto, in
/// line order (imports are statements only). `exempt` lifts check 1 ONLY,
/// for statements only; checks 2 and 3 still run on them.
fn scan_internal(
    internal: &[Scope],
    m: Matchers,
    allowed_types: &[&str],
    allowed_imports: &[&str],
    exempt: &[StatementId],
    report: &mut Report,
) {
    // (line, text, the statement's identity; `None` for a scope header)
    let mut texts: Vec<(usize, &str, Option<StatementId>)> = Vec::new();
    for (idx, s) in internal.iter().enumerate() {
        if let Some(h) = &s.header {
            report.headers += 1;
            texts.push((s.line, h, None));
        }
        for (n, (line, t)) in s.statements.iter().enumerate() {
            report.statements += 1;
            texts.push((*line, t, Some((idx, n))));
        }
    }
    texts.sort_by_key(|(line, _, _)| *line);
    for (line, text, id) in texts {
        let is_statement = id.is_some();
        let exempted = id.is_some_and(|id| exempt.contains(&id));
        if (m.bytes)(text) && !exempted {
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
        let top = |kw: &str| top_level(signaling, kw, short).next();
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

const RULE_TEXT: &str = "ADR-0036 §4 / ADR-0039 §5 \"no secret or media-key material on the \
    internal contract; exactly allow-listed public verification keys only\"";
const ROUTE_TEXT: &str = "This guard is a mechanical proxy for that rule. A legitimate need is a \
    re-derivation of the rule with @security — not a field deletion, and not an edit to this \
    guard (it has no suppression path).";

/// The `(file, line)` a finding points at. An invalid key allow-list entry
/// with no declaration line (`message-not-found`, `field-not-found`,
/// `duplicate-entry`) points at [`ALLOWED_BYTES_FIELDS`] in THIS module -- the
/// thing to fix -- never at a non-existent internal.proto line.
fn location(f: &Finding) -> (&'static str, usize) {
    if f.rule == Rule::KeyAllowlistInvalid && f.line == 0 {
        let (file, line) = ALLOWED_BYTES_FIELDS_AT;
        return (file, line as usize);
    }
    (INTERNAL_PROTO, f.line.max(1))
}

/// `run`'s REASON precedence: vacuity, then an invalid key allow-list entry
/// (the root cause when a listed field was renamed), then content.
const fn precedence(rule: Rule) -> u8 {
    match rule {
        Rule::Vacuous => 0,
        Rule::KeyAllowlistInvalid => 1,
        Rule::BytesField
        | Rule::CrossPackageType
        | Rule::AllowlistNotEnum
        | Rule::ImportNotAllowed => 2,
    }
}

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
        ALLOWED_BYTES_FIELDS,
    );
    emit_scope(format!(
        "{INTERNAL_PROTO}: {} statements, {} scope headers, {} imports, {} qualified refs, {} key \
         allow-list entries; {} findings",
        report.statements,
        report.headers,
        report.imports,
        report.refs,
        report.key_allowlist_entries,
        report.findings.len()
    ));
    if report.findings.is_empty() {
        emit_ok("internal-proto-no-key-material-clean");
        return Ok(());
    }
    let mut ordered: Vec<&Finding> = report.findings.iter().collect();
    ordered.sort_by_key(|f| precedence(f.rule));
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
            Rule::KeyAllowlistInvalid => println!(
                "VIOLATION [{}] key allow-list entry {} ({}). Every \
                 ALLOWED_BYTES_FIELDS entry must resolve to exactly one field declared as plain \
                 `bytes <field> = <N>` directly in a top-level message of {INTERNAL_PROTO} (no \
                 label, no options, not a oneof member); an invalid entry exempts nothing. Fix or \
                 remove the entry with @security — never string-encode a key to dodge the guard \
                 (ADR-0039 §5).",
                f.rule.id(),
                f.subject,
                if f.line == 0 {
                    format!(
                        "entry in ALLOWED_BYTES_FIELDS at {}:{}; no matching declaration in \
                         {INTERNAL_PROTO}",
                        location(f).0,
                        location(f).1
                    )
                } else {
                    format!("declaration at {INTERNAL_PROTO}:{}", f.line)
                }
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
                "EXPLAIN: {}:{}:1 policy={} matched=\"{}\" src={}:{}",
                location(f).0,
                location(f).1,
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
            ALLOWED_BYTES_FIELDS,
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
        let r = check_with(
            Some(&i),
            Some(&s),
            never,
            ALLOWED_TYPES,
            ALLOWED_IMPORTS,
            ALLOWED_BYTES_FIELDS,
        );
        assert!(r.findings.is_empty(), "{:?}", r.findings);
    }

    // ---- check 1's key allow-list (ADR-0039 §5), test-local lists only ----

    const LISTED_MSG: &str = "dark_tower.internal.v1.RegisterMeetingRequest";
    const ENTRY: AllowedBytesField = AllowedBytesField {
        message: LISTED_MSG,
        field: "listed",
    };
    const LISTED: &str = "  bytes listed = 90;";

    fn eval_with(internal: &str, list: &[AllowedBytesField]) -> Report {
        let (_, s) = tree();
        check_with(
            Some(internal),
            Some(&s),
            MATCHERS,
            ALLOWED_TYPES,
            ALLOWED_IMPORTS,
            list,
        )
    }

    fn bytes_red(line: usize, subject: &str) -> Finding {
        Finding {
            rule: Rule::BytesField,
            line,
            subject: subject.into(),
        }
    }

    fn invalid(line: usize, e: &AllowedBytesField, reason: &str) -> Finding {
        Finding {
            rule: Rule::KeyAllowlistInvalid,
            line,
            subject: format!("(message={}, field={}): {reason}", e.message, e.field),
        }
    }

    /// 1-based line of the first line of `src` equal to `needle`.
    fn line_of(src: &str, needle: &str) -> usize {
        src.lines()
            .position(|l| l == needle)
            .map(|i| i + 1)
            .expect("precondition: fixture line present")
    }

    /// The membership pin (@security S7, Lead ruling): the shipped list is
    /// reviewed CONTENT. P1 replaces this with an exact `assert_eq!` on its
    /// one (message, field) pair, so the entry arrives as a deliberate,
    /// security-reviewed test diff, never only as a const edit.
    #[test]
    fn shipped_key_allowlist_is_reviewed_content() {
        assert_eq!(ALLOWED_BYTES_FIELDS, &[] as &[AllowedBytesField]);
    }

    /// Positive arm and its paired negative on ONE planted tree: the list,
    /// and only the list, makes it green.
    #[test]
    fn a_listed_bytes_field_passes_and_the_same_tree_unlisted_reds() {
        let (i, _) = tree();
        let (i, line) = plant(&i, LISTED);
        let r = eval_with(&i, &[ENTRY]);
        assert!(r.findings.is_empty(), "{:?}", r.findings);
        assert_eq!(r.key_allowlist_entries, 1);
        let r = eval_with(&i, &[]);
        assert_eq!(r.findings, vec![bytes_red(line, "bytes listed = 90")]);
        assert_eq!(r.key_allowlist_entries, 0);
    }

    /// The same field name elsewhere stays red: a different top-level
    /// message, a message nested IN the listed one, and a same-named message
    /// nested in another message. The real listed field stays green.
    #[test]
    fn the_listed_name_on_any_other_message_reds() {
        let (base, _) = tree();
        let (base, _) = plant(&base, LISTED);
        let other = base.replacen(
            "message EndMeetingRequest {",
            "message EndMeetingRequest {\n  bytes listed = 90;",
            1,
        );
        let nested_in_listed = plant(&base, "  message Inner {\n    bytes listed = 1;\n  }").0;
        let same_name_nested =
            format!("{base}message Other {{\n  message RegisterMeetingRequest {{\n    bytes listed = 1;\n  }}\n}}\n");
        for (label, i, needle, subject) in [
            (
                "other message",
                &other,
                "  bytes listed = 90;",
                "bytes listed = 90",
            ),
            (
                "nested in listed",
                &nested_in_listed,
                "    bytes listed = 1;",
                "bytes listed = 1",
            ),
            (
                "same name nested",
                &same_name_nested,
                "    bytes listed = 1;",
                "bytes listed = 1",
            ),
        ] {
            assert_ne!(i, &base, "precondition: mutation applied ({label})");
            // The mutated line: the first `needle` that is not the listed
            // plant itself (in `other` both lines read `bytes listed = 90`).
            let listed_line = line_of(&base, LISTED);
            let line = i
                .lines()
                .enumerate()
                .filter(|(n, l)| *l == needle && n + 1 != listed_line)
                .map(|(n, _)| n + 1)
                .next()
                .expect("precondition: planted line");
            assert_eq!(
                eval_with(i, &[ENTRY]).findings,
                vec![bytes_red(line, subject)],
                "{label}"
            );
        }
    }

    /// Entries that name no top-level message of internal.proto: a wrong
    /// package (the short name DOES exist here), a nested-message FQN, an
    /// unknown message, the bare short name. Each is invalid, exempts
    /// nothing, and is the only finding besides the planted red; the
    /// positive arm (the real entry, same tree) is the test above.
    #[test]
    fn an_entry_naming_no_top_level_message_is_invalid() {
        let (i, _) = tree();
        let (i, line) = plant(&i, LISTED);
        for message in [
            "dark_tower.signaling.v1.RegisterMeetingRequest",
            "dark_tower.internal.v1.RegisterMeetingRequest.Inner",
            "dark_tower.internal.v1.NoSuchMessage",
            "RegisterMeetingRequest",
            "dark_tower.internal.v1.",
        ] {
            let e = AllowedBytesField {
                message,
                field: "listed",
            };
            assert_eq!(
                eval_with(&i, &[e]).findings,
                vec![
                    invalid(0, &e, "message-not-found"),
                    bytes_red(line, "bytes listed = 90")
                ],
                "{message}"
            );
        }
    }

    /// Vacuity of a renamed / deleted field: invalid, and the renamed field
    /// itself still reds. REASON precedence puts the invalid entry first.
    #[test]
    fn a_renamed_or_deleted_listed_field_is_invalid() {
        let (i, _) = tree();
        let (renamed, line) = plant(&i, "  bytes listed_v2 = 90;");
        let r = eval_with(&renamed, &[ENTRY]);
        assert_eq!(
            r.findings,
            vec![
                invalid(0, &ENTRY, "field-not-found"),
                bytes_red(line, "bytes listed_v2 = 90")
            ]
        );
        let first = r
            .findings
            .iter()
            .min_by_key(|f| precedence(f.rule))
            .map(|f| f.rule);
        assert_eq!(first, Some(Rule::KeyAllowlistInvalid));
        // Deleted: an otherwise clean tree reds ONLY on the entry.
        assert_eq!(
            eval_with(&i, &[ENTRY]).findings,
            vec![invalid(0, &ENTRY, "field-not-found")]
        );
    }

    /// The field exists on the exact message but is not plain
    /// `bytes <field> = <N>`: invalid at the declaration, never a silent pass,
    /// and the statement's own content reds are kept.
    #[test]
    fn a_listed_field_that_is_not_plain_bytes_is_invalid() {
        for (decl, text, bytes, cross) in [
            ("  string listed = 90;", "string listed = 90", false, None),
            ("  uint64 listed = 90;", "uint64 listed = 90", false, None),
            (
                "  repeated bytes listed = 90;",
                "repeated bytes listed = 90",
                true,
                None,
            ),
            (
                "  optional bytes listed = 90;",
                "optional bytes listed = 90",
                true,
                None,
            ),
            (
                "  map<string, bytes> listed = 90;",
                "map<string, bytes> listed = 90",
                true,
                None,
            ),
            (
                "  google.protobuf.BytesValue listed = 90;",
                "google.protobuf.BytesValue listed = 90",
                true,
                Some("google.protobuf.BytesValue"),
            ),
            (
                "  bytes listed = 90 [deprecated = true];",
                "bytes listed = 90 [deprecated = true]",
                true,
                None,
            ),
        ] {
            let (i, _) = tree();
            let (i, line) = plant(&i, decl);
            let mut want = vec![invalid(line, &ENTRY, "non-plain-bytes")];
            if bytes {
                want.push(bytes_red(line, text));
            }
            if let Some(r) = cross {
                want.push(Finding {
                    rule: Rule::CrossPackageType,
                    line,
                    subject: r.into(),
                });
            }
            assert_eq!(eval_with(&i, &[ENTRY]).findings, want, "{decl}");
        }
    }

    /// A oneof member of the listed message is not a direct field (S4); a
    /// field inside an `extend` within it is not a field of it at all
    /// (security Q1). Neither is ever green; the direct field is.
    #[test]
    fn oneof_and_extend_declarations_never_satisfy_an_entry() {
        let (i, _) = tree();
        let (oneof, at) = plant(&i, "  oneof choice {\n    bytes listed = 90;\n  }");
        assert_eq!(
            eval_with(&oneof, &[ENTRY]).findings,
            vec![
                invalid(at + 1, &ENTRY, "non-plain-bytes"),
                bytes_red(at + 1, "bytes listed = 90")
            ]
        );
        let (extend, at) = plant(&i, "  extend Foo {\n    bytes listed = 100;\n  }");
        assert_eq!(
            eval_with(&extend, &[ENTRY]).findings,
            vec![
                invalid(0, &ENTRY, "field-not-found"),
                bytes_red(at + 1, "bytes listed = 100")
            ]
        );
        let (direct, _) = plant(&i, LISTED);
        assert!(eval_with(&direct, &[ENTRY]).findings.is_empty());
    }

    /// Token equality, not substrings (S3): prefix/suffix names red, and the
    /// exemption is exactly one STATEMENT (a second field on the same line
    /// stays red).
    #[test]
    fn near_miss_names_and_same_line_neighbours_stay_red() {
        let (i, _) = tree();
        let (i, line) = plant(
            &i,
            "  bytes listed = 90; bytes other = 93;\n  bytes listed_v2 = 91;\n  bytes x_listed = 92;",
        );
        assert_eq!(
            eval_with(&i, &[ENTRY]).findings,
            vec![
                bytes_red(line, "bytes other = 93"),
                bytes_red(line + 1, "bytes listed_v2 = 91"),
                bytes_red(line + 2, "bytes x_listed = 92"),
            ]
        );
    }

    /// One entry exempts exactly one declaration: two declarations of the
    /// listed name are invalid and both red. A duplicate list entry is
    /// invalid; the first copy still resolves.
    #[test]
    fn duplicate_declarations_and_duplicate_entries_are_invalid() {
        let (i, _) = tree();
        let (dup, line) = plant(&i, "  bytes listed = 90;\n  bytes listed = 91;");
        assert_eq!(
            eval_with(&dup, &[ENTRY]).findings,
            vec![
                invalid(line, &ENTRY, "duplicate-declaration"),
                bytes_red(line, "bytes listed = 90"),
                bytes_red(line + 1, "bytes listed = 91"),
            ]
        );
        let (one, _) = plant(&i, LISTED);
        assert_eq!(
            eval_with(&one, &[ENTRY, ENTRY]).findings,
            vec![invalid(0, &ENTRY, "duplicate-entry")]
        );
    }

    /// A good entry plus an invalid one reports exactly the invalid one.
    #[test]
    fn a_good_and_an_invalid_entry_report_only_the_invalid_one() {
        let (i, _) = tree();
        let (i, _) = plant(&i, LISTED);
        let stale = AllowedBytesField {
            message: "dark_tower.internal.v1.NoSuchMessage",
            field: "k",
        };
        let r = eval_with(&i, &[ENTRY, stale]);
        assert_eq!(r.findings, vec![invalid(0, &stale, "message-not-found")]);
        // Both components named, on the finding the GUARD returned.
        assert!(
            r.findings[0].subject.contains(stale.message)
                && r.findings[0].subject.contains(stale.field)
        );
        assert_eq!(r.key_allowlist_entries, 2);
    }

    /// Only check 1 consults the list (S5): check 2 still reds in the listed
    /// message, a `BytesValue` rpc header still reds, and a `guard:ignore`
    /// marker on an unlisted field is still not honoured.
    #[test]
    fn the_list_lifts_check_one_only() {
        let (i, _) = tree();
        let (i, line) = plant(
            &i,
            "  bytes listed = 90;\n  dark_tower.signaling.v1.MeetingKekUpdate u = 91;\n  bytes k = 92; // guard:ignore(needed)",
        );
        assert_eq!(
            eval_with(&i, &[ENTRY]).findings,
            vec![
                Finding {
                    rule: Rule::CrossPackageType,
                    line: line + 1,
                    subject: "dark_tower.signaling.v1.MeetingKekUpdate".into()
                },
                bytes_red(line + 2, "bytes k = 92"),
            ]
        );
        // A `BytesValue` rpc header with the list present: the FULL finding
        // set (header never exempt, its ref still a check-2 red).
        let (listed, _) = plant(&tree().0, LISTED);
        let probe = "  rpc Probe(google.protobuf.BytesValue) returns (RegisterMeetingRequest) { option deprecated = true; }";
        let header = listed.replacen(
            "service MediaHandlerService {",
            &format!("service MediaHandlerService {{\n{probe}"),
            1,
        );
        let hl = line_of(&header, probe);
        assert_eq!(
            eval_with(&header, &[ENTRY]).findings,
            vec![
                bytes_red(
                    hl,
                    "rpc Probe(google.protobuf.BytesValue) returns (RegisterMeetingRequest)"
                ),
                Finding {
                    rule: Rule::CrossPackageType,
                    line: hl,
                    subject: "google.protobuf.BytesValue".into()
                },
            ]
        );
    }

    /// Security SEC-1 (a) sibling top-level message and (b) nested message,
    /// each an unchanged-text neighbour on the listed field's line: exactly
    /// the neighbour reds. Positive arm: the listed field alone is green.
    #[test]
    fn identical_neighbours_in_sibling_or_nested_messages_stay_red() {
        let (base, _) = tree();
        for (decl, message, field, alone) in [
            (
                "message KeyHolder { bytes k = 1; } message Smuggle { bytes k = 1; }",
                "dark_tower.internal.v1.KeyHolder",
                "k",
                "message KeyHolder { bytes k = 1; }",
            ),
            (
                "message Outer { bytes j = 1; message Inner { bytes j = 1; } }",
                "dark_tower.internal.v1.Outer",
                "j",
                "message Outer { bytes j = 1; }",
            ),
        ] {
            let e = AllowedBytesField { message, field };
            let i = format!("{base}{decl}\n");
            let line = line_of(&i, decl);
            assert_eq!(
                eval_with(&i, &[e]).findings,
                vec![bytes_red(line, &format!("bytes {field} = 1"))],
                "{decl}"
            );
            let only = format!("{base}{alone}\n");
            assert!(eval_with(&only, &[e]).findings.is_empty(), "{alone}");
        }
    }

    /// Test F1: the exemption is keyed on statement IDENTITY, not content. An
    /// identical same-line declaration in ANOTHER message (here a top-level
    /// `Evil` opened on the listed field's line) is a second, unlisted
    /// `bytes` field and stays red. Positive arm: the listed-only plant
    /// (`a_listed_bytes_field_passes_and_the_same_tree_unlisted_reds`).
    #[test]
    fn an_identical_same_line_declaration_elsewhere_stays_red() {
        let (i, line) = plant(
            &tree().0,
            "  bytes listed = 90; } message Evil { bytes listed = 90;",
        );
        assert_eq!(
            eval_with(&i, &[ENTRY]).findings,
            vec![bytes_red(line, "bytes listed = 90")]
        );
    }

    /// Observability F2: the subject format and the closed reason set are
    /// pinned against LITERALS (the runbook's list), not rebuilt by the same
    /// `format!` the code uses.
    #[test]
    fn invalid_entry_subjects_are_the_documented_literals() {
        let (i, _) = tree();
        let (one, _) = plant(&i, LISTED);
        let (dup, _) = plant(&i, "  bytes listed = 90;\n  bytes listed = 91;");
        let (string, _) = plant(&i, "  string listed = 90;");
        let unknown = AllowedBytesField {
            message: "dark_tower.internal.v1.NoSuchMessage",
            field: "listed",
        };
        let subject = |src: &str, list: &[AllowedBytesField]| {
            eval_with(src, list)
                .findings
                .into_iter()
                .find(|f| f.rule == Rule::KeyAllowlistInvalid)
                .map(|f| f.subject)
                .expect("an invalid-entry finding")
        };
        let m = "(message=dark_tower.internal.v1.RegisterMeetingRequest, field=listed)";
        assert_eq!(
            subject(&one, &[unknown]),
            "(message=dark_tower.internal.v1.NoSuchMessage, field=listed): message-not-found"
        );
        assert_eq!(subject(&i, &[ENTRY]), format!("{m}: field-not-found"));
        assert_eq!(
            subject(&dup, &[ENTRY]),
            format!("{m}: duplicate-declaration")
        );
        assert_eq!(subject(&string, &[ENTRY]), format!("{m}: non-plain-bytes"));
        assert_eq!(
            subject(&one, &[ENTRY, ENTRY]),
            format!("{m}: duplicate-entry")
        );
        assert_eq!(
            m, "(message=dark_tower.internal.v1.RegisterMeetingRequest, field=listed)",
            "literal prefix"
        );
    }

    /// Observability F1: an entry finding with no declaration line points at
    /// the `ALLOWED_BYTES_FIELDS` const in this module; one with a declaration
    /// line points at internal.proto; other rules are unchanged.
    #[test]
    fn invalid_entry_location_points_at_the_const_or_the_declaration() {
        let (i, _) = tree();
        let r = eval_with(&i, &[ENTRY]);
        let (file, line) = location(&r.findings[0]);
        assert_eq!(file, file!());
        let src = real("crates/dt-guard/src/internal_proto_no_key_material.rs");
        assert!(
            src.lines().nth(line - 1).is_some_and(
                |l| l.starts_with("pub const ALLOWED_BYTES_FIELDS: &[AllowedBytesField] =")
            ),
            "the location is the const's own first line"
        );
        let (string, at) = plant(&i, "  string listed = 90;");
        let r = eval_with(&string, &[ENTRY]);
        assert_eq!(location(&r.findings[0]), (INTERNAL_PROTO, at));
        let (bytes, at) = plant(&i, "  bytes k = 90;");
        let r = eval_with(&bytes, &[]);
        assert_eq!(location(&r.findings[0]), (INTERNAL_PROTO, at));
    }

    /// Security SEC-2: an id that does not name the declaration in the real
    /// scopes (walker drift) exempts NOTHING and is vacuity; the agreeing id
    /// is the positive arm.
    #[test]
    fn a_drifted_declaration_id_exempts_nothing() {
        let (i, _) = tree();
        let (i, _) = plant(&i, LISTED);
        let tokens = lex(&i).unwrap();
        let sc = scopes(&tokens);
        let good = fields(&tokens);
        let (exempt, findings) = resolve_key_allowlist(&sc, &good, &[ENTRY]);
        assert_eq!(exempt.len(), 1);
        assert!(findings.is_empty(), "{findings:?}");
        for (dscope, dstmt) in [(1usize, 0usize), (0, 1)] {
            let drifted: Vec<Field> = good
                .iter()
                .cloned()
                .map(|mut f| {
                    if f.message == LISTED_MSG && f.name == "listed" {
                        f.scope += dscope;
                        f.statement += dstmt;
                    }
                    f
                })
                .collect();
            let (exempt, findings) = resolve_key_allowlist(&sc, &drifted, &[ENTRY]);
            assert!(exempt.is_empty(), "({dscope}, {dstmt})");
            assert_eq!(
                findings.iter().map(|f| f.rule).collect::<Vec<_>>(),
                vec![Rule::Vacuous],
                "({dscope}, {dstmt})"
            );
            assert!(findings[0]
                .subject
                .contains("(message=dark_tower.internal.v1.RegisterMeetingRequest, field=listed)"));
        }
    }
}
