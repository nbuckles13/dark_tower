use crate::errors::AcError;
use crate::models::AuthEvent;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

/// Log an authentication event
#[expect(clippy::too_many_arguments)] // Represents all auth_events table columns
pub async fn log_event(
    pool: &PgPool,
    event_type: &str,
    user_id: Option<Uuid>,
    credential_id: Option<Uuid>,
    success: bool,
    failure_reason: Option<&str>,
    ip_address: Option<&str>,
    user_agent: Option<&str>,
    metadata: Option<serde_json::Value>,
) -> Result<AuthEvent, AcError> {
    let event = sqlx::query_as::<_, AuthEvent>(
        r#"
        INSERT INTO auth_events (
            event_type, user_id, credential_id, success, failure_reason,
            ip_address, user_agent, metadata
        )
        VALUES ($1, $2, $3, $4, $5, $6::inet, $7, $8)
        RETURNING
            event_id, event_type, user_id, credential_id, success, failure_reason,
            host(ip_address) as ip_address, user_agent, metadata, created_at
        "#,
    )
    .bind(event_type)
    .bind(user_id)
    .bind(credential_id)
    .bind(success)
    .bind(failure_reason)
    .bind(ip_address)
    .bind(user_agent)
    .bind(metadata)
    .fetch_one(pool)
    .await
    .map_err(|e| AcError::Database(format!("Failed to log auth event: {}", e)))?;

    Ok(event)
}

/// Get authentication events for a user
#[expect(dead_code)] // Will be used in Phase 4 audit endpoints
pub async fn get_events_by_user(
    pool: &PgPool,
    user_id: Uuid,
    limit: i64,
) -> Result<Vec<AuthEvent>, AcError> {
    let events = sqlx::query_as::<_, AuthEvent>(
        r#"
        SELECT
            event_id, event_type, user_id, credential_id, success, failure_reason,
            host(ip_address) as ip_address, user_agent, metadata, created_at
        FROM auth_events
        WHERE user_id = $1
        ORDER BY created_at DESC
        LIMIT $2
        "#,
    )
    .bind(user_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|e| AcError::Database(format!("Failed to fetch user events: {}", e)))?;

    Ok(events)
}

/// Get authentication events for a service credential
#[expect(dead_code)] // Will be used in Phase 4 audit endpoints
pub async fn get_events_by_credential(
    pool: &PgPool,
    credential_id: Uuid,
    limit: i64,
) -> Result<Vec<AuthEvent>, AcError> {
    let events = sqlx::query_as::<_, AuthEvent>(
        r#"
        SELECT
            event_id, event_type, user_id, credential_id, success, failure_reason,
            host(ip_address) as ip_address, user_agent, metadata, created_at
        FROM auth_events
        WHERE credential_id = $1
        ORDER BY created_at DESC
        LIMIT $2
        "#,
    )
    .bind(credential_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|e| AcError::Database(format!("Failed to fetch credential events: {}", e)))?;

    Ok(events)
}

/// Get failed authentication attempts from an IP address
#[expect(dead_code)] // Will be used in Phase 4 rate limiting
pub async fn get_failed_attempts_by_ip(
    pool: &PgPool,
    ip_address: &str,
    since: DateTime<Utc>,
) -> Result<Vec<AuthEvent>, AcError> {
    let events = sqlx::query_as::<_, AuthEvent>(
        r#"
        SELECT
            event_id, event_type, user_id, credential_id, success, failure_reason,
            host(ip_address) as ip_address, user_agent, metadata, created_at
        FROM auth_events
        WHERE ip_address = $1::inet
            AND success = false
            AND created_at >= $2
        ORDER BY created_at DESC
        "#,
    )
    .bind(ip_address)
    .bind(since)
    .fetch_all(pool)
    .await
    .map_err(|e| AcError::Database(format!("Failed to fetch failed attempts: {}", e)))?;

    Ok(events)
}

/// Get events by type within a time range
#[expect(dead_code)] // Will be used in Phase 4 analytics/monitoring
pub async fn get_events_by_type(
    pool: &PgPool,
    event_type: &str,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
    limit: i64,
) -> Result<Vec<AuthEvent>, AcError> {
    let events = sqlx::query_as::<_, AuthEvent>(
        r#"
        SELECT
            event_id, event_type, user_id, credential_id, success, failure_reason,
            host(ip_address) as ip_address, user_agent, metadata, created_at
        FROM auth_events
        WHERE event_type = $1
            AND created_at >= $2
            AND created_at <= $3
        ORDER BY created_at DESC
        LIMIT $4
        "#,
    )
    .bind(event_type)
    .bind(start_time)
    .bind(end_time)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|e| AcError::Database(format!("Failed to fetch events by type: {}", e)))?;

    Ok(events)
}

/// Get count of failed authentication attempts for a credential since a given time
pub async fn get_failed_attempts_count(
    pool: &PgPool,
    credential_id: &Uuid,
    since: DateTime<Utc>,
) -> Result<i64, AcError> {
    let count: (i64,) = sqlx::query_as(
        r#"
        SELECT COUNT(*)
        FROM auth_events
        WHERE credential_id = $1
          AND success = false
          AND created_at >= $2
        "#,
    )
    .bind(credential_id)
    .bind(since)
    .fetch_one(pool)
    .await
    .map_err(|e| AcError::Database(format!("Failed to count failed attempts: {}", e)))?;

    Ok(count.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repositories::service_credentials;
    use chrono::Duration;

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_log_event_success(pool: PgPool) -> Result<(), AcError> {
        // Create a service credential first to satisfy foreign key
        let credential = service_credentials::create_service_credential(
            &pool,
            "test-client",
            "hash",
            "global-controller",
            None,
            &vec!["test:scope".to_string()],
        )
        .await?;
        let credential_id = credential.credential_id;

        let event = log_event(
            &pool,
            "service_token_issued",
            None,
            Some(credential_id),
            true,
            None,
            Some("192.168.1.1"),
            Some("test-agent"),
            Some(serde_json::json!({"test": "data"})),
        )
        .await?;

        assert_eq!(event.event_type, "service_token_issued");
        assert_eq!(event.credential_id, Some(credential_id));
        assert!(event.success);
        assert_eq!(event.failure_reason, None);
        assert_eq!(event.ip_address, Some("192.168.1.1".to_string()));
        assert_eq!(event.user_agent, Some("test-agent".to_string()));
        assert!(event.metadata.is_some());

        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_log_event_failure(pool: PgPool) -> Result<(), AcError> {
        // Create a service credential first to satisfy foreign key
        let credential = service_credentials::create_service_credential(
            &pool,
            "test-client-fail",
            "hash",
            "global-controller",
            None,
            &vec!["test:scope".to_string()],
        )
        .await?;
        let credential_id = credential.credential_id;

        let event = log_event(
            &pool,
            "service_token_failed",
            None,
            Some(credential_id),
            false,
            Some("invalid_credentials"),
            Some("192.168.1.100"),
            None,
            None,
        )
        .await?;

        assert_eq!(event.event_type, "service_token_failed");
        assert!(!event.success);
        assert_eq!(
            event.failure_reason,
            Some("invalid_credentials".to_string())
        );
        assert_eq!(event.ip_address, Some("192.168.1.100".to_string()));

        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_get_events_by_user(pool: PgPool) -> Result<(), AcError> {
        // Note: users table doesn't exist yet (Phase 2+), so we test with key rotation events
        // which don't require user_id or credential_id

        // Log multiple key rotation events
        for _ in 0..5 {
            log_event(
                &pool,
                "key_rotated",
                None,
                None,
                true,
                None,
                Some("192.168.1.1"),
                None,
                Some(serde_json::json!({"key_id": "test-key"})),
            )
            .await?;
        }

        // For now, just verify we can log events without user_id
        // Once users table exists, we can test get_events_by_user properly
        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_get_events_by_credential(pool: PgPool) -> Result<(), AcError> {
        // Create service credentials for testing
        let credential = service_credentials::create_service_credential(
            &pool,
            "test-cred-1",
            "hash",
            "global-controller",
            None,
            &vec!["test:scope".to_string()],
        )
        .await?;
        let credential_id = credential.credential_id;

        let other_credential = service_credentials::create_service_credential(
            &pool,
            "test-cred-2",
            "hash",
            "media-handler",
            None,
            &vec!["test:scope".to_string()],
        )
        .await?;
        let other_credential_id = other_credential.credential_id;

        // Log events for different credentials
        for _ in 0..3 {
            log_event(
                &pool,
                "service_token_issued",
                None,
                Some(credential_id),
                true,
                None,
                None,
                None,
                None,
            )
            .await?;
        }

        log_event(
            &pool,
            "service_token_issued",
            None,
            Some(other_credential_id),
            true,
            None,
            None,
            None,
            None,
        )
        .await?;

        // Should only get events for the specific credential
        let events = get_events_by_credential(&pool, credential_id, 10).await?;
        assert_eq!(events.len(), 3);

        for event in &events {
            assert_eq!(event.credential_id, Some(credential_id));
        }

        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_get_failed_attempts_by_ip(pool: PgPool) -> Result<(), AcError> {
        let now = Utc::now();
        let one_hour_ago = now - Duration::hours(1);

        // Create credentials for testing
        let cred1 = service_credentials::create_service_credential(
            &pool,
            "test-ip-1",
            "hash",
            "global-controller",
            None,
            &vec!["test:scope".to_string()],
        )
        .await?;
        let cred2 = service_credentials::create_service_credential(
            &pool,
            "test-ip-2",
            "hash",
            "global-controller",
            None,
            &vec!["test:scope".to_string()],
        )
        .await?;
        let cred3 = service_credentials::create_service_credential(
            &pool,
            "test-ip-3",
            "hash",
            "global-controller",
            None,
            &vec!["test:scope".to_string()],
        )
        .await?;

        // Log failed attempts from same IP
        for i in 0..3 {
            log_event(
                &pool,
                "service_token_failed",
                None,
                Some(
                    [
                        cred1.credential_id,
                        cred2.credential_id,
                        cred3.credential_id,
                    ][i],
                ),
                false,
                Some("invalid_credentials"),
                Some("192.168.1.100"),
                None,
                None,
            )
            .await?;
        }

        // Create another credential for successful attempt
        let cred4 = service_credentials::create_service_credential(
            &pool,
            "test-ip-4",
            "hash",
            "global-controller",
            None,
            &vec!["test:scope".to_string()],
        )
        .await?;

        // Log successful attempt from same IP (should be excluded)
        log_event(
            &pool,
            "service_token_issued",
            None,
            Some(cred4.credential_id),
            true,
            None,
            Some("192.168.1.100"),
            None,
            None,
        )
        .await?;

        // Create credential for different IP
        let cred5 = service_credentials::create_service_credential(
            &pool,
            "test-ip-5",
            "hash",
            "global-controller",
            None,
            &vec!["test:scope".to_string()],
        )
        .await?;

        // Log failed attempt from different IP (should be excluded)
        log_event(
            &pool,
            "service_token_failed",
            None,
            Some(cred5.credential_id),
            false,
            Some("invalid_credentials"),
            Some("192.168.1.200"),
            None,
            None,
        )
        .await?;

        let failed_attempts =
            get_failed_attempts_by_ip(&pool, "192.168.1.100", one_hour_ago).await?;

        // Should only get failed attempts from the specific IP
        assert_eq!(failed_attempts.len(), 3);
        for event in &failed_attempts {
            assert_eq!(event.ip_address, Some("192.168.1.100".to_string()));
            assert!(!event.success);
        }

        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_get_events_by_type(pool: PgPool) -> Result<(), AcError> {
        let now = Utc::now();
        let start_time = now - Duration::hours(1);
        let end_time = now + Duration::hours(1);

        // Create credentials for service token events
        let cred1 = service_credentials::create_service_credential(
            &pool,
            "test-type-1",
            "hash",
            "global-controller",
            None,
            &vec!["test:scope".to_string()],
        )
        .await?;
        let cred2 = service_credentials::create_service_credential(
            &pool,
            "test-type-2",
            "hash",
            "global-controller",
            None,
            &vec!["test:scope".to_string()],
        )
        .await?;

        // Log different event types
        log_event(
            &pool,
            "service_token_issued",
            None,
            Some(cred1.credential_id),
            true,
            None,
            None,
            None,
            None,
        )
        .await?;

        log_event(
            &pool,
            "service_token_issued",
            None,
            Some(cred2.credential_id),
            true,
            None,
            None,
            None,
            None,
        )
        .await?;

        log_event(
            &pool,
            "key_rotated",
            None,
            None,
            true,
            None,
            None,
            None,
            None,
        )
        .await?;

        let token_events =
            get_events_by_type(&pool, "service_token_issued", start_time, end_time, 10).await?;

        assert_eq!(token_events.len(), 2);
        for event in &token_events {
            assert_eq!(event.event_type, "service_token_issued");
        }

        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_get_failed_attempts_count(pool: PgPool) -> Result<(), AcError> {
        // Create credential for testing
        let credential = service_credentials::create_service_credential(
            &pool,
            "test-count",
            "hash",
            "global-controller",
            None,
            &vec!["test:scope".to_string()],
        )
        .await?;
        let credential_id = credential.credential_id;

        let now = Utc::now();
        let fifteen_minutes_ago = now - Duration::minutes(15);

        // Log 5 failed attempts
        for _ in 0..5 {
            log_event(
                &pool,
                "service_token_failed",
                None,
                Some(credential_id),
                false,
                Some("invalid_credentials"),
                Some("192.168.1.1"),
                None,
                None,
            )
            .await?;
        }

        // Log 1 successful attempt (should not be counted)
        log_event(
            &pool,
            "service_token_issued",
            None,
            Some(credential_id),
            true,
            None,
            Some("192.168.1.1"),
            None,
            None,
        )
        .await?;

        let count = get_failed_attempts_count(&pool, &credential_id, fifteen_minutes_ago).await?;

        assert_eq!(count, 5, "Should count only failed attempts");

        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_event_ordering(pool: PgPool) -> Result<(), AcError> {
        // Create credential for testing
        let credential = service_credentials::create_service_credential(
            &pool,
            "test-order",
            "hash",
            "global-controller",
            None,
            &vec!["test:scope".to_string()],
        )
        .await?;
        let credential_id = credential.credential_id;

        // Log events in sequence
        for i in 0..3 {
            log_event(
                &pool,
                "service_token_issued",
                None,
                Some(credential_id),
                true,
                None,
                Some(&format!("192.168.1.{}", i)),
                None,
                None,
            )
            .await?;

            // Small delay to ensure different timestamps
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }

        let events = get_events_by_credential(&pool, credential_id, 10).await?;

        assert_eq!(events.len(), 3);

        // Verify DESC ordering (most recent first)
        assert_eq!(events[0].ip_address, Some("192.168.1.2".to_string()));
        assert_eq!(events[1].ip_address, Some("192.168.1.1".to_string()));
        assert_eq!(events[2].ip_address, Some("192.168.1.0".to_string()));

        Ok(())
    }
}
