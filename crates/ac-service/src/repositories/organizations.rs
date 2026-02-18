//! Organization repository module for database operations.
//!
//! Provides database access for organization lookup, used by subdomain-based
//! organization extraction per ADR-0020.

use crate::errors::AcError;
use crate::observability::metrics::record_db_query;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use std::time::Instant;
use uuid::Uuid;

/// Organization model (maps to organizations table)
#[derive(Debug, Clone, sqlx::FromRow)]
#[allow(dead_code)] // Library type - fields read in future phases
pub struct Organization {
    pub org_id: Uuid,
    pub subdomain: String,
    pub display_name: String,
    pub plan_tier: String,
    pub max_concurrent_meetings: i32,
    pub max_participants_per_meeting: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub is_active: bool,
}

/// Get organization by subdomain.
///
/// Used by subdomain extraction middleware to look up org_id from Host header.
pub async fn get_by_subdomain(
    pool: &PgPool,
    subdomain: &str,
) -> Result<Option<Organization>, AcError> {
    let start = Instant::now();
    let result = sqlx::query_as::<_, Organization>(
        r#"
        SELECT
            org_id, subdomain, display_name, plan_tier,
            max_concurrent_meetings, max_participants_per_meeting,
            created_at, updated_at, is_active
        FROM organizations
        WHERE subdomain = $1 AND is_active = true
        "#,
    )
    .bind(subdomain)
    .fetch_optional(pool)
    .await;
    let status = if result.is_ok() { "success" } else { "error" };
    record_db_query("select", "organizations", status, start.elapsed());
    let org = result.map_err(|e| {
        AcError::Database(format!("Failed to fetch organization by subdomain: {}", e))
    })?;

    Ok(org)
}

/// Get organization by org_id.
#[allow(dead_code)] // Library function - will be used in future phases
pub async fn get_by_id(pool: &PgPool, org_id: Uuid) -> Result<Option<Organization>, AcError> {
    let start = Instant::now();
    let result = sqlx::query_as::<_, Organization>(
        r#"
        SELECT
            org_id, subdomain, display_name, plan_tier,
            max_concurrent_meetings, max_participants_per_meeting,
            created_at, updated_at, is_active
        FROM organizations
        WHERE org_id = $1
        "#,
    )
    .bind(org_id)
    .fetch_optional(pool)
    .await;
    let status = if result.is_ok() { "success" } else { "error" };
    record_db_query("select", "organizations", status, start.elapsed());
    let org = result
        .map_err(|e| AcError::Database(format!("Failed to fetch organization by id: {}", e)))?;

    Ok(org)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_get_by_subdomain(pool: PgPool) -> Result<(), AcError> {
        // Create an organization
        let org_id: (Uuid,) = sqlx::query_as(
            r#"
            INSERT INTO organizations (subdomain, display_name)
            VALUES ('acme', 'Acme Corp')
            RETURNING org_id
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("Should create organization");

        // Get by subdomain
        let org = get_by_subdomain(&pool, "acme").await?;
        assert!(org.is_some());
        let org = org.unwrap();
        assert_eq!(org.org_id, org_id.0);
        assert_eq!(org.subdomain, "acme");
        assert_eq!(org.display_name, "Acme Corp");
        assert!(org.is_active);

        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_get_by_subdomain_not_found(pool: PgPool) -> Result<(), AcError> {
        let org = get_by_subdomain(&pool, "nonexistent").await?;
        assert!(org.is_none());
        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_get_by_subdomain_inactive(pool: PgPool) -> Result<(), AcError> {
        // Create an inactive organization
        sqlx::query(
            r#"
            INSERT INTO organizations (subdomain, display_name, is_active)
            VALUES ('inactive', 'Inactive Org', false)
            "#,
        )
        .execute(&pool)
        .await
        .expect("Should create organization");

        // Should not find inactive org
        let org = get_by_subdomain(&pool, "inactive").await?;
        assert!(org.is_none());

        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_get_by_id(pool: PgPool) -> Result<(), AcError> {
        // Create an organization
        let org_id: (Uuid,) = sqlx::query_as(
            r#"
            INSERT INTO organizations (subdomain, display_name)
            VALUES ('test-id', 'Test ID')
            RETURNING org_id
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("Should create organization");

        // Get by ID
        let org = get_by_id(&pool, org_id.0).await?;
        assert!(org.is_some());
        let org = org.unwrap();
        assert_eq!(org.subdomain, "test-id");

        // Non-existent ID
        let org = get_by_id(&pool, Uuid::new_v4()).await?;
        assert!(org.is_none());

        Ok(())
    }
}
