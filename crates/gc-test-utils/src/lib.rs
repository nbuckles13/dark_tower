//! # GC Test Utilities
//!
//! Shared test utilities for the Global Controller (GC) service.
//!
//! This crate provides:
//! - Server test harness (`TestGcServer` for E2E tests)
//!
//! ## Usage
//!
//! ```rust,ignore
//! use gc_test_utils::*;
//!
//! #[sqlx::test(migrations = "../../migrations")]
//! async fn test_example(pool: PgPool) -> Result<()> {
//!     let server = TestGcServer::spawn(pool).await?;
//!     let client = reqwest::Client::new();
//!
//!     let response = client
//!         .get(&format!("{}/v1/health", server.url()))
//!         .send()
//!         .await?;
//!
//!     assert_eq!(response.status(), 200);
//!     Ok(())
//! }
//! ```

pub mod server_harness;

// Re-export commonly used items
pub use server_harness::*;
