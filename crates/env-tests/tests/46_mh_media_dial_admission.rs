//! Env-test: MH admits a client ONLY on a current MC-minted `media_dial`
//! credential for that meeting and that handler (story 4 MH-1; ADR-0039 §3,
//! §5, §7; R-12, R-13, R-19, R-36).
//!
//! Every leg dials a REAL MH from a Rust WebTransport client against a LIVE
//! meeting, and every refusal is observed on the session-close capsule
//! (`fixtures::mh_close`), never on a timeout:
//!
//! - a valid MC-issued token connects, and the SAME token dials again after
//!   the first session closed (no jti cache);
//! - wrong `aud` (handler A's token dialled at handler B) closes
//!   `WRONG_HANDLER`; the PRECONDITION of two distinct handlers fails loudly,
//!   never skips;
//! - an AC meeting token presented as the dial credential closes `DECLINED`;
//! - a dial after the join JWT's lifetime would have expired: a token from a
//!   LATER `token_refresh` set (asserted to be a different string) is admitted
//!   alone AND as an OLD client sends it, with a string on the reserved
//!   `MhConnectRequest` tag 1 (the deleted `join_token`, story 4 P3) — MH
//!   decodes it as unknown and never reads it;
//! - a participant who LEFT redials with its still-valid token and is
//!   `DECLINED` (MC answers `sender_id` 0), receiving no media while a
//!   still-joined peer DOES receive in the same window;
//! - a meeting this MH no longer holds: after a POSITIVE per-meeting release
//!   signal (the wrong-owner `EndMeeting` probe answers OK, not
//!   FAILED_PRECONDITION, on every holder), ONE dial with a token that was
//!   admitted earlier in the test closes `DECLINED`.
//!
//! Tokens are bearer credentials: no panic or assert message here names one.
//!
//! No `#[serial]`: this binary calls no pool-mutating fixture.
//!
//! Edge-sum (concurrent Σ P×(N+M)): 0

#![cfg(feature = "flows")]

use std::time::{Duration, Instant};

use env_tests::cluster::ClusterConnection;
use env_tests::fixtures::gc_client::GcClient;
use env_tests::fixtures::mc_session::{self, connect_wt, JoinDeclaration, MhSession};
use env_tests::fixtures::media::{bind_until_received, drain};
use env_tests::fixtures::mh_address::{live_mh_addresses_by_pod, pods_serving};
use env_tests::fixtures::mh_close::expect_session_close;
use env_tests::fixtures::mh_grpc::{self, authed};
use env_tests::fixtures::participant::{
    self, dial_token_for, filled_senders, meeting_of, two_offered_handlers, Invited, AUDIO,
    VIEW_MARGIN,
};
use env_tests::fixtures::{AuthClient, PrometheusClient};
use proto_gen::dark_tower::internal::v1::{EndMeetingRequest, ReleaseCause};
use proto_gen::dark_tower::signaling::v1::MhCloseCode;
use tonic::Code;

/// How long a session must stay open to count as ADMITTED (MH's refusals all
/// close well inside it: verification is local and the bind answer is one RPC).
const HELD_FOR: Duration = Duration::from_millis(2500);

/// Bound on a refusal's session close.
const CLOSE_WITHIN: Duration = Duration::from_secs(10);

/// Bound on MC's teardown + MH's release after the last participant leaves
/// (covers MC's `teardown_fence_hold_max_seconds`; a failure bound only).
const RELEASE_BOUND: Duration = Duration::from_secs(150);

const MARK_Q: u8 = 0x51;

async fn cluster() -> ClusterConnection {
    let cluster = ClusterConnection::new()
        .await
        .expect("Failed to connect to cluster - ensure port-forwards are running");
    cluster.check_ac_health().await.expect("AC must be running");
    cluster.check_gc_health().await.expect("GC must be running");
    cluster
}

/// Assert the session on `conn` is held open (ADMITTED): no close within
/// [`HELD_FOR`].
async fn assert_held(conn: &wtransport::Connection, ctx: &str) {
    let closed = tokio::time::timeout(HELD_FOR, conn.closed()).await;
    assert!(
        closed.is_err(),
        "{ctx}: MH ended a session that should have been ADMITTED ({closed:?}). Read \
         mh_jwt_validations_total{{token_type=\"media_dial\"}} by failure_reason \
         (docs/runbooks/mh-incident-response.md Scenario 24)"
    );
}

/// Dial `url` with `frame` and return the live session, failing as an
/// ENVIRONMENT fact if the handler is unreachable.
async fn dial(url: &str, frame: &[u8], ctx: &str) -> MhSession {
    mc_session::try_mh_connect_frame(url, frame, None)
        .await
        .unwrap_or_else(|e| panic!("{ctx}: ENVIRONMENT — could not reach MH at {url}: {e}"))
}

#[tokio::test]
async fn valid_dial_tokens_admit_and_foreign_credentials_are_refused() {
    let cluster = cluster().await;
    let auth = AuthClient::new(&cluster.ac_base_url);
    let gc = GcClient::new(&cluster.gc_base_url);
    let ps = meeting_of(&auth, &gc, "MH Media Dial Admission", &["A"]).await;
    let a = &ps[0];
    // PRECONDITION (never a skip): the wrong-aud leg needs two DISTINCT handlers.
    let (x, y) = two_offered_handlers("ADMISSION", &ps);
    let token_x = dial_token_for("ADMISSION", a, &x);
    let token_y = dial_token_for("ADMISSION", a, &y);

    // (1) Valid token admitted; the SAME token admitted again after the first
    // session closed (reuse within the TTL, no jti cache).
    {
        let first = dial(
            &x,
            &mc_session::mh_connect_frame(token_x.expose()),
            "VALID #1",
        )
        .await;
        assert_held(first.connection(), "VALID #1").await;
    }
    let second = dial(
        &x,
        &mc_session::mh_connect_frame(token_x.expose()),
        "VALID #2",
    )
    .await;
    assert_held(second.connection(), "VALID #2 (same token, reused)").await;
    drop(second);

    // (2) Wrong aud: x's token at y -> WRONG_HANDLER. Positive control: y's
    // own token at y is admitted.
    let wrong = dial(
        &y,
        &mc_session::mh_connect_frame(token_x.expose()),
        "WRONG-AUD",
    )
    .await;
    expect_session_close(
        wrong.connection(),
        MhCloseCode::WrongHandler,
        CLOSE_WITHIN,
        "WRONG-AUD: handler x's token dialled at handler y",
    )
    .await;
    let right = dial(
        &y,
        &mc_session::mh_connect_frame(token_y.expose()),
        "RIGHT-AUD",
    )
    .await;
    assert_held(right.connection(), "RIGHT-AUD (control for WRONG-AUD)").await;
    drop(right);

    // (3) The AC meeting token as the dial credential -> DECLINED (it is
    // signed by AC, never by the meeting's dial key). Control: (1).
    let ac = dial(&x, &mc_session::mh_connect_frame(&a.token), "AC-TOKEN").await;
    expect_session_close(
        ac.connection(),
        MhCloseCode::Declined,
        CLOSE_WITHIN,
        "AC-TOKEN: an AC meeting token presented as media_dial_token",
    )
    .await;
    drop(ps);
}

#[tokio::test]
async fn a_refreshed_dial_token_is_admitted_whatever_vacated_tag_1_carries() {
    let cluster = cluster().await;
    let prom = PrometheusClient::new(&cluster.prometheus_base_url);
    let auth = AuthClient::new(&cluster.ac_base_url);
    let gc = GcClient::new(&cluster.gc_base_url);
    let mut ps = meeting_of(&auth, &gc, "MH Media Dial Refresh", &["A"]).await;
    let x = ps[0]
        .offered()
        .into_iter()
        .next()
        .expect("PRECONDITION: the meeting's handler set is empty on a healthy cluster");
    let join_time = dial_token_for("REFRESH", &ps[0], &x);
    let handler_ref = ps[0]
        .sets
        .ref_for_url(&x)
        .expect("the handler is in the current set");

    // MC re-pushes every TTL/3 (jittered); the derived interval is published.
    let refresh = prom
        .instance_counter_map("max by (instance) (mc_media_dial_token_refresh_interval_seconds)")
        .await
        .into_values()
        .fold(0.0_f64, f64::max);
    assert!(
        refresh > 0.0,
        "PRECONDITION: mc_media_dial_token_refresh_interval_seconds is not published by MC"
    );
    let bound = Duration::from_secs_f64(refresh * 2.0) + VIEW_MARGIN;
    ps[0]
        .await_handler_set("REFRESH", bound, |sets| {
            sets.dial_token(handler_ref)
                .is_some_and(|t| t.expose() != join_time.expose())
        })
        .await;
    let refreshed = dial_token_for("REFRESH", &ps[0], &x);
    assert!(
        refreshed.expose() != join_time.expose(),
        "REFRESH: the later set must carry a DIFFERENT token"
    );

    // (i) No AC credential at all.
    let bare = dial(
        &x,
        &mc_session::mh_connect_frame(refreshed.expose()),
        "REFRESH (i)",
    )
    .await;
    assert_held(bare.connection(), "REFRESH (i): refreshed token alone").await;
    drop(bare);

    // (ii) An old client's string on the vacated tag 1: unknown, ignored, not
    // rejected. A fixed sentinel, deliberately not credential-shaped.
    let frame = mc_session::mh_connect_frame_with_vacated_tag_1(
        "p3-vacated-tag-1-sentinel",
        refreshed.expose(),
    );
    let old_client = dial(&x, &frame, "REFRESH (ii)").await;
    assert_held(
        old_client.connection(),
        "REFRESH (ii): refreshed token beside a string on the vacated tag 1",
    )
    .await;
    drop(old_client);
    drop(ps);
}

#[tokio::test]
async fn a_left_participant_and_an_unheld_meeting_are_declined() {
    let cluster = cluster().await;
    let window = participant::settle_window(&cluster).await;
    let auth = AuthClient::new(&cluster.ac_base_url);
    let gc = GcClient::new(&cluster.gc_base_url);
    let invited = Invited::new(&auth, &gc, "MH Media Dial Left", 3).await;
    let p = invited.join(0, "P", &JoinDeclaration::default()).await;
    let q = invited.join(1, "Q", &JoinDeclaration::default()).await;
    let mut r = invited
        .join(2, "R", &JoinDeclaration::default().receiving(&[1, 2], &[]))
        .await;
    let meeting_id = p.gc.meeting_id.to_string();
    let owner_mc = p.gc.mc_assignment.mc_id.clone();
    let (sp, sq) = (p.sender_id(), q.sender_id());

    // All three dial ONE handler: MC fills a slot only for a sender and a
    // subscriber connected to a common handler, so R's view stays empty until
    // the media sessions exist (the first Layer-7 run converged on the view
    // BEFORE anyone dialled, and correctly saw nothing). P and Q dial with
    // SAVED tokens: P's is re-used after it leaves, Q's by the unheld leg.
    let url = r.offered().first().cloned().unwrap_or_else(|| {
        panic!("PRECONDITION: R's MediaHandlerSet is empty — no handler to dial (environment)")
    });
    let p_token = dial_token_for("LEFT", &p, &url);
    let q_token = dial_token_for("LEFT", &q, &url);
    let p_session = dial(
        &url,
        &mc_session::mh_connect_frame(p_token.expose()),
        "LEFT P",
    )
    .await;
    let q_session = dial(
        &url,
        &mc_session::mh_connect_frame(q_token.expose()),
        "LEFT Q",
    )
    .await;
    let r_session = participant::reach("LEFT", &r, &url).await;

    // R hears both P and Q: the positive starting state.
    let bound = window + VIEW_MARGIN;
    r.converge("LEFT", bound, |v, _| {
        v.is_some_and(|v| {
            let mut f = filled_senders(v, AUDIO);
            f.sort_unstable();
            let mut w = vec![sp, sq];
            w.sort_unstable();
            f == w
        })
    })
    .await;
    bind_until_received(
        "LEFT (before)",
        r_session.connection(),
        &[(q_session.connection(), MARK_Q)],
        &[],
    )
    .await;

    // P LEAVES: a clean close of its MC session, which MC treats as a
    // voluntary leave and removes from the roster AT ONCE (an abrupt drop
    // would keep P in the roster for the disconnect grace, during which MC
    // rightly binds a redial — the first local run of this leg showed exactly
    // that). Its media session goes with it. Positive signals that MC
    // processed the leave: R receives ParticipantLeft for P, and R's view
    // no longer fills a slot with P.
    let p_id = p.join.participant_id.clone();
    drop(p_session);
    env_tests::fixtures::mc_session::leave(p.session.connection());
    drop(p);
    r.await_left("LEFT", bound, &p_id).await;
    r.converge("LEFT", bound, |v, _| {
        v.is_some_and(|v| filled_senders(v, AUDIO) == vec![sq])
    })
    .await;

    // P redials with its saved, still-valid token: MC answers sender_id 0 and
    // MH closes DECLINED. In the same window Q's frames DO reach R (control)
    // and NONE reach P's connection.
    let redial = dial(
        &url,
        &mc_session::mh_connect_frame(p_token.expose()),
        "LEFT redial",
    )
    .await;
    bind_until_received(
        "LEFT (control: a joined peer receives in the same window)",
        r_session.connection(),
        &[(q_session.connection(), MARK_Q)],
        &[],
    )
    .await;
    let mut leaked = 0_u32;
    drain(redial.connection(), |_, _, _| leaked += 1).await;
    assert_eq!(
        leaked, 0,
        "LEFT: the left participant's pending connection received media"
    );
    expect_session_close(
        redial.connection(),
        MhCloseCode::Declined,
        CLOSE_WITHIN,
        "LEFT: a left participant redialling with its still-valid token",
    )
    .await;
    drop(redial);

    // Unheld meeting: everyone leaves; wait for a POSITIVE per-meeting release
    // signal on every holder (never by redialling), then ONE dial.
    let holder_pods = pods_serving(q.handler_set(), &live_mh_addresses_by_pod().await);
    assert!(
        !holder_pods.is_empty(),
        "PRECONDITION: the meeting's handler set maps to no live MH pod"
    );
    drop(q_session);
    drop(r_session);
    drop(q);
    drop(r);
    let svc = mh_grpc::mc_service_token(&cluster.ac_base_url).await;
    let holders: Vec<mh_grpc::Handler> = mh_grpc::handlers()
        .await
        .into_iter()
        .filter(|h| holder_pods.contains(&h.name))
        .collect();
    assert_eq!(holders.len(), holder_pods.len(), "every holder pod is live");
    for h in &holders {
        let deadline = Instant::now() + RELEASE_BOUND;
        loop {
            // A wrong-owner EndMeeting releases nothing: refused
            // FAILED_PRECONDITION while the meeting is held, acknowledged as a
            // no-op once it is not (see 45_mh_meeting_heartbeat.rs). Its only
            // effects: each poll while held increments
            // `mh_media_meeting_teardowns_total{outcome="rejected_ownership"}`
            // and logs the ownership WARN; nothing alerts on that MH counter
            // (precedent: 45), and 29 asserts >= baseline+1, so it tolerates it.
            let answer = mh_grpc::connect(h)
                .await
                .end_meeting(authed(
                    &svc,
                    EndMeetingRequest {
                        meeting_id: meeting_id.clone(),
                        mc_id: format!("{owner_mc}-not-the-owner"),
                        cause: ReleaseCause::MeetingEnded as i32,
                    },
                ))
                .await;
            match answer {
                Ok(_) => break,
                Err(status) if status.code() == Code::FailedPrecondition => {
                    assert!(
                        Instant::now() < deadline,
                        "UNHELD: {} still holds the meeting {RELEASE_BOUND:?} after every \
                         participant left (check mc_media_end_meeting_total{{outcome}} and \
                         mh_media_meeting_teardowns_total{{outcome}})",
                        h.name
                    );
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
                Err(status) => panic!(
                    "UNHELD: the release probe on {} failed: {:?}",
                    h.name,
                    status.code()
                ),
            }
        }
    }
    // ONE dial with Q's token, admitted earlier in this test (the control).
    let unheld = connect_wt(&url).await;
    let (_send, _recv) = mc_session::mh_open_connect(&unheld, q_token.expose()).await;
    expect_session_close(
        &unheld,
        MhCloseCode::Declined,
        CLOSE_WITHIN,
        "UNHELD: a correctly signed token for a meeting this MH no longer holds",
    )
    .await;
}
