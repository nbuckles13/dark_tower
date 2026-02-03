# DRY Reviewer Checkpoint

**Task**: GC TokenManager Integration
**Date**: 2026-02-02
**Reviewer**: DRY Reviewer Specialist

---

## Files Reviewed

| File | Description |
|------|-------------|
| `crates/global-controller/src/config.rs` | Added `gc_client_id`, `gc_client_secret` fields |
| `crates/global-controller/src/services/mc_client.rs` | Migrated from `SecretString` to `TokenReceiver` |
| `crates/global-controller/src/services/ac_client.rs` | Migrated from `String` to `TokenReceiver` |
| `crates/common/src/token_manager.rs` | Added `from_watch_receiver()` constructor |

---

## Cross-Service Pattern Analysis

### 1. TokenReceiver Usage

Searched for `TokenReceiver` usage across all services:

| Service | Uses TokenReceiver | Notes |
|---------|-------------------|-------|
| global-controller | YES | GC->MC and GC->AC communication |
| meeting-controller | NO | Uses static `MC_SERVICE_TOKEN` |
| media-handler | NO | Not yet implemented |
| ac-service | N/A | Provides tokens, doesn't consume them |

**Finding**: Meeting Controller (`crates/meeting-controller/src/config.rs` lines 85-91 and `crates/meeting-controller/src/grpc/gc_client.rs`) uses a static `service_token: SecretString` pattern similar to what GC previously had.

**Severity**: TECH_DEBT - MC should be migrated to use TokenManager in a follow-up task, but this is not blocking since:
1. MC->GC authentication exists and works with static tokens
2. This integration focuses on GC only
3. MC integration would be a separate task

### 2. OAuth Client Credentials Configuration

Pattern: `client_id: String` + `client_secret: SecretString` in config

| Service | Pattern Present | Notes |
|---------|----------------|-------|
| global-controller | YES | `gc_client_id`, `gc_client_secret` |
| meeting-controller | NO | Uses `service_token` directly |
| ac-service | YES | `client_id/secret` for registered services (different purpose - issuer) |

**Finding**: No duplication concern - GC is a service authenticating TO AC (consumer), while AC stores credentials FOR services (provider). Different domains.

### 3. `from_watch_receiver()` Usage

Added constructor in `common::token_manager::TokenReceiver` for testing:

| Crate | Uses Constructor | Purpose |
|-------|-----------------|---------|
| global-controller/services/mc_client.rs | YES | Unit test mocking |
| global-controller/services/ac_client.rs | YES | Unit test mocking |
| global-controller/tests/*.rs | YES | Integration test mocking |
| gc-test-utils | YES | Server harness mocking |

**Finding**: This is correctly centralized in `common`. All usages import from `common::token_manager::TokenReceiver`.

### 4. SecretString Pattern

Verified all OAuth-related secrets use `common::secret::SecretString`:

| Location | Field | Uses SecretString |
|----------|-------|------------------|
| GC config | `gc_client_secret` | YES |
| MC config | `service_token` | YES |
| MC config | `binding_token_secret` | YES |
| MC config | `redis_url` | YES |
| Common TokenManager | `client_secret` | YES |

**Finding**: Consistent usage of `SecretString` for sensitive data. No duplication concern.

---

## BLOCKER Findings (Must Fix)

**None identified.**

All code correctly uses exports from `common`:
- `common::token_manager::TokenReceiver` - properly imported and used
- `common::token_manager::spawn_token_manager` - properly imported and used
- `common::secret::SecretString` - properly imported and used
- `common::secret::ExposeSecret` - properly imported and used

---

## TECH_DEBT Findings (Document for Follow-up)

### TD-DRY-001: Meeting Controller Static Token

**Location**: `crates/meeting-controller/src/config.rs` (lines 85-91) and `crates/meeting-controller/src/grpc/gc_client.rs`

**Description**: MC uses static `MC_SERVICE_TOKEN` env var with `SecretString` for MC->GC authentication. This is the same pattern GC previously used before this integration.

**Recommendation**: In a future task, integrate TokenManager into MC:
1. Add `mc_client_id` and `mc_client_secret` to MC config
2. Spawn TokenManager during MC startup
3. Pass TokenReceiver to GcClient
4. Remove `MC_SERVICE_TOKEN` env var

**Tracking**: Add to `.claude/TODO.md` for Phase 6 MC hardening

---

## Summary

The implementation correctly uses centralized code from the `common` crate:
- `TokenReceiver` is properly imported from `common::token_manager`
- `from_watch_receiver()` was added to `common` to enable testing without full manager spawn
- `SecretString` is consistently used for all secrets
- No code duplication was introduced that should have been centralized

The pattern is now established in GC and can be replicated in MC when that service is updated.

---

## Verdict

**APPROVED**

| Category | Count |
|----------|-------|
| BLOCKER | 0 |
| TECH_DEBT | 1 |

The integration correctly uses the shared `common` crate exports. One TECH_DEBT item identified (MC uses static token pattern that should eventually migrate to TokenManager), but this does not block the current GC integration work.
