//! One signalling participant driven through the real GC -> MC join, with
//! the latest `StreamAssignments` / `SendDirective` it has seen — the
//! vocabulary the multi-party env-tests share.
//!
//! Hoisted from `tests/27_mc_slot_placement.rs` at story 2 task 10, when
//! `tests/26_mh_quic.rs` (S4 server mute, S5 two-sender attribution, edge
//! churn) became its second consumer. The two triage literals moved with it
//! unchanged; `docs/runbooks/devloop-validation.md` §8 catalogues them.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use crate::cluster::ClusterConnection;
use crate::fixtures::auth_client::UserRegistrationRequest;
use crate::fixtures::gc_client::{CreateMeetingRequest, JoinMeetingResponse};
use crate::fixtures::mc_session::{self, McSession};
use crate::fixtures::metrics::{gauge_by_instance_present, PrometheusClient};
use crate::fixtures::{AuthClient, GcClient};
use proto_gen::dark_tower::signaling::v1::{
    client_message, server_message, ClientMessage, JoinResponse, MediaKind, ReceiveCapability,
    ReceiveSlot, SendDirective, StreamAssignments,
};

/// Greppable triage literal for the ONE failure in this suite that is an
/// environment fact rather than a diff defect: fewer than two distinct handlers
/// were programmed for the meeting, so the split scenario has nothing to
/// observe. Layer 7 reports every non-zero env-test as `STATUS=FAIL
/// REASON=env-tests-failed` (implementer lane, attempt consumed) and
/// deliberately does not grep suite output, so the only thing that can put a
/// triager on the operator lane is a stable literal they can grep for —
/// the same discipline as `32_media_metric_hygiene.rs`'s `TRIAGE_SCRAPE`.
/// Catalogued in `docs/runbooks/devloop-validation.md` §8 (pointer to §6.7).
pub const TRIAGE_HANDLER_SET: &str = "Triage MH handler-set health";

/// Greppable triage literal for a mock client that could not open a media
/// session to a handler it was TOLD to use: an environment fact (NodePort,
/// network policy, cert, MH health), which would otherwise present as the very
/// partial-connectivity shape the canonical phase asserts. Catalogued beside
/// [`TRIAGE_HANDLER_SET`] in `docs/runbooks/devloop-validation.md` §8.
pub const TRIAGE_MH_REACH: &str = "Triage MH reachability";

/// How long to wait for MC's settle-window gauge to be scraped.
const GAUGE_SCRAPE_BOUND: Duration = Duration::from_secs(60);

/// One participant's live MC signalling session. Connection/framing/join live
/// in `env_tests::fixtures::mc_session`; this wraps the shared session with the
/// suite's own vocabulary and the latest view it has seen.
pub struct Participant {
    pub label: &'static str,
    pub session: McSession,
    pub join: JoinResponse,
    pub token: String,
    pub directive: Option<SendDirective>,
    pub view: Option<StreamAssignments>,
    /// GC's join answer for this participant: the meeting id and MC's
    /// assignment (`mc_id`, gRPC endpoint), which a test that programs MH
    /// directly must present so MH's notifications still reach the real MC.
    pub gc: JoinMeetingResponse,
}

impl Participant {
    pub fn sender_id(&self) -> u32 {
        self.join.sender_id.expect("MC allocates a sender id")
    }

    /// The handler urls this participant was OFFERED, sorted.
    pub fn offered(&self) -> Vec<String> {
        let mut urls: Vec<String> = self
            .join
            .media_servers
            .iter()
            .map(|m| m.media_handler_url.clone())
            .collect();
        urls.sort();
        urls
    }

    pub async fn declare(&mut self, audio_slot_ids: &[u32]) {
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
    pub async fn converge(
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

    pub fn view(&self) -> &StreamAssignments {
        self.view.as_ref().expect("converged view")
    }

    /// The handler url owning this participant's edge from `sender`.
    pub fn slot_url(&self, sender: u32) -> String {
        self.view()
            .assignments
            .iter()
            .find(|a| a.sender_id == Some(sender))
            .unwrap_or_else(|| panic!("{} does not hold sender {sender}", self.label))
            .media_handler_url
            .clone()
    }

    /// The slot id this participant declared for its edge from `sender`.
    pub fn slot_for(&self, sender: u32) -> u32 {
        self.view()
            .assignments
            .iter()
            .find(|a| a.sender_id == Some(sender))
            .unwrap_or_else(|| panic!("{} does not hold sender {sender}", self.label))
            .slot_id
    }
}

pub fn targets_of(d: &SendDirective) -> BTreeSet<String> {
    d.streams
        .iter()
        .flat_map(|s| s.targets.iter().map(|t| t.media_handler_url.clone()))
        .collect()
}

/// Open a media session to `url`, or fail as an ENVIRONMENT fact.
pub async fn reach(phase: &str, who: &str, url: &str, token: &str) -> mc_session::MhSession {
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

/// Open one media session per participant per url (`result[i][url]`), each
/// failing as an ENVIRONMENT fact via [`reach`].
///
/// Hoisted from `tests/26_mh_quic.rs` at story 2 task 12, when
/// `tests/35_mc_server_mute_teardown.rs` became its second consumer.
pub async fn open_sessions(
    phase: &str,
    ps: &[Participant],
    urls: &[&String],
) -> Vec<std::collections::HashMap<String, mc_session::MhSession>> {
    let mut all = Vec::new();
    for p in ps {
        let mut mine = std::collections::HashMap::new();
        for url in urls {
            mine.insert((*url).clone(), reach(phase, p.label, url, &p.token).await);
        }
        all.push(mine);
    }
    all
}

/// MC's enforced connect settle window, read from the gauge it publishes.
pub async fn settle_window(cluster: &ClusterConnection) -> Duration {
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

pub async fn mc_join(
    label: &'static str,
    gc: &JoinMeetingResponse,
    display_name: &str,
) -> Participant {
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
        gc: gc.clone(),
    }
}

pub fn filled_senders(view: &StreamAssignments) -> Vec<u32> {
    view.assignments
        .iter()
        .filter_map(|a| a.sender_id)
        .collect()
}

/// Register `labels.len()` users, create a meeting as the first, and join all
/// sequentially (each JoinResponse awaited, so join order is label order).
pub async fn meeting_of(
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
pub fn two_offered_handlers(phase: &str, ps: &[Participant]) -> (String, String) {
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
