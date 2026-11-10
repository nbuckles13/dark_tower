//! Media Handlers repository for database operations.
//!
//! Provides CRUD operations for the media_handlers table using sqlx.
//! Implements ADR-0010 Section 4a: MH registry in GC.
//!
//! # Security
//!
//! - All queries use parameterized statements (SQL injection safe)
//! - Sensitive data is not logged
//! - Registration runs the RegisterMH rule in one advisory-locked transaction

use crate::errors::GcError;
use crate::observability::metrics;
use crate::repositories::HealthStatus;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
use std::time::{Duration, Instant};
use tracing::instrument;

/// Number of candidate MHs to select for weighted random load balancing
/// (the SQL `LIMIT`, a `BIGINT` bind).
///
/// Must stay >= the maximum `GC_MH_INITIAL_HANDLERS_PER_MEETING`, or
/// selection could return fewer handlers than configured with more available.
/// Enforced at compile time below.
const LOAD_BALANCING_CANDIDATE_COUNT: i64 = 5;
const _: () = assert!(
    LOAD_BALANCING_CANDIDATE_COUNT > 0
        && LOAD_BALANCING_CANDIDATE_COUNT.unsigned_abs()
            >= crate::config::MAX_MH_INITIAL_HANDLERS_PER_MEETING,
    "LOAD_BALANCING_CANDIDATE_COUNT must cover the max initial handlers per meeting"
);

/// Convert a span to whole milliseconds for a `$n * INTERVAL '1 millisecond'`
/// bind. Every registry interval goes through here (one convention, no string
/// concatenation into SQL). Config bounds keep every configured span far below
/// `i64::MAX` ms; an out-of-range value is an internal error, never a wrap.
fn bind_ms(span: Duration) -> Result<i64, GcError> {
    i64::try_from(span.as_millis())
        .map_err(|_| GcError::Internal("registry interval exceeds i64 milliseconds".to_string()))
}

/// One `gc_mh_available` aggregate row.
///
/// `state` is the row's reported status when it is within the staleness bound,
/// and `Unhealthy` when it is older than the bound (not yet retention-deleted).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MhAvailabilityRow {
    /// Whether the row's region equals GC's own configured region.
    pub own_region: bool,
    /// Effective state (see type docs).
    pub state: HealthStatus,
    /// Number of rows.
    pub count: u64,
}

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

/// The advisory-lock key serialising every RegisterMH transaction
/// (`pg_advisory_xact_lock(hashtext(..))`, the idiom of
/// `crates/ac-service/src/handlers/admin_handler.rs`). One registry-wide key.
pub const MH_REGISTRY_LOCK_KEY: &str = "gc_mh_registry";

/// One RegisterMH request after arm 0 (allowlists) has passed.
#[derive(Debug, Clone, Copy)]
pub struct NewMhRegistration<'a> {
    /// MH-asserted incarnation id.
    pub handler_id: &'a str,
    /// MH-asserted region.
    pub region: &'a str,
    /// Canonical client-facing endpoint.
    pub webtransport_endpoint: &'a str,
    /// Canonical internal endpoint.
    pub grpc_endpoint: &'a str,
    /// Maximum concurrent streams (already range-checked).
    pub max_streams: i32,
}

/// The repository's RegisterMH decision (arms 1-3). The handler maps it to
/// the wire answer and the `gc_mh_registrations_total` outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MhRegistrationDecision {
    /// Registered; no other row matched.
    Registered,
    /// Registered; `ids` were deleted (arm 2a/2b).
    Superseded {
        /// Which endpoints matched (`Both` if any deleted row matched both).
        matched: metrics::MhSupersedeMatch,
        /// The superseded handler ids (for the log line, never a label).
        ids: Vec<String>,
    },
    /// Rejected, no writes: a one-sided match with a fresh serving row (2c).
    AddressConflict {
        /// The matched ids (for the log line, never the Status).
        conflicting_ids: Vec<String>,
    },
    /// Rejected, row unchanged: the id's endpoints differ from its row (1).
    /// Which endpoint(s) changed (for the log line; never the stored values).
    EndpointChanged {
        /// `webtransport_endpoint` differs from the stored row.
        webtransport: bool,
        /// `grpc_endpoint` differs from the stored row.
        grpc: bool,
    },
}

/// One other-id row sharing an endpoint, as decided on.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ConflictRow {
    handler_id: String,
    health_status: HealthStatus,
    wt_match: bool,
    grpc_match: bool,
    stale: bool,
}

#[derive(sqlx::FromRow)]
struct ConflictRowDb {
    handler_id: String,
    health_status: String,
    wt_match: bool,
    grpc_match: bool,
    stale: bool,
}

impl TryFrom<ConflictRowDb> for ConflictRow {
    type Error = GcError;

    /// Fail CLOSED on a status outside the schema's set: the lenient
    /// `from_db_str` would read it as `unhealthy`, which is supersedable.
    fn try_from(r: ConflictRowDb) -> Result<Self, GcError> {
        let health_status = HealthStatus::try_from_db_str(&r.health_status).ok_or_else(|| {
            GcError::Internal(format!(
                "media_handlers row has health_status outside the schema set: {:?}",
                r.health_status
            ))
        })?;
        Ok(ConflictRow {
            handler_id: r.handler_id,
            health_status,
            wt_match: r.wt_match,
            grpc_match: r.grpc_match,
            stale: r.stale,
        })
    }
}

/// Roll back on a registry-invariant breach WITHOUT losing the breach: if the
/// rollback itself fails, both contexts go into the returned error (a `?` on
/// the rollback would replace the breach with a generic database error, on
/// exactly the path meant to be loud).
async fn rollback_keeping(tx: sqlx::Transaction<'_, sqlx::Postgres>, breach: String) -> GcError {
    match tx.rollback().await {
        Ok(()) => GcError::Internal(breach),
        Err(rb) => GcError::Internal(format!("{breach}; rollback also failed: {rb}")),
    }
}

/// Outcome of [`decide`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Decision {
    /// Accept; `Some(match)` = every matched row is superseded.
    Accept(Option<metrics::MhSupersedeMatch>),
    /// Reject (arm 2c), no writes.
    Reject,
}

/// Arm 2 over the other ids' matched rows (pure; ADR-0039 §7):
/// a. both endpoints match -> supersede (match=both);
/// b. exactly one matches and the row is draining, unhealthy or stale ->
///    supersede (match=one_sided);
/// c. exactly one matches and the row is fresh and pending/healthy/degraded
///    -> reject the WHOLE request, whatever the other rows are. A live
///    serving MH is never displaced by a one-sided match. (A both-match row
///    plus a fresh one-sided row is legacy/mixed-version state: it rejects,
///    failing closed.)
fn decide(rows: &[ConflictRow]) -> Decision {
    let mut any_both = false;
    for row in rows {
        if row.wt_match && row.grpc_match {
            any_both = true;
            continue;
        }
        // Exhaustive, no `_` arm: a new status must be classified explicitly.
        let supersedable = row.stale
            || match row.health_status {
                HealthStatus::Draining | HealthStatus::Unhealthy => true,
                HealthStatus::Pending | HealthStatus::Healthy | HealthStatus::Degraded => false,
            };
        if !supersedable {
            return Decision::Reject;
        }
    }
    Decision::Accept(match (rows.is_empty(), any_both) {
        (true, _) => None,
        (false, true) => Some(metrics::MhSupersedeMatch::Both),
        (false, false) => Some(metrics::MhSupersedeMatch::OneSided),
    })
}

/// Repository for media handler operations.
pub struct MediaHandlersRepository;

impl MediaHandlersRepository {
    /// Register a media handler under the RegisterMH rule, arms 1-3
    /// (ADR-0039 §7; the rule is stated once above `RegisterMHRequest` in
    /// `internal.proto`). Arm 0 (the endpoint allowlists) is the caller's and
    /// has already passed; endpoints are canonical, so the exact string
    /// equality below is address equality.
    ///
    /// ONE transaction, serialised registry-wide by
    /// `pg_advisory_xact_lock(hashtext(MH_REGISTRY_LOCK_KEY))` as its first
    /// statement (registration is rare; per-endpoint keys would need ordered
    /// double-locking and still miss a new row overlapping two existing rows):
    ///
    /// 1. The id's own row (`FOR UPDATE`): either endpoint changed ->
    ///    rollback, [`MhRegistrationDecision::EndpointChanged`], row unchanged.
    /// 2. Other ids' rows sharing either endpoint (`FOR UPDATE`), staleness
    ///    computed in SQL against `staleness_bound` (GC-1's one bound) on the
    ///    DB clock; [`decide`] applies arms 2a/2b/2c.
    /// 3. DELETE the superseded ids, then the upsert, which never SETs an
    ///    endpoint (re-registering the same id with identical endpoints only
    ///    resets it to `pending` + `NOW()`).
    /// 4. COMMIT.
    ///
    /// One `gc_db_queries_total{operation="register_mh_txn"}` sample covers
    /// the whole transaction: `success` for every completed decision,
    /// INCLUDING the reject arms (the queries ran; the rule refused), and
    /// `error` for a database failure OR a registry-invariant breach observed
    /// under the lock (short supersede DELETE, upsert not affecting one row:
    /// a GC bug); the GC error log distinguishes them.
    ///
    /// # Errors
    ///
    /// `GcError::Database` on a database failure; `GcError::Internal` when the
    /// registry invariant is observed broken inside the lock (a status outside
    /// the schema's set, a short DELETE, or an upsert not affecting one row).
    #[instrument(skip_all, fields(handler_id = %reg.handler_id, region = %reg.region))]
    pub async fn register_mh(
        pool: &PgPool,
        reg: &NewMhRegistration<'_>,
        staleness_bound: Duration,
    ) -> Result<MhRegistrationDecision, GcError> {
        let bound_ms = bind_ms(staleness_bound)?;
        let start = Instant::now();
        let result = Self::register_mh_txn(pool, reg, bound_ms).await;
        // A rule reject is Ok(decision) here: `success`. Only Err is `error`.
        let status = if result.is_ok() { "success" } else { "error" };
        metrics::record_db_query("register_mh_txn", status, start.elapsed());
        result
    }

    /// The transaction body of [`Self::register_mh`]. Every exit (including
    /// the explicit rollbacks) returns here, so the caller records exactly
    /// one DB-query sample. Only DB statements run inside the transaction.
    async fn register_mh_txn(
        pool: &PgPool,
        reg: &NewMhRegistration<'_>,
        bound_ms: i64,
    ) -> Result<MhRegistrationDecision, GcError> {
        let mut tx = pool.begin().await?;

        // FIRST statement, on this transaction's connection: every read below
        // happens under the registry-wide lock.
        sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
            .bind(MH_REGISTRY_LOCK_KEY)
            .execute(&mut *tx)
            .await?;

        // Arm 1: an incarnation's endpoints are fixed for its life.
        let own: Option<(String, String)> = sqlx::query_as(
            r#"
            SELECT webtransport_endpoint, grpc_endpoint
            FROM media_handlers
            WHERE handler_id = $1
            FOR UPDATE
            "#,
        )
        .bind(reg.handler_id)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some((wt, grpc)) = own {
            let webtransport = wt != reg.webtransport_endpoint;
            let grpc = grpc != reg.grpc_endpoint;
            if webtransport || grpc {
                tx.rollback().await?;
                return Ok(MhRegistrationDecision::EndpointChanged { webtransport, grpc });
            }
        }

        // Arm 2: other ids' rows sharing either endpoint. `stale` uses the DB
        // clock and GC-1's bound (the same predicate as selection and the
        // sweep). NOW() is the transaction's START time, so a wait on the lock
        // makes rows look slightly fresher: that errs toward reject, the safe
        // direction. FOR UPDATE: a concurrent load report / sweep / retention
        // DELETE on a matched row waits for COMMIT, so the decision is made on
        // rows nothing else can change underneath it.
        let rows: Vec<ConflictRowDb> = sqlx::query_as(
            r#"
            SELECT
                handler_id,
                health_status,
                webtransport_endpoint = $2 AS wt_match,
                grpc_endpoint = $3 AS grpc_match,
                last_heartbeat_at < NOW() - $4 * INTERVAL '1 millisecond' AS stale
            FROM media_handlers
            WHERE handler_id <> $1
              AND (webtransport_endpoint = $2 OR grpc_endpoint = $3)
            FOR UPDATE
            "#,
        )
        .bind(reg.handler_id)
        .bind(reg.webtransport_endpoint)
        .bind(reg.grpc_endpoint)
        .bind(bound_ms)
        .fetch_all(&mut *tx)
        .await?;
        let rows = rows
            .into_iter()
            .map(ConflictRow::try_from)
            .collect::<Result<Vec<_>, _>>()?;

        let superseded = match decide(&rows) {
            Decision::Reject => {
                tx.rollback().await?;
                return Ok(MhRegistrationDecision::AddressConflict {
                    conflicting_ids: rows.into_iter().map(|r| r.handler_id).collect(),
                });
            }
            Decision::Accept(m) => m.map(|m| {
                (
                    m,
                    rows.into_iter().map(|r| r.handler_id).collect::<Vec<_>>(),
                )
            }),
        };

        if let Some((_, ids)) = &superseded {
            let deleted = sqlx::query("DELETE FROM media_handlers WHERE handler_id = ANY($1)")
                .bind(ids)
                .execute(&mut *tx)
                .await?
                .rows_affected();
            // Every decided id is a row this transaction holds FOR UPDATE, and
            // row locks block EVERY writer (old-version replicas, load reports,
            // the sweep, retention, a manual DELETE): a retention DELETE that
            // was already running made our SELECT wait and, under READ
            // COMMITTED, dropped that row from the result, so it never became a
            // decided id. A short count is therefore unreachable while row
            // locks hold: it means a bug in this function. Fail loud.
            if deleted != ids.len() as u64 {
                return Err(rollback_keeping(
                    tx,
                    format!(
                        "register_mh: deleted {deleted} of {} superseded rows under the registry lock",
                        ids.len()
                    ),
                )
                .await);
            }
        }

        // The ON CONFLICT path only re-registers the same id with identical
        // endpoints (arm 1 returned above otherwise). Endpoints are never SET;
        // the WHERE makes a future regression in arm 1 fail loudly here
        // instead of silently rewriting them.
        let upserted = sqlx::query(
            r#"
            INSERT INTO media_handlers (
                handler_id, region, webtransport_endpoint, grpc_endpoint,
                max_streams, health_status, last_heartbeat_at
            )
            VALUES ($1, $2, $3, $4, $5, 'pending', NOW())
            ON CONFLICT (handler_id) DO UPDATE SET
                region = EXCLUDED.region,
                max_streams = EXCLUDED.max_streams,
                health_status = 'pending',
                last_heartbeat_at = NOW(),
                updated_at = NOW()
            WHERE media_handlers.webtransport_endpoint = EXCLUDED.webtransport_endpoint
              AND media_handlers.grpc_endpoint = EXCLUDED.grpc_endpoint
            "#,
        )
        .bind(reg.handler_id)
        .bind(reg.region)
        .bind(reg.webtransport_endpoint)
        .bind(reg.grpc_endpoint)
        .bind(reg.max_streams)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if upserted != 1 {
            return Err(rollback_keeping(
                tx,
                format!("register_mh: upsert affected {upserted} rows (expected 1)"),
            )
            .await);
        }

        tx.commit().await?;

        Ok(match superseded {
            None => MhRegistrationDecision::Registered,
            Some((matched, ids)) => MhRegistrationDecision::Superseded { matched, ids },
        })
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
    /// Any handler — draining included — with no load report within the
    /// staleness bound is marked unhealthy. A live draining MH keeps reporting
    /// DRAINING, so it is never swept; one that exited must not stay
    /// 'draining' forever (ADR-0039 §10). Rows already unhealthy are skipped
    /// so the returned count is transitions only.
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `staleness_bound` - The MH staleness bound (`MhRegistryConfig::staleness_bound`)
    ///
    /// # Returns
    ///
    /// Returns the number of handlers marked as unhealthy.
    #[instrument(skip_all, fields(staleness_bound = ?staleness_bound))]
    pub async fn mark_stale_handlers_unhealthy(
        pool: &PgPool,
        staleness_bound: Duration,
    ) -> Result<u64, GcError> {
        let bound_ms = bind_ms(staleness_bound)?;
        let start = Instant::now();

        let query_result = sqlx::query(
            r#"
            UPDATE media_handlers
            SET
                health_status = 'unhealthy',
                updated_at = NOW()
            WHERE
                last_heartbeat_at < NOW() - $1 * INTERVAL '1 millisecond'
                AND health_status != 'unhealthy'
            "#,
        )
        .bind(bound_ms)
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
                threshold_ms = bound_ms,
                "Marked stale handlers as unhealthy"
            );
        }

        Ok(count)
    }

    /// Delete registry rows with no load report within `retention`.
    ///
    /// Rows are per incarnation (`handler_id`), so a restarted MH leaves its
    /// old row behind; this bounds the table. Config guarantees retention >
    /// staleness bound, so a live (reporting) MH — draining or not — is never
    /// deleted. A load report for a deleted row stays an UPDATE matching zero
    /// rows (NOT_FOUND); the row is never recreated there.
    ///
    /// # Returns
    ///
    /// Number of rows deleted.
    #[instrument(skip_all, fields(retention = ?retention))]
    pub async fn delete_expired_handlers(
        pool: &PgPool,
        retention: Duration,
    ) -> Result<u64, GcError> {
        let retention_ms = bind_ms(retention)?;
        let start = Instant::now();

        let query_result = sqlx::query(
            r#"
            DELETE FROM media_handlers
            WHERE last_heartbeat_at < NOW() - $1 * INTERVAL '1 millisecond'
            "#,
        )
        .bind(retention_ms)
        .execute(pool)
        .await;

        let (status, result) = match query_result {
            Ok(r) => ("success", Ok(r)),
            Err(e) => ("error", Err(e)),
        };
        metrics::record_db_query("delete_expired_mh", status, start.elapsed());

        let count = result?.rows_affected();

        if count > 0 {
            tracing::info!(
                target: "gc.repository.mh",
                count = count,
                retention_ms = retention_ms,
                "Deleted expired media handler registry rows"
            );
        }

        Ok(count)
    }

    /// Count registry rows by (own region?, effective state) for `gc_mh_available`.
    ///
    /// One aggregate. A row within the staleness bound counts under its
    /// reported state; an older row counts as `unhealthy`. The MH-asserted
    /// region string is reduced to "equals GC's own region" here, so it never
    /// leaves the database as a label value.
    #[instrument(skip_all)]
    pub async fn count_available_handlers(
        pool: &PgPool,
        staleness_bound: Duration,
        own_region: &str,
    ) -> Result<Vec<MhAvailabilityRow>, GcError> {
        let bound_ms = bind_ms(staleness_bound)?;
        let start = Instant::now();

        let query_result = sqlx::query(
            r#"
            SELECT
                (region = $2) AS own_region,
                CASE
                    WHEN last_heartbeat_at < NOW() - $1 * INTERVAL '1 millisecond'
                        THEN 'unhealthy'
                    ELSE health_status
                END AS state,
                COUNT(*) AS count
            FROM media_handlers
            GROUP BY 1, 2
            "#,
        )
        .bind(bound_ms)
        .bind(own_region)
        .fetch_all(pool)
        .await;

        let (status, result) = match query_result {
            Ok(r) => ("success", Ok(r)),
            Err(e) => ("error", Err(e)),
        };
        metrics::record_db_query("count_available_mh", status, start.elapsed());

        result?
            .into_iter()
            .map(|row| -> Result<MhAvailabilityRow, GcError> {
                let own_region: bool = row.try_get("own_region")?;
                let state: String = row.try_get("state")?;
                let count: i64 = row.try_get("count")?;
                let count = u64::try_from(count).map_err(|_| {
                    GcError::Internal("negative COUNT(*) from registry".to_string())
                })?;
                Ok(MhAvailabilityRow {
                    own_region,
                    state: HealthStatus::from_db_str(&state),
                    count,
                })
            })
            .collect()
    }

    /// Get candidate MHs for load balancing in a region.
    ///
    /// Returns up to 5 healthy MHs with capacity and a load report within the
    /// staleness bound, ordered by load ratio (least loaded first). Only
    /// `health_status = 'healthy'` qualifies, so a draining, degraded, pending
    /// or unhealthy MH is never a candidate (ADR-0039 §7).
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `region` - Deployment region
    /// * `staleness_bound` - The MH staleness bound (`MhRegistryConfig::staleness_bound`)
    #[instrument(skip_all, fields(region = %region))]
    pub async fn get_candidate_mhs(
        pool: &PgPool,
        region: &str,
        staleness_bound: Duration,
    ) -> Result<Vec<MhCandidate>, GcError> {
        let bound_ms = bind_ms(staleness_bound)?;
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
              AND last_heartbeat_at > NOW() - $2 * INTERVAL '1 millisecond'
            ORDER BY load_ratio ASC, last_heartbeat_at DESC
            LIMIT $3
            "#,
        )
        .bind(region)
        .bind(bound_ms)
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

    fn row(status: HealthStatus, wt: bool, grpc: bool, stale: bool) -> ConflictRow {
        ConflictRow {
            handler_id: "other".to_string(),
            health_status: status,
            wt_match: wt,
            grpc_match: grpc,
            stale,
        }
    }

    #[test]
    fn decide_table() {
        use crate::observability::metrics::MhSupersedeMatch::{Both, OneSided};
        use HealthStatus::{Degraded, Draining, Healthy, Pending, Unhealthy};
        let cases: Vec<(&str, Vec<ConflictRow>, Decision)> = vec![
            ("no match", vec![], Decision::Accept(None)),
            (
                "2a fresh healthy both",
                vec![row(Healthy, true, true, false)],
                Decision::Accept(Some(Both)),
            ),
            (
                "2b draining wt-only",
                vec![row(Draining, true, false, false)],
                Decision::Accept(Some(OneSided)),
            ),
            (
                "2b unhealthy grpc-only",
                vec![row(Unhealthy, false, true, false)],
                Decision::Accept(Some(OneSided)),
            ),
            (
                "2b stale healthy",
                vec![row(Healthy, true, false, true)],
                Decision::Accept(Some(OneSided)),
            ),
            (
                "2c fresh pending",
                vec![row(Pending, true, false, false)],
                Decision::Reject,
            ),
            (
                "2c fresh healthy",
                vec![row(Healthy, false, true, false)],
                Decision::Reject,
            ),
            (
                "2c fresh degraded",
                vec![row(Degraded, true, false, false)],
                Decision::Reject,
            ),
            (
                "two one-sided rows, both supersedable",
                vec![
                    row(Healthy, true, false, true),
                    row(Draining, false, true, false),
                ],
                Decision::Accept(Some(OneSided)),
            ),
            (
                "two one-sided rows, one fresh serving",
                vec![
                    row(Healthy, true, false, true),
                    row(Healthy, false, true, false),
                ],
                Decision::Reject,
            ),
            (
                "both-match plus fresh one-sided (legacy state) fails closed",
                vec![
                    row(Healthy, true, true, false),
                    row(Healthy, true, false, false),
                ],
                Decision::Reject,
            ),
            (
                "both-match plus stale one-sided",
                vec![
                    row(Healthy, true, true, false),
                    row(Pending, false, true, true),
                ],
                Decision::Accept(Some(Both)),
            ),
        ];
        for (why, rows, want) in cases {
            assert_eq!(decide(&rows), want, "{why}");
        }
    }

    #[test]
    fn conflict_row_unknown_status_fails_closed() {
        let r = ConflictRow::try_from(ConflictRowDb {
            handler_id: "x".to_string(),
            health_status: "zombie".to_string(),
            wt_match: true,
            grpc_match: false,
            stale: false,
        });
        assert!(matches!(r, Err(GcError::Internal(_))), "{r:?}");
        let ok = ConflictRow::try_from(ConflictRowDb {
            handler_id: "x".to_string(),
            health_status: "draining".to_string(),
            wt_match: true,
            grpc_match: false,
            stale: false,
        });
        assert!(ok.is_ok(), "positive twin");
    }

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
