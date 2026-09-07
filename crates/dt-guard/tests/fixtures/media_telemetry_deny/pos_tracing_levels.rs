//! The five tracing level macros inside the media path.
//!
//! `trace!` is in the list on purpose — `crates/mh-service/src/media/mod.rs`
//! names `tracing::trace!` by name as the thing that must not appear, and a
//! level list that stopped at `debug!` would be a partial vocabulary.

pub fn on_frame(sender: u64, payload_length: usize) {
    trace!(sender, payload_length, "frame in");
    debug!(?payload_length, "frame sized");
    info!("frame forwarded");
    warn!(sender, "slow subscriber");
    error!(sender, "egress refused");
    tracing::info!(sender, "qualified spelling");
    log::warn!(sender, "log-crate spelling of the same name");
}

// Invariant: seven hits, all `media-telemetry-deny-macro-in-media-path`.
// The last two pin that a `tracing::` or `log::` path qualifier does not
// evade the bare-name alternation. LOG_CRATE and LEVEL deliberately share
// these five names and the alternation is deduped — if a future edit splits
// them into two independently-maintained lists, this fixture still passes,
// so the dedup itself is pinned by a unit test rather than here.
