//! OpenTelemetry span capture + W3C fixtures for service integration tests.
//!
//! One home for what GC/AC/MC/MH tests previously copied per crate:
//!
//! - [`SpanCapture`] — installs the production tracing layer
//!   ([`configured_layer`]) backed by an in-memory exporter as the current
//!   thread's default subscriber. [`SpanCapture::install`] enables every
//!   level (a stricter tripwire than production);
//!   [`SpanCapture::install_with_filter`] composes an `EnvFilter` exactly as
//!   the service `main.rs` files do — pass [`deployed_rust_log`] so the test
//!   runs the filter that ships — plus a JSON fmt layer captured into a buffer
//!   and a counter for WARNs from the extraction helper.
//! - [`deployed_rust_log`] — reads `RUST_LOG` from a deployed manifest, never a
//!   re-typed literal that could drift from what ships.
//! - Fixed W3C trace/span ids ([`KNOWN_TRACE_ID_U128`], [`known_traceparent`],
//!   ...) and [`install_test_propagator`].

use crate::observability::otel::configured_layer;
use crate::observability::otel_grpc::BoundedTraceContextPropagator;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_sdk::trace::{InMemorySpanExporter, SdkTracerProvider, SpanData};
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tracing::subscriber::DefaultGuard;
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::{EnvFilter, Layer};

/// Known W3C trace id shared by the OTel continuity tests.
pub const KNOWN_TRACE_ID_U128: u128 = 0x4bf9_2f35_77b3_4da6_a3ce_929d_0e0e_4736;
/// Known W3C parent span id shared by the OTel continuity tests.
pub const KNOWN_SPAN_ID_U64: u64 = 0x00f0_67aa_0ba9_02b7;
/// [`KNOWN_TRACE_ID_U128`] as lowercase 32-hex digits.
pub const KNOWN_TRACE_ID_HEX: &str = "4bf92f3577b34da6a3ce929d0e0e4736";
/// [`KNOWN_SPAN_ID_U64`] as lowercase 16-hex digits.
pub const KNOWN_PARENT_SPAN_ID_HEX: &str = "00f067aa0ba902b7";

/// The `traceparent` header for the known ids (`00-<trace>-<span>-01`).
#[must_use]
pub fn known_traceparent() -> String {
    format!("00-{KNOWN_TRACE_ID_HEX}-{KNOWN_PARENT_SPAN_ID_HEX}-01")
}

/// The known trace id as lowercase hex, for comparisons.
#[must_use]
pub fn known_trace_id_hex() -> String {
    KNOWN_TRACE_ID_HEX.to_string()
}

/// Format a span's trace id as the lowercase 32-hex-digit W3C form.
#[must_use]
pub fn trace_id_hex(span: &SpanData) -> String {
    format!("{:032x}", span.span_context.trace_id())
}

/// Value of a span attribute as a string, if present.
#[must_use]
pub fn attr(span: &SpanData, key: &str) -> Option<String> {
    span.attributes
        .iter()
        .find(|kv| kv.key.as_str() == key)
        .map(|kv| kv.value.as_str().into_owned())
}

/// Idempotently install `BoundedTraceContextPropagator` as the global text-map
/// propagator, exactly as production `init_otel` does.
pub fn install_test_propagator() {
    static INSTALLED: OnceLock<()> = OnceLock::new();
    INSTALLED.get_or_init(|| {
        opentelemetry::global::set_text_map_propagator(BoundedTraceContextPropagator::new());
    });
}

/// Read `RUST_LOG` from a deployed manifest, given its path relative to the
/// repository root. Panics (fails loudly) when the file or variable is missing.
#[must_use]
pub fn deployed_rust_log(manifest: &str) -> String {
    let path: PathBuf = [env!("CARGO_MANIFEST_DIR"), "..", "..", manifest]
        .iter()
        .collect();
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read deployed manifest {}: {e}", path.display()));
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        if line.trim() == "- name: RUST_LOG" {
            let value = lines
                .next()
                .and_then(|l| l.trim().strip_prefix("value:"))
                .map_or_else(
                    || panic!("RUST_LOG not followed by `value:` in {}", path.display()),
                    |v| v.trim().trim_matches('"').to_string(),
                );
            assert!(!value.is_empty(), "empty RUST_LOG in {}", path.display());
            return value;
        }
    }
    panic!(
        "no RUST_LOG env var in deployed manifest {}",
        path.display()
    );
}

/// Target the extraction helper (`set_remote_parent`) warns under.
const EXTRACTION_WARN_TARGET: &str = "common::observability::otel";

#[derive(Clone, Default)]
struct WarnCounter(Arc<AtomicUsize>);

impl<S: Subscriber> Layer<S> for WarnCounter {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let meta = event.metadata();
        if *meta.level() == Level::WARN && meta.target() == EXTRACTION_WARN_TARGET {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
}

#[derive(Clone, Default)]
struct SharedBuf(Arc<Mutex<Vec<u8>>>);

impl io::Write for SharedBuf {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// RAII span capture: while held, this thread's default subscriber is the
/// production tracing layer backed by an in-memory exporter. Spans export
/// synchronously on close.
///
/// **Not `Send`.** The guard is thread-local; hold it on the test's stack frame
/// under the default current-thread `#[tokio::test]` runtime.
#[must_use]
pub struct SpanCapture {
    exporter: InMemorySpanExporter,
    warns: Arc<AtomicUsize>,
    json: Arc<Mutex<Vec<u8>>>,
    _guard: DefaultGuard,
    _provider: SdkTracerProvider,
}

impl SpanCapture {
    /// Capture with every level enabled (no filter, no fmt layer).
    pub fn install() -> Self {
        let exporter = InMemorySpanExporter::default();
        let provider = SdkTracerProvider::builder()
            .with_simple_exporter(exporter.clone())
            .build();
        let subscriber =
            tracing_subscriber::registry().with(configured_layer(provider.tracer("test")));
        Self {
            exporter,
            warns: Arc::default(),
            json: Arc::default(),
            _guard: tracing::subscriber::set_default(subscriber),
            _provider: provider,
        }
    }

    /// Capture under `filter` (an `EnvFilter` directive string), composed after
    /// the OTel layer as in `main.rs`, plus the JSON fmt layer (into
    /// [`SpanCapture::json_output`]) and the extraction-WARN counter.
    pub fn install_with_filter(filter: &str) -> Self {
        let exporter = InMemorySpanExporter::default();
        let provider = SdkTracerProvider::builder()
            .with_simple_exporter(exporter.clone())
            .build();
        let warns = WarnCounter::default();
        let buf = SharedBuf::default();
        let writer = buf.clone();
        let subscriber = tracing_subscriber::registry()
            .with(configured_layer(provider.tracer("test")))
            .with(EnvFilter::new(filter))
            .with(warns.clone())
            .with(
                tracing_subscriber::fmt::layer()
                    .json()
                    .with_writer(move || writer.clone()),
            );
        Self {
            exporter,
            warns: warns.0,
            json: buf.0,
            _guard: tracing::subscriber::set_default(subscriber),
            _provider: provider,
        }
    }

    /// Every span exported so far.
    #[must_use]
    pub fn finished_spans(&self) -> Vec<SpanData> {
        self.exporter
            .get_finished_spans()
            .expect("in-memory span exporter lock poisoned")
    }

    /// The first exported span with the given name.
    #[must_use]
    pub fn find_span(&self, name: &str) -> Option<SpanData> {
        self.finished_spans().into_iter().find(|s| s.name == name)
    }

    /// Every exported span with the given name.
    #[must_use]
    pub fn spans_named(&self, name: &str) -> Vec<SpanData> {
        self.finished_spans()
            .into_iter()
            .filter(|s| s.name == name)
            .collect()
    }

    /// WARN events from the extraction helper so far (filtered capture only).
    #[must_use]
    pub fn extraction_warns(&self) -> usize {
        self.warns.load(Ordering::SeqCst)
    }

    /// Everything the JSON fmt layer wrote so far (filtered capture only).
    #[must_use]
    pub fn json_output(&self) -> String {
        String::from_utf8_lossy(
            &self
                .json
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        )
        .into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The hex fixtures are a second encoding of the numeric ids; pin them to
    /// each other so "same trace id" tests can never compare two different ids.
    #[test]
    fn hex_fixtures_encode_the_numeric_ids() {
        assert_eq!(format!("{KNOWN_TRACE_ID_U128:032x}"), KNOWN_TRACE_ID_HEX);
        assert_eq!(
            format!("{KNOWN_SPAN_ID_U64:016x}"),
            KNOWN_PARENT_SPAN_ID_HEX
        );
    }
}
