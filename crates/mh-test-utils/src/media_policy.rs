//! `RegisterMeetingRequest` fixture builders for the ADR-0036 §8 control plane.
//!
//! # Why these live here rather than in three test modules
//!
//! `EgressStream` and `RegisterMeetingRequest` literals were being rebuilt
//! byte-for-byte in `mh-service`'s `routing` unit tests, its `grpc::mh_service`
//! unit tests and its `policy_apply_integration` binary. That is three homes
//! for one fixture, and the part that makes it more than cosmetic is the
//! DECLARATION every registration must carry: MH has one reading of a
//! registration (its `source_streams`), and an egress whose candidate is not
//! declared there is rejected. [`register_request`] declares what MC would, so
//! that rule lives in one place. Three copies means three places to find, and
//! the one that is missed keeps passing while asserting the old world.
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
    CandidateSource, EgressStream, RegisterMeetingRequest, SourceDisposition, SourceStream,
    SubscriberSlot,
};
use proto_gen::dark_tower::signaling::v1::TransportMode;

/// The MC identity every fixture registration claims.
pub const TEST_MC_ID: &str = "mc-1";

/// The MC gRPC endpoint every fixture registration claims.
pub const TEST_MC_ENDPOINT: &str = "http://mc:50052";

/// One egress stream: `subscriber` receives `candidate`'s stream 0. Its
/// transport is the candidate's declared one (DATAGRAM via
/// [`register_request`]); an `EgressStream` carries none of its own.
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
    }
}

/// The loopback shape: one participant subscribed to their own stream.
#[must_use]
pub fn loopback_egress(egress_stream_id: u32, sender: u32, slot: u32) -> EgressStream {
    egress(egress_stream_id, sender, slot, sender)
}

/// A well-formed, MC-SHAPED registration carrying `streams` at
/// `policy_generation`: `source_streams` declares every distinct candidate
/// `(sender_id, stream_number)` the egresses name, DATAGRAM and FORWARD, in
/// first-appearance order — what MC declares for an audio-only meeting.
///
/// Only NAMED candidates are declared. A sender with no edge here is
/// undeclared, so its frames drop `unmatched_stream`; a test that needs such a
/// sender's frames to reach `no_subscriber` declares it explicitly
/// ([`register_request_declared`] / [`declare_forward`]). The request is built
/// from `streams` once: a test that mutates `candidate_sources` afterwards must
/// re-declare, or MH rejects the undeclared candidate — which is the rule.
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
    let mut source_streams: Vec<SourceStream> = Vec::new();
    for candidate in streams.iter().flat_map(|s| s.candidate_sources.iter()) {
        let named = source_streams.iter().any(|d| {
            d.sender_id == candidate.sender_id && d.stream_number == candidate.stream_number
        });
        if !named {
            source_streams.push(source_stream(
                candidate.sender_id,
                candidate.stream_number,
                TransportMode::Datagram,
                SourceDisposition::Forward,
            ));
        }
    }
    RegisterMeetingRequest {
        meeting_id: meeting_id.to_string(),
        mc_id: TEST_MC_ID.to_string(),
        mc_grpc_endpoint: TEST_MC_ENDPOINT.to_string(),
        egress_streams: streams,
        selection_rules: None,
        policy_generation,
        source_streams,
    }
}

/// Declare `senders`' stream 0 DATAGRAM FORWARD on `request` where not
/// already declared — for senders with no edge whose frames a test expects at
/// `no_subscriber` rather than `unmatched_stream`.
pub fn declare_forward(request: &mut RegisterMeetingRequest, senders: &[u32]) {
    for &sender_id in senders {
        if !request
            .source_streams
            .iter()
            .any(|d| d.sender_id == sender_id && d.stream_number == 0)
        {
            request.source_streams.push(source_stream(
                sender_id,
                0,
                TransportMode::Datagram,
                SourceDisposition::Forward,
            ));
        }
    }
}

/// [`register_request`] with every sender in `muted` server-muted at the
/// SENDER level (story 2 R-9, story 3 R-16): each of its declared streams is
/// `DROP_AT_INGRESS`, and a muted sender no edge names is declared at stream 0
/// `DROP_AT_INGRESS` so the mute is held for it too (MH's sender-level
/// shortcut needs at least one declared stream).
///
/// A separate builder rather than a parameter on [`register_request`], so the
/// mute is visible at every call site that sets one and the dozens of sites
/// that do not stay untouched.
#[must_use]
pub fn register_request_muted(
    meeting_id: &str,
    policy_generation: u64,
    streams: Vec<EgressStream>,
    muted: &[u32],
) -> RegisterMeetingRequest {
    let mut request = register_request(meeting_id, policy_generation, streams);
    for &sender_id in muted {
        let mut declared = false;
        for d in &mut request.source_streams {
            if d.sender_id == sender_id {
                d.disposition = SourceDisposition::DropAtIngress as i32;
                declared = true;
            }
        }
        if !declared {
            request.source_streams.push(source_stream(
                sender_id,
                0,
                TransportMode::Datagram,
                SourceDisposition::DropAtIngress,
            ));
        }
    }
    request
}

/// One egress stream: `subscriber`'s `slot` receives `candidate`'s stream
/// `stream` (story 3). The edge's transport derives from the candidate's
/// declared `SourceStream`.
#[must_use]
pub fn egress_from(
    egress_stream_id: u32,
    subscriber_sender: u32,
    slot: u32,
    candidate_sender: u32,
    stream: u32,
) -> EgressStream {
    let mut e = egress(egress_stream_id, subscriber_sender, slot, candidate_sender);
    e.candidate_sources[0].stream_number = stream;
    e
}

/// One declared source stream (story 3; `internal.proto` `SourceStream`).
#[must_use]
pub fn source_stream(
    sender_id: u32,
    stream_number: u32,
    transport: TransportMode,
    disposition: SourceDisposition,
) -> SourceStream {
    SourceStream {
        sender_id,
        stream_number,
        transport_mode: transport as i32,
        disposition: disposition as i32,
    }
}

/// `register_request` with `source_streams` REPLACED by exactly `sources` —
/// for tests that state every declaration themselves.
#[must_use]
pub fn register_request_declared(
    meeting_id: &str,
    policy_generation: u64,
    streams: Vec<EgressStream>,
    sources: Vec<SourceStream>,
) -> RegisterMeetingRequest {
    let mut request = register_request(meeting_id, policy_generation, streams);
    request.source_streams = sources;
    request
}

/// One STREAM-transport (video-shaped) egress: `subscriber_sender`'s `slot`
/// fed by `candidate_sender`'s `stream`, at MC's video priority group (2) and
/// superseding on an independently-decodable frame. The transport derives
/// from the candidate's declared `SourceStream`.
#[must_use]
pub fn stream_egress(
    egress_stream_id: u32,
    subscriber_sender: u32,
    slot: u32,
    candidate_sender: u32,
    stream: u32,
) -> EgressStream {
    let mut e = egress_from(
        egress_stream_id,
        subscriber_sender,
        slot,
        candidate_sender,
        stream,
    );
    e.priority_group = 2;
    e.supersede_on_independent_frame = true;
    e
}
