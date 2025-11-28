//! Test server harness for E2E testing
//!
//! Provides TestAuthServer for spawning real AC server instances in tests.

use std::net::SocketAddr;
use tokio::task::JoinHandle;

/// Test harness for spawning Auth Controller server in E2E tests
///
/// # Example
/// ```rust,ignore
/// #[tokio::test]
/// async fn test_auth_flow_e2e() {
///     let server = TestAuthServer::spawn().await;
///     let client = reqwest::Client::new();
///
///     let response = client
///         .post(&format!("{}/api/v1/auth/service/token", server.url()))
///         .json(&token_request)
///         .send()
///         .await?;
///
///     assert_eq!(response.status(), 200);
/// }
/// ```
pub struct TestAuthServer {
    addr: SocketAddr,
    _handle: JoinHandle<()>,
}

impl TestAuthServer {
    /// Spawn a new test server instance
    ///
    /// The server will bind to a random available port.
    pub async fn spawn() -> Result<Self, anyhow::Error> {
        // Implementation will be added in Phase 4.4
        // For now, this is a placeholder to establish the API
        unimplemented!("TestAuthServer::spawn will be implemented in Phase 4.4")
    }

    /// Get the base URL of the test server
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// Get the socket address
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }
}

impl Drop for TestAuthServer {
    fn drop(&mut self) {
        // Server task will be aborted when handle is dropped
    }
}

#[cfg(test)]
mod tests {
    // Tests will be added when implementing TestAuthServer
}
