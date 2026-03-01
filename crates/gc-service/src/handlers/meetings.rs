//! Meeting handlers for Global Controller.
//!
//! Implements meeting endpoints:
//!
//! - `POST /api/v1/meetings` - Create meeting (user authenticated)
//! - `GET /api/v1/meetings/{code}` - Join meeting (service authenticated)
//! - `POST /api/v1/meetings/{code}/guest-token` - Get guest token (public)
//! - `PATCH /api/v1/meetings/{id}/settings` - Update meeting settings (service authenticated)
//!
//! # Security
//!
//! - User-authenticated endpoints validate JWT with UserClaims (org_id, roles)
//! - Service-authenticated endpoints validate JWT with Claims (scope)
//! - Guest endpoint is public but rate limited (5 req/min per IP)
//! - Meeting codes and secrets generated using CSPRNG
//! - Error messages are generic to prevent information leakage

use crate::auth::Claims;
use crate::errors::GcError;
use crate::models::{
    CreateMeetingRequest, CreateMeetingResponse, GuestJoinRequest, JoinMeetingResponse,
    McAssignmentInfo, MeetingResponse, MeetingRow, UpdateMeetingSettingsRequest,
    DEFAULT_MAX_PARTICIPANTS, MIN_PARTICIPANTS,
};
use crate::observability::metrics;
use crate::repositories::{map_row_to_meeting, MeetingsRepository};
use crate::routes::AppState;
use crate::services::ac_client::{
    AcClient, GuestTokenRequest, MeetingRole, MeetingTokenRequest, ParticipantType,
};
use crate::services::McAssignmentService;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use common::jwt::UserClaims;
use ring::rand::{SecureRandom, SystemRandom};
use sqlx::{PgPool, Row};
use std::sync::Arc;
use std::time::Instant;
use tracing::{info, instrument, warn};
use uuid::Uuid;

/// Default token TTL in seconds (15 minutes).
const DEFAULT_TOKEN_TTL_SECONDS: u32 = 900;

/// Default capabilities for meeting participants.
const DEFAULT_PARTICIPANT_CAPABILITIES: &[&str] = &["audio", "video", "screen_share", "chat"];

/// Roles allowed to create meetings (R-3).
const MEETING_CREATE_ROLES: &[&str] = &["user", "admin", "org_admin"];

/// Base62 alphabet for meeting code generation.
const BASE62_CHARS: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// Length of generated meeting codes.
const MEETING_CODE_LENGTH: usize = 12;

/// Number of random bytes for meeting code generation (72 bits entropy).
const MEETING_CODE_RANDOM_BYTES: usize = 9;

/// Maximum collision retries for meeting code generation.
const MAX_CODE_COLLISION_RETRIES: usize = 3;

/// Length of join token secret in bytes (256 bits).
const JOIN_TOKEN_SECRET_BYTES: usize = 32;

// ============================================================================
// Handler: POST /api/v1/meetings
// ============================================================================

/// Handler for POST /api/v1/meetings
///
/// Create a new meeting in the authenticated user's organization.
///
/// # Authorization
///
/// - Requires valid user JWT (via `require_user_auth` middleware)
/// - User must have at least one of: user, admin, org_admin roles
///
/// # Response
///
/// - 201 Created: Meeting created successfully
/// - 400 Bad Request: Invalid request body
/// - 401 Unauthorized: Invalid or missing user token
/// - 403 Forbidden: Insufficient role or org meeting limit exceeded
/// - 500 Internal Server Error: Meeting code collision or database error
#[instrument(
    skip_all,
    name = "gc.meeting.create",
    fields(
        method = "POST",
        endpoint = "/api/v1/meetings",
        status = tracing::field::Empty,
    )
)]
pub async fn create_meeting(
    State(state): State<Arc<AppState>>,
    Extension(user_claims): Extension<UserClaims>,
    body: axum::body::Bytes,
) -> Result<(StatusCode, Json<CreateMeetingResponse>), GcError> {
    let start = Instant::now();

    // Deserialize request body manually to return 400 (not Axum's default 422)
    let request: CreateMeetingRequest = serde_json::from_slice(&body).map_err(|e| {
        tracing::debug!(target: "gc.handlers.meetings", error = %e, "Invalid request body");
        let duration = start.elapsed();
        metrics::record_meeting_creation("error", Some("bad_request"), duration);
        GcError::BadRequest("Invalid request body".to_string())
    })?;

    // 1. Validate role (R-3)
    let has_required_role = user_claims
        .roles
        .iter()
        .any(|r| MEETING_CREATE_ROLES.contains(&r.as_str()));

    if !has_required_role {
        let duration = start.elapsed();
        metrics::record_meeting_creation("error", Some("forbidden"), duration);
        warn!(
            target: "gc.handlers.meetings",
            user_id = %user_claims.sub,
            roles = ?user_claims.roles,
            "User lacks required role for meeting creation"
        );
        return Err(GcError::Forbidden(
            "Insufficient permissions to create meetings".to_string(),
        ));
    }

    // 2. Validate request (R-7)
    request.validate().map_err(|e| {
        let duration = start.elapsed();
        metrics::record_meeting_creation("error", Some("bad_request"), duration);
        GcError::BadRequest(e.to_string())
    })?;

    // 3. Parse user ID and org ID from claims
    let user_id = parse_user_id(&user_claims.sub).inspect_err(|_| {
        let duration = start.elapsed();
        metrics::record_meeting_creation("error", Some("unauthorized"), duration);
    })?;

    let org_id = Uuid::parse_str(&user_claims.org_id).map_err(|e| {
        tracing::debug!(target: "gc.handlers.meetings", error = %e, "Failed to parse org_id from token");
        let duration = start.elapsed();
        metrics::record_meeting_creation("error", Some("unauthorized"), duration);
        GcError::InvalidToken("Invalid organization identifier in token".to_string())
    })?;

    // 4. Apply secure defaults (R-7)
    let display_name = request.display_name.trim().to_string();
    let max_participants = request.max_participants.unwrap_or(DEFAULT_MAX_PARTICIPANTS);
    let enable_e2e_encryption = request.enable_e2e_encryption.unwrap_or(true);
    let require_auth = request.require_auth.unwrap_or(true);
    let recording_enabled = request.recording_enabled.unwrap_or(false);
    let allow_guests = request.allow_guests.unwrap_or(false);
    let allow_external_participants = request.allow_external_participants.unwrap_or(false);
    let waiting_room_enabled = request.waiting_room_enabled.unwrap_or(true);

    // 5. Validate max_participants lower bound
    if max_participants < MIN_PARTICIPANTS {
        let duration = start.elapsed();
        metrics::record_meeting_creation("error", Some("bad_request"), duration);
        return Err(GcError::BadRequest(
            "Maximum participants must be at least 2".to_string(),
        ));
    }

    // 6. Generate join_token_secret (R-5: 256 bits CSPRNG, hex-encoded)
    let join_token_secret = generate_join_token_secret().inspect_err(|_| {
        let duration = start.elapsed();
        metrics::record_meeting_creation("error", Some("internal"), duration);
    })?;

    // 7. Generate meeting code and create meeting with collision retry (R-4, R-6)
    let mut meeting_row = None;
    for attempt in 0..MAX_CODE_COLLISION_RETRIES {
        let meeting_code = generate_meeting_code().inspect_err(|_| {
            let duration = start.elapsed();
            metrics::record_meeting_creation("error", Some("internal"), duration);
        })?;

        match MeetingsRepository::create_meeting_with_limit_check(
            &state.pool,
            org_id,
            user_id,
            &display_name,
            &meeting_code,
            &join_token_secret,
            max_participants,
            enable_e2e_encryption,
            require_auth,
            recording_enabled,
            allow_guests,
            allow_external_participants,
            waiting_room_enabled,
            request.scheduled_start_time,
        )
        .await
        {
            Ok(Some(row)) => {
                meeting_row = Some(row);
                break;
            }
            Ok(None) => {
                // Org limit exceeded (R-6)
                let duration = start.elapsed();
                metrics::record_meeting_creation("error", Some("forbidden"), duration);
                return Err(GcError::Forbidden(
                    "Organization meeting limit exceeded".to_string(),
                ));
            }
            Err(GcError::Database(ref e))
                if e.contains("unique constraint") || e.contains("duplicate key") =>
            {
                // Meeting code collision — retry with new code
                tracing::debug!(
                    target: "gc.handlers.meetings",
                    attempt = attempt + 1,
                    "Meeting code collision, retrying"
                );
                if attempt == MAX_CODE_COLLISION_RETRIES - 1 {
                    let duration = start.elapsed();
                    metrics::record_meeting_creation("error", Some("code_collision"), duration);
                    return Err(GcError::Internal(
                        "Failed to generate unique meeting code".to_string(),
                    ));
                }
            }
            Err(e) => {
                let duration = start.elapsed();
                metrics::record_meeting_creation("error", Some("db_error"), duration);
                return Err(e);
            }
        }
    }

    let row = meeting_row.ok_or_else(|| {
        let duration = start.elapsed();
        metrics::record_meeting_creation("error", Some("code_collision"), duration);
        GcError::Internal("Failed to generate unique meeting code".to_string())
    })?;

    // 8. Fire-and-forget audit log (R-9)
    if let Err(e) =
        MeetingsRepository::log_audit_event(&state.pool, org_id, user_id, row.meeting_id).await
    {
        warn!(
            target: "gc.handlers.meetings",
            meeting_id = %row.meeting_id,
            error = %e,
            "Failed to log audit event for meeting creation"
        );
    }

    // 9. Record success metrics (R-10)
    let duration = start.elapsed();
    metrics::record_meeting_creation("success", None, duration);

    info!(
        target: "gc.handlers.meetings",
        meeting_id = %row.meeting_id,
        meeting_code = %row.meeting_code,
        user_id = %user_id,
        org_id = %org_id,
        "Meeting created successfully"
    );

    // 10. Return 201 Created (R-1, R-8)
    Ok((StatusCode::CREATED, Json(CreateMeetingResponse::from(row))))
}

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
#[instrument(
    skip_all,
    name = "gc.meeting.join",
    fields(
        method = "GET",
        endpoint = "/api/v1/meetings/{code}",
        status = tracing::field::Empty,
    )
)]
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

    // Assign meeting to MC with MH selection (ADR-0010 Section 4a)
    let assignment_with_mh = McAssignmentService::assign_meeting_with_mh(
        &state.pool,
        state.mc_client.clone(),
        &meeting.meeting_id.to_string(),
        &state.config.region,
        &state.config.gc_id,
    )
    .await?;

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
        mc_id = %assignment_with_mh.mc_assignment.mc_id,
        primary_mh_id = %assignment_with_mh.mh_selection.primary.mh_id,
        participant_type = ?participant_type,
        "User joined meeting"
    );

    Ok(Json(JoinMeetingResponse {
        token: token_response.token,
        expires_in: token_response.expires_in,
        meeting_id: meeting.meeting_id,
        meeting_name: meeting.display_name,
        mc_assignment: McAssignmentInfo {
            mc_id: assignment_with_mh.mc_assignment.mc_id,
            webtransport_endpoint: assignment_with_mh.mc_assignment.webtransport_endpoint,
            grpc_endpoint: assignment_with_mh.mc_assignment.grpc_endpoint,
        },
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
#[instrument(
    skip_all,
    name = "gc.meeting.guest_token",
    fields(
        method = "POST",
        endpoint = "/api/v1/meetings/{code}/guest-token",
        status = tracing::field::Empty,
    )
)]
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

    // Assign meeting to MC with MH selection (ADR-0010 Section 4a)
    let assignment_with_mh = McAssignmentService::assign_meeting_with_mh(
        &state.pool,
        state.mc_client.clone(),
        &meeting.meeting_id.to_string(),
        &state.config.region,
        &state.config.gc_id,
    )
    .await?;

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
        mc_id = %assignment_with_mh.mc_assignment.mc_id,
        primary_mh_id = %assignment_with_mh.mh_selection.primary.mh_id,
        waiting_room = meeting.waiting_room_enabled,
        "Guest joined meeting"
    );

    Ok(Json(JoinMeetingResponse {
        token: token_response.token,
        expires_in: token_response.expires_in,
        meeting_id: meeting.meeting_id,
        meeting_name: meeting.display_name,
        mc_assignment: McAssignmentInfo {
            mc_id: assignment_with_mh.mc_assignment.mc_id,
            webtransport_endpoint: assignment_with_mh.mc_assignment.webtransport_endpoint,
            grpc_endpoint: assignment_with_mh.mc_assignment.grpc_endpoint,
        },
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
#[instrument(
    skip_all,
    name = "gc.meeting.update_settings",
    fields(
        method = "PATCH",
        endpoint = "/api/v1/meetings/{id}/settings",
        status = tracing::field::Empty,
    )
)]
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

    Ok(map_row_to_meeting(row))
}

/// Find a meeting by its ID.
async fn find_meeting_by_id(pool: &PgPool, meeting_id: Uuid) -> Result<MeetingRow, GcError> {
    let query = format!("{} WHERE meeting_id = $1", MEETING_SELECT_QUERY);

    let row = sqlx::query(&query)
        .bind(meeting_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| GcError::NotFound("Meeting not found".to_string()))?;

    Ok(map_row_to_meeting(row))
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

    Ok(map_row_to_meeting(row))
}

// ============================================================================
// Utility Helpers
// ============================================================================

/// Parse user ID from JWT subject.
///
/// Supports both plain UUID and "user:{uuid}" formats.
fn parse_user_id(sub: &str) -> Result<Uuid, GcError> {
    let uuid_str = sub.strip_prefix("user:").unwrap_or(sub);
    Uuid::parse_str(uuid_str).map_err(|e| {
        tracing::debug!(target: "gc.handlers.meetings", error = %e, "Failed to parse user ID from token");
        GcError::InvalidToken("Invalid user identifier in token".to_string())
    })
}

/// Generate a cryptographically secure guest ID.
fn generate_guest_id() -> Result<Uuid, GcError> {
    let rng = SystemRandom::new();
    let mut bytes = [0u8; 16];

    rng.fill(&mut bytes).map_err(|e| {
        tracing::error!(target: "gc.handlers.meetings", error = %e, "Failed to generate random bytes");
        GcError::Internal(format!("RNG failure: {}", e))
    })?;

    // Set UUID version 4 and variant bits
    bytes[6] = (bytes[6] & 0x0f) | 0x40; // Version 4
    bytes[8] = (bytes[8] & 0x3f) | 0x80; // Variant 1

    Ok(Uuid::from_bytes(bytes))
}

/// Create AC client with configuration from state.
fn create_ac_client(state: &AppState) -> Result<AcClient, GcError> {
    AcClient::new(
        state.config.ac_internal_url.clone(),
        state.token_receiver.clone(),
    )
}

/// Generate a cryptographically secure meeting code.
///
/// Produces 12 base62 characters (72 bits entropy) using CSPRNG.
/// Always returns exactly `MEETING_CODE_LENGTH` characters, left-padded
/// with '0' if the random value produces fewer digits.
fn generate_meeting_code() -> Result<String, GcError> {
    let rng = SystemRandom::new();
    let mut bytes = [0u8; MEETING_CODE_RANDOM_BYTES];

    rng.fill(&mut bytes).map_err(|e| {
        tracing::error!(target: "gc.handlers.meetings", error = %e, "Failed to generate random bytes for meeting code");
        GcError::Internal("RNG failure".to_string())
    })?;

    // Convert bytes to a big integer (u128 can hold 9 bytes = 72 bits)
    let mut value: u128 = 0;
    for &b in &bytes {
        value = (value << 8) | u128::from(b);
    }

    // Encode as base62, extracting digits from least-significant end
    let mut code = Vec::with_capacity(MEETING_CODE_LENGTH);
    for _ in 0..MEETING_CODE_LENGTH {
        let idx = (value % 62) as usize;
        let ch = BASE62_CHARS
            .get(idx)
            .ok_or_else(|| GcError::Internal("Base62 index out of range".to_string()))?;
        code.push(*ch);
        value /= 62;
    }

    // Reverse to get most-significant digit first (consistent ordering)
    code.reverse();

    String::from_utf8(code)
        .map_err(|_| GcError::Internal("Meeting code contained invalid UTF-8".to_string()))
}

/// Generate a cryptographically secure join token secret.
///
/// Produces 32 random bytes (256 bits) hex-encoded to 64 characters.
fn generate_join_token_secret() -> Result<String, GcError> {
    let rng = SystemRandom::new();
    let mut bytes = [0u8; JOIN_TOKEN_SECRET_BYTES];

    rng.fill(&mut bytes).map_err(|e| {
        tracing::error!(target: "gc.handlers.meetings", error = %e, "Failed to generate random bytes for join token secret");
        GcError::Internal("RNG failure".to_string())
    })?;

    Ok(hex::encode(bytes))
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

    // ========================================================================
    // Meeting Code Generation Tests
    // ========================================================================

    #[test]
    fn test_generate_meeting_code_format() {
        let code = generate_meeting_code().unwrap();
        assert_eq!(
            code.len(),
            MEETING_CODE_LENGTH,
            "Meeting code must be exactly {} chars",
            MEETING_CODE_LENGTH
        );

        // All characters must be base62 (0-9, A-Z, a-z)
        for ch in code.chars() {
            assert!(
                ch.is_ascii_alphanumeric(),
                "Meeting code char '{}' is not base62",
                ch
            );
        }
    }

    #[test]
    fn test_generate_meeting_code_uniqueness() {
        let code1 = generate_meeting_code().unwrap();
        let code2 = generate_meeting_code().unwrap();
        assert_ne!(code1, code2, "Two generated codes should differ");
    }

    #[test]
    fn test_generate_meeting_code_always_12_chars() {
        // Generate many codes to verify padding works even when
        // random bytes produce small values (leading zeros)
        for _ in 0..100 {
            let code = generate_meeting_code().unwrap();
            assert_eq!(code.len(), 12);
        }
    }

    // ========================================================================
    // Join Token Secret Generation Tests
    // ========================================================================

    #[test]
    fn test_generate_join_token_secret_format() {
        let secret = generate_join_token_secret().unwrap();

        // 32 bytes hex-encoded = 64 hex characters
        assert_eq!(secret.len(), 64, "Join token secret must be 64 hex chars");

        // All characters must be valid hex
        for ch in secret.chars() {
            assert!(
                ch.is_ascii_hexdigit(),
                "Join token secret char '{}' is not hex",
                ch
            );
        }
    }

    #[test]
    fn test_generate_join_token_secret_uniqueness() {
        let secret1 = generate_join_token_secret().unwrap();
        let secret2 = generate_join_token_secret().unwrap();
        assert_ne!(secret1, secret2, "Two generated secrets should differ");
    }

    // ========================================================================
    // Role Validation Tests
    // ========================================================================

    #[test]
    fn test_meeting_create_roles_user() {
        let roles = ["user".to_string()];
        assert!(roles
            .iter()
            .any(|r| MEETING_CREATE_ROLES.contains(&r.as_str())));
    }

    #[test]
    fn test_meeting_create_roles_admin() {
        let roles = ["admin".to_string()];
        assert!(roles
            .iter()
            .any(|r| MEETING_CREATE_ROLES.contains(&r.as_str())));
    }

    #[test]
    fn test_meeting_create_roles_org_admin() {
        let roles = ["org_admin".to_string()];
        assert!(roles
            .iter()
            .any(|r| MEETING_CREATE_ROLES.contains(&r.as_str())));
    }

    #[test]
    fn test_meeting_create_roles_viewer_rejected() {
        let roles = ["viewer".to_string()];
        assert!(!roles
            .iter()
            .any(|r| MEETING_CREATE_ROLES.contains(&r.as_str())));
    }

    #[test]
    fn test_meeting_create_roles_empty_rejected() {
        let roles: Vec<String> = vec![];
        assert!(!roles
            .iter()
            .any(|r| MEETING_CREATE_ROLES.contains(&r.as_str())));
    }
}
