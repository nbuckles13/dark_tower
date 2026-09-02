//! `RegisterMeetingRequest` fixture builders for the ADR-0036 §8 control plane.
//!
//! # Why these live here rather than in three test modules
//!
//! `EgressStream` and `RegisterMeetingRequest` literals were being rebuilt
//! byte-for-byte in `mh-service`'s `routing` unit tests, its `grpc::mh_service`
//! unit tests and its `policy_apply_integration` binary. That is three homes
//! for one fixture, and the field that makes it more than cosmetic is
//! [`TransportMode::Datagram`]: it is the fixture default all three
//! independently assert against, and story 3 (video) changes what a default
//! egress stream looks like. Three copies means three places to find, and the
//! one that is missed keeps passing while asserting the old world.
//!
//! This crate is the only home reachable from **all three** — it is a
//! `[dev-dependencies]` edge of `mh-service`, so both `#[cfg(test)]` modules
//! inside `src/` and the `tests/` integration binaries can import it.
//!
//! # This does not dilute the crate's stated character
//!
//! The crate doc argues `mh-test-utils` is "different in kind" from its
//! siblings because [`crate::transport_shim`] implements a **production
//! trait** that production code is generic over, which is why that module
//! carries a file-scoped deny of the panic lints. That argument is about
//! `transport_shim` specifically and is untouched here: these are inert proto
//! message builders, substitutable for nothing, and they implement no trait.
//! They are ordinary fixtures of exactly the kind the manifest already
//! describes this crate as providing.

use proto_gen::dark_tower::internal::v1::{
    CandidateSource, EgressStream, RegisterMeetingRequest, SubscriberSlot,
};
use proto_gen::dark_tower::signaling::v1::TransportMode;

/// The MC identity every fixture registration claims.
pub const TEST_MC_ID: &str = "mc-1";

/// The MC gRPC endpoint every fixture registration claims.
pub const TEST_MC_ENDPOINT: &str = "http://mc:50052";

/// One datagram egress stream: `subscriber` receives `candidate`'s stream 0.
///
/// The subscriber and the candidate are named separately on purpose. Passing
/// the same id for both is the loopback shape ("hear yourself"), and passing
/// different ids is the ordinary forwarding shape — a builder that collapsed
/// them into one parameter could not express the second, and a builder that
/// only expressed the second could not express loopback.
#[must_use]
pub fn egress(
    egress_stream_id: u32,
    subscriber_sender: u32,
    slot: u32,
    candidate_sender: u32,
) -> EgressStream {
    EgressStream {
        egress_stream_id,
        subscriber: Some(SubscriberSlot {
            sender_id: subscriber_sender,
            slot_id: slot,
        }),
        candidate_sources: vec![CandidateSource {
            sender_id: candidate_sender,
            stream_number: 0,
        }],
        priority_group: 0,
        supersede_on_independent_frame: false,
        // The fixture default, and the reason this builder has one home:
        // story 3 (video) changes what a default egress stream looks like.
        transport_mode: TransportMode::Datagram as i32,
    }
}

/// The loopback shape: one participant subscribed to their own stream.
#[must_use]
pub fn loopback_egress(egress_stream_id: u32, sender: u32, slot: u32) -> EgressStream {
    egress(egress_stream_id, sender, slot, sender)
}

/// A well-formed registration carrying `streams` at `policy_generation`.
///
/// The caller-identity scalars are always valid here: every test that needs a
/// malformed `meeting_id`, `mc_id` or `mc_grpc_endpoint` is testing the
/// PRE-boundary checks, and mutates the returned value so the departure from
/// well-formed is visible at the assertion site rather than hidden in a
/// builder argument.
#[must_use]
pub fn register_request(
    meeting_id: &str,
    policy_generation: u64,
    streams: Vec<EgressStream>,
) -> RegisterMeetingRequest {
    RegisterMeetingRequest {
        meeting_id: meeting_id.to_string(),
        mc_id: TEST_MC_ID.to_string(),
        mc_grpc_endpoint: TEST_MC_ENDPOINT.to_string(),
        egress_streams: streams,
        selection_rules: None,
        policy_generation,
    }
}
