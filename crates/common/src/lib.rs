//! Common utilities and types shared across Dark Tower components.

#![warn(clippy::pedantic)]
// We standardize on seconds for Duration construction (most timeouts are
// not multiples of 60; config field naming uses seconds; RFC TTL/JWT exp
// conventions use seconds). Opting out of this pedantic sublint avoids
// splitting durations into two unit families on arbitrary divisibility.
#![allow(clippy::duration_suboptimal_units)]

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
