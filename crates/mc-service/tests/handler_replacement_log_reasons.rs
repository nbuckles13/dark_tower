//! The replacement driver's terminal log line carries the REMOVAL's reason
//! (story 4 MC-C, observability ruling on `admission_refused`): a grant
//! removed while still Registering logs `reason="declared"` when a
//! declaration removed it — telling it apart from a failed first
//! RegisterMeeting (`initial_registration_failed`). Ids and bounded labels
//! only.
//!
//! Its own test binary ON PURPOSE: it installs a thread-local TRACE
//! subscriber, and tracing's callsite interest is process-global, so a
//! concurrent test in the same binary could blind the capture (the positive
//! control below would catch that, as a flake).

#![cfg(feature = "test-seams")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "common/mod.rs"]
mod test_common;

use std::sync::{Arc, Mutex};

use mc_service::grpc::gc_client::AcquireResult;
use mc_service::media_routing::{HandlerEndpoint, HandlerId, RemovalTrigger};
use mc_test_utils::ScriptedGc;
use test_common::{
    advance_live, handler_set, heartbeat_timing, join_media_with_gc, mh_handler, run_ready,
    MockMhRegistrationClient,
};

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

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_grant_declared_while_registering_logs_reason_declared() {
    let capture = Capture::default();
    let sink = capture.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .with_writer(move || sink.clone())
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);
    tracing::callsite::rebuild_interest_cache();

    let c = Arc::new(test_common::make_controller("mc-log-reasons"));
    let mh = Arc::new(MockMhRegistrationClient::new());
    mh.heartbeat_to_controller(Arc::clone(&c));
    let gc = Arc::new(ScriptedGc::with_cap(16));
    let info = mh_handler("mh-3");
    gc.push(AcquireResult::Granted(HandlerEndpoint {
        id: HandlerId::new(info.mh_id.clone()),
        webtransport_url: info.webtransport_endpoint.clone(),
        grpc_endpoint: info.grpc_endpoint.clone(),
    }));
    mh.hold_handler("mh-3");
    c.create_meeting(
        "m-log".to_string(),
        handler_set(&[mh_handler("mh-1"), mh_handler("mh-2")]),
    )
    .await
    .unwrap();
    let meeting = c.get_meeting_handle("m-log".to_string()).await.unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel::<bytes::Bytes>(256);
    let reply = c
        .join_connection(
            "m-log".to_string(),
            "conn-a".to_string(),
            "user-a".to_string(),
            "a".to_string(),
            String::new(),
            false,
            test_common::test_identity_key(),
            tx,
            join_media_with_gc(Arc::clone(&mh), Arc::clone(&gc)),
        )
        .await
        .unwrap();
    reply.await.unwrap().unwrap();
    run_ready().await;

    mh.silence("mh-1");
    advance_live(heartbeat_timing().timeout() + heartbeat_timing().interval()).await;
    gc.wait_for_calls(1, heartbeat_timing().timeout()).await;
    run_ready().await;
    assert!(meeting
        .remove_handler_for_test("mh-3", RemovalTrigger::ApplyLag)
        .await
        .unwrap());
    run_ready().await;

    let logs = String::from_utf8(capture.0.lock().unwrap().clone()).unwrap();
    let ended: Vec<&str> = logs
        .lines()
        .filter(|l| l.contains("Media handler replacement attempt ended"))
        .collect();
    assert!(
        logs.contains("Media handler migration started"),
        "positive control: the capture saw the driver"
    );
    let refused = ended
        .iter()
        .find(|l| l.contains("admission_refused"))
        .unwrap_or_else(|| panic!("no admission_refused terminal line in {ended:?}"));
    assert!(refused.contains(r#"reason="declared""#), "{refused}");
    assert!(
        refused.contains("mh-3") && refused.contains("m-log"),
        "ids in the log: {refused}"
    );
    assert!(
        logs.lines().any(
            |l| l.contains("removed before its first RegisterMeeting ack")
                && l.contains(r#"removal_trigger="apply_lag""#)
        ),
        "the removal's trigger is logged"
    );
    c.cancel();
}
