//! Env-tests: the shared-handler edge model on the live Kind cluster (story 2
//! tasks 6 and 20; R-2, R-3, R-4, R-33; ADR-0036 §6, §9), extended to two slot
//! kinds by story 3 MC-A (R-1, R-2, R-3, R-4, R-8, R-22, R-26).
//!
//! Every participant is OFFERED every handler of its meeting
//! (`JoinResponse.media_servers` = the full registered set) and MC routes each
//! pair through a handler BOTH are connected to — where "connected" is what MH
//! observes and reports, never what a client claims. So these mock clients
//! produce partial connectivity the only way a real client can: by which
//! handlers they actually open a WebTransport media session to. There is no MC
//! lever (the Kind MC image is a release build; no test seam exists).
//!
//! The story-2 `#[tokio::test]`s each own ONE meeting, with phase-labelled
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
//! # Story 3 MC-A: two slot kinds
//!
//! Four more fns, each owning one meeting: publish MIXES (with the R-2 /
//! R-22 refusals, each twinned with an admitted join that differs in one
//! field — MC's refusal text is generic, so the twin is what attributes it),
//! video OVER-SUBSCRIPTION (truncation to UNAVAILABLE), PAIRING and stability
//! with the WATCHED-ONLY video directive, and TWO-HANDLER video visibility.
//! Every expected occupant is derived from the R-3 rule text beside its step,
//! and every fill-determining event is converged on before the next (the
//! event-order note above the story-3 section). A LIVE video slot names no
//! sender (R-5), so its occupant is proven by the frames that arrive on it;
//! such a frame also proves MC's registration declared the video stream.
//!
//! # Edge sum (README per-file edge-sum header rule)
//!
//! Six concurrent fns; declared slots per fn: CANONICAL 2+2+2 = 6;
//! ALL-CONNECTED 1+3+2 = 6; MIXES 7 (S) + 1 (R) = 8; OVER-SUBSCRIPTION cap+2
//! = 6 at Kind's cap 4; PAIRING 2+1 = 3; TWO-HANDLER 2+2+2 = 6. Within the
//! planned per-file cap `fixtures::egress_admission::MAX_PER_FILE_EDGE_SUM`,
//! which the Kind egress window is sized against.
//!
//! Edge-sum (concurrent Σ P×(N+M)): 35
//!
//! Registration cost: 6 + 5 (MIXES) + cap+2 (OVER-SUBSCRIPTION) + 3 + 3 = 23
//! AC registrations per run at Kind's cap.

#![cfg(feature = "flows")]

use env_tests::cluster::ClusterConnection;
use env_tests::fixtures::gc_client::GcClient;
use env_tests::fixtures::kube::{
    configmap_key, configmap_name_for_all, configmap_u64, MC_INSTANCES, PROFILE_CONFIGMAP,
};
use env_tests::fixtures::mc_session::{self, JoinDeclaration, McSession};
use env_tests::fixtures::media::{
    assert_relayed_on_slot, bind_until_received, AUDIO_SEND_ID, VIDEO_SEND_ID,
};
use env_tests::fixtures::participant::{
    self, filled_senders, meeting_of, reach, settle_window, slots_of, targets_of,
    two_offered_handlers, Invited, Participant, AUDIO, VIDEO,
};
use env_tests::fixtures::video::{until_video_on_slot, BackgroundPublisher, VideoSubscriber};
use env_tests::fixtures::AuthClient;
use proto_gen::dark_tower::signaling::v1::{
    Codec, ErrorCode, MediaKind, SendDirective, SendStream, SlotState, StreamAssignment,
    StreamAssignments, TransportMode,
};
use std::collections::{BTreeSet, HashMap};
use std::time::Duration;

// The participant vocabulary (`Participant`, `meeting_of`, `reach`, the two
// triage literals `TRIAGE_HANDLER_SET` / `TRIAGE_MH_REACH`) lives in
// `env_tests::fixtures::participant` since story 2 task 10, shared with
// `26_mh_quic.rs`.

/// Margin over MC's settle window (one shared home).
use env_tests::fixtures::participant::VIEW_MARGIN;

async fn cluster() -> ClusterConnection {
    let cluster = ClusterConnection::new()
        .await
        .expect("Failed to connect to cluster - ensure port-forwards are running");
    cluster.check_ac_health().await.expect("AC must be running");
    cluster.check_gc_health().await.expect("GC must be running");
    cluster
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
    ps[0].declare(&[7, 300], &[]).await;
    ps[1].declare(&[513, 3], &[]).await;
    ps[2].declare(&[260, 9], &[]).await;

    let bound = window + VIEW_MARGIN;
    let (a, b, c) = (ids[0], ids[1], ids[2]);
    ps[0]
        .converge("CANONICAL", bound, |v, d| {
            v.is_some_and(|v| {
                let mut f = filled_senders(v, AUDIO);
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
            v.is_some_and(|v| filled_senders(v, AUDIO) == vec![a])
                && d.is_some_and(|d| !targets_of(d).is_empty())
        })
        .await;
    ps[2]
        .converge("CANONICAL", bound, |v, d| {
            v.is_some_and(|v| filled_senders(v, AUDIO) == vec![a])
                && d.is_some_and(|d| !targets_of(d).is_empty())
        })
        .await;

    // Each edge sits on the only handler its pair shares.
    assert_eq!(
        ps[0].slot_url(b, AUDIO),
        x,
        "CANONICAL: A reads B where they share a handler"
    );
    assert_eq!(
        ps[0].slot_url(c, AUDIO),
        y,
        "CANONICAL: A reads C where they share a handler"
    );
    assert_eq!(ps[1].slot_url(a, AUDIO), x);
    assert_eq!(ps[2].slot_url(a, AUDIO), y);

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
        ps[0].slot_for(b, AUDIO),
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
        ps[0].slot_for(c, AUDIO),
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
        ps[1].slot_for(a, AUDIO),
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
        ps[2].slot_for(a, AUDIO),
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
        p.declare(declared[i], &[]).await;
    }
    let bound = window + VIEW_MARGIN;
    let (d, e, f) = (ids[0], ids[1], ids[2]);
    let expect: [Vec<u32>; 3] = [vec![e], vec![d, f], vec![d, e]];
    for (i, p) in ps.iter_mut().enumerate() {
        let want = expect[i].clone();
        p.converge("ALL-CONNECTED", bound, move |v, dir| {
            v.is_some_and(|v| filled_senders(v, AUDIO) == want)
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
        for sender in filled_senders(receiver.view(), AUDIO) {
            let si = ids.iter().position(|id| *id == sender).unwrap();
            let target = targets_of(ps[si].directive.as_ref().unwrap())
                .into_iter()
                .next()
                .unwrap();
            let phase = format!("ALL-CONNECTED {}<-{}", receiver.label, ps[si].label);
            // Every other sender this receiver holds may still have frames in
            // flight from an earlier pair: tolerated, never counted.
            let also_held: Vec<u8> = filled_senders(receiver.view(), AUDIO)
                .iter()
                .filter(|s| **s != sender)
                .filter_map(|s| ids.iter().position(|id| id == s).map(|i| MARK[i]))
                .collect();
            let got = bind_until_received(
                &phase,
                sessions[ri][&receiver.slot_url(sender, AUDIO)].connection(),
                &[(sessions[si][&target].connection(), MARK[si])],
                &also_held,
            )
            .await;
            assert_relayed_on_slot(
                &phase,
                &got[&MARK[si]],
                receiver.slot_for(sender, AUDIO),
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

// ============================================================================
// Story 3 MC-A: two slot kinds (R-1, R-2, R-3, R-4, R-8, R-22, R-26)
// ============================================================================
//
// EVENT ORDER (the timing hazard every scenario below is built around). MC
// fills a slot when a pair becomes ROUTABLE, which is when MH reports both
// connected — not at join. So the occupant a rule picks depends on the ORDER
// pairs become routable, and two joins in flight at once could resolve either
// way. Every scenario therefore advances ONE fill-determining event at a time
// (a join + its media sessions, a leave, a redeclaration) and converges on the
// subscriber's observed view before the next. Every expectation below is
// derived from the R-3 rule text for that order, by hand, beside the step —
// never from MC's output.

/// One expected slot, from the rule text. `Live` is a video ACTIVE slot: MC
/// names no sender (R-5), so its occupant is proven by FRAMES
/// ([`until_video_on_slot`]), never read off the view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Want {
    /// Audio ACTIVE, naming the sender.
    Heard(u32),
    /// Video ACTIVE: no sender, the edge's handler url.
    Live,
    /// AVATAR_NO_VIDEO naming the participant, empty url.
    NoVideo(u32),
    /// FEWER_SOURCES_THAN_SLOTS: no sender, empty url.
    Fewer,
    /// UNAVAILABLE (over the kind's cap): no sender, empty url.
    Unavailable,
}

impl Want {
    fn matches(self, a: &StreamAssignment) -> bool {
        let (state, sender, has_url) = match self {
            Self::Heard(s) => (SlotState::Active, Some(s), true),
            Self::Live => (SlotState::Active, None, true),
            Self::NoVideo(s) => (SlotState::AvatarNoVideo, Some(s), false),
            Self::Fewer => (SlotState::FewerSourcesThanSlots, None, false),
            Self::Unavailable => (SlotState::Unavailable, None, false),
        };
        a.slot_state == state as i32
            && a.sender_id == sender
            && a.media_handler_url.is_empty() != has_url
    }
}

fn view_is(v: &StreamAssignments, kind: MediaKind, want: &[Want]) -> bool {
    let got = slots_of(v, kind);
    got.len() == want.len() && got.iter().zip(want).all(|(a, w)| w.matches(a))
}

/// Read `p`'s signalling until its audio and video slots are exactly `audio`
/// and `video` (rule-text expectations), within `bound`.
async fn converge_slots(
    phase: &str,
    p: &mut Participant,
    bound: Duration,
    audio: &[Want],
    video: &[Want],
) {
    p.converge(phase, bound, |v, _| {
        v.is_some_and(|v| view_is(v, AUDIO, audio) && view_is(v, VIDEO, video))
    })
    .await;
}

/// `d`'s stream of `kind`, if listed.
fn stream_of(d: &SendDirective, kind: MediaKind) -> Option<&SendStream> {
    d.streams.iter().find(|s| s.media_kind == kind as i32)
}

/// `(url, transport)` of `kind`'s targets (empty if unlisted).
fn kind_targets(d: Option<&SendDirective>, kind: MediaKind) -> BTreeSet<(String, i32)> {
    d.and_then(|d| stream_of(d, kind))
        .map(|s| {
            s.targets
                .iter()
                .map(|t| (t.media_handler_url.clone(), t.transport_mode))
                .collect()
        })
        .unwrap_or_default()
}

fn video_target(url: &str) -> (String, i32) {
    (url.to_string(), TransportMode::StreamPerGroup as i32)
}

/// The directive lists EXACTLY the kinds in `publishes` (R-8: every published
/// kind always listed, an unpublished kind never), with the pinned send ids
/// (audio 0, video 1 — the fixtures' `AUDIO_SEND_ID`/`VIDEO_SEND_ID`, which
/// MH attributes by), and a video stream carries the live profile.
fn assert_directive_shape(phase: &str, d: &SendDirective, publishes: &[MediaKind]) {
    let mut kinds: Vec<i32> = d.streams.iter().map(|s| s.media_kind).collect();
    kinds.sort_unstable();
    let mut want: Vec<i32> = publishes.iter().map(|k| *k as i32).collect();
    want.sort_unstable();
    assert_eq!(
        kinds, want,
        "{phase}: the directive lists exactly the publish set"
    );
    for s in &d.streams {
        if s.media_kind == AUDIO as i32 {
            assert_eq!(
                s.stream_number,
                u32::from(AUDIO_SEND_ID),
                "{phase}: audio send id"
            );
            assert!(
                s.targets
                    .iter()
                    .all(|t| t.transport_mode == TransportMode::Datagram as i32),
                "{phase}: audio targets are DATAGRAM"
            );
        } else {
            assert_eq!(
                s.stream_number,
                u32::from(VIDEO_SEND_ID),
                "{phase}: video send id"
            );
            assert!(
                s.targets
                    .iter()
                    .all(|t| t.transport_mode == TransportMode::StreamPerGroup as i32),
                "{phase}: video targets are STREAM_PER_GROUP"
            );
            assert_video_encoding(phase, s);
        }
    }
}

/// Every video encoding field is SET and equals the LIVE configuration MC
/// reads: the shared profile's five video keys and MC's own
/// `MC_VIDEO_MIN_KEYFRAME_INTERVAL_MS` (read from the ConfigMaps the pods run,
/// never a literal; the client refuses a video stream missing any field).
fn assert_video_encoding(phase: &str, s: &SendStream) {
    let profile = configmap_name_for_all(MC_INSTANCES, PROFILE_CONFIGMAP);
    let mc = configmap_name_for_all(MC_INSTANCES, "mc-service-config");
    let n = |cm: &str, k: &str| u32::try_from(configmap_u64(cm, k)).expect("fits u32");
    assert_eq!(
        configmap_key(&profile, "MEDIA_VIDEO_CODEC"),
        "vp8",
        "{phase}: PRECONDITION — story 3 directs VP8 only"
    );
    let e = s
        .encoding
        .unwrap_or_else(|| panic!("{phase}: video stream has no encoding"));
    assert_eq!(e.codec, Codec::Vp8 as i32, "{phase}: codec");
    assert_eq!(e.width, n(&profile, "MEDIA_VIDEO_WIDTH"), "{phase}: width");
    assert_eq!(
        e.height,
        n(&profile, "MEDIA_VIDEO_HEIGHT"),
        "{phase}: height"
    );
    assert_eq!(
        e.frame_rate,
        n(&profile, "MEDIA_VIDEO_FRAME_RATE_HZ"),
        "{phase}: frame rate"
    );
    assert_eq!(
        e.max_bitrate_bps,
        n(&profile, "MEDIA_VIDEO_MAX_BITRATE_BPS"),
        "{phase}: bitrate"
    );
    assert_eq!(
        e.min_keyframe_interval_ms,
        Some(n(&mc, "MC_VIDEO_MIN_KEYFRAME_INTERVAL_MS")),
        "{phase}: the minimum keyframe interval rides the directive from MC's required key"
    );
}

/// A join MC must REFUSE: on a fresh session and a fresh GC token, with the
/// generic join-refusal shape. MC's text is generic, so the refusal is
/// attributed by the caller's POSITIVE TWIN — the same user joining with a
/// declaration that differs in ONE field, admitted next.
async fn assert_refused(phase: &str, invited: &Invited<'_>, i: usize, d: &JoinDeclaration) {
    let gc_join = invited.gc_join(i).await;
    let mut session = McSession::connect(&participant::mc_url(&gc_join)).await;
    let refusal = mc_session::try_mc_join(
        &mut session,
        &gc_join.meeting_id.to_string(),
        &gc_join.token,
        invited.display(i),
        d,
        phase,
    )
    .await
    .err()
    .unwrap_or_else(|| panic!("{phase}: MC ADMITTED a join it must refuse"));
    assert_eq!(
        refusal.code,
        ErrorCode::InvalidRequest as i32,
        "{phase}: generic join refusal"
    );
}

/// Open media sessions to both offered handlers (an ENVIRONMENT failure if
/// either is unreachable), keyed by url.
async fn reach_both(phase: &str, p: &Participant) -> HashMap<String, mc_session::MhSession> {
    let mut out = HashMap::new();
    for url in p.offered() {
        out.insert(url.clone(), reach(phase, p.label, &url, &p.token).await);
    }
    out
}

/// R-1, R-2, R-3, R-4, R-8, R-22: every publish mix in one meeting, each join
/// preceded by the refusal it twins.
///
/// Join order S, PA (audio), PV (video), R (receive-only), PAV (both). S
/// publishes both and declares 3 audio + 4 video slots (within the Kind caps).
/// Expected for S, step by step from R-3 (audio takes audio publishers only;
/// video takes anyone; audio first, then video, one slot at a time):
///
/// | after | S audio            | S video                         |
/// |-------|--------------------|---------------------------------|
/// | PA    | PA, -, -           | NoVideo(PA), -, -, -  (rule 1)  |
/// | PV    | (unchanged)        | NoVideo(PA), Live, -, -         |
/// | R     | (unchanged)        | NoVideo(PA), Live, NoVideo(R), -|
/// | PAV   | PA, PAV, -         | NoVideo(PA), Live, NoVideo(R), Live (rule 1) |
///
/// The two Live slots are proven to hold PV and PAV by their frames.
#[tokio::test]
async fn test_publish_mixes_render_per_kind_and_refusals_are_twinned() {
    const PHASE: &str = "MIXES";
    let cluster = cluster().await;
    let window = settle_window(&cluster).await;
    let bound = window + VIEW_MARGIN;
    let auth = AuthClient::new(&cluster.ac_base_url);
    let gc = GcClient::new(&cluster.gc_base_url);
    let invited = Invited::new(&auth, &gc, "Publish Mixes", 5).await;
    let (av, a_only, v_only) = ([AUDIO, VIDEO], [AUDIO], [VIDEO]);

    let s_audio: [u32; 3] = [11, 300, 12];
    let s_video: [u32; 4] = [513, 21, 22, 700];
    let mut s = invited
        .join(
            0,
            "S",
            &JoinDeclaration::default()
                .publishing(&av)
                .receiving(&s_audio, &s_video),
        )
        .await;
    let cap =
        mc_session::slot_cap(&s.join, MediaKind::VideoCamera).expect("MC sends the video cap");
    assert!(
        mc_session::slot_cap(&s.join, MediaKind::Audio).expect("MC sends the audio cap") >= 3
            && cap >= 4,
        "{PHASE}: PRECONDITION — the scenario declares 3 audio + 4 video slots within the caps"
    );
    let _ = two_offered_handlers(PHASE, std::slice::from_ref(&s));
    let s_mh = reach_both(PHASE, &s).await;
    use Want::{Fewer, Heard, Live, NoVideo};
    converge_slots(PHASE, &mut s, bound, &[Fewer; 3], &[Fewer; 4]).await;
    let echoed: Vec<u32> = s.view().assignments.iter().map(|a| a.slot_id).collect();
    assert_eq!(
        echoed,
        [&s_audio[..], &s_video[..]].concat(),
        "{PHASE}: the declared ids are echoed, audio then video"
    );

    // PA. Twin: a v2-only client (R-22) is refused; the same join on v3 is not.
    let v2_only = JoinDeclaration::default().header_versions(&[2]);
    assert_refused("MIXES v2-only", &invited, 1, &v2_only).await;
    let mut pa = invited.join(1, "PA", &JoinDeclaration::default()).await;
    let _pa_mh = reach_both(PHASE, &pa).await;
    let pa_id = pa.sender_id();
    converge_slots(
        "MIXES after PA",
        &mut s,
        bound,
        &[Heard(pa_id), Fewer, Fewer],
        &[NoVideo(pa_id), Fewer, Fewer, Fewer],
    )
    .await;

    // PV. Twin: a screen-share publish kind is rejected, never ignored (R-2).
    let screen = JoinDeclaration::default().publishing(&[MediaKind::VideoScreen]);
    assert_refused("MIXES screen", &invited, 2, &screen).await;
    let mut pv = invited
        .join(2, "PV", &JoinDeclaration::default().publishing(&v_only))
        .await;
    let pv_mh = reach_both(PHASE, &pv).await;
    converge_slots(
        "MIXES after PV",
        &mut s,
        bound,
        &[Heard(pa_id), Fewer, Fewer],
        &[NoVideo(pa_id), Live, Fewer, Fewer],
    )
    .await;

    // R. Twin: publishing nothing AND receiving nothing is refused (R-2); the
    // same receive-only join with one video slot is admitted.
    let neither = JoinDeclaration::default().publishing(&[]);
    assert_refused("MIXES neither", &invited, 3, &neither).await;
    let mut r = invited
        .join(
            3,
            "R",
            &JoinDeclaration::default()
                .publishing(&[])
                .receiving(&[], &[31]),
        )
        .await;
    let _r_mh = reach_both(PHASE, &r).await;
    let r_id = r.sender_id();
    converge_slots(
        "MIXES after R",
        &mut s,
        bound,
        &[Heard(pa_id), Fewer, Fewer],
        &[NoVideo(pa_id), Live, NoVideo(r_id), Fewer],
    )
    .await;
    // R's own video slot: the earliest eligible joiner is S, who publishes
    // video, so it is Live (proven below by S's frames).
    converge_slots("MIXES R's view", &mut r, bound, &[], &[Live]).await;

    // PAV: audio first (PAV is the only unheld audio publisher), then video by
    // rule (1) — PAV now holds audio with S but no video.
    let mut pav = invited
        .join(4, "PAV", &JoinDeclaration::default().publishing(&av))
        .await;
    let pav_mh = reach_both(PHASE, &pav).await;
    let pav_id = pav.sender_id();
    converge_slots(
        "MIXES after PAV",
        &mut s,
        bound,
        &[Heard(pa_id), Heard(pav_id), Fewer],
        &[NoVideo(pa_id), Live, NoVideo(r_id), Live],
    )
    .await;

    // DIRECTIVES (R-8). Each publisher's directive lists exactly its publish
    // set; a stream's targets are the handlers of its watchers' edges.
    let s_url = |slot: u32| -> String {
        s.view()
            .assignments
            .iter()
            .find(|a| a.slot_id == slot)
            .map(|a| a.media_handler_url.clone())
            .expect("declared slot")
    };
    let (pv_url, pav_url) = (s_url(s_video[1]), s_url(s_video[3]));
    let r_url = r.slots(VIDEO)[0].media_handler_url.clone();
    pv.converge("MIXES PV directive", bound, |_, d| {
        kind_targets(d, VIDEO) == BTreeSet::from([video_target(&pv_url)])
    })
    .await;
    assert_directive_shape("MIXES PV", pv.directive.as_ref().unwrap(), &v_only);
    pav.converge("MIXES PAV directive", bound, |_, d| {
        kind_targets(d, VIDEO) == BTreeSet::from([video_target(&pav_url)])
            && !kind_targets(d, AUDIO).is_empty()
    })
    .await;
    assert_directive_shape("MIXES PAV", pav.directive.as_ref().unwrap(), &av);
    pa.converge("MIXES PA directive", bound, |_, d| {
        !kind_targets(d, AUDIO).is_empty()
    })
    .await;
    assert_directive_shape("MIXES PA", pa.directive.as_ref().unwrap(), &a_only);
    // S's AUDIO is unwatched (nobody declared an audio slot that could hold
    // it) yet still LISTED, with empty targets; its video is watched by R.
    s.converge("MIXES S directive", bound, |_, d| {
        d.is_some_and(|d| stream_of(d, AUDIO).is_some())
            && kind_targets(d, AUDIO).is_empty()
            && kind_targets(d, VIDEO) == BTreeSet::from([video_target(&r_url)])
    })
    .await;
    assert_directive_shape("MIXES S", s.directive.as_ref().unwrap(), &av);
    // R publishes nothing: whatever directive it holds lists no stream.
    assert!(
        r.directive.as_ref().is_none_or(|d| d.streams.is_empty()),
        "{PHASE}: a receive-only participant is directed to send nothing"
    );

    // FRAMES: each Live slot holds the publisher the table names.
    let mut s_subs: HashMap<String, VideoSubscriber> = s_mh
        .iter()
        .map(|(u, m)| (u.clone(), VideoSubscriber::spawn(m.connection().clone())))
        .collect();
    for (who, mh, id, url, slot) in [
        ("PV", &pv_mh, pv.sender_id(), &pv_url, s_video[1]),
        ("PAV", &pav_mh, pav_id, &pav_url, s_video[3]),
    ] {
        let publisher = BackgroundPublisher::start(mh[url].connection().clone(), id);
        let phase = format!("MIXES S<-{who} video");
        let sub = s_subs.get_mut(url).expect("S reached every handler");
        until_video_on_slot(&phase, "S", sub, slot, id, FRAME_DEADLINE).await;
        publisher.stop().await;
    }
    drop((s_mh, pv_mh, pav_mh));
}

/// Failure bound for a directed video frame to arrive (group cadence + edge
/// install + network); a rig bound, never a pass condition.
const FRAME_DEADLINE: Duration = Duration::from_secs(30);

/// R-1: a video declaration past the cap is TRUNCATED, never refused. S
/// declares cap + 2 video slots (non-ordinal ids); cap + 1 audio-only peers join
/// one at a time. From R-3 rule (2), in join order, each peer takes S's next
/// video slot as NoVideo (named — which is what makes the occupants readable
/// off the wire); the last peer holds no slot and is NOT unreachable; the last
/// two declared slots are UNAVAILABLE from the first view on, ids echoed.
#[tokio::test]
async fn test_video_over_subscription_truncates_to_unavailable() {
    const PHASE: &str = "OVER-SUBSCRIPTION";
    let cluster = cluster().await;
    let window = settle_window(&cluster).await;
    let bound = window + VIEW_MARGIN;
    let auth = AuthClient::new(&cluster.ac_base_url);
    let gc = GcClient::new(&cluster.gc_base_url);

    // The meeting is sized to the cap MC RUNS: read from the live ConfigMap
    // (02 proves the process loaded it) and cross-checked against S's own
    // JoinResponse below. Kind's is 4.
    let cap = usize::try_from(configmap_u64(
        &configmap_name_for_all(MC_INSTANCES, "mc-service-config"),
        "MC_MAX_VIDEO_RECEIVE_SLOTS",
    ))
    .unwrap();
    assert!(
        (1..=8).contains(&cap),
        "{PHASE}: PRECONDITION — this scenario sizes itself to the video cap and is meant for \
         Kind's small one; got {cap}"
    );

    let invited = Invited::new(&auth, &gc, "Video Over-subscription", cap + 2).await;
    let declared: Vec<u32> = (0..cap + 2)
        .map(|i| 900 - 97 * u32::try_from(i).unwrap())
        .collect();
    let mut s = invited
        .join(
            0,
            "S",
            &JoinDeclaration::default().receiving(&[], &declared),
        )
        .await;
    assert_eq!(
        mc_session::slot_cap(&s.join, MediaKind::VideoCamera),
        Some(u32::try_from(cap).unwrap()),
        "{PHASE}: the JoinResponse video cap is the configured one"
    );
    let _s_mh = reach_both(PHASE, &s).await;
    let mut want: Vec<Want> = (0..cap + 2)
        .map(|i| {
            if i < cap {
                Want::Fewer
            } else {
                Want::Unavailable
            }
        })
        .collect();
    converge_slots(PHASE, &mut s, bound, &[], &want).await;
    let echoed: Vec<u32> = s.view().assignments.iter().map(|a| a.slot_id).collect();
    assert_eq!(
        echoed, declared,
        "{PHASE}: every declared id is echoed in order"
    );

    const LABELS: [&str; 9] = ["P1", "P2", "P3", "P4", "P5", "P6", "P7", "P8", "P9"];
    let mut peers = Vec::new();
    for k in 0..=cap {
        let p = invited
            .join(k + 1, LABELS[k], &JoinDeclaration::default())
            .await;
        let mh = reach_both(PHASE, &p).await;
        if k < cap {
            want[k] = Want::NoVideo(p.sender_id());
        }
        converge_slots(
            &format!("{PHASE} after {}", LABELS[k]),
            &mut s,
            bound,
            &[],
            &want,
        )
        .await;
        peers.push((p, mh));
    }
    let surplus = peers[cap].0.sender_id();
    assert!(
        !s.view().unreachable_sender_ids.contains(&surplus),
        "{PHASE}: the surplus peer is 'no slot', never 'unreachable'"
    );
}

/// R-3 stability and pairing, and R-8 watched-only video directives.
///
/// S (audio publisher; 1 audio + 1 video slot), R (receive-only; 1 video
/// slot), P (both; no slots), joined in that order, then:
///
/// 1. after R: S video = NoVideo(R); R video = NoVideo(S) (S publishes no video).
/// 2. after P: S audio = P; S video STAYS R — stability beats pairing. P's
///    video is held by nobody, so it is LISTED with EMPTY targets.
/// 3. R leaves: S's video refills by rule (1) to P (holds audio, no video) —
///    Live, proven by P's frames — and P's video gains S's handler.
/// 4. S redeclares video to 0: P's video is withdrawn (empty targets), S's
///    audio keeps P by ordinal. 5. back to 1: rule (1) restores P.
/// 6. S leaves: P's last watcher — both streams listed, both empty.
#[tokio::test]
async fn test_pairing_stability_and_watched_only_video_directive() {
    const PHASE: &str = "PAIRING";
    let cluster = cluster().await;
    let window = settle_window(&cluster).await;
    let bound = window + VIEW_MARGIN;
    let auth = AuthClient::new(&cluster.ac_base_url);
    let gc = GcClient::new(&cluster.gc_base_url);
    let invited = Invited::new(&auth, &gc, "Pairing Stability", 3).await;
    use Want::{Fewer, Heard, Live, NoVideo};

    let (s_a, s_v) = (5_u32, 9_u32);
    let mut s = invited
        .join(
            0,
            "S",
            &JoinDeclaration::default().receiving(&[s_a], &[s_v]),
        )
        .await;
    let s_mh = reach_both(PHASE, &s).await;
    let s_id = s.sender_id();
    converge_slots(PHASE, &mut s, bound, &[Fewer], &[Fewer]).await;

    // 1.
    let mut r = invited
        .join(
            1,
            "R",
            &JoinDeclaration::default()
                .publishing(&[])
                .receiving(&[], &[8]),
        )
        .await;
    let r_mh = reach_both(PHASE, &r).await;
    let r_id = r.sender_id();
    converge_slots("PAIRING 1 S", &mut s, bound, &[Fewer], &[NoVideo(r_id)]).await;
    converge_slots("PAIRING 1 R", &mut r, bound, &[], &[NoVideo(s_id)]).await;

    // 2.
    let both = [AUDIO, VIDEO];
    let mut p = invited
        .join(2, "P", &JoinDeclaration::default().publishing(&both))
        .await;
    let p_mh = reach_both(PHASE, &p).await;
    let p_id = p.sender_id();
    converge_slots(
        "PAIRING 2 stability",
        &mut s,
        bound,
        &[Heard(p_id)],
        &[NoVideo(r_id)],
    )
    .await;
    p.converge("PAIRING 2 P unwatched video", bound, |_, d| {
        d.is_some_and(|d| stream_of(d, VIDEO).is_some())
            && kind_targets(d, VIDEO).is_empty()
            && !kind_targets(d, AUDIO).is_empty()
    })
    .await;
    assert_directive_shape("PAIRING 2", p.directive.as_ref().unwrap(), &both);

    // 3.
    r.session
        .connection()
        .close(wtransport::VarInt::from_u32(0), b"leave");
    drop((r, r_mh));
    converge_slots("PAIRING 3 refill", &mut s, bound, &[Heard(p_id)], &[Live]).await;
    let s_video_url = s.slots(VIDEO)[0].media_handler_url.clone();
    p.converge("PAIRING 3 P watched", bound, |_, d| {
        kind_targets(d, VIDEO) == BTreeSet::from([video_target(&s_video_url)])
    })
    .await;
    assert_directive_shape("PAIRING 3", p.directive.as_ref().unwrap(), &both);
    let publisher = BackgroundPublisher::start(p_mh[&s_video_url].connection().clone(), p_id);
    let mut sub = VideoSubscriber::spawn(s_mh[&s_video_url].connection().clone());
    until_video_on_slot("PAIRING 3 S<-P", "S", &mut sub, s_v, p_id, FRAME_DEADLINE).await;
    publisher.stop().await;

    // 4.
    s.declare(&[s_a], &[]).await;
    converge_slots("PAIRING 4 S", &mut s, bound, &[Heard(p_id)], &[]).await;
    p.converge("PAIRING 4 withdrawn", bound, |_, d| {
        d.is_some_and(|d| stream_of(d, VIDEO).is_some()) && kind_targets(d, VIDEO).is_empty()
    })
    .await;

    // 5.
    s.declare(&[s_a], &[s_v]).await;
    converge_slots("PAIRING 5 S", &mut s, bound, &[Heard(p_id)], &[Live]).await;
    let restored_url = s.slots(VIDEO)[0].media_handler_url.clone();
    p.converge("PAIRING 5 restored", bound, |_, d| {
        kind_targets(d, VIDEO) == BTreeSet::from([video_target(&restored_url)])
    })
    .await;

    // 6.
    s.session
        .connection()
        .close(wtransport::VarInt::from_u32(0), b"leave");
    drop((s, s_mh, sub));
    p.converge("PAIRING 6 last watcher left", bound, |_, d| {
        d.is_some_and(|d| stream_of(d, VIDEO).is_some() && stream_of(d, AUDIO).is_some())
            && kind_targets(d, VIDEO).is_empty()
            && kind_targets(d, AUDIO).is_empty()
    })
    .await;
}

/// R-3 (story 2 R-33 for video): the canonical split. A (both kinds) on both
/// handlers, B on one, C on the other; each declares 2 video slots. A's video
/// targets BOTH handlers; B and C each hold A Live on the only handler they
/// share with A (proven by frames), carry each other as unreachable, and hold
/// no slot for each other. A holds B and C as NoVideo (in either order: see
/// the settle-window note at the assertion).
#[tokio::test]
async fn test_two_handler_video_visibility() {
    const PHASE: &str = "TWO-HANDLER VIDEO";
    let cluster = cluster().await;
    let window = settle_window(&cluster).await;
    let bound = window + VIEW_MARGIN;
    let auth = AuthClient::new(&cluster.ac_base_url);
    let gc = GcClient::new(&cluster.gc_base_url);
    let invited = Invited::new(&auth, &gc, "Two Handler Video", 3).await;
    use Want::{Fewer, Live, NoVideo};
    let both = [AUDIO, VIDEO];

    let mut a = invited
        .join(
            0,
            "A",
            &JoinDeclaration::default()
                .publishing(&both)
                .receiving(&[], &[1, 2]),
        )
        .await;
    let mut b = invited
        .join(1, "B", &JoinDeclaration::default().receiving(&[], &[3, 4]))
        .await;
    let mut c = invited
        .join(2, "C", &JoinDeclaration::default().receiving(&[], &[5, 6]))
        .await;
    let (x, y) = {
        let trio = [a, b, c];
        let xy = two_offered_handlers(PHASE, &trio);
        [a, b, c] = trio;
        xy
    };
    let a_x = reach(PHASE, "A", &x, &a.token).await;
    let a_y = reach(PHASE, "A", &y, &a.token).await;
    let b_x = reach(PHASE, "B", &x, &b.token).await;
    let c_y = reach(PHASE, "C", &y, &c.token).await;
    let (a_id, b_id, c_id) = (a.sender_id(), b.sender_id(), c.sender_id());

    // B and C are each routed only after MC's settle window (neither reaches
    // every handler), so which fills A's first video slot is a timing fact,
    // not a rule: assert the SET.
    a.converge(PHASE, bound, |v, _| {
        v.is_some_and(|v| {
            view_is(v, VIDEO, &[NoVideo(b_id), NoVideo(c_id)])
                || view_is(v, VIDEO, &[NoVideo(c_id), NoVideo(b_id)])
        })
    })
    .await;
    for (p, other) in [(&mut b, c_id), (&mut c, b_id)] {
        p.converge(PHASE, bound, |v, _| {
            v.is_some_and(|v| {
                view_is(v, VIDEO, &[Live, Fewer]) && v.unreachable_sender_ids == vec![other]
            })
        })
        .await;
    }
    assert_eq!(b.slots(VIDEO)[0].media_handler_url, x, "{PHASE}: B<-A on x");
    assert_eq!(c.slots(VIDEO)[0].media_handler_url, y, "{PHASE}: C<-A on y");
    a.converge(PHASE, bound, |_, d| {
        kind_targets(d, VIDEO) == BTreeSet::from([video_target(&x), video_target(&y)])
    })
    .await;
    assert_directive_shape(PHASE, a.directive.as_ref().unwrap(), &both);

    let mut b_sub = VideoSubscriber::spawn(b_x.connection().clone());
    let mut c_sub = VideoSubscriber::spawn(c_y.connection().clone());
    let on_x = BackgroundPublisher::start(a_x.connection().clone(), a_id);
    let on_y = BackgroundPublisher::start(a_y.connection().clone(), a_id);
    until_video_on_slot("TWO-HANDLER B<-A", "B", &mut b_sub, 3, a_id, FRAME_DEADLINE).await;
    until_video_on_slot("TWO-HANDLER C<-A", "C", &mut c_sub, 5, a_id, FRAME_DEADLINE).await;
    on_x.stop().await;
    on_y.stop().await;
}
