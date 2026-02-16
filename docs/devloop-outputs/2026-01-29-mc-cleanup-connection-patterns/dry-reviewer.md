# DRY Reviewer: MC Cleanup - Connection Patterns

**Task**: MC cleanup: remove legacy proto methods and fix connection patterns (Arc<RwLock> removal)

**Date**: 2026-01-30

**Files Reviewed**:
- `crates/meeting-controller/src/grpc/gc_client.rs`
- `crates/meeting-controller/src/redis/client.rs`

---

## Review Summary

Reviewed the cleaned-up connection patterns in Meeting Controller for cross-service duplication per ADR-0019. The changes simplify the codebase by leveraging tonic's `Channel` and redis's `MultiplexedConnection` clone semantics.

---

## Findings

### No BLOCKER Findings

The code correctly uses existing common utilities:
- `common::secret::{ExposeSecret, SecretString}` is properly imported and used in `gc_client.rs` (line 23)
- No common utilities exist that should have been used but weren't

### TECH_DEBT Findings

#### 1. Similar gRPC Client Patterns Between MC and GC

**Location**:
- `crates/meeting-controller/src/grpc/gc_client.rs` (MC's client to GC)
- `crates/global-controller/src/services/mc_client.rs` (GC's client to MC)

**Similarity**: ~60%

**Patterns Observed**:

| Pattern | MC's GcClient | GC's McClient |
|---------|---------------|---------------|
| Service token | `SecretString` | `SecretString` |
| Auth header | `add_auth()` helper | Inline `format!("Bearer ...")` |
| Timeout config | Constants (`GC_RPC_TIMEOUT`, etc.) | Constants (`MC_RPC_TIMEOUT_SECS`, etc.) |
| Channel handling | Direct `Channel` (eager) | `HashMap<String, Channel>` (pooled) |
| Error mapping | `McError::Grpc` | `GcError::ServiceUnavailable` |

**Analysis**: Both clients share the core pattern of:
1. Creating/reusing tonic `Channel`
2. Adding authorization header with `SecretString`
3. Making gRPC calls with timeouts
4. Error mapping with logging

The difference in connection strategy (eager vs pooled) is appropriate for their use cases:
- MC has one GC endpoint (eager is fine)
- GC talks to many MCs (pooling prevents connection churn)

**Recommendation**: Consider extracting a `GrpcClientHelper` trait or common utilities for:
- Authorization header injection
- Timeout configuration types
- Common error mapping patterns

**Priority**: Low - the implementations are working and the differences are intentional.

---

#### 2. Exponential Backoff Constants (Minor Pattern)

**Location**:
- `crates/meeting-controller/src/grpc/gc_client.rs` (lines 48-54)
- `crates/env-tests/src/eventual.rs` (similar pattern)

**Similarity**: ~40%

**Details**: MC defines `BACKOFF_BASE`, `BACKOFF_MAX`, and `MAX_REGISTRATION_RETRIES` for retry logic. Similar backoff patterns exist elsewhere in the codebase.

**Recommendation**: Not urgent - backoff configurations are often context-specific. Could consider a `common::retry::BackoffConfig` if more services need similar retry logic.

**Priority**: Very Low

---

## Existing Common Utilities Audit

Verified the following `crates/common/` modules:

| Module | Contents | Used by MC? |
|--------|----------|-------------|
| `secret.rs` | `SecretString`, `ExposeSecret` | Yes - gc_client.rs |
| `config.rs` | `DatabaseConfig`, `RedisConfig`, `ObservabilityConfig` | No - MC uses its own Config |
| `error.rs` | `DarkTowerError` | No - MC has `McError` |
| `types.rs` | ID types (`MeetingId`, `ParticipantId`, etc.) | Not in reviewed files |

The MC's custom `Config` and `McError` types are appropriate - they have service-specific fields.

---

## Verdict

**APPROVED**

No BLOCKER findings. The code correctly uses existing common utilities where appropriate. Cross-service pattern similarities are documented as tech debt for potential future extraction.

---

## Tech Debt Summary

| ID | Pattern | Services | Priority | Future Task |
|----|---------|----------|----------|-------------|
| TD-001 | gRPC auth header injection | MC, GC | Low | Extract `GrpcAuthHelper` |
| TD-002 | Exponential backoff config | MC, env-tests | Very Low | Extract `BackoffConfig` |

---

## Metrics

```
verdict: APPROVED
finding_count:
  blocker: 0
  critical: 0
  major: 0
  minor: 0
  tech_debt: 2
checkpoint_exists: true
summary: No BLOCKER findings. Code correctly uses common::secret. Two tech debt items identified for potential future extraction of gRPC client helpers and backoff configuration.
```
