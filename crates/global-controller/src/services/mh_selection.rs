//! Media Handler Selection Service.
//!
//! Provides MH selection logic for meeting assignments per ADR-0010 Section 4a.
//! Uses weighted random selection based on load ratio.
//!
//! # Security
//!
//! - Uses CSPRNG for weighted random selection
//! - All database operations use parameterized queries
//! - Error messages are generic to prevent information leakage

// Allow dead code during incremental development - will be used by mc_assignment
// service when wired into handlers.
#![allow(dead_code)]

use crate::errors::GcError;
use crate::repositories::{MediaHandlersRepository, MhCandidate};
use ring::rand::{SecureRandom, SystemRandom};
use sqlx::PgPool;
use tracing::instrument;

/// Result of MH selection for a meeting.
#[derive(Debug, Clone)]
pub struct MhSelection {
    /// Selected primary MH.
    pub primary: MhAssignmentInfo,
    /// Optional backup MH (if available).
    pub backup: Option<MhAssignmentInfo>,
}

/// MH assignment information.
#[derive(Debug, Clone)]
pub struct MhAssignmentInfo {
    /// Handler ID.
    pub mh_id: String,
    /// WebTransport endpoint for client connections.
    pub webtransport_endpoint: String,
}

/// Service for MH selection operations.
pub struct MhSelectionService;

impl MhSelectionService {
    /// Select MHs for a meeting in a region.
    ///
    /// Selects a primary MH and optionally a backup MH using weighted random
    /// selection based on load ratio.
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `region` - Region to select MHs from
    ///
    /// # Returns
    ///
    /// Returns `MhSelection` with primary and optional backup MH.
    ///
    /// # Errors
    ///
    /// - `GcError::ServiceUnavailable` - No healthy MHs available
    /// - `GcError::Database` - Database operation failed
    #[instrument(skip(pool), fields(region = %region))]
    pub async fn select_mhs_for_meeting(
        pool: &PgPool,
        region: &str,
    ) -> Result<MhSelection, GcError> {
        // Get candidate MHs
        let candidates = MediaHandlersRepository::get_candidate_mhs(pool, region).await?;

        if candidates.is_empty() {
            tracing::warn!(
                target: "gc.service.mh_selection",
                region = %region,
                "No healthy MHs available for selection"
            );
            return Err(GcError::ServiceUnavailable(
                "No media handlers available in this region".to_string(),
            ));
        }

        tracing::debug!(
            target: "gc.service.mh_selection",
            region = %region,
            candidate_count = candidates.len(),
            "Found candidate MHs for selection"
        );

        // Select primary MH using weighted random
        let primary = weighted_random_select(&candidates).ok_or_else(|| {
            GcError::ServiceUnavailable("No media handlers available in this region".to_string())
        })?;

        let primary_info = MhAssignmentInfo {
            mh_id: primary.handler_id.clone(),
            webtransport_endpoint: primary.webtransport_endpoint.clone(),
        };

        tracing::debug!(
            target: "gc.service.mh_selection",
            primary_mh = %primary.handler_id,
            load_ratio = primary.load_ratio,
            "Selected primary MH"
        );

        // Try to select backup MH (different from primary)
        let backup = if candidates.len() > 1 {
            let remaining: Vec<_> = candidates
                .iter()
                .filter(|c| c.handler_id != primary.handler_id)
                .collect();

            if !remaining.is_empty() {
                // Convert to owned for weighted_random_select
                let remaining_owned: Vec<MhCandidate> = remaining.into_iter().cloned().collect();
                weighted_random_select(&remaining_owned).map(|backup| {
                    tracing::debug!(
                        target: "gc.service.mh_selection",
                        backup_mh = %backup.handler_id,
                        load_ratio = backup.load_ratio,
                        "Selected backup MH"
                    );
                    MhAssignmentInfo {
                        mh_id: backup.handler_id.clone(),
                        webtransport_endpoint: backup.webtransport_endpoint.clone(),
                    }
                })
            } else {
                None
            }
        } else {
            None
        };

        Ok(MhSelection {
            primary: primary_info,
            backup,
        })
    }
}

/// Select an MH from candidates using weighted random selection.
///
/// Weight is inversely proportional to load ratio:
/// - 0% loaded = weight 1.0
/// - 90% loaded = weight 0.1
///
/// This prevents thundering herd to a single MH while preferring less-loaded instances.
fn weighted_random_select(candidates: &[MhCandidate]) -> Option<&MhCandidate> {
    if candidates.is_empty() {
        return None;
    }

    if candidates.len() == 1 {
        return candidates.first();
    }

    // Calculate weights: weight = 1.0 - load_ratio (capped at 0.99 to ensure minimum weight)
    let weights: Vec<f64> = candidates
        .iter()
        .map(|mh| 1.0 - mh.load_ratio.min(0.99))
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
            target: "gc.service.mh_selection",
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_weighted_random_select_empty() {
        let candidates: Vec<MhCandidate> = vec![];
        assert!(weighted_random_select(&candidates).is_none());
    }

    #[test]
    fn test_weighted_random_select_single() {
        let candidates = vec![MhCandidate {
            handler_id: "mh-1".to_string(),
            webtransport_endpoint: "https://mh1:443".to_string(),
            grpc_endpoint: "https://mh1:50051".to_string(),
            load_ratio: 0.5,
        }];

        let selected = weighted_random_select(&candidates);
        assert!(selected.is_some());
        assert_eq!(selected.unwrap().handler_id, "mh-1");
    }

    #[test]
    fn test_weighted_random_select_multiple_returns_valid() {
        let candidates = vec![
            MhCandidate {
                handler_id: "mh-1".to_string(),
                webtransport_endpoint: "https://mh1:443".to_string(),
                grpc_endpoint: "https://mh1:50051".to_string(),
                load_ratio: 0.1,
            },
            MhCandidate {
                handler_id: "mh-2".to_string(),
                webtransport_endpoint: "https://mh2:443".to_string(),
                grpc_endpoint: "https://mh2:50051".to_string(),
                load_ratio: 0.9,
            },
        ];

        // Run multiple times to verify it always returns a valid candidate
        for _ in 0..100 {
            let selected = weighted_random_select(&candidates);
            assert!(selected.is_some());
            let mh_id = &selected.unwrap().handler_id;
            assert!(mh_id == "mh-1" || mh_id == "mh-2");
        }
    }

    #[test]
    fn test_weighted_random_select_prefers_lower_load() {
        let candidates = vec![
            MhCandidate {
                handler_id: "mh-light".to_string(),
                webtransport_endpoint: "https://mh1:443".to_string(),
                grpc_endpoint: "https://mh1:50051".to_string(),
                load_ratio: 0.0, // Empty, weight = 1.0
            },
            MhCandidate {
                handler_id: "mh-heavy".to_string(),
                webtransport_endpoint: "https://mh2:443".to_string(),
                grpc_endpoint: "https://mh2:50051".to_string(),
                load_ratio: 0.99, // Almost full, weight = 0.01
            },
        ];

        // Run many times and count selections
        let mut light_count = 0;
        let mut heavy_count = 0;

        for _ in 0..1000 {
            let selected = weighted_random_select(&candidates);
            assert!(selected.is_some());
            match selected.unwrap().handler_id.as_str() {
                "mh-light" => light_count += 1,
                "mh-heavy" => heavy_count += 1,
                _ => unreachable!(),
            }
        }

        // Light should be selected much more often
        assert!(
            light_count > heavy_count * 10,
            "Expected light ({}) to be selected much more than heavy ({})",
            light_count,
            heavy_count
        );
    }

    #[test]
    fn test_mh_selection_fields() {
        let selection = MhSelection {
            primary: MhAssignmentInfo {
                mh_id: "mh-primary".to_string(),
                webtransport_endpoint: "https://primary:443".to_string(),
            },
            backup: Some(MhAssignmentInfo {
                mh_id: "mh-backup".to_string(),
                webtransport_endpoint: "https://backup:443".to_string(),
            }),
        };

        assert_eq!(selection.primary.mh_id, "mh-primary");
        assert!(selection.backup.is_some());
        assert_eq!(selection.backup.as_ref().unwrap().mh_id, "mh-backup");
    }

    #[test]
    fn test_mh_assignment_info_fields() {
        let info = MhAssignmentInfo {
            mh_id: "mh-test".to_string(),
            webtransport_endpoint: "https://test:443".to_string(),
        };

        assert_eq!(info.mh_id, "mh-test");
        assert_eq!(info.webtransport_endpoint, "https://test:443");
    }
}
