//! Mock Media Handler for MC testing.
//!
//! Two distinct things live here, and the split is deliberate:
//!
//! * [`MockMh`] — a plain **data holder** describing an MH's advertised
//!   capacity. No gRPC, no server, no behaviour.
//! * [`MediaHandlerStub`] — a real, configurable **tonic
//!   `MediaHandlerService`** MC's component tests dial over the wire.
//!
//! # Why the stub is here rather than copied into each test
//!
//! Three MC test files drive `MhClient::register_meeting` against a stub MH:
//! `register_meeting_integration.rs`, `otel_grpc_outbound_integration.rs`, and
//! `media_policy_push_integration.rs`. All three need the same
//! applied-generation echo semantics, and those semantics are the thing under
//! test — ADR-0036 §8's "`applied_generation` is what the live forward path
//! reflects, never an echo of the request". Three private copies of an echo rule
//! inside the very fixture whose job is to catch an echo lie is exactly the
//! duplication that goes wrong quietly.
//!
//! # The echo is a knob, and its default is the honest one
//!
//! [`AppliedGenerationBehaviour::EchoSent`] must be selected **deliberately**.
//! A fixture that echoed the received generation by default would hand every
//! success test a green result validating the precise anti-pattern §8's check
//! exists to catch, so the default is [`AppliedGenerationBehaviour::ReportZero`]
//! — a handler that installed nothing and says so.

use proto_gen::dark_tower::internal::v1::media_handler_service_server::{
    MediaHandlerService, MediaHandlerServiceServer,
};
use proto_gen::dark_tower::internal::v1::{RegisterMeetingRequest, RegisterMeetingResponse};
use proto_gen::dark_tower::signaling::v1::TransportMode;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tonic::{Request, Response, Status};

/// What the stub reports in `applied_generation`.
///
/// ADR-0036 §8: this field is the generation MH's **live forward path actually
/// reflects** — never the highest received, never an echo of the request. The
/// variants below are the three shapes MC's confirm logic must tell apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AppliedGenerationBehaviour {
    /// Report `0`: nothing is installed for this meeting.
    ///
    /// **The default, deliberately.** It is the truthful answer for a handler
    /// that applied nothing, and it means a test must opt in to the
    /// echo-the-sent-value behaviour rather than inherit it.
    #[default]
    ReportZero,
    /// Report exactly the `policy_generation` the request carried.
    ///
    /// The success shape: only select this where the test's subject is a
    /// genuinely-programmed handler.
    EchoSent,
    /// Report a fixed value regardless of what was sent.
    ///
    /// The **injected apply failure**: MH received a newer generation, failed to
    /// install it, and correctly continues to report the older one its forward
    /// path still reflects. Use `Stall(0)` for "never programmed" and
    /// `Stall(n)` for "stalled at n".
    Stall(u64),
}

/// A configurable tonic `MediaHandlerService` for MC component tests.
///
/// Build with [`MediaHandlerStub::builder`], then [`MediaHandlerStubBuilder::spawn`]
/// to bind an ephemeral port and serve.
pub struct MediaHandlerStub {
    accept: bool,
    applied_generation: AppliedGenerationBehaviour,
    handler_id: String,
    transport_mode: TransportMode,
    process_start_epoch_ms: u64,
    /// Every `policy_generation` the stub has been sent, in order.
    received_generations: Arc<Mutex<Vec<u64>>>,
    /// Number of `RegisterMeeting` calls served.
    call_count: Arc<AtomicU64>,
    /// The `egress_streams` of the most recent request, so a test can assert
    /// what MC actually put on the wire.
    last_request: Arc<Mutex<Option<RegisterMeetingRequest>>>,
}

impl MediaHandlerStub {
    /// Start building a stub.
    #[must_use]
    pub fn builder() -> MediaHandlerStubBuilder {
        MediaHandlerStubBuilder::default()
    }
}

/// Handle to a running [`MediaHandlerStub`].
#[derive(Clone)]
pub struct MediaHandlerStubHandle {
    /// Address the stub is listening on.
    pub addr: SocketAddr,
    received_generations: Arc<Mutex<Vec<u64>>>,
    call_count: Arc<AtomicU64>,
    last_request: Arc<Mutex<Option<RegisterMeetingRequest>>>,
}

impl MediaHandlerStubHandle {
    /// `http://<addr>`, ready to hand to `MhClient`.
    #[must_use]
    pub fn endpoint(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// Every `policy_generation` received, in order.
    #[must_use]
    pub fn received_generations(&self) -> Vec<u64> {
        self.received_generations
            .lock()
            .expect("MediaHandlerStub mutex poisoned")
            .clone()
    }

    /// How many `RegisterMeeting` calls the stub has served.
    #[must_use]
    pub fn call_count(&self) -> u64 {
        self.call_count.load(Ordering::Relaxed)
    }

    /// The most recent request, for asserting what MC put on the wire.
    #[must_use]
    pub fn last_request(&self) -> Option<RegisterMeetingRequest> {
        self.last_request
            .lock()
            .expect("MediaHandlerStub mutex poisoned")
            .clone()
    }
}

/// Builder for [`MediaHandlerStub`].
#[derive(Debug)]
pub struct MediaHandlerStubBuilder {
    accept: bool,
    applied_generation: AppliedGenerationBehaviour,
    handler_id: String,
    transport_mode: TransportMode,
    process_start_epoch_ms: u64,
}

impl Default for MediaHandlerStubBuilder {
    fn default() -> Self {
        Self {
            accept: true,
            // See `AppliedGenerationBehaviour::ReportZero`: the honest default,
            // so echoing the sent value is always a deliberate act.
            applied_generation: AppliedGenerationBehaviour::default(),
            handler_id: String::new(),
            transport_mode: TransportMode::Unspecified,
            process_start_epoch_ms: 0,
        }
    }
}

impl MediaHandlerStubBuilder {
    /// Set `accepted` on the response.
    ///
    /// Note MC does **not** treat `accepted == true` as success (ADR-0036 §8),
    /// so this knob alone cannot make a push confirm.
    #[must_use]
    pub fn accept(mut self, accept: bool) -> Self {
        self.accept = accept;
        self
    }

    /// Choose what the stub reports in `applied_generation`.
    #[must_use]
    pub fn applied_generation(mut self, behaviour: AppliedGenerationBehaviour) -> Self {
        self.applied_generation = behaviour;
        self
    }

    /// Set the `handler_id` the stub asserts in its reply.
    #[must_use]
    pub fn handler_id(mut self, handler_id: impl Into<String>) -> Self {
        self.handler_id = handler_id.into();
        self
    }

    /// Set the transport mode the stub echoes.
    #[must_use]
    pub fn transport_mode(mut self, transport_mode: TransportMode) -> Self {
        self.transport_mode = transport_mode;
        self
    }

    /// Set the process-start epoch the stub reports.
    #[must_use]
    pub fn process_start_epoch_ms(mut self, epoch_ms: u64) -> Self {
        self.process_start_epoch_ms = epoch_ms;
        self
    }

    /// Convenience: a handler that genuinely programs what MC sends.
    ///
    /// Echoes the sent generation, asserts `handler_id`, and echoes datagram —
    /// i.e. every field MC's confirm compares agrees.
    #[must_use]
    pub fn programs_successfully(self, handler_id: impl Into<String>) -> Self {
        self.accept(true)
            .applied_generation(AppliedGenerationBehaviour::EchoSent)
            .handler_id(handler_id)
            .transport_mode(TransportMode::Datagram)
    }

    /// Build the bare tonic service without serving it.
    ///
    /// For the one caller that must wrap the service in its own
    /// `MediaHandlerServiceServer::with_interceptor(..)` — MC's outbound-OTel
    /// test, which asserts on the `traceparent` metadata the interceptor
    /// captures. Everything else wants [`Self::spawn`].
    #[must_use]
    pub fn build(self) -> MediaHandlerStub {
        MediaHandlerStub {
            accept: self.accept,
            applied_generation: self.applied_generation,
            handler_id: self.handler_id,
            transport_mode: self.transport_mode,
            process_start_epoch_ms: self.process_start_epoch_ms,
            received_generations: Arc::new(Mutex::new(Vec::new())),
            call_count: Arc::new(AtomicU64::new(0)),
            last_request: Arc::new(Mutex::new(None)),
        }
    }

    /// Bind an ephemeral port, serve the stub, and return a handle.
    ///
    /// The server task runs until the process ends; component tests are
    /// short-lived, so there is no shutdown token to thread.
    ///
    /// # Panics
    ///
    /// Panics if the ephemeral port cannot be bound — a test-utility failure
    /// that must fail the test loudly rather than be reported as a connect
    /// error later.
    pub async fn spawn(self) -> MediaHandlerStubHandle {
        let received_generations = Arc::new(Mutex::new(Vec::new()));
        let call_count = Arc::new(AtomicU64::new(0));
        let last_request = Arc::new(Mutex::new(None));

        let stub = MediaHandlerStub {
            accept: self.accept,
            applied_generation: self.applied_generation,
            handler_id: self.handler_id,
            transport_mode: self.transport_mode,
            process_start_epoch_ms: self.process_start_epoch_ms,
            received_generations: Arc::clone(&received_generations),
            call_count: Arc::clone(&call_count),
            last_request: Arc::clone(&last_request),
        };

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("MediaHandlerStub: bind ephemeral port");
        let addr = listener.local_addr().expect("MediaHandlerStub: local_addr");
        let incoming = tokio_stream::wrappers::TcpListenerStream::new(listener);

        tokio::spawn(async move {
            let _ = tonic::transport::Server::builder()
                .add_service(MediaHandlerServiceServer::new(stub))
                .serve_with_incoming(incoming)
                .await;
        });

        // Brief settle so the server is ready to accept connections.
        tokio::time::sleep(Duration::from_millis(50)).await;

        MediaHandlerStubHandle {
            addr,
            received_generations,
            call_count,
            last_request,
        }
    }
}

#[tonic::async_trait]
impl MediaHandlerService for MediaHandlerStub {
    async fn register_meeting(
        &self,
        request: Request<RegisterMeetingRequest>,
    ) -> Result<Response<RegisterMeetingResponse>, Status> {
        let req = request.into_inner();
        let sent = req.policy_generation;

        self.received_generations
            .lock()
            .expect("MediaHandlerStub mutex poisoned")
            .push(sent);
        self.call_count.fetch_add(1, Ordering::Relaxed);
        *self
            .last_request
            .lock()
            .expect("MediaHandlerStub mutex poisoned") = Some(req);

        let applied_generation = match self.applied_generation {
            AppliedGenerationBehaviour::ReportZero => 0,
            AppliedGenerationBehaviour::EchoSent => sent,
            AppliedGenerationBehaviour::Stall(at) => at,
        };

        Ok(Response::new(RegisterMeetingResponse {
            accepted: self.accept,
            applied_generation,
            handler_id: self.handler_id.clone(),
            process_start_epoch_ms: self.process_start_epoch_ms,
            transport_mode: self.transport_mode as i32,
        }))
    }
}

// ---------------------------------------------------------------------------
// MockMh — capacity data holder, unrelated to the gRPC stub above.
// ---------------------------------------------------------------------------

/// Mock Media Handler capacity data for testing MC-MH coordination.
#[derive(Debug)]
pub struct MockMh {
    id: String,
    accept_registration: bool,
    max_streams: u32,
    current_streams: u32,
}

impl Default for MockMh {
    fn default() -> Self {
        Self {
            id: "mh-test-default".to_string(),
            accept_registration: true,
            max_streams: 1000,
            current_streams: 0,
        }
    }
}

impl MockMh {
    /// Create a new MockMh builder.
    #[must_use]
    pub fn builder() -> MockMhBuilder {
        MockMhBuilder::default()
    }

    /// Get the MH ID.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Check if this mock accepts registrations.
    #[must_use]
    pub fn accepts_registration(&self) -> bool {
        self.accept_registration
    }

    /// Get the maximum stream capacity.
    #[must_use]
    pub fn max_streams(&self) -> u32 {
        self.max_streams
    }

    /// Get the current stream count.
    #[must_use]
    pub fn current_streams(&self) -> u32 {
        self.current_streams
    }

    /// Check if the MH is at capacity.
    #[must_use]
    pub fn is_at_capacity(&self) -> bool {
        self.current_streams >= self.max_streams
    }

    /// Get the capacity utilization percentage.
    #[must_use]
    pub fn utilization_percent(&self) -> f32 {
        if self.max_streams == 0 {
            return 100.0;
        }
        (self.current_streams as f32 / self.max_streams as f32) * 100.0
    }
}

/// Builder for MockMh configuration.
#[derive(Debug, Default)]
pub struct MockMhBuilder {
    id: Option<String>,
    accept_registration: bool,
    max_streams: u32,
    current_streams: u32,
}

impl MockMhBuilder {
    /// Set the MH ID.
    #[must_use]
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Configure the mock to accept participant registration.
    #[must_use]
    pub fn accept_registration(mut self) -> Self {
        self.accept_registration = true;
        self
    }

    /// Configure the mock to reject participant registration.
    #[must_use]
    pub fn reject_registration(mut self) -> Self {
        self.accept_registration = false;
        self
    }

    /// Set capacity configuration.
    #[must_use]
    pub fn with_capacity(mut self, max_streams: u32, current_streams: u32) -> Self {
        self.max_streams = max_streams;
        self.current_streams = current_streams;
        self
    }

    /// Build the MockMh.
    #[must_use]
    pub fn build(self) -> MockMh {
        MockMh {
            id: self.id.unwrap_or_else(|| "mh-test-default".to_string()),
            accept_registration: self.accept_registration,
            max_streams: self.max_streams,
            current_streams: self.current_streams,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_mh_builder() {
        let mh = MockMh::builder()
            .id("mh-test-1")
            .accept_registration()
            .with_capacity(100, 50)
            .build();

        assert_eq!(mh.id(), "mh-test-1");
        assert!(mh.accepts_registration());
        assert_eq!(mh.max_streams(), 100);
        assert_eq!(mh.current_streams(), 50);
        assert!(!mh.is_at_capacity());
        assert!((mh.utilization_percent() - 50.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_mock_mh_at_capacity() {
        let mh = MockMh::builder().with_capacity(100, 100).build();

        assert!(mh.is_at_capacity());
        assert!((mh.utilization_percent() - 100.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_mock_mh_default() {
        let mh = MockMh::default();

        assert_eq!(mh.id(), "mh-test-default");
        assert!(mh.accepts_registration());
        assert_eq!(mh.max_streams(), 1000);
        assert_eq!(mh.current_streams(), 0);
    }

    /// The echo default must stay honest: a fixture that echoed the sent
    /// generation by default would validate ADR-0036 §8's anti-pattern in every
    /// success test that forgot to configure it.
    #[test]
    fn applied_generation_default_reports_zero_not_the_sent_value() {
        assert_eq!(
            AppliedGenerationBehaviour::default(),
            AppliedGenerationBehaviour::ReportZero
        );
        assert_eq!(
            MediaHandlerStubBuilder::default().applied_generation,
            AppliedGenerationBehaviour::ReportZero
        );
    }
}
