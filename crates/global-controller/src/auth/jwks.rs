//! JWKS client for fetching and caching public keys from Auth Controller.
//!
//! The JWKS (JSON Web Key Set) client fetches public keys from the Auth Controller's
//! `/.well-known/jwks.json` endpoint and caches them with a configurable TTL.
//!
//! # Security
//!
//! - Keys are cached to reduce load on AC and improve latency
//! - Cache is invalidated on TTL expiry to pick up key rotations
//! - HTTPS should be used in production (enforced by deployment config)

use crate::errors::GcError;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::instrument;

/// Default cache TTL in seconds (5 minutes).
const DEFAULT_CACHE_TTL_SECONDS: u64 = 300;

/// JSON Web Key from JWKS endpoint.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)] // Fields used for deserialization and future validation
pub struct Jwk {
    /// Key type (always "OKP" for Ed25519).
    pub kty: String,

    /// Key ID - used to select the correct key for verification.
    pub kid: String,

    /// Curve name (always "Ed25519" for EdDSA).
    #[serde(default)]
    pub crv: Option<String>,

    /// Public key value (base64url encoded).
    #[serde(default)]
    pub x: Option<String>,

    /// Algorithm (should be "EdDSA").
    #[serde(default)]
    pub alg: Option<String>,

    /// Key use (should be "sig" for signing).
    #[serde(default, rename = "use")]
    pub key_use: Option<String>,
}

/// JWKS response from Auth Controller.
#[derive(Debug, Clone, Deserialize)]
pub struct JwksResponse {
    /// List of JSON Web Keys.
    pub keys: Vec<Jwk>,
}

/// Cached JWKS data with expiry time.
struct CachedJwks {
    /// Map of key ID to JWK.
    keys: HashMap<String, Jwk>,

    /// When this cache entry expires.
    expires_at: Instant,
}

/// JWKS client for fetching and caching public keys.
///
/// Thread-safe client that fetches JWKS from Auth Controller and caches
/// the keys with configurable TTL.
pub struct JwksClient {
    /// URL to the JWKS endpoint.
    jwks_url: String,

    /// HTTP client for fetching JWKS.
    http_client: reqwest::Client,

    /// Cached JWKS data.
    cache: Arc<RwLock<Option<CachedJwks>>>,

    /// Cache TTL duration.
    cache_ttl: Duration,
}

impl JwksClient {
    /// Create a new JWKS client.
    ///
    /// # Arguments
    ///
    /// * `jwks_url` - URL to the Auth Controller's JWKS endpoint
    pub fn new(jwks_url: String) -> Self {
        Self::with_ttl(jwks_url, Duration::from_secs(DEFAULT_CACHE_TTL_SECONDS))
    }

    /// Create a new JWKS client with custom cache TTL.
    ///
    /// # Arguments
    ///
    /// * `jwks_url` - URL to the Auth Controller's JWKS endpoint
    /// * `cache_ttl` - How long to cache JWKS before refreshing
    pub fn with_ttl(jwks_url: String, cache_ttl: Duration) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_else(|e| {
                tracing::warn!(target: "gc.auth.jwks", error = %e, "Failed to build HTTP client with custom config, using defaults");
                reqwest::Client::new()
            });

        Self {
            jwks_url,
            http_client,
            cache: Arc::new(RwLock::new(None)),
            cache_ttl,
        }
    }

    /// Get a JWK by key ID.
    ///
    /// Returns the JWK if found, or fetches from AC if cache is expired/empty.
    ///
    /// # Arguments
    ///
    /// * `kid` - Key ID to look up
    ///
    /// # Errors
    ///
    /// Returns `GcError::ServiceUnavailable` if JWKS cannot be fetched.
    /// Returns `GcError::InvalidToken` if key ID is not found.
    #[instrument(skip(self), fields(kid = %kid))]
    pub async fn get_key(&self, kid: &str) -> Result<Jwk, GcError> {
        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.as_ref() {
                if cached.expires_at > Instant::now() {
                    if let Some(key) = cached.keys.get(kid) {
                        tracing::debug!(target: "gc.auth.jwks", kid = %kid, "JWKS cache hit");
                        return Ok(key.clone());
                    }
                    // Key not found in valid cache
                    tracing::debug!(target: "gc.auth.jwks", kid = %kid, "Key not found in JWKS cache");
                    return Err(GcError::InvalidToken(
                        "The access token is invalid or expired".to_string(),
                    ));
                }
            }
        }

        // Cache miss or expired - fetch fresh JWKS
        self.refresh_cache().await?;

        // Try to get key from refreshed cache
        let cache = self.cache.read().await;
        if let Some(cached) = cache.as_ref() {
            if let Some(key) = cached.keys.get(kid) {
                return Ok(key.clone());
            }
        }

        // Key not found even after refresh
        tracing::warn!(target: "gc.auth.jwks", kid = %kid, "Key not found in JWKS after refresh");
        Err(GcError::InvalidToken(
            "The access token is invalid or expired".to_string(),
        ))
    }

    /// Refresh the JWKS cache by fetching from Auth Controller.
    #[instrument(skip(self))]
    async fn refresh_cache(&self) -> Result<(), GcError> {
        tracing::debug!(target: "gc.auth.jwks", url = %self.jwks_url, "Fetching JWKS from AC");

        let response = self
            .http_client
            .get(&self.jwks_url)
            .send()
            .await
            .map_err(|e| {
                tracing::error!(target: "gc.auth.jwks", error = %e, "Failed to fetch JWKS");
                GcError::ServiceUnavailable("Authentication service unavailable".to_string())
            })?;

        if !response.status().is_success() {
            tracing::error!(
                target: "gc.auth.jwks",
                status = %response.status(),
                "JWKS endpoint returned error"
            );
            return Err(GcError::ServiceUnavailable(
                "Authentication service unavailable".to_string(),
            ));
        }

        let jwks: JwksResponse = response.json().await.map_err(|e| {
            tracing::error!(target: "gc.auth.jwks", error = %e, "Failed to parse JWKS response");
            GcError::ServiceUnavailable("Authentication service unavailable".to_string())
        })?;

        // Build key map
        let keys: HashMap<String, Jwk> = jwks
            .keys
            .into_iter()
            .map(|key| (key.kid.clone(), key))
            .collect();

        tracing::info!(
            target: "gc.auth.jwks",
            key_count = keys.len(),
            "JWKS cache refreshed"
        );

        // Update cache
        let mut cache = self.cache.write().await;
        *cache = Some(CachedJwks {
            keys,
            expires_at: Instant::now() + self.cache_ttl,
        });

        Ok(())
    }

    /// Force refresh the cache.
    ///
    /// Useful for testing or manual cache invalidation.
    #[allow(dead_code)] // API for manual cache invalidation
    pub async fn force_refresh(&self) -> Result<(), GcError> {
        self.refresh_cache().await
    }

    /// Clear the cache.
    ///
    /// Useful for testing.
    #[cfg(test)]
    #[allow(dead_code)] // Test utility method
    pub async fn clear_cache(&self) {
        let mut cache = self.cache.write().await;
        *cache = None;
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_jwk_deserialization() {
        let json = r#"{
            "kty": "OKP",
            "kid": "test-key-01",
            "crv": "Ed25519",
            "x": "dGVzdC1wdWJsaWMta2V5LWRhdGE",
            "alg": "EdDSA",
            "use": "sig"
        }"#;

        let jwk: Jwk = serde_json::from_str(json).unwrap();

        assert_eq!(jwk.kty, "OKP");
        assert_eq!(jwk.kid, "test-key-01");
        assert_eq!(jwk.crv, Some("Ed25519".to_string()));
        assert_eq!(jwk.x, Some("dGVzdC1wdWJsaWMta2V5LWRhdGE".to_string()));
        assert_eq!(jwk.alg, Some("EdDSA".to_string()));
        assert_eq!(jwk.key_use, Some("sig".to_string()));
    }

    #[test]
    fn test_jwk_deserialization_minimal() {
        // Only required fields
        let json = r#"{
            "kty": "OKP",
            "kid": "test-key-02"
        }"#;

        let jwk: Jwk = serde_json::from_str(json).unwrap();

        assert_eq!(jwk.kty, "OKP");
        assert_eq!(jwk.kid, "test-key-02");
        assert!(jwk.crv.is_none());
        assert!(jwk.x.is_none());
        assert!(jwk.alg.is_none());
        assert!(jwk.key_use.is_none());
    }

    #[test]
    fn test_jwks_response_deserialization() {
        let json = r#"{
            "keys": [
                {"kty": "OKP", "kid": "key-1"},
                {"kty": "OKP", "kid": "key-2"}
            ]
        }"#;

        let jwks: JwksResponse = serde_json::from_str(json).unwrap();

        assert_eq!(jwks.keys.len(), 2);
        assert_eq!(jwks.keys.first().unwrap().kid, "key-1");
        assert_eq!(jwks.keys.get(1).unwrap().kid, "key-2");
    }

    #[test]
    fn test_jwks_client_creation() {
        let client = JwksClient::new("http://localhost:8082/.well-known/jwks.json".to_string());
        assert_eq!(
            client.jwks_url,
            "http://localhost:8082/.well-known/jwks.json"
        );
    }

    #[test]
    fn test_jwks_client_custom_ttl() {
        let client = JwksClient::with_ttl(
            "http://localhost:8082/.well-known/jwks.json".to_string(),
            Duration::from_secs(60),
        );
        assert_eq!(client.cache_ttl, Duration::from_secs(60));
    }
}
