//! Denied NAMES in non-macro positions. Must be green.
//!
//! This is the anti-vocabulary proof. Every line below would red a
//! name-matching guard and none reds a shape-matching one. ADR-0036 §11:
//! "Vocabulary additions cannot be cited as the protection... The
//! directory-scoped deny catches by *shape*."

pub struct Handles {
    pub error: u64,
    pub info: u64,
    pub span: u64,
}

pub trait Sink {
    fn record(&self, value: f64);
    fn set(&self, value: f64);
}

pub fn on_frame(handles: &Handles) -> u64 {
    let counter = handles.error + 1;
    let gauge = handles.info;
    let span = handles.span;
    let histogram = counter + gauge + span;
    let literal = "counter!(\"x\") and println! and #[instrument]";
    let _ = literal.len();
    histogram
}

// The two negatives that BOUND the 2026-09-08 whitespace widening. They live
// here, in the catalog, and not only in a unit test -- `pos_macro_space_before_bang.rs`
// argues two paragraphs long that a class living only in unit tests reads as
// uncovered to whoever audits next, and that argument applies to the negatives
// that bound a widening just as much as to the positives that motivate it.
pub fn widening_is_bounded(error: u32, v: &[u8]) -> bool {
    // `\b` still holds: a COMPOUND name is not a denied name, however it is
    // spaced. If the widening had dropped the word boundary these would fire.
    my_info !(v);
    frame_counter !(v);
    // `!=` is not an invocation: the character after `!` must be an open
    // delimiter, so no amount of spacing makes this match.
    error != (0)
}

// Invariant: ZERO hits.
//
// `let counter = ...` is not hypothetical — `crates/mh-service/src/media/
// forward.rs:212` is literally `counter.increment(1);`, a local named
// `counter`. A name-based matcher reds the real hot path on line one.
//
// The `literal` binding pins that a denied form inside a string literal is out
// of scope, including `counter!(` which carries the full `!(` anchor.
