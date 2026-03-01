# ADR-0027: Approved Cryptographic Algorithms

## Status

Accepted

## Context

Dark Tower requires a curated list of approved cryptographic algorithms to prevent ad-hoc algorithm choices. This reference belongs in an ADR as it represents an architectural decision about which algorithms are permitted.

Last reviewed: 2026-02-10
Next review: 2027-02-10

## Decision

### Approved Algorithms

| Purpose | Algorithm | Library | Notes |
|---------|-----------|---------|-------|
| Signatures | Ed25519 | ring | EdDSA, 256-bit security |
| Symmetric encryption | AES-256-GCM | ring | AEAD |
| Password hashing | bcrypt | bcrypt crate | cost=12 (~250ms) |
| Random generation | CSPRNG | ring::rand::SystemRandom | Required for all secrets |
| Key derivation | HKDF-SHA256 | ring::hkdf | Per-resource key scoping |
| Message authentication | HMAC-SHA256 | ring::hmac | Session binding tokens |

### Deprecated (do not use)

| Algorithm | Reason | Replacement |
|-----------|--------|-------------|
| HS256 | Symmetric JWT signing | EdDSA |
| RSA < 2048 | Weak key size | Ed25519 |
| MD5, SHA1 | Collision attacks | SHA-256+ |
| DES, 3DES, RC4 | Weak algorithms | AES-256-GCM |
| Direct master key usage | No isolation | HKDF for per-resource keys |

### Usage Guidelines

- Always use ring crate for new crypto operations
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
