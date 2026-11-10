//! Shared fixtures for the MC `media_dial` suites (ADR-0039 §5; story 4
//! MC-D): an actor-level meeting with the dial seams (anchored clock, seeded
//! jitter), joins with a non-default TTL, the wire key recorded by the mock
//! MH, and an INDEPENDENT token oracle (`ring`, never MC's own code).
//!
//! Tokens are never printed: callers compare fields, and failure messages
//! name the participant or handler, never the token.

use std::collections::BTreeMap;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;

use ::common::secret::SecretBox;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use mc_service::actors::{
    ActorMetrics, ControllerMetrics, JoinMedia, JoinResult, MeetingActor, MeetingActorHandle,
    MeetingSeams,
};
use mc_service::errors::McError;
use mc_service::media_dial::status::{ClientHandlerReport, ReportedState};
use mc_service::media_dial::MediaDialTiming;
use mc_service::redis::MhEndpointInfo;
use proto_gen::dark_tower::signaling::v1::{server_message, MediaHandlerSet};
use ring::signature::{UnparsedPublicKey, ED25519};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use super::{handler_set, MockMhRegistrationClient};

pub const BASE_UNIX: i64 = 1_800_000_000;
pub const TTL: u32 = 120;
pub const H_A: &str = "mh-a";
pub const H_B: &str = "mh-b";

pub fn timing() -> MediaDialTiming {
    MediaDialTiming::from_ttl_seconds(TTL)
}

pub fn seams() -> MeetingSeams {
    MeetingSeams {
        dial_refresh_seed: Some(0x5eed_0d1a),
        dial_clock_base_unix: Some(BASE_UNIX),
        ..MeetingSeams::default()
    }
}

pub fn spawn_meeting(meeting_id: &str, handlers: &[MhEndpointInfo]) -> MeetingActorHandle {
    spawn_meeting_with(meeting_id, handlers, &seams())
}

/// [`spawn_meeting`] with explicit seams (e.g. a clock anchored near the end
/// of time, to force a mint failure).
pub fn spawn_meeting_with(
    meeting_id: &str,
    handlers: &[MhEndpointInfo],
    seams: &MeetingSeams,
) -> MeetingActorHandle {
    let (handle, _task) = MeetingActor::spawn_with_seams(
        meeting_id.to_string(),
        CancellationToken::new(),
        ActorMetrics::new(),
        ControllerMetrics::new(),
        SecretBox::new(Box::new(vec![0u8; 32])),
        mc_test_utils::kek::kek_lifecycle(),
        handler_set(handlers),
        seams,
    )
    .expect("system CSPRNG must be available in tests");
    handle
}

pub fn media(mh: &Arc<MockMhRegistrationClient>) -> JoinMedia {
    super::join_media_with_timing(
        Arc::clone(mh) as Arc<dyn mc_service::grpc::MhRegistrationClient>,
        super::heartbeat_timing(),
        timing(),
    )
}

/// A joined participant: its join result and its outbound queue.
pub struct Joined {
    pub result: JoinResult,
    pub rx: mpsc::Receiver<bytes::Bytes>,
}

/// Participant `id` joins as user `user-{id}` (participant id and user id
/// differ, so a participant_id-vs-user_id mix-up in `sub` is caught).
pub async fn join(
    handle: &MeetingActorHandle,
    id: &str,
    mh: &Arc<MockMhRegistrationClient>,
) -> Joined {
    let (tx, rx) = mpsc::channel::<bytes::Bytes>(512);
    let h = handle.clone();
    let pid = id.to_string();
    let media = media(mh);
    let reply: JoinHandle<Result<JoinResult, McError>> = tokio::spawn(async move {
        h.connection_join(
            format!("conn-{pid}"),
            format!("user-{pid}"),
            pid.clone(),
            String::new(),
            false,
            super::test_identity_key(),
            Some(tx),
            media,
        )
        .await
    });
    let result = reply.await.unwrap().expect("join succeeds");
    run_ready().await;
    Joined { result, rx }
}

/// Let every runnable task run WITHOUT advancing paused time.
pub async fn run_ready() {
    for _ in 0..200 {
        tokio::task::yield_now().await;
    }
}

pub async fn advance(d: Duration) {
    tokio::time::advance(d).await;
    run_ready().await;
}

/// Every `MediaHandlerSet` queued so far for this participant, in order.
pub fn sets(rx: &mut mpsc::Receiver<bytes::Bytes>) -> Vec<MediaHandlerSet> {
    super::drain_frames(rx)
        .into_iter()
        .filter_map(|m| match m {
            server_message::Message::MediaHandlerSet(s) => Some(s),
            _ => None,
        })
        .collect()
}

/// The meeting's wire key: the one `media_dial_public_key` every
/// `RegisterMeeting` for it carried.
pub fn wire_key(mh: &MockMhRegistrationClient, meeting_id: &str) -> [u8; 32] {
    let keys: Vec<[u8; 32]> = mh
        .calls()
        .into_iter()
        .filter(|c| c.meeting_id == meeting_id)
        .map(|c| c.media_dial_public_key)
        .collect();
    let first = *keys
        .first()
        .unwrap_or_else(|| panic!("{meeting_id}: RegisterMeeting was sent"));
    assert!(
        keys.iter().all(|k| *k == first),
        "{meeting_id}: every RegisterMeeting carries the same key"
    );
    first
}

/// A decoded compact JWS (header and claims as JSON), and whether it verifies.
pub struct Token {
    pub header: serde_json::Value,
    pub claims: serde_json::Value,
    signing_input: String,
    signature: Vec<u8>,
}

impl Token {
    pub fn parse(token: &str) -> Self {
        let mut parts = token.split('.');
        let (Some(header), Some(claims), Some(signature), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            panic!("a compact JWS has three segments");
        };
        Self {
            header: serde_json::from_slice(&URL_SAFE_NO_PAD.decode(header).unwrap()).unwrap(),
            claims: serde_json::from_slice(&URL_SAFE_NO_PAD.decode(claims).unwrap()).unwrap(),
            signing_input: format!("{header}.{claims}"),
            signature: URL_SAFE_NO_PAD.decode(signature).unwrap(),
        }
    }

    pub fn verifies_under(&self, key: &[u8; 32]) -> bool {
        UnparsedPublicKey::new(&ED25519, key)
            .verify(self.signing_input.as_bytes(), &self.signature)
            .is_ok()
    }

    pub fn i64(&self, claim: &str) -> i64 {
        self.claims
            .get(claim)
            .and_then(serde_json::Value::as_i64)
            .expect("integer claim")
    }

    pub fn str(&self, claim: &str) -> &str {
        self.claims
            .get(claim)
            .and_then(serde_json::Value::as_str)
            .expect("string claim")
    }
}

/// url -> handler id for the fixture's handlers.
pub fn handler_of_url(handlers: &[MhEndpointInfo]) -> BTreeMap<String, String> {
    handlers
        .iter()
        .map(|h| (h.webtransport_endpoint.clone(), h.mh_id.clone()))
        .collect()
}

pub fn now_unix(start: tokio::time::Instant) -> i64 {
    BASE_UNIX + i64::try_from(start.elapsed().as_secs()).unwrap()
}

pub fn push_cell<'a>(
    snap: &'a ::common::observability::testing::MetricSnapshot,
    reason: &'static str,
) -> ::common::observability::testing::CounterQuery<'a> {
    snap.counter("mc_media_handler_set_pushes_total")
        .with_labels(&[("reason", reason), ("key_custody", "operator")])
}

pub fn report(handler_ref: u32, version: u64, state: ReportedState) -> ClientHandlerReport {
    ClientHandlerReport {
        handler_ref: NonZeroU32::new(handler_ref).unwrap(),
        handler_set_version: version,
        state,
    }
}
