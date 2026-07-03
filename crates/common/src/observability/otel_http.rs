//! HTTP trace-context propagation for Dark Tower (R-56).
//!
//! Framework-agnostic HTTP counterpart to [`crate::observability::otel_grpc`]:
//! adapts the same globally-registered `BoundedTraceContextPropagator`
//! (installed by [`crate::observability::otel::init_otel`]) to
//! [`http::HeaderMap`] instead of `tonic::MetadataMap`. Two pieces, mirroring
//! `otel_grpc.rs`'s shape:
//!
//! 1. [`HeaderInjector`]/[`HeaderExtractor`] — adapters implementing
//!    [`opentelemetry::propagation::Injector`]/[`opentelemetry::propagation::Extractor`]
//!    over `&mut http::HeaderMap` / `&http::HeaderMap`. `http::HeaderMap` is
//!    shared by axum (inbound) and reqwest (outbound —
//!    `reqwest::header::HeaderMap` is a re-export of `http::HeaderMap`), so
//!    one adapter pair serves both directions.
//!
//! 2. [`inject_trace_context`]/[`extract_trace_context`] — plain functions
//!    (NOT framework-specific middleware) that inject/extract via the
//!    adapters and the global propagator. Callers own the framework glue:
//!    gc-service's `middleware::otel` wraps [`extract_trace_context`] in an
//!    axum `middleware::from_fn` for inbound requests; `ac_client.rs` /
//!    `telemetry_forwarder.rs` call [`inject_trace_context`] directly on a
//!    `HeaderMap` before dispatching a `reqwest` request. `common`
//!    intentionally has no `axum` dependency — this module only depends on
//!    the framework-agnostic `http` crate.
//!
//! # Security contract (mirrors `otel_grpc.rs`, user-story line 411)
//!
//! - Inject/extract touch ONLY [`PROPAGATED_HEADERS`] (`traceparent`,
//!   `tracestate`), re-exported from [`crate::observability::otel_grpc`]
//!   rather than declared as a second copy of the allowlist.
//! - [`HeaderInjector`] implements `Injector::set()` only — no method reads
//!   or iterates the target map, so an accidental read-and-forward of
//!   `authorization`/`cookie` (which may live in the same `HeaderMap` a
//!   caller is injecting into) is structurally impossible, not just
//!   test-covered.
//! - [`HeaderExtractor::get`] is a single keyed lookup; `keys()` returns
//!   header names only, never values.
//! - W3C bounds enforcement (length / list-member caps) is NOT reimplemented
//!   here — the globally registered `BoundedTraceContextPropagator` already
//!   enforces it generically over any `Extractor`, HTTP included.
//!
//! Authored `--paired-with=observability` per Gate-1 classification
//! (Domain-judgment: same shape as task #24's `otel.rs`/`otel_grpc.rs`,
//! first-of-kind new file with a security-sensitive public API).

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use http::{HeaderMap, HeaderName, HeaderValue};
use opentelemetry::propagation::{Extractor, Injector};
use tracing_opentelemetry::OpenTelemetrySpanExt;

// Reuse the gRPC module's allowlist rather than declaring a second copy of
// the same two-string security contract (per @dry-reviewer, Gate-1).
pub use crate::observability::otel_grpc::PROPAGATED_HEADERS;

// =============================================================================
// http::HeaderMap adapters: HeaderMap ↔ TextMap{Injector,Extractor}
// =============================================================================

/// Injector adapter over `&mut http::HeaderMap`.
///
/// Write-only by construction: the only method is `set()`, which inserts by
/// key. There is no method on this type that reads or iterates the map it is
/// writing into — a caller cannot accidentally read an existing
/// `Authorization`/`Cookie` entry through this adapter, because the type
/// simply has no such capability.
struct HeaderInjector<'a> {
    headers: &'a mut HeaderMap,
}

impl Injector for HeaderInjector<'_> {
    fn set(&mut self, key: &str, value: String) {
        // The propagator only ever calls `set` with the two
        // `PROPAGATED_HEADERS` values, both of which parse cleanly as
        // `HeaderName`/`HeaderValue`. Fallible APIs, silently discard any
        // failure rather than `unwrap`/`expect` (module-level
        // deny(unwrap_used, panic), mirrors `otel_grpc.rs`'s `MetadataInjector`).
        if let (Ok(name), Ok(val)) = (
            HeaderName::from_bytes(key.as_bytes()),
            HeaderValue::from_str(&value),
        ) {
            self.headers.insert(name, val);
        }
    }
}

/// Extractor adapter over `&http::HeaderMap`.
///
/// Keyed-lookup-only: `get()` returns a single header's value, `keys()`
/// returns header names only (never values) — no path exists through this
/// adapter that could copy a header *value* into a span attribute or log
/// line.
struct HeaderExtractor<'a> {
    headers: &'a HeaderMap,
}

impl Extractor for HeaderExtractor<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        // `to_str()` returns `Err` for non-UTF8 header values (unlike
        // `tonic::MetadataValue`, which is ASCII-only by construction,
        // `http::HeaderValue` may hold opaque bytes via `from_bytes`). Map
        // to `None` — treated as absent, not a panic (security review item 2).
        self.headers.get(key).and_then(|v| v.to_str().ok())
    }

    fn keys(&self) -> Vec<&str> {
        self.headers.keys().map(HeaderName::as_str).collect()
    }
}

// =============================================================================
// Public inject / extract functions
// =============================================================================

/// Inject the active `OTel` context into `headers` as `traceparent` /
/// `tracestate`.
///
/// Reads from the active `tracing::Span`'s `OTel` context; if no span is
/// active, no headers are injected (the request dispatches with no
/// propagated parent). Never reads any existing entry in `headers` — safe to
/// call on a `HeaderMap` that already carries `Authorization` or other
/// sensitive headers (the write-only [`HeaderInjector`] cannot read them).
pub fn inject_trace_context(headers: &mut HeaderMap) {
    let cx = tracing::Span::current().context();
    opentelemetry::global::get_text_map_propagator(|propagator| {
        propagator.inject_context(&cx, &mut HeaderInjector { headers });
    });
}

/// Extract the W3C trace context from `headers` and attach it as the parent
/// of the current `tracing::Span`.
///
/// Reads ONLY `traceparent`/`tracestate` (per [`PROPAGATED_HEADERS`]). Any
/// other header (including `Authorization`) is left untouched. W3C bounds
/// enforcement happens inside the globally registered
/// `BoundedTraceContextPropagator` — this function does not duplicate that
/// logic; an out-of-bounds `traceparent`/`tracestate` simply fails to attach
/// a parent (the request continues as a new root span).
pub fn extract_trace_context(headers: &HeaderMap) {
    let parent_cx = opentelemetry::global::get_text_map_propagator(|propagator| {
        propagator.extract(&HeaderExtractor { headers })
    });
    tracing::Span::current().set_parent(parent_cx);
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
    use crate::observability::otel_grpc::BoundedTraceContextPropagator;
    use opentelemetry::trace::{
        SpanContext, SpanId, TraceContextExt, TraceFlags, TraceId, TraceState,
    };
    use opentelemetry::Context;
    use std::sync::{Mutex, OnceLock};

    // Serializes tests that touch the global propagator slot, mirrors
    // `otel_grpc.rs`'s `GLOBAL_PROPAGATOR_LOCK`.
    static GLOBAL_PROPAGATOR_LOCK: Mutex<()> = Mutex::new(());

    fn ensure_bounded_propagator_installed() {
        static INSTALLED: OnceLock<()> = OnceLock::new();
        INSTALLED.get_or_init(|| {
            opentelemetry::global::set_text_map_propagator(BoundedTraceContextPropagator::new());
        });
    }

    const KNOWN_TRACE_ID_U128: u128 = 0x4bf9_2f35_77b3_4da6_a3ce_929d_0e0e_4736;
    const KNOWN_SPAN_ID_U64: u64 = 0x00f0_67aa_0ba9_02b7;

    fn known_span_context() -> SpanContext {
        SpanContext::new(
            TraceId::from(KNOWN_TRACE_ID_U128),
            SpanId::from(KNOWN_SPAN_ID_U64),
            TraceFlags::SAMPLED,
            true,
            TraceState::default(),
        )
    }

    fn known_traceparent_header() -> String {
        format!("00-{KNOWN_TRACE_ID_U128:032x}-{KNOWN_SPAN_ID_U64:016x}-01")
    }

    /// Build a `tracing_subscriber::Registry` with the OpenTelemetry layer
    /// installed against a noop tracer provider — required for
    /// `OpenTelemetrySpanExt::{context, set_parent}` to actually wire span
    /// context, mirrors `otel_grpc.rs`'s `with_test_subscriber`.
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
    fn inject_writes_traceparent_from_active_span_parent() {
        let _g = GLOBAL_PROPAGATOR_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        ensure_bounded_propagator_installed();

        let mut headers = HeaderMap::new();
        with_test_subscriber(|| {
            let span = tracing::info_span!("test_inject");
            let parent_cx = Context::new().with_remote_span_context(known_span_context());
            span.set_parent(parent_cx);
            span.in_scope(|| inject_trace_context(&mut headers));
        });

        let tp = headers
            .get("traceparent")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        assert!(
            tp.starts_with("00-"),
            "traceparent header missing or malformed: {tp:?}"
        );
        let trace_id_hex = tp.get(3..35).unwrap_or_default();
        assert_eq!(
            trace_id_hex,
            format!("{KNOWN_TRACE_ID_U128:032x}"),
            "injected trace_id should match the active span's parent"
        );
    }

    #[test]
    fn extract_attaches_parent_from_headers() {
        let _g = GLOBAL_PROPAGATOR_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        ensure_bounded_propagator_installed();

        let mut headers = HeaderMap::new();
        headers.insert(
            "traceparent",
            HeaderValue::from_str(&known_traceparent_header())
                .unwrap_or_else(|e| panic!("known traceparent must parse: {e}")),
        );

        let recovered = with_test_subscriber(|| {
            let span = tracing::info_span!("test_extract");
            span.in_scope(|| {
                extract_trace_context(&headers);
                tracing::Span::current().context()
            })
        });

        let sc = recovered.span().span_context().clone();
        assert!(sc.is_valid(), "extracted parent context should be valid");
        assert_eq!(sc.trace_id(), TraceId::from(KNOWN_TRACE_ID_U128));
    }

    #[test]
    fn round_trip_inject_then_extract_recovers_trace_id() {
        let _g = GLOBAL_PROPAGATOR_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        ensure_bounded_propagator_installed();

        let mut headers = HeaderMap::new();
        with_test_subscriber(|| {
            let span = tracing::info_span!("test_round_trip_inject");
            span.set_parent(Context::new().with_remote_span_context(known_span_context()));
            span.in_scope(|| inject_trace_context(&mut headers));
        });

        let recovered = with_test_subscriber(|| {
            let span = tracing::info_span!("test_round_trip_extract");
            span.in_scope(|| {
                extract_trace_context(&headers);
                tracing::Span::current().context()
            })
        });

        assert_eq!(
            recovered.span().span_context().trace_id(),
            TraceId::from(KNOWN_TRACE_ID_U128)
        );
    }

    /// W3C bounds are enforced by the globally registered
    /// `BoundedTraceContextPropagator`, generically over any `Extractor` —
    /// this exercises that the HTTP adapter correctly delegates to it (does
    /// NOT duplicate the bounds logic itself).
    ///
    /// Tests the `HeaderExtractor` + propagator directly against a bare
    /// `Context::new()`, mirroring `otel_grpc.rs`'s own
    /// `bounded_propagator_rejects_*` tests — NOT through a `tracing::Span`,
    /// because a tracing span under `OpenTelemetryLayer` always gets ITS OWN
    /// valid auto-generated `SpanContext` once entered (the layer assigns a
    /// fresh `trace_id` when no valid parent attaches), so `Span::current()`'s
    /// context is "valid" either way and can't distinguish accept from
    /// reject. The propagator-level `Context` is the correct assertion point.
    #[test]
    fn extract_rejects_overlong_traceparent_via_shared_bounded_propagator() {
        let _g = GLOBAL_PROPAGATOR_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        ensure_bounded_propagator_installed();

        let mut headers = HeaderMap::new();
        headers.insert(
            "traceparent",
            HeaderValue::from_str(&format!("{}-EXTRAEXTRAEXTRA", known_traceparent_header()))
                .unwrap_or_else(|e| panic!("test fixture: {e}")),
        );

        let ctx = opentelemetry::global::get_text_map_propagator(|propagator| {
            propagator.extract(&HeaderExtractor { headers: &headers })
        });

        assert!(
            !ctx.span().span_context().is_valid(),
            "overlong traceparent must NOT attach a parent context"
        );
    }

    /// Non-UTF8 header value → `Extractor::get` returns `None`, not a panic
    /// (security review item 2 — new attacker-reachable surface vs. the gRPC
    /// adapter, since `http::HeaderValue` can hold opaque bytes unlike
    /// `tonic::MetadataValue`, which is ASCII-only by construction). Same
    /// propagator-level assertion point as the overlong-traceparent test
    /// above, for the same reason.
    #[test]
    fn extract_treats_non_utf8_traceparent_as_absent_not_panic() {
        let _g = GLOBAL_PROPAGATOR_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        ensure_bounded_propagator_installed();

        let mut headers = HeaderMap::new();
        // Opaque non-UTF8 bytes — valid as a HeaderValue, invalid as UTF-8.
        headers.insert(
            "traceparent",
            HeaderValue::from_bytes(&[0xFF, 0xFE, 0xFD])
                .unwrap_or_else(|e| panic!("test fixture: {e}")),
        );

        // Must not panic, and must not attach a parent context.
        let ctx = opentelemetry::global::get_text_map_propagator(|propagator| {
            propagator.extract(&HeaderExtractor { headers: &headers })
        });

        assert!(
            !ctx.span().span_context().is_valid(),
            "non-UTF8 traceparent must be treated as absent, not attach a parent"
        );
    }

    /// Inbound `authorization` must not be mutated by extraction.
    #[test]
    fn extract_does_not_mutate_authorization_header() {
        let _g = GLOBAL_PROPAGATOR_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        ensure_bounded_propagator_installed();

        let mut headers = HeaderMap::new();
        let auth_value = "Bearer SECRETxyz";
        headers.insert(
            "authorization",
            HeaderValue::from_str(auth_value).unwrap_or_else(|e| panic!("test fixture: {e}")),
        );
        headers.insert(
            "traceparent",
            HeaderValue::from_str(&known_traceparent_header())
                .unwrap_or_else(|e| panic!("test fixture: {e}")),
        );

        with_test_subscriber(|| {
            let span = tracing::info_span!("test_no_auth_mutate");
            span.in_scope(|| extract_trace_context(&headers));
        });

        let after = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        assert_eq!(
            after, auth_value,
            "extraction must not modify the authorization header"
        );
    }

    /// Outbound injection must NOT copy any pre-existing `authorization`
    /// value into a new header — the write-only `HeaderInjector` never reads
    /// the map it writes into, so this is structurally guaranteed, but this
    /// test locks the observable behavior too.
    #[test]
    fn inject_does_not_read_or_duplicate_authorization_header() {
        let _g = GLOBAL_PROPAGATOR_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        ensure_bounded_propagator_installed();

        let mut headers = HeaderMap::new();
        let auth_value = "Bearer SECRETxyz";
        headers.insert(
            "authorization",
            HeaderValue::from_str(auth_value).unwrap_or_else(|e| panic!("test fixture: {e}")),
        );

        with_test_subscriber(|| {
            let span = tracing::info_span!("test_no_auth_leak_inject");
            span.set_parent(Context::new().with_remote_span_context(known_span_context()));
            span.in_scope(|| inject_trace_context(&mut headers));
        });

        // Authorization untouched.
        let after = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        assert_eq!(after, auth_value, "injection must not modify authorization");

        // Only PROPAGATED_HEADERS keys were added alongside it.
        for key in headers.keys() {
            assert!(
                key.as_str() == "authorization" || PROPAGATED_HEADERS.contains(&key.as_str()),
                "unexpected header key injected: {key:?}"
            );
        }
    }

    /// The injected header keyset MUST be a subset of `PROPAGATED_HEADERS`
    /// (beyond whatever the caller already had in the map).
    #[test]
    fn inject_only_adds_propagated_headers_keys() {
        let _g = GLOBAL_PROPAGATOR_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        ensure_bounded_propagator_installed();

        let mut headers = HeaderMap::new();
        with_test_subscriber(|| {
            let span = tracing::info_span!("test_only_w3c_keys");
            span.set_parent(Context::new().with_remote_span_context(known_span_context()));
            span.in_scope(|| inject_trace_context(&mut headers));
        });

        for key in headers.keys() {
            assert!(
                PROPAGATED_HEADERS.contains(&key.as_str()),
                "injected header key {key:?} is not in PROPAGATED_HEADERS"
            );
        }
    }

    #[test]
    fn propagated_headers_reused_from_otel_grpc_not_redeclared() {
        // Anchors that this module deliberately has no second copy of the
        // allowlist — `PROPAGATED_HEADERS` here IS `otel_grpc::PROPAGATED_HEADERS`.
        assert_eq!(
            PROPAGATED_HEADERS,
            crate::observability::otel_grpc::PROPAGATED_HEADERS
        );
    }
}
