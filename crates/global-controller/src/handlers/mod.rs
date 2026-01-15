//! HTTP request handlers for Global Controller.

pub mod health;
pub mod me;

pub use health::health_check;
pub use me::get_me;
