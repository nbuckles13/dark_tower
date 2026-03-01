//! Meetings repository for database operations.
//!
//! Provides meeting creation with atomic org limit enforcement
//! and audit logging.
//!
//! # Security
//!
//! - Atomic CTE prevents TOCTOU race on concurrent meeting limit
//! - All queries use parameterized statements (SQL injection safe)
//! - Audit log failures are logged but don't block meeting creation

use crate::errors::GcError;
use crate::models::MeetingRow;
use crate::observability::metrics;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
use std::time::Instant;
use tracing::instrument;
use uuid::Uuid;

/// Meetings repository for database operations.
pub struct MeetingsRepository;

impl MeetingsRepository {
    /// Create a meeting with atomic org concurrent meeting limit check.
    ///
    /// Uses a single CTE query that atomically:
    /// 1. Fetches org limits (max_concurrent_meetings, max_participants_per_meeting)
    /// 2. Counts active/scheduled meetings for the org
    /// 3. Inserts the meeting only if under the limit
    /// 4. Caps max_participants at the org's max_participants_per_meeting
    ///
    /// Returns `Some(MeetingRow)` on success, `None` if org limit exceeded.
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
    ) -> Result<Option<MeetingRow>, GcError> {
        let start = Instant::now();

        let row = sqlx::query(
            r#"
            WITH org_limits AS (
                SELECT max_concurrent_meetings, max_participants_per_meeting
                FROM organizations
                WHERE org_id = $1 AND is_active = true
            ),
            current_count AS (
                SELECT COUNT(*) as cnt
                FROM meetings
                WHERE org_id = $1 AND status IN ('scheduled', 'active')
            )
            INSERT INTO meetings (
                org_id, created_by_user_id, display_name, meeting_code,
                join_token_secret, max_participants, enable_e2e_encryption,
                require_auth, recording_enabled, allow_guests,
                allow_external_participants, waiting_room_enabled,
                scheduled_start_time, status
            )
            SELECT
                $1, $2, $3, $4, $5,
                LEAST($6, org_limits.max_participants_per_meeting),
                $7, $8, $9, $10, $11, $12, $13,
                'scheduled'
            FROM org_limits, current_count
            WHERE current_count.cnt < org_limits.max_concurrent_meetings
            RETURNING
                meeting_id, org_id, created_by_user_id, display_name,
                meeting_code, join_token_secret, max_participants,
                enable_e2e_encryption, require_auth, recording_enabled,
                meeting_controller_id, meeting_controller_region,
                status, scheduled_start_time, actual_start_time,
                actual_end_time, created_at, updated_at,
                allow_guests, allow_external_participants, waiting_room_enabled
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
        .fetch_optional(pool)
        .await
        .map_err(|e| {
            let duration = start.elapsed();
            metrics::record_db_query("create_meeting", "error", duration);
            GcError::Database(e.to_string())
        })?;

        let duration = start.elapsed();
        metrics::record_db_query("create_meeting", "success", duration);

        match row {
            Some(row) => Ok(Some(map_row_to_meeting(row))),
            None => Ok(None),
        }
    }

    /// Log a meeting creation audit event.
    ///
    /// Fire-and-forget: failures are logged at warn level but do not
    /// block meeting creation.
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `org_id` - Organization UUID
    /// * `user_id` - User UUID who created the meeting
    /// * `meeting_id` - Created meeting UUID
    #[instrument(skip_all, name = "gc.repo.log_audit_event")]
    pub async fn log_audit_event(
        pool: &PgPool,
        org_id: Uuid,
        user_id: Uuid,
        meeting_id: Uuid,
    ) -> Result<(), GcError> {
        let start = Instant::now();

        sqlx::query(
            r#"
            INSERT INTO audit_logs (org_id, user_id, action, resource_type, resource_id, details)
            VALUES ($1, $2, 'meeting_created', 'meeting', $3, $4)
            "#,
        )
        .bind(org_id)
        .bind(user_id)
        .bind(meeting_id)
        .bind(serde_json::json!({"action": "meeting_created"}))
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
}

/// Map a database row to a MeetingRow struct.
///
/// Shared by all queries that return meeting rows to avoid
/// field-by-field mapping duplication.
pub fn map_row_to_meeting(row: sqlx::postgres::PgRow) -> MeetingRow {
    MeetingRow {
        meeting_id: row.get("meeting_id"),
        org_id: row.get("org_id"),
        created_by_user_id: row.get("created_by_user_id"),
        display_name: row.get("display_name"),
        meeting_code: row.get("meeting_code"),
        join_token_secret: row.get("join_token_secret"),
        max_participants: row.get("max_participants"),
        enable_e2e_encryption: row.get("enable_e2e_encryption"),
        require_auth: row.get("require_auth"),
        recording_enabled: row.get("recording_enabled"),
        meeting_controller_id: row.get("meeting_controller_id"),
        meeting_controller_region: row.get("meeting_controller_region"),
        status: row.get("status"),
        scheduled_start_time: row.get("scheduled_start_time"),
        actual_start_time: row.get("actual_start_time"),
        actual_end_time: row.get("actual_end_time"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        allow_guests: row.get("allow_guests"),
        allow_external_participants: row.get("allow_external_participants"),
        waiting_room_enabled: row.get("waiting_room_enabled"),
    }
}
