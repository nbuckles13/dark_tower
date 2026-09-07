//! Rust lifetimes must not blank the rest of the line.
//!
//! Regression fixture for a LIVE BYPASS of the primary control, reproduced by
//! @security on 2026-09-07 against the built binary with the real manifest.
//!
//! `lex_line` treated every `'` as a string-literal opener. A lifetime has one
//! tick and no closer, so `in_string` stayed true to end-of-line and every
//! denied form to its right was blanked — invisible to BOTH matchers.
//!
//! **What actually reds on reversion, and why the earlier draft of this
//! sentence was wrong** (@test, 2026-09-07): the bug only reaches a denied form
//! SHARING the lifetime's line, because `lex_line` resets `in_string` per line.
//! Cases (a) and (e) are the same-line macro and import shapes and are the two
//! that drop when the fix is reverted. The cross-line cases (b)(c)(d)(h) fire
//! under the buggy lexer too and document the per-line-reset safety property;
//! (f)(g) are same-line char literals that guard the opposite over-correction.
//! An earlier draft claimed "four of eight passed clean before the fix", which
//! described @security's original same-line repro, not this fixture — whose
//! cases had been split cross-line and would all have fired under the bug.
//!
//! Two things made this the finding of the loop rather than a bug:
//!
//!   1. BOTH layers fail together. The import deny exists as the mitigation
//!      for the alias evasion the invocation deny structurally cannot see
//!      (`use tracing::info as note;` + `note!(..)`). After an unpaired tick
//!      neither fires — no coverage, not degraded coverage.
//!   2. It was live in the protected directory. `media/forward.rs` carries
//!      `fmt::Formatter<'_>` and `enum FrameSource<'a>` in code position
//!      today. Nothing was missed only because no denied form happened to sit
//!      to their right — line layout, not the control working. And
//!      `&'static str` is the ADR-0029 mandated bounded-label type, i.e. the
//!      construct most likely to share a line with telemetry added later.

use crate::media::forwarder::ConnectionForwarder;

// (a) THE ACTUAL BUG SHAPE, and the case that gives this fixture its teeth: a
// bare lifetime immediately followed by a denied macro ON THE SAME LINE.
// Reverting the `'`-discrimination fix reds THIS line — `&'static str`'s
// unpaired tick opens a string and blanks `tracing::info!` to end-of-line.
//
// Cross-line placement (lifetime in a signature, macro on a LATER line) does
// NOT pin the bug: `lex_line` resets `in_string` per line, so a tick on line N
// cannot reach a macro on line N+1. Cases (b)(c)(d)(h) are cross-line and fire
// under the buggy lexer too — they document that the per-line reset is safe,
// not that the discrimination works. Only same-line cases test the fix.
pub fn a(s: &'static str) { tracing::info!("A {}", s); }

// (b) Balanced ticks in a generic bound. Caught even before the fix, kept so a
// regression cannot pass by only handling the unbalanced case.
pub fn b<'x>(s: &'x str) {
    tracing::info!("B {}", s);
}

// (c) Anonymous lifetime. `'_` is a LIFETIME; `'_'` is a char literal. The
// discriminator is the trailing quote, and this pins that we read it that way.
pub fn c(f: &mut core::fmt::Formatter<'_>) {
    let _ = f;
    counter!("mh_media_frames_forwarded_total").increment(1);
}

// (d) Lifetime inside a generic argument, then a print-family macro.
pub fn d() {
    let v: Vec<&'static str> = vec![];
    println!("D {}", v.len());
}

// (e) THE IMPORT HALF OF THE SAME BUG, same-line, so it too reds on reversion.
// `&'static str` then a renamed `use` on one line: the buggy lexer blanks the
// `use`, and the import deny — itself the mitigation for the alias evasion the
// invocation deny cannot see — goes blind. BOTH matchers fail together on this
// line, which is what made the bypass total rather than partial. (The `use` is
// also mid-line, so it exercises `classify_use_line`'s split path as well.)
pub fn e(s: &'static str) { use tracing::info as note; let _ = (s, note); }

// (f) A char literal whose CONTENT would corrupt lexing, on the SAME LINE as a
// denied form. Same-line placement is load-bearing: `lex_line` initialises
// `in_string = false` per line and only block-comment state carries across
// lines, so a char literal can only influence a denied form sharing its line.
//
// This pins the MIS-FIX direction that (a)/(e) do not: if someone "simplifies"
// the fix by never entering string mode for `'`, the `"` inside this char
// literal opens string mode, blanks the rest of the line, `histogram!` is
// missed -> count drops, this fixture reds.
//
// It does NOT pin the original bug: the pre-fix lexer CLOSED a char literal on
// its next tick (`string_quote = '\''`), so `'"'` closed and `histogram!` fired
// even before the fix. @test measured this (2026-09-07). The original bug is
// pinned by (a)/(e); this case guards the opposite over-correction. Both
// directions matter, which is why the fix must DISTINGUISH, not disable.
pub fn f(s: &'static str) {
    let _ = s;
    let quote = '"'; histogram!("mh_media_forward_latency_seconds").record(0.0);
    let _ = quote;
}

// (g) The ambiguous ESCAPE, on the same line as a denied form. `'\n'` would
// pin nothing — the escape that matters is `'\''`, where a naive lexer reads
// the escaped tick as the closing delimiter, leaving the real terminator to
// open a fresh string that blanks the rest of the line.
//
// Correct lexing closes the char literal at the true terminator and
// `debug_span!` is a hit. A lexer that mishandles the escape misses it ->
// count drops, this fixture reds.
pub fn g() {
    let tick = '\''; debug_span!("G");
    let _ = tick;
}

// (h) Two denied forms after one tick — the second must not be swallowed by
// the first being reported.
pub fn h(s: &'static str) {
    dbg!(s);
    eprintln!("H2");
}

pub fn touch(fwd: &ConnectionForwarder) {
    let _ = fwd;
}

// Invariant: NINE hits total —
//   eight `media-telemetry-deny-macro-in-media-path`:
//     (a) tracing::info!, (b) tracing::info!, (c) counter!, (d) println!,
//     (f) histogram!, (g) debug_span!, (h) dbg!, (h) eprintln!
//   plus ONE `media-telemetry-deny-telemetry-crate-import` for (e)'s
//     `use tracing::info as note;`.
//
// `note` itself is NOT counted: the invocation matcher is a deny LIST and
// `note` is not on it. That residual is stated in the module doc and is why
// the import deny exists at all.
//
// Cases (f) and (g) are ONE LINE EACH ON PURPOSE, and that is the property the
// block previously described wrongly. `lex_line` initialises `in_string = false`
// per line — only block-comment state carries across lines — so a char literal
// on line N cannot affect lexing on line N+1 under ANY implementation, correct
// or broken. A char-literal case pins something only when the denied form
// shares its line. Split either of them across two lines and the case still
// passes while proving nothing.
