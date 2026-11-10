//! Media Handler Selection Service.
//!
//! ONE selection function for initial allocation AND replacement (ADR-0039 §3,
//! user ruling R-23): [`MhSelectionService::select`]`(region, count, exclude)`.
//! Initial assignment calls it with `count = GC_MH_INITIAL_HANDLERS_PER_MEETING`
//! and an empty exclude list; `AcquireMediaHandler` with `count = 1` and the
//! request's exclude list. The pick is weighted-random by load ratio through
//! the shared kernel (`services::weighted_pick`).
//!
//! `select` records NO metrics: each caller owns its series
//! (`gc_mh_selection_*` for new assignments, `gc_mh_replacement_*` for
//! replacements), so neither counts the other's work.
//!
//! # Security
//!
//! - Uses CSPRNG for weighted random selection (injectable for tests only)
//! - All database operations use parameterized queries
//! - Error messages are generic to prevent information leakage

use crate::errors::GcError;
use crate::repositories::{MediaHandlersRepository, MhCandidate};
use crate::services::weighted_pick::{weighted_random_select, SelectionRng};
use proto_gen::dark_tower::internal::v1::MhAssignment;
use sqlx::PgPool;
use std::time::Duration;
use tracing::instrument;

/// Result of MH selection for a NEW meeting assignment.
///
/// Contains one or more MH peers selected by load/AZ.
/// All handlers are active/active -- there is no primary/backup distinction.
#[derive(Debug, Clone)]
pub struct MhSelection {
    /// Selected MH handlers (active/active peers).
    /// Non-empty; at least one MH is always selected.
    pub handlers: Vec<MhAssignmentInfo>,
}

/// MH assignment information.
#[derive(Debug, Clone)]
pub struct MhAssignmentInfo {
    /// Handler ID.
    pub mh_id: String,
    /// WebTransport endpoint for client connections.
    pub webtransport_endpoint: String,
    /// gRPC endpoint for MC→MH communication.
    pub grpc_endpoint: String,
}

impl From<MhAssignmentInfo> for MhAssignment {
    fn from(h: MhAssignmentInfo) -> Self {
        Self {
            mh_id: h.mh_id,
            webtransport_endpoint: h.webtransport_endpoint,
            grpc_endpoint: h.grpc_endpoint,
        }
    }
}

impl From<&MhAssignmentInfo> for MhAssignment {
    fn from(h: &MhAssignmentInfo) -> Self {
        h.clone().into()
    }
}

/// The ONE MH freshness rule in Rust: fresh iff the last load report is
/// younger than the staleness bound (the SQL's `last_heartbeat_at > NOW() -
/// bound`, see `MediaHandlersRepository::get_candidate_mhs`). Used by
/// `mh_replacement::classify` for `gc_view`, and by the eligibility oracle in
/// `tests/mh_replacement_tests.rs` that proves the SQL predicate.
#[must_use]
pub fn is_fresh(heartbeat_age: Duration, staleness_bound: Duration) -> bool {
    heartbeat_age < staleness_bound
}

/// Service for MH selection operations.
pub struct MhSelectionService;

impl MhSelectionService {
    /// Select up to `count` distinct MHs in `region`, never any id in
    /// `exclude`.
    ///
    /// Each pick is weighted-random by load ratio among the candidates not yet
    /// picked. `exclude` is applied inside the candidate query, before its
    /// least-loaded cut. Unknown ids in `exclude` are ignored.
    ///
    /// Deliberately takes nothing about WHY the selection happens (no
    /// trigger, no replaced handler, no GC health view): selection is
    /// identical with and without replacement evidence (ADR-0039 §9).
    ///
    /// # Returns
    ///
    /// The picked handlers, possibly EMPTY (no eligible candidate). An empty
    /// result is not an error: each caller maps it to its own answer.
    ///
    /// # Errors
    ///
    /// `GcError::Database` (or `Internal`) on a query failure.
    #[instrument(skip_all, fields(region = %region, count = count, exclude_count = exclude.len()))]
    pub async fn select(
        pool: &PgPool,
        region: &str,
        staleness_bound: Duration,
        count: usize,
        exclude: &[String],
        rng: &dyn SelectionRng,
    ) -> Result<Vec<MhAssignmentInfo>, GcError> {
        let candidates =
            MediaHandlersRepository::get_candidate_mhs(pool, region, staleness_bound, exclude)
                .await?;

        tracing::debug!(
            target: "gc.service.mh_selection",
            region = %region,
            candidate_count = candidates.len(),
            "Found candidate MHs for selection"
        );

        Ok(pick(candidates, count, rng))
    }
}

/// Up to `count` weighted-random picks without replacement.
fn pick(
    mut remaining: Vec<MhCandidate>,
    count: usize,
    rng: &dyn SelectionRng,
) -> Vec<MhAssignmentInfo> {
    let mut handlers = Vec::with_capacity(count);
    while handlers.len() < count {
        let Some(picked) = weighted_random_select(&remaining, |c| c.load_ratio, rng).cloned()
        else {
            break;
        };
        tracing::debug!(
            target: "gc.service.mh_selection",
            mh_id = %picked.handler_id,
            load_ratio = picked.load_ratio,
            mh_index = handlers.len(),
            "Selected MH"
        );
        remaining.retain(|c| c.handler_id != picked.handler_id);
        handlers.push(MhAssignmentInfo {
            mh_id: picked.handler_id,
            webtransport_endpoint: picked.webtransport_endpoint,
            grpc_endpoint: picked.grpc_endpoint,
        });
    }
    handlers
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    use crate::services::weighted_pick::SystemSelectionRng;

    fn cand(id: &str, load: f64) -> MhCandidate {
        MhCandidate {
            handler_id: id.to_string(),
            webtransport_endpoint: format!("https://{id}:443"),
            grpc_endpoint: format!("grpc://{id}:50051"),
            load_ratio: load,
        }
    }

    #[test]
    fn pick_returns_distinct_handlers_up_to_count() {
        let picked = pick(
            vec![cand("a", 0.1), cand("b", 0.2), cand("c", 0.3)],
            2,
            &SystemSelectionRng,
        );
        assert_eq!(picked.len(), 2);
        assert_ne!(picked[0].mh_id, picked[1].mh_id);
    }

    #[test]
    fn pick_returns_all_when_fewer_than_count_and_empty_for_none() {
        assert_eq!(pick(vec![cand("a", 0.1)], 3, &SystemSelectionRng).len(), 1);
        assert!(pick(vec![], 1, &SystemSelectionRng).is_empty());
    }

    #[test]
    fn is_fresh_boundary() {
        let bound = Duration::from_millis(3750);
        assert!(is_fresh(Duration::ZERO, bound));
        assert!(is_fresh(Duration::from_millis(3749), bound));
        assert!(!is_fresh(Duration::from_millis(3750), bound));
    }

    #[test]
    fn assignment_info_maps_to_proto() {
        let info = MhAssignmentInfo {
            mh_id: "mh-1".to_string(),
            webtransport_endpoint: "https://mh-1:443".to_string(),
            grpc_endpoint: "grpc://mh-1:50051".to_string(),
        };
        let by_ref = MhAssignment::from(&info);
        let by_value = MhAssignment::from(info);
        assert_eq!(by_ref, by_value);
        assert_eq!(by_value.mh_id, "mh-1");
        assert_eq!(by_value.webtransport_endpoint, "https://mh-1:443");
        assert_eq!(by_value.grpc_endpoint, "grpc://mh-1:50051");
    }

    #[test]
    fn test_mh_selection_fields() {
        let selection = MhSelection {
            handlers: vec![
                MhAssignmentInfo {
                    mh_id: "mh-1".to_string(),
                    webtransport_endpoint: "https://mh1:443".to_string(),
                    grpc_endpoint: "https://mh1:50051".to_string(),
                },
                MhAssignmentInfo {
                    mh_id: "mh-2".to_string(),
                    webtransport_endpoint: "https://mh2:443".to_string(),
                    grpc_endpoint: "https://mh2:50051".to_string(),
                },
            ],
        };

        assert_eq!(selection.handlers.len(), 2);
        assert_eq!(selection.handlers[0].mh_id, "mh-1");
        assert_eq!(selection.handlers[1].mh_id, "mh-2");
    }

    #[test]
    fn test_mh_assignment_info_fields() {
        let info = MhAssignmentInfo {
            mh_id: "mh-test".to_string(),
            webtransport_endpoint: "https://test:443".to_string(),
            grpc_endpoint: "https://test:50051".to_string(),
        };

        assert_eq!(info.mh_id, "mh-test");
        assert_eq!(info.webtransport_endpoint, "https://test:443");
        assert_eq!(info.grpc_endpoint, "https://test:50051");
    }
}
