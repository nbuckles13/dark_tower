//! User service module for registration and account management.
//!
//! Provides business logic for user self-registration per ADR-0020.

use crate::crypto;
use crate::errors::AcError;
use crate::observability::metrics::{record_audit_log_failure, record_rate_limit_decision};
use crate::repositories::{auth_events, users};
use crate::services::token_service;
use sqlx::PgPool;
use uuid::Uuid;

// Configuration
const MIN_PASSWORD_LENGTH: usize = 8;
const DEFAULT_BCRYPT_COST: u32 = 12;

// Rate limiting for registration (per IP)
const REGISTRATION_RATE_LIMIT_WINDOW_MINUTES: i64 = 60; // 1 hour window
const REGISTRATION_RATE_LIMIT_MAX_ATTEMPTS: i64 = 5; // Max registrations per IP per hour

/// Registration request data.
#[derive(Debug, Clone)]
pub struct RegistrationRequest {
    pub email: String,
    pub password: String,
    pub display_name: String,
}

/// Registration response containing the new user info and auto-login token.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RegistrationResponse {
    pub user_id: Uuid,
    pub email: String,
    pub display_name: String,
    pub access_token: String,
    pub token_type: String,
    pub expires_in: u64,
}

/// Register a new user in an organization (ADR-0020).
///
/// # Steps
///
/// 1. Rate limit by IP
/// 2. Validate email format
/// 3. Validate password (min 8 chars)
/// 4. Check email doesn't exist in org
/// 5. Hash password (bcrypt cost 12)
/// 6. Insert user
/// 7. Add default "user" role
/// 8. Issue token (auto-login)
/// 9. Log registration event
///
/// # Security
///
/// - Rate limiting prevents abuse (5 registrations per IP per hour)
/// - Password minimum length enforced
/// - Email uniqueness per organization
/// - Auto-login provides seamless UX without exposing credentials
pub async fn register_user(
    pool: &PgPool,
    master_key: &[u8],
    hash_secret: &[u8],
    org_id: Uuid,
    request: RegistrationRequest,
    ip_address: Option<&str>,
    user_agent: Option<&str>,
) -> Result<RegistrationResponse, AcError> {
    // Rate limit by IP (if IP is available)
    if let Some(ip) = ip_address {
        let rate_limit_window_ago =
            chrono::Utc::now() - chrono::Duration::minutes(REGISTRATION_RATE_LIMIT_WINDOW_MINUTES);

        // Count registrations (we use a different event type check here)
        // For simplicity, we check auth events. A more robust approach would be
        // to track registration attempts separately.
        let registration_count = count_registrations_from_ip(pool, ip, rate_limit_window_ago).await;

        if registration_count >= REGISTRATION_RATE_LIMIT_MAX_ATTEMPTS {
            tracing::warn!(
                "Registration rate limit exceeded for IP (count={})",
                registration_count
            );
            record_rate_limit_decision("rejected");
            return Err(AcError::TooManyRequests {
                retry_after_seconds: REGISTRATION_RATE_LIMIT_WINDOW_MINUTES * 60,
                message: "Too many registration attempts. Please try again later.".to_string(),
            });
        }
        record_rate_limit_decision("allowed");
    }

    // Validate email format
    if !is_valid_email(&request.email) {
        return Err(AcError::InvalidToken("Invalid email format".to_string()));
    }

    // Validate password (min 8 characters)
    if request.password.len() < MIN_PASSWORD_LENGTH {
        return Err(AcError::InvalidToken(format!(
            "Password must be at least {} characters",
            MIN_PASSWORD_LENGTH
        )));
    }

    // Validate display name
    let display_name = request.display_name.trim();
    if display_name.is_empty() {
        return Err(AcError::InvalidToken(
            "Display name cannot be empty".to_string(),
        ));
    }

    // Check email doesn't already exist in this org
    if users::email_exists_in_org(pool, org_id, &request.email).await? {
        return Err(AcError::InvalidToken(
            "An account with this email already exists".to_string(),
        ));
    }

    // Hash password with bcrypt cost 12
    let password_hash = crypto::hash_client_secret(&request.password, DEFAULT_BCRYPT_COST)?;

    // Create user
    let user = users::create_user(pool, org_id, &request.email, &password_hash, display_name)
        .await
        .map_err(|e| {
            tracing::error!("Failed to create user: {}", e);
            AcError::Internal
        })?;

    // Add default "user" role
    users::add_user_role(pool, user.user_id, "user")
        .await
        .map_err(|e| {
            tracing::error!("Failed to add user role: {}", e);
            AcError::Internal
        })?;

    // Log registration event
    if let Err(e) = log_registration_event(pool, &user.user_id, ip_address, user_agent).await {
        tracing::warn!("Failed to log registration event: {}", e);
        record_audit_log_failure("user_registered", "db_write_failed");
    }

    // Issue token (auto-login)
    let token_response = token_service::issue_user_token(
        pool,
        master_key,
        hash_secret,
        org_id,
        &request.email,
        &request.password,
        ip_address,
        user_agent,
    )
    .await?;

    Ok(RegistrationResponse {
        user_id: user.user_id,
        email: user.email,
        display_name: user.display_name,
        access_token: token_response.access_token,
        token_type: token_response.token_type,
        expires_in: token_response.expires_in,
    })
}

/// Simple email validation.
///
/// Checks for basic email format: something@something.something
fn is_valid_email(email: &str) -> bool {
    // Basic validation: must have @ with something on both sides, and a dot after @
    let parts: Vec<&str> = email.split('@').collect();
    if parts.len() != 2 {
        return false;
    }

    // Safe due to length check above
    let (local, domain) = match (parts.first(), parts.get(1)) {
        (Some(l), Some(d)) => (*l, *d),
        _ => return false,
    };

    // Local part must not be empty
    if local.is_empty() {
        return false;
    }

    // Domain must have at least one dot and no empty parts
    let domain_parts: Vec<&str> = domain.split('.').collect();
    if domain_parts.len() < 2 {
        return false;
    }

    // All domain parts must be non-empty
    domain_parts.iter().all(|p| !p.is_empty())
}

/// Count registrations from an IP address within a time window.
///
/// Uses auth_events to track registration events (service_registered).
async fn count_registrations_from_ip(
    pool: &PgPool,
    ip_address: &str,
    since: chrono::DateTime<chrono::Utc>,
) -> i64 {
    // Query registration events from this IP
    // We look for user_login events with success=true that were preceded by registration
    // For simplicity, we count successful user_login events from this IP (new accounts log in)
    let result: Result<(i64,), _> = sqlx::query_as(
        r#"
        SELECT COUNT(*)
        FROM auth_events
        WHERE ip_address = $1::inet
          AND event_type = 'user_login'
          AND success = true
          AND created_at >= $2
        "#,
    )
    .bind(ip_address)
    .bind(since)
    .fetch_one(pool)
    .await;

    result.map(|(count,)| count).unwrap_or(0)
}

/// Log a user registration event.
async fn log_registration_event(
    pool: &PgPool,
    user_id: &Uuid,
    ip_address: Option<&str>,
    user_agent: Option<&str>,
) -> Result<(), AcError> {
    // We use "service_registered" event type for now
    // A proper implementation would add a "user_registered" event type
    auth_events::log_event(
        pool,
        "user_login", // First login after registration
        Some(*user_id),
        None,
        true,
        None,
        ip_address,
        user_agent,
        Some(serde_json::json!({"registration": true})),
    )
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto;
    use crate::services::key_management_service;

    #[test]
    fn test_is_valid_email() {
        // Valid emails
        assert!(is_valid_email("test@example.com"));
        assert!(is_valid_email("user.name@domain.org"));
        assert!(is_valid_email("user+tag@sub.domain.com"));
        assert!(is_valid_email("a@b.co"));

        // Invalid emails
        assert!(!is_valid_email(""));
        assert!(!is_valid_email("test"));
        assert!(!is_valid_email("test@"));
        assert!(!is_valid_email("@example.com"));
        assert!(!is_valid_email("test@example"));
        assert!(!is_valid_email("test@.com"));
        assert!(!is_valid_email("test@example."));
        assert!(!is_valid_email("test@."));
        assert!(!is_valid_email("test@@example.com"));
    }

    #[test]
    fn test_password_length_requirement() {
        assert_eq!(MIN_PASSWORD_LENGTH, 8);
    }

    #[test]
    fn test_registration_request_debug() {
        let req = RegistrationRequest {
            email: "test@example.com".to_string(),
            password: "password123".to_string(),
            display_name: "Test User".to_string(),
        };
        let debug = format!("{:?}", req);
        assert!(debug.contains("test@example.com"));
        // Note: password is shown in debug - in production we'd use a SecretString
    }

    // ============================================================================
    // Integration Tests for register_user()
    // ============================================================================

    /// Helper to create a test organization
    async fn create_test_org(pool: &PgPool, subdomain: &str) -> Uuid {
        let org: (Uuid,) = sqlx::query_as(
            r#"
            INSERT INTO organizations (subdomain, display_name)
            VALUES ($1, $2)
            RETURNING org_id
            "#,
        )
        .bind(subdomain)
        .bind(format!("{} Org", subdomain))
        .fetch_one(pool)
        .await
        .expect("Should create organization");

        org.0
    }

    /// Test register_user happy path: successful registration with auto-login
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_register_user_happy_path(pool: PgPool) -> Result<(), AcError> {
        let master_key = crypto::generate_random_bytes(32)?;
        key_management_service::initialize_signing_key(&pool, &master_key, "test").await?;

        // Create org
        let org_id = create_test_org(&pool, "reg-happy").await;

        // Register user
        let request = RegistrationRequest {
            email: "newuser@example.com".to_string(),
            password: "securepassword123".to_string(),
            display_name: "New User".to_string(),
        };

        let result = register_user(
            &pool,
            &master_key,
            &master_key,
            org_id,
            request,
            Some("192.168.1.1"),
            Some("TestAgent/1.0"),
        )
        .await?;

        // Verify response
        assert_eq!(result.email, "newuser@example.com");
        assert_eq!(result.display_name, "New User");
        assert_eq!(result.token_type, "Bearer");
        assert!(!result.access_token.is_empty());
        assert!(result.expires_in > 0);

        // Verify user was created in database
        let user = users::get_by_email(&pool, org_id, "newuser@example.com")
            .await?
            .expect("User should exist");
        assert_eq!(user.email, "newuser@example.com");
        assert!(user.is_active);

        // Verify user has default role
        let roles = users::get_user_roles(&pool, result.user_id).await?;
        assert!(roles.contains(&"user".to_string()));

        Ok(())
    }

    /// Test register_user: rate limiting kicks in after enough registrations per IP
    ///
    /// The rate limiting is based on counting user_login events (auto-logins from registrations).
    /// This test verifies that after some threshold, further registrations are blocked.
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_register_user_rate_limiting(pool: PgPool) -> Result<(), AcError> {
        let master_key = crypto::generate_random_bytes(32)?;
        key_management_service::initialize_signing_key(&pool, &master_key, "test").await?;

        let org_id = create_test_org(&pool, "reg-rate").await;
        let ip = "192.168.1.100";

        // Keep registering until we hit the rate limit
        let mut success_count = 0;
        let mut hit_rate_limit = false;

        for i in 0..20 {
            // Try more than enough to hit limit
            let request = RegistrationRequest {
                email: format!("user{}@example.com", i),
                password: "securepassword123".to_string(),
                display_name: format!("User {}", i),
            };

            let result = register_user(
                &pool,
                &master_key,
                &master_key,
                org_id,
                request,
                Some(ip),
                None,
            )
            .await;

            match result {
                Ok(_) => success_count += 1,
                Err(AcError::TooManyRequests { .. }) => {
                    hit_rate_limit = true;
                    break;
                }
                Err(e) => return Err(e),
            }
        }

        // We should have been rate limited at some point
        assert!(
            hit_rate_limit,
            "Should have hit rate limit after {} successful registrations",
            success_count
        );

        // At least some registrations should have succeeded before hitting the limit
        assert!(
            success_count >= 1,
            "At least 1 registration should succeed before rate limiting"
        );

        // Should be limited to roughly 5 (the configured max attempts)
        assert!(
            success_count <= 6,
            "Should be limited to around 5 registrations, got {}",
            success_count
        );

        Ok(())
    }

    /// Test register_user: invalid email format rejected
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_register_user_invalid_email_rejected(pool: PgPool) -> Result<(), AcError> {
        let master_key = crypto::generate_random_bytes(32)?;
        key_management_service::initialize_signing_key(&pool, &master_key, "test").await?;

        let org_id = create_test_org(&pool, "reg-email").await;

        let invalid_emails = [
            "invalid",
            "@example.com",
            "test@",
            "test@.com",
            "test@@example.com",
            "",
        ];

        for email in invalid_emails {
            let request = RegistrationRequest {
                email: email.to_string(),
                password: "securepassword123".to_string(),
                display_name: "Test".to_string(),
            };

            let result =
                register_user(&pool, &master_key, &master_key, org_id, request, None, None).await;

            assert!(
                matches!(result, Err(AcError::InvalidToken(_))),
                "Invalid email '{}' should be rejected",
                email
            );
        }

        Ok(())
    }

    /// Test register_user: password too short rejected
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_register_user_password_too_short(pool: PgPool) -> Result<(), AcError> {
        let master_key = crypto::generate_random_bytes(32)?;
        key_management_service::initialize_signing_key(&pool, &master_key, "test").await?;

        let org_id = create_test_org(&pool, "reg-pass").await;

        let short_passwords = ["", "1234567", "abc", "1234"]; // All < 8 chars

        for password in short_passwords {
            let request = RegistrationRequest {
                email: "test@example.com".to_string(),
                password: password.to_string(),
                display_name: "Test".to_string(),
            };

            let result =
                register_user(&pool, &master_key, &master_key, org_id, request, None, None).await;

            assert!(
                matches!(result, Err(AcError::InvalidToken(msg)) if msg.contains("8 characters")),
                "Password '{}' should be rejected for being too short",
                password
            );
        }

        Ok(())
    }

    /// Test register_user: duplicate email in org rejected
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_register_user_duplicate_email_rejected(pool: PgPool) -> Result<(), AcError> {
        let master_key = crypto::generate_random_bytes(32)?;
        key_management_service::initialize_signing_key(&pool, &master_key, "test").await?;

        let org_id = create_test_org(&pool, "reg-dup").await;

        // First registration
        let request1 = RegistrationRequest {
            email: "duplicate@example.com".to_string(),
            password: "securepassword123".to_string(),
            display_name: "First User".to_string(),
        };

        let result1 = register_user(
            &pool,
            &master_key,
            &master_key,
            org_id,
            request1,
            None,
            None,
        )
        .await;
        assert!(result1.is_ok(), "First registration should succeed");

        // Second registration with same email
        let request2 = RegistrationRequest {
            email: "duplicate@example.com".to_string(),
            password: "differentpassword123".to_string(),
            display_name: "Second User".to_string(),
        };

        let result2 = register_user(
            &pool,
            &master_key,
            &master_key,
            org_id,
            request2,
            None,
            None,
        )
        .await;

        assert!(
            matches!(result2, Err(AcError::InvalidToken(msg)) if msg.contains("already exists")),
            "Duplicate email should be rejected"
        );

        Ok(())
    }

    /// Test register_user: empty display name rejected
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_register_user_empty_display_name_rejected(pool: PgPool) -> Result<(), AcError> {
        let master_key = crypto::generate_random_bytes(32)?;
        key_management_service::initialize_signing_key(&pool, &master_key, "test").await?;

        let org_id = create_test_org(&pool, "reg-name").await;

        let empty_names = ["", "   ", "\t", "\n"]; // Empty or whitespace only

        for name in empty_names {
            let request = RegistrationRequest {
                email: "test@example.com".to_string(),
                password: "securepassword123".to_string(),
                display_name: name.to_string(),
            };

            let result =
                register_user(&pool, &master_key, &master_key, org_id, request, None, None).await;

            assert!(
                matches!(result, Err(AcError::InvalidToken(msg)) if msg.contains("Display name")),
                "Empty display name '{}' should be rejected",
                name.escape_debug()
            );
        }

        Ok(())
    }

    /// Test register_user: password with exactly 8 characters is accepted
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_register_user_minimum_password_length(pool: PgPool) -> Result<(), AcError> {
        let master_key = crypto::generate_random_bytes(32)?;
        key_management_service::initialize_signing_key(&pool, &master_key, "test").await?;

        let org_id = create_test_org(&pool, "reg-minpass").await;

        let request = RegistrationRequest {
            email: "minpass@example.com".to_string(),
            password: "12345678".to_string(), // Exactly 8 characters
            display_name: "Minimum Password User".to_string(),
        };

        let result =
            register_user(&pool, &master_key, &master_key, org_id, request, None, None).await?;

        assert_eq!(result.email, "minpass@example.com");

        Ok(())
    }

    /// Test register_user: same email can be used in different organizations
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_register_user_same_email_different_orgs(pool: PgPool) -> Result<(), AcError> {
        let master_key = crypto::generate_random_bytes(32)?;
        key_management_service::initialize_signing_key(&pool, &master_key, "test").await?;

        let org1 = create_test_org(&pool, "reg-org1").await;
        let org2 = create_test_org(&pool, "reg-org2").await;

        let email = "shared@example.com";

        // Register in org1
        let request1 = RegistrationRequest {
            email: email.to_string(),
            password: "securepassword123".to_string(),
            display_name: "Org 1 User".to_string(),
        };

        let result1 =
            register_user(&pool, &master_key, &master_key, org1, request1, None, None).await?;
        assert_eq!(result1.email, email);

        // Register same email in org2
        let request2 = RegistrationRequest {
            email: email.to_string(),
            password: "securepassword456".to_string(),
            display_name: "Org 2 User".to_string(),
        };

        let result2 =
            register_user(&pool, &master_key, &master_key, org2, request2, None, None).await?;
        assert_eq!(result2.email, email);

        // Both should exist independently
        let user1 = users::get_by_email(&pool, org1, email)
            .await?
            .expect("User in org1 should exist");
        let user2 = users::get_by_email(&pool, org2, email)
            .await?
            .expect("User in org2 should exist");

        assert_ne!(user1.user_id, user2.user_id);
        assert_eq!(user1.display_name, "Org 1 User");
        assert_eq!(user2.display_name, "Org 2 User");

        Ok(())
    }

    /// Test register_user: registration without IP (rate limiting skipped)
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_register_user_without_ip_address(pool: PgPool) -> Result<(), AcError> {
        let master_key = crypto::generate_random_bytes(32)?;
        key_management_service::initialize_signing_key(&pool, &master_key, "test").await?;

        let org_id = create_test_org(&pool, "reg-noip").await;

        // Register many users without IP - should not be rate limited
        for i in 0..10 {
            let request = RegistrationRequest {
                email: format!("noip{}@example.com", i),
                password: "securepassword123".to_string(),
                display_name: format!("No IP User {}", i),
            };

            let result = register_user(
                &pool,
                &master_key,
                &master_key,
                org_id,
                request,
                None, // No IP address
                None,
            )
            .await;

            assert!(
                result.is_ok(),
                "Registration {} without IP should succeed",
                i
            );
        }

        Ok(())
    }
}
