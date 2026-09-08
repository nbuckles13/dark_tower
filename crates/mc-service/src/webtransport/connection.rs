//! `ConnectionActor` - owns WebTransport streams and bridge loop.
//!
//! This actor lives in the webtransport layer (not the actor hierarchy) and:
//! 1. Receives a `JoinResult` from the meeting actor via oneshot
//! 2. Sends `JoinResponse` to the client over the WebTransport stream
//! 3. Runs the bridge loop forwarding `ParticipantUpdate` messages to the client
//! 4. Notifies the meeting when the connection drops (via `MeetingActorHandle`)

use crate::actors::messages::{DisconnectCause, JoinResult};
use crate::actors::{
    BoundedMhStatus, MeetingActorHandle, MeetingControllerActorHandle, MhState,
    ParticipantActorHandle,
};
use crate::auth::McJwtValidator;
use crate::errors::McError;
use crate::grpc::{MeetingProgramming, MhRegistrationClient};
use crate::media_routing::{
    compute_assignment, HandlerId, MeetingAssignment, MeetingRoutingInput, PolicyGenerations,
    PushDisposition, RoutingParticipant,
};
use crate::media_signaling::{
    build_send_directive, build_stream_assignments, CapabilityOutcome, DirectiveOutcome,
    HandlerUrls, MediaStreamPolicy, MuteOutcome, PlannedAudioSlot, ReceiveCapabilityDeclaration,
    SlotId, SourceMuteView,
};
use crate::observability::metrics;
use crate::redis::{MhAssignmentData, MhAssignmentStore};

use super::trace::{inject_current_context, reparent_current_span};

use crate::actors::messages::SignalingPayload;
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
    policy_generations: Arc<PolicyGenerations>,
    mc_id: String,
    mc_grpc_endpoint: String,
    client_media_config: crate::media_signaling::ClientMediaConfig,
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

    // Step 6b: parse the client's Ed25519 identity signing public key.
    //
    // ORDERING IS LOAD-BEARING — this runs AFTER JWT validation and the
    // meeting_id binding check, never before. An earlier revision placed it
    // ahead of `validate_meeting_token`, which let an UNAUTHENTICATED caller
    // probe a validation surface and receive a distinguishable response before
    // presenting a valid token, and made a bad-token join report
    // `identity_key_invalid` instead of `jwt_validation` — hiding auth failures
    // behind an input-validation error. Authenticate first, then validate
    // input. Do not move this above Step 5.
    //
    // THREE STATES, not two (ADR-0036 §4, `signaling.proto`):
    //   len 0  -> `None`: NO KEY PUBLISHED. A defined, contract-required state,
    //             admitted. Consumers MUST fail closed on this participant's
    //             frames — drop them, never fall back to accepting unsigned
    //             ones, and never treat "no key" as "skip verification".
    //   len 32 -> `Some(key)`, published on the roster verbatim.
    //   else   -> REJECT. No client-controlled blob of any other length ever
    //             reaches the roster, which MC fans out to every participant.
    //
    // Rejecting len 0 was considered and reversed: validation here is
    // length-only, so a hostile client satisfies it with 32 random bytes it
    // holds no private half for and is admitted anyway. Reject would have
    // excluded only honest clients that have not yet implemented the field.
    //
    // SECURITY FLOOR, so nothing downstream overclaims: an exact-length check on
    // an opaque 32-byte blob. NO `cnf` thumbprint check binds the key to the
    // meeting token — deferred to story 2 — so a present key is TRUST ON FIRST
    // USE, and a verifying signature proves only that all such frames came from
    // the same keyholder. Same-keyholder consistency, never a verified identity.
    let identity_public_key = match crate::media_admission::IdentityPublicKey::parse_join_field(
        &join_request.identity_public_key,
    ) {
        Ok(key) => key,
        Err(_) => {
            // Server-side: specific and bounded, so the rejection is
            // observable. Client-side: generic, so it is not an oracle for
            // which check failed — every wrong length is indistinguishable in
            // BOTH directions.
            warn!(
                target: "mc.webtransport.connection",
                connection_id = %connection_id,
                meeting_id = %meeting_id,
                "Rejecting join: identity public key length is neither 0 nor 32"
            );
            let err = McError::IdentityKeyInvalid;
            let _ = send_error(&mut send_stream, err.error_code(), &err.client_message()).await;
            metrics::record_session_join(
                "failure",
                Some(err.error_type_label()),
                join_start.elapsed(),
            );
            return Err(err);
        }
    };

    // Observability for the absent case (ADR-0036 §11). "Every client omits and
    // nobody notices" must not be the silent steady state, so presence is
    // counted on both arms, giving the metric its own denominator. Bounded
    // 2-value label; no participant and no meeting dimension.
    //
    // DELIBERATELY recorded here, at the parse, NOT after the join succeeds —
    // so this counts authenticated join *attempts that passed identity-key
    // validation*, and a join that later fails on capacity, `sender_id`
    // exhaustion, Conflict or Draining is still counted. That is the correct
    // denominator for the question this metric exists to answer, which is a
    // question about the CLIENT population ("are client builds publishing
    // keys?"), not about server admission: conditioning it on server capacity
    // would answer a different question, and it would make the series go dead
    // during precisely the incident (`sender_id_space_exhausted`) in which an
    // operator would consult it. `sum()` of this metric therefore does NOT
    // equal `mc_session_joins_total{status="success"}`; the catalog says so.
    metrics::record_join_identity_key_presence(if identity_public_key.is_some() {
        "present"
    } else {
        "absent"
    });

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
            identity_public_key,
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

    // Step 9: [ASYNC, first participant only] Program each assigned MH with the
    // meeting's forwarding policy (R-12, ADR-0036 §7/§8/§9).
    //
    // The trigger is UNCHANGED and that is deliberate: one push on assignment,
    // then confirm. Structural re-push on every join/leave, the periodic
    // re-assert cadence, the connectivity-loss trigger and dispatch jitter are
    // the handler-restart story and are NOT here — they land additively on
    // `PolicyGenerations`.
    let is_first_participant = join_result.participants.is_empty();
    if is_first_participant {
        debug!(
            target: "mc.webtransport.connection",
            connection_id = %connection_id,
            meeting_id = %meeting_id,
            "First participant joined — spawning async RegisterMeeting"
        );

        // Compute the assignment BEFORE the spawn, so a malformed one fails on
        // the join path where it can be logged against this connection.
        match build_routing_input(&join_result, &mh_data) {
            Ok(routing_input) => match compute_assignment(&routing_input) {
                Ok(assignment) => {
                    // Cloned for the spawned task: the caller still needs
                    // `mh_data` below to resolve this connection's handler urls.
                    let reg_mh_data = mh_data.clone();
                    let reg_mh_client = Arc::clone(&mh_client);
                    let reg_generations = Arc::clone(&policy_generations);
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
                                &reg_mh_data,
                                &assignment,
                                reg_generations.as_ref(),
                                &reg_meeting_id,
                                &reg_mc_id,
                                &reg_mc_grpc_endpoint,
                                &reg_cancel_token,
                            )
                            .await;
                        }
                        .instrument(span),
                    );
                }
                Err(e) => {
                    // Fail loud: no push at all is better than pushing a policy
                    // MH will reject whole, and it must not be silent.
                    error!(
                        target: "mc.register_meeting.trigger",
                        connection_id = %connection_id,
                        meeting_id = %meeting_id,
                        reason = e.label(),
                        error = %e,
                        "Forwarding assignment could not be computed; MH not programmed"
                    );
                }
            },
            Err(e) => {
                error!(
                    target: "mc.register_meeting.trigger",
                    connection_id = %connection_id,
                    meeting_id = %meeting_id,
                    error = %e,
                    "Routing input could not be built; MH not programmed"
                );
            }
        }
    } else {
        debug!(
            target: "mc.webtransport.connection",
            connection_id = %connection_id,
            meeting_id = %meeting_id,
            "Not first participant — skipping RegisterMeeting"
        );
    }

    // Step 9b: Resolve the per-connection media-signalling context (ADR-0036
    // §5, §6).
    //
    // Everything the receive-capability validation path needs is resolved HERE,
    // once, so that path never touches the meeting actor: the client-facing
    // handler urls (server-derived, from the Redis assignment — never from a
    // client's `MediaConnectionUpdate`) and the audio slot MC's forwarding
    // assignment routes this subscriber into.
    //
    // The assignment is computed for EVERY connection, not just the first
    // participant's. It is a pure function with no I/O and no actor hop, and
    // reading the planned slot out of its output is what keeps
    // `compute_assignment` the single producer of "which slot do I route into"
    // rather than having MC reconstruct it from `MAIN_AUDIO_SLOT_ID` separately.
    // The MH push remains first-participant-only and is unchanged.
    let media_context = build_media_signaling_context(
        &controller_handle,
        &join_result,
        &mh_data,
        &meeting_id,
        client_media_config,
    )
    .await;
    let mut media_context = match media_context {
        Ok(context) => Some(context),
        Err(outcome) => {
            // COUNTED, not merely warned. This is the one failure that silences
            // a client for its entire session, so it is the last place the
            // "every reason MC did not COMPOSE a directive lands on one counter"
            // claim may be allowed to leak — a WARN alone would make it visible
            // only to whoever happens to read the logs.
            //
            // Loud once, here, where it can be attributed to this connection —
            // and NOT re-logged per client message. The session stays usable:
            // the client simply is never directed to send.
            metrics::record_send_directive(outcome);
            warn!(
                target: "mc.webtransport.connection",
                connection_id = %connection_id,
                meeting_id = %meeting_id,
                outcome = outcome.label(),
                "Media signalling context unavailable; this connection will not receive a send \
                 directive or slot assignments"
            );
            None
        }
    };

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
        &mut media_context,
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
#[expect(
    clippy::too_many_arguments,
    reason = "bridge-loop wiring; all params are distinct per-connection dependencies"
)]
async fn run_bridge_loop(
    send_stream: &mut SendStream,
    recv_stream: &mut RecvStream,
    outbound_rx: &mut mpsc::Receiver<bytes::Bytes>,
    cancel_token: &CancellationToken,
    connection: &Connection,
    connection_id: &str,
    participant_handle: &ParticipantActorHandle,
    media: &mut Option<MediaSignalingContext>,
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
                        // Media signalling needs the per-connection context
                        // resolved at join. If it could not be built, the
                        // connection stays healthy and every other post-join
                        // message keeps working — the failure was already logged
                        // loudly at join and must not be re-logged per message.
                        match media.as_mut() {
                            Some(media) => {
                                handle_client_message(
                                    &data,
                                    connection_id,
                                    participant_handle,
                                    media,
                                )
                                .await;
                            }
                            None => {
                                handle_client_message_without_media(
                                    &data,
                                    connection_id,
                                    participant_handle,
                                )
                                .await;
                            }
                        }
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
/// - `ReceiveCapability` (ADR-0036 §6): validates the declaration, then emits a
///   send directive and the slot assignments composed from one meeting-state
///   snapshot.
/// - `MuteRequest` (ADR-0036 §5): records the reported client mute on the
///   meeting actor and re-conveys slot state. **Never touches the send
///   directive** — that invariant is enforced by the module boundary in
///   `media_signaling::directive`, which has no path to mute state at all.
/// - `ServerMuteRequest`: consumed as a rename of the former host-mute message
///   with NO behaviour change. Enforcement is at MH ingress and is story 2.
/// - All other messages: Ignored (logged at debug level).
///
/// # A client that never declares is never told to send
///
/// Directive emission is triggered by the capability declaration, not by the
/// join, and that is contractual rather than incidental — see the
/// `media_signaling` module doc. Such a connection stays healthy: no directive,
/// no assignments, no error, and every other post-join message keeps working.
async fn handle_client_message(
    data: &[u8],
    connection_id: &str,
    participant_handle: &ParticipantActorHandle,
    media: &mut MediaSignalingContext,
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
        Some(client_message::Message::ReceiveCapability(capability)) => {
            handle_receive_capability(
                &capability,
                &trace_parent,
                &trace_state,
                connection_id,
                participant_handle,
                media,
            )
            .await;
        }
        Some(client_message::Message::MuteRequest(request)) => {
            handle_mute_request(&request, connection_id, participant_handle, media).await;
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

/// Mute-work tokens a fresh connection may spend immediately.
///
/// Sized far above human behaviour: a push-to-talk user produces roughly one
/// toggle per utterance, so a burst of 8 covers any realistic flurry (unmute,
/// speak, mute, correct, unmute) without ever reaching the limiter.
///
/// A Rust constant rather than a config key, matching
/// [`MAX_MH_STATUSES_PER_UPDATE`] and [`MAX_CLIENT_STRING_BYTES`] in this file:
/// these are client-input security caps with no operator-tuning story.
const MUTE_WORK_BURST: u32 = 8;

/// Rejection RESPONSES a fresh connection may be sent immediately.
///
/// Sized for a client correcting itself, not for one looping: a legitimate SDK
/// gets its declaration wrong a handful of times at most (wrong slot id, then a
/// pin it should not have set, then right). 8 covers that with room.
const CAPABILITY_REJECTION_BURST: u32 = 8;

/// Milliseconds per refilled rejection-response token — 1 s.
///
/// Slower than the mute bucket because the two actions differ in kind: a mute
/// toggle is an ongoing user action for the life of the session, whereas
/// re-declaring capability is a correction that converges. A client still
/// rejecting once a second after the burst is looping, not converging.
const CAPABILITY_REJECTION_REFILL_INTERVAL_MS: u64 = 1_000;

/// Milliseconds per refilled mute-work token — 250 ms, i.e. 4 per second
/// sustained.
///
/// Above any human toggle rate and roughly three to four orders of magnitude
/// below what a client can send on an idle QUIC connection.
const MUTE_WORK_REFILL_INTERVAL_MS: u64 = 250;

/// Per-connection token bucket bounding a repeatable client-driven action.
///
/// # Why a rate limit and NOT a budget
///
/// The receive-capability path takes a cumulative budget for ACCEPTED
/// declarations because re-declaring is rare and a client that stops
/// re-declaring loses nothing. The two actions bounded here are the opposite:
/// repeatable steady-state paths that must keep working for the life of the
/// session. A cumulative budget would spend out and then permanently deny —
/// freezing `audio_self_muted` on the roster so every other participant renders
/// a live speaker as muted, or silencing the error a client needs in order to
/// correct its declaration.
///
/// A rate limit bounds work per unit time and never permanently denies. See
/// `media_signaling`'s module doc for the criterion in general form.
///
/// # Two instances, deliberately not one shared bucket
///
/// [`MediaSignalingContext`] holds a separate bucket per bounded path, so a
/// client flooding malformed declarations cannot also suppress its own mute
/// reports (or the reverse). One shared bucket would make each path a denial
/// vector against the other, which is the amplification problem one level down.
#[derive(Debug)]
struct ClientWorkLimiter {
    tokens: u32,
    burst: u32,
    refill_interval_ms: u64,
    refilled_at: Instant,
}

impl ClientWorkLimiter {
    fn new(now: Instant, burst: u32, refill_interval_ms: u64) -> Self {
        Self {
            tokens: burst,
            burst,
            refill_interval_ms,
            refilled_at: now,
        }
    }

    /// Refill, then spend one token. `false` means the caller must do no work.
    ///
    /// Tokens are spent ONLY on work actually about to be done — the caller
    /// short-circuits identical repeats before reaching here. Draining the
    /// bucket on no-op messages would let a client spamming a steady state
    /// suppress its own next genuine toggle, and would report `rate_limited`
    /// while the expensive path was never approached.
    fn try_spend(&mut self, now: Instant) -> bool {
        self.refill(now);
        if self.tokens == 0 {
            return false;
        }
        self.tokens -= 1;
        true
    }

    fn refill(&mut self, now: Instant) {
        let elapsed_ms = now.saturating_duration_since(self.refilled_at).as_millis();
        let earned = elapsed_ms / u128::from(self.refill_interval_ms);
        if earned == 0 {
            return;
        }
        if earned >= u128::from(self.burst) {
            // Long enough idle that the bucket is full however the arithmetic
            // is rounded; reset the clock rather than advance it, which also
            // keeps `Duration` multiplication away from overflow.
            self.tokens = self.burst;
            self.refilled_at = now;
            return;
        }
        // `earned < self.burst` here, so the narrowing cannot lose a token.
        let earned = u32::try_from(earned).unwrap_or(self.burst);
        self.tokens = self.tokens.saturating_add(earned).min(self.burst);
        // Advance by exactly what was granted, so a sub-interval remainder is
        // carried rather than discarded — otherwise a client polling just under
        // the interval would earn nothing forever.
        self.refilled_at += Duration::from_millis(u64::from(earned) * self.refill_interval_ms);
    }
}

/// `message_kind` for the rejection `ErrorMessage` on the capability path.
const SIGNALING_KIND_CAPABILITY_REJECTION: &str = "capability_rejection";

/// `message_kind` for the ADR-0036 §5 send directive.
const SIGNALING_KIND_SEND_DIRECTIVE: &str = "send_directive";

/// `message_kind` for the ADR-0036 §6 slot assignments.
const SIGNALING_KIND_STREAM_ASSIGNMENTS: &str = "stream_assignments";

/// Per-connection state for the ADR-0036 §5/§6 client-facing media signalling.
///
/// Owned by the single-threaded bridge loop, so no lock: every field is read and
/// written from one task.
///
/// # Every rejection is decidable from this struct alone
///
/// [`Self::planned_audio_slot`] and [`Self::handler_urls`] are resolved ONCE, at
/// join, from the same `compute_assignment` output the MC→MH control plane is
/// programmed from. That is what lets the whole receive-capability validation
/// path run without touching the meeting actor — a client looping unservable
/// declarations cannot drive an O(N) roster clone per message through the shared
/// mailbox, and only *accepted* declarations reach `get_state()`, bounded by
/// [`Self::declaration_budget`].
///
/// Caching is sound because the values can only change if the pushed forwarding
/// policy changes, and the only trigger for that in this story is the
/// first-participant join, which precedes every post-join dispatch. The
/// capability-triggered re-push story retires this cache.
///
/// # TWO client-driven paths reach `get_state()`, and BOTH are bounded
///
/// Naming both, because the bound is the security property and a third caller
/// added without one would reinstate the amplification this struct exists to
/// prevent:
///
/// - **capability** — only *accepted* declarations reach it, bounded per
///   connection by [`Self::declaration_budget`]; every rejection is decided from
///   this struct alone.
/// - **client mute** — bounded by [`Self::mute_limiter`], a RATE limit rather
///   than a budget, because mute is a repeatable steady-state user action and a
///   cumulative budget would permanently deny it. Layered on two
///   short-circuits that cost nothing: an identical repeat never reaches the
///   actor, and a video-only change never reaches the recomposition.
///
/// **If you add a third caller of `compose_and_emit`, it needs its own bound** —
/// and see `media_signaling`'s module doc for choosing which KIND of bound.
struct MediaSignalingContext {
    /// Live handle to the meeting actor, taken once at join.
    meeting_handle: MeetingActorHandle,
    /// This connection's participant id, for the self-mute report.
    participant_id: String,
    /// This connection's own sender handle.
    sender_id: crate::media_admission::SenderId,
    /// Server-derived client-facing handler urls. NEVER sourced from a client's
    /// `MediaConnectionUpdate` — see [`HandlerUrls`].
    handler_urls: HandlerUrls,
    /// Handlers assigned to this meeting, for rebuilding the routing input.
    ///
    /// # A per-connection cache used as a MEETING-WIDE fact
    ///
    /// Unlike [`Self::handler_urls`] and [`Self::planned_audio_slot`], which are
    /// this subscriber's own, this list is applied to EVERY participant on the
    /// roster when the routing input is rebuilt. That is sound only because the
    /// handler set is currently a property of the meeting rather than of a
    /// participant: it is read from the meeting's `MhAssignmentData`, and **MC
    /// has no per-participant placement input** — nothing on the roster records
    /// which handler a given participant reached — so every participant is
    /// described with the meeting's full handler set. That is the same premise
    /// `routing_input_for` fills in one place.
    ///
    /// **The premise is about the routing INPUT, not about steering.** Since
    /// story task 25 the client does not pick a handler: MC directs it to the one
    /// `compute_assignment` placed its edges on (ADR-0036 §5). Do not read the
    /// full-list fill as a surviving `connectAll()` assumption — a client's
    /// transport bootstrap set and the handler MC directs it to SEND on are two
    /// different things, and only the second is a routing decision.
    ///
    /// **It stops being sound the moment per-participant handler placement
    /// lands** — participants on different handlers would all be described with
    /// this connection's set, and the composed assignment would name handlers a
    /// source is not actually on. The fix then is to carry membership on the
    /// roster snapshot instead of caching it here; `routing_input_for` is the
    /// single site that would consume it.
    ///
    /// Also stale if a meeting's handler assignment changes mid-session, which
    /// nothing in this story does — the same soundness condition as
    /// [`Self::planned_audio_slot`], retired by the same re-push story.
    handlers: Vec<HandlerId>,
    /// The audio slot MC's assignment routes this subscriber's audio into.
    planned_audio_slot: PlannedAudioSlot,
    /// The stream-number to media-kind/encoding table MC directs from.
    stream_policy: MediaStreamPolicy,
    /// Configured cap on declared slots per declaration.
    max_receive_slots: usize,
    /// Remaining budget of ACCEPTED declarations on this connection.
    declaration_budget: u32,
    /// The last accepted declaration, if any.
    ///
    /// Doubles as the identical-redeclaration short-circuit: an unchanged
    /// declaration is answered without a meeting-state read.
    declaration: Option<ReceiveCapabilityDeclaration>,
    /// The last client-mute pair this connection reported.
    ///
    /// The mute-path counterpart of the identical-redeclaration short-circuit,
    /// and it exists for the same reason: `MuteRequest` is otherwise the
    /// cheapest way to drive an actor round trip plus an O(N) roster clone plus
    /// an assignment computation, from a ~4-byte message, unbounded — because
    /// the declaration budget charges only on the capability path.
    ///
    /// SOUND because `audio_self_muted`/`video_self_muted` have exactly ONE
    /// writer in the tree (`MeetingActor::handle_self_mute`); `handle_server_mute`
    /// writes only the `*_server_muted` fields. So a connection-local cache of
    /// what this client last *reported* cannot drift from actor state.
    ///
    /// Behaviour-preserving: a genuine mute→unmute→mute cycle changes the pair
    /// every time and passes straight through, so R-2's instantaneous unmute is
    /// untouched. Only true no-ops are dropped. `None` at join, so the first
    /// report always passes.
    last_reported_mute: Option<(bool, bool)>,
    /// Bounds client-driven mute work.
    ///
    /// Mute is the only repeatable client-driven path that puts work on the
    /// SHARED meeting actor's mailbox — one `GetState` roster snapshot per
    /// composition. The O(N) awaited `broadcast_update` this bound was
    /// originally sized against was removed by S-2 (`MuteChanged` had no
    /// consumer), so the meeting-wide blast radius is now a queue slot rather
    /// than head-of-line blocking, and the dominant remaining cost is the
    /// per-connection composition. The bound is still required — it is still
    /// unbounded client-driven work — but it is no longer a meeting-wide
    /// contention control, and `rate_limited` must not be read as one.
    mute_limiter: ClientWorkLimiter,

    /// Bounds the rejection RESPONSE, not the counting (ADR-0036 §6).
    ///
    /// A rejected declaration is ~12 bytes inbound and provokes a ~120-byte
    /// `ErrorMessage` out (the static text plus a 55-byte W3C `traceparent`),
    /// through a `ServerMessage` clone, a context injection, an encode, an
    /// awaited participant-mailbox hop and a QUIC stream write — roughly 10x
    /// egress amplification at 1:1 with inbound, and it shares the 200-slot
    /// participant mailbox with roster broadcasts, so a flooding client can
    /// drop its OWN `ParticipantJoined`/`Left` updates.
    ///
    /// The counter and the one-shot WARN are deliberately OUTSIDE this bound:
    /// rejections stay fully observable however fast they arrive, and only the
    /// reply is rationed. Rejections are also deliberately not budget-charged —
    /// a budget here would permanently deny, which is the same objection that
    /// ruled out a cumulative budget for mute.
    ///
    /// NOT one-shot per connection: a legitimate client's second rejection is
    /// usually a genuinely different one it needs to see in order to converge.
    rejection_reply_limiter: ClientWorkLimiter,
    /// Whether the mute rate limit has already been logged on this connection.
    ///
    /// Same one-shot discipline as [`Self::rejection_logged`], and for the same
    /// reason: the condition is client-driven, so a line per message is a
    /// log-amplification vector and the counter is the complete record.
    mute_rate_limit_logged: bool,
    /// Whether a capability rejection has already been logged on this
    /// connection.
    ///
    /// Rejections are ALWAYS counted; the WARN fires once. A per-message log on
    /// a client-driven path is a log-amplification vector, and the second
    /// identical line tells an operator nothing the counter does not.
    rejection_logged: bool,

    /// One-shot bound on the rejection-reply-suppressed WARN.
    rejection_reply_suppressed_logged: bool,
}

impl MediaSignalingContext {
    /// Consume one unit of declaration budget if any remains.
    ///
    /// Kept behind one function taking the declaration context and returning a
    /// decision, deliberately NOT inlined into the dispatch arm: a token-bucket
    /// rate limit — the better model once a real user re-lays-out a grid over a
    /// long session — must be able to replace this without touching the call
    /// site. That substitutability is what makes the cap-over-bucket choice
    /// cheap to revisit rather than a claim nobody can act on.
    fn budget_remaining(&self) -> bool {
        self.declaration_budget > 0
    }

    fn charge_budget(&mut self) {
        self.declaration_budget = self.declaration_budget.saturating_sub(1);
    }
}

/// Resolve the audio slot MC's forwarding assignment routes `subscriber` into.
///
/// Read out of the assignment output rather than reconstructed from
/// `MAIN_AUDIO_SLOT_ID`, so MC keeps exactly one answer to "which slot do I route
/// into" — the assignment computation is the sole producer, here as on the MH
/// control plane.
fn planned_audio_slot_for(
    assignment: &MeetingAssignment,
    subscriber: crate::media_admission::SenderId,
) -> Option<SlotId> {
    assignment
        .per_handler
        .values()
        .flat_map(|h| h.egress_streams.iter())
        .find(|plan| plan.subscriber == subscriber)
        .map(|plan| SlotId::from_u16(plan.slot_id))
}

/// Handle a post-join `ReceiveCapability` (ADR-0036 §6).
///
/// Validate, then compose and emit the send directive and the slot assignments
/// from ONE meeting-state snapshot — which is what keeps the directive's target
/// set and the assignments' sources from disagreeing, and costs one actor round
/// trip rather than two.
///
/// Every rejection short-circuits before the snapshot is read.
#[instrument(
    target = "mc.webtransport.connection",
    name = "mc.receive_capability",
    skip_all,
    fields(connection_id = %connection_id, slot_count = capability.slots.len())
)]
async fn handle_receive_capability(
    capability: &v1::ReceiveCapability,
    trace_parent: &str,
    trace_state: &str,
    connection_id: &str,
    participant_handle: &ParticipantActorHandle,
    media: &mut MediaSignalingContext,
) {
    reparent_current_span(trace_parent, trace_state);

    let declaration = match ReceiveCapabilityDeclaration::parse(
        capability,
        media.max_receive_slots,
        media.budget_remaining(),
        media.planned_audio_slot,
    ) {
        Ok(declaration) => declaration,
        Err(outcome) => {
            reject_capability(outcome, connection_id, participant_handle, media).await;
            return;
        }
    };

    // Identical re-declaration: MC does NO work and sends NOTHING — it returns
    // before `compose_and_emit`, before the meeting-state read, and charges no
    // budget. Recorded under its own outcome rather than as `Accepted`, because
    // `accepted` means "declarations MC acted on" and is the denominator for any
    // rejection ratio; this arm is client-inflatable at near-zero server cost
    // and must never inflate that denominator.
    //
    // No log line: this is a client-driven path, so one line per message is a
    // log-amplification vector, and the counter is the complete record. Same
    // reasoning as the one-shot rejection WARN.
    if media.declaration.as_ref() == Some(&declaration) {
        metrics::record_receive_capability(CapabilityOutcome::AcceptedUnchanged);
        return;
    }

    media.charge_budget();
    metrics::record_receive_capability(CapabilityOutcome::Accepted);

    // ARM the identical-redeclaration short-circuit only if the composition
    // actually delivered. Same ordering discipline as `last_reported_mute`
    // below, and for a sharper reason.
    //
    // Arming it first and then failing to compose leaves the connection
    // PERMANENTLY DARK: the client's natural recovery is to re-send the same
    // declaration, that now matches the stored value, and it is answered
    // `accepted_unchanged` — no directive, no assignments, no error, and the
    // rejection WARN is one-shot so nothing is logged either. Only a full
    // reconnect escapes. Restoring the previous declaration on failure makes the
    // retry work.
    //
    // The budget is deliberately NOT refunded. Charging on every accepted
    // declaration — including ones whose composition failed — is what keeps
    // retries bounded, so this stays a retryable path rather than an unbounded
    // one.
    let previous = media.declaration.replace(declaration);
    if !compose_and_emit(connection_id, participant_handle, media, true).await {
        media.declaration = previous;
    }
}

/// Count, bound-log, and answer a rejected declaration.
///
/// The counter always fires; the WARN fires once per connection. The rejection
/// token is logged; the offending `slot_id` / `pinned_sender_id` value never is —
/// those are stream identities (ADR-0036 §11) and a per-message log carrying
/// client-chosen values is a log-amplification vector besides.
async fn reject_capability(
    outcome: CapabilityOutcome,
    connection_id: &str,
    participant_handle: &ParticipantActorHandle,
    media: &mut MediaSignalingContext,
) {
    metrics::record_receive_capability(outcome);

    if !media.rejection_logged {
        media.rejection_logged = true;
        if outcome == CapabilityOutcome::SlotIdNotPlanned {
            // Not a client defect. The declaration is well-formed and the wire
            // contract permits it: MH stamps the relay-region stream id from the
            // policy pushed at first-participant join, before this client could
            // declare, so MC cannot address a slot it did not plan. Named
            // explicitly because the bare token reads as "bad client" and sends
            // an operator into the SDK.
            warn!(
                target: "mc.webtransport.connection",
                connection_id = %connection_id,
                outcome = outcome.label(),
                "Rejecting receive capability: the declared audio slot is not the one MC's \
                 forwarding assignment routes this subscriber into. This is an MC limitation, \
                 not a client defect — the join-time policy push fixes the egress slot id \
                 before a client can declare. Further rejections on this connection are \
                 counted, not logged."
            );
        } else {
            warn!(
                target: "mc.webtransport.connection",
                connection_id = %connection_id,
                outcome = outcome.label(),
                "Rejecting receive capability declaration. Further rejections on this \
                 connection are counted, not logged."
            );
        }
    }

    // Bound the REPLY only. The counter above already fired and the WARN has
    // already had its one-shot chance, so a flooding client stays fully
    // observable; what it stops buying is ~10x egress amplification and
    // contention for its own participant mailbox. Spent here — after the
    // counter, before the send — so a suppressed rejection is counted, not
    // silent.
    if !media.rejection_reply_limiter.try_spend(Instant::now()) {
        if !media.rejection_reply_suppressed_logged {
            media.rejection_reply_suppressed_logged = true;
            warn!(
                target: "mc.webtransport.connection",
                connection_id = %connection_id,
                "Capability rejection replies on this connection exceeded the sustained rate \
                 limit and are being suppressed. Rejections are still counted on \
                 mc_media_receive_capability_declarations_total. A converging client never \
                 reaches this bound, so the likely cause is a client-side repeat loop."
            );
        }
        return;
    }

    send_signaling(
        participant_handle,
        connection_id,
        SIGNALING_KIND_CAPABILITY_REJECTION,
        &ServerMessage {
            message: Some(server_message::Message::Error(ErrorMessage {
                code: v1::ErrorCode::InvalidRequest as i32,
                // A bounded `&'static str`. No client-supplied value is echoed
                // back on any path, so this cannot become a reflection vector.
                message: outcome.client_message().to_string(),
                details: Default::default(),
            })),
            trace_parent: String::new(),
            trace_state: String::new(),
        },
    )
    .await;
}

/// Handle a post-join `MuteRequest` (ADR-0036 §5 client mute).
///
/// Records the reported state on the meeting actor (the single home for mute)
/// and re-conveys slot state so a subscriber sees `SLOT_STATE_SOURCE_MUTED`.
///
/// # Three guards, in this order, and the order is the security property
///
/// 1. **Identical repeat** — returns before the actor hop and before spending a
///    token, so a client stuck on one state costs a tuple compare.
/// 2. **Rate limit** ([`ClientWorkLimiter`]) — spent only on work about to be
///    done. This is the bound on the only repeatable client-driven path that
///    puts work on the SHARED meeting actor's mailbox (one `GetState` per
///    composition). It is NOT a bound on an O(N) fan-out any more: S-2 removed
///    `handle_self_mute`'s `broadcast_update`, so the cost is now dominated by
///    the per-connection composition.
/// 3. **Audio-only recomposition** — a video-only change is reported to the
///    actor but never recomposed, because the slot view is derived from
///    `audio_self_muted` alone.
///
/// # The send directive is not touched, and cannot be
///
/// This function calls [`compose_and_emit`] with `emit_directive = false`, but
/// that flag is belt and braces: `media_signaling::directive` has no import
/// through which mute state could arrive and `build_send_directive` takes no
/// mute argument, so a mute/unmute cycle altering the directive is a compile
/// error rather than a behaviour to be careful about. That is what keeps
/// *"MC has not asked you to send"* and *"you have muted yourself"* distinct,
/// and what makes unmute a purely local decision with no round trip.
async fn handle_mute_request(
    request: &v1::MuteRequest,
    connection_id: &str,
    participant_handle: &ParticipantActorHandle,
    media: &mut MediaSignalingContext,
) {
    // Short-circuit a no-op BEFORE the actor hop, before the limiter, and
    // before `compose_and_emit`.
    //
    // `MeetingActor::handle_self_mute` is already idempotent, which correctly
    // suppresses a broadcast of a transition that did not occur — but
    // `UpdateSelfMute` has no `respond_to`, so the caller cannot learn that
    // nothing changed and would proceed to a full roster read regardless. The
    // idempotence guard fixes the wire lie; this fixes the cost.
    let reported = (request.audio_muted, request.video_muted);
    if media.last_reported_mute == Some(reported) {
        metrics::record_mute_request(MuteOutcome::Unchanged);
        return;
    }

    // Spend a token only now — AFTER the no-op short-circuit, so tokens are
    // spent on work actually about to be done. Draining the bucket on identical
    // repeats would let a client spamming a steady state suppress its own next
    // GENUINE toggle, and would report `rate_limited` for a path that was never
    // approached.
    if !media.mute_limiter.try_spend(Instant::now()) {
        metrics::record_mute_request(MuteOutcome::RateLimited);

        // The report is DROPPED, not queued. That is safe because ADR-0036 §5
        // enforces client mute at CAPTURE on the client: a suppressed report
        // means the audio genuinely stopped and only the indicator other
        // participants see is stale. There is no window in which someone
        // believes they have stopped transmitting and has not.
        //
        // This holds for a merely BROKEN client (a repeating button, a reactive
        // loop) as much as a hostile one, which is why it is the stated reason
        // rather than "the flag is client-controlled anyway".
        //
        // `last_reported_mute` is deliberately NOT updated, so the client's next
        // report re-attempts instead of being swallowed by the no-op
        // short-circuit above. The staleness is therefore bounded by that next
        // toggle rather than lasting the session.
        if !media.mute_rate_limit_logged {
            media.mute_rate_limit_logged = true;
            warn!(
                target: "mc.webtransport.connection",
                connection_id = %connection_id,
                "Mute reports on this connection exceeded the sustained rate limit and are being \
                 dropped. A human never reaches this bound, so the likely cause is a client-side \
                 repeat loop. Further occurrences on this connection are counted, not logged."
            );
        }
        return;
    }

    // Captured BEFORE the cache is overwritten. `None` — the first report after
    // join — stands for `(false, false)`, which is the actual post-join actor
    // state, so a first report of "not muted" correctly counts as no change.
    let previously_audio_muted = media.last_reported_mute.is_some_and(|(audio, _)| audio);

    if let Err(e) = media
        .meeting_handle
        .update_self_mute(
            media.participant_id.clone(),
            request.audio_muted,
            request.video_muted,
        )
        .await
    {
        debug!(
            target: "mc.webtransport.connection",
            connection_id = %connection_id,
            error = %e,
            "Failed to record client mute on the meeting actor"
        );
        metrics::record_mute_request(MuteOutcome::ActorUnavailable);
        return;
    }

    // Cache only AFTER the actor accepted the report. Assigning before the await
    // would record a report the actor never received, and an identical retry
    // would then be short-circuited forever. The consequence is bounded — the
    // send fails only when the meeting actor is gone, so the connection is
    // already terminal, and ADR-0036 §5 enforces client mute at capture on the
    // client regardless of what MC recorded — but the early return above makes
    // ordering it correctly free.
    media.last_reported_mute = Some(reported);

    // RECOMPOSE ONLY IF THE AUDIO FLAG MOVED.
    //
    // `last_reported_mute` is the right dedupe key for "should I report this to
    // the meeting actor" — the actor stores both flags — and the WRONG key for
    // "should I recompose". `SourceMuteView` and `build_stream_assignments` read
    // `audio_self_muted` alone, so a video-only toggle would spend an O(N)
    // roster read, an assignment computation and an outbound message to produce
    // a `StreamAssignments` byte-identical to the last one, and would move
    // `mc_media_slot_states_total` with provably zero information delivered.
    //
    // Two consumers reading different fields of one pair is two questions; they
    // now have two answers. This is not an attack mitigation — it fires the
    // first time a client wires a camera button, with nobody hostile involved.
    if media.declaration.is_none() || previously_audio_muted == request.audio_muted {
        metrics::record_mute_request(MuteOutcome::AppliedNoRecompose);
        return;
    }

    metrics::record_mute_request(MuteOutcome::Applied);

    // Return value deliberately discarded: it reports whether the DIRECTIVE was
    // delivered, and this path emits no directive. The declaration must stay
    // armed regardless — a failed mute re-convey is self-healing on the next
    // toggle, and disarming here would turn a transient actor hiccup into a
    // client that gets re-sent a directive it already has.
    let _ = compose_and_emit(connection_id, participant_handle, media, false).await;
}

/// Compose the client-facing media messages from one meeting-state snapshot.
///
/// `emit_directive` selects §5 plus §6 (a fresh capability declaration) versus
/// §6 alone (a mute report). The directive is composed from the assignment and
/// the configured encoding only; the mute view feeds the assignments alone.
///
/// # Return value is load-bearing
///
/// `true` iff MC delivered everything this call was supposed to deliver. The
/// caller on the capability path uses it to decide whether to ARM the
/// identical-redeclaration short-circuit: arming it after a failed composition
/// would make the client's natural recovery — re-sending the same declaration —
/// answer `accepted_unchanged` forever, leaving the connection silently dark and
/// recoverable only by a full reconnect.
///
/// A directive failure returns `false` even though the assignments were still
/// emitted: assignments without a directive still means the client is never told
/// to send, which is the state the retry needs to be able to escape.
async fn compose_and_emit(
    connection_id: &str,
    participant_handle: &ParticipantActorHandle,
    media: &mut MediaSignalingContext,
    emit_directive: bool,
) -> bool {
    let Some(declaration) = media.declaration.clone() else {
        return false;
    };

    // ONE snapshot for both messages.
    let state = match media.meeting_handle.get_state().await {
        Ok(state) => state,
        Err(e) => {
            if emit_directive {
                metrics::record_send_directive(DirectiveOutcome::MeetingStateUnavailable);
            }
            warn!(
                target: "mc.webtransport.connection",
                connection_id = %connection_id,
                error = %e,
                "Meeting state unavailable; not composing media signalling"
            );
            return false;
        }
    };

    // Through the SINGLE routing-input constructor, not a second inline one:
    // `compute_assignment` is the one producer of "which slot MC routes into",
    // and that only holds if its input has one encoding too.
    let routing_input = match routing_input_for(
        state.participants.iter().map(|p| p.sender_id),
        media.handlers.clone(),
    ) {
        Ok(routing_input) => routing_input,
        Err(e) => {
            if emit_directive {
                metrics::record_send_directive(DirectiveOutcome::AssignmentFailed);
            }
            error!(
                target: "mc.webtransport.connection",
                connection_id = %connection_id,
                error = %e,
                "Routing input could not be built; no media signalling emitted"
            );
            return false;
        }
    };

    let assignment = match compute_assignment(&routing_input) {
        Ok(assignment) => assignment,
        Err(e) => {
            if emit_directive {
                metrics::record_send_directive(DirectiveOutcome::AssignmentFailed);
            }
            error!(
                target: "mc.webtransport.connection",
                connection_id = %connection_id,
                reason = e.label(),
                "Forwarding assignment could not be computed; no media signalling emitted"
            );
            return false;
        }
    };

    let mut directive_emitted = true;

    if emit_directive {
        match build_send_directive(
            media.sender_id,
            &assignment,
            &media.handler_urls,
            &media.stream_policy,
        ) {
            Ok((directive, outcome)) => {
                metrics::record_send_directive(outcome);
                send_signaling(
                    participant_handle,
                    connection_id,
                    SIGNALING_KIND_SEND_DIRECTIVE,
                    &ServerMessage {
                        message: Some(server_message::Message::SendDirective(directive)),
                        trace_parent: String::new(),
                        trace_state: String::new(),
                    },
                )
                .await;
            }
            Err(outcome) => {
                metrics::record_send_directive(outcome);
                directive_emitted = false;
                error!(
                    target: "mc.webtransport.connection",
                    connection_id = %connection_id,
                    outcome = outcome.label(),
                    "Send directive could not be composed; client not directed to send"
                );
            }
        }
    }

    let mute = SourceMuteView::from_pairs(
        state
            .participants
            .iter()
            .map(|p| (p.sender_id, p.audio_self_muted)),
    );

    let composition = build_stream_assignments(
        media.sender_id,
        &declaration,
        &assignment,
        &media.handler_urls,
        &mute,
    );

    for state in &composition.slot_states {
        metrics::record_slot_state(*state);
    }
    metrics::record_unmatched_plan_slots(composition.unmatched_plan_slots);

    send_signaling(
        participant_handle,
        connection_id,
        SIGNALING_KIND_STREAM_ASSIGNMENTS,
        &ServerMessage {
            message: Some(server_message::Message::StreamAssignments(
                composition.assignments,
            )),
            trace_parent: String::new(),
            trace_state: String::new(),
        },
    )
    .await;

    directive_emitted
}

/// Send a `ServerMessage` to the client through the participant actor.
///
/// Routed through the actor's outbound channel rather than written straight to
/// the send stream, so ordering with roster broadcasts stays FIFO and there is
/// one write choke point. A full mailbox is counted at that choke point.
///
/// `message_kind` is a bounded `&'static str` naming what failed to arrive, so
/// the delivery WARN is triageable without carrying any client-supplied value.
async fn send_signaling(
    participant_handle: &ParticipantActorHandle,
    connection_id: &str,
    message_kind: &'static str,
    message: &ServerMessage,
) {
    // R-57: carry the current server-side trace context (bounded W3C ids only).
    let (trace_parent, trace_state) = inject_current_context();
    let mut message = message.clone();
    message.trace_parent = trace_parent;
    message.trace_state = trace_state;

    if let Err(e) = participant_handle
        .send(SignalingPayload::Raw {
            message_type: 0,
            data: message.encode_to_vec(),
        })
        .await
    {
        // WARN, not DEBUG. `ParticipantActorHandle::send` is an awaited mpsc
        // send, so an `Err` means the mailbox is CLOSED — the participant actor
        // is gone. That is the one hop between "MC decided to direct this client
        // to send" and "the client was told", and `record_send_directive` has
        // ALREADY fired `emitted` by the time we get here.
        //
        // At the default `RUST_LOG=info` a DEBUG line is invisible, so a client
        // that never publishes would present as `emitted` incrementing normally,
        // `accepted` incrementing normally, and nothing anywhere disagreeing.
        // This is the only evidence that contradicts the counter, so it must be
        // visible at the level operators actually run.
        //
        // Deliberately NOT a delivery-outcome counter: the mailbox-FULL case is
        // already counted downstream at the actor's `try_send` choke point by
        // `mc_participant_outbound_messages_dropped_total{payload_kind}`. The
        // uncovered case is actor-gone, and a WARN is proportionate to it.
        warn!(
            target: "mc.webtransport.connection",
            connection_id = %connection_id,
            error = %e,
            // `message_kind`, NOT `payload_kind`. The two name DISJOINT value
            // domains across this exact hop: here the domain is
            // {capability_rejection, send_directive, stream_assignments}, while
            // the `payload_kind` label on
            // `mc_participant_outbound_messages_dropped_total` is
            // {signaling_raw, participant_update} — every `ServerMessage`
            // leaves as `SignalingPayload::Raw`, so `send_directive` here
            // becomes `signaling_raw` there. The catalog and the dashboard both
            // tell an operator to correlate this WARN with that counter; naming
            // both fields `payload_kind` would invite a pivot that silently
            // joins on nothing.
            message_kind = message_kind,
            "Failed to deliver media signalling to participant actor; the client was NOT told \
             what the counter says it was told"
        );
    }
}

/// Resolve the per-connection media-signalling context, or the
/// [`DirectiveOutcome`] naming the stage that failed.
///
/// An `Err` is a **degraded but healthy** connection: signalling for everything
/// else keeps working and the client is simply never directed to send. It is
/// logged and COUNTED once at the call site, never per client message.
///
/// # Why the error type is `DirectiveOutcome` and not a local enum
///
/// This is the most severe way a client can end up never told to send — it
/// silences the connection for its whole life, not for one declaration — so it
/// must not be the one such failure that moves no counter. Reporting it on
/// `mc_media_send_directives_total` keeps that counter's central claim true:
/// *every* reason MC did not COMPOSE a directive lands on one series, so the 3am
/// question "did MC compose a directive, and if not why" is one query.
///
/// It is NOT every reason a client was not told to send. `emitted` fires before
/// `send_signaling`, so the two delivery hops after it are covered elsewhere:
/// mailbox-FULL by
/// `mc_participant_outbound_messages_dropped_total{payload_kind="signaling_raw"}`
/// and mailbox-CLOSED by the delivery WARN. See this file's `send_signaling`,
/// which states the same split from the other side.
///
/// The stages below are the SAME sequential stages `compose_and_emit` runs, so
/// the vocabulary's disjoint-by-construction property is preserved rather than
/// stretched: the first failing stage returns, and later stages are unreachable.
async fn build_media_signaling_context(
    controller_handle: &MeetingControllerActorHandle,
    join_result: &JoinResult,
    mh_data: &MhAssignmentData,
    meeting_id: &str,
    config: crate::media_signaling::ClientMediaConfig,
) -> Result<MediaSignalingContext, DirectiveOutcome> {
    // Each stage keeps its underlying error at DEBUG: the outcome token on the
    // call-site WARN says WHICH stage failed, and this says WHY. Join-time only
    // — once per connection, never client-drivable — so there is no
    // log-amplification concern here.
    let meeting_handle = controller_handle
        .get_meeting_handle(meeting_id.to_string())
        .await
        .map_err(|e| {
            debug!(
                target: "mc.webtransport.connection",
                meeting_id = %meeting_id,
                error = %e,
                "Media signalling context: meeting handle unavailable"
            );
            DirectiveOutcome::MeetingStateUnavailable
        })?;

    let routing_input = build_routing_input(join_result, mh_data).map_err(|e| {
        debug!(
            target: "mc.webtransport.connection",
            meeting_id = %meeting_id,
            error = %e,
            "Media signalling context: routing input could not be built"
        );
        DirectiveOutcome::AssignmentFailed
    })?;

    let assignment = compute_assignment(&routing_input).map_err(|e| {
        debug!(
            target: "mc.webtransport.connection",
            meeting_id = %meeting_id,
            reason = e.label(),
            "Media signalling context: forwarding assignment could not be computed"
        );
        DirectiveOutcome::AssignmentFailed
    })?;

    // NOT `AssignmentFailed`: the assignment computed successfully and simply
    // contains no egress plan naming this subscriber. "MC could not compute a
    // plan" and "MC computed a plan that does not include you" have different
    // causes and different remedies, so they get different tokens.
    let planned_slot = planned_audio_slot_for(&assignment, join_result.sender_id)
        .ok_or(DirectiveOutcome::NoPlannedEgressSlot)?;

    // Server-derived urls only. The similar-looking client-supplied map is
    // `ParticipantActor::mh_statuses`; it must never be a source here.
    let handler_urls = HandlerUrls::from_pairs(
        mh_data
            .handlers
            .iter()
            .map(|h| (HandlerId::new(&h.mh_id), h.webtransport_endpoint.clone())),
    );

    // One clock reading for both buckets: they start together at join, so a
    // skew between them would be meaningless state.
    let limiter_start = Instant::now();

    Ok(MediaSignalingContext {
        meeting_handle,
        participant_id: join_result.participant_id.clone(),
        sender_id: join_result.sender_id,
        handler_urls,
        handlers: routing_input.handlers,
        planned_audio_slot: PlannedAudioSlot::new(planned_slot),
        stream_policy: MediaStreamPolicy::new(config.audio_encoding),
        max_receive_slots: config.max_receive_slots,
        declaration_budget: config.max_receive_capability_declarations,
        declaration: None,
        last_reported_mute: None,
        mute_limiter: ClientWorkLimiter::new(
            limiter_start,
            MUTE_WORK_BURST,
            MUTE_WORK_REFILL_INTERVAL_MS,
        ),
        rejection_reply_limiter: ClientWorkLimiter::new(
            limiter_start,
            CAPABILITY_REJECTION_BURST,
            CAPABILITY_REJECTION_REFILL_INTERVAL_MS,
        ),
        mute_rate_limit_logged: false,
        rejection_logged: false,
        rejection_reply_suppressed_logged: false,
    })
}

/// Post-join dispatch for a connection with no media-signalling context.
///
/// Identical to [`handle_client_message`] minus the two media arms. Split rather
/// than branching inside each arm so the media handlers cannot be reached at all
/// without a context, and so this path stays silent per message.
async fn handle_client_message_without_media(
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
        _ => {
            debug!(
                target: "mc.webtransport.connection",
                connection_id = %connection_id,
                "Post-join client message ignored (no media signalling context)"
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
            // ADR-0036 §4: the roster carries the sender id and the identity
            // signing public key, and NO other key material — no meeting KEK,
            // no wrapped transmit key, no key id, no thumbprint.
            //
            // `sender_id` is `Some` of a `NonZeroU16`, so it can never be
            // `Some(0)`: 0 is reserved-invalid, and N participants sharing it
            // would collide on one roster identity and — via key id, derived
            // wrap nonce — on one AES-GCM nonce.
            //
            // `identity_public_key` is either exactly 32 bytes or EMPTY. Never
            // any other length: MC refuses the join otherwise.
            //
            // This is the chain a receiver walks: key id -> sender_id -> this
            // roster entry -> identity_public_key, for Ed25519 verification.
            // Trust on first use: same-keyholder consistency, never a verified
            // identity.
            sender_id: Some(u32::from(p.sender_id.get().get())),
            // `None` MUST serialize to EMPTY BYTES, never to a zero-filled
            // 32-byte array. An explicit `match` rather than
            // `unwrap_or_default()` because the failure mode is silent and
            // strictly worse than empty: a present-looking all-zero key makes a
            // consumer take the VERIFY path and never see the "no key
            // published" signal the contract defines. It would fail closed
            // (all-zeros is not a valid Ed25519 point) but it destroys the
            // distinction accept-absent rests on. Do not "simplify" this.
            identity_public_key: match &p.identity_public_key {
                Some(key) => key.as_bytes().to_vec(),
                None => Vec::new(),
            },
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

    // Connection BOOTSTRAP data, and nothing else: the handlers this meeting is
    // registered on, for the client to open transports to.
    //
    // # NON-AUTHORITATIVE. Selection is the send directive.
    //
    // ADR-0036 §5 is that MC directs where a client sends. That instruction is
    // `SendTarget.media_handler_url` on the `SendDirective`, matching the active
    // slot's `StreamAssignment.media_handler_url` — both read out of the same
    // `MeetingAssignment` the MC→MH `RegisterMeeting` push is programmed from, so
    // steering and placement cannot diverge. This list is not that instruction
    // and must never be used as one.
    //
    // # The order is DELIBERATELY not sorted — a refusal, not an omission
    //
    // `edge_handler` places every edge on the lexicographically smallest shared
    // `mh_id` (`media_routing/assignment.rs`). So ANY ordering of this list keyed
    // on `mh_id` would correlate with the placement rule: ascending makes
    // `.first()` accidentally correct, and descending merely relocates the same
    // defect to `.last()`. The only order that is independent of placement is one
    // keyed on something unrelated to it — which the Redis enumeration order
    // already is. Leaving it alone is what keeps the two mechanisms independent
    // and makes selecting off this list FAIL LOUDLY rather than pass for the
    // wrong reason. That loudness is not hypothetical: it is how the defect this
    // comment exists because of was found (three of four media connections on
    // mh-1 while every edge sat on mh-0).
    //
    // # Sorting would silently disarm a live test, and this comment is its only guard
    //
    // `crates/mc-service/tests/media_client_signaling_integration.rs`'s
    // `steering_follows_edge_placement_not_redis_order` seeds the handlers as
    // `[mh-1, mh-0]` PRECISELY so that list-order selection yields mh-1 while the
    // assignment places on mh-0. Sorting here collapses those two answers into
    // one: the test keeps passing while no longer able to fail for the reason it
    // exists, and nothing else in the tree notices. There is no type, guard or
    // test that reds when someone sorts this list — this paragraph is the control.
    //
    // # The honest cost
    //
    // Two env-test sites (`crates/env-tests/tests/26_mh_quic.rs`) legitimately
    // take `.first()` because they want "any handler this meeting is registered
    // on", and they therefore get an arbitrary one. Both handlers answer their
    // question identically (MC pushes `RegisterMeeting` to EVERY assigned
    // handler), so that is tolerable — and it rotates them across pods over runs,
    // which surfaces per-pod drift that a sort pinning every test to one handler
    // would hide.
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
            existing_participants,
            media_servers,
            // ADR-0036 §2/§4. `sender_id` is `NonZeroU16` widened into the
            // proto's `uint32` — `u32::from`, never `as`, and never 0.
            sender_id: Some(u32::from(result.sender_id.get().get())),
            // The joiner receives the meeting KEK. This is the ENTIRE
            // key-distribution step (§4 step 3): it costs nothing beyond the
            // join MC already performed and touches no existing member.
            //
            // This is the one legitimate egress of the KEK — to a client MC has
            // just admitted, over its own authenticated session. It goes
            // nowhere else: not to MH, not into any `dark_tower.internal.v1`
            // message, not to disk, not to a log, metric, span or error
            // payload. `JoinResponse`'s derived `Debug` is suppressed in
            // `crates/proto-gen/build.rs` and the hand-written impl renders this
            // field as a length only.
            meeting_kek: result.meeting_kek.expose().to_vec(),
            // u16 semantics widened into the proto's uint32. Always 0 in this
            // story — rotation is deferred. Note 0 is a LEGAL first generation,
            // not a sentinel: the not-provisioned signal is `meeting_kek` not
            // being exactly 32 bytes, so never gate on `kek_generation != 0`.
            kek_generation: u32::from(result.kek_generation),
            correlation_id: result.correlation_id.clone(),
            binding_token: result.binding_token.clone(),
        },
        mh_data,
    ))
}

/// Build the routing computation's input from the join result and the
/// meeting's handler assignment.
///
/// Participant->handler membership is an **input** to the computation, and this
/// is where it is filled. Every participant gets the meeting's full handler list
/// — **because MC has no per-participant placement input**, not because a client
/// chose to connect to all of them: `MhAssignmentData` is a property of the
/// meeting and nothing on the roster records which handler a participant
/// reached. When real per-participant placement lands, only this function
/// changes — [`compute_assignment`] does not.
///
/// This is the routing INPUT and says nothing about steering. MC directs each
/// client to the single handler the assignment places its edges on (ADR-0036
/// §5); see [`RoutingParticipant::handlers`] for that contract.
///
/// The joiner is included alongside the existing roster, so at N=1 the input is
/// a single participant and the general computation yields the loopback edge.
///
/// # Errors
///
/// [`McError::MhAssignmentMissing`] if the meeting has no assigned handlers —
/// there is nothing to program, and silently pushing to zero handlers would
/// make an unroutable meeting indistinguishable from a healthy one.
fn build_routing_input(
    join_result: &JoinResult,
    mh_data: &MhAssignmentData,
) -> Result<MeetingRoutingInput, McError> {
    let handlers: Vec<HandlerId> = mh_data
        .handlers
        .iter()
        .map(|h| HandlerId::new(&h.mh_id))
        .collect();

    // The joiner is not yet on the roster the join returned, so it is chained
    // on rather than assumed present.
    routing_input_for(
        std::iter::once(join_result.sender_id)
            .chain(join_result.participants.iter().map(|p| p.sender_id)),
        handlers,
    )
}

/// Build a [`MeetingRoutingInput`] from a set of sender ids and the meeting's
/// handler list.
///
/// # The SINGLE constructor, and why that is load-bearing
///
/// The whole design rests on [`compute_assignment`] being the one producer of
/// "which slot MC routes into". That guarantee is only as good as its INPUT
/// having one encoding too: two constructors 500 lines apart in this file could
/// each gain a field, a filter, or a guard the other did not, and the two sides
/// would then disagree about the meeting while both looking correct. So both
/// call sites — the join-time context/push path and the per-composition path —
/// come through here.
///
/// Participant->handler membership is an **input** to the computation, and this
/// is where it is filled. Every participant gets the meeting's full handler list
/// — **because MC has no per-participant placement input**, not because a client
/// chose to connect to all of them: `MhAssignmentData` is a property of the
/// meeting and nothing on the roster records which handler a participant
/// reached. When real per-participant placement lands, only this function
/// changes — [`compute_assignment`] does not, and neither does either caller.
///
/// This is the routing INPUT and says nothing about steering. MC directs each
/// client to the single handler the assignment places its edges on (ADR-0036
/// §5); see [`RoutingParticipant::handlers`] for that contract.
///
/// # Errors
///
/// [`McError::MhAssignmentMissing`] if the meeting has no assigned handlers —
/// there is nothing to program, and silently pushing to zero handlers would
/// make an unroutable meeting indistinguishable from a healthy one. The check
/// lives HERE rather than at one call site precisely so it cannot hold at one
/// and not the other.
fn routing_input_for(
    senders: impl IntoIterator<Item = crate::media_admission::SenderId>,
    handlers: Vec<HandlerId>,
) -> Result<MeetingRoutingInput, McError> {
    if handlers.is_empty() {
        return Err(McError::MhAssignmentMissing(
            "no handlers assigned; cannot compute a forwarding assignment".to_string(),
        ));
    }

    let participants = senders
        .into_iter()
        .map(|sender_id| RoutingParticipant {
            sender_id,
            handlers: handlers.clone(),
        })
        .collect();

    Ok(MeetingRoutingInput {
        participants,
        handlers,
    })
}

/// Program each assigned MH with its slice of the meeting's forwarding
/// assignment, and confirm each push (R-12, ADR-0036 §8).
///
/// Called as a spawned task after the first participant joins. For each
/// handler: take a generation, push, classify the reply.
///
/// This function handles all errors internally (log + continue) since it runs
/// as a fire-and-forget spawned task with no caller to propagate errors to.
///
/// # The generation is taken ONCE per handler, outside the retry loop
///
/// Deliberately not inside `MhClient::register_meeting`, where it would be
/// recomputed on every attempt. ADR-0036 §8 requires an unchanged policy to
/// carry an unchanged number; a per-attempt recomputation would still return the
/// same number today (the assignment is unchanged, so `next_generation` is
/// idempotent) but the guarantee would rest on that coincidence rather than on
/// where the call sits, and the first structural re-push would break it.
///
/// # Retryable versus terminal
///
/// Split per [`PushDisposition`]: `generation_mismatch` and
/// `no_applied_generation` are transient apply failures and consume retries (an
/// identical re-send is an idempotent MH no-op), while
/// `transport_mode_mismatch` is terminal — a version-skewed handler does not
/// become correct after backoff, so MC fails immediately rather than delaying
/// the loud failure by three attempts. `handler_id_mismatch` is neither: it is
/// non-fatal for the interim and never reaches this loop as an error at all
/// (see `media_routing::confirm::evaluate`).
#[expect(
    clippy::too_many_arguments,
    reason = "Spawned-task wiring; all params are distinct dependencies"
)]
async fn register_meeting_with_handlers(
    mh_client: &dyn MhRegistrationClient,
    mh_data: &MhAssignmentData,
    assignment: &MeetingAssignment,
    policy_generations: &PolicyGenerations,
    meeting_id: &str,
    mc_id: &str,
    mc_grpc_endpoint: &str,
    cancel_token: &CancellationToken,
) {
    for handler in &mh_data.handlers {
        let grpc_endpoint = &handler.grpc_endpoint;
        let handler_id = HandlerId::new(&handler.mh_id);

        let Some(handler_assignment) = assignment.for_handler(&handler_id) else {
            // The assignment is computed from this same handler list, so this
            // is unreachable. Log rather than skip silently: if it ever fires,
            // a handler is going unprogrammed and that must be visible.
            error!(
                target: "mc.register_meeting.trigger",
                mh_grpc_endpoint = %grpc_endpoint,
                "No assignment computed for this handler; not programming it"
            );
            continue;
        };

        let policy_generation = match policy_generations
            .next_generation(meeting_id, &handler_id, handler_assignment)
            .await
        {
            Ok(generation) => generation,
            Err(e) => {
                error!(
                    target: "mc.register_meeting.trigger",
                    mh_grpc_endpoint = %grpc_endpoint,
                    error = %e,
                    "Could not derive a policy generation; MH not programmed"
                );
                continue;
            }
        };

        let programming = MeetingProgramming {
            mh_grpc_endpoint: grpc_endpoint,
            expected_handler_id: &handler.mh_id,
            meeting_id,
            mc_id,
            mc_grpc_endpoint,
            assignment: handler_assignment,
            policy_generation,
        };

        let mut last_error = None;
        // Attempts actually made, and whether the loop stopped because retrying
        // could not help. Both feed the exit log: a terminal failure that
        // deliberately did NOT retry must not report a retry history it never
        // had (see the exit log below).
        let mut attempts_made = 0_u32;
        let mut ended_terminally = false;
        for attempt in 1..=MAX_REGISTER_ATTEMPTS {
            if cancel_token.is_cancelled() {
                info!(
                    target: "mc.register_meeting.trigger",
                    "RegisterMeeting cancelled during shutdown"
                );
                return;
            }
            attempts_made = attempt;
            match mh_client.register_meeting(&programming).await {
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
                    // A terminal divergence does not become correct after
                    // backoff. Fail once, loudly, rather than three times
                    // slowly.
                    let terminal = matches!(
                        &e,
                        McError::MediaPolicyDivergence { outcome }
                            if outcome.disposition() == PushDisposition::Terminal
                    );
                    warn!(
                        target: "mc.register_meeting.trigger",
                        attempt = attempt,
                        max_attempts = MAX_REGISTER_ATTEMPTS,
                        mh_grpc_endpoint = %grpc_endpoint,
                        terminal = terminal,
                        error = %e,
                        "RegisterMeeting attempt failed"
                    );
                    last_error = Some(e);
                    if terminal {
                        ended_terminally = true;
                        break;
                    }

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

        // TWO EXIT MESSAGES, because the two dispositions have OPPOSITE remedies
        // and this log is where a responder learns which one they have.
        //
        // `mc-incident-response.md` Scenario 12 keys on the literal string
        // "RegisterMeeting retries exhausted" and reads it as flaky coordination
        // — investigate the transport. A terminal `transport_mode_mismatch` is
        // the opposite: a genuine two-ends version skew whose remedy is to roll
        // MH FORWARD, never to roll MC back (`mc-deployment.md`'s rollback
        // carve-out). Reporting it as "retries exhausted" with
        // `total_attempts = 3` would assert a retry history that never happened
        // and route the responder to the wrong branch, under incident pressure.
        //
        // Both stay at `error!` on the same target, so the existing runbook
        // string keeps matching the case it was written for and the new one is
        // greppable for the case it was not.
        if let Some(e) = last_error {
            if ended_terminally {
                error!(
                    target: "mc.register_meeting.trigger",
                    mh_grpc_endpoint = %grpc_endpoint,
                    attempts_made = attempts_made,
                    terminal = true,
                    error = %e,
                    "RegisterMeeting failed terminally; not retried"
                );
            } else {
                error!(
                    target: "mc.register_meeting.trigger",
                    mh_grpc_endpoint = %grpc_endpoint,
                    total_attempts = MAX_REGISTER_ATTEMPTS,
                    attempts_made = attempts_made,
                    terminal = false,
                    error = %e,
                    "RegisterMeeting retries exhausted"
                );
            }
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
    use crate::media_routing::PolicyPushOutcome;

    // Full R-60 behavioral coverage (all-CONNECTED / partial / all-FAILED state
    // recording + metric + truncation + cap) lives in
    // `tests/media_connection_update_integration.rs`, which drives a real framed
    // `ClientMessage{MediaConnectionUpdate}` through the decode+dispatch seam.
    // These in-module tests only assert the non-`MediaConnectionUpdate` branches
    // don't panic and the char-boundary truncation helper is correct.

    /// The limiter is driven with SYNTHETIC instants, not sleeps.
    ///
    /// A wall-clock test of a 250 ms refill would either sleep (slow, and an
    /// implicit timing assertion in a suite that gates none) or race. Passing
    /// `now` in makes the refill arithmetic exactly testable and keeps the
    /// production call site a plain `Instant::now()`.
    #[test]
    fn mute_limiter_allows_a_burst_then_clamps_to_the_sustained_rate() {
        let t0 = Instant::now();
        let mut limiter = ClientWorkLimiter::new(t0, MUTE_WORK_BURST, MUTE_WORK_REFILL_INTERVAL_MS);

        // The whole burst is available immediately: a human flurry never waits.
        for i in 0..MUTE_WORK_BURST {
            assert!(limiter.try_spend(t0), "burst token {i} should be available");
        }
        // Exhausted, and it stays exhausted while no time passes — this is the
        // clamp on a client sending at line rate.
        assert!(!limiter.try_spend(t0));
        assert!(!limiter.try_spend(t0));
    }

    #[test]
    fn mute_limiter_refills_at_the_sustained_rate_and_never_permanently_denies() {
        let t0 = Instant::now();
        let mut limiter = ClientWorkLimiter::new(t0, MUTE_WORK_BURST, MUTE_WORK_REFILL_INTERVAL_MS);
        for _ in 0..MUTE_WORK_BURST {
            assert!(limiter.try_spend(t0));
        }
        assert!(!limiter.try_spend(t0));

        // One interval later exactly one token is back. THIS is the property
        // that makes it a rate limit and not a budget: no amount of past
        // spending can permanently deny a later legitimate toggle.
        let t1 = t0 + Duration::from_millis(MUTE_WORK_REFILL_INTERVAL_MS);
        assert!(limiter.try_spend(t1));
        assert!(!limiter.try_spend(t1));

        // Three intervals later, three tokens.
        let t2 = t1 + Duration::from_millis(MUTE_WORK_REFILL_INTERVAL_MS * 3);
        assert!(limiter.try_spend(t2));
        assert!(limiter.try_spend(t2));
        assert!(limiter.try_spend(t2));
        assert!(!limiter.try_spend(t2));
    }

    #[test]
    fn mute_limiter_caps_refill_at_the_burst_and_does_not_bank_idle_time() {
        let t0 = Instant::now();
        let mut limiter = ClientWorkLimiter::new(t0, MUTE_WORK_BURST, MUTE_WORK_REFILL_INTERVAL_MS);
        for _ in 0..MUTE_WORK_BURST {
            assert!(limiter.try_spend(t0));
        }
        // An hour idle must not bank an hour of tokens, or the limiter becomes
        // a no-op for any client patient enough to wait once.
        let much_later = t0 + Duration::from_secs(3600);
        for _ in 0..MUTE_WORK_BURST {
            assert!(limiter.try_spend(much_later));
        }
        assert!(!limiter.try_spend(much_later));
    }

    #[test]
    fn mute_limiter_carries_a_sub_interval_remainder_rather_than_discarding_it() {
        // A client polling just under the refill interval must still earn
        // tokens over time. Discarding the remainder on every call would starve
        // it forever while looking like a working limiter.
        let t0 = Instant::now();
        let mut limiter = ClientWorkLimiter::new(t0, MUTE_WORK_BURST, MUTE_WORK_REFILL_INTERVAL_MS);
        for _ in 0..MUTE_WORK_BURST {
            assert!(limiter.try_spend(t0));
        }

        let just_under = Duration::from_millis(MUTE_WORK_REFILL_INTERVAL_MS - 10);
        let mut now = t0;
        let mut granted = 0;
        for _ in 0..30 {
            now += just_under;
            if limiter.try_spend(now) {
                granted += 1;
            }
        }
        // 30 x 240 ms = 7.2 s, so ~28 tokens are earned but only the burst-capped
        // stream is spendable; the point is simply that it is NOT zero.
        assert!(
            granted > 0,
            "a client polling just under the interval must still earn tokens"
        );
    }

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
        // Should not panic -- exercises the catch-all branch.
        //
        // Driven through the no-media-context path deliberately: `MuteRequest`
        // IS handled on the full path now (ADR-0036 §5), so asserting
        // "unhandled" there would assert the opposite of the behaviour. What
        // this still pins is that the decode+dispatch skeleton tolerates a
        // message it has no arm for.
        handle_client_message_without_media(&data, "test-conn-3", &handle).await;
    }

    #[tokio::test]
    async fn test_handle_client_message_invalid_data() {
        let (handle, _task) = test_participant_handle();
        let garbage = vec![0xFF, 0xFE, 0xFD, 0xFC, 0xFB];
        // Should not panic -- exercises the decode error branch
        handle_client_message_without_media(&garbage, "test-conn-4", &handle).await;
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
        handle_client_message_without_media(&data, "test-conn-5", &handle).await;
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
            _programming: &'a MeetingProgramming<'a>,
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

    /// A one-participant assignment covering exactly the handlers in `mh_data`,
    /// which is the shape the trigger builds at N=1.
    fn make_assignment(mh_data: &MhAssignmentData) -> MeetingAssignment {
        use crate::media_admission::SenderId;
        use std::num::NonZeroU16;

        let handlers: Vec<HandlerId> = mh_data
            .handlers
            .iter()
            .map(|h| HandlerId::new(&h.mh_id))
            .collect();
        compute_assignment(&MeetingRoutingInput {
            participants: vec![RoutingParticipant {
                sender_id: SenderId::from_nonzero(NonZeroU16::new(1).unwrap()),
                handlers: handlers.clone(),
            }],
            handlers,
        })
        .expect("assignment")
    }

    /// Drive the trigger's fan-out with a fresh generation registry.
    async fn run_register(
        client: &MockRegClient,
        mh_data: &MhAssignmentData,
        cancel: &CancellationToken,
    ) {
        let assignment = make_assignment(mh_data);
        let generations = PolicyGenerations::new();
        register_meeting_with_handlers(
            client,
            mh_data,
            &assignment,
            &generations,
            "m1",
            "mc1",
            "http://mc:50052",
            cancel,
        )
        .await;
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

        run_register(&client, &mh_data, &cancel).await;

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

        run_register(&client, &mh_data, &cancel).await;

        assert_eq!(
            client.call_count(),
            MAX_REGISTER_ATTEMPTS,
            "Should attempt exactly MAX_REGISTER_ATTEMPTS times"
        );
    }

    /// OPS-3's terminal split, pinned by USE rather than by value.
    ///
    /// `confirm.rs` pins what `disposition()` *returns*; nothing pinned that the
    /// retry loop *acts* on it. Every other test here drives the loop with
    /// `McError::Grpc`, which is non-terminal, so deleting `if terminal { break }`
    /// left the whole suite green.
    ///
    /// A `transport_mode_mismatch` is a genuine two-ends version skew: backoff
    /// cannot make a handler running a different transport mode agree, so
    /// failing three times slowly is strictly worse than failing once loudly.
    /// This also covers the terminal arm of the split exit log — the path that
    /// must NOT report "retries exhausted" with `total_attempts = 3`.
    #[tokio::test(start_paused = true)]
    async fn test_register_terminal_divergence_fails_without_retrying() {
        let client = MockRegClient::new(vec![Err(McError::MediaPolicyDivergence {
            outcome: PolicyPushOutcome::TransportModeMismatch,
        })]);
        let mh_data = make_mh_data(vec![MhEndpointInfo {
            mh_id: "mh-1".to_string(),
            webtransport_endpoint: "wt://mh-1:4433".to_string(),
            grpc_endpoint: "http://mh-1:50053".to_string(),
        }]);
        let cancel = CancellationToken::new();

        run_register(&client, &mh_data, &cancel).await;

        assert_eq!(
            client.call_count(),
            1,
            "a terminal divergence must fail on the FIRST attempt; retrying a \
             version-skewed handler only delays the loud failure"
        );
    }

    /// The other arm of the same split: a retryable divergence must consume the
    /// full retry budget. Asserted with a divergence input rather than a generic
    /// gRPC error, so the two dispositions are pinned by the same input class and
    /// a mis-classification cannot pass by landing on the other arm's test.
    #[tokio::test(start_paused = true)]
    async fn test_register_retryable_divergence_consumes_full_retry_budget() {
        let client = MockRegClient::new(vec![
            Err(McError::MediaPolicyDivergence {
                outcome: PolicyPushOutcome::GenerationMismatch,
            }),
            Err(McError::MediaPolicyDivergence {
                outcome: PolicyPushOutcome::GenerationMismatch,
            }),
            Err(McError::MediaPolicyDivergence {
                outcome: PolicyPushOutcome::GenerationMismatch,
            }),
        ]);
        let mh_data = make_mh_data(vec![MhEndpointInfo {
            mh_id: "mh-1".to_string(),
            webtransport_endpoint: "wt://mh-1:4433".to_string(),
            grpc_endpoint: "http://mh-1:50053".to_string(),
        }]);
        let cancel = CancellationToken::new();

        run_register(&client, &mh_data, &cancel).await;

        assert_eq!(
            client.call_count(),
            MAX_REGISTER_ATTEMPTS,
            "a transient apply failure is an idempotent re-send MH-side; it must \
             consume the retry budget rather than fail once"
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

        run_register(&client, &mh_data, &cancel).await;

        assert_eq!(
            client.call_count(),
            4,
            "1 call for handler 1 (success) + 3 calls for handler 2 (exhausted)"
        );
    }
}
