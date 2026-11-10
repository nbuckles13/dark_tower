//! Why MH releases a meeting: the ONE projection from the wire `ReleaseCause`
//! (story 4 MH-2; ADR-0039 §6 "One enum").
//!
//! Two carriers bring a cause to MH and both go through [`ReleaseKind::of`]:
//! `EndMeetingRequest.cause` and the RELEASED heartbeat verdict's
//! `MeetingHeartbeatResponse.cause`. They differ only in what a missing cause
//! MEANS, and that difference is stated at the carrier, not by a second
//! projection: on `EndMeeting` a [`ReleaseKind::NoCause`] is refused
//! `INVALID_ARGUMENT`; on a RELEASED verdict it is MC's documented misroute,
//! which still ends the meeting.
//!
//! The `trigger` label values themselves have one fleet-wide home,
//! [`RemovalTrigger`] in `crates/common`. This projection is MH's own (GC's
//! `project_trigger` classifies the same enum for its own carrier).

use common::observability::labels::RemovalTrigger;
use proto_gen::dark_tower::internal::v1::ReleaseCause;

/// What a release cause means to MH.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseKind {
    /// MC removed this handler from the meeting. Counted on
    /// `mh_media_handler_releases_total{trigger}`; connections still awaiting
    /// their bind are closed with `HANDLER_RELEASED` (the handler is leaving).
    Removal(RemovalTrigger),
    /// The meeting itself ended. Counted on
    /// `mh_media_meeting_teardowns_total{outcome="released"}`; connections
    /// still awaiting their bind are left to the bind outcome.
    MeetingEnded,
    /// No valid cause: `UNSPECIFIED`, or a number outside `ReleaseCause`
    /// (proto3 enums are open, so an unknown number decodes). Refused on
    /// `EndMeeting`; on a RELEASED verdict it is the misroute and is treated
    /// as a removal for pending connections (the handler is leaving), but is
    /// counted on the teardown series, never with a `trigger`.
    NoCause,
}

impl ReleaseKind {
    /// The ONE projection from a wire cause. Exhaustive with no wildcard, so a
    /// new `ReleaseCause` value is a compile error here until it is
    /// classified.
    #[must_use]
    pub fn of(raw_cause: i32) -> Self {
        let Ok(cause) = ReleaseCause::try_from(raw_cause) else {
            return Self::NoCause;
        };
        match cause {
            ReleaseCause::HeartbeatTimeout => Self::Removal(RemovalTrigger::HeartbeatTimeout),
            ReleaseCause::ApplyLag => Self::Removal(RemovalTrigger::ApplyLag),
            ReleaseCause::PlannedTransfer => Self::Removal(RemovalTrigger::PlannedTransfer),
            ReleaseCause::MeetingEnded => Self::MeetingEnded,
            ReleaseCause::Unspecified => Self::NoCause,
        }
    }

    /// Whether connections still awaiting their bind are closed with
    /// `HANDLER_RELEASED` (`EndMeetingRequest` SEMANTICS): yes when the
    /// handler is leaving the meeting, no when the meeting ended (one may be a
    /// re-join racing the teardown, which the next registration rescues).
    #[must_use]
    pub const fn closes_pending(self) -> bool {
        match self {
            Self::Removal(_) | Self::NoCause => true,
            Self::MeetingEnded => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_projection_is_total_over_release_cause_and_unknown_numbers() {
        assert_eq!(
            ReleaseKind::of(ReleaseCause::HeartbeatTimeout as i32),
            ReleaseKind::Removal(RemovalTrigger::HeartbeatTimeout)
        );
        assert_eq!(
            ReleaseKind::of(ReleaseCause::ApplyLag as i32),
            ReleaseKind::Removal(RemovalTrigger::ApplyLag)
        );
        assert_eq!(
            ReleaseKind::of(ReleaseCause::PlannedTransfer as i32),
            ReleaseKind::Removal(RemovalTrigger::PlannedTransfer)
        );
        assert_eq!(
            ReleaseKind::of(ReleaseCause::MeetingEnded as i32),
            ReleaseKind::MeetingEnded
        );
        assert_eq!(
            ReleaseKind::of(ReleaseCause::Unspecified as i32),
            ReleaseKind::NoCause
        );
        // An unknown number is never read as a removal or as meeting-ended.
        for unknown in [99, -1, i32::MAX] {
            assert_eq!(ReleaseKind::of(unknown), ReleaseKind::NoCause, "{unknown}");
        }
    }

    #[test]
    fn only_meeting_ended_leaves_pending_connections() {
        assert!(!ReleaseKind::MeetingEnded.closes_pending());
        assert!(ReleaseKind::NoCause.closes_pending());
        for trigger in RemovalTrigger::ALL {
            assert!(ReleaseKind::Removal(trigger).closes_pending());
        }
    }
}
