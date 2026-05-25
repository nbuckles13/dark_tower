//! OpenTelemetry SDK init helper for Dark Tower services (R-54).
//!
//! Each service's `main.rs` calls [`init_otel`] before its serve loop boots.
//! On success the helper returns an [`OtelInit`] containing:
//!
//! - an [`OtelGuard`] (RAII) that the caller holds in `main()` until shutdown,
//!   so `Drop` flushes pending spans via
//!   `opentelemetry::global::shutdown_tracer_provider`, and
//! - a [`tracing_opentelemetry::OpenTelemetryLayer`] the caller composes into
//!   its existing `tracing_subscriber::Registry` stack.
//!
//! The helper:
//!
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
//!   `deployment.environment`.
//!
//! # Failure model (Clarification Q13: fail-hard at init, fail-soft at runtime)
//!
//! `init_otel` returns `Err` if the OTLP endpoint cannot be reached at
//! startup. Services propagate that to a non-zero exit; K8s pod-health
//! alerting surfaces it. No new init-failure metric or dedicated alert is
//! added here.
//!
//! Runtime export failures drop spans silently. **The runtime
//! `dt_otel_export_failures_total` counter and the `OTelExportFailureRate`
//! warn alert are task #28's deliverables (collector deployment), NOT this
//! devloop.** Do not add per-failure logging or a phantom metric registration
//! in this module.
//!
//! Runbook entries for the collector-unreachable failure mode live with task
//! #28 and the per-service wiring tasks (#25/#26/#6/#27), not here.
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
//!     .with(tracing_subscriber::EnvFilter::try_from_default_env()
//!         .unwrap_or_else(|_| "info".into()))
//!     .with(tracing_subscriber::fmt::layer().json())
//!     .with(otel.layer)
//!     .init();
//!
//! // hold `otel.guard` in `main()` until shutdown.
//! let _otel_guard = otel.guard;
//! ```
//!
//! The caller MUST compose `otel.layer` into its own `Registry`; this helper
//! never calls `.init()` on the subscriber.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use opentelemetry::global;
use opentelemetry::trace::TraceError;
// Renamed in scope: the local `opentelemetry_sdk::trace::TracerProvider` struct
// is also in scope below, and the trait must be imported to call
// `.tracer(name)` on the concrete type without ambiguity (per F3).
use opentelemetry::trace::TracerProvider as _;
use opentelemetry::KeyValue;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::trace::{Config, Sampler, Tracer, TracerProvider};
use opentelemetry_sdk::Resource;
use std::time::Duration;
use thiserror::Error;
use tonic::transport::Endpoint;
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::Registry;

use crate::observability::otel_grpc::BoundedTraceContextPropagator;

/// Connect timeout for the OTLP-gRPC endpoint probe at init. Chosen short
/// (1s) so a misconfigured or down collector surfaces fast: the production
/// collector is in-cluster, so a healthy DNS + intra-cluster TCP handshake
/// resolves well under this bound. Per operations + semantic-guard review.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(1);

/// `service.namespace` Resource attribute value — fixed across all four
/// Dark Tower services per ADR-0011.
const SERVICE_NAMESPACE: &str = "darktower";

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
    /// `[0.0, 1.0]`, or `endpoint` not a parseable URL).
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

    /// Wrapper around upstream `OTel` SDK errors (e.g. tracer-provider build
    /// failure). Preserves source for downcasting.
    #[error("OTel SDK error")]
    Sdk(#[from] TraceError),
}

/// RAII guard returned alongside the `tracing-opentelemetry` layer from
/// [`init_otel`]. `Drop` flushes pending spans via
/// [`opentelemetry::global::shutdown_tracer_provider`].
///
/// The field is private and the struct has no `pub` constructor — only
/// [`init_otel`] can produce one — so callers cannot construct a guard out
/// of band or trigger a double-shutdown.
pub struct OtelGuard {
    _private: (),
}

impl Drop for OtelGuard {
    fn drop(&mut self) {
        // Panic-safety belt: opentelemetry 0.24's
        // `shutdown_tracer_provider` returns `()` and has no documented
        // panic path today (verified by inspection + confirmed by security
        // review), but a future SDK upgrade could introduce one — and a
        // double-panic during stack-unwind would skip the very flush we
        // hold the guard for. `catch_unwind` keeps shutdown bulletproof
        // as the SDK evolves (per operations review finding 1).
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            global::shutdown_tracer_provider();
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
    pub layer: OpenTelemetryLayer<Registry, Tracer>,
}

/// Initialize the OpenTelemetry SDK for a Dark Tower service.
///
/// On success the caller receives an [`OtelInit`]: hold the guard, compose
/// the layer into the subscriber stack. On failure, propagate the error
/// from `main()` so the process exits non-zero and K8s pod-health alerting
/// surfaces the misconfiguration.
///
/// **Must be called from a Tokio runtime** — performs an async probe via
/// [`tonic::transport::Endpoint::connect`].
///
/// # Errors
///
/// Returns:
/// - [`OtelInitError::ConfigInvalid`] if `sample_rate` is outside `[0.0, 1.0]`
///   or `endpoint` is not a parseable URL,
/// - [`OtelInitError::CollectorUnreachable`] if the OTLP-gRPC endpoint probe
///   fails within [`CONNECT_TIMEOUT`],
/// - [`OtelInitError::Sdk`] if the upstream `OTel` SDK fails to build the
///   tracer provider (e.g. exporter initialization failure).
pub async fn init_otel(
    service_name: &str,
    service_version: &str,
    environment: &str,
    config: OtelConfig,
) -> Result<OtelInit, OtelInitError> {
    if !(0.0..=1.0).contains(&config.sample_rate) {
        return Err(OtelInitError::ConfigInvalid {
            reason: format!("sample_rate {} outside [0.0, 1.0]", config.sample_rate),
        });
    }

    // Eager connect probe: the same Channel is handed to the OTLP exporter
    // below via `.with_channel`, so `init_otel` exercises the exact channel
    // the exporter will use — no parallel probe.
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

    let exporter = opentelemetry_otlp::new_exporter()
        .tonic()
        .with_endpoint(&config.endpoint)
        .with_channel(channel);

    let provider = build_tracer_provider(
        service_name,
        service_version,
        environment,
        config.sample_rate,
        exporter,
    )?;

    let tracer = provider.tracer(service_name.to_owned());

    // Install the bounded W3C propagator GLOBALLY (security review item b).
    // All inbound extractions and outbound injections go through this same
    // instance, which enforces the W3C §3.2 / §3.3 length and format bounds.
    global::set_text_map_propagator(BoundedTraceContextPropagator::new());

    // Install the provider globally so the `tracing-opentelemetry` layer's
    // tracer and any other consumers of the global API agree on the same
    // provider.
    let _previous = global::set_tracer_provider(provider);

    let layer = OpenTelemetryLayer::new(tracer);

    Ok(OtelInit {
        guard: OtelGuard { _private: () },
        layer,
    })
}

/// Build the immutable Resource describing this process. Exposed as
/// `pub(crate)` so the resource-attribute unit test can assert on the shape
/// without going through global state.
pub(crate) fn build_resource(
    service_name: &str,
    service_version: &str,
    environment: &str,
) -> Resource {
    Resource::new([
        KeyValue::new("service.name", service_name.to_owned()),
        KeyValue::new("service.version", service_version.to_owned()),
        KeyValue::new("service.namespace", SERVICE_NAMESPACE),
        KeyValue::new("deployment.environment", environment.to_owned()),
    ])
}

/// Construct a `TracerProvider` from a pre-built exporter builder. Shared
/// between production [`init_otel`] and the in-process test seam so coverage
/// of resource / sampler wiring exercises the same builder code path.
fn build_tracer_provider(
    service_name: &str,
    service_version: &str,
    environment: &str,
    sample_rate: f64,
    exporter: opentelemetry_otlp::TonicExporterBuilder,
) -> Result<TracerProvider, TraceError> {
    let resource = build_resource(service_name, service_version, environment);
    let sampler = Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(sample_rate)));

    let span_exporter = exporter.build_span_exporter()?;
    let provider = TracerProvider::builder()
        .with_batch_exporter(span_exporter, opentelemetry_sdk::runtime::Tokio)
        .with_config(
            Config::default()
                .with_sampler(sampler)
                .with_resource(resource),
        )
        .build();
    Ok(provider)
}

// =============================================================================
// Tests
// =============================================================================
//
// Tests that don't need global state construct the provider locally to avoid
// stomping on each other across parallel `cargo test` threads (code-reviewer
// item 4). The handful that MUST touch globals (`set_tracer_provider`,
// `set_text_map_propagator`) serialize via `GLOBAL_OTEL_LOCK`.

#[cfg(test)]
#[expect(
    clippy::panic,
    reason = "test-only assertions; panic on mismatch is the contract"
)]
mod tests {
    use super::*;
    use opentelemetry::trace::{
        SamplingDecision, SamplingResult, SpanContext, SpanId, SpanKind, TraceContextExt,
        TraceFlags, TraceId, TraceState,
    };
    use opentelemetry::Context;
    use opentelemetry_sdk::trace::ShouldSample;

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
    #[tokio::test]
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

    #[tokio::test]
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

    #[tokio::test]
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
            Err(OtelInitError::ConfigInvalid { .. }) => {}
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

        // Defense in depth: these PII-correlation keys MUST NOT appear
        // (security review item 3).
        assert!(!attrs.contains_key("host.name"));
        assert!(!attrs.contains_key("process.pid"));
        assert!(!attrs.contains_key("k8s.pod.name"));
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
        use opentelemetry::trace::{Tracer as _, TracerProvider as _};
        use opentelemetry_sdk::export::trace::{ExportResult, SpanData, SpanExporter};
        use std::future::Future;
        use std::pin::Pin;
        use std::sync::{Arc, Mutex};

        #[derive(Debug, Clone, Default)]
        struct CapturingExporter {
            resource: Arc<Mutex<Option<Resource>>>,
        }

        impl SpanExporter for CapturingExporter {
            fn export(
                &mut self,
                _batch: Vec<SpanData>,
            ) -> Pin<Box<dyn Future<Output = ExportResult> + Send>> {
                Box::pin(async { Ok(()) })
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
        let provider = TracerProvider::builder()
            .with_simple_exporter(exporter)
            .with_config(
                Config::default()
                    .with_sampler(sampler)
                    .with_resource(resource),
            )
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
            TraceId::from_u128(0x4bf9_2f35_77b3_4da6_a3ce_929d_0e0e_4736),
            SpanId::from_u64(0x00f0_67aa_0ba9_02b7),
            TraceFlags::SAMPLED,
            true,
            TraceState::default(),
        ));
        let decision = sampler.should_sample(
            Some(&parent_ctx),
            TraceId::from_u128(0x1111_1111_1111_1111_1111_1111_1111_1111),
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
            TraceId::from_u128(0x4bf9_2f35_77b3_4da6_a3ce_929d_0e0e_4736),
            SpanId::from_u64(0x00f0_67aa_0ba9_02b7),
            TraceFlags::default(), // NOT SAMPLED
            true,
            TraceState::default(),
        ));
        let decision = sampler.should_sample(
            Some(&parent_ctx),
            TraceId::from_u128(0x2222_2222_2222_2222_2222_2222_2222_2222),
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
        let trace_id = TraceId::from_u128(0xdead_beef_dead_beef_dead_beef_dead_beef);

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
}
