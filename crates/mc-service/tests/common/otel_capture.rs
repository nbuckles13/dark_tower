//! In-memory OTel span capture + global-propagator installation for
//! integration tests (R-56/R-57).
//!
//! Two pieces:
//!
//! - [`install_test_propagator`] — idempotently registers
//!   `BoundedTraceContextPropagator` as the GLOBAL `opentelemetry` text-map
//!   propagator, exactly as production `init_otel` does. Without this,
//!   `opentelemetry::global::get_text_map_propagator` falls back to the crate's
//!   default `NoopTextMapPropagator` and every inject/extract silently becomes a
//!   no-op regardless of what headers are present.
//! - [`SpanCapture`] — installs a real `tracing_opentelemetry::OpenTelemetryLayer`
//!   backed by `opentelemetry_sdk`'s `InMemorySpanExporter` as the thread-local
//!   default tracing subscriber (via `tracing::subscriber::set_default`, held for
//!   the capture's lifetime), so spans created during the capture — including
//!   ones created inside `tokio::spawn`'d tasks that share the SAME OS thread
//!   (the default current-thread `#[tokio::test]` runtime) — are exported
//!   synchronously on span end and can be asserted on.
//!
//! Third copy of the GC/MH pattern (`gc-service/tests/common/otel_support.rs`,
//! `mh-service/tests/common/otel_capture.rs`) per the DRY adjudication for
//! task #6 — kept local rather than hoisted while the shape settles.

use common::observability::otel_grpc::BoundedTraceContextPropagator;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_sdk::export::trace::SpanData;
use opentelemetry_sdk::testing::trace::InMemorySpanExporter;
use opentelemetry_sdk::trace::TracerProvider;
use std::sync::OnceLock;
use tracing::subscriber::DefaultGuard;
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::layer::SubscriberExt;

/// Fixed W3C trace/span ids shared across the OTel test files so continuity
/// assertions compare against a known value (matches GC/MH exemplars).
pub const KNOWN_TRACE_ID_U128: u128 = 0x4bf9_2f35_77b3_4da6_a3ce_929d_0e0e_4736;
pub const KNOWN_SPAN_ID_U64: u64 = 0x00f0_67aa_0ba9_02b7;

/// The `traceparent` header form of the known ids (`00-<trace>-<span>-01`).
#[must_use]
pub fn known_traceparent() -> String {
    format!("00-{KNOWN_TRACE_ID_U128:032x}-{KNOWN_SPAN_ID_U64:016x}-01")
}

/// The known trace id as the lowercase 32-hex-digit form for comparison.
#[must_use]
pub fn known_trace_id_hex() -> String {
    format!("{KNOWN_TRACE_ID_U128:032x}")
}

/// Idempotently install `BoundedTraceContextPropagator` as the global W3C
/// text-map propagator. Safe to call from multiple tests/threads: the
/// propagator is stateless, so a race between two installs converges on the
/// same (correct) global state; `OnceLock` just avoids redundant installs.
pub fn install_test_propagator() {
    static INSTALLED: OnceLock<()> = OnceLock::new();
    INSTALLED.get_or_init(|| {
        opentelemetry::global::set_text_map_propagator(BoundedTraceContextPropagator::new());
    });
}

/// RAII handle for an in-memory-exporter-backed OTel subscriber installed as the
/// current thread's default. Drop restores the previous subscriber.
///
/// **Not `Send`.** `DefaultGuard` is thread-local; hold this in the test
/// function's stack frame and don't move it across an `.await` that could hop
/// threads (fine under the default current-thread `#[tokio::test]` runtime).
#[must_use]
pub struct SpanCapture {
    exporter: InMemorySpanExporter,
    _guard: DefaultGuard,
    _provider: TracerProvider,
}

impl SpanCapture {
    /// Install the capturing subscriber as this thread's default.
    pub fn install() -> Self {
        let exporter = InMemorySpanExporter::default();
        let provider = TracerProvider::builder()
            .with_simple_exporter(exporter.clone())
            .build();
        let tracer = provider.tracer("mc-service-test");
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
    /// synchronously on span end, so this reflects all spans closed by the time
    /// this is called — no explicit flush needed.
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
#[must_use]
pub fn trace_id_hex(span: &SpanData) -> String {
    format!("{:032x}", span.span_context.trace_id())
}
