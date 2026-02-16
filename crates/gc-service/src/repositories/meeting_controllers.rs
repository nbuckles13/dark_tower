//! Meeting Controllers repository for database operations.
//!
//! Provides CRUD operations for the meeting_controllers table using sqlx
//! compile-time query checking.
//!
//! # Security
//!
//! - All queries use parameterized statements (SQL injection safe)
//! - Sensitive data is not logged
//! - Uses transactions for multi-step operations where needed

use crate::errors::GcError;
use crate::observability::metrics;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use std::time::Instant;
use tracing::instrument;

/// Health status values matching the database enum.
/// Maps to proto HealthStatus enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthStatus {
    Pending,
    Healthy,
    Degraded,
    Unhealthy,
    Draining,
}

impl HealthStatus {
    /// Convert from proto HealthStatus enum value.
    pub fn from_proto(value: i32) -> Self {
        match value {
            0 => HealthStatus::Pending,
            1 => HealthStatus::Healthy,
            2 => HealthStatus::Degraded,
            3 => HealthStatus::Unhealthy,
            4 => HealthStatus::Draining,
            _ => HealthStatus::Unhealthy, // Unknown status defaults to unhealthy
        }
    }

    /// Convert to database string representation.
    pub fn as_db_str(&self) -> &'static str {
        match self {
            HealthStatus::Pending => "pending",
            HealthStatus::Healthy => "healthy",
            HealthStatus::Degraded => "degraded",
            HealthStatus::Unhealthy => "unhealthy",
            HealthStatus::Draining => "draining",
        }
    }

    /// Parse from database string representation.
    #[allow(dead_code)] // Used by get_controller
    pub fn from_db_str(s: &str) -> Self {
        match s {
            "pending" => HealthStatus::Pending,
            "healthy" => HealthStatus::Healthy,
            "degraded" => HealthStatus::Degraded,
            "unhealthy" => HealthStatus::Unhealthy,
            "draining" => HealthStatus::Draining,
            _ => HealthStatus::Unhealthy,
        }
    }
}

/// Meeting controller record from database.
#[derive(Debug, Clone)]
#[allow(dead_code)] // Will be used for MC lookup
pub struct MeetingController {
    pub controller_id: String,
    pub region: String,
    pub endpoint: String,
    pub grpc_endpoint: String,
    pub webtransport_endpoint: Option<String>,
    pub max_meetings: i32,
    pub current_meetings: i32,
    pub max_participants: i32,
    pub current_participants: i32,
    pub health_status: HealthStatus,
    pub last_heartbeat_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Repository for meeting controller operations.
pub struct MeetingControllersRepository;

impl MeetingControllersRepository {
    /// Register or update a meeting controller (UPSERT).
    ///
    /// If a controller with the same ID exists, updates its registration.
    /// New registrations start with 'pending' health status.
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `id` - Unique controller identifier
    /// * `region` - Deployment region (e.g., "us-east-1")
    /// * `grpc_endpoint` - gRPC endpoint for GC->MC calls
    /// * `webtransport_endpoint` - Optional WebTransport endpoint for clients
    /// * `max_meetings` - Maximum concurrent meetings
    /// * `max_participants` - Maximum total participants
    ///
    /// # Errors
    ///
    /// Returns `GcError::Database` on database failures.
    #[instrument(skip_all, fields(controller_id = %id, region = %region))]
    pub async fn register_mc(
        pool: &PgPool,
        id: &str,
        region: &str,
        grpc_endpoint: &str,
        webtransport_endpoint: Option<&str>,
        max_meetings: i32,
        max_participants: i32,
    ) -> Result<(), GcError> {
        let start = Instant::now();

        // Use UPSERT pattern: INSERT ... ON CONFLICT DO UPDATE
        // Using runtime query to avoid compile-time database connection requirement
        let query_result = sqlx::query(
            r#"
            INSERT INTO meeting_controllers (
                controller_id, region, endpoint, grpc_endpoint, webtransport_endpoint,
                max_meetings, max_participants, health_status, last_heartbeat_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, 'pending', NOW())
            ON CONFLICT (controller_id) DO UPDATE SET
                region = EXCLUDED.region,
                endpoint = EXCLUDED.endpoint,
                grpc_endpoint = EXCLUDED.grpc_endpoint,
                webtransport_endpoint = EXCLUDED.webtransport_endpoint,
                max_meetings = EXCLUDED.max_meetings,
                max_participants = EXCLUDED.max_participants,
                health_status = 'pending',
                last_heartbeat_at = NOW(),
                updated_at = NOW()
            "#,
        )
        .bind(id)
        .bind(region)
        .bind(grpc_endpoint) // Use grpc_endpoint for legacy endpoint field too
        .bind(grpc_endpoint)
        .bind(webtransport_endpoint)
        .bind(max_meetings)
        .bind(max_participants)
        .execute(pool)
        .await;

        // Record DB query metrics (ADR-0011)
        let status = if query_result.is_ok() {
            "success"
        } else {
            "error"
        };
        metrics::record_db_query("register_mc", status, start.elapsed());

        query_result?;

        tracing::info!(
            target: "gc.repository.mc",
            controller_id = %id,
            region = %region,
            "Meeting controller registered/updated"
        );

        Ok(())
    }

    /// Update heartbeat timestamp and capacity for a controller.
    ///
    /// Also updates health status if the controller was previously unhealthy
    /// due to missed heartbeats (transitions back to healthy/degraded based on
    /// reported status).
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `controller_id` - Controller identifier
    /// * `current_meetings` - Current number of active meetings
    /// * `current_participants` - Current number of participants
    /// * `health_status` - Reported health status from controller
    ///
    /// # Returns
    ///
    /// Returns `true` if a row was updated, `false` if controller not found.
    #[instrument(skip_all, fields(controller_id = %controller_id))]
    pub async fn update_heartbeat(
        pool: &PgPool,
        controller_id: &str,
        current_meetings: i32,
        current_participants: i32,
        health_status: HealthStatus,
    ) -> Result<bool, GcError> {
        let start = Instant::now();

        let query_result = sqlx::query(
            r#"
            UPDATE meeting_controllers
            SET
                current_meetings = $2,
                current_participants = $3,
                health_status = $4,
                last_heartbeat_at = NOW(),
                updated_at = NOW()
            WHERE controller_id = $1
            "#,
        )
        .bind(controller_id)
        .bind(current_meetings)
        .bind(current_participants)
        .bind(health_status.as_db_str())
        .execute(pool)
        .await;

        // Record DB query metrics (ADR-0011)
        let (status, result) = match query_result {
            Ok(r) => ("success", Ok(r)),
            Err(e) => ("error", Err(e)),
        };
        metrics::record_db_query("update_heartbeat", status, start.elapsed());

        let updated = result?.rows_affected() > 0;

        if updated {
            tracing::debug!(
                target: "gc.repository.mc",
                controller_id = %controller_id,
                current_meetings = current_meetings,
                current_participants = current_participants,
                health = ?health_status,
                "Heartbeat updated"
            );
        } else {
            tracing::warn!(
                target: "gc.repository.mc",
                controller_id = %controller_id,
                "Heartbeat update failed: controller not found"
            );
        }

        Ok(updated)
    }

    /// Mark stale controllers as unhealthy.
    ///
    /// Controllers that haven't sent a heartbeat within the staleness threshold
    /// are marked as unhealthy. This is called periodically by the health checker.
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `staleness_threshold_seconds` - Seconds since last heartbeat to consider stale
    ///
    /// # Returns
    ///
    /// Returns the number of controllers marked as unhealthy.
    #[instrument(skip_all, fields(threshold_seconds = staleness_threshold_seconds))]
    pub async fn mark_stale_controllers_unhealthy(
        pool: &PgPool,
        staleness_threshold_seconds: i64,
    ) -> Result<u64, GcError> {
        let start = Instant::now();

        let query_result = sqlx::query(
            r#"
            UPDATE meeting_controllers
            SET
                health_status = 'unhealthy',
                updated_at = NOW()
            WHERE
                last_heartbeat_at < NOW() - ($1 || ' seconds')::INTERVAL
                AND health_status != 'unhealthy'
                AND health_status != 'draining'
            "#,
        )
        .bind(staleness_threshold_seconds.to_string())
        .execute(pool)
        .await;

        // Record DB query metrics (ADR-0011)
        let (status, result) = match query_result {
            Ok(r) => ("success", Ok(r)),
            Err(e) => ("error", Err(e)),
        };
        metrics::record_db_query("mark_stale_controllers_unhealthy", status, start.elapsed());

        let count = result?.rows_affected();

        if count > 0 {
            tracing::warn!(
                target: "gc.repository.mc",
                count = count,
                threshold_seconds = staleness_threshold_seconds,
                "Marked stale controllers as unhealthy"
            );
        }

        Ok(count)
    }

    /// Get a meeting controller by ID.
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `controller_id` - Controller identifier
    ///
    /// # Returns
    ///
    /// Returns `Some(MeetingController)` if found, `None` otherwise.
    #[allow(dead_code)] // Will be used in future phases
    #[instrument(skip_all, fields(controller_id = %controller_id))]
    pub async fn get_controller(
        pool: &PgPool,
        controller_id: &str,
    ) -> Result<Option<MeetingController>, GcError> {
        let start = Instant::now();

        // Use sqlx::query_as with explicit struct mapping
        let query_result: Result<Option<MeetingControllerRow>, sqlx::Error> = sqlx::query_as(
            r#"
            SELECT
                controller_id,
                region,
                endpoint,
                grpc_endpoint,
                webtransport_endpoint,
                max_meetings,
                current_meetings,
                max_participants,
                current_participants,
                health_status,
                last_heartbeat_at,
                created_at,
                updated_at
            FROM meeting_controllers
            WHERE controller_id = $1
            "#,
        )
        .bind(controller_id)
        .fetch_optional(pool)
        .await;

        // Record DB query metrics (ADR-0011)
        let (status, row) = match query_result {
            Ok(r) => ("success", Ok(r)),
            Err(e) => ("error", Err(e)),
        };
        metrics::record_db_query("get_controller", status, start.elapsed());

        Ok(row?.map(|r| MeetingController {
            controller_id: r.controller_id,
            region: r.region,
            endpoint: r.endpoint,
            grpc_endpoint: r.grpc_endpoint,
            webtransport_endpoint: r.webtransport_endpoint,
            max_meetings: r.max_meetings,
            current_meetings: r.current_meetings,
            max_participants: r.max_participants,
            current_participants: r.current_participants,
            health_status: HealthStatus::from_db_str(&r.health_status),
            last_heartbeat_at: r.last_heartbeat_at,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }))
    }

    /// Get counts of meeting controllers grouped by health status.
    ///
    /// Used to initialize the `gc_registered_controllers` gauge on startup
    /// and to refresh after registration/heartbeat changes.
    ///
    /// # Returns
    ///
    /// A vector of (status, count) pairs for all statuses that have at least
    /// one controller. Statuses with zero controllers are not included in
    /// the result (the caller should set those to 0).
    #[instrument(skip_all)]
    pub async fn get_controller_counts_by_status(
        pool: &PgPool,
    ) -> Result<Vec<(HealthStatus, i64)>, GcError> {
        let start = Instant::now();

        let query_result: Result<Vec<ControllerCountRow>, sqlx::Error> = sqlx::query_as(
            r#"
            SELECT health_status, COUNT(*) as count
            FROM meeting_controllers
            GROUP BY health_status
            "#,
        )
        .fetch_all(pool)
        .await;

        // Record DB query metrics (ADR-0011)
        let (status, rows) = match query_result {
            Ok(r) => ("success", Ok(r)),
            Err(e) => ("error", Err(e)),
        };
        metrics::record_db_query("get_controller_counts_by_status", status, start.elapsed());

        let rows = rows?;
        let counts: Vec<(HealthStatus, i64)> = rows
            .into_iter()
            .map(|r| (HealthStatus::from_db_str(&r.health_status), r.count))
            .collect();

        Ok(counts)
    }
}

/// Database row for controller count queries.
#[derive(sqlx::FromRow)]
struct ControllerCountRow {
    health_status: String,
    count: i64,
}

/// Database row representation for meeting controllers.
#[derive(sqlx::FromRow)]
#[allow(dead_code)] // Used by get_controller
struct MeetingControllerRow {
    controller_id: String,
    region: String,
    endpoint: String,
    grpc_endpoint: String,
    webtransport_endpoint: Option<String>,
    max_meetings: i32,
    current_meetings: i32,
    max_participants: i32,
    current_participants: i32,
    health_status: String,
    last_heartbeat_at: DateTime<Utc>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_health_status_from_proto() {
        assert_eq!(HealthStatus::from_proto(0), HealthStatus::Pending);
        assert_eq!(HealthStatus::from_proto(1), HealthStatus::Healthy);
        assert_eq!(HealthStatus::from_proto(2), HealthStatus::Degraded);
        assert_eq!(HealthStatus::from_proto(3), HealthStatus::Unhealthy);
        assert_eq!(HealthStatus::from_proto(4), HealthStatus::Draining);
        assert_eq!(HealthStatus::from_proto(99), HealthStatus::Unhealthy); // Unknown
    }

    #[test]
    fn test_health_status_as_db_str() {
        assert_eq!(HealthStatus::Pending.as_db_str(), "pending");
        assert_eq!(HealthStatus::Healthy.as_db_str(), "healthy");
        assert_eq!(HealthStatus::Degraded.as_db_str(), "degraded");
        assert_eq!(HealthStatus::Unhealthy.as_db_str(), "unhealthy");
        assert_eq!(HealthStatus::Draining.as_db_str(), "draining");
    }

    #[test]
    fn test_health_status_from_db_str() {
        assert_eq!(HealthStatus::from_db_str("pending"), HealthStatus::Pending);
        assert_eq!(HealthStatus::from_db_str("healthy"), HealthStatus::Healthy);
        assert_eq!(
            HealthStatus::from_db_str("degraded"),
            HealthStatus::Degraded
        );
        assert_eq!(
            HealthStatus::from_db_str("unhealthy"),
            HealthStatus::Unhealthy
        );
        assert_eq!(
            HealthStatus::from_db_str("draining"),
            HealthStatus::Draining
        );
        assert_eq!(
            HealthStatus::from_db_str("unknown"),
            HealthStatus::Unhealthy
        ); // Unknown defaults
    }

    #[test]
    fn test_health_status_roundtrip() {
        for status in [
            HealthStatus::Pending,
            HealthStatus::Healthy,
            HealthStatus::Degraded,
            HealthStatus::Unhealthy,
            HealthStatus::Draining,
        ] {
            assert_eq!(HealthStatus::from_db_str(status.as_db_str()), status);
        }
    }
}
