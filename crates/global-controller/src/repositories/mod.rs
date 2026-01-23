//! Repository layer for Global Controller.
//!
//! Provides database access patterns following the Handler -> Service -> Repository
//! architecture. All database queries use sqlx compile-time checking.

pub mod meeting_assignments;
pub mod meeting_controllers;

pub use meeting_assignments::{weighted_random_select, McAssignment, MeetingAssignmentsRepository};
// McCandidate and MeetingAssignment are used in tests
#[allow(unused_imports)]
pub use meeting_assignments::{McCandidate, MeetingAssignment};
pub use meeting_controllers::{HealthStatus, MeetingControllersRepository};
