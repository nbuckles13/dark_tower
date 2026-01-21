//! Background tasks for Global Controller.
//!
//! Provides long-running background tasks for maintenance operations.

pub mod health_checker;

pub use health_checker::start_health_checker;
