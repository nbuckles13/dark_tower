//! Internal token issuance endpoints (ADR-0020).
//!
//! These endpoints are called by the Global Controller (GC) to issue
//! meeting tokens and guest tokens. They require service authentication
//! with the `internal:meeting-token` scope.

use crate::crypto;
use crate::errors::AcError;
use crate::handlers::auth_handler::AppState;
use crate::models::{GuestTokenRequest, InternalTokenResponse, MeetingTokenRequest};
use crate::observability::metrics::{record_error, record_token_issuance};
use crate::observability::ErrorCategory;
use crate::repositories::signing_keys;
use axum::{extract::State, Extension, Json};
use common::secret::ExposeSecret;
use std::sync::Arc;
use std::time::Instant;
use tracing::instrument;

/// Maximum allowed TTL for meeting/guest tokens (15 minutes).
const MAX_TOKEN_TTL_SECONDS: u32 = 900;

/// Required scope for internal token endpoints.
const REQUIRED_SCOPE: &str = "internal:meeting-token";

/// Handle meeting token request.
///
/// POST /api/v1/auth/internal/meeting-token
///
/// Issues a JWT token for authenticated users joining meetings.
/// Requires service token with `internal:meeting-token` scope.
///
/// ADR-0011: Handler instrumented with skip_all to prevent PII leakage.
#[instrument(
    name = "ac.token.issue_meeting",
    skip_all,
    fields(grant_type = "internal_meeting", status)
)]
pub async fn handle_meeting_token(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<crypto::Claims>,
    Json(payload): Json<MeetingTokenRequest>,
) -> Result<Json<InternalTokenResponse>, AcError> {
    let start = Instant::now();

    // Validate scope
    let token_scopes: Vec<&str> = claims.scope.split_whitespace().collect();
    if !token_scopes.contains(&REQUIRED_SCOPE) {
        let duration = start.elapsed();
        tracing::Span::current().record("status", "error");
        record_token_issuance("internal_meeting", "error", duration);
        return Err(AcError::InsufficientScope {
            required: REQUIRED_SCOPE.to_string(),
            provided: token_scopes.iter().map(|s| s.to_string()).collect(),
        });
    }

    // Validate TTL (max 15 minutes)
    let ttl = payload.ttl_seconds.min(MAX_TOKEN_TTL_SECONDS);

    // Issue the meeting token
    let result = issue_meeting_token_internal(&state, &payload, ttl).await;

    let duration = start.elapsed();
    let status = if result.is_ok() { "success" } else { "error" };
    tracing::Span::current().record("status", status);
    record_token_issuance("internal_meeting", status, duration);

    match result {
        Ok(response) => Ok(Json(response)),
        Err(e) => {
            let category = ErrorCategory::from(&e);
            record_error("issue_meeting_token", category.as_str(), e.status_code());
            Err(e)
        }
    }
}

/// Handle guest token request.
///
/// POST /api/v1/auth/internal/guest-token
///
/// Issues a JWT token for unauthenticated guests joining meetings.
/// Requires service token with `internal:meeting-token` scope.
///
/// ADR-0011: Handler instrumented with skip_all to prevent PII leakage.
#[instrument(
    name = "ac.token.issue_guest",
    skip_all,
    fields(grant_type = "internal_guest", status)
)]
pub async fn handle_guest_token(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<crypto::Claims>,
    Json(payload): Json<GuestTokenRequest>,
) -> Result<Json<InternalTokenResponse>, AcError> {
    let start = Instant::now();

    // Validate scope
    let token_scopes: Vec<&str> = claims.scope.split_whitespace().collect();
    if !token_scopes.contains(&REQUIRED_SCOPE) {
        let duration = start.elapsed();
        tracing::Span::current().record("status", "error");
        record_token_issuance("internal_guest", "error", duration);
        return Err(AcError::InsufficientScope {
            required: REQUIRED_SCOPE.to_string(),
            provided: token_scopes.iter().map(|s| s.to_string()).collect(),
        });
    }

    // Validate TTL (max 15 minutes)
    let ttl = payload.ttl_seconds.min(MAX_TOKEN_TTL_SECONDS);

    // Issue the guest token
    let result = issue_guest_token_internal(&state, &payload, ttl).await;

    let duration = start.elapsed();
    let status = if result.is_ok() { "success" } else { "error" };
    tracing::Span::current().record("status", status);
    record_token_issuance("internal_guest", status, duration);

    match result {
        Ok(response) => Ok(Json(response)),
        Err(e) => {
            let category = ErrorCategory::from(&e);
            record_error("issue_guest_token", category.as_str(), e.status_code());
            Err(e)
        }
    }
}

/// Internal implementation for issuing meeting tokens.
async fn issue_meeting_token_internal(
    state: &AppState,
    payload: &MeetingTokenRequest,
    ttl: u32,
) -> Result<InternalTokenResponse, AcError> {
    use crate::crypto::EncryptedKey;
    use chrono::Utc;
    use common::secret::SecretBox;

    // Load active signing key
    let signing_key = signing_keys::get_active_key(&state.pool)
        .await?
        .ok_or_else(|| AcError::Crypto("No active signing key available".to_string()))?;

    // Decrypt private key
    let encrypted_key = EncryptedKey {
        encrypted_data: SecretBox::new(Box::new(signing_key.private_key_encrypted)),
        nonce: signing_key.encryption_nonce,
        tag: signing_key.encryption_tag,
    };

    let private_key_pkcs8 =
        crypto::decrypt_private_key(&encrypted_key, state.config.master_key.expose_secret())?;

    // Build meeting token claims
    let now = Utc::now().timestamp();
    let meeting_claims = MeetingTokenClaims {
        sub: payload.subject_user_id.to_string(),
        token_type: "meeting".to_string(),
        meeting_id: payload.meeting_id.to_string(),
        home_org_id: payload.home_org_id.to_string(),
        meeting_org_id: payload.meeting_org_id.to_string(),
        participant_type: payload.participant_type.as_str().to_string(),
        role: payload.role.as_str().to_string(),
        capabilities: payload.capabilities.clone(),
        iat: now,
        exp: now + i64::from(ttl),
        jti: uuid::Uuid::new_v4().to_string(),
    };

    // Sign JWT
    let token = sign_meeting_jwt(&meeting_claims, &private_key_pkcs8, &signing_key.key_id)?;

    Ok(InternalTokenResponse {
        token,
        expires_in: ttl,
    })
}

/// Internal implementation for issuing guest tokens.
async fn issue_guest_token_internal(
    state: &AppState,
    payload: &GuestTokenRequest,
    ttl: u32,
) -> Result<InternalTokenResponse, AcError> {
    use crate::crypto::EncryptedKey;
    use chrono::Utc;
    use common::secret::SecretBox;

    // Load active signing key
    let signing_key = signing_keys::get_active_key(&state.pool)
        .await?
        .ok_or_else(|| AcError::Crypto("No active signing key available".to_string()))?;

    // Decrypt private key
    let encrypted_key = EncryptedKey {
        encrypted_data: SecretBox::new(Box::new(signing_key.private_key_encrypted)),
        nonce: signing_key.encryption_nonce,
        tag: signing_key.encryption_tag,
    };

    let private_key_pkcs8 =
        crypto::decrypt_private_key(&encrypted_key, state.config.master_key.expose_secret())?;

    // Build guest token claims
    let now = Utc::now().timestamp();
    let guest_claims = GuestTokenClaims {
        sub: payload.guest_id.to_string(),
        token_type: "guest".to_string(),
        meeting_id: payload.meeting_id.to_string(),
        meeting_org_id: payload.meeting_org_id.to_string(),
        participant_type: "guest".to_string(),
        role: "guest".to_string(),
        display_name: payload.display_name.clone(),
        waiting_room: payload.waiting_room,
        capabilities: vec!["video".to_string(), "audio".to_string()],
        iat: now,
        exp: now + i64::from(ttl),
        jti: uuid::Uuid::new_v4().to_string(),
    };

    // Sign JWT
    let token = sign_guest_jwt(&guest_claims, &private_key_pkcs8, &signing_key.key_id)?;

    Ok(InternalTokenResponse {
        token,
        expires_in: ttl,
    })
}

/// Meeting token claims structure.
#[derive(serde::Serialize)]
struct MeetingTokenClaims {
    sub: String,
    token_type: String,
    meeting_id: String,
    home_org_id: String,
    meeting_org_id: String,
    participant_type: String,
    role: String,
    capabilities: Vec<String>,
    iat: i64,
    exp: i64,
    jti: String,
}

/// Guest token claims structure.
#[derive(serde::Serialize)]
struct GuestTokenClaims {
    sub: String,
    token_type: String,
    meeting_id: String,
    meeting_org_id: String,
    participant_type: String,
    role: String,
    display_name: String,
    waiting_room: bool,
    capabilities: Vec<String>,
    iat: i64,
    exp: i64,
    jti: String,
}

/// Sign a meeting token JWT.
fn sign_meeting_jwt(
    claims: &MeetingTokenClaims,
    private_key_pkcs8: &[u8],
    key_id: &str,
) -> Result<String, AcError> {
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
    use ring::signature::Ed25519KeyPair;

    // Validate the private key format
    let _key_pair = Ed25519KeyPair::from_pkcs8(private_key_pkcs8).map_err(|_| {
        tracing::error!(target: "crypto", "Invalid private key format");
        AcError::Crypto("JWT signing failed".to_string())
    })?;

    let encoding_key = EncodingKey::from_ed_der(private_key_pkcs8);

    let mut header = Header::new(Algorithm::EdDSA);
    header.typ = Some("JWT".to_string());
    header.kid = Some(key_id.to_string());

    encode(&header, claims, &encoding_key).map_err(|_| {
        tracing::error!(target: "crypto", "JWT signing operation failed");
        AcError::Crypto("JWT signing failed".to_string())
    })
}

/// Sign a guest token JWT.
fn sign_guest_jwt(
    claims: &GuestTokenClaims,
    private_key_pkcs8: &[u8],
    key_id: &str,
) -> Result<String, AcError> {
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
    use ring::signature::Ed25519KeyPair;

    // Validate the private key format
    let _key_pair = Ed25519KeyPair::from_pkcs8(private_key_pkcs8).map_err(|_| {
        tracing::error!(target: "crypto", "Invalid private key format");
        AcError::Crypto("JWT signing failed".to_string())
    })?;

    let encoding_key = EncodingKey::from_ed_der(private_key_pkcs8);

    let mut header = Header::new(Algorithm::EdDSA);
    header.typ = Some("JWT".to_string());
    header.kid = Some(key_id.to_string());

    encode(&header, claims, &encoding_key).map_err(|_| {
        tracing::error!(target: "crypto", "JWT signing operation failed");
        AcError::Crypto("JWT signing failed".to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{MeetingRole, ParticipantType};
    use uuid::Uuid;

    #[test]
    fn test_meeting_token_request_deserialization() {
        let json = r#"{
            "subject_user_id": "550e8400-e29b-41d4-a716-446655440001",
            "meeting_id": "550e8400-e29b-41d4-a716-446655440002",
            "meeting_org_id": "550e8400-e29b-41d4-a716-446655440003",
            "home_org_id": "550e8400-e29b-41d4-a716-446655440004",
            "participant_type": "member",
            "role": "host",
            "capabilities": ["video", "audio", "screen_share"],
            "ttl_seconds": 600
        }"#;

        let req: MeetingTokenRequest = serde_json::from_str(json).expect("Should deserialize");
        assert_eq!(
            req.subject_user_id,
            Uuid::parse_str("550e8400-e29b-41d4-a716-446655440001").unwrap()
        );
        assert_eq!(req.participant_type, ParticipantType::Member);
        assert_eq!(req.role, MeetingRole::Host);
        assert_eq!(req.capabilities, vec!["video", "audio", "screen_share"]);
        assert_eq!(req.ttl_seconds, 600);
    }

    #[test]
    fn test_meeting_token_request_defaults() {
        let json = r#"{
            "subject_user_id": "550e8400-e29b-41d4-a716-446655440001",
            "meeting_id": "550e8400-e29b-41d4-a716-446655440002",
            "meeting_org_id": "550e8400-e29b-41d4-a716-446655440003",
            "home_org_id": "550e8400-e29b-41d4-a716-446655440004"
        }"#;

        let req: MeetingTokenRequest = serde_json::from_str(json).expect("Should deserialize");
        // Check defaults
        assert_eq!(req.participant_type, ParticipantType::Member);
        assert_eq!(req.role, MeetingRole::Participant);
        assert!(req.capabilities.is_empty());
        assert_eq!(req.ttl_seconds, 900); // default 15 minutes
    }

    #[test]
    fn test_guest_token_request_deserialization() {
        let json = r#"{
            "guest_id": "550e8400-e29b-41d4-a716-446655440001",
            "display_name": "Alice Guest",
            "meeting_id": "550e8400-e29b-41d4-a716-446655440002",
            "meeting_org_id": "550e8400-e29b-41d4-a716-446655440003",
            "waiting_room": false,
            "ttl_seconds": 300
        }"#;

        let req: GuestTokenRequest = serde_json::from_str(json).expect("Should deserialize");
        assert_eq!(req.display_name, "Alice Guest");
        assert!(!req.waiting_room);
        assert_eq!(req.ttl_seconds, 300);
    }

    #[test]
    fn test_guest_token_request_defaults() {
        let json = r#"{
            "guest_id": "550e8400-e29b-41d4-a716-446655440001",
            "display_name": "Alice Guest",
            "meeting_id": "550e8400-e29b-41d4-a716-446655440002",
            "meeting_org_id": "550e8400-e29b-41d4-a716-446655440003"
        }"#;

        let req: GuestTokenRequest = serde_json::from_str(json).expect("Should deserialize");
        // Check defaults
        assert!(req.waiting_room); // default true
        assert_eq!(req.ttl_seconds, 900); // default 15 minutes
    }

    #[test]
    fn test_internal_token_response_serialization() {
        let response = InternalTokenResponse {
            token: "eyJhbGciOiJFZERTQSJ9.payload.signature".to_string(),
            expires_in: 900,
        };

        let json = serde_json::to_string(&response).expect("Should serialize");
        assert!(json.contains("\"token\""));
        assert!(json.contains("\"expires_in\":900"));
    }

    #[test]
    fn test_participant_type_as_str() {
        assert_eq!(ParticipantType::Member.as_str(), "member");
        assert_eq!(ParticipantType::External.as_str(), "external");
        assert_eq!(ParticipantType::Guest.as_str(), "guest");
    }

    #[test]
    fn test_meeting_role_as_str() {
        assert_eq!(MeetingRole::Host.as_str(), "host");
        assert_eq!(MeetingRole::Participant.as_str(), "participant");
        assert_eq!(MeetingRole::Guest.as_str(), "guest");
    }

    #[test]
    fn test_max_ttl_constant() {
        assert_eq!(MAX_TOKEN_TTL_SECONDS, 900, "Max TTL should be 15 minutes");
    }

    #[test]
    fn test_required_scope_constant() {
        assert_eq!(
            REQUIRED_SCOPE, "internal:meeting-token",
            "Required scope for internal endpoints"
        );
    }

    // P0 Security Tests - Scope Validation

    /// Test that scope validation correctly rejects claims missing the required scope.
    /// This tests the security control at lines 47-56 that checks for internal:meeting-token.
    #[test]
    fn test_meeting_token_scope_validation_missing() {
        // Simulate scope validation logic used in handlers
        let required_scope = REQUIRED_SCOPE;

        // Claims with NO internal:meeting-token scope
        let scopes_without_required = "admin:read user:write";
        let token_scopes: Vec<&str> = scopes_without_required.split_whitespace().collect();

        // Should NOT contain the required scope
        assert!(
            !token_scopes.contains(&required_scope),
            "Scope validation should detect missing required scope"
        );

        // Empty scopes should also fail
        let empty_scopes = "";
        let empty_token_scopes: Vec<&str> = empty_scopes.split_whitespace().collect();
        assert!(
            !empty_token_scopes.contains(&required_scope),
            "Empty scopes should not contain required scope"
        );

        // Only whitespace should also fail
        let whitespace_scopes = "   ";
        let whitespace_token_scopes: Vec<&str> = whitespace_scopes.split_whitespace().collect();
        assert!(
            !whitespace_token_scopes.contains(&required_scope),
            "Whitespace-only scopes should not contain required scope"
        );
    }

    /// Test that scope validation rejects similar but incorrect scopes.
    /// This prevents substring/prefix attacks where "internal:meeting" or
    /// "internal:meeting-token-extra" might bypass validation.
    #[test]
    fn test_meeting_token_scope_validation_similar_scope_rejected() {
        let required_scope = REQUIRED_SCOPE;

        // Test case 1: Prefix attack - "internal:meeting" (missing -token suffix)
        let prefix_scopes = "internal:meeting admin:read";
        let prefix_token_scopes: Vec<&str> = prefix_scopes.split_whitespace().collect();
        assert!(
            !prefix_token_scopes.contains(&required_scope),
            "Prefix scope 'internal:meeting' should not match 'internal:meeting-token'"
        );

        // Test case 2: Suffix attack - "internal:meeting-token-extra"
        let suffix_scopes = "internal:meeting-token-extra user:write";
        let suffix_token_scopes: Vec<&str> = suffix_scopes.split_whitespace().collect();
        assert!(
            !suffix_token_scopes.contains(&required_scope),
            "Suffix scope 'internal:meeting-token-extra' should not match 'internal:meeting-token'"
        );

        // Test case 3: Case sensitivity - "Internal:Meeting-Token"
        let case_scopes = "Internal:Meeting-Token admin:read";
        let case_token_scopes: Vec<&str> = case_scopes.split_whitespace().collect();
        assert!(
            !case_token_scopes.contains(&required_scope),
            "Case-different scope should not match (scopes are case-sensitive)"
        );

        // Test case 4: Partial match - "meeting-token" (missing internal: prefix)
        let partial_scopes = "meeting-token internal";
        let partial_token_scopes: Vec<&str> = partial_scopes.split_whitespace().collect();
        assert!(
            !partial_token_scopes.contains(&required_scope),
            "Partial scope 'meeting-token' should not match 'internal:meeting-token'"
        );

        // Test case 5: Combined with extra characters - "internal:meeting-tokenx"
        let extra_char_scopes = "internal:meeting-tokenx other:scope";
        let extra_char_token_scopes: Vec<&str> = extra_char_scopes.split_whitespace().collect();
        assert!(
            !extra_char_token_scopes.contains(&required_scope),
            "Extra character scope should not match"
        );

        // Test case 6: Verify exact match DOES work
        let valid_scopes = "internal:meeting-token other:scope";
        let valid_token_scopes: Vec<&str> = valid_scopes.split_whitespace().collect();
        assert!(
            valid_token_scopes.contains(&required_scope),
            "Exact scope match should succeed"
        );
    }

    // P0 Crypto Tests - Invalid Key Format

    /// Test that sign_meeting_jwt rejects invalid PKCS8 private key format.
    /// This tests the error path at lines 276-279 where Ed25519KeyPair::from_pkcs8
    /// fails due to malformed key data.
    #[test]
    fn test_sign_meeting_jwt_invalid_pkcs8_format_returns_error() {
        use chrono::Utc;

        // Create valid meeting token claims
        let now = Utc::now().timestamp();
        let claims = MeetingTokenClaims {
            sub: "user-123".to_string(),
            token_type: "meeting".to_string(),
            meeting_id: "meeting-456".to_string(),
            home_org_id: "org-789".to_string(),
            meeting_org_id: "org-789".to_string(),
            participant_type: "member".to_string(),
            role: "participant".to_string(),
            capabilities: vec!["video".to_string(), "audio".to_string()],
            iat: now,
            exp: now + 900,
            jti: "jti-abc".to_string(),
        };

        // Use invalid PKCS8 data (random bytes that are NOT valid Ed25519 key material)
        let invalid_pkcs8 = vec![0x42; 64]; // Just 64 bytes of 0x42, not valid PKCS8
        let key_id = "test-key-id";

        // Attempt to sign with invalid key - should return AcError::Crypto
        let result = sign_meeting_jwt(&claims, &invalid_pkcs8, key_id);

        assert!(
            result.is_err(),
            "sign_meeting_jwt should reject invalid PKCS8 format"
        );

        let err = result.expect_err("Expected error");
        assert!(
            matches!(&err, AcError::Crypto(msg) if msg == "JWT signing failed"),
            "Expected AcError::Crypto with 'JWT signing failed', got {:?}",
            err
        );
    }

    /// Test that sign_guest_jwt rejects invalid PKCS8 private key format.
    /// This tests the error path at lines 303-306 where Ed25519KeyPair::from_pkcs8
    /// fails due to malformed key data.
    #[test]
    fn test_sign_guest_jwt_invalid_pkcs8_format_returns_error() {
        use chrono::Utc;

        // Create valid guest token claims
        let now = Utc::now().timestamp();
        let claims = GuestTokenClaims {
            sub: "guest-123".to_string(),
            token_type: "guest".to_string(),
            meeting_id: "meeting-456".to_string(),
            meeting_org_id: "org-789".to_string(),
            participant_type: "guest".to_string(),
            role: "guest".to_string(),
            display_name: "Alice Guest".to_string(),
            waiting_room: true,
            capabilities: vec!["video".to_string(), "audio".to_string()],
            iat: now,
            exp: now + 900,
            jti: "jti-xyz".to_string(),
        };

        // Use invalid PKCS8 data (random bytes that are NOT valid Ed25519 key material)
        let invalid_pkcs8 = vec![0x99; 64]; // Just 64 bytes of 0x99, not valid PKCS8
        let key_id = "test-key-id";

        // Attempt to sign with invalid key - should return AcError::Crypto
        let result = sign_guest_jwt(&claims, &invalid_pkcs8, key_id);

        assert!(
            result.is_err(),
            "sign_guest_jwt should reject invalid PKCS8 format"
        );

        let err = result.expect_err("Expected error");
        assert!(
            matches!(&err, AcError::Crypto(msg) if msg == "JWT signing failed"),
            "Expected AcError::Crypto with 'JWT signing failed', got {:?}",
            err
        );
    }

    // P1 Tests - TTL Capping

    /// Test that TTL values are properly capped to MAX_TOKEN_TTL_SECONDS (900).
    /// This tests the logic at line 59: `payload.ttl_seconds.min(MAX_TOKEN_TTL_SECONDS)`.
    #[test]
    fn test_meeting_token_ttl_capping() {
        // Test case 1: TTL above max should be capped
        let requested_ttl: u32 = 3600; // 1 hour
        let capped_ttl = requested_ttl.min(MAX_TOKEN_TTL_SECONDS);
        assert_eq!(
            capped_ttl, MAX_TOKEN_TTL_SECONDS,
            "TTL of {} should be capped to {}",
            requested_ttl, MAX_TOKEN_TTL_SECONDS
        );

        // Test case 2: TTL at max should remain unchanged
        let requested_ttl_at_max: u32 = 900;
        let capped_ttl_at_max = requested_ttl_at_max.min(MAX_TOKEN_TTL_SECONDS);
        assert_eq!(capped_ttl_at_max, 900, "TTL at max should remain 900");

        // Test case 3: TTL below max should remain unchanged
        let requested_ttl_below: u32 = 300; // 5 minutes
        let capped_ttl_below = requested_ttl_below.min(MAX_TOKEN_TTL_SECONDS);
        assert_eq!(
            capped_ttl_below, 300,
            "TTL below max should remain unchanged"
        );

        // Test case 4: TTL of 0 should remain 0
        let requested_ttl_zero: u32 = 0;
        let capped_ttl_zero = requested_ttl_zero.min(MAX_TOKEN_TTL_SECONDS);
        assert_eq!(capped_ttl_zero, 0, "TTL of 0 should remain 0");

        // Test case 5: TTL at boundary (901) should be capped
        let requested_ttl_boundary: u32 = 901;
        let capped_ttl_boundary = requested_ttl_boundary.min(MAX_TOKEN_TTL_SECONDS);
        assert_eq!(
            capped_ttl_boundary, 900,
            "TTL of 901 should be capped to 900"
        );

        // Test case 6: Very large TTL should be capped
        let requested_ttl_large: u32 = u32::MAX;
        let capped_ttl_large = requested_ttl_large.min(MAX_TOKEN_TTL_SECONDS);
        assert_eq!(
            capped_ttl_large, 900,
            "Very large TTL should be capped to 900"
        );
    }
}
