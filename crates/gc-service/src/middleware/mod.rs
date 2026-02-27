//! Middleware for Global Controller.
//!
//! This module contains HTTP middleware layers for the GC service.
//!
//! # Components
//!
//! - `auth` - Authentication middleware for protected routes
//! - `http_metrics` - HTTP request metrics middleware (ADR-0011)

pub mod auth;
pub mod http_metrics;

pub use auth::{require_auth, require_user_auth, AuthState};
pub use http_metrics::http_metrics_middleware;
