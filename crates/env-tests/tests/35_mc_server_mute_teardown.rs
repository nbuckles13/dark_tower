//! Env-test: server mute, client mute and meeting teardown driven through MC's
//! PUBLIC APIs on the live Kind cluster (story 2 task 12; R-8, R-9, R-10,
//! R-11, R-20; story 3 MC-B: R-15, R-16, R-17, R-18, R-33; ADR-0036 §5, §7).
//!
//! Every participant joins through GC -> MC with a real meeting token (the
//! meeting's creator carrying `MeetingRole::Host`, exactly as GC stamps it) and
//! opens real MH media sessions. Nothing here programs MH directly except the
//! post-teardown PROBE in the coordination test, which is the release proof.
//!
//! # Evidence, per the env-tests README evidence rule
//!
//! Suites run in parallel against SHARED pods, so the proof is always
//! per-entity — what THIS test's own connections observed — and a counter is
//! only ever `>= own baseline + 1`, as SECONDARY evidence. No assertion reads
//! the value of a shared pod-level gauge.
//!
//! **A contradiction with the task text, resolved in the README's favour and
//! recorded here so it is not silent.** Story 2 task 12 asked for "a meeting
//! end produces a teardown observed on MH's registered-meetings gauge". That
//! gauge is POD-WIDE and other suites register and release meetings on the
//! same pods concurrently, so its value moves in both directions for reasons
//! unrelated to this test; asserting it is exactly what the README forbids
//! (and what failed test 20's Gate 2). The substitute is per-entity: a probe
//! registration of THIS meeting at generation 1 installs only if MH released
//! the meeting (`test_meeting_end_releases_every_handler_even_with_a_straggler`).
//! The gauge is checked for PRESENCE only.
//!
//! # No wall-clock gates
//!
//! Every wait is a bounded poll on an observation (a view converging, a frame
//! arriving, a connection closing, a counter rising); the bounds are rig
//! bounds for unreliable datagrams and scrape latency, never performance
//! assertions.
//!
//! # Server-muted drops are attributable because the producers are SERIALISED
//!
//! `mh_media_frames_dropped_total{reason="server_muted"}` carries no sender,
//! and test fns within one file run concurrently (files run one after
//! another). So EVERY fn here that produces `server_muted` drops (S3 audio, and
//! the story-3 VIDEO server mute with its patched publisher) holds the
//! file-level [`SERVER_MUTED_PRODUCERS`] lock for its whole body. Under that
//! lock a pinned per-pod delta of at least K — K being the label-1 uni-stream
//! frames the patched publisher sent to THAT pod while muted — is a
//! single-entity attribution. The other fns (client mute, the non-host refusal,
//! teardown) produce no `server_muted` drops and run concurrently: in
//! particular the client-mute fn's fence is an AUDIO server mute of a publisher
//! that sends no audio, so its DROP_AT_INGRESS stream receives no frames.
//!
//! The client-mute fn's `mh_media_paused_source_frames_total` delta (at least
//! K per pod) is attributable WITHOUT the lock: no other fn in this file
//! produces paused-source arrivals — the video server-mute fn never
//! client-mutes, and S3 is audio-only with no client mute.
//!
//! # What these env-tests do NOT prove (story 3 MC-B)
//!
//! The client-mute storm asserts per-entity CONVERGENCE (every holder ends on
//! the final mute state, the publisher's directive never moved). The NUMERIC
//! push bound (at most one `RegisterMeeting` in flight and one pending per
//! handler) is MC-internal and pod-wide, so it is proven in-process by
//! `crates/mc-service/tests/server_mute_integration.rs::a_client_mute_storm_is_bounded_by_one_push_in_flight_and_one_pending`,
//! never read off a shared counter here.
//!
//! # Edge sum (README per-file edge-sum header rule)
//!
//! Declared receive slots per fn: S3 3 x 2 = 6 (audio); the non-host refusal
//! declares nothing (0); TEARDOWN 2 + 1 + 1 = 4 (audio); VIDEO SERVER MUTE: S
//! declares 2 audio + 2 video = 4 (H, P, C declare none); CLIENT VIDEO MUTE: S1
//! 1 + 1 and S2 1 + 1 = 4 (P declares none).
//!
//! S3 and VIDEO SERVER MUTE hold [`SERVER_MUTED_PRODUCERS`], so they never run
//! together: that serialised group counts as its larger member, max(6, 4) = 6.
//! Concurrent sum: 6 + 0 + 4 + 4 = 14, within the file budget of 46.
//!
//! Edge-sum (concurrent Σ P×(N+M)): 14

#![cfg(feature = "flows")]

use std::collections::{BTreeSet, HashMap};
use std::time::{Duration, Instant};

use env_tests::cluster::ClusterConnection;
use env_tests::fixtures::gc_client::GcClient;
use env_tests::fixtures::mc_session::JoinDeclaration;
use env_tests::fixtures::mc_session::{self, MhSession};
use env_tests::fixtures::media::{
    assert_relayed_on_slot, audio_datagram, bind_until_received, drain, AUDIO_SEND_ID,
};
use env_tests::fixtures::metrics::{
    format_instance_map, gauge_by_instance_present, instance_for_pod_ip, paused_promql,
    poll_until_any_instance_above, poll_until_pinned_instance, reason_promql, settled_baseline,
    PrometheusClient, PINNED_METRIC_BOUND,
};
use env_tests::fixtures::mh_grpc::{self, authed, probe_registration, ReleaseOnDrop};
use env_tests::fixtures::participant::{
    self, filled_senders, meeting_of, open_sessions, slots_of, two_offered_handlers, Invited,
    Participant, TRIAGE_MH_REACH, VIDEO,
};
use env_tests::fixtures::video::{
    key_id_sender, BackgroundPublisher, FrameOutcome, GroupStartSource, Next, PublisherEvent,
    PublisherMode, ReceivedFrame, SubscriberEvent, UplinkLabel, VideoPublisher,
    VideoPublisherConfig, VideoSubscriber, VideoTransport,
};
use env_tests::fixtures::AuthClient;
use proto_gen::dark_tower::internal::v1::EndMeetingRequest;
use proto_gen::dark_tower::signaling::v1::{
    client_message, server_message, ClientMessage, MediaKind, MuteRequest, ParticipantMuteUpdate,
    SendDirective, SendStream, ServerMuteRequest, SlotState, StreamAssignment, StreamAssignments,
};

async fn cluster() -> ClusterConnection {
    let cluster = ClusterConnection::new()
        .await
        .expect("Failed to connect to cluster - ensure port-forwards are running");
    cluster.check_ac_health().await.expect("AC must be running");
    cluster.check_gc_health().await.expect("GC must be running");
    cluster
}

use env_tests::fixtures::participant::AUDIO;
/// Margin over MC's settle window (one shared home).
use env_tests::fixtures::participant::VIEW_MARGIN;

/// Rig bound on one send-until-observed phase (unreliable datagrams).
const PHASE_DEADLINE: Duration = Duration::from_secs(45);

/// Consecutive send rounds, with the muted sender transmitting on every one,
/// over which no receiver may hear it: the "goes flat" observation. Counted in
/// rounds of real traffic, not in elapsed time.
const FLAT_ROUNDS: u32 = 50;

/// Rig bound on a Prometheus counter becoming visible past its baseline.
const COUNTER_BUDGET: Duration = Duration::from_secs(120);

/// Rig bound on the straggler's MH connections closing after the last
/// participant leaves. It must cover MC's teardown worst case — read it off
/// MC's startup line as `teardown_fence_hold_max_seconds` — plus MH's release
/// and scheduling. Generous on purpose: a slow teardown is not what this test
/// measures, and the keep-alive (below) means a close inside it cannot be an
/// idle timeout.
const CLOSE_BOUND: Duration = Duration::from_secs(150);

/// Keep-alive for the straggler's MH sessions, so their only way to end inside
/// [`CLOSE_BOUND`] is the PEER closing them.
const STRAGGLER_KEEP_ALIVE: Duration = Duration::from_secs(5);

const MARK_H: u8 = 0x31;
const MARK_A: u8 = 0x3A;
const MARK_B: u8 = 0x3B;
const MARK_C: u8 = 0x3C;

/// Sequence floors separating frames by phase.
const SEQ_MUTED: u32 = 1_000_000;
// The shared binding sequence (`next_audio_sequence`) must stay below every
// hand-rolled phase floor, or a sender's GoP counter could regress.
const _: () = assert!(SEQ_MUTED >= env_tests::fixtures::media::AUDIO_SEQUENCE_CEILING);
const SEQ_UNMUTED: u32 = 2_000_000;

/// Every fn producing `server_muted` drops holds this for its whole body (see
/// the module doc): the per-pod drop delta is then attributable to one fn.
static SERVER_MUTED_PRODUCERS: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// A `ServerMuteRequest` about `target`, per kind: `None` = that kind absent
/// on the wire, i.e. UNCHANGED (story 3 MC-B). Send only the kind under test.
fn server_mute(
    target: &str,
    audio_muted: Option<bool>,
    video_muted: Option<bool>,
) -> ClientMessage {
    ClientMessage {
        message: Some(client_message::Message::ServerMuteRequest(
            ServerMuteRequest {
                participant_id: target.to_string(),
                audio_muted,
                video_muted,
                reason: String::new(),
            },
        )),
        trace_parent: String::new(),
        trace_state: String::new(),
    }
}

fn server_mute_requests(outcome: &str) -> String {
    format!(
        "sum by (instance) (mc_media_server_mute_requests_total{{action=\"mute\",outcome=\"{outcome}\"}})"
    )
}

const SERVER_MUTED_DROPS: &str =
    r#"sum by (instance) (mh_media_frames_dropped_total{reason="server_muted"})"#;
const MC_RELEASED: &str = r#"sum by (instance) (mc_media_end_meeting_total{outcome="released"})"#;
const MH_RELEASED: &str =
    r#"sum by (instance) (mh_media_meeting_teardowns_total{outcome="released"})"#;

// ============================================================================
// S3 (server half) + unmute: a host's server mute is enforced at MH ingress
// ============================================================================

/// A host server-mutes A through MC's public API: every other receiver stops
/// hearing A while A keeps transmitting and keeps hearing others (the mute is
/// SOURCE-only); an authorized unmute restores A on the receiver's own slot.
///
/// # Evidence
///
/// - PRIMARY, per receiver: over [`FLAT_ROUNDS`] consecutive rounds in which A
///   transmits on every one of its sessions, neither B nor the host receives an
///   A frame — while B keeps hearing the host (B's receive path is alive) and A
///   keeps hearing B (A is still connected and sending, not silenced
///   client-side).
/// - Positive control BEFORE the mute: B hears A on B's own slot for A.
/// - SECONDARY: `mh_media_frames_dropped_total{reason="server_muted"}` rises past
///   its own baseline on some MH pod (within this file the producers are
///   serialised; env-test 26's S4 produces the same series, in another file).
#[tokio::test]
async fn test_host_server_mute_is_enforced_at_mh_ingress_and_unmute_restores_it() {
    let _serialised = SERVER_MUTED_PRODUCERS.lock().await;
    let cluster = cluster().await;
    let prom = PrometheusClient::new(&cluster.prometheus_base_url);
    let window = participant::settle_window(&cluster).await;
    let auth = AuthClient::new(&cluster.ac_base_url);
    let gc = GcClient::new(&cluster.gc_base_url);
    // H creates the meeting, so GC stamps H's token Host.
    let mut ps = meeting_of(&auth, &gc, "Server Mute S3", &["H", "A", "B"]).await;
    let (x, y) = two_offered_handlers("S3", &ps);
    let (h, a, b) = (ps[0].sender_id(), ps[1].sender_id(), ps[2].sender_id());

    let sessions = open_sessions("S3", &ps, &[&x, &y]).await;
    ps[0].declare(&[1, 2], &[]).await;
    ps[1].declare(&[1, 2], &[]).await;
    ps[2].declare(&[1, 2], &[]).await;
    let bound = window + VIEW_MARGIN;
    for (i, want) in [(0, [a, b]), (1, [h, b]), (2, [h, a])] {
        ps[i]
            .converge("S3", bound, |v, d| {
                v.is_some_and(|v| {
                    let mut f = filled_senders(v, AUDIO);
                    f.sort_unstable();
                    let mut w = want.to_vec();
                    w.sort_unstable();
                    f == w
                }) && d.is_some()
            })
            .await;
    }

    // PRE: B hears A on B's own slot for A.
    let b_edge_url = ps[2].slot_url(a, AUDIO);
    let pre = bind_until_received(
        "S3 PRE",
        sessions[2][&b_edge_url].connection(),
        &[(sessions[1][&b_edge_url].connection(), MARK_A)],
        &[MARK_H],
    )
    .await;
    assert_relayed_on_slot(
        "S3 PRE",
        &pre[&MARK_A],
        ps[2].slot_for(a, AUDIO),
        MARK_A,
        "A",
    );

    // MUTE through MC's public API, by the host.
    let drops_before = settled_baseline(
        &prom,
        SERVER_MUTED_DROPS,
        COUNTER_BUDGET,
        "the assertion under test",
    )
    .await;
    let a_id = ps[1].join.participant_id.clone();
    ps[0]
        .session
        .write(&server_mute(&a_id, Some(true), None))
        .await;
    // MC applied it: B's own view shows A's audio slot as the server-muted
    // avatar (story 3 flat vocabulary; SOURCE_MUTED is no longer emitted), still
    // naming A and still on the edge's handler (lead ruling 2026-10-22: a mute
    // avatar keeps its edge, so the url does not move).
    ps[2]
        .converge("S3 MUTE-VISIBLE", bound, |v, _| {
            v.is_some_and(|v| {
                v.assignments.iter().any(|s| {
                    s.media_kind == AUDIO as i32
                        && s.sender_id == Some(a)
                        && s.slot_state == SlotState::AvatarServerMuted as i32
                })
            })
        })
        .await;
    assert_eq!(
        ps[2].slot_url(a, AUDIO),
        b_edge_url,
        "S3 MUTE-VISIBLE: the server-muted avatar keeps the edge's handler url"
    );

    // MUTED: A transmits on every session every round. B and H go FLAT for A;
    // A still hears B; B still hears H.
    let conns = |i: usize| -> Vec<&wtransport::Connection> {
        [&x, &y]
            .iter()
            .map(|u| sessions[i][*u].connection())
            .collect()
    };
    let (h_conns, a_conns, b_conns) = (conns(0), conns(1), conns(2));
    let deadline = Instant::now() + PHASE_DEADLINE;
    let (mut flat, mut a_heard_b, mut b_heard_h) = (0_u32, false, false);
    let mut seq = SEQ_MUTED;
    while flat < FLAT_ROUNDS || !a_heard_b || !b_heard_h {
        assert!(
            Instant::now() < deadline,
            "S3 MUTED: within {PHASE_DEADLINE:?} A's frames did not go flat at every other \
             receiver (flat rounds {flat}/{FLAT_ROUNDS}), or a liveness control never held (A \
             heard B: {a_heard_b}, B heard H: {b_heard_h}). A frames still reaching B/H after \
             MC showed AVATAR_SERVER_MUTED means MH is not enforcing the mute: check \
             mh_media_frames_dropped_total{{reason=\"server_muted\"}} and \
             mc_media_policy_pushes_total{{outcome}}"
        );
        seq += 1;
        for (conns, mark) in [(&a_conns, MARK_A), (&b_conns, MARK_B), (&h_conns, MARK_H)] {
            for conn in conns {
                conn.send_datagram(audio_datagram(seq, mark))
                    .unwrap_or_else(|e| panic!("S3 MUTED: sender {mark:#04x} could not send: {e}"));
            }
        }
        let mut a_reached = false;
        for conn in b_conns.iter().chain(&h_conns) {
            drain(conn, |mark, _, _| {
                if mark == MARK_A {
                    a_reached = true;
                }
                if mark == MARK_H {
                    b_heard_h = true;
                }
            })
            .await;
        }
        for conn in &a_conns {
            drain(conn, |mark, _, _| {
                if mark == MARK_B {
                    a_heard_b = true;
                }
            })
            .await;
        }
        // An A frame may still arrive while MH installs the new generation;
        // FLAT is counted from the last one, never across it.
        flat = if a_reached { 0 } else { flat + 1 };
    }

    // SECONDARY: the drop path ran on some pod.
    poll_until_any_instance_above(
        &prom,
        SERVER_MUTED_DROPS,
        &drops_before,
        COUNTER_BUDGET,
        Duration::from_secs(5),
        |current| {
            format!(
                "S3 DROPS (secondary): server_muted drops rose on no MH pod (baseline {}, now {})",
                format_instance_map(&drops_before),
                format_instance_map(current)
            )
        },
    )
    .await;

    // UNMUTE, by the host: B's view flips back, then A's frames land on B's
    // slot for A again — within the next generation MC pushes.
    ps[0]
        .session
        .write(&server_mute(&a_id, Some(false), None))
        .await;
    ps[2]
        .converge("S3 UNMUTE-VISIBLE", bound, |v, _| {
            v.is_some() && slot_state_of_view(v, a) == Some(SlotState::Active as i32)
        })
        .await;
    let deadline = Instant::now() + PHASE_DEADLINE;
    let mut resumed = None;
    let mut seq = SEQ_UNMUTED;
    while resumed.is_none() {
        assert!(
            Instant::now() < deadline,
            "S3 UNMUTED: B did not hear A within {PHASE_DEADLINE:?} of MC showing A active again"
        );
        seq += 1;
        for conn in &a_conns {
            conn.send_datagram(audio_datagram(seq, MARK_A))
                .unwrap_or_else(|e| panic!("S3 UNMUTED: A could not send: {e}"));
        }
        for conn in &b_conns {
            drain(conn, |mark, s, slot| {
                if mark == MARK_A && s > SEQ_UNMUTED {
                    resumed = Some(slot);
                }
            })
            .await;
        }
    }
    assert_eq!(
        resumed,
        Some(ps[2].slot_for(a, AUDIO)),
        "S3 UNMUTED: A's frames land on B's own slot for A"
    );
    drop(sessions);
}

/// The slot state a view shows for `sender`.
fn slot_state_of_view(
    v: Option<&proto_gen::dark_tower::signaling::v1::StreamAssignments>,
    sender: u32,
) -> Option<i32> {
    v?.assignments
        .iter()
        .find(|a| a.media_kind == AUDIO as i32 && a.sender_id == Some(sender))
        .map(|a| a.slot_state)
}

// ============================================================================
// R-8: a non-host is refused, generically, and counted
// ============================================================================

#[tokio::test]
async fn test_non_host_server_mute_is_refused_and_counted() {
    let cluster = cluster().await;
    let prom = PrometheusClient::new(&cluster.prometheus_base_url);
    let auth = AuthClient::new(&cluster.ac_base_url);
    let gc = GcClient::new(&cluster.gc_base_url);
    let mut ps = meeting_of(&auth, &gc, "Server Mute Non-Host", &["H", "A"]).await;

    let before = settled_baseline(
        &prom,
        &server_mute_requests("not_permitted"),
        COUNTER_BUDGET,
        "the assertion under test",
    )
    .await;
    let h_id = ps[0].join.participant_id.clone();
    ps[1]
        .session
        .write(&server_mute(&h_id, Some(true), None))
        .await;

    // A's OWN connection receives the one generic refusal.
    let deadline = Instant::now() + PHASE_DEADLINE;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(
            !remaining.is_zero(),
            "NON-HOST: A's server-mute request drew no refusal within {PHASE_DEADLINE:?}"
        );
        match ps[1]
            .session
            .try_read(remaining)
            .await
            .and_then(|m| m.message)
        {
            Some(server_message::Message::Error(e)) => {
                assert_eq!(
                    e.message, "Server mute request refused",
                    "NON-HOST: the refusal is the one generic message"
                );
                break;
            }
            Some(server_message::Message::ParticipantMuteUpdate(u)) => {
                panic!("NON-HOST: a non-host's request was APPLIED: {u:?}")
            }
            _ => {}
        }
    }

    // Counted (>= own baseline + 1).
    poll_until_any_instance_above(
        &prom,
        &server_mute_requests("not_permitted"),
        &before,
        COUNTER_BUDGET,
        Duration::from_secs(5),
        |current| {
            format!(
                "NON-HOST: the refusal was not counted as not_permitted on any MC (baseline {}, \
                 now {})",
                format_instance_map(&before),
                format_instance_map(current)
            )
        },
    )
    .await;
}

// ============================================================================
// R-20: meeting end releases every handler — even one a straggler still holds
// ============================================================================

/// Open a media session to `url` that cannot idle out (see
/// [`STRAGGLER_KEEP_ALIVE`]), failing as an ENVIRONMENT fact.
async fn reach_kept_alive(phase: &str, url: &str, token: &str) -> MhSession {
    mc_session::try_mh_connect_with(url, token, Some(STRAGGLER_KEEP_ALIVE))
        .await
        .unwrap_or_else(|e| {
            panic!(
                "{TRIAGE_MH_REACH}: {phase} — ENVIRONMENT, not a teardown defect: the straggler \
                 could not open a media session to {url}: {e}"
            )
        })
}

/// Participants join through GC/MC and the meeting's handlers are each
/// programmed to generation >= 2; then every participant leaves through MC —
/// but one STRAGGLER keeps its MH media sessions open (the crashed-tab case).
///
/// (a) MH closes the straggler's connection on EACH handler — the release
///     reached every handler, including ones a client still holds.
/// (b) Only then, for each handler, a probe registration of THIS meeting id at
///     generation 1, as a separate MC principal, returns
///     `applied_generation == 1` — possible only if the real MC's `EndMeeting`
///     released it.
///
/// # Why each handler is held at generation >= 2 — by construction
///
/// A generation-1 probe against a meeting HELD at generation 1 is ambiguous
/// (an equal generation is a no-op that also echoes 1). So the edges are laid
/// out so BOTH handlers carry edges: the straggler S reaches both handlers, B
/// only `x`, C only `y`, so S<->B sits on `x` and S<->C on `y`. Every handler's
/// first registration was the empty first-join snapshot (generation 1); an
/// edge on it is a later snapshot, generation >= 2. Each edge is CONFIRMED
/// through a participant's own `StreamAssignments` — no generation number is
/// read, and nothing shared is.
///
/// # Never probe a live meeting
///
/// Any validation-passing registration takes the meeting over (MH rebinds
/// ownership, by design, for failover). So no probe runs until (a) has shown
/// MH released the meeting on that handler, and each probe is released under
/// its own `mc_id`, on every exit path (`ReleaseOnDrop`).
#[tokio::test]
async fn test_meeting_end_releases_every_handler_even_with_a_straggler() {
    let cluster = cluster().await;
    let prom = PrometheusClient::new(&cluster.prometheus_base_url);
    let window = participant::settle_window(&cluster).await;
    let auth = AuthClient::new(&cluster.ac_base_url);
    let gc = GcClient::new(&cluster.gc_base_url);
    let mut ps = meeting_of(&auth, &gc, "Teardown Straggler", &["S", "B", "C"]).await;
    let (x, y) = two_offered_handlers("TEARDOWN", &ps);
    let (b, c) = (ps[1].sender_id(), ps[2].sender_id());
    let meeting_id = ps[0].gc.meeting_id.to_string();

    // GAUGE PRESENCE only (never its value — see the module docs).
    gauge_by_instance_present(&prom, "mh_media_registered_meetings", 2, COUNTER_BUDGET).await;

    // S keeps its sessions alive; B and C use ordinary ones.
    let s_x = reach_kept_alive("TEARDOWN", &x, &ps[0].token).await;
    let s_y = reach_kept_alive("TEARDOWN", &y, &ps[0].token).await;
    let b_x = participant::reach("TEARDOWN", "B", &x, &ps[1].token).await;
    let c_y = participant::reach("TEARDOWN", "C", &y, &ps[2].token).await;
    ps[0].declare(&[4, 5], &[]).await;
    ps[1].declare(&[6], &[]).await;
    ps[2].declare(&[7], &[]).await;
    let bound = window + VIEW_MARGIN;
    ps[0]
        .converge("TEARDOWN", bound, |v, _| {
            v.is_some_and(|v| {
                let mut f = filled_senders(v, AUDIO);
                f.sort_unstable();
                let mut w = vec![b, c];
                w.sort_unstable();
                f == w
            })
        })
        .await;
    // Structural changes CONFIRMED on both handlers: S's own view places its
    // edge from B on x and its edge from C on y.
    assert_eq!(ps[0].slot_url(b, AUDIO), x, "TEARDOWN: S<->B sits on x");
    assert_eq!(ps[0].slot_url(c, AUDIO), y, "TEARDOWN: S<->C sits on y");

    // POSITIVE CONTROL that the straggler's sessions are ALIVE right before the
    // teardown: frames reach S on both.
    bind_until_received(
        "TEARDOWN LIVE x",
        s_x.connection(),
        &[(b_x.connection(), MARK_B)],
        &[],
    )
    .await;
    bind_until_received(
        "TEARDOWN LIVE y",
        s_y.connection(),
        &[(c_y.connection(), MARK_C)],
        &[],
    )
    .await;

    let mc_before = settled_baseline(
        &prom,
        MC_RELEASED,
        COUNTER_BUDGET,
        "the assertion under test",
    )
    .await;
    let mh_before = settled_baseline(
        &prom,
        MH_RELEASED,
        COUNTER_BUDGET,
        "the assertion under test",
    )
    .await;

    // Everyone leaves through MC. B and C also drop their media sessions; S
    // leaves MC LAST and keeps its media sessions (the crashed tab).
    let mut leaving = ps.split_off(1);
    drop(b_x);
    drop(c_y);
    leaving.clear();
    let straggler = ps.pop().expect("S");
    drop(straggler);

    // (a) MH closes the straggler's connection on EACH handler. With the
    // keep-alive, the only way these end inside the bound is the PEER closing
    // them; MH closes a released meeting's connections by dropping them, with
    // no reason string, so the close is classified by KIND, never by text.
    let mut closed = HashMap::new();
    for (name, session) in [("x", &s_x), ("y", &s_y)] {
        let why = tokio::time::timeout(CLOSE_BOUND, session.connection().closed())
            .await
            .unwrap_or_else(|_| {
                panic!(
                    "TEARDOWN (a): MH did not close the straggler's connection on handler {name} \
                     within {CLOSE_BOUND:?} of the last participant leaving MC. Check \
                     mc_media_end_meeting_total{{outcome}} and mc_media_push_quiesce_total on MC \
                     and mh_media_meeting_teardowns_total{{outcome}} on the MHs"
                )
            });
        assert!(
            matches!(
                why,
                wtransport::error::ConnectionError::ApplicationClosed(_)
                    | wtransport::error::ConnectionError::ConnectionClosed(_)
            ),
            "TEARDOWN (a): the straggler's connection on {name} ended by {why:?}, not by the PEER \
             closing it — with a {STRAGGLER_KEEP_ALIVE:?} keep-alive an idle timeout cannot be \
             the teardown"
        );
        closed.insert(name, why);
    }

    // (b) Only now: ONE generation-1 probe per handler, as a separate MC
    // principal, with no edges.
    let token = mh_grpc::mc_service_token(&cluster.ac_base_url).await;
    let cleanup = ReleaseOnDrop::new(token.clone());
    let probe_mc = format!("env-test-35-probe-{}", uuid::Uuid::new_v4().simple());
    for handler in mh_grpc::handlers() {
        let name = handler.name.clone();
        let mut client = mh_grpc::connect(&handler).await;
        cleanup.record(&handler.grpc_url, &meeting_id, &probe_mc);
        let reply = client
            .register_meeting(authed(
                &token,
                probe_registration(&meeting_id, &probe_mc, 1),
            ))
            .await
            .unwrap_or_else(|s| {
                panic!(
                    "PROBE on {name}: registration refused ({:?}: {})",
                    s.code(),
                    s.message()
                )
            })
            .into_inner();
        // The two wrong answers mean OPPOSITE things, so they fail separately.
        assert_ne!(
            reply.applied_generation, 0,
            "BACK-PRESSURE on {name}: the generation-1 probe echoed 0 — MH installed nothing. On a \
             busy shared pod that is config-apply back-pressure (MH WARNs \
             reason=config_apply_mailbox_full or config_apply_timeout), NOT a teardown fault. \
             Re-run; if it persists, read MH's config-apply WARNs on this pod."
        );
        assert_eq!(
            reply.applied_generation, 1,
            "RELEASE-NOT-PROVEN on {name}: a generation-1 probe of meeting {meeting_id} echoed {} — \
             MH still holds the meeting at the generation the real MC programmed (>= 2 by \
             construction), so MC's EndMeeting did not release it on this handler",
            reply.applied_generation
        );
        let released = client
            .end_meeting(authed(
                &token,
                EndMeetingRequest {
                    meeting_id: meeting_id.clone(),
                    mc_id: probe_mc.clone(),
                },
            ))
            .await
            .unwrap_or_else(|s| panic!("PROBE CLEANUP on {name}: {:?}: {}", s.code(), s.message()))
            .into_inner();
        assert!(
            released.acknowledged,
            "PROBE CLEANUP on {name}: not acknowledged"
        );
        cleanup.forget(&meeting_id);
    }

    // SECONDARY, >= own baseline + 1: MC recorded releases, and some MH counted
    // one. Never attributed to this test alone.
    poll_until_any_instance_above(
        &prom,
        MC_RELEASED,
        &mc_before,
        COUNTER_BUDGET,
        Duration::from_secs(5),
        |current| {
            format!(
                "TEARDOWN (secondary): mc_media_end_meeting_total{{outcome=\"released\"}} rose on \
                 no MC (baseline {}, now {})",
                format_instance_map(&mc_before),
                format_instance_map(current)
            )
        },
    )
    .await;
    poll_until_any_instance_above(
        &prom,
        MH_RELEASED,
        &mh_before,
        COUNTER_BUDGET,
        Duration::from_secs(5),
        |current| {
            format!(
                "TEARDOWN (secondary): mh_media_meeting_teardowns_total{{outcome=\"released\"}} \
                 rose on no MH (baseline {}, now {})",
                format_instance_map(&mh_before),
                format_instance_map(current)
            )
        },
    )
    .await;
    drop((s_x, s_y, closed));
}

// ============================================================================
// Story 3 MC-B: per-kind server mute (R-15, R-16, R-18) and client mute (R-17)
// ============================================================================

/// Marker of P's GENUINE audio datagrams (a marker-only payload, so never
/// confused with a video frame, whose payload leads with a key id).
const MARK_P: u8 = 0x50;

/// Rounds the patched publisher transmits in one measured mute window.
const WINDOW_ROUNDS: u32 = 30;

/// Per-read bound inside a round (a drain slice, never a pass condition).
const READ_SLICE: Duration = Duration::from_millis(30);

/// At most this many events per subscriber per round, so a drain on a
/// continuously-fed subscriber always returns to the round's checks.
const EVENTS_PER_ROUND: usize = 64;

/// The fixture group cadence (fixture-only; never a production keyframe
/// interval) — short, so a re-forwarded edge waits at most one group.
const GROUP_INTERVAL: Duration = Duration::from_millis(250);

/// Rig bound on a video frame arriving on its slot (failure bound only).
const FRAME_DEADLINE: Duration = Duration::from_secs(45);

/// A participant's MC-signalling mute request, per kind (`None` = absent).
fn mute_request(audio_muted: Option<bool>, video_muted: Option<bool>) -> ClientMessage {
    ClientMessage {
        message: Some(client_message::Message::MuteRequest(MuteRequest {
            audio_muted,
            video_muted,
        })),
        trace_parent: String::new(),
        trace_state: String::new(),
    }
}

/// `d`'s stream of `kind` (a published kind is always LISTED, R-8).
fn stream_of(d: &SendDirective, kind: MediaKind) -> &SendStream {
    d.streams
        .iter()
        .find(|s| s.media_kind == kind as i32)
        .unwrap_or_else(|| panic!("the directive lists no {} stream", kind.as_str_name()))
}

/// The target urls of `d`'s `kind` stream.
fn kind_targets(d: &SendDirective, kind: MediaKind) -> BTreeSet<String> {
    stream_of(d, kind)
        .targets
        .iter()
        .map(|t| t.media_handler_url.clone())
        .collect()
}

/// `view`'s `kind` slots, owned (for byte-identity comparisons).
fn kind_slots(view: &StreamAssignments, kind: MediaKind) -> Vec<StreamAssignment> {
    slots_of(view, kind).into_iter().cloned().collect()
}

/// The state of `view`'s `kind` slot naming `sender`, if any.
fn named_state(view: &StreamAssignments, kind: MediaKind, sender: u32) -> Option<i32> {
    slots_of(view, kind)
        .into_iter()
        .find(|a| a.sender_id == Some(sender))
        .map(|a| a.slot_state)
}

/// Open a media session to EVERY handler `p` was offered.
async fn reach_both(phase: &str, p: &Participant) -> HashMap<String, MhSession> {
    let mut out = HashMap::new();
    for url in p.offered() {
        out.insert(
            url.clone(),
            participant::reach(phase, p.label, &url, &p.token).await,
        );
    }
    out
}

/// R-18 on the TARGET's own ordered signalling path: read `p`'s messages until
/// a `SendDirective` satisfying `is_change` arrives, requiring that a
/// `ParticipantMuteUpdate` about `p` satisfying `is_update` arrived STRICTLY
/// before it. Keeps `p`'s latest view/directive current. Returns the directive.
async fn update_before_directive(
    p: &mut Participant,
    phase: &str,
    bound: Duration,
    is_update: impl Fn(&ParticipantMuteUpdate) -> bool,
    is_change: impl Fn(&SendDirective) -> bool,
) -> SendDirective {
    let own = p.join.participant_id.clone();
    let deadline = Instant::now() + bound;
    let mut update_seen = false;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(
            !remaining.is_zero(),
            "{phase}: {}'s directive change never arrived within {bound:?}",
            p.label
        );
        match p.session.try_read(remaining).await.and_then(|m| m.message) {
            Some(server_message::Message::ParticipantMuteUpdate(u))
                if u.participant_id == own && is_update(&u) =>
            {
                update_seen = true;
            }
            Some(server_message::Message::SendDirective(d)) => {
                p.directive = Some(d.clone());
                if is_change(&d) {
                    assert!(
                        update_seen,
                        "{phase}: R-18 — {}'s directive change arrived BEFORE its own \
                         ParticipantMuteUpdate on the same ordered signalling stream",
                        p.label
                    );
                    return d;
                }
            }
            Some(server_message::Message::StreamAssignments(a)) => p.view = Some(a),
            Some(server_message::Message::Error(e)) => {
                panic!("{phase}: MC sent {} an error: {}", p.label, e.message)
            }
            _ => {}
        }
    }
}

/// One patched (or compliant) video publisher per MH session.
fn video_publishers(
    sessions: &HashMap<String, MhSession>,
    cfg: VideoPublisherConfig,
) -> Vec<VideoPublisher<wtransport::Connection>> {
    sessions
        .values()
        .map(|s| VideoPublisher::new(s.connection().clone(), cfg))
        .collect()
}

/// One frame tick on every publisher; returns how many frames EACH sent
/// (`Sent` outcomes, per publisher, in order).
async fn pump(pubs: &mut [VideoPublisher<wtransport::Connection>]) -> Vec<u64> {
    let mut sent = Vec::with_capacity(pubs.len());
    for publisher in pubs.iter_mut() {
        publisher.poll();
        let n = u64::from(matches!(
            publisher.send_frame().await,
            FrameOutcome::Sent { .. }
        ));
        sent.push(n);
    }
    sent
}

/// The newest group counter any of `pubs` has started (None before the first).
fn newest_group(pubs: &[VideoPublisher<wtransport::Connection>]) -> Option<u32> {
    pubs.iter()
        .flat_map(|p| p.events().iter())
        .filter_map(|e| match e {
            PublisherEvent::GroupStart { gop_counter, .. } => Some(*gop_counter),
            _ => None,
        })
        .max()
}

/// Every video frame the subscribers report this round (bounded per round).
async fn video_frames(subs: &mut [VideoSubscriber]) -> Vec<ReceivedFrame> {
    let mut out = Vec::new();
    for sub in subs.iter_mut() {
        for _ in 0..EVENTS_PER_ROUND {
            match sub.next(READ_SLICE).await {
                Next::Quiet => break,
                Next::Event(SubscriberEvent::Frame { frame, .. }) => out.push(frame),
                Next::Event(SubscriberEvent::ConnectionClosed) => {
                    panic!("a subscriber's MH connection closed mid-test")
                }
                Next::Event(_) => {}
            }
        }
    }
    out
}

/// Every datagram queued on `conn`, as `(relay slot, stream sequence, payload)`.
async fn datagrams(conn: &wtransport::Connection) -> Vec<(u32, u32, Vec<u8>)> {
    let mut out = Vec::new();
    while let Ok(Ok(d)) = tokio::time::timeout(READ_SLICE, conn.receive_datagram()).await {
        let raw = d.payload();
        let view = media_protocol::codec::decode_datagram(&raw)
            .unwrap_or_else(|_| panic!("received a malformed datagram"));
        out.push((
            u32::from(view.stream_id()),
            view.stream_sequence(),
            view.payload().to_vec(),
        ));
    }
    out
}

/// Pump `pubs` until `sub` relays a frame from `sender` onto `slot` whose group
/// counter is above `after` (so it was SENT after the phase began). Bounded by
/// [`FRAME_DEADLINE`] (a failure bound).
async fn until_fresh_video_on_slot(
    phase: &str,
    pubs: &mut [VideoPublisher<wtransport::Connection>],
    subs: &mut [VideoSubscriber],
    slot: u32,
    sender: u32,
    after: Option<u32>,
) {
    let deadline = Instant::now() + FRAME_DEADLINE;
    loop {
        assert!(
            Instant::now() < deadline,
            "{phase}: no fresh video frame from the publisher reached slot {slot} within \
             {FRAME_DEADLINE:?}"
        );
        let _ = pump(pubs).await;
        if video_frames(subs).await.iter().any(|f| {
            u32::from(f.slot_id) == slot
                && u32::from(f.key_id_sender) == sender
                && after.is_none_or(|g| f.gop_counter > g)
        }) {
            return;
        }
    }
}

/// R-15/R-16/R-18 (VIDEO SERVER MUTE): a host server-mutes publisher P's VIDEO
/// only. Per-entity evidence on this test's own connections throughout.
///
/// Participants (join order P, C, S, H; H created the meeting, so GC stamps
/// H's token Host): P and C publish audio and video and declare nothing; S
/// declares two audio slots and two video slots, so by R-3 S holds P and C on
/// both kinds (audio by name; live video proven by frames — MC names no live
/// video source, R-5); H publishes audio only and holds nothing.
///
/// 1. MUTE: P's OWN directive lists video with EMPTY targets while its audio
///    stream is byte-identical, and P's self `ParticipantMuteUpdate` arrives
///    BEFORE it (R-18, mute direction). S's video slot for P reads
///    `AVATAR_SERVER_MUTED`; S's audio slots are byte-identical.
/// 2. PATCHED P keeps sending (`KeepSendingWhileMuted`), per variant, each
///    with the pre-control (P's video reached S before the mute), a liveness
///    control (C's video keeps reaching S in the same window) and a window
///    bounded by TRAFFIC (rounds, then a C frame arrives after P's last):
///    - label 1, uni stream: no P frame on any S video slot; MH counts at least
///      K `server_muted` drops on EACH pod (P transmitted K frames to each;
///      attributable under [`SERVER_MUTED_PRODUCERS`]);
///    - label 1, datagram: `transport_mismatch` (the transport check runs
///      before the disposition), never a video slot;
///    - relabelled to 0, datagram: reaches S's AUDIO slot for P (allowed by
///      R-16: audio is not muted) and never a video slot. P's genuine audio
///      keeps reaching that slot too — the other kind flows.
/// 3. UNMUTE: P's self update (video no longer server-muted) arrives BEFORE
///    the directive whose video targets go empty -> non-empty (the pair the
///    client keys its "a host resumed your camera" notice on), and that
///    directive equals the pre-mute one; P's video reaches S's slot again.
/// 4. REVERSE: an AUDIO server mute withdraws only audio (video stream
///    byte-identical), S's audio slot for P reads `AVATAR_SERVER_MUTED`, its
///    video slots are unchanged, and fresh P video keeps reaching S.
#[tokio::test]
async fn test_video_server_mute_withdraws_drops_a_patched_publisher_and_unmute_restores_it() {
    const PHASE: &str = "VIDEO SERVER MUTE";
    let _serialised = SERVER_MUTED_PRODUCERS.lock().await;
    let cluster = cluster().await;
    let prom = PrometheusClient::new(&cluster.prometheus_base_url);
    let bound = participant::settle_window(&cluster).await + VIEW_MARGIN;
    let auth = AuthClient::new(&cluster.ac_base_url);
    let gc = GcClient::new(&cluster.gc_base_url);
    let invited = Invited::new(&auth, &gc, "Video Server Mute", 4).await;
    let av = [AUDIO, VIDEO];

    let mut p = invited
        .join(1, "P", &JoinDeclaration::default().publishing(&av))
        .await;
    let p_mh = reach_both(PHASE, &p).await;
    let c = invited
        .join(2, "C", &JoinDeclaration::default().publishing(&av))
        .await;
    let c_mh = reach_both(PHASE, &c).await;
    let (s_audio, s_video) = ([11_u32, 12], [21_u32, 22]);
    let mut s = invited
        .join(
            3,
            "S",
            &JoinDeclaration::default()
                .publishing(&[])
                .receiving(&s_audio, &s_video),
        )
        .await;
    let s_mh = reach_both(PHASE, &s).await;
    let mut h = invited.join(0, "H", &JoinDeclaration::default()).await;
    let (p_id, c_id) = (p.sender_id(), c.sender_id());
    let p_pid = p.join.participant_id.clone();
    let _ = two_offered_handlers(PHASE, std::slice::from_ref(&p));

    // S holds P and C on both kinds.
    let mut want = vec![p_id, c_id];
    want.sort_unstable();
    s.converge(&format!("{PHASE} S"), bound, |v, _| {
        v.is_some_and(|v| {
            let mut f = filled_senders(v, AUDIO);
            f.sort_unstable();
            f == want
                && slots_of(v, VIDEO).len() == 2
                && slots_of(v, VIDEO)
                    .iter()
                    .all(|a| a.slot_state == SlotState::Active as i32)
        })
    })
    .await;
    let s_before = s.view().clone();
    let (p_vslot, c_vslot) = (s_video[0], s_video[1]);
    let p_audio_url = s.slot_url(p_id, AUDIO);
    let p_audio_slot = s.slot_for(p_id, AUDIO);
    let s_video_urls: BTreeSet<String> = s
        .slots(VIDEO)
        .iter()
        .map(|a| a.media_handler_url.clone())
        .collect();
    p.converge(&format!("{PHASE} P directed"), bound, |_, d| {
        d.is_some_and(|d| {
            kind_targets(d, AUDIO) == BTreeSet::from([p_audio_url.clone()])
                && !kind_targets(d, VIDEO).is_empty()
                && kind_targets(d, VIDEO).is_subset(&s_video_urls)
        })
    })
    .await;
    let d0 = p.directive.clone().expect("P's converged directive");

    // C's compliant video (the liveness control) on every handler it is
    // directed to, and S's subscribers on every handler.
    let c_pubs: Vec<BackgroundPublisher> = c_mh
        .values()
        .map(|m| BackgroundPublisher::start(m.connection().clone(), c_id))
        .collect();
    let mut s_subs: Vec<VideoSubscriber> = s_mh
        .values()
        .map(|m| VideoSubscriber::spawn(m.connection().clone()))
        .collect();
    let patched = |label, transport| VideoPublisherConfig {
        group_start: GroupStartSource::Periodic(GROUP_INTERVAL),
        mode: PublisherMode::KeepSendingWhileMuted,
        label,
        transport,
        ..VideoPublisherConfig::new(p_id)
    };
    let mut p_uni = video_publishers(
        &p_mh,
        patched(UplinkLabel::Directed, VideoTransport::StreamPerGroup),
    );

    // PRE: P's video and C's video reach S's own slots for them; P's genuine
    // audio reaches S's audio slot for P.
    until_fresh_video_on_slot(
        &format!("{PHASE} PRE P"),
        &mut p_uni,
        &mut s_subs,
        p_vslot,
        p_id,
        None,
    )
    .await;
    until_fresh_video_on_slot(
        &format!("{PHASE} PRE C"),
        &mut [],
        &mut s_subs,
        c_vslot,
        c_id,
        None,
    )
    .await;
    let pre_audio = bind_until_received(
        &format!("{PHASE} PRE audio"),
        s_mh[&p_audio_url].connection(),
        &[(p_mh[&p_audio_url].connection(), MARK_P)],
        &[],
    )
    .await;
    assert_relayed_on_slot(PHASE, &pre_audio[&MARK_P], p_audio_slot, MARK_P, "P");

    // ---- 1. MUTE video ------------------------------------------------------
    let dropped_base = settled_baseline(
        &prom,
        &reason_promql("server_muted"),
        COUNTER_BUDGET,
        "the assertion under test",
    )
    .await;
    let mismatch_base = settled_baseline(
        &prom,
        &reason_promql("transport_mismatch"),
        COUNTER_BUDGET,
        "the assertion under test",
    )
    .await;
    h.session
        .write(&server_mute(&p_pid, None, Some(true)))
        .await;
    let withdrawn = update_before_directive(
        &mut p,
        &format!("{PHASE} 1 MUTE"),
        bound,
        |u| u.video_server_muted && !u.audio_server_muted,
        |d| kind_targets(d, VIDEO).is_empty(),
    )
    .await;
    assert_eq!(
        stream_of(&withdrawn, AUDIO),
        stream_of(&d0, AUDIO),
        "{PHASE} 1: the AUDIO stream of P's directive is byte-identical"
    );
    assert_eq!(
        stream_of(&withdrawn, VIDEO).encoding,
        stream_of(&d0, VIDEO).encoding,
        "{PHASE} 1: a withdrawal empties the targets; the stream stays listed"
    );
    s.converge(&format!("{PHASE} 1 S"), bound, |v, _| {
        v.is_some_and(|v| named_state(v, VIDEO, p_id) == Some(SlotState::AvatarServerMuted as i32))
    })
    .await;
    assert_eq!(
        kind_slots(s.view(), AUDIO),
        kind_slots(&s_before, AUDIO),
        "{PHASE} 1: S's audio slots (P's ACTIVE among them) are byte-identical"
    );

    // ---- 2a. PATCHED, label 1 on uni streams --------------------------------
    // Settle: P transmits every round until FLAT_ROUNDS consecutive rounds bring
    // no P video to S (frames in flight before MH installed the mute may still
    // land; flat counts from the last one), with C's video still arriving.
    let deadline = Instant::now() + PHASE_DEADLINE;
    let (mut flat, mut c_alive) = (0_u32, false);
    while flat < FLAT_ROUNDS || !c_alive {
        assert!(
            Instant::now() < deadline,
            "{PHASE} 2a SETTLE: P's video did not go flat at S (flat {flat}/{FLAT_ROUNDS}, C \
             alive {c_alive}). P frames on S's video slots after MC showed AVATAR_SERVER_MUTED \
             mean MH is not dropping the DROP_AT_INGRESS stream: check \
             mh_media_frames_dropped_total{{reason=\"server_muted\"}}"
        );
        let _ = pump(&mut p_uni).await;
        let frames = video_frames(&mut s_subs).await;
        c_alive |= frames
            .iter()
            .any(|f| u32::from(f.key_id_sender) == c_id && u32::from(f.slot_id) == c_vslot);
        let p_reached = frames
            .iter()
            .any(|f| u32::from(f.key_id_sender) == p_id || u32::from(f.slot_id) == p_vslot);
        flat = if p_reached { 0 } else { flat + 1 };
    }
    // The measured window: K frames per pod, NONE reaching S.
    let mut sent = vec![0_u64; p_uni.len()];
    let mut c_since: Option<u32> = None;
    let no_p = |f: &ReceivedFrame, what: &str| {
        assert!(
            u32::from(f.key_id_sender) != p_id && u32::from(f.slot_id) != p_vslot,
            "{PHASE} 2a {what}: a frame of the server-muted video stream reached S's video slot"
        );
    };
    for _ in 0..WINDOW_ROUNDS {
        for (total, n) in sent.iter_mut().zip(pump(&mut p_uni).await) {
            *total += n;
        }
        for f in video_frames(&mut s_subs).await {
            no_p(&f, "WINDOW");
            if u32::from(f.key_id_sender) == c_id {
                c_since = c_since.max(Some(f.gop_counter));
            }
        }
    }
    // Window end bounded by TRAFFIC: a C frame from a group NEWER than any C
    // frame seen in the window (so sent after P's last frame) lands on S's
    // slot for C; every frame drained on the way is checked too.
    let deadline = Instant::now() + FRAME_DEADLINE;
    'end: loop {
        assert!(
            Instant::now() < deadline,
            "{PHASE} 2a END: no fresh C frame reached S within {FRAME_DEADLINE:?}"
        );
        for f in video_frames(&mut s_subs).await {
            no_p(&f, "END");
            if u32::from(f.key_id_sender) == c_id
                && u32::from(f.slot_id) == c_vslot
                && c_since.is_none_or(|g| f.gop_counter > g)
            {
                break 'end;
            }
        }
    }
    let k = sent.iter().copied().min().unwrap_or(0);
    assert!(
        k > 0,
        "{PHASE} 2a: positive control — P transmitted on every pod"
    );
    for handler in mh_grpc::handlers() {
        let base = instance_for_pod_ip(&dropped_base, &handler.pod_ip).map_or(0.0, |(_, v)| v);
        #[expect(
            clippy::cast_precision_loss,
            reason = "a frame count of a few dozen; exact in f64"
        )]
        let want = base + k as f64;
        poll_until_pinned_instance(
            &prom,
            &reason_promql("server_muted"),
            &handler.pod_ip,
            PINNED_METRIC_BOUND,
            Duration::from_secs(2),
            &format!("{PHASE} 2a DROPS ({})", handler.name),
            ">= own baseline + K (P's label-1 frames to this pod while muted)",
            |v| v >= want,
        )
        .await;
    }
    for publisher in &mut p_uni {
        publisher.finish().await;
    }

    // ---- 2b. PATCHED, label 1 as datagrams: transport_mismatch -----------------
    let mut p_dgram = video_publishers(
        &p_mh,
        patched(UplinkLabel::Directed, VideoTransport::Datagram),
    );
    // P's GENUINE audio rides every round: the liveness control (S's receive
    // path and P's uplink are alive), and after the last video datagram it
    // bounds the window by traffic.
    let mut seq = SEQ_MUTED;
    let send_audio = |seq: u32| {
        p_mh[&p_audio_url]
            .connection()
            .send_datagram(audio_datagram(seq, MARK_P))
            .unwrap_or_else(|e| panic!("{PHASE} 2b/2c: P could not send audio: {e}"));
    };
    // Drains S; returns whether a genuine MARK_P datagram SENT AFTER `after`
    // (its stream sequence) landed on P's audio slot. A datagram on a video
    // slot, or P video on a uni stream, fails.
    let check_2b =
        |dgrams: Vec<(u32, u32, Vec<u8>)>, frames: Vec<ReceivedFrame>, after: u32| -> bool {
            for f in frames {
                assert!(
                    u32::from(f.key_id_sender) != p_id,
                    "{PHASE} 2b: P's video reached S"
                );
            }
            let mut genuine = false;
            for (slot, sequence, payload) in dgrams {
                assert!(
                    !s_video.contains(&slot),
                    "{PHASE} 2b: a datagram reached S's VIDEO slot {slot}"
                );
                genuine |= slot == p_audio_slot
                    && sequence > after
                    && payload.iter().all(|b| *b == MARK_P);
            }
            genuine
        };
    for _ in 0..WINDOW_ROUNDS {
        let _ = pump(&mut p_dgram).await;
        seq += 1;
        send_audio(seq);
        let mut dgrams = Vec::new();
        for conn in s_mh.values() {
            dgrams.extend(datagrams(conn.connection()).await);
        }
        let _ = check_2b(dgrams, video_frames(&mut s_subs).await, u32::MAX);
    }
    // Traffic-bounded end: genuine audio SENT AFTER the last video datagram
    // (sequence above the window's last) arrives on P's audio slot. A window
    // datagram drained here late cannot satisfy it.
    let last_window_seq = seq;
    let deadline = Instant::now() + PHASE_DEADLINE;
    loop {
        assert!(
            Instant::now() < deadline,
            "{PHASE} 2b END: P's genuine audio never reached S's audio slot for P after the \
             last video datagram — the liveness control failed"
        );
        seq += 1;
        send_audio(seq);
        let mut dgrams = Vec::new();
        for conn in s_mh.values() {
            dgrams.extend(datagrams(conn.connection()).await);
        }
        if check_2b(dgrams, video_frames(&mut s_subs).await, last_window_seq) {
            break;
        }
    }
    for handler in mh_grpc::handlers() {
        let base = instance_for_pod_ip(&mismatch_base, &handler.pod_ip).map_or(0.0, |(_, v)| v);
        poll_until_pinned_instance(
            &prom,
            &reason_promql("transport_mismatch"),
            &handler.pod_ip,
            PINNED_METRIC_BOUND,
            Duration::from_secs(2),
            &format!("{PHASE} 2b MISMATCH ({})", handler.name),
            ">= own baseline + 1 (datagrams may be lost, so not K)",
            |v| v > base,
        )
        .await;
    }

    // ---- 2c. PATCHED, relabelled to 0 as datagrams: audio slots only ---------
    let mut p_relabel = video_publishers(
        &p_mh,
        patched(
            UplinkLabel::Override(AUDIO_SEND_ID),
            VideoTransport::Datagram,
        ),
    );
    let deadline = Instant::now() + PHASE_DEADLINE;
    let (mut relabelled_on_audio, mut genuine_audio) = (false, false);
    while !relabelled_on_audio || !genuine_audio {
        assert!(
            Instant::now() < deadline,
            "{PHASE} 2c: within {PHASE_DEADLINE:?} S's audio slot for P did not receive both a \
             relabelled frame ({relabelled_on_audio}) and P's genuine audio ({genuine_audio}) — \
             the other kind must keep flowing"
        );
        let _ = pump(&mut p_relabel).await;
        seq += 1;
        send_audio(seq);
        for conn in s_mh.values() {
            for (slot, _, payload) in datagrams(conn.connection()).await {
                assert!(
                    !s_video.contains(&slot),
                    "{PHASE} 2c: a relabelled frame reached S's VIDEO slot {slot}"
                );
                if slot == p_audio_slot {
                    if payload.iter().all(|b| *b == MARK_P) {
                        genuine_audio = true;
                    } else if key_id_sender(&payload).map(u32::from) == Some(p_id) {
                        relabelled_on_audio = true;
                    }
                }
            }
        }
        for f in video_frames(&mut s_subs).await {
            assert!(
                u32::from(f.key_id_sender) != p_id,
                "{PHASE} 2c: P's video reached a video slot"
            );
        }
    }
    drop((p_dgram, p_relabel));

    // ---- 3. UNMUTE video: R-18 pair, restored targets, frames return ----------
    h.session
        .write(&server_mute(&p_pid, None, Some(false)))
        .await;
    let restored = update_before_directive(
        &mut p,
        &format!("{PHASE} 3 UNMUTE"),
        bound,
        |u| !u.video_server_muted,
        |d| !kind_targets(d, VIDEO).is_empty(),
    )
    .await;
    assert_eq!(
        restored, d0,
        "{PHASE} 3: the host's unmute restores P's directive exactly"
    );
    s.converge(&format!("{PHASE} 3 S"), bound, |v, _| {
        v.is_some_and(|v| *v == s_before)
    })
    .await;
    let mut p_uni = video_publishers(
        &p_mh,
        patched(UplinkLabel::Directed, VideoTransport::StreamPerGroup),
    );
    until_fresh_video_on_slot(
        &format!("{PHASE} 3 RESTORED"),
        &mut p_uni,
        &mut s_subs,
        p_vslot,
        p_id,
        None,
    )
    .await;

    // ---- 4. REVERSE: an AUDIO server mute leaves video flowing -------------
    h.session
        .write(&server_mute(&p_pid, Some(true), None))
        .await;
    let audio_withdrawn = update_before_directive(
        &mut p,
        &format!("{PHASE} 4 AUDIO MUTE"),
        bound,
        |u| u.audio_server_muted && !u.video_server_muted,
        |d| kind_targets(d, AUDIO).is_empty(),
    )
    .await;
    assert_eq!(
        stream_of(&audio_withdrawn, VIDEO),
        stream_of(&d0, VIDEO),
        "{PHASE} 4: the VIDEO stream of P's directive is byte-identical"
    );
    s.converge(&format!("{PHASE} 4 S"), bound, |v, _| {
        v.is_some_and(|v| named_state(v, AUDIO, p_id) == Some(SlotState::AvatarServerMuted as i32))
    })
    .await;
    assert_eq!(
        kind_slots(s.view(), VIDEO),
        kind_slots(&s_before, VIDEO),
        "{PHASE} 4: S's video slots are byte-identical"
    );
    let since = newest_group(&p_uni);
    until_fresh_video_on_slot(
        &format!("{PHASE} 4 VIDEO FLOWS"),
        &mut p_uni,
        &mut s_subs,
        p_vslot,
        p_id,
        since,
    )
    .await;
    h.session
        .write(&server_mute(&p_pid, Some(false), None))
        .await;
    let back = update_before_directive(
        &mut p,
        &format!("{PHASE} 4 AUDIO UNMUTE"),
        bound,
        |u| !u.audio_server_muted,
        |d| !kind_targets(d, AUDIO).is_empty(),
    )
    .await;
    assert_eq!(
        back, d0,
        "{PHASE} 4: the audio unmute restores the directive"
    );

    for publisher in &mut p_uni {
        publisher.finish().await;
    }
    for publisher in c_pubs {
        publisher.stop().await;
    }
    drop((s_subs, s_mh, p_mh, c_mh));
}

/// R-17 (CLIENT VIDEO MUTE, with a toggle storm): publisher P client-mutes its
/// VIDEO. S1 and S2 each hold P on one audio and one video slot.
///
/// - STORM then FENCE: P sends seven alternating video mute requests (audio
///   absent: unchanged), ending muted, then an AUDIO mute (video absent). The
///   requests ride one ordered connection into one actor, so the holder view
///   that first shows P's audio avatar was composed after every video toggle:
///   in THAT view, P's video slot must read `AVATAR_CLIENT_MUTED` (the final
///   state), on its edge's url. Convergence only — the numeric push bound is
///   proven in-process (see the module doc).
/// - PAUSED, by its per-entity signature: P keeps sending video
///   (`KeepSendingWhileMuted`), and FRESH frames (a group started after the
///   holders converged) still reach both holders' video slots — a PAUSED stream
///   is forwarded; DROP_AT_INGRESS would drop them.
/// - The DIRECTIVE never moved: S1 (the meeting's creator, so its token is
///   Host) then server-mutes P's AUDIO. The FIRST directive P reads after its
///   pre-storm one must be exactly that withdrawal — the pre-storm directive
///   with empty AUDIO targets and the VIDEO stream byte-identical. Had any
///   client mute moved the directive, an earlier directive would arrive first
///   (or the video stream would differ). The fence is an AUDIO server mute of a
///   publisher sending no audio, so it produces no `server_muted` drops and
///   this fn stays outside [`SERVER_MUTED_PRODUCERS`].
#[tokio::test]
async fn test_client_video_mute_pauses_without_touching_the_directive_and_a_storm_converges() {
    const PHASE: &str = "CLIENT VIDEO MUTE";
    /// Video toggles in the storm: odd (ends muted), and with the audio fence
    /// well under the per-connection mute burst (8), so none is rate-limited.
    const TOGGLES: u32 = 5;
    let cluster = cluster().await;
    let prom = PrometheusClient::new(&cluster.prometheus_base_url);
    let bound = participant::settle_window(&cluster).await + VIEW_MARGIN;
    let auth = AuthClient::new(&cluster.ac_base_url);
    let gc = GcClient::new(&cluster.gc_base_url);
    let invited = Invited::new(&auth, &gc, "Client Video Mute", 3).await;

    // P joins first (user 2); S1 is the creator (user 0: Host), S2 user 1.
    let mut p = invited
        .join(
            2,
            "P",
            &JoinDeclaration::default().publishing(&[AUDIO, VIDEO]),
        )
        .await;
    let p_mh = reach_both(PHASE, &p).await;
    let p_id = p.sender_id();
    let slots = [(1_u32, 2_u32), (3, 4)];
    let mut holders = Vec::new();
    for (i, (label, (a, v))) in [("S1", slots[0]), ("S2", slots[1])].into_iter().enumerate() {
        let mut s = invited
            .join(
                i,
                label,
                &JoinDeclaration::default()
                    .publishing(&[])
                    .receiving(&[a], &[v]),
            )
            .await;
        let mh = reach_both(PHASE, &s).await;
        s.converge(&format!("{PHASE} {label}"), bound, |v, _| {
            v.is_some_and(|v| {
                filled_senders(v, AUDIO) == vec![p_id]
                    && slots_of(v, VIDEO)
                        .first()
                        .is_some_and(|a| a.slot_state == SlotState::Active as i32)
            })
        })
        .await;
        holders.push((s, mh, a, v));
    }
    // P's directive targets exactly the handlers its holders' edges are on.
    let want = |kind: MediaKind| -> BTreeSet<String> {
        holders
            .iter()
            .map(|(s, ..)| s.slots(kind)[0].media_handler_url.clone())
            .collect()
    };
    let (want_audio, want_video) = (want(AUDIO), want(VIDEO));
    p.converge(&format!("{PHASE} P directed"), bound, |_, d| {
        d.is_some_and(|d| {
            kind_targets(d, AUDIO) == want_audio && kind_targets(d, VIDEO) == want_video
        })
    })
    .await;
    let d0 = p.directive.clone().expect("P's converged directive");
    let urls_before: Vec<String> = holders
        .iter()
        .map(|(s, ..)| s.slots(VIDEO)[0].media_handler_url.clone())
        .collect();

    let mut p_pubs = video_publishers(
        &p_mh,
        VideoPublisherConfig {
            group_start: GroupStartSource::Periodic(GROUP_INTERVAL),
            mode: PublisherMode::KeepSendingWhileMuted,
            ..VideoPublisherConfig::new(p_id)
        },
    );
    let mut subs: Vec<Vec<VideoSubscriber>> = holders
        .iter()
        .map(|(_, mh, ..)| {
            mh.values()
                .map(|m| VideoSubscriber::spawn(m.connection().clone()))
                .collect()
        })
        .collect();
    // PRE: P's video reaches each holder's own video slot.
    for ((s, _, _, v), sub) in holders.iter().zip(subs.iter_mut()) {
        until_fresh_video_on_slot(
            &format!("{PHASE} PRE {}", s.label),
            &mut p_pubs,
            sub,
            *v,
            p_id,
            None,
        )
        .await;
    }

    // The paused-source baseline, per pod, before any client mute.
    let paused_base = settled_baseline(
        &prom,
        &paused_promql(),
        COUNTER_BUDGET,
        "the assertion under test",
    )
    .await;

    // STORM, then the audio FENCE.
    for i in 0..TOGGLES {
        p.session.write(&mute_request(None, Some(i % 2 == 0))).await;
    }
    p.session.write(&mute_request(Some(true), None)).await;
    for ((s, ..), url) in holders.iter_mut().zip(&urls_before) {
        s.converge(&format!("{PHASE} FENCE {}", s.label), bound, |v, _| {
            v.is_some_and(|v| {
                named_state(v, AUDIO, p_id) == Some(SlotState::AvatarClientMuted as i32)
            })
        })
        .await;
        let video = s.slots(VIDEO)[0].clone();
        assert_eq!(
            (video.slot_state, video.sender_id),
            (SlotState::AvatarClientMuted as i32, Some(p_id)),
            "{PHASE}: {} — in the view composed after every toggle, P's video slot shows the \
             FINAL state (client-muted)",
            s.label
        );
        assert_eq!(
            &video.media_handler_url, url,
            "{PHASE}: a client-mute avatar keeps its edge's url"
        );
    }

    // PAUSED: fresh frames (a group started after the holders converged) are
    // still forwarded to both holders, and every pod P sent to counts them as
    // paused-source arrivals.
    let since = newest_group(&p_pubs);
    let mut sent = vec![0_u64; p_pubs.len()];
    let mut fresh = vec![false; holders.len()];
    let deadline = Instant::now() + FRAME_DEADLINE;
    while fresh.iter().any(|f| !f) {
        assert!(
            Instant::now() < deadline,
            "{PHASE} PAUSED: within {FRAME_DEADLINE:?} fresh frames did not reach every holder \
             ({fresh:?}) — a PAUSED stream is forwarded; DROP_AT_INGRESS would drop them"
        );
        for (total, n) in sent.iter_mut().zip(pump(&mut p_pubs).await) {
            *total += n;
        }
        for (i, ((_, _, _, v), sub)) in holders.iter().zip(subs.iter_mut()).enumerate() {
            fresh[i] |= video_frames(sub).await.iter().any(|f| {
                u32::from(f.slot_id) == *v
                    && u32::from(f.key_id_sender) == p_id
                    && since.is_none_or(|g| f.gop_counter > g)
            });
        }
    }
    let k = sent.iter().copied().min().unwrap_or(0);
    assert!(
        k > 0,
        "{PHASE} PAUSED: positive control — P transmitted on every pod"
    );
    for handler in mh_grpc::handlers() {
        let base = instance_for_pod_ip(&paused_base, &handler.pod_ip).map_or(0.0, |(_, v)| v);
        #[expect(
            clippy::cast_precision_loss,
            reason = "a frame count of a few dozen; exact in f64"
        )]
        let want = base + k as f64;
        poll_until_pinned_instance(
            &prom,
            &paused_promql(),
            &handler.pod_ip,
            PINNED_METRIC_BOUND,
            Duration::from_secs(2),
            &format!("{PHASE} PAUSED ARRIVALS ({})", handler.name),
            ">= own baseline + K (P's frames to this pod while client-muted)",
            |v| v >= want,
        )
        .await;
    }

    // The directive never moved: the first one after d0 is the host's AUDIO
    // server-mute withdrawal (the fence), with P's video stream untouched.
    let p_pid = p.join.participant_id.clone();
    holders[0]
        .0
        .session
        .write(&server_mute(&p_pid, Some(true), None))
        .await;
    let deadline = Instant::now() + bound;
    let next = loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(
            !remaining.is_zero(),
            "{PHASE}: P's audio withdrawal (the fence) never arrived"
        );
        if let Some(server_message::Message::SendDirective(d)) =
            p.session.try_read(remaining).await.and_then(|m| m.message)
        {
            break d;
        }
    };
    let mut expected = d0.clone();
    for stream in &mut expected.streams {
        if stream.media_kind == AUDIO as i32 {
            stream.targets.clear();
        }
    }
    assert_eq!(
        next, expected,
        "{PHASE}: client mute changed no directive — the first one after the pre-storm \
         directive is the fence's audio withdrawal, its video stream byte-identical"
    );

    for publisher in &mut p_pubs {
        publisher.finish().await;
    }
    drop((subs, holders, p_mh));
}
