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
use crate::observability::metrics;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use std::time::{Duration, Instant};
use tracing::instrument;

/// SQLSTATE `query_canceled`: what PostgreSQL returns when a statement
/// exceeds `statement_timeout`.
const SQLSTATE_QUERY_CANCELED: &str = "57014";

/// THE MC liveness predicate, the one definition of "this MC is alive" for
/// assignment rows: its heartbeat is younger than the configured staleness
/// threshold (`MC_STALENESS_THRESHOLD_SECONDS`, the value the health checker
/// marks stale MCs with). Expands to a SQL string LITERAL (so queries stay
/// static, `SqlSafeStr`): `$alias` is the `meeting_controllers` alias and
/// `$param` the placeholder binding [`staleness_param`].
///
/// Liveness is heartbeat FRESHNESS, never `health_status`. Status is
/// placement eligibility: a `degraded`, `draining` or `pending`
/// (re-registered) MC still heartbeats and still holds its meetings, and
/// `mark_stale_controllers_unhealthy` never re-marks a `draining` MC that
/// dies. Every query that decides whether a row's MC still holds the meeting
/// (join reuse, the failover arm, the cleanup backstop) uses this macro, so
/// the three cannot disagree. New-meeting PLACEMENT (`get_candidate_mcs`)
/// additionally requires `health_status = 'healthy'`.
macro_rules! mc_is_live {
    ($alias:literal, $param:literal) => {
        concat!(
            $alias,
            ".last_heartbeat_at >= NOW() - (",
            $param,
            " || ' seconds')::INTERVAL"
        )
    };
}

/// The staleness threshold as the text `mc_is_live!` binds.
fn staleness_param(mc_staleness: Duration) -> String {
    mc_staleness.as_secs().to_string()
}

/// What is left of the accept->commit budget, or `None` once it is spent.
fn remaining_budget(deadline: Instant) -> Option<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|d| !d.is_zero())
}

/// True for PostgreSQL's `statement_timeout` cancellation (SQLSTATE 57014).
fn is_query_canceled(e: &sqlx::Error) -> bool {
    e.as_database_error()
        .and_then(|d| d.code())
        .is_some_and(|code| code == SQLSTATE_QUERY_CANCELED)
}

/// One meeting incarnation's fence (ADR-0010 §3). Defined once in `common`
/// so GC and MC share one definition of a valid wire value.
pub use common::meeting_lifecycle::AssignmentId;

/// Outcome of [`MeetingAssignmentsRepository::atomic_assign`].
#[derive(Debug, Clone)]
pub enum AtomicAssignOutcome {
    /// The row now names an MC: ours if we wrote it, or the winner's if a
    /// concurrent assignment holds a live row (`assignment_id` tells which).
    Assigned {
        /// The MC the row names.
        assignment: McAssignment,
        /// Whether THIS call wrote the row (our MC and our id).
        written: bool,
    },
    /// The accept->commit budget ran out before the row committed: no row
    /// was written. The MC that accepted holds an orphan meeting whose
    /// fenced notify can end nothing; the caller tries the next MC.
    BudgetExceeded,
}

/// Outcome of [`MeetingAssignmentsRepository::end_assignment`], decided in
/// ONE statement so the three cases are read from one snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndAssignmentOutcome {
    /// The named incarnation was live and is now ended.
    Ended,
    /// THIS incarnation's row exists and was already ended (a re-sent
    /// notify, or the backstop got there first).
    AlreadyEnded,
    /// No row of this incarnation exists: it never committed (an orphan of a
    /// lost race or a missed accept->commit budget) or a revive / failover
    /// re-point replaced it. Nothing was ended.
    Fenced,
}

impl EndAssignmentOutcome {
    /// Every variant (metric zero-init enumerates this).
    pub const ALL: [EndAssignmentOutcome; 3] = [Self::Ended, Self::AlreadyEnded, Self::Fenced];

    /// `gc_meeting_end_notifications_total{outcome}` label.
    #[must_use]
    pub fn as_label(self) -> &'static str {
        match self {
            Self::Ended => "ended",
            Self::AlreadyEnded => "already_ended",
            Self::Fenced => "fenced",
        }
    }
}

/// Number of candidate MCs to select for weighted random load balancing.
const LOAD_BALANCING_CANDIDATE_COUNT: i64 = 5;

/// Default batch size for cleanup operations.
/// Prevents long-running transactions in pathological cases.
const DEFAULT_CLEANUP_BATCH_SIZE: i64 = 1000;

/// Meeting assignment record from database.
// Allow: Fields used in tests; struct will be used by future query handlers
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
    /// The live assignment of a meeting in a region, for join reuse.
    ///
    /// Returns the active row when its MC is ALIVE (`mc_is_live!`), whatever
    /// that MC's `health_status`: a draining, degraded or re-registered MC
    /// still holds its meetings, and sending a joiner anywhere else would
    /// split the meeting. `None` when there is no active row, or its MC has
    /// stopped heartbeating (the caller then assigns afresh, and
    /// `atomic_assign`'s failover arm re-points the row).
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `meeting_id` - Meeting identifier
    /// * `region` - Deployment region
    /// * `mc_staleness` - MC liveness threshold (`MC_STALENESS_THRESHOLD_SECONDS`)
    #[instrument(skip_all, fields(meeting_id = %meeting_id, region = %region))]
    pub async fn get_live_assignment(
        pool: &PgPool,
        meeting_id: &str,
        region: &str,
        mc_staleness: Duration,
    ) -> Result<Option<McAssignment>, GcError> {
        let start = Instant::now();

        let sql = concat!(
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
              AND "#,
            mc_is_live!("mc", "$3"),
            r#"
            "#,
        );
        let result: Result<Option<McAssignmentRow>, sqlx::Error> = sqlx::query_as(sql)
            .bind(meeting_id)
            .bind(region)
            .bind(staleness_param(mc_staleness))
            .fetch_optional(pool)
            .await;

        // Record DB query metrics (ADR-0011)
        let (status, row) = match result {
            Ok(r) => ("success", Ok(r)),
            Err(e) => ("error", Err(e)),
        };
        metrics::record_db_query("get_live_assignment", status, start.elapsed());

        Ok(row?.map(|r| McAssignment {
            mc_id: r.meeting_controller_id,
            grpc_endpoint: r.grpc_endpoint,
            webtransport_endpoint: r.webtransport_endpoint,
        }))
    }

    /// Region of the ACTIVE assignment of `meeting_id` to `controller_id`
    /// (the `AcquireMediaHandler` ownership check, ADR-0039 §3).
    ///
    /// `Ok(None)` when no active row pairs this meeting with this controller:
    /// the meeting does not exist, has ended, or is owned by another
    /// controller. Callers must not distinguish those cases to the peer.
    ///
    /// A sibling of [`Self::get_live_assignment`] rather than a reuse: that
    /// query keys on (meeting, region) and joins MC liveness; this one keys on
    /// (meeting, controller), ignores MC liveness, and returns the region.
    ///
    /// # Errors
    ///
    /// `GcError::Database` on a query failure. `GcError::Internal` when the
    /// controller holds the meeting in MORE than one region: the primary key
    /// (meeting_id, region) allows it, and only MCs being region-bound
    /// prevents it, so it is a broken invariant, never resolved by picking a row.
    #[instrument(skip_all)]
    pub async fn get_active_assignment_region(
        pool: &PgPool,
        meeting_id: &str,
        controller_id: &str,
    ) -> Result<Option<String>, GcError> {
        let start = Instant::now();

        // Served by idx_assignments_by_region (meeting_id, region) WHERE
        // ended_at IS NULL. LIMIT 2: enough to detect a second region.
        let result: Result<Vec<(String,)>, sqlx::Error> = sqlx::query_as(
            r#"
            SELECT region
            FROM meeting_assignments
            WHERE meeting_id = $1
              AND meeting_controller_id = $2
              AND ended_at IS NULL
            LIMIT 2
            "#,
        )
        .bind(meeting_id)
        .bind(controller_id)
        .fetch_all(pool)
        .await;

        let (status, rows) = match result {
            Ok(r) => ("success", Ok(r)),
            Err(e) => ("error", Err(e)),
        };
        metrics::record_db_query("get_active_assignment_region", status, start.elapsed());

        let mut rows = rows?;
        if rows.len() > 1 {
            tracing::error!(
                target: "gc.repository.assignments",
                meeting_id = %meeting_id,
                controller_id = %controller_id,
                "Controller holds one meeting in multiple regions (broken invariant)"
            );
            return Err(GcError::Internal(
                "multiple active assignments for one (meeting, controller) pair".to_string(),
            ));
        }
        Ok(rows.pop().map(|(region,)| region))
    }

    /// Get candidate MCs for load balancing in a region.
    ///
    /// Returns up to 5 healthy, live MCs with capacity, ordered by load ratio
    /// (least loaded first).
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `region` - Deployment region
    /// * `mc_staleness` - MC liveness threshold (`MC_STALENESS_THRESHOLD_SECONDS`)
    #[instrument(skip_all, fields(region = %region))]
    pub async fn get_candidate_mcs(
        pool: &PgPool,
        region: &str,
        mc_staleness: Duration,
    ) -> Result<Vec<McCandidate>, GcError> {
        let start = Instant::now();

        // Placement: alive AND `healthy` (a draining/degraded MC keeps its
        // meetings but takes no new ones).
        let sql = concat!(
            r#"
            SELECT
                mc.controller_id,
                mc.grpc_endpoint,
                mc.webtransport_endpoint,
                CASE
                    WHEN mc.max_meetings = 0 THEN 1.0
                    ELSE (mc.current_meetings::float / mc.max_meetings)
                END AS load_ratio
            FROM meeting_controllers mc
            WHERE mc.health_status = 'healthy'
              AND mc.region = $1
              AND mc.current_meetings < mc.max_meetings
              AND "#,
            mc_is_live!("mc", "$2"),
            r#"
            ORDER BY load_ratio ASC, mc.last_heartbeat_at DESC
            LIMIT $3
            "#,
        );
        let result: Result<Vec<McCandidateRow>, sqlx::Error> = sqlx::query_as(sql)
            .bind(region)
            .bind(staleness_param(mc_staleness))
            .bind(LOAD_BALANCING_CANDIDATE_COUNT)
            .fetch_all(pool)
            .await;

        // Record DB query metrics (ADR-0011)
        let (status, rows) = match result {
            Ok(r) => ("success", Ok(r)),
            Err(e) => ("error", Err(e)),
        };
        metrics::record_db_query("get_candidate_mcs", status, start.elapsed());

        Ok(rows?
            .into_iter()
            .map(|r| McCandidate {
                controller_id: r.controller_id,
                grpc_endpoint: r.grpc_endpoint,
                webtransport_endpoint: r.webtransport_endpoint,
                load_ratio: r.load_ratio,
            })
            .collect())
    }

    /// Atomically assign a meeting to an MC using ADR-0010 algorithm, within
    /// the accept->commit budget.
    ///
    /// Called only AFTER `selected_mc` accepted `assignment_id` over
    /// `AssignMeetingWithMh`. This operation:
    /// 1. Re-points an existing assignment whose MC is NO LONGER ALIVE
    ///    (`mc_is_live!`) — never one whose MC still heartbeats, whatever its
    ///    `health_status`, because that MC still holds the meeting
    /// 2. Revives an ENDED assignment row — a new incarnation of the meeting
    ///    after its MC reported it ended (`notify_meeting_ended`)
    /// 3. Inserts a new assignment if there is no row at all
    /// 4. Returns the assignment (ours, or the winner's if the race was lost)
    ///
    /// Every write sets `assignment_id`, so the row always names the
    /// incarnation its MC accepted. That holds because MC accepts at most one
    /// incarnation per live meeting (a duplicate `AssignMeetingWithMh` is
    /// refused) and GC writes only after acceptance: a live row
    /// (controller X, id i) implies X accepted i.
    ///
    /// # The accept->commit budget
    ///
    /// MC ends an unjoined meeting at its join window, measured from its
    /// acceptance. If this row committed AFTER that, MC's fenced
    /// `NotifyMeetingEnded` would have found nothing to end and the row would
    /// name a live MC that no longer holds the meeting — stranded until that
    /// MC dies. So the write must commit before `deadline` (the caller sets it
    /// from when it SENT the assignment, at most half of
    /// `common::meeting_lifecycle::MIN_MEETING_JOIN_WINDOW` later):
    /// pool acquire (bounded by the remaining budget) -> BEGIN -> elapsed
    /// check -> `statement_timeout` = remaining -> upsert -> COMMIT. The
    /// elapsed check comes after acquire and BEGIN because waiting for a pool
    /// connection is the likeliest GC-side stall. Any miss returns
    /// [`AtomicAssignOutcome::BudgetExceeded`] with NO row written; the MC
    /// that accepted then holds an orphan whose notify is fenced.
    ///
    /// The budget is re-checked once more just before COMMIT (a written row
    /// whose budget ran out during the upsert is rolled back, not committed).
    ///
    /// Residual: neither the re-check nor `statement_timeout` can stop a
    /// COMMIT once sent, and neither covers network time; the other half of
    /// the join window is the margin for both. A breach shows up as
    /// `gc_meeting_end_notifications_total{outcome="fenced"}`.
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `meeting_id` - Meeting identifier
    /// * `region` - Deployment region
    /// * `selected_mc` - The MC that accepted (from weighted selection)
    /// * `gc_id` - ID of this GC instance
    /// * `assignment_id` - The incarnation id `selected_mc` accepted
    /// * `mc_staleness` - MC liveness threshold (`MC_STALENESS_THRESHOLD_SECONDS`)
    /// * `deadline` - Commit-by instant (the accept->commit budget)
    #[expect(
        clippy::too_many_arguments,
        reason = "each is a distinct input of the one assignment write"
    )]
    #[instrument(skip_all, fields(meeting_id = %meeting_id, region = %region, mc_id = %selected_mc.controller_id))]
    pub async fn atomic_assign(
        pool: &PgPool,
        meeting_id: &str,
        region: &str,
        selected_mc: &McCandidate,
        gc_id: &str,
        assignment_id: AssignmentId,
        mc_staleness: Duration,
        deadline: Instant,
    ) -> Result<AtomicAssignOutcome, GcError> {
        let start = Instant::now();
        let outcome = Self::atomic_assign_txn(
            pool,
            meeting_id,
            region,
            selected_mc,
            gc_id,
            assignment_id,
            mc_staleness,
            deadline,
        )
        .await;

        // Record DB query metrics (ADR-0011). A budget miss is a failed write.
        let status = match &outcome {
            Ok(AtomicAssignOutcome::Assigned { .. }) => "success",
            Ok(AtomicAssignOutcome::BudgetExceeded) | Err(_) => "error",
        };
        metrics::record_db_query("atomic_assign", status, start.elapsed());

        if matches!(outcome, Ok(AtomicAssignOutcome::BudgetExceeded)) {
            tracing::warn!(
                target: "gc.repository.assignments",
                meeting_id = %meeting_id,
                region = %region,
                mc_id = %selected_mc.controller_id,
                assignment_id = %assignment_id,
                "Assignment not written: accept->commit budget exhausted (the accepting MC holds a fenced orphan)"
            );
        }
        outcome
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "the transaction body of `atomic_assign`, same inputs"
    )]
    async fn atomic_assign_txn(
        pool: &PgPool,
        meeting_id: &str,
        region: &str,
        selected_mc: &McCandidate,
        gc_id: &str,
        assignment_id: AssignmentId,
        mc_staleness: Duration,
        deadline: Instant,
    ) -> Result<AtomicAssignOutcome, GcError> {
        // Acquire + BEGIN, the wait bounded by what is left of the budget.
        let Some(remaining) = remaining_budget(deadline) else {
            return Ok(AtomicAssignOutcome::BudgetExceeded);
        };
        let mut tx = match tokio::time::timeout(remaining, pool.begin()).await {
            Err(_elapsed) => return Ok(AtomicAssignOutcome::BudgetExceeded),
            Ok(begun) => begun?,
        };

        // Elapsed check AFTER acquire and BEGIN; the transaction is rolled
        // back on drop on every early return below.
        let Some(remaining) = remaining_budget(deadline) else {
            return Ok(AtomicAssignOutcome::BudgetExceeded);
        };
        // `SET LOCAL` cannot take a bind parameter; `set_config(.., true)` is
        // its transaction-scoped equivalent. Whole milliseconds, at least 1
        // (0 would DISABLE the timeout).
        let timeout_ms = remaining.as_millis().max(1);
        sqlx::query("SELECT set_config('statement_timeout', $1, true)")
            .bind(format!("{timeout_ms}ms"))
            .execute(&mut *tx)
            .await?;

        // INSERT ... ON CONFLICT DO UPDATE atomically handles:
        // - New assignments (no existing row)
        // - Re-pointing a row whose MC is no longer alive (failover)
        // - Reviving an ENDED row (a meeting re-joined after it ended)
        // - Races (several GCs assigning at once): the PK row lock
        //   serializes them; a loser re-reads a row that is now live on a
        //   live MC, neither arm fires, nothing updates, and it returns the
        //   winner's row below.
        //
        // WHY AN ENDED ROW MUST BE REVIVABLE (story 2 task 12). The key is the
        // non-partial `PRIMARY KEY (meeting_id, region)`, and an ended
        // assignment is a SOFT delete that keeps occupying the key until
        // `cleanup_old_assignments` removes it `GC_RETENTION_DAYS` later.
        // Without the revive arm a rejoin of an ended meeting would conflict,
        // update nothing, and fail until the retention cleanup. Revived in
        // place rather than via a partial unique index, which would drop the
        // PK that this ON CONFLICT target needs.
        //
        // BOTH HALVES ARE REQUIRED. `ended_at IS NOT NULL` lets an ended row
        // be replaced; `ended_at = NULL` in the SET makes the replacement
        // LIVE. Without the second, the failover arm would re-point an ended
        // row while it stayed ended, and lookups would keep returning nothing.
        //
        // THE FAILOVER ARM KEYS ON LIVENESS, NOT STATUS. Re-pointing a row
        // whose MC still heartbeats (draining, degraded, re-registered) would
        // split a live meeting across two MCs. `get_live_assignment` reuses
        // such a row, so a join normally never reaches here for it; when one
        // races in, the arm does not fire and the caller gets the live row.
        //
        // INCARNATION FENCE. Every write sets `assignment_id`, so a late
        // `NotifyMeetingEnded` from a previous incarnation (revive) or from
        // the MC a row failed over FROM can no longer end this row:
        // `end_assignment` matches controller AND id.
        let sql = concat!(
            r#"
            INSERT INTO meeting_assignments
                (meeting_id, meeting_controller_id, region, assigned_by_gc_id, assignment_id)
            VALUES ($1, $3, $2, $4, $5)
            ON CONFLICT (meeting_id, region) DO UPDATE
            SET meeting_controller_id = EXCLUDED.meeting_controller_id,
                assigned_by_gc_id = EXCLUDED.assigned_by_gc_id,
                assignment_id = EXCLUDED.assignment_id,
                assigned_at = NOW(),
                ended_at = NULL
            WHERE meeting_assignments.ended_at IS NOT NULL
               OR NOT EXISTS (
                -- Re-point only when the row's MC is no longer alive
                SELECT 1 FROM meeting_controllers mc
                WHERE mc.controller_id = meeting_assignments.meeting_controller_id
                  AND "#,
            mc_is_live!("mc", "$6"),
            r#"
            )
            RETURNING meeting_controller_id
            "#,
        );
        let upsert: Result<Option<AtomicAssignResult>, sqlx::Error> = sqlx::query_as(sql)
            .bind(meeting_id)
            .bind(region)
            .bind(&selected_mc.controller_id)
            .bind(gc_id)
            .bind(assignment_id.as_uuid())
            .bind(staleness_param(mc_staleness))
            .fetch_optional(&mut *tx)
            .await;

        let written = match upsert {
            Ok(row) => row.is_some(),
            Err(e) if is_query_canceled(&e) => return Ok(AtomicAssignOutcome::BudgetExceeded),
            Err(e) => return Err(e.into()),
        };

        let assignment = if written {
            McAssignment {
                mc_id: selected_mc.controller_id.clone(),
                grpc_endpoint: selected_mc.grpc_endpoint.clone(),
                webtransport_endpoint: selected_mc.webtransport_endpoint.clone(),
            }
        } else {
            // Lost the race: a live row on a live MC exists. Read it in the
            // same transaction.
            tracing::debug!(
                target: "gc.repository.assignments",
                meeting_id = %meeting_id,
                region = %region,
                "Insert/update returned no rows, fetching current assignment"
            );
            let current = match Self::get_current_assignment(&mut tx, meeting_id, region).await {
                Ok(current) => current,
                Err(e) if is_query_canceled(&e) => return Ok(AtomicAssignOutcome::BudgetExceeded),
                Err(e) => return Err(e.into()),
            };
            match current {
                Some(current) => current,
                None => {
                    // This shouldn't happen - indicates a bug or DB inconsistency
                    tracing::error!(
                        target: "gc.repository.assignments",
                        meeting_id = %meeting_id,
                        region = %region,
                        "Assignment failed - no current assignment found"
                    );
                    return Err(GcError::ServiceUnavailable(
                        "Failed to assign meeting controller - please retry".to_string(),
                    ));
                }
            }
        };

        // Re-check the budget before COMMIT: once COMMIT is sent nothing can
        // stop it, so a write whose budget ran out while the upsert ran is
        // rolled back here (on drop) rather than committed late. What remains
        // uncovered is a COMMIT that itself outlasts the budget (and network
        // time), which the other half of the join window absorbs.
        if written && remaining_budget(deadline).is_none() {
            return Ok(AtomicAssignOutcome::BudgetExceeded);
        }

        match tx.commit().await {
            Ok(()) => Ok(AtomicAssignOutcome::Assigned {
                assignment,
                written,
            }),
            Err(e) if is_query_canceled(&e) => Ok(AtomicAssignOutcome::BudgetExceeded),
            Err(e) => Err(e.into()),
        }
    }

    /// The active assignment for (meeting, region) regardless of MC
    /// liveness, read inside `atomic_assign`'s transaction after losing a race.
    async fn get_current_assignment(
        conn: &mut sqlx::PgConnection,
        meeting_id: &str,
        region: &str,
    ) -> Result<Option<McAssignment>, sqlx::Error> {
        let start = Instant::now();

        let query_result: Result<Option<McAssignmentRow>, sqlx::Error> = sqlx::query_as(
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
        .fetch_optional(conn)
        .await;

        // Record DB query metrics (ADR-0011)
        let status = if query_result.is_ok() {
            "success"
        } else {
            "error"
        };
        metrics::record_db_query("get_current_assignment", status, start.elapsed());

        Ok(query_result?.map(|r| McAssignment {
            mc_id: r.meeting_controller_id,
            grpc_endpoint: r.grpc_endpoint,
            webtransport_endpoint: r.webtransport_endpoint,
        }))
    }

    /// End ONE incarnation of a meeting's assignment (soft delete), fenced.
    ///
    /// Called for `NotifyMeetingEnded`. Ends the row only when it is live AND
    /// names `controller_id` AND `assignment_id`: a notify can end nothing
    /// but the incarnation it names, so a late notify (after a revive or a
    /// failover re-point) or one from an orphan meeting (a lost assignment
    /// race) is harmless, and MC may re-send a notify safely.
    ///
    /// ONE statement (a data-modifying CTE): the conditional UPDATE, plus a
    /// probe for THIS incarnation's row regardless of `ended_at`, both read
    /// from the statement's snapshot. `AlreadyEnded` is exact (our row exists
    /// and was already ended); everything else that ended nothing is
    /// `Fenced`, including a notify for an incarnation whose row was never
    /// written (an orphan) or was overwritten by a revive / re-point.
    ///
    /// `controller_id` is self-asserted by the caller under the shared MC
    /// service credential: a narrowing match (no misrouted end), not an
    /// authorization check.
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `meeting_id` - Meeting identifier
    /// * `region` - Deployment region
    /// * `controller_id` - The notifying MC
    /// * `assignment_id` - The incarnation MC is ending
    #[instrument(skip_all, fields(meeting_id = %meeting_id, region = %region))]
    pub async fn end_assignment(
        pool: &PgPool,
        meeting_id: &str,
        region: &str,
        controller_id: &str,
        assignment_id: AssignmentId,
    ) -> Result<EndAssignmentOutcome, GcError> {
        let start = Instant::now();

        // `own_row`: does a row of THIS incarnation exist at all (ended or
        // not)? Read only when `ended` is false; then our row, if present,
        // was already ended, so `already_ended` is exact. Anything else — a
        // row of another incarnation or controller, or no row at all (an
        // orphan whose row was never written) — is `fenced`.
        let query_result: Result<(bool, bool), sqlx::Error> = sqlx::query_as(
            r#"
            WITH ended AS (
                UPDATE meeting_assignments
                SET ended_at = NOW()
                WHERE meeting_id = $1
                  AND region = $2
                  AND meeting_controller_id = $3
                  AND assignment_id = $4
                  AND ended_at IS NULL
                RETURNING 1
            )
            SELECT
                EXISTS (SELECT 1 FROM ended) AS ended,
                EXISTS (
                    SELECT 1 FROM meeting_assignments
                    WHERE meeting_id = $1
                      AND region = $2
                      AND meeting_controller_id = $3
                      AND assignment_id = $4
                ) AS own_row
            "#,
        )
        .bind(meeting_id)
        .bind(region)
        .bind(controller_id)
        .bind(assignment_id.as_uuid())
        .fetch_one(pool)
        .await;

        // Record DB query metrics (ADR-0011)
        let (status, result) = match query_result {
            Ok(r) => ("success", Ok(r)),
            Err(e) => ("error", Err(e)),
        };
        metrics::record_db_query("end_assignment", status, start.elapsed());

        Ok(match result? {
            (true, _) => EndAssignmentOutcome::Ended,
            (false, true) => EndAssignmentOutcome::AlreadyEnded,
            (false, false) => EndAssignmentOutcome::Fenced,
        })
    }

    /// Backstop: end assignments MC never reported ended.
    ///
    /// Soft-deletes active assignments whose MC is NO LONGER ALIVE
    /// (`mc_is_live!`: heartbeat older than `mc_staleness`) and whose
    /// `assigned_at` is older than `inactivity_hours`. This handles meetings
    /// whose MC died before it could send `NotifyMeetingEnded`. It is NOT an
    /// inactivity timer (GC has no participant data); MC ends empty and
    /// never-joined meetings itself (ADR-0010 §3). A row whose MC still
    /// heartbeats is never touched, whatever that MC's `health_status`
    /// (draining, degraded, re-registered): it still holds the meeting, and
    /// ending the row would hand the next joiner a second MC.
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `inactivity_hours` - Minimum row age before it may be ended
    /// * `mc_staleness` - MC liveness threshold (`MC_STALENESS_THRESHOLD_SECONDS`)
    /// * `batch_size` - Optional batch size limit (defaults to 1000)
    ///
    /// # Returns
    ///
    /// Number of assignments ended.
    #[instrument(skip_all, fields(inactivity_hours = inactivity_hours))]
    pub async fn end_stale_assignments(
        pool: &PgPool,
        inactivity_hours: i32,
        mc_staleness: Duration,
        batch_size: Option<i64>,
    ) -> Result<u64, GcError> {
        let start = Instant::now();
        let limit = batch_size.unwrap_or(DEFAULT_CLEANUP_BATCH_SIZE);

        // Mark as ended any assignment that:
        // 1. Is still active (ended_at IS NULL)
        // 2. Was assigned more than `inactivity_hours` ago
        // 3. Has an MC that is no longer alive (heartbeat stale)
        // LIMIT prevents long-running transactions in pathological cases.
        let sql = concat!(
            r#"
            UPDATE meeting_assignments ma
            SET ended_at = NOW()
            WHERE (ma.meeting_id, ma.region) IN (
                SELECT inner_ma.meeting_id, inner_ma.region
                FROM meeting_assignments inner_ma
                WHERE inner_ma.ended_at IS NULL
                  AND inner_ma.assigned_at < NOW() - ($1 || ' hours')::INTERVAL
                  AND NOT EXISTS (
                      SELECT 1 FROM meeting_controllers mc
                      WHERE mc.controller_id = inner_ma.meeting_controller_id
                        AND "#,
            mc_is_live!("mc", "$3"),
            r#"
                  )
                LIMIT $2
            )
              -- ALL THREE predicates re-asserted on the OUTER row, not only in
              -- the subquery. Under READ COMMITTED the subquery's snapshot can
              -- list a row that a concurrent `atomic_assign` then revives or
              -- re-points before this statement locks it; such a row has
              -- `assigned_at = NOW()`, so the age re-check alone stops it
              -- being ended, even if its new MC has itself gone stale. The
              -- liveness re-check also stops a row whose MC resumed
              -- heartbeating in between. Keyed on (meeting_id, region) so a
              -- meeting's rows in OTHER regions are never touched.
              AND ma.ended_at IS NULL
              AND ma.assigned_at < NOW() - ($1 || ' hours')::INTERVAL
              AND NOT EXISTS (
                  SELECT 1 FROM meeting_controllers mc
                  WHERE mc.controller_id = ma.meeting_controller_id
                    AND "#,
            mc_is_live!("mc", "$3"),
            r#"
              )
            "#,
        );
        let query_result = sqlx::query(sql)
            .bind(inactivity_hours.to_string())
            .bind(limit)
            .bind(staleness_param(mc_staleness))
            .execute(pool)
            .await;

        // Record DB query metrics (ADR-0011)
        let (status, result) = match query_result {
            Ok(r) => ("success", Ok(r)),
            Err(e) => ("error", Err(e)),
        };
        metrics::record_db_query("end_stale_assignments", status, start.elapsed());

        let count = result?.rows_affected();

        // DEBUG, not INFO: the caller (`assignment_cleanup::run_cleanup`) is
        // the single WARN/INFO record; this keeps only `batch_size`.
        if count > 0 {
            tracing::debug!(
                target: "gc.repository.assignments",
                inactivity_hours = inactivity_hours,
                count = count,
                batch_size = limit,
                "Ended stale assignments"
            );
        }

        Ok(count)
    }

    /// Clean up old ended assignments.
    ///
    /// Deletes assignments that ended more than `retention_days` ago.
    /// Run periodically as a background job.
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `retention_days` - Days of retention before hard-deleting
    /// * `batch_size` - Optional batch size limit (defaults to 1000)
    #[instrument(skip_all, fields(retention_days = retention_days))]
    pub async fn cleanup_old_assignments(
        pool: &PgPool,
        retention_days: i32,
        batch_size: Option<i64>,
    ) -> Result<u64, GcError> {
        let start = Instant::now();
        let limit = batch_size.unwrap_or(DEFAULT_CLEANUP_BATCH_SIZE);

        // LIMIT via subquery prevents long-running transactions in pathological cases.
        let query_result = sqlx::query(
            r#"
            DELETE FROM meeting_assignments
            WHERE (meeting_id, region) IN (
                SELECT meeting_id, region FROM meeting_assignments
                WHERE ended_at < NOW() - ($1 || ' days')::INTERVAL
                LIMIT $2
            )
              -- Re-asserted on the OUTER row: a row revived by a concurrent
              -- `atomic_assign` after the subquery's snapshot is live again and
              -- must not be deleted. Keyed on (meeting_id, region) so a meeting's
              -- live row in another region is never deleted with an old one.
              AND ended_at < NOW() - ($1 || ' days')::INTERVAL
            "#,
        )
        .bind(retention_days.to_string())
        .bind(limit)
        .execute(pool)
        .await;

        // Record DB query metrics (ADR-0011)
        let (status, result) = match query_result {
            Ok(r) => ("success", Ok(r)),
            Err(e) => ("error", Err(e)),
        };
        metrics::record_db_query("cleanup_old_assignments", status, start.elapsed());

        let count = result?.rows_affected();

        // DEBUG, not INFO: the caller (`assignment_cleanup::run_cleanup`) is
        // the single WARN/INFO record; this keeps only `batch_size`.
        if count > 0 {
            tracing::debug!(
                target: "gc.repository.assignments",
                retention_days = retention_days,
                count = count,
                batch_size = limit,
                "Cleaned up old assignments"
            );
        }

        Ok(count)
    }
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
    #[expect(
        dead_code,
        reason = "decoded by query_as from the RETURNING row; only Some/None is read"
    )]
    meeting_controller_id: String,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

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
