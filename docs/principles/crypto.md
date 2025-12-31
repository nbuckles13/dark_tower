# Principle: Cryptography

**Status**: Active
**Guards**: `scripts/guards/simple/no-hardcoded-secrets.sh`

## DO

### Digital Signatures
- **USE EdDSA (Ed25519)** for all JWT signing operations
- **INCLUDE `kid` header** in all JWTs to identify signing key
- **ROTATE keys weekly** with 1-week validation overlap
- **STORE signing keys encrypted** with AES-256-GCM at rest

### Password Hashing
- **USE bcrypt** with cost factor ≥12 for all password hashing
- **RUN bcrypt even for non-existent accounts** to prevent timing attacks (use dummy hash)
- **VERIFY cost factor is 12** in tests (per ADR-0003)

### Random Number Generation
- **USE `ring::rand::SystemRandom`** for all security-critical randomness
- **GENERATE 256-bit secrets** (32 bytes) for client credentials
- **USE CSPRNG** for nonces, IVs, session tokens, API keys

### Encryption at Rest
- **USE AES-256-GCM** for encrypting private keys in database
- **GENERATE random 96-bit nonces** for each encryption operation
- **STORE nonce and authentication tag** alongside ciphertext
- **LOAD master key from environment** variable (`AC_MASTER_KEY`)

### Secret Management
- **LOAD secrets from environment variables** at runtime (`std::env::var`, `dotenvy`)
- **WRAP runtime secrets** in `SecretString` type to prevent logging
- **STORE credentials in secret management** systems (Vault, AWS Secrets Manager)
- **NEVER commit secrets** to version control (git)

### Timing Safety
- **USE constant-time comparison** for secret validation
- **RUN bcrypt verification** even when credentials not found (prevents user enumeration)
- **AVOID early returns** that leak information via timing

### Key Rotation
- **ROTATE weekly** with external scheduler (K8s CronJob, AWS EventBridge)
- **MAINTAIN 1-week overlap** for old key validation
- **AUTHENTICATE rotation requests** via OAuth 2.0 with `service.rotate-keys.ac` scope
- **RATE LIMIT rotation** to 1 per 6 days (normal) or 1 per hour (emergency)

## DON'T

### Hardcoded Secrets (NEVER)
- **DON'T hardcode passwords** in source code (`password = "secret123"`)
- **DON'T hardcode API keys** (`api_key = "sk-live-abc123..."`)
- **DON'T hardcode connection strings** with embedded credentials
- **DON'T hardcode bearer tokens** (`Authorization: Bearer eyJ...`)
- **DON'T commit master keys** to git

### Weak Cryptography (NEVER)
- **DON'T use `rand::thread_rng()`** for security-critical operations
- **DON'T use `std::collections::hash_map::RandomState`** for cryptographic purposes
- **DON'T use HMAC-SHA256** for JWT signing (use EdDSA per ADR-0003)
- **DON'T use bcrypt cost <12** (security floor per OWASP 2024)
- **DON'T use MD5 or SHA-1** for cryptographic hashing

### Key Management (NEVER)
- **DON'T store private keys unencrypted** in database
- **DON'T reuse nonces** for AES-GCM encryption
- **DON'T expose master key** in APIs or logs
- **DON'T skip `kid` header** in JWTs (prevents key identification)
- **DON'T rotate keys without overlap** (breaks in-flight tokens)

### Timing Attacks (NEVER)
- **DON'T skip bcrypt for non-existent users** (reveals account existence)
- **DON'T use `==` for secret comparison** (use constant-time comparison)
- **DON'T return early** on invalid credentials (consistent timing)

## Examples

### Good: EdDSA Signing with `kid`

```rust
use ring::signature::{Ed25519KeyPair, KeyPair};
use jsonwebtoken::{encode, Algorithm, Header};

pub fn sign_jwt(
    claims: &Claims,
    private_key: &[u8],
    key_id: &str,
) -> Result<String, AcError> {
    let header = Header {
        algorithm: Algorithm::EdDSA,
        key_id: Some(key_id.to_string()),  // ✅ Include kid
        ..Default::default()
    };

    let encoding_key = EncodingKey::from_ed_der(private_key);
    encode(&header, claims, &encoding_key)
        .map_err(|e| AcError::JwtSigningFailed(e.to_string()))
}
```

### Bad: Missing `kid` Header

```rust
// ❌ VIOLATION: No kid header
pub fn sign_jwt_bad(claims: &Claims, key: &[u8]) -> String {
    let header = Header::new(Algorithm::EdDSA);  // Missing kid!
    encode(&header, claims, &EncodingKey::from_ed_der(key)).unwrap()
}
```

---

### Good: CSPRNG for Secret Generation

```rust
use ring::rand::{SecureRandom, SystemRandom};

pub fn generate_client_secret() -> Result<String, CryptoError> {
    let rng = SystemRandom::new();  // ✅ CSPRNG
    let mut secret_bytes = [0u8; 32];
    rng.fill(&mut secret_bytes)
        .map_err(|_| CryptoError::RandomGenerationFailed)?;

    Ok(base64::encode_config(&secret_bytes, base64::URL_SAFE_NO_PAD))
}
```

### Bad: Non-Cryptographic RNG

```rust
use rand::Rng;

// ❌ VIOLATION: Not cryptographically secure
pub fn generate_secret_bad() -> String {
    let mut rng = rand::thread_rng();  // NOT CSPRNG!
    let secret: u64 = rng.gen();
    format!("{:x}", secret)
}
```

---

### Good: Bcrypt with Cost Factor 12

```rust
use bcrypt;

/// Hash client secret with bcrypt (cost factor 12)
pub fn hash_client_secret(secret: &str) -> Result<String, AcError> {
    bcrypt::hash(secret, 12)  // ✅ Cost factor 12
        .map_err(|e| AcError::CryptoError(e.to_string()))
}
```

### Bad: Weak Bcrypt Cost

```rust
// ❌ VIOLATION: Cost factor too low
pub fn hash_password_weak(password: &str) -> String {
    bcrypt::hash(password, 8).unwrap()  // Cost <12 is weak!
}
```

---

### Good: AES-256-GCM Encryption

```rust
use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
use ring::rand::{SecureRandom, SystemRandom};

pub fn encrypt_private_key(
    plaintext: &[u8],
    master_key: &[u8; 32],
) -> Result<EncryptedKey, CryptoError> {
    let unbound_key = UnboundKey::new(&AES_256_GCM, master_key)?;
    let key = LessSafeKey::new(unbound_key);

    // ✅ Random nonce via CSPRNG
    let rng = SystemRandom::new();
    let mut nonce_bytes = [0u8; 12];
    rng.fill(&mut nonce_bytes)?;
    let nonce = Nonce::assume_unique_for_key(nonce_bytes);

    let mut ciphertext = plaintext.to_vec();
    let tag = key.seal_in_place_separate_tag(nonce, Aad::empty(), &mut ciphertext)?;

    Ok(EncryptedKey {
        ciphertext,
        nonce: nonce_bytes.to_vec(),
        tag: tag.as_ref().to_vec(),
    })
}
```

### Bad: Reused Nonce

```rust
// ❌ VIOLATION: Hardcoded/reused nonce breaks AES-GCM security
pub fn encrypt_bad(data: &[u8], key: &[u8; 32]) -> Vec<u8> {
    let nonce = [0u8; 12];  // NEVER reuse nonces!
    // ...encryption code...
}
```

---

### Good: Environment-Based Secret Loading

```rust
use std::env;
use common::secret::SecretString;

pub fn load_master_key() -> Result<SecretString, ConfigError> {
    let key = env::var("AC_MASTER_KEY")  // ✅ From environment
        .map_err(|_| ConfigError::MissingMasterKey)?;
    Ok(SecretString::from(key))
}
```

### Bad: Hardcoded Master Key

```rust
// ❌ VIOLATION: Hardcoded secret
pub fn get_master_key_bad() -> &'static str {
    "super-secret-master-key-12345"  // NEVER!
}
```

---

### Good: Constant-Time Credential Verification

```rust
pub async fn verify_credentials(
    client_id: &str,
    client_secret: &str,
    pool: &PgPool,
) -> Result<ServiceCredential, AcError> {
    let credential = get_credential(client_id, pool).await?;

    // ✅ Always run bcrypt to prevent timing attacks
    let hash_to_verify = credential.as_ref()
        .map(|c| c.client_secret_hash.as_str())
        .unwrap_or("$2b$12$dummy_hash_for_timing_safety");

    let valid = verify_client_secret(client_secret, hash_to_verify)?;

    match (credential, valid) {
        (Some(cred), true) => Ok(cred),
        _ => Err(AcError::InvalidCredentials),
    }
}
```

### Bad: Early Return (Timing Leak)

```rust
// ❌ VIOLATION: Timing attack reveals if credential exists
pub async fn verify_bad(id: &str, secret: &str) -> bool {
    let credential = get_credential(id).await?;
    if credential.is_none() {
        return false;  // Early return leaks timing info!
    }
    bcrypt::verify(secret, &credential.unwrap().hash).unwrap()
}
```

---

### Good: Test Secrets in Test Code

```rust
#[cfg(test)]
mod tests {
    // ✅ Test secrets are excluded from guard
    const TEST_CLIENT_SECRET: &str = "test-secret-12345";

    #[test]
    fn test_credential_verification() {
        let hash = hash_client_secret(TEST_CLIENT_SECRET).unwrap();
        assert!(verify_client_secret(TEST_CLIENT_SECRET, &hash).unwrap());
    }
}
```

### Bad: Production Secret in Code

```rust
// ❌ VIOLATION: Production secret hardcoded
const ADMIN_API_KEY: &str = "sk-live-abc123def456ghi789";  // NEVER!

pub fn authenticate_admin(key: &str) -> bool {
    key == ADMIN_API_KEY
}
```

## Guards

**`scripts/guards/simple/no-hardcoded-secrets.sh`**

Checks for:

| Check | Pattern | Example Violation |
|-------|---------|-------------------|
| Secret assignments | `password = "..."` | `let password = "admin123";` |
| API key prefixes | `"sk-..."`, `"AKIA..."` | `let stripe_key = "sk-live-...";` |
| Connection strings | `"postgresql://user:pass@..."` | `let db_url = "postgresql://admin:pwd@...";` |
| Auth headers | `"Authorization: Bearer ..."` | `let auth = "Authorization: Bearer eyJ...";` |
| Long base64 strings | 40+ char base64 | Manual review for accidental secrets |

**Exclusions**:
- Test files and `#[cfg(test)]` blocks (via compiler detection)
- Environment variable references (`std::env`, `dotenvy`)
- Placeholder values (`"your-api-key-here"`, `"changeme"`)

## ADR References

- **ADR-0003: Service Authentication** - EdDSA (Ed25519) for JWT signing, bcrypt cost ≥12, CSPRNG via `ring::rand::SystemRandom`, AES-256-GCM for key encryption at rest
- **ADR-0008: Key Rotation Strategy** - Weekly key rotation, 1-week overlap for validation, JWT `kid` header required, OAuth 2.0 scoped rotation endpoint
- **ADR-0002: No Panic Policy** - All cryptographic operations return `Result<T, E>` (never panic)

## Related Principles

- **[logging-safety.md](logging-safety.md)** - Don't log secrets at runtime (complements no-hardcoded-secrets)
- Both work together: crypto.md prevents secrets in code, logging-safety.md prevents secrets in logs

## Resolution Strategies

**If guard detects hardcoded secret**:

1. **Environment Variables**: Best for deployment-specific values
   ```rust
   let secret = std::env::var("MY_SECRET")?;
   ```

2. **Configuration Files**: For structured config (not committed to git)
   ```rust
   let config: Config = config::Config::builder()
       .add_source(config::File::with_name("secrets"))
       .build()?;
   ```

3. **Secret Management**: For production (Vault, AWS Secrets Manager)
   ```rust
   let secret = vault_client.get_secret("path/to/secret").await?;
   ```

4. **SecretString**: Wrap runtime secrets to prevent logging
   ```rust
   let password = SecretString::from(env_password);
   ```

## Security Checklist

Before committing crypto code, verify:

- [ ] All JWT signatures use EdDSA (Ed25519)
- [ ] All JWTs include `kid` header
- [ ] Bcrypt cost factor is exactly 12
- [ ] All randomness uses `ring::rand::SystemRandom`
- [ ] Private keys encrypted with AES-256-GCM at rest
- [ ] No hardcoded secrets in source code
- [ ] Master key loaded from environment variable
- [ ] Constant-time comparison for secret validation
- [ ] Bcrypt runs even for non-existent credentials
- [ ] Tests verify cryptographic invariants (cost factor, key format, etc.)
