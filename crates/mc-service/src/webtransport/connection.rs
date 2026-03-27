//! `ConnectionActor` - owns WebTransport streams and bridge loop.
//!
//! This actor lives in the webtransport layer (not the actor hierarchy) and:
//! 1. Receives a `JoinResult` from the meeting actor via oneshot
//! 2. Sends `JoinResponse` to the client over the WebTransport stream
//! 3. Runs the bridge loop forwarding `ParticipantUpdate` messages to the client
//! 4. Notifies the meeting when the connection drops (via `MeetingActorHandle`)

use crate::actors::messages::JoinResult;
use crate::actors::MeetingControllerActorHandle;
use crate::auth::McJwtValidator;
use crate::errors::McError;

use bytes::{BufMut, BytesMut};
use common::jwt::MeetingRole;
use prost::Message;
use proto_gen::signaling::{
    self, client_message, server_message, ClientMessage, ErrorMessage, JoinResponse, Participant,
    ServerMessage,
};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, instrument, warn};
use wtransport::endpoint::IncomingSession;
use wtransport::stream::{RecvStream, SendStream};

/// Maximum size for a single framed message (64KB).
const MAX_MESSAGE_SIZE: usize = 64 * 1024;

/// Maximum length for a participant display name (bytes).
const MAX_PARTICIPANT_NAME_LEN: usize = 256;

/// Channel buffer for outbound messages from ParticipantActor to WebTransport stream.
const OUTBOUND_CHANNEL_BUFFER: usize = 100;

/// Handle an incoming WebTransport connection.
///
/// This is the thin entry point: accept session, accept stream, read JoinRequest,
/// validate JWT, fire JoinConnection to controller, then hand off to the
/// connection run loop which owns the streams until disconnect.
#[instrument(skip_all, name = "mc.webtransport.connection", fields(connection_id = tracing::field::Empty))]
pub async fn handle_connection(
    incoming: IncomingSession,
    controller_handle: Arc<MeetingControllerActorHandle>,
    jwt_validator: Arc<McJwtValidator>,
    cancel_token: CancellationToken,
) -> Result<(), McError> {
    // Step 1: Accept the WebTransport session
    let session_request = incoming.await.map_err(|e| {
        warn!(
            target: "mc.webtransport.connection",
            error = %e,
            "Failed to receive session request"
        );
        McError::Internal(format!("Session request failed: {e}"))
    })?;

    let connection = session_request.accept().await.map_err(|e| {
        warn!(
            target: "mc.webtransport.connection",
            error = %e,
            "Failed to accept WebTransport session"
        );
        McError::Internal(format!("Session accept failed: {e}"))
    })?;

    let connection_id = uuid::Uuid::new_v4().to_string();
    tracing::Span::current().record("connection_id", connection_id.as_str());

    debug!(
        target: "mc.webtransport.connection",
        connection_id = %connection_id,
        "WebTransport session accepted"
    );

    // Step 2: Accept bidirectional stream
    let (mut send_stream, mut recv_stream) = connection.accept_bi().await.map_err(|e| {
        warn!(
            target: "mc.webtransport.connection",
            connection_id = %connection_id,
            error = %e,
            "Failed to accept bidirectional stream"
        );
        McError::Internal(format!("BiStream accept failed: {e}"))
    })?;

    // Step 3: Read length-prefixed ClientMessage (max 64KB)
    let client_msg = read_framed_message(&mut recv_stream).await?;

    let client_message = ClientMessage::decode(client_msg.as_ref()).map_err(|e| {
        warn!(
            target: "mc.webtransport.connection",
            connection_id = %connection_id,
            error = %e,
            "Failed to decode ClientMessage"
        );
        McError::Internal("Invalid message format".to_string())
    })?;

    // Step 4: Extract JoinRequest
    let join_request = match client_message.message {
        Some(client_message::Message::JoinRequest(req)) => req,
        _ => {
            warn!(
                target: "mc.webtransport.connection",
                connection_id = %connection_id,
                "First message was not a JoinRequest"
            );
            let _ = send_error(
                &mut send_stream,
                signaling::ErrorCode::InvalidRequest as i32,
                "First message must be JoinRequest",
            )
            .await;
            return Err(McError::Internal(
                "Expected JoinRequest as first message".to_string(),
            ));
        }
    };

    let meeting_id = join_request.meeting_id.clone();

    // Validate participant_name length
    if join_request.participant_name.len() > MAX_PARTICIPANT_NAME_LEN {
        warn!(
            target: "mc.webtransport.connection",
            connection_id = %connection_id,
            name_len = join_request.participant_name.len(),
            "Participant name exceeds maximum length"
        );
        let _ = send_error(
            &mut send_stream,
            signaling::ErrorCode::InvalidRequest as i32,
            "Participant name too long",
        )
        .await;
        return Err(McError::Internal("Participant name too long".to_string()));
    }

    debug!(
        target: "mc.webtransport.connection",
        connection_id = %connection_id,
        meeting_id = %meeting_id,
        "Received JoinRequest"
    );

    // Step 5: JWT validation BEFORE any actor interaction
    let claims = match jwt_validator
        .validate_meeting_token(&join_request.join_token)
        .await
    {
        Ok(claims) => claims,
        Err(e) => {
            warn!(
                target: "mc.webtransport.connection",
                connection_id = %connection_id,
                meeting_id = %meeting_id,
                error = %e,
                "JWT validation failed"
            );
            let _ = send_error(
                &mut send_stream,
                signaling::ErrorCode::Unauthorized as i32,
                "Invalid or expired token",
            )
            .await;
            return Err(e);
        }
    };

    info!(
        target: "mc.webtransport.connection",
        connection_id = %connection_id,
        meeting_id = %meeting_id,
        participant_type = ?claims.participant_type,
        "JWT validation succeeded"
    );

    // Step 6: meeting_id binding check
    if claims.meeting_id != meeting_id {
        warn!(
            target: "mc.webtransport.connection",
            connection_id = %connection_id,
            "Token meeting_id does not match JoinRequest meeting_id"
        );
        let _ = send_error(
            &mut send_stream,
            signaling::ErrorCode::Unauthorized as i32,
            "Invalid or expired token",
        )
        .await;
        return Err(McError::JwtValidation(
            "Token meeting_id mismatch".to_string(),
        ));
    }

    // Step 7: Create outbound channel BEFORE join so ParticipantActor is spawned with stream wired
    let is_host = claims.role == MeetingRole::Host;
    let participant_id = uuid::Uuid::new_v4().to_string();
    let (outbound_tx, mut outbound_rx) = mpsc::channel::<bytes::Bytes>(OUTBOUND_CHANNEL_BUFFER);

    let join_rx = match controller_handle
        .join_connection(
            meeting_id.clone(),
            connection_id.clone(),
            claims.sub.clone(),
            participant_id.clone(),
            is_host,
            outbound_tx,
        )
        .await
    {
        Ok(rx) => rx,
        Err(e) => {
            let error_code = e.error_code();
            let client_msg = e.client_message();
            warn!(
                target: "mc.webtransport.connection",
                connection_id = %connection_id,
                meeting_id = %meeting_id,
                error = %e,
                "Failed to send join to controller"
            );
            let _ = send_error(&mut send_stream, error_code, &client_msg).await;
            return Err(e);
        }
    };

    // Await the join result from the meeting actor
    let join_result = match join_rx.await {
        Ok(Ok(result)) => result,
        Ok(Err(e)) => {
            let error_code = e.error_code();
            let client_msg = e.client_message();
            warn!(
                target: "mc.webtransport.connection",
                connection_id = %connection_id,
                meeting_id = %meeting_id,
                error = %e,
                "Join failed"
            );
            let _ = send_error(&mut send_stream, error_code, &client_msg).await;
            return Err(e);
        }
        Err(_) => {
            warn!(
                target: "mc.webtransport.connection",
                connection_id = %connection_id,
                meeting_id = %meeting_id,
                "Join response channel dropped"
            );
            let _ = send_error(
                &mut send_stream,
                signaling::ErrorCode::InternalError as i32,
                "Internal error",
            )
            .await;
            return Err(McError::Internal(
                "Join response channel dropped".to_string(),
            ));
        }
    };

    info!(
        target: "mc.webtransport.connection",
        connection_id = %connection_id,
        meeting_id = %meeting_id,
        participant_id = %join_result.participant_id,
        "Join succeeded"
    );

    // Step 8: Build and send JoinResponse
    let join_response = build_join_response(&join_result);
    let server_msg = ServerMessage {
        message: Some(server_message::Message::JoinResponse(join_response)),
    };

    if let Err(e) = write_framed_message(&mut send_stream, &server_msg).await {
        warn!(
            target: "mc.webtransport.connection",
            connection_id = %connection_id,
            error = %e,
            "Failed to send JoinResponse"
        );
        // ParticipantActor will detect disconnect via its handle
        return Err(e);
    }

    debug!(
        target: "mc.webtransport.connection",
        connection_id = %connection_id,
        "JoinResponse sent"
    );

    // Step 9: Run bridge loop — forward ParticipantActor updates to client
    // outbound_tx was passed through the join flow and is now owned by ParticipantActor.
    // outbound_rx receives encoded protobuf bytes written by ParticipantActor.
    let bridge_result = run_bridge_loop(
        &mut send_stream,
        &mut recv_stream,
        &mut outbound_rx,
        &cancel_token,
        &connection_id,
    )
    .await;

    // Cancel the ParticipantActor — it will notify the meeting of disconnect on exit
    info!(
        target: "mc.webtransport.connection",
        connection_id = %connection_id,
        meeting_id = %meeting_id,
        participant_id = %join_result.participant_id,
        "Connection closing, cancelling ParticipantActor"
    );
    join_result.participant_handle.cancel();

    bridge_result
}

/// Run the bridge loop: forward outbound messages to the WebTransport stream.
///
/// Exits when:
/// - Cancellation token is triggered
/// - Outbound channel is closed (ParticipantActor stopped)
/// - WebTransport stream errors
/// - Client closes their end of the stream
async fn run_bridge_loop(
    send_stream: &mut SendStream,
    recv_stream: &mut RecvStream,
    outbound_rx: &mut mpsc::Receiver<bytes::Bytes>,
    cancel_token: &CancellationToken,
    connection_id: &str,
) -> Result<(), McError> {
    let mut probe_buf = [0u8; 1];
    loop {
        tokio::select! {
            () = cancel_token.cancelled() => {
                debug!(
                    target: "mc.webtransport.connection",
                    connection_id = %connection_id,
                    "Bridge loop cancelled"
                );
                break;
            }

            msg = outbound_rx.recv() => {
                match msg {
                    Some(data) => {
                        if let Err(e) = write_raw_framed(send_stream, &data).await {
                            warn!(
                                target: "mc.webtransport.connection",
                                connection_id = %connection_id,
                                error = %e,
                                "Failed to write outbound message"
                            );
                            return Err(e);
                        }
                    }
                    None => {
                        debug!(
                            target: "mc.webtransport.connection",
                            connection_id = %connection_id,
                            "Outbound channel closed, ending bridge loop"
                        );
                        break;
                    }
                }
            }

            // Monitor client stream for closure
            result = recv_stream.read(&mut probe_buf) => {
                match result {
                    Ok(None) | Err(_) => {
                        debug!(
                            target: "mc.webtransport.connection",
                            connection_id = %connection_id,
                            "Client stream closed"
                        );
                        break;
                    }
                    Ok(Some(_)) => {
                        // Client sent data after join — ignore for now
                    }
                }
            }
        }
    }

    Ok(())
}

/// Read a length-prefixed protobuf message from a `RecvStream`.
///
/// Wire format: 4-byte big-endian length prefix + protobuf bytes.
/// Enforces `MAX_MESSAGE_SIZE` (64KB) to prevent abuse.
async fn read_framed_message(stream: &mut RecvStream) -> Result<bytes::Bytes, McError> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await.map_err(|e| {
        warn!(
            target: "mc.webtransport.connection",
            error = %e,
            "Failed to read message length prefix"
        );
        McError::Internal("Failed to read message".to_string())
    })?;

    let msg_len = u32::from_be_bytes(len_buf) as usize;

    if msg_len > MAX_MESSAGE_SIZE {
        warn!(
            target: "mc.webtransport.connection",
            msg_len = msg_len,
            max = MAX_MESSAGE_SIZE,
            "Message exceeds maximum size"
        );
        return Err(McError::Internal("Message too large".to_string()));
    }

    if msg_len == 0 {
        return Err(McError::Internal("Empty message".to_string()));
    }

    let mut buf = vec![0u8; msg_len];
    stream.read_exact(&mut buf).await.map_err(|e| {
        warn!(
            target: "mc.webtransport.connection",
            error = %e,
            msg_len = msg_len,
            "Failed to read message body"
        );
        McError::Internal("Failed to read message body".to_string())
    })?;

    Ok(bytes::Bytes::from(buf))
}

/// Write a length-prefixed protobuf message to a `SendStream`.
async fn write_framed_message(stream: &mut SendStream, msg: &ServerMessage) -> Result<(), McError> {
    let encoded = msg.encode_to_vec();
    write_raw_framed(stream, &encoded).await
}

/// Write raw bytes with 4-byte big-endian length prefix.
async fn write_raw_framed(stream: &mut SendStream, data: &[u8]) -> Result<(), McError> {
    let len: u32 = data
        .len()
        .try_into()
        .map_err(|_| McError::Internal("Message too large to frame".to_string()))?;
    let mut frame = BytesMut::with_capacity(4 + data.len());
    frame.put_u32(len);
    frame.put_slice(data);

    stream
        .write_all(&frame)
        .await
        .map_err(|e| McError::Internal(format!("Stream write failed: {e}")))
}

/// Send an error message to the client before closing.
async fn send_error(
    stream: &mut SendStream,
    error_code: i32,
    message: &str,
) -> Result<(), McError> {
    let server_msg = ServerMessage {
        message: Some(server_message::Message::Error(ErrorMessage {
            code: error_code,
            message: message.to_string(),
            details: Default::default(),
        })),
    };
    write_framed_message(stream, &server_msg).await
}

/// Build a protobuf `JoinResponse` from the actor's `JoinResult`.
fn build_join_response(result: &JoinResult) -> JoinResponse {
    let existing_participants = result
        .participants
        .iter()
        .map(|p| Participant {
            participant_id: p.participant_id.clone(),
            name: p.display_name.clone(),
            streams: Vec::new(),
            joined_at: 0,
        })
        .collect();

    JoinResponse {
        participant_id: result.participant_id.clone(),
        user_id: 0,
        existing_participants,
        media_servers: Vec::new(),
        encryption_keys: None,
        correlation_id: result.correlation_id.clone(),
        binding_token: result.binding_token.clone(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::actors::messages::{ParticipantInfo, ParticipantStatus};

    fn make_participant_info(id: &str, name: &str) -> ParticipantInfo {
        ParticipantInfo {
            participant_id: id.to_string(),
            user_id: "user-1".to_string(),
            display_name: name.to_string(),
            audio_self_muted: false,
            video_self_muted: false,
            audio_host_muted: false,
            video_host_muted: false,
            status: ParticipantStatus::Connected,
        }
    }

    fn dummy_participant_handle() -> crate::actors::participant::ParticipantActorHandle {
        use crate::actors::metrics::ActorMetrics;
        let cancel_token = tokio_util::sync::CancellationToken::new();
        let metrics = ActorMetrics::new();
        let (handle, _task) = crate::actors::participant::ParticipantActor::spawn(
            "test-conn".to_string(),
            "test-part".to_string(),
            "test-meeting".to_string(),
            cancel_token,
            metrics,
        );
        handle
    }

    #[test]
    fn test_build_join_response_basic() {
        let _rt = tokio::runtime::Runtime::new().unwrap();
        let _guard = _rt.enter();
        let result = JoinResult {
            participant_id: "part-100".to_string(),
            correlation_id: "corr-abc".to_string(),
            binding_token: "token-xyz".to_string(),
            participants: vec![
                make_participant_info("part-1", "Alice"),
                make_participant_info("part-2", "Bob"),
            ],
            fencing_generation: 1,
            participant_handle: dummy_participant_handle(),
        };

        let response = build_join_response(&result);
        assert_eq!(response.participant_id, "part-100");
        assert_eq!(response.correlation_id, "corr-abc");
        assert_eq!(response.binding_token, "token-xyz");
        assert_eq!(response.existing_participants.len(), 2);
        assert_eq!(response.existing_participants[0].participant_id, "part-1");
        assert_eq!(response.existing_participants[0].name, "Alice");
        assert_eq!(response.existing_participants[1].participant_id, "part-2");
        assert_eq!(response.existing_participants[1].name, "Bob");
    }

    #[test]
    fn test_build_join_response_empty_participants() {
        let _rt = tokio::runtime::Runtime::new().unwrap();
        let _guard = _rt.enter();
        let result = JoinResult {
            participant_id: "part-solo".to_string(),
            correlation_id: "corr-1".to_string(),
            binding_token: "token-1".to_string(),
            participants: vec![],
            fencing_generation: 1,
            participant_handle: dummy_participant_handle(),
        };

        let response = build_join_response(&result);
        assert_eq!(response.participant_id, "part-solo");
        assert!(response.existing_participants.is_empty());
    }
}
