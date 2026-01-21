//! Generated Protocol Buffer code for Dark Tower signaling messages.
//!
//! This crate contains the compiled Protocol Buffer definitions used for
//! communication between Dark Tower components.

#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::doc_markdown)] // Generated code has various doc formatting
#![allow(clippy::default_trait_access)] // Generated code uses Default::default()
#![allow(clippy::too_many_lines)] // Generated code has long functions

// Re-export prost traits for convenience
pub use prost::Message;

// Re-export tonic for gRPC service traits
pub use tonic;

// Generated protobuf modules
pub mod signaling {
    //! Client-server signaling messages
    include!("generated/dark_tower.signaling.rs");
}

pub mod internal {
    //! Internal service-to-service messages with gRPC service traits
    include!("generated/dark_tower.internal.rs");
}
