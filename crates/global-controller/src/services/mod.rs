//! Service layer for Global Controller.
//!
//! This module contains services that interact with external systems
//! and encapsulate business logic.
//!
//! # Components
//!
//! - `ac_client` - HTTP client for Auth Controller internal endpoints
//! - `mc_assignment` - Meeting Controller assignment with load balancing
//! - `mc_client` - gRPC client for GC→MC communication
//! - `mh_selection` - Media Handler selection for meetings

pub mod ac_client;
pub mod mc_assignment;
pub mod mc_client;
pub mod mh_selection;

pub use mc_assignment::McAssignmentService;
// MC client and MH selection types will be used in handlers in future phase
#[allow(unused_imports)]
pub use mc_client::{McAssignmentResult, McClient, McClientTrait, McRejectionReason};
#[allow(unused_imports)]
pub use mh_selection::{MhAssignmentInfo, MhSelection, MhSelectionService};
