# Test Specialist Checkpoint: Cross-Service E2E Tests

**Specialist**: test
**Task**: Implement cross-service e2e tests for AC + GC flows
**Timestamp**: 2026-01-17
**Status**: Iteration 4 complete, all verification passed

---

## Patterns Discovered

### 1. GcClient Fixture Pattern
**Pattern**: Mirror existing AuthClient pattern for new service clients.

Created `GcClient` following the same structure as `AuthClient`:
- Error enum with thiserror
- Request/response structs with serde
- Async methods returning `Result<T, GcClientError>`
- Raw methods for testing error paths
- Health check method for availability detection

This ensures consistency across fixtures and makes tests predictable.

### 2. Graceful Service Availability Check
**Pattern**: Use `is_gc_available()` to skip tests when service not deployed.

```rust
if !cluster.is_gc_available().await {
    println!("SKIPPED: GC not deployed");
    return;
}
```

This allows running env-tests even when not all services are deployed, which is important during phased development.

### 3. Cross-Service Token Validation Flow
**Pattern**: Test the full token validation chain: AC issues -> GC validates -> GC returns claims.

The `/v1/me` endpoint provides a clean way to validate that:
1. AC JWKS is accessible from GC
2. GC correctly parses and validates tokens
3. Claims are correctly extracted and returned

This single endpoint exercises the entire token validation pipeline.

### 4. Raw Response Methods for Error Testing
**Pattern**: Provide `raw_*` methods that return Response instead of parsed result.

```rust
pub async fn raw_join_meeting(...) -> Result<reqwest::Response, GcClientError>
```

This allows tests to:
- Check exact status codes (401, 403, 404)
- Read error bodies for debugging
- Test authentication failures without parsing

---

## Gotchas Encountered

### 1. UUID Serde Feature Required
**Issue**: UUID deserialization failed until `serde` feature added to Cargo.toml.
**Resolution**: Changed `uuid = { version = "1.11", features = ["v4"] }` to include `serde` feature.

### 2. Response Borrow After Move
**Issue**: `response.status()` called after `response.text().await`.
**Resolution**: Store status before consuming response body: `let status = response.status();`

### 3. expect_fun_call Clippy Lint
**Issue**: Using `expect(&format!(...))` triggers clippy warning.
**Resolution**: Use `unwrap_or_else(|_| panic!(...))` for dynamic error messages in loops.

### 4. Cross-Service Tests Need Both Services Running
**Issue**: Tests will fail if GC not deployed but AC is running.
**Resolution**: All cross-service tests check `is_gc_available()` first and skip with message if GC unavailable.

### 5. Debug Trait Credential Leak Risk (Fixed in Iteration 2)
**Issue**: Structs with sensitive fields (`token`, `captcha_token`) had `#[derive(Debug)]` which could leak credentials if logged with `{:?}`.
**Resolution**: Implemented custom `Debug` traits that redact sensitive fields:
- `JoinMeetingResponse.token` → `[REDACTED]`
- `GuestTokenRequest.captcha_token` → `[REDACTED]`

Added tests to verify redaction works correctly.

### 6. Error Response Body Credential Leak Risk (Fixed in Iteration 4)
**Issue**: `GcClientError::RequestFailed` included raw HTTP response bodies without sanitization. Auth error responses could contain JWTs or sensitive internal details.
**Resolution**: Implemented `sanitize_error_body()` function that:
- Uses regex to detect and replace JWT patterns with `[JWT_REDACTED]`
- Detects and replaces Bearer token patterns with `[BEARER_REDACTED]`
- Truncates long error bodies (>256 chars) to prevent info disclosure

### 7. MeResponse Debug Exposes Subject ID (Fixed in Iteration 4)
**Issue**: `MeResponse` used derived `Debug` which exposed the `sub` (user/client ID) field.
**Resolution**: Implemented custom `Debug` for `MeResponse` that redacts `sub` field.

---

## Key Decisions

### 1. Optional GC in ClusterConnection
Made GC port-forward optional (doesn't fail if GC not running), unlike AC which is required. This allows running existing env-tests even when GC is not deployed yet.

### 2. Test Structure Follows ADR-0020 Flows
Organized tests by the three flows defined in ADR-0020:
1. Authenticated user join
2. Guest token
3. Meeting settings update

Each flow has multiple tests covering happy path, auth errors, and validation.

### 3. Skip vs Fail for Missing Services
Chose to `return` with SKIPPED message rather than fail when GC not available. This is better for CI where services may be deployed incrementally.

### 4. Meeting-Related Tests Expect 404
Since we don't have seeded meetings, tests expect 404 for meeting operations. This validates:
- Auth middleware works (token validated before meeting lookup)
- Proper error responses returned
- No information leakage in error messages

---

## Files Created/Modified

| File | Action | Purpose |
|------|--------|---------|
| `src/fixtures/gc_client.rs` | Created | GC API client for tests |
| `src/fixtures/mod.rs` | Modified | Export gc_client |
| `src/cluster.rs` | Modified | Add gc_base_url, gc_service port, health check |
| `src/lib.rs` | Modified | Update docs for GC port |
| `Cargo.toml` | Modified | Add uuid serde feature, regex dependency |
| `tests/21_cross_service_flows.rs` | Created | 12 cross-service e2e tests |

---

## Verification Results

### Layer 1: cargo check
**Status**: PASS

### Layer 2: cargo fmt
**Status**: PASS (after formatting)

### Layer 3: Guards
**Status**: Not run (env-tests excluded from guards)

### Layer 4: Unit Tests
**Status**: PASS (20 tests after Iteration 4)
```
test result: ok. 20 passed; 0 failed
```

*Note:
- Iteration 2: +2 tests for Debug trait redaction verification
- Iteration 4: +5 tests for error body sanitization and MeResponse Debug*

### Layer 5: Integration Tests
**Status**: Feature-gated (requires cluster)
Tests compile but require `--features flows` and running cluster.

### Layer 6: Clippy
**Status**: PASS

### Layer 7: Semantic Guards
**Status**: PASS (after Iteration 4 fixes)

The credential-leak semantic guard flagged issues across iterations:

**Iteration 2 fixes**:
- `JoinMeetingResponse` with `#[derive(Debug)]` containing JWT `token` field
- `GuestTokenRequest` with `#[derive(Debug)]` containing `captcha_token` field

**Iteration 4 fixes**:
- `GcClientError::RequestFailed` including unsanitized HTTP response bodies (HIGH risk)
- `MeResponse` with `#[derive(Debug)]` exposing `sub` field (LOW risk)

After implementing `sanitize_error_body()` and custom `Debug` traits, the guard now reports SAFE:
- All structs with sensitive fields have custom Debug that redacts them
- Error response bodies are sanitized to remove JWT/Bearer patterns
- Long error bodies are truncated to prevent information disclosure

---

## Test Summary

### Cross-Service Flow Tests (21_cross_service_flows.rs)

| Test | Description | Expected Result |
|------|-------------|-----------------|
| `test_ac_gc_services_healthy` | Both services respond to health checks | Both healthy |
| `test_gc_validates_ac_token_via_me_endpoint` | AC token validated by GC | Claims returned |
| `test_gc_rejects_unauthenticated_requests` | No auth header | 401 |
| `test_gc_rejects_invalid_token` | Tampered token | 401 |
| `test_meeting_join_requires_authentication` | No auth for meeting join | 401 |
| `test_meeting_join_returns_404_for_unknown_meeting` | Non-existent meeting | 404 |
| `test_guest_token_endpoint_is_public` | No auth required | 404 (not 401) |
| `test_guest_token_validates_display_name` | Empty display name | 400 |
| `test_meeting_settings_requires_authentication` | No auth for settings | 401 |
| `test_meeting_settings_returns_404_for_unknown_meeting` | Non-existent meeting | 404 |
| `test_token_validation_consistency` | Same token validated 5 times | All succeed |
| `test_multiple_tokens_validated` | 3 different tokens | All validated |

---

## Next Steps

1. **Code review**: Security + Code Quality reviewers
2. **Reflection**: Update test specialist knowledge files
3. **Integration**: Deploy GC to kind cluster to run full tests
4. **Seed data**: Create test meetings to enable full flow testing

---

## Notes for Future Work

1. **Meeting seeding**: When test data seeding is implemented, update tests to use real meetings instead of expecting 404.

2. **AC internal endpoints**: Tests for actual meeting token issuance require AC's `/api/v1/auth/internal/meeting-token` and `/api/v1/auth/internal/guest-token` endpoints to be implemented.

3. **Captcha validation**: Guest token tests currently pass any captcha token. When captcha validation is implemented, tests will need valid test tokens.

---

## Reflection Summary (2026-01-18)

### Knowledge Files Updated

**patterns.md**: Added 2 entries
- Cross-Service Client Fixture with Graceful Service Availability
- Error Body Sanitization in Test Clients

**gotchas.md**: Added 2 entries
- Custom Debug Not Sufficient for Error Response Bodies
- Response Body Consumed Before Status Check

**integration.md**: Added 2 entries
- For Security Specialist: Error Body Sanitization in Test Clients
- For All Specialists: Cross-Service Test Client Consistency

### Key Learnings

1. **Semantic guards find real issues**: The credential-leak guard caught errors across 3 iterations that would have made it to production. Custom Debug is not sufficient - error bodies need sanitization at capture time.

2. **GcClient is now the reference pattern**: The `sanitize_error_body()` enhancement makes GcClient more complete than AuthClient. Future service clients should follow GcClient, and AuthClient should be backported.

3. **Graceful degradation for multi-service tests**: The `is_gc_available()` pattern allows running test suites during phased deployments without failures on missing services.
