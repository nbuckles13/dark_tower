//! One signalling participant driven through the real GC -> MC join, with
//! the latest `StreamAssignments` / `SendDirective` it has seen — the
//! vocabulary the multi-party env-tests share.
//!
//! Hoisted from `tests/27_mc_slot_placement.rs` at story 2 task 10, when
//! `tests/26_mh_quic.rs` (S4 server mute, S5 two-sender attribution, edge
//! churn) became its second consumer. The two triage literals moved with it
//! unchanged; `docs/runbooks/devloop-validation.md` §8 catalogues them.

use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use crate::cluster::ClusterConnection;
use crate::fixtures::auth_client::UserRegistrationRequest;
use crate::fixtures::gc_client::{CreateMeetingRequest, JoinMeetingResponse};
use crate::fixtures::mc_session::{self, JoinDeclaration, McSession};
use crate::fixtures::metrics::{gauge_by_instance_present, PrometheusClient};
use crate::fixtures::{AuthClient, GcClient};
use proto_gen::dark_tower::signaling::v1::{
    client_message, server_message, ClientMessage, JoinResponse, MediaHandlerSet, MediaKind,
    SendDirective, StreamAssignment, StreamAssignments,
};

/// A `media_dial_token` (ADR-0039 §5): a BEARER CREDENTIAL. Its `Debug` is
/// redacted, it has no `Display` and no `Serialize`, and [`DialToken::expose`]
/// is for building the MH connect request only — never for a log line, a
/// panic, or an assertion message.
#[derive(Clone, PartialEq, Eq)]
pub struct DialToken(String);

impl DialToken {
    /// The token text, for the MH connect request ONLY.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for DialToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DialToken(<redacted, {} bytes>)", self.0.len())
    }
}

/// One `MediaHandlerSet` as membership only: version and `(handler_ref, url)`
/// entries, sorted by ref. Never a token (history holds no credentials).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandlerSetView {
    pub version: u64,
    pub entries: Vec<(u32, String)>,
}

impl HandlerSetView {
    /// The urls, sorted.
    #[must_use]
    pub fn urls(&self) -> Vec<String> {
        let mut urls: Vec<String> = self.entries.iter().map(|(_, u)| u.clone()).collect();
        urls.sort();
        urls
    }

    /// The refs, sorted.
    #[must_use]
    pub fn refs(&self) -> Vec<u32> {
        self.entries.iter().map(|(r, _)| *r).collect()
    }
}

/// The MediaHandlerSets one participant has seen (ADR-0039 §4): the join's,
/// then every `ServerMessage.media_handler_set` (full snapshot, replace
/// semantics). The join-time set MAY be empty (MC releases a parked join with
/// the Active-only snapshot), so the CURRENT set is the latest one, not the
/// join's.
///
/// Tokens: the latest per IN-SET ref. A ref that leaves the set has its token
/// DROPPED (not hidden); an empty token (MC not minting yet) is absent.
#[derive(Clone, Debug, Default)]
pub struct HandlerSets {
    current: Option<HandlerSetView>,
    history: Vec<HandlerSetView>,
    tokens: BTreeMap<u32, DialToken>,
}

impl HandlerSets {
    /// Apply one received set (replace semantics).
    pub fn apply(&mut self, set: &MediaHandlerSet) {
        let mut entries: Vec<(u32, String)> = set
            .handlers
            .iter()
            .map(|h| (h.handler_ref, h.media_handler_url.clone()))
            .collect();
        entries.sort();
        self.tokens = set
            .handlers
            .iter()
            .filter(|h| !h.media_dial_token.is_empty())
            .map(|h| (h.handler_ref, DialToken(h.media_dial_token.clone())))
            .collect();
        let view = HandlerSetView {
            version: set.handler_set_version,
            entries,
        };
        self.history.push(view.clone());
        self.current = Some(view);
    }

    /// The latest set, if any was received.
    #[must_use]
    pub fn current(&self) -> Option<&HandlerSetView> {
        self.current.as_ref()
    }

    /// Every set received, in order (membership only).
    #[must_use]
    pub fn history(&self) -> &[HandlerSetView] {
        &self.history
    }

    /// The latest dial token for `handler_ref`, if it is in the current set
    /// and MC minted one.
    #[must_use]
    pub fn dial_token(&self, handler_ref: u32) -> Option<&DialToken> {
        self.tokens.get(&handler_ref)
    }

    /// The current set's urls, sorted (empty before any set).
    #[must_use]
    pub fn urls(&self) -> Vec<String> {
        self.current
            .as_ref()
            .map(HandlerSetView::urls)
            .unwrap_or_default()
    }

    /// The current entry for `url`, if any.
    #[must_use]
    pub fn ref_for_url(&self, url: &str) -> Option<u32> {
        self.current
            .as_ref()?
            .entries
            .iter()
            .find(|(_, u)| u == url)
            .map(|(r, _)| *r)
    }
}

/// The audio slot kind, for the kind-taking helpers below. Every helper that
/// reads a view takes a kind: a video slot showing an avatar NAMES its
/// participant, so a kind-blind "which slot holds sender X" would answer with
/// the video avatar (empty url) as readily as the audio edge.
pub const AUDIO: MediaKind = MediaKind::Audio;
/// The camera-video slot kind (see [`AUDIO`]).
pub const VIDEO: MediaKind = MediaKind::VideoCamera;

/// Greppable triage literal for the ONE failure in this suite that is an
/// environment fact rather than a diff defect: fewer than two distinct handlers
/// were programmed for the meeting, so the split scenario has nothing to
/// observe. Layer 7 reports every non-zero env-test as `STATUS=FAIL
/// REASON=env-tests-failed` (implementer lane, attempt consumed) and
/// deliberately does not grep suite output, so the only thing that can put a
/// triager on the operator lane is a stable literal they can grep for —
/// the same discipline as `32_media_metric_hygiene.rs`'s `TRIAGE_SCRAPE`.
/// Catalogued in `docs/runbooks/devloop-validation.md` §8 (pointer to §6.7).
/// Margin over MC's settle window for a participant view to converge (a
/// failure bound, never a pass condition).
pub const VIEW_MARGIN: std::time::Duration = std::time::Duration::from_secs(10);

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
    /// Every MediaHandlerSet this participant has seen ([`HandlerSets`]), fed
    /// from the JoinResponse and from every set message the fixture reads.
    pub sets: HandlerSets,
    /// Participant ids MC announced as LEFT (`ServerMessage.participant_left`)
    /// while this fixture read signalling — the roster-removal signal (MC
    /// removes a cleanly-closed participant at once, an abruptly lost one
    /// only after its disconnect grace). Ids only; never logged.
    pub left: BTreeSet<String>,
}

impl Participant {
    pub fn sender_id(&self) -> u32 {
        self.join.sender_id.expect("MC allocates a sender id")
    }

    /// The `JoinResponse.media_handler_set` (ADR-0039 §4). MC always sends
    /// it (empty included), so its absence is an MC defect, not an
    /// environment fact.
    ///
    /// # Panics
    /// If the JoinResponse carries no set.
    pub fn handler_set(&self) -> &proto_gen::dark_tower::signaling::v1::MediaHandlerSet {
        self.join.media_handler_set.as_ref().unwrap_or_else(|| {
            panic!(
                "{}: JoinResponse carries no media_handler_set — MC always sends one (MC defect)",
                self.label
            )
        })
    }

    /// The handler urls this participant was OFFERED: its CURRENT
    /// MediaHandlerSet's urls, sorted (never the legacy `media_servers`).
    pub fn offered(&self) -> Vec<String> {
        self.sets.urls()
    }

    /// Apply one signalling message to this participant's state (latest view,
    /// directive, handler set).
    fn absorb(&mut self, phase: &str, m: server_message::Message) {
        match m {
            server_message::Message::StreamAssignments(a) => self.view = Some(a),
            server_message::Message::SendDirective(d) => self.directive = Some(d),
            server_message::Message::MediaHandlerSet(set) => self.sets.apply(&set),
            server_message::Message::ParticipantLeft(left) => {
                self.left.insert(left.participant_id);
            }
            server_message::Message::Error(e) => {
                panic!(
                    "{phase}: MC rejected {}'s request: {}",
                    self.label, e.message
                )
            }
            // Roster traffic. Nothing is logged about it: it carries names.
            _ => {}
        }
    }

    /// Read signalling until `done` holds on this participant's handler sets,
    /// within `bound` (the view and directive are kept current meanwhile).
    /// Messages name only versions, refs and urls — never a token.
    ///
    /// # Panics
    /// At `bound`, naming the last set's version and refs.
    pub async fn await_handler_set(
        &mut self,
        phase: &str,
        bound: Duration,
        done: impl Fn(&HandlerSets) -> bool,
    ) {
        let deadline = Instant::now() + bound;
        while !done(&self.sets) {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(
                !remaining.is_zero(),
                "{phase}: {}'s MediaHandlerSet did not reach the expected state within {bound:?}; \
                 last set {:?} ({} received)",
                self.label,
                self.sets.current().map(|v| (v.version, v.refs(), v.urls())),
                self.sets.history().len()
            );
            if let Some(m) = self
                .session
                .try_read(remaining)
                .await
                .and_then(|m| m.message)
            {
                self.absorb(phase, m);
            }
        }
    }

    /// Read signalling until MC announced `participant_id` LEFT, within
    /// `bound` (the view, directive and sets are kept current meanwhile).
    ///
    /// # Panics
    /// At `bound` (the id is not printed).
    pub async fn await_left(&mut self, phase: &str, bound: Duration, participant_id: &str) {
        let deadline = Instant::now() + bound;
        while !self.left.contains(participant_id) {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(
                !remaining.is_zero(),
                "{phase}: {} saw no ParticipantLeft for the departing participant within \
                 {bound:?} — MC did not remove it from the roster (a clean close removes at once; \
                 check mc_participant_leaves_total{{reason}})",
                self.label
            );
            if let Some(m) = self
                .session
                .try_read(remaining)
                .await
                .and_then(|m| m.message)
            {
                self.absorb(phase, m);
            }
        }
    }

    /// Redeclare this participant's receive slots (post-join
    /// `ReceiveCapability`): audio ids, then video ids, in declaration order.
    /// Built by the one builder, `mc_session::receive_capability`.
    pub async fn declare(&mut self, audio_slot_ids: &[u32], video_slot_ids: &[u32]) {
        self.session
            .write(&ClientMessage {
                message: Some(client_message::Message::ReceiveCapability(
                    mc_session::receive_capability(audio_slot_ids, video_slot_ids),
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
                self.view.as_ref().map(|v| (
                    filled_senders(v, AUDIO),
                    filled_senders(v, VIDEO),
                    &v.unreachable_sender_ids
                )),
                self.directive.as_ref().map(targets_of)
            );
            if let Some(m) = self
                .session
                .try_read(remaining)
                .await
                .and_then(|m| m.message)
            {
                self.absorb(phase, m);
            }
        }
    }

    pub fn view(&self) -> &StreamAssignments {
        self.view.as_ref().expect("converged view")
    }

    /// This participant's slots of `kind`, in declaration order.
    pub fn slots(&self, kind: MediaKind) -> Vec<&StreamAssignment> {
        slots_of(self.view(), kind)
    }

    /// The `kind` slot naming `sender`. Panics if none does.
    fn slot_naming(&self, sender: u32, kind: MediaKind) -> &StreamAssignment {
        self.view()
            .assignments
            .iter()
            .find(|a| a.media_kind == kind as i32 && a.sender_id == Some(sender))
            .unwrap_or_else(|| {
                panic!(
                    "{} has no {} slot naming sender {sender}",
                    self.label,
                    kind.as_str_name()
                )
            })
    }

    /// The handler url on this participant's `kind` slot naming `sender`.
    pub fn slot_url(&self, sender: u32, kind: MediaKind) -> String {
        self.slot_naming(sender, kind).media_handler_url.clone()
    }

    /// The slot id this participant declared for its `kind` slot naming
    /// `sender`.
    pub fn slot_for(&self, sender: u32, kind: MediaKind) -> u32 {
        self.slot_naming(sender, kind).slot_id
    }
}

/// `view`'s slots of `kind`, in declaration order.
pub fn slots_of(view: &StreamAssignments, kind: MediaKind) -> Vec<&StreamAssignment> {
    view.assignments
        .iter()
        .filter(|a| a.media_kind == kind as i32)
        .collect()
}

pub fn targets_of(d: &SendDirective) -> BTreeSet<String> {
    d.streams
        .iter()
        .flat_map(|s| s.targets.iter().map(|t| t.media_handler_url.clone()))
        .collect()
}

/// Open a media session to `url`, or fail as an ENVIRONMENT fact.
///
/// The credential is the participant's `media_dial` token for THAT handler
/// (ADR-0039 §5; MH admits on nothing else since story 4 MH-1), resolved from
/// its CURRENT `MediaHandlerSet` by url -> `handler_ref` -> token. A handler the
/// participant was told to use that has no token in its set is an MC defect
/// (MC mints one per member), reported as such — never as reachability.
pub async fn reach(phase: &str, p: &Participant, url: &str) -> mc_session::MhSession {
    let token = dial_token_for(phase, p, url);
    let who = p.label;
    mc_session::try_mh_connect(url, token.expose())
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

/// `p`'s current `media_dial` token for the handler at `url`.
///
/// # Panics
/// If `url` is not in `p`'s current set, or its entry carries no token — both
/// MC defects (MC mints a token for EVERY member of every set). The token is
/// never named in the message.
#[must_use]
pub fn dial_token_for(phase: &str, p: &Participant, url: &str) -> DialToken {
    let who = p.label;
    let handler_ref = p.sets.ref_for_url(url).unwrap_or_else(|| {
        panic!(
            "{phase}: {who} was asked to dial {url}, which is not in its current \
             MediaHandlerSet {:?} (MC defect or a stale test view)",
            p.sets.urls()
        )
    });
    p.sets.dial_token(handler_ref).cloned().unwrap_or_else(|| {
        panic!(
            "{phase}: {who}'s MediaHandlerSet entry for {url} (handler_ref {handler_ref}) \
             carries no media_dial token — MC mints one for every member (MC defect)"
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
            mine.insert((*url).clone(), reach(phase, p, url).await);
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
    mc_join_with(label, gc, display_name, &JoinDeclaration::default()).await
}

/// [`mc_join`] with an explicit join declaration (publish set, initial receive
/// slots, header versions).
pub async fn mc_join_with(
    label: &'static str,
    gc: &JoinMeetingResponse,
    display_name: &str,
    declaration: &JoinDeclaration,
) -> Participant {
    let mut session = McSession::connect(&mc_url(gc)).await;
    let join = mc_session::mc_join_with(
        &mut session,
        &gc.meeting_id.to_string(),
        &gc.token,
        display_name,
        declaration,
        label,
    )
    .await;
    let mut sets = HandlerSets::default();
    if let Some(set) = &join.media_handler_set {
        sets.apply(set);
    }
    Participant {
        label,
        session,
        join,
        token: gc.token.clone(),
        directive: None,
        view: None,
        gc: gc.clone(),
        sets,
        left: BTreeSet::new(),
    }
}

/// MC's WebTransport endpoint from GC's join answer.
pub fn mc_url(gc: &JoinMeetingResponse) -> String {
    gc.mc_assignment
        .webtransport_endpoint
        .clone()
        .expect("MC assignment must include webtransport_endpoint")
}

pub fn filled_senders(view: &StreamAssignments, kind: MediaKind) -> Vec<u32> {
    slots_of(view, kind)
        .into_iter()
        .filter_map(|a| a.sender_id)
        .collect()
}

/// A meeting created as its first user, with `users.len()` registered users
/// who have NOT joined yet: for scenarios that must sequence each join (and
/// converge on an observed view) before the next, because MC fills slots when
/// a pair becomes routable, so the fill depends on event ORDER.
pub struct Invited<'a> {
    gc: &'a GcClient,
    meeting_code: String,
    /// `(user access token, display name)` per invited user, in label order.
    users: Vec<(String, String)>,
}

impl Invited<'_> {
    /// Register `count` users and create a meeting as the first.
    pub async fn new<'a>(
        auth: &AuthClient,
        gc: &'a GcClient,
        title: &str,
        count: usize,
    ) -> Invited<'a> {
        let mut users = Vec::new();
        for i in 0..count {
            let request = UserRegistrationRequest::unique(format!("{title} {i}"));
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
        Invited {
            gc,
            meeting_code: created.meeting_code,
            users,
        }
    }

    /// A fresh GC meeting token for user `i` (each MC join presents its own).
    pub async fn gc_join(&self, i: usize) -> JoinMeetingResponse {
        self.gc
            .join_meeting(&self.meeting_code, &self.users[i].0)
            .await
            .expect("GC should issue a meeting token")
    }

    /// User `i`'s display name.
    pub fn display(&self, i: usize) -> &str {
        &self.users[i].1
    }

    /// Join user `i` through GC -> MC with `declaration`, awaiting the
    /// `JoinResponse` (see [`mc_join_with`]).
    pub async fn join(
        &self,
        i: usize,
        label: &'static str,
        declaration: &JoinDeclaration,
    ) -> Participant {
        let gc_join = self.gc_join(i).await;
        mc_join_with(label, &gc_join, self.display(i), declaration).await
    }
}

/// Register `labels.len()` users, create a meeting as the first, and join all
/// sequentially (each JoinResponse awaited, so join order is label order),
/// each with the default declaration (audio publisher, no receive slots).
pub async fn meeting_of(
    auth: &AuthClient,
    gc: &GcClient,
    title: &str,
    labels: &[&'static str],
) -> Vec<Participant> {
    let invited = Invited::new(auth, gc, title, labels.len()).await;
    let mut ps = Vec::new();
    for (i, label) in labels.iter().enumerate() {
        ps.push(invited.join(i, label, &JoinDeclaration::default()).await);
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

#[cfg(test)]
mod tests {
    use super::*;
    use proto_gen::dark_tower::signaling::v1::MediaServerInfo;

    const SECRET_A: &str = "eyJhbGciOiJFZERTQSJ9.dGVzdC1h.c2lnLWE";
    const SECRET_A2: &str = "eyJhbGciOiJFZERTQSJ9.dGVzdC1hMg.c2lnLWEy";
    const SECRET_B: &str = "eyJhbGciOiJFZERTQSJ9.dGVzdC1i.c2lnLWI";

    fn set(version: u64, entries: &[(u32, &str, &str)]) -> MediaHandlerSet {
        MediaHandlerSet {
            handler_set_version: version,
            handlers: entries
                .iter()
                .map(|(r, url, token)| MediaServerInfo {
                    media_handler_url: (*url).to_string(),
                    handler_ref: *r,
                    media_dial_token: (*token).to_string(),
                })
                .collect(),
        }
    }

    #[test]
    fn empty_token_is_absent_not_an_error() {
        let mut s = HandlerSets::default();
        s.apply(&set(1, &[(1, "https://h:1", "")]));
        assert_eq!(s.dial_token(1), None);
        assert_eq!(s.urls(), vec!["https://h:1".to_string()]);
    }

    #[test]
    fn latest_token_per_in_set_ref_and_removed_refs_drop_theirs() {
        let mut s = HandlerSets::default();
        s.apply(&set(
            1,
            &[(1, "https://h:1", SECRET_A), (2, "https://h:2", SECRET_B)],
        ));
        assert_eq!(s.dial_token(1).map(DialToken::expose), Some(SECRET_A));
        // Token-only refresh: same version and refs, new token.
        s.apply(&set(
            1,
            &[(1, "https://h:1", SECRET_A2), (2, "https://h:2", SECRET_B)],
        ));
        assert_eq!(s.dial_token(1).map(DialToken::expose), Some(SECRET_A2));
        // Ref 2 leaves the set: its token is gone, not hidden.
        s.apply(&set(2, &[(1, "https://h:1", SECRET_A2)]));
        assert_eq!(s.dial_token(2), None);
        assert_eq!(s.dial_token(9), None, "a ref never in the set");
        assert_eq!(s.current().unwrap().refs(), vec![1]);
        assert_eq!(s.history().len(), 3);
        assert_eq!(s.ref_for_url("https://h:1"), Some(1));
        assert_eq!(s.ref_for_url("https://h:2"), None);
    }

    #[test]
    fn no_debug_output_carries_a_token() {
        let mut s = HandlerSets::default();
        s.apply(&set(
            1,
            &[(1, "https://h:1", SECRET_A), (2, "https://h:2", SECRET_B)],
        ));
        let printed = format!("{s:?} {:?} {:?}", s.dial_token(1), s.history());
        // Positive control: the Debug output is not vacuous.
        assert!(
            printed.contains("https://h:2") && printed.contains("redacted"),
            "{printed}"
        );
        for secret in [SECRET_A, SECRET_B, "dGVzdC1h", "c2lnLWI"] {
            assert!(!printed.contains(secret), "Debug leaked token material");
        }
        assert!(!format!("{:?}", DialToken(SECRET_A.into())).contains(SECRET_A));
    }
}
