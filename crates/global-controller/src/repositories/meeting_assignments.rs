//! Meeting Assignments repository for database operations.
//!
//! Provides operations for assigning meetings to meeting controllers using
//! weighted round-robin load balancing per ADR-0010.
//!
//! # Security
//!
//! - All queries use parameterized statements (SQL injection safe)
//! - Sensitive data is not logged
//! - Uses atomic operations to prevent race conditions

use crate::errors::GcError;
use chrono::{DateTime, Utc};
use ring::rand::{SecureRandom, SystemRandom};
use sqlx::PgPool;
use tracing::instrument;

/// Default heartbeat staleness threshold in seconds.
/// Controllers without heartbeat within this time are considered unhealthy.
const DEFAULT_HEARTBEAT_STALENESS_SECONDS: i64 = 30;

/// Number of candidate MCs to select for weighted random load balancing.
const LOAD_BALANCING_CANDIDATE_COUNT: i64 = 5;

/// Meeting assignment record from database.
// Allow: Fields used in tests; struct will be used by future query handlers
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct MeetingAssignment {
    /// Meeting ID.
    pub meeting_id: String,
    /// Assigned meeting controller ID.
    pub meeting_controller_id: String,
    /// Deployment region.
    pub region: String,
    /// When the assignment was created.
    pub assigned_at: DateTime<Utc>,
    /// GC instance that made this assignment.
    pub assigned_by_gc_id: String,
    /// When the assignment ended (None = active).
    pub ended_at: Option<DateTime<Utc>>,
}

/// MC candidate for load balancing selection.
#[derive(Debug, Clone)]
pub struct McCandidate {
    /// Controller ID.
    pub controller_id: String,
    /// gRPC endpoint for GC->MC communication.
    pub grpc_endpoint: String,
    /// WebTransport endpoint for client connections.
    pub webtransport_endpoint: Option<String>,
    /// Load ratio (0.0 = empty, 1.0 = full).
    pub load_ratio: f64,
}

/// Result of MC assignment operation.
#[derive(Debug, Clone)]
pub struct McAssignment {
    /// Assigned controller ID.
    pub mc_id: String,
    /// gRPC endpoint for GC->MC communication.
    pub grpc_endpoint: String,
    /// WebTransport endpoint for client connections.
    pub webtransport_endpoint: Option<String>,
}

/// Repository for meeting assignment operations.
pub struct MeetingAssignmentsRepository;

impl MeetingAssignmentsRepository {
    /// Get existing healthy assignment for a meeting in a region.
    ///
    /// Returns `Some(McAssignment)` if an active assignment exists with a healthy MC,
    /// `None` otherwise.
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `meeting_id` - Meeting identifier
    /// * `region` - Deployment region
    #[instrument(skip_all, fields(meeting_id = %meeting_id, region = %region))]
    pub async fn get_healthy_assignment(
        pool: &PgPool,
        meeting_id: &str,
        region: &str,
    ) -> Result<Option<McAssignment>, GcError> {
        let row: Option<McAssignmentRow> = sqlx::query_as(
            r#"
            SELECT
                ma.meeting_controller_id,
                mc.grpc_endpoint,
                mc.webtransport_endpoint
            FROM meeting_assignments ma
            JOIN meeting_controllers mc ON ma.meeting_controller_id = mc.controller_id
            WHERE ma.meeting_id = $1
              AND ma.region = $2
              AND ma.ended_at IS NULL
              AND mc.health_status = 'healthy'
              AND mc.last_heartbeat_at > NOW() - ($3 || ' seconds')::INTERVAL
            "#,
        )
        .bind(meeting_id)
        .bind(region)
        .bind(DEFAULT_HEARTBEAT_STALENESS_SECONDS.to_string())
        .fetch_optional(pool)
        .await?;

        Ok(row.map(|r| McAssignment {
            mc_id: r.meeting_controller_id,
            grpc_endpoint: r.grpc_endpoint,
            webtransport_endpoint: r.webtransport_endpoint,
        }))
    }

    /// Get candidate MCs for load balancing in a region.
    ///
    /// Returns up to 5 healthy MCs with capacity, ordered by load ratio (least loaded first).
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `region` - Deployment region
    #[instrument(skip_all, fields(region = %region))]
    pub async fn get_candidate_mcs(
        pool: &PgPool,
        region: &str,
    ) -> Result<Vec<McCandidate>, GcError> {
        let rows: Vec<McCandidateRow> = sqlx::query_as(
            r#"
            SELECT
                controller_id,
                grpc_endpoint,
                webtransport_endpoint,
                CASE
                    WHEN max_meetings = 0 THEN 1.0
                    ELSE (current_meetings::float / max_meetings)
                END AS load_ratio
            FROM meeting_controllers
            WHERE health_status = 'healthy'
              AND region = $1
              AND current_meetings < max_meetings
              AND last_heartbeat_at > NOW() - ($2 || ' seconds')::INTERVAL
            ORDER BY load_ratio ASC, last_heartbeat_at DESC
            LIMIT $3
            "#,
        )
        .bind(region)
        .bind(DEFAULT_HEARTBEAT_STALENESS_SECONDS.to_string())
        .bind(LOAD_BALANCING_CANDIDATE_COUNT)
        .fetch_all(pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| McCandidate {
                controller_id: r.controller_id,
                grpc_endpoint: r.grpc_endpoint,
                webtransport_endpoint: r.webtransport_endpoint,
                load_ratio: r.load_ratio,
            })
            .collect())
    }

    /// Atomically assign a meeting to an MC using ADR-0010 algorithm.
    ///
    /// This operation:
    /// 1. Ends any unhealthy existing assignment
    /// 2. Inserts new assignment only if no healthy assignment exists
    /// 3. Returns the assignment (ours or the winner's if race lost)
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `meeting_id` - Meeting identifier
    /// * `region` - Deployment region
    /// * `selected_mc` - The MC to assign (from weighted selection)
    /// * `gc_id` - ID of this GC instance
    #[instrument(skip_all, fields(meeting_id = %meeting_id, region = %region, mc_id = %selected_mc.controller_id))]
    pub async fn atomic_assign(
        pool: &PgPool,
        meeting_id: &str,
        region: &str,
        selected_mc: &McCandidate,
        gc_id: &str,
    ) -> Result<McAssignment, GcError> {
        // Use INSERT ... ON CONFLICT DO UPDATE to atomically handle:
        // - New assignments (no existing row)
        // - Replacing unhealthy assignments (existing row with unhealthy MC)
        // - Race conditions (multiple GCs trying to assign simultaneously)
        //
        // Note: We use ON CONFLICT DO UPDATE (not DO NOTHING) so that we can
        // conditionally update the row if the existing MC is unhealthy.
        let result: Option<AtomicAssignResult> = sqlx::query_as(
            r#"
            INSERT INTO meeting_assignments (meeting_id, meeting_controller_id, region, assigned_by_gc_id)
            VALUES ($1, $3, $2, $4)
            ON CONFLICT (meeting_id, region) DO UPDATE
            SET meeting_controller_id = EXCLUDED.meeting_controller_id,
                assigned_by_gc_id = EXCLUDED.assigned_by_gc_id,
                assigned_at = NOW()
            WHERE EXISTS (
                -- Only update if current assignment's MC is unhealthy or stale
                SELECT 1 FROM meeting_controllers mc
                WHERE mc.controller_id = meeting_assignments.meeting_controller_id
                  AND (mc.health_status != 'healthy'
                       OR mc.last_heartbeat_at < NOW() - ($5 || ' seconds')::INTERVAL)
            )
            RETURNING meeting_controller_id
            "#,
        )
        .bind(meeting_id)
        .bind(region)
        .bind(&selected_mc.controller_id)
        .bind(gc_id)
        .bind(DEFAULT_HEARTBEAT_STALENESS_SECONDS.to_string())
        .fetch_optional(pool)
        .await?;

        match result {
            Some(_) => {
                // We won the race - return our assignment
                // Note: Logging happens at service layer to avoid duplication
                Ok(McAssignment {
                    mc_id: selected_mc.controller_id.clone(),
                    grpc_endpoint: selected_mc.grpc_endpoint.clone(),
                    webtransport_endpoint: selected_mc.webtransport_endpoint.clone(),
                })
            }
            None => {
                // Either:
                // 1. Another GC won the race and assigned a healthy MC
                // 2. There's already a healthy assignment (we should have caught this earlier)
                // Re-query to get the current assignment
                tracing::debug!(
                    target: "gc.repository.assignments",
                    meeting_id = %meeting_id,
                    region = %region,
                    "Insert/update returned no rows, fetching current assignment"
                );

                let current = Self::get_current_assignment(pool, meeting_id, region).await?;
                current.ok_or_else(|| {
                    // This shouldn't happen - indicates a bug or DB inconsistency
                    tracing::error!(
                        target: "gc.repository.assignments",
                        meeting_id = %meeting_id,
                        region = %region,
                        "Assignment failed - no current assignment found"
                    );
                    GcError::ServiceUnavailable(
                        "Failed to assign meeting controller - please retry".to_string(),
                    )
                })
            }
        }
    }

    /// Get current assignment for a meeting regardless of MC health.
    ///
    /// Used to retrieve the winner's assignment after losing a race.
    #[instrument(skip_all, fields(meeting_id = %meeting_id, region = %region))]
    async fn get_current_assignment(
        pool: &PgPool,
        meeting_id: &str,
        region: &str,
    ) -> Result<Option<McAssignment>, GcError> {
        let row: Option<McAssignmentRow> = sqlx::query_as(
            r#"
            SELECT
                ma.meeting_controller_id,
                mc.grpc_endpoint,
                mc.webtransport_endpoint
            FROM meeting_assignments ma
            JOIN meeting_controllers mc ON ma.meeting_controller_id = mc.controller_id
            WHERE ma.meeting_id = $1
              AND ma.region = $2
              AND ma.ended_at IS NULL
            "#,
        )
        .bind(meeting_id)
        .bind(region)
        .fetch_optional(pool)
        .await?;

        Ok(row.map(|r| McAssignment {
            mc_id: r.meeting_controller_id,
            grpc_endpoint: r.grpc_endpoint,
            webtransport_endpoint: r.webtransport_endpoint,
        }))
    }

    /// End an assignment (soft delete).
    ///
    /// Called when a meeting ends normally.
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `meeting_id` - Meeting identifier
    /// * `region` - Deployment region (if None, ends all regional assignments)
    // Allow: Used in tests; will be called from meeting end handler
    #[allow(dead_code)]
    #[instrument(skip_all, fields(meeting_id = %meeting_id))]
    pub async fn end_assignment(
        pool: &PgPool,
        meeting_id: &str,
        region: Option<&str>,
    ) -> Result<u64, GcError> {
        let result = match region {
            Some(r) => {
                sqlx::query(
                    r#"
                    UPDATE meeting_assignments
                    SET ended_at = NOW()
                    WHERE meeting_id = $1
                      AND region = $2
                      AND ended_at IS NULL
                    "#,
                )
                .bind(meeting_id)
                .bind(r)
                .execute(pool)
                .await?
            }
            None => {
                sqlx::query(
                    r#"
                    UPDATE meeting_assignments
                    SET ended_at = NOW()
                    WHERE meeting_id = $1
                      AND ended_at IS NULL
                    "#,
                )
                .bind(meeting_id)
                .execute(pool)
                .await?
            }
        };

        let count = result.rows_affected();

        if count > 0 {
            tracing::info!(
                target: "gc.repository.assignments",
                meeting_id = %meeting_id,
                region = ?region,
                count = count,
                "Ended meeting assignment(s)"
            );
        }

        Ok(count)
    }

    /// Clean up old ended assignments.
    ///
    /// Deletes assignments that ended more than `retention_days` ago.
    /// Run periodically as a background job.
    // Allow: Used in tests; will be called from background cleanup task
    #[allow(dead_code)]
    #[instrument(skip_all, fields(retention_days = retention_days))]
    pub async fn cleanup_old_assignments(
        pool: &PgPool,
        retention_days: i32,
    ) -> Result<u64, GcError> {
        let result = sqlx::query(
            r#"
            DELETE FROM meeting_assignments
            WHERE ended_at < NOW() - ($1 || ' days')::INTERVAL
            "#,
        )
        .bind(retention_days.to_string())
        .execute(pool)
        .await?;

        let count = result.rows_affected();

        if count > 0 {
            tracing::info!(
                target: "gc.repository.assignments",
                retention_days = retention_days,
                count = count,
                "Cleaned up old assignments"
            );
        }

        Ok(count)
    }
}

/// Select an MC from candidates using weighted random selection.
///
/// Weight is inversely proportional to load ratio:
/// - 0% loaded = weight 1.0
/// - 90% loaded = weight 0.1
///
/// This prevents thundering herd to a single MC while preferring less-loaded instances.
pub fn weighted_random_select(candidates: &[McCandidate]) -> Option<&McCandidate> {
    if candidates.is_empty() {
        return None;
    }

    if candidates.len() == 1 {
        return candidates.first();
    }

    // Calculate weights: weight = 1.0 - load_ratio (capped at 0.99 to ensure minimum weight)
    let weights: Vec<f64> = candidates
        .iter()
        .map(|mc| 1.0 - mc.load_ratio.min(0.99))
        .collect();

    let total: f64 = weights.iter().sum();
    if total <= 0.0 {
        return candidates.first();
    }

    // Generate random value using CSPRNG
    let rng = SystemRandom::new();
    let mut random_bytes = [0u8; 8];
    if rng.fill(&mut random_bytes).is_err() {
        // Fallback to first candidate if CSPRNG fails
        tracing::warn!(
            target: "gc.repository.assignments",
            "CSPRNG failed, falling back to first candidate"
        );
        return candidates.first();
    }

    // Convert bytes to f64 in range [0, 1)
    let random_u64 = u64::from_le_bytes(random_bytes);
    let random_f64 = (random_u64 as f64) / (u64::MAX as f64);
    let mut choice = random_f64 * total;

    // Select based on weight
    for (i, weight) in weights.iter().enumerate() {
        choice -= weight;
        if choice <= 0.0 {
            return candidates.get(i);
        }
    }

    // Fallback to last candidate (floating point edge case)
    candidates.last()
}

// ============================================================================
// Database Row Types
// ============================================================================

#[derive(sqlx::FromRow)]
struct McAssignmentRow {
    meeting_controller_id: String,
    grpc_endpoint: String,
    webtransport_endpoint: Option<String>,
}

#[derive(sqlx::FromRow)]
struct McCandidateRow {
    controller_id: String,
    grpc_endpoint: String,
    webtransport_endpoint: Option<String>,
    load_ratio: f64,
}

#[derive(sqlx::FromRow)]
struct AtomicAssignResult {
    #[allow(dead_code)]
    meeting_controller_id: String,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_weighted_random_select_empty() {
        let candidates: Vec<McCandidate> = vec![];
        assert!(weighted_random_select(&candidates).is_none());
    }

    #[test]
    fn test_weighted_random_select_single() {
        let candidates = vec![McCandidate {
            controller_id: "mc-1".to_string(),
            grpc_endpoint: "https://mc1:50051".to_string(),
            webtransport_endpoint: None,
            load_ratio: 0.5,
        }];

        let selected = weighted_random_select(&candidates);
        assert!(selected.is_some(), "Expected Some, got None");
        // Safe to use if-let after assert
        if let Some(mc) = selected {
            assert_eq!(mc.controller_id, "mc-1");
        }
    }

    #[test]
    fn test_weighted_random_select_multiple_returns_valid() {
        let candidates = vec![
            McCandidate {
                controller_id: "mc-1".to_string(),
                grpc_endpoint: "https://mc1:50051".to_string(),
                webtransport_endpoint: None,
                load_ratio: 0.1, // Low load, high weight
            },
            McCandidate {
                controller_id: "mc-2".to_string(),
                grpc_endpoint: "https://mc2:50051".to_string(),
                webtransport_endpoint: None,
                load_ratio: 0.9, // High load, low weight
            },
        ];

        // Run multiple times to verify it always returns a valid candidate
        for _ in 0..100 {
            let selected = weighted_random_select(&candidates);
            assert!(selected.is_some(), "Expected Some, got None");
            if let Some(mc) = selected {
                let mc_id = &mc.controller_id;
                assert!(mc_id == "mc-1" || mc_id == "mc-2");
            }
        }
    }

    #[test]
    fn test_weighted_random_select_prefers_lower_load() {
        let candidates = vec![
            McCandidate {
                controller_id: "mc-light".to_string(),
                grpc_endpoint: "https://mc1:50051".to_string(),
                webtransport_endpoint: None,
                load_ratio: 0.0, // Empty, weight = 1.0
            },
            McCandidate {
                controller_id: "mc-heavy".to_string(),
                grpc_endpoint: "https://mc2:50051".to_string(),
                webtransport_endpoint: None,
                load_ratio: 0.99, // Almost full, weight = 0.01
            },
        ];

        // Run many times and count selections
        let mut light_count = 0;
        let mut heavy_count = 0;

        for _ in 0..1000 {
            let selected = weighted_random_select(&candidates);
            assert!(selected.is_some(), "weighted_random_select returned None");
            if let Some(mc) = selected {
                match mc.controller_id.as_str() {
                    "mc-light" => light_count += 1,
                    "mc-heavy" => heavy_count += 1,
                    other => {
                        // Fail test if unexpected MC - this should never happen given test setup
                        assert_eq!(other, "mc-light", "Unexpected MC selected");
                    }
                }
            }
        }

        // Light should be selected much more often (100x weight difference)
        // Allow some variance but light should dominate
        assert!(
            light_count > heavy_count * 10,
            "Expected light ({}) to be selected much more than heavy ({})",
            light_count,
            heavy_count
        );
    }

    #[test]
    fn test_mc_candidate_fields() {
        let candidate = McCandidate {
            controller_id: "mc-test".to_string(),
            grpc_endpoint: "https://mc:50051".to_string(),
            webtransport_endpoint: Some("https://mc:443".to_string()),
            load_ratio: 0.5,
        };

        assert_eq!(candidate.controller_id, "mc-test");
        assert_eq!(candidate.grpc_endpoint, "https://mc:50051");
        assert_eq!(
            candidate.webtransport_endpoint,
            Some("https://mc:443".to_string())
        );
        assert!((candidate.load_ratio - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_mc_assignment_fields() {
        let assignment = McAssignment {
            mc_id: "mc-test".to_string(),
            grpc_endpoint: "https://mc:50051".to_string(),
            webtransport_endpoint: Some("https://mc:443".to_string()),
        };

        assert_eq!(assignment.mc_id, "mc-test");
        assert_eq!(assignment.grpc_endpoint, "https://mc:50051");
        assert_eq!(
            assignment.webtransport_endpoint,
            Some("https://mc:443".to_string())
        );
    }

    #[test]
    fn test_meeting_assignment_fields() {
        let now = Utc::now();
        let assignment = MeetingAssignment {
            meeting_id: "meeting-123".to_string(),
            meeting_controller_id: "mc-1".to_string(),
            region: "us-east-1".to_string(),
            assigned_at: now,
            assigned_by_gc_id: "gc-1".to_string(),
            ended_at: None,
        };

        assert_eq!(assignment.meeting_id, "meeting-123");
        assert_eq!(assignment.meeting_controller_id, "mc-1");
        assert_eq!(assignment.region, "us-east-1");
        assert_eq!(assignment.assigned_by_gc_id, "gc-1");
        assert!(assignment.ended_at.is_none());
    }
}
