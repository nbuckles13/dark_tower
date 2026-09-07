//! Macros deliberately NOT denied. Must be green.
//!
//! Recorded as a decision rather than left as an omission. A deny that reaches
//! load-bearing `Display` / `Write` machinery and control-flow macros is a deny
//! people route around, and a routed-around guard protects nothing.

use core::fmt;

pub struct Frame {
    len: usize,
}

impl fmt::Display for Frame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "frame(len={})", self.len)?;
        writeln!(f, " ok")
    }
}

pub fn on_frame(frame: &Frame) -> String {
    assert!(frame.len > 0, "empty frame");
    assert_eq!(frame.len % 2, 0);
    debug_assert!(frame.len < 4096);
    if frame.len > 65_535 {
        panic!("frame exceeds cap");
    }
    format!("len={}", frame.len)
}

// Invariant: ZERO hits.
//
// `write!` / `writeln!` are the interesting exclusion: they DO emit text, and a
// reader who knows only "this guard denies output macros" will expect them to
// red. They are excluded because `fmt::Display` cannot be implemented without
// them, so denying them bans a language feature rather than a telemetry
// pattern. "Why didn't it catch my `writeln!`" is a legible question and the
// answer is findable in the runbook, not just here.
//
// `panic!` / `assert*!` are not telemetry: they abort rather than emit into the
// shipping pipeline, and `#[should_panic]` tests depend on them.
