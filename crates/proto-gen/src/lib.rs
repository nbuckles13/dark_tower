//! Generated Protocol Buffer code for Dark Tower signaling messages.
//!
//! This crate contains the compiled Protocol Buffer definitions used for
//! communication between Dark Tower components.
//!
//! Module paths mirror the proto package hierarchy so the schema version is
//! visible at every consumer use-site: `proto_gen::dark_tower::signaling::v1`,
//! `proto_gen::dark_tower::internal::v1`. No flat re-export aliases — adding
//! a future v2 will not silently rename v1 message semantics for any caller.

#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::doc_markdown)] // Generated code has various doc formatting
#![allow(clippy::default_trait_access)] // Generated code uses Default::default()
#![allow(clippy::too_many_lines)] // Generated code has long functions
#![allow(clippy::struct_excessive_bools)] // Generated protobuf structs may have many bool fields

// Re-export prost traits for convenience
pub use prost::Message;

// Re-export tonic for gRPC service traits
pub use tonic;

// Generated protobuf modules — paths mirror proto package hierarchy.
pub mod dark_tower {
    pub mod signaling {
        pub mod v1 {
            include!(concat!(env!("OUT_DIR"), "/dark_tower.signaling.v1.rs"));
        }
    }

    pub mod internal {
        pub mod v1 {
            include!(concat!(env!("OUT_DIR"), "/dark_tower.internal.v1.rs"));
        }
    }
}
