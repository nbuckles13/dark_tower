# Technical Debt and Future Work

## Phase 4: P1 Security Test Improvements (Future PRs)

Based on comprehensive security code review of P1 tests, the following improvements should be addressed in future PRs:

### JWT Security Enhancements

- [ ] **iat Validation**: Either implement strict iat validation in token verification, or document as accepted limitation with rationale (forward-dated tokens currently accepted)
- [ ] **JWT Header Injection**: Add test for typ claim tampering (e.g., changing "JWT" to "something-else")
- [ ] **Key Rotation Edge Cases**: Test token verification during key rotation window

### SQL Injection Testing Enhancements

- [ ] **Deterministic Oversized Input Test**: Refactor `test_oversized_input_handling` to check against actual schema limits (VARCHAR lengths, etc.) instead of arbitrary 1000-char strings
- [ ] **Second-Order SQL Injection**: Add tests that store malicious input, then retrieve and use it in queries (e.g., store "admin' OR '1'='1" in region, then query by region)
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
