//! Background tasks for Global Controller.
//!
//! Provides long-running background tasks for maintenance operations.
//!
//! # Tasks
//!
//! - `health_checker` - Monitors MC heartbeats and marks stale controllers unhealthy
//! - `mh_health_checker` - Monitors MH heartbeats and marks stale handlers unhealthy
//! - `generic_health_checker` - Shared health checker loop used by MC and MH checkers
//! - `assignment_cleanup` - Cleans up stale and old meeting assignments

pub mod assignment_cleanup;
pub mod generic_health_checker;
pub mod health_checker;
pub mod mh_health_checker;

pub use assignment_cleanup::{start_assignment_cleanup, AssignmentCleanupConfig};
pub use health_checker::start_health_checker;
pub use mh_health_checker::start_mh_health_checker;
