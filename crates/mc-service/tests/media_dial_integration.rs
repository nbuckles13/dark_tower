//! MC-minted `media_dial` credential through the real meeting actor
//! (ADR-0039 §4/§5; story 4 MC-D: R-5, R-13, R-14, R-15, R-17).
//!
//! Actor-level (`MeetingActor::spawn_with_seams` + handle joins), every
//! participant's outbound queue wired to an `mpsc` channel, a
//! `MockMhRegistrationClient` that records each `RegisterMeeting`'s
//! `media_dial_public_key` — the bytes every token is verified against here
//! with an INDEPENDENT oracle (`ring::signature::UnparsedPublicKey`), never
//! with MC's own code.
//!
//! # Time
//!
//! `current_thread` + `start_paused`, no wall-clock sleeps. The actor's
//! `DialClock` is anchored (test seam) to tokio time at [`BASE_UNIX`], so
//! advancing paused time moves the minted `iat` together with the refresh arm
//! — otherwise a wall-clock `iat` would never expire under paused time and the
//! "never only an expired token" walk would be vacuous. A NON-default TTL
//! ([`TTL`] = 120 s, refresh 40 s) keeps a hardcoded 300 from passing. Jitter
//! is seeded.
//!
//! # Sinks
//!
//! Tokens are never printed: every assertion compares fields, and failure
//! messages name the participant/handler, never the token.

#![cfg(feature = "test-seams")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "common/mod.rs"]
mod test_common;

use std::collections::BTreeMap;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;

use ::common::observability::testing::MetricAssertion;
use mc_service::media_dial::status::{ReportedState, FAILED_REPUSH_BURST, FAILED_REPUSH_REFILL_MS};
use mc_service::media_routing::RemovalTrigger;
use proto_gen::dark_tower::signaling::v1::{server_message, MediaHandlerSet};
use test_common::{mh_handler, MockMhRegistrationClient};
use tokio::sync::mpsc;

use test_common::media_dial::*;

// ---------------------------------------------------------------------------

/// Every entry of every participant's set carries a token for ITS session:
/// verifies under the bytes on the wire, header exact, `sub` = own user id
/// (not the peer's, not the participant id), `aud` = that entry's handler,
/// `meeting_id`, `exp = iat + TTL`. The wire key is the PUBLIC half (not the
/// KEK), and per meeting: another meeting's key differs and rejects these
/// tokens.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn every_member_carries_a_verifiable_token_minted_for_its_own_session() {
    let handlers = [mh_handler(H_A), mh_handler(H_B)];
    let urls = handler_of_url(&handlers);
    let mh = Arc::new(MockMhRegistrationClient::new());
    let meeting = spawn_meeting("dial-1", &handlers);
    let a = join(&meeting, "a", &mh).await;
    let b = join(&meeting, "b", &mh).await;
    let key = wire_key(&mh, "dial-1");

    // The wire key is not the KEK the joiners hold.
    assert_ne!(&key[..], &a.result.meeting_kek.expose()[..]);

    for (who, other, joined) in [("a", "b", &a), ("b", "a", &b)] {
        let set = &joined.result.handler_set;
        assert_eq!(set.handlers.len(), 2, "{who}: both handlers are Active");
        let mut auds = Vec::new();
        for entry in &set.handlers {
            assert!(!entry.media_dial_token.is_empty(), "{who}: token present");
            let token = Token::parse(&entry.media_dial_token);
            assert!(
                token.verifies_under(&key),
                "{who}: verifies under the wire key"
            );
            assert_eq!(
                token.header,
                serde_json::json!({"alg": "EdDSA", "typ": "JWT"}),
                "{who}: header exact, no kid"
            );
            assert_eq!(token.str("token_type"), "media_dial");
            assert_eq!(token.str("sub"), format!("user-{who}"), "{who}: own sub");
            assert_ne!(
                token.str("sub"),
                format!("user-{other}"),
                "{who}: not the peer's"
            );
            assert_ne!(
                token.str("sub"),
                who,
                "{who}: the user id, not the participant id"
            );
            assert_eq!(token.str("meeting_id"), "dial-1");
            assert_eq!(
                token.str("aud"),
                urls[&entry.media_handler_url],
                "{who}: aud is that entry's handler id"
            );
            assert_eq!(token.i64("iat"), BASE_UNIX);
            assert_eq!(token.i64("exp") - token.i64("iat"), i64::from(TTL));
            auds.push(token.str("aud").to_string());
        }
        auds.sort();
        assert_eq!(auds, [H_A, H_B]);
    }

    // Per meeting: a second meeting has its own key, which rejects A's tokens.
    let mh2 = Arc::new(MockMhRegistrationClient::new());
    let meeting2 = spawn_meeting("dial-2", &[mh_handler(H_A)]);
    let _c = join(&meeting2, "c", &mh2).await;
    let key2 = wire_key(&mh2, "dial-2");
    assert_ne!(key, key2);
    let a_token = Token::parse(&a.result.handler_set.handlers[0].media_dial_token);
    assert!(
        !a_token.verifies_under(&key2),
        "another meeting's key rejects it"
    );
}

/// A token-only refresh at TTL/3 (jittered EARLIER, never later) keeps the
/// version and refs, carries NEW tokens, and counts `token_refresh` (change
/// unobserved); a membership change advances the version and counts `change`.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn token_only_refresh_keeps_version_and_refs_and_a_change_advances_it() {
    let handlers = [mh_handler(H_A), mh_handler(H_B)];
    let mh = Arc::new(MockMhRegistrationClient::new());
    let meeting = spawn_meeting("dial-refresh", &handlers);
    let mut a = join(&meeting, "a", &mh).await;
    let t = timing();
    let joined = a.result.handler_set.clone();

    // Nothing before the earliest jittered refresh.
    let snap = MetricAssertion::snapshot();
    advance(t.refresh_interval() - t.max_jitter() - Duration::from_secs(1)).await;
    assert!(
        sets(&mut a.rx).is_empty(),
        "no refresh before interval - max_jitter"
    );

    // Exactly one by the nominal interval.
    advance(t.max_jitter() + Duration::from_secs(1)).await;
    let refreshed = sets(&mut a.rx);
    assert_eq!(refreshed.len(), 1, "one refresh by the nominal interval");
    let refreshed = &refreshed[0];
    assert_eq!(refreshed.handler_set_version, joined.handler_set_version);
    let refs = |s: &MediaHandlerSet| s.handlers.iter().map(|h| h.handler_ref).collect::<Vec<_>>();
    assert_eq!(refs(refreshed), refs(&joined));
    for (old, new) in joined.handlers.iter().zip(&refreshed.handlers) {
        assert!(
            old.media_dial_token != new.media_dial_token,
            "tokens replaced"
        );
    }
    push_cell(&snap, "token_refresh").assert_delta(1);
    push_cell(&snap, "change").assert_delta(0);

    // A membership change advances the version and is counted as one.
    let snap = MetricAssertion::snapshot();
    assert!(meeting
        .remove_handler_for_test(H_B, RemovalTrigger::HeartbeatTimeout)
        .await
        .unwrap());
    run_ready().await;
    let changed = sets(&mut a.rx);
    assert_eq!(changed.len(), 1);
    assert!(changed[0].handler_set_version > joined.handler_set_version);
    assert_eq!(changed[0].handlers.len(), 1);
    push_cell(&snap, "change").assert_delta(1);
    push_cell(&snap, "token_refresh").assert_delta(0);
}

/// A change push at t restarts the refresh clock: no refresh until at least
/// t + interval - max_jitter.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_change_push_reschedules_the_refresh() {
    let handlers = [mh_handler(H_A), mh_handler(H_B)];
    let mh = Arc::new(MockMhRegistrationClient::new());
    let meeting = spawn_meeting("dial-resched", &handlers);
    let mut a = join(&meeting, "a", &mh).await;
    let t = timing();

    advance(Duration::from_secs(20)).await;
    meeting
        .remove_handler_for_test(H_B, RemovalTrigger::HeartbeatTimeout)
        .await
        .unwrap();
    run_ready().await;
    assert_eq!(sets(&mut a.rx).len(), 1, "the change push at t = 20 s");

    // The join-time schedule (due by 40 s) is gone.
    advance(t.refresh_interval() - Duration::from_secs(20)).await;
    assert!(sets(&mut a.rx).is_empty(), "no refresh at the old due time");
    advance(Duration::from_secs(20) - t.max_jitter() - Duration::from_secs(1)).await;
    assert!(
        sets(&mut a.rx).is_empty(),
        "nothing before t + interval - max_jitter"
    );
    advance(t.max_jitter() + Duration::from_secs(1)).await;
    assert_eq!(sets(&mut a.rx).len(), 1, "the rescheduled refresh");
}

/// With paused time across five TTLs (a handler removed mid-walk), an in-set
/// client never holds only an expired token, and each refresh brings new
/// bytes. Positive control: the first token, never replaced, IS observed
/// expired — so the checker can fail.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn an_in_set_client_never_holds_only_an_expired_token() {
    let handlers = [mh_handler(H_A), mh_handler(H_B)];
    let mh = Arc::new(MockMhRegistrationClient::new());
    let meeting = spawn_meeting("dial-walk", &handlers);
    let start = tokio::time::Instant::now();
    let mut a = join(&meeting, "a", &mh).await;
    let key = wire_key(&mh, "dial-walk");

    // The client's holding: the latest token per ref of the latest set.
    let mut held: BTreeMap<u32, String> = a
        .result
        .handler_set
        .handlers
        .iter()
        .map(|h| (h.handler_ref, h.media_dial_token.clone()))
        .collect();
    let first = held.values().next().unwrap().clone();
    let mut refreshes = 0;

    let step = timing().ttl() / 20;
    for i in 0..100 {
        if i == 50 {
            meeting
                .remove_handler_for_test(H_B, RemovalTrigger::HeartbeatTimeout)
                .await
                .unwrap();
            run_ready().await;
        }
        advance(step).await;
        for set in sets(&mut a.rx) {
            let next: BTreeMap<u32, String> = set
                .handlers
                .iter()
                .map(|h| (h.handler_ref, h.media_dial_token.clone()))
                .collect();
            for (r, tok) in &next {
                if let Some(old) = held.get(r) {
                    // `assert!`, never `assert_ne!`: a failure must not print a token.
                    assert!(old != tok, "step {i}: a re-push carries new token bytes");
                }
            }
            held = next;
            refreshes += 1;
        }
        let now = now_unix(start);
        assert!(!held.is_empty());
        for (r, tok) in &held {
            let token = Token::parse(tok);
            assert!(token.verifies_under(&key));
            assert!(
                token.i64("exp") > now,
                "step {i}: ref {r} holds an expired token"
            );
        }
    }
    assert_eq!(held.len(), 1, "the removed handler left the holding");
    assert!(
        refreshes >= 14,
        "about three refreshes per TTL ({refreshes})"
    );
    assert!(
        Token::parse(&first).i64("exp") <= now_unix(start),
        "positive control: an unrefreshed token does expire"
    );
}

/// A FAILED report for an in-set ref re-pushes THAT participant's set only,
/// at once, same version, counted `failed_report`; over-rate reports are
/// limited per session (and refill); another session is unaffected; an
/// out-of-set ref does nothing. No routing effect: only set frames appear.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_failed_report_re_pushes_to_the_sender_only_under_its_session_limit() {
    let handlers = [mh_handler(H_A)];
    let mh = Arc::new(MockMhRegistrationClient::new());
    let meeting = spawn_meeting("dial-failed", &handlers);
    let mut p1 = join(&meeting, "p1", &mh).await;
    let mut p2 = join(&meeting, "p2", &mh).await;
    let mut p3 = join(&meeting, "p3", &mh).await;
    for p in [&mut p1, &mut p2, &mut p3] {
        let _ = test_common::drain_frames(&mut p.rx);
    }
    let set = p1.result.handler_set.clone();
    let in_set = set.handlers[0].handler_ref;
    let version = set.handler_set_version;
    let failed = || vec![report(in_set, version, ReportedState::Failed)];
    // No routing effect: every MH programming call made before the reports.
    let mh_events_before = mh.events();

    let snap = MetricAssertion::snapshot();
    meeting
        .report_client_handler_statuses("p1".into(), failed())
        .await
        .unwrap();
    run_ready().await;
    let p1_frames = test_common::drain_frames(&mut p1.rx);
    assert_eq!(p1_frames.len(), 1, "exactly one frame, and it is the set");
    match &p1_frames[0] {
        server_message::Message::MediaHandlerSet(s) => {
            assert_eq!(s.handler_set_version, version);
            let key = wire_key(&mh, "dial-failed");
            assert!(
                Token::parse(&s.handlers[0].media_dial_token).verifies_under(&key),
                "a valid credential for the in-set ref"
            );
        }
        other => panic!("expected only a set, got a {}", frame_kind(other)),
    }
    assert!(
        test_common::drain_frames(&mut p2.rx).is_empty(),
        "p2 gets nothing"
    );
    assert!(
        test_common::drain_frames(&mut p3.rx).is_empty(),
        "p3 gets nothing"
    );
    push_cell(&snap, "failed_report").assert_delta(1);

    // Over-rate: one token is spent; BURST more reports re-push BURST - 1.
    let snap = MetricAssertion::snapshot();
    for _ in 0..FAILED_REPUSH_BURST {
        meeting
            .report_client_handler_statuses("p1".into(), failed())
            .await
            .unwrap();
        run_ready().await;
    }
    assert_eq!(sets(&mut p1.rx).len(), (FAILED_REPUSH_BURST - 1) as usize);
    push_cell(&snap, "failed_report").assert_delta(u64::from(FAILED_REPUSH_BURST - 1));

    // Session isolation: p2 is not limited by p1's exhaustion.
    meeting
        .report_client_handler_statuses("p2".into(), failed())
        .await
        .unwrap();
    run_ready().await;
    assert_eq!(sets(&mut p2.rx).len(), 1, "p2's own session has tokens");

    // Refill: one more is granted after one refill interval.
    advance(Duration::from_millis(FAILED_REPUSH_REFILL_MS)).await;
    meeting
        .report_client_handler_statuses("p1".into(), failed())
        .await
        .unwrap();
    run_ready().await;
    assert_eq!(sets(&mut p1.rx).len(), 1, "refilled");

    // Out-of-set ref: nothing.
    let snap = MetricAssertion::snapshot();
    meeting
        .report_client_handler_statuses(
            "p3".into(),
            vec![report(999, version, ReportedState::Failed)],
        )
        .await
        .unwrap();
    run_ready().await;
    assert!(test_common::drain_frames(&mut p3.rx).is_empty());
    push_cell(&snap, "failed_report").assert_delta(0);

    // No report re-programmed any handler: client status is never routing input.
    assert_eq!(
        mh.events(),
        mh_events_before,
        "no RegisterMeeting/EndMeeting caused"
    );
}

fn frame_kind(m: &server_message::Message) -> &'static str {
    match m {
        server_message::Message::MediaHandlerSet(_) => "MediaHandlerSet",
        _ => "non-set message",
    }
}

/// The latest status per (participant, ref): a CONNECTED after a DISCONNECTED
/// supersedes it; a stale version on an in-set ref is still recorded
/// (correlation only); an out-of-set ref creates nothing; after the ref leaves
/// the set its entry is gone and a later report for it is ignored.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn the_latest_status_per_in_set_ref_is_kept_and_removed_refs_are_ignored() {
    let handlers = [mh_handler(H_A), mh_handler(H_B)];
    let urls = handler_of_url(&handlers);
    let mh = Arc::new(MockMhRegistrationClient::new());
    let meeting = spawn_meeting("dial-status", &handlers);
    let a = join(&meeting, "a", &mh).await;
    let set = &a.result.handler_set;
    let ref_of = |id: &str| {
        set.handlers
            .iter()
            .find(|h| urls[&h.media_handler_url] == id)
            .unwrap()
            .handler_ref
    };
    let (ra, rb) = (ref_of(H_A), ref_of(H_B));
    let v = set.handler_set_version;

    meeting
        .report_client_handler_statuses(
            "a".into(),
            vec![report(ra, v, ReportedState::Disconnected)],
        )
        .await
        .unwrap();
    meeting
        .report_client_handler_statuses(
            "a".into(),
            vec![
                report(ra, v - 1, ReportedState::Connected),
                report(rb, v, ReportedState::Failed),
                report(999, v, ReportedState::Connected),
            ],
        )
        .await
        .unwrap();
    let latest = meeting.client_handler_statuses_for_test("a").await.unwrap();
    let ra_nz = NonZeroU32::new(ra).unwrap();
    let rb_nz = NonZeroU32::new(rb).unwrap();
    assert_eq!(latest[&ra_nz].state, ReportedState::Connected, "superseded");
    assert_eq!(
        latest[&ra_nz].handler_set_version,
        v - 1,
        "stale version recorded"
    );
    assert_eq!(latest[&rb_nz].state, ReportedState::Failed);
    assert_eq!(latest.len(), 2, "the out-of-set ref created nothing");

    meeting
        .remove_handler_for_test(H_B, RemovalTrigger::HeartbeatTimeout)
        .await
        .unwrap();
    meeting
        .report_client_handler_statuses("a".into(), vec![report(rb, v, ReportedState::Connected)])
        .await
        .unwrap();
    let latest = meeting.client_handler_statuses_for_test("a").await.unwrap();
    assert!(
        !latest.contains_key(&rb_nz),
        "pruned, and the late report ignored"
    );
    assert!(latest.contains_key(&ra_nz));
}

/// A departed participant gets no further push and no refresh is counted for
/// it.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_departed_participant_gets_no_refresh() {
    let t = timing();
    let handlers = [mh_handler(H_A)];
    let mh = Arc::new(MockMhRegistrationClient::new());
    let meeting = spawn_meeting("dial-depart", &handlers);
    let mut a = join(&meeting, "a", &mh).await;
    let mut b = join(&meeting, "b", &mh).await;
    meeting.participant_leave("a".into()).await.unwrap();
    run_ready().await;
    let _ = test_common::drain_frames(&mut a.rx);
    let _ = test_common::drain_frames(&mut b.rx);

    let snap = MetricAssertion::snapshot();
    for _ in 0..(3 * t.refresh_interval().as_secs()) {
        advance(Duration::from_secs(1)).await;
    }
    assert!(sets(&mut a.rx).is_empty(), "the leaver gets nothing");
    let b_refreshes = sets(&mut b.rx).len();
    assert!(b_refreshes >= 3);
    push_cell(&snap, "token_refresh").assert_delta(b_refreshes as u64);
}

/// A parked joiner gets no refresh before its release (structurally: the
/// park deadline is shorter than the earliest refresh at the lowest legal
/// TTL), then the set minted AT release, and its refresh clock starts there.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_parked_joiner_is_minted_at_release_and_refreshed_from_there() {
    let t = timing();
    // Parked: hold the only handler's ack; the joiner parks.
    let mh2 = Arc::new(MockMhRegistrationClient::new());
    mh2.hold_handler(H_A);
    let meeting2 = spawn_meeting("dial-park", &[mh_handler(H_A)]);
    mh2.heartbeat_to_meeting(&meeting2);
    let start = tokio::time::Instant::now();
    let (tx, mut rx) = mpsc::channel::<bytes::Bytes>(64);
    let h = meeting2.clone();
    let m = media(&mh2);
    let reply = tokio::spawn(async move {
        h.connection_join(
            "conn-p".into(),
            "user-p".into(),
            "p".into(),
            String::new(),
            false,
            test_common::test_identity_key(),
            Some(tx),
            m,
        )
        .await
    });
    // The invariant that makes "no refresh while parked" structural: the park
    // deadline is shorter than the earliest refresh at the LOWEST legal TTL.
    let floor = mc_service::media_dial::MediaDialTiming::from_ttl_seconds(60);
    assert!(
        mc_service::actors::join_park::FIRST_JOIN_PARK_DEADLINE
            < floor.refresh_interval() - floor.max_jitter()
    );
    advance(Duration::from_secs(5)).await;
    assert!(!reply.is_finished(), "parked");
    assert!(sets(&mut rx).is_empty(), "no set before the JoinResponse");
    mh2.release_handler(H_A);
    run_ready().await;
    let result = reply.await.unwrap().unwrap();
    let token = Token::parse(&result.handler_set.handlers[0].media_dial_token);
    assert_eq!(token.i64("iat"), now_unix(start), "minted at release");

    // The refresh clock starts AT RELEASE.
    let snap = MetricAssertion::snapshot();
    advance(t.refresh_interval() - t.max_jitter() - Duration::from_secs(1)).await;
    assert!(
        sets(&mut rx).is_empty(),
        "no refresh before interval - max_jitter after release"
    );
    advance(t.max_jitter() + Duration::from_secs(1)).await;
    assert_eq!(
        sets(&mut rx).len(),
        1,
        "one refresh by the nominal interval after release"
    );
    push_cell(&snap, "token_refresh").assert_delta(1);
}

/// A join whose set cannot be minted is refused with NOTHING admitted: the
/// mint runs before any roster, binding, slot or broadcast state (fail
/// closed, never half-admitted). Forced through the anchored clock: once
/// `iat + TTL` overflows, every mint fails `ExpOverflow`. A refresh that
/// cannot be minted sends nothing (never a tokenless set).
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_join_whose_set_cannot_be_minted_admits_nothing() {
    let handlers = [mh_handler(H_A)];
    let mh = Arc::new(MockMhRegistrationClient::new());
    // Mintable for the first 100 s, overflowing after.
    let seams = mc_service::actors::MeetingSeams {
        dial_clock_base_unix: Some(i64::MAX - i64::from(TTL) - 100),
        ..seams()
    };
    let meeting = spawn_meeting_with("dial-mint-fail", &handlers, &seams);
    let mut a = join(&meeting, "a", &mh).await;
    assert_eq!(
        a.result.handler_set.handlers.len(),
        1,
        "positive arm: minted"
    );

    advance(Duration::from_secs(200)).await;
    // Time passes continuously, so the refreshes due while the clock was
    // still mintable (before 100 s) went out; drop them. Every refresh from
    // here on is past the overflow.
    let _mintable = sets(&mut a.rx);
    // A's refresh could not be minted: nothing partial was sent.
    for _ in 0..timing().refresh_interval().as_secs() {
        advance(Duration::from_secs(1)).await;
    }
    assert!(
        sets(&mut a.rx).is_empty(),
        "no tokenless set is ever pushed"
    );

    let (tx, _rx) = mpsc::channel::<bytes::Bytes>(16);
    let refused = meeting
        .connection_join(
            "conn-b".into(),
            "user-b".into(),
            "b".into(),
            String::new(),
            false,
            test_common::test_identity_key(),
            Some(tx),
            media(&mh),
        )
        .await;
    assert!(matches!(
        refused,
        Err(mc_service::errors::McError::Internal(_))
    ));
    let state = meeting.get_state().await.unwrap();
    let ids: Vec<&str> = state
        .participants
        .iter()
        .map(|p| p.participant_id.as_str())
        .collect();
    assert_eq!(ids, ["a"], "the refused joiner was never admitted");
    assert!(state.participants.iter().all(|p| p.user_id != "user-b"));
}

/// A parked join whose set cannot be minted AT RELEASE is refused AND its
/// admission is undone (the leave path): the roster ends consistent with the
/// reply, so the user's sub is not left bound to a ghost entry. Forced
/// through the anchored clock: admission mints an empty set (nothing Active
/// yet), and by release `iat + TTL` overflows.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_parked_join_whose_set_cannot_be_minted_at_release_is_undone() {
    let mh = Arc::new(MockMhRegistrationClient::new());
    mh.hold_handler(H_A);
    let seams = mc_service::actors::MeetingSeams {
        dial_clock_base_unix: Some(i64::MAX - i64::from(TTL) - 3),
        ..seams()
    };
    let meeting = spawn_meeting_with("dial-park-fail", &[mh_handler(H_A)], &seams);
    let (tx, _rx) = mpsc::channel::<bytes::Bytes>(16);
    let h = meeting.clone();
    let m = media(&mh);
    let reply = tokio::spawn(async move {
        h.connection_join(
            "conn-p".into(),
            "user-p".into(),
            "p".into(),
            String::new(),
            false,
            test_common::test_identity_key(),
            Some(tx),
            m,
        )
        .await
    });
    advance(Duration::from_secs(5)).await;
    assert!(!reply.is_finished(), "parked");
    let before = meeting.get_state().await.unwrap();
    assert_eq!(before.participants.len(), 1, "admitted while parked");

    let snap = MetricAssertion::snapshot();
    mh.release_handler(H_A);
    run_ready().await;
    let refused = reply.await.unwrap();
    assert!(matches!(
        refused,
        Err(mc_service::errors::McError::Internal(_))
    ));
    // Undone through the ONE removal path: counted as a leave, and — the
    // refused joiner being the meeting's only participant — the emptied
    // meeting ends (its actor is gone), exactly as after a real last leave.
    snap.counter("mc_participant_leaves_total")
        .with_labels(&[("reason", "removed")])
        .assert_delta(1);
    assert!(
        meeting.get_state().await.is_err(),
        "the emptied meeting ended: no ghost roster entry survives"
    );
}
