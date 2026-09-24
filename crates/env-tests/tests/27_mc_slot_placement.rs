//! Env-tests: multi-party static join-order slot placement on the live Kind
//! cluster (story 2 task 6; R-1, R-2, R-3, R-4, R-33; ADR-0036 §6, §9).
//!
//! ONE `#[tokio::test]` owns ONE five-participant meeting, and every failure
//! message names its phase (PRECONDITION / S10b / BINDING / BINDING-H1 /
//! OVER-SUBSCRIPTION). Separate test functions sharing a meeting would race on
//! the signalling streams.
//!
//! # The meeting
//!
//! Five DISTINCT registered users (MH resolves a media connection's sender by
//! the token `sub`, so one user joining twice would be ambiguous and rejected)
//! join sequentially — each `JoinResponse` is awaited before the next join, so
//! join-order ranks are 0..4. MC places participants round-robin over the
//! SORTED handler set: ranks 0, 2, 4 (A, C, E) on one handler and ranks 1, 3
//! (B, D) on the other. The test never assumes which handler is which: it
//! OBSERVES placement from each client's scoped `media_servers`.
//!
//! Registration cost: five AC registrations per run.
//!
//! # PRECONDITION — two handlers, or an ENVIRONMENT failure
//!
//! GC's set selection is weighted-random and returns both Kind handlers only
//! when both are healthy. The test HARD-FAILS — never skips, never falls back
//! to a single-handler assertion — unless exactly two distinct handler urls
//! appear across the five participants, with a message that says ENVIRONMENT,
//! so a one-healthy-MH cluster is not triaged as an MC placement bug. The
//! `test-seams` placement pin cannot help here: the Kind MC image is a release
//! build, and the seam is compiled out of every release build.
//!
//! # Declared slot ids are non-ordinal by design
//!
//! Every participant declares NON-ordinal, non-sorted slot ids, several above
//! 255 (slot ids are u16). A defect that stamped a slot's ORDINAL, or truncated
//! its id to u8, would be invisible under `[0, 1, 2, …]` — here it fails the
//! echo assertion (every view's assignments must echo the declarer's own ids,
//! one per slot, in declaration order) and, at MH, the BINDING `stream_id`
//! checks (B holds D on a slot > 255).
//!
//! # What is proven, and what is proven elsewhere
//!
//! - **S10b** — each subscriber's filled slots are co-handler senders only; its
//!   `unreachable_sender_ids` is EXACTLY the cross-handler set (set equality);
//!   the roster carries all five; `media_servers` has length 1 and equals the
//!   handler url on the subscriber's own active slots.
//! - **BINDING at MH, both handlers** — two arms share one send-until-received
//!   loop (bounded by `BINDING_DEADLINE`; any arrival reflects a
//!   post-declaration policy, since a subscriber's edges exist only after it
//!   declares — never a Prometheus gate, never a sleep). Each arm reads the
//!   expected `stream_id` off the RECEIVER's own `StreamAssignments` (never a
//!   literal) and checks the relay rewrote only its relay region — hop sequence
//!   changed; signed publisher region, payload and signature byte-identical, MH
//!   being keyless:
//!     - **mh-0 (`BINDING`)** — A declares two slots; its co-handler peers C and
//!       E send marked datagrams; A receives each on the distinct slot A was
//!       told, and BOTH must arrive (positive control).
//!     - **mh-1 (`BINDING-H1`)** — B and D sit on the OTHER handler and B holds
//!       D. url(0) alone would never touch mh-1, so this second arm is what
//!       proves mh-1 was programmed at MH, not just mh-0: D sends into B, which
//!       receives on the slot B's own view named for D (a value > 255).
//! - **OVER-SUBSCRIPTION** — C declares one slot and gets A (earliest); E is
//!   unassigned for C and is NOT in C's unreachable set ("no slot" is not
//!   "unreachable"). B declares three slots with one co-handler peer: one
//!   `ACTIVE`, two `FEWER_SOURCES_THAN_SLOTS` with `sender_id` absent.
//!
//! Cross-handler NON-delivery is not asserted with a timed negative (a
//! wall-clock flake): it is proven by MC's wire (the S10b set equality — no
//! cross-handler edge is ever pushed) plus MH's component-tier S10c (task 10).
//! Per-meeting MH metrics do not exist (ADR-0036 §11), so no handler's edge set
//! is read from Prometheus.

#![cfg(feature = "flows")]

use env_tests::cluster::ClusterConnection;
use env_tests::fixtures::auth_client::UserRegistrationRequest;
use env_tests::fixtures::gc_client::{CreateMeetingRequest, GcClient, JoinMeetingResponse};
use env_tests::fixtures::mc_session::{self, McSession};
use env_tests::fixtures::AuthClient;
use proto_gen::dark_tower::signaling::v1::{
    client_message, server_message, ClientMessage, JoinResponse, MediaKind, ReceiveCapability,
    ReceiveSlot, SlotState, StreamAssignments,
};
use std::collections::{BTreeSet, HashMap};
use std::time::{Duration, Instant};

/// Greppable triage literal for the ONE failure in this suite that is an
/// environment fact rather than a diff defect: fewer than two distinct handlers
/// were programmed for the meeting, so the split scenario has nothing to
/// observe. Layer 7 reports every non-zero env-test as `STATUS=FAIL
/// REASON=env-tests-failed` (implementer lane, attempt consumed) and
/// deliberately does not grep suite output, so the only thing that can put a
/// triager on the operator lane is a stable literal they can grep for —
/// the same discipline as `32_media_metric_hygiene.rs`'s `TRIAGE_SCRAPE`.
/// Catalogued in `docs/runbooks/devloop-validation.md` §8 (pointer to §6.7).
const TRIAGE_HANDLER_SET: &str = "Triage MH handler-set health";

/// Bound on "no more media signalling is coming" while settling a view.
const QUIET: Duration = Duration::from_secs(3);

/// Bound on the whole binding phase's send-until-received loop. A rig
/// precondition for unreliable QUIC datagrams, not a performance assertion.
const BINDING_DEADLINE: Duration = Duration::from_secs(30);

async fn cluster() -> ClusterConnection {
    let cluster = ClusterConnection::new()
        .await
        .expect("Failed to connect to cluster - ensure port-forwards are running");
    cluster.check_ac_health().await.expect("AC must be running");
    cluster.check_gc_health().await.expect("GC must be running");
    cluster
}

/// One participant's live MC signalling session. Connection/framing/join live
/// in `env_tests::fixtures::mc_session` (shared with `26_mh_quic.rs`); this
/// wraps the shared session with the slot-placement suite's own vocabulary
/// (`declare`, `settled_view`) and the join-time state it asserts on.
struct Participant {
    label: &'static str,
    session: McSession,
    join: JoinResponse,
    token: String,
}

impl Participant {
    fn sender_id(&self) -> u32 {
        self.join.sender_id.expect("MC allocates a sender id")
    }

    async fn declare(&mut self, audio_slot_ids: &[u32]) {
        self.session
            .write(&ClientMessage {
                message: Some(client_message::Message::ReceiveCapability(
                    ReceiveCapability {
                        slots: audio_slot_ids
                            .iter()
                            .map(|id| ReceiveSlot {
                                slot_id: *id,
                                media_kind: MediaKind::Audio as i32,
                                pinned_sender_id: None,
                            })
                            .collect(),
                    },
                )),
                trace_parent: String::new(),
                trace_state: String::new(),
            })
            .await;
    }

    /// Drain until quiet; the LAST `StreamAssignments` is the view (MC sends
    /// full-replace snapshots). Distinct failure for "none at all".
    async fn settled_view(&mut self, phase: &str) -> StreamAssignments {
        let mut last = None;
        while let Some(m) = self.session.try_read(QUIET).await {
            match m.message {
                Some(server_message::Message::StreamAssignments(a)) => last = Some(a),
                Some(server_message::Message::Error(e)) => {
                    panic!(
                        "{phase}: MC rejected {}'s declaration: {}",
                        self.label, e.message
                    )
                }
                // Directives and roster traffic. Nothing is logged about them:
                // roster payloads carry display names.
                _ => {}
            }
        }
        last.unwrap_or_else(|| {
            panic!(
                "{phase}: {} received no StreamAssignments at all",
                self.label
            )
        })
    }
}

async fn mc_join(label: &'static str, gc: &JoinMeetingResponse, display_name: &str) -> Participant {
    let mc_url = gc
        .mc_assignment
        .webtransport_endpoint
        .clone()
        .expect("MC assignment must include webtransport_endpoint");
    let mut session = McSession::connect(&mc_url).await;
    let join = mc_session::mc_join(
        &mut session,
        &gc.meeting_id.to_string(),
        &gc.token,
        display_name,
        label,
    )
    .await;
    Participant {
        label,
        session,
        join,
        token: gc.token.clone(),
    }
}

/// A frame-v2 audio datagram through the production codec, with publisher-side
/// relay values a correct relay must rewrite and a payload marker naming the
/// sender (MH is keyless: payload and signature must survive byte-identically).
///
/// ANCHOR (DRY): this is a deliberate TWIN of
/// `crates/mh-service/tests/common/media_frame.rs` (the mh-service forward-path
/// suites' frame builder). env-tests does NOT share it by import: that file is a
/// SERVICE crate's test-support module, and ADR-0028 layering forbids this
/// black-box cluster suite from depending on a service's test utilities — the
/// same rule `sample_identity_public_key` in `fixtures/media.rs` records for its
/// own twin. Both are built through `media_protocol::codec::encode_frame` (a
/// wire-format crate, not a service crate), so neither hand-rolls the wire
/// layout; two homes is the accepted cost, kept equal in shape, not by import.
fn audio_datagram(stream_sequence: u32, marker: u8) -> bytes::Bytes {
    use media_protocol::codec::{encode_frame, MediaFrameParts};
    use media_protocol::frame::{FrameFlags, SIGNATURE_SIZE};
    let payload = vec![marker; 160];
    let mut signature = [0_u8; SIGNATURE_SIZE];
    for (i, b) in signature.iter_mut().enumerate() {
        *b = u8::try_from(i % 251).unwrap_or(0);
    }
    encode_frame(&MediaFrameParts {
        flags: FrameFlags {
            independently_decodable: true,
            discardable: false,
            key_bearing: false,
        },
        stream_sequence,
        wrapped_transmit_key: None,
        extensions: &[],
        stream_id: 0xFFFF,
        hop_sequence: 0xDEAD_BEEF,
        payload: &payload,
        signature: &signature,
    })
    .expect("fixture frame must encode")
}

fn filled_senders(view: &StreamAssignments) -> Vec<u32> {
    view.assignments
        .iter()
        .filter_map(|a| a.sender_id)
        .collect()
}

/// Send marked datagrams from every sender each round until `receiver` has
/// received one bearing each sender's marker; return the raw received datagram
/// per marker. Shared by both binding arms, so mh-0 and mh-1 are proven with the
/// SAME loop. Bounded by [`BINDING_DEADLINE`]; `phase` names the failure.
///
/// Ordering is send-until-received under the deadline: any arrival reflects a
/// post-declaration policy (the receiver's edges exist only after it declares) —
/// never a sleep, never a Prometheus gate.
async fn bind_until_received(
    phase: &str,
    receiver: &wtransport::Connection,
    senders: &[(&wtransport::Connection, u8)],
) -> HashMap<u8, bytes::Bytes> {
    let mut seen: HashMap<u8, bytes::Bytes> = HashMap::new();
    let deadline = Instant::now() + BINDING_DEADLINE;
    let mut seq = 0u32;
    while seen.len() < senders.len() {
        assert!(
            Instant::now() < deadline,
            "{phase}: within {BINDING_DEADLINE:?} the receiver did not receive from every sender \
             (markers seen: {:?} of {} expected). Check mh_media_frames_dropped_total{{reason}} \
             (no_subscriber = MC pushed no edge; no_local_subscriber = receiver not connected; \
             no_media_session = binding not yet made)",
            seen.keys().copied().collect::<BTreeSet<u8>>(),
            senders.len(),
        );
        seq += 1;
        for (conn, marker) in senders {
            // The send error is NOT discarded. A dropped `Err` here makes the
            // deadline message above ambiguous between "MH forwarded nothing"
            // and "the client never got a frame onto the wire" (datagram too
            // large for the negotiated size, unsupported by the peer, or the
            // session already closed) — and it would send triage to MH's drop
            // counters for a fault on this side of the link.
            conn.send_datagram(audio_datagram(seq, *marker))
                .unwrap_or_else(|e| {
                    panic!("{phase}: sender {marker:#04x} could not send its datagram: {e}")
                });
        }
        match tokio::time::timeout(Duration::from_millis(250), receiver.receive_datagram()).await {
            // Nothing this round: the expected case while the policy is still
            // being installed. Keep sending until the deadline.
            Err(_elapsed) => {}
            // The receiver's SESSION is gone — MH closed or reset it. Terminal
            // now rather than at the deadline: waiting cannot un-close a
            // connection, and reporting it as "did not receive from every
            // sender" would name the wrong subject.
            Ok(Err(e)) => panic!(
                "{phase}: the receiver's MH connection ended while waiting for forwarded \
                 datagrams: {e}. Check MH's logs for this session — a close here is MH \
                 refusing or dropping the connection, not a forwarding gap"
            ),
            Ok(Ok(d)) => {
                let payload = d.payload();
                let view = media_protocol::codec::decode_datagram(&payload)
                    .unwrap_or_else(|_| panic!("{phase}: receiver got a malformed datagram"));
                if let Some(&marker) = view.payload().first() {
                    seen.entry(marker).or_insert(payload.clone());
                }
            }
        }
    }
    seen
}

/// Assert one received datagram `raw` was relayed onto `expected_slot` (read off
/// the RECEIVER's own `StreamAssignments`, never a literal), with the relay
/// region rewritten and the signed publisher region, payload and signature
/// relayed byte-identically (MH is keyless). `marker`/`who` name the sender.
fn assert_relayed_on_slot(
    phase: &str,
    raw: &bytes::Bytes,
    expected_slot: u32,
    marker: u8,
    who: &str,
) {
    let view = media_protocol::codec::decode_datagram(raw).expect("received datagram decodes");
    assert_eq!(
        u32::from(view.stream_id()),
        expected_slot,
        "{phase}: {who}'s frames must arrive on the slot the receiver's own StreamAssignments \
         named for {who}"
    );
    assert_ne!(
        view.hop_sequence(),
        0xDEAD_BEEF,
        "{phase}: MH writes its own hop sequence"
    );
    // Re-encode exactly what was sent for this sequence number.
    let sent_bytes = audio_datagram(view.stream_sequence(), marker);
    let sent = media_protocol::codec::decode_datagram(&sent_bytes).expect("fixture decodes");
    assert_eq!(
        view.publisher_region(),
        sent.publisher_region(),
        "{phase}: the signed publisher region is relayed byte-identically"
    );
    assert_eq!(
        view.payload(),
        sent.payload(),
        "{phase}: the payload is opaque to MH"
    );
    assert_eq!(
        view.signature(),
        sent.signature(),
        "{phase}: the signature is untouched"
    );
}

#[tokio::test]
async fn test_multi_party_slot_placement_across_two_handlers() {
    let cluster = cluster().await;
    let auth = AuthClient::new(&cluster.ac_base_url);
    let gc = GcClient::new(&cluster.gc_base_url);

    // ---- five distinct users; A creates the meeting; each joins via GC ----
    let labels: [&'static str; 5] = ["A", "B", "C", "D", "E"];
    let mut users = Vec::new();
    for label in labels {
        let request = UserRegistrationRequest::unique(format!("Slot Placement {label}"));
        let display = request.display_name.clone();
        let registered = auth
            .register_user(&request)
            .await
            .expect("AC should register a test user");
        users.push((registered.access_token, display));
    }
    let created = gc
        .create_meeting(
            &users[0].0,
            &CreateMeetingRequest::new("Slot Placement Meeting"),
        )
        .await
        .expect("GC should create the meeting");

    // Sequential joins: each JoinResponse is awaited before the next, so the
    // ranks are 0..4 in label order.
    let mut ps = Vec::new();
    for (i, label) in labels.iter().enumerate() {
        let gc_join = gc
            .join_meeting(&created.meeting_code, &users[i].0)
            .await
            .expect("GC should issue a meeting token");
        ps.push(mc_join(label, &gc_join, &users[i].1).await);
    }

    // ---- PRECONDITION: exactly two handlers, observed, never assumed ----
    for p in &ps {
        assert_eq!(
            p.join.media_servers.len(),
            1,
            "PRECONDITION: {}'s media_servers must be scoped to exactly its placed handler",
            p.label
        );
    }
    let urls: Vec<String> = ps
        .iter()
        .map(|p| p.join.media_servers[0].media_handler_url.clone())
        .collect();
    let url = |i: usize| urls[i].clone();
    let distinct: BTreeSet<String> = (0..5).map(url).collect();
    assert_eq!(
        distinct.len(),
        2,
        "{TRIAGE_HANDLER_SET}: PRECONDITION — ENVIRONMENT, not an MC placement defect: GC \
         returned {} distinct handler(s) for this meeting; the split scenario needs both Kind \
         MHs healthy and registered (kubectl -n dark-tower get pods -l app=mh-service)",
        distinct.len()
    );
    assert!(
        url(0) == url(2) && url(2) == url(4) && url(1) == url(3) && url(0) != url(1),
        "S10b: round-robin by join rank must place A,C,E together and B,D together"
    );
    let h0 = [0usize, 2, 4];
    let h1 = [1usize, 3];
    let ids: Vec<u32> = ps.iter().map(Participant::sender_id).collect();

    // Roster carries everyone: E (last) was told about all four others.
    assert_eq!(
        ps[4].join.existing_participants.len(),
        4,
        "S10b: the roster carries every participant, across handlers"
    );

    // ---- declarations: non-ordinal, non-sorted ids, several > 255 (slot ids
    // are u16), so a stamped-ordinal or u8-truncation defect is catchable ----
    let declared: [&[u32]; 5] = [
        &[7, 3],        // A: two slots, hears C and E (co-handler peers)
        &[700, 3, 300], // B: three slots, one co-handler peer (D); 700, 300 > 255
        &[513],         // C: one slot, two co-handler peers (A, E); 513 > 255
        &[77],          // D: one slot
        &[260],         // E: one slot; 260 > 255
    ];
    for (i, p) in ps.iter_mut().enumerate() {
        p.declare(declared[i]).await;
    }

    let mut views = HashMap::new();
    for p in &mut ps {
        let v = p.settled_view("S10b").await;
        views.insert(p.label, v);
    }

    // ---- ECHO: every view carries one assignment per declared slot, in
    // declaration order, echoing the declarer's OWN slot ids (never renumbered
    // to ordinals). This is what makes the non-ordinal ids load-bearing. ----
    for (i, label) in labels.iter().enumerate() {
        let got: Vec<u32> = views[label].assignments.iter().map(|a| a.slot_id).collect();
        assert_eq!(
            &got[..],
            declared[i],
            "ECHO: {label}'s assignments must echo its declared slot ids, one per slot, in \
             declaration order"
        );
    }

    // ---- S10b: per-handler edge sets, exact unreachable sets ----
    for (mine, other) in [(&h0[..], &h1[..]), (&h1[..], &h0[..])] {
        for &i in mine {
            let view = &views[ps[i].label];
            let mut unreachable: Vec<u32> = view.unreachable_sender_ids.clone();
            unreachable.sort_unstable();
            let mut expected: Vec<u32> = other.iter().map(|&j| ids[j]).collect();
            expected.sort_unstable();
            assert_eq!(
                unreachable, expected,
                "S10b: {}'s unreachable set must be EXACTLY the other handler's participants",
                ps[i].label
            );
            for s in filled_senders(view) {
                assert!(
                    mine.iter().any(|&j| ids[j] == s),
                    "S10b: {} holds a sender from the other handler",
                    ps[i].label
                );
                assert_ne!(s, ids[i], "S10b: {} holds itself (R-3)", ps[i].label);
            }
            for a in view
                .assignments
                .iter()
                .filter(|a| a.slot_state == SlotState::Active as i32)
            {
                assert_eq!(
                    a.media_handler_url,
                    url(i),
                    "S10b: {}'s slot url, send url and media_servers are one handler",
                    ps[i].label
                );
            }
        }
    }

    // ---- OVER-SUBSCRIPTION (assignments are one-per-declared-slot in
    // declaration order, verified by ECHO above, so positions ARE slots) ----
    let c_view = &views["C"];
    assert_eq!(
        filled_senders(c_view),
        vec![ids[0]],
        "OVER-SUBSCRIPTION: C's one slot holds A, the earliest"
    );
    assert!(
        !c_view.unreachable_sender_ids.contains(&ids[4]),
        "OVER-SUBSCRIPTION: E is 'no slot' for C, never 'unreachable'"
    );
    let b_view = &views["B"];
    let states: Vec<i32> = b_view.assignments.iter().map(|a| a.slot_state).collect();
    assert_eq!(
        states,
        vec![
            SlotState::Active as i32,
            SlotState::FewerSourcesThanSlots as i32,
            SlotState::FewerSourcesThanSlots as i32
        ],
        "OVER-SUBSCRIPTION: B's first declared slot fills with its one peer; the rest say so"
    );
    assert_eq!(b_view.assignments[0].sender_id, Some(ids[3]));
    for a in &b_view.assignments[1..] {
        assert!(
            a.sender_id.is_none(),
            "OVER-SUBSCRIPTION: an unfilled slot names no sender (never 0)"
        );
        assert!(a.media_handler_url.is_empty());
    }

    // ---- BINDING at mh-0: two co-handler senders into A, each on the slot A
    // was told, read off A's own view ----
    let a_view = &views["A"];
    let slot_for = |sender: u32| -> u32 {
        a_view
            .assignments
            .iter()
            .find(|a| a.sender_id == Some(sender))
            .unwrap_or_else(|| panic!("BINDING: A's view does not hold sender {sender}"))
            .slot_id
    };
    let (slot_c, slot_e) = (slot_for(ids[2]), slot_for(ids[4]));
    assert_ne!(slot_c, slot_e, "BINDING: two senders, two distinct slots");

    let mh0 = url(0);
    let a_mh = mc_session::mh_connect(&mh0, &ps[0].token).await;
    let c_mh = mc_session::mh_connect(&mh0, &ps[2].token).await;
    let e_mh = mc_session::mh_connect(&mh0, &ps[4].token).await;
    const MARK_C: u8 = 0xC0;
    const MARK_E: u8 = 0xE0;
    let seen = bind_until_received(
        "BINDING",
        a_mh.connection(),
        &[(c_mh.connection(), MARK_C), (e_mh.connection(), MARK_E)],
    )
    .await;
    assert_relayed_on_slot("BINDING", &seen[&MARK_C], slot_c, MARK_C, "C");
    assert_relayed_on_slot("BINDING", &seen[&MARK_E], slot_e, MARK_E, "E");

    // ---- BINDING at mh-1: D into B, proving mh-1 was programmed at MH too.
    // B and D sit on the OTHER handler; url(0) alone never touches mh-1. B holds
    // D on a slot > 255, so this arm also catches a u8 truncation at MH. ----
    let b_view = &views["B"];
    let slot_d = b_view
        .assignments
        .iter()
        .find(|a| a.sender_id == Some(ids[3]))
        .expect("BINDING-H1: B's view must hold D")
        .slot_id;
    // Stated, not assumed: the u8-truncation coverage this arm claims exists
    // only if D's slot is really > 255. A change to B's declaration or to the
    // fill rule must fail here, not silently turn the arm into a <256 check.
    assert!(
        slot_d > 255,
        "BINDING-H1: B must hold D on a slot > 255 (got {slot_d}) for this arm to catch a u8 \
         truncation at MH"
    );
    let mh1 = url(1);
    let b_mh = mc_session::mh_connect(&mh1, &ps[1].token).await;
    let d_mh = mc_session::mh_connect(&mh1, &ps[3].token).await;
    const MARK_D: u8 = 0xD0;
    let seen_h1 = bind_until_received(
        "BINDING-H1",
        b_mh.connection(),
        &[(d_mh.connection(), MARK_D)],
    )
    .await;
    assert_relayed_on_slot("BINDING-H1", &seen_h1[&MARK_D], slot_d, MARK_D, "D");
}
