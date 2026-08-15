//! Meetings repository for database operations.
//!
//! Provides meeting creation with org limit enforcement and audit logging.
//!
//! # Security
//!
//! - The creation CTE performs the limit check and the INSERT in a single
//!   statement, so there is no application-level check-then-insert window.
//!   See [`MeetingsRepository::create_meeting_with_limit_check`] for the
//!   precise (and narrower than it sounds) concurrency guarantee.
//! - All queries use parameterized statements (SQL injection safe)
//! - Audit log failures are logged but don't block meeting creation

use crate::errors::GcError;
use crate::models::MeetingRow;
use crate::observability::metrics;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
use std::time::Instant;
use tracing::{error, instrument};
use uuid::Uuid;

/// SQLSTATE for `unique_violation`.
const SQLSTATE_UNIQUE_VIOLATION: &str = "23505";

/// Unique constraint on `(org_id, meeting_code)`.
///
/// Declared as `CONSTRAINT meetings_org_code_unique UNIQUE (org_id, meeting_code)`
/// in `migrations/20250118000001_initial_schema.sql`. Pinned by
/// `tests/db_metrics_integration.rs`, which asserts a duplicate meeting code
/// classifies as [`CreateMeetingOutcome::MeetingCodeTaken`] — so a rename in the
/// migration reds a test rather than silently degrading collisions into 500s.
const CONSTRAINT_MEETING_CODE_UNIQUE: &str = "meetings_org_code_unique";

/// Why the database refused to create a meeting.
///
/// One home for the refusal taxonomy. The three causes are unrelated — a full
/// cap is a routine policy refusal, while the other two mean the organization
/// row is in a state no application flow produces — so collapsing them (as this
/// repository did until story R-6) makes a broken organization indistinguishable
/// from a busy one.
///
/// Ordering of the variants matches the reporting precedence documented on
/// [`MeetingsRepository::create_meeting_with_limit_check`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeetingRefusal {
    /// No `organizations` row for this `org_id`.
    ///
    /// A valid, correctly-signed token naming an organization that does not
    /// exist: a referential-integrity violation the schema cannot express,
    /// because a JWT is not a foreign key.
    OrganizationNotProvisioned,

    /// The `organizations` row exists with `is_active = false`.
    ///
    /// A modeled state — `is_active` is a designed column whose semantics the
    /// schema and AC's queries both honour.
    OrganizationInactive,

    /// The organization is active but already at `max_concurrent_meetings`.
    CapacityExhausted,
}

impl MeetingRefusal {
    /// Bounded `error_type` metric label for this cause (ADR-0011).
    ///
    /// Returns `&'static str` by construction, so no unbounded value can reach
    /// a label position. These strings are also the `CASE` discriminants the
    /// creation query emits and the values in
    /// `docs/observability/metrics/gc-service.md`; the round-trip is pinned by
    /// `discriminant_and_metric_label_round_trip` below.
    pub fn metric_label(self) -> &'static str {
        match self {
            MeetingRefusal::OrganizationNotProvisioned => "org_not_provisioned",
            MeetingRefusal::OrganizationInactive => "org_inactive",
            MeetingRefusal::CapacityExhausted => "org_limit",
        }
    }

    /// Parse the `outcome` discriminant emitted by the creation query.
    ///
    /// Returns `None` for `'created'` (not a refusal) and for any unrecognised
    /// value; callers must treat the latter as an error class, never as a
    /// business cause.
    fn from_discriminant(discriminant: &str) -> Option<Self> {
        match discriminant {
            "org_not_provisioned" => Some(MeetingRefusal::OrganizationNotProvisioned),
            "org_inactive" => Some(MeetingRefusal::OrganizationInactive),
            "org_limit" => Some(MeetingRefusal::CapacityExhausted),
            _ => None,
        }
    }
}

/// Outcome of a meeting-creation attempt.
///
/// Replaces the `Option<MeetingRow>` that made "refused" a single
/// indistinguishable state (story R-6).
#[derive(Debug)]
pub enum CreateMeetingOutcome {
    /// The meeting was inserted.
    Created(Box<MeetingRow>),

    /// The database declined to insert, for the carried reason.
    Refused(MeetingRefusal),

    /// The generated `meeting_code` was already taken for this org.
    ///
    /// Not a refusal: the caller is expected to retry with a fresh code.
    MeetingCodeTaken,
}

/// Classification of an error raised by the creation statement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InsertErrorClass {
    /// Unique violation on the org/meeting-code constraint — retryable.
    MeetingCodeTaken,
    /// Anything else — surfaced to the caller as a database error.
    Other,
}

/// Classify a creation-statement failure from its SQLSTATE and constraint name.
///
/// A pure function over the two fields that carry the decision, so both inputs
/// are unit-testable without a database. Replaces matching on the English text
/// of the driver's error message, which changes with driver and server locale.
///
/// A `23505` on any *other* constraint is [`InsertErrorClass::Other`]: retrying
/// with a fresh meeting code cannot clear it, so retrying would burn attempts
/// and then report the wrong cause.
fn classify_insert_error(sqlstate: Option<&str>, constraint: Option<&str>) -> InsertErrorClass {
    match (sqlstate, constraint) {
        (Some(SQLSTATE_UNIQUE_VIOLATION), Some(CONSTRAINT_MEETING_CODE_UNIQUE)) => {
            InsertErrorClass::MeetingCodeTaken
        }
        _ => InsertErrorClass::Other,
    }
}

/// Meetings repository for database operations.
pub struct MeetingsRepository;

impl MeetingsRepository {
    /// Create a meeting, enforcing the organization's concurrent-meeting limit.
    ///
    /// Uses a single CTE query that:
    /// 1. Fetches the org row (limits and `is_active`)
    /// 2. Counts scheduled/active meetings for the org
    /// 3. Inserts the meeting only if the org is active and under the limit
    /// 4. Caps max_participants at the org's max_participants_per_meeting
    /// 5. Reports which of those conditions refused the insert
    ///
    /// # Return value
    ///
    /// Returns [`CreateMeetingOutcome`]. The statement always returns exactly
    /// one row, so "no rows" is not a representable state — which is what let
    /// three unrelated causes collapse into one `None` before story R-6.
    ///
    /// # Reporting precedence
    ///
    /// When more than one condition holds, the cause reported is the first of:
    /// `OrganizationNotProvisioned` → `OrganizationInactive` → `CapacityExhausted`.
    /// An inactive org that is *also* at its cap reports inactive: organization
    /// state is the more actionable fact, and reporting it first is precisely
    /// what stops a broken organization from looking like a busy one.
    ///
    /// # Concurrency
    ///
    /// Executing the check and the INSERT as one statement removes the
    /// application-level check-then-insert window: there is no separate `SELECT`
    /// round trip during which the count can change. It does **not** serialise
    /// concurrent executions of this statement — under READ COMMITTED
    /// `current_count` takes no locks, so N concurrent creates against a cap of
    /// M can overshoot to M+N-1. The cap is advisory, not enforced by a
    /// constraint. See `docs/TODO.md` (Database) for the tracked fix; note it
    /// is scoped to the *pattern*, since `participants.rs` names this function
    /// as the model for the future join-capacity CTE, where an overshoot on
    /// participant capacity matters more than on a soft meeting count.
    ///
    /// Because the diagnostic branches read the same `org_row` CTE the INSERT
    /// consulted, under one snapshot, the reported cause cannot disagree with
    /// the decision the INSERT actually made.
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `org_id` - Organization UUID
    /// * `created_by_user_id` - User UUID from token
    /// * `display_name` - Meeting display name (already trimmed)
    /// * `meeting_code` - Generated meeting code (12 base62 chars)
    /// * `join_token_secret` - Generated join token secret (hex-encoded)
    /// * `max_participants` - Requested max participants
    /// * `enable_e2e_encryption` - E2E encryption setting
    /// * `require_auth` - Auth required setting
    /// * `recording_enabled` - Recording setting
    /// * `allow_guests` - Guest access setting
    /// * `allow_external_participants` - External participants setting
    /// * `waiting_room_enabled` - Waiting room setting
    /// * `scheduled_start_time` - Optional scheduled start time
    #[instrument(skip_all, name = "gc.repo.create_meeting")]
    #[expect(
        clippy::too_many_arguments,
        reason = "Represents all meeting table columns for atomic INSERT"
    )]
    pub async fn create_meeting_with_limit_check(
        pool: &PgPool,
        org_id: Uuid,
        created_by_user_id: Uuid,
        display_name: &str,
        meeting_code: &str,
        join_token_secret: &str,
        max_participants: i32,
        enable_e2e_encryption: bool,
        require_auth: bool,
        recording_enabled: bool,
        allow_guests: bool,
        allow_external_participants: bool,
        waiting_room_enabled: bool,
        scheduled_start_time: Option<DateTime<Utc>>,
    ) -> Result<CreateMeetingOutcome, GcError> {
        let start = Instant::now();

        let row = sqlx::query(
            r#"
            WITH org_row AS (
                SELECT org_id, is_active,
                       max_concurrent_meetings, max_participants_per_meeting
                FROM organizations
                WHERE org_id = $1
            ),
            current_count AS (
                SELECT COUNT(*) as cnt
                FROM meetings
                WHERE org_id = $1 AND status IN ('scheduled', 'active')
            ),
            inserted AS (
                INSERT INTO meetings (
                    org_id, created_by_user_id, display_name, meeting_code,
                    join_token_secret, max_participants, enable_e2e_encryption,
                    require_auth, recording_enabled, allow_guests,
                    allow_external_participants, waiting_room_enabled,
                    scheduled_start_time, status
                )
                SELECT
                    $1, $2, $3, $4, $5,
                    LEAST($6, o.max_participants_per_meeting),
                    $7, $8, $9, $10, $11, $12, $13,
                    'scheduled'
                -- INNER join, deliberately: LEAST($6, NULL) returns $6 in
                -- Postgres, so a LEFT JOIN here would silently uncap
                -- max_participants for an organization that does not exist.
                -- The LEFT JOINs below are diagnostic only and never feed the
                -- inserted values.
                FROM org_row o, current_count c
                WHERE o.is_active AND c.cnt < o.max_concurrent_meetings
                RETURNING
                    meeting_id, org_id, created_by_user_id, display_name,
                    meeting_code, join_token_secret, max_participants,
                    enable_e2e_encryption, require_auth, recording_enabled,
                    meeting_controller_id, meeting_controller_region,
                    status, scheduled_start_time, actual_start_time,
                    actual_end_time, created_at, updated_at,
                    allow_guests, allow_external_participants, waiting_room_enabled
            )
            -- Always exactly one row: a one-row anchor left-joined to two
            -- at-most-one-row relations. The discriminants below are the
            -- MeetingRefusal::metric_label() values verbatim.
            SELECT
                CASE
                    WHEN i.meeting_id IS NOT NULL THEN 'created'
                    WHEN o.org_id IS NULL         THEN 'org_not_provisioned'
                    WHEN NOT o.is_active          THEN 'org_inactive'
                    ELSE 'org_limit'
                END AS outcome,
                i.*
            FROM (SELECT 1) AS anchor
            LEFT JOIN org_row  o ON true
            LEFT JOIN inserted i ON true
            "#,
        )
        .bind(org_id) // $1
        .bind(created_by_user_id) // $2
        .bind(display_name) // $3
        .bind(meeting_code) // $4
        .bind(join_token_secret) // $5
        .bind(max_participants) // $6
        .bind(enable_e2e_encryption) // $7
        .bind(require_auth) // $8
        .bind(recording_enabled) // $9
        .bind(allow_guests) // $10
        .bind(allow_external_participants) // $11
        .bind(waiting_room_enabled) // $12
        .bind(scheduled_start_time) // $13
        .fetch_one(pool)
        .await;

        let row = match row {
            Ok(row) => row,
            Err(e) => {
                // The statement failed: record "error" regardless of how the
                // failure classifies. A meeting-code collision is a failed
                // statement that the caller retries, not a successful query.
                let duration = start.elapsed();
                metrics::record_db_query("create_meeting", "error", duration);

                let db_err = e.as_database_error();
                let class = classify_insert_error(
                    db_err.and_then(|d| d.code()).as_deref(),
                    db_err.and_then(|d| d.constraint()),
                );
                return match class {
                    InsertErrorClass::MeetingCodeTaken => {
                        Ok(CreateMeetingOutcome::MeetingCodeTaken)
                    }
                    // Includes `RowNotFound`: the statement is built to return
                    // exactly one row, so its absence means that invariant is
                    // broken. Report it as the error it is — never as a
                    // business cause.
                    InsertErrorClass::Other => Err(GcError::Database(e.to_string())),
                };
            }
        };

        let duration = start.elapsed();
        metrics::record_db_query("create_meeting", "success", duration);

        let outcome: String = row.try_get("outcome")?;
        if outcome == "created" {
            return Ok(CreateMeetingOutcome::Created(Box::new(map_row_to_meeting(
                row,
            )?)));
        }

        match MeetingRefusal::from_discriminant(&outcome) {
            Some(refusal) => Ok(CreateMeetingOutcome::Refused(refusal)),
            None => {
                // Query and enum have drifted. Fail loudly as an error class:
                // bucketing this into any business cause would recreate the
                // exact defect R-6 removed, in a form that looks deliberate.
                error!(
                    target: "gc.repo.meetings",
                    outcome = %outcome,
                    "create_meeting returned an unrecognised outcome discriminant"
                );
                Err(GcError::Database(format!(
                    "create_meeting returned an unrecognised outcome discriminant: {outcome}"
                )))
            }
        }
    }

    /// Log a meeting audit event.
    ///
    /// Fire-and-forget: failures are logged at warn level but do not
    /// block the calling operation.
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `org_id` - Organization UUID
    /// * `user_id` - User UUID who triggered the action (None for guests)
    /// * `meeting_id` - Meeting UUID
    /// * `action` - Audit action string (e.g., "meeting_created", "meeting_activated")
    #[instrument(skip_all, name = "gc.repo.log_audit_event", fields(action = %action))]
    pub async fn log_audit_event(
        pool: &PgPool,
        org_id: Uuid,
        user_id: Option<Uuid>,
        meeting_id: Uuid,
        action: &str,
    ) -> Result<(), GcError> {
        let start = Instant::now();

        sqlx::query(
            r#"
            INSERT INTO audit_logs (org_id, user_id, action, resource_type, resource_id, details)
            VALUES ($1, $2, $3, 'meeting', $4, $5)
            "#,
        )
        .bind(org_id)
        .bind(user_id)
        .bind(action)
        .bind(meeting_id)
        .bind(serde_json::json!({"action": action}))
        .execute(pool)
        .await
        .map_err(|e| {
            let duration = start.elapsed();
            metrics::record_db_query("log_audit_event", "error", duration);
            GcError::Database(e.to_string())
        })?;

        let duration = start.elapsed();
        metrics::record_db_query("log_audit_event", "success", duration);

        Ok(())
    }

    /// Activate a meeting on first participant join.
    ///
    /// Atomically transitions the meeting status from `scheduled` to `active`
    /// and sets `actual_start_time`. The `WHERE status = 'scheduled'` clause
    /// ensures idempotency — if the meeting is already `active`, `ended`, or
    /// `cancelled`, this is a no-op returning `None`.
    ///
    /// Concurrency-safe: only one concurrent caller will successfully transition
    /// the row; others will see zero rows affected and get `None`.
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `meeting_id` - Meeting UUID to activate
    ///
    /// # Returns
    ///
    /// `Some((meeting_id, org_id))` if the meeting was activated (was `scheduled`),
    /// `None` if no transition occurred (already `active`/`ended`/`cancelled`).
    #[allow(dead_code)] // Used by integration tests and future join handler
    #[instrument(skip_all, name = "gc.repo.activate_meeting", fields(meeting_id = %meeting_id))]
    pub async fn activate_meeting(
        pool: &PgPool,
        meeting_id: Uuid,
    ) -> Result<Option<(Uuid, Uuid)>, GcError> {
        let start = Instant::now();

        let row: Option<(Uuid, Uuid)> = sqlx::query_as(
            r#"
            UPDATE meetings
            SET status = 'active', actual_start_time = NOW()
            WHERE meeting_id = $1 AND status = 'scheduled'
            RETURNING meeting_id, org_id
            "#,
        )
        .bind(meeting_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| {
            let duration = start.elapsed();
            metrics::record_db_query("activate_meeting", "error", duration);
            GcError::Database(e.to_string())
        })?;

        let duration = start.elapsed();
        metrics::record_db_query("activate_meeting", "success", duration);

        Ok(row)
    }
}

/// Map a database row to a MeetingRow struct.
///
/// Shared by all queries that return meeting rows to avoid
/// field-by-field mapping duplication.
///
/// Uses `try_get` rather than `get`: `get` panics on a decode failure, and the
/// creation query's diagnostic projection returns a row whose meeting columns
/// are all NULL on every refusal path. That row is unreachable here by `CASE`-arm
/// discipline alone, and "unreachable by construction" is exactly the argument
/// this module refuses elsewhere (see the unrecognised-discriminant arm above),
/// so the decode failure is returned rather than panicked (ADR-0002).
pub fn map_row_to_meeting(row: sqlx::postgres::PgRow) -> Result<MeetingRow, GcError> {
    Ok(MeetingRow {
        meeting_id: row.try_get("meeting_id")?,
        org_id: row.try_get("org_id")?,
        created_by_user_id: row.try_get("created_by_user_id")?,
        display_name: row.try_get("display_name")?,
        meeting_code: row.try_get("meeting_code")?,
        join_token_secret: row.try_get("join_token_secret")?,
        max_participants: row.try_get("max_participants")?,
        enable_e2e_encryption: row.try_get("enable_e2e_encryption")?,
        require_auth: row.try_get("require_auth")?,
        recording_enabled: row.try_get("recording_enabled")?,
        meeting_controller_id: row.try_get("meeting_controller_id")?,
        meeting_controller_region: row.try_get("meeting_controller_region")?,
        status: row.try_get("status")?,
        scheduled_start_time: row.try_get("scheduled_start_time")?,
        actual_start_time: row.try_get("actual_start_time")?,
        actual_end_time: row.try_get("actual_end_time")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        allow_guests: row.try_get("allow_guests")?,
        allow_external_participants: row.try_get("allow_external_participants")?,
        waiting_room_enabled: row.try_get("waiting_room_enabled")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every variant's label, asserted individually.
    ///
    /// This is the link the wrapper-level `ALL_ERROR_TYPES` mirror in
    /// `tests/meeting_creation_metrics_integration.rs` cannot cover: that test
    /// hands the recording wrapper a string literal, so it would still pass if
    /// this mapping changed underneath it (@observability, O-7).
    #[test]
    fn metric_label_per_variant() {
        assert_eq!(
            MeetingRefusal::OrganizationNotProvisioned.metric_label(),
            "org_not_provisioned"
        );
        assert_eq!(
            MeetingRefusal::OrganizationInactive.metric_label(),
            "org_inactive"
        );
        assert_eq!(
            MeetingRefusal::CapacityExhausted.metric_label(),
            "org_limit"
        );
    }

    /// The SQL `CASE` discriminants and the metric label values are the same
    /// strings. That makes either side greppable from the other, but it also
    /// means a future edit that bypassed `metric_label()` and passed the raw
    /// discriminant into a label position would emit exactly the right value
    /// and be undetectable by observation. This pins the coincidence as
    /// intentional, so un-aligning either side reds a test (@database).
    #[test]
    fn discriminant_and_metric_label_round_trip() {
        for variant in [
            MeetingRefusal::OrganizationNotProvisioned,
            MeetingRefusal::OrganizationInactive,
            MeetingRefusal::CapacityExhausted,
        ] {
            let label = variant.metric_label();
            assert_eq!(
                MeetingRefusal::from_discriminant(label),
                Some(variant),
                "discriminant {label} must parse back to the variant that emits it"
            );
        }
    }

    #[test]
    fn unknown_discriminant_is_not_a_refusal() {
        assert_eq!(MeetingRefusal::from_discriminant("created"), None);
        assert_eq!(MeetingRefusal::from_discriminant("cap_exhausted"), None);
        assert_eq!(MeetingRefusal::from_discriminant(""), None);
    }

    #[test]
    fn unique_violation_on_meeting_code_is_retryable() {
        assert_eq!(
            classify_insert_error(Some("23505"), Some("meetings_org_code_unique")),
            InsertErrorClass::MeetingCodeTaken
        );
    }

    /// A unique violation on a *different* constraint must not be retried:
    /// a fresh meeting code cannot clear it, so retrying burns the attempts and
    /// then reports the wrong cause.
    #[test]
    fn unique_violation_on_other_constraint_is_not_retryable() {
        assert_eq!(
            classify_insert_error(Some("23505"), Some("meetings_pkey")),
            InsertErrorClass::Other
        );
    }

    #[test]
    fn non_unique_violations_are_not_retryable() {
        // Foreign-key violation on the constraint we do retry on: the SQLSTATE
        // half of the pair is load-bearing, not decoration.
        assert_eq!(
            classify_insert_error(Some("23503"), Some("meetings_org_code_unique")),
            InsertErrorClass::Other
        );
        assert_eq!(
            classify_insert_error(Some("23505"), None),
            InsertErrorClass::Other
        );
        assert_eq!(classify_insert_error(None, None), InsertErrorClass::Other);
    }
}
