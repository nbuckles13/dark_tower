//! One loopback forward-path rig for the three media integration binaries.
//!
//! # Why this exists
//!
//! `media_forward_integration`, `media_backpressure_integration` and
//! `media_metrics_integration` each hand-rolled the same ~20-line sequence:
//! `register_request` → `MeetingPolicy::from_request` → `RoutingTable::install`
//! → `LocalSubscribers::register` → `ConnectionForwarder::new`. Three copies of
//! one setup is three places to find when the shape changes, and the shape is
//! about to change — the sender binding lands from the control plane, which
//! moves how a forwarder is constructed.
//!
//! This directory is already the established rig home for these binaries
//! (`accept_loop_rig`, `grpc_rig`, `jwks_rig`, `mock_mc`, `wt_client`,
//! `media_frame`), so it needs no new crate edge. Same shape as
//! `mc-service`'s `tests/common/mod.rs::build_test_stack`.
//!
//! # The arena hint comes from production, not from a test literal
//!
//! The five call sites previously typed `512` while production derives
//! `NOMINAL_AUDIO_FRAME_BYTES`. A test that sizes the fan-out arena differently
//! from production is measuring a different allocator behaviour than the one
//! that ships — which matters specifically for the no-per-frame-allocation
//! gate, whose whole subject is when the arena reclaims in place.

use mh_service::config::{PolicyLimits, EGRESS_QUEUE_FRAMES, NOMINAL_AUDIO_FRAME_BYTES};
use mh_service::media::forward::EgressQueue;
use mh_service::media::forwarder::ConnectionForwarder;
use mh_service::media::queue::SharedQueue;
use mh_service::observability::metrics::{resolve_media_handles, MediaMetricHandles};
use mh_service::routing::{MeetingKey, MeetingPolicy, RoutingTable, SenderId};
use mh_service::session::LocalSubscribers;
use proto_gen::dark_tower::internal::v1::EgressStream;
use std::sync::Arc;

/// A live forward path for one meeting: policy installed, subscribers
/// registered, forwarder bound.
pub struct LoopbackRig {
    /// The publisher's forwarder, sampling at the ratio the caller asked for.
    pub forwarder: ConnectionForwarder,
    /// The installed routing table. Re-read per frame by the forward path.
    pub routing: RoutingTable,
    /// The subscriber registry.
    pub subscribers: LocalSubscribers,
    /// Handles, shared with the forwarder so a test can assert through them.
    pub handles: Arc<MediaMetricHandles>,
    /// The meeting every id in this rig is scoped to.
    pub meeting: MeetingKey,
}

impl LoopbackRig {
    /// Build a rig for `meeting` with `streams` installed and `publisher` bound.
    ///
    /// `sample_ratio` is passed through rather than defaulted: any test that
    /// observes a latency emission must force `1.0`, because a random sampler
    /// gating an assertion is a flake by construction.
    #[must_use]
    pub fn new(
        meeting: &str,
        publisher: u32,
        streams: Vec<EgressStream>,
        sample_ratio: f64,
    ) -> Self {
        let handles = Arc::new(resolve_media_handles());
        let request = mh_test_utils::media_policy::register_request(meeting, 1, streams);
        let policy = MeetingPolicy::from_request(&request, &PolicyLimits::default())
            .expect("fixture policy must validate");
        let routing = RoutingTable::new();
        routing.install(&policy);

        let forwarder = ConnectionForwarder::new(
            MeetingKey::new(meeting),
            sender(publisher),
            Arc::clone(&handles),
            sample_ratio,
            // Production's own sizing, not a test literal.
            NOMINAL_AUDIO_FRAME_BYTES,
        );

        Self {
            forwarder,
            routing,
            subscribers: LocalSubscribers::new(),
            handles,
            meeting: MeetingKey::new(meeting),
        }
    }

    /// Register one subscriber connection and hand back its egress queue.
    ///
    /// The queue is built from `EGRESS_QUEUE_FRAMES`, the declared bound, so a
    /// back-pressure assertion written against that constant stays true whatever
    /// its value.
    #[must_use]
    pub fn subscriber(&self, subscriber_sender: u32) -> Arc<EgressQueue> {
        let queue: Arc<EgressQueue> = Arc::new(SharedQueue::new(EGRESS_QUEUE_FRAMES));
        self.subscribers
            .register(self.meeting.clone(), sender(subscriber_sender), &queue);
        queue
    }
}

/// A meeting-scoped sender ordinal from a well-formed fixture value.
#[must_use]
pub fn sender(value: u32) -> SenderId {
    SenderId::from_wire(value).expect("fixture sender ids are well-formed")
}
