#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use super::*;
// One fixture home for all three test sites that build these messages —
// see `mh_test_utils::media_policy` for why.
use mh_test_utils::media_policy::{
    egress, egress_from, register_request as request, register_request_declared,
    register_request_muted, source_stream,
};
use proto_gen::dark_tower::internal::v1::{CandidateSource, EgressStream, SourceDisposition};

const DGRAM: TransportMode = TransportMode::Datagram;
const SPG: TransportMode = TransportMode::StreamPerGroup;
const FWD: SourceDisposition = SourceDisposition::Forward;
const PAUSED: SourceDisposition = SourceDisposition::Paused;
const DROP: SourceDisposition = SourceDisposition::DropAtIngress;

/// The publisher's declared send ids in this file (MC's contract values;
/// MH does not depend on the numbers).
const AUDIO: u32 = 0;
const VIDEO: u32 = 1;

fn sender(v: u32) -> SenderId {
    SenderId::from_wire(v).unwrap()
}

/// What one attribution resolved to, collected AT THE TEST SITE (an
/// allocating form on the type would be a footgun the per-frame path would
/// have to remember not to call).
#[derive(Debug, PartialEq, Eq)]
enum Seen {
    NoPolicy,
    ServerMuted,
    Unmatched,
    TransportMismatch,
    /// `(paused, egress_stream_ids)`.
    Route(bool, Vec<u32>),
}

fn attribute(
    snapshot: &RoutingSnapshot,
    meeting: &str,
    from: u32,
    label: u16,
    arrival: TransportMode,
) -> Seen {
    match snapshot.attribute(&MeetingKey::new(meeting), sender(from), label, arrival) {
        Attribution::NoPolicy => Seen::NoPolicy,
        Attribution::ServerMuted => Seen::ServerMuted,
        Attribution::Unmatched => Seen::Unmatched,
        Attribution::TransportMismatch => Seen::TransportMismatch,
        Attribution::Route { paused, edges, .. } => {
            Seen::Route(paused, edges.map(|e| e.edge.egress_stream_id).collect())
        }
    }
}

fn policy(meeting: &str, generation: u64, streams: Vec<EgressStream>) -> MeetingPolicy {
    MeetingPolicy::from_request(
        &request(meeting, generation, streams),
        &PolicyLimits::for_tests(),
    )
    .unwrap()
}

fn parse(req: &RegisterMeetingRequest) -> Result<MeetingPolicy, PolicyRejection> {
    MeetingPolicy::from_request(req, &PolicyLimits::for_tests())
}

/// The mixed meeting every attribution row runs against: publisher 5 declares
/// audio (send id 0, DATAGRAM) and video (send id 1, `STREAM_PER_GROUP`);
/// subscriber 6 holds audio slot 0 (edge 10) and video slot 4 (edge 11).
fn mixed(audio: SourceDisposition, video: SourceDisposition) -> RegisterMeetingRequest {
    register_request_declared(
        "m",
        1,
        vec![
            egress_from(10, 6, 0, 5, AUDIO),
            egress_from(11, 6, 4, 5, VIDEO),
        ],
        vec![
            source_stream(5, AUDIO, DGRAM, audio),
            source_stream(5, VIDEO, SPG, video),
            source_stream(6, AUDIO, DGRAM, FWD),
        ],
    )
}

fn install(req: &RegisterMeetingRequest) -> RoutingTable {
    let table = RoutingTable::unbounded_for_tests();
    table.install(&parse(req).unwrap());
    table
}

// -- identifier widths ------------------------------------------------------

#[test]
fn sender_id_rejects_zero_and_over_range_never_truncates() {
    assert_eq!(SenderId::from_wire(0), Err(IdError::SenderIdZero));
    assert_eq!(
        SenderId::from_wire(65_536),
        Err(IdError::SenderIdOutOfRange)
    );
    // 65536 must NOT wrap to 0, and 65537 must NOT wrap to 1.
    assert_eq!(
        SenderId::from_wire(65_537),
        Err(IdError::SenderIdOutOfRange)
    );
    assert_eq!(SenderId::from_wire(65_535).unwrap().get(), 65_535);
    assert_eq!(SenderId::from_wire(1).unwrap().get(), 1);
}

#[test]
fn slot_id_allows_zero_and_rejects_over_range() {
    // Zero IS valid for a slot — two subscribers both choosing slot 0 is
    // normal, which is why this differs from SenderId.
    assert_eq!(SlotId::from_wire(0).unwrap().get(), 0);
    assert_eq!(SlotId::from_wire(65_535).unwrap().get(), 65_535);
    assert_eq!(SlotId::from_wire(65_536), Err(IdError::SlotIdOutOfRange));
}

#[test]
fn stream_number_is_eight_bit_and_rejects_never_truncates() {
    assert_eq!(StreamNumber::from_wire(255).unwrap().get(), 255);
    // 256 must not alias stream 0 of the same sender.
    assert_eq!(
        StreamNumber::from_wire(256),
        Err(IdError::StreamNumberOutOfRange)
    );
    // The 16-bit uplink label narrows the same way: rejects, never truncates.
    assert_eq!(StreamNumber::try_from(255_u16).unwrap().get(), 255);
    assert_eq!(
        StreamNumber::try_from(256_u16),
        Err(IdError::StreamNumberOutOfRange)
    );
}

// -- the two-meeting acceptance pin -----------------------------------------

/// **The acceptance pin for `docs/TODO.md` §Media Path Obligations (a)**,
/// extended to (sender, stream) by story 3.
///
/// Two meetings are registered into ONE routing table and ONE published
/// snapshot — two separately-constructed tables would pass vacuously and
/// pin nothing. Both arms are asserted deliberately: without the positive arm
/// an implementation that resolves nothing anywhere also passes.
///
/// The corpse this test is looking for: a flat sender- or source-keyed map at
/// snapshot level instead of the per-meeting nesting. Under it, meeting B's
/// lookup for sender 5 returns meeting A's edge, and one meeting's media
/// reaches another meeting's subscriber. **No other validation layer can catch
/// this**: a single meeting cannot distinguish the correct and the leaking
/// implementations.
#[test]
fn sender_id_valid_in_one_meeting_does_not_resolve_in_another() {
    let table = RoutingTable::unbounded_for_tests();
    table.install(&policy("meeting-a", 1, vec![egress(1, 5, 0, 5)]));
    table.install(&policy("meeting-b", 1, vec![egress(1, 7, 0, 7)]));
    let snapshot = table.load();

    // Positive arm: sender 5 resolves inside meeting A.
    assert_eq!(
        attribute(&snapshot, "meeting-a", 5, 0, DGRAM),
        Seen::Route(false, vec![1])
    );
    // Negative arm: meeting B declares nothing for sender 5, so its frame is
    // unmatched — it reaches NOTHING, which is the property.
    assert_eq!(
        attribute(&snapshot, "meeting-b", 5, 0, DGRAM),
        Seen::Unmatched
    );
    // And B's own sender still resolves, so the negative arm is not just
    // "meeting B is empty".
    assert_eq!(
        attribute(&snapshot, "meeting-b", 7, 0, DGRAM),
        Seen::Route(false, vec![1])
    );
    assert_eq!(
        attribute(&snapshot, "meeting-a", 7, 0, DGRAM),
        Seen::Unmatched
    );
}

/// The story-3 extension: a (sender, stream) DECLARED in meeting A does not
/// resolve in meeting B, where the same ordinal declares nothing.
#[test]
fn a_declared_source_stream_in_one_meeting_does_not_resolve_in_another() {
    let table = RoutingTable::unbounded_for_tests();
    table.install(&parse(&mixed(FWD, FWD)).unwrap());
    // The realistic collision: sender ids are meeting-local ordinals, so
    // meeting B has its OWN sender 5 — declaring only its video stream.
    let b = register_request_declared(
        "meeting-b",
        1,
        vec![egress_from(1, 9, 0, 5, VIDEO)],
        vec![source_stream(5, VIDEO, SPG, FWD)],
    );
    table.install(&parse(&b).unwrap());
    let snapshot = table.load();

    // Positive: (m, 5, audio) routes to m's audio edge.
    assert_eq!(
        attribute(&snapshot, "m", 5, 0, DGRAM),
        Seen::Route(false, vec![10])
    );
    // Negative: the SAME (sender 5, audio) does not resolve in meeting B,
    // where sender 5 exists but declares only video. A sender-scoped but not
    // stream-scoped index would resolve it.
    assert_eq!(
        attribute(&snapshot, "meeting-b", 5, 0, DGRAM),
        Seen::Unmatched
    );
    // B's own (5, video) resolves to B's edge only.
    assert_eq!(
        attribute(&snapshot, "meeting-b", 5, 1, SPG),
        Seen::Route(false, vec![1])
    );
}

// -- the attribution outcome table (datagram path) ---------------------------

/// The task's ATTRIBUTION OUTCOME TABLE, through the single lookup. Each row
/// that asserts an absence ("never a video slot") also asserts the audio
/// edge's presence, so the absence is not vacuous.
#[test]
fn attribution_outcome_table_on_the_datagram_path() {
    let fwd = install(&mixed(FWD, FWD)).load();

    // label 0 datagram -> audio slot only.
    assert_eq!(
        attribute(&fwd, "m", 5, 0, DGRAM),
        Seen::Route(false, vec![10])
    );
    // label 1 datagram -> transport_mismatch (video is STREAM_PER_GROUP).
    assert_eq!(attribute(&fwd, "m", 5, 1, DGRAM), Seen::TransportMismatch);
    // label 1 on its declared transport reaches the video edge only (the
    // stream path's lookup, MH-A2).
    assert_eq!(
        attribute(&fwd, "m", 5, 1, SPG),
        Seen::Route(false, vec![11])
    );
    // label 0 on the wrong transport.
    assert_eq!(attribute(&fwd, "m", 5, 0, SPG), Seen::TransportMismatch);
    // undeclared labels -> unmatched_stream.
    assert_eq!(attribute(&fwd, "m", 5, 7, DGRAM), Seen::Unmatched);
    assert_eq!(attribute(&fwd, "m", 5, 255, DGRAM), Seen::Unmatched);
    // 256 = 0x0100: its LOW byte is the declared audio id, so an `as u8`
    // regression would ROUTE it to the audio edge rather than merely not
    // match. It must be unmatched.
    assert_eq!(attribute(&fwd, "m", 5, 256, DGRAM), Seen::Unmatched);
    assert_eq!(attribute(&fwd, "m", 5, u16::MAX, DGRAM), Seen::Unmatched);

    // The patched client: video DROP_AT_INGRESS, video bytes relabelled as
    // audio over datagrams -> audio slot only, never the video slot.
    let video_muted = install(&mixed(FWD, DROP)).load();
    assert_eq!(
        attribute(&video_muted, "m", 5, 0, DGRAM),
        Seen::Route(false, vec![10])
    );
    // The muted stream on its own transport drops server_muted.
    assert_eq!(attribute(&video_muted, "m", 5, 1, SPG), Seen::ServerMuted);
    // ORDER PIN: the transport check precedes the disposition, so the muted
    // stream's video-labelled DATAGRAM is a transport_mismatch, not
    // server_muted. Do not "fix" this order: the remedies differ.
    assert_eq!(
        attribute(&video_muted, "m", 5, 1, DGRAM),
        Seen::TransportMismatch
    );

    // Per-kind server mute of AUDIO while video flows.
    let audio_muted = install(&mixed(DROP, FWD)).load();
    assert_eq!(attribute(&audio_muted, "m", 5, 0, DGRAM), Seen::ServerMuted);
    assert_eq!(
        attribute(&audio_muted, "m", 5, 1, SPG),
        Seen::Route(false, vec![11])
    );

    // Every declared stream DROP_AT_INGRESS -> the sender-level shortcut,
    // FIRST: even an undeclared label and a wrong transport count server_muted
    // (this row is what distinguishes the ordering).
    let all_muted = install(&mixed(DROP, DROP)).load();
    assert_eq!(attribute(&all_muted, "m", 5, 7, DGRAM), Seen::ServerMuted);
    assert_eq!(attribute(&all_muted, "m", 5, 256, DGRAM), Seen::ServerMuted);
    assert_eq!(attribute(&all_muted, "m", 5, 1, DGRAM), Seen::ServerMuted);
    assert!(all_muted.is_server_muted(&MeetingKey::new("m"), sender(5)));
    // ...and the other sender in the meeting is unaffected.
    assert!(!all_muted.is_server_muted(&MeetingKey::new("m"), sender(6)));

    // PAUSED -> forwarded (never dropped) and flagged for the paused counter.
    let paused = install(&mixed(PAUSED, FWD)).load();
    assert_eq!(
        attribute(&paused, "m", 5, 0, DGRAM),
        Seen::Route(true, vec![10])
    );
    // Every stream PAUSED is NOT the all-muted shortcut.
    let all_paused = install(&mixed(PAUSED, PAUSED)).load();
    assert!(!all_paused.is_server_muted(&MeetingKey::new("m"), sender(5)));
    assert_eq!(
        attribute(&all_paused, "m", 5, 0, DGRAM),
        Seen::Route(true, vec![10])
    );

    // A declared stream nobody watches routes to NO edge (no_subscriber).
    assert_eq!(
        attribute(&fwd, "m", 6, 0, DGRAM),
        Seen::Route(false, vec![])
    );

    // No policy for the meeting.
    assert_eq!(attribute(&fwd, "elsewhere", 5, 0, DGRAM), Seen::NoPolicy);
}

/// The shortcut is NON-VACUOUS: a sender with ZERO declared streams is not
/// "every declared stream `DROP_AT_INGRESS`" — its frames are `unmatched_stream`.
#[test]
fn a_sender_with_no_declared_stream_is_unmatched_not_server_muted() {
    let snapshot = install(&mixed(FWD, FWD)).load();
    assert!(!snapshot.is_server_muted(&MeetingKey::new("m"), sender(9)));
    assert_eq!(attribute(&snapshot, "m", 9, 0, DGRAM), Seen::Unmatched);
}

// -- no fallback reading (MC-B) -------------------------------------------------

/// MC-B deleted the story-2 transitional reading: an absent/empty
/// `source_streams` declares NOTHING, so every candidate is undeclared and the
/// registration rejects `rejected_transport_inconsistent` through the generic
/// undeclared-candidate rule — no new token, never a stream-0 fallback.
#[test]
fn empty_source_streams_with_a_candidate_bearing_egress_rejects_transport_inconsistent() {
    let mut req = request("m", 1, vec![egress(1, 6, 0, 5)]);
    // Positive control on the identical input: declared, it applies — so the
    // reject below is the missing declaration and nothing else.
    assert!(parse(&req).is_ok(), "control: the declared request applies");
    req.source_streams.clear();
    let rejection = parse(&req).unwrap_err();
    assert_eq!(rejection, PolicyRejection::UndeclaredCandidate);
    assert_eq!(
        rejection.outcome(),
        PolicyApplyOutcome::RejectedTransportInconsistent
    );
}

/// Boundary positive controls for the reject above: it is the CANDIDATE that
/// is undeclared, not the empty list that is refused. An empty registration
/// (the env-test probe shape) and an egress with zero candidates (a legal edge
/// that forwards nothing) both still apply with empty `source_streams`.
#[test]
fn empty_source_streams_without_candidates_still_applies() {
    let mut req = request("m", 1, vec![]);
    req.source_streams.clear();
    assert!(
        parse(&req).is_ok(),
        "the empty (probe) registration applies"
    );

    let mut stream = egress(1, 6, 0, 5);
    stream.candidate_sources.clear();
    let mut req = request("m", 1, vec![stream]);
    req.source_streams.clear();
    let parsed = parse(&req).unwrap();
    assert_eq!(parsed.edges.len(), 1);
    assert_eq!(parsed.edges[0].transport, None);
}

// -- declared-path apply-time rejects -----------------------------------------

#[test]
fn a_mixed_mode_registration_applies_and_echoes_every_declared_stream() {
    let req = mixed(FWD, PAUSED);
    let parsed = parse(&req).unwrap();
    assert_eq!(parsed.edges[0].transport, Some(DGRAM));
    assert_eq!(parsed.edges[1].transport, Some(SPG));

    let table = RoutingTable::unbounded_for_tests();
    table.install(&parsed);
    let snapshot = table.load();
    let meeting = MeetingKey::new("m");
    let echo: Vec<(u32, u32, i32)> = snapshot
        .applied_source_streams_for(&meeting)
        .iter()
        .map(|e| (e.sender_id, e.stream_number, e.transport_mode))
        .collect();
    assert_eq!(
        echo,
        vec![
            (5, AUDIO, DGRAM as i32),
            (5, VIDEO, SPG as i32),
            (6, AUDIO, DGRAM as i32),
        ],
        "every declared stream, in request order, PAUSED included"
    );
}

#[test]
fn heterogeneous_modes_across_egresses_are_legal() {
    // The retired story-2 reject: one meeting mixes DATAGRAM and
    // STREAM_PER_GROUP egresses.
    assert!(parse(&mixed(FWD, FWD)).is_ok());
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "one table of every reject, deliberately in one place"
)]
fn every_declared_path_reject_carries_its_outcome_token() {
    use PolicyApplyOutcome::{RejectedInvalid, RejectedTransportInconsistent};

    let cases: Vec<(
        &str,
        RegisterMeetingRequest,
        PolicyRejection,
        PolicyApplyOutcome,
    )> = vec![
        (
            "duplicate (sender, stream)",
            {
                let mut r = mixed(FWD, FWD);
                r.source_streams
                    .push(source_stream(5, AUDIO, DGRAM, PAUSED));
                r
            },
            PolicyRejection::DuplicateSourceStream,
            RejectedInvalid,
        ),
        (
            "UNSPECIFIED disposition",
            {
                let mut r = mixed(FWD, FWD);
                r.source_streams[0].disposition = SourceDisposition::Unspecified as i32;
                r
            },
            PolicyRejection::UnspecifiedDisposition,
            RejectedInvalid,
        ),
        (
            "unknown disposition",
            {
                let mut r = mixed(FWD, FWD);
                r.source_streams[0].disposition = 7;
                r
            },
            PolicyRejection::UnspecifiedDisposition,
            RejectedInvalid,
        ),
        (
            "UNSPECIFIED transport",
            {
                let mut r = mixed(FWD, FWD);
                r.source_streams[1].transport_mode = TransportMode::Unspecified as i32;
                r
            },
            PolicyRejection::UnspecifiedSourceTransport,
            RejectedInvalid,
        ),
        (
            "unknown transport",
            {
                let mut r = mixed(FWD, FWD);
                r.source_streams[1].transport_mode = 9_999;
                r
            },
            PolicyRejection::UnspecifiedSourceTransport,
            RejectedInvalid,
        ),
        (
            "source sender 0",
            {
                let mut r = mixed(FWD, FWD);
                r.source_streams[0].sender_id = 0;
                r
            },
            PolicyRejection::Id(IdError::SenderIdZero),
            RejectedInvalid,
        ),
        (
            "source stream above 255",
            {
                let mut r = mixed(FWD, FWD);
                r.source_streams[0].stream_number = 256;
                r
            },
            PolicyRejection::Id(IdError::StreamNumberOutOfRange),
            RejectedInvalid,
        ),
        (
            "undeclared candidate",
            {
                let mut r = mixed(FWD, FWD);
                r.source_streams.remove(1);
                r
            },
            PolicyRejection::UndeclaredCandidate,
            RejectedTransportInconsistent,
        ),
        (
            "candidates of one egress declare different transports",
            {
                let mut r = mixed(FWD, FWD);
                r.egress_streams[0].candidate_sources.push(CandidateSource {
                    sender_id: 5,
                    stream_number: VIDEO,
                });
                r
            },
            PolicyRejection::InconsistentCandidateTransports,
            RejectedTransportInconsistent,
        ),
        (
            // A candidate width failure is rejected_invalid BEFORE the
            // undeclared-candidate check could fire.
            "candidate stream above 255",
            {
                let mut r = mixed(FWD, FWD);
                r.egress_streams[0].candidate_sources[0].stream_number = 256;
                r
            },
            PolicyRejection::Id(IdError::StreamNumberOutOfRange),
            RejectedInvalid,
        ),
    ];
    for (name, req, rejection, outcome) in cases {
        assert_eq!(parse(&req), Err(rejection), "{name}");
        assert_eq!(rejection.outcome(), outcome, "{name}");
    }
}

/// The rule MC-B relies on, already true for non-empty lists: a declaration
/// list that omits a candidate's stream rejects as transport-inconsistent.
#[test]
fn a_non_empty_declaration_missing_every_candidate_rejects_inconsistent() {
    let req = register_request_declared(
        "m",
        1,
        vec![egress_from(1, 6, 0, 5, AUDIO)],
        vec![source_stream(9, AUDIO, DGRAM, FWD)],
    );
    assert_eq!(parse(&req), Err(PolicyRejection::UndeclaredCandidate));
}

#[test]
fn a_zero_candidate_egress_is_legal_and_names_no_transport() {
    let mut stream = egress_from(1, 6, 0, 5, AUDIO);
    stream.candidate_sources.clear();
    let req =
        register_request_declared("m", 1, vec![stream], vec![source_stream(5, 0, DGRAM, FWD)]);
    let parsed = parse(&req).unwrap();
    assert_eq!(parsed.edges[0].transport, None);
}

#[test]
fn the_source_stream_count_bound_fires_before_duplicate_detection() {
    let limits = PolicyLimits {
        max_source_streams_per_meeting: 2,
        ..PolicyLimits::for_tests()
    };
    // Three entries, two of them DUPLICATES and one malformed: the COUNT must
    // be what fires, proving it ran before the dedup `HashSet` was built.
    let mut req = mixed(FWD, FWD);
    req.source_streams[2] = source_stream(5, AUDIO, DGRAM, FWD);
    req.source_streams[1].sender_id = 0;
    assert_eq!(
        MeetingPolicy::from_request(&req, &limits),
        Err(PolicyRejection::TooManySourceStreams)
    );
    assert_eq!(
        PolicyRejection::TooManySourceStreams.outcome(),
        PolicyApplyOutcome::RejectedInvalid
    );
    // At the bound is fine.
    let mut at = mixed(FWD, FWD);
    at.source_streams.truncate(2);
    at.egress_streams.truncate(2);
    assert!(MeetingPolicy::from_request(&at, &limits).is_ok());
}

// -- structural rejects (unchanged from story 2) --------------------------------

#[test]
fn duplicate_egress_stream_id_rejects_whole_registration() {
    let req = request("m", 1, vec![egress(1, 5, 0, 5), egress(1, 6, 1, 6)]);
    assert_eq!(parse(&req), Err(PolicyRejection::DuplicateEgressStreamId));
}

#[test]
fn duplicate_subscriber_slot_rejects_whole_registration() {
    // Same (sender, slot) twice means "two sources into one slot".
    let req = request("m", 1, vec![egress(1, 5, 3, 5), egress(2, 5, 3, 6)]);
    assert_eq!(parse(&req), Err(PolicyRejection::DuplicateSubscriberSlot));
}

#[test]
fn same_slot_id_for_different_subscribers_is_legal() {
    // The reciprocal of the previous test: slot ids are subscriber-scoped,
    // so two subscribers both choosing slot 0 must NOT collide.
    let req = request("m", 1, vec![egress(1, 5, 0, 5), egress(2, 6, 0, 6)]);
    assert!(parse(&req).is_ok());
}

#[test]
fn missing_subscriber_rejects() {
    let mut stream = egress(1, 5, 0, 5);
    stream.subscriber = None;
    let req = request("m", 1, vec![stream]);
    assert_eq!(parse(&req), Err(PolicyRejection::MissingSubscriber));
}

#[test]
fn malformed_sender_id_rejects_on_subscriber_and_on_candidate() {
    let req = request("m", 1, vec![egress(1, 0, 0, 5)]);
    assert_eq!(parse(&req), Err(PolicyRejection::Id(IdError::SenderIdZero)));
    // The candidate side is the ingress-side lookup, where a global index
    // is most tempting — it carries the identical rule.
    let req = request("m", 1, vec![egress(1, 5, 0, 0)]);
    assert_eq!(parse(&req), Err(PolicyRejection::Id(IdError::SenderIdZero)));
}

#[test]
fn over_wide_stream_number_rejects() {
    let mut stream = egress(1, 5, 0, 5);
    stream.candidate_sources[0].stream_number = 256;
    let req = request("m", 1, vec![stream]);
    assert_eq!(
        parse(&req),
        Err(PolicyRejection::Id(IdError::StreamNumberOutOfRange))
    );
}

#[test]
fn count_bounds_reject_before_any_per_element_work() {
    let limits = PolicyLimits {
        max_egress_streams_per_meeting: 1,
        ..PolicyLimits::for_tests()
    };
    // Two streams that ALSO duplicate their egress_stream_id: the count
    // bound must be what fires, proving it ran before duplicate detection
    // allocated a HashSet sized by the repeated field.
    let req = request("m", 1, vec![egress(1, 5, 0, 5), egress(1, 6, 1, 6)]);
    assert_eq!(
        MeetingPolicy::from_request(&req, &limits),
        Err(PolicyRejection::TooManyEgressStreams)
    );

    let limits = PolicyLimits {
        max_candidate_sources_per_egress: 1,
        ..PolicyLimits::for_tests()
    };
    let mut stream = egress(1, 5, 0, 5);
    stream.candidate_sources.push(CandidateSource {
        sender_id: 0, // malformed, but the COUNT must fire first
        stream_number: 0,
    });
    let req = request("m", 1, vec![stream]);
    assert_eq!(
        MeetingPolicy::from_request(&req, &limits),
        Err(PolicyRejection::TooManyCandidateSources)
    );
}

#[test]
fn empty_egress_set_is_legal() {
    let parsed = parse(&request("m", 1, vec![])).unwrap();
    assert_eq!(parsed.edges, Vec::<EgressEdge>::new());
}

#[test]
fn reject_reasons_are_static_and_carry_no_request_data() {
    // A bounded reason string is what keeps identities out of outbound
    // status messages. `reason()` returning `&'static str` makes formatting
    // request data into it impossible by construction; this walks every
    // variant, sourced from `ALL` so the list cannot go short.
    for rejection in PolicyRejection::ALL {
        let reason = rejection.reason();
        assert!(
            !reason.is_empty(),
            "every rejection needs an operator reason"
        );
        // An embedded newline makes the outbound gRPC status multi-line and
        // breaks line-oriented log parsing of the `reason` field.
        assert!(
            !reason.contains('\n'),
            "reject reason must be a single line: {reason:?}"
        );
        // Every rejection maps to a REJECT outcome, never a success value.
        assert!(matches!(
            rejection.outcome(),
            PolicyApplyOutcome::RejectedInvalid | PolicyApplyOutcome::RejectedTransportInconsistent
        ));
    }

    // The reasons must also be DISTINCT, or an operator reading one cannot
    // tell which check refused the registration.
    let reasons: HashSet<&str> = PolicyRejection::ALL.iter().map(|r| r.reason()).collect();
    assert_eq!(
        reasons.len(),
        PolicyRejection::ALL.len(),
        "two rejections share a reason string"
    );

    // And every `IdError` reaches a distinct one through the `From` impl,
    // which is the arm that was silently uncovered before.
    for id_error in IdError::ALL {
        assert!(PolicyRejection::ALL.contains(&PolicyRejection::from(id_error)));
    }
}

// -- snapshot behaviour ---------------------------------------------------------

#[test]
fn install_publishes_generation_and_the_echo() {
    let table = RoutingTable::unbounded_for_tests();
    let meeting = MeetingKey::new("m");
    assert_eq!(table.load().generation_for(&meeting), 0);
    assert_eq!(table.load().applied_source_streams_for(&meeting).len(), 0);

    table.install(&policy("m", 7, vec![egress(1, 5, 0, 5)]));
    assert_eq!(table.load().generation_for(&meeting), 7);
    assert_eq!(table.load().applied_source_streams_for(&meeting).len(), 1);
}

#[test]
fn installing_one_meeting_leaves_another_meetings_routes_untouched() {
    let table = RoutingTable::unbounded_for_tests();
    table.install(&policy("a", 1, vec![egress(1, 5, 0, 5)]));
    let a_before = Arc::clone(table.load().routes_for(&MeetingKey::new("a")).unwrap());

    table.install(&policy("b", 1, vec![egress(1, 7, 0, 7)]));
    let a_after = Arc::clone(table.load().routes_for(&MeetingKey::new("a")).unwrap());

    assert!(
        Arc::ptr_eq(&a_before, &a_after),
        "installing meeting B must carry meeting A's routes over by pointer, not rebuild them"
    );
}

#[test]
fn projected_total_subtracts_the_meetings_own_current_edges() {
    let table = RoutingTable::unbounded_for_tests();
    let three = vec![egress(1, 5, 0, 5), egress(2, 6, 0, 6), egress(3, 7, 0, 7)];
    table.install(&policy("m", 1, three.clone()));
    assert_eq!(table.load().total_edges(), 3);

    // An IDENTICAL re-assert must project to 3, not 6. Without the
    // subtraction, ADR-0036 §8's <=10s re-assert cadence would trip the
    // aggregate bound for every meeting in turn once the handler is half
    // full.
    let reassert = policy("m", 2, three);
    assert_eq!(table.load().projected_total_edges(&reassert), 3);

    // A different meeting genuinely adds.
    let other = policy("other", 1, vec![egress(1, 9, 0, 9)]);
    assert_eq!(table.load().projected_total_edges(&other), 4);
}

#[test]
fn total_edges_tracks_replacement_not_accumulation() {
    let table = RoutingTable::unbounded_for_tests();
    table.install(&policy(
        "m",
        1,
        vec![egress(1, 5, 0, 5), egress(2, 6, 0, 6)],
    ));
    assert_eq!(table.load().total_edges(), 2);
    table.install(&policy("m", 2, vec![egress(1, 5, 0, 5)]));
    assert_eq!(table.load().total_edges(), 1);
    table.install(&policy("m", 3, vec![]));
    assert_eq!(table.load().total_edges(), 0);
    assert_eq!(table.load().meeting_count(), 1);
}

// -- release (story 2 R-20) -----------------------------------------------------

/// Release returns the meeting's edges, leaves the other meeting's routes
/// untouched BY POINTER, and forgets the released meeting's generation.
#[test]
fn remove_returns_edges_forgets_generation_and_spares_other_meetings() {
    let table = RoutingTable::unbounded_for_tests();
    table.install(&policy(
        "a",
        9,
        vec![egress(1, 5, 0, 5), egress(2, 6, 0, 6)],
    ));
    table.install(&policy("b", 3, vec![egress(1, 7, 0, 7)]));
    assert_eq!(table.load().total_edges(), 3);
    let b_before = Arc::clone(table.load().routes_for(&MeetingKey::new("b")).unwrap());

    assert!(table.remove(&MeetingKey::new("a")));

    let after = table.load();
    assert_eq!(after.total_edges(), 1, "a's two edges return to the budget");
    assert_eq!(after.meeting_count(), 1);
    assert_eq!(
        after.generation_for(&MeetingKey::new("a")),
        0,
        "release must FORGET the applied generation, or a same-id re-create \
         at generation 1 reads as stale forever"
    );
    assert!(Arc::ptr_eq(
        &b_before,
        after.routes_for(&MeetingKey::new("b")).unwrap()
    ));
}

/// Per-source-stream state is freed with the meeting's routes: after release
/// nothing about a declared stream resolves. Nothing keyed by sender or stream
/// lives outside `MeetingRoutes`, so this is the whole lifetime.
#[test]
fn per_source_state_is_freed_when_the_meetings_routes_are_removed() {
    let table = install(&mixed(DROP, FWD));
    assert!(!table
        .load()
        .is_server_muted(&MeetingKey::new("m"), sender(5)));
    assert_eq!(
        attribute(&table.load(), "m", 5, 0, DGRAM),
        Seen::ServerMuted
    );
    assert!(table.remove(&MeetingKey::new("m")));
    let after = table.load();
    assert_eq!(attribute(&after, "m", 5, 0, DGRAM), Seen::NoPolicy);
    assert_eq!(
        after
            .applied_source_streams_for(&MeetingKey::new("m"))
            .len(),
        0
    );
}

/// Removing an absent meeting is a no-op WITHOUT a swap, and the accounting
/// kernel stays consistent across install -> remove -> re-install.
#[test]
fn remove_of_an_absent_meeting_does_not_swap_and_reinstall_is_fresh() {
    let table = RoutingTable::unbounded_for_tests();
    table.install(&policy("a", 4, vec![egress(1, 5, 0, 5)]));
    let before = table.load();
    assert!(!table.remove(&MeetingKey::new("never")));
    assert!(Arc::ptr_eq(&before, &table.load()), "no-op must not swap");

    assert!(table.remove(&MeetingKey::new("a")));
    assert_eq!(table.load().total_edges(), 0);
    table.install(&policy("a", 1, vec![egress(1, 5, 0, 5)]));
    assert_eq!(table.load().generation_for(&MeetingKey::new("a")), 1);
    assert_eq!(table.load().total_edges(), 1);
}

// -- sender-level server mute (every declared stream DROP_AT_INGRESS) ----------

#[test]
fn muted_sender_with_no_edge_here_is_held_not_rejected() {
    // Sender 9 sources nothing on this handler and may not be connected: its
    // declared DROP_AT_INGRESS stream is HELD (it stays muted when it
    // attaches).
    let table = RoutingTable::unbounded_for_tests();
    let req = register_request_muted("m", 1, vec![egress(1, 5, 0, 6)], &[9]);
    table.install(&parse(&req).unwrap());
    assert!(table
        .load()
        .is_server_muted(&MeetingKey::new("m"), sender(9)));
}

/// The two-meeting pin for server mute (@security S-2). A handler-global
/// muted set keyed on the wire integer passes every single-meeting test and
/// silences an innocent sender in meeting B.
#[test]
fn sender_muted_in_one_meeting_is_not_muted_in_another() {
    let table = RoutingTable::unbounded_for_tests();
    table.install(
        &parse(&register_request_muted(
            "meeting-a",
            1,
            vec![egress(1, 7, 0, 5)],
            &[5],
        ))
        .unwrap(),
    );
    table.install(&policy("meeting-b", 1, vec![egress(1, 7, 0, 5)]));

    let snapshot = table.load();
    assert!(snapshot.is_server_muted(&MeetingKey::new("meeting-a"), sender(5)));
    assert!(
        !snapshot.is_server_muted(&MeetingKey::new("meeting-b"), sender(5)),
        "sender 5 is muted in meeting A only; muting it in B is cross-tenant"
    );
    // And B's sender 5 still resolves its edge.
    assert_eq!(
        attribute(&snapshot, "meeting-b", 5, 0, DGRAM),
        Seen::Route(false, vec![1])
    );
}

#[test]
fn an_absent_meeting_reads_not_muted() {
    // The documented fail-open arm: safe only because `attribute` then fails
    // closed (`NoPolicy`).
    let table = RoutingTable::unbounded_for_tests();
    assert!(!table
        .load()
        .is_server_muted(&MeetingKey::new("nowhere"), sender(1)));
    assert_eq!(
        attribute(&table.load(), "nowhere", 1, 0, DGRAM),
        Seen::NoPolicy
    );
}

#[test]
fn unmute_is_the_next_generation_without_the_sender() {
    let table = RoutingTable::unbounded_for_tests();
    let meeting = MeetingKey::new("m");
    table.install(
        &parse(&register_request_muted(
            "m",
            1,
            vec![egress(1, 5, 0, 6)],
            &[6],
        ))
        .unwrap(),
    );
    assert!(table.load().is_server_muted(&meeting, sender(6)));
    table.install(&policy("m", 2, vec![egress(1, 5, 0, 6)]));
    assert!(!table.load().is_server_muted(&meeting, sender(6)));
}

#[test]
fn declarations_match_is_order_insensitive_set_equality() {
    let table = install(&mixed(FWD, PAUSED));
    let snapshot = table.load();
    let routes = snapshot.routes_for(&MeetingKey::new("m")).unwrap();

    let mut reordered = mixed(FWD, PAUSED);
    reordered.source_streams.reverse();
    assert!(routes.declarations_match(&parse(&reordered).unwrap().declarations));
    // A disposition toggle is a different declaration.
    assert!(!routes.declarations_match(&parse(&mixed(FWD, FWD)).unwrap().declarations));
    // Declaring nothing is never equal to a declaration.
    assert!(!routes.declarations_match(&policy("m", 1, vec![]).declarations));
    assert_eq!(routes.declared_count(), 3);
    assert_eq!(routes.server_muted_count(), 0);
}

#[test]
fn multiple_candidates_all_index_the_same_edge() {
    let mut stream = egress(1, 5, 0, 5);
    stream.candidate_sources.push(CandidateSource {
        sender_id: 6,
        stream_number: 0,
    });
    let table = RoutingTable::unbounded_for_tests();
    table.install(&policy("m", 1, vec![stream]));
    let snapshot = table.load();
    assert_eq!(
        attribute(&snapshot, "m", 5, 0, DGRAM),
        Seen::Route(false, vec![1])
    );
    assert_eq!(
        attribute(&snapshot, "m", 6, 0, DGRAM),
        Seen::Route(false, vec![1])
    );
    // A sender no candidate names is undeclared.
    assert_eq!(attribute(&snapshot, "m", 8, 0, DGRAM), Seen::Unmatched);
}
