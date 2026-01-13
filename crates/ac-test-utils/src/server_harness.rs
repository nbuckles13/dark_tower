//! Test server harness for E2E testing
//!
//! Provides TestAuthServer for spawning real AC server instances in tests.

use crate::crypto_fixtures::test_master_key;
use ac_service::config::{Config, DEFAULT_BCRYPT_COST};
use ac_service::crypto;
use ac_service::handlers::auth_handler::AppState;
use ac_service::repositories::{service_credentials, signing_keys};
use ac_service::routes;
use ac_service::services::{key_management_service, token_service};
use chrono::Utc;
use common::secret::{ExposeSecret, SecretBox};
use sqlx::PgPool;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::task::JoinHandle;

/// Test harness for spawning Auth Controller server in E2E tests
///
/// # Example
/// ```rust,ignore
/// #[sqlx::test(migrations = "../../migrations")]
/// async fn test_auth_flow_e2e(pool: PgPool) -> Result<()> {
///     let server = TestAuthServer::spawn(pool).await?;
///     let client = reqwest::Client::new();
///
///     let response = client
///         .post(&format!("{}/api/v1/auth/service/token", server.url()))
///         .json(&token_request)
///         .send()
///         .await?;
///
///     assert_eq!(response.status(), 200);
///     Ok(())
/// }
/// ```
pub struct TestAuthServer {
    addr: SocketAddr,
    pool: PgPool,
    config: Config,
    _handle: JoinHandle<()>,
}

impl TestAuthServer {
    /// Spawn a new test server instance with isolated database
    ///
    /// The server will:
    /// - Bind to a random available port (127.0.0.1:0)
    /// - Initialize signing keys using the test master key
    /// - Start the HTTP server in the background
    ///
    /// # Arguments
    /// * `pool` - Database connection pool (typically from `#[sqlx::test]`)
    ///
    /// # Returns
    /// * `Ok(TestAuthServer)` - Running server instance
    /// * `Err(anyhow::Error)` - If server spawn fails
    pub async fn spawn(pool: PgPool) -> Result<Self, anyhow::Error> {
        // Use test master key from crypto_fixtures
        let master_key = test_master_key();

        // Initialize signing key if none exists
        key_management_service::initialize_signing_key(&pool, &master_key, "test-cluster")
            .await
            .map_err(|e| anyhow::anyhow!("Failed to initialize signing key: {}", e))?;

        // Build configuration
        let config = Config {
            database_url: String::new(), // Not used after connection established
            bind_address: "127.0.0.1:0".to_string(),
            master_key: SecretBox::new(Box::new(master_key.clone())),
            hash_secret: SecretBox::new(Box::new(master_key.clone())), // Use same as master_key for tests
            otlp_endpoint: None,
            jwt_clock_skew_seconds: ac_service::config::DEFAULT_JWT_CLOCK_SKEW_SECONDS,
            bcrypt_cost: DEFAULT_BCRYPT_COST,
        };

        // Create application state
        let state = Arc::new(AppState {
            pool: pool.clone(),
            config: config.clone(),
        });

        // Initialize metrics recorder for test server
        // Note: This may fail if already installed in the test process.
        // In that case, we create a new recorder without installing it globally.
        let metrics_handle = match routes::init_metrics_recorder() {
            Ok(handle) => handle,
            Err(_) => {
                // If metrics recorder already installed globally, create a standalone recorder
                // without installing it. This allows each test to have its own metrics.
                use metrics_exporter_prometheus::PrometheusBuilder;
                let recorder = PrometheusBuilder::new().build_recorder();
                recorder.handle()
            }
        };

        // Build routes using ac-service's real route builder
        let app = routes::build_routes(state, metrics_handle);

        // Bind to random port
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|e| anyhow::anyhow!("Failed to bind test server: {}", e))?;

        let addr = listener
            .local_addr()
            .map_err(|e| anyhow::anyhow!("Failed to get local address: {}", e))?;

        // Spawn server in background
        let handle = tokio::spawn(async move {
            // Use into_make_service_with_connect_info to support SocketAddr extraction
            let make_service = app.into_make_service_with_connect_info::<SocketAddr>();
            if let Err(e) = axum::serve(listener, make_service).await {
                eprintln!("Test server error: {}", e);
            }
        });

        Ok(Self {
            addr,
            pool,
            config,
            _handle: handle,
        })
    }

    /// Get reference to the database pool
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Get the base URL of the test server
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// Get the socket address
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Get reference to the server configuration
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Create a service token with specified scopes
    ///
    /// Registers a test service credential and issues a token.
    /// Uses a deterministic client secret for reproducibility.
    ///
    /// # Arguments
    /// * `client_id` - Unique identifier for the test service
    /// * `scopes` - List of scopes to include in token
    ///
    /// # Example
    /// ```rust,ignore
    /// let token = server.create_service_token("test-client", &["service.rotate-keys.ac"]).await?;
    /// ```
    pub async fn create_service_token(
        &self,
        client_id: &str,
        scopes: &[&str],
    ) -> Result<String, anyhow::Error> {
        // Use deterministic test secret for reproducibility
        let client_secret = "test-secret-12345";
        let client_secret_hash = crypto::hash_client_secret(client_secret, DEFAULT_BCRYPT_COST)?;

        // Convert scopes to Vec<String>
        let scopes_vec: Vec<String> = scopes.iter().map(|s| s.to_string()).collect();

        // Register service credential
        // Use 'global-controller' as it's a valid service_type per DB constraint
        service_credentials::create_service_credential(
            &self.pool,
            client_id,
            &client_secret_hash,
            "global-controller", // service_type (must be valid per DB constraint)
            None,                // region
            &scopes_vec,
        )
        .await?;

        // Issue token
        let token_response = token_service::issue_service_token(
            &self.pool,
            self.config.master_key.expose_secret(),
            self.config.hash_secret.expose_secret(),
            client_id,
            client_secret,
            "client_credentials", // grant_type
            None,                 // requested_scopes (use credential's scopes)
            None,                 // ip_address
            None,                 // user_agent
        )
        .await?;

        Ok(token_response.access_token)
    }

    /// Create a user token with specified scopes
    ///
    /// Creates a JWT with user claims (service_type: None).
    /// Useful for testing endpoints that reject user tokens.
    ///
    /// # Arguments
    /// * `user_id` - Subject identifier for the user
    /// * `scopes` - List of scopes to include in token
    ///
    /// # Example
    /// ```rust,ignore
    /// let token = server.create_user_token("user-123", &["user.read"]).await?;
    /// ```
    pub async fn create_user_token(
        &self,
        user_id: &str,
        scopes: &[&str],
    ) -> Result<String, anyhow::Error> {
        // Get active signing key
        let signing_key = signing_keys::get_active_key(&self.pool)
            .await?
            .ok_or_else(|| anyhow::anyhow!("No active signing key available"))?;

        // Decrypt private key
        let encrypted_key = crypto::EncryptedKey {
            encrypted_data: SecretBox::new(Box::new(signing_key.private_key_encrypted.clone())),
            nonce: signing_key.encryption_nonce.clone(),
            tag: signing_key.encryption_tag.clone(),
        };
        let private_key_bytes =
            crypto::decrypt_private_key(&encrypted_key, self.config.master_key.expose_secret())?;

        // Create user claims (service_type: None)
        let now = Utc::now().timestamp();
        let claims = crypto::Claims {
            sub: user_id.to_string(),
            exp: now + 3600, // 1 hour
            iat: now,
            scope: scopes.join(" "),
            service_type: None, // User token, not service token
        };

        // Sign and return JWT
        let token = crypto::sign_jwt(&claims, &private_key_bytes, &signing_key.key_id)?;
        Ok(token)
    }

    /// Create an expired service token
    ///
    /// Creates a JWT that expired a specified number of seconds ago.
    /// Useful for testing token expiration validation.
    ///
    /// # Arguments
    /// * `client_id` - Subject identifier
    /// * `scopes` - List of scopes to include
    /// * `expired_seconds_ago` - How many seconds ago the token expired
    ///
    /// # Example
    /// ```rust,ignore
    /// let token = server.create_expired_token("test-client", &["admin"], 3600).await?;
    /// ```
    pub async fn create_expired_token(
        &self,
        client_id: &str,
        scopes: &[&str],
        expired_seconds_ago: i64,
    ) -> Result<String, anyhow::Error> {
        // Get active signing key
        let signing_key = signing_keys::get_active_key(&self.pool)
            .await?
            .ok_or_else(|| anyhow::anyhow!("No active signing key available"))?;

        // Decrypt private key
        let encrypted_key = crypto::EncryptedKey {
            encrypted_data: SecretBox::new(Box::new(signing_key.private_key_encrypted.clone())),
            nonce: signing_key.encryption_nonce.clone(),
            tag: signing_key.encryption_tag.clone(),
        };
        let private_key_bytes =
            crypto::decrypt_private_key(&encrypted_key, self.config.master_key.expose_secret())?;

        // Create expired claims
        let now = Utc::now().timestamp();
        let exp_time = now - expired_seconds_ago;
        let iat_time = exp_time - 3600; // Issued 1 hour before expiration

        let claims = crypto::Claims {
            sub: client_id.to_string(),
            exp: exp_time,
            iat: iat_time,
            scope: scopes.join(" "),
            service_type: Some("service".to_string()),
        };

        // Sign and return JWT
        let token = crypto::sign_jwt(&claims, &private_key_bytes, &signing_key.key_id)?;
        Ok(token)
    }
}

impl Drop for TestAuthServer {
    fn drop(&mut self) {
        // Explicitly abort the HTTP server task to ensure immediate cleanup
        // when the test completes. This stops the server gracefully.
        self._handle.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_server_spawns_successfully(pool: PgPool) -> Result<(), anyhow::Error> {
        let server = TestAuthServer::spawn(pool).await?;

        // Verify server is accessible
        assert!(server.url().starts_with("http://127.0.0.1:"));

        // Verify health endpoint works
        let response = reqwest::get(&format!("{}/health", server.url())).await?;
        assert_eq!(response.status(), 200);
        assert_eq!(response.text().await?, "OK");

        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_server_provides_pool_access(pool: PgPool) -> Result<(), anyhow::Error> {
        let server = TestAuthServer::spawn(pool.clone()).await?;

        // Verify we can access the pool
        let pool_ref = server.pool();

        // Execute a simple query to verify pool works
        let result: (i32,) = sqlx::query_as("SELECT 1").fetch_one(pool_ref).await?;

        assert_eq!(result.0, 1);

        Ok(())
    }
}
