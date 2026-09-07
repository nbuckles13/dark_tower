//! Canonical home for the tracing / log / print / span macro vocabulary.
//!
//! Exact structural mirror of [`crate::metric_macros`], which is the crate's
//! other macro-family vocabulary: an `ALL` array is the single source of
//! truth, and every alternation / regex below is *derived* from it at
//! `Lazy::new` time. Adding a variant widens every consumer with zero edits
//! at the consumer.
//!
//! # Why this module exists at this address
//!
//! ADR-0034 §6 makes structural duplication of regex patterns a clippy
//! error, but that only binds `Regex::new` call sites — it does not stop two
//! modules declaring byte-identical canonical-home statics. That is exactly
//! what had happened: `\b(info|debug|warn|error|trace)!\s*\(` was declared
//! **twice**, at `rust_pii.rs` and `rust_log_secrets.rs`, and
//! `#\[instrument` **three** times (adding `instrument_skip_all.rs`). The
//! ADR-0036 §11 media-telemetry deny guard would have been the third and
//! fourth. Per `docs/specialist-knowledge/observability/INDEX.md`'s
//! promote-at-the-third-consumer convention, the vocabulary moved here.
//!
//! Lead-approved deviation from the originating task's literal wording
//! (which said "in the new module"), recorded at
//! `docs/devloop-outputs/2026-09-07-media-telemetry-deny-guard/main.md` §12:
//! a third byte-identical declaration would have satisfied the words and
//! defeated the intent of the same sentence ("do not re-inline it
//! elsewhere").
//!
//! # Groups are separate, and that is the load-bearing design
//!
//! Consumers select the [`TelemetryGroup`]s they deny. There is deliberately
//! **no** flat "all telemetry macros" alternation, because a flat union
//! creates a false SSoT: someone narrowing the list to unbreak a PII guard
//! would silently disarm the media-path deny, and someone widening it to
//! strengthen the media deny would silently widen a PII guard and a secrets
//! guard onto `println!`. Group selection makes both impossible by
//! construction rather than by care.
//!
//! # Ownership
//!
//! Membership is **policy content** owned by `observability` (with `security`
//! for the two legacy consumer guards); the module layout, the `ALL`-derives-
//! the-alternation construct, and the matcher shapes are `infrastructure`
//! machinery. See `CLAUDE.md` §Specialists "Guard-crate ownership (interim)".

use once_cell::sync::Lazy;
use regex::Regex;

/// Which family a [`TelemetryMacro`] belongs to.
///
/// Consumers pass a slice of these to [`alternation_for`] and get back only
/// the names in those groups. **Do not add a "all groups" convenience
/// constant** — see the module doc on why the union is deliberately absent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TelemetryGroup {
    /// `tracing` / `log` level macros: `trace!` `debug!` `info!` `warn!` `error!`.
    ///
    /// **BLAST RADIUS — read before changing membership.** This group is
    /// consumed by THREE guards, two of which are not the one you are
    /// probably editing:
    ///
    /// * `rust_pii::LOG_MACRO_RE` and `rust_pii::TRACING_NAMED_RE` — the PII
    ///   guard. Widening this group widens what counts as a PII-bearing log
    ///   site.
    /// * `rust_log_secrets::LOG_MACRO_RE` — the secrets-in-logs guard. Same.
    /// * `media_telemetry_deny` — the ADR-0036 §11 media-path deny.
    ///
    /// Membership is frozen at exactly `{trace, debug, info, warn, error}`
    /// and may not change without sign-off from BOTH `security` and
    /// `observability` (recorded co-signs, 2026-09-07). `log!`, `event!`,
    /// span and print macros are held in their OWN groups precisely so that
    /// promotion could not silently widen a PII guard and a secrets guard.
    Level,
    /// `tracing::event!` — the macro every level macro expands to. Denied by
    /// ADR-0036 §11 by name ("Deny the event macro too, since level macros
    /// expand to it").
    Event,
    /// The `log` crate's generic `log!(Level::Info, ...)` form.
    ///
    /// Deliberately NOT in [`Self::Level`] despite being the same crate
    /// family: `Level`'s membership is frozen against two security-owned
    /// guards, and folding `log` in would widen both.
    LogMacro,
    /// `tracing` span constructors: `span!` plus the five `*_span!` forms.
    Span,
    /// `println!` `print!` `eprintln!` `eprint!` `dbg!`.
    Print,
}

/// One macro name in the telemetry vocabulary.
///
/// Discriminated rather than stringly-typed, matching
/// [`crate::metric_macros::MacroKind`]: consumers match-arm on the variant
/// and the compiler flags any new variant that lands without a handler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TelemetryMacro {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Event,
    Log,
    Span,
    TraceSpan,
    DebugSpan,
    InfoSpan,
    WarnSpan,
    ErrorSpan,
    Println,
    Eprintln,
    Print,
    Eprint,
    Dbg,
}

impl TelemetryMacro {
    /// All variants, in regex-alternation order.
    ///
    /// **Ordering rule**: where one name is a prefix of another, the LONGER
    /// name comes first, so the regex engine prefers the longer alternative
    /// at each position. Same rule and same reason as
    /// [`crate::metric_macros::MacroKind::ALL`]'s describe-first ordering.
    /// Concretely this covers `println`/`print`, `eprintln`/`eprint`, and the
    /// `*_span` forms versus their bare level names.
    ///
    /// The ordering is belt-and-braces rather than strictly required — the
    /// `!` that every consumer anchors after disambiguates `print!` from
    /// `println!` on its own — but the invariant is cheap to hold and
    /// removing it would make correctness depend on every consumer's anchor.
    /// [`tests::alternation_prefers_longer_names`] pins it.
    pub const ALL: &'static [Self] = &[
        // Span forms before bare level names (`trace_span` before `trace`).
        Self::TraceSpan,
        Self::DebugSpan,
        Self::InfoSpan,
        Self::WarnSpan,
        Self::ErrorSpan,
        Self::Span,
        // Level family.
        Self::Trace,
        Self::Debug,
        Self::Info,
        Self::Warn,
        Self::Error,
        // Event + log-crate generic.
        Self::Event,
        Self::Log,
        // Print family: longer forms first.
        Self::Println,
        Self::Eprintln,
        Self::Print,
        Self::Eprint,
        Self::Dbg,
    ];

    /// The bare macro name, without the `!`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Trace => "trace",
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
            Self::Event => "event",
            Self::Log => "log",
            Self::Span => "span",
            Self::TraceSpan => "trace_span",
            Self::DebugSpan => "debug_span",
            Self::InfoSpan => "info_span",
            Self::WarnSpan => "warn_span",
            Self::ErrorSpan => "error_span",
            Self::Println => "println",
            Self::Eprintln => "eprintln",
            Self::Print => "print",
            Self::Eprint => "eprint",
            Self::Dbg => "dbg",
        }
    }

    /// Which family this macro belongs to.
    pub const fn group(self) -> TelemetryGroup {
        match self {
            Self::Trace | Self::Debug | Self::Info | Self::Warn | Self::Error => {
                TelemetryGroup::Level
            }
            Self::Event => TelemetryGroup::Event,
            Self::Log => TelemetryGroup::LogMacro,
            Self::Span
            | Self::TraceSpan
            | Self::DebugSpan
            | Self::InfoSpan
            | Self::WarnSpan
            | Self::ErrorSpan => TelemetryGroup::Span,
            Self::Println | Self::Eprintln | Self::Print | Self::Eprint | Self::Dbg => {
                TelemetryGroup::Print
            }
        }
    }

    /// Parse from a bare macro name. `None` for anything outside the set.
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|m| m.as_str() == s)
    }
}

/// Names in `groups`, in [`TelemetryMacro::ALL`] order.
pub fn names_in_groups(groups: &[TelemetryGroup]) -> Vec<&'static str> {
    TelemetryMacro::ALL
        .iter()
        .filter(|m| groups.contains(&m.group()))
        .map(|m| m.as_str())
        .collect()
}

/// A `|`-joined regex alternation of the names in `groups`, in
/// [`TelemetryMacro::ALL`] order.
///
/// Callers `format!` this into their own pattern shape rather than receiving
/// a finished regex, because the consumers genuinely differ: the PII guard
/// anchors `\b…!\s*\(`, its sibling anchors `tracing::…!\s*\(` with no `\b`,
/// and the media deny accepts `(`, `[` and `{` delimiters.
pub fn alternation_for(groups: &[TelemetryGroup]) -> String {
    names_in_groups(groups).join("|")
}

/// The level-family alternation: `trace|debug|info|warn|error` in `ALL` order.
static LEVEL_ALTERNATION: Lazy<String> = Lazy::new(|| alternation_for(&[TelemetryGroup::Level]));

/// `\b(trace|debug|info|warn|error)!\s*\(` — the historical `LOG_MACRO_RE`
/// shape, now derived from [`TelemetryGroup::Level`].
///
/// **Canonical home for a pattern that was declared twice.** Consumed by
/// `rust_pii` and `rust_log_secrets`, which each carried a byte-identical
/// private copy before 2026-09-07. Behaviour is unchanged: see
/// [`tests::log_macro_re_matches_historical_literal`], which pins the full
/// compiled pattern string — not merely the member set, because a change to
/// the `\b` anchor or the `!\s*\(` suffix alters both guards' behaviour just
/// as much as a membership change would.
///
/// Note the member ORDER differs from the pre-2026-09-07 literal
/// (`ALL` order is `trace|debug|info|warn|error`; the old literal read
/// `info|debug|warn|error|trace`). That is semantically inert for this
/// alternation — no member is a prefix of another, so no input can match
/// two alternatives — and the equality test asserts against the ALL-derived
/// form deliberately, so the SSoT is the array rather than a frozen string.
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
pub static LOG_MACRO_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(&format!(r"\b({})!\s*\(", *LEVEL_ALTERNATION)).expect("static pattern compiles")
});

/// `tracing::(trace|debug|info|warn|error)!\s*\(|#\[instrument` — the
/// historical `rust_pii::TRACING_NAMED_RE` shape.
///
/// Three differences from [`LOG_MACRO_RE`], all deliberate and all preserved
/// verbatim from the pre-promotion literal:
///   1. a literal `tracing::` prefix,
///   2. **no** `\b` anchor,
///   3. a second alternative, `|#\[instrument`.
///
/// Point 3 is the one to be careful with. Dropping it would silently remove
/// `rust_pii` Check 2's instrument-attribute coverage — a **false negative in
/// a PII guard**, which nothing else in the pipeline would notice because the
/// guard would keep reporting clean. [`tests::tracing_named_re_matches_historical_literal`]
/// pins the full compiled string with the alternative explicitly present, so
/// its removal reds the test rather than passing.
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
pub static TRACING_NAMED_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(&format!(
        r"tracing::({})!\s*\(|#\[instrument",
        *LEVEL_ALTERNATION
    ))
    .expect("static pattern compiles")
});

/// `#\[instrument` — the NARROW, historical attribute matcher.
///
/// **This pattern is known-incomplete, and that is why it is named `BARE`.**
/// It does not match the qualified spelling `#[tracing::instrument(...)]`,
/// which is live at four sites today:
///
/// * `crates/gc-service/src/handlers/metrics.rs`
/// * `crates/gc-service/src/handlers/health.rs`
/// * `crates/mh-service/src/session/mod.rs`
/// * `crates/mh-service/src/webtransport/connection.rs`
///
/// It is hosted here anyway, beside the correct [`INSTRUMENT_ATTR_ANY_RE`],
/// so that no private copy of the construct survives outside this module and
/// the narrowness is *documented* rather than silently inherited by whoever
/// reads the next consumer.
///
/// **COUPLING — read before flipping any consumer to `INSTRUMENT_ATTR_ANY_RE`.**
/// Widening the legacy consumers is only safe AFTER the `rust_pii` Check-3
/// fields-scoping fix lands. Check 3 is gated on `!line.contains("skip(")`,
/// and `"skip_all"` does not contain `"skip("`, so a correctly-written
/// `#[tracing::instrument(skip_all, fields(name = ...))]` satisfies all three
/// of its conditions. Flip today and `mh-service/src/webtransport/connection.rs`
/// reds falsely. See `docs/TODO.md` §Observability Debt.
///
/// The coupling is the part that gets forgotten, not the site list.
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
pub static INSTRUMENT_ATTR_BARE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"#\[instrument").expect("static pattern compiles"));

/// `#\[instrument` in any attribute position — the CORRECT attribute matcher.
///
/// Matches the whole attribute body up to the first `]`, so it covers every
/// realistic spelling regardless of path qualification or `cfg_attr` nesting,
/// and across line breaks:
///
/// ```text
/// #[instrument]
/// #[tracing::instrument(skip_all)]
/// #[::tracing::instrument]
/// #[cfg_attr(test, tracing::instrument)]
/// ```
///
/// `\binstrument\b` does not match `instrument_skip_all` — `_` is a word
/// character — so the word boundaries are load-bearing, not decoration.
///
/// **Callers MUST run this over comment- and string-blanked text**
/// (`common::test_code_filter::blank_non_code`). Two reasons: `#[doc =
/// "instrument"]` would otherwise be a false positive, and a doc comment
/// spelling `#[instrument]` in prose would red the very code that documents
/// the ban — which `crates/mh-service/src/media/mod.rs` does today.
///
/// **Known limitation and its failure direction**: `[^\]]*` stops at the
/// first `]`, so an attribute containing a literal `]` outside a string
/// truncates the search window early. That fails AWAY from the finding — a
/// miss, not a spurious hit — which is the wrong direction. Consumers that
/// care must pair this with a conservative second pass; see
/// `media_telemetry_deny::find_instrument_attributes`.
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
pub static INSTRUMENT_ATTR_ANY_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?s)#\[[^\]]*\binstrument\b").expect("static pattern compiles"));

#[cfg(test)]
mod tests {
    use super::*;

    /// Every `ALL` member must appear in an alternation built from its own
    /// group. This is the DERIVATION test: it proves a new variant ripples
    /// out with zero edits at any consumer, which an equality-against-a-
    /// frozen-string test cannot show (that one passes just as happily
    /// against a hand-copied literal).
    #[test]
    fn every_all_member_is_reachable_through_its_group() {
        for m in TelemetryMacro::ALL {
            let alt = alternation_for(&[m.group()]);
            assert!(
                alt.split('|').any(|n| n == m.as_str()),
                "`{}` is in ALL but absent from its own group's alternation — derivation is broken",
                m.as_str()
            );
        }
    }

    /// Group selection must be exclusive: a name never leaks into a group it
    /// does not belong to. This is what makes the promotion safe for the two
    /// security-owned consumers.
    #[test]
    fn groups_do_not_leak_into_each_other() {
        let level = names_in_groups(&[TelemetryGroup::Level]);
        for forbidden in ["log", "event", "span", "println", "dbg", "trace_span"] {
            assert!(
                !level.contains(&forbidden),
                "`{forbidden}` leaked into the Level group — this would widen a PII guard and a secrets guard"
            );
        }
    }

    /// @security + @observability co-signed freeze, 2026-09-07. Changing this
    /// set requires BOTH sign-offs; this test is the forcing function.
    #[test]
    fn level_group_membership_is_frozen() {
        let mut level = names_in_groups(&[TelemetryGroup::Level]);
        level.sort_unstable();
        assert_eq!(level, vec!["debug", "error", "info", "trace", "warn"]);
    }

    /// Full COMPILED PATTERN STRING equality, not a member-set assertion.
    /// A change to the `\b` anchor or the `!\s*\(` suffix alters both
    /// consumer guards' behaviour just as much as a membership change, and a
    /// member-set assertion would sail past it. (@security condition,
    /// 2026-09-07.)
    ///
    /// **`_is_equivalent_to_` and not `_matches_`, deliberately.** The
    /// alternation here is REORDERED relative to the pre-promotion literal —
    /// `ALL` order gives `trace|debug|info|warn|error`, the historical literal
    /// read `info|debug|warn|error|trace`. It is **equivalent, not identical**,
    /// and the guarantee rests on a premise worth stating rather than leaving
    /// implicit: **no `Level` member is a prefix of another**, so no input can
    /// match two alternatives and alternation order cannot be load-bearing
    /// within this group. (`alternation_prefers_longer_names` covers the
    /// CROSS-group prefix pairs; it does not establish this.) The name matters
    /// because these tests are what @security's and @observability's
    /// `Approved-Cross-Boundary` trailers certify against — a name promising
    /// byte-identity over a reordered string over-claims. (@paired-observability
    /// F3, Gate 2.)
    #[test]
    fn log_macro_re_is_equivalent_to_historical_literal() {
        assert_eq!(
            LOG_MACRO_RE.as_str(),
            r"\b(trace|debug|info|warn|error)!\s*\("
        );
    }

    /// Same, for the second template the same member list feeds. The
    /// `|#\[instrument` alternative is written out explicitly so that
    /// dropping it REDS this test rather than passing silently — its removal
    /// would be a false negative in a PII guard introduced by an edit that
    /// looks value-neutral.
    #[test]
    fn tracing_named_re_is_equivalent_to_historical_literal() {
        assert_eq!(
            TRACING_NAMED_RE.as_str(),
            r"tracing::(trace|debug|info|warn|error)!\s*\(|#\[instrument"
        );
    }

    /// The historical behaviour these two statics replaced, asserted as
    /// behaviour rather than as text.
    #[test]
    fn log_macro_re_behaviour_is_unchanged() {
        for s in [
            r#"info!("user = {}", email);"#,
            "debug!(target: \"x\", phone)",
            "error!(?email)",
            "trace!(x)",
            "warn!(y)",
        ] {
            assert!(LOG_MACRO_RE.is_match(s), "should match: {s}");
        }
        assert!(!LOG_MACRO_RE.is_match("my_info!(x)"));
        assert!(!LOG_MACRO_RE.is_match("println!(x)"));
    }

    #[test]
    fn tracing_named_re_behaviour_is_unchanged() {
        assert!(TRACING_NAMED_RE.is_match(r#"tracing::info!("x")"#));
        assert!(TRACING_NAMED_RE.is_match("#[instrument]"));
        assert!(TRACING_NAMED_RE.is_match("#[instrument(skip_all)]"));
        // No `\b`, verbatim from the historical literal: a qualified path
        // still matches because the literal `tracing::` is present.
        assert!(TRACING_NAMED_RE.is_match("crate::tracing::info!(x)"));
        assert!(!TRACING_NAMED_RE.is_match(r#"info!("x")"#));
    }

    #[test]
    fn alternation_prefers_longer_names() {
        let all = alternation_for(&[
            TelemetryGroup::Level,
            TelemetryGroup::Span,
            TelemetryGroup::Print,
        ]);
        let names: Vec<&str> = all.split('|').collect();
        for (long, short) in [
            ("trace_span", "trace"),
            ("println", "print"),
            ("eprintln", "eprint"),
        ] {
            let li = names.iter().position(|n| *n == long);
            let si = names.iter().position(|n| *n == short);
            assert!(
                li < si,
                "`{long}` must precede `{short}` in the alternation (ALL ordering rule)"
            );
        }
    }

    #[test]
    fn instrument_attr_any_covers_realistic_spellings() {
        for s in [
            "#[instrument]",
            "#[tracing::instrument(skip_all)]",
            "#[::tracing::instrument]",
            r#"#[cfg_attr(test, tracing::instrument)]"#,
            "#[cfg_attr(feature = \"x\",\n  instrument\n)]",
        ] {
            assert!(INSTRUMENT_ATTR_ANY_RE.is_match(s), "should match: {s}");
        }
        // `_` is a word char, so the trailing \b rejects the longer ident.
        assert!(!INSTRUMENT_ATTR_ANY_RE.is_match("#[instrument_skip_all]"));
    }

    /// The narrow one is narrow ON PURPOSE. If this test ever fails because
    /// someone "fixed" `INSTRUMENT_ATTR_BARE_RE`, read its doc-comment first:
    /// widening it without the `rust_pii` Check-3 fix reds a live file.
    #[test]
    fn instrument_attr_bare_is_deliberately_narrow() {
        assert!(INSTRUMENT_ATTR_BARE_RE.is_match("#[instrument]"));
        assert!(
            !INSTRUMENT_ATTR_BARE_RE.is_match("#[tracing::instrument(skip_all)]"),
            "BARE is the historical narrow shape; widening it here silently changes three guards"
        );
    }

    #[test]
    fn parse_roundtrips_as_str() {
        for m in TelemetryMacro::ALL {
            assert_eq!(TelemetryMacro::parse(m.as_str()), Some(*m));
        }
        assert_eq!(TelemetryMacro::parse("counter"), None);
        assert_eq!(TelemetryMacro::parse(""), None);
    }
}
