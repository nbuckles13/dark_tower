# Test Review

**Reviewer**: Test Specialist
**Date**: 2026-02-02
**Verdict**: REQUEST_CHANGES

## Summary

The TokenManager integration introduces new error types (`TokenAcquisition`, `TokenAcquisitionTimeout`) and OAuth configuration fields without corresponding test coverage. While the integration tests demonstrate mock token receiver usage via `from_test_channel()`, unit tests for error mapping and config validation are missing. The test-utils feature approach is sound.

## Findings

### BLOCKER
None

### CRITICAL
1. **Missing tests for `TokenAcquisition` and `TokenAcquisitionTimeout` error variants in `errors.rs`**
   - Location: `crates/meeting-controller/src/errors.rs` lines 85-89
   - Issue: The `test_error_code_mapping()` test (lines 180-238) does not test `TokenAcquisition` and `TokenAcquisitionTimeout` variants for error code mapping (should return 6 for INTERNAL_ERROR)
   - Impact: Cannot verify these new error types correctly map to signaling codes

2. **Missing tests for `client_message()` hiding internal details for token errors**
   - Location: `crates/meeting-controller/src/errors.rs` lines 152-153
   - Issue: The `test_client_messages_hide_internal_details()` test (lines 241-257) does not verify that `TokenAcquisition` and `TokenAcquisitionTimeout` hide sensitive details (should return "An internal error occurred")
   - Impact: Token acquisition error messages could leak internal details to clients

### MAJOR
1. **No test for display formatting of token error variants**
   - Location: `crates/meeting-controller/src/errors.rs` lines 84-89
   - Issue: The `test_display_formatting()` test (lines 269-292) does not test the Display impl for `TokenAcquisition` and `TokenAcquisitionTimeout`
   - Impact: Error message formatting is untested

### MINOR
1. **Integration tests use `std::mem::forget(tx)` to keep sender alive**
   - Location: `crates/meeting-controller/tests/gc_integration.rs` line 267, `crates/meeting-controller/src/grpc/gc_client.rs` line 648
   - Issue: Using `mem::forget` is technically a memory leak. While acceptable in tests, a cleaner pattern would be to store the sender in a test context struct or use a static.
   - Impact: Minor test hygiene concern

## Observations

### Positive Aspects

1. **Test-utils feature approach is sound**: The `#[cfg(any(test, feature = "test-utils"))]` gating on `TokenReceiver::from_test_channel()` is a clean pattern that enables testing without exposing internals in production builds. The dev-dependency in `meeting-controller/Cargo.toml` correctly enables this feature.

2. **Integration tests properly updated**: The `gc_integration.rs` tests correctly use the mock `TokenReceiver` pattern:
   - `mock_token_receiver()` helper function (lines 264-269)
   - `test_config()` updated with OAuth fields (lines 241-261)
   - All `GcClient::new()` calls updated to use token receiver

3. **Config tests comprehensive for OAuth fields**: The config tests (lines 300-456) properly cover:
   - Missing `AC_ENDPOINT` (test_from_vars_missing_ac_endpoint)
   - Missing `MC_CLIENT_ID` (test_from_vars_missing_client_id)
   - Missing `MC_CLIENT_SECRET` (test_from_vars_missing_client_secret)
   - OAuth config loaded correctly (test_oauth_config_loaded_correctly)
   - Debug redacts sensitive fields (test_debug_redacts_sensitive_fields)

4. **TokenManager has extensive tests in common crate**: The `common/src/token_manager.rs` tests (lines 577-1225) comprehensively cover:
   - Token acquisition success/failure
   - Retry with exponential backoff
   - Token refresh on expiry
   - HTTPS enforcement (`test_new_secure_requires_https`)
   - HTTP timeout handling
   - Channel closed error handling

### Missing Coverage Analysis

The following scenarios in `main.rs` are only tested via the 7-layer verification (cargo check, clippy) but lack unit/integration tests:

1. **TokenManagerConfig::new_secure() failure path** (line 108-116): The HTTP endpoint rejection is tested in common crate, but MC-specific error mapping is not tested.

2. **Token acquisition timeout** (lines 119-127): No test verifies the 30-second timeout behavior.

3. **Initial token acquisition failure** (lines 128-131): No test verifies the error propagation path.

Note: These are acceptable as they occur in `main.rs` which is binary-only code and difficult to unit test. The underlying TokenManager behavior is well-tested in common crate.

### Test Coverage Estimate

| Component | Coverage | Notes |
|-----------|----------|-------|
| Config OAuth fields | Good | All env var validations tested |
| GcClient TokenReceiver | Good | Integration tests use mock |
| New error variants | **Gap** | Missing unit tests |
| TokenManager (common) | Excellent | Extensive test suite |
| Main startup flow | Acceptable | Binary code, difficult to test |

## Required Actions

To achieve APPROVED status:

1. Add to `crates/meeting-controller/src/errors.rs` `test_error_code_mapping()`:
   ```rust
   // Token acquisition errors -> 6 (INTERNAL_ERROR)
   assert_eq!(McError::TokenAcquisition("failed".to_string()).error_code(), 6);
   assert_eq!(McError::TokenAcquisitionTimeout.error_code(), 6);
   ```

2. Add to `crates/meeting-controller/src/errors.rs` `test_client_messages_hide_internal_details()`:
   ```rust
   // Token errors should hide details
   let token_err = McError::TokenAcquisition("AC connection refused at 192.168.1.1".to_string());
   assert!(!token_err.client_message().contains("192.168"));
   assert_eq!(token_err.client_message(), "An internal error occurred");

   let timeout_err = McError::TokenAcquisitionTimeout;
   assert_eq!(timeout_err.client_message(), "An internal error occurred");
   ```

3. Add to `crates/meeting-controller/src/errors.rs` `test_display_formatting()`:
   ```rust
   assert_eq!(
       format!("{}", McError::TokenAcquisition("AC unreachable".to_string())),
       "Token acquisition failed: AC unreachable"
   );
   assert_eq!(
       format!("{}", McError::TokenAcquisitionTimeout),
       "Token acquisition timed out"
   );
   ```
