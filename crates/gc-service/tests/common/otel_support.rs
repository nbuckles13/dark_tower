//! Shared OTel test-environment helpers for GC's R-56 trace-continuity
//! integration tests (`otel_ac_trace_continuity.rs`,
//! `otel_http_grpc_bridge.rs`, `otel_telemetry_proxy_trace_context.rs`,
//! `otel_grpc_inbound_continuity.rs`).
//!
//! Mirrors the test-support patterns already established in
//! `common::observability::otel_grpc`'s and `otel_http`'s own in-module unit
//! tests, hoisted here since multiple integration test BINARIES need it.

use opentelemetry::trace::TracerProvider as _;
use opentelemetry_sdk::trace::TracerProvider;
use std::sync::OnceLock;
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::layer::SubscriberExt;

/// Install the globally-registered `BoundedTraceContextPropagator` exactly
/// once per test binary process (each `tests/*.rs` file is its own binary,
/// so this only needs to coordinate WITHIN one file's parallel test
/// threads). Idempotent-safe to call from every test — `OnceLock`
/// guarantees the actual `set_text_map_propagator` call happens once.
pub fn ensure_global_propagator_installed() {
    static INSTALLED: OnceLock<()> = OnceLock::new();
    INSTALLED.get_or_init(|| {
        opentelemetry::global::set_text_map_propagator(
            common::observability::otel_grpc::BoundedTraceContextPropagator::new(),
        );
    });
}

/// RAII guard: while held, the current OS thread's default `tracing`
/// subscriber has an `OpenTelemetryLayer` wired to a real (non-exporting)
/// `TracerProvider`. Required for `tracing::Span::current().context()` /
/// `.set_parent()` (used throughout `otel_http`/`otel_grpc`) to actually
/// carry W3C trace context, rather than being no-ops against a subscriber
/// with no OTel bridge. Thread-local — safe to call independently from
/// parallel tests (each on its own OS thread under `cargo test`'s default
/// thread-per-test model); only the process-wide propagator (see
/// [`ensure_global_propagator_installed`]) needs cross-test coordination.
///
/// Callers MUST run on a `#[tokio::test]` with the default (`current_thread`)
/// runtime flavor so `.await` points don't hop OS threads and drop this
/// thread-local override early.
#[must_use = "the subscriber override is only active while this guard is held"]
pub struct TestOtelSubscriberGuard {
    _guard: tracing::subscriber::DefaultGuard,
}

pub fn install_test_otel_subscriber() -> TestOtelSubscriberGuard {
    let provider = TracerProvider::builder().build();
    let tracer = provider.tracer("gc-service-otel-integration-test");
    let layer = OpenTelemetryLayer::new(tracer);
    let subscriber = tracing_subscriber::registry().with(layer);
    TestOtelSubscriberGuard {
        _guard: tracing::subscriber::set_default(subscriber),
    }
}

/// Convenience: install both the global propagator and a scoped OTel
/// subscriber. Most tests want both together.
#[must_use = "the subscriber override is only active while this guard is held"]
pub fn setup_otel_test_environment() -> TestOtelSubscriberGuard {
    ensure_global_propagator_installed();
    install_test_otel_subscriber()
}

/// Like [`setup_otel_test_environment`], but the `TracerProvider` is wired to
/// an `InMemorySpanExporter` (via `with_simple_exporter`, so spans are
/// exported synchronously on end — no batching delay to wait out) instead of
/// a no-op provider. Returns the guard AND the exporter handle so the caller
/// can inspect actually-finished spans (e.g. assert a specific span's
/// `trace_id`) rather than only observing indirect effects (call success).
#[must_use = "the subscriber override is only active while this guard is held"]
pub fn setup_otel_test_environment_with_exporter() -> (
    TestOtelSubscriberGuard,
    opentelemetry_sdk::testing::trace::InMemorySpanExporter,
) {
    ensure_global_propagator_installed();

    let exporter = opentelemetry_sdk::testing::trace::InMemorySpanExporter::default();
    let provider = TracerProvider::builder()
        .with_simple_exporter(exporter.clone())
        .build();
    let tracer = provider.tracer("gc-service-otel-integration-test");
    let layer = OpenTelemetryLayer::new(tracer);
    let subscriber = tracing_subscriber::registry().with(layer);
    let guard = TestOtelSubscriberGuard {
        _guard: tracing::subscriber::set_default(subscriber),
    };
    (guard, exporter)
}

/// A fixed, valid W3C `traceparent` trace-id (version 00, sampled) usable
/// wherever a test needs a known, injectable inbound header.
pub const KNOWN_TRACE_ID_HEX: &str = "4bf92f3577b34da6a3ce929d0e0e4736";
/// Matching fixed parent span-id for the same fixture.
pub const KNOWN_PARENT_SPAN_ID_HEX: &str = "00f067aa0ba902b7";

/// Build the full W3C `traceparent` header value for the fixed trace-id.
pub fn known_traceparent() -> String {
    format!("00-{KNOWN_TRACE_ID_HEX}-{KNOWN_PARENT_SPAN_ID_HEX}-01")
}

/// Extract the 32-hex-char trace-id portion from a W3C `traceparent` header
/// value (`"{version}-{trace_id}-{parent_id}-{flags}"`). Returns `None` if
/// the value is too short to contain one — callers should assert `Some`.
pub fn trace_id_from_traceparent(traceparent: &str) -> Option<&str> {
    traceparent.get(3..35)
}
