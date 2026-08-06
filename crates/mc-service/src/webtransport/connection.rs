//! `ConnectionActor` - owns WebTransport streams and bridge loop.
//!
//! This actor lives in the webtransport layer (not the actor hierarchy) and:
//! 1. Receives a `JoinResult` from the meeting actor via oneshot
//! 2. Sends `JoinResponse` to the client over the WebTransport stream
//! 3. Runs the bridge loop forwarding `ParticipantUpdate` messages to the client
//! 4. Notifies the meeting when the connection drops (via `MeetingActorHandle`)

use crate::actors::messages::{DisconnectCause, JoinResult};
use crate::actors::{
    BoundedMhStatus, MeetingControllerActorHandle, MhState, ParticipantActorHandle,
};
use crate::auth::McJwtValidator;
use crate::errors::McError;
use crate::grpc::MhRegistrationClient;
use crate::observability::metrics;
use crate::redis::{MhAssignmentData, MhAssignmentStore};

use super::trace::{inject_current_context, reparent_current_span};

use bytes::{BufMut, BytesMut};
use common::jwt::MeetingRole;
use prost::Message;
use proto_gen::dark_tower::signaling::v1::{
    self, client_message, server_message, ClientMessage, ErrorMessage, JoinResponse,
    MediaServerInfo, Participant, ServerMessage,
};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, instrument, warn, Instrument};
use wtransport::endpoint::IncomingSession;
use wtransport::error::ConnectionError;
use wtransport::stream::{RecvStream, SendStream};
use wtransport::Connection;

/// Maximum size for a single framed message (64KB).
const MAX_MESSAGE_SIZE: usize = 64 * 1024;

/// Maximum bytes for a client-controlled string before it is stored or logged
/// (R-60 security). Applied to `mh_url` / `failure_reason` / `failure_code`.
const MAX_CLIENT_STRING_BYTES: usize = 256;

/// Maximum number of per-MH statuses processed from a SINGLE
/// `MediaConnectionUpdate` message (R-60 security — input-amplification bound).
/// The persistent per-participant map is capped separately at
/// `MAX_MH_STATUSES_PER_PARTICIPANT` (16); this bounds the transient work the
/// handler does per message. A 64 KiB frame can pack tens of thousands of
/// minimal statuses, but at most 16 can ever land — so we refuse to map+iterate
/// an unbounded attacker-sized batch. Set to 4× the map cap so a legitimate
/// client (which sends ≤ its assigned MH count, ≤ the map cap) is never
/// affected. Over-limit batches bump
/// `mc_participant_mh_status_dropped_total{reason="over_limit"}`.
const MAX_MH_STATUSES_PER_UPDATE: usize = 64;

/// Maximum length for a participant display name (bytes).
const MAX_PARTICIPANT_NAME_LEN: usize = 256;

/// Channel buffer for outbound messages from ParticipantActor to WebTransport stream.
const OUTBOUND_CHANNEL_BUFFER: usize = 100;

/// Micro-bound for resolving the authoritative session close reason after the
/// client's recv stream ends or an outbound write fails.
///
/// This is NOT a policy/config knob: on any real session close `Connection::closed()`
/// is already resolved and this returns in ~0ms. It only bounds the pathological
/// case where a single stream ends while the QUIC session lingers (does not occur
/// on a browser tab-close), so the connection handler can't hang. If it elapses,
/// the cause fails safe to `ConnectionLost` (grace-preserving).
const CLOSE_CAUSE_RESOLVE_BOUND: Duration = Duration::from_secs(2);

/// Maximum number of attempts for RegisterMeeting RPC per MH.
const MAX_REGISTER_ATTEMPTS: u32 = 3;

/// Backoff delays between RegisterMeeting retry attempts.
const REGISTER_BACKOFF_DELAYS: [Duration; 2] = [Duration::from_secs(1), Duration::from_secs(2)];

/// Truncate `s` to at most `max_bytes`, snapping DOWN to a UTF-8 char boundary
/// so a multi-byte codepoint is never split (R-60 security — bounds
/// client-controlled strings before store/log).
///
/// `str::floor_char_boundary` is still unstable, so this rolls the equivalent.
/// Bounds BYTES (log/memory blast radius is bytes), never panics.
fn truncate_utf8(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

/// Classify a WebTransport `ConnectionError` (from `Connection::closed()`) into
/// the transport-authenticated [`DisconnectCause`].
///
/// Only a clean peer/application close maps to `ClientClosed` (immediate roster
/// removal — grace skipped). EVERYTHING ELSE, including any future `wtransport`
/// variant via the catch-all, maps to the grace-preserving `ConnectionLost`, so
/// a new/unknown variant can never fall into the immediate-remove path
/// (fail-safe). Classification uses ONLY the error VARIANT — the client-chosen
/// application error-code / close-reason string inside `ApplicationClosed` is
/// never inspected, logged, or used as a metric label.
fn classify_connection_error(error: &ConnectionError) -> DisconnectCause {
    match error {
        // Deliberate close by the peer — application-level (WebTransport session
        // close, e.g. a browser tab close) or QUIC transport-level CONNECTION_CLOSE.
        ConnectionError::ApplicationClosed(_) | ConnectionError::ConnectionClosed(_) => {
            DisconnectCause::ClientClosed
        }
        // We closed it (shutdown/drain/explicit-leave already handled).
        ConnectionError::LocallyClosed => DisconnectCause::ServerInitiated,
        // Idle timeout, QUIC/H3 protocol errors, CID exhaustion, and any future
        // variant → abrupt/ambiguous loss: keep the ADR-0023 grace period.
        _ => DisconnectCause::ConnectionLost,
    }
}

/// Map a `ConnectionError` to a FIXED, bounded variant discriminant for logging.
///
/// This returns a `&'static str` variant name ONLY — it never renders the error's
/// `Display`, which for `ApplicationClosed` embeds the client-controlled
/// WebTransport close-reason phrase (`String::from_utf8_lossy`, unescaped). Logging
/// that would be a log-injection / log-forging vector and would violate the
/// `classify_connection_error` contract ("the client-chosen close-reason string ...
/// is never inspected, logged, or used as a metric label"). Forensic-only hint; the
/// authoritative signal is the bounded [`DisconnectCause`].
fn connection_error_variant(error: &ConnectionError) -> &'static str {
    match error {
        ConnectionError::ApplicationClosed(_) => "application_closed",
        ConnectionError::ConnectionClosed(_) => "connection_closed",
        ConnectionError::LocallyClosed => "locally_closed",
        ConnectionError::TimedOut => "timed_out",
        ConnectionError::CidsExhausted => "cids_exhausted",
        // Any other/future variant (QUIC/H3 protocol errors, etc.): a fixed label,
        // never the client-influenced Display.
        _ => "other",
    }
}

/// Resolve the authoritative disconnect cause after a stream ended / write failed.
///
/// `Connection::closed()` yields the session close reason; on any real close it is
/// already resolved (~0ms). Bounded by [`CLOSE_CAUSE_RESOLVE_BOUND`] so a lingering
/// half-closed session cannot hang the handler — on timeout, fail safe to
/// `ConnectionLost` (grace-preserving).
async fn resolve_close_cause(connection: &Connection) -> DisconnectCause {
    match tokio::time::timeout(CLOSE_CAUSE_RESOLVE_BOUND, connection.closed()).await {
        Ok(error) => classify_connection_error(&error),
        Err(_) => DisconnectCause::ConnectionLost,
    }
}

/// Map the proto `ConnectionState` (stored as `i32`) to the bounded domain enum
/// via a TOTAL match with an `Unspecified` catch-all (R-60 allowlist-clamp — an
/// unknown/malformed wire int can never widen the `mc_participant_mh_status_total`
/// label domain).
fn map_connection_state(state: i32) -> MhState {
    use v1::ConnectionState;
    match ConnectionState::try_from(state) {
        Ok(ConnectionState::Connected) => MhState::Connected,
        Ok(ConnectionState::Failed) => MhState::Failed,
        Ok(ConnectionState::Disconnected) => MhState::Disconnected,
        Ok(ConnectionState::Unspecified) | Err(_) => MhState::Unspecified,
    }
}

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

    // R-57: reparent the `mc.webtransport.connection` span onto the browser's
    // trace so the whole join flow (and the `mc.actor.participant` spawn) become
    // children of the client's `dt_client.join` span. Extraction uses the global
    // bounded propagator; empty proto3-default fields → clean root (no-op).
    reparent_current_span(&client_message.trace_parent, &client_message.trace_state);

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
                v1::ErrorCode::InvalidRequest as i32,
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
            v1::ErrorCode::InvalidRequest as i32,
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
                v1::ErrorCode::Unauthorized as i32,
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
            v1::ErrorCode::Unauthorized as i32,
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

    // Length-bound the token's display_name at the trust boundary before it
    // enters the roster / JoinResponse / ParticipantJoined broadcast. The claim
    // is only whole-token-size limited (and guest names are self-reported), so
    // truncate (never reject) with the same UTF-8-safe helper + cap used for the
    // client-supplied participant_name. An empty claim stays empty so the
    // handle_join sink applies the generic "Participant N" fallback.
    let display_name = truncate_utf8(&claims.display_name, MAX_PARTICIPANT_NAME_LEN);

    let join_rx = match controller_handle
        .join_connection(
            meeting_id.clone(),
            connection_id.clone(),
            claims.sub.clone(),
            participant_id.clone(),
            display_name,
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
                v1::ErrorCode::InternalError as i32,
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
    // R-57: symmetric outbound injection — carry the current server-side trace
    // context back to the client on the JoinResponse (bounded W3C IDs only).
    let (trace_parent, trace_state) = inject_current_context();
    let server_msg = ServerMessage {
        message: Some(server_message::Message::JoinResponse(join_response)),
        trace_parent,
        trace_state,
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
    // Returns the transport-authenticated disconnect cause.
    let disconnect_cause = run_bridge_loop(
        &mut send_stream,
        &mut recv_stream,
        &mut outbound_rx,
        &cancel_token,
        &connection,
        &connection_id,
        &join_result.participant_handle,
    )
    .await;

    // Record the cause on the ParticipantActor handle BEFORE cancelling, so the
    // actor's exit notification carries it to the meeting (Release store happens
    // -before the cancel the actor observes). A clean close → immediate roster
    // removal; abrupt/ambiguous → grace period preserved.
    join_result
        .participant_handle
        .set_disconnect_cause(disconnect_cause);

    // Cancel the ParticipantActor — it will notify the meeting of disconnect on exit
    info!(
        target: "mc.webtransport.connection",
        connection_id = %connection_id,
        meeting_id = %meeting_id,
        participant_id = %join_result.participant_id,
        cause = %disconnect_cause.label(),
        "Connection closing, cancelling ParticipantActor"
    );
    join_result.participant_handle.cancel();

    Ok(())
}

/// Run the bridge loop: forward outbound messages to the WebTransport stream.
///
/// Returns the transport-authenticated [`DisconnectCause`] describing why the
/// loop exited, so the caller can drive an immediate roster removal (clean close)
/// vs. the ADR-0023 grace period (abrupt/ambiguous loss).
///
/// Exits when:
/// - Cancellation token is triggered → [`DisconnectCause::ServerInitiated`]
/// - The session closes (`Connection::closed()` fires) → classified from the
///   `ConnectionError` (clean peer close → `ClientClosed`; idle-timeout/abrupt →
///   `ConnectionLost`; local close → `ServerInitiated`)
/// - The client's recv stream ends or an outbound write fails → the authoritative
///   cause is resolved via [`resolve_close_cause`]
/// - The outbound channel closes (ParticipantActor stopped) →
///   [`DisconnectCause::ServerInitiated`]
///
/// Each arm cleanly `return`s or `break`s — no busy-loop. A FRESH
/// `connection.closed()` future is created each iteration (cancellation-safe to
/// re-poll).
async fn run_bridge_loop(
    send_stream: &mut SendStream,
    recv_stream: &mut RecvStream,
    outbound_rx: &mut mpsc::Receiver<bytes::Bytes>,
    cancel_token: &CancellationToken,
    connection: &Connection,
    connection_id: &str,
    participant_handle: &ParticipantActorHandle,
) -> DisconnectCause {
    loop {
        tokio::select! {
            () = cancel_token.cancelled() => {
                debug!(
                    target: "mc.webtransport.connection",
                    connection_id = %connection_id,
                    "Bridge loop cancelled"
                );
                return DisconnectCause::ServerInitiated;
            }

            // Session-level close (the authoritative departure signal). A browser
            // tab close surfaces here as ApplicationClosed/ConnectionClosed; a
            // crash/network-loss as TimedOut once the configured idle timeout fires.
            close_error = connection.closed() => {
                let cause = classify_connection_error(&close_error);
                // Log the bounded classification + a FIXED variant discriminant only.
                // Never `%close_error`: its Display embeds the client-controlled
                // close-reason string (log-injection vector; contract-violating).
                debug!(
                    target: "mc.webtransport.connection",
                    connection_id = %connection_id,
                    error_variant = %connection_error_variant(&close_error),
                    cause = %cause.label(),
                    "WebTransport session closed"
                );
                return cause;
            }

            msg = outbound_rx.recv() => {
                match msg {
                    Some(data) => {
                        if let Err(e) = write_raw_framed(send_stream, &data).await {
                            // Post-join write failure = the peer is going away.
                            // Resolve the authoritative session close reason rather
                            // than treating it as a server error.
                            warn!(
                                target: "mc.webtransport.connection",
                                connection_id = %connection_id,
                                error = %e,
                                "Failed to write outbound message; resolving close cause"
                            );
                            return resolve_close_cause(connection).await;
                        }
                    }
                    None => {
                        debug!(
                            target: "mc.webtransport.connection",
                            connection_id = %connection_id,
                            "Outbound channel closed, ending bridge loop"
                        );
                        return DisconnectCause::ServerInitiated;
                    }
                }
            }

            // Read client messages (framed protobuf)
            result = read_framed_message(recv_stream) => {
                match result {
                    Ok(data) => {
                        handle_client_message(&data, connection_id, participant_handle).await;
                    }
                    Err(e) => {
                        // The client's recv stream ended. Resolve the authoritative
                        // session close reason (bounded) so a clean tab-close is
                        // classified as ClientClosed. The underlying stream error is
                        // logged for forensics (variant only; no client strings).
                        debug!(
                            target: "mc.webtransport.connection",
                            connection_id = %connection_id,
                            error = %e,
                            "Client stream closed or read error; resolving close cause"
                        );
                        return resolve_close_cause(connection).await;
                    }
                }
            }
        }
    }
}

/// Handle a post-join client message in the bridge loop.
///
/// Handles:
/// - `MediaConnectionUpdate` (R-60): records per-MH state on the participant
///   actor under its own reparented child span (see
///   [`handle_media_connection_update`]).
/// - All other messages: Ignored (logged at debug level).
async fn handle_client_message(
    data: &[u8],
    connection_id: &str,
    participant_handle: &ParticipantActorHandle,
) {
    let Ok(client_message) = ClientMessage::decode(data) else {
        debug!(
            target: "mc.webtransport.connection",
            connection_id = %connection_id,
            "Failed to decode post-join client message, ignoring"
        );
        return;
    };

    // Read the trace fields off the envelope before the `match` moves `.message`.
    let trace_parent = client_message.trace_parent;
    let trace_state = client_message.trace_state;

    match client_message.message {
        Some(client_message::Message::MediaConnectionUpdate(update)) => {
            handle_media_connection_update(
                update,
                &trace_parent,
                &trace_state,
                connection_id,
                participant_handle,
            )
            .await;
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

/// Handle a post-join `MediaConnectionUpdate` (R-60), recording per-MH state on
/// the participant actor.
///
/// Runs in its OWN span reparented onto the message's trace context (R-57 —
/// a fresh CHILD span, NOT a re-parent of the long-lived connection span, whose
/// single parent is fixed at the JoinRequest). All client-controlled strings
/// (`mh_url` / `failure_reason` / `failure_code`) are truncated to
/// `MAX_CLIENT_STRING_BYTES` on a char boundary at this trust boundary before
/// being handed to the actor, so no untruncated field is ever stored or logged.
#[instrument(
    target = "mc.webtransport.connection",
    name = "mc.media_connection_update",
    skip_all,
    fields(connection_id = %connection_id, statuses_count = update.statuses.len())
)]
async fn handle_media_connection_update(
    update: v1::MediaConnectionUpdate,
    trace_parent: &str,
    trace_state: &str,
    connection_id: &str,
    participant_handle: &ParticipantActorHandle,
) {
    // Reparent THIS span onto the client's per-message trace context (empty →
    // clean root; extraction via the global bounded propagator).
    reparent_current_span(trace_parent, trace_state);

    // Bound the per-message fan-out BEFORE doing any per-entry work (R-60
    // security): a hostile client can pack a 64 KiB frame with tens of thousands
    // of minimal statuses, but at most the map cap can ever land. Refuse to
    // map+iterate an unbounded attacker-sized batch — `take` the first
    // `MAX_MH_STATUSES_PER_UPDATE` and record the truncation as an observable
    // drop (no client strings logged, mirroring the cap-drop discipline).
    let over_limit = update.statuses.len() > MAX_MH_STATUSES_PER_UPDATE;
    if over_limit {
        crate::observability::metrics::record_participant_mh_status_dropped("over_limit");
    }

    // Map + truncate at the trust boundary: the actor only ever receives bounded
    // domain values keyed by the truncated `mh_url`.
    let statuses: Vec<(String, BoundedMhStatus)> = update
        .statuses
        .into_iter()
        .take(MAX_MH_STATUSES_PER_UPDATE)
        .map(|s| {
            let key = truncate_utf8(&s.mh_url, MAX_CLIENT_STRING_BYTES);
            let bounded = BoundedMhStatus {
                state: map_connection_state(s.state),
                failure_reason: s
                    .failure_reason
                    .map(|r| truncate_utf8(&r, MAX_CLIENT_STRING_BYTES)),
                failure_code: s
                    .failure_code
                    .map(|c| truncate_utf8(&c, MAX_CLIENT_STRING_BYTES)),
            };
            (key, bounded)
        })
        .collect();

    if let Err(e) = participant_handle.record_mh_statuses(statuses).await {
        // Actor gone mid-connection (mailbox receiver dropped). Graceful — no
        // panic; the connection will close on its own. No client-controlled
        // strings in this log line.
        debug!(
            target: "mc.webtransport.connection",
            connection_id = %connection_id,
            error = %e,
            "Failed to deliver MediaConnectionUpdate to participant actor"
        );
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
    // R-57: symmetric outbound injection on the error path too.
    let (trace_parent, trace_state) = inject_current_context();
    let server_msg = ServerMessage {
        message: Some(server_message::Message::Error(ErrorMessage {
            code: error_code,
            message: message.to_string(),
            details: Default::default(),
        })),
        trace_parent,
        trace_state,
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
/// each. Retries with exponential backoff on failure.
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
        let grpc_endpoint = &handler.grpc_endpoint;

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

    // Full R-60 behavioral coverage (all-CONNECTED / partial / all-FAILED state
    // recording + metric + truncation + cap) lives in
    // `tests/media_connection_update_integration.rs`, which drives a real framed
    // `ClientMessage{MediaConnectionUpdate}` through the decode+dispatch seam.
    // These in-module tests only assert the non-`MediaConnectionUpdate` branches
    // don't panic and the char-boundary truncation helper is correct.

    /// Spawn a throwaway participant actor for the dispatch-branch tests.
    fn test_participant_handle() -> (ParticipantActorHandle, tokio::task::JoinHandle<()>) {
        use crate::actors::{ActorMetrics, ParticipantActor};
        ParticipantActor::spawn(
            "test-conn".to_string(),
            "test-part".to_string(),
            "test-meeting".to_string(),
            CancellationToken::new(),
            ActorMetrics::new(),
        )
    }

    #[tokio::test]
    async fn test_handle_client_message_unhandled_type() {
        let (handle, _task) = test_participant_handle();
        let msg = ClientMessage {
            message: Some(client_message::Message::MuteRequest(v1::MuteRequest {
                audio_muted: true,
                video_muted: false,
            })),
            trace_parent: String::new(),
            trace_state: String::new(),
        };
        let data = msg.encode_to_vec();
        // Should not panic -- exercises the Some(_) branch
        handle_client_message(&data, "test-conn-3", &handle).await;
    }

    #[tokio::test]
    async fn test_handle_client_message_invalid_data() {
        let (handle, _task) = test_participant_handle();
        let garbage = vec![0xFF, 0xFE, 0xFD, 0xFC, 0xFB];
        // Should not panic -- exercises the decode error branch
        handle_client_message(&garbage, "test-conn-4", &handle).await;
    }

    #[tokio::test]
    async fn test_handle_client_message_empty_message() {
        let (handle, _task) = test_participant_handle();
        let msg = ClientMessage {
            message: None,
            trace_parent: String::new(),
            trace_state: String::new(),
        };
        let data = msg.encode_to_vec();
        // Should not panic -- exercises the None branch
        handle_client_message(&data, "test-conn-5", &handle).await;
    }

    #[test]
    fn test_truncate_utf8_short_string_unchanged() {
        assert_eq!(truncate_utf8("hello", 256), "hello");
    }

    #[test]
    fn test_truncate_utf8_bounds_bytes_on_char_boundary() {
        // A 300-byte ASCII string truncates to exactly 256 bytes.
        let long = "a".repeat(300);
        let out = truncate_utf8(&long, MAX_CLIENT_STRING_BYTES);
        assert_eq!(out.len(), MAX_CLIENT_STRING_BYTES);
    }

    #[test]
    fn test_truncate_utf8_never_splits_multibyte_codepoint() {
        // '€' is 3 bytes. Build a string whose 256th byte lands mid-codepoint,
        // and assert we floor to a boundary (shorter than 256) without panic.
        let s = "€".repeat(100); // 300 bytes, boundaries at multiples of 3
        let out = truncate_utf8(&s, MAX_CLIENT_STRING_BYTES);
        assert!(out.len() <= MAX_CLIENT_STRING_BYTES);
        assert!(s.starts_with(&out));
        // 256 is not a multiple of 3 → floors to 255 (85 full '€').
        assert_eq!(out.len(), 255);
    }

    #[test]
    fn test_classify_connection_error_abrupt_and_local() {
        // Unit-constructible ConnectionError variants. The clean-close variants
        // (ApplicationClosed/ConnectionClosed → ClientClosed) wrap private quinn
        // types that cannot be built in a unit test; that mapping is exercised
        // end-to-end by the transport integration test
        // `join_tests::test_clean_close_broadcasts_prompt_participant_left_voluntary`
        // (real session close → ClientClosed → prompt Left{Voluntary}). Here we
        // pin the grace-preserving and server-initiated mappings, incl. the catch-all.
        assert_eq!(
            classify_connection_error(&ConnectionError::TimedOut),
            DisconnectCause::ConnectionLost,
            "idle-timeout must keep grace (ConnectionLost)"
        );
        assert_eq!(
            classify_connection_error(&ConnectionError::LocallyClosed),
            DisconnectCause::ServerInitiated,
        );
        // Catch-all: a non-clean variant must NEVER map to the immediate-remove
        // ClientClosed path.
        assert_eq!(
            classify_connection_error(&ConnectionError::CidsExhausted),
            DisconnectCause::ConnectionLost,
        );
        assert_ne!(
            classify_connection_error(&ConnectionError::CidsExhausted),
            DisconnectCause::ClientClosed,
        );
    }

    #[test]
    fn test_map_connection_state_total_with_catch_all() {
        assert_eq!(map_connection_state(1), MhState::Connected);
        assert_eq!(map_connection_state(2), MhState::Failed);
        assert_eq!(map_connection_state(3), MhState::Disconnected);
        assert_eq!(map_connection_state(0), MhState::Unspecified);
        // Unknown / malformed wire int → Unspecified (allowlist-clamp).
        assert_eq!(map_connection_state(999), MhState::Unspecified);
        assert_eq!(map_connection_state(-1), MhState::Unspecified);
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
            grpc_endpoint: "http://mh-1:50053".to_string(),
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
            grpc_endpoint: "http://mh-1:50053".to_string(),
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
                grpc_endpoint: "http://mh-1:50053".to_string(),
            },
            MhEndpointInfo {
                mh_id: "mh-2".to_string(),
                webtransport_endpoint: "wt://mh-2:4433".to_string(),
                grpc_endpoint: "http://mh-2:50053".to_string(),
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
}
