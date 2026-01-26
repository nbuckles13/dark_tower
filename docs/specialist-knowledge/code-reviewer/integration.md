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

Auth Controller has established patterns for config, crypto, and validation. When reviewing AC changes, verify pattern consistency with existing code. Key files to check: `config.rs` for configuration patterns, `crypto/mod.rs` for cryptographic operations, `errors.rs` for error handling patterns.

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
3. Handlers use State extractor, delegate to services/repositories
4. Error handling maps to HTTP status codes via impl From<GcError> for StatusCode
5. Health checks always return 200 with status field - never error on probe failure
6. Test harness spawns real server instance with JoinHandle for cleanup

When reviewing future GC features (meeting APIs, rate limiting, etc.), ensure they follow these established patterns.

---

## Integration: Test Harness Patterns
**Added**: 2026-01-14
**Related files**: `crates/gc-test-utils/src/server_harness.rs`, `crates/ac-test-utils/src/server_harness.rs`

The GC and AC test harnesses are reusable for all integration tests. Future test specs should:
1. Import the appropriate TestServer from *-test-utils
2. Use `#[sqlx::test(migrations = "../../migrations")]` to get a real database
3. Call `TestServer::spawn(pool).await?` to get a running server
4. Use `server.url()` for HTTP requests
5. Use `server.pool()` for database queries
6. Don't worry about cleanup - Drop impl handles it

---

## Integration: Test Specialist Coordination for DRY Findings
**Added**: 2026-01-15
**Related files**: `crates/ac-service/tests/integration/`

When code review identifies DRY violations in test code (duplicated JWT decoding, repeated assertion patterns), coordinate with Test specialist to:
1. Create shared helper functions in the test-utils crate
2. Update existing tests to use new helpers
3. Document helpers in test harness module docs
4. Ensure helpers return `Result` for ADR-0002 compliance

Flag as tech debt if not immediately addressable per ADR-0019.

---

## Integration: Meeting Controller Service Foundation
**Added**: 2026-01-25
**Related files**: `crates/meeting-controller/`, `crates/mc-test-utils/`

MC Phase 6a establishes the foundation for WebTransport signaling. Key patterns for future reviewers:
1. Config uses builder pattern with `#[must_use]` for fluent test configuration
2. Custom Debug implementations redact sensitive fields (WebTransport secrets, session tokens)
3. Error types follow ADR-0003 with From implementations for clean conversions
4. ADR-0023 references appear in doc comments for traceability
5. mc-test-utils provides MockRedis for session state testing (note: uses std::sync::Mutex - tech debt)

When reviewing future MC features (session management, participant coordination), ensure they follow these established patterns and reference ADR-0023 sections.
