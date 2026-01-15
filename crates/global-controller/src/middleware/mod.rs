//! Middleware for Global Controller.
//!
//! This module contains HTTP middleware layers for the GC service.
//!
//! # Components
//!
//! - `auth` - Authentication middleware for protected routes

pub mod auth;

pub use auth::{require_auth, AuthState};
