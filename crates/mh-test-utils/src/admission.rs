//! Stream-admission fixtures for `mh-service`'s `tests/` binaries.
//!
//! `SessionManagerHandle::new_for_tests` requires a [`StreamAdmission`] — a session
//! manager with no stream ceiling is not representable (story 2 R-19). Tests
//! that are not ABOUT admission use [`never_binding`]; tests that are build
//! their own ceiling at the call site so the number under test is visible
//! there.
//!
//! `src/` unit tests use the twin `mh_service::session::admission::
//! test_admission` instead: through the dev-dependency they see a different
//! `mh_service` crate instance, so this fixture's type is not theirs.

use mh_service::config::MAX_TOTAL_EGRESS_EDGES_CEILING;
use mh_service::session::StreamAdmission;

/// An ARBITRARY rejection-ratio threshold for fixtures. It happens to equal the
/// deployed `MH_EGRESS_REJECTION_RATIO_THRESHOLD`; that is coincidence, not a
/// requirement — no fixture test depends on the threshold's value. Named so a
/// reader does not mistake a matching number for a load-bearing one.
const FIXTURE_REJECTION_RATIO_THRESHOLD: f64 = 0.05;

/// An admission whose ceiling no fixture can reach: the largest ceiling config
/// load can ever produce (it is bounded by the edge-bound ceiling).
#[must_use]
pub fn never_binding() -> StreamAdmission {
    StreamAdmission {
        stream_ceiling: MAX_TOTAL_EGRESS_EDGES_CEILING,
        rejection_ratio_threshold: FIXTURE_REJECTION_RATIO_THRESHOLD,
    }
}

/// An admission with an explicit stream ceiling.
#[must_use]
pub fn with_ceiling(stream_ceiling: usize) -> StreamAdmission {
    StreamAdmission {
        stream_ceiling,
        rejection_ratio_threshold: FIXTURE_REJECTION_RATIO_THRESHOLD,
    }
}

/// Generous ADR-0036 §8 policy bounds for `tests/` binaries that are not ABOUT
/// a bound. `PolicyLimits` has no `Default` — its six keys are required reads
/// and a default would be the second encoding that removed — so the fixture
/// lives here, named as a fixture. The `src/` twin is
/// `PolicyLimits::for_tests` (same crate-instance reason as above).
///
/// **These numbers equal the deployed ConfigMap values BY HISTORY, NOT BY
/// REQUIREMENT** — they were the code defaults story 2 task 8 deleted. Any
/// generous value would do. DO NOT collapse this and its twin into one shared
/// constant: that resurrects, through the test surface, the default the
/// required read removed.
#[must_use]
pub fn fixture_policy_limits() -> mh_service::config::PolicyLimits {
    mh_service::config::PolicyLimits {
        max_egress_streams_per_meeting: 512,
        max_candidate_sources_per_egress: 16,
        max_total_egress_edges: 65_536,
        policy_apply_timeout_ms: 1_000,
        max_registered_meetings: 8_192,
        max_source_streams_per_meeting: 1_024,
    }
}

/// The stream-path limits at their deployed values
/// (`infra/services/mh-service/config.env`), for rigs that build a `Config` or
/// a `MediaSetup` by hand. Tests ABOUT the stream path set their own.
#[must_use]
pub fn fixture_stream_path_limits() -> mh_service::config::StreamPathLimits {
    mh_service::config::StreamPathLimits {
        opens_per_connection_per_sec: 10,
        egress_stream_backlog_bytes: 262_144,
        mid_frame_stall: std::time::Duration::from_millis(1000),
    }
}

/// The ADR-0036 §1 QUIC transport parameters at their deployed values
/// (`infra/services/mh-service/config.env`), for rigs that build a `Config`
/// or bind an endpoint by hand, and for bounds a test derives from them.
#[must_use]
pub fn fixture_quic_transport() -> mh_service::config::QuicTransportParams {
    mh_service::config::QuicTransportParams {
        max_concurrent_uni_streams: 64,
        datagram_send_buffer_audio_frames: 32,
        datagram_send_buffer_bytes: 32 * mh_service::config::NOMINAL_AUDIO_FRAME_BYTES,
        keepalive_interval_ms: 10_000,
    }
}
