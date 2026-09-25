//! Env-tests: the shared-handler edge model on the live Kind cluster (story 2
//! tasks 6 and 20; R-2, R-3, R-4, R-33; ADR-0036 §6, §9).
//!
//! Every participant is OFFERED every handler of its meeting
//! (`JoinResponse.media_servers` = the full registered set) and MC routes each
//! pair through a handler BOTH are connected to — where "connected" is what MH
//! observes and reports, never what a client claims. So these mock clients
//! produce partial connectivity the only way a real client can: by which
//! handlers they actually open a WebTransport media session to. There is no MC
//! lever (the Kind MC image is a release build; no test seam exists).
//!
//! Two `#[tokio::test]`s, each owning ONE meeting, with phase-labelled
//! failures. Separate tests sharing a meeting would race on the signalling
//! streams.
//!
//! # CANONICAL — A on both handlers, B on one, C on the other
//!
//! Which Kind handler is "one" and which "the other" is picked arbitrarily from
//! the OBSERVED set; nothing depends on which handler is which. Ordering is
//! deliberate (@operations OPS-7, @test): a broken environment — a NodePort
//! down, a network-policy regression, an unhealthy mh-1 — produces exactly the
//! connected-set shape this phase asserts, so:
//!
//! 1. PRECONDITION: every client is offered exactly two distinct handlers
//!    (`TRIAGE_HANDLER_SET` otherwise).
//! 2. REACH: every mock opens every session it is told to; a failure is an
//!    ENVIRONMENT hard-fail (`TRIAGE_MH_REACH`), never a degrade into the
//!    partial case.
//! 3. POSITIVE CONTROLS: A hears B on B's handler and C on C's handler, and B
//!    and C each hear A — real frames, each arriving on the transport of the
//!    handler that owns that edge (read off the RECEIVER's own
//!    `StreamAssignments`, never a literal). This proves both handlers live and
//!    forwarding.
//! 4. ONLY THEN: B and C carry each other in `unreachable_sender_ids`, A's send
//!    directive targets both handlers, B's and C's target only their own.
//!
//! # ALL-CONNECTED — everyone on both handlers
//!
//! Everyone hears everyone (a real frame for EVERY ordered pair, so an empty
//! unreachable set cannot pass vacuously on total silence), and every sender
//! has exactly ONE send target (co-location — not which one). Also carries the
//! R-2 over-subscription checks (one slot: the earliest joiner; a surplus sender
//! is "no slot", never "unreachable"; undeclared-for slots say
//! `FEWER_SOURCES_THAN_SLOTS` with `sender_id` absent) and the relay-region
//! checks (stream id rewritten to the receiver's declared slot, hop sequence
//! MH's own, publisher region / payload / signature byte-identical — MH is
//! keyless).
//!
//! Declared slot ids are non-ordinal and several are > 255 (slot ids are u16),
//! so a stamped-ordinal or u8-truncation defect fails the echo and relay checks.
//!
//! # Timing
//!
//! A participant that never reaches every handler (B, C) is routed only after
//! MC's connect settle window. The wait is DERIVED from the window MC publishes
//! (`mc_media_connect_settle_window_seconds`, the enforced value), never a
//! literal, so a ConfigMap change cannot erode the margin. No sleeps gate any
//! assertion: views are read until a predicate holds, frames are sent until
//! received.
//!
//! Cross-handler NON-delivery is not asserted with a timed negative: it is proven
//! by MC's wire (the unreachable sets and per-handler edges) plus MH's
//! component-tier S10c.
//!
//! Registration cost: six AC registrations per run.

#![cfg(feature = "flows")]

use env_tests::cluster::ClusterConnection;
use env_tests::fixtures::auth_client::UserRegistrationRequest;
use env_tests::fixtures::gc_client::{CreateMeetingRequest, GcClient, JoinMeetingResponse};
use env_tests::fixtures::mc_session::{self, McSession};
use env_tests::fixtures::metrics::{gauge_by_instance_present, PrometheusClient};
use env_tests::fixtures::AuthClient;
use proto_gen::dark_tower::signaling::v1::{
    client_message, server_message, ClientMessage, JoinResponse, MediaKind, ReceiveCapability,
    ReceiveSlot, SendDirective, SlotState, StreamAssignments,
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

/// Greppable triage literal for a mock client that could not open a media
/// session to a handler it was TOLD to use: an environment fact (NodePort,
/// network policy, cert, MH health), which would otherwise present as the very
/// partial-connectivity shape the canonical phase asserts. Catalogued beside
/// [`TRIAGE_HANDLER_SET`] in `docs/runbooks/devloop-validation.md` §8.
const TRIAGE_MH_REACH: &str = "Triage MH reachability";

/// Margin added to MC's published settle window before a view must have
/// converged: covers MH's connect notification, the push and the flush.
const VIEW_MARGIN: Duration = Duration::from_secs(10);

/// How long to wait for MC's settle-window gauge to be scraped.
const GAUGE_SCRAPE_BOUND: Duration = Duration::from_secs(60);

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
/// in `env_tests::fixtures::mc_session`; this wraps the shared session with the
/// suite's own vocabulary and the latest view it has seen.
struct Participant {
    label: &'static str,
    session: McSession,
    join: JoinResponse,
    token: String,
    directive: Option<SendDirective>,
    view: Option<StreamAssignments>,
}

impl Participant {
    fn sender_id(&self) -> u32 {
        self.join.sender_id.expect("MC allocates a sender id")
    }

    /// The handler urls this participant was OFFERED, sorted.
    fn offered(&self) -> Vec<String> {
        let mut urls: Vec<String> = self
            .join
            .media_servers
            .iter()
            .map(|m| m.media_handler_url.clone())
            .collect();
        urls.sort();
        urls
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

    /// Read signalling until the latest view AND directive satisfy `done`
    /// (full-replace semantics: the latest message of each kind is current),
    /// within `bound`. Distinct failures for "nothing arrived" and "arrived but
    /// never matched".
    async fn converge(
        &mut self,
        phase: &str,
        bound: Duration,
        done: impl Fn(Option<&StreamAssignments>, Option<&SendDirective>) -> bool,
    ) {
        let deadline = Instant::now() + bound;
        while !done(self.view.as_ref(), self.directive.as_ref()) {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(
                !remaining.is_zero(),
                "{phase}: {}'s view did not converge within {bound:?}; last view {:?}, last \
                 directive targets {:?}. Before reading this as an MC routing defect, rule out \
                 SHARED CAPACITY on the handler carrying these edges — MH refuses a registration \
                 WHOLE when the projected installed streams exceed its derived ceiling, and \
                 suites 26/28 share these pods: check \
                 mh_media_policy_applies_total{{outcome=\"rejected_stream_ceiling\"}} and \
                 mh_media_egress_edges against mh_media_egress_stream_ceiling on both MH pods \
                 ({TRIAGE_HANDLER_SET}).",
                self.label,
                self.view
                    .as_ref()
                    .map(|v| (filled_senders(v), &v.unreachable_sender_ids)),
                self.directive.as_ref().map(targets_of)
            );
            match self
                .session
                .try_read(remaining)
                .await
                .and_then(|m| m.message)
            {
                Some(server_message::Message::StreamAssignments(a)) => self.view = Some(a),
                Some(server_message::Message::SendDirective(d)) => self.directive = Some(d),
                Some(server_message::Message::Error(e)) => {
                    panic!(
                        "{phase}: MC rejected {}'s declaration: {}",
                        self.label, e.message
                    )
                }
                // Roster traffic. Nothing is logged about it: it carries names.
                _ => {}
            }
        }
    }

    fn view(&self) -> &StreamAssignments {
        self.view.as_ref().expect("converged view")
    }

    /// The handler url owning this participant's edge from `sender`.
    fn slot_url(&self, sender: u32) -> String {
        self.view()
            .assignments
            .iter()
            .find(|a| a.sender_id == Some(sender))
            .unwrap_or_else(|| panic!("{} does not hold sender {sender}", self.label))
            .media_handler_url
            .clone()
    }

    /// The slot id this participant declared for its edge from `sender`.
    fn slot_for(&self, sender: u32) -> u32 {
        self.view()
            .assignments
            .iter()
            .find(|a| a.sender_id == Some(sender))
            .unwrap_or_else(|| panic!("{} does not hold sender {sender}", self.label))
            .slot_id
    }
}

fn targets_of(d: &SendDirective) -> BTreeSet<String> {
    d.streams
        .iter()
        .flat_map(|s| s.targets.iter().map(|t| t.media_handler_url.clone()))
        .collect()
}

/// Open a media session to `url`, or fail as an ENVIRONMENT fact.
async fn reach(phase: &str, who: &str, url: &str, token: &str) -> mc_session::MhSession {
    mc_session::try_mh_connect(url, token)
        .await
        .unwrap_or_else(|e| {
            panic!(
                "{TRIAGE_MH_REACH}: {phase} — ENVIRONMENT, not an MC routing defect: {who} could \
                 not open a media session to {url}, a handler it was told to use: {e}. Check the \
                 MH NodePorts, network policy and pod health (kubectl -n dark-tower get pods -l \
                 app=mh-service)."
            )
        })
}

/// MC's enforced connect settle window, read from the gauge it publishes.
async fn settle_window(cluster: &ClusterConnection) -> Duration {
    let prom = PrometheusClient::new(&cluster.prometheus_base_url);
    let by_instance = gauge_by_instance_present(
        &prom,
        "mc_media_connect_settle_window_seconds",
        1,
        GAUGE_SCRAPE_BOUND,
    )
    .await;
    let seconds = by_instance.values().copied().fold(0.0_f64, f64::max);
    assert!(
        seconds > 0.0,
        "mc_media_connect_settle_window_seconds must be positive; got {by_instance:?}"
    );
    Duration::from_secs_f64(seconds)
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
        directive: None,
        view: None,
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
///
/// `also_held` names markers of senders this receiver legitimately holds on the
/// same transport but that this call is not waiting for: frames from an earlier
/// phase can still be in flight, so they are tolerated and ignored — never
/// counted toward completion. A frame bearing ANY other marker is a sender the
/// receiver must not hear on this transport (e.g. a peer it shares no handler
/// with), and fails at once: a positive observation of a leak, never a timed
/// negative.
async fn bind_until_received(
    phase: &str,
    receiver: &wtransport::Connection,
    senders: &[(&wtransport::Connection, u8)],
    also_held: &[u8],
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
                    if senders.iter().any(|(_, m)| *m == marker) {
                        seen.entry(marker).or_insert(payload.clone());
                    } else {
                        assert!(
                            also_held.contains(&marker),
                            "{phase}: the receiver got a frame from sender {marker:#04x}, which it \
                             must not hear on this transport"
                        );
                    }
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

/// Register `labels.len()` users, create a meeting as the first, and join all
/// sequentially (each JoinResponse awaited, so join order is label order).
async fn meeting_of(
    auth: &AuthClient,
    gc: &GcClient,
    title: &str,
    labels: &[&'static str],
) -> Vec<Participant> {
    let mut users = Vec::new();
    for label in labels {
        let request = UserRegistrationRequest::unique(format!("{title} {label}"));
        let display = request.display_name.clone();
        let registered = auth
            .register_user(&request)
            .await
            .expect("AC should register a test user");
        users.push((registered.access_token, display));
    }
    let created = gc
        .create_meeting(&users[0].0, &CreateMeetingRequest::new(title))
        .await
        .expect("GC should create the meeting");
    let mut ps = Vec::new();
    for (i, label) in labels.iter().enumerate() {
        let gc_join = gc
            .join_meeting(&created.meeting_code, &users[i].0)
            .await
            .expect("GC should issue a meeting token");
        ps.push(mc_join(label, &gc_join, &users[i].1).await);
    }
    ps
}

/// PRECONDITION: every participant is offered the SAME two distinct handlers.
fn two_offered_handlers(phase: &str, ps: &[Participant]) -> (String, String) {
    let offered = ps[0].offered();
    assert_eq!(
        offered.len(),
        2,
        "{TRIAGE_HANDLER_SET}: {phase} PRECONDITION — ENVIRONMENT, not an MC defect: this \
         meeting's handler set has {} handler(s); the scenario needs both Kind MHs healthy and \
         registered (kubectl -n dark-tower get pods -l app=mh-service)",
        offered.len()
    );
    for p in ps {
        assert_eq!(
            p.offered(),
            offered,
            "{phase}: every participant is offered the FULL registered set (ADR-0036 §9)"
        );
    }
    (offered[0].clone(), offered[1].clone())
}

const MARK: [u8; 3] = [0xA0, 0xB0, 0xC0];

#[tokio::test]
async fn test_canonical_partial_connectivity_across_two_handlers() {
    let cluster = cluster().await;
    let window = settle_window(&cluster).await;
    let auth = AuthClient::new(&cluster.ac_base_url);
    let gc = GcClient::new(&cluster.gc_base_url);
    let mut ps = meeting_of(&auth, &gc, "Edge Model Canonical", &["A", "B", "C"]).await;

    // 1. PRECONDITION. `x` and `y` are an ARBITRARY labelling of the two.
    let (x, y) = two_offered_handlers("CANONICAL", &ps);
    let ids: Vec<u32> = ps.iter().map(Participant::sender_id).collect();

    // 2. REACH: A opens both; B only x; C only y.
    let a_x = reach("CANONICAL", "A", &x, &ps[0].token).await;
    let a_y = reach("CANONICAL", "A", &y, &ps[0].token).await;
    let b_x = reach("CANONICAL", "B", &x, &ps[1].token).await;
    let c_y = reach("CANONICAL", "C", &y, &ps[2].token).await;

    // Non-ordinal ids, several > 255.
    ps[0].declare(&[7, 300]).await;
    ps[1].declare(&[513, 3]).await;
    ps[2].declare(&[260, 9]).await;

    let bound = window + VIEW_MARGIN;
    let (a, b, c) = (ids[0], ids[1], ids[2]);
    ps[0]
        .converge("CANONICAL", bound, |v, d| {
            v.is_some_and(|v| {
                let mut f = filled_senders(v);
                f.sort_unstable();
                f == {
                    let mut w = vec![b, c];
                    w.sort_unstable();
                    w
                }
            }) && d.is_some_and(|d| targets_of(d).len() == 2)
        })
        .await;
    ps[1]
        .converge("CANONICAL", bound, |v, d| {
            v.is_some_and(|v| filled_senders(v) == vec![a])
                && d.is_some_and(|d| !targets_of(d).is_empty())
        })
        .await;
    ps[2]
        .converge("CANONICAL", bound, |v, d| {
            v.is_some_and(|v| filled_senders(v) == vec![a])
                && d.is_some_and(|d| !targets_of(d).is_empty())
        })
        .await;

    // Each edge sits on the only handler its pair shares.
    assert_eq!(
        ps[0].slot_url(b),
        x,
        "CANONICAL: A reads B where they share a handler"
    );
    assert_eq!(
        ps[0].slot_url(c),
        y,
        "CANONICAL: A reads C where they share a handler"
    );
    assert_eq!(ps[1].slot_url(a), x);
    assert_eq!(ps[2].slot_url(a), y);

    // 3. POSITIVE CONTROLS — real frames on the owning transports.
    let a_from_b = bind_until_received(
        "CANONICAL A<-B",
        a_x.connection(),
        &[(b_x.connection(), MARK[1])],
        &[],
    )
    .await;
    assert_relayed_on_slot(
        "CANONICAL A<-B",
        &a_from_b[&MARK[1]],
        ps[0].slot_for(b),
        MARK[1],
        "B",
    );
    let a_from_c = bind_until_received(
        "CANONICAL A<-C",
        a_y.connection(),
        &[(c_y.connection(), MARK[2])],
        &[],
    )
    .await;
    assert_relayed_on_slot(
        "CANONICAL A<-C",
        &a_from_c[&MARK[2]],
        ps[0].slot_for(c),
        MARK[2],
        "C",
    );
    let b_from_a = bind_until_received(
        "CANONICAL B<-A",
        b_x.connection(),
        &[(a_x.connection(), MARK[0])],
        &[],
    )
    .await;
    assert_relayed_on_slot(
        "CANONICAL B<-A",
        &b_from_a[&MARK[0]],
        ps[1].slot_for(a),
        MARK[0],
        "A",
    );
    let c_from_a = bind_until_received(
        "CANONICAL C<-A",
        c_y.connection(),
        &[(a_y.connection(), MARK[0])],
        &[],
    )
    .await;
    assert_relayed_on_slot(
        "CANONICAL C<-A",
        &c_from_a[&MARK[0]],
        ps[2].slot_for(a),
        MARK[0],
        "A",
    );

    // 4. ONLY NOW the unreachable and send-target assertions. B and C settle at
    //    different instants (each at its own first connect + the window), so a
    //    view that converged on "hears A" can predate the peer's settle. Read on
    //    until the view carries the settled unreachable set, bounded by the
    //    window — the assertions below then check the converged state.
    let (b_id, c_id) = (b, c);
    ps[1]
        .converge("CANONICAL", bound, |v, _| {
            v.is_some_and(|v| v.unreachable_sender_ids == vec![c_id])
        })
        .await;
    ps[2]
        .converge("CANONICAL", bound, |v, _| {
            v.is_some_and(|v| v.unreachable_sender_ids == vec![b_id])
        })
        .await;
    assert!(
        ps[0].view().unreachable_sender_ids.is_empty(),
        "CANONICAL: A reaches everyone"
    );
    assert_eq!(
        ps[1].view().unreachable_sender_ids,
        vec![c],
        "CANONICAL: C is unreachable to B"
    );
    assert_eq!(
        ps[2].view().unreachable_sender_ids,
        vec![b],
        "CANONICAL: B is unreachable to C"
    );
    assert_eq!(
        targets_of(ps[0].directive.as_ref().unwrap()),
        BTreeSet::from([x.clone(), y.clone()]),
        "CANONICAL: A sends to both handlers"
    );
    assert_eq!(
        targets_of(ps[1].directive.as_ref().unwrap()),
        BTreeSet::from([x.clone()])
    );
    assert_eq!(
        targets_of(ps[2].directive.as_ref().unwrap()),
        BTreeSet::from([y.clone()])
    );
    // Each unfilled slot names no sender and carries the explicit state.
    for p in &ps[1..] {
        let unfilled = &p.view().assignments[1];
        assert!(unfilled.sender_id.is_none());
        assert_eq!(unfilled.slot_state, SlotState::FewerSourcesThanSlots as i32);
        assert!(unfilled.media_handler_url.is_empty());
    }
}

#[tokio::test]
async fn test_all_connected_everyone_hears_everyone_with_one_target_each() {
    let cluster = cluster().await;
    let window = settle_window(&cluster).await;
    let auth = AuthClient::new(&cluster.ac_base_url);
    let gc = GcClient::new(&cluster.gc_base_url);
    let mut ps = meeting_of(&auth, &gc, "Edge Model All Connected", &["D", "E", "F"]).await;
    let (x, y) = two_offered_handlers("ALL-CONNECTED", &ps);
    let ids: Vec<u32> = ps.iter().map(Participant::sender_id).collect();

    // Everyone reaches both handlers. sessions[i] = (url -> session).
    let mut sessions: Vec<HashMap<String, mc_session::MhSession>> = Vec::new();
    for p in &ps {
        let mut mine = HashMap::new();
        for url in [&x, &y] {
            mine.insert(
                url.clone(),
                reach("ALL-CONNECTED", p.label, url, &p.token).await,
            );
        }
        sessions.push(mine);
    }

    // D: ONE slot (over-subscription: holds the earliest, E; F is "no slot").
    // E: three slots with two peers (one FEWER_SOURCES). F: two slots.
    let declared: [&[u32]; 3] = [&[513], &[7, 300, 700], &[260, 3]];
    for (i, p) in ps.iter_mut().enumerate() {
        p.declare(declared[i]).await;
    }
    let bound = window + VIEW_MARGIN;
    let (d, e, f) = (ids[0], ids[1], ids[2]);
    let expect: [Vec<u32>; 3] = [vec![e], vec![d, f], vec![d, e]];
    for (i, p) in ps.iter_mut().enumerate() {
        let want = expect[i].clone();
        p.converge("ALL-CONNECTED", bound, move |v, dir| {
            v.is_some_and(|v| filled_senders(v) == want)
                && dir.is_some_and(|dir| targets_of(dir).len() == 1)
        })
        .await;
    }

    // ECHO: one assignment per declared slot, in order, echoing the declarer's
    // own ids.
    for (i, p) in ps.iter().enumerate() {
        let got: Vec<u32> = p.view().assignments.iter().map(|a| a.slot_id).collect();
        assert_eq!(&got[..], declared[i], "ECHO: {}", p.label);
    }

    // POSITIVE CONTROL for EVERY held pair: the sender transmits on its one
    // target; the receiver reads on the transport its slot names.
    for (ri, receiver) in ps.iter().enumerate() {
        for sender in filled_senders(receiver.view()) {
            let si = ids.iter().position(|id| *id == sender).unwrap();
            let target = targets_of(ps[si].directive.as_ref().unwrap())
                .into_iter()
                .next()
                .unwrap();
            let phase = format!("ALL-CONNECTED {}<-{}", receiver.label, ps[si].label);
            // Every other sender this receiver holds may still have frames in
            // flight from an earlier pair: tolerated, never counted.
            let also_held: Vec<u8> = filled_senders(receiver.view())
                .iter()
                .filter(|s| **s != sender)
                .filter_map(|s| ids.iter().position(|id| id == s).map(|i| MARK[i]))
                .collect();
            let got = bind_until_received(
                &phase,
                sessions[ri][&receiver.slot_url(sender)].connection(),
                &[(sessions[si][&target].connection(), MARK[si])],
                &also_held,
            )
            .await;
            assert_relayed_on_slot(
                &phase,
                &got[&MARK[si]],
                receiver.slot_for(sender),
                MARK[si],
                ps[si].label,
            );
        }
    }

    // Only now: nobody unreachable, one target per sender.
    for p in &ps {
        assert!(
            p.view().unreachable_sender_ids.is_empty(),
            "ALL-CONNECTED: {} reaches all",
            p.label
        );
        assert_eq!(
            targets_of(p.directive.as_ref().unwrap()).len(),
            1,
            "ALL-CONNECTED: one target"
        );
    }
    // OVER-SUBSCRIPTION: F is "no slot" for D, never "unreachable"; E's third
    // slot says so explicitly.
    assert!(!ps[0].view().unreachable_sender_ids.contains(&f));
    let e_states: Vec<i32> = ps[1]
        .view()
        .assignments
        .iter()
        .map(|a| a.slot_state)
        .collect();
    assert_eq!(
        e_states,
        vec![
            SlotState::Active as i32,
            SlotState::Active as i32,
            SlotState::FewerSourcesThanSlots as i32
        ]
    );
    assert!(ps[1].view().assignments[2].sender_id.is_none());
}
