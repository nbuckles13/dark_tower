//! A `use` declaration the classifier cannot resolve to a first path segment.
//!
//! Deliberately malformed. Safe to leave un-compilable: unreferenced `.rs`
//! files under `tests/` belong to no cargo target, so neither `cargo fmt
//! --all --check` (Layer 2) nor clippy (Layer 6) sees this file.

use ;
use tracing::{info, ;
use *;

// Invariant: three hits, and the guard FAILS. The per-rule split is
// deliberate and is itself the policy this fixture pins:
//
//   * `use ;`  and  `use *;`   -> `media-telemetry-deny-unparseable-use`
//   * `use tracing::{info, ;`  -> `media-telemetry-deny-telemetry-crate-import`
//
// PRECEDENCE RULE: a `use` line whose first identifier segment resolves to a
// denied crate root reports as the IMPORT violation, even when the remainder
// of the line is malformed. `unparseable-use` is reserved for lines where the
// first segment cannot be determined at all.
//
// That direction is the informative one. We resolved enough to know this line
// is a real violation with a named remedy; reporting it as "a shape I could
// not parse" would replace a specific finding with a vague one and send the
// reader to the wrong runbook row. Degrade to `unparseable-use` only when
// there is genuinely nothing to say.
//
// The unparseable class still has to FAIL rather than fall through to green:
// "a shape I did not anticipate" reported as clean is ADR-0036 §11's "reads as
// coverage", reproduced inside the guard written to prevent it — and it is the
// cheap failure here, because the entire scope is ~40 `use` lines in one small
// directory.
//
// CORRECTED 2026-09-07: this block previously claimed all three hits were
// `unparseable-use`, which would have pinned the opposite precedence by
// omission.
