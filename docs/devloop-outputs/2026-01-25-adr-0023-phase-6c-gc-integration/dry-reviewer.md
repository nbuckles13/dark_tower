# DRY Reviewer - ADR-0023 Phase 6c GC Integration

**Date**: 2026-01-26
**Reviewer**: DRY Reviewer Specialist
**Task**: ADR-0023 Phase 6c - GC Integration for Meeting Controller

---

## Files Reviewed

1. `crates/meeting-controller/src/grpc/mod.rs`
2. `crates/meeting-controller/src/grpc/gc_client.rs`
3. `crates/meeting-controller/src/grpc/mc_service.rs`
4. `crates/meeting-controller/src/redis/mod.rs`
5. `crates/meeting-controller/src/redis/lua_scripts.rs`
6. `crates/meeting-controller/src/redis/client.rs`

---

## Cross-Service Analysis

### 1. gRPC Client Pattern: GcClient (MC) vs McClient (GC)

**Location**:
- `crates/meeting-controller/src/grpc/gc_client.rs`
- `crates/global-controller/src/services/mc_client.rs`

**Pattern**: Both implement gRPC client with channel caching:
- `get_channel()` with `RwLock<Option<Channel>>` (MC single-channel) / `RwLock<HashMap<String, Channel>>` (GC multi-channel pool)
- `add_auth()` helper to inject `Authorization: Bearer` header
- Similar timeout constants (`RPC_TIMEOUT_SECS`, `CONNECT_TIMEOUT_SECS`)
- Both use `SecretString` from `common::secret` for token protection

**Classification**: **TECH_DEBT** (TD-7 - NEW)

**Rationale**:
- This is the standard gRPC client pattern emerging in the codebase
- Both implementations correctly use `common::secret::SecretString`
- Channel caching strategy differs (single vs pool) based on use case
- Similar but not identical - MC talks to one GC, GC talks to many MCs
- Does NOT ignore existing common code (no `common::grpc` exists)

**Assessment**: Ships now, consider extraction when MH client appears (Phase 7).

---

### 2. SecretString Usage

**Location**: `crates/meeting-controller/src/grpc/gc_client.rs:17`

**Pattern**: `use common::secret::{ExposeSecret, SecretString};`

**Classification**: **ACCEPTABLE**

**Rationale**: MC correctly imports and uses `SecretString` from `common::secret` crate. This is the intended pattern per ADR-0014 (SecretBox/SecretString refactor). No duplication - proper reuse of existing common code.

---

### 3. Config Pattern: Manual Debug Redaction

**Location**:
- `crates/meeting-controller/src/config.rs` (custom `fmt::Debug` implementation)
- `crates/global-controller/src/config.rs` (same pattern)

**Classification**: **ACCEPTABLE** (per ADR-0019 Section: "Acceptable Duplication Patterns")

**Rationale**: Per-service configuration with custom Debug redaction is an acceptable pattern. Each service has different sensitive fields. This is listed in the Tech Debt Registry under "Acceptable Duplication Patterns."

---

### 4. Exponential Backoff Pattern

**Location**: `crates/meeting-controller/src/grpc/gc_client.rs:47-50`
```rust
const BACKOFF_BASE_MS: u64 = 1000;
const BACKOFF_MAX_MS: u64 = 30_000;
```

**Pattern**: Exponential backoff with base delay and max cap.

**Classification**: **ACCEPTABLE**

**Rationale**:
- This is the first implementation of retry backoff in a gRPC client
- GC's `mc_client.rs` does NOT have backoff (single-shot RPC)
- `crates/env-tests/src/eventual.rs` has different backoff for test polling
- No existing common backoff utility to reuse

---

### 5. Redis Lua Scripts (Fencing Token)

**Location**: `crates/meeting-controller/src/redis/lua_scripts.rs`

**Pattern**: Lua scripts for atomic fenced operations with generation checking.

**Classification**: **ACCEPTABLE**

**Rationale**:
- First implementation of Redis Lua scripts in the codebase
- No existing Redis utilities in `common/` crate
- Fencing token pattern is MC-specific for split-brain prevention (ADR-0023)
- No duplication exists to compare against

---

### 6. FencedRedisClient Pattern

**Location**: `crates/meeting-controller/src/redis/client.rs`

**Pattern**: Redis client with:
- `MultiplexedConnection` behind `RwLock`
- Precompiled Lua `Script` objects
- Local generation cache

**Classification**: **ACCEPTABLE**

**Rationale**:
- First Redis client implementation in Dark Tower services
- GC uses PostgreSQL (no Redis patterns to compare)
- AC uses PostgreSQL (no Redis patterns to compare)
- No duplication exists in the codebase

---

### 7. Mock Trait Pattern for gRPC Clients

**Location**: `crates/global-controller/src/services/mc_client.rs` has `McClientTrait` + `MockMcClient`

**Classification**: **ACCEPTABLE** (per patterns.md: "Mock Trait Pattern for gRPC Clients")

**Rationale**: MC's `GcClient` does NOT implement a mock trait pattern yet. This is acceptable - testing may use different strategies (integration tests with real GC). When/if MC needs mocking, following GC's pattern is the right approach, not duplication.

---

## Findings Summary

### BLOCKING Findings: 0

No code exists in `common/` that was ignored. All new patterns either:
- Correctly use existing common utilities (`SecretString`)
- Implement service-specific functionality (Redis, gRPC client)
- Follow acceptable duplication patterns (config, errors)

### TECH_DEBT Findings: 1

| ID | Pattern | Location | Severity | Improvement Path |
|----|---------|----------|----------|------------------|
| TD-7 | gRPC Client with Channel Caching | MC `GcClient`, GC `McClient` | Low | Consider `common::grpc::CachingClient<T>` trait when MH client appears (Phase 7) |

---

## Tech Debt Registry Update

### TD-7: gRPC Client Channel Caching Pattern (NEW)
**Added**: 2026-01-26
**Related files**: `crates/meeting-controller/src/grpc/gc_client.rs`, `crates/global-controller/src/services/mc_client.rs`

Both MC and GC implement gRPC clients with channel caching and auth header injection. Pattern includes: `get_channel()` with RwLock-protected cache, `add_auth()` helper for Bearer token, configurable timeouts. Severity: Low (implementations differ in cache strategy). Improvement path: Consider `common::grpc::AuthenticatedClient<C>` trait when third gRPC client appears (MH). Timeline: Phase 7+ (when MH implementation begins). Note: Current duplication acceptable - implementations differ (single-channel vs multi-channel pool) and extraction cost exceeds benefit for 2 implementations.

---

## Verdict

**APPROVED**

- No BLOCKING findings
- 1 TECH_DEBT finding (TD-7) documented for future consideration
- MC correctly uses existing `common::secret` utilities
- New patterns (Redis, fencing) are service-specific, not candidates for extraction
- gRPC client pattern similarity is acceptable parallel evolution

---

## Recommendations

1. **TD-7 Follow-up**: When implementing MH client (Phase 7), evaluate whether to extract common gRPC client infrastructure or continue with service-specific implementations.

2. **Consider**: If MC needs mock testing for GcClient, follow the `McClientTrait` pattern from GC for consistency.

---

## Iteration 2 Re-Review (2026-01-26)

**Reviewer**: DRY Reviewer Specialist
**Purpose**: Verify iteration 2 fixes did not introduce new duplication

### Files Re-Reviewed

1. `crates/meeting-controller/src/grpc/gc_client.rs`
2. `crates/meeting-controller/src/grpc/mc_service.rs`
3. `crates/meeting-controller/src/redis/client.rs`
4. `crates/meeting-controller/src/errors.rs`

### Verification Checklist

| Check | Result |
|-------|--------|
| Uses `common::secret::SecretString` for tokens | PASS |
| Uses `common::secret::ExposeSecret` for token access | PASS |
| No new code duplicating existing common utilities | PASS |
| Error types follow established per-service pattern | PASS |
| Redis client pattern is MC-specific (no common Redis) | PASS |
| gRPC channel caching follows documented TD-7 pattern | PASS |

### New Duplication Analysis

**No new duplication introduced in iteration 2 fixes.**

The reviewed files continue to:
- Properly import and use `common::secret::{ExposeSecret, SecretString}` (gc_client.rs:17)
- Follow the established error type pattern (McError vs GcError - both service-specific)
- Use the documented gRPC client pattern (TD-7)

### Comparison with GC Parallel Patterns

| Pattern | MC Implementation | GC Implementation | Status |
|---------|-------------------|-------------------|--------|
| Secret handling | `common::secret::SecretString` | `common::secret::SecretString` | ACCEPTABLE (shared) |
| Error types | `McError` with `error_code()` | `GcError` with `status_code()` | ACCEPTABLE (service-specific) |
| gRPC client | `GcClient` with channel cache | `McClient` with channel pool | TD-7 (documented) |
| Redis client | `FencedRedisClient` | N/A (uses PostgreSQL) | ACCEPTABLE (MC-specific) |

### Iteration 2 Verdict

**APPROVED**

- No new BLOCKING findings
- TD-7 (gRPC client caching) remains the only TECH_DEBT item
- All iteration 2 fixes maintain proper use of common utilities
- No regression from iteration 1 review

---

## Iteration 3 Re-Review (2026-01-26)

**Reviewer**: DRY Reviewer Specialist
**Purpose**: Review new `auth_interceptor.rs` for cross-service duplication

### New File Reviewed

1. `crates/meeting-controller/src/grpc/auth_interceptor.rs`

### Cross-Service Comparison: Auth Interceptors

**MC Implementation** (`crates/meeting-controller/src/grpc/auth_interceptor.rs`):
- `McAuthInterceptor` implements `tonic::service::Interceptor`
- `extract_token()` helper extracts Bearer token from metadata
- `MAX_TOKEN_SIZE = 8192` constant
- Basic token validation (non-empty, size limit)
- Returns `Status::unauthenticated()` on failure
- Note: "Full cryptographic validation deferred to Phase 6h (JWKS integration)"

**GC Implementation** (`crates/global-controller/src/grpc/auth_layer.rs`):
- `GrpcAuthInterceptor` implements `tonic::service::Interceptor`
- `extract_token()` helper extracts Bearer token from metadata
- `MAX_TOKEN_SIZE = 8192` constant
- Basic token validation (non-empty, size limit)
- Returns `Status::unauthenticated()` on failure
- Has `JwtValidator` for full validation via async layer

### Code Similarity Analysis

| Element | MC auth_interceptor.rs | GC auth_layer.rs | Identical? |
|---------|------------------------|------------------|------------|
| `MAX_TOKEN_SIZE` constant | 8192 | 8192 | YES |
| `extract_token()` signature | `fn extract_token<'a>(&self, auth_value: &'a MetadataValue<...>) -> Option<&'a str>` | Same | YES |
| `extract_token()` body | `auth_str.strip_prefix("Bearer ")` | Same | YES |
| Missing header error | `"Missing authorization header"` | Same | YES |
| Invalid format error | `"Invalid authorization format"` | Same | YES |
| Empty token error | `"Empty token"` | Same | YES |
| Size limit error | `"Invalid token"` | Same | YES |
| Tracing target | `"mc.grpc.auth"` | `"gc.grpc.auth"` | NO (expected) |

### Classification: **TECH_DEBT** (TD-8 - NEW)

**Rationale**:
1. **No existing common utility exists** - There is no `common::grpc::auth` module
2. **Parallel evolution is acceptable** - Both services need auth interceptors
3. **Service-specific differences exist**:
   - MC: Simple structural validation (JWKS deferred to Phase 6h)
   - GC: Full JWT validation via `JwtValidator` + async layer
   - Different tracing targets (correctly scoped per service)
4. **Current duplication cost is LOW**:
   - ~50 lines of nearly identical code
   - Well-documented validation rules
   - Security requirements are identical (8KB limit per ADR standards)

### Why NOT BLOCKING

Per ADR-0019 Section 3 (DRY Reviewer Guidelines):

> "BLOCKING: Code exists in common/ that was ignored"

There is NO auth interceptor code in `common/` that was ignored. The `common::secret` module provides `SecretString` (which MC correctly uses elsewhere), but no gRPC auth utilities exist.

This is **parallel evolution** of a common pattern, not ignoring existing utilities.

### Why TECH_DEBT

The duplication represents a reasonable extraction candidate for future:
- 3+ services will need this pattern (MC, GC, MH at minimum)
- Core validation logic is identical across services
- Extraction would ensure consistent security enforcement

### Proposed Tech Debt Entry

**TD-8: gRPC Auth Interceptor Pattern**
**Added**: 2026-01-26
**Related files**:
- `crates/meeting-controller/src/grpc/auth_interceptor.rs`
- `crates/global-controller/src/grpc/auth_layer.rs`

**Pattern**: Both MC and GC implement gRPC auth interceptors with identical:
- `MAX_TOKEN_SIZE` (8KB) constant
- `extract_token()` helper for Bearer extraction
- Basic token validation (non-empty, size limit)
- Error message strings

**Severity**: Low (security-critical logic is consistent, no divergence risk)

**Improvement Path**: Consider `common::grpc::BearerTokenExtractor` trait when:
1. MH auth interceptor is implemented (Phase 7)
2. If auth validation rules need to change (single update point)

**Timeline**: Phase 7+ (when MH implementation begins)

**Note**: MC intentionally defers JWKS validation to Phase 6h; GC has full validation. This architectural difference justifies separate implementations currently.

### Iteration 3 Verification Checklist

| Check | Result |
|-------|--------|
| Auth interceptor ignores existing common utility | NO (none exists) |
| Error messages match security requirements | PASS |
| Token size limit matches ADR standards (8KB) | PASS |
| Tracing targets are service-scoped | PASS |
| Code follows established interceptor pattern | PASS |

### Iteration 3 Verdict

**APPROVED**

- No BLOCKING findings
- 1 new TECH_DEBT finding (TD-8) documented
- Auth interceptor follows established pattern from GC
- No common auth utilities were ignored (none exist)
- Parallel evolution is acceptable per ADR-0019

---

## Cumulative Findings Summary

### BLOCKING Findings: 0

### TECH_DEBT Findings: 2

| ID | Pattern | Location | Severity | Status |
|----|---------|----------|----------|--------|
| TD-7 | gRPC Client Channel Caching | MC `GcClient`, GC `McClient` | Low | From iteration 1 |
| TD-8 | gRPC Auth Interceptor | MC `auth_interceptor.rs`, GC `auth_layer.rs` | Low | NEW in iteration 3 |

---

## Final Verdict

**APPROVED**

Phase 6c implementation correctly:
- Uses `common::secret::SecretString` where appropriate
- Follows established gRPC patterns from GC
- Does not ignore any existing common utilities
- Documents parallel evolution as TECH_DEBT for future extraction consideration
