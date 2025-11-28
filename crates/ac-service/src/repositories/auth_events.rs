use crate::errors::AcError;
use crate::models::AuthEvent;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

/// Log an authentication event
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
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        RETURNING
            event_id, event_type, user_id, credential_id, success, failure_reason,
            ip_address, user_agent, metadata, created_at
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
pub async fn get_events_by_user(
    pool: &PgPool,
    user_id: Uuid,
    limit: i64,
) -> Result<Vec<AuthEvent>, AcError> {
    let events = sqlx::query_as::<_, AuthEvent>(
        r#"
        SELECT
            event_id, event_type, user_id, credential_id, success, failure_reason,
            ip_address, user_agent, metadata, created_at
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
pub async fn get_events_by_credential(
    pool: &PgPool,
    credential_id: Uuid,
    limit: i64,
) -> Result<Vec<AuthEvent>, AcError> {
    let events = sqlx::query_as::<_, AuthEvent>(
        r#"
        SELECT
            event_id, event_type, user_id, credential_id, success, failure_reason,
            ip_address, user_agent, metadata, created_at
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
pub async fn get_failed_attempts_by_ip(
    pool: &PgPool,
    ip_address: &str,
    since: DateTime<Utc>,
) -> Result<Vec<AuthEvent>, AcError> {
    let events = sqlx::query_as::<_, AuthEvent>(
        r#"
        SELECT
            event_id, event_type, user_id, credential_id, success, failure_reason,
            ip_address, user_agent, metadata, created_at
        FROM auth_events
        WHERE ip_address = $1
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
            ip_address, user_agent, metadata, created_at
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
