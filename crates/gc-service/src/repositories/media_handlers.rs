//! Media Handlers repository for database operations.
//!
//! Provides CRUD operations for the media_handlers table using sqlx.
//! Implements ADR-0010 Section 4a: MH registry in GC.
//!
//! # Security
//!
//! - All queries use parameterized statements (SQL injection safe)
//! - Sensitive data is not logged
//! - Uses UPSERT pattern for registration

// Allow dead code during incremental development - these types are used in tests
// and will be wired into handlers in future phases.
#![allow(dead_code)]

use crate::errors::GcError;
use crate::observability::metrics;
use crate::repositories::HealthStatus;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use std::time::Instant;
use tracing::instrument;

/// Default heartbeat staleness threshold in seconds.
/// Handlers without heartbeat within this time are considered unhealthy.
const DEFAULT_HEARTBEAT_STALENESS_SECONDS: i64 = 30;

/// Number of candidate MHs to select for weighted random load balancing.
const LOAD_BALANCING_CANDIDATE_COUNT: i64 = 5;

/// Media handler record from database.
#[derive(Debug, Clone)]
pub struct MediaHandler {
    pub handler_id: String,
    pub region: String,
    pub webtransport_endpoint: String,
    pub grpc_endpoint: String,
    pub max_streams: i32,
    pub current_streams: i32,
    pub health_status: HealthStatus,
    pub cpu_usage_percent: Option<f32>,
    pub memory_usage_percent: Option<f32>,
    pub bandwidth_usage_percent: Option<f32>,
    pub last_heartbeat_at: DateTime<Utc>,
    pub registered_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// MH candidate for load balancing selection.
#[derive(Debug, Clone)]
pub struct MhCandidate {
    /// Handler ID.
    pub handler_id: String,
    /// WebTransport endpoint for client connections.
    pub webtransport_endpoint: String,
    /// gRPC endpoint for MC→MH communication.
    pub grpc_endpoint: String,
    /// Load ratio (0.0 = empty, 1.0 = full).
    pub load_ratio: f64,
}

/// Repository for media handler operations.
pub struct MediaHandlersRepository;

impl MediaHandlersRepository {
    /// Register or update a media handler (UPSERT).
    ///
    /// If a handler with the same ID exists, updates its registration.
    /// New registrations start with 'pending' health status.
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `handler_id` - Unique handler identifier
    /// * `region` - Deployment region (e.g., "us-east-1")
    /// * `webtransport_endpoint` - WebTransport endpoint for client connections
    /// * `grpc_endpoint` - gRPC endpoint for MC→MH communication
    /// * `max_streams` - Maximum concurrent streams
    ///
    /// # Errors
    ///
    /// Returns `GcError::Database` on database failures.
    #[instrument(skip_all, fields(handler_id = %handler_id, region = %region))]
    pub async fn register_mh(
        pool: &PgPool,
        handler_id: &str,
        region: &str,
        webtransport_endpoint: &str,
        grpc_endpoint: &str,
        max_streams: i32,
    ) -> Result<(), GcError> {
        let start = Instant::now();

        let query_result = sqlx::query(
            r#"
            INSERT INTO media_handlers (
                handler_id, region, webtransport_endpoint, grpc_endpoint,
                max_streams, health_status, last_heartbeat_at
            )
            VALUES ($1, $2, $3, $4, $5, 'pending', NOW())
            ON CONFLICT (handler_id) DO UPDATE SET
                region = EXCLUDED.region,
                webtransport_endpoint = EXCLUDED.webtransport_endpoint,
                grpc_endpoint = EXCLUDED.grpc_endpoint,
                max_streams = EXCLUDED.max_streams,
                health_status = 'pending',
                last_heartbeat_at = NOW(),
                updated_at = NOW()
            "#,
        )
        .bind(handler_id)
        .bind(region)
        .bind(webtransport_endpoint)
        .bind(grpc_endpoint)
        .bind(max_streams)
        .execute(pool)
        .await;

        // Record DB query metrics (ADR-0011)
        let status = if query_result.is_ok() {
            "success"
        } else {
            "error"
        };
        metrics::record_db_query("register_mh", status, start.elapsed());

        query_result?;

        tracing::info!(
            target: "gc.repository.mh",
            handler_id = %handler_id,
            region = %region,
            "Media handler registered/updated"
        );

        Ok(())
    }

    /// Update load report (heartbeat with capacity info) for a handler.
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `handler_id` - Handler identifier
    /// * `current_streams` - Current number of active streams
    /// * `health_status` - Reported health status
    /// * `cpu_usage` - Optional CPU usage percent
    /// * `memory_usage` - Optional memory usage percent
    /// * `bandwidth_usage` - Optional bandwidth usage percent
    ///
    /// # Returns
    ///
    /// Returns `true` if a row was updated, `false` if handler not found.
    #[instrument(skip_all, fields(handler_id = %handler_id))]
    pub async fn update_load_report(
        pool: &PgPool,
        handler_id: &str,
        current_streams: i32,
        health_status: HealthStatus,
        cpu_usage: Option<f32>,
        memory_usage: Option<f32>,
        bandwidth_usage: Option<f32>,
    ) -> Result<bool, GcError> {
        let start = Instant::now();

        let query_result = sqlx::query(
            r#"
            UPDATE media_handlers
            SET
                current_streams = $2,
                health_status = $3,
                cpu_usage_percent = $4,
                memory_usage_percent = $5,
                bandwidth_usage_percent = $6,
                last_heartbeat_at = NOW(),
                updated_at = NOW()
            WHERE handler_id = $1
            "#,
        )
        .bind(handler_id)
        .bind(current_streams)
        .bind(health_status.as_db_str())
        .bind(cpu_usage)
        .bind(memory_usage)
        .bind(bandwidth_usage)
        .execute(pool)
        .await;

        // Record DB query metrics (ADR-0011)
        let (status, result) = match query_result {
            Ok(r) => ("success", Ok(r)),
            Err(e) => ("error", Err(e)),
        };
        metrics::record_db_query("update_load_report", status, start.elapsed());

        let updated = result?.rows_affected() > 0;

        if updated {
            tracing::debug!(
                target: "gc.repository.mh",
                handler_id = %handler_id,
                current_streams = current_streams,
                health = ?health_status,
                "Load report updated"
            );
        } else {
            tracing::warn!(
                target: "gc.repository.mh",
                handler_id = %handler_id,
                "Load report update failed: handler not found"
            );
        }

        Ok(updated)
    }

    /// Mark stale handlers as unhealthy.
    ///
    /// Handlers that haven't sent a heartbeat within the staleness threshold
    /// are marked as unhealthy. This is called periodically by the health checker.
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `staleness_threshold_seconds` - Seconds since last heartbeat to consider stale
    ///
    /// # Returns
    ///
    /// Returns the number of handlers marked as unhealthy.
    #[instrument(skip_all, fields(threshold_seconds = staleness_threshold_seconds))]
    pub async fn mark_stale_handlers_unhealthy(
        pool: &PgPool,
        staleness_threshold_seconds: i64,
    ) -> Result<u64, GcError> {
        let start = Instant::now();

        let query_result = sqlx::query(
            r#"
            UPDATE media_handlers
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
        metrics::record_db_query("mark_stale_mh_unhealthy", status, start.elapsed());

        let count = result?.rows_affected();

        if count > 0 {
            tracing::warn!(
                target: "gc.repository.mh",
                count = count,
                threshold_seconds = staleness_threshold_seconds,
                "Marked stale handlers as unhealthy"
            );
        }

        Ok(count)
    }

    /// Get candidate MHs for load balancing in a region.
    ///
    /// Returns up to 5 healthy MHs with capacity, ordered by load ratio (least loaded first).
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `region` - Deployment region
    #[instrument(skip_all, fields(region = %region))]
    pub async fn get_candidate_mhs(
        pool: &PgPool,
        region: &str,
    ) -> Result<Vec<MhCandidate>, GcError> {
        let start = Instant::now();

        let query_result: Result<Vec<MhCandidateRow>, sqlx::Error> = sqlx::query_as(
            r#"
            SELECT
                handler_id,
                webtransport_endpoint,
                grpc_endpoint,
                CASE
                    WHEN max_streams = 0 THEN 1.0
                    ELSE (current_streams::float / max_streams)
                END AS load_ratio
            FROM media_handlers
            WHERE health_status = 'healthy'
              AND region = $1
              AND current_streams < max_streams
              AND last_heartbeat_at > NOW() - ($2 || ' seconds')::INTERVAL
            ORDER BY load_ratio ASC, last_heartbeat_at DESC
            LIMIT $3
            "#,
        )
        .bind(region)
        .bind(DEFAULT_HEARTBEAT_STALENESS_SECONDS.to_string())
        .bind(LOAD_BALANCING_CANDIDATE_COUNT)
        .fetch_all(pool)
        .await;

        // Record DB query metrics (ADR-0011)
        let (status, rows) = match query_result {
            Ok(r) => ("success", Ok(r)),
            Err(e) => ("error", Err(e)),
        };
        metrics::record_db_query("get_candidate_mhs", status, start.elapsed());

        Ok(rows?
            .into_iter()
            .map(|r| MhCandidate {
                handler_id: r.handler_id,
                webtransport_endpoint: r.webtransport_endpoint,
                grpc_endpoint: r.grpc_endpoint,
                load_ratio: r.load_ratio,
            })
            .collect())
    }

    /// Get a media handler by ID.
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `handler_id` - Handler identifier
    ///
    /// # Returns
    ///
    /// Returns `Some(MediaHandler)` if found, `None` otherwise.
    #[allow(dead_code)] // Will be used in future phases
    #[instrument(skip_all, fields(handler_id = %handler_id))]
    pub async fn get_handler(
        pool: &PgPool,
        handler_id: &str,
    ) -> Result<Option<MediaHandler>, GcError> {
        let start = Instant::now();

        let query_result: Result<Option<MediaHandlerRow>, sqlx::Error> = sqlx::query_as(
            r#"
            SELECT
                handler_id,
                region,
                webtransport_endpoint,
                grpc_endpoint,
                max_streams,
                current_streams,
                health_status,
                cpu_usage_percent,
                memory_usage_percent,
                bandwidth_usage_percent,
                last_heartbeat_at,
                registered_at,
                updated_at
            FROM media_handlers
            WHERE handler_id = $1
            "#,
        )
        .bind(handler_id)
        .fetch_optional(pool)
        .await;

        // Record DB query metrics (ADR-0011)
        let (status, row) = match query_result {
            Ok(r) => ("success", Ok(r)),
            Err(e) => ("error", Err(e)),
        };
        metrics::record_db_query("get_handler", status, start.elapsed());

        Ok(row?.map(|r| MediaHandler {
            handler_id: r.handler_id,
            region: r.region,
            webtransport_endpoint: r.webtransport_endpoint,
            grpc_endpoint: r.grpc_endpoint,
            max_streams: r.max_streams,
            current_streams: r.current_streams,
            health_status: HealthStatus::from_db_str(&r.health_status),
            cpu_usage_percent: r.cpu_usage_percent,
            memory_usage_percent: r.memory_usage_percent,
            bandwidth_usage_percent: r.bandwidth_usage_percent,
            last_heartbeat_at: r.last_heartbeat_at,
            registered_at: r.registered_at,
            updated_at: r.updated_at,
        }))
    }
}

// ============================================================================
// Database Row Types
// ============================================================================

#[derive(sqlx::FromRow)]
struct MhCandidateRow {
    handler_id: String,
    webtransport_endpoint: String,
    grpc_endpoint: String,
    load_ratio: f64,
}

#[derive(sqlx::FromRow)]
#[allow(dead_code)]
struct MediaHandlerRow {
    handler_id: String,
    region: String,
    webtransport_endpoint: String,
    grpc_endpoint: String,
    max_streams: i32,
    current_streams: i32,
    health_status: String,
    cpu_usage_percent: Option<f32>,
    memory_usage_percent: Option<f32>,
    bandwidth_usage_percent: Option<f32>,
    last_heartbeat_at: DateTime<Utc>,
    registered_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_mh_candidate_fields() {
        let candidate = MhCandidate {
            handler_id: "mh-test".to_string(),
            webtransport_endpoint: "https://mh:443".to_string(),
            grpc_endpoint: "https://mh:50051".to_string(),
            load_ratio: 0.5,
        };

        assert_eq!(candidate.handler_id, "mh-test");
        assert_eq!(candidate.webtransport_endpoint, "https://mh:443");
        assert_eq!(candidate.grpc_endpoint, "https://mh:50051");
        assert!((candidate.load_ratio - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_media_handler_fields() {
        let now = Utc::now();
        let handler = MediaHandler {
            handler_id: "mh-test".to_string(),
            region: "us-east-1".to_string(),
            webtransport_endpoint: "https://mh:443".to_string(),
            grpc_endpoint: "https://mh:50051".to_string(),
            max_streams: 1000,
            current_streams: 100,
            health_status: HealthStatus::Healthy,
            cpu_usage_percent: Some(25.0),
            memory_usage_percent: Some(50.0),
            bandwidth_usage_percent: Some(30.0),
            last_heartbeat_at: now,
            registered_at: now,
            updated_at: now,
        };

        assert_eq!(handler.handler_id, "mh-test");
        assert_eq!(handler.region, "us-east-1");
        assert_eq!(handler.max_streams, 1000);
        assert_eq!(handler.current_streams, 100);
        assert_eq!(handler.health_status, HealthStatus::Healthy);
        assert_eq!(handler.cpu_usage_percent, Some(25.0));
    }
}
