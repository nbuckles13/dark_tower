//! The MH -> MC heartbeat rig (story 4 MH-2, MH-3): the PRODUCTION session
//! actor with a heartbeat sink and the PRODUCTION sender task, behind the real
//! `MhMediaService`; only the RPC is faked, through the `HeartbeatTransport`
//! seam (paused time over a real socket lets tokio auto-advance the clock
//! mid-read and fire deadlines spuriously). Time moves only through
//! [`run_for`] — small steps with yields, never a sleep.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mh_service::config::PolicyLimits;
use mh_service::drain::DrainDeadline;
use mh_service::grpc::heartbeat_sender::{HeartbeatError, HeartbeatSender, HeartbeatTransport};
use mh_service::grpc::MhMediaService;
use mh_service::session::heartbeat::{HeartbeatSetup, SenderCommand};
use mh_service::session::{AdmitOutcome, ConnectionEntry, SessionManagerHandle, StreamAdmission};
use mh_test_utils::media_policy::register_request;
use proto_gen::dark_tower::internal::v1::media_handler_service_server::MediaHandlerService;
use proto_gen::dark_tower::internal::v1::{
    EndMeetingRequest, HeartbeatVerdict, MeetingHeartbeatRequest, MeetingHeartbeatResponse,
    RegisterMeetingRequest,
};
use rand::rngs::SmallRng;
use rand::SeedableRng;
use tokio::sync::{mpsc, watch, Notify};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use tonic::Request;

pub const HANDLER_ID: &str = "mh-heartbeat-test";
pub const SEED: u64 = 0x4D48_2D32;
/// The owner-silence timeout every rig uses: the real config floor
/// (`MEETING_OWNER_SILENCE_TIMEOUT_SECONDS_RANGE.0`), never a test-only value.
pub const OWNER_SILENCE: Duration =
    Duration::from_secs(mh_service::config::MEETING_OWNER_SILENCE_TIMEOUT_SECONDS_RANGE.0);

// ---------------------------------------------------------------------------
// Fake MC transport
// ---------------------------------------------------------------------------

/// How the fake MC answers one meeting's beats.
#[derive(Clone)]
pub enum Reply {
    Owned,
    Released(i32),
    /// A raw verdict number (UNSPECIFIED / unknown).
    Verdict(i32),
    Status(tonic::Code),
    /// Never answers.
    Hang,
    /// Wait for the notify, then answer with the inner reply.
    HoldThen(Arc<Notify>, Box<Reply>),
}

#[derive(Default)]
pub struct FakeMc {
    pub replies: Mutex<HashMap<String, Reply>>,
    pub calls: Mutex<Vec<(Instant, String, MeetingHeartbeatRequest)>>,
}

impl FakeMc {
    pub fn reply(&self, meeting: &str, reply: Reply) {
        self.replies
            .lock()
            .unwrap()
            .insert(meeting.to_string(), reply);
    }

    pub fn calls_for(&self, meeting: &str) -> Vec<(Instant, MeetingHeartbeatRequest)> {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, _, r)| r.meeting_id == meeting)
            .map(|(t, _, r)| (*t, r.clone()))
            .collect()
    }

    pub fn count(&self, meeting: &str) -> usize {
        self.calls_for(meeting).len()
    }

    pub fn last(&self, meeting: &str) -> MeetingHeartbeatRequest {
        let mut calls = self.calls_for(meeting);
        assert!(!calls.is_empty(), "{meeting}: no heartbeat was sent");
        calls.pop().expect("non-empty, asserted above").1
    }

    pub fn endpoint_of_last(&self, meeting: &str) -> String {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|(_, _, r)| r.meeting_id == meeting)
            .map(|(_, e, _)| e.clone())
            .unwrap()
    }
}

pub fn answer(reply: &Reply) -> Result<MeetingHeartbeatResponse, HeartbeatError> {
    match reply {
        Reply::Owned => Ok(MeetingHeartbeatResponse {
            verdict: HeartbeatVerdict::Owned as i32,
            cause: 0,
        }),
        Reply::Released(cause) => Ok(MeetingHeartbeatResponse {
            verdict: HeartbeatVerdict::Released as i32,
            cause: *cause,
        }),
        Reply::Verdict(raw) => Ok(MeetingHeartbeatResponse {
            verdict: *raw,
            cause: 0,
        }),
        Reply::Status(code) => Err(HeartbeatError::Status(*code)),
        Reply::Hang | Reply::HoldThen(..) => unreachable!("resolved by the caller"),
    }
}

impl HeartbeatTransport for FakeMc {
    async fn send(
        &self,
        endpoint: &str,
        request: MeetingHeartbeatRequest,
    ) -> Result<MeetingHeartbeatResponse, HeartbeatError> {
        let reply = self
            .replies
            .lock()
            .unwrap()
            .get(&request.meeting_id)
            .cloned()
            .unwrap_or(Reply::Owned);
        self.calls
            .lock()
            .unwrap()
            .push((Instant::now(), endpoint.to_string(), request));
        match reply {
            Reply::Hang => std::future::pending().await,
            Reply::HoldThen(notify, inner) => {
                notify.notified().await;
                answer(&inner)
            }
            other => answer(&other),
        }
    }

    fn evict(&self, _endpoint: &str) {}
}

// ---------------------------------------------------------------------------
// Rig
// ---------------------------------------------------------------------------

pub struct Rig {
    pub sm: SessionManagerHandle,
    pub svc: MhMediaService,
    pub mc: Arc<FakeMc>,
    pub shutdown: CancellationToken,
    /// The process drain state the actor reads (story 4 MH-3). Shared so a
    /// spawned `drain::run` can hold it.
    pub drain: Arc<watch::Sender<Option<DrainDeadline>>>,
}

impl Drop for Rig {
    fn drop(&mut self) {
        self.shutdown.cancel();
    }
}

/// The production `HeartbeatSetup` shape for a test actor: the fixed id and
/// seed, a not-draining drain state, and the real owner-silence floor.
/// Returns the drain sender so a test can start a drain.
pub fn heartbeat_setup(
    commands: mpsc::Sender<SenderCommand>,
) -> (HeartbeatSetup, watch::Sender<Option<DrainDeadline>>) {
    let (drain_tx, drain_rx) = mh_service::drain::channel();
    (
        HeartbeatSetup {
            handler_id: HANDLER_ID.to_string(),
            commands,
            rng: SmallRng::seed_from_u64(SEED),
            drain: drain_rx,
            owner_silence_timeout: OWNER_SILENCE,
        },
        drain_tx,
    )
}

pub fn rig_with(admission: StreamAdmission, limits: PolicyLimits) -> Rig {
    let (tx, rx) = mpsc::channel(mh_service::session::heartbeat::HEARTBEAT_QUEUE_CAPACITY);
    let (setup, drain) = heartbeat_setup(tx);
    let (sm, actor) = SessionManagerHandle::new_with_heartbeat_for_tests(admission, setup);
    tokio::spawn(actor.run());
    let mc = Arc::new(FakeMc::default());
    let shutdown = CancellationToken::new();
    tokio::spawn(
        HeartbeatSender::new(Arc::clone(&mc), rx, sm.verdict_sink(), 64).run(shutdown.clone()),
    );
    let svc = MhMediaService::new(sm.clone(), HANDLER_ID.to_string(), limits);
    Rig {
        sm,
        svc,
        mc,
        shutdown,
        drain: Arc::new(drain),
    }
}

pub fn rig() -> Rig {
    rig_with(
        mh_test_utils::admission::never_binding(),
        mh_test_utils::admission::fixture_policy_limits(),
    )
}

pub async fn register(rig: &Rig, request: RegisterMeetingRequest) -> Result<(), tonic::Status> {
    rig.svc
        .register_meeting(Request::new(request))
        .await
        .map(|_| ())
}

/// A valid, edge-free registration.
pub fn plain(meeting: &str) -> RegisterMeetingRequest {
    register_request(meeting, 1, Vec::new())
}

pub fn end(meeting: &str, mc_id: &str, cause: i32) -> Request<EndMeetingRequest> {
    Request::new(EndMeetingRequest {
        meeting_id: meeting.to_string(),
        mc_id: mc_id.to_string(),
        cause,
    })
}

pub fn connection(id: &str, participant: &str) -> ConnectionEntry {
    ConnectionEntry {
        connection_id: id.to_string(),
        participant_id: participant.to_string(),
        connected_at: std::time::Instant::now(),
        close: CancellationToken::new(),
        is_bound: false,
    }
}

/// Admit `entry` into `meeting` (awaiting its bind), asserting it was admitted.
pub async fn admit(sm: &SessionManagerHandle, meeting: &str, entry: ConnectionEntry) {
    assert_eq!(
        sm.admit_connection(
            meeting,
            entry,
            mh_test_utils::media_dial::meeting_dial_key(),
            8
        )
        .await,
        AdmitOutcome::Admitted,
        "{meeting}: fixture admission"
    );
}

pub const HEARTBEATS: &str = "mh_mc_heartbeats_total";
pub const VERDICTS: &str = "mh_mc_heartbeat_verdicts_total";
pub const RELEASES: &str = "mh_media_handler_releases_total";
pub const TEARDOWNS: &str = "mh_media_meeting_teardowns_total";
pub const KC: (&str, &str) = ("key_custody", "operator");
