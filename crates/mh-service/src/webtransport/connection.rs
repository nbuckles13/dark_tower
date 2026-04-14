//! Per-connection handler for MH WebTransport connections.
//!
//! Handles the connection lifecycle:
//! 1. Accept WebTransport session
//! 2. Accept bidirectional stream
//! 3. Read meeting JWT from first length-prefixed message
//! 4. Validate JWT via `MhJwtValidator`
//! 5. Check meeting registration status:
//!    - Registered: add connection, hold open
//!    - Not registered: provisional accept with configurable timeout
//! 6. Monitor for disconnect or cancellation

use crate::auth::MhJwtValidator;
use crate::errors::MhError;
use crate::observability::metrics;
use crate::session::{ConnectionEntry, PendingConnection, SessionManagerHandle};

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};
use wtransport::endpoint::IncomingSession;
use wtransport::stream::RecvStream;

/// Maximum size for a single framed message (64KB).
const MAX_MESSAGE_SIZE: usize = 64 * 1024;

/// Handle an incoming WebTransport connection.
///
/// This is the entry point for each client connection: accept session,
/// read JWT, validate, check registration, then hold the connection open.
///
/// # Errors
///
/// Returns `MhError` if session acceptance, JWT validation, or
/// meeting registration check fails.
#[tracing::instrument(skip_all, name = "mh.webtransport.connection", fields(connection_id = tracing::field::Empty))]
#[expect(
    clippy::too_many_lines,
    reason = "Connection lifecycle is sequential; splitting would fragment the accept-validate-register-hold flow"
)]
pub async fn handle_connection(
    incoming: IncomingSession,
    jwt_validator: Arc<MhJwtValidator>,
    session_manager: SessionManagerHandle,
    register_meeting_timeout: Duration,
    cancel_token: CancellationToken,
) -> Result<(), MhError> {
    let handshake_start = Instant::now();

    // Step 1: Accept the WebTransport session
    let session_request = incoming.await.map_err(|e| {
        warn!(
            target: "mh.webtransport.connection",
            error = %e,
            "Failed to receive session request"
        );
        MhError::WebTransportError(format!("Session request failed: {e}"))
    })?;

    let connection = session_request.accept().await.map_err(|e| {
        warn!(
            target: "mh.webtransport.connection",
            error = %e,
            "Failed to accept WebTransport session"
        );
        MhError::WebTransportError(format!("Session accept failed: {e}"))
    })?;

    let connection_id = uuid::Uuid::new_v4().to_string();
    tracing::Span::current().record("connection_id", connection_id.as_str());

    debug!(
        target: "mh.webtransport.connection",
        connection_id = %connection_id,
        "WebTransport session accepted"
    );

    // Step 2: Accept bidirectional stream
    let (_, mut recv_stream) = connection.accept_bi().await.map_err(|e| {
        warn!(
            target: "mh.webtransport.connection",
            connection_id = %connection_id,
            error = %e,
            "Failed to accept bidirectional stream"
        );
        MhError::WebTransportError(format!("BiStream accept failed: {e}"))
    })?;

    // Step 3: Read length-prefixed JWT from first message
    let jwt_bytes = read_framed_message(&mut recv_stream).await?;
    let token = String::from_utf8(jwt_bytes.to_vec()).map_err(|_| {
        warn!(
            target: "mh.webtransport.connection",
            connection_id = %connection_id,
            "JWT is not valid UTF-8"
        );
        MhError::JwtValidation("The access token is invalid or expired".to_string())
    })?;

    // Step 4: Validate meeting JWT
    let claims = match jwt_validator.validate_meeting_token(&token).await {
        Ok(claims) => {
            metrics::record_jwt_validation("success", "meeting");
            claims
        }
        Err(e) => {
            warn!(
                target: "mh.webtransport.connection",
                connection_id = %connection_id,
                error = %e,
                "JWT validation failed"
            );
            metrics::record_jwt_validation("failure", "meeting");
            return Err(e);
        }
    };

    let meeting_id = &claims.meeting_id;
    let participant_id = &claims.sub;

    info!(
        target: "mh.webtransport.connection",
        connection_id = %connection_id,
        meeting_id = %meeting_id,
        "JWT validation succeeded"
    );

    // Record handshake duration (session accept through JWT validation)
    metrics::record_webtransport_handshake_duration(handshake_start.elapsed());

    // Step 5: Check meeting registration status
    if session_manager.is_meeting_registered(meeting_id).await {
        // Meeting already registered — add as active connection
        session_manager
            .add_connection(
                meeting_id,
                ConnectionEntry {
                    connection_id: connection_id.clone(),
                    participant_id: participant_id.clone(),
                    connected_at: Instant::now(),
                },
            )
            .await;

        info!(
            target: "mh.webtransport.connection",
            connection_id = %connection_id,
            meeting_id = %meeting_id,
            participant_id = %participant_id,
            "Connection established for registered meeting"
        );
    } else {
        // Meeting not yet registered — provisional accept with timeout
        debug!(
            target: "mh.webtransport.connection",
            connection_id = %connection_id,
            meeting_id = %meeting_id,
            timeout_secs = register_meeting_timeout.as_secs(),
            "Meeting not registered, entering provisional accept"
        );

        let pending = PendingConnection {
            connection_id: connection_id.clone(),
            meeting_id: meeting_id.clone(),
            participant_id: participant_id.clone(),
            connected_at: Instant::now(),
        };

        let notify = session_manager.add_pending_connection(pending).await;

        // Wait for either: RegisterMeeting notification, timeout, or cancellation
        tokio::select! {
            () = notify.notified() => {
                // RegisterMeeting arrived — connection was promoted by SessionManager
                info!(
                    target: "mh.webtransport.connection",
                    connection_id = %connection_id,
                    meeting_id = %meeting_id,
                    "Pending connection promoted after RegisterMeeting"
                );
            }
            () = tokio::time::sleep(register_meeting_timeout) => {
                // Timeout expired — disconnect client
                warn!(
                    target: "mh.webtransport.connection",
                    connection_id = %connection_id,
                    meeting_id = %meeting_id,
                    "RegisterMeeting timeout expired, disconnecting client"
                );
                session_manager
                    .remove_pending_connection(meeting_id, &connection_id)
                    .await;
                return Err(MhError::MeetingNotRegistered(
                    meeting_id.clone(),
                ));
            }
            () = cancel_token.cancelled() => {
                debug!(
                    target: "mh.webtransport.connection",
                    connection_id = %connection_id,
                    "Provisional connection cancelled during shutdown"
                );
                session_manager
                    .remove_pending_connection(meeting_id, &connection_id)
                    .await;
                return Ok(());
            }
        }
    }

    // Step 6: Hold connection open — monitor for disconnect or cancellation
    // The connection stays open for future media frame forwarding (separate story).
    // For now, we monitor the recv stream for closure and the cancellation token.
    let mut probe_buf = [0u8; 1];
    loop {
        tokio::select! {
            () = cancel_token.cancelled() => {
                debug!(
                    target: "mh.webtransport.connection",
                    connection_id = %connection_id,
                    "Connection cancelled during shutdown"
                );
                break;
            }
            result = recv_stream.read(&mut probe_buf) => {
                match result {
                    Ok(None) | Err(_) => {
                        info!(
                            target: "mh.webtransport.connection",
                            connection_id = %connection_id,
                            meeting_id = %meeting_id,
                            "Client disconnected"
                        );
                        break;
                    }
                    Ok(Some(_)) => {
                        // Client sent data — ignore for now (media forwarding is future scope)
                    }
                }
            }
        }
    }

    // Step 7: Cleanup — remove connection from session manager
    session_manager
        .remove_connection(meeting_id, &connection_id)
        .await;

    info!(
        target: "mh.webtransport.connection",
        connection_id = %connection_id,
        meeting_id = %meeting_id,
        participant_id = %participant_id,
        "Connection closed and cleaned up"
    );

    Ok(())
}

/// Read a length-prefixed message from a `RecvStream`.
///
/// Wire format: 4-byte big-endian length prefix + payload bytes.
/// Enforces `MAX_MESSAGE_SIZE` (64KB) to prevent abuse.
async fn read_framed_message(stream: &mut RecvStream) -> Result<bytes::Bytes, MhError> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await.map_err(|e| {
        warn!(
            target: "mh.webtransport.connection",
            error = %e,
            "Failed to read message length prefix"
        );
        MhError::WebTransportError("Failed to read message".to_string())
    })?;

    let msg_len = u32::from_be_bytes(len_buf) as usize;

    if msg_len > MAX_MESSAGE_SIZE {
        warn!(
            target: "mh.webtransport.connection",
            msg_len = msg_len,
            max = MAX_MESSAGE_SIZE,
            "Message exceeds maximum size"
        );
        return Err(MhError::WebTransportError("Message too large".to_string()));
    }

    if msg_len == 0 {
        return Err(MhError::WebTransportError("Empty message".to_string()));
    }

    let mut buf = vec![0u8; msg_len];
    stream.read_exact(&mut buf).await.map_err(|e| {
        warn!(
            target: "mh.webtransport.connection",
            error = %e,
            msg_len = msg_len,
            "Failed to read message body"
        );
        MhError::WebTransportError("Failed to read message body".to_string())
    })?;

    Ok(bytes::Bytes::from(buf))
}
