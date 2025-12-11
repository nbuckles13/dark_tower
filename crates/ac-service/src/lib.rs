//! Authentication Controller (AC) Service Library
//!
//! This library provides the core authentication and authorization functionality
//! for the Dark Tower video conferencing system.
//!
//! # Modules
//!
//! - `config` - Service configuration
//! - `crypto` - Cryptographic operations (JWT signing, key encryption)
//! - `errors` - Error types
//! - `handlers` - HTTP request handlers
//! - `models` - Data models
//! - `repositories` - Database access layer
//! - `services` - Business logic layer

pub mod config;
pub mod crypto;
pub mod errors;
pub mod handlers;
pub mod models;
pub mod observability;
pub mod repositories;
pub mod services;
