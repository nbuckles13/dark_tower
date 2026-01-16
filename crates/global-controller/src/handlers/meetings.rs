//! Meeting handlers for Global Controller.
//!
//! Implements meeting join and settings management endpoints:
//!
//! - `GET /v1/meetings/{code}` - Join meeting (authenticated)
//! - `POST /v1/meetings/{code}/guest-token` - Get guest token (public)
//! - `PATCH /v1/meetings/{id}/settings` - Update meeting settings (host only)
//!
//! # Security
//!
//! - Authenticated endpoints validate JWT against AC JWKS
//! - Guest endpoint is public but rate limited (5 req/min per IP)
//! - Guest IDs generated using CSPRNG
//! - Error messages are generic to prevent information leakage

use crate::auth::Claims;
use crate::errors::GcError;
use crate::models::{
    GuestJoinRequest, JoinMeetingResponse, MeetingResponse, MeetingRow,
    UpdateMeetingSettingsRequest,
};
use crate::routes::AppState;
use crate::services::ac_client::{
    AcClient, GuestTokenRequest, MeetingRole, MeetingTokenRequest, ParticipantType,
};
use axum::{
    extract::{Path, State},
    Extension, Json,
};
use ring::rand::{SecureRandom, SystemRandom};
use sqlx::{PgPool, Row};
use std::sync::Arc;
use tracing::{info, instrument, warn};
use uuid::Uuid;

/// Default token TTL in seconds (15 minutes).
const DEFAULT_TOKEN_TTL_SECONDS: u32 = 900;

/// Default capabilities for meeting participants.
const DEFAULT_PARTICIPANT_CAPABILITIES: &[&str] = &["audio", "video", "screen_share", "chat"];

// ============================================================================
// Handler: GET /v1/meetings/{code}
// ============================================================================

/// Handler for GET /v1/meetings/{code}
///
/// Join a meeting as an authenticated user. Validates the user's token,
/// checks meeting permissions, and returns a meeting token from AC.
///
/// # Authorization
///
/// - Same organization: Always allowed
/// - Different organization: Only if `allow_external_participants` is true
///
/// # Response
///
/// - 200 OK: Meeting token returned
/// - 401 Unauthorized: Invalid or missing token
/// - 403 Forbidden: User not allowed to join
/// - 404 Not Found: Meeting not found
/// - 503 Service Unavailable: AC unreachable
#[instrument(skip(state, claims), fields(meeting_code = %code))]
pub async fn join_meeting(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<Claims>,
    Path(code): Path<String>,
) -> Result<Json<JoinMeetingResponse>, GcError> {
    // Look up meeting by code
    let meeting = find_meeting_by_code(&state.pool, &code).await?;

    // Check meeting status
    if meeting.status == "cancelled" || meeting.status == "ended" {
        return Err(GcError::NotFound(
            "Meeting not found or has ended".to_string(),
        ));
    }

    // Parse user's org_id from claims (assuming sub is in format "user:{user_id}" or just user_id)
    // For now, we need to look up the user's org_id from the database
    let user_id = parse_user_id(&claims.sub)?;
    let user_org_id = get_user_org_id(&state.pool, user_id).await?;

    // Check if user is allowed to join
    let is_same_org = user_org_id == meeting.org_id;
    let is_host = meeting.created_by_user_id == user_id;

    if !is_same_org && !meeting.allow_external_participants {
        warn!(
            target: "gc.handlers.meetings",
            user_id = %user_id,
            meeting_id = %meeting.meeting_id,
            "External user denied access to meeting"
        );
        return Err(GcError::Forbidden(
            "External participants are not allowed in this meeting".to_string(),
        ));
    }

    // Determine participant type and role
    let (participant_type, home_org_id) = if is_same_org {
        (ParticipantType::Member, None)
    } else {
        (ParticipantType::External, Some(user_org_id))
    };

    let role = if is_host {
        MeetingRole::Host
    } else {
        MeetingRole::Participant
    };

    // Create AC client and request meeting token
    let ac_client = create_ac_client(&state)?;
    let token_request = MeetingTokenRequest {
        subject_user_id: user_id,
        meeting_id: meeting.meeting_id,
        meeting_org_id: meeting.org_id,
        home_org_id,
        participant_type,
        role,
        capabilities: DEFAULT_PARTICIPANT_CAPABILITIES
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
        ttl_seconds: DEFAULT_TOKEN_TTL_SECONDS,
    };

    let token_response = ac_client.request_meeting_token(&token_request).await?;

    info!(
        target: "gc.handlers.meetings",
        meeting_id = %meeting.meeting_id,
        user_id = %user_id,
        participant_type = ?participant_type,
        "User joined meeting"
    );

    Ok(Json(JoinMeetingResponse {
        token: token_response.token,
        expires_in: token_response.expires_in,
        meeting_id: meeting.meeting_id,
        meeting_name: meeting.display_name,
    }))
}

// ============================================================================
// Handler: POST /v1/meetings/{code}/guest-token
// ============================================================================

/// Handler for POST /v1/meetings/{code}/guest-token
///
/// Get a guest token to join a meeting as an anonymous user.
/// This is a PUBLIC endpoint (no authentication required).
///
/// # Rate Limiting
///
/// 5 requests per minute per IP address.
///
/// # Request Body
///
/// ```json
/// {
///   "display_name": "Guest Name",
///   "captcha_token": "recaptcha-token"
/// }
/// ```
///
/// # Response
///
/// - 200 OK: Guest token returned
/// - 400 Bad Request: Invalid request body
/// - 403 Forbidden: Guests not allowed
/// - 404 Not Found: Meeting not found
/// - 429 Too Many Requests: Rate limit exceeded
/// - 503 Service Unavailable: AC unreachable
#[instrument(skip(state, request), fields(meeting_code = %code))]
pub async fn get_guest_token(
    State(state): State<Arc<AppState>>,
    Path(code): Path<String>,
    Json(request): Json<GuestJoinRequest>,
) -> Result<Json<JoinMeetingResponse>, GcError> {
    // Validate request
    request
        .validate()
        .map_err(|e| GcError::BadRequest(e.to_string()))?;

    // TODO: Validate captcha token (integration with captcha service)
    // For now, we just check that it's not empty (validation handles this)

    // Look up meeting by code
    let meeting = find_meeting_by_code(&state.pool, &code).await?;

    // Check meeting status
    if meeting.status == "cancelled" || meeting.status == "ended" {
        return Err(GcError::NotFound(
            "Meeting not found or has ended".to_string(),
        ));
    }

    // Check if guests are allowed
    if !meeting.allow_guests {
        warn!(
            target: "gc.handlers.meetings",
            meeting_id = %meeting.meeting_id,
            "Guest denied access - guests not allowed"
        );
        return Err(GcError::Forbidden(
            "Guest access is not allowed in this meeting".to_string(),
        ));
    }

    // Generate guest ID using CSPRNG
    let guest_id = generate_guest_id()?;

    // Create AC client and request guest token
    let ac_client = create_ac_client(&state)?;
    let token_request = GuestTokenRequest {
        guest_id,
        display_name: request.display_name.trim().to_string(),
        meeting_id: meeting.meeting_id,
        meeting_org_id: meeting.org_id,
        waiting_room: meeting.waiting_room_enabled,
        ttl_seconds: DEFAULT_TOKEN_TTL_SECONDS,
    };

    let token_response = ac_client.request_guest_token(&token_request).await?;

    info!(
        target: "gc.handlers.meetings",
        meeting_id = %meeting.meeting_id,
        guest_id = %guest_id,
        waiting_room = meeting.waiting_room_enabled,
        "Guest joined meeting"
    );

    Ok(Json(JoinMeetingResponse {
        token: token_response.token,
        expires_in: token_response.expires_in,
        meeting_id: meeting.meeting_id,
        meeting_name: meeting.display_name,
    }))
}

// ============================================================================
// Handler: PATCH /v1/meetings/{id}/settings
// ============================================================================

/// Handler for PATCH /v1/meetings/{id}/settings
///
/// Update meeting settings. Only the meeting host can update settings.
///
/// # Request Body
///
/// ```json
/// {
///   "allow_guests": true,
///   "allow_external_participants": false,
///   "waiting_room_enabled": true
/// }
/// ```
///
/// All fields are optional - only provided fields will be updated.
///
/// # Response
///
/// - 200 OK: Updated meeting returned
/// - 400 Bad Request: Invalid request body
/// - 401 Unauthorized: Invalid or missing token
/// - 403 Forbidden: User is not the host
/// - 404 Not Found: Meeting not found
#[instrument(skip(state, claims, request), fields(meeting_id = %meeting_id))]
pub async fn update_meeting_settings(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<Claims>,
    Path(meeting_id): Path<Uuid>,
    Json(request): Json<UpdateMeetingSettingsRequest>,
) -> Result<Json<MeetingResponse>, GcError> {
    // Check if request has any changes
    if !request.has_changes() {
        return Err(GcError::BadRequest("No changes provided".to_string()));
    }

    // Look up meeting by ID
    let meeting = find_meeting_by_id(&state.pool, meeting_id).await?;

    // Parse user ID and verify host status
    let user_id = parse_user_id(&claims.sub)?;

    if meeting.created_by_user_id != user_id {
        warn!(
            target: "gc.handlers.meetings",
            meeting_id = %meeting_id,
            user_id = %user_id,
            host_id = %meeting.created_by_user_id,
            "Non-host user attempted to update meeting settings"
        );
        return Err(GcError::Forbidden(
            "Only the meeting host can update settings".to_string(),
        ));
    }

    // Update meeting settings
    let updated_meeting = update_meeting_settings_in_db(&state.pool, meeting_id, &request).await?;

    info!(
        target: "gc.handlers.meetings",
        meeting_id = %meeting_id,
        user_id = %user_id,
        "Meeting settings updated"
    );

    Ok(Json(MeetingResponse::from(updated_meeting)))
}

// ============================================================================
// Database Helpers
// ============================================================================

/// SQL query for selecting all meeting fields.
const MEETING_SELECT_QUERY: &str = r#"
    SELECT
        meeting_id,
        org_id,
        created_by_user_id,
        display_name,
        meeting_code,
        join_token_secret,
        max_participants,
        enable_e2e_encryption,
        require_auth,
        recording_enabled,
        meeting_controller_id,
        meeting_controller_region,
        status,
        scheduled_start_time,
        actual_start_time,
        actual_end_time,
        created_at,
        updated_at,
        allow_guests,
        allow_external_participants,
        waiting_room_enabled
    FROM meetings
"#;

/// Find a meeting by its code.
async fn find_meeting_by_code(pool: &PgPool, code: &str) -> Result<MeetingRow, GcError> {
    let query = format!("{} WHERE meeting_code = $1", MEETING_SELECT_QUERY);

    let row = sqlx::query(&query)
        .bind(code)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| GcError::NotFound("Meeting not found".to_string()))?;

    map_row_to_meeting(row)
}

/// Find a meeting by its ID.
async fn find_meeting_by_id(pool: &PgPool, meeting_id: Uuid) -> Result<MeetingRow, GcError> {
    let query = format!("{} WHERE meeting_id = $1", MEETING_SELECT_QUERY);

    let row = sqlx::query(&query)
        .bind(meeting_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| GcError::NotFound("Meeting not found".to_string()))?;

    map_row_to_meeting(row)
}

/// Get a user's organization ID.
async fn get_user_org_id(pool: &PgPool, user_id: Uuid) -> Result<Uuid, GcError> {
    let row = sqlx::query(
        r#"
        SELECT org_id
        FROM users
        WHERE user_id = $1 AND is_active = true
        "#,
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| GcError::NotFound("User not found".to_string()))?;

    Ok(row.get("org_id"))
}

/// Update meeting settings in the database.
async fn update_meeting_settings_in_db(
    pool: &PgPool,
    meeting_id: Uuid,
    request: &UpdateMeetingSettingsRequest,
) -> Result<MeetingRow, GcError> {
    let row = sqlx::query(
        r#"
        UPDATE meetings
        SET
            allow_guests = COALESCE($2, allow_guests),
            allow_external_participants = COALESCE($3, allow_external_participants),
            waiting_room_enabled = COALESCE($4, waiting_room_enabled),
            updated_at = NOW()
        WHERE meeting_id = $1
        RETURNING
            meeting_id,
            org_id,
            created_by_user_id,
            display_name,
            meeting_code,
            join_token_secret,
            max_participants,
            enable_e2e_encryption,
            require_auth,
            recording_enabled,
            meeting_controller_id,
            meeting_controller_region,
            status,
            scheduled_start_time,
            actual_start_time,
            actual_end_time,
            created_at,
            updated_at,
            allow_guests,
            allow_external_participants,
            waiting_room_enabled
        "#,
    )
    .bind(meeting_id)
    .bind(request.allow_guests)
    .bind(request.allow_external_participants)
    .bind(request.waiting_room_enabled)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| GcError::NotFound("Meeting not found".to_string()))?;

    map_row_to_meeting(row)
}

/// Map a database row to MeetingRow struct.
fn map_row_to_meeting(row: sqlx::postgres::PgRow) -> Result<MeetingRow, GcError> {
    Ok(MeetingRow {
        meeting_id: row.get("meeting_id"),
        org_id: row.get("org_id"),
        created_by_user_id: row.get("created_by_user_id"),
        display_name: row.get("display_name"),
        meeting_code: row.get("meeting_code"),
        join_token_secret: row.get("join_token_secret"),
        max_participants: row.get("max_participants"),
        enable_e2e_encryption: row.get("enable_e2e_encryption"),
        require_auth: row.get("require_auth"),
        recording_enabled: row.get("recording_enabled"),
        meeting_controller_id: row.get("meeting_controller_id"),
        meeting_controller_region: row.get("meeting_controller_region"),
        status: row.get("status"),
        scheduled_start_time: row.get("scheduled_start_time"),
        actual_start_time: row.get("actual_start_time"),
        actual_end_time: row.get("actual_end_time"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        allow_guests: row.get("allow_guests"),
        allow_external_participants: row.get("allow_external_participants"),
        waiting_room_enabled: row.get("waiting_room_enabled"),
    })
}

// ============================================================================
// Utility Helpers
// ============================================================================

/// Parse user ID from JWT subject.
///
/// Supports both plain UUID and "user:{uuid}" formats.
fn parse_user_id(sub: &str) -> Result<Uuid, GcError> {
    let uuid_str = sub.strip_prefix("user:").unwrap_or(sub);
    Uuid::parse_str(uuid_str)
        .map_err(|_| GcError::InvalidToken("Invalid user identifier in token".to_string()))
}

/// Generate a cryptographically secure guest ID.
fn generate_guest_id() -> Result<Uuid, GcError> {
    let rng = SystemRandom::new();
    let mut bytes = [0u8; 16];

    rng.fill(&mut bytes).map_err(|_| {
        tracing::error!(target: "gc.handlers.meetings", "Failed to generate random bytes");
        GcError::Internal
    })?;

    // Set UUID version 4 and variant bits
    bytes[6] = (bytes[6] & 0x0f) | 0x40; // Version 4
    bytes[8] = (bytes[8] & 0x3f) | 0x80; // Variant 1

    Ok(Uuid::from_bytes(bytes))
}

/// Create AC client with configuration from state.
fn create_ac_client(state: &AppState) -> Result<AcClient, GcError> {
    // TODO: Get service token from config or token refresh service
    // For now, use a placeholder that will need to be configured
    let service_token = std::env::var("GC_SERVICE_TOKEN").unwrap_or_default();

    AcClient::new(state.config.ac_internal_url.clone(), service_token)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_user_id_plain_uuid() {
        let uuid = Uuid::new_v4();
        let result = parse_user_id(&uuid.to_string());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), uuid);
    }

    #[test]
    fn test_parse_user_id_with_prefix() {
        let uuid = Uuid::new_v4();
        let sub = format!("user:{}", uuid);
        let result = parse_user_id(&sub);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), uuid);
    }

    #[test]
    fn test_parse_user_id_invalid() {
        let result = parse_user_id("invalid-uuid");
        assert!(result.is_err());
    }

    #[test]
    fn test_generate_guest_id() {
        let result = generate_guest_id();
        assert!(result.is_ok());

        let guest_id = result.unwrap();
        // Check it's a valid v4 UUID
        assert_eq!(guest_id.get_version_num(), 4);
    }

    #[test]
    fn test_generate_guest_id_uniqueness() {
        let id1 = generate_guest_id().unwrap();
        let id2 = generate_guest_id().unwrap();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_default_capabilities() {
        assert!(DEFAULT_PARTICIPANT_CAPABILITIES.contains(&"audio"));
        assert!(DEFAULT_PARTICIPANT_CAPABILITIES.contains(&"video"));
        assert!(DEFAULT_PARTICIPANT_CAPABILITIES.contains(&"screen_share"));
        assert!(DEFAULT_PARTICIPANT_CAPABILITIES.contains(&"chat"));
    }
}
