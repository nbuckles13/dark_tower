//! Service layer for Global Controller.
//!
//! This module contains services that interact with external systems
//! and encapsulate business logic.
//!
//! # Components
//!
//! - `ac_client` - HTTP client for Auth Controller internal endpoints
//! - `mc_assignment` - Meeting Controller assignment with load balancing

pub mod ac_client;
pub mod mc_assignment;

pub use mc_assignment::McAssignmentService;
