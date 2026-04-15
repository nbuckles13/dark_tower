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
use crate::grpc::MhRegistrationClient;
use crate::observability::metrics;
use crate::redis::{MhAssignmentData, MhAssignmentStore};

use bytes::{BufMut, BytesMut};
use common::jwt::MeetingRole;
use prost::Message;
use proto_gen::signaling::{
    self, client_message, server_message, ClientMessage, ErrorMessage, JoinResponse,
    MediaServerInfo, Participant, ServerMessage,
};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, instrument, warn, Instrument};
use wtransport::endpoint::IncomingSession;
use wtransport::stream::{RecvStream, SendStream};

/// Maximum size for a single framed message (64KB).
const MAX_MESSAGE_SIZE: usize = 64 * 1024;

/// Maximum length for a participant display name (bytes).
const MAX_PARTICIPANT_NAME_LEN: usize = 256;

/// Channel buffer for outbound messages from ParticipantActor to WebTransport stream.
const OUTBOUND_CHANNEL_BUFFER: usize = 100;

/// Maximum number of attempts for RegisterMeeting RPC per MH.
const MAX_REGISTER_ATTEMPTS: u32 = 3;

/// Backoff delays between RegisterMeeting retry attempts.
const REGISTER_BACKOFF_DELAYS: [Duration; 2] = [Duration::from_secs(1), Duration::from_secs(2)];

/// Handle an incoming WebTransport connection.
///
/// This is the thin entry point: accept session, accept stream, read JoinRequest,
/// validate JWT, fire JoinConnection to controller, then hand off to the
/// connection run loop which owns the streams until disconnect.
#[instrument(skip_all, name = "mc.webtransport.connection", fields(connection_id = tracing::field::Empty))]
#[expect(
    clippy::too_many_arguments,
    reason = "Connection handler wiring; all params are distinct dependencies"
)]
pub async fn handle_connection(
    incoming: IncomingSession,
    controller_handle: Arc<MeetingControllerActorHandle>,
    jwt_validator: Arc<McJwtValidator>,
    redis_client: Arc<dyn MhAssignmentStore>,
    mh_client: Arc<dyn MhRegistrationClient>,
    mc_id: String,
    mc_grpc_endpoint: String,
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

    // Start join duration timer after session accept (excludes QUIC handshake)
    let join_start = Instant::now();

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
        let err = McError::Internal(format!("BiStream accept failed: {e}"));
        metrics::record_session_join(
            "failure",
            Some(err.error_type_label()),
            join_start.elapsed(),
        );
        err
    })?;

    // Step 3: Read length-prefixed ClientMessage (max 64KB)
    let client_msg = match read_framed_message(&mut recv_stream).await {
        Ok(msg) => msg,
        Err(e) => {
            metrics::record_session_join(
                "failure",
                Some(e.error_type_label()),
                join_start.elapsed(),
            );
            return Err(e);
        }
    };

    let client_message = ClientMessage::decode(client_msg.as_ref()).map_err(|e| {
        warn!(
            target: "mc.webtransport.connection",
            connection_id = %connection_id,
            error = %e,
            "Failed to decode ClientMessage"
        );
        let err = McError::Internal("Invalid message format".to_string());
        metrics::record_session_join(
            "failure",
            Some(err.error_type_label()),
            join_start.elapsed(),
        );
        err
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
            let err = McError::Internal("Expected JoinRequest as first message".to_string());
            metrics::record_session_join(
                "failure",
                Some(err.error_type_label()),
                join_start.elapsed(),
            );
            return Err(err);
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
        let err = McError::Internal("Participant name too long".to_string());
        metrics::record_session_join(
            "failure",
            Some(err.error_type_label()),
            join_start.elapsed(),
        );
        return Err(err);
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
            metrics::record_jwt_validation("failure", "meeting", "signature_invalid");
            let _ = send_error(
                &mut send_stream,
                signaling::ErrorCode::Unauthorized as i32,
                "Invalid or expired token",
            )
            .await;
            metrics::record_session_join(
                "failure",
                Some(e.error_type_label()),
                join_start.elapsed(),
            );
            return Err(e);
        }
    };

    metrics::record_jwt_validation("success", "meeting", "none");

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
        let err = McError::JwtValidation("Token meeting_id mismatch".to_string());
        metrics::record_session_join(
            "failure",
            Some(err.error_type_label()),
            join_start.elapsed(),
        );
        return Err(err);
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
            metrics::record_session_join(
                "failure",
                Some(e.error_type_label()),
                join_start.elapsed(),
            );
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
            metrics::record_session_join(
                "failure",
                Some(e.error_type_label()),
                join_start.elapsed(),
            );
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
            let err = McError::Internal("Join response channel dropped".to_string());
            metrics::record_session_join(
                "failure",
                Some(err.error_type_label()),
                join_start.elapsed(),
            );
            return Err(err);
        }
    };

    info!(
        target: "mc.webtransport.connection",
        connection_id = %connection_id,
        meeting_id = %meeting_id,
        participant_id = %join_result.participant_id,
        "Join succeeded"
    );

    // Step 8: Build and send JoinResponse (reads MH assignment data from Redis)
    let (join_response, mh_data) =
        match build_join_response(&join_result, redis_client.as_ref(), &meeting_id).await {
            Ok(resp) => resp,
            Err(e) => {
                warn!(
                    target: "mc.webtransport.connection",
                    connection_id = %connection_id,
                    meeting_id = %meeting_id,
                    error = %e,
                    "Failed to build JoinResponse"
                );
                join_result.participant_handle.cancel();
                let _ = send_error(&mut send_stream, e.error_code(), &e.client_message()).await;
                metrics::record_session_join(
                    "failure",
                    Some(e.error_type_label()),
                    join_start.elapsed(),
                );
                return Err(e);
            }
        };
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
        metrics::record_session_join("failure", Some(e.error_type_label()), join_start.elapsed());
        // ParticipantActor will detect disconnect via its handle
        return Err(e);
    }

    // Join flow complete — record success metrics
    metrics::record_session_join("success", None, join_start.elapsed());

    debug!(
        target: "mc.webtransport.connection",
        connection_id = %connection_id,
        "JoinResponse sent"
    );

    // Step 9: [ASYNC, first participant only] Fire RegisterMeeting to each MH (R-12)
    let is_first_participant = join_result.participants.is_empty();
    if is_first_participant {
        debug!(
            target: "mc.webtransport.connection",
            connection_id = %connection_id,
            meeting_id = %meeting_id,
            "First participant joined — spawning async RegisterMeeting"
        );
        let reg_mh_client = Arc::clone(&mh_client);
        let reg_meeting_id = meeting_id.clone();
        let reg_mc_id = mc_id.clone();
        let reg_mc_grpc_endpoint = mc_grpc_endpoint.clone();
        let reg_cancel_token = cancel_token.child_token();
        let span = tracing::info_span!(
            target: "mc.register_meeting.trigger",
            "register_meeting_trigger",
            meeting_id = %meeting_id,
        );
        tokio::spawn(
            async move {
                register_meeting_with_handlers(
                    reg_mh_client.as_ref(),
                    &mh_data,
                    &reg_meeting_id,
                    &reg_mc_id,
                    &reg_mc_grpc_endpoint,
                    &reg_cancel_token,
                )
                .await;
            }
            .instrument(span),
        );
    } else {
        debug!(
            target: "mc.webtransport.connection",
            connection_id = %connection_id,
            meeting_id = %meeting_id,
            "Not first participant — skipping RegisterMeeting"
        );
    }

    // Step 10: Run bridge loop — forward ParticipantActor updates to client
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

            // Read client messages (framed protobuf)
            result = read_framed_message(recv_stream) => {
                match result {
                    Ok(data) => {
                        handle_client_message(&data, connection_id);
                    }
                    Err(_) => {
                        debug!(
                            target: "mc.webtransport.connection",
                            connection_id = %connection_id,
                            "Client stream closed or read error"
                        );
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}

/// Handle a post-join client message in the bridge loop.
///
/// Currently handles:
/// - `MediaConnectionFailed` (R-20): Log warning + record metric. No reallocation.
/// - All other messages: Ignored (logged at debug level).
fn handle_client_message(data: &[u8], connection_id: &str) {
    let Ok(client_message) = ClientMessage::decode(data) else {
        debug!(
            target: "mc.webtransport.connection",
            connection_id = %connection_id,
            "Failed to decode post-join client message, ignoring"
        );
        return;
    };

    match client_message.message {
        Some(client_message::Message::MediaConnectionFailed(msg)) => {
            // Truncate client-controlled fields before logging to prevent log injection.
            // Use floor_char_boundary to avoid panicking on multi-byte UTF-8 boundaries.
            let truncated_url =
                &msg.media_handler_url[..msg.media_handler_url.floor_char_boundary(256)];
            let truncated_reason = &msg.error_reason[..msg.error_reason.floor_char_boundary(256)];
            warn!(
                target: "mc.webtransport.connection",
                connection_id = %connection_id,
                media_handler_url = %truncated_url,
                error_reason = %truncated_reason,
                all_handlers_failed = msg.all_handlers_failed,
                "Client reported media connection failure (R-20: no reallocation)"
            );
            metrics::record_media_connection_failed(msg.all_handlers_failed);
        }
        Some(_) => {
            debug!(
                target: "mc.webtransport.connection",
                connection_id = %connection_id,
                "Received unhandled post-join client message, ignoring"
            );
        }
        None => {
            debug!(
                target: "mc.webtransport.connection",
                connection_id = %connection_id,
                "Received empty client message, ignoring"
            );
        }
    }
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
///
/// Finishes the stream after writing to ensure data is flushed
/// before the function returns and the stream is dropped.
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
    let result = write_framed_message(stream, &server_msg).await;
    // Finish the stream to flush buffered data before the caller drops it
    let _ = stream.finish().await;
    result
}

/// Build a protobuf `JoinResponse` from the actor's `JoinResult`.
///
/// Reads MH assignment data from Redis to populate `media_servers`.
/// Returns the `MhAssignmentData` alongside the response so it can be
/// passed to the async `RegisterMeeting` task without re-reading Redis.
/// Fails the join if MH assignment data is unavailable — a meeting
/// without media handlers is not useful (R-6).
async fn build_join_response(
    result: &JoinResult,
    redis_client: &dyn MhAssignmentStore,
    meeting_id: &str,
) -> Result<(JoinResponse, MhAssignmentData), McError> {
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

    // Read MH assignment data from Redis (R-6)
    let mh_data = redis_client
        .get_mh_assignment(meeting_id)
        .await?
        .ok_or_else(|| {
            warn!(
                target: "mc.webtransport.connection",
                meeting_id = %meeting_id,
                "No MH assignment data in Redis — cannot join without media handlers"
            );
            McError::MhAssignmentMissing(meeting_id.to_string())
        })?;

    // Populate media_servers with WebTransport endpoints from MH assignment data
    let media_servers: Vec<MediaServerInfo> = mh_data
        .handlers
        .iter()
        .map(|h| MediaServerInfo {
            media_handler_url: h.webtransport_endpoint.clone(),
        })
        .collect();

    Ok((
        JoinResponse {
            participant_id: result.participant_id.clone(),
            user_id: 0,
            existing_participants,
            media_servers,
            encryption_keys: None,
            correlation_id: result.correlation_id.clone(),
            binding_token: result.binding_token.clone(),
        },
        mh_data,
    ))
}

/// Fire `RegisterMeeting` RPCs to each assigned MH (R-12).
///
/// Called as a spawned task after the first participant joins. Iterates over
/// all MH handlers in the assignment data, calling `register_meeting()` on
/// each that has a gRPC endpoint. Retries with exponential backoff on failure.
///
/// This function handles all errors internally (log + continue) since it runs
/// as a fire-and-forget spawned task with no caller to propagate errors to.
async fn register_meeting_with_handlers(
    mh_client: &dyn MhRegistrationClient,
    mh_data: &MhAssignmentData,
    meeting_id: &str,
    mc_id: &str,
    mc_grpc_endpoint: &str,
    cancel_token: &CancellationToken,
) {
    for handler in &mh_data.handlers {
        let grpc_endpoint = match &handler.grpc_endpoint {
            Some(ep) => ep,
            None => {
                debug!(
                    target: "mc.register_meeting.trigger",
                    mh_id = %handler.mh_id,
                    "MH has no gRPC endpoint, skipping RegisterMeeting"
                );
                continue;
            }
        };

        let mut last_error = None;
        for attempt in 1..=MAX_REGISTER_ATTEMPTS {
            if cancel_token.is_cancelled() {
                info!(
                    target: "mc.register_meeting.trigger",
                    "RegisterMeeting cancelled during shutdown"
                );
                return;
            }
            match mh_client
                .register_meeting(grpc_endpoint, meeting_id, mc_id, mc_grpc_endpoint)
                .await
            {
                Ok(()) => {
                    debug!(
                        target: "mc.register_meeting.trigger",
                        mh_grpc_endpoint = %grpc_endpoint,
                        "RegisterMeeting succeeded"
                    );
                    last_error = None;
                    break;
                }
                Err(e) => {
                    warn!(
                        target: "mc.register_meeting.trigger",
                        attempt = attempt,
                        max_attempts = MAX_REGISTER_ATTEMPTS,
                        mh_grpc_endpoint = %grpc_endpoint,
                        error = %e,
                        "RegisterMeeting attempt failed"
                    );
                    last_error = Some(e);

                    // Backoff before next attempt (unless this was the last attempt)
                    if let Some(&delay) = REGISTER_BACKOFF_DELAYS.get(attempt as usize - 1) {
                        tokio::select! {
                            () = cancel_token.cancelled() => {
                                info!(
                                    target: "mc.register_meeting.trigger",
                                    "RegisterMeeting cancelled during shutdown"
                                );
                                return;
                            }
                            () = tokio::time::sleep(delay) => {}
                        }
                    }
                }
            }
        }

        if let Some(e) = last_error {
            error!(
                target: "mc.register_meeting.trigger",
                mh_grpc_endpoint = %grpc_endpoint,
                total_attempts = MAX_REGISTER_ATTEMPTS,
                error = %e,
                "RegisterMeeting retries exhausted"
            );
        }
    }
}

// Note: build_join_response is async and requires a Redis client.
// Integration tests in tests/join_tests.rs cover the full join flow
// including Redis MH assignment data population and media_servers verification.

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_handle_media_connection_failed() {
        let msg = ClientMessage {
            message: Some(client_message::Message::MediaConnectionFailed(
                signaling::MediaConnectionFailed {
                    media_handler_url: "https://mh-1.example.com".to_string(),
                    error_reason: "timeout".to_string(),
                    all_handlers_failed: false,
                },
            )),
        };
        let data = msg.encode_to_vec();
        // Should not panic
        handle_client_message(&data, "test-conn-1");
    }

    #[test]
    fn test_handle_media_connection_failed_all_handlers() {
        let msg = ClientMessage {
            message: Some(client_message::Message::MediaConnectionFailed(
                signaling::MediaConnectionFailed {
                    media_handler_url: "https://mh-2.example.com".to_string(),
                    error_reason: "connection_refused".to_string(),
                    all_handlers_failed: true,
                },
            )),
        };
        let data = msg.encode_to_vec();
        handle_client_message(&data, "test-conn-2");
    }

    #[test]
    fn test_handle_client_message_unhandled_type() {
        let msg = ClientMessage {
            message: Some(client_message::Message::MuteRequest(
                signaling::MuteRequest {
                    audio_muted: true,
                    video_muted: false,
                },
            )),
        };
        let data = msg.encode_to_vec();
        // Should not panic -- exercises the Some(_) branch
        handle_client_message(&data, "test-conn-3");
    }

    #[test]
    fn test_handle_client_message_invalid_data() {
        let garbage = vec![0xFF, 0xFE, 0xFD, 0xFC, 0xFB];
        // Should not panic -- exercises the decode error branch
        handle_client_message(&garbage, "test-conn-4");
    }

    #[test]
    fn test_handle_client_message_empty_message() {
        let msg = ClientMessage { message: None };
        let data = msg.encode_to_vec();
        // Should not panic -- exercises the None branch
        handle_client_message(&data, "test-conn-5");
    }

    // ========================================================================
    // register_meeting_with_handlers unit tests
    // ========================================================================

    use crate::redis::MhEndpointInfo;
    use std::collections::VecDeque;
    use std::pin::Pin;
    use std::sync::Mutex;

    /// Mock MhRegistrationClient that returns results from a queue.
    /// When the queue is empty, returns Ok(()).
    struct MockRegClient {
        results: Mutex<VecDeque<Result<(), McError>>>,
        call_count: Mutex<u32>,
    }

    impl MockRegClient {
        fn new(results: Vec<Result<(), McError>>) -> Self {
            Self {
                results: Mutex::new(VecDeque::from(results)),
                call_count: Mutex::new(0),
            }
        }

        fn call_count(&self) -> u32 {
            *self.call_count.lock().unwrap()
        }
    }

    impl MhRegistrationClient for MockRegClient {
        fn register_meeting<'a>(
            &'a self,
            _mh_grpc_endpoint: &'a str,
            _meeting_id: &'a str,
            _mc_id: &'a str,
            _mc_grpc_endpoint: &'a str,
        ) -> Pin<Box<dyn std::future::Future<Output = Result<(), McError>> + Send + 'a>> {
            *self.call_count.lock().unwrap() += 1;
            let result = self.results.lock().unwrap().pop_front().unwrap_or(Ok(()));
            Box::pin(async move { result })
        }
    }

    fn make_mh_data(handlers: Vec<MhEndpointInfo>) -> MhAssignmentData {
        MhAssignmentData {
            handlers,
            assigned_at: "2024-01-01T00:00:00Z".to_string(),
        }
    }

    #[tokio::test(start_paused = true)]
    async fn test_register_retry_succeeds_on_second_attempt() {
        let client = MockRegClient::new(vec![Err(McError::Grpc("transient".to_string())), Ok(())]);
        let mh_data = make_mh_data(vec![MhEndpointInfo {
            mh_id: "mh-1".to_string(),
            webtransport_endpoint: "wt://mh-1:4433".to_string(),
            grpc_endpoint: Some("http://mh-1:50053".to_string()),
        }]);
        let cancel = CancellationToken::new();

        register_meeting_with_handlers(&client, &mh_data, "m1", "mc1", "http://mc:50052", &cancel)
            .await;

        assert_eq!(client.call_count(), 2, "Should succeed on 2nd attempt");
    }

    #[tokio::test(start_paused = true)]
    async fn test_register_all_retries_exhausted() {
        let client = MockRegClient::new(vec![
            Err(McError::Grpc("fail-1".to_string())),
            Err(McError::Grpc("fail-2".to_string())),
            Err(McError::Grpc("fail-3".to_string())),
        ]);
        let mh_data = make_mh_data(vec![MhEndpointInfo {
            mh_id: "mh-1".to_string(),
            webtransport_endpoint: "wt://mh-1:4433".to_string(),
            grpc_endpoint: Some("http://mh-1:50053".to_string()),
        }]);
        let cancel = CancellationToken::new();

        register_meeting_with_handlers(&client, &mh_data, "m1", "mc1", "http://mc:50052", &cancel)
            .await;

        assert_eq!(
            client.call_count(),
            MAX_REGISTER_ATTEMPTS,
            "Should attempt exactly MAX_REGISTER_ATTEMPTS times"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn test_register_multiple_handlers_partial_failure() {
        // Handler 1 succeeds immediately, handler 2 fails all retries
        let client = MockRegClient::new(vec![
            Ok(()),                                   // handler 1, attempt 1
            Err(McError::Grpc("fail-1".to_string())), // handler 2, attempt 1
            Err(McError::Grpc("fail-2".to_string())), // handler 2, attempt 2
            Err(McError::Grpc("fail-3".to_string())), // handler 2, attempt 3
        ]);
        let mh_data = make_mh_data(vec![
            MhEndpointInfo {
                mh_id: "mh-1".to_string(),
                webtransport_endpoint: "wt://mh-1:4433".to_string(),
                grpc_endpoint: Some("http://mh-1:50053".to_string()),
            },
            MhEndpointInfo {
                mh_id: "mh-2".to_string(),
                webtransport_endpoint: "wt://mh-2:4433".to_string(),
                grpc_endpoint: Some("http://mh-2:50053".to_string()),
            },
        ]);
        let cancel = CancellationToken::new();

        register_meeting_with_handlers(&client, &mh_data, "m1", "mc1", "http://mc:50052", &cancel)
            .await;

        assert_eq!(
            client.call_count(),
            4,
            "1 call for handler 1 (success) + 3 calls for handler 2 (exhausted)"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn test_register_skips_handler_without_grpc_endpoint() {
        let client = MockRegClient::new(vec![Ok(())]);
        let mh_data = make_mh_data(vec![
            MhEndpointInfo {
                mh_id: "mh-no-grpc".to_string(),
                webtransport_endpoint: "wt://mh-1:4433".to_string(),
                grpc_endpoint: None,
            },
            MhEndpointInfo {
                mh_id: "mh-with-grpc".to_string(),
                webtransport_endpoint: "wt://mh-2:4433".to_string(),
                grpc_endpoint: Some("http://mh-2:50053".to_string()),
            },
        ]);
        let cancel = CancellationToken::new();

        register_meeting_with_handlers(&client, &mh_data, "m1", "mc1", "http://mc:50052", &cancel)
            .await;

        assert_eq!(
            client.call_count(),
            1,
            "Should only call register_meeting for handler with gRPC endpoint"
        );
    }
}
