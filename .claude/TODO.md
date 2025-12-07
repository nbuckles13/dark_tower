# Technical Debt and Future Work

## Phase 4: P1 Security Test Improvements

### Completed Items (2025-12-01)

#### P2 Security Tests (2025-12-01)

- [x] **JWT Size Limits (DoS Prevention)**: Added `MAX_JWT_SIZE_BYTES = 4096` constant, size check in `verify_jwt()` BEFORE parsing/crypto operations
- [x] **test_jwt_size_limit_enforcement**: Unit test verifying oversized tokens rejected
- [x] **test_jwt_size_limit_allows_normal_tokens**: Regression test for normal tokens
- [x] **test_jwt_oversized_token_rejected**: Integration test with 10KB payload
- [x] **Time-Based SQL Injection Prevention**: Tests pg_sleep() injection attempts with timing validation
- [x] **test_time_based_sql_injection_prevented**: Multiple attack vectors, measures execution time
- [x] **Test Specialist Review**: ACCEPTABLE
- [x] **Security Specialist Review**: ACCEPTABLE

**Test Count**: Increased from 80 → 84 tests (+4 new P2 security tests)

#### JWT Header Injection Tests (2025-12-01)

- [x] **test_jwt_header_typ_tampering**: Tests various `typ` header values (at+jwt, jwt, CUSTOM, null) - documents typ is not security-critical per RFC 7519
- [x] **test_jwt_header_alg_mismatch_rejected**: Tests algorithm confusion attack defense (CVE-2015-2951, CVE-2016-5431) - EdDSA→HS256/RS256 rejected
- [x] **test_jwt_header_kid_injection**: Tests key ID injection attack defense - verifies kid header cannot redirect to attacker-controlled keys
- [x] **Test Specialist Review**: WELL TESTED - Excellent security coverage, outstanding documentation
- [x] **Security Specialist Review**: ACCEPTABLE - Recommends adding JWT size limits and preparing for future key rotation security

**Test Count**: Increased from 77 → 80 tests (+3 new header injection tests)

#### JWT iat Validation (2025-12-01)

- [x] **iat Validation Implementation**: Implemented strict iat validation with ±5 minute clock skew tolerance in `crypto::verify_jwt()`. Tokens with future `iat` beyond tolerance are rejected (prevents token pre-generation attacks)
- [x] **JWT_CLOCK_SKEW_SECONDS Constant**: Added 300-second (5 minute) constant per NIST SP 800-63B
- [x] **iat Unit Tests**: Added 3 tests in crypto module (rejects future, accepts within skew, constant value)
- [x] **iat Integration Tests**: Added 4 tests in token_service (boundary tests at exact 5-min mark)
- [x] **Test Specialist Review**: WELL TESTED - Comprehensive coverage, excellent documentation
- [x] **Security Specialist Review**: ACCEPTABLE - Secure implementation, defense-in-depth recommendation noted

**Test Count**: Increased from 71 → 77 tests (+6 new iat validation tests)

#### Previous Improvements (2025-11-30)

- [x] **Error Information Leakage Prevention**: Added 2 tests preventing sensitive data exposure (OWASP A05:2021, CWE-209)
- [x] **Key Rotation Tests Planning**: Documented required tests and missing repository methods (`get_by_key_id()`, `list_all_keys()`)
- [x] **Timing Attack Tolerance**: Tightened from 50% to 30% to reduce attack surface
- [x] **UNION SELECT SQL Injection**: Added test with 3 attack vectors including information_schema exploitation
- [x] **Second-Order SQL Injection**: Added test verifying malicious stored data cannot execute in subsequent queries
- [x] **bcrypt Cost Factor Validation**: Added 2 tests verifying cost=12 per ADR-0003 (CWE-916 mitigation)
- [x] **Test Naming Standardization**: Fixed null byte test name and verified consistent naming across all tests
- [x] **Magic Number Extraction**: Extracted 5 constants (token expiry, rate limits, timing thresholds) to improve maintainability

**Code Coverage**: Maintained 83% (targeting 95%)

### Future Enhancements

#### JWT Security Enhancements

- [x] ~~**iat Validation Implementation**: Implement strict iat validation with clock skew tolerance (±5 minutes)~~ ✅ DONE
- [x] ~~**JWT Header Injection**: Add tests for typ/alg/kid header tampering~~ ✅ DONE
- [x] ~~**JWT Size Limits**: Added `MAX_JWT_SIZE_BYTES = 4096` (4KB) DoS prevention~~ ✅ DONE
- [x] ~~**Maximum Token Age Validation** (DEBATED)~~: Debate concluded 2025-12-01 with consensus on context-specific strategy. See [ADR-0007](../docs/decisions/adr-0007-token-lifetime-strategy.md). **Decision**: Service tokens keep 1-hour lifetime (no max age), user tokens (Phase 8) will use 15-min access + refresh pattern.
- [x] ~~**Key Rotation Implementation**: Complete key rotation endpoint and tests~~ ✅ DONE (2025-12-04)
  - Implemented `/internal/rotate-keys` endpoint (ADR-0008)
  - Added 10 integration tests for key rotation
  - Fixed TOCTOU race condition with PostgreSQL advisory lock
  - Added `signing_keys::get_by_key_id()` repository method
  - Service-only token validation (user tokens rejected)
  - Rate limiting: 6 days (normal), 1 hour (force)
  - See: ADR-0008, ADR-0009 for design decisions
- [ ] **Configurable Clock Skew**: Consider making `JWT_CLOCK_SKEW_SECONDS` configurable via environment variable for different security postures

#### SQL Injection Testing Enhancements

- [ ] **Deterministic Oversized Input Test**: Refactor `test_oversized_input_handling` to check against actual schema limits (VARCHAR lengths, etc.) instead of arbitrary 1000-char strings
- [x] ~~**Time-Based SQL Injection**: Added `test_time_based_sql_injection_prevented` with pg_sleep() timing validation~~ ✅ DONE
- [ ] **Stored Procedure Injection**: If using stored procedures in future, add tests for SQL injection via procedure parameters

### Test Infrastructure

- [ ] **Code Coverage Target**: Improve P1 test coverage from current 83% to target 95%
- [ ] **Performance Benchmarks**: Add criterion benchmarks for token validation performance under security attacks

### Documentation

- [ ] **Security Testing Guide**: Document security testing patterns and attack vectors in developer documentation
- [ ] **Threat Model**: Create formal threat model documentation for authentication controller

## Phase 5: Global Controller Implementation

### Architecture (from ADR-0010)

- [x] ~~**Inter-Region MC Discovery**~~: ✅ RESOLVED (2025-12-05)
  Design completed via multi-agent debate. See [ADR-0010 Section 5](../docs/decisions/adr-0010-global-controller-architecture.md).

  **Final Design - Direct MC-to-Bus (83.2% consensus)**:
  - MC subscribes directly to Redis Streams (no GC subscription tracking)
  - Transactional outbox pattern for atomic DB write + bus publish
  - Regional DB isolation (no cross-region PostgreSQL queries)
  - Blind cross-region broadcast via GC-to-GC gRPC
  - Remote GC blindly writes to local Redis (handles late-joiner race conditions)
  - Redis ACLs enforce read/write separation (GC write-only, MC read-only)
  - mTLS for all inter-service communication

- [ ] **MC Failure and Migration**: Design graceful handling when MC becomes unhealthy
  - How to detect MC failure (heartbeat timeout)
  - How to migrate active meetings to healthy MC
  - Participant reconnection flow
  - State transfer or reconstruction
  - Impact on cross-region MC connections

- [ ] **PostgreSQL Caching for Meeting Joins**: Evaluate caching strategy
  - Every meeting join currently requires at least one Postgres query
  - Consider Redis cache for hot meetings with short TTL
  - Cache invalidation when meeting ends

- [ ] **Meeting Timeout Detection**: Clarify how background job detects no participants
  - MC must report participant count to GC (via heartbeat?)
  - Or: MC directly marks assignment ended when last participant leaves
  - Background job is fallback for orphaned assignments

- [ ] **Asymmetric Cluster Counts**: Support different numbers of clusters per region (deferred)
  - Example: 3 clusters in us-west, 2 in eu-west, 1 in asia
  - Affects: GC peer registry, load balancing, meeting assignment
  - Currently assumes symmetric 1 cluster per region
  - Implement when multi-cluster regions are needed

### Token Revocation (from ADR-0007)

- [ ] **JWT Denylist in Redis**: Implement emergency revocation capability for service tokens
  - Key pattern: `denylist:jwt:{jti}`
  - TTL: Remaining token lifetime at denylist time
  - Check denylist in token validation path

## Phase 8: User Authentication

### Refresh Token Flow (from ADR-0007)

- [ ] **Refresh Token Implementation**: Short-lived access tokens (15 min) + long-lived refresh tokens (24 hours)
- [ ] **Refresh Token Rotation**: Issue new refresh token on each use
- [ ] **Token Family Tracking**: Detect refresh token reuse attacks
- [ ] **Rate Limiting on Refresh**: Prevent abuse of refresh endpoint
- [ ] **Meeting Token Protocol**: Implement `TOKEN_EXPIRING_SOON` and `UPDATE_TOKEN` signaling messages for seamless WebTransport session continuity

## Low Priority

### Clean up dead_code lints (Phase 5+)
Once more of the system is implemented and library functions are actually used by binaries:
- Review all `#[allow(dead_code)]` attributes
- Replace with `#[expect(dead_code)]` where appropriate
- Remove attributes entirely for code that's now in use
- Consider splitting library into smaller modules if dead code patterns persist

**Why deferred**: Currently many library functions are tested but not used by binaries yet. The dead_code lint situation will resolve naturally as we implement Phase 4+ features (admin endpoints, audit endpoints, key rotation, etc).

**Files affected**:
- `crates/ac-service/src/config.rs`
- `crates/ac-service/src/models/mod.rs`
- `crates/ac-service/src/repositories/*.rs`
- `crates/ac-service/src/services/*.rs`
