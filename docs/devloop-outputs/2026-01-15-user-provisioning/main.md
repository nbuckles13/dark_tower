# Dev-Loop Output: User Provisioning and Login

**Date**: 2026-01-15
**Task**: Implement user self-registration and login in AC per ADR-0020
**Branch**: `feature/gc-phases-1-3`
**Duration**: ~45 minutes

---

## Loop State (Internal)

<!-- This section is maintained by the orchestrator for state recovery after context compression. -->
<!-- Do not edit manually - the orchestrator updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Implementing Agent | `a6b1956` |
| Implementing Specialist | `auth-controller` |
| Current Step | `complete` |
| Iteration | `2` |
| Security Reviewer | `a9825fd` |
| Test Reviewer | `af2a66c` (BLOCKER resolved) |
| Code Reviewer | `a4bc84b` |
| DRY Reviewer | `a354681` |
| Reflection Complete | `2026-01-15` |

---

## Task Overview

### Objective

Implement user self-registration and login in AC, following ADR-0020's token architecture:
- `POST /api/v1/auth/register` - User self-registration with auto-login
- `POST /api/v1/auth/user/token` - User login (issue User Token)
- Subdomain-based organization extraction middleware
- User Token claims per ADR-0020 (sub, org_id, email, roles, exp, iat, jti)

### Scope

- **Service(s)**: AC (Authentication Controller)
- **Schema**: Yes - add user_roles junction table
- **Cross-cutting**: No - AC-only implementation

### Design Reference

- ADR-0020: User Authentication and Meeting Access Flows
- Plan file: `/home/nathan/.claude/plans/flickering-finding-hartmanis.md`

---

## Pre-Work

- Users table exists with password_hash field
- Organizations table exists with subdomain field
- Bcrypt hashing implemented (cost 12)
- EdDSA JWT signing working for service tokens
- `issue_user_token` placeholder exists in token_service.rs

---

## Implementation Summary

### 1. Database Migration

**File**: `migrations/20260116000001_add_user_roles.sql`

Created junction table for user roles:
- `user_roles` table with `user_id`, `role`, `created_at`
- Composite primary key `(user_id, role)`
- CHECK constraint for valid roles: `user`, `admin`, `org_admin`
- Index on `user_id` for efficient role lookups

### 2. Crypto Module Updates

**File**: `crates/ac-service/src/crypto/mod.rs`

Added user token support:
- `UserClaims` struct per ADR-0020 with `sub`, `org_id`, `email`, `roles`, `iat`, `exp`, `jti`
- Custom `Debug` implementation that redacts sensitive fields (`sub`, `email`, `jti`)
- `sign_user_jwt()` function for signing user tokens with EdDSA
- `verify_user_jwt()` function for validating user tokens (for use by GC/MC)

### 3. Repository Layer

**Files**:
- `crates/ac-service/src/repositories/users.rs` (new)
- `crates/ac-service/src/repositories/organizations.rs` (new)

Users repository:
- `get_by_email(pool, org_id, email)` - Fetch user by email within org
- `get_by_id(pool, user_id)` - Fetch user by ID
- `create_user(pool, org_id, email, password_hash, display_name)` - Create new user
- `update_last_login(pool, user_id)` - Update last login timestamp
- `get_user_roles(pool, user_id)` - Get all roles for a user
- `add_user_role(pool, user_id, role)` - Add role with validation
- `remove_user_role(pool, user_id, role)` - Remove role
- `email_exists_in_org(pool, org_id, email)` - Check email uniqueness

Organizations repository:
- `get_by_subdomain(pool, subdomain)` - Lookup org by subdomain
- `get_by_id(pool, org_id)` - Lookup org by ID

### 4. Organization Extraction Middleware

**File**: `crates/ac-service/src/middleware/org_extraction.rs` (new)

Subdomain-based organization identification:
- Extracts subdomain from HTTP `Host` header
- Supports formats: `acme.darktower.com`, `acme.localhost:3000`
- Validates subdomain format (lowercase alphanumeric and hyphens)
- Looks up organization in database
- Injects `OrgContext { org_id, subdomain }` into request extensions
- Returns 400 for invalid/missing subdomain, 404 for unknown org

### 5. Token Service Updates

**File**: `crates/ac-service/src/services/token_service.rs`

Replaced placeholder with full implementation:
- `issue_user_token()` implements full flow:
  - Rate limiting by user_id (15-minute window, 5 attempts max)
  - Constant-time bcrypt verification (dummy hash for non-existent users)
  - Check `is_active` status
  - Fetch user roles
  - Generate JWT with `UserClaims` (1hr lifetime, unique `jti`)
  - Log auth events
  - Update `last_login_at`
- Added `UserTokenResponse` struct for response type
- Added `get_failed_attempts_count_by_user()` to auth_events repository

### 6. User Registration Service

**File**: `crates/ac-service/src/services/user_service.rs` (new)

User registration flow:
- Rate limiting by IP (5 registrations per hour)
- Email format validation
- Password validation (min 8 chars)
- Email uniqueness check within org
- Password hashing (bcrypt cost 12)
- User creation with default "user" role
- Auto-login (token issuance)
- Event logging

### 7. Handlers

**File**: `crates/ac-service/src/handlers/auth_handler.rs`

Updated handlers:
- `UserTokenRequest` now uses `email` (not `username`) per ADR-0020
- `handle_user_token()` accepts `OrgContext` from middleware
- Added `handle_register()` for `POST /api/v1/auth/register`
- Both handlers extract IP and User-Agent for logging/rate limiting

### 8. Routes

**File**: `crates/ac-service/src/routes/mod.rs`

Added user auth routes:
- Created `user_auth_routes` router with org extraction middleware
- `POST /api/v1/auth/user/token` - User login
- `POST /api/v1/auth/register` - User registration
- Merged into main router

---

## Files Created

- `migrations/20260116000001_add_user_roles.sql`
- `crates/ac-service/src/repositories/users.rs`
- `crates/ac-service/src/repositories/organizations.rs`
- `crates/ac-service/src/middleware/org_extraction.rs`
- `crates/ac-service/src/services/user_service.rs`
- `docs/dev-loop-outputs/2026-01-15-user-provisioning/auth-controller.md`

## Files Modified

- `crates/ac-service/src/crypto/mod.rs` - Added UserClaims, sign_user_jwt, verify_user_jwt
- `crates/ac-service/src/services/token_service.rs` - Replaced placeholder with full implementation
- `crates/ac-service/src/handlers/auth_handler.rs` - Updated handlers, added handle_register
- `crates/ac-service/src/routes/mod.rs` - Added user auth routes with middleware
- `crates/ac-service/src/repositories/mod.rs` - Added users and organizations modules
- `crates/ac-service/src/services/mod.rs` - Added user_service module
- `crates/ac-service/src/middleware/mod.rs` - Added org_extraction module
- `crates/ac-service/src/repositories/auth_events.rs` - Added get_failed_attempts_count_by_user

---

## Dev-Loop Verification Steps

| Step | Status | Output |
|------|--------|--------|
| `cargo check --workspace` | PASSED | No errors |
| `cargo fmt --all --check` | PASSED | Code formatted |
| `./scripts/guards/run-guards.sh` | PASSED | 6/6 guards passed |
| `cargo test -p ac-service --lib` (no DB) | PASSED | 166/166 unit tests passed (skipped DB-dependent) |
| `cargo test -p ac-service` | SKIPPED | DATABASE_URL not set |
| `cargo clippy --workspace --lib --bins -- -D warnings` | PASSED | No warnings |
| Semantic guards | N/A | No semantic guard modifications |

### Iteration 2: Tests Added (2026-01-15)

Following Test Reviewer BLOCKER, added comprehensive tests:

| Step | Status | Output |
|------|--------|--------|
| `cargo test -p ac-service --lib` (with DB) | PASSED | 368/368 tests passed |

#### Tests Added

**crypto/mod.rs - Unit tests for UserClaims and user JWT functions:**
- `test_sign_user_jwt_valid_jwt` - Verifies sign_user_jwt produces valid JWT with correct claims
- `test_sign_user_jwt_includes_kid_header` - Verifies kid is in header
- `test_verify_user_jwt_validates_signature` - Verifies signature validation
- `test_verify_user_jwt_rejects_expired` - Verifies expired token rejection
- `test_verify_user_jwt_rejects_future_iat` - Verifies iat validation (beyond clock skew)
- `test_verify_user_jwt_accepts_iat_within_skew` - Verifies iat within tolerance
- `test_verify_user_jwt_rejects_oversized` - Verifies DoS protection
- `test_user_claims_debug_redacts_sensitive` - Verifies Debug redacts sub, email, jti
- `test_user_claims_serde_roundtrip` - Verifies serialization/deserialization
- `test_user_claims_clone` - Verifies Clone implementation
- `test_sign_user_jwt_invalid_private_key` - Verifies invalid key handling
- `test_verify_user_jwt_malformed_token` - Verifies malformed token rejection

**token_service.rs - Integration tests for issue_user_token():**
- `test_issue_user_token_happy_path` - Valid credentials return JWT with correct UserClaims
- `test_issue_user_token_no_enumeration` - Invalid password returns same error as non-existent user
- `test_issue_user_token_inactive_user_rejected` - Inactive user is rejected
- `test_issue_user_token_lockout_after_5_failures` - Account lockout after 5 failed attempts
- `test_issue_user_token_updates_last_login` - Successful login updates last_login_at
- `test_issue_user_token_includes_all_roles` - Multi-role user includes all roles in token
- `test_issue_user_token_timing_attack_prevention` - Dummy hash timing protection

**user_service.rs - Integration tests for register_user():**
- `test_register_user_happy_path` - Successful registration with auto-login
- `test_register_user_rate_limiting` - Rate limiting (5 registrations per IP per hour)
- `test_register_user_invalid_email_rejected` - Invalid email format rejected
- `test_register_user_password_too_short` - Password < 8 chars rejected
- `test_register_user_duplicate_email_rejected` - Duplicate email in org rejected
- `test_register_user_empty_display_name_rejected` - Empty display name rejected
- `test_register_user_minimum_password_length` - Exactly 8 chars accepted
- `test_register_user_same_email_different_orgs` - Same email in different orgs allowed
- `test_register_user_without_ip_address` - Registration without IP (no rate limiting)

---

## Issues Encountered & Resolutions

### 1. UserTokenRequest field rename

**Issue**: Existing tests used `username` field, but ADR-0020 specifies `email`.

**Resolution**: Updated `UserTokenRequest` to use `email` field, updated all related tests.

### 2. Missing auth_events function

**Issue**: Rate limiting for user login needed per-user attempt counting.

**Resolution**: Added `get_failed_attempts_count_by_user()` to auth_events repository.

### 3. Clippy indexing lint

**Issue**: Direct array indexing (`parts[0]`) triggered `clippy::indexing_slicing`.

**Resolution**: Used `.first()` and `.get()` methods with proper error handling.

### 4. Unfulfilled lint expectation

**Issue**: `#[expect(clippy::too_many_arguments)]` on function with 7 arguments (limit is 7).

**Resolution**: Removed the expectation since the function has exactly 7 arguments.

### 5. Test database not running (dev-loop flow validation)

**Issue**: During orchestrator validation, 130 integration tests failed because `DATABASE_URL` wasn't set - the test database wasn't running.

**Dev-loop behavior**: The validation step correctly detected the failure and reported it to the user rather than proceeding to code review. This is the expected flow:
- Validation step runs all 7 verification layers
- Layer 5 (`cargo test --workspace`) failed due to missing database
- Orchestrator identified this as an environmental issue (not a code bug)
- Reported to user for resolution

**Resolution**: User instructed to start database with `podman-compose -f docker-compose.test.yml up -d`. After database was running and migrations applied, the dev-loop resumed successfully with 368 tests passing.

**Validation**: This confirms the dev-loop workflow correctly handles environmental failures by escalating to the user rather than silently working around them.

---

## Lessons Learned

1. **UserTokenRequest vs email**: ADR-0020 specifies email-based authentication, not username. Always check ADR for field naming.

2. **Rate limiting needs dedicated tracking**: User rate limiting required a new query function; couldn't reuse credential-based rate limiting directly.

3. **Subdomain extraction edge cases**: IP addresses, ports, and single-part hostnames need careful handling.

---

## Tech Debt

1. **Registration rate limiting simplification**: Currently uses successful login events as proxy for registration count. Could add dedicated registration event type.

2. **Email validation**: Basic format check only. Could enhance with DNS/MX verification for production.

3. **Dead code annotations**: Some library functions marked `#[allow(dead_code)]` for future use by GC/MC.

---

## Security Notes

1. **Timing Attack Prevention**: Dummy bcrypt hash (matching cost factor 12) used for non-existent users to ensure consistent response times.

2. **Rate Limiting**:
   - User login: 5 failed attempts per 15-minute window
   - Registration: 5 registrations per IP per hour

3. **Password Requirements**: Minimum 8 characters (could be enhanced).

4. **Sensitive Data Protection**:
   - Passwords wrapped in `SecretString`
   - `UserClaims` Debug redacts `sub`, `email`, `jti`

5. **JWT Security**:
   - EdDSA (Ed25519) signing
   - 1-hour expiry
   - Unique `jti` for revocation support
   - Size check before parsing (DoS prevention)

---

## Reflection Phase

All specialists completed reflection and updated their knowledge files.

### Knowledge Files Updated

| Specialist | Patterns | Gotchas | Integration |
|------------|----------|---------|-------------|
| auth-controller | +4 | +4 | +3 |
| security | +3 | +3 | +2 |
| test | +2 | +3 | +2 |
| code-reviewer | +4 | +6 | +2 |
| dry-reviewer | Bootstrapped | Bootstrapped | Bootstrapped |

### Key Learnings Captured

**Auth Controller**:
- UserClaims with custom Debug for PII redaction
- Subdomain-based org extraction middleware
- Auto-login on registration pattern

**Security**:
- Timing-safe dummy hash must match production cost
- Custom Debug implementations for sensitive types

**Test**:
- BLOCKER enforcement for missing integration tests
- User provisioning test coverage template

**Code Reviewer**:
- Repository organization (one file per entity)
- Middleware context injection pattern

**DRY Reviewer**:
- Bootstrapped knowledge files with ADR-0019 workflow
- Tech debt registry (TD-1, TD-2)

---

## Dev-Loop Complete

This dev-loop is complete. All verification passed, code review approved, and reflection captured.
