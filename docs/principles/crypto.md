# Principle: Cryptography

**All cryptographic operations MUST use approved algorithms and libraries.** EdDSA for signatures, bcrypt for passwords, ring for randomness.

**ADRs**: ADR-0003 (Service Auth), ADR-0008 (Key Rotation), ADR-0002 (No-Panic)
**Guards**: `scripts/guards/simple/no-hardcoded-secrets.sh`

---

## DO

### Signatures
- **Use EdDSA (Ed25519)** for all JWT signing - via `jsonwebtoken` crate
- **Include `kid` header** in all JWTs to identify signing key
- **Rotate keys weekly** with 1-week validation overlap for in-flight tokens

### Password Hashing
- **Use bcrypt with cost factor ≥12** for all password/secret hashing
- **Run bcrypt even for non-existent accounts** to prevent timing attacks (use dummy hash)

### Random Number Generation
- **Use `ring::rand::SystemRandom`** for all security-critical randomness
- **Generate 256-bit secrets** (32 bytes) for client credentials
- **Use CSPRNG** for nonces, IVs, session tokens, API keys

### Encryption at Rest
- **Use AES-256-GCM** for encrypting private keys in database
- **Generate random 96-bit nonces** per encryption - never reuse
- **Store nonce and auth tag** alongside ciphertext

### Secret Management
- **Load secrets from environment** (`std::env::var`, `AC_MASTER_KEY`)
- **Wrap secrets in `SecretString`** to prevent logging
- **Never commit secrets** to version control

### Timing Safety
- **Use constant-time comparison** for secret validation
- **Avoid early returns** that leak timing information

---

## DON'T

### Hardcoded Secrets
- **NEVER hardcode passwords, API keys, or tokens** in source code
- **NEVER commit master keys** to git
- **NEVER embed credentials** in connection strings

### Weak Cryptography
- **NEVER use `rand::thread_rng()`** for security - not CSPRNG
- **NEVER use HMAC-SHA256** for JWT signing - use EdDSA per ADR-0003
- **NEVER use bcrypt cost <12** - security floor per OWASP 2024
- **NEVER use MD5 or SHA-1** for cryptographic hashing

### Key Management
- **NEVER store private keys unencrypted** in database
- **NEVER reuse nonces** for AES-GCM (breaks security)
- **NEVER skip `kid` header** in JWTs
- **NEVER rotate without overlap** - breaks in-flight tokens

### Timing Attacks
- **NEVER skip bcrypt for non-existent users** - reveals account existence
- **NEVER use `==` for secret comparison** - use constant-time
- **NEVER return early** on credential failure

---

## Quick Reference

| Purpose | Algorithm/Library | Parameter |
|---------|-------------------|-----------|
| JWT signing | EdDSA (Ed25519) | `jsonwebtoken`, `ring` |
| Password hashing | bcrypt | cost factor ≥12 |
| Randomness | `ring::rand::SystemRandom` | CSPRNG |
| Encryption at rest | AES-256-GCM | 256-bit key, 96-bit nonce |
| Secret wrapping | `SecretString` | `common::secret` |

| Secret Type | Size | Generation |
|-------------|------|------------|
| Client secret | 256 bits (32 bytes) | CSPRNG, base64 encoded |
| AES key | 256 bits (32 bytes) | Environment variable |
| Nonce | 96 bits (12 bytes) | CSPRNG, per-operation |
| Key ID (kid) | UUID or timestamp | Unique per key |

| Key Rotation | Value |
|--------------|-------|
| Rotation interval | Weekly |
| Overlap period | 1 week |
| Rate limit (normal) | 1 per 6 days |
| Rate limit (emergency) | 1 per hour |

---

## Guards

**`scripts/guards/simple/no-hardcoded-secrets.sh`** detects:
- Secret assignments (`password = "..."`)
- API key prefixes (`sk-...`, `AKIA...`)
- Connection strings with credentials
- Auth headers with tokens
- Long base64 strings (potential secrets)

**Exclusions**: Test files, `#[cfg(test)]`, env var references, placeholders
