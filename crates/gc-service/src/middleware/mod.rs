//! Middleware for Global Controller.
//!
//! This module contains HTTP middleware layers for the GC service.
//!
//! # Components
//!
//! - `auth` - Authentication middleware for protected routes
//! - `http_metrics` - HTTP request metrics middleware (ADR-0011)
//! - `otel` - Inbound W3C trace-context extraction middleware (R-56)

pub mod auth;
pub mod http_metrics;
pub mod otel;

pub use auth::{require_auth, require_user_auth, AuthState};
pub use http_metrics::http_metrics_middleware;
pub use otel::extract_trace_context_middleware;
