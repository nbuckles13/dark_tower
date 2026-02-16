# Auth Controller Specialist Checkpoint

**Date**: 2026-01-15
**Task**: User Provisioning and Login Implementation
**Phase**: Development Loop - Specialist Implementation

---

## Loop State

```yaml
task: User provisioning and login in AC
phase: tests_added
iteration: 2
status: blocker_resolved
specialist: auth-controller
last_action: Added missing tests per Test Reviewer BLOCKER
```

---

## Implementation Summary

Implemented full user authentication flow per ADR-0020:
- Database migration for user_roles
- UserClaims and JWT signing for user tokens
- User and Organization repositories
- Organization extraction middleware (subdomain-based)
- Token issuance with rate limiting and timing attack prevention
- User registration service with validation
- HTTP handlers for login and registration
- Routes with middleware integration

---

## Verification Status

| Check | Result |
|-------|--------|
| `cargo check` | PASSED |
| `cargo fmt` | PASSED |
| Guards | PASSED (6/6) |
| Unit tests | PASSED (201/201) |
| Integration tests | SKIPPED (no database) |
| `cargo clippy` | PASSED |

### Iteration 2: Tests Added (BLOCKER Resolution)

| Check | Result |
|-------|--------|
| `cargo test -p ac-service --lib` (with DB) | PASSED (368/368) |

Added tests for:
- crypto/mod.rs: 12 unit tests for UserClaims and user JWT functions
- token_service.rs: 7 integration tests for issue_user_token()
- user_service.rs: 9 integration tests for register_user()

---

## Patterns Applied

### From Accumulated Knowledge

1. **Config Testability**: Crypto functions accept config parameters
2. **JWT Claims Extension**: Dedicated `UserClaims` struct for user tokens
3. **Scope Validation at Handler Level**: OrgContext injected via middleware
4. **Middleware Pattern**: `Extension<OrgContext>` for claims injection
5. **Timing Attack Prevention**: Dummy hash with matching cost factor
6. **TTL Capping**: 1-hour token lifetime

### Security Patterns

1. **Constant-time comparison**: Bcrypt verification always runs
2. **Error message consistency**: Generic messages for auth failures
3. **JWT size check before parsing**: Prevents DoS
4. **Rate limiting per-user**: 5 attempts / 15 minutes

---

## Gotchas Encountered

1. **UserTokenRequest.username renamed to email**: Per ADR-0020, users authenticate with email not username. Updated existing tests.

2. **Missing auth_events function**: Added `get_failed_attempts_count_by_user()` for user rate limiting.

3. **Clippy indexing lint**: Used `.first()` and `.get()` instead of direct indexing.

---

## Files Modified

```
migrations/20260116000001_add_user_roles.sql (new)
crates/ac-service/src/crypto/mod.rs
crates/ac-service/src/repositories/mod.rs
crates/ac-service/src/repositories/users.rs (new)
crates/ac-service/src/repositories/organizations.rs (new)
crates/ac-service/src/repositories/auth_events.rs
crates/ac-service/src/middleware/mod.rs
crates/ac-service/src/middleware/org_extraction.rs (new)
crates/ac-service/src/services/mod.rs
crates/ac-service/src/services/token_service.rs
crates/ac-service/src/services/user_service.rs (new)
crates/ac-service/src/handlers/auth_handler.rs
crates/ac-service/src/routes/mod.rs
```

---

## Dependencies Added

None - all functionality uses existing dependencies (sqlx, axum, bcrypt, ring, etc.)

---

## Ready for Review

- [x] Security specialist review (authentication flow, rate limiting) - PASSED
- [x] Test specialist review (integration test coverage) - BLOCKER RESOLVED (tests added)
- [x] Code quality review (patterns, error handling) - PASSED
- [x] DRY reviewer - No cross-service duplication found

---

## Recovery Information

If context is lost, resume from:
1. Read this checkpoint file
2. Check `docs/dev-loop-outputs/2026-01-15-user-provisioning/main.md` for full details
3. Run `DATABASE_URL=postgresql://postgres:postgres@localhost:5433/dark_tower_test cargo test -p ac-service --lib` to verify state
4. All reviews complete - ready for merge or next task

---

## Reflection Summary (2026-01-15)

### What Went Well

The user provisioning implementation benefited significantly from accumulated specialist knowledge. Patterns like custom Debug redaction for sensitive fields, middleware-based context injection, and repository separation were applied directly from prior work. The ADR-0020 specification provided clear guidance on claim structure, which prevented design ambiguity.

### Key Learnings Captured

**Patterns added**:
- UserClaims with custom Debug for PII redaction
- Subdomain-based organization extraction middleware
- Repository functions for domain entity lookups
- Auto-login on registration pattern

**Gotchas documented**:
- UserTokenRequest uses email, not username (ADR-0020 compliance)
- User rate limiting requires separate database function (can't reuse credential-based)
- Subdomain extraction edge cases (IPs, ports, single-part hostnames)
- Clippy indexing lint - use `.first()` and `.get()` instead of direct indexing

**Integration notes added**:
- User Token claims structure per ADR-0020
- Subdomain requirement for user endpoints
- verify_user_jwt() availability for GC/MC token validation

### Iteration Lesson

The Test Reviewer BLOCKER for missing tests caught a coverage gap early. Adding 28 tests (12 unit + 7 token service + 9 user service) after initial implementation was more work than writing them alongside the implementation. Future implementations should prioritize test coverage during initial implementation to avoid iteration overhead.

### Knowledge File Locations

- `/home/nathan/code/dark_tower/docs/specialist-knowledge/auth-controller/patterns.md`
- `/home/nathan/code/dark_tower/docs/specialist-knowledge/auth-controller/gotchas.md`
- `/home/nathan/code/dark_tower/docs/specialist-knowledge/auth-controller/integration.md`
