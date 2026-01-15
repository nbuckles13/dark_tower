//! Authentication module for Global Controller.
//!
//! This module handles JWT validation via the Authentication Controller's JWKS endpoint.
//!
//! # Components
//!
//! - `jwks` - JWKS client for fetching and caching public keys from AC
//! - `jwt` - JWT validation using cached JWKS keys
//! - `claims` - JWT claims structure for validated tokens

pub mod claims;
pub mod jwks;
pub mod jwt;

pub use claims::Claims;
pub use jwks::JwksClient;
pub use jwt::JwtValidator;
