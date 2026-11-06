# ADR-0027: Approved Cryptographic Algorithms

## Status

Accepted

## Context

Dark Tower requires a curated list of approved cryptographic algorithms to prevent ad-hoc algorithm choices. This reference belongs in an ADR as it represents an architectural decision about which algorithms are permitted.

Last reviewed: 2026-11-05
Next review: 2027-02-10

## Decision

### Approved Algorithms

| Purpose | Algorithm | Library | Notes |
|---------|-----------|---------|-------|
| Signatures | Ed25519 | ring | EdDSA, 256-bit security; direct use incl. key generation — JWT sign/verify: row below |
| JWT signing/verification (EdDSA) | Ed25519 | jsonwebtoken feature aws_lc_rs (aws-lc-rs) | jsonwebtoken >=10 has no ring backend; rust_crypto backend not approved (non-strict dalek verify, pulls rsa / RUSTSEC-2023-0071) |
| Symmetric encryption | AES-256-GCM | ring | AEAD |
| Password hashing | bcrypt | bcrypt crate | cost=12 (~250ms) |
| Random generation | CSPRNG | ring::rand::SystemRandom | Required for all secrets |
| Key derivation | HKDF-SHA256, HKDF-SHA512 | ring::hkdf (`HKDF_SHA256`, `HKDF_SHA512`) | Per-resource key scoping. **`HKDF_SHA512` is required by the SFrame ciphersuite** `AES_256_GCM_SHA512_128` (0x0005, RFC 9605 §8.1), which ADR-0036 §4 selects for media frames: the suite fixes the hash, so this is not a free choice. Consequence worth stating because it is asserted in code: the extract PRK is **64 bytes, not 32**, and the derivation is the direct GCM form with **no `enc_key`/`auth_key` split** (that split belongs to the AES-CTR suites). Not an upgrade recommendation — HKDF-SHA256 remains correct everywhere it is already used, and neither is deprecated |
| Message authentication | HMAC-SHA256 | ring::hmac | Session binding tokens |

Enforced by: `crates/dt-guard/src/resolved_features/policy.rs` (crypto-provider features as resolved by cargo) — amend it in the same commit as this table.

### Deprecated (do not use)

| Algorithm | Reason | Replacement |
|-----------|--------|-------------|
| HS256 | Symmetric JWT signing | EdDSA |
| RSA < 2048 | Weak key size | Ed25519 |
| MD5, SHA1 | Collision attacks | SHA-256+ |
| DES, 3DES, RC4 | Weak algorithms | AES-256-GCM |
| Direct master key usage | No isolation | HKDF for per-resource keys |

### Usage Guidelines

- Use ring for direct crypto operations. The only approved non-ring backends are aws-lc-rs inside jsonwebtoken (aws_lc_rs feature) and the TLS providers selected by rustls/quinn/wtransport (runtime TLS currently uses rustls' ring provider). Do not enable jsonwebtoken rust_crypto; do not call aws-lc-rs directly without amending this ADR.
- Use constant-time comparisons via `ring::constant_time::verify_slices_are_equal()` or `subtle::ConstantTimeEq`
- For HMAC verification, use `ring::hmac::verify()` which performs constant-time comparison internally
- Generate all secrets with SystemRandom, never rand crate
- Tune bcrypt cost for ~250ms on target hardware
- Use HKDF to derive per-resource keys from master secrets
- Wrap all secrets in `SecretBox<T>` or `SecretString` for automatic redaction

## Review Triggers

Update this ADR when:
- NIST publishes new recommendations
- Cryptographic weaknesses discovered
- Performance characteristics change significantly
- New crate versions with security fixes

## Consequences

### Positive
- Single authoritative source for algorithm decisions
- Discoverable via ADR index
- Version-controlled change history

### Negative
- Requires ADR update process for algorithm changes (intentionally higher bar)

## Participants

- Security specialist: Primary owner
- Auth Controller specialist: Primary consumer
