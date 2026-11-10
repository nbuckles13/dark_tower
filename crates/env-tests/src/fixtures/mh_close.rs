//! MH session-close observation for env-tests (ADR-0039 §7).
//!
//! Thin by design: the vendored wtransport accessor plus a comparison. The
//! primary observation is `Connection::session_close_received()` — filled only
//! by the transport's `CLOSE_WEBTRANSPORT_SESSION` parser — because only that
//! capsule reaches a browser as `WebTransportCloseInfo.closeCode`; a QUIC close
//! carrying the same number would read as transport loss there, while
//! `closed()` on a Rust client could not tell the two apart.
//!
//! A deliberate second copy of mh-service's `tests/common/wt_client.rs`
//! observer: env-tests link no service or test-utils crate (ADR-0028), and both
//! copies take the code as a proto-gen `MhCloseCode`, never a numeric literal.

use proto_gen::dark_tower::signaling::v1::MhCloseCode;
use std::time::Duration;

/// Assert MH closed the session with `expected`: a session-close capsule with
/// exactly that code and an EMPTY message (numeric only, no reason string).
pub async fn expect_session_close(
    conn: &wtransport::Connection,
    expected: MhCloseCode,
    within: Duration,
    ctx: &str,
) {
    let received = tokio::time::timeout(within, conn.session_close_received())
        .await
        .unwrap_or_else(|_| panic!("{ctx}: MH did not end the session within {within:?}"));
    let close = received.unwrap_or_else(|| {
        panic!(
            "{ctx}: the session ended WITHOUT a CLOSE_WEBTRANSPORT_SESSION capsule — to a browser \
             that is transport loss, never {expected:?}. Is the MH image built with the \
             vendored wtransport patch (vendor/patches/wtransport/PATCH.md)?"
        )
    });
    assert_eq!(
        close.code().into_inner(),
        u64::from(i32::from(expected).unsigned_abs()),
        "{ctx}: wrong close code (expected {expected:?})"
    );
    assert!(
        close.reason().is_empty(),
        "{ctx}: the close message must be empty, got {} bytes",
        close.reason().len()
    );
}
