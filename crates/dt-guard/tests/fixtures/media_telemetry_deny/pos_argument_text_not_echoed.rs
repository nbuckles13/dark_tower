//! Denied macros whose ARGUMENTS carry the exact data ADR-0036 §11 forbids
//! retaining. The guard must report the macro spelling and `path:line` and
//! must never echo the argument text.
//!
//! Why this fixture exists: a guard that prints the offending line to prove it
//! found something copies per-participant identifiers, frame sizes and
//! timestamps into CI logs, build artifacts and their retention windows. §11
//! names that data precisely — "no per-frame, per-participant, or
//! per-stream-identity dimension in media-path logs, metric labels, or span
//! attributes", and "the time-ordered sequence of sizes for a single stream is
//! the voice-activity trace". A detector that leaks what it detects has moved
//! the leak, not closed it.
//!
//! The sentinel strings below are deliberately self-describing: anyone who ever
//! finds one in a CI log knows immediately what regressed and where to look.

pub fn on_frame(payload_length: usize, queued_at_micros: u64) {
    counter!(
        "mh_media_frames_forwarded_total",
        "sender" => "SENTINEL_MUST_NOT_APPEAR_IN_GUARD_OUTPUT_participant_7f3a",
        "payload_bytes" => payload_length.to_string(),
        "queued_at_micros" => queued_at_micros.to_string()
    )
    .increment(1);

    info!(
        sender = "SENTINEL_MUST_NOT_APPEAR_IN_GUARD_OUTPUT_participant_7f3a",
        payload_bytes = payload_length,
        queued_at_micros,
        "frame forwarded"
    );

    println!(
        "SENTINEL_MUST_NOT_APPEAR_IN_GUARD_OUTPUT_participant_7f3a {payload_length} {queued_at_micros}"
    );
}

// Invariant: three hits, all `media-telemetry-deny-macro-in-media-path`.
//
// AND — the load-bearing half — the literal
// `SENTINEL_MUST_NOT_APPEAR_IN_GUARD_OUTPUT` must appear NOWHERE in FOUR
// channels. Assert absence in each explicitly; do not infer it from the hit
// count, and do not assert only the channel that is easiest to capture:
//
//   1. stdout — the per-finding `VIOLATION:` records.
//   2. stderr — captured into the same stream by `run-guards.sh` (`2>&1`), so
//      a leak here is no less public than one on stdout.
//   3. `--explain` — the verbose path EXISTS to say more, so it is the channel
//      most likely to grow an excerpt, and the one a reader assumes is safe
//      because it is opt-in. It is not opt-in: an operator debugging a red
//      guard runs it, and its output lands in the same terminal scrollback.
//   4. the non-zero-exit SUMMARY line — @security's addition, and the one I
//      had missed. Summary lines are where `3 violations in forward.rs: ...`
//      grows a helpful excerpt six months later, long after the per-finding
//      output was reviewed for exactly this.
//
// Downstream filtering in `scripts/guards/run-guards.sh` is a display
// convenience, NOT a redaction boundary, and must not be argued as mitigation
// for a leak here. Two properties of its design, not of any line in it: the
// non-verbose re-emission grep preserves every finding line by construction,
// so it keeps exactly the line an excerpt would be added to; and the verbose
// branch does not filter at all, which is the path an operator takes when
// debugging a red guard. Invoking the binary directly per the runbook triage
// path bypasses the runner entirely.
//
// Stated as design properties on purpose. Reproducing the grep predicate or
// its line numbers here would be a second encoding of a file this fixture does
// not own, with nothing binding the two — it would keep asserting the old
// behaviour, in confident detail, the first time that file is edited. Read
// `run-guards.sh` for the mechanism. The control has to be HERE.
//
// The three call sites are deliberately different shapes, because each has its
// own way of tempting an implementation into echoing:
//
//   * `counter!` — the argument is a label map, and the natural diagnostic
//     ("which label is the problem?") prints the map.
//   * `info!` — the argument is a structured field list, and the natural
//     diagnostic prints the fields.
//   * `println!` — the argument is a format string, and the natural diagnostic
//     prints the line because there is nothing else to print.
//
// All three are multi-line invocations, which additionally pins that the
// balanced-paren walker's captured body never reaches an output channel.
//
// The trio is also §11's leak set exactly: stream identity, payload size, and
// timing. Reporting `counter!(` at `path:line` is sufficient for an implementer
// to fix it — the argument text adds nothing a maintainer needs and everything
// an adversary does.
