//! When a video publisher starts a group: the pure state machine behind
//! [`GroupStartSource`]. No I/O; time is a `tokio::time::Instant` the caller
//! passes, so paused-time tests drive it exactly.

use std::time::Duration;
use tokio::time::Instant;

/// What may start a new group after the first (see the module docs of
/// [`super`] for the agreed semantics of each value).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GroupStartSource {
    /// Groups start only at stream start. Also the never-keying source.
    #[default]
    None,
    /// Answer `MhServerMessage.keyframe_request` by the counter rule.
    OnKeyframeRequest {
        /// Fixture-side floor between group starts (default 0). An in-interval
        /// request is DEFERRED (one slot per stream), never refused.
        min_interval: Duration,
    },
    /// A new group every interval on tokio time. **Fixture-only — never a
    /// production keyframe interval** (ADR-0036 as amended has none).
    Periodic(Duration),
}

/// Why a group started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupStartCause {
    /// The first group of the stream.
    StreamStart,
    /// An answered keyframe request.
    Request,
    /// A deferred keyframe request, fired once `min_interval` elapsed.
    DeferredRequest,
    /// The fixture-only periodic cadence.
    Periodic,
    /// The first frame after a compliant publisher is re-directed: a resume is
    /// a new group (story 3: the counter increments on every keyframe,
    /// including the one that starts a resume).
    Resume,
}

/// What became of one keyframe request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestOutcome {
    /// Named the directed stream with a counter `>= current`: a group starts now.
    Answered,
    /// As `Answered`, but inside `min_interval`: held in the one deferred slot
    /// (a later in-interval request coalesces into it).
    Deferred,
    /// Its counter is below the current group's (stale, or an answered repeat).
    IgnoredLower,
    /// It named a stream number other than the directed send id.
    IgnoredOtherStream,
    /// The source is not `OnKeyframeRequest`.
    NotRequestDriven,
    /// No group has started yet (a request never starts capture), the
    /// publisher is withdrawn (a request never makes an undirected stream
    /// send), or the stream stopped at the counter ceiling.
    NotSending,
}

/// One entry of the ordered publisher log — requests and group starts in ONE
/// sequence, so a consumer can prove "a request for exactly that stream was
/// observed before the group start". In-memory only (R-30): never logged, and
/// its `Debug` prints variant, outcome and cause only — never a counter or a
/// stream number — so a failing `assert_eq!` on the log leaks neither.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PublisherEvent {
    /// A keyframe request arrived.
    KeyframeRequest {
        /// The request's `stream_number`.
        stream_number: u32,
        /// The request's `gop_counter`.
        gop_counter: u32,
        /// What the publisher did with it.
        outcome: RequestOutcome,
    },
    /// A group started (decided; its first frame goes out on the next send).
    GroupStart {
        /// The new group's counter.
        gop_counter: u32,
        /// Why.
        cause: GroupStartCause,
    },
    /// A group start was due at `u32::MAX`: the stream stopped, never wrapped.
    Exhausted,
    /// MH stopped the group stream (STOP_SENDING): the stream is dropped and
    /// frames are withheld until the next decided group start.
    StreamStoppedByPeer,
}

impl std::fmt::Debug for PublisherEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::KeyframeRequest { outcome, .. } => f
                .debug_struct("KeyframeRequest")
                .field("outcome", outcome)
                .finish_non_exhaustive(),
            Self::GroupStart { cause, .. } => f
                .debug_struct("GroupStart")
                .field("cause", cause)
                .finish_non_exhaustive(),
            Self::Exhausted => f.write_str("Exhausted"),
            Self::StreamStoppedByPeer => f.write_str("StreamStoppedByPeer"),
        }
    }
}

/// A scheduler decision the publisher must act on. `Debug` omits the counter.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Start a new group carrying this counter.
    GroupStart {
        /// The new group's counter.
        gop_counter: u32,
        /// Why.
        cause: GroupStartCause,
    },
    /// The counter is exhausted: stop the stream.
    Exhausted,
}

impl std::fmt::Debug for Decision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GroupStart { cause, .. } => f
                .debug_struct("GroupStart")
                .field("cause", cause)
                .finish_non_exhaustive(),
            Self::Exhausted => f.write_str("Exhausted"),
        }
    }
}

/// The group-start state machine for ONE stream. `Debug` prints only the
/// source and whether it is exhausted (its counters are stream identity).
pub struct GroupScheduler {
    source: GroupStartSource,
    directed_send_id: u16,
    current: Option<u32>,
    last_start: Option<Instant>,
    /// The deferred request's counter, if one is held.
    deferred: Option<u32>,
    next_periodic: Option<Instant>,
    exhausted: bool,
    /// Whether the publisher is sending (a request never makes an undirected
    /// stream send: the client's `not_sending`).
    sending: bool,
    events: Vec<PublisherEvent>,
}

impl std::fmt::Debug for GroupScheduler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GroupScheduler")
            .field("exhausted", &self.exhausted)
            .field("sending", &self.sending)
            .finish_non_exhaustive()
    }
}

impl GroupScheduler {
    /// A scheduler that has not started its first group.
    #[must_use]
    pub fn new(source: GroupStartSource, directed_send_id: u16) -> Self {
        Self {
            source,
            directed_send_id,
            current: None,
            last_start: None,
            deferred: None,
            next_periodic: None,
            exhausted: false,
            sending: true,
            events: Vec::new(),
        }
    }

    /// Test-only: a scheduler whose current group already carries `current`
    /// (started at `now`), so the counter ceiling is reachable without four
    /// billion group starts.
    #[cfg(test)]
    pub(crate) fn started_at(
        source: GroupStartSource,
        directed_send_id: u16,
        current: u32,
        now: Instant,
    ) -> Self {
        let mut s = Self::new(source, directed_send_id);
        s.current = Some(current);
        s.last_start = Some(now);
        if let GroupStartSource::Periodic(interval) = source {
            s.next_periodic = Some(now + interval);
        }
        s
    }

    /// The current group's counter (`None` before the first group).
    #[must_use]
    pub fn current(&self) -> Option<u32> {
        self.current
    }

    /// Whether the stream stopped at the counter ceiling.
    #[must_use]
    pub fn is_exhausted(&self) -> bool {
        self.exhausted
    }

    /// The ordered event log.
    #[must_use]
    pub fn events(&self) -> &[PublisherEvent] {
        &self.events
    }

    /// Start the first group. Idempotent: once started it returns `None`.
    pub fn start(&mut self, now: Instant) -> Option<Decision> {
        if self.current.is_some() || self.exhausted {
            return None;
        }
        Some(self.begin_group(GroupStartCause::StreamStart, now))
    }

    /// Apply one keyframe request at `now` (the counter rule). The request is
    /// logged before any group start it causes, so the log order is the causal
    /// order.
    pub fn on_request(
        &mut self,
        stream_number: u32,
        gop_counter: u32,
        now: Instant,
    ) -> Option<Decision> {
        let (outcome, cause) = self.judge_request(stream_number, gop_counter, now);
        self.events.push(PublisherEvent::KeyframeRequest {
            stream_number,
            gop_counter,
            outcome,
        });
        cause.map(|cause| self.begin_group(cause, now))
    }

    /// Whether the publisher is sending. While not, requests are `NotSending`
    /// and a held deferred request is dropped.
    pub fn set_sending(&mut self, sending: bool) {
        self.sending = sending;
        if !sending {
            self.deferred = None;
        }
    }

    /// Start the group a resume begins (after a withdrawal). `None` before the
    /// first group or once exhausted.
    pub fn resume(&mut self, now: Instant) -> Option<Decision> {
        if self.current.is_none() || self.exhausted {
            return None;
        }
        Some(self.begin_group(GroupStartCause::Resume, now))
    }

    /// Record that MH stopped the group stream.
    pub fn record_stopped_by_peer(&mut self) {
        self.events.push(PublisherEvent::StreamStoppedByPeer);
    }

    fn judge_request(
        &mut self,
        stream_number: u32,
        gop_counter: u32,
        now: Instant,
    ) -> (RequestOutcome, Option<GroupStartCause>) {
        let GroupStartSource::OnKeyframeRequest { min_interval } = self.source else {
            return (RequestOutcome::NotRequestDriven, None);
        };
        // A u32 that does not fit the u16 send id names some other stream.
        if u16::try_from(stream_number).ok() != Some(self.directed_send_id) {
            return (RequestOutcome::IgnoredOtherStream, None);
        }
        let (Some(current), Some(last_start)) = (self.current, self.last_start) else {
            return (RequestOutcome::NotSending, None);
        };
        if self.exhausted || !self.sending {
            return (RequestOutcome::NotSending, None);
        }
        if gop_counter < current {
            return (RequestOutcome::IgnoredLower, None);
        }
        if now < last_start + min_interval {
            // One deferred slot; a later in-interval request coalesces into it.
            self.deferred = Some(self.deferred.map_or(gop_counter, |d| d.max(gop_counter)));
            return (RequestOutcome::Deferred, None);
        }
        // Answering now also answers anything deferred.
        self.deferred = None;
        (RequestOutcome::Answered, Some(GroupStartCause::Request))
    }

    /// The next instant [`Self::poll`] may decide something, if any.
    #[must_use]
    pub fn next_deadline(&self) -> Option<Instant> {
        if self.exhausted {
            return None;
        }
        let deferred_at = match (self.deferred, self.source, self.last_start) {
            (Some(_), GroupStartSource::OnKeyframeRequest { min_interval }, Some(last)) => {
                Some(last + min_interval)
            }
            _ => None,
        };
        match (deferred_at, self.next_periodic) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    /// Fire whatever is due at `now`: a deferred request (re-checked against
    /// the current counter) or the periodic cadence.
    pub fn poll(&mut self, now: Instant) -> Option<Decision> {
        if self.exhausted {
            return None;
        }
        if let (Some(requested), GroupStartSource::OnKeyframeRequest { min_interval }, Some(last)) =
            (self.deferred, self.source, self.last_start)
        {
            if now >= last + min_interval {
                self.deferred = None;
                if self.current.is_some_and(|c| requested >= c) {
                    return Some(self.begin_group(GroupStartCause::DeferredRequest, now));
                }
            }
        }
        if self.next_periodic.is_some_and(|at| now >= at) {
            return Some(self.begin_group(GroupStartCause::Periodic, now));
        }
        None
    }

    fn begin_group(&mut self, cause: GroupStartCause, now: Instant) -> Decision {
        let next = match self.current {
            None => Some(0),
            Some(c) => c.checked_add(1),
        };
        let Some(gop_counter) = next else {
            self.exhausted = true;
            self.deferred = None;
            self.next_periodic = None;
            self.events.push(PublisherEvent::Exhausted);
            return Decision::Exhausted;
        };
        self.current = Some(gop_counter);
        self.last_start = Some(now);
        if let GroupStartSource::Periodic(interval) = self.source {
            self.next_periodic = Some(now + interval);
        }
        self.events
            .push(PublisherEvent::GroupStart { gop_counter, cause });
        Decision::GroupStart { gop_counter, cause }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIDEO: u16 = 1;

    fn starts(events: &[PublisherEvent]) -> Vec<(u32, GroupStartCause)> {
        events
            .iter()
            .filter_map(|e| match e {
                PublisherEvent::GroupStart { gop_counter, cause } => Some((*gop_counter, *cause)),
                _ => None,
            })
            .collect()
    }

    fn outcomes(events: &[PublisherEvent]) -> Vec<RequestOutcome> {
        events
            .iter()
            .filter_map(|e| match e {
                PublisherEvent::KeyframeRequest { outcome, .. } => Some(*outcome),
                _ => None,
            })
            .collect()
    }

    #[tokio::test(start_paused = true)]
    async fn on_keyframe_request_counter_rule_in_one_scripted_sequence() {
        let src = GroupStartSource::OnKeyframeRequest {
            min_interval: Duration::ZERO,
        };
        let mut s = GroupScheduler::new(src, VIDEO);
        let t0 = Instant::now();
        assert_eq!(
            s.start(t0),
            Some(Decision::GroupStart {
                gop_counter: 0,
                cause: GroupStartCause::StreamStart
            })
        );
        // equal → keys once (0 → 1)
        assert!(matches!(
            s.on_request(1, 0, t0),
            Some(Decision::GroupStart { gop_counter: 1, .. })
        ));
        // the answered repeat (counter 0, now lower) → nothing
        assert_eq!(s.on_request(1, 0, t0), None);
        // higher → keys once (1 → 2)
        assert!(matches!(
            s.on_request(1, 7, t0),
            Some(Decision::GroupStart { gop_counter: 2, .. })
        ));
        // lower → nothing
        assert_eq!(s.on_request(1, 1, t0), None);
        // another stream's number (the audio id, and one past u16) → nothing
        assert_eq!(s.on_request(0, 2, t0), None);
        assert_eq!(s.on_request(u32::from(u16::MAX) + 2, 2, t0), None);

        assert_eq!(
            starts(s.events()),
            vec![
                (0, GroupStartCause::StreamStart),
                (1, GroupStartCause::Request),
                (2, GroupStartCause::Request),
            ],
            "exactly one group per answered request, none otherwise"
        );
        assert_eq!(
            outcomes(s.events()),
            vec![
                RequestOutcome::Answered,
                RequestOutcome::IgnoredLower,
                RequestOutcome::Answered,
                RequestOutcome::IgnoredLower,
                RequestOutcome::IgnoredOtherStream,
                RequestOutcome::IgnoredOtherStream,
            ]
        );
        // The log order is the causal order: each answered request sits
        // immediately before its group start.
        let ev = s.events();
        let first_req = ev
            .iter()
            .position(|e| matches!(e, PublisherEvent::KeyframeRequest { gop_counter: 0, .. }))
            .expect("request logged");
        assert_eq!(
            ev[first_req + 1],
            PublisherEvent::GroupStart {
                gop_counter: 1,
                cause: GroupStartCause::Request
            }
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_request_before_the_first_group_never_starts_capture() {
        let mut s = GroupScheduler::new(
            GroupStartSource::OnKeyframeRequest {
                min_interval: Duration::ZERO,
            },
            VIDEO,
        );
        assert_eq!(s.on_request(1, 0, Instant::now()), None);
        assert_eq!(outcomes(s.events()), vec![RequestOutcome::NotSending]);
        assert_eq!(s.current(), None);
    }

    #[tokio::test(start_paused = true)]
    async fn a_withdrawn_publisher_answers_nothing_and_a_resume_is_a_new_group() {
        let mut s = GroupScheduler::new(
            GroupStartSource::OnKeyframeRequest {
                min_interval: Duration::from_millis(500),
            },
            VIDEO,
        );
        let now = Instant::now();
        s.start(now);
        assert_eq!(s.on_request(1, 0, now), None, "in-interval: deferred");
        s.set_sending(false);
        assert_eq!(s.on_request(1, 0, now), None);
        tokio::time::advance(Duration::from_secs(1)).await;
        assert_eq!(
            s.poll(Instant::now()),
            None,
            "the deferred request was dropped on withdrawal"
        );
        assert_eq!(
            outcomes(s.events()),
            vec![RequestOutcome::Deferred, RequestOutcome::NotSending]
        );
        s.set_sending(true);
        assert_eq!(
            s.resume(Instant::now()),
            Some(Decision::GroupStart {
                gop_counter: 1,
                cause: GroupStartCause::Resume
            })
        );
    }

    #[test]
    fn debug_never_prints_a_counter_or_a_stream_number() {
        let ev = PublisherEvent::KeyframeRequest {
            stream_number: 0x0005_1234,
            gop_counter: 0x0007_7777,
            outcome: RequestOutcome::Answered,
        };
        let gs = PublisherEvent::GroupStart {
            gop_counter: 0x0007_7777,
            cause: GroupStartCause::Request,
        };
        let d = Decision::GroupStart {
            gop_counter: 0x0007_7777,
            cause: GroupStartCause::Periodic,
        };
        for text in [format!("{ev:?}"), format!("{gs:?}"), format!("{d:?}")] {
            for secret in [0x0005_1234_u32, 0x0007_7777_u32] {
                assert!(!text.contains(&secret.to_string()), "{text}");
                assert!(
                    !text.to_lowercase().contains(&format!("{secret:x}")),
                    "{text}"
                );
            }
            assert!(
                !text.contains("gop") && !text.contains("stream_number"),
                "{text}"
            );
        }
        assert!(format!("{ev:?}").contains("Answered"));
    }

    #[tokio::test(start_paused = true)]
    async fn min_interval_defers_one_request_and_rechecks_it_when_it_fires() {
        let floor = Duration::from_millis(500);
        let mut s = GroupScheduler::new(
            GroupStartSource::OnKeyframeRequest {
                min_interval: floor,
            },
            VIDEO,
        );
        let t0 = Instant::now();
        s.start(t0);
        tokio::time::advance(Duration::from_millis(100)).await;
        // in-interval → deferred, no group yet
        assert_eq!(s.on_request(1, 0, Instant::now()), None);
        // a second in-interval request coalesces into the same slot
        assert_eq!(s.on_request(1, 0, Instant::now()), None);
        assert_eq!(s.next_deadline(), Some(t0 + floor));
        tokio::time::advance(Duration::from_millis(399)).await;
        assert_eq!(s.poll(Instant::now()), None, "not before the floor");
        tokio::time::advance(Duration::from_millis(1)).await;
        assert_eq!(
            s.poll(Instant::now()),
            Some(Decision::GroupStart {
                gop_counter: 1,
                cause: GroupStartCause::DeferredRequest
            })
        );
        assert_eq!(s.poll(Instant::now()), None, "the deferred slot fires once");
        assert_eq!(
            outcomes(s.events()),
            vec![RequestOutcome::Deferred, RequestOutcome::Deferred]
        );
        assert_eq!(starts(s.events()).len(), 2);
        // Out of interval again: answered immediately.
        tokio::time::advance(floor).await;
        assert!(s.on_request(1, 1, Instant::now()).is_some());
    }

    #[tokio::test(start_paused = true)]
    async fn periodic_starts_a_group_every_interval_and_none_between() {
        let every = Duration::from_millis(100);
        let mut s = GroupScheduler::new(GroupStartSource::Periodic(every), VIDEO);
        let t0 = Instant::now();
        s.start(t0);
        for k in 1..=5_u32 {
            tokio::time::advance(every - Duration::from_millis(1)).await;
            assert_eq!(
                s.poll(Instant::now()),
                None,
                "nothing at k*interval - 1ms (k={k})"
            );
            tokio::time::advance(Duration::from_millis(1)).await;
            assert_eq!(
                s.poll(Instant::now()),
                Some(Decision::GroupStart {
                    gop_counter: k,
                    cause: GroupStartCause::Periodic
                }),
                "a group start at exactly k*interval (k={k})"
            );
            assert_eq!(s.poll(Instant::now()), None, "exactly one per tick");
        }
        assert_eq!(starts(s.events()).len(), 6);
        // Requests are not honoured under Periodic.
        assert_eq!(s.on_request(1, 99, Instant::now()), None);
        assert_eq!(outcomes(s.events()), vec![RequestOutcome::NotRequestDriven]);
    }

    #[tokio::test(start_paused = true)]
    async fn none_never_starts_a_group_after_the_first() {
        let mut s = GroupScheduler::new(GroupStartSource::None, VIDEO);
        s.start(Instant::now());
        assert_eq!(s.next_deadline(), None);
        for _ in 0..100 {
            tokio::time::advance(Duration::from_secs(60)).await;
            assert_eq!(s.poll(Instant::now()), None);
        }
        assert_eq!(s.on_request(1, 0, Instant::now()), None);
        assert_eq!(starts(s.events()), vec![(0, GroupStartCause::StreamStart)]);
    }

    #[tokio::test(start_paused = true)]
    async fn the_counter_stops_at_u32_max_and_never_wraps() {
        let now = Instant::now();
        let mut s = GroupScheduler::started_at(
            GroupStartSource::OnKeyframeRequest {
                min_interval: Duration::ZERO,
            },
            VIDEO,
            u32::MAX - 1,
            now,
        );
        assert!(matches!(
            s.on_request(1, u32::MAX - 1, now),
            Some(Decision::GroupStart {
                gop_counter: u32::MAX,
                ..
            })
        ));
        assert_eq!(s.on_request(1, u32::MAX, now), Some(Decision::Exhausted));
        assert!(s.is_exhausted());
        assert_eq!(s.current(), Some(u32::MAX), "never wrapped to 0");
        assert_eq!(s.on_request(1, u32::MAX, now), None);
        assert_eq!(s.poll(now), None);
        assert_eq!(
            s.events().last(),
            Some(&PublisherEvent::KeyframeRequest {
                stream_number: 1,
                gop_counter: u32::MAX,
                outcome: RequestOutcome::NotSending
            })
        );
        assert!(s.events().contains(&PublisherEvent::Exhausted));

        let mut p = GroupScheduler::started_at(
            GroupStartSource::Periodic(Duration::from_millis(10)),
            VIDEO,
            u32::MAX,
            now,
        );
        assert_eq!(
            p.poll(now + Duration::from_millis(10)),
            Some(Decision::Exhausted)
        );
        assert_eq!(p.next_deadline(), None);
    }
}
