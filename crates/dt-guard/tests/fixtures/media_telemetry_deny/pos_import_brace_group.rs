//! Brace-group telemetry imports inside the media path.
//!
//! No `use` line under `crates/mh-service/src/media/` uses this shape today,
//! which is precisely why it needs a fixture: a line-shape assumption that
//! happens to hold on the current tree, holding a deny together, is ADR-0036
//! §11's "reads as coverage" in miniature.

use tracing::{info, warn};
use {log::error, std::fmt};
use ::tracing::{field::Empty, Level};

// Invariant: three hits, all `media-telemetry-deny-telemetry-crate-import`.
//
// The second line is the hard one — the denied root is not the first token
// after `use`, it is the first segment of one path INSIDE the group, and the
// group's other member (`std::fmt`) is benign. Classifying the line as a whole
// gets this wrong in both directions.
//
// Any `use` shape the classifier cannot resolve must emit
// `media-telemetry-deny-unparseable-use` and FAIL rather than pass. "A shape I
// did not anticipate" falling through to green is the failure mode this whole
// guard exists to prevent, reproduced inside the guard.
