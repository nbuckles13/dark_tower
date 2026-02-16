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
// MC client types exposed for external use
pub use mc_client::{McClient, McClientTrait};
// Mock MC client for testing (exposed for integration tests)
#[allow(unused_imports)]
pub use mc_client::mock::MockMcClient;
// MH selection types exposed for external/test use
#[allow(unused_imports)]
pub use mh_selection::{MhAssignmentInfo, MhSelection, MhSelectionService};
