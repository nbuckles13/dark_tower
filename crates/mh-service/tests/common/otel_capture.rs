//! In-memory OTel span capture + global-propagator installation for
//! integration tests (R-55/R-56/R-58).
//!
//! Two pieces:
//!
//! - [`install_test_propagator`] — idempotently registers
//!   `BoundedTraceContextPropagator` as the GLOBAL `opentelemetry`
//!   text-map propagator, exactly as production `init_otel` does. Without
//!   this, `opentelemetry::global::get_text_map_propagator` falls back to
//!   the crate's default `NoopTextMapPropagator` and every inject/extract
//!   silently becomes a no-op regardless of what headers are present —
//!   this is a real footgun for tests that never call `init_otel`.
//! - [`SpanCapture`] — installs a real `tracing_opentelemetry::OpenTelemetryLayer`
//!   backed by `opentelemetry_sdk`'s `InMemorySpanExporter` as the
//!   thread-local default tracing subscriber (via
//!   `tracing::subscriber::set_default`, held for the capture's lifetime),
//!   so spans created during the capture — including ones created inside
//!   `tokio::spawn`'d tasks that share the SAME OS thread (the default
//!   current-thread `#[tokio::test]` runtime) — are exported synchronously
//!   on span end (`with_simple_exporter`) and can be asserted on.
//!
//! Mirrors the technique `common::observability::otel_grpc::tests::
//! with_test_subscriber` uses for its own unit tests, and the capturing
//! exporter pattern `common::observability::otel::tests::
//! build_tracer_provider_installs_resource_via_set_resource` uses — both
//! scoped locally per-test rather than touching global tracer-provider state.

use common::observability::otel_grpc::BoundedTraceContextPropagator;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_sdk::export::trace::SpanData;
use opentelemetry_sdk::testing::trace::InMemorySpanExporter;
use opentelemetry_sdk::trace::TracerProvider;
use std::sync::OnceLock;
use tracing::subscriber::DefaultGuard;
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::layer::SubscriberExt;

/// Idempotently install `BoundedTraceContextPropagator` as the global W3C
/// text-map propagator. Safe to call from multiple tests/threads: the
/// propagator is stateless, so a race between two installs converges on the
/// same (correct) global state either way; `OnceLock` just avoids redundant
/// installs.
pub fn install_test_propagator() {
    static INSTALLED: OnceLock<()> = OnceLock::new();
    INSTALLED.get_or_init(|| {
        opentelemetry::global::set_text_map_propagator(BoundedTraceContextPropagator::new());
    });
}

/// RAII handle for an in-memory-exporter-backed OTel subscriber installed as
/// the current thread's default. Drop restores the previous subscriber.
///
/// **Not `Send`.** `DefaultGuard` is thread-local; hold this in the test
/// function's stack frame and don't move it across an `.await` that could
/// hop threads (fine under the default current-thread `#[tokio::test]`
/// runtime, which never moves a test's task to another OS thread).
#[must_use]
pub struct SpanCapture {
    exporter: InMemorySpanExporter,
    _guard: DefaultGuard,
    // Keeps the TracerProvider (and its registered exporter) alive for the
    // capture's lifetime; spans reference it indirectly via the tracer.
    _provider: TracerProvider,
}

impl SpanCapture {
    /// Install the capturing subscriber as this thread's default.
    pub fn install() -> Self {
        let exporter = InMemorySpanExporter::default();
        let provider = TracerProvider::builder()
            .with_simple_exporter(exporter.clone())
            .build();
        let tracer = provider.tracer("mh-service-test");
        let otel_layer = OpenTelemetryLayer::new(tracer);
        let subscriber = tracing_subscriber::registry().with(otel_layer);
        let guard = tracing::subscriber::set_default(subscriber);
        Self {
            exporter,
            _guard: guard,
            _provider: provider,
        }
    }

    /// Return every span exported so far. `SimpleSpanProcessor` exports
    /// synchronously on span end, so this reflects all spans that have
    /// closed by the time this is called — no explicit flush needed.
    pub fn finished_spans(&self) -> Vec<SpanData> {
        self.exporter
            .get_finished_spans()
            .expect("in-memory span exporter lock poisoned")
    }

    /// Find the first exported span with the given name.
    pub fn find_span(&self, name: &str) -> Option<SpanData> {
        self.finished_spans().into_iter().find(|s| s.name == name)
    }
}

/// Format an OTel `TraceId` as the lowercase 32-hex-digit form used in W3C
/// `traceparent` headers, for comparing against a known injected trace id.
pub fn trace_id_hex(span: &SpanData) -> String {
    format!("{:032x}", span.span_context.trace_id())
}
