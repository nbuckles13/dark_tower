# Known Shared Patterns in Common Crate

Last updated: 2026-02-12

## Currently in `crates/common/`

| Pattern | Module | Services Using |
|---------|--------|----------------|
| `SecretString`, `SecretBox` | `common::secret` | AC, GC, MC |
| `DarkTowerError` | `common::error` | All |
| Domain IDs (OrganizationId, UserId, etc.) | `common::types` | All |
| Configuration structs | `common::config` | All |
| JWT utilities (`extract_kid`, `validate_iat`) | `common::jwt` | AC, GC |
| JWT constants (`MAX_JWT_SIZE_BYTES`, clock skew) | `common::jwt` | AC, GC, MC |
| `ServiceClaims` struct | `common::jwt` | AC, GC |
| EdDSA key decoding (`decode_ed25519_public_key_pem/jwk`) | `common::jwt` | AC, GC |
| `TokenManager` (OAuth 2.0 client) | `common::token_manager` | GC, MC |

## Outstanding Tech Debt (Candidates for Extraction)

Source: `docs/dev-loop-outputs/*/main.md` Tech Debt sections

### High Priority

| Pattern | Locations | Estimated Lines | Notes |
|---------|-----------|-----------------|-------|
| `HealthState` pattern | MC, could be shared | ~50 | Generic health state tracking |

### Medium Priority

| Pattern | Locations | Notes |
|---------|-----------|-------|
| `ParticipantType` enum | GC `services/ac_client.rs`, AC `models/mod.rs` | TD-3 from 2026-01-23 |
| `MeetingRole` enum | GC `services/ac_client.rs`, AC `models/mod.rs` | TD-3 from 2026-01-23 |
| Token TTL constants (900s) | GC, AC, MC | Multiple places define same TTL |
| Rate limiting patterns | AC, GC | Similar middleware structure |

### Low Priority (Deferred)

| Pattern | Locations | Notes |
|---------|-----------|-------|
| Redis ConnectionManager wrapper | MC | Optional enhancement |
| Histogram bucket configuration | MC metrics | Align with SLO targets |

## Resolved Tech Debt

| Pattern | Resolution | Date |
|---------|------------|------|
| JWT `extract_kid` duplication | Extracted to `common::jwt` | 2026-01-30 |
| JWT Claims struct | Extracted to `common::jwt::ServiceClaims` | 2026-01-30 |
| Clock skew constants | Extracted to `common::jwt` | 2026-01-30 |
| EdDSA key handling | Extracted to `common::jwt` | 2026-01-30 |
| Static service tokens | Replaced with `TokenManager` | 2026-02-02 |
| Health Checker Task (TD-13) | Extracted to `generic_health_checker.rs` with closure-based generic; simplified in iteration 2 (removed config struct, `.instrument()` chaining) | 2026-02-12 |

## False Positives

These look like duplication but are intentionally separate:

| Pattern | Reason |
|---------|--------|
| Service-specific error types | Different error variants per service |
| Config structs with same field names | Service-specific defaults and validation |
| Test utilities in `*-test-utils` crates | Intentionally service-scoped |
| Service-specific health endpoints | Different readiness criteria |

## When to Flag

**BLOCKER**: Code exists in `common` but service reimplements it
**TECH_DEBT**: Similar code in 2+ services, not yet in `common`

## Finding More Tech Debt

```bash
# Search dev-loop outputs for tech debt
grep -r "Tech Debt" docs/dev-loop-outputs/*/main.md

# Search for TECH_DEBT findings in reviews
grep -r "TECH_DEBT" docs/dev-loop-outputs/
```
