//! A denied macro inside a `#[cfg(test)] mod tests` block in the media path.
//!
//! Test modules under `media/` are IN SCOPE, with no exemption. Two independent
//! routes led to the same ruling:
//!
//!   * `common::test_code_filter::is_test_path()` matches any `/fixtures/`
//!     segment, so applying the standard filter would skip every fixture in this
//!     directory, turn every `pos_*` case green, and make the entire "does it
//!     fire" half vacuous while the guard reported clean.
//!   * `compute_test_block_ranges()` is documented fail-safe-BROAD (it extends
//!     to EOF on unbalanced braces), so a `#[cfg(test)]` block could silently
//!     swallow production lines below it.

pub fn forward_one(payload_length: usize) -> usize {
    payload_length
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forwards() {
        println!("debugging the forward path");
        assert_eq!(forward_one(3), 3);
    }
}

// Invariant: one hit, `media-telemetry-deny-macro-in-media-path`, on the
// `println!`. `assert_eq!` is not denied.
//
// The consequence is real and is a stated decision, not an oversight: a debug
// `println!` in a media test module reds Layer 3. It costs nothing today — the
// five `#[cfg(test)]` blocks under `media/` (`queue.rs:212`, `sampler.rs:72`,
// `caps.rs:63`, `forwarder.rs:265` and `:274`) use no denied macro. It is
// recorded in `media/mod.rs` and in the runbook so it is read before it is hit.
