//! Shared test scaffolding for AC integration tests (ADR-0032 Step 4).
//!
//! Provides parameterized JWT-signing helpers that fetch the active signing
//! key from the test DB, decrypt it with the supplied master key, and sign
//! the caller-supplied claims. Callers retain full control over `iat`,
//! `scope`, `service_type`, `sub`, and other claim-specific fields — the
//! helper only de-duplicates the fixed signing-decrypt-sign 4-line block
//! that recurred at 9 call sites across `tests/errors_metric_integration.rs`,
//! `tests/token_validation_integration.rs`, and
//! `tests/key_rotation_metrics_integration.rs` (per @dry-reviewer Finding 1
//! iter-3 closure with disposition (a)).
//!
//! Pre-condition: the active signing key must already exist in the pool
//! (e.g., via `test_state::seed_signing_key` or
//! `key_management_service::initialize_signing_key`). Call sites fail
//! loudly via `unwrap()` rather than returning rich errors — these are
//! tests, not production.

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use ac_service::crypto::{self, Claims, EncryptedKey};
use ac_service::repositories::signing_keys;
use common::jwt::UserClaims;
use common::secret::SecretBox;
use sqlx::PgPool;

/// Sign a service-token `Claims` with the active signing key. Caller supplies
/// the master key (matching the one passed to `initialize_signing_key`) and
/// the claims (any combination of `sub`, `iat`, `exp`, `scope`,
/// `service_type` is fine — helper is parameterized on claims, not opinionated
/// on their content).
pub async fn sign_service_token(pool: &PgPool, master_key: &[u8], claims: &Claims) -> String {
    let signing_key = signing_keys::get_active_key(pool).await.unwrap().unwrap();
    let encrypted_key = EncryptedKey {
        encrypted_data: SecretBox::new(Box::new(signing_key.private_key_encrypted)),
        nonce: signing_key.encryption_nonce,
        tag: signing_key.encryption_tag,
    };
    let private_key = crypto::decrypt_private_key(&encrypted_key, master_key).unwrap();
    crypto::sign_jwt(claims, &private_key, &signing_key.key_id).unwrap()
}

/// Sign a user-token `UserClaims` with the active signing key. Mirror of
/// `sign_service_token` for the user-JWT shape (different production sign fn).
pub async fn sign_user_token(pool: &PgPool, master_key: &[u8], claims: &UserClaims) -> String {
    let signing_key = signing_keys::get_active_key(pool).await.unwrap().unwrap();
    let encrypted_key = EncryptedKey {
        encrypted_data: SecretBox::new(Box::new(signing_key.private_key_encrypted)),
        nonce: signing_key.encryption_nonce,
        tag: signing_key.encryption_tag,
    };
    let private_key = crypto::decrypt_private_key(&encrypted_key, master_key).unwrap();
    crypto::sign_user_jwt(claims, &private_key, &signing_key.key_id).unwrap()
}
