//! Direct `MediaHandlerService` access for env-tests — authenticate as the
//! Kind meeting-controller and call `RegisterMeeting` / `EndMeeting` on ONE MH
//! pod through Layer 7's per-pod gRPC port-forward (`scripts/layer7.sh`,
//! step (i): `ENV_TEST_MH_{0,1}_GRPC_URL`, `ENV_TEST_MH_{0,1}_POD_IP`).
//!
//! Hoisted from `tests/29_mh_meeting_teardown.rs` at story 2 task 10, when
//! `tests/26_mh_quic.rs` (server-mute S4, edge churn) became its second
//! consumer.
//!
//! # This is a deliberate exercise of an ownership gap — and stays TEST-ONLY
//!
//! `RegisterMeeting` authorizes on scope alone (`service.write.mh`), so any
//! holder of that credential can install MH forwarding policy, including a
//! server mute. When a caller uses the meeting's REAL `mc_id` (as S4 does, so
//! MH's sender-binding notifications still reach the real MC), it also evades
//! the one detection control that exists — the register-path takeover WARN
//! (`crates/mh-service/src/session/mod.rs`, `Ownership`). The bound on who can
//! do this is recorded in `docs/TODO.md` §Media Path Obligations and is not
//! restated here. What keeps story 2's "no operator-facing API or tooling"
//! statement true is that this helper is a TEST-TARGET fixture: it must never
//! become a dependency of a service crate, and never sit behind a bin/CLI
//! entry point.
//!
//! The bearer token is read from AC with the credentials MC itself is deployed
//! with, and it must never reach an assertion message, a panic payload or a
//! log line — every failure message below names the credential's SOURCE, never
//! its value.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::fixtures::auth_client::TokenRequest;
use crate::fixtures::kube::{deployment_env_value, secret_key};
use crate::fixtures::AuthClient;
use proto_gen::dark_tower::internal::v1::media_handler_service_client::MediaHandlerServiceClient;
use proto_gen::dark_tower::internal::v1::EndMeetingRequest;
use tonic::metadata::MetadataValue;
use tonic::transport::{Channel, Endpoint};
use tonic::Request;

/// Where MC's own AC client credentials are deployed. The test READS them from
/// the cluster rather than restating them: the Secret is the single source (it
/// matches what `infra/kind/scripts/setup.sh` seeds into AC), and a copy here
/// would drift on rotation and then fail as an apparent environment fault.
const MC_DEPLOYMENT: &str = "mc-0";
const MC_CLIENT_ID_ENV: &str = "MC_CLIENT_ID";
const MC_SECRET: &str = "mc-service-secrets";
const MC_SECRET_KEY: &str = "MC_CLIENT_SECRET";
/// Narrowed to the one scope MH's gate needs; the dev client also holds
/// `service.write.gc`, which this test has no use for.
const MC_SCOPE: &str = "service.write.mh";

/// The AC-issued meeting-controller token, narrowed to [`MC_SCOPE`], from the
/// credentials MC itself is deployed with — never minted locally. A refusal
/// names the CREDENTIAL, so a rotation mismatch is not triaged as a broken
/// forward. The token itself is never printed.
///
/// # Panics
///
/// PRECONDITION, when AC refuses the deployed credentials.
pub async fn mc_service_token(ac_base_url: &str) -> String {
    let client_id = deployment_env_value(MC_DEPLOYMENT, MC_CLIENT_ID_ENV);
    let client_secret = secret_key(MC_SECRET, MC_SECRET_KEY);
    AuthClient::new(ac_base_url)
        .issue_token(TokenRequest::client_credentials(
            &client_id,
            client_secret,
            MC_SCOPE,
        ))
        .await
        .unwrap_or_else(|e| {
            panic!(
                "PRECONDITION: AC refused MC's deployed client credentials ({client_id}, secret \
                 from {MC_SECRET}/{MC_SECRET_KEY}) — a CREDENTIAL fault: the deployed Secret and \
                 what AC was seeded with disagree. Not a forward or teardown fault. {e}"
            )
        })
        .access_token
}

/// Read a Layer-7-exported variable, never skipping.
pub fn required_env(key: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| {
        panic!(
            "PRECONDITION: {key} is unset. Layer 7 (scripts/layer7.sh, step (i)) starts the \
             per-pod MH gRPC port-forwards and exports it; run this suite through Layer 7. \
             This test is never skipped."
        )
    })
}

/// One MH pod as Layer 7 exposes it.
pub struct Handler {
    /// `mh-0` / `mh-1`.
    pub name: String,
    /// The Layer-7 per-pod gRPC forward.
    pub grpc_url: String,
    /// The pod IP, for pinning Prometheus reads to this pod's `instance`.
    pub pod_ip: String,
}

pub fn handlers() -> Vec<Handler> {
    (0..2)
        .map(|n| Handler {
            name: format!("mh-{n}"),
            grpc_url: required_env(&format!("ENV_TEST_MH_{n}_GRPC_URL")),
            pod_ip: required_env(&format!("ENV_TEST_MH_{n}_POD_IP")),
        })
        .collect()
}

pub fn authed<T>(token: &str, message: T) -> Request<T> {
    let mut request = Request::new(message);
    let value: MetadataValue<_> = format!("Bearer {token}")
        .parse()
        .expect("a bearer header parses");
    request.metadata_mut().insert("authorization", value);
    request
}

pub async fn connect(handler: &Handler) -> MediaHandlerServiceClient<Channel> {
    let channel = Endpoint::from_shared(handler.grpc_url.clone())
        .expect("the exported URL parses")
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(10))
        .connect()
        .await
        .unwrap_or_else(|e| {
            panic!(
                "PRECONDITION: cannot connect to {}'s gRPC at {} ({e}) — the Layer-7 \
                 port-forward is down (a forward dies with its pod)",
                handler.name, handler.grpc_url
            )
        });
    MediaHandlerServiceClient::new(channel)
}

/// A meeting this test registered: where, and under which `mc_id`.
struct Owned {
    grpc_url: String,
    meeting_id: String,
    mc_id: String,
}

/// Releases every meeting still recorded here when dropped — on success AND on
/// a panic mid-test. Recorded BEFORE each registration call, so even a
/// registration whose response was lost is released. `Drop` cannot be async,
/// so it drives the releases on a fresh thread with its own runtime.
pub struct ReleaseOnDrop {
    token: String,
    owned: Arc<Mutex<Vec<Owned>>>,
}

impl ReleaseOnDrop {
    /// A guard releasing with `token` (never printed).
    #[must_use]
    pub fn new(token: String) -> Self {
        Self {
            token,
            owned: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn record(&self, grpc_url: &str, meeting_id: &str, mc_id: &str) {
        self.owned.lock().expect("cleanup registry").push(Owned {
            grpc_url: grpc_url.to_string(),
            meeting_id: meeting_id.to_string(),
            mc_id: mc_id.to_string(),
        });
    }

    /// The test released it itself; nothing left to clean up.
    pub fn forget(&self, meeting_id: &str) {
        self.owned
            .lock()
            .expect("cleanup registry")
            .retain(|o| o.meeting_id != meeting_id);
    }
}

impl Drop for ReleaseOnDrop {
    fn drop(&mut self) {
        let owned: Vec<Owned> =
            std::mem::take(&mut *self.owned.lock().unwrap_or_else(|p| p.into_inner()));
        if owned.is_empty() {
            return;
        }
        let token = self.token.clone();
        let released = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("cleanup runtime");
            runtime.block_on(async move {
                for o in owned {
                    let result = async {
                        let channel = Endpoint::from_shared(o.grpc_url.clone())
                            .map_err(|e| e.to_string())?
                            .connect_timeout(Duration::from_secs(5))
                            .timeout(Duration::from_secs(10))
                            .connect()
                            .await
                            .map_err(|e| e.to_string())?;
                        MediaHandlerServiceClient::new(channel)
                            .end_meeting(authed(
                                &token,
                                EndMeetingRequest {
                                    meeting_id: o.meeting_id.clone(),
                                    mc_id: o.mc_id.clone(),
                                },
                            ))
                            .await
                            .map_err(|s| format!("{:?}: {}", s.code(), s.message()))
                    }
                    .await;
                    // Loud, but never a panic inside Drop (it would abort a test
                    // that is already unwinding). The token is never printed.
                    if let Err(e) = result {
                        eprintln!(
                            "CLEANUP: could not release test meeting {} at {}: {e} — it holds a \
                             registration (and, at a high generation, wedges that id) until that \
                             pod restarts",
                            o.meeting_id, o.grpc_url
                        );
                    }
                }
            });
        })
        .join();
        if released.is_err() {
            eprintln!("CLEANUP: the release thread panicked; test meetings may be leaked");
        }
    }
}
