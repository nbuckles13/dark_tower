//! Authentication module for Global Controller.
//!
//! This module handles JWT validation via the Authentication Controller's JWKS endpoint.
//!
//! # Components
//!
//! - `jwks` - JWKS client re-exported from common
//! - `jwt` - JWT validation wrapper for GC-specific error mapping
//! - `claims` - JWT claims structure for validated tokens

pub mod claims;
pub mod jwks;
pub mod jwt;

pub use claims::Claims;
pub use jwks::JwksClient;
pub use jwt::JwtValidator;
