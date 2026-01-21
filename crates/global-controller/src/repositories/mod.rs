//! Repository layer for Global Controller.
//!
//! Provides database access patterns following the Handler -> Service -> Repository
//! architecture. All database queries use sqlx compile-time checking.

pub mod meeting_controllers;

pub use meeting_controllers::{HealthStatus, MeetingControllersRepository};
