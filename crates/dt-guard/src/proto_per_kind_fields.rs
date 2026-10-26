//! `proto-per-kind-fields`: no proto FIELD may encode a media kind in its name
//! (docs/protocol/CONVENTIONS.md §6, ADR-0036 §5 amendment 2026-10-25).
//!
//! Every per-kind fact on the wire is a repeated entry keyed by
//! `MediaKind media_kind`. A hand-named `audio_*`/`video_*` pair encodes the
//! kind in the field name, so a new kind becomes a schema change at every site
//! and a consumer can read one half and forget the other.
//!
//! Policy content (the vocabulary's synonym list, the allow-list and its
//! reasons): @protocol. Machinery (lexing, scope tracking, rule ids): @infrastructure.
//!
//! # The rule
//!
//! Over every `proto/**/*.proto`, a FIELD DECLARATION (message field, oneof
//! member, map field; nested messages included) fails when any `_`-token of
//! its name -- camelCase humps split too -- is in the vocabulary. Tokens, not
//! substrings: `microsecond` does not hit `mic`, `audiolevel` is one token.
//!
//! - `reserved` statements are not field declarations, so reserved names are
//!   exempt STRUCTURALLY (the recogniser never sees them), never by a name
//!   list -- reserved names are history and must stay greppable.
//! - Enum values (`MEDIA_KIND_VIDEO_CAMERA` IS the key), message and enum type
//!   names, options and comments are out of scope.
//!
//! # The vocabulary is DERIVED
//!
//! From the `MediaKind` enum found in `proto/`: the lower-cased `_`-tokens of
//! every value after stripping `MEDIA_KIND_`, `UNSPECIFIED` excluded (today
//! {audio, video, camera, screen}), plus [`SYNONYMS`]. A new kind is covered
//! with zero guard edits. An enum that cannot be found, or yields no tokens,
//! is VACUITY -- never "no vocabulary, no findings".
//!
//! # Rule ids
//!
//! - `proto-per-kind-field`: the content violation.
//! - `proto-per-kind-scan-vacuous`: the guard could not evaluate its input (no
//!   proto files, an empty file, a lexer refusal -- a `/*` block comment, a
//!   single quote, unbalanced braces -- no `MediaKind` vocabulary, a file with
//!   no recognised field, or no `reserved` statement anywhere). The real-tree
//!   positive control is the test `the_real_proto_tree_is_clean_and_non_vacuous`.
//! - `proto-per-kind-stale-allowlist`: an [`ALLOW`] entry suppresses nothing
//!   (no such field, or the field no longer hits).
//! - `proto-per-kind-allowlist-reason`: an [`ALLOW`] entry's reason is missing
//!   or shorter than [`MIN_REASON_LEN`].
//!
//! No suppression path but [`ALLOW`], which ships EMPTY.

use crate::common::path_safety::to_repo_relative;
use crate::common::proto_lex::{lex, Token};
use crate::common::status::{emit_ok, emit_scope};
use anyhow::{bail, Result};
use std::collections::BTreeSet;
use std::path::Path;

/// Explicit synonyms the enum cannot yield (@protocol policy).
pub const SYNONYMS: &[&str] = &["mic", "microphone", "cam"];

/// Reasoned exceptions: `(fully-qualified field, reason)`, e.g.
/// `("dark_tower.signaling.v1.Message.field", "why this is not a per-kind fact")`.
/// Adding an entry is a policy change with @protocol. EMPTY by design.
pub const ALLOW: &[AllowEntry] = &[];

/// The shortest acceptable allow-list reason.
pub const MIN_REASON_LEN: usize = 10;

/// One allow-list entry: `(fully-qualified field, reason)`.
pub type AllowEntry = (&'static str, &'static str);

/// A finding's rule. Exhaustive: every printer and token match has no
/// wildcard (code-quality CR-6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    /// A field name encodes a media kind.
    Field,
    /// The guard could not evaluate its input.
    Vacuous,
    /// An allow-list entry suppresses nothing.
    StaleAllowlist,
    /// An allow-list entry lacks a reason.
    AllowlistReason,
}

impl Rule {
    /// The stable rule id (the `VIOLATION [...]` / `REASON=` token).
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Field => "proto-per-kind-field",
            Self::Vacuous => "proto-per-kind-scan-vacuous",
            Self::StaleAllowlist => "proto-per-kind-stale-allowlist",
            Self::AllowlistReason => "proto-per-kind-allowlist-reason",
        }
    }
}

/// The kind vocabulary (lower-case tokens).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Vocabulary(pub BTreeSet<String>);

/// One finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// The rule id.
    pub rule: Rule,
    /// Root-relative file (empty for tree-level findings).
    pub file: String,
    /// 1-based line (0 = file- or tree-level).
    pub line: usize,
    /// The fully-qualified field, or the vacuity/allow-list detail.
    pub subject: String,
}

/// What one evaluation saw.
#[derive(Debug, Default)]
pub struct Report {
    /// Every finding, vacuity first.
    pub findings: Vec<Finding>,
    /// Field declarations recognised.
    pub fields_scanned: usize,
    /// `reserved` statements seen (structurally exempt).
    pub reserved_seen: usize,
    /// Files evaluated.
    pub files: usize,
}

fn vacuous(file: &str, line: usize, subject: impl Into<String>) -> Finding {
    Finding {
        rule: Rule::Vacuous,
        file: file.to_string(),
        line,
        subject: subject.into(),
    }
}

/// Split a field name into lower-case tokens: on `_`, and on camelCase humps.
#[must_use]
pub fn name_tokens(name: &str) -> Vec<String> {
    let mut out = Vec::new();
    for part in name.split('_').filter(|p| !p.is_empty()) {
        let mut cur = String::new();
        let mut prev_lower = false;
        for c in part.chars() {
            if c.is_ascii_uppercase() && prev_lower && !cur.is_empty() {
                out.push(cur.to_ascii_lowercase());
                cur.clear();
            }
            prev_lower = c.is_ascii_lowercase() || c.is_ascii_digit();
            cur.push(c);
        }
        if !cur.is_empty() {
            out.push(cur.to_ascii_lowercase());
        }
    }
    out
}

/// The recognised field name of a statement inside a message or oneof, if it
/// is a field declaration. Excludes `option`, `reserved`, `extensions`,
/// `syntax`, `package`, `import` and everything that is not
/// `[label] <type|map<K,V>> <name> = <number> [options]`.
#[must_use]
pub fn field_name(statement: &str) -> Option<&str> {
    let first = statement.split(' ').next().unwrap_or("");
    if matches!(
        first,
        "option" | "reserved" | "extensions" | "syntax" | "package" | "import" | "extend"
    ) {
        return None;
    }
    let (lhs, rhs) = statement.split_once('=')?;
    let number = rhs.trim().split([' ', '[']).next()?;
    // A proto intLit: decimal, `0x` hex or octal -- first char a digit, the
    // rest alphanumeric (infra F2: hex numbers must not slip past).
    if !number.chars().next().is_some_and(|c| c.is_ascii_digit())
        || !number.chars().all(|c| c.is_ascii_alphanumeric())
    {
        return None;
    }
    let lhs = lhs.trim();
    let name = lhs.rsplit(' ').next()?;
    let ty = lhs.strip_suffix(name).unwrap_or("").trim();
    let ty = ty
        .strip_prefix("optional ")
        .or_else(|| ty.strip_prefix("repeated "))
        .or_else(|| ty.strip_prefix("required "))
        .unwrap_or(ty);
    let valid_ident = |s: &str| {
        s.chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    };
    let valid_type = ty.starts_with("map<") && ty.ends_with('>')
        || (!ty.is_empty()
            && ty
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.'));
    (valid_ident(name) && valid_type).then_some(name)
}

/// Derive the vocabulary from every `enum MediaKind` in `files`.
#[must_use]
pub fn derive_vocabulary(files: &[(String, String)]) -> Vocabulary {
    let mut vocab = BTreeSet::new();
    for (_, content) in files {
        let Ok(tokens) = lex(content) else { continue };
        let mut depth_in_enum: Option<usize> = None;
        let mut depth = 0usize;
        for t in &tokens {
            match t {
                Token::Open { header, .. } => {
                    depth += 1;
                    if depth_in_enum.is_none() && header == "enum MediaKind" {
                        depth_in_enum = Some(depth);
                    }
                }
                Token::Close { .. } => {
                    if depth_in_enum == Some(depth) {
                        depth_in_enum = None;
                    }
                    depth = depth.saturating_sub(1);
                }
                Token::Statement { text, .. } if depth_in_enum == Some(depth) => {
                    let Some((value, _)) = text.split_once('=') else {
                        continue;
                    };
                    let value = value.trim();
                    let Some(kind) = value.strip_prefix("MEDIA_KIND_") else {
                        continue;
                    };
                    if kind == "UNSPECIFIED" {
                        continue;
                    }
                    vocab.extend(
                        kind.split('_')
                            .filter(|s| !s.is_empty())
                            .map(str::to_ascii_lowercase),
                    );
                }
                Token::Statement { .. } => {}
            }
        }
    }
    if !vocab.is_empty() {
        vocab.extend(SYNONYMS.iter().map(|s| (*s).to_string()));
    }
    Vocabulary(vocab)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Scope {
    Message,
    Oneof,
    Other,
}

/// Evaluate `files` (root-relative path, content) against `vocab` and
/// `allow`. The test seams are these parameters: `run` passes the derived
/// vocabulary and [`ALLOW`].
#[must_use]
pub fn check_with(files: &[(String, String)], vocab: &Vocabulary, allow: &[AllowEntry]) -> Report {
    let mut report = Report {
        files: files.len(),
        ..Report::default()
    };
    if files.is_empty() {
        report
            .findings
            .push(vacuous("proto", 0, "no proto files under proto/"));
        return report;
    }
    if vocab.0.is_empty() {
        report.findings.push(vacuous(
            "proto",
            0,
            "vocabulary-empty: no `enum MediaKind` with a non-UNSPECIFIED value found",
        ));
    }
    let mut hits: Vec<Finding> = Vec::new();
    for (file, content) in files {
        if content.trim().is_empty() {
            report.findings.push(vacuous(file, 0, "empty proto file"));
            continue;
        }
        let tokens = match lex(content) {
            Ok(t) => t,
            Err(e) => {
                report.findings.push(vacuous(file, 0, e.to_string()));
                continue;
            }
        };
        let mut package = String::new();
        let mut stack: Vec<(Scope, String)> = Vec::new();
        let mut file_fields = 0usize;
        for t in &tokens {
            match t {
                Token::Open { header, .. } => {
                    let mut words = header.split(' ');
                    let entry = match (words.next(), words.next()) {
                        (Some("message"), Some(name)) => (Scope::Message, name.to_string()),
                        (Some("oneof"), Some(name)) => (Scope::Oneof, name.to_string()),
                        _ => (Scope::Other, String::new()),
                    };
                    stack.push(entry);
                }
                Token::Close { .. } => {
                    stack.pop();
                }
                Token::Statement { line, text } => {
                    if stack.is_empty() {
                        if let Some(p) = text.strip_prefix("package ") {
                            package = p.trim().to_string();
                        }
                        continue;
                    }
                    if text.starts_with("reserved ") {
                        report.reserved_seen += 1;
                        continue;
                    }
                    let in_field_scope = matches!(
                        stack.last().map(|s| s.0),
                        Some(Scope::Message | Scope::Oneof)
                    );
                    if !in_field_scope {
                        continue;
                    }
                    let Some(name) = field_name(text) else {
                        continue;
                    };
                    file_fields += 1;
                    let mut path: Vec<&str> = Vec::new();
                    if !package.is_empty() {
                        path.push(&package);
                    }
                    path.extend(
                        stack
                            .iter()
                            .filter(|(s, _)| *s == Scope::Message)
                            .map(|(_, n)| n.as_str()),
                    );
                    path.push(name);
                    let fq = path.join(".");
                    if name_tokens(name).iter().any(|tok| vocab.0.contains(tok)) {
                        hits.push(Finding {
                            rule: Rule::Field,
                            file: file.clone(),
                            line: *line,
                            subject: fq,
                        });
                    }
                }
            }
        }
        report.fields_scanned += file_fields;
        if file_fields == 0 {
            report
                .findings
                .push(vacuous(file, 0, "no field declarations recognised"));
        }
    }
    if report.reserved_seen == 0 {
        report.findings.push(vacuous(
            "proto",
            0,
            "no `reserved` statement recognised anywhere (the structural exemption is untested)",
        ));
    }
    for (fq, reason) in allow {
        if reason.trim().len() < MIN_REASON_LEN {
            report.findings.push(Finding {
                rule: Rule::AllowlistReason,
                file: String::new(),
                line: 0,
                subject: (*fq).to_string(),
            });
        }
        if !hits.iter().any(|h| h.subject == *fq) {
            report.findings.push(Finding {
                rule: Rule::StaleAllowlist,
                file: String::new(),
                line: 0,
                subject: (*fq).to_string(),
            });
        }
    }
    let allowed: BTreeSet<&str> = allow.iter().map(|(fq, _)| *fq).collect();
    report.findings.extend(
        hits.into_iter()
            .filter(|h| !allowed.contains(h.subject.as_str())),
    );
    report
}

/// Every `proto/**/*.proto` under `root`, root-relative and sorted, with its
/// content.
///
/// # Errors
///
/// An unreadable file.
pub fn proto_files(root: &Path) -> Result<Vec<(String, String)>> {
    let mut out = Vec::new();
    let base = root.join("proto");
    if !base.is_dir() {
        return Ok(out);
    }
    for entry in walkdir::WalkDir::new(&base).sort_by_file_name() {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type().is_file() && path.extension().is_some_and(|e| e == "proto") {
            let rel = to_repo_relative(root, path).replace('\\', "/");
            out.push((rel, std::fs::read_to_string(path)?));
        }
    }
    Ok(out)
}

/// Run the guard over `root`.
///
/// # Errors
///
/// Any finding (the token names the first rule), or an unreadable file.
pub fn run(root: &Path, explain: bool) -> Result<()> {
    let files = proto_files(root)?;
    let vocab = derive_vocabulary(&files);
    let report = check_with(&files, &vocab, ALLOW);
    emit_scope(format!(
        "{} proto files, {} fields, {} reserved statements, {} vocabulary tokens, {} allow-list \
         entries, {} findings",
        report.files,
        report.fields_scanned,
        report.reserved_seen,
        vocab.0.len(),
        ALLOW.len(),
        report.findings.len()
    ));
    if report.findings.is_empty() {
        emit_ok("proto-per-kind-fields-clean");
        return Ok(());
    }
    let mut ordered: Vec<&Finding> = report.findings.iter().collect();
    ordered.sort_by_key(|f| usize::from(f.rule != Rule::Vacuous));
    for f in &ordered {
        match f.rule {
            Rule::Field => println!(
                "VIOLATION [{}] {}:{}: field `{}` encodes a media kind in its name. Key the \
                 fact by kind instead: `repeated <Entry> ... = N;` with `MediaKind media_kind = 1;` \
                 in the entry (docs/protocol/CONVENTIONS.md §6). A reasoned exception is a policy \
                 change with @protocol (`ALLOW` in crates/dt-guard/src/proto_per_kind_fields.rs).",
                f.rule.id(),
                f.file,
                f.line,
                f.subject
            ),
            Rule::Vacuous => println!(
                "ERROR: PRECONDITION [{}] {}: {} -- the guard could not evaluate its \
                 input, so it refuses to report clean. NOT a content violation: restore the \
                 input, do not loosen the rule.",
                f.rule.id(),
                f.file,
                f.subject
            ),
            Rule::StaleAllowlist => println!(
                "VIOLATION [{}] allow-list entry `{}` suppresses nothing (no such \
                 field, or the field no longer hits): remove it (with @protocol).",
                f.rule.id(),
                f.subject
            ),
            Rule::AllowlistReason => println!(
                "VIOLATION [{}] allow-list entry `{}` has no reason of at least \
                 {MIN_REASON_LEN} characters.",
                f.rule.id(),
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

    const KINDS: &str = "syntax = \"proto3\";\npackage t.v1;\n\
        enum MediaKind {\n  MEDIA_KIND_UNSPECIFIED = 0;\n  MEDIA_KIND_AUDIO = 1;\n\
        \x20 MEDIA_KIND_VIDEO_CAMERA = 2;\n  MEDIA_KIND_VIDEO_SCREEN = 3;\n}\n";

    fn tree(body: &str) -> Vec<(String, String)> {
        vec![(
            "proto/t.proto".to_string(),
            format!("{KINDS}message Holder {{\n  reserved 99;\n  int32 ok = 98;\n}}\n{body}"),
        )]
    }

    fn eval(body: &str) -> Report {
        let files = tree(body);
        let vocab = derive_vocabulary(&files);
        check_with(&files, &vocab, &[])
    }

    fn field_hits(r: &Report) -> Vec<(usize, String)> {
        r.findings
            .iter()
            .filter(|f| f.rule == Rule::Field)
            .map(|f| (f.line, f.subject.clone()))
            .collect()
    }

    #[test]
    fn the_vocabulary_is_derived_from_media_kind_plus_synonyms() {
        let v = derive_vocabulary(&tree(""));
        let got: Vec<&str> = v.0.iter().map(String::as_str).collect();
        assert_eq!(
            got,
            [
                "audio",
                "cam",
                "camera",
                "mic",
                "microphone",
                "screen",
                "video"
            ]
        );
    }

    /// Red fixtures: each asserts the SPECIFIC finding (line + fq field).
    #[test]
    fn prefix_suffix_infix_bare_camel_synonym_hit_with_their_field() {
        let body = "message M {\n\
            bool audio_muted = 1;\n\
            bool request_video = 2;\n\
            uint32 max_audio_receive_slots = 3;\n\
            bool audio = 4;\n\
            bool audioLevel = 5;\n\
            uint32 mic_level = 6;\n\
            string note = 7 [json_name = \"a//b\"]; bool camera_on = 8;\n\
            bool video_hex = 0x9;\n\
            }\n";
        let r = eval(body);
        let base = 13; // the line `message M {` lands on in the fixture
        assert_eq!(
            field_hits(&r),
            vec![
                (base + 1, "t.v1.M.audio_muted".to_string()),
                (base + 2, "t.v1.M.request_video".to_string()),
                (base + 3, "t.v1.M.max_audio_receive_slots".to_string()),
                (base + 4, "t.v1.M.audio".to_string()),
                (base + 5, "t.v1.M.audioLevel".to_string()),
                (base + 6, "t.v1.M.mic_level".to_string()),
                // Infra F1: a `//` inside a string must not swallow the field.
                (base + 7, "t.v1.M.camera_on".to_string()),
                // Infra F2: a hex field number is still a field.
                (base + 8, "t.v1.M.video_hex".to_string()),
            ]
        );
    }

    #[test]
    fn oneof_map_nested_and_multi_line_declarations_hit() {
        let body = "message M {\n\
            oneof o {\n  bool screen_on = 1;\n}\n\
            map<string, int32> camera_by_id = 2;\n\
            message Inner {\n  bool video_x = 3;\n}\n\
            repeated\n  int32\n    audio_ids\n  = 4;\n\
            }\n";
        let subjects: Vec<String> = field_hits(&eval(body)).into_iter().map(|h| h.1).collect();
        assert_eq!(
            subjects,
            [
                "t.v1.M.screen_on",
                "t.v1.M.camera_by_id",
                "t.v1.M.Inner.video_x",
                "t.v1.M.audio_ids"
            ]
        );
    }

    /// Green fixtures: structural exemptions and token (not substring)
    /// semantics.
    #[test]
    fn reserved_enum_values_comments_and_non_tokens_do_not_hit() {
        let body = "message M {\n\
            reserved 7;\n  reserved \"audio_muted\", \"video_muted\";\n\
            // bool audio_muted = 9;\n\
            uint64 microsecond = 1;\n  bool audiolevel = 2;\n  repeated string videos = 3;\n\
            MediaKind media_kind = 4;\n\
            }\nenum E {\n  E_AUDIO = 0;\n}\n";
        let r = eval(body);
        assert!(field_hits(&r).is_empty(), "{:?}", r.findings);
        assert!(r.findings.is_empty(), "{:?}", r.findings);
        // Positive control (test finding 6): every green row was SEEN and
        // passed, not missed -- Holder's `ok` plus `microsecond`, `audiolevel`,
        // `videos`, `media_kind` (the commented field is not counted), and
        // Holder's `reserved 99` plus the two here.
        assert_eq!(r.fields_scanned, 5);
        assert_eq!(r.reserved_seen, 3);
    }

    #[test]
    fn an_allow_listed_existing_field_passes_and_stale_or_reasonless_entries_fail() {
        let files = tree("message M {\n  bool audio_muted = 1;\n}\n");
        let vocab = derive_vocabulary(&files);
        let ok = check_with(
            &files,
            &vocab,
            &[("t.v1.M.audio_muted", "a documented exception")],
        );
        assert!(ok.findings.is_empty(), "{:?}", ok.findings);

        let stale = check_with(&files, &vocab, &[("t.v1.M.gone", "a documented exception")]);
        assert!(stale
            .findings
            .iter()
            .any(|f| f.rule == Rule::StaleAllowlist && f.subject == "t.v1.M.gone"));
        assert!(stale.findings.iter().any(|f| f.rule == Rule::Field));

        // Infra F5: an entry for an EXISTING field that no longer hits is
        // stale too -- it suppresses nothing.
        let quiet = tree("message M {\n  bool on = 1;\n}\n");
        let r = check_with(
            &quiet,
            &derive_vocabulary(&quiet),
            &[("t.v1.M.on", "a documented exception")],
        );
        assert!(r
            .findings
            .iter()
            .any(|f| f.rule == Rule::StaleAllowlist && f.subject == "t.v1.M.on"));

        let short = check_with(&files, &vocab, &[("t.v1.M.audio_muted", "short")]);
        assert!(short
            .findings
            .iter()
            .any(|f| f.rule == Rule::AllowlistReason && f.subject == "t.v1.M.audio_muted"));
    }

    #[test]
    fn a_missing_enum_a_block_comment_and_no_files_are_vacuous() {
        let no_enum = vec![(
            "proto/x.proto".to_string(),
            "message M {\n  reserved 1;\n  bool audio_muted = 2;\n}\n".to_string(),
        )];
        let v = derive_vocabulary(&no_enum);
        assert!(v.0.is_empty());
        let r = check_with(&no_enum, &v, &[]);
        assert!(r
            .findings
            .iter()
            .any(|f| f.rule == Rule::Vacuous && f.subject.starts_with("vocabulary-empty")));

        // Infra F4: assert the SPECIFIC subject. A second file carries the
        // enum, so `vocabulary-empty` cannot satisfy these on its own.
        let with_enum = |name: &str, body: &str| {
            let mut files = tree("");
            files.push((name.to_string(), body.to_string()));
            files
        };
        let vacuous_about = |files: &[(String, String)], needle: &str| {
            let r = check_with(files, &derive_vocabulary(files), &[]);
            assert!(
                r.findings
                    .iter()
                    .any(|f| f.rule == Rule::Vacuous && f.subject.contains(needle)),
                "{needle}: {:?}",
                r.findings
            );
        };
        vacuous_about(
            &with_enum(
                "proto/b.proto",
                "/* message Hidden { bool audio = 1; } */\n",
            ),
            "block comment",
        );
        vacuous_about(&with_enum("proto/e.proto", "  \n"), "empty proto file");
        // Infra F1 as re-ruled: a single-quoted string is refused, never
        // mis-read (its `//` would otherwise swallow `camera_on`).
        vacuous_about(
            &with_enum(
                "proto/q.proto",
                "message M {\n  string n = 1 [json_name = 'a//b']; bool camera_on = 2;\n}\n",
            ),
            "single quote",
        );
        vacuous_about(
            &with_enum("proto/u.proto", "message M {\n  bool a = 1;\n}\n}\n"),
            "unbalanced braces",
        );
        vacuous_about(
            &with_enum("proto/o.proto", "message M {\n  bool a = 1;\n"),
            "unbalanced braces",
        );

        let r = check_with(&[], &Vocabulary::default(), &[]);
        assert_eq!(r.findings.len(), 1);
        assert_eq!(r.findings[0].rule, Rule::Vacuous);
    }

    /// The neutered-matcher proof: with an EMPTY vocabulary passed through the
    /// seam, the red fixtures produce no content finding -- so a broken
    /// matcher is detectable by the red tests above.
    #[test]
    fn red_fixtures_go_green_with_the_vocabulary_neutered() {
        let files = tree("message M {\n  bool audio_muted = 1;\n  bool request_video = 2;\n}\n");
        let neutered = check_with(&files, &Vocabulary::default(), &[]);
        assert!(neutered.findings.iter().all(|f| f.rule != Rule::Field));
        let armed = check_with(&files, &derive_vocabulary(&files), &[]);
        assert_eq!(
            armed
                .findings
                .iter()
                .filter(|f| f.rule == Rule::Field)
                .count(),
            2
        );
    }

    #[test]
    fn field_recogniser_excludes_non_fields() {
        for s in [
            "option java_package = \"x\"",
            "reserved 1, 2",
            "reserved \"audio\"",
            "extensions 100 to 199",
            "syntax = \"proto3\"",
            "MEDIA_KIND_AUDIO = 1",
            "rpc Foo(Bar) returns (Baz)",
        ] {
            assert_eq!(field_name(s), None, "{s}");
        }
        assert_eq!(field_name("optional uint32 a = 13"), Some("a"));
        assert_eq!(
            field_name("map<string, Foo> m = 2 [deprecated = true]"),
            Some("m")
        );
        assert_eq!(field_name("google.protobuf.Timestamp t = 3"), Some("t"));
        assert_eq!(field_name("bool a = 0x2"), Some("a"));
        assert_eq!(field_name("bool a = 017"), Some("a"));
    }

    #[test]
    fn name_tokens_split_on_underscore_and_humps() {
        assert_eq!(
            name_tokens("max_audio_receive_slots"),
            ["max", "audio", "receive", "slots"]
        );
        assert_eq!(name_tokens("audioLevel"), ["audio", "level"]);
        assert_eq!(name_tokens("audiolevel"), ["audiolevel"]);
    }

    /// The real tree is GREEN, non-vacuous: fields scanned, reserved
    /// statements seen (in signaling.proto among others), the derived
    /// vocabulary present.
    #[test]
    fn the_real_proto_tree_is_clean_and_non_vacuous() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let files = proto_files(&root).unwrap();
        assert!(files.iter().any(|(f, _)| f.ends_with("signaling.proto")));
        let vocab = derive_vocabulary(&files);
        assert!(vocab.0.contains("audio") && vocab.0.contains("screen"));
        let r = check_with(&files, &vocab, ALLOW);
        assert!(r.findings.is_empty(), "{:?}", r.findings);
        assert!(r.fields_scanned > 0);
        let signaling = files
            .iter()
            .filter(|(f, _)| f.ends_with("signaling.proto"))
            .cloned()
            .collect::<Vec<_>>();
        assert!(check_with(&signaling, &vocab, &[]).reserved_seen > 0);
    }
}
