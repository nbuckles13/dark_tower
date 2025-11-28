use crate::crypto;
use crate::errors::AcError;
use crate::models::{AuthEventType, JsonWebKey, Jwks};
use crate::repositories::{auth_events, signing_keys};
use chrono::{Duration, Utc};
use sqlx::PgPool;

const KEY_VALIDITY_DAYS: i64 = 365; // 1 year

/// Get the next sequence number for a key with given prefix
async fn get_next_key_sequence(pool: &PgPool, prefix: &str) -> Result<u32, AcError> {
    let count: (i64,) = sqlx::query_as(
        r#"
        SELECT COUNT(*)
        FROM signing_keys
        WHERE key_id LIKE $1
        "#,
    )
    .bind(format!("{}%", prefix))
    .fetch_one(pool)
    .await
    .map_err(|e| AcError::Database(format!("Failed to count keys: {}", e)))?;

    Ok((count.0 + 1) as u32)
}

/// Initialize the first signing key if none exists
pub async fn initialize_signing_key(
    pool: &PgPool,
    master_key: &[u8],
    cluster_name: &str,
) -> Result<(), AcError> {
    // Check if any active keys exist
    let active_key = signing_keys::get_active_key(pool).await?;

    if active_key.is_some() {
        // Key already exists, no need to initialize
        return Ok(());
    }

    // Generate key_id: Format 'auth-{cluster}-{YYYY}-{NN}'
    let now = Utc::now();
    let key_prefix = format!("auth-{}-{}-", cluster_name, now.format("%Y"));
    let sequence = get_next_key_sequence(pool, &key_prefix).await?;
    let key_id = format!("{}{:02}", key_prefix, sequence);

    // Generate EdDSA keypair
    let (public_key_pem, private_key_pkcs8) = crypto::generate_signing_key()?;

    // Encrypt private key with master key
    let encrypted = crypto::encrypt_private_key(&private_key_pkcs8, master_key)?;

    // Set validity period
    let valid_from = now;
    let valid_until = now + Duration::days(KEY_VALIDITY_DAYS);

    // Store in database
    signing_keys::create_signing_key(
        pool,
        &key_id,
        &public_key_pem,
        &encrypted.encrypted_data,
        &encrypted.nonce,
        &encrypted.tag,
        1, // master_key_version
        valid_from,
        valid_until,
    )
    .await?;

    // Log key generation
    if let Err(e) = auth_events::log_event(
        pool,
        AuthEventType::KeyGenerated.as_str(),
        None,
        None,
        true,
        None,
        None,
        None,
        Some(serde_json::json!({
            "key_id": key_id,
            "valid_from": valid_from.to_rfc3339(),
            "valid_until": valid_until.to_rfc3339(),
        })),
    )
    .await
    {
        tracing::warn!("Failed to log auth event: {}", e);
    }

    Ok(())
}

/// Rotate signing keys (generate new key, mark old keys as inactive)
#[allow(dead_code)] // Will be used in Phase 4 key rotation endpoints
pub async fn rotate_signing_key(
    pool: &PgPool,
    master_key: &[u8],
    cluster_name: &str,
) -> Result<String, AcError> {
    // Generate new key_id
    let now = Utc::now();
    let key_prefix = format!("auth-{}-{}-", cluster_name, now.format("%Y"));
    let sequence = get_next_key_sequence(pool, &key_prefix).await?;
    let key_id = format!("{}{:02}", key_prefix, sequence);

    // Generate new EdDSA keypair
    let (public_key_pem, private_key_pkcs8) = crypto::generate_signing_key()?;

    // Encrypt private key
    let encrypted = crypto::encrypt_private_key(&private_key_pkcs8, master_key)?;

    // Set validity period
    let valid_from = now;
    let valid_until = now + Duration::days(KEY_VALIDITY_DAYS);

    // Create new key
    signing_keys::create_signing_key(
        pool,
        &key_id,
        &public_key_pem,
        &encrypted.encrypted_data,
        &encrypted.nonce,
        &encrypted.tag,
        1, // master_key_version
        valid_from,
        valid_until,
    )
    .await?;

    // Rotate keys (mark old as inactive, new as active)
    signing_keys::rotate_key(pool, &key_id).await?;

    // Log key rotation
    if let Err(e) = auth_events::log_event(
        pool,
        AuthEventType::KeyRotated.as_str(),
        None,
        None,
        true,
        None,
        None,
        None,
        Some(serde_json::json!({
            "new_key_id": key_id,
            "valid_from": valid_from.to_rfc3339(),
            "valid_until": valid_until.to_rfc3339(),
        })),
    )
    .await
    {
        tracing::warn!("Failed to log auth event: {}", e);
    }

    Ok(key_id)
}

/// Get JWKS (JSON Web Key Set) for public key distribution
///
/// Returns all active public keys in RFC 7517 format
pub async fn get_jwks(pool: &PgPool) -> Result<Jwks, AcError> {
    // Fetch all active keys
    let keys = signing_keys::get_all_active_keys(pool).await?;

    // Convert to JWKS format
    let json_web_keys: Vec<JsonWebKey> = keys
        .into_iter()
        .map(|key| {
            // Extract base64 from PEM format
            let public_key_b64 = key
                .public_key
                .lines()
                .filter(|line| !line.starts_with("-----"))
                .collect::<String>();

            JsonWebKey {
                kid: key.key_id,
                kty: "OKP".to_string(),     // Octet Key Pair for EdDSA
                crv: "Ed25519".to_string(), // Curve
                x: public_key_b64,          // Public key
                use_: "sig".to_string(),    // Signature use
                alg: "EdDSA".to_string(),   // Algorithm
            }
        })
        .collect();

    Ok(Jwks {
        keys: json_web_keys,
    })
}

/// Check and mark expired keys as inactive
#[allow(dead_code)] // Will be used in Phase 4 background tasks
pub async fn expire_old_keys(_pool: &PgPool) -> Result<Vec<String>, AcError> {
    // This would be called periodically by a background task
    // For now, it's a placeholder that could be implemented in Phase 4

    Ok(vec![])
}
