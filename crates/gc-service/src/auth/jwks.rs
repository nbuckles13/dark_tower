//! JWKS client re-exported from common.
//!
//! The JWKS client, JWK types, and JwksResponse are now in `crates/common/src/jwt.rs`.
//! This module re-exports them for backwards compatibility within GC.

pub use common::jwt::JwksClient;
