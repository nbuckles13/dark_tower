//! JWKS mock rig: wiremock + `TestKeypair` pair.

use std::sync::Arc;

use common::jwt::JwksClient;
use wiremock::MockServer;

use super::{mount_jwks_mock, TestKeypair};

/// JWKS fixture: mock server, signing keypair, and JWKS URL.
///
/// Each test gets its own `MockServer` (ephemeral port → unique URL) so the
/// `JwksClient` URL-keyed cache does not leak keys across tests in the same
/// process.
pub struct JwksRig {
    pub mock_server: MockServer,
    pub keypair: TestKeypair,
    pub jwks_url: String,
}

impl JwksRig {
    pub async fn start(seed: u8, kid: &str) -> Self {
        let mock_server = MockServer::start().await;
        let keypair = TestKeypair::new(seed, kid);
        let jwks_url = mount_jwks_mock(&mock_server, &keypair).await;
        Self {
            mock_server,
            keypair,
            jwks_url,
        }
    }

    pub fn jwks_client(&self) -> Arc<JwksClient> {
        Arc::new(
            JwksClient::new(self.jwks_url.clone())
                .expect("failed to build JwksClient from mock URL"),
        )
    }
}
