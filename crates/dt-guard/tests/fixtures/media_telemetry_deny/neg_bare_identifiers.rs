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

// Invariant: ZERO hits.
//
// `let counter = ...` is not hypothetical — `crates/mh-service/src/media/
// forward.rs:212` is literally `counter.increment(1);`, a local named
// `counter`. A name-based matcher reds the real hot path on line one.
//
// The `literal` binding pins that a denied form inside a string literal is out
// of scope, including `counter!(` which carries the full `!(` anchor.
