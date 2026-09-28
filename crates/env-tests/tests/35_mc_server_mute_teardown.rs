//! Env-test: server mute and meeting teardown driven through MC's PUBLIC APIs
//! on the live Kind cluster (story 2 task 12; R-8, R-9, R-10, R-11, R-20;
//! ADR-0036 §5, §7).
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

#![cfg(feature = "flows")]

use std::collections::HashMap;
use std::time::{Duration, Instant};

use env_tests::cluster::ClusterConnection;
use env_tests::fixtures::gc_client::GcClient;
use env_tests::fixtures::mc_session::{self, MhSession};
use env_tests::fixtures::media::{
    assert_relayed_on_slot, audio_datagram, bind_until_received, drain,
};
use env_tests::fixtures::metrics::{
    format_instance_map, gauge_by_instance_present, poll_until_any_instance_above, PrometheusClient,
};
use env_tests::fixtures::mh_grpc::{self, authed, probe_registration, ReleaseOnDrop};
use env_tests::fixtures::participant::{
    self, filled_senders, meeting_of, open_sessions, two_offered_handlers, TRIAGE_MH_REACH,
};
use env_tests::fixtures::AuthClient;
use proto_gen::dark_tower::internal::v1::EndMeetingRequest;
use proto_gen::dark_tower::signaling::v1::{
    client_message, server_message, ClientMessage, ServerMuteRequest, SlotState,
};

async fn cluster() -> ClusterConnection {
    let cluster = ClusterConnection::new()
        .await
        .expect("Failed to connect to cluster - ensure port-forwards are running");
    cluster.check_ac_health().await.expect("AC must be running");
    cluster.check_gc_health().await.expect("GC must be running");
    cluster
}

/// Margin over MC's published connect-settle window before a view must have
/// converged. Same role as `27_mc_slot_placement.rs`'s margin.
const VIEW_MARGIN: Duration = Duration::from_secs(10);

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
const SEQ_UNMUTED: u32 = 2_000_000;

fn server_mute(target: &str, audio_muted: bool) -> ClientMessage {
    ClientMessage {
        message: Some(client_message::Message::ServerMuteRequest(
            ServerMuteRequest {
                participant_id: target.to_string(),
                audio_muted,
                video_muted: false,
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
///   its own baseline on some MH pod. Not attributable to this test alone
///   (env-test 26's S4 produces the same series on the same pods).
#[tokio::test]
async fn test_host_server_mute_is_enforced_at_mh_ingress_and_unmute_restores_it() {
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
    ps[0].declare(&[1, 2]).await;
    ps[1].declare(&[1, 2]).await;
    ps[2].declare(&[1, 2]).await;
    let bound = window + VIEW_MARGIN;
    for (i, want) in [(0, [a, b]), (1, [h, b]), (2, [h, a])] {
        ps[i]
            .converge("S3", bound, |v, d| {
                v.is_some_and(|v| {
                    let mut f = filled_senders(v);
                    f.sort_unstable();
                    let mut w = want.to_vec();
                    w.sort_unstable();
                    f == w
                }) && d.is_some()
            })
            .await;
    }

    // PRE: B hears A on B's own slot for A.
    let b_edge_url = ps[2].slot_url(a);
    let pre = bind_until_received(
        "S3 PRE",
        sessions[2][&b_edge_url].connection(),
        &[(sessions[1][&b_edge_url].connection(), MARK_A)],
        &[MARK_H],
    )
    .await;
    assert_relayed_on_slot("S3 PRE", &pre[&MARK_A], ps[2].slot_for(a), MARK_A, "A");

    // MUTE through MC's public API, by the host.
    let drops_before = prom.instance_counter_map(SERVER_MUTED_DROPS).await;
    let a_id = ps[1].join.participant_id.clone();
    ps[0].session.write(&server_mute(&a_id, true)).await;
    // MC applied it: B's own view marks A's slot SOURCE_MUTED.
    ps[2]
        .converge("S3 MUTE-VISIBLE", bound, |v, _| {
            v.is_some_and(|v| {
                v.assignments.iter().any(|s| {
                    s.sender_id == Some(a) && s.slot_state == SlotState::SourceMuted as i32
                })
            })
        })
        .await;

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
             MC showed SOURCE_MUTED means MH is not enforcing the mute: check \
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
    ps[0].session.write(&server_mute(&a_id, false)).await;
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
        Some(ps[2].slot_for(a)),
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
        .find(|a| a.sender_id == Some(sender))
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

    let before = prom
        .instance_counter_map(&server_mute_requests("not_permitted"))
        .await;
    let h_id = ps[0].join.participant_id.clone();
    ps[1].session.write(&server_mute(&h_id, true)).await;

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
    ps[0].declare(&[4, 5]).await;
    ps[1].declare(&[6]).await;
    ps[2].declare(&[7]).await;
    let bound = window + VIEW_MARGIN;
    ps[0]
        .converge("TEARDOWN", bound, |v, _| {
            v.is_some_and(|v| {
                let mut f = filled_senders(v);
                f.sort_unstable();
                let mut w = vec![b, c];
                w.sort_unstable();
                f == w
            })
        })
        .await;
    // Structural changes CONFIRMED on both handlers: S's own view places its
    // edge from B on x and its edge from C on y.
    assert_eq!(ps[0].slot_url(b), x, "TEARDOWN: S<->B sits on x");
    assert_eq!(ps[0].slot_url(c), y, "TEARDOWN: S<->C sits on y");

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

    let mc_before = prom.instance_counter_map(MC_RELEASED).await;
    let mh_before = prom.instance_counter_map(MH_RELEASED).await;

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
