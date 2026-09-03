//! Shared `TokenReceiver` fixture for MC tests.
//!
//! Every MC test that constructs a `GcClient` or an `MhClient` needs a
//! `TokenReceiver` carrying a throwaway token. That helper had **five**
//! near-identical private copies — three under `tests/` (`make_token_rx` in
//! `register_meeting_integration.rs`, `mock_token_receiver` in
//! `otel_grpc_outbound_integration.rs`, and a third in `gc_integration.rs`) and
//! two more in `#[cfg(test)]` modules inside `src/` (`grpc/mh_client.rs` and
//! `grpc/gc_client.rs`) — differing only in the token string and in how they
//! kept the `watch::Sender` alive (`Box::leak` versus a `OnceLock`).
//!
//! The `src/` pair is reachable from here because `TokenReceiver` is a
//! **third-crate** type (`common`), so it unifies across the rlib/`--test`
//! boundary. A helper returning one of `mc-service`'s *own* types would not —
//! see the note at `grpc/mh_client.rs`'s local `loopback_assignment`.
//!
//! That last difference is the one worth a single home: a `watch::Receiver`
//! whose sender drops still yields its last value, so both spellings happen to
//! work, and a fourth copy written without either would also *appear* to work
//! until something began observing changes. Keeping the sender alive
//! deliberately, in one place, is the point.

use common::secret::SecretString;
use common::token_manager::TokenReceiver;
use std::sync::OnceLock;
use tokio::sync::watch;

/// The token every MC test fixture presents. Not a credential — it is never
/// validated by anything in a component test.
const TEST_SERVICE_TOKEN: &str = "test-service-token";

/// A `TokenReceiver` yielding a fixed throwaway token.
///
/// The `watch::Sender` is held in a process-wide `OnceLock` so it outlives
/// every receiver handed out. A dropped sender would leave the receiver
/// yielding its last value — which works today and would silently stop working
/// for any test that ever waits on a refresh.
#[must_use]
pub fn test_token_receiver() -> TokenReceiver {
    static TOKEN_SENDER: OnceLock<watch::Sender<SecretString>> = OnceLock::new();

    let sender = TOKEN_SENDER.get_or_init(|| {
        let (tx, _rx) = watch::channel(SecretString::from(TEST_SERVICE_TOKEN));
        tx
    });

    TokenReceiver::from_test_channel(sender.subscribe())
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::secret::ExposeSecret;

    #[test]
    fn yields_the_fixed_test_token() {
        let rx = test_token_receiver();
        assert_eq!(rx.token().expose_secret(), TEST_SERVICE_TOKEN);
    }

    /// Two receivers from the same static sender both stay live — the property
    /// the `OnceLock` exists for.
    #[test]
    fn multiple_receivers_share_one_live_sender() {
        let a = test_token_receiver();
        let b = test_token_receiver();
        assert_eq!(a.token().expose_secret(), b.token().expose_secret());
    }
}
