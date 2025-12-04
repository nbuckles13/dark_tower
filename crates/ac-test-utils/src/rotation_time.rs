//! Time manipulation utilities for key rotation tests
//!
//! Provides database-level time manipulation to test rate limiting behavior
//! without requiring production code changes or slow wait times.

use chrono::{DateTime, Duration, Utc};
use sqlx::PgPool;

/// Set the last key rotation timestamp
///
/// This directly modifies the `created_at` timestamp of the most recent signing key,
/// allowing tests to simulate the passage of time for rate limiting scenarios.
///
/// # Arguments
/// * `pool` - Database connection pool
/// * `timestamp` - Timestamp to set as the last rotation time
///
/// # Returns
/// * `Ok(())` - If update successful
/// * `Err(sqlx::Error)` - If database query fails
///
/// # Example
/// ```rust,ignore
/// // Set last rotation to 8 days ago
/// let eight_days_ago = Utc::now() - Duration::days(8);
/// set_last_rotation(&pool, eight_days_ago).await?;
/// ```
pub async fn set_last_rotation(pool: &PgPool, timestamp: DateTime<Utc>) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE signing_keys
        SET created_at = $1
        WHERE key_id = (
            SELECT key_id FROM signing_keys
            ORDER BY created_at DESC LIMIT 1
        )
        "#,
    )
    .bind(timestamp)
    .execute(pool)
    .await?;

    Ok(())
}

/// Set rotation eligible (7 days ago)
///
/// Makes the last key rotation eligible for normal rotation according to the
/// 6-day rate limit. This is 7 days to ensure we're well past the limit.
///
/// # Arguments
/// * `pool` - Database connection pool
///
/// # Example
/// ```rust,ignore
/// set_eligible(&pool).await?;
/// // Now normal rotation should succeed
/// ```
pub async fn set_eligible(pool: &PgPool) -> Result<(), sqlx::Error> {
    set_last_rotation(pool, Utc::now() - Duration::days(7)).await
}

/// Set force rotation eligible (2 hours ago)
///
/// Makes the last key rotation eligible for FORCE rotation (1-hour rate limit)
/// but NOT eligible for normal rotation (6-day rate limit).
///
/// This tests the scenario where:
/// - Normal rotation would be blocked (< 6 days)
/// - Force rotation is allowed (> 1 hour)
///
/// # Arguments
/// * `pool` - Database connection pool
///
/// # Example
/// ```rust,ignore
/// set_force_eligible(&pool).await?;
/// // Normal rotation should fail (429)
/// // Force rotation should succeed (200)
/// ```
pub async fn set_force_eligible(pool: &PgPool) -> Result<(), sqlx::Error> {
    set_last_rotation(pool, Utc::now() - Duration::hours(2)).await
}

/// Set rotation rate limited (30 minutes ago)
///
/// Makes the last key rotation too recent for both normal AND force rotation.
///
/// This tests the scenario where:
/// - Normal rotation is blocked (< 6 days)
/// - Force rotation is also blocked (< 1 hour)
///
/// # Arguments
/// * `pool` - Database connection pool
///
/// # Example
/// ```rust,ignore
/// set_rate_limited(&pool).await?;
/// // Both normal and force rotation should fail (429)
/// ```
pub async fn set_rate_limited(pool: &PgPool) -> Result<(), sqlx::Error> {
    set_last_rotation(pool, Utc::now() - Duration::minutes(30)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_set_last_rotation_updates_timestamp(pool: PgPool) -> Result<(), anyhow::Error> {
        // Initialize a signing key first
        use crate::crypto_fixtures::test_master_key;
        let master_key = test_master_key();
        ac_service::services::key_management_service::initialize_signing_key(
            &pool,
            &master_key,
            "test-cluster",
        )
        .await?;

        // Set a specific timestamp
        let target_time = Utc::now() - Duration::days(10);
        set_last_rotation(&pool, target_time).await?;

        // Verify the timestamp was updated
        let (created_at,): (DateTime<Utc>,) = sqlx::query_as(
            r#"
            SELECT created_at FROM signing_keys
            ORDER BY created_at DESC LIMIT 1
            "#,
        )
        .fetch_one(&pool)
        .await?;

        // Allow small difference due to database precision
        let diff = (created_at - target_time).num_milliseconds().abs();
        assert!(diff < 1000, "Timestamp should be within 1 second of target");

        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_set_eligible_sets_7_days_ago(pool: PgPool) -> Result<(), anyhow::Error> {
        // Initialize a signing key first
        use crate::crypto_fixtures::test_master_key;
        let master_key = test_master_key();
        ac_service::services::key_management_service::initialize_signing_key(
            &pool,
            &master_key,
            "test-cluster",
        )
        .await?;

        // Set eligible for rotation
        set_eligible(&pool).await?;

        // Verify it's approximately 7 days ago
        let (created_at,): (DateTime<Utc>,) = sqlx::query_as(
            r#"
            SELECT created_at FROM signing_keys
            ORDER BY created_at DESC LIMIT 1
            "#,
        )
        .fetch_one(&pool)
        .await?;

        let expected = Utc::now() - Duration::days(7);
        let diff = (created_at - expected).num_seconds().abs();
        assert!(diff < 5, "Should be approximately 7 days ago");

        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_set_force_eligible_sets_2_hours_ago(pool: PgPool) -> Result<(), anyhow::Error> {
        // Initialize a signing key first
        use crate::crypto_fixtures::test_master_key;
        let master_key = test_master_key();
        ac_service::services::key_management_service::initialize_signing_key(
            &pool,
            &master_key,
            "test-cluster",
        )
        .await?;

        // Set eligible for force rotation
        set_force_eligible(&pool).await?;

        // Verify it's approximately 2 hours ago
        let (created_at,): (DateTime<Utc>,) = sqlx::query_as(
            r#"
            SELECT created_at FROM signing_keys
            ORDER BY created_at DESC LIMIT 1
            "#,
        )
        .fetch_one(&pool)
        .await?;

        let expected = Utc::now() - Duration::hours(2);
        let diff = (created_at - expected).num_seconds().abs();
        assert!(diff < 5, "Should be approximately 2 hours ago");

        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_set_rate_limited_sets_30_minutes_ago(pool: PgPool) -> Result<(), anyhow::Error> {
        // Initialize a signing key first
        use crate::crypto_fixtures::test_master_key;
        let master_key = test_master_key();
        ac_service::services::key_management_service::initialize_signing_key(
            &pool,
            &master_key,
            "test-cluster",
        )
        .await?;

        // Set rate limited
        set_rate_limited(&pool).await?;

        // Verify it's approximately 30 minutes ago
        let (created_at,): (DateTime<Utc>,) = sqlx::query_as(
            r#"
            SELECT created_at FROM signing_keys
            ORDER BY created_at DESC LIMIT 1
            "#,
        )
        .fetch_one(&pool)
        .await?;

        let expected = Utc::now() - Duration::minutes(30);
        let diff = (created_at - expected).num_seconds().abs();
        assert!(diff < 5, "Should be approximately 30 minutes ago");

        Ok(())
    }
}
