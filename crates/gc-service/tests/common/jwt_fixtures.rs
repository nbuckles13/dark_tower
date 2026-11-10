//! Test JWT signing fixtures shared across `gc-service` integration tests.
//!
//! Consolidated under ADR-0032 Step 5 from inline copies in
//! `meeting_tests.rs:77-189`, `auth_tests.rs:58-108`,
//! `meeting_create_tests.rs:66-162`. Per @dry-reviewer + @team-lead
//! 2026-04-27 — keep-per-crate (not workspace-extracted) until topology
//! converges with AC/MC.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use ring::signature::{Ed25519KeyPair, KeyPair};
use serde::{Deserialize, Serialize};

// ============================================================================
// Claims types
// ============================================================================

/// User JWT Claims for test tokens (matches `common::jwt::UserClaims`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestUserClaims {
    pub sub: String,
    pub org_id: String,
    pub email: String,
    pub roles: Vec<String>,
    pub iat: i64,
    pub exp: i64,
    pub jti: String,
}

/// Service JWT Claims (for testing wrong token type, ADR-0003 layer-2 paths).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestServiceClaims {
    pub sub: String,
    pub exp: i64,
    pub iat: i64,
    pub scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_type: Option<String>,
}

/// MC-minted `media_dial` claims exactly as ADR-0039 §5 specifies them
/// ({token_type, sub, meeting_id, aud, iat, exp}). Never valid at GC: GC
/// verifies only AC-signed tokens (R-15).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestMediaDialClaims {
    pub token_type: String,
    pub sub: String,
    pub meeting_id: String,
    pub aud: String,
    pub iat: i64,
    pub exp: i64,
}

/// `media_dial` claims UNION a complete user-claim set, WITHOUT `aud`. Used
/// by the R-15 signature/alg arms so the payload would be ACCEPTED if signed
/// by the live AC key: the signing key (or alg) is then the only possible
/// reason for rejection (`common::jwt` maps claim-shape failures to the same
/// `InvalidSignature`). `aud` is left out because GC's validator also rejects
/// any token carrying an `aud` claim (jsonwebtoken validates `aud` with no
/// expected audience configured), which would over-determine the rejection;
/// the literal [`TestMediaDialClaims`] arm keeps it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestMediaDialUserShapedClaims {
    #[serde(flatten)]
    pub user: TestUserClaims,
    pub token_type: String,
    pub meeting_id: String,
}

impl TestMediaDialClaims {
    /// A live (valid iat/exp) media_dial claim set.
    pub fn live(now: i64) -> Self {
        Self {
            token_type: "media_dial".to_string(),
            sub: "participant-1".to_string(),
            meeting_id: "meeting-1".to_string(),
            aud: "mh-1".to_string(),
            iat: now,
            exp: now + 300,
        }
    }
}

impl TestMediaDialUserShapedClaims {
    /// A live user-shaped media_dial claim set.
    pub fn live(now: i64) -> Self {
        let dial = TestMediaDialClaims::live(now);
        Self {
            user: TestUserClaims {
                sub: dial.sub,
                org_id: "org-1".to_string(),
                email: "p1@example.test".to_string(),
                roles: vec!["user".to_string()],
                iat: dial.iat,
                exp: dial.exp,
                jti: "jti-1".to_string(),
            },
            token_type: dial.token_type,
            meeting_id: dial.meeting_id,
        }
    }
}

/// Sign `claims` with HS256 under `secret`, `kid` in the header: the
/// algorithm-confusion attack (an HMAC keyed with the PUBLIC key's bytes).
/// Test-only; GC must reject it at its EdDSA pin.
pub fn sign_hs256<T: Serialize>(secret: &[u8], kid: &str, claims: &T) -> String {
    let mut header = Header::new(Algorithm::HS256);
    header.typ = Some("JWT".to_string());
    header.kid = Some(kid.to_string());
    encode(&header, claims, &EncodingKey::from_secret(secret)).expect("sign HS256")
}

// ============================================================================
// Keypair
// ============================================================================

/// Deterministic Ed25519 test keypair. Use a stable `seed: u8` per fixture
/// site so that tests asserting on `kid` cross-references stay stable.
pub struct TestKeypair {
    pub kid: String,
    pub public_key_bytes: Vec<u8>,
    pub private_key_pkcs8: Vec<u8>,
}

impl TestKeypair {
    pub fn new(seed: u8, kid: &str) -> Self {
        let mut seed_bytes = [0u8; 32];
        seed_bytes[0] = seed;
        for (i, byte) in seed_bytes.iter_mut().enumerate().skip(1) {
            *byte = seed.wrapping_mul(i as u8).wrapping_add(i as u8);
        }

        let key_pair = Ed25519KeyPair::from_seed_unchecked(&seed_bytes)
            .expect("Failed to create test keypair");

        let public_key_bytes = key_pair.public_key().as_ref().to_vec();
        let private_key_pkcs8 = build_pkcs8_from_seed(&seed_bytes);

        Self {
            kid: kid.to_string(),
            public_key_bytes,
            private_key_pkcs8,
        }
    }

    pub fn sign_user_token(&self, claims: &TestUserClaims) -> String {
        self.sign_claims(claims, Some(&self.kid))
    }

    pub fn sign_service_token(&self, claims: &TestServiceClaims) -> String {
        self.sign_claims(claims, Some(&self.kid))
    }

    /// Sign arbitrary claims with EdDSA; `kid = None` omits the header's kid
    /// (the header is then exactly `{"alg":"EdDSA","typ":"JWT"}`, as MC mints
    /// `media_dial` tokens).
    pub fn sign_claims<T: Serialize>(&self, claims: &T, kid: Option<&str>) -> String {
        let encoding_key = EncodingKey::from_ed_der(&self.private_key_pkcs8);
        let mut header = Header::new(Algorithm::EdDSA);
        header.typ = Some("JWT".to_string());
        header.kid = kid.map(str::to_string);
        encode(&header, claims, &encoding_key).expect("Failed to sign claims")
    }

    pub fn jwk_json(&self) -> serde_json::Value {
        serde_json::json!({
            "kty": "OKP",
            "kid": self.kid,
            "crv": "Ed25519",
            "x": URL_SAFE_NO_PAD.encode(&self.public_key_bytes),
            "alg": "EdDSA",
            "use": "sig"
        })
    }
}

/// Build a minimal Ed25519 PKCS#8 v1 envelope from a 32-byte seed.
///
/// This is the test-fixture twin of the production `pkcs8` crate's encoder —
/// kept here because adding a `pkcs8` dep solely for test signing widens the
/// crate's dep graph for no production gain.
pub fn build_pkcs8_from_seed(seed: &[u8; 32]) -> Vec<u8> {
    let mut pkcs8 = Vec::new();
    pkcs8.push(0x30);
    pkcs8.push(0x2e);
    pkcs8.extend_from_slice(&[0x02, 0x01, 0x00]);
    pkcs8.push(0x30);
    pkcs8.push(0x05);
    pkcs8.extend_from_slice(&[0x06, 0x03, 0x2b, 0x65, 0x70]);
    pkcs8.push(0x04);
    pkcs8.push(0x22);
    pkcs8.push(0x04);
    pkcs8.push(0x20);
    pkcs8.extend_from_slice(seed);
    pkcs8
}
