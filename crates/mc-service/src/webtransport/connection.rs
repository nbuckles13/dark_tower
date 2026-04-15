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
use crate::observability::metrics;
use crate::redis::MhAssignmentStore;

use bytes::{BufMut, BytesMut};
use common::jwt::MeetingRole;
use prost::Message;
use proto_gen::signaling::{
    self, client_message, server_message, ClientMessage, ErrorMessage, JoinResponse,
    MediaServerInfo, Participant, ServerMessage,
};
use std::sync::Arc;
use std::time::Instant;
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
    redis_client: Arc<dyn MhAssignmentStore>,
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
            metrics::record_jwt_validation("failure", "meeting");
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

    metrics::record_jwt_validation("success", "meeting");

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
    let join_response =
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
/// Fails the join if MH assignment data is unavailable — a meeting
/// without media handlers is not useful (R-6).
async fn build_join_response(
    result: &JoinResult,
    redis_client: &dyn MhAssignmentStore,
    meeting_id: &str,
) -> Result<JoinResponse, McError> {
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

    Ok(JoinResponse {
        participant_id: result.participant_id.clone(),
        user_id: 0,
        existing_participants,
        media_servers,
        encryption_keys: None,
        correlation_id: result.correlation_id.clone(),
        binding_token: result.binding_token.clone(),
    })
}

// Note: build_join_response is async and requires a Redis client.
// Integration tests in tests/join_tests.rs cover the full join flow
// including Redis MH assignment data population and media_servers verification.
