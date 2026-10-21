//! Test-injected MH forwarding policy for a meeting the REAL MC also programs
//! — the [`Injection`] helper, hoisted from `tests/26_mh_quic.rs` (server-mute
//! S4, edge churn) at story 3 MH-A1, when `tests/37_mh_stream_attribution.rs`
//! became its second consumer. One home; the consumers import it from here.
//!
//! Built on [`super::mh_grpc`]; the ownership-gap caveat stated there applies
//! to every injection here and is not restated.

use std::time::Duration;

use crate::fixtures::metrics::{
    format_instance_map, gauge_by_instance_present, instance_for_pod_ip,
};
pub use crate::fixtures::metrics::{BASELINE_SETTLE_BOUND, PINNED_METRIC_BOUND};
use crate::fixtures::mh_grpc::{self, Handler, ReleaseOnDrop};
use crate::fixtures::participant::Participant;
use crate::fixtures::PrometheusClient;
use proto_gen::dark_tower::internal::v1::media_handler_service_client::MediaHandlerServiceClient;
use proto_gen::dark_tower::internal::v1::{
    CandidateSource, EgressStream, EndMeetingRequest, MutedSource, RegisterMeetingRequest,
    RegisterMeetingResponse, SourceDisposition, SourceStream, SubscriberSlot,
};
use proto_gen::dark_tower::signaling::v1::TransportMode;

/// One declared source stream (story 3; `internal.proto` `SourceStream`).
/// `const` so a suite can hold a fixed probe stream in a `const`. Mirrors
/// `mh_test_utils::media_policy::source_stream`, which env-tests cannot import
/// (ADR-0028: no service test utilities).
#[must_use]
pub const fn source_stream(
    sender_id: u32,
    stream_number: u32,
    transport: TransportMode,
    disposition: SourceDisposition,
) -> SourceStream {
    SourceStream {
        sender_id,
        stream_number,
        transport_mode: transport as i32,
        disposition: disposition as i32,
    }
}

/// One MC-shaped egress stream: `source`'s `stream_number` into
/// `subscriber`'s `slot`, with MC's contract priority and supersede behaviour
/// for that send id (`internal.proto` `EgressStream`: audio 1 / no supersede,
/// video 2 / supersede on an independent frame).
///
/// `transport` is the SUPERSEDED per-egress `transport_mode`: DATAGRAM on the
/// transitional reading (it is read there), UNSPECIFIED with `source_streams`
/// declared (MC-A stops writing it; MH ignores it).
#[must_use]
pub fn edge(
    egress_stream_id: u32,
    subscriber: u32,
    slot: u32,
    source: u32,
    stream_number: u32,
    transport: TransportMode,
) -> EgressStream {
    let video = stream_number == u32::from(crate::fixtures::media::VIDEO_SEND_ID);
    EgressStream {
        egress_stream_id,
        subscriber: Some(SubscriberSlot {
            sender_id: subscriber,
            slot_id: slot,
        }),
        candidate_sources: vec![CandidateSource {
            sender_id: source,
            stream_number,
        }],
        priority_group: if video { 2 } else { 1 },
        supersede_on_independent_frame: video,
        transport_mode: transport as i32,
    }
}

/// How far above MC's installed generation the injected policy sits. Small on
/// purpose (@security, @paired-meeting-controller): it only has to stay above
/// MC's own generation, which cannot move during the injection window because
/// nothing structural changes in the meeting. If MC ever out-ran it, MC's
/// next push would install its own (empty) muted set, forwarding would resume,
/// and the mute assertions would go RED — the safe failure direction.
pub const INJECTION_MARGIN: u64 = 16;

/// A test-injected policy on ONE MH pod for a meeting the REAL MC also
/// programs, presenting MC's own `mc_id` and gRPC endpoint so MH's
/// notifications keep reaching MC and no ownership takeover occurs.
///
/// # This deliberately risks an MC-side footprint — do not triage it as MC's bug
///
/// While an injected policy is live, MH's installed generation is ABOVE MC's.
/// MC's floor adoption is closed after its first confirmed push
/// (`crates/mc-service/src/grpc/mh_client.rs`, the comment on the higher-echo
/// case), so if MC pushed for this meeting mid-window it would record
/// `mc_media_policy_pushes_total{outcome="generation_mismatch"}`, write
/// `mc_media_generation_divergence`, and fire `MCMediaGenerationDivergence`
/// (`for: 0m`). The tests keep the window free of structural changes (no
/// joins, leaves or declarations), so MC has nothing to push; and every
/// injection ends with `EndMeeting` on the pod BEFORE any session closes.
///
/// MC re-programs a pod after that `EndMeeting` ONLY when that pod's MC
/// snapshot CHANGES: MC's per-handler worker does not advance the generation
/// for an unchanged snapshot, and a publish at its last confirmed generation
/// sends no RPC (`crates/mc-service/src/media_routing/pusher.rs`). The
/// carrying pod's snapshot does change as the sessions close (edges removed),
/// so it gets a fresh install. A pod whose MC snapshot is unchanged (empty, in
/// these meeting shapes) stays UNREGISTERED for the rest of this test-owned
/// meeting — which is why the meeting is dedicated and disposable. Do not
/// generalise "EndMeeting is recovered by MC's next push" to production: until
/// story 4's re-assert cadence exists, it is false for an unchanged snapshot.
///
/// # If cleanup is skipped
///
/// `ReleaseOnDrop` releases on success and on a panic, but not on SIGKILL or a
/// harness kill. Then this meeting id stays wedged at the injected generation
/// on that pod — every later MC push for it refused as stale — holding its
/// edges and a registered-meeting slot. From story 2 task 12 that is normally
/// SELF-HEALING: the killed harness's MC sessions die with it, so once every
/// participant has left and MC's reconnect grace expires, MC ends the meeting
/// and its own `EndMeeting` releases the wedge, because the injection carries
/// MC's own `mc_id`. It persists until the pod restarts only if MC never
/// completes that `EndMeeting`. The id is a fresh GC meeting per run, so no
/// later run reuses it. Manual recovery, if needed: `EndMeeting`
/// for the id on the pod through the Layer-7 gRPC forward with the MC
/// credential (`env_tests::fixtures::mh_grpc`). Not a converge
/// (`REDEPLOY_HINT`): pods whose image/config are unchanged are not restarted,
/// so a converge leaves the wedge in place. A later run is not blocked by it —
/// its meeting id is fresh — unless leaked registrations reach the cap, which
/// [`injectable_handlers`] reports as a named PRECONDITION.
pub struct Injection<'a> {
    /// The pod this injection targets.
    pub handler: &'a Handler,
    client: MediaHandlerServiceClient<tonic::transport::Channel>,
    token: &'a str,
    meeting_id: String,
    mc_id: String,
    mc_grpc_endpoint: String,
    /// The generation the probe in [`Injection::open`] read back on this pod.
    mc_generation: u64,
}

impl<'a> Injection<'a> {
    /// Connect, record for cleanup BEFORE any call, and read MC's installed
    /// generation with a generation-1 EMPTY registration used as a probe.
    ///
    /// On a pod MC has already programmed past generation 1, the probe is
    /// refused as stale without a swap and echoes MC's live generation. But MC
    /// pushes to each handler on its own async worker, and nothing orders this
    /// probe after MC's push to a pod that carries NO edges: there the probe
    /// can INSTALL (empty, generation 1) first. That is harmless ONLY because
    /// this meeting's MC snapshot for such a pod is itself empty at generation
    /// 1 — MC's own push then no-ops as equal and truthfully confirms `match`.
    /// **Do not reuse this helper for a meeting shape where BOTH pods carry
    /// edges**: a probe that won the race would swallow MC's first real push
    /// (equal generation, no swap) while MC recorded `match` for content that
    /// was never installed. The real precondition — that MC HAS programmed the
    /// carrying pod past its pre-edge render — is
    /// [`assert_mc_programmed_a_carrying_pod`], checked across both pods.
    pub async fn open(
        handler: &'a Handler,
        token: &'a str,
        cleanup: &ReleaseOnDrop,
        owner: &Participant,
    ) -> Injection<'a> {
        let meeting_id = owner.gc.meeting_id.to_string();
        let mc_id = owner.gc.mc_assignment.mc_id.clone();
        let mc_grpc_endpoint = owner.gc.mc_assignment.grpc_endpoint.clone();
        cleanup.record(&handler.grpc_url, &meeting_id, &mc_id);
        let mut injection = Injection {
            handler,
            client: mh_grpc::connect(handler).await,
            token,
            meeting_id,
            mc_id,
            mc_grpc_endpoint,
            mc_generation: 0,
        };
        injection.mc_generation = injection.register(1, Vec::new(), &[], Vec::new()).await;
        injection
    }

    /// The injected generation for `step`: MC's probed generation plus
    /// [`INJECTION_MARGIN`].
    #[must_use]
    pub fn generation(&self, step: u64) -> u64 {
        self.mc_generation + INJECTION_MARGIN + step
    }

    /// Register at `generation` and return the echoed `applied_generation`.
    /// `sources` empty selects MH's transitional story-2 reading
    /// (`internal.proto` `server_muted_sources`); non-empty declares every
    /// source stream (story 3).
    pub async fn register(
        &mut self,
        generation: u64,
        streams: Vec<EgressStream>,
        muted: &[u32],
        sources: Vec<SourceStream>,
    ) -> u64 {
        self.register_full(generation, streams, muted, sources)
            .await
            .applied_generation
    }

    /// [`Self::register`], returning the whole response (the per-stream echo).
    pub async fn register_full(
        &mut self,
        generation: u64,
        streams: Vec<EgressStream>,
        muted: &[u32],
        sources: Vec<SourceStream>,
    ) -> RegisterMeetingResponse {
        let request = RegisterMeetingRequest {
            meeting_id: self.meeting_id.clone(),
            mc_id: self.mc_id.clone(),
            mc_grpc_endpoint: self.mc_grpc_endpoint.clone(),
            egress_streams: streams,
            selection_rules: None,
            policy_generation: generation,
            server_muted_sources: muted
                .iter()
                .map(|sender_id| MutedSource {
                    sender_id: *sender_id,
                })
                .collect(),
            source_streams: sources,
        };
        self.client
            .register_meeting(mh_grpc::authed(self.token, request))
            .await
            .unwrap_or_else(|s| {
                panic!(
                    "INJECT on {}: RegisterMeeting at generation {generation} refused ({:?}): {}",
                    self.handler.name,
                    s.code(),
                    s.message()
                )
            })
            .into_inner()
    }

    /// Install at `generation` on the TRANSITIONAL reading (no
    /// `source_streams`) and require the synchronous echo to match.
    pub async fn install(&mut self, generation: u64, streams: Vec<EgressStream>, muted: &[u32]) {
        self.install_declared(generation, streams, muted, Vec::new())
            .await;
    }

    /// Install at `generation` with `sources` declared (story 3; empty is the
    /// transitional reading) and require the synchronous echo to match.
    pub async fn install_declared(
        &mut self,
        generation: u64,
        streams: Vec<EgressStream>,
        muted: &[u32],
        sources: Vec<SourceStream>,
    ) {
        let applied = self.register(generation, streams, muted, sources).await;
        assert_eq!(
            applied, generation,
            "INJECT on {}: MH did not apply generation {generation} (echoed {applied})",
            self.handler.name
        );
    }

    /// Release, then PROVE the injected generation is gone: a generation-1
    /// registration must now echo below it (1 if it installed, or MC's own
    /// fresh generation if MC re-programmed first). Our probe is released
    /// again only if it is what installed.
    pub async fn release_and_prove(mut self, cleanup: &ReleaseOnDrop) {
        let injected = self.generation(0);
        self.end().await;
        let echoed = self.register(1, Vec::new(), &[], Vec::new()).await;
        assert!(
            echoed < injected,
            "CLEANUP on {}: after EndMeeting the handler still reports generation {echoed} (>= \
             the injected {injected}) — the meeting is wedged; recover with EndMeeting or a pod \
             restart (see Injection's docs)",
            self.handler.name
        );
        if echoed == 1 {
            self.end().await;
        }
        cleanup.forget(&self.meeting_id);
    }

    async fn end(&mut self) {
        let done = self
            .client
            .end_meeting(mh_grpc::authed(
                self.token,
                EndMeetingRequest {
                    meeting_id: self.meeting_id.clone(),
                    mc_id: self.mc_id.clone(),
                },
            ))
            .await
            .unwrap_or_else(|s| {
                panic!(
                    "CLEANUP on {}: EndMeeting refused ({:?}): {}",
                    self.handler.name,
                    s.code(),
                    s.message()
                )
            })
            .into_inner();
        assert!(done.acknowledged, "CLEANUP on {}", self.handler.name);
    }
}

/// PRECONDITION: MC has programmed the pod that carries this meeting's edges.
/// Generation 1 is MC's pre-edge render, so once edges exist the carrying
/// pod's installed generation is at least 2 — a check the probe CAN fail,
/// unlike a per-pod `>= 1` (see [`Injection::open`]).
pub fn assert_mc_programmed_a_carrying_pod(injections: &[Injection<'_>]) {
    let seen: Vec<(String, u64)> = injections
        .iter()
        .map(|i| (i.handler.name.clone(), i.mc_generation))
        .collect();
    assert!(
        seen.iter().any(|(_, g)| *g >= 2),
        "PRECONDITION: no MH pod reports an MC-programmed generation past the pre-edge render \
         (probe echoes {seen:?}); MC has not installed this meeting's edges yet, so injecting \
         now would race MC's first real push"
    );
}

/// Both MH pods Layer 7 exports. Every injection in this file goes to BOTH
/// with the same policy, so no test needs to know which pod serves which
/// WebTransport URL.
#[must_use]
pub fn both_handlers() -> Vec<Handler> {
    let hs = mh_grpc::handlers();
    assert_eq!(hs.len(), 2, "PRECONDITION: Layer 7 exports two MH pods");
    hs
}

/// [`both_handlers`], after a PRECONDITION that each pod can register one more
/// meeting: an injection registers a NEW meeting id on BOTH pods. Pods persist
/// across runs (ADR-0038: nothing restarts an unchanged MH), so a pod can hold
/// registrations a killed harness never released (see [`Injection`]'s "If
/// cleanup is skipped"). At the cap, the injection would be refused as
/// `rejected_meeting_cap` — a confusing registration refusal rather than the
/// named leak it is. Read from the published gauges, per pinned pod.
pub async fn injectable_handlers(prom: &PrometheusClient, settle_bound: Duration) -> Vec<Handler> {
    let handlers = both_handlers();
    let registered = gauge_by_instance_present(
        prom,
        "mh_media_registered_meetings",
        handlers.len(),
        settle_bound,
    )
    .await;
    let limits = gauge_by_instance_present(
        prom,
        "mh_media_registered_meetings_limit",
        handlers.len(),
        settle_bound,
    )
    .await;
    for h in &handlers {
        let held = instance_for_pod_ip(&registered, &h.pod_ip).map(|(_, v)| v);
        let limit = instance_for_pod_ip(&limits, &h.pod_ip).map(|(_, v)| v);
        let (Some(held), Some(limit)) = (held, limit) else {
            panic!(
                "PRECONDITION: {} (pod IP {}) publishes no mh_media_registered_meetings / \
                 _limit series (registered: {}, limit: {}) — a scrape/startup condition.",
                h.name,
                h.pod_ip,
                format_instance_map(&registered),
                format_instance_map(&limits),
            );
        };
        assert!(
            held + 1.0 <= limit,
            "PRECONDITION: {} holds {held}/{limit} registered meetings, so the injection's new \
             meeting id would be refused at the registration cap. Registrations nothing \
             releases: a meeting's MC never completed EndMeeting (a killed harness, see \
             Injection's recovery note; compare mh_media_registered_meetings with pod uptime). \
             An environment/leak condition, not the behaviour under test.",
            h.name
        );
    }
    handlers
}

/// One declared-reading egress (story 3): `source`'s `stream` into
/// `subscriber`'s `slot`, the superseded per-egress transport UNSPECIFIED as
/// MC-A writes it (the edge's transport derives from `source_streams`).
#[must_use]
pub fn declared_edge(
    id: u32,
    subscriber: u32,
    slot: u32,
    source: u32,
    stream: u16,
) -> EgressStream {
    edge(
        id,
        subscriber,
        slot,
        source,
        u32::from(stream),
        TransportMode::Unspecified,
    )
}

/// [`source_stream`] taking the 16-bit send-id shape the media fixtures use.
#[must_use]
pub fn declared_source(
    sender_id: u32,
    stream: u16,
    mode: TransportMode,
    disposition: SourceDisposition,
) -> SourceStream {
    source_stream(sender_id, u32::from(stream), mode, disposition)
}

/// Settled per-pod baselines for `promql` (shared by the MH attribution and
/// video-forwarding suites), bounded by [`BASELINE_SETTLE_BOUND`].
pub async fn baselines(
    prom: &PrometheusClient,
    promql: &str,
) -> crate::fixtures::metrics::InstanceCounters {
    crate::fixtures::metrics::settled_baseline(
        prom,
        promql,
        BASELINE_SETTLE_BOUND,
        "the assertion under test",
    )
    .await
}

/// Each pinned pod's counter rises by at least `at_least` past its baseline,
/// within [`PINNED_METRIC_BOUND`]. A lower bound, so one stray event cannot
/// satisfy it.
pub async fn assert_rise(
    prom: &PrometheusClient,
    handlers: &[Handler],
    promql: &str,
    base: &crate::fixtures::metrics::InstanceCounters,
    at_least: u32,
    phase: &str,
) {
    for h in handlers {
        let b = instance_for_pod_ip(base, &h.pod_ip).map_or(0.0, |(_, v)| v);
        let target = b + f64::from(at_least);
        crate::fixtures::metrics::poll_until_pinned_instance(
            prom,
            promql,
            &h.pod_ip,
            PINNED_METRIC_BOUND,
            Duration::from_secs(2),
            phase,
            &format!(
                ">= {target} (baseline {b} + the {at_least} test events sent to this pod). \
                 If short, rule out TRANSPORT-OVERFLOW first: receive-buffer loss before \
                 attribution is counted as transport_receive_dropped only at connection \
                 TEARDOWN (it is forensic, not live), so read that series on this pod after \
                 the run; a rise there is input-side loss, not an attribution fault, and is \
                 never fixed by relaxing this gate"
            ),
            |v| v >= target,
        )
        .await;
    }
}

/// Install `edges` + `declared` at `step` on every pod; require the apply and
/// a per-stream echo of exactly the declared set (PAUSED and DROP_AT_INGRESS
/// included — the echo carries no disposition).
pub async fn install_all(
    injections: &mut [Injection<'_>],
    step: u64,
    edges: Vec<EgressStream>,
    declared: Vec<SourceStream>,
) {
    use std::collections::BTreeSet;
    let want: BTreeSet<(u32, u32, i32)> = declared
        .iter()
        .map(|s| (s.sender_id, s.stream_number, s.transport_mode))
        .collect();
    for injection in injections.iter_mut() {
        let g = injection.generation(step);
        let resp = injection
            .register_full(g, edges.clone(), &[], declared.clone())
            .await;
        assert_eq!(
            resp.applied_generation, g,
            "INJECT APPLY on {}: the mixed-transport registration must apply",
            injection.handler.name
        );
        let echoed: BTreeSet<(u32, u32, i32)> = resp
            .applied_source_streams
            .iter()
            .map(|e| (e.sender_id, e.stream_number, e.transport_mode))
            .collect();
        assert_eq!(
            echoed, want,
            "INJECT ECHO on {}: the per-stream echo must list every declared source stream \
             with its transport",
            injection.handler.name
        );
    }
}
