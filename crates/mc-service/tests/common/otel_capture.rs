//! In-memory OTel span capture + global-propagator installation for MC
//! integration tests (R-56/R-57).
//!
//! Re-exports the shared helpers from `common::observability::testing::otel`
//! (one home for GC/AC/MC/MH), plus MC's deployed `RUST_LOG`.

#[allow(
    unused_imports,
    reason = "shared test module: each test binary uses a subset of these re-exports"
)]
pub use common::observability::testing::otel::{
    install_test_propagator, known_trace_id_hex, known_traceparent, trace_id_hex, SpanCapture,
    KNOWN_SPAN_ID_U64, KNOWN_TRACE_ID_U128,
};

/// MC's deployed `RUST_LOG`, read from the mc-0 manifest.
#[must_use]
pub fn deployed_rust_log() -> String {
    common::observability::testing::otel::deployed_rust_log(
        "infra/services/mc-service/mc-0-deployment.yaml",
    )
}
