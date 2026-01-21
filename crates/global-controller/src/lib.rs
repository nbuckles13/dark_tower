//! Global Controller (GC) Service Library
//!
//! This library provides the core functionality for the Dark Tower
//! Global Controller - a stateless HTTP/3 API gateway responsible for:
//!
//! - Meeting management (create, schedule, cancel)
//! - Multi-tenancy and geographic routing
//! - Meeting Controller registry and health tracking
//! - Authentication token validation (via Auth Controller JWKS)
//!
//! # Architecture
//!
//! The GC follows the Handler -> Service -> Repository pattern:
//!
//! ```text
//! routes/mod.rs -> handlers/*.rs -> services/*.rs -> repositories/*.rs
//! ```
//!
//! # Modules
//!
//! - `auth` - JWT validation via AC JWKS endpoint
//! - `config` - Service configuration from environment
//! - `errors` - Error types with HTTP status code mapping
//! - `handlers` - HTTP request handlers
//! - `middleware` - HTTP middleware (authentication)
//! - `models` - Data models
//! - `routes` - Axum router setup
//! - `services` - External service clients (AC, etc.)

pub mod auth;
pub mod config;
pub mod errors;
pub mod grpc;
pub mod handlers;
pub mod middleware;
pub mod models;
pub mod repositories;
pub mod routes;
pub mod services;
pub mod tasks;
