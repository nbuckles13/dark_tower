//! User service module for registration and account management.
//!
//! Provides business logic for user self-registration per ADR-0020.

use crate::crypto;
use crate::errors::AcError;
use crate::models::AuthEventType;
use crate::observability::metrics::{record_audit_log_failure, record_rate_limit_decision};
use crate::repositories::{auth_events, users};
use crate::services::token_service;
use sqlx::PgPool;
use uuid::Uuid;

// Configuration
const MIN_PASSWORD_LENGTH: usize = 8;

/// Body message for a too-short password. Must name `MIN_PASSWORD_LENGTH`
/// (pinned by `tests::weak_password_message_names_min_length`).
const WEAK_PASSWORD_MESSAGE: &str = "Password must be at least 8 characters";

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

/// A registration attempt refused by validation, with its fixed audit
/// `failure_reason` token. The submitted email, password and display name are
/// never logged or recorded (ADR-0011 PII).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RegistrationRejection {
    InvalidEmail,
    WeakPassword,
    EmptyDisplayName,
    EmailExists,
}

impl RegistrationRejection {
    fn failure_reason(self) -> &'static str {
        match self {
            RegistrationRejection::InvalidEmail => "invalid_email",
            RegistrationRejection::WeakPassword => "weak_password",
            RegistrationRejection::EmptyDisplayName => "empty_display_name",
            RegistrationRejection::EmailExists => "email_exists",
        }
    }

    fn error(self) -> AcError {
        match self {
            RegistrationRejection::InvalidEmail => AcError::BadRequest("Invalid email format"),
            RegistrationRejection::WeakPassword => AcError::BadRequest(WEAK_PASSWORD_MESSAGE),
            RegistrationRejection::EmptyDisplayName => {
                AcError::BadRequest("Display name cannot be empty")
            }
            // Message verbatim until the email-existence oracle is removed
            // (docs/TODO.md, auth-controller).
            RegistrationRejection::EmailExists => {
                AcError::Conflict("An account with this email already exists")
            }
        }
    }
}

/// Register a new user in an organization (ADR-0020).
///
/// # Steps
///
/// 1. Rate limit by IP (successful + failed registrations share one budget)
/// 2. Validate email format, password length, display name
/// 3. Check email doesn't exist in org
/// 4. Hash password, insert user, add default "user" role
/// 5. Log the `user_registered` event, then issue a token (auto-login)
///
/// Every validation failure writes exactly one subject-less
/// `user_registration_failed` row (fixed `failure_reason`, no metadata); a
/// rate-limited request writes none, so a blocked IP cannot extend its own
/// window.
///
/// # Security
///
/// - Rate limiting prevents abuse and throttles the email-existence oracle
/// - Password minimum length enforced
/// - Email uniqueness per organization
/// - `ip_address` is required: the handler always has the ConnectInfo peer, so
///   the limiter cannot be bypassed by a missing address
#[expect(clippy::too_many_arguments)]
pub async fn register_user(
    pool: &PgPool,
    master_key: &[u8],
    hash_secret: &[u8],
    org_id: Uuid,
    request: RegistrationRequest,
    ip_address: &str,
    user_agent: Option<&str>,
    bcrypt_cost: u32,
    registration_rate_limit_window_minutes: i64,
    registration_rate_limit_max_attempts: i64,
    rate_limit_window_minutes: i64,
    rate_limit_max_attempts: i64,
) -> Result<RegistrationResponse, AcError> {
    // Rate limit by IP, BEFORE any validation or lookup.
    //
    // Fails closed: a count-query error propagates, so registration fails
    // rather than running unthrottled. Accepted residuals: (a) the counted rows
    // are best-effort audit rows (ADR-0032), so a failed insert leaves that
    // attempt uncounted — `ACAuditLogWriteFailures` detects it; (c)
    // count-then-insert is not atomic, so a parallel burst can exceed the
    // maximum by up to its concurrency.
    let rate_limit_window_ago =
        chrono::Utc::now() - chrono::Duration::minutes(registration_rate_limit_window_minutes);
    let registration_count =
        auth_events::count_registration_attempts_by_ip(pool, ip_address, rate_limit_window_ago)
            .await?;
    if registration_count >= registration_rate_limit_max_attempts {
        tracing::warn!(
            "Registration rate limit exceeded for IP (count={})",
            registration_count
        );
        record_rate_limit_decision("rejected");
        return Err(AcError::TooManyRequests {
            retry_after_seconds: registration_rate_limit_window_minutes * 60,
            message: "Too many registration attempts. Please try again later.".to_string(),
        });
    }
    record_rate_limit_decision("allowed");

    if let Err(rejection) = validate_registration(pool, org_id, &request).await? {
        tracing::info!(
            failure_reason = rejection.failure_reason(),
            "Registration rejected"
        );
        log_registration_failure(pool, rejection, ip_address, user_agent).await;
        return Err(rejection.error());
    }
    let display_name = request.display_name.trim();

    // Hash password with configured bcrypt cost
    let password_hash = crypto::hash_client_secret(&request.password, bcrypt_cost)?;

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

    // Log registration event (counted by the registration limiter).
    if let Err(e) = log_registration_event(pool, &user.user_id, ip_address, user_agent).await {
        tracing::warn!("Failed to log registration event: {}", e);
        record_audit_log_failure(AuthEventType::UserRegistered, "db_write_failed");
    }

    // Issue token (auto-login)
    let token_response = token_service::issue_user_token(
        pool,
        master_key,
        hash_secret,
        org_id,
        &request.email,
        &request.password,
        Some(ip_address),
        user_agent,
        rate_limit_window_minutes,
        rate_limit_max_attempts,
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

/// Validate a registration request. `Ok(Err(_))` is a client rejection;
/// `Err(_)` is an internal failure of the email-existence lookup.
async fn validate_registration(
    pool: &PgPool,
    org_id: Uuid,
    request: &RegistrationRequest,
) -> Result<Result<(), RegistrationRejection>, AcError> {
    if !is_valid_email(&request.email) {
        return Ok(Err(RegistrationRejection::InvalidEmail));
    }
    if request.password.len() < MIN_PASSWORD_LENGTH {
        return Ok(Err(RegistrationRejection::WeakPassword));
    }
    if request.display_name.trim().is_empty() {
        return Ok(Err(RegistrationRejection::EmptyDisplayName));
    }
    if users::email_exists_in_org(pool, org_id, &request.email).await? {
        return Ok(Err(RegistrationRejection::EmailExists));
    }
    Ok(Ok(()))
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

/// Log a user registration event (`user_registered`). Feeds the registration
/// limiter (see [`auth_events::count_registration_attempts_by_ip`]) — keep
/// `ip_address` on it.
async fn log_registration_event(
    pool: &PgPool,
    user_id: &Uuid,
    ip_address: &str,
    user_agent: Option<&str>,
) -> Result<(), AcError> {
    auth_events::log_event(
        pool,
        AuthEventType::UserRegistered.as_str(),
        Some(*user_id),
        None,
        true,
        None,
        Some(ip_address),
        user_agent,
        None,
    )
    .await?;

    Ok(())
}

/// Log a refused registration attempt (`user_registration_failed`):
/// subject-less, fixed `failure_reason`, no metadata. Feeds the registration
/// limiter. Best-effort (ADR-0032).
async fn log_registration_failure(
    pool: &PgPool,
    rejection: RegistrationRejection,
    ip_address: &str,
    user_agent: Option<&str>,
) {
    if let Err(e) = auth_events::log_event(
        pool,
        AuthEventType::UserRegistrationFailed.as_str(),
        None,
        None,
        false,
        Some(rejection.failure_reason()),
        Some(ip_address),
        user_agent,
        None,
    )
    .await
    {
        tracing::warn!("Failed to log registration failure event: {}", e);
        record_audit_log_failure(AuthEventType::UserRegistrationFailed, "db_write_failed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        DEFAULT_BCRYPT_COST, DEFAULT_RATE_LIMIT_MAX_ATTEMPTS, DEFAULT_RATE_LIMIT_WINDOW_MINUTES,
        DEFAULT_REGISTRATION_RATE_LIMIT_MAX_ATTEMPTS,
        DEFAULT_REGISTRATION_RATE_LIMIT_WINDOW_MINUTES,
    };
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

    /// The weak-password message is a fixed literal (it is the response body);
    /// keep it in step with `MIN_PASSWORD_LENGTH`.
    #[test]
    fn weak_password_message_names_min_length() {
        assert!(WEAK_PASSWORD_MESSAGE.contains(&MIN_PASSWORD_LENGTH.to_string()));
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
            "192.168.1.1",
            Some("TestAgent/1.0"),
            DEFAULT_BCRYPT_COST,
            DEFAULT_REGISTRATION_RATE_LIMIT_WINDOW_MINUTES,
            DEFAULT_REGISTRATION_RATE_LIMIT_MAX_ATTEMPTS,
            DEFAULT_RATE_LIMIT_WINDOW_MINUTES,
            DEFAULT_RATE_LIMIT_MAX_ATTEMPTS,
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

    /// Test register_user: exactly the configured number of registrations per IP
    /// succeed, and the next one is rate limited.
    ///
    /// The limiter counts `user_registered` + `user_registration_failed` rows per IP
    /// (one row per attempt), so the threshold is the configured maximum itself.
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_register_user_rate_limiting(pool: PgPool) -> Result<(), AcError> {
        let master_key = crypto::generate_random_bytes(32)?;
        key_management_service::initialize_signing_key(&pool, &master_key, "test").await?;

        let org_id = create_test_org(&pool, "reg-rate").await;
        let ip = "192.168.1.100";

        let max = usize::try_from(DEFAULT_REGISTRATION_RATE_LIMIT_MAX_ATTEMPTS)
            .expect("configured maximum fits usize");
        let mut success_count = 0;
        let mut hit_rate_limit = false;

        for i in 0..=max {
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
                ip,
                None,
                DEFAULT_BCRYPT_COST,
                DEFAULT_REGISTRATION_RATE_LIMIT_WINDOW_MINUTES,
                DEFAULT_REGISTRATION_RATE_LIMIT_MAX_ATTEMPTS,
                DEFAULT_RATE_LIMIT_WINDOW_MINUTES,
                DEFAULT_RATE_LIMIT_MAX_ATTEMPTS,
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

        // Exactly MAX registrations succeed; attempt MAX+1 is refused.
        assert_eq!(
            success_count, max,
            "exactly the configured maximum of registrations should succeed"
        );
        assert!(hit_rate_limit, "attempt {} should be rate limited", max + 1);

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

        for (i, email) in invalid_emails.into_iter().enumerate() {
            // Distinct IPs: every refused attempt spends registration budget.
            let ip = format!("192.0.2.{}", 10 + i);
            let request = RegistrationRequest {
                email: email.to_string(),
                password: "securepassword123".to_string(),
                display_name: "Test".to_string(),
            };

            let result = register_user(
                &pool,
                &master_key,
                &master_key,
                org_id,
                request,
                &ip,
                None,
                DEFAULT_BCRYPT_COST,
                DEFAULT_REGISTRATION_RATE_LIMIT_WINDOW_MINUTES,
                DEFAULT_REGISTRATION_RATE_LIMIT_MAX_ATTEMPTS,
                DEFAULT_RATE_LIMIT_WINDOW_MINUTES,
                DEFAULT_RATE_LIMIT_MAX_ATTEMPTS,
            )
            .await;

            assert!(
                matches!(result, Err(AcError::BadRequest("Invalid email format"))),
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

            let result = register_user(
                &pool,
                &master_key,
                &master_key,
                org_id,
                request,
                "192.0.2.1",
                None,
                DEFAULT_BCRYPT_COST,
                DEFAULT_REGISTRATION_RATE_LIMIT_WINDOW_MINUTES,
                DEFAULT_REGISTRATION_RATE_LIMIT_MAX_ATTEMPTS,
                DEFAULT_RATE_LIMIT_WINDOW_MINUTES,
                DEFAULT_RATE_LIMIT_MAX_ATTEMPTS,
            )
            .await;

            assert!(
                matches!(result, Err(AcError::BadRequest(msg)) if msg == WEAK_PASSWORD_MESSAGE),
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
            "192.0.2.1",
            None,
            DEFAULT_BCRYPT_COST,
            DEFAULT_REGISTRATION_RATE_LIMIT_WINDOW_MINUTES,
            DEFAULT_REGISTRATION_RATE_LIMIT_MAX_ATTEMPTS,
            DEFAULT_RATE_LIMIT_WINDOW_MINUTES,
            DEFAULT_RATE_LIMIT_MAX_ATTEMPTS,
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
            "192.0.2.1",
            None,
            DEFAULT_BCRYPT_COST,
            DEFAULT_REGISTRATION_RATE_LIMIT_WINDOW_MINUTES,
            DEFAULT_REGISTRATION_RATE_LIMIT_MAX_ATTEMPTS,
            DEFAULT_RATE_LIMIT_WINDOW_MINUTES,
            DEFAULT_RATE_LIMIT_MAX_ATTEMPTS,
        )
        .await;

        assert!(
            matches!(
                result2,
                Err(AcError::Conflict(
                    "An account with this email already exists"
                ))
            ),
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

            let result = register_user(
                &pool,
                &master_key,
                &master_key,
                org_id,
                request,
                "192.0.2.1",
                None,
                DEFAULT_BCRYPT_COST,
                DEFAULT_REGISTRATION_RATE_LIMIT_WINDOW_MINUTES,
                DEFAULT_REGISTRATION_RATE_LIMIT_MAX_ATTEMPTS,
                DEFAULT_RATE_LIMIT_WINDOW_MINUTES,
                DEFAULT_RATE_LIMIT_MAX_ATTEMPTS,
            )
            .await;

            assert!(
                matches!(
                    result,
                    Err(AcError::BadRequest("Display name cannot be empty"))
                ),
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

        let result = register_user(
            &pool,
            &master_key,
            &master_key,
            org_id,
            request,
            "192.0.2.1",
            None,
            DEFAULT_BCRYPT_COST,
            DEFAULT_REGISTRATION_RATE_LIMIT_WINDOW_MINUTES,
            DEFAULT_REGISTRATION_RATE_LIMIT_MAX_ATTEMPTS,
            DEFAULT_RATE_LIMIT_WINDOW_MINUTES,
            DEFAULT_RATE_LIMIT_MAX_ATTEMPTS,
        )
        .await?;

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

        let result1 = register_user(
            &pool,
            &master_key,
            &master_key,
            org1,
            request1,
            "192.0.2.1",
            None,
            DEFAULT_BCRYPT_COST,
            DEFAULT_REGISTRATION_RATE_LIMIT_WINDOW_MINUTES,
            DEFAULT_REGISTRATION_RATE_LIMIT_MAX_ATTEMPTS,
            DEFAULT_RATE_LIMIT_WINDOW_MINUTES,
            DEFAULT_RATE_LIMIT_MAX_ATTEMPTS,
        )
        .await?;
        assert_eq!(result1.email, email);

        // Register same email in org2
        let request2 = RegistrationRequest {
            email: email.to_string(),
            password: "securepassword456".to_string(),
            display_name: "Org 2 User".to_string(),
        };

        let result2 = register_user(
            &pool,
            &master_key,
            &master_key,
            org2,
            request2,
            "192.0.2.1",
            None,
            DEFAULT_BCRYPT_COST,
            DEFAULT_REGISTRATION_RATE_LIMIT_WINDOW_MINUTES,
            DEFAULT_REGISTRATION_RATE_LIMIT_MAX_ATTEMPTS,
            DEFAULT_RATE_LIMIT_WINDOW_MINUTES,
            DEFAULT_RATE_LIMIT_MAX_ATTEMPTS,
        )
        .await?;
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
}
