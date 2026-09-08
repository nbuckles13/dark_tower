//! `media-telemetry-deny` subcommand — ADR-0036 §11's media-path telemetry
//! deny, scoped to a configured directory list.
//!
//! # What §11 asks for, and why the scope is a directory
//!
//! > *"Enforcement is a deny of log and metric macros scoped to a directory,
//! > covering the whole media path — not a file list, which narrows silently
//! > on refactor while the guard keeps passing. This requires one layout
//! > constraint: lifecycle, setup, and teardown are **siblings** of the media
//! > directory, not children, so the directory boundary and the hot-path
//! > boundary are the same boundary. Deny the event macro too, since level
//! > macros expand to it, and **allow** cached-handle record and increment
//! > calls — the guard denies macro forms and must not touch handle methods,
//! > or it bans the pattern it exists to enforce."*
//!
//! Everything below follows from those four sentences — **except the SPAN and
//! `#[instrument]` families, which derive from a different clause of §11 and
//! are argued separately in the next section.** That caveat is load-bearing:
//! those four sentences name log macros, metric macros and `event!` and do
//! not mention spans, so an unqualified "everything below follows" is a
//! standing invitation to a future reader to trim two denied families as
//! scope creep. Read the next section before removing them.
//!
//! # SPAN and `#[instrument]`: §11's own invariant, NOT an extension
//!
//! This section exists because its absence was the defect. Do not delete it
//! as commentary; it is the argument a reader has to answer before narrowing
//! this guard.
//!
//! **On the leak axis it is not an extension at all.** §11's retention bullet
//! names the surface directly: *"No per-frame, per-participant, or
//! per-stream-identity dimension in media-path logs, metric labels, **or span
//! attributes**."* Span attributes are enumerated alongside logs and metric
//! labels in the ADR's own text.
//!
//! **On the cost axis the argument has to be made rather than cited, because
//! the objection is specific.** §11 states the per-frame invariant as *"zero
//! allocation and zero registry lookup"*, and *"no metric macro is reachable
//! from the forward function"*. A reader will object: the level is off in
//! production, the subscriber discards it, so a `debug_span!` costs nothing.
//! That is wrong on the facts and, more importantly, wrong in kind.
//!
//! * **Disabled is not free, and what you still pay is the thing §11 names.**
//!   `tracing` does short-circuit: `span!`/`event!` put the `valueset!`
//!   holding your field expressions INSIDE the enabled arm, so a disabled
//!   callsite really does skip evaluating them and construct a `Span::none()`.
//!   What it does not skip is the guard: a `level_enabled!` check, then
//!   `CALLSITE.interest()`, then — when interest is `sometimes` — a
//!   `Dispatch::current()` lookup. **That interest/dispatch check IS the
//!   "registry lookup" §11 forbids per frame**, in the ADR's own words.
//!   (An earlier draft of this paragraph claimed the field expressions are
//!   evaluated regardless of interest. That is FALSE, and it is recorded here
//!   rather than quietly deleted because the trimmer this section must answer
//!   is precisely the reader who knows it — a refutable premise here would
//!   discredit the whole cost axis and license the trim again.)
//! * **Enabled, you pay the allocation and the field recording, per frame** —
//!   anything non-`Copy` in the field list (a formatted stream id, a `String`,
//!   a `%`- or `?`-rendered value). **And enablement is not yours to assume:**
//!   it is a runtime configuration this guard cannot see, which is the whole
//!   of the next bullet.
//! * **`#[instrument]` is worse per frame precisely because it is invisible
//!   at the call site**: it wraps every call in span construction plus
//!   enter/exit and, absent `skip`/`skip_all`, records **every function
//!   argument** as a field. On a forward function those arguments are the
//!   connection identity, the stream and the frame — the exact per-frame,
//!   per-participant value set §11 exists to contain, entering span
//!   attributes by default rather than by mistake.
//! * **"The level is off" is a runtime-configuration defence for a per-frame
//!   invariant**, and it is the defence §11 has already rejected in terms:
//!   *"A log level is not an acceptable gate. The incident motivating a level
//!   change is the same incident producing the sensitive trace."*
//!
//! **The closing move, which is the one a trimmer must answer.** Denying the
//! level macros while allowing span forms would make this guard's coverage
//! depend on a subscriber configuration the guard cannot see. That is not a
//! narrower policy — it is a policy with a runtime escape hatch, in a control
//! whose whole design principle (§11, and the ADR's coverage section) is
//! structural impossibility over a control that has to notice.
//!
//! # The allow-list is satisfied BY CONSTRUCTION, not by subtraction
//!
//! §11's allow requirement — `.increment` / `.record` / `.set` / `.absolute`
//! / `.decrement` must not be flagged — is met because every matcher here
//! anchors on `<name>!` followed by a delimiter. `.record(x)` structurally
//! cannot match `record!(`. [`ALLOWED_HANDLE_METHODS`] is documentation plus
//! a test, **never a line filter**: a filter that dropped lines containing
//! `.increment(` would suppress a real `counter!(...)` co-located with a
//! handle call on the same line, which is a masking bug (CLAUDE.md §Fail
//! loudly).
//!
//! This is not hypothetical. `crates/mh-service/src/media/` already calls
//! `.increment(1)`, `.set(..)` and `.record(..)` on cached handles today, so
//! a guard that touched handle methods would red the real tree on its first
//! run — §11's "it bans the pattern it exists to enforce", demonstrated
//! rather than asserted.
//!
//! # Comments and string literals are out of scope, and this is not optional
//!
//! `crates/mh-service/src/media/mod.rs` documents its own ban in prose and
//! therefore contains `counter!`, `histogram!`, `gauge!`, `describe_*!`,
//! `println!`, `eprintln!`, `dbg!`, `event!`, span macros and `#[instrument]`
//! as *text*; `ingress.rs` and `forward.rs` do the same, and `forward.rs`
//! carries ``/// ... a `debug!(?frame)` in``, which matches the invocation
//! anchor exactly. **A raw line matcher reds the real tree on its first run.**
//! Every matcher therefore runs over
//! [`crate::common::test_code_filter::blank_non_code`] output.
//!
//! # STATUS lane: both scope failures are the IMPLEMENTER lane (FAIL, exit 1)
//!
//! Deliberate, and deliberately different from the frame-vectors and
//! `release_build_profile` precedents, which route vacuity to
//! `PRECONDITION_FAILURE`. Three reasons:
//!
//! 1. **Substantive.** Their vacuity means the vector file or the Dockerfile
//!    set is absent — a wrong-root / wrong-checkout *machine* fact that a
//!    retry can fix. Ours means someone renamed or emptied
//!    `crates/mh-service/src/media/` **in this diff** and did not update the
//!    manifest. That is a diff defect with a named fixer and a one-line fix.
//!    Routing it to the operator lane burns a retry on a deterministic
//!    failure and then pages operations for a code change.
//!
//! 2. **Mechanical — the operator lane is not available to a `simple/`
//!    guard.** `scripts/guards/run-guards.sh::classify_guard_exit`
//!    classifies on the exit *value*: only 124 and 137 reach
//!    `PRECONDITION_GUARDS`; every other non-zero, **including 2**, falls
//!    into the `*)` arm and is counted as a violation. A guard exiting 2
//!    would print operator-lane text while being counted implementer-lane —
//!    the STATUS line and the aggregation would disagree, which is worse
//!    than either lane. No `simple/` guard emits `PRECONDITION_FAILURE`
//!    today.
//!
//! 3. **The token is the lane.** A guard subprocess cannot self-declare a
//!    lane; the exit code and the reason token carry it.
//!
//! The `ERROR: PRECONDITION [<token>]` prefix is reused from
//! `release_build_profile` for consistency — inventing a second vocabulary
//! for this class in one guard would itself be the drift — but the body
//! leads with **"THIS IS A DIFF DEFECT, not a machine fault"**, because the
//! neighbouring guard's rows say the opposite and a reader who pattern-matches
//! the prefix will otherwise re-run on a quiet machine forever.
//!
//! # Output carries the macro spelling and the location. Never the arguments.
//!
//! The identifiers §11 exists to keep out of logs live inside the denied
//! macro's *argument list* — `info!(participant_id = p, stream = s, ...)`.
//! A guard that echoed the matched span to stdout would take that identifier
//! out of a process-local log and put it into CI output, build logs and
//! artifact retention: **the control reproducing the leak it exists to
//! prevent, on a worse surface, in the moment it reports success.**
//!
//! So findings emit the macro spelling plus `path:line` only, and `--explain`
//! uses [`crate::common::explain::SecretFinding`], which has **no `matched`
//! field by design** and carries a construction-based assertion blocking its
//! reintroduction. Compile-time impossibility over a convention.
//!
//! # No suppression, at all
//!
//! No `guard:ignore` marker, no environment bypass, no flag. This module
//! deliberately does not import [`crate::ignore`]. §11's whole argument is
//! structural impossibility over "a control that has to notice", and an
//! in-file annotation is a one-line silent disarm sitting in the exact
//! directory the control protects, added by the same commit that adds the
//! violation.
//!
//! # `#[cfg(test)]` blocks inside the scope are IN SCOPE
//!
//! No path exemption, no test-block exemption. `is_test_path` matches any
//! `/fixtures/` or `/tests/` segment, so composing it would silently drop a
//! future `media/forward_test.rs` — the "alive, never applied" failure
//! reachable by a filename. `compute_test_block_ranges` is documented
//! fail-safe-broad and extends to EOF on unbalanced braces, which here would
//! disable the deny for the rest of a file.
//!
//! **Consequence, stated as a decision rather than left to be discovered: a
//! `println!` added to a `#[cfg(test)] mod tests` under the scope reds the
//! pipeline. That is intended — move the debug print to a sibling.**
//!
//! # Residual coverage gap, stated so it cannot be read as coverage
//!
//! An alias that *keeps* a denied name (`pub use tracing::info as debug;`) is
//! still caught by the invocation matcher. An alias that renames *outside*
//! the list (`use tracing::info as note;`) is caught by the import deny
//! wherever the `use` appears inside the scope — **including mid-line, after a
//! `{` or `;`**.
//!
//! **What is NOT caught, and NOT a matcher gap at all — a per-frame surface
//! OUTSIDE the configured directory.** Deliberately unnumbered: the two
//! residuals below are matchers that miss something INSIDE the scope, and
//! folding this into the same series invites closing all three with the
//! allowlist inversion, which would not touch this one. `crates/mh-service/src/webtransport/media_transport.rs`
//! is the transport-seam adapter and is called per frame, but it cannot enter
//! `denied_directories`: a lone file is the file-list shape §11 rules out,
//! and its directory cannot be denied wholesale because `connection.rs` sits
//! beside it and legitimately needs telemetry. It is covered instead by the
//! in-crate walker `crates/mh-service/tests/media_metrics_integration.rs::
//! no_log_or_metric_macro_is_reachable_from_the_hot_path`, which is this
//! guard's **complement, not its predecessor** — retiring it as redundant
//! silently drops the adapter's only coverage. Full reasoning lives once, in
//! `scripts/guards/simple/media-telemetry-deny.yaml` §INCOMPLETE BY DESIGN.
//!
//! **What is NOT caught**: a re-export living in a **sibling**, reached from
//! the scope as `crate::obs::note!(...)`. The import deny excludes
//! `crate::`-rooted paths by design (the media path legitimately imports
//! `crate::observability::metrics`), and the invocation matcher is a deny list
//! that `note` is not on. Only inverting the invocation matcher to an
//! *allowlist* of macro names permitted inside the scope would close it. Not
//! in scope for this loop.
//!
//! **Corrected 2026-09-07 (@security finding 2)**: this paragraph previously
//! tied the whole class to a sibling re-export. That UNDER-STATED it — until
//! `classify_use_line` learned to see a mid-line `use`, the same evasion
//! worked **inside one file** with no sibling involved:
//! `fn b() { use tracing::warn as w; w!("x"); }` escaped the import deny (not
//! at line start) and the invocation deny (`w` not on the list). The sentence
//! a future reader relies on now matches what the code does.
//!
//! # Ownership
//!
//! Machinery (module layout, clap dispatch, matcher implementations, manifest
//! schema, the walk) is `infrastructure`. Policy content (the directory list,
//! the denied families, the allowed-handle-method list, the reason-token
//! names, the fixtures) is `observability`. See `CLAUDE.md` §Specialists
//! "Guard-crate ownership (interim)".

use crate::common::explain::{print_secret_finding, SecretFinding};
use crate::common::scope::{assert_scope_live, ScopeFailure, ScopeRoot};
use crate::common::status::{emit_fail, emit_ok, emit_scope};
use crate::common::test_code_filter::blank_non_code;
use crate::metric_macros::MACRO_NAME_ALTERNATION;
use crate::telemetry_macros::{alternation_for, TelemetryGroup, INSTRUMENT_ATTR_ANY_RE};
use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::Deserialize;
use std::path::Path;

/// Reason-token prefix for every token this subcommand emits.
const TOKEN_PREFIX: &str = "media-telemetry-deny";

/// Manifest location, beside the wrapper.
const MANIFEST_PATH: &str = "scripts/guards/simple/media-telemetry-deny.yaml";

/// Extensions the scope walk considers. Rust source only — the media path is
/// a Rust hot path and a non-`.rs` file cannot contain a Rust macro
/// invocation.
const EXTENSIONS: &[&str] = &[".rs"];

/// Cached-handle methods that MUST NOT be flagged (ADR-0036 §11).
///
/// **This list is documentation and a test, not a runtime filter.** Under the
/// `<name>!` + delimiter anchor these forms structurally cannot match, so the
/// allow is satisfied by construction. A runtime subtraction would be
/// actively harmful: it would suppress a real macro violation that happened
/// to share a line with a handle call.
///
/// The complete `metrics`-crate handle surface: `Counter::increment` /
/// `absolute`, `Gauge::set` / `increment` / `decrement`, `Histogram::record`.
/// Policy content — owner `observability`.
pub const ALLOWED_HANDLE_METHODS: &[&str] =
    &[".increment", ".record", ".set", ".absolute", ".decrement"];

/// Crate roots whose import into the scope is denied.
///
/// Closes the alias evasion the invocation matcher cannot see:
/// `use tracing::info as note;` followed by `note!(...)`.
/// `crates/mh-service/src/media/mod.rs` already asserts "no `tracing` or
/// `log` import" as the invariant in prose, so this mechanises a premise the
/// media owner wrote down. Policy content — owner `observability` + `security`.
pub const DENIED_CRATE_ROOTS: &[&str] = &["tracing", "log", "metrics", "tracing_subscriber"];

/// Path keywords that make a `use` path crate-local, and therefore allowed.
///
/// **A closed keyword list, deliberately not a punctuation predicate.** An
/// earlier draft excluded "a leading `::`", which inverts the rule: in Rust
/// 2018+ `use ::tracing::info as note;` forces resolution from the extern
/// prelude and is the *most* unambiguous spelling of the external crate —
/// strictly more so than bare `tracing`, which a local `mod tracing` could
/// shadow. Excluding it would let the one spelling that cannot resolve
/// locally walk straight through. (@security, 2026-09-07.)
const LOCAL_PATH_KEYWORDS: &[&str] = &["crate", "self", "super"];

// ---------------------------------------------------------------------------
// Rule ordering — ONE array, and it is both the print order and the REASON
// precedence.
// ---------------------------------------------------------------------------

/// Every condition this guard can report, in precedence order.
///
/// **Index 0 both wins the STATUS line and prints first.** These were settled
/// in two separate threads (print order with @operations, REASON precedence
/// with @paired-observability) and deriving them from one array is a Gate-3
/// requirement — two lists that agree on the day they are written are two
/// encodings of one ordering. The `release_build_profile::RULE_ORDER`
/// precedent corroborates: a comment in that module records its premise count
/// once carrying three encodings, two of them wrong, drifting silently.
///
/// # Why scope/parse conditions outrank content
///
/// A content token tells the operator "go fix the cited line" when the truth
/// is "the guard checked nothing". That is §11's alive-never-applied failure
/// reaching the operator as **misdirection** rather than as silence, which is
/// worse than either.
///
/// # Why `escapes-root` outranks `missing` and `empty`
///
/// The axis is *what will the operator do next*, not *how much did the guard
/// fail to check*. The `-missing` runbook row says "restore the directory or
/// update the manifest path"; applied to a symlink escape that walks the
/// operator toward re-pointing the manifest at whatever the symlink targets,
/// **completing the evasion**. `escapes-root` is also the only token here
/// with an adversarial reading and the rarest, so ranked low it would be
/// invisible in exactly the runs where it co-occurs with the common case —
/// and both arise from the same manifest edit, so co-occurrence is the
/// likely shape. (@paired-observability ruling, 2026-09-07.)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    /// Tier A — the guard could not read its configuration at all.
    ManifestMissing,
    ManifestUnparseable,
    ScopeNoDirectoriesConfigured,
    /// Tier B — a configured directory is not usable.
    ScopeDirectoryEscapesRoot,
    ScopeDirectoryMissing,
    ScopeDirectoryEmpty,
    /// Tier C — a `use` declaration inside the scope could not be classified.
    UnparseableUse,
    /// Tier D — content findings.
    TelemetryCrateImport,
    MacroInMediaPath,
}

impl Rule {
    /// Precedence order: index 0 wins the STATUS line and prints first.
    pub const ORDER: &'static [Self] = &[
        Self::ManifestMissing,
        Self::ManifestUnparseable,
        Self::ScopeNoDirectoriesConfigured,
        Self::ScopeDirectoryEscapesRoot,
        Self::ScopeDirectoryMissing,
        Self::ScopeDirectoryEmpty,
        Self::UnparseableUse,
        Self::TelemetryCrateImport,
        Self::MacroInMediaPath,
    ];

    /// Kebab-case token suffix; the emitted REASON is `<TOKEN_PREFIX>-<suffix>`.
    pub const fn suffix(self) -> &'static str {
        match self {
            Self::ManifestMissing => "manifest-missing",
            Self::ManifestUnparseable => "manifest-unparseable",
            Self::ScopeNoDirectoriesConfigured => "scope-no-directories-configured",
            Self::ScopeDirectoryEscapesRoot => "scope-directory-escapes-root",
            Self::ScopeDirectoryMissing => "scope-directory-missing",
            Self::ScopeDirectoryEmpty => "scope-directory-empty",
            Self::UnparseableUse => "unparseable-use",
            Self::TelemetryCrateImport => "telemetry-crate-import",
            Self::MacroInMediaPath => "macro-in-media-path",
        }
    }

    /// Full reason token as it appears on the STATUS line.
    pub fn token(self) -> String {
        format!("{TOKEN_PREFIX}-{}", self.suffix())
    }

    /// Position in [`Self::ORDER`]; lower is higher precedence.
    pub fn rank(self) -> usize {
        Self::ORDER
            .iter()
            .position(|r| *r == self)
            .unwrap_or(usize::MAX)
    }

    /// Content rules carry an `-<n>-of-<m>-findings` suffix so truncation is
    /// visible from the STATUS line alone — `run-guards.sh` caps re-emitted
    /// output at `head -5`, and a diff with 8 violations showing 5 with
    /// nothing saying so is its own small "reads as coverage". The scope and
    /// parse rules are singular conditions and stay bare.
    pub const fn is_content(self) -> bool {
        matches!(self, Self::TelemetryCrateImport | Self::MacroInMediaPath)
    }

    /// `ERROR: PRECONDITION` for "the guard checked nothing"; `VIOLATION` for
    /// "go fix the cited line". Different first actions; collapsing them
    /// would lose that.
    /// Returns the prefix INCLUDING its trailing punctuation, so both emit
    /// sites can use a single `"{} [{}] …"` shape.
    ///
    /// The two arms are punctuated differently on purpose — `VIOLATION:` takes
    /// a colon, `ERROR: PRECONDITION` does not, because that is the literal
    /// string `release_build_profile` emits and the one @operations keys the
    /// runbook rows on. Carrying the punctuation HERE rather than in the format
    /// string is what stops the two emit sites drifting apart: an earlier draft
    /// had `"{}: [{}]"` at both sites and produced `ERROR: PRECONDITION: [token]`,
    /// one character off the precedent this module's doc claims to match.
    pub const fn prefix(self) -> &'static str {
        if self.is_content() {
            "VIOLATION:"
        } else {
            "ERROR: PRECONDITION"
        }
    }

    fn from_scope_failure(f: &ScopeFailure) -> Self {
        match f {
            ScopeFailure::NoDirectoriesConfigured => Self::ScopeNoDirectoriesConfigured,
            ScopeFailure::DirectoryEscapesRoot { .. } => Self::ScopeDirectoryEscapesRoot,
            ScopeFailure::DirectoryMissing { .. } => Self::ScopeDirectoryMissing,
            ScopeFailure::DirectoryEmpty { .. } => Self::ScopeDirectoryEmpty,
        }
    }
}

// ---------------------------------------------------------------------------
// Manifest
// ---------------------------------------------------------------------------

/// Typed manifest. `deny_unknown_fields` makes a typo a hard error rather
/// than a silently-ignored key — the schema IS the validation (ADR-0034 §3).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    /// Repo-relative directories whose Rust sources are denied telemetry
    /// macro forms. No globs: a glob matching nothing is a silent disarm, and
    /// the two scope tokens exist specifically to prevent that class.
    denied_directories: Vec<String>,
}

// ---------------------------------------------------------------------------
// Matchers
// ---------------------------------------------------------------------------

/// Every denied macro NAME, from both vocabularies.
///
/// The metrics half is `format!`ed from
/// [`crate::metric_macros::MACRO_NAME_ALTERNATION`], which is itself derived
/// from `MacroKind::ALL` — so a seventh `MacroKind` variant widens this guard
/// with **zero edits in this module**. The telemetry half comes from
/// [`crate::telemetry_macros`] via group selection. Neither alternation is
/// re-inlined here, and there is deliberately no third list.
static DENIED_MACRO_ALTERNATION: Lazy<String> = Lazy::new(|| {
    let telemetry = alternation_for(&[
        TelemetryGroup::Level,
        TelemetryGroup::Event,
        TelemetryGroup::LogMacro,
        TelemetryGroup::Span,
        TelemetryGroup::Print,
    ]);
    format!("{}|{telemetry}", *MACRO_NAME_ALTERNATION)
});

/// Denied macro invocation forms.
///
/// # The `!` + delimiter anchor is the whole design
///
/// Without it this becomes exactly the vocabulary-based guard §11 says cannot
/// be cited as protection — *"the directory-scoped deny catches by shape"* —
/// and it would red on `crates/mh-service/src/media/forward.rs`, which has a
/// local variable literally named `counter`, and on `fn record(&self)`.
///
/// The leading `\b` with **no** anchored qualifier group is deliberate: `\b`
/// matches after `:`, so one pattern covers `info!`, `tracing::info!`,
/// `::tracing::info!`, `log::info!` and `crate::obs::info!`. A pattern
/// requiring an optional-but-anchored `(?:tracing::)?` would miss a
/// three-segment path.
///
/// All three delimiters are accepted: `println!{"x"}` and `vec![..]` syntax
/// are legal Rust, so a `(`-only anchor is a one-character evasion. There is
/// no false-positive cost — `name!` is a macro invocation under any
/// delimiter.
///
/// This guard **was, until 2026-09-08, the only consumer in the crate with
/// the delimiter class right.** It is no longer alone and the list of
/// stragglers is deliberately not repeated here — `telemetry_macros`' two
/// anchors and `metric_labels::MACRO_OPENER_RE` were widened alongside it in
/// the same commit. The current residual is `metric_macros.rs`'s two
/// invocation regexes, tracked in `docs/TODO.md` §Observability Debt, which
/// is the single place that list is maintained. (An earlier draft of this
/// parenthetical named siblings the same commit had already fixed — prose
/// outliving its support, in the hunk that argues against exactly that.)
///
/// # `\s*` before the `!` — closed 2026-09-08, and it was a live evasion
///
/// The anchor was `!\s*[\(\[\{]`, requiring the `!` to follow the name
/// IMMEDIATELY. Rust tokenises `path ! delim`, so **`info !("x")` compiles,
/// logs, and walked straight through this deny**. Reproduced during the
/// ADR-0036 §11 policy audit:
///
/// ```text
/// $ printf 'fn f() { info !("x"); }\nfn h() { info!("z"); }\n' \
///     | grep -nP '\b(info|warn)!\s*[\(\[\{]'
/// 2:fn h() { info!("z"); }
/// ```
///
/// Interposed comments make it worse rather than better: every matcher here
/// runs over `blank_non_code` output, which turns `tracing::info /*x*/ !(…)`
/// into whitespace.
///
/// The widening has no realistic false-positive surface, because the
/// character after `!` must still be an open delimiter: `!=` is excluded, and
/// every near-miss in valid Rust (`&&`, `||`, `|x|`, `;`, `=>`, `)`)
/// interposes a non-whitespace token. Verified against the real scope before
/// landing — `crates/mh-service/src/media/` contains zero `name !(` forms,
/// and its only macro invocations are `assert*!`, `format_args!` and
/// `select!`, none of them denied names.
///
/// `compile_error!` is deliberately absent: it is §11's own release-build
/// control and appears in `media/mod.rs`. That is why the alternation is
/// enumerated rather than a `\w+!\s*[\(\[\{]` shape.
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static DENIED_MACRO_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(&format!(
        r"\b({})\s*!\s*[\(\[\{{]",
        *DENIED_MACRO_ALTERNATION
    ))
    .expect("static pattern compiles")
});

/// Open `*_span!` shape, media-scope only and deliberately NOT promoted.
///
/// Rationale: an import deny is being spent to close `use tracing::info as
/// note;`, and a sibling-defined `custom_span!` wrapper is the same evasion
/// class — closing one and not the other is incoherent. It stays local
/// because a shared vocabulary home can only honestly export enumerated
/// membership; `rust_pii` must not inherit false positives on arbitrary
/// `*_span!`.
///
/// `\w+_span` requires a literal `_`, so bare `span!` still needs its
/// enumerated entry in the SPAN group.
///
/// **This anchor carries the same `\s*` before the `!` as
/// [`DENIED_MACRO_RE`], and the two must be widened together.** Fixing one
/// and not the other would leave `custom_span !(…)` passing while
/// `custom_span!(…)` is denied — an asymmetry WORSE than the uniform gap was,
/// because this doc comment would then claim the open-span shape is covered
/// when it is covered for one spelling of two. A partial fix here reads as
/// coverage, which is the failure ADR-0036's coverage section is about.
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static OPEN_SPAN_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\b(\w+_span)\s*!\s*[\(\[\{]").expect("static pattern compiles"));

/// A `use` declaration, captured up to its first path segment.
///
/// Group 1 is the first identifier segment after any `pub…` qualifier and any
/// leading `::`. Group 2 is the remainder, used only to detect the brace-group
/// form. Lookaround is unavailable (the `regex` crate rejects it by
/// construction — that is the linear-time guarantee), so the qualifier is
/// consumed explicitly rather than asserted.
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static USE_DECL_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*(?:pub\s*(?:\([^)]*\)\s*)?)?use\s+(.*)$").expect("static pattern compiles")
});

/// One finding. Carries the rule, the location, and the **macro spelling** —
/// never argument text or the remainder of the source line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub rule: Rule,
    /// Repo-relative path of the offending file.
    pub file: String,
    /// 1-based line number.
    pub line: usize,
    /// 1-based column.
    pub col: usize,
    /// The matched construct's SPELLING only — e.g. `debug_span!`,
    /// `#[instrument]`, `use tracing::…`. Never the arguments.
    pub spelling: String,
}

/// How a `use` declaration classified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UseVerdict {
    /// First segment is `crate` / `self` / `super` — allowed.
    Local,
    /// First segment is not on the denied list — allowed.
    AllowedExternal,
    /// First segment is a denied crate root.
    Denied { root: String },
    /// The shape could not be resolved. **Fails toward the finding**: a
    /// `use` line the classifier cannot read must fail loudly rather than
    /// pass, or "a shape I didn't anticipate" becomes a silent hole — §11's
    /// "reads as coverage" in miniature.
    Unparseable,
}

/// Classify one already-blanked `use` line.
///
/// Handles `pub` / `pub(crate)` / `pub(in ...)` qualifiers, an optional
/// leading `::`, whitespace around `::`, and the brace-group form
/// `use {a::b, c::d};` (denied if ANY member's first segment is denied).
/// # `use` is a TOKEN, not a line prefix
///
/// A line-anchored match misses `fn b() { use tracing::warn as w; w!("x"); }`,
/// which is a **complete evasion of both layers inside one file**: the mid-line
/// `use` escapes the import deny, and `w` is not on the invocation deny list.
/// No sibling re-export is needed. (@security finding 2, 2026-09-07.)
///
/// **Layer 2 is a mitigation, not a guarantee.** `cargo fmt` splits that form
/// onto separate lines, after which a line-anchored match would catch it — but
/// `#[rustfmt::skip]` survives `cargo fmt --check` by design, and the pair
/// reported `STATUS=OK`. "rustfmt would have reformatted it" is exactly the
/// control-that-has-to-notice reasoning ADR-0036 §11 rejects.
///
/// **Ordering is load-bearing**: try the WHOLE line first, unchanged; only if
/// that fails, split on `{` / `;` and test each segment. Splitting first would
/// cut the brace-group form `use {tracing::info, std::fmt};` at its own `{`,
/// downgrading a correct `Denied` to `Unparseable`.
pub fn classify_use_line(line: &str) -> Option<UseVerdict> {
    if let Some(caps) = USE_DECL_RE.captures(line) {
        let rest = caps.get(1)?.as_str().trim();
        return Some(classify_use_body(rest));
    }
    let mut strongest: Option<UseVerdict> = None;
    for segment in line.split(['{', ';']) {
        let Some(caps) = USE_DECL_RE.captures(segment) else {
            continue;
        };
        let Some(rest) = caps.get(1) else { continue };
        match classify_use_body(rest.as_str().trim()) {
            UseVerdict::Denied { root } => return Some(UseVerdict::Denied { root }),
            UseVerdict::Unparseable => strongest = Some(UseVerdict::Unparseable),
            v @ (UseVerdict::Local | UseVerdict::AllowedExternal) => {
                let _ = strongest.get_or_insert(v);
            }
        }
    }
    strongest
}

fn classify_use_body(body: &str) -> UseVerdict {
    let body = body.trim().trim_start_matches("::").trim();

    // Brace-group form: `use {a::b, c::d};` — classify every member and take
    // the strongest verdict.
    if let Some(inner) = body.strip_prefix('{') {
        let Some(close) = inner.rfind('}') else {
            return UseVerdict::Unparseable;
        };
        let members: Vec<&str> = split_top_level_commas(&inner[..close]);
        if members.is_empty() {
            return UseVerdict::Unparseable;
        }
        let mut verdict = UseVerdict::AllowedExternal;
        for m in members {
            match classify_use_body(m) {
                UseVerdict::Denied { root } => return UseVerdict::Denied { root },
                UseVerdict::Unparseable => verdict = UseVerdict::Unparseable,
                UseVerdict::Local | UseVerdict::AllowedExternal => {}
            }
        }
        return verdict;
    }

    // First identifier segment: up to `::`, `{`, `;`, whitespace, or `,`.
    let seg: String = body
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    if seg.is_empty() {
        return UseVerdict::Unparseable;
    }
    if LOCAL_PATH_KEYWORDS.contains(&seg.as_str()) {
        return UseVerdict::Local;
    }
    if let Some(root) = DENIED_CRATE_ROOTS.iter().find(|r| **r == seg) {
        return UseVerdict::Denied {
            root: (*root).to_string(),
        };
    }
    UseVerdict::AllowedExternal
}

/// Split on commas that are not nested inside braces or parentheses.
fn split_top_level_commas(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    for (i, c) in s.char_indices() {
        match c {
            '{' | '(' | '[' => depth += 1,
            '}' | ')' | ']' => depth -= 1,
            ',' if depth == 0 => {
                let piece = s[start..i].trim();
                if !piece.is_empty() {
                    out.push(piece);
                }
                start = i + 1;
            }
            _ => {}
        }
    }
    let piece = s[start..].trim();
    if !piece.is_empty() {
        out.push(piece);
    }
    out
}

/// Find `#[instrument]`-family attributes in already-blanked source.
///
/// Two passes, because the primary regex fails in the wrong direction.
/// [`INSTRUMENT_ATTR_ANY_RE`]'s `[^\]]*` stops at the first `]`, so an
/// attribute containing a literal `]` outside a string truncates the search
/// window and produces a **miss** — failing away from the finding, which is
/// the direction that matters. The second pass keys on `\binstrument\b`
/// anywhere in blanked text and reports any occurrence the regex did not
/// already claim, provided the line plausibly sits in attribute position.
/// Conservative by construction: it fails toward the finding.
fn find_instrument_attributes(blanked: &str) -> Vec<(usize, usize)> {
    let mut hits: Vec<(usize, usize)> = Vec::new();
    for m in INSTRUMENT_ATTR_ANY_RE.find_iter(blanked) {
        hits.push((m.start(), m.end()));
    }
    // Conservative second pass.
    for m in INSTRUMENT_WORD_RE.find_iter(blanked) {
        let already = hits.iter().any(|(s, e)| m.start() >= *s && m.start() < *e);
        if already {
            continue;
        }
        // Is there an unclosed `#[` before this occurrence?
        let before = &blanked[..m.start()];
        let open = before.rfind("#[");
        let close = before.rfind(']');
        if open.is_some() && open > close {
            hits.push((m.start(), m.end()));
        }
    }
    hits.sort_unstable();
    hits
}

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static INSTRUMENT_WORD_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\binstrument\b").expect("static pattern compiles"));

/// Byte offset -> (1-based line, 1-based column).
fn offset_to_line_col(text: &str, offset: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut last_nl: usize = 0;
    for (i, c) in text.char_indices() {
        if i >= offset {
            break;
        }
        if c == '\n' {
            line += 1;
            last_nl = i + 1;
        }
    }
    (line, offset.saturating_sub(last_nl) + 1)
}

/// Blank comments and string bodies across a whole file, preserving byte
/// offsets so positions computed on the blanked text are valid in the source.
pub fn blank_file(content: &str) -> String {
    let mut out = String::with_capacity(content.len());
    let mut in_block = false;
    for (idx, line) in content.split('\n').enumerate() {
        if idx > 0 {
            out.push('\n');
        }
        let (blanked, still) = blank_non_code(line, in_block);
        in_block = still;
        // `blank_non_code` truncates at a line comment; pad back to length so
        // byte offsets stay aligned with the source.
        out.push_str(&blanked);
        for _ in blanked.len()..line.len() {
            out.push(' ');
        }
    }
    out
}

/// The pure policy function: findings for one file's content.
///
/// Takes already-resolved inputs so fixtures can drive it directly without a
/// filesystem or a manifest.
pub fn check_file(rel_path: &str, content: &str) -> Vec<Finding> {
    let blanked = blank_file(content);
    let mut findings: Vec<Finding> = Vec::new();

    // Denied macro invocation forms (metrics + telemetry vocabularies).
    for caps in DENIED_MACRO_RE.captures_iter(&blanked) {
        let Some(name) = caps.get(1) else { continue };
        let (line, col) = offset_to_line_col(&blanked, name.start());
        findings.push(Finding {
            rule: Rule::MacroInMediaPath,
            file: rel_path.to_string(),
            line,
            col,
            spelling: format!("{}!", name.as_str()),
        });
    }

    // Open `*_span!` shape (media-scope only).
    for caps in OPEN_SPAN_RE.captures_iter(&blanked) {
        let Some(name) = caps.get(1) else { continue };
        let (line, col) = offset_to_line_col(&blanked, name.start());
        let dup = findings
            .iter()
            .any(|f| f.line == line && f.col == col && f.rule == Rule::MacroInMediaPath);
        if dup {
            continue;
        }
        findings.push(Finding {
            rule: Rule::MacroInMediaPath,
            file: rel_path.to_string(),
            line,
            col,
            spelling: format!("{}!", name.as_str()),
        });
    }

    // `#[instrument]` family.
    for (start, _end) in find_instrument_attributes(&blanked) {
        let (line, col) = offset_to_line_col(&blanked, start);
        findings.push(Finding {
            rule: Rule::MacroInMediaPath,
            file: rel_path.to_string(),
            line,
            col,
            spelling: "#[instrument]".to_string(),
        });
    }

    // Telemetry-crate imports.
    for (idx, raw_line) in blanked.split('\n').enumerate() {
        let Some(verdict) = classify_use_line(raw_line) else {
            continue;
        };
        match verdict {
            UseVerdict::Denied { root } => findings.push(Finding {
                rule: Rule::TelemetryCrateImport,
                file: rel_path.to_string(),
                line: idx + 1,
                col: 1,
                spelling: format!("use {root}::…"),
            }),
            UseVerdict::Unparseable => findings.push(Finding {
                rule: Rule::UnparseableUse,
                file: rel_path.to_string(),
                line: idx + 1,
                col: 1,
                spelling: "use <unparseable>".to_string(),
            }),
            UseVerdict::Local | UseVerdict::AllowedExternal => {}
        }
    }

    findings.sort_by_key(|f| (f.rule.rank(), f.line, f.col));
    findings
}

/// Static remediation advice. Carries nothing read from a scanned file.
fn fix_advice(rule: Rule) -> &'static str {
    match rule {
        Rule::MacroInMediaPath => {
            "no macro form is reachable from the forward function (ADR-0036 §11); resolve a metric handle ONCE AT SETUP IN A SIBLING MODULE (e.g. crate::observability::metrics — lifecycle and setup are siblings of the media directory, not children, so a setup fn added HERE reds too), store it on the forwarder, and call .increment/.record/.set/.absolute/.decrement on it here"
        }
        Rule::TelemetryCrateImport => {
            "the media path imports no telemetry crate (ADR-0036 §11); take handles from a sibling module via a crate:: path instead"
        }
        Rule::UnparseableUse => {
            "this `use` declaration could not be classified, so it is reported rather than passed; rewrite it in a plain `use <path>;` form"
        }
        _ => "see docs/runbooks/devloop-validation.md",
    }
}

/// Emit one finding as exactly ONE physical line.
///
/// **The single-physical-line property is load-bearing and is invisible from
/// inside this module.** `run-guards.sh` surfaces guard output through
/// `grep -E "(VIOLATION|ERROR|WARN)" | head -5`, so a record wrapped for
/// readability loses its continuation lines silently, or burns the five-line
/// cap on one finding. Do not "tidy" these into multiple lines.
fn print_finding_line(f: &Finding) {
    println!(
        "{} [{}] {}:{}:{} — {} — {}",
        f.rule.prefix(),
        f.rule.token(),
        f.file,
        f.line,
        f.col,
        f.spelling,
        fix_advice(f.rule)
    );
}

/// Emit the `ERROR: MIXED_CONDITIONS:` banner when more than one rule class
/// fired in a single run.
///
/// # Why this exists as a printed mechanism rather than runbook prose
///
/// Precedence means the STATUS line **under-reports by design**: only the
/// highest-ranked condition reaches `REASON=`, and an operator's first move is
/// to grep that token and triage from it. The `-<n>-of-<m>-findings` suffix
/// does not cover this — it is gated on `winner.is_content()`, so it says
/// nothing in exactly the case that motivated this line: a scope or parse
/// token winning while content findings sit unshown.
///
/// A runbook row saying "read past the STATUS line" cannot help someone who
/// never opens the runbook because the STATUS token looked self-explanatory.
/// The line has to be in the output.
///
/// # The `ERROR:` prefix is load-bearing
///
/// `run-guards.sh` re-emits only lines matching
/// `(VIOLATION|violation|ERROR|error|WARN)`. Without a matching substring this
/// banner is captured and discarded, and a control that is emitted but never
/// seen is the failure this whole guard is about. Pinned by the
/// filter-survival case in `scripts/guards/media-telemetry-deny.test.sh`,
/// which pipes every record class through the real grep — because asserting a
/// line is *emitted* proves nothing about whether anyone *sees* it.
fn print_mixed_conditions_line(winner: Rule, other_classes: usize) {
    if other_classes == 0 {
        return;
    }
    println!(
        "ERROR: MIXED_CONDITIONS: winner={} co-firing={} other condition class(es) — the STATUS token is the highest-ranked condition, not the only one. Scan the full output; do not triage from the STATUS line alone.",
        winner.token(),
        other_classes
    );
}

/// Emit one scope failure as exactly ONE physical line.
///
/// The `ERROR: PRECONDITION` prefix matches `release_build_profile`'s, but the
/// body inverts its triage and says so explicitly — that guard's neighbouring
/// rows read "NOT a diff defect", and a reader who pattern-matches the prefix
/// would otherwise re-run on a quiet machine forever.
fn print_scope_failure_line(rule: Rule, detail: &str) {
    println!(
        "{} [{}] THIS IS A DIFF DEFECT, not a machine fault — {} — see docs/runbooks/devloop-validation.md §6.3.1",
        rule.prefix(),
        rule.token(),
        detail
    );
}

fn explain_finding(f: &Finding) {
    let policy = format!("media-telemetry-deny::{}", f.rule.suffix());
    print_secret_finding(&SecretFinding {
        file: &f.file,
        row: f.line,
        col: f.col,
        policy: &policy,
        // `pattern_name` carries the macro SPELLING, never arguments.
        pattern_name: &f.spelling,
        extras: &[],
        src_file: file!(),
        src_line: line!(),
    });
}

/// Subcommand entry point.
pub fn run(repo_root: &Path, explain: bool) -> Result<()> {
    // --- Manifest, fail-closed ------------------------------------------
    let manifest_path = repo_root.join(MANIFEST_PATH);
    let Ok(raw) = std::fs::read_to_string(&manifest_path) else {
        print_scope_failure_line(
            Rule::ManifestMissing,
            &format!("manifest `{MANIFEST_PATH}` could not be read"),
        );
        emit_fail(Rule::ManifestMissing.token());
        std::process::exit(1);
    };
    let manifest: Manifest = match serde_norway::from_str(&raw) {
        Ok(m) => m,
        Err(e) => {
            print_scope_failure_line(
                Rule::ManifestUnparseable,
                &format!("manifest `{MANIFEST_PATH}` failed to deserialize ({e})"),
            );
            emit_fail(Rule::ManifestUnparseable.token());
            std::process::exit(1);
        }
    };

    // --- Scope liveness --------------------------------------------------
    let roots: Vec<ScopeRoot> =
        match assert_scope_live(repo_root, &manifest.denied_directories, EXTENSIONS) {
            Ok(r) => r,
            Err(failures) => {
                let mut ordered: Vec<(Rule, String)> = failures
                    .iter()
                    .map(|f| (Rule::from_scope_failure(f), f.detail()))
                    .collect();
                ordered.sort_by_key(|(r, _)| r.rank());
                let winner_rule = ordered
                    .first()
                    .map_or(Rule::ScopeDirectoryMissing, |(r, _)| *r);
                let mut classes: Vec<Rule> = ordered.iter().map(|(r, _)| *r).collect();
                classes.sort_by_key(|r: &Rule| r.rank());
                classes.dedup();
                // Ahead of every record, so it survives the `head -5` cap.
                print_mixed_conditions_line(winner_rule, classes.len().saturating_sub(1));
                for (rule, detail) in &ordered {
                    print_scope_failure_line(*rule, detail);
                }
                emit_scope(format!(
                    "{} configured director{}, 0 usable, {} scope failure(s)",
                    manifest.denied_directories.len(),
                    if manifest.denied_directories.len() == 1 {
                        "y"
                    } else {
                        "ies"
                    },
                    ordered.len()
                ));
                emit_fail(winner_rule.token());
                std::process::exit(1);
            }
        };

    // --- Walk -------------------------------------------------------------
    let mut findings: Vec<Finding> = Vec::new();
    let mut file_count = 0usize;
    for root in &roots {
        for path in root.files() {
            file_count += 1;
            let rel = path
                .strip_prefix(repo_root)
                .unwrap_or(path)
                .to_string_lossy()
                .to_string();
            let content = std::fs::read_to_string(path)
                .with_context(|| format!("reading {}", path.display()))?;
            findings.extend(check_file(&rel, &content));
        }
    }
    findings.sort_by_key(|f| (f.rule.rank(), f.file.clone(), f.line, f.col));

    emit_scope(format!(
        "{} configured director{}, {file_count} .rs files, {} enumerated macro forms, {} hits",
        roots.len(),
        if roots.len() == 1 { "y" } else { "ies" },
        DENIED_MACRO_ALTERNATION.split('|').count(),
        findings.len()
    ));

    if findings.is_empty() {
        emit_ok(format!(
            "{TOKEN_PREFIX}-clean-{file_count}-files-{}-dirs",
            roots.len()
        ));
        return Ok(());
    }

    // `if explain { … } else { … }`, mutually exclusive, matching every sibling
    // subcommand (`no_insecure_browser_flags`, `release_build_profile`, …).
    // Emitting both would double every finding in the `head -5` re-emission
    // window that `run-guards.sh` applies, which is the budget OP-11/OP-12
    // exist to protect.
    let mut classes: Vec<Rule> = findings.iter().map(|f| f.rule).collect();
    classes.sort_by_key(|r: &Rule| r.rank());
    classes.dedup();
    let winner_rule = classes.first().copied().unwrap_or(Rule::MacroInMediaPath);
    // Printed BEFORE any record so it survives `run-guards.sh`'s `head -5`.
    print_mixed_conditions_line(winner_rule, classes.len().saturating_sub(1));

    for f in &findings {
        if explain {
            explain_finding(f);
        } else {
            print_finding_line(f);
        }
    }

    let winner = findings
        .iter()
        .map(|f| f.rule)
        .min_by_key(|r| r.rank())
        .unwrap_or(Rule::MacroInMediaPath);
    let winner_count = findings.iter().filter(|f| f.rule == winner).count();
    let token = if winner.is_content() {
        format!(
            "{}-{winner_count}-of-{}-findings",
            winner.token(),
            findings.len()
        )
    } else {
        winner.token()
    };
    emit_fail(token);
    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telemetry_macros::TelemetryMacro;

    fn rules(findings: &[Finding]) -> Vec<Rule> {
        findings.iter().map(|f| f.rule).collect()
    }

    // ---- Deny families ----

    #[test]
    fn metrics_family_is_derived_from_macro_kind_all() {
        // DERIVATION, not equality: every MacroKind name must be reachable
        // through this guard's alternation with zero edits here.
        for kind in crate::metric_macros::MacroKind::ALL {
            let src = format!(
                "fn f() {{ {}(\"m\"); }}",
                format_args!("{}!", kind.as_str())
            );
            let found = check_file("x.rs", &src);
            assert!(
                !found.is_empty(),
                "`{}!` is in MacroKind::ALL but this guard does not deny it — the derivation is cosmetic",
                kind.as_str()
            );
        }
    }

    /// CONSUMER-side forcing function, symmetric with
    /// `metrics_family_is_derived_from_macro_kind_all`.
    ///
    /// `DENIED_MACRO_ALTERNATION` selects groups by a runtime literal —
    /// `alternation_for(&[Level, Event, LogMacro, Span, Print])`. The enum
    /// machinery (`group()`, `as_str()`, `ALL`) is compiler-exhaustive, so a
    /// new `TelemetryMacro` variant forces those to update; **the group
    /// selection slice is not checked by the compiler**. A future vocabulary
    /// member landing in a group this guard does not select would silently
    /// escape the deny.
    ///
    /// That default — silent exclusion — is ADR-0036 §11's "alive, never
    /// applied", one level down and scoped to a single new construct. For
    /// `rust_pii` selecting `Level` only is correct; for THIS guard
    /// completeness is the intent, because §11 denies the whole telemetry
    /// surface inside the media path. So the completeness is asserted rather
    /// than assumed. (@test, Gate 2.)
    #[test]
    fn every_telemetry_macro_in_all_is_denied_by_this_guard() {
        for m in TelemetryMacro::ALL {
            let name = m.as_str();
            let src = format!("fn f() {{ {name}!(x); }}");
            assert!(
                !check_file("x.rs", &src).is_empty(),
                "`{name}!` is in TelemetryMacro::ALL but this guard's group selection does not deny it — add its group to DENIED_MACRO_ALTERNATION"
            );
        }
    }

    /// The same property stated at the group level, so the failure message
    /// names the missing GROUP rather than an example member.
    #[test]
    fn denied_alternation_selects_every_telemetry_group() {
        for m in TelemetryMacro::ALL {
            assert!(
                DENIED_MACRO_ALTERNATION.split('|').any(|n| n == m.as_str()),
                "group {:?} (member `{}`) is absent from DENIED_MACRO_ALTERNATION",
                m.group(),
                m.as_str()
            );
        }
    }

    /// Whitespace between the macro name and its `!` — closed 2026-09-08.
    ///
    /// `info !("x")` is legal Rust that compiles and logs, and every one of
    /// these walked through the deny before the anchor was widened. Asserted
    /// across BOTH anchors, because `OPEN_SPAN_RE` is a second matcher with
    /// the identical shape and a one-anchor fix would leave `custom_span !(`
    /// passing while `custom_span!(` is denied.
    #[test]
    fn space_between_name_and_bang_does_not_evade() {
        for src in [
            r#"fn f() { info !("x"); }"#,
            r#"fn f() { tracing::warn  !("x"); }"#,
            r#"fn f() { counter !("m"); }"#,
            r#"fn f() { println !("x"); }"#,
            // OPEN_SPAN_RE's half of the same class.
            r#"fn f() { custom_span !("x"); }"#,
            // What `blank_non_code` hands the matcher for an interposed
            // comment: `tracing::info /*x*/ !(...)`.
            r#"fn f() { tracing::info        !("x"); }"#,
        ] {
            assert!(
                !check_file("x.rs", src).is_empty(),
                "spaced invocation must not evade: {src}"
            );
        }
    }

    /// The widening is bounded. These must stay silent.
    ///
    /// `\s*` never crosses a token, and the character after `!` must still be
    /// an open delimiter — so `!=` cannot match however the operands are
    /// spaced. Without these the widening would be a plausible source of
    /// false positives on real code, which is the objection it has to answer.
    #[test]
    fn space_widening_does_not_over_match() {
        for src in [
            // `\b` still holds: a compound name is not a denied name.
            "fn f() { my_info !(x); }",
            "fn f(v: &[u8]) { my_counter !(v); }",
            // `!=` — the char after `!` is `=`, not a delimiter.
            "fn f(a: u32, b: u32) -> bool { a != (b) }",
            "fn f(error: u32) -> bool { error != (0) }",
            // A denied NAME as a plain binding, with no invocation anywhere.
            "fn f() { let counter = 0; let _ = counter; }",
        ] {
            assert!(check_file("x.rs", src).is_empty(), "must not fire: {src}");
        }
    }

    /// `\s*` reaches ACROSS LINES here, and that is intended (@security S2).
    ///
    /// `\s` includes `\n`, and [`check_file`] runs the matchers over the whole
    /// `blank_file` output rather than line by line, so a macro call split
    /// across lines is denied. Correct for a fail-loud deny — it compiles and
    /// logs like any other — but it was ASSERTED NOWHERE, and the fixture
    /// footer previously claimed the opposite. Pinned so that a later reader
    /// who believes the widening is line-local cannot quietly re-narrow it.
    ///
    /// Note the asymmetry with the sibling anchors this loop also widened:
    /// `telemetry_macros`' consumers iterate `content.lines()`, so there the
    /// same `\s*` genuinely cannot cross a line. The reach is a property of
    /// the CONSUMER, not of the character class.
    #[test]
    fn space_widening_spans_lines_deliberately() {
        let findings = check_file("x.rs", "fn f() {\n    info\n        !(\"x\");\n}\n");
        assert_eq!(
            findings.len(),
            1,
            "a line-spanning invocation is still an invocation; got {findings:?}"
        );
        assert_eq!(findings[0].spelling, "info!");
    }

    /// The by-construction allow, demonstrated rather than asserted.
    ///
    /// One physical line carrying a handle call AND a real denied macro. This
    /// is the only shape that distinguishes "structurally cannot match" from
    /// "filtered out at line level": the allow-fixture, which holds handle
    /// calls alone, passes identically under either implementation.
    #[test]
    fn handle_call_colocated_with_macro_still_fires_exactly_once() {
        let src = r#"fn f(h: &H) { h.frames.increment(1); counter!("mh_x", 1); }"#;
        let findings = check_file("x.rs", src);
        assert_eq!(
            findings.len(),
            1,
            "expected exactly one finding; zero means a line filter masked the \
             macro, two means the handle method was flagged — got {findings:?}"
        );
        assert_eq!(findings[0].spelling, "counter!");
    }

    #[test]
    fn telemetry_families_all_fire() {
        for src in [
            "fn f() { trace!(x); }",
            "fn f() { debug!(x); }",
            "fn f() { info!(x); }",
            "fn f() { warn!(x); }",
            "fn f() { error!(x); }",
            "fn f() { tracing::info!(x); }",
            "fn f() { ::tracing::info!(x); }",
            "fn f() { log::error!(x); }",
            "fn f() { crate::obs::warn!(x); }",
            "fn f() { event!(Level::INFO, x); }",
            "fn f() { log!(Level::Info, x); }",
            "fn f() { span!(x); }",
            "fn f() { trace_span!(x); }",
            "fn f() { debug_span!(x); }",
            "fn f() { info_span!(x); }",
            "fn f() { warn_span!(x); }",
            "fn f() { error_span!(x); }",
            "fn f() { println!(x); }",
            "fn f() { print!(x); }",
            "fn f() { eprintln!(x); }",
            "fn f() { eprint!(x); }",
            "fn f() { dbg!(x); }",
            "#[instrument]\nfn f() {}",
            "#[tracing::instrument(skip_all)]\nfn f() {}",
            "#[::tracing::instrument]\nfn f() {}",
            "#[cfg_attr(test, tracing::instrument)]\nfn f() {}",
        ] {
            assert!(!check_file("x.rs", src).is_empty(), "should fire: {src}");
        }
    }

    /// Non-`(` delimiters are legal Rust and must not be a one-character
    /// evasion.
    #[test]
    fn non_paren_delimiters_fire() {
        assert!(!check_file("x.rs", r#"fn f() { println!{"x"} }"#).is_empty());
        assert!(!check_file("x.rs", r#"fn f() { info![x] }"#).is_empty());
    }

    /// A sibling-defined `custom_span!` wrapper is the same evasion class as
    /// an aliased import, so the open shape is denied inside the scope.
    #[test]
    fn open_span_shape_fires() {
        let f = check_file("x.rs", "fn f() { custom_span!(a); }");
        assert_eq!(rules(&f), vec![Rule::MacroInMediaPath]);
        assert_eq!(f[0].spelling, "custom_span!");
    }

    // ---- Allow-list: satisfied BY CONSTRUCTION ----

    /// ADR-0036 §11: "the guard denies macro forms and must not touch handle
    /// methods, or it bans the pattern it exists to enforce."
    #[test]
    fn cached_handle_methods_are_never_flagged() {
        let src = r#"
fn forward(h: &Handles) {
    h.frames.increment(1);
    h.latency.record(0.5);
    h.depth.set(3.0);
    h.total.absolute(42);
    h.depth.decrement(1.0);
}
"#;
        assert!(
            check_file("media/forward.rs", src).is_empty(),
            "cached-handle calls must never fire"
        );
        // And each individually, so a partial regression is localised.
        for m in ALLOWED_HANDLE_METHODS {
            let one = format!("fn f(h: &H) {{ h.x{m}(1); }}");
            assert!(check_file("x.rs", &one).is_empty(), "must not fire: {m}");
        }
    }

    /// The name-based false positives a shape-based matcher must not produce.
    #[test]
    fn bare_identifiers_are_not_flagged() {
        let src = r#"
struct S { error: u32, span: u32 }
fn f(&self) {
    let counter = 0;
    let span = 1;
    fn record(&self) {}
    let s = "counter!(";
}
"#;
        assert!(
            check_file("x.rs", src).is_empty(),
            "bare identifiers must not fire"
        );
    }

    /// Not telemetry. A deny that reaches these makes the guard something
    /// people route around.
    #[test]
    fn non_telemetry_macros_are_not_denied() {
        let src = r#"
fn f() {
    let s = format!("x");
    write!(f, "x").ok();
    writeln!(f, "x").ok();
    assert!(true);
    assert_eq!(1, 1);
    vec![1, 2];
    compile_error!("release control");
}
"#;
        assert!(
            check_file("x.rs", src).is_empty(),
            "non-telemetry macros must not fire"
        );
    }

    // ---- Comment / string blanking ----

    /// Verbatim shapes from the real `crates/mh-service/src/media/` tree.
    /// These break for DIFFERENT reasons: the first has a real `!(` and
    /// defeats a correctly-shaped anchor; the second has no paren at all and
    /// defeats the attribute matcher. A fixture covering one reads as
    /// coverage for both.
    #[test]
    fn real_tree_prose_shapes_stay_clean() {
        let src = r#"
//! No `counter!` / `histogram!` / `gauge!` / `describe_*!`, no
//! `println!` / `eprintln!` / `dbg!`, no `event!` or span macro, no
//! `#[instrument]`.
/// formatting would happen in a *sibling* — a `debug!(?frame)` in
/* block: span!(x) and #[instrument] */
fn forward() {
    let s = "event!(a, b)";  // trailing: println!("y")
}
"#;
        assert!(
            check_file("crates/mh-service/src/media/mod.rs", src).is_empty(),
            "prose must not red the code that documents the ban"
        );
    }

    // ---- Import deny ----

    #[test]
    fn denied_imports_fire_including_the_leading_colons_spelling() {
        for src in [
            "use tracing::info as note;",
            "use ::tracing::info as note;",
            "use tracing as t;",
            "use log::{error, warn};",
            "pub use tracing::info;",
            "pub(crate) use tracing::info;",
            "use   tracing :: info ;",
            "use metrics::counter;",
            "use tracing_subscriber::fmt;",
            "use {tracing::info, std::fmt};",
        ] {
            let f = check_file("x.rs", src);
            assert!(
                f.iter().any(|x| x.rule == Rule::TelemetryCrateImport),
                "should deny: {src}"
            );
        }
    }

    /// Verbatim from the real tree. If anyone loosens the first-segment
    /// anchor to a bare-token match, this reds — which is the point.
    /// `per_frame_trace` is §11's dev-only facility, deliberately a SIBLING so
    /// the `#[cfg]` decision never enters the media directory; it contains the
    /// token `trace`.
    #[test]
    fn crate_local_imports_stay_clean() {
        let src = r#"
use crate::observability::metrics::{MediaDirection, MediaDropReason, MediaLatencyPhase};
use crate::observability::metrics::MediaMetricHandles;
use crate::observability::metrics::MediaDropReason;
use crate::observability::per_frame_trace;
use self::inner::thing;
use super::*;
use std::sync::Arc;
use media_protocol::frame::MAX_FRAME_BYTES;
"#;
        assert!(
            check_file("x.rs", src).is_empty(),
            "crate-local and unrelated external imports must stay clean"
        );
    }

    /// `use` is a token, not a line prefix. The mid-line form evades BOTH
    /// layers inside one file. @security finding 2, 2026-09-07.
    #[test]
    fn mid_line_use_is_denied() {
        for src in [
            r#"fn b() { use tracing::warn as w; w!("x"); }"#,
            "fn b() { use tracing::info; }",
            "fn b() { let x = 1; use log::error; }",
            "#[rustfmt::skip]\nfn b() { use metrics::counter as c; }",
        ] {
            let f = check_file("x.rs", src);
            assert!(
                f.iter().any(|x| x.rule == Rule::TelemetryCrateImport),
                "mid-line `use` must be denied: {src}"
            );
        }
    }

    /// The whole-line attempt must come FIRST: splitting on `{` before trying
    /// the line would cut the brace-group form at its own `{` and downgrade a
    /// correct `Denied` to `Unparseable`.
    #[test]
    fn brace_group_still_classifies_as_denied_not_unparseable() {
        assert_eq!(
            classify_use_line("use {tracing::info, std::fmt};"),
            Some(UseVerdict::Denied {
                root: "tracing".to_string()
            })
        );
        let f = check_file("x.rs", "use {tracing::info, std::fmt};");
        assert!(f.iter().all(|x| x.rule != Rule::UnparseableUse));
    }

    /// Mid-line crate-local `use` must stay silent, exactly as at line start.
    #[test]
    fn mid_line_crate_local_use_stays_clean() {
        for src in [
            "fn b() { use crate::observability::metrics::MediaMetricHandles; }",
            "fn b() { use super::*; }",
            "fn b() { use std::sync::Arc; }",
        ] {
            assert!(check_file("x.rs", src).is_empty(), "must stay clean: {src}");
        }
    }

    #[test]
    fn unresolvable_use_fails_toward_the_finding() {
        let f = check_file("x.rs", "use {;");
        assert!(
            f.iter().any(|x| x.rule == Rule::UnparseableUse),
            "an unreadable `use` must be reported, never passed"
        );
    }

    // ---- No suppression ----

    #[test]
    fn ignore_annotations_are_not_honored() {
        let src = "fn f() { counter!(\"m\"); } // guard:ignore(a sufficiently long reason)";
        assert!(
            !check_file("x.rs", src).is_empty(),
            "no suppression path exists; an annotation must not disarm the deny"
        );
    }

    // ---- Test blocks are in scope ----

    #[test]
    fn macros_inside_cfg_test_blocks_still_fire() {
        let src = "#[cfg(test)]\nmod tests {\n    #[test]\n    fn t() { println!(\"dbg\"); }\n}\n";
        assert!(
            !check_file("x.rs", src).is_empty(),
            "no test-block exemption: an exemption is a hole"
        );
    }

    // ---- Rule ordering ----

    #[test]
    fn rule_order_contains_every_variant_exactly_once() {
        // A variant missing from ORDER gets rank usize::MAX and silently
        // sorts last, which would break both the print order and the REASON
        // precedence at the same time.
        let all = [
            Rule::ManifestMissing,
            Rule::ManifestUnparseable,
            Rule::ScopeNoDirectoriesConfigured,
            Rule::ScopeDirectoryEscapesRoot,
            Rule::ScopeDirectoryMissing,
            Rule::ScopeDirectoryEmpty,
            Rule::UnparseableUse,
            Rule::TelemetryCrateImport,
            Rule::MacroInMediaPath,
        ];
        assert_eq!(Rule::ORDER.len(), all.len());
        for r in all {
            assert_eq!(
                Rule::ORDER.iter().filter(|x| **x == r).count(),
                1,
                "{r:?} must appear exactly once in ORDER"
            );
            assert!(r.rank() < usize::MAX);
        }
    }

    /// Scope and parse conditions must outrank content: a content token tells
    /// the operator "go fix the cited line" when the truth is "the guard
    /// checked nothing".
    #[test]
    fn scope_and_parse_rules_outrank_content_rules() {
        for scope in [
            Rule::ManifestMissing,
            Rule::ManifestUnparseable,
            Rule::ScopeNoDirectoriesConfigured,
            Rule::ScopeDirectoryEscapesRoot,
            Rule::ScopeDirectoryMissing,
            Rule::ScopeDirectoryEmpty,
            Rule::UnparseableUse,
        ] {
            for content in [Rule::TelemetryCrateImport, Rule::MacroInMediaPath] {
                assert!(
                    scope.rank() < content.rank(),
                    "{scope:?} must outrank {content:?}"
                );
            }
        }
    }

    /// `escapes-root` outranks `missing`/`empty` because the `-missing`
    /// remediation ("restore the directory or update the manifest path")
    /// applied to a symlink escape completes the evasion.
    #[test]
    fn escapes_root_outranks_missing_and_empty() {
        assert!(Rule::ScopeDirectoryEscapesRoot.rank() < Rule::ScopeDirectoryMissing.rank());
        assert!(Rule::ScopeDirectoryEscapesRoot.rank() < Rule::ScopeDirectoryEmpty.rank());
    }

    #[test]
    fn tokens_are_distinct_and_prefixed() {
        let mut seen: Vec<String> = Vec::new();
        for r in Rule::ORDER {
            let t = r.token();
            assert!(
                t.starts_with(TOKEN_PREFIX),
                "{t} must carry the subcommand prefix"
            );
            assert!(!seen.contains(&t), "duplicate token {t}");
            seen.push(t);
        }
        // The two scope tokens the task requires be distinguishable by a
        // reader of the STATUS line.
        assert_ne!(
            Rule::ScopeDirectoryMissing.token(),
            Rule::ScopeDirectoryEmpty.token()
        );
    }

    #[test]
    fn only_content_rules_carry_the_findings_suffix() {
        assert!(Rule::MacroInMediaPath.is_content());
        assert!(Rule::TelemetryCrateImport.is_content());
        for r in [
            Rule::ManifestMissing,
            Rule::ScopeDirectoryMissing,
            Rule::ScopeDirectoryEmpty,
            Rule::UnparseableUse,
        ] {
            assert!(!r.is_content(), "{r:?} is a singular condition");
        }
    }

    // ---- Output shape ----

    /// Findings carry the macro SPELLING and the location. Never arguments.
    #[test]
    fn findings_never_carry_argument_text() {
        const SENTINEL: &str = "SENTINEL_MUST_NOT_APPEAR_participant_7f3a";
        let src = format!("fn f() {{ info!(stream = {SENTINEL}, bytes = 1200); }}");
        let found = check_file("x.rs", &src);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].spelling, "info!");
        assert!(
            !found[0].spelling.contains(SENTINEL),
            "argument text must never reach a Finding"
        );
        assert!(!fix_advice(found[0].rule).contains(SENTINEL));
    }

    #[test]
    fn positions_are_one_based_and_survive_blanking() {
        let src = "fn a() {}\n// comment\nfn b() { info!(x); }\n";
        let f = check_file("x.rs", src);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].line, 3);
        assert_eq!(f[0].col, "fn b() { ".len() + 1);
    }

    #[test]
    fn use_classifier_covers_the_grammar_forms() {
        assert_eq!(
            classify_use_line("use crate::x::y;"),
            Some(UseVerdict::Local)
        );
        assert_eq!(
            classify_use_line("use std::sync::Arc;"),
            Some(UseVerdict::AllowedExternal)
        );
        assert_eq!(
            classify_use_line("use ::tracing::info as note;"),
            Some(UseVerdict::Denied {
                root: "tracing".to_string()
            })
        );
        assert_eq!(classify_use_line("fn f() {}"), None);
    }
}
