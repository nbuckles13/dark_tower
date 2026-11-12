//! The MH -> MC coordination calls as an integration test makes them, through
//! the REAL gRPC service (`McMediaCoordinationService`): the
//! `NotifyParticipant{Connected,Disconnected}` builders and the
//! `MeetingHeartbeat` request + verdict decode. One home for every suite that
//! drives the service (heartbeat intake, detection).

use mc_service::grpc::McMediaCoordinationService;
use proto_gen::dark_tower::internal::v1::media_coordination_service_server::MediaCoordinationService;
use proto_gen::dark_tower::internal::v1::{
    ApplyRefusal, HeartbeatVerdict, MeetingHeartbeatRequest, NotifyParticipantConnectedRequest,
    NotifyParticipantDisconnectedRequest, ReleaseCause, ServingState,
};
use tonic::{Code, Request};

/// A SERVING heartbeat from `handler` for `meeting`, owned by `mc`.
pub fn heartbeat_request(
    mc: &str,
    meeting: &str,
    handler: &str,
    applied: u64,
    refusal: ApplyRefusal,
    ids: &[&str],
) -> MeetingHeartbeatRequest {
    MeetingHeartbeatRequest {
        meeting_id: meeting.to_string(),
        mc_id: mc.to_string(),
        handler_id: handler.to_string(),
        serving_state: ServingState::Serving as i32,
        drain_remaining_ms: 0,
        applied_generation: applied,
        apply_refusal: refusal as i32,
        connection_ids: ids.iter().map(ToString::to_string).collect(),
    }
}

/// Send one heartbeat; `Ok((verdict, cause))` or the gRPC code.
pub async fn beat(
    svc: &McMediaCoordinationService,
    req: MeetingHeartbeatRequest,
) -> Result<(HeartbeatVerdict, ReleaseCause), Code> {
    svc.meeting_heartbeat(Request::new(req))
        .await
        .map(|r| {
            let r = r.into_inner();
            (
                HeartbeatVerdict::try_from(r.verdict).unwrap(),
                ReleaseCause::try_from(r.cause).unwrap(),
            )
        })
        .map_err(|s| s.code())
}

/// `NotifyParticipantConnected` for participant `p` (token sub `user-p`);
/// the sender id MC answered.
pub async fn connect(
    svc: &McMediaCoordinationService,
    meeting: &str,
    p: &str,
    h: &str,
    conn: &str,
) -> u32 {
    svc.notify_participant_connected(Request::new(NotifyParticipantConnectedRequest {
        meeting_id: meeting.to_string(),
        participant_id: format!("user-{p}"),
        handler_id: h.to_string(),
        connection_id: conn.to_string(),
    }))
    .await
    .unwrap()
    .into_inner()
    .sender_id
}

/// `NotifyParticipantDisconnected` for participant `p` (token sub `user-p`).
pub async fn disconnect(
    svc: &McMediaCoordinationService,
    meeting: &str,
    p: &str,
    h: &str,
    conn: &str,
) {
    svc.notify_participant_disconnected(Request::new(NotifyParticipantDisconnectedRequest {
        meeting_id: meeting.to_string(),
        participant_id: format!("user-{p}"),
        handler_id: h.to_string(),
        reason: 1,
        connection_id: conn.to_string(),
    }))
    .await
    .unwrap();
}
