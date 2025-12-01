# Technical Debt and Future Work

## Phase 4: P1 Security Test Improvements

### Completed Items (2025-11-30)

Based on comprehensive security code review, the following 11 improvements have been completed:

- [x] **iat Validation Documentation**: Documented iat validation policy decision in `test_jwt_future_iat_accepted_by_library` with comprehensive TODO comments outlining options for custom validation vs accepted risk
- [x] **Error Information Leakage Prevention**: Added 2 tests preventing sensitive data exposure (OWASP A05:2021, CWE-209)
- [x] **Key Rotation Tests Planning**: Documented required tests and missing repository methods (`get_by_key_id()`, `list_all_keys()`)
- [x] **Timing Attack Tolerance**: Tightened from 50% to 30% to reduce attack surface
- [x] **UNION SELECT SQL Injection**: Added test with 3 attack vectors including information_schema exploitation
- [x] **Second-Order SQL Injection**: Added test verifying malicious stored data cannot execute in subsequent queries
- [x] **bcrypt Cost Factor Validation**: Added 2 tests verifying cost=12 per ADR-0003 (CWE-916 mitigation)
- [x] **Test Naming Standardization**: Fixed null byte test name and verified consistent naming across all tests
- [x] **Magic Number Extraction**: Extracted 5 constants (token expiry, rate limits, timing thresholds) to improve maintainability
- [x] **Code Quality**: All tests pass (71 tests), zero clippy warnings, properly formatted

**Test Count**: Increased from 65 → 71 tests (+6 new security tests)
**Code Coverage**: Maintained 83% (targeting 95%)

### Future Enhancements

#### JWT Security Enhancements

- [ ] **iat Validation Implementation**: Implement strict iat validation with clock skew tolerance (±5 minutes) based on decision from documented TODO
- [ ] **JWT Header Injection**: Add test for typ claim tampering (e.g., changing "JWT" to "something-else")
- [ ] **Key Rotation Implementation**: Complete key rotation tests once `signing_keys::get_by_key_id()` and `signing_keys::list_all_keys()` are implemented

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
