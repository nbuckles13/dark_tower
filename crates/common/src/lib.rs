//! Common utilities and types shared across Dark Tower components.

#![warn(clippy::pedantic)]

/// Module for common error types
pub mod error;

/// Module for common data types
pub mod types;

/// Module for common configuration
pub mod config;

/// Module for secret types that prevent accidental logging
pub mod secret;

/// Module for JWT utilities (validation, claims, constants)
pub mod jwt;

/// Module for OAuth 2.0 token management with automatic refresh
pub mod token_manager;

/// Shared types for internal meeting/guest token requests (GC <-> AC)
pub mod meeting_token;

/// Observability utilities shared across services.
///
/// The `observability::testing` submodule is only compiled when `cfg(test)`
/// is active or the `test-utils` feature is enabled.
pub mod observability;
