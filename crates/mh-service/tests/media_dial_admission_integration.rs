//! Story 4 MH-1: `media_dial`-only client admission and the pending cap,
//! through the REAL accept loop (Tier 1b; ADR-0039 §3/§5/§7/§8).
//!
//! Gates (`docs/debates/2026-10-28-mh-replacement-and-migration/test-gates.md`
//! "MH", token and pending bullets). Every refusal arm is paired with a
//! positive arm that varies only the field under test, and asserts the close
//! code on the capsule, the exact `mh_jwt_validations_total` cell (siblings
//! unobserved), and that NOTHING was held: no admitted connection and no
//! `NotifyParticipantConnected` reached the mock MC.
//!
//! The ordered verification itself (a)–(f), header members and boundaries are
//! unit-tested on the pure functions in `src/auth/mod.rs`; this suite proves
//! the wiring: the code each refusal carries, that nothing is parked, the
//! counters, the pending cap and the bind-time rules.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "common/mod.rs"]
mod test_common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use common::observability::testing::{MetricAssertion, MetricSnapshot};
use mh_service::auth::{MediaDialKey, MediaDialRejection};
use mh_service::grpc::McClient;
use mh_service::observability::metrics::MediaSessionStartOutcome;
use mh_service::routing::{MeetingKey, MeetingPolicy, SenderId};
use mh_service::session::release::ReleaseKind;
use mh_service::session::{MeetingRegistration, SessionManagerHandle};
use mh_service::webtransport::close::CloseCode;
use mh_service::webtransport::connection::ClientAdmission;
use mh_test_utils::media_dial::{tamper_signature, DialForge, ATTACKER_SEED, MEETING_SEED};
use proto_gen::dark_tower::internal::v1::NotifyParticipantConnectedRequest;
use proto_gen::dark_tower::signaling::v1::MhCloseCode;
use serde_json::json;
use test_common::accept_loop_rig::AcceptLoopRig;
use test_common::close_metrics::assert_closed_only;
use test_common::media_frame::audio_datagram;
use test_common::mock_mc::{
    start_mock_mc_server, MockBehavior, MockMcHandle, MockMcServer, SenderReplies,
};
use test_common::test_token_receiver;
use test_common::tokens::{
    mint_aged_dial_token, mint_dial_token, mint_meeting_token, DIAL_TTL_SECS,
};
use test_common::wt_client::{
    connect_and_open_bi, expect_session_close, expect_still_open, write_mh_connect,
    write_mh_connect_both,
};
use test_common::TestKeypair;
use tokio::sync::mpsc;

const HANDLER_ID: &str = "mh-dial-admission-001";
const MEETING: &str = "meeting-dial";
/// A non-default skew, so a hardcoded 30 (or the old 300) fails the boundary.
const SKEW_SECS: u64 = 7;
/// A non-default cap, so a hardcoded 2 fails.
const CAP: usize = 3;
const WAIT: Duration = Duration::from_secs(5);

const SENDER_PUBLISHER: u32 = 41;
const SENDER_SUBSCRIBER: u32 = 17;

struct Suite {
    sm: SessionManagerHandle,
    wt: AcceptLoopRig,
    mc: MockMcHandle,
    /// Every `NotifyParticipantConnected` the mock received.
    connected_rx: mpsc::Receiver<NotifyParticipantConnectedRequest>,
    /// Our own sender into the mock's capacity-1 capture channel: filling it
    /// with a dummy holds every later Connected RPC in flight.
    gate_tx: mpsc::Sender<NotifyParticipantConnectedRequest>,
    /// Held: when the suite was built `gated`, the channel is pre-filled and
    /// nothing drains it until [`Suite::open_gate`], so every Connected RPC
    /// stays in flight (the connection awaits its bind). Deterministic, no
    /// sleeps.
    gated: bool,
}

impl Suite {
    async fn start(replies: SenderReplies) -> Self {
        Self::build(replies, false, MockBehavior::Accept).await
    }

    async fn start_gated(replies: SenderReplies) -> Self {
        Self::build(replies, true, MockBehavior::Accept).await
    }

    async fn build(replies: SenderReplies, gated: bool, behavior: MockBehavior) -> Self {
        let sm = SessionManagerHandle::new_for_tests(mh_test_utils::admission::never_binding());
        let mc_client = Arc::new(McClient::new(test_token_receiver()));
        let wt = AcceptLoopRig::start_with(
            sm.clone(),
            mc_client,
            HANDLER_ID.to_string(),
            32,
            ClientAdmission {
                clock_skew: Duration::from_secs(SKEW_SECS),
                max_pending_per_participant: CAP,
            },
        )
        .await;
        let (tx, connected_rx) = mpsc::channel(1);
        if gated {
            tx.send(NotifyParticipantConnectedRequest::default())
                .await
                .unwrap();
        }
        let mc = start_mock_mc_server(
            MockMcServer::new(behavior)
                .with_sender_replies(replies)
                .with_connected_tx(tx.clone()),
        )
        .await;
        Self {
            sm,
            wt,
            mc,
            connected_rx,
            gate_tx: tx,
            gated,
        }
    }

    /// Release every held Connected RPC (and keep draining).
    fn open_gate(&mut self) {
        assert!(self.gated);
        self.gated = false;
        self.drain_connected();
    }

    /// Hold every LATER Connected RPC in flight: drain, then fill the
    /// capacity-1 channel with a dummy.
    fn close_gate(&mut self) {
        assert!(!self.gated);
        loop {
            self.drain_connected();
            if self
                .gate_tx
                .try_send(NotifyParticipantConnectedRequest::default())
                .is_ok()
            {
                break;
            }
        }
        self.gated = true;
    }

    async fn register_with(&self, meeting: &str, key: &[u8]) {
        self.sm
            .register_meeting(
                meeting.to_string(),
                MeetingRegistration {
                    mc_id: "mc-dial-test".to_string(),
                    mc_grpc_endpoint: format!("http://{}", self.mc.addr),
                    heartbeat_interval: mh_test_utils::media_policy::TEST_HEARTBEAT_INTERVAL,
                    registered_at: Instant::now(),
                    media_dial_key: MediaDialKey::from_wire(key).unwrap(),
                },
                None,
                mh_test_utils::admission::fixture_policy_limits().max_registered_meetings,
            )
            .await
            .unwrap();
    }

    async fn register(&self, meeting: &str) {
        self.register_with(meeting, &DialForge::new(MEETING_SEED).public_key())
            .await;
    }

    async fn dial(&self, token: &str) -> (wtransport::Connection, wtransport::stream::SendStream) {
        let (conn, mut send, _recv) = connect_and_open_bi(&self.wt.url).await;
        write_mh_connect(&mut send, token).await.unwrap();
        (conn, send)
    }

    async fn dial_both(
        &self,
        join_token: &str,
        media_dial_token: &str,
    ) -> (wtransport::Connection, wtransport::stream::SendStream) {
        let (conn, mut send, _recv) = connect_and_open_bi(&self.wt.url).await;
        write_mh_connect_both(&mut send, join_token, media_dial_token)
            .await
            .unwrap();
        (conn, send)
    }

    /// Drain every Connected request the mock has seen so far (only while
    /// ungated). Returns how many.
    fn drain_connected(&mut self) -> usize {
        let mut n = 0;
        while self.connected_rx.try_recv().is_ok() {
            n += 1;
        }
        n
    }

    /// Wait until `participant` holds `sender` in `meeting`, draining the
    /// Connected channel meanwhile (an ungated suite).
    async fn bound_within(&mut self, meeting: &str, sender: u32) -> bool {
        let stop = Instant::now() + WAIT;
        let sender = SenderId::from_wire(sender).unwrap();
        loop {
            if !self.gated {
                self.drain_connected();
            }
            if self
                .sm
                .sender_bindings()
                .holder_of(&MeetingKey::new(meeting), sender)
                .is_some()
            {
                return true;
            }
            if Instant::now() >= stop {
                return false;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    /// Wait until the actor holds exactly `n` admitted connections.
    async fn admitted(&self, n: usize) {
        let stop = Instant::now() + WAIT;
        while self.sm.active_connection_count().await != n {
            assert!(Instant::now() < stop, "expected {n} admitted connections");
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }
}

/// The exact `mh_jwt_validations_total{token_type="media_dial"}` cell moved by
/// one, and every sibling cell is unobserved (ADR-0032).
fn assert_dial_validation(snap: &MetricSnapshot, expected: Option<MediaDialRejection>) {
    let cell = |result: &str, reason: &str| {
        snap.counter("mh_jwt_validations_total").with_labels(&[
            ("result", result),
            ("token_type", "media_dial"),
            ("failure_reason", reason),
        ])
    };
    if expected.is_none() {
        cell("success", "none").assert_delta(1);
    } else {
        cell("success", "none").assert_unobserved();
    }
    for r in MediaDialRejection::ALL {
        if Some(r) == expected {
            cell("failure", r.as_label()).assert_delta(1);
        } else {
            cell("failure", r.as_label()).assert_unobserved();
        }
    }
}

fn assert_start_outcome_only(snap: &MetricSnapshot, expected: Option<MediaSessionStartOutcome>) {
    for outcome in MediaSessionStartOutcome::ALL {
        let q = snap
            .counter("mh_media_session_starts_total")
            .with_labels(&[("outcome", outcome.as_label()), ("key_custody", "operator")]);
        if Some(outcome) == expected {
            q.assert_delta(1);
        } else {
            q.assert_unobserved();
        }
    }
}

fn claims(meeting: &str, aud: &str, sub: &str) -> serde_json::Value {
    DialForge::claims_json(meeting, aud, sub, Utc::now().timestamp(), DIAL_TTL_SECS)
}

/// Dial `token`, expect `code`, and prove nothing was held: no admitted
/// connection, no Connected RPC, and exactly the expected counters.
async fn expect_refused(
    suite: &mut Suite,
    token: &str,
    code: MhCloseCode,
    rejection: MediaDialRejection,
    ctx: &str,
) {
    let snap = MetricAssertion::snapshot();
    let (conn, _send) = suite.dial(token).await;
    expect_session_close(&conn, code, WAIT, ctx).await;
    suite.wt.wait_idle(WAIT).await;
    assert_dial_validation(&snap, Some(rejection));
    let close = if code == MhCloseCode::WrongHandler {
        CloseCode::WrongHandler
    } else {
        CloseCode::Declined
    };
    assert_closed_only(&snap, close, 1);
    assert_start_outcome_only(&snap, None);
    assert_eq!(
        suite.sm.active_connection_count().await,
        0,
        "{ctx}: nothing held"
    );
    assert_eq!(suite.drain_connected(), 0, "{ctx}: MC was never asked");
    snap.counter("mh_webtransport_connections_total")
        .with_labels(&[("status", "error")])
        .assert_delta(u64::from(code == MhCloseCode::Declined));
}

/// Dial `token` and expect it admitted and bound as `sender`.
async fn expect_admitted(
    suite: &mut Suite,
    token: &str,
    sender: u32,
    ctx: &str,
) -> wtransport::Connection {
    let snap = MetricAssertion::snapshot();
    let (conn, _send) = suite.dial(token).await;
    assert!(
        suite.bound_within(MEETING, sender).await,
        "{ctx}: admitted and bound"
    );
    assert_dial_validation(&snap, None);
    expect_still_open(&conn, Duration::from_millis(200), ctx).await;
    conn
}

// ---------------------------------------------------------------------------
// Positive: a meeting-key token is admitted, repeatedly (no jti cache)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn a_meeting_key_token_is_admitted_and_reusable_within_its_ttl() {
    let mut suite = Suite::start(SenderReplies::default().with("user-1", SENDER_PUBLISHER)).await;
    suite.register(MEETING).await;
    let token = mint_dial_token(MEETING, "user-1", HANDLER_ID);

    let first = expect_admitted(&mut suite, &token, SENDER_PUBLISHER, "first dial").await;
    first.close(0u32.into(), b"");
    suite.wt.wait_idle(WAIT).await;
    // The SAME string again, after the first session closed.
    let _second = expect_admitted(&mut suite, &token, SENDER_PUBLISHER, "second dial").await;
}

/// The SDK's join-time shape (until P3): a valid dial token PLUS a non-empty
/// `join_token` — here a genuinely EXPIRED AC token, the migration case — is
/// admitted: MH never reads `join_token`.
#[tokio::test(flavor = "current_thread")]
async fn a_join_token_beside_a_valid_dial_token_is_ignored_even_when_expired() {
    let mut suite = Suite::start(SenderReplies::default().with("user-1", SENDER_PUBLISHER)).await;
    suite.register(MEETING).await;
    let ac = TestKeypair::new(31, "ac-key");
    let mut expired = test_common::tokens::meeting_claims(MEETING, "user-1", "meeting");
    expired.iat -= 7200;
    expired.exp = expired.iat + 900;
    let expired_ac = ac.sign_token(&expired);

    let snap = MetricAssertion::snapshot();
    let (conn, _send) = suite
        .dial_both(&expired_ac, &mint_dial_token(MEETING, "user-1", HANDLER_ID))
        .await;
    assert!(suite.bound_within(MEETING, SENDER_PUBLISHER).await);
    assert_dial_validation(&snap, None);
    expect_still_open(&conn, Duration::from_millis(200), "join-time shape").await;
}

/// Never both credentials: a VALID AC meeting token with an EMPTY dial token
/// is DECLINED (empty is malformed; there is no fallback).
#[tokio::test(flavor = "current_thread")]
async fn a_valid_ac_join_token_without_a_dial_token_is_declined() {
    let mut suite = Suite::start(SenderReplies::default().with("user-1", SENDER_PUBLISHER)).await;
    suite.register(MEETING).await;
    let ac = TestKeypair::new(31, "ac-key");
    let snap = MetricAssertion::snapshot();
    let (conn, _send) = suite
        .dial_both(&mint_meeting_token(&ac, MEETING, "user-1"), "")
        .await;
    expect_session_close(
        &conn,
        MhCloseCode::Declined,
        WAIT,
        "AC token, no dial token",
    )
    .await;
    suite.wt.wait_idle(WAIT).await;
    assert_dial_validation(&snap, Some(MediaDialRejection::Malformed));
    assert_eq!(suite.drain_connected(), 0);
}

// ---------------------------------------------------------------------------
// Refusals, each with its positive twin
// ---------------------------------------------------------------------------

/// Another meeting's key (cross-meeting): meeting-2 is registered with the
/// ATTACKER key; a token for meeting-2 signed with MEETING's key is DECLINED,
/// and the same claims signed with meeting-2's own key are admitted.
#[tokio::test(flavor = "current_thread")]
async fn another_meetings_key_is_declined_and_its_own_key_admits() {
    let mut suite = Suite::start(SenderReplies::default().with("user-1", SENDER_PUBLISHER)).await;
    let other = DialForge::new(ATTACKER_SEED);
    suite.register_with("meeting-2", &other.public_key()).await;
    let c = claims("meeting-2", HANDLER_ID, "user-1");
    expect_refused(
        &mut suite,
        &DialForge::new(MEETING_SEED).sign(&c),
        MhCloseCode::Declined,
        MediaDialRejection::SignatureInvalid,
        "cross-meeting key",
    )
    .await;
    let snap = MetricAssertion::snapshot();
    let (_conn, _send) = suite.dial(&other.sign(&c)).await;
    assert!(
        suite.bound_within("meeting-2", SENDER_PUBLISHER).await,
        "own key admits"
    );
    assert_dial_validation(&snap, None);
}

/// Expiry against the CONFIGURED skew (7 s, not 30, not 300).
#[tokio::test(flavor = "current_thread")]
async fn expiry_is_judged_against_the_configured_skew() {
    let mut suite = Suite::start(SenderReplies::default().with("user-1", SENDER_PUBLISHER)).await;
    suite.register(MEETING).await;
    let skew = i64::try_from(SKEW_SECS).unwrap();
    expect_refused(
        &mut suite,
        &mint_aged_dial_token(MEETING, "user-1", HANDLER_ID, DIAL_TTL_SECS + skew + 5),
        MhCloseCode::Declined,
        MediaDialRejection::Expired,
        "expired beyond the skew",
    )
    .await;
    expect_admitted(
        &mut suite,
        &mint_aged_dial_token(MEETING, "user-1", HANDLER_ID, DIAL_TTL_SECS + skew - 4),
        SENDER_PUBLISHER,
        "expired within the skew",
    )
    .await;
}

#[tokio::test(flavor = "current_thread")]
async fn a_token_type_other_than_media_dial_is_declined() {
    let mut suite = Suite::start(SenderReplies::default().with("user-1", SENDER_PUBLISHER)).await;
    suite.register(MEETING).await;
    let mut c = claims(MEETING, HANDLER_ID, "user-1");
    c["token_type"] = json!("meeting");
    expect_refused(
        &mut suite,
        &DialForge::new(MEETING_SEED).sign(&c),
        MhCloseCode::Declined,
        MediaDialRejection::WrongType,
        "token_type",
    )
    .await;
    expect_admitted(
        &mut suite,
        &mint_dial_token(MEETING, "user-1", HANDLER_ID),
        SENDER_PUBLISHER,
        "media_dial",
    )
    .await;
}

/// An AC-signed meeting token (as the dial token) is DECLINED: for a held
/// meeting it fails the signature (never `wrong_type`), for an unheld one the
/// key lookup.
#[tokio::test(flavor = "current_thread")]
async fn an_ac_meeting_token_is_declined() {
    let mut suite = Suite::start(SenderReplies::default().with("user-1", SENDER_PUBLISHER)).await;
    suite.register(MEETING).await;
    let ac = TestKeypair::new(31, "ac-key");
    expect_refused(
        &mut suite,
        &mint_meeting_token(&ac, MEETING, "user-1"),
        MhCloseCode::Declined,
        MediaDialRejection::SignatureInvalid,
        "AC token, held meeting",
    )
    .await;
    expect_refused(
        &mut suite,
        &mint_meeting_token(&ac, "meeting-unheld", "user-1"),
        MhCloseCode::Declined,
        MediaDialRejection::NoMeetingKey,
        "AC token, unheld meeting",
    )
    .await;
    expect_admitted(
        &mut suite,
        &mint_dial_token(MEETING, "user-1", HANDLER_ID),
        SENDER_PUBLISHER,
        "control",
    )
    .await;
}

/// Malformed through the two inputs an unauthenticated peer can drive at
/// will — oversize and a non-EdDSA `alg` — plus garbage and a tampered
/// signature; the exactly-at-cap token is admitted.
#[tokio::test(flavor = "current_thread")]
async fn malformed_oversized_and_non_eddsa_tokens_are_declined() {
    let mut suite = Suite::start(SenderReplies::default().with("user-1", SENDER_PUBLISHER)).await;
    suite.register(MEETING).await;
    let forge = DialForge::new(MEETING_SEED);
    let c = claims(MEETING, HANDLER_ID, "user-1");
    let cases = [
        (
            "oversized",
            // VALIDLY signed, one byte over the cap: only step (a) refuses it.
            forge.sign_padded_to(&c, common::jwt::MAX_JWT_SIZE_BYTES + 1),
            MediaDialRejection::Malformed,
        ),
        (
            "alg none",
            forge.sign_with_header(&json!({"alg": "none"}), &c),
            MediaDialRejection::Malformed,
        ),
        (
            "alg HS256",
            forge.sign_with_header(&json!({"alg": "HS256"}), &c),
            MediaDialRejection::Malformed,
        ),
        (
            "garbage",
            "not.a.jwt".to_string(),
            MediaDialRejection::Malformed,
        ),
        (
            "tampered signature",
            tamper_signature(&forge.sign(&c)),
            MediaDialRejection::SignatureInvalid,
        ),
    ];
    for (ctx, token, rejection) in cases {
        expect_refused(&mut suite, &token, MhCloseCode::Declined, rejection, ctx).await;
    }
    let at_cap = forge.sign_padded_to(&c, common::jwt::MAX_JWT_SIZE_BYTES);
    expect_admitted(
        &mut suite,
        &at_cap,
        SENDER_PUBLISHER,
        "exactly at the size cap",
    )
    .await;
}

/// A correctly signed token for a meeting MH does not hold — never
/// registered, or already released — is DECLINED at accept with nothing
/// parked; registering it (positive twin) admits the same token.
#[tokio::test(flavor = "current_thread")]
async fn a_meeting_this_handler_does_not_hold_is_declined_and_nothing_is_parked() {
    let mut suite = Suite::start(SenderReplies::default().with("user-1", SENDER_PUBLISHER)).await;
    let token = mint_dial_token(MEETING, "user-1", HANDLER_ID);
    expect_refused(
        &mut suite,
        &token,
        MhCloseCode::Declined,
        MediaDialRejection::NoMeetingKey,
        "never registered",
    )
    .await;
    assert!(
        !suite.sm.is_meeting_registered(MEETING).await,
        "nothing was created for it"
    );

    suite.register(MEETING).await;
    let conn = expect_admitted(&mut suite, &token, SENDER_PUBLISHER, "registered").await;

    suite
        .sm
        .end_meeting(
            MEETING.to_string(),
            "mc-dial-test".to_string(),
            ReleaseKind::MeetingEnded,
            WAIT,
        )
        .await
        .unwrap();
    expect_session_close(&conn, MhCloseCode::HandlerReleased, WAIT, "released").await;
    suite.wt.wait_idle(WAIT).await;
    expect_refused(
        &mut suite,
        &token,
        MhCloseCode::Declined,
        MediaDialRejection::NoMeetingKey,
        "released",
    )
    .await;
}

/// `aud` ≠ this handler: WRONG_HANDLER, only after full validation. The
/// distinguishing inputs: a bad signature with a wrong aud, and an expired
/// token with a wrong aud, are DECLINED.
#[tokio::test(flavor = "current_thread")]
async fn wrong_audience_is_wrong_handler_only_after_full_validation() {
    let mut suite = Suite::start(SenderReplies::default().with("user-1", SENDER_PUBLISHER)).await;
    suite.register(MEETING).await;
    let wrong = claims(MEETING, "mh-some-other-handler", "user-1");
    expect_refused(
        &mut suite,
        &DialForge::new(MEETING_SEED).sign(&wrong),
        MhCloseCode::WrongHandler,
        MediaDialRejection::AudMismatch,
        "wrong aud",
    )
    .await;
    expect_refused(
        &mut suite,
        &DialForge::new(ATTACKER_SEED).sign(&wrong),
        MhCloseCode::Declined,
        MediaDialRejection::SignatureInvalid,
        "bad signature + wrong aud",
    )
    .await;
    expect_refused(
        &mut suite,
        &mint_aged_dial_token(
            MEETING,
            "user-1",
            "mh-some-other-handler",
            DIAL_TTL_SECS + 60,
        ),
        MhCloseCode::Declined,
        MediaDialRejection::Expired,
        "expired + wrong aud",
    )
    .await;
    expect_admitted(
        &mut suite,
        &mint_dial_token(MEETING, "user-1", HANDLER_ID),
        SENDER_PUBLISHER,
        "own aud",
    )
    .await;
}

/// `kid`/`jwk` headers are ignored: same outcome with and without them, on an
/// accept arm and a reject arm. (MH has no key-fetching client on this path
/// at all — the rig builds none — so there is nothing to fetch from.)
#[tokio::test(flavor = "current_thread")]
async fn key_locating_headers_change_nothing() {
    let mut suite = Suite::start(SenderReplies::default().with("user-1", SENDER_PUBLISHER)).await;
    suite.register(MEETING).await;
    let forge = DialForge::new(MEETING_SEED);
    let attacker = DialForge::new(ATTACKER_SEED);
    let header = json!({"alg": "EdDSA", "kid": "k-1", "jwk": attacker.jwk()});
    let c = claims(MEETING, HANDLER_ID, "user-1");
    expect_refused(
        &mut suite,
        &attacker.sign_with_header(&header, &c),
        MhCloseCode::Declined,
        MediaDialRejection::SignatureInvalid,
        "attacker key announced in the header",
    )
    .await;
    expect_admitted(
        &mut suite,
        &forge.sign_with_header(&header, &c),
        SENDER_PUBLISHER,
        "meeting key with kid/jwk",
    )
    .await;
}

// ---------------------------------------------------------------------------
// Pending cap (ADR-0039 §5; L4-8)
// ---------------------------------------------------------------------------

/// With MC's bind answers held in flight: the CAP-th connection of one
/// (meeting, sub) is admitted and the CAP+1-th is DECLINED and counted
/// `declined_pending_cap` (siblings unobserved), without asking MC. Another
/// sub, and the same sub in another meeting, are still admitted. Once the
/// answers arrive the slots free and the same sub is admitted again.
#[tokio::test(flavor = "current_thread")]
async fn the_pending_cap_is_per_meeting_and_sub_and_frees_when_the_bind_resolves() {
    let mut suite = Suite::start_gated(SenderReplies::default()).await;
    suite.register(MEETING).await;
    suite.register("meeting-other").await;
    let token = mint_dial_token(MEETING, "user-1", HANDLER_ID);

    let mut held = Vec::new();
    for _ in 0..CAP {
        held.push(suite.dial(&token).await);
    }
    suite.admitted(CAP).await;

    let snap = MetricAssertion::snapshot();
    let (excess, _s) = suite.dial(&token).await;
    expect_session_close(&excess, MhCloseCode::Declined, WAIT, "cap+1").await;
    suite.wt.wait_held(CAP, WAIT).await;
    assert_start_outcome_only(&snap, Some(MediaSessionStartOutcome::DeclinedPendingCap));
    assert_closed_only(&snap, CloseCode::Declined, 1);
    assert_dial_validation(&snap, None);
    snap.counter("mh_webtransport_connections_total")
        .with_labels(&[("status", "error")])
        .assert_delta(0);
    assert_eq!(
        suite.sm.active_connection_count().await,
        CAP,
        "the excess was never admitted"
    );

    // Per (meeting, sub): another sub, and the same sub in another meeting.
    let _other_sub = suite
        .dial(&mint_dial_token(MEETING, "user-2", HANDLER_ID))
        .await;
    let _other_meeting = suite
        .dial(&mint_dial_token("meeting-other", "user-1", HANDLER_ID))
        .await;
    suite.admitted(CAP + 2).await;

    // One client drops while its Connected RPC is still in flight.
    held.pop().unwrap().0.close(0u32.into(), b"");

    // MC answers (sender 0 for everyone here): every held bind resolves and
    // every slot frees — including the dropped client's.
    suite.open_gate();
    let stop = Instant::now() + Duration::from_secs(15);
    while suite.sm.active_connection_count().await != 0 {
        suite.drain_connected();
        assert!(Instant::now() < stop, "every awaiting connection resolves");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    // Re-hold, so the freed slot is observed while admitted (not racing a
    // sender-0 answer).
    suite.close_gate();
    let (again, _s) = suite.dial(&token).await;
    suite.admitted(1).await;
    expect_still_open(&again, Duration::from_millis(100), "freed slot admits").await;
}

/// The MC-ERROR exit frees the slot too: MC refuses MH's credential
/// (non-retryable), each dial is declined `declined_mc_auth_rejected`, and
/// 2×CAP sequential dials of one sub never hit `declined_pending_cap` — a
/// leaked entry would make the (CAP+1)-th one a pending-cap decline.
#[tokio::test(flavor = "current_thread")]
async fn an_mc_error_frees_the_pending_slot() {
    let mut suite = Suite::build(
        SenderReplies::default(),
        false,
        MockBehavior::Unauthenticated,
    )
    .await;
    suite.register(MEETING).await;
    let token = mint_dial_token(MEETING, "user-1", HANDLER_ID);
    let snap = MetricAssertion::snapshot();
    for i in 0..(2 * CAP) {
        let (conn, _s) = suite.dial(&token).await;
        expect_session_close(&conn, MhCloseCode::Declined, WAIT, &format!("dial {i}")).await;
        suite.wt.wait_idle(WAIT).await;
        suite.drain_connected();
    }
    snap.counter("mh_media_session_starts_total")
        .with_labels(&[
            ("outcome", "declined_mc_auth_rejected"),
            ("key_custody", "operator"),
        ])
        .assert_delta(u64::try_from(2 * CAP).unwrap());
    snap.counter("mh_media_session_starts_total")
        .with_labels(&[
            ("outcome", "declined_pending_cap"),
            ("key_custody", "operator"),
        ])
        .assert_unobserved();
    assert_eq!(suite.sm.active_connection_count().await, 0);
}

// ---------------------------------------------------------------------------
// A participant who has left (MC answers sender 0)
// ---------------------------------------------------------------------------

/// A participant who left still holds a valid token: MC answers sender 0 and
/// MH closes DECLINED. While its Connected RPC is HELD (it is admitted and
/// awaiting its bind), a publisher's frames reach a bound subscriber
/// (positive control) and NOT it.
#[tokio::test(flavor = "current_thread")]
async fn a_left_participant_is_declined_and_receives_no_frame_while_pending() {
    let replies = SenderReplies::default()
        .with("publisher", SENDER_PUBLISHER)
        .with("subscriber", SENDER_SUBSCRIBER);
    let mut suite = Suite::start(replies).await;
    suite.register(MEETING).await;
    let limits = mh_test_utils::admission::fixture_policy_limits();
    let policy = MeetingPolicy::from_request(
        &mh_test_utils::media_policy::register_request(
            MEETING,
            1,
            vec![mh_test_utils::media_policy::egress(
                1,
                SENDER_SUBSCRIBER,
                0,
                SENDER_PUBLISHER,
            )],
        ),
        &limits,
    )
    .unwrap();
    suite
        .sm
        .apply_policy(policy, limits.max_total_egress_edges, WAIT)
        .await;

    let (publisher, _ps) = suite
        .dial(&mint_dial_token(MEETING, "publisher", HANDLER_ID))
        .await;
    let (subscriber, _ss) = suite
        .dial(&mint_dial_token(MEETING, "subscriber", HANDLER_ID))
        .await;
    assert!(suite.bound_within(MEETING, SENDER_PUBLISHER).await);
    assert!(suite.bound_within(MEETING, SENDER_SUBSCRIBER).await);

    // Hold ONLY what follows: the left participant's bind.
    suite.close_gate();
    let snap = MetricAssertion::snapshot();
    let (left, _ls) = suite
        .dial(&mint_dial_token(MEETING, "left-participant", HANDLER_ID))
        .await;
    suite.admitted(3).await;

    for seq in 0..20 {
        let _ = publisher.send_datagram(audio_datagram(seq, 0x33));
    }
    let received = tokio::time::timeout(WAIT, subscriber.receive_datagram()).await;
    assert!(
        received.is_ok_and(|d| d.is_ok()),
        "positive control: the bound subscriber received while the left one was pending"
    );
    expect_still_open(&left, Duration::from_millis(100), "still awaiting its bind").await;
    assert_eq!(
        suite.sm.active_connection_count().await,
        3,
        "admitted, awaiting its bind"
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(200), left.receive_datagram())
            .await
            .is_err(),
        "the pending connection received no frame"
    );
    assert_start_outcome_only(&snap, None);

    // MC answers 0 (it has left): DECLINED.
    suite.open_gate();
    expect_session_close(&left, MhCloseCode::Declined, WAIT, "left participant").await;
    assert_start_outcome_only(
        &snap,
        Some(MediaSessionStartOutcome::DeclinedNoSenderBinding),
    );
    assert_dial_validation(&snap, None);
}

// ---------------------------------------------------------------------------
// Release while the bind is in flight (ADR-0039 §6)
// ---------------------------------------------------------------------------

/// REMOVAL cause: a connection awaiting its bind is closed HANDLER_RELEASED at
/// the release, exactly once, and nothing is bound. (The in-flight RPC is
/// abandoned with the connection; the late-ordinal case is the MEETING_ENDED
/// twin below.)
#[tokio::test(flavor = "current_thread")]
async fn a_removal_closes_an_awaiting_connection_handler_released_once_and_binds_nothing() {
    let mut suite =
        Suite::start_gated(SenderReplies::default().with("user-1", SENDER_PUBLISHER)).await;
    suite.register(MEETING).await;
    let snap = MetricAssertion::snapshot();
    let (conn, _s) = suite
        .dial(&mint_dial_token(MEETING, "user-1", HANDLER_ID))
        .await;
    suite.admitted(1).await;

    suite
        .sm
        .end_meeting(
            MEETING.to_string(),
            "mc-dial-test".to_string(),
            ReleaseKind::Removal(common::observability::labels::RemovalTrigger::HeartbeatTimeout),
            WAIT,
        )
        .await
        .unwrap();
    expect_session_close(
        &conn,
        MhCloseCode::HandlerReleased,
        WAIT,
        "removal while awaiting",
    )
    .await;
    suite.open_gate();
    suite.drain_connected();
    suite.wt.wait_idle(WAIT).await;
    assert_closed_only(&snap, CloseCode::HandlerReleased, 1);
    assert!(suite
        .sm
        .sender_bindings()
        .holder_of(
            &MeetingKey::new(MEETING),
            SenderId::from_wire(SENDER_PUBLISHER).unwrap()
        )
        .is_none());
}

/// MEETING_ENDED: the awaiting connection is left to the bind outcome; a
/// NONZERO answer for the released meeting binds nothing and closes DECLINED.
/// Control: the same answer with no release binds.
#[tokio::test(flavor = "current_thread")]
async fn meeting_ended_leaves_an_awaiting_connection_to_the_bind_and_a_late_ordinal_binds_nothing()
{
    let mut suite =
        Suite::start_gated(SenderReplies::default().with("user-1", SENDER_PUBLISHER)).await;
    suite.register(MEETING).await;
    let snap = MetricAssertion::snapshot();
    let (conn, _s) = suite
        .dial(&mint_dial_token(MEETING, "user-1", HANDLER_ID))
        .await;
    suite.admitted(1).await;
    suite
        .sm
        .end_meeting(
            MEETING.to_string(),
            "mc-dial-test".to_string(),
            ReleaseKind::MeetingEnded,
            WAIT,
        )
        .await
        .unwrap();
    expect_still_open(
        &conn,
        Duration::from_millis(200),
        "left to the bind outcome",
    )
    .await;
    suite.open_gate();
    suite.drain_connected();
    expect_session_close(
        &conn,
        MhCloseCode::Declined,
        WAIT,
        "late ordinal, meeting gone",
    )
    .await;
    suite.wt.wait_idle(WAIT).await;
    assert_closed_only(&snap, CloseCode::Declined, 1);
    assert!(suite
        .sm
        .sender_bindings()
        .holder_of(
            &MeetingKey::new(MEETING),
            SenderId::from_wire(SENDER_PUBLISHER).unwrap()
        )
        .is_none());

    // Control: no release, the same answer binds.
    suite.register(MEETING).await;
    let _conn = expect_admitted(
        &mut suite,
        &mint_dial_token(MEETING, "user-1", HANDLER_ID),
        SENDER_PUBLISHER,
        "no release",
    )
    .await;
}

/// The configured skew is published as its config gauge from the same value
/// admission reads (`main` publishes `config.media_dial_clock_skew_seconds`).
#[tokio::test(flavor = "current_thread")]
async fn the_configured_skew_is_published_as_its_config_gauge() {
    let snap = MetricAssertion::snapshot();
    mh_service::observability::metrics::publish_media_dial_clock_skew(SKEW_SECS);
    #[allow(clippy::cast_precision_loss)]
    let expected = SKEW_SECS as f64;
    snap.gauge("mh_media_dial_clock_skew_seconds")
        .with_labels(&[("key_custody", "operator")])
        .assert_value(expected);
}
