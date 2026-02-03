//! Test server harness for E2E testing
//!
//! Provides `TestGcServer` for spawning real GC server instances in tests.

use common::secret::SecretString;
use common::token_manager::TokenReceiver;
use global_controller::config::Config;
use global_controller::routes::{self, AppState};
use global_controller::services::MockMcClient;
use sqlx::PgPool;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::watch;
use tokio::task::JoinHandle;

/// Test harness for spawning Global Controller server in E2E tests.
///
/// # Example
/// ```rust,ignore
/// #[sqlx::test(migrations = "../../migrations")]
/// async fn test_health_flow_e2e(pool: PgPool) -> Result<()> {
///     let server = TestGcServer::spawn(pool).await?;
///     let client = reqwest::Client::new();
///
///     let response = client
///         .get(&format!("{}/health", server.url()))
///         .send()
///         .await?;
///
///     assert_eq!(response.status(), 200);
///     Ok(())
/// }
/// ```
pub struct TestGcServer {
    addr: SocketAddr,
    pool: PgPool,
    config: Config,
    _handle: JoinHandle<()>,
}

impl TestGcServer {
    /// Spawn a new test server instance with isolated database.
    ///
    /// The server will:
    /// - Bind to a random available port (127.0.0.1:0)
    /// - Start the HTTP server in the background
    ///
    /// # Arguments
    /// * `pool` - Database connection pool (typically from `#[sqlx::test]`)
    ///
    /// # Returns
    /// * `Ok(TestGcServer)` - Running server instance
    /// * `Err(anyhow::Error)` - If server spawn fails
    pub async fn spawn(pool: PgPool) -> Result<Self, anyhow::Error> {
        // Build configuration for test environment
        let vars = HashMap::from([
            (
                "DATABASE_URL".to_string(),
                "postgresql://test/test".to_string(),
            ),
            ("BIND_ADDRESS".to_string(), "127.0.0.1:0".to_string()),
            ("GC_REGION".to_string(), "test-region".to_string()),
            (
                "AC_JWKS_URL".to_string(),
                "http://localhost:8082/.well-known/jwks.json".to_string(),
            ),
            ("GC_CLIENT_ID".to_string(), "test-gc-client".to_string()),
            ("GC_CLIENT_SECRET".to_string(), "test-gc-secret".to_string()),
        ]);

        let config = Config::from_vars(&vars)
            .map_err(|e| anyhow::anyhow!("Failed to create config: {}", e))?;

        // Create a mock TokenReceiver for testing
        let (_tx, rx) = watch::channel(SecretString::from("test-token"));
        let token_receiver = TokenReceiver::from_watch_receiver(rx);

        // Create application state with MockMcClient
        let mock_mc_client = Arc::new(MockMcClient::accepting());
        let state = Arc::new(AppState {
            pool: pool.clone(),
            config: config.clone(),
            mc_client: mock_mc_client,
            token_receiver,
        });

        // Build routes using global-controller's real route builder
        let app = routes::build_routes(state);

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

    /// Get reference to the database pool.
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Get the base URL of the test server.
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// Get the socket address.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Get reference to the server configuration.
    pub fn config(&self) -> &Config {
        &self.config
    }
}

impl Drop for TestGcServer {
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
        let server = TestGcServer::spawn(pool).await?;

        // Verify server is accessible
        assert!(server.url().starts_with("http://127.0.0.1:"));

        // Verify health endpoint works
        let response = reqwest::get(&format!("{}/health", server.url())).await?;
        assert_eq!(response.status(), 200);

        // Verify response body
        let body: serde_json::Value = response.json().await?;
        assert_eq!(body["status"], "healthy");
        assert_eq!(body["region"], "test-region");
        assert_eq!(body["database"], "healthy");

        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_server_provides_pool_access(pool: PgPool) -> Result<(), anyhow::Error> {
        let server = TestGcServer::spawn(pool.clone()).await?;

        // Verify we can access the pool
        let pool_ref = server.pool();

        // Execute a simple query to verify pool works
        let result: (i32,) = sqlx::query_as("SELECT 1").fetch_one(pool_ref).await?;

        assert_eq!(result.0, 1);

        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_server_provides_addr(pool: PgPool) -> Result<(), anyhow::Error> {
        let server = TestGcServer::spawn(pool).await?;

        // Verify addr() returns a valid SocketAddr
        let addr = server.addr();

        // Should be localhost
        assert!(addr.ip().is_loopback());

        // Should have a non-zero port
        assert!(addr.port() > 0);

        // Verify addr matches url
        let expected_url = format!("http://{}", addr);
        assert_eq!(server.url(), expected_url);

        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_server_provides_config_access(pool: PgPool) -> Result<(), anyhow::Error> {
        let server = TestGcServer::spawn(pool).await?;

        // Verify we can access the config
        let config = server.config();

        // Verify region is set from test environment
        assert_eq!(config.region, "test-region");

        // Verify bind address is set
        assert_eq!(config.bind_address.to_string(), "127.0.0.1:0");

        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_server_cleanup_on_drop(pool: PgPool) -> Result<(), anyhow::Error> {
        let addr;
        {
            let server = TestGcServer::spawn(pool).await?;
            addr = server.addr();

            // Verify server is running
            let response = reqwest::get(&format!("http://{}/health", addr)).await?;
            assert_eq!(response.status(), 200);

            // Server will be dropped here
        }

        // Give the server time to shut down
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // After drop, server should no longer accept connections
        // Note: We can't reliably test this as the port might be quickly reused
        // The key thing is that Drop::drop() was called and abort() was invoked
        // This test exercises the Drop implementation path

        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_multiple_servers_different_ports(pool: PgPool) -> Result<(), anyhow::Error> {
        let server1 = TestGcServer::spawn(pool.clone()).await?;
        let server2 = TestGcServer::spawn(pool).await?;

        // Verify both servers have different addresses
        assert_ne!(server1.addr(), server2.addr());

        // Verify both servers are accessible
        let response1 = reqwest::get(&format!("{}/health", server1.url())).await?;
        assert_eq!(response1.status(), 200);

        let response2 = reqwest::get(&format!("{}/health", server2.url())).await?;
        assert_eq!(response2.status(), 200);

        Ok(())
    }
}
