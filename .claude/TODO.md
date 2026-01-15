# Technical Debt and Future Work

## Recently Completed (January 2026)

### SecretBox/SecretString Refactor ✅
- [x] Wrapped Config master_key/hash_secret with SecretBox<Vec<u8>>
- [x] Wrapped EncryptedKey.encrypted_data with SecretBox<Vec<u8>>
- [x] Changed generate_client_secret() return to SecretString
- [x] Response types client_secret → SecretString
- [x] Custom Debug impls redact all sensitive data as [REDACTED]
- [x] Custom Clone/Serialize for SecretBox-containing structs
- [x] Fixed clock_skew_tests.rs integration (was missing from harness)
- **Commit**: bf65dce

### Guard Pipeline Phase 1 ✅
- [x] Principles framework (ADR-0015)
- [x] Simple guards: no-hardcoded-secrets, no-secrets-in-logs, no-pii-in-logs, no-test-removal, api-version-check, test-coverage
- [x] Semantic guard: credential-leak detection using Claude API
- [x] Guard runner script with unified execution
- [x] 7-layer verification: check → fmt → guards → tests → clippy → semantic
- **Commits**: 80113e3, 9b1b599, d490c80

### Development Loop Workflow ✅ (ADR-0016)
- [x] Specialist-owned verification (runs checks, fixes failures)
- [x] Context injection: principles + specialist knowledge + ADR
- [x] Trust-but-verify orchestrator validation
- [x] Code review integration with resume for fixes
- [x] Reflection step for knowledge capture
- [x] State checkpointing for context compression recovery
- [x] Output file as proof-of-work (docs/dev-loop-outputs/)
- **Commits**: b949052, 14dbf44

### Specialist Knowledge Architecture ✅ (ADR-0017)
- [x] Dynamic knowledge files in docs/specialist-knowledge/{specialist}/
- [x] patterns.md, gotchas.md, integration.md per specialist
- [x] Reflection captures learnings after each implementation
- [x] Knowledge injected into specialist prompts
- **Specialists with knowledge**: auth-controller, security, test, code-reviewer

---

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

### Observability (ADR-0011 Phase 5)

- [ ] **OpenTelemetry Integration**: Add OTel SDK to AC service for trace propagation
  - Currently: Basic `tracing` crate with `#[instrument]` macros
  - Needed: `tracing-opentelemetry` layer, OTLP exporter
  - Ref: ADR-0011 Implementation Plan Phase 5
- [ ] **Trace ID in Logs**: Once OTel is integrated, `test_logs_have_trace_ids` in env-tests should pass
  - Currently: Test is a soft check (warns but doesn't fail)
  - Location: `crates/env-tests/tests/30_observability.rs`
  - ADR-0011 specifies `trace_id`, `span_id`, `request_id` as SAFE fields for logging
- [ ] **Cardinality Validation Test**: Validate metric cardinality matches ADR-0011 bounds
  - e.g., 4 grant_types × 2 statuses = 8 max series for token issuance
- [ ] **Histogram Bucket Alignment Test**: Verify buckets include SLO threshold (350ms for token issuance)

### env-tests Enhancements (from Code Review 2025-12-16)

#### Security Specialist Findings (Priority 1 - Before Production)

- [x] **NetworkPolicy Testing**: Implement `CanaryPod` in `crates/env-tests/src/canary.rs` ✅ DONE (2026-01-13)
  - Implemented: Full `CanaryPod` with deploy, can_reach, cleanup methods
  - Tests: `test_same_namespace_connectivity` (positive), `test_network_policy_blocks_cross_namespace` (negative)
  - Location: `crates/env-tests/src/canary.rs`, `crates/env-tests/tests/40_resilience.rs`
- [x] **Rate Limit Smoke Test**: Add `test_rate_limiting_enabled` ✅ DONE (2026-01-13)
  - Implemented: Checks /metrics endpoint for rate limit metrics, falls back to rapid request testing
  - Location: `crates/env-tests/tests/10_auth_smoke.rs`
- [ ] **TLS/Transport Security Tests**: DEFERRED - Infrastructure concern
  - AC service serves HTTP; TLS termination happens at ingress/service mesh level
  - Port-forwards bypass TLS (localhost HTTP tunnel)
  - Future: Test via ingress URL if `INGRESS_URL` env var set, or document infrastructure-level TLS validation
- [x] **JWT Header Injection (env-level)**: Add `test_jwt_header_injection_attacks` ✅ DONE (2026-01-13)
  - Implemented: `test_kid_injection_rejected`, `test_jwk_header_injection_rejected` (CVE-2018-0114), `test_jku_header_injection_rejected`
  - Location: `crates/env-tests/tests/25_auth_security.rs`
- [x] **Time-Based Claims Validation**: Add `test_iat_validation` ✅ DONE (2026-01-13)
  - Implemented: `test_iat_claim_is_current`, `test_token_lifetime_is_reasonable` (ADR-0007)
  - Location: `crates/env-tests/tests/25_auth_security.rs`
- [x] **JWKS Security Properties**: Add `test_jwks_no_private_key_leakage` ✅ DONE (2026-01-13)
  - Verifies no `d`, `p`, `q`, `dp`, `dq`, `qi` parameters in JWKS response (CWE-321)
  - Location: `crates/env-tests/tests/25_auth_security.rs`

#### Infrastructure Specialist Findings (Priority 1 - Before Resilience)

- [ ] **ClusterConnection Retry Logic**: Add retry with exponential backoff to `ClusterConnection::new()`
  - Currently: Fails immediately if port-forward not ready
  - Need: Retry TCP checks up to 3 times with 2s delays
  - Location: `crates/env-tests/src/cluster.rs:59-100`
- [ ] **Health Check Validation**: Verify services are ready, not just TCP-listening
  - AC may accept connections but have pending migrations
  - Add optional health check parameter to `ClusterConnection::new()`
- [ ] **Connection Error Details**: Include underlying error reason in TCP check failures
  - Currently only reports port number, not why (refused vs timeout vs no route)

#### Code Reviewer Findings (Low Priority)

- [ ] **Extract Common Test Utilities**: Consider moving `cluster()` helper and `Claims` struct to shared module
  - Currently duplicated in 5 test files (idiomatic for integration tests, but creates maintenance burden)
- [ ] **Document Pre-seeded Credentials**: Add comments explaining `test-client` must be seeded by setup script

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

## Guard Infrastructure

### Phase 1: Complete ✅

See "Recently Completed (January 2026)" above.

### Phase 2: Future Enhancements

#### Additional Simple Guards
- [ ] **Complexity guard**: Flag functions over N lines or cyclomatic complexity
- [ ] **Import guard**: Enforce no direct `panic!` usage in production code
- [ ] **Doc guard**: Ensure public APIs have documentation

#### Semantic Guards
- [ ] **Logic leak detection**: Detect business logic that shouldn't be in certain layers
- [ ] **Dependency injection check**: Verify proper DI patterns in handlers

#### Guard Orchestration
- [ ] **PR visibility**: Include guard warnings in PR descriptions
- [ ] **CI integration**: Enforce guard output appears in PRs
- [ ] **Guard result aggregation**: Single summary for review
- [ ] **Blocking vs warning**: Configure which guards block merge

### JWT Principle Test Coverage (Phase 1 Complete)

All major attack vectors now covered:
- [x] Algorithm confusion attack
- [x] `alg: "none"` attack
- [x] Oversized token rejection
- [x] Signature tampering
- [x] Wrong key rejection
- [x] Expiration validation
- [x] Future `iat` validation
- [x] `kid` injection attacks
- [x] Missing required claims
- [x] Clock skew boundary conditions (added in configurable clock skew work)

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
