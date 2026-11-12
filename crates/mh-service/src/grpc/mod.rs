//! gRPC service implementations for Media Handler.
//!
//! This module provides:
//! - `gc_client` - Client for MH→GC communication (registration, load reports)
//! - `mc_client` - Client for MH→MC communication (participant notifications)
//! - `mh_service` - Server for MC→MH communication (meeting registration)
//! - `auth_interceptor` - Authorization validation for incoming MC requests
//!
//! # Architecture
//!
//! ```text
//! MH → GC: RegisterMH, SendLoadReport
//! MH → MC: NotifyParticipantConnected, NotifyParticipantDisconnected
//! MC → MH: RegisterMeeting (requires authorization)
//! ```
//!
//! # Security
//!
//! All incoming gRPC requests from MC must pass through the [`MhAuthLayer`]
//! which validates authorization headers and enforces caller-type routing
//! (ADR-0003). This provides defense-in-depth beyond transport-level security.

pub mod auth_interceptor;
pub mod gc_client;
pub mod gc_task;
pub mod heartbeat_sender;
pub mod mc_client;
pub mod mh_service;
pub mod span_layer;

pub use auth_interceptor::MhAuthLayer;
pub use gc_client::GcClient;
pub use mc_client::McClient;
pub use mh_service::MhMediaService;
pub use span_layer::SpanLayer;

/// MH's inbound gRPC layer stack: `SpanLayer` → trace-context extraction →
/// `auth`. The ONE place the ordering is decided (see
/// [`common::observability::otel_grpc::inbound_layers`]); `main.rs` and the
/// test rig both use it.
#[must_use]
pub fn server_layers(
    auth: MhAuthLayer,
) -> common::observability::otel_grpc::InboundLayers<SpanLayer, MhAuthLayer> {
    common::observability::otel_grpc::inbound_layers(SpanLayer, auth)
}

/// Wrap `request` with MH's outbound service credential: THE one MH-local
/// home for the `authorization: Bearer` header (`McClient`, `GcClient` and the
/// MC heartbeat sender all call it; `docs/TODO.md` tracks the cross-service
/// extraction).
///
/// The token is read from `token_rx` per call and exposed ONLY into the
/// metadata value, never into a cached channel, endpoint or interceptor, and
/// never logged. FAILS CLOSED: an empty token (not yet acquired) is an error,
/// so no request is ever sent without credentials.
///
/// # Errors
///
/// [`crate::errors::MhError::OutboundAuthUnavailable`] when the token is empty
/// or does not form a valid header value. The caller logs under its own
/// target; the message never contains the token.
pub(crate) fn authorized_request<T>(
    token_rx: &common::token_manager::TokenReceiver,
    request: T,
) -> Result<tonic::Request<T>, crate::errors::MhError> {
    use common::secret::ExposeSecret;
    let current_token = token_rx.token();
    if current_token.expose_secret().is_empty() {
        return Err(crate::errors::MhError::OutboundAuthUnavailable(
            "service token not yet available".to_string(),
        ));
    }
    let value = format!("Bearer {}", current_token.expose_secret())
        .parse()
        .map_err(|e| {
            crate::errors::MhError::OutboundAuthUnavailable(format!(
                "Authorization header parse failed: {e}"
            ))
        })?;
    let mut grpc_request = tonic::Request::new(request);
    grpc_request.metadata_mut().insert("authorization", value);
    Ok(grpc_request)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::authorized_request;
    use common::secret::SecretString;
    use common::token_manager::TokenReceiver;
    use tokio::sync::watch;

    fn receiver(token: &str) -> (watch::Sender<SecretString>, TokenReceiver) {
        let (tx, rx) = watch::channel(SecretString::from(token));
        (tx, TokenReceiver::from_test_channel(rx))
    }

    /// Fail closed: an empty (not yet acquired) token never produces a
    /// request, so no outbound call is ever sent without credentials.
    #[test]
    fn an_empty_token_is_refused_and_sends_nothing() {
        let (_tx, rx) = receiver("");
        let err = authorized_request(&rx, ()).unwrap_err();
        assert!(
            matches!(err, crate::errors::MhError::OutboundAuthUnavailable(_)),
            "{err}"
        );
    }

    /// Positive arm: a token becomes exactly `Bearer <token>`.
    #[test]
    fn a_token_becomes_the_bearer_header() {
        let (_tx, rx) = receiver("tok-123");
        let request = authorized_request(&rx, ()).unwrap();
        assert_eq!(
            request
                .metadata()
                .get("authorization")
                .unwrap()
                .to_str()
                .unwrap(),
            "Bearer tok-123"
        );
    }

    /// A token that cannot form a header value is refused, and neither the
    /// error's Display nor its Debug carries the token.
    #[test]
    fn a_header_build_failure_never_echoes_the_token() {
        let secret = "secret-part\nmore-secret";
        let (_tx, rx) = receiver(secret);
        let err = authorized_request(&rx, ()).unwrap_err();
        assert!(matches!(
            err,
            crate::errors::MhError::OutboundAuthUnavailable(_)
        ));
        for rendered in [format!("{err}"), format!("{err:?}")] {
            assert!(!rendered.contains("secret-part"), "{rendered}");
            assert!(!rendered.contains("more-secret"), "{rendered}");
        }
    }
}
