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
