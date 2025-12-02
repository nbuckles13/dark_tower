# Technical Debt and Future Work

## Phase 4: P1 Security Test Improvements

### Completed Items (2025-12-01)

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
- [ ] **Maximum Token Age Validation** (Security Specialist Recommendation): Add validation to reject tokens with `iat` too far in the PAST (e.g., >15 minutes old) to prevent replay attacks with old tokens. Currently only future `iat` is validated.
- [ ] **JWT Size Limits** (Security Specialist Recommendation): Add size limit check in `verify_jwt()` to prevent DoS via large tokens (suggest 4KB limit)
- [ ] **Key Rotation Implementation**: Complete key rotation tests once `signing_keys::get_by_key_id()` and `signing_keys::list_all_keys()` are implemented. **SECURITY NOTE**: When implementing, must validate `kid` against database whitelist only - never fetch keys based on untrusted kid values.
- [ ] **Configurable Clock Skew**: Consider making `JWT_CLOCK_SKEW_SECONDS` configurable via environment variable for different security postures

#### SQL Injection Testing Enhancements

- [ ] **Deterministic Oversized Input Test**: Refactor `test_oversized_input_handling` to check against actual schema limits (VARCHAR lengths, etc.) instead of arbitrary 1000-char strings
- [ ] **Time-Based SQL Injection**: Add tests that verify timing-based blind SQL injection is prevented (e.g., SLEEP/pg_sleep injection attempts)
- [ ] **Stored Procedure Injection**: If using stored procedures in future, add tests for SQL injection via procedure parameters

### Test Infrastructure

- [ ] **Code Coverage Target**: Improve P1 test coverage from current 83% to target 95%
- [ ] **Performance Benchmarks**: Add criterion benchmarks for token validation performance under security attacks

### Documentation

- [ ] **Security Testing Guide**: Document security testing patterns and attack vectors in developer documentation
- [ ] **Threat Model**: Create formal threat model documentation for authentication controller

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
