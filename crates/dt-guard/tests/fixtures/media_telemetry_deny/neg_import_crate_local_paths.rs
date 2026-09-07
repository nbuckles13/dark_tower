//! Crate-local imports whose paths contain denied tokens. Must be green.
//!
//! All six lines are copied VERBATIM from the real media directory. If anyone
//! ever loosens the import anchor from "first path segment" to a bare-token
//! match on the `use` line, this fixture reds — which is the entire reason the
//! lines are copied rather than paraphrased.

use crate::observability::metrics::{MediaDirection, MediaDropReason, MediaLatencyPhase};
use crate::observability::metrics::MediaMetricHandles;
use crate::observability::metrics::MediaDropReason;
use crate::observability::per_frame_trace;
use self::metrics::Handle;
use super::log::Sink;

// Invariant: ZERO hits.
//
// Five of these are `crate::observability::metrics` — the module that hands out
// the cached handles, i.e. THE pattern the guard exists to protect. Redding
// them is ADR-0036 §11's named failure mode verbatim: "it bans the pattern it
// exists to enforce."
//
// `use crate::observability::per_frame_trace;` is the highest-value line here.
// It is §11's dev-only per-frame tracing facility, whose whole design point is
// living in a SIBLING so the `#[cfg]` decision never enters `media/`. It
// contains the token `trace`. Loosen the anchor and the §11 sibling design
// itself reads as a violation.
//
// The `self::` and `super::` lines pin the keyword exclusions independently of
// `crate::` — the anchor is keyword-based, not punctuation-based, because the
// keyword list is closed and small and the punctuation list is not.
