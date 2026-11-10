//! `mh_webtransport_session_closes_total` assertions shared by the close-code
//! test binaries (ADR-0032 semantics; the label set iterates `CloseCode::ALL`).

use common::observability::testing::MetricSnapshot;
use mh_service::webtransport::close::CloseCode;

/// The series moved by exactly `times` on `expected` and by 0 on every other code.
pub fn assert_closed_only(snap: &MetricSnapshot, expected: CloseCode, times: u64) {
    for code in CloseCode::ALL {
        snap.counter("mh_webtransport_session_closes_total")
            .with_labels(&[("code", code.as_label())])
            .assert_delta(if code == expected { times } else { 0 });
    }
}

/// Hard form (ADR-0032 unobserved): the path never touched the series.
pub fn assert_no_close_counted(snap: &MetricSnapshot) {
    for code in CloseCode::ALL {
        snap.counter("mh_webtransport_session_closes_total")
            .with_labels(&[("code", code.as_label())])
            .assert_unobserved();
    }
}
