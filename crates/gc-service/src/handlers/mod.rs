//! HTTP request handlers for Global Controller.

pub mod health;
pub mod me;
pub mod meetings;
pub mod metrics;

pub use health::{health_check, readiness_check};
pub use me::get_me;
pub use meetings::{create_meeting, get_guest_token, join_meeting, update_meeting_settings};
pub use metrics::metrics_handler;
