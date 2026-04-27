//! Shared test fixtures for `gc-service` integration tests.
//!
//! Per ADR-0032 Step 5 + @team-lead authorization (2026-04-27): consolidates
//! the previously-triplicated `TestKeypair` / `build_pkcs8_from_seed` /
//! `TestUserClaims` / `TestServiceClaims` definitions that lived inline in
//! `meeting_tests.rs`, `auth_tests.rs`, and `meeting_create_tests.rs`. Mirrors
//! AC's `crates/ac-service/tests/common/` pattern.
//!
//! Test files include this module via:
//! ```ignore
//! #[path = "common/mod.rs"]
//! mod test_common;
//! ```
//!
//! # Security
//!
//! All keypairs derived here are deterministic from a `u8` seed and must
//! NEVER be used outside test code. The seed-to-PKCS8 derivation is
//! intentionally unguarded against weak input — the threat model is
//! "test fixture," not "production crypto."

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

pub mod jwt_fixtures;
