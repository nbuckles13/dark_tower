//! Sizing a video payload so that a HELD subscriber trips MH's
//! per-destination backlog cap within one fixture group (story 3 R-12, R-21):
//! shared by every env-test that drives MH's `subscriber_backlog` abandonment
//! (38's backlog row, 39's backlog-recovery row), so the sizing has one home.

/// MH's declared connection send window —
/// `mh_service::config::CONNECTION_SEND_WINDOW_BYTES`. env-tests cannot import
/// mh-service (ADR-0028), so it is restated here.
// Drift-guarded: `mh-service`'s config tests read this line and fail if it
// differs from `CONNECTION_SEND_WINDOW_BYTES`.
pub const MH_CONNECTION_SEND_WINDOW_BYTES: u64 = 524_288;

/// The fixture client's per-stream receive window — quinn 0.11's default
/// `STREAM_RWND` (12 500 x 100 B), which `mc_session::try_connect_wt_with`
/// inherits (it sets no transport config).
pub const CLIENT_STREAM_RECEIVE_WINDOW_BYTES: u64 = 1_250_000;

/// Margin over the bytes a held destination can absorb before MH's cap trips.
pub const BACKLOG_MARGIN: u64 = 2;

/// The payload length per frame such that `frames_per_group` frames carry
/// [`BACKLOG_MARGIN`] x everything a held destination can absorb (client
/// stream window + MH send window + MH's deployed `backlog_cap`).
///
/// # Panics
///
/// PRECONDITION: when the deployed cap makes a reset unreachable within one
/// group at `<= MAX_PAYLOAD_BYTES` per frame.
#[must_use]
pub fn backlog_payload_len(backlog_cap: u64, frames_per_group: u64) -> usize {
    let absorbable =
        CLIENT_STREAM_RECEIVE_WINDOW_BYTES + MH_CONNECTION_SEND_WINDOW_BYTES + backlog_cap;
    let per_group = BACKLOG_MARGIN * absorbable;
    let payload_len = usize::try_from(per_group / frames_per_group).expect("fits");
    assert!(
        payload_len < media_protocol::frame::MAX_PAYLOAD_BYTES,
        "PRECONDITION: the deployed backlog cap ({backlog_cap} B) makes a reset unreachable within \
         one fixture group at <= MAX_PAYLOAD_BYTES per frame; raise frames_per_group"
    );
    payload_len
}
