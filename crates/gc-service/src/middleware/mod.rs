//! Middleware for Global Controller.
//!
//! This module contains HTTP middleware layers for the GC service.
//!
//! # Components
//!
//! - `auth` - Authentication middleware for protected routes
//! - `cors_observer` - CORS preflight observer: metric + denied-403 rewrite (R-1/R-52)
//! - `http_metrics` - HTTP request metrics middleware (ADR-0011)
//! - `otel` - Inbound W3C trace-context extraction middleware (R-56)

pub mod auth;
pub mod cors_observer;
pub mod http_metrics;
pub mod otel;

pub use auth::{require_auth, require_user_auth, AuthState};
pub use cors_observer::cors_preflight_observer;
pub use http_metrics::http_metrics_middleware;
pub use otel::extract_trace_context_middleware;
