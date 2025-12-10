//! Test server harness for E2E testing
//!
//! Provides TestAuthServer for spawning real AC server instances in tests.

use crate::crypto_fixtures::test_master_key;
use ac_service::config::Config;
use ac_service::crypto;
use ac_service::handlers::auth_handler::AppState;
use ac_service::repositories::{service_credentials, signing_keys};
use ac_service::services::{key_management_service, token_service};
use axum::Router;
use chrono::Utc;
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
            master_key,
            otlp_endpoint: None,
        };

        // Create application state
        let state = Arc::new(AppState {
            pool: pool.clone(),
            config: config.clone(),
        });

        // Build routes using ac-service's route builder
        let app = build_test_routes(state);

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
        let client_secret_hash = crypto::hash_client_secret(client_secret)?;

        // Convert scopes to Vec<String>
        let scopes_vec: Vec<String> = scopes.iter().map(|s| s.to_string()).collect();

        // Register service credential
        service_credentials::create_service_credential(
            &self.pool,
            client_id,
            &client_secret_hash,
            "test-service", // service_type
            None,           // region
            &scopes_vec,
        )
        .await?;

        // Issue token
        let token_response = token_service::issue_service_token(
            &self.pool,
            &self.config.master_key,
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
            encrypted_data: signing_key.private_key_encrypted.clone(),
            nonce: signing_key.encryption_nonce.clone(),
            tag: signing_key.encryption_tag.clone(),
        };
        let private_key_bytes =
            crypto::decrypt_private_key(&encrypted_key, &self.config.master_key)?;

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
            encrypted_data: signing_key.private_key_encrypted.clone(),
            nonce: signing_key.encryption_nonce.clone(),
            tag: signing_key.encryption_tag.clone(),
        };
        let private_key_bytes =
            crypto::decrypt_private_key(&encrypted_key, &self.config.master_key)?;

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

/// Build routes for test server
///
/// NOTE: This duplicates middleware from ac-service because the middleware
/// module is not publicly exported. This is intentional for Phase 4 to
/// maintain test isolation and allow tests to verify actual HTTP behavior
/// end-to-end. Consider refactoring in Phase 5 when middleware API stabilizes
/// by exporting `pub mod middleware;` from ac-service.
fn build_test_routes(state: Arc<AppState>) -> Router {
    use ac_service::handlers::{admin_handler, auth_handler, jwks_handler};
    use axum::{middleware, routing::get, routing::post, Router};
    use std::time::Duration;
    use tower_http::{timeout::TimeoutLayer, trace::TraceLayer};

    // Create auth middleware state
    let auth_state = Arc::new(AuthMiddlewareState {
        pool: state.pool.clone(),
    });

    // Admin routes that require authentication with admin:services scope
    let admin_routes = Router::new()
        .route(
            "/api/v1/admin/services/register",
            post(admin_handler::handle_register_service),
        )
        .layer(middleware::from_fn_with_state(
            auth_state.clone(),
            require_admin_scope,
        ))
        .with_state(state.clone());

    // Internal routes (key rotation) - authentication handled in handler
    let internal_routes = Router::new()
        .route(
            "/internal/rotate-keys",
            post(admin_handler::handle_rotate_keys),
        )
        .with_state(state.clone());

    // Public routes (no authentication required)
    let public_routes = Router::new()
        .route(
            "/api/v1/auth/user/token",
            post(auth_handler::handle_user_token),
        )
        .route(
            "/api/v1/auth/service/token",
            post(auth_handler::handle_service_token),
        )
        .route("/.well-known/jwks.json", get(jwks_handler::handle_get_jwks))
        // Health check (liveness probe)
        .route("/health", get(health_check))
        // Readiness probe (ADR-0012)
        .route("/ready", get(readiness_check))
        .with_state(state);

    // Merge routes with global layers
    admin_routes
        .merge(internal_routes)
        .merge(public_routes)
        .layer(TraceLayer::new_for_http())
        // ADR-0012: 30s HTTP request timeout
        .layer(TimeoutLayer::new(Duration::from_secs(30)))
}

/// Readiness response structure
#[derive(serde::Serialize)]
struct ReadinessResponse {
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    database: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    signing_key: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Readiness probe - verifies service dependencies are available
/// Security: Error messages are intentionally generic to avoid leaking infrastructure details.
async fn readiness_check(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> impl axum::response::IntoResponse {
    use ac_service::repositories::signing_keys;
    use axum::http::StatusCode;

    // Check 1: Database connectivity
    let db_check = sqlx::query("SELECT 1").fetch_one(&state.pool).await;

    if let Err(_e) = db_check {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            axum::Json(ReadinessResponse {
                status: "not_ready",
                database: Some("unhealthy"),
                signing_key: None,
                // Generic error - don't leak infrastructure details
                error: Some("Service dependencies unavailable".to_string()),
            }),
        );
    }

    // Check 2: Active signing key availability
    let key_check = signing_keys::get_active_key(&state.pool).await;

    match key_check {
        Ok(Some(_)) => (
            StatusCode::OK,
            axum::Json(ReadinessResponse {
                status: "ready",
                database: Some("healthy"),
                signing_key: Some("available"),
                error: None,
            }),
        ),
        Ok(None) => {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                axum::Json(ReadinessResponse {
                    status: "not_ready",
                    database: Some("healthy"),
                    signing_key: Some("unavailable"),
                    // Generic error - don't leak key rotation state
                    error: Some("Service dependencies unavailable".to_string()),
                }),
            )
        }
        Err(_e) => {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                axum::Json(ReadinessResponse {
                    status: "not_ready",
                    database: Some("healthy"),
                    signing_key: Some("error"),
                    // Generic error - don't leak infrastructure details
                    error: Some("Service dependencies unavailable".to_string()),
                }),
            )
        }
    }
}

/// Middleware state for authentication
///
/// Duplicated from ac-service since middleware module is not public.
#[derive(Clone)]
struct AuthMiddlewareState {
    pool: PgPool,
}

/// Authentication middleware requiring admin:services scope
///
/// Duplicated from ac-service since middleware module is not public.
async fn require_admin_scope(
    axum::extract::State(state): axum::extract::State<Arc<AuthMiddlewareState>>,
    mut req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Result<impl axum::response::IntoResponse, ac_service::errors::AcError> {
    use ac_service::crypto;
    use ac_service::errors::AcError;
    use ac_service::repositories::signing_keys;

    // Extract Authorization header
    let auth_header = req
        .headers()
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .ok_or(AcError::InvalidToken(
            "Missing Authorization header".to_string(),
        ))?;

    // Extract Bearer token
    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or(AcError::InvalidToken(
            "Invalid Authorization header format".to_string(),
        ))?;

    // Get active signing key for verification
    let signing_key = signing_keys::get_active_key(&state.pool)
        .await?
        .ok_or_else(|| AcError::Crypto("No active signing key available".to_string()))?;

    // Verify JWT
    let claims = crypto::verify_jwt(token, &signing_key.public_key)?;

    // Check if token has required scope (admin:services)
    let required_scope = "admin:services";
    let token_scopes: Vec<&str> = claims.scope.split_whitespace().collect();

    if !token_scopes.contains(&required_scope) {
        return Err(AcError::InsufficientScope {
            required: required_scope.to_string(),
            provided: token_scopes.iter().map(|s| s.to_string()).collect(),
        });
    }

    // Store claims in request extensions for downstream handlers
    req.extensions_mut().insert(claims);

    // Continue to next handler
    Ok(next.run(req).await)
}

async fn health_check() -> &'static str {
    "OK"
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
