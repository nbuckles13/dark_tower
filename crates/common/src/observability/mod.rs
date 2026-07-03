//! Observability utilities shared across Dark Tower services.
//!
//! This module hosts cross-service observability primitives that must not
//! live inside a single service crate:
//!
//! - [`otel`] — OpenTelemetry SDK init helper (R-54). Each service calls
//!   `init_otel` from its `main.rs` to wire OTLP-gRPC export, the W3C
//!   trace-context propagator, sampling, and resource attributes.
//! - [`otel_grpc`] — Tonic interceptors and the bounded W3C propagator
//!   (R-56) for trace-context inject/extract on gRPC client and server
//!   boundaries.
//! - [`otel_http`] — framework-agnostic `http::HeaderMap` inject/extract
//!   functions (R-56) for trace-context propagation on HTTP client and
//!   server boundaries. Sibling to [`otel_grpc`], reuses the same globally
//!   registered `BoundedTraceContextPropagator`.
//! - [`testing`] — test-side `MetricAssertion` helper, only compiled when
//!   `cfg(test)` is active or the `test-utils` feature is enabled, so
//!   production builds of consumer services do not pull in `metrics-util`
//!   or the `metrics` facade through this crate.

pub mod otel;
pub mod otel_grpc;
pub mod otel_http;

#[cfg(any(test, feature = "test-utils"))]
pub mod testing;
