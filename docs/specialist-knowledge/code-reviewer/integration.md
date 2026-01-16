# Code Reviewer - Integration Notes

Notes on working with other specialists in the Dark Tower project.

---

## Integration: Security Specialist Handoff
**Added**: 2026-01-11
**Related files**: `.claude/agents/security.md`

When reviewing security-critical code (crypto, auth, validation), flag findings as MAJOR or CRITICAL for security specialist review. Defense-in-depth recommendations should be explicitly requested if not already implemented. Security specialist should verify cryptographic parameter choices match OWASP/NIST guidance.

---

## Integration: Test Specialist Collaboration
**Added**: 2026-01-11
**Related files**: `.claude/agents/test.md`

After code review, coordinate with test specialist to ensure: boundary conditions have tests, error paths are exercised, security-critical paths have P0 priority tests. For config changes, verify both valid and invalid input tests exist.

---

## Integration: Auth Controller Specialist Context
**Added**: 2026-01-11
**Related files**: `crates/ac-service/`

Auth Controller has established patterns for config, crypto, and validation. When reviewing AC changes, verify pattern consistency with existing code. Key files to check: `config.rs` for configuration patterns, `crypto.rs` for cryptographic operations, `error.rs` for error handling patterns.

---

## Integration: Pre-Review Checklist
**Added**: 2026-01-11
**Related files**: `.claude/workflows/code-review.md`

Before starting review, verify: (1) no unwrap/expect/panic in production paths, (2) sqlx used for all database queries, (3) Result<T,E> used for fallible operations, (4) documentation includes security references where applicable, (5) tests cover boundary conditions.

---

## Integration: ADR Compliance Check
**Added**: 2026-01-11
**Related files**: `docs/decisions/`

Cross-reference code changes against existing ADRs. Key ADRs: ADR-0002 (no-panic policy), ADR-0003 (error handling). Flag violations as MAJOR findings requiring remediation before approval.

---

## Integration: Global Controller Service Foundation
**Added**: 2026-01-14
**Related files**: `crates/global-controller/`

GC Phase 1 establishes the foundation for HTTP/3 API gateway. Key patterns for future reviewers:
1. Config loads from environment with sensible defaults (`from_vars()` for testing)
2. AppState holds shared resources (Arc<PgPool>, Config) - must all implement Clone
3. Handlers use State extractor, delegate to services/repositories (not yet implemented)
4. Error handling maps to HTTP status codes via impl From<GcError> for StatusCode
5. Health checks always return 200 with status field - never error on probe failure
6. Test harness spawns real server instance with JoinHandle for cleanup

When reviewing future GC features (meeting APIs, rate limiting, etc.), ensure they follow these established patterns.

---

## Integration: Test Harness Patterns
**Added**: 2026-01-14
**Related files**: `crates/gc-test-utils/src/server_harness.rs`

The GC test harness is reusable for all GC integration tests. Future test specs should:
1. Import TestGcServer from gc-test-utils
2. Use `#[sqlx::test(migrations = "../../migrations")]` to get a real database
3. Call `TestGcServer::spawn(pool).await?` to get a running server
4. Use `server.url()` for HTTP requests
5. Use `server.pool()` for database queries
6. Don't worry about cleanup - Drop impl handles it

This pattern is similar to ac-test-utils' `TestAcServer` if it exists, or establishes the pattern for future services.

---

## Integration: User Provisioning Foundation in AC Service
**Added**: 2026-01-15
**Related files**: `crates/ac-service/src/models/users.rs`, `crates/ac-service/src/service/user_service.rs`, `crates/ac-service/src/handlers/user_handler.rs`

User provisioning establishes these patterns for AC service:
1. **Models**: UserClaims (private, for token validation), UserResponse (public, for API)
2. **Service layer**: `UserService` wraps repository with domain logic, returns domain errors
3. **Middleware**: `OrgContext` extracts organization ID from tokens, handlers use via Extension
4. **Organization extraction**: Done in middleware, not handlers - centralizes auth logic
5. **Error mapping**: Service errors map to HTTP status codes via IntoResponse trait

When reviewing future user-related features (permissions, profile updates, etc.), ensure they follow these established patterns. User service is the canonical example of layered architecture for this project.

---

## Integration: Service Layer Pattern for Repositories
**Added**: 2026-01-15
**Related files**: `crates/ac-service/src/service/user_service.rs`, `crates/ac-service/src/repository/users.rs`

The service layer pattern used in user provisioning is the template for all future AC service methods. Service functions should:
1. Take repository functions as parameters or use dependency injection (not hardcoded)
2. Wrap repository errors in domain-specific error types
3. Add business logic (validation, filtering, transformation) between repository and handler
4. Be unit-testable without database (mock repository layer)
5. Implement `IntoResponse` on error type to map to HTTP status codes

This pattern separates data access (repository) from business logic (service) from HTTP handling (handlers).
