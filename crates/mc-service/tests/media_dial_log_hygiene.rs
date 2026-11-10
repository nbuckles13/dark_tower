//! Log hygiene for the MC-minted `media_dial` credential (ADR-0039 §5,
//! `scripts/guards/semantic/checks.md` credential-leak item 12).
//!
//! Its own test binary ON PURPOSE: it installs a thread-local TRACE subscriber,
//! and tracing's callsite interest is process-global, so a concurrent test in
//! the same binary can cache "never" for a callsite and silently blind the
//! capture (the positive control below would catch it, as a flake).

#![cfg(feature = "test-seams")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "common/mod.rs"]
mod test_common;

use std::sync::{Arc, Mutex};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use mc_service::media_dial::status::ReportedState;
use test_common::media_dial::*;
use test_common::{mh_handler, MockMhRegistrationClient};

/// Nothing a mint, refresh, FAILED re-push or RegisterMeeting logs (at TRACE)
/// carries a token or key material. Positive control: the capture saw the
/// meeting id.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn no_token_or_key_reaches_a_log_line() {
    #[derive(Clone, Default)]
    struct Capture(Arc<Mutex<Vec<u8>>>);
    impl std::io::Write for Capture {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let capture = Capture::default();
    let sink = capture.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_writer(move || sink.clone())
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);
    // Callsite interest is process-global and another test's thread may have
    // cached "never" before this subscriber existed: recompute it.
    tracing::callsite::rebuild_interest_cache();

    let handlers = [mh_handler(H_A)];
    let mh = Arc::new(MockMhRegistrationClient::new());
    let meeting = spawn_meeting("dial-logs", &handlers);
    let mut a = join(&meeting, "a", &mh).await;
    let set = a.result.handler_set.clone();
    meeting
        .report_client_handler_statuses(
            "a".into(),
            vec![report(
                set.handlers[0].handler_ref,
                set.handler_set_version,
                ReportedState::Failed,
            )],
        )
        .await
        .unwrap();
    run_ready().await;
    advance(timing().refresh_interval()).await;
    let mut tokens: Vec<String> = set
        .handlers
        .iter()
        .map(|h| h.media_dial_token.clone())
        .collect();
    for s in sets(&mut a.rx) {
        tokens.extend(s.handlers.iter().map(|h| h.media_dial_token.clone()));
    }
    assert!(tokens.len() >= 3 && tokens.iter().all(|t| !t.is_empty()));
    let key = wire_key(&mh, "dial-logs");

    let logs = String::from_utf8(capture.0.lock().unwrap().clone()).unwrap();
    assert!(
        logs.contains("dial-logs"),
        "positive control: the capture saw the meeting"
    );
    for token in &tokens {
        for segment in token.split('.').skip(1) {
            assert!(
                !logs.contains(segment),
                "a token segment reached a log line"
            );
        }
    }
    for needle in [
        hex::encode(key),
        URL_SAFE_NO_PAD.encode(key),
        base64::engine::general_purpose::STANDARD.encode(key),
    ] {
        assert!(
            !logs.contains(&needle),
            "the public key is not logged either"
        );
    }
}
