//! Direct `MediaHandlerService` access for env-tests — authenticate as the
//! Kind meeting-controller and call `RegisterMeeting` / `EndMeeting` on ONE MH
//! pod through a per-test gRPC port-forward this fixture owns ([`handlers`]).
//!
//! # Discovery and the forward (story 4 T-0, ADR-0039 §7)
//!
//! Every MH a test talks to is found by label (`kube::live_mh_pods`,
//! `app=mh-service`) from the live pods — no fixed name, no fixed count. Each
//! [`Handler`] owns a `kubectl -n dark-tower port-forward --address 127.0.0.1
//! pod/<pod> :50053`, started per test and stopped (kill + wait) when the last
//! reference drops. The pod name the forward targets and the `pod_ip` a test
//! pins Prometheus by come from the SAME pod object.
//!
//! **The forward never binds a devloop-helper port-map port.** I-2 turns the
//! helper's MH offsets into MH hostPort slots, so this is a hard constraint, met
//! by construction rather than by a check:
//! - the local port is ONLY what kubectl reports for a `:50053` (local port 0)
//!   request — nothing in env-tests reads `ports.json`, a `HOST_PORT_MH_*` /
//!   `mh_N_grpc` key, or any `ENV_TEST_MH_*` variable;
//! - port 0 draws from the kernel's `ip_local_port_range` (32768-60999 measured
//!   in the devloop container), disjoint from the helper's allocation range
//!   (`crates/devloop-helper/src/ports.rs`: `PORT_RANGE_START` +
//!   `NUM_SLOTS * STRIDE`, 20000-29999 today); on the host those numbers are
//!   held by Kind, and the kernel never hands out a bound port.
//!
//! Deliberately no "not in the helper range" assertion: it would duplicate the
//! helper's single source and drift when I-2 changes it.
//!
//! **Accepted residual.** A SIGKILLed or aborted test binary never runs `Drop`,
//! so its kubectl children are orphaned until the container exits (a process
//! plus an apiserver stream). Port 0 means an orphan can never collide with a
//! later run's forward, so no reaper is needed. `PR_SET_PDEATHSIG` was rejected:
//! it fires on the death of the spawning THREAD, which could kill a live forward
//! created on a short-lived runtime thread.
//!
//! kubectl port-forward reaches the pod through the API server and kubelet, so it
//! BYPASSES the MH NetworkPolicy (which admits only MC to 50053) — dev-only, and a
//! separate fact from the channel being PLAINTEXT h2c (docs/TODO.md, §Media Path
//! Obligations). It is bound to 127.0.0.1 only.
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

use std::collections::VecDeque;
use std::ffi::OsStr;
use std::io::{BufRead, BufReader, Read};
use std::net::{Ipv4Addr, SocketAddr};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::cluster::tcp_accepts;
use crate::fixtures::auth_client::TokenRequest;
use crate::fixtures::deployment_config::LivePod;
use crate::fixtures::kube::{deployment_env_value, join_blocking, live_mh_pods, secret_key};
use crate::fixtures::AuthClient;
use crate::NAMESPACE;
use proto_gen::dark_tower::internal::v1::media_handler_service_client::MediaHandlerServiceClient;
use proto_gen::dark_tower::internal::v1::{
    EndMeetingRequest, RegisterMeetingRequest, ReleaseCause,
};
use tonic::metadata::MetadataValue;
use tonic::transport::{Channel, Endpoint};
use tonic::Request;

/// Where MC's own AC client credentials are deployed. The test READS them from
/// the cluster rather than restating them: the Secret is the single source (it
/// matches what `infra/kind/scripts/deploy.sh:seed_test_data` seeds into AC), and a copy here
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

/// MH's gRPC (`MediaHandlerService`) container port.
const MH_GRPC_PORT: u16 = 50053;

/// How long kubectl may take to print its `Forwarding from` line. A bound, never
/// a sleep: the wait ends at the line.
const FORWARD_LINE_BOUND: Duration = Duration::from_secs(30);

/// How long the forward may take to accept on its local port once announced.
const FORWARD_ACCEPT_BOUND: Duration = Duration::from_secs(30);

/// Lines of kubectl stdout / stderr kept for a failure message (bounded: kubectl
/// prints `Handling connection for 50053` on every connection).
const TAIL_LINES: usize = 20;

/// PURE: the local port from kubectl port-forward's announcement line,
/// `Forwarding from 127.0.0.1:<port> -> 50053`. Surrounding whitespace (a `\r`
/// line ending) is trimmed; anything else — the `[::1]` line, another remote
/// port, port 0, garbage — is refused, so the URL is never built from text
/// kubectl merely echoed.
///
/// # Errors
/// The line is not exactly that shape.
pub fn parse_forwarding_line(line: &str) -> Result<u16, String> {
    let malformed = || {
        format!(
            "mh-forward-malformed-line: expected `Forwarding from 127.0.0.1:<port> -> \
             {MH_GRPC_PORT}`, got {line:?}"
        )
    };
    let rest = line
        .trim()
        .strip_prefix("Forwarding from 127.0.0.1:")
        .ok_or_else(malformed)?;
    let (port, remote) = rest.split_once(" -> ").ok_or_else(malformed)?;
    if remote != MH_GRPC_PORT.to_string() {
        return Err(malformed());
    }
    match port.parse::<u16>() {
        Ok(p) if p != 0 => Ok(p),
        _ => Err(malformed()),
    }
}

/// A bounded tail of a child's output, shared with its drain thread.
type Tail = Arc<Mutex<VecDeque<String>>>;

fn push_tail(tail: &Tail, line: String) {
    let mut t = tail.lock().unwrap_or_else(|p| p.into_inner());
    if t.len() == TAIL_LINES {
        t.pop_front();
    }
    t.push_back(line);
}

fn show_tail(tail: &Tail) -> String {
    let t = tail.lock().unwrap_or_else(|p| p.into_inner());
    if t.is_empty() {
        "<none>".to_string()
    } else {
        t.iter().cloned().collect::<Vec<_>>().join(" | ")
    }
}

/// Drain `pipe` to EOF on a thread (an undrained pipe blocks kubectl), keeping a
/// bounded tail; the FIRST line also goes to `first` when given.
fn drain(
    pipe: impl Read + Send + 'static,
    tail: Tail,
    first: Option<mpsc::Sender<String>>,
) -> JoinHandle<()> {
    std::thread::spawn(move || {
        let mut first = first;
        for line in BufReader::new(pipe).lines() {
            let Ok(line) = line else { break };
            if let Some(tx) = first.take() {
                let _ = tx.send(line.clone());
            }
            push_tail(&tail, line);
        }
    })
}

/// The kubectl argv for one forward: namespace pinned, loopback only, local
/// port 0 (kubectl picks it; see the module doc's port-map constraint).
fn kubectl_forward_args(pod: &str) -> Vec<String> {
    vec![
        "-n".to_string(),
        NAMESPACE.to_string(),
        "port-forward".to_string(),
        "--address".to_string(),
        "127.0.0.1".to_string(),
        format!("pod/{pod}"),
        format!(":{MH_GRPC_PORT}"),
    ]
}

/// One fixture-owned `kubectl port-forward` to an MH pod's gRPC port. Stopped
/// (kill, then wait — no zombie) on `Drop`. Never held in a static: statics are
/// never dropped.
struct PortForward {
    child: Child,
    pod: String,
}

/// What a forward has captured while starting: the output tails and the drain
/// threads that fill them.
struct Capture {
    out: Tail,
    err: Tail,
    drains: Vec<JoinHandle<()>>,
}

impl PortForward {
    /// Start the forward and return it with its local port once it accepts.
    ///
    /// # Panics
    /// PRECONDITION, naming the pod, the elapsed time and kubectl's output:
    /// `mh-forward-spawn`, `mh-forward-no-line` (nothing announced within
    /// [`FORWARD_LINE_BOUND`], or EOF), `mh-forward-malformed-line`,
    /// `mh-forward-exited`, `mh-forward-not-accepting`.
    fn start(pod: &str) -> (Self, u16) {
        Self::start_program(OsStr::new("kubectl"), &kubectl_forward_args(pod), pod)
    }

    /// [`Self::start`] with the executed program as a parameter — the hermetic
    /// seam for this module's own lifecycle tests (not a config key: production
    /// always passes `kubectl`).
    fn start_program(program: &OsStr, args: &[String], pod: &str) -> (Self, u16) {
        let started = Instant::now();
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap_or_else(|e| {
                panic!(
                    "PRECONDITION (mh-forward-spawn): could not run kubectl port-forward to \
                     pod/{pod}: {e}"
                )
            });
        let (tx, rx) = mpsc::channel();
        let mut cap = Capture {
            out: Arc::default(),
            err: Arc::default(),
            drains: Vec::new(),
        };
        cap.drains.push(drain(
            child.stdout.take().expect("stdout is piped"),
            Arc::clone(&cap.out),
            Some(tx),
        ));
        cap.drains.push(drain(
            child.stderr.take().expect("stderr is piped"),
            Arc::clone(&cap.err),
            None,
        ));
        let mut forward = Self {
            child,
            pod: pod.to_string(),
        };
        let line = match rx.recv_timeout(FORWARD_LINE_BOUND) {
            Ok(line) => line,
            Err(e) => forward.fail(
                cap,
                &format!(
                    "PRECONDITION (mh-forward-no-line): kubectl port-forward to pod/{pod} \
                     announced no local port after {:?} ({e})",
                    started.elapsed()
                ),
            ),
        };
        let port = match parse_forwarding_line(&line) {
            Ok(port) => port,
            Err(e) => forward.fail(
                cap,
                &format!("PRECONDITION (mh-forward-malformed-line): forward to pod/{pod}: {e}"),
            ),
        };
        let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
        let announced = Instant::now();
        loop {
            // Liveness FIRST: a forward that exited fails fast, not at the bound.
            match forward.child.try_wait() {
                Ok(Some(status)) => forward.fail(
                    cap,
                    &format!(
                        "PRECONDITION (mh-forward-exited): kubectl port-forward to pod/{pod} \
                         exited ({status}) after {:?}, before accepting on {addr}",
                        started.elapsed()
                    ),
                ),
                Err(e) => forward.fail(
                    cap,
                    &format!(
                        "PRECONDITION (mh-forward-exited): cannot poll kubectl port-forward to \
                         pod/{pod}: {e}"
                    ),
                ),
                Ok(None) => {}
            }
            if tcp_accepts(&addr, Duration::from_secs(1)).is_ok() {
                break;
            }
            if announced.elapsed() >= FORWARD_ACCEPT_BOUND {
                forward.fail(
                    cap,
                    &format!(
                        "PRECONDITION (mh-forward-not-accepting): kubectl port-forward to \
                         pod/{pod} announced {addr} but did not accept within \
                         {FORWARD_ACCEPT_BOUND:?} ({:?} since spawn)",
                        started.elapsed()
                    ),
                );
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        (forward, port)
    }

    /// Fail loudly with the COMPLETE captured output: stop the child first
    /// (kill + wait), so both pipes close and the drains finish, then join them
    /// — otherwise a kubectl that printed its error and exited could be
    /// reported as `stderr: <none>`.
    fn fail(mut self, cap: Capture, head: &str) -> ! {
        let _ = self.child.kill();
        let _ = self.child.wait();
        for d in cap.drains {
            let _ = d.join();
        }
        panic!(
            "{head}. kubectl stdout: {}; stderr: {}",
            show_tail(&cap.out),
            show_tail(&cap.err)
        )
    }
}

impl Drop for PortForward {
    fn drop(&mut self) {
        // Loud, but never a panic inside Drop. Kill and wait on an already
        // reaped child are no-ops; a real failure is reported.
        if let Err(e) = self.child.kill() {
            eprintln!(
                "mh-forward: kill of the forward to pod/{} failed: {e}",
                self.pod
            );
        }
        if let Err(e) = self.child.wait() {
            eprintln!(
                "mh-forward: wait for the forward to pod/{} failed: {e}",
                self.pod
            );
        }
    }
}

/// One live MH pod, reachable through this test's own forward.
pub struct Handler {
    /// The pod name (no instance naming assumed).
    pub name: String,
    /// `http://127.0.0.1:<local port>` of this handler's forward.
    pub grpc_url: String,
    /// The pod IP, for pinning Prometheus reads to this pod's `instance`.
    pub pod_ip: String,
    /// Keeps the forward alive while any holder (this handler, a pending
    /// [`ReleaseOnDrop`] release) still needs it.
    forward: Arc<PortForward>,
}

/// EVERY live MH pod (`kube::live_mh_pods`: at least one, none Terminating or
/// not Ready — refused, never a subset), each with its own fixture-owned forward.
/// Owned per test; the forwards stop when the handlers (and any pending release)
/// drop.
///
/// The forwards start CONCURRENTLY on the blocking pool, never on the test's
/// (current-thread) runtime thread, so live sessions keep being driven while
/// they come up.
///
/// A test that kills an MH must WAIT for its replacement to be Ready before
/// anything resolves handlers again — do not loosen the discovery rule instead.
///
/// # Panics
/// PRECONDITION from discovery (`mh-pods-unserving`) or from a forward
/// ([`PortForward::start`]), with its own text.
pub async fn handlers() -> Vec<Handler> {
    let starting: Vec<_> = live_mh_pods()
        .await
        .into_iter()
        .map(|pod| {
            tokio::task::spawn_blocking(move || {
                let started = Instant::now();
                let (forward, port) = PortForward::start(&pod.name);
                (pod, forward, port, started.elapsed())
            })
        })
        .collect();
    let mut handlers = Vec::with_capacity(starting.len());
    for task in starting {
        let (pod, forward, port, took) = join_blocking(task.await);
        eprintln!(
            "mh-forward: pod={} ip={} local=127.0.0.1:{port} ready_in={}ms",
            pod.name,
            pod.ip,
            took.as_millis()
        );
        handlers.push(Handler {
            grpc_url: format!("http://127.0.0.1:{port}"),
            name: pod.name,
            pod_ip: pod.ip,
            forward: Arc::new(forward),
        });
    }
    handlers
}

/// The Prometheus pins of a test's OWN handlers — the same lookup that chose
/// the forwards, so injection and pin can never disagree about the pod set.
#[must_use]
pub fn pins(handlers: &[Handler]) -> Vec<LivePod> {
    handlers
        .iter()
        .map(|h| LivePod {
            instance: None,
            name: h.name.clone(),
            ip: h.pod_ip.clone(),
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
        .expect("the forward URL parses")
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(10))
        .connect()
        .await
        .unwrap_or_else(|e| {
            panic!(
                "PRECONDITION: cannot connect to pod {}'s gRPC through this test's forward at {} \
                 ({e}). The forward accepted TCP, which only proves kubectl is listening; the \
                 handshake is the real readiness check. A forward dies with its pod: an MH \
                 restart during the suite lands here",
                handler.name, handler.grpc_url
            )
        });
    MediaHandlerServiceClient::new(channel)
}

/// An MC gRPC endpoint that never ANSWERS. Syntactically valid so it passes
/// MH's scheme check; `.invalid` is reserved, so it cannot resolve by accident.
/// A probe registration names it so no real MC is ever told about the probe:
/// a probe has no participants, so MH never notifies about connections there.
/// MH DOES heartbeat it once per [`PROBE_HEARTBEAT_INTERVAL_MS`] (story 4
/// MH-2: every registered meeting heartbeats its registered MC); each beat
/// fails and is counted `mh_mc_heartbeats_total{outcome="error"}` (or
/// `timeout`), and MH takes NO action on it — MH never ends a meeting because
/// MC is unreachable. So a probe stays held until its own `EndMeeting`.
pub const NEVER_DIALLED_MC_ENDPOINT: &str = "http://env-test-mc.invalid:50052";

/// The `heartbeat_interval_ms` a PROBE registration carries (story 4 MH-2: 0
/// is a record reject). Probes name [`NEVER_DIALLED_MC_ENDPOINT`], so nothing
/// couples this to MC's N: it only paces the failing beats. An injection on a
/// REAL MC's meeting must use MC's own interval instead
/// ([`crate::fixtures::metrics::mc_heartbeat_interval`]). Never 1 ms.
pub const PROBE_HEARTBEAT_INTERVAL_MS: u32 = 2_000;

/// Seed of the fixture `media_dial` keypair (test-only key material).
const FIXTURE_DIAL_SEED: [u8; 32] = [0x5d; 32];

/// The ONE `media_dial` PUBLIC key every registration this suite injects
/// directly into MH carries (`RegisterMeetingRequest.media_dial_public_key`,
/// exactly 32 raw Ed25519 bytes, else MH refuses the registration —
/// ADR-0039 §5). A real Ed25519 public key from a fixed test seed.
///
/// **A direct registration REPLACES the meeting's dial key on that handler**
/// (the last record-valid registration's key wins). For a probe meeting no
/// client ever dials, that is moot. For an injection on a REAL MC's meeting
/// ([`crate::fixtures::injection`]) it means: sessions opened BEFORE the
/// injection are unaffected (admission already happened), but a NEW dial to
/// that handler with an MC-minted token is DECLINED (`signature_invalid`)
/// until MC re-registers. Every current suite opens its sessions before
/// injecting; a suite that must dial after an injection needs a token signed
/// by this fixture key.
#[must_use]
pub fn fixture_dial_public_key() -> Vec<u8> {
    use ring::signature::{Ed25519KeyPair, KeyPair};
    Ed25519KeyPair::from_seed_unchecked(&FIXTURE_DIAL_SEED)
        .unwrap_or_else(|e| panic!("fixture Ed25519 seed rejected: {e}"))
        .public_key()
        .as_ref()
        .to_vec()
}

/// A PROBE registration: `meeting_id` at `generation` as `mc_id`, with NO
/// edges and NO declared source streams.
///
/// Hoisted at its second consumer (tests 29 and 35) so both probes are the
/// same decision under the same policy:
///
/// - **Empty `egress_streams`, deliberately.** Edges would make the probe
///   depend on FREE SHARED CAPACITY: MH's egress admission refuses a
///   registration whole when the projected installed streams exceed the pod's
///   ceiling, and suites share these pods. With no edges the projection is
///   unchanged by the probe, so the ceiling can never refuse it. Do not
///   "strengthen" a probe by giving it edges.
/// - **Never generation 0.** A gen-0 registration installs nothing and proves
///   nothing.
/// - **A probe TAKES OWNERSHIP of the meeting** (MH rebinds `mc_id` on any
///   validation-passing registration, by design, for failover). Never probe a
///   meeting some real MC is still using, and always release the probe under
///   its own `mc_id` (see [`ReleaseOnDrop`]).
#[must_use]
pub fn probe_registration(
    meeting_id: &str,
    mc_id: &str,
    generation: u64,
) -> RegisterMeetingRequest {
    assert_ne!(
        generation, 0,
        "a gen-0 probe installs nothing and proves nothing"
    );
    RegisterMeetingRequest {
        meeting_id: meeting_id.to_string(),
        mc_id: mc_id.to_string(),
        mc_grpc_endpoint: NEVER_DIALLED_MC_ENDPOINT.to_string(),
        egress_streams: Vec::new(),
        selection_rules: None,
        policy_generation: generation,
        source_streams: Vec::new(),
        heartbeat_interval_ms: PROBE_HEARTBEAT_INTERVAL_MS,
        media_dial_public_key: fixture_dial_public_key(),
    }
}

/// A meeting this test registered: where, and under which `mc_id`.
struct Owned {
    grpc_url: String,
    meeting_id: String,
    mc_id: String,
    /// Keeps the handler's forward alive until this release has run (moved
    /// into the release thread with the rest).
    _forward: Arc<PortForward>,
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

    /// Record a meeting registered on `handler`. Holds that handler's forward
    /// until the release runs, so a release never dials a dropped forward.
    pub fn record(&self, handler: &Handler, meeting_id: &str, mc_id: &str) {
        self.owned.lock().expect("cleanup registry").push(Owned {
            grpc_url: handler.grpc_url.clone(),
            meeting_id: meeting_id.to_string(),
            mc_id: mc_id.to_string(),
            _forward: Arc::clone(&handler.forward),
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
                                    cause: ReleaseCause::MeetingEnded as i32,
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    /// Every direct registration carries a real, deterministic 32-byte
    /// Ed25519 public key (MH refuses any other length, story 4 MH-1).
    #[test]
    fn every_probe_carries_the_fixture_dial_key() {
        let key = fixture_dial_public_key();
        assert_eq!(key.len(), 32);
        assert_eq!(key, fixture_dial_public_key(), "deterministic");
        assert_ne!(
            key,
            FIXTURE_DIAL_SEED.to_vec(),
            "the public half, never the seed"
        );
        assert_eq!(probe_registration("m", "mc", 1).media_dial_public_key, key);
    }

    #[test]
    fn the_ipv4_forwarding_line_yields_its_local_port() {
        assert_eq!(
            parse_forwarding_line("Forwarding from 127.0.0.1:41234 -> 50053"),
            Ok(41234)
        );
    }

    #[test]
    fn a_trailing_cr_or_whitespace_is_trimmed_and_accepted() {
        assert_eq!(
            parse_forwarding_line("Forwarding from 127.0.0.1:41234 -> 50053\r"),
            Ok(41234)
        );
        assert_eq!(
            parse_forwarding_line("  Forwarding from 127.0.0.1:41234 -> 50053 \n"),
            Ok(41234)
        );
    }

    #[test]
    fn every_other_shape_is_refused_as_malformed() {
        for line in [
            "Forwarding from [::1]:41234 -> 50053",
            "Forwarding from 127.0.0.1:41234 -> 8083",
            "Forwarding from 127.0.0.1:0 -> 50053",
            "Forwarding from 127.0.0.1:99999 -> 50053",
            "Forwarding from 127.0.0.1:abc -> 50053",
            "Forwarding from 127.0.0.1:41234",
            "Forwarding from 0.0.0.0:41234 -> 50053",
            "Handling connection for 50053",
            "error: unable to forward port because pod is not running",
            "",
        ] {
            let err = parse_forwarding_line(line).unwrap_err();
            assert!(
                err.starts_with("mh-forward-malformed-line:"),
                "{line:?} -> {err}"
            );
        }
    }

    // ---- forward lifecycle, through the program seam (`sh` stands in) -----

    fn sh(script: &str, arg: &str) -> (OsString, Vec<String>) {
        (
            OsString::from("sh"),
            vec!["-c".to_string(), script.to_string(), arg.to_string()],
        )
    }

    fn panic_text(f: impl FnOnce() + std::panic::UnwindSafe) -> String {
        let payload = std::panic::catch_unwind(f).expect_err("must fail loudly");
        payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_string()))
            .unwrap_or_default()
    }

    #[test]
    fn an_announced_accepting_forward_returns_its_port_and_is_reaped_on_drop() {
        // The test holds the listener the fake "forward" announces.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let (prog, args) = sh(
            r#"echo "Forwarding from 127.0.0.1:$0 -> 50053"; exec sleep 300"#,
            &port.to_string(),
        );
        let (forward, got) = PortForward::start_program(&prog, &args, "fake-pod");
        assert_eq!(got, port);
        let pid = forward.child.id();
        assert!(std::path::Path::new(&format!("/proc/{pid}")).exists());
        drop(forward);
        assert!(
            !std::path::Path::new(&format!("/proc/{pid}")).exists(),
            "Drop must kill AND wait (no running child, no zombie)"
        );
    }

    #[test]
    fn a_kubectl_that_errors_and_exits_is_no_line_with_its_stderr() {
        let (prog, args) = sh("echo 'error: boom' >&2; exit 1", "");
        let text = panic_text(move || {
            let _ = PortForward::start_program(&prog, &args, "fake-pod");
        });
        assert!(text.contains("PRECONDITION (mh-forward-no-line)"), "{text}");
        assert!(text.contains("pod/fake-pod"), "{text}");
        assert!(
            text.contains("error: boom"),
            "stderr must be captured: {text}"
        );
    }

    #[test]
    fn a_garbage_first_line_is_malformed() {
        let (prog, args) = sh(
            "echo 'Forwarding from [::1]:1234 -> 50053'; exec sleep 300",
            "",
        );
        let text = panic_text(move || {
            let _ = PortForward::start_program(&prog, &args, "fake-pod");
        });
        assert!(
            text.contains("PRECONDITION (mh-forward-malformed-line)"),
            "{text}"
        );
    }

    #[test]
    fn a_forward_that_exits_after_announcing_fails_fast_as_exited() {
        // A port nothing listens on: bind, read, release.
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .and_then(|l| l.local_addr())
            .expect("free port")
            .port();
        let (prog, args) = sh(
            r#"echo "Forwarding from 127.0.0.1:$0 -> 50053"; echo 'lost pod' >&2; exit 3"#,
            &port.to_string(),
        );
        let started = Instant::now();
        let text = panic_text(move || {
            let _ = PortForward::start_program(&prog, &args, "fake-pod");
        });
        assert!(text.contains("PRECONDITION (mh-forward-exited)"), "{text}");
        assert!(text.contains("lost pod"), "{text}");
        assert!(
            started.elapsed() < FORWARD_ACCEPT_BOUND,
            "an exited forward must fail fast, not at the accept bound"
        );
    }
}
