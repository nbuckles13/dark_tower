//! The per-edge source-switch state machine (ADR-0036 §7; story 3 R-4, R-10,
//! R-17). Pure: no I/O, no clock read, no metric — `now` is passed in.
//!
//! One machine per stream-transport egress edge, held in that edge's runtime
//! cell (`crate::media::edge_runtime`). It decides, for every frame of a
//! source the edge might forward, one of three things: skip it, continue the
//! current downlink group with it, or start a new downlink group with it.
//!
//! # The rule is a behaviour, never a media type
//!
//! A group starts ONLY at an independently-decodable frame (the wire flag,
//! never stream position). A subscriber's first forwarded frame from any
//! source is therefore a group start, and a switch lands only on the new
//! source's group start. A source whose every frame is independently
//! decodable — audio — never waits, with no branch here knowing that.
//!
//! # States
//!
//! * [`EdgeState::Waiting`] — nothing being forwarded; the next group start of
//!   the assigned source opens a downlink.
//! * [`EdgeState::Forwarding`] — forwarding `source`'s ingress group `group`.
//! * [`EdgeState::SwitchPending`] — MC moved the slot A→B: keep forwarding A
//!   until B's group start, then finish A's downlink at its frame boundary and
//!   open B's. If B leaves first, the switch is abandoned and A kept.
//! * [`EdgeState::Paused`] — the assigned source is PAUSED (client mute): a
//!   switch to it completes at once as "forward nothing", cancelling any
//!   group-start wait. Frames that still arrive from it (a resume that beat
//!   MC's update) forward from its next group start.
//!
//! There is NO give-up timeout anywhere while a source stays assigned (R-4;
//! ADR §7's open item, closed 2026-10-13). "Waiting with the source seen" is
//! exposed for MH-B's keyframe requests ([`EdgeMachine::waiting_with_source_seen`]).
//!
//! # Group tokens
//!
//! `group` identifies the INGRESS group (one publisher QUIC stream; tokens are
//! process-monotonic). A frame continues the current downlink only if it
//! belongs to the same ingress group that opened it, so a stale earlier group
//! of the same source can never be spliced onto a newer group's downlink; and
//! every frame of that group continues it — an independently-decodable frame
//! inside one uplink group does NOT open a new downlink, so downlink opens per
//! source never exceed ingress group opens, which the per-connection rate cap
//! bounds (no egress stream-open amplification).

use crate::routing::CandidateRef;
use tokio::time::Instant;

/// Identifies one ingress group (one publisher uni stream), process-unique.
pub type GroupToken = u64;

/// Where the edge is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeState {
    /// Forwarding nothing; waiting for `target`'s group start.
    Waiting {
        /// The assigned source, or `None` for an edge with no candidate.
        target: Option<CandidateRef>,
        /// When a frame from `target` was first seen while waiting.
        seen_at: Option<Instant>,
    },
    /// Forwarding `source`'s ingress group `group`.
    Forwarding {
        /// The source being forwarded.
        source: CandidateRef,
        /// The ingress group whose frames continue the current downlink.
        group: GroupToken,
    },
    /// Forwarding `current` until `target`'s group start.
    SwitchPending {
        /// The source still being forwarded.
        current: CandidateRef,
        /// Its ingress group.
        group: GroupToken,
        /// The source the slot is switching to.
        target: CandidateRef,
        /// When a frame from `target` was first seen.
        seen_at: Option<Instant>,
    },
    /// Forwarding nothing for a PAUSED assigned source.
    Paused {
        /// The paused source.
        source: CandidateRef,
    },
}

/// What to do with one frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameDecision {
    /// Not forwarded to this edge; consumes no hop number.
    Skip,
    /// Append to the current downlink.
    Continue,
    /// Open a new downlink with this frame; any current downlink finishes at
    /// its frame boundary.
    StartGroup,
}

/// The side effect an assignment demands of the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignEffect {
    /// Nothing.
    None,
    /// Stop forwarding the current downlink: finish it at its frame boundary.
    Detach,
}

/// The side effect a source ending demands of the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceEndedEffect {
    /// Nothing.
    None,
    /// The forwarded source ended mid-group: reset the current downlink
    /// (`source_ended`).
    ResetCurrent,
}

/// The pure per-edge machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EdgeMachine {
    state: EdgeState,
    /// MC's current assignment: the source and whether it is PAUSED.
    assigned: Option<(CandidateRef, bool)>,
}

impl EdgeMachine {
    /// A machine for a freshly installed edge.
    #[must_use]
    pub const fn new(target: Option<CandidateRef>, paused: bool) -> Self {
        let state = match target {
            Some(source) if paused => EdgeState::Paused { source },
            _ => EdgeState::Waiting {
                target,
                seen_at: None,
            },
        };
        let assigned = match target {
            Some(source) => Some((source, paused)),
            None => None,
        };
        Self { state, assigned }
    }

    /// The current state.
    #[must_use]
    pub const fn state(&self) -> EdgeState {
        self.state
    }

    /// The source whose frames must still reach this edge although MC may no
    /// longer assign it — the one being forwarded during a switch.
    #[must_use]
    pub const fn forwarding_source(&self) -> Option<CandidateRef> {
        match self.state {
            EdgeState::Forwarding { source, .. } => Some(source),
            EdgeState::SwitchPending { current, .. } => Some(current),
            EdgeState::Waiting { .. } | EdgeState::Paused { .. } => None,
        }
    }

    /// MH-B's keyframe-request input: `Some(first seen)` while the edge waits
    /// for a group start from a source it has already received a frame from.
    /// `None` when paused, forwarding, or waiting for a source never seen.
    #[must_use]
    pub const fn waiting_with_source_seen(&self) -> Option<Instant> {
        match self.state {
            EdgeState::Waiting { seen_at, .. } | EdgeState::SwitchPending { seen_at, .. } => {
                seen_at
            }
            EdgeState::Forwarding { .. } | EdgeState::Paused { .. } => None,
        }
    }

    /// The state the edge rests in when it forwards nothing.
    const fn resting(&self) -> EdgeState {
        match self.assigned {
            Some((source, true)) => EdgeState::Paused { source },
            Some((source, false)) => EdgeState::Waiting {
                target: Some(source),
                seen_at: None,
            },
            None => EdgeState::Waiting {
                target: None,
                seen_at: None,
            },
        }
    }

    /// Apply MC's assignment. Idempotent: an identical re-assignment changes
    /// nothing (ADR-0036 §10 re-assert idempotency).
    pub fn assign(&mut self, target: Option<CandidateRef>, paused: bool) -> AssignEffect {
        let assigned = target.map(|source| (source, paused));
        if assigned == self.assigned {
            return AssignEffect::None;
        }
        self.assigned = assigned;
        match self.state {
            EdgeState::Forwarding { source, group } => match target {
                // Pausing the source being forwarded changes nothing: its
                // frames, if any still arrive, keep flowing (R-17).
                Some(t) if t == source => AssignEffect::None,
                Some(t) if !paused => {
                    self.state = EdgeState::SwitchPending {
                        current: source,
                        group,
                        target: t,
                        seen_at: None,
                    };
                    AssignEffect::None
                }
                _ => {
                    self.state = self.resting();
                    AssignEffect::Detach
                }
            },
            EdgeState::SwitchPending {
                current,
                group,
                target: pending,
                seen_at,
            } => match target {
                // MC reverted: the pending switch is abandoned, not queued.
                Some(t) if t == current => {
                    self.state = EdgeState::Forwarding {
                        source: current,
                        group,
                    };
                    AssignEffect::None
                }
                Some(t) if !paused => {
                    // A second switch supersedes the pending one.
                    self.state = EdgeState::SwitchPending {
                        current,
                        group,
                        target: t,
                        seen_at: if t == pending { seen_at } else { None },
                    };
                    AssignEffect::None
                }
                // To a PAUSED source, or to nothing: completes at once as
                // "forward nothing", cancelling the wait.
                _ => {
                    self.state = self.resting();
                    AssignEffect::Detach
                }
            },
            EdgeState::Waiting {
                target: old,
                seen_at,
            } => {
                self.state = match self.resting() {
                    EdgeState::Waiting { target: new, .. } if new == old => EdgeState::Waiting {
                        target: new,
                        seen_at,
                    },
                    resting => resting,
                };
                AssignEffect::None
            }
            EdgeState::Paused { .. } => {
                self.state = self.resting();
                AssignEffect::None
            }
        }
    }

    /// Decide what to do with one frame of `source` from ingress group
    /// `group`, transitioning on a group start.
    pub fn on_frame(
        &mut self,
        source: CandidateRef,
        group: GroupToken,
        independently_decodable: bool,
        now: Instant,
    ) -> FrameDecision {
        match self.state {
            EdgeState::Waiting {
                target: Some(target),
                seen_at,
            } if target == source => {
                if independently_decodable {
                    self.state = EdgeState::Forwarding { source, group };
                    FrameDecision::StartGroup
                } else {
                    self.state = EdgeState::Waiting {
                        target: Some(target),
                        seen_at: seen_at.or(Some(now)),
                    };
                    FrameDecision::Skip
                }
            }
            EdgeState::Forwarding {
                source: current,
                group: current_group,
            } if current == source => {
                if group == current_group {
                    // One uplink stream is one group: an independently
                    // decodable frame INSIDE it continues the same downlink,
                    // so egress opens never exceed (rate-limited) ingress
                    // group opens.
                    FrameDecision::Continue
                } else if independently_decodable && group > current_group {
                    self.state = EdgeState::Forwarding { source, group };
                    FrameDecision::StartGroup
                } else {
                    // A stale (older) group of the same source.
                    FrameDecision::Skip
                }
            }
            EdgeState::SwitchPending {
                current,
                group: current_group,
                target,
                seen_at,
            } => {
                if source == target {
                    if independently_decodable {
                        self.state = EdgeState::Forwarding { source, group };
                        FrameDecision::StartGroup
                    } else {
                        self.state = EdgeState::SwitchPending {
                            current,
                            group: current_group,
                            target,
                            seen_at: seen_at.or(Some(now)),
                        };
                        FrameDecision::Skip
                    }
                } else if source == current {
                    if group == current_group {
                        FrameDecision::Continue
                    } else if independently_decodable && group > current_group {
                        self.state = EdgeState::SwitchPending {
                            current,
                            group,
                            target,
                            seen_at,
                        };
                        FrameDecision::StartGroup
                    } else {
                        FrameDecision::Skip
                    }
                } else {
                    FrameDecision::Skip
                }
            }
            // A resume that beat MC's update forwards from the next group
            // start; the edge stays assigned-paused in policy terms.
            EdgeState::Paused { source: paused } if paused == source && independently_decodable => {
                self.state = EdgeState::Forwarding { source, group };
                FrameDecision::StartGroup
            }
            _ => FrameDecision::Skip,
        }
    }

    /// The current downlink was abandoned (or lost): wait for the next group
    /// start of whatever is assigned. MH never self-triggers recovery; MH-B's
    /// keyframe request rides on this Waiting state.
    pub fn on_abandoned(&mut self) {
        self.state = self.resting();
    }

    /// `source` ended (its publisher's connection closed, or MC stopped
    /// declaring it).
    pub fn on_source_ended(&mut self, source: CandidateRef) -> SourceEndedEffect {
        match self.state {
            EdgeState::Forwarding {
                source: current, ..
            } if current == source => {
                self.state = self.resting();
                SourceEndedEffect::ResetCurrent
            }
            EdgeState::SwitchPending {
                current, target, ..
            } if current == source => {
                self.state = EdgeState::Waiting {
                    target: Some(target),
                    seen_at: None,
                };
                SourceEndedEffect::ResetCurrent
            }
            // B left before its group start: A is KEPT (the slot must not go
            // dark) and the edge stays pending on B — MC still assigns B, so
            // if B returns its next group start completes the switch (and
            // MH-B's keyframe request can fire for it). Only `seen_at` resets:
            // nothing of the returning B has been seen yet.
            EdgeState::SwitchPending {
                current,
                group,
                target,
                ..
            } if target == source => {
                self.state = EdgeState::SwitchPending {
                    current,
                    group,
                    target,
                    seen_at: None,
                };
                SourceEndedEffect::None
            }
            EdgeState::Waiting {
                target: Some(target),
                ..
            } if target == source => {
                self.state = EdgeState::Waiting {
                    target: Some(target),
                    seen_at: None,
                };
                SourceEndedEffect::None
            }
            _ => SourceEndedEffect::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::{SenderId, StreamNumber};
    use std::time::Duration;

    fn src(sender: u32) -> CandidateRef {
        CandidateRef {
            sender: SenderId::from_wire(sender).unwrap(),
            stream: StreamNumber::from_wire(1).unwrap(),
        }
    }

    fn now() -> Instant {
        Instant::now()
    }

    #[test]
    fn a_first_forwarded_frame_is_always_a_group_start() {
        let mut m = EdgeMachine::new(Some(src(1)), false);
        let t = now();
        assert_eq!(m.on_frame(src(1), 7, false, t), FrameDecision::Skip);
        assert_eq!(m.waiting_with_source_seen(), Some(t), "seen while waiting");
        assert_eq!(m.on_frame(src(1), 7, false, t), FrameDecision::Skip);
        assert_eq!(m.on_frame(src(1), 8, true, t), FrameDecision::StartGroup);
        assert_eq!(m.on_frame(src(1), 8, false, t), FrameDecision::Continue);
        assert_eq!(m.waiting_with_source_seen(), None);
    }

    #[test]
    fn a_source_never_seen_does_not_read_as_seen() {
        let m = EdgeMachine::new(Some(src(1)), false);
        assert_eq!(m.waiting_with_source_seen(), None);
    }

    /// ADR §10 Tier 1a: ONE parameterised test, instant switching for a
    /// source whose every frame is independently decodable (audio-shaped) and
    /// keyframe-gated switching for one that is not (video-shaped). No branch
    /// knows which is which.
    #[test]
    fn swap_only_at_an_independently_decodable_frame_for_both_shapes() {
        for every_frame_decodable in [true, false] {
            let mut m = EdgeMachine::new(Some(src(1)), false);
            let t = now();
            assert_eq!(m.on_frame(src(1), 1, true, t), FrameDecision::StartGroup);
            m.assign(Some(src(2)), false);
            // B's first frame: a group start for the audio shape, a delta for video.
            let first_b = m.on_frame(src(2), 2, every_frame_decodable, t);
            if every_frame_decodable {
                assert_eq!(first_b, FrameDecision::StartGroup, "audio never waits");
            } else {
                assert_eq!(
                    first_b,
                    FrameDecision::Skip,
                    "video waits for B's group start"
                );
                assert_eq!(
                    m.on_frame(src(1), 1, false, t),
                    FrameDecision::Continue,
                    "A keeps forwarding while the switch is pending"
                );
                assert_eq!(m.on_frame(src(2), 3, true, t), FrameDecision::StartGroup);
            }
            assert_eq!(
                m.state(),
                EdgeState::Forwarding {
                    source: src(2),
                    group: if every_frame_decodable { 2 } else { 3 }
                }
            );
            assert_eq!(
                m.on_frame(src(1), 1, false, t),
                FrameDecision::Skip,
                "A is done"
            );
        }
    }

    #[test]
    fn a_switch_is_abandoned_and_a_kept_when_b_leaves_first() {
        let mut m = EdgeMachine::new(Some(src(1)), false);
        let t = now();
        m.on_frame(src(1), 1, true, t);
        m.assign(Some(src(2)), false);
        m.on_frame(src(2), 2, false, t);
        assert_eq!(m.on_source_ended(src(2)), SourceEndedEffect::None);
        assert_eq!(
            m.forwarding_source(),
            Some(src(1)),
            "A kept: the slot never goes dark"
        );
        assert_eq!(
            m.waiting_with_source_seen(),
            None,
            "the departed B is no longer seen"
        );
        // B returns with MC's assignment unchanged: its group start switches.
        let mut returned = m.clone();
        assert_eq!(
            returned.on_frame(src(2), 3, true, t),
            FrameDecision::StartGroup
        );
        assert_eq!(
            returned.state(),
            EdgeState::Forwarding {
                source: src(2),
                group: 3
            }
        );
        assert_eq!(m.on_frame(src(1), 1, false, t), FrameDecision::Continue);
    }

    #[test]
    fn mc_reverting_abandons_the_pending_switch() {
        let mut m = EdgeMachine::new(Some(src(1)), false);
        let t = now();
        m.on_frame(src(1), 1, true, t);
        m.assign(Some(src(2)), false);
        assert_eq!(m.assign(Some(src(1)), false), AssignEffect::None);
        assert_eq!(
            m.state(),
            EdgeState::Forwarding {
                source: src(1),
                group: 1
            }
        );
        assert_eq!(
            m.on_frame(src(2), 2, true, t),
            FrameDecision::Skip,
            "B no longer wanted"
        );
    }

    #[test]
    fn a_second_switch_supersedes_the_pending_one() {
        let mut m = EdgeMachine::new(Some(src(1)), false);
        let t = now();
        m.on_frame(src(1), 1, true, t);
        m.assign(Some(src(2)), false);
        m.on_frame(src(2), 2, false, t);
        m.assign(Some(src(3)), false);
        assert_eq!(m.waiting_with_source_seen(), None, "C has not been seen");
        assert_eq!(m.on_frame(src(2), 4, true, t), FrameDecision::Skip);
        assert_eq!(m.on_frame(src(3), 5, true, t), FrameDecision::StartGroup);
    }

    #[test]
    fn a_switch_to_a_paused_source_completes_at_once_and_cancels_the_wait() {
        let mut m = EdgeMachine::new(Some(src(1)), false);
        let t = now();
        m.on_frame(src(1), 1, true, t);
        m.assign(Some(src(2)), false);
        m.on_frame(src(2), 2, false, t);
        assert!(m.waiting_with_source_seen().is_some());
        assert_eq!(m.assign(Some(src(2)), true), AssignEffect::Detach);
        assert_eq!(m.state(), EdgeState::Paused { source: src(2) });
        assert_eq!(
            m.waiting_with_source_seen(),
            None,
            "no keyframe wait while paused"
        );
        assert_eq!(
            m.on_frame(src(1), 1, false, t),
            FrameDecision::Skip,
            "A stopped"
        );
        // A resume that beat MC's update forwards from the next group start.
        assert_eq!(m.on_frame(src(2), 3, false, t), FrameDecision::Skip);
        assert_eq!(m.on_frame(src(2), 3, true, t), FrameDecision::StartGroup);
    }

    #[test]
    fn pausing_the_forwarded_source_changes_nothing() {
        let mut m = EdgeMachine::new(Some(src(1)), false);
        let t = now();
        m.on_frame(src(1), 1, true, t);
        assert_eq!(m.assign(Some(src(1)), true), AssignEffect::None);
        assert_eq!(m.on_frame(src(1), 1, false, t), FrameDecision::Continue);
    }

    #[test]
    fn an_identical_reassignment_is_a_true_no_op() {
        let mut m = EdgeMachine::new(Some(src(1)), false);
        let t = now();
        m.on_frame(src(1), 1, false, t);
        let before = m.clone();
        assert_eq!(m.assign(Some(src(1)), false), AssignEffect::None);
        assert_eq!(m, before, "seen_at and state preserved");
    }

    #[test]
    fn abandonment_returns_to_waiting_for_the_assigned_source() {
        let mut m = EdgeMachine::new(Some(src(1)), false);
        let t = now();
        m.on_frame(src(1), 1, true, t);
        m.on_abandoned();
        assert_eq!(
            m.state(),
            EdgeState::Waiting {
                target: Some(src(1)),
                seen_at: None
            }
        );
        assert_eq!(
            m.on_frame(src(1), 1, false, t),
            FrameDecision::Skip,
            "never mid-group"
        );
        assert_eq!(m.on_frame(src(1), 2, true, t), FrameDecision::StartGroup);
    }

    #[test]
    fn abandoning_during_a_switch_waits_for_the_target() {
        let mut m = EdgeMachine::new(Some(src(1)), false);
        let t = now();
        m.on_frame(src(1), 1, true, t);
        m.assign(Some(src(2)), false);
        m.on_abandoned();
        assert_eq!(m.on_frame(src(1), 1, true, t), FrameDecision::Skip);
        assert_eq!(m.on_frame(src(2), 2, true, t), FrameDecision::StartGroup);
    }

    #[test]
    fn the_forwarded_source_ending_resets_and_waits() {
        let mut m = EdgeMachine::new(Some(src(1)), false);
        let t = now();
        m.on_frame(src(1), 1, true, t);
        assert_eq!(m.on_source_ended(src(1)), SourceEndedEffect::ResetCurrent);
        assert_eq!(m.forwarding_source(), None);
        m.on_frame(src(1), 1, true, t);
        m.assign(Some(src(2)), false);
        assert_eq!(m.on_source_ended(src(1)), SourceEndedEffect::ResetCurrent);
        assert_eq!(
            m.state(),
            EdgeState::Waiting {
                target: Some(src(2)),
                seen_at: None
            }
        );
    }

    #[test]
    fn a_stale_group_of_the_same_source_never_continues_a_newer_downlink() {
        let mut m = EdgeMachine::new(Some(src(1)), false);
        let t = now();
        m.on_frame(src(1), 1, true, t);
        m.on_frame(src(1), 2, true, t);
        assert_eq!(m.on_frame(src(1), 1, false, t), FrameDecision::Skip);
        assert_eq!(m.on_frame(src(1), 2, false, t), FrameDecision::Continue);
    }

    #[test]
    fn an_independently_decodable_frame_inside_the_same_group_continues_it() {
        let mut m = EdgeMachine::new(Some(src(1)), false);
        let t = now();
        assert_eq!(m.on_frame(src(1), 1, true, t), FrameDecision::StartGroup);
        assert_eq!(
            m.on_frame(src(1), 1, true, t),
            FrameDecision::Continue,
            "no new downlink"
        );
        m.assign(Some(src(2)), false);
        assert_eq!(
            m.on_frame(src(1), 1, true, t),
            FrameDecision::Continue,
            "pending: same group"
        );
        assert_eq!(
            m.on_frame(src(1), 0, true, t),
            FrameDecision::Skip,
            "an older group never reopens"
        );
    }

    #[test]
    fn an_edge_with_no_candidate_forwards_nothing() {
        let mut m = EdgeMachine::new(None, false);
        let t = now();
        assert_eq!(m.on_frame(src(1), 1, true, t), FrameDecision::Skip);
        m.assign(Some(src(1)), false);
        assert_eq!(m.on_frame(src(1), 1, true, t), FrameDecision::StartGroup);
        assert_eq!(m.assign(None, false), AssignEffect::Detach);
        assert_eq!(m.forwarding_source(), None);
    }

    #[test]
    fn there_is_no_give_up_timeout() {
        let mut m = EdgeMachine::new(Some(src(1)), false);
        let t = now();
        m.on_frame(src(1), 1, false, t);
        let much_later = t + Duration::from_hours(24);
        assert_eq!(
            m.on_frame(src(1), 1, false, much_later),
            FrameDecision::Skip
        );
        assert_eq!(
            m.waiting_with_source_seen(),
            Some(t),
            "first-seen kept, still waiting"
        );
        assert_eq!(
            m.on_frame(src(1), 2, true, much_later),
            FrameDecision::StartGroup
        );
    }
}
