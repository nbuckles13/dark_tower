//! Per-connection handler for MH WebTransport connections.
//!
//! Handles the connection lifecycle:
//! 1. Accept WebTransport session
//! 2. Accept bidirectional stream
//! 3. Read meeting JWT from first length-prefixed message
//! 4. Validate JWT via `MhJwtValidator`
//! 5. Check meeting registration status:
//!    - Registered: add connection, notify MC, hold open
//!    - Not registered: provisional accept with configurable timeout
//! 6. Monitor for disconnect or cancellation
//! 7. On disconnect: notify MC, clean up session

use crate::auth::MhJwtValidator;
use crate::errors::MhError;
use crate::grpc::McClient;
use crate::observability::metrics;
use crate::session::{ConnectionEntry, PendingConnection, SessionManagerHandle};

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};
use wtransport::endpoint::IncomingSession;
use wtransport::stream::RecvStream;

/// Maximum size for a single framed message (64KB).
const MAX_MESSAGE_SIZE: usize = 64 * 1024;

/// Outcome of awaiting a `RegisterMeeting` notification for a provisional
/// connection. Drives the caller's dispatch in [`handle_connection`].
#[derive(Debug)]
enum RegistrationOutcome {
    /// `RegisterMeeting` arrived before the timeout; the pending connection
    /// was promoted by `SessionManager`. Caller should notify MC.
    Registered,
    /// The provisional-accept timeout expired before `RegisterMeeting`
    /// arrived. `mh_register_meeting_timeouts_total` has been recorded and
    /// the pending connection has been removed. Caller should return
    /// `MhError::MeetingNotRegistered`.
    Timeout,
    /// The cancellation token fired (server shutdown). The pending
    /// connection has been removed. Caller should return `Ok(())`.
    Cancelled,
}

/// Await one of three outcomes for a provisional WebTransport connection:
/// `RegisterMeeting` arrives, the timeout expires, or the server shuts down.
///
/// On `Timeout`, records `mh_register_meeting_timeouts_total` and removes
/// the pending connection from `SessionManager`. On `Cancelled`, removes
/// the pending connection. On `Registered`, leaves MC notification to the
/// caller so MC-client I/O stays out of this helper (keeps the helper
/// unit-testable with only `SessionManagerHandle` + `CancellationToken`).
///
/// The metric fires ONLY on the timeout arm — this is the invariant the
/// unit tests in this module enforce.
#[must_use]
async fn await_meeting_registration(
    session_manager: &SessionManagerHandle,
    meeting_id: &str,
    connection_id: &str,
    notify: &Notify,
    register_meeting_timeout: Duration,
    cancel_token: &CancellationToken,
) -> RegistrationOutcome {
    tokio::select! {
        () = notify.notified() => {
            info!(
                target: "mh.webtransport.connection",
                connection_id = %connection_id,
                meeting_id = %meeting_id,
                "Pending connection promoted after RegisterMeeting"
            );
            RegistrationOutcome::Registered
        }
        () = tokio::time::sleep(register_meeting_timeout) => {
            warn!(
                target: "mh.webtransport.connection",
                connection_id = %connection_id,
                meeting_id = %meeting_id,
                "RegisterMeeting timeout expired, disconnecting client"
            );
            metrics::record_register_meeting_timeout();
            session_manager
                .remove_pending_connection(meeting_id, connection_id)
                .await;
            RegistrationOutcome::Timeout
        }
        () = cancel_token.cancelled() => {
            debug!(
                target: "mh.webtransport.connection",
                connection_id = %connection_id,
                "Provisional connection cancelled during shutdown"
            );
            session_manager
                .remove_pending_connection(meeting_id, connection_id)
                .await;
            RegistrationOutcome::Cancelled
        }
    }
}

/// Handle an incoming WebTransport connection.
///
/// This is the entry point for each client connection: accept session,
/// read JWT, validate, check registration, notify MC, then hold the
/// connection open. On disconnect, notifies MC and cleans up.
///
/// MC notifications are best-effort (fire-and-forget via `tokio::spawn`).
/// Notification failure does NOT affect the client connection.
///
/// # Errors
///
/// Returns `MhError` if session acceptance, JWT validation, or
/// meeting registration check fails.
#[tracing::instrument(skip_all, name = "mh.webtransport.connection", fields(connection_id = tracing::field::Empty))]
#[expect(
    clippy::too_many_lines,
    reason = "Connection lifecycle is sequential; splitting would fragment the accept-validate-register-notify-hold flow"
)]
pub async fn handle_connection(
    incoming: IncomingSession,
    jwt_validator: Arc<MhJwtValidator>,
    session_manager: SessionManagerHandle,
    mc_client: Arc<McClient>,
    handler_id: String,
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
            metrics::record_jwt_validation("success", "meeting", "none");
            claims
        }
        Err(e) => {
            warn!(
                target: "mh.webtransport.connection",
                connection_id = %connection_id,
                error = %e,
                "JWT validation failed"
            );
            metrics::record_jwt_validation("failure", "meeting", "validation_failed");
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

    // Step 5: Check meeting registration status and notify MC
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

        // Notify MC (best-effort, fire-and-forget)
        spawn_notify_connected(
            &mc_client,
            &session_manager,
            meeting_id,
            participant_id,
            &handler_id,
        )
        .await;
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
        match await_meeting_registration(
            &session_manager,
            meeting_id,
            &connection_id,
            &notify,
            register_meeting_timeout,
            &cancel_token,
        )
        .await
        {
            RegistrationOutcome::Registered => {
                // Notify MC about the now-promoted connection (best-effort)
                spawn_notify_connected(
                    &mc_client,
                    &session_manager,
                    meeting_id,
                    participant_id,
                    &handler_id,
                )
                .await;
            }
            RegistrationOutcome::Timeout => {
                // No MC disconnect notification — connection was never established with MC
                return Err(MhError::MeetingNotRegistered(meeting_id.clone()));
            }
            RegistrationOutcome::Cancelled => {
                return Ok(());
            }
        }
    }

    // Step 6: Hold connection open — monitor for disconnect or cancellation
    // The connection stays open for future media frame forwarding (separate story).
    // For now, we monitor the recv stream for closure and the cancellation token.
    //
    // Track the disconnect reason for MC notification
    let disconnect_reason;
    let mut probe_buf = [0u8; 1];
    loop {
        tokio::select! {
            () = cancel_token.cancelled() => {
                debug!(
                    target: "mh.webtransport.connection",
                    connection_id = %connection_id,
                    "Connection cancelled during shutdown"
                );
                // Server-initiated shutdown — not a client close or error
                disconnect_reason = proto_gen::internal::DisconnectReason::Unspecified;
                break;
            }
            result = recv_stream.read(&mut probe_buf) => {
                match result {
                    Ok(None) => {
                        info!(
                            target: "mh.webtransport.connection",
                            connection_id = %connection_id,
                            meeting_id = %meeting_id,
                            "Client disconnected"
                        );
                        disconnect_reason = proto_gen::internal::DisconnectReason::ClientClosed;
                        break;
                    }
                    Err(_) => {
                        info!(
                            target: "mh.webtransport.connection",
                            connection_id = %connection_id,
                            meeting_id = %meeting_id,
                            "Client disconnected with error"
                        );
                        disconnect_reason = proto_gen::internal::DisconnectReason::Error;
                        break;
                    }
                    Ok(Some(_)) => {
                        // Client sent data — ignore for now (media forwarding is future scope)
                    }
                }
            }
        }
    }

    // Step 7: Cleanup — remove connection from session manager and notify MC
    session_manager
        .remove_connection(meeting_id, &connection_id)
        .await;

    // Notify MC of disconnection (best-effort, fire-and-forget)
    if let Some(mc_endpoint) = session_manager.get_mc_endpoint(meeting_id).await {
        let mc_client = Arc::clone(&mc_client);
        let meeting_id = meeting_id.clone();
        let participant_id = participant_id.clone();
        let handler_id = handler_id.clone();
        let reason = disconnect_reason as i32;
        tokio::spawn(async move {
            if let Err(e) = mc_client
                .notify_participant_disconnected(
                    &mc_endpoint,
                    &meeting_id,
                    &participant_id,
                    &handler_id,
                    reason,
                )
                .await
            {
                warn!(
                    target: "mh.webtransport.connection",
                    error = %e,
                    meeting_id = %meeting_id,
                    "Failed to notify MC of participant disconnection"
                );
            }
        });
    }

    info!(
        target: "mh.webtransport.connection",
        connection_id = %connection_id,
        meeting_id = %meeting_id,
        participant_id = %participant_id,
        "Connection closed and cleaned up"
    );

    Ok(())
}

/// Spawn a best-effort `NotifyParticipantConnected` notification to MC.
///
/// Looks up the MC endpoint from `SessionManager`. If found, spawns the
/// notification as a fire-and-forget task. If the MC endpoint is not found
/// (should not happen for registered meetings), logs a warning and skips.
async fn spawn_notify_connected(
    mc_client: &Arc<McClient>,
    session_manager: &SessionManagerHandle,
    meeting_id: &str,
    participant_id: &str,
    handler_id: &str,
) {
    if let Some(mc_endpoint) = session_manager.get_mc_endpoint(meeting_id).await {
        let mc_client = Arc::clone(mc_client);
        let meeting_id = meeting_id.to_string();
        let participant_id = participant_id.to_string();
        let handler_id = handler_id.to_string();
        tokio::spawn(async move {
            if let Err(e) = mc_client
                .notify_participant_connected(
                    &mc_endpoint,
                    &meeting_id,
                    &participant_id,
                    &handler_id,
                )
                .await
            {
                warn!(
                    target: "mh.webtransport.connection",
                    error = %e,
                    meeting_id = %meeting_id,
                    "Failed to notify MC of participant connection"
                );
            }
        });
    } else {
        warn!(
            target: "mh.webtransport.connection",
            meeting_id = %meeting_id,
            "Cannot notify MC: no MC endpoint found for meeting"
        );
    }
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    //! Behavioral tests for [`await_meeting_registration`].
    //!
    //! Each test isolates metric recording via
    //! `::metrics::set_default_local_recorder` (RAII thread-local guard).
    //! `#[tokio::test]` uses the current-thread runtime, so the helper
    //! runs on the same thread that holds the guard, and recorder calls
    //! made across `.await` points are captured.
    //!
    //! These tests enforce the invariant documented on
    //! `::metrics::record_register_meeting_timeout`: the counter fires
    //! ONLY on the timeout arm of `await_meeting_registration`, never on
    //! the cancellation or registered arms.
    //!
    //! The timeout test uses `#[tokio::test(start_paused = true)]` for
    //! virtual-time control: with a paused clock on a current-thread
    //! runtime, `tokio::time::sleep` inside the helper's timeout arm is
    //! resolved by auto-advance when all other arms (notify, cancel) are
    //! idle, so `.await` on the helper completes in virtual — not real —
    //! time.
    use super::*;
    use crate::session::PendingConnection;
    use metrics_util::debugging::{DebugValue, DebuggingRecorder};
    use metrics_util::MetricKind;
    use std::time::Instant;

    const METRIC_NAME: &str = "mh_register_meeting_timeouts_total";
    const TEST_TIMEOUT: Duration = Duration::from_secs(15);
    const LONG_TIMEOUT: Duration = Duration::from_secs(30);

    /// Return the counter value for `mh_register_meeting_timeouts_total`,
    /// or `None` if the counter was never recorded against this recorder.
    fn timeout_counter_value(recorder: &DebuggingRecorder) -> Option<u64> {
        recorder
            .snapshotter()
            .snapshot()
            .into_vec()
            .into_iter()
            .find_map(|(composite, _unit, _desc, value)| {
                if composite.kind() == MetricKind::Counter && composite.key().name() == METRIC_NAME
                {
                    match value {
                        DebugValue::Counter(v) => Some(v),
                        _ => None,
                    }
                } else {
                    None
                }
            })
    }

    async fn setup_pending(
        session_manager: &SessionManagerHandle,
        meeting_id: &str,
        connection_id: &str,
    ) -> Arc<Notify> {
        session_manager
            .add_pending_connection(PendingConnection {
                connection_id: connection_id.to_string(),
                meeting_id: meeting_id.to_string(),
                participant_id: "user-1".to_string(),
                connected_at: Instant::now(),
            })
            .await
    }

    #[tokio::test(start_paused = true)]
    async fn timeout_arm_records_metric_once() {
        let recorder = DebuggingRecorder::new();
        let _guard = ::metrics::set_default_local_recorder(&recorder);

        let session_manager = SessionManagerHandle::new();
        let meeting_id = "meeting-1";
        let connection_id = "conn-1";
        let notify = setup_pending(&session_manager, meeting_id, connection_id).await;
        let cancel_token = CancellationToken::new();

        // Virtual time: notify and cancel_token stay idle, so tokio's
        // auto-advance fires the sleep arm; no real wall-clock wait.
        let outcome = await_meeting_registration(
            &session_manager,
            meeting_id,
            connection_id,
            &notify,
            TEST_TIMEOUT,
            &cancel_token,
        )
        .await;

        assert!(
            matches!(outcome, RegistrationOutcome::Timeout),
            "expected Timeout, got {outcome:?}"
        );
        assert_eq!(
            timeout_counter_value(&recorder),
            Some(1),
            "timeout arm must record the counter exactly once"
        );
    }

    #[tokio::test]
    async fn cancel_arm_does_not_record_metric() {
        let recorder = DebuggingRecorder::new();
        let _guard = ::metrics::set_default_local_recorder(&recorder);

        let session_manager = SessionManagerHandle::new();
        let meeting_id = "meeting-2";
        let connection_id = "conn-2";
        let notify = setup_pending(&session_manager, meeting_id, connection_id).await;
        let cancel_token = CancellationToken::new();
        // Pre-cancel: the cancelled() future resolves immediately on poll,
        // so the cancel arm wins the select! before LONG_TIMEOUT elapses.
        cancel_token.cancel();

        let outcome = await_meeting_registration(
            &session_manager,
            meeting_id,
            connection_id,
            &notify,
            LONG_TIMEOUT,
            &cancel_token,
        )
        .await;

        assert!(
            matches!(outcome, RegistrationOutcome::Cancelled),
            "expected Cancelled, got {outcome:?}"
        );
        assert!(
            matches!(timeout_counter_value(&recorder), None | Some(0)),
            "cancel arm must not record the timeout counter, got {:?}",
            timeout_counter_value(&recorder)
        );
    }

    #[tokio::test]
    async fn registered_arm_does_not_record_metric() {
        let recorder = DebuggingRecorder::new();
        let _guard = ::metrics::set_default_local_recorder(&recorder);

        let session_manager = SessionManagerHandle::new();
        let meeting_id = "meeting-3";
        let connection_id = "conn-3";
        let notify = setup_pending(&session_manager, meeting_id, connection_id).await;
        let cancel_token = CancellationToken::new();

        // Pre-fire the Notify: its permit is held until the first
        // `.notified()` consumes it, so the notified() future inside
        // await_meeting_registration completes immediately on poll.
        notify.notify_one();

        let outcome = await_meeting_registration(
            &session_manager,
            meeting_id,
            connection_id,
            &notify,
            LONG_TIMEOUT,
            &cancel_token,
        )
        .await;

        assert!(
            matches!(outcome, RegistrationOutcome::Registered),
            "expected Registered, got {outcome:?}"
        );
        assert!(
            matches!(timeout_counter_value(&recorder), None | Some(0)),
            "registered arm must not record the timeout counter, got {:?}",
            timeout_counter_value(&recorder)
        );
    }
}
