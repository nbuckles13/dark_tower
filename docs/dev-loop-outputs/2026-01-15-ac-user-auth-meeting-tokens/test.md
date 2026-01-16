# Test Specialist Code Review

**Date**: 2026-01-15
**Reviewer**: Test Specialist
**Files Reviewed**:
- `crates/ac-service/src/handlers/internal_tokens.rs`
- `crates/ac-service/src/models/mod.rs`

---

## Verdict: FINDINGS

The implementation has good unit test coverage for type serialization/deserialization and enum conversions, but **lacks critical integration tests** for the actual handler logic and security-relevant scenarios.

---

## Test Coverage Assessment

### Current Unit Tests (20 total)

#### internal_tokens.rs (9 tests)
| Test | What it covers | Quality |
|------|---------------|---------|
| `test_meeting_token_request_deserialization` | Full JSON parsing | Good |
| `test_meeting_token_request_defaults` | Default values | Good |
| `test_guest_token_request_deserialization` | Full JSON parsing | Good |
| `test_guest_token_request_defaults` | Default values | Good |
| `test_internal_token_response_serialization` | Response format | Good |
| `test_participant_type_as_str` | Enum conversion | Good |
| `test_meeting_role_as_str` | Enum conversion | Good |
| `test_max_ttl_constant` | Constant value | Minimal |
| `test_required_scope_constant` | Constant value | Minimal |

#### models/mod.rs (11 new ADR-0020 tests)
| Test | What it covers | Quality |
|------|---------------|---------|
| `test_participant_type_display` | Display trait | Good |
| `test_participant_type_default` | Default trait | Good |
| `test_participant_type_serde` | Serialize/Deserialize | Good |
| `test_meeting_role_display` | Display trait | Good |
| `test_meeting_role_default` | Default trait | Good |
| `test_meeting_role_serde` | Serialize/Deserialize | Good |
| `test_internal_token_response_serialization` | Response format | Good |
| `test_meeting_token_request_full` | All fields | Good |
| `test_guest_token_request_full` | All fields | Good |
| `test_default_ttl_value` | Default function | Minimal |
| `test_default_waiting_room_value` | Default function | Minimal |

### Coverage Gaps Identified

**Type coverage**: Excellent (all enum variants, serde, defaults)
**Handler coverage**: **MISSING** (0%)
**Security behavior coverage**: **MISSING** (critical)

---

## Missing Tests

### P0 - Security Critical (BLOCKING)

1. **Scope validation rejection test**
   - Test `handle_meeting_token` returns 403 when token lacks `internal:meeting-token` scope
   - Test `handle_guest_token` returns 403 when token lacks `internal:meeting-token` scope
   - Required pattern: Similar to `test_admin_endpoint_rejects_insufficient_scope` in `admin_auth_tests.rs`
   - Status: **BLOCKING** - Security control must be verified

2. **Scope extraction edge cases**
   - Test with empty scope string
   - Test with whitespace-only scope
   - Test with scope as substring (e.g., `internal:meeting-token-extra` should NOT match)
   - Status: **BLOCKING** - Prevents scope bypass attacks

### P1 - Important Functionality

3. **TTL capping behavior test**
   - Test that TTL > 900 is capped to 900
   - Test that TTL <= 900 is preserved
   - Pattern: Unit test for the `.min(MAX_TOKEN_TTL_SECONDS)` logic
   - Status: Should add

4. **Handler integration tests (require database)**
   - Test `handle_meeting_token` success path (valid token, valid scope, issues JWT)
   - Test `handle_guest_token` success path
   - Test response contains valid JWT structure
   - Test `expires_in` matches requested/capped TTL
   - Pattern: Use `TestAuthServer` harness with `create_service_token(&["internal:meeting-token"])`
   - Status: Should add

5. **JWT claims verification**
   - Verify meeting token contains expected claims: `sub`, `token_type`, `meeting_id`, etc.
   - Verify guest token contains expected claims including `waiting_room`
   - Verify `jti` is unique per token (not hardcoded)
   - Status: Should add

### P2 - Nice to Have

6. **Error response format tests**
   - Verify INSUFFICIENT_SCOPE error includes `required_scope` and `provided_scopes`
   - Verify error matches API contract
   - Status: Can defer

7. **Metrics recording tests**
   - Verify `record_token_issuance` is called with correct grant_type and status
   - Status: Can defer (observability tests are lower priority)

---

## Recommendations

### Must Fix Before Merge

1. **Add integration tests for scope validation** - Create a new test file `tests/integration/internal_token_tests.rs` with:
   ```rust
   #[sqlx::test(migrations = "../../migrations")]
   async fn test_meeting_token_requires_internal_scope(pool: PgPool)

   #[sqlx::test(migrations = "../../migrations")]
   async fn test_guest_token_requires_internal_scope(pool: PgPool)

   #[sqlx::test(migrations = "../../migrations")]
   async fn test_meeting_token_rejects_wrong_scope(pool: PgPool)
   ```

2. **Add TTL capping unit test** - Simple test that validates the `.min()` logic without database.

### Should Add Soon

3. **Happy path integration tests** - Exercise full flow with valid credentials and scope.

4. **JWT structure validation** - Decode returned JWT and verify claims match request.

### Implementation Notes

The test harness (`TestAuthServer`) already supports:
- `create_service_token(client_id, &scopes)` - Perfect for creating tokens with `internal:meeting-token`
- Token issuance via real routes
- Database isolation per test

Pattern for internal token tests (reference `admin_auth_tests.rs`):
```rust
use ac_test_utils::server_harness::TestAuthServer;

#[sqlx::test(migrations = "../../migrations")]
async fn test_meeting_token_requires_internal_scope(pool: PgPool) -> Result<(), anyhow::Error> {
    let server = TestAuthServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    // Create token WITHOUT internal:meeting-token scope
    let token = server.create_service_token("test-gc", &["meeting:create"]).await?;

    let response = client
        .post(format!("{}/api/v1/auth/internal/meeting-token", server.url()))
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "subject_user_id": "550e8400-e29b-41d4-a716-446655440001",
            "meeting_id": "550e8400-e29b-41d4-a716-446655440002",
            "meeting_org_id": "550e8400-e29b-41d4-a716-446655440003",
            "home_org_id": "550e8400-e29b-41d4-a716-446655440004"
        }))
        .send()
        .await?;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    // ... verify error response
    Ok(())
}
```

---

## Summary

| Category | Status |
|----------|--------|
| Type serialization tests | Pass (comprehensive) |
| Enum tests | Pass (all variants covered) |
| Default value tests | Pass |
| Handler integration tests | **FAIL** (missing) |
| Scope validation tests | **FAIL** (P0 security - missing) |
| TTL capping tests | **FAIL** (missing) |

**Blocking issues**: 2 (scope validation tests)
**Non-blocking gaps**: 5

The unit tests for types and enums are well-written. However, the handlers contain critical security logic (scope validation) that is **not tested**. The pattern already exists in `admin_auth_tests.rs` and should be replicated for these internal endpoints.

---

## Specialist Sign-off

- [ ] P0 scope validation tests added
- [ ] P1 handler integration tests added
- [ ] P1 TTL capping test added

**Status**: FINDINGS - requires P0 tests before approval
