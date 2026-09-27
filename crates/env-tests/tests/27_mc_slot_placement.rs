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
use env_tests::fixtures::gc_client::GcClient;
use env_tests::fixtures::mc_session;
use env_tests::fixtures::media::{assert_relayed_on_slot, bind_until_received};
use env_tests::fixtures::participant::{
    filled_senders, meeting_of, reach, settle_window, targets_of, two_offered_handlers, Participant,
};
use env_tests::fixtures::AuthClient;
use proto_gen::dark_tower::signaling::v1::SlotState;
use std::collections::{BTreeSet, HashMap};
use std::time::Duration;

// The participant vocabulary (`Participant`, `meeting_of`, `reach`, the two
// triage literals `TRIAGE_HANDLER_SET` / `TRIAGE_MH_REACH`) lives in
// `env_tests::fixtures::participant` since story 2 task 10, shared with
// `26_mh_quic.rs`.

/// Margin added to MC's published settle window before a view must have
/// converged: covers MH's connect notification, the push and the flush.
const VIEW_MARGIN: Duration = Duration::from_secs(10);

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
