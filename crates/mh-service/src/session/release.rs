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
    /// as a removal for connections awaiting their bind (the handler is leaving), but is
    /// counted on the teardown series, never with a `trigger`.
    NoCause,
    /// **MH-local, outside the `ReleaseCause` projection** ([`Self::of`] never
    /// produces it): the owning MC has not communicated for
    /// `MH_MEETING_OWNER_SILENCE_TIMEOUT_SECONDS` (ADR-0039 §6;
    /// `super::owner_silence`). Released exactly like a RELEASED verdict
    /// (`HANDLER_RELEASED`, no Disconnected) and counted on
    /// `mh_media_handler_releases_total{trigger="owner_silence"}`.
    OwnerSilence,
    /// **MH-local, outside the `ReleaseCause` projection**: the meeting was
    /// still held at the planned-drain deadline (ADR-0039 §7; `crate::drain`).
    /// Its sessions close `HANDLER_RELEASED`. Counted ONLY on
    /// `mh_drain_meetings_shed_total`, in bulk by the drain coordinator —
    /// never on the release or teardown series (one home per event).
    DrainShed,
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

    /// Whether connections still awaiting their bind (`NotifyParticipantConnected`
    /// in flight) are closed with `HANDLER_RELEASED` at the release
    /// (ADR-0039 §6; `EndMeetingRequest` SEMANTICS): yes when the handler is
    /// leaving the meeting; no when the meeting ended — they are left to the
    /// bind outcome, and an answer for a meeting MH no longer holds binds
    /// nothing and closes `DECLINED`.
    #[must_use]
    pub const fn closes_awaiting_bind(self) -> bool {
        match self {
            Self::Removal(_) | Self::NoCause | Self::OwnerSilence | Self::DrainShed => true,
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
    fn only_meeting_ended_leaves_awaiting_bind_connections_to_the_bind() {
        assert!(!ReleaseKind::MeetingEnded.closes_awaiting_bind());
        assert!(ReleaseKind::NoCause.closes_awaiting_bind());
        // The MH-local kinds: the handler is leaving the meeting.
        assert!(ReleaseKind::OwnerSilence.closes_awaiting_bind());
        assert!(ReleaseKind::DrainShed.closes_awaiting_bind());
        for trigger in RemovalTrigger::ALL {
            assert!(ReleaseKind::Removal(trigger).closes_awaiting_bind());
        }
    }
}
