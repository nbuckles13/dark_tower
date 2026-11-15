//! The replacement / planned-transfer rig (story 4 MC-C, reused by MC-F):
//! the real gRPC coordination service and meeting actors under a controller,
//! a LIVE fake MH (`MockMhRegistrationClient::heartbeat_via`) and a scripted
//! GC. Shared so the crash and planned suites drive ONE rig.

#![allow(dead_code)]

use std::sync::Arc;
use std::time::Duration;

use super::media_dial::Token;
use super::media_session::slot;
use super::{
    client_loss_disagreement, client_media_config, coordination, handler_set, heartbeat_timing,
    join_media_configured, media_dial_timing, mh_handler, run_ready, MhEvent,
    MockMhRegistrationClient,
};
use ::common::observability::testing::MetricSnapshot;
use mc_service::actors::{JoinMedia, MeetingActorHandle, MeetingControllerActorHandle};
use mc_service::grpc::gc_client::AcquireResult;
use mc_service::grpc::McMediaCoordinationService;
use mc_service::media_routing::{
    DisagreementThreshold, HandlerEndpoint, HandlerId, MigrationOutcome, RemovalTrigger,
};
use mc_service::media_signaling::ReceiveCapabilityDeclaration;
use mc_test_utils::ScriptedGc;
use proto_gen::dark_tower::signaling::v1::{server_message, MediaKind, ReceiveCapability};
use tokio::sync::mpsc;

pub const MC: &str = "mc-replacement-test";
pub const KC: (&str, &str) = ("key_custody", "operator");
pub const H1: &str = "mh-1";
pub const H2: &str = "mh-2";
pub const H3: &str = "mh-3";
pub const H4: &str = "mh-4";
pub const H5: &str = "mh-5";

// ----------------------------------------------------------------------------
// Rig
// ----------------------------------------------------------------------------

pub struct Rig {
    pub c: Arc<MeetingControllerActorHandle>,
    pub svc: McMediaCoordinationService,
    pub mh: Arc<MockMhRegistrationClient>,
    pub gc: Arc<ScriptedGc>,
    /// The client-loss disagreement threshold joins install (story 4 MC-E):
    /// the shipped one unless a test sets a NON-default one.
    pub disagreement: DisagreementThreshold,
}

pub fn rig_with_cap(cap: u32) -> Rig {
    let c = Arc::new(super::make_controller(MC));
    let svc = McMediaCoordinationService::new(Arc::clone(&c), super::heartbeat_intake(MC));
    let mh = Arc::new(MockMhRegistrationClient::new());
    mh.heartbeat_to_controller(Arc::clone(&c));
    Rig {
        c,
        svc,
        mh,
        gc: Arc::new(ScriptedGc::with_cap(cap)),
        disagreement: client_loss_disagreement(),
    }
}

pub fn rig() -> Rig {
    rig_with_cap(16)
}

pub fn n() -> Duration {
    heartbeat_timing().timeout()
}

pub fn interval() -> Duration {
    heartbeat_timing().interval()
}

/// Comfortably past a declaration: N plus one heartbeat interval.
pub fn past_declaration() -> Duration {
    n() + interval()
}

pub fn settle() -> Duration {
    client_media_config().connect_settle_window + interval()
}

pub fn grant(h: &str) -> AcquireResult {
    let info = mh_handler(h);
    AcquireResult::Granted(HandlerEndpoint {
        id: HandlerId::new(info.mh_id),
        webtransport_url: info.webtransport_endpoint,
        grpc_endpoint: info.grpc_endpoint,
    })
}

pub fn unavailable() -> AcquireResult {
    AcquireResult::Unavailable {
        code: tonic::Code::Unavailable,
        message: "gc down (fixture)".to_string(),
    }
}

pub fn url(h: &str) -> String {
    mh_handler(h).webtransport_endpoint
}

impl Rig {
    pub async fn create(&self, meeting: &str, handlers: &[&str]) -> MeetingActorHandle {
        let infos: Vec<_> = handlers.iter().map(|h| mh_handler(h)).collect();
        self.c
            .create_meeting(
                meeting.to_string(),
                ::common::meeting_lifecycle::AssignmentId::new_v4(),
                handler_set(&infos),
            )
            .await
            .unwrap();
        self.c
            .get_meeting_handle(meeting.to_string())
            .await
            .unwrap()
    }

    pub fn media(&self) -> JoinMedia {
        let mh: Arc<dyn mc_service::grpc::MhRegistrationClient> = Arc::clone(&self.mh) as _;
        let mut media = join_media_configured(
            mh,
            heartbeat_timing(),
            media_dial_timing(),
            Arc::clone(&self.gc),
            self.disagreement,
        );
        media.demand = ReceiveCapabilityDeclaration::parse(
            &ReceiveCapability {
                slots: vec![slot(0, MediaKind::Audio)],
            },
            true,
        )
        .expect("valid declaration")
        .truncate(client_media_config().receive_slot_caps);
        media
    }

    pub async fn join(&self, meeting: &str, p: &str) -> mpsc::Receiver<bytes::Bytes> {
        self.join_with_set(meeting, p).await.0
    }

    /// [`Self::join`], also returning the `JoinResponse` handler set (the
    /// joiner's first set; later sets arrive on the queue).
    pub async fn join_with_set(
        &self,
        meeting: &str,
        p: &str,
    ) -> (
        mpsc::Receiver<bytes::Bytes>,
        proto_gen::dark_tower::signaling::v1::MediaHandlerSet,
    ) {
        let (tx, rx) = mpsc::channel::<bytes::Bytes>(1024);
        let reply = self
            .c
            .join_connection(
                meeting.to_string(),
                format!("conn-{p}"),
                format!("user-{p}"),
                p.to_string(),
                String::new(),
                false,
                super::test_identity_key(),
                tx,
                self.media(),
            )
            .await
            .unwrap();
        let joined = reply.await.unwrap().unwrap();
        run_ready().await;
        (rx, joined.handler_set)
    }

    pub async fn connect(&self, meeting: &str, p: &str, h: &str, conn: &str) -> u32 {
        coordination::connect(&self.svc, meeting, p, h, conn).await
    }

    /// RegisterMeeting pushes MC issued to `h`.
    pub fn registers_to(&self, h: &str) -> usize {
        let endpoint = mh_handler(h).grpc_endpoint;
        self.mh
            .events()
            .iter()
            .filter(|e| matches!(e, MhEvent::Register { mh_grpc_endpoint, .. } if *mh_grpc_endpoint == endpoint))
            .count()
    }
}

/// The handler's health label in the meeting (`None` once it left the set).
/// THE one reader the actor-level handler suites share.
pub async fn health(meeting: &MeetingActorHandle, h: &str) -> Option<&'static str> {
    meeting.probe_handler_for_test(h).await.unwrap().health
}

/// Every `MediaHandlerSet` on the queue: (urls, token audiences).
pub fn sets(queue: &[server_message::Message]) -> Vec<(Vec<String>, Vec<String>)> {
    queue
        .iter()
        .filter_map(|f| match f {
            server_message::Message::MediaHandlerSet(s) => Some((
                s.handlers
                    .iter()
                    .map(|h| h.media_handler_url.clone())
                    .collect(),
                s.handlers
                    .iter()
                    .map(|h| Token::parse(&h.media_dial_token).str("aud").to_string())
                    .collect(),
            )),
            _ => None,
        })
        .collect()
}

pub fn migrations(snap: &MetricSnapshot, trigger: &str, outcome: MigrationOutcome) -> u64 {
    snap.counter("mc_media_handler_migrations_total")
        .with_labels(&[("trigger", trigger), ("outcome", outcome.label()), KC])
        .delta()
}

/// THE counting-unit property: every acquisition attempt — one Acquire call,
/// or one owed replacement refused at the bound — records exactly one outcome.
pub fn assert_one_outcome_per_attempt(snap: &MetricSnapshot, calls: usize, bound_refusals: u64) {
    let mut total = 0;
    for t in RemovalTrigger::ALL {
        for o in MigrationOutcome::ALL {
            total += migrations(snap, t.label(), o);
        }
    }
    assert_eq!(
        total,
        calls as u64 + bound_refusals,
        "sum of outcome deltas == Acquire calls ({calls}) + bound refusals ({bound_refusals})"
    );
}
