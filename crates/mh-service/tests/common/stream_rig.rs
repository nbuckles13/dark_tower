//! The stream-per-group forward-path rig (story 3 MH-A2): real routing table,
//! real subscriber registry, real stream ingress accept loop and real
//! per-subscriber stream egress task, all over the deterministic transport
//! shim — zero syscalls, tokio paused time.
//!
//! Build it AFTER taking a `MetricAssertion` snapshot: the handles resolve
//! against whatever recorder is current on this thread.

use mh_service::config::StreamPathLimits;
use mh_service::config::EGRESS_QUEUE_FRAMES;
use mh_service::media::downlink::run_stream_egress;
use mh_service::media::edge_runtime::{EdgeContext, EdgeRuntime};
use mh_service::media::forward::EgressQueue;
use mh_service::media::queue::SharedQueue;
use mh_service::media::stream_ingress::{run_stream_ingress, StreamIngressContext};
use mh_service::observability::metrics::{resolve_media_handles, MediaMetricHandles};
use mh_service::routing::{MeetingKey, MeetingPolicy, RoutingTable};
use mh_service::session::LocalSubscribers;
use mh_test_utils::transport_shim::LossDelayTransport;
use proto_gen::dark_tower::internal::v1::RegisterMeetingRequest;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// The rig's meeting id.
pub const MEETING: &str = "m-stream";

/// Stream-path limits for tests: a generous rate, a backlog cap and stall
/// bound the test chooses.
#[must_use]
pub fn limits(backlog: usize, stall_ms: u64) -> StreamPathLimits {
    StreamPathLimits {
        opens_per_connection_per_sec: 100,
        egress_stream_backlog_bytes: backlog,
        mid_frame_stall: Duration::from_millis(stall_ms),
    }
}

/// The rig.
pub struct StreamRig {
    pub routing: Arc<RoutingTable>,
    pub subscribers: Arc<LocalSubscribers>,
    pub handles: Arc<MediaMetricHandles>,
    pub meeting: MeetingKey,
    pub limits: StreamPathLimits,
    pub cancel: CancellationToken,
    /// Each publisher's per-connection cancel, so one can be "disconnected".
    publishers: Vec<(u32, CancellationToken)>,
}

pub use super::media_rig::sender;

impl StreamRig {
    /// Install `request` (which must be for [`MEETING`]).
    #[must_use]
    pub fn new(request: &RegisterMeetingRequest, limits: StreamPathLimits) -> Self {
        let handles = Arc::new(resolve_media_handles());
        let routing = Arc::new(RoutingTable::with_edge_context(EdgeContext {
            backlog_cap: limits.egress_stream_backlog_bytes,
            handles: Arc::clone(&handles),
        }));
        let rig = Self {
            routing,
            subscribers: Arc::new(LocalSubscribers::new()),
            handles,
            meeting: MeetingKey::new(MEETING),
            limits,
            cancel: CancellationToken::new(),
            publishers: Vec::new(),
        };
        rig.install(request);
        rig
    }

    /// Install (or re-push) a policy, as the session actor's apply does.
    pub fn install(&self, request: &RegisterMeetingRequest) {
        let policy = MeetingPolicy::from_request(
            request,
            &mh_test_utils::admission::fixture_policy_limits(),
        )
        .expect("fixture policy must validate");
        self.routing.install(&policy);
    }

    /// Connect a publisher: its stream ingress accept loop runs on the shim.
    pub fn publisher(&mut self, publisher: u32) -> LossDelayTransport {
        let shim = LossDelayTransport::new();
        let cancel = self.cancel.child_token();
        let context = Arc::new(StreamIngressContext {
            meeting: self.meeting.clone(),
            sender: sender(publisher),
            routing: Arc::clone(&self.routing),
            subscribers: Arc::clone(&self.subscribers),
            handles: Arc::clone(&self.handles),
            limits: self.limits,
            sample_ratio: 1.0,
        });
        tokio::spawn(run_stream_ingress(
            Arc::new(shim.clone()),
            context,
            cancel.clone(),
        ));
        self.publishers.push((publisher, cancel));
        shim
    }

    /// Disconnect a publisher as the connection teardown does: cancel its
    /// tasks, then tell the edges its sources ended.
    pub async fn disconnect_publisher(&mut self, publisher: u32) {
        for (who, cancel) in &self.publishers {
            if *who == publisher {
                cancel.cancel();
            }
        }
        settle().await;
        self.routing.sender_ended(&self.meeting, sender(publisher));
    }

    /// Connect a subscriber: registered with a datagram queue and a downlink
    /// channel; its stream egress task runs on the shim.
    pub fn subscriber(&self, subscriber: u32) -> LossDelayTransport {
        self.subscriber_with_cancel(subscriber).0
    }

    /// [`Self::subscriber`], also returning the token that tears down just
    /// this subscriber's stream egress task (its connection going away).
    pub fn subscriber_with_cancel(
        &self,
        subscriber: u32,
    ) -> (LossDelayTransport, CancellationToken) {
        let shim = LossDelayTransport::new();
        let cancel = self.cancel.child_token();
        let queue: Arc<EgressQueue> = Arc::new(SharedQueue::new(EGRESS_QUEUE_FRAMES));
        let (tx, rx) = tokio::sync::mpsc::channel(mh_service::media::downlink_channel_capacity(64));
        self.subscribers.register_with_streams(
            self.meeting.clone(),
            sender(subscriber),
            &format!("conn-{subscriber}"),
            &queue,
            Some(tx),
        );
        tokio::spawn(run_stream_egress(
            Arc::new(shim.clone()),
            rx,
            cancel.clone(),
        ));
        (shim, cancel)
    }

    /// The runtime cell of `egress_stream_id`.
    #[must_use]
    pub fn cell(&self, egress_stream_id: u32) -> Arc<EdgeRuntime> {
        let snapshot = self.routing.load();
        let routes = snapshot
            .routes_for(&self.meeting)
            .expect("policy installed");
        routes
            .cells()
            .iter()
            .find(|cell| cell.egress_stream_id() == egress_stream_id)
            .cloned()
            .expect("edge installed")
    }
}

impl Drop for StreamRig {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

/// Let every spawned task run to quiescence without advancing virtual time.
pub async fn settle() {
    for _ in 0..200 {
        tokio::task::yield_now().await;
    }
}

/// Concatenate frames into one stream's bytes.
#[must_use]
pub fn concat(frames: &[bytes::Bytes]) -> bytes::Bytes {
    let mut all = Vec::new();
    for frame in frames {
        all.extend_from_slice(frame);
    }
    bytes::Bytes::from(all)
}
