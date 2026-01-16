//! HTTP request handlers for Global Controller.

pub mod health;
pub mod me;
pub mod meetings;

pub use health::health_check;
pub use me::get_me;
pub use meetings::{get_guest_token, join_meeting, update_meeting_settings};
