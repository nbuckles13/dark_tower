use crate::errors::AcError;
use base64::{Engine as _, engine::general_purpose};
use jsonwebtoken::{encode, decode, Header, Validation, Algorithm, EncodingKey, DecodingKey};
use ring::{
    aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM},
    rand::{SecureRandom, SystemRandom},
    signature::{Ed25519KeyPair, KeyPair},
};
use serde::{Deserialize, Serialize};

/// JWT Claims structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,           // Subject (user_id or client_id)
    pub exp: i64,              // Expiration timestamp
    pub iat: i64,              // Issued at timestamp
    pub scope: String,         // Space-separated scopes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_type: Option<String>,  // Service type for service tokens
}

/// Encrypted key structure (AES-256-GCM)
#[derive(Debug, Clone)]
pub struct EncryptedKey {
    pub encrypted_data: Vec<u8>,
    pub nonce: Vec<u8>,        // 96-bit (12 bytes)
    pub tag: Vec<u8>,          // 128-bit (16 bytes)
}

/// Generate EdDSA (Ed25519) keypair using CSPRNG
///
/// Returns (public_key_pem, private_key_pkcs8)
pub fn generate_signing_key() -> Result<(String, Vec<u8>), AcError> {
    let rng = SystemRandom::new();

    // Generate Ed25519 keypair in PKCS8 format
    let pkcs8_bytes = Ed25519KeyPair::generate_pkcs8(&rng)
        .map_err(|e| {
            tracing::error!(target: "crypto", error = ?e, "Keypair generation failed");
            AcError::Crypto("Key generation failed".to_string())
        })?;

    let key_pair = Ed25519KeyPair::from_pkcs8(pkcs8_bytes.as_ref())
        .map_err(|e| {
            tracing::error!(target: "crypto", error = ?e, "Keypair parsing failed");
            AcError::Crypto("Key generation failed".to_string())
        })?;

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
pub fn encrypt_private_key(private_key: &[u8], master_key: &[u8]) -> Result<EncryptedKey, AcError> {
    if master_key.len() != 32 {
        tracing::error!(target: "crypto", "Invalid master key length: {}", master_key.len());
        return Err(AcError::Crypto("Invalid encryption key".to_string()));
    }

    let rng = SystemRandom::new();

    // Generate random 96-bit nonce (12 bytes)
    let mut nonce_bytes = [0u8; 12];
    rng.fill(&mut nonce_bytes)
        .map_err(|e| {
            tracing::error!(target: "crypto", error = ?e, "Nonce generation failed");
            AcError::Crypto("Encryption failed".to_string())
        })?;

    let nonce = Nonce::assume_unique_for_key(nonce_bytes);

    // Create AES-256-GCM cipher
    let unbound_key = UnboundKey::new(&AES_256_GCM, master_key)
        .map_err(|e| {
            tracing::error!(target: "crypto", error = ?e, "Cipher key creation failed");
            AcError::Crypto("Encryption failed".to_string())
        })?;
    let sealing_key = LessSafeKey::new(unbound_key);

    // Encrypt the private key (in-place operation requires mutable buffer)
    let mut in_out = private_key.to_vec();
    sealing_key
        .seal_in_place_append_tag(nonce, Aad::empty(), &mut in_out)
        .map_err(|e| {
            tracing::error!(target: "crypto", error = ?e, "Encryption operation failed");
            AcError::Crypto("Encryption failed".to_string())
        })?;

    // Split ciphertext and tag (last 16 bytes are the tag)
    let tag_start = in_out.len() - 16;
    let encrypted_data = in_out[..tag_start].to_vec();
    let tag = in_out[tag_start..].to_vec();

    Ok(EncryptedKey {
        encrypted_data,
        nonce: nonce_bytes.to_vec(),
        tag,
    })
}

/// Decrypt private key with AES-256-GCM
pub fn decrypt_private_key(encrypted: &EncryptedKey, master_key: &[u8]) -> Result<Vec<u8>, AcError> {
    if master_key.len() != 32 {
        tracing::error!(target: "crypto", "Invalid master key length: {}", master_key.len());
        return Err(AcError::Crypto("Invalid decryption key".to_string()));
    }

    if encrypted.nonce.len() != 12 {
        tracing::error!(target: "crypto", "Invalid nonce length: {}", encrypted.nonce.len());
        return Err(AcError::Crypto("Decryption failed".to_string()));
    }

    if encrypted.tag.len() != 16 {
        tracing::error!(target: "crypto", "Invalid tag length: {}", encrypted.tag.len());
        return Err(AcError::Crypto("Decryption failed".to_string()));
    }

    // Reconstruct ciphertext with tag
    let mut in_out = encrypted.encrypted_data.clone();
    in_out.extend_from_slice(&encrypted.tag);

    let nonce_bytes: [u8; 12] = encrypted.nonce.as_slice().try_into()
        .map_err(|_| {
            tracing::error!(target: "crypto", "Invalid nonce format");
            AcError::Crypto("Decryption failed".to_string())
        })?;
    let nonce = Nonce::assume_unique_for_key(nonce_bytes);

    // Create AES-256-GCM cipher
    let unbound_key = UnboundKey::new(&AES_256_GCM, master_key)
        .map_err(|e| {
            tracing::error!(target: "crypto", error = ?e, "Cipher key creation failed");
            AcError::Crypto("Decryption failed".to_string())
        })?;
    let opening_key = LessSafeKey::new(unbound_key);

    // Decrypt in place
    let decrypted = opening_key
        .open_in_place(nonce, Aad::empty(), &mut in_out)
        .map_err(|e| {
            tracing::error!(target: "crypto", error = ?e, "Decryption operation failed");
            AcError::Crypto("Decryption failed".to_string())
        })?;

    Ok(decrypted.to_vec())
}

/// Sign JWT with EdDSA private key
pub fn sign_jwt(claims: &Claims, private_key_pkcs8: &[u8]) -> Result<String, AcError> {
    // Validate the private key format
    let _key_pair = Ed25519KeyPair::from_pkcs8(private_key_pkcs8)
        .map_err(|e| {
            tracing::error!(target: "crypto", error = ?e, "Invalid private key format");
            AcError::Crypto("JWT signing failed".to_string())
        })?;

    // Get the raw private key bytes for jsonwebtoken
    // Ed25519KeyPair doesn't expose the seed directly, so we need to use the PKCS8 format
    let encoding_key = EncodingKey::from_ed_der(private_key_pkcs8);

    let mut header = Header::new(Algorithm::EdDSA);
    header.typ = Some("JWT".to_string());

    let token = encode(&header, claims, &encoding_key)
        .map_err(|e| {
            tracing::error!(target: "crypto", error = ?e, "JWT signing operation failed");
            AcError::Crypto("JWT signing failed".to_string())
        })?;

    Ok(token)
}

/// Verify JWT with EdDSA public key
pub fn verify_jwt(token: &str, public_key_pem: &str) -> Result<Claims, AcError> {
    // Extract base64 from PEM format
    let public_key_b64 = public_key_pem
        .lines()
        .filter(|line| !line.starts_with("-----"))
        .collect::<String>();

    let public_key_bytes = general_purpose::STANDARD
        .decode(&public_key_b64)
        .map_err(|e| {
            tracing::debug!(target: "crypto", error = ?e, "Invalid public key encoding");
            AcError::InvalidToken("The access token is invalid or expired".to_string())
        })?;

    let decoding_key = DecodingKey::from_ed_der(&public_key_bytes);

    let mut validation = Validation::new(Algorithm::EdDSA);
    validation.validate_exp = true;

    let token_data = decode::<Claims>(token, &decoding_key, &validation)
        .map_err(|e| {
            tracing::debug!(target: "crypto", error = ?e, "Token verification failed");
            AcError::InvalidToken("The access token is invalid or expired".to_string())
        })?;

    Ok(token_data.claims)
}

/// Hash client secret with bcrypt (cost factor 12)
pub fn hash_client_secret(secret: &str) -> Result<String, AcError> {
    bcrypt::hash(secret, 12)
        .map_err(|e| {
            tracing::error!(target: "crypto", error = ?e, "Password hashing failed");
            AcError::Crypto("Password hashing failed".to_string())
        })
}

/// Verify client secret against bcrypt hash
pub fn verify_client_secret(secret: &str, hash: &str) -> Result<bool, AcError> {
    bcrypt::verify(secret, hash)
        .map_err(|e| {
            tracing::error!(target: "crypto", error = ?e, "Password verification failed");
            AcError::Crypto("Password verification failed".to_string())
        })
}

/// Generate cryptographically secure random bytes
pub fn generate_random_bytes(len: usize) -> Result<Vec<u8>, AcError> {
    let rng = SystemRandom::new();
    let mut bytes = vec![0u8; len];
    rng.fill(&mut bytes)
        .map_err(|e| {
            tracing::error!(target: "crypto", error = ?e, "Random bytes generation failed");
            AcError::Crypto("Random generation failed".to_string())
        })?;
    Ok(bytes)
}

/// Generate a client secret (32 bytes, base64 encoded)
pub fn generate_client_secret() -> Result<String, AcError> {
    let bytes = generate_random_bytes(32)?;
    Ok(general_purpose::STANDARD.encode(&bytes))
}

#[cfg(test)]
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

        let token = sign_jwt(&claims, &private_pkcs8).unwrap();
        let verified_claims = verify_jwt(&token, &public_pem).unwrap();

        assert_eq!(verified_claims.sub, claims.sub);
        assert_eq!(verified_claims.scope, claims.scope);
    }

    #[test]
    fn test_password_hashing() {
        let secret = "my-secure-secret";
        let hash = hash_client_secret(secret).unwrap();

        assert!(verify_client_secret(secret, &hash).unwrap());
        assert!(!verify_client_secret("wrong-secret", &hash).unwrap());
    }

    #[test]
    fn test_generate_client_secret() {
        let secret = generate_client_secret().unwrap();
        assert!(!secret.is_empty());

        // Should be base64 encoded
        assert!(general_purpose::STANDARD.decode(&secret).is_ok());
    }

    // Error path tests
    #[test]
    fn test_encrypt_with_invalid_master_key_length() {
        let data = b"secret data";
        let wrong_key = vec![0u8; 16]; // Wrong length (should be 32)

        let result = encrypt_private_key(data, &wrong_key);
        assert!(result.is_err());
        match result {
            Err(AcError::Crypto(msg)) => assert_eq!(msg, "Invalid encryption key"),
            _ => panic!("Expected Crypto error"),
        }
    }

    #[test]
    fn test_decrypt_with_wrong_master_key() {
        let master_key = vec![0u8; 32];
        let wrong_key = vec![1u8; 32]; // Different key
        let data = b"secret data";

        let encrypted = encrypt_private_key(data, &master_key).unwrap();
        let result = decrypt_private_key(&encrypted, &wrong_key);

        assert!(result.is_err());
        match result {
            Err(AcError::Crypto(msg)) => assert_eq!(msg, "Decryption failed"),
            _ => panic!("Expected Crypto error"),
        }
    }

    #[test]
    fn test_decrypt_with_invalid_master_key_length() {
        let data = b"secret data";
        let master_key = vec![0u8; 32];
        let wrong_key = vec![0u8; 16]; // Wrong length

        let encrypted = encrypt_private_key(data, &master_key).unwrap();
        let result = decrypt_private_key(&encrypted, &wrong_key);

        assert!(result.is_err());
        match result {
            Err(AcError::Crypto(msg)) => assert_eq!(msg, "Invalid decryption key"),
            _ => panic!("Expected Crypto error"),
        }
    }

    #[test]
    fn test_decrypt_with_invalid_nonce_length() {
        let data = b"secret data";
        let master_key = vec![0u8; 32];

        let mut encrypted = encrypt_private_key(data, &master_key).unwrap();
        encrypted.nonce = vec![0u8; 8]; // Wrong nonce length (should be 12)

        let result = decrypt_private_key(&encrypted, &master_key);
        assert!(result.is_err());
        match result {
            Err(AcError::Crypto(msg)) => assert_eq!(msg, "Decryption failed"),
            _ => panic!("Expected Crypto error"),
        }
    }

    #[test]
    fn test_decrypt_with_invalid_tag_length() {
        let data = b"secret data";
        let master_key = vec![0u8; 32];

        let mut encrypted = encrypt_private_key(data, &master_key).unwrap();
        encrypted.tag = vec![0u8; 8]; // Wrong tag length (should be 16)

        let result = decrypt_private_key(&encrypted, &master_key);
        assert!(result.is_err());
        match result {
            Err(AcError::Crypto(msg)) => assert_eq!(msg, "Decryption failed"),
            _ => panic!("Expected Crypto error"),
        }
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

        let token = sign_jwt(&claims, &private_pkcs8).unwrap();
        let result = verify_jwt(&token, &public_pem);

        assert!(result.is_err());
        match result {
            Err(AcError::InvalidToken(_)) => {},
            _ => panic!("Expected InvalidToken error for expired token"),
        }
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

        let token = sign_jwt(&claims, &private_pkcs8).unwrap();
        let result = verify_jwt(&token, &wrong_public_pem);

        assert!(result.is_err());
        match result {
            Err(AcError::InvalidToken(_)) => {},
            _ => panic!("Expected InvalidToken error for wrong public key"),
        }
    }

    #[test]
    fn test_password_hashing_empty_string() {
        // Empty password should still hash successfully
        let result = hash_client_secret("");
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

        assert!(result.is_err());
        match result {
            Err(AcError::Crypto(msg)) => assert_eq!(msg, "Password verification failed"),
            _ => panic!("Expected Crypto error"),
        }
    }
}
