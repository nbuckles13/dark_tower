//! Meeting-lifecycle facts shared by GC and MC.
//!
//! These are values one service DEFINES and the other must stay inside. Each
//! lives here once so the two cannot drift (CLAUDE.md "Single source of
//! truth").

use std::time::Duration;

/// The SHORTEST join window MC accepts (`MC_MEETING_JOIN_WINDOW_SECONDS`
/// lower bound): a meeting MC creates on GC's assignment and that no
/// participant joins ends no sooner than this after MC accepted the
/// assignment.
///
/// GC depends on it. GC calls MC's `AssignMeetingWithMh` BEFORE it writes the
/// assignment row (`atomic_assign`). If that write commits only after MC has
/// already ended the meeting at its join window, MC's fenced
/// `NotifyMeetingEnded` found no row to end, and the row GC then writes points
/// at a live MC that does not hold the meeting: every join to that id fails
/// until the MC dies. GC therefore bounds accept->commit by
/// `GC_ASSIGN_COMMIT_BUDGET_MS`, validated at startup to at most HALF of this
/// value. The other half is margin for what the budget cannot police:
/// `statement_timeout` cannot cancel a COMMIT already being written, and it
/// does not cover network time to and from the database or MC's own
/// accept-to-reply latency (ADR-0010 §3, "MC failure (backstop)" residuals).
pub const MIN_MEETING_JOIN_WINDOW: Duration = Duration::from_secs(5);

/// One INCARNATION of a meeting's (`meeting_id`, region) assignment row
/// (`AssignMeetingWithMhRequest.assignment_id`, `NotifyMeetingEndedRequest
/// .assignment_id`; rules in `internal.proto`).
///
/// GC mints a fresh value per `AssignMeetingWithMh` attempt and writes it to
/// the row only for the attempt MC accepted; MC echoes it when the meeting
/// ends, and GC ends only the matching row. A non-secret discriminator, NOT a
/// credential. Both services parse the wire value through [`Self::parse`] —
/// one definition of "valid", so GC's `INVALID_ARGUMENT` and MC's
/// `INVALID_REQUEST` cannot disagree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AssignmentId(uuid::Uuid);

/// Why a wire `assignment_id` was refused. `Malformed` KEEPS `uuid`'s error as
/// its `source` (wrong length / bad character / bad group layout) so a GC↔MC
/// contract defect is diagnosable, while its `Display` names only the input
/// LENGTH — never input bytes — so `%e` is safe in a log line or a gRPC
/// status (security: no raw input echoed).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AssignmentIdError {
    /// The field was empty (an unset proto3 string).
    #[error("assignment_id is empty")]
    Empty,
    /// Not a UUID.
    #[error("assignment_id is not a UUID (length {len})")]
    Malformed {
        /// Byte length of the refused value.
        len: usize,
        /// `uuid`'s reason (may name one offending character).
        #[source]
        source: uuid::Error,
    },
    /// The nil UUID: the "unset" look-alike, never minted.
    #[error("assignment_id is the nil UUID")]
    Nil,
}

impl AssignmentId {
    /// Mint a fresh incarnation id (`UUIDv4`, OS CSPRNG). GC only.
    #[must_use]
    pub fn new_v4() -> Self {
        Self(uuid::Uuid::new_v4())
    }

    /// Parse a wire value: empty, malformed and nil are refused.
    ///
    /// # Errors
    ///
    /// [`AssignmentIdError`] naming which rule failed (no input echoed).
    pub fn parse(raw: &str) -> Result<Self, AssignmentIdError> {
        if raw.is_empty() {
            return Err(AssignmentIdError::Empty);
        }
        let id = uuid::Uuid::parse_str(raw).map_err(|source| AssignmentIdError::Malformed {
            len: raw.len(),
            source,
        })?;
        if id.is_nil() {
            return Err(AssignmentIdError::Nil);
        }
        Ok(Self(id))
    }

    /// From a value already stored as a UUID (GC's column). Nil is refused.
    ///
    /// # Errors
    ///
    /// [`AssignmentIdError::Nil`].
    pub fn from_uuid(id: uuid::Uuid) -> Result<Self, AssignmentIdError> {
        if id.is_nil() {
            Err(AssignmentIdError::Nil)
        } else {
            Ok(Self(id))
        }
    }

    /// The UUID value (for binding to the database column).
    #[must_use]
    pub fn as_uuid(&self) -> uuid::Uuid {
        self.0
    }
}

/// Canonical wire form: lowercase hyphenated.
impl std::fmt::Display for AssignmentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.hyphenated().fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_refuses_empty_malformed_and_nil() {
        assert_eq!(AssignmentId::parse(""), Err(AssignmentIdError::Empty));
        assert!(matches!(
            AssignmentId::parse("not-a-uuid"),
            Err(AssignmentIdError::Malformed { len: 10, .. })
        ));
        assert_eq!(
            AssignmentId::parse("00000000-0000-0000-0000-000000000000"),
            Err(AssignmentIdError::Nil)
        );
        assert_eq!(
            AssignmentId::from_uuid(uuid::Uuid::nil()),
            Err(AssignmentIdError::Nil)
        );
    }

    #[test]
    fn display_round_trips_through_parse_and_compares_by_value() {
        let id = AssignmentId::new_v4();
        assert_eq!(AssignmentId::parse(&id.to_string()), Ok(id));
        // A case variant of the same value is the SAME id (value compare).
        let upper = id.to_string().to_uppercase();
        assert_eq!(AssignmentId::parse(&upper), Ok(id));
        assert_ne!(AssignmentId::new_v4(), id);
    }
}
