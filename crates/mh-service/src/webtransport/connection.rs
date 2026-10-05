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
use crate::config::{
    EGRESS_QUEUE_FRAMES, INGRESS_QUEUE_FRAMES, MC_UNAVAILABLE_CLOSE_JITTER_MAX_MS,
    NOMINAL_AUDIO_FRAME_BYTES,
};
use crate::errors::MhError;
use crate::grpc::McClient;
use crate::media::forward::EgressQueue;
use crate::media::forwarder::ConnectionForwarder;
use crate::media::ingress::{
    run_egress, run_forward, run_ingress, FramesRead, IngressQueue, LoopExit,
};
use crate::media::queue::SharedQueue;
use crate::media::{MediaSetup, MediaTaskContext};
use crate::observability::metrics;
use crate::routing::{IdError, MeetingKey, SenderId};
use crate::session::{ConnectionEntry, PendingConnection, SessionManagerHandle};
use crate::webtransport::WtMediaTransport;

use prost::Message;
use proto_gen::dark_tower::internal::v1::{DisconnectReason, NotifyParticipantConnectedResponse};
use proto_gen::dark_tower::signaling::v1::{mh_client_message, MhClientMessage};
use rand::Rng;
use std::collections::HashMap;
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
    // LOST-WAKEUP GUARD. `notify_waiters` wakes only `Notified` futures that
    // already exist, and this connection's pending entry was added by an
    // earlier message: if `RegisterMeeting` was processed between that reply and
    // this point, its wake-up went to nobody and a plain `select!` would sit out
    // the whole timeout for a meeting that is already registered — then kick
    // the client. So create (and enable) the future FIRST, then re-check: a
    // registration from here on is caught by the future, one before it by the
    // check. A registration that ran first also promoted this connection, so
    // `Registered` is the truthful answer.
    let notified = notify.notified();
    tokio::pin!(notified);
    notified.as_mut().enable();
    if session_manager.is_meeting_registered(meeting_id).await {
        info!(
            target: "mh.webtransport.connection",
            connection_id = %connection_id,
            meeting_id = %meeting_id,
            "Pending connection promoted by a RegisterMeeting that landed before it began waiting"
        );
        return RegistrationOutcome::Registered;
    }
    tokio::select! {
        () = notified => {
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
#[expect(
    clippy::too_many_arguments,
    reason = "one lifecycle entry point; every argument is a collaborator resolved once at process setup and threaded through the accept loop"
)]
pub async fn handle_connection(
    incoming: IncomingSession,
    jwt_validator: Arc<MhJwtValidator>,
    session_manager: SessionManagerHandle,
    mc_client: Arc<McClient>,
    handler_id: String,
    register_meeting_timeout: Duration,
    media: MediaSetup,
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

    // Also the `connection_id` on this session's MH→MC notifications
    // (`internal.proto`), and this ONE binding is the only source for it: minted
    // once here, random, never reused within the process, and threaded — never
    // re-minted — into the Connected call, every retry of it, and whichever of
    // the two disconnect paths runs. MC keys live connectivity on it, so a
    // second mint anywhere would let a disconnect retire the wrong session.
    let connection_id = uuid::Uuid::new_v4().to_string();
    tracing::Span::current().record("connection_id", connection_id.as_str());

    debug!(
        target: "mh.webtransport.connection",
        connection_id = %connection_id,
        "WebTransport session accepted"
    );

    // Wrapped in an `Arc` because the media tasks below hold the same
    // connection through `WtMediaTransport` (ADR-0036 §10's per-connection
    // seam) while this function keeps driving the lifecycle.
    let connection = Arc::new(connection);

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

    // Step 3: Read length-prefixed `MhClientMessage` (typed connect envelope)
    let envelope_bytes = read_framed_message(&mut recv_stream).await?;
    let envelope = MhClientMessage::decode(envelope_bytes.as_ref()).map_err(|e| {
        warn!(
            target: "mh.webtransport.connection",
            connection_id = %connection_id,
            error = %e,
            "Failed to decode MhClientMessage envelope"
        );
        MhError::WebTransportError("Invalid connect message".to_string())
    })?;

    // R-58: extract inbound W3C trace context from the envelope's two trace
    // fields and attach it as the parent of this span. Proto3-default empty
    // strings are skipped, so the map stays empty for connections with no
    // trace context — `extract()` then returns the input context unchanged
    // (no-op) and the span keeps today's default no-parent behavior.
    //
    // MUST go through the GLOBAL propagator (registered by `init_otel`, not a
    // fresh `TraceContextPropagator`) so `BoundedTraceContextPropagator`'s W3C
    // bounds-checking applies to this untrusted, browser-originated input —
    // same security contract as `otel_grpc::server_interceptor`. Keys are
    // literal `"traceparent"`/`"tracestate"` (matching
    // `otel_grpc::PROPAGATED_HEADERS`, not indexed here to avoid an
    // indexing_slicing deny on a foreign-crate slice) rather than a bespoke
    // `Extractor` impl — `HashMap<String, String>` already implements
    // `Extractor` (see `otel_grpc.rs` tests, which use the same shape).
    let mut trace_carrier: HashMap<String, String> = HashMap::new();
    if !envelope.trace_parent.is_empty() {
        trace_carrier.insert("traceparent".to_string(), envelope.trace_parent.clone());
    }
    if !envelope.trace_state.is_empty() {
        trace_carrier.insert("tracestate".to_string(), envelope.trace_state.clone());
    }
    let parent_cx = opentelemetry::global::get_text_map_propagator(|propagator| {
        propagator.extract(&trace_carrier)
    });
    //
    // INVARIANT: no span may be created under the connection span before this
    // reparent: tracing-opentelemetry >=0.32 starts the parent's context when a
    // child is created, and set_parent then fails AlreadyStarted (warned by
    // `set_remote_parent`).
    common::observability::otel::set_remote_parent(&tracing::Span::current(), parent_cx);

    // Decode failure, empty oneof, and unknown variant all collapse to a single
    // generic client-facing error: in all three the JWT was never extracted, so
    // validation never ran — splitting labels here would be a distinction without
    // a difference for the accept_loop's status=error observable.
    let token = match envelope.message {
        Some(mh_client_message::Message::ConnectRequest(req)) => req.join_token,
        None => {
            warn!(
                target: "mh.webtransport.connection",
                connection_id = %connection_id,
                "MhClientMessage carries no oneof variant"
            );
            return Err(MhError::WebTransportError(
                "Invalid connect message".to_string(),
            ));
        }
    };

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
    // PROVENANCE, load-bearing: `participant_id` is the validated meeting token's
    // `sub` and nothing else. It is now an authorization query key — the value MC
    // answers an identity question about (`NotifyParticipantConnected`) and the
    // value MH binds to a media route — not a payload field. Sourcing it from the
    // client's connect envelope, or re-deriving it downstream, would make the
    // identity client-assertable, which is the cross-participant injection
    // primitive this whole contract exists to close. `internal.proto`'s
    // `NotifyParticipantConnectedResponse` paragraph points back at this line by
    // name; this is where the hazard would be introduced (@security S1/SEC-4).
    let participant_id = &claims.sub;

    info!(
        target: "mh.webtransport.connection",
        connection_id = %connection_id,
        meeting_id = %meeting_id,
        "JWT validation succeeded"
    );

    // Record handshake duration (session accept through JWT validation)
    metrics::record_webtransport_handshake_duration(handshake_start.elapsed());

    // This connection's own close signal. `EndMeeting` release fires it for
    // every ACTIVE connection of the released meeting (story 2 R-20); a child of
    // the server shutdown token, so shutdown still reaches everything that
    // watches it. The media loops watch a child of THIS token, not the server's.
    let connection_close = cancel_token.child_token();

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
                    close: connection_close.clone(),
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
            close: connection_close.clone(),
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
                // Falls through to the shared media-session start sequence
                // below. Deliberately NOT its own copy of the ordering.
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

    // Step 5b: the media-session start sequence (ADR-0036 §2/§7/§11).
    //
    // AFTER the JWT gate, never before: the sender identity every forwarding
    // decision is made against comes from the validated token's meeting, and
    // starting media I/O on an accepted-but-unvalidated session would forward
    // frames for a participant nobody authenticated.
    //
    // `participant_id` is threaded from ONE read of the validated token's `sub`
    // — it is the value MC answers an identity question about and the value MH
    // binds to a media route, and those must be the same binding rather than two
    // derivations that happen to agree. Two reads would be two places for a
    // later edit to substitute a client-supplied hint into one of them.
    //
    // This is the SIBLING half of the layout constraint: spawning, logging and
    // teardown live here, and `crate::media` holds only the loops.
    let meeting_key = MeetingKey::new(meeting_id);
    let media_cancel = connection_close.child_token();
    let media_session = match resolve_sender_binding(
        &mc_client,
        &session_manager,
        &meeting_key,
        meeting_id,
        participant_id,
        &connection_id,
        &handler_id,
    )
    .await
    {
        SenderBindingOutcome::Bound(sender) => {
            let session = start_media_session(
                &session_manager,
                &media,
                &connection,
                &meeting_key,
                sender,
                &connection_id,
                &media_cancel,
            );
            metrics::record_media_session_start(metrics::MediaSessionStartOutcome::Started);
            info!(
                target: "mh.webtransport.connection",
                connection_id = %connection_id,
                meeting_id = %meeting_id,
                "Media forward path started"
            );
            session
        }
        SenderBindingOutcome::Declined(outcome) => {
            metrics::record_media_session_start(outcome);
            close_declined_connection(
                &mc_client,
                &session_manager,
                meeting_id,
                participant_id,
                &connection_id,
                &handler_id,
                outcome,
            )
            .await;
            // Every datagram this connection received is discarded: no ingress
            // loop is ever spawned on a declined session, so there is no reader
            // to have taken any of them off the transport. No difference is
            // needed here, and none is available — the whole received count IS
            // the loss.
            //
            // Sampled AFTER the close, not before, and the ordering is the
            // honest half of the claim. `close_declined_connection` awaits a
            // gRPC notification, and on the MC-unavailable arm it also sleeps a
            // jittered interval before closing — a client that is publishing
            // sends throughout that window. Sampling first would have silently
            // under-counted by exactly the traffic the decline arms take
            // longest to shed, which is the case this token exists for.
            //
            // The residue that remains is the flight time of the QUIC close
            // itself: a datagram already in the air when the close frame goes
            // out is discarded and not counted here. Bounded by one RTT and
            // stated rather than papered over — the catalog says "exact" about
            // the no-reader property, not about a race with the wire.
            metrics::record_media_frames_dropped(
                metrics::MediaDropReason::NoMediaSession,
                quic_datagram_frames_received(&connection),
            );
            // Counted, closed, and NOT an error: returning `Ok` keeps the
            // decline off `mh_webtransport_connections_total{status="error"}`
            // (F9). The connection drops with this frame.
            return Ok(());
        }
    };

    // Step 6: Hold connection open — monitor for disconnect or cancellation
    // The connection stays open for future media frame forwarding (separate story).
    // For now, we monitor the recv stream for closure and the cancellation token.
    //
    // WHY the connection ended decides whether MC is told (see `ConnectionEnd`).
    let connection_end;
    let mut probe_buf = [0u8; 1];
    loop {
        tokio::select! {
            () = connection_close.cancelled() => {
                // Two server-side causes share this arm; the parent token tells
                // them apart. Neither is a client close or an error.
                if cancel_token.is_cancelled() {
                    debug!(
                        target: "mh.webtransport.connection",
                        connection_id = %connection_id,
                        "Connection cancelled during shutdown"
                    );
                    connection_end = ConnectionEnd::Shutdown;
                } else {
                    // `EndMeeting` released this meeting (or the connection
                    // arrived for one already released). MH closing the
                    // connection is NOT the client's meeting-ended signal —
                    // MC's signalling is — and MC is deliberately NOT told:
                    // see `ConnectionEnd::MeetingReleased`.
                    info!(
                        target: "mh.webtransport.connection",
                        connection_id = %connection_id,
                        meeting_id = %meeting_id,
                        "Closing connection: its meeting was released by EndMeeting"
                    );
                    connection_end = ConnectionEnd::MeetingReleased;
                }
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
                        connection_end = ConnectionEnd::ClientClosed;
                        break;
                    }
                    Err(_) => {
                        info!(
                            target: "mh.webtransport.connection",
                            connection_id = %connection_id,
                            meeting_id = %meeting_id,
                            "Client disconnected with error"
                        );
                        connection_end = ConnectionEnd::Error;
                        break;
                    }
                    Ok(Some(_)) => {
                        // Client sent data — ignore for now (media forwarding is future scope)
                    }
                }
            }
        }
    }

    // Media teardown, before the session-state cleanup below: cancel the three
    // loops and drop this connection's egress queue out of the subscriber
    // registry, so no other connection's forward loop pushes into a queue
    // nothing will ever drain.
    media_cancel.cancel();
    // Compare-and-remove keyed on this connection id, symmetric to the sender
    // binding below: a connection superseded by a same-participant reconnect
    // holds the same ordinal and must not clear the live successor's egress queue
    // (SEC-2).
    session_manager
        .subscribers()
        .unregister(&meeting_key, media_session.sender, &connection_id);
    // Release the ordinal for the uniqueness check. Compare-and-remove keyed on
    // this connection id: a connection superseded by a reconnect no longer holds
    // the entry and must not clear the live one's.
    session_manager
        .sender_bindings()
        .unbind(&meeting_key, media_session.sender, &connection_id);
    // The ingress arm is awaited SEPARATELY from the other two, and only
    // because it carries the frame tally the delta below needs.
    //
    // PRIVACY, and it is why this arm is hand-written rather than folded back
    // into the array: the log statement here is BYTE-IDENTICAL to what the
    // array emits for the other two loops, and `FramesRead` deliberately
    // appears in no `tracing` field. A per-connection count of media frames is
    // talk duration for this participant, `connection_id` is logged beside
    // `participant_id` further down, and `crate::media::ingress`'s module docs
    // bar exactly this — a sibling formatting a per-frame-derived value into a
    // log line, by the second route a directory-scoped macro deny structurally
    // cannot see. `FramesRead` has no `Display`, no `Debug` and no accessor
    // returning the count, so adding a field here is a compile error rather
    // than a review miss; do not "improve" this arm by logging the new value.
    let frames_read = match media_session.ingress.await {
        Ok((exit, frames_read)) => {
            match exit {
                LoopExit::ConnectionClosed => debug!(
                    target: "mh.webtransport.connection",
                    connection_id = %connection_id,
                    loop_name = "ingress",
                    "Media loop exited: connection closed"
                ),
                LoopExit::Cancelled => debug!(
                    target: "mh.webtransport.connection",
                    connection_id = %connection_id,
                    loop_name = "ingress",
                    "Media loop exited: cancelled"
                ),
            }
            Some(frames_read)
        }
        Err(e) => {
            warn!(
                target: "mh.webtransport.connection",
                connection_id = %connection_id,
                loop_name = "ingress",
                error = %e,
                "Media loop task failed"
            );
            None
        }
    };

    // Datagrams quinn received on this connection that the ingress loop never
    // read: evicted from quinn's receive buffer before the loop existed, shed
    // mid-session while the loop was behind, or arrived after teardown.
    //
    // Read AFTER awaiting the ingress task, never before: a datagram arriving
    // between the read and the loop's last `recv_datagram` would otherwise be
    // counted as unread while the loop was about to read it.
    //
    // FAIL CLOSED TO SILENCE on a join error. Without `frames_read` the
    // difference is unknowable, and publishing `frame_rx.datagram` alone would
    // report every datagram this connection ever received as lost. A missing
    // observation is recoverable; a confidently wrong one sends the next
    // responder where this task's own escalation went. The join error itself is
    // NOT silent — the `warn!` above carries it.
    if let Some(frames_read) = frames_read {
        metrics::record_media_frames_dropped(
            metrics::MediaDropReason::TransportReceiveDropped,
            frames_read.unread_since(quic_datagram_frames_received(&connection)),
        );
    }

    for (name, task) in [
        ("forward", media_session.forward),
        ("egress", media_session.egress),
    ] {
        match task.await {
            Ok(LoopExit::ConnectionClosed) => debug!(
                target: "mh.webtransport.connection",
                connection_id = %connection_id,
                loop_name = name,
                "Media loop exited: connection closed"
            ),
            Ok(LoopExit::Cancelled) => debug!(
                target: "mh.webtransport.connection",
                connection_id = %connection_id,
                loop_name = name,
                "Media loop exited: cancelled"
            ),
            Err(e) => warn!(
                target: "mh.webtransport.connection",
                connection_id = %connection_id,
                loop_name = name,
                error = %e,
                "Media loop task failed"
            ),
        }
    }

    // Step 7: Cleanup — remove connection from session manager and notify MC
    session_manager
        .remove_connection(meeting_id, &connection_id)
        .await;

    // Notify MC of disconnection (best-effort, fire-and-forget) — unless the
    // cause rules it out. Decided by the CAUSE, never by whether a registration
    // happens to exist at this moment: see `ConnectionEnd::MeetingReleased`.
    let mc_endpoint = match connection_end.mc_disconnect_reason() {
        Some(reason) => session_manager
            .get_mc_endpoint(meeting_id)
            .await
            .map(|endpoint| (endpoint, reason)),
        None => None,
    };
    if let Some((mc_endpoint, reason)) = mc_endpoint {
        let mc_client = Arc::clone(&mc_client);
        let meeting_id = meeting_id.clone();
        let participant_id = participant_id.clone();
        let handler_id = handler_id.clone();
        // The SAME id this session's `NotifyParticipantConnected` carried: MC
        // retires only this session's connectivity entry, so a disconnect from
        // a session superseded by a reconnect cannot remove the live one.
        let connection_id = connection_id.clone();
        let reason = reason as i32;
        tokio::spawn(async move {
            if let Err(e) = mc_client
                .notify_participant_disconnected(
                    &mc_endpoint,
                    &meeting_id,
                    &participant_id,
                    &handler_id,
                    &connection_id,
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

/// DATAGRAM frames this QUIC connection has received, from quinn's own frame
/// accounting.
///
/// # `frame_rx.datagram`, NOT `udp_rx.datagrams` — one letter apart on the same struct
///
/// `frame_rx.datagram` (singular) counts QUIC **DATAGRAM frames**, which is what
/// carries media. `udp_rx.datagrams` (plural, `quinn-proto`'s `UdpStats`) counts
/// **UDP packets** on the connection, handshake and ACK-only packets included —
/// reading it here would produce a counter that is badly wrong and entirely
/// plausible-looking, and no test asserting merely "it went up" would catch it.
///
/// # Why this number sees frames MH never can
///
/// `quinn-proto`'s `connection/mod.rs` records `stats.frame_rx` on **every**
/// decoded frame, before the datagram reaches the accept-or-evict decision in
/// `connection/datagrams.rs`. That eviction loop discards the oldest queued
/// datagram with a bare `debug!` and nothing else — no counter MH can read, no
/// error surfaced to the application. So this is the only figure that includes
/// datagrams evicted before any MH code ran, which is precisely the population
/// [`MediaDropReason::TransportReceiveDropped`] exists to count.
///
/// # A STATS READ ONLY — this is not an I/O path
///
/// `quic_connection()` compiles here because `Cargo.toml` enables `wtransport`'s
/// `quinn` feature for `with_custom_transport`. That does **not** license using
/// the raw QUIC connection for datagram I/O: datagrams sent through it skip the
/// HTTP/3 session-id varint and are unattributable to a WebTransport session.
/// See the seam's clause on this in `crate::transport`. Nothing below reads or
/// writes a datagram; it reads a counter quinn already maintains.
fn quic_datagram_frames_received(connection: &wtransport::Connection) -> u64 {
    connection.quic_connection().stats().frame_rx.datagram
}

/// Why an established connection's hold loop ended — which decides whether, and
/// with which reason, MC is sent `NotifyParticipantDisconnected`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConnectionEnd {
    /// The client closed its stream.
    ClientClosed,
    /// The stream failed.
    Error,
    /// Server shutdown.
    Shutdown,
    /// `EndMeeting` released the connection's meeting (story 2 R-20), or the
    /// connection arrived for a meeting already released.
    MeetingReleased,
}

impl ConnectionEnd {
    /// The reason MC is told, or `None` when MC must NOT be told.
    ///
    /// `MeetingReleased` is `None` STRUCTURALLY, not because the registration
    /// "is already gone" by the time the notification would be sent: between the
    /// release and this task's cleanup, MC may legitimately re-create the same
    /// `meeting_id` (a fresh install from generation 1), and a lookup would then
    /// find the NEW registration's endpoint and report the OLD connection's
    /// disconnect to it. MC keys that removal on (meeting, participant, handler),
    /// not on the connection, so it would drop the participant's NEW connection
    /// on this handler and could not tell the stale notice from a real one. MC
    /// ended the meeting; it needs no per-participant notice of its own teardown.
    const fn mc_disconnect_reason(self) -> Option<DisconnectReason> {
        match self {
            Self::ClientClosed => Some(DisconnectReason::ClientClosed),
            Self::Error => Some(DisconnectReason::Error),
            Self::Shutdown => Some(DisconnectReason::Unspecified),
            Self::MeetingReleased => None,
        }
    }
}

/// The three media tasks one connection owns, plus the sender they are bound
/// to.
struct MediaSession {
    sender: SenderId,
    ingress: tokio::task::JoinHandle<(LoopExit, FramesRead)>,
    forward: tokio::task::JoinHandle<LoopExit>,
    egress: tokio::task::JoinHandle<LoopExit>,
}

/// Start this connection's media forward path.
///
/// # The sender is a PARAMETER, and that is the safety property
///
/// `sender` is a validated [`SenderId`] obtained from MC's
/// `NotifyParticipantConnectedResponse` — the only server-to-server contract
/// that carries the participant → ordinal association. It is taken by value
/// rather than looked up here, so **there is no code path on which this function
/// runs with a sender that did not come from a successful, validated
/// `NotifyParticipantConnectedResponse`**. The type of the parameter is what
/// enforces that; a lookup returning `Option` would make it a property of the
/// call graph that every future reader has to re-derive.
///
/// The alternatives to the control-plane binding — reading the ordinal out of
/// the frame's `SFrame` key id, or accepting one the client asserts in its
/// connect envelope — are cross-participant injection primitives, not
/// shortcuts. See `crate::session::SenderBindings`.
fn start_media_session(
    session_manager: &SessionManagerHandle,
    media: &MediaSetup,
    connection: &Arc<wtransport::Connection>,
    meeting: &MeetingKey,
    sender: SenderId,
    connection_id: &str,
    cancel: &CancellationToken,
) -> MediaSession {
    let transport = Arc::new(WtMediaTransport::new(Arc::clone(connection)));
    let context = Arc::new(MediaTaskContext {
        routing: session_manager.routing_table(),
        subscribers: Arc::clone(session_manager.subscribers()),
        handles: Arc::clone(&media.handles),
    });

    let ingress_queue: Arc<IngressQueue> = Arc::new(SharedQueue::new(INGRESS_QUEUE_FRAMES));
    let egress_queue: Arc<EgressQueue> = Arc::new(SharedQueue::new(EGRESS_QUEUE_FRAMES));

    // Registered BEFORE the loops start: a frame arriving from another
    // participant between spawn and registration would otherwise count as
    // `no_local_subscriber` against a subscriber that is in fact connected.
    session_manager
        .subscribers()
        .register(meeting.clone(), sender, connection_id, &egress_queue);

    let forwarder = ConnectionForwarder::new(
        meeting.clone(),
        sender,
        Arc::clone(&media.handles),
        media.latency_sample_ratio,
        NOMINAL_AUDIO_FRAME_BYTES,
    );

    let ingress = tokio::spawn(run_ingress(
        Arc::clone(&transport),
        Arc::clone(&ingress_queue),
        Arc::clone(&context),
        cancel.clone(),
    ));
    let forward = tokio::spawn(run_forward(
        Arc::clone(&ingress_queue),
        forwarder,
        Arc::clone(&context),
        cancel.clone(),
    ));
    let egress = tokio::spawn(run_egress(transport, egress_queue, context, cancel.clone()));

    MediaSession {
        sender,
        ingress,
        forward,
        egress,
    }
}

/// Tear down a connection whose media session was declined.
///
/// # Why the connection closes rather than staying up
///
/// A connection MH cannot bind carries no media, and before this contract it
/// stayed open anyway — healthy to every observer, silent to the user, reported
/// by nothing but a log line. Closing is what makes the failure countable and
/// lets the client reconnect into a handler that can serve it.
///
/// **No close reason crosses to the client.** MH closes by dropping the
/// connection; it calls `wtransport::Connection::close()` nowhere, so no reason
/// string is emitted at all. That is deliberate and is stronger than a bounded
/// static: a reject arm is the natural place to "helpfully" include the
/// offending value, and that value is the identity this whole contract exists to
/// protect. If a close **code** is ever added for the retryable/terminal
/// distinction (`docs/TODO.md`), it stays a bounded code, never a string, and
/// never carries an identifier.
///
/// # A decline is NOT a handler error — it must not reach `status="error"`
///
/// This returns `()`, not `Result`, on purpose. A deliberate, fail-closed
/// decline is already counted once on `mh_media_session_starts_total{outcome}`,
/// which is its home at full resolution. If it also returned `Err`, the accept
/// loop (`webtransport/server.rs`) would increment
/// `mh_webtransport_connections_total{status="error"}` for it — and that series
/// feeds `mh-deployment.md`'s immediate-rollback gate on `{status!="accepted"}`.
/// Rolling MH back restores the pre-contract silent-no-media behaviour, so
/// declines stop and the ratio recovers: a control that *inverts* under the
/// exact fault it exists to catch (@operations / @observability F9). The
/// connection still closes — MH drops it when `handle_connection` returns — so
/// nothing about the close weakens; only the error-series pollution is removed.
///
/// # The disconnect notification is conditional, and the jitter is one-armed
///
/// [`mc_may_hold_a_registration`] decides whether `NotifyParticipantDisconnected`
/// is owed: MC that registered this connection (definitely, having answered, or
/// possibly, having timed out after recording it) must be told it is going
/// away, while MC that never reached its handler saw no connection and would
/// receive a disconnection for something it never knew about — a state
/// divergence, not a courtesy.
///
/// The jitter applies to the MC-unreachable arm ONLY; see
/// [`MC_UNAVAILABLE_CLOSE_JITTER_MAX_MS`]. The other arms are MC's *answer*,
/// where retrying is pointless and an immediate terminal close is correct.
async fn close_declined_connection(
    mc_client: &Arc<McClient>,
    session_manager: &SessionManagerHandle,
    meeting_id: &str,
    participant_id: &str,
    connection_id: &str,
    handler_id: &str,
    outcome: metrics::MediaSessionStartOutcome,
) {
    session_manager
        .remove_connection(meeting_id, connection_id)
        .await;

    if mc_may_hold_a_registration(outcome) {
        if let Some(mc_endpoint) = session_manager.get_mc_endpoint(meeting_id).await {
            if let Err(e) = mc_client
                .notify_participant_disconnected(
                    &mc_endpoint,
                    meeting_id,
                    participant_id,
                    handler_id,
                    // The id the abandoned or refused `NotifyParticipantConnected`
                    // carried — one binding in `handle_connection`, never re-minted —
                    // so MC can retire exactly the entry it may hold.
                    connection_id,
                    proto_gen::dark_tower::internal::v1::DisconnectReason::Error as i32,
                )
                .await
            {
                warn!(
                    target: "mh.webtransport.connection",
                    error = %e,
                    meeting_id = %meeting_id,
                    "Failed to notify MC that a declined connection is going away"
                );
            }
        }
    }

    if outcome == metrics::MediaSessionStartOutcome::DeclinedMcUnavailable {
        let jitter = rand::thread_rng().gen_range(0..=MC_UNAVAILABLE_CLOSE_JITTER_MAX_MS);
        tokio::time::sleep(Duration::from_millis(jitter)).await;
    }

    // No return value: the decline is fully handled and counted. The connection
    // closes when `handle_connection` returns and drops it (§S8 — close by drop,
    // no reason string). Returning `Err` here would double-count the decline as
    // a connection error (F9).
}

/// The outcome of one connection's media-session start sequence.
///
/// Either the connection is bound and forwarding, or it is declined with a
/// reason — there is no third state in which the connection stays up and
/// silently carries no media. That state used to exist and is what this
/// devloop removes.
enum SenderBindingOutcome {
    /// MC named an ordinal, it validated, and it was claimed. The connection
    /// may start its media loops.
    Bound(SenderId),
    /// The connection must be closed. Carries the counted outcome, which is
    /// also what decides whether a disconnect notification is owed on the way
    /// out — see [`mc_may_hold_a_registration`].
    Declined(metrics::MediaSessionStartOutcome),
}

/// Whether MC may hold a registration for this connection that MH now owes a
/// `NotifyParticipantDisconnected` to clear.
///
/// **Derived from the outcome rather than carried alongside it**, because the
/// outcome already records how far the start sequence got and two fields
/// encoding one fact can disagree. The question is not "did MC *answer*" — it is
/// "could MC be left holding a live registration if MH walks away silently",
/// and only MH can clear one.
///
/// **What a ghost entry costs** (story 2 task 20). MC records this connection
/// as *connectivity* — participant P is reachable on this handler — in the
/// meeting actor, in the same turn that resolves the ordinal, and routes media
/// edges from it: a subscriber hears a sender only through a handler both are
/// connected to. A ghost entry is therefore **phantom connectivity**: MC keeps
/// placing edges for P on a handler P is no longer on, and those edges carry
/// nothing — silence, not misdelivered media — and no later event from MH is
/// guaranteed to clear it: this notice is the clearing event. (Before task 20
/// the same ghost cost one slot of a per-meeting connection cap; the registry
/// holding that cap is gone, and the stakes of this predicate went up, not
/// down.)
///
/// MC records the connection **before** the RPC returns, so a slow MC can
/// record it and still miss MH's deadline. That splits the outcomes three ways:
///
/// - **MC answered** (`0`, out-of-range, or an ordinal MH refused as a
///   collision): the handler ran to completion, so the connection may be
///   recorded — notify. MC records connectivity only for a resolved, in-set
///   participant, so after a `0` there is usually nothing to clear; the notice
///   is then a no-op MC counts, and that is the cheap side of the trade.
/// - **RPC did not complete** (`DeclinedMcUnavailable`: timeout, transport
///   failure, or a non-auth status after every retry): MH **cannot know**
///   whether a ghost entry exists. Notify anyway — a disconnect for something
///   MC does not hold is a counted no-op, so the false positive costs one RPC
///   while the false negative is phantom connectivity for the rest of the
///   session. This is the arm @security flagged as the S4 shape on the side S4
///   did not reach.
/// - **The request never reached MC's handler** (`DeclinedMcAuthRejected`: MC's
///   auth interceptor refused MH's credential *before* the handler, or MH never
///   built one; `DeclinedMcEndpointUnknown`: no dialable endpoint, so nothing
///   was sent): MC never recorded anything, so there is nothing to clear —
///   notifying would tell MC about a connection it never saw, a state
///   divergence rather than a courtesy.
///
/// The `match` is wildcard-free on purpose: a seventh outcome has to be
/// classified here deliberately rather than defaulted into whichever answer the
/// wildcard happened to give.
const fn mc_may_hold_a_registration(outcome: metrics::MediaSessionStartOutcome) -> bool {
    match outcome {
        metrics::MediaSessionStartOutcome::DeclinedNoSenderBinding
        | metrics::MediaSessionStartOutcome::DeclinedSenderBindingOutOfRange
        | metrics::MediaSessionStartOutcome::DeclinedSenderBindingConflict
        // Uncertain, so notify: MC may have recorded the connection and then
        // missed MH's deadline, and a disconnect for an entry MC does not hold
        // is a no-op, so the safe default is to clear what might exist.
        | metrics::MediaSessionStartOutcome::DeclinedMcUnavailable => true,
        metrics::MediaSessionStartOutcome::DeclinedMcAuthRejected
        | metrics::MediaSessionStartOutcome::DeclinedMcEndpointUnknown => false,
        // Not a decline; never reaches the close path. Kept as its own arm
        // rather than folded into the never-registered arm because it answers a
        // different question with the same value: those are "MC never registered
        // this connection", whereas `Started` is "not a decline at all" and
        // never reaches this predicate's caller on the close path. Collapsing
        // them would erase that split.
        #[expect(
            clippy::match_same_arms,
            reason = "`Started` is not a decline and never reaches the close path; \
                      its `false` is a distinct fact from the never-registered declines above"
        )]
        metrics::MediaSessionStartOutcome::Started => false,
    }
}

/// Obtain and claim this connection's `sender_id` from MC.
///
/// # This RPC is a blocking precondition, not an ack
///
/// `NotifyParticipantConnected` used to be fire-and-forget. Its response now
/// carries the **only** server-to-server statement of which ordinal this
/// participant publishes as (`internal.proto`
/// `NotifyParticipantConnectedResponse.sender_id`), so MH must await it and use
/// it: without it there is no forwarding, under the old behaviour and the new
/// one alike. What the old behaviour preserved was a WebTransport connection to
/// a *media* handler structurally incapable of carrying media, reported by
/// nothing but a log line.
///
/// The ordering is `JWT gate → MC endpoint → NotifyParticipantConnected →
/// validate → bind → start_media_session`, and it exists **once**: both the
/// already-registered and the promoted-after-provisional-accept paths call this
/// one helper rather than carrying two copies that can drift apart.
///
/// `acknowledged` is deliberately not read. It answers a different question —
/// received-and-parsed, not bound — and the two diverge: MC answers
/// `acknowledged: true` with `sender_id: 0` when it has no answer, so gating on
/// it would be fail-open. `sender_id` is the sole input to this decision.
///
/// Every terminal path here records `mh_media_session_starts_total` exactly
/// once, including the `Bound` one (recorded by the caller once the loops are
/// spawned), so the sum is a real denominator over connections that attempted a
/// media session.
async fn resolve_sender_binding(
    mc_client: &Arc<McClient>,
    session_manager: &SessionManagerHandle,
    meeting: &MeetingKey,
    meeting_id: &str,
    participant_id: &str,
    connection_id: &str,
    handler_id: &str,
) -> SenderBindingOutcome {
    let Some(mc_endpoint) = session_manager.get_mc_endpoint(meeting_id).await else {
        warn!(
            target: "mh.webtransport.connection",
            connection_id = %connection_id,
            meeting_id = %meeting_id,
            "Media session declined: no MC endpoint is known for this meeting, so MH cannot ask \
             which sender_id this participant publishes as. The meeting should have been \
             registered on this handler before a connection for it was accepted."
        );
        // Narrow in practice: the registration check runs just upstream, so this
        // needs the meeting to be unregistered in between, or the session actor
        // to be gone during shutdown. **The token's operator-facing meaning is
        // set by the common route** — a registration that named an endpoint MH
        // cannot dial, handled at the RPC error below, whose remedy is "fix what
        // `RegisterMeeting` carried". The shutdown route is a rare co-tenant
        // whose remedy is *nothing*, because it is shutdown; it is not what the
        // runbook entry describes and must not be claimed as such.
        return SenderBindingOutcome::Declined(
            metrics::MediaSessionStartOutcome::DeclinedMcEndpointUnknown,
        );
    };

    let response = match mc_client
        .notify_participant_connected(
            &mc_endpoint,
            meeting_id,
            participant_id,
            handler_id,
            connection_id,
        )
        .await
    {
        Ok(response) => response,
        Err(e) => {
            let outcome = decline_for_notify_error(&e);
            warn!(
                target: "mh.webtransport.connection",
                connection_id = %connection_id,
                meeting_id = %meeting_id,
                // The endpoint MC registered itself with, echoed back so the
                // runbook's first move for `declined_mc_endpoint_unknown` —
                // "read what the RegisterMeeting carried" — is answerable from
                // the logs instead of needing a `kubectl exec`. It is a service
                // address, not a participant or stream identity, so ADR-0036 §11
                // does not reach it.
                mc_grpc_endpoint = %mc_endpoint,
                error = %e,
                outcome = outcome.as_label(),
                "Media session declined: MH could not obtain a sender_id from MC. Closing rather \
                 than holding a connection that cannot carry media."
            );
            return SenderBindingOutcome::Declined(outcome);
        }
    };

    // `acknowledged` is intentionally NOT read — see this function's docs. It is
    // destructured away here rather than left reachable as `response.acknowledged`
    // so a later edit cannot casually gate on it.
    let NotifyParticipantConnectedResponse {
        acknowledged: _,
        sender_id,
    } = response;

    // `SenderId::from_wire` is the single-sourced bound: zero and out-of-range
    // are SEPARATE variants, so the two-event distinction is enforced by the
    // type rather than by remembering to branch here. There is deliberately no
    // shared `validate_16bit_id()` helper — `slot_id` zero is valid while
    // `sender_id` zero is this contract's reject signal, so a collapsed
    // validator would fail open on exactly the reserved value.
    let sender = match SenderId::from_wire(sender_id) {
        Ok(sender) => sender,
        Err(IdError::SenderIdZero) => {
            warn!(
                target: "mh.webtransport.connection",
                connection_id = %connection_id,
                meeting_id = %meeting_id,
                "Media session declined: MC has no sender_id for this participant. Several \
                 distinct causes — read MC's mc_media_sender_binding_responses_total, which \
                 separates them; MH sees only the refusal."
            );
            return SenderBindingOutcome::Declined(
                metrics::MediaSessionStartOutcome::DeclinedNoSenderBinding,
            );
        }
        Err(_) => {
            // ADR-0036 §11: the offending value is NOT named here. A reject arm
            // is the natural place to include it, and it is exactly the identity
            // the contract exists to protect. The range-vs-uniqueness
            // distinction a reader would want it for is carried by the `outcome`
            // label on mh_media_session_starts_total, not by this line.
            warn!(
                target: "mh.webtransport.connection",
                connection_id = %connection_id,
                meeting_id = %meeting_id,
                "Media session declined: MC returned a sender_id outside the 16-bit ordinal \
                 range. MC's allocator cannot produce this, so the candidates are field \
                 corruption in transit or a peer that is not MC."
            );
            return SenderBindingOutcome::Declined(
                metrics::MediaSessionStartOutcome::DeclinedSenderBindingOutOfRange,
            );
        }
    };

    // The last-hop uniqueness check. MH is the component that would ACT on a
    // collision — forwarding one participant's frames onto another's edges — so
    // MH is where it is refused. Incumbent wins.
    if let Err(conflict) = session_manager.sender_bindings().bind(
        meeting.clone(),
        participant_id,
        connection_id,
        sender,
    ) {
        // Again no ordinal in the line (§11). Two candidate causes and MH cannot
        // tell them apart: MC allocated one live ordinal to two participants, or
        // MH failed to unbind a previous holder. Hence the MH-side first move.
        warn!(
            target: "mh.webtransport.connection",
            connection_id = %connection_id,
            meeting_id = %meeting_id,
            conflict = ?conflict,
            "Media session declined: the sender_id MC returned is already held by a different \
             participant in this meeting. MH refused rather than overwrite — accepting would \
             have crossed media between participants. Check MH's unbind path for this meeting \
             before MC's allocator."
        );
        return SenderBindingOutcome::Declined(
            metrics::MediaSessionStartOutcome::DeclinedSenderBindingConflict,
        );
    }

    SenderBindingOutcome::Bound(sender)
}

/// The decline outcome for a `NotifyParticipantConnected` that returned no
/// response at all.
///
/// A registration that named an undialable endpoint and a reachable endpoint
/// that would not answer are DIFFERENT faults with different first moves — fix
/// the registration vs. fix MC or the network — so they get different outcomes.
/// `MhError::McEndpointInvalid` exists to carry that split: `MhError::Config`
/// would have folded in an auth-header parse failure, which is neither.
///
/// Three faults with three different services to open, so three outcomes. A
/// credential rejection in particular must NOT report as unavailability: MC
/// dialled fine and refused MH's token, so MC is healthy and the remedy is MH's
/// outbound auth. An expired service token fires it for every connection on the
/// handler at once.
fn decline_for_notify_error(error: &MhError) -> metrics::MediaSessionStartOutcome {
    match error {
        MhError::McEndpointInvalid(_) => {
            metrics::MediaSessionStartOutcome::DeclinedMcEndpointUnknown
        }
        // Two routes, one remedy: MC refused MH's credential, or MH could not
        // build one. Both are MH's outbound auth and both start at
        // `mh_token_refresh_total{status="error"}`; neither is a reason to open
        // MC's health. They differ only in timing — the refusal is terminal, the
        // build failure is retried because a refresh landing mid-budget can fix
        // it.
        MhError::JwtValidation(_) | MhError::OutboundAuthUnavailable(_) => {
            metrics::MediaSessionStartOutcome::DeclinedMcAuthRejected
        }
        _ => metrics::MediaSessionStartOutcome::DeclinedMcUnavailable,
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
    //! `common::observability::testing::MetricAssertion::snapshot()`, a
    //! thread-local recorder. `#[tokio::test]` uses the current-thread
    //! runtime, so the helper runs on the same thread that holds the
    //! snapshot, and recorder calls made across `.await` points are captured.
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
    use common::observability::testing::MetricAssertion;
    use std::time::Instant;

    const METRIC_NAME: &str = "mh_register_meeting_timeouts_total";
    const TEST_TIMEOUT: Duration = Duration::from_secs(15);
    const LONG_TIMEOUT: Duration = Duration::from_secs(30);

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
                close: tokio_util::sync::CancellationToken::new(),
            })
            .await
    }

    #[tokio::test(start_paused = true)]
    async fn timeout_arm_records_metric_once() {
        let snap = MetricAssertion::snapshot();

        let session_manager =
            SessionManagerHandle::new(crate::session::admission::test_admission());
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
        // Timeout arm records the counter exactly once.
        snap.counter(METRIC_NAME).assert_delta(1);
    }

    #[tokio::test]
    async fn cancel_arm_does_not_record_metric() {
        let snap = MetricAssertion::snapshot();

        let session_manager =
            SessionManagerHandle::new(crate::session::admission::test_admission());
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
        // Cancel arm must not record the timeout counter.
        snap.counter(METRIC_NAME).assert_unobserved();
    }

    #[tokio::test]
    async fn registered_arm_does_not_record_metric() {
        let snap = MetricAssertion::snapshot();

        let session_manager =
            SessionManagerHandle::new(crate::session::admission::test_admission());
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
        // Registered arm must not record the timeout counter.
        snap.counter(METRIC_NAME).assert_unobserved();
    }

    /// A connection closed by `EndMeeting` release never notifies MC, whatever
    /// registration exists by cleanup time; every other end does, with its own
    /// reason. The release half is the guarantee: were it decided by a later
    /// registration lookup, an MC that re-created the same meeting id in the
    /// window would be told a stale disconnect and drop the participant's NEW
    /// connection.
    #[test]
    fn a_released_meeting_never_notifies_mc_and_every_other_end_does() {
        assert_eq!(ConnectionEnd::MeetingReleased.mc_disconnect_reason(), None);
        assert_eq!(
            ConnectionEnd::ClientClosed.mc_disconnect_reason(),
            Some(DisconnectReason::ClientClosed)
        );
        assert_eq!(
            ConnectionEnd::Error.mc_disconnect_reason(),
            Some(DisconnectReason::Error)
        );
        assert_eq!(
            ConnectionEnd::Shutdown.mc_disconnect_reason(),
            Some(DisconnectReason::Unspecified)
        );
    }
}
