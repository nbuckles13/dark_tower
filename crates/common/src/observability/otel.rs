//! OpenTelemetry SDK init helper for Dark Tower services (R-54).
//!
//! Each service's `main.rs` calls [`init_otel`] before its serve loop boots.
//! On success the helper returns an [`OtelInit`] containing:
//!
//! - an [`OtelGuard`] (RAII) that the caller holds in `main()` until shutdown,
//!   so `Drop` flushes pending spans via
//!   [`SdkTracerProvider::shutdown_with_timeout`], and
//! - the OpenTelemetry tracing layer (built by [`configured_layer`]) the caller
//!   composes into its existing `tracing_subscriber::Registry` stack.
//!
//! The helper:
//!
//! - rejects a current-thread Tokio runtime (see [`OtelGuard`] for why),
//! - builds an OTLP-gRPC exporter pointed at [`OtelConfig::endpoint`],
//! - eagerly probes that endpoint via [`tonic::transport::Endpoint::connect`]
//!   with [`CONNECT_TIMEOUT`] so an unreachable collector surfaces as
//!   `Err(OtelInitError::CollectorUnreachable)` synchronously at init,
//! - registers a [`crate::observability::otel_grpc::BoundedTraceContextPropagator`]
//!   globally so W3C `traceparent` / `tracestate` propagation reuses the same
//!   bounds-enforcing instance on both inject and extract paths,
//! - configures `Sampler::ParentBased(TraceIdRatioBased(sample_rate))`,
//! - sets Resource attributes per ADR-0011 and `OTel` semantic conventions:
//!   `service.name`, `service.version`, `service.namespace=darktower`,
//!   `deployment.environment` — exactly these four (no SDK/env detectors).
//!
//! # Failure model (Clarification Q13: fail-hard at init, fail-soft at runtime)
//!
//! `init_otel` returns `Err` if the OTLP endpoint cannot be reached at
//! startup. Services propagate that to a non-zero exit; K8s pod-health
//! alerting surfaces it.
//!
//! Runtime export failures do not fail requests. They surface as the SDK's
//! own `internal-logs` events at warn/error through the service's subscriber
//! (the fmt/JSON layer — the OpenTelemetry layer filters SDK targets out, see
//! [`configured_layer`]). The `dt_otel_export_failures_total` counter and the
//! `OTelExportFailureRate` alert remain task #28's deliverables. `init_otel`
//! rejects an endpoint carrying userinfo credentials (its startup errors echo
//! the endpoint), so no credential can reach those logs.
//!
//! # Caller wiring pattern
//!
//! ```ignore
//! use common::observability::otel::{init_otel, OtelConfig};
//! use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
//!
//! let otel = init_otel(
//!     "auth-controller",
//!     env!("CARGO_PKG_VERSION"),
//!     &config.environment,
//!     OtelConfig {
//!         endpoint: config.otel_endpoint.clone(),
//!         sample_rate: config.otel_sample_rate,
//!     },
//! ).await?;
//!
//! tracing_subscriber::registry()
//!     // Compose `otel.layer` FIRST, directly onto the bare `registry()`:
//!     // its concrete type is a `Layer<Registry>` only.
//!     .with(otel.layer)
//!     .with(tracing_subscriber::EnvFilter::try_from_default_env()
//!         .unwrap_or_else(|_| "info".into()))
//!     .with(tracing_subscriber::fmt::layer().json())
//!     .init();
//!
//! // hold `otel.guard` in `main()` until shutdown.
//! let _otel_guard = otel.guard;
//! ```
//!
//! The caller MUST compose `otel.layer` into its own `Registry` (layer-first,
//! per the note above); this helper never calls `.init()` on the subscriber.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use opentelemetry::global;
use opentelemetry::trace::{TraceContextExt, TracerProvider as _};
use opentelemetry::{Context, KeyValue};
use opentelemetry_otlp::{RetryPolicy, WithExportConfig, WithTonicConfig};
use opentelemetry_sdk::trace::{Sampler, SdkTracer, SdkTracerProvider};
use opentelemetry_sdk::Resource;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use thiserror::Error;
use tonic::transport::Endpoint;
use tracing::Metadata;
use tracing_opentelemetry::{OpenTelemetryLayer, OpenTelemetrySpanExt, SetParentError};
use tracing_subscriber::filter::{FilterFn, Filtered};
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::Registry;

use crate::observability::otel_grpc::BoundedTraceContextPropagator;

/// Connect timeout for the OTLP-gRPC endpoint probe at init. Chosen short
/// (1s) so a misconfigured or down collector surfaces fast: the production
/// collector is in-cluster, so a healthy DNS + intra-cluster TCP handshake
/// resolves well under this bound. Per operations + semantic-guard review.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(1);

/// Upper bound on the span flush in [`OtelGuard`]'s `Drop`. AC/GC drain for
/// 30s by default against a 35s `terminationGracePeriodSeconds`; the SDK's
/// own 5s default would consume the whole margin, so a slow collector would
/// get the pod `SIGKILL`ed mid-flush. 3s leaves headroom (operations review).
pub const OTEL_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(3);

/// `service.namespace` Resource attribute value — fixed across all four
/// Dark Tower services per ADR-0011.
const SERVICE_NAMESPACE: &str = "darktower";

/// Target prefixes whose spans the OpenTelemetry layer never sees:
///
/// - the gRPC/HTTP transport an OTLP export runs on (`h2`, `hyper`, `tonic`,
///   `tower::buffer`). The batch processor suppresses telemetry on its own
///   thread, but tonic's channel I/O runs on tokio workers outside that scope,
///   so raising `RUST_LOG` while debugging a collector outage would otherwise
///   export the export traffic, feeding the failing exporter;
/// - the QUIC/WebTransport stack (`quinn*`, `wtransport*`). Its per-packet
///   spans are created under the MC/MH connection span before the first
///   message (and its trace context) arrives, and since tracing-opentelemetry
///   0.32 a child span starts its parent's `OTel` context, after which the
///   connection span can no longer be reparented.
///
/// Matched on crate boundaries by [`is_otel_transport_target`];
/// `tower::buffer`, never bare `tower` (that would also hit `tower_http`
/// request spans).
const OTEL_TRANSPORT_TARGETS: &[&str] = &[
    "h2",
    "hyper",
    "tonic",
    "tower::buffer",
    "quinn",
    "quinn_proto",
    "quinn_udp",
    "wtransport",
    "wtransport_proto",
];

/// Set the first time [`configured_layer`] builds the OpenTelemetry layer.
/// Read by [`set_remote_parent`]: a missing layer is expected when `OTel` is
/// disabled by config, and a wiring bug when it is not.
static OTEL_LAYER_INSTALLED: AtomicBool = AtomicBool::new(false);

/// Latch behind the warn-once for a filtered-out request span; see
/// [`set_remote_parent`].
static SPAN_DISABLED_WARNED: AtomicBool = AtomicBool::new(false);

/// The OpenTelemetry tracing layer type every Dark Tower subscriber composes.
pub type OtelLayer<S> =
    Filtered<OpenTelemetryLayer<S, SdkTracer>, FilterFn<fn(&Metadata<'_>) -> bool>, S>;

/// Caller-supplied `OTel` configuration.
///
/// Distinct from [`crate::config::ObservabilityConfig`], which is the
/// env-var-deserialized persisted shape. Each service maps its
/// `ObservabilityConfig` into an `OtelConfig` at the [`init_otel`] call site;
/// this struct intentionally has no `Default` impl so callers cannot rely on
/// silent default endpoints (security review item 6).
#[derive(Debug, Clone)]
pub struct OtelConfig {
    /// OTLP-gRPC endpoint URL (e.g.
    /// `http://otel-collector.default.svc.cluster.local:4317`).
    pub endpoint: String,
    /// Head-sampling ratio in `[0.0, 1.0]` for `TraceIdRatioBased` under a
    /// `ParentBased` parent rule. Out-of-range values are rejected with
    /// [`OtelInitError::ConfigInvalid`].
    pub sample_rate: f64,
}

/// Errors returned by [`init_otel`].
#[derive(Debug, Error)]
pub enum OtelInitError {
    /// `OtelConfig` failed pre-flight validation (e.g. `sample_rate` outside
    /// `[0.0, 1.0]`, `endpoint` not a parseable URL, or a current-thread
    /// runtime).
    #[error("invalid OTel config: {reason}")]
    ConfigInvalid { reason: String },

    /// The eager OTLP-gRPC endpoint probe failed. Surfaces a non-routable or
    /// collector-down state at startup so the pod fails K8s readiness.
    #[error("OTLP collector unreachable at {endpoint}")]
    CollectorUnreachable {
        endpoint: String,
        #[source]
        source: tonic::transport::Error,
    },

    /// Wrapper around the OTLP exporter build failure. Preserves source for
    /// downcasting.
    #[error("OTel SDK error")]
    Sdk(#[from] opentelemetry_otlp::ExporterBuildError),
}

/// Reason string for the current-thread rejection, distinct from every other
/// [`OtelInitError::ConfigInvalid`] cause.
const CURRENT_THREAD_REASON: &str = "init_otel requires a multi-thread Tokio runtime: the batch \
     span processor's shutdown blocks the calling thread while the tonic export needs the runtime";

/// Reason string for an endpoint carrying userinfo; deliberately does not
/// echo the configured value.
const USERINFO_REASON: &str = "endpoint must not contain userinfo credentials";

/// RAII guard returned alongside the tracing layer from [`init_otel`]. `Drop`
/// flushes pending spans via [`SdkTracerProvider::shutdown_with_timeout`]
/// (bounded by [`OTEL_SHUTDOWN_TIMEOUT`]); the global provider slot holds its
/// own clone, so the flush must be explicit.
///
/// **Must be dropped in `main()`, never moved into a spawned task.** The
/// batch span processor exports from a dedicated thread that blocks on the
/// tonic export, whose channel I/O is driven by the Tokio runtime. GC/MC/MH
/// run a single worker under their CPU limits; dropping the guard on the
/// `block_on` thread keeps that worker free, whereas dropping it inside a
/// task would block the worker and stall the flush until the timeout.
///
/// The field is private and the struct has no `pub` constructor — only
/// [`init_otel`] can produce one.
pub struct OtelGuard {
    provider: SdkTracerProvider,
}

impl Drop for OtelGuard {
    fn drop(&mut self) {
        // `catch_unwind` keeps shutdown bulletproof should a future SDK
        // introduce a panic path: a double panic during unwind would skip the
        // very flush we hold the guard for (operations review finding 1).
        let provider = &self.provider;
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if let Err(e) = provider.shutdown_with_timeout(OTEL_SHUTDOWN_TIMEOUT) {
                tracing::warn!(error = %e, "OpenTelemetry span flush at shutdown failed");
            }
        }));
    }
}

/// Return value of [`init_otel`].
///
/// Named container (rather than tuple) so the contract is self-documenting
/// at the call site and tolerates future additions.
pub struct OtelInit {
    /// Hold in `main()` until shutdown to flush pending spans on Drop.
    pub guard: OtelGuard,
    /// Compose into the caller's existing `tracing_subscriber::Registry`
    /// stack via `.with(otel.layer)`. The caller — not this helper — owns
    /// the final `.init()`.
    pub layer: OtelLayer<Registry>,
}

/// Initialize the OpenTelemetry SDK for a Dark Tower service.
///
/// On success the caller receives an [`OtelInit`]: hold the guard, compose
/// the layer into the subscriber stack. On failure, propagate the error
/// from `main()` so the process exits non-zero and K8s pod-health alerting
/// surfaces the misconfiguration.
///
/// **Must be called from a multi-thread Tokio runtime** — performs an async
/// probe via [`tonic::transport::Endpoint::connect`], and the guard's flush
/// needs a runtime worker free (see [`OtelGuard`]).
///
/// # Errors
///
/// Returns:
/// - [`OtelInitError::ConfigInvalid`] if called on a current-thread runtime,
///   if `sample_rate` is outside `[0.0, 1.0]`, or if `endpoint` is not a
///   parseable URL,
/// - [`OtelInitError::CollectorUnreachable`] if the OTLP-gRPC endpoint probe
///   fails within [`CONNECT_TIMEOUT`],
/// - [`OtelInitError::Sdk`] if the OTLP exporter fails to build.
pub async fn init_otel(
    service_name: &str,
    service_version: &str,
    environment: &str,
    config: OtelConfig,
) -> Result<OtelInit, OtelInitError> {
    if tokio::runtime::Handle::current().runtime_flavor()
        == tokio::runtime::RuntimeFlavor::CurrentThread
    {
        return Err(OtelInitError::ConfigInvalid {
            reason: CURRENT_THREAD_REASON.to_owned(),
        });
    }

    if !(0.0..=1.0).contains(&config.sample_rate) {
        return Err(OtelInitError::ConfigInvalid {
            reason: format!("sample_rate {} outside [0.0, 1.0]", config.sample_rate),
        });
    }

    // Eager connect probe: the same Channel is handed to the OTLP exporter
    // below via `.with_channel`, so `init_otel` exercises the exact channel
    // the exporter will use — no parallel probe.
    //
    // The endpoint is echoed into startup errors (`ConfigInvalid`,
    // `CollectorUnreachable`), so credentials in its userinfo would land in
    // logs: reject them on the RAW string, before any error can echo it, and
    // without echoing the value.
    if endpoint_has_userinfo(&config.endpoint) {
        return Err(OtelInitError::ConfigInvalid {
            reason: USERINFO_REASON.to_owned(),
        });
    }
    let endpoint = Endpoint::from_shared(config.endpoint.clone()).map_err(|e| {
        OtelInitError::ConfigInvalid {
            reason: format!("endpoint {:?} is not a valid URL: {e}", config.endpoint),
        }
    })?;
    let channel = endpoint
        .connect_timeout(CONNECT_TIMEOUT)
        .connect()
        .await
        .map_err(|source| OtelInitError::CollectorUnreachable {
            endpoint: config.endpoint.clone(),
            source,
        })?;

    // Retries are disabled: otlp 0.17 had none, and 0.33's default policy
    // would stretch the shutdown flush past `OTEL_SHUTDOWN_TIMEOUT`.
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(&config.endpoint)
        .with_channel(channel)
        .with_retry_policy(RetryPolicy::disabled())
        .build()?;

    let provider = build_tracer_provider(
        service_name,
        service_version,
        environment,
        config.sample_rate,
        exporter,
    );

    let tracer = provider.tracer(service_name.to_owned());

    // Install the bounded W3C propagator GLOBALLY (security review item b).
    // All inbound extractions and outbound injections go through this same
    // instance, which enforces the W3C §3.2 / §3.3 length and format bounds.
    global::set_text_map_propagator(BoundedTraceContextPropagator::new());

    // Install the provider globally so the tracing layer's tracer and any
    // other consumers of the global API agree on the same provider.
    global::set_tracer_provider(provider.clone());

    Ok(OtelInit {
        guard: OtelGuard { provider },
        layer: configured_layer(tracer),
    })
}

/// True when the raw endpoint's authority (between `scheme://` and the first
/// `/`, `?` or `#`) contains `@`, i.e. carries userinfo. Works on unparsed
/// input so the check runs before any URL-parse error could echo the value.
fn endpoint_has_userinfo(raw: &str) -> bool {
    let after_scheme = raw.split_once("://").map_or(raw, |(_, rest)| rest);
    after_scheme
        .split(['/', '?', '#'])
        .next()
        .is_some_and(|authority| authority.contains('@'))
}

/// Build the OpenTelemetry tracing layer with Dark Tower's settings. The ONE
/// constructor for production ([`init_otel`]) and every test harness, so the
/// continuity tests exercise the configuration that ships:
///
/// - `with_context_activation(false)`: tracing-opentelemetry 0.32+ starts a
///   span's `OTel` context on entry by default, after which `set_parent`
///   returns `AlreadyStarted`. Every inbound extraction reparents the
///   already-entered request span, so activation must stay off (0.25
///   semantics).
/// - `with_target(false)`: keeps the exported span attribute set unchanged
///   (0.32 added a default `target` attribute).
/// - a per-layer filter dropping [`OTEL_TRANSPORT_TARGETS`] and the
///   OpenTelemetry crates themselves.
pub fn configured_layer<S>(tracer: SdkTracer) -> OtelLayer<S>
where
    S: tracing::Subscriber + for<'span> LookupSpan<'span>,
{
    OTEL_LAYER_INSTALLED.store(true, Ordering::Relaxed);
    let filter: fn(&Metadata<'_>) -> bool = |meta| !is_otel_transport_target(meta.target());
    tracing_subscriber::Layer::with_filter(
        OpenTelemetryLayer::new(tracer)
            .with_context_activation(false)
            .with_target(false),
        FilterFn::new(filter),
    )
}

/// True when `target` belongs to the OTLP export path: any `opentelemetry*`
/// crate, or one of [`OTEL_TRANSPORT_TARGETS`] on a crate/module boundary
/// (`t == p || t.starts_with("{p}::")`), so `tower_http` and our own crates
/// never match.
fn is_otel_transport_target(target: &str) -> bool {
    target.starts_with("opentelemetry")
        || OTEL_TRANSPORT_TARGETS.iter().any(|p| {
            target == *p
                || target
                    .strip_prefix(p)
                    .is_some_and(|rest| rest.starts_with("::"))
        })
}

/// Attach the remote parent carried in `cx` to `span` — the only production
/// path for inbound trace-context reparenting.
///
/// A context with no valid remote span context (no or rejected
/// `traceparent`) has nothing to attach and is a no-op, so probes and scrapes
/// never reach the outcome reporting below. Otherwise the outcome follows
/// [`parent_disposition`].
pub fn set_remote_parent(span: &tracing::Span, cx: Context) {
    if !cx.span().span_context().is_valid() {
        return;
    }
    if let Err(err) = span.set_parent(cx) {
        report_parent_disposition(
            &err,
            parent_disposition(&err, OTEL_LAYER_INSTALLED.load(Ordering::Relaxed)),
            &SPAN_DISABLED_WARNED,
        );
    }
}

/// How a failed `set_parent` is reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParentDisposition {
    /// Expected: nothing to report.
    Silent,
    /// A wiring bug: warn every time.
    Warn,
    /// The hosting span was filtered out by config: warn once per process.
    WarnOnce,
}

/// Pure classification of a `set_parent` failure.
///
/// - `LayerNotFound`: `OTel` disabled by config (silent) or a subscriber stack
///   that hides the layer (warn).
/// - `SpanDisabled`: `OTel` disabled (silent) or the request span is filtered
///   out by the log filter while `OTel` is on, dropping the parent (warn once).
/// - `AlreadyStarted`: the span's context was started before reparenting —
///   precluded by `with_context_activation(false)`, so a wiring bug (warn).
pub(crate) fn parent_disposition(err: &SetParentError, installed: bool) -> ParentDisposition {
    match err {
        SetParentError::LayerNotFound if installed => ParentDisposition::Warn,
        SetParentError::SpanDisabled if installed => ParentDisposition::WarnOnce,
        SetParentError::LayerNotFound | SetParentError::SpanDisabled => ParentDisposition::Silent,
        SetParentError::AlreadyStarted => ParentDisposition::Warn,
    }
}

/// Emit the warning for `disposition`. Logs the error variant only — never
/// the context, carrier or headers. `latch` is injected so tests can use a
/// fresh one instead of the process-global latch.
pub(crate) fn report_parent_disposition(
    err: &SetParentError,
    disposition: ParentDisposition,
    latch: &AtomicBool,
) {
    match disposition {
        ParentDisposition::Silent => {}
        ParentDisposition::Warn => {
            tracing::warn!(error = %err, "inbound trace context not attached");
        }
        ParentDisposition::WarnOnce => {
            if !latch.swap(true, Ordering::Relaxed) {
                tracing::warn!(
                    error = %err,
                    "inbound trace context dropped: the request span is disabled by the log \
                     filter (reported once)"
                );
            }
        }
    }
}

/// Build the immutable Resource describing this process. Exposed as
/// `pub(crate)` so the resource-attribute unit test can assert on the shape
/// without going through global state. `builder_empty` keeps exactly these
/// four attributes: `Resource::builder()` would add `telemetry.sdk.*` and
/// environment-detector attributes.
pub(crate) fn build_resource(
    service_name: &str,
    service_version: &str,
    environment: &str,
) -> Resource {
    Resource::builder_empty()
        .with_attributes([
            KeyValue::new("service.name", service_name.to_owned()),
            KeyValue::new("service.version", service_version.to_owned()),
            KeyValue::new("service.namespace", SERVICE_NAMESPACE),
            KeyValue::new("deployment.environment", environment.to_owned()),
        ])
        .build()
}

/// Construct the tracer provider from a built span exporter. Shared between
/// production [`init_otel`] and the in-process test seam so coverage of
/// resource / sampler wiring exercises the same builder code path.
fn build_tracer_provider<E>(
    service_name: &str,
    service_version: &str,
    environment: &str,
    sample_rate: f64,
    exporter: E,
) -> SdkTracerProvider
where
    E: opentelemetry_sdk::trace::SpanExporter + 'static,
{
    let sampler = Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(sample_rate)));
    SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_sampler(sampler)
        .with_resource(build_resource(service_name, service_version, environment))
        .build()
}

// =============================================================================
// Tests
// =============================================================================
//
// Tests that don't need global state construct the provider locally to avoid
// stomping on each other across parallel `cargo test` threads (code-reviewer
// item 4).

#[cfg(test)]
#[expect(
    clippy::panic,
    reason = "test-only assertions; panic on mismatch is the contract"
)]
mod tests {
    use super::*;
    use crate::observability::testing::otel::{KNOWN_SPAN_ID_U64, KNOWN_TRACE_ID_U128};
    use opentelemetry::trace::{SpanContext, SpanId, SpanKind, TraceFlags, TraceId, TraceState};
    use opentelemetry_sdk::trace::{SamplingDecision, SamplingResult, ShouldSample};

    // All tests here construct their TracerProvider locally (via
    // build_tracer_provider with a capturing exporter) so they don't touch
    // the global tracer-provider slot. The lone test that drives init_otel
    // exercises the Err path before any global install runs, so no
    // serialization mutex is needed in this module.

    // R-54 user-story-specified test: init MUST return Err when the
    // collector is unreachable. Port 1 on loopback is reserved and the
    // kernel refuses connections synchronously (ECONNREFUSED), so the
    // 1s CONNECT_TIMEOUT is plenty.
    //
    // No GLOBAL_OTEL_LOCK needed here: the Err path returns BEFORE any
    // global state is installed (probe fails → return Err before
    // set_text_map_propagator / set_tracer_provider are called).
    #[tokio::test(flavor = "multi_thread")]
    async fn init_returns_err_on_unreachable_collector() {
        let result = init_otel(
            "auth-controller",
            "0.0.0-test",
            "test",
            OtelConfig {
                endpoint: "http://127.0.0.1:1".into(),
                sample_rate: 1.0,
            },
        )
        .await;
        match result {
            Err(OtelInitError::CollectorUnreachable { endpoint, .. }) => {
                assert_eq!(endpoint, "http://127.0.0.1:1");
            }
            Err(other) => panic!("expected CollectorUnreachable, got {other:?}"),
            Ok(_) => panic!("expected Err, got Ok"),
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn init_rejects_out_of_range_sample_rate() {
        let result = init_otel(
            "auth-controller",
            "0.0.0-test",
            "test",
            OtelConfig {
                endpoint: "http://127.0.0.1:1".into(),
                sample_rate: 1.5,
            },
        )
        .await;
        match result {
            Err(OtelInitError::ConfigInvalid { reason }) => {
                assert!(
                    reason.contains("1.5"),
                    "reason should mention value: {reason}"
                );
            }
            Err(other) => panic!("expected ConfigInvalid, got {other:?}"),
            Ok(_) => panic!("expected Err, got Ok"),
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn init_rejects_malformed_endpoint() {
        let result = init_otel(
            "auth-controller",
            "0.0.0-test",
            "test",
            OtelConfig {
                endpoint: "::not a url::".into(),
                sample_rate: 1.0,
            },
        )
        .await;
        match result {
            Err(OtelInitError::ConfigInvalid { reason }) => {
                assert!(
                    reason.contains("not a valid URL"),
                    "reason should name the endpoint parse failure: {reason}"
                );
            }
            Err(other) => panic!("expected ConfigInvalid, got {other:?}"),
            Ok(_) => panic!("expected Err, got Ok"),
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn init_rejects_endpoint_with_userinfo_without_echoing_it() {
        let result = init_otel(
            "auth-controller",
            "0.0.0-test",
            "test",
            OtelConfig {
                endpoint: "http://user:s3cret@127.0.0.1:1".into(),
                sample_rate: 1.0,
            },
        )
        .await;
        match result {
            Err(OtelInitError::ConfigInvalid { reason }) => {
                assert_eq!(reason, USERINFO_REASON);
                assert!(!reason.contains("s3cret"));
            }
            Err(other) => panic!("expected ConfigInvalid, got {other:?}"),
            Ok(_) => panic!("expected Err, got Ok"),
        }
    }

    // A credential-bearing endpoint that would ALSO fail to parse must be
    // rejected before the parse error (which echoes the endpoint) is built.
    #[tokio::test(flavor = "multi_thread")]
    async fn init_rejects_unparseable_endpoint_with_userinfo_without_echoing_it() {
        let result = init_otel(
            "auth-controller",
            "0.0.0-test",
            "test",
            OtelConfig {
                endpoint: "http://user:s3cret@host:bad".into(),
                sample_rate: 1.0,
            },
        )
        .await;
        match result {
            Err(OtelInitError::ConfigInvalid { reason }) => {
                assert_eq!(reason, USERINFO_REASON);
                assert!(!reason.contains("s3cret"));
            }
            Err(other) => panic!("expected ConfigInvalid, got {other:?}"),
            Ok(_) => panic!("expected Err, got Ok"),
        }
    }

    #[test]
    fn endpoint_userinfo_detection_is_authority_scoped() {
        assert!(endpoint_has_userinfo("http://user:pw@collector:4317"));
        assert!(endpoint_has_userinfo("user@collector:4317"));
        assert!(!endpoint_has_userinfo("http://collector:4317"));
        assert!(!endpoint_has_userinfo("http://collector:4317/path@x"));
        assert!(!endpoint_has_userinfo("http://collector:4317?q=a@b"));
    }

    // A current-thread runtime is rejected before the probe: the guard's
    // flush would block the only executor (see `OtelGuard`).
    #[tokio::test(flavor = "current_thread")]
    async fn init_rejects_current_thread_runtime() {
        let result = init_otel(
            "auth-controller",
            "0.0.0-test",
            "test",
            OtelConfig {
                endpoint: "http://127.0.0.1:1".into(),
                sample_rate: 1.0,
            },
        )
        .await;
        match result {
            Err(OtelInitError::ConfigInvalid { reason }) => {
                assert_eq!(reason, CURRENT_THREAD_REASON);
                assert!(reason.contains("multi-thread Tokio runtime"));
            }
            Err(other) => panic!("expected ConfigInvalid, got {other:?}"),
            Ok(_) => panic!("expected Err, got Ok"),
        }
    }

    #[test]
    fn build_resource_has_expected_attrs() {
        let resource = build_resource("auth-controller", "0.1.0", "dev");
        let attrs: std::collections::HashMap<String, String> = resource
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();

        assert_eq!(
            attrs.get("service.name").map(String::as_str),
            Some("auth-controller")
        );
        assert_eq!(
            attrs.get("service.version").map(String::as_str),
            Some("0.1.0")
        );
        assert_eq!(
            attrs.get("service.namespace").map(String::as_str),
            Some(SERVICE_NAMESPACE)
        );
        assert_eq!(
            attrs.get("deployment.environment").map(String::as_str),
            Some("dev")
        );

        // Exactly these four: no `telemetry.sdk.*`, no env/host detector
        // attributes (PII-correlation keys, security review item 3).
        let mut keys: Vec<&str> = attrs.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "deployment.environment",
                "service.name",
                "service.namespace",
                "service.version"
            ]
        );
    }

    // Asserts that `build_tracer_provider`'s configuration path wires the
    // Resource INTO the exporter (via `SpanExporter::set_resource`) — not
    // just builds it. Uses a tiny capturing exporter so a regression in
    // "build but don't install" fails (per test review item #2a).
    //
    // Constructs the provider locally (not via init_otel) so it doesn't
    // touch the global tracer-provider slot — keeps this test parallel-safe
    // with the global-installing tests.
    #[tokio::test]
    async fn build_tracer_provider_installs_resource_via_set_resource() {
        use opentelemetry::trace::Tracer as _;
        use opentelemetry_sdk::error::OTelSdkResult;
        use opentelemetry_sdk::trace::{SpanData, SpanExporter};
        use std::sync::{Arc, Mutex};

        #[derive(Debug, Clone, Default)]
        struct CapturingExporter {
            resource: Arc<Mutex<Option<Resource>>>,
        }

        impl SpanExporter for CapturingExporter {
            async fn export(&self, _batch: Vec<SpanData>) -> OTelSdkResult {
                Ok(())
            }
            fn set_resource(&mut self, resource: &Resource) {
                if let Ok(mut guard) = self.resource.lock() {
                    *guard = Some(resource.clone());
                }
            }
        }

        let exporter = CapturingExporter::default();
        let captured = Arc::clone(&exporter.resource);
        let resource = build_resource("global-controller", "9.9.9", "stage");
        let sampler = Sampler::ParentBased(Box::new(Sampler::AlwaysOn));
        let provider = SdkTracerProvider::builder()
            .with_simple_exporter(exporter)
            .with_sampler(sampler)
            .with_resource(resource)
            .build();

        // Force a span end so any deferred wiring runs.
        let tracer = provider.tracer("global-controller");
        tracer.in_span("install_check", |_| {});

        let captured = captured
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
            .unwrap_or_else(|| {
                panic!(
                    "set_resource was never called on the exporter — \
                        provider built without installing the Resource"
                )
            });
        let attrs: std::collections::HashMap<String, String> = captured
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        assert_eq!(
            attrs.get("service.name").map(String::as_str),
            Some("global-controller")
        );
        assert_eq!(
            attrs.get("service.version").map(String::as_str),
            Some("9.9.9")
        );
        assert_eq!(
            attrs.get("service.namespace").map(String::as_str),
            Some(SERVICE_NAMESPACE)
        );
        assert_eq!(
            attrs.get("deployment.environment").map(String::as_str),
            Some("stage")
        );

        let _ = provider; // keep alive until here
    }

    // Parent-based sampler: parent SAMPLED → child SAMPLED even at ratio
    // 0.0 (which would drop roots). Exercises that the SDK's ParentBased
    // honors the parent's sampling decision.
    #[test]
    fn sampler_parent_sampled_child_sampled_even_with_zero_ratio() {
        let sampler = Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(0.0)));
        let parent_ctx = Context::new().with_remote_span_context(SpanContext::new(
            TraceId::from(KNOWN_TRACE_ID_U128),
            SpanId::from(KNOWN_SPAN_ID_U64),
            TraceFlags::SAMPLED,
            true,
            TraceState::default(),
        ));
        let decision = sampler.should_sample(
            Some(&parent_ctx),
            TraceId::from(0x1111_1111_1111_1111_1111_1111_1111_1111),
            "child",
            &SpanKind::Internal,
            &[],
            &[],
        );
        assert_decision(&decision, &SamplingDecision::RecordAndSample);
    }

    #[test]
    fn sampler_parent_unsampled_child_dropped_even_with_full_ratio() {
        let sampler = Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(1.0)));
        let parent_ctx = Context::new().with_remote_span_context(SpanContext::new(
            TraceId::from(KNOWN_TRACE_ID_U128),
            SpanId::from(KNOWN_SPAN_ID_U64),
            TraceFlags::default(), // NOT SAMPLED
            true,
            TraceState::default(),
        ));
        let decision = sampler.should_sample(
            Some(&parent_ctx),
            TraceId::from(0x2222_2222_2222_2222_2222_2222_2222_2222),
            "child",
            &SpanKind::Internal,
            &[],
            &[],
        );
        assert_decision(&decision, &SamplingDecision::Drop);
    }

    // Root (no parent) obeys the ratio: 0.0 always drops, 1.0 always
    // records.
    #[test]
    fn sampler_root_obeys_ratio_zero_and_one() {
        let trace_id = TraceId::from(0xdead_beef_dead_beef_dead_beef_dead_beef);

        let always_off = Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(0.0)));
        let drop_decision =
            always_off.should_sample(None, trace_id, "root", &SpanKind::Internal, &[], &[]);
        assert_decision(&drop_decision, &SamplingDecision::Drop);

        let always_on = Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(1.0)));
        let keep_decision =
            always_on.should_sample(None, trace_id, "root", &SpanKind::Internal, &[], &[]);
        assert_decision(&keep_decision, &SamplingDecision::RecordAndSample);
    }

    fn assert_decision(result: &SamplingResult, expected: &SamplingDecision) {
        assert_eq!(
            &result.decision,
            expected,
            "expected {expected:?}, got {actual:?}",
            actual = result.decision
        );
    }

    // =========================================================================
    // Layer configuration (`configured_layer`)
    // =========================================================================

    #[test]
    fn transport_target_matching_is_on_crate_boundaries() {
        for excluded in [
            "opentelemetry_sdk::trace",
            "opentelemetry_otlp",
            "opentelemetry",
            "h2",
            "h2::codec",
            "hyper::client",
            "tonic::transport",
            "tower::buffer",
            "tower::buffer::worker",
            "quinn_proto::connection",
            "wtransport::driver",
        ] {
            assert!(
                is_otel_transport_target(excluded),
                "{excluded} must be excluded"
            );
        }
        for kept in [
            "tower_http::trace::on_request",
            "tower",
            "tower::util",
            "h2c_lookalike",
            "hyperion",
            "tonic_extra",
            "quinnipiac",
            "gc_service::routes",
            "mh_service::grpc::span_layer",
        ] {
            assert!(!is_otel_transport_target(kept), "{kept} must be kept");
        }
    }

    // Through the shared constructor: transport/SDK spans are not exported
    // while real request-span targets are (positive control in the same run),
    // and no span carries a `target` attribute.
    #[test]
    fn configured_layer_filters_transport_spans_and_omits_target_attribute() {
        use opentelemetry_sdk::trace::InMemorySpanExporter;
        use tracing_subscriber::layer::SubscriberExt;

        let exporter = InMemorySpanExporter::default();
        let provider = SdkTracerProvider::builder()
            .with_simple_exporter(exporter.clone())
            .build();
        let subscriber =
            tracing_subscriber::registry().with(configured_layer(provider.tracer("filter-test")));
        tracing::subscriber::with_default(subscriber, || {
            tracing::info_span!(target: "h2::proto", "h2_span").in_scope(|| {});
            tracing::info_span!(target: "opentelemetry_sdk::trace", "sdk_span").in_scope(|| {});
            tracing::info_span!(target: "quinn_proto::connection", "quinn_span").in_scope(|| {});
            // The real request-span builders that ship (HTTP + gRPC), plus
            // MH SpanLayer's target.
            crate::observability::otel_http::http_request_span(
                &http::Method::GET,
                "/api/v1/meetings",
            )
            .in_scope(|| {});
            tower_http::trace::MakeSpan::make_span(
                &mut crate::observability::otel_grpc::grpc_make_span(),
                &http::Request::new(()),
            )
            .in_scope(|| {});
            tracing::info_span!(target: "mh_service::grpc::span_layer", "mh.grpc.request")
                .in_scope(|| {});
        });

        let spans = exporter
            .get_finished_spans()
            .unwrap_or_else(|e| panic!("in-memory exporter failed: {e}"));
        let mut names: Vec<String> = spans.iter().map(|s| s.name.to_string()).collect();
        names.sort_unstable();
        assert_eq!(names, ["mh.grpc.request", "request", "request"]);
        for span in &spans {
            assert!(
                span.attributes.iter().all(|kv| kv.key.as_str() != "target"),
                "span {:?} must not carry a `target` attribute",
                span.name
            );
        }
    }

    // =========================================================================
    // `set_remote_parent` dispositions
    // =========================================================================

    #[test]
    fn parent_disposition_table() {
        use ParentDisposition::{Silent, Warn, WarnOnce};
        use SetParentError::{AlreadyStarted, LayerNotFound, SpanDisabled};
        let cases = [
            (LayerNotFound, false, Silent),
            (LayerNotFound, true, Warn),
            (SpanDisabled, false, Silent),
            (SpanDisabled, true, WarnOnce),
            (AlreadyStarted, false, Warn),
            (AlreadyStarted, true, Warn),
        ];
        for (err, installed, expected) in cases {
            assert_eq!(
                parent_disposition(&err, installed),
                expected,
                "{err:?} with installed={installed}"
            );
        }
    }

    /// Counts WARN events emitted by this module, so warn/silent outcomes are
    /// asserted on captured output rather than "did not panic".
    #[derive(Clone, Default)]
    struct WarnCounter(std::sync::Arc<std::sync::atomic::AtomicUsize>);

    impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for WarnCounter {
        fn on_event(
            &self,
            event: &tracing::Event<'_>,
            _ctx: tracing_subscriber::layer::Context<'_, S>,
        ) {
            if *event.metadata().level() == tracing::Level::WARN
                && Some(event.metadata().target()) == module_path!().strip_suffix("::tests")
            {
                self.0.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    impl WarnCounter {
        fn count(&self) -> usize {
            self.0.load(Ordering::Relaxed)
        }
    }

    fn count_warns(f: impl FnOnce()) -> usize {
        use tracing_subscriber::layer::SubscriberExt;
        let counter = WarnCounter::default();
        let subscriber = tracing_subscriber::registry().with(counter.clone());
        tracing::subscriber::with_default(subscriber, f);
        counter.count()
    }

    #[test]
    fn report_silent_emits_nothing() {
        let latch = AtomicBool::new(false);
        for err in [SetParentError::LayerNotFound, SetParentError::SpanDisabled] {
            let warns = count_warns(|| {
                report_parent_disposition(&err, parent_disposition(&err, false), &latch);
            });
            assert_eq!(warns, 0, "{err:?} with OTel not installed must be silent");
        }
        assert!(
            !latch.load(Ordering::Relaxed),
            "silent path must not trip the latch"
        );
    }

    #[test]
    fn report_warn_emits_every_time() {
        let latch = AtomicBool::new(false);
        for err in [
            SetParentError::LayerNotFound,
            SetParentError::AlreadyStarted,
        ] {
            let warns = count_warns(|| {
                for _ in 0..2 {
                    report_parent_disposition(&err, parent_disposition(&err, true), &latch);
                }
            });
            assert_eq!(
                warns, 2,
                "{err:?} with OTel installed must warn on every call"
            );
        }
    }

    #[test]
    fn report_warn_once_latches_after_first() {
        let latch = AtomicBool::new(false);
        let err = SetParentError::SpanDisabled;
        let warns = count_warns(|| {
            for _ in 0..3 {
                report_parent_disposition(&err, parent_disposition(&err, true), &latch);
            }
        });
        assert_eq!(warns, 1);
        assert!(latch.load(Ordering::Relaxed));
    }

    fn known_remote_context() -> Context {
        Context::new().with_remote_span_context(SpanContext::new(
            TraceId::from(KNOWN_TRACE_ID_U128),
            SpanId::from(KNOWN_SPAN_ID_U64),
            TraceFlags::SAMPLED,
            true,
            TraceState::default(),
        ))
    }

    // Success through the shared layer: the reparented span carries the
    // remote trace id and the remote span as its parent, with no warning.
    #[test]
    fn set_remote_parent_attaches_remote_parent() {
        use opentelemetry_sdk::trace::InMemorySpanExporter;
        use tracing_subscriber::layer::SubscriberExt;

        let exporter = InMemorySpanExporter::default();
        let provider = SdkTracerProvider::builder()
            .with_simple_exporter(exporter.clone())
            .build();
        let counter = WarnCounter::default();
        let subscriber = tracing_subscriber::registry()
            .with(configured_layer(provider.tracer("reparent-test")))
            .with(counter.clone());
        tracing::subscriber::with_default(subscriber, || {
            let span = tracing::info_span!("request");
            let _entered = span.enter();
            set_remote_parent(&tracing::Span::current(), known_remote_context());
        });

        let spans = exporter
            .get_finished_spans()
            .unwrap_or_else(|e| panic!("in-memory exporter failed: {e}"));
        let span = spans
            .iter()
            .find(|s| s.name == "request")
            .unwrap_or_else(|| panic!("request span not exported"));
        let remote = known_remote_context();
        let remote = remote.span().span_context().clone();
        assert_eq!(span.span_context.trace_id(), remote.trace_id());
        assert_eq!(span.parent_span_id, remote.span_id());
        assert_eq!(counter.count(), 0);
    }

    // No traceparent (or a rejected one) means nothing to attach: no call, no
    // warning — this is why probes and scrapes never reach the warn-once.
    #[test]
    fn set_remote_parent_without_valid_context_is_a_silent_no_op() {
        let warns = count_warns(|| {
            set_remote_parent(&tracing::Span::none(), Context::new());
        });
        assert_eq!(warns, 0);
    }

    // `AlreadyStarted` is reachable only when context activation is on (the
    // shared layer turns it off): reproduce it with a raw layer and confirm the
    // disposition warns.
    #[test]
    fn already_started_is_classified_as_warn() {
        use tracing_subscriber::layer::SubscriberExt;

        let provider = SdkTracerProvider::builder().build();
        let subscriber = tracing_subscriber::registry()
            .with(OpenTelemetryLayer::new(provider.tracer("activation-on")));
        let err = tracing::subscriber::with_default(subscriber, || {
            let span = tracing::info_span!("entered");
            let _entered = span.enter();
            span.set_parent(known_remote_context())
        });
        match err {
            Err(err @ SetParentError::AlreadyStarted) => {
                assert_eq!(parent_disposition(&err, true), ParentDisposition::Warn);
            }
            other => panic!("expected AlreadyStarted with activation on, got {other:?}"),
        }
    }

    // =========================================================================
    // `OtelGuard` shutdown flush (multi-thread runtime, fake OTLP collector)
    // =========================================================================

    // `init_otel` installs the global propagator/provider; serialize the
    // tests that call it successfully.
    static INIT_OTEL_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    #[derive(Clone, Default)]
    struct FakeCollector {
        spans: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    }

    #[tonic::async_trait]
    impl opentelemetry_proto::tonic::collector::trace::v1::trace_service_server::TraceService
        for FakeCollector
    {
        async fn export(
            &self,
            request: tonic::Request<
                opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest,
            >,
        ) -> Result<
            tonic::Response<
                opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceResponse,
            >,
            tonic::Status,
        > {
            let names = request
                .into_inner()
                .resource_spans
                .into_iter()
                .flat_map(|rs| rs.scope_spans)
                .flat_map(|ss| ss.spans)
                .map(|span| span.name);
            self.spans
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .extend(names);
            Ok(tonic::Response::new(
                opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceResponse::default(),
            ))
        }
    }

    async fn start_fake_collector() -> (String, FakeCollector) {
        use opentelemetry_proto::tonic::collector::trace::v1::trace_service_server::TraceServiceServer;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .unwrap_or_else(|e| panic!("bind fake collector: {e}"));
        let addr = listener
            .local_addr()
            .unwrap_or_else(|e| panic!("fake collector addr: {e}"));
        let collector = FakeCollector::default();
        let service = TraceServiceServer::new(collector.clone());
        tokio::spawn(async move {
            let _ = tonic::transport::Server::builder()
                .add_service(service)
                .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
                .await;
        });
        (format!("http://{addr}"), collector)
    }

    async fn init_test_otel(endpoint: String) -> OtelInit {
        init_otel(
            "global-controller",
            "0.0.0-test",
            "test",
            OtelConfig {
                endpoint,
                sample_rate: 1.0,
            },
        )
        .await
        .unwrap_or_else(|e| panic!("init_otel against the test collector failed: {e}"))
    }

    // (a) Dropping the guard flushes: a span ended just before shutdown
    // reaches the collector.
    #[tokio::test(flavor = "multi_thread")]
    async fn guard_drop_flushes_pending_spans_to_collector() {
        use tracing_subscriber::layer::SubscriberExt;

        let _serial = INIT_OTEL_LOCK.lock().await;
        let (endpoint, collector) = start_fake_collector().await;
        let otel = init_test_otel(endpoint).await;

        let subscriber = tracing_subscriber::registry().with(otel.layer);
        tracing::subscriber::with_default(subscriber, || {
            tracing::info_span!("flushed_on_shutdown").in_scope(|| {});
        });
        drop(otel.guard);

        let spans = collector
            .spans
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        assert!(
            spans.iter().any(|name| name == "flushed_on_shutdown"),
            "span ended before the guard dropped must reach the collector, got {spans:?}"
        );
    }

    // (b) A collector that accepts the connection but never answers cannot
    // hold shutdown past the bound.
    #[tokio::test(flavor = "multi_thread")]
    async fn guard_drop_is_bounded_by_shutdown_timeout_when_collector_hangs() {
        use tracing_subscriber::layer::SubscriberExt;

        let _serial = INIT_OTEL_LOCK.lock().await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .unwrap_or_else(|e| panic!("bind hung collector: {e}"));
        let addr = listener
            .local_addr()
            .unwrap_or_else(|e| panic!("hung collector addr: {e}"));
        // Accept and hold every connection without ever speaking HTTP/2.
        tokio::spawn(async move {
            let mut held = Vec::new();
            while let Ok((socket, _)) = listener.accept().await {
                held.push(socket);
            }
        });

        let otel = init_test_otel(format!("http://{addr}")).await;
        let subscriber = tracing_subscriber::registry().with(otel.layer);
        tracing::subscriber::with_default(subscriber, || {
            tracing::info_span!("never_acknowledged").in_scope(|| {});
        });

        let started = std::time::Instant::now();
        drop(otel.guard);
        let elapsed = started.elapsed();
        let slack = Duration::from_secs(2);
        assert!(
            elapsed >= OTEL_SHUTDOWN_TIMEOUT.saturating_sub(Duration::from_millis(500)),
            "guard drop took {elapsed:?}: an export that fails instantly would not exercise \
             the {OTEL_SHUTDOWN_TIMEOUT:?} bound"
        );
        assert!(
            elapsed <= OTEL_SHUTDOWN_TIMEOUT + slack,
            "guard drop took {elapsed:?}, bound is {OTEL_SHUTDOWN_TIMEOUT:?} + {slack:?}"
        );
    }
}
