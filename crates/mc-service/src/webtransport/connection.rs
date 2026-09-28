//! `ConnectionActor` - owns WebTransport streams and bridge loop.
//!
//! This actor lives in the webtransport layer (not the actor hierarchy) and:
//! 1. Receives a `JoinResult` from the meeting actor via oneshot
//! 2. Sends `JoinResponse` to the client over the WebTransport stream
//! 3. Runs the bridge loop forwarding `ParticipantUpdate` messages to the client
//! 4. Notifies the meeting when the connection drops (via `MeetingActorHandle`)

use crate::actors::messages::{
    DisconnectCause, JoinResult, ServerMuteDecision, ServerMuteRefusal, UnmuteRelay,
};
use crate::actors::{
    BoundedMhStatus, MeetingActorHandle, MeetingControllerActorHandle, MhState,
    ParticipantActorHandle,
};
use crate::actors::{JoinMedia, MediaRoutingDeps};
use crate::auth::McJwtValidator;
use crate::errors::McError;
use crate::media_routing::{HandlerEndpoint, HandlerId, MeetingHandlers};
use crate::media_signaling::{
    CapabilityOutcome, DirectiveOutcome, MuteOutcome, ReceiveCapabilityDeclaration,
    RefusalReplySurface, ServerMuteAction, ServerMuteOutcome, UnmuteRequestOutcome,
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
use tracing::{debug, info, instrument, warn};
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
pub async fn handle_connection(
    incoming: IncomingSession,
    controller_handle: Arc<MeetingControllerActorHandle>,
    jwt_validator: Arc<McJwtValidator>,
    redis_client: Arc<dyn MhAssignmentStore>,
    media_deps: Arc<MediaRoutingDeps>,
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
    // validation*, and a join that later fails on capacity, Conflict or
    // Draining is still counted. That is the correct denominator for the
    // question this metric exists to answer, which is a question about the
    // CLIENT population ("are client builds publishing keys?"), not about
    // server admission: conditioning it on server capacity would answer a
    // different question, and it would make the series go dead during
    // precisely the capacity incident in which an operator would consult it.
    // (`sender_id` exhaustion is no longer such an incident — since story 2
    // R-16 it triggers a KEK-epoch reset and the join succeeds.) `sum()` of
    // this metric therefore does NOT
    // equal `mc_session_joins_total{status="success"}`; the catalog says so.
    metrics::record_join_identity_key_presence(if identity_public_key.is_some() {
        "present"
    } else {
        "absent"
    });

    // Step 6c: resolve the live meeting BEFORE reading its handler assignment,
    // so a join into a meeting this MC does not host reports NOT_FOUND rather
    // than a missing assignment. The handle is kept for post-join signalling.
    let meeting_handle = match controller_handle
        .get_meeting_handle(meeting_id.clone())
        .await
    {
        Ok(handle) => handle,
        Err(e) => {
            warn!(
                target: "mc.webtransport.connection",
                connection_id = %connection_id,
                meeting_id = %meeting_id,
                error = %e,
                "Meeting not hosted on this MC"
            );
            let _ = send_error(&mut send_stream, e.error_code(), &e.client_message()).await;
            metrics::record_session_join(
                "failure",
                Some(e.error_type_label()),
                join_start.elapsed(),
            );
            return Err(e);
        }
    };

    // Step 6d: read the meeting's handler assignment BEFORE admission (R-6).
    //
    // A meeting without media handlers is not useful, and the actor freezes the
    // meeting's handler set in its first join turn (ADR-0036 §9) and offers
    // every joiner that whole set, so it must be in hand before the join rather
    // than read afterwards. A missing or
    // malformed assignment now fails the join before any admission state
    // exists — nothing to unwind.
    let handlers = match read_meeting_handlers(redis_client.as_ref(), &meeting_id).await {
        Ok(handlers) => handlers,
        Err(e) => {
            warn!(
                target: "mc.webtransport.connection",
                connection_id = %connection_id,
                meeting_id = %meeting_id,
                error = %e,
                "No usable MH assignment for this meeting — cannot join without media handlers"
            );
            let _ = send_error(&mut send_stream, e.error_code(), &e.client_message()).await;
            metrics::record_session_join(
                "failure",
                Some(e.error_type_label()),
                join_start.elapsed(),
            );
            return Err(e);
        }
    };

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
            JoinMedia {
                deps: Arc::clone(&media_deps),
                handlers,
            },
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

    // Step 8: Build and send JoinResponse.
    let join_response = build_join_response(&join_result);
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

    // Step 8b (story 2 R-11): replay who is server-muted RIGHT NOW, written
    // directly after the JoinResponse and before the bridge loop starts
    // forwarding queued updates — the snapshot was taken in the join's own actor
    // turn, and every later change is queued behind it, so the joiner sees
    // snapshot-then-deltas. Through the SAME encoder as a live broadcast.
    for update in &join_result.server_mute_replay {
        let Some(encoded) = crate::webtransport::handler::encode_participant_update(update) else {
            continue;
        };
        if let Err(e) = write_framed_message(&mut send_stream, &encoded.server_message).await {
            warn!(
                target: "mc.webtransport.connection",
                connection_id = %connection_id,
                error = %e,
                "Failed to replay server-mute state to the joiner"
            );
            return Err(e);
        }
    }

    // Step 9: The per-connection media-signalling context (ADR-0036 §5, §6):
    // the live meeting handle plus the per-connection bounds. The MH push and
    // every client view are the MEETING ACTOR's work; this connection only
    // validates, bounds and forwards what its client says.
    // `is_host` is threaded from the ONE derivation above (`claims.role`), never
    // re-derived from a message or a roster lookup at this layer.
    let mut media_context =
        media_signaling_context(meeting_handle, &join_result, client_media_config, is_host);

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
    media: &mut MediaSignalingContext,
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
                        handle_client_message(
                            &data,
                            connection_id,
                            participant_handle,
                            media,
                        )
                        .await;
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
/// - `ReceiveCapability` (ADR-0036 §6): validates the declaration and, once
///   accepted, registers it with the meeting actor, which re-renders, re-pushes
///   and emits the send directive and slot assignments.
/// - `MuteRequest` (ADR-0036 §5): records the reported client mute on the
///   meeting actor, which re-emits the changed slot view to the subscribers
///   holding this source. **Never touches the send directive** — that
///   invariant is enforced by the module boundary in
///   `media_signaling::directive`, which has no path to mute state at all.
/// - `ServerMuteRequest` (ADR-0036 §5, §7; story 2 R-8): host-only server
///   mute — see [`handle_server_mute_request`] for the order of checks.
/// - `UnmuteRequest` (R-10): a server-muted participant asks the host(s);
///   NOTIFIES, never clears — see [`handle_unmute_request`].
/// - All other messages, INCLUDING `UnmuteResponse` (the host lifts a mute
///   with a `ServerMuteRequest`, so there is one lift path under one authority):
///   ignored, logged at debug.
///
/// # A client that never declares is never told to send
///
/// The meeting actor emits a participant's view only once it has declared a
/// receive capability — see the `media_signaling` module doc. Such a connection
/// stays healthy: no directive, no assignments, no error, and every other
/// post-join message keeps working.
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
            handle_mute_request(&request, connection_id, media).await;
        }
        Some(client_message::Message::ServerMuteRequest(request)) => {
            handle_server_mute_request(&request, connection_id, participant_handle, media).await;
        }
        Some(client_message::Message::UnmuteRequest(request)) => {
            handle_unmute_request(&request, connection_id, media).await;
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

/// Server-mute work tokens a fresh connection may spend immediately.
///
/// DERIVED from the client-mute bucket rather than restated, and asserted below
/// to be no looser: R-8 requires server mute to be rate-bounded "at least as
/// tightly as" client mute, and a second independent number would drift.
/// Spent only by a HOST (authority is checked first), so this bounds the one
/// principal who may drive a roster-wide broadcast from one connection.
const SERVER_MUTE_WORK_BURST: u32 = MUTE_WORK_BURST;

/// Milliseconds per refilled server-mute work token. See
/// [`SERVER_MUTE_WORK_BURST`].
const SERVER_MUTE_WORK_REFILL_INTERVAL_MS: u64 = MUTE_WORK_REFILL_INTERVAL_MS;

// R-8: server mute bounded at least as tightly as client mute. Fails the build,
// not a test, if either bucket is later loosened past the other.
const _: () = {
    assert!(SERVER_MUTE_WORK_BURST <= MUTE_WORK_BURST);
    assert!(SERVER_MUTE_WORK_REFILL_INTERVAL_MS >= MUTE_WORK_REFILL_INTERVAL_MS);
};

/// Server-mute REFUSAL replies a fresh connection may be sent immediately.
///
/// Its own bucket, NOT the capability path's `rejection_reply_limiter`: that
/// bucket's rationale is "a correction that converges", which an authorization
/// probe is not, and sharing it would let a client driving refusals here
/// silence the error replies another path needs. Refusals stay fully counted
/// however fast they arrive; only the reply is rationed.
const SERVER_MUTE_REFUSAL_REPLY_BURST: u32 = 4;

/// Milliseconds per refilled server-mute refusal-reply token — 1 s.
const SERVER_MUTE_REFUSAL_REPLY_REFILL_INTERVAL_MS: u64 = 1_000;

/// `UnmuteRequest`s a fresh connection may send immediately.
///
/// Tighter than mute, because each one is relayed into ANOTHER participant's
/// mailbox and onto a human host's screen: asking a couple of times is
/// legitimate, a stream of asks is harassment of the host.
const UNMUTE_REQUEST_BURST: u32 = 2;

/// Milliseconds per refilled unmute-request token — 5 s.
const UNMUTE_REQUEST_REFILL_INTERVAL_MS: u64 = 5_000;

/// Per-connection token bucket bounding a repeatable client-driven action.
///
/// # Why a rate limit and NOT a budget
///
/// The receive-capability path takes a cumulative budget for ACCEPTED
/// declarations because re-declaring is rare and a client that stops
/// re-declaring loses nothing. Every action bounded here is the opposite: a
/// repeatable steady-state path that must keep working for the life of the
/// session. A cumulative budget would spend out and then permanently deny — for
/// instance freezing `audio_self_muted` on the roster so every other
/// participant renders a live speaker as muted, or silencing the error a client
/// needs in order to correct its declaration.
///
/// A rate limit bounds work per unit time and never permanently denies. See
/// `media_signaling`'s module doc for the criterion in general form.
///
/// # One instance per bounded path, deliberately never a shared bucket
///
/// [`MediaSignalingContext`] holds a separate bucket for every bounded path —
/// client mute, capability rejection replies, server mute, server-mute refusal
/// replies, unmute requests — so that flooding ANY one of them cannot suppress
/// any other on the same connection. A shared bucket would make each path a
/// denial vector against the rest, which is the amplification problem one
/// level down.
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

/// Per-connection state for the ADR-0036 §5/§6 client-facing media signalling.
///
/// Owned by the single-threaded bridge loop, so no lock: every field is read and
/// written from one task.
///
/// # What lives here, and what does not
///
/// Only the CLIENT-DRIVEN bounds and short-circuits live here — the declaration
/// budget, the identical-redeclaration and identical-mute short-circuits, the
/// mute rate limit and the rejection-reply limit. Every rejection is decided
/// from this struct alone, so a client looping malformed declarations cannot
/// drive work through the shared meeting actor's mailbox.
///
/// Routing state does NOT live here any more. Story 1 cached the handler set and
/// the "planned" audio slot per connection; both were sound only while MC had
/// no per-participant routing and pushed once at join. Since story 2 the
/// meeting actor holds observed connectivity, edges, slot demand and slot state, and composes every
/// participant's view itself (including views changed by SOMEONE ELSE's join,
/// leave, declaration or mute), so there is no per-connection copy left to go
/// stale.
///
/// # Client-driven paths into the actor, each bounded
///
/// - **capability** — only *accepted* declarations reach the actor, bounded per
///   connection by [`Self::declaration_budget`].
/// - **client mute** — bounded by [`Self::mute_limiter`], a RATE limit rather
///   than a budget; an identical repeat never reaches the actor.
///
/// The actor's own re-emit fan-out is bounded per MEETING, per actor turn
/// (`actors::meeting_media::SLOT_VIEW_FLUSH_BATCH`) — see `media_signaling`'s
/// module doc for why that path needs a third kind of bound.
struct MediaSignalingContext {
    /// Live handle to the meeting actor, taken once at join.
    meeting_handle: MeetingActorHandle,
    /// This connection's participant id.
    participant_id: String,
    /// Configured cap on declared slots per declaration.
    max_receive_slots: usize,
    /// Remaining budget of ACCEPTED declarations on this connection.
    declaration_budget: u32,
    /// The last declaration the meeting actor ACCEPTED, if any.
    ///
    /// Doubles as the identical-redeclaration short-circuit: an unchanged
    /// declaration is answered without an actor hop.
    declaration: Option<ReceiveCapabilityDeclaration>,
    /// The last client-mute pair this connection reported.
    ///
    /// The mute-path counterpart of the identical-redeclaration short-circuit:
    /// `MuteRequest` is otherwise the cheapest way to drive an actor hop from a
    /// ~4-byte message.
    ///
    /// SOUND because `audio_self_muted`/`video_self_muted` have exactly ONE
    /// writer in the tree (`MeetingActor::handle_self_mute`); `handle_server_mute`
    /// writes only the `*_server_muted` fields. `None` at join, so the first
    /// report always passes.
    last_reported_mute: Option<(bool, bool)>,
    /// Bounds client-driven mute work (a RATE limit: mute is a repeatable
    /// steady-state user action, and a cumulative budget would permanently deny
    /// it).
    mute_limiter: ClientWorkLimiter,

    /// Bounds the rejection RESPONSE, not the counting (ADR-0036 §6).
    ///
    /// A rejected declaration is ~12 bytes inbound and provokes a ~120-byte
    /// `ErrorMessage` out, sharing the participant mailbox with roster
    /// broadcasts. The counter and the one-shot WARN are deliberately OUTSIDE
    /// this bound: rejections stay fully observable however fast they arrive,
    /// and only the reply is rationed.
    rejection_reply_limiter: ClientWorkLimiter,
    /// Whether the mute rate limit has already been logged on this connection.
    mute_rate_limit_logged: bool,
    /// Whether a capability rejection has already been logged on this
    /// connection. Rejections are ALWAYS counted; the WARN fires once.
    rejection_logged: bool,

    /// One-shot bound on the rejection-reply-suppressed WARN.
    rejection_reply_suppressed_logged: bool,

    /// Whether this connection's token carries `MeetingRole::Host`.
    ///
    /// Taken from the ONE derivation at accept (`claims.role`); never
    /// re-derived here from a message or a roster lookup. The meeting actor
    /// repeats the check against its roster entry — the two are AND-composed.
    is_host: bool,
    /// Bounds a host's server-mute work (see [`SERVER_MUTE_WORK_BURST`]).
    server_mute_limiter: ClientWorkLimiter,
    /// Bounds the REPLIES to refused server-mute requests (never the counting).
    server_mute_refusal_reply_limiter: ClientWorkLimiter,
    /// Bounds `UnmuteRequest`s relayed to the host(s).
    unmute_request_limiter: ClientWorkLimiter,
    /// One-shot bound on the server-mute refusal WARN (refusals always counted).
    server_mute_refusal_logged: bool,
    /// One-shot bound on the server-mute rate-limit WARN.
    server_mute_rate_limit_logged: bool,
    /// One-shot bound on the server-mute refusal-reply-suppressed WARN.
    server_mute_refusal_reply_suppressed_logged: bool,
    /// One-shot bound on the unmute-request rate-limit WARN.
    unmute_rate_limit_logged: bool,
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

/// Handle a post-join `ReceiveCapability` (ADR-0036 §6).
///
/// Validate (every rejection decided here, without touching the actor), then
/// register the accepted declaration with the meeting actor — the source of
/// truth for slot demand — which re-renders, re-pushes and emits the send
/// directive and slot assignments to this and every other affected participant.
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

    // The slot cap is checked HERE, against `media.max_receive_slots` — the
    // same `ClientMediaConfig::max_receive_slots` field `main.rs` publishes as
    // `mc_media_receive_slot_cap`. Over the cap is rejected whole and counted
    // as `slot_count_over_cap` (R-1: loud, never one silent participant).
    let declaration = match ReceiveCapabilityDeclaration::parse(
        capability,
        media.max_receive_slots,
        media.budget_remaining(),
    ) {
        Ok(declaration) => declaration,
        Err(outcome) => {
            reject_capability(outcome, connection_id, participant_handle, media).await;
            return;
        }
    };

    // Identical re-declaration: MC does NO work and sends NOTHING — no actor
    // hop, no budget charge. Recorded under its own outcome rather than as
    // `Accepted`, because `accepted` means "declarations MC acted on" and is the
    // denominator for any rejection ratio; this arm is client-inflatable at
    // near-zero server cost. No log line: one per message would be a
    // log-amplification vector, and the counter is the complete record.
    if media.declaration.as_ref() == Some(&declaration) {
        metrics::record_receive_capability(CapabilityOutcome::AcceptedUnchanged);
        return;
    }

    // Charged on every accepted declaration — including one the actor then
    // fails to register — which is what keeps retries bounded.
    media.charge_budget();
    metrics::record_receive_capability(CapabilityOutcome::Accepted);

    // ARM the identical-redeclaration short-circuit only once the actor holds
    // the declaration. Arming it first and then failing would leave the
    // connection PERMANENTLY DARK: the client's natural recovery — re-sending
    // the same declaration — would be answered `accepted_unchanged` forever.
    match media
        .meeting_handle
        .register_receive_capability(media.participant_id.clone(), declaration.clone())
        .await
    {
        Ok(()) => media.declaration = Some(declaration),
        Err(e) => {
            metrics::record_send_directive(DirectiveOutcome::MeetingStateUnavailable);
            warn!(
                target: "mc.webtransport.connection",
                connection_id = %connection_id,
                error = %e,
                "Meeting actor did not register the receive capability; no media signalling \
                 will be emitted until the client re-declares"
            );
        }
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
        warn!(
            target: "mc.webtransport.connection",
            connection_id = %connection_id,
            outcome = outcome.label(),
            "Rejecting receive capability declaration. Further rejections on this connection \
             are counted, not logged."
        );
    }

    // Bound the REPLY only. The counter above already fired and the WARN has
    // already had its one-shot chance, so a flooding client stays fully
    // observable; what it stops buying is ~10x egress amplification and
    // contention for its own participant mailbox. Spent here — after the
    // counter, before the send — so a suppressed rejection is counted, not
    // silent.
    if !media.rejection_reply_limiter.try_spend(Instant::now()) {
        metrics::record_refusal_reply_suppressed(RefusalReplySurface::Capability);
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
/// Records the reported state on the meeting actor (the single home for mute),
/// which re-emits the changed slot view — `SLOT_STATE_SOURCE_MUTED` — to the
/// subscribers holding this source.
///
/// # Three guards, in this order, and the order is the security property
///
/// 1. **Identical repeat** — returns before the actor hop and before spending a
///    token, so a client stuck on one state costs a tuple compare.
/// 2. **Rate limit** ([`ClientWorkLimiter`]) — spent only on work about to be
///    done. This bounds the repeatable client-driven path into the SHARED
///    meeting actor per connection; the actor's resulting fan-out to holders is
///    bounded per MEETING by its per-turn flush bound.
/// 3. **Audio-only re-emit** — a video-only change is recorded by the actor but
///    re-emits nothing, because the slot view reads only audio mute.
///
/// # The send directive is not touched, and cannot be
///
/// `media_signaling::directive` has no import through which mute state could
/// arrive and `build_send_directive` takes no mute argument, so a mute/unmute
/// cycle altering the directive is a compile error rather than a behaviour to
/// be careful about. That is what keeps
/// *"MC has not asked you to send"* and *"you have muted yourself"* distinct,
/// and what makes unmute a purely local decision with no round trip.
async fn handle_mute_request(
    request: &v1::MuteRequest,
    connection_id: &str,
    media: &mut MediaSignalingContext,
) {
    // Short-circuit a no-op BEFORE the actor hop and before the limiter.
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

    // `applied` means the audio flag moved on a declared connection, so the
    // actor re-emits to the holders of this source. A video-only change, or a
    // report before any declaration, is recorded by the actor and re-emits
    // nothing: the slot view reads only audio mute. A camera button produces
    // `applied_no_recompose`, which is routine.
    if media.declaration.is_none() || previously_audio_muted == request.audio_muted {
        metrics::record_mute_request(MuteOutcome::AppliedNoRecompose);
        return;
    }
    metrics::record_mute_request(MuteOutcome::Applied);
}

/// `message_kind` for the server-mute refusal `ErrorMessage`.
const SIGNALING_KIND_SERVER_MUTE_REFUSAL: &str = "server_mute_refusal";

/// The ONE client-visible refusal for a server-mute request, whatever the
/// reason. A bounded `&'static str`; no client value is ever echoed back.
const SERVER_MUTE_REFUSED_MESSAGE: &str = "Server mute request refused";

/// Handle a post-join `ServerMuteRequest` (ADR-0036 §5, §7; story 2 R-8..R-11).
///
/// # The order of checks is the security property
///
/// 1. **Authority, before `request.participant_id` is read at all.** A
///    non-host is refused here, before any actor hop, so it never reaches the
///    shared meeting actor and learns nothing about who is in the meeting —
///    every request it sends gets the same refusal, whatever it names. The
///    ordering is the control; the single wire error below is defence in depth
///    on top of it (it collapses the one case where this layer and the actor
///    read different vintages of the role).
/// 2. **Rate limit** — a host's work only, from a bucket no looser than client
///    mute's ([`SERVER_MUTE_WORK_BURST`]).
/// 3. **The actor**, which re-checks authority against its roster (AND-composed)
///    and then resolves the target.
///
/// The requester is `media.participant_id` — the AUTHENTICATED connection's own
/// id. `ServerMuteRequest` carries no requester field, and the actor cannot tell
/// a forged `requester` from a real one, so this call site is the only place
/// that binding is made.
///
/// `reason` (client-controlled, echoed to another participant per the proto)
/// is deliberately neither forwarded nor logged.
async fn handle_server_mute_request(
    request: &v1::ServerMuteRequest,
    connection_id: &str,
    participant_handle: &ParticipantActorHandle,
    media: &mut MediaSignalingContext,
) {
    let action = ServerMuteAction::from_request(request.audio_muted, request.video_muted);

    // 1. Authority.
    if !media.is_host {
        refuse_server_mute(
            ServerMuteRefusal::NotPermitted,
            action,
            None,
            connection_id,
            participant_handle,
            media,
        )
        .await;
        return;
    }

    // 2. Rate limit (host work only).
    if !media.server_mute_limiter.try_spend(Instant::now()) {
        metrics::record_server_mute_request(action, ServerMuteOutcome::RateLimited);
        if !media.server_mute_rate_limit_logged {
            media.server_mute_rate_limit_logged = true;
            warn!(
                target: "mc.webtransport.connection",
                connection_id = %connection_id,
                requester_participant_id = %media.participant_id,
                "Server-mute requests on this connection exceeded the sustained rate limit and \
                 are being dropped. Further occurrences on this connection are counted, not logged."
            );
        }
        return;
    }

    // 3. The actor. The target id is client-supplied, so it is length-bounded
    // at this trust boundary before it enters the actor or a log line.
    let target = truncate_utf8(&request.participant_id, MAX_CLIENT_STRING_BYTES);
    match media
        .meeting_handle
        .server_mute(
            target.clone(),
            media.participant_id.clone(),
            request.audio_muted,
            request.video_muted,
        )
        .await
    {
        Err(e) => {
            metrics::record_server_mute_request(action, ServerMuteOutcome::ActorUnavailable);
            debug!(
                target: "mc.webtransport.connection",
                connection_id = %connection_id,
                error = %e,
                "Meeting actor unavailable for a server-mute request"
            );
        }
        Ok(Ok(decision)) => {
            let outcome = match decision {
                ServerMuteDecision::Applied => ServerMuteOutcome::Applied,
                ServerMuteDecision::Unchanged => ServerMuteOutcome::Unchanged,
            };
            metrics::record_server_mute_request(action, outcome);
            // The decision, with who asked and about whom, on the signalling
            // path where identity is legitimate. Participant ids only — never a
            // sender id, user id or display name.
            info!(
                target: "mc.webtransport.connection",
                connection_id = %connection_id,
                requester_participant_id = %media.participant_id,
                target_participant_id = %target,
                action = action.label(),
                outcome = outcome.label(),
                audio_muted = request.audio_muted,
                video_muted = request.video_muted,
                "Server mute decision"
            );
        }
        Ok(Err(refusal)) => {
            refuse_server_mute(
                refusal,
                action,
                Some(&target),
                connection_id,
                participant_handle,
                media,
            )
            .await;
        }
    }
}

/// Count, bound-log and answer a refused server-mute request.
///
/// ONE client-visible error for every refusal — same code, same static text —
/// so nothing about the refusal's reason reaches the client. The reason
/// survives only in the metric label and this log, which are operator-visible.
/// The counter always fires; the WARN fires once per connection; the REPLY is
/// rationed by its own bucket, and a withheld reply is itself counted.
async fn refuse_server_mute(
    refusal: ServerMuteRefusal,
    action: ServerMuteAction,
    target: Option<&str>,
    connection_id: &str,
    participant_handle: &ParticipantActorHandle,
    media: &mut MediaSignalingContext,
) {
    let outcome = match refusal {
        ServerMuteRefusal::NotPermitted => ServerMuteOutcome::NotPermitted,
        ServerMuteRefusal::UnknownTarget => ServerMuteOutcome::UnknownTarget,
    };
    metrics::record_server_mute_request(action, outcome);

    if !media.server_mute_refusal_logged {
        media.server_mute_refusal_logged = true;
        // The target is logged only when the requester was authorized: a
        // non-host's request is refused before its target is looked at.
        warn!(
            target: "mc.webtransport.connection",
            connection_id = %connection_id,
            requester_participant_id = %media.participant_id,
            target_participant_id = target.unwrap_or(""),
            action = action.label(),
            outcome = outcome.label(),
            "Server-mute request refused. Further refusals on this connection are counted, not \
             logged."
        );
    }

    if !media
        .server_mute_refusal_reply_limiter
        .try_spend(Instant::now())
    {
        metrics::record_refusal_reply_suppressed(RefusalReplySurface::ServerMute);
        if !media.server_mute_refusal_reply_suppressed_logged {
            media.server_mute_refusal_reply_suppressed_logged = true;
            warn!(
                target: "mc.webtransport.connection",
                connection_id = %connection_id,
                "Server-mute refusal replies on this connection exceeded their rate limit and are \
                 being suppressed. Refusals are still counted on \
                 mc_media_server_mute_requests_total and suppressions on \
                 mc_media_refusal_replies_suppressed_total."
            );
        }
        return;
    }

    send_signaling(
        participant_handle,
        connection_id,
        SIGNALING_KIND_SERVER_MUTE_REFUSAL,
        &ServerMessage {
            message: Some(server_message::Message::Error(ErrorMessage {
                code: v1::ErrorCode::Forbidden as i32,
                message: SERVER_MUTE_REFUSED_MESSAGE.to_string(),
                details: Default::default(),
            })),
            trace_parent: String::new(),
            trace_state: String::new(),
        },
    )
    .await;
}

/// Handle a post-join `UnmuteRequest` (R-10): a server-muted participant asks
/// the host(s) to lift its mute.
///
/// NOTIFIES — never clears. The actor relays a FRESH `UnmuteRequest` whose
/// `participant_id` is this connection's authenticated id, so the
/// `participant_id` a client put in its own request is never read and can never
/// be forwarded. There is no write path to server-mute state from here.
async fn handle_unmute_request(
    request: &v1::UnmuteRequest,
    connection_id: &str,
    media: &mut MediaSignalingContext,
) {
    if !media.unmute_request_limiter.try_spend(Instant::now()) {
        metrics::record_unmute_request(UnmuteRequestOutcome::RateLimited);
        if !media.unmute_rate_limit_logged {
            media.unmute_rate_limit_logged = true;
            warn!(
                target: "mc.webtransport.connection",
                connection_id = %connection_id,
                "Unmute requests on this connection exceeded their rate limit and are being \
                 dropped. Further occurrences on this connection are counted, not logged."
            );
        }
        return;
    }
    let outcome = match media
        .meeting_handle
        .request_unmute(
            media.participant_id.clone(),
            request.request_audio,
            request.request_video,
        )
        .await
    {
        Ok(UnmuteRelay::Relayed { .. }) => UnmuteRequestOutcome::Relayed,
        Ok(UnmuteRelay::NotServerMuted) => UnmuteRequestOutcome::NotServerMuted,
        Ok(UnmuteRelay::NoHostConnected) => UnmuteRequestOutcome::NoHostConnected,
        Err(_) => UnmuteRequestOutcome::ActorUnavailable,
    };
    metrics::record_unmute_request(outcome);
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
        .send(SignalingPayload::server_message(&message))
        .await
    {
        // WARN, not DEBUG. `ParticipantActorHandle::send` is an awaited mpsc
        // send, so an `Err` means the mailbox is CLOSED — the participant actor
        // is gone. This path sends only rejections and refusals — a capability
        // rejection or a server-mute refusal, named by `message_kind` —
        // (directives and slot assignments are the meeting actor's, which
        // counts its own delivery failures on
        // `mc_media_slot_view_emissions_total{outcome="delivery_failed"}`).
        warn!(
            target: "mc.webtransport.connection",
            connection_id = %connection_id,
            error = %e,
            // `message_kind`, NOT `payload_kind`: the `payload_kind` label on
            // `mc_participant_outbound_messages_dropped_total` is a DISJOINT
            // domain on a later hop (its values are enumerated in that
            // metric's catalog row, deliberately not restated here).
            message_kind = message_kind,
            "Failed to deliver a rejection or refusal to the participant actor"
        );
    }
}

/// Build the per-connection media-signalling context from the live meeting
/// handle resolved before the join.
fn media_signaling_context(
    meeting_handle: MeetingActorHandle,
    join_result: &JoinResult,
    config: crate::media_signaling::ClientMediaConfig,
    is_host: bool,
) -> MediaSignalingContext {
    MediaSignalingContext::new(
        meeting_handle,
        join_result.participant_id.clone(),
        &config,
        is_host,
    )
}

impl MediaSignalingContext {
    /// The ONE constructor, shared by the join path and the unit tests, so a
    /// test cannot drift from the limiters production actually installs.
    fn new(
        meeting_handle: MeetingActorHandle,
        participant_id: String,
        config: &crate::media_signaling::ClientMediaConfig,
        is_host: bool,
    ) -> Self {
        // One clock reading for every bucket: they start together at join, so a
        // skew between them would be meaningless state.
        let limiter_start = Instant::now();

        Self {
            meeting_handle,
            participant_id,
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
            is_host,
            server_mute_limiter: ClientWorkLimiter::new(
                limiter_start,
                SERVER_MUTE_WORK_BURST,
                SERVER_MUTE_WORK_REFILL_INTERVAL_MS,
            ),
            server_mute_refusal_reply_limiter: ClientWorkLimiter::new(
                limiter_start,
                SERVER_MUTE_REFUSAL_REPLY_BURST,
                SERVER_MUTE_REFUSAL_REPLY_REFILL_INTERVAL_MS,
            ),
            unmute_request_limiter: ClientWorkLimiter::new(
                limiter_start,
                UNMUTE_REQUEST_BURST,
                UNMUTE_REQUEST_REFILL_INTERVAL_MS,
            ),
            server_mute_refusal_logged: false,
            server_mute_rate_limit_logged: false,
            server_mute_refusal_reply_suppressed_logged: false,
            unmute_rate_limit_logged: false,
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

/// Read the meeting's handler assignment from Redis and freeze it into a
/// validated, sorted [`MeetingHandlers`] (R-6: no handlers, no join).
///
/// Every url is server-derived — read from the registration GC wrote — never
/// client-supplied and never computed from a pod ordinal.
async fn read_meeting_handlers(
    redis_client: &dyn MhAssignmentStore,
    meeting_id: &str,
) -> Result<MeetingHandlers, McError> {
    let mh_data: MhAssignmentData = redis_client
        .get_mh_assignment(meeting_id)
        .await?
        .ok_or_else(|| McError::MhAssignmentMissing(meeting_id.to_string()))?;
    MeetingHandlers::new(mh_data.handlers.into_iter().map(|h| HandlerEndpoint {
        id: HandlerId::new(h.mh_id),
        webtransport_url: h.webtransport_endpoint,
        grpc_endpoint: h.grpc_endpoint,
    }))
    .map_err(|e| McError::MhAssignmentMissing(format!("{meeting_id}: {e}")))
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

    // The meeting's FULL frozen handler set (ADR-0036 §9, R-33): every
    // participant is offered every handler and connects to all it can; MC then
    // routes each pair through a handler both are observed on. Read from the
    // actor's frozen `MeetingHandlers` — the same value that supplies every
    // `StreamAssignment.media_handler_url` and `SendTarget.media_handler_url` —
    // so all three are byte-identical strings (the client looks transports up by
    // exact url). Never the Redis list this join happened to read. Order carries
    // no meaning.
    let media_servers = result
        .media_handlers
        .iter()
        .map(|h| MediaServerInfo {
            media_handler_url: h.webtransport_url.clone(),
        })
        .collect();

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
        // u16 semantics widened into the proto's uint32. The CURRENT
        // generation, which advances on every rotation. Note 0 is a LEGAL
        // first generation, not a sentinel: the not-provisioned signal is
        // `meeting_kek` not being exactly 32 bytes, so never gate on
        // `kek_generation != 0`.
        kek_generation: u32::from(result.kek_generation),
        // W, from the same `Duration` the rotation debounce enforces
        // (`Config::kek_lifecycle`). The client derives its previous-KEK
        // retention as `min(W/2, ceiling)` from this, so MC performs no
        // retention derivation and no retention check — the relationship is
        // structural. Never 0 from this MC: W is required and bounded at load,
        // and 0 is reserved as the older-MC observable the client floor-
        // substitutes on.
        kek_rotation_debounce_seconds: result.kek_rotation_debounce_seconds,
        correlation_id: result.correlation_id.clone(),
        binding_token: result.binding_token.clone(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::observability::metrics::{KEY_CUSTODY_LABEL, KEY_CUSTODY_OPERATOR};

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

    /// A live media-signalling context over a throwaway meeting actor, for a
    /// NON-host connection.
    fn test_media_context() -> (MediaSignalingContext, tokio::task::JoinHandle<()>) {
        test_media_context_as(false)
    }

    /// As [`test_media_context`], choosing the connection's host authority.
    fn test_media_context_as(
        is_host: bool,
    ) -> (MediaSignalingContext, tokio::task::JoinHandle<()>) {
        use crate::actors::{ActorMetrics, ControllerMetrics, MeetingActor};
        let (meeting, task) = MeetingActor::spawn(
            "test-meeting".to_string(),
            CancellationToken::new(),
            ActorMetrics::new(),
            ControllerMetrics::new(),
            common::secret::SecretBox::new(Box::new(vec![0u8; 32])),
            crate::media_admission::fixtures::kek_lifecycle(),
        )
        .expect("system CSPRNG must be available in tests");
        let config = crate::media_signaling::ClientMediaConfig {
            max_receive_slots: 8,
            max_receive_capability_declarations: 4,
            audio_encoding: crate::media_signaling::AudioEncoding::new(v1::Codec::Opus, 48_000, 50)
                .expect("in band"),
            connect_settle_window: std::time::Duration::from_millis(1500),
        };
        (
            MediaSignalingContext::new(meeting, "test-part".to_string(), &config, is_host),
            task,
        )
    }

    #[tokio::test]
    async fn test_handle_client_message_unhandled_type() {
        let (handle, _task) = test_participant_handle();
        let (mut media, _meeting) = test_media_context();
        // `UnmuteResponse` has no dispatch arm by design (the host lifts a mute
        // with a `ServerMuteRequest`); it must fall through to the ignored
        // branch without panicking.
        let msg = ClientMessage {
            message: Some(client_message::Message::UnmuteResponse(
                v1::UnmuteResponse::default(),
            )),
            trace_parent: String::new(),
            trace_state: String::new(),
        };
        let data = msg.encode_to_vec();
        handle_client_message(&data, "test-conn-3", &handle, &mut media).await;
    }

    // ------------------------------------------------------------------
    // Server mute and unmute requests at the dispatch boundary (R-8, R-10)
    // ------------------------------------------------------------------

    /// A participant actor whose outbound stream the test can read.
    fn streamed_participant(
        participant_id: &str,
    ) -> (
        ParticipantActorHandle,
        tokio::sync::mpsc::Receiver<bytes::Bytes>,
    ) {
        use crate::actors::{ActorMetrics, ParticipantActor};
        let (tx, rx) = tokio::sync::mpsc::channel(64);
        let (handle, _task) = ParticipantActor::spawn_with_stream(
            format!("conn-{participant_id}"),
            participant_id.to_string(),
            "test-meeting".to_string(),
            CancellationToken::new(),
            ActorMetrics::new(),
            tx,
        );
        (handle, rx)
    }

    fn server_mute_msg(target: &str) -> Vec<u8> {
        ClientMessage {
            message: Some(client_message::Message::ServerMuteRequest(
                v1::ServerMuteRequest {
                    participant_id: target.to_string(),
                    audio_muted: true,
                    video_muted: false,
                    reason: String::new(),
                },
            )),
            trace_parent: String::new(),
            trace_state: String::new(),
        }
        .encode_to_vec()
    }

    /// The next `ErrorMessage` written to a participant's stream.
    async fn next_error(rx: &mut tokio::sync::mpsc::Receiver<bytes::Bytes>) -> ErrorMessage {
        let bytes = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("a reply is written")
            .expect("stream open");
        match ServerMessage::decode(&bytes[..]).unwrap().message {
            Some(server_message::Message::Error(e)) => e,
            other => unreachable!("expected an ErrorMessage, got {other:?}"),
        }
    }

    /// R-8: a NON-HOST is refused at dispatch, before any actor hop. Proven by
    /// making the hop impossible: the meeting actor is stopped first, so a
    /// request that reached it would be `actor_unavailable`, never
    /// `not_permitted`.
    #[tokio::test]
    async fn a_non_host_server_mute_is_refused_at_dispatch_without_an_actor_hop() {
        let snap = common::observability::testing::MetricAssertion::snapshot();
        let (participant, mut rx) = streamed_participant("test-part");
        let (mut media, meeting_task) = test_media_context_as(false);
        media.meeting_handle.cancel();
        let _ = meeting_task.await;

        handle_client_message(&server_mute_msg("anyone"), "c", &participant, &mut media).await;

        snap.counter("mc_media_server_mute_requests_total")
            .with_labels(&[
                ("action", "mute"),
                ("outcome", "not_permitted"),
                (KEY_CUSTODY_LABEL, KEY_CUSTODY_OPERATOR),
            ])
            .assert_delta(1);
        snap.counter("mc_media_server_mute_requests_total")
            .with_labels(&[
                ("action", "mute"),
                ("outcome", "actor_unavailable"),
                (KEY_CUSTODY_LABEL, KEY_CUSTODY_OPERATOR),
            ])
            .assert_delta(0);
        let error = next_error(&mut rx).await;
        assert_eq!(error.code, v1::ErrorCode::Forbidden as i32);
        assert_eq!(error.message, SERVER_MUTE_REFUSED_MESSAGE);
    }

    /// R-8, the ORDERING invariant at the metric: after driving the non-host
    /// path, the `unknown_target` cell is PRESENT-and-still-ZERO — never merely
    /// absent, which would pass on a process that never recorded anything. A
    /// non-host can never reach target resolution; if someone reorders the
    /// checks and reintroduces the existence oracle, this goes red.
    #[tokio::test]
    async fn a_non_host_never_reaches_unknown_target() {
        let snap = common::observability::testing::MetricAssertion::snapshot();
        crate::observability::metrics::zero_initialize_counters();
        let (participant, _rx) = streamed_participant("test-part");
        let (mut media, _meeting) = test_media_context_as(false);

        handle_client_message(
            &server_mute_msg("no-such-id"),
            "c",
            &participant,
            &mut media,
        )
        .await;

        snap.counter("mc_media_server_mute_requests_total")
            .with_labels(&[
                ("action", "mute"),
                ("outcome", "unknown_target"),
                (KEY_CUSTODY_LABEL, KEY_CUSTODY_OPERATOR),
            ])
            .assert_delta(0);
        snap.counter("mc_media_server_mute_requests_total")
            .with_labels(&[
                ("action", "mute"),
                ("outcome", "not_permitted"),
                (KEY_CUSTODY_LABEL, KEY_CUSTODY_OPERATOR),
            ])
            .assert_delta(1);
    }

    /// The two refusals are ONE wire error — byte-identical — so the client
    /// learns nothing about which one it was. Only the metric distinguishes.
    #[tokio::test]
    async fn not_permitted_and_unknown_target_are_byte_identical_on_the_wire() {
        // Non-host: refused at dispatch.
        let (participant, mut rx) = streamed_participant("test-part");
        let (mut media, _m1) = test_media_context_as(false);
        handle_client_message(&server_mute_msg("x"), "c", &participant, &mut media).await;
        let not_permitted = next_error(&mut rx).await;

        // A host on the roster naming an absent participant: the ACTOR refuses
        // with `UnknownTarget`.
        let (host, mut host_rx) = streamed_participant("test-part");
        let (mut host_media, _m2) = test_media_context_as(true);
        host_media
            .meeting_handle
            .connection_join(
                "conn-h".to_string(),
                "user-h".to_string(),
                "test-part".to_string(),
                String::new(),
                true,
                crate::media_admission::fixtures::sample_identity_key(),
                None,
                crate::actors::meeting_media::test_join_media(),
            )
            .await
            .unwrap();
        let snap = common::observability::testing::MetricAssertion::snapshot();
        handle_client_message(&server_mute_msg("ghost"), "c", &host, &mut host_media).await;
        snap.counter("mc_media_server_mute_requests_total")
            .with_labels(&[
                ("action", "mute"),
                ("outcome", "unknown_target"),
                (KEY_CUSTODY_LABEL, KEY_CUSTODY_OPERATOR),
            ])
            .assert_delta(1);
        let unknown_target = next_error(&mut host_rx).await;

        assert_eq!(
            not_permitted.encode_to_vec(),
            unknown_target.encode_to_vec(),
            "the two refusals must be indistinguishable to the client"
        );
    }

    /// R-8 rate bound: a host's server-mute work is bounded by a bucket no
    /// looser than client mute's (also a compile-time assertion); past the
    /// burst, requests are counted `rate_limited` and do no work.
    #[tokio::test]
    async fn a_host_is_rate_limited_after_the_server_mute_burst() {
        let (participant, _rx) = streamed_participant("test-part");
        let (mut media, _meeting) = test_media_context_as(true);
        for _ in 0..SERVER_MUTE_WORK_BURST {
            handle_client_message(&server_mute_msg("x"), "c", &participant, &mut media).await;
        }
        let snap = common::observability::testing::MetricAssertion::snapshot();
        handle_client_message(&server_mute_msg("x"), "c", &participant, &mut media).await;
        snap.counter("mc_media_server_mute_requests_total")
            .with_labels(&[
                ("action", "mute"),
                ("outcome", "rate_limited"),
                (KEY_CUSTODY_LABEL, KEY_CUSTODY_OPERATOR),
            ])
            .assert_delta(1);
    }

    /// Refusals are ALWAYS counted; past the reply burst the REPLY is withheld
    /// and that suppression is itself counted — and `not_permitted` keeps
    /// counting every refusal (the partition is intact).
    #[tokio::test]
    async fn suppressed_refusal_replies_are_counted_and_refusals_still_are() {
        let (participant, _rx) = streamed_participant("test-part");
        let (mut media, _meeting) = test_media_context_as(false);
        for _ in 0..SERVER_MUTE_REFUSAL_REPLY_BURST {
            handle_client_message(&server_mute_msg("x"), "c", &participant, &mut media).await;
        }
        let snap = common::observability::testing::MetricAssertion::snapshot();
        handle_client_message(&server_mute_msg("x"), "c", &participant, &mut media).await;
        snap.counter("mc_media_refusal_replies_suppressed_total")
            .with_labels(&[
                ("surface", "server_mute"),
                (KEY_CUSTODY_LABEL, KEY_CUSTODY_OPERATOR),
            ])
            .assert_delta(1);
        snap.counter("mc_media_server_mute_requests_total")
            .with_labels(&[
                ("action", "mute"),
                ("outcome", "not_permitted"),
                (KEY_CUSTODY_LABEL, KEY_CUSTODY_OPERATOR),
            ])
            .assert_delta(1);
    }

    /// R-10: the relayed `UnmuteRequest` carries the AUTHENTICATED requester's
    /// id, never the one the client wrote. Asserted POSITIVELY, on a path where
    /// the relay actually happens (the requester is genuinely server-muted and
    /// a host is connected) — a "forged value absent" check alone would pass
    /// vacuously on the no-relay path.
    #[tokio::test]
    async fn a_forged_participant_id_in_an_unmute_request_is_never_relayed() {
        let (_participant, _rx) = streamed_participant("test-part");
        let (mut media, _meeting) = test_media_context_as(false);
        let (host_tx, mut host_rx) = tokio::sync::mpsc::channel(64);
        let meeting = media.meeting_handle.clone();
        let join = |id: &str, host: bool, tx| {
            let meeting = meeting.clone();
            let id = id.to_string();
            async move {
                meeting
                    .connection_join(
                        format!("conn-{id}"),
                        format!("user-{id}"),
                        id,
                        String::new(),
                        host,
                        crate::media_admission::fixtures::sample_identity_key(),
                        tx,
                        crate::actors::meeting_media::test_join_media(),
                    )
                    .await
                    .unwrap()
            }
        };
        join("host", true, Some(host_tx)).await;
        join("test-part", false, None).await;
        meeting
            .server_mute("test-part".to_string(), "host".to_string(), true, false)
            .await
            .unwrap()
            .unwrap();

        let snap = common::observability::testing::MetricAssertion::snapshot();
        let forged = ClientMessage {
            message: Some(client_message::Message::UnmuteRequest(v1::UnmuteRequest {
                request_audio: true,
                request_video: false,
                participant_id: "forged-victim".to_string(),
            })),
            trace_parent: String::new(),
            trace_state: String::new(),
        }
        .encode_to_vec();
        handle_client_message(&forged, "c", &_participant, &mut media).await;

        snap.counter("mc_media_unmute_requests_total")
            .with_labels(&[
                ("outcome", "relayed"),
                (KEY_CUSTODY_LABEL, KEY_CUSTODY_OPERATOR),
            ])
            .assert_delta(1);
        let relayed = loop {
            let bytes = tokio::time::timeout(Duration::from_secs(5), host_rx.recv())
                .await
                .expect("the relay reached the host")
                .expect("stream open");
            if let Some(server_message::Message::UnmuteRequest(r)) =
                ServerMessage::decode(&bytes[..]).unwrap().message
            {
                break r;
            }
        };
        assert_eq!(
            relayed.participant_id, "test-part",
            "the relay names the AUTHENTICATED requester, whatever the client wrote"
        );
    }

    /// The unmute-request bucket is its own, and tighter than mute's.
    #[tokio::test]
    async fn unmute_requests_are_rate_limited_after_their_burst() {
        let (participant, _rx) = streamed_participant("test-part");
        let (mut media, _meeting) = test_media_context_as(false);
        let msg = ClientMessage {
            message: Some(client_message::Message::UnmuteRequest(v1::UnmuteRequest {
                request_audio: true,
                request_video: false,
                participant_id: String::new(),
            })),
            trace_parent: String::new(),
            trace_state: String::new(),
        }
        .encode_to_vec();
        for _ in 0..UNMUTE_REQUEST_BURST {
            handle_client_message(&msg, "c", &participant, &mut media).await;
        }
        let snap = common::observability::testing::MetricAssertion::snapshot();
        handle_client_message(&msg, "c", &participant, &mut media).await;
        snap.counter("mc_media_unmute_requests_total")
            .with_labels(&[
                ("outcome", "rate_limited"),
                (KEY_CUSTODY_LABEL, KEY_CUSTODY_OPERATOR),
            ])
            .assert_delta(1);
        const { assert!(UNMUTE_REQUEST_BURST < MUTE_WORK_BURST) };
    }

    #[tokio::test]
    async fn test_handle_client_message_invalid_data() {
        let (handle, _task) = test_participant_handle();
        let (mut media, _meeting) = test_media_context();
        let garbage = vec![0xFF, 0xFE, 0xFD, 0xFC, 0xFB];
        // Should not panic -- exercises the decode error branch
        handle_client_message(&garbage, "test-conn-4", &handle, &mut media).await;
    }

    #[tokio::test]
    async fn test_handle_client_message_empty_message() {
        let (handle, _task) = test_participant_handle();
        let (mut media, _meeting) = test_media_context();
        let msg = ClientMessage {
            message: None,
            trace_parent: String::new(),
            trace_state: String::new(),
        };
        let data = msg.encode_to_vec();
        // Should not panic -- exercises the None branch
        handle_client_message(&data, "test-conn-5", &handle, &mut media).await;
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
}
