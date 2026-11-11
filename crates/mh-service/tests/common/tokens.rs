//! Helpers for minting test JWTs: service tokens, AC meeting tokens (which MH
//! client admission must DECLINE), and `media_dial` tokens through the ONE
//! forge (`mh_test_utils::media_dial`).

use chrono::Utc;
use common::jwt::{MeetingRole, MeetingTokenClaims, ParticipantType, ServiceClaims};

use super::TestKeypair;

/// Required scope for MC → MH gRPC calls (matches `auth_interceptor.rs`).
pub const MC_SCOPE: &str = "service.write.mh";

/// Expected caller `service_type` claim for MC → MH calls.
pub const MC_SERVICE_TYPE: &str = "meeting-controller";

/// Mint a valid MC → MH service token signed by `keypair`.
pub fn mint_valid_mc_token(keypair: &TestKeypair) -> String {
    let now = Utc::now().timestamp();
    let claims = ServiceClaims::new(
        "mc-service-test".to_string(),
        now + 3600,
        now,
        MC_SCOPE.to_string(),
        Some(MC_SERVICE_TYPE.to_string()),
    );
    keypair.sign_token(&claims)
}

/// Mint an expired MC → MH service token (`exp = now - 3600`).
pub fn mint_expired_mc_token(keypair: &TestKeypair) -> String {
    let now = Utc::now().timestamp();
    let claims = ServiceClaims::new(
        "mc-service-test".to_string(),
        now - 3600,
        now - 7200,
        MC_SCOPE.to_string(),
        Some(MC_SERVICE_TYPE.to_string()),
    );
    keypair.sign_token(&claims)
}

/// Mint a service token that authenticates but declares a different caller type
/// (e.g., `global-controller` — used to probe Layer 2 routing).
pub fn mint_wrong_service_type_token(keypair: &TestKeypair, service_type: &str) -> String {
    let now = Utc::now().timestamp();
    let claims = ServiceClaims::new(
        "gc-service-test".to_string(),
        now + 3600,
        now,
        MC_SCOPE.to_string(),
        Some(service_type.to_string()),
    );
    keypair.sign_token(&claims)
}

/// Mint a service token with no `service_type` claim (hits the `unwrap_or("unknown")`
/// fail-closed branch in Layer 2).
pub fn mint_no_service_type_token(keypair: &TestKeypair) -> String {
    let now = Utc::now().timestamp();
    let claims = ServiceClaims::new(
        "anonymous-service".to_string(),
        now + 3600,
        now,
        MC_SCOPE.to_string(),
        None,
    );
    keypair.sign_token(&claims)
}

/// The ONE meeting-token claim set for these tests: valid for an hour from
/// now, for `participant_id` in `meeting_id`, with the given `token_type`.
/// Callers that need something else (expiry, a different type) adjust fields.
pub fn meeting_claims(
    meeting_id: &str,
    participant_id: &str,
    token_type: &str,
) -> MeetingTokenClaims {
    let now = Utc::now().timestamp();
    MeetingTokenClaims {
        sub: participant_id.to_string(),
        token_type: token_type.to_string(),
        meeting_id: meeting_id.to_string(),
        home_org_id: None,
        meeting_org_id: "org-test".to_string(),
        participant_type: ParticipantType::Member,
        role: MeetingRole::Participant,
        capabilities: vec!["video".to_string(), "audio".to_string()],
        display_name: "Test Participant".to_string(),
        iat: now,
        exp: now + 3600,
        jti: format!("jti-{}", uuid::Uuid::new_v4()),
    }
}

/// Mint a standard meeting token for a participant joining `meeting_id`.
pub fn mint_meeting_token(keypair: &TestKeypair, meeting_id: &str, participant_id: &str) -> String {
    keypair.sign_token(&meeting_claims(meeting_id, participant_id, "meeting"))
}

/// Mint an expired meeting token.
pub fn mint_expired_meeting_token(keypair: &TestKeypair, meeting_id: &str) -> String {
    let past = Utc::now().timestamp() - 7200;
    let mut claims = meeting_claims(meeting_id, "user-expired", "meeting");
    claims.iat = past;
    claims.exp = past + 3600;
    keypair.sign_token(&claims)
}

/// Mint a meeting-shaped token whose `token_type` is NOT `"meeting"`.
///
/// An AC-signed token of another type: still never a `media_dial` credential.
pub fn mint_wrong_token_type_token(keypair: &TestKeypair, meeting_id: &str) -> String {
    keypair.sign_token(&meeting_claims(meeting_id, "user-guest", "guest"))
}

/// Hand-craft an `alg: none` unsigned JWT for the given claims payload.
///
/// Shape: `base64url({"alg":"none","typ":"JWT","kid":"<kid>"}).base64url(claims).`
/// Trailing empty signature. Targets the CVE-2015-9235-class bypass.
pub fn craft_alg_none_token(kid: &str, claims: &impl serde::Serialize) -> String {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

    let header = serde_json::json!({
        "alg": "none",
        "typ": "JWT",
        "kid": kid,
    });
    let header_b64 = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header).unwrap());
    let claims_b64 = URL_SAFE_NO_PAD.encode(serde_json::to_vec(claims).unwrap());
    format!("{header_b64}.{claims_b64}.")
}

/// Sign claims with HS256 using the JWKS public key bytes as the HMAC secret.
///
/// Classic algorithm-confusion attack: attacker substitutes the public key
/// as a symmetric secret. JWKS-pinned EdDSA validators must reject this.
pub fn craft_hs256_key_confusion_token(
    keypair: &TestKeypair,
    claims: &impl serde::Serialize,
) -> String {
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};

    let mut header = Header::new(Algorithm::HS256);
    header.kid = Some(keypair.kid.clone());
    let key = EncodingKey::from_secret(&keypair.public_key_bytes);
    encode(&header, claims, &key).expect("failed to sign HS256 confusion token")
}

/// TTL of the `media_dial` tokens these tests mint (MC's default).
pub const DIAL_TTL_SECS: i64 = 300;

/// A `media_dial` token for `participant_id` in `meeting_id`, addressed to
/// `handler_id`, signed by the key every test registration carries by default
/// (`mh_test_utils::media_dial::MEETING_SEED`).
pub fn mint_dial_token(meeting_id: &str, participant_id: &str, handler_id: &str) -> String {
    mint_aged_dial_token(meeting_id, participant_id, handler_id, 0)
}

/// [`mint_dial_token`] issued `age_secs` ago (so it expired `age_secs -
/// DIAL_TTL_SECS` seconds ago when that is positive).
pub fn mint_aged_dial_token(
    meeting_id: &str,
    participant_id: &str,
    handler_id: &str,
    age_secs: i64,
) -> String {
    use mh_test_utils::media_dial::{DialForge, MEETING_SEED};
    DialForge::new(MEETING_SEED).sign(&DialForge::claims_json(
        meeting_id,
        handler_id,
        participant_id,
        Utc::now().timestamp() - age_secs,
        DIAL_TTL_SECS,
    ))
}
