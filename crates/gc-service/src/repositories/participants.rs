//! Participants repository for database operations.
//!
//! Provides operations for tracking meeting participants and enforcing
//! capacity limits as part of the meeting join user story (R-9).
//!
//! # Security
//!
//! - All queries use parameterized statements (SQL injection safe)
//! - Sensitive data is not logged
//! - Partial unique index prevents duplicate active participants

use crate::errors::GcError;
use crate::models::Participant;
use crate::observability::metrics;
use sqlx::PgPool;
use std::time::Instant;
use tracing::instrument;
use uuid::Uuid;

/// Repository for meeting participant operations.
#[allow(dead_code)] // Used by integration tests and future join handler
pub struct ParticipantsRepository;

#[allow(dead_code)] // Methods used by integration tests and future join handler
impl ParticipantsRepository {
    /// Count active participants in a meeting.
    ///
    /// Returns the number of participants with `left_at IS NULL`.
    /// Used by MC to enforce `max_participants` capacity locally.
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `meeting_id` - Meeting UUID
    #[instrument(skip_all, name = "gc.repo.count_active_participants", fields(meeting_id = %meeting_id))]
    pub async fn count_active_participants(
        pool: &PgPool,
        meeting_id: Uuid,
    ) -> Result<i64, GcError> {
        let start = Instant::now();

        let result: Result<(i64,), sqlx::Error> = sqlx::query_as(
            r#"
            SELECT COUNT(*) as count
            FROM participants
            WHERE meeting_id = $1
              AND left_at IS NULL
            "#,
        )
        .bind(meeting_id)
        .fetch_one(pool)
        .await;

        let (status, row) = match result {
            Ok(r) => ("success", Ok(r)),
            Err(e) => ("error", Err(e)),
        };
        metrics::record_db_query("count_active_participants", status, start.elapsed());

        Ok(row?.0)
    }

    /// Add a participant to a meeting.
    ///
    /// Inserts a new active participant record. The partial unique index
    /// on `(meeting_id, user_id) WHERE left_at IS NULL` prevents duplicate
    /// active participants.
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `meeting_id` - Meeting UUID
    /// * `user_id` - User UUID (None for anonymous guests)
    /// * `display_name` - Participant's display name
    /// * `participant_type` - "member" (same-org) or "external" (cross-org/guest)
    /// * `role` - "host" or "participant"
    ///
    /// # Errors
    ///
    /// Returns `GcError::Database` if the user is already an active participant
    /// (unique constraint violation) or on other database errors.
    #[instrument(skip_all, name = "gc.repo.add_participant", fields(meeting_id = %meeting_id))]
    pub async fn add_participant(
        pool: &PgPool,
        meeting_id: Uuid,
        user_id: Option<Uuid>,
        display_name: &str,
        participant_type: &str,
        role: &str,
    ) -> Result<Participant, GcError> {
        let start = Instant::now();

        let result: Result<Participant, sqlx::Error> = sqlx::query_as(
            r#"
            INSERT INTO participants (meeting_id, user_id, display_name, participant_type, role)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING participant_id, meeting_id, user_id, display_name,
                      participant_type, role, joined_at, left_at
            "#,
        )
        .bind(meeting_id)
        .bind(user_id)
        .bind(display_name)
        .bind(participant_type)
        .bind(role)
        .fetch_one(pool)
        .await;

        let (status, row) = match result {
            Ok(r) => ("success", Ok(r)),
            Err(e) => ("error", Err(e)),
        };
        metrics::record_db_query("add_participant", status, start.elapsed());

        Ok(row?)
    }

    /// Remove a participant from a meeting (soft delete).
    ///
    /// Sets `left_at` to the current time for the active participant record.
    /// Returns `true` if a participant was removed, `false` if no active
    /// participant was found.
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `meeting_id` - Meeting UUID
    /// * `user_id` - User UUID (must not be None — guest removal uses participant_id)
    #[instrument(skip_all, name = "gc.repo.remove_participant", fields(meeting_id = %meeting_id))]
    pub async fn remove_participant(
        pool: &PgPool,
        meeting_id: Uuid,
        user_id: Uuid,
    ) -> Result<bool, GcError> {
        let start = Instant::now();

        let result = sqlx::query(
            r#"
            UPDATE participants
            SET left_at = NOW()
            WHERE meeting_id = $1
              AND user_id = $2
              AND left_at IS NULL
            "#,
        )
        .bind(meeting_id)
        .bind(user_id)
        .execute(pool)
        .await;

        let (status, query_result) = match result {
            Ok(r) => ("success", Ok(r)),
            Err(e) => ("error", Err(e)),
        };
        metrics::record_db_query("remove_participant", status, start.elapsed());

        Ok(query_result?.rows_affected() > 0)
    }
}
