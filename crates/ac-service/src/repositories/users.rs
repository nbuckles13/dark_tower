//! User repository module for database operations.
//!
//! Provides database access for user management including lookup, creation,
//! and role management per ADR-0020.

use crate::errors::AcError;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

/// User model (maps to users table)
#[derive(Debug, Clone, sqlx::FromRow)]
#[allow(dead_code)] // Library type - fields read in future phases
pub struct User {
    pub user_id: Uuid,
    pub org_id: Uuid,
    pub email: String,
    pub password_hash: String,
    pub display_name: String,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_login_at: Option<DateTime<Utc>>,
}

/// Get user by email within an organization.
///
/// Users are unique per org (org_id, email is unique constraint).
pub async fn get_by_email(
    pool: &PgPool,
    org_id: Uuid,
    email: &str,
) -> Result<Option<User>, AcError> {
    let user = sqlx::query_as::<_, User>(
        r#"
        SELECT
            user_id, org_id, email, password_hash, display_name,
            is_active, created_at, updated_at, last_login_at
        FROM users
        WHERE org_id = $1 AND email = $2
        "#,
    )
    .bind(org_id)
    .bind(email)
    .fetch_optional(pool)
    .await
    .map_err(|e| AcError::Database(format!("Failed to fetch user by email: {}", e)))?;

    Ok(user)
}

/// Get user by user_id.
#[allow(dead_code)] // Library function - will be used in future phases
pub async fn get_by_id(pool: &PgPool, user_id: Uuid) -> Result<Option<User>, AcError> {
    let user = sqlx::query_as::<_, User>(
        r#"
        SELECT
            user_id, org_id, email, password_hash, display_name,
            is_active, created_at, updated_at, last_login_at
        FROM users
        WHERE user_id = $1
        "#,
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| AcError::Database(format!("Failed to fetch user by id: {}", e)))?;

    Ok(user)
}

/// Create a new user in an organization.
///
/// Returns the created user record.
pub async fn create_user(
    pool: &PgPool,
    org_id: Uuid,
    email: &str,
    password_hash: &str,
    display_name: &str,
) -> Result<User, AcError> {
    let user = sqlx::query_as::<_, User>(
        r#"
        INSERT INTO users (org_id, email, password_hash, display_name)
        VALUES ($1, $2, $3, $4)
        RETURNING
            user_id, org_id, email, password_hash, display_name,
            is_active, created_at, updated_at, last_login_at
        "#,
    )
    .bind(org_id)
    .bind(email)
    .bind(password_hash)
    .bind(display_name)
    .fetch_one(pool)
    .await
    .map_err(|e| {
        // Check for unique constraint violation (duplicate email in org)
        if e.to_string().contains("users_org_email_unique") {
            AcError::Database("User with this email already exists in organization".to_string())
        } else {
            AcError::Database(format!("Failed to create user: {}", e))
        }
    })?;

    Ok(user)
}

/// Update the last_login_at timestamp for a user.
pub async fn update_last_login(pool: &PgPool, user_id: Uuid) -> Result<(), AcError> {
    sqlx::query(
        r#"
        UPDATE users
        SET last_login_at = NOW()
        WHERE user_id = $1
        "#,
    )
    .bind(user_id)
    .execute(pool)
    .await
    .map_err(|e| AcError::Database(format!("Failed to update last login: {}", e)))?;

    Ok(())
}

/// Get all roles for a user.
///
/// Returns a list of role strings (e.g., ["user", "admin"]).
pub async fn get_user_roles(pool: &PgPool, user_id: Uuid) -> Result<Vec<String>, AcError> {
    let roles: Vec<(String,)> = sqlx::query_as(
        r#"
        SELECT role
        FROM user_roles
        WHERE user_id = $1
        ORDER BY role
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(|e| AcError::Database(format!("Failed to fetch user roles: {}", e)))?;

    Ok(roles.into_iter().map(|(r,)| r).collect())
}

/// Add a role to a user.
///
/// Ignores duplicates (role already exists).
pub async fn add_user_role(pool: &PgPool, user_id: Uuid, role: &str) -> Result<(), AcError> {
    // Validate role value
    if !["user", "admin", "org_admin"].contains(&role) {
        return Err(AcError::Database(format!("Invalid role: {}", role)));
    }

    sqlx::query(
        r#"
        INSERT INTO user_roles (user_id, role)
        VALUES ($1, $2)
        ON CONFLICT (user_id, role) DO NOTHING
        "#,
    )
    .bind(user_id)
    .bind(role)
    .execute(pool)
    .await
    .map_err(|e| AcError::Database(format!("Failed to add user role: {}", e)))?;

    Ok(())
}

/// Remove a role from a user.
#[allow(dead_code)] // Library function - will be used in future phases
pub async fn remove_user_role(pool: &PgPool, user_id: Uuid, role: &str) -> Result<(), AcError> {
    sqlx::query(
        r#"
        DELETE FROM user_roles
        WHERE user_id = $1 AND role = $2
        "#,
    )
    .bind(user_id)
    .bind(role)
    .execute(pool)
    .await
    .map_err(|e| AcError::Database(format!("Failed to remove user role: {}", e)))?;

    Ok(())
}

/// Check if email exists in an organization.
///
/// Used for registration validation.
pub async fn email_exists_in_org(
    pool: &PgPool,
    org_id: Uuid,
    email: &str,
) -> Result<bool, AcError> {
    let exists: (bool,) = sqlx::query_as(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM users
            WHERE org_id = $1 AND email = $2
        )
        "#,
    )
    .bind(org_id)
    .bind(email)
    .fetch_one(pool)
    .await
    .map_err(|e| AcError::Database(format!("Failed to check email existence: {}", e)))?;

    Ok(exists.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_create_and_get_user(pool: PgPool) -> Result<(), AcError> {
        // First create an organization
        let org_id: (Uuid,) = sqlx::query_as(
            r#"
            INSERT INTO organizations (subdomain, display_name)
            VALUES ('test-org', 'Test Org')
            RETURNING org_id
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("Should create organization");

        // Create a user
        let user = create_user(
            &pool,
            org_id.0,
            "test@example.com",
            "$2b$12$LQv3c1yqBWVHxkd0LHAkCOYz6TtxMQJqhN8/LewY5GyYqExt7YD3a",
            "Test User",
        )
        .await?;

        assert_eq!(user.org_id, org_id.0);
        assert_eq!(user.email, "test@example.com");
        assert_eq!(user.display_name, "Test User");
        assert!(user.is_active);
        assert!(user.last_login_at.is_none());

        // Get by email
        let fetched = get_by_email(&pool, org_id.0, "test@example.com").await?;
        assert!(fetched.is_some());
        let fetched = fetched.unwrap();
        assert_eq!(fetched.user_id, user.user_id);

        // Get by id
        let fetched_by_id = get_by_id(&pool, user.user_id).await?;
        assert!(fetched_by_id.is_some());
        assert_eq!(fetched_by_id.unwrap().email, "test@example.com");

        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_duplicate_email_fails(pool: PgPool) -> Result<(), AcError> {
        // Create organization
        let org_id: (Uuid,) = sqlx::query_as(
            r#"
            INSERT INTO organizations (subdomain, display_name)
            VALUES ('test-dup', 'Test Dup')
            RETURNING org_id
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("Should create organization");

        // Create first user
        create_user(&pool, org_id.0, "dup@example.com", "hash1", "User 1").await?;

        // Try to create duplicate
        let result = create_user(&pool, org_id.0, "dup@example.com", "hash2", "User 2").await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, AcError::Database(msg) if msg.contains("already exists")));

        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_user_roles(pool: PgPool) -> Result<(), AcError> {
        // Create organization and user
        let org_id: (Uuid,) = sqlx::query_as(
            r#"
            INSERT INTO organizations (subdomain, display_name)
            VALUES ('test-roles', 'Test Roles')
            RETURNING org_id
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("Should create organization");

        let user = create_user(&pool, org_id.0, "roles@example.com", "hash", "Role User").await?;

        // Initially no roles
        let roles = get_user_roles(&pool, user.user_id).await?;
        assert!(roles.is_empty());

        // Add user role
        add_user_role(&pool, user.user_id, "user").await?;
        let roles = get_user_roles(&pool, user.user_id).await?;
        assert_eq!(roles, vec!["user"]);

        // Add admin role
        add_user_role(&pool, user.user_id, "admin").await?;
        let roles = get_user_roles(&pool, user.user_id).await?;
        assert_eq!(roles, vec!["admin", "user"]); // Ordered alphabetically

        // Adding same role again should be idempotent
        add_user_role(&pool, user.user_id, "user").await?;
        let roles = get_user_roles(&pool, user.user_id).await?;
        assert_eq!(roles.len(), 2);

        // Remove a role
        remove_user_role(&pool, user.user_id, "admin").await?;
        let roles = get_user_roles(&pool, user.user_id).await?;
        assert_eq!(roles, vec!["user"]);

        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_invalid_role_rejected(pool: PgPool) -> Result<(), AcError> {
        // Create organization and user
        let org_id: (Uuid,) = sqlx::query_as(
            r#"
            INSERT INTO organizations (subdomain, display_name)
            VALUES ('test-invalid', 'Test Invalid')
            RETURNING org_id
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("Should create organization");

        let user = create_user(
            &pool,
            org_id.0,
            "invalid@example.com",
            "hash",
            "Invalid Role User",
        )
        .await?;

        // Try to add invalid role
        let result = add_user_role(&pool, user.user_id, "superadmin").await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, AcError::Database(msg) if msg.contains("Invalid role")));

        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_update_last_login(pool: PgPool) -> Result<(), AcError> {
        // Create organization and user
        let org_id: (Uuid,) = sqlx::query_as(
            r#"
            INSERT INTO organizations (subdomain, display_name)
            VALUES ('test-login', 'Test Login')
            RETURNING org_id
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("Should create organization");

        let user = create_user(&pool, org_id.0, "login@example.com", "hash", "Login User").await?;

        assert!(user.last_login_at.is_none());

        // Update last login
        update_last_login(&pool, user.user_id).await?;

        // Verify it was updated
        let updated = get_by_id(&pool, user.user_id).await?.unwrap();
        assert!(updated.last_login_at.is_some());

        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_email_exists_in_org(pool: PgPool) -> Result<(), AcError> {
        // Create organization
        let org_id: (Uuid,) = sqlx::query_as(
            r#"
            INSERT INTO organizations (subdomain, display_name)
            VALUES ('test-exists', 'Test Exists')
            RETURNING org_id
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("Should create organization");

        // Check non-existent email
        assert!(!email_exists_in_org(&pool, org_id.0, "new@example.com").await?);

        // Create user
        create_user(&pool, org_id.0, "exists@example.com", "hash", "Exists User").await?;

        // Check existing email
        assert!(email_exists_in_org(&pool, org_id.0, "exists@example.com").await?);

        // Check in different org - should not exist
        let other_org_id: (Uuid,) = sqlx::query_as(
            r#"
            INSERT INTO organizations (subdomain, display_name)
            VALUES ('other-org', 'Other Org')
            RETURNING org_id
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("Should create other organization");

        assert!(!email_exists_in_org(&pool, other_org_id.0, "exists@example.com").await?);

        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_get_nonexistent_user(pool: PgPool) -> Result<(), AcError> {
        let random_id = Uuid::new_v4();
        let random_org_id = Uuid::new_v4();

        let by_id = get_by_id(&pool, random_id).await?;
        assert!(by_id.is_none());

        let by_email = get_by_email(&pool, random_org_id, "nonexistent@example.com").await?;
        assert!(by_email.is_none());

        Ok(())
    }
}
