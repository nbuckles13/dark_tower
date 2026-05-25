//! gRPC trace-context propagation for Dark Tower (R-56).
//!
//! Two pieces:
//!
//! 1. [`BoundedTraceContextPropagator`] — wraps the SDK's
//!    [`opentelemetry_sdk::propagation::TraceContextPropagator`] and enforces
//!    the W3C Trace Context inbound bounds the SDK does not natively check:
//!    `traceparent` ≤ 55 chars and `tracestate` ≤ 512 bytes / ≤ 32 list
//!    members. Out-of-bounds inputs are **rejected** (no parent context
//!    attached); the gRPC request itself dispatches normally and the span
//!    becomes a new root. Inject delegates unchanged.
//!
//!    [`crate::observability::otel::init_otel`] registers an instance of this
//!    wrapper globally via `set_text_map_propagator`. Both interceptor types
//!    below read from the same global, so a single configuration knob (the
//!    propagator registration) governs both directions.
//!
//! 2. [`client_interceptor`] / [`server_interceptor`] — Tonic
//!    [`Interceptor`](tonic::service::Interceptor) closures that inject the
//!    active `OTel` context into outbound gRPC metadata and extract the inbound
//!    parent into the current tracing span. Both use the globally registered
//!    propagator.
//!
//! # Security contract (user-story line 411)
//!
//! - Inject path ONLY writes the [`PROPAGATED_HEADERS`] set (`traceparent`,
//!   `tracestate`). The interceptor MUST NEVER read `authorization` or any
//!   other metadata key, and MUST NEVER copy metadata into span attributes.
//! - Extract path reads ONLY the same two headers. Any other metadata
//!   (including `authorization`) is left untouched.
//! - W3C bounds enforced on extract per the wrapper above.
//!
//! These invariants are exercised by the in-module test matrix below.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use opentelemetry::propagation::{Extractor, Injector, TextMapPropagator};
use opentelemetry::Context;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use tonic::metadata::{KeyRef, MetadataKey, MetadataMap, MetadataValue};
use tonic::Status;
use tracing_opentelemetry::OpenTelemetrySpanExt;

/// The COMPLETE set of metadata keys this module ever injects on outbound
/// requests, or reads on inbound requests. Anchored as a `const` so a code
/// reviewer can verify the security contract by grepping for the constant
/// name (security review item 1). Both values are W3C Trace Context headers
/// and contain non-sensitive hex span/trace identifiers.
pub const PROPAGATED_HEADERS: &[&str] = &["traceparent", "tracestate"];

/// Hard cap on `traceparent` header length per W3C Trace Context §3.2.2.5.
/// Version 00 produces a 55-character string; longer payloads are spec
/// violations and MUST be rejected.
const TRACEPARENT_MAX_LEN: usize = 55;

/// Hard cap on `tracestate` header length per W3C Trace Context §3.3.1.1.
/// Over-cap payloads MAY be rejected; we reject (do not truncate) because
/// truncation produces malformed key-value pairs at the boundary, which is
/// worse than dropping. Confirmed by security review item a.
const TRACESTATE_MAX_BYTES: usize = 512;

/// Hard cap on `tracestate` list-member count per W3C Trace Context §3.3.1.2.
const TRACESTATE_MAX_MEMBERS: usize = 32;

/// W3C-bounds-enforcing wrapper around [`TraceContextPropagator`].
///
/// On extract, validates inbound `traceparent` and `tracestate` against the
/// W3C-mandated bounds before delegating; if any bound is violated, returns
/// the input context unchanged (no parent attached). On inject, delegates
/// unchanged — outbound headers come from a well-formed in-process
/// `SpanContext` and cannot violate the bounds by construction.
#[derive(Debug, Default, Clone)]
pub struct BoundedTraceContextPropagator {
    inner: TraceContextPropagator,
}

impl BoundedTraceContextPropagator {
    /// Create a new wrapper. The inner propagator has no state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: TraceContextPropagator::new(),
        }
    }

    /// Returns true if the extractor's `traceparent`/`tracestate` headers
    /// pass W3C size and member-count bounds. Format / lowercase / all-zero
    /// checks are delegated to the inner `TraceContextPropagator`, which
    /// already enforces W3C §3.2.2 invariants (verified by inspection of
    /// `opentelemetry_sdk` 0.24 source).
    fn within_bounds(extractor: &dyn Extractor) -> bool {
        if let Some(tp) = extractor.get("traceparent") {
            if tp.len() > TRACEPARENT_MAX_LEN {
                return false;
            }
        }
        if let Some(ts) = extractor.get("tracestate") {
            if ts.len() > TRACESTATE_MAX_BYTES {
                return false;
            }
            // List members are comma-separated; W3C allows OWS around commas
            // but a member count above 32 is a hard reject regardless of
            // whitespace.
            if ts.split(',').filter(|m| !m.trim().is_empty()).count() > TRACESTATE_MAX_MEMBERS {
                return false;
            }
        }
        true
    }
}

impl TextMapPropagator for BoundedTraceContextPropagator {
    fn inject_context(&self, cx: &Context, injector: &mut dyn Injector) {
        self.inner.inject_context(cx, injector);
    }

    fn extract_with_context(&self, cx: &Context, extractor: &dyn Extractor) -> Context {
        if Self::within_bounds(extractor) {
            self.inner.extract_with_context(cx, extractor)
        } else {
            cx.clone()
        }
    }

    fn fields(&self) -> opentelemetry::propagation::text_map_propagator::FieldIter<'_> {
        self.inner.fields()
    }
}

// =============================================================================
// Tonic adapters: MetadataMap ↔ TextMap{Injector,Extractor}
// =============================================================================
//
// Tonic's `MetadataMap` is HTTP-header-like (ASCII byte keys). The
// `MetadataInjector` wraps a `&mut MetadataMap` and exposes the
// `opentelemetry::propagation::Injector` interface used by the propagator.
// `MetadataExtractor` does the read direction.

struct MetadataInjector<'a> {
    metadata: &'a mut MetadataMap,
}

impl Injector for MetadataInjector<'_> {
    fn set(&mut self, key: &str, value: String) {
        // The propagator only ever calls `set` with the two `PROPAGATED_HEADERS`
        // values, both of which parse cleanly as `MetadataKey` and
        // `MetadataValue`. We still go through the fallible APIs and silently
        // discard any failure rather than `unwrap`/`expect` (module-level
        // deny(unwrap_used, panic) per security review item 2).
        if let (Ok(name), Ok(val)) = (
            MetadataKey::from_bytes(key.as_bytes()),
            MetadataValue::try_from(value),
        ) {
            self.metadata.insert(name, val);
        }
    }
}

struct MetadataExtractor<'a> {
    metadata: &'a MetadataMap,
}

impl Extractor for MetadataExtractor<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        self.metadata.get(key).and_then(|v| v.to_str().ok())
    }

    fn keys(&self) -> Vec<&str> {
        self.metadata
            .keys()
            .map(|k| match k {
                KeyRef::Ascii(k) => k.as_str(),
                KeyRef::Binary(k) => k.as_str(),
            })
            .collect()
    }
}

// =============================================================================
// Tonic interceptors
// =============================================================================

/// Tonic client interceptor that injects the active `OTel` context into the
/// outbound request's metadata as `traceparent` / `tracestate`.
///
/// The interceptor reads from the active `tracing::Span`'s `OTel` context; if
/// no span is active, no headers are injected (the request dispatches with
/// no propagated parent). Never reads any inbound metadata, never touches
/// `authorization` or any non-W3C header.
#[must_use]
#[expect(
    clippy::result_large_err,
    reason = "tonic::service::Interceptor::call signature is fixed by upstream; \
              the closure body never returns Err but the trait demands Result<_, Status>"
)]
pub fn client_interceptor() -> impl tonic::service::Interceptor + Clone + Send + Sync + 'static {
    |mut req: tonic::Request<()>| -> Result<tonic::Request<()>, Status> {
        let cx = tracing::Span::current().context();
        opentelemetry::global::get_text_map_propagator(|propagator| {
            propagator.inject_context(
                &cx,
                &mut MetadataInjector {
                    metadata: req.metadata_mut(),
                },
            );
        });
        Ok(req)
    }
}

/// Tonic server interceptor that extracts inbound W3C trace context from the
/// request's metadata and attaches it as the parent of the current
/// `tracing::Span`. Subsequent spans created during the handler's execution
/// inherit this parent automatically via the `tracing-opentelemetry` bridge.
///
/// Reads ONLY `traceparent` / `tracestate`. Never reads `authorization` or
/// any other metadata key. Never copies metadata into span attributes.
#[must_use]
#[expect(
    clippy::result_large_err,
    reason = "tonic::service::Interceptor::call signature is fixed by upstream; \
              the closure body never returns Err but the trait demands Result<_, Status>"
)]
pub fn server_interceptor() -> impl tonic::service::Interceptor + Clone + Send + Sync + 'static {
    |req: tonic::Request<()>| -> Result<tonic::Request<()>, Status> {
        let parent_cx = opentelemetry::global::get_text_map_propagator(|propagator| {
            propagator.extract(&MetadataExtractor {
                metadata: req.metadata(),
            })
        });
        tracing::Span::current().set_parent(parent_cx);
        Ok(req)
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
#[expect(
    clippy::panic,
    reason = "test-only assertions; panic on mismatch is the contract"
)]
mod tests {
    use super::*;
    use opentelemetry::trace::{
        SpanContext, SpanId, TraceContextExt, TraceFlags, TraceId, TraceState,
    };
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    use tonic::metadata::MetadataValue;
    use tonic::service::Interceptor;

    // Serializes tests that touch the global propagator slot. Poison-safe:
    // a panic inside a holding test would otherwise wedge the whole module.
    static GLOBAL_PROPAGATOR_LOCK: Mutex<()> = Mutex::new(());

    /// Lazy installer for the bounded propagator on tests that exercise the
    /// global path. Idempotent: `OnceLock` guarantees we only register once
    /// per test binary, and `set_text_map_propagator` itself replaces
    /// silently.
    fn ensure_bounded_propagator_installed() {
        static INSTALLED: OnceLock<()> = OnceLock::new();
        INSTALLED.get_or_init(|| {
            opentelemetry::global::set_text_map_propagator(BoundedTraceContextPropagator::new());
        });
    }

    const KNOWN_TRACE_ID_U128: u128 = 0x4bf9_2f35_77b3_4da6_a3ce_929d_0e0e_4736;
    const KNOWN_SPAN_ID_U64: u64 = 0x00f0_67aa_0ba9_02b7;

    fn known_trace_id() -> TraceId {
        TraceId::from(KNOWN_TRACE_ID_U128)
    }
    fn known_span_id() -> SpanId {
        SpanId::from(KNOWN_SPAN_ID_U64)
    }
    fn known_span_context() -> SpanContext {
        SpanContext::new(
            known_trace_id(),
            known_span_id(),
            TraceFlags::SAMPLED,
            true,
            TraceState::default(),
        )
    }
    fn known_traceparent_header() -> String {
        // Version 00, sampled flag set — 55 chars total per W3C §3.2.2.
        format!("00-{KNOWN_TRACE_ID_U128:032x}-{KNOWN_SPAN_ID_U64:016x}-01")
    }

    // -------------------------------------------------------------------------
    // Round-trip: inject via the wrapper, extract via the wrapper, recover
    // the same trace_id / span_id.
    // -------------------------------------------------------------------------

    #[test]
    fn propagator_round_trip_recovers_trace_id_and_span_id() {
        let propagator = BoundedTraceContextPropagator::new();
        let span_cx = known_span_context();
        let ctx = Context::new().with_remote_span_context(span_cx.clone());

        let mut headers: HashMap<String, String> = HashMap::new();
        propagator.inject_context(&ctx, &mut headers);

        let extracted = propagator.extract_with_context(&Context::new(), &headers);
        let extracted_sc = extracted.span().span_context().clone();
        assert_eq!(extracted_sc.trace_id(), span_cx.trace_id());
        assert_eq!(extracted_sc.span_id(), span_cx.span_id());
    }

    // -------------------------------------------------------------------------
    // BoundedTraceContextPropagator rejection matrix (security review item b).
    // Each test exercises a single W3C invariant so a failure pinpoints the
    // broken bound without a re-read (test reviewer item: split #12 into 4).
    // -------------------------------------------------------------------------

    /// W3C §3.2.2.5 — `traceparent` MUST be ≤ 55 chars (version 00 = 55).
    /// Bound enforced by [`BoundedTraceContextPropagator::within_bounds`].
    #[test]
    fn bounded_propagator_rejects_overlong_traceparent() {
        let propagator = BoundedTraceContextPropagator::new();
        let mut headers = HashMap::new();
        headers.insert(
            "traceparent".to_string(),
            format!("{}-EXTRAEXTRAEXTRA", known_traceparent_header()),
        );
        let ctx = propagator.extract_with_context(&Context::new(), &headers);
        assert!(
            !ctx.span().span_context().is_valid(),
            "overlong traceparent must NOT attach a parent context"
        );
    }

    /// W3C §3.3.1.1 — `tracestate` MUST be ≤ 512 bytes; over-cap MAY be
    /// rejected. We reject (security review item a).
    #[test]
    fn bounded_propagator_rejects_overlength_tracestate() {
        let propagator = BoundedTraceContextPropagator::new();
        let mut headers = HashMap::new();
        headers.insert("traceparent".to_string(), known_traceparent_header());
        // 600-byte value: "foo=" + 596 ASCII 'a' bytes.
        let big = format!("foo={}", "a".repeat(596));
        assert!(big.len() > TRACESTATE_MAX_BYTES);
        headers.insert("tracestate".to_string(), big);
        let ctx = propagator.extract_with_context(&Context::new(), &headers);
        assert!(
            !ctx.span().span_context().is_valid(),
            "overlength tracestate must drop the entire extraction"
        );
    }

    /// W3C §3.3.1.2 — `tracestate` MUST be ≤ 32 list members.
    #[test]
    fn bounded_propagator_rejects_too_many_tracestate_list_members() {
        let propagator = BoundedTraceContextPropagator::new();
        let mut headers = HashMap::new();
        headers.insert("traceparent".to_string(), known_traceparent_header());
        // 40 list members, each short enough to keep total bytes well under
        // 512 so we test the member-count axis in isolation.
        let members: Vec<String> = (0..40).map(|i| format!("k{i}=v")).collect();
        let combined = members.join(",");
        assert!(combined.len() <= TRACESTATE_MAX_BYTES);
        headers.insert("tracestate".to_string(), combined);
        let ctx = propagator.extract_with_context(&Context::new(), &headers);
        assert!(
            !ctx.span().span_context().is_valid(),
            "tracestate with >32 members must drop the entire extraction"
        );
    }

    // The next four assert upstream `TraceContextPropagator` (which the
    // wrapper delegates to for format checks) rejects per W3C §3.2.2. If a
    // future SDK upgrade weakens any of these, the test fails loudly and we
    // tighten `within_bounds` to compensate.

    /// W3C §3.2.2.1 — version `ff` is reserved and MUST be rejected. We
    /// exercise the wrapper to prove the rejection survives delegation.
    #[test]
    fn bounded_propagator_rejects_unsupported_version_byte() {
        let propagator = BoundedTraceContextPropagator::new();
        let mut headers = HashMap::new();
        headers.insert(
            "traceparent".to_string(),
            format!("ff-{KNOWN_TRACE_ID_U128:032x}-{KNOWN_SPAN_ID_U64:016x}-01"),
        );
        let ctx = propagator.extract_with_context(&Context::new(), &headers);
        assert!(
            !ctx.span().span_context().is_valid(),
            "version 'ff' is reserved and must not attach a parent"
        );
    }

    /// W3C §3.2.2.2 — non-hex characters in trace-id MUST be rejected.
    #[test]
    fn bounded_propagator_rejects_non_hex_trace_id() {
        let propagator = BoundedTraceContextPropagator::new();
        let mut headers = HashMap::new();
        headers.insert(
            "traceparent".to_string(),
            "00-zz000000000000000000000000000000-00f067aa0ba902b7-01".to_string(),
        );
        let ctx = propagator.extract_with_context(&Context::new(), &headers);
        assert!(
            !ctx.span().span_context().is_valid(),
            "non-hex trace-id must not attach a parent"
        );
    }

    /// W3C §3.2.2.3 — all-zero trace-id MUST be rejected.
    #[test]
    fn bounded_propagator_rejects_all_zero_trace_id() {
        let propagator = BoundedTraceContextPropagator::new();
        let mut headers = HashMap::new();
        headers.insert(
            "traceparent".to_string(),
            "00-00000000000000000000000000000000-00f067aa0ba902b7-01".to_string(),
        );
        let ctx = propagator.extract_with_context(&Context::new(), &headers);
        assert!(
            !ctx.span().span_context().is_valid(),
            "all-zero trace-id must not attach a parent"
        );
    }

    /// W3C §3.2.2.4 — all-zero parent-id (span-id) MUST be rejected.
    #[test]
    fn bounded_propagator_rejects_all_zero_parent_id() {
        let propagator = BoundedTraceContextPropagator::new();
        let mut headers = HashMap::new();
        headers.insert(
            "traceparent".to_string(),
            format!("00-{KNOWN_TRACE_ID_U128:032x}-0000000000000000-01"),
        );
        let ctx = propagator.extract_with_context(&Context::new(), &headers);
        assert!(
            !ctx.span().span_context().is_valid(),
            "all-zero parent-id must not attach a parent"
        );
    }

    // -------------------------------------------------------------------------
    // Interceptor inject / extract round-trip via the GLOBAL propagator —
    // exercises the production code path the four services will hit.
    // -------------------------------------------------------------------------

    /// Build a `tracing_subscriber::Registry` with the OpenTelemetry layer
    /// installed against a noop tracer provider. Required for
    /// `OpenTelemetrySpanExt::{context, set_parent}` to actually wire span
    /// context into / out of `tracing::Span`s — without the layer, those
    /// methods are no-ops and the interceptors see empty contexts.
    fn with_test_subscriber<F, R>(f: F) -> R
    where
        F: FnOnce() -> R,
    {
        use opentelemetry::trace::TracerProvider as _;
        use opentelemetry_sdk::trace::TracerProvider;
        use tracing_opentelemetry::OpenTelemetryLayer;
        use tracing_subscriber::layer::SubscriberExt;

        let provider = TracerProvider::builder().build();
        let tracer = provider.tracer("test");
        let layer = OpenTelemetryLayer::new(tracer);
        let subscriber = tracing_subscriber::registry().with(layer);
        tracing::subscriber::with_default(subscriber, f)
    }

    #[test]
    fn client_interceptor_injects_traceparent_into_metadata() {
        let _g = GLOBAL_PROPAGATOR_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        ensure_bounded_propagator_installed();

        let req = with_test_subscriber(|| {
            let span = tracing::info_span!("test_client_inject");
            let parent_cx = Context::new().with_remote_span_context(known_span_context());
            span.set_parent(parent_cx);
            span.in_scope(|| {
                let mut interceptor = client_interceptor();
                interceptor
                    .call(tonic::Request::new(()))
                    .unwrap_or_else(|e| panic!("interceptor returned err: {e}"))
            })
        });

        let tp = req
            .metadata()
            .get("traceparent")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        assert!(
            tp.starts_with("00-"),
            "traceparent header missing or malformed: {tp:?}"
        );
        // The trace_id portion is hex chars 3..35 of the standard 55-char form.
        let trace_id_hex = tp.get(3..35).unwrap_or_default();
        assert_eq!(
            trace_id_hex,
            format!("{KNOWN_TRACE_ID_U128:032x}"),
            "injected trace_id should match the active span's parent"
        );
    }

    #[test]
    fn server_interceptor_extracts_and_attaches_parent_context() {
        let _g = GLOBAL_PROPAGATOR_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        ensure_bounded_propagator_installed();

        let mut req = tonic::Request::new(());
        req.metadata_mut().insert(
            "traceparent",
            MetadataValue::try_from(known_traceparent_header())
                .unwrap_or_else(|e| panic!("known traceparent must parse: {e}")),
        );

        let recovered = with_test_subscriber(|| {
            let span = tracing::info_span!("test_server_extract");
            span.in_scope(|| {
                let mut interceptor = server_interceptor();
                let _ = interceptor
                    .call(req)
                    .unwrap_or_else(|e| panic!("interceptor returned err: {e}"));
                tracing::Span::current().context()
            })
        });

        let sc = recovered.span().span_context().clone();
        assert!(sc.is_valid(), "extracted parent context should be valid");
        assert_eq!(sc.trace_id(), known_trace_id());
    }

    // -------------------------------------------------------------------------
    // Security guarantees on the interceptors (user-story line 411).
    // -------------------------------------------------------------------------

    /// Inbound `authorization` must not be mutated by the server interceptor.
    #[test]
    fn server_interceptor_does_not_mutate_authorization_metadata() {
        let _g = GLOBAL_PROPAGATOR_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        ensure_bounded_propagator_installed();

        let mut req = tonic::Request::new(());
        let auth_value = "Bearer SECRETxyz";
        req.metadata_mut().insert(
            "authorization",
            MetadataValue::try_from(auth_value).unwrap_or_else(|e| panic!("test fixture: {e}")),
        );
        req.metadata_mut().insert(
            "traceparent",
            MetadataValue::try_from(known_traceparent_header())
                .unwrap_or_else(|e| panic!("test fixture: {e}")),
        );

        let mut interceptor = server_interceptor();
        let out = interceptor
            .call(req)
            .unwrap_or_else(|e| panic!("interceptor returned err: {e}"));
        let after = out
            .metadata()
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        assert_eq!(
            after, auth_value,
            "interceptor must not modify authorization metadata"
        );
    }

    /// Outbound client interceptor must NOT auto-propagate any
    /// `authorization` value from the caller's environment — auth bridging is
    /// a request-scoped caller concern, not a context-scoped one.
    #[test]
    fn client_interceptor_does_not_propagate_authorization_metadata() {
        let _g = GLOBAL_PROPAGATOR_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        ensure_bounded_propagator_installed();

        let req = with_test_subscriber(|| {
            let span = tracing::info_span!("test_no_auth_leak");
            span.set_parent(Context::new().with_remote_span_context(known_span_context()));
            span.in_scope(|| {
                let mut interceptor = client_interceptor();
                interceptor
                    .call(tonic::Request::new(()))
                    .unwrap_or_else(|e| panic!("interceptor returned err: {e}"))
            })
        });

        assert!(
            req.metadata().get("authorization").is_none(),
            "client interceptor must not introduce an authorization header"
        );
    }

    /// The injected metadata keyset MUST be a subset of `PROPAGATED_HEADERS`.
    /// Anchors the security contract documented at the constant declaration.
    #[test]
    fn client_interceptor_injects_only_traceparent_and_tracestate_keys() {
        let _g = GLOBAL_PROPAGATOR_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        ensure_bounded_propagator_installed();

        let req = with_test_subscriber(|| {
            let span = tracing::info_span!("test_only_w3c_keys");
            span.set_parent(Context::new().with_remote_span_context(known_span_context()));
            span.in_scope(|| {
                let mut interceptor = client_interceptor();
                interceptor
                    .call(tonic::Request::new(()))
                    .unwrap_or_else(|e| panic!("interceptor returned err: {e}"))
            })
        });

        for key in req.metadata().keys() {
            let name = match key {
                KeyRef::Ascii(k) => k.as_str(),
                KeyRef::Binary(k) => k.as_str(),
            };
            assert!(
                PROPAGATED_HEADERS.contains(&name),
                "injected metadata key {name:?} is not in PROPAGATED_HEADERS"
            );
        }
    }

    /// Capture span attributes during the SERVER interceptor lifecycle and
    /// assert NONE contain the `authorization` key name or a bearer token
    /// substring. Server-side: an inbound request carries `authorization`
    /// from the caller, and the regression would be the interceptor reading
    /// that header into a span attribute. Pairs with the client variant
    /// below (per test review finding 2).
    #[test]
    fn server_interceptor_never_copies_authorization_to_span() {
        run_no_auth_leak_capture(|req| {
            let mut interceptor = server_interceptor();
            let _ = interceptor.call(req).unwrap_or_else(|e| panic!("err: {e}"));
        });
    }

    /// Client-side counterpart: a Request constructed with an
    /// `authorization` metadata value MUST NOT have that value copied into
    /// a span attribute by the client interceptor. The natural future
    /// regression would be "read inbound metadata into span attributes for
    /// outbound debugging" (per test review finding 2).
    #[test]
    fn client_interceptor_never_copies_authorization_to_span() {
        run_no_auth_leak_capture(|req| {
            let mut interceptor = client_interceptor();
            let _ = interceptor.call(req).unwrap_or_else(|e| panic!("err: {e}"));
        });
    }

    /// Shared driver for the two no-auth-leak tests above. Installs a
    /// capturing `tracing_subscriber::Layer`, builds a Request bearing both
    /// `authorization: Bearer SECRETxyz` and a valid `traceparent`, runs
    /// the supplied interceptor closure inside an active span, then asserts
    /// none of the captured span fields contain the auth key or value.
    fn run_no_auth_leak_capture<F>(run_interceptor: F)
    where
        F: FnOnce(tonic::Request<()>),
    {
        use tracing::subscriber::with_default;
        use tracing_subscriber::layer::SubscriberExt;

        let _g = GLOBAL_PROPAGATOR_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        ensure_bounded_propagator_installed();

        let capture = TestSpanCapture::default();
        let subscriber = tracing_subscriber::registry().with(capture.layer());

        with_default(subscriber, || {
            let span = tracing::info_span!("auth_leak_check");
            let _entered = span.enter();

            let mut req = tonic::Request::new(());
            req.metadata_mut().insert(
                "authorization",
                MetadataValue::try_from("Bearer SECRETxyz")
                    .unwrap_or_else(|e| panic!("test fixture: {e}")),
            );
            req.metadata_mut().insert(
                "traceparent",
                MetadataValue::try_from(known_traceparent_header())
                    .unwrap_or_else(|e| panic!("test fixture: {e}")),
            );
            run_interceptor(req);
        });

        let observations = capture.into_observations();
        for (field, value) in observations {
            assert!(
                !field.contains("authorization") && !field.contains("auth.token"),
                "span field name {field:?} suggests auth leak"
            );
            assert!(
                !value.contains("SECRETxyz") && !value.contains("Bearer"),
                "span field value {value:?} contains auth token material"
            );
        }
    }

    // -------------------------------------------------------------------------
    // Test-only span-attribute capture layer. Mirrors the Mutex-wrapped
    // pattern in observability/testing.rs (per test reviewer item b).
    // -------------------------------------------------------------------------

    use std::sync::Arc;
    use tracing::field::{Field, Visit};
    use tracing::span::Attributes;
    use tracing::Subscriber;
    use tracing_subscriber::layer::Context as LayerContext;
    use tracing_subscriber::registry::LookupSpan;
    use tracing_subscriber::Layer;

    #[derive(Default, Clone)]
    struct TestSpanCapture {
        observations: Arc<Mutex<Vec<(String, String)>>>,
    }

    impl TestSpanCapture {
        fn layer(&self) -> TestSpanCaptureLayer {
            TestSpanCaptureLayer {
                observations: Arc::clone(&self.observations),
            }
        }

        fn into_observations(self) -> Vec<(String, String)> {
            let lock = self
                .observations
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            lock.clone()
        }
    }

    struct TestSpanCaptureLayer {
        observations: Arc<Mutex<Vec<(String, String)>>>,
    }

    impl<S> Layer<S> for TestSpanCaptureLayer
    where
        S: Subscriber + for<'a> LookupSpan<'a>,
    {
        fn on_new_span(&self, attrs: &Attributes<'_>, _id: &tracing::Id, _cx: LayerContext<'_, S>) {
            let mut visitor = AttrVisitor {
                out: Arc::clone(&self.observations),
            };
            attrs.values().record(&mut visitor);
        }

        // Capture post-creation `Span::record()` calls so a regression that
        // records `authorization` AFTER span creation (the natural shape of
        // an interceptor-side leak) is caught (per test review finding 1).
        fn on_record(
            &self,
            _id: &tracing::Id,
            values: &tracing::span::Record<'_>,
            _cx: LayerContext<'_, S>,
        ) {
            let mut visitor = AttrVisitor {
                out: Arc::clone(&self.observations),
            };
            values.record(&mut visitor);
        }
    }

    struct AttrVisitor {
        out: Arc<Mutex<Vec<(String, String)>>>,
    }

    impl Visit for AttrVisitor {
        fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
            let mut lock = self
                .out
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            lock.push((field.name().to_string(), format!("{value:?}")));
        }
        fn record_str(&mut self, field: &Field, value: &str) {
            let mut lock = self
                .out
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            lock.push((field.name().to_string(), value.to_string()));
        }
    }
}
