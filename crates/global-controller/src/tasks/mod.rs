//! Background tasks for Global Controller.
//!
//! Provides long-running background tasks for maintenance operations.
//!
//! # Tasks
//!
//! - `health_checker` - Monitors MC heartbeats and marks stale controllers unhealthy
//! - `assignment_cleanup` - Cleans up stale and old meeting assignments

pub mod assignment_cleanup;
pub mod health_checker;

pub use assignment_cleanup::{start_assignment_cleanup, AssignmentCleanupConfig};
pub use health_checker::start_health_checker;
