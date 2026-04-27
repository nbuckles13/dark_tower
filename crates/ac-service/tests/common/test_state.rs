//! Shared test scaffolding for AC integration tests (ADR-0032 Step 4).
//!
//! Provides AppState/Config construction, signing-key seeding, and
//! credential-seeding helpers used across the per-cluster metric tests in
//! `crates/ac-service/tests/`. Mirrors MC Step 3's `tests/common/` pattern
//! and the in-`src/` `admin_handler::tests::test_config()` shape.
//!
//! # Bcrypt cost
//!
//! `seed_service_credential` uses `MIN_BCRYPT_COST` (10) for incidental
//! credential creation where bcrypt is scaffolding, not the test's invariant.
//! Tests that assert the production bcrypt cost factor itself (e.g.,
//! `tests/bcrypt_metrics_integration.rs` asserting `ac_bcrypt_duration_seconds`
//! buckets) call `crypto::hash_client_secret` directly with
//! `DEFAULT_BCRYPT_COST` (12) — they don't need an `AppState` because they
//! drive the crypto wrapper directly. The in-test direct-call form is the
//! @operations + @dry-reviewer reconciled approach: no parallel
//! `make_app_state_with_default_cost` factory needed.

// `tests/common/mod.rs` is `#[path]`-imported into every integration-test
// binary, but each binary uses only a subset of these helpers; per-binary
// dead-code warnings here are noise, not bugs.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use ac_service::config::{Config, MIN_BCRYPT_COST};
use ac_service::crypto;
use ac_service::handlers::auth_handler::AppState;
use ac_service::repositories::service_credentials;
use ac_service::services::key_management_service;
use ac_test_utils::crypto_fixtures::test_master_key;
use common::secret::SecretBox;
use sqlx::PgPool;
use std::sync::Arc;

/// Build an `Arc<AppState>` with the standard test config and the supplied
/// pool. The config uses `MIN_BCRYPT_COST` to keep credential-creation paths
/// fast in tests where bcrypt is incidental scaffolding.
pub fn make_app_state(pool: PgPool) -> Arc<AppState> {
    let master_key = test_master_key();
    let config = Config {
        database_url: String::new(),
        bind_address: "127.0.0.1:0".to_string(),
        master_key: SecretBox::new(Box::new(master_key.clone())),
        hash_secret: SecretBox::new(Box::new(master_key)),
        otlp_endpoint: None,
        jwt_clock_skew_seconds: ac_service::config::DEFAULT_JWT_CLOCK_SKEW.as_secs() as i64,
        bcrypt_cost: MIN_BCRYPT_COST,
        rate_limit_window_minutes: ac_service::config::DEFAULT_RATE_LIMIT_WINDOW_MINUTES,
        rate_limit_max_attempts: ac_service::config::DEFAULT_RATE_LIMIT_MAX_ATTEMPTS,
        registration_rate_limit_window_minutes:
            ac_service::config::DEFAULT_REGISTRATION_RATE_LIMIT_WINDOW_MINUTES,
        registration_rate_limit_max_attempts:
            ac_service::config::DEFAULT_REGISTRATION_RATE_LIMIT_MAX_ATTEMPTS,
    };
    Arc::new(AppState { pool, config })
}

/// Initialize a signing key in the test DB using the standard `test_master_key()`.
/// Idempotent — safe to call multiple times in the same `#[sqlx::test]`.
pub async fn seed_signing_key(pool: &PgPool) -> Result<(), anyhow::Error> {
    let master_key = test_master_key();
    key_management_service::initialize_signing_key(pool, &master_key, "test-cluster")
        .await
        .map_err(|e| anyhow::anyhow!("Failed to initialize signing key: {}", e))?;
    Ok(())
}

/// Seed a service credential in the test DB. Uses `MIN_BCRYPT_COST` because
/// every caller treats bcrypt as incidental scaffolding (not the test's
/// invariant). Bcrypt-bucket-fidelity tests drive `crypto::hash_client_secret`
/// directly with `DEFAULT_BCRYPT_COST`.
pub async fn seed_service_credential(
    pool: &PgPool,
    client_id: &str,
    scopes: &[&str],
) -> Result<(), anyhow::Error> {
    let client_secret_hash = crypto::hash_client_secret(TEST_CLIENT_SECRET, MIN_BCRYPT_COST)
        .map_err(|e| anyhow::anyhow!("Failed to hash test secret: {:?}", e))?;
    let scopes_vec: Vec<String> = scopes.iter().map(|s| (*s).to_string()).collect();
    service_credentials::create_service_credential(
        pool,
        client_id,
        &client_secret_hash,
        "global-controller",
        None,
        &scopes_vec,
    )
    .await
    .map_err(|e| anyhow::anyhow!("Failed to seed service credential: {:?}", e))?;
    Ok(())
}

/// The deterministic test client secret used by `seed_service_credential`.
/// Exposed so tests can drive `handle_service_token` with the matching secret.
pub const TEST_CLIENT_SECRET: &str = "test-secret-12345";
