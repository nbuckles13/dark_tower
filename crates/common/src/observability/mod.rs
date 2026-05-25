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
//! - [`testing`] — test-side `MetricAssertion` helper, only compiled when
//!   `cfg(test)` is active or the `test-utils` feature is enabled, so
//!   production builds of consumer services do not pull in `metrics-util`
//!   or the `metrics` facade through this crate.

pub mod otel;
pub mod otel_grpc;

#[cfg(any(test, feature = "test-utils"))]
pub mod testing;
