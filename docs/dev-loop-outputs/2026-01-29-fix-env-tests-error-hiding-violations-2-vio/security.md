# Security Review: env-tests Error Hiding Fixes

**Reviewer**: Security Specialist
**Date**: 2026-01-29
**Files Reviewed**: `crates/env-tests/src/cluster.rs`
**Verdict**: APPROVED

## Summary

The changes fix two error hiding violations by preserving the original error context in error messages. This is test infrastructure code (env-tests crate) used for local development and CI testing against kind clusters. The error messages appropriately include the underlying error details without leaking sensitive information.

## Analysis

### Line 124: Address Parsing Error

```rust
&addr.parse().map_err(|e| ClusterError::HealthCheckFailed {
    message: format!("Invalid address '{}': {}", addr, e),
})?,
```

**Security Assessment**: SAFE
- The address being parsed is always `127.0.0.1:{port}` where port is a `u16`
- The address value is hardcoded localhost, no user input injection possible
- Parse errors for `SocketAddr` contain technical details about parsing failures, not sensitive data
- This is test infrastructure, not production code

### Line 129: TCP Connection Error

```rust
.map_err(|e| ClusterError::HealthCheckFailed {
    message: format!(
        "Port-forward not detected on localhost:{}. Run './infra/kind/scripts/setup.sh' to start port-forwards. TCP error: {}",
        port, e
    ),
})?;
```

**Security Assessment**: SAFE
- TCP connection errors to localhost contain only:
  - Connection refused (service not running)
  - Timeout (service unreachable)
  - Other IO errors (permissions, resource limits)
- No credentials, tokens, or sensitive data can leak through `std::io::Error`
- Port number is a `u16` (0-65535), not sensitive
- This is test infrastructure for local development

## Findings

| ID | Severity | Description |
|----|----------|-------------|
| None | - | No security issues found |

## Verdict

**APPROVED**

The error handling changes are appropriate for test infrastructure code:

1. **No sensitive data exposure**: The error messages only include localhost addresses and standard IO error messages
2. **Improved debuggability**: Preserving error context helps developers troubleshoot cluster connectivity issues
3. **Appropriate scope**: This is test utility code, not production code handling user data or credentials
4. **No attack surface**: The code connects only to localhost and doesn't process external input

## Checklist

- [x] Error messages don't leak credentials or secrets
- [x] Error messages don't leak internal system paths (beyond localhost)
- [x] Error context is appropriate for the code's purpose (test infrastructure)
- [x] No new attack vectors introduced
