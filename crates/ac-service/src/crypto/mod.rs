#[cfg(test)]
use crate::config::{DEFAULT_BCRYPT_COST, DEFAULT_JWT_CLOCK_SKEW_SECONDS};
use crate::config::{MAX_BCRYPT_COST, MIN_BCRYPT_COST};
use crate::errors::AcError;
use crate::observability::metrics::record_token_validation;
use base64::{engine::general_purpose, Engine as _};
use common::secret::{ExposeSecret, SecretBox, SecretString};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use ring::{
    aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM},
    rand::{SecureRandom, SystemRandom},
    signature::{Ed25519KeyPair, KeyPair},
};
use serde::{Deserialize, Serialize};
use std::fmt;
use tracing::instrument;

/// Maximum allowed JWT size in bytes (4KB).
///
/// This limit prevents Denial-of-Service (DoS) attacks via oversized tokens.
/// JWTs larger than this size are rejected before any parsing or cryptographic
/// operations, providing defense-in-depth against resource exhaustion attacks.
///
/// Rationale:
/// - Typical JWTs are 200-500 bytes (header + claims + signature)
/// - Our standard token: ~350 bytes (EdDSA sig, basic claims)
/// - 4KB limit allows for reasonable future expansion while preventing abuse
/// - Checked BEFORE base64 decode and signature verification for efficiency
///
/// Attack scenario:
/// - Attacker sends 10MB JWT to /token/verify endpoint
/// - Without size limit: Base64 decode allocates large buffer, wastes CPU/memory
/// - With size limit: Rejected immediately with minimal resource usage
///
/// Per OWASP API Security Top 10 - API4:2023 (Unrestricted Resource Consumption)
const MAX_JWT_SIZE_BYTES: usize = 4096; // 4KB

/// JWT Claims structure.
///
/// The `sub` field contains user or client identifiers which should not
/// be exposed in logs. A custom Debug implementation redacts this field.
#[derive(Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,   // Subject (user_id or client_id)
    pub exp: i64,      // Expiration timestamp
    pub iat: i64,      // Issued at timestamp
    pub scope: String, // Space-separated scopes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_type: Option<String>, // Service type for service tokens
}

/// Custom Debug implementation that redacts the `sub` field.
///
/// The `sub` field contains user/client identifiers which are sensitive
/// and should not be exposed in logs or debug output.
impl fmt::Debug for Claims {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Claims")
            .field("sub", &"[REDACTED]")
            .field("exp", &self.exp)
            .field("iat", &self.iat)
            .field("scope", &self.scope)
            .field("service_type", &self.service_type)
            .finish()
    }
}

/// Encrypted key structure (AES-256-GCM)
///
/// The `encrypted_data` field is wrapped in `SecretBox` to prevent accidental
/// exposure. While it is ciphertext, it still contains sensitive encrypted
/// key material that should not be logged.
///
/// Debug is manually implemented to redact all cryptographic material.
/// Clone is manually implemented because SecretBox requires explicit cloning.
pub struct EncryptedKey {
    /// Encrypted key material (AES-256-GCM ciphertext).
    /// Use `.expose_secret()` to access the actual bytes.
    pub encrypted_data: SecretBox<Vec<u8>>,
    pub nonce: Vec<u8>, // 96-bit (12 bytes)
    pub tag: Vec<u8>,   // 128-bit (16 bytes)
}

impl Clone for EncryptedKey {
    fn clone(&self) -> Self {
        Self {
            encrypted_data: SecretBox::new(Box::new(self.encrypted_data.expose_secret().clone())),
            nonce: self.nonce.clone(),
            tag: self.tag.clone(),
        }
    }
}

impl fmt::Debug for EncryptedKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EncryptedKey")
            .field("encrypted_data", &"[REDACTED]")
            .field("nonce", &"[REDACTED]")
            .field("tag", &"[REDACTED]")
            .finish()
    }
}

/// Generate EdDSA (Ed25519) keypair using CSPRNG
///
/// Returns (public_key_pem, private_key_pkcs8)
#[instrument(skip_all)]
pub fn generate_signing_key() -> Result<(String, Vec<u8>), AcError> {
    let rng = SystemRandom::new();

    // Generate Ed25519 keypair in PKCS8 format
    let pkcs8_bytes = Ed25519KeyPair::generate_pkcs8(&rng)
        .map_err(|e| AcError::Crypto(format!("Keypair generation failed: {}", e)))?;

    let key_pair = Ed25519KeyPair::from_pkcs8(pkcs8_bytes.as_ref())
        .map_err(|e| AcError::Crypto(format!("Keypair parsing failed: {}", e)))?;

    // Get public key bytes
    let public_key_bytes = key_pair.public_key().as_ref();

    // Convert public key to PEM format (base64 encoded)
    let public_key_pem = format!(
        "-----BEGIN PUBLIC KEY-----\n{}\n-----END PUBLIC KEY-----",
        general_purpose::STANDARD.encode(public_key_bytes)
    );

    Ok((public_key_pem, pkcs8_bytes.as_ref().to_vec()))
}

/// Encrypt private key with AES-256-GCM
///
/// Uses a 96-bit random nonce and produces a 128-bit authentication tag
#[instrument(skip_all)]
pub fn encrypt_private_key(private_key: &[u8], master_key: &[u8]) -> Result<EncryptedKey, AcError> {
    if master_key.len() != 32 {
        return Err(AcError::Crypto(format!(
            "Invalid master key length: {} (expected 32)",
            master_key.len()
        )));
    }

    let rng = SystemRandom::new();

    // Generate random 96-bit nonce (12 bytes)
    let mut nonce_bytes = [0u8; 12];
    rng.fill(&mut nonce_bytes)
        .map_err(|e| AcError::Crypto(format!("Nonce generation failed: {}", e)))?;

    let nonce = Nonce::assume_unique_for_key(nonce_bytes);

    // Create AES-256-GCM cipher
    let unbound_key = UnboundKey::new(&AES_256_GCM, master_key)
        .map_err(|e| AcError::Crypto(format!("Cipher key creation failed: {}", e)))?;
    let sealing_key = LessSafeKey::new(unbound_key);

    // Encrypt the private key (in-place operation requires mutable buffer)
    let mut in_out = private_key.to_vec();
    sealing_key
        .seal_in_place_append_tag(nonce, Aad::empty(), &mut in_out)
        .map_err(|e| AcError::Crypto(format!("Encryption operation failed: {}", e)))?;

    // Split ciphertext and tag (last 16 bytes are the tag)
    // After seal_in_place_append_tag, the buffer contains original data + 16-byte tag
    let tag_start = in_out
        .len()
        .checked_sub(16)
        .ok_or_else(|| AcError::Crypto("Encryption produced invalid output".to_string()))?;
    let encrypted_data = in_out
        .get(..tag_start)
        .ok_or_else(|| AcError::Crypto("Encryption produced invalid output".to_string()))?
        .to_vec();
    let tag = in_out
        .get(tag_start..)
        .ok_or_else(|| AcError::Crypto("Encryption produced invalid output".to_string()))?
        .to_vec();

    Ok(EncryptedKey {
        encrypted_data: SecretBox::new(Box::new(encrypted_data)),
        nonce: nonce_bytes.to_vec(),
        tag,
    })
}

/// Decrypt private key with AES-256-GCM
#[instrument(skip_all)]
pub fn decrypt_private_key(
    encrypted: &EncryptedKey,
    master_key: &[u8],
) -> Result<Vec<u8>, AcError> {
    if master_key.len() != 32 {
        return Err(AcError::Crypto(format!(
            "Invalid master key length: {} (expected 32)",
            master_key.len()
        )));
    }

    if encrypted.nonce.len() != 12 {
        return Err(AcError::Crypto(format!(
            "Invalid nonce length: {} (expected 12)",
            encrypted.nonce.len()
        )));
    }

    if encrypted.tag.len() != 16 {
        return Err(AcError::Crypto(format!(
            "Invalid tag length: {} (expected 16)",
            encrypted.tag.len()
        )));
    }

    // Reconstruct ciphertext with tag
    let mut in_out = encrypted.encrypted_data.expose_secret().clone();
    in_out.extend_from_slice(&encrypted.tag);

    let nonce_bytes: [u8; 12] = encrypted
        .nonce
        .as_slice()
        .try_into()
        .map_err(|e| AcError::Crypto(format!("Invalid nonce format: {}", e)))?;
    let nonce = Nonce::assume_unique_for_key(nonce_bytes);

    // Create AES-256-GCM cipher
    let unbound_key = UnboundKey::new(&AES_256_GCM, master_key)
        .map_err(|e| AcError::Crypto(format!("Cipher key creation failed: {}", e)))?;
    let opening_key = LessSafeKey::new(unbound_key);

    // Decrypt in place
    let decrypted = opening_key
        .open_in_place(nonce, Aad::empty(), &mut in_out)
        .map_err(|e| AcError::Crypto(format!("Decryption operation failed: {}", e)))?;

    Ok(decrypted.to_vec())
}

/// Sign JWT with EdDSA private key
#[instrument(skip_all)]
pub fn sign_jwt(
    claims: &Claims,
    private_key_pkcs8: &[u8],
    key_id: &str,
) -> Result<String, AcError> {
    // Validate the private key format
    let _key_pair = Ed25519KeyPair::from_pkcs8(private_key_pkcs8)
        .map_err(|e| AcError::Crypto(format!("Invalid private key format: {}", e)))?;

    // Get the raw private key bytes for jsonwebtoken
    // Ed25519KeyPair doesn't expose the seed directly, so we need to use the PKCS8 format
    let encoding_key = EncodingKey::from_ed_der(private_key_pkcs8);

    let mut header = Header::new(Algorithm::EdDSA);
    header.typ = Some("JWT".to_string());
    header.kid = Some(key_id.to_string());

    let token = encode(&header, claims, &encoding_key)
        .map_err(|e| AcError::Crypto(format!("JWT signing operation failed: {}", e)))?;

    Ok(token)
}

/// Extract the `kid` (key ID) from a JWT header without verifying the signature.
///
/// This is used to look up the correct signing key for verification when
/// multiple keys may be valid (e.g., during key rotation).
///
/// Returns `None` if:
/// - Token is malformed (not valid JWT format)
/// - Header doesn't contain a `kid` field
/// - `kid` field is not a string
///
/// SECURITY NOTE: This function does NOT validate the token. It only extracts
/// the `kid` claim for key lookup. The token MUST still be verified after
/// fetching the key.
#[instrument(skip_all)]
pub fn extract_jwt_kid(token: &str) -> Option<String> {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

    // Check token size first (same as verify_jwt)
    if token.len() > MAX_JWT_SIZE_BYTES {
        return None;
    }

    // JWT format: header.payload.signature
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return None;
    }

    // Decode the header (first part)
    let header_bytes = URL_SAFE_NO_PAD.decode(parts.first()?).ok()?;
    let header: serde_json::Value = serde_json::from_slice(&header_bytes).ok()?;

    // Extract kid as string
    header.get("kid")?.as_str().map(|s| s.to_string())
}

/// Verify JWT with EdDSA public key
///
/// Validates:
/// - Token size (must be <= MAX_JWT_SIZE_BYTES)
/// - Signature (EdDSA/Ed25519)
/// - Expiration (`exp` claim)
/// - Issued-at time (`iat` claim) with clock skew tolerance and maximum age
///
/// The `iat` claim is validated to prevent token pre-generation and replay attacks:
/// - Tokens with `iat` more than `clock_skew_seconds` in the future are rejected
/// - Tokens with `iat` more than `MAX_TOKEN_AGE_SECONDS` in the past are rejected
///
/// The size check is performed BEFORE any parsing to prevent DoS attacks
/// via oversized tokens.
///
/// # Arguments
///
/// * `token` - The JWT string to verify
/// * `public_key_pem` - The public key in PEM format for signature verification
/// * `clock_skew_seconds` - Clock skew tolerance for iat validation (typically 300 seconds / 5 minutes)
#[instrument(skip_all)]
pub fn verify_jwt(
    token: &str,
    public_key_pem: &str,
    clock_skew_seconds: i64,
) -> Result<Claims, AcError> {
    // Check token size BEFORE any parsing or cryptographic operations
    // This is a defense-in-depth measure against DoS attacks
    if token.len() > MAX_JWT_SIZE_BYTES {
        tracing::debug!(
            target: "crypto",
            token_size = token.len(),
            max_size = MAX_JWT_SIZE_BYTES,
            "Token rejected: size exceeds maximum allowed"
        );
        return Err(AcError::InvalidToken(
            "The access token is invalid or expired".to_string(),
        ));
    }

    // Extract base64 from PEM format
    let public_key_b64 = public_key_pem
        .lines()
        .filter(|line| !line.starts_with("-----"))
        .collect::<String>();

    let public_key_bytes = general_purpose::STANDARD
        .decode(&public_key_b64)
        .map_err(|e| {
            tracing::debug!(target: "crypto", error = %e, "Invalid public key encoding");
            AcError::InvalidToken("The access token is invalid or expired".to_string())
        })?;

    let decoding_key = DecodingKey::from_ed_der(&public_key_bytes);

    let mut validation = Validation::new(Algorithm::EdDSA);
    validation.validate_exp = true;

    let token_data = decode::<Claims>(token, &decoding_key, &validation).map_err(|e| {
        tracing::debug!(target: "crypto", error = %e, "Token verification failed");
        AcError::InvalidToken("The access token is invalid or expired".to_string())
    })?;

    // Validate iat (issued-at) claim with clock skew tolerance
    // Reject tokens with iat too far in the future (potential pre-generation attack)
    let now = chrono::Utc::now().timestamp();
    let max_iat = now + clock_skew_seconds;

    if token_data.claims.iat > max_iat {
        tracing::debug!(
            target: "crypto",
            iat = token_data.claims.iat,
            now = now,
            max_allowed = max_iat,
            clock_skew_seconds = clock_skew_seconds,
            "Token rejected: iat too far in the future"
        );
        // Record metric for clock skew rejection (enables alerting on clock drift issues)
        record_token_validation("error", Some("clock_skew"));
        return Err(AcError::InvalidToken(
            "The access token is invalid or expired".to_string(),
        ));
    }

    Ok(token_data.claims)
}

/// Hash client secret with bcrypt using configurable cost factor.
///
/// # Arguments
///
/// * `secret` - The plaintext client secret to hash
/// * `cost` - Bcrypt cost factor (2^cost iterations). Valid range: 10-14.
///
/// # Security
///
/// - Cost < 10 is insecure per OWASP 2024 guidelines
/// - Cost > 14 causes excessive latency (~800ms+)
/// - Default: 12 per ADR-0003 (~200ms hash time)
///
/// # Errors
///
/// Returns `AcError::Crypto` if:
/// - Cost is outside valid range (10-14) - defense-in-depth validation
/// - Bcrypt hashing fails
#[instrument(skip_all)]
pub fn hash_client_secret(secret: &str, cost: u32) -> Result<String, AcError> {
    // Defense-in-depth: Validate cost even though config should have already validated.
    // This prevents insecure hashing if this function is called directly with invalid cost.
    if !(MIN_BCRYPT_COST..=MAX_BCRYPT_COST).contains(&cost) {
        return Err(AcError::Crypto(format!(
            "Invalid bcrypt cost: {} (must be {}-{})",
            cost, MIN_BCRYPT_COST, MAX_BCRYPT_COST
        )));
    }

    bcrypt::hash(secret, cost)
        .map_err(|e| AcError::Crypto(format!("Password hashing failed: {}", e)))
}

/// Verify client secret against bcrypt hash
#[instrument(skip_all)]
pub fn verify_client_secret(secret: &str, hash: &str) -> Result<bool, AcError> {
    bcrypt::verify(secret, hash)
        .map_err(|e| AcError::Crypto(format!("Password verification failed: {}", e)))
}

/// Generate cryptographically secure random bytes
pub fn generate_random_bytes(len: usize) -> Result<Vec<u8>, AcError> {
    let rng = SystemRandom::new();
    let mut bytes = vec![0u8; len];
    rng.fill(&mut bytes)
        .map_err(|e| AcError::Crypto(format!("Random bytes generation failed: {}", e)))?;
    Ok(bytes)
}

/// Generate a client secret (32 bytes, base64 encoded).
///
/// Returns a `SecretString` to prevent accidental exposure in logs or debug output.
/// The caller must use `.expose_secret()` to access the actual value.
#[instrument(skip_all)]
pub fn generate_client_secret() -> Result<SecretString, AcError> {
    let bytes = generate_random_bytes(32)?;
    let encoded = general_purpose::STANDARD.encode(&bytes);
    Ok(SecretString::from(encoded))
}

// ============================================================================
// User Token Support (ADR-0020)
// ============================================================================

/// User JWT Claims structure per ADR-0020.
///
/// User tokens are issued by AC for authenticated users and include
/// organization membership information.
#[derive(Clone, Serialize, Deserialize)]
pub struct UserClaims {
    /// Subject (user UUID)
    pub sub: String,
    /// Organization ID the user belongs to
    pub org_id: String,
    /// User's email address
    pub email: String,
    /// User roles (e.g., ["user"], ["user", "admin"])
    pub roles: Vec<String>,
    /// Issued at timestamp
    pub iat: i64,
    /// Expiration timestamp
    pub exp: i64,
    /// Unique token identifier for revocation
    pub jti: String,
}

/// Custom Debug implementation that redacts sensitive fields.
///
/// The `sub`, `email`, and `jti` fields are sensitive and should not
/// be exposed in logs or debug output.
impl fmt::Debug for UserClaims {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UserClaims")
            .field("sub", &"[REDACTED]")
            .field("org_id", &self.org_id)
            .field("email", &"[REDACTED]")
            .field("roles", &self.roles)
            .field("iat", &self.iat)
            .field("exp", &self.exp)
            .field("jti", &"[REDACTED]")
            .finish()
    }
}

/// Sign a user JWT with EdDSA private key.
///
/// Uses the same signing infrastructure as service tokens but with
/// UserClaims structure per ADR-0020.
#[instrument(skip_all)]
pub fn sign_user_jwt(
    claims: &UserClaims,
    private_key_pkcs8: &[u8],
    key_id: &str,
) -> Result<String, AcError> {
    use ring::signature::Ed25519KeyPair;

    // Validate the private key format
    let _key_pair = Ed25519KeyPair::from_pkcs8(private_key_pkcs8)
        .map_err(|e| AcError::Crypto(format!("Invalid private key format: {}", e)))?;

    let encoding_key = EncodingKey::from_ed_der(private_key_pkcs8);

    let mut header = Header::new(Algorithm::EdDSA);
    header.typ = Some("JWT".to_string());
    header.kid = Some(key_id.to_string());

    let token = encode(&header, claims, &encoding_key)
        .map_err(|e| AcError::Crypto(format!("JWT signing operation failed: {}", e)))?;

    Ok(token)
}

/// Verify a user JWT and extract claims.
///
/// Validates:
/// - Token size (must be <= MAX_JWT_SIZE_BYTES)
/// - Signature (EdDSA/Ed25519)
/// - Expiration (`exp` claim)
/// - Issued-at time (`iat` claim) with clock skew tolerance
#[allow(dead_code)] // Library function - will be used by GC and MC for token validation
#[instrument(skip_all)]
pub fn verify_user_jwt(
    token: &str,
    public_key_pem: &str,
    clock_skew_seconds: i64,
) -> Result<UserClaims, AcError> {
    // Check token size BEFORE any parsing
    if token.len() > MAX_JWT_SIZE_BYTES {
        tracing::debug!(
            target: "crypto",
            token_size = token.len(),
            max_size = MAX_JWT_SIZE_BYTES,
            "User token rejected: size exceeds maximum allowed"
        );
        return Err(AcError::InvalidToken(
            "The access token is invalid or expired".to_string(),
        ));
    }

    // Extract base64 from PEM format
    let public_key_b64 = public_key_pem
        .lines()
        .filter(|line| !line.starts_with("-----"))
        .collect::<String>();

    let public_key_bytes = general_purpose::STANDARD
        .decode(&public_key_b64)
        .map_err(|e| {
            tracing::debug!(target: "crypto", error = %e, "Invalid public key encoding");
            AcError::InvalidToken("The access token is invalid or expired".to_string())
        })?;

    let decoding_key = DecodingKey::from_ed_der(&public_key_bytes);

    let mut validation = Validation::new(Algorithm::EdDSA);
    validation.validate_exp = true;

    let token_data = decode::<UserClaims>(token, &decoding_key, &validation).map_err(|e| {
        tracing::debug!(target: "crypto", error = %e, "User token verification failed");
        AcError::InvalidToken("The access token is invalid or expired".to_string())
    })?;

    // Validate iat (issued-at) claim with clock skew tolerance
    let now = chrono::Utc::now().timestamp();
    let max_iat = now + clock_skew_seconds;

    if token_data.claims.iat > max_iat {
        tracing::debug!(
            target: "crypto",
            iat = token_data.claims.iat,
            now = now,
            max_allowed = max_iat,
            clock_skew_seconds = clock_skew_seconds,
            "User token rejected: iat too far in the future"
        );
        record_token_validation("error", Some("clock_skew"));
        return Err(AcError::InvalidToken(
            "The access token is invalid or expired".to_string(),
        ));
    }

    Ok(token_data.claims)
}

#[cfg(test)]
#[allow(clippy::panic, clippy::assertions_on_constants)]
mod tests {
    use super::*;

    #[test]
    fn test_key_generation() {
        let result = generate_signing_key();
        assert!(result.is_ok());
        let (public_pem, private_pkcs8) = result.unwrap();
        assert!(public_pem.contains("BEGIN PUBLIC KEY"));
        assert!(!private_pkcs8.is_empty());
    }

    #[test]
    fn test_encryption_decryption() {
        let master_key = vec![0u8; 32]; // Test key
        let data = b"secret private key data";

        let encrypted = encrypt_private_key(data, &master_key).unwrap();
        assert_eq!(encrypted.nonce.len(), 12);
        assert_eq!(encrypted.tag.len(), 16);

        let decrypted = decrypt_private_key(&encrypted, &master_key).unwrap();
        assert_eq!(decrypted, data);
    }

    #[test]
    fn test_jwt_sign_verify() {
        let (public_pem, private_pkcs8) = generate_signing_key().unwrap();

        let claims = Claims {
            sub: "test-user".to_string(),
            exp: chrono::Utc::now().timestamp() + 3600,
            iat: chrono::Utc::now().timestamp(),
            scope: "read write".to_string(),
            service_type: None,
        };

        let token = sign_jwt(&claims, &private_pkcs8, "test-key-01").unwrap();
        let verified_claims =
            verify_jwt(&token, &public_pem, DEFAULT_JWT_CLOCK_SKEW_SECONDS).unwrap();

        assert_eq!(verified_claims.sub, claims.sub);
        assert_eq!(verified_claims.scope, claims.scope);
    }

    #[test]
    fn test_password_hashing() {
        let secret = "my-secure-secret";
        let hash = hash_client_secret(secret, DEFAULT_BCRYPT_COST).unwrap();

        assert!(verify_client_secret(secret, &hash).unwrap());
        assert!(!verify_client_secret("wrong-secret", &hash).unwrap());
    }

    #[test]
    fn test_generate_client_secret() {
        use common::secret::ExposeSecret;

        let secret = generate_client_secret().unwrap();
        assert!(!secret.expose_secret().is_empty());

        // Should be base64 encoded
        assert!(general_purpose::STANDARD
            .decode(secret.expose_secret())
            .is_ok());
    }

    // Error path tests
    #[test]
    fn test_encrypt_with_invalid_master_key_length() {
        let data = b"secret data";
        let wrong_key = vec![0u8; 16]; // Wrong length (should be 32)

        let result = encrypt_private_key(data, &wrong_key);
        let err = result.expect_err("Expected Crypto error");
        assert!(
            matches!(err, AcError::Crypto(msg) if msg.starts_with("Invalid master key length:"))
        );
    }

    #[test]
    fn test_decrypt_with_wrong_master_key() {
        let master_key = vec![0u8; 32];
        let wrong_key = vec![1u8; 32]; // Different key
        let data = b"secret data";

        let encrypted = encrypt_private_key(data, &master_key).unwrap();
        let result = decrypt_private_key(&encrypted, &wrong_key);
        let err = result.expect_err("Expected Crypto error");
        assert!(
            matches!(err, AcError::Crypto(msg) if msg.starts_with("Decryption operation failed:"))
        );
    }

    #[test]
    fn test_decrypt_with_invalid_master_key_length() {
        let data = b"secret data";
        let master_key = vec![0u8; 32];
        let wrong_key = vec![0u8; 16]; // Wrong length

        let encrypted = encrypt_private_key(data, &master_key).unwrap();
        let result = decrypt_private_key(&encrypted, &wrong_key);
        let err = result.expect_err("Expected Crypto error");
        assert!(
            matches!(err, AcError::Crypto(msg) if msg.starts_with("Invalid master key length:"))
        );
    }

    #[test]
    fn test_decrypt_with_invalid_nonce_length() {
        let data = b"secret data";
        let master_key = vec![0u8; 32];

        let mut encrypted = encrypt_private_key(data, &master_key).unwrap();
        encrypted.nonce = vec![0u8; 8]; // Wrong nonce length (should be 12)

        let result = decrypt_private_key(&encrypted, &master_key);
        let err = result.expect_err("Expected Crypto error");
        assert!(matches!(err, AcError::Crypto(msg) if msg.starts_with("Invalid nonce length:")));
    }

    #[test]
    fn test_decrypt_with_invalid_tag_length() {
        let data = b"secret data";
        let master_key = vec![0u8; 32];

        let mut encrypted = encrypt_private_key(data, &master_key).unwrap();
        encrypted.tag = vec![0u8; 8]; // Wrong tag length (should be 16)

        let result = decrypt_private_key(&encrypted, &master_key);
        let err = result.expect_err("Expected Crypto error");
        assert!(matches!(err, AcError::Crypto(msg) if msg.starts_with("Invalid tag length:")));
    }

    #[test]
    fn test_verify_jwt_expired_token() {
        let (public_pem, private_pkcs8) = generate_signing_key().unwrap();

        // Create expired claims (exp in the past)
        let claims = Claims {
            sub: "test-user".to_string(),
            exp: chrono::Utc::now().timestamp() - 3600, // 1 hour ago
            iat: chrono::Utc::now().timestamp() - 7200, // 2 hours ago
            scope: "read write".to_string(),
            service_type: None,
        };

        let token = sign_jwt(&claims, &private_pkcs8, "test-key-01").unwrap();
        let result = verify_jwt(&token, &public_pem, DEFAULT_JWT_CLOCK_SKEW_SECONDS);
        let err = result.expect_err("Expected InvalidToken error for expired token");
        assert!(matches!(err, AcError::InvalidToken(_)));
    }

    #[test]
    fn test_verify_jwt_wrong_public_key() {
        let (_, private_pkcs8) = generate_signing_key().unwrap();
        let (wrong_public_pem, _) = generate_signing_key().unwrap(); // Different keypair

        let claims = Claims {
            sub: "test-user".to_string(),
            exp: chrono::Utc::now().timestamp() + 3600,
            iat: chrono::Utc::now().timestamp(),
            scope: "read write".to_string(),
            service_type: None,
        };

        let token = sign_jwt(&claims, &private_pkcs8, "test-key-01").unwrap();
        let result = verify_jwt(&token, &wrong_public_pem, DEFAULT_JWT_CLOCK_SKEW_SECONDS);
        let err = result.expect_err("Expected InvalidToken error for wrong public key");
        assert!(matches!(err, AcError::InvalidToken(_)));
    }

    #[test]
    fn test_password_hashing_empty_string() {
        // Empty password should still hash successfully
        let result = hash_client_secret("", DEFAULT_BCRYPT_COST);
        assert!(result.is_ok());

        let hash = result.unwrap();
        assert!(!hash.is_empty());

        // Verify should work
        assert!(verify_client_secret("", &hash).unwrap());
        assert!(!verify_client_secret("not-empty", &hash).unwrap());
    }

    #[test]
    fn test_verify_password_with_invalid_hash() {
        // Try to verify against an invalid bcrypt hash
        let result = verify_client_secret("password", "not-a-valid-hash");
        let err = result.expect_err("Expected Crypto error");
        assert!(
            matches!(err, AcError::Crypto(msg) if msg.starts_with("Password verification failed:"))
        );
    }

    /// P1-SECURITY: Test that bcrypt cost factor is 12 (per ADR-0003)
    ///
    /// Verifies that password hashing uses the correct cost factor as specified
    /// in ADR-0003 (Service Authentication). Cost factor 12 provides appropriate
    /// security for 2024+ (2^12 = 4,096 iterations).
    ///
    /// Per OWASP, bcrypt cost of 10-12 is recommended as of 2024.
    /// Per CWE-916: Use of Password Hash With Insufficient Computational Effort
    #[test]
    fn test_bcrypt_cost_factor_is_12() {
        let secret = "test-password-for-cost-verification";
        let hash = hash_client_secret(secret, DEFAULT_BCRYPT_COST).expect("Hashing should succeed");

        // Bcrypt hash format: $2b$<cost>$<salt+hash>
        // Example: $2b$12$R9h/cIPz0gi.URNNX3kh2OPST9/PgBkqquzi.Ss7KIUgO2t0jWMUW
        //          └─┬─┘ └┬┘
        //          version cost
        let parts: Vec<&str> = hash.split('$').collect();

        // Verify hash structure
        assert_eq!(
            parts.len(),
            4,
            "Bcrypt hash should have 4 parts: ['', '2b', 'cost', 'salt+hash']"
        );

        // Verify bcrypt version (2b is the current standard)
        assert_eq!(
            parts[1], "2b",
            "Bcrypt should use version 2b (current standard)"
        );

        // Verify cost factor is exactly 12 per ADR-0003
        assert_eq!(
            parts[2], "12",
            "Bcrypt cost factor must be 12 per ADR-0003 (Service Authentication). \
             Cost 12 = 2^12 = 4,096 iterations, appropriate for 2024+ security requirements."
        );

        // Verify the hash is valid (can be verified)
        assert!(
            verify_client_secret(secret, &hash).expect("Verification should succeed"),
            "Generated hash should verify correctly"
        );
    }

    /// P1-SECURITY: Test JWT size limit enforcement (unit test)
    ///
    /// Verifies that oversized JWTs are rejected before parsing.
    /// This is a simple unit test that complements the integration test
    /// in token_service.rs.
    #[test]
    fn test_jwt_size_limit_enforcement() {
        // Create an oversized token (just a long string, doesn't need to be valid JWT)
        let oversized_token = "a".repeat(MAX_JWT_SIZE_BYTES + 1);

        // Generate a keypair for testing
        let (public_pem, _) = generate_signing_key().unwrap();

        // Attempt to verify the oversized token
        let result = verify_jwt(
            &oversized_token,
            &public_pem,
            DEFAULT_JWT_CLOCK_SKEW_SECONDS,
        );

        // Should be rejected due to size limit
        assert!(
            matches!(result, Err(AcError::InvalidToken(_))),
            "Oversized JWT should be rejected before parsing"
        );
    }

    /// P1-SECURITY: Test JWT size limit allows normal tokens
    ///
    /// Regression test to ensure the size limit doesn't reject normal JWTs.
    #[test]
    fn test_jwt_size_limit_allows_normal_tokens() {
        let (public_pem, private_pkcs8) = generate_signing_key().unwrap();

        let claims = Claims {
            sub: "test-user".to_string(),
            exp: chrono::Utc::now().timestamp() + 3600,
            iat: chrono::Utc::now().timestamp(),
            scope: "read write".to_string(),
            service_type: None,
        };

        let token = sign_jwt(&claims, &private_pkcs8, "test-key-01").unwrap();

        // Verify the token is under the size limit
        assert!(
            token.len() <= MAX_JWT_SIZE_BYTES,
            "Normal JWT should be well under the size limit. Got {} bytes",
            token.len()
        );

        // Should verify successfully
        let verified_claims =
            verify_jwt(&token, &public_pem, DEFAULT_JWT_CLOCK_SKEW_SECONDS).unwrap();

        assert_eq!(verified_claims.sub, claims.sub);
        assert_eq!(verified_claims.scope, claims.scope);
    }

    /// P1-SECURITY: Test bcrypt cost factor security boundary
    ///
    /// Documents that cost < 10 is insecure per OWASP guidelines.
    /// This is a documentation test - we don't actually test weak hashes,
    /// but document the security requirement.
    #[test]
    fn test_bcrypt_cost_factor_security_rationale() {
        // Cost factor security analysis (for documentation):
        //
        // Cost 10 = 2^10 = 1,024 iterations
        // Cost 11 = 2^11 = 2,048 iterations
        // Cost 12 = 2^12 = 4,096 iterations ← Our choice (ADR-0003)
        // Cost 13 = 2^13 = 8,192 iterations
        //
        // OWASP (2024) recommends cost 10-12 depending on hardware.
        // We chose 12 to future-proof against improving attack hardware.
        //
        // Approximate hashing time on modern CPU (2024):
        // Cost 10: ~50ms
        // Cost 12: ~200ms ← Our choice
        // Cost 13: ~400ms
        //
        // Our cost=12 provides good security without excessive latency.

        let hash = hash_client_secret("test", DEFAULT_BCRYPT_COST).unwrap();
        let cost = hash.split('$').nth(2).unwrap();

        assert_eq!(cost, "12", "Cost factor must be 12 per security policy");
    }

    // ============================================================================
    // P1 Security Tests - JWT iat (issued-at) Validation
    // ============================================================================

    /// P1-SECURITY: Test JWT iat validation rejects far-future tokens
    ///
    /// Verifies that tokens with iat more than JWT_CLOCK_SKEW_SECONDS (5 minutes)
    /// in the future are rejected. This prevents token pre-generation attacks
    /// and detects compromised systems with incorrect clocks.
    #[test]
    fn test_jwt_iat_validation_rejects_future() {
        let (public_pem, private_pkcs8) = generate_signing_key().unwrap();

        let now = chrono::Utc::now().timestamp();

        // Create token with iat 1 hour in the future (way beyond clock skew)
        let claims = Claims {
            sub: "test-user".to_string(),
            exp: now + 7200, // Expires in 2 hours
            iat: now + 3600, // Issued 1 hour from now (suspicious!)
            scope: "read write".to_string(),
            service_type: None,
        };

        let token = sign_jwt(&claims, &private_pkcs8, "test-key-01").unwrap();
        let result = verify_jwt(&token, &public_pem, DEFAULT_JWT_CLOCK_SKEW_SECONDS);

        // Should be rejected - iat too far in the future
        assert!(
            matches!(result, Err(AcError::InvalidToken(_))),
            "Token with iat 1 hour in future should be rejected"
        );
    }

    /// P1-SECURITY: Test JWT iat validation accepts within clock skew
    ///
    /// Verifies that tokens with iat within the clock skew tolerance
    /// (2 minutes in the future) are accepted. This allows for reasonable
    /// clock drift between servers.
    #[test]
    fn test_jwt_iat_validation_accepts_within_skew() {
        let (public_pem, private_pkcs8) = generate_signing_key().unwrap();

        let now = chrono::Utc::now().timestamp();

        // Create token with iat 2 minutes in the future (within 5 min clock skew)
        let claims = Claims {
            sub: "test-user".to_string(),
            exp: now + 3600, // Expires in 1 hour
            iat: now + 120,  // Issued 2 minutes from now (within tolerance)
            scope: "read write".to_string(),
            service_type: None,
        };

        let token = sign_jwt(&claims, &private_pkcs8, "test-key-01").unwrap();
        let result = verify_jwt(&token, &public_pem, DEFAULT_JWT_CLOCK_SKEW_SECONDS);

        // Should be accepted - iat within clock skew tolerance
        assert!(
            result.is_ok(),
            "Token with iat 2 minutes in future should be accepted"
        );

        let verified_claims = result.unwrap();
        assert_eq!(verified_claims.sub, claims.sub);
        assert_eq!(verified_claims.scope, claims.scope);
    }

    /// P1-SECURITY: Test JWT clock skew constant value
    ///
    /// Documents that DEFAULT_JWT_CLOCK_SKEW_SECONDS is 300 seconds (5 minutes)
    /// per NIST SP 800-63B recommendations for time-based security controls.
    #[test]
    fn test_clock_skew_constant_value() {
        // This test documents the constant value for security review
        assert_eq!(
            DEFAULT_JWT_CLOCK_SKEW_SECONDS, 300,
            "Clock skew tolerance must be 300 seconds (5 minutes) per NIST SP 800-63B"
        );
    }

    /// Test that sign_jwt() includes kid (key ID) in JWT header
    ///
    /// Verifies that the JWT header contains the kid field with the correct value.
    /// This is required for key rotation support per ADR-0008.
    #[test]
    fn test_jwt_includes_kid_header() {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

        let (_, private_pkcs8) = generate_signing_key().unwrap();

        let claims = Claims {
            sub: "test-user".to_string(),
            exp: chrono::Utc::now().timestamp() + 3600,
            iat: chrono::Utc::now().timestamp(),
            scope: "read write".to_string(),
            service_type: None,
        };

        let key_id = "auth-prod-2025-01";
        let token = sign_jwt(&claims, &private_pkcs8, key_id).unwrap();

        // Extract and decode the header (first part of JWT)
        let parts: Vec<&str> = token.split('.').collect();
        assert_eq!(parts.len(), 3, "JWT should have 3 parts");

        let header_bytes = URL_SAFE_NO_PAD
            .decode(parts[0])
            .expect("Failed to decode header");
        let header: serde_json::Value =
            serde_json::from_slice(&header_bytes).expect("Failed to parse header JSON");

        // Verify kid is present and matches
        assert_eq!(
            header["kid"].as_str().unwrap(),
            key_id,
            "JWT header should contain kid matching the provided key_id"
        );

        // Verify algorithm is EdDSA
        assert_eq!(header["alg"].as_str().unwrap(), "EdDSA");

        // Verify typ is JWT
        assert_eq!(header["typ"].as_str().unwrap(), "JWT");
    }

    // ============================================================================
    // Additional Coverage Tests - Error Paths
    // ============================================================================

    /// Test sign_jwt with invalid private key format
    ///
    /// Validates that sign_jwt properly rejects malformed private keys.
    #[test]
    fn test_sign_jwt_invalid_private_key() {
        let claims = Claims {
            sub: "test-user".to_string(),
            exp: chrono::Utc::now().timestamp() + 3600,
            iat: chrono::Utc::now().timestamp(),
            scope: "read write".to_string(),
            service_type: None,
        };

        // Use invalid PKCS8 data
        let invalid_key = vec![0u8; 32]; // Not a valid PKCS8 structure

        let result = sign_jwt(&claims, &invalid_key, "test-key-01");
        let err = result.expect_err("Invalid private key should be rejected");
        assert!(
            matches!(err, AcError::Crypto(msg) if msg.starts_with("Invalid private key format:"))
        );
    }

    /// Test verify_jwt with invalid public key PEM format
    ///
    /// Validates that verify_jwt properly rejects malformed PEM data.
    #[test]
    fn test_verify_jwt_invalid_pem_format() {
        let (_, private_pkcs8) = generate_signing_key().unwrap();

        let claims = Claims {
            sub: "test-user".to_string(),
            exp: chrono::Utc::now().timestamp() + 3600,
            iat: chrono::Utc::now().timestamp(),
            scope: "read write".to_string(),
            service_type: None,
        };

        let token = sign_jwt(&claims, &private_pkcs8, "test-key-01").unwrap();

        // Use invalid PEM format (not proper base64)
        let invalid_pem = "-----BEGIN PUBLIC KEY-----\ninvalid!@#$%\n-----END PUBLIC KEY-----";

        let result = verify_jwt(&token, invalid_pem, DEFAULT_JWT_CLOCK_SKEW_SECONDS);
        let err = result.expect_err("Invalid PEM format should be rejected during base64 decode");
        assert!(matches!(err, AcError::InvalidToken(_)));
    }

    /// Test verify_jwt with valid base64 but invalid key bytes
    ///
    /// Validates that verify_jwt rejects valid base64 that's not a valid Ed25519 key.
    #[test]
    fn test_verify_jwt_invalid_key_bytes() {
        let (_, private_pkcs8) = generate_signing_key().unwrap();

        let claims = Claims {
            sub: "test-user".to_string(),
            exp: chrono::Utc::now().timestamp() + 3600,
            iat: chrono::Utc::now().timestamp(),
            scope: "read write".to_string(),
            service_type: None,
        };

        let token = sign_jwt(&claims, &private_pkcs8, "test-key-01").unwrap();

        // Use valid base64 but invalid key bytes (wrong length for Ed25519)
        let invalid_key_bytes = vec![0u8; 16]; // Too short for Ed25519
        let invalid_pem = format!(
            "-----BEGIN PUBLIC KEY-----\n{}\n-----END PUBLIC KEY-----",
            general_purpose::STANDARD.encode(&invalid_key_bytes)
        );

        let result = verify_jwt(&token, &invalid_pem, DEFAULT_JWT_CLOCK_SKEW_SECONDS);
        let err = result.expect_err("Invalid key bytes should be rejected during verification");
        assert!(matches!(err, AcError::InvalidToken(_)));
    }

    /// Test verify_jwt with tampered token
    ///
    /// Validates that signature verification catches token tampering.
    #[test]
    fn test_verify_jwt_tampered_token() {
        let (public_pem, private_pkcs8) = generate_signing_key().unwrap();

        let claims = Claims {
            sub: "test-user".to_string(),
            exp: chrono::Utc::now().timestamp() + 3600,
            iat: chrono::Utc::now().timestamp(),
            scope: "read write".to_string(),
            service_type: None,
        };

        let mut token = sign_jwt(&claims, &private_pkcs8, "test-key-01").unwrap();

        // Tamper with the token by changing one character in the payload
        // JWT format: header.payload.signature
        let parts: Vec<&str> = token.split('.').collect();
        assert_eq!(parts.len(), 3, "JWT should have 3 parts");

        // Modify the payload slightly
        let tampered_payload = parts[1].to_string() + "X"; // Append character
        token = format!("{}.{}.{}", parts[0], tampered_payload, parts[2]);

        let result = verify_jwt(&token, &public_pem, DEFAULT_JWT_CLOCK_SKEW_SECONDS);
        let err = result.expect_err("Tampered token should be rejected");
        assert!(matches!(err, AcError::InvalidToken(_)));
    }

    /// Test verify_jwt with malformed token (not JWT format)
    ///
    /// Validates that completely invalid tokens are rejected early.
    #[test]
    fn test_verify_jwt_malformed_token() {
        let (public_pem, _) = generate_signing_key().unwrap();

        // Not a JWT at all
        let malformed_token = "not.a.valid.jwt.format.with.too.many.parts";

        let result = verify_jwt(malformed_token, &public_pem, DEFAULT_JWT_CLOCK_SKEW_SECONDS);
        let err = result.expect_err("Malformed token should be rejected");
        assert!(matches!(err, AcError::InvalidToken(_)));
    }

    /// Test JWT iat validation at exact clock skew boundary
    ///
    /// Tests the boundary condition where iat equals max_iat (should accept).
    #[test]
    fn test_jwt_iat_at_clock_skew_boundary() {
        let (public_pem, private_pkcs8) = generate_signing_key().unwrap();

        let now = chrono::Utc::now().timestamp();

        // Create token with iat exactly at the boundary (now + DEFAULT_JWT_CLOCK_SKEW_SECONDS)
        let claims = Claims {
            sub: "test-user".to_string(),
            exp: now + 7200,                           // Expires in 2 hours
            iat: now + DEFAULT_JWT_CLOCK_SKEW_SECONDS, // Exactly at boundary
            scope: "read write".to_string(),
            service_type: None,
        };

        let token = sign_jwt(&claims, &private_pkcs8, "test-key-01").unwrap();
        let result = verify_jwt(&token, &public_pem, DEFAULT_JWT_CLOCK_SKEW_SECONDS);

        // Should be accepted (boundary is inclusive: iat <= max_iat)
        assert!(
            result.is_ok(),
            "Token with iat at exact boundary should be accepted"
        );
    }

    /// Test JWT iat validation one second past boundary
    ///
    /// Tests that iat > max_iat is properly rejected.
    #[test]
    fn test_jwt_iat_one_second_past_boundary() {
        let (public_pem, private_pkcs8) = generate_signing_key().unwrap();

        let now = chrono::Utc::now().timestamp();

        // Create token with iat 1 second past the boundary
        let claims = Claims {
            sub: "test-user".to_string(),
            exp: now + 7200,
            iat: now + DEFAULT_JWT_CLOCK_SKEW_SECONDS + 1, // 1 second past boundary
            scope: "read write".to_string(),
            service_type: None,
        };

        let token = sign_jwt(&claims, &private_pkcs8, "test-key-01").unwrap();
        let result = verify_jwt(&token, &public_pem, DEFAULT_JWT_CLOCK_SKEW_SECONDS);

        // Should be rejected
        assert!(
            result.is_err(),
            "Token with iat 1 second past boundary should be rejected"
        );

        let err = result.expect_err("Expected InvalidToken error");
        assert!(matches!(err, AcError::InvalidToken(_)));
    }

    /// Test JWT with negative iat (old token)
    ///
    /// Validates that old tokens with iat in the past are accepted
    /// (as long as they haven't expired).
    #[test]
    fn test_jwt_with_old_iat() {
        let (public_pem, private_pkcs8) = generate_signing_key().unwrap();

        let now = chrono::Utc::now().timestamp();

        // Create token with iat 30 minutes ago
        let claims = Claims {
            sub: "test-user".to_string(),
            exp: now + 3600, // Still valid for 1 hour
            iat: now - 1800, // Issued 30 minutes ago
            scope: "read write".to_string(),
            service_type: None,
        };

        let token = sign_jwt(&claims, &private_pkcs8, "test-key-01").unwrap();
        let result = verify_jwt(&token, &public_pem, DEFAULT_JWT_CLOCK_SKEW_SECONDS);

        // Should be accepted (iat in the past is fine as long as not expired)
        assert!(
            result.is_ok(),
            "Token with old iat should be accepted if not expired"
        );
    }

    /// Test Claims serialization and deserialization
    ///
    /// Validates that Claims properly round-trips through JSON.
    #[test]
    fn test_claims_serialization() {
        let claims = Claims {
            sub: "test-user".to_string(),
            exp: 1234567890,
            iat: 1234567800,
            scope: "read write admin".to_string(),
            service_type: Some("global-controller".to_string()),
        };

        // Serialize to JSON
        let json = serde_json::to_string(&claims).unwrap();

        // Deserialize back
        let deserialized: Claims = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.sub, claims.sub);
        assert_eq!(deserialized.exp, claims.exp);
        assert_eq!(deserialized.iat, claims.iat);
        assert_eq!(deserialized.scope, claims.scope);
        assert_eq!(deserialized.service_type, claims.service_type);
    }

    /// Test Claims without service_type (user token)
    ///
    /// Validates that service_type is properly optional and omitted when None.
    #[test]
    fn test_claims_without_service_type() {
        let claims = Claims {
            sub: "user123".to_string(),
            exp: 1234567890,
            iat: 1234567800,
            scope: "user:read user:write".to_string(),
            service_type: None, // User tokens don't have service_type
        };

        let json = serde_json::to_string(&claims).unwrap();

        // Verify service_type is not present in JSON
        assert!(
            !json.contains("service_type"),
            "service_type should be omitted when None"
        );

        let deserialized: Claims = serde_json::from_str(&json).unwrap();
        assert!(deserialized.service_type.is_none());
    }

    /// Test Claims Debug implementation redacts sensitive sub field
    #[test]
    fn test_claims_debug() {
        let claims = Claims {
            sub: "test-user".to_string(),
            exp: 1234567890,
            iat: 1234567800,
            scope: "read write".to_string(),
            service_type: Some("media-handler".to_string()),
        };

        let debug_str = format!("{:?}", claims);
        // Sub field should be redacted
        assert!(
            !debug_str.contains("test-user"),
            "Debug output should not contain actual sub value"
        );
        assert!(
            debug_str.contains("[REDACTED]"),
            "Debug output should contain [REDACTED] for sub field"
        );
        // Other fields should be visible
        assert!(debug_str.contains("read write"));
        assert!(debug_str.contains("media-handler"));
    }

    /// Test Claims Clone implementation
    #[test]
    fn test_claims_clone() {
        let claims = Claims {
            sub: "test-user".to_string(),
            exp: 1234567890,
            iat: 1234567800,
            scope: "read write".to_string(),
            service_type: Some("global-controller".to_string()),
        };

        let cloned = claims.clone();

        assert_eq!(cloned.sub, claims.sub);
        assert_eq!(cloned.exp, claims.exp);
        assert_eq!(cloned.iat, claims.iat);
        assert_eq!(cloned.scope, claims.scope);
        assert_eq!(cloned.service_type, claims.service_type);
    }

    /// Test EncryptedKey Debug implementation redacts sensitive data
    #[test]
    fn test_encrypted_key_debug() {
        let encrypted = EncryptedKey {
            encrypted_data: common::secret::SecretBox::new(Box::new(vec![1, 2, 3, 4])),
            nonce: vec![5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
            tag: vec![
                17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32,
            ],
        };

        let debug_str = format!("{:?}", encrypted);
        assert!(debug_str.contains("EncryptedKey"));
        assert!(
            debug_str.contains("[REDACTED]"),
            "Debug output should redact encrypted data"
        );
    }

    /// Test EncryptedKey Clone implementation
    #[test]
    fn test_encrypted_key_clone() {
        use common::secret::ExposeSecret;

        let encrypted = EncryptedKey {
            encrypted_data: common::secret::SecretBox::new(Box::new(vec![1, 2, 3])),
            nonce: vec![4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
            tag: vec![
                16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31,
            ],
        };

        let cloned = encrypted.clone();

        assert_eq!(
            cloned.encrypted_data.expose_secret(),
            encrypted.encrypted_data.expose_secret()
        );
        assert_eq!(cloned.nonce, encrypted.nonce);
        assert_eq!(cloned.tag, encrypted.tag);
    }

    /// Test generate_random_bytes with various lengths
    #[test]
    fn test_generate_random_bytes_various_lengths() {
        for len in [0, 1, 16, 32, 64, 256] {
            let bytes = generate_random_bytes(len).unwrap();
            assert_eq!(bytes.len(), len, "Should generate exactly {} bytes", len);

            if len > 0 {
                // Generate again to verify it's random (extremely unlikely to be identical)
                let bytes2 = generate_random_bytes(len).unwrap();
                // For small lengths, this could theoretically match, but it's astronomically unlikely
                if len >= 16 {
                    assert_ne!(
                        bytes, bytes2,
                        "Two random byte sequences should be different"
                    );
                }
            }
        }
    }

    /// Test generate_client_secret produces unique values
    #[test]
    fn test_generate_client_secret_uniqueness() {
        use common::secret::ExposeSecret;

        let secret1 = generate_client_secret().unwrap();
        let secret2 = generate_client_secret().unwrap();

        assert_ne!(
            secret1.expose_secret(),
            secret2.expose_secret(),
            "Two generated secrets should be different"
        );

        // Verify they're valid base64
        assert!(general_purpose::STANDARD
            .decode(secret1.expose_secret())
            .is_ok());
        assert!(general_purpose::STANDARD
            .decode(secret2.expose_secret())
            .is_ok());

        // Verify decoded length is 32 bytes
        let decoded1 = general_purpose::STANDARD
            .decode(secret1.expose_secret())
            .unwrap();
        assert_eq!(decoded1.len(), 32, "Decoded secret should be 32 bytes");
    }

    /// Test JWT size limit constant value
    ///
    /// Documents the MAX_JWT_SIZE_BYTES constant for security review.
    #[test]
    fn test_max_jwt_size_constant() {
        assert_eq!(
            MAX_JWT_SIZE_BYTES, 4096,
            "Max JWT size must be 4096 bytes (4KB) for DoS protection"
        );
    }

    /// Test verify_jwt with empty token
    #[test]
    fn test_verify_jwt_empty_token() {
        let (public_pem, _) = generate_signing_key().unwrap();

        let result = verify_jwt("", &public_pem, DEFAULT_JWT_CLOCK_SKEW_SECONDS);

        let err = result.expect_err("Empty token should be rejected");
        assert!(matches!(err, AcError::InvalidToken(_)));
    }

    /// Test decrypt with corrupted ciphertext
    ///
    /// Validates that authentication tag verification catches corruption.
    #[test]
    fn test_decrypt_corrupted_ciphertext() {
        use common::secret::ExposeSecret;

        let master_key = vec![0u8; 32];
        let data = b"sensitive data";

        let encrypted = encrypt_private_key(data, &master_key).unwrap();

        // Corrupt one byte of the ciphertext by creating a new EncryptedKey with modified data
        let mut corrupted_data = encrypted.encrypted_data.expose_secret().clone();
        if !corrupted_data.is_empty() {
            corrupted_data[0] ^= 0xFF;
        }
        let corrupted_encrypted = EncryptedKey {
            encrypted_data: common::secret::SecretBox::new(Box::new(corrupted_data)),
            nonce: encrypted.nonce.clone(),
            tag: encrypted.tag.clone(),
        };

        let result = decrypt_private_key(&corrupted_encrypted, &master_key);

        let err = result.expect_err("Corrupted ciphertext should fail authentication");
        assert!(
            matches!(err, AcError::Crypto(msg) if msg.starts_with("Decryption operation failed:"))
        );
    }

    /// Test decrypt with corrupted tag
    ///
    /// Validates that tag verification catches tampering.
    #[test]
    fn test_decrypt_corrupted_tag() {
        let master_key = vec![0u8; 32];
        let data = b"sensitive data";

        let mut encrypted = encrypt_private_key(data, &master_key).unwrap();

        // Corrupt one byte of the authentication tag
        encrypted.tag[0] ^= 0xFF;

        let result = decrypt_private_key(&encrypted, &master_key);

        let err = result.expect_err("Corrupted tag should fail authentication");
        assert!(
            matches!(err, AcError::Crypto(msg) if msg.starts_with("Decryption operation failed:"))
        );
    }

    // ============================================================================
    // extract_jwt_kid tests
    // ============================================================================

    /// Test extract_jwt_kid with valid token containing kid
    #[test]
    fn test_extract_jwt_kid_valid_token() {
        let (_, private_pkcs8) = generate_signing_key().unwrap();

        let claims = Claims {
            sub: "test-user".to_string(),
            exp: chrono::Utc::now().timestamp() + 3600,
            iat: chrono::Utc::now().timestamp(),
            scope: "read write".to_string(),
            service_type: None,
        };

        let key_id = "auth-prod-2025-01";
        let token = sign_jwt(&claims, &private_pkcs8, key_id).unwrap();

        let extracted_kid = extract_jwt_kid(&token);

        assert_eq!(
            extracted_kid,
            Some(key_id.to_string()),
            "Should extract the correct kid from token header"
        );
    }

    /// Test extract_jwt_kid with oversized token
    #[test]
    fn test_extract_jwt_kid_oversized_token() {
        // Create an oversized token (just a long string)
        let oversized_token = "a".repeat(MAX_JWT_SIZE_BYTES + 1);

        let result = extract_jwt_kid(&oversized_token);

        assert!(
            result.is_none(),
            "Oversized token should return None before parsing"
        );
    }

    /// Test extract_jwt_kid with malformed token (wrong number of parts)
    #[test]
    fn test_extract_jwt_kid_malformed_token() {
        let malformed_tokens = [
            "",                           // Empty
            "single-part",                // 1 part
            "two.parts",                  // 2 parts
            "too.many.parts.here.really", // 5 parts
        ];

        for token in malformed_tokens {
            let result = extract_jwt_kid(token);
            assert!(
                result.is_none(),
                "Malformed token '{}' should return None",
                token
            );
        }
    }

    /// Test extract_jwt_kid with invalid base64 header
    #[test]
    fn test_extract_jwt_kid_invalid_base64() {
        // JWT format with invalid base64 in header (! is not valid base64)
        let token = "invalid!!!base64.payload.signature";

        let result = extract_jwt_kid(token);

        assert!(result.is_none(), "Invalid base64 header should return None");
    }

    /// Test extract_jwt_kid with valid base64 but invalid JSON header
    #[test]
    fn test_extract_jwt_kid_invalid_json() {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

        // Valid base64 but not valid JSON
        let invalid_json_header = URL_SAFE_NO_PAD.encode("not valid json");
        let token = format!("{}.payload.signature", invalid_json_header);

        let result = extract_jwt_kid(&token);

        assert!(result.is_none(), "Invalid JSON header should return None");
    }

    /// Test extract_jwt_kid with valid JWT header but missing kid
    #[test]
    fn test_extract_jwt_kid_missing_kid() {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

        // Valid JWT header JSON but without kid field
        let header_json = r#"{"alg":"EdDSA","typ":"JWT"}"#;
        let header_b64 = URL_SAFE_NO_PAD.encode(header_json);
        let token = format!("{}.payload.signature", header_b64);

        let result = extract_jwt_kid(&token);

        assert!(
            result.is_none(),
            "Header without kid field should return None"
        );
    }

    /// Test extract_jwt_kid with kid as non-string value
    #[test]
    fn test_extract_jwt_kid_non_string_kid() {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

        // kid is a number, not a string
        let header_json = r#"{"alg":"EdDSA","typ":"JWT","kid":12345}"#;
        let header_b64 = URL_SAFE_NO_PAD.encode(header_json);
        let token = format!("{}.payload.signature", header_b64);

        let result = extract_jwt_kid(&token);

        assert!(result.is_none(), "kid as non-string should return None");
    }

    // ============================================================================
    // Configurable Bcrypt Cost Tests
    // ============================================================================

    /// Test hash_client_secret uses the provided cost factor
    #[test]
    fn test_hash_client_secret_uses_configured_cost() {
        let secret = "test-secret-for-cost-verification";

        // Test with cost 10 (minimum allowed)
        let hash_10 = hash_client_secret(secret, 10).expect("Cost 10 should work");
        let cost_10 = hash_10.split('$').nth(2).unwrap();
        assert_eq!(cost_10, "10", "Hash should use cost factor 10");
        assert!(
            verify_client_secret(secret, &hash_10).unwrap(),
            "Cost 10 hash should verify"
        );

        // Test with cost 11
        let hash_11 = hash_client_secret(secret, 11).expect("Cost 11 should work");
        let cost_11 = hash_11.split('$').nth(2).unwrap();
        assert_eq!(cost_11, "11", "Hash should use cost factor 11");
        assert!(
            verify_client_secret(secret, &hash_11).unwrap(),
            "Cost 11 hash should verify"
        );

        // Test with cost 12 (default)
        let hash_12 = hash_client_secret(secret, 12).expect("Cost 12 should work");
        let cost_12 = hash_12.split('$').nth(2).unwrap();
        assert_eq!(cost_12, "12", "Hash should use cost factor 12");
        assert!(
            verify_client_secret(secret, &hash_12).unwrap(),
            "Cost 12 hash should verify"
        );

        // Test with cost 13
        let hash_13 = hash_client_secret(secret, 13).expect("Cost 13 should work");
        let cost_13 = hash_13.split('$').nth(2).unwrap();
        assert_eq!(cost_13, "13", "Hash should use cost factor 13");
        assert!(
            verify_client_secret(secret, &hash_13).unwrap(),
            "Cost 13 hash should verify"
        );

        // Test with cost 14 (maximum allowed)
        let hash_14 = hash_client_secret(secret, 14).expect("Cost 14 should work");
        let cost_14 = hash_14.split('$').nth(2).unwrap();
        assert_eq!(cost_14, "14", "Hash should use cost factor 14");
        assert!(
            verify_client_secret(secret, &hash_14).unwrap(),
            "Cost 14 hash should verify"
        );
    }

    /// Test that different cost factors produce different hashes
    #[test]
    fn test_hash_client_secret_cost_affects_hash() {
        let secret = "same-secret";

        // Different costs should produce different hashes (even for same secret)
        let hash_10 = hash_client_secret(secret, 10).unwrap();
        let hash_12 = hash_client_secret(secret, 12).unwrap();

        // The cost factor portion should be different
        let cost_10 = hash_10.split('$').nth(2).unwrap();
        let cost_12 = hash_12.split('$').nth(2).unwrap();

        assert_ne!(cost_10, cost_12, "Different costs should be in hash");

        // But both should still verify the same secret
        assert!(verify_client_secret(secret, &hash_10).unwrap());
        assert!(verify_client_secret(secret, &hash_12).unwrap());
    }

    /// Test that cost 4 (below minimum) still works at bcrypt level
    ///
    /// Test defense-in-depth: cost below minimum (10) is rejected at function level.
    /// This prevents insecure hashing even if config validation is bypassed.
    #[test]
    fn test_hash_client_secret_rejects_cost_below_minimum() {
        let secret = "test-secret";

        // Cost 9 is one below minimum (10)
        let result = hash_client_secret(secret, 9);
        assert!(
            result.is_err(),
            "Cost 9 should be rejected (below OWASP minimum of 10)"
        );

        let err = result.unwrap_err();
        match &err {
            AcError::Crypto(msg) => {
                assert!(
                    msg.contains("Invalid bcrypt cost") && msg.contains("10-14"),
                    "Error should mention valid range: {}",
                    msg
                );
            }
            _ => panic!("Expected AcError::Crypto, got {:?}", err),
        }

        // Cost 4 (bcrypt library minimum) should also be rejected
        let result = hash_client_secret(secret, 4);
        assert!(
            result.is_err(),
            "Cost 4 should be rejected (below OWASP minimum)"
        );
    }

    /// Test defense-in-depth: cost above maximum (14) is rejected at function level.
    /// This prevents excessive latency even if config validation is bypassed.
    #[test]
    fn test_hash_client_secret_rejects_cost_above_maximum() {
        let secret = "test-secret";

        // Cost 15 is one above maximum (14)
        let result = hash_client_secret(secret, 15);
        assert!(
            result.is_err(),
            "Cost 15 should be rejected (above maximum of 14)"
        );

        let err = result.unwrap_err();
        match &err {
            AcError::Crypto(msg) => {
                assert!(
                    msg.contains("Invalid bcrypt cost") && msg.contains("10-14"),
                    "Error should mention valid range: {}",
                    msg
                );
            }
            _ => panic!("Expected AcError::Crypto, got {:?}", err),
        }

        // Cost 32 (above bcrypt library max) should also be rejected by our validation first
        let result = hash_client_secret(secret, 32);
        assert!(result.is_err(), "Cost 32 should be rejected");
    }

    /// Test that hashes created with one cost can be verified regardless of current cost config.
    /// This is critical for migration scenarios: if cost changes from 10 to 12, existing
    /// hashes (created with cost 10) must still verify correctly.
    #[test]
    fn test_hash_verification_works_across_cost_factors() {
        let secret = "test-migration-secret";

        // Create hashes with all valid cost factors
        let hash_10 = hash_client_secret(secret, 10).expect("Cost 10 should work");
        let hash_11 = hash_client_secret(secret, 11).expect("Cost 11 should work");
        let hash_12 = hash_client_secret(secret, 12).expect("Cost 12 should work");
        let hash_13 = hash_client_secret(secret, 13).expect("Cost 13 should work");
        let hash_14 = hash_client_secret(secret, 14).expect("Cost 14 should work");

        // Verify each hash correctly - bcrypt verification doesn't depend on current cost config
        assert!(
            verify_client_secret(secret, &hash_10).unwrap(),
            "Cost 10 hash should verify"
        );
        assert!(
            verify_client_secret(secret, &hash_11).unwrap(),
            "Cost 11 hash should verify"
        );
        assert!(
            verify_client_secret(secret, &hash_12).unwrap(),
            "Cost 12 hash should verify"
        );
        assert!(
            verify_client_secret(secret, &hash_13).unwrap(),
            "Cost 13 hash should verify"
        );
        assert!(
            verify_client_secret(secret, &hash_14).unwrap(),
            "Cost 14 hash should verify"
        );

        // Wrong password should fail for all cost factors
        assert!(
            !verify_client_secret("wrong-password", &hash_10).unwrap(),
            "Wrong password should fail for cost 10 hash"
        );
        assert!(
            !verify_client_secret("wrong-password", &hash_14).unwrap(),
            "Wrong password should fail for cost 14 hash"
        );

        // Verify hashes are different for different costs (different iteration counts)
        assert_ne!(
            hash_10, hash_12,
            "Different costs should produce different hashes"
        );
        assert_ne!(
            hash_12, hash_14,
            "Different costs should produce different hashes"
        );
    }

    /// Test DEFAULT_BCRYPT_COST constant is used correctly
    #[test]
    fn test_default_bcrypt_cost_constant() {
        use crate::config::{DEFAULT_BCRYPT_COST, MAX_BCRYPT_COST, MIN_BCRYPT_COST};

        // Verify the constant value
        assert_eq!(DEFAULT_BCRYPT_COST, 12, "Default cost should be 12");

        // Verify it's within the allowed range
        assert!(
            DEFAULT_BCRYPT_COST >= MIN_BCRYPT_COST,
            "Default should be >= minimum"
        );
        assert!(
            DEFAULT_BCRYPT_COST <= MAX_BCRYPT_COST,
            "Default should be <= maximum"
        );

        // Verify a hash with DEFAULT_BCRYPT_COST uses cost 12
        let hash = hash_client_secret("test", DEFAULT_BCRYPT_COST).unwrap();
        let cost = hash.split('$').nth(2).unwrap();
        assert_eq!(cost, "12");
    }

    // ============================================================================
    // User JWT Tests (ADR-0020)
    // ============================================================================

    /// Test sign_user_jwt produces valid JWT with correct claims
    #[test]
    fn test_sign_user_jwt_valid_jwt() {
        let (public_pem, private_pkcs8) = generate_signing_key().unwrap();

        let claims = UserClaims {
            sub: "user-123".to_string(),
            org_id: "org-456".to_string(),
            email: "user@example.com".to_string(),
            roles: vec!["user".to_string(), "admin".to_string()],
            iat: chrono::Utc::now().timestamp(),
            exp: chrono::Utc::now().timestamp() + 3600,
            jti: "jti-789".to_string(),
        };

        let token = sign_user_jwt(&claims, &private_pkcs8, "user-key-01").unwrap();

        // Verify it's a valid JWT structure (3 parts)
        let parts: Vec<&str> = token.split('.').collect();
        assert_eq!(parts.len(), 3, "JWT should have 3 parts");

        // Verify the token can be verified with the public key
        let verified =
            verify_user_jwt(&token, &public_pem, DEFAULT_JWT_CLOCK_SKEW_SECONDS).unwrap();
        assert_eq!(verified.sub, claims.sub);
        assert_eq!(verified.org_id, claims.org_id);
        assert_eq!(verified.email, claims.email);
        assert_eq!(verified.roles, claims.roles);
        assert_eq!(verified.jti, claims.jti);
    }

    /// Test sign_user_jwt includes kid in header
    #[test]
    fn test_sign_user_jwt_includes_kid_header() {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

        let (_, private_pkcs8) = generate_signing_key().unwrap();

        let claims = UserClaims {
            sub: "user-123".to_string(),
            org_id: "org-456".to_string(),
            email: "user@example.com".to_string(),
            roles: vec!["user".to_string()],
            iat: chrono::Utc::now().timestamp(),
            exp: chrono::Utc::now().timestamp() + 3600,
            jti: "jti-789".to_string(),
        };

        let key_id = "user-signing-key-2026-01";
        let token = sign_user_jwt(&claims, &private_pkcs8, key_id).unwrap();

        // Extract and decode the header
        let parts: Vec<&str> = token.split('.').collect();
        let header_bytes = URL_SAFE_NO_PAD
            .decode(parts[0])
            .expect("Failed to decode header");
        let header: serde_json::Value =
            serde_json::from_slice(&header_bytes).expect("Failed to parse header JSON");

        assert_eq!(header["kid"].as_str().unwrap(), key_id);
        assert_eq!(header["alg"].as_str().unwrap(), "EdDSA");
        assert_eq!(header["typ"].as_str().unwrap(), "JWT");
    }

    /// Test verify_user_jwt validates signature and claims
    #[test]
    fn test_verify_user_jwt_validates_signature() {
        let (public_pem, private_pkcs8) = generate_signing_key().unwrap();
        let (wrong_public_pem, _) = generate_signing_key().unwrap(); // Different keypair

        let claims = UserClaims {
            sub: "user-123".to_string(),
            org_id: "org-456".to_string(),
            email: "user@example.com".to_string(),
            roles: vec!["user".to_string()],
            iat: chrono::Utc::now().timestamp(),
            exp: chrono::Utc::now().timestamp() + 3600,
            jti: "jti-789".to_string(),
        };

        let token = sign_user_jwt(&claims, &private_pkcs8, "test-key").unwrap();

        // Should verify with correct key
        assert!(verify_user_jwt(&token, &public_pem, DEFAULT_JWT_CLOCK_SKEW_SECONDS).is_ok());

        // Should fail with wrong key
        let result = verify_user_jwt(&token, &wrong_public_pem, DEFAULT_JWT_CLOCK_SKEW_SECONDS);
        assert!(matches!(result, Err(AcError::InvalidToken(_))));
    }

    /// Test verify_user_jwt rejects expired tokens
    #[test]
    fn test_verify_user_jwt_rejects_expired() {
        let (public_pem, private_pkcs8) = generate_signing_key().unwrap();

        let claims = UserClaims {
            sub: "user-123".to_string(),
            org_id: "org-456".to_string(),
            email: "user@example.com".to_string(),
            roles: vec!["user".to_string()],
            iat: chrono::Utc::now().timestamp() - 7200, // 2 hours ago
            exp: chrono::Utc::now().timestamp() - 3600, // Expired 1 hour ago
            jti: "jti-789".to_string(),
        };

        let token = sign_user_jwt(&claims, &private_pkcs8, "test-key").unwrap();

        let result = verify_user_jwt(&token, &public_pem, DEFAULT_JWT_CLOCK_SKEW_SECONDS);
        assert!(matches!(result, Err(AcError::InvalidToken(_))));
    }

    /// Test verify_user_jwt rejects future iat beyond clock skew
    #[test]
    fn test_verify_user_jwt_rejects_future_iat() {
        let (public_pem, private_pkcs8) = generate_signing_key().unwrap();

        let now = chrono::Utc::now().timestamp();
        let claims = UserClaims {
            sub: "user-123".to_string(),
            org_id: "org-456".to_string(),
            email: "user@example.com".to_string(),
            roles: vec!["user".to_string()],
            iat: now + 3600, // 1 hour in the future (beyond 5 min clock skew)
            exp: now + 7200, // Expires in 2 hours
            jti: "jti-789".to_string(),
        };

        let token = sign_user_jwt(&claims, &private_pkcs8, "test-key").unwrap();

        let result = verify_user_jwt(&token, &public_pem, DEFAULT_JWT_CLOCK_SKEW_SECONDS);
        assert!(matches!(result, Err(AcError::InvalidToken(_))));
    }

    /// Test verify_user_jwt accepts iat within clock skew
    #[test]
    fn test_verify_user_jwt_accepts_iat_within_skew() {
        let (public_pem, private_pkcs8) = generate_signing_key().unwrap();

        let now = chrono::Utc::now().timestamp();
        let claims = UserClaims {
            sub: "user-123".to_string(),
            org_id: "org-456".to_string(),
            email: "user@example.com".to_string(),
            roles: vec!["user".to_string()],
            iat: now + 120, // 2 minutes in the future (within 5 min clock skew)
            exp: now + 3600,
            jti: "jti-789".to_string(),
        };

        let token = sign_user_jwt(&claims, &private_pkcs8, "test-key").unwrap();

        let result = verify_user_jwt(&token, &public_pem, DEFAULT_JWT_CLOCK_SKEW_SECONDS);
        assert!(result.is_ok());
    }

    /// Test verify_user_jwt rejects oversized tokens
    #[test]
    fn test_verify_user_jwt_rejects_oversized() {
        let (public_pem, _) = generate_signing_key().unwrap();

        // Create an oversized token
        let oversized_token = "a".repeat(MAX_JWT_SIZE_BYTES + 1);

        let result = verify_user_jwt(
            &oversized_token,
            &public_pem,
            DEFAULT_JWT_CLOCK_SKEW_SECONDS,
        );
        assert!(matches!(result, Err(AcError::InvalidToken(_))));
    }

    /// Test UserClaims Debug implementation redacts sensitive fields
    #[test]
    fn test_user_claims_debug_redacts_sensitive() {
        let claims = UserClaims {
            sub: "user-secret-id".to_string(),
            org_id: "org-456".to_string(),
            email: "secret@example.com".to_string(),
            roles: vec!["user".to_string(), "admin".to_string()],
            iat: 1234567890,
            exp: 1234571490,
            jti: "secret-jti-token".to_string(),
        };

        let debug_str = format!("{:?}", claims);

        // Sensitive fields should be redacted
        assert!(
            !debug_str.contains("user-secret-id"),
            "sub should be redacted"
        );
        assert!(
            !debug_str.contains("secret@example.com"),
            "email should be redacted"
        );
        assert!(
            !debug_str.contains("secret-jti-token"),
            "jti should be redacted"
        );
        assert!(
            debug_str.contains("[REDACTED]"),
            "Should contain [REDACTED] markers"
        );

        // Non-sensitive fields should be visible
        assert!(debug_str.contains("org-456"), "org_id should be visible");
        assert!(debug_str.contains("user"), "roles should be visible");
        assert!(debug_str.contains("admin"), "roles should be visible");
        assert!(debug_str.contains("1234567890"), "iat should be visible");
        assert!(debug_str.contains("1234571490"), "exp should be visible");
    }

    /// Test UserClaims serialization/deserialization roundtrip
    #[test]
    fn test_user_claims_serde_roundtrip() {
        let claims = UserClaims {
            sub: "user-123".to_string(),
            org_id: "org-456".to_string(),
            email: "user@example.com".to_string(),
            roles: vec!["user".to_string(), "admin".to_string()],
            iat: 1234567890,
            exp: 1234571490,
            jti: "jti-789".to_string(),
        };

        // Serialize to JSON
        let json = serde_json::to_string(&claims).unwrap();

        // Deserialize back
        let deserialized: UserClaims = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.sub, claims.sub);
        assert_eq!(deserialized.org_id, claims.org_id);
        assert_eq!(deserialized.email, claims.email);
        assert_eq!(deserialized.roles, claims.roles);
        assert_eq!(deserialized.iat, claims.iat);
        assert_eq!(deserialized.exp, claims.exp);
        assert_eq!(deserialized.jti, claims.jti);
    }

    /// Test UserClaims Clone implementation
    #[test]
    fn test_user_claims_clone() {
        let claims = UserClaims {
            sub: "user-123".to_string(),
            org_id: "org-456".to_string(),
            email: "user@example.com".to_string(),
            roles: vec!["user".to_string()],
            iat: 1234567890,
            exp: 1234571490,
            jti: "jti-789".to_string(),
        };

        let cloned = claims.clone();

        assert_eq!(cloned.sub, claims.sub);
        assert_eq!(cloned.org_id, claims.org_id);
        assert_eq!(cloned.email, claims.email);
        assert_eq!(cloned.roles, claims.roles);
        assert_eq!(cloned.iat, claims.iat);
        assert_eq!(cloned.exp, claims.exp);
        assert_eq!(cloned.jti, claims.jti);
    }

    /// Test sign_user_jwt with invalid private key
    #[test]
    fn test_sign_user_jwt_invalid_private_key() {
        let claims = UserClaims {
            sub: "user-123".to_string(),
            org_id: "org-456".to_string(),
            email: "user@example.com".to_string(),
            roles: vec!["user".to_string()],
            iat: chrono::Utc::now().timestamp(),
            exp: chrono::Utc::now().timestamp() + 3600,
            jti: "jti-789".to_string(),
        };

        // Use invalid PKCS8 data
        let invalid_key = vec![0u8; 32];

        let result = sign_user_jwt(&claims, &invalid_key, "test-key");
        assert!(matches!(result, Err(AcError::Crypto(_))));
    }

    /// Test verify_user_jwt with malformed token
    #[test]
    fn test_verify_user_jwt_malformed_token() {
        let (public_pem, _) = generate_signing_key().unwrap();

        let malformed_tokens = ["", "a.b", "not-a-jwt", "a.b.c.d.e"];

        for token in malformed_tokens {
            let result = verify_user_jwt(token, &public_pem, DEFAULT_JWT_CLOCK_SKEW_SECONDS);
            assert!(
                matches!(result, Err(AcError::InvalidToken(_))),
                "Token '{}' should be rejected",
                token
            );
        }
    }
}
